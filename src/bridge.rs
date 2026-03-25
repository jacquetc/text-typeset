//! Bridge between text-document snapshot types and text-typeset layout params.
//!
//! Converts `FlowSnapshot`, `BlockSnapshot`, `TextFormat`, etc. into
//! `BlockLayoutParams`, `FragmentParams`, `TableLayoutParams`, etc.

use text_document::{
    BlockSnapshot, CellSnapshot, FlowElementSnapshot, FlowSnapshot, FragmentContent, FrameSnapshot,
    TableSnapshot,
};

use crate::layout::block::{BlockLayoutParams, FragmentParams};
use crate::layout::frame::{FrameLayoutParams, FramePosition};
use crate::layout::paragraph::Alignment;
use crate::layout::table::{CellLayoutParams, TableLayoutParams};

const DEFAULT_LIST_INDENT: f32 = 24.0;
const INDENT_PER_LEVEL: f32 = 24.0;

/// Convert a FlowSnapshot into layout params that can be fed to the Typesetter.
pub fn convert_flow(flow: &FlowSnapshot) -> FlowElements {
    let mut blocks = Vec::new();
    let mut tables = Vec::new();
    let mut frames = Vec::new();

    for (i, element) in flow.elements.iter().enumerate() {
        match element {
            FlowElementSnapshot::Block(block) => {
                blocks.push((i, convert_block(block)));
            }
            FlowElementSnapshot::Table(table) => {
                tables.push((i, convert_table(table)));
            }
            FlowElementSnapshot::Frame(frame) => {
                frames.push((i, convert_frame(frame)));
            }
        }
    }

    FlowElements {
        blocks,
        tables,
        frames,
    }
}

/// Converted flow elements, ordered by their position in the flow.
pub struct FlowElements {
    /// (flow_index, params)
    pub blocks: Vec<(usize, BlockLayoutParams)>,
    pub tables: Vec<(usize, TableLayoutParams)>,
    pub frames: Vec<(usize, FrameLayoutParams)>,
}

pub fn convert_block(block: &BlockSnapshot) -> BlockLayoutParams {
    let alignment = block
        .block_format
        .alignment
        .as_ref()
        .map(convert_alignment)
        .unwrap_or_default();

    let heading_scale = match block.block_format.heading_level {
        Some(1) => 2.0,
        Some(2) => 1.5,
        Some(3) => 1.25,
        Some(4) => 1.1,
        _ => 1.0,
    };

    let fragments: Vec<FragmentParams> = block
        .fragments
        .iter()
        .map(|f| convert_fragment(f, heading_scale))
        .collect();

    let indent_level = block.block_format.indent.unwrap_or(0) as f32;

    let (list_marker, list_indent) = if let Some(ref info) = block.list_info {
        (
            info.marker.clone(),
            DEFAULT_LIST_INDENT + indent_level * INDENT_PER_LEVEL,
        )
    } else {
        (String::new(), indent_level * INDENT_PER_LEVEL)
    };

    let checkbox = match block.block_format.marker {
        Some(text_document::MarkerType::Checked) => Some(true),
        Some(text_document::MarkerType::Unchecked) => Some(false),
        _ => None,
    };

    BlockLayoutParams {
        block_id: block.block_id,
        position: block.position,
        text: block.text.clone(),
        fragments,
        alignment,
        top_margin: block.block_format.top_margin.unwrap_or(0) as f32,
        bottom_margin: block.block_format.bottom_margin.unwrap_or(0) as f32,
        left_margin: block.block_format.left_margin.unwrap_or(0) as f32,
        right_margin: block.block_format.right_margin.unwrap_or(0) as f32,
        text_indent: block.block_format.text_indent.unwrap_or(0) as f32,
        list_marker,
        list_indent,
        tab_positions: block
            .block_format
            .tab_positions
            .iter()
            .map(|&t| t as f32)
            .collect(),
        line_height_multiplier: block.block_format.line_height,
        non_breakable_lines: block.block_format.non_breakable_lines.unwrap_or(false),
        checkbox,
        background_color: None, // TODO: parse CSS color string from block_format.background_color
    }
}

fn convert_fragment(frag: &FragmentContent, heading_scale: f32) -> FragmentParams {
    match frag {
        FragmentContent::Text {
            text,
            format,
            offset,
            length,
        } => FragmentParams {
            text: text.clone(),
            offset: *offset,
            length: *length,
            font_family: format.font_family.clone(),
            font_weight: format.font_weight,
            font_bold: format.font_bold,
            font_italic: format.font_italic,
            font_point_size: if heading_scale != 1.0 {
                // Apply heading scale; use 16 as default if no explicit size
                Some((format.font_point_size.unwrap_or(16) as f32 * heading_scale) as u32)
            } else {
                format.font_point_size
            },
            underline: format.font_underline.unwrap_or(false),
            overline: format.font_overline.unwrap_or(false),
            strikeout: format.font_strikeout.unwrap_or(false),
            is_link: format.is_anchor.unwrap_or(false),
            letter_spacing: format.letter_spacing.unwrap_or(0) as f32,
            word_spacing: format.word_spacing.unwrap_or(0) as f32,
        },
        FragmentContent::Image {
            name: _,
            width: _,
            height: _,
            quality: _,
            format,
            offset,
        } => {
            // For now, images are rendered as empty placeholders.
            // The adapter handles actual image loading.
            FragmentParams {
                text: String::new(),
                offset: *offset,
                length: 1,
                font_family: None,
                font_weight: None,
                font_bold: None,
                font_italic: None,
                font_point_size: None,
                underline: false,
                overline: false,
                strikeout: false,
                is_link: format.is_anchor.unwrap_or(false),
                letter_spacing: 0.0,
                word_spacing: 0.0,
            }
        }
    }
}

fn convert_alignment(a: &text_document::Alignment) -> Alignment {
    match a {
        text_document::Alignment::Left => Alignment::Left,
        text_document::Alignment::Right => Alignment::Right,
        text_document::Alignment::Center => Alignment::Center,
        text_document::Alignment::Justify => Alignment::Justify,
    }
}

pub fn convert_table(table: &TableSnapshot) -> TableLayoutParams {
    let column_widths: Vec<f32> = table.column_widths.iter().map(|&w| w as f32).collect();

    let cells: Vec<CellLayoutParams> = table.cells.iter().map(convert_cell).collect();

    TableLayoutParams {
        table_id: table.table_id,
        rows: table.rows,
        columns: table.columns,
        column_widths,
        border_width: table.format.border.unwrap_or(1) as f32,
        cell_spacing: table.format.cell_spacing.unwrap_or(0) as f32,
        cell_padding: table.format.cell_padding.unwrap_or(4) as f32,
        cells,
    }
}

fn convert_cell(cell: &CellSnapshot) -> CellLayoutParams {
    let blocks: Vec<BlockLayoutParams> = cell.blocks.iter().map(convert_block).collect();

    // Parse background color from CSS color string (simplified)
    let background_color = cell
        .format
        .background_color
        .as_ref()
        .map(|_| [0.9, 0.9, 0.9, 1.0]); // placeholder: light gray

    CellLayoutParams {
        row: cell.row,
        column: cell.column,
        blocks,
        background_color,
    }
}

pub fn convert_frame(frame: &FrameSnapshot) -> FrameLayoutParams {
    let mut blocks = Vec::new();
    let mut tables = Vec::new();

    for (i, element) in frame.elements.iter().enumerate() {
        match element {
            FlowElementSnapshot::Block(block) => {
                blocks.push(convert_block(block));
            }
            FlowElementSnapshot::Table(table) => {
                tables.push((i, convert_table(table)));
            }
            FlowElementSnapshot::Frame(_) => {
                // Nested frames within frames -could recurse, but for now skip
            }
        }
    }

    let position = match &frame.format.position {
        Some(text_document::FramePosition::InFlow) | None => FramePosition::Inline,
        Some(text_document::FramePosition::FloatLeft) => FramePosition::FloatLeft,
        Some(text_document::FramePosition::FloatRight) => FramePosition::FloatRight,
    };

    FrameLayoutParams {
        frame_id: frame.frame_id,
        position,
        width: frame.format.width.map(|w| w as f32),
        height: frame.format.height.map(|h| h as f32),
        margin_top: frame.format.top_margin.unwrap_or(0) as f32,
        margin_bottom: frame.format.bottom_margin.unwrap_or(0) as f32,
        margin_left: frame.format.left_margin.unwrap_or(0) as f32,
        margin_right: frame.format.right_margin.unwrap_or(0) as f32,
        padding: frame.format.padding.unwrap_or(0) as f32,
        border_width: frame.format.border.unwrap_or(0) as f32,
        blocks,
        tables,
    }
}
