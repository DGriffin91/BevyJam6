/*
Windows hotpatch note:
set BEVY_ASSET_ROOT=.
set CARGO_TARGET_DIR=
dx serve --hot-patch --features bevy/file_watcher,subsecond
Save file before first run to trigger initial rebuild
*/

use bevy::asset::AssetMetaCheck;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages,
};
use bevy::sprite::{AlphaMode2d, Material2d, Material2dPlugin};
use bytemuck::cast_slice;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            // Wasm builds will check for meta files (that don't exist) if this isn't set.
            // This causes errors and even panics in web builds on itch.
            // See https://github.com/bevyengine/bevy_github_ci_template/issues/48.
            meta_check: AssetMetaCheck::Never,
            ..default()
        }))
        .add_plugins((
            #[cfg(feature = "subsecond")]
            bevy_simple_subsecond_system::prelude::SimpleSubsecondPlugin::default(),
            Material2dPlugin::<GameMaterial>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, greet)
        .run();
}

fn greet(time: Res<Time>) {
    info_once!(
        "Hello from a hotpatched system! Try changing this string while the app is running! Patched at t = {} s!!",
        time.elapsed_secs()
    );
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GameMaterial>>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        Camera2d,
        Camera {
            hdr: true,
            ..default()
        },
        Tonemapping::TonyMcMapface,
    ));
    let temp_pos_radius = vec![
        vec4(0.0, 0.0, 0.3, 0.0),
        vec4(0.3, 0.3, 0.2, 0.0),
        vec4(0.1, -0.3, 0.2, 0.0),
    ];
    let temp_color = vec![
        vec4(0.2, 0.2, 0.2, 0.2),
        vec4(1.0, 0.1, 0.1, 0.3),
        vec4(1.0, 0.1, 1.1, 0.2),
    ];
    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO))),
        MeshMaterial2d(materials.add(GameMaterial {
            data: GameData {
                bg_color: vec4(1.0, 0.0, 1.0, 1.0),
                circle_count: temp_pos_radius.len() as u32,
                ..default()
            },
            pos_radius_tex: images.add(data_image(&temp_pos_radius)),
            color_tex: images.add(data_image(&temp_color)),
        })),
        Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
    ));
}

#[derive(ShaderType, Debug, Clone, Default)]
struct GameData {
    bg_color: Vec4,
    circle_count: u32,
    spare1: u32,
    spare2: u32,
    spare3: u32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct GameMaterial {
    #[uniform(0)]
    data: GameData,
    #[texture(1)]
    #[sampler(2)]
    pos_radius_tex: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    color_tex: Handle<Image>,
}

impl Material2d for GameMaterial {
    fn fragment_shader() -> ShaderRef {
        "game.wgsl".into()
    }

    fn vertex_shader() -> ShaderRef {
        "game.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

fn data_image(data: &[Vec4]) -> Image {
    Image {
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width: data.len() as u32,
                height: 1,
                ..default()
            },
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba32Float,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        data: Some(cast_slice(data).to_vec()),
        ..Default::default()
    }
}
