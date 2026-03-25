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

/// Text decoration flags carried from the source TextFormat.
#[derive(Clone, Copy, Debug, Default)]
pub struct RunDecorations {
    pub underline: bool,
    pub overline: bool,
    pub strikeout: bool,
    pub is_link: bool,
}
