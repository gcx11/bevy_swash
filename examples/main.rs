use bevy::app::{App, Startup, Update};
use bevy::asset::{AssetServer, Assets};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::math::Quat;
use bevy::prelude::{
    Camera, Camera2dBundle, Circle, ClearColor, Color, ColorMaterial, Commands, Component, Mesh,
    Query, Res, ResMut, Transform, Window, With, Without,
};
use bevy::sprite::{Anchor, MaterialMesh2dBundle, Mesh2dHandle};
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
    commands.spawn(Camera2dBundle::default());
    commands
        .spawn(OutlinedText2dBundle {
            text: OutlinedText {
                sections: vec![
                    OutlinedTextSection {
                        value: "Outline".to_string(),
                        color: Color::ORANGE,
                        outline: OutlineStyle::Outline {
                            width: 10.0,
                            color: Color::RED,
                        },
                    },
                    OutlinedTextSection {
                        value: "!".to_string(),
                        color: Color::CYAN,
                        outline: OutlineStyle::Outline {
                            width: 10.0,
                            color: Color::BLUE,
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
                            color: Color::RED,
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

    commands.spawn(MaterialMesh2dBundle {
        mesh: Mesh2dHandle(meshes.add(Circle { radius: 5.0 })),
        material: materials.add(Color::YELLOW),
        transform: Transform::from_xyz(0.0, 0.0, 7.0),
        ..default()
    });

    commands.spawn(MaterialMesh2dBundle {
        mesh: Mesh2dHandle(meshes.add(Circle { radius: 2.0 })),
        material: materials.add(Color::CYAN),
        transform: Transform::from_xyz(-100.0, -100.0, 7.0),
        ..default()
    });
}

fn update_fps_text(
    window_query: Query<&Window>,
    camera_query: Query<&Transform, With<Camera>>,
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<(&mut Transform, &mut OutlinedText), (With<FpsCounter>, Without<Camera>)>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or_default() as i32;

    let window = window_query.get_single().unwrap();
    let camera = camera_query.get_single().unwrap();

    for (mut transform, mut text) in query.iter_mut() {
        transform.translation.x = camera.translation.x - (window.width() / 2.0);
        transform.translation.y = camera.translation.y + (window.height() / 2.0);

        text.sections[1].value = fps.to_string();
    }
}

fn spin(time: Res<Time>, mut query: Query<&mut Transform, With<Spinner>>) {
    for mut transform in &mut query {
        transform.rotation = Quat::from_rotation_z(-time.elapsed_seconds() * PI / 2.0);
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
