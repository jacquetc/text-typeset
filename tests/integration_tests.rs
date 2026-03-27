//! Functional tests using the real text-document API.
//! These tests verify the full pipeline: TextDocument -> FlowSnapshot -> Typesetter -> RenderFrame.

use text_document::TextDocument;
use text_typeset::Typesetter;

const NOTO_SANS: &[u8] = include_bytes!("../test-fonts/NotoSans-Variable.ttf");

fn setup_typesetter() -> Typesetter {
    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(800.0, 600.0);
    ts
}

#[test]
fn plain_text_document_renders_glyphs() {
    let doc = TextDocument::new();
    doc.set_plain_text("Hello, world!").unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    assert!(
        !frame.glyphs.is_empty(),
        "plain text document should produce glyph quads"
    );
    assert!(frame.atlas_dirty);
    assert!(frame.atlas_width > 0);
}

#[test]
fn html_document_renders_glyphs() {
    let doc = TextDocument::new();
    let op = doc
        .set_html("<p>Bold <b>text</b> and <i>italic</i>.</p>")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    assert!(
        !frame.glyphs.is_empty(),
        "HTML document should produce glyph quads"
    );
}

#[test]
fn markdown_document_renders() {
    let doc = TextDocument::new();
    let op = doc
        .set_markdown("# Heading\n\nParagraph text.\n\n- Item one\n- Item two")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    assert!(
        !frame.glyphs.is_empty(),
        "Markdown document should produce glyph quads"
    );
    assert!(
        ts.content_height() > 0.0,
        "document should have positive content height"
    );
}

#[test]
fn multi_paragraph_document_has_increasing_y() {
    let doc = TextDocument::new();
    doc.set_plain_text("First paragraph.\n\nSecond paragraph.\n\nThird paragraph.")
        .unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    // Collect unique y positions
    let mut ys: Vec<f32> = frame.glyphs.iter().map(|g| g.screen[1]).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);

    assert!(
        ys.len() >= 3,
        "3 paragraphs should produce glyphs at 3+ distinct y positions, got {}",
        ys.len()
    );
}

#[test]
fn hit_test_on_document_returns_valid_position() {
    let doc = TextDocument::new();
    doc.set_plain_text("Hello world").unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let _ = ts.render();

    let result = ts.hit_test(40.0, 10.0);
    assert!(result.is_some(), "hit test should return a result");

    let result = result.unwrap();
    assert!(
        result.position <= 11, // "Hello world" = 11 chars
        "hit test position {} should be within document bounds",
        result.position
    );
}

#[test]
fn cursor_and_selection_on_document() {
    let doc = TextDocument::new();
    doc.set_plain_text("Select this text.").unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);

    // Set a selection spanning "this"
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 7,
        anchor: 11,
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

    let carets: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Cursor)
        .collect();
    assert_eq!(carets.len(), 1, "should have one caret");
}

#[test]
fn html_with_formatting_renders_decorations() {
    let doc = TextDocument::new();
    let op = doc
        .set_html("<p><u>Underlined</u> and <s>strikethrough</s> text.</p>")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    let underlines: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::Underline)
        .collect();
    assert!(
        !underlines.is_empty(),
        "HTML <u> tag should produce Underline decorations"
    );
}

#[test]
fn document_with_table_renders() {
    let doc = TextDocument::new();
    let cursor = doc.cursor();
    cursor.insert_table(2, 2).unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    let borders: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::TableBorder)
        .collect();
    assert!(
        !borders.is_empty(),
        "document with table should produce border decorations"
    );
}

#[test]
fn incremental_update_after_edit() {
    let doc = TextDocument::new();
    doc.set_plain_text("Short.").unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame1 = ts.render();
    let count1 = frame1.glyphs.len();

    // Edit the document
    let cursor = doc.cursor();
    cursor.set_position(6, text_document::MoveMode::MoveAnchor);
    cursor.insert_text(" More text here.").unwrap();

    // Relayout with new snapshot
    let flow2 = doc.snapshot_flow();
    ts.layout_full(&flow2);
    let frame2 = ts.render();
    let count2 = frame2.glyphs.len();

    assert!(
        count2 > count1,
        "adding text should produce more glyphs: {} -> {}",
        count1,
        count2
    );
}

#[test]
fn text_then_table_then_text_renders_all() {
    let doc = TextDocument::new();
    doc.set_plain_text("Before table.").unwrap();

    // Insert table after the text
    let cursor = doc.cursor();
    cursor.move_position(
        text_document::MoveOperation::End,
        text_document::MoveMode::MoveAnchor,
        1,
    );
    cursor.insert_block().unwrap();
    cursor.insert_table(1, 2).unwrap();
    cursor.move_position(
        text_document::MoveOperation::End,
        text_document::MoveMode::MoveAnchor,
        1,
    );
    cursor.insert_block().unwrap();
    cursor.insert_text("After table.").unwrap();

    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    // Should have glyphs from both text blocks AND table borders
    assert!(
        frame.glyphs.len() >= 20,
        "mixed block+table+block should produce many glyphs, got {}",
        frame.glyphs.len()
    );

    let borders: Vec<_> = frame
        .decorations
        .iter()
        .filter(|d| d.kind == text_typeset::DecorationKind::TableBorder)
        .collect();
    assert!(
        !borders.is_empty(),
        "table between text blocks should still produce borders"
    );
}

#[test]
fn heading_renders_larger_than_body() {
    let doc = TextDocument::new();
    let op = doc
        .set_markdown("# Big Heading\n\nNormal paragraph.")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    // Heading glyphs should be taller than body glyphs
    // The heading is on the first line (lowest y), body is below
    if frame.glyphs.len() >= 2 {
        // Find the max glyph height on the first line vs last line
        let mut ys_and_heights: Vec<(f32, f32)> = frame
            .glyphs
            .iter()
            .map(|g| (g.screen[1], g.screen[3]))
            .collect();
        ys_and_heights.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let first_line_height = ys_and_heights.first().map(|g| g.1).unwrap_or(0.0);
        let last_line_height = ys_and_heights.last().map(|g| g.1).unwrap_or(0.0);

        // Heading (first line) should have taller glyphs
        assert!(
            first_line_height > last_line_height * 1.2,
            "heading glyph height ({}) should be >1.2x body glyph height ({})",
            first_line_height,
            last_line_height
        );
    }
}

#[test]
fn empty_document_renders_without_panic() {
    let doc = TextDocument::new();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    let frame = ts.render();

    // Should not panic; may produce glyphs for the empty initial block
    assert!(frame.atlas_width > 0 || frame.glyphs.is_empty());
}

#[test]
fn content_height_grows_with_content() {
    let doc = TextDocument::new();
    doc.set_plain_text("One line").unwrap();
    let flow1 = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow1);
    let h1 = ts.content_height();

    let doc2 = TextDocument::new();
    doc2.set_plain_text("Line one.\n\nLine two.\n\nLine three.\n\nLine four.")
        .unwrap();
    let flow2 = doc2.snapshot_flow();
    ts.layout_full(&flow2);
    let h2 = ts.content_height();

    assert!(
        h2 > h1,
        "more content should produce greater height: {} vs {}",
        h1,
        h2
    );
}

// ── Cursor movement across frame (blockquote) boundaries ───────

#[test]
fn hit_test_below_blockquote_lands_on_block_after() {
    let doc = TextDocument::new();
    let op = doc
        .set_markdown("Before\n\n> Quoted text\n\nAfter")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    ts.render();

    // Hit test near the bottom of the viewport - "After" should be there
    let content_h = ts.content_height();
    let result = ts.hit_test(10.0, content_h - 5.0);
    assert!(result.is_some(), "hit test near bottom should return a result");
    let hit = result.unwrap();
    // The last block "After" should be hit, not the blockquote's block
    assert!(
        hit.position >= "Before\n\n> Quoted text\n\n".len() - 5,
        "hit test at bottom should be in the 'After' block, got position {}",
        hit.position
    );
}

#[test]
fn caret_rect_moves_through_blockquote() {
    let doc = TextDocument::new();
    let op = doc
        .set_markdown("AB\n\n> CD\n\nEF")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    ts.render();

    // Caret at position 0 ("A") should be at the top
    let rect_before = ts.caret_rect(0);
    // Find a position inside the blockquote
    // The blockquote text "CD" is somewhere in the middle of the document
    let rect_quote = ts.caret_rect(4);
    // Find a position after the blockquote
    let rect_after = ts.caret_rect(8);

    // Each successive caret should be below the previous
    assert!(
        rect_quote[1] > rect_before[1],
        "caret in blockquote ({}) should be below caret before ({})",
        rect_quote[1],
        rect_before[1]
    );
    assert!(
        rect_after[1] > rect_quote[1],
        "caret after blockquote ({}) should be below caret in blockquote ({})",
        rect_after[1],
        rect_quote[1]
    );
    // All carets should have valid dimensions
    assert!(rect_before[3] > 0.0);
    assert!(rect_quote[3] > 0.0);
    assert!(rect_after[3] > 0.0);
}

#[test]
fn selection_spanning_blockquote_boundary() {
    let doc = TextDocument::new();
    let op = doc
        .set_markdown("AB\n\n> CD\n\nEF")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);

    // Select from "AB" through the blockquote into "EF"
    ts.set_cursor(&text_typeset::CursorDisplay {
        position: 0,
        anchor: 10,
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
        "cross-blockquote selection should produce multiple rects, got {}",
        selections.len()
    );
}

#[test]
fn ensure_caret_visible_inside_blockquote() {
    let doc = TextDocument::new();
    // Create enough content to require scrolling
    let mut md = String::new();
    for i in 0..20 {
        md.push_str(&format!("Paragraph {}.\n\n", i));
    }
    md.push_str("> Deep blockquote text\n\n");
    md.push_str("Final paragraph.");
    let op = doc.set_markdown(&md).unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(800.0, 50.0); // very short viewport

    ts.layout_full(&flow);
    ts.render();

    // Find a position deep in the document via hit_test
    let h = ts.content_height();
    let hit = ts.hit_test(10.0, h - 5.0);
    assert!(hit.is_some(), "should find content at bottom");
    let deep_pos = hit.unwrap().position;

    ts.set_cursor(&text_typeset::CursorDisplay {
        position: deep_pos,
        anchor: deep_pos,
        visible: true,
    });

    let result = ts.ensure_caret_visible();
    assert!(
        result.is_some(),
        "should need to scroll to reveal caret near bottom of long document"
    );
    assert!(result.unwrap() > 0.0, "scroll offset should be positive");
}

#[test]
fn caret_rect_after_edit_inside_blockquote() {
    let doc = TextDocument::new();
    let op = doc
        .set_markdown("Before\n\n> Short\n\nAfter")
        .unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = Typesetter::new();
    let face = ts.register_font(NOTO_SANS);
    ts.set_default_font(face, 16.0);
    ts.set_viewport(200.0, 600.0); // narrow viewport to force wrapping

    ts.layout_full(&flow);
    ts.render();

    // Find the "After" block position via hit_test at the bottom of the content
    let h = ts.content_height();
    let after_hit = ts.hit_test(10.0, h - 5.0);
    assert!(after_hit.is_some(), "should find the After block");
    let after_pos = after_hit.unwrap().position;
    let rect_before_edit = ts.caret_rect(after_pos);
    assert!(
        rect_before_edit[3] > 0.0,
        "caret for 'After' should have valid height before edit"
    );

    // Edit the blockquote to make it much longer
    let doc2 = TextDocument::new();
    let op = doc2
        .set_markdown("Before\n\n> This is a much longer blockquote that takes more vertical space than the short one did before\n\nAfter")
        .unwrap();
    op.wait().unwrap();
    let flow2 = doc2.snapshot_flow();
    ts.layout_full(&flow2);
    ts.render();

    // Find the "After" position again in the new layout
    let h2 = ts.content_height();
    let after_hit2 = ts.hit_test(10.0, h2 - 5.0);
    assert!(after_hit2.is_some(), "should find After block in new layout");
    let after_pos2 = after_hit2.unwrap().position;
    let rect_after_edit = ts.caret_rect(after_pos2);

    assert!(
        rect_after_edit[3] > 0.0,
        "caret after edit should have valid height"
    );
    // The caret for "After" should have moved down since the blockquote grew
    assert!(
        rect_after_edit[1] > rect_before_edit[1],
        "caret for 'After' should move down after blockquote grows: {} -> {}",
        rect_before_edit[1],
        rect_after_edit[1]
    );
}

#[test]
fn scroll_to_position_inside_blockquote() {
    let doc = TextDocument::new();
    let mut md = String::new();
    for i in 0..15 {
        md.push_str(&format!("Paragraph {}.\n\n", i));
    }
    md.push_str("> Blockquote deep in document\n\n");
    let op = doc.set_markdown(&md).unwrap();
    op.wait().unwrap();
    let flow = doc.snapshot_flow();

    let mut ts = setup_typesetter();
    ts.layout_full(&flow);
    ts.render();

    // Use hit_test to find a position deep in the document
    let h = ts.content_height();
    let hit = ts.hit_test(10.0, h - 5.0);
    assert!(hit.is_some());
    let deep_pos = hit.unwrap().position;

    let offset = ts.scroll_to_position(deep_pos);
    assert!(
        offset > 0.0,
        "scroll_to_position inside blockquote should produce positive offset"
    );
}
