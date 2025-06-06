/*
Windows hotpatch note:
set BEVY_ASSET_ROOT=.
set CARGO_TARGET_DIR=
dx serve --hot-patch
Save file before first run to trigger initial rebuild
*/

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use bevy_simple_subsecond_system::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            // Wasm builds will check for meta files (that don't exist) if this isn't set.
            // This causes errors and even panics in web builds on itch.
            // See https://github.com/bevyengine/bevy_github_ci_template/issues/48.
            meta_check: AssetMetaCheck::Never,
            ..default()
        }))
        .add_plugins(SimpleSubsecondPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, greet)
        .run();
}

#[hot]
fn greet(time: Res<Time>) {
    info_once!(
        "Hello from a hotpatched system! Try changing this string while the app is running! Patched at t = {} s!!",
        time.elapsed_secs()
    );
}
use bevy::color::palettes::css::RED;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(100.0))),
        MeshMaterial2d(materials.add(Color::from(RED))),
        Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
    ));
}
