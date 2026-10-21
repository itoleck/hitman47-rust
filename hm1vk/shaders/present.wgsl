// Presentation pass: copies the game's back buffer into the swapchain image,
// scaled to the destination viewport, and applies the DirectDraw gamma ramp
// (IDirectDrawGammaControl) through a 256-entry lookup texture.

struct PresentUniforms {
    src_uv: vec4f,   // u0, v0, u1, v1 of the source rectangle
    flags: vec4u,    // x: gamma enabled
};

@group(0) @binding(0) var<uniform> pu: PresentUniforms;
@group(0) @binding(2) var src: texture_2d<f32>;
@group(0) @binding(3) var lut: texture_2d<f32>;
@group(0) @binding(6) var smp: sampler;
@group(0) @binding(7) var lut_smp: sampler;

struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    // Two-triangle quad covering the viewport.
    var corners = array<vec2f, 6>(
        vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(0.0, 1.0),
        vec2f(0.0, 1.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0)
    );
    let c = corners[vi];
    var o: VsOut;
    o.pos = vec4f(c.x * 2.0 - 1.0, c.y * 2.0 - 1.0, 0.0, 1.0);
    o.uv = mix(pu.src_uv.xy, pu.src_uv.zw, c);
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4f {
    var c = textureSample(src, smp, i.uv);
    let gr = textureSample(lut, lut_smp, vec2f(c.r * (255.0 / 256.0) + 0.5 / 256.0, 0.5)).r;
    let gg = textureSample(lut, lut_smp, vec2f(c.g * (255.0 / 256.0) + 0.5 / 256.0, 0.5)).g;
    let gb = textureSample(lut, lut_smp, vec2f(c.b * (255.0 / 256.0) + 0.5 / 256.0, 0.5)).b;
    if (pu.flags.x != 0u) {
        c = vec4f(gr, gg, gb, 1.0);
    }
    return vec4f(c.rgb, 1.0);
}
