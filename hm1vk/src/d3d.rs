//! IDirect3D7: device enumeration and capabilities, device / vertex buffer creation.

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::ddraw::DirectDraw;
use crate::device::Device;
use crate::pixel;
use crate::surface::Surface;
use crate::types::*;
use crate::vb::VertexBuffer;
use crate::{first_call, info};

type This = *mut Direct3D;

#[repr(C)]
pub struct Direct3D {
    vtbl: *const Direct3DVtbl,
    refs: u32,
    pub ddraw: *mut DirectDraw,
}

#[repr(C)]
struct Direct3DVtbl {
    query_interface: unsafe extern "system" fn(This, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(This) -> u32,
    release: unsafe extern "system" fn(This) -> u32,
    enum_devices: unsafe extern "system" fn(This, Option<LPD3DENUMDEVICESCALLBACK7>, *mut c_void) -> HRESULT,
    create_device: unsafe extern "system" fn(This, *const GUID, *mut Surface, *mut *mut Device) -> HRESULT,
    create_vertex_buffer: unsafe extern "system" fn(This, *mut D3DVERTEXBUFFERDESC, *mut *mut VertexBuffer, u32) -> HRESULT,
    enum_z_buffer_formats:
        unsafe extern "system" fn(This, *const GUID, Option<LPD3DENUMPIXELFORMATSCALLBACK>, *mut c_void) -> HRESULT,
    evict_managed_textures: unsafe extern "system" fn(This) -> HRESULT,
}

static D3D7_VTBL: Direct3DVtbl = Direct3DVtbl {
    query_interface: d_query_interface,
    add_ref: d_add_ref,
    release: d_release,
    enum_devices: d_enum_devices,
    create_device: d_create_device,
    create_vertex_buffer: d_create_vertex_buffer,
    enum_z_buffer_formats: d_enum_z_buffer_formats,
    evict_managed_textures: d_evict_managed_textures,
};

impl Direct3D {
    pub fn create(ddraw: *mut DirectDraw) -> This {
        // The Direct3D object is aggregated with its DirectDraw object: its
        // own count only tracks references handed out via QueryInterface.
        Box::into_raw(Box::new(Direct3D { vtbl: &D3D7_VTBL, refs: 0, ddraw }))
    }
    pub unsafe fn add_ref(this: This) {
        (*this).refs += 1;
        DirectDraw::add_ref((*this).ddraw);
    }
    pub unsafe fn release(this: This) {
        d_release(this);
    }
    pub unsafe fn destroy(this: This) {
        if !this.is_null() {
            drop(Box::from_raw(this));
        }
    }
}

unsafe extern "system" fn d_query_interface(this: This, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    if *iid == IID_IDIRECT3D7 || *iid == IID_IUNKNOWN {
        Direct3D::add_ref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    if *iid == IID_IDIRECTDRAW7 {
        DirectDraw::add_ref((*this).ddraw);
        *out = (*this).ddraw as *mut c_void;
        return S_OK;
    }
    info!("IDirect3D7::QueryInterface: unsupported {}", *iid);
    *out = null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn d_add_ref(this: This) -> u32 {
    Direct3D::add_ref(this);
    (*this).refs
}

unsafe extern "system" fn d_release(this: This) -> u32 {
    if (*this).refs > 0 {
        (*this).refs -= 1;
    }
    let r = (*this).refs;
    // Releasing may destroy the DirectDraw object, which frees us.
    DirectDraw::release((*this).ddraw);
    r
}

/// Capabilities reported for our devices.
pub fn device_desc(guid: GUID) -> D3DDEVICEDESC7 {
    let tnl = guid == IID_IDIRECT3DTNLHALDEVICE;
    let prim = D3DPRIMCAPS {
        dwSize: 56,
        // MASKZ | CULLNONE | CULLCW | CULLCCW
        dwMiscCaps: 0x2 | 0x10 | 0x20 | 0x40,
        // DITHER | ZTEST | SUBPIXEL | FOGVERTEX | FOGTABLE | MIPMAPLODBIAS | ZBIAS | ANISOTROPY | WFOG | ZFOG | FOGRANGE
        dwRasterCaps: 0x1 | 0x10 | 0x20 | 0x80 | 0x100 | 0x2000 | 0x4000 | 0x20000 | 0x100000 | 0x200000 | 0x10000,
        dwZCmpCaps: 0xFF,
        dwSrcBlendCaps: 0x1FFF,
        dwDestBlendCaps: 0x1FFF,
        dwAlphaCmpCaps: 0xFF,
        // COLORGOURAUDRGB | SPECULARGOURAUDRGB | ALPHAGOURAUDBLEND | FOGGOURAUD
        dwShadeCaps: 0x8 | 0x200 | 0x4000 | 0x80000,
        // PERSPECTIVE | ALPHA | TRANSPARENCY | TEXREPEATNOTSCALEDBYSIZE | ALPHAPALETTE | PROJECTED
        dwTextureCaps: 0x1 | 0x4 | 0x8 | 0x40 | 0x80 | 0x400,
        dwTextureFilterCaps: 0x1 | 0x2 | 0x4 | 0x8 | 0x10 | 0x20 | 0x100 | 0x200 | 0x400 | 0x10000 | 0x20000 | 0x0100_0000 | 0x0200_0000 | 0x1000_0000,
        dwTextureBlendCaps: 0xFF,
        // WRAP | MIRROR | CLAMP | BORDER | INDEPENDENTUV
        dwTextureAddressCaps: 0x1F,
        dwStippleWidth: 0,
        dwStippleHeight: 0,
    };
    let mut d = D3DDEVICEDESC7 {
        dwDevCaps: D3DDEVCAPS_FLOATTLVERTEX
            | D3DDEVCAPS_EXECUTESYSTEMMEMORY
            | D3DDEVCAPS_EXECUTEVIDEOMEMORY
            | D3DDEVCAPS_TLVERTEXSYSTEMMEMORY
            | D3DDEVCAPS_TLVERTEXVIDEOMEMORY
            | D3DDEVCAPS_TEXTURESYSTEMMEMORY
            | D3DDEVCAPS_TEXTUREVIDEOMEMORY
            | D3DDEVCAPS_DRAWPRIMTLVERTEX
            | D3DDEVCAPS_CANRENDERAFTERFLIP
            | D3DDEVCAPS_TEXTURENONLOCALVIDMEM
            | D3DDEVCAPS_DRAWPRIMITIVES2
            | D3DDEVCAPS_DRAWPRIMITIVES2EX
            | D3DDEVCAPS_CANBLTSYSTONONLOCAL
            | D3DDEVCAPS_HWRASTERIZATION
            | if tnl { D3DDEVCAPS_HWTRANSFORMANDLIGHT } else { 0 },
        dpcLineCaps: prim,
        dpcTriCaps: prim,
        dwDeviceRenderBitDepth: DDBD_16 | DDBD_32,
        dwDeviceZBufferBitDepth: DDBD_16 | DDBD_24 | DDBD_32,
        dwMinTextureWidth: 1,
        dwMinTextureHeight: 1,
        dwMaxTextureWidth: 4096,
        dwMaxTextureHeight: 4096,
        dwMaxTextureRepeat: 8192,
        dwMaxTextureAspectRatio: 4096,
        dwMaxAnisotropy: 16,
        dvGuardBandLeft: -16384.0,
        dvGuardBandTop: -16384.0,
        dvGuardBandRight: 16384.0,
        dvGuardBandBottom: 16384.0,
        dvExtentsAdjust: 0.0,
        dwStencilCaps: 0xFF,
        // 8 texture coordinate sets, DONOTSTRIPELEMENTS
        dwFVFCaps: 8 | 0x80000,
        // all D3DTOP_* except the bump-mapping ops
        dwTextureOpCaps: 0x00FF_FFFF & !(0x0020_0000 | 0x0040_0000),
        wMaxTextureBlendStages: 8,
        wMaxSimultaneousTextures: 4,
        dwMaxActiveLights: 8,
        dvMaxVertexW: 1.0e10,
        deviceGUID: guid,
        wMaxUserClipPlanes: 0,
        wMaxVertexBlendMatrices: 0,
        // TEXGEN | MATERIALSOURCE7 | VERTEXFOG | DIRECTIONALLIGHTS | POSITIONALLIGHTS | LOCALVIEWER
        dwVertexProcessingCaps: 0x3F,
        dwReserved1: 0,
        dwReserved2: 0,
        dwReserved3: 0,
        dwReserved4: 0,
    };
    if !tnl {
        d.dwVertexProcessingCaps = 0;
    }
    d
}

unsafe extern "system" fn d_enum_devices(_this: This, cb: Option<LPD3DENUMDEVICESCALLBACK7>, ctx: *mut c_void) -> HRESULT {
    first_call!("IDirect3D7::EnumDevices");
    let Some(cb) = cb else { return DDERR_INVALIDPARAMS };
    let devices: [(&[u8], &[u8], GUID); 2] = [
        (b"Direct3D HAL (Vulkan)\0", b"Direct3D HAL\0", IID_IDIRECT3DHALDEVICE),
        (b"Direct3D T&L HAL (Vulkan)\0", b"Direct3D T&L HAL\0", IID_IDIRECT3DTNLHALDEVICE),
    ];
    for (desc, name, guid) in devices {
        let mut d = device_desc(guid);
        if cb(desc.as_ptr(), name.as_ptr(), &mut d, ctx) == D3DENUMRET_CANCEL {
            break;
        }
    }
    D3D_OK
}
const D3D_OK: HRESULT = 0;

unsafe extern "system" fn d_create_device(this: This, guid: *const GUID, rt: *mut Surface, out: *mut *mut Device) -> HRESULT {
    if guid.is_null() || rt.is_null() || out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let g = *guid;
    info!("IDirect3D7::CreateDevice({g}, rt={:?} {}x{})", (*rt).kind, (*rt).width(), (*rt).height());
    if g != IID_IDIRECT3DHALDEVICE && g != IID_IDIRECT3DTNLHALDEVICE && g != IID_IDIRECT3DRGBDEVICE && g != IID_IDIRECT3DREFDEVICE {
        info!("CreateDevice: unknown device GUID");
        return DDERR_INVALIDPARAMS;
    }
    *out = Device::create(this, g, rt);
    D3D_OK
}

unsafe extern "system" fn d_create_vertex_buffer(
    _this: This,
    desc: *mut D3DVERTEXBUFFERDESC,
    out: *mut *mut VertexBuffer,
    _flags: u32,
) -> HRESULT {
    if desc.is_null() || out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *out = VertexBuffer::create(&*desc);
    if (*out).is_null() {
        return D3DERR_VBUF_CREATE_FAILED;
    }
    D3D_OK
}

pub fn z_formats() -> Vec<DDPIXELFORMAT> {
    vec![
        pixel::pf_depth(16, 0xFFFF, 0, 0),
        pixel::pf_depth(24, 0xFF_FFFF, 0, 0),
        pixel::pf_depth(32, 0xFF_FFFF, 0, 0),
        pixel::pf_depth(32, 0xFF_FFFF, 8, 0xFF00_0000),
    ]
}

unsafe extern "system" fn d_enum_z_buffer_formats(
    _this: This,
    _dev: *const GUID,
    cb: Option<LPD3DENUMPIXELFORMATSCALLBACK>,
    ctx: *mut c_void,
) -> HRESULT {
    first_call!("IDirect3D7::EnumZBufferFormats");
    let Some(cb) = cb else { return DDERR_INVALIDPARAMS };
    for mut pf in z_formats() {
        if cb(&mut pf, ctx) == D3DENUMRET_CANCEL {
            break;
        }
    }
    D3D_OK
}

unsafe extern "system" fn d_evict_managed_textures(_this: This) -> HRESULT {
    D3D_OK
}
