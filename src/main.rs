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

use crate::sampling::{hash_noise, hash_noise_signed};
pub mod sampling;

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
        .add_systems(Startup, (setup, spawn_blobs))
        .add_systems(
            Update,
            (
                shrink_blobs,
                set_blob_state,
                move_blobs,
                render_blobs,
                set_game_text,
            )
                .chain(),
        )
        .run();
}

#[derive(Clone, Copy, Component, Deref, DerefMut)]
pub struct BlobSize(pub f32);

#[derive(Clone, Copy, Component, Deref, DerefMut)]
pub struct BlobPosition(pub Vec2);

#[derive(Clone, Copy, Component, Deref, DerefMut)]
pub struct BlobVelocity(pub Vec2);

#[derive(Clone, Copy, Component, Deref, DerefMut)]
pub struct BlobColor(pub Vec3);

#[derive(Clone, Copy, Component)]
pub struct BlobCanBeClicked;

fn spawn_blobs(mut commands: Commands) {
    for i in 0..32 {
        let vel_rng = vec2(hash_noise_signed(0, i, 1), hash_noise_signed(0, i, 2));

        commands.spawn((
            BlobSize(0.15 + hash_noise(i, 0, 0) * 0.2),
            BlobPosition(vec2(
                hash_noise_signed(0, i, 1) * 0.5,
                hash_noise_signed(0, i, 2) * 0.5,
            )),
            BlobVelocity(0.2 * vel_rng.signum() + vel_rng * 0.1),
            BlobColor(vec3(
                0.2 + hash_noise(i, i, 1) * 0.5,
                0.2 + hash_noise(i, i, 2) * 0.5,
                0.2 + hash_noise(i, i, 3) * 0.5,
            )),
        ));
    }
}

fn shrink_blobs(blobs: Query<&mut BlobSize>, time: Res<Time>) {
    let shink_speed = 0.02;
    for mut blob_size in blobs {
        **blob_size -= time.delta_secs() * shink_speed;
        //**blob_size = blob_size.max(0.0);
    }
}

fn set_blob_state(mut commands: Commands, blobs: Query<(Entity, &BlobSize)>) {
    let temp_size_thresh = 0.1;
    for (entity, blob_size) in blobs {
        if **blob_size < temp_size_thresh {
            commands.entity(entity).insert(BlobCanBeClicked);
        } else {
            commands.entity(entity).remove::<BlobCanBeClicked>();
        }
    }
}

fn move_blobs(
    blobs: Query<(&BlobSize, &mut BlobPosition, &mut BlobVelocity, &BlobColor)>,
    window: Query<&Window>,
    time: Res<Time>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let window_size = window.resolution.physical_size().as_vec2();
    let window_ratio = window_size.x / window_size.y;
    for (size, mut pos, mut vel, _color) in blobs {
        **pos += **vel * time.delta_secs();

        // bounce off walls
        if pos.x - **size < -window_ratio {
            vel.x = -vel.x;
        }
        if pos.y - **size < -1.0 {
            vel.y = -vel.y;
        }
        if pos.x + **size > window_ratio {
            vel.x = -vel.x;
        }
        if pos.y + **size > 1.0 {
            vel.y = -vel.y;
        }
    }
}

fn render_blobs(
    blobs: Query<(&BlobSize, &BlobPosition, &BlobColor, Has<BlobCanBeClicked>)>,
    mut game_materials: ResMut<Assets<GameMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let temp_click_color = Vec3::ONE * 2.0;

    let (_, game_material) = game_materials.iter_mut().next().unwrap();
    let mut temp_pos_radius = vec![];
    let mut temp_color = vec![];
    for (size, pos, color, can_be_clicked) in blobs {
        temp_pos_radius.push(pos.extend(**size).extend(0.0));
        let color = if can_be_clicked {
            temp_click_color
        } else {
            **color
        };
        temp_color.push(color.extend(0.0));
    }
    game_material.pos_radius_tex = images.add(data_image(&temp_pos_radius));
    game_material.color_tex = images.add(data_image(&temp_color));
    game_material.data.circle_count = temp_pos_radius.len() as u32;
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GameMaterial>>,
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
    let temp_pos_radius = vec![Vec4::ZERO];
    let temp_color = vec![Vec4::ZERO];
    commands.spawn((
        Mesh2d(meshes.add(Triangle2d::new(
            Vec2::new(-10000., -100000.),
            Vec2::new(-10000., 10000.),
            Vec2::new(100000., 10000.),
        ))),
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

    commands.spawn((
        Text::default(),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
        GameText,
    ));
}

fn set_game_text(mut text: Single<&mut Text, With<GameText>>, blobs: Query<&BlobSize>) {
    let mut alive_count = 0;
    for blob_size in blobs {
        if **blob_size > 0.0 {
            alive_count += 1;
        }
    }
    text.clear();
    text.push_str(&format!("{alive_count} alive\n"));
}

#[derive(Component)]
struct GameText;

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
