// 2D overlay (HUD), positions in pixels: flat colour, or the thumbnail atlas
// tinted by the colour where a vertex has a UV.

struct UiGlobals {
    screen: vec4<f32>, // xy: size in pixels
};
@group(0) @binding(0) var<uniform> ui: UiGlobals;
@group(1) @binding(0) var thumbs: texture_2d<f32>;
@group(1) @binding(1) var thumbs_sampler: sampler;

struct UiIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
};
struct UiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_ui(v: UiIn) -> UiOut {
    var o: UiOut;
    let ndc = v.pos / ui.screen.xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    o.clip = vec4<f32>(ndc, 0.0, 1.0);
    o.color = v.color;
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_ui(v: UiOut) -> @location(0) vec4<f32> {
    if (v.uv.x < 0.0) {
        return v.color;
    }
    return textureSampleLevel(thumbs, thumbs_sampler, v.uv, 0.0) * v.color;
}
