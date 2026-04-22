//! Known-bug regression tests for text-typeset.
//!
//! Each test here demonstrates a concrete bug with a minimal
//! failing input. All are marked `#[ignore = "FIXME: ..."]` so
//! they don't flicker CI but stay visible to `cargo test --
//! --ignored`. When a bug is fixed, remove the `#[ignore]` and
//! the test becomes a regression guard.
//!
//! This file exists because the invariant properties in
//! `invariant_tests.rs` had to be loosened to pass while the bugs
//! here remain unfixed. Rather than silently weakening those
//! assertions, each loosening has a concrete counterpart here that
//! shows exactly what's broken.

mod helpers;

use helpers::{make_block, make_typesetter};

// ── Bug 1 ───────────────────────────────────────────────────────────
// `character_geometry(block, 0, n)` returns 0 entries when the text
// contains a standard Latin ligature (e.g. "fi"), even though the
// range covers 2 characters. This is an **accessibility bug**:
// AccessKit's `character_positions` and `character_widths` arrays
// need one entry per character so screen readers and magnifiers
// can track the caret at character granularity. When the shaper
// substitutes a single ligature glyph, `character_geometry` today
// returns nothing at all — not even a synthesised advance.
//
// Fix options:
//   (a) Return one entry per character by sub-dividing the
//       ligature's advance width evenly. Lossy but keeps the count
//       invariant.
//   (b) Return one entry per shaped glyph and expose a separate
//       character→glyph mapping so callers can split the advance
//       themselves if they want.
//
// Option (a) is the a11y-friendly default; option (b) is more
// correct but requires more caller cooperation.
//
// Related: `invariant_tests.rs::character_geometry_length_is_bounded
// _by_char_range` asserts the weak bound `geom.len() <= n` to make
// proptest pass; this test pins the `== n` contract the a11y layer
// actually needs.

#[test]
#[ignore = "FIXME: 'fi' ligature drops character_geometry entries (a11y regression)"]
fn ligature_character_geometry_returns_one_entry_per_char() {
    let mut ts = make_typesetter();
    ts.layout_blocks(vec![make_block(1, "fi")]);
    let geom = ts.character_geometry(1, 0, 2);
    // Currently returns 0 entries.
    // Expected: 2 entries, one per character, so a screen reader
    // can position the caret between `f` and `i`.
    assert_eq!(
        geom.len(),
        2,
        "ligature must not collapse character_geometry entries"
    );
}

#[test]
#[ignore = "FIXME: 'ffi' ligature drops character_geometry entries (a11y regression)"]
fn ffi_ligature_character_geometry_returns_three_entries() {
    let mut ts = make_typesetter();
    ts.layout_blocks(vec![make_block(1, "ffi")]);
    let geom = ts.character_geometry(1, 0, 3);
    // "ffi" triggers a 3→1 ligature in many fonts. The character
    // geometry must still expose three positions.
    assert_eq!(geom.len(), 3, "ffi ligature must expose 3 character slots");
}
