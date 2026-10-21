//! DirectDraw 7 / Direct3D 7 ABI: structures, constants and HRESULTs, laid out
//! exactly as in the DirectX 7 SDK headers (ddraw.h, d3d.h, d3dtypes.h,
//! d3dcaps.h). All structures are `#[repr(C)]` with 4-byte packing on x86.
#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::ffi::c_void;

pub type HRESULT = i32;
pub type DWORD = u32;
pub type BOOL = i32;

// ---------------------------------------------------------------- HRESULTs
pub const DD_OK: HRESULT = 0;
pub const S_OK: HRESULT = 0;
pub const S_FALSE: HRESULT = 1;
pub const DDENUMRET_CANCEL: HRESULT = 0;
pub const DDENUMRET_OK: HRESULT = 1;
pub const D3DENUMRET_CANCEL: HRESULT = 0;
pub const D3DENUMRET_OK: HRESULT = 1;

const fn hr(v: u32) -> HRESULT {
    v as HRESULT
}
const fn dderr(code: u32) -> HRESULT {
    hr(0x8876_0000 | code)
}
pub const E_NOTIMPL: HRESULT = hr(0x8000_4001);
pub const E_NOINTERFACE: HRESULT = hr(0x8000_4002);
pub const E_POINTER: HRESULT = hr(0x8000_4003);
pub const E_FAIL: HRESULT = hr(0x8000_4005);
pub const E_OUTOFMEMORY: HRESULT = hr(0x8007_000E);
pub const E_INVALIDARG: HRESULT = hr(0x8007_0057);

pub const DDERR_GENERIC: HRESULT = E_FAIL;
pub const DDERR_UNSUPPORTED: HRESULT = E_NOTIMPL;
pub const DDERR_INVALIDPARAMS: HRESULT = E_INVALIDARG;
pub const DDERR_OUTOFMEMORY: HRESULT = E_OUTOFMEMORY;
pub const DDERR_INVALIDOBJECT: HRESULT = dderr(130);
pub const DDERR_INVALIDPIXELFORMAT: HRESULT = dderr(145);
pub const DDERR_INVALIDRECT: HRESULT = dderr(150);
pub const DDERR_INVALIDCAPS: HRESULT = dderr(100);
pub const DDERR_INVALIDMODE: HRESULT = dderr(140);
pub const DDERR_NOCLIPPERATTACHED: HRESULT = dderr(205);
pub const DDERR_NOCOLORKEY: HRESULT = dderr(215);
pub const DDERR_NOTFOUND: HRESULT = dderr(255);
pub const DDERR_SURFACEALREADYATTACHED: HRESULT = dderr(410);
pub const DDERR_SURFACEBUSY: HRESULT = dderr(430);
pub const DDERR_SURFACELOST: HRESULT = dderr(450);
pub const DDERR_SURFACENOTATTACHED: HRESULT = dderr(460);
pub const DDERR_CANNOTATTACHSURFACE: HRESULT = dderr(232);
pub const DDERR_NOTFLIPPABLE: HRESULT = dderr(582);
pub const DDERR_NOTLOCKED: HRESULT = dderr(584);
pub const DDERR_NOPALETTEATTACHED: HRESULT = dderr(561);
pub const DDERR_NOTPALETTIZED: HRESULT = dderr(589);
pub const DDERR_NODC: HRESULT = dderr(586);
pub const DDERR_WASSTILLDRAWING: HRESULT = dderr(540);
pub const DDERR_NOEXCLUSIVEMODE: HRESULT = dderr(225);
pub const DDERR_EXCLUSIVEMODEALREADYSET: HRESULT = dderr(581);
pub const DDERR_PRIMARYSURFACEALREADYEXISTS: HRESULT = dderr(564);
pub const DDERR_NOZBUFFERHW: HRESULT = dderr(280);
pub const DDERR_CANTCREATEDC: HRESULT = dderr(589 + 0); // close enough for logging
pub const D3DERR_INVALIDCALL: HRESULT = DDERR_INVALIDPARAMS;
pub const D3DERR_NOTINBEGIN: HRESULT = dderr(720);
pub const D3DERR_INBEGIN: HRESULT = dderr(721);
pub const D3DERR_SCENE_IN_SCENE: HRESULT = dderr(732);
pub const D3DERR_SCENE_NOT_IN_SCENE: HRESULT = dderr(733);
pub const D3DERR_SCENE_END_FAILED: HRESULT = dderr(734);
pub const D3DERR_LIGHT_SET_FAILED: HRESULT = dderr(750);
pub const D3DERR_INVALIDVERTEXTYPE: HRESULT = dderr(723);
pub const D3DERR_VBUF_CREATE_FAILED: HRESULT = dderr(731);
pub const D3DERR_VERTEXBUFFERLOCKED: HRESULT = dderr(737);

// ---------------------------------------------------------------- basic structs
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
impl RECT {
    pub fn new(l: i32, t: i32, r: i32, b: i32) -> Self {
        RECT { left: l, top: t, right: r, bottom: b }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    pub fn intersect(&self, o: &RECT) -> RECT {
        RECT {
            left: self.left.max(o.left),
            top: self.top.max(o.top),
            right: self.right.min(o.right),
            bottom: self.bottom.min(o.bottom),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}
impl GUID {
    pub const fn new(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Self {
        GUID { data1: d1, data2: d2, data3: d3, data4: d4 }
    }
}
impl std::fmt::Display for GUID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7]
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct DDCOLORKEY {
    pub low: u32,
    pub high: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct DDPIXELFORMAT {
    pub dwSize: u32,
    pub dwFlags: u32,
    pub dwFourCC: u32,
    /// dwRGBBitCount / dwYUVBitCount / dwZBufferBitDepth / dwAlphaBitDepth /
    /// dwLuminanceBitCount / dwBumpBitCount
    pub dwRGBBitCount: u32,
    /// dwRBitMask / dwYBitMask / dwStencilBitDepth / dwLuminanceBitMask / dwBumpDuBitMask
    pub dwRBitMask: u32,
    /// dwGBitMask / dwUBitMask / dwZBitMask / dwBumpDvBitMask
    pub dwGBitMask: u32,
    /// dwBBitMask / dwVBitMask / dwStencilBitMask / dwBumpLuminanceBitMask
    pub dwBBitMask: u32,
    /// dwRGBAlphaBitMask / dwYUVAlphaBitMask / dwLuminanceAlphaBitMask / dwRGBZBitMask
    pub dwRGBAlphaBitMask: u32,
}
const _: () = assert!(std::mem::size_of::<DDPIXELFORMAT>() == 32);

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct DDSCAPS2 {
    pub dwCaps: u32,
    pub dwCaps2: u32,
    pub dwCaps3: u32,
    pub dwCaps4: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DDSURFACEDESC2 {
    pub dwSize: u32,
    pub dwFlags: u32,
    pub dwHeight: u32,
    pub dwWidth: u32,
    /// lPitch / dwLinearSize
    pub lPitch: i32,
    /// dwBackBufferCount / dwDepth
    pub dwBackBufferCount: u32,
    /// dwMipMapCount / dwRefreshRate / dwSrcVBHandle
    pub dwMipMapCount: u32,
    pub dwAlphaBitDepth: u32,
    pub dwReserved: u32,
    pub lpSurface: *mut c_void,
    /// ddckCKDestOverlay / dwEmptyFaceColor
    pub ddckCKDestOverlay: DDCOLORKEY,
    pub ddckCKDestBlt: DDCOLORKEY,
    pub ddckCKSrcOverlay: DDCOLORKEY,
    pub ddckCKSrcBlt: DDCOLORKEY,
    /// ddpfPixelFormat / dwFVF
    pub ddpfPixelFormat: DDPIXELFORMAT,
    pub ddsCaps: DDSCAPS2,
    pub dwTextureStage: u32,
}
const _: () = assert!(std::mem::size_of::<DDSURFACEDESC2>() == 124);
impl Default for DDSURFACEDESC2 {
    fn default() -> Self {
        // SAFETY: plain old data
        let mut d: Self = unsafe { std::mem::zeroed() };
        d.dwSize = std::mem::size_of::<Self>() as u32;
        d
    }
}

/// Legacy DirectX 1-6 surface description (used by EnumDisplayModes callbacks
/// of old interfaces and by some apps). 108 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DDSURFACEDESC {
    pub dwSize: u32,
    pub dwFlags: u32,
    pub dwHeight: u32,
    pub dwWidth: u32,
    pub lPitch: i32,
    pub dwBackBufferCount: u32,
    pub dwMipMapCount: u32,
    pub dwAlphaBitDepth: u32,
    pub dwReserved: u32,
    pub lpSurface: *mut c_void,
    pub ddckCKDestOverlay: DDCOLORKEY,
    pub ddckCKDestBlt: DDCOLORKEY,
    pub ddckCKSrcOverlay: DDCOLORKEY,
    pub ddckCKSrcBlt: DDCOLORKEY,
    pub ddpfPixelFormat: DDPIXELFORMAT,
    pub ddsCaps: u32,
}
const _: () = assert!(std::mem::size_of::<DDSURFACEDESC>() == 108);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DDBLTFX {
    pub dwSize: u32,
    pub dwDDFX: u32,
    pub dwROP: u32,
    pub dwDDROP: u32,
    pub dwRotationAngle: u32,
    pub dwZBufferOpCode: u32,
    pub dwZBufferLow: u32,
    pub dwZBufferHigh: u32,
    pub dwZBufferBaseDest: u32,
    pub dwZDestConstBitDepth: u32,
    pub dwZDestConst: u32,
    pub dwZSrcConstBitDepth: u32,
    pub dwZSrcConst: u32,
    pub dwAlphaEdgeBlendBitDepth: u32,
    pub dwAlphaEdgeBlend: u32,
    pub dwReserved: u32,
    pub dwAlphaDestConstBitDepth: u32,
    pub dwAlphaDestConst: u32,
    pub dwAlphaSrcConstBitDepth: u32,
    pub dwAlphaSrcConst: u32,
    /// dwFillColor / dwFillDepth / dwFillPixel / lpDDSPattern
    pub dwFillColor: u32,
    pub ddckDestColorkey: DDCOLORKEY,
    pub ddckSrcColorkey: DDCOLORKEY,
}
const _: () = assert!(std::mem::size_of::<DDBLTFX>() == 100);

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PALETTEENTRY {
    pub peRed: u8,
    pub peGreen: u8,
    pub peBlue: u8,
    pub peFlags: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DDGAMMARAMP {
    pub red: [u16; 256],
    pub green: [u16; 256],
    pub blue: [u16; 256],
}
impl DDGAMMARAMP {
    pub fn identity() -> Self {
        let mut r = DDGAMMARAMP { red: [0; 256], green: [0; 256], blue: [0; 256] };
        for i in 0..256 {
            let v = (i as u16) * 257;
            r.red[i] = v;
            r.green[i] = v;
            r.blue[i] = v;
        }
        r
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DDDEVICEIDENTIFIER2 {
    pub szDriver: [u8; 512],
    pub szDescription: [u8; 512],
    pub liDriverVersion: u64,
    pub dwVendorId: u32,
    pub dwDeviceId: u32,
    pub dwSubSysId: u32,
    pub dwRevision: u32,
    pub guidDeviceIdentifier: GUID,
    pub dwWHQLLevel: u32,
}

/// DDCAPS as defined for DirectX 7 (380 bytes). Accessed as DWORD slots.
pub const DDCAPS_DX7_SIZE: usize = 380;
pub mod ddcaps_off {
    pub const SIZE: usize = 0;
    pub const CAPS: usize = 4;
    pub const CAPS2: usize = 8;
    pub const CKEYCAPS: usize = 12;
    pub const FXCAPS: usize = 16;
    pub const FXALPHACAPS: usize = 20;
    pub const PALCAPS: usize = 24;
    pub const SVCAPS: usize = 28;
    pub const ZBUFFERBITDEPTHS: usize = 56;
    pub const VIDMEMTOTAL: usize = 60;
    pub const VIDMEMFREE: usize = 64;
    pub const NUMFOURCC: usize = 76;
    pub const OLDCAPS: usize = 132;
    pub const SVBCAPS: usize = 172;
    pub const VSBCAPS: usize = 216;
    pub const SSBCAPS: usize = 260;
    pub const DDSCAPS: usize = 364;
}

// ---------------------------------------------------------------- DDSD flags
pub const DDSD_CAPS: u32 = 0x1;
pub const DDSD_HEIGHT: u32 = 0x2;
pub const DDSD_WIDTH: u32 = 0x4;
pub const DDSD_PITCH: u32 = 0x8;
pub const DDSD_BACKBUFFERCOUNT: u32 = 0x20;
pub const DDSD_ZBUFFERBITDEPTH: u32 = 0x40;
pub const DDSD_ALPHABITDEPTH: u32 = 0x80;
pub const DDSD_LPSURFACE: u32 = 0x800;
pub const DDSD_PIXELFORMAT: u32 = 0x1000;
pub const DDSD_CKDESTOVERLAY: u32 = 0x2000;
pub const DDSD_CKDESTBLT: u32 = 0x4000;
pub const DDSD_CKSRCOVERLAY: u32 = 0x8000;
pub const DDSD_CKSRCBLT: u32 = 0x10000;
pub const DDSD_MIPMAPCOUNT: u32 = 0x20000;
pub const DDSD_REFRESHRATE: u32 = 0x40000;
pub const DDSD_LINEARSIZE: u32 = 0x80000;
pub const DDSD_TEXTURESTAGE: u32 = 0x100000;
pub const DDSD_FVF: u32 = 0x200000;
pub const DDSD_SRCVBHANDLE: u32 = 0x400000;
pub const DDSD_DEPTH: u32 = 0x800000;

// ---------------------------------------------------------------- DDSCAPS
pub const DDSCAPS_ALPHA: u32 = 0x2;
pub const DDSCAPS_BACKBUFFER: u32 = 0x4;
pub const DDSCAPS_COMPLEX: u32 = 0x8;
pub const DDSCAPS_FLIP: u32 = 0x10;
pub const DDSCAPS_FRONTBUFFER: u32 = 0x20;
pub const DDSCAPS_OFFSCREENPLAIN: u32 = 0x40;
pub const DDSCAPS_OVERLAY: u32 = 0x80;
pub const DDSCAPS_PALETTE: u32 = 0x100;
pub const DDSCAPS_PRIMARYSURFACE: u32 = 0x200;
pub const DDSCAPS_SYSTEMMEMORY: u32 = 0x800;
pub const DDSCAPS_TEXTURE: u32 = 0x1000;
pub const DDSCAPS_3DDEVICE: u32 = 0x2000;
pub const DDSCAPS_VIDEOMEMORY: u32 = 0x4000;
pub const DDSCAPS_VISIBLE: u32 = 0x8000;
pub const DDSCAPS_WRITEONLY: u32 = 0x10000;
pub const DDSCAPS_ZBUFFER: u32 = 0x20000;
pub const DDSCAPS_OWNDC: u32 = 0x40000;
pub const DDSCAPS_MIPMAP: u32 = 0x400000;
pub const DDSCAPS_ALLOCONLOAD: u32 = 0x4000000;
pub const DDSCAPS_LOCALVIDMEM: u32 = 0x10000000;
pub const DDSCAPS_NONLOCALVIDMEM: u32 = 0x20000000;

pub const DDSCAPS2_TEXTUREMANAGE: u32 = 0x10;
pub const DDSCAPS2_OPAQUE: u32 = 0x80;
pub const DDSCAPS2_HINTANTIALIASING: u32 = 0x100;
pub const DDSCAPS2_CUBEMAP: u32 = 0x200;
pub const DDSCAPS2_MIPMAPSUBLEVEL: u32 = 0x10000;
pub const DDSCAPS2_D3DTEXTUREMANAGE: u32 = 0x20000;

// ---------------------------------------------------------------- DDPF
pub const DDPF_ALPHAPIXELS: u32 = 0x1;
pub const DDPF_ALPHA: u32 = 0x2;
pub const DDPF_FOURCC: u32 = 0x4;
pub const DDPF_PALETTEINDEXED4: u32 = 0x8;
pub const DDPF_PALETTEINDEXEDTO8: u32 = 0x10;
pub const DDPF_PALETTEINDEXED8: u32 = 0x20;
pub const DDPF_RGB: u32 = 0x40;
pub const DDPF_COMPRESSED: u32 = 0x80;
pub const DDPF_ZBUFFER: u32 = 0x400;
pub const DDPF_PALETTEINDEXED1: u32 = 0x800;
pub const DDPF_PALETTEINDEXED2: u32 = 0x1000;
pub const DDPF_ZPIXELS: u32 = 0x2000;
pub const DDPF_STENCILBUFFER: u32 = 0x4000;
pub const DDPF_ALPHAPREMULT: u32 = 0x8000;
pub const DDPF_LUMINANCE: u32 = 0x20000;
pub const DDPF_BUMPLUMINANCE: u32 = 0x40000;
pub const DDPF_BUMPDUDV: u32 = 0x80000;

pub const fn fourcc(s: &[u8; 4]) -> u32 {
    (s[0] as u32) | ((s[1] as u32) << 8) | ((s[2] as u32) << 16) | ((s[3] as u32) << 24)
}
pub const FOURCC_DXT1: u32 = fourcc(b"DXT1");
pub const FOURCC_DXT2: u32 = fourcc(b"DXT2");
pub const FOURCC_DXT3: u32 = fourcc(b"DXT3");
pub const FOURCC_DXT4: u32 = fourcc(b"DXT4");
pub const FOURCC_DXT5: u32 = fourcc(b"DXT5");

// ---------------------------------------------------------------- misc DD flags
pub const DDSCL_FULLSCREEN: u32 = 0x1;
pub const DDSCL_ALLOWREBOOT: u32 = 0x2;
pub const DDSCL_NOWINDOWCHANGES: u32 = 0x4;
pub const DDSCL_NORMAL: u32 = 0x8;
pub const DDSCL_EXCLUSIVE: u32 = 0x10;
pub const DDSCL_ALLOWMODEX: u32 = 0x40;
pub const DDSCL_SETFOCUSWINDOW: u32 = 0x80;
pub const DDSCL_SETDEVICEWINDOW: u32 = 0x100;
pub const DDSCL_CREATEDEVICEWINDOW: u32 = 0x200;
pub const DDSCL_MULTITHREADED: u32 = 0x400;
pub const DDSCL_FPUSETUP: u32 = 0x800;
pub const DDSCL_FPUPRESERVE: u32 = 0x1000;

pub const DDLOCK_WAIT: u32 = 0x1;
pub const DDLOCK_READONLY: u32 = 0x10;
pub const DDLOCK_WRITEONLY: u32 = 0x20;
pub const DDLOCK_NOSYSLOCK: u32 = 0x800;
pub const DDLOCK_NOOVERWRITE: u32 = 0x1000;
pub const DDLOCK_DISCARDCONTENTS: u32 = 0x2000;
pub const DDLOCK_DONOTWAIT: u32 = 0x4000;

pub const DDBLT_ALPHADEST: u32 = 0x1;
pub const DDBLT_ALPHASRC: u32 = 0x100;
pub const DDBLT_ASYNC: u32 = 0x200;
pub const DDBLT_COLORFILL: u32 = 0x400;
pub const DDBLT_DDFX: u32 = 0x800;
pub const DDBLT_DDROPS: u32 = 0x1000;
pub const DDBLT_KEYDEST: u32 = 0x2000;
pub const DDBLT_KEYDESTOVERRIDE: u32 = 0x4000;
pub const DDBLT_KEYSRC: u32 = 0x8000;
pub const DDBLT_KEYSRCOVERRIDE: u32 = 0x10000;
pub const DDBLT_ROP: u32 = 0x20000;
pub const DDBLT_ROTATIONANGLE: u32 = 0x40000;
pub const DDBLT_ZBUFFER: u32 = 0x80000;
pub const DDBLT_WAIT: u32 = 0x1000000;
pub const DDBLT_DEPTHFILL: u32 = 0x2000000;
pub const DDBLT_DONOTWAIT: u32 = 0x8000000;

pub const DDBLTFX_MIRRORLEFTRIGHT: u32 = 0x2;
pub const DDBLTFX_MIRRORUPDOWN: u32 = 0x4;

pub const DDBLTFAST_NOCOLORKEY: u32 = 0x0;
pub const DDBLTFAST_SRCCOLORKEY: u32 = 0x1;
pub const DDBLTFAST_DESTCOLORKEY: u32 = 0x2;

pub const DDCKEY_COLORSPACE: u32 = 0x1;
pub const DDCKEY_DESTBLT: u32 = 0x2;
pub const DDCKEY_DESTOVERLAY: u32 = 0x4;
pub const DDCKEY_SRCBLT: u32 = 0x8;
pub const DDCKEY_SRCOVERLAY: u32 = 0x10;

pub const DDFLIP_WAIT: u32 = 0x1;
pub const DDFLIP_NOVSYNC: u32 = 0x8;
pub const DDFLIP_DONOTWAIT: u32 = 0x20;

pub const DDPCAPS_4BIT: u32 = 0x1;
pub const DDPCAPS_8BITENTRIES: u32 = 0x2;
pub const DDPCAPS_8BIT: u32 = 0x4;
pub const DDPCAPS_INITIALIZE: u32 = 0x8;
pub const DDPCAPS_PRIMARYSURFACE: u32 = 0x10;
pub const DDPCAPS_ALLOW256: u32 = 0x40;
pub const DDPCAPS_1BIT: u32 = 0x100;
pub const DDPCAPS_2BIT: u32 = 0x200;
pub const DDPCAPS_ALPHA: u32 = 0x400;

pub const DDEDM_REFRESHRATES: u32 = 0x1;
pub const DDEDM_STANDARDVGAMODES: u32 = 0x2;

pub const DDWAITVB_BLOCKBEGIN: u32 = 0x1;

// DDCAPS bits
pub const DDCAPS_3D: u32 = 0x1;
pub const DDCAPS_BLT: u32 = 0x40;
pub const DDCAPS_BLTQUEUE: u32 = 0x80;
pub const DDCAPS_BLTFOURCC: u32 = 0x100;
pub const DDCAPS_BLTSTRETCH: u32 = 0x200;
pub const DDCAPS_GDI: u32 = 0x400;
pub const DDCAPS_COLORKEY: u32 = 0x10000;
pub const DDCAPS_PALETTE: u32 = 0x8000000;
pub const DDCAPS_ZBLTS: u32 = 0x10000000;
pub const DDCAPS_BLTCOLORFILL: u32 = 0x4000000;
pub const DDCAPS_BLTDEPTHFILL: u32 = 0x40000000;
pub const DDCAPS_CANBLTSYSMEM: u32 = 0x80000000;
pub const DDCAPS2_CERTIFIED: u32 = 0x1;
pub const DDCAPS2_WIDESURFACES: u32 = 0x1000;
pub const DDCAPS2_COPYFOURCC: u32 = 0x8000;
pub const DDCAPS2_PRIMARYGAMMA: u32 = 0x20000;
pub const DDCAPS2_CANRENDERWINDOWED: u32 = 0x80000;
pub const DDCAPS2_FLIPNOVSYNC: u32 = 0x2000000;
pub const DDCAPS2_NONLOCALVIDMEM: u32 = 0x10;
pub const DDCAPS2_NOPAGELOCKREQUIRED: u32 = 0x40000;
pub const DDCKEYCAPS_SRCBLT: u32 = 0x200;
pub const DDCKEYCAPS_DESTBLT: u32 = 0x2;

pub const DDBD_8: u32 = 0x800;
pub const DDBD_16: u32 = 0x400;
pub const DDBD_24: u32 = 0x200;
pub const DDBD_32: u32 = 0x100;

pub const DDGDI_GETHOSTIDENTIFIER: u32 = 1;

// ---------------------------------------------------------------- Direct3D 7
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct D3DCOLORVALUE {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct D3DVECTOR {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct D3DMATRIX {
    pub m: [[f32; 4]; 4],
}
impl Default for D3DMATRIX {
    fn default() -> Self {
        Self::identity()
    }
}
impl D3DMATRIX {
    pub const fn identity() -> Self {
        D3DMATRIX {
            m: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
        }
    }
    /// Row-vector convention (D3D): result = self * o.
    pub fn mul(&self, o: &D3DMATRIX) -> D3DMATRIX {
        let mut r = [[0f32; 4]; 4];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..4).map(|k| self.m[i][k] * o.m[k][j]).sum();
            }
        }
        D3DMATRIX { m: r }
    }
    pub fn transpose(&self) -> D3DMATRIX {
        let mut r = [[0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = self.m[j][i];
            }
        }
        D3DMATRIX { m: r }
    }
    /// General 4x4 inverse; returns identity if singular.
    pub fn inverse(&self) -> D3DMATRIX {
        let m = &self.m;
        let a = [
            m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1], m[2][2],
            m[2][3], m[3][0], m[3][1], m[3][2], m[3][3],
        ];
        let mut inv = [0f32; 16];
        inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15] + a[9] * a[7] * a[14]
            + a[13] * a[6] * a[11]
            - a[13] * a[7] * a[10];
        inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15] - a[8] * a[7] * a[14]
            - a[12] * a[6] * a[11]
            + a[12] * a[7] * a[10];
        inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15] + a[8] * a[7] * a[13]
            + a[12] * a[5] * a[11]
            - a[12] * a[7] * a[9];
        inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14] - a[8] * a[6] * a[13]
            - a[12] * a[5] * a[10]
            + a[12] * a[6] * a[9];
        inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15] - a[9] * a[3] * a[14]
            - a[13] * a[2] * a[11]
            + a[13] * a[3] * a[10];
        inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15] + a[8] * a[3] * a[14]
            + a[12] * a[2] * a[11]
            - a[12] * a[3] * a[10];
        inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15] - a[8] * a[3] * a[13]
            - a[12] * a[1] * a[11]
            + a[12] * a[3] * a[9];
        inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14] + a[8] * a[2] * a[13]
            + a[12] * a[1] * a[10]
            - a[12] * a[2] * a[9];
        inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15] + a[5] * a[3] * a[14]
            + a[13] * a[2] * a[7]
            - a[13] * a[3] * a[6];
        inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15] - a[4] * a[3] * a[14]
            - a[12] * a[2] * a[7]
            + a[12] * a[3] * a[6];
        inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15] + a[4] * a[3] * a[13]
            + a[12] * a[1] * a[7]
            - a[12] * a[3] * a[5];
        inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14] - a[4] * a[2] * a[13]
            - a[12] * a[1] * a[6]
            + a[12] * a[2] * a[5];
        inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11] - a[5] * a[3] * a[10]
            - a[9] * a[2] * a[7]
            + a[9] * a[3] * a[6];
        inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11] + a[4] * a[3] * a[10]
            + a[8] * a[2] * a[7]
            - a[8] * a[3] * a[6];
        inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11] - a[4] * a[3] * a[9]
            - a[8] * a[1] * a[7]
            + a[8] * a[3] * a[5];
        inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10] + a[4] * a[2] * a[9]
            + a[8] * a[1] * a[6]
            - a[8] * a[2] * a[5];
        let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
        if det.abs() < 1e-30 {
            return D3DMATRIX::identity();
        }
        let id = 1.0 / det;
        let mut r = [[0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = inv[i * 4 + j] * id;
            }
        }
        D3DMATRIX { m: r }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct D3DVIEWPORT7 {
    pub dwX: u32,
    pub dwY: u32,
    pub dwWidth: u32,
    pub dwHeight: u32,
    pub dvMinZ: f32,
    pub dvMaxZ: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct D3DMATERIAL7 {
    pub diffuse: D3DCOLORVALUE,
    pub ambient: D3DCOLORVALUE,
    pub specular: D3DCOLORVALUE,
    pub emissive: D3DCOLORVALUE,
    pub power: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct D3DLIGHT7 {
    pub dltType: u32,
    pub dcvDiffuse: D3DCOLORVALUE,
    pub dcvSpecular: D3DCOLORVALUE,
    pub dcvAmbient: D3DCOLORVALUE,
    pub dvPosition: D3DVECTOR,
    pub dvDirection: D3DVECTOR,
    pub dvRange: f32,
    pub dvFalloff: f32,
    pub dvAttenuation0: f32,
    pub dvAttenuation1: f32,
    pub dvAttenuation2: f32,
    pub dvTheta: f32,
    pub dvPhi: f32,
}
const _: () = assert!(std::mem::size_of::<D3DLIGHT7>() == 104);
pub const D3DLIGHT_POINT: u32 = 1;
pub const D3DLIGHT_SPOT: u32 = 2;
pub const D3DLIGHT_DIRECTIONAL: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct D3DRECT {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct D3DCLIPSTATUS {
    pub dwFlags: u32,
    pub dwStatus: u32,
    pub minx: f32,
    pub maxx: f32,
    pub miny: f32,
    pub maxy: f32,
    pub minz: f32,
    pub maxz: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct D3DPRIMCAPS {
    pub dwSize: u32,
    pub dwMiscCaps: u32,
    pub dwRasterCaps: u32,
    pub dwZCmpCaps: u32,
    pub dwSrcBlendCaps: u32,
    pub dwDestBlendCaps: u32,
    pub dwAlphaCmpCaps: u32,
    pub dwShadeCaps: u32,
    pub dwTextureCaps: u32,
    pub dwTextureFilterCaps: u32,
    pub dwTextureBlendCaps: u32,
    pub dwTextureAddressCaps: u32,
    pub dwStippleWidth: u32,
    pub dwStippleHeight: u32,
}
const _: () = assert!(std::mem::size_of::<D3DPRIMCAPS>() == 56);

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct D3DDEVICEDESC7 {
    pub dwDevCaps: u32,
    pub dpcLineCaps: D3DPRIMCAPS,
    pub dpcTriCaps: D3DPRIMCAPS,
    pub dwDeviceRenderBitDepth: u32,
    pub dwDeviceZBufferBitDepth: u32,
    pub dwMinTextureWidth: u32,
    pub dwMinTextureHeight: u32,
    pub dwMaxTextureWidth: u32,
    pub dwMaxTextureHeight: u32,
    pub dwMaxTextureRepeat: u32,
    pub dwMaxTextureAspectRatio: u32,
    pub dwMaxAnisotropy: u32,
    pub dvGuardBandLeft: f32,
    pub dvGuardBandTop: f32,
    pub dvGuardBandRight: f32,
    pub dvGuardBandBottom: f32,
    pub dvExtentsAdjust: f32,
    pub dwStencilCaps: u32,
    pub dwFVFCaps: u32,
    pub dwTextureOpCaps: u32,
    pub wMaxTextureBlendStages: u16,
    pub wMaxSimultaneousTextures: u16,
    pub dwMaxActiveLights: u32,
    pub dvMaxVertexW: f32,
    pub deviceGUID: GUID,
    pub wMaxUserClipPlanes: u16,
    pub wMaxVertexBlendMatrices: u16,
    pub dwVertexProcessingCaps: u32,
    pub dwReserved1: u32,
    pub dwReserved2: u32,
    pub dwReserved3: u32,
    pub dwReserved4: u32,
}
const _: () = assert!(std::mem::size_of::<D3DDEVICEDESC7>() == 236);

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct D3DVERTEXBUFFERDESC {
    pub dwSize: u32,
    pub dwCaps: u32,
    pub dwFVF: u32,
    pub dwNumVertices: u32,
}
pub const D3DVBCAPS_SYSTEMMEMORY: u32 = 0x800;
pub const D3DVBCAPS_WRITEONLY: u32 = 0x10000;
pub const D3DVBCAPS_OPTIMIZED: u32 = 0x80000000;
pub const D3DVBCAPS_DONOTCLIP: u32 = 0x1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct D3DDP_PTRSTRIDE {
    pub lpvData: *const u8,
    pub dwStride: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct D3DDRAWPRIMITIVESTRIDEDDATA {
    pub position: D3DDP_PTRSTRIDE,
    pub normal: D3DDP_PTRSTRIDE,
    pub diffuse: D3DDP_PTRSTRIDE,
    pub specular: D3DDP_PTRSTRIDE,
    pub textureCoords: [D3DDP_PTRSTRIDE; 8],
}

// Devcaps
pub const D3DDEVCAPS_FLOATTLVERTEX: u32 = 0x1;
pub const D3DDEVCAPS_EXECUTESYSTEMMEMORY: u32 = 0x10;
pub const D3DDEVCAPS_EXECUTEVIDEOMEMORY: u32 = 0x20;
pub const D3DDEVCAPS_TLVERTEXSYSTEMMEMORY: u32 = 0x40;
pub const D3DDEVCAPS_TLVERTEXVIDEOMEMORY: u32 = 0x80;
pub const D3DDEVCAPS_TEXTURESYSTEMMEMORY: u32 = 0x100;
pub const D3DDEVCAPS_TEXTUREVIDEOMEMORY: u32 = 0x200;
pub const D3DDEVCAPS_DRAWPRIMTLVERTEX: u32 = 0x400;
pub const D3DDEVCAPS_CANRENDERAFTERFLIP: u32 = 0x800;
pub const D3DDEVCAPS_TEXTURENONLOCALVIDMEM: u32 = 0x1000;
pub const D3DDEVCAPS_DRAWPRIMITIVES2: u32 = 0x2000;
pub const D3DDEVCAPS_SEPARATETEXTUREMEMORIES: u32 = 0x4000;
pub const D3DDEVCAPS_DRAWPRIMITIVES2EX: u32 = 0x8000;
pub const D3DDEVCAPS_HWTRANSFORMANDLIGHT: u32 = 0x10000;
pub const D3DDEVCAPS_CANBLTSYSTONONLOCAL: u32 = 0x20000;
pub const D3DDEVCAPS_HWRASTERIZATION: u32 = 0x80000;

// Primitive types
pub const D3DPT_POINTLIST: u32 = 1;
pub const D3DPT_LINELIST: u32 = 2;
pub const D3DPT_LINESTRIP: u32 = 3;
pub const D3DPT_TRIANGLELIST: u32 = 4;
pub const D3DPT_TRIANGLESTRIP: u32 = 5;
pub const D3DPT_TRIANGLEFAN: u32 = 6;

// FVF
pub const D3DFVF_POSITION_MASK: u32 = 0xE;
pub const D3DFVF_XYZ: u32 = 0x2;
pub const D3DFVF_XYZRHW: u32 = 0x4;
pub const D3DFVF_XYZB1: u32 = 0x6;
pub const D3DFVF_XYZB5: u32 = 0xE;
pub const D3DFVF_NORMAL: u32 = 0x10;
pub const D3DFVF_RESERVED1: u32 = 0x20;
pub const D3DFVF_DIFFUSE: u32 = 0x40;
pub const D3DFVF_SPECULAR: u32 = 0x80;
pub const D3DFVF_TEXCOUNT_MASK: u32 = 0xF00;
pub const D3DFVF_TEXCOUNT_SHIFT: u32 = 8;

// Transform state types
pub const D3DTRANSFORMSTATE_WORLD: u32 = 1;
pub const D3DTRANSFORMSTATE_VIEW: u32 = 2;
pub const D3DTRANSFORMSTATE_PROJECTION: u32 = 3;
pub const D3DTRANSFORMSTATE_WORLD1: u32 = 4;
pub const D3DTRANSFORMSTATE_WORLD2: u32 = 5;
pub const D3DTRANSFORMSTATE_WORLD3: u32 = 6;
pub const D3DTRANSFORMSTATE_TEXTURE0: u32 = 16;
pub const D3DTRANSFORMSTATE_TEXTURE7: u32 = 23;

// Clear flags
pub const D3DCLEAR_TARGET: u32 = 1;
pub const D3DCLEAR_ZBUFFER: u32 = 2;
pub const D3DCLEAR_STENCIL: u32 = 4;

// Render states
pub mod rs {
    pub const ANTIALIAS: u32 = 2;
    pub const TEXTUREPERSPECTIVE: u32 = 4;
    pub const ZENABLE: u32 = 7;
    pub const FILLMODE: u32 = 8;
    pub const SHADEMODE: u32 = 9;
    pub const LINEPATTERN: u32 = 10;
    pub const ZWRITEENABLE: u32 = 14;
    pub const ALPHATESTENABLE: u32 = 15;
    pub const LASTPIXEL: u32 = 16;
    pub const SRCBLEND: u32 = 19;
    pub const DESTBLEND: u32 = 20;
    pub const CULLMODE: u32 = 22;
    pub const ZFUNC: u32 = 23;
    pub const ALPHAREF: u32 = 24;
    pub const ALPHAFUNC: u32 = 25;
    pub const DITHERENABLE: u32 = 26;
    pub const ALPHABLENDENABLE: u32 = 27;
    pub const FOGENABLE: u32 = 28;
    pub const SPECULARENABLE: u32 = 29;
    pub const ZVISIBLE: u32 = 30;
    pub const STIPPLEDALPHA: u32 = 33;
    pub const FOGCOLOR: u32 = 34;
    pub const FOGTABLEMODE: u32 = 35;
    pub const FOGSTART: u32 = 36;
    pub const FOGEND: u32 = 37;
    pub const FOGDENSITY: u32 = 38;
    pub const EDGEANTIALIAS: u32 = 40;
    pub const COLORKEYENABLE: u32 = 41;
    pub const ZBIAS: u32 = 47;
    pub const RANGEFOGENABLE: u32 = 48;
    pub const STENCILENABLE: u32 = 52;
    pub const STENCILFAIL: u32 = 53;
    pub const STENCILZFAIL: u32 = 54;
    pub const STENCILPASS: u32 = 55;
    pub const STENCILFUNC: u32 = 56;
    pub const STENCILREF: u32 = 57;
    pub const STENCILMASK: u32 = 58;
    pub const STENCILWRITEMASK: u32 = 59;
    pub const TEXTUREFACTOR: u32 = 60;
    pub const WRAP0: u32 = 128;
    pub const CLIPPING: u32 = 136;
    pub const LIGHTING: u32 = 137;
    pub const EXTENTS: u32 = 138;
    pub const AMBIENT: u32 = 139;
    pub const FOGVERTEXMODE: u32 = 140;
    pub const COLORVERTEX: u32 = 141;
    pub const LOCALVIEWER: u32 = 142;
    pub const NORMALIZENORMALS: u32 = 143;
    pub const COLORKEYBLENDENABLE: u32 = 144;
    pub const DIFFUSEMATERIALSOURCE: u32 = 145;
    pub const SPECULARMATERIALSOURCE: u32 = 146;
    pub const AMBIENTMATERIALSOURCE: u32 = 147;
    pub const EMISSIVEMATERIALSOURCE: u32 = 148;
    pub const VERTEXBLEND: u32 = 151;
    pub const CLIPPLANEENABLE: u32 = 152;
    pub const MAX: usize = 256;
}

// Texture stage states
pub mod tss {
    pub const COLOROP: u32 = 1;
    pub const COLORARG1: u32 = 2;
    pub const COLORARG2: u32 = 3;
    pub const ALPHAOP: u32 = 4;
    pub const ALPHAARG1: u32 = 5;
    pub const ALPHAARG2: u32 = 6;
    pub const BUMPENVMAT00: u32 = 7;
    pub const BUMPENVMAT01: u32 = 8;
    pub const BUMPENVMAT10: u32 = 9;
    pub const BUMPENVMAT11: u32 = 10;
    pub const TEXCOORDINDEX: u32 = 11;
    pub const ADDRESS: u32 = 12;
    pub const ADDRESSU: u32 = 13;
    pub const ADDRESSV: u32 = 14;
    pub const BORDERCOLOR: u32 = 15;
    pub const MAGFILTER: u32 = 16;
    pub const MINFILTER: u32 = 17;
    pub const MIPFILTER: u32 = 18;
    pub const MIPMAPLODBIAS: u32 = 19;
    pub const MAXMIPLEVEL: u32 = 20;
    pub const MAXANISOTROPY: u32 = 21;
    pub const BUMPENVLSCALE: u32 = 22;
    pub const BUMPENVLOFFSET: u32 = 23;
    pub const TEXTURETRANSFORMFLAGS: u32 = 24;
    pub const MAX: usize = 32;
}

pub mod top {
    pub const DISABLE: u32 = 1;
    pub const SELECTARG1: u32 = 2;
    pub const SELECTARG2: u32 = 3;
    pub const MODULATE: u32 = 4;
    pub const BUMPENVMAP: u32 = 22;
    pub const BUMPENVMAPLUMINANCE: u32 = 23;
}
pub const D3DTA_DIFFUSE: u32 = 0;
pub const D3DTA_CURRENT: u32 = 1;
pub const D3DTA_TEXTURE: u32 = 2;
pub const D3DTA_TFACTOR: u32 = 3;
pub const D3DTA_SPECULAR: u32 = 4;
pub const D3DTA_SELECTMASK: u32 = 0xF;

pub const D3DTADDRESS_WRAP: u32 = 1;
pub const D3DTADDRESS_MIRROR: u32 = 2;
pub const D3DTADDRESS_CLAMP: u32 = 3;
pub const D3DTADDRESS_BORDER: u32 = 4;

pub const D3DTFG_POINT: u32 = 1;
pub const D3DTFG_LINEAR: u32 = 2;
pub const D3DTFG_ANISOTROPIC: u32 = 5;
pub const D3DTFN_POINT: u32 = 1;
pub const D3DTFN_LINEAR: u32 = 2;
pub const D3DTFN_ANISOTROPIC: u32 = 3;
pub const D3DTFP_NONE: u32 = 1;
pub const D3DTFP_POINT: u32 = 2;
pub const D3DTFP_LINEAR: u32 = 3;

pub const D3DBLEND_ZERO: u32 = 1;
pub const D3DBLEND_ONE: u32 = 2;
pub const D3DBLEND_SRCCOLOR: u32 = 3;
pub const D3DBLEND_INVSRCCOLOR: u32 = 4;
pub const D3DBLEND_SRCALPHA: u32 = 5;
pub const D3DBLEND_INVSRCALPHA: u32 = 6;
pub const D3DBLEND_DESTALPHA: u32 = 7;
pub const D3DBLEND_INVDESTALPHA: u32 = 8;
pub const D3DBLEND_DESTCOLOR: u32 = 9;
pub const D3DBLEND_INVDESTCOLOR: u32 = 10;
pub const D3DBLEND_SRCALPHASAT: u32 = 11;
pub const D3DBLEND_BOTHSRCALPHA: u32 = 12;
pub const D3DBLEND_BOTHINVSRCALPHA: u32 = 13;

pub const D3DCMP_NEVER: u32 = 1;
pub const D3DCMP_LESS: u32 = 2;
pub const D3DCMP_EQUAL: u32 = 3;
pub const D3DCMP_LESSEQUAL: u32 = 4;
pub const D3DCMP_GREATER: u32 = 5;
pub const D3DCMP_NOTEQUAL: u32 = 6;
pub const D3DCMP_GREATEREQUAL: u32 = 7;
pub const D3DCMP_ALWAYS: u32 = 8;

pub const D3DCULL_NONE: u32 = 1;
pub const D3DCULL_CW: u32 = 2;
pub const D3DCULL_CCW: u32 = 3;

pub const D3DFILL_POINT: u32 = 1;
pub const D3DFILL_WIREFRAME: u32 = 2;
pub const D3DFILL_SOLID: u32 = 3;

pub const D3DSTENCILOP_KEEP: u32 = 1;
pub const D3DSTENCILOP_ZERO: u32 = 2;
pub const D3DSTENCILOP_REPLACE: u32 = 3;
pub const D3DSTENCILOP_INCRSAT: u32 = 4;
pub const D3DSTENCILOP_DECRSAT: u32 = 5;
pub const D3DSTENCILOP_INVERT: u32 = 6;
pub const D3DSTENCILOP_INCR: u32 = 7;
pub const D3DSTENCILOP_DECR: u32 = 8;

pub const D3DFOG_NONE: u32 = 0;
pub const D3DFOG_EXP: u32 = 1;
pub const D3DFOG_EXP2: u32 = 2;
pub const D3DFOG_LINEAR: u32 = 3;

pub const D3DZB_FALSE: u32 = 0;
pub const D3DZB_TRUE: u32 = 1;
pub const D3DZB_USEW: u32 = 2;

pub const D3DMCS_MATERIAL: u32 = 0;
pub const D3DMCS_COLOR1: u32 = 1;
pub const D3DMCS_COLOR2: u32 = 2;

pub const D3DTSS_TCI_PASSTHRU: u32 = 0x0;
pub const D3DTSS_TCI_CAMERASPACENORMAL: u32 = 0x10000;
pub const D3DTSS_TCI_CAMERASPACEPOSITION: u32 = 0x20000;
pub const D3DTSS_TCI_CAMERASPACEREFLECTIONVECTOR: u32 = 0x30000;
pub const D3DTTFF_DISABLE: u32 = 0;
pub const D3DTTFF_PROJECTED: u32 = 256;

pub const D3DPV_DONOTCOPYDATA: u32 = 1;

pub const D3DDP_WAIT: u32 = 1;
pub const D3DDP_DONOTCLIP: u32 = 4;
pub const D3DDP_DONOTUPDATEEXTENTS: u32 = 8;
pub const D3DDP_DONOTLIGHT: u32 = 16;

// Callback types
pub type LPDDENUMCALLBACKEXA =
    unsafe extern "system" fn(*mut GUID, *const u8, *const u8, *mut c_void, *mut c_void) -> BOOL;
pub type LPDDENUMCALLBACKA = unsafe extern "system" fn(*mut GUID, *const u8, *const u8, *mut c_void) -> BOOL;
pub type LPDDENUMMODESCALLBACK2 = unsafe extern "system" fn(*mut DDSURFACEDESC2, *mut c_void) -> HRESULT;
pub type LPDDENUMSURFACESCALLBACK7 =
    unsafe extern "system" fn(*mut c_void, *mut DDSURFACEDESC2, *mut c_void) -> HRESULT;
pub type LPD3DENUMDEVICESCALLBACK7 =
    unsafe extern "system" fn(*const u8, *const u8, *mut D3DDEVICEDESC7, *mut c_void) -> HRESULT;
pub type LPD3DENUMPIXELFORMATSCALLBACK = unsafe extern "system" fn(*mut DDPIXELFORMAT, *mut c_void) -> HRESULT;

// ---------------------------------------------------------------- IIDs / CLSIDs
pub const IID_IUNKNOWN: GUID = GUID::new(0x00000000, 0x0000, 0x0000, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IDIRECTDRAW: GUID =
    GUID::new(0x6C14DB80, 0xA733, 0x11CE, [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60]);
pub const IID_IDIRECTDRAW2: GUID =
    GUID::new(0xB3A6F3E0, 0x2B43, 0x11CF, [0xA2, 0xDE, 0x00, 0xAA, 0x00, 0xB9, 0x33, 0x56]);
pub const IID_IDIRECTDRAW4: GUID =
    GUID::new(0x9C59509A, 0x39BD, 0x11D1, [0x8C, 0x4A, 0x00, 0xC0, 0x4F, 0xD9, 0x30, 0xC5]);
pub const IID_IDIRECTDRAW7: GUID =
    GUID::new(0x15E65EC0, 0x3B9C, 0x11D2, [0xB9, 0x2F, 0x00, 0x60, 0x97, 0x97, 0xEA, 0x5B]);
pub const IID_IDIRECTDRAWSURFACE7: GUID =
    GUID::new(0x06675A80, 0x3B9B, 0x11D2, [0xB9, 0x2F, 0x00, 0x60, 0x97, 0x97, 0xEA, 0x5B]);
pub const IID_IDIRECTDRAWPALETTE: GUID =
    GUID::new(0x6C14DB84, 0xA733, 0x11CE, [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60]);
pub const IID_IDIRECTDRAWCLIPPER: GUID =
    GUID::new(0x6C14DB85, 0xA733, 0x11CE, [0xA5, 0x21, 0x00, 0x20, 0xAF, 0x0B, 0xE5, 0x60]);
pub const IID_IDIRECTDRAWGAMMACONTROL: GUID =
    GUID::new(0x69C11C3E, 0xB46B, 0x11D1, [0xAD, 0x7A, 0x00, 0xC0, 0x4F, 0xC2, 0x9B, 0x4E]);
pub const IID_IDIRECT3D7: GUID =
    GUID::new(0xF5049E77, 0x4861, 0x11D2, [0xA4, 0x07, 0x00, 0xA0, 0xC9, 0x06, 0x29, 0xA8]);
pub const IID_IDIRECT3DDEVICE7: GUID =
    GUID::new(0xF5049E79, 0x4861, 0x11D2, [0xA4, 0x07, 0x00, 0xA0, 0xC9, 0x06, 0x29, 0xA8]);
pub const IID_IDIRECT3DVERTEXBUFFER7: GUID =
    GUID::new(0xF5049E7D, 0x4861, 0x11D2, [0xA4, 0x07, 0x00, 0xA0, 0xC9, 0x06, 0x29, 0xA8]);
pub const IID_IDIRECT3DHALDEVICE: GUID =
    GUID::new(0x84E63DE0, 0x46AA, 0x11CF, [0x81, 0x6F, 0x00, 0x00, 0xC0, 0x20, 0x15, 0x6E]);
pub const IID_IDIRECT3DTNLHALDEVICE: GUID =
    GUID::new(0xF5049E78, 0x4861, 0x11D2, [0xA4, 0x07, 0x00, 0xA0, 0xC9, 0x06, 0x29, 0xA8]);
pub const IID_IDIRECT3DRGBDEVICE: GUID =
    GUID::new(0xA4665C60, 0x2673, 0x11CF, [0xA3, 0x1A, 0x00, 0xAA, 0x00, 0xB9, 0x33, 0x56]);
pub const IID_IDIRECT3DREFDEVICE: GUID =
    GUID::new(0x50936643, 0x13E9, 0x11D1, [0x89, 0xAA, 0x00, 0xA0, 0xC9, 0x05, 0x41, 0x29]);
/// Device identifier GUID reported for our adapter.
pub const HM1VK_DEVICE_GUID: GUID =
    GUID::new(0x4D31564B, 0x4831, 0x4D56, [0x4B, 0x52, 0x55, 0x53, 0x54, 0x00, 0x00, 0x01]);
