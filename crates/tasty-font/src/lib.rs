#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use cosmic_text::{Attrs, Buffer, FamilyOwned, FontSystem, Metrics, Shaping, SwashCache};

/// Family name of the bundled D2Coding ligature font.
/// Must match the `family` entry in the ttf `name` table exactly
/// (lowercase `l`, single space — verified against NAVER Ver 1.3.2).
pub const D2CODING_FAMILY: &str = "D2Coding ligature";

/// D2Coding ligature Regular ttf bytes embedded at compile time (OFL 1.1).
pub const D2CODING_REGULAR_TTF: &[u8] = include_bytes!("../assets/D2Coding-ligature-Regular.ttf");

/// D2Coding ligature Bold ttf bytes embedded at compile time (OFL 1.1).
pub const D2CODING_BOLD_TTF: &[u8] = include_bytes!("../assets/D2Coding-ligature-Bold.ttf");

/// Font metrics for monospace grid layout.
pub struct FontMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub font_size: f32,
    /// Baseline position within a cell (distance from cell top to text baseline)
    pub baseline: f32,
}

/// A font file, as a path or as bytes, and the number of one face inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFileFace<F> {
    pub file: F,
    pub face: u32,
}

/// Adds the file's faces to the database. A file without faces is an error,
/// so it must not be logged as loaded.
fn register_font_file(font_system: &mut FontSystem, path: &str) -> Result<(), &'static str> {
    let data = std::fs::read(path).map_err(|_| "cannot read the file")?;
    let source = cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(data));
    if font_system.db_mut().load_font_source(source).is_empty() {
        return Err("no usable font face");
    }
    Ok(())
}

/// Reports whether any of the file's faces was registered.
fn load_custom_font_file(font_system: &mut FontSystem, path: &str) -> bool {
    match register_font_file(font_system, path) {
        Ok(()) => {
            tracing::info!("Loaded custom font file: {}", path);
            true
        }
        Err(reason) => {
            tracing::warn!("Failed to load custom font file {}: {}", path, reason);
            false
        }
    }
}

/// Values a font configuration is built from. Both the first build and every
/// later reconfiguration take the whole set, so no value can be left out on one
/// path only.
#[derive(Debug, Clone, PartialEq)]
pub struct FontRequest {
    /// Pixel size after display scaling, not the point size of the setting.
    pub size_px: f32,
    /// The family as written in the settings. Empty or `monospace` means the bundled font.
    pub family: String,
    /// Empty means no custom font file.
    pub custom_font_path: String,
    /// Multiplier of the size giving the line height.
    pub line_height: f32,
}

impl FontRequest {
    /// A request without a custom font file and with single line height.
    pub fn new(size_px: f32, family: &str) -> Self {
        Self {
            size_px,
            family: family.to_string(),
            custom_font_path: String::new(),
            line_height: 1.0,
        }
    }
}

/// Font configuration holding cosmic-text state.
pub struct FontConfig {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub metrics: FontMetrics,
    /// The font family used for rendering glyphs.
    pub font_family: FamilyOwned,
    /// The request the current metrics and family were built from.
    request: FontRequest,
}

impl FontConfig {
    /// Create a new FontConfig with the given font size and family name.
    /// If `font_family` is empty or "monospace", the bundled D2Coding ligature font is used.
    pub fn new(font_size: f32, font_family: &str) -> Self {
        Self::from_request(&FontRequest::new(font_size, font_family))
    }

    /// Create a new FontConfig from every value of the request.
    pub fn from_request(request: &FontRequest) -> Self {
        let mut font_system = FontSystem::new();
        font_system
            .db_mut()
            .load_font_data(D2CODING_REGULAR_TTF.to_vec());
        font_system
            .db_mut()
            .load_font_data(D2CODING_BOLD_TTF.to_vec());
        let (font_family, metrics) = Self::resolve(&mut font_system, request);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            metrics,
            font_family,
            request: request.clone(),
        }
    }

    /// Reconfigure while reusing the system font database.
    /// A non-empty custom font path is read again on every call.
    pub fn reconfigure(&mut self, request: &FontRequest) {
        let (font_family, metrics) = Self::resolve(&mut self.font_system, request);
        self.metrics = metrics;
        self.font_family = font_family;
        self.swash_cache = SwashCache::new();
        self.request = request.clone();
    }

    /// The request the current state was built from.
    pub fn request(&self) -> &FontRequest {
        &self.request
    }

    /// Loads the custom file, resolves the family and measures a cell. The first
    /// build and every reconfiguration go through here.
    fn resolve(font_system: &mut FontSystem, request: &FontRequest) -> (FamilyOwned, FontMetrics) {
        if !request.custom_font_path.is_empty() {
            load_custom_font_file(font_system, &request.custom_font_path);
        }
        let family =
            if request.family.is_empty() || request.family.eq_ignore_ascii_case("monospace") {
                FamilyOwned::Name(D2CODING_FAMILY.to_string().into())
            } else {
                FamilyOwned::Name(request.family.clone().into())
            };
        let metrics =
            Self::measure_cell(font_system, request.size_px, &family, request.line_height);
        (family, metrics)
    }

    /// Bytes of the file providing `family` and the number of the matching face
    /// inside it. A collection holds several families, so the bytes alone name
    /// none of them. Returns `None` when no face of that family can be read.
    pub fn load_family_data(&self, family: &str) -> Option<FontFileFace<Vec<u8>>> {
        for face in self.font_system.db().faces() {
            for (name, _) in &face.families {
                if name.eq_ignore_ascii_case(family) {
                    let mut result = None;
                    self.font_system
                        .db()
                        .with_face_data(face.id, |data, index| {
                            result = Some(FontFileFace {
                                file: data.to_vec(),
                                face: index,
                            });
                        });
                    if result.is_some() {
                        return result;
                    }
                }
            }
        }
        None
    }

    /// On-disk path of a font file providing `family` and the number of the
    /// matching face, when that face is backed by a file on disk. Used to resolve
    /// a language pack's `[font] family = …` to a concrete file so the same face
    /// can be handed to plugin processes via `TASTY_LOCALE_FONT` and
    /// `TASTY_LOCALE_FONT_FACE` (they must not re-search the system DB). Returns
    /// `None` for in-memory faces or unknown families.
    pub fn family_source_path(&self, family: &str) -> Option<FontFileFace<std::path::PathBuf>> {
        for face in self.font_system.db().faces() {
            for (name, _) in &face.families {
                if name.eq_ignore_ascii_case(family)
                    && let cosmic_text::fontdb::Source::File(path) = &face.source
                {
                    return Some(FontFileFace {
                        file: path.clone(),
                        face: face.index,
                    });
                }
            }
        }
        None
    }

    /// List all available font family names from the system, sorted alphabetically.
    pub fn list_families(&self) -> Vec<String> {
        let mut families = BTreeSet::new();
        for face in self.font_system.db().faces() {
            for (name, _) in &face.families {
                families.insert(name.clone());
            }
        }
        families.into_iter().collect()
    }

    fn measure_cell(
        font_system: &mut FontSystem,
        font_size: f32,
        family: &FamilyOwned,
        line_height_mult: f32,
    ) -> FontMetrics {
        let line_height = (font_size * line_height_mult).ceil();
        let cosmic_metrics = Metrics::new(font_size, line_height);

        let mut buffer = Buffer::new(font_system, cosmic_metrics);
        buffer.set_size(font_system, Some(font_size * 10.0), Some(line_height * 2.0));
        buffer.set_text(
            font_system,
            "M",
            &Attrs::new().family(family.as_family()),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(font_system, false);

        let mut cell_width = font_size * 0.6; // fallback
        let mut baseline = line_height * 0.8; // fallback
        if let Some(run) = buffer.layout_runs().next() {
            if let Some(glyph) = run.glyphs.first() {
                cell_width = glyph.w;
            }
            // line_y is the baseline position within the layout
            baseline = run.line_y;
        }

        FontMetrics {
            cell_width: cell_width.ceil(),
            cell_height: line_height,
            font_size,
            baseline,
        }
    }
}

/// Key for glyph cache lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub ch: char,
    pub bold: bool,
    pub italic: bool,
}

/// Location of a glyph within the atlas texture.
#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    /// UV coordinates in 0..1 range
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_w: f32,
    pub uv_h: f32,
    /// Pixel offset from cell origin to place the glyph bitmap
    pub offset_x: f32,
    pub offset_y: f32,
    /// Pixel size of the glyph bitmap
    pub width: f32,
    pub height: f32,
    /// Atlas page (D2Array layer) the glyph lives on.
    pub page: u32,
}

/// Per-page shelf-packing state. Pure-Rust — no wgpu dependency, so it
/// can be unit-tested without a device.
#[derive(Debug, Clone, Copy, Default)]
pub struct AtlasPage {
    pub shelf_x: u32,
    pub shelf_y: u32,
    pub shelf_height: u32,
    /// Number of cache entries currently allocated on this page (diagnostic).
    pub entry_count: u32,
    /// Frame index when this page was last touched (cache hit OR insertion).
    /// Used as the LRU tiebreaker when picking an eviction victim.
    pub last_access_frame: u64,
}

impl AtlasPage {
    /// Try to reserve a (w × h) box on this page.
    /// Returns the (x, y) origin if there is room; mutates the shelf cursor.
    pub fn try_allocate(&mut self, w: u32, h: u32, atlas_size: u32) -> Option<(u32, u32)> {
        if w == 0 || h == 0 || w > atlas_size || h > atlas_size {
            return None;
        }
        // Advance shelf if the current row can't fit horizontally.
        if self.shelf_x + w > atlas_size {
            self.shelf_y = self.shelf_y.saturating_add(self.shelf_height + 1);
            self.shelf_x = 0;
            self.shelf_height = 0;
        }
        if self.shelf_y + h > atlas_size {
            return None;
        }
        let origin = (self.shelf_x, self.shelf_y);
        self.shelf_x += w + 1;
        self.shelf_height = self.shelf_height.max(h);
        self.entry_count += 1;
        Some(origin)
    }

    /// Reset this page's packing state (used after eviction).
    pub fn reset(&mut self) {
        self.shelf_x = 0;
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.entry_count = 0;
    }
}

/// Pick the eviction victim across `pages`, excluding the currently active
/// page. The victim is the page with the smallest `last_access_frame` (true
/// per-page LRU). Returns `None` if there is no non-active page.
pub fn pick_lru_victim(pages: &[AtlasPage], active_page: u32) -> Option<u32> {
    pages
        .iter()
        .enumerate()
        .filter(|(i, _)| (*i as u32) != active_page)
        .min_by_key(|(_, p)| p.last_access_frame)
        .map(|(i, _)| i as u32)
}

/// Frame stamps and the once-per-frame eviction limit, testable without a GPU device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameClock {
    current: u64,
    last_evict: Option<u64>,
}

impl FrameClock {
    /// Advance to the next frame. This is the *only* thing that re-arms the
    /// eviction throttle: a clock that is never advanced refuses every
    /// eviction after its first one, for as long as the atlas lives.
    pub fn begin_frame(&mut self) {
        // Wrapping, not saturating: saturating would freeze the counter at
        // `u64::MAX` and latch the throttle exactly the same way.
        self.current = self.current.wrapping_add(1);
    }

    /// Current frame stamp, written onto every page this frame touches.
    pub fn now(&self) -> u64 {
        self.current
    }

    /// Whether a page was already evicted in the current frame. Evicting more
    /// than once per frame thrashes the atlas, so the second one is refused.
    pub fn evicted_this_frame(&self) -> bool {
        self.last_evict == Some(self.current)
    }

    /// Record that a page was evicted in the current frame.
    pub fn note_eviction(&mut self) {
        self.last_evict = Some(self.current);
    }
}

/// GPU 아틀라스. 헤드리스 소비자는 gpu 기능을 켜지 않아 wgpu 의존성을 제외한다.
#[cfg(feature = "gpu")]
mod atlas;
mod judge;
#[cfg(test)]
mod test_fonts;
#[cfg(feature = "gpu")]
pub use atlas::GlyphAtlas;
pub use judge::{
    TerminalFontRejection, file_provides_family, judge_terminal_bytes, judge_terminal_file,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_config_default_monospace() {
        let config = FontConfig::new(14.0, "");
        match &config.font_family {
            FamilyOwned::Name(name) => assert_eq!(&**name, D2CODING_FAMILY),
            other => panic!("expected bundled D2Coding family, got {other:?}"),
        }
        assert_eq!(config.metrics.font_size, 14.0);
        assert!(config.metrics.cell_width > 0.0);
        assert!(config.metrics.cell_height > 0.0);
    }

    #[test]
    fn font_config_explicit_monospace() {
        let config = FontConfig::new(14.0, "monospace");
        match &config.font_family {
            FamilyOwned::Name(name) => assert_eq!(&**name, D2CODING_FAMILY),
            other => panic!("expected bundled D2Coding family, got {other:?}"),
        }
    }

    #[test]
    fn font_config_named_family() {
        let config = FontConfig::new(16.0, "JetBrains Mono");
        assert!(matches!(config.font_family, FamilyOwned::Name(_)));
        assert_eq!(config.metrics.font_size, 16.0);
        assert!(config.metrics.cell_width > 0.0);
        assert!(config.metrics.cell_height > 0.0);
    }

    #[test]
    fn font_config_different_sizes() {
        let small = FontConfig::new(10.0, "");
        let large = FontConfig::new(24.0, "");
        assert!(large.metrics.cell_height > small.metrics.cell_height);
    }

    #[test]
    fn atlas_page_first_alloc_lands_at_origin() {
        let mut page = AtlasPage::default();
        let (x, y) = page.try_allocate(32, 32, 2048).unwrap();
        assert_eq!((x, y), (0, 0));
        assert_eq!(page.shelf_x, 33);
        assert_eq!(page.shelf_height, 32);
        assert_eq!(page.entry_count, 1);
    }

    #[test]
    fn atlas_page_wraps_to_next_shelf_when_row_full() {
        let mut page = AtlasPage::default();
        // 2048 / 100 = 20 glyphs per shelf row; the 21st must wrap.
        for _ in 0..20 {
            page.try_allocate(100, 50, 2048).unwrap();
        }
        let before_y = page.shelf_y;
        let (_, y) = page.try_allocate(100, 50, 2048).unwrap();
        assert!(y > before_y, "expected wrap to next shelf, got y={y}");
        assert_eq!(page.shelf_x, 101);
    }

    #[test]
    fn atlas_page_returns_none_when_full() {
        let mut page = AtlasPage::default();
        // First shelf occupies y=0..1024. Anything taller than ~1023 in the
        // next shelf no longer fits within the 2048 atlas height.
        page.try_allocate(2000, 1024, 2048).unwrap();
        assert!(page.try_allocate(100, 1100, 2048).is_none());
    }

    #[test]
    fn atlas_page_rejects_oversized_glyph() {
        let mut page = AtlasPage::default();
        assert!(page.try_allocate(3000, 32, 2048).is_none());
        assert!(page.try_allocate(32, 3000, 2048).is_none());
        assert_eq!(page.shelf_x, 0);
        assert_eq!(page.entry_count, 0);
    }

    #[test]
    fn lru_victim_picks_oldest_non_active_page() {
        let mut pages = vec![AtlasPage::default(); 4];
        pages[0].last_access_frame = 100;
        pages[1].last_access_frame = 50; // oldest
        pages[2].last_access_frame = 200;
        pages[3].last_access_frame = 75;
        let victim = pick_lru_victim(&pages, 2).unwrap();
        assert_eq!(victim, 1);
    }

    #[test]
    fn lru_victim_excludes_active_page() {
        let mut pages = vec![AtlasPage::default(); 4];
        pages[0].last_access_frame = 10; // would be oldest…
        pages[1].last_access_frame = 50;
        pages[2].last_access_frame = 200;
        pages[3].last_access_frame = 75;
        // …but 0 is the active page, so pick the next oldest.
        let victim = pick_lru_victim(&pages, 0).unwrap();
        assert_eq!(victim, 1);
    }

    #[test]
    fn lru_victim_returns_none_when_only_active_page_exists() {
        let pages = vec![AtlasPage::default(); 1];
        assert!(pick_lru_victim(&pages, 0).is_none());
    }

    #[test]
    fn frame_clock_starts_at_zero_with_eviction_available() {
        let clock = FrameClock::default();
        assert_eq!(clock.now(), 0);
        assert!(!clock.evicted_this_frame());
    }

    #[test]
    fn frame_clock_begin_frame_advances_the_stamp() {
        let mut clock = FrameClock::default();
        clock.begin_frame();
        assert_eq!(clock.now(), 1);
        clock.begin_frame();
        assert_eq!(clock.now(), 2);
    }

    #[test]
    fn frame_clock_refuses_a_second_eviction_in_the_same_frame() {
        let mut clock = FrameClock::default();
        clock.begin_frame();
        assert!(!clock.evicted_this_frame());
        clock.note_eviction();
        assert!(clock.evicted_this_frame());
    }

    #[test]
    fn frame_clock_rearms_eviction_on_the_next_frame() {
        let mut clock = FrameClock::default();
        clock.begin_frame();
        clock.note_eviction();
        assert!(clock.evicted_this_frame());
        clock.begin_frame();
        assert!(
            !clock.evicted_this_frame(),
            "a new frame must re-arm the eviction throttle"
        );
    }

    #[test]
    fn frame_clock_never_advanced_latches_the_throttle() {
        let mut clock = FrameClock::default();
        clock.note_eviction();
        for _ in 0..1000 {
            assert!(clock.evicted_this_frame());
        }
    }

    #[test]
    fn frame_clock_wraps_instead_of_saturating() {
        // Saturating would freeze the counter at `u64::MAX`, which latches the
        // throttle in exactly the way the test above describes.
        let mut clock = FrameClock {
            current: u64::MAX,
            last_evict: None,
        };
        clock.begin_frame();
        assert_eq!(clock.now(), 0);
    }

    #[test]
    fn atlas_page_reset_clears_state() {
        let mut page = AtlasPage::default();
        page.try_allocate(100, 100, 2048).unwrap();
        page.last_access_frame = 42;
        page.reset();
        assert_eq!(page.shelf_x, 0);
        assert_eq!(page.shelf_y, 0);
        assert_eq!(page.shelf_height, 0);
        assert_eq!(page.entry_count, 0);
        // Resetting the packing cursor preserves the last-use timestamp.
        assert_eq!(page.last_access_frame, 42);
    }

    /// 시스템 글꼴을 읽지 않는다. `FontConfig::new`는 시스템 글꼴을 먼저 읽어, 같은 family가
    /// 묶음 파일로 설치된 컴퓨터에서는 그 파일과 그 안의 번호를 찾는다.
    fn config_with_only_bundled_fonts() -> FontConfig {
        let mut db = cosmic_text::fontdb::Database::new();
        db.load_font_data(D2CODING_REGULAR_TTF.to_vec());
        db.load_font_data(D2CODING_BOLD_TTF.to_vec());
        let mut font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
        let font_family = FamilyOwned::Name(D2CODING_FAMILY.to_string().into());
        let metrics = FontConfig::measure_cell(&mut font_system, 14.0, &font_family, 1.0);
        FontConfig {
            font_system,
            swash_cache: SwashCache::new(),
            metrics,
            font_family,
            request: FontRequest::new(14.0, D2CODING_FAMILY),
        }
    }

    #[test]
    fn a_bundled_family_is_found_with_its_face_number() {
        let config = config_with_only_bundled_fonts();
        let found = config.load_family_data(D2CODING_FAMILY).unwrap();
        assert_eq!(found.face, 0);
        assert!(
            found.file == D2CODING_REGULAR_TTF,
            "expected the bundled regular font, got {} bytes",
            found.file.len()
        );
        assert!(config.load_family_data("No Such Family").is_none());
    }

    /// 시스템 글꼴 상태와 무관하게 통과한다. 번들 적재가 빠지면 같은 family가 설치되지 않은
    /// 컴퓨터에서 실패한다. 번호는 설치된 글꼴에 따라 달라 묻지 않는다.
    #[test]
    fn the_default_config_has_the_bundled_family() {
        let config = FontConfig::new(14.0, "");
        assert!(config.load_family_data(D2CODING_FAMILY).is_some());
    }

    /// 저장소에 넣을 수 없는 시스템 글꼴을 읽는다. 설치되지 않은 컴퓨터에서는 아무것도 재지 않는다.
    #[test]
    fn a_family_inside_a_collection_keeps_its_face_number() {
        let config = FontConfig::new(14.0, "");
        let (Some(simplified), Some(japanese)) = (
            config.family_source_path("Noto Sans CJK SC"),
            config.family_source_path("Noto Sans CJK JP"),
        ) else {
            return;
        };
        assert_ne!(simplified.face, japanese.face);
        let data = config.load_family_data("Noto Sans CJK SC").unwrap();
        assert_eq!(data.face, simplified.face);
    }

    fn saved_values(path: &str) -> FontRequest {
        FontRequest {
            size_px: 14.0,
            family: crate::test_fonts::RENAMED_FAMILY.to_string(),
            custom_font_path: path.to_string(),
            line_height: 1.5,
        }
    }

    /// 첫 구성이 파일과 행간을 모두 적용한다. 칸 폭은 unitsPerEm 을 절반으로 줄인 파일이라야
    /// 묶음 D2Coding(7)과 구별된다.
    #[test]
    fn the_first_build_applies_the_file_and_the_line_height() {
        use crate::test_fonts::{TempFont, half_units_per_em, renamed};
        let file = TempFont::new(&half_units_per_em(renamed(D2CODING_REGULAR_TTF.to_vec())));
        let request = saved_values(&file.path());
        let config = FontConfig::from_request(&request);
        assert_eq!(config.metrics.cell_height, 21.0);
        assert_eq!(config.metrics.cell_width, 14.0);
        assert_eq!(config.request(), &request);
    }

    /// 첫 구성과 다시 구성이 같은 값을 낸다. 두 경로가 다른 코드를 쓰면 여기서 갈린다.
    #[test]
    fn rebuilding_gives_the_same_cell_as_the_first_build() {
        use crate::test_fonts::{TempFont, half_units_per_em, renamed};
        let file = TempFont::new(&half_units_per_em(renamed(D2CODING_REGULAR_TTF.to_vec())));
        let request = saved_values(&file.path());
        let first = FontConfig::from_request(&request);
        let mut rebuilt = FontConfig::new(14.0, "");
        rebuilt.reconfigure(&request);
        assert_eq!(
            (rebuilt.metrics.cell_width, rebuilt.metrics.cell_height),
            (first.metrics.cell_width, first.metrics.cell_height)
        );
        assert_eq!(rebuilt.request(), &request);
    }

    /// 없는 파일은 건너뛰고 행간은 그대로 적용한다.
    #[test]
    fn a_missing_file_is_skipped_without_dropping_the_line_height() {
        let config = FontConfig::from_request(&FontRequest {
            custom_font_path: "/nonexistent/tasty-font-test.ttf".to_string(),
            line_height: 1.5,
            ..FontRequest::new(14.0, "")
        });
        assert_eq!(config.metrics.cell_height, 21.0);
    }

    /// 실행된 기능 조합만으로는 기본값·선택적 의존 선언을 알 수 없어 매니페스트를 읽는다.
    #[test]
    fn wgpu_stays_an_optional_dependency_of_this_crate() {
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        )
        .expect("이 크레이트의 Cargo.toml을 읽지 못했다");

        let wgpu_line = manifest
            .lines()
            .find(|l| l.trim_start().starts_with("wgpu = "))
            .expect("wgpu 의존 선언이 없다. 의존성이 제거됐다면 이 검사도 검토해야 한다");
        assert!(
            wgpu_line.contains("optional = true"),
            "wgpu는 선택적 의존이어야 한다. 헤드리스 소비자에 GPU 의존성을 추가하지 않는다: {wgpu_line}"
        );
        assert!(
            manifest.contains("\ngpu = [\"dep:wgpu\"]"),
            "`gpu` feature 선언이 없어졌다 — `optional` 만으로는 소비자가 켤 수단이 없다"
        );
        assert!(
            manifest.contains("\ndefault = []"),
            "기본 기능은 비어 있어야 한다. GPU 소비자가 gpu 기능을 명시적으로 켠다"
        );
    }
}
