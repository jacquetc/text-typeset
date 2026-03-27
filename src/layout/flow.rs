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
    pub cached_max_content_width: f32,
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
            cached_max_content_width: 0.0,
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
        if table.total_width > self.cached_max_content_width {
            self.cached_max_content_width = table.total_width;
        }
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
        if frame.total_width > self.cached_max_content_width {
            self.cached_max_content_width = frame.total_width;
        }
        self.frames.insert(frame.frame_id, frame);
    }

    /// Clear all layout state. Call before rebuilding from a new FlowSnapshot.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.tables.clear();
        self.frames.clear();
        self.flow_order.clear();
        self.content_height = 0.0;
        self.cached_max_content_width = 0.0;
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
        self.update_max_width_for_block(&block);
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

    /// Update a single block's layout and shift subsequent items if height changed.
    ///
    /// Finds the block in top-level blocks, table cells, or frames, re-layouts
    /// it, and propagates any height delta to subsequent flow items.
    pub fn relayout_block(
        &mut self,
        registry: &FontRegistry,
        params: &BlockLayoutParams,
        available_width: f32,
    ) {
        let block_id = params.block_id;

        // Top-level block
        if self.blocks.contains_key(&block_id) {
            self.relayout_top_level_block(registry, params, available_width);
            return;
        }

        // Table cell block: scan tables for the block_id
        let table_match = self.tables.iter().find_map(|(&tid, table)| {
            for cell in &table.cell_layouts {
                if cell.blocks.iter().any(|b| b.block_id == block_id) {
                    return Some((tid, cell.row, cell.column));
                }
            }
            None
        });
        if let Some((table_id, row, col)) = table_match {
            self.relayout_table_block(registry, params, table_id, row, col);
            return;
        }

        // Frame block: scan frames (including nested frames) for the block_id
        let frame_match = self.frames.iter().find_map(|(&fid, frame)| {
            if frame_contains_block(frame, block_id) {
                return Some(fid);
            }
            None
        });
        if let Some(frame_id) = frame_match {
            self.relayout_frame_block(registry, params, frame_id);
        }
    }

    /// Relayout a top-level block (existing logic).
    fn relayout_top_level_block(
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
        self.update_max_width_for_block(&block);
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

        self.shift_items_after_block(block_id, delta);
    }

    /// Relayout a block inside a table cell. Recomputes the row height
    /// and propagates any table height delta to subsequent flow items.
    fn relayout_table_block(
        &mut self,
        registry: &FontRegistry,
        params: &BlockLayoutParams,
        table_id: usize,
        row: usize,
        col: usize,
    ) {
        let table = match self.tables.get_mut(&table_id) {
            Some(t) => t,
            None => return,
        };

        let cell_width = table
            .column_content_widths
            .get(col)
            .copied()
            .unwrap_or(200.0);
        let old_table_height = table.total_height;

        // Find the cell and replace the block
        let cell = match table
            .cell_layouts
            .iter_mut()
            .find(|c| c.row == row && c.column == col)
        {
            Some(c) => c,
            None => return,
        };

        let new_block = layout_block(registry, params, cell_width);
        if let Some(old) = cell
            .blocks
            .iter_mut()
            .find(|b| b.block_id == params.block_id)
        {
            *old = new_block;
        }

        // Reposition blocks within the cell and recompute cell height
        let mut block_y = 0.0f32;
        for block in &mut cell.blocks {
            block.y = block_y;
            block_y += block.height;
        }
        let cell_height = block_y;

        // Recompute row height by scanning all cells in this row
        if row < table.row_heights.len() {
            let mut max_h = 0.0f32;
            for c in &table.cell_layouts {
                if c.row == row {
                    let h: f32 = c.blocks.iter().map(|b| b.height).sum();
                    max_h = max_h.max(h);
                }
            }
            // Also consider the cell we just updated
            max_h = max_h.max(cell_height);
            table.row_heights[row] = max_h;
        }

        // Recompute row y positions and total height
        let border = table.border_width;
        let padding = table.cell_padding;
        let spacing = if table.row_ys.len() > 1 {
            // Infer spacing from existing layout
            if table.row_ys.len() >= 2 && !table.row_heights.is_empty() {
                let expected = table.row_ys[0] + padding + table.row_heights[0] + padding;
                (table.row_ys.get(1).copied().unwrap_or(expected) - expected + padding).max(0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };
        let mut y = border;
        for (r, &row_h) in table.row_heights.iter().enumerate() {
            if r < table.row_ys.len() {
                table.row_ys[r] = y + padding;
            }
            y += padding * 2.0 + row_h;
            if r < table.row_heights.len() - 1 {
                y += spacing;
            }
        }
        table.total_height = y + border;

        let delta = table.total_height - old_table_height;

        // Update flow_order entry for this table
        for item in &mut self.flow_order {
            if let FlowItem::Table {
                table_id: id,
                height,
                ..
            } = item
                && *id == table_id
            {
                *height = table.total_height;
                break;
            }
        }

        self.shift_items_after_table(table_id, delta);
    }

    /// Relayout a block inside a frame. Recomputes frame content height
    /// and propagates any height delta to subsequent flow items.
    fn relayout_frame_block(
        &mut self,
        registry: &FontRegistry,
        params: &BlockLayoutParams,
        frame_id: usize,
    ) {
        let frame = match self.frames.get_mut(&frame_id) {
            Some(f) => f,
            None => return,
        };

        let old_total_height = frame.total_height;
        let new_block = layout_block(registry, params, frame.content_width);

        relayout_block_in_frame(frame, params.block_id, new_block);

        let delta = frame.total_height - old_total_height;

        for item in &mut self.flow_order {
            if let FlowItem::Frame {
                frame_id: id,
                height,
                ..
            } = item
                && *id == frame_id
            {
                *height = frame.total_height;
                break;
            }
        }

        self.shift_items_after_frame(frame_id, delta);
    }

    /// Shift all flow items after the given block by `delta` pixels.
    fn shift_items_after_block(&mut self, block_id: usize, delta: f32) {
        if delta.abs() <= 0.001 {
            return;
        }
        let mut found = false;
        for item in &mut self.flow_order {
            match item {
                FlowItem::Block {
                    block_id: id, y, ..
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

    /// Shift all flow items after the given table by `delta` pixels.
    fn shift_items_after_table(&mut self, table_id: usize, delta: f32) {
        if delta.abs() <= 0.001 {
            return;
        }
        let mut found = false;
        for item in &mut self.flow_order {
            match item {
                FlowItem::Table {
                    table_id: id, y, ..
                } => {
                    if *id == table_id {
                        found = true;
                        continue;
                    }
                    if found {
                        *y += delta;
                        if let Some(t) = self.tables.get_mut(id) {
                            t.y += delta;
                        }
                    }
                }
                FlowItem::Block {
                    block_id: id, y, ..
                } => {
                    if found {
                        *y += delta;
                        if let Some(b) = self.blocks.get_mut(id) {
                            b.y += delta;
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

    /// Shift all flow items after the given frame by `delta` pixels.
    fn shift_items_after_frame(&mut self, frame_id: usize, delta: f32) {
        if delta.abs() <= 0.001 {
            return;
        }
        let mut found = false;
        for item in &mut self.flow_order {
            match item {
                FlowItem::Frame {
                    frame_id: id, y, ..
                } => {
                    if *id == frame_id {
                        found = true;
                        continue;
                    }
                    if found {
                        *y += delta;
                        if let Some(f) = self.frames.get_mut(id) {
                            f.y += delta;
                        }
                    }
                }
                FlowItem::Block {
                    block_id: id, y, ..
                } => {
                    if found {
                        *y += delta;
                        if let Some(b) = self.blocks.get_mut(id) {
                            b.y += delta;
                        }
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
            }
        }
        self.content_height += delta;
    }

    /// Update the cached max content width considering a single block's lines.
    fn update_max_width_for_block(&mut self, block: &BlockLayout) {
        for line in &block.lines {
            let w = line.width + block.left_margin + block.right_margin;
            if w > self.cached_max_content_width {
                self.cached_max_content_width = w;
            }
        }
    }

    /// Find the bottom margin of the block immediately before `block_id` in flow order.
    fn prev_block_bottom_margin(&self, block_id: usize) -> Option<f32> {
        let mut prev_bm = None;
        for item in &self.flow_order {
            match item {
                FlowItem::Block { block_id: id, .. } => {
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

/// Check whether a frame (or any of its nested frames) contains a block with the given id.
pub(crate) fn frame_contains_block(frame: &FrameLayout, block_id: usize) -> bool {
    if frame.blocks.iter().any(|b| b.block_id == block_id) {
        return true;
    }
    frame
        .frames
        .iter()
        .any(|nested| frame_contains_block(nested, block_id))
}

/// Replace a block inside a frame (searching nested frames recursively)
/// and recompute content/total heights up the tree.
fn relayout_block_in_frame(frame: &mut FrameLayout, block_id: usize, new_block: BlockLayout) {
    let old_content_height = frame.content_height;

    // Try direct blocks first
    if let Some(old) = frame.blocks.iter_mut().find(|b| b.block_id == block_id) {
        *old = new_block;
    } else {
        // Recurse into nested frames
        for nested in &mut frame.frames {
            if frame_contains_block(nested, block_id) {
                relayout_block_in_frame(nested, block_id, new_block);
                break;
            }
        }
    }

    // Reposition all direct content (blocks, tables, nested frames) vertically
    let mut content_y = 0.0f32;
    for block in &mut frame.blocks {
        block.y = content_y + block.top_margin;
        let block_content = block.height - block.top_margin - block.bottom_margin;
        content_y = block.y + block_content + block.bottom_margin;
    }
    for table in &mut frame.tables {
        table.y = content_y;
        content_y += table.total_height;
    }
    for nested in &mut frame.frames {
        nested.y = content_y;
        content_y += nested.total_height;
    }

    frame.content_height = content_y;
    frame.total_height += content_y - old_content_height;
}
