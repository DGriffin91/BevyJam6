#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#import bevy_render::view::View
#import bevy_render::globals::Globals

struct GameData {
    bg_color: vec4<f32>,
    circle_count: u32,
    spare1: u32,
    spare2: u32,
    spare3: u32,
}

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(2) @binding(0) var<uniform> game: GameData;
@group(2) @binding(1) var pos_radius_tex: texture_2d<f32>;
@group(2) @binding(2) var pos_radius_sampler: sampler;
@group(2) @binding(3) var color_tex: texture_2d<f32>;
@group(2) @binding(4) var color_sampler: sampler;

@group(2) @binding(5) var base_color_texture: texture_2d<f32>;
@group(2) @binding(6) var base_color_sampler: sampler;

@group(2) @binding(7) var ripple_texture: texture_2d<f32>;
@group(2) @binding(8) var ripple_sampler: sampler;

// https://iquilezles.org/articles/distfunctions2d/
// https://iquilezles.org/articles/smin/

struct BlobData {
    color: vec3<f32>,
    position: vec2<f32>,
    radius: f32,
}

fn load_blob_pos_radius(index: u32) -> vec3<f32> {
    return textureLoad(pos_radius_tex, vec2(index, 0), 0).xyz;
}

fn load_blob_data(index: u32) -> BlobData {
    let cir_data = textureLoad(pos_radius_tex, vec2(index, 0), 0);
    let cir_color = textureLoad(color_tex, vec2(index, 0), 0);

    var blob: BlobData;

    blob.color = cir_color.rgb;
    blob.position = cir_data.xy;
    blob.radius = cir_data.z;

    return blob;
}

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

fn map_height(p: vec2<f32>) -> f32 {
    let blob = load_blob_data(0);
    var c1 = 0.0;
    for (var i = 0u; i < game.circle_count; i += 1u) {
        let pos_radius = load_blob_pos_radius(i);
        var c2 = sdCircle(p - pos_radius.xy, pos_radius.z);
        var shape_k = max(blob.radius * 0.5, 0.001);
        c1 = opSmoothUnion(c1, c2, shape_k);
    }
    var d = max(0.0, -c1);
    d = pow(d, 1.0 / 1.0);
    return d;
}

fn map(p: vec2<f32>) -> vec4<f32> {
    let blob = load_blob_data(0);
    var shape = vec4(0.0,0.0,0.0,1.0);
    for (var i = 0u; i < game.circle_count; i += 1u) {
        var blob = load_blob_data(i);
        // Brighten as it approaches time to turn white
        if blob.radius < 0.15 && blob.radius > 0.1 {
            blob.color *= 1.0 + saturate(-(blob.radius - 0.15)) * 4.0;
        }
        var new_shape = vec4(blob.color, sdCircle(p - blob.position, blob.radius));


        var shape_k = max(blob.radius * 0.5, 0.001);
        var color_k = max(blob.radius * 0.5, 0.001);

        //if cir_data.z > 0.0 {
        shape = blend_shapes(shape, new_shape, shape_k, color_k);
        //}
    }

    return shape;
}

fn ready_to_click_map(p: vec2<f32>) -> vec4<f32> {
    let blob = load_blob_data(0);
    var shape = vec4(0.0,0.0,0.0,1.0);
    for (var i = 0u; i < game.circle_count; i += 1u) {
        let blob = load_blob_data(i);
        if all(blob.color == vec3(2.0, 2.0, 2.0)) {
            var new_shape = vec4(blob.color, sdCircle(p - blob.position, blob.radius * 0.8));
            shape = blend_shapes(shape, new_shape, 0.01, 0.01);
        }
    }

    return shape;
}

fn refract(I: vec3<f32>, N: vec3<f32>, eta: f32) -> vec3<f32> {
    let k = max((1.0 - eta * eta * (1.0 - dot(N, I) * dot(N, I))), 0.0);
    return eta * I - (eta * dot(N, I) + sqrt(k)) * N;
}

@fragment
fn fragment(vert: VertexOutput) -> @location(0) vec4<f32> {
    let resolution = view.viewport.zw;
    let fragcoord = vert.position.xy;
    let frag_size = 1.0 / resolution;
    let frag_uv = fragcoord / resolution;
    var p = (2.0 * fragcoord - resolution.xy) / resolution.y;

    let ripple = textureSample(ripple_texture, ripple_sampler, frag_uv);

    p += ripple.zw * 0.5;

    var p1 = vec2(p);
    var p2 = vec2(p + vec2(frag_size.x * 0.5, 0.0));
    var p3 = vec2(p + vec2(0.0, frag_size.y * 0.5));

    let dc = map(p);

    var h = map_height(p1.xy);

    h += ripple.x * 0.05;

    

    let dxy = h - vec2(
        map_height(p + vec2(frag_size.x, 0.)), 
        map_height(p + vec2(0., frag_size.y))
   );
    
    var nor = normalize(vec3(dxy * resolution, h * resolution.y * 0.1));
    nor.y = -nor.y;
    nor.x = saturate(abs(nor.x) - h * 0.2) * sign(nor.x);
    nor.y = saturate(abs(nor.y) - h * 0.2) * sign(nor.y);

    let mask = 1.0 - smoothstep(0.0, 3.0 / resolution.y, dc.w); // aa
    var col = mix(vec3(0.0), dc.rgb, mask); 

    col = pow(col * 1.5, vec3(10.0)); // Some rando color curve

    let dc2 = ready_to_click_map(p);
    let edge2 = smoothstep(0.0, 1.0 / resolution.y, dc2.w); // Highlight
    let highlight = mix(dc2.rgb, vec3(0.0), edge2);

    let fresnel = saturate(pow((0.2 - saturate(-dc.w)) * 5.0, 9.0) * mask);

    var bg = vec3(0.0);

    
    var sky = textureSample(base_color_texture, base_color_sampler, (abs(p.xy * vec2(0.0, 1.0) * 0.5 + ripple.x * 2.0 + vec2(globals.time * 0.1, 0.0))) % 1.0).rgb;
    bg += pow(sky * 0.7, vec3(3.0));

    // Glint
    //let glint_normal = normalize(vec3(ripple.z * 0.1, ripple.w * 0.1, ripple.x));
    //let lightDir = normalize(vec3(-3.0, 10.0, 3.0));
    //let diffuse = pow(max(dot(glint_normal, lightDir), 0.0), 3.0);
    //bg += diffuse * sky + pow(sky, vec3(3.0));


    let refr_d = refract(vec3(p, h), nor, 1.0/1.52);
    let refr = textureSample(base_color_texture, base_color_sampler, (abs(refr_d.xy)) % 1.0).rgb * 2.0;
    bg = mix(bg, refr * col, mask);

    let refl_d = reflect(vec3(p, h), nor);
    let refl = textureSample(base_color_texture, base_color_sampler, (abs(refl_d.xy)) % 1.0).rgb * 2.0;
    bg += mix(bg, refl * col * fresnel * 20.0, mask);

    bg = mix(bg, col * fresnel * 0.5 + col * h, 0.5 * mask);

    bg += highlight * col * (fresnel + 1.0) * 6.0; 

    //return vec4(vec3(highlight), 1.0);
    return vec4(bg, 1.0);
    //return vec4(nor, 1.0);
}
