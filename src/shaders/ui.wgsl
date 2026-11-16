// 2D overlay (HUD), positions in pixels: flat colour, the thumbnail atlas
// tinted by the colour where a vertex has a UV, or text from the font's
// distance-field atlas where it has an edge ramp (`sdf.y`).

struct UiGlobals {
    screen: vec4<f32>, // xy: size in pixels
};
@group(0) @binding(0) var<uniform> ui: UiGlobals;
@group(0) @binding(1) var font: texture_2d<f32>;
@group(0) @binding(2) var font_sampler: sampler;
// Distance-field reach either side of an outline, font atlas px (`ui::font::SPREAD`).
const SPREAD: f32 = 8.0;
@group(1) @binding(0) var thumbs: texture_2d<f32>;
@group(1) @binding(1) var thumbs_sampler: sampler;

struct UiIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) sdf: vec2<f32>,
};
struct UiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) sdf: vec2<f32>,
};

@vertex
fn vs_ui(v: UiIn) -> UiOut {
    var o: UiOut;
    let ndc = v.pos / ui.screen.xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    o.clip = vec4<f32>(ndc, 0.0, 1.0);
    o.color = v.color;
    o.uv = v.uv;
    o.sdf = v.sdf;
    return o;
}

@fragment
fn fs_ui(v: UiOut) -> @location(0) vec4<f32> {
    // Sampled up front: implicit mip selection needs uniform control flow.
    let field = textureSample(font, font_sampler, v.uv).r;
    // Font atlas px per screen px, to turn the field into screen px.
    let size = vec2<f32>(textureDimensions(font));
    let texels = max(length(dpdx(v.uv) * size), length(dpdy(v.uv) * size));
    if (v.sdf.y > 0.0) {
        // Signed distance to the outline in screen px, positive inside.
        let d = (field - 0.5) * 2.0 * SPREAD / max(texels, 1e-4);
        let cover = clamp((d + v.sdf.x) / v.sdf.y + 0.5, 0.0, 1.0);
        return vec4<f32>(v.color.rgb, v.color.a * cover);
    }
    if (v.uv.x < 0.0) {
        return v.color;
    }
    return textureSampleLevel(thumbs, thumbs_sampler, v.uv, 0.0) * v.color;
}
