//! # text-typeset
//!
//! Turns rich text documents into GPU-ready glyph quads.
//!
//! Typesetting crate for the `text-document` ecosystem. Takes a rich text
//! document model (styled paragraphs, tables, lists, frames) and produces
//! positioned glyph quads, decoration rectangles, and a glyph atlas texture
//! that any GPU framework can render in a few draw calls.
//!
//! ```text
//! text-document (model) --> text-typeset (shaping + layout) --> framework adapter (rendering)
//! ```
//!
//! # Quick start
//!
//! ```rust,no_run
//! use text_typeset::Typesetter;
//!
//! let mut ts = Typesetter::new();
//! let font = ts.register_font(include_bytes!("../test-fonts/NotoSans-Variable.ttf"));
//! ts.set_default_font(font, 16.0);
//! ts.set_viewport(800.0, 600.0);
//!
//! // With text-document (default feature):
//! # #[cfg(feature = "text-document")]
//! # {
//! let doc = text_document::TextDocument::new();
//! doc.set_plain_text("Hello, world!").unwrap();
//! ts.layout_full(&doc.snapshot_flow());
//! # }
//!
//! let frame = ts.render();
//! // frame.glyphs     -> glyph quads (textured rects from atlas)
//! // frame.atlas_pixels -> RGBA texture to upload
//! // frame.decorations  -> cursor, selection, underlines, borders
//! ```
//!
//! # Features
//!
//! - `text-document` (default) : enables [`bridge`] module and [`Typesetter::layout_full`]
//!   for direct integration with text-document's `FlowSnapshot`.

mod types;

pub mod atlas;
pub mod font;
pub mod layout;
mod render;
pub mod shaping;

#[cfg(feature = "text-document")]
pub mod bridge;

mod typesetter;

// Public API
pub use types::{
    BlockVisualInfo, CursorDisplay, DecorationKind, DecorationRect, FontFaceId, GlyphQuad,
    HitRegion, HitTestResult, ImageQuad, ParagraphResult, RenderFrame, SingleLineResult,
    TextFormat, UnderlineStyle, VerticalAlignment,
};

pub use typesetter::{ContentWidthMode, Typesetter};
