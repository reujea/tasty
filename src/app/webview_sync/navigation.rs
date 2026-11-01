//! Navigation effects retain user/host origin and fetch a fresh remote entry on explicit Reload.
use super::*;

fn reload_received_entry(
    remote: &RemoteSurface,
    mark_origin: fn(&mut tasty_model::html_script::HtmlScriptState),
) {
    perform_received_reload(remote, mark_origin);
}

fn reload_with_native(
    remote: &RemoteSurface,
    view: &MainView,
    sid: u32,
    mark_origin: fn(&mut tasty_model::html_script::HtmlScriptState),
) {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    if remote.http_session.is_some() {
        if !remote
            .lock_html_mirror()
            .is_some_and(|mirror| mirror.connected && mirror.connection.is_active())
        {
            return;
        }
        let Some(wv) = view.webviews.get(&sid) else {
            reload_received_entry(remote, mark_origin);
            return;
        };
        match wv.request_http_reload() {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                tracing::warn!(%error,"native HTTP Reload refused");
                return;
            }
        }
    }
    // Platforms without a native HTTP viewer reload the received entry only.
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let _ = (view, sid);
    reload_received_entry(remote, mark_origin);
}
fn perform_received_reload(
    remote: &RemoteSurface,
    mark_origin: fn(&mut tasty_model::html_script::HtmlScriptState),
) {
    if remote
        .lock_html_mirror()
        .is_some_and(|mut mirror| mirror.user_reload())
    {
        remote.with_html_script(|state| {
            mark_origin(state);
            state.on_load_started();
        });
    }
}

/// User navigation marks only a load that actually starts.
pub(super) fn apply_user_nav(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    nav: crate::webview::WebViewUserNav,
) {
    use crate::webview::WebViewUserNav;
    if let Some(rs) = find_remote_surface(view, engine, sid)
        && rs.html_mirror.is_some()
    {
        if matches!(nav, WebViewUserNav::Reload) {
            reload_with_native(
                rs,
                view,
                sid,
                tasty_model::html_script::HtmlScriptState::on_user_load_requested,
            );
        }
        return;
    }
    let Some(wv) = view.webviews.get(&sid) else {
        tracing::warn!("webview user navigation {nav:?}: surface {sid} has no webview");
        return;
    };
    let mark_user_origin = match plan_user_nav(&nav, &wv.nav_info()) {
        UserNavPlan::Skip => {
            tracing::debug!("WebView surface {sid}: user navigation {nav:?} has nothing to load");
            return;
        }
        UserNavPlan::Navigate { mark_user_origin } => mark_user_origin,
    };
    if mark_user_origin && let Some(rs) = find_remote_surface(view, engine, sid) {
        rs.with_html_script(|st| st.on_user_load_requested());
    }
    match nav {
        WebViewUserNav::Back => wv.go_back(),
        WebViewUserNav::Forward => wv.go_forward(),
        WebViewUserNav::Reload => wv.reload(),
        WebViewUserNav::Open(url) => wv.load_url(&url),
    }
}

/// Agent reload keeps host origin while requesting a fresh received entry.
pub(super) fn apply_agent_history_step(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    step: crate::webview::WebViewHistoryStep,
) {
    use crate::webview::WebViewHistoryStep;
    if let Some(rs) = find_remote_surface(view, engine, sid)
        && rs.html_mirror.is_some()
    {
        if step == WebViewHistoryStep::Reload {
            reload_with_native(
                rs,
                view,
                sid,
                tasty_model::html_script::HtmlScriptState::on_host_load_requested,
            );
        }
        return;
    }
    let Some(wv) = view.webviews.get(&sid) else {
        tracing::warn!("webview agent history {step:?}: surface {sid} has no webview");
        return;
    };
    let info = wv.nav_info();
    let plan = plan_agent_history_step(step, &info);
    tracing::debug!(
        "WebView surface {sid}: agent history {step:?} -> {plan:?} (back={}, forward={})",
        info.can_go_back,
        info.can_go_forward
    );
    if plan == AgentStepPlan::Skip {
        return;
    }
    if let Some(rs) = find_remote_surface(view, engine, sid) {
        rs.with_html_script(|st| mark_agent_step(plan, st));
    }
    match step {
        WebViewHistoryStep::Back => wv.go_back(),
        WebViewHistoryStep::Forward => wv.go_forward(),
        WebViewHistoryStep::Reload => wv.reload(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tasty_ipc::html_transfer::TransferId;
    use tasty_model::html_script::{
        BannerPhase, Fingerprint, HtmlScriptState, ScriptDetection, ScriptScan,
    };

    fn remote() -> RemoteSurface {
        let (sender, _frames) = tasty_remote::connection::channel();
        let mut mirror = tasty_remote::html_mirror::HtmlMirror::new(7, 9, sender.epoch());
        assert_eq!(
            mirror
                .begin(TransferId {
                    epoch: 1,
                    surface_id: 7,
                    binding: 9,
                    document: 1,
                    request: 1,
                    transfer: 1
                })
                .len(),
            1
        );
        let mut remote = RemoteSurface::new(7, "html", "com.tasty.html".into(), "html".into());
        remote.html_mirror = Some(Arc::new(Mutex::new(mirror)));
        remote.with_html_script(|state| {
            *state = HtmlScriptState::new(true);
            state.on_load_started();
            state.on_main_response("file:///old.html", None);
            state.on_committed(Some("file:///old.html"));
            state.on_load_finished();
            state.on_user_view();
            state.update_banner();
        });
        remote
    }
    #[test]
    fn explicit_remote_reload_requests_a_new_generation_and_preserves_origin() {
        for user in [true, false] {
            let remote = remote();
            let mark = if user {
                HtmlScriptState::on_user_load_requested
            } else {
                HtmlScriptState::on_host_load_requested
            };
            reload_received_entry(&remote, mark);
            let mirror = remote.lock_html_mirror().unwrap();
            assert_eq!(mirror.generation, 2);
            assert!(mirror.needs_request());
            drop(mirror);
            remote.with_html_script(|state| {
                assert!(state.loading_before_commit());
                state.on_main_response(
                    "file:///new.html",
                    Some(ScriptScan {
                        fingerprint: Fingerprint([4; 32]),
                        detection: ScriptDetection::Scripts,
                        remote_refs: false,
                        remote_scripts: false,
                    }),
                );
                state.on_committed(Some("file:///new.html"));
                state.on_load_finished();
                state.update_banner();
                assert_eq!(
                    state.banner_phase(),
                    if user {
                        BannerPhase::Blocked
                    } else {
                        BannerPhase::Hidden
                    }
                );
                assert_eq!(state.banner().pending_view, !user);
            });
        }
    }
    #[test]
    fn disconnected_remote_reload_preserves_the_existing_page_and_origin() {
        let remote = remote();
        remote.lock_html_mirror().unwrap().disconnect();
        reload_received_entry(&remote, HtmlScriptState::on_host_load_requested);
        assert_eq!(remote.lock_html_mirror().unwrap().generation, 1);
        remote.with_html_script(|state| {
            assert!(!state.loading_before_commit());
            assert!(state.banner().viewed);
            assert_eq!(state.current().unwrap().url, "file:///old.html");
        });
    }
}
