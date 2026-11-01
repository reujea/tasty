//! HTML display values and one-shot decisions retain their captured document and resource identity.
use super::surface_binding::SurfaceBinding;
use crate::core::origin::IntentOrigin;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::engine_access::EngineMut;
use std::sync::{Arc, Mutex, Weak};
use tasty_model::html_script::{BannerPhase, DocumentRecord, HtmlScriptState, ScriptMarker};

#[derive(Clone, Copy, Debug)]
pub(crate) enum HtmlActionKind {
    Allow,
    Dismiss,
    Reshow,
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
    pub(crate) marker: Option<ScriptMarker>,
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
        marker: state.marker(),
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
        if matches!(self.kind, HtmlActionKind::Allow) && !origin.is_user() {
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
        HtmlActionKind::Allow => match state.allow_current() {
            Ok(()) => tracing::info!(surface, "html scripts allowed by the user"),
            Err(error) => tracing::warn!(surface, %error, "html script allowance refused"),
        },
        HtmlActionKind::Dismiss => state.dismiss_banner(),
        HtmlActionKind::Reshow => state.reshow_banner(),
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
}
