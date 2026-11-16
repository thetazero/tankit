// Screen-space ambient occlusion: for each pixel, reconstruct its world
// position from depth, scatter samples in the hemisphere above it, and count
// how many land behind other geometry. Output: 1 = open, 0 = fully enclosed.
// Works at pixel resolution, so it catches contact between separate parts
// (turret on hull, hull over tracks, vehicle on ground) that per-vertex
// baking on big flat plates can't.

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
    params: vec4<f32>,       // z: screen width, w: screen height
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var depth_tex: texture_depth_2d;

@vertex
fn vs_ao(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn size() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth_tex));
}

fn world_at(px: vec2<i32>) -> vec3<f32> {
    let s = size();
    let q = clamp(px, vec2<i32>(0), s - 1);
    let d = textureLoad(depth_tex, q, 0);
    let ndc = vec2<f32>((f32(q.x) + 0.5) / f32(s.x) * 2.0 - 1.0, 1.0 - (f32(q.y) + 0.5) / f32(s.y) * 2.0);
    let w = g.inv_view_proj * vec4<f32>(ndc, d, 1.0);
    return w.xyz / w.w;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs_ao(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(frag.xy);
    let d = textureLoad(depth_tex, px, 0);
    if (d >= 1.0) {
        return vec4<f32>(1.0);
    }
    let p = world_at(px);
    // Normal from depth: pick the neighbor on each axis that's closer, so
    // silhouette edges don't produce garbage normals.
    let r = world_at(px + vec2<i32>(1, 0)) - p;
    let l = p - world_at(px - vec2<i32>(1, 0));
    let u = world_at(px + vec2<i32>(0, 1)) - p;
    let dn = p - world_at(px - vec2<i32>(0, 1));
    let dx = select(l, r, dot(r, r) < dot(l, l));
    let dy = select(dn, u, dot(u, u) < dot(dn, dn));
    var n = normalize(cross(dx, dy));
    let to_cam = g.cam_pos.xyz - p;
    if (dot(n, to_cam) < 0.0) {
        n = -n;
    }
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let t = normalize(cross(up, n));
    let b = cross(n, t);

    let radius = 0.75;
    let cam_dist = length(to_cam);
    let spin = hash(frag.xy) * 6.2831;
    let count = 16;
    var occ = 0.0;
    for (var i = 0; i < count; i++) {
        let fi = f32(i);
        // Cosine-ish hemisphere spiral, denser near the surface point.
        let a = fi * 2.399 + spin;
        let rr = sqrt((fi + 0.5) / f32(count));
        let h = sqrt(max(1.0 - rr * rr, 0.0));
        let scale = mix(0.15, 1.0, (fi / f32(count)) * (fi / f32(count)));
        let dir = t * (cos(a) * rr) + b * (sin(a) * rr) + n * h;
        let s = p + n * 0.03 + dir * radius * scale;
        let c = g.view_proj * vec4<f32>(s, 1.0);
        if (c.w <= 0.0) {
            continue;
        }
        let ndc = c.xy / c.w;
        let spx = vec2<i32>(vec2<f32>((ndc.x * 0.5 + 0.5) * f32(size().x), (0.5 - ndc.y * 0.5) * f32(size().y)));
        if (spx.x < 0 || spx.y < 0 || spx.x >= size().x || spx.y >= size().y) {
            continue;
        }
        let q = world_at(spx);
        let sd = length(s - g.cam_pos.xyz);
        let qd = length(q - g.cam_pos.xyz);
        // Occluded if the scene surface along that ray is in front of the sample,
        // weighted down when that surface is far away (no halos around silhouettes).
        let range = smoothstep(0.0, 1.0, radius / max(length(q - p), 0.001));
        occ += select(0.0, 1.0, qd < sd - 0.02) * range;
    }
    let ao = 1.0 - occ / f32(count);
    // Fade out with distance so far geometry doesn't turn into noise.
    let fade = smoothstep(80.0, 30.0, cam_dist);
    return vec4<f32>(mix(1.0, pow(clamp(ao, 0.0, 1.0), 2.0), fade));
}
