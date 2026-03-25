use etagere::{AllocId, Allocation, BucketedAtlasAllocator, size2};

const INITIAL_ATLAS_SIZE: i32 = 512;
const MAX_ATLAS_SIZE: i32 = 4096;

pub struct GlyphAtlas {
    pub allocator: BucketedAtlasAllocator,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    pub fn new() -> Self {
        let size = INITIAL_ATLAS_SIZE;
        let pixel_count = (size * size) as usize * 4;
        Self {
            allocator: BucketedAtlasAllocator::new(size2(size, size)),
            pixels: vec![0u8; pixel_count],
            width: size as u32,
            height: size as u32,
            dirty: false,
        }
    }

    pub fn allocate(&mut self, width: u32, height: u32) -> Option<Allocation> {
        let size = size2(width as i32, height as i32);
        if let Some(alloc) = self.allocator.allocate(size) {
            return Some(alloc);
        }
        // Try growing the atlas
        let new_w = (self.width * 2).min(MAX_ATLAS_SIZE as u32) as i32;
        let new_h = (self.height * 2).min(MAX_ATLAS_SIZE as u32) as i32;
        if new_w as u32 == self.width && new_h as u32 == self.height {
            return None; // Already at max size
        }
        self.grow(new_w as u32, new_h as u32);
        self.allocator.allocate(size)
    }

    pub fn deallocate(&mut self, id: AllocId) {
        self.allocator.deallocate(id);
    }

    fn grow(&mut self, new_width: u32, new_height: u32) {
        let mut new_pixels = vec![0u8; (new_width * new_height) as usize * 4];
        // Copy existing pixels row by row
        for y in 0..self.height {
            let src_start = (y * self.width) as usize * 4;
            let src_end = src_start + self.width as usize * 4;
            let dst_start = (y * new_width) as usize * 4;
            let dst_end = dst_start + self.width as usize * 4;
            new_pixels[dst_start..dst_end].copy_from_slice(&self.pixels[src_start..src_end]);
        }
        self.allocator
            .grow(size2(new_width as i32, new_height as i32));
        self.pixels = new_pixels;
        self.width = new_width;
        self.height = new_height;
        self.dirty = true;
    }

    /// Blit RGBA pixel data into the atlas at the given position.
    pub fn blit_rgba(&mut self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        for row in 0..h {
            let src_start = (row * w) as usize * 4;
            let src_end = src_start + w as usize * 4;
            let dst_start = ((y + row) * self.width + x) as usize * 4;
            let dst_end = dst_start + w as usize * 4;
            self.pixels[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
        }
        self.dirty = true;
    }

    /// Blit a single-channel alpha mask into the atlas as white RGBA.
    pub fn blit_mask(&mut self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        for row in 0..h {
            for col in 0..w {
                let alpha = data[(row * w + col) as usize];
                let dst = ((y + row) * self.width + x + col) as usize * 4;
                self.pixels[dst] = 255;
                self.pixels[dst + 1] = 255;
                self.pixels[dst + 2] = 255;
                self.pixels[dst + 3] = alpha;
            }
        }
        self.dirty = true;
    }
}
