//! 터미널 글꼴 경로가 파일이나 이름을 실제로 그릴 수 있는지 판정한다.
//!
//! face 가 등록되어도 터미널이 그 파일로 그린다는 뜻이 아니다. cmap 이 없으면 셰이핑이 다른 글꼴로
//! 넘어가고 hhea 가 없으면 래스터가 비어 있다. 그래서 그 파일만 든 `FontSystem` 에서 파일의 cmap 에 있는
//! 글자를 셰이핑하고 래스터까지 확인한다. 시스템 글꼴이 없으니 다른 글꼴로의 대체도 없다.
//! 일부 글리프만 손상된 파일은 그릴 수 있는 글자가 있으므로 통과한다.

use std::sync::Arc;

use cosmic_text::{Attrs, Buffer, Family, Font, FontSystem, Metrics, Shaping, SwashCache, fontdb};

use crate::FontConfig;

/// 터미널이 그 값을 쓰지 못하는 이유.
#[derive(Debug)]
pub enum TerminalFontRejection {
    Read(std::io::Error),
    /// fontdb 가 face 를 하나도 등록하지 못했다.
    NoFace,
    /// face 는 있지만 어느 face 도 글자를 그리지 못한다.
    CannotDraw,
    /// 그 이름의 글꼴이 없다.
    FamilyNotFound,
}

impl std::fmt::Display for TerminalFontRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "cannot read font file: {error}"),
            Self::NoFace => write!(f, "no font face in the file"),
            Self::CannotDraw => write!(f, "no face can draw a glyph"),
            Self::FamilyNotFound => write!(f, "no font with this family name"),
        }
    }
}

impl std::error::Error for TerminalFontRejection {}

// 재는 크기는 판정에 영향을 주지 않는다. 래스터가 나올 만큼 크면 된다.
const PROBE_SIZE: f32 = 14.0;
const PROBE_LINE_HEIGHT: f32 = 17.0;
/// 빈 글리프만 고르는 일을 피하려고 cmap 전체에 고르게 퍼뜨려 보는 글자 수의 상한.
const PROBE_CHARS: usize = 32;

/// 그 글자의 칸에 찍히는 픽셀이 있는지 본다. 글리프 0 은 글자가 없다는 뜻이라 세지 않는다.
fn draws_char(system: &mut FontSystem, swash: &mut SwashCache, family: &str, ch: char) -> bool {
    let mut buffer = Buffer::new(system, Metrics::new(PROBE_SIZE, PROBE_LINE_HEIGHT));
    let mut text = [0u8; 4];
    buffer.set_text(
        system,
        ch.encode_utf8(&mut text),
        &Attrs::new().family(Family::Name(family)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(system, false);
    let keys: Vec<_> = buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.iter())
        .filter(|glyph| glyph.glyph_id != 0)
        .map(|glyph| glyph.physical((0.0, 0.0), 1.0).cache_key)
        .collect();
    keys.into_iter().any(|key| {
        swash
            .get_image_uncached(system, key)
            .is_some_and(|image| image.data.iter().any(|byte| *byte != 0))
    })
}

/// 파일 자기 cmap 에 있는 글자들. 라틴 글자가 없는 기호 글꼴도 그 글꼴의 글자로 잰다.
fn probe_chars(font: &Font) -> Vec<char> {
    let mut chars = Vec::new();
    font.as_swash().charmap().enumerate(|code, glyph| {
        if glyph != 0
            && let Some(ch) = char::from_u32(code)
            && !ch.is_whitespace()
            && !ch.is_control()
        {
            chars.push(ch);
        }
    });
    let step = (chars.len() / PROBE_CHARS).max(1);
    chars.into_iter().step_by(step).collect()
}

/// 이 바이트로 터미널이 글자를 그릴 수 있는지 판정한다.
pub fn judge_terminal_bytes(bytes: Vec<u8>) -> Result<(), TerminalFontRejection> {
    let mut db = fontdb::Database::new();
    let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
    if ids.is_empty() {
        return Err(TerminalFontRejection::NoFace);
    }
    let mut system = FontSystem::new_with_locale_and_db("en".to_owned(), db);
    let mut swash = SwashCache::new();
    for id in ids {
        let Some(font) = Font::new(system.db(), id, fontdb::Weight::NORMAL) else {
            continue;
        };
        let chars = probe_chars(&font);
        let Some(family) = system
            .db()
            .face(id)
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone())
        else {
            continue;
        };
        if chars
            .into_iter()
            .any(|ch| draws_char(&mut system, &mut swash, &family, ch))
        {
            return Ok(());
        }
    }
    Err(TerminalFontRejection::CannotDraw)
}

/// 이 경로의 파일로 터미널이 글자를 그릴 수 있는지 판정한다.
pub fn judge_terminal_file(path: &str) -> Result<(), TerminalFontRejection> {
    let bytes = std::fs::read(path).map_err(TerminalFontRejection::Read)?;
    judge_terminal_bytes(bytes)
}

fn has_family(face: &fontdb::FaceInfo, family: &str) -> bool {
    face.families
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case(family))
}

fn bytes_provide_family(bytes: &[u8], family: &str) -> bool {
    let mut db = fontdb::Database::new();
    db.load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));
    db.faces().any(|face| has_family(face, family))
}

/// 이 파일이 그 이름의 글꼴을 제공하는지 본다. 값이 비었거나 읽지 못하면 거짓이다.
pub fn file_provides_family(path: &str, family: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    std::fs::read(path).is_ok_and(|bytes| bytes_provide_family(&bytes, family))
}

impl FontConfig {
    /// 터미널이 이 이름을 쓸 수 있는지 판정한다. `custom_path` 는 이 행의 파일이다. 그 파일이 제공하는
    /// 이름은 시스템 목록에 없으므로 함께 찾는다. 다른 행의 파일이 끼지 않도록 파일은 이 호출 안에서만
    /// 보고 `self` 에 넣지 않는다. `self` 는 custom 파일을 넣은 적이 없는 구성이어야 한다.
    pub fn judge_terminal_family(
        &self,
        family: &str,
        custom_path: &str,
    ) -> Result<(), TerminalFontRejection> {
        let mut found = false;
        if !custom_path.is_empty()
            && let Ok(bytes) = std::fs::read(custom_path)
            && bytes_provide_family(&bytes, family)
        {
            found = true;
            if judge_terminal_bytes(bytes).is_ok() {
                return Ok(());
            }
        }
        let db = self.font_system.db();
        let faces: Vec<_> = db
            .faces()
            .filter(|face| has_family(face, family))
            .map(|face| face.id)
            .collect();
        found |= !faces.is_empty();
        for id in faces {
            let mut bytes = None;
            db.with_face_data(id, |data, _| bytes = Some(data.to_vec()));
            if bytes.is_some_and(|bytes| judge_terminal_bytes(bytes).is_ok()) {
                return Ok(());
            }
        }
        Err(if found {
            TerminalFontRejection::CannotDraw
        } else {
            TerminalFontRejection::FamilyNotFound
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::D2CODING_REGULAR_TTF as VALID;

    fn table_record(font: &[u8], tag: &[u8; 4]) -> usize {
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&r| &font[r..r + 4] == tag)
            .unwrap()
    }

    fn table(font: &[u8], tag: &[u8; 4]) -> (usize, usize) {
        let r = table_record(font, tag);
        let word = |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
        (word(r + 8), word(r + 12))
    }

    /// 표 레코드 이름을 바꿔 파서가 그 표를 찾지 못하게 한다.
    fn without_table(tag: &[u8; 4]) -> Vec<u8> {
        let mut font = VALID.to_vec();
        let record = table_record(&font, tag);
        font[record] = b'x';
        font
    }

    fn negative_height() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let (hhea, _) = table(&font, b"hhea");
        font[hhea + 4..hhea + 6].copy_from_slice(&(-1000i16).to_be_bytes());
        font[hhea + 6..hhea + 8].copy_from_slice(&1000i16.to_be_bytes());
        font
    }

    fn glyf_garbage() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let (glyf, len) = table(&font, b"glyf");
        font[glyf + len / 4..glyf + len / 2].fill(0xff);
        font
    }

    /// name 표의 글꼴 이름을 바꾼다. `D2Coding` 이 시스템 목록과 겹치지 않게 하는 데 쓴다.
    fn renamed(mut font: Vec<u8>) -> Vec<u8> {
        let (name, len) = table(&font, b"name");
        let utf16 = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_be_bytes)
                .collect::<Vec<u8>>()
        };
        let pairs = [
            (b"D2Coding".to_vec(), b"D2Codinh".to_vec()),
            (utf16("D2Coding"), utf16("D2Codinh")),
        ];
        for (from, to) in pairs {
            for start in name..name + len - from.len() {
                if font[start..start + from.len()] == from[..] {
                    font[start..start + to.len()].copy_from_slice(&to);
                }
            }
        }
        font
    }

    /// cmap 을 기호 하나(U+2603)만 있는 형식 4 표로 바꾼다. 라틴 글자가 없는 글꼴의 대역이다.
    fn only_a_symbol() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let (cmap, _) = table(&font, b"cmap");
        // 원본에서 'M' 을 그리는 글리프 번호. 내용이 있는 글리프면 무엇이든 된다.
        const GLYPH: u16 = 78;
        const SYMBOL: u16 = 0x2603;
        let delta = GLYPH.wrapping_sub(SYMBOL);
        let words: [u16; 24] = [
            0, 1, 3, 1, 0, 12, // cmap 머리와 레코드 하나
            4, 32, 0, 4, 4, 1, 0, // 형식 4 머리, 구간 둘
            SYMBOL, 0xffff, 0, SYMBOL, 0xffff, delta, 1, 0, 0, 0, 0,
        ];
        for (i, word) in words.iter().enumerate() {
            font[cmap + i * 2..cmap + i * 2 + 2].copy_from_slice(&word.to_be_bytes());
        }
        let record = table_record(&font, b"cmap");
        font[record + 12..record + 16].copy_from_slice(&48u32.to_be_bytes());
        font
    }

    fn bytes_verdict(bytes: Vec<u8>) -> &'static str {
        match judge_terminal_bytes(bytes) {
            Ok(()) => "ok",
            Err(TerminalFontRejection::NoFace) => "no face",
            Err(TerminalFontRejection::CannotDraw) => "cannot draw",
            Err(_) => "other",
        }
    }

    #[test]
    fn files_the_terminal_cannot_use_are_rejected() {
        assert_eq!(bytes_verdict(b"this is not a font\n".to_vec()), "no face");
        assert_eq!(bytes_verdict(Vec::new()), "no face");
        assert_eq!(bytes_verdict(VALID[..200].to_vec()), "no face");
        assert_eq!(bytes_verdict(VALID[..1024].to_vec()), "no face");
        assert_eq!(bytes_verdict(without_table(b"cmap")), "cannot draw");
        assert_eq!(bytes_verdict(without_table(b"hhea")), "cannot draw");
    }

    /// egui 판정과 갈리는 입력이다. 줄 높이가 음수여도 터미널은 그 파일로 그린다.
    #[test]
    fn files_the_terminal_can_draw_pass() {
        assert_eq!(bytes_verdict(VALID.to_vec()), "ok");
        assert_eq!(bytes_verdict(negative_height()), "ok");
        assert_eq!(bytes_verdict(glyf_garbage()), "ok");
    }

    #[test]
    fn a_font_without_latin_letters_is_judged_by_its_own_glyphs() {
        assert_eq!(bytes_verdict(only_a_symbol()), "ok");
    }

    #[test]
    fn a_missing_path_and_a_folder_are_read_failures() {
        let dir = tempfile::tempdir().unwrap();
        for path in [dir.path().join("no-such-font.ttf"), dir.path().to_owned()] {
            let verdict = judge_terminal_file(&path.display().to_string());
            assert!(
                matches!(verdict, Err(TerminalFontRejection::Read(_))),
                "{path:?}: {verdict:?}"
            );
        }
    }

    struct TempFont(tempfile::TempDir);

    impl TempFont {
        fn new(bytes: &[u8]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("font.ttf"), bytes).unwrap();
            Self(dir)
        }

        fn path(&self) -> String {
            self.0.path().join("font.ttf").display().to_string()
        }
    }

    const RENAMED_FAMILY: &str = "D2Codinh ligature";

    #[test]
    fn a_family_only_the_rows_file_provides_passes() {
        let file = TempFont::new(&renamed(VALID.to_vec()));
        let config = FontConfig::new(PROBE_SIZE, "");
        assert!(file_provides_family(&file.path(), RENAMED_FAMILY));
        assert!(
            config
                .judge_terminal_family(RENAMED_FAMILY, &file.path())
                .is_ok()
        );
    }

    /// 다른 행의 파일이 제공한 이름은 이 행에서 찾지 못한다. 판정이 구성을 바꾸면 여기서 드러난다.
    #[test]
    fn a_family_from_another_rows_file_is_not_found() {
        let file = TempFont::new(&renamed(VALID.to_vec()));
        let config = FontConfig::new(PROBE_SIZE, "");
        assert!(
            config
                .judge_terminal_family(RENAMED_FAMILY, &file.path())
                .is_ok()
        );
        assert!(matches!(
            config.judge_terminal_family(RENAMED_FAMILY, ""),
            Err(TerminalFontRejection::FamilyNotFound)
        ));
    }

    #[test]
    fn a_family_whose_file_cannot_draw_is_rejected() {
        let file = TempFont::new(&renamed(without_table(b"cmap")));
        let config = FontConfig::new(PROBE_SIZE, "");
        assert!(matches!(
            config.judge_terminal_family(RENAMED_FAMILY, &file.path()),
            Err(TerminalFontRejection::CannotDraw)
        ));
    }

    #[test]
    fn a_bundled_family_passes_and_an_unknown_one_is_not_found() {
        let config = FontConfig::new(PROBE_SIZE, "");
        assert!(
            config
                .judge_terminal_family(crate::D2CODING_FAMILY, "")
                .is_ok()
        );
        assert!(matches!(
            config.judge_terminal_family("Tasty No Such Family", ""),
            Err(TerminalFontRejection::FamilyNotFound)
        ));
    }

    /// 면이 없는 파일에 "Loaded" 를 남기지 않는다. 면이 있는지가 그 분기를 정한다.
    #[test]
    fn loading_a_text_file_registers_no_face() {
        let file = TempFont::new(b"this is not a font\n");
        let mut config = FontConfig::new(PROBE_SIZE, "");
        let before = config.font_system.db().len();
        assert!(!crate::load_custom_font_file(
            &mut config.font_system,
            &file.path()
        ));
        assert_eq!(config.font_system.db().len(), before);
        let good = TempFont::new(VALID);
        assert!(crate::load_custom_font_file(
            &mut config.font_system,
            &good.path()
        ));
        assert_eq!(config.font_system.db().len(), before + 1);
    }
}
