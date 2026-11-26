//! `gpu` 기능으로 켜는 GPU 글리프 아틀라스.
//! 캐시 키·페이지 상태·프레임 시계는 장치 없이 시험할 수 있도록 크레이트 루트에 둔다.

use cosmic_text::{Buffer, Metrics, Shaping, SwashContent};
use rustc_hash::FxHashMap;

use crate::{AtlasEntry, AtlasPage, FontConfig, FrameClock, GlyphKey, pick_lru_victim};

/// Direct cache for ASCII code points 0..128 × bold × italic: 512 slots.
struct AsciiCache {
    slots: Box<[Option<AtlasEntry>; 512]>,
}

impl AsciiCache {
    fn new() -> Self {
        Self {
            slots: Box::new([None; 512]),
        }
    }

    #[inline]
    fn index_of(key: &GlyphKey) -> Option<usize> {
        let cp = key.ch as u32;
        if cp < 128 {
            let base = (cp as usize) << 2;
            let off = (key.bold as usize) | ((key.italic as usize) << 1);
            Some(base | off)
        } else {
            None
        }
    }

    #[inline]
    fn get(&self, key: &GlyphKey) -> Option<AtlasEntry> {
        Self::index_of(key).and_then(|i| self.slots[i])
    }

    #[inline]
    fn insert(&mut self, key: &GlyphKey, entry: AtlasEntry) -> bool {
        if let Some(i) = Self::index_of(key) {
            self.slots[i] = Some(entry);
            true
        } else {
            false
        }
    }

    fn retain_pages_except(&mut self, victim_page: u32) {
        for slot in self.slots.iter_mut() {
            if let Some(e) = slot
                && e.page == victim_page
            {
                *slot = None;
            }
        }
    }
}

/// GPU texture atlas for glyph bitmaps, backed by a `D2Array` texture with
/// `MAX_PAGES` layers. Uses a simple shelf-based row packer per page. Once
/// all pages are full, the least-recently-used non-active page is evicted
/// (cache entries dropped + layer zero-cleared) and reused.
pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    atlas_size: u32,
    ascii_cache: AsciiCache,
    overflow_cache: FxHashMap<GlyphKey, AtlasEntry>,
    pages: Vec<AtlasPage>,
    /// Page where new glyphs are currently being packed.
    active_page: u32,
    /// Frame counter + once-per-frame eviction throttle. See `FrameClock`.
    frame: FrameClock,
    /// Monotonic count of page evictions since atlas construction. Surfaced
    /// via `eviction_count()` for perf logging; never read by atlas logic.
    eviction_count: u64,
}

impl GlyphAtlas {
    pub const ATLAS_SIZE: u32 = 2048;
    /// Maximum number of atlas pages (D2Array layers). Fixed at construction:
    /// 4 × 2048² R8Unorm = 32 MiB resident memory.
    pub const MAX_PAGES: u32 = 4;

    pub fn new(device: &wgpu::Device) -> Self {
        let atlas_size = Self::ATLAS_SIZE;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: Self::MAX_PAGES,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("glyph_atlas_view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            atlas_size,
            ascii_cache: AsciiCache::new(),
            overflow_cache: FxHashMap::default(),
            pages: vec![AtlasPage::default(); Self::MAX_PAGES as usize],
            active_page: 0,
            frame: FrameClock::default(),
            eviction_count: 0,
        }
    }

    /// Bump the frame counter. Call once per render frame, before any
    /// `get_or_insert` calls, so LRU stamps are coherent and the eviction
    /// throttle re-arms. Skipping it is silent: the atlas evicts once and then
    /// drops every glyph that would need another eviction.
    pub fn begin_frame(&mut self) {
        self.frame.begin_frame();
    }

    /// Monotonic eviction count since construction. Diagnostic only.
    pub fn eviction_count(&self) -> u64 {
        self.eviction_count
    }

    /// Number of pages that currently hold at least one glyph entry.
    pub fn active_page_count(&self) -> u32 {
        self.pages.iter().filter(|p| p.entry_count > 0).count() as u32
    }

    /// Sum of `entry_count` across all pages. Approximate live cache size.
    pub fn entry_count_sum(&self) -> u32 {
        self.pages.iter().map(|p| p.entry_count).sum()
    }

    /// Get or rasterize a glyph, returning its atlas entry.
    pub fn get_or_insert(
        &mut self,
        key: GlyphKey,
        font_config: &mut FontConfig,
        queue: &wgpu::Queue,
    ) -> Option<AtlasEntry> {
        if let Some(entry) = self.ascii_cache.get(&key) {
            self.pages[entry.page as usize].last_access_frame = self.frame.now();
            return Some(entry);
        }
        if let Some(entry) = self.overflow_cache.get(&key).copied() {
            self.pages[entry.page as usize].last_access_frame = self.frame.now();
            return Some(entry);
        }
        self.rasterize_glyph(key, font_config, queue)
    }

    #[inline]
    fn cache_insert(&mut self, key: GlyphKey, entry: AtlasEntry) {
        if !self.ascii_cache.insert(&key, entry) {
            self.overflow_cache.insert(key, entry);
        }
    }

    /// Try to reserve a glyph box on some page, possibly evicting one if all
    /// pages are full. Returns `(page_index, x, y)` on success.
    fn allocate_box(&mut self, w: u32, h: u32, queue: &wgpu::Queue) -> Option<(u32, u32, u32)> {
        if w > self.atlas_size || h > self.atlas_size {
            return None;
        }
        let max_pages = self.pages.len() as u32;
        for step in 0..max_pages {
            let idx = (self.active_page + step) % max_pages;
            if let Some((x, y)) = self.pages[idx as usize].try_allocate(w, h, self.atlas_size) {
                self.active_page = idx;
                self.pages[idx as usize].last_access_frame = self.frame.now();
                return Some((idx, x, y));
            }
        }
        // All pages full — evict the LRU non-active page (at most once per frame).
        if self.frame.evicted_this_frame() {
            tracing::warn!(
                "glyph atlas: second eviction skipped in frame {}; glyph deferred",
                self.frame.now()
            );
            return None;
        }
        let victim = pick_lru_victim(&self.pages, self.active_page)?;
        tracing::warn!(
            "evicting atlas page {} ({} entries, last_access_frame={})",
            victim,
            self.pages[victim as usize].entry_count,
            self.pages[victim as usize].last_access_frame
        );
        self.ascii_cache.retain_pages_except(victim);
        self.overflow_cache.retain(|_, e| e.page != victim);
        self.pages[victim as usize].reset();
        // Zero-clear the layer so stale glyph pixels don't bleed into new UVs.
        let empty = vec![0u8; (self.atlas_size * self.atlas_size) as usize];
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: victim,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &empty,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.atlas_size),
                rows_per_image: Some(self.atlas_size),
            },
            wgpu::Extent3d {
                width: self.atlas_size,
                height: self.atlas_size,
                depth_or_array_layers: 1,
            },
        );
        self.frame.note_eviction();
        self.eviction_count = self.eviction_count.saturating_add(1);
        let (x, y) = self.pages[victim as usize].try_allocate(w, h, self.atlas_size)?;
        self.active_page = victim;
        self.pages[victim as usize].last_access_frame = self.frame.now();
        Some((victim, x, y))
    }

    /// Try to rasterize a builtin block/box character programmatically.
    fn rasterize_builtin_glyph(
        &mut self,
        ch: char,
        font_config: &FontConfig,
        queue: &wgpu::Queue,
    ) -> Option<AtlasEntry> {
        let cw = font_config.metrics.cell_width.round() as u32;
        let ch_px = font_config.metrics.cell_height.round() as u32;
        if cw == 0 || ch_px == 0 {
            return None;
        }

        let mut bitmap = vec![0u8; (cw * ch_px) as usize];

        if !draw_builtin_char(ch, &mut bitmap, cw, ch_px) {
            return None;
        }

        self.upload_bitmap(&bitmap, cw, ch_px, 0.0, 0.0, queue)
    }

    /// Pack a bitmap into the next available page and upload it.
    fn upload_bitmap(
        &mut self,
        bitmap: &[u8],
        glyph_width: u32,
        glyph_height: u32,
        offset_x: f32,
        offset_y: f32,
        queue: &wgpu::Queue,
    ) -> Option<AtlasEntry> {
        let (page, x, y) = self.allocate_box(glyph_width, glyph_height, queue)?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: page },
                aspect: wgpu::TextureAspect::All,
            },
            bitmap,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(glyph_width),
                rows_per_image: Some(glyph_height),
            },
            wgpu::Extent3d {
                width: glyph_width,
                height: glyph_height,
                depth_or_array_layers: 1,
            },
        );

        let atlas_f = self.atlas_size as f32;
        Some(AtlasEntry {
            uv_x: x as f32 / atlas_f,
            uv_y: y as f32 / atlas_f,
            uv_w: glyph_width as f32 / atlas_f,
            uv_h: glyph_height as f32 / atlas_f,
            offset_x,
            offset_y,
            width: glyph_width as f32,
            height: glyph_height as f32,
            page,
        })
    }

    fn rasterize_glyph(
        &mut self,
        key: GlyphKey,
        font_config: &mut FontConfig,
        queue: &wgpu::Queue,
    ) -> Option<AtlasEntry> {
        if !key.bold
            && !key.italic
            && let Some(entry) = self.rasterize_builtin_glyph(key.ch, font_config, queue)
        {
            self.cache_insert(key, entry);
            return Some(entry);
        }

        let font_size = font_config.metrics.font_size;
        let line_height = font_config.metrics.cell_height;
        let cosmic_metrics = Metrics::new(font_size, line_height);

        let mut buffer = Buffer::new(&mut font_config.font_system, cosmic_metrics);
        buffer.set_size(
            &mut font_config.font_system,
            Some(font_size * 4.0),
            Some(line_height * 2.0),
        );

        let attrs = key.attrs(&font_config.font_family);

        let text = key.ch.to_string();
        buffer.set_text(
            &mut font_config.font_system,
            &text,
            &attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut font_config.font_system, false);

        let found_glyph = buffer.layout_runs().find_map(|run| {
            run.glyphs
                .first()
                .map(|g| (g.physical((0.0, 0.0), 1.0), run.line_y))
        });

        let (physical_glyph, _line_y) = found_glyph?;

        let image = font_config
            .swash_cache
            .get_image(&mut font_config.font_system, physical_glyph.cache_key)
            .as_ref()?;

        let glyph_width = image.placement.width;
        let glyph_height = image.placement.height;

        if glyph_width == 0 || glyph_height == 0 {
            // Space or invisible character - cache an empty entry on the active page.
            let entry = AtlasEntry {
                uv_x: 0.0,
                uv_y: 0.0,
                uv_w: 0.0,
                uv_h: 0.0,
                offset_x: 0.0,
                offset_y: 0.0,
                width: 0.0,
                height: 0.0,
                page: self.active_page,
            };
            self.cache_insert(key, entry);
            return Some(entry);
        }

        let grayscale_data: Vec<u8> = match image.content {
            SwashContent::Mask => image.data.clone(),
            SwashContent::Color => {
                // RGBA -> take alpha channel
                image.data.chunks_exact(4).map(|pixel| pixel[3]).collect()
            }
            SwashContent::SubpixelMask => {
                // RGB subpixel -> average to grayscale
                image
                    .data
                    .chunks_exact(3)
                    .map(|pixel| ((pixel[0] as u16 + pixel[1] as u16 + pixel[2] as u16) / 3) as u8)
                    .collect()
            }
        };

        // Glyph offset relative to cell origin:
        // placement.left is the horizontal bearing (distance from cell left to glyph left)
        // placement.top is the vertical bearing (distance from baseline to glyph top)
        // We need: offset from cell top-left to glyph top-left
        let offset_x = image.placement.left as f32;
        let offset_y = font_config.metrics.baseline - image.placement.top as f32;

        let entry = self.upload_bitmap(
            &grayscale_data,
            glyph_width,
            glyph_height,
            offset_x,
            offset_y,
            queue,
        )?;
        self.cache_insert(key, entry);
        Some(entry)
    }
}

/// Returns `true` if the character was handled (bitmap filled).
fn draw_builtin_char(ch: char, bitmap: &mut [u8], w: u32, h: u32) -> bool {
    let cp = ch as u32;
    match cp {
        0x2580..=0x259F => draw_block_element(cp, bitmap, w, h),
        0x2500..=0x257F => draw_box_drawing(cp, bitmap, w, h),
        _ => false,
    }
}

/// Fill [x0, x1) × [y0, y1) with the given alpha.
#[allow(clippy::too_many_arguments)] // reason: 비트맵과 사각형 좌표, 알파를 함께 받는 내부 그리기 함수
fn fill_rect(bitmap: &mut [u8], w: u32, _h: u32, x0: u32, y0: u32, x1: u32, y1: u32, alpha: u8) {
    for y in y0..y1 {
        for x in x0..x1 {
            let idx = (y * w + x) as usize;
            if idx < bitmap.len() {
                // Composite: max of existing and new so overlapping fills work
                let cur = bitmap[idx];
                bitmap[idx] = cur.max(alpha);
            }
        }
    }
}

/// Fill a horizontal line of given thickness centred at cy, from x0 to x1.
pub(crate) fn fill_hline(
    bitmap: &mut [u8],
    w: u32,
    h: u32,
    x0: u32,
    x1: u32,
    cy: u32,
    thickness: u32,
) {
    let half = thickness / 2;
    let y0 = cy.saturating_sub(half);
    let y1 = (cy + thickness - half).min(h);
    fill_rect(bitmap, w, h, x0, y0, x1, y1, 255);
}

/// Fill a vertical line of given thickness centred at cx, from y0 to y1.
pub(crate) fn fill_vline(
    bitmap: &mut [u8],
    w: u32,
    h: u32,
    y0: u32,
    y1: u32,
    cx: u32,
    thickness: u32,
) {
    let half = thickness / 2;
    let x0 = cx.saturating_sub(half);
    let x1 = (cx + thickness - half).min(w);
    fill_rect(bitmap, w, h, x0, y0, x1, y1, 255);
}

fn draw_block_element(cp: u32, bitmap: &mut [u8], w: u32, h: u32) -> bool {
    match cp {
        // U+2580  ▀  Upper half block
        0x2580 => fill_rect(bitmap, w, h, 0, 0, w, h / 2, 255),
        // U+2581–U+2587  Lower 1/8 … 7/8 blocks
        0x2581 => {
            let t = h / 8;
            fill_rect(bitmap, w, h, 0, h - t.max(1), w, h, 255);
        }
        0x2582 => {
            let t = h / 4;
            fill_rect(bitmap, w, h, 0, h - t.max(1), w, h, 255);
        }
        0x2583 => {
            let t = h * 3 / 8;
            fill_rect(bitmap, w, h, 0, h - t.max(1), w, h, 255);
        }
        0x2584 => fill_rect(bitmap, w, h, 0, h / 2, w, h, 255),
        0x2585 => {
            let t = h * 5 / 8;
            fill_rect(bitmap, w, h, 0, h - t.max(1), w, h, 255);
        }
        0x2586 => {
            let t = h * 3 / 4;
            fill_rect(bitmap, w, h, 0, h - t.max(1), w, h, 255);
        }
        0x2587 => {
            let t = h * 7 / 8;
            fill_rect(bitmap, w, h, 0, h - t.max(1), w, h, 255);
        }
        // U+2588  █  Full block
        0x2588 => fill_rect(bitmap, w, h, 0, 0, w, h, 255),
        // U+2589–U+258F  Left 7/8 … 1/8 blocks
        0x2589 => {
            let t = w * 7 / 8;
            fill_rect(bitmap, w, h, 0, 0, t.max(1), h, 255);
        }
        0x258A => {
            let t = w * 3 / 4;
            fill_rect(bitmap, w, h, 0, 0, t.max(1), h, 255);
        }
        0x258B => {
            let t = w * 5 / 8;
            fill_rect(bitmap, w, h, 0, 0, t.max(1), h, 255);
        }
        0x258C => fill_rect(bitmap, w, h, 0, 0, w / 2, h, 255),
        0x258D => {
            let t = w * 3 / 8;
            fill_rect(bitmap, w, h, 0, 0, t.max(1), h, 255);
        }
        0x258E => {
            let t = w / 4;
            fill_rect(bitmap, w, h, 0, 0, t.max(1), h, 255);
        }
        0x258F => {
            let t = w / 8;
            fill_rect(bitmap, w, h, 0, 0, t.max(1), h, 255);
        }
        // U+2590  ▐  Right half block
        0x2590 => fill_rect(bitmap, w, h, w / 2, 0, w, h, 255),
        // U+2591–U+2593  Shade characters
        0x2591 => fill_shade(bitmap, w, h, 64),
        0x2592 => fill_shade(bitmap, w, h, 128),
        0x2593 => fill_shade(bitmap, w, h, 191),
        // U+2594  ▔  Upper one eighth block
        0x2594 => {
            let t = (h / 8).max(1);
            fill_rect(bitmap, w, h, 0, 0, w, t, 255);
        }
        // U+2595  ▕  Right one eighth block
        0x2595 => {
            let t = (w / 8).max(1);
            fill_rect(bitmap, w, h, w - t, 0, w, h, 255);
        }
        // U+2596–U+259F  Quadrants
        0x2596 => fill_rect(bitmap, w, h, 0, h / 2, w / 2, h, 255), // lower left
        0x2597 => fill_rect(bitmap, w, h, w / 2, h / 2, w, h, 255), // lower right
        0x2598 => fill_rect(bitmap, w, h, 0, 0, w / 2, h / 2, 255), // upper left
        0x2599 => {
            // UL + LL + LR
            fill_rect(bitmap, w, h, 0, 0, w / 2, h / 2, 255);
            fill_rect(bitmap, w, h, 0, h / 2, w / 2, h, 255);
            fill_rect(bitmap, w, h, w / 2, h / 2, w, h, 255);
        }
        0x259A => {
            // UL + LR
            fill_rect(bitmap, w, h, 0, 0, w / 2, h / 2, 255);
            fill_rect(bitmap, w, h, w / 2, h / 2, w, h, 255);
        }
        0x259B => {
            // UL + UR + LL
            fill_rect(bitmap, w, h, 0, 0, w / 2, h / 2, 255);
            fill_rect(bitmap, w, h, w / 2, 0, w, h / 2, 255);
            fill_rect(bitmap, w, h, 0, h / 2, w / 2, h, 255);
        }
        0x259C => {
            // UL + UR + LR
            fill_rect(bitmap, w, h, 0, 0, w / 2, h / 2, 255);
            fill_rect(bitmap, w, h, w / 2, 0, w, h / 2, 255);
            fill_rect(bitmap, w, h, w / 2, h / 2, w, h, 255);
        }
        0x259D => fill_rect(bitmap, w, h, w / 2, 0, w, h / 2, 255), // upper right
        0x259E => {
            // UR + LL
            fill_rect(bitmap, w, h, w / 2, 0, w, h / 2, 255);
            fill_rect(bitmap, w, h, 0, h / 2, w / 2, h, 255);
        }
        0x259F => {
            // UR + LL + LR
            fill_rect(bitmap, w, h, w / 2, 0, w, h / 2, 255);
            fill_rect(bitmap, w, h, 0, h / 2, w / 2, h, 255);
            fill_rect(bitmap, w, h, w / 2, h / 2, w, h, 255);
        }
        _ => return false,
    }
    true
}

fn fill_shade(bitmap: &mut [u8], w: u32, h: u32, alpha: u8) {
    fill_rect(bitmap, w, h, 0, 0, w, h, alpha);
}
mod box_drawing;
use box_drawing::draw_box_drawing;
