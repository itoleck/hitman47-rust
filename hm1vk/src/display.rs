//! Display modes and the game window (fullscreen emulation).

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, CDS_FULLSCREEN,
    DEVMODEW, DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH,
    ENUM_CURRENT_SETTINGS, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, MessageBoxW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, MB_ICONERROR, MB_OK,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW, WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME,
    WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
};

use crate::info;
use crate::state::Mode;

pub fn error_box(text: &str) {
    let t: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let c: Vec<u16> = "HitMan-Rust (Vulkan)".encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn devmode() -> DEVMODEW {
    let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
    dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    dm
}

pub fn desktop_mode() -> Mode {
    let mut dm = devmode();
    unsafe {
        if EnumDisplaySettingsW(null(), ENUM_CURRENT_SETTINGS, &mut dm) != 0 {
            return Mode {
                width: dm.dmPelsWidth,
                height: dm.dmPelsHeight,
                bpp: dm.dmBitsPerPel.max(32),
                refresh: dm.dmDisplayFrequency,
            };
        }
    }
    Mode { width: 1920, height: 1080, bpp: 32, refresh: 60 }
}

/// Resolutions the primary monitor supports, with the highest refresh rate of each.
pub fn monitor_modes() -> Vec<(u32, u32, u32)> {
    let mut out: Vec<(u32, u32, u32)> = Vec::new();
    let mut i = 0;
    loop {
        let mut dm = devmode();
        if unsafe { EnumDisplaySettingsW(null(), i, &mut dm) } == 0 {
            break;
        }
        i += 1;
        if dm.dmBitsPerPel < 16 {
            continue;
        }
        let (w, h, f) = (dm.dmPelsWidth, dm.dmPelsHeight, dm.dmDisplayFrequency);
        if let Some(e) = out.iter_mut().find(|e| e.0 == w && e.1 == h) {
            e.2 = e.2.max(f);
        } else {
            out.push((w, h, f));
        }
    }
    out
}

/// The list of modes advertised to the game: everything the monitor supports
/// up to the configured maximum (3840x2160 by default), plus configured extra
/// modes which are rendered at that size and scaled to the display.
pub fn advertised_modes() -> Vec<Mode> {
    let cfg = crate::state::config();
    let mut res: Vec<(u32, u32, u32)> = monitor_modes()
        .into_iter()
        .filter(|m| m.0 >= 640 && m.1 >= 480 && m.0 <= cfg.max_width && m.1 <= cfg.max_height)
        .collect();
    for &(w, h) in &cfg.extra_modes {
        if w <= cfg.max_width && h <= cfg.max_height && !res.iter().any(|m| m.0 == w && m.1 == h) {
            res.push((w, h, 60));
        }
    }
    let d = desktop_mode();
    if d.width <= cfg.max_width && d.height <= cfg.max_height && !res.iter().any(|m| m.0 == d.width && m.1 == d.height) {
        res.push((d.width, d.height, d.refresh));
    }
    res.sort();
    let mut modes = Vec::new();
    for (w, h, f) in res {
        if cfg.modes_16bit {
            modes.push(Mode { width: w, height: h, bpp: 16, refresh: f });
        }
        modes.push(Mode { width: w, height: h, bpp: 32, refresh: f });
    }
    modes
}

/// Switches the desktop resolution (keeps 32-bit colour; 16-bit game modes
/// are rendered in 32-bit internally). Returns true if the display changed.
pub fn change_display_mode(w: u32, h: u32) -> bool {
    let desk = desktop_mode();
    if desk.width == w && desk.height == h {
        return false;
    }
    let Some(&(_, _, freq)) = monitor_modes().iter().find(|m| m.0 == w && m.1 == h) else {
        info!("monitor does not support {w}x{h}; rendering at that size and scaling");
        return false;
    };
    let mut dm = devmode();
    dm.dmPelsWidth = w;
    dm.dmPelsHeight = h;
    dm.dmBitsPerPel = 32;
    dm.dmDisplayFrequency = freq;
    dm.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL | DM_DISPLAYFREQUENCY;
    let r = unsafe { ChangeDisplaySettingsExW(null(), &dm, null_mut(), CDS_FULLSCREEN, null()) };
    info!("ChangeDisplaySettingsEx {w}x{h}@{freq}: {r}");
    r == DISP_CHANGE_SUCCESSFUL
}

pub fn restore_display_mode() {
    unsafe { ChangeDisplaySettingsExW(null(), null(), null_mut(), 0, null()) };
    info!("display mode restored");
}

pub fn monitor_rect(hwnd: isize) -> RECT {
    unsafe {
        let mon = MonitorFromWindow(hwnd as HWND, MONITOR_DEFAULTTOPRIMARY);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            return mi.rcMonitor;
        }
    }
    let d = desktop_mode();
    RECT { left: 0, top: 0, right: d.width as i32, bottom: d.height as i32 }
}

/// Makes the game window a borderless window covering its monitor.
pub fn make_fullscreen_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let h = hwnd as HWND;
        let style = GetWindowLongW(h, GWL_STYLE) as u32;
        let new_style = (style & !(WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MAXIMIZEBOX | WS_MINIMIZEBOX)) | WS_POPUP | WS_VISIBLE;
        SetWindowLongW(h, GWL_STYLE, new_style as i32);
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        SetWindowLongW(h, GWL_EXSTYLE, (ex & !(WS_EX_CLIENTEDGE | WS_EX_WINDOWEDGE | WS_EX_DLGMODALFRAME)) as i32);
        let r = monitor_rect(hwnd);
        SetWindowPos(
            h,
            HWND_TOP,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
        );
    }
}
