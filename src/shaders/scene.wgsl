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
// Procedural surface detail (render/surface.rs): layer 0 = normal xy, height,
// cavity; layer 1 = albedo variation, roughness offset, wear, grime streaks.
@group(1) @binding(2) var surf_tex: texture_2d_array<f32>;
@group(1) @binding(3) var surf_samp: sampler;

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
    // The part's axes in world space, to rotate object-space normal detail.
    @location(6) ax: vec3<f32>,
    @location(7) ay: vec3<f32>,
    @location(8) az: vec3<f32>,
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
    out.ax = v.m0.xyz;
    out.ay = v.m1.xyz;
    out.az = v.m2.xyz;
    return out;
}

@vertex
fn vs_shadow(v: VIn) -> @builtin(position) vec4<f32> {
    // Glowing effects (shots, flame, blasts) are light, not matter: no shadow.
    if (v.tint.a >= 1.0) {
        return vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
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

// Tank materials (ids 6..11) work in object space, so they move with the part.
fn tank_surface(m: i32, base: vec3<f32>, lpos: vec3<f32>, ln: vec3<f32>, wpos: vec3<f32>, duv: vec2<f32>) -> Surface {
    var s: Surface;
    s.albedo = base;
    s.rough = 0.5;
    s.ao = 1.0;
    s.emissive = vec3<f32>(0.0);
    s.metal = 0.0;
    let uv = plane_uv(lpos, ln);
    // Grime collects low on the vehicle.
    let grime = mix(0.72, 1.0, smoothstep(0.1, 1.1, wpos.y));
    if (m == MAT_CARBON) {
        // Dark composite: fine diagonal weave plus faint panel seams.
        let q = uv * 22.0;
        let weave = step(0.5, fract((q.x + q.y) * 0.5)) * 0.5 + step(0.5, fract((q.x - q.y) * 0.5)) * 0.5;
        let fade = 1.0 - clamp(length(duv * 22.0) * 0.6, 0.0, 1.0);
        s.albedo = base * (1.0 + (weave - 0.5) * 0.22 * fade) * grime;
        let seams = max(grid_line(uv.x * 0.9, 0.006, duv.x * 0.9), grid_line(uv.y * 0.9 + 0.3, 0.006, duv.y * 0.9));
        s.albedo *= 1.0 - seams * 0.4;
        s.rough = 0.38 + 0.15 * noise2(uv * 9.0);
    } else if (m == MAT_PAINT || m == MAT_WORN) {
        // Painted armor (team color via tint). Worn edges (11) show bare metal through scuffs.
        let n = noise2(uv * 7.0) * 0.6 + noise2(uv * 23.0) * 0.4;
        s.albedo = base * (0.92 + 0.1 * n) * grime;
        s.rough = 0.42;
        if (m == MAT_WORN) {
            let wear = smoothstep(0.55, 0.68, noise2(uv * 14.0 + 3.1) * 0.7 + noise2(uv * 41.0) * 0.3);
            s.albedo = mix(s.albedo, vec3<f32>(0.42, 0.42, 0.44), wear);
            s.metal = wear * 0.8;
            s.rough = mix(0.42, 0.3, wear);
        }
        // Small scratches.
        let scratch = smoothstep(0.86, 0.92, noise2(vec2<f32>(uv.x * 60.0, uv.y * 4.0)));
        s.albedo = mix(s.albedo, s.albedo * 0.8, scratch * 0.3);
    } else if (m == MAT_TREAD) {
        // Track steel: dark, worn bright on ridges facing outward.
        let n = noise2(uv * 30.0);
        s.albedo = base * (0.8 + 0.4 * n) * grime;
        s.metal = 0.6;
        s.rough = 0.55;
    } else if (m == MAT_GLOW) {
        // Light strip: slow pulse running along the hull.
        let pulse = 0.75 + 0.25 * sin(g.params.x * 3.0 - lpos.z * 1.5 + lpos.x * 0.7);
        s.albedo = base * 0.2;
        s.emissive = base * 2.2 * pulse;
    } else if (m == MAT_VHAAL) {
        // Ithessar worked crystal: near-black and glassy, with a faint
        // clouded depth and rare flecks of trapped plasma deep inside.
        let n = noise2(uv * 4.0) * 0.6 + noise2(uv * 13.0) * 0.4;
        s.albedo = base * (0.75 + 0.5 * n);
        s.rough = 0.3 + 0.12 * n;
        s.metal = 0.18;
        let fleck = smoothstep(0.95, 0.99, noise2(uv * 31.0 + vec2<f32>(lpos.y * 5.0, 0.0)));
        let flick = 0.6 + 0.4 * sin(g.params.x * 2.0 + lpos.z * 3.0);
        s.emissive = vec3<f32>(1.0, 0.36, 0.62) * fleck * 0.35 * flick;
    } else if (m == MAT_METAL) {
        // Bare gunmetal: brushed along the part's long axis.
        let brush = noise2(vec2<f32>(uv.x * 3.0, uv.y * 80.0));
        s.albedo = base * (0.85 + 0.25 * brush);
        s.metal = 0.9;
        s.rough = 0.28 + 0.1 * brush;
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
    if (m == MAT_WALL) {
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
    } else if (m == MAT_ASPHALT) {
        // Asphalt with coarse patches.
        let h = hash2(floor(wpos.xz * 0.5));
        let h2 = hash2(floor(wpos.xz * 4.0));
        s.albedo = base * (0.85 + 0.25 * h) * (0.95 + 0.1 * h2);
        s.rough = 0.9;
    } else if (m == MAT_ROOF) {
        let h = hash2(floor(wpos.xz * 0.7));
        s.albedo = base * 0.75 * (0.9 + 0.15 * h);
        s.rough = 0.85;
    } else if (m == MAT_GRASS) {
        let h = hash2(floor(wpos.xz * 2.0));
        s.albedo = base * (0.85 + 0.3 * h);
        s.rough = 0.95;
    } else if (m == MAT_PAVING) {
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
    c += g.sun_color.rgb * (pow(sd, 2400.0) * 30.0 + pow(sd, 40.0) * 0.12 + pow(sd, 6.0) * 0.06);
    return c;
}

fn apply_fog(c: vec3<f32>, wpos: vec3<f32>) -> vec3<f32> {
    let d = distance(wpos, g.cam_pos.xyz);
    // Dust banks drifting downwind: haze thickens and thins as they pass.
    let wind = vec2<f32>(g.sun_color.w, g.sky_zenith.w);
    let mid = mix(g.cam_pos.xz, wpos.xz, 0.5);
    let bank = noise2(mid * 0.012 - wind * g.params.x * 0.006);
    let thick = 1.0 + g.sky_horizon.w * (bank - 0.4) * 1.4;
    let f = 1.0 - exp(-d * g.params.y * max(thick, 0.3));
    let dir = normalize(wpos - g.cam_pos.xyz);
    return mix(c, sky_color(vec3<f32>(dir.x, max(dir.y, 0.0) * 0.3, dir.z)), f);
}

// Surface detail for a fragment, sampled triplanar in object space (no UVs
// needed; detail moves with the part). Two octaves: `scale` texels per
// meter, and 3.8x finer so narrow faces (plate edges, bevels) carry detail too.
struct Detail {
    // Perturbed world-space normal.
    normal: vec3<f32>,
    // Albedo variation, roughness offset, wear, grime (0.5 neutral for r/g).
    masks: vec4<f32>,
    fine: vec4<f32>,
    // 1 = open, lower in crevices.
    cavity: f32,
};

// Must be called in uniform control flow (it samples textures).
fn surface_detail(lpos: vec3<f32>, lnormal: vec3<f32>, ax: vec3<f32>, ay: vec3<f32>, az: vec3<f32>, scale: f32, bump: f32) -> Detail {
    let ln = normalize(lnormal);
    var tw = pow(abs(ln), vec3<f32>(4.0));
    tw = tw / (tw.x + tw.y + tw.z);
    let tp = lpos * scale;
    let tq = lpos * scale * 3.8;
    let sx = textureSample(surf_tex, surf_samp, tp.zy, 0);
    let sy = textureSample(surf_tex, surf_samp, tp.xz, 0);
    let sz = textureSample(surf_tex, surf_samp, tp.xy, 0);
    let fx = textureSample(surf_tex, surf_samp, tq.zy, 0).xy * 2.0 - 1.0;
    let fy = textureSample(surf_tex, surf_samp, tq.xz, 0).xy * 2.0 - 1.0;
    let fz = textureSample(surf_tex, surf_samp, tq.xy, 0).xy * 2.0 - 1.0;
    var d: Detail;
    d.masks = textureSample(surf_tex, surf_samp, tp.zy, 1) * tw.x + textureSample(surf_tex, surf_samp, tp.xz, 1) * tw.y
        + textureSample(surf_tex, surf_samp, tp.xy, 1) * tw.z;
    d.fine = textureSample(surf_tex, surf_samp, tq.zy, 1) * tw.x + textureSample(surf_tex, surf_samp, tq.xz, 1) * tw.y
        + textureSample(surf_tex, surf_samp, tq.xy, 1) * tw.z;
    d.cavity = sx.a * tw.x + sy.a * tw.y + sz.a * tw.z;
    let tx = sx.xy * 2.0 - 1.0;
    let ty = sy.xy * 2.0 - 1.0;
    let tz = sz.xy * 2.0 - 1.0;
    let n_obj = normalize(
        ln + bump * (tw.x * vec3<f32>(0.0, tx.y, tx.x) + tw.y * vec3<f32>(ty.x, 0.0, ty.y) + tw.z * vec3<f32>(tz.x, tz.y, 0.0))
            + bump * 0.7 * (tw.x * vec3<f32>(0.0, fx.y, fx.x) + tw.y * vec3<f32>(fy.x, 0.0, fy.y) + tw.z * vec3<f32>(fz.x, fz.y, 0.0)),
    );
    d.normal = normalize(normalize(ax) * n_obj.x + normalize(ay) * n_obj.y + normalize(az) * n_obj.z);
    return d;
}

// World materials 13..17 (map.rs `mat`): sand, plaster, concrete, rubble,
// rust. Detail comes from the procedural surface textures; these work in
// world space (static map geometry has an identity transform).
const SAND_COL: vec3<f32> = vec3<f32>(0.74, 0.6, 0.42);

fn world_surface(m: i32, base: vec3<f32>, wpos: vec3<f32>, n: vec3<f32>, d: Detail) -> Surface {
    var s: Surface;
    s.albedo = base;
    s.rough = 0.88;
    s.ao = mix(1.0, d.cavity, 0.8);
    s.emissive = vec3<f32>(0.0);
    s.metal = 0.0;
    let up = clamp(n.y, 0.0, 1.0);
    let vertical = 1.0 - abs(n.y);
    let blotch = d.masks.r;
    let grain = d.fine.r;
    if (m == MAT_SAND) {
        // Sand: wind ripples across the flats, darker crusted hardpan in
        // patches, cracked dry mud on the reservoir floor.
        // Ripples: two trains at different angles and wavelengths, bent by
        // large-scale warping, faded in and out in patches, with the sharp
        // crest and long gentle back slope real wind ripples have.
        let warp = vec2<f32>(noise2(wpos.xz * 0.045), noise2(wpos.xz * 0.045 + 17.0)) * 9.0;
        let q = wpos.xz + warp;
        let ph1 = fract((q.x * 0.9 + q.y * 0.42) * 0.42 + noise2(q * 0.3) * 0.6);
        let ph2 = fract((q.x * 0.35 - q.y * 0.94) * 0.31 + noise2(q * 0.21 + 5.0) * 0.8);
        let r1 = pow(ph1, 3.0) * 2.0 - 0.6;
        let r2 = pow(ph2, 3.0) * 2.0 - 0.6;
        let m1 = smoothstep(0.35, 0.7, noise2(wpos.xz * 0.05 + 3.0));
        let m2 = smoothstep(0.45, 0.8, noise2(wpos.xz * 0.04 + 41.0));
        let ripple = (r1 * m1 + r2 * m2 * (1.0 - m1 * 0.6)) * 0.8;
        let flat_ = smoothstep(0.85, 0.98, up);
        let hardpan = smoothstep(0.55, 0.75, noise2(wpos.xz * 0.035) * 0.7 + blotch * 0.3);
        s.albedo = base * (0.92 + 0.1 * ripple * flat_ + 0.22 * (blotch - 0.5) + 0.1 * (grain - 0.5));
        s.albedo = mix(s.albedo, base * vec3<f32>(0.82, 0.78, 0.74), hardpan * 0.5);
        // Sand on the move: sheets of loose grains sliding downwind over the
        // surface, in gusty patches (wind and amount packed in the globals' .w).
        let wind = vec2<f32>(g.sun_color.w, g.sky_zenith.w);
        let ws = max(length(wind), 0.01);
        let wdir = wind / ws;
        let along = dot(wpos.xz, wdir);
        let across = dot(wpos.xz, vec2<f32>(-wdir.y, wdir.x));
        let t = g.params.x;
        let sheet = noise2(vec2<f32>(along * 0.5 - t * ws * 0.5, across * 1.6))
            * smoothstep(0.3, 0.75, noise2(vec2<f32>(along * 0.05 - t * ws * 0.05, across * 0.12)));
        let moving = smoothstep(0.25, 0.6, sheet) * g.sky_horizon.w * flat_;
        s.albedo = mix(s.albedo, base * 1.15, moving * 0.55);
        let mud = smoothstep(-1.5, -3.0, wpos.y);
        let crack = smoothstep(0.48, 0.62, d.fine.b + d.masks.b * 0.4);
        s.albedo = mix(s.albedo, vec3<f32>(0.6, 0.53, 0.44), mud * 0.6);
        s.albedo *= 1.0 - crack * mud * 0.45;
        s.rough = 0.96;
    } else if (m == MAT_PLASTER) {
        // Plaster: sun-bleached and blotchy, chipped through to stone and brick
        // underneath, grime streaks running down from ledges, dust caked low.
        s.albedo = base * (0.84 + 0.32 * blotch) * (0.94 + 0.12 * grain);
        let chip = smoothstep(0.38, 0.6, d.masks.b * 0.7 + d.fine.b * 0.6);
        let masonry = vec3<f32>(0.58, 0.45, 0.34) * (0.75 + 0.5 * grain);
        s.albedo = mix(s.albedo, masonry, chip);
        s.ao *= 1.0 - chip * 0.25;
        // Rain-stain runs: broad and soft, not grain.
        let stain = smoothstep(0.35, 0.8, d.masks.a) * smoothstep(0.3, 0.6, noise2(vec2<f32>(wpos.x + wpos.z, wpos.y * 0.15) * 0.4));
        s.albedo *= 1.0 - 0.28 * stain * vertical;
        let caked = 1.0 - smoothstep(0.0, 1.6, wpos.y);
        s.albedo = mix(s.albedo, SAND_COL * 0.9, caked * vertical * 0.55);
        s.rough = 0.9;
    } else if (m == MAT_CONCRETE) {
        // Cast concrete: grey, blotchy, pitted, stained along the edges.
        s.albedo = base * (0.76 + 0.48 * blotch) * (0.9 + 0.2 * grain);
        s.albedo *= 1.0 - 0.18 * smoothstep(0.4, 0.8, d.masks.a) * vertical;
        s.rough = 0.92;
    } else if (m == MAT_RUBBLE) {
        // Rubble: broken masonry and stone, high variation, deep crevices.
        s.albedo = base * (0.62 + 0.7 * grain) * (0.8 + 0.4 * blotch);
        s.ao = d.cavity * d.cavity;
        s.rough = 0.95;
    } else if (m == MAT_RUST) {
        // Rusted steel: orange-brown bloom, flaking dark scale, a little bare
        // metal where it's been scraped.
        let bloom = vec3<f32>(0.4, 0.24, 0.14) * (0.7 + 0.6 * grain);
        s.albedo = mix(base, bloom, smoothstep(0.25, 0.75, blotch + 0.2 * grain));
        s.albedo *= 1.0 - 0.4 * smoothstep(0.55, 0.75, d.fine.b);
        let bare = smoothstep(0.7, 0.85, d.masks.b);
        s.albedo = mix(s.albedo, vec3<f32>(0.4, 0.39, 0.37), bare);
        s.metal = bare * 0.8;
        s.rough = mix(0.8, 0.4, bare);
    }
    if (m == MAT_PAINTED) {
        // Weathered paint over steel: faded and blotchy, rust breaking through
        // in patches (worse low down) and running down from them.
        s.albedo = base * (0.8 + 0.3 * blotch) * (0.94 + 0.12 * grain);
        let low = 1.0 - smoothstep(0.0, 1.8, wpos.y);
        let rust = smoothstep(0.62, 0.8, blotch * 0.55 + d.fine.b * 0.45 + low * 0.25);
        s.albedo = mix(s.albedo, vec3<f32>(0.36, 0.22, 0.13) * (0.7 + 0.6 * grain), rust);
        s.albedo *= 1.0 - 0.3 * smoothstep(0.5, 0.85, d.masks.a) * vertical;
        s.rough = mix(0.65, 0.85, rust);
    }
    if (m == MAT_CLOTH) {
        // Sun-faded fabric: bleached where it faces the sky, blotchy, with
        // stains run down from the folds. Soft, no sheen.
        s.albedo = base * (0.82 + 0.3 * blotch) * (0.95 + 0.1 * grain);
        s.albedo = mix(s.albedo, s.albedo * 1.25 + vec3<f32>(0.06), up * 0.45);
        s.albedo *= 1.0 - 0.3 * smoothstep(0.5, 0.85, d.masks.a);
        s.rough = 0.97;
    } else if (m == MAT_WOOD) {
        // Dry timber: grain streaks, grey where the sun bleached it, cracks.
        let grain2 = noise2(vec2<f32>((wpos.x + wpos.z) * 9.0, wpos.y * 0.7)) * 0.6
            + noise2(vec2<f32>(wpos.y * 9.0, (wpos.x - wpos.z) * 0.7)) * 0.4;
        s.albedo = base * (0.72 + 0.5 * grain2) * (0.85 + 0.3 * blotch);
        s.albedo = mix(s.albedo, vec3<f32>(0.5, 0.48, 0.44), up * 0.3);
        s.albedo *= 1.0 - 0.45 * smoothstep(0.6, 0.8, d.fine.b);
        s.rough = 0.9;
    }
    // Sand settles on everything that faces up.
    if (m != MAT_SAND) {
        let dust = smoothstep(0.55, 0.95, up) * clamp(0.3 + 0.6 * d.masks.g, 0.0, 1.0);
        s.albedo = mix(s.albedo, SAND_COL, dust * 0.65);
        s.rough = mix(s.rough, 0.96, dust);
        s.metal *= 1.0 - dust;
    }
    return s;
}

// GGX / Smith / Schlick specular, scaled by pi to match this renderer's
// un-normalized Lambert diffuse.
fn ggx_spec(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, rough: f32, f0: vec3<f32>) -> vec3<f32> {
    let h = normalize(v + l);
    let nl = max(dot(n, l), 0.0);
    let nv = max(dot(n, v), 0.001);
    let nh = max(dot(n, h), 0.0);
    let vh = max(dot(v, h), 0.0);
    let a = max(rough * rough, 0.002);
    let a2 = a * a;
    let dd = nh * nh * (a2 - 1.0) + 1.0;
    let d = a2 / (3.14159 * dd * dd);
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let gv = nv / (nv * (1.0 - k) + k);
    let gl = nl / (nl * (1.0 - k) + k);
    let f = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - vh, 5.0);
    return d * gv * gl * f / (4.0 * nv * max(nl, 0.001)) * 3.14159;
}

@fragment
fn fs_main(v: VOut) -> @location(0) vec4<f32> {
    var n = normalize(v.normal);
    let m = i32(round(v.color.a));
    let is_tank = m == MAT_CARBON || m == MAT_PAINT || m == MAT_TREAD || m == MAT_GLOW || m == MAT_METAL || m == MAT_WORN || m == MAT_VHAAL;
    // On tanks the instance tint is the team color and only affects paint.
    let paint = m == MAT_PAINT || m == MAT_WORN;
    let base = select(v.color.rgb * v.tint.rgb, v.color.rgb, is_tank && !paint);
    let duv = fwidth(plane_uv(v.lpos, normalize(v.lnormal)));
    let detail = surface_detail(v.lpos, v.lnormal, v.ax, v.ay, v.az, 0.45, 0.35);
    let is_world_detail = m == MAT_SAND || m == MAT_PLASTER || m == MAT_CONCRETE || m == MAT_RUBBLE || m == MAT_RUST || m == MAT_PAINTED || m == MAT_CLOTH || m == MAT_WOOD;
    var s: Surface;
    if (is_tank) {
        s = tank_surface(m, base, v.lpos, normalize(v.lnormal), v.wpos, duv);
    } else if (is_world_detail) {
        n = detail.normal;
        s = world_surface(m, base, v.wpos, n, detail);
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
    var c = vec3<f32>(0.0);
    if (is_world_detail) {
        // Physically based highlights and reflections for the detailed materials.
        let f0v = mix(vec3<f32>(0.04), s.albedo, s.metal);
        let kd = s.albedo * (1.0 - s.metal);
        let nv = max(dot(n, view), 0.0);
        c = (kd + ggx_spec(n, view, l, s.rough, f0v)) * g.sun_color.rgb * ndl * sh;
        let fr = f0v + (max(vec3<f32>(1.0 - s.rough), f0v) - f0v) * pow(1.0 - nv, 5.0);
        c += kd * hemi * 0.36 * s.ao + env * fr * s.ao * 0.4;
    } else {
        c = diffuse * hemi * 0.36 * s.ao;
        c += (diffuse + spec * f * spec_col * (1.0 - s.rough * 0.7)) * g.sun_color.rgb * ndl * sh;
        c += env * f * spec_col * (0.15 + 0.35 * s.metal) * (1.0 - s.rough) * s.ao;
    }
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
