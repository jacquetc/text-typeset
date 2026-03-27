use crate::layout::flow::{FlowItem, FlowLayout};
use crate::types::{HitRegion, HitTestResult};

/// Map a screen-space point to a document position.
///
/// The coordinates are relative to the widget's top-left corner,
/// with scroll already accounted for (the caller passes screen coords,
/// and this function adds `scroll_offset` to convert to document space).
pub fn hit_test(flow: &FlowLayout, scroll_offset: f32, x: f32, y: f32) -> Option<HitTestResult> {
    let doc_y = y + scroll_offset;

    // Find which block contains this y position
    let (block_id, block) = find_block_at_y(flow, doc_y)?;

    let content_top = block.y;

    // Check if x is in the left margin
    if x < block.left_margin {
        // Return start of block
        return Some(HitTestResult {
            position: block.position,
            block_id,
            offset_in_block: 0,
            region: HitRegion::LeftMargin,
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
                });
            }
            return Some(HitTestResult {
                position: block.position,
                block_id,
                offset_in_block: 0,
                region: HitRegion::BelowContent,
            });
        }
    };

    // Find which glyph within the line
    let local_x = x - block.left_margin;
    let (offset_in_block, region) = find_position_in_line(line, local_x);

    Some(HitTestResult {
        position: block.position + offset_in_block,
        block_id,
        offset_in_block,
        region,
    })
}

/// Get the screen-space caret rectangle at a document position.
pub fn caret_rect(flow: &FlowLayout, scroll_offset: f32, position: usize) -> [f32; 4] {
    // Find which block contains this position
    for item in &flow.flow_order {
        let bid = match item {
            FlowItem::Block { block_id, .. } => *block_id,
            _ => continue,
        };
        let block = match flow.blocks.get(&bid) {
            Some(b) => b,
            None => continue,
        };

        let block_end = block.position + block.lines.last().map(|l| l.char_range.end).unwrap_or(0);

        if position < block.position || position > block_end {
            continue;
        }

        let offset_in_block = position - block.position;

        // Find the line containing this offset
        for line in &block.lines {
            if offset_in_block < line.char_range.start {
                continue;
            }
            if offset_in_block > line.char_range.end {
                continue;
            }

            // Find x position within the line
            let caret_x = find_x_for_offset(line, offset_in_block) + block.left_margin;
            let caret_y = block.y + line.y - line.ascent - scroll_offset;
            let caret_height = line.line_height;

            return [caret_x, caret_y, 2.0, caret_height];
        }
    }

    // Fallback: top-left
    [0.0, -scroll_offset, 2.0, 16.0]
}

// ── Internal helpers ────────────────────────────────────────────

use crate::layout::block::BlockLayout;
use crate::layout::line::LayoutLine;

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

fn find_position_in_line(line: &LayoutLine, local_x: f32) -> (usize, HitRegion) {
    for run in &line.runs {
        let mut glyph_x = run.x;

        for glyph in &run.shaped_run.glyphs {
            let glyph_mid = glyph_x + glyph.x_advance / 2.0;

            if local_x < glyph_mid {
                // Click is on the left half of this glyph - position before it
                let offset = glyph.cluster as usize;
                if run.decorations.is_link {
                    // Report as link region. The adapter can look up the actual
                    // href from the document position.
                    return (
                        offset,
                        HitRegion::Link {
                            href: String::new(),
                        },
                    );
                }
                return (offset, HitRegion::Text);
            }

            glyph_x += glyph.x_advance;
        }
    }

    // Past end of line
    (line.char_range.end, HitRegion::PastLineEnd)
}

fn find_x_for_offset(line: &LayoutLine, offset: usize) -> f32 {
    let runs = &line.runs;
    for (i, run) in runs.iter().enumerate() {
        let mut glyph_x = run.x;
        for glyph in &run.shaped_run.glyphs {
            if glyph.cluster as usize >= offset {
                return glyph_x;
            }
            glyph_x += glyph.x_advance;
        }
        // Only return from this run if the offset doesn't belong to a later run
        let next_run_start = runs
            .get(i + 1)
            .and_then(|r| r.shaped_run.glyphs.first())
            .map(|g| g.cluster as usize);
        match next_run_start {
            Some(next_start) if offset >= next_start => continue,
            _ => return glyph_x,
        }
    }
    // Fallback: end of line
    line.runs
        .last()
        .map(|r| r.x + r.shaped_run.advance_width)
        .unwrap_or(0.0)
}
