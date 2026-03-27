use rustybuzz::{Direction, Face, UnicodeBuffer};

use crate::font::registry::FontRegistry;
use crate::font::resolve::ResolvedFont;
use crate::shaping::run::{ShapedGlyph, ShapedRun};

/// Text direction for shaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    /// Auto-detect from text content (default).
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

/// Shape a text string with the given resolved font.
///
/// Returns a ShapedRun with glyph IDs and pixel-space positions.
/// The `text_offset` is the byte offset of this text within the block
/// (used for cluster mapping back to document positions).
/// Shape a text string with automatic glyph fallback.
///
/// After shaping with the primary font, any .notdef glyphs (glyph_id==0)
/// are detected and re-shaped with fallback fonts. If no fallback font
/// covers a character, it remains as .notdef (renders as blank space
/// with correct advance).
pub fn shape_text(
    registry: &FontRegistry,
    resolved: &ResolvedFont,
    text: &str,
    text_offset: usize,
) -> Option<ShapedRun> {
    let mut run = shape_text_directed(registry, resolved, text, text_offset, TextDirection::Auto)?;

    // Check for .notdef glyphs and attempt fallback
    if run.glyphs.iter().any(|g| g.glyph_id == 0) && !text.is_empty() {
        apply_glyph_fallback(registry, resolved, text, text_offset, &mut run);
    }

    Some(run)
}

/// Re-shape .notdef glyphs using fallback fonts.
///
/// For each .notdef glyph, finds the source character via the cluster value,
/// queries all registered fonts for coverage, and if one covers it,
/// shapes that single character with the fallback font and replaces
/// the .notdef glyph with the result.
fn apply_glyph_fallback(
    registry: &FontRegistry,
    primary: &ResolvedFont,
    text: &str,
    text_offset: usize,
    run: &mut ShapedRun,
) {
    use crate::font::resolve::find_fallback_font;

    for glyph in &mut run.glyphs {
        if glyph.glyph_id != 0 {
            continue;
        }

        // Find the character that produced this .notdef
        let byte_offset = glyph.cluster as usize;
        let ch = match text.get(byte_offset..).and_then(|s| s.chars().next()) {
            Some(c) => c,
            None => continue,
        };

        // Find a fallback font that has this character
        let fallback_id = match find_fallback_font(registry, ch, primary.font_face_id) {
            Some(id) => id,
            None => continue, // no fallback available -leave as .notdef
        };

        let fallback_entry = match registry.get(fallback_id) {
            Some(e) => e,
            None => continue,
        };

        // Shape just this character with the fallback font
        let fallback_resolved = ResolvedFont {
            font_face_id: fallback_id,
            size_px: primary.size_px,
            face_index: fallback_entry.face_index,
            swash_cache_key: fallback_entry.swash_cache_key,
        };

        let char_str = &text[byte_offset..byte_offset + ch.len_utf8()];
        if let Some(fallback_run) = shape_text_directed(
            registry,
            &fallback_resolved,
            char_str,
            text_offset + byte_offset,
            TextDirection::Auto,
        ) {
            // Replace the .notdef glyph with the fallback glyph(s)
            if let Some(fb_glyph) = fallback_run.glyphs.first() {
                glyph.glyph_id = fb_glyph.glyph_id;
                glyph.x_advance = fb_glyph.x_advance;
                glyph.y_advance = fb_glyph.y_advance;
                glyph.x_offset = fb_glyph.x_offset;
                glyph.y_offset = fb_glyph.y_offset;
                glyph.font_face_id = fallback_id;
            }
        }
    }

    // Recompute total advance
    run.advance_width = run.glyphs.iter().map(|g| g.x_advance).sum();
}

/// Shape text with an explicit direction.
pub fn shape_text_directed(
    registry: &FontRegistry,
    resolved: &ResolvedFont,
    text: &str,
    text_offset: usize,
    direction: TextDirection,
) -> Option<ShapedRun> {
    let entry = registry.get(resolved.font_face_id)?;
    let face = Face::from_slice(&entry.data, entry.face_index)?;

    let units_per_em = face.units_per_em() as f32;
    if units_per_em == 0.0 {
        return None;
    }
    let scale = resolved.size_px / units_per_em;

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    match direction {
        TextDirection::LeftToRight => buffer.set_direction(Direction::LeftToRight),
        TextDirection::RightToLeft => buffer.set_direction(Direction::RightToLeft),
        TextDirection::Auto => {} // let rustybuzz guess
    }

    let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut total_advance = 0.0f32;

    for (info, pos) in infos.iter().zip(positions.iter()) {
        let x_advance = pos.x_advance as f32 * scale;
        let y_advance = pos.y_advance as f32 * scale;
        let x_offset = pos.x_offset as f32 * scale;
        let y_offset = pos.y_offset as f32 * scale;

        glyphs.push(ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            cluster: info.cluster,
            x_advance,
            y_advance,
            x_offset,
            y_offset,
            font_face_id: resolved.font_face_id,
        });

        total_advance += x_advance;
    }

    Some(ShapedRun {
        font_face_id: resolved.font_face_id,
        size_px: resolved.size_px,
        glyphs,
        advance_width: total_advance,
        text_range: text_offset..text_offset + text.len(),
        underline_style: crate::types::UnderlineStyle::None,
        overline: false,
        strikeout: false,
        is_link: false,
        foreground_color: None,
        underline_color: None,
        background_color: None,
        anchor_href: None,
        tooltip: None,
        vertical_alignment: crate::types::VerticalAlignment::Normal,
    })
}

/// Shape a text string, reusing a UnicodeBuffer to avoid allocations.
pub fn shape_text_with_buffer(
    registry: &FontRegistry,
    resolved: &ResolvedFont,
    text: &str,
    text_offset: usize,
    buffer: UnicodeBuffer,
) -> Option<(ShapedRun, UnicodeBuffer)> {
    let entry = registry.get(resolved.font_face_id)?;
    let face = Face::from_slice(&entry.data, entry.face_index)?;

    let units_per_em = face.units_per_em() as f32;
    if units_per_em == 0.0 {
        return None;
    }
    let scale = resolved.size_px / units_per_em;

    let mut buffer = buffer;
    buffer.push_str(text);

    let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut total_advance = 0.0f32;

    for (info, pos) in infos.iter().zip(positions.iter()) {
        let x_advance = pos.x_advance as f32 * scale;
        let y_advance = pos.y_advance as f32 * scale;
        let x_offset = pos.x_offset as f32 * scale;
        let y_offset = pos.y_offset as f32 * scale;

        glyphs.push(ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            cluster: info.cluster,
            x_advance,
            y_advance,
            x_offset,
            y_offset,
            font_face_id: resolved.font_face_id,
        });

        total_advance += x_advance;
    }

    let run = ShapedRun {
        font_face_id: resolved.font_face_id,
        size_px: resolved.size_px,
        glyphs,
        advance_width: total_advance,
        text_range: text_offset..text_offset + text.len(),
        underline_style: crate::types::UnderlineStyle::None,
        overline: false,
        strikeout: false,
        is_link: false,
        foreground_color: None,
        underline_color: None,
        background_color: None,
        anchor_href: None,
        tooltip: None,
        vertical_alignment: crate::types::VerticalAlignment::Normal,
    };

    // Reclaim the buffer for reuse
    let recycled = glyph_buffer.clear();
    Some((run, recycled))
}

/// Get font metrics (ascent, descent, leading) scaled to pixels.
pub fn font_metrics_px(registry: &FontRegistry, resolved: &ResolvedFont) -> Option<FontMetricsPx> {
    let entry = registry.get(resolved.font_face_id)?;
    let font_ref = swash::FontRef::from_index(&entry.data, entry.face_index as usize)?;
    let metrics = font_ref.metrics(&[]).scale(resolved.size_px);

    Some(FontMetricsPx {
        ascent: metrics.ascent,
        descent: metrics.descent,
        leading: metrics.leading,
        underline_offset: metrics.underline_offset,
        strikeout_offset: metrics.strikeout_offset,
        stroke_size: metrics.stroke_size,
    })
}

/// A bidi run: a contiguous range of text with the same direction.
pub struct BidiRun {
    pub byte_range: std::ops::Range<usize>,
    pub direction: TextDirection,
    /// Visual order index (for reordering after line breaking).
    pub visual_order: usize,
}

/// Analyze text for bidirectional content and return directional runs.
/// If the text is purely LTR, returns a single run.
pub fn bidi_runs(text: &str) -> Vec<BidiRun> {
    use unicode_bidi::BidiInfo;

    if text.is_empty() {
        return vec![BidiRun {
            byte_range: 0..0,
            direction: TextDirection::LeftToRight,
            visual_order: 0,
        }];
    }

    let bidi_info = BidiInfo::new(text, None);

    let mut runs = Vec::new();

    for para in &bidi_info.paragraphs {
        let para_text = &text[para.range.clone()];
        let para_offset = para.range.start;

        // Get levels for this paragraph
        let levels = &bidi_info.levels[para.range.clone()];

        // Split into runs of same level
        if levels.is_empty() {
            continue;
        }

        let mut run_start = 0usize;
        let mut current_level = levels[0];

        for (i, &level) in levels.iter().enumerate() {
            if level != current_level {
                // Emit previous run
                let dir = if current_level.is_rtl() {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                };
                // Snap to char boundaries
                let start = snap_to_char_boundary(para_text, run_start);
                let end = snap_to_char_boundary(para_text, i);
                if start < end {
                    runs.push(BidiRun {
                        byte_range: (para_offset + start)..(para_offset + end),
                        direction: dir,
                        visual_order: runs.len(),
                    });
                }
                run_start = i;
                current_level = level;
            }
        }

        // Emit final run
        let dir = if current_level.is_rtl() {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        };
        let start = snap_to_char_boundary(para_text, run_start);
        let end = para_text.len();
        if start < end {
            runs.push(BidiRun {
                byte_range: (para_offset + start)..(para_offset + end),
                direction: dir,
                visual_order: runs.len(),
            });
        }
    }

    if runs.is_empty() {
        runs.push(BidiRun {
            byte_range: 0..text.len(),
            direction: TextDirection::LeftToRight,
            visual_order: 0,
        });
    }

    runs
}

fn snap_to_char_boundary(text: &str, byte_pos: usize) -> usize {
    if byte_pos >= text.len() {
        return text.len();
    }
    // Walk forward to the next char boundary
    let mut pos = byte_pos;
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

pub struct FontMetricsPx {
    pub ascent: f32,
    pub descent: f32,
    pub leading: f32,
    pub underline_offset: f32,
    pub strikeout_offset: f32,
    pub stroke_size: f32,
}
