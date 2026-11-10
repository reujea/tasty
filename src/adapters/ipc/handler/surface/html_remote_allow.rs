//! `surface.html_remote_allow` — 에이전트가 html surface의 현재 문서에 원격 콘텐츠를 허용한다.
//!
//! 원격 안내의 허용 버튼과 같은 `HtmlScriptState::allow_remote_current`로 문서 단위 허용을 남기고
//! 재로드를 요청한다. 스크립트 허용과 따로 기록하며 서로를 바꾸지 않는다. 대상은 surface ID로만 받고
//! 포커스·선택·활성 탭·안내 표지는 바꾸지 않는다. 조회한 문서와 허용할 문서가 달라지는 경쟁은
//! `surface.html_script` 응답의 `url`·`fingerprint`로 막는다(docs/surfaces/html/index.md). 지문 없는
//! `http(s)` 페이지는 `url`로만 확인하고, `fingerprint`를 주면 다른 문서로 보고 거절한다.

use serde_json::{Value, json};
use tasty_ipc::protocol::JsonRpcResponse;
use tasty_model::html_script::{AllowError, HtmlScriptState, RemoteAllowError};

use super::html_script::find_html;
use super::html_script_allow::{
    DOCUMENT_CHANGED, HTML_SCRIPT_ALLOW_REFUSED, document_args, document_changed, document_json,
};

#[derive(Debug, PartialEq)]
enum Outcome {
    Allowed,
    AlreadyAllowed,
    Refused(&'static str, String),
}

fn decide(st: &mut HtmlScriptState, url: Option<&str>, fingerprint: Option<&str>) -> Outcome {
    // 원격 허용을 받을 수 없는 view·설정이면 문서와 관계없이 그 이유를 먼저 답한다.
    if !st.remote_supported() {
        return refused(RemoteAllowError::UnsupportedView);
    }
    if st.remote_global() {
        return refused(RemoteAllowError::GloballyAllowed);
    }
    if st.loading_before_commit() {
        return refused(RemoteAllowError::Document(AllowError::Loading));
    }
    if st.current().is_none() {
        return refused(RemoteAllowError::Document(AllowError::NoDocument));
    }
    if document_changed(st, url, fingerprint) {
        return Outcome::Refused("document_changed", DOCUMENT_CHANGED.to_string());
    }
    if st.current_is_remote_allowed() {
        return Outcome::AlreadyAllowed;
    }
    match st.allow_remote_current() {
        Ok(()) => Outcome::Allowed,
        Err(e) => refused(e),
    }
}

fn refused(e: RemoteAllowError) -> Outcome {
    let reason = match e {
        RemoteAllowError::UnsupportedView => "unsupported_view",
        RemoteAllowError::GloballyAllowed => "globally_allowed",
        RemoteAllowError::Document(AllowError::Loading) => "loading",
        RemoteAllowError::Document(AllowError::NoDocument) => "no_document",
        RemoteAllowError::Document(AllowError::NoFingerprint) => "no_fingerprint",
    };
    Outcome::Refused(reason, e.to_string())
}

pub(crate) fn handle_html_remote_allow(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let (url, fingerprint) = match document_args(params, &id) {
        Ok(args) => args,
        Err(e) => return e,
    };
    let (surface_id, rs) = match find_html(engine, &id, params) {
        Ok(found) => found,
        Err(e) => return e,
    };
    let (outcome, document) = rs.with_html_script(|st| {
        let outcome = decide(st, url, fingerprint);
        (outcome, document_json(st))
    });
    match outcome {
        Outcome::Allowed | Outcome::AlreadyAllowed => {
            let allowed_now = outcome == Outcome::Allowed;
            tracing::info!(
                surface_id,
                allowed_now,
                "html remote content allowed by an agent"
            );
            JsonRpcResponse::success(
                id,
                json!({
                    "surface_id": surface_id,
                    "result": if allowed_now { "allowed" } else { "already_allowed" },
                    "reload": allowed_now,
                    "document": document,
                }),
            )
        }
        Outcome::Refused(reason, message) => {
            tracing::warn!(
                surface_id,
                reason,
                "surface.html_remote_allow refused: {message}"
            );
            JsonRpcResponse::error_with_data(
                id,
                HTML_SCRIPT_ALLOW_REFUSED,
                message,
                json!({ "reason": reason, "surface_id": surface_id, "document": document }),
            )
        }
    }
}

#[cfg(test)]
mod tests;
