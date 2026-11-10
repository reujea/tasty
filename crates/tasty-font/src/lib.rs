#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use cosmic_text::{Attrs, Buffer, FamilyOwned, FontSystem, Metrics, Shaping, SwashCache};
use tasty_type_geometry::length::PhysicalPx;

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

/// Family name given to the faces of the custom font file. A name of its own
/// keeps a system font or an earlier file with the same name from being picked
/// instead of the file.
pub const CUSTOM_FILE_FAMILY: &str = "Tasty Custom Font File";

/// Reads the custom file and judges the face the terminal would use. Returns
/// the bytes and that face's number, or `None` after logging why it is skipped.
fn usable_custom_file(path: &str, family: &str) -> Option<(Vec<u8>, u32)> {
    let bytes = std::fs::read(path)
        .map_err(|error| tracing::warn!("Failed to load custom font file {}: {}", path, error))
        .ok()?;
    match judge::judge_terminal_bytes(bytes.clone(), family) {
        Ok(index) => Some((bytes, index)),
        Err(reason) => {
            tracing::warn!("Failed to load custom font file {}: {}", path, reason);
            None
        }
    }
}

/// Registers `chosen` under `alias` in the regular slot (normal weight, style
/// and stretch), so plain text uses it whatever its own weight is, and keeps a
/// copy with its own weight and style when those differ. Each of `others` is
/// added under `alias` unless its weight and style take one of those slots.
/// Returns the IDs added.
fn push_chosen_first(
    db: &mut cosmic_text::fontdb::Database,
    chosen: cosmic_text::fontdb::FaceInfo,
    others: impl IntoIterator<Item = cosmic_text::fontdb::FaceInfo>,
    alias: &str,
) -> Vec<cosmic_text::fontdb::ID> {
    let regular = (
        cosmic_text::fontdb::Weight::NORMAL,
        cosmic_text::fontdb::Style::Normal,
    );
    let variant = |face: &cosmic_text::fontdb::FaceInfo| (face.weight, face.style);
    let mut kept = Vec::new();
    let mut push = |db: &mut cosmic_text::fontdb::Database,
                    mut info: cosmic_text::fontdb::FaceInfo| {
        info.families = vec![(
            alias.to_string(),
            cosmic_text::fontdb::Language::English_UnitedStates,
        )];
        kept.push(db.push_face_info(info));
    };
    let mut as_regular = chosen.clone();
    (as_regular.weight, as_regular.style) = regular;
    as_regular.stretch = cosmic_text::fontdb::Stretch::Normal;
    push(db, as_regular);
    if variant(&chosen) != regular {
        push(db, chosen.clone());
    }
    for info in others {
        let taken = variant(&info) == regular || variant(&info) == variant(&chosen);
        if !taken {
            push(db, info);
        }
    }
    kept
}

/// Keeps the face numbered `index` and the faces of its family under
/// `CUSTOM_FILE_FAMILY` and drops the rest. The chosen face takes the regular
/// slot, so plain text uses the face the settings row judged and the preview
/// draws; it also keeps its own weight and style when those differ. A face of
/// the family in either of those slots is dropped. Returns the kept IDs and the
/// family the chosen face had.
fn rename_chosen_faces(
    db: &mut cosmic_text::fontdb::Database,
    ids: &[cosmic_text::fontdb::ID],
    index: u32,
) -> (Vec<cosmic_text::fontdb::ID>, Option<String>) {
    let chosen_family = ids
        .iter()
        .filter_map(|id| db.face(*id))
        .find(|face| face.index == index)
        .and_then(|face| face.families.first())
        .map(|(name, _)| name.clone());
    let faces: Vec<_> = ids.iter().filter_map(|id| db.face(*id).cloned()).collect();
    for id in ids {
        db.remove_face(*id);
    }
    let Some(chosen) = faces.iter().find(|face| face.index == index).cloned() else {
        return (Vec::new(), chosen_family);
    };
    let others = faces.into_iter().filter(|info| {
        info.index != index
            && chosen_family
                .as_deref()
                .is_some_and(|name| judge::has_family(info, name))
    });
    let kept = push_chosen_first(db, chosen, others, CUSTOM_FILE_FAMILY);
    (kept, chosen_family)
}

/// Registers the faces of the custom file the terminal uses and returns their
/// IDs. The face is the one `judge_terminal_bytes` judges, so a rejected file is
/// never drawn. Faces of the same family stay for bold and italic; the other
/// faces of a collection are dropped.
fn register_custom_font_file(
    font_system: &mut FontSystem,
    path: &str,
    family: &str,
) -> Vec<cosmic_text::fontdb::ID> {
    let Some((bytes, index)) = usable_custom_file(path, family) else {
        return Vec::new();
    };
    let db = font_system.db_mut();
    let ids = db.load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
        bytes,
    )));
    let (mut kept, chosen_family) = rename_chosen_faces(db, &ids, index);
    if let Some(name) = chosen_family {
        borrow_missing_variants(db, &name, &mut kept);
    }
    tracing::info!("Loaded custom font file: {}", path);
    kept
}

/// `family` 의 면을 `alias` 이름으로 하나씩 더 등록한다. 지정 파일에 없는 글자를 미리보기와 같은
/// 순서(설정 family, 묶음 D2Coding)로 찾게 하는 대체 목록의 이름이다(`fallback` 모듈). 원래 면은
/// 그대로 두고, 더한 면은 `added` 에 넣어 다음 구성에서 지운다.
fn alias_family(
    db: &mut cosmic_text::fontdb::Database,
    family: &str,
    alias: &str,
    added: &mut Vec<cosmic_text::fontdb::ID>,
) {
    let faces: Vec<_> = db
        .faces()
        .filter(|face| !added.contains(&face.id) && judge::has_family(face, family))
        .cloned()
        .collect();
    for mut info in faces {
        info.families = vec![(
            alias.to_string(),
            cosmic_text::fontdb::Language::English_UnitedStates,
        )];
        added.push(db.push_face_info(info));
    }
}

/// 설정 family 의 면을 `alias` 이름으로 더 등록한다. 대체 단계는 요청 굵기와 같은 면만 쓰므로,
/// 미리보기가 쓰는 면(`load_family_data` 처럼 DB 에서 그 family 의 첫 면)을 굵기와 관계없이 보통
/// 굵기·보통 기울임 자리에 둔다. 다른 굵기·기울임은 그 family 에 그 면이 있으면 그 면을 쓴다.
fn alias_settings_family(
    db: &mut cosmic_text::fontdb::Database,
    family: &str,
    alias: &str,
    added: &mut Vec<cosmic_text::fontdb::ID>,
) {
    let mut faces = db
        .faces()
        .filter(|face| !added.contains(&face.id) && judge::has_family(face, family))
        .cloned()
        .collect::<Vec<_>>()
        .into_iter();
    let Some(chosen) = faces.next() else {
        return;
    };
    added.extend(push_chosen_first(db, chosen, faces, alias));
}

/// cosmic-text 는 굵기·기울임이 맞는 면이 없는 family 를 대체 글꼴(비례폭일 수 있다)로 넘긴다. 파일에
/// 없는 굵기·기울임은 같은 원래 이름의 다른 면(설치·묶음 글꼴)을 지정 파일 이름으로 하나 더 등록해
/// 쓴다. 원래 면은 그대로 두고, 더한 면은 다음 구성에서 지운다.
fn borrow_missing_variants(
    db: &mut cosmic_text::fontdb::Database,
    family: &str,
    kept: &mut Vec<cosmic_text::fontdb::ID>,
) {
    let variant = |face: &cosmic_text::fontdb::FaceInfo| (face.weight, face.style);
    let mut have: Vec<_> = kept
        .iter()
        .filter_map(|id| db.face(*id))
        .map(variant)
        .collect();
    let borrowed: Vec<_> = db
        .faces()
        .filter(|face| !kept.contains(&face.id) && judge::has_family(face, family))
        .cloned()
        .collect();
    for mut info in borrowed {
        if have.contains(&variant(&info)) {
            continue;
        }
        have.push(variant(&info));
        info.families = vec![(
            CUSTOM_FILE_FAMILY.to_string(),
            cosmic_text::fontdb::Language::English_UnitedStates,
        )];
        kept.push(db.push_face_info(info));
    }
}

/// Values a font configuration is built from. Both the first build and every
/// later reconfiguration take the whole set, so no value can be left out on one
/// path only.
#[derive(Debug, Clone, PartialEq)]
pub struct FontRequest {
    /// Size after display scaling, not the point size of the setting.
    pub size: PhysicalPx,
    /// The family as written in the settings. Empty or `monospace` means the bundled font.
    pub family: String,
    /// Empty means no custom font file.
    pub custom_font_path: String,
    /// Multiplier of the size giving the line height.
    pub line_height: f32,
}

impl FontRequest {
    /// A request without a custom font file and with single line height.
    pub fn new(size: PhysicalPx, family: &str) -> Self {
        Self {
            size,
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
    /// Faces registered from the custom file, removed on the next reconfiguration.
    custom_faces: Vec<cosmic_text::fontdb::ID>,
}

impl FontConfig {
    /// Create a new FontConfig with the given font size and family name.
    /// If `font_family` is empty or "monospace", the bundled D2Coding ligature font is used.
    pub fn new(font_size: f32, font_family: &str) -> Self {
        Self::from_request(&FontRequest::new(PhysicalPx(font_size), font_family))
    }

    /// Create a new FontConfig from every value of the request.
    pub fn from_request(request: &FontRequest) -> Self {
        // 시스템 글꼴은 한 번만 읽고, 대체 목록만 지정 파일용으로 바꿔 다시 묶는다.
        let (locale, db) = FontSystem::new().into_locale_and_db();
        let mut font_system = FontSystem::new_with_locale_and_db_and_fallback(
            locale,
            db,
            fallback::TerminalFallback::new(),
        );
        font_system
            .db_mut()
            .load_font_data(D2CODING_REGULAR_TTF.to_vec());
        font_system
            .db_mut()
            .load_font_data(D2CODING_BOLD_TTF.to_vec());
        let mut custom_faces = Vec::new();
        let (font_family, metrics) = Self::resolve(&mut font_system, &mut custom_faces, request);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            metrics,
            font_family,
            request: request.clone(),
            custom_faces,
        }
    }

    /// Reconfigure while reusing the system font database.
    /// A non-empty custom font path is read again on every call.
    pub fn reconfigure(&mut self, request: &FontRequest) {
        let (font_family, metrics) =
            Self::resolve(&mut self.font_system, &mut self.custom_faces, request);
        self.metrics = metrics;
        self.font_family = font_family;
        self.swash_cache = SwashCache::new();
        self.request = request.clone();
    }

    /// The request the current state was built from.
    pub fn request(&self) -> &FontRequest {
        &self.request
    }

    /// Replaces the custom file's faces, resolves the family and measures a
    /// cell. The first build and every reconfiguration go through here. A usable
    /// custom file comes first, as in the settings preview; otherwise the family,
    /// and the bundled font when no family is named.
    fn resolve(
        font_system: &mut FontSystem,
        custom_faces: &mut Vec<cosmic_text::fontdb::ID>,
        request: &FontRequest,
    ) -> (FamilyOwned, FontMetrics) {
        for id in custom_faces.drain(..) {
            font_system.db_mut().remove_face(id);
        }
        if !request.custom_font_path.is_empty() {
            *custom_faces =
                register_custom_font_file(font_system, &request.custom_font_path, &request.family);
        }
        if !custom_faces.is_empty() {
            let db = font_system.db_mut();
            if judge::names_a_font(&request.family) {
                alias_settings_family(
                    db,
                    &request.family,
                    fallback::CUSTOM_FALLBACK_FAMILY,
                    custom_faces,
                );
            }
            alias_family(
                db,
                D2CODING_FAMILY,
                fallback::CUSTOM_FALLBACK_BUNDLED,
                custom_faces,
            );
        }
        let name = if !custom_faces.is_empty() {
            CUSTOM_FILE_FAMILY
        } else if judge::names_a_font(&request.family) {
            &request.family
        } else {
            D2CODING_FAMILY
        };
        let family = FamilyOwned::Name(name.to_string().into());
        let metrics = Self::measure_cell(
            font_system,
            request.size.value(),
            &family,
            request.line_height,
        );
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
mod fallback;
mod judge;
#[cfg(test)]
mod test_fonts;
#[cfg(feature = "gpu")]
pub use atlas::GlyphAtlas;
pub use judge::{
    TerminalFontRejection, file_provides_family, judge_terminal_bytes, judge_terminal_file,
    terminal_face_index,
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
            request: FontRequest::new(PhysicalPx(14.0), D2CODING_FAMILY),
            custom_faces: Vec::new(),
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
            size: PhysicalPx(14.0),
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
            ..FontRequest::new(PhysicalPx(14.0), "")
        });
        assert_eq!(config.metrics.cell_height, 21.0);
    }

    /// 그 글자를 그린 면. 칸 값은 이 컴퓨터의 글꼴에 따라 우연히 맞을 수 있어 면으로 단언한다.
    fn drawn_face(config: &mut FontConfig, text: &str, bold: bool) -> cosmic_text::fontdb::ID {
        let metrics = Metrics::new(14.0, 14.0);
        let mut buffer = Buffer::new(&mut config.font_system, metrics);
        let mut attrs = Attrs::new().family(config.font_family.as_family());
        if bold {
            attrs = attrs.weight(cosmic_text::Weight::BOLD);
        }
        buffer.set_text(
            &mut config.font_system,
            text,
            &attrs,
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut config.font_system, false);
        let run = buffer.layout_runs().next().expect("한 줄이 있어야 한다");
        run.glyphs.first().expect("글리프가 있어야 한다").font_id
    }

    fn with_file(path: &str, family: &str) -> FontRequest {
        FontRequest {
            custom_font_path: path.to_string(),
            ..FontRequest::new(PhysicalPx(14.0), family)
        }
    }

    fn uses_the_file(config: &mut FontConfig) -> bool {
        let id = drawn_face(config, "M", false);
        config.custom_faces.contains(&id)
    }

    /// family 를 비워도, 다른 이름이어도, 같은 이름의 글꼴이 이미 있어도 지정한 파일로 그린다.
    #[test]
    fn the_terminal_draws_with_the_specified_file() {
        use crate::test_fonts::{TempFont, half_units_per_em, renamed};
        let unique = TempFont::new(&half_units_per_em(renamed(D2CODING_REGULAR_TTF.to_vec())));
        let same_name = TempFont::new(&half_units_per_em(D2CODING_REGULAR_TTF.to_vec()));
        for (file, family) in [
            (&unique, ""),
            (&unique, "monospace"),
            (&unique, D2CODING_FAMILY),
            (&unique, crate::test_fonts::RENAMED_FAMILY),
            (&same_name, D2CODING_FAMILY),
            (&same_name, ""),
        ] {
            let mut config = FontConfig::from_request(&with_file(&file.path(), family));
            assert!(uses_the_file(&mut config), "family {family:?}");
            assert_eq!(config.metrics.cell_width, 14.0, "family {family:?}");
        }
    }

    /// 다시 적용하면 앞 파일의 면이 남지 않는다. 같은 이름의 앞 파일이 이기지 않고, 비우면 면 수가
    /// 처음 값으로 돌아온다.
    #[test]
    fn reapplying_replaces_the_previous_file() {
        use crate::test_fonts::{TempFont, half_units_per_em};
        let first = TempFont::new(D2CODING_REGULAR_TTF);
        let second = TempFont::new(&half_units_per_em(D2CODING_REGULAR_TTF.to_vec()));
        let mut config = FontConfig::new(14.0, "");
        let faces = config.font_system.db().len();
        config.reconfigure(&with_file(&first.path(), D2CODING_FAMILY));
        assert_eq!(config.metrics.cell_width, 7.0);
        config.reconfigure(&with_file(&second.path(), D2CODING_FAMILY));
        assert!(uses_the_file(&mut config));
        assert_eq!(config.metrics.cell_width, 14.0);
        config.reconfigure(&FontRequest::new(PhysicalPx(14.0), ""));
        assert!(config.custom_faces.is_empty());
        assert_eq!(config.font_system.db().len(), faces);
        assert_eq!(config.metrics.cell_width, 7.0);
    }

    /// 판정이 거절한 파일은 이름이 일치해도 그리지 않는다. 판정과 선택이 같은 함수를 쓴다.
    #[test]
    fn a_rejected_file_is_not_drawn() {
        use crate::test_fonts::{RENAMED_FAMILY, TempFont, renamed, without_table};
        for broken in [without_table(b"cmap"), without_table(b"hhea")] {
            let file = TempFont::new(&renamed(broken));
            let mut config = FontConfig::from_request(&with_file(&file.path(), RENAMED_FAMILY));
            assert!(config.custom_faces.is_empty());
            assert!(config.load_family_data(RENAMED_FAMILY).is_none());
            assert!(!uses_the_file(&mut config));
        }
        let text = TempFont::new(b"this is not a font\n");
        let config = FontConfig::from_request(&with_file(&text.path(), ""));
        assert!(config.custom_faces.is_empty());
        match &config.font_family {
            FamilyOwned::Name(name) => assert_eq!(&**name, D2CODING_FAMILY),
            other => panic!("expected the bundled family, got {other:?}"),
        }
    }

    /// Regular 만 든 파일의 굵게는 같은 원래 이름의 Bold(여기서는 묶음 D2Coding Bold)로 그린다.
    /// 빌려 온 면이 없으면 cosmic-text 가 굵기가 없는 family 를 비례폭 대체 글꼴로 넘긴다.
    #[test]
    fn bold_text_uses_the_same_family_when_the_file_has_no_bold() {
        use crate::test_fonts::TempFont;
        let file = TempFont::new(D2CODING_REGULAR_TTF);
        let mut config = FontConfig::from_request(&with_file(&file.path(), D2CODING_FAMILY));
        let regular = drawn_face(&mut config, "M", false);
        let bold = drawn_face(&mut config, "M", true);
        assert!(config.custom_faces.contains(&regular));
        assert!(config.custom_faces.contains(&bold));
        let db = config.font_system.db();
        assert_eq!(
            db.face(regular).unwrap().weight,
            cosmic_text::Weight::NORMAL
        );
        assert_eq!(db.face(bold).unwrap().weight, cosmic_text::Weight::BOLD);
    }

    /// 같은 family 의 Bold 가 0번, Regular 가 1번인 묶음에서 family 를 비우면 판정·미리보기 번호
    /// (`terminal_face_index`)·터미널 보통 글자가 모두 0번이다. 굵게도 0번이다.
    #[test]
    fn the_chosen_face_draws_plain_text() {
        use crate::test_fonts::{TempFont, collection};
        let bytes = collection(&[D2CODING_BOLD_TTF.to_vec(), D2CODING_REGULAR_TTF.to_vec()]);
        assert_eq!(judge::judge_terminal_bytes(bytes.clone(), "").unwrap(), 0);
        assert_eq!(judge::terminal_face_index(&bytes, ""), 0);
        let file = TempFont::new(&bytes);
        let mut config = FontConfig::from_request(&with_file(&file.path(), ""));
        for bold in [false, true] {
            let id = drawn_face(&mut config, "M", bold);
            assert!(config.custom_faces.contains(&id), "bold {bold}");
            assert_eq!(
                config.font_system.db().face(id).unwrap().index,
                0,
                "bold {bold}"
            );
        }
    }

    /// 그 글자를 그린 면의 이름.
    fn drawn_family(config: &mut FontConfig, text: &str) -> String {
        let id = drawn_face(config, text, false);
        let db = config.font_system.db();
        db.face(id).unwrap().families[0].0.clone()
    }

    /// 지정 파일에 없는 글자는 미리보기와 같이 설정 family, 그다음 묶음 D2Coding 이 맡는다. 표본
    /// 네 줄(라틴·한글·숫자·가나)의 글자로 잰다.
    #[test]
    fn missing_glyphs_follow_the_family_then_the_bundled_font() {
        use crate::fallback::{CUSTOM_FALLBACK_BUNDLED, CUSTOM_FALLBACK_FAMILY};
        use crate::test_fonts::{TempFont, half_units_per_em, latin_only, renamed};
        let file = TempFont::new(&latin_only(renamed(D2CODING_REGULAR_TTF.to_vec())));

        let mut no_family = FontConfig::from_request(&with_file(&file.path(), ""));
        for text in ["A", "1"] {
            assert_eq!(
                drawn_family(&mut no_family, text),
                CUSTOM_FILE_FAMILY,
                "{text}"
            );
        }
        for text in ["한", "あ"] {
            assert_eq!(
                drawn_family(&mut no_family, text),
                CUSTOM_FALLBACK_BUNDLED,
                "{text}"
            );
        }

        // 시스템 글꼴과 겹치지 않는 이름의 family 를 직접 넣는다.
        let mut named = FontConfig::new(14.0, "");
        let family_font = crate::test_fonts::renamed_as(
            half_units_per_em(D2CODING_REGULAR_TTF.to_vec()),
            "D2Codinj",
        );
        named.font_system.db_mut().load_font_data(family_font);
        named.reconfigure(&with_file(&file.path(), "D2Codinj ligature"));
        assert_eq!(drawn_family(&mut named, "A"), CUSTOM_FILE_FAMILY);
        for text in ["한", "あ"] {
            assert_eq!(
                drawn_family(&mut named, text),
                CUSTOM_FALLBACK_FAMILY,
                "{text}"
            );
        }
    }

    /// 설정 family 에 보통 굵기의 면이 없어도 파일에 없는 글자는 그 family 가 맡고, 미리보기가 쓰는
    /// 면(`load_family_data`)으로 그린다. 대체 단계는 요청 굵기와 같은 면만 쓰므로 굵은 면만 있는
    /// family 를 그대로 별칭으로 붙이면 묶음 D2Coding 으로 넘어간다.
    #[test]
    fn missing_glyphs_use_a_family_without_a_regular_face() {
        use crate::fallback::CUSTOM_FALLBACK_FAMILY;
        use crate::test_fonts::{TempFont, latin_only, renamed, renamed_as};
        let file = TempFont::new(&latin_only(renamed(D2CODING_REGULAR_TTF.to_vec())));
        let mut config = FontConfig::new(14.0, "");
        config
            .font_system
            .db_mut()
            .load_font_data(renamed_as(D2CODING_BOLD_TTF.to_vec(), "D2Codinj"));
        config.reconfigure(&with_file(&file.path(), "D2Codinj ligature"));
        let preview = config.load_family_data("D2Codinj ligature").unwrap();
        for text in ["한", "あ"] {
            let id = drawn_face(&mut config, text, false);
            let db = config.font_system.db();
            assert!(
                judge::has_family(db.face(id).unwrap(), CUSTOM_FALLBACK_FAMILY),
                "{text}"
            );
            let mut drawn = None;
            db.with_face_data(id, |data, index| drawn = Some((data.to_vec(), index)));
            let (data, index) = drawn.unwrap();
            assert!(data == preview.file && index == preview.face, "{text}");
        }
    }

    /// 지정 파일이 없으면 두 대체 이름의 면이 없어 대체 순서가 바뀌지 않는다.
    #[test]
    fn without_a_custom_file_no_fallback_alias_is_registered() {
        use crate::fallback::{CUSTOM_FALLBACK_BUNDLED, CUSTOM_FALLBACK_FAMILY};
        let config = FontConfig::new(14.0, "");
        assert!(config.load_family_data(CUSTOM_FALLBACK_FAMILY).is_none());
        assert!(config.load_family_data(CUSTOM_FALLBACK_BUNDLED).is_none());
    }

    /// 지정 파일이 없으면 빠진 글자는 플랫폼 대체 순서(로케일에 따른 한자 자형 선택 포함) 그대로
    /// 고른다. 플랫폼 목록만 쓴 같은 DB 와 결과가 같아야 한다.
    #[test]
    fn without_a_custom_file_missing_glyphs_follow_the_platform_order() {
        let family = "Tasty No Such Family";
        let mut ours = FontConfig::new(14.0, family);
        let (locale, db) = FontSystem::new().into_locale_and_db();
        let mut platform = FontConfig {
            font_system: FontSystem::new_with_locale_and_db_and_fallback(
                locale,
                db,
                cosmic_text::PlatformFallback,
            ),
            swash_cache: SwashCache::new(),
            metrics: FontMetrics {
                cell_width: ours.metrics.cell_width,
                cell_height: ours.metrics.cell_height,
                font_size: ours.metrics.font_size,
                baseline: ours.metrics.baseline,
            },
            font_family: ours.font_family.clone(),
            request: ours.request().clone(),
            custom_faces: Vec::new(),
        };
        for text in ["A", "漢", "한", "あ"] {
            assert_eq!(
                drawn_family(&mut ours, text),
                drawn_family(&mut platform, text),
                "{text}"
            );
        }
    }

    /// 묶음 파일은 family 가 가리키는 면을 쓰고, 아니면 첫 면을 쓴다.
    #[test]
    fn a_collection_uses_the_named_face_or_the_first() {
        use crate::test_fonts::{RENAMED_FAMILY, TempFont, collection, half_units_per_em, renamed};
        let file = TempFont::new(&collection(&[
            renamed(D2CODING_REGULAR_TTF.to_vec()),
            half_units_per_em(D2CODING_REGULAR_TTF.to_vec()),
        ]));
        let first = FontConfig::from_request(&with_file(&file.path(), ""));
        assert_eq!(first.metrics.cell_width, 7.0);
        let named = FontConfig::from_request(&with_file(&file.path(), D2CODING_FAMILY));
        assert_eq!(named.metrics.cell_width, 14.0);
        let other = FontConfig::from_request(&with_file(&file.path(), RENAMED_FAMILY));
        assert_eq!(other.metrics.cell_width, 7.0);
        let file_faces = other
            .custom_faces
            .iter()
            .filter_map(|id| other.font_system.db().face(*id))
            .filter(|face| judge::has_family(face, CUSTOM_FILE_FAMILY))
            .count();
        assert_eq!(file_faces, 1);
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
