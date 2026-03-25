use std::ops::Range;

use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::layout::line::{LayoutLine, PositionedRun, RunDecorations};
use crate::shaping::run::ShapedRun;
use crate::shaping::shaper::FontMetricsPx;

/// Text alignment within a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

/// Break shaped runs into lines that fit within `available_width`.
///
/// Strategy: shape-first-then-break.
/// 1. The caller has already shaped the full paragraph into one or more ShapedRuns.
/// 2. We use unicode-linebreak to find break opportunities in the original text.
/// 3. We map break positions to glyph boundaries via cluster values.
/// 4. Greedy line wrapping: accumulate glyph advances, break at the last
///    allowed opportunity before exceeding the width.
/// 5. Apply alignment per line.
pub fn break_into_lines(
    runs: Vec<ShapedRun>,
    text: &str,
    available_width: f32,
    alignment: Alignment,
    first_line_indent: f32,
    metrics: &FontMetricsPx,
) -> Vec<LayoutLine> {
    if runs.is_empty() || text.is_empty() {
        // Empty paragraph: produce one empty line for the block to have height
        return vec![make_empty_line(metrics, 0..0)];
    }

    // Flatten all glyphs into a single sequence with their run association
    let flat = flatten_runs(&runs);
    if flat.is_empty() {
        return vec![make_empty_line(metrics, 0..0)];
    }

    // Get break opportunities from unicode-linebreak (byte offsets in text)
    let breaks: Vec<(usize, BreakOpportunity)> = linebreaks(text).collect();

    // Build a set of allowed break positions (glyph indices)
    let break_points = map_breaks_to_glyph_indices(&flat, &breaks);

    // Greedy line wrapping
    let mut lines = Vec::new();
    let mut line_start_glyph = 0usize;
    let mut line_width = 0.0f32;
    let mut last_break_glyph: Option<usize> = None;
    // First line may be indented; subsequent lines use full width
    let mut effective_width = available_width - first_line_indent;

    for i in 0..flat.len() {
        let glyph_advance = flat[i].x_advance;
        line_width += glyph_advance;

        // Check if this glyph index is a break point
        if break_points.contains(&(i + 1)) {
            last_break_glyph = Some(i + 1);
        }

        // Check for mandatory break
        let is_mandatory = break_points_mandatory(&breaks, &flat, i + 1);

        let exceeds_width = line_width > effective_width && line_start_glyph < i;

        if is_mandatory || exceeds_width {
            let break_at = if is_mandatory {
                i + 1
            } else if let Some(bp) = last_break_glyph {
                if bp > line_start_glyph {
                    bp
                } else {
                    i + 1 // emergency break -no opportunity found
                }
            } else {
                i + 1 // emergency break -no break opportunities at all
            };

            let indent = if lines.is_empty() {
                first_line_indent
            } else {
                0.0
            };
            let line = build_line(&runs, &flat, line_start_glyph, break_at, metrics, indent);
            lines.push(line);

            line_start_glyph = break_at;
            // Subsequent lines use full available width
            effective_width = available_width;
            // Re-accumulate width for glyphs already scanned past the break
            line_width = 0.0;
            for j in break_at..=i {
                if j < flat.len() {
                    line_width += flat[j].x_advance;
                }
            }
            last_break_glyph = None;
        }
    }

    // Remaining glyphs form the last line
    if line_start_glyph < flat.len() {
        let line = build_line(
            &runs,
            &flat,
            line_start_glyph,
            flat.len(),
            metrics,
            if lines.is_empty() {
                first_line_indent
            } else {
                0.0
            },
        );
        lines.push(line);
    }

    // Apply alignment
    let effective_width = available_width;
    let last_idx = lines.len().saturating_sub(1);
    for (i, line) in lines.iter_mut().enumerate() {
        let indent = if i == 0 { first_line_indent } else { 0.0 };
        let line_avail = effective_width - indent;
        match alignment {
            Alignment::Left => {} // runs already at x=0 (plus indent)
            Alignment::Right => {
                let shift = (line_avail - line.width).max(0.0);
                for run in &mut line.runs {
                    run.x += shift;
                }
            }
            Alignment::Center => {
                let shift = ((line_avail - line.width) / 2.0).max(0.0);
                for run in &mut line.runs {
                    run.x += shift;
                }
            }
            Alignment::Justify => {
                // Don't justify the last line
                if i < last_idx && line.width > 0.0 {
                    justify_line(line, line_avail, text);
                }
            }
        }
    }

    if lines.is_empty() {
        lines.push(make_empty_line(metrics, 0..0));
    }

    lines
}

/// A flattened glyph with enough info to map back to runs.
struct FlatGlyph {
    x_advance: f32,
    cluster: u32,
    run_index: usize,
    glyph_index_in_run: usize,
}

fn flatten_runs(runs: &[ShapedRun]) -> Vec<FlatGlyph> {
    let mut flat = Vec::new();
    for (run_idx, run) in runs.iter().enumerate() {
        // Offset cluster values from fragment-text space to block-text space.
        // rustybuzz assigns clusters as byte offsets within the fragment text (0-based),
        // but unicode-linebreak returns byte offsets in the full block text.
        let cluster_offset = run.text_range.start as u32;
        for (glyph_idx, glyph) in run.glyphs.iter().enumerate() {
            flat.push(FlatGlyph {
                x_advance: glyph.x_advance,
                cluster: glyph.cluster + cluster_offset,
                run_index: run_idx,
                glyph_index_in_run: glyph_idx,
            });
        }
    }
    flat
}

/// Map unicode-linebreak byte offsets to glyph indices.
/// A break at byte offset B maps to the glyph whose cluster >= B.
fn map_breaks_to_glyph_indices(
    flat: &[FlatGlyph],
    breaks: &[(usize, BreakOpportunity)],
) -> Vec<usize> {
    let mut result = Vec::new();
    for &(byte_offset, _opportunity) in breaks {
        // Find the first glyph whose cluster starts at or after byte_offset
        if let Some(glyph_idx) = flat.iter().position(|g| g.cluster as usize >= byte_offset) {
            if !result.contains(&glyph_idx) {
                result.push(glyph_idx);
            }
        } else {
            // Break is at or past end of text
            result.push(flat.len());
        }
    }
    result
}

/// Check if glyph index `glyph_idx` corresponds to a mandatory break.
fn break_points_mandatory(
    breaks: &[(usize, BreakOpportunity)],
    flat: &[FlatGlyph],
    glyph_idx: usize,
) -> bool {
    if glyph_idx == 0 || glyph_idx > flat.len() {
        return false;
    }

    // Find the byte offset for this glyph boundary
    let byte_offset = if glyph_idx < flat.len() {
        flat[glyph_idx].cluster as usize
    } else {
        // Past last glyph -check if there's a mandatory break at the end
        return false;
    };

    breaks
        .iter()
        .any(|&(bo, op)| bo == byte_offset && op == BreakOpportunity::Mandatory)
}

/// Build a LayoutLine from a glyph range within the flat sequence.
fn build_line(
    runs: &[ShapedRun],
    flat: &[FlatGlyph],
    start: usize,
    end: usize,
    metrics: &FontMetricsPx,
    indent: f32,
) -> LayoutLine {
    // Group consecutive glyphs by run_index to reconstruct PositionedRuns
    let mut positioned_runs = Vec::new();
    let mut x = indent;
    let mut current_run_idx: Option<usize> = None;
    let mut run_glyph_start = 0usize;

    for i in start..end {
        let fg = &flat[i];
        if current_run_idx != Some(fg.run_index) {
            // Emit previous run segment if any
            if let Some(prev_run_idx) = current_run_idx {
                // End of previous run: use the last glyph we saw from that run
                let prev_end = if i > start {
                    flat[i - 1].glyph_index_in_run + 1
                } else {
                    run_glyph_start
                };
                let sub_run = extract_sub_run(runs, prev_run_idx, run_glyph_start, prev_end);
                if let Some((pr, advance)) = sub_run {
                    positioned_runs.push(PositionedRun {
                        decorations: RunDecorations {
                            underline: pr.underline,
                            overline: pr.overline,
                            strikeout: pr.strikeout,
                            is_link: pr.is_link,
                        },
                        shaped_run: pr,
                        x,
                    });
                    x += advance;
                }
            }
            current_run_idx = Some(fg.run_index);
            run_glyph_start = fg.glyph_index_in_run;
        }
    }

    // Emit final run segment
    if let Some(run_idx) = current_run_idx {
        let end_in_run = if end < flat.len() && flat[end].run_index == run_idx {
            flat[end].glyph_index_in_run
        } else if end > start {
            flat[end - 1].glyph_index_in_run + 1
        } else {
            run_glyph_start
        };
        let sub_run = extract_sub_run(runs, run_idx, run_glyph_start, end_in_run);
        if let Some((pr, advance)) = sub_run {
            positioned_runs.push(PositionedRun {
                decorations: RunDecorations {
                    underline: pr.underline,
                    overline: pr.overline,
                    strikeout: pr.strikeout,
                    is_link: pr.is_link,
                },
                shaped_run: pr,
                x,
            });
            x += advance;
        }
    }

    let width = x - indent;

    // Compute char range from cluster values
    let char_start = if start < flat.len() {
        flat[start].cluster as usize
    } else {
        0
    };
    let char_end = if end > 0 && end <= flat.len() {
        if end < flat.len() {
            flat[end].cluster as usize
        } else {
            // Use end of last glyph's run text range
            flat[end - 1].cluster as usize + 1 // approximate
        }
    } else {
        char_start
    };

    let line_height = metrics.ascent + metrics.descent + metrics.leading;

    LayoutLine {
        runs: positioned_runs,
        y: 0.0, // will be set by the caller (block layout)
        ascent: metrics.ascent,
        descent: metrics.descent,
        leading: metrics.leading,
        width,
        char_range: char_start..char_end,
        line_height,
    }
}

/// Extract a sub-run (slice of glyphs) from a ShapedRun.
/// Cluster values are offset to block-text space (adding text_range.start).
fn extract_sub_run(
    runs: &[ShapedRun],
    run_index: usize,
    glyph_start: usize,
    glyph_end: usize,
) -> Option<(ShapedRun, f32)> {
    let run = &runs[run_index];
    let end = glyph_end.min(run.glyphs.len());
    if glyph_start >= end {
        return None;
    }
    let cluster_offset = run.text_range.start as u32;
    let mut sub_glyphs = run.glyphs[glyph_start..end].to_vec();
    // Offset cluster values from fragment-local to block-text space
    for g in &mut sub_glyphs {
        g.cluster += cluster_offset;
    }
    let advance: f32 = sub_glyphs.iter().map(|g| g.x_advance).sum();

    let sub_run = ShapedRun {
        font_face_id: run.font_face_id,
        size_px: run.size_px,
        glyphs: sub_glyphs,
        advance_width: advance,
        text_range: run.text_range.clone(),
        underline: run.underline,
        overline: run.overline,
        strikeout: run.strikeout,
        is_link: run.is_link,
    };
    Some((sub_run, advance))
}

fn make_empty_line(metrics: &FontMetricsPx, char_range: Range<usize>) -> LayoutLine {
    LayoutLine {
        runs: Vec::new(),
        y: 0.0,
        ascent: metrics.ascent,
        descent: metrics.descent,
        leading: metrics.leading,
        width: 0.0,
        char_range,
        line_height: metrics.ascent + metrics.descent + metrics.leading,
    }
}

/// Distribute extra space among word gaps for justification.
///
/// Finds space glyphs (cluster mapping to ' ') across all runs and
/// increases their x_advance proportionally. Then recomputes run x positions.
fn justify_line(line: &mut LayoutLine, target_width: f32, text: &str) {
    let extra = target_width - line.width;
    if extra <= 0.0 {
        return;
    }

    // Count space glyphs across all runs
    let mut space_count = 0usize;
    for run in &line.runs {
        for glyph in &run.shaped_run.glyphs {
            let byte_offset = glyph.cluster as usize;
            if let Some(ch) = text.get(byte_offset..).and_then(|s| s.chars().next())
                && ch == ' '
            {
                space_count += 1;
            }
        }
    }

    if space_count == 0 {
        return;
    }

    let extra_per_space = extra / space_count as f32;

    // Increase x_advance of space glyphs
    for run in &mut line.runs {
        for glyph in &mut run.shaped_run.glyphs {
            let byte_offset = glyph.cluster as usize;
            if let Some(ch) = text.get(byte_offset..).and_then(|s| s.chars().next())
                && ch == ' '
            {
                glyph.x_advance += extra_per_space;
            }
        }
        // Recompute run advance width
        run.shaped_run.advance_width = run.shaped_run.glyphs.iter().map(|g| g.x_advance).sum();
    }

    // Recompute run x positions (runs follow each other)
    let first_x = line.runs.first().map(|r| r.x).unwrap_or(0.0);
    let mut x = first_x;
    for run in &mut line.runs {
        run.x = x;
        x += run.shaped_run.advance_width;
    }

    line.width = target_width;
}
