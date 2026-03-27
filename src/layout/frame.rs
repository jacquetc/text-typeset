use crate::font::registry::FontRegistry;
use crate::layout::block::layout_block;
use crate::layout::block::{BlockLayout, BlockLayoutParams};
use crate::layout::table::{TableLayout, TableLayoutParams, layout_table};

/// Frame position type (from text-document's FramePosition).
///
/// **Note on floats**: `FloatLeft` and `FloatRight` currently place the frame
/// at the correct x position but do not implement content wrapping (text does
/// not flow beside the float). They advance `content_height` like `Inline`,
/// so subsequent content appears below rather than beside the float.
/// True float exclusion zones would require tracking available width per
/// y-range during paragraph layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FramePosition {
    /// Inline: rendered in normal flow order, advances content_height.
    #[default]
    Inline,
    /// Float left: placed at x=0, advances content_height.
    /// Content wrapping is not yet implemented.
    FloatLeft,
    /// Float right: placed at x=available_width - frame_width, advances content_height.
    /// Content wrapping is not yet implemented.
    FloatRight,
    /// Absolute: placed at (margin_left, margin_top) from document origin.
    /// Does not affect flow or content_height.
    Absolute,
}

/// How the frame border is drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FrameBorderStyle {
    /// Draw border on all four sides (default).
    #[default]
    Full,
    /// Draw only the left border (blockquote style).
    LeftOnly,
    /// No border.
    None,
}

/// Parameters for a frame, extracted from text-document's FrameSnapshot.
pub struct FrameLayoutParams {
    pub frame_id: usize,
    pub position: FramePosition,
    /// Frame width constraint (None = use available width).
    pub width: Option<f32>,
    /// Frame height constraint (None = auto from content).
    pub height: Option<f32>,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub padding: f32,
    pub border_width: f32,
    pub border_style: FrameBorderStyle,
    /// Nested flow elements: blocks and tables within the frame.
    pub blocks: Vec<BlockLayoutParams>,
    pub tables: Vec<(usize, TableLayoutParams)>, // (flow_index, params) for ordering
}

/// Computed layout for a frame.
pub struct FrameLayout {
    pub frame_id: usize,
    pub y: f32,
    pub x: f32,
    pub total_width: f32,
    pub total_height: f32,
    pub content_x: f32,
    pub content_y: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub blocks: Vec<BlockLayout>,
    pub tables: Vec<TableLayout>,
    pub border_width: f32,
    pub border_style: FrameBorderStyle,
}

/// Lay out a frame: compute dimensions, lay out nested content.
pub fn layout_frame(
    registry: &FontRegistry,
    params: &FrameLayoutParams,
    available_width: f32,
) -> FrameLayout {
    let border = params.border_width;
    let pad = params.padding;
    let frame_width = params.width.unwrap_or(available_width);
    let content_width =
        (frame_width - border * 2.0 - pad * 2.0 - params.margin_left - params.margin_right)
            .max(0.0);

    // Lay out nested blocks
    let mut blocks = Vec::new();
    let mut content_y = 0.0f32;

    for block_params in &params.blocks {
        let mut block = layout_block(registry, block_params, content_width);
        block.y = content_y + block.top_margin;
        let block_content = block.height - block.top_margin - block.bottom_margin;
        content_y = block.y + block_content + block.bottom_margin;
        blocks.push(block);
    }

    // Lay out nested tables
    let mut tables = Vec::new();
    for (_flow_idx, table_params) in &params.tables {
        let mut table = layout_table(registry, table_params, content_width);
        table.y = content_y;
        content_y += table.total_height;
        tables.push(table);
    }

    let auto_content_height = content_y;
    let content_height = params
        .height
        .map(|h| (h - border * 2.0 - pad * 2.0).max(0.0))
        .unwrap_or(auto_content_height);

    let total_height =
        params.margin_top + border + pad + content_height + pad + border + params.margin_bottom;
    let total_width =
        params.margin_left + border + pad + content_width + pad + border + params.margin_right;

    let content_x = params.margin_left + border + pad;
    let content_y_offset = params.margin_top + border + pad;

    FrameLayout {
        frame_id: params.frame_id,
        y: 0.0, // set by flow
        x: 0.0,
        total_width,
        total_height,
        content_x,
        content_y: content_y_offset,
        content_width,
        content_height,
        blocks,
        tables,
        border_width: border,
        border_style: params.border_style,
    }
}
