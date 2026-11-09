//! 언어팩의 파일·시스템 family·후보 목록에서 첫 유효한 폰트를 찾는다. builtin이면 추가하지 않는다.
//! 파일 선언의 절대경로·상위 참조는 거절하지만 심볼릭 링크의 경계까지 검사하지는 않는다.
//! ab_glyph로 검증한 뒤 경로를 환경변수로 전달하며 실패해도 문자열 언어는 유지한다.

use std::path::{Component, Path, PathBuf};

use crate::i18n::{FontDecl, LoadOutcome};

pub(crate) enum FontResolution {
    None,
    Resolved(PathBuf),
    Failed { detail: String },
}

pub(crate) fn resolve(outcome: &LoadOutcome) -> FontResolution {
    let LoadOutcome::Pack { path, font } = outcome else {
        return FontResolution::None;
    };
    let Some(pack_dir) = path.parent() else {
        return FontResolution::Failed {
            detail: format!("pack manifest has no parent directory: {}", path.display()),
        };
    };
    match font {
        FontDecl::Builtin => FontResolution::None,
        FontDecl::File(rel) => resolve_file(pack_dir, rel),
        FontDecl::Family(name) => resolve_family(name),
        FontDecl::Candidates(items) => resolve_candidates(pack_dir, items),
    }
}

fn resolve_file(pack_dir: &Path, rel: &str) -> FontResolution {
    let relp = Path::new(rel);
    if relp.is_absolute() || relp.components().any(|c| matches!(c, Component::ParentDir)) {
        return FontResolution::Failed {
            detail: format!("[font] file must stay inside the pack directory: {rel}"),
        };
    }
    let full = pack_dir.join(relp);
    match validate(&full) {
        Ok(()) => FontResolution::Resolved(full),
        Err(e) => FontResolution::Failed {
            detail: format!("{}: {e}", full.display()),
        },
    }
}

fn resolve_family(name: &str) -> FontResolution {
    match family_path(name) {
        Some(p) => match validate(&p) {
            Ok(()) => FontResolution::Resolved(p),
            Err(e) => FontResolution::Failed {
                detail: format!("family '{name}' at {}: {e}", p.display()),
            },
        },
        None => FontResolution::Failed {
            detail: format!("font family not installed or not file-backed: {name}"),
        },
    }
}

fn resolve_candidates(pack_dir: &Path, items: &[String]) -> FontResolution {
    let mut errors = Vec::new();
    for item in items {
        let r = if looks_like_path(item) {
            resolve_file(pack_dir, item)
        } else {
            resolve_family(item)
        };
        match r {
            FontResolution::Resolved(p) => return FontResolution::Resolved(p),
            FontResolution::Failed { detail } => errors.push(detail),
            FontResolution::None => {}
        }
    }
    FontResolution::Failed {
        detail: format!("no [font] candidate resolved: {}", errors.join(" | ")),
    }
}

fn looks_like_path(s: &str) -> bool {
    if s.contains('/') || s.contains('\\') {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")
}

/// 시스템 폰트 DB는 family 조회가 필요할 때만 만든다.
fn family_path(name: &str) -> Option<PathBuf> {
    // 파일 경로 조회에는 크기 설정이 쓰이지 않는다.
    let cfg = tasty_font::FontConfig::new(14.0, "");
    cfg.family_source_path(name)
}

/// 부팅 때 파일을 검증한다. 실제 폰트 추가 시점에도 다시 읽고 검증한다.
fn validate(path: &Path) -> Result<(), String> {
    tasty_i18n::font::read_validated(path, 0)
        .map(|_| ())
        .map_err(|error| match error {
            tasty_i18n::font::LocaleFontError::Read(error) => format!("cannot read: {error}"),
            tasty_i18n::font::LocaleFontError::Parse => "not a valid font file".to_string(),
            tasty_i18n::font::LocaleFontError::NoLineHeight => {
                "font has no positive line height".to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(dir: &Path, font: FontDecl) -> LoadOutcome {
        LoadOutcome::Pack {
            path: dir.join("pack.toml"),
            font,
        }
    }

    fn tmp_pack(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tasty-locale-font-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn valid_bundled_font_file_resolves() {
        let dir = tmp_pack("valid");
        let fonts_dir = dir.join("fonts");
        std::fs::create_dir_all(&fonts_dir).unwrap();
        std::fs::write(fonts_dir.join("x.ttf"), crate::font::D2CODING_REGULAR_TTF).unwrap();
        let out = pack(&dir, FontDecl::File("fonts/x.ttf".to_string()));
        match resolve(&out) {
            FontResolution::Resolved(p) => assert_eq!(p, dir.join("fonts/x.ttf")),
            other => panic!("expected Resolved, got {:?}", ResolutionDbg(&other)),
        }
        let _ = std::fs::remove_dir_all(&dir); // 시험 뒤 임시 디렉터리 삭제 실패는 무시한다.
    }

    #[test]
    fn builtin_declaration_resolves_to_none() {
        let dir = tmp_pack("builtin");
        assert!(matches!(
            resolve(&pack(&dir, FontDecl::Builtin)),
            FontResolution::None
        ));
        let _ = std::fs::remove_dir_all(&dir); // 시험 뒤 임시 디렉터리 삭제 실패는 무시한다.
    }

    #[test]
    fn non_pack_outcome_resolves_to_none() {
        assert!(matches!(
            resolve(&LoadOutcome::Builtin),
            FontResolution::None
        ));
    }

    #[test]
    fn missing_file_fails_not_panics() {
        let dir = tmp_pack("missing");
        assert!(matches!(
            resolve(&pack(&dir, FontDecl::File("fonts/nope.ttf".to_string()))),
            FontResolution::Failed { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir); // 시험 뒤 임시 디렉터리 삭제 실패는 무시한다.
    }

    #[test]
    fn garbage_bytes_fail_not_panics() {
        let dir = tmp_pack("garbage");
        std::fs::write(dir.join("bad.ttf"), b"definitely not a font").unwrap();
        assert!(matches!(
            resolve(&pack(&dir, FontDecl::File("bad.ttf".to_string()))),
            FontResolution::Failed { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir); // 시험 뒤 임시 디렉터리 삭제 실패는 무시한다.
    }

    #[test]
    fn a_path_that_escapes_the_pack_is_refused() {
        let dir = tmp_pack("escape");
        assert!(matches!(
            resolve(&pack(
                &dir,
                FontDecl::File("../../etc/evil.ttf".to_string())
            )),
            FontResolution::Failed { .. }
        ));
        assert!(matches!(
            resolve(&pack(&dir, FontDecl::File("/etc/evil.ttf".to_string()))),
            FontResolution::Failed { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir); // 시험 뒤 임시 디렉터리 삭제 실패는 무시한다.
    }

    #[test]
    fn looks_like_path_splits_files_from_families() {
        assert!(looks_like_path("fonts/x.ttf"));
        assert!(looks_like_path("x.OTF"));
        assert!(looks_like_path(r"win\path.ttc"));
        assert!(!looks_like_path("Noto Sans Arabic"));
    }

    struct ResolutionDbg<'a>(&'a FontResolution);
    impl std::fmt::Debug for ResolutionDbg<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self.0 {
                FontResolution::None => write!(f, "None"),
                FontResolution::Resolved(p) => write!(f, "Resolved({})", p.display()),
                FontResolution::Failed { detail } => write!(f, "Failed({detail})"),
            }
        }
    }
}
