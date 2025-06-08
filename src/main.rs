/*
Windows hotpatch note:
set BEVY_ASSET_ROOT=.
set CARGO_TARGET_DIR=
dx serve --hot-patch --features bevy/file_watcher,subsecond
Save file before first run to trigger initial rebuild
*/

use bevy::asset::{AssetMetaCheck, RenderAssetUsages};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::diagnostic::{FrameCount, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages,
};
use bevy::render::view::RenderLayers;
use bevy::sprite::{Material2d, Material2dPlugin};
use bevy::window::PresentMode;
use bevy::winit::{UpdateMode, WinitSettings};
use bytemuck::cast_slice;

use crate::sampling::{hash_noise, hash_noise_signed};

pub mod sampling;

fn main() {
    App::new()
        .init_resource::<MousePosition>()
        .insert_resource(BlobClickableSize(0.1))
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::Continuous,
        })
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    // Wasm builds will check for meta files (that don't exist) if this isn't set.
                    // This causes errors and even panics in web builds on itch.
                    // See https://github.com/bevyengine/bevy_github_ci_template/issues/48.
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: String::from("Game"),
                        present_mode: PresentMode::AutoVsync,
                        fit_canvas_to_parent: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            #[cfg(feature = "subsecond")]
            bevy_simple_subsecond_system::prelude::SimpleSubsecondPlugin::default(),
            Material2dPlugin::<GameMaterial>::default(),
            Material2dPlugin::<RippleMaterial>::default(),
        ))
        .add_systems(Startup, (setup, spawn_blobs))
        .add_systems(
            Update,
            (
                handle_mouse_move,
                click_blobs,
                shrink_blobs,
                set_blob_state,
                move_blobs,
                render_blobs,
                set_game_text,
                ripple_swap,
            )
                .chain(),
        )
        .run();
}

#[derive(Resource, Clone, Copy, Deref, DerefMut)]
pub struct BlobClickableSize(pub f32);

#[derive(Component, Clone, Copy, Deref, DerefMut)]
pub struct BlobSizeRadius(pub f32);

#[derive(Component, Clone, Copy, Deref, DerefMut)]
pub struct BlobPosition(pub Vec2);

#[derive(Component, Clone, Copy, Deref, DerefMut)]
pub struct BlobVelocity(pub Vec2);

#[derive(Component, Clone, Copy, Deref, DerefMut)]
pub struct BlobColor(pub Vec3);

#[derive(Clone, Copy, Component)]
pub struct BlobCanBeClicked;

fn spawn_blobs(mut commands: Commands) {
    for i in 0..32 {
        let vel_rng = vec2(hash_noise_signed(0, i, 1), hash_noise_signed(0, i, 2));

        commands.spawn((
            BlobSizeRadius(0.15 + hash_noise(i, 0, 0) * 0.2),
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

fn shrink_blobs(blobs: Query<&mut BlobSizeRadius>, time: Res<Time>) {
    let shink_speed = 0.02;
    for mut blob_size in blobs {
        **blob_size -= time.delta_secs() * shink_speed;
        //**blob_size = blob_size.max(0.0);
    }
}

fn set_blob_state(
    mut commands: Commands,
    blobs: Query<(Entity, &BlobSizeRadius)>,
    clickable_size: Res<BlobClickableSize>,
) {
    for (entity, blob_size) in blobs {
        if **blob_size < **clickable_size {
            commands.entity(entity).insert(BlobCanBeClicked);
        } else {
            commands.entity(entity).remove::<BlobCanBeClicked>();
        }
    }
}

fn move_blobs(
    blobs: Query<(
        &BlobSizeRadius,
        &mut BlobPosition,
        &mut BlobVelocity,
        &BlobColor,
    )>,
    window: Single<&Window>,
    time: Res<Time>,
) {
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

#[derive(Resource, Clone, Debug, Default)]
pub struct MousePosition {
    pub window_rel: Vec2,
    pub ndc: Vec2,
}

fn handle_mouse_move(
    mut cursor_events: EventReader<CursorMoved>,
    mut mouse_position: ResMut<MousePosition>,
    window: Single<&Window>,
) {
    if let Some(cursor_event) = cursor_events.read().last() {
        let window_size = window.resolution.size();
        let window_ratio = window_size.x / window_size.y;
        mouse_position.ndc = cursor_event.position / window_size * 2.0 - 1.0;
        mouse_position.window_rel = mouse_position.ndc;
        mouse_position.window_rel.x *= window_ratio;
    }
}

fn click_blobs(
    mut button_events: EventReader<MouseButtonInput>,
    mouse_position: Res<MousePosition>,
    blobs: Query<(
        &mut BlobSizeRadius,
        &mut BlobPosition,
        &BlobColor,
        Has<BlobCanBeClicked>,
    )>,
) {
    let mut clicked = false;
    for button_event in button_events.read() {
        if button_event.button == MouseButton::Left {
            clicked = true;
        }
    }

    if clicked {
        for (mut size, pos, _color, can_be_clicked) in blobs {
            if can_be_clicked {
                if pos.distance(mouse_position.window_rel) < **size {
                    **size += 0.3;
                }
            }
        }
    }
}

fn render_blobs(
    blobs: Query<(
        &BlobSizeRadius,
        &BlobPosition,
        &BlobColor,
        Has<BlobCanBeClicked>,
    )>,
    mut game_materials: ResMut<Assets<GameMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let temp_click_color = Vec3::ONE * 2.0;

    let (_, game_material) = game_materials.iter_mut().next().unwrap();
    let mut temp_pos_radius = vec![];
    let mut temp_color = vec![];

    for (size, pos, color, can_be_clicked) in blobs {
        if !can_be_clicked {
            temp_pos_radius.push(pos.extend(**size).extend(0.0));
            temp_color.push(color.extend(0.0));
        }
    }

    for (size, pos, _color, can_be_clicked) in blobs {
        if can_be_clicked {
            temp_pos_radius.push(pos.extend(**size).extend(0.0));
            temp_color.push(temp_click_color.extend(0.0));
        }
    }

    game_material.pos_radius_tex = images.add(data_image(&temp_pos_radius));
    game_material.color_tex = images.add(data_image(&temp_color));
    game_material.data.circle_count = temp_pos_radius.len() as u32;
}

fn set_game_text(mut text: Single<&mut Text, With<GameText>>, blobs: Query<&BlobSizeRadius>) {
    let mut alive_count = 0;
    for blob_size in blobs {
        if **blob_size > 0.0 {
            alive_count += 1;
        }
    }
    text.clear();
    text.push_str(&format!("{alive_count} alive\n"));
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GameMaterial>>,
    mut ripple_materials: ResMut<Assets<RippleMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    let ripple_images = RippleImages::new(vec2(1280.0, 720.0), &mut images);

    commands.spawn((
        Msaa::Off,
        Camera2d::default(),
        Camera {
            hdr: true,
            target: ripple_images.a.clone().into(),
            ..default()
        },
        RenderLayers::layer(1),
        RippleCamera,
    ));

    commands.spawn((
        Mesh2d(meshes.add(fullscreen_tri())),
        MeshMaterial2d(ripple_materials.add(RippleMaterial {
            mouse_pos_dt: Vec4::ZERO,
            prev_tex: ripple_images.b.clone().into(),
        })),
        Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
        RenderLayers::layer(1),
    ));

    commands.spawn((
        Msaa::Off,
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
        Mesh2d(meshes.add(fullscreen_tri())),
        MeshMaterial2d(materials.add(GameMaterial {
            data: GameData {
                bg_color: vec4(1.0, 0.0, 1.0, 1.0),
                circle_count: temp_pos_radius.len() as u32,
                ..default()
            },
            pos_radius_tex: images.add(data_image(&temp_pos_radius)),
            color_tex: images.add(data_image(&temp_color)),
            bg_tex: asset_server.load("sky.jpg"),
            ripple_tex: ripple_images.a.clone(),
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

    commands.insert_resource(ripple_images);
}

fn ripple_swap(
    mut button_events: EventReader<MouseButtonInput>,
    mut ripple_images: ResMut<RippleImages>,
    mut camera: Single<&mut Camera, With<RippleCamera>>,
    window: Single<&Window>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut ripple_materials: ResMut<Assets<RippleMaterial>>,
    mut game_materials: ResMut<Assets<GameMaterial>>,
    mouse_position: Res<MousePosition>,
    frame: Res<FrameCount>,
) {
    let mut clicked = false;
    for button_event in button_events.read() {
        if button_event.button == MouseButton::Left {
            clicked = true;
        }
    }

    let init = frame.0 < 20;
    ripple_images.swap();
    let res = window.resolution.physical_size().as_vec2();
    if ripple_images.res != res {
        *ripple_images = RippleImages::new(res, &mut images);
    }
    camera.target = ripple_images.a.clone().into();
    let (_, ripple_material) = ripple_materials.iter_mut().next().unwrap();
    ripple_material.mouse_pos_dt = vec4(
        mouse_position.ndc.x,
        mouse_position.ndc.y,
        if clicked {
            1.0
        } else {
            if init { -1.0 } else { 0.0 }
        },
        time.delta_secs(),
    );
    ripple_material.prev_tex = ripple_images.b.clone();

    let (_, game_material) = game_materials.iter_mut().next().unwrap();
    game_material.ripple_tex = ripple_images.a.clone();
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
    #[texture(5)]
    #[sampler(6)]
    bg_tex: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    ripple_tex: Handle<Image>,
}

impl Material2d for GameMaterial {
    fn fragment_shader() -> ShaderRef {
        "game.wgsl".into()
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct RippleMaterial {
    #[uniform(0)]
    mouse_pos_dt: Vec4,
    #[texture(1)]
    #[sampler(2)]
    prev_tex: Handle<Image>,
}

impl Material2d for RippleMaterial {
    fn fragment_shader() -> ShaderRef {
        "ripple.wgsl".into()
    }
}

#[derive(Component, Clone, Copy)]
struct RippleCamera;

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

#[derive(Resource)]
pub struct RippleImages {
    pub a: Handle<Image>,
    pub b: Handle<Image>,
    pub res: Vec2,
}

impl RippleImages {
    pub fn new(res: Vec2, images: &mut Assets<Image>) -> RippleImages {
        let size = Extent3d {
            width: res.x as u32,
            height: res.y as u32,
            ..default()
        };
        let mut image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[0, 0, 0, 0, 0, 0, 0, 0],
            TextureFormat::Rgba16Float,
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;
        RippleImages {
            a: images.add(image.clone()),
            b: images.add(image),
            res,
        }
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.a, &mut self.b);
    }
}

fn fullscreen_tri() -> Triangle2d {
    // lol
    Triangle2d::new(
        Vec2::new(-10000., -100000.),
        Vec2::new(-10000., 10000.),
        Vec2::new(100000., 10000.),
    )
}
