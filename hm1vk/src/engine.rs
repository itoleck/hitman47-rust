//! Fix-ups for limitations of the original Hitman: Codename 47 engine.
//!
//! RenderD3D.dll snaps the fullscreen resolution to a fixed 4:3 table
//! (512x384, 640x480, 800x600, 1024x768, 1280x1024, 1600x1200) before it
//! enumerates display modes. Wider resolutions (1920x1080, 2560x1440,
//! 3840x2160, ...) end up with an undefined height and the engine fails
//! with "Unable to find a suitable display mode". Its 16:9 modes such as
//! 1280x720 silently become 1280x1024.
//!
//! The snapped values live in the engine's `ZSysInterface` object (exported
//! by Globals.dll as `g_pSysInterface`). Before RenderD3D looks at them (its
//! first DirectDraw call is DirectDrawEnumerateExA), we restore the
//! resolution the player asked for in the .ini file:
//!
//! | offset | field                                    |
//! |--------|------------------------------------------|
//! | 0x12   | u8  fullscreen                           |
//! | 0x19   | i32 width   (current)                    |
//! | 0x1D   | i32 height  (current)                    |
//! | 0x21   | i32 width   (requested)                  |
//! | 0x25   | i32 height  (requested)                  |
//! | 0x29   | i32 colour depth                         |
//! | 0xA95  | f32 width / 2 (screen centre, mouse)     |
//! | 0xA99  | f32 height / 2                           |

use std::path::PathBuf;

use crate::info;

unsafe fn sys_interface() -> *mut u8 {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    let g = GetModuleHandleA(b"Globals.dll\0".as_ptr());
    if g.is_null() {
        return std::ptr::null_mut();
    }
    match GetProcAddress(g, b"?g_pSysInterface@@3PAVZSysInterface@@A\0".as_ptr()) {
        Some(p) => *(p as *const *mut u8),
        None => std::ptr::null_mut(),
    }
}

unsafe fn rd(p: *mut u8, off: usize) -> i32 {
    (p.add(off) as *const i32).read_unaligned()
}
unsafe fn wr(p: *mut u8, off: usize, v: i32) {
    (p.add(off) as *mut i32).write_unaligned(v)
}
unsafe fn wrf(p: *mut u8, off: usize, v: f32) {
    (p.add(off) as *mut f32).write_unaligned(v)
}
unsafe fn rdf(p: *mut u8, off: usize) -> f32 {
    (p.add(off) as *const f32).read_unaligned()
}

/// The .ini the launcher handed to the engine (HITMAN_RUST_INI), or the
/// stock Hitman.ini.
fn ini_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HITMAN_RUST_INI") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    for name in [exe.with_extension("ini"), exe.with_file_name("Hitman.ini")] {
        if name.is_file() {
            return Some(name);
        }
    }
    None
}

/// Parses the last "Resolution WxH" line (the engine's own format).
pub fn requested_resolution() -> Option<(i32, i32)> {
    let text = std::fs::read(ini_path()?).ok()?;
    let text = String::from_utf8_lossy(&text);
    let mut res = None;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("//") || l.starts_with(';') {
            continue;
        }
        let mut it = l.splitn(2, char::is_whitespace);
        if !it.next().unwrap_or("").eq_ignore_ascii_case("Resolution") {
            continue;
        }
        let v = it.next().unwrap_or("").trim();
        if let Some((w, h)) = v.split_once(['x', 'X', '*']) {
            if let (Ok(w), Ok(h)) = (w.trim().parse::<i32>(), h.trim().parse::<i32>()) {
                if (320..=16384).contains(&w) && (200..=16384).contains(&h) {
                    res = Some((w, h));
                }
            }
        }
    }
    res
}

static mut DONE: bool = false;

/// Restores the requested fullscreen resolution in the engine's settings.
pub unsafe fn fix_resolution() {
    if DONE || !crate::state::config().fix_resolution {
        return;
    }
    DONE = true;
    let sys = sys_interface();
    if sys.is_null() {
        return;
    }
    let fullscreen = *sys.add(0x12) != 0;
    let (w, h) = (rd(sys, 0x19), rd(sys, 0x1D));
    info!(
        "engine settings: fullscreen={} current={}x{} requested={}x{} depth={} centre=({}, {})",
        fullscreen,
        w,
        h,
        rd(sys, 0x21),
        rd(sys, 0x25),
        rd(sys, 0x29),
        rdf(sys, 0xA95),
        rdf(sys, 0xA99)
    );
    if !fullscreen {
        return;
    }
    let Some((rw, rh)) = requested_resolution() else { return };
    if (rw, rh) == (w, h) {
        return;
    }
    let cfg = crate::state::config();
    if rw as u32 > cfg.max_width || rh as u32 > cfg.max_height {
        info!("requested {rw}x{rh} exceeds MaxWidth/MaxHeight; leaving engine setting alone");
        return;
    }
    wr(sys, 0x19, rw);
    wr(sys, 0x1D, rh);
    wr(sys, 0x21, rw);
    wr(sys, 0x25, rh);
    wrf(sys, 0xA95, (rw >> 1) as f32);
    wrf(sys, 0xA99, (rh >> 1) as f32);
    info!("engine resolution {w}x{h} -> {rw}x{rh} (RenderD3D's 4:3 table bypassed)");
}
