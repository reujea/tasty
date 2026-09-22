//! `webview.open_external` — plugin 이 **자기 webview surface** 안의 외부 링크를 OS 기본
//! 핸들러로 연다.
//!
//! plugin 프로세스는 OS 열기를 직접 하지 않는다. host 의 OS 열기 자리
//! (`terminal_link::open_uri`)로 모아야 debug 스위치(ADR-0511)가 그것도 기록하고, 무엇이
//! 열리는지를 host 가 한 자리에서 본다. 근거·대안은 ADR-0527.
//!
//! 판정은 [`authorize`] 하나다 — surface 가 호출 plugin 소유의 plugin surface 이고, URL 에
//! 스킴이 있으며 `javascript:` 가 아니어야 한다. surface→plugin 매핑은 각 view 의
//! `core_state` 만 알아서 이 판정은 App 층에 있다(`banner.open` 의 D1 과 같은 자리).

use serde_json::json;
use tasty_host_plugin::manager::PendingPluginCall;

use crate::app::App;

impl App {
    /// `webview.open_external { surface_id, url }` 인터셉트. 권한(`surface.write`)은 진입부
    /// pre-gate 가 이미 봤다. 연 결과는 `{ "opened": bool }` 이고, OS 가 열기를 거절한 것은
    /// 오류가 아니라 `false` 다(원인은 `open_uri` 가 경고로 남긴다).
    pub(super) fn handle_ipc_webview_open_external(&mut self, call: &PendingPluginCall) {
        let (result, error, code) = match self.webview_open_request(call) {
            Ok(url) => (
                Some(json!({ "opened": crate::terminal_link::open_uri(&url) })),
                None,
                None,
            ),
            Err(message) => (None, Some(message), Some(-32602)),
        };
        if let Some(mgr) = self.plugin_manager.as_mut() {
            mgr.send_ipc_result(&call.plugin_id, call.call_id, result, error, code);
        }
    }

    /// 요청을 읽고 판정한다. 통과하면 열 URL.
    fn webview_open_request(&self, call: &PendingPluginCall) -> Result<String, String> {
        let surface_id =
            crate::adapters::ipc::handler::params::read_u32(&call.params, "surface_id")
                .map_err(|m| format!("webview.open_external: {m}"))?
                .ok_or("webview.open_external: missing 'surface_id'")?;
        let url = call
            .params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("webview.open_external: missing 'url'")?;
        let owner = self.plugin_surface_owner(surface_id);
        authorize(owner.as_deref(), &call.plugin_id, surface_id, url)?;
        Ok(url.trim().to_string())
    }

    /// 살아 있는 창에서 `surface_id` 를 가진 plugin surface 의 소유 plugin. 없으면 `None` —
    /// 보이지 않는(parked) 창의 surface 에서 온 클릭은 없다.
    fn plugin_surface_owner(&self, surface_id: u32) -> Option<String> {
        self.view.views.values().find_map(|w| {
            let surface = w.as_main()?.core_state.find_surface_by_id(surface_id)?;
            surface
                .as_any()
                .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                .map(|rs| rs.plugin_id.clone())
        })
    }
}

/// 열어도 되는가. `owner` 는 `surface_id` 가 가리키는 plugin surface 의 소유 plugin 이다.
///
/// - 남의 surface · 없는 surface 는 거절한다 — plugin 은 자기 surface 에서 일어난 일만 연다.
/// - URL 은 스킴이 있어야 한다(`https:` · `mailto:` 등). 스킴 없는 문자열은 파일 경로이고
///   그것은 `file_handler.dispatch` 의 일이다. `javascript:` 는 거절한다.
pub(crate) fn authorize(
    owner: Option<&str>,
    caller: &str,
    surface_id: u32,
    url: &str,
) -> Result<(), String> {
    match owner {
        None => {
            return Err(format!(
                "webview.open_external: surface {surface_id} is not a live plugin surface"
            ));
        }
        Some(owner) if owner != caller => {
            return Err(format!(
                "webview.open_external: plugin '{caller}' does not own surface {surface_id} (owner '{owner}')"
            ));
        }
        Some(_) => {}
    }
    let scheme = url_scheme(url.trim())
        .ok_or_else(|| format!("webview.open_external: '{url}' has no URL scheme"))?;
    if scheme.eq_ignore_ascii_case("javascript") {
        return Err("webview.open_external: javascript: URLs are not opened".to_string());
    }
    Ok(())
}

/// RFC 3986 스킴(`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`) 뒤에 `:` 가 오면 그 스킴.
/// 한 글자 스킴은 Windows 드라이브 문자(`C:\…`)와 겹쳐 스킴으로 치지 않는다.
fn url_scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic()
        && scheme.len() > 1
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(scheme)
}

#[cfg(test)]
mod tests {
    use super::authorize;

    const P: &str = "com.tasty.markdown";

    #[test]
    fn the_owner_opens_a_url_with_a_scheme() {
        for url in [
            "https://example.com/a?b=1",
            "http://x",
            "mailto:a@b.com",
            "  https://trim.me  ",
        ] {
            assert_eq!(authorize(Some(P), P, 7, url), Ok(()), "{url}");
        }
    }

    #[test]
    fn another_plugins_surface_or_a_missing_one_is_refused() {
        assert!(authorize(Some("com.example.other"), P, 7, "https://x").is_err());
        assert!(authorize(None, P, 7, "https://x").is_err());
    }

    #[test]
    fn a_path_or_a_script_is_not_opened() {
        for url in [
            "",
            "/tmp/a.md",
            "a.md",
            "C:\\docs\\a.md",
            "javascript:alert(1)",
            "JavaScript:alert(1)",
        ] {
            assert!(authorize(Some(P), P, 7, url).is_err(), "{url}");
        }
    }
}
