//! UI zoom (`UIZoom` in hm1vk.ini: 1x/2x/3x or 100%/200%/300%).
//!
//! The engine draws all of its 2D interface - front-end menus, the laptop
//! briefing, the in-game HUD, subtitles, its own mouse cursor - as flat
//! geometry in front of a dedicated 45-degree perspective camera with an
//! identity view matrix, positioned in absolute screen pixels. At 3840x2160
//! that makes the 640x480 menus and the HUD tiny. Every draw made with that
//! camera is scaled in screen space by the configured factor:
//!
//! * Menu screens (frames in which the game draws its mouse cursor: options
//!   screens, laptop briefing) are scaled about the screen centre, limited so
//!   the menu panel still fits the screen.
//!   The game moves its own cursor by the mouse offset from the screen centre
//!   each frame (and puts the system cursor back to the centre), and draws it
//!   with the UI camera, so it is zoomed together with the menu and clicks
//!   land on what it points at. Mouse messages are scaled by 1/zoom so the
//!   cursor moves 1:1 with the mouse on the zoomed picture, and movement past
//!   the edge of the visible area is blocked so it cannot get lost off-screen.
//! * During gameplay each UI element is scaled towards the edge or corner it
//!   sits at (top-left health display, top-right weapon panel, subtitles,
//!   centred messages), so the HUD stays anchored.
//!
//! The UI camera is also used for some 3D scenes, e.g. Hitman's body on the
//! death screen. Only flat draws (all vertices at one depth) count as
//! interface elements; while a frame contains a large 3D model drawn with the
//! UI camera, nothing is scaled outside menus.
//!
//! The scale is applied by scaling the draw's viewport rectangle about the
//! anchor point, so nothing about the engine's own layout changes and UI
//! drawn into a sub-viewport (e.g. the laptop's scrolling text box) is
//! clipped to the scaled box.

use crate::types::D3DMATRIX;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{CallWindowProcA, GetClientRect, SetWindowLongA, GWL_WNDPROC, WNDPROC};

/// Parses `1x`, `2.5x`, `200%`, `2` ... into a factor in [1, 3].
pub fn parse_scale(v: &str) -> Option<f32> {
    let t = v.trim().to_ascii_lowercase();
    let (num, percent) = if let Some(n) = t.strip_suffix('%') {
        (n.trim().to_string(), true)
    } else if let Some(n) = t.strip_suffix('x') {
        (n.trim().to_string(), false)
    } else {
        (t.clone(), false)
    };
    let x: f32 = num.parse().ok()?;
    let s = if percent || x >= 10.0 { x / 100.0 } else { x };
    s.is_finite().then(|| s.clamp(1.0, 3.0))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// Gameplay (or unknown): HUD elements scaled towards their edges.
    Game,
    /// A menu screen: everything scaled by `scale` about the screen centre.
    Menu { scale: f32 },
    /// A 3D scene drawn with the UI camera (death screen): nothing scaled.
    Scene,
}

struct Frame {
    world_draws: u32,
    ui_draws: u32,
    /// Large 3D models drawn with the UI camera.
    ui_meshes: u32,
    /// Union of the UI draws' screen bounds (ignoring small quads such as the cursor).
    ui_box: Option<[f32; 4]>,
}

static mut MODE: Mode = Mode::Game;
static mut FRAME: Frame = Frame { world_draws: 0, ui_draws: 0, ui_meshes: 0, ui_box: None };
/// Backbuffer size and scale for mapping mouse positions (menu mode only).
static mut MOUSE_MAP: Option<(f32, f32, f32)> = None; // (scale, width, height)
/// Position of the game's menu cursor (unscaled pixels) in the last frame.
static mut CURSOR: Option<(f32, f32)> = None;
static mut CURSOR_FRAME: Option<(f32, f32)> = None;
static mut ORIG_WNDPROC: WNDPROC = None;
static mut HOOKED_HWND: isize = 0;

fn zoom() -> f32 {
    crate::state::config().ui_zoom
}

/// True for draws made with the engine's UI camera: 45-degree perspective
/// projection and identity view matrix.
pub fn is_ui_camera(proj: &D3DMATRIX, view: &D3DMATRIX) -> bool {
    let p = &proj.m;
    let persp = (p[2][3] - 1.0).abs() < 1e-4 && p[3][3].abs() < 1e-6;
    // x scale of a 45-degree horizontal FOV: 1 / tan(22.5) = 2.41421.
    if !persp || (p[0][0] - 2.414_213_6).abs() > 1e-3 {
        return false;
    }
    let id = D3DMATRIX::identity();
    view.m.iter().flatten().zip(id.m.iter().flatten()).all(|(a, b)| (a - b).abs() < 1e-4)
}

/// Records a draw to the back buffer (screen bounds in pixels). `cursor`:
/// the draw is the game's mouse cursor sprite; `big_mesh`: a large 3D model
/// drawn with the UI camera.
pub fn note_draw(ui: bool, b: [f32; 4], cursor: bool, big_mesh: bool) {
    unsafe {
        let f = &mut *std::ptr::addr_of_mut!(FRAME);
        if !ui {
            f.world_draws += 1;
            return;
        }
        f.ui_draws += 1;
        if big_mesh {
            f.ui_meshes += 1;
        }
        if cursor {
            CURSOR_FRAME = Some((b[0], b[1]));
            return;
        }
        if b[2] - b[0] >= 96.0 || b[3] - b[1] >= 96.0 {
            f.ui_box = Some(match f.ui_box {
                Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
                None => b,
            });
        }
    }
}

/// Scaled viewport (x, y, w, h; may extend past the render target) for a UI
/// draw with pixel bounds `b`, drawn into viewport `vp` on a `rt` sized
/// render target. None = draw unchanged.
/// `flat`: the draw is a flat interface element (3D models drawn with the UI
/// camera are only scaled together with a whole menu screen).
pub fn scaled_viewport(b: [f32; 4], vp: (f32, f32, f32, f32), rt: (f32, f32), flat: bool) -> Option<[f32; 4]> {
    let (vx, vy, vw, vh) = vp;
    let (rw, rh) = rt;
    if vw <= 0.0 || vh <= 0.0 || rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let (s, ax, ay) = match unsafe { MODE } {
        Mode::Menu { scale } => (scale, rw / 2.0, rh / 2.0),
        Mode::Scene => return None,
        Mode::Game if !flat => return None,
        Mode::Game => {
            // Anchor each element to the screen edge/corner of the third it sits in.
            let (cx, cy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            let ax = if cx < rw / 3.0 {
                0.0
            } else if cx > rw * 2.0 / 3.0 {
                rw
            } else {
                rw / 2.0
            };
            let ay = if cy < rh / 3.0 {
                0.0
            } else if cy > rh * 2.0 / 3.0 {
                rh
            } else {
                rh / 2.0
            };
            (zoom(), ax, ay)
        }
    };
    if (s - 1.0).abs() < 1e-3 {
        return None;
    }
    Some([ax + (vx - ax) * s, ay + (vy - ay) * s, vw * s, vh * s])
}

/// Called once per presented frame: decides menu vs gameplay for the next frame.
pub fn end_frame(width: u32, height: u32) {
    unsafe {
        let f = &mut *std::ptr::addr_of_mut!(FRAME);
        let s = zoom();
        let prev = MODE;
        let cursor_frame = CURSOR_FRAME;
        if cursor_frame.is_none() {
            // No menu cursor: gameplay, cutscenes, loading screens, or a 3D
            // scene drawn with the UI camera (death screen).
            if f.ui_meshes > 0 {
                MODE = Mode::Scene;
            } else if f.world_draws > 0 || f.ui_draws > 0 {
                MODE = Mode::Game;
            }
        } else {
            // Largest scale (up to the setting) that keeps the menu panel on
            // screen. Panels are snapped to the engine's standard sizes so the
            // scale does not change when the content changes.
            let (w, h) = (width as f32, height as f32);
            let (cx, cy) = (w / 2.0, h / 2.0);
            let (mut hx, mut hy) = match f.ui_box {
                Some(b) => ((cx - b[0]).max(b[2] - cx), (cy - b[1]).max(b[3] - cy)),
                None => (320.0, 240.0),
            };
            if let Some((pw, ph)) = [(320.0, 240.0), (400.0, 300.0), (512.0, 384.0)].into_iter().find(|(pw, ph)| hx <= pw + 2.0 && hy <= ph + 2.0) {
                hx = pw;
                hy = ph;
            }
            let fit = (cx / hx.max(1.0)).min(cy / hy.max(1.0));
            MODE = Mode::Menu { scale: s.min(fit).max(1.0) };
        }
        // (An empty frame - fades, videos - keeps the current mode.)
        let mode = MODE;
        if mode != prev {
            crate::debug!("UI zoom mode: {:?}", mode);
        }
        let cur = CURSOR_FRAME;
        CURSOR_FRAME = None;
        if cur.is_some() && cur != CURSOR {
            crate::debug!("UI cursor at {:.0},{:.0}", cur.unwrap().0, cur.unwrap().1);
        }
        CURSOR = cur;
        MOUSE_MAP = match MODE {
            Mode::Menu { scale } if scale > 1.001 => Some((scale, width as f32, height as f32)),
            _ => None,
        };
        *f = Frame { world_draws: 0, ui_draws: 0, ui_meshes: 0, ui_box: None };
    }
}

// ------------------------------------------------------------ mouse

/// Subclasses the game window so mouse positions can be mapped while a
/// zoomed menu is shown.
pub unsafe fn hook_window(hwnd: isize) {
    if hwnd == 0 || hwnd == HOOKED_HWND || zoom() <= 1.001 {
        return;
    }
    let old = SetWindowLongA(hwnd as HWND, GWL_WNDPROC, wndproc as *const () as usize as i32);
    if old != 0 {
        ORIG_WNDPROC = std::mem::transmute::<isize, WNDPROC>(old as isize);
        HOOKED_HWND = hwnd;
        crate::info!("UI zoom: {}x (mouse mapping on window {hwnd:#x})", zoom());
    }
}

/// Maps a client-area mouse position from the zoomed picture to the
/// unscaled coordinates the game hit-tests with.
unsafe fn map_lparam(hwnd: HWND, lp: LPARAM) -> LPARAM {
    let Some((s, bw, bh)) = MOUSE_MAP else { return lp };
    let x = (lp & 0xFFFF) as u16 as i16 as f32;
    let y = ((lp >> 16) & 0xFFFF) as u16 as i16 as f32;
    let mut rc: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rc);
    let (cw, ch) = ((rc.right - rc.left) as f32, (rc.bottom - rc.top) as f32);
    if cw <= 0.0 || ch <= 0.0 {
        return lp;
    }
    // Client -> back buffer pixels, scale the offset from the centre by
    // 1/zoom, and back.
    let (cx, cy) = (bw / 2.0, bh / 2.0);
    let (bx, by) = (x * bw / cw, y * bh / ch);
    let (mut dx, mut dy) = ((bx - cx) / s, (by - cy) / s);
    // Keep the game's cursor inside the part of the screen that is visible:
    // limit each movement to the distance left to the edge (the game moves
    // its cursor by about 1.3x the mouse offset at its default sensitivity),
    // and move it back in if it is outside.
    if let Some((qx, qy)) = CURSOR {
        const GAIN: f32 = 1.3;
        let (hw, hh) = (cx / s, cy / s);
        let (x0, x1) = (cx - hw, cx + hw - 8.0);
        let (y0, y1) = (cy - hh, cy + hh - 8.0);
        dx = dx.clamp((x0 - qx) / GAIN, ((x1 - qx) / GAIN).max((x0 - qx) / GAIN));
        dy = dy.clamp((y0 - qy) / GAIN, ((y1 - qy) / GAIN).max((y0 - qy) / GAIN));
    }
    let (ux, uy) = (cx + dx, cy + dy);
    let (nx, ny) = ((ux * cw / bw).round() as i32, (uy * ch / bh).round() as i32);
    ((nx as u16 as u32) | ((ny as u16 as u32) << 16)) as LPARAM
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // WM_MOUSEMOVE .. WM_MBUTTONDBLCLK carry client coordinates.
    let lp = if (0x0200..=0x0209).contains(&msg) { map_lparam(hwnd, lp) } else { lp };
    CallWindowProcA(ORIG_WNDPROC, hwnd, msg, wp, lp)
}
