#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::view::View

struct GameData {
    bg_color: vec4<f32>,
    circle_count: u32,
    spare1: u32,
    spare2: u32,
    spare3: u32,
}

@group(0) @binding(0) var<uniform> view: View;
@group(2) @binding(0) var<uniform> game: GameData;
@group(2) @binding(1) var pos_radius_tex: texture_2d<f32>;
@group(2) @binding(2) var pos_radius_sampler: sampler;
@group(2) @binding(3) var color_tex: texture_2d<f32>;
@group(2) @binding(4) var color_sampler: sampler;

// https://iquilezles.org/articles/distfunctions2d/
// https://iquilezles.org/articles/smin/


fn sdCircle(p: vec2<f32>, r: f32) -> f32 {
    return length(p) - r;
}

fn opSmoothUnion(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

fn smooth_color(c1: vec4<f32>, c2: vec4<f32>, k: f32) -> vec3<f32> {
    let h = clamp(0.5 + 0.5 * (c2.w - c1.w) / k, 0.0, 1.0);
    return mix(c2.rgb, c1.rgb, h);
}

fn blend_shapes(c1: vec4<f32>, c2: vec4<f32>, shape_k: f32, color_k: f32) -> vec4<f32> {
    let dist = opSmoothUnion(c1.w, c2.w, shape_k);
    let color = smooth_color(c1, c2, color_k); 
    return vec4(color, dist);
}

fn map(p: vec2<f32>) -> vec4<f32> {
    let cir_0_data = textureLoad(pos_radius_tex, vec2(0, 0), 0);
    let cir_0_color = textureLoad(color_tex, vec2(0, 0), 0);
    var shape = vec4(cir_0_color.rgb, sdCircle(p - cir_0_data.xy, cir_0_data.z));
    for (var i = 1u; i < game.circle_count; i += 1u) {
        let cir_data = textureLoad(pos_radius_tex, vec2(i, 0), 0);
        let cir_color = textureLoad(color_tex, vec2(i, 0), 0);
        var new_shape = vec4(cir_color.rgb, sdCircle(p - cir_data.xy, cir_data.z));
        shape = blend_shapes(shape, new_shape, 0.15, 0.3);
    }

    return shape;
}

@fragment
fn fragment(vert: VertexOutput) -> @location(0) vec4<f32> {
    let resolution = view.viewport.zw;
    let fragcoord = vert.position.xy;
    let p = (2.0 * fragcoord - resolution.xy) / resolution.y;

    let dc = map(p);

    let edge = smoothstep(0.0, 3.0 / resolution.y, dc.w); // aa
    var col = mix(dc.rgb, vec3(0.0), edge); 
    col = pow(col, vec3(5.0)); // Some rando color curve
    return vec4(col, 1.0);// * textureSample(base_color_texture, base_color_sampler, vert.uv);
}
