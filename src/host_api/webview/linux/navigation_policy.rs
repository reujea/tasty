//! Install native navigation and response decisions on the owning WebKit thread.
use super::*;

pub(super) fn install(
    webview: &WebView,
    entry_source: &Option<EntryResponseSource>,
    http: Option<Rc<RefCell<std::sync::Arc<crate::app::http_session::HttpSession>>>>,
    remote: &RemoteBlock,
    pending_navigations: &Rc<RefCell<Vec<PendingNavigation>>>,
    script_gate: &Rc<RefCell<Option<ScriptGate>>>,
    surface_id: u32,
) {
    {
        let entry = entry_source.clone();
        let remote = remote.clone();
        let pending_nav = pending_navigations.clone();
        let gate = script_gate.clone();
        webview.connect_decide_policy(move |wv, decision, decision_type| {
            let uri = match decision_type {
                PolicyDecisionType::Response => decision
                    .downcast_ref::<ResponsePolicyDecision>()
                    .and_then(|d| d.request())
                    .and_then(|r| r.uri()),
                PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction => {
                    decision
                        .downcast_ref::<NavigationPolicyDecision>()
                        .and_then(|d| d.navigation_action())
                        .and_then(|a| a.request())
                        .and_then(|r| r.uri())
                }
                _ => None,
            };
            if let Some(source) = &entry
                && (matches!(decision_type, PolicyDecisionType::NewWindowAction)
                    || uri
                        .as_ref()
                        .is_none_or(|uri| !source.allows_uri(uri.as_str())))
            {
                decision.ignore();
                return true;
            }
            discard_nonmain(decision, decision_type, uri.as_deref(), http.as_ref());
            // navigation action에는 frame 정보가 없어 main frame 응답에서만 문서 JS를 정한다(docs/surfaces/html/index.md).
            if matches!(decision_type, PolicyDecisionType::Response)
                && let Some(gate) = gate.borrow().as_ref()
                && decision
                    .downcast_ref::<ResponsePolicyDecision>()
                    .is_some_and(|d| d.is_main_frame_main_resource())
                && let Some(uri) = &uri
                && decide_main(
                    wv,
                    decision,
                    uri.as_str(),
                    gate,
                    &remote,
                    http.as_ref(),
                    surface_id,
                )
            {
                return true;
            }

            if let Some(uri) = &uri
                && !matches!(decision_type, PolicyDecisionType::Response)
            {
                tracing::debug!(
                    "WebView surface {surface_id}: navigation {decision_type:?} {}",
                    uri.as_str()
                );
            }
            // 탐색 시도는 차단 여부와 별개로 기록한다. 응답 정책 이벤트는 제외한다.
            if matches!(
                decision_type,
                PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction
            ) && let Some(uri) = &uri
            {
                let user_gesture = decision
                    .downcast_ref::<NavigationPolicyDecision>()
                    .and_then(|d| d.navigation_action())
                    .is_some_and(|a| a.is_user_gesture());
                pending_nav.borrow_mut().push(PendingNavigation {
                    url: uri.as_str().to_string(),
                    user_gesture,
                });
            }
            if http.is_some() || !remote.block_cell().get() {
                return false;
            }
            if let Some(uri) = &uri {
                let s = uri.as_str();
                if s.starts_with("http://") || s.starts_with("https://") {
                    decision.ignore();
                    return true;
                }
            }
            false
        });
    }
}

type NativeSessionSlot = Rc<RefCell<std::sync::Arc<crate::app::http_session::HttpSession>>>;
fn decide_main(
    wv: &WebView,
    decision: &webkit2gtk::PolicyDecision,
    uri: &str,
    gate: &ScriptGate,
    remote: &RemoteBlock,
    http: Option<&NativeSessionSlot>,
    surface_id: u32,
) -> bool {
    let response = if let Some(http) = http {
        let session = http.borrow().clone();
        let method = decision
            .downcast_ref::<ResponsePolicyDecision>()
            .and_then(|d| d.request())
            .and_then(|r| r.http_method());
        let body = method
            .as_ref()
            .ok_or_else(|| "native main response method unavailable".to_owned())
            .and_then(|method| session.claim_main_response(uri, method.as_str()));
        match body {
            Ok(body) => {
                let js = gate.main_response_scanned(uri, Some(body.scan()));
                session.bodies.finish_replay(&body);
                Response::Decided(js)
            }
            Err(error) => {
                tracing::warn!(%error,"native HTTP main response refused");
                session.fail();
                decision.ignore();
                return true;
            }
        }
    } else {
        gate.main_response(uri)
    };
    match response {
        Response::Decided(js) => {
            set_js(wv, js);
            remote.sync(gate);
        }
        Response::Deferred(scan) => {
            // 스캔이 JS 값을 정하는 응답(허용한 문서의 재로드)은 결정을 보류하고
            // 작업 스레드에서 읽는다. GTK 루프는 그동안 다른 이벤트를 처리한다.
            defer_response(
                wv.clone(),
                decision.clone(),
                scan,
                remote.clone(),
                surface_id,
            );
            return true;
        }
    }
    false
}

fn discard_nonmain(
    decision: &webkit2gtk::PolicyDecision,
    kind: PolicyDecisionType,
    uri: Option<&str>,
    http: Option<&NativeSessionSlot>,
) {
    if matches!(kind, PolicyDecisionType::Response)
        && let Some(http) = http
        && let Some(response) = decision.downcast_ref::<ResponsePolicyDecision>()
        && !response.is_main_frame_main_resource()
        && let Some(uri) = uri
        && let Some(method) = response.request().and_then(|request| request.http_method())
    {
        // Discard every ambiguous candidate; a nonmain policy cannot mint authority.
        http.borrow().discard_nonmain_response(uri, method.as_str());
    }
}
