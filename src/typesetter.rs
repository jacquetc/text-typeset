use crate::atlas::allocator::GlyphAtlas;
use crate::atlas::cache::{GlyphCache, GlyphCacheKey};
use crate::atlas::rasterizer::rasterize_glyph;
use crate::font::registry::FontRegistry;
use crate::layout::flow::{FlowItem, FlowLayout};
use crate::types::{
    BlockVisualInfo, CursorDisplay, FontFaceId, GlyphQuad, HitTestResult, RenderFrame,
    SingleLineResult, TextFormat,
};

/// How the content (layout) width is determined.
///
/// Controls whether text reflows when the viewport resizes (web/editor style)
/// or wraps at a fixed width (page/WYSIWYG style).
#[derive(Debug, Clone, Copy, Default)]
pub enum ContentWidthMode {
    /// Content width equals viewport width. Text reflows on window resize.
    /// This is the default.typical for editors and web-style layout.
    #[default]
    Auto,
    /// Content width is fixed, independent of viewport.
    /// For page-like layout (WYSIWYG), print preview, or side panels.
    /// If the content is wider than the viewport, horizontal scrolling is needed.
    /// If narrower, the content is centered or left-aligned within the viewport.
    Fixed(f32),
}

/// The main entry point for text typesetting.
///
/// Owns the font registry, glyph atlas, layout cache, and render state.
/// The typical usage pattern is:
///
/// 1. Create with [`Typesetter::new`]
/// 2. Register fonts with [`register_font`](Typesetter::register_font)
/// 3. Set default font with [`set_default_font`](Typesetter::set_default_font)
/// 4. Set viewport with [`set_viewport`](Typesetter::set_viewport)
/// 5. Lay out content with [`layout_full`](Typesetter::layout_full) or [`layout_blocks`](Typesetter::layout_blocks)
/// 6. Set cursor state with [`set_cursor`](Typesetter::set_cursor)
/// 7. Render with [`render`](Typesetter::render) to get a [`RenderFrame`]
/// 8. On edits, use [`relayout_block`](Typesetter::relayout_block) for incremental updates
///
/// # Thread safety
///
/// `Typesetter` is `!Send + !Sync` because its internal fontdb, atlas allocator,
/// and swash scale context are not thread-safe. It lives on the adapter's render
/// thread alongside the framework's drawing calls.
pub struct Typesetter {
    font_registry: FontRegistry,
    atlas: GlyphAtlas,
    glyph_cache: GlyphCache,
    flow_layout: FlowLayout,
    scale_context: swash::scale::ScaleContext,
    render_frame: RenderFrame,
    scroll_offset: f32,
    rendered_scroll_offset: f32,
    viewport_width: f32,
    viewport_height: f32,
    content_width_mode: ContentWidthMode,
    selection_color: [f32; 4],
    cursor_color: [f32; 4],
    text_color: [f32; 4],
    cursors: Vec<CursorDisplay>,
    zoom: f32,
    rendered_zoom: f32,
}

impl Typesetter {
    /// Create a new typesetter with no fonts loaded.
    ///
    /// Call [`register_font`](Self::register_font) and [`set_default_font`](Self::set_default_font)
    /// before laying out any content.
    pub fn new() -> Self {
        Self {
            font_registry: FontRegistry::new(),
            atlas: GlyphAtlas::new(),
            glyph_cache: GlyphCache::new(),
            flow_layout: FlowLayout::new(),
            scale_context: swash::scale::ScaleContext::new(),
            render_frame: RenderFrame::new(),
            scroll_offset: 0.0,
            rendered_scroll_offset: f32::NAN,
            viewport_width: 0.0,
            viewport_height: 0.0,
            content_width_mode: ContentWidthMode::Auto,
            selection_color: [0.26, 0.52, 0.96, 0.3],
            cursor_color: [0.0, 0.0, 0.0, 1.0],
            text_color: [0.0, 0.0, 0.0, 1.0],
            cursors: Vec::new(),
            zoom: 1.0,
            rendered_zoom: f32::NAN,
        }
    }

    // ── Font registration ───────────────────────────────────────

    /// Register a font face from raw TTF/OTF/WOFF bytes.
    ///
    /// Parses the font's name table to extract family, weight, and style,
    /// then indexes it for CSS-spec font matching via [`fontdb`].
    /// Returns the first face ID (font collections like `.ttc` may contain multiple faces).
    ///
    /// # Panics
    ///
    /// Panics if the font data contains no parseable faces.
    pub fn register_font(&mut self, data: &[u8]) -> FontFaceId {
        let ids = self.font_registry.register_font(data);
        ids.into_iter()
            .next()
            .expect("font data contained no faces")
    }

    /// Register a font with explicit metadata, overriding the font's name table.
    ///
    /// Use when the font's internal metadata is unreliable or when aliasing
    /// a font to a different family name.
    ///
    /// # Panics
    ///
    /// Panics if the font data contains no parseable faces.
    pub fn register_font_as(
        &mut self,
        data: &[u8],
        family: &str,
        weight: u16,
        italic: bool,
    ) -> FontFaceId {
        let ids = self
            .font_registry
            .register_font_as(data, family, weight, italic);
        ids.into_iter()
            .next()
            .expect("font data contained no faces")
    }

    /// Set which font face to use as the document default.
    ///
    /// This is the fallback font when a fragment's `TextFormat` doesn't specify
    /// a family or the specified family isn't found.
    pub fn set_default_font(&mut self, face: FontFaceId, size_px: f32) {
        self.font_registry.set_default_font(face, size_px);
    }

    /// Map a generic family name to a registered font family.
    ///
    /// Common mappings: `"serif"` → `"Noto Serif"`, `"monospace"` → `"Fira Code"`.
    /// When text-document specifies `font_family: "monospace"`, the typesetter
    /// resolves it through this mapping before querying fontdb.
    pub fn set_generic_family(&mut self, generic: &str, family: &str) {
        self.font_registry.set_generic_family(generic, family);
    }

    /// Look up the family name of a registered font by its face ID.
    pub fn font_family_name(&self, face_id: FontFaceId) -> Option<String> {
        self.font_registry.font_family_name(face_id)
    }

    /// Access the font registry for advanced queries (glyph coverage, fallback, etc.).
    pub fn font_registry(&self) -> &FontRegistry {
        &self.font_registry
    }

    // ── Viewport & content width ───────────────────────────────

    /// Set the viewport dimensions (visible area in pixels).
    ///
    /// The viewport controls:
    /// - **Culling**: only blocks within the viewport are rendered.
    /// - **Selection highlight**: multi-line selections extend to viewport width.
    /// - **Layout width** (in [`ContentWidthMode::Auto`]): text wraps at viewport width.
    ///
    /// Call this when the window or container resizes.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.flow_layout.viewport_width = width;
        self.flow_layout.viewport_height = height;
    }

    /// Set a fixed content width, independent of viewport.
    ///
    /// Text wraps at this width regardless of how wide the viewport is.
    /// Use for page-like (WYSIWYG) layout or documents with explicit width.
    /// Pass `f32::INFINITY` for no-wrap mode.
    pub fn set_content_width(&mut self, width: f32) {
        self.content_width_mode = ContentWidthMode::Fixed(width);
    }

    /// Set content width to follow viewport width (default).
    ///
    /// Text reflows when the viewport is resized. This is the standard
    /// behavior for editors and web-style layout.
    pub fn set_content_width_auto(&mut self) {
        self.content_width_mode = ContentWidthMode::Auto;
    }

    /// The effective width used for text layout (line wrapping, table columns, etc.).
    ///
    /// In [`ContentWidthMode::Auto`], equals `viewport_width / zoom` so that
    /// text reflows to fit the zoomed viewport.
    /// In [`ContentWidthMode::Fixed`], equals the set value (zoom only magnifies).
    pub fn layout_width(&self) -> f32 {
        match self.content_width_mode {
            ContentWidthMode::Auto => self.viewport_width / self.zoom,
            ContentWidthMode::Fixed(w) => w,
        }
    }

    /// Set the vertical scroll offset in pixels from the top of the document.
    ///
    /// Affects which blocks are visible (culling) and the screen-space
    /// y coordinates in the rendered [`RenderFrame`].
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset;
    }

    /// Total content height after layout, in pixels.
    ///
    /// Use for scrollbar range: `scrollbar.max = content_height - viewport_height`.
    pub fn content_height(&self) -> f32 {
        self.flow_layout.content_height
    }

    /// Maximum content width across all laid-out lines, in pixels.
    ///
    /// Use for horizontal scrollbar range when text wrapping is disabled.
    /// Returns 0.0 if no blocks have been laid out.
    pub fn max_content_width(&self) -> f32 {
        self.flow_layout.cached_max_content_width
    }

    // -- Zoom ────────────────────────────────────────────────────

    /// Set the display zoom level.
    ///
    /// Zoom is a pure display transform: layout stays at base size, and all
    /// screen-space output (glyph quads, decorations, caret rects) is scaled
    /// by the zoom factor. Hit-test input coordinates are inversely scaled.
    ///
    /// This is PDF-viewer-style zoom (no text reflow). For browser-style
    /// zoom that reflows text, combine with
    /// `set_content_width(viewport_width / zoom)`.
    ///
    /// Clamped to `0.1..=10.0`. Default is `1.0`.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.1, 10.0);
    }

    /// The current display zoom level (default 1.0).
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    // ── Layout ──────────────────────────────────────────────────

    /// Full layout from a text-document `FlowSnapshot`.
    ///
    /// Converts all snapshot elements (blocks, tables, frames) to internal
    /// layout params and lays out the entire document flow. Call this on
    /// `DocumentReset` events or initial document load.
    ///
    /// For incremental updates after small edits, prefer [`relayout_block`](Self::relayout_block).
    #[cfg(feature = "text-document")]
    pub fn layout_full(&mut self, flow: &text_document::FlowSnapshot) {
        use crate::bridge::convert_flow;

        let converted = convert_flow(flow);

        // Merge all elements by flow index and process in order
        let mut all_items: Vec<(usize, FlowItemKind)> = Vec::new();
        for (idx, params) in converted.blocks {
            all_items.push((idx, FlowItemKind::Block(params)));
        }
        for (idx, params) in converted.tables {
            all_items.push((idx, FlowItemKind::Table(params)));
        }
        for (idx, params) in converted.frames {
            all_items.push((idx, FlowItemKind::Frame(params)));
        }
        all_items.sort_by_key(|(idx, _)| *idx);

        let lw = self.layout_width();
        self.flow_layout.clear();
        self.flow_layout.viewport_width = self.viewport_width;
        self.flow_layout.viewport_height = self.viewport_height;

        for (_idx, kind) in all_items {
            match kind {
                FlowItemKind::Block(params) => {
                    self.flow_layout.add_block(&self.font_registry, &params, lw);
                }
                FlowItemKind::Table(params) => {
                    self.flow_layout.add_table(&self.font_registry, &params, lw);
                }
                FlowItemKind::Frame(params) => {
                    self.flow_layout.add_frame(&self.font_registry, &params, lw);
                }
            }
        }
    }

    /// Lay out a list of blocks from scratch (framework-agnostic API).
    ///
    /// Replaces all existing layout state with the given blocks.
    /// This is the non-text-document equivalent of [`layout_full`](Self::layout_full).
    /// the caller converts snapshot types to [`BlockLayoutParams`](crate::layout::block::BlockLayoutParams).
    pub fn layout_blocks(&mut self, block_params: Vec<crate::layout::block::BlockLayoutParams>) {
        self.flow_layout
            .layout_blocks(&self.font_registry, block_params, self.layout_width());
    }

    /// Add a frame to the current flow layout.
    ///
    /// The frame is placed after all previously laid-out content.
    /// Frame position (inline, float, absolute) is determined by
    /// [`FrameLayoutParams::position`](crate::layout::frame::FrameLayoutParams).
    pub fn add_frame(&mut self, params: &crate::layout::frame::FrameLayoutParams) {
        self.flow_layout
            .add_frame(&self.font_registry, params, self.layout_width());
    }

    /// Add a table to the current flow layout.
    ///
    /// The table is placed after all previously laid-out content.
    pub fn add_table(&mut self, params: &crate::layout::table::TableLayoutParams) {
        self.flow_layout
            .add_table(&self.font_registry, params, self.layout_width());
    }

    /// Relayout a single block after its content or formatting changed.
    ///
    /// Re-shapes and re-wraps the block, then shifts subsequent blocks
    /// if the height changed. Much cheaper than [`layout_full`](Self::layout_full)
    /// for single-block edits (typing, formatting changes).
    ///
    /// If the block is inside a table cell (`BlockSnapshot::table_cell` is `Some`),
    /// the table row height is re-measured and content below the table shifts.
    pub fn relayout_block(&mut self, params: &crate::layout::block::BlockLayoutParams) {
        self.flow_layout
            .relayout_block(&self.font_registry, params, self.layout_width());
    }

    // ── Rendering ───────────────────────────────────────────────

    /// Render the visible viewport and return everything needed to draw.
    ///
    /// Performs viewport culling (only processes blocks within the scroll window),
    /// rasterizes any new glyphs into the atlas, and produces glyph quads,
    /// image placeholders, and decoration rectangles.
    ///
    /// The returned reference borrows the `Typesetter`. The adapter should iterate
    /// the frame for drawing, then drop the reference before calling any
    /// layout/scroll methods on the next frame.
    ///
    /// On each call, stale glyphs (unused for ~120 frames) are evicted from the
    /// atlas to reclaim space.
    pub fn render(&mut self) -> &RenderFrame {
        let effective_vw = self.viewport_width / self.zoom;
        let effective_vh = self.viewport_height / self.zoom;
        crate::render::frame::build_render_frame(
            &self.flow_layout,
            &self.font_registry,
            &mut self.atlas,
            &mut self.glyph_cache,
            &mut self.scale_context,
            self.scroll_offset,
            effective_vw,
            effective_vh,
            &self.cursors,
            self.cursor_color,
            self.selection_color,
            self.text_color,
            &mut self.render_frame,
        );
        self.rendered_scroll_offset = self.scroll_offset;
        self.rendered_zoom = self.zoom;
        apply_zoom(&mut self.render_frame, self.zoom);
        &self.render_frame
    }

    /// Incremental render that only re-renders one block's glyphs.
    ///
    /// Reuses cached glyph/decoration data for all other blocks from the
    /// last full `render()`. Use after `relayout_block()` when only one
    /// block's text changed.
    ///
    /// If the block's height changed (causing subsequent blocks to shift),
    /// this falls back to a full `render()` since cached glyph positions
    /// for other blocks would be stale.
    pub fn render_block_only(&mut self, block_id: usize) -> &RenderFrame {
        // If scroll offset or zoom changed, all cached glyph positions are stale.
        if (self.scroll_offset - self.rendered_scroll_offset).abs() > 0.001
            || (self.zoom - self.rendered_zoom).abs() > 0.001
        {
            return self.render();
        }

        // Table cell blocks are cached per-table (keyed by table_id), and
        // frame blocks are cached per-frame (keyed by frame_id). Neither has
        // entries in block_decorations or block_glyphs keyed by the cell
        // block_id, so incremental rendering cannot update them in place.
        // Fall back to a full render for both cases.
        if !self.flow_layout.blocks.contains_key(&block_id) {
            let in_table = self.flow_layout.tables.values().any(|table| {
                table
                    .cell_layouts
                    .iter()
                    .any(|c| c.blocks.iter().any(|b| b.block_id == block_id))
            });
            if in_table {
                return self.render();
            }
            let in_frame = self
                .flow_layout
                .frames
                .values()
                .any(|frame| crate::layout::flow::frame_contains_block(frame, block_id));
            if in_frame {
                return self.render();
            }
        }

        // If the block's height changed, cached glyph positions for subsequent
        // blocks are stale. Fall back to a full re-render.
        if let Some(block) = self.flow_layout.blocks.get(&block_id) {
            let old_height = self
                .render_frame
                .block_heights
                .get(&block_id)
                .copied()
                .unwrap_or(block.height);
            if (block.height - old_height).abs() > 0.001 {
                return self.render();
            }
        }

        // Re-render just this block's glyphs into a temporary frame
        let effective_vw = self.viewport_width / self.zoom;
        let effective_vh = self.viewport_height / self.zoom;
        let mut new_glyphs = Vec::new();
        let mut new_images = Vec::new();
        if let Some(block) = self.flow_layout.blocks.get(&block_id) {
            let mut tmp = crate::types::RenderFrame::new();
            crate::render::frame::render_block_at_offset(
                block,
                0.0,
                0.0,
                &self.font_registry,
                &mut self.atlas,
                &mut self.glyph_cache,
                &mut self.scale_context,
                self.scroll_offset,
                effective_vh,
                self.text_color,
                &mut tmp,
            );
            new_glyphs = tmp.glyphs;
            new_images = tmp.images;
        }

        // Re-generate this block's decorations
        let new_decos = if let Some(block) = self.flow_layout.blocks.get(&block_id) {
            crate::render::decoration::generate_block_decorations(
                block,
                &self.font_registry,
                self.scroll_offset,
                effective_vh,
                0.0,
                0.0,
                effective_vw,
                self.text_color,
            )
        } else {
            Vec::new()
        };

        // Replace this block's entry in the per-block caches
        if let Some(entry) = self
            .render_frame
            .block_glyphs
            .iter_mut()
            .find(|(id, _)| *id == block_id)
        {
            entry.1 = new_glyphs;
        }
        if let Some(entry) = self
            .render_frame
            .block_images
            .iter_mut()
            .find(|(id, _)| *id == block_id)
        {
            entry.1 = new_images;
        }
        if let Some(entry) = self
            .render_frame
            .block_decorations
            .iter_mut()
            .find(|(id, _)| *id == block_id)
        {
            entry.1 = new_decos;
        }

        // Rebuild flat vecs from per-block cache + cursor decorations
        self.rebuild_flat_frame();
        apply_zoom(&mut self.render_frame, self.zoom);

        &self.render_frame
    }

    /// Lightweight render that only updates cursor/selection decorations.
    ///
    /// Reuses the existing glyph quads and images from the last full `render()`.
    /// Use this when only the cursor blinked or selection changed, not the text.
    ///
    /// If the scroll offset changed since the last full render, falls back to
    /// a full [`render`](Self::render) so that glyph positions are updated.
    pub fn render_cursor_only(&mut self) -> &RenderFrame {
        // If scroll offset or zoom changed, glyph quads are stale - need full re-render
        if (self.scroll_offset - self.rendered_scroll_offset).abs() > 0.001
            || (self.zoom - self.rendered_zoom).abs() > 0.001
        {
            return self.render();
        }

        // Remove old cursor/selection decorations, keep block decorations
        self.render_frame.decorations.retain(|d| {
            !matches!(
                d.kind,
                crate::types::DecorationKind::Cursor
                    | crate::types::DecorationKind::Selection
                    | crate::types::DecorationKind::CellSelection
            )
        });

        // Regenerate cursor/selection decorations at 1x, then zoom
        let effective_vw = self.viewport_width / self.zoom;
        let effective_vh = self.viewport_height / self.zoom;
        let mut cursor_decos = crate::render::cursor::generate_cursor_decorations(
            &self.flow_layout,
            &self.cursors,
            self.scroll_offset,
            self.cursor_color,
            self.selection_color,
            effective_vw,
            effective_vh,
        );
        apply_zoom_decorations(&mut cursor_decos, self.zoom);
        self.render_frame.decorations.extend(cursor_decos);

        &self.render_frame
    }

    /// Rebuild flat glyphs/images/decorations from per-block caches + cursor decorations.
    fn rebuild_flat_frame(&mut self) {
        self.render_frame.glyphs.clear();
        self.render_frame.images.clear();
        self.render_frame.decorations.clear();
        for (_, glyphs) in &self.render_frame.block_glyphs {
            self.render_frame.glyphs.extend_from_slice(glyphs);
        }
        for (_, images) in &self.render_frame.block_images {
            self.render_frame.images.extend_from_slice(images);
        }
        for (_, decos) in &self.render_frame.block_decorations {
            self.render_frame.decorations.extend_from_slice(decos);
        }

        // Regenerate table and frame decorations (these are not stored in
        // per-block caches, only in the flat decorations vec during full render).
        for item in &self.flow_layout.flow_order {
            match item {
                FlowItem::Table { table_id, .. } => {
                    if let Some(table) = self.flow_layout.tables.get(table_id) {
                        let decos = crate::layout::table::generate_table_decorations(
                            table,
                            self.scroll_offset,
                        );
                        self.render_frame.decorations.extend(decos);
                    }
                }
                FlowItem::Frame { frame_id, .. } => {
                    if let Some(frame) = self.flow_layout.frames.get(frame_id) {
                        crate::render::frame::append_frame_border_decorations(
                            frame,
                            self.scroll_offset,
                            &mut self.render_frame.decorations,
                        );
                    }
                }
                FlowItem::Block { .. } => {}
            }
        }

        let effective_vw = self.viewport_width / self.zoom;
        let effective_vh = self.viewport_height / self.zoom;
        let cursor_decos = crate::render::cursor::generate_cursor_decorations(
            &self.flow_layout,
            &self.cursors,
            self.scroll_offset,
            self.cursor_color,
            self.selection_color,
            effective_vw,
            effective_vh,
        );
        self.render_frame.decorations.extend(cursor_decos);

        // Update atlas metadata
        self.render_frame.atlas_dirty = self.atlas.dirty;
        self.render_frame.atlas_width = self.atlas.width;
        self.render_frame.atlas_height = self.atlas.height;
        if self.atlas.dirty {
            let pixels = &self.atlas.pixels;
            let needed = (self.atlas.width * self.atlas.height * 4) as usize;
            self.render_frame.atlas_pixels.resize(needed, 0);
            let copy_len = needed.min(pixels.len());
            self.render_frame.atlas_pixels[..copy_len].copy_from_slice(&pixels[..copy_len]);
            self.atlas.dirty = false;
        }
    }

    /// Read the glyph atlas state without triggering the full document
    /// render pipeline. Advances the cache generation and runs eviction
    /// (to reclaim atlas space), but does NOT re-render document content.
    ///
    /// Returns `(dirty, width, height, pixels, glyphs_evicted)`.
    /// When `glyphs_evicted` is true, callers that cache glyph output
    /// (e.g. paint caches) must invalidate — evicted atlas space may be
    /// reused by future glyph allocations.
    pub fn atlas_snapshot(&mut self, advance_generation: bool) -> (bool, u32, u32, &[u8], bool) {
        // Only advance generation and run eviction when text work happened.
        // Skipping this on idle frames prevents aging out glyphs that are
        // still visible but not re-measured (paint cache reuse scenario).
        let mut glyphs_evicted = false;
        if advance_generation {
            self.glyph_cache.advance_generation();
            let evicted = self.glyph_cache.evict_unused();
            glyphs_evicted = !evicted.is_empty();
            for alloc_id in evicted {
                self.atlas.deallocate(alloc_id);
            }
        }

        let dirty = self.atlas.dirty;
        let w = self.atlas.width;
        let h = self.atlas.height;
        let pixels = &self.atlas.pixels[..];
        if dirty {
            self.atlas.dirty = false;
        }
        (dirty, w, h, pixels, glyphs_evicted)
    }

    // ── Single-line layout ───────────────────────────────────────

    /// Lay out a single line of text and return GPU-ready glyph quads.
    ///
    /// This is the fast path for simple labels, tooltips, overlays, and other
    /// single-line text that does not need the full document layout pipeline.
    ///
    /// What it does:
    /// - Resolves the font from `format` (family, weight, italic, size).
    /// - Shapes the text with rustybuzz (including glyph fallback).
    /// - Rasterizes glyphs into the atlas (same path as the full pipeline).
    /// - If `max_width` is provided and the text exceeds it, truncates with
    ///   an ellipsis character.
    ///
    /// What it skips:
    /// - Line breaking (there is only one line).
    /// - Bidi analysis (assumes a single direction run).
    /// - Flow layout, margins, indents, block stacking.
    ///
    /// Glyph quads are positioned with the top-left at (0, 0).
    pub fn layout_single_line(
        &mut self,
        text: &str,
        format: &TextFormat,
        max_width: Option<f32>,
    ) -> SingleLineResult {
        use crate::font::resolve::resolve_font;
        use crate::shaping::shaper::{bidi_runs, font_metrics_px, shape_text, shape_text_with_fallback};

        let empty = SingleLineResult {
            width: 0.0,
            height: 0.0,
            baseline: 0.0,
            glyphs: Vec::new(),
        };

        if text.is_empty() {
            return empty;
        }

        // Resolve font from TextFormat fields
        let font_point_size = format.font_size.map(|s| s as u32);
        let resolved = match resolve_font(
            &self.font_registry,
            format.font_family.as_deref(),
            format.font_weight,
            format.font_bold,
            format.font_italic,
            font_point_size,
        ) {
            Some(r) => r,
            None => return empty,
        };

        // Get font metrics for line height
        let metrics = match font_metrics_px(&self.font_registry, &resolved) {
            Some(m) => m,
            None => return empty,
        };
        let line_height = metrics.ascent + metrics.descent + metrics.leading;
        let baseline = metrics.ascent;

        // Shape the text, split into bidi runs in visual order.
        //
        // Each directional run is shaped with its own explicit direction
        // so rustybuzz cannot infer RTL from a strong Arabic/Hebrew char
        // and reverse an embedded Latin cluster (UAX #9, rule L2).
        //
        // Runs are already in visual order — concatenating their glyphs
        // left-to-right produces the correct visual line.
        let runs: Vec<_> = bidi_runs(text)
            .into_iter()
            .filter_map(|br| {
                let slice = text.get(br.byte_range.clone())?;
                shape_text_with_fallback(
                    &self.font_registry,
                    &resolved,
                    slice,
                    br.byte_range.start,
                    br.direction,
                )
            })
            .collect();

        if runs.is_empty() {
            return empty;
        }

        let total_advance: f32 = runs.iter().map(|r| r.advance_width).sum();

        // Determine which glyphs to render (truncation with ellipsis if needed).
        // Truncation operates on the visual-order glyph stream and cuts from
        // the visual-end (right side), matching the pre-bidi behavior for the
        // common single-direction case.
        let (truncate_at_visual_index, final_width, ellipsis_run) =
            if let Some(max_w) = max_width
                && total_advance > max_w
            {
                let ellipsis_run = shape_text(&self.font_registry, &resolved, "\u{2026}", 0);
                let ellipsis_width = ellipsis_run
                    .as_ref()
                    .map(|r| r.advance_width)
                    .unwrap_or(0.0);
                let budget = (max_w - ellipsis_width).max(0.0);

                let mut used = 0.0f32;
                let mut count = 0usize;
                'outer: for run in &runs {
                    for g in &run.glyphs {
                        if used + g.x_advance > budget {
                            break 'outer;
                        }
                        used += g.x_advance;
                        count += 1;
                    }
                }

                (Some(count), used + ellipsis_width, ellipsis_run)
            } else {
                (None, total_advance, None)
            };

        // Rasterize glyphs in visual order and build GlyphQuads
        let text_color = format.color.unwrap_or(self.text_color);
        let glyph_capacity: usize = runs.iter().map(|r| r.glyphs.len()).sum();
        let mut quads = Vec::with_capacity(glyph_capacity + 1);
        let mut pen_x = 0.0f32;
        let mut emitted = 0usize;

        'emit: for run in &runs {
            for glyph in &run.glyphs {
                if let Some(limit) = truncate_at_visual_index
                    && emitted >= limit
                {
                    break 'emit;
                }
                self.rasterize_glyph_quad(
                    glyph,
                    run,
                    pen_x,
                    baseline,
                    text_color,
                    &mut quads,
                );
                pen_x += glyph.x_advance;
                emitted += 1;
            }
        }

        // Render ellipsis glyphs if truncated
        if let Some(ref e_run) = ellipsis_run {
            for glyph in &e_run.glyphs {
                self.rasterize_glyph_quad(
                    glyph,
                    e_run,
                    pen_x,
                    baseline,
                    text_color,
                    &mut quads,
                );
                pen_x += glyph.x_advance;
            }
        }

        SingleLineResult {
            width: final_width,
            height: line_height,
            baseline,
            glyphs: quads,
        }
    }

    /// Rasterize a single glyph and append a GlyphQuad to the output vec.
    ///
    /// Shared helper for `layout_single_line`. Handles cache lookup,
    /// rasterization on miss, and atlas allocation.
    fn rasterize_glyph_quad(
        &mut self,
        glyph: &crate::shaping::run::ShapedGlyph,
        run: &crate::shaping::run::ShapedRun,
        pen_x: f32,
        baseline: f32,
        text_color: [f32; 4],
        quads: &mut Vec<GlyphQuad>,
    ) {
        if glyph.glyph_id == 0 {
            return;
        }

        let entry = match self.font_registry.get(glyph.font_face_id) {
            Some(e) => e,
            None => return,
        };

        let cache_key = GlyphCacheKey::new(glyph.font_face_id, glyph.glyph_id, run.size_px);

        // Ensure glyph is cached (rasterize on miss)
        if self.glyph_cache.peek(&cache_key).is_none()
            && let Some(image) = rasterize_glyph(
                &mut self.scale_context,
                &entry.data,
                entry.face_index,
                entry.swash_cache_key,
                glyph.glyph_id,
                run.size_px,
            )
            && image.width > 0
            && image.height > 0
            && let Some(alloc) = self.atlas.allocate(image.width, image.height)
        {
            let rect = alloc.rectangle;
            let atlas_x = rect.min.x as u32;
            let atlas_y = rect.min.y as u32;
            if image.is_color {
                self.atlas
                    .blit_rgba(atlas_x, atlas_y, image.width, image.height, &image.data);
            } else {
                self.atlas
                    .blit_mask(atlas_x, atlas_y, image.width, image.height, &image.data);
            }
            self.glyph_cache.insert(
                cache_key,
                crate::atlas::cache::CachedGlyph {
                    alloc_id: alloc.id,
                    atlas_x,
                    atlas_y,
                    width: image.width,
                    height: image.height,
                    placement_left: image.placement_left,
                    placement_top: image.placement_top,
                    is_color: image.is_color,
                    last_used: 0,
                },
            );
        }

        if let Some(cached) = self.glyph_cache.get(&cache_key) {
            let screen_x = pen_x + glyph.x_offset + cached.placement_left as f32;
            let screen_y = baseline - glyph.y_offset - cached.placement_top as f32;
            let color = if cached.is_color {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                text_color
            };
            quads.push(GlyphQuad {
                screen: [
                    screen_x,
                    screen_y,
                    cached.width as f32,
                    cached.height as f32,
                ],
                atlas: [
                    cached.atlas_x as f32,
                    cached.atlas_y as f32,
                    cached.width as f32,
                    cached.height as f32,
                ],
                color,
            });
        }
    }

    // ── Hit testing ─────────────────────────────────────────────

    /// Map a screen-space point to a document position.
    ///
    /// Coordinates are relative to the widget's top-left corner.
    /// The scroll offset is accounted for internally.
    /// Returns `None` if the flow has no content.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<HitTestResult> {
        crate::render::hit_test::hit_test(
            &self.flow_layout,
            self.scroll_offset,
            x / self.zoom,
            y / self.zoom,
        )
    }

    /// Get the screen-space caret rectangle at a document position.
    ///
    /// Returns `[x, y, width, height]` in screen pixels. Use this to report
    /// the caret position to the platform IME system for composition window
    /// placement. For drawing the caret, use the [`crate::DecorationKind::Cursor`]
    /// entry in [`crate::RenderFrame::decorations`] instead.
    pub fn caret_rect(&self, position: usize) -> [f32; 4] {
        let mut rect =
            crate::render::hit_test::caret_rect(&self.flow_layout, self.scroll_offset, position);
        rect[0] *= self.zoom;
        rect[1] *= self.zoom;
        rect[2] *= self.zoom;
        rect[3] *= self.zoom;
        rect
    }

    // ── Cursor display ──────────────────────────────────────────

    /// Update the cursor display state for a single cursor.
    ///
    /// The adapter reads `position` and `anchor` from text-document's
    /// `TextCursor`, toggles `visible` on a blink timer, and passes
    /// the result here. The typesetter includes cursor and selection
    /// decorations in the next [`render`](Self::render) call.
    pub fn set_cursor(&mut self, cursor: &CursorDisplay) {
        self.cursors = vec![CursorDisplay {
            position: cursor.position,
            anchor: cursor.anchor,
            visible: cursor.visible,
            selected_cells: cursor.selected_cells.clone(),
        }];
    }

    /// Update multiple cursors (multi-cursor editing support).
    ///
    /// Each cursor independently generates a caret and optional selection highlight.
    pub fn set_cursors(&mut self, cursors: &[CursorDisplay]) {
        self.cursors = cursors
            .iter()
            .map(|c| CursorDisplay {
                position: c.position,
                anchor: c.anchor,
                visible: c.visible,
                selected_cells: c.selected_cells.clone(),
            })
            .collect();
    }

    /// Set the selection highlight color (`[r, g, b, a]`, 0.0-1.0).
    ///
    /// Default: `[0.26, 0.52, 0.96, 0.3]` (translucent blue).
    pub fn set_selection_color(&mut self, color: [f32; 4]) {
        self.selection_color = color;
    }

    /// Set the cursor caret color (`[r, g, b, a]`, 0.0-1.0).
    ///
    /// Default: `[0.0, 0.0, 0.0, 1.0]` (black).
    pub fn set_cursor_color(&mut self, color: [f32; 4]) {
        self.cursor_color = color;
    }

    /// Set the default text color (`[r, g, b, a]`, 0.0-1.0).
    ///
    /// This color is used for glyphs and decorations (underline, strikeout, overline)
    /// when no per-fragment `foreground_color` is set.
    ///
    /// Default: `[0.0, 0.0, 0.0, 1.0]` (black).
    pub fn set_text_color(&mut self, color: [f32; 4]) {
        self.text_color = color;
    }

    // ── Scrolling ───────────────────────────────────────────────

    /// Get the visual position and height of a laid-out block.
    ///
    /// Returns `None` if the block ID is not in the current layout.
    pub fn block_visual_info(&self, block_id: usize) -> Option<BlockVisualInfo> {
        let block = self.flow_layout.blocks.get(&block_id)?;
        Some(BlockVisualInfo {
            block_id,
            y: block.y,
            height: block.height,
        })
    }

    /// Check whether a block belongs to a table cell.
    ///
    /// Returns `true` if `block_id` is found in any table cell layout,
    /// `false` if it is a top-level or frame block (or unknown).
    pub fn is_block_in_table(&self, block_id: usize) -> bool {
        self.flow_layout.tables.values().any(|table| {
            table
                .cell_layouts
                .iter()
                .any(|cell| cell.blocks.iter().any(|b| b.block_id == block_id))
        })
    }

    /// Scroll so that the given document position is visible, placing it
    /// roughly 1/3 from the top of the viewport.
    ///
    /// Returns the new scroll offset.
    pub fn scroll_to_position(&mut self, position: usize) -> f32 {
        let rect =
            crate::render::hit_test::caret_rect(&self.flow_layout, self.scroll_offset, position);
        let target_y = rect[1] + self.scroll_offset - self.viewport_height / (3.0 * self.zoom);
        self.scroll_offset = target_y.max(0.0);
        self.scroll_offset
    }

    /// Scroll the minimum amount needed to make the current caret visible.
    ///
    /// Call after cursor movement (arrow keys, click, typing) to keep
    /// the caret in view. Returns `Some(new_offset)` if scrolling occurred,
    /// or `None` if the caret was already visible.
    pub fn ensure_caret_visible(&mut self) -> Option<f32> {
        if self.cursors.is_empty() {
            return None;
        }
        let pos = self.cursors[0].position;
        // Work in 1x (document) coordinates so scroll_offset stays in document space
        let rect = crate::render::hit_test::caret_rect(&self.flow_layout, self.scroll_offset, pos);
        let caret_screen_y = rect[1];
        let caret_screen_bottom = caret_screen_y + rect[3];
        let effective_vh = self.viewport_height / self.zoom;
        let margin = 10.0 / self.zoom;
        let old_offset = self.scroll_offset;

        if caret_screen_y < 0.0 {
            self.scroll_offset += caret_screen_y - margin;
            self.scroll_offset = self.scroll_offset.max(0.0);
        } else if caret_screen_bottom > effective_vh {
            self.scroll_offset += caret_screen_bottom - effective_vh + margin;
        }

        if (self.scroll_offset - old_offset).abs() > 0.001 {
            Some(self.scroll_offset)
        } else {
            None
        }
    }
}

#[cfg(feature = "text-document")]
enum FlowItemKind {
    Block(crate::layout::block::BlockLayoutParams),
    Table(crate::layout::table::TableLayoutParams),
    Frame(crate::layout::frame::FrameLayoutParams),
}

/// Scale all screen-space coordinates in a RenderFrame by the zoom factor.
fn apply_zoom(frame: &mut RenderFrame, zoom: f32) {
    if (zoom - 1.0).abs() <= f32::EPSILON {
        return;
    }
    for q in &mut frame.glyphs {
        q.screen[0] *= zoom;
        q.screen[1] *= zoom;
        q.screen[2] *= zoom;
        q.screen[3] *= zoom;
    }
    for q in &mut frame.images {
        q.screen[0] *= zoom;
        q.screen[1] *= zoom;
        q.screen[2] *= zoom;
        q.screen[3] *= zoom;
    }
    apply_zoom_decorations(&mut frame.decorations, zoom);
}

/// Scale all screen-space coordinates in decoration rects by the zoom factor.
fn apply_zoom_decorations(decorations: &mut [crate::types::DecorationRect], zoom: f32) {
    if (zoom - 1.0).abs() <= f32::EPSILON {
        return;
    }
    for d in decorations.iter_mut() {
        d.rect[0] *= zoom;
        d.rect[1] *= zoom;
        d.rect[2] *= zoom;
        d.rect[3] *= zoom;
    }
}

impl Default for Typesetter {
    fn default() -> Self {
        Self::new()
    }
}
