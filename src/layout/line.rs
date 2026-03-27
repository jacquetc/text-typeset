use std::ops::Range;

use crate::shaping::run::ShapedRun;

pub struct LayoutLine {
    pub runs: Vec<PositionedRun>,
    /// Baseline y relative to block top (set by block layout).
    pub y: f32,
    pub ascent: f32,
    pub descent: f32,
    pub leading: f32,
    /// Total line height: ascent + descent + leading.
    pub line_height: f32,
    /// Actual content width (sum of run advances).
    pub width: f32,
    /// Character range in the block's text.
    pub char_range: Range<usize>,
}

pub struct PositionedRun {
    pub shaped_run: ShapedRun,
    /// X offset from the left edge of the content area.
    pub x: f32,
    /// Decoration flags for this run.
    pub decorations: RunDecorations,
}

/// Text decoration flags and metadata carried from the source TextFormat.
#[derive(Clone, Debug, Default)]
pub struct RunDecorations {
    pub underline_style: crate::types::UnderlineStyle,
    pub overline: bool,
    pub strikeout: bool,
    pub is_link: bool,
    /// Text foreground color (RGBA). None means default (black).
    pub foreground_color: Option<[f32; 4]>,
    /// Underline color (RGBA). None means use foreground_color.
    pub underline_color: Option<[f32; 4]>,
    /// Text-level background highlight color (RGBA). None means transparent.
    pub background_color: Option<[f32; 4]>,
    /// Hyperlink destination URL.
    pub anchor_href: Option<String>,
    /// Tooltip text.
    pub tooltip: Option<String>,
    /// Vertical alignment (normal, superscript, subscript).
    pub vertical_alignment: crate::types::VerticalAlignment,
}
