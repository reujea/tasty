//! HTML display values and one-shot decisions retain their captured document and resource identity.
use super::surface_binding::SurfaceBinding;
use crate::core::origin::IntentOrigin;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::engine_access::EngineMut;
use std::sync::{Arc, Mutex, Weak};
use tasty_model::html_script::{
    BannerPhase, DocumentRecord, HtmlScriptState, RemoteMarker, ScriptMarker,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum HtmlActionKind {
    Allow,
    Dismiss,
    Reshow,
    /// 원격 콘텐츠 안내의 허용·닫기·표지로 다시 보이기. 스크립트 결정과 독립이다.
    RemoteAllow,
    RemoteDismiss,
    RemoteReshow,
}
#[derive(Clone, Debug)]
pub(crate) struct HtmlAction {
    target: SurfaceBinding,
    surface: u32,
    owner: Weak<Mutex<HtmlScriptState>>,
    document: Option<DocumentRecord>,
    kind: HtmlActionKind,
    consumed: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Clone)]
pub(crate) struct HtmlSnapshot {
    pub(crate) phase: BannerPhase,
    /// 스크립트 안내가 원격 안내와 함께 보이며 원격 스크립트도 막혀 있다. 본문을 `body_remote`로 바꾼다.
    pub(crate) remote_body: bool,
    /// 화면 문서가 로컬 view에 연 `http(s)` 페이지다. 두 안내와 allowed 표지 툴팁이 페이지 문구를 쓴다.
    pub(crate) web: bool,
    /// 전역 script sandbox. 꺼져 있으면 페이지의 원격 안내 본문이 네트워크 스크립트를 목록에 넣는다.
    pub(crate) sandbox: bool,
    pub(crate) marker: Option<ScriptMarker>,
    /// 원격 콘텐츠 안내 단계.
    pub(crate) remote_phase: BannerPhase,
    /// 주소 칸 끝 원격 표지. 안내가 보이는 동안 그리지 않는 것은 호출자가 정한다.
    pub(crate) remote_marker: Option<RemoteMarker>,
    /// 마지막 로드가 실패한 채 새 문서가 commit되지 않았다.
    pub(crate) load_failed: bool,
    request: HtmlAction,
}
impl HtmlSnapshot {
    pub(crate) fn request(&self, kind: HtmlActionKind) -> HtmlAction {
        let mut request = self.request.clone();
        request.kind = kind;
        request.consumed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        request
    }
}
pub(crate) fn snapshot(remote: &RemoteSurface, target: SurfaceBinding) -> HtmlSnapshot {
    let surface = remote.id;
    remote.with_html_script(|state| HtmlSnapshot {
        phase: state.banner_phase(),
        remote_body: state.script_banner_needs_remote_body(),
        web: state.current_is_web_page(),
        sandbox: state.sandbox(),
        marker: state.marker(),
        remote_phase: state.remote_banner_phase(),
        remote_marker: state.remote_marker(),
        load_failed: state.load_failed(),
        request: HtmlAction {
            target,
            surface,
            owner: Arc::downgrade(&remote.html_script),
            document: state.current().cloned(),
            kind: HtmlActionKind::Dismiss,
            consumed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
    })
}
impl HtmlAction {
    /// 허용은 사용자 결정이다. 사용자 출처가 아닌 허용 요청은 소비하지 않고 거절한다.
    pub(crate) fn apply(&self, engine: &mut EngineMut<'_>, origin: &IntentOrigin) {
        if matches!(
            self.kind,
            HtmlActionKind::Allow | HtmlActionKind::RemoteAllow
        ) && !origin.is_user()
        {
            tracing::warn!(
                surface = self.surface,
                ?origin,
                "html script allowance refused: not a user request"
            );
            return;
        }
        if self
            .consumed
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }
        if !self.target.current(&engine.as_ref()) {
            return;
        }
        let Some(remote) = engine
            .runtime
            .surfaces
            .get(&self.surface)
            .and_then(|surface| surface.as_any().downcast_ref::<RemoteSurface>())
        else {
            return;
        };
        if !Arc::downgrade(&remote.html_script).ptr_eq(&self.owner) {
            return;
        }
        remote.with_html_script(|state| {
            apply_decision(state, self.document.as_ref(), self.kind, self.surface)
        });
    }
}

fn apply_decision(
    state: &mut HtmlScriptState,
    document: Option<&DocumentRecord>,
    kind: HtmlActionKind,
    surface: u32,
) {
    // Record identity is the existing policy's fragment-free URL and content fingerprint.
    if state.current() != document {
        return;
    }
    match kind {
        HtmlActionKind::Allow => allow_scripts(state, surface),
        HtmlActionKind::Dismiss => state.dismiss_banner(),
        HtmlActionKind::Reshow => state.reshow_banner(),
        HtmlActionKind::RemoteAllow => allow_remote(state, surface),
        HtmlActionKind::RemoteDismiss => state.dismiss_remote_banner(),
        HtmlActionKind::RemoteReshow => state.reshow_remote_banner(),
    }
}

fn allow_scripts(state: &mut HtmlScriptState, surface: u32) {
    match state.allow_current() {
        Ok(()) => tracing::info!(surface, "html scripts allowed by the user"),
        Err(error) => tracing::warn!(surface, %error, "html script allowance refused"),
    }
}

fn allow_remote(state: &mut HtmlScriptState, surface: u32) {
    match state.allow_remote_current() {
        Ok(()) => tracing::info!(surface, "html remote content allowed by the user"),
        Err(error) => tracing::warn!(surface, %error, "html remote content allowance refused"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_model::html_script::{Fingerprint, ScriptDetection, ScriptScan};

    fn commit(state: &mut HtmlScriptState, url: &str, byte: u8) {
        state.on_load_started();
        state.on_main_response(
            url,
            Some(ScriptScan {
                fingerprint: Fingerprint([byte; 32]),
                detection: ScriptDetection::Scripts,
                remote_refs: false,
                remote_scripts: false,
            }),
        );
        state.on_committed(Some(url));
        let _ = state.on_load_finished(); // The fixture needs committed state, not the load effect.
    }

    #[test]
    fn a_delayed_banner_decision_cannot_allow_or_dismiss_a_replaced_document() {
        let mut state = HtmlScriptState::new(true);
        commit(&mut state, "file:///document.html", 1);
        let drawn = state.current().cloned();
        commit(&mut state, "file:///document.html", 2);
        apply_decision(&mut state, drawn.as_ref(), HtmlActionKind::Allow, 1);
        assert!(state.allowance().is_none());
        assert!(!state.take_reload_request());
        apply_decision(&mut state, drawn.as_ref(), HtmlActionKind::Dismiss, 1);
        assert!(!state.banner().dismissed);
        let current = state.current().cloned();
        apply_decision(&mut state, current.as_ref(), HtmlActionKind::Allow, 1);
        assert!(state.current_is_allowed());
        assert!(state.take_reload_request());
        assert!(!state.take_reload_request());
    }

    /// 원격 결정도 그린 문서에 묶인다. 바뀐 문서에는 허용·닫기가 닿지 않고 스크립트 허용과 독립이다.
    #[test]
    fn a_delayed_remote_decision_cannot_reach_a_replaced_document() {
        let mut state = HtmlScriptState::new(true);
        commit(&mut state, "file:///document.html", 1);
        let drawn = state.current().cloned();
        commit(&mut state, "file:///document.html", 2);
        apply_decision(&mut state, drawn.as_ref(), HtmlActionKind::RemoteAllow, 1);
        assert!(state.remote_allowance().is_none());
        apply_decision(&mut state, drawn.as_ref(), HtmlActionKind::RemoteDismiss, 1);
        assert!(!state.remote_banner().dismissed);
        let current = state.current().cloned();
        apply_decision(&mut state, current.as_ref(), HtmlActionKind::RemoteAllow, 1);
        assert!(state.current_is_remote_allowed());
        assert!(!state.current_is_allowed(), "scripts stay blocked");
        assert!(state.take_reload_request());
        apply_decision(
            &mut state,
            current.as_ref(),
            HtmlActionKind::RemoteDismiss,
            1,
        );
        assert!(state.remote_banner().dismissed);
        apply_decision(
            &mut state,
            current.as_ref(),
            HtmlActionKind::RemoteReshow,
            1,
        );
        assert!(!state.remote_banner().dismissed);
    }
}
