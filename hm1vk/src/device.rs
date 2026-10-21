//! IDirect3DDevice7: render/texture stage state, transforms, lighting and the
//! translation of D3D7 draw calls into Vulkan draws.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::null_mut;

use ash::vk;

use crate::d3d::{device_desc, Direct3D};
use crate::ff::*;
use crate::pixel;
use crate::state::world;
use crate::surface::{Kind, Surface};
use crate::types::*;
use crate::vb::VertexBuffer;
use crate::vk::{vk_rect, BlendKey, Draw, ImageId, RasterState, RingSlice, SamplerKey, StencilState};
use crate::{debug, first_call, info, trace, unimpl};

type This = *mut Device;

#[derive(Clone, Default)]
struct StateBlock {
    rs: Vec<(u32, u32)>,
    tss: Vec<(u32, u32, u32)>,
    transforms: Vec<(u32, D3DMATRIX)>,
    viewport: Option<D3DVIEWPORT7>,
    material: Option<D3DMATERIAL7>,
    lights: Vec<(u32, D3DLIGHT7)>,
    light_enable: Vec<(u32, bool)>,
    textures: Vec<(u32, *mut Surface)>,
}

#[repr(C)]
pub struct Device {
    vtbl: *const DeviceVtbl,
    refs: u32,
    d3d: *mut Direct3D,
    guid: GUID,
    pub rt: *mut Surface,
    rs: [u32; rs::MAX],
    tss: [[u32; tss::MAX]; 8],
    textures: [*mut Surface; 8],
    world: [D3DMATRIX; 4],
    view: D3DMATRIX,
    proj: D3DMATRIX,
    texm: [D3DMATRIX; 8],
    viewport: D3DVIEWPORT7,
    material: D3DMATERIAL7,
    lights: HashMap<u32, (D3DLIGHT7, bool)>,
    clip_planes: [[f32; 4]; 32],
    clip_status: D3DCLIPSTATUS,
    in_scene: bool,
    scratch: Vec<CanonVertex>,
    vs_cache: Option<(u64, VsUniforms, RingSlice)>,
    fs_cache: Option<(u64, FsUniforms, RingSlice)>,
    blocks: HashMap<u32, StateBlock>,
    next_block: u32,
    recording: Option<StateBlock>,
    draws_this_frame: u32,
}

static mut CURRENT: This = null_mut();

#[repr(C)]
struct DeviceVtbl {
    query_interface: unsafe extern "system" fn(This, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(This) -> u32,
    release: unsafe extern "system" fn(This) -> u32,
    get_caps: unsafe extern "system" fn(This, *mut D3DDEVICEDESC7) -> HRESULT,
    enum_texture_formats: unsafe extern "system" fn(This, Option<LPD3DENUMPIXELFORMATSCALLBACK>, *mut c_void) -> HRESULT,
    begin_scene: unsafe extern "system" fn(This) -> HRESULT,
    end_scene: unsafe extern "system" fn(This) -> HRESULT,
    get_direct3d: unsafe extern "system" fn(This, *mut *mut Direct3D) -> HRESULT,
    set_render_target: unsafe extern "system" fn(This, *mut Surface, u32) -> HRESULT,
    get_render_target: unsafe extern "system" fn(This, *mut *mut Surface) -> HRESULT,
    clear: unsafe extern "system" fn(This, u32, *const D3DRECT, u32, u32, f32, u32) -> HRESULT,
    set_transform: unsafe extern "system" fn(This, u32, *const D3DMATRIX) -> HRESULT,
    get_transform: unsafe extern "system" fn(This, u32, *mut D3DMATRIX) -> HRESULT,
    set_viewport: unsafe extern "system" fn(This, *const D3DVIEWPORT7) -> HRESULT,
    multiply_transform: unsafe extern "system" fn(This, u32, *const D3DMATRIX) -> HRESULT,
    get_viewport: unsafe extern "system" fn(This, *mut D3DVIEWPORT7) -> HRESULT,
    set_material: unsafe extern "system" fn(This, *const D3DMATERIAL7) -> HRESULT,
    get_material: unsafe extern "system" fn(This, *mut D3DMATERIAL7) -> HRESULT,
    set_light: unsafe extern "system" fn(This, u32, *const D3DLIGHT7) -> HRESULT,
    get_light: unsafe extern "system" fn(This, u32, *mut D3DLIGHT7) -> HRESULT,
    set_render_state: unsafe extern "system" fn(This, u32, u32) -> HRESULT,
    get_render_state: unsafe extern "system" fn(This, u32, *mut u32) -> HRESULT,
    begin_state_block: unsafe extern "system" fn(This) -> HRESULT,
    end_state_block: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    pre_load: unsafe extern "system" fn(This, *mut Surface) -> HRESULT,
    draw_primitive: unsafe extern "system" fn(This, u32, u32, *const c_void, u32, u32) -> HRESULT,
    draw_indexed_primitive: unsafe extern "system" fn(This, u32, u32, *const c_void, u32, *const u16, u32, u32) -> HRESULT,
    set_clip_status: unsafe extern "system" fn(This, *const D3DCLIPSTATUS) -> HRESULT,
    get_clip_status: unsafe extern "system" fn(This, *mut D3DCLIPSTATUS) -> HRESULT,
    draw_primitive_strided: unsafe extern "system" fn(This, u32, u32, *const D3DDRAWPRIMITIVESTRIDEDDATA, u32, u32) -> HRESULT,
    draw_indexed_primitive_strided:
        unsafe extern "system" fn(This, u32, u32, *const D3DDRAWPRIMITIVESTRIDEDDATA, u32, *const u16, u32, u32) -> HRESULT,
    draw_primitive_vb: unsafe extern "system" fn(This, u32, *mut VertexBuffer, u32, u32, u32) -> HRESULT,
    draw_indexed_primitive_vb: unsafe extern "system" fn(This, u32, *mut VertexBuffer, u32, u32, *const u16, u32, u32) -> HRESULT,
    compute_sphere_visibility: unsafe extern "system" fn(This, *const D3DVECTOR, *const f32, u32, u32, *mut u32) -> HRESULT,
    get_texture: unsafe extern "system" fn(This, u32, *mut *mut Surface) -> HRESULT,
    set_texture: unsafe extern "system" fn(This, u32, *mut Surface) -> HRESULT,
    get_texture_stage_state: unsafe extern "system" fn(This, u32, u32, *mut u32) -> HRESULT,
    set_texture_stage_state: unsafe extern "system" fn(This, u32, u32, u32) -> HRESULT,
    validate_device: unsafe extern "system" fn(This, *mut u32) -> HRESULT,
    apply_state_block: unsafe extern "system" fn(This, u32) -> HRESULT,
    capture_state_block: unsafe extern "system" fn(This, u32) -> HRESULT,
    delete_state_block: unsafe extern "system" fn(This, u32) -> HRESULT,
    create_state_block: unsafe extern "system" fn(This, u32, *mut u32) -> HRESULT,
    load: unsafe extern "system" fn(This, *mut Surface, *mut c_void, *mut Surface, *mut RECT, u32) -> HRESULT,
    light_enable: unsafe extern "system" fn(This, u32, BOOL) -> HRESULT,
    get_light_enable: unsafe extern "system" fn(This, u32, *mut BOOL) -> HRESULT,
    set_clip_plane: unsafe extern "system" fn(This, u32, *const f32) -> HRESULT,
    get_clip_plane: unsafe extern "system" fn(This, u32, *mut f32) -> HRESULT,
    get_info: unsafe extern "system" fn(This, u32, *mut c_void, u32) -> HRESULT,
}

static DEVICE_VTBL: DeviceVtbl = DeviceVtbl {
    query_interface: d_query_interface,
    add_ref: d_add_ref,
    release: d_release,
    get_caps: d_get_caps,
    enum_texture_formats: d_enum_texture_formats,
    begin_scene: d_begin_scene,
    end_scene: d_end_scene,
    get_direct3d: d_get_direct3d,
    set_render_target: d_set_render_target,
    get_render_target: d_get_render_target,
    clear: d_clear,
    set_transform: d_set_transform,
    get_transform: d_get_transform,
    set_viewport: d_set_viewport,
    multiply_transform: d_multiply_transform,
    get_viewport: d_get_viewport,
    set_material: d_set_material,
    get_material: d_get_material,
    set_light: d_set_light,
    get_light: d_get_light,
    set_render_state: d_set_render_state,
    get_render_state: d_get_render_state,
    begin_state_block: d_begin_state_block,
    end_state_block: d_end_state_block,
    pre_load: d_pre_load,
    draw_primitive: d_draw_primitive,
    draw_indexed_primitive: d_draw_indexed_primitive,
    set_clip_status: d_set_clip_status,
    get_clip_status: d_get_clip_status,
    draw_primitive_strided: d_draw_primitive_strided,
    draw_indexed_primitive_strided: d_draw_indexed_primitive_strided,
    draw_primitive_vb: d_draw_primitive_vb,
    draw_indexed_primitive_vb: d_draw_indexed_primitive_vb,
    compute_sphere_visibility: d_compute_sphere_visibility,
    get_texture: d_get_texture,
    set_texture: d_set_texture,
    get_texture_stage_state: d_get_texture_stage_state,
    set_texture_stage_state: d_set_texture_stage_state,
    validate_device: d_validate_device,
    apply_state_block: d_apply_state_block,
    capture_state_block: d_capture_state_block,
    delete_state_block: d_delete_state_block,
    create_state_block: d_create_state_block,
    load: d_load,
    light_enable: d_light_enable,
    get_light_enable: d_get_light_enable,
    set_clip_plane: d_set_clip_plane,
    get_clip_plane: d_get_clip_plane,
    get_info: d_get_info,
};

fn default_rs(has_z: bool) -> [u32; rs::MAX] {
    let mut r = [0u32; rs::MAX];
    r[rs::TEXTUREPERSPECTIVE as usize] = 1;
    r[rs::ZENABLE as usize] = has_z as u32;
    r[rs::FILLMODE as usize] = D3DFILL_SOLID;
    r[rs::SHADEMODE as usize] = 2;
    r[rs::ZWRITEENABLE as usize] = 1;
    r[rs::LASTPIXEL as usize] = 1;
    r[rs::SRCBLEND as usize] = D3DBLEND_ONE;
    r[rs::DESTBLEND as usize] = D3DBLEND_ZERO;
    r[rs::CULLMODE as usize] = D3DCULL_CCW;
    r[rs::ZFUNC as usize] = D3DCMP_LESSEQUAL;
    r[rs::ALPHAFUNC as usize] = D3DCMP_ALWAYS;
    r[rs::FOGEND as usize] = 1.0f32.to_bits();
    r[rs::FOGDENSITY as usize] = 1.0f32.to_bits();
    r[rs::STENCILFAIL as usize] = D3DSTENCILOP_KEEP;
    r[rs::STENCILZFAIL as usize] = D3DSTENCILOP_KEEP;
    r[rs::STENCILPASS as usize] = D3DSTENCILOP_KEEP;
    r[rs::STENCILFUNC as usize] = D3DCMP_ALWAYS;
    r[rs::STENCILMASK as usize] = 0xFFFF_FFFF;
    r[rs::STENCILWRITEMASK as usize] = 0xFFFF_FFFF;
    r[rs::TEXTUREFACTOR as usize] = 0xFFFF_FFFF;
    r[rs::CLIPPING as usize] = 1;
    r[rs::LIGHTING as usize] = 1;
    r[rs::COLORVERTEX as usize] = 1;
    r[rs::LOCALVIEWER as usize] = 1;
    r[rs::DIFFUSEMATERIALSOURCE as usize] = D3DMCS_COLOR1;
    r[rs::SPECULARMATERIALSOURCE as usize] = D3DMCS_COLOR2;
    r
}

fn default_tss() -> [[u32; tss::MAX]; 8] {
    let mut t = [[0u32; tss::MAX]; 8];
    for (s, st) in t.iter_mut().enumerate() {
        st[tss::COLOROP as usize] = if s == 0 { top::MODULATE } else { top::DISABLE };
        st[tss::COLORARG1 as usize] = D3DTA_TEXTURE;
        st[tss::COLORARG2 as usize] = D3DTA_CURRENT;
        st[tss::ALPHAOP as usize] = if s == 0 { top::SELECTARG1 } else { top::DISABLE };
        st[tss::ALPHAARG1 as usize] = D3DTA_TEXTURE;
        st[tss::ALPHAARG2 as usize] = D3DTA_CURRENT;
        st[tss::TEXCOORDINDEX as usize] = s as u32;
        st[tss::ADDRESS as usize] = D3DTADDRESS_WRAP;
        st[tss::ADDRESSU as usize] = D3DTADDRESS_WRAP;
        st[tss::ADDRESSV as usize] = D3DTADDRESS_WRAP;
        st[tss::MAGFILTER as usize] = D3DTFG_POINT;
        st[tss::MINFILTER as usize] = D3DTFN_POINT;
        st[tss::MIPFILTER as usize] = D3DTFP_NONE;
        st[tss::MAXANISOTROPY as usize] = 1;
    }
    t
}

impl Device {
    pub unsafe fn create(d3d: *mut Direct3D, guid: GUID, rt: *mut Surface) -> This {
        Surface::add_ref(rt);
        Direct3D::add_ref(d3d);
        let has_z = !(*rt).zbuffer.is_null();
        let dev = Box::into_raw(Box::new(Device {
            vtbl: &DEVICE_VTBL,
            refs: 1,
            d3d,
            guid,
            rt,
            rs: default_rs(has_z),
            tss: default_tss(),
            textures: [null_mut(); 8],
            world: [D3DMATRIX::identity(); 4],
            view: D3DMATRIX::identity(),
            proj: D3DMATRIX::identity(),
            texm: [D3DMATRIX::identity(); 8],
            viewport: D3DVIEWPORT7 { dwX: 0, dwY: 0, dwWidth: (*rt).width(), dwHeight: (*rt).height(), dvMinZ: 0.0, dvMaxZ: 1.0 },
            material: D3DMATERIAL7::default(),
            lights: HashMap::new(),
            clip_planes: [[0.0; 4]; 32],
            clip_status: D3DCLIPSTATUS::default(),
            in_scene: false,
            scratch: Vec::with_capacity(4096),
            vs_cache: None,
            fs_cache: None,
            blocks: HashMap::new(),
            next_block: 1,
            recording: None,
            draws_this_frame: 0,
        }));
        CURRENT = dev;
        dev
    }

    /// Sets the projection (logged at LogLevel 1, see fov.rs).
    unsafe fn set_projection(&mut self, m: D3DMATRIX) {
        crate::fov::on_projection(&m, self.viewport.dwHeight);
        self.proj = m;
    }

    /// world * view * projection * viewport mapping (used by ProcessVertices).
    pub fn world_view_proj_viewport(&self) -> D3DMATRIX {
        let vp = &self.viewport;
        let (w, h) = (vp.dwWidth as f32, vp.dwHeight as f32);
        let mut m = D3DMATRIX::identity();
        m.m[0][0] = w / 2.0;
        m.m[1][1] = -h / 2.0;
        m.m[2][2] = vp.dvMaxZ - vp.dvMinZ;
        m.m[3][0] = vp.dwX as f32 + w / 2.0;
        m.m[3][1] = vp.dwY as f32 + h / 2.0;
        m.m[3][2] = vp.dvMinZ;
        self.world[0].mul(&self.view).mul(&self.proj).mul(&m)
    }

    fn r(&self, s: u32) -> u32 {
        self.rs[s as usize]
    }
    fn rf(&self, s: u32) -> f32 {
        f32::from_bits(self.rs[s as usize])
    }
    fn t(&self, stage: usize, s: u32) -> u32 {
        self.tss[stage][s as usize]
    }

    fn active_stages(&self) -> usize {
        (0..8).find(|&s| self.t(s, tss::COLOROP) == top::DISABLE).unwrap_or(8)
    }

    fn sampler_key(&self, stage: usize, levels: u32) -> SamplerKey {
        let mag = self.t(stage, tss::MAGFILTER);
        let min = self.t(stage, tss::MINFILTER);
        let mip = self.t(stage, tss::MIPFILTER);
        let aniso = mag == D3DTFG_ANISOTROPIC || min == D3DTFN_ANISOTROPIC;
        let border = {
            let c = self.t(stage, tss::BORDERCOLOR);
            if c >> 24 == 0 {
                0
            } else if ((c >> 16) & 0xFF) + ((c >> 8) & 0xFF) + (c & 0xFF) > 384 {
                2
            } else {
                1
            }
        };
        SamplerKey {
            mag_linear: mag != D3DTFG_POINT,
            min_linear: min != D3DTFN_POINT,
            mip: if levels <= 1 {
                0
            } else {
                match mip {
                    D3DTFP_POINT => 1,
                    D3DTFP_LINEAR => 2,
                    _ => 0,
                }
            },
            address_u: self.t(stage, tss::ADDRESSU).clamp(1, 4) as u8,
            address_v: self.t(stage, tss::ADDRESSV).clamp(1, 4) as u8,
            anisotropy: if aniso { self.t(stage, tss::MAXANISOTROPY).clamp(1, 16) as u8 } else { 1 },
            lod_bias: self.t(stage, tss::MIPMAPLODBIAS),
            max_mip_level: self.t(stage, tss::MAXMIPLEVEL).min(levels.saturating_sub(1)) as u8,
            border,
        }
    }

    fn vs_uniforms(&self, l: &FvfLayout, rt_w: u32, rt_h: u32) -> VsUniforms {
        let mut u = VsUniforms::default();
        let wv = self.world[0].mul(&self.view);
        u.world_view = wv.m;
        u.proj = self.proj.m;
        u.normal_mat = wv.inverse().transpose().m;
        for s in 0..4 {
            u.tex_mat[s] = self.texm[s].m;
            u.tci[s] = self.t(s, tss::TEXCOORDINDEX);
            u.ttff[s] = self.t(s, tss::TEXTURETRANSFORMFLAGS);
        }
        u.mat_diffuse = color_value(&self.material.diffuse);
        u.mat_ambient = color_value(&self.material.ambient);
        let sp = color_value(&self.material.specular);
        u.mat_specular = [sp[0], sp[1], sp[2], self.material.power];
        u.mat_emissive = color_value(&self.material.emissive);
        u.global_ambient = argb(self.r(rs::AMBIENT));
        let fog_mode = if self.r(rs::FOGENABLE) != 0 && self.r(rs::FOGTABLEMODE) != D3DFOG_NONE {
            VS_FOG_TABLE
        } else {
            self.r(rs::FOGVERTEXMODE) as f32
        };
        u.fog = [self.rf(rs::FOGSTART), self.rf(rs::FOGEND), self.rf(rs::FOGDENSITY), fog_mode];
        let vw = self.viewport.dwWidth.max(1) as f32;
        let vh = self.viewport.dwHeight.max(1) as f32;
        u.rt = [2.0 / rt_w.max(1) as f32, 2.0 / rt_h.max(1) as f32, 1.0 / vw, 1.0 / vh];
        let mut f = 0;
        if l.rhw {
            f |= VF_RHW;
        }
        if l.normal.is_some() {
            f |= VF_NORMAL;
        }
        if l.diffuse.is_some() {
            f |= VF_DIFFUSE;
        }
        if l.specular.is_some() {
            f |= VF_SPECULAR;
        }
        if self.r(rs::LIGHTING) != 0 && !l.rhw {
            f |= VF_LIGHTING;
        }
        if self.r(rs::NORMALIZENORMALS) != 0 {
            f |= VF_NORMALIZE;
        }
        if self.r(rs::LOCALVIEWER) != 0 {
            f |= VF_LOCALVIEWER;
        }
        if self.r(rs::COLORVERTEX) != 0 {
            f |= VF_COLORVERTEX;
        }
        if self.r(rs::SPECULARENABLE) != 0 {
            f |= VF_SPECULARENABLE;
        }
        if self.r(rs::RANGEFOGENABLE) != 0 {
            f |= VF_RANGEFOG;
        }
        let srcs = (self.r(rs::DIFFUSEMATERIALSOURCE) & 3)
            | (self.r(rs::SPECULARMATERIALSOURCE) & 3) << 2
            | (self.r(rs::AMBIENTMATERIALSOURCE) & 3) << 4
            | (self.r(rs::EMISSIVEMATERIALSOURCE) & 3) << 6;
        let mut count = 0usize;
        if f & VF_LIGHTING != 0 {
            let mut idx: Vec<_> = self.lights.iter().filter(|(_, (_, e))| *e).map(|(k, _)| *k).collect();
            idx.sort();
            for k in idx.into_iter().take(8) {
                let (l7, _) = self.lights[&k];
                let p = [l7.dvPosition.x, l7.dvPosition.y, l7.dvPosition.z, 1.0];
                let d = [l7.dvDirection.x, l7.dvDirection.y, l7.dvDirection.z, 0.0];
                let tp = transform_row(&p, &self.view);
                let td = transform_row(&d, &self.view);
                u.lights[count] = LightU {
                    diffuse: color_value(&l7.dcvDiffuse),
                    specular: color_value(&l7.dcvSpecular),
                    ambient: color_value(&l7.dcvAmbient),
                    position: [tp[0], tp[1], tp[2], l7.dltType as f32],
                    direction: [td[0], td[1], td[2], if l7.dvRange > 0.0 { l7.dvRange } else { f32::MAX }],
                    atten: [l7.dvAttenuation0, l7.dvAttenuation1, l7.dvAttenuation2, l7.dvFalloff],
                    spot: [(l7.dvTheta * 0.5).cos(), (l7.dvPhi * 0.5).cos(), 0.0, 0.0],
                };
                count += 1;
            }
        }
        u.flags = [f, srcs, count as u32, 0];
        u
    }

    fn fs_uniforms(&self, bound: &[bool; 4]) -> FsUniforms {
        let mut u = FsUniforms::default();
        for s in 0..8 {
            u.stages[s].op = [
                self.t(s, tss::COLOROP),
                self.t(s, tss::COLORARG1),
                self.t(s, tss::COLORARG2),
                self.t(s, tss::ALPHAOP),
            ];
            let slot = if s < 4 && bound[s] { s as u32 } else { 0xFFFF_FFFF };
            u.stages[s].op2 = [self.t(s, tss::ALPHAARG1), self.t(s, tss::ALPHAARG2), slot, 0];
        }
        u.tfactor = argb(self.r(rs::TEXTUREFACTOR));
        u.fog_color = argb(self.r(rs::FOGCOLOR) | 0xFF00_0000);
        u.fog = [self.rf(rs::FOGSTART), self.rf(rs::FOGEND), self.rf(rs::FOGDENSITY), 0.0];
        let fog_mode = if self.r(rs::FOGENABLE) == 0 {
            0
        } else if self.r(rs::FOGTABLEMODE) != D3DFOG_NONE {
            1
        } else {
            2
        };
        let mut flags = 0;
        if self.r(rs::SPECULARENABLE) != 0 {
            flags |= FF_SPECULAR;
        }
        if self.r(rs::ALPHATESTENABLE) != 0 {
            flags |= FF_ALPHATEST;
        }
        if self.r(rs::COLORKEYENABLE) != 0 && bound[0] {
            let t0 = self.textures[0];
            if !t0.is_null() && unsafe { (*(*t0).root).ckey_src.is_some() } {
                flags |= FF_COLORKEY;
            }
        }
        u.mode = [self.r(rs::ALPHAFUNC), fog_mode, self.r(rs::FOGTABLEMODE), flags];
        u.alpha_ref = [(self.r(rs::ALPHAREF) & 0xFF) as f32, 0.0, 0.0, 0.0];
        u
    }

    fn raster(&self, prim: u32, rhw: bool, rt_w: u32, rt_h: u32, rt_alpha: bool, ui_vp: Option<[f32; 4]>) -> RasterState {
        let blend = if self.r(rs::ALPHABLENDENABLE) != 0 {
            let (mut s, mut d) = (self.r(rs::SRCBLEND), self.r(rs::DESTBLEND));
            if s == D3DBLEND_BOTHSRCALPHA {
                s = D3DBLEND_SRCALPHA;
                d = D3DBLEND_INVSRCALPHA;
            } else if s == D3DBLEND_BOTHINVSRCALPHA {
                s = D3DBLEND_INVSRCALPHA;
                d = D3DBLEND_SRCALPHA;
            }
            Some(BlendKey { src: blend_factor(s, rt_alpha), dst: blend_factor(d, rt_alpha) })
        } else {
            None
        };
        let cull = match self.r(rs::CULLMODE) {
            D3DCULL_NONE => vk::CullModeFlags::NONE,
            D3DCULL_CW => vk::CullModeFlags::FRONT,
            _ => vk::CullModeFlags::BACK,
        };
        let polygon = match self.r(rs::FILLMODE) {
            D3DFILL_POINT => vk::PolygonMode::POINT,
            D3DFILL_WIREFRAME => vk::PolygonMode::LINE,
            _ => vk::PolygonMode::FILL,
        };
        let stencil = if self.r(rs::STENCILENABLE) != 0 {
            Some(StencilState {
                fail: stencil_op(self.r(rs::STENCILFAIL)),
                zfail: stencil_op(self.r(rs::STENCILZFAIL)),
                pass: stencil_op(self.r(rs::STENCILPASS)),
                cmp: compare_op(self.r(rs::STENCILFUNC)),
                reference: self.r(rs::STENCILREF),
                read_mask: self.r(rs::STENCILMASK),
                write_mask: self.r(rs::STENCILWRITEMASK),
            })
        } else {
            None
        };
        let vp = &self.viewport;
        let viewport = if rhw {
            vk::Viewport { x: 0.0, y: 0.0, width: rt_w as f32, height: rt_h as f32, min_depth: 0.0, max_depth: 1.0 }
        } else {
            vk::Viewport {
                x: vp.dwX as f32,
                y: vp.dwY as f32,
                width: vp.dwWidth.max(1) as f32,
                height: vp.dwHeight.max(1) as f32,
                min_depth: vp.dvMinZ.clamp(0.0, 1.0),
                max_depth: vp.dvMaxZ.clamp(0.0, 1.0),
            }
        };
        // UI zoom (ui.rs): the viewport scaled about its anchor; clip to it.
        let (viewport, (x0, y0, x1, y1)) = match ui_vp {
            Some(v) if !rhw => (
                vk::Viewport { x: v[0], y: v[1], width: v[2], height: v[3], ..viewport },
                (v[0].floor() as i32, v[1].floor() as i32, (v[0] + v[2]).ceil() as i32, (v[1] + v[3]).ceil() as i32),
            ),
            _ => (viewport, (vp.dwX as i32, vp.dwY as i32, (vp.dwX + vp.dwWidth) as i32, (vp.dwY + vp.dwHeight) as i32)),
        };
        let x0 = x0.clamp(0, rt_w as i32);
        let y0 = y0.clamp(0, rt_h as i32);
        let x1 = x1.clamp(0, rt_w as i32);
        let y1 = y1.clamp(0, rt_h as i32);
        let topology = match prim {
            D3DPT_POINTLIST => vk::PrimitiveTopology::POINT_LIST,
            D3DPT_LINELIST => vk::PrimitiveTopology::LINE_LIST,
            D3DPT_LINESTRIP => vk::PrimitiveTopology::LINE_STRIP,
            D3DPT_TRIANGLESTRIP => vk::PrimitiveTopology::TRIANGLE_STRIP,
            D3DPT_TRIANGLEFAN => vk::PrimitiveTopology::TRIANGLE_FAN,
            _ => vk::PrimitiveTopology::TRIANGLE_LIST,
        };
        RasterState {
            blend,
            color_write: true,
            cull,
            topology,
            polygon,
            depth_test: self.r(rs::ZENABLE) != D3DZB_FALSE,
            depth_write: self.r(rs::ZWRITEENABLE) != 0,
            depth_cmp: compare_op(self.r(rs::ZFUNC)),
            depth_bias: -(self.r(rs::ZBIAS).min(16) as f32) * 4.0,
            stencil,
            viewport,
            scissor: vk_rect(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32),
        }
    }

    /// Images of the current render target and its z-buffer, uploading any
    /// CPU-side changes first.
    unsafe fn targets(&mut self) -> Option<(ImageId, Option<ImageId>, u32, u32, bool)> {
        let rt = self.rt;
        if rt.is_null() {
            return None;
        }
        let rid = Surface::sync_to_gpu(rt)?;
        let z = (*rt).zbuffer;
        let zid = if z.is_null() { None } else { Surface::image(z) };
        let rt_alpha = (*rt).pf().dwRGBAlphaBitMask != 0;
        Some((rid, zid, (*rt).width(), (*rt).height(), rt_alpha))
    }

    /// Core draw path shared by all DrawPrimitive variants. `fill` writes the
    /// canonical vertices into `self.scratch`.
    unsafe fn draw(&mut self, prim: u32, layout: &FvfLayout, vcount: u32, indices: Option<&[u16]>) -> HRESULT {
        if vcount == 0 || self.scratch.is_empty() {
            return D3D_OK;
        }
        if let Some(ix) = indices {
            if ix.is_empty() {
                return D3D_OK;
            }
        }
        if prim == D3DPT_POINTLIST {
            first_call!("draw POINTLIST");
        }
        self.draws_this_frame += 1;
        let Some((rid, zid, rw, rh, rt_alpha)) = self.targets() else { return D3D_OK };
        // UI zoom (ui.rs): scale draws made with the engine's UI camera.
        let mut ui_vp = None;
        if matches!((*self.rt).kind, Kind::BackBuffer | Kind::Primary) && !layout.rhw {
            let ui = crate::ui::is_ui_camera(&self.proj, &self.view);
            let b = self.projected_bounds();
            // The game's mouse cursor: one 16x16 textured quad at depth 11.
            let t = self.textures[0];
            let cursor = ui
                && vcount == 4
                && !t.is_null()
                && (*t).width() == 16
                && (*t).height() == 16
                && self.scratch.iter().all(|v| v.pos[2] == 11.0);
            // Interface elements are flat (every vertex at the same depth);
            // the UI camera is also used for 3D models such as Hitman's body
            // on the death screen, which must not be treated as HUD pieces.
            let flat = !ui || self.flat_in_view();
            let big_mesh = ui && !flat && (b[2] - b[0] > rw as f32 / 5.0 || b[3] - b[1] > rh as f32 / 5.0);
            crate::ui::note_draw(ui, b, cursor, big_mesh);
            if ui {
                let vp = &self.viewport;
                ui_vp = crate::ui::scaled_viewport(
                    b,
                    (vp.dwX as f32, vp.dwY as f32, vp.dwWidth as f32, vp.dwHeight as f32),
                    (rw as f32, rh as f32),
                    flat,
                );
            }
        }

        // Textures (uploads may break the pass, so they happen first).
        let active = self.active_stages();
        let mut bound = [false; 4];
        let white = world().gpu().map(|g| g.white).unwrap_or(ImageId(0));
        let mut texs = [(white, SamplerKey::default()); 4];
        for (s, slot) in texs.iter_mut().enumerate() {
            let t = self.textures[s];
            if s >= active || t.is_null() || t == self.rt {
                continue;
            }
            if let Some(id) = Surface::sync_to_gpu(t) {
                let levels = 1 + (*(*t).root).chain.len() as u32;
                *slot = (id, self.sampler_key(s, levels));
                bound[s] = true;
            }
        }

        let vsu = self.vs_uniforms(layout, rw, rh);
        let fsu = self.fs_uniforms(&bound);
        let raster = self.raster(prim, layout.rhw, rw, rh, rt_alpha, ui_vp);

        let Some(g) = world().gpu() else { return D3D_OK };
        let align = g.ubo_align;
        let vbytes = slice_bytes(&self.scratch);
        let ibytes = indices.map(slice_bytes).unwrap_or(&[]);
        let need = vbytes.len() as u64 + ibytes.len() as u64 + 2 * (2048 + align) + 64;
        g.ring_reserve(need);
        let epoch = g.epoch;
        let vs_slice = match &self.vs_cache {
            Some((e, u, s)) if *e == epoch && *u == vsu => *s,
            _ => {
                let s = g.ring_upload(as_bytes(&vsu), align);
                self.vs_cache = Some((epoch, vsu, s));
                s
            }
        };
        let fs_slice = match &self.fs_cache {
            Some((e, u, s)) if *e == epoch && *u == fsu => *s,
            _ => {
                let s = g.ring_upload(as_bytes(&fsu), align);
                self.fs_cache = Some((epoch, fsu, s));
                s
            }
        };
        let vslice = g.ring_upload(vbytes, 16);
        let islice = if ibytes.is_empty() { None } else { Some(g.ring_upload(ibytes, 4)) };
        if !g.begin_pass(Some(rid), zid) {
            return D3D_OK;
        }
        g.draw(&Draw {
            raster,
            vs_ubo: vs_slice,
            fs_ubo: fs_slice,
            textures: texs,
            vertices: vslice,
            vertex_count: vcount,
            indices: islice,
            index_count: indices.map(|i| i.len() as u32).unwrap_or(0),
            _marker: std::marker::PhantomData,
        });
        Surface::mark_gpu_written(self.rt);
        D3D_OK
    }

    /// True if all vertices in `scratch` lie at the same camera depth (a flat
    /// interface element rather than a 3D model).
    fn flat_in_view(&self) -> bool {
        let m = self.world[0].mul(&self.view);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for v in &self.scratch {
            let z = v.pos[0] * m.m[0][2] + v.pos[1] * m.m[1][2] + v.pos[2] * m.m[2][2] + m.m[3][2];
            lo = lo.min(z);
            hi = hi.max(z);
        }
        hi - lo <= 0.01 + hi.abs() * 1e-4
    }

    /// Screen-space bounds of the transformed vertices in `scratch` (used by
    /// the UI zoom). Large meshes and geometry crossing the camera plane
    /// count as covering the whole viewport.
    fn projected_bounds(&self) -> [f32; 4] {
        let vp = &self.viewport;
        let full = [vp.dwX as f32, vp.dwY as f32, (vp.dwX + vp.dwWidth) as f32, (vp.dwY + vp.dwHeight) as f32];
        if self.scratch.len() > 2048 {
            return full;
        }
        let m = self.world[0].mul(&self.view).mul(&self.proj);
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for v in &self.scratch {
            let c = transform_row(&[v.pos[0], v.pos[1], v.pos[2], 1.0], &m);
            if c[3] <= 1e-6 {
                return full;
            }
            let x = vp.dwX as f32 + (c[0] / c[3] + 1.0) * 0.5 * vp.dwWidth as f32;
            let y = vp.dwY as f32 + (1.0 - c[1] / c[3]) * 0.5 * vp.dwHeight as f32;
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
        }
        [b[0].max(full[0]), b[1].max(full[1]), b[2].min(full[2]), b[3].min(full[3])]
    }

    fn record(&mut self, f: impl FnOnce(&mut StateBlock)) -> bool {
        if let Some(b) = self.recording.as_mut() {
            f(b);
            true
        } else {
            false
        }
    }
}

fn transform_row(v: &[f32; 4], m: &D3DMATRIX) -> [f32; 4] {
    let mut o = [0f32; 4];
    for (c, oc) in o.iter_mut().enumerate() {
        *oc = (0..4).map(|k| v[k] * m.m[k][c]).sum();
    }
    o
}

fn blend_factor(b: u32, rt_alpha: bool) -> vk::BlendFactor {
    match b {
        D3DBLEND_ZERO => vk::BlendFactor::ZERO,
        D3DBLEND_ONE => vk::BlendFactor::ONE,
        D3DBLEND_SRCCOLOR => vk::BlendFactor::SRC_COLOR,
        D3DBLEND_INVSRCCOLOR => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        D3DBLEND_SRCALPHA => vk::BlendFactor::SRC_ALPHA,
        D3DBLEND_INVSRCALPHA => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        D3DBLEND_DESTALPHA if rt_alpha => vk::BlendFactor::DST_ALPHA,
        D3DBLEND_DESTALPHA => vk::BlendFactor::ONE,
        D3DBLEND_INVDESTALPHA if rt_alpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        D3DBLEND_INVDESTALPHA => vk::BlendFactor::ZERO,
        D3DBLEND_DESTCOLOR => vk::BlendFactor::DST_COLOR,
        D3DBLEND_INVDESTCOLOR => vk::BlendFactor::ONE_MINUS_DST_COLOR,
        D3DBLEND_SRCALPHASAT => vk::BlendFactor::SRC_ALPHA_SATURATE,
        _ => vk::BlendFactor::ONE,
    }
}

fn compare_op(c: u32) -> vk::CompareOp {
    match c {
        D3DCMP_NEVER => vk::CompareOp::NEVER,
        D3DCMP_LESS => vk::CompareOp::LESS,
        D3DCMP_EQUAL => vk::CompareOp::EQUAL,
        D3DCMP_LESSEQUAL => vk::CompareOp::LESS_OR_EQUAL,
        D3DCMP_GREATER => vk::CompareOp::GREATER,
        D3DCMP_NOTEQUAL => vk::CompareOp::NOT_EQUAL,
        D3DCMP_GREATEREQUAL => vk::CompareOp::GREATER_OR_EQUAL,
        _ => vk::CompareOp::ALWAYS,
    }
}

fn stencil_op(o: u32) -> vk::StencilOp {
    match o {
        D3DSTENCILOP_ZERO => vk::StencilOp::ZERO,
        D3DSTENCILOP_REPLACE => vk::StencilOp::REPLACE,
        D3DSTENCILOP_INCRSAT => vk::StencilOp::INCREMENT_AND_CLAMP,
        D3DSTENCILOP_DECRSAT => vk::StencilOp::DECREMENT_AND_CLAMP,
        D3DSTENCILOP_INVERT => vk::StencilOp::INVERT,
        D3DSTENCILOP_INCR => vk::StencilOp::INCREMENT_AND_WRAP,
        D3DSTENCILOP_DECR => vk::StencilOp::DECREMENT_AND_WRAP,
        _ => vk::StencilOp::KEEP,
    }
}

const D3D_OK: HRESULT = 0;

/// Called when a surface is destroyed so the device never keeps a dangling pointer.
pub unsafe fn forget_surface(s: *mut Surface) {
    let d = CURRENT;
    if d.is_null() {
        return;
    }
    for t in (*d).textures.iter_mut() {
        if *t == s {
            *t = null_mut();
        }
    }
    if (*d).rt == s {
        (*d).rt = null_mut();
    }
}

/// Draws a textured quad (used for colour-keyed / compressed-source blits to
/// render targets).
#[allow(clippy::too_many_arguments)]
pub unsafe fn draw_blit_quad(
    dst: ImageId,
    dst_size: (u32, u32),
    drect: RECT,
    src: ImageId,
    src_size: (u32, u32),
    srect: RECT,
    colorkey: bool,
    linear: bool,
) {
    let Some(g) = world().gpu() else { return };
    let (dw, dh) = (dst_size.0.max(1), dst_size.1.max(1));
    let mut vs = VsUniforms::default();
    vs.flags = [VF_RHW | VF_DIFFUSE, 0, 0, 0];
    vs.rt = [2.0 / dw as f32, 2.0 / dh as f32, 1.0, 1.0];
    let mut fs = FsUniforms::default();
    fs.stages[0].op = [top::SELECTARG1, D3DTA_TEXTURE, D3DTA_CURRENT, top::SELECTARG1];
    fs.stages[0].op2 = [D3DTA_TEXTURE, D3DTA_CURRENT, 0, 0];
    if colorkey {
        fs.mode[3] = FF_COLORKEY;
    }
    let (sw, sh) = (src_size.0.max(1) as f32, src_size.1.max(1) as f32);
    let (u0, v0, u1, v1) = (srect.left as f32 / sw, srect.top as f32 / sh, srect.right as f32 / sw, srect.bottom as f32 / sh);
    let (x0, y0, x1, y1) = (drect.left as f32 - 0.5, drect.top as f32 - 0.5, drect.right as f32 - 0.5, drect.bottom as f32 - 0.5);
    let mk = |x: f32, y: f32, u: f32, v: f32| CanonVertex {
        pos: [x, y, 0.0, 1.0],
        diffuse: 0xFFFF_FFFF,
        t0: [u, v, 1.0, 0.0],
        ..Default::default()
    };
    let verts = [mk(x0, y0, u0, v0), mk(x1, y0, u1, v0), mk(x0, y1, u0, v1), mk(x1, y1, u1, v1)];
    let align = g.ubo_align;
    g.ring_reserve(4096);
    let vsl = g.ring_upload(as_bytes(&vs), align);
    let fsl = g.ring_upload(as_bytes(&fs), align);
    let vbl = g.ring_upload(slice_bytes(&verts), 16);
    let sk = SamplerKey { mag_linear: linear, min_linear: linear, address_u: 3, address_v: 3, ..Default::default() };
    let white = g.white;
    if !g.begin_pass(Some(dst), None) {
        return;
    }
    let raster = RasterState {
        blend: None,
        color_write: true,
        cull: vk::CullModeFlags::NONE,
        topology: vk::PrimitiveTopology::TRIANGLE_STRIP,
        polygon: vk::PolygonMode::FILL,
        depth_test: false,
        depth_write: false,
        depth_cmp: vk::CompareOp::ALWAYS,
        depth_bias: 0.0,
        stencil: None,
        viewport: vk::Viewport { x: 0.0, y: 0.0, width: dw as f32, height: dh as f32, min_depth: 0.0, max_depth: 1.0 },
        scissor: vk_rect(0, 0, dw, dh),
    };
    g.draw(&Draw {
        raster,
        vs_ubo: vsl,
        fs_ubo: fsl,
        textures: [(src, sk), (white, SamplerKey::default()), (white, SamplerKey::default()), (white, SamplerKey::default())],
        vertices: vbl,
        vertex_count: 4,
        indices: None,
        index_count: 0,
        _marker: std::marker::PhantomData,
    });
}

// ------------------------------------------------------------ IUnknown

unsafe extern "system" fn d_query_interface(this: This, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return E_POINTER;
    }
    if *iid == IID_IDIRECT3DDEVICE7 || *iid == IID_IUNKNOWN {
        (*this).refs += 1;
        *out = this as *mut c_void;
        return S_OK;
    }
    info!("IDirect3DDevice7::QueryInterface: unsupported {}", *iid);
    *out = null_mut();
    E_NOINTERFACE
}
unsafe extern "system" fn d_add_ref(this: This) -> u32 {
    (*this).refs += 1;
    (*this).refs
}
unsafe extern "system" fn d_release(this: This) -> u32 {
    (*this).refs -= 1;
    let r = (*this).refs;
    if r == 0 {
        info!("IDirect3DDevice7 released");
        let d = &mut *this;
        for t in d.textures.iter_mut() {
            if !t.is_null() {
                Surface::release(*t);
                *t = null_mut();
            }
        }
        if !d.rt.is_null() {
            Surface::release(d.rt);
        }
        let d3d = d.d3d;
        if CURRENT == this {
            CURRENT = null_mut();
        }
        drop(Box::from_raw(this));
        Direct3D::release(d3d);
    }
    r
}

// ------------------------------------------------------------ caps & formats

unsafe extern "system" fn d_get_caps(this: This, desc: *mut D3DDEVICEDESC7) -> HRESULT {
    if desc.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *desc = device_desc((*this).guid);
    D3D_OK
}

pub fn texture_formats() -> Vec<DDPIXELFORMAT> {
    let mut v = vec![
        pixel::pf_argb8888(),
        pixel::pf_xrgb8888(),
        pixel::pf_rgb565(),
        pixel::pf_xrgb1555(),
        pixel::pf_argb1555(),
        pixel::pf_argb4444(),
        pixel::pf_pal8(),
    ];
    let bc = world().gpu_if_ready().map(|g| g.bc_supported).unwrap_or(true);
    if bc {
        for f in [FOURCC_DXT1, FOURCC_DXT2, FOURCC_DXT3, FOURCC_DXT4, FOURCC_DXT5] {
            v.push(pixel::pf_fourcc(f));
        }
    }
    v
}

unsafe extern "system" fn d_enum_texture_formats(_this: This, cb: Option<LPD3DENUMPIXELFORMATSCALLBACK>, ctx: *mut c_void) -> HRESULT {
    first_call!("IDirect3DDevice7::EnumTextureFormats");
    let Some(cb) = cb else { return DDERR_INVALIDPARAMS };
    for mut pf in texture_formats() {
        if cb(&mut pf, ctx) == D3DENUMRET_CANCEL {
            break;
        }
    }
    D3D_OK
}

unsafe extern "system" fn d_begin_scene(this: This) -> HRESULT {
    if (*this).in_scene {
        return D3DERR_SCENE_IN_SCENE;
    }
    (*this).in_scene = true;
    D3D_OK
}
unsafe extern "system" fn d_end_scene(this: This) -> HRESULT {
    if !(*this).in_scene {
        return D3DERR_SCENE_NOT_IN_SCENE;
    }
    (*this).in_scene = false;
    trace!("EndScene: {} draws", (*this).draws_this_frame);
    (*this).draws_this_frame = 0;
    D3D_OK
}
unsafe extern "system" fn d_get_direct3d(this: This, out: *mut *mut Direct3D) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    Direct3D::add_ref((*this).d3d);
    *out = (*this).d3d;
    D3D_OK
}

unsafe extern "system" fn d_set_render_target(this: This, rt: *mut Surface, _flags: u32) -> HRESULT {
    if rt.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    first_call!("IDirect3DDevice7::SetRenderTarget");
    debug!("SetRenderTarget {:?} {}x{}", (*rt).kind, (*rt).width(), (*rt).height());
    let d = &mut *this;
    if d.rt == rt {
        return D3D_OK;
    }
    (*rt).render_target = true;
    Surface::add_ref(rt);
    if !d.rt.is_null() {
        Surface::release(d.rt);
    }
    d.rt = rt;
    d.viewport = D3DVIEWPORT7 { dwX: 0, dwY: 0, dwWidth: (*rt).width(), dwHeight: (*rt).height(), dvMinZ: 0.0, dvMaxZ: 1.0 };
    D3D_OK
}
unsafe extern "system" fn d_get_render_target(this: This, out: *mut *mut Surface) -> HRESULT {
    if out.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let rt = (*this).rt;
    if !rt.is_null() {
        Surface::add_ref(rt);
    }
    *out = rt;
    D3D_OK
}

unsafe extern "system" fn d_clear(this: This, count: u32, rects: *const D3DRECT, flags: u32, color: u32, z: f32, stencil: u32) -> HRESULT {
    first_call!("IDirect3DDevice7::Clear", "flags={flags:#x}");
    let d = &mut *this;
    let Some((rid, zid, rw, rh, _)) = d.targets() else { return D3D_OK };
    let vp = d.viewport;
    let vpr = RECT::new(vp.dwX as i32, vp.dwY as i32, (vp.dwX + vp.dwWidth) as i32, (vp.dwY + vp.dwHeight) as i32)
        .intersect(&RECT::new(0, 0, rw as i32, rh as i32));
    let mut list: Vec<vk::Rect2D> = Vec::new();
    if count == 0 || rects.is_null() {
        list.push(vk_rect(vpr.left, vpr.top, vpr.width().max(0) as u32, vpr.height().max(0) as u32));
    } else {
        for i in 0..count as usize {
            let r = *rects.add(i);
            let c = RECT::new(r.x1, r.y1, r.x2, r.y2).intersect(&vpr);
            if !c.is_empty() {
                list.push(vk_rect(c.left, c.top, c.width() as u32, c.height() as u32));
            }
        }
    }
    let Some(g) = world().gpu() else { return D3D_OK };
    if !g.begin_pass(Some(rid), zid) {
        return D3D_OK;
    }
    let col = if flags & D3DCLEAR_TARGET != 0 { Some(pixel::argb_to_rgba(color)) } else { None };
    let depth = if flags & D3DCLEAR_ZBUFFER != 0 { Some(z) } else { None };
    let st = if flags & D3DCLEAR_STENCIL != 0 { Some(stencil & 0xFF) } else { None };
    g.clear_rects(&list, col, depth, st);
    if col.is_some() {
        Surface::mark_gpu_written(d.rt);
    }
    D3D_OK
}

// ------------------------------------------------------------ transforms

unsafe fn transform_slot(d: &mut Device, state: u32) -> Option<&mut D3DMATRIX> {
    match state {
        D3DTRANSFORMSTATE_WORLD => Some(&mut d.world[0]),
        D3DTRANSFORMSTATE_VIEW => Some(&mut d.view),
        D3DTRANSFORMSTATE_PROJECTION => Some(&mut d.proj),
        D3DTRANSFORMSTATE_WORLD1..=D3DTRANSFORMSTATE_WORLD3 => Some(&mut d.world[(state - D3DTRANSFORMSTATE_WORLD1 + 1) as usize]),
        D3DTRANSFORMSTATE_TEXTURE0..=D3DTRANSFORMSTATE_TEXTURE7 => Some(&mut d.texm[(state - D3DTRANSFORMSTATE_TEXTURE0) as usize]),
        _ => None,
    }
}

unsafe extern "system" fn d_set_transform(this: This, state: u32, m: *const D3DMATRIX) -> HRESULT {
    if m.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let mm = *m;
    if d.record(|b| b.transforms.push((state, mm))) {
        return D3D_OK;
    }
    if state == D3DTRANSFORMSTATE_PROJECTION {
        d.set_projection(mm);
        return D3D_OK;
    }
    match transform_slot(d, state) {
        Some(slot) => {
            *slot = mm;
            D3D_OK
        }
        None => DDERR_INVALIDPARAMS,
    }
}
unsafe extern "system" fn d_get_transform(this: This, state: u32, m: *mut D3DMATRIX) -> HRESULT {
    if m.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    match transform_slot(&mut *this, state) {
        Some(slot) => {
            *m = *slot;
            D3D_OK
        }
        None => DDERR_INVALIDPARAMS,
    }
}
unsafe extern "system" fn d_multiply_transform(this: This, state: u32, m: *const D3DMATRIX) -> HRESULT {
    if m.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    if state == D3DTRANSFORMSTATE_PROJECTION {
        let d = &mut *this;
        let p = (*m).mul(&d.proj);
        d.set_projection(p);
        return D3D_OK;
    }
    match transform_slot(&mut *this, state) {
        Some(slot) => {
            *slot = (*m).mul(slot);
            D3D_OK
        }
        None => DDERR_INVALIDPARAMS,
    }
}

unsafe extern "system" fn d_set_viewport(this: This, vp: *const D3DVIEWPORT7) -> HRESULT {
    if vp.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let v = *vp;
    let d = &mut *this;
    if d.record(|b| b.viewport = Some(v)) {
        return D3D_OK;
    }
    d.viewport = v;
    D3D_OK
}
unsafe extern "system" fn d_get_viewport(this: This, vp: *mut D3DVIEWPORT7) -> HRESULT {
    if vp.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *vp = (*this).viewport;
    D3D_OK
}
unsafe extern "system" fn d_set_material(this: This, m: *const D3DMATERIAL7) -> HRESULT {
    if m.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let v = *m;
    let d = &mut *this;
    if d.record(|b| b.material = Some(v)) {
        return D3D_OK;
    }
    d.material = v;
    D3D_OK
}
unsafe extern "system" fn d_get_material(this: This, m: *mut D3DMATERIAL7) -> HRESULT {
    if m.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *m = (*this).material;
    D3D_OK
}
unsafe extern "system" fn d_set_light(this: This, idx: u32, l: *const D3DLIGHT7) -> HRESULT {
    if l.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let v = *l;
    let d = &mut *this;
    if d.record(|b| b.lights.push((idx, v))) {
        return D3D_OK;
    }
    let e = d.lights.entry(idx).or_insert((v, false));
    e.0 = v;
    D3D_OK
}
unsafe extern "system" fn d_get_light(this: This, idx: u32, l: *mut D3DLIGHT7) -> HRESULT {
    if l.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    match (*this).lights.get(&idx) {
        Some((v, _)) => {
            *l = *v;
            D3D_OK
        }
        None => DDERR_INVALIDPARAMS,
    }
}
unsafe extern "system" fn d_light_enable(this: This, idx: u32, en: BOOL) -> HRESULT {
    let d = &mut *this;
    if d.record(|b| b.light_enable.push((idx, en != 0))) {
        return D3D_OK;
    }
    let e = d.lights.entry(idx).or_insert((
        D3DLIGHT7 {
            dltType: D3DLIGHT_DIRECTIONAL,
            dcvDiffuse: D3DCOLORVALUE { r: 1.0, g: 1.0, b: 1.0, a: 0.0 },
            dvDirection: D3DVECTOR { x: 0.0, y: 0.0, z: 1.0 },
            ..Default::default()
        },
        false,
    ));
    e.1 = en != 0;
    D3D_OK
}
unsafe extern "system" fn d_get_light_enable(this: This, idx: u32, en: *mut BOOL) -> HRESULT {
    if en.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    match (*this).lights.get(&idx) {
        Some((_, e)) => {
            *en = *e as BOOL;
            D3D_OK
        }
        None => DDERR_INVALIDPARAMS,
    }
}

// ------------------------------------------------------------ states

unsafe extern "system" fn d_set_render_state(this: This, state: u32, value: u32) -> HRESULT {
    let d = &mut *this;
    if state as usize >= rs::MAX {
        return DDERR_INVALIDPARAMS;
    }
    trace!("SetRenderState({state}, {value:#x})");
    if d.record(|b| b.rs.push((state, value))) {
        return D3D_OK;
    }
    d.rs[state as usize] = value;
    D3D_OK
}
unsafe extern "system" fn d_get_render_state(this: This, state: u32, value: *mut u32) -> HRESULT {
    if value.is_null() || state as usize >= rs::MAX {
        return DDERR_INVALIDPARAMS;
    }
    *value = (*this).rs[state as usize];
    D3D_OK
}
unsafe extern "system" fn d_set_texture_stage_state(this: This, stage: u32, ty: u32, value: u32) -> HRESULT {
    if stage >= 8 || ty as usize >= tss::MAX {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    if d.record(|b| b.tss.push((stage, ty, value))) {
        return D3D_OK;
    }
    let st = &mut d.tss[stage as usize];
    st[ty as usize] = value;
    if ty == tss::ADDRESS {
        st[tss::ADDRESSU as usize] = value;
        st[tss::ADDRESSV as usize] = value;
    }
    if ty == tss::COLOROP && (value == top::BUMPENVMAP || value == top::BUMPENVMAPLUMINANCE) {
        unimpl!("bump environment mapping");
    }
    D3D_OK
}
unsafe extern "system" fn d_get_texture_stage_state(this: This, stage: u32, ty: u32, value: *mut u32) -> HRESULT {
    if value.is_null() || stage >= 8 || ty as usize >= tss::MAX {
        return DDERR_INVALIDPARAMS;
    }
    *value = (*this).tss[stage as usize][ty as usize];
    D3D_OK
}
unsafe extern "system" fn d_set_texture(this: This, stage: u32, tex: *mut Surface) -> HRESULT {
    if stage >= 8 {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    if d.record(|b| b.textures.push((stage, tex))) {
        return D3D_OK;
    }
    let old = d.textures[stage as usize];
    if old == tex {
        return D3D_OK;
    }
    if !tex.is_null() {
        Surface::add_ref(tex);
        if (*tex).kind != Kind::Texture && !(*tex).render_target {
            first_call!("SetTexture with non-texture surface");
        }
    }
    d.textures[stage as usize] = tex;
    if !old.is_null() {
        Surface::release(old);
    }
    D3D_OK
}
unsafe extern "system" fn d_get_texture(this: This, stage: u32, out: *mut *mut Surface) -> HRESULT {
    if out.is_null() || stage >= 8 {
        return DDERR_INVALIDPARAMS;
    }
    let t = (*this).textures[stage as usize];
    if !t.is_null() {
        Surface::add_ref(t);
    }
    *out = t;
    D3D_OK
}

// ------------------------------------------------------------ state blocks

unsafe fn apply_block(d: This, b: &StateBlock) {
    for &(s, v) in &b.rs {
        (*d).rs[s as usize] = v;
    }
    for &(st, t, v) in &b.tss {
        d_set_texture_stage_state(d, st, t, v);
    }
    for (s, m) in &b.transforms {
        if *s == D3DTRANSFORMSTATE_PROJECTION {
            (*d).set_projection(*m);
        } else if let Some(slot) = transform_slot(&mut *d, *s) {
            *slot = *m;
        }
    }
    if let Some(v) = b.viewport {
        (*d).viewport = v;
    }
    if let Some(m) = b.material {
        (*d).material = m;
    }
    for (i, l) in &b.lights {
        let e = (*d).lights.entry(*i).or_insert((*l, false));
        e.0 = *l;
    }
    for (i, en) in &b.light_enable {
        d_light_enable(d, *i, *en as BOOL);
    }
    for (s, t) in &b.textures {
        d_set_texture(d, *s, *t);
    }
}

unsafe fn snapshot(d: &Device) -> StateBlock {
    StateBlock {
        rs: (0..rs::MAX as u32).map(|i| (i, d.rs[i as usize])).collect(),
        tss: (0..8u32).flat_map(|s| (0..tss::MAX as u32).map(move |t| (s, t))).map(|(s, t)| (s, t, d.tss[s as usize][t as usize])).collect(),
        transforms: {
            let mut v = vec![(D3DTRANSFORMSTATE_WORLD, d.world[0]), (D3DTRANSFORMSTATE_VIEW, d.view), (D3DTRANSFORMSTATE_PROJECTION, d.proj)];
            for i in 0..8 {
                v.push((D3DTRANSFORMSTATE_TEXTURE0 + i, d.texm[i as usize]));
            }
            v
        },
        viewport: Some(d.viewport),
        material: Some(d.material),
        lights: d.lights.iter().map(|(k, (l, _))| (*k, *l)).collect(),
        light_enable: d.lights.iter().map(|(k, (_, e))| (*k, *e)).collect(),
        textures: Vec::new(),
    }
}

unsafe extern "system" fn d_begin_state_block(this: This) -> HRESULT {
    first_call!("IDirect3DDevice7::BeginStateBlock");
    (*this).recording = Some(StateBlock::default());
    D3D_OK
}
unsafe extern "system" fn d_end_state_block(this: This, handle: *mut u32) -> HRESULT {
    let d = &mut *this;
    let Some(b) = d.recording.take() else { return D3DERR_INVALIDCALL };
    let h = d.next_block;
    d.next_block += 1;
    d.blocks.insert(h, b);
    if !handle.is_null() {
        *handle = h;
    }
    D3D_OK
}
unsafe extern "system" fn d_apply_state_block(this: This, h: u32) -> HRESULT {
    let Some(b) = (*this).blocks.get(&h).cloned() else { return D3DERR_INVALIDCALL };
    apply_block(this, &b);
    D3D_OK
}
unsafe extern "system" fn d_capture_state_block(this: This, h: u32) -> HRESULT {
    let d = &mut *this;
    let snap = snapshot(d);
    let Some(b) = d.blocks.get_mut(&h) else { return D3DERR_INVALIDCALL };
    // Update only the states the block contains.
    for e in b.rs.iter_mut() {
        e.1 = d.rs[e.0 as usize];
    }
    for e in b.tss.iter_mut() {
        e.2 = d.tss[e.0 as usize][e.1 as usize];
    }
    if b.viewport.is_some() {
        b.viewport = snap.viewport;
    }
    if b.material.is_some() {
        b.material = snap.material;
    }
    D3D_OK
}
unsafe extern "system" fn d_delete_state_block(this: This, h: u32) -> HRESULT {
    (*this).blocks.remove(&h);
    D3D_OK
}
unsafe extern "system" fn d_create_state_block(this: This, _ty: u32, handle: *mut u32) -> HRESULT {
    let d = &mut *this;
    let b = snapshot(d);
    let h = d.next_block;
    d.next_block += 1;
    d.blocks.insert(h, b);
    if !handle.is_null() {
        *handle = h;
    }
    D3D_OK
}

// ------------------------------------------------------------ draws

fn prim_ok(prim: u32) -> bool {
    (D3DPT_POINTLIST..=D3DPT_TRIANGLEFAN).contains(&prim)
}

unsafe extern "system" fn d_draw_primitive(this: This, prim: u32, fvf: u32, verts: *const c_void, count: u32, _flags: u32) -> HRESULT {
    first_call!("IDirect3DDevice7::DrawPrimitive", "prim={prim} fvf={fvf:#x}");
    if verts.is_null() || !prim_ok(prim) {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let l = parse_fvf(fvf);
    d.scratch.clear();
    let p = verts as *const u8;
    for i in 0..count as usize {
        d.scratch.push(convert_vertex(p.add(i * l.stride), &l));
    }
    d.draw(prim, &l, count, None)
}

unsafe extern "system" fn d_draw_indexed_primitive(
    this: This,
    prim: u32,
    fvf: u32,
    verts: *const c_void,
    vcount: u32,
    indices: *const u16,
    icount: u32,
    _flags: u32,
) -> HRESULT {
    first_call!("IDirect3DDevice7::DrawIndexedPrimitive", "prim={prim} fvf={fvf:#x}");
    if verts.is_null() || indices.is_null() || !prim_ok(prim) {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let l = parse_fvf(fvf);
    d.scratch.clear();
    let p = verts as *const u8;
    for i in 0..vcount as usize {
        d.scratch.push(convert_vertex(p.add(i * l.stride), &l));
    }
    let ix = std::slice::from_raw_parts(indices, icount as usize);
    d.draw(prim, &l, vcount, Some(ix))
}

unsafe extern "system" fn d_draw_primitive_strided(
    this: This,
    prim: u32,
    fvf: u32,
    data: *const D3DDRAWPRIMITIVESTRIDEDDATA,
    count: u32,
    _flags: u32,
) -> HRESULT {
    first_call!("IDirect3DDevice7::DrawPrimitiveStrided", "prim={prim} fvf={fvf:#x}");
    if data.is_null() || !prim_ok(prim) {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let l = parse_fvf(fvf);
    d.scratch.clear();
    for i in 0..count as usize {
        d.scratch.push(convert_strided(&*data, i, fvf));
    }
    d.draw(prim, &l, count, None)
}

unsafe extern "system" fn d_draw_indexed_primitive_strided(
    this: This,
    prim: u32,
    fvf: u32,
    data: *const D3DDRAWPRIMITIVESTRIDEDDATA,
    vcount: u32,
    indices: *const u16,
    icount: u32,
    _flags: u32,
) -> HRESULT {
    first_call!("IDirect3DDevice7::DrawIndexedPrimitiveStrided", "prim={prim} fvf={fvf:#x}");
    if data.is_null() || indices.is_null() || !prim_ok(prim) {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let l = parse_fvf(fvf);
    d.scratch.clear();
    for i in 0..vcount as usize {
        d.scratch.push(convert_strided(&*data, i, fvf));
    }
    let ix = std::slice::from_raw_parts(indices, icount as usize);
    d.draw(prim, &l, vcount, Some(ix))
}

unsafe fn fill_from_vb(d: &mut Device, vb: *mut VertexBuffer, start: u32, num: u32) -> Option<FvfLayout> {
    let v = &*vb;
    let l = v.layout;
    let total = v.desc.dwNumVertices;
    if start >= total {
        return None;
    }
    let num = num.min(total - start);
    d.scratch.clear();
    let base = v.data.as_ptr();
    for i in start..start + num {
        d.scratch.push(convert_vertex(base.add(i as usize * l.stride), &l));
    }
    Some(l)
}

unsafe extern "system" fn d_draw_primitive_vb(this: This, prim: u32, vb: *mut VertexBuffer, start: u32, num: u32, _flags: u32) -> HRESULT {
    first_call!("IDirect3DDevice7::DrawPrimitiveVB", "prim={prim}");
    if vb.is_null() || !prim_ok(prim) {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let Some(l) = fill_from_vb(d, vb, start, num) else { return D3D_OK };
    let n = d.scratch.len() as u32;
    d.draw(prim, &l, n, None)
}

unsafe extern "system" fn d_draw_indexed_primitive_vb(
    this: This,
    prim: u32,
    vb: *mut VertexBuffer,
    start: u32,
    num: u32,
    indices: *const u16,
    icount: u32,
    _flags: u32,
) -> HRESULT {
    first_call!("IDirect3DDevice7::DrawIndexedPrimitiveVB", "prim={prim}");
    if vb.is_null() || indices.is_null() || !prim_ok(prim) {
        return DDERR_INVALIDPARAMS;
    }
    let d = &mut *this;
    let Some(l) = fill_from_vb(d, vb, start, num) else { return D3D_OK };
    let n = d.scratch.len() as u32;
    let ix = std::slice::from_raw_parts(indices, icount as usize);
    d.draw(prim, &l, n, Some(ix))
}

unsafe extern "system" fn d_set_clip_status(this: This, s: *const D3DCLIPSTATUS) -> HRESULT {
    if !s.is_null() {
        (*this).clip_status = *s;
    }
    D3D_OK
}
unsafe extern "system" fn d_get_clip_status(this: This, s: *mut D3DCLIPSTATUS) -> HRESULT {
    if s.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *s = (*this).clip_status;
    D3D_OK
}

/// Tests bounding spheres against the view frustum (D3DSTATUS_CLIP* bits).
unsafe extern "system" fn d_compute_sphere_visibility(
    this: This,
    centers: *const D3DVECTOR,
    radii: *const f32,
    num: u32,
    _flags: u32,
    results: *mut u32,
) -> HRESULT {
    first_call!("IDirect3DDevice7::ComputeSphereVisibility");
    if centers.is_null() || radii.is_null() || results.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let d = &*this;
    let m = d.world[0].mul(&d.view).mul(&d.proj);
    // Frustum planes from the combined matrix (row-vector convention).
    let col = |c: usize| [m.m[0][c], m.m[1][c], m.m[2][c], m.m[3][c]];
    let (c0, c1, c2, c3) = (col(0), col(1), col(2), col(3));
    let add = |a: [f32; 4], b: [f32; 4], s: f32| [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2], a[3] + s * b[3]];
    let planes = [add(c3, c0, 1.0), add(c3, c0, -1.0), add(c3, c1, -1.0), add(c3, c1, 1.0), c2, add(c3, c2, -1.0)];
    // D3DSTATUS_CLIPINTERSECTION* bits: left, right, top, bottom, front, back
    let inter = [0x1000u32, 0x2000, 0x4000, 0x8000, 0x10000, 0x20000];
    let union_ = [0x1u32, 0x2, 0x4, 0x8, 0x10, 0x20];
    for i in 0..num as usize {
        let c = *centers.add(i);
        let r = *radii.add(i);
        let mut res = 0;
        for (p, pl) in planes.iter().enumerate() {
            let len = (pl[0] * pl[0] + pl[1] * pl[1] + pl[2] * pl[2]).sqrt().max(1e-12);
            let dist = (pl[0] * c.x + pl[1] * c.y + pl[2] * c.z + pl[3]) / len;
            if dist < -r {
                res |= inter[p] | union_[p];
            } else if dist < r {
                res |= union_[p];
            }
        }
        *results.add(i) = res;
    }
    D3D_OK
}

unsafe extern "system" fn d_pre_load(_this: This, s: *mut Surface) -> HRESULT {
    if !s.is_null() {
        Surface::sync_to_gpu(s);
    }
    D3D_OK
}

unsafe extern "system" fn d_validate_device(_this: This, passes: *mut u32) -> HRESULT {
    if !passes.is_null() {
        *passes = 1;
    }
    D3D_OK
}

unsafe extern "system" fn d_load(_this: This, dst: *mut Surface, pt: *mut c_void, src: *mut Surface, rect: *mut RECT, _flags: u32) -> HRESULT {
    first_call!("IDirect3DDevice7::Load");
    if dst.is_null() || src.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let (px, py) = if pt.is_null() {
        (0, 0)
    } else {
        let p = pt as *const i32;
        (*p, *p.add(1))
    };
    let dl = Surface::levels(dst);
    let sl = Surface::levels(src);
    let mut sr = if rect.is_null() { RECT::new(0, 0, (*src).width() as i32, (*src).height() as i32) } else { *rect };
    let (mut x, mut y) = (px, py);
    for (d, s) in dl.iter().zip(sl.iter()) {
        let dr = RECT::new(x, y, x + sr.width(), y + sr.height());
        crate::surface::do_blt(*d, Some(dr), *s, Some(sr), 0, None);
        sr = RECT::new(sr.left / 2, sr.top / 2, (sr.right / 2).max(sr.left / 2 + 1), (sr.bottom / 2).max(sr.top / 2 + 1));
        x /= 2;
        y /= 2;
    }
    D3D_OK
}

unsafe extern "system" fn d_set_clip_plane(this: This, idx: u32, p: *const f32) -> HRESULT {
    if p.is_null() || idx >= 32 {
        return DDERR_INVALIDPARAMS;
    }
    first_call!("IDirect3DDevice7::SetClipPlane");
    std::ptr::copy_nonoverlapping(p, (*this).clip_planes[idx as usize].as_mut_ptr(), 4);
    D3D_OK
}
unsafe extern "system" fn d_get_clip_plane(this: This, idx: u32, p: *mut f32) -> HRESULT {
    if p.is_null() || idx >= 32 {
        return DDERR_INVALIDPARAMS;
    }
    std::ptr::copy_nonoverlapping((*this).clip_planes[idx as usize].as_ptr(), p, 4);
    D3D_OK
}
unsafe extern "system" fn d_get_info(_this: This, _id: u32, _p: *mut c_void, _s: u32) -> HRESULT {
    S_FALSE
}
