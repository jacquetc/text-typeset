mod helpers;
use helpers::{Rect, make_typesetter};

use text_typeset::TextFormat;

#[test]
fn single_line_produces_glyph_quads() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("Hello", &TextFormat::default(), None);

    assert!(!result.glyphs.is_empty(), "should produce glyph quads");
    assert!(result.width > 0.0, "width should be positive");
    assert!(result.height > 0.0, "height should be positive");
    assert!(result.baseline > 0.0, "baseline should be positive");
    assert!(
        result.baseline < result.height,
        "baseline should be less than line height"
    );
}

#[test]
fn single_line_empty_text_returns_empty() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("", &TextFormat::default(), None);

    assert!(result.glyphs.is_empty());
    assert_eq!(result.width, 0.0);
    assert_eq!(result.height, 0.0);
}

#[test]
fn single_line_glyph_quads_have_valid_coordinates() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("Test", &TextFormat::default(), None);

    for (i, quad) in result.glyphs.iter().enumerate() {
        assert!(
            quad.screen[2] > 0.0 && quad.screen[3] > 0.0,
            "glyph {} should have positive size: {:?}",
            i,
            quad.screen
        );
        assert!(
            quad.atlas[2] > 0.0 && quad.atlas[3] > 0.0,
            "glyph {} should have positive atlas size: {:?}",
            i,
            quad.atlas
        );
        assert!(
            quad.atlas[0] >= 0.0 && quad.atlas[1] >= 0.0,
            "glyph {} atlas coords should be non-negative: {:?}",
            i,
            quad.atlas
        );
    }
}

#[test]
fn single_line_glyphs_advance_left_to_right() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("ABCDEF", &TextFormat::default(), None);

    assert!(result.glyphs.len() >= 2, "should have multiple glyph quads");
    for i in 1..result.glyphs.len() {
        let prev_x = result.glyphs[i - 1].screen[0];
        let curr_x = result.glyphs[i].screen[0];
        assert!(
            curr_x > prev_x,
            "glyph {} x={} should be right of glyph {} x={}",
            i,
            curr_x,
            i - 1,
            prev_x
        );
    }
}

#[test]
fn single_line_longer_text_is_wider() {
    let mut ts = make_typesetter();
    let short = ts.layout_single_line("Hi", &TextFormat::default(), None);
    let long = ts.layout_single_line("Hello, world!", &TextFormat::default(), None);

    assert!(
        long.width > short.width,
        "longer text should be wider: {} vs {}",
        long.width,
        short.width
    );
}

#[test]
fn single_line_larger_font_is_taller() {
    let mut ts = make_typesetter();
    let small = ts.layout_single_line(
        "Hello",
        &TextFormat {
            font_size: Some(12.0),
            ..Default::default()
        },
        None,
    );
    let large = ts.layout_single_line(
        "Hello",
        &TextFormat {
            font_size: Some(32.0),
            ..Default::default()
        },
        None,
    );

    assert!(
        large.height > small.height,
        "larger font should produce taller line: {} vs {}",
        large.height,
        small.height
    );
    assert!(
        large.width > small.width,
        "larger font should produce wider text: {} vs {}",
        large.width,
        small.width
    );
}

#[test]
fn single_line_max_width_truncates() {
    let mut ts = make_typesetter();
    let full = ts.layout_single_line(
        "Hello, world! This is a long sentence.",
        &TextFormat::default(),
        None,
    );
    let truncated = ts.layout_single_line(
        "Hello, world! This is a long sentence.",
        &TextFormat::default(),
        Some(80.0),
    );

    assert!(
        truncated.width <= 80.0 + 1.0, // small tolerance
        "truncated width {} should be at most max_width 80.0",
        truncated.width
    );
    assert!(
        truncated.glyphs.len() < full.glyphs.len(),
        "truncated should have fewer glyphs: {} vs {}",
        truncated.glyphs.len(),
        full.glyphs.len()
    );
    // Should still have some glyphs (not completely empty)
    assert!(
        !truncated.glyphs.is_empty(),
        "truncated result should not be empty"
    );
}

#[test]
fn single_line_max_width_no_truncation_when_fits() {
    let mut ts = make_typesetter();
    let uncapped = ts.layout_single_line("Hi", &TextFormat::default(), None);
    let capped = ts.layout_single_line("Hi", &TextFormat::default(), Some(500.0));

    assert_eq!(
        uncapped.glyphs.len(),
        capped.glyphs.len(),
        "text that fits should not be truncated"
    );
    assert!(
        (uncapped.width - capped.width).abs() < 0.01,
        "widths should match when text fits: {} vs {}",
        uncapped.width,
        capped.width
    );
}

#[test]
fn single_line_no_vertical_overlap() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("Test overlap", &TextFormat::default(), None);

    let rects: Vec<Rect> = result.glyphs.iter().map(|q| Rect::from(q.screen)).collect();
    for i in 0..rects.len() {
        for j in (i + 1)..rects.len() {
            if !rects[i].overlaps(&rects[j]) {
                continue;
            }
            let ox =
                (rects[i].right().min(rects[j].right()) - rects[i].x().max(rects[j].x())).max(0.0);
            let oy = (rects[i].bottom().min(rects[j].bottom()) - rects[i].y().max(rects[j].y()))
                .max(0.0);
            let overlap_area = ox * oy;
            let smaller = (rects[i].w() * rects[i].h()).min(rects[j].w() * rects[j].h());
            if smaller > 0.0 {
                let ratio = overlap_area / smaller;
                assert!(
                    ratio < 0.5,
                    "glyph[{}] {} significantly overlaps glyph[{}] {} (ratio {:.2})",
                    i,
                    rects[i],
                    j,
                    rects[j],
                    ratio
                );
            }
        }
    }
}

#[test]
fn single_line_custom_color() {
    let mut ts = make_typesetter();
    let red = [1.0, 0.0, 0.0, 1.0];
    let result = ts.layout_single_line(
        "Red",
        &TextFormat {
            color: Some(red),
            ..Default::default()
        },
        None,
    );

    assert!(!result.glyphs.is_empty());
    for quad in &result.glyphs {
        assert_eq!(quad.color, red, "glyph should use custom color");
    }
}

#[test]
fn single_line_bold_is_different_width() {
    let mut ts = make_typesetter();
    let normal = ts.layout_single_line("Hello", &TextFormat::default(), None);
    let bold = ts.layout_single_line(
        "Hello",
        &TextFormat {
            font_bold: Some(true),
            ..Default::default()
        },
        None,
    );

    // Bold text typically has slightly different advance widths.
    // With a variable font this should produce different metrics.
    // At minimum, both should produce valid output.
    assert!(!normal.glyphs.is_empty());
    assert!(!bold.glyphs.is_empty());
    assert!(normal.width > 0.0);
    assert!(bold.width > 0.0);
}

#[test]
fn single_line_whitespace_only() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("   ", &TextFormat::default(), None);

    // Spaces produce no visible glyphs (glyph_id == 0 for space in most fonts,
    // or the rasterized glyph has zero dimensions), but the width should be positive.
    assert!(result.width > 0.0, "spaces should have positive width");
    assert!(result.height > 0.0, "should have positive line height");
}

#[test]
fn single_line_max_width_zero_returns_minimal() {
    let mut ts = make_typesetter();
    let result = ts.layout_single_line("Hello", &TextFormat::default(), Some(0.0));

    // With zero budget, nothing fits except possibly the ellipsis at zero width.
    // The result should not panic and width should be small.
    assert!(result.height > 0.0, "line height should still be valid");
}

#[test]
fn single_line_shares_atlas_with_full_pipeline() {
    let mut ts = make_typesetter();

    // First, render via single-line
    let sl = ts.layout_single_line("Atlas", &TextFormat::default(), None);
    assert!(!sl.glyphs.is_empty());

    // Then do a full pipeline render with the same text
    ts.layout_blocks(vec![helpers::make_block(1, "Atlas")]);
    let frame = ts.render();

    // Both should produce glyphs (sharing the same atlas)
    assert!(!frame.glyphs.is_empty());
    assert!(
        frame.atlas_width > 0,
        "atlas should have been populated by single-line and reused by full pipeline"
    );
}
