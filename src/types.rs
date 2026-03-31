/// Opaque handle to a registered font face.
///
/// Obtained from [`crate::Typesetter::register_font`] or [`crate::Typesetter::register_font_as`].
/// Pass to [`crate::Typesetter::set_default_font`] to make it the default.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FontFaceId(pub u32);

// ── Render output ───────────────────────────────────────────────

/// Everything needed to draw one frame.
///
/// Produced by [`crate::Typesetter::render`]. Contains glyph quads (textured rectangles
/// from the atlas), inline image placeholders, and decoration rectangles
/// (selections, cursor, underlines, table borders, etc.).
///
/// The adapter draws the frame in three passes:
/// 1. Upload `atlas_pixels` as a GPU texture (only when `atlas_dirty` is true).
/// 2. Draw each [`GlyphQuad`] as a textured rectangle from the atlas.
/// 3. Draw each [`DecorationRect`] as a colored rectangle.
pub struct RenderFrame {
    /// True if the atlas texture changed since the last frame (needs re-upload).
    pub atlas_dirty: bool,
    /// Atlas texture width in pixels.
    pub atlas_width: u32,
    /// Atlas texture height in pixels.
    pub atlas_height: u32,
    /// RGBA pixel data, row-major. Length = `atlas_width * atlas_height * 4`.
    pub atlas_pixels: Vec<u8>,
    /// One textured rectangle per visible glyph.
    pub glyphs: Vec<GlyphQuad>,
    /// Inline image placeholders. The adapter loads the actual image data
    /// (e.g., via `TextDocument::resource(name)`) and draws it at the given
    /// screen position.
    pub images: Vec<ImageQuad>,
    /// Decoration rectangles: selections, cursor, underlines, strikeouts,
    /// overlines, backgrounds, table borders, and cell backgrounds.
    pub decorations: Vec<DecorationRect>,
    /// Per-block glyph data for incremental updates. Keyed by block_id.
    pub(crate) block_glyphs: Vec<(usize, Vec<GlyphQuad>)>,
    /// Per-block decoration data (underlines, etc. — NOT cursor/selection).
    pub(crate) block_decorations: Vec<(usize, Vec<DecorationRect>)>,
    /// Per-block image data for incremental updates.
    pub(crate) block_images: Vec<(usize, Vec<ImageQuad>)>,
    /// Per-block height snapshot for detecting height changes in incremental render.
    pub(crate) block_heights: std::collections::HashMap<usize, f32>,
}

/// A positioned glyph to draw as a textured quad from the atlas.
///
/// The adapter draws the rectangle at `screen` position, sampling from
/// the `atlas` rectangle in the atlas texture, tinted with `color`.
#[derive(Clone)]
pub struct GlyphQuad {
    /// Screen position and size: `[x, y, width, height]` in pixels.
    pub screen: [f32; 4],
    /// Atlas source rectangle: `[x, y, width, height]` in atlas pixel coordinates.
    pub atlas: [f32; 4],
    /// Glyph color: `[r, g, b, a]`, 0.0-1.0.
    /// For normal text glyphs, this is the text color (default black).
    /// For color emoji, this is `[1, 1, 1, 1]` (color is baked into the atlas).
    pub color: [f32; 4],
}

/// An inline image placeholder.
///
/// text-typeset computes the position and size but does NOT load or rasterize
/// the image. The adapter retrieves the image data (e.g., from
/// `TextDocument::resource(name)`) and draws it as a separate texture.
#[derive(Clone)]
pub struct ImageQuad {
    /// Screen position and size: `[x, y, width, height]` in pixels.
    pub screen: [f32; 4],
    /// Image resource name (matches `FragmentContent::Image::name` from text-document).
    pub name: String,
}

/// A colored rectangle for decorations (underlines, selections, borders, etc.).
#[derive(Clone)]
pub struct DecorationRect {
    /// Screen position and size: `[x, y, width, height]` in pixels.
    pub rect: [f32; 4],
    /// Color: `[r, g, b, a]`, 0.0-1.0.
    pub color: [f32; 4],
    /// What kind of decoration this rectangle represents.
    pub kind: DecorationKind,
}

/// The type of a [`DecorationRect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    /// Selection highlight (translucent background behind selected text).
    Selection,
    /// Cursor caret (thin vertical line at the insertion point).
    Cursor,
    /// Underline (below baseline, from font metrics).
    Underline,
    /// Strikethrough (at x-height, from font metrics).
    Strikeout,
    /// Overline (at ascent line).
    Overline,
    /// Generic background (e.g., frame borders).
    Background,
    /// Block-level background color.
    BlockBackground,
    /// Table border line.
    TableBorder,
    /// Table cell background color.
    TableCellBackground,
    /// Text-level background highlight (behind individual text runs).
    /// Adapters should draw these before glyph quads so text appears on top.
    TextBackground,
    /// Cell-level selection highlight (entire cell background when cells are
    /// selected as a rectangular region, as opposed to text within cells).
    CellSelection,
}

/// Underline style for text decorations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnderlineStyle {
    /// No underline.
    #[default]
    None,
    /// Solid single underline.
    Single,
    /// Dashed underline.
    Dash,
    /// Dotted underline.
    Dot,
    /// Alternating dash-dot pattern.
    DashDot,
    /// Alternating dash-dot-dot pattern.
    DashDotDot,
    /// Wavy underline.
    Wave,
    /// Spell-check underline (wavy, typically red).
    SpellCheck,
}

/// Vertical alignment for characters (superscript, subscript, etc.).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VerticalAlignment {
    /// Normal baseline alignment.
    #[default]
    Normal,
    /// Superscript: smaller size, shifted up.
    SuperScript,
    /// Subscript: smaller size, shifted down.
    SubScript,
}

// ── Hit testing ─────────────────────────────────────────────────

/// Result of [`crate::Typesetter::hit_test`] - maps a screen-space point to a
/// document position.
pub struct HitTestResult {
    /// Absolute character position in the document.
    pub position: usize,
    /// Which block (paragraph) was hit, identified by stable block ID.
    pub block_id: usize,
    /// Character offset within the block (0 = start of block).
    pub offset_in_block: usize,
    /// What region of the layout was hit.
    pub region: HitRegion,
    /// Tooltip text if the hit position has a tooltip. None otherwise.
    pub tooltip: Option<String>,
    /// When non-None, the hit position is inside a table cell.
    /// Identifies the table by its stable table ID.
    /// None for hits on top-level blocks, frame blocks, or outside any table.
    pub table_id: Option<usize>,
}

/// What region of the layout a hit test landed in.
#[derive(Debug)]
pub enum HitRegion {
    /// Inside a text run (normal text content).
    Text,
    /// In the block's left margin area (before any text content).
    LeftMargin,
    /// In the block's indent area.
    Indent,
    /// On a table border line.
    TableBorder,
    /// Below all content in the document.
    BelowContent,
    /// Past the end of a line (to the right of the last character).
    PastLineEnd,
    /// On an inline image.
    Image { name: String },
    /// On a hyperlink.
    Link { href: String },
}

// ── Cursor display ──────────────────────────────────────────────

/// Cursor display state for rendering.
///
/// The adapter reads cursor position from text-document's `TextCursor`
/// and creates this struct to feed to [`crate::Typesetter::set_cursor`].
/// text-typeset uses it to generate caret and selection decorations
/// in the next [`crate::Typesetter::render`] call.
pub struct CursorDisplay {
    /// Cursor position (character offset in the document).
    pub position: usize,
    /// Selection anchor. Equals `position` when there is no selection.
    /// When different from `position`, the range `[min(anchor, position), max(anchor, position))`
    /// is highlighted as a selection.
    pub anchor: usize,
    /// Whether the caret is visible (false during the blink-off phase).
    /// The adapter manages the blink timer; text-typeset just respects this flag.
    pub visible: bool,
    /// When non-empty, render cell-level selection highlights instead of
    /// text-level selection. Each tuple is `(table_id, row, col)` identifying
    /// a selected cell. The adapter fills this from `TextCursor::selected_cells()`.
    pub selected_cells: Vec<(usize, usize, usize)>,
}

// ── Scrolling ───────────────────────────────────────────────────

/// Visual position and size of a laid-out block.
///
/// Returned by [`crate::Typesetter::block_visual_info`].
pub struct BlockVisualInfo {
    /// Block ID (matches `BlockSnapshot::block_id`).
    pub block_id: usize,
    /// Y position of the block's top edge relative to the document start, in pixels.
    pub y: f32,
    /// Total height of the block including margins, in pixels.
    pub height: f32,
}

// ── Single-line API ────────────────────────────────────────────

/// Text formatting parameters for the single-line layout API.
///
/// Controls font selection, size, and text color. All fields are optional
/// and fall back to the typesetter's defaults (default font, default size,
/// default text color).
#[derive(Clone, Debug, Default)]
pub struct TextFormat {
    /// Font family name (e.g., "Noto Sans", "monospace").
    /// None means use the default font.
    pub font_family: Option<String>,
    /// Font weight (100-900). Overrides `font_bold`.
    pub font_weight: Option<u32>,
    /// Shorthand for weight 700. Ignored if `font_weight` is set.
    pub font_bold: Option<bool>,
    /// Italic style.
    pub font_italic: Option<bool>,
    /// Font size in pixels. None means use the default size.
    pub font_size: Option<f32>,
    /// Text color (RGBA, 0.0-1.0). None means use the typesetter's text color.
    pub color: Option<[f32; 4]>,
}

/// Result of [`crate::Typesetter::layout_single_line`].
///
/// Contains the measured dimensions and GPU-ready glyph quads for a
/// single line of text. No flow layout, line breaking, or bidi analysis
/// is performed.
pub struct SingleLineResult {
    /// Total advance width of the shaped text, in pixels.
    pub width: f32,
    /// Line height (ascent + descent + leading), in pixels.
    pub height: f32,
    /// Distance from the top of the line to the baseline, in pixels.
    pub baseline: f32,
    /// GPU-ready glyph quads, positioned at y=0 (no scroll offset).
    pub glyphs: Vec<GlyphQuad>,
}

impl RenderFrame {
    pub(crate) fn new() -> Self {
        Self {
            atlas_dirty: false,
            atlas_width: 0,
            atlas_height: 0,
            atlas_pixels: Vec::new(),
            glyphs: Vec::new(),
            images: Vec::new(),
            decorations: Vec::new(),
            block_glyphs: Vec::new(),
            block_decorations: Vec::new(),
            block_images: Vec::new(),
            block_heights: std::collections::HashMap::new(),
        }
    }
}
