// Soft, camera-facing, alpha-blended particles (dust, blown sand). Drawn into
// the HDR target after the opaque scene: depth-tested, not depth-written.

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
};

struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) wpos: vec3<f32>,
    @location(3) streak: f32,
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
    return o;
}

@fragment
fn fs_particle(v: POut) -> @location(0) vec4<f32> {
    let r2 = dot(v.uv, v.uv);
    // Round puffs: soft all over. Streaks: solid across, tapering at the ends.
    let puff = (1.0 - r2) * (1.0 - r2);
    let line = (1.0 - v.uv.x * v.uv.x) * (1.0 - abs(v.uv.y));
    let soft = max(mix(puff, line, v.streak), 0.0);
    if (soft <= 0.0) {
        discard;
    }
    // Lit like a sunlit cloud of grains: sun from above, sky and bounce fill.
    let sun = g.sun_color.rgb * (0.35 + 0.35 * max(g.sun_dir.y, 0.0)) * sunlit(v.wpos);
    let fill = mix(g.ground_color.rgb, g.sky_horizon.rgb, 0.6) * 0.55;
    var c = v.color.rgb * (sun + fill);
    // Same haze as the scene.
    let d = distance(v.wpos, g.cam_pos.xyz);
    let f = 1.0 - exp(-d * g.params.y);
    c = mix(c, g.sky_horizon.rgb, f);
    return vec4<f32>(c, v.color.a * soft);
}
