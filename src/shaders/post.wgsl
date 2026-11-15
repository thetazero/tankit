// HDR -> display: exposure, ACES filmic tonemap, subtle vignette.

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_samp: sampler;

struct PostOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_post(@builtin(vertex_index) i: u32) -> PostOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: PostOut;
    o.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_post(v: PostOut) -> @location(0) vec4<f32> {
    var c = textureSample(hdr_tex, hdr_samp, v.uv).rgb;
    c = aces(c * 0.8);
    // Gentle saturation boost keeps the palette punchy after tonemapping.
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    c = mix(vec3<f32>(luma), c, 1.15);
    let q = v.uv - 0.5;
    c *= 1.0 - dot(q, q) * 0.45;
    return vec4<f32>(c, 1.0);
}
