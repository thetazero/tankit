// Lit scene geometry, shadow depth pass, and sky. Output is linear HDR.

struct Globals {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    sun_dir: vec4<f32>,      // xyz: direction toward the sun
    sun_color: vec4<f32>,
    sky_zenith: vec4<f32>,
    sky_horizon: vec4<f32>,
    ground_color: vec4<f32>,
    params: vec4<f32>,       // x: time, y: fog density, z: screen width, w: screen height
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var shadow_tex: texture_depth_2d;
@group(1) @binding(1) var shadow_samp: sampler_comparison;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
    @location(7) tint: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) wpos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) tint: vec4<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let wpos = model * vec4<f32>(v.pos, 1.0);
    var out: VOut;
    out.clip = g.view_proj * wpos;
    out.wpos = wpos.xyz;
    out.normal = (model * vec4<f32>(v.normal, 0.0)).xyz;
    out.color = v.color;
    out.tint = v.tint;
    return out;
}

@vertex
fn vs_shadow(v: VIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    return g.light_view_proj * model * vec4<f32>(v.pos, 1.0);
}

fn hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn shadow_factor(wpos: vec3<f32>, n: vec3<f32>) -> f32 {
    // Normal offset reduces acne on surfaces at grazing angles.
    let p = g.light_view_proj * vec4<f32>(wpos + n * 0.08, 1.0);
    let ndc = p.xyz / p.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) {
        return 1.0;
    }
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_tex));
    var sum = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            sum += textureSampleCompareLevel(shadow_tex, shadow_samp, uv + vec2<f32>(f32(x), f32(y)) * texel * 1.25, ndc.z - 0.0008);
        }
    }
    return sum / 9.0;
}

struct Surface {
    albedo: vec3<f32>,
    rough: f32,
    ao: f32,
    emissive: vec3<f32>,
};

// Procedural surface detail keyed by material id.
fn surface(id: f32, base: vec3<f32>, wpos: vec3<f32>, n: vec3<f32>) -> Surface {
    var s: Surface;
    s.albedo = base;
    s.rough = 0.6;
    s.ao = 1.0;
    s.emissive = vec3<f32>(0.0);
    let m = i32(round(id));
    if (m == 1) {
        // Building wall: window grid with floor bands, a few windows lit.
        let tangent = normalize(vec3<f32>(-n.z, 0.0, n.x));
        let u = dot(wpos, tangent);
        let cell = vec2<f32>(u / 3.2, wpos.y / 3.6);
        let f = fract(cell);
        let id2 = floor(cell);
        let is_window = f.x > 0.18 && f.x < 0.82 && f.y > 0.3 && f.y < 0.85 && wpos.y > 1.6;
        if (is_window) {
            let h = hash2(id2 + floor(wpos.xz * 0.01));
            s.albedo = mix(vec3<f32>(0.10, 0.14, 0.18), vec3<f32>(0.22, 0.28, 0.34), h);
            s.rough = 0.15;
            if (h > 0.93) {
                s.emissive = vec3<f32>(1.0, 0.78, 0.45) * 0.6;
            }
        } else if (f.y < 0.12) {
            s.albedo = base * 0.82;
        }
        // Grounding: darken the base of walls.
        s.ao = mix(0.55, 1.0, smoothstep(0.0, 2.5, wpos.y));
    } else if (m == 2) {
        // Asphalt with coarse patches.
        let h = hash2(floor(wpos.xz * 0.5));
        let h2 = hash2(floor(wpos.xz * 4.0));
        s.albedo = base * (0.85 + 0.25 * h) * (0.95 + 0.1 * h2);
        s.rough = 0.9;
    } else if (m == 3) {
        let h = hash2(floor(wpos.xz * 0.7));
        s.albedo = base * 0.75 * (0.9 + 0.15 * h);
        s.rough = 0.85;
    } else if (m == 4) {
        let h = hash2(floor(wpos.xz * 2.0));
        s.albedo = base * (0.85 + 0.3 * h);
        s.rough = 0.95;
    } else if (m == 5) {
        // Paving tiles.
        let f = fract(wpos.xz * 0.5);
        let grout = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y));
        let h = hash2(floor(wpos.xz * 0.5));
        s.albedo = base * (0.92 + 0.12 * h) * select(0.75, 1.0, grout > 0.04);
        s.rough = 0.8;
    }
    return s;
}

fn sky_color(dir: vec3<f32>) -> vec3<f32> {
    let t = clamp(dir.y, -1.0, 1.0);
    var c = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, pow(max(t, 0.0), 0.55));
    if (t < 0.0) {
        c = mix(g.sky_horizon.rgb, g.ground_color.rgb * 0.6, min(-t * 4.0, 1.0));
    }
    let sd = max(dot(dir, g.sun_dir.xyz), 0.0);
    c += g.sun_color.rgb * (pow(sd, 900.0) * 40.0 + pow(sd, 12.0) * 0.25);
    return c;
}

fn apply_fog(c: vec3<f32>, wpos: vec3<f32>) -> vec3<f32> {
    let d = distance(wpos, g.cam_pos.xyz);
    let f = 1.0 - exp(-d * g.params.y);
    let dir = normalize(wpos - g.cam_pos.xyz);
    return mix(c, sky_color(vec3<f32>(dir.x, max(dir.y, 0.0) * 0.3, dir.z)), f);
}

@fragment
fn fs_main(v: VOut) -> @location(0) vec4<f32> {
    var n = normalize(v.normal);
    let base = v.color.rgb * v.tint.rgb;
    let s = surface(v.color.a, base, v.wpos, n);

    let l = normalize(g.sun_dir.xyz);
    let view = normalize(g.cam_pos.xyz - v.wpos);
    let h = normalize(l + view);
    let ndl = max(dot(n, l), 0.0);
    let sh = select(1.0, shadow_factor(v.wpos, n), ndl > 0.0);

    let gloss = 2.0 / max(s.rough * s.rough, 0.02);
    let spec = pow(max(dot(n, h), 0.0), gloss) * (gloss + 8.0) / 25.0;
    let fresnel = 0.04 + 0.96 * pow(1.0 - max(dot(n, view), 0.0), 5.0);

    let sky_amb = mix(g.sky_horizon.rgb, g.sky_zenith.rgb, 0.35);
    let hemi = mix(g.ground_color.rgb, sky_amb, n.y * 0.5 + 0.5);
    var c = s.albedo * hemi * 0.5 * s.ao;
    c += (s.albedo + spec * fresnel * (1.0 - s.rough * 0.7)) * g.sun_color.rgb * ndl * sh;
    c += hemi * fresnel * 0.15 * (1.0 - s.rough);
    c += s.emissive;

    // Emissive instances (effects, pickups) replace lighting with their own glow.
    let e = v.tint.a;
    c = mix(c, base * e, clamp(e, 0.0, 1.0));

    return vec4<f32>(apply_fog(c, v.wpos), 1.0);
}

// Sky: fullscreen triangle at the far plane, drawn after geometry.
struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: SkyOut;
    o.clip = vec4<f32>(p, 1.0, 1.0);
    o.ndc = p;
    return o;
}

@fragment
fn fs_sky(v: SkyOut) -> @location(0) vec4<f32> {
    let far = g.inv_view_proj * vec4<f32>(v.ndc, 1.0, 1.0);
    let near = g.inv_view_proj * vec4<f32>(v.ndc, 0.0, 1.0);
    let dir = normalize(far.xyz / far.w - near.xyz / near.w);
    return vec4<f32>(sky_color(dir), 1.0);
}
