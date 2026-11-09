//! plugin이 선언한 webview 종류 이름과 그 설정 출처를 프로세스 전역에 기록한다.
//! remote kind 정의 등록·권한 검사·실제 overlay 생성은 호출 경로가 따로 맡는다.

use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

use tasty_plugin_manifest::{SettingsItemDecl, SettingsPageContribute, SurfaceKindDecl};

static WEBVIEW_KINDS: RwLock<Option<HashMap<String, WebViewSettingsSource>>> = RwLock::new(None);

/// 같은 poison을 프레임마다 반복해 로그에 남기지 않는다.
static POISON_REPORTED: AtomicBool = AtomicBool::new(false);

/// poison을 첫 로그와 함께 복구해 기존 집합을 계속 사용한다. 내용을 재검증하지는 않는다.
const WHAT: &str = "webview kind set";

/// 사용자 설정이 없을 때 쓸 WebView 설정 값. 저장 키 이름(`zoom` 등)과 대응한다.
#[derive(Debug, Clone, PartialEq)]
pub struct WebViewSettingDefaults {
    pub zoom_percent: f64,
    pub sandbox_scripts: bool,
    pub allow_remote_content: bool,
    /// follow | light | dark.
    pub color_scheme: String,
}

impl Default for WebViewSettingDefaults {
    fn default() -> Self {
        Self {
            zoom_percent: 100.0,
            sandbox_scripts: true,
            allow_remote_content: false,
            color_scheme: "follow".to_string(),
        }
    }
}

impl WebViewSettingDefaults {
    /// 같은 저장 키의 설정 페이지 항목 기본값 > kind의 webview_defaults > 호스트 기본값 순으로 정한다.
    /// 설정 창이 보여 주는 기본값과 실제 적용값이 갈리지 않도록 설정 페이지 항목을 먼저 본다.
    pub fn from_manifest(decl: &SurfaceKindDecl, pages: &[SettingsPageContribute]) -> Self {
        let host = Self::default();
        let declared = decl.webview_defaults.clone().unwrap_or_default();
        let items = || pages.iter().flat_map(|p| p.items.iter());
        let toggle = |key: &str| {
            items().find_map(|item| match item {
                SettingsItemDecl::Toggle {
                    storage_key,
                    default,
                    ..
                } if storage_key == key => Some(*default),
                _ => None,
            })
        };
        let zoom_percent = items()
            .find_map(|item| match item {
                SettingsItemDecl::Number {
                    storage_key,
                    default,
                    ..
                } if storage_key == "zoom" => Some(*default),
                _ => None,
            })
            .or(declared.zoom)
            .unwrap_or(host.zoom_percent);
        let color_scheme = items()
            .find_map(|item| match item {
                SettingsItemDecl::Select {
                    storage_key,
                    default,
                    ..
                } if storage_key == "color_scheme" => Some(default.clone()),
                _ => None,
            })
            .or(declared.color_scheme)
            .unwrap_or(host.color_scheme);
        Self {
            zoom_percent,
            sandbox_scripts: toggle("sandbox_scripts")
                .or(declared.sandbox_scripts)
                .unwrap_or(host.sandbox_scripts),
            allow_remote_content: toggle("allow_remote_content")
                .or(declared.allow_remote_content)
                .unwrap_or(host.allow_remote_content),
            color_scheme,
        }
    }
}

/// webview kind의 설정 소유 plugin과 기본값. 설정은 `plugin_settings.<plugin_id>`에서 읽는다.
#[derive(Debug, Clone, PartialEq)]
pub struct WebViewSettingsSource {
    pub plugin_id: String,
    pub defaults: WebViewSettingDefaults,
    /// kind 선언의 `lazy_webview` — 처음 보일 때 WebView를 만들고 그 전 메시지는 버린다.
    pub lazy: bool,
    /// 설정 페이지 토글 [`RELEASE_HIDDEN_KEY`]의 기본값. 항목이 없으면 꺼짐이다.
    pub release_hidden_default: bool,
}

/// 메모리 절약 토글의 저장 키. lazy kind의 소유 plugin이 이 키를 켜면 호스트는 가려진 WebView를 없앤다.
pub const RELEASE_HIDDEN_KEY: &str = "release_hidden_webview";

/// 설정 페이지에서 `key` 토글의 기본값.
fn toggle_default(pages: &[SettingsPageContribute], key: &str) -> Option<bool> {
    pages
        .iter()
        .flat_map(|p| p.items.iter())
        .find_map(|item| match item {
            SettingsItemDecl::Toggle {
                storage_key,
                default,
                ..
            } if storage_key == key => Some(*default),
            _ => None,
        })
}

/// 같은 kind를 다른 plugin이 다시 등록하면 surface 등록부처럼 나중 등록이 설정 출처가 된다.
pub fn register_webview_kind(
    plugin_id: &str,
    decl: &SurfaceKindDecl,
    settings_pages: &[SettingsPageContribute],
) {
    let source = WebViewSettingsSource {
        plugin_id: plugin_id.to_string(),
        defaults: WebViewSettingDefaults::from_manifest(decl, settings_pages),
        lazy: decl.lazy_webview,
        release_hidden_default: toggle_default(settings_pages, RELEASE_HIDDEN_KEY).unwrap_or(false),
    };
    let previous = {
        let mut guard = crate::poison::recover_write(WEBVIEW_KINDS.write(), WHAT, &POISON_REPORTED);
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(decl.kind.clone(), source.clone())
    };
    if previous.as_ref() != Some(&source) {
        tracing::info!(
            "registered webview-enabled surface kind '{}' for plugin '{}' ({:?})",
            decl.kind,
            plugin_id,
            source.defaults
        );
    }
}

#[cfg(any(feature = "gui", test))]
pub fn is_webview_kind(kind: &str) -> bool {
    crate::poison::recover_read(WEBVIEW_KINDS.read(), WHAT, &POISON_REPORTED)
        .as_ref()
        .is_some_and(|s| s.contains_key(kind))
}

/// 등록된 webview kind의 설정 출처. 미등록이면 None이며 호출자는 호스트 기본값을 쓴다.
#[cfg(any(feature = "gui", test))]
pub fn webview_settings_source(kind: &str) -> Option<WebViewSettingsSource> {
    crate::poison::recover_read(WEBVIEW_KINDS.read(), WHAT, &POISON_REPORTED)
        .as_ref()
        .and_then(|s| s.get(kind).cloned())
}

/// 전역 집합을 등록·초기화·조회하는 검사는 이 락을 함께 사용해야 한다.
/// 등록만 하는 헬퍼는 그 호출을, 초기화·조회 검사는 검사 전체를 잠근다.
/// 제품 등록 경로는 이 검사 락을 쓰지 않는다. 접근 이름을 찾는 source guard는
/// 이름이 드러나지 않는 간접 호출까지 추적하지는 못한다.
#[cfg(test)]
pub static WEBVIEW_KIND_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub fn reset_for_test() {
    *crate::poison::recover_write(WEBVIEW_KINDS.write(), WHAT, &POISON_REPORTED) = None;
}

#[cfg(test)]
// 이유: 검사에서 오류를 의도적으로 버리는 표현을 허용한다.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    fn decl(kind: &str) -> SurfaceKindDecl {
        serde_json::from_value(serde_json::json!({
            "kind": kind,
            "display_name_i18n_key": "surface.kind.html",
            "rendering": "webview",
        }))
        .unwrap()
    }

    #[test]
    fn register_and_query() {
        let _guard = WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        reset_for_test();
        assert!(!is_webview_kind("foo"));
        register_webview_kind("com.example", &decl("foo"), &[]);
        assert!(is_webview_kind("foo"));
        assert!(!is_webview_kind("bar"));
    }

    #[test]
    fn register_and_is_webview_kind_survive_poison() {
        let _guard = WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        reset_for_test();

        let _ = std::thread::spawn(|| {
            let _guard = WEBVIEW_KINDS.write().unwrap();
            panic!("simulate poison");
        })
        .join();

        register_webview_kind("plugin-a", &decl("html"), &[]);
        assert!(is_webview_kind("html"));
        assert!(!is_webview_kind("markdown"));

        reset_for_test();
    }

    fn pages(toml_items: &str) -> Vec<SettingsPageContribute> {
        let page: SettingsPageContribute = toml::from_str(&format!(
            "id = \"p\"\ntitle_key = \"t\"\ncategory = \"appearance\"\n{toml_items}"
        ))
        .unwrap();
        vec![page]
    }

    #[test]
    fn defaults_fall_back_to_host_values() {
        assert_eq!(
            WebViewSettingDefaults::from_manifest(&decl("view"), &[]),
            WebViewSettingDefaults::default()
        );
    }

    #[test]
    fn defaults_prefer_settings_items_then_kind_declaration() {
        let mut d = decl("view");
        d.webview_defaults = Some(tasty_plugin_manifest::WebViewDefaultsDecl {
            zoom: Some(150.0),
            sandbox_scripts: Some(false),
            allow_remote_content: Some(true),
            color_scheme: Some("dark".into()),
        });
        let only_kind = WebViewSettingDefaults::from_manifest(&d, &[]);
        assert_eq!(
            only_kind,
            WebViewSettingDefaults {
                zoom_percent: 150.0,
                sandbox_scripts: false,
                allow_remote_content: true,
                color_scheme: "dark".into(),
            }
        );
        let items = pages(
            r#"
[[items]]
kind = "number"
id = "zoom"
label_key = "l"
storage_key = "zoom"
default = 120.0
[[items]]
kind = "toggle"
id = "sandbox"
label_key = "l"
storage_key = "sandbox_scripts"
default = true
"#,
        );
        let both = WebViewSettingDefaults::from_manifest(&d, &items);
        assert_eq!(both.zoom_percent, 120.0);
        assert!(both.sandbox_scripts);
        assert!(both.allow_remote_content);
        assert_eq!(both.color_scheme, "dark");
    }

    #[test]
    fn lazy_flag_follows_the_kind_declaration() {
        let _guard = WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        reset_for_test();
        register_webview_kind("com.example.a", &decl("eager"), &[]);
        let mut lazy = decl("lazy");
        lazy.lazy_webview = true;
        register_webview_kind("com.example.a", &lazy, &[]);
        let lazy_of = |k: &str| webview_settings_source(k).is_some_and(|s| s.lazy);
        assert!(!lazy_of("eager"));
        assert!(lazy_of("lazy"));
        assert!(!lazy_of("unknown"));
        reset_for_test();
    }

    #[test]
    fn source_follows_the_latest_registration() {
        let _guard = WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        reset_for_test();
        assert_eq!(webview_settings_source("view"), None);
        register_webview_kind("com.example.a", &decl("view"), &[]);
        assert_eq!(
            webview_settings_source("view").map(|s| s.plugin_id),
            Some("com.example.a".to_string())
        );
        register_webview_kind("com.example.b", &decl("view"), &[]);
        assert_eq!(
            webview_settings_source("view").map(|s| s.plugin_id),
            Some("com.example.b".to_string())
        );
        reset_for_test();
    }
}
