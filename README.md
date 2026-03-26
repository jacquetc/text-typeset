[![crates.io](https://img.shields.io/crates/v/text-typeset?style=flat-square&logo=rust)](https://crates.io/crates/text-typeset)
[![API](https://docs.rs/text-typeset/badge.svg)](https://docs.rs/text-typeset)
![quality](https://img.shields.io/github/actions/workflow/status/jacquetc/text-typeset/ci.yml)
[![codecov](https://codecov.io/gh/jacquetc/text-typeset/branch/main/graph/badge.svg?token=AONY49DQM0)](https://codecov.io/gh/jacquetc/text-typeset)
[![license](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue?style=flat-square)](#license)

# text-typeset

Turns rich text documents into GPU-ready glyph quads.

Typesetting crate for the [text-document](https://github.com/jacquetc/text-document) ecosystem. Takes a rich text document model (styled paragraphs, tables, lists, frames) and produces positioned glyph quads, decoration rectangles, and a glyph atlas texture that any GPU framework can render in a few draw calls.

```text
text-document (model) --> text-typeset (shaping + layout) --> framework adapter (rendering)
```

## Features

- **Text shaping** via [rustybuzz](https://crates.io/crates/rustybuzz) (Rust port of HarfBuzz) with OpenType feature support
- **Font management** via [fontdb](https://crates.io/crates/fontdb) with CSS-spec font matching, generic family mapping, and glyph fallback
- **Glyph rasterization** via [swash](https://crates.io/crates/swash) with color emoji support (COLR/CBDT)
- **Glyph atlas** backed by [etagere](https://crates.io/crates/etagere) shelf packing with auto-grow and LRU eviction
- **Paragraph layout** with line breaking ([unicode-linebreak](https://crates.io/crates/unicode-linebreak), UAX #14), four alignment modes (left, right, center, justify), and first-line indent
- **BiDi text** analysis via [unicode-bidi](https://crates.io/crates/unicode-bidi) with per-run directional shaping
- **Tables** with column width distribution, cell layout, borders, and cell backgrounds
- **Lists** with marker rendering (bullet, decimal, alpha, roman) at configurable indent levels
- **Frames** with inline, float-left, float-right, and absolute positioning
- **Text decorations**: underline, strikeout, overline (from font metrics)
- **Letter spacing** and **word spacing**
- **Hit testing**: screen coordinates to document position with region detection (text, margin, link, image)
- **Cursor display** with caret rendering, blink support, multi-cursor, and selection painting with full-width line highlighting
- **Scrolling**: `ensure_caret_visible`, `scroll_to_position`, viewport culling
- **Incremental relayout**: update a single block without re-laying-out the entire document
- **Content width modes**: auto (follows viewport) or fixed (for page-like WYSIWYG layout)

## Framework-agnostic output

text-typeset produces a `RenderFrame` containing:

- **Glyph quads** (`[screen_rect, atlas_rect, color]`) drawn as textured rectangles from the atlas
- **Image quads** (position + resource name) for inline images loaded by the adapter
- **Decoration rects** (underline, strikeout, selection, cursor, table borders, backgrounds)
- **Atlas texture** (RGBA, updated incrementally)

The rendering contract is thin: "draw N sub-rects from a texture + M colored rects." Any framework that supports textured quads can serve as a backend: Godot (`draw_texture_rect_region`), Qt (`QPainter::drawImage`), wgpu, egui, iced.

## Quick start

```rust
use text_typeset::Typesetter;
use text_document::TextDocument;

// Set up the typesetter
let mut typesetter = Typesetter::new();
let font = typesetter.register_font(include_bytes!("fonts/NotoSans.ttf"));
typesetter.set_default_font(font, 16.0);
typesetter.set_viewport(800.0, 600.0);

// Load a document
let doc = TextDocument::new();
doc.set_plain_text("Hello, world!").unwrap();

// Layout and render
typesetter.layout_full(&doc.snapshot_flow());
let frame = typesetter.render();

// frame.glyphs    -> glyph quads to draw
// frame.atlas_pixels -> RGBA texture data
// frame.decorations  -> cursor, selection, underlines
```

## Content width

By default, text wraps at the viewport width (web/editor style). For page-like layout:

```rust
// Fixed width: text wraps at 600px regardless of viewport size
typesetter.set_content_width(600.0);

// Back to auto: text reflows when viewport resizes
typesetter.set_content_width_auto();
```

## Dependencies

| Crate | Version | Purpose |
| --- | --- | --- |
| rustybuzz | 0.20 | OpenType shaping |
| swash | 0.2 | Font metrics and glyph rasterization |
| fontdb | 0.23 | Font discovery and CSS-spec matching |
| etagere | 0.3 | Glyph atlas allocation (shelf packing) |
| unicode-linebreak | 0.1 | Line break opportunities (UAX #14) |
| unicode-bidi | 0.3 | Bidirectional text (UAX #9) |
| text-document | -- | Document model (optional, default feature) |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
