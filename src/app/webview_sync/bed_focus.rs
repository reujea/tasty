//! 바탕 kind WebView의 페이지에 바탕 포커스 상태를 넘긴다. 값은 `state::surface_focus::bed_focused`가
//! 세션의 포커스와 점유로 정하고, 페이지 쪽 계약은 `crate::webview::bed_focus`에 있다.

use crate::state::surface_focus::{FocusMark, bed_focused, focus_mark};
use crate::view::MainView;
use crate::webview::PlatformWebView;

use super::find_remote_surface;

/// 페이지가 상태를 받는 surface인지. 바탕 kind인 WebView만 받는다.
/// html은 원격·mirror를 포함해 띠 kind라 받지 않고, 페이지에 아무것도 넣지 않는다.
fn receives(kind: &str, plugin_id: &str) -> bool {
    focus_mark(kind, Some(plugin_id)) == FocusMark::Bed
}

/// surface의 목표 상태. 받지 않는 surface는 `None`이다.
fn target(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
) -> Option<bool> {
    let remote = find_remote_surface(view, engine, sid)?;
    receives(remote.kind_static, &remote.plugin_id).then(|| {
        bed_focused(
            view.state.focused_surface_id(engine),
            sid,
            &engine.live.occupancy,
        )
    })
}

#[cfg(target_os = "linux")]
fn apply(wv: &PlatformWebView, focused: bool) {
    wv.set_bed_focused(focused);
}

/// macOS·Windows 백엔드에는 이 연산이 아직 없다(`docs/design/systems/webview.md` 연산 표).
#[cfg(not(target_os = "linux"))]
fn apply(_wv: &PlatformWebView, _focused: bool) {}

/// 새 WebView에 첫 문서를 싣기 전에 상태를 둔다. 포커스를 알면 첫 렌더부터 그 값이다.
pub(super) fn apply_initial(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    wv: &PlatformWebView,
) {
    if let Some(focused) = target(view, engine, sid) {
        apply(wv, focused);
        view.webview_bed_focus.insert(sid, focused);
    }
}

/// 상태가 바뀐 WebView에만 다시 둔다. 숨긴 WebView도 갱신해 다시 보일 때 숨기기 전 값을 쓰지 않는다.
pub(super) fn sync(view: &mut MainView, engine: &crate::runtime::engine_access::EngineRef<'_>) {
    let sids: Vec<u32> = view.webviews.keys().copied().collect();
    for sid in sids {
        let Some(focused) = target(view, engine, sid) else {
            continue;
        };
        if view.webview_bed_focus.get(&sid) == Some(&focused) {
            continue;
        }
        if let Some(wv) = view.webviews.get(&sid) {
            apply(wv, focused);
        }
        view.webview_bed_focus.insert(sid, focused);
    }
}

#[cfg(test)]
mod tests {
    use super::receives;

    #[test]
    fn only_the_bundled_markdown_page_receives_the_state() {
        assert!(receives("markdown", "com.tasty.markdown"));
        assert!(!receives("html", "com.tasty.html"));
        assert!(!receives("markdown", "com.example.markdown"));
        assert!(!receives("custom-view", "com.example.view"));
    }
}
