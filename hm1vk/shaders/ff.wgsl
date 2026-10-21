// Direct3D 7 fixed-function pipeline emulation.
//
// Vertex stage: pre-transformed (XYZRHW) or transformed vertices, D3D7
// lighting (8 lights), texture coordinate generation and texture transforms,
// vertex fog.
// Fragment stage: up to 8 texture blend stages (4 textures), specular add,
// table/vertex fog, alpha test.
//
// Matrices are uploaded in D3D memory order (row-major, row vectors). Read as
// WGSL column-major matrices they are the transposes, so `M * v` computes the
// D3D `v * M`.

struct Light {
    diffuse: vec4f,
    specular: vec4f,
    ambient: vec4f,
    position: vec4f,   // xyz eye-space position, w = type (1 point, 2 spot, 3 directional)
    direction: vec4f,  // xyz eye-space direction, w = range
    atten: vec4f,      // a0, a1, a2, falloff
    spot: vec4f,       // cos(theta/2), cos(phi/2), -, -
};

struct VsUniforms {
    world_view: mat4x4f,
    proj: mat4x4f,
    normal_mat: mat4x4f,
    tex_mat: array<mat4x4f, 4>,
    mat_diffuse: vec4f,
    mat_ambient: vec4f,
    mat_specular: vec4f,     // w = power
    mat_emissive: vec4f,
    global_ambient: vec4f,
    fog: vec4f,              // start, end, density, vertex fog mode
    rt: vec4f,               // 2/rt_width, 2/rt_height, 1/vp_width, 1/vp_height
    // x: flags, y: material sources (2 bits each: diffuse, specular, ambient, emissive),
    // z: light count, w: unused
    flags: vec4u,
    tci: vec4u,              // per bound stage: texcoord index | (texgen mode << 16)
    ttff: vec4u,             // per bound stage: texture transform flags
    lights: array<Light, 8>,
};

const VF_RHW: u32 = 1u;
const VF_NORMAL: u32 = 2u;
const VF_DIFFUSE: u32 = 4u;
const VF_SPECULAR: u32 = 8u;
const VF_LIGHTING: u32 = 16u;
const VF_NORMALIZE: u32 = 32u;
const VF_LOCALVIEWER: u32 = 64u;
const VF_COLORVERTEX: u32 = 128u;
const VF_SPECULARENABLE: u32 = 256u;
const VF_RANGEFOG: u32 = 512u;

struct Stage {
    op: vec4u,    // colorop, colorarg1, colorarg2, alphaop
    op2: vec4u,   // alphaarg1, alphaarg2, texture slot (or 0xFFFFFFFF), -
};

struct FsUniforms {
    stages: array<Stage, 8>,
    tfactor: vec4f,
    fog_color: vec4f,
    fog: vec4f,     // start, end, density, -
    // x: alpha func (1..8), y: fog mode (0 off, 1 table, 2 vertex), z: table mode, w: flags
    mode: vec4u,
    alpha_ref: vec4f,
};

const FF_SPECULAR: u32 = 1u;
const FF_ALPHATEST: u32 = 2u;
const FF_COLORKEY: u32 = 4u;

@group(0) @binding(0) var<uniform> vsu: VsUniforms;
@group(0) @binding(1) var<uniform> fsu: FsUniforms;
@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;
@group(0) @binding(4) var tex2: texture_2d<f32>;
@group(0) @binding(5) var tex3: texture_2d<f32>;
@group(0) @binding(6) var smp0: sampler;
@group(0) @binding(7) var smp1: sampler;
@group(0) @binding(8) var smp2: sampler;
@group(0) @binding(9) var smp3: sampler;

struct VsIn {
    @location(0) pos: vec4f,
    @location(1) normal: vec3f,
    @location(2) diffuse: vec4f,
    @location(3) specular: vec4f,
    @location(4) t0: vec4f,
    @location(5) t1: vec4f,
    @location(6) t2: vec2f,
    @location(7) t3: vec2f,
};

struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) diffuse: vec4f,
    @location(1) specular: vec4f,
    @location(2) uv0: vec4f,
    @location(3) uv1: vec4f,
    @location(4) uv2: vec4f,
    @location(5) uv3: vec4f,
    @location(6) fog: f32,
};

fn input_tc(v: VsIn, idx: u32) -> vec4f {
    switch (idx) {
        case 0u: { return v.t0; }
        case 1u: { return v.t1; }
        case 2u: { return vec4f(v.t2, 1.0, 0.0); }
        case 3u: { return vec4f(v.t3, 1.0, 0.0); }
        default: { return v.t0; }
    }
}

fn pick_comp(t: vec4f, i: u32) -> f32 {
    switch (i) {
        case 0u: { return t.x; }
        case 1u: { return t.y; }
        case 2u: { return t.z; }
        default: { return t.w; }
    }
}

// Texture coordinate for bound stage s. Output: xy coordinate, w divisor.
fn stage_tc(v: VsIn, s: u32, eye: vec3f, n: vec3f, rhw: bool) -> vec4f {
    let tci = vsu.tci[s];
    let ttff = vsu.ttff[s];
    let mode = tci >> 16u;
    var t = input_tc(v, tci & 0xFFFFu);
    if (!rhw) {
        if (mode == 1u) {
            t = vec4f(n, 1.0);
        } else if (mode == 2u) {
            t = vec4f(eye, 1.0);
        } else if (mode == 3u) {
            var e = vec3f(0.0, 0.0, -1.0);
            if ((vsu.flags.x & VF_LOCALVIEWER) != 0u) {
                e = normalize(eye);
            }
            let r = e - 2.0 * dot(e, n) * n;
            t = vec4f(r, 1.0);
        }
    }
    let count = ttff & 0xFFu;
    let projected = (ttff & 256u) != 0u;
    if (!rhw && count != 0u) {
        t = vsu.tex_mat[s] * t;
    }
    if (projected && count > 0u) {
        return vec4f(t.x, t.y, 0.0, pick_comp(t, count - 1u));
    }
    return vec4f(t.x, t.y, 0.0, 1.0);
}

fn mat_color(src: u32, mat: vec4f, v: VsIn) -> vec4f {
    if ((vsu.flags.x & VF_COLORVERTEX) != 0u) {
        if (src == 1u && (vsu.flags.x & VF_DIFFUSE) != 0u) {
            return v.diffuse;
        }
        if (src == 2u && (vsu.flags.x & VF_SPECULAR) != 0u) {
            return v.specular;
        }
    }
    return mat;
}

fn fog_factor(d: f32, mode: u32) -> f32 {
    let start = vsu.fog.x;
    let end = vsu.fog.y;
    let density = vsu.fog.z;
    if (mode == 3u) {
        if (end == start) { return 1.0; }
        return clamp((end - d) / (end - start), 0.0, 1.0);
    } else if (mode == 1u) {
        return clamp(exp(-d * density), 0.0, 1.0);
    } else if (mode == 2u) {
        let x = d * density;
        return clamp(exp(-x * x), 0.0, 1.0);
    }
    return 1.0;
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    let f = vsu.flags.x;
    let rhw = (f & VF_RHW) != 0u;
    var diffuse = vec4f(1.0, 1.0, 1.0, 1.0);
    var specular = vec4f(0.0, 0.0, 0.0, 0.0);
    if ((f & VF_DIFFUSE) != 0u) { diffuse = v.diffuse; }
    if ((f & VF_SPECULAR) != 0u) { specular = v.specular; }
    var eye = vec3f(0.0);
    var n = vec3f(0.0, 0.0, 1.0);

    if (rhw) {
        var w = 1.0;
        if (v.pos.w != 0.0) { w = 1.0 / v.pos.w; }
        // D3D pixel centres are at integer coordinates, Vulkan's at +0.5.
        let x = (v.pos.x + 0.5) * vsu.rt.x - 1.0;
        let y = (v.pos.y + 0.5) * vsu.rt.y - 1.0;
        o.pos = vec4f(x * w, y * w, v.pos.z * w, w);
        // Table fog (mode 4) uses eye depth, otherwise the vertex fog factor
        // is the specular alpha supplied with the vertex.
        if (u32(vsu.fog.w) == 4u) {
            o.fog = w;
        } else {
            o.fog = specular.a;
        }
        eye = vec3f(0.0, 0.0, w);
    } else {
        let e4 = vsu.world_view * vec4f(v.pos.xyz, 1.0);
        eye = e4.xyz;
        var clip = vsu.proj * e4;
        clip.y = -clip.y;
        clip.x = clip.x + clip.w * vsu.rt.z;
        clip.y = clip.y + clip.w * vsu.rt.w;
        o.pos = clip;
        if ((f & VF_NORMAL) != 0u) {
            n = (vsu.normal_mat * vec4f(v.normal, 0.0)).xyz;
            if ((f & VF_NORMALIZE) != 0u) {
                n = normalize(n);
            }
        }
        if ((f & VF_LIGHTING) != 0u && (f & VF_NORMAL) != 0u) {
            let srcs = vsu.flags.y;
            let md = mat_color(srcs & 3u, vsu.mat_diffuse, v);
            let ms = mat_color((srcs >> 2u) & 3u, vec4f(vsu.mat_specular.xyz, 1.0), v);
            let ma = mat_color((srcs >> 4u) & 3u, vsu.mat_ambient, v);
            let me = mat_color((srcs >> 6u) & 3u, vsu.mat_emissive, v);
            var acc_d = vec3f(0.0);
            var acc_s = vec3f(0.0);
            var acc_a = vec3f(0.0);
            let nl = normalize(n);
            var view_dir = vec3f(0.0, 0.0, -1.0);
            if ((f & VF_LOCALVIEWER) != 0u) {
                view_dir = -normalize(eye);
            }
            let count = vsu.flags.z;
            for (var i = 0u; i < count; i = i + 1u) {
                let l = vsu.lights[i];
                let ltype = u32(l.position.w);
                var ldir: vec3f;
                var att = 1.0;
                if (ltype == 3u) {
                    ldir = normalize(-l.direction.xyz);
                } else {
                    let d = l.position.xyz - eye;
                    let dist = length(d);
                    if (dist > l.direction.w) {
                        continue;
                    }
                    ldir = d / max(dist, 1e-6);
                    let denom = l.atten.x + l.atten.y * dist + l.atten.z * dist * dist;
                    att = 1.0 / max(denom, 1e-6);
                    if (ltype == 2u) {
                        let rho = dot(normalize(-l.direction.xyz), ldir);
                        let ct = l.spot.x;
                        let cp = l.spot.y;
                        if (rho <= cp) {
                            att = 0.0;
                        } else if (rho < ct) {
                            att = att * pow(clamp((rho - cp) / max(ct - cp, 1e-6), 0.0, 1.0), l.atten.w);
                        }
                    }
                }
                acc_a = acc_a + l.ambient.xyz * att;
                let ndl = max(dot(nl, ldir), 0.0);
                acc_d = acc_d + l.diffuse.xyz * (ndl * att);
                if ((f & VF_SPECULARENABLE) != 0u && ndl > 0.0) {
                    let h = normalize(view_dir + ldir);
                    let p = vsu.mat_specular.w;
                    var sp = 0.0;
                    if (p > 0.0) {
                        sp = pow(max(dot(nl, h), 0.0), p);
                    }
                    acc_s = acc_s + l.specular.xyz * (sp * att);
                }
            }
            let c = me.xyz + ma.xyz * (vsu.global_ambient.xyz + acc_a) + md.xyz * acc_d;
            diffuse = vec4f(clamp(c, vec3f(0.0), vec3f(1.0)), md.a);
            specular = vec4f(clamp(ms.xyz * acc_s, vec3f(0.0), vec3f(1.0)), specular.a);
        }
        // Fog: vsu.fog.w = vertex fog mode (0 none -> specular alpha; 4 = table fog -> pass eye depth)
        let vmode = u32(vsu.fog.w);
        if (vmode == 4u) {
            o.fog = eye.z;
        } else if (vmode == 0u) {
            o.fog = specular.a;
        } else {
            var d = abs(eye.z);
            if ((f & VF_RANGEFOG) != 0u) {
                d = length(eye);
            }
            o.fog = fog_factor(d, vmode);
        }
    }
    o.diffuse = diffuse;
    o.specular = specular;
    o.uv0 = stage_tc(v, 0u, eye, n, rhw);
    o.uv1 = stage_tc(v, 1u, eye, n, rhw);
    o.uv2 = stage_tc(v, 2u, eye, n, rhw);
    o.uv3 = stage_tc(v, 3u, eye, n, rhw);
    return o;
}

fn select_arg(sel: u32, tex: vec4f, current: vec4f, diffuse: vec4f, specular: vec4f) -> vec4f {
    var v: vec4f;
    switch (sel & 0xFu) {
        case 0u: { v = diffuse; }
        case 1u: { v = current; }
        case 2u: { v = tex; }
        case 3u: { v = fsu.tfactor; }
        case 4u: { v = specular; }
        default: { v = current; }
    }
    if ((sel & 0x10u) != 0u) {
        v = vec4f(1.0) - v;
    }
    if ((sel & 0x20u) != 0u) {
        v = vec4f(v.a);
    }
    return v;
}

fn blend_op(op: u32, a1: vec4f, a2: vec4f, tex: vec4f, current: vec4f, diffuse: vec4f) -> vec4f {
    switch (op) {
        case 2u: { return a1; }
        case 3u: { return a2; }
        case 4u: { return a1 * a2; }
        case 5u: { return a1 * a2 * 2.0; }
        case 6u: { return a1 * a2 * 4.0; }
        case 7u: { return a1 + a2; }
        case 8u: { return a1 + a2 - vec4f(0.5); }
        case 9u: { return (a1 + a2 - vec4f(0.5)) * 2.0; }
        case 10u: { return a1 - a2; }
        case 11u: { return a1 + a2 - a1 * a2; }
        case 12u: { return mix(a2, a1, diffuse.a); }
        case 13u: { return mix(a2, a1, tex.a); }
        case 14u: { return mix(a2, a1, fsu.tfactor.a); }
        case 15u: { return a1 + a2 * (1.0 - tex.a); }
        case 16u: { return mix(a2, a1, current.a); }
        case 17u: { return a1; }
        case 18u: { return vec4f(a1.rgb + a1.a * a2.rgb, a1.a); }
        case 19u: { return vec4f(a1.rgb * a2.rgb + vec3f(a1.a), a1.a); }
        case 20u: { return vec4f((1.0 - a1.a) * a2.rgb + a1.rgb, a1.a); }
        case 21u: { return vec4f((vec3f(1.0) - a1.rgb) * a2.rgb + vec3f(a1.a), a1.a); }
        case 24u: {
            let d = clamp(4.0 * dot(a1.rgb - vec3f(0.5), a2.rgb - vec3f(0.5)), 0.0, 1.0);
            return vec4f(d);
        }
        default: { return a1; }
    }
}

fn alpha_pass(func: u32, a: f32, r: f32) -> bool {
    switch (func) {
        case 1u: { return false; }
        case 2u: { return a < r; }
        case 3u: { return a == r; }
        case 4u: { return a <= r; }
        case 5u: { return a > r; }
        case 6u: { return a != r; }
        case 7u: { return a >= r; }
        default: { return true; }
    }
}

fn proj_uv(t: vec4f) -> vec2f {
    if (t.w != 0.0 && t.w != 1.0) {
        return t.xy / t.w;
    }
    return t.xy;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4f {
    let s0 = textureSample(tex0, smp0, proj_uv(i.uv0));
    let s1 = textureSample(tex1, smp1, proj_uv(i.uv1));
    let s2 = textureSample(tex2, smp2, proj_uv(i.uv2));
    let s3 = textureSample(tex3, smp3, proj_uv(i.uv3));
    let diffuse = clamp(i.diffuse, vec4f(0.0), vec4f(1.0));
    let specular = clamp(i.specular, vec4f(0.0), vec4f(1.0));

    if ((fsu.mode.w & FF_COLORKEY) != 0u && s0.a < 0.5) {
        discard;
    }

    var current = diffuse;
    for (var s = 0u; s < 8u; s = s + 1u) {
        let st = fsu.stages[s];
        let cop = st.op.x;
        if (cop == 1u) {
            break;
        }
        var tex = vec4f(1.0);
        switch (st.op2.z) {
            case 0u: { tex = s0; }
            case 1u: { tex = s1; }
            case 2u: { tex = s2; }
            case 3u: { tex = s3; }
            default: { tex = vec4f(1.0); }
        }
        let c1 = select_arg(st.op.y, tex, current, diffuse, specular);
        let c2 = select_arg(st.op.z, tex, current, diffuse, specular);
        let color = blend_op(cop, c1, c2, tex, current, diffuse);
        var alpha = current.a;
        let aop = st.op.w;
        if (aop != 1u) {
            let a1 = select_arg(st.op2.x, tex, current, diffuse, specular);
            let a2 = select_arg(st.op2.y, tex, current, diffuse, specular);
            alpha = blend_op(aop, vec4f(a1.a), vec4f(a2.a), tex, current, diffuse).a;
        }
        current = clamp(vec4f(color.rgb, alpha), vec4f(0.0), vec4f(1.0));
    }

    if ((fsu.mode.w & FF_SPECULAR) != 0u) {
        current = vec4f(clamp(current.rgb + specular.rgb, vec3f(0.0), vec3f(1.0)), current.a);
    }

    if ((fsu.mode.w & FF_ALPHATEST) != 0u) {
        let a = floor(current.a * 255.0 + 0.5);
        if (!alpha_pass(fsu.mode.x, a, fsu.alpha_ref.x)) {
            discard;
        }
    }

    let fog_mode = fsu.mode.y;
    if (fog_mode != 0u) {
        var factor = 1.0;
        if (fog_mode == 1u) {
            let d = abs(i.fog);
            let tmode = fsu.mode.z;
            if (tmode == 3u) {
                if (fsu.fog.y != fsu.fog.x) {
                    factor = (fsu.fog.y - d) / (fsu.fog.y - fsu.fog.x);
                }
            } else if (tmode == 1u) {
                factor = exp(-d * fsu.fog.z);
            } else if (tmode == 2u) {
                let x = d * fsu.fog.z;
                factor = exp(-x * x);
            }
        } else {
            factor = i.fog;
        }
        factor = clamp(factor, 0.0, 1.0);
        current = vec4f(mix(fsu.fog_color.rgb, current.rgb, factor), current.a);
    }
    return current;
}
