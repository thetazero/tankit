// Soft, camera-facing particles drawn into the HDR target after the opaque
// scene: depth-tested, not depth-written. Blended premultiplied, so one pass
// holds both kinds: lit, alpha-blended dust and blown sand (glow 0), and
// self-lit plasma glow (glow > 0) that adds light and blocks none. Either
// kind can billow (billow > 0, seeding its shape): smoke and flame, lumpy at
// the edge and through the body instead of a smooth round blob.

struct Globals {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    sky_zenith: vec4<f32>,
    sky_horizon: vec4<f32>,
    ground_color: vec4<f32>,
    params: vec4<f32>, // x: time, y: fog density
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var shadow_tex: texture_depth_2d;
@group(1) @binding(1) var shadow_samp: sampler_comparison;
@group(2) @binding(0) var depth_tex: texture_depth_2d;

// Distance from the camera to the opaque scene behind this pixel.
fn scene_dist(frag: vec2<f32>) -> f32 {
    let s = vec2<i32>(textureDimensions(depth_tex));
    let q = clamp(vec2<i32>(frag), vec2<i32>(0), s - 1);
    let d = textureLoad(depth_tex, q, 0);
    let ndc = vec2<f32>(frag.x / f32(s.x) * 2.0 - 1.0, 1.0 - frag.y / f32(s.y) * 2.0);
    let w = g.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return distance(w.xyz / w.w, g.cam_pos.xyz);
}

// In the sun (1) or in shadow (0): a single tap is enough for soft blobs.
fn sunlit(wpos: vec3<f32>) -> f32 {
    let p = g.light_view_proj * vec4<f32>(wpos, 1.0);
    let ndc = p.xyz / p.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(shadow_tex, shadow_samp, uv, ndc.z - 0.002);
}

struct PIn {
    @location(0) pos: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) stretch: vec4<f32>,
    @location(4) billow: f32,
};

struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) wpos: vec3<f32>,
    @location(3) streak: f32,
    @location(4) size: f32,
    @location(5) glow: f32,
    @location(6) near: f32,
    @location(7) billow: f32,
};

@vertex
fn vs_particle(@builtin(vertex_index) i: u32, p: PIn) -> POut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[i];
    let to_cam = normalize(g.cam_pos.xyz - p.pos);
    var right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), to_cam));
    var up = cross(to_cam, right) * p.size;
    right = right * p.size;
    // Streaks: long axis along the motion, short axis across it, facing the camera.
    let s = p.stretch.xyz;
    let sl = length(s);
    if (sl > 0.01) {
        let dir = s / sl;
        let side = normalize(cross(dir, to_cam));
        up = dir * p.size * (1.0 + sl);
        right = side * p.size;
    }
    let w = p.pos + right * c.x + up * c.y;
    var o: POut;
    o.clip = g.view_proj * vec4<f32>(w, 1.0);
    o.uv = c;
    o.color = p.color;
    o.wpos = w;
    o.streak = select(0.0, 1.0, sl > 0.01);
    o.size = p.size;
    o.glow = p.stretch.w;
    o.billow = p.billow;
    // Fade out a puff the camera is in or about to be in: up close its quad
    // fills the view and the near plane cuts it off in a straight line.
    let dc = distance(p.pos, g.cam_pos.xyz);
    o.near = clamp((dc - p.size - 1.0) / (p.size + 1.5), 0.0, 1.0);
    return o;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash2(i), hash2(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash2(i + vec2<f32>(0.0, 1.0)), hash2(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y,
    );
}

// 0..1 lumps for a smoke puff, a different shape for each seed. Each octave
// is turned against the last (and the whole against the screen) so the
// noise lattice never lines up into visible squares.
fn billow(uv: vec2<f32>, seed: f32) -> f32 {
    let a = seed * 6.2832;
    let r = mat2x2<f32>(cos(a), sin(a), -sin(a), cos(a));
    let turn = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let o = vec2<f32>(seed * 37.0, seed * 91.0);
    let p1 = r * uv * 1.6 + o;
    let p2 = turn * p1 * 2.1 + 5.3;
    let p3 = turn * p2 * 2.1 + 9.1;
    return noise2(p1) * 0.62 + noise2(p2) * 0.28 + noise2(p3) * 0.1;
}

@fragment
fn fs_particle(v: POut) -> @location(0) vec4<f32> {
    let r2 = dot(v.uv, v.uv);
    // Round puffs: soft all over. Streaks: solid across, tapering at the ends.
    let k = max(1.0 - r2, 0.0);
    let puff = k * k;
    let line = (1.0 - v.uv.x * v.uv.x) * (1.0 - abs(v.uv.y));
    var soft0 = max(mix(puff, line, v.streak), 0.0);
    // Smoke and flame: lumps eat into the edge and thin the body, so it
    // reads as a billow rather than a ball.
    let lumpy = v.billow > 0.0;
    var lump = 0.5;
    if (lumpy) {
        lump = billow(v.uv, v.billow);
        // Thins to nothing before the quad's edge (r2 = 1), however lumpy.
        let body = clamp((1.0 - r2) * 1.6 - (1.0 - lump) * 0.75, 0.0, 1.0);
        soft0 = body * body;
    }
    // Outside the puff (the quad's corners): nothing to draw, so don't
    // pay for the depth and shadow lookups below.
    if (soft0 <= 0.0) {
        discard;
    }
    // Soft particles: fade out as the quad nears the ground or a hull it
    // cuts into, so no straight edge shows where it's clipped.
    let d = distance(v.wpos, g.cam_pos.xyz);
    let gap = scene_dist(v.clip.xy) - d;
    let fade = clamp(gap / clamp(v.size * 0.6, 0.05, 2.0), 0.0, 1.0) * v.near;
    let soft = soft0 * fade;
    if (soft <= 0.0) {
        discard;
    }
    let d_fog = 1.0 - exp(-d * g.params.y);
    if (v.glow > 0.0) {
        // Plasma: a wide colored halo around a white-hot heart, both falling
        // off smoothly (no hard edge), haze swallowing it with distance.
        var halo = exp(-r2 * 4.0) * fade;
        var heart = exp(-r2 * 22.0) * fade;
        if (lumpy) {
            // Flame: the billow's shape, hottest in its thickest lumps.
            halo = soft0 * fade;
            heart = soft0 * exp(-r2 * 8.0) * fade;
        }
        let hot = mix(v.color.rgb, vec3<f32>(1.0, 0.92, 0.97), select(0.6, 0.3, lumpy));
        let strength = v.glow * v.color.a * (1.0 - d_fog);
        let light = (v.color.rgb * halo + hot * heart * 2.0) * strength;
        // A little coverage too, so the color holds against a bright sky
        // or wall instead of adding up to white.
        let cover = clamp(halo * min(strength, 1.0) * 0.45, 0.0, 0.6);
        return vec4<f32>(light, cover);
    }
    // Lit like a sunlit cloud of grains: sun from above, sky and bounce fill.
    let sun = g.sun_color.rgb * (0.35 + 0.35 * max(g.sun_dir.y, 0.0)) * sunlit(v.wpos);
    let fill = mix(g.ground_color.rgb, g.sky_horizon.rgb, 0.6) * 0.55 * g.ground_color.w;
    var c = v.color.rgb * (sun + fill);
    if (lumpy) {
        // Lit from above, darker in its folds.
        c = c * (0.85 + 0.15 * v.uv.y + 0.6 * (lump - 0.5));
    }
    // Same haze as the scene.
    c = mix(c, g.sky_horizon.rgb, d_fog);
    let a = v.color.a * soft;
    return vec4<f32>(c * a, a);
}
