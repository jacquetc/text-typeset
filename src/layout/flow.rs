use std::collections::HashMap;

use crate::font::registry::FontRegistry;
use crate::layout::block::{BlockLayout, BlockLayoutParams, layout_block};
use crate::layout::frame::{FrameLayout, FrameLayoutParams, layout_frame};
use crate::layout::table::{TableLayout, TableLayoutParams, layout_table};

pub enum FlowItem {
    Block {
        block_id: usize,
        y: f32,
        height: f32,
    },
    Table {
        table_id: usize,
        y: f32,
        height: f32,
    },
    Frame {
        frame_id: usize,
        y: f32,
        height: f32,
    },
}

pub struct FlowLayout {
    pub blocks: HashMap<usize, BlockLayout>,
    pub tables: HashMap<usize, TableLayout>,
    pub frames: HashMap<usize, FrameLayout>,
    pub flow_order: Vec<FlowItem>,
    pub content_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl Default for FlowLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowLayout {
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            tables: HashMap::new(),
            frames: HashMap::new(),
            flow_order: Vec::new(),
            content_height: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
        }
    }

    /// Add a table to the flow at the current y position.
    pub fn add_table(
        &mut self,
        registry: &FontRegistry,
        params: &TableLayoutParams,
        available_width: f32,
    ) {
        let mut table = layout_table(registry, params, available_width);

        let mut y = self.content_height;
        table.y = y;
        y += table.total_height;

        self.flow_order.push(FlowItem::Table {
            table_id: table.table_id,
            y: table.y,
            height: table.total_height,
        });
        self.tables.insert(table.table_id, table);
        self.content_height = y;
    }

    /// Add a frame to the flow.
    ///
    /// - **Inline**: placed in normal flow, advances content_height.
    /// - **FloatLeft**: placed at current y, x=0. Does not advance content_height
    ///   (surrounding content wraps around it).
    /// - **FloatRight**: placed at current y, x=available_width - frame_width.
    /// - **Absolute**: placed at (margin_left, margin_top) from document origin.
    ///   Does not affect flow at all.
    pub fn add_frame(
        &mut self,
        registry: &FontRegistry,
        params: &FrameLayoutParams,
        available_width: f32,
    ) {
        use crate::layout::frame::FramePosition;

        let mut frame = layout_frame(registry, params, available_width);

        match params.position {
            FramePosition::Inline => {
                frame.y = self.content_height;
                frame.x = 0.0;
                self.content_height += frame.total_height;
            }
            FramePosition::FloatLeft => {
                frame.y = self.content_height;
                frame.x = 0.0;
                // Float doesn't advance content_height -content wraps beside it.
                // For simplicity, we still advance so subsequent blocks appear below.
                // True float wrapping would require a "float exclusion zone" tracked
                // during paragraph layout, which is significantly more complex.
                self.content_height += frame.total_height;
            }
            FramePosition::FloatRight => {
                frame.y = self.content_height;
                frame.x = (available_width - frame.total_width).max(0.0);
                self.content_height += frame.total_height;
            }
            FramePosition::Absolute => {
                // Absolute frames are positioned relative to the document origin
                // using their margin values as coordinates. They don't affect flow.
                frame.y = params.margin_top;
                frame.x = params.margin_left;
                // Don't advance content_height
            }
        }

        self.flow_order.push(FlowItem::Frame {
            frame_id: frame.frame_id,
            y: frame.y,
            height: frame.total_height,
        });
        self.frames.insert(frame.frame_id, frame);
    }

    /// Clear all layout state. Call before rebuilding from a new FlowSnapshot.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.tables.clear();
        self.frames.clear();
        self.flow_order.clear();
        self.content_height = 0.0;
    }

    /// Add a single block to the flow at the current y position.
    pub fn add_block(
        &mut self,
        registry: &FontRegistry,
        params: &BlockLayoutParams,
        available_width: f32,
    ) {
        let mut block = layout_block(registry, params, available_width);

        // Margin collapsing with previous block
        let mut y = self.content_height;
        if let Some(FlowItem::Block {
            block_id: prev_id, ..
        }) = self.flow_order.last()
        {
            if let Some(prev_block) = self.blocks.get(prev_id) {
                let collapsed = prev_block.bottom_margin.max(block.top_margin);
                y -= prev_block.bottom_margin;
                y += collapsed;
            } else {
                y += block.top_margin;
            }
        } else {
            y += block.top_margin;
        }

        block.y = y;
        let block_content = block.height - block.top_margin - block.bottom_margin;
        y += block_content + block.bottom_margin;

        self.flow_order.push(FlowItem::Block {
            block_id: block.block_id,
            y: block.y,
            height: block.height,
        });
        self.blocks.insert(block.block_id, block);
        self.content_height = y;
    }

    /// Lay out a sequence of blocks vertically.
    pub fn layout_blocks(
        &mut self,
        registry: &FontRegistry,
        block_params: Vec<BlockLayoutParams>,
        available_width: f32,
    ) {
        self.clear();
        // Note: viewport_width is NOT set here. It's a display property
        // set by Typesetter::set_viewport(), not a layout property.
        // available_width is the layout width which may differ from viewport
        // when using ContentWidthMode::Fixed.
        for params in &block_params {
            self.add_block(registry, params, available_width);
        }
    }

    /// Update a single block's layout and shift subsequent blocks if the
    /// position or height changed.
    ///
    /// If the block's top margin changed, its y position is recomputed using
    /// margin collapsing with the previous block. Subsequent items are shifted
    /// by the resulting delta.
    pub fn relayout_block(
        &mut self,
        registry: &FontRegistry,
        params: &BlockLayoutParams,
        available_width: f32,
    ) {
        let block_id = params.block_id;
        let old_y = self.blocks.get(&block_id).map(|b| b.y).unwrap_or(0.0);
        let old_height = self.blocks.get(&block_id).map(|b| b.height).unwrap_or(0.0);
        let old_top_margin = self
            .blocks
            .get(&block_id)
            .map(|b| b.top_margin)
            .unwrap_or(0.0);
        let old_bottom_margin = self
            .blocks
            .get(&block_id)
            .map(|b| b.bottom_margin)
            .unwrap_or(0.0);
        let old_content = old_height - old_top_margin - old_bottom_margin;
        let old_end = old_y + old_content + old_bottom_margin;

        let mut block = layout_block(registry, params, available_width);
        block.y = old_y;

        // If top margin changed, recompute this block's y position
        if (block.top_margin - old_top_margin).abs() > 0.001 {
            let prev_bm = self.prev_block_bottom_margin(block_id).unwrap_or(0.0);
            let old_collapsed = prev_bm.max(old_top_margin);
            let new_collapsed = prev_bm.max(block.top_margin);
            block.y = old_y + (new_collapsed - old_collapsed);
        }

        let new_content = block.height - block.top_margin - block.bottom_margin;
        let new_end = block.y + new_content + block.bottom_margin;
        let delta = new_end - old_end;

        let new_y = block.y;
        let new_height = block.height;
        self.blocks.insert(block_id, block);

        // Update flow_order entry
        for item in &mut self.flow_order {
            if let FlowItem::Block {
                block_id: id,
                y,
                height,
            } = item
                && *id == block_id
            {
                *y = new_y;
                *height = new_height;
                break;
            }
        }

        // Shift subsequent items if position or height changed
        if delta.abs() > 0.001 {
            let mut found = false;
            for item in &mut self.flow_order {
                match item {
                    FlowItem::Block {
                        block_id: id,
                        y,
                        height: _,
                    } => {
                        if found {
                            *y += delta;
                            if let Some(b) = self.blocks.get_mut(id) {
                                b.y += delta;
                            }
                        }
                        if *id == block_id {
                            found = true;
                        }
                    }
                    FlowItem::Table {
                        table_id: id, y, ..
                    } => {
                        if found {
                            *y += delta;
                            if let Some(t) = self.tables.get_mut(id) {
                                t.y += delta;
                            }
                        }
                    }
                    FlowItem::Frame {
                        frame_id: id, y, ..
                    } => {
                        if found {
                            *y += delta;
                            if let Some(f) = self.frames.get_mut(id) {
                                f.y += delta;
                            }
                        }
                    }
                }
            }
            self.content_height += delta;
        }
    }

    /// Find the bottom margin of the block immediately before `block_id` in flow order.
    fn prev_block_bottom_margin(&self, block_id: usize) -> Option<f32> {
        let mut prev_bm = None;
        for item in &self.flow_order {
            match item {
                FlowItem::Block {
                    block_id: id, ..
                } => {
                    if *id == block_id {
                        return prev_bm;
                    }
                    if let Some(b) = self.blocks.get(id) {
                        prev_bm = Some(b.bottom_margin);
                    }
                }
                _ => {
                    // Non-block items reset margin collapsing
                    prev_bm = None;
                }
            }
        }
        None
    }
}
