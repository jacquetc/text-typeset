use crate::layout::flow::{FlowItem, FlowLayout};
use crate::types::{HitRegion, HitTestResult};

/// Map a screen-space point to a document position.
///
/// The coordinates are relative to the widget's top-left corner,
/// with scroll already accounted for (the caller passes screen coords,
/// and this function adds `scroll_offset` to convert to document space).
pub fn hit_test(flow: &FlowLayout, scroll_offset: f32, x: f32, y: f32) -> Option<HitTestResult> {
    let doc_y = y + scroll_offset;

    // Try frames first (clicks inside frame content areas take priority)
    if let Some(result) = hit_test_in_frame(flow, doc_y, x) {
        return Some(result);
    }

    // Fall back to top-level blocks
    let (block_id, block) = find_block_at_y(flow, doc_y)?;
    hit_test_block(block_id, block, doc_y, x, 0.0, 0.0)
}

/// Get the screen-space caret rectangle at a document position.
pub fn caret_rect(flow: &FlowLayout, scroll_offset: f32, position: usize) -> [f32; 4] {
    // Search top-level blocks
    for item in &flow.flow_order {
        let bid = match item {
            FlowItem::Block { block_id, .. } => *block_id,
            _ => continue,
        };
        let block = match flow.blocks.get(&bid) {
            Some(b) => b,
            None => continue,
        };

        if let Some(rect) = caret_rect_in_block(block, position, scroll_offset, 0.0, 0.0) {
            return rect;
        }
    }

    // Search frame blocks
    for frame in flow.frames.values() {
        let offset_x = frame.x + frame.content_x;
        let offset_y = frame.y + frame.content_y;
        for block in &frame.blocks {
            if let Some(rect) = caret_rect_in_block(block, position, scroll_offset, offset_x, offset_y) {
                return rect;
            }
        }
    }

    // Fallback: top-left
    [0.0, -scroll_offset, 2.0, 16.0]
}

// ── Internal helpers ────────────────────────────────────────────

use crate::layout::block::BlockLayout;
use crate::layout::line::LayoutLine;

/// Hit-test a single block, with optional coordinate offsets for frame-nested blocks.
fn hit_test_block(
    block_id: usize,
    block: &BlockLayout,
    doc_y: f32,
    x: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<HitTestResult> {
    let content_top = offset_y + block.y;
    let local_x = x - offset_x;

    // Check if x is in the left margin
    if local_x < block.left_margin {
        return Some(HitTestResult {
            position: block.position,
            block_id,
            offset_in_block: 0,
            region: HitRegion::LeftMargin,
            tooltip: None,
        });
    }

    // Find which line within the block
    let local_y = doc_y - content_top;
    let line = match find_line_at_y(&block.lines, local_y) {
        Some(l) => l,
        None => {
            // Below all lines in the block
            if let Some(last_line) = block.lines.last() {
                return Some(HitTestResult {
                    position: block.position + last_line.char_range.end,
                    block_id,
                    offset_in_block: last_line.char_range.end,
                    region: HitRegion::BelowContent,
                    tooltip: None,
                });
            }
            return Some(HitTestResult {
                position: block.position,
                block_id,
                offset_in_block: 0,
                region: HitRegion::BelowContent,
                tooltip: None,
            });
        }
    };

    // Find which glyph within the line
    let glyph_x = local_x - block.left_margin;
    let (offset_in_block, region, tooltip) = find_position_in_line(line, glyph_x);

    Some(HitTestResult {
        position: block.position + offset_in_block,
        block_id,
        offset_in_block,
        region,
        tooltip,
    })
}

/// Try to hit-test within frame blocks.
fn hit_test_in_frame(flow: &FlowLayout, doc_y: f32, x: f32) -> Option<HitTestResult> {
    for item in &flow.flow_order {
        let frame_id = match item {
            FlowItem::Frame {
                frame_id,
                y,
                height,
            } if doc_y >= *y && doc_y < *y + *height => *frame_id,
            _ => continue,
        };
        let frame = flow.frames.get(&frame_id)?;
        let offset_x = frame.x + frame.content_x;
        let offset_y = frame.y + frame.content_y;
        let local_y = doc_y - offset_y;

        // Find block at local_y within frame
        for block in &frame.blocks {
            let block_bottom = block.y + block.height;
            if local_y >= block.y && local_y < block_bottom {
                return hit_test_block(block.block_id, block, doc_y, x, offset_x, offset_y);
            }
        }

        // Fallback: last block in frame
        if let Some(block) = frame.blocks.last() {
            return hit_test_block(block.block_id, block, doc_y, x, offset_x, offset_y);
        }
    }
    None
}

/// Compute the caret rect for a position within a single block.
/// Returns None if the position is not within this block.
fn caret_rect_in_block(
    block: &BlockLayout,
    position: usize,
    scroll_offset: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<[f32; 4]> {
    let block_end = block.position + block.lines.last().map(|l| l.char_range.end).unwrap_or(0);

    if position < block.position || position > block_end {
        return None;
    }

    let offset_in_block = position - block.position;

    for line in &block.lines {
        if offset_in_block < line.char_range.start {
            continue;
        }
        if offset_in_block > line.char_range.end {
            continue;
        }

        let caret_x = line.x_for_offset(offset_in_block) + block.left_margin + offset_x;
        let caret_y = offset_y + block.y + line.y - line.ascent - scroll_offset;
        let caret_height = line.line_height;

        return Some([caret_x, caret_y, 2.0, caret_height]);
    }

    None
}

fn find_block_at_y(flow: &FlowLayout, doc_y: f32) -> Option<(usize, &BlockLayout)> {
    if flow.flow_order.is_empty() {
        return None;
    }

    // Binary search: find the last item whose y <= doc_y
    let idx = flow.flow_order.partition_point(|item| {
        let y = match item {
            FlowItem::Block { y, .. } | FlowItem::Table { y, .. } | FlowItem::Frame { y, .. } => *y,
        };
        y <= doc_y
    });

    // Check the item at idx-1 (the last one with y <= doc_y) and nearby items
    let start = idx.saturating_sub(1);
    let end = (idx + 1).min(flow.flow_order.len());
    for i in start..end {
        if let FlowItem::Block {
            block_id,
            y,
            height,
        } = &flow.flow_order[i]
            && doc_y >= *y
            && doc_y < *y + *height
            && let Some(block) = flow.blocks.get(block_id)
        {
            return Some((*block_id, block));
        }
    }

    // If below all blocks, return the last one
    for item in flow.flow_order.iter().rev() {
        if let FlowItem::Block { block_id, .. } = item
            && let Some(block) = flow.blocks.get(block_id)
        {
            return Some((*block_id, block));
        }
    }
    None
}

fn find_line_at_y(lines: &[LayoutLine], local_y: f32) -> Option<&LayoutLine> {
    // line.y is the baseline; the line occupies from (y - ascent) to (y - ascent + line_height)
    for line in lines {
        let line_top = line.y - line.ascent;
        let line_bottom = line_top + line.line_height;
        if local_y >= line_top && local_y < line_bottom {
            return Some(line);
        }
    }
    None
}

fn find_position_in_line(line: &LayoutLine, local_x: f32) -> (usize, HitRegion, Option<String>) {
    for run in &line.runs {
        let mut glyph_x = run.x;

        for glyph in &run.shaped_run.glyphs {
            let glyph_mid = glyph_x + glyph.x_advance / 2.0;

            if local_x < glyph_mid {
                let offset = glyph.cluster as usize;
                let tooltip = run.decorations.tooltip.clone();
                if run.decorations.is_link {
                    return (
                        offset,
                        HitRegion::Link {
                            href: run.decorations.anchor_href.clone().unwrap_or_default(),
                        },
                        tooltip,
                    );
                }
                return (offset, HitRegion::Text, tooltip);
            }

            glyph_x += glyph.x_advance;
        }
    }

    // Past end of line
    (line.char_range.end, HitRegion::PastLineEnd, None)
}

