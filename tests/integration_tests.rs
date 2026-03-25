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
