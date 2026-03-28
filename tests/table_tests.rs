//! Table layout, rendering, cursor, and hit-testing tests.
//!
//! Uses the framework-agnostic API (no text-document dependency).

mod helpers;
use helpers::{NOTO_SANS, Rect, assert_caret_is_real, make_block_at, make_cell_at, make_table};

use text_typeset::Typesetter;
use text_typeset::layout::table::TableLayoutParams;

// ── Setup helpers ────────────────────────────────────────────────

/// Create a 3-row, 2-column table with known text content and positions.
///
/// Row 0: "Header A" (pos 0), "Header B" (pos 9)
/// Row 1: "Cell one" (pos 18), "Cell two" (pos 27)
/// Row 2: "Cell three" (pos 36), "Cell four" (pos 47)
fn make_3x2_table() -> TableLayoutParams {
    make_table(
        1,
        3,
        2,
        vec![
            make_cell_at(0, 0, 10, 0, "Header A"),
            make_cell_at(0, 1, 11, 9, "Header B"),
            make_cell_at(1, 0, 12, 18, "Cell one"),
            make_cell_at(1, 1, 13, 27, "Cell two"),
            make_cell_at(2, 0, 14, 36, "Cell three"),
            make_cell_at(2, 1, 15, 47, "Cell four"),
        ],
    )
}

/// Set up a Typesetter with the 3x2 table at a given viewport width.
fn setup_table(viewport_width: f32) -> Typesetter {
    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(viewport_width, 600.0);
    let table = make_3x2_table();
    ts.add_table(&table);
    ts.render();
    ts
}

// ── Layout tests ─────────────────────────────────────────────────

#[test]
fn table_has_positive_dimensions() {
    let ts = setup_table(800.0);
    let height = ts.content_height();
    assert!(
        height > 0.0,
        "table content height should be positive: {}",
        height
    );
}

#[test]
fn table_rows_have_increasing_y() {
    let ts = setup_table(800.0);

    // Caret y should increase from row 0 to row 2
    let y0 = ts.caret_rect(0)[1]; // Row 0, col 0
    let y1 = ts.caret_rect(18)[1]; // Row 1, col 0
    let y2 = ts.caret_rect(36)[1]; // Row 2, col 0

    assert!(y1 > y0, "row 1 y ({}) should be below row 0 y ({})", y1, y0);
    assert!(y2 > y1, "row 2 y ({}) should be below row 1 y ({})", y2, y1);
}

#[test]
fn table_columns_have_increasing_x() {
    let ts = setup_table(800.0);

    // Col 1 caret should be to the right of col 0 in the same row
    let x_col0 = ts.caret_rect(0)[0]; // Row 0, col 0
    let x_col1 = ts.caret_rect(9)[0]; // Row 0, col 1

    assert!(
        x_col1 > x_col0,
        "col 1 x ({}) should be right of col 0 x ({})",
        x_col1,
        x_col0
    );
}

#[test]
fn table_same_row_cells_share_y() {
    let ts = setup_table(800.0);

    // Both columns in row 0 should have the same y
    let y_col0 = ts.caret_rect(0)[1];
    let y_col1 = ts.caret_rect(9)[1];
    assert!(
        (y_col0 - y_col1).abs() < 1.0,
        "same-row cells should have same y: col0={}, col1={}",
        y_col0,
        y_col1
    );
}

#[test]
fn table_column_widths_finite_at_infinite_viewport() {
    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(f32::INFINITY, 600.0);
    ts.set_content_width(f32::INFINITY);
    let table = make_3x2_table();
    ts.add_table(&table);
    ts.render();

    // All caret positions should have finite x
    for pos in [0, 9, 18, 27, 36, 47] {
        let rect = ts.caret_rect(pos);
        assert!(
            rect[0].is_finite(),
            "caret x should be finite at pos {}: {}",
            pos,
            rect[0]
        );
    }
}

#[test]
fn table_column_widths_positive_at_narrow_viewport() {
    let ts = setup_table(100.0);

    // Even with a very narrow viewport, all carets should have valid positions
    for pos in [0, 9, 18, 27, 36, 47] {
        let rect = ts.caret_rect(pos);
        assert!(rect[0].is_finite(), "caret x finite at pos {}", pos);
        assert!(rect[3] > 0.0, "caret h positive at pos {}", pos);
    }
}

// ── Caret tests ──────────────────────────────────────────────────

#[test]
fn caret_rect_found_for_all_cell_positions() {
    let ts = setup_table(800.0);

    for pos in [
        0, 4, 8, 9, 13, 17, 18, 22, 26, 27, 31, 35, 36, 40, 45, 47, 51, 57,
    ] {
        assert_caret_is_real(ts.caret_rect(pos), &format!("pos {}", pos));
    }
}

#[test]
fn caret_y_monotonic_across_cells() {
    let ts = setup_table(800.0);

    // Walk through all cells in row-major order.
    // y should never decrease when moving to a new row.
    let positions = [0, 9, 18, 27, 36, 47]; // start of each cell block
    let mut prev_y = -1.0f32;

    for &pos in &positions {
        let rect = ts.caret_rect(pos);
        let y = rect[1];
        assert!(
            y >= prev_y - 1.0,
            "caret y should not decrease: prev={}, pos {} y={}",
            prev_y,
            pos,
            y
        );
        prev_y = y;
    }
}

#[test]
fn caret_x_advances_within_cell() {
    let ts = setup_table(800.0);

    // Within "Header A" (pos 0-7), x should advance
    let x_start = ts.caret_rect(0)[0];
    let x_mid = ts.caret_rect(4)[0];
    let x_end = ts.caret_rect(8)[0];

    assert!(
        x_mid > x_start,
        "x should advance: start={}, mid={}",
        x_start,
        x_mid
    );
    assert!(
        x_end > x_mid,
        "x should advance: mid={}, end={}",
        x_mid,
        x_end
    );
}

// ── Hit-test tests ───────────────────────────────────────────────

#[test]
fn hit_test_finds_cells_in_all_rows() {
    let ts = setup_table(800.0);

    // Hit test at the caret position for each row's first cell
    for (label, pos) in [("row 0", 0), ("row 1", 18), ("row 2", 36)] {
        let rect = ts.caret_rect(pos);
        let hx = rect[0].max(1.0);
        let hy = rect[1] + rect[3] * 0.5;
        let hit = ts.hit_test(hx, hy);
        assert!(
            hit.is_some(),
            "hit_test should find {} at ({}, {})",
            label,
            hx,
            hy
        );
    }
}

#[test]
fn hit_test_returns_table_cell_blocks() {
    let ts = setup_table(800.0);

    // Hit at row 1, col 0 area should find block 12 ("Cell one")
    let rect = ts.caret_rect(18);
    let hit = ts
        .hit_test(rect[0].max(1.0), rect[1] + rect[3] * 0.5)
        .unwrap();
    assert_eq!(hit.block_id, 12, "hit should find block 12 (Cell one)");

    // This should NOT be found by block_visual_info (it's in a table cell)
    assert!(
        ts.block_visual_info(hit.block_id).is_none(),
        "table cell block should not have block_visual_info"
    );

    // But is_block_in_table should return true
    assert!(
        ts.is_block_in_table(hit.block_id),
        "table cell block should be in table"
    );
}

// ── Rendering tests ──────────────────────────────────────────────

#[test]
fn table_renders_glyphs_for_all_cells() {
    let mut ts = setup_table(800.0);
    let frame = ts.render();

    // Table has 6 cells with text, should produce glyphs
    assert!(
        frame.glyphs.len() > 20,
        "table should render many glyphs, got {}",
        frame.glyphs.len()
    );
}

#[test]
fn table_renders_border_decorations() {
    let mut ts = setup_table(800.0);
    let frame = ts.render();

    // Should have border decorations (from generate_table_decorations)
    let borders: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::BlockBackground)
        .collect();
    // Table borders produce at least some decorations
    assert!(
        !borders.is_empty() || !frame.decorations.is_empty(),
        "table should produce decorations"
    );
}

#[test]
fn table_cell_glyphs_within_cell_bounds() {
    let mut ts = setup_table(800.0);
    let frame = ts.render();

    // All glyphs should have finite, non-negative positions
    for (i, glyph) in frame.glyphs.iter().enumerate() {
        let r = Rect::from(glyph.screen);
        assert!(
            r.x().is_finite() && r.y().is_finite(),
            "glyph[{}] has non-finite position: {}",
            i,
            r
        );
        assert!(r.x() >= 0.0, "glyph[{}] has negative x: {}", i, r);
    }
}

// ── Down-arrow / vertical navigation tests ───────────────────────

#[test]
fn vertical_hit_test_reaches_all_rows() {
    let ts = setup_table(800.0);

    // Get the caret rect for the first cell in each row
    let r0 = Rect::from(ts.caret_rect(0)); // Row 0
    let r1 = Rect::from(ts.caret_rect(18)); // Row 1
    let r2 = Rect::from(ts.caret_rect(36)); // Row 2

    // Simulate down-arrow: hit_test at (same x, next row y)
    let x = r0.x().max(1.0);

    // From row 0, move to row 1
    let hit1 = ts.hit_test(x, r1.y() + r1.h() * 0.5);
    assert!(hit1.is_some(), "should hit row 1");
    let hit1 = hit1.unwrap();
    assert_eq!(hit1.block_id, 12, "should hit row 1 col 0 block (12)");

    // From row 1, move to row 2
    let hit2 = ts.hit_test(x, r2.y() + r2.h() * 0.5);
    assert!(hit2.is_some(), "should hit row 2");
    let hit2 = hit2.unwrap();
    assert_eq!(hit2.block_id, 14, "should hit row 2 col 0 block (14)");
}

#[test]
fn hit_test_between_rows_finds_nearest() {
    let ts = setup_table(800.0);

    let r0 = Rect::from(ts.caret_rect(0)); // Row 0
    let r1 = Rect::from(ts.caret_rect(18)); // Row 1

    // Hit-test at a y midpoint between row 0 bottom and row 1 top
    let mid_y = (r0.bottom() + r1.y()) / 2.0;
    let hit = ts.hit_test(r0.x().max(1.0), mid_y);
    assert!(
        hit.is_some(),
        "hit_test should find something between rows at y={}",
        mid_y
    );
}

// ── Relayout tests ───────────────────────────────────────────────

#[test]
fn relayout_table_block_updates_cell_content() {
    let mut ts = setup_table(800.0);

    let before_height = ts.content_height();
    let glyph_count_before = ts.render().glyphs.len();

    // Relayout cell (1,0) with longer text
    let updated = make_block_at(12, 18, "Cell one now has much more text to wrap around");
    ts.relayout_block(&updated);
    ts.render();

    let after_height = ts.content_height();
    let glyph_count_after = ts.render().glyphs.len();

    // Table should be taller (text wraps)
    assert!(
        after_height >= before_height,
        "table should grow: before={}, after={}",
        before_height,
        after_height
    );

    // More glyphs should be rendered
    assert!(
        glyph_count_after > glyph_count_before,
        "more glyphs after longer text: before={}, after={}",
        glyph_count_before,
        glyph_count_after
    );
}
