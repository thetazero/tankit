// Flat-colored 2D overlay (HUD), positions in pixels.

struct UiGlobals {
    screen: vec4<f32>, // xy: size in pixels
};
@group(0) @binding(0) var<uniform> ui: UiGlobals;

struct UiIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};
struct UiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_ui(v: UiIn) -> UiOut {
    var o: UiOut;
    let ndc = v.pos / ui.screen.xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    o.clip = vec4<f32>(ndc, 0.0, 1.0);
    o.color = v.color;
    return o;
}

@fragment
fn fs_ui(v: UiOut) -> @location(0) vec4<f32> {
    return v.color;
}
