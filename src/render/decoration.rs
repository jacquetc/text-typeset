use crate::font::registry::FontRegistry;
use crate::font::resolve::ResolvedFont;
use crate::layout::block::BlockLayout;
use crate::shaping::shaper::{FontMetricsPx, font_metrics_px};
use crate::types::{DecorationKind, DecorationRect};

/// Generate decoration rects (underline, strikeout, overline) for a block.
/// `x_offset` and `y_offset` are added when the block is inside a table cell or frame.
pub fn generate_block_decorations(
    block: &BlockLayout,
    registry: &FontRegistry,
    scroll_offset: f32,
    viewport_height: f32,
    x_offset: f32,
    y_offset: f32,
    available_width: f32,
) -> Vec<DecorationRect> {
    let mut decorations = Vec::new();

    // Block background color
    if let Some(bg_color) = block.background_color {
        let block_top = y_offset + block.y - scroll_offset;
        let block_height = block.height - block.top_margin - block.bottom_margin;
        decorations.push(DecorationRect {
            rect: [x_offset, block_top, available_width, block_height],
            color: bg_color,
            kind: DecorationKind::BlockBackground,
        });
    }

    for line in &block.lines {
        let line_y = y_offset + block.y + line.y; // baseline in document space
        let screen_top = line_y - line.ascent - scroll_offset;
        // Line-level viewport culling
        if screen_top + line.line_height < 0.0 {
            continue;
        }
        if screen_top > viewport_height {
            break;
        }
        let screen_baseline = line_y - scroll_offset;

        for positioned_run in &line.runs {
            let run = &positioned_run.shaped_run;
            let decos = &positioned_run.decorations;

            if !decos.underline && !decos.overline && !decos.strikeout {
                continue;
            }

            // Get font metrics for decoration positioning
            let metrics = get_run_metrics(registry, run.font_face_id, run.size_px);

            let run_x = x_offset + block.left_margin + positioned_run.x;
            let run_width = run.advance_width;
            let stroke = metrics.stroke_size.max(1.0);
            let color = [0.0, 0.0, 0.0, 1.0]; // default decoration color: black

            if decos.underline {
                // underline_offset is typically negative (below baseline)
                let y = screen_baseline - metrics.underline_offset;
                decorations.push(DecorationRect {
                    rect: [run_x, y, run_width, stroke],
                    color,
                    kind: DecorationKind::Underline,
                });
            }

            if decos.strikeout {
                // strikeout_offset is typically positive (above baseline)
                let y = screen_baseline - metrics.strikeout_offset;
                decorations.push(DecorationRect {
                    rect: [run_x, y, run_width, stroke],
                    color,
                    kind: DecorationKind::Strikeout,
                });
            }

            if decos.overline {
                // Overline at the ascent line
                let y = screen_baseline - metrics.ascent;
                decorations.push(DecorationRect {
                    rect: [run_x, y, run_width, stroke],
                    color,
                    kind: DecorationKind::Overline,
                });
            }
        }
    }

    decorations
}

fn get_run_metrics(
    registry: &FontRegistry,
    font_face_id: crate::types::FontFaceId,
    size_px: f32,
) -> FontMetricsPx {
    let entry = match registry.get(font_face_id) {
        Some(e) => e,
        None => return fallback_metrics(),
    };
    let resolved = ResolvedFont {
        font_face_id,
        size_px,
        face_index: entry.face_index,
        swash_cache_key: entry.swash_cache_key,
    };
    font_metrics_px(registry, &resolved).unwrap_or_else(fallback_metrics)
}

fn fallback_metrics() -> FontMetricsPx {
    FontMetricsPx {
        ascent: 14.0,
        descent: 4.0,
        leading: 0.0,
        underline_offset: -2.0,
        strikeout_offset: 5.0,
        stroke_size: 1.0,
    }
}
