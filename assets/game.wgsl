#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::view::View

struct GameData {
    bg_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(2) @binding(0) var<uniform> game: GameData;
@group(2) @binding(1) var base_color_texture: texture_2d<f32>;
@group(2) @binding(2) var base_color_sampler: sampler;

struct FullscreenVertexOutput {
    @builtin(position)
    position: vec4<f32>,
    @location(0)
    uv: vec2<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> FullscreenVertexOutput {
    let uv = vec2<f32>(f32(vertex_index >> 1u), f32(vertex_index & 1u)) * 2.0;
    let clip_position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return FullscreenVertexOutput(clip_position, uv);
}

// https://iquilezles.org/articles/distfunctions2d/
// https://iquilezles.org/articles/smin/

fn opSmoothUnion(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

fn sdCircle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn map(p: vec2<f32>) -> f32 {
  let dg1 = sdCircle(p - vec2(0.3, 0.3), 0.2);
  let dg2 = sdCircle(p, 0.3);

  return opSmoothUnion(dg1, dg2, 0.2);
}

@fragment
fn fragment(vert: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let resolution = view.viewport.zw;
    let fragcoord = vert.position.xy;
	let p = (2.0 * fragcoord - resolution.xy)/resolution.y;
    let d = map(p);
    var col = vec3(1.0, 0.0, 0.0);
	col = mix(col, vec3(1.0), smoothstep(0.0, 3.0 / resolution.y, d));
    return vec4(col, 1.0) * textureSample(base_color_texture, base_color_sampler, vert.uv);
}
