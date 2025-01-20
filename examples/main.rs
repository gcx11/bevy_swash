use bevy::app::{App, Startup, Update};
use bevy::asset::{AssetServer, Assets};
use bevy::color::palettes::css::*;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::math::Quat;
use bevy::prelude::{Camera, Camera2d, Circle, ClearColor, Color, ColorMaterial, Commands, Component, Mesh, Mesh2d, Query, Res, ResMut, Single, Transform, Window, With, Without};
use bevy::sprite::{Anchor, MeshMaterial2d};
use bevy::time::Time;
use bevy::utils::default;
use bevy::DefaultPlugins;
use bevy_swash::{
    JustifyOutlinedText, OutlineStyle, OutlinedFont, OutlinedFontStyle, OutlinedText,
    OutlinedText2dBundle, OutlinedTextPlugin, OutlinedTextSection,
};
use std::f32::consts::PI;

#[derive(Component)]
struct FpsCounter;

#[derive(Component)]
struct Spinner;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d::default());
    commands
        .spawn(OutlinedText2dBundle {
            text: OutlinedText {
                sections: vec![
                    OutlinedTextSection {
                        value: "Outline".to_string(),
                        color: ORANGE.into(),
                        outline: OutlineStyle::Outline {
                            width: 10.0,
                            color: RED.into(),
                        },
                    },
                    OutlinedTextSection {
                        value: "!".to_string(),
                        color: AQUA.into(),
                        outline: OutlineStyle::Outline {
                            width: 10.0,
                            color: BLUE.into(),
                        },
                    },
                ],
                justify: JustifyOutlinedText::Left,
                font_style: OutlinedFontStyle {
                    font: asset_server.load::<OutlinedFont>("fonts/Montserrat-Bold.ttf"),
                    size: 160.0,
                },
            },
            text_anchor: Anchor::Center,
            transform: Transform::from_xyz(0.0, 0.0, 5.0),
            ..default()
        })
        .insert(Spinner);

    commands.spawn(OutlinedText2dBundle {
        text: OutlinedText {
            sections: vec![OutlinedTextSection {
                value: "Bevy, bevy, bevy...\nAnother line".to_string(),
                color: Color::WHITE,
                outline: OutlineStyle::None,
            }],
            justify: JustifyOutlinedText::Center,
            font_style: OutlinedFontStyle {
                font: asset_server.load::<OutlinedFont>("fonts/Montserrat-Regular.ttf"),
                size: 20.0,
            },
        },
        text_anchor: Anchor::BottomLeft,
        transform: Transform::from_xyz(-100.0, -100.0, 7.0),
        ..default()
    });

    commands.spawn((
        OutlinedText2dBundle {
            text: OutlinedText {
                sections: vec![
                    OutlinedTextSection {
                        value: "FPS: ".to_string(),
                        color: Color::BLACK,
                        outline: OutlineStyle::Outline {
                            width: 5.0,
                            color: Color::WHITE,
                        },
                    },
                    OutlinedTextSection {
                        value: "".to_string(),
                        color: Color::BLACK,
                        outline: OutlineStyle::Outline {
                            width: 5.0,
                            color: RED.into(),
                        },
                    },
                ],
                justify: JustifyOutlinedText::Left,
                font_style: OutlinedFontStyle {
                    font: asset_server.load::<OutlinedFont>("fonts/Montserrat-Italic.ttf"),
                    size: 40.0,
                },
            },
            text_anchor: Anchor::TopLeft,
            transform: Transform::from_xyz(-300.0, 300.0, 5.0),
            ..default()
        },
        FpsCounter,
    ));

    commands.spawn((
        Mesh2d(meshes.add(Circle { radius: 5.0 })),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(YELLOW))),
        Transform::from_xyz(0.0, 0.0, 7.0),
    ));

    commands.spawn((
        Mesh2d(meshes.add(Circle { radius: 2.0 })),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(AQUA))),
        Transform::from_xyz(-100.0, -100.0, 7.0),
    ));
}

fn update_fps_text(
    window: Single<&Window>,
    camera: Single<&Transform, With<Camera>>,
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<(&mut Transform, &mut OutlinedText), (With<FpsCounter>, Without<Camera>)>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or_default() as i32;

    for (mut transform, mut text) in query.iter_mut() {
        transform.translation.x = camera.translation.x - (window.width() / 2.0);
        transform.translation.y = camera.translation.y + (window.height() / 2.0);

        text.sections[1].value = fps.to_string();
    }
}

fn spin(time: Res<Time>, mut query: Query<&mut Transform, With<Spinner>>) {
    for mut transform in &mut query {
        transform.rotation = Quat::from_rotation_z(-time.elapsed_secs() * PI / 2.0);
    }
}

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(OutlinedTextPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, setup)
        .add_systems(Update, spin)
        .add_systems(Update, update_fps_text);

    app.run();
}
