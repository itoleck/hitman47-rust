//! hm1vk — DirectDraw 7 / Direct3D 7 implemented on Vulkan.
//!
//! Built as `ddraw.dll` and pre-loaded by HitMan-Rust.exe from the `HitManVK`
//! sub-folder, so that Hitman: Codename 47's RenderD3D.dll, which imports
//! `DirectDrawCreateEx` and `DirectDrawEnumerateExA` from DDRAW.dll, renders
//! through Vulkan. Every Direct3D 7 call the engine makes is translated:
//!
//! * IDirectDraw7: display modes (up to 3840x2160, 16/32-bit), cooperative
//!   level, surfaces, palettes, clippers, gamma.
//! * IDirectDrawSurface7: system-memory copies + Vulkan images, Lock/Unlock,
//!   Blt/BltFast (fills, stretch, colour key), Flip -> vkQueuePresentKHR.
//! * IDirect3D7 / IDirect3DDevice7: caps, texture formats (incl. DXT1-5 as BC1-3),
//!   render and texture-stage state, transforms, lights, materials, all
//!   DrawPrimitive variants, emulated with a fixed-function uber shader.
//! * IDirect3DVertexBuffer7.

#![allow(clippy::missing_safety_doc, clippy::too_many_arguments)]

#[cfg(not(all(target_arch = "x86", target_os = "windows")))]
compile_error!("hm1vk must be built for i686-pc-windows-msvc (Hitman is a 32-bit game): use tools\\build.bat or `cargo build --release --target i686-pc-windows-msvc`");

pub mod config;
pub mod d3d;
pub mod ddraw;
pub mod device;
pub mod display;
pub mod engine;
pub mod ff;
pub mod fov;
pub mod input;
pub mod log;
pub mod pixel;
pub mod state;
pub mod surface;
pub mod types;
pub mod ui;
pub mod vb;
pub mod vk;

use std::ffi::c_void;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;

fn module_dir(h: HMODULE) -> PathBuf {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let p = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        DLL_PROCESS_ATTACH => {
            let dir = module_dir(module);
            let cfg = config::Config::load(&dir);
            log::init(&dir, cfg.log_level);
            info!("hm1vk {} loaded from {}", env!("HITMAN_RUST_VERSION"), dir.display());
            info!("config: {:?}", cfg);
            state::init(dir, cfg);
        }
        DLL_PROCESS_DETACH => {
            // Do not touch Vulkan under the loader lock; just put the
            // desktop back the way we found it.
            let w = state::world();
            if w.display_changed {
                display::restore_display_mode();
                w.display_changed = false;
            }
            info!("hm1vk unloaded");
        }
        _ => {}
    }
    1
}

// Extra DDRAW.dll exports some components probe for.

#[no_mangle]
pub unsafe extern "system" fn DirectDrawEnumerateExW(_cb: *mut c_void, _ctx: *mut c_void, _flags: u32) -> i32 {
    types::DDERR_UNSUPPORTED
}

#[no_mangle]
pub unsafe extern "system" fn DirectDrawEnumerateW(_cb: *mut c_void, _ctx: *mut c_void) -> i32 {
    types::DDERR_UNSUPPORTED
}

#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> i32 {
    types::S_FALSE
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(_clsid: *const c_void, _iid: *const c_void, out: *mut *mut c_void) -> i32 {
    if !out.is_null() {
        *out = std::ptr::null_mut();
    }
    0x80040111u32 as i32 // CLASS_E_CLASSNOTAVAILABLE
}
