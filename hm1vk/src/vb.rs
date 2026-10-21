//! IDirect3DVertexBuffer7. Vertex data lives in system memory and is
//! converted to the canonical vertex format at draw time.

use std::ffi::c_void;
use std::ptr::null_mut;

use crate::device::Device;
use crate::ff::{parse_fvf, FvfLayout};
use crate::types::*;
use crate::{first_call, unimpl};

type This = *mut VertexBuffer;

#[repr(C)]
pub struct VertexBuffer {
    vtbl: *const VbVtbl,
    refs: u32,
    pub desc: D3DVERTEXBUFFERDESC,
    pub layout: FvfLayout,
    pub data: Vec<u8>,
    locked: bool,
}

#[repr(C)]
struct VbVtbl {
    query_interface: unsafe extern "system" fn(This, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(This) -> u32,
    release: unsafe extern "system" fn(This) -> u32,
    lock: unsafe extern "system" fn(This, u32, *mut *mut c_void, *mut u32) -> HRESULT,
    unlock: unsafe extern "system" fn(This) -> HRESULT,
    process_vertices: unsafe extern "system" fn(This, u32, u32, u32, This, u32, *mut Device, u32) -> HRESULT,
    get_vertex_buffer_desc: unsafe extern "system" fn(This, *mut D3DVERTEXBUFFERDESC) -> HRESULT,
    optimize: unsafe extern "system" fn(This, *mut Device, u32) -> HRESULT,
    process_vertices_strided:
        unsafe extern "system" fn(This, u32, u32, u32, *mut D3DDRAWPRIMITIVESTRIDEDDATA, u32, *mut Device, u32) -> HRESULT,
}

static VB_VTBL: VbVtbl = VbVtbl {
    query_interface: v_query_interface,
    add_ref: v_add_ref,
    release: v_release,
    lock: v_lock,
    unlock: v_unlock,
    process_vertices: v_process_vertices,
    get_vertex_buffer_desc: v_get_vertex_buffer_desc,
    optimize: v_optimize,
    process_vertices_strided: v_process_vertices_strided,
};

impl VertexBuffer {
    pub fn create(desc: &D3DVERTEXBUFFERDESC) -> This {
        let layout = parse_fvf(desc.dwFVF);
        if layout.stride == 0 {
            return null_mut();
        }
        let size = layout.stride * desc.dwNumVertices as usize;
        first_call!("IDirect3D7::CreateVertexBuffer", "fvf={:#x} n={} caps={:#x}", desc.dwFVF, desc.dwNumVertices, desc.dwCaps);
        crate::debug!("CreateVertexBuffer fvf={:#x} n={} caps={:#x}", desc.dwFVF, desc.dwNumVertices, desc.dwCaps);
        let mut d = *desc;
        d.dwSize = std::mem::size_of::<D3DVERTEXBUFFERDESC>() as u32;
        Box::into_raw(Box::new(VertexBuffer { vtbl: &VB_VTBL, refs: 1, desc: d, layout, data: vec![0u8; size], locked: false }))
    }
    pub unsafe fn add_ref(v: This) {
        (*v).refs += 1;
    }
    pub unsafe fn release(v: This) {
        v_release(v);
    }
}

unsafe extern "system" fn v_query_interface(this: This, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if !iid.is_null() && !out.is_null() && (*iid == IID_IDIRECT3DVERTEXBUFFER7 || *iid == IID_IUNKNOWN) {
        (*this).refs += 1;
        *out = this as *mut c_void;
        return S_OK;
    }
    E_NOINTERFACE
}
unsafe extern "system" fn v_add_ref(this: This) -> u32 {
    (*this).refs += 1;
    (*this).refs
}
unsafe extern "system" fn v_release(this: This) -> u32 {
    (*this).refs -= 1;
    let r = (*this).refs;
    if r == 0 {
        drop(Box::from_raw(this));
    }
    r
}
unsafe extern "system" fn v_lock(this: This, _flags: u32, ptr: *mut *mut c_void, size: *mut u32) -> HRESULT {
    if ptr.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let v = &mut *this;
    v.locked = true;
    *ptr = v.data.as_mut_ptr() as *mut c_void;
    if !size.is_null() {
        *size = v.data.len() as u32;
    }
    DD_OK
}
unsafe extern "system" fn v_unlock(this: This) -> HRESULT {
    (*this).locked = false;
    DD_OK
}

/// Software vertex processing (D3DVOP_TRANSFORM [| LIGHT | CLIP | EXTENTS]):
/// transforms source vertices into a D3DFVF_XYZRHW destination buffer.
unsafe extern "system" fn v_process_vertices(
    this: This,
    _op: u32,
    dst_index: u32,
    count: u32,
    src: This,
    src_index: u32,
    device: *mut Device,
    flags: u32,
) -> HRESULT {
    first_call!("IDirect3DVertexBuffer7::ProcessVertices", "flags={flags:#x}");
    if src.is_null() || device.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    let dst = &mut *this;
    let s = &*src;
    if !dst.layout.rhw {
        return DDERR_INVALIDPARAMS;
    }
    let dev = &*device;
    let m = dev.world_view_proj_viewport();
    for i in 0..count as usize {
        let si = (src_index as usize + i) * s.layout.stride;
        let di = (dst_index as usize + i) * dst.layout.stride;
        if si + s.layout.stride > s.data.len() || di + dst.layout.stride > dst.data.len() {
            break;
        }
        let sp = s.data.as_ptr().add(si);
        let dp = dst.data.as_mut_ptr().add(di);
        let x = (sp as *const f32).read_unaligned();
        let y = (sp.add(4) as *const f32).read_unaligned();
        let z = (sp.add(8) as *const f32).read_unaligned();
        let v = [x, y, z, 1.0];
        let mut o = [0f32; 4];
        for (c, oc) in o.iter_mut().enumerate() {
            *oc = (0..4).map(|k| v[k] * m.m[k][c]).sum();
        }
        let rhw = if o[3] != 0.0 { 1.0 / o[3] } else { 1.0 };
        let out = [o[0] * rhw, o[1] * rhw, o[2] * rhw, rhw];
        for (k, val) in out.iter().enumerate() {
            (dp.add(k * 4) as *mut f32).write_unaligned(*val);
        }
        if flags & D3DPV_DONOTCOPYDATA == 0 {
            if let (Some(so), Some(dof)) = (s.layout.diffuse, dst.layout.diffuse) {
                std::ptr::copy_nonoverlapping(sp.add(so), dp.add(dof), 4);
            }
            if let (Some(so), Some(dof)) = (s.layout.specular, dst.layout.specular) {
                std::ptr::copy_nonoverlapping(sp.add(so), dp.add(dof), 4);
            }
            for t in 0..s.layout.tex_count.min(dst.layout.tex_count) {
                let (so, sc) = s.layout.tex[t];
                let (dof, dc) = dst.layout.tex[t];
                std::ptr::copy_nonoverlapping(sp.add(so), dp.add(dof), sc.min(dc) * 4);
            }
        }
    }
    DD_OK
}

unsafe extern "system" fn v_get_vertex_buffer_desc(this: This, d: *mut D3DVERTEXBUFFERDESC) -> HRESULT {
    if d.is_null() {
        return DDERR_INVALIDPARAMS;
    }
    *d = (*this).desc;
    DD_OK
}
unsafe extern "system" fn v_optimize(this: This, _d: *mut Device, _f: u32) -> HRESULT {
    (*this).desc.dwCaps |= D3DVBCAPS_OPTIMIZED;
    DD_OK
}
unsafe extern "system" fn v_process_vertices_strided(
    _t: This,
    _op: u32,
    _di: u32,
    _c: u32,
    _s: *mut D3DDRAWPRIMITIVESTRIDEDDATA,
    _fvf: u32,
    _d: *mut Device,
    _f: u32,
) -> HRESULT {
    unimpl!("IDirect3DVertexBuffer7::ProcessVerticesStrided");
    DDERR_UNSUPPORTED
}
