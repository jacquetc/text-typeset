use std::ops::Range;

use crate::types::FontFaceId;

#[derive(Clone)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    pub cluster: u32,
    pub x_advance: f32,
    pub y_advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    /// Font face for this specific glyph. May differ from the run's font_face_id
    /// when glyph fallback replaced a .notdef with a glyph from another font.
    pub font_face_id: FontFaceId,
}

#[derive(Clone)]
pub struct ShapedRun {
    pub font_face_id: FontFaceId,
    pub size_px: f32,
    pub glyphs: Vec<ShapedGlyph>,
    pub advance_width: f32,
    pub text_range: Range<usize>,
    /// Decoration flags from the source fragment's TextFormat.
    pub underline: bool,
    pub overline: bool,
    pub strikeout: bool,
    pub is_link: bool,
}
