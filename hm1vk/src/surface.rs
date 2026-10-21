//! IDirectDrawSurface7, IDirectDrawPalette, IDirectDrawClipper and
//! IDirectDrawGammaControl.
//!
//! Every surface level keeps a CPU copy of its pixels in its DirectDraw
//! format (allocated on demand) and may have a GPU image (owned by the root of
//! a complex surface). `cpu_dirty` means the CPU copy is newer (upload before
//! the GPU uses it). `gpu_dirty` means the GPU image is newer (read back before
//! the CPU reads it).

use std::ffi::c_void;
use std::ptr::null_mut;

use ash::vk as ashvk;

use crate::ddraw::DirectDraw;
use crate::pixel::{self, Fmt};
use crate::state::world;
use crate::types::*;
use crate::vk::{vk_rect, ImageId, ImageKind, COLOR_FORMAT};
use crate::{debug, first_call, info, unimpl};

type This = *mut Surface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Primary,
    BackBuffer,
    Offscreen,
    Texture,
    ZBuffer,
}

struct Dc {
    hdc: isize,
    bitmap: isize,
    old: isize,
    bits: *mut u8,
}

#[repr(C)]
pub struct Surface {
    vtbl: *const SurfaceVtbl,
    refs: u32,
    pub ddraw: *mut DirectDraw,
    pub desc: DDSURFACEDESC2,
    pub kind: Kind,
    /// Root of the complex surface (self for roots).
    pub root: *mut Surface,
    pub level: u32,
    /// Root only: the other members of the complex surface (mip levels 1..n,
    /// or the back buffers of a flip chain).
    pub chain: Vec<*mut Surface>,
    /// Next attached surface in the complex chain.
    pub next: *mut Surface,
    pub zbuffer: *mut Surface,
    /// GPU image (on the root; back buffers and z buffers have their own).
    pub img: Option<ImageId>,
    pub mem: Vec<u8>,
    pub pitch: u32,
    pub cpu_dirty: bool,
    pub gpu_dirty: bool,
    pub palette: *mut Palette,
    pal_version: u64,
    key_version: u64,
    pub ckey_src: Option<DDCOLORKEY>,
    pub ckey_dest: Option<DDCOLORKEY>,
    locked: bool,
    lock_readonly: bool,
    clipper: *mut Clipper,
    dc: Option<Dc>,
    private: Vec<(GUID, Vec<u8>, u32)>,
    uniq: u32,
    priority: u32,
    lod: u32,
    gamma: *mut GammaControl,
    /// Has been rendered to / can be a render target.
    pub render_target: bool,
}

#[repr(C)]
pub struct SurfaceVtbl {
    query_interface: unsafe extern "system" fn(This, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(This) -> u32,
    release: unsafe extern "system" fn(This) -> u32,
    add_attached_surface: unsafe extern "system" fn(This, This) -> HRESULT,
    add_overlay_dirty_rect: unsafe extern "system" fn(This, *mut RECT) -> HRESULT,
    blt: unsafe extern "system" fn(This, *mut RECT, This, *mut RECT, u32, *mut DDBLTFX) -> HRESULT,
    blt_batch: unsafe extern "system" fn(This, *mut c_void, u32, u32) -> HRESULT,
    blt_fast: unsafe extern "system" fn(This, u32, u32, This, *mut RECT, u32) -> HRESULT,
    delete_attached_surface: unsafe extern "system" fn(This, u32, This) -> HRESULT,
    enum_attached_surfaces: unsafe extern "system" fn(This, *mut c_void, Option<LPDDENUMSURFACESCALLBACK7>) -> HRESULT,
    enum_overlay_z_orders: unsafe extern "system" fn(This, u32, *mut c_void, *mut c_void) -> HRESULT,
    flip: unsafe extern "system" fn(This, This, u32) -> HRESULT,
    get_attached_surface: unsafe extern "system" fn(This, *mut DDSCAPS2, *mut This) -> HRESULT,
    get_blt_status: unsafe extern "system" fn(This, u32) -> HRESULT,
    get_caps: unsafe extern "system" fn(This, *mut DDSCAPS2) -> HRESULT,
    get_clipper: unsafe extern "system" fn(This, *mut *mut Clipper) -> HRESULT,
    get_color_key: unsafe extern "system" fn(This, u32, *mut DDCOLORKEY) -> HRESULT,
    get_dc: unsafe extern "system" fn(This, *mut isize) -> HRESULT,
    get_flip_status: unsafe extern "system" fn(This, u32) -> HRESULT,
    get_overlay_position: unsafe extern "system" fn(This, *mut i32, *mut i32) -> HRESULT,
    get_palette: unsafe extern "system" fn(This, *mut *mut Palette) -> HRESULT,
    get_pixel_format: unsafe extern "system" fn(This, *mut DDPIXELFORMAT) -> HRESULT,
    get_surface_desc: unsafe extern "system" fn(This, *mut DDSURFACEDESC2) -> HRESULT,
    initialize: unsafe extern "system" fn(This, *mut c_void, *mut DDSURFACEDESC2) -> HRESULT,
    is_lost: unsafe extern "system" fn(This) -> HRESULT,
    lock: unsafe extern "system" fn(This, *mut RECT, *mut DDSURFACEDESC2, u32, isize) -> HRESULT,
    release_dc: unsafe extern "system" fn(This, isize) -> HRESULT,
    restore: unsafe extern "system" fn(This) -> HRESULT,
    set_clipper: unsafe extern "system" fn(This, *mut Clipper) -> HRESULT,
    set_color_key: unsafe extern "system" fn(This, u32, *mut DDCOLORKEY) -> HRESULT,
    set_overlay_position: unsafe extern "system" fn(This, i32, i32) -> HRESULT,
    set_palette: unsafe extern "system" fn(This, *mut Palette) -> HRESULT,
    unlock: unsafe extern "system" fn(This, *mut RECT) -> HRESULT,
    update_overlay: unsafe extern "system" fn(This, *mut RECT, This, *mut RECT, u32, *mut c_void) -> HRESULT,
    update_overlay_display: unsafe extern "system" fn(This, u32) -> HRESULT,
    update_overlay_z_order: unsafe extern "system" fn(This, u32, This) -> HRESULT,
    get_dd_interface: unsafe extern "system" fn(This, *mut *mut c_void) -> HRESULT,
    page_lock: unsafe extern "system" fn(This, u32) -> HRESULT,
    page_unlock: unsafe extern "system" fn(This, u32) -> HRESULT,
    set_surface_desc: unsafe extern "system" fn(This, *mut DDSURFACEDESC2, u32) -> HRESULT,
    set_private_data: unsafe extern "system" fn(This, *const GUID, *mut c_void, u32, u32) -> HRESULT,
    get_private_data: unsafe extern "system" fn(This, *const GUID, *mut c_void, *mut u32) -> HRESULT,
    free_private_data: unsafe extern "system" fn(This, *const GUID) -> HRESULT,
    get_uniqueness_value: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    change_uniqueness_value: unsafe extern "system" fn(This) -> HRESULT,
    set_priority: unsafe extern "system" fn(This, u32) -> HRESULT,
    get_priority: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    set_lod: unsafe extern "system" fn(This, u32) -> HRESULT,
    get_lod: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
}

static SURFACE_VTBL: SurfaceVtbl = SurfaceVtbl {
    query_interface: s_query_interface,
    add_ref: s_add_ref,
    release: s_release,
    add_attached_surface: s_add_attached_surface,
    add_overlay_dirty_rect: s_add_overlay_dirty_rect,
    blt: s_blt,
    blt_batch: s_blt_batch,
    blt_fast: s_blt_fast,
    delete_attached_surface: s_delete_attached_surface,
    enum_attached_surfaces: s_enum_attached_surfaces,
    enum_overlay_z_orders: s_enum_overlay_z_orders,
    flip: s_flip,
    get_attached_surface: s_get_attached_surface,
    get_blt_status: s_get_blt_status,
    get_caps: s_get_caps,
    get_clipper: s_get_clipper,
    get_color_key: s_get_color_key,
    get_dc: s_get_dc,
    get_flip_status: s_get_flip_status,
    get_overlay_position: s_get_overlay_position,
    get_palette: s_get_palette,
    get_pixel_format: s_get_pixel_format,
    get_surface_desc: s_get_surface_desc,
    initialize: s_initialize,
    is_lost: s_is_lost,
    lock: s_lock,
    release_dc: s_release_dc,
    restore: s_restore,
    set_clipper: s_set_clipper,
    set_color_key: s_set_color_key,
    set_overlay_position: s_set_overlay_position,
    set_palette: s_set_palette,
    unlock: s_unlock,
    update_overlay: s_update_overlay,
    update_overlay_display: s_update_overlay_display,
    update_overlay_z_order: s_update_overlay_z_order,
    get_dd_interface: s_get_dd_interface,
    page_lock: s_page_lock,
    page_unlock: s_page_unlock,
    set_surface_desc: s_set_surface_desc,
    set_private_data: s_set_private_data,
    get_private_data: s_get_private_data,
    free_private_data: s_free_private_data,
    get_uniqueness_value: s_get_uniqueness_value,
    change_uniqueness_value: s_change_uniqueness_value,
    set_priority: s_set_priority,
    get_priority: s_get_priority,
    set_lod: s_set_lod,
    get_lod: s_get_lod,
};

static mut KEY_VERSION: u64 = 1;

impl Surface {
    fn alloc(ddraw: *mut DirectDraw, desc: DDSURFACEDESC2, kind: Kind) -> This {
        let s = Box::into_raw(Box::new(Surface {
            vtbl: &SURFACE_VTBL,
            refs: 1,
            ddraw,
            desc,
            kind,
            root: null_mut(),
            level: 0,
            chain: Vec::new(),
            next: null_mut(),
            zbuffer: null_mut(),
            img: None,
            mem: Vec::new(),
            pitch: 0,
            cpu_dirty: false,
            gpu_dirty: false,
            palette: null_mut(),
            pal_version: 0,
            key_version: 0,
            ckey_src: None,
            ckey_dest: None,
            locked: false,
            lock_readonly: false,
            clipper: null_mut(),
            dc: None,
            private: Vec::new(),
            uniq: 1,
            priority: 0,
            lod: 0,
            gamma: null_mut(),
            render_target: false,
        }));
        unsafe {
            (*s).root = s;
            let (pitch, _) = pixel::level_layout(&(*s).desc.ddpfPixelFormat, (*s).desc.dwWidth, (*s).desc.dwHeight);
            (*s).pitch = pitch;
            if (*s).desc.dwFlags & DDSD_CKSRCBLT != 0 {
                (*s).ckey_src = Some((*s).desc.ddckCKSrcBlt);
            }
            if (*s).desc.dwFlags & DDSD_CKDESTBLT != 0 {
                (*s).ckey_dest = Some((*s).desc.ddckCKDestBlt);
            }
        }
        s
    }

    pub unsafe fn add_ref(s: This) {
        (*s).refs += 1;
    }
    pub unsafe fn release(s: This) {
        s_release(s);
    }

    pub fn width(&self) -> u32 {
        self.desc.dwWidth
    }
    pub fn height(&self) -> u32 {
        self.desc.dwHeight
    }
    pub fn pf(&self) -> &DDPIXELFORMAT {
        &self.desc.ddpfPixelFormat
    }
    pub fn caps(&self) -> u32 {
        self.desc.ddsCaps.dwCaps
    }
    fn full_rect(&self) -> RECT {
        RECT::new(0, 0, self.width() as i32, self.height() as i32)
    }

    /// Implements IDirectDraw7::CreateSurface.
    pub unsafe fn create(ddraw: *mut DirectDraw, req: &DDSURFACEDESC2) -> Result<This, HRESULT> {
        let w = world();
        let mut d = DDSURFACEDESC2::default();
        let n = (req.dwSize as usize).clamp(0, std::mem::size_of::<DDSURFACEDESC2>());
        if n >= 108 {
            std::ptr::copy_nonoverlapping(req as *const _ as *const u8, &mut d as *mut _ as *mut u8, n);
        } else {
            return Err(DDERR_INVALIDPARAMS);
        }
        d.dwSize = std::mem::size_of::<DDSURFACEDESC2>() as u32;
        if d.dwFlags & DDSD_CAPS == 0 {
            d.dwFlags |= DDSD_CAPS;
            d.ddsCaps.dwCaps = DDSCAPS_OFFSCREENPLAIN;
        }
        let caps = d.ddsCaps.dwCaps;
        let mode = w.screen_mode();
        let kind = if caps & DDSCAPS_PRIMARYSURFACE != 0 {
            Kind::Primary
        } else if caps & DDSCAPS_ZBUFFER != 0 {
            Kind::ZBuffer
        } else if caps & DDSCAPS_TEXTURE != 0 {
            Kind::Texture
        } else {
            Kind::Offscreen
        };
        if kind == Kind::Primary || d.dwFlags & (DDSD_WIDTH | DDSD_HEIGHT) != (DDSD_WIDTH | DDSD_HEIGHT) {
            if kind == Kind::Primary {
                d.dwWidth = mode.width;
                d.dwHeight = mode.height;
            } else if d.dwFlags & DDSD_WIDTH == 0 || d.dwFlags & DDSD_HEIGHT == 0 {
                d.dwWidth = if d.dwFlags & DDSD_WIDTH != 0 { d.dwWidth } else { mode.width };
                d.dwHeight = if d.dwFlags & DDSD_HEIGHT != 0 { d.dwHeight } else { mode.height };
            }
            d.dwFlags |= DDSD_WIDTH | DDSD_HEIGHT;
        }
        if d.dwWidth == 0 || d.dwHeight == 0 || d.dwWidth > 16384 || d.dwHeight > 16384 {
            return Err(DDERR_INVALIDPARAMS);
        }
        if d.dwFlags & DDSD_PIXELFORMAT == 0 {
            d.dwFlags |= DDSD_PIXELFORMAT;
            d.ddpfPixelFormat = if kind == Kind::ZBuffer {
                let bits = if d.dwFlags & DDSD_ZBUFFERBITDEPTH != 0 { d.dwMipMapCount } else { 16 };
                match bits {
                    32 => pixel::pf_depth(32, 0xFFFFFF, 0, 0),
                    24 => pixel::pf_depth(24, 0xFFFFFF, 0, 0),
                    _ => pixel::pf_depth(16, 0xFFFF, 0, 0),
                }
            } else if mode.bpp == 16 {
                pixel::pf_rgb565()
            } else {
                pixel::pf_xrgb8888()
            };
        }
        d.ddpfPixelFormat.dwSize = 32;
        // Normalise caps to what a hardware driver reports.
        if caps & DDSCAPS_SYSTEMMEMORY == 0 {
            d.ddsCaps.dwCaps |= DDSCAPS_VIDEOMEMORY | DDSCAPS_LOCALVIDMEM;
        }
        if let Fmt::Dxt { fourcc } = pixel::classify(&d.ddpfPixelFormat) {
            if !matches!(fourcc, FOURCC_DXT1 | FOURCC_DXT2 | FOURCC_DXT3 | FOURCC_DXT4 | FOURCC_DXT5) {
                info!("CreateSurface: unsupported FOURCC {fourcc:#x}");
                return Err(DDERR_INVALIDPIXELFORMAT);
            }
        }

        match kind {
            Kind::Primary => Self::create_primary(ddraw, d),
            Kind::Texture => Self::create_texture(ddraw, d),
            Kind::ZBuffer => {
                let s = Self::alloc(ddraw, d, Kind::ZBuffer);
                if let Some(g) = w.gpu() {
                    let fmt = g.depth_format;
                    (*s).img = g.create_image(d.dwWidth, d.dwHeight, 1, ImageKind::Depth, fmt).ok();
                    if let Some(id) = (*s).img {
                        g.fill_depth(id, vk_rect(0, 0, d.dwWidth, d.dwHeight), 1.0, Some(0));
                    }
                }
                debug!("CreateSurface ZBUFFER {}x{} {}bit", d.dwWidth, d.dwHeight, d.ddpfPixelFormat.dwRGBBitCount);
                Ok(s)
            }
            _ => {
                let s = Self::alloc(ddraw, d, kind);
                if caps & DDSCAPS_3DDEVICE != 0 {
                    (*s).render_target = true;
                }
                debug!(
                    "CreateSurface {:?} {}x{} caps={:#x} pf={:?}",
                    kind,
                    d.dwWidth,
                    d.dwHeight,
                    caps,
                    pixel::classify(&d.ddpfPixelFormat)
                );
                Ok(s)
            }
        }
    }

    unsafe fn create_primary(ddraw: *mut DirectDraw, mut d: DDSURFACEDESC2) -> Result<This, HRESULT> {
        let w = world();
        let backbuffers = if d.dwFlags & DDSD_BACKBUFFERCOUNT != 0 && d.ddsCaps.dwCaps & DDSCAPS_FLIP != 0 {
            d.dwBackBufferCount.clamp(1, 3)
        } else {
            0
        };
        d.ddsCaps.dwCaps |= DDSCAPS_FRONTBUFFER | DDSCAPS_VISIBLE;
        let p = Self::alloc(ddraw, d, Kind::Primary);
        info!(
            "CreateSurface PRIMARY {}x{} backbuffers={} caps={:#x}",
            d.dwWidth, d.dwHeight, backbuffers, d.ddsCaps.dwCaps
        );
        let mut prev = p;
        let mut shared_img = None;
        for i in 0..backbuffers {
            let mut bd = d;
            bd.dwFlags &= !DDSD_BACKBUFFERCOUNT;
            bd.dwBackBufferCount = 0;
            bd.ddsCaps.dwCaps &= !(DDSCAPS_PRIMARYSURFACE | DDSCAPS_FRONTBUFFER | DDSCAPS_VISIBLE);
            bd.ddsCaps.dwCaps |= DDSCAPS_BACKBUFFER | DDSCAPS_FLIP | DDSCAPS_COMPLEX;
            let b = Self::alloc(ddraw, bd, Kind::BackBuffer);
            (*b).refs = 0;
            (*b).root = p;
            (*b).level = i + 1;
            (*b).render_target = true;
            if shared_img.is_none() {
                if let Some(g) = w.gpu() {
                    shared_img = g.create_image(d.dwWidth, d.dwHeight, 1, ImageKind::Color, COLOR_FORMAT).ok();
                    if let Some(id) = shared_img {
                        g.fill_color(id, vk_rect(0, 0, d.dwWidth, d.dwHeight), [0.0, 0.0, 0.0, 1.0]);
                    }
                }
            }
            (*b).img = shared_img;
            (*prev).next = b;
            (*p).chain.push(b);
            prev = b;
        }
        if backbuffers == 0 {
            // Windowed primary: the visible window. It gets an image so that
            // blits to it can be composed before presenting.
            (*p).render_target = false;
        }
        w.primary = p;
        Ok(p)
    }

    unsafe fn create_texture(ddraw: *mut DirectDraw, mut d: DDSURFACEDESC2) -> Result<This, HRESULT> {
        let mut levels = 1u32;
        if d.ddsCaps.dwCaps & DDSCAPS_MIPMAP != 0 {
            let max_levels = 32 - d.dwWidth.max(d.dwHeight).leading_zeros();
            levels = if d.dwFlags & DDSD_MIPMAPCOUNT != 0 && d.dwMipMapCount > 0 { d.dwMipMapCount } else { max_levels };
            levels = levels.min(max_levels).max(1);
            d.dwFlags |= DDSD_MIPMAPCOUNT;
            d.dwMipMapCount = levels;
        }
        let root = Self::alloc(ddraw, d, Kind::Texture);
        if d.ddsCaps.dwCaps & DDSCAPS_3DDEVICE != 0 {
            (*root).render_target = true;
        }
        let mut prev = root;
        for l in 1..levels {
            let mut ld = d;
            ld.dwWidth = (d.dwWidth >> l).max(1);
            ld.dwHeight = (d.dwHeight >> l).max(1);
            ld.dwMipMapCount = levels - l;
            ld.ddsCaps.dwCaps2 |= DDSCAPS2_MIPMAPSUBLEVEL;
            let s = Self::alloc(ddraw, ld, Kind::Texture);
            (*s).refs = 0;
            (*s).root = root;
            (*s).level = l;
            (*prev).next = s;
            (*root).chain.push(s);
            prev = s;
        }
        debug!(
            "CreateSurface TEXTURE {}x{} levels={} caps={:#x}/{:#x} pf={:?} flags={:#x}",
            d.dwWidth,
            d.dwHeight,
            levels,
            d.ddsCaps.dwCaps,
            d.ddsCaps.dwCaps2,
            pixel::classify(&d.ddpfPixelFormat),
            d.ddpfPixelFormat.dwFlags
        );
        Ok(root)
    }

    /// All members of this surface's complex structure, root first.
    pub unsafe fn levels(root: This) -> Vec<This> {
        let mut v = vec![root];
        v.extend((*root).chain.iter().copied());
        v
    }

    pub unsafe fn ensure_mem(s: This) {
        let sf = &mut *s;
        if sf.mem.is_empty() {
            let (pitch, size) = pixel::level_layout(sf.pf(), sf.width(), sf.height());
            sf.pitch = pitch;
            sf.mem = vec![0u8; size as usize];
        }
    }

    /// The GPU image holding this surface's pixels (creating it if needed).
    pub unsafe fn image(s: This) -> Option<ImageId> {
        let sf = &mut *s;
        let owner: This = match sf.kind {
            Kind::BackBuffer | Kind::ZBuffer => s,
            Kind::Primary => {
                if let Some(&b) = sf.chain.first() {
                    b
                } else {
                    s
                }
            }
            _ => sf.root,
        };
        if let Some(id) = (*owner).img {
            return Some(id);
        }
        let o = &mut *owner;
        let g = world().gpu()?;
        let levels = if o.kind == Kind::Texture { 1 + o.chain.len() as u32 } else { 1 };
        let (kind, fmt) = match pixel::classify(o.pf()) {
            Fmt::Dxt { fourcc } if g.bc_supported => (
                ImageKind::Compressed,
                match fourcc {
                    FOURCC_DXT1 => ashvk::Format::BC1_RGBA_UNORM_BLOCK,
                    FOURCC_DXT2 | FOURCC_DXT3 => ashvk::Format::BC2_UNORM_BLOCK,
                    _ => ashvk::Format::BC3_UNORM_BLOCK,
                },
            ),
            Fmt::Depth { .. } => (ImageKind::Depth, g.depth_format),
            _ => (ImageKind::Color, COLOR_FORMAT),
        };
        let id = g.create_image(o.width(), o.height(), levels, kind, fmt).ok()?;
        o.img = Some(id);
        // Everything on the CPU side must be uploaded.
        for l in Self::levels(owner) {
            if !(*l).mem.is_empty() {
                (*l).cpu_dirty = true;
            }
        }
        Some(id)
    }

    fn palette_entries(&self) -> Option<(&[PALETTEENTRY; 256], bool)> {
        let root = unsafe { &*self.root };
        let p = if !self.palette.is_null() { self.palette } else { root.palette };
        if p.is_null() {
            None
        } else {
            unsafe { Some((&(*p).entries, (*p).caps & DDPCAPS_ALPHA != 0)) }
        }
    }

    /// Uploads CPU-side changes so the GPU image is current.
    pub unsafe fn sync_to_gpu(s: This) -> Option<ImageId> {
        let id = Self::image(s)?;
        let sf = &*s;
        let owner: This = match sf.kind {
            Kind::BackBuffer | Kind::ZBuffer => s,
            Kind::Primary => *sf.chain.first().unwrap_or(&s),
            _ => sf.root,
        };
        if (*owner).kind == Kind::ZBuffer {
            return Some(id);
        }
        let root = &mut *owner;
        // Palette or colour-key changes force a re-upload of all levels.
        let pal_v = root.palette_entries().map(|_| (*root.palette).version).unwrap_or(0);
        let force = pal_v != root.pal_version || root.key_version != KEY_VERSION_OF(root);
        let g = world().gpu()?;
        let compressed = g.image(id).map(|i| i.kind == ImageKind::Compressed).unwrap_or(false);
        for l in Self::levels(owner) {
            let lv = &mut *l;
            if lv.mem.is_empty() || !(lv.cpu_dirty || force) {
                continue;
            }
            if lv.gpu_dirty && !lv.cpu_dirty {
                continue;
            }
            let (w, h) = (lv.width(), lv.height());
            if compressed {
                let data = lv.mem.clone();
                g.upload_level(id, lv.level_index(), w, h, &data);
            } else {
                let (pal, pal_alpha) = match root.palette_entries() {
                    Some((p, a)) => (Some(p), a),
                    None => (None, false),
                };
                let key = root.ckey_src.filter(|_| root.kind == Kind::Texture);
                let bgra = pixel::to_bgra8(&lv.mem, lv.pitch, w, h, lv.pf(), pal, pal_alpha, key);
                g.upload_level(id, lv.level_index(), w, h, &bgra);
            }
            lv.cpu_dirty = false;
            lv.gpu_dirty = false;
        }
        root.pal_version = pal_v;
        root.key_version = KEY_VERSION_OF(root);
        Some(id)
    }

    fn level_index(&self) -> u32 {
        if self.kind == Kind::Texture {
            self.level
        } else {
            0
        }
    }

    /// Reads GPU-side changes back into the CPU copy.
    pub unsafe fn sync_to_cpu(s: This) {
        Self::ensure_mem(s);
        let sf = &mut *s;
        if !sf.gpu_dirty {
            return;
        }
        sf.gpu_dirty = false;
        let Some(id) = Self::image(s) else { return };
        let Some(g) = world().gpu() else { return };
        if g.image(id).map(|i| i.kind != ImageKind::Color).unwrap_or(true) {
            return;
        }
        if let Some(bgra) = g.read_image(id) {
            let (iw, ih) = g.image(id).map(|i| (i.width, i.height)).unwrap_or((0, 0));
            if iw == sf.width() && ih == sf.height() {
                let (pitch, pf) = (sf.pitch, sf.desc.ddpfPixelFormat);
                pixel::from_bgra8(&bgra, iw, ih, &mut sf.mem, pitch, &pf);
            }
        }
    }

    /// Marks that the GPU image was written (render target / blit destination).
    pub unsafe fn mark_gpu_written(s: This) {
        let t = match (*s).kind {
            Kind::Primary => *(*s).chain.first().unwrap_or(&s),
            _ => s,
        };
        (*t).gpu_dirty = true;
        (*t).cpu_dirty = false;
        (*t).uniq = (*t).uniq.wrapping_add(1);
    }

    pub fn is_gpu_target(&self) -> bool {
        matches!(self.kind, Kind::BackBuffer | Kind::Primary) || self.render_target || self.gpu_dirty
    }

    unsafe fn destroy(s: This) {
        let sf = &mut *s;
        info_destroy(sf);
        if let Some(dc) = sf.dc.take() {
            release_dc_objects(dc);
        }
        if !sf.zbuffer.is_null() {
            s_release(sf.zbuffer);
            sf.zbuffer = null_mut();
        }
        if !sf.palette.is_null() {
            Palette::release(sf.palette);
            sf.palette = null_mut();
        }
        if !sf.clipper.is_null() {
            Clipper::release(sf.clipper);
            sf.clipper = null_mut();
        }
        if !sf.gamma.is_null() {
            drop(Box::from_raw(sf.gamma));
            sf.gamma = null_mut();
        }
        for (_, data, flags) in sf.private.drain(..) {
            release_private(&data, flags);
        }
        let mut freed_imgs: Vec<ImageId> = Vec::new();
        if let Some(id) = sf.img.take() {
            freed_imgs.push(id);
        }
        for c in std::mem::take(&mut sf.chain) {
            if let Some(id) = (*c).img.take() {
                if !freed_imgs.contains(&id) {
                    freed_imgs.push(id);
                }
            }
            if !(*c).zbuffer.is_null() {
                s_release((*c).zbuffer);
            }
            drop(Box::from_raw(c));
        }
        if let Some(g) = world().gpu_if_ready() {
            for id in freed_imgs {
                g.destroy_image(id);
            }
        }
        let w = world();
        if w.primary == s {
            w.primary = null_mut();
        }
        crate::device::forget_surface(s);
        drop(Box::from_raw(s));
    }

    fn hwnd(&self) -> isize {
        if !self.clipper.is_null() {
            let h = unsafe { (*self.clipper).hwnd };
            if h != 0 {
                return h;
            }
        }
        world().hwnd
    }
}

#[allow(non_snake_case)]
fn KEY_VERSION_OF(s: &Surface) -> u64 {
    match s.ckey_src {
        Some(k) => 1 + ((k.low as u64) << 32 | k.high as u64) % 0xFFFF_FFFF,
        None => 0,
    }
}

fn info_destroy(s: &Surface) {
    debug!("surface destroyed {:?} {}x{}", s.kind, s.width(), s.height());
}

unsafe fn release_private(data: &[u8], flags: u32) {
    const DDSPD_IUNKNOWNPOINTER: u32 = 4;
    if flags & DDSPD_IUNKNOWNPOINTER != 0 && data.len() >= 4 {
        let p = (data.as_ptr() as *const *mut *const usize).read_unaligned();
        if !p.is_null() {
            let rel: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(*(*p).add(2));
            rel(p as *mut c_void);
        }
    }
}

// ------------------------------------------------------------ presentation

/// Presents a surface's pixels to the game window.
pub unsafe fn present(src: This, src_rect: Option<RECT>, dst_client: Option<RECT>) {
    let w = world();
    let hwnd = (*src).hwnd();
    let Some(id) = Surface::sync_to_gpu(src) else { return };
    let (sw, sh) = ((*src).width(), (*src).height());
    let r = src_rect.unwrap_or(RECT::new(0, 0, sw as i32, sh as i32)).intersect(&RECT::new(0, 0, sw as i32, sh as i32));
    if r.is_empty() {
        return;
    }
    // Frame limiter.
    if w.config.fps_limit > 0 {
        let frame = std::time::Duration::from_secs_f64(1.0 / w.config.fps_limit as f64);
        if let Some(last) = w.last_present {
            let el = last.elapsed();
            if el < frame {
                std::thread::sleep(frame - el);
            }
        }
        w.last_present = Some(std::time::Instant::now());
    }
    // Gamma ramp -> LUT texture.
    if w.gamma_dirty {
        w.gamma_dirty = false;
        let ramp = w.gamma;
        if let Some(g) = w.gpu() {
            let mut lut = vec![0u8; 256 * 4];
            for i in 0..256 {
                lut[i * 4] = (ramp.blue[i] >> 8) as u8;
                lut[i * 4 + 1] = (ramp.green[i] >> 8) as u8;
                lut[i * 4 + 2] = (ramp.red[i] >> 8) as u8;
                lut[i * 4 + 3] = 255;
            }
            let lid = g.gamma_lut;
            g.upload_level(lid, 0, 256, 1, &lut);
        }
    }
    let gamma = w.gamma_active;
    let vsync = w.config.vsync;
    w.frame_count += 1;
    let fc = w.frame_count;
    let now = std::time::Instant::now();
    match w.fps_timer {
        None => w.fps_timer = Some(now),
        Some(t) if now.duration_since(t).as_secs() >= 10 => {
            debug!("frames presented: {fc}");
            w.fps_timer = Some(now);
        }
        _ => {}
    }
    let r = if dst_client.is_none() && src_rect.is_none() { menu_zoom(w, hwnd, sw, sh).unwrap_or(r) } else { r };
    let Some(g) = w.gpu() else { return };
    let dst = dst_client.map(|d| [d.left, d.top, d.right, d.bottom]);
    g.present(hwnd, id, [r.left as u32, r.top as u32, r.right as u32, r.bottom as u32], dst, gamma, vsync);
}

/// Menu zoom: the engine draws its front-end screens (main menu, options,
/// laptop briefing, ...) as a fixed 640x480 panel in the centre of the frame,
/// which is tiny at 3840x2160. When a frame contains only pre-transformed 2D
/// geometry inside such a centred panel, present just that panel, scaled to
/// fill the display. The game draws its own cursor into the frame, so what
/// the player sees and what the game hit-tests stay consistent.
unsafe fn menu_zoom(w: &mut crate::state::World, hwnd: isize, sw: u32, sh: u32) -> Option<RECT> {
    let enabled = w.config.menu_zoom;
    let clip = w.config.menu_zoom_clip_cursor;
    let (draws, bbox) = (w.frame_2d_draws, w.frame_2d_bbox);
    w.frame_2d_draws = 0;
    w.frame_2d_bbox = None;
    if w.frame_count % 240 == 1 {
        crate::trace!("frame stats: draws={draws} region={bbox:?} zoom={:?}", w.zoom_rect);
    }
    if !enabled {
        return None;
    }
    let (fw, fh) = (sw as f32, sh as f32);
    let new_zoom = if draws == 0 {
        // Empty frame (fades, loading): keep the current decision.
        w.zoom_rect
    } else {
        bbox.and_then(|b| {
            let (x0, y0) = (b[0].max(0.0), b[1].max(0.0));
            let (x1, y1) = (b[2].min(fw), b[3].min(fh));
            let (bw, bh) = (x1 - x0, y1 - y0);
            let centred = ((x0 + x1) / 2.0 - fw / 2.0).abs() <= 8.0 && ((y0 + y1) / 2.0 - fh / 2.0).abs() <= 8.0;
            // Only zoom panels that are clearly smaller than the screen.
            if !centred || bw < 160.0 || bh < 120.0 || bw > fw * 0.67 || bh > fh * 0.67 {
                return None;
            }
            // Snap to the engine's standard panel sizes when the content fits one.
            let snapped = [(640u32, 480u32), (800, 600), (1024, 768)].into_iter().find_map(|(pw, ph)| {
                let px = ((sw.saturating_sub(pw)) / 2) as f32;
                let py = ((sh.saturating_sub(ph)) / 2) as f32;
                let fits = x0 >= px - 2.0 && y0 >= py - 2.0 && x1 <= px + pw as f32 + 2.0 && y1 <= py + ph as f32 + 2.0;
                fits.then(|| RECT::new(px as i32, py as i32, px as i32 + pw as i32, py as i32 + ph as i32))
            });
            Some(snapped.unwrap_or(RECT::new(x0.floor() as i32, y0.floor() as i32, x1.ceil() as i32, y1.ceil() as i32)))
        })
    };
    if new_zoom != w.zoom_rect {
        info!("menu zoom: {:?}", new_zoom);
        w.zoom_rect = new_zoom;
    }
    if clip {
        update_cursor_clip(hwnd, new_zoom);
    }
    new_zoom
}

static mut CLIPPED: Option<RECT> = None;

/// Confines the cursor to the zoomed panel while the game is in the
/// foreground; releases it otherwise.
unsafe fn update_cursor_clip(hwnd: isize, r: Option<RECT>) {
    use windows_sys::Win32::Foundation::{POINT, RECT as WRECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::{ClipCursor, GetForegroundWindow};
    let want = r.filter(|_| GetForegroundWindow() as isize == hwnd);
    if want == CLIPPED {
        return;
    }
    match want {
        Some(r) => {
            let mut a = POINT { x: r.left, y: r.top };
            let mut b = POINT { x: r.right, y: r.bottom };
            ClientToScreen(hwnd as _, &mut a);
            ClientToScreen(hwnd as _, &mut b);
            ClipCursor(&WRECT { left: a.x, top: a.y, right: b.x, bottom: b.y });
        }
        None => {
            ClipCursor(std::ptr::null());
        }
    }
    CLIPPED = want;
}

/// Releases a cursor clip set by the menu zoom (DLL unload).
pub unsafe fn release_cursor_clip() {
    if (*&raw const CLIPPED).is_some() {
        windows_sys::Win32::UI::WindowsAndMessaging::ClipCursor(std::ptr::null());
        CLIPPED = None;
    }
}

// ------------------------------------------------------------ IUnknown

unsafe extern "system" fn s_query_interface(this: This, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    let iid = *iid;
    if iid == IID_IDIRECTDRAWSURFACE7 || iid == IID_IUNKNOWN {
        s_add_ref(this);
        *out = this as *mut c_void;
        return S_OK;
    }
    if iid == IID_IDIRECTDRAWGAMMACONTROL {
        let sf = &mut *this;
        if sf.gamma.is_null() {
            sf.gamma = Box::into_raw(Box::new(GammaControl { vtbl: &GAMMA_VTBL, surface: this }));
        }
        s_add_ref(this);
        *out = sf.gamma as *mut c_void;
        return S_OK;
    }
    info!("IDirectDrawSurface7::QueryInterface: unsupported {iid}");
    *out = null_mut();
    E_NOINTERFACE
}

unsafe extern "system" fn s_add_ref(this: This) -> u32 {
    (*this).refs += 1;
    (*this).refs
}

unsafe extern "system" fn s_release(this: This) -> u32 {
    let sf = &mut *this;
    if sf.refs == 0 {
        return 0;
    }
    sf.refs -= 1;
    let r = sf.refs;
    if r == 0 && sf.root == this {
        Surface::destroy(this);
    }
    r
}

// ------------------------------------------------------------ attachments

unsafe extern "system" fn s_add_attached_surface(this: This, att: This) -> HRESULT {
    if att.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    if (*att).kind == Kind::ZBuffer {
        if !(*this).zbuffer.is_null() {
            return DDERR_SURFACEALREADYATTACHED;
        }
        s_add_ref(att);
        (*this).zbuffer = att;
        debug!("z-buffer attached to {:?}", (*this).kind);
        return DD_OK;
    }
    unimpl!("AddAttachedSurface (non z-buffer)");
    DDERR_CANNOTATTACHSURFACE
}

unsafe extern "system" fn s_delete_attached_surface(this: This, _flags: u32, att: This) -> HRESULT {
    let z = (*this).zbuffer;
    if !z.is_null() && (att.is_null() || att == z) {
        (*this).zbuffer = null_mut();
        s_release(z);
        return DD_OK;
    }
    DDERR_SURFACENOTATTACHED
}

unsafe fn attached(this: This) -> Vec<This> {
    let mut v = Vec::new();
    if !(*this).next.is_null() {
        v.push((*this).next);
    }
    if !(*this).zbuffer.is_null() {
        v.push((*this).zbuffer);
    }
    v
}

unsafe extern "system" fn s_get_attached_surface(this: This, caps: *mut DDSCAPS2, out: *mut This) -> HRESULT {
    if caps.is_null() || out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let want = (*caps).dwCaps;
    let want2 = (*caps).dwCaps2;
    for a in attached(this) {
        let c = (*a).desc.ddsCaps;
        if c.dwCaps & want == want && c.dwCaps2 & want2 == want2 {
            s_add_ref(a);
            *out = a;
            return DD_OK;
        }
    }
    *out = null_mut();
    DDERR_NOTFOUND
}

unsafe extern "system" fn s_enum_attached_surfaces(this: This, ctx: *mut c_void, cb: Option<LPDDENUMSURFACESCALLBACK7>) -> HRESULT {
    let Some(cb) = cb else { return DDERR_INVALIDPARAMS };
    for a in attached(this) {
        s_add_ref(a);
        let mut d = (*a).desc;
        if cb(a as *mut c_void, &mut d, ctx) == DDENUMRET_CANCEL {
            break;
        }
    }
    DD_OK
}

// ------------------------------------------------------------ blits

/// Destination kind for blits.
unsafe fn blt_to_gpu(dst: This) -> bool {
    let d = &*dst;
    match d.kind {
        Kind::Primary | Kind::BackBuffer => true,
        Kind::ZBuffer => true,
        _ => d.render_target || (d.gpu_dirty && !d.cpu_dirty),
    }
}

pub unsafe fn do_blt(dst: This, dst_rect: Option<RECT>, src: This, src_rect: Option<RECT>, flags: u32, fx: Option<&DDBLTFX>) -> HRESULT {
    let d = &mut *dst;
    let drect = dst_rect.unwrap_or(d.full_rect());
    let dclip = drect.intersect(&d.full_rect());

    // ---- colour / depth fills
    if flags & DDBLT_COLORFILL != 0 {
        let Some(fx) = fx else { return DDERR_INVALIDPARAMS };
        if dclip.is_empty() {
            return DD_OK;
        }
        if blt_to_gpu(dst) && d.kind != Kind::ZBuffer {
            let rgba = pixel::pixel_to_rgba(fx.dwFillColor, d.pf());
            if d.kind == Kind::Primary && d.chain.is_empty() {
                // Windowed primary fill: nothing to show until a present.
                return DD_OK;
            }
            Surface::sync_to_gpu(dst);
            if let (Some(id), Some(g)) = (Surface::image(dst), world().gpu()) {
                g.fill_color(id, vk_rect(dclip.left, dclip.top, dclip.width() as u32, dclip.height() as u32), rgba);
                Surface::mark_gpu_written(dst);
            }
        } else {
            Surface::sync_to_cpu(dst);
            let (pitch, pf) = (d.pitch, d.desc.ddpfPixelFormat);
            pixel::fill(&mut d.mem, pitch, &pf, dclip, fx.dwFillColor);
            d.cpu_dirty = true;
        }
        return DD_OK;
    }
    if flags & DDBLT_DEPTHFILL != 0 {
        let Some(fx) = fx else { return DDERR_INVALIDPARAMS };
        let bits = d.pf().dwRGBBitCount.clamp(16, 32);
        let max = if bits >= 32 { 0xFFFF_FFFFu64 } else { (1u64 << bits.min(24)) - 1 };
        let depth = (fx.dwFillColor as u64).min(max) as f32 / max as f32;
        if let (Some(id), Some(g)) = (Surface::image(dst), world().gpu()) {
            g.fill_depth(id, vk_rect(dclip.left, dclip.top, dclip.width() as u32, dclip.height() as u32), depth, None);
        }
        return DD_OK;
    }
    if src.is_null() {
        unimpl!("Blt without source (ROP/other)");
        return DD_OK;
    }
    let s = &mut *src;
    let srect = src_rect.unwrap_or(s.full_rect());
    if srect.is_empty() || drect.is_empty() {
        return DD_OK;
    }
    let key = if flags & DDBLT_KEYSRCOVERRIDE != 0 {
        fx.map(|f| f.ddckSrcColorkey)
    } else if flags & DDBLT_KEYSRC != 0 {
        s.ckey_src
    } else {
        None
    };
    let mirror_lr = flags & DDBLT_DDFX != 0 && fx.map(|f| f.dwDDFX & DDBLTFX_MIRRORLEFTRIGHT != 0).unwrap_or(false);
    let mirror_ud = flags & DDBLT_DDFX != 0 && fx.map(|f| f.dwDDFX & DDBLTFX_MIRRORUPDOWN != 0).unwrap_or(false);

    // ---- presentation: back buffer (or anything) -> primary
    if d.kind == Kind::Primary {
        if !d.chain.is_empty() && !world().is_fullscreen() {
            // fall through to windowed present below
        }
        let hwnd = s.hwnd();
        let client = screen_to_client(hwnd, drect);
        let dst_client = if world().is_fullscreen() { None } else { Some(client) };
        present(src, Some(srect), dst_client);
        return DD_OK;
    }

    // ---- GPU destination
    if blt_to_gpu(dst) {
        Surface::sync_to_gpu(dst);
        let Some(did) = Surface::image(dst) else { return DD_OK };
        let src_gpu = s.is_gpu_target() && !s.cpu_dirty;
        if !src_gpu {
            // CPU source: make sure its texture image is current.
            Surface::ensure_mem(src);
        }
        let Some(sid) = Surface::sync_to_gpu(src) else { return DD_OK };
        let Some(g) = world().gpu() else { return DD_OK };
        let compressed_src = g.image(sid).map(|i| i.kind != ImageKind::Color).unwrap_or(true);
        let stretch = srect.width() != drect.width() || srect.height() != drect.height();
        if key.is_none() && !compressed_src {
            let (mut sr, mut dr) = (
                [srect.left, srect.top, srect.right, srect.bottom],
                [drect.left, drect.top, drect.right, drect.bottom],
            );
            if mirror_lr {
                dr.swap(0, 2);
            }
            if mirror_ud {
                dr.swap(1, 3);
            }
            clamp_blit_rects(&mut sr, &mut dr, s.width() as i32, s.height() as i32, d.width() as i32, d.height() as i32);
            g.blit(sid, sr, did, dr, stretch);
        } else {
            // Keyed or compressed source: draw a textured quad.
            crate::device::draw_blit_quad(did, (d.width(), d.height()), drect, sid, (s.width(), s.height()), srect, key.is_some(), stretch);
        }
        Surface::mark_gpu_written(dst);
        return DD_OK;
    }

    // ---- CPU destination
    Surface::sync_to_cpu(dst);
    Surface::sync_to_cpu(src);
    Surface::ensure_mem(src);
    let same_fmt = s.pf() == d.pf();
    let bytes = match pixel::classify(d.pf()) {
        Fmt::Masked { bpp } => bpp.div_ceil(8) as usize,
        Fmt::Pal8 => 1,
        _ => {
            unimpl!("CPU Blt to compressed/depth surface");
            return DD_OK;
        }
    };
    let (dpitch, spitch) = (d.pitch, s.pitch);
    let mut sr = srect.intersect(&s.full_rect());
    let mut dr = drect;
    // Clip destination, adjusting source proportionally for 1:1 blits.
    if sr.width() == dr.width() && sr.height() == dr.height() {
        let c = dr.intersect(&d.full_rect());
        sr = RECT::new(sr.left + (c.left - dr.left), sr.top + (c.top - dr.top), sr.right - (dr.right - c.right), sr.bottom - (dr.bottom - c.bottom));
        dr = c;
    } else {
        dr = dr.intersect(&d.full_rect());
    }
    if sr.is_empty() || dr.is_empty() {
        return DD_OK;
    }
    if same_fmt {
        let src_mem = if src == dst { s.mem.clone() } else { std::mem::take(&mut s.mem) };
        pixel::blit(&src_mem, spitch, sr, &mut d.mem, dpitch, dr, bytes, key, mirror_lr, mirror_ud);
        if src != dst {
            s.mem = src_mem;
        }
    } else {
        // Convert through BGRA8.
        let (pal, pa) = match s.palette_entries() {
            Some((p, a)) => (Some(p), a),
            None => (None, false),
        };
        let bgra = pixel::to_bgra8(&s.mem, spitch, s.width(), s.height(), s.pf(), pal, pa, key);
        let mut tmp = vec![0u8; (d.width() * d.height() * 4) as usize];
        let dpf = d.desc.ddpfPixelFormat;
        // existing destination pixels
        let cur = pixel::to_bgra8(&d.mem, dpitch, d.width(), d.height(), &dpf, None, false, None);
        tmp.copy_from_slice(&cur);
        let k = key.map(|_| DDCOLORKEY { low: 0, high: 0x00FF_FFFF });
        let _ = k;
        pixel::blit(&bgra, s.width() * 4, sr, &mut tmp, d.width() * 4, dr, 4, None, mirror_lr, mirror_ud);
        let (w, h) = (d.width(), d.height());
        pixel::from_bgra8(&tmp, w, h, &mut d.mem, dpitch, &dpf);
    }
    d.cpu_dirty = true;
    d.uniq = d.uniq.wrapping_add(1);
    DD_OK
}

fn clamp_blit_rects(s: &mut [i32; 4], d: &mut [i32; 4], sw: i32, sh: i32, dw: i32, dh: i32) {
    for i in [0, 2] {
        s[i] = s[i].clamp(0, sw);
        d[i] = d[i].clamp(0, dw);
    }
    for i in [1, 3] {
        s[i] = s[i].clamp(0, sh);
        d[i] = d[i].clamp(0, dh);
    }
}

fn screen_to_client(hwnd: isize, r: RECT) -> RECT {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    let mut a = POINT { x: r.left, y: r.top };
    let mut b = POINT { x: r.right, y: r.bottom };
    unsafe {
        ScreenToClient(hwnd as _, &mut a);
        ScreenToClient(hwnd as _, &mut b);
    }
    RECT::new(a.x, a.y, b.x, b.y)
}

unsafe extern "system" fn s_blt(this: This, dst_rect: *mut RECT, src: This, src_rect: *mut RECT, flags: u32, fx: *mut DDBLTFX) -> HRESULT {
    first_call!("IDirectDrawSurface7::Blt", "flags={flags:#x}");
    let dr = if dst_rect.is_null() { None } else { Some(*dst_rect) };
    let sr = if src_rect.is_null() { None } else { Some(*src_rect) };
    let fx = if fx.is_null() { None } else { Some(&*fx) };
    do_blt(this, dr, src, sr, flags, fx)
}

unsafe extern "system" fn s_blt_batch(_this: This, _b: *mut c_void, _c: u32, _f: u32) -> HRESULT {
    unimpl!("IDirectDrawSurface7::BltBatch");
    DDERR_UNSUPPORTED
}

unsafe extern "system" fn s_blt_fast(this: This, x: u32, y: u32, src: This, src_rect: *mut RECT, trans: u32) -> HRESULT {
    first_call!("IDirectDrawSurface7::BltFast", "trans={trans:#x}");
    if src.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let sr = if src_rect.is_null() { (*src).full_rect() } else { *src_rect };
    let dr = RECT::new(x as i32, y as i32, x as i32 + sr.width(), y as i32 + sr.height());
    let mut flags = 0;
    if trans & DDBLTFAST_SRCCOLORKEY != 0 {
        flags |= DDBLT_KEYSRC;
    }
    if trans & DDBLTFAST_DESTCOLORKEY != 0 {
        unimpl!("BltFast DESTCOLORKEY");
    }
    do_blt(this, Some(dr), src, Some(sr), flags, None)
}

unsafe extern "system" fn s_flip(this: This, _target: This, flags: u32) -> HRESULT {
    first_call!("IDirectDrawSurface7::Flip", "flags={flags:#x}");
    let sf = &*this;
    let back = match sf.kind {
        Kind::Primary => sf.chain.first().copied(),
        Kind::BackBuffer => Some(this),
        _ => None,
    };
    let Some(back) = back else { return DDERR_NOTFLIPPABLE };
    present(back, None, None);
    DD_OK
}

// ------------------------------------------------------------ simple getters

unsafe extern "system" fn s_add_overlay_dirty_rect(_this: This, _r: *mut RECT) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_enum_overlay_z_orders(_t: This, _f: u32, _c: *mut c_void, _cb: *mut c_void) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_get_blt_status(_this: This, _f: u32) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn s_get_caps(this: This, caps: *mut DDSCAPS2) -> HRESULT {
    if caps.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *caps = (*this).desc.ddsCaps;
    DD_OK
}
unsafe extern "system" fn s_get_clipper(this: This, out: *mut *mut Clipper) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let c = (*this).clipper;
    if c.is_null() {
        *out = null_mut();
        return DDERR_NOCLIPPERATTACHED;
    }
    (*c).refs += 1;
    *out = c;
    DD_OK
}
unsafe extern "system" fn s_get_color_key(this: This, flags: u32, key: *mut DDCOLORKEY) -> HRESULT {
    if key.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let k = if flags & DDCKEY_SRCBLT != 0 {
        (*this).ckey_src
    } else if flags & DDCKEY_DESTBLT != 0 {
        (*this).ckey_dest
    } else {
        None
    };
    match k {
        Some(k) => {
            *key = k;
            DD_OK
        }
        None => DDERR_NOCOLORKEY,
    }
}

unsafe fn release_dc_objects(dc: Dc) {
    use windows_sys::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, SelectObject};
    SelectObject(dc.hdc as _, dc.old as _);
    DeleteObject(dc.bitmap as _);
    DeleteDC(dc.hdc as _);
}

unsafe extern "system" fn s_get_dc(this: This, out: *mut isize) -> HRESULT {
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    first_call!("IDirectDrawSurface7::GetDC");
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let sf = &mut *this;
    if sf.dc.is_some() {
        return DDERR_SURFACEBUSY;
    }
    let target = if sf.kind == Kind::Primary { *sf.chain.first().unwrap_or(&this) } else { this };
    Surface::sync_to_cpu(target);
    let t = &mut *target;
    if !matches!(pixel::classify(t.pf()), Fmt::Masked { .. } | Fmt::Pal8) {
        return DDERR_UNSUPPORTED;
    }
    let (w, h) = (t.width(), t.height());
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w as i32,
        biHeight: -(h as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let hdc = CreateCompatibleDC(null_mut());
    let mut bits: *mut c_void = null_mut();
    let bmp = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
        return DDERR_GENERIC;
    }
    let old = SelectObject(hdc, bmp as _);
    let (pal, pa) = match t.palette_entries() {
        Some((p, a)) => (Some(p), a),
        None => (None, false),
    };
    let bgra = pixel::to_bgra8(&t.mem, t.pitch, w, h, t.pf(), pal, pa, None);
    std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
    sf.dc = Some(Dc { hdc: hdc as isize, bitmap: bmp as isize, old: old as isize, bits: bits as *mut u8 });
    *out = hdc as isize;
    DD_OK
}

unsafe extern "system" fn s_release_dc(this: This, _hdc: isize) -> HRESULT {
    use windows_sys::Win32::Graphics::Gdi::GdiFlush;
    let sf = &mut *this;
    let Some(dc) = sf.dc.take() else { return DDERR_NODC };
    GdiFlush();
    let target = if sf.kind == Kind::Primary { *sf.chain.first().unwrap_or(&this) } else { this };
    let t = &mut *target;
    let (w, h) = (t.width(), t.height());
    let bgra = std::slice::from_raw_parts(dc.bits, (w * h * 4) as usize).to_vec();
    let (pitch, pf) = (t.pitch, t.desc.ddpfPixelFormat);
    if matches!(pixel::classify(&pf), Fmt::Masked { .. }) {
        pixel::from_bgra8(&bgra, w, h, &mut t.mem, pitch, &pf);
        t.cpu_dirty = true;
        t.gpu_dirty = false;
    }
    release_dc_objects(dc);
    if sf.kind == Kind::Primary && world().is_fullscreen() {
        // GDI drawing onto the front buffer: show it.
        present(target, None, None);
    }
    DD_OK
}

unsafe extern "system" fn s_get_flip_status(_this: This, _f: u32) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn s_get_overlay_position(_this: This, _x: *mut i32, _y: *mut i32) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_get_palette(this: This, out: *mut *mut Palette) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let p = (*this).palette;
    if p.is_null() {
        *out = null_mut();
        return DDERR_NOPALETTEATTACHED;
    }
    (*p).refs += 1;
    *out = p;
    DD_OK
}
unsafe extern "system" fn s_get_pixel_format(this: This, pf: *mut DDPIXELFORMAT) -> HRESULT {
    if pf.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *pf = (*this).desc.ddpfPixelFormat;
    DD_OK
}
unsafe extern "system" fn s_get_surface_desc(this: This, desc: *mut DDSURFACEDESC2) -> HRESULT {
    if desc.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let sf = &*this;
    let mut d = sf.desc;
    d.dwFlags |= DDSD_CAPS | DDSD_WIDTH | DDSD_HEIGHT | DDSD_PIXELFORMAT;
    let (pitch, size) = pixel::level_layout(sf.pf(), sf.width(), sf.height());
    if matches!(pixel::classify(sf.pf()), Fmt::Dxt { .. }) {
        d.dwFlags = (d.dwFlags & !DDSD_PITCH) | DDSD_LINEARSIZE;
        d.lPitch = size as i32;
    } else {
        d.dwFlags |= DDSD_PITCH;
        d.lPitch = pitch as i32;
    }
    d.lpSurface = null_mut();
    copy_desc_out(&d, desc);
    DD_OK
}

unsafe fn copy_desc_out(d: &DDSURFACEDESC2, out: *mut DDSURFACEDESC2) {
    let size = ((*out).dwSize as usize).clamp(108, std::mem::size_of::<DDSURFACEDESC2>());
    std::ptr::copy_nonoverlapping(d as *const _ as *const u8, out as *mut u8, size);
    (*out).dwSize = size as u32;
}

unsafe extern "system" fn s_initialize(_this: This, _dd: *mut c_void, _d: *mut DDSURFACEDESC2) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_is_lost(_this: This) -> HRESULT {
    DD_OK
}

// ------------------------------------------------------------ lock

unsafe extern "system" fn s_lock(this: This, rect: *mut RECT, desc: *mut DDSURFACEDESC2, flags: u32, _ev: isize) -> HRESULT {
    if desc.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let target = if (*this).kind == Kind::Primary && !(*this).chain.is_empty() { (&(*this).chain)[0] } else { this };
    let sf = &mut *target;
    if sf.kind == Kind::ZBuffer {
        unimpl!("Lock of a z-buffer");
        return DDERR_UNSUPPORTED;
    }
    if sf.locked {
        return DDERR_SURFACEBUSY;
    }
    first_call!("IDirectDrawSurface7::Lock", "{:?} flags={flags:#x}", sf.kind);
    if flags & DDLOCK_DISCARDCONTENTS != 0 {
        Surface::ensure_mem(target);
        sf.gpu_dirty = false;
    } else {
        Surface::sync_to_cpu(target);
    }
    let (pitch, size) = pixel::level_layout(sf.pf(), sf.width(), sf.height());
    let mut off = 0usize;
    if !rect.is_null() {
        let r = *rect;
        let bpp = match pixel::classify(sf.pf()) {
            Fmt::Masked { bpp } | Fmt::Depth { bpp } => bpp,
            Fmt::Pal8 => 8,
            _ => 0,
        };
        if bpp > 0 {
            off = (r.top.max(0) as usize) * pitch as usize + (r.left.max(0) as usize) * (bpp as usize / 8);
        } else if let Fmt::Dxt { fourcc } = pixel::classify(sf.pf()) {
            off = (r.top.max(0) as usize / 4) * pitch as usize + (r.left.max(0) as usize / 4) * pixel::dxt_block_bytes(fourcc) as usize;
        }
        off = off.min(sf.mem.len().saturating_sub(1));
    }
    let mut d = sf.desc;
    d.dwFlags |= DDSD_LPSURFACE | DDSD_PITCH | DDSD_CAPS | DDSD_WIDTH | DDSD_HEIGHT | DDSD_PIXELFORMAT;
    if matches!(pixel::classify(sf.pf()), Fmt::Dxt { .. }) {
        d.dwFlags = (d.dwFlags & !DDSD_PITCH) | DDSD_LINEARSIZE;
        d.lPitch = size as i32;
    } else {
        d.lPitch = pitch as i32;
    }
    d.lpSurface = sf.mem.as_mut_ptr().add(off) as *mut c_void;
    copy_desc_out(&d, desc);
    sf.locked = true;
    sf.lock_readonly = flags & DDLOCK_READONLY != 0;
    DD_OK
}

unsafe extern "system" fn s_unlock(this: This, _rect: *mut RECT) -> HRESULT {
    let target = if (*this).kind == Kind::Primary && !(*this).chain.is_empty() { (&(*this).chain)[0] } else { this };
    let sf = &mut *target;
    if !sf.locked {
        return DDERR_NOTLOCKED;
    }
    sf.locked = false;
    if !sf.lock_readonly {
        sf.cpu_dirty = true;
        sf.gpu_dirty = false;
        sf.uniq = sf.uniq.wrapping_add(1);
    }
    if (*this).kind == Kind::Primary && world().is_fullscreen() && !sf.lock_readonly {
        // Direct writes to the front buffer become visible.
        present(target, None, None);
    }
    DD_OK
}

unsafe extern "system" fn s_restore(_this: This) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn s_set_clipper(this: This, c: *mut Clipper) -> HRESULT {
    let sf = &mut *this;
    if !sf.clipper.is_null() {
        Clipper::release(sf.clipper);
    }
    if !c.is_null() {
        (*c).refs += 1;
    }
    sf.clipper = c;
    DD_OK
}
unsafe extern "system" fn s_set_color_key(this: This, flags: u32, key: *mut DDCOLORKEY) -> HRESULT {
    let sf = &mut *this;
    let k = if key.is_null() { None } else { Some(*key) };
    if flags & DDCKEY_SRCBLT != 0 {
        sf.ckey_src = k;
        if let Some(k) = k {
            sf.desc.dwFlags |= DDSD_CKSRCBLT;
            sf.desc.ddckCKSrcBlt = k;
        } else {
            sf.desc.dwFlags &= !DDSD_CKSRCBLT;
        }
        KEY_VERSION += 1;
    } else if flags & DDCKEY_DESTBLT != 0 {
        sf.ckey_dest = k;
    } else {
        unimpl!("SetColorKey overlay keys");
    }
    DD_OK
}
unsafe extern "system" fn s_set_overlay_position(_this: This, _x: i32, _y: i32) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_set_palette(this: This, p: *mut Palette) -> HRESULT {
    let sf = &mut *this;
    if !sf.palette.is_null() {
        Palette::release(sf.palette);
    }
    if !p.is_null() {
        (*p).refs += 1;
    }
    sf.palette = p;
    for l in Surface::levels(sf.root) {
        if !(*l).mem.is_empty() {
            (*l).cpu_dirty = true;
        }
    }
    DD_OK
}
unsafe extern "system" fn s_update_overlay(_t: This, _a: *mut RECT, _b: This, _c: *mut RECT, _d: u32, _e: *mut c_void) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_update_overlay_display(_t: This, _f: u32) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_update_overlay_z_order(_t: This, _f: u32, _r: This) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_get_dd_interface(this: This, out: *mut *mut c_void) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let dd = (*this).ddraw;
    DirectDraw::add_ref(dd);
    *out = dd as *mut c_void;
    DD_OK
}
unsafe extern "system" fn s_page_lock(_t: This, _f: u32) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn s_page_unlock(_t: This, _f: u32) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn s_set_surface_desc(_t: This, _d: *mut DDSURFACEDESC2, _f: u32) -> HRESULT {
    unimpl!("IDirectDrawSurface7::SetSurfaceDesc");
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn s_set_private_data(this: This, g: *const GUID, data: *mut c_void, size: u32, flags: u32) -> HRESULT {
    if g.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    s_free_private_data(this, g);
    let bytes = if data.is_null() { Vec::new() } else { std::slice::from_raw_parts(data as *const u8, size as usize).to_vec() };
    const DDSPD_IUNKNOWNPOINTER: u32 = 4;
    if flags & DDSPD_IUNKNOWNPOINTER != 0 && bytes.len() >= 4 {
        let p = (bytes.as_ptr() as *const *mut *const usize).read_unaligned();
        if !p.is_null() {
            let add: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(*(*p).add(1));
            add(p as *mut c_void);
        }
    }
    (*this).private.push((*g, bytes, flags));
    DD_OK
}
unsafe extern "system" fn s_get_private_data(this: This, g: *const GUID, data: *mut c_void, size: *mut u32) -> HRESULT {
    if g.is_null() || size.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    for (k, v, _) in &(*this).private {
        if *k == *g {
            if (*size as usize) < v.len() || data.is_null() {
                *size = v.len() as u32;
                return 0x887600EAu32 as HRESULT; // DDERR_MOREDATA
            }
            std::ptr::copy_nonoverlapping(v.as_ptr(), data as *mut u8, v.len());
            *size = v.len() as u32;
            return DD_OK;
        }
    }
    DDERR_NOTFOUND
}
unsafe extern "system" fn s_free_private_data(this: This, g: *const GUID) -> HRESULT {
    if g.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let p = &mut (*this).private;
    if let Some(i) = p.iter().position(|(k, _, _)| *k == *g) {
        let (_, d, f) = p.remove(i);
        release_private(&d, f);
        return DD_OK;
    }
    DDERR_NOTFOUND
}
unsafe extern "system" fn s_get_uniqueness_value(this: This, v: *mut u32) -> HRESULT {
    if v.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *v = (*this).uniq;
    DD_OK
}
unsafe extern "system" fn s_change_uniqueness_value(this: This) -> HRESULT {
    (*this).uniq = (*this).uniq.wrapping_add(1);
    DD_OK
}
unsafe extern "system" fn s_set_priority(this: This, p: u32) -> HRESULT {
    (*this).priority = p;
    DD_OK
}
unsafe extern "system" fn s_get_priority(this: This, p: *mut u32) -> HRESULT {
    if !p.is_null() {
        *p = (*this).priority;
    }
    DD_OK
}
unsafe extern "system" fn s_set_lod(this: This, l: u32) -> HRESULT {
    (*this).lod = l;
    DD_OK
}
unsafe extern "system" fn s_get_lod(this: This, l: *mut u32) -> HRESULT {
    if !l.is_null() {
        *l = (*this).lod;
    }
    DD_OK
}

// ================================================================ palette

#[repr(C)]
pub struct Palette {
    vtbl: *const PaletteVtbl,
    pub refs: u32,
    pub caps: u32,
    pub entries: [PALETTEENTRY; 256],
    pub version: u64,
}

type PThis = *mut Palette;

#[repr(C)]
struct PaletteVtbl {
    query_interface: unsafe extern "system" fn(PThis, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(PThis) -> u32,
    release: unsafe extern "system" fn(PThis) -> u32,
    get_caps: unsafe extern "system" fn(PThis, *mut u32) -> HRESULT,
    get_entries: unsafe extern "system" fn(PThis, u32, u32, u32, *mut PALETTEENTRY) -> HRESULT,
    initialize: unsafe extern "system" fn(PThis, *mut c_void, u32, *mut PALETTEENTRY) -> HRESULT,
    set_entries: unsafe extern "system" fn(PThis, u32, u32, u32, *const PALETTEENTRY) -> HRESULT,
}

static PALETTE_VTBL: PaletteVtbl = PaletteVtbl {
    query_interface: p_query_interface,
    add_ref: p_add_ref,
    release: p_release,
    get_caps: p_get_caps,
    get_entries: p_get_entries,
    initialize: p_initialize,
    set_entries: p_set_entries,
};

static mut PALETTE_VERSION: u64 = 1;

fn palette_size(caps: u32) -> usize {
    if caps & DDPCAPS_1BIT != 0 {
        2
    } else if caps & DDPCAPS_2BIT != 0 {
        4
    } else if caps & DDPCAPS_4BIT != 0 {
        16
    } else {
        256
    }
}

impl Palette {
    pub unsafe fn create(caps: u32, entries: *const PALETTEENTRY) -> *mut Palette {
        let mut p = Palette { vtbl: &PALETTE_VTBL, refs: 1, caps, entries: [PALETTEENTRY::default(); 256], version: 0 };
        if !entries.is_null() {
            let n = palette_size(caps);
            std::ptr::copy_nonoverlapping(entries, p.entries.as_mut_ptr(), n);
        }
        PALETTE_VERSION += 1;
        p.version = PALETTE_VERSION;
        first_call!("IDirectDraw7::CreatePalette", "caps={caps:#x}");
        Box::into_raw(Box::new(p))
    }
    pub unsafe fn release(p: PThis) {
        p_release(p);
    }
}

unsafe extern "system" fn p_query_interface(this: PThis, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if !iid.is_null() && !out.is_null() && (*iid == IID_IDIRECTDRAWPALETTE || *iid == IID_IUNKNOWN) {
        (*this).refs += 1;
        *out = this as *mut c_void;
        return S_OK;
    }
    E_NOINTERFACE
}
unsafe extern "system" fn p_add_ref(this: PThis) -> u32 {
    (*this).refs += 1;
    (*this).refs
}
unsafe extern "system" fn p_release(this: PThis) -> u32 {
    (*this).refs -= 1;
    let r = (*this).refs;
    if r == 0 {
        drop(Box::from_raw(this));
    }
    r
}
unsafe extern "system" fn p_get_caps(this: PThis, caps: *mut u32) -> HRESULT {
    if !caps.is_null() {
        *caps = (*this).caps;
    }
    DD_OK
}
unsafe extern "system" fn p_get_entries(this: PThis, _f: u32, base: u32, n: u32, out: *mut PALETTEENTRY) -> HRESULT {
    if out.is_null() || base as usize + n as usize > 256 {
        return DDERR_INVALIDPARAMS;
    }
    std::ptr::copy_nonoverlapping((*this).entries.as_ptr().add(base as usize), out, n as usize);
    DD_OK
}
unsafe extern "system" fn p_initialize(_t: PThis, _d: *mut c_void, _f: u32, _e: *mut PALETTEENTRY) -> HRESULT {
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn p_set_entries(this: PThis, _f: u32, base: u32, n: u32, e: *const PALETTEENTRY) -> HRESULT {
    if e.is_null() || base as usize + n as usize > 256 {
        return DDERR_INVALIDPARAMS;
    }
    std::ptr::copy_nonoverlapping(e, (*this).entries.as_mut_ptr().add(base as usize), n as usize);
    PALETTE_VERSION += 1;
    (*this).version = PALETTE_VERSION;
    DD_OK
}

// ================================================================ clipper

#[repr(C)]
pub struct Clipper {
    vtbl: *const ClipperVtbl,
    pub refs: u32,
    pub hwnd: isize,
}
type CThis = *mut Clipper;

#[repr(C)]
struct ClipperVtbl {
    query_interface: unsafe extern "system" fn(CThis, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(CThis) -> u32,
    release: unsafe extern "system" fn(CThis) -> u32,
    get_clip_list: unsafe extern "system" fn(CThis, *mut RECT, *mut c_void, *mut u32) -> HRESULT,
    get_hwnd: unsafe extern "system" fn(CThis, *mut isize) -> HRESULT,
    initialize: unsafe extern "system" fn(CThis, *mut c_void, u32) -> HRESULT,
    is_clip_list_changed: unsafe extern "system" fn(CThis, *mut BOOL) -> HRESULT,
    set_clip_list: unsafe extern "system" fn(CThis, *mut c_void, u32) -> HRESULT,
    set_hwnd: unsafe extern "system" fn(CThis, u32, isize) -> HRESULT,
}

static CLIPPER_VTBL: ClipperVtbl = ClipperVtbl {
    query_interface: c_query_interface,
    add_ref: c_add_ref,
    release: c_release,
    get_clip_list: c_get_clip_list,
    get_hwnd: c_get_hwnd,
    initialize: c_initialize,
    is_clip_list_changed: c_is_clip_list_changed,
    set_clip_list: c_set_clip_list,
    set_hwnd: c_set_hwnd,
};

impl Clipper {
    pub fn create() -> *mut Clipper {
        Box::into_raw(Box::new(Clipper { vtbl: &CLIPPER_VTBL, refs: 1, hwnd: 0 }))
    }
    pub unsafe fn release(c: CThis) {
        c_release(c);
    }
}

unsafe extern "system" fn c_query_interface(this: CThis, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if !iid.is_null() && !out.is_null() && (*iid == IID_IDIRECTDRAWCLIPPER || *iid == IID_IUNKNOWN) {
        (*this).refs += 1;
        *out = this as *mut c_void;
        return S_OK;
    }
    E_NOINTERFACE
}
unsafe extern "system" fn c_add_ref(this: CThis) -> u32 {
    (*this).refs += 1;
    (*this).refs
}
unsafe extern "system" fn c_release(this: CThis) -> u32 {
    (*this).refs -= 1;
    let r = (*this).refs;
    if r == 0 {
        drop(Box::from_raw(this));
    }
    r
}
unsafe extern "system" fn c_get_clip_list(_t: CThis, _r: *mut RECT, _l: *mut c_void, _s: *mut u32) -> HRESULT {
    unimpl!("IDirectDrawClipper::GetClipList");
    DDERR_UNSUPPORTED
}
unsafe extern "system" fn c_get_hwnd(this: CThis, h: *mut isize) -> HRESULT {
    if !h.is_null() {
        *h = (*this).hwnd;
    }
    DD_OK
}
unsafe extern "system" fn c_initialize(_t: CThis, _d: *mut c_void, _f: u32) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn c_is_clip_list_changed(_t: CThis, b: *mut BOOL) -> HRESULT {
    if !b.is_null() {
        *b = 0;
    }
    DD_OK
}
unsafe extern "system" fn c_set_clip_list(_t: CThis, _l: *mut c_void, _f: u32) -> HRESULT {
    DD_OK
}
unsafe extern "system" fn c_set_hwnd(this: CThis, _f: u32, h: isize) -> HRESULT {
    (*this).hwnd = h;
    if world().hwnd == 0 {
        world().hwnd = h;
    }
    DD_OK
}

// ================================================================ gamma

#[repr(C)]
pub struct GammaControl {
    vtbl: *const GammaVtbl,
    surface: This,
}
type GThis = *mut GammaControl;

#[repr(C)]
struct GammaVtbl {
    query_interface: unsafe extern "system" fn(GThis, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(GThis) -> u32,
    release: unsafe extern "system" fn(GThis) -> u32,
    get_gamma_ramp: unsafe extern "system" fn(GThis, u32, *mut DDGAMMARAMP) -> HRESULT,
    set_gamma_ramp: unsafe extern "system" fn(GThis, u32, *const DDGAMMARAMP) -> HRESULT,
}

static GAMMA_VTBL: GammaVtbl = GammaVtbl {
    query_interface: g_query_interface,
    add_ref: g_add_ref,
    release: g_release,
    get_gamma_ramp: g_get_gamma_ramp,
    set_gamma_ramp: g_set_gamma_ramp,
};

unsafe extern "system" fn g_query_interface(this: GThis, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    s_query_interface((*this).surface, iid, out)
}
unsafe extern "system" fn g_add_ref(this: GThis) -> u32 {
    s_add_ref((*this).surface)
}
unsafe extern "system" fn g_release(this: GThis) -> u32 {
    s_release((*this).surface)
}
unsafe extern "system" fn g_get_gamma_ramp(_this: GThis, _f: u32, r: *mut DDGAMMARAMP) -> HRESULT {
    if r.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *r = world().gamma;
    DD_OK
}
unsafe extern "system" fn g_set_gamma_ramp(_this: GThis, _f: u32, r: *const DDGAMMARAMP) -> HRESULT {
    first_call!("IDirectDrawGammaControl::SetGammaRamp");
    let w = world();
    let ramp = if r.is_null() { DDGAMMARAMP::identity() } else { *r };
    let id = DDGAMMARAMP::identity();
    w.gamma_active = ramp.red != id.red || ramp.green != id.green || ramp.blue != id.blue;
    w.gamma = ramp;
    w.gamma_dirty = true;
    DD_OK
}
