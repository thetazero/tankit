// Bloom: the brightest of the frame, blurred at every size from a few pixels
// to a third of the screen, for the post pass to add back. Down a chain of
// ever smaller textures, then up again, each added onto the next larger.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_bloom(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSample(src, samp, uv).rgb;
}

// What of a color is bright enough to bloom, coming on softly.
fn bright(c: vec3<f32>) -> vec3<f32> {
    let threshold = 0.6;
    let knee = 0.35;
    let b = max(c.r, max(c.g, c.b));
    let soft = clamp(b - threshold + knee, 0.0, 2.0 * knee);
    let k = max(soft * soft / (4.0 * knee), b - threshold) / max(b, 1e-4);
    // (Nothing so bright that one pixel of it flickers across the screen.)
    return min(c * k, vec3<f32>(12.0));
}

@fragment
fn fs_bright(v: Out) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    var sum = vec3<f32>(0.0);
    var all = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let c = bright(tap(v.uv + vec2<f32>(f32(x), f32(y)) * px * 1.5));
            // A lone bright pixel counts for less: it doesn't sparkle.
            let w = 1.0 / (1.0 + max(c.r, max(c.g, c.b)));
            sum += c * w;
            all += w;
        }
    }
    return vec4<f32>(sum / all, 1.0);
}

@fragment
fn fs_down(v: Out) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    var c = tap(v.uv) * 0.25;
    for (var k = 0; k < 4; k++) {
        let s = vec2<f32>(f32(k & 1) * 2.0 - 1.0, f32(k >> 1) * 2.0 - 1.0);
        c += tap(v.uv + s * px) * 0.125;
        c += tap(v.uv + s * px * 2.0) * 0.03125;
    }
    for (var k = 0; k < 4; k++) {
        let a = f32(k) * 1.5707963;
        c += tap(v.uv + vec2<f32>(cos(a), sin(a)) * px * 2.0) * 0.03125;
    }
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_up(v: Out) -> @location(0) vec4<f32> {
    let px = 1.0 / vec2<f32>(textureDimensions(src));
    var c = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let w = f32((2 - abs(x)) * (2 - abs(y))) / 16.0;
            c += tap(v.uv + vec2<f32>(f32(x), f32(y)) * px) * w;
        }
    }
    return vec4<f32>(c, 1.0);
}
