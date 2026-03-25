use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use swash::{CacheKey, FontRef};

pub struct GlyphImage {
    pub width: u32,
    pub height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub data: Vec<u8>,
    pub is_color: bool,
}

pub fn rasterize_glyph(
    scale_context: &mut ScaleContext,
    font_data: &[u8],
    face_index: u32,
    cache_key: CacheKey,
    glyph_id: u16,
    size_px: f32,
) -> Option<GlyphImage> {
    let base = FontRef::from_index(font_data, face_index as usize)?;
    let font_ref = FontRef {
        data: base.data,
        offset: base.offset,
        key: cache_key,
    };

    let mut scaler = scale_context
        .builder(font_ref)
        .size(size_px)
        .hint(true)
        .build();

    let image = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ])
    .format(Format::Alpha)
    .render(&mut scaler, glyph_id)?;

    let is_color = matches!(image.content, swash::scale::image::Content::Color);

    Some(GlyphImage {
        width: image.placement.width,
        height: image.placement.height,
        placement_left: image.placement.left,
        placement_top: image.placement.top,
        data: image.data,
        is_color,
    })
}
