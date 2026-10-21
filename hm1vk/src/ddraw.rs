//! IDirectDraw7 and the DDRAW.dll entry points.

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::d3d::Direct3D;
use crate::display;
use crate::state::{world, Mode};
use crate::surface::{Clipper, Palette, Surface};
use crate::types::*;
use crate::{debug, first_call, info, unimpl};

type This = *mut DirectDraw;

#[repr(C)]
pub struct IDirectDraw7Vtbl {
    pub query_interface: unsafe extern "system" fn(This, *const GUID, *mut *mut c_void) -> HRESULT,
    pub add_ref: unsafe extern "system" fn(This) -> u32,
    pub release: unsafe extern "system" fn(This) -> u32,
    pub compact: unsafe extern "system" fn(This) -> HRESULT,
    pub create_clipper: unsafe extern "system" fn(This, u32, *mut *mut Clipper, *mut c_void) -> HRESULT,
    pub create_palette:
        unsafe extern "system" fn(This, u32, *const PALETTEENTRY, *mut *mut Palette, *mut c_void) -> HRESULT,
    pub create_surface: unsafe extern "system" fn(This, *mut DDSURFACEDESC2, *mut *mut Surface, *mut c_void) -> HRESULT,
    pub duplicate_surface: unsafe extern "system" fn(This, *mut Surface, *mut *mut Surface) -> HRESULT,
    pub enum_display_modes:
        unsafe extern "system" fn(This, u32, *mut DDSURFACEDESC2, *mut c_void, Option<LPDDENUMMODESCALLBACK2>) -> HRESULT,
    pub enum_surfaces: unsafe extern "system" fn(This, u32, *mut DDSURFACEDESC2, *mut c_void, *mut c_void) -> HRESULT,
    pub flip_to_gdi_surface: unsafe extern "system" fn(This) -> HRESULT,
    pub get_caps: unsafe extern "system" fn(This, *mut u8, *mut u8) -> HRESULT,
    pub get_display_mode: unsafe extern "system" fn(This, *mut DDSURFACEDESC2) -> HRESULT,
    pub get_fourcc_codes: unsafe extern "system" fn(This, *mut u32, *mut u32) -> HRESULT,
    pub get_gdi_surface: unsafe extern "system" fn(This, *mut *mut Surface) -> HRESULT,
    pub get_monitor_frequency: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    pub get_scan_line: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    pub get_vertical_blank_status: unsafe extern "system" fn(This, *mut BOOL) -> HRESULT,
    pub initialize: unsafe extern "system" fn(This, *mut GUID) -> HRESULT,
    pub restore_display_mode: unsafe extern "system" fn(This) -> HRESULT,
    pub set_cooperative_level: unsafe extern "system" fn(This, isize, u32) -> HRESULT,
    pub set_display_mode: unsafe extern "system" fn(This, u32, u32, u32, u32, u32) -> HRESULT,
    pub wait_for_vertical_blank: unsafe extern "system" fn(This, u32, isize) -> HRESULT,
    pub get_available_vid_mem: unsafe extern "system" fn(This, *mut DDSCAPS2, *mut u32, *mut u32) -> HRESULT,
    pub get_surface_from_dc: unsafe extern "system" fn(This, isize, *mut *mut Surface) -> HRESULT,
    pub restore_all_surfaces: unsafe extern "system" fn(This) -> HRESULT,
    pub test_cooperative_level: unsafe extern "system" fn(This) -> HRESULT,
    pub get_device_identifier: unsafe extern "system" fn(This, *mut DDDEVICEIDENTIFIER2, u32) -> HRESULT,
    pub start_mode_test: unsafe extern "system" fn(This, *mut c_void, u32, u32) -> HRESULT,
    pub evaluate_mode: unsafe extern "system" fn(This, u32, *mut u32) -> HRESULT,
}

#[repr(C)]
pub struct DirectDraw {
    vtbl: *const IDirectDraw7Vtbl,
    refs: u32,
    pub d3d: *mut Direct3D,
}

static DDRAW7_VTBL: IDirectDraw7Vtbl = IDirectDraw7Vtbl {
    query_interface: dd_query_interface,
    add_ref: dd_add_ref,
    release: dd_release,
    compact: dd_compact,
    create_clipper: dd_create_clipper,
    create_palette: dd_create_palette,
    create_surface: dd_create_surface,
    duplicate_surface: dd_duplicate_surface,
    enum_display_modes: dd_enum_display_modes,
    enum_surfaces: dd_enum_surfaces,
    flip_to_gdi_surface: dd_flip_to_gdi_surface,
    get_caps: dd_get_caps,
    get_display_mode: dd_get_display_mode,
    get_fourcc_codes: dd_get_fourcc_codes,
    get_gdi_surface: dd_get_gdi_surface,
    get_monitor_frequency: dd_get_monitor_frequency,
    get_scan_line: dd_get_scan_line,
    get_vertical_blank_status: dd_get_vertical_blank_status,
    initialize: dd_initialize,
    restore_display_mode: dd_restore_display_mode,
    set_cooperative_level: dd_set_cooperative_level,
    set_display_mode: dd_set_display_mode,
    wait_for_vertical_blank: dd_wait_for_vertical_blank,
    get_available_vid_mem: dd_get_available_vid_mem,
    get_surface_from_dc: dd_get_surface_from_dc,
    restore_all_surfaces: dd_restore_all_surfaces,
    test_cooperative_level: dd_test_cooperative_level,
    get_device_identifier: dd_get_device_identifier,
    start_mode_test: dd_start_mode_test,
    evaluate_mode: dd_evaluate_mode,
};

impl DirectDraw {
    pub fn create() -> *mut DirectDraw {
        let dd = Box::into_raw(Box::new(DirectDraw { vtbl: &DDRAW7_VTBL, refs: 1, d3d: null_mut() }));
        unsafe {
            (*dd).d3d = Direct3D::create(dd);
        }
        dd
    }
    pub unsafe fn add_ref(this: This) {
        (*this).refs += 1;
    }
    pub unsafe fn release(this: This) -> u32 {
        dd_release(this)
    }
}

// ------------------------------------------------------------ exports

/// HRESULT WINAPI DirectDrawCreateEx(GUID*, LPVOID*, REFIID, IUnknown*)
#[no_mangle]
pub unsafe extern "system" fn DirectDrawCreateEx(
    guid: *mut GUID,
    out: *mut *mut c_void,
    iid: *const GUID,
    _outer: *mut c_void,
) -> HRESULT {
    let g = if guid.is_null() { "NULL".to_string() } else { (*guid).to_string() };
    info!("DirectDrawCreateEx(guid={g}, iid={})", if iid.is_null() { "NULL".into() } else { (*iid).to_string() });
    crate::engine::fix_resolution();
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    if !iid.is_null() && *iid != IID_IDIRECTDRAW7 {
        info!("DirectDrawCreateEx: only IID_IDirectDraw7 is supported");
        return DDERR_INVALIDPARAMS;
    }
    if world().gpu().is_none() {
        *out = null_mut();
        return DDERR_GENERIC;
    }
    *out = DirectDraw::create() as *mut c_void;
    DD_OK
}

/// HRESULT WINAPI DirectDrawCreate(GUID*, LPDIRECTDRAW*, IUnknown*) - only
/// DirectDraw 7 is implemented.
#[no_mangle]
pub unsafe extern "system" fn DirectDrawCreate(_guid: *mut GUID, out: *mut *mut c_void, _outer: *mut c_void) -> HRESULT {
    unimpl!("DirectDrawCreate (legacy IDirectDraw)");
    if !out.is_null() {
        *out = null_mut();
    }
    DDERR_UNSUPPORTED
}

#[no_mangle]
pub unsafe extern "system" fn DirectDrawCreateClipper(_flags: u32, out: *mut *mut Clipper, _outer: *mut c_void) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *out = Clipper::create();
    DD_OK
}

const DRIVER_DESC: &[u8] = b"Primary Display Driver (HitMan-Rust Vulkan)\0";
const DRIVER_NAME: &[u8] = b"display\0";

/// HRESULT WINAPI DirectDrawEnumerateExA(LPDDENUMCALLBACKEXA, LPVOID, DWORD)
#[no_mangle]
pub unsafe extern "system" fn DirectDrawEnumerateExA(cb: Option<LPDDENUMCALLBACKEXA>, ctx: *mut c_void, flags: u32) -> HRESULT {
    first_call!("DirectDrawEnumerateExA", "flags={flags:#x}");
    crate::engine::fix_resolution();
    if let Some(cb) = cb {
        cb(null_mut(), DRIVER_DESC.as_ptr(), DRIVER_NAME.as_ptr(), ctx, null_mut());
    }
    DD_OK
}

#[no_mangle]
pub unsafe extern "system" fn DirectDrawEnumerateA(cb: Option<LPDDENUMCALLBACKA>, ctx: *mut c_void) -> HRESULT {
    crate::engine::fix_resolution();
    if let Some(cb) = cb {
        cb(null_mut(), DRIVER_DESC.as_ptr(), DRIVER_NAME.as_ptr(), ctx);
    }
    DD_OK
}

// ------------------------------------------------------------ IUnknown

unsafe extern "system" fn dd_query_interface(this: This, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() || iid.is_null() {
        return E_POINTER;
    }
    let iid = *iid;
    if iid == IID_IDIRECTDRAW7 || iid == IID_IUNKNOWN {
        DirectDraw::add_ref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    if iid == IID_IDIRECT3D7 {
        let d3d = (*this).d3d;
        Direct3D::add_ref(d3d);
        *out = d3d as *mut c_void;
        return S_OK;
    }
    info!("IDirectDraw7::QueryInterface: unsupported {iid}");
    *out = null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn dd_add_ref(this: This) -> u32 {
    (*this).refs += 1;
    (*this).refs
}

unsafe extern "system" fn dd_release(this: This) -> u32 {
    (*this).refs -= 1;
    let r = (*this).refs;
    if r == 0 {
        info!("IDirectDraw7 released");
        let w = world();
        if w.display_changed {
            display::restore_display_mode();
            w.display_changed = false;
        }
        w.mode = None;
        w.coop = 0;
        Direct3D::destroy((*this).d3d);
        drop(Box::from_raw(this));
    }
    r
}

// ------------------------------------------------------------ methods

unsafe extern "system" fn dd_compact(_this: This) -> HRESULT {
    DD_OK
}

unsafe extern "system" fn dd_create_clipper(_this: This, _flags: u32, out: *mut *mut Clipper, _o: *mut c_void) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *out = Clipper::create();
    DD_OK
}

unsafe extern "system" fn dd_create_palette(
    _this: This,
    flags: u32,
    entries: *const PALETTEENTRY,
    out: *mut *mut Palette,
    _o: *mut c_void,
) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *out = Palette::create(flags, entries);
    DD_OK
}

unsafe extern "system" fn dd_create_surface(
    this: This,
    desc: *mut DDSURFACEDESC2,
    out: *mut *mut Surface,
    _o: *mut c_void,
) -> HRESULT {
    if desc.is_null() || out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    match Surface::create(this, &*desc) {
        Ok(s) => {
            *out = s;
            DD_OK
        }
        Err(e) => {
            *out = null_mut();
            e
        }
    }
}

unsafe extern "system" fn dd_duplicate_surface(_this: This, _s: *mut Surface, out: *mut *mut Surface) -> HRESULT {
    unimpl!("IDirectDraw7::DuplicateSurface");
    if !out.is_null() {
        *out = null_mut();
    }
    DDERR_UNSUPPORTED
}

pub fn mode_desc(m: &Mode) -> DDSURFACEDESC2 {
    let mut d = DDSURFACEDESC2::default();
    d.dwFlags = DDSD_WIDTH | DDSD_HEIGHT | DDSD_PITCH | DDSD_PIXELFORMAT | DDSD_REFRESHRATE;
    d.dwWidth = m.width;
    d.dwHeight = m.height;
    d.ddpfPixelFormat = if m.bpp == 16 { crate::pixel::pf_rgb565() } else { crate::pixel::pf_xrgb8888() };
    d.lPitch = (m.width * m.bpp / 8) as i32;
    d.dwMipMapCount = m.refresh; // dwRefreshRate
    d
}

unsafe extern "system" fn dd_enum_display_modes(
    _this: This,
    flags: u32,
    filter: *mut DDSURFACEDESC2,
    ctx: *mut c_void,
    cb: Option<LPDDENUMMODESCALLBACK2>,
) -> HRESULT {
    first_call!("IDirectDraw7::EnumDisplayModes", "flags={flags:#x}");
    let Some(cb) = cb else { return DDERR_INVALIDPARAMS };
    let modes = display::advertised_modes();
    for m in &modes {
        if !filter.is_null() {
            let f = &*filter;
            if f.dwFlags & DDSD_WIDTH != 0 && f.dwWidth != m.width {
                continue;
            }
            if f.dwFlags & DDSD_HEIGHT != 0 && f.dwHeight != m.height {
                continue;
            }
            if f.dwFlags & DDSD_PIXELFORMAT != 0 && f.ddpfPixelFormat.dwRGBBitCount != m.bpp {
                continue;
            }
        }
        let mut d = mode_desc(m);
        if flags & DDEDM_REFRESHRATES == 0 {
            d.dwMipMapCount = 0;
        }
        debug!("  mode {}x{}x{} {}Hz", m.width, m.height, m.bpp, m.refresh);
        if cb(&mut d, ctx) == DDENUMRET_CANCEL {
            break;
        }
    }
    DD_OK
}

unsafe extern "system" fn dd_enum_surfaces(_t: This, _f: u32, _d: *mut DDSURFACEDESC2, _c: *mut c_void, _cb: *mut c_void) -> HRESULT {
    unimpl!("IDirectDraw7::EnumSurfaces");
    DDERR_UNSUPPORTED
}

unsafe extern "system" fn dd_flip_to_gdi_surface(_this: This) -> HRESULT {
    DD_OK
}

pub unsafe fn fill_ddcaps(p: *mut u8) {
    if p.is_null() {
        return;
    }
    let size = (p as *const u32).read_unaligned() as usize;
    let size = if size == 0 || size > DDCAPS_DX7_SIZE { DDCAPS_DX7_SIZE } else { size };
    let mut buf = [0u8; DDCAPS_DX7_SIZE];
    let mut put = |off: usize, v: u32| buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    use ddcaps_off::*;
    let (total, free) = vid_mem();
    put(SIZE, size as u32);
    put(
        CAPS,
        DDCAPS_3D
            | DDCAPS_BLT
            | DDCAPS_BLTQUEUE
            | DDCAPS_BLTFOURCC
            | DDCAPS_BLTSTRETCH
            | DDCAPS_COLORKEY
            | DDCAPS_BLTCOLORFILL
            | DDCAPS_BLTDEPTHFILL
            | DDCAPS_CANBLTSYSMEM
            | DDCAPS_ZBLTS,
    );
    put(
        CAPS2,
        DDCAPS2_CERTIFIED
            | DDCAPS2_WIDESURFACES
            | DDCAPS2_COPYFOURCC
            | DDCAPS2_PRIMARYGAMMA
            | DDCAPS2_CANRENDERWINDOWED
            | DDCAPS2_FLIPNOVSYNC
            | DDCAPS2_NOPAGELOCKREQUIRED,
    );
    put(CKEYCAPS, DDCKEYCAPS_SRCBLT);
    put(ZBUFFERBITDEPTHS, DDBD_16 | DDBD_24 | DDBD_32);
    put(VIDMEMTOTAL, total);
    put(VIDMEMFREE, free);
    put(NUMFOURCC, 5);
    let scaps = DDSCAPS_BACKBUFFER
        | DDSCAPS_COMPLEX
        | DDSCAPS_FLIP
        | DDSCAPS_FRONTBUFFER
        | DDSCAPS_OFFSCREENPLAIN
        | DDSCAPS_PALETTE
        | DDSCAPS_PRIMARYSURFACE
        | DDSCAPS_TEXTURE
        | DDSCAPS_3DDEVICE
        | DDSCAPS_VIDEOMEMORY
        | DDSCAPS_ZBUFFER
        | DDSCAPS_MIPMAP
        | DDSCAPS_LOCALVIDMEM;
    put(OLDCAPS, scaps);
    put(SVBCAPS, DDCAPS_BLT | DDCAPS_BLTSTRETCH | DDCAPS_COLORKEY);
    put(VSBCAPS, DDCAPS_BLT | DDCAPS_BLTSTRETCH | DDCAPS_COLORKEY);
    put(SSBCAPS, DDCAPS_BLT | DDCAPS_BLTSTRETCH | DDCAPS_COLORKEY);
    put(DDSCAPS, scaps);
    std::ptr::copy_nonoverlapping(buf.as_ptr(), p, size);
}

pub fn vid_mem() -> (u32, u32) {
    // 32-bit values; keep well below 2 GB so signed arithmetic in the engine
    // does not overflow.
    let vram = world().gpu_if_ready().map(|g| g.vram_bytes).unwrap_or(512 << 20);
    let total = vram.min(1 << 30) as u32;
    (total, total - total / 16)
}

unsafe extern "system" fn dd_get_caps(_this: This, drv: *mut u8, hel: *mut u8) -> HRESULT {
    first_call!("IDirectDraw7::GetCaps");
    fill_ddcaps(drv);
    fill_ddcaps(hel);
    DD_OK
}

unsafe extern "system" fn dd_get_display_mode(_this: This, desc: *mut DDSURFACEDESC2) -> HRESULT {
    if desc.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let m = world().screen_mode();
    let d = mode_desc(&m);
    let size = ((*desc).dwSize as usize).clamp(4, std::mem::size_of::<DDSURFACEDESC2>());
    std::ptr::copy_nonoverlapping(&d as *const _ as *const u8, desc as *mut u8, size);
    (*desc).dwSize = size as u32;
    DD_OK
}

unsafe extern "system" fn dd_get_fourcc_codes(_this: This, num: *mut u32, codes: *mut u32) -> HRESULT {
    let list = [FOURCC_DXT1, FOURCC_DXT2, FOURCC_DXT3, FOURCC_DXT4, FOURCC_DXT5];
    if num.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    if !codes.is_null() {
        let n = (*num as usize).min(list.len());
        std::ptr::copy_nonoverlapping(list.as_ptr(), codes, n);
    }
    *num = list.len() as u32;
    DD_OK
}

unsafe extern "system" fn dd_get_gdi_surface(_this: This, out: *mut *mut Surface) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let p = world().primary;
    if p.is_null() {
        *out = null_mut();
        return DDERR_NOTFOUND;
    }
    Surface::add_ref(p);
    *out = p;
    DD_OK
}

unsafe extern "system" fn dd_get_monitor_frequency(_this: This, f: *mut u32) -> HRESULT {
    if !f.is_null() {
        *f = world().desktop.refresh.max(60);
    }
    DD_OK
}

unsafe extern "system" fn dd_get_scan_line(_this: This, l: *mut u32) -> HRESULT {
    if !l.is_null() {
        *l = 0;
    }
    DD_OK
}

unsafe extern "system" fn dd_get_vertical_blank_status(_this: This, b: *mut BOOL) -> HRESULT {
    static mut T: bool = false;
    if !b.is_null() {
        T = !T;
        *b = T as BOOL;
    }
    DD_OK
}

unsafe extern "system" fn dd_initialize(_this: This, _g: *mut GUID) -> HRESULT {
    DDERR_ALREADYINITIALIZED
}
const DDERR_ALREADYINITIALIZED: HRESULT = 0x88760005u32 as HRESULT;

unsafe extern "system" fn dd_restore_display_mode(_this: This) -> HRESULT {
    let w = world();
    if w.display_changed {
        display::restore_display_mode();
        w.display_changed = false;
    }
    w.mode = None;
    DD_OK
}

unsafe extern "system" fn dd_set_cooperative_level(_this: This, hwnd: isize, flags: u32) -> HRESULT {
    info!("IDirectDraw7::SetCooperativeLevel(hwnd={hwnd:#x}, flags={flags:#x})");
    let w = world();
    if hwnd != 0 {
        w.hwnd = hwnd;
    }
    w.coop = flags;
    if flags & DDSCL_FULLSCREEN != 0 && w.hwnd != 0 {
        display::make_fullscreen_window(w.hwnd);
    }
    DD_OK
}

unsafe extern "system" fn dd_set_display_mode(_this: This, width: u32, height: u32, bpp: u32, refresh: u32, flags: u32) -> HRESULT {
    info!("IDirectDraw7::SetDisplayMode({width}x{height}x{bpp} @{refresh}Hz, flags={flags:#x})");
    let w = world();
    let bpp = if bpp == 0 { 32 } else { bpp };
    if !matches!(bpp, 8 | 16 | 24 | 32) {
        return DDERR_INVALIDMODE;
    }
    if w.config.change_display_mode {
        if display::change_display_mode(width, height) {
            w.display_changed = true;
        } else if w.display_changed && (w.desktop.width, w.desktop.height) == (width, height) {
            display::restore_display_mode();
            w.display_changed = false;
        }
    }
    w.mode = Some(Mode { width, height, bpp, refresh: if refresh == 0 { 60 } else { refresh } });
    if w.hwnd != 0 && w.is_fullscreen() {
        display::make_fullscreen_window(w.hwnd);
    }
    DD_OK
}

unsafe extern "system" fn dd_wait_for_vertical_blank(_this: This, _flags: u32, _ev: isize) -> HRESULT {
    DD_OK
}

unsafe extern "system" fn dd_get_available_vid_mem(_this: This, caps: *mut DDSCAPS2, total: *mut u32, free: *mut u32) -> HRESULT {
    let (t, f) = vid_mem();
    if !caps.is_null() {
        first_call!("IDirectDraw7::GetAvailableVidMem", "caps={:#x}", (*caps).dwCaps);
    }
    if !total.is_null() {
        *total = t;
    }
    if !free.is_null() {
        *free = f;
    }
    DD_OK
}

unsafe extern "system" fn dd_get_surface_from_dc(_this: This, _hdc: isize, out: *mut *mut Surface) -> HRESULT {
    unimpl!("IDirectDraw7::GetSurfaceFromDC");
    if !out.is_null() {
        *out = null_mut();
    }
    DDERR_NOTFOUND
}

unsafe extern "system" fn dd_restore_all_surfaces(_this: This) -> HRESULT {
    DD_OK
}

unsafe extern "system" fn dd_test_cooperative_level(_this: This) -> HRESULT {
    DD_OK
}

unsafe extern "system" fn dd_get_device_identifier(_this: This, id: *mut DDDEVICEIDENTIFIER2, _flags: u32) -> HRESULT {
    if id.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let mut d: DDDEVICEIDENTIFIER2 = std::mem::zeroed();
    let w = world();
    let (name, vendor, devid) = match w.gpu_if_ready() {
        Some(g) => (g.device_name.clone(), g.props.vendor_id, g.props.device_id),
        None => ("Vulkan".to_string(), 0, 0),
    };
    let drv = b"hm1vk (ddraw.dll on Vulkan)";
    d.szDriver[..drv.len()].copy_from_slice(drv);
    let desc = format!("{name} (Vulkan)");
    let n = desc.len().min(511);
    d.szDescription[..n].copy_from_slice(&desc.as_bytes()[..n]);
    d.liDriverVersion = (7u64 << 48) | 1;
    d.dwVendorId = vendor;
    d.dwDeviceId = devid;
    d.guidDeviceIdentifier = HM1VK_DEVICE_GUID;
    d.dwWHQLLevel = 1;
    *id = d;
    DD_OK
}

unsafe extern "system" fn dd_start_mode_test(_this: This, _m: *mut c_void, _n: u32, _f: u32) -> HRESULT {
    DDERR_UNSUPPORTED
}

unsafe extern "system" fn dd_evaluate_mode(_this: This, _f: u32, _t: *mut u32) -> HRESULT {
    DDERR_UNSUPPORTED
}
