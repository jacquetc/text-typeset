use std::collections::HashMap;

use etagere::AllocId;

use crate::types::FontFaceId;

#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct GlyphCacheKey {
    pub font_face_id: FontFaceId,
    pub glyph_id: u16,
    pub size_bits: u32,
}

impl GlyphCacheKey {
    pub fn new(font_face_id: FontFaceId, glyph_id: u16, size_px: f32) -> Self {
        Self {
            font_face_id,
            glyph_id,
            size_bits: size_px.to_bits(),
        }
    }
}

pub struct CachedGlyph {
    pub alloc_id: AllocId,
    pub atlas_x: u32,
    pub atlas_y: u32,
    pub width: u32,
    pub height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub is_color: bool,
    /// Frame generation when this glyph was last used.
    pub last_used: u64,
}

/// Glyph cache with LRU eviction.
///
/// Tracks a frame generation counter. Each `get` marks the glyph as used
/// in the current generation. `evict_unused` removes glyphs not used
/// for `max_idle_frames` generations and deallocates their atlas space.
pub struct GlyphCache {
    pub(crate) entries: HashMap<GlyphCacheKey, CachedGlyph>,
    generation: u64,
    last_eviction_generation: u64,
}

/// Number of frames a glyph can go unused before being evicted.
const MAX_IDLE_FRAMES: u64 = 120; // ~2 seconds at 60fps

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            last_eviction_generation: 0,
        }
    }

    /// Advance the frame generation counter. Call once per render frame.
    pub fn advance_generation(&mut self) {
        self.generation += 1;
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Look up a cached glyph, marking it as used in the current generation.
    pub fn get(&mut self, key: &GlyphCacheKey) -> Option<&CachedGlyph> {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_used = self.generation;
            Some(entry)
        } else {
            None
        }
    }

    /// Look up without marking as used (for read-only queries).
    pub fn peek(&self, key: &GlyphCacheKey) -> Option<&CachedGlyph> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: GlyphCacheKey, mut glyph: CachedGlyph) {
        glyph.last_used = self.generation;
        self.entries.insert(key, glyph);
    }

    /// Evict glyphs unused for MAX_IDLE_FRAMES generations.
    /// Returns the AllocIds that should be deallocated from the atlas.
    /// Only runs the actual eviction scan every 60 calls (~1 second at 60fps)
    /// to avoid iterating the entire cache on every render.
    pub fn evict_unused(&mut self) -> Vec<AllocId> {
        // Only scan every 60 generations (~1 second at 60fps)
        if self.generation - self.last_eviction_generation < 60 {
            return Vec::new();
        }
        self.last_eviction_generation = self.generation;

        let threshold = self.generation.saturating_sub(MAX_IDLE_FRAMES);
        let mut evicted = Vec::new();

        self.entries.retain(|_key, glyph| {
            if glyph.last_used < threshold {
                evicted.push(glyph.alloc_id);
                false
            } else {
                true
            }
        });

        evicted
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
