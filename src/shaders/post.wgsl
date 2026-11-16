// HDR -> display: exposure, ACES filmic tonemap, subtle vignette.

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_samp: sampler;
@group(0) @binding(2) var ao_tex: texture_2d<f32>;

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
    // SSAO, lightly blurred (the raw pass uses a per-pixel rotated kernel).
    let px = 1.0 / vec2<f32>(textureDimensions(ao_tex));
    // A 5x5 box in 3x3 taps: a linear tap halfway between two texels is
    // their average, so along each axis texels -2,-1 come from one tap at
    // -1.5, texels 0,1 from one at 0.5, and texel 2 alone (weights 2, 2, 1).
    let at = array<vec2<f32>, 3>(vec2<f32>(-1.5, 2.0), vec2<f32>(0.5, 2.0), vec2<f32>(2.0, 1.0));
    var ao = 0.0;
    for (var y = 0; y < 3; y++) {
        for (var x = 0; x < 3; x++) {
            let o = vec2<f32>(at[x].x, at[y].x);
            ao += textureSample(ao_tex, hdr_samp, v.uv + o * px).r * at[x].y * at[y].y;
        }
    }
    c *= ao / 25.0;
    // Exposure, filmic tonemap, then a little contrast (an S-curve) and
    // saturation so the image doesn't sit washed out in the mid-greys.
    c = aces(c * 0.68);
    c = mix(c, c * c * (3.0 - 2.0 * c), 0.3);
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    c = max(mix(vec3<f32>(luma), c, 1.3), vec3<f32>(0.0));
    let q = v.uv - 0.5;
    c *= 1.0 - dot(q, q) * 0.45;
    return vec4<f32>(c, 1.0);
}
