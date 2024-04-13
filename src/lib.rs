use bevy::asset::io::Reader;
use bevy::asset::LoadContext;
use bevy::asset::{AssetLoader, AsyncReadExt};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::{Extract, RenderApp};
use bevy::sprite::{Anchor, ExtractedSprite, ExtractedSprites, SpriteSystem};
use bevy::utils::HashMap;
use bevy::window::{PrimaryWindow, WindowScaleFactorChanged};
use bevy_utils::thiserror::Error;
use bevy_utils::BoxedFuture;
use std::mem;
use std::sync::Arc;
use swash::scale::{Render, ScaleContext, Scaler, Source};
use swash::shape::{ShapeContext, Shaper};
use swash::text::cluster::{CharCluster, Parser, Token, Whitespace};
use swash::text::{Codepoint, Script};
use swash::zeno::{Cap, Format, Join, Stroke};
use swash::{CacheKey, Charmap, FontRef, GlyphId};

type SwashImage = swash::scale::image::Image;

#[derive(Asset, TypePath, Debug, Clone)]
pub struct OutlinedFont {
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
pub struct OutlinedFontLoader;

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
pub struct OutlinedText {
    pub sections: Vec<OutlinedTextSection>,
    pub font_style: OutlinedFontStyle,
    pub justify: JustifyOutlinedText,
}

#[derive(Clone, Debug, Default)]
pub struct OutlinedTextSection {
    pub value: String,
    pub color: Color,
    pub outline: OutlineStyle,
}

#[derive(Component, Clone, Debug, Default)]
pub struct OutlinedFontStyle {
    pub font: Handle<OutlinedFont>,
    pub size: f32,
}

#[derive(Debug, Clone, Default)]
pub enum OutlineStyle {
    #[default]
    None,
    Outline {
        width: f32,
        color: Color,
    },
}

#[derive(Clone, Debug, Default)]
pub enum JustifyOutlinedText {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Bundle, Clone, Debug, Default)]
pub struct OutlinedText2dBundle {
    pub text: OutlinedText,
    pub text_anchor: Anchor,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
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
            .flat_map(|alpha| [red, green, blue, *alpha])
            .collect::<Vec<u8>>(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[derive(Resource, Default)]
struct OutlinedGlyphs {
    cache: HashMap<Entity, Vec<ComposedGlyphImage>>,
}

struct GlyphImage {
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    image: Image,
}

#[derive(Default)]
struct OutlinedGlyphLine {
    glyphs: Vec<GlyphImage>,
    width: f32,
}

struct ComposedGlyphImage {
    x: f32,
    y: f32,
    z: f32,
    image: Handle<Image>,
}

fn create_missing_text(
    fonts: Res<Assets<OutlinedFont>>,
    text_query: Query<(Entity, Ref<OutlinedText>, Ref<Anchor>)>,
    mut removed: RemovedComponents<OutlinedText>,
    mut scale_factor_changed: EventReader<WindowScaleFactorChanged>,
    mut images: ResMut<Assets<Image>>,
    mut outlined_glyphs: ResMut<OutlinedGlyphs>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let factor_changed = scale_factor_changed.read().last().is_some();

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
        if !factor_changed
            && !text.is_changed()
            && !anchor.is_changed()
            && outlined_glyphs.cache.contains_key(&entity)
        {
            continue;
        }

        let handle = &text.font_style.font;

        if let Some(outlined_font) = fonts.get(handle) {
            let glyph_images = create_glyph_images(
                &mut shape_context,
                &mut scale_context,
                text,
                anchor,
                outlined_font.as_ref(),
                scale_factor,
            );

            let (glyphs, outlines): (Vec<_>, Vec<_>) = glyph_images
                .into_iter()
                .partition(|glyph| glyph.offset_z == 0.0);
            let mut glyph_images = Vec::new();

            if !glyphs.is_empty() {
                let composed_glyph_image = compose_glyph_images(&mut images, &glyphs);
                glyph_images.push(composed_glyph_image);
            }

            if !outlines.is_empty() {
                let composed_glyph_image = compose_glyph_images(&mut images, &outlines);
                glyph_images.push(composed_glyph_image);
            }

            outlined_glyphs.cache.insert(entity, glyph_images);
        }
    }
}

fn create_glyph_images(
    shape_context: &mut ShapeContext,
    scale_context: &mut ScaleContext,
    text: Ref<OutlinedText>,
    anchor: Ref<Anchor>,
    font_ref: FontRef,
    scale_factor: f32,
) -> Vec<GlyphImage> {
    let sections = &text.sections;
    if sections.is_empty() {
        return Vec::new();
    }

    let mut lines: Vec<OutlinedGlyphLine> = Vec::new();
    let mut current_line = OutlinedGlyphLine::default();

    let size = text.font_style.size / scale_factor;

    let script = Script::Latin;
    let mut shaper = shape_context
        .builder(font_ref)
        .script(script)
        .size(size)
        .build();

    let metrics = shaper.metrics();
    let ascent = metrics.ascent;
    let descent = metrics.descent;
    let leading = metrics.leading;
    let line_height = descent + ascent + leading;

    let mut x = 0.0;
    let mut scaler = scale_context
        .builder(font_ref)
        .size(size)
        .hint(true)
        .build();

    for (index, section) in sections.iter().enumerate() {
        add_section_to_shaper(
            &mut shaper,
            section,
            script,
            font_ref.charmap(),
            index as u32,
        );
    }

    shaper.shape_with(|glyph_cluster| {
        let related_section = &sections[glyph_cluster.data as usize];
        let color = related_section.color;
        let outline = &related_section.outline;

        if glyph_cluster.info.whitespace() == Whitespace::Newline {
            current_line.width = x;
            x = 0.0;
            lines.push(mem::take(&mut current_line));
        }

        for glyph in glyph_cluster.glyphs {
            if let OutlineStyle::Outline {
                width: outline_width,
                color: outline_color,
            } = outline
            {
                let stroke_width = outline_width / scale_factor;

                let outline_bitmap = glyph_outline_to_bitmap(glyph.id, stroke_width, &mut scaler);
                let outline_image = bitmap_to_image(&outline_bitmap, *outline_color);

                if outline_image.width() != 0 && outline_image.height() != 0 {
                    current_line.glyphs.push(GlyphImage {
                        offset_x: x + outline_bitmap.placement.left as f32,
                        offset_y: descent - outline_bitmap.placement.height as f32
                            + outline_bitmap.placement.top as f32,
                        offset_z: -0.001,
                        image: outline_image,
                    });
                }
            }

            let bitmap = glyph_to_bitmap(glyph.id, &mut scaler);
            let image = bitmap_to_image(&bitmap, color);

            if image.width() != 0 && image.height() != 0 {
                current_line.glyphs.push(GlyphImage {
                    offset_x: x + bitmap.placement.left as f32,
                    offset_y: descent - bitmap.placement.height as f32
                        + bitmap.placement.top as f32,
                    offset_z: 0.0,
                    image,
                });
            }

            x += glyph.advance;
        }
    });
    current_line.width = x;
    lines.push(current_line);

    let line_count = lines.len();
    let text_width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
    let text_height = descent + ascent + (lines.len() - 1) as f32 * line_height;

    let anchor_offset = anchor.as_vec();
    let anchor_offset_x = -anchor_offset.x * text_width - text_width / 2.0;
    let anchor_offset_y = -anchor_offset.y * text_height - text_height / 2.0;

    for (i, line) in lines.iter_mut().enumerate() {
        let padding = match text.justify {
            JustifyOutlinedText::Left => 0.0,
            JustifyOutlinedText::Center => (text_width - line.width) / 2.0,
            JustifyOutlinedText::Right => text_width - line.width,
        };

        for glyph in line.glyphs.iter_mut() {
            glyph.offset_x += anchor_offset_x + padding;
            glyph.offset_y += anchor_offset_y + (line_count - i - 1) as f32 * line_height;
        }
    }

    lines.into_iter().flat_map(|line| line.glyphs).collect()
}

fn add_section_to_shaper(
    shaper: &mut Shaper,
    section: &OutlinedTextSection,
    script: Script,
    charmap: Charmap,
    section_index: u32,
) {
    let mut cluster = CharCluster::new();
    let mut parser = Parser::new(
        script,
        section.value.char_indices().map(|(i, ch)| Token {
            ch,
            offset: i as u32,
            len: ch.len_utf8() as u8,
            info: ch.properties().into(),
            data: section_index,
        }),
    );
    while parser.next(&mut cluster) {
        cluster.map(|ch| charmap.map(ch));
        shaper.add_cluster(&cluster);
    }
}

fn compose_glyph_images(
    images: &mut Assets<Image>,
    glyph_images: &[GlyphImage],
) -> ComposedGlyphImage {
    let z_index = glyph_images.first().unwrap().offset_z;

    let mut x_min = f32::INFINITY;
    let mut x_max = f32::NEG_INFINITY;
    let mut y_min = f32::INFINITY;
    let mut y_max = f32::NEG_INFINITY;

    for glyph in glyph_images {
        let x = glyph.offset_x;
        let y = glyph.offset_y;
        let width = glyph.image.width() as f32;
        let height = glyph.image.height() as f32;

        x_min = x_min.min(x);
        x_max = x_max.max(x + width);
        y_min = y_min.min(y);
        y_max = y_max.max(y + height);
    }

    let total_width = (x_max - x_min).ceil() as u32;
    let total_height = (y_max - y_min).ceil() as u32;

    let mut data = vec![0; (total_width * total_height * 4) as usize];

    for glyph in glyph_images {
        let width = glyph.image.width();
        let height = glyph.image.height();

        let dest_x = (glyph.offset_x - x_min).round() as u32;
        let dest_y = total_height - height - (glyph.offset_y - y_min).round() as u32;

        for source_y in 0..height {
            for source_x in 0..width {
                let src_index = (source_y * width + source_x) as usize * 4;
                let dest_index =
                    ((dest_y + source_y) * total_width + dest_x + source_x) as usize * 4;

                let source_data = &glyph.image.data[src_index..src_index + 4];
                if source_data[3] != 0 {
                    data[dest_index..dest_index + 4].copy_from_slice(source_data);
                }
            }
        }
    }

    let image = Image::new(
        Extent3d {
            width: total_width,
            height: total_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );

    let x = x_min;
    let y = y_min;

    ComposedGlyphImage {
        x,
        y,
        z: z_index,
        image: images.add(image),
    }
}

fn extract_outlined_text(
    mut commands: Commands,
    mut extracted_sprites: ResMut<ExtractedSprites>,
    query: Extract<Query<(Entity, &GlobalTransform), With<OutlinedText>>>,
    outlined_glyphs: Extract<Res<OutlinedGlyphs>>,
) {
    for (original_entity, global_transform) in query.iter() {
        if let Some(glyph_images) = outlined_glyphs.cache.get(&original_entity) {
            for glyph_image in glyph_images {
                let entity = commands.spawn_empty().id();

                let transform = GlobalTransform::from_translation(Vec3 {
                    x: glyph_image.x,
                    y: glyph_image.y,
                    z: glyph_image.z,
                });

                extracted_sprites.sprites.insert(
                    entity,
                    ExtractedSprite {
                        transform: *global_transform * transform,
                        color: Color::WHITE,
                        rect: None,
                        custom_size: None,
                        image_handle_id: glyph_image.image.id(),
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

pub struct OutlinedTextPlugin;

impl Plugin for OutlinedTextPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(OutlinedGlyphs::default())
            .init_asset::<OutlinedFont>()
            .init_asset_loader::<OutlinedFontLoader>()
            .add_systems(PostUpdate, create_missing_text);

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                ExtractSchedule,
                extract_outlined_text.after(SpriteSystem::ExtractSprites),
            );
        }
    }
}
