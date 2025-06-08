// https://www.shadertoy.com/view/wdtyDH

#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;
@group(2) @binding(0) var<uniform> mouse_pos_dt: vec4<f32>;
@group(2) @binding(1) var prev_tex: texture_2d<f32>;
@group(2) @binding(2) var prev_tex_samp: sampler;

const delta: f32 = 1.4;

@fragment
fn fragment(
    vert: VertexOutput
) -> @location(0) vec4<f32> {
    let resolution = view.viewport.zw;
    let coord = vec2<i32>(vert.position.xy);
    let fragcoord = vert.position.xy;
    let p = (2.0 * fragcoord - resolution.xy) / resolution.y;

    let mouse_pos = (mouse_pos_dt.xy * 0.5 + 0.5) * resolution.xy;
    let mouse_click = mouse_pos_dt.z;
    let dt = mouse_pos_dt.w;

    // TODO use dt

    if (mouse_click == -1.0) {
        return vec4<f32>(0.0);
    }

    let texSize = vec2<i32>(resolution.xy);

    let center = textureLoad(prev_tex, coord, 0);
    var pressure = center.x;
    var pVel = center.y;

    var p_right = textureLoad(prev_tex, coord + vec2<i32>(1, 0), 0).x;
    var p_left = textureLoad(prev_tex, coord + vec2<i32>(-1, 0), 0).x;
    var p_up = textureLoad(prev_tex, coord + vec2<i32>(0, 1), 0).x;
    var p_down = textureLoad(prev_tex, coord + vec2<i32>(0, -1), 0).x;

    // Boundary conditions
    if (fragcoord.x == 0.5) {
        p_left = p_right;
    }
    if (fragcoord.x == resolution.x - 0.5) {
        p_right = p_left;
    }
    if (fragcoord.y == 0.5) {
        p_down = p_up;
    }
    if (fragcoord.y == resolution.y - 0.5) {
        p_up = p_down;
    }

    // Horizontal wave
    pVel += delta * (-2.0 * pressure + p_right + p_left) / 4.0;
    // Vertical wave
    pVel += delta * (-2.0 * pressure + p_up + p_down) / 4.0;

    // Update pressure
    pressure += delta * pVel;

    // Spring motion
    pVel -= 0.005 * delta * pressure;

    // Damping
    pVel *= 1.0 - 0.002 * delta;
    pressure *= 0.999;

    var result = vec4<f32>(
        pressure,
        pVel,
        (p_right - p_left) / 2.0,
        (p_up - p_down) / 2.0
    );

    if (mouse_click >= 1.0) {
        let dist = distance(fragcoord.xy, mouse_pos) / resolution.y;
        if (dist <= 0.05) {
            result.x += dist * 10.0;
        }
    }

    return result;
}
