//! Minimal inline-markup parser for label text.
//!
//! Supports:
//! - `[label](url)` — inline link
//! - `*italic*`     — italic run
//! - `**bold**`     — bold run
//!
//! Escapes: `\[`, `\]`, `\(`, `\)`, `\*`, `\\`. Unclosed markers fall back
//! to literal text — the parser never throws input away. Nesting works in
//! the obvious cases (`**bold *italic* bold**`, `[**bold link**](url)`).
//!
//! This module is independent of text-document: it only produces a small
//! `InlineMarkup` representation that `Typesetter::layout_single_line_markup`
//! and `Typesetter::layout_paragraph_markup` consume for tooltip / rich
//! label content.

use std::ops::Range;

/// Per-span style attributes. Link is orthogonal and carried on
/// [`InlineSpan::link_url`] directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InlineAttrs(u8);

impl InlineAttrs {
    pub const EMPTY: Self = Self(0);
    pub const BOLD: Self = Self(1 << 0);
    pub const ITALIC: Self = Self(1 << 1);

    pub fn empty() -> Self {
        Self::EMPTY
    }
    pub fn is_bold(self) -> bool {
        self.0 & Self::BOLD.0 != 0
    }
    pub fn is_italic(self) -> bool {
        self.0 & Self::ITALIC.0 != 0
    }
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for InlineAttrs {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for InlineAttrs {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// One parsed span of a minimally-marked-up string.
#[derive(Debug, Clone)]
pub struct InlineSpan {
    /// Visible text of this span (link label for links).
    pub text: String,
    /// Bold / italic attributes.
    pub attrs: InlineAttrs,
    /// `Some(url)` if this span is a link.
    pub link_url: Option<String>,
    /// Byte range into the original source string.
    pub byte_range: Range<usize>,
}

/// Parsed input ready for shaping. Preserves the original source for
/// diagnostics and round-tripping.
#[derive(Debug, Clone)]
pub struct InlineMarkup {
    pub source: String,
    pub spans: Vec<InlineSpan>,
}

impl InlineMarkup {
    /// Parse a minimal markdown subset.
    pub fn parse(source: &str) -> Self {
        let spans = parse_spans(source, 0);
        Self {
            source: source.to_string(),
            spans,
        }
    }

    /// Shortcut for plain text (no markup) — a single span, no attrs.
    pub fn plain(text: impl Into<String>) -> Self {
        let s: String = text.into();
        let spans = if s.is_empty() {
            Vec::new()
        } else {
            let len = s.len();
            vec![InlineSpan {
                text: s.clone(),
                attrs: InlineAttrs::EMPTY,
                link_url: None,
                byte_range: 0..len,
            }]
        };
        Self { source: s, spans }
    }

    /// Flatten the markup to its visible text only (no attributes, no urls).
    pub fn flatten_plain(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

// --- parser ------------------------------------------------------------

fn parse_spans(source: &str, base_offset: usize) -> Vec<InlineSpan> {
    let bytes = source.as_bytes();
    let mut out: Vec<InlineSpan> = Vec::new();
    let mut i: usize = 0;
    let mut text_start: usize = 0;
    let mut text_buf = String::new();

    let flush_text =
        |out: &mut Vec<InlineSpan>, text_buf: &mut String, text_start: usize, end: usize| {
            if !text_buf.is_empty() {
                out.push(InlineSpan {
                    text: std::mem::take(text_buf),
                    attrs: InlineAttrs::EMPTY,
                    link_url: None,
                    byte_range: (base_offset + text_start)..(base_offset + end),
                });
            }
        };

    while i < bytes.len() {
        let b = bytes[i];

        // Escape: \X → literal X
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if matches!(next, b'[' | b']' | b'(' | b')' | b'*' | b'\\') {
                if text_buf.is_empty() {
                    text_start = i;
                }
                text_buf.push(next as char);
                i += 2;
                continue;
            }
        }

        // Bold **…**
        if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            if let Some(close) = find_marker(source, i + 2, "**")
                && close > i + 2
            {
                flush_text(&mut out, &mut text_buf, text_start, i);
                let inner = &source[i + 2..close];
                let mut inner_spans = parse_spans(inner, base_offset + i + 2);
                for sp in inner_spans.iter_mut() {
                    sp.attrs |= InlineAttrs::BOLD;
                }
                out.extend(inner_spans);
                i = close + 2;
                text_start = i;
                continue;
            }
        }

        // Italic *…*
        if b == b'*' {
            if let Some(close) = find_marker(source, i + 1, "*")
                && close > i + 1
            {
                // Don't consume a `*` that's actually the start of a `**`.
                let close_is_double = close + 1 < bytes.len() && bytes[close + 1] == b'*';
                if !close_is_double {
                    flush_text(&mut out, &mut text_buf, text_start, i);
                    let inner = &source[i + 1..close];
                    let mut inner_spans = parse_spans(inner, base_offset + i + 1);
                    for sp in inner_spans.iter_mut() {
                        sp.attrs |= InlineAttrs::ITALIC;
                    }
                    out.extend(inner_spans);
                    i = close + 1;
                    text_start = i;
                    continue;
                }
            }
        }

        // Link [label](url)
        if b == b'['
            && let Some(close_label) = find_bracket_close(source, i + 1)
            && close_label + 1 < bytes.len()
            && bytes[close_label + 1] == b'('
            && let Some(close_paren) = find_paren_close(source, close_label + 2)
        {
            flush_text(&mut out, &mut text_buf, text_start, i);
            let label = source[i + 1..close_label].to_string();
            let url = source[close_label + 2..close_paren].to_string();
            out.push(InlineSpan {
                text: label,
                attrs: InlineAttrs::EMPTY,
                link_url: Some(url),
                byte_range: (base_offset + i)..(base_offset + close_paren + 1),
            });
            i = close_paren + 1;
            text_start = i;
            continue;
        }

        // Literal char. Advance by UTF-8 scalar length so we never split a
        // multi-byte sequence.
        if text_buf.is_empty() {
            text_start = i;
        }
        let ch_len = utf8_char_len(b);
        let ch_end = (i + ch_len).min(bytes.len());
        text_buf.push_str(&source[i..ch_end]);
        i = ch_end;
    }

    flush_text(&mut out, &mut text_buf, text_start, bytes.len());
    out
}

fn utf8_char_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 1,
    }
}

/// Find the next occurrence of `marker` in `source` starting at `from`,
/// skipping escaped characters. Returns the byte index of the first byte
/// of the marker, or `None` if unmatched.
fn find_marker(source: &str, from: usize, marker: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mk = marker.as_bytes();
    if mk.is_empty() {
        return None;
    }
    let mut i = from;
    while i + mk.len() <= bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i..i + mk.len()] == *mk {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_bracket_close(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_paren_close(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b')' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_produces_single_span() {
        let m = InlineMarkup::parse("hello world");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "hello world");
        assert!(m.spans[0].link_url.is_none());
        assert_eq!(m.spans[0].attrs, InlineAttrs::EMPTY);
        assert_eq!(m.spans[0].byte_range, 0..11);
    }

    #[test]
    fn empty_input_produces_no_spans() {
        let m = InlineMarkup::parse("");
        assert!(m.spans.is_empty());
    }

    #[test]
    fn link_between_text() {
        let m = InlineMarkup::parse("see [docs](https://x) now");
        assert_eq!(m.spans.len(), 3);
        assert_eq!(m.spans[0].text, "see ");
        assert_eq!(m.spans[1].text, "docs");
        assert_eq!(m.spans[1].link_url.as_deref(), Some("https://x"));
        assert_eq!(m.spans[2].text, " now");
    }

    #[test]
    fn two_adjacent_links() {
        let m = InlineMarkup::parse("[a](b)[c](d)");
        assert_eq!(m.spans.len(), 2);
        assert_eq!(m.spans[0].text, "a");
        assert_eq!(m.spans[0].link_url.as_deref(), Some("b"));
        assert_eq!(m.spans[1].text, "c");
        assert_eq!(m.spans[1].link_url.as_deref(), Some("d"));
    }

    #[test]
    fn unclosed_bracket_is_literal() {
        let m = InlineMarkup::parse("unclosed [bracket text");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "unclosed [bracket text");
    }

    #[test]
    fn escaped_brackets_are_literal() {
        let m = InlineMarkup::parse(r"\[not a link\]");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "[not a link]");
    }

    #[test]
    fn empty_label_link_still_parses() {
        let m = InlineMarkup::parse("[](url)");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "");
        assert_eq!(m.spans[0].link_url.as_deref(), Some("url"));
    }

    #[test]
    fn empty_url_link_still_parses() {
        let m = InlineMarkup::parse("[label]()");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "label");
        assert_eq!(m.spans[0].link_url.as_deref(), Some(""));
    }

    #[test]
    fn bold_wraps_inner_text() {
        let m = InlineMarkup::parse("a **b** c");
        assert_eq!(m.spans.len(), 3);
        assert_eq!(m.spans[0].text, "a ");
        assert!(!m.spans[0].attrs.is_bold());
        assert_eq!(m.spans[1].text, "b");
        assert!(m.spans[1].attrs.is_bold());
        assert!(!m.spans[1].attrs.is_italic());
        assert_eq!(m.spans[2].text, " c");
    }

    #[test]
    fn italic_wraps_inner_text() {
        let m = InlineMarkup::parse("a *b* c");
        assert_eq!(m.spans.len(), 3);
        assert!(m.spans[1].attrs.is_italic());
        assert!(!m.spans[1].attrs.is_bold());
    }

    #[test]
    fn bold_italic_nesting() {
        let m = InlineMarkup::parse("**bold *italic* bold**");
        // Inside bold: "bold ", "italic" (italic), " bold" — all bold.
        assert!(m.spans.iter().all(|s| s.attrs.is_bold()));
        assert!(m.spans.iter().any(|s| s.attrs.is_italic()));
    }

    #[test]
    fn link_inside_bold() {
        let m = InlineMarkup::parse("**see [docs](url)**");
        assert!(m.spans.iter().all(|s| s.attrs.is_bold()));
        assert!(m.spans.iter().any(|s| s.link_url.is_some()));
    }

    #[test]
    fn unclosed_bold_is_literal() {
        let m = InlineMarkup::parse("**unclosed");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "**unclosed");
    }

    #[test]
    fn tooltip_key_url_passes_through_verbatim() {
        // The `:key` URL scheme is recognized by the tooltip widget,
        // not the parser. Parser just stores the URL as-is.
        let m = InlineMarkup::parse("click [here](:my-key) to learn more");
        let link = m.spans.iter().find(|s| s.link_url.is_some()).unwrap();
        assert_eq!(link.text, "here");
        assert_eq!(link.link_url.as_deref(), Some(":my-key"));
    }

    #[test]
    fn flatten_plain_concatenates_text() {
        let m = InlineMarkup::parse("a **b** [c](d) e");
        assert_eq!(m.flatten_plain(), "a b c e");
    }

    #[test]
    fn utf8_multibyte_characters_preserved() {
        let m = InlineMarkup::parse("café ☕ résumé");
        assert_eq!(m.spans.len(), 1);
        assert_eq!(m.spans[0].text, "café ☕ résumé");
    }

    #[test]
    fn byte_ranges_are_absolute_into_source() {
        let m = InlineMarkup::parse("a [b](c) d");
        // "a " = 0..2
        // "[b](c)" = 2..8
        // " d" = 8..10
        assert_eq!(m.spans[0].byte_range, 0..2);
        assert_eq!(m.spans[1].byte_range, 2..8);
        assert_eq!(m.spans[2].byte_range, 8..10);
    }
}
