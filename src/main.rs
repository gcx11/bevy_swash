use bevy::asset::io::Reader;
use bevy::asset::LoadContext;
use bevy::asset::{AssetLoader, AsyncReadExt};
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::{Extract, RenderApp};
use bevy::sprite::{
    Anchor, ExtractedSprite, ExtractedSprites, MaterialMesh2dBundle, Mesh2dHandle, SpriteSystem,
};
use bevy::utils::HashMap;
use bevy::window::PrimaryWindow;
use bevy::DefaultPlugins;
use bevy_utils::thiserror::Error;
use bevy_utils::BoxedFuture;
use std::sync::Arc;
use swash::scale::{Render, ScaleContext, Scaler, Source};
use swash::shape::ShapeContext;
use swash::text::Script;
use swash::zeno::{Cap, Format, Join, Stroke};
use swash::{CacheKey, FontRef, GlyphId};

type SwashImage = swash::scale::image::Image;

#[derive(Asset, TypePath, Debug, Clone)]
struct OutlinedFont {
    data: Arc<Vec<u8>>,
    offset: u32,
    key: CacheKey,
}

impl OutlinedFont {
    fn as_ref(&self) -> FontRef {
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum OutlineFontLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid font")]
    InvalidFont,
}

#[derive(Default)]
struct OutlinedFontLoader;

impl AssetLoader for OutlinedFontLoader {
    type Asset = OutlinedFont;
    type Settings = ();
    type Error = OutlineFontLoaderError;
    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a (),
        _load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<OutlinedFont, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            let font = FontRef::from_index(&bytes, 0);

            if let Some(font_ref) = font {
                let (offset, key) = (font_ref.offset, font_ref.key);

                Ok(OutlinedFont {
                    data: Arc::new(bytes),
                    offset,
                    key,
                })
            } else {
                Err(OutlineFontLoaderError::InvalidFont)
            }
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ttf", "otf"]
    }
}

#[derive(Component, Clone, Debug, Default)]
struct OutlinedText {
    value: String,
    style: OutlinedTextStyle,
}

#[derive(Debug, Clone, Default)]
enum OutlineStyle {
    #[default]
    None,
    Outline {
        size: f32,
        color: Color,
    },
}

#[derive(Component, Clone, Debug, Default)]
struct OutlinedTextStyle {
    font: Handle<OutlinedFont>,
    font_size: f32,
    color: Color,
    outline: OutlineStyle,
}

#[derive(Bundle, Clone, Debug, Default)]
struct OutlinedText2dBundle {
    text: OutlinedText,
    text_anchor: Anchor,
    transform: Transform,
    global_transform: GlobalTransform,
    visibility: Visibility,
    inherited_visibility: InheritedVisibility,
    view_visibility: ViewVisibility,
}

fn glyph_to_bitmap(glyph_id: GlyphId, scaler: &mut Scaler) -> SwashImage {
    Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .render(scaler, glyph_id)
        .unwrap()
}

fn glyph_outline_to_bitmap(
    glyph_id: GlyphId,
    stroke_width: f32,
    scaler: &mut Scaler,
) -> SwashImage {
    Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .style(
            Stroke::new(stroke_width)
                .cap(Cap::Square)
                .join(Join::Round)
                .miter_limit(0.0),
        )
        .render(scaler, glyph_id)
        .unwrap()
}

fn bitmap_to_image(bitmap: &SwashImage, color: Color) -> Image {
    let [red, green, blue, _] = color.as_rgba_u8();

    Image::new(
        Extent3d {
            width: bitmap.placement.width,
            height: bitmap.placement.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bitmap
            .data
            .iter()
            .map(|alpha| vec![red, green, blue, *alpha])
            .flatten()
            .collect::<Vec<u8>>(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[derive(Resource, Default)]
struct OutlinedGlyphs {
    cache: HashMap<Entity, Vec<OutlinedGlyph>>,
}

struct OutlinedGlyph {
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    image: Handle<Image>,
}

fn create_missing_text(
    fonts: Res<Assets<OutlinedFont>>,
    text_query: Query<(Entity, &OutlinedText, &Anchor), Changed<OutlinedText>>,
    mut removed: RemovedComponents<OutlinedText>,
    mut images: ResMut<Assets<Image>>,
    mut outlined_glyphs: ResMut<OutlinedGlyphs>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    for entity in removed.read() {
        outlined_glyphs.cache.remove(&entity);
    }

    let scale_factor = windows
        .get_single()
        .map(|window| window.resolution.scale_factor())
        .unwrap_or(1.0);

    let mut shape_context = ShapeContext::new();
    let mut scale_context = ScaleContext::new();

    for (entity, text, anchor) in text_query.iter() {
        let handle = &text.style.font;

        if let Some(outlined_font) = fonts.get(handle) {
            let mut glyphs: Vec<OutlinedGlyph> = Vec::new();

            let font_ref = outlined_font.as_ref();
            let size = text.style.font_size / scale_factor;

            let mut shaper = shape_context
                .builder(font_ref)
                .script(Script::Latin)
                .size(size)
                .build();

            let metrics = shaper.metrics();
            let ascent = metrics.ascent;
            let descent = metrics.descent;

            let mut x = 0.0;
            let mut scaler = scale_context
                .builder(font_ref)
                .size(size)
                .hint(true)
                .build();

            shaper.add_str(&text.value);
            shaper.shape_with(|glyph_cluster| {
                for glyph in glyph_cluster.glyphs {
                    if let OutlineStyle::Outline {
                        size: outline_size,
                        color: outline_color,
                    } = text.style.outline
                    {
                        let stroke_size = outline_size / scale_factor; // TODO required???

                        let outline_bitmap =
                            glyph_outline_to_bitmap(glyph.id, stroke_size, &mut scaler);
                        let outline_image = bitmap_to_image(&outline_bitmap, outline_color);

                        if outline_image.width() != 0 && outline_image.height() != 0 {
                            let handle = images.add(outline_image.clone());

                            glyphs.push(OutlinedGlyph {
                                offset_x: x + outline_bitmap.placement.left as f32,
                                offset_y: descent - outline_bitmap.placement.height as f32
                                    + outline_bitmap.placement.top as f32,
                                offset_z: -0.001, // TODO
                                image: handle,
                            });
                        }
                    }

                    let bitmap = glyph_to_bitmap(glyph.id, &mut scaler);
                    let image = bitmap_to_image(&bitmap, text.style.color);

                    if image.width() != 0 && image.height() != 0 {
                        let handle = images.add(image.clone());

                        glyphs.push(OutlinedGlyph {
                            offset_x: x + bitmap.placement.left as f32,
                            offset_y: descent - bitmap.placement.height as f32
                                + bitmap.placement.top as f32,
                            offset_z: 0.0,
                            image: handle,
                        });
                    }

                    x += glyph.advance;
                }
            });

            let text_width = x;
            let text_height = descent + ascent;

            let anchor_offset = anchor.as_vec();
            let anchor_offset_x = -anchor_offset.x * text_width - text_width / 2.0;
            let anchor_offset_y = -anchor_offset.y * text_height - text_height / 2.0;

            for glyph in glyphs.iter_mut() {
                glyph.offset_x += anchor_offset_x;
                glyph.offset_y += anchor_offset_y;
            }

            outlined_glyphs.cache.insert(entity, glyphs);
        }
    }
}

fn extract_outlined_text(
    mut commands: Commands,
    mut extracted_sprites: ResMut<ExtractedSprites>,
    query: Extract<Query<(Entity, &GlobalTransform), With<OutlinedText>>>,
    outlined_glyphs: Extract<Res<OutlinedGlyphs>>,
) {
    for (original_entity, global_transform) in query.iter() {
        if let Some(glyphs) = outlined_glyphs.cache.get(&original_entity) {
            for glyph in glyphs {
                let entity = commands.spawn_empty().id();

                let transform = GlobalTransform::from_translation(Vec3 {
                    x: glyph.offset_x,
                    y: glyph.offset_y,
                    z: glyph.offset_z,
                });

                extracted_sprites.sprites.insert(
                    entity,
                    ExtractedSprite {
                        transform: transform * *global_transform,
                        color: Color::WHITE,
                        rect: None,
                        custom_size: None,
                        image_handle_id: glyph.image.id(),
                        flip_x: false,
                        flip_y: false,
                        anchor: Anchor::BottomLeft.as_vec(),
                        original_entity: Some(original_entity),
                    },
                );
            }
        }
    }
}

#[derive(Component)]
struct FpsCounter;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn(OutlinedText2dBundle {
        text: OutlinedText {
            value: "Outline!".to_string(),
            style: OutlinedTextStyle {
                font: asset_server.load::<OutlinedFont>("fonts/Montserrat-Bold.ttf"),
                font_size: 160.0,
                color: Color::ORANGE,
                outline: OutlineStyle::Outline {
                    size: 10.0,
                    color: Color::RED,
                },
            },
        },
        text_anchor: Anchor::Center,
        transform: Transform::from_xyz(0.0, 0.0, 5.0),
        ..default()
    });

    commands.spawn(OutlinedText2dBundle {
        text: OutlinedText {
            value: "Bevy, bevy, bevy...".to_string(),
            style: OutlinedTextStyle {
                font: asset_server.load::<OutlinedFont>("fonts/Montserrat-Regular.ttf"),
                font_size: 20.0,
                color: Color::WHITE,
                outline: OutlineStyle::None,
            },
        },
        text_anchor: Anchor::BottomLeft,
        transform: Transform::from_xyz(-100.0, -100.0, 5.0),
        ..default()
    });

    commands.spawn((
        OutlinedText2dBundle {
            text: OutlinedText {
                value: "FPS".to_string(),
                style: OutlinedTextStyle {
                    font: asset_server.load::<OutlinedFont>("fonts/Montserrat-Italic.ttf"),
                    font_size: 40.0,
                    color: Color::BLACK,
                    outline: OutlineStyle::Outline {
                        size: 5.0,
                        color: Color::WHITE,
                    },
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

        text.value = format!("FPS: {}", fps);
    }
}

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(OutlinedGlyphs::default())
        .init_asset::<OutlinedFont>()
        .init_asset_loader::<OutlinedFontLoader>()
        .add_systems(Startup, setup)
        .add_systems(PostUpdate, create_missing_text)
        .add_systems(Update, update_fps_text);

    if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(
            ExtractSchedule,
            extract_outlined_text.after(SpriteSystem::ExtractSprites),
        );
    }

    app.run();
}
