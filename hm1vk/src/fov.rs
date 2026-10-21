//! Field of view (`FOV` in hm1vk.ini).
//!
//! The gameplay camera's FOV is the `CameraFOV` property of the game's camera
//! controller (HitmanDlc.dlc, `double CameraFOV=67.4;`, also stored in every
//! scene). It is a *horizontal* FOV in degrees; the vertical FOV follows from
//! the aspect ratio (41.1 degrees at 16:9). Cutscenes, the sniper scope and
//! binoculars, and the HUD's 3D overlay use other cameras and FOVs.
//!
//! Scaling the projection matrix in the renderer is not enough, because the
//! engine culls objects against its own camera: a wider picture would show
//! holes at the edges. So the FOV is changed in the engine's camera object
//! instead. RenderD3D.dll sets up every camera through one method of its
//! render object (vtable slot 0x30, `SetupCamera(this, camera, flag)`), which
//! reads the camera's FOV as a double in radians at `camera + 0x16E`.
//! hm1vk hooks that vtable slot. When the camera holds the stock 67.4 degrees
//! and another FOV is configured, the field is set to the configured FOV
//! before RenderD3D reads it, so the projection and the engine's culling both
//! use it. Cameras with any other FOV are not touched.

use std::ffi::c_void;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

use crate::types::D3DMATRIX;

/// The engine's gameplay camera FOV (horizontal, degrees).
pub const DEFAULT_FOV: f32 = 67.4;
/// Allowed range: 10 degrees narrower to 20 degrees wider than the default.
pub const MIN_FOV: f32 = DEFAULT_FOV - 10.0;
pub const MAX_FOV: f32 = DEFAULT_FOV + 20.0;

/// Clamps a configured FOV to [MIN_FOV, MAX_FOV].
pub fn clamp(fov: f32) -> f32 {
    if fov.is_finite() {
        fov.clamp(MIN_FOV, MAX_FOV)
    } else {
        DEFAULT_FOV
    }
}

// --- RenderD3D.dll (Steam build) layout -------------------------------------

/// RVA of the render object's camera-setup method.
const SETUP_CAMERA_RVA: usize = 0x26410;
/// Its first bytes (push ebp; mov ebp,esp; and esp,-8; push -1; push imm32).
const SETUP_CAMERA_PROLOGUE: [u8; 9] = [0x55, 0x8B, 0xEC, 0x83, 0xE4, 0xF8, 0x6A, 0xFF, 0x68];
/// RVAs of the two render-object vtables; the method is in slot 0x30 of both.
const VTABLE_RVAS: [usize; 2] = [0x351F8, 0x35D70];
const VTABLE_SLOT: usize = 0x30;
/// Offset of the FOV (double, radians) in the engine's camera object.
const CAMERA_FOV_OFFSET: usize = 0x16E;

type SetupCamera = unsafe extern "thiscall" fn(this: *mut c_void, camera: *mut u8, flag: u32);

static mut ORIGINAL: Option<SetupCamera> = None;
static mut TARGET_RAD: f64 = 0.0;
static mut INSTALLED: bool = false;

unsafe extern "thiscall" fn setup_camera_hook(this: *mut c_void, camera: *mut u8, flag: u32) {
    if !camera.is_null() {
        let p = camera.add(CAMERA_FOV_OFFSET) as *mut f64;
        let fov = p.read_unaligned();
        if (fov - (DEFAULT_FOV as f64).to_radians()).abs() < 1e-4 {
            p.write_unaligned(TARGET_RAD);
            crate::log::once(
                "fov-applied",
                format_args!("FOV: gameplay camera {:.1} -> {:.1} degrees (horizontal)", DEFAULT_FOV, TARGET_RAD.to_degrees()),
            );
        }
    }
    if let Some(f) = ORIGINAL {
        f(this, camera, flag);
    }
}

/// Hooks RenderD3D's camera setup when a non-default FOV is configured.
/// Called at RenderD3D's first DirectDraw call (it is loaded by then).
pub unsafe fn install() {
    if INSTALLED {
        return;
    }
    INSTALLED = true;
    let fov = crate::state::config().fov;
    if (fov - DEFAULT_FOV).abs() < 0.01 {
        return;
    }
    let base = GetModuleHandleA(b"RenderD3D.dll\0".as_ptr()) as usize;
    if base == 0 {
        crate::info!("FOV: RenderD3D.dll not loaded; FOV setting ignored");
        return;
    }
    let func = base + SETUP_CAMERA_RVA;
    let prologue = std::slice::from_raw_parts(func as *const u8, SETUP_CAMERA_PROLOGUE.len());
    let slots: Vec<*mut usize> = VTABLE_RVAS.iter().map(|rva| (base + rva + VTABLE_SLOT) as *mut usize).collect();
    if prologue != SETUP_CAMERA_PROLOGUE || slots.iter().any(|s| s.read() != func) {
        crate::info!("FOV: unknown RenderD3D.dll build; FOV setting ignored");
        return;
    }
    TARGET_RAD = (fov as f64).to_radians();
    ORIGINAL = Some(std::mem::transmute::<usize, SetupCamera>(func));
    for slot in slots {
        let mut old = 0u32;
        if VirtualProtect(slot as *const c_void, 4, PAGE_READWRITE, &mut old) != 0 {
            slot.write(setup_camera_hook as *const () as usize);
            VirtualProtect(slot as *const c_void, 4, old, &mut old);
        }
    }
    crate::info!("FOV: {fov:.1} degrees (game default {DEFAULT_FOV})");
}

fn horizontal_fov(m: &D3DMATRIX) -> f32 {
    2.0 * (1.0 / m.m[0][0]).atan().to_degrees()
}

/// Debug log (LogLevel >= 1) of every distinct perspective projection.
pub fn on_projection(m: &D3DMATRIX, vp_h: u32) {
    if crate::state::config().log_level >= 1 && (m.m[2][3] - 1.0).abs() < 1e-4 && m.m[0][0] > 0.0 {
        let h = horizontal_fov(m);
        crate::log::once(&format!("proj {h:.1} {vp_h}"), format_args!("projection: horizontal FOV {h:.2}, viewport height {vp_h}"));
    }
}
