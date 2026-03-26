use std::sync::Arc;

use fontdb::{Database, Family, Query, Source, Style, Weight};

use crate::types::FontFaceId;

pub struct FontEntry {
    pub fontdb_id: fontdb::ID,
    pub face_index: u32,
    pub data: Arc<Vec<u8>>,
    pub swash_cache_key: swash::CacheKey,
}

pub struct FontRegistry {
    fontdb: Database,
    fonts: Vec<Option<FontEntry>>,
    generic_families: std::collections::HashMap<String, String>,
    default_font: Option<FontFaceId>,
    default_size_px: f32,
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FontRegistry {
    pub fn new() -> Self {
        Self {
            fontdb: Database::new(),
            fonts: Vec::new(),
            generic_families: std::collections::HashMap::new(),
            default_font: None,
            default_size_px: 16.0,
        }
    }

    /// Register a font from raw bytes. Returns IDs for all faces found
    /// (font collections may contain multiple faces).
    pub fn register_font(&mut self, data: &[u8]) -> Vec<FontFaceId> {
        let arc_data: Arc<Vec<u8>> = Arc::new(data.to_vec());
        let source = Source::Binary(arc_data.clone());
        let fontdb_ids = self.fontdb.load_font_source(source);

        let mut face_ids = Vec::new();
        for fontdb_id in fontdb_ids {
            let face_index = self.fontdb.face(fontdb_id).map(|f| f.index).unwrap_or(0);

            let swash_cache_key = swash::CacheKey::new();
            let entry = FontEntry {
                fontdb_id,
                face_index,
                data: arc_data.clone(),
                swash_cache_key,
            };

            let face_id = FontFaceId(self.fonts.len() as u32);
            self.fonts.push(Some(entry));
            face_ids.push(face_id);
        }
        face_ids
    }

    /// Register a font with explicit metadata, overriding the font's name table.
    pub fn register_font_as(
        &mut self,
        data: &[u8],
        family: &str,
        weight: u16,
        italic: bool,
    ) -> Vec<FontFaceId> {
        let arc_data: Arc<Vec<u8>> = Arc::new(data.to_vec());
        let source = Source::Binary(arc_data.clone());
        let fontdb_ids = self.fontdb.load_font_source(source);

        let mut face_ids = Vec::new();
        for fontdb_id in fontdb_ids {
            // Override metadata in fontdb
            if let Some(face_info) = self.fontdb.face(fontdb_id) {
                let mut info = face_info.clone();
                info.families = vec![(family.to_string(), fontdb::Language::English_UnitedStates)];
                info.weight = Weight(weight);
                info.style = if italic { Style::Italic } else { Style::Normal };
                // Remove old and re-add with new metadata
                let face_index = info.index;
                self.fontdb.remove_face(fontdb_id);
                let new_id = self.fontdb.push_face_info(info);

                let swash_cache_key = swash::CacheKey::new();
                let entry = FontEntry {
                    fontdb_id: new_id,
                    face_index,
                    data: arc_data.clone(),
                    swash_cache_key,
                };

                let face_id = FontFaceId(self.fonts.len() as u32);
                self.fonts.push(Some(entry));
                face_ids.push(face_id);
            }
        }
        face_ids
    }

    pub fn set_default_font(&mut self, face: FontFaceId, size_px: f32) {
        self.default_font = Some(face);
        self.default_size_px = size_px;
    }

    pub fn default_font(&self) -> Option<FontFaceId> {
        self.default_font
    }

    pub fn default_size_px(&self) -> f32 {
        self.default_size_px
    }

    pub fn set_generic_family(&mut self, generic: &str, family: &str) {
        self.generic_families
            .insert(generic.to_string(), family.to_string());
    }

    /// Resolve a family name, mapping generic names (serif, monospace, etc.)
    /// to their configured concrete family names.
    pub fn resolve_family_name<'a>(&'a self, family: &'a str) -> &'a str {
        self.generic_families
            .get(family)
            .map(|s| s.as_str())
            .unwrap_or(family)
    }

    /// Query fontdb for a font matching the given criteria.
    pub fn query_font(&self, family: &str, weight: u16, italic: bool) -> Option<FontFaceId> {
        let resolved = self.resolve_family_name(family);
        let style = if italic { Style::Italic } else { Style::Normal };

        let query = Query {
            families: &[Family::Name(resolved)],
            weight: Weight(weight),
            style,
            ..Query::default()
        };

        let fontdb_id = self.fontdb.query(&query)?;
        self.fontdb_id_to_face_id(fontdb_id)
    }

    /// Look up a FontEntry by FontFaceId.
    pub fn get(&self, face_id: FontFaceId) -> Option<&FontEntry> {
        self.fonts
            .get(face_id.0 as usize)
            .and_then(|opt| opt.as_ref())
    }

    /// Query for a variant (different weight/style) of an existing registered font.
    /// Looks up the family name of `base_face` and queries fontdb for a match.
    pub fn query_variant(
        &self,
        base_face: FontFaceId,
        weight: u16,
        italic: bool,
    ) -> Option<FontFaceId> {
        let entry = self.get(base_face)?;
        let face_info = self.fontdb.face(entry.fontdb_id)?;
        let family_name = face_info.families.first().map(|(name, _)| name.as_str())?;
        self.query_font(family_name, weight, italic)
    }

    /// Find our FontFaceId for a fontdb ID.
    fn fontdb_id_to_face_id(&self, fontdb_id: fontdb::ID) -> Option<FontFaceId> {
        self.fonts.iter().enumerate().find_map(|(i, entry)| {
            entry
                .as_ref()
                .filter(|e| e.fontdb_id == fontdb_id)
                .map(|_| FontFaceId(i as u32))
        })
    }

    /// Iterate all registered font entries for glyph fallback.
    pub fn all_entries(&self) -> impl Iterator<Item = (FontFaceId, &FontEntry)> {
        self.fonts
            .iter()
            .enumerate()
            .filter_map(|(i, opt)| opt.as_ref().map(|entry| (FontFaceId(i as u32), entry)))
    }
}
