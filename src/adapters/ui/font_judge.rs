//! 저장하거나 입력한 글꼴 값을 그 글꼴을 쓰는 경로가 쓸 수 있는지 판정한다.
//!
//! egui 와 터미널은 다른 글꼴 경로를 쓰고 거절하는 입력도 다르다. 줄 높이가 음수인 파일은 egui 만
//! 거절하고, cmap 이 없는 파일은 터미널만 쓰지 못한다. 설정 행, 미리보기 등록, 저장값 토스트가 모두
//! 이 모듈의 판정을 지나므로 셋의 답이 갈리지 않는다.

use tasty_i18n::font::LocaleFontError;

use crate::font::{
    FontConfig, FontFileFace, TerminalFontRejection, file_provides_family, judge_terminal_file,
};

/// 판정이 따르는 글꼴 경로.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontRenderer {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the saved value check is the first caller")
    )]
    Ui,
    Terminal,
}

/// 거절된 값이 어느 입력에서 왔는지.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontOrigin {
    Custom,
    Named,
}

/// 입력별 판정. `None` 이면 오류가 아니고 값은 사유다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontVerdict {
    pub custom: Option<String>,
    pub family: Option<String>,
}

/// 코드로 정해지지 않고 제품 답에 달린 경우들.
#[derive(Clone, Copy)]
enum OpenCase {
    /// 정상 파일인데 family 가 비었거나 달라 터미널이 그 파일을 쓰지 않는다.
    FileNotUsedByTerminal,
    /// 저장된 family 이름의 글꼴이 없고 그 행의 파일도 그 이름을 제공하지 않는다.
    SavedFamilyGone,
}

/// 위 경우가 오류인지 정하는 유일한 지점이다. 답이 달라지면 이 함수만 바꾼다.
fn is_error(case: OpenCase) -> bool {
    match case {
        OpenCase::FileNotUsedByTerminal => false,
        OpenCase::SavedFamilyGone => true,
    }
}

/// 시스템 글꼴 목록. 스캔이 값마다 곱해지지 않도록 처음 필요할 때 한 번 만들고 다시 쓴다.
/// custom 파일을 넣지 않으므로 한 행의 파일이 다른 행의 이름 조회에 끼지 않는다.
#[derive(Default)]
pub struct FontLookup {
    config: Option<FontConfig>,
}

impl FontLookup {
    fn config(&mut self) -> &FontConfig {
        self.config.get_or_insert_with(|| {
            // 크기는 목록 조회에 영향을 주지 않는다.
            FontConfig::new(crate::theme::theme().font_size_caption.value(), "")
        })
    }

    pub(crate) fn load_family(&mut self, family: &str) -> Option<FontFileFace<Vec<u8>>> {
        self.config().load_family_data(family)
    }
}

/// 비어 있거나 `monospace` 면 어떤 글꼴도 지정하지 않은 값이다.
fn names_a_font(family: &str) -> bool {
    !family.is_empty() && !family.eq_ignore_ascii_case("monospace")
}

/// egui 가 쓸 수 있는 파일의 바이트를 읽는다.
pub(crate) fn load_ui_custom(path: &str) -> Result<Vec<u8>, LocaleFontError> {
    let bytes = std::fs::read(path).map_err(LocaleFontError::Read)?;
    tasty_i18n::font::validate(&bytes)?;
    Ok(bytes)
}

/// 이름으로 찾은 egui 글꼴.
pub(crate) enum UiFamily {
    /// 등록할 것도 오류도 없다. 이름이 없는 값이거나 그 행의 파일이 이름을 제공한다.
    Skip,
    Usable(FontFileFace<Vec<u8>>),
    /// 글꼴은 찾았지만 egui 가 쓰지 못한다.
    Refused(LocaleFontError),
    /// 그 이름의 글꼴이 없다.
    Missing,
}

/// `load` 로 찾은 바이트도 파일과 같은 판정을 거친다. fontdb 는 이름 표만 읽어 ab_glyph 가 거절하는
/// 파일도 목록에 올린다.
pub(crate) fn load_ui_family(
    family: &str,
    custom: &str,
    load: impl FnOnce(&str) -> Option<FontFileFace<Vec<u8>>>,
) -> UiFamily {
    if !names_a_font(family) {
        return UiFamily::Skip;
    }
    match load(family) {
        Some(found) => match tasty_i18n::font::validate_face(&found.file, found.face) {
            Ok(()) => UiFamily::Usable(found),
            Err(error) => UiFamily::Refused(error),
        },
        None if file_provides_family(custom, family) => UiFamily::Skip,
        None if is_error(OpenCase::SavedFamilyGone) => UiFamily::Missing,
        None => UiFamily::Skip,
    }
}

fn missing_family_reason() -> String {
    TerminalFontRejection::FamilyNotFound.to_string()
}

/// egui 경로의 판정. 값이 없는 칸은 읽지 않는다.
fn judge_ui(
    family: &str,
    custom: &str,
    load: impl FnOnce(&str) -> Option<FontFileFace<Vec<u8>>>,
) -> FontVerdict {
    let custom_reason = if custom.is_empty() {
        None
    } else {
        load_ui_custom(custom).err().map(|e| e.to_string())
    };
    let family_reason = match load_ui_family(family, custom, load) {
        UiFamily::Skip | UiFamily::Usable(_) => None,
        UiFamily::Refused(error) => Some(error.to_string()),
        UiFamily::Missing => Some(missing_family_reason()),
    };
    FontVerdict {
        custom: custom_reason,
        family: family_reason,
    }
}

/// 터미널 경로의 판정. UI 판정을 쓰지 않는다.
fn judge_terminal(family: &str, custom: &str, lookup: &mut FontLookup) -> FontVerdict {
    let custom_reason = if custom.is_empty() {
        None
    } else {
        judge_terminal_file(custom)
            .err()
            .map(|e| e.to_string())
            .or_else(|| {
                (is_error(OpenCase::FileNotUsedByTerminal) && !file_provides_family(custom, family))
                    .then(|| "the terminal does not use this file".to_owned())
            })
    };
    let family_reason = if names_a_font(family) {
        match lookup.config().judge_terminal_family(family, custom) {
            Ok(()) => None,
            Err(TerminalFontRejection::FamilyNotFound) if !is_error(OpenCase::SavedFamilyGone) => {
                None
            }
            Err(error) => Some(error.to_string()),
        }
    } else {
        None
    };
    FontVerdict {
        custom: custom_reason,
        family: family_reason,
    }
}

/// 이 경로가 이 값을 쓸 수 있는지 판정한다. 파일을 읽으므로 입력 중 프레임마다 부르지 않는다.
pub fn judge_font(
    renderer: FontRenderer,
    family: &str,
    custom: &str,
    lookup: &mut FontLookup,
) -> FontVerdict {
    match renderer {
        FontRenderer::Ui => judge_ui(family, custom, |name| lookup.load_family(name)),
        FontRenderer::Terminal => judge_terminal(family, custom, lookup),
    }
}

/// 깨진 글꼴 입력을 만드는 시험 도구. 묶음 D2Coding 의 표를 고쳐 만든다.
#[cfg(test)]
pub(crate) mod fixtures {
    pub(crate) const VALID: &[u8] = crate::font::D2CODING_REGULAR_TTF;
    /// `renamed` 가 만든 글꼴의 family 이름. 시스템 목록과 겹치지 않는다.
    pub(crate) const RENAMED_FAMILY: &str = "D2Codinh ligature";

    pub(crate) fn whole_file(file: Vec<u8>) -> crate::font::FontFileFace<Vec<u8>> {
        crate::font::FontFileFace { file, face: 0 }
    }

    pub(crate) fn table_record(font: &[u8], tag: &[u8; 4]) -> usize {
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

    /// Returns the bundled font with a table record renamed so the parser cannot find it.
    pub(crate) fn without_table(tag: &[u8; 4]) -> Vec<u8> {
        let mut font = VALID.to_vec();
        let record = table_record(&font, tag);
        font[record] = b'x';
        font
    }

    /// Parses, but its height is negative. epaint asserts on it at layout time.
    pub(crate) fn negative_height() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let (hhea, _) = table(&font, b"hhea");
        font[hhea + 4..hhea + 6].copy_from_slice(&(-1000i16).to_be_bytes());
        font[hhea + 6..hhea + 8].copy_from_slice(&1000i16.to_be_bytes());
        font
    }

    pub(crate) fn glyf_garbage() -> Vec<u8> {
        let mut font = VALID.to_vec();
        let (glyf, len) = table(&font, b"glyf");
        font[glyf + len / 4..glyf + len / 2].fill(0xff);
        font
    }

    pub(crate) fn not_a_font() -> Vec<u8> {
        b"this is not a font\n".to_vec()
    }

    /// 글꼴 이름을 바꿔 시스템에 같은 이름이 있어도 이 파일만 그 이름을 제공하게 한다.
    pub(crate) fn renamed(mut font: Vec<u8>) -> Vec<u8> {
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
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn file_rejected(renderer: FontRenderer, bytes: &[u8]) -> bool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, bytes).unwrap();
        let mut lookup = FontLookup::default();
        judge_font(renderer, "", &path.display().to_string(), &mut lookup)
            .custom
            .is_some()
    }

    /// 판정표 전체. 두 경로가 갈리는 입력(a3, t1, t3)이 이 표의 핵심이다.
    #[test]
    fn each_input_gets_the_verdict_of_its_renderer() {
        let cases: [(&str, Vec<u8>, bool, bool); 8] = [
            ("a1 text", not_a_font(), true, true),
            ("a2 empty", Vec::new(), true, true),
            ("a3 first 1024 bytes", VALID[..1024].to_vec(), false, true),
            ("a4 first 200 bytes", VALID[..200].to_vec(), true, true),
            ("t1 negative height", negative_height(), true, false),
            ("t2 glyf garbage", glyf_garbage(), false, false),
            ("t3 no cmap", without_table(b"cmap"), false, true),
            ("n1 no hhea", without_table(b"hhea"), true, true),
        ];
        let mut wrong = Vec::new();
        for (name, bytes, ui, terminal) in cases {
            let got = (
                file_rejected(FontRenderer::Ui, &bytes),
                file_rejected(FontRenderer::Terminal, &bytes),
            );
            if got != (ui, terminal) {
                wrong.push((name, got));
            }
        }
        assert!(wrong.is_empty(), "wrong (ui, terminal): {wrong:?}");
    }

    #[test]
    fn a_normal_file_passes_both() {
        assert!(!file_rejected(FontRenderer::Ui, VALID));
        assert!(!file_rejected(FontRenderer::Terminal, VALID));
    }

    #[test]
    fn a_missing_path_and_a_folder_are_rejected_by_both() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such.ttf").display().to_string();
        let folder = dir.path().display().to_string();
        let mut lookup = FontLookup::default();
        for renderer in [FontRenderer::Ui, FontRenderer::Terminal] {
            for path in [&missing, &folder] {
                let verdict = judge_font(renderer, "", path, &mut lookup);
                assert!(verdict.custom.is_some(), "{renderer:?} {path}");
            }
        }
    }

    #[test]
    fn empty_values_and_monospace_are_not_judged() {
        let mut lookup = FontLookup::default();
        for renderer in [FontRenderer::Ui, FontRenderer::Terminal] {
            for family in ["", "monospace", "MONOSPACE"] {
                assert_eq!(
                    judge_font(renderer, family, "", &mut lookup),
                    FontVerdict::default()
                );
            }
        }
        assert!(lookup.config.is_none(), "no value needs the system list");
    }

    /// 제품 답 A: 파일이 정상이면 터미널이 그 파일을 쓰지 않아도 오류가 아니다.
    #[test]
    fn a_normal_file_the_terminal_does_not_use_is_no_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, VALID).unwrap();
        let path = path.display().to_string();
        let mut lookup = FontLookup::default();
        let verdict = judge_font(FontRenderer::Terminal, "", &path, &mut lookup);
        assert_eq!(verdict, FontVerdict::default());
    }

    /// 제품 답 B: 이름의 글꼴이 없으면 두 경로 모두 family 오류다.
    #[test]
    fn a_saved_family_that_is_gone_is_an_error() {
        let mut lookup = FontLookup::default();
        for renderer in [FontRenderer::Ui, FontRenderer::Terminal] {
            let verdict = judge_font(renderer, "Tasty No Such Family", "", &mut lookup);
            assert!(verdict.family.is_some(), "{renderer:?}");
            assert!(verdict.custom.is_none(), "{renderer:?}");
        }
    }

    /// 시스템 목록에 없는 이름이어도 그 행의 파일이 제공하면 정상 설정이다.
    #[test]
    fn a_family_the_rows_file_provides_passes_both() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, renamed(VALID.to_vec())).unwrap();
        let path = path.display().to_string();
        let mut lookup = FontLookup::default();
        for renderer in [FontRenderer::Ui, FontRenderer::Terminal] {
            let verdict = judge_font(renderer, RENAMED_FAMILY, &path, &mut lookup);
            assert_eq!(verdict, FontVerdict::default(), "{renderer:?}");
        }
    }

    /// 한 행에서 파일을 본 뒤에도 다른 행은 그 이름을 찾지 못한다. 시스템 목록을 재사용해도 섞이지 않는다.
    #[test]
    fn a_family_from_another_rows_file_does_not_pass() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, renamed(VALID.to_vec())).unwrap();
        let path = path.display().to_string();
        let mut lookup = FontLookup::default();
        for renderer in [FontRenderer::Ui, FontRenderer::Terminal] {
            judge_font(renderer, RENAMED_FAMILY, &path, &mut lookup);
            let other_row = judge_font(renderer, RENAMED_FAMILY, "", &mut lookup);
            assert!(other_row.family.is_some(), "{renderer:?}");
        }
    }

    /// 터미널 family 는 파일이 그리지 못하면 오류고, 이름을 찾았다는 것만으로 통과하지 않는다.
    #[test]
    fn a_terminal_family_whose_file_cannot_draw_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.ttf");
        std::fs::write(&path, renamed(without_table(b"cmap"))).unwrap();
        let path = path.display().to_string();
        let mut lookup = FontLookup::default();
        let verdict = judge_font(FontRenderer::Terminal, RENAMED_FAMILY, &path, &mut lookup);
        assert!(verdict.family.is_some());
        assert!(verdict.custom.is_some());
    }

    /// 시스템 글꼴 폴더 없이 이름으로 찾은 바이트의 판정을 재려고 `judge_font` 뒤의 `judge_ui` 를 부른다.
    /// `judge_font` 가 이 경로를 쓰는지는 `a_saved_family_that_is_gone_is_an_error` 가 재고, 등록 경로는
    /// `font_registry` 시험이 같은 `load_ui_family` 로 잰다.
    #[test]
    fn a_ui_family_egui_cannot_use_is_an_error() {
        for bytes in [not_a_font(), negative_height(), without_table(b"hhea")] {
            let verdict = judge_ui("Some Family", "", |_| Some(whole_file(bytes)));
            assert!(verdict.family.is_some());
        }
        assert_eq!(
            judge_ui("Some Family", "", |_| Some(whole_file(VALID.to_vec()))),
            FontVerdict::default()
        );
    }
}
