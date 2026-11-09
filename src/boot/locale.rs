//! 설정 언어로 i18n을 초기화하고 실제 적용 언어·폰트를 자식 프로세스의 환경변수에 반영한다.
//! 언어팩 로드 실패 시 영어로 돌아가지만 설정값은 바꾸지 않는다.
//! 폰트 파일 검증 실패는 문자열 언어를 유지하고 경고한다.

use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) const LOCALE_ENV: &str = "TASTY_LOCALE";
pub(crate) const LOCALE_FONT_ENV: &str = "TASTY_LOCALE_FONT";
pub(crate) const LOCALE_FONT_FACE_ENV: &str = "TASTY_LOCALE_FONT_FACE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedLocale {
    /// 요청값이 아니라 폴백을 반영한 실제 i18n 언어 코드.
    pub code: String,
    /// 언어팩에서 검증한 파일 경로. 없거나 검증 실패면 환경변수에서도 지운다.
    pub font_file: Option<PathBuf>,
    /// `font_file` 안에서 읽을 글꼴 번호.
    pub font_face: u32,
}

impl ResolvedLocale {
    pub fn from_report(report: &crate::i18n::LoadReport) -> Self {
        Self {
            code: report.effective.clone(),
            font_file: None,
            font_face: 0,
        }
    }

    /// 폰트가 없으면 이전 셸에서 상속한 값을 지워 자식에 잘못 전달하지 않는다.
    pub fn env_entries(&self) -> [(&'static str, Option<OsString>); 3] {
        [
            (LOCALE_ENV, Some(OsString::from(&self.code))),
            (
                LOCALE_FONT_ENV,
                self.font_file.as_ref().map(|p| p.clone().into_os_string()),
            ),
            (
                LOCALE_FONT_FACE_ENV,
                self.font_file
                    .as_ref()
                    .map(|_| OsString::from(self.font_face.to_string())),
            ),
        ]
    }
}

/// 호스트와 플러그인이 부팅 때 정한 같은 폰트 경로를 사용한다.
#[cfg(feature = "gui")]
pub(crate) fn font_env_path() -> Option<PathBuf> {
    std::env::var_os(LOCALE_FONT_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// 빈 코드를 소비자마다 다르게 해석하지 않도록 en으로 정규화한다.
pub(crate) fn normalize_code(language: &str) -> String {
    let code = language.trim();
    if code.is_empty() {
        "en".to_string()
    } else {
        code.to_string()
    }
}

use std::sync::Once;

static INIT: Once = Once::new();

/// 폰트 검증 실패 사유를 보관해 GUI가 부팅 후 알릴 수 있게 한다. 헤드리스·CLI는 로그만 사용한다.
static FONT_WARNING: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg(feature = "gui")]
pub(crate) fn font_warning() -> Option<String> {
    FONT_WARNING.get().cloned()
}

/// 재호출 때 설정 파일 읽기·경고까지 반복하지 않도록 한 번만 초기화한다.
pub(crate) fn init() {
    INIT.call_once(|| {
        let lang_settings = crate::settings::Settings::load();
        let requested = normalize_code(&lang_settings.general.language);
        let report = crate::i18n::init(&requested);
        let mut locale = ResolvedLocale::from_report(&report);
        match crate::boot::locale_font::resolve(&report.outcome) {
            crate::boot::locale_font::FontResolution::Resolved { path, face } => {
                locale.font_file = Some(path);
                locale.font_face = face;
            }
            crate::boot::locale_font::FontResolution::Failed { detail } => {
                tracing::warn!(
                    "locale '{}' declares a [font] that could not be resolved: {detail}",
                    locale.code
                );
                // 첫 사유만 보관한다. 이미 값이 있으면 새 사유는 버린다.
                FONT_WARNING.get_or_init(|| detail);
            }
            crate::boot::locale_font::FontResolution::None => {}
        }
        export_to_process_env(&locale);
    });
}

/// 자식이 상속할 환경을 정한다. 다른 스레드가 환경에 접근하지 않는 부팅 구간에서만 호출해야 한다.
/// 현재 첫 호출은 CLI 라우팅 시작점이며 이벤트 루프·IPC·플러그인·PTY 워커 생성보다 앞이다.
/// 실행 중 언어 변경은 재시작 전까지 반영하지 않는다.
fn export_to_process_env(locale: &ResolvedLocale) {
    for (key, value) in locale.env_entries() {
        match value {
            Some(value) => {
                // SAFETY: 다른 스레드가 환경에 접근하기 전의 부팅 구간에서만 호출한다.
                unsafe { std::env::set_var(key, &value) };
            }
            None => {
                // SAFETY: set_var와 같은 부팅 구간이며 환경에 동시에 접근하는 스레드가 없어야 한다.
                unsafe { std::env::remove_var(key) };
            }
        }
    }
    tracing::debug!(
        "locale: {} exported to process env ({LOCALE_ENV}; font={})",
        locale.code,
        locale
            .font_file
            .as_ref()
            .map_or_else(|| "unset".to_string(), |p| p.display().to_string())
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{LoadOutcome, LoadReport};

    fn report(requested: &str, effective: &str, outcome: LoadOutcome) -> LoadReport {
        LoadReport {
            requested: requested.to_string(),
            effective: effective.to_string(),
            outcome,
        }
    }

    #[test]
    fn from_report_keeps_effective_code_and_has_no_font() {
        let l = ResolvedLocale::from_report(&report("ko", "ko", LoadOutcome::Builtin));
        assert_eq!(l.code, "ko");
        assert_eq!(l.font_file, None);
    }

    #[test]
    fn from_report_uses_fallback_language_not_the_requested_one() {
        let l = ResolvedLocale::from_report(&report(
            "zz",
            "en",
            LoadOutcome::PackMissing {
                expected: PathBuf::from("lang/zz/pack.toml"),
            },
        ));
        assert_eq!(l.code, "en");
    }

    #[test]
    fn normalize_code_maps_blank_to_en() {
        assert_eq!(normalize_code(""), "en");
        assert_eq!(normalize_code("  "), "en");
        assert_eq!(normalize_code(" ja "), "ja");
        assert_eq!(normalize_code("xx"), "xx");
    }

    #[test]
    fn env_entries_set_locale_and_unset_font_when_absent() {
        let l = ResolvedLocale::from_report(&report("ja", "ja", LoadOutcome::Builtin));
        let [(k1, v1), (k2, v2), (k3, v3)] = l.env_entries();
        assert_eq!(k1, "TASTY_LOCALE");
        assert_eq!(v1, Some(OsString::from("ja")));
        assert_eq!(k2, "TASTY_LOCALE_FONT");
        assert_eq!(v2, None);
        assert_eq!(k3, "TASTY_LOCALE_FONT_FACE");
        assert_eq!(v3, None);
    }

    #[test]
    fn env_entries_carry_font_path_when_resolved() {
        let font = crate::test_support::abs_path("lang/ko/fonts/x.ttf");
        let l = ResolvedLocale {
            code: "ko".to_string(),
            font_file: Some(font.clone()),
            font_face: 2,
        };
        let [_, (k, v), (k_face, v_face)] = l.env_entries();
        assert_eq!(k, "TASTY_LOCALE_FONT");
        assert_eq!(v, Some(font.into_os_string()));
        assert_eq!(k_face, "TASTY_LOCALE_FONT_FACE");
        assert_eq!(v_face, Some(OsString::from("2")));
    }
}
