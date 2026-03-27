use crate::layout::block::BlockLayout;
use crate::layout::flow::{FlowItem, FlowLayout};
use crate::render::hit_test::caret_rect;
use crate::types::{CursorDisplay, DecorationKind, DecorationRect};

/// Generate cursor and selection decoration rects from the current cursor state.
pub fn generate_cursor_decorations(
    flow: &FlowLayout,
    cursors: &[CursorDisplay],
    scroll_offset: f32,
    cursor_color: [f32; 4],
    selection_color: [f32; 4],
) -> Vec<DecorationRect> {
    let mut decorations = Vec::new();

    for cursor in cursors {
        // Selection highlight (if anchor != position)
        if cursor.anchor != cursor.position {
            let sel_start = cursor.anchor.min(cursor.position);
            let sel_end = cursor.anchor.max(cursor.position);
            let sel_rects =
                compute_selection_rects(flow, scroll_offset, sel_start, sel_end, selection_color);
            decorations.extend(sel_rects);
        }

        // Cursor caret (if visible)
        if cursor.visible {
            let rect = caret_rect(flow, scroll_offset, cursor.position);
            decorations.push(DecorationRect {
                rect,
                color: cursor_color,
                kind: DecorationKind::Cursor,
            });
        }
    }

    decorations
}

/// Compute selection highlight rectangles spanning from `start` to `end` document positions.
/// May produce multiple rects if the selection spans multiple lines.
///
/// Covers top-level blocks, blocks inside table cells, and blocks inside frames.
/// When a selection continues past the end of a line (multi-line selection),
/// the highlight extends to the viewport width - matching the behavior of
/// VS Code, Sublime Text, and other modern editors.
fn compute_selection_rects(
    flow: &FlowLayout,
    scroll_offset: f32,
    start: usize,
    end: usize,
    color: [f32; 4],
) -> Vec<DecorationRect> {
    let mut rects = Vec::new();
    let viewport_width = flow.viewport_width;
    let view_top = scroll_offset;
    let view_bottom = scroll_offset + flow.viewport_height;

    for item in &flow.flow_order {
        match item {
            FlowItem::Block {
                block_id,
                y,
                height,
            } => {
                if *y + *height < view_top {
                    continue;
                }
                if *y > view_bottom {
                    break;
                }
                if let Some(block) = flow.blocks.get(block_id) {
                    selection_rects_for_block(
                        block,
                        0.0,
                        0.0,
                        start,
                        end,
                        scroll_offset,
                        viewport_width,
                        color,
                        &mut rects,
                    );
                }
            }
            FlowItem::Table {
                table_id,
                y,
                height,
            } => {
                if *y + *height < view_top {
                    continue;
                }
                if *y > view_bottom {
                    break;
                }
                if let Some(table) = flow.tables.get(table_id) {
                    for cell in &table.cell_layouts {
                        if cell.row >= table.row_ys.len()
                            || cell.column >= table.column_xs.len()
                        {
                            continue;
                        }
                        let cell_x = table.column_xs[cell.column];
                        let cell_y = table.y + table.row_ys[cell.row];
                        for block in &cell.blocks {
                            selection_rects_for_block(
                                block,
                                cell_x,
                                cell_y,
                                start,
                                end,
                                scroll_offset,
                                viewport_width,
                                color,
                                &mut rects,
                            );
                        }
                    }
                }
            }
            FlowItem::Frame {
                frame_id,
                y,
                height,
            } => {
                if *y + *height < view_top {
                    continue;
                }
                if *y > view_bottom {
                    break;
                }
                if let Some(frame) = flow.frames.get(frame_id) {
                    let fx = frame.x + frame.content_x;
                    let fy = frame.y + frame.content_y;
                    for block in &frame.blocks {
                        selection_rects_for_block(
                            block,
                            fx,
                            fy,
                            start,
                            end,
                            scroll_offset,
                            viewport_width,
                            color,
                            &mut rects,
                        );
                    }
                    // Tables nested inside frames
                    for table in &frame.tables {
                        for cell in &table.cell_layouts {
                            if cell.row >= table.row_ys.len()
                                || cell.column >= table.column_xs.len()
                            {
                                continue;
                            }
                            let cell_x = fx + table.column_xs[cell.column];
                            let cell_y = fy + table.y + table.row_ys[cell.row];
                            for block in &cell.blocks {
                                selection_rects_for_block(
                                    block,
                                    cell_x,
                                    cell_y,
                                    start,
                                    end,
                                    scroll_offset,
                                    viewport_width,
                                    color,
                                    &mut rects,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    rects
}

/// Generate selection rects for a single block at the given offset.
#[allow(clippy::too_many_arguments)]
fn selection_rects_for_block(
    block: &BlockLayout,
    offset_x: f32,
    offset_y: f32,
    start: usize,
    end: usize,
    scroll_offset: f32,
    viewport_width: f32,
    color: [f32; 4],
    rects: &mut Vec<DecorationRect>,
) {
    let block_start = block.position;

    for line in &block.lines {
        let line_abs_start = block_start + line.char_range.start;
        let line_abs_end = block_start + line.char_range.end;

        if line_abs_end <= start || line_abs_start >= end {
            continue;
        }

        let sel_line_start = start.max(line_abs_start);
        let sel_line_end = end.min(line_abs_end);

        let offset_start = sel_line_start - block_start;
        let offset_end = sel_line_end - block_start;

        let x_start = offset_x + line.x_for_offset(offset_start) + block.left_margin;
        let x_end_text = offset_x + line.x_for_offset(offset_end) + block.left_margin;

        let line_top = offset_y + block.y + line.y - line.ascent - scroll_offset;
        let line_height = line.line_height;

        let selection_continues_past_line = end > line_abs_end;
        let x_end = if selection_continues_past_line && viewport_width > 0.0 {
            viewport_width
        } else {
            x_end_text
        };

        if x_end > x_start {
            rects.push(DecorationRect {
                rect: [x_start, line_top, x_end - x_start, line_height],
                color,
                kind: DecorationKind::Selection,
            });
        }
    }
}

