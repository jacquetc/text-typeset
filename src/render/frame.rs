use crate::atlas::allocator::GlyphAtlas;
use crate::atlas::cache::{CachedGlyph, GlyphCache, GlyphCacheKey};
use crate::atlas::rasterizer::rasterize_glyph;
use crate::font::registry::FontRegistry;
use crate::layout::block::BlockLayout;
use crate::layout::flow::{FlowItem, FlowLayout};
use crate::layout::table::generate_table_decorations;
use crate::render::cursor::generate_cursor_decorations;
use crate::render::decoration::generate_block_decorations;
use crate::types::{CursorDisplay, GlyphQuad, ImageQuad, RenderFrame};

/// Build a RenderFrame from the current flow layout.
///
/// Iterates visible blocks (viewport culling), rasterizes uncached glyphs
/// into the atlas, and produces GlyphQuad entries for each visible glyph.
#[allow(clippy::too_many_arguments)]
pub fn build_render_frame(
    flow: &FlowLayout,
    registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    cache: &mut GlyphCache,
    scale_context: &mut swash::scale::ScaleContext,
    scroll_offset: f32,
    viewport_height: f32,
    cursors: &[CursorDisplay],
    cursor_color: [f32; 4],
    selection_color: [f32; 4],
    render_frame: &mut RenderFrame,
) {
    render_frame.glyphs.clear();
    render_frame.images.clear();
    render_frame.decorations.clear();
    render_frame.block_glyphs.clear();
    render_frame.block_decorations.clear();
    render_frame.block_images.clear();
    render_frame.block_heights.clear();

    // Advance generation and evict stale glyphs
    cache.advance_generation();
    let evicted = cache.evict_unused();
    for alloc_id in evicted {
        atlas.deallocate(alloc_id);
    }

    let view_top = scroll_offset;
    let view_bottom = scroll_offset + viewport_height;

    for item in &flow.flow_order {
        let (block_id, item_y, item_height) = match item {
            FlowItem::Block {
                block_id,
                y,
                height,
            } => (*block_id, *y, *height),
            FlowItem::Table {
                table_id,
                y,
                height,
            } => {
                if *y + *height < view_top || *y > view_bottom {
                    continue;
                }
                if let Some(table) = flow.tables.get(table_id) {
                    render_table_cells(
                        table,
                        0.0,
                        0.0,
                        registry,
                        atlas,
                        cache,
                        scale_context,
                        scroll_offset,
                        viewport_height,
                        render_frame,
                    );
                    let decos = generate_table_decorations(table, scroll_offset);
                    render_frame.decorations.extend(decos);
                }
                continue;
            }
            FlowItem::Frame {
                frame_id,
                y,
                height,
            } => {
                if *y + *height < view_top || *y > view_bottom {
                    continue;
                }
                if let Some(frame_layout) = flow.frames.get(frame_id) {
                    render_frame_layout(
                        frame_layout,
                        registry,
                        atlas,
                        cache,
                        scale_context,
                        scroll_offset,
                        viewport_height,
                        render_frame,
                    );
                }
                continue;
            }
        };

        // Viewport culling
        if item_y + item_height < view_top || item_y > view_bottom {
            continue;
        }

        if let Some(block) = flow.blocks.get(&block_id) {
            // Capture per-block glyphs and images
            let g_start = render_frame.glyphs.len();
            let i_start = render_frame.images.len();
            render_block_at_offset(
                block,
                0.0,
                0.0,
                registry,
                atlas,
                cache,
                scale_context,
                scroll_offset,
                viewport_height,
                render_frame,
            );
            let block_g: Vec<GlyphQuad> = render_frame.glyphs[g_start..].to_vec();
            let block_i: Vec<ImageQuad> = render_frame.images[i_start..].to_vec();
            render_frame.block_glyphs.push((block_id, block_g));
            render_frame.block_images.push((block_id, block_i));

            let decos = generate_block_decorations(
                block,
                registry,
                scroll_offset,
                viewport_height,
                0.0,
                0.0,
                flow.viewport_width,
            );
            render_frame
                .block_decorations
                .push((block_id, decos.clone()));
            render_frame.decorations.extend(decos);

            // Snapshot block height for incremental render height-change detection
            render_frame.block_heights.insert(block_id, block.height);
        }
    }

    // Generate cursor and selection decorations
    let cursor_decos =
        generate_cursor_decorations(flow, cursors, scroll_offset, cursor_color, selection_color);
    render_frame.decorations.extend(cursor_decos);

    // Update atlas metadata in the render frame
    render_frame.atlas_dirty = atlas.dirty;
    render_frame.atlas_width = atlas.width;
    render_frame.atlas_height = atlas.height;
    // Reuse existing allocation when capacity is sufficient
    if atlas.dirty {
        render_frame.atlas_pixels.clone_from(&atlas.pixels);
        atlas.dirty = false;
    }
}

/// Render a block's glyphs at the given offset.
///
/// Handles list markers and all lines. The offset is (0, 0) for top-level
/// blocks, and non-zero for blocks inside table cells or frames.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_block_at_offset(
    block: &BlockLayout,
    offset_x: f32,
    offset_y: f32,
    registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    cache: &mut GlyphCache,
    scale_context: &mut swash::scale::ScaleContext,
    scroll_offset: f32,
    viewport_height: f32,
    render_frame: &mut RenderFrame,
) {
    // Render list marker on the first line (if present)
    if let Some(marker) = &block.list_marker
        && let Some(first_line) = block.lines.first()
    {
        let baseline_y = offset_y + block.y + first_line.y;
        let screen_top = baseline_y - first_line.ascent - scroll_offset;
        if screen_top + first_line.line_height >= 0.0 && screen_top <= viewport_height {
            render_run_glyphs(
                &marker.run,
                offset_x + marker.x,
                baseline_y,
                registry,
                atlas,
                cache,
                scale_context,
                scroll_offset,
                render_frame,
            );
        }
    }

    for line in &block.lines {
        let line_y = offset_y + block.y + line.y;
        // Line-level viewport culling
        let screen_top = line_y - line.ascent - scroll_offset;
        if screen_top + line.line_height < 0.0 {
            continue; // above viewport
        }
        if screen_top > viewport_height {
            break; // below viewport, and lines are ordered top-to-bottom
        }

        for positioned_run in &line.runs {
            let pen_x = offset_x + block.left_margin + positioned_run.x;
            // Adjust baseline for superscript/subscript
            let baseline_offset = match positioned_run.decorations.vertical_alignment {
                crate::types::VerticalAlignment::SuperScript => {
                    -(positioned_run.shaped_run.size_px * 0.35)
                }
                crate::types::VerticalAlignment::SubScript => {
                    positioned_run.shaped_run.size_px * 0.2
                }
                crate::types::VerticalAlignment::Normal => 0.0,
            };
            render_run_glyphs(
                &positioned_run.shaped_run,
                pen_x,
                line_y + baseline_offset,
                registry,
                atlas,
                cache,
                scale_context,
                scroll_offset,
                render_frame,
            );
        }
    }
}

/// Render all cells in a table at the given offset.
///
/// The offset is (0, 0) for top-level tables, and non-zero for tables
/// inside frames.
#[allow(clippy::too_many_arguments)]
fn render_table_cells(
    table: &crate::layout::table::TableLayout,
    offset_x: f32,
    offset_y: f32,
    registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    cache: &mut GlyphCache,
    scale_context: &mut swash::scale::ScaleContext,
    scroll_offset: f32,
    viewport_height: f32,
    render_frame: &mut RenderFrame,
) {
    for cell in &table.cell_layouts {
        if cell.row >= table.row_ys.len() || cell.column >= table.column_xs.len() {
            continue;
        }
        let cell_x = offset_x + table.column_xs[cell.column];
        let cell_y = offset_y + table.y + table.row_ys[cell.row];

        for block in &cell.blocks {
            render_block_at_offset(
                block,
                cell_x,
                cell_y,
                registry,
                atlas,
                cache,
                scale_context,
                scroll_offset,
                viewport_height,
                render_frame,
            );
            let cell_w = table
                .column_content_widths
                .get(cell.column)
                .copied()
                .unwrap_or(200.0);
            let decos = generate_block_decorations(
                block,
                registry,
                scroll_offset,
                viewport_height,
                cell_x,
                cell_y,
                cell_w,
            );
            render_frame.decorations.extend(decos);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_frame_layout(
    frame: &crate::layout::frame::FrameLayout,
    registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    cache: &mut GlyphCache,
    scale_context: &mut swash::scale::ScaleContext,
    scroll_offset: f32,
    viewport_height: f32,
    render_frame: &mut RenderFrame,
) {
    let offset_x = frame.x + frame.content_x;
    let offset_y = frame.y + frame.content_y;

    // Render nested blocks
    for block in &frame.blocks {
        render_block_at_offset(
            block,
            offset_x,
            offset_y,
            registry,
            atlas,
            cache,
            scale_context,
            scroll_offset,
            viewport_height,
            render_frame,
        );
        let decos = generate_block_decorations(
            block,
            registry,
            scroll_offset,
            viewport_height,
            offset_x,
            offset_y,
            frame.content_width,
        );
        render_frame.decorations.extend(decos);
    }

    // Render nested tables
    for table in &frame.tables {
        render_table_cells(
            table,
            offset_x,
            offset_y,
            registry,
            atlas,
            cache,
            scale_context,
            scroll_offset,
            viewport_height,
            render_frame,
        );
    }

    // Frame border decorations
    if frame.border_width > 0.0
        && frame.border_style != crate::layout::frame::FrameBorderStyle::None
    {
        let bw = frame.border_width;
        let fx = frame.x;
        let fy = frame.y - scroll_offset;
        let fw = frame.total_width;
        let fh = frame.total_height;
        let color = match frame.border_style {
            crate::layout::frame::FrameBorderStyle::LeftOnly => [0.5, 0.5, 0.6, 1.0],
            _ => [0.6, 0.6, 0.6, 1.0],
        };

        match frame.border_style {
            crate::layout::frame::FrameBorderStyle::LeftOnly => {
                // Blockquote style: left border only
                render_frame.decorations.push(crate::types::DecorationRect {
                    rect: [fx, fy, bw, fh],
                    color,
                    kind: crate::types::DecorationKind::Background,
                });
            }
            crate::layout::frame::FrameBorderStyle::Full => {
                // Top
                render_frame.decorations.push(crate::types::DecorationRect {
                    rect: [fx, fy, fw, bw],
                    color,
                    kind: crate::types::DecorationKind::Background,
                });
                // Bottom
                render_frame.decorations.push(crate::types::DecorationRect {
                    rect: [fx, fy + fh - bw, fw, bw],
                    color,
                    kind: crate::types::DecorationKind::Background,
                });
                // Left
                render_frame.decorations.push(crate::types::DecorationRect {
                    rect: [fx, fy, bw, fh],
                    color,
                    kind: crate::types::DecorationKind::Background,
                });
                // Right
                render_frame.decorations.push(crate::types::DecorationRect {
                    rect: [fx + fw - bw, fy, bw, fh],
                    color,
                    kind: crate::types::DecorationKind::Background,
                });
            }
            crate::layout::frame::FrameBorderStyle::None => {}
        }
    }
}

/// Render all glyphs in a shaped run at the given position.
///
/// Uses each glyph's `font_face_id` (which may differ from the run's
/// if glyph fallback replaced a .notdef with a glyph from another font).
#[allow(clippy::too_many_arguments)]
fn render_run_glyphs(
    run: &crate::shaping::run::ShapedRun,
    start_x: f32,
    baseline_y: f32,
    registry: &FontRegistry,
    atlas: &mut GlyphAtlas,
    cache: &mut GlyphCache,
    scale_context: &mut swash::scale::ScaleContext,
    scroll_offset: f32,
    render_frame: &mut RenderFrame,
) {
    let mut pen_x = start_x;
    for glyph in &run.glyphs {
        if glyph.glyph_id == 0 {
            pen_x += glyph.x_advance;
            continue;
        }

        // Use the glyph's own font_face_id (may be a fallback font)
        let entry = match registry.get(glyph.font_face_id) {
            Some(e) => e,
            None => {
                pen_x += glyph.x_advance;
                continue;
            }
        };

        let cache_key = GlyphCacheKey::new(glyph.font_face_id, glyph.glyph_id, run.size_px);
        ensure_glyph_cached(
            &cache_key,
            cache,
            atlas,
            scale_context,
            &entry.data,
            entry.face_index,
            entry.swash_cache_key,
            run.size_px,
        );
        if let Some(cached) = cache.get(&cache_key) {
            let screen_x = pen_x + glyph.x_offset + cached.placement_left as f32;
            let screen_y =
                baseline_y - glyph.y_offset - cached.placement_top as f32 - scroll_offset;
            let color = if cached.is_color {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                run.foreground_color.unwrap_or([0.0, 0.0, 0.0, 1.0])
            };
            render_frame.glyphs.push(GlyphQuad {
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
        pen_x += glyph.x_advance;
    }
}

/// Ensure a glyph is in the cache and atlas. Rasterizes on cache miss.
#[allow(clippy::too_many_arguments)]
fn ensure_glyph_cached(
    key: &GlyphCacheKey,
    cache: &mut GlyphCache,
    atlas: &mut GlyphAtlas,
    scale_context: &mut swash::scale::ScaleContext,
    font_data: &[u8],
    face_index: u32,
    swash_cache_key: swash::CacheKey,
    size_px: f32,
) {
    if cache.peek(key).is_some() {
        return;
    }

    let image = match rasterize_glyph(
        scale_context,
        font_data,
        face_index,
        swash_cache_key,
        key.glyph_id,
        size_px,
    ) {
        Some(img) => img,
        None => return,
    };

    if image.width == 0 || image.height == 0 {
        return;
    }

    let alloc = match atlas.allocate(image.width, image.height) {
        Some(a) => a,
        None => return,
    };
    let rect = alloc.rectangle;
    let atlas_x = rect.min.x as u32;
    let atlas_y = rect.min.y as u32;

    if image.is_color {
        atlas.blit_rgba(atlas_x, atlas_y, image.width, image.height, &image.data);
    } else {
        atlas.blit_mask(atlas_x, atlas_y, image.width, image.height, &image.data);
    }

    cache.insert(
        *key,
        CachedGlyph {
            alloc_id: alloc.id,
            atlas_x,
            atlas_y,
            width: image.width,
            height: image.height,
            placement_left: image.placement_left,
            placement_top: image.placement_top,
            is_color: image.is_color,
            last_used: 0, // will be set by insert()
        },
    );
}
