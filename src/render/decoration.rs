use crate::font::registry::FontRegistry;
use crate::font::resolve::ResolvedFont;
use crate::layout::block::BlockLayout;
use crate::shaping::shaper::{FontMetricsPx, font_metrics_px};
use crate::types::{DecorationKind, DecorationRect, UnderlineStyle};

/// Generate decoration rects (underline, strikeout, overline) for a block.
/// `x_offset` and `y_offset` are added when the block is inside a table cell or frame.
#[allow(clippy::too_many_arguments)]
pub fn generate_block_decorations(
    block: &BlockLayout,
    registry: &FontRegistry,
    scroll_offset: f32,
    viewport_height: f32,
    x_offset: f32,
    y_offset: f32,
    available_width: f32,
    default_text_color: [f32; 4],
    scale_factor: f32,
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

            // Text-level background highlight
            if let Some(bg) = decos.background_color {
                let run_x = x_offset + block.left_margin + positioned_run.x;
                let run_width = run.advance_width;
                let bg_y = screen_baseline - line.ascent;
                decorations.push(DecorationRect {
                    rect: [run_x, bg_y, run_width, line.line_height],
                    color: bg,
                    kind: DecorationKind::TextBackground,
                });
            }

            let has_underline = decos.underline_style != UnderlineStyle::None;
            if !has_underline && !decos.overline && !decos.strikeout {
                continue;
            }

            // Get font metrics for decoration positioning
            let metrics = get_run_metrics(registry, run.font_face_id, run.size_px, scale_factor);

            let run_x = x_offset + block.left_margin + positioned_run.x;
            let run_width = run.advance_width;
            let stroke = metrics.stroke_size.max(1.0);
            let base_color = decos.foreground_color.unwrap_or(default_text_color);

            if has_underline {
                let underline_color = decos.underline_color.unwrap_or(base_color);
                // underline_offset is typically negative (below baseline)
                let y = screen_baseline - metrics.underline_offset;
                generate_underline_rects(
                    run_x,
                    y,
                    run_width,
                    stroke,
                    decos.underline_style,
                    underline_color,
                    &mut decorations,
                );
            }

            if decos.strikeout {
                // strikeout_offset is typically positive (above baseline)
                let y = screen_baseline - metrics.strikeout_offset;
                decorations.push(DecorationRect {
                    rect: [run_x, y, run_width, stroke],
                    color: base_color,
                    kind: DecorationKind::Strikeout,
                });
            }

            if decos.overline {
                // Overline at the ascent line
                let y = screen_baseline - metrics.ascent;
                decorations.push(DecorationRect {
                    rect: [run_x, y, run_width, stroke],
                    color: base_color,
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
    scale_factor: f32,
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
        scale_factor,
        weight: 400,
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

/// Generate underline decoration rects for various underline styles.
fn generate_underline_rects(
    x: f32,
    y: f32,
    width: f32,
    stroke: f32,
    style: UnderlineStyle,
    color: [f32; 4],
    out: &mut Vec<DecorationRect>,
) {
    match style {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => {
            out.push(DecorationRect {
                rect: [x, y, width, stroke],
                color,
                kind: DecorationKind::Underline,
            });
        }
        UnderlineStyle::Dash => {
            let dash = (stroke * 4.0).max(4.0);
            let gap = (stroke * 2.0).max(2.0);
            let mut cx = x;
            while cx < x + width {
                let seg = dash.min(x + width - cx);
                out.push(DecorationRect {
                    rect: [cx, y, seg, stroke],
                    color,
                    kind: DecorationKind::Underline,
                });
                cx += dash + gap;
            }
        }
        UnderlineStyle::Dot => {
            let dot = stroke.max(1.0);
            let gap = dot;
            let mut cx = x;
            while cx < x + width {
                let seg = dot.min(x + width - cx);
                out.push(DecorationRect {
                    rect: [cx, y, seg, stroke],
                    color,
                    kind: DecorationKind::Underline,
                });
                cx += dot + gap;
            }
        }
        UnderlineStyle::DashDot => {
            let dash = (stroke * 4.0).max(4.0);
            let dot = stroke.max(1.0);
            let gap = (stroke * 1.5).max(1.5);
            let mut cx = x;
            let mut is_dash = true;
            while cx < x + width {
                let seg_len = if is_dash { dash } else { dot };
                let seg = seg_len.min(x + width - cx);
                out.push(DecorationRect {
                    rect: [cx, y, seg, stroke],
                    color,
                    kind: DecorationKind::Underline,
                });
                cx += seg_len + gap;
                is_dash = !is_dash;
            }
        }
        UnderlineStyle::DashDotDot => {
            let dash = (stroke * 4.0).max(4.0);
            let dot = stroke.max(1.0);
            let gap = (stroke * 1.5).max(1.5);
            let mut cx = x;
            // Pattern: dash, dot, dot, dash, dot, dot, ...
            let mut phase = 0u8; // 0=dash, 1=dot, 2=dot
            while cx < x + width {
                let seg_len = if phase == 0 { dash } else { dot };
                let seg = seg_len.min(x + width - cx);
                out.push(DecorationRect {
                    rect: [cx, y, seg, stroke],
                    color,
                    kind: DecorationKind::Underline,
                });
                cx += seg_len + gap;
                phase = (phase + 1) % 3;
            }
        }
        UnderlineStyle::Wave | UnderlineStyle::SpellCheck => {
            // Approximate wave with a zigzag of small rects at alternating y offsets.
            let step = (stroke * 2.0).max(2.0);
            let amplitude = stroke;
            let mut cx = x;
            let mut up = true;
            while cx < x + width {
                let seg = step.min(x + width - cx);
                let wy = if up { y - amplitude } else { y + amplitude };
                out.push(DecorationRect {
                    rect: [cx, wy, seg, stroke],
                    color,
                    kind: DecorationKind::Underline,
                });
                cx += step;
                up = !up;
            }
        }
    }
}
