/*
Windows hotpatch note:
set BEVY_ASSET_ROOT=.
set CARGO_TARGET_DIR=
dx serve --hot-patch --features bevy/file_watcher,subsecond
Save file before first run to trigger initial rebuild
*/

use argh::FromArgs;
use bevy::asset::{AssetMetaCheck, RenderAssetUsages};
use bevy::audio::{PlaybackMode, Volume};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::diagnostic::{FrameCount, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::input::ButtonState;
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
use bevy_framepace::{FramepaceSettings, Limiter};
use bytemuck::cast_slice;

use crate::sampling::{hash_noise, hash_noise_signed};

pub mod sampling;

#[derive(Clone, Eq, PartialEq, Debug, Hash, Default, States)]
enum GameState {
    #[default]
    Paused,
    Running,
    Start,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(FromArgs)]
/// Options
struct Args {
    /// disable frame pacing
    #[argh(switch)]
    disable_pacing: bool,
}

fn main() {
    let mut app = App::new();

    #[cfg(not(target_arch = "wasm32"))]
    let args: Args = argh::from_env();

    #[cfg(not(target_arch = "wasm32"))]
    if !args.disable_pacing {
        app.insert_resource(FramepaceSettings {
            limiter: Limiter::Auto,
        });
    }

    #[cfg(target_arch = "wasm32")]
    app.insert_resource(FramepaceSettings {
        limiter: Limiter::Auto,
    });

    app.init_resource::<GameSpeed>()
        .init_resource::<Score>()
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
                        present_mode: PresentMode::AutoNoVsync,
                        fit_canvas_to_parent: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .init_state::<GameState>()
        .add_plugins((
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            #[cfg(feature = "subsecond")]
            bevy_simple_subsecond_system::prelude::SimpleSubsecondPlugin::default(),
            Material2dPlugin::<GameMaterial>::default(),
            Material2dPlugin::<RippleMaterial>::default(),
            bevy_framepace::FramepacePlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(OnEnter(GameState::Start), spawn_blobs_init_game)
        .add_systems(
            Update,
            (
                unpaused,
                handle_mouse_move,
                click_blobs,
                shrink_grow_blobs,
                set_blob_state,
                move_blobs,
                splash_blobs,
                ripple_swap,
                update_score,
            )
                .chain()
                .run_if(in_state(GameState::Running)),
        )
        .add_systems(Update, (update_game_text, render_blobs, mute))
        .add_systems(Update, main_menu_paused.run_if(in_state(GameState::Paused)))
        .run();
}

const SPLASH_START_SIZE: f32 = 0.03;

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

#[derive(Clone, Copy, Component, Deref, DerefMut)]
pub struct BlobGrowing(f32);

#[derive(Clone, Component)]
pub struct SplashBlob {
    age: f32,
    spawned_by: Vec<Entity>,
}

#[derive(Resource, Clone, Copy, Default)]
pub struct Score {
    pub raw: f32,
    pub hits: u64,
    pub misses: u64,
}

#[derive(Resource, Clone, Copy, Deref, DerefMut)]
pub struct GameSpeed(pub f32);

impl Default for GameSpeed {
    fn default() -> Self {
        Self(0.8)
    }
}

fn spawn_blobs_init_game(
    mut commands: Commands,
    existing_blobs: Query<Entity, With<BlobSizeRadius>>,
    mut score: ResMut<Score>,
    mut next_state: ResMut<NextState<GameState>>,
    mut game_speed: ResMut<GameSpeed>,
) {
    for entity in existing_blobs {
        commands.entity(entity).despawn();
    }

    *score = Score::default();
    *game_speed = GameSpeed::default();

    for i in 0..28 {
        let vel_rng = vec2(hash_noise_signed(0, i, 1), hash_noise_signed(0, i, 2));

        commands.spawn((
            BlobSizeRadius(0.18 + hash_noise(i, 0, 0) * 0.3),
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
            BlobGrowing(0.0),
        ));
    }
    next_state.set(GameState::Running);
}

fn shrink_grow_blobs(
    mut blobs: Query<(&mut BlobSizeRadius, &mut BlobGrowing), Without<SplashBlob>>,
    time: Res<Time>,
) {
    let shink_speed = 0.02;
    let grow_speed = 0.95;
    for (i, (mut blob_size, mut blob_growing)) in blobs.iter_mut().enumerate() {
        let ui = i as u32;
        if **blob_growing > 0.0 {
            **blob_size += time.delta_secs()
                * grow_speed
                * (hash_noise(ui, ui, ui) * 0.5 + 0.5).clamp(1.0, 1.0);
            **blob_growing *= (0.00075 / time.delta_secs()).min(0.99);
        } else {
            **blob_size -= time.delta_secs() * shink_speed;
        }

        //**blob_size = blob_size.max(0.0);
    }
}

fn set_blob_state(
    mut commands: Commands,
    blobs: Query<(Entity, &BlobSizeRadius, &BlobGrowing), Without<SplashBlob>>,
    clickable_size: Res<BlobClickableSize>,
) {
    for (entity, blob_size, growing) in blobs {
        if **blob_size < **clickable_size && **growing == 0.0 {
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
        Has<SplashBlob>,
    )>,
    window: Single<&Window>,
    time: Res<Time>,
    mut game_speed: ResMut<GameSpeed>,
    mut ripple_materials: ResMut<Assets<RippleMaterial>>,
) {
    let (_, ripple_material) = ripple_materials.iter_mut().next().unwrap();
    **game_speed += (time.delta_secs() * 0.05) / **game_speed;
    let window_size = window.resolution.physical_size().as_vec2();
    let window_ratio = window_size.x / window_size.y;
    let mut hit_pos_rad = None;
    for (size, mut pos, mut vel, _color, splash_blob) in blobs {
        **pos += **vel * time.delta_secs() * **game_speed;
        let size = **size;

        if !splash_blob {
            // bounce off walls
            if pos.x - size < -window_ratio {
                vel.x = -vel.x;
                hit_pos_rad = Some(vec3(pos.x + size, pos.y, size));
            }
            if pos.y - size < -1.0 {
                vel.y = -vel.y;
                hit_pos_rad = Some(vec3(pos.x, pos.y - size, size));
            }
            if pos.x + size > window_ratio {
                vel.x = -vel.x;
                hit_pos_rad = Some(vec3(pos.x + size, pos.y, size));
            }
            if pos.y + size > 1.0 {
                vel.y = -vel.y;
                hit_pos_rad = Some(vec3(pos.x, pos.y + size, size));
            }
        }
    }
    if let Some(hit_pos) = hit_pos_rad {
        ripple_material.blob_pos_hit = hit_pos.extend(0.0);
    } else {
        ripple_material.blob_pos_hit = Vec4::ZERO;
    }
}

fn splash_blobs(
    mut commands: Commands,
    mut blobs: Query<
        (
            Entity,
            &mut BlobSizeRadius,
            &mut BlobPosition,
            &mut BlobVelocity,
            &BlobColor,
            &mut BlobGrowing,
        ),
        Without<SplashBlob>,
    >,
    mut splash_blobs: Query<(
        Entity,
        &mut BlobSizeRadius,
        &mut BlobPosition,
        &mut BlobVelocity,
        &BlobColor,
        &mut SplashBlob,
    )>,
    time: Res<Time>,
    mut game_speed: ResMut<GameSpeed>,
    frame: Res<FrameCount>,
) {
    **game_speed += (time.delta_secs() * 0.05) / **game_speed;
    for (splash_entity, mut splash_size, splash_pos, _splash_vel, _splash_color, mut splash_blob) in
        splash_blobs.iter_mut()
    {
        splash_blob.age -= time.delta_secs() * 0.1;
        **splash_size = splash_blob.age * SPLASH_START_SIZE;
        if splash_blob.age <= 0.0 {
            commands.entity(splash_entity).despawn();
            continue;
        }
        for (i, (entity, mut size, pos, _vel, color, _growing)) in blobs.iter_mut().enumerate() {
            if splash_blob.spawned_by.contains(&entity) {
                continue;
            }
            if splash_pos.distance(**pos) < **size {
                //**growing = growing.max(splash_blob.age * 0.00001);
                **size += **splash_size * 0.4 + SPLASH_START_SIZE * 0.1; // TODO use area, smooth anim
                commands.entity(splash_entity).despawn();
                if splash_blob.spawned_by.len() < 4 {
                    let mut new_spawned_by = splash_blob.spawned_by.clone();
                    new_spawned_by.push(entity);
                    spawn_splash(
                        &mut commands,
                        &frame,
                        new_spawned_by,
                        &pos,
                        color,
                        i as u32,
                        1,
                    );
                }
                break;
            }
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
    mut commands: Commands,
    mut button_events: EventReader<MouseButtonInput>,
    mouse_position: Res<MousePosition>,
    mut blobs: Query<
        (
            Entity,
            &mut BlobSizeRadius,
            &mut BlobPosition,
            &BlobColor,
            Has<BlobCanBeClicked>,
            &mut BlobGrowing,
        ),
        Without<SplashBlob>,
    >,
    mut score: ResMut<Score>,
    game_speed: Res<GameSpeed>,
    frame: Res<FrameCount>,
    asset_server: Res<AssetServer>,
) {
    let mut clicked = false;
    for button_event in button_events.read() {
        if button_event.button == MouseButton::Left && button_event.state == ButtonState::Pressed {
            clicked = true;
        }
    }

    if clicked {
        let mut hit = false;
        for (i, (entity, size, pos, color, can_be_clicked, mut blob_growing)) in
            blobs.iter_mut().enumerate()
        {
            let i = i as u32;
            if can_be_clicked && pos.distance(mouse_position.window_rel) < **size {
                //**size += 0.3;
                score.raw += 5.0 * (**game_speed);
                score.hits += 1;
                hit = true;
                **blob_growing = 1.0;
                spawn_splash(&mut commands, &frame, vec![entity], &pos, color, i, 3);
            }
        }
        if hit {
            commands.spawn((
                AudioPlayer::new(asset_server.load("hit.flac")),
                GameAudio,
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Decibels(-19.0),
                    speed: 1.0,
                    paused: false,
                    muted: false,
                    spatial: false,
                    spatial_scale: None,
                },
            ));
        } else {
            commands.spawn((
                AudioPlayer::new(asset_server.load("missed.flac")),
                GameAudio,
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Decibels(-24.0),
                    speed: 1.0,
                    paused: false,
                    muted: false,
                    spatial: false,
                    spatial_scale: None,
                },
            ));
            score.misses += 1;
        }
    }
}

fn spawn_splash(
    commands: &mut Commands,
    frame: &FrameCount,
    spawned_by: Vec<Entity>,
    pos: &BlobPosition,
    color: &BlobColor,
    i: u32,
    count: u32,
) {
    for j in 0..count {
        let vel_rng = vec2(
            hash_noise_signed(i, frame.0, j + 1),
            hash_noise_signed(i, frame.0, j + 2),
        );
        commands.spawn((
            BlobSizeRadius(SPLASH_START_SIZE),
            *pos,
            BlobVelocity(0.3 * vel_rng.signum() + vel_rng * 0.3),
            BlobColor(**color * 0.9),
            SplashBlob {
                age: 1.0,
                spawned_by: spawned_by.clone(),
            },
        ));
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

    if temp_pos_radius.is_empty() || temp_color.is_empty() {
        temp_pos_radius.push(Default::default());
        temp_color.push(Default::default());
    }

    game_material.pos_radius_tex = images.add(data_image(&temp_pos_radius));
    game_material.color_tex = images.add(data_image(&temp_color));
    game_material.data.circle_count = temp_pos_radius.len() as u32;
}

fn update_score(
    mut score: ResMut<Score>,
    blobs: Query<&BlobSizeRadius, Without<SplashBlob>>,
    time: Res<Time>,
) {
    let mut alive_count = 0;
    for blob_size in blobs {
        if **blob_size > 0.0 {
            alive_count += 1;
        }
    }

    score.raw += time.delta_secs() * alive_count as f32 * 0.5;
}

fn update_game_text(
    mut text: Single<&mut Text, With<GameText>>,
    blobs: Query<&BlobSizeRadius, Without<SplashBlob>>,
    score: Res<Score>,
    //game_speed: Res<GameSpeed>,
) {
    let mut alive_count = 0;
    for blob_size in blobs {
        if **blob_size > 0.0 {
            alive_count += 1;
        }
    }

    let comp_score =
        score.raw * 0.2 + score.raw * ((score.hits + 20) as f32 / (score.misses + 20) as f32) * 0.8;

    text.clear();
    text.push_str(&format!("Alive  {alive_count}\n"));
    text.push_str(&format!("Score  {comp_score:0.1}\n"));
    //text.push_str(&format!("  Hit  {}\n", score.hits));
    //text.push_str(&format!(" Miss  {}\n", score.misses));
    //text.push_str(&format!("Speed  {:0.1}\n", **game_speed));
}

fn main_menu_paused(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut text: Single<&mut Text, With<CenteredText>>,
    score: Res<Score>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyP)
        || keyboard_input.just_pressed(KeyCode::Escape)
        || keyboard_input.just_pressed(KeyCode::Tab)
    {
        next_state.set(GameState::Running);
    }

    if keyboard_input.just_pressed(KeyCode::Space) {
        next_state.set(GameState::Start);
    }

    text.clear();
    text.push_str("PRESS SPACE TO START A NEW GAME\n\n");
    if score.raw > 0.0 {
        text.push_str("PRESS P OR TAB TO RESUME\n");
    }
}

fn unpaused(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut text: Single<&mut Text, With<CenteredText>>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyP)
        || keyboard_input.just_pressed(KeyCode::Escape)
        || keyboard_input.just_pressed(KeyCode::Tab)
    {
        next_state.set(GameState::Paused);
    }
    text.clear();
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
        AudioPlayer::new(asset_server.load("blorb_theme.flac")),
        GameAudio,
        PlaybackSettings {
            mode: PlaybackMode::Loop,
            volume: Volume::Decibels(-3.0),
            speed: 1.0,
            paused: false,
            muted: false,
            spatial: false,
            spatial_scale: None,
        },
    ));

    commands.spawn((
        Msaa::Off,
        Camera2d,
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
            blob_pos_hit: Vec4::ZERO,
            prev_tex: ripple_images.b.clone(),
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
            top: Val::Vh(2.0),
            left: Val::Vh(2.0),
            ..default()
        },
        GameText,
    ));

    commands
        .spawn((Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            position_type: PositionType::Absolute,
            ..default()
        },))
        .with_children(|parent| {
            parent.spawn((
                Text::default(),
                TextLayout::new_with_justify(JustifyText::Center),
                Node::default(),
                CenteredText,
            ));
        });

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
    score: Res<Score>,
) {
    let mut clicked = false;
    for button_event in button_events.read() {
        if button_event.button == MouseButton::Left && button_event.state == ButtonState::Pressed {
            clicked = true;
        }
    }

    let mut init = score.raw < 10.0;
    ripple_images.swap();
    let res = window.resolution.physical_size().as_vec2();
    if ripple_images.res != res {
        *ripple_images = RippleImages::new(res, &mut images);
        init = true;
    }
    camera.target = ripple_images.a.clone().into();
    let (_, ripple_material) = ripple_materials.iter_mut().next().unwrap();
    ripple_material.mouse_pos_dt = vec4(
        mouse_position.ndc.x,
        mouse_position.ndc.y,
        if clicked {
            1.0
        } else if init {
            -1.0
        } else {
            0.0
        },
        time.delta_secs(),
    );
    ripple_material.prev_tex = ripple_images.b.clone();

    let (_, game_material) = game_materials.iter_mut().next().unwrap();
    game_material.ripple_tex = ripple_images.a.clone();
}

fn mute(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut audio_controller: Query<&mut AudioSink, With<GameAudio>>,
) {
    let Ok(mut sink) = audio_controller.single_mut() else {
        return;
    };

    if keyboard_input.just_pressed(KeyCode::KeyM) {
        sink.toggle_mute();
    }
}

#[derive(Component)]
struct GameAudio;

#[derive(Component)]
struct GameText;

#[derive(Component)]
struct CenteredText;

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
    #[uniform(0)]
    blob_pos_hit: Vec4,
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
