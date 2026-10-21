//! HitMan-Rust.exe — a Rust re-implementation of `Hitman.Exe` from
//! *Hitman: Codename 47* (IO Interactive, 2000).
//!
//! The original executable is a ~45 KB PECompact2-packed stub. Unpacked
//! (see `re/NOTES.md`), its `WinMain` does exactly this:
//!
//! 1. `CreateMutexA(NULL, FALSE, "Hitboy")`; if the mutex already exists the
//!    game is running, so it returns 0.
//! 2. A (now neutered) copy-protection check that always succeeds.
//! 3. Installs CRT `signal()` handlers for SIGFPE, SIGABRT, SIGILL and SIGSEGV.
//!    The handler asks the engine to shut its render and sound subsystems down
//!    so the desktop display mode is restored after a crash.
//! 4. `LoadLibraryA("System.dll")`. Its DllMain creates the engine's
//!    `ZSysInterface` singleton and publishes it via `Globals.dll`'s exported
//!    `g_pSysInterface`.
//! 5. Builds the configuration argument. If `lpCmdLine` is at least 2 characters
//!    it is passed through unchanged. Otherwise the exe's own path with the
//!    extension replaced by `.ini` is used as `"@<path>"` when that file exists,
//!    and `"@main.ini"` when it does not.
//! 6. Calls `g_pSysInterface->vtbl[0xC8 / 4](arg)`, which runs the engine until
//!    the player quits.
//! 7. `FreeLibrary(System.dll)` and returns 0.
//!
//! This launcher reproduces all of that, and adds:
//!
//! * Per-monitor DPI awareness, so a 3840x2160 mode is real pixels on 4K
//!   displays instead of being virtualised by Windows.
//! * Pre-loading of `HitManVK\ddraw.dll` (the `hm1vk` crate) before the engine
//!   loads `RenderD3D.dll`. RenderD3D's `DDRAW.dll` import then binds to the
//!   Vulkan implementation of DirectDraw 7 / Direct3D 7 instead of the system
//!   one. Pass `-d3d` to skip this and use the system Direct3D.
//! * When `HitMan-Rust.ini` does not exist, falls back to the stock `Hitman.ini`
//!   before `@main.ini`, so the launcher works in an unmodified install.

#![windows_subsystem = "windows"]

#[cfg(not(all(target_arch = "x86", target_os = "windows")))]
compile_error!("HitMan-Rust must be built for i686-pc-windows-msvc (the game's DLLs are 32-bit): use tools\\build.bat or `cargo build --release --target i686-pc-windows-msvc`");

use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{FreeLibrary, GetLastError, ERROR_ALREADY_EXISTS, HMODULE};
use windows_sys::Win32::System::Environment::GetCommandLineA;
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, LoadLibraryA, LoadLibraryExW,
    LOAD_WITH_ALTERED_SEARCH_PATH,
};
use windows_sys::Win32::System::Threading::CreateMutexA;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_ICONWARNING, MB_OK};

/// Mangled name of `ZSysInterface* g_pSysInterface` exported by Globals.dll.
const G_PSYSINTERFACE: &[u8] = b"?g_pSysInterface@@3PAVZSysInterface@@A\0";
/// Byte offset of the engine's "run" method in the ZSysInterface vtable.
const VTBL_RUN: usize = 0xC8;
/// Byte offsets inside ZSysInterface of the two subsystem pointers the crash
/// handler shuts down (render device and sound device). The original code
/// reads them unaligned, so we do too.
const SUBSYSTEM_OFFSETS: [usize; 2] = [0x38F5, 0x38F9];
/// vtable slot (byte offset 0x28) of the subsystems' shutdown method.
const VTBL_SHUTDOWN: usize = 0x28 / 4;

// CRT signal numbers (MSVC).
const SIGILL: i32 = 4;
const SIGFPE: i32 = 8;
const SIGSEGV: i32 = 11;
const SIGABRT: i32 = 22;

extern "C" {
    fn signal(sig: i32, func: usize) -> usize;
}

/// Address of Globals.dll's `g_pSysInterface` variable (a `ZSysInterface**`).
static mut G_SYS_INTERFACE: *mut *mut u8 = null_mut();

/// The `CDAPFN0506_WinMain` export of the original exe: a C-Dilla
/// copy-protection descriptor. The protection was removed from the Steam
/// release, but the symbol is reproduced verbatim (with our WinMain address)
/// so anything that looks for it finds the same layout.
#[repr(C)]
pub struct CdapfnProperties {
    magic: u32,
    flags: u32,
    size: u32,
    entry: unsafe extern "system" fn(HMODULE, HMODULE, *const c_char, i32) -> i32,
    params: u32,
    reserved: [u32; 11],
}
#[export_name = "?CDAPFN0506_WinMain@@3UCDAPFN_PROPERTIES@@A"]
#[used]
pub static CDAPFN0506_WINMAIN: CdapfnProperties = CdapfnProperties {
    magic: 0xD4F7_0933,
    flags: 3,
    size: 0x84,
    entry: win_main,
    params: 4,
    reserved: [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
};

/// Signal handler. Same behaviour as the original handler at 0x401010.
extern "C" fn crash_handler(_sig: i32) {
    unsafe {
        let pp = G_SYS_INTERFACE;
        if pp.is_null() {
            return;
        }
        let sys = *pp;
        if sys.is_null() {
            return;
        }
        for off in SUBSYSTEM_OFFSETS {
            let sub = (sys.add(off) as *const *mut *const usize).read_unaligned();
            if !sub.is_null() {
                let vtbl = *sub;
                let shutdown: extern "thiscall" fn(*mut c_void) =
                    std::mem::transmute(*vtbl.add(VTBL_SHUTDOWN));
                shutdown(sub as *mut c_void);
            }
        }
    }
}

/// The copy-protection check at 0x401000 (`mov al,1 / ret`): always passes.
fn protection_check(_instance: HMODULE) -> bool {
    true
}

fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("HitMan-Rust.exe"))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message(text: &str, flags: u32) {
    let t = wide(text);
    let c = wide("HitMan-Rust");
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), MB_OK | flags) };
}

/// Splits the raw `GetCommandLineA` string the way the MSVC CRT does for
/// `WinMain`'s `lpCmdLine`: skip the (possibly quoted) program name and the
/// whitespace after it.
fn crt_cmdline_tail(raw: &[u8]) -> &[u8] {
    let mut i = 0;
    if raw.first() == Some(&b'"') {
        i = 1;
        while i < raw.len() && raw[i] != b'"' {
            i += 1;
        }
        if i < raw.len() {
            i += 1;
        }
    } else {
        while i < raw.len() && raw[i] > b' ' {
            i += 1;
        }
    }
    while i < raw.len() && (raw[i] == b' ' || raw[i] == b'\t') {
        i += 1;
    }
    &raw[i..]
}

/// Options understood by this launcher. They are removed from the command line
/// before it reaches the engine.
#[derive(Default)]
struct LauncherOptions {
    native_d3d: bool,
}

fn strip_launcher_options(tail: &[u8]) -> (LauncherOptions, Vec<u8>) {
    let mut opts = LauncherOptions::default();
    let text = String::from_utf8_lossy(tail).into_owned();
    let mut kept: Vec<&str> = Vec::new();
    for tok in text.split_whitespace() {
        match tok.to_ascii_lowercase().as_str() {
            "-d3d" | "--d3d" | "/d3d" => opts.native_d3d = true,
            _ => kept.push(tok),
        }
    }
    // Preserve the engine's part of the command line byte-for-byte when we did
    // not remove anything.
    if !opts.native_d3d {
        return (opts, tail.to_vec());
    }
    (opts, kept.join(" ").into_bytes())
}

/// Step 5 of the original WinMain: build the configuration argument.
fn config_argument(cmd: &[u8], exe: &Path) -> CString {
    if cmd.len() >= 2 {
        // "@file" selects a configuration file explicitly.
        if let Some(rest) = cmd.strip_prefix(b"@") {
            let path = String::from_utf8_lossy(rest).trim().trim_matches('"').to_string();
            std::env::set_var("HITMAN_RUST_INI", path);
        }
        return CString::new(cmd.to_vec()).unwrap_or_else(|_| CString::new("@main.ini").unwrap());
    }
    let own_ini = exe.with_extension("ini");
    let stock_ini = exe.with_file_name("Hitman.ini");
    for ini in [own_ini, stock_ini] {
        if ini.is_file() {
            // Tell the Vulkan renderer which file the engine is reading, so
            // it can honour resolutions the engine's 4:3 table rejects.
            std::env::set_var("HITMAN_RUST_INI", &ini);
            return CString::new(format!("@{}", ini.display())).unwrap();
        }
    }
    CString::new("@main.ini").unwrap()
}

/// Loads the Vulkan DirectDraw/Direct3D 7 implementation from
/// `<exe dir>\HitManVK\ddraw.dll`. When RenderD3D.dll is loaded later, the
/// loader finds a module named `ddraw.dll` already loaded and binds its
/// imports to it.
fn preload_vulkan_renderer(exe_dir: &Path) -> bool {
    let dll = exe_dir.join("HitManVK").join("ddraw.dll");
    if !dll.is_file() {
        message(
            &format!(
                "The Vulkan renderer was not found:\n{}\n\nThe game will use the system Direct3D instead.",
                dll.display()
            ),
            MB_ICONWARNING,
        );
        return false;
    }
    let w: Vec<u16> = dll.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
    let h = unsafe { LoadLibraryExW(w.as_ptr(), null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if h.is_null() {
        let err = unsafe { GetLastError() };
        message(
            &format!("Failed to load {} (error {err}).\nThe game will use the system Direct3D instead.", dll.display()),
            MB_ICONWARNING,
        );
        return false;
    }
    true
}

/// The original `WinMain(hInstance, hPrevInstance, lpCmdLine, nShowCmd)`.
unsafe extern "system" fn win_main(
    instance: HMODULE,
    _prev: HMODULE,
    cmd_line: *const c_char,
    _show: i32,
) -> i32 {
    // 1. Single instance.
    CreateMutexA(null(), 0, b"Hitboy\0".as_ptr());
    if GetLastError() == ERROR_ALREADY_EXISTS {
        return 0;
    }
    // 2. Copy protection (always passes).
    if !protection_check(instance) {
        return 1;
    }
    // 3. Crash handlers.
    let handler = crash_handler as extern "C" fn(i32) as usize;
    for sig in [SIGFPE, SIGABRT, SIGILL, SIGSEGV] {
        signal(sig, handler);
    }
    // 4. Load the engine.
    let system = LoadLibraryA(b"System.dll\0".as_ptr());
    if system.is_null() {
        message("Unable to load System.dll.\nHitMan-Rust.exe must be in the Hitman: Codename 47 folder.", MB_ICONERROR);
        return 1;
    }
    let pp = G_SYS_INTERFACE;
    if pp.is_null() || (*pp).is_null() {
        message("System.dll did not initialise g_pSysInterface.", MB_ICONERROR);
        FreeLibrary(system);
        return 1;
    }
    // 5. Configuration argument.
    let cmd = std::ffi::CStr::from_ptr(cmd_line).to_bytes();
    let arg = config_argument(cmd, &exe_path());
    // 6. Run the engine.
    let sys = *pp;
    let vtbl = *(sys as *const *const usize);
    let run: extern "thiscall" fn(*mut u8, *const c_char) -> u32 =
        std::mem::transmute(*vtbl.add(VTBL_RUN / 4));
    run(sys, arg.as_ptr());
    // 7. Unload.
    FreeLibrary(system);
    0
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let exe = exe_path();
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    // The engine opens its data files relative to the current directory.
    let _ = std::env::set_current_dir(&exe_dir);

    let raw = unsafe { std::ffi::CStr::from_ptr(GetCommandLineA() as *const c_char) }.to_bytes();
    let (opts, engine_cmd) = strip_launcher_options(crt_cmdline_tail(raw));

    // Globals.dll was a static import of the original exe, so it is loaded
    // before WinMain runs.
    let globals = unsafe { LoadLibraryA(b"Globals.dll\0".as_ptr()) };
    if globals.is_null() {
        message("Unable to load Globals.dll.\nHitMan-Rust.exe must be in the Hitman: Codename 47 folder.", MB_ICONERROR);
        std::process::exit(1);
    }
    unsafe {
        G_SYS_INTERFACE = GetProcAddress(globals, G_PSYSINTERFACE.as_ptr())
            .map(|f| f as *mut *mut u8)
            .unwrap_or(null_mut());
    }

    if !opts.native_d3d {
        preload_vulkan_renderer(&exe_dir);
    }

    let cmd = CString::new(engine_cmd).unwrap_or_default();
    let instance = unsafe { GetModuleHandleW(null()) };
    let code = unsafe { win_main(instance, null_mut(), cmd.as_ptr(), 1) };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmdline_tail() {
        assert_eq!(crt_cmdline_tail(b"\"C:\\a b\\x.exe\"  -foo"), b"-foo");
        assert_eq!(crt_cmdline_tail(b"x.exe"), b"");
        assert_eq!(crt_cmdline_tail(b"x.exe   @a.ini"), b"@a.ini");
    }

    #[test]
    fn strip_d3d() {
        let (o, c) = strip_launcher_options(b"-d3d @x.ini");
        assert!(o.native_d3d);
        assert_eq!(c, b"@x.ini");
        let (o, c) = strip_launcher_options(b"@y.ini");
        assert!(!o.native_d3d);
        assert_eq!(c, b"@y.ini");
    }
}
