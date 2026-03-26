use text_typeset::layout::block::{BlockLayoutParams, FragmentParams};
use text_typeset::layout::paragraph::Alignment;
use text_typeset::{HitRegion, Typesetter};

const NOTO_SANS: &[u8] = include_bytes!("../test-fonts/NotoSans-Variable.ttf");

fn setup() -> Typesetter {
    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(800.0, 600.0);
    ts
}

fn make_block(id: usize, text: &str) -> BlockLayoutParams {
    BlockLayoutParams {
        block_id: id,
        position: 0,
        text: text.to_string(),
        fragments: vec![FragmentParams {
            text: text.to_string(),
            offset: 0,
            length: text.len(),
            font_family: None,
            font_weight: None,
            font_bold: None,
            font_italic: None,
            font_point_size: None,
            underline: false,
            overline: false,
            strikeout: false,
            is_link: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
        }],
        alignment: Alignment::Left,
        top_margin: 0.0,
        bottom_margin: 0.0,
        left_margin: 0.0,
        right_margin: 0.0,
        text_indent: 0.0,
        list_marker: String::new(),
        list_indent: 0.0,
        tab_positions: vec![],
        line_height_multiplier: None,
        non_breakable_lines: false,
        checkbox: None,
        background_color: None,
    }
}

#[test]
fn hit_test_on_text_returns_some() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello world")]);
    ts.render(); // ensure layout is computed

    // Hit somewhere in the middle of the text
    let result = ts.hit_test(40.0, 10.0);
    assert!(result.is_some(), "hit test on text should return Some");
}

#[test]
fn hit_test_returns_correct_block_id() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "First"), make_block(2, "Second")]);

    // First block should be near y=0
    let r1 = ts.hit_test(10.0, 5.0);
    assert!(r1.is_some());
    assert_eq!(r1.unwrap().block_id, 1);

    // Second block should be below the first
    let height = ts.content_height();
    let r2 = ts.hit_test(10.0, height - 5.0);
    assert!(r2.is_some());
    assert_eq!(r2.unwrap().block_id, 2);
}

#[test]
fn hit_test_position_increases_left_to_right() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "ABCDEFGHIJ")]);

    let r_left = ts.hit_test(5.0, 10.0);
    let r_mid = ts.hit_test(60.0, 10.0);
    let r_right = ts.hit_test(120.0, 10.0);

    assert!(r_left.is_some());
    assert!(r_mid.is_some());
    assert!(r_right.is_some());

    let pos_left = r_left.unwrap().position;
    let pos_mid = r_mid.unwrap().position;
    let pos_right = r_right.unwrap().position;

    assert!(
        pos_left <= pos_mid && pos_mid <= pos_right,
        "positions should increase left-to-right: {} <= {} <= {}",
        pos_left,
        pos_mid,
        pos_right
    );
}

#[test]
fn hit_test_past_line_end() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hi")]);

    // Far to the right of a short line
    let result = ts.hit_test(700.0, 10.0);
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(
        matches!(result.region, HitRegion::PastLineEnd),
        "far right should be PastLineEnd, got {:?}",
        std::mem::discriminant(&result.region)
    );
}

#[test]
fn hit_test_left_margin_region() {
    let mut ts = setup();
    let mut block = make_block(1, "Hello");
    block.left_margin = 50.0;
    ts.layout_blocks(vec![block]);

    // Click in the left margin area
    let result = ts.hit_test(10.0, 10.0);
    assert!(result.is_some());
    assert!(
        matches!(result.unwrap().region, HitRegion::LeftMargin),
        "click in left margin should return LeftMargin region"
    );
}

#[test]
fn caret_rect_at_position_zero() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);

    let rect = ts.caret_rect(0);
    // Caret at position 0 should be near x=0
    assert!(
        rect[0] < 10.0,
        "caret x at pos 0 should be near 0, got {}",
        rect[0]
    );
    assert!(rect[2] > 0.0, "caret should have positive width");
    assert!(rect[3] > 0.0, "caret should have positive height");
}

#[test]
fn caret_rect_moves_right_with_position() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "ABCDEF")]);

    let rect0 = ts.caret_rect(0);
    let rect3 = ts.caret_rect(3);
    let rect6 = ts.caret_rect(6);

    assert!(
        rect3[0] > rect0[0],
        "caret at pos 3 ({}) should be right of pos 0 ({})",
        rect3[0],
        rect0[0]
    );
    assert!(
        rect6[0] > rect3[0],
        "caret at pos 6 ({}) should be right of pos 3 ({})",
        rect6[0],
        rect3[0]
    );
}

#[test]
fn caret_rect_height_matches_line_height() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);

    let rect = ts.caret_rect(0);
    // Caret height should be roughly one line height (ascent + descent + leading)
    // For 16px Noto Sans, line height is ~20px
    assert!(
        rect[3] > 10.0 && rect[3] < 40.0,
        "caret height {} should be reasonable for 16px font",
        rect[3]
    );
}

#[test]
fn hit_test_with_scroll_offset() {
    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(800.0, 600.0);

    let blocks: Vec<_> = (0..10)
        .map(|i| {
            let mut b = make_block(i, &format!("Paragraph {i} text."));
            b.position = i * 20; // give each block a different document position
            b
        })
        .collect();
    ts.layout_blocks(blocks);

    // Without scroll, hit at y=5 should be in block 0
    let r0 = ts.hit_test(10.0, 5.0);
    assert!(r0.is_some());
    assert_eq!(r0.unwrap().block_id, 0);

    // With scroll offset, the same screen y maps to a different document position
    ts.set_scroll_offset(100.0);
    let r_scrolled = ts.hit_test(10.0, 5.0);
    assert!(r_scrolled.is_some());
    // Should be a different block (further down in the document)
    assert_ne!(
        r_scrolled.unwrap().block_id,
        0,
        "scrolled hit test should be in a different block"
    );
}

// ── Cursor and selection tests ──────────────────────────────────

#[test]
fn cursor_produces_caret_decoration() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello world")]);
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 5,
        anchor: 5,
        visible: true,
    });
    let frame = ts.render();

    let carets: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Cursor)
        .collect();
    assert_eq!(carets.len(), 1, "should have exactly one cursor caret");
    assert!(carets[0].rect[2] > 0.0, "caret should have positive width");
    assert!(carets[0].rect[3] > 0.0, "caret should have positive height");
}

#[test]
fn invisible_cursor_produces_no_caret() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 3,
        anchor: 3,
        visible: false, // blink off
    });
    let frame = ts.render();

    let carets: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Cursor)
        .collect();
    assert!(
        carets.is_empty(),
        "invisible cursor should produce no caret"
    );
}

#[test]
fn selection_produces_highlight_rects() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello world")]);
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 0,
        anchor: 5, // select "Hello"
        visible: true,
    });
    let frame = ts.render();

    let selections: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Selection)
        .collect();
    assert!(
        !selections.is_empty(),
        "selection should produce highlight rects"
    );
    // Selection should have positive width and height
    for sel in &selections {
        assert!(sel.rect[2] > 0.0, "selection width should be positive");
        assert!(sel.rect[3] > 0.0, "selection height should be positive");
    }
}

#[test]
fn no_selection_when_anchor_equals_position() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 3,
        anchor: 3, // no selection
        visible: true,
    });
    let frame = ts.render();

    let selections: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Selection)
        .collect();
    assert!(
        selections.is_empty(),
        "no selection when anchor == position"
    );
}

#[test]
fn multi_line_selection_extends_to_viewport_width() {
    let mut ts = setup(); // 800x600
    ts.layout_blocks(vec![
        make_block(1, "Short line."),
        make_block(2, "Another line."),
    ]);
    // Select across both blocks (position 0 in block 1 to end of block 2)
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 0,
        anchor: 24, // past both blocks
        visible: true,
    });
    let frame = ts.render();

    let selections: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Selection)
        .collect();

    assert!(
        selections.len() >= 2,
        "multi-block selection should produce at least 2 selection rects, got {}",
        selections.len()
    );

    // The first selection rect (first line) should extend to viewport width (800)
    // because the selection continues to the next line
    let first_sel = &selections[0];
    let sel_right_edge = first_sel.rect[0] + first_sel.rect[2];
    assert!(
        sel_right_edge > 700.0,
        "first line selection should extend to near viewport width (800), got right edge at {}",
        sel_right_edge
    );
}

#[test]
fn single_line_selection_does_not_extend() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello world, this is text.")]);
    // Select just "world" — single line, doesn't continue to next line
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 6,
        anchor: 11,
        visible: true,
    });
    let frame = ts.render();

    let selections: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Selection)
        .collect();

    assert!(!selections.is_empty());
    let sel = &selections[0];
    let sel_width = sel.rect[2];
    // "world" is ~5 characters wide at 16px ≈ 40-50px, NOT 800px
    assert!(
        sel_width < 200.0,
        "single-line selection should NOT extend to viewport width, got width {}",
        sel_width
    );
}

#[test]
fn multiple_cursors() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "ABCDEFGHIJ")]);
    ts.set_cursors(&[
        text_typeset::CursorDisplay {
            position: 2,
            anchor: 2,
            visible: true,
        },
        text_typeset::CursorDisplay {
            position: 7,
            anchor: 7,
            visible: true,
        },
    ]);
    let frame = ts.render();

    let carets: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Cursor)
        .collect();
    assert_eq!(carets.len(), 2, "should have two cursor carets");

    // Second caret should be to the right of the first
    assert!(
        carets[1].rect[0] > carets[0].rect[0],
        "second caret x ({}) should be > first caret x ({})",
        carets[1].rect[0],
        carets[0].rect[0]
    );
}

// ── Scrolling tests ─────────────────────────────────────────────

#[test]
fn block_visual_info_returns_data() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);

    let info = ts.block_visual_info(1);
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.block_id, 1);
    assert!(info.height > 0.0);
}

#[test]
fn block_visual_info_nonexistent_returns_none() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);
    assert!(ts.block_visual_info(999).is_none());
}

#[test]
fn ensure_caret_visible_when_already_visible() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 0,
        anchor: 0,
        visible: true,
    });

    // Content fits in viewport — caret is already visible
    let result = ts.ensure_caret_visible();
    assert!(
        result.is_none(),
        "caret should already be visible in a large viewport"
    );
}

#[test]
fn ensure_caret_visible_scrolls_down_when_needed() {
    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(800.0, 50.0); // very short viewport

    // Create many blocks
    let blocks: Vec<_> = (0..20)
        .map(|i| {
            let mut b = make_block(i, &format!("Paragraph {i}."));
            b.position = i * 20;
            b
        })
        .collect();
    ts.layout_blocks(blocks);

    // Put cursor at a position that's below the viewport
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 300, // deep in the document
        anchor: 300,
        visible: true,
    });

    let result = ts.ensure_caret_visible();
    assert!(
        result.is_some(),
        "should need to scroll to make caret at position 300 visible"
    );
    assert!(
        result.unwrap() > 0.0,
        "scroll offset should be positive to reveal the caret"
    );
}

#[test]
fn scroll_to_position_changes_offset() {
    let mut ts = setup();
    let blocks: Vec<_> = (0..10)
        .map(|i| {
            let mut b = make_block(i, &format!("Paragraph {i}."));
            b.position = i * 20;
            b
        })
        .collect();
    ts.layout_blocks(blocks);

    let offset = ts.scroll_to_position(100);
    assert!(
        offset >= 0.0,
        "scroll_to_position should return non-negative offset"
    );
}

// ── Coverage: hit_test edge cases ───────────────────────────────

#[test]
fn hit_test_below_all_content() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Short")]);
    // Click far below the single-line content
    let result = ts.hit_test(10.0, 500.0);
    assert!(
        result.is_some(),
        "below-content hit test should still return a result"
    );
    let result = result.unwrap();
    assert!(
        matches!(
            result.region,
            HitRegion::BelowContent | HitRegion::PastLineEnd | HitRegion::Text
        ),
        "should be below content or past line end"
    );
}

#[test]
fn caret_rect_at_end_of_document() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(1, "Hello")]);
    // Position past the last character
    let rect = ts.caret_rect(5);
    assert!(
        rect[2] > 0.0 && rect[3] > 0.0,
        "caret at end should have size"
    );
}

#[test]
fn caret_rect_with_no_layout() {
    let ts = setup();
    // No layout done — should return fallback
    let rect = ts.caret_rect(0);
    assert!(rect[3] > 0.0, "fallback caret should have positive height");
}

#[test]
fn hit_test_between_blocks_below_content() {
    let mut ts = setup();
    let mut block = make_block(1, "Short");
    block.top_margin = 0.0;
    block.bottom_margin = 100.0; // large gap after block
    ts.layout_blocks(vec![block]);
    // Click in the gap below the block's content but within its margin
    let result = ts.hit_test(10.0, 50.0);
    assert!(result.is_some());
}

#[test]
fn hit_test_link_region_detected() {
    let mut ts = setup();
    // Create a block where the text is marked as a link
    let block = BlockLayoutParams {
        block_id: 1,
        position: 0,
        text: "Click here".to_string(),
        fragments: vec![FragmentParams {
            text: "Click here".to_string(),
            offset: 0,
            length: 10,
            font_family: None,
            font_weight: None,
            font_bold: None,
            font_italic: None,
            font_point_size: None,
            underline: true,
            overline: false,
            strikeout: false,
            is_link: true,
            letter_spacing: 0.0,
            word_spacing: 0.0,
        }],
        alignment: Alignment::Left,
        top_margin: 0.0,
        bottom_margin: 0.0,
        left_margin: 0.0,
        right_margin: 0.0,
        text_indent: 0.0,
        list_marker: String::new(),
        list_indent: 0.0,
        tab_positions: vec![],
        line_height_multiplier: None,
        non_breakable_lines: false,
        checkbox: None,
        background_color: None,
    };
    ts.layout_blocks(vec![block]);

    // Hit test on the link text
    let result = ts.hit_test(30.0, 10.0);
    assert!(result.is_some());
    match result.unwrap().region {
        HitRegion::Link { .. } => {} // expected
        other => panic!("expected Link region, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn caret_rect_on_second_line() {
    let mut ts = setup();
    ts.layout_blocks(vec![make_block(
        1,
        "First line that wraps to a second line at narrow width.",
    )]);
    ts.set_viewport(150.0, 600.0);
    ts.layout_blocks(vec![make_block(
        1,
        "First line that wraps to a second line at narrow width.",
    )]);

    // Position somewhere on the second line
    let rect_line1 = ts.caret_rect(0);
    let rect_line2 = ts.caret_rect(30); // should be on a different line
    // Line 2 caret should be below line 1 caret
    assert!(
        rect_line2[1] > rect_line1[1],
        "caret on second line ({}) should be below first line ({})",
        rect_line2[1],
        rect_line1[1]
    );
}
