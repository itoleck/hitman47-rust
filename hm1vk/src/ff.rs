//! CPU-side mirrors of the fixed-function shader interface (`shaders/ff.wgsl`):
//! uniform blocks and the canonical vertex every D3D vertex format is
//! converted to.

use crate::types::*;

pub type Vec4 = [f32; 4];
pub type Mat4 = [[f32; 4]; 4];

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct LightU {
    pub diffuse: Vec4,
    pub specular: Vec4,
    pub ambient: Vec4,
    pub position: Vec4,
    pub direction: Vec4,
    pub atten: Vec4,
    pub spot: Vec4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VsUniforms {
    pub world_view: Mat4,
    pub proj: Mat4,
    pub normal_mat: Mat4,
    pub tex_mat: [Mat4; 4],
    pub mat_diffuse: Vec4,
    pub mat_ambient: Vec4,
    pub mat_specular: Vec4,
    pub mat_emissive: Vec4,
    pub global_ambient: Vec4,
    pub fog: Vec4,
    pub rt: Vec4,
    pub flags: [u32; 4],
    pub tci: [u32; 4],
    pub ttff: [u32; 4],
    pub lights: [LightU; 8],
}
const _: () = assert!(std::mem::size_of::<VsUniforms>() == 1504);

impl Default for VsUniforms {
    fn default() -> Self {
        let id = D3DMATRIX::identity().m;
        VsUniforms {
            world_view: id,
            proj: id,
            normal_mat: id,
            tex_mat: [id; 4],
            mat_diffuse: [1.0; 4],
            mat_ambient: [0.0; 4],
            mat_specular: [0.0; 4],
            mat_emissive: [0.0; 4],
            global_ambient: [0.0; 4],
            fog: [0.0, 1.0, 1.0, 0.0],
            rt: [1.0; 4],
            flags: [0; 4],
            tci: [0, 1, 2, 3],
            ttff: [0; 4],
            lights: [LightU::default(); 8],
        }
    }
}

pub const VF_RHW: u32 = 1;
pub const VF_NORMAL: u32 = 2;
pub const VF_DIFFUSE: u32 = 4;
pub const VF_SPECULAR: u32 = 8;
pub const VF_LIGHTING: u32 = 16;
pub const VF_NORMALIZE: u32 = 32;
pub const VF_LOCALVIEWER: u32 = 64;
pub const VF_COLORVERTEX: u32 = 128;
pub const VF_SPECULARENABLE: u32 = 256;
pub const VF_RANGEFOG: u32 = 512;

/// Special "vertex fog mode" value telling the VS to output eye depth for
/// per-pixel (table) fog.
pub const VS_FOG_TABLE: f32 = 4.0;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct StageU {
    pub op: [u32; 4],
    pub op2: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FsUniforms {
    pub stages: [StageU; 8],
    pub tfactor: Vec4,
    pub fog_color: Vec4,
    pub fog: Vec4,
    pub mode: [u32; 4],
    pub alpha_ref: Vec4,
}
const _: () = assert!(std::mem::size_of::<FsUniforms>() == 336);

impl Default for FsUniforms {
    fn default() -> Self {
        let mut s = [StageU::default(); 8];
        for st in s.iter_mut() {
            st.op = [top::DISABLE, D3DTA_TEXTURE, D3DTA_CURRENT, top::DISABLE];
            st.op2 = [D3DTA_TEXTURE, D3DTA_CURRENT, 0xFFFF_FFFF, 0];
        }
        FsUniforms {
            stages: s,
            tfactor: [1.0; 4],
            fog_color: [0.0; 4],
            fog: [0.0, 1.0, 1.0, 0.0],
            mode: [8, 0, 0, 0],
            alpha_ref: [0.0; 4],
        }
    }
}

pub const FF_SPECULAR: u32 = 1;
pub const FF_ALPHATEST: u32 = 2;
pub const FF_COLORKEY: u32 = 4;

/// The canonical vertex (84 bytes, matches the pipeline's vertex input).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CanonVertex {
    pub pos: [f32; 4],
    pub normal: [f32; 3],
    pub diffuse: u32,
    pub specular: u32,
    pub t0: [f32; 4],
    pub t1: [f32; 4],
    pub t2: [f32; 2],
    pub t3: [f32; 2],
}
const _: () = assert!(std::mem::size_of::<CanonVertex>() == crate::vk::pipeline::VERTEX_STRIDE as usize);

pub fn as_bytes<T: Copy>(v: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }
}

pub fn slice_bytes<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

/// Parsed FVF layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct FvfLayout {
    pub stride: usize,
    pub rhw: bool,
    pub weights: usize,
    pub normal: Option<usize>,
    pub diffuse: Option<usize>,
    pub specular: Option<usize>,
    pub tex_count: usize,
    pub tex: [(usize, usize); 8],
}

pub fn parse_fvf(fvf: u32) -> FvfLayout {
    let mut l = FvfLayout::default();
    let mut off = 0usize;
    match fvf & D3DFVF_POSITION_MASK {
        D3DFVF_XYZ => off += 12,
        D3DFVF_XYZRHW => {
            off += 16;
            l.rhw = true;
        }
        p @ D3DFVF_XYZB1..=D3DFVF_XYZB5 => {
            let n = ((p - D3DFVF_XYZB1) / 2 + 1) as usize;
            l.weights = n;
            off += 12 + 4 * n;
        }
        _ => {}
    }
    if fvf & D3DFVF_NORMAL != 0 {
        l.normal = Some(off);
        off += 12;
    }
    if fvf & D3DFVF_RESERVED1 != 0 {
        off += 4;
    }
    if fvf & D3DFVF_DIFFUSE != 0 {
        l.diffuse = Some(off);
        off += 4;
    }
    if fvf & D3DFVF_SPECULAR != 0 {
        l.specular = Some(off);
        off += 4;
    }
    let tc = ((fvf & D3DFVF_TEXCOUNT_MASK) >> D3DFVF_TEXCOUNT_SHIFT).min(8) as usize;
    l.tex_count = tc;
    for i in 0..tc {
        let comps = match (fvf >> (16 + 2 * i)) & 3 {
            0 => 2,
            1 => 3,
            2 => 4,
            _ => 1,
        };
        l.tex[i] = (off, comps);
        off += comps * 4;
    }
    l.stride = off;
    l
}

#[inline]
unsafe fn rf(p: *const u8) -> f32 {
    (p as *const f32).read_unaligned()
}
#[inline]
unsafe fn ru(p: *const u8) -> u32 {
    (p as *const u32).read_unaligned()
}

/// Pads a 1-4 component texture coordinate the way D3D feeds texture
/// transforms: the first missing component is 1, the rest 0.
#[inline]
unsafe fn read_tc(p: *const u8, comps: usize) -> [f32; 4] {
    let mut t = [0f32; 4];
    for (i, v) in t.iter_mut().enumerate().take(comps) {
        *v = rf(p.add(i * 4));
    }
    if comps < 4 {
        t[comps] = 1.0;
    }
    t
}

/// Converts one vertex at `p` with layout `l`.
#[inline]
pub unsafe fn convert_vertex(p: *const u8, l: &FvfLayout) -> CanonVertex {
    let mut v = CanonVertex { diffuse: 0xFFFF_FFFF, ..Default::default() };
    v.pos = [rf(p), rf(p.add(4)), rf(p.add(8)), if l.rhw { rf(p.add(12)) } else { 1.0 }];
    if let Some(o) = l.normal {
        v.normal = [rf(p.add(o)), rf(p.add(o + 4)), rf(p.add(o + 8))];
    }
    if let Some(o) = l.diffuse {
        v.diffuse = ru(p.add(o));
    }
    if let Some(o) = l.specular {
        v.specular = ru(p.add(o));
    }
    let tcs = [&mut v.t0, &mut v.t1];
    for (i, dst) in tcs.into_iter().enumerate() {
        if i < l.tex_count {
            let (o, c) = l.tex[i];
            *dst = read_tc(p.add(o), c);
        } else {
            *dst = [0.0, 0.0, 1.0, 0.0];
        }
    }
    if l.tex_count > 2 {
        let (o, c) = l.tex[2];
        let t = read_tc(p.add(o), c);
        v.t2 = [t[0], t[1]];
    }
    if l.tex_count > 3 {
        let (o, c) = l.tex[3];
        let t = read_tc(p.add(o), c);
        v.t3 = [t[0], t[1]];
    }
    v
}

/// Converts a strided vertex (DrawPrimitiveStrided).
pub unsafe fn convert_strided(d: &D3DDRAWPRIMITIVESTRIDEDDATA, i: usize, fvf: u32) -> CanonVertex {
    let mut v = CanonVertex { diffuse: 0xFFFF_FFFF, ..Default::default() };
    let at = |s: &D3DDP_PTRSTRIDE| s.lpvData.add(i * s.dwStride as usize);
    let p = at(&d.position);
    let rhw = fvf & D3DFVF_POSITION_MASK == D3DFVF_XYZRHW;
    v.pos = [rf(p), rf(p.add(4)), rf(p.add(8)), if rhw { rf(p.add(12)) } else { 1.0 }];
    if fvf & D3DFVF_NORMAL != 0 && !d.normal.lpvData.is_null() {
        let n = at(&d.normal);
        v.normal = [rf(n), rf(n.add(4)), rf(n.add(8))];
    }
    if fvf & D3DFVF_DIFFUSE != 0 && !d.diffuse.lpvData.is_null() {
        v.diffuse = ru(at(&d.diffuse));
    }
    if fvf & D3DFVF_SPECULAR != 0 && !d.specular.lpvData.is_null() {
        v.specular = ru(at(&d.specular));
    }
    let tc = ((fvf & D3DFVF_TEXCOUNT_MASK) >> D3DFVF_TEXCOUNT_SHIFT).min(8) as usize;
    for t in 0..tc.min(4) {
        if d.textureCoords[t].lpvData.is_null() {
            continue;
        }
        let comps = match (fvf >> (16 + 2 * t)) & 3 {
            0 => 2,
            1 => 3,
            2 => 4,
            _ => 1,
        };
        let c = read_tc(at(&d.textureCoords[t]), comps);
        match t {
            0 => v.t0 = c,
            1 => v.t1 = c,
            2 => v.t2 = [c[0], c[1]],
            _ => v.t3 = [c[0], c[1]],
        }
    }
    if tc < 1 {
        v.t0 = [0.0, 0.0, 1.0, 0.0];
    }
    if tc < 2 {
        v.t1 = [0.0, 0.0, 1.0, 0.0];
    }
    v
}

pub fn color_value(c: &D3DCOLORVALUE) -> Vec4 {
    [c.r, c.g, c.b, c.a]
}

pub fn argb(c: u32) -> Vec4 {
    crate::pixel::argb_to_rgba(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tlvertex_layout() {
        // D3DFVF_TLVERTEX = XYZRHW | DIFFUSE | SPECULAR | TEX1
        let l = parse_fvf(D3DFVF_XYZRHW | D3DFVF_DIFFUSE | D3DFVF_SPECULAR | 0x100);
        assert_eq!(l.stride, 32);
        assert!(l.rhw);
        assert_eq!(l.diffuse, Some(16));
        assert_eq!(l.specular, Some(20));
        assert_eq!(l.tex[0], (24, 2));
    }

    #[test]
    fn lvertex_layout() {
        // D3DFVF_LVERTEX = XYZ | RESERVED1 | DIFFUSE | SPECULAR | TEX1
        let l = parse_fvf(D3DFVF_XYZ | D3DFVF_RESERVED1 | D3DFVF_DIFFUSE | D3DFVF_SPECULAR | 0x100);
        assert_eq!(l.stride, 32);
        assert_eq!(l.diffuse, Some(16));
    }
}
