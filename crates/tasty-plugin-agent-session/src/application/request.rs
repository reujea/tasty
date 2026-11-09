//! 요청 인자에서 대상(surface · 세션 · 보낼 말)을 읽는다 — IPC 처리기와 탭 쪽 로직이 함께 쓴다.
//!
//! 사유는 [`AppError`] 로 돌려준다. IPC 오류로 옮기는 것은 답하는 쪽(`ipc.rs`)이 한다.

use serde_json::Value;

use crate::application::app_error::AppError;
use crate::ports::host::HostCall;
use crate::{domain::i18n, ports::supervisor};

pub(crate) fn require_surface(params: &Value) -> Result<u32, AppError> {
    params
        .get("surface_id")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| AppError::new(i18n::t("sup.err.need_surface_id")))
}

/// surface 로 세션을 찾는다 — 안 이어져 있으면 **다음 행동을 말한다**.
pub(crate) fn session_for(host: &impl HostCall, params: &Value) -> Result<(u32, String), AppError> {
    let surface_id = require_surface(params)?;
    // 사유는 그대로 올린다 — `session_of_surface` 가 이미 다음 행동을 말한다.
    // 여기서 한 번 더 붙이면 같은 문장이 두 번 나온다(실측으로 그랬다).
    let session = crate::application::surface_link::session_of_surface(host, surface_id)
        .map_err(|e| AppError::new(e.to_string()))?;
    Ok((surface_id, session))
}

/// `tell` 로 보낼 말 — 위치 인자(`message`)나 `--text`(`text`) 중 하나.
///
/// claude plugin 의 `tell` 은 본문을 위치 인자로 받는다. 같은 이름의 명령이 본문을 다르게 받으면
/// 에이전트가 한쪽 형태로 다른 쪽을 부르다 실패한다(`--text` 누락) — 둘 다 받는다.
pub(crate) fn tell_text(params: &Value) -> Result<String, AppError> {
    let pick = |k: &str| {
        params
            .get(k)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
    };
    match (pick("message"), pick("text")) {
        (Some(_), Some(_)) => Err(AppError::new(i18n::t("sup.err.text_twice"))),
        (Some(t), None) | (None, Some(t)) => Ok(t.to_string()),
        (None, None) => Err(AppError::new(i18n::t("sup.err.need_text"))),
    }
}

/// 탭에 이어진 세션에 대한 오류 — 그 세션이 없으면 **연결이 낡았다**고 말한다.
///
/// `그런 세션이 없다` 만으로는 id 를 잘못 친 것인지 탭이 지워진 세션을 가리키는 것인지
/// 모른다. 여기서는 id 를 surface 의 meta 에서 읽었으므로 후자다 — 끊고 다시 잇는 법을 준다.
pub(crate) fn linked_error(surface_id: u32, session: &str, e: supervisor::ClientError) -> AppError {
    match &e {
        supervisor::ClientError::Daemon { kind, .. } if kind == "not_found" => AppError::new({
            let s = surface_id.to_string();
            i18n::tn("sup.err.stale_link", &[&s, session, &s])
        }),
        _ => AppError::new(e.to_string()),
    }
}
