//! Construct native widgets only after their App-owned preparation receipts settle.
use super::*;

pub(super) fn create_missing_webviews(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    all_html_ids: &[u32],
    active_html: &std::collections::HashMap<u32, crate::webview::WebViewBounds>,
    scale_factor: f64,
) {
    // Create new webviews for Html panels that don't have one yet
    for &sid in all_html_ids {
        if !view.webviews.contains_key(&sid) {
            // 재시도 상한에 도달한 surface는 다시 만들지 않는다.
            let attempts = view.webview_create_attempts.get(&sid).copied().unwrap_or(0);
            if !should_attempt_webview(attempts) {
                continue;
            }
            // Find the URL for this surface
            let url = find_webview_url(view, engine, sid);
            // 생성 시 적용할 plugin 설정(부재 시 default) 을 미리 해석한다.
            let settings = resolve_webview_settings(view, engine, sid);
            let result = construct(view, engine, sid, active_html, scale_factor);
            match result {
                Ok(None) => continue,
                Ok(Some(wv)) => {
                    let Some(binding) =
                        crate::app::html_runtime::NativeWebviewBinding::capture(engine, sid)
                    else {
                        continue;
                    };
                    let bounds = active_html.get(&sid);
                    tracing::debug!(
                        "WebView surface {sid}: created (visible={}, bounds={:?}, url={})",
                        bounds.is_some(),
                        bounds,
                        describe_webview_url(url.as_ref())
                    );
                    attach_script_gate(view, engine, sid, &wv, &settings);
                    // 첫 문서를 싣기 전에 둬야 그 문서의 첫 렌더가 이 값이다.
                    super::bed_focus::apply_initial(view, engine, sid, &wv);
                    load_initial_url(view, engine, sid, &wv, url.as_ref());
                    // 생성 직후 HTML viewer 설정(zoom/JS/scheme/remote) 적용 + 기록.
                    tracing::debug!("WebView surface {sid}: requesting settings {settings:?}");
                    settings.apply(&wv);
                    // Start hidden if not active
                    if !active_html.contains_key(&sid) {
                        wv.set_visible(false);
                    }
                    view.webviews.insert(sid, wv);
                    view.webview_runtime.insert(sid, binding);
                    view.webview_applied_settings.insert(sid, settings);
                    view.webview_create_attempts.remove(&sid);
                }
                Err(e) => record_webview_failure(view, sid, attempts, &e),
            }
        }
    }
}

fn default_bounds() -> crate::webview::WebViewBounds {
    crate::webview::WebViewBounds {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    }
}
/// Whether the surface's HTTP session lost its native owner without failing. When a surface's
/// view is rebuilt (for example after another tab opens in the same mirror pane), dropping the old
/// view revokes the owner of the session it showed. The attach refresh replaces such a session;
/// preparing it first fails as retired, and that failure gave the surface up until it was reopened.
/// A failed session keeps the permanent failure so it does not loop.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn awaits_replacement(failed: bool, native_owner_current: bool) -> bool {
    !failed && !native_owner_current
}

fn construct(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    active_html: &std::collections::HashMap<u32, crate::webview::WebViewBounds>,
    scale_factor: f64,
) -> Result<Option<crate::webview::PlatformWebView>, crate::webview::WebViewCreateError> {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    let http =
        find_remote_surface(view, engine, sid).and_then(|remote| remote.http_session.clone());
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    let prepared = if let Some(session) = http {
        if awaits_replacement(session.failed(), session.native_owner_current()) {
            view.http_preparing.remove(&sid);
            return Ok(None);
        }
        let pending = view
            .http_preparing
            .entry(sid)
            .or_insert_with(|| crate::webview::http_native::PendingNative::new(session));
        match pending.poll(engine.runtime.waker.clone()) {
            Ok(Some(document)) => {
                view.http_preparing.remove(&sid);
                Some(document)
            }
            Ok(None) => return Ok(None),
            Err(error) => {
                view.http_preparing.remove(&sid);
                return Err(crate::webview::WebViewCreateError::Permanent(error));
            }
        }
    } else {
        None
    };
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    let result = if let Some(document) = prepared {
        crate::webview::PlatformWebView::new_http(
            view.base.winit.as_ref(),
            active_html.get(&sid).copied().unwrap_or(default_bounds()),
            scale_factor,
            sid,
            view.webview_key_bridge.clone(),
            document,
        )
    } else {
        crate::webview::PlatformWebView::new(
            view.base.winit.as_ref(),
            active_html.get(&sid).copied().unwrap_or(default_bounds()),
            scale_factor,
            sid,
            view.webview_key_bridge.clone(),
            find_remote_surface(view, engine, sid)
                .and_then(|rs| rs.lock_html_mirror())
                .map(|mirror| {
                    crate::webview::entry_response::EntryResponseSource::new(&mirror.bodies)
                        .with_notify(std::sync::Arc::clone(&engine.runtime.waker))
                }),
        )
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let result = crate::webview::PlatformWebView::new(
        view.base.winit.as_ref(),
        active_html.get(&sid).copied().unwrap_or(default_bounds()),
        scale_factor,
        sid,
        view.webview_key_bridge.clone(),
        find_remote_surface(view, engine, sid)
            .and_then(|rs| rs.lock_html_mirror())
            .map(|mirror| {
                crate::webview::entry_response::EntryResponseSource::new(&mirror.bodies)
                    .with_notify(std::sync::Arc::clone(&engine.runtime.waker))
            }),
    );
    result.map(Some)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
mod tests {
    use super::awaits_replacement;

    #[test]
    fn a_live_session_without_its_native_owner_waits_for_its_replacement() {
        assert!(awaits_replacement(false, false));
    }

    #[test]
    fn a_failed_or_owned_session_is_prepared_as_before() {
        assert!(!awaits_replacement(true, false));
        assert!(!awaits_replacement(true, true));
        assert!(!awaits_replacement(false, true));
    }
}
