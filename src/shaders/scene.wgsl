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
    // Object-space position and normal, so surface detail sticks to moving parts.
    @location(4) lpos: vec3<f32>,
    @location(5) lnormal: vec3<f32>,
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
    out.lpos = v.pos;
    out.lnormal = v.normal;
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
    metal: f32,
};

fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Project object-space position onto the plane facing the dominant normal axis.
fn plane_uv(p: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    let a = abs(n);
    if (a.x > a.y && a.x > a.z) {
        return p.zy;
    } else if (a.y > a.z) {
        return p.xz;
    }
    return p.xy;
}

// Antialiased thin line mask for a periodic coordinate (1 on the line).
// `w` is the screen-space derivative of x (derivatives must be taken in uniform control flow).
fn grid_line(x: f32, width: f32, w: f32) -> f32 {
    let d = abs(fract(x + 0.5) - 0.5);
    return 1.0 - smoothstep(width, width + w * 1.5, d);
}

// Tank materials (ids 6..12) work in object space, so detail moves with the part.
// Direction (docs/design-direction.md): weathered solid gunmetal, dust and
// wear; nothing on the machine glows.
fn tank_surface(m: i32, base: vec3<f32>, lpos: vec3<f32>, ln: vec3<f32>, wpos: vec3<f32>, wn: vec3<f32>, duv: vec2<f32>) -> Surface {
    var s: Surface;
    s.albedo = base;
    s.rough = 0.6;
    s.ao = 1.0;
    s.emissive = vec3<f32>(0.0);
    s.metal = 0.0;
    let uv = plane_uv(lpos, ln);
    let mottle = noise2(uv * 2.3) * 0.5 + noise2(uv * 7.0) * 0.3 + noise2(uv * 23.0) * 0.2;
    // Grime collects low on the vehicle.
    let grime = mix(0.5, 1.0, smoothstep(0.15, 1.3, wpos.y));
    if (m == 6) {
        // Bare armor steel: big clean surfaces, only a soft mottle.
        s.albedo = base * (0.88 + 0.24 * mottle) * grime;
        s.metal = 0.35;
        s.rough = 0.7 + 0.1 * mottle;
    } else if (m == 7 || m == 11) {
        // Sun-bleached paint over steel; chamfered edges (11) rubbed slightly darker.
        s.albedo = base * (0.9 + 0.2 * mottle) * grime;
        s.rough = 0.82;
        if (m == 11) {
            // Bevels: paint rubbed thin on the edges, catching the light.
            s.albedo *= 1.35;
            s.metal = 0.3;
            s.rough = 0.5;
        }
    } else if (m == 8) {
        // Track steel, polished where it meets the ground.
        s.albedo = base * (0.8 + 0.4 * noise2(uv * 30.0)) * grime;
        s.metal = 0.55;
        s.rough = 0.6;
    } else if (m == 10) {
        // Machined metal (rods, pins, axles): the only slightly bright parts.
        let brush = noise2(vec2<f32>(uv.x * 3.0, uv.y * 80.0));
        s.albedo = base * (0.9 + 0.2 * brush);
        s.metal = 0.8;
        s.rough = 0.4 + 0.1 * brush;
    } else if (m == 12) {
        // Vision blocks: dark glass.
        s.metal = 0.3;
        s.rough = 0.12;
    }
    // Value hierarchy: lit top planes, a dark dense underside. Downward faces
    // and everything low on the vehicle lose most of their ambient light.
    let under = smoothstep(0.05, -0.6, wn.y);
    let low_ao = mix(0.3, 1.0, smoothstep(0.2, 1.3, wpos.y));
    s.albedo *= 1.0 - under * 0.45;
    s.ao = low_ao * (1.0 - under * 0.5);
    // An even film of fine dust over everything, a little heavier on top and low down.
    if (m != 12) {
        let up = smoothstep(0.3, 0.95, wn.y);
        let low = 1.0 - smoothstep(0.2, 1.0, wpos.y);
        let d = clamp(0.1 + up * 0.14 + low * 0.14 + (noise2(uv * 1.5 + 7.0) - 0.5) * 0.08, 0.0, 0.45);
        s.albedo = mix(s.albedo, vec3<f32>(0.3, 0.27, 0.22), d);
        s.rough = mix(s.rough, 0.95, d);
        s.metal *= 1.0 - d;
    }
    return s;
}

// Procedural surface detail keyed by material id.
fn surface(id: f32, base: vec3<f32>, wpos: vec3<f32>, n: vec3<f32>) -> Surface {
    var s: Surface;
    s.albedo = base;
    s.rough = 0.6;
    s.ao = 1.0;
    s.emissive = vec3<f32>(0.0);
    s.metal = 0.0;
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
    let m = i32(round(v.color.a));
    let is_tank = m >= 6 && m <= 12 && m != 9;
    // On tanks the instance tint is the team color; it only affects paint, and
    // muted, so bright color stays reserved for combat effects.
    let paint = m == 7 || m == 11;
    // Near-monochrome: the team color is only a faint cast on the paint.
    let team = mix(vec3<f32>(dot(v.tint.rgb, vec3<f32>(0.3, 0.55, 0.15))), v.tint.rgb, 0.12);
    var base = select(v.color.rgb * v.tint.rgb, v.color.rgb, is_tank && !paint);
    if (is_tank && paint) {
        base = v.color.rgb * team;
    }
    let duv = fwidth(plane_uv(v.lpos, normalize(v.lnormal)));
    var s: Surface;
    if (is_tank) {
        s = tank_surface(m, base, v.lpos, normalize(v.lnormal), v.wpos, n, duv);
    } else {
        s = surface(v.color.a, base, v.wpos, n);
    }

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
    // Metals: tinted specular, darker diffuse, and a sky/ground reflection.
    let spec_col = mix(vec3<f32>(1.0), s.albedo * 1.6, s.metal);
    let f = mix(fresnel, 0.6 + 0.4 * fresnel, s.metal);
    let diffuse = s.albedo * (1.0 - s.metal * 0.75);
    let r = reflect(-view, n);
    let env = mix(g.ground_color.rgb * 0.7, mix(g.sky_horizon.rgb, g.sky_zenith.rgb, clamp(r.y, 0.0, 1.0)), smoothstep(-0.15, 0.15, r.y));
    var c = diffuse * hemi * 0.5 * s.ao;
    c += (diffuse + spec * f * spec_col * (1.0 - s.rough * 0.7)) * g.sun_color.rgb * ndl * sh;
    c += env * f * spec_col * (0.15 + 0.35 * s.metal) * (1.0 - s.rough) * s.ao;
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
