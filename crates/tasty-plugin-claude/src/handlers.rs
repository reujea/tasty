//! `tasty-claude` 의 IPC handler fn 들 — 외부 plugin SDK 진입점.
//!
//! 자식 terminal 관리(spawn/tell/wait/children/parent/kill/respawn/broadcast)는
//! 호스트가 내재화한 `terminal.*` IPC(ADR-0040 / occupancy-04)로 **위임**한다. 이
//! plugin 은 더 이상 자체 child registry 를 보유하지 않는다(호스트 registry 가 단일
//! SoT). claude **특화**만 여기 남는다:
//! - `start_claude_in_surface` / `issue_session_token` — session token + agent id +
//!   TASTY_SURFACE_ID inline env 를 박은 기동 명령 (spawn/respawn 이 소비).
//! - `build_launch_command` / `handle_launch` — 새 workspace 기동 + error_scan 등록
//!   (top-level). `handle_spawn`/`handle_respawn` 은 자식 surface 를 error_scan 에
//!   등록하고 `handle_kill` 은 내린다.
//! - `handle_children` — 호스트 registry 목록에 `surface.foreground_process` 를 덧씌움.
//! - wall-time 텔레메트리 타이밍(`ClaudeState`) — hook.rs 가 소비.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tasty_plugin_agent_common::children::{join_indices, spawn_census};
use tasty_plugin_agent_common::host_call::HostCall;
#[cfg(test)]
use tasty_plugin_agent_common::host_call::cleanup_sibling_hooks;
use tasty_plugin_agent_common::params::{
    TargetSurfaceError, U32FieldError, forward, target_surface,
};
use tasty_plugin_agent_common::prompt_file;
use tasty_plugin_sdk::{HostHandle, IpcMethodError, i18n::Translator};

use crate::error_scan::{ErrorScanner, ScanTarget};
use crate::reboot::reboot_surface;

/// `profile_file`(직접 지정한 경로)과 `profile`(레지스트리에 등록된 이름 목록,
/// 쉼표 구분 — `profile.rs` 참고) params 를 최종 `--settings` 파일 경로 하나로
/// 해석한다. 둘 다 주어지면
/// 어느 쪽이 이기는지 조용히 정하지 않고 즉시 거부한다 — last-wins 함정을
/// 경로/이름 인자 사이에서도 반복하지 않기 위함.
pub(crate) fn resolve_profile_file_param(
    data_dir: Option<&Path>,
    params: &Value,
    tr: &Translator,
) -> Result<Option<String>, IpcMethodError> {
    let path = params
        .get("profile_file")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let names = params
        .get("profile")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    match (path, names) {
        (Some(_), Some(_)) => Err(IpcMethodError::new(
            tr.t("claude.profile.mutually_exclusive_file_and_profile"),
        )),
        (Some(p), None) => Ok(Some(p.to_string())),
        (None, Some(n)) => crate::profile::resolve_names(data_dir, n, tr)
            .map(|p| Some(p.to_string_lossy().into_owned()))
            .map_err(|e| crate::profile::to_ipc_err(e, tr)),
        (None, None) => Ok(None),
    }
}

/// Claude Code `--permission-mode` 가 받는 값 집합. 외부 도구의 계약이라 실측으로
/// 얻었다(`claude --help`, 2026-09-12). 각 값이 무엇을 뜻하는지는 Claude Code 가
/// 정하며 이 plugin 은 해석하지 않고 전달만 한다 — 여기서 하는 일은 **모르는 값을
/// 조용히 흘려보내지 않는 것**뿐이다(`docs/adr/0265-child-approval-policy-is-the-callers-choice.md`
/// 결정 7).
pub(crate) const VALID_PERMISSION_MODES: &[&str] = &[
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "manual",
    "dontAsk",
    "plan",
];

/// 승인 정책 기본값을 담는 plugin 설정 키. 매니페스트
/// `[[contributes.settings_pages.items]]` 의 `storage_key` 와 같은 문자열이어야 한다.
const PERMISSION_MODE_SETTING_KEY: &str = "default_permission_mode";

/// `--permission-mode` 조각(또는 빈 문자열). 앞에 공백을 포함해 기존 명령 문자열
/// 뒤에 그대로 이어 붙인다.
fn permission_mode_flag(mode: Option<&str>) -> String {
    match mode {
        Some(m) => format!(" --permission-mode {m}"),
        None => String::new(),
    }
}

/// settings JSON 이 **권한 모드**를 정하고 있는가. 순수 함수 — 단위 테스트 대상.
///
/// `permissions.allow`/`deny` 는 규칙 목록이지 모드가 아니라 `--permission-mode` 와
/// 축이 달라 충돌로 보지 않는다. 모드를 정하는 키는 `permissions.defaultMode`
/// 하나다.
fn settings_json_sets_default_mode(settings: &Value) -> bool {
    settings
        .get("permissions")
        .and_then(|p| p.get("defaultMode"))
        .is_some()
}

/// `--profile`/`--profile-file` 로 주입될 settings 파일이 권한 모드를 정하는지.
/// 읽기·파싱 실패는 "정하지 않는다" 로 본다 — 이 판정의 목적은 **두 축이 같은 값을
/// 놓고 조용히 경쟁하는 것**을 막는 것이라, 읽을 수 없는 파일 때문에 기동 자체를
/// 막지는 않는다(그 파일이 실제로 깨졌다면 Claude Code 가 자기 자리에서 말한다).
fn profile_file_sets_default_mode(path: &str) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .is_some_and(|v| settings_json_sets_default_mode(&v))
}

/// 전역 설정(`default_permission_mode`)의 fallback 값. 미설정·`"inherit"` 이면
/// `None`(= 플래그를 안 붙인다). 설정에 모르는 값이 들어 있으면 기동을 막지 않고
/// 경고만 남기고 무시한다 — 설정 항목은 select 라 정상 경로로는 생길 수 없는 값이고,
/// 그것 때문에 모든 기동이 실패하면 사용자가 복구할 창구가 좁다.
fn default_permission_mode<H: HostCall>(host: &H) -> Option<String> {
    let value = host
        .call(
            "settings.get_plugin_setting",
            json!({ "storage_key": PERMISSION_MODE_SETTING_KEY }),
        )
        .ok()
        .and_then(|v| v.get("value").and_then(|v| v.as_str()).map(String::from))
        .filter(|s| s != "inherit" && !s.is_empty())?;
    if !VALID_PERMISSION_MODES.contains(&value.as_str()) {
        tracing::warn!("claude: ignoring unknown {PERMISSION_MODE_SETTING_KEY} setting '{value}'");
        return None;
    }
    Some(value)
}

/// `permission_mode` param → 기동 명령에 실릴 값. 우선순위는 **호출별 params >
/// 전역 설정 > 미부착**이고, 아무도 안 고르면 플래그 자체가 안 붙어 자식은 사용자
/// 자신의 Claude Code 설정대로 뜬다
/// (`docs/adr/0265-child-approval-policy-is-the-callers-choice.md` 결정 2·4).
///
/// `profile_file` 이 정하는 settings JSON 에 `permissions.defaultMode` 가 있고
/// `permission_mode` 도 함께 오면 **거부**한다 — 어느 쪽이 이기는지 조용히 정하지
/// 않는다([`resolve_profile_file_param`] 이 `profile_file`+`profile` 조합에서,
/// `profile_merge` 가 `defaultMode` 충돌에서 이미 같은 선택을 했다).
pub(crate) fn resolve_permission_mode<H: HostCall>(
    host: &H,
    params: &Value,
    profile_file: Option<&str>,
    tr: &Translator,
) -> Result<Option<String>, IpcMethodError> {
    let explicit = params
        .get("permission_mode")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let Some(mode) = explicit else {
        return Ok(default_permission_mode(host));
    };
    if !VALID_PERMISSION_MODES.contains(&mode) {
        return Err(IpcMethodError::invalid_params(&tr.t_fmt(
            "claude.params.invalid_permission_mode",
            &format!("{mode} (valid: {})", VALID_PERMISSION_MODES.join(", ")),
        )));
    }
    if profile_file.is_some_and(profile_file_sets_default_mode) {
        return Err(IpcMethodError::invalid_params(
            tr.t("claude.params.permission_mode_conflicts_with_profile"),
        ));
    }
    Ok(Some(mode.to_string()))
}

/// 필수 u32 파라미터를 읽는다 — **없는 것과 잘못된 것을 가른다.**
///
/// `hook.rs` 의 `resolve_surface_id_from` 은 [`optional_target_surface`] 에 **env 폴백**만
/// 더한 것이라 판정 자체는 아래와 같은 한 벌에서 온다. 이쪽은 폴백이 없어 어느 쪽이든
/// 에러지만, **자르기는 여기도 위험하다**:
/// `4_294_967_297 as u32` 는 `1` 이고 `5_000_000_000 as u32` 는 `705_032_704` 다.
/// 둘 다 실재할 수 있는 다른 surface 의 id 라, 못 읽는 값이 조용히 남의 터미널로 간다.
///
/// 메시지도 가른다 — 값이 왔는데 "missing" 이라고 답하면 호출자가 자기가 준 값을
/// 안 의심한다.
/// 판정은 [`tasty_plugin_agent_common::params::u32_field`] 가 한다 — 여기서 하는
/// 일은 그 갈래를 **이 plugin 의 카탈로그 문구로 옮기는 것**뿐이다.
///
/// 문구를 파라미터마다 전용 키로 짓는 것은 짝 crate(codex, `{key}` 를 끼우는 공용
/// 키)와 다르고, 그 차이는 표류가 아니다 — 이쪽은 호출자가 하나(`child_index`)라
/// 전용 문구가 더 정확하고, 저쪽은 다섯이라 공용 키가 카탈로그를 안 불린다.
/// **갈릴 수 있었던 것은 문구가 아니라 판정이었고, 그쪽은 이제 한 벌이다.**
fn require_u32(
    params: &Value,
    key: &str,
    missing_key: &str,
    malformed_key: &str,
    tr: &Translator,
) -> Result<u32, IpcMethodError> {
    tasty_plugin_agent_common::params::u32_field(params, key).map_err(|e| match e {
        U32FieldError::Missing => IpcMethodError::invalid_params(tr.t(missing_key)),
        U32FieldError::Malformed { raw } => {
            IpcMethodError::invalid_params(&tr.t_fmt(malformed_key, &raw))
        }
    })
}

/// 대상 parent surface — 판정은 [`tasty_plugin_agent_common::params::target_surface`]
/// 한 벌이고, 여기서는 그 실패를 **claude 카탈로그의 문구로** 옮기기만 한다.
/// 두 plugin 이 같은 판정을 각자 구현하면 한쪽만 고쳐지는 순간 갈린다 — 이 함수가
/// 고치고 있는 결함 자체가 그 형태로 생겼다.
pub(crate) fn optional_target_surface(
    params: &Value,
    tr: &Translator,
) -> Result<Option<u32>, IpcMethodError> {
    target_surface(params).map_err(|e| match e {
        // `key` 를 버리지 않는다 — 이 판정은 `surface` 와 `surface_id` **두 이름**을 한
        // 필드로 읽으므로, 어느 쪽이 틀렸는지 안 대면 호출자는 자기가 보낸 두 키 중
        // 무엇을 고쳐야 하는지 모른다. 짝 plugin(codex)의 **같은 자리**는 처음부터
        // 그것을 댔다 — 그쪽 훅 경로(`handle_hook`)는 그러지 않았고, 그건 따로 고쳤다.
        // placeholder 형태는 이 crate 의 관례(`{}` 하나 + 조립한 인자)를 따른다 —
        // 바로 아래 `Conflict` 가 같은 형태다.
        TargetSurfaceError::Malformed { key, raw } => IpcMethodError::invalid_params(&tr.t_fmt(
            "claude.params.target_surface_not_a_number",
            &format!("'{key}' = {raw}"),
        )),
        TargetSurfaceError::Conflict {
            surface,
            surface_id,
        } => IpcMethodError::invalid_params(&tr.t_fmt(
            "claude.params.surface_conflict",
            &format!("surface={surface}, surface_id={surface_id}"),
        )),
    })
}

/// 대상 surface — 없으면 거절한다. 판정은 [`optional_target_surface`] 와 같다.
///
/// **이름이 `require_surface_id` 가 아닌 이유.** 이 함수는 `surface` 와 `surface_id`
/// **두 키를 한 필드로** 읽는다. 그런데 이 저장소에는 그 이름이 이미 있고, 거기서는
/// `surface_id` **한 키만** 읽는다(`src/adapters/ipc/handler.rs` 의 호스트 헬퍼와
/// `tasty-plugin-sdk` 의 런타임 헬퍼). 그래서 여기서 그 이름을 쓰면 **한 이름이 두 술어를
/// 가리키고**, 두 키의 어긋남이 조용히 지나가던 것이 바로 그 형태였다 — raw IPC 가
/// `surface_id` 로 지목한 호출이 대상 없는 호출이 되어 호스트의 유일-parent 폴백에
/// 떨어졌다. 수리는 코드에 남지만 오해는 이름에 남는다.
///
/// 실패 문구의 키는 `missing_surface_id` 로 **그대로 둔다** — `missing_target_surface`
/// 는 같은 표에서 이미 다른 뜻이다(`target_surface` 라는 **literal 파라미터**를 받는
/// 메서드용). 문구 자체는 두 키를 다 대고 있어 참이다.
pub(crate) fn require_target_surface(
    params: &Value,
    tr: &Translator,
) -> Result<u32, IpcMethodError> {
    optional_target_surface(params, tr)?
        .ok_or_else(|| IpcMethodError::invalid_params(tr.t("claude.params.missing_surface_id")))
}

/// 호스트로 넘길 params 에 대상 surface 를 싣는다 — 실패 문구만 claude 것으로 옮긴다.
fn put_target_surface(
    dst: &mut serde_json::Map<String, Value>,
    params: &Value,
    tr: &Translator,
) -> Result<(), IpcMethodError> {
    if let Some(surface) = optional_target_surface(params, tr)? {
        dst.insert("surface".into(), json!(surface));
    }
    Ok(())
}

pub(crate) fn require_child_index(params: &Value, tr: &Translator) -> Result<u32, IpcMethodError> {
    require_u32(
        params,
        "child_index",
        "claude.params.missing_child_index",
        "claude.params.child_index_not_a_number",
        tr,
    )
}

/// `--child <index>` 를 그 자식의 **surface id** 로 해석한다.
///
/// `kill`/`respawn` 은 index 를 그대로 `terminal.kill`/`terminal.respawn` 에 넘겨
/// 호스트가 해석하게 두지만, 자식 surface 를 대상으로 다른 명령(예: reboot 경로)을
/// 태우려면 여기서 직접 id 를 알아야 한다.
///
/// **필드명 주의**: 여기서 부르는 것은 claude 특화 remap 된 `claude.children`
/// (`child_surface_id`)이 아니라 **원본** `terminal.children` 이므로 `surface_id` 를
/// 읽는다 — `handle_children` 이 remap 하는 쪽과 헷갈리면 항상 `None` 이 나온다
/// (`compute_spawn_warning` 에도 같은 취지의 경고가 붙어 있다).
pub(crate) fn resolve_child_surface_id<H: HostCall>(
    host: &H,
    parent_surface_id: u32,
    child_index: u32,
    tr: &Translator,
) -> Result<u32, IpcMethodError> {
    let resp = host
        .call("terminal.children", json!({ "surface": parent_surface_id }))
        .map_err(IpcMethodError::from)?;
    let children = resp
        .get("children")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let found = children
        .iter()
        .find(|c| c.get("index").and_then(|v| v.as_u64()) == Some(child_index as u64))
        .and_then(|c| c.get("surface_id").and_then(|v| v.as_u64()))
        .map(|v| v as u32);
    found.ok_or_else(|| {
        // 있는 index 를 함께 보여준다 — "없다" 만으로는 오타인지 자식이 이미
        // 죽은 것인지 호출자가 구분할 수 없다.
        let available: Vec<String> = children
            .iter()
            .filter_map(|c| c.get("index").and_then(|v| v.as_u64()))
            .map(|i| i.to_string())
            .collect();
        let available = if available.is_empty() {
            "-".to_string()
        } else {
            available.join(", ")
        };
        IpcMethodError::invalid_params(
            &tr.t("claude.child.unknown_index")
                .replacen("{}", &child_index.to_string(), 1)
                .replacen("{}", &parent_surface_id.to_string(), 1)
                .replacen("{}", &available, 1),
        )
    })
}

fn host_call<H: HostCall>(host: &H, method: &str, params: Value) -> Result<Value, IpcMethodError> {
    host.call(method, params).map_err(IpcMethodError::from)
}

/// 호스트 registry 목록(`terminal.children`)에 `surface.foreground_process` 로
/// 각 자식의 PTY 전경 프로세스를 덧씌운다. claude 특화 필드명(`child_surface_id`)을
/// 보존하기 위해 호스트 응답(`surface_id`)을 remap 한다. 응답은 bare 배열(claude
/// CLI 출력 shape).
/// ★ 짝 crate(codex)의 같은 함수와 **응답 shape 이 다르다** — 이쪽은 remap 한
/// bare 배열, 저쪽은 호스트 응답 그대로다. 그 차이가 왜 남아 있는지는
/// `tasty_plugin_agent_common` 의 crate doc "짝이 갈린 채 남는 것" 에 한 곳으로
/// 적혀 있다. 여기에 사본을 두지 않는다. 이쪽 shape 을 고정하는 것은
/// `children_response_is_a_bare_remapped_array` 이고, 저쪽 shape 을 고정하는 짝
/// 시험이 codex 에 같은 이름 규칙으로 있다 — 한쪽만 바뀌면 그 시험이 빨개진다.
pub(crate) fn handle_children<H: HostCall>(
    host: &H,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let mut cp = serde_json::Map::new();
    put_target_surface(&mut cp, params, tr)?;
    let resp = host_call(host, "terminal.children", Value::Object(cp))?;
    let list = resp
        .get("children")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut entries: Vec<Value> = list
        .iter()
        .map(|c| {
            json!({
                "child_surface_id": c.get("surface_id").cloned().unwrap_or(Value::Null),
                "index": c.get("index").cloned().unwrap_or(Value::Null),
                "cwd": c.get("cwd").cloned().unwrap_or(Value::Null),
                "role": c.get("role").cloned().unwrap_or(Value::Null),
                "nickname": c.get("nickname").cloned().unwrap_or(Value::Null),
                "state": c.get("state").cloned().unwrap_or(Value::Null),
                // remap 이 화이트리스트라 호스트가 실어 보낸 판정 근거 3 축 중
                // `state` 만 옮기면 나머지 둘이 여기서 잘린다 — `confidence` 가
                // 없으면 소비자가 확정 판정과 휴리스틱을 구분할 수 없다(ADR-0072).
                "evidence": c.get("evidence").cloned().unwrap_or(Value::Null),
                "confidence": c.get("confidence").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    for entry in &mut entries {
        let Some(sid) = entry
            .get("child_surface_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
        else {
            continue;
        };
        if let Ok(resp) = host.call("surface.foreground_process", json!({ "surface_id": sid })) {
            if let Some(name) = resp.get("name").and_then(|v| v.as_str()) {
                entry["foreground_process"] = json!(name);
            }
            if let Some(pid) = resp.get("pid").and_then(|v| v.as_u64()) {
                entry["foreground_pid"] = json!(pid);
            }
        }
    }
    Ok(json!(entries))
}

/// 자식 Claude 를 종료한다 — 호스트 `terminal.kill` 로 위임(surface.close +
/// soft 점유 해제 + registry 제거). `child_index` → 호스트 `child` 매핑.
/// 종료된 surface 는 error scanner 에서도 즉시 내린다.
/// ★ `error_scan` 을 내리는 것은 **의도된 비대칭**이다(codex 에 그 하위 시스템이
/// 없다). 그 옆의 응답 shape 차이(`{killed: true}` vs 호스트 응답 그대로)는 공개
/// 호출자의 호환성을 위해 유지하며, 근거는 `tasty_plugin_agent_common` 의 crate doc "짝이 갈린 채
/// 남는 것" 에 있다. 이쪽 shape 을 고정하는 것은
/// `kill_response_is_reduced_to_a_killed_flag` 이고, 저쪽에 짝 시험이 있다.
pub(crate) fn handle_kill<H: HostCall>(
    scanner: &Arc<Mutex<ErrorScanner>>,
    host: &H,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let child_index = require_child_index(params, tr)?;
    let mut kp = serde_json::Map::new();
    put_target_surface(&mut kp, params, tr)?;
    kp.insert("child".into(), json!(child_index));
    // 호스트 terminal.kill 성공 시 { killed_surface_id, child_index } 반환. claude
    // CLI 는 기존에 { killed: true } 를 기대하므로 성공을 그 shape 으로 변환한다.
    let resp = host_call(host, "terminal.kill", Value::Object(kp))?;
    // 폴링 루프의 생존 대조가 최대 800ms 뒤 어차피 정리하지만, 여기서 즉시 내리면
    // 그 사이 마지막 출력에 대고 `claude-error` 를 발화할 여지가 사라진다.
    if let Some(killed) = resp
        .get("killed_surface_id")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
    {
        crate::error_scan::lock_scanner(scanner).disable(killed);
    }
    Ok(json!({ "killed": true }))
}

/// 부모의 모든(또는 role 필터된) 자식에 텍스트를 broadcast — 호스트
/// `terminal.broadcast` 로 위임.
///
/// ★ 짝 crate(codex)의 같은 함수와 **본문이 글자 그대로 같고**, 갈리는 것은 실패
/// 문구의 카탈로그 키 하나뿐이다. 합치지 않은 이유: 남는 다섯 줄은 판정이 아니라
/// 조립이고, 공용화하려면 문구와 `put_target_surface` 를 **둘 다** 주입해야 한다 —
/// 뒤엣것을 주입하면 이 crate 안에 대상 surface 를 싣는 길이 둘이 되고, 그것이 지금
/// 없애려는 종류의 표류다. 이 조립이 읽는 판정(대상 surface)은 이미
/// [`tasty_plugin_agent_common::params::target_surface`] 한 벌이다.
///
/// 두 키의 ko 어미가 다른 것(`누락` vs `가 없다`)도 이 키 하나의 표류가 아니다 —
/// 카탈로그마다 어미 규약이 통째로 다르다(`[claude.params]` 는 `missing_*` 11 중
/// 10 이 "누락", `[codex.params]` 는 5 중 4 가 "가 없다"). 이 키만 맞추면 자기 파일
/// 안에서 그 키가 예외가 된다. en/ja 는 두 카탈로그가 이미 글자 그대로 같다.
pub(crate) fn handle_broadcast(
    host: &HostHandle,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IpcMethodError::invalid_params(tr.t("claude.params.missing_text")))?;
    let mut bp = forward(params, &["role"]);
    put_target_surface(&mut bp, params, tr)?;
    bp.insert("text".into(), json!(text));
    host_call(host, "terminal.broadcast", Value::Object(bp))
}

/// 자식 Claude 에 메시지를 보낸다 — 호스트 `terminal.tell` 로 위임. 개행/제출
/// 규칙(단일라인 평문 / 멀티라인 bracketed paste + 별도 `\r`)은 호스트가 동일하게
/// 처리하므로 본문 포맷을 재구현하지 않는다.
pub(crate) fn handle_tell(
    host: &HostHandle,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let surface_id = require_target_surface(params, tr)?;
    let message = params
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IpcMethodError::invalid_params(tr.t("claude.params.missing_message")))?;
    let resp = host_call(
        host,
        "terminal.tell",
        json!({ "surface": surface_id, "text": message }),
    )?;

    // caller_surface 는 dynamic.rs 의 TASTY_SURFACE_ID 자동 채움으로 대개 채워진다.
    // 없으면(구버전 클라이언트 등) 어디로 알릴지 알 수 없으므로 알림 배선을
    // 생략한다(soft) — tell 자체의 성공/실패에는 영향 없음.
    if let Some(caller_surface) = params.get("caller_surface").and_then(|v| v.as_u64()) {
        register_notify_hooks(host, caller_surface as u32, surface_id, "tell");
    }

    Ok(resp)
}

#[cfg(test)]
use crate::notifications::*;
pub(crate) use crate::notifications::{
    handle_notify_done, handle_notify_error, register_notify_hooks,
};

/// 새 workspace 를 만들고 그 안에서 claude 를 기동한다. child 가 아니라 top-level
/// 이므로 호스트 child registry 에 등록하지 않는다(launch 는 05 범위 밖 특화 잔류).
/// error scanner 에 그 surface 를 등록한다.
pub(crate) fn handle_launch(
    scanner: &Arc<Mutex<ErrorScanner>>,
    host: &HostHandle,
    params: &Value,
    data_dir: Option<&Path>,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let workspace_name = params
        .get("workspace")
        .and_then(|v| v.as_str())
        .unwrap_or("claude")
        .to_string();
    let directory = params
        .get("directory")
        .and_then(|v| v.as_str())
        .map(String::from);
    let task = params
        .get("task")
        .and_then(|v| v.as_str())
        .map(String::from);
    let profile_file = resolve_profile_file_param(data_dir, params, tr)?;
    let permission_mode = resolve_permission_mode(host, params, profile_file.as_deref(), tr)?;

    // cwd 는 CLI 가 미리 absolute path 로 정규화 + 검증해 전달 (path_kind hint).
    // 호스트 workspace.create 가 직접 PTY 의 working_dir 로 사용 → `cd` echo trick 불필요.
    let mut ws_params = json!({
        "type": "terminal",
        "name": workspace_name,
    });
    if let Some(dir) = directory.as_deref() {
        ws_params["cwd"] = Value::String(dir.to_string());
    }
    let ws_resp = host
        .call("workspace.create", ws_params)
        .map_err(IpcMethodError::from)?;

    let workspace_id = ws_resp
        .get("id")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| IpcMethodError::new(tr.t("claude.launch.workspace_create_missing_id")))?;
    let surface_id = ws_resp
        .get("surface_id")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    if let Some(sid) = surface_id {
        let cmd = build_launch_command(
            task.as_deref(),
            profile_file.as_deref(),
            permission_mode.as_deref(),
        );
        if let Err(e) = host.call(
            "surface.send",
            json!({ "surface_id": sid, "text": format!("{cmd}\r") }),
        ) {
            tracing::warn!("surface.send (launch) failed: {e}");
        }

        crate::error_scan::lock_scanner(scanner).enable(sid, ScanTarget::TopLevel);
    }

    Ok(json!({
        "workspace_id": workspace_id,
        "workspace_name": workspace_name,
        "surface_id": surface_id,
    }))
}

/// `claude` / `claude --task <escaped>` / 뒤에 `--settings "<path>"` ·
/// `--permission-mode <mode>` 가 붙는 조합.
/// `profile_file` 은 CLI `path_kind = "file"` 정규화를 이미 거친 절대경로 — 인라인
/// JSON 이 아니라 파일 경로를 큰따옴표로 감싼다(`reboot::resume_command` 와 동일 규칙).
pub(crate) fn build_launch_command(
    task: Option<&str>,
    profile_file: Option<&str>,
    permission_mode: Option<&str>,
) -> String {
    let mut cmd = "claude".to_string();
    if let Some(t) = task {
        let escaped = shell_escape::escape(t.into());
        cmd.push_str(&format!(" --task {escaped}"));
    }
    if let Some(path) = profile_file {
        cmd.push_str(&format!(" --settings \"{path}\""));
    }
    cmd.push_str(&permission_mode_flag(permission_mode));
    cmd
}

/// 자식 surface 의 PTY 를 갈아끼우고 claude 를 재시작한다. registry 조작(PTY 교체/
/// Ctrl-C + metadata 갱신 + idle 초기화)은 호스트 `terminal.respawn` 으로 위임하고,
/// claude 특화 기동 명령만 그 위에 재전송한다. `child_index` → 호스트 `child` 매핑.
/// error scanner 에는 (재)등록 + dedupe 초기화한다 — surface_id 가 유지되므로 이전
/// 인스턴스가 남긴 dedupe 스니펫이 재기동 후 같은 에러를 억제할 수 있다.
pub(crate) fn handle_respawn(
    scanner: &Arc<Mutex<ErrorScanner>>,
    host: &HostHandle,
    params: &Value,
    data_dir: Option<&Path>,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let parent_surface_id = require_target_surface(params, tr)?;
    let child_index = require_child_index(params, tr)?;
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(String::from);
    let profile_file = resolve_profile_file_param(data_dir, params, tr)?;
    let permission_mode = resolve_permission_mode(host, params, profile_file.as_deref(), tr)?;

    // 1) 호스트 registry 위임(command 미전송): cwd 있으면 PTY 교체, 없으면 Ctrl-C.
    //    role/nickname/cwd 갱신 + idle 초기화까지 호스트가 수행.
    let mut rp = forward(params, &["cwd", "role", "nickname"]);
    put_target_surface(&mut rp, params, tr)?;
    rp.insert("child".into(), json!(child_index));
    let resp = host_call(host, "terminal.respawn", Value::Object(rp))?;
    let child_surface_id = resp
        .get("child_surface_id")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| {
            IpcMethodError::new(
                tr.t_fmt("claude.respawn.missing_child_surface_id", &resp.to_string()),
            )
        })?;

    // 2) claude 특화 기동 명령 재전송.
    start_claude_in_surface(
        host,
        child_surface_id,
        prompt.as_deref(),
        profile_file.as_deref(),
        permission_mode.as_deref(),
    );

    // 3) error scan 대상으로 (재)등록. PTY 가 갈렸으므로 이전 인스턴스의 dedupe
    //    스니펫은 버린다 — 안 버리면 재기동 후 같은 에러 텍스트가 다시 나도
    //    `claude-error` 가 억제된다.
    {
        let mut s = crate::error_scan::lock_scanner(scanner);
        s.enable(child_surface_id, ScanTarget::Child);
        s.reset_dedupe(child_surface_id);
    }

    Ok(json!({
        "child_surface_id": child_surface_id,
        "child_index": child_index,
        "parent_surface_id": parent_surface_id,
    }))
}

/// `claude.child_profile` 진입점 — 부모가 **자식**에게 지속 세션 프로필을 붙인다.
///
/// 부착 자체는 새로 만들지 않는다: `--child <index>` 를 자식 surface id 로 바꾼 뒤
/// [`crate::reboot::reboot_surface`] 에 그대로 넘긴다 — 자식이 스스로
/// `reboot --profile` 을 부른 것과 완전히 같은 경로(프로필 검증 → surface meta 부착
/// → Ctrl+C → `claude -r <sid> --settings`)를 탄다. 중복 가드(`inflight`)도 reboot
/// 과 같은 set 을 공유하므로 같은 자식에 두 명령이 겹치면 뒤엣것이 거부된다.
///
/// `reboot` 과 다른 점은 둘뿐이다:
/// - 대상이 자식이므로 **호출자(부모)의 턴은 잘리지 않는다** — reboot 문서의
///   "턴의 마지막 행동으로 호출하라" 제약은 여기 해당하지 않는다.
/// - `spawn`/`tell` 과 같은 완료 알림 hook 을 자동으로 건다 — 부모가 자식의
///   재기동 완료(idle/needs_input/exited)를 기다릴 수 있어야 하기 때문.
pub(crate) fn handle_child_profile(
    inflight: &Arc<Mutex<HashSet<u32>>>,
    host: &HostHandle,
    params: &Value,
    data_dir: Option<&Path>,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let parent_surface_id = require_target_surface(params, tr)?;
    // `--child` 는 항상 필수다 — 생략을 caller 자신으로 해석하면 `reboot --profile`
    // 과 창구가 겹친다.
    let child_index = require_child_index(params, tr)?;
    let child_surface_id = resolve_child_surface_id(host, parent_surface_id, child_index, tr)?;

    let resp = reboot_surface(inflight, host, child_surface_id, params, data_dir, tr)?;

    register_notify_hooks(host, parent_surface_id, child_surface_id, "child-profile");

    Ok(json!({
        "child_surface_id": child_surface_id,
        "child_index": child_index,
        "parent_surface_id": parent_surface_id,
        "session_id": resp.get("session_id").cloned().unwrap_or(Value::Null),
        "reboot_in_secs": resp.get("reboot_in_secs").cloned().unwrap_or(Value::Null),
    }))
}

/// 자식 surface 에서 claude 를 기동한다. surface_id 를 박은 inline env prefix 를
/// 항상 붙인다:
/// - `TASTY_SURFACE_ID={surface_id}` — 자식 셸이 `tasty claude hook` 을 발사할 때
///   자기 위치 식별 (없으면 hook 이 silent skip → idle/needs_input 미갱신).
/// - `TASTY_AGENT_ID=claude_s<surface_id>` — 관측/비용 agent 식별.
/// - `TASTY_SESSION_TOKEN=<hex>` — 신원 검증 토큰(발급 실패 시 생략).
pub(crate) fn start_claude_in_surface(
    host: &HostHandle,
    surface_id: u32,
    prompt: Option<&str>,
    profile_file: Option<&str>,
    permission_mode: Option<&str>,
) {
    let agent_id = format!("claude_s{surface_id}");
    let session_token = issue_session_token(host, &agent_id);
    let agent_prefix = match session_token {
        Some(tok) => format!(
            "TASTY_SURFACE_ID={surface_id} TASTY_AGENT_ID={agent_id} TASTY_SESSION_TOKEN={tok} "
        ),
        None => format!("TASTY_SURFACE_ID={surface_id} TASTY_AGENT_ID={agent_id} "),
    };
    let text = match prompt {
        Some(p) => claude_launch_command_with_prompt(
            surface_id,
            &agent_prefix,
            p,
            profile_file,
            permission_mode,
        ),
        None => {
            let settings_flag = match profile_file {
                Some(path) => format!(" --settings \"{path}\""),
                None => String::new(),
            };
            format!(
                "{agent_prefix}claude{settings_flag}{}\r",
                permission_mode_flag(permission_mode)
            )
        }
    };

    if let Err(e) = host.call(
        "surface.send",
        json!({ "surface_id": surface_id, "text": text }),
    ) {
        tracing::warn!("surface.send (claude) failed: {e}");
    }
}

/// prompt 임시파일 이름 prefix. 청소 스윕(`prompt_file::sweep_stale`)이 같은 패턴으로
/// 자기 파일만 매칭하도록 상수로 뽑는다. suffix·TTL·쓰기·스윕은
/// `tasty-plugin-agent-common` 이 갖고, **prefix 만** 여기 남는다 — codex plugin 이
/// 같은 surface_id 로 자기 prompt 파일을 같은 디렉터리에 쓰기 때문에 이름이 갈려야 한다.
const PROMPT_FILE_PREFIX: &str = "tasty-prompt-";
/// prompt 를 임시 파일에 쓰고 `$(cat ...)` 로 주입하는 claude 기동 명령을 만든다.
/// 파일 쓰기 실패는 warn 후에도 계속 진행한다(빈 프롬프트로라도 기동은 시도).
/// `profile_file` 이 있으면 positional prompt 인자보다 앞에 `--settings "<path>"` 를
/// 붙인다. 이 함수는 이미 POSIX 전용 env prefix 를 쓰고 있어(`agent_prefix`) 그
/// 플랫폼 정합은 이 변경의 범위 밖 — 기존 상태를 그대로 따른다.
///
/// 파일 정리 시점: 자식이 `$(cat ...)` 로 이 파일을 다 읽은 순간을 tasty 가 알
/// 방법이 없다(`surface.send` 는 fire-and-forget 텍스트 주입) — 쓰자마자 지우면
/// 아직 안 읽은 자식과 레이스한다. 대신 매 spawn 마다 TTL 을 넘긴 이전 파일들을
/// 먼저 청소한다(`prompt_file::sweep_stale`) — 지연 삭제.
/// 권한은 생성 시점부터 0600(owner-only, Unix) 으로 좁힌다 — 생성 후 별도
/// `chmod` 로 좁히면 그 사이 기본 권한(보통 0644)으로 잠깐 노출되는 TOCTOU 창이
/// 생기므로, `OpenOptions`(Unix `mode`)로 처음부터 좁게 만든다.
fn claude_launch_command_with_prompt(
    surface_id: u32,
    agent_prefix: &str,
    prompt: &str,
    profile_file: Option<&str>,
    permission_mode: Option<&str>,
) -> String {
    let temp_dir = std::env::temp_dir();
    prompt_file::sweep_stale(&temp_dir, PROMPT_FILE_PREFIX);
    let prompt_path = prompt_file::path_for(&temp_dir, PROMPT_FILE_PREFIX, surface_id);
    if let Err(e) = prompt_file::write(&prompt_path, prompt) {
        tracing::warn!("Failed to write prompt file: {e}");
    }
    let settings_flag = match profile_file {
        Some(path) => format!("--settings \"{path}\" "),
        None => String::new(),
    };
    let mode_flag = match permission_mode {
        Some(m) => format!("--permission-mode {m} "),
        None => String::new(),
    };
    format!(
        "{agent_prefix}claude {settings_flag}{mode_flag}\"$(cat '{}')\"\r",
        prompt_path.display()
    )
}

/// 자식 Claude 에 발급할 SessionToken 을 호스트에서 가져온다. 부모(claude plugin)의
/// 권한 부분집합만 발급되며, 발급 실패는 치명적이지 않으므로 `Option` 반환.
///
/// `ipc.invoke:claude` 는 자식이 이 plugin 으로 돌아오는 호출(`tasty claude hook` 완료
/// 알림, 손자 spawn·tell)의 자격이다 — 호스트는 권한 셋을 가진 caller 가 plugin
/// namespace 를 부를 때 그 토큰을 요구하고, 자기 namespace 토큰은 소유 plugin 이 쥐지
/// 않고도 넘길 수 있다. `ipc.invoke:codex` 는 자식이 Codex 교차 검증을 띄우는 자격이라
/// 매니페스트에 선언해 쥔 것을 넘긴다(docs/adr/0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md).
pub(crate) fn issue_session_token(host: &HostHandle, agent_id: &str) -> Option<String> {
    let resp = match host.call(
        "session.issue",
        json!({
            "agent_id": agent_id,
            "permissions": [
                "surface.read",
                "surface.write",
                "terminal.write",
                "terminal.read",
                "notification",
                "telemetry",
                "agent",
                "ipc.invoke:claude",
                "ipc.invoke:codex",
            ],
        }),
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("session.issue failed for child {agent_id}: {e}");
            return None;
        }
    };
    let token = resp.get("token").and_then(|v| v.as_str()).map(String::from);
    if token.is_none() {
        tracing::warn!("session.issue returned no 'token' field for child {agent_id}");
    }
    token
}

/// 자식 Claude 를 spawn 한다 — 호스트 `terminal.spawn` 으로 registry 등록 + soft
/// 점유 + tab 생성(command 미전송)한 뒤, 반환된 surface_id 에 claude 특화 기동
/// 명령을 전송한다(2단계 spawn). 호스트가 index/tab-name/pane/occupancy 를 소유.
/// 자식 surface 는 error scanner 대상으로 등록한다 — 사람이 보고 있지 않은 자식이야말로
/// 네트워크/API 에러 감지가 가장 필요한 대상이다.
pub(crate) fn handle_spawn(
    scanner: &Arc<Mutex<ErrorScanner>>,
    host: &HostHandle,
    params: &Value,
    data_dir: Option<&Path>,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let parent_surface_id = require_target_surface(params, tr)?;
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(String::from);
    let profile_file = resolve_profile_file_param(data_dir, params, tr)?;
    let permission_mode = resolve_permission_mode(host, params, profile_file.as_deref(), tr)?;

    // 1) 호스트 registry 에 등록 + 점유 + tab 생성. workspace required.
    let mut sp = forward(params, &["workspace", "pane", "cwd", "role", "nickname"]);
    sp.insert("parent".into(), json!(parent_surface_id));
    let resp = host_call(host, "terminal.spawn", Value::Object(sp))?;
    let child_surface_id = resp
        .get("child_surface_id")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| {
            IpcMethodError::new(
                tr.t_fmt("claude.spawn.missing_child_surface_id", &resp.to_string()),
            )
        })?;

    // 2) claude 특화 기동 명령 전송(session token + surface_id inline env 필요).
    start_claude_in_surface(
        host,
        child_surface_id,
        prompt.as_deref(),
        profile_file.as_deref(),
        permission_mode.as_deref(),
    );

    // 2-1) error scan 대상 등록. `ScanTarget::Child` 로 넣으면 폴링 루프가
    //      `terminal.parent` 로 관계 생존을 대조하므로, kill/close 뿐 아니라
    //      `terminal.release`(surface 는 남기고 관계만 해제)까지 함께 정리된다.
    crate::error_scan::lock_scanner(scanner).enable(child_surface_id, ScanTarget::Child);

    // claude CLI/auto_wait 는 응답에 parent_surface_id 를 기대한다(호스트 응답엔
    // 없으므로 caller surface 로 채운다). 나머지 필드(child_surface_id/child_index/
    // pane_id/workspace_id)는 호스트 응답 그대로.
    let mut out = resp;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("parent_surface_id".into(), json!(parent_surface_id));
    }

    // 3) child 개수 임계치 경고(soft) — spawn 자체를 막지 않는다.
    if let Some(warning) = compute_spawn_warning(host, parent_surface_id, tr) {
        if let Some(obj) = out.as_object_mut() {
            obj.insert("warning".into(), json!(warning));
        }
    }

    // 4) caller(parent_surface_id)에게 완료(idle/needs_input/exited) 1회성 알림 배선.
    register_notify_hooks(host, parent_surface_id, child_surface_id, "spawn");

    Ok(out)
}

/// spawn 직후 parent 의 현재 child 목록/상태를 재조회해 임계치 초과 여부를 판단한다.
/// host 호출 실패는 경고 생략으로 처리한다(soft 경고이므로 spawn 성공을 막지 않음).
///
/// 여기서 부르는 건 claude 특화 remap 된 `claude.children`(필드명 `child_surface_id`)이
/// 아니라 **원본** `terminal.children`(필드명 `surface_id`) — `index`/`state` 필드명은
/// 양쪽 shape 모두 동일하므로 아래 파싱 코드는 원본 응답에 그대로 맞는다.
/// 자식 인구는 [`tasty_plugin_agent_common::children::spawn_census`] 가 센다 —
/// 그 판정(확정 stale 만 센다 · 못 읽으면 `None`)이 짝의 두 crate 에 주석까지
/// 글자 그대로 두 벌 있었다. 여기 남는 것은 **문구 조립**뿐이다: 카탈로그
/// namespace 와 기존 placeholder 형식을 보존해 공개 카탈로그 호환성을 유지한다.
fn compute_spawn_warning(
    host: &HostHandle,
    parent_surface_id: u32,
    tr: &Translator,
) -> Option<String> {
    let c = spawn_census(host, parent_surface_id)?;
    build_spawn_warning(tr, c.total, &c.idle, &c.stale, c.threshold)
}

/// host 호출은 `tr`(순수 조회) 뿐 — 단위 테스트 대상.
///
/// 재사용 후보를 **두 목록으로 나눈다.** 둘 다 respawn 대상이지만 근거가 다르다:
/// `idle` 은 자식이 hook 으로 완료를 직접 보고한 값이고, 확정 `stale` 은 보고가 오지
/// 않은 채 호스트 관측이 "전경이 셸로 돌아왔다" 를 잡아낸 값이다(hook 유실 —
/// ADR-0072 가 겨냥한 시나리오). 후자에 "이미 작업을 끝냈다" 는 문구를 쓰면 자식이
/// 그렇게 보고한 적 없는데 보고한 것처럼 읽히므로 문구를 분리한다.
fn build_spawn_warning(
    tr: &Translator,
    total: usize,
    idle_indices: &[u64],
    stale_indices: &[u64],
    threshold: f64,
) -> Option<String> {
    if (total as f64) <= threshold {
        return None;
    }
    let mut msg = tr
        .t("claude.spawn.warning_threshold")
        .replacen("{}", &total.to_string(), 1)
        .replacen("{}", &threshold.to_string(), 1);
    if !idle_indices.is_empty() {
        msg.push_str(&tr.t_fmt(
            "claude.spawn.warning_idle_children",
            &join_indices(idle_indices),
        ));
    }
    if !stale_indices.is_empty() {
        msg.push_str(&tr.t_fmt(
            "claude.spawn.warning_stale_children",
            &join_indices(stale_indices),
        ));
    }
    Some(msg)
}

/// 자식 surface 의 parent 를 조회 — 호스트 `terminal.parent` 로 위임.
pub(crate) fn handle_parent(
    host: &HostHandle,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let surface = require_target_surface(params, tr)?;
    host_call(host, "terminal.parent", json!({ "surface": surface }))
}

/// 자식 surface 단건 상태 조회 — 호스트 `terminal.state` 로 위임.
/// `claude` namespace 안에 두는 이유는 완료 판정 전략의 `poll_method` 가 owner
/// namespace 밖을 참조할 수 없어서다(결정 2) — `claude.spawn` 기본 전략이 이
/// 메서드를 poll_method 로 참조한다(매니페스트 `[[contributes.completion_strategy]]`).
pub(crate) fn handle_state(
    host: &HostHandle,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let surface = require_target_surface(params, tr)?;
    host_call(host, "terminal.state", json!({ "surface": surface }))
}

#[cfg(test)]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다(전수 가드가 제외한다) —
// 여기 경고는 조치 대상이 될 수 없어 프로덕션 신호만 가린다. error-handling.md.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    /// 대상 surface 를 **호스트로 넘기는 params 에 실제로 싣는다.**
    ///
    /// 이 판정이 없어서 났던 일: `claude.kill` / `claude.respawn` 은 `surface_id`
    /// 를 읽어놓고 호스트에는 `surface` 키만 pass-through 했다. 두 이름이 갈린
    /// 자리라 `surface_id` 만 실은 호출은 **아무 대상도 안 실은 호출**이 됐고,
    /// 호스트는 유일-parent 폴백으로 답했다 — 존재하지 않는 surface 를 지목한
    /// 호출이 남의 자식을 죽이고 성공을 돌려줬다.
    ///
    /// 그래서 이 테스트는 "에러가 안 난다" 가 아니라 **실린 값**을 본다.
    #[test]
    fn the_target_surface_reaches_the_host_under_either_name() {
        let tr = test_translator();
        for params in [json!({ "surface": 7 }), json!({ "surface_id": 7 })] {
            let mut out = serde_json::Map::new();
            put_target_surface(&mut out, &params, &tr).expect("두 이름 다 받는다");
            assert_eq!(
                out.get("surface"),
                Some(&json!(7)),
                "{params} 에서 대상이 호스트로 안 실렸다 — 유일-parent 폴백에 떨어진다"
            );
        }
    }

    /// 아무 이름도 안 주면 **아무것도 안 싣는다** — 호스트의 유일-parent 폴백이
    /// 곧 CLI 의 "`--surface` 생략" 동작이므로, 여기서 값을 지어내면 그 동작이
    /// 사라진다.
    #[test]
    fn no_target_named_stays_no_target_sent() {
        let tr = test_translator();
        let mut out = serde_json::Map::new();
        put_target_surface(&mut out, &json!({ "child_index": 0 }), &tr).expect("없어도 성공");
        assert!(
            out.is_empty(),
            "대상을 안 준 호출에 값을 지어냈다 — 폴백이 사라진다: {out:?}"
        );
    }

    /// 두 이름이 **다른 값**이면 고르지 않고 거절한다. 어느 쪽을 골라도 절반의
    /// 호출자에게는 지목하지 않은 대상이 된다.
    #[test]
    fn two_names_with_different_values_are_refused_not_picked() {
        let tr = test_translator();
        let e = optional_target_surface(&json!({ "surface": 1, "surface_id": 2 }), &tr)
            .expect_err("서로 다른 두 대상을 조용히 하나로 고르면 안 된다");
        let msg = format!("{e:?}");
        assert!(
            msg.contains('1') && msg.contains('2'),
            "어느 두 값이 부딪혔는지 안 알려준다: {msg}"
        );
        // 같은 값이면 부딪힌 것이 아니다.
        assert_eq!(
            optional_target_surface(&json!({ "surface": 3, "surface_id": 3 }), &tr).unwrap(),
            Some(3),
            "CLI 가 두 키를 같은 값으로 채워 보내는 형태를 막으면 안 된다"
        );
    }

    /// `require_target_surface` / `require_child_index` 의 **네 갈래**를 픽스처로 못박는다.
    /// 실재하는 surface id 를 쓰지 않는다 — 그 id 가 사라지면 회귀가 뜻을 잃는다.
    #[test]
    fn required_u32_params_separate_absent_from_malformed_and_refuse_to_truncate() {
        let tr = test_translator();

        // ① 키 없음.
        assert!(require_target_surface(&json!({}), &tr).is_err());
        assert!(require_child_index(&json!({}), &tr).is_err());

        // ② 정상 — 경계값이 그대로 통과한다.
        assert_eq!(
            require_target_surface(&json!({ "surface_id": 0 }), &tr).unwrap(),
            0
        );
        assert_eq!(
            require_target_surface(&json!({ "surface_id": u32::MAX }), &tr).unwrap(),
            u32::MAX
        );

        // ③ 숫자가 아니다 — 거부하고, "missing" 이라고 답하지 않는다.
        let e = require_target_surface(&json!({ "surface_id": "conductor" }), &tr).unwrap_err();
        let m = format!("{e:?}");
        assert!(m.contains("32 bits"), "{m}");
        assert!(!m.contains("Missing"), "값이 왔는데 없다고 답한다: {m}");

        // ④ ★ 범위 초과 — 자르면 다른 surface 가 된다(`u32::MAX + 2` → 1).
        for over in [
            u64::from(u32::MAX) + 1,
            u64::from(u32::MAX) + 2,
            5_000_000_000,
        ] {
            assert!(
                require_target_surface(&json!({ "surface_id": over }), &tr).is_err(),
                "{over} 가 안 걸린다"
            );
            assert!(require_child_index(&json!({ "child_index": over }), &tr).is_err());
        }

        assert!(require_target_surface(&json!({ "surface_id": -1 }), &tr).is_err());
    }

    /// `null` 슬롯은 **안 왔다**로 읽는다 — 직렬화가 빈 슬롯을 `null` 로 채우는 경우가
    /// 있어, 오타로 취급하면 정상 경로가 막힌다.
    #[test]
    fn a_null_slot_reads_as_absent_not_as_a_malformed_value() {
        let tr = test_translator();
        let e = require_target_surface(&json!({ "surface_id": Value::Null }), &tr).unwrap_err();
        assert!(format!("{e:?}").contains("Missing"), "{e:?}");
    }

    /// 실제 crate `lang/` 을 로드한 `Translator` — 하드코딩 영문 assertion 을
    /// lang 파일 드리프트로부터 고정한다(`checklist.rs` 의 SENTINEL 핀 테스트와
    /// 동일 패턴).
    fn test_translator() -> Translator {
        let lang_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang");
        Translator::load(&lang_dir, "en")
    }

    fn test_translator_for(code: &str) -> Translator {
        let lang_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang");
        Translator::load(&lang_dir, code)
    }

    /// 오형식 대상 surface 는 **어느 키가 틀렸는지** 댄다.
    ///
    /// 이 판정은 `surface` 와 `surface_id` **두 이름을 한 필드로** 읽는다. 그래서
    /// 틀린 키를 안 대면 호출자는 자기가 보낸 둘 중 무엇을 고쳐야 하는지 모른다 —
    /// 한동안 이쪽이 `Malformed { raw, .. }` 로 키를 버려서 정확히 그랬고, 짝
    /// plugin(codex)은 처음부터 댔다. 같은 이름의 시험이 그쪽에도 있다: 두 사본이
    /// **정보량**을 함께 고정한다(문구·placeholder 형태는 여전히 crate 마다 다르고,
    /// 그 축은 `tasty-plugin-agent-common` 의 crate doc 이 호환성을 위해 보존하는 것이다).
    ///
    /// 이 판정을 거치는 자리는 **넷**이다. 나머지 둘은 훅 경로다: 이쪽의
    /// `hook.rs::resolve_surface_id_from` 은 env 폴백이 더 붙어 있고, codex 쪽의
    /// `handlers.rs::handle_hook` 은 한동안 세 갈래를 전부 `--surface 를 대라` 한
    /// 문장으로 덮어 이 축을 깨고 있었다(그래서 위 "짝 plugin 은 처음부터" 는
    /// **판정부 자리에 한한 말**이다). 넷 다 자기 시험으로 같은 축을 따로 고정한다.
    ///
    /// 로케일 셋을 다 본다 — 키 이름은 번역 대상이 아니라 **파라미터 이름**이라
    /// 세 카탈로그에서 똑같이 나와야 하고, 한 언어만 보면 다른 언어에서 문구를
    /// 손보다 키를 흘려도 안 잡힌다.
    #[test]
    fn a_malformed_target_surface_names_which_of_the_two_keys_was_wrong() {
        for locale in ["en", "ko", "ja"] {
            let tr = test_translator_for(locale);
            let by_surface = optional_target_surface(&json!({ "surface": "x" }), &tr)
                .expect_err("문자열은 surface id 가 아니다")
                .message;
            let by_surface_id = optional_target_surface(&json!({ "surface_id": "x" }), &tr)
                .expect_err("문자열은 surface id 가 아니다")
                .message;
            assert!(
                by_surface_id.contains("surface_id"),
                "{locale}: 'surface_id' 가 틀렸는데 그 이름을 안 댄다 — {by_surface_id}"
            );
            assert!(
                !by_surface.contains("surface_id"),
                "{locale}: 'surface' 가 틀렸는데 'surface_id' 를 댄다 — {by_surface}"
            );
            assert_ne!(
                by_surface, by_surface_id,
                "{locale}: 두 키가 같은 문구를 받는다 — 어느 쪽이 틀렸는지 못 가른다"
            );
        }
    }

    #[test]
    fn build_spawn_warning_none_below_threshold() {
        let tr = test_translator();
        assert_eq!(build_spawn_warning(&tr, 3, &[], &[], 6.0), None);
    }

    #[test]
    fn build_spawn_warning_above_threshold_lists_idle_and_mentions_respawn() {
        let tr = test_translator();
        let w = build_spawn_warning(&tr, 7, &[2, 5], &[], 6.0).unwrap();
        assert!(w.contains("respawn"));
        assert!(w.contains('2') && w.contains('5'));
    }

    #[test]
    fn build_spawn_warning_above_threshold_no_idle_has_no_respawn_word() {
        let tr = test_translator();
        let w = build_spawn_warning(&tr, 7, &[], &[], 6.0).unwrap();
        assert!(!w.contains("respawn"));
    }

    #[test]
    fn build_spawn_warning_respects_custom_threshold() {
        let tr = test_translator();
        assert_eq!(build_spawn_warning(&tr, 3, &[], &[], 6.0), None);
        assert!(build_spawn_warning(&tr, 4, &[], &[], 3.0).is_some());
    }

    /// 확정 stale 자식만 있어도 respawn 을 권해야 한다 — hook 유실로 idle 보고가
    /// 영영 오지 않는 자식이 정확히 이 경우다(ADR-0072 가 겨냥한 시나리오).
    #[test]
    fn build_spawn_warning_lists_stale_children_as_respawn_candidates() {
        let tr = test_translator();
        let w = build_spawn_warning(&tr, 7, &[], &[3], 6.0).unwrap();
        assert!(w.contains("respawn"), "{w}");
        assert!(w.contains('3'), "{w}");
    }

    /// stale 문구는 idle 문구와 분리된다 — 보고받지 않은 자식에 "이미 끝냈다" 는
    /// 문구를 쓰면 자식이 그렇게 보고한 것처럼 읽힌다.
    #[test]
    fn build_spawn_warning_separates_stale_wording_from_idle() {
        let tr = test_translator();
        let idle_only = build_spawn_warning(&tr, 7, &[2], &[], 6.0).unwrap();
        let stale_only = build_spawn_warning(&tr, 7, &[], &[3], 6.0).unwrap();
        assert!(idle_only.contains("Idle children"), "{idle_only}");
        assert!(!stale_only.contains("Idle children"), "{stale_only}");
        assert!(
            stale_only.contains("never reported completion"),
            "{stale_only}"
        );

        let both = build_spawn_warning(&tr, 7, &[2], &[3], 6.0).unwrap();
        assert!(both.contains("Idle children"), "{both}");
        assert!(both.contains("never reported completion"), "{both}");
    }

    #[test]
    fn build_launch_command_no_task() {
        assert_eq!(build_launch_command(None, None, None), "claude");
    }

    #[test]
    fn build_launch_command_with_simple_task() {
        assert_eq!(
            build_launch_command(Some("fix"), None, None),
            "claude --task fix"
        );
    }

    #[test]
    fn build_launch_command_with_spaces_gets_escaped() {
        let out = build_launch_command(Some("fix the bug"), None, None);
        assert!(out.starts_with("claude --task "), "prefix wrong: {out}");
        assert!(out.contains("fix the bug"), "task body missing: {out}");
        assert_ne!(out, "claude --task fix the bug", "must be escaped");
    }

    #[test]
    fn build_launch_command_with_profile_appends_quoted_settings_path() {
        assert_eq!(
            build_launch_command(None, Some("/home/user/profile.json"), None),
            "claude --settings \"/home/user/profile.json\""
        );
    }

    #[test]
    fn build_launch_command_with_task_and_profile_appends_both() {
        assert_eq!(
            build_launch_command(Some("fix"), Some("/home/user/profile.json"), None),
            "claude --task fix --settings \"/home/user/profile.json\""
        );
    }

    /// `settings.get_plugin_setting` 하나만 답하는 최소 host — 승인 정책 해석은
    /// 그 한 물음 말고는 host 를 안 부른다. `MockHost` 는 hook 사이클을 흉내 내는
    /// 물건이라 이 축을 표현할 자리가 없다.
    struct SettingHost(Option<&'static str>);

    impl HostCall for SettingHost {
        fn call(
            &self,
            method: &str,
            _params: Value,
        ) -> Result<Value, tasty_plugin_sdk::PluginError> {
            match (method, self.0) {
                ("settings.get_plugin_setting", Some(v)) => Ok(json!({ "value": v })),
                ("settings.get_plugin_setting", None) => Ok(json!({})),
                _ => Ok(json!({})),
            }
        }
    }

    #[test]
    fn build_launch_command_with_permission_mode_appends_flag() {
        assert_eq!(
            build_launch_command(None, None, Some("acceptEdits")),
            "claude --permission-mode acceptEdits"
        );
    }

    /// 두 플래그가 함께 오면 순서가 고정된다 — `--settings` 먼저, 정책이 뒤.
    #[test]
    fn build_launch_command_with_profile_and_permission_mode_fixes_order() {
        assert_eq!(
            build_launch_command(None, Some("/home/u/p.json"), Some("plan")),
            "claude --settings \"/home/u/p.json\" --permission-mode plan"
        );
    }

    #[test]
    fn claude_launch_command_with_prompt_puts_mode_before_the_positional_prompt() {
        let out = claude_launch_command_with_prompt(
            3,
            "TASTY_SURFACE_ID=3 ",
            "hello",
            None,
            Some("dontAsk"),
        );
        assert!(
            out.starts_with("TASTY_SURFACE_ID=3 claude --permission-mode dontAsk \"$(cat '"),
            "got {out}"
        );
    }

    #[test]
    fn resolve_permission_mode_defaults_to_no_flag() {
        // ADR-0265 결정 2 — params 도 설정도 없으면 플래그를 안 붙인다(사용자 자신의
        // Claude Code 설정이 그대로 정한다). codex 와 달리 비대화형 값으로 떨어뜨리지
        // 않는다.
        let host = SettingHost(None);
        assert_eq!(
            resolve_permission_mode(&host, &json!({}), None, &test_translator()).unwrap(),
            None
        );
    }

    #[test]
    fn resolve_permission_mode_takes_the_explicit_param() {
        let host = SettingHost(None);
        assert_eq!(
            resolve_permission_mode(
                &host,
                &json!({ "permission_mode": "plan" }),
                None,
                &test_translator()
            )
            .unwrap(),
            Some("plan".to_string())
        );
    }

    #[test]
    fn resolve_permission_mode_falls_back_to_the_plugin_setting() {
        let host = SettingHost(Some("acceptEdits"));
        assert_eq!(
            resolve_permission_mode(&host, &json!({}), None, &test_translator()).unwrap(),
            Some("acceptEdits".to_string())
        );
    }

    /// 호출별 params 가 설정 기본값을 이 호출에 한해 덮는다(ADR-0265 결정 4).
    #[test]
    fn explicit_param_overrides_the_plugin_setting() {
        let host = SettingHost(Some("acceptEdits"));
        assert_eq!(
            resolve_permission_mode(
                &host,
                &json!({ "permission_mode": "plan" }),
                None,
                &test_translator()
            )
            .unwrap(),
            Some("plan".to_string())
        );
    }

    /// `inherit` 은 "플래그를 안 붙인다" 와 같은 뜻이다.
    #[test]
    fn inherit_setting_means_no_flag() {
        let host = SettingHost(Some("inherit"));
        assert_eq!(
            resolve_permission_mode(&host, &json!({}), None, &test_translator()).unwrap(),
            None
        );
    }

    /// 설정에 모르는 값이 들어 있어도 기동을 막지 않는다 — 무시하고 미부착.
    #[test]
    fn unknown_setting_value_is_ignored_not_fatal() {
        let host = SettingHost(Some("yolo"));
        assert_eq!(
            resolve_permission_mode(&host, &json!({}), None, &test_translator()).unwrap(),
            None
        );
    }

    #[test]
    fn resolve_permission_mode_rejects_an_unknown_value() {
        let host = SettingHost(None);
        let err = resolve_permission_mode(
            &host,
            &json!({ "permission_mode": "yolo" }),
            None,
            &test_translator(),
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("yolo"), "got {err:?}");
    }

    #[test]
    fn resolve_permission_mode_rejects_a_profile_that_sets_the_same_axis() {
        // ADR-0265 결정 5 — 어느 쪽이 이기는지 조용히 정하지 않는다.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        std::fs::write(&path, r#"{"permissions":{"defaultMode":"plan"}}"#).unwrap();
        let host = SettingHost(None);
        let err = resolve_permission_mode(
            &host,
            &json!({ "permission_mode": "acceptEdits" }),
            path.to_str(),
            &test_translator(),
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("defaultMode"), "got {err:?}");
    }

    /// `allow`/`deny` 는 규칙 목록이지 모드가 아니다 — 같은 축이 아니므로 공존한다.
    #[test]
    fn resolve_permission_mode_allows_a_profile_that_only_lists_rules() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        std::fs::write(&path, r#"{"permissions":{"allow":["Bash(ls:*)"]}}"#).unwrap();
        let host = SettingHost(None);
        assert_eq!(
            resolve_permission_mode(
                &host,
                &json!({ "permission_mode": "acceptEdits" }),
                path.to_str(),
                &test_translator()
            )
            .unwrap(),
            Some("acceptEdits".to_string())
        );
    }

    #[test]
    fn claude_launch_command_with_prompt_no_profile_unchanged() {
        let out = claude_launch_command_with_prompt(1, "TASTY_SURFACE_ID=1 ", "hello", None, None);
        assert!(
            out.starts_with("TASTY_SURFACE_ID=1 claude \"$(cat '"),
            "got {out}"
        );
        assert!(!out.contains("--settings"), "got {out}");
    }

    #[test]
    fn claude_launch_command_with_prompt_and_profile_prepends_settings() {
        let out = claude_launch_command_with_prompt(
            2,
            "TASTY_SURFACE_ID=2 ",
            "hello",
            Some("/home/user/profile.json"),
            None,
        );
        assert!(
            out.starts_with(
                "TASTY_SURFACE_ID=2 claude --settings \"/home/user/profile.json\" \"$(cat '"
            ),
            "got {out}"
        );
    }

    // ── 완료 알림 문구 — "spawn 완료" 오독 방지 ──
    // 원래 이 회귀 가드가 겨냥한 한국어 문구는 `lang/ko.toml` 로 옮겨졌으므로,
    // 그 locale 을 실제로 로드해 동일 속성을 검증한다(하드코딩 복제 대신 lang
    // 파일을 SoT 로 삼는다 — `checklist.rs` SENTINEL 핀 테스트와 동일 이유).
    fn test_translator_ko() -> Translator {
        let lang_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang");
        Translator::load(&lang_dir, "ko")
    }

    #[test]
    fn notify_done_message_leads_with_work_completion() {
        let tr = test_translator_ko();
        let msg = notify_done_message(&tr, "spawn", 42);
        assert!(
            msg.contains("작업 완료"),
            "완료 대상이 '작업'임이 드러나야 함: {msg}"
        );
        assert!(msg.contains("42"), "target surface 번호 누락: {msg}");
        assert!(msg.contains("spawn"), "호출 방식 정보 누락: {msg}");
    }

    #[test]
    fn notify_done_message_does_not_read_as_command_itself_completing() {
        // 회귀 방지: 과거 "{command_name} 완료: surface N" 형태는 "spawn 이라는 동작이
        // 완료됐다"로 오독되기 쉬웠다 — command_name 이 더 이상 완료의 주어로 문장
        // 맨 앞에 오지 않아야 한다.
        let tr = test_translator_ko();
        for command_name in ["spawn", "tell"] {
            let msg = notify_done_message(&tr, command_name, 7);
            assert!(
                !msg.starts_with(&format!("{command_name} 완료")),
                "옛 오독 유발 포맷으로 회귀함: {msg}"
            );
        }
    }

    #[test]
    fn stop_failure_hint_is_appended_after_the_completion_text() {
        let tr = test_translator_ko();
        let base = notify_done_message(&tr, "spawn", 42);
        let msg =
            crate::notifications::with_stop_failure_hint(&tr, base.clone(), Some("overloaded"));
        assert!(
            msg.starts_with(&base),
            "완료 문구 앞부분은 그대로여야 함: {msg}"
        );
        assert!(msg.contains("overloaded"), "에러 종류 누락: {msg}");
        assert!(
            msg.contains("API 에러"),
            "에러로 끝났음이 드러나야 함: {msg}"
        );
    }

    #[test]
    fn no_stop_failure_record_leaves_the_completion_text_unchanged() {
        let tr = test_translator();
        let base = notify_done_message(&tr, "tell", 7);
        for error in [None, Some("")] {
            assert_eq!(
                crate::notifications::with_stop_failure_hint(&tr, base.clone(), error),
                base
            );
        }
    }

    #[test]
    fn require_child_index_missing_is_invalid_params() {
        let tr = test_translator();
        let err = require_child_index(&json!({ "surface_id": 1 }), &tr).unwrap_err();
        assert_eq!(err.code, -32602);
    }

    // ── 형제 once-hook 정리 재현 (docs/plugins/claude/index.md 의 notify-done 형제 hook 정리 절 참조) ──

    use std::cell::RefCell;

    struct MockHook {
        id: u64,
        surface_id: u32,
        command: String,
        event: String,
        /// once=false 인 상시 hook 은 발화해도 남는다(에러 정지 알림 hook).
        once: bool,
    }

    /// hook.set/list/unset + terminal.tell 을 in-memory 로 시뮬레이션하는 mock 호스트.
    /// `fire` 로 특정 surface 의 특정 event once-hook 을 실제 host 처럼 제거(once)한다.
    /// `alive` 는 `surface.locate` 응답을 시뮬레이션 — 기본은 아무도 살아있지 않은
    /// 것으로 취급하고(= surface.locate 조회 실패와 동일하게 안전 쪽으로 fallback),
    /// `mark_alive`/`mark_dead` 로 명시적으로 상태를 세팅한다.
    struct MockHost {
        hooks: RefCell<Vec<MockHook>>,
        next_id: RefCell<u64>,
        alive: RefCell<std::collections::HashSet<u32>>,
        /// `terminal.children` 응답의 `children` 배열. 호스트 원본 스키마 그대로
        /// `surface_id` 필드를 쓴다 — claude 특화 remap(`child_surface_id`)은
        /// `handle_children` 이 나중에 얹는 것이라 여기 있으면 안 된다.
        children: RefCell<Vec<Value>>,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                hooks: RefCell::new(Vec::new()),
                next_id: RefCell::new(1),
                alive: RefCell::new(std::collections::HashSet::new()),
                children: RefCell::new(Vec::new()),
            }
        }

        fn set_children(&self, children: Vec<Value>) {
            *self.children.borrow_mut() = children;
        }

        /// event 발화 시뮬레이션 — 매칭 once-hook 제거(호스트 `check_and_fire` 의
        /// retain 과 동일). 상시 hook(once=false)은 남긴다. 발화한 hook 개수를 반환.
        fn fire(&self, surface_id: u32, event: &str) -> usize {
            let mut hooks = self.hooks.borrow_mut();
            let fired = hooks
                .iter()
                .filter(|h| h.surface_id == surface_id && h.event == event)
                .count();
            hooks.retain(|h| !(h.surface_id == surface_id && h.event == event && h.once));
            fired
        }

        /// 완료 알림(notify-done) 그룹의 command 만 — 에러 정지 알림 hook 은 별개
        /// 수명이라 형제 사이클 assertion 에서 제외한다.
        fn done_commands_on(&self, surface_id: u32) -> Vec<String> {
            self.commands_on(surface_id)
                .into_iter()
                .filter(|c| c.contains("notify-done"))
                .collect()
        }

        /// 특정 event 로 등록된 hook 개수.
        fn hooks_for_event(&self, surface_id: u32, event: &str) -> usize {
            self.hooks
                .borrow()
                .iter()
                .filter(|h| h.surface_id == surface_id && h.event == event)
                .count()
        }

        fn commands_on(&self, surface_id: u32) -> Vec<String> {
            self.hooks
                .borrow()
                .iter()
                .filter(|h| h.surface_id == surface_id)
                .map(|h| h.command.clone())
                .collect()
        }

        /// `surface.locate` 가 `exists: true` 를 돌려주도록(= 아직 process 가 살아있음).
        fn mark_alive(&self, surface_id: u32) {
            self.alive.borrow_mut().insert(surface_id);
        }

        /// `surface.locate` 가 `exists: false` 를 돌려주도록(= process-exit 로 host 가
        /// 이미 surface 를 닫아버림을 재현).
        fn mark_dead(&self, surface_id: u32) {
            self.alive.borrow_mut().remove(&surface_id);
        }
    }

    impl HostCall for MockHost {
        fn call(
            &self,
            method: &str,
            params: Value,
        ) -> Result<Value, tasty_plugin_sdk::PluginError> {
            match method {
                "hook.set" => {
                    let mut id = self.next_id.borrow_mut();
                    let hid = *id;
                    *id += 1;
                    self.hooks.borrow_mut().push(MockHook {
                        id: hid,
                        surface_id: params["surface_id"].as_u64().unwrap() as u32,
                        command: params["command"].as_str().unwrap().to_string(),
                        event: params["event"].as_str().unwrap().to_string(),
                        once: params["once"].as_bool().unwrap_or(false),
                    });
                    Ok(json!({ "hook_id": hid }))
                }
                "hook.list" => {
                    let sid = params["surface_id"].as_u64().map(|v| v as u32);
                    let arr: Vec<Value> = self
                        .hooks
                        .borrow()
                        .iter()
                        .filter(|h| sid.is_none_or(|s| h.surface_id == s))
                        .map(|h| {
                            json!({ "id": h.id, "surface_id": h.surface_id, "command": h.command, "event": h.event })
                        })
                        .collect();
                    Ok(json!(arr))
                }
                "hook.unset" => {
                    let hid = params["hook_id"].as_u64().unwrap();
                    self.hooks.borrow_mut().retain(|h| h.id != hid);
                    Ok(json!({ "removed": true }))
                }
                "surface.locate" => {
                    let sid = params["surface_id"].as_u64().unwrap() as u32;
                    let exists = self.alive.borrow().contains(&sid);
                    Ok(json!({ "surface_id": sid, "exists": exists }))
                }
                "terminal.children" => Ok(json!({ "children": self.children.borrow().clone() })),
                _ => Ok(json!({})),
            }
        }
    }

    // ── `--child <index>` → child surface id 해석 — 호스트가 주는 원본 `surface_id`
    // 로만 풀고, 없는 index 는 실재하는 index 목록과 함께 거절한다
    // (docs/plugins/claude/index.md 의 `child-profile` 절) ──

    #[test]
    fn child_index_resolves_to_the_hosts_surface_id() {
        let tr = test_translator();
        let host = MockHost::new();
        host.set_children(vec![
            json!({ "index": 0, "surface_id": 7 }),
            json!({ "index": 1, "surface_id": 9 }),
        ]);
        assert_eq!(resolve_child_surface_id(&host, 3, 1, &tr).unwrap(), 9);
        assert_eq!(resolve_child_surface_id(&host, 3, 0, &tr).unwrap(), 7);
    }

    #[test]
    fn child_index_out_of_range_is_invalid_params_and_lists_what_exists() {
        let tr = test_translator();
        let host = MockHost::new();
        host.set_children(vec![
            json!({ "index": 0, "surface_id": 7 }),
            json!({ "index": 1, "surface_id": 9 }),
        ]);
        let err = resolve_child_surface_id(&host, 3, 99, &tr).unwrap_err();
        assert_eq!(err.code, -32602);
        // 오타인지 자식이 죽은 것인지 구분할 수 있게 실제 index 목록이 실린다.
        assert!(err.message.contains("99"), "{}", err.message);
        assert!(err.message.contains("0, 1"), "{}", err.message);
    }

    #[test]
    fn child_index_with_no_children_at_all_is_rejected() {
        let tr = test_translator();
        let host = MockHost::new();
        let err = resolve_child_surface_id(&host, 3, 0, &tr).unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn child_index_does_not_read_the_claude_remapped_field() {
        // `claude.children` 이 쓰는 `child_surface_id` 만 있고 원본 `surface_id` 가
        // 없으면 해석은 실패해야 한다 — 두 필드를 헷갈린 채 통과하면 엉뚱한
        // surface 를 재기동시킨다.
        let tr = test_translator();
        let host = MockHost::new();
        host.set_children(vec![json!({ "index": 0, "child_surface_id": 7 })]);
        assert!(resolve_child_surface_id(&host, 3, 0, &tr).is_err());
    }

    #[test]
    fn sibling_cleanup_removes_all_after_one_fires() {
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        register_notify_hooks(&host, caller, target, "tell");
        assert_eq!(host.done_commands_on(target).len(), 3, "3 형제 등록");

        // needs-input 이 fire(once 제거) → 나머지 형제(claude-idle, process-exit) 정리.
        assert_eq!(host.fire(target, "needs-input"), 1);
        let expected = notify_done_command(caller, target, "tell");
        cleanup_sibling_hooks(&host, target, &expected);

        assert!(
            host.done_commands_on(target).is_empty(),
            "형제 hook 이 하나도 남지 않아야 함 — process-exit 좀비 없음: {:?}",
            host.done_commands_on(target)
        );
    }

    #[test]
    fn concurrent_registrations_leave_no_zombie() {
        // 같은 child(target) 에 spawn 완료 hook 과 tell 완료 hook 이 겹쳐 등록된 상태
        // (spawn 후 fire 전에 tell 이 들어온 경우). 단일 meta 슬롯을 덮어쓰던 옛
        // 방식이면 여기서 spawn 그룹의 process-exit 이 좀비로 남았다.
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        register_notify_hooks(&host, caller, target, "spawn");
        register_notify_hooks(&host, caller, target, "tell");
        assert_eq!(host.done_commands_on(target).len(), 6, "두 그룹 = 6 hook");
        // 에러 정지 hook 은 command 에 command_name 이 없어 두 그룹이 같은 문자열을
        // 쓴다 — 멱등 등록이라 두 번 불러도 1 개만 남는다.
        assert_eq!(
            host.hooks_for_event(target, crate::error_scan::STALLED_EVENT),
            1,
            "에러 정지 hook 은 중복 등록되지 않아야 함"
        );

        // spawn 그룹의 claude-idle 이 먼저 fire → spawn 그룹만 정리.
        host.fire(target, "claude-idle");
        let spawn_cmd = notify_done_command(caller, target, "spawn");
        cleanup_sibling_hooks(&host, target, &spawn_cmd);

        let remaining = host.done_commands_on(target);
        let tell_cmd = notify_done_command(caller, target, "tell");
        // tell 그룹은 그대로(claude-idle fire 가 tell 의 claude-idle 도 제거했으므로 2개),
        // spawn 그룹은 완전히 사라져야 한다.
        assert!(
            remaining.iter().all(|c| c == &tell_cmd),
            "spawn 그룹 좀비 잔존: {remaining:?}"
        );
        assert!(
            !remaining.iter().any(|c| c == &spawn_cmd),
            "spawn 그룹 process-exit 좀비 남음"
        );

        // 이제 tell 그룹도 fire → 전부 정리.
        host.fire(target, "needs-input");
        cleanup_sibling_hooks(&host, target, &tell_cmd);
        assert!(
            host.done_commands_on(target).is_empty(),
            "최종적으로 형제 hook 이 전부 사라져야 함: {:?}",
            host.done_commands_on(target)
        );
    }

    // ── 자기재무장(self-rearm) — child 가 살아있는 동안 알림 반복 (docs/plugins/claude/index.md 의 자기재무장 절 참조) ──
    //
    // 배경: needs-input/claude-idle 은 process-exit 와 달리 "child 가 아직 살아있는
    // 상태 전환"일 수 있다(예: 애매한 지시에 되묻고 다시 작업 재개). 형제 hook 이
    // once=true 라 한 번 fire 하면 남은 형제도 정리돼 그 spawn/tell 콜당 알림이 딱
    // 1번만 오던 문제 — child 가 진짜 완료되기 전에 needs-input 을 한 번이라도 거치면
    // 그 뒤엔 재알림 경로가 없었다.

    #[test]
    fn handle_notify_done_rearms_when_target_still_alive() {
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        host.mark_alive(target);
        register_notify_hooks(&host, caller, target, "tell");
        assert_eq!(host.done_commands_on(target).len(), 3, "최초 3 형제 등록");

        let tr = test_translator();

        // 1번째 전환: needs-input(되묻기) — child 는 여전히 살아있다.
        assert_eq!(host.fire(target, "needs-input"), 1);
        handle_notify_done(
            &host,
            &json!({ "caller_surface": caller, "target_surface": target, "command": "tell" }),
            &tr,
        )
        .unwrap();
        assert_eq!(
            host.done_commands_on(target).len(),
            3,
            "살아있으면 형제 hook 이 다시 3개로 재무장돼야 함"
        );

        // 2번째 전환: 진짜 완료(claude-idle) — 여전히 살아있는 상태에서 fire 됐다고
        // 가정(실제로는 이 직후 host 가 종료를 감지해도, hook 발화 자체는 idle 이 먼저
        // 다다르는 케이스를 재현). 재무장이 반복되는지 확인.
        assert_eq!(host.fire(target, "claude-idle"), 1);
        handle_notify_done(
            &host,
            &json!({ "caller_surface": caller, "target_surface": target, "command": "tell" }),
            &tr,
        )
        .unwrap();
        assert_eq!(
            host.done_commands_on(target).len(),
            3,
            "두 번째 전환에도 계속 재무장돼야 함 — 'spawn/tell 당 1회' 로 되돌아가면 안 됨"
        );
    }

    // ── 에러 정지 알림 배선 (`claude-error-stalled`) ──

    #[test]
    fn spawn_tell_wiring_subscribes_the_error_axis() {
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        register_notify_hooks(&host, caller, target, "spawn");
        assert_eq!(
            host.hooks_for_event(target, crate::error_scan::STALLED_EVENT),
            1,
            "완료 3종과 함께 에러 정지 hook 도 등록돼야 함"
        );
        assert!(
            host.commands_on(target)
                .iter()
                .any(|c| c == &notify_error_command(caller, target))
        );
    }

    #[test]
    fn error_hook_survives_the_sibling_fire_cleanup_rearm_cycle() {
        // 형제 그룹은 fire → 정리 → 재무장 사이클을 도는데, 에러 정지 hook 은 그
        // 사이클에 휘말리지 않아야 한다(수명이 다르다).
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        host.mark_alive(target);
        register_notify_hooks(&host, caller, target, "tell");
        let tr = test_translator();

        for _ in 0..3 {
            assert_eq!(host.fire(target, "needs-input"), 1);
            handle_notify_done(
                &host,
                &json!({ "caller_surface": caller, "target_surface": target, "command": "tell" }),
                &tr,
            )
            .unwrap();
            assert_eq!(
                host.hooks_for_event(target, crate::error_scan::STALLED_EVENT),
                1,
                "재무장 사이클을 돌아도 에러 hook 은 정확히 1개로 유지"
            );
            assert_eq!(host.done_commands_on(target).len(), 3, "형제 재무장도 정상");
        }
    }

    #[test]
    fn error_hook_is_standing_so_repeated_stalls_keep_notifying() {
        // once=true 였다면 첫 발화 후 사라져 두 번째 정지를 놓친다.
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        register_notify_hooks(&host, caller, target, "spawn");
        assert_eq!(host.fire(target, crate::error_scan::STALLED_EVENT), 1);
        assert_eq!(
            host.hooks_for_event(target, crate::error_scan::STALLED_EVENT),
            1,
            "상시 hook 이라 발화해도 남아야 한다"
        );
        assert_eq!(host.fire(target, crate::error_scan::STALLED_EVENT), 1);
    }

    #[test]
    fn handle_notify_error_requires_both_surfaces() {
        let host = MockHost::new();
        let tr = test_translator();
        assert_eq!(
            handle_notify_error(&host, &json!({ "target_surface": 5 }), &tr)
                .unwrap_err()
                .code,
            -32602
        );
        assert_eq!(
            handle_notify_error(&host, &json!({ "caller_surface": 5 }), &tr)
                .unwrap_err()
                .code,
            -32602
        );
    }

    #[test]
    fn notify_error_message_appends_error_line_hint() {
        // codex `notify-caller` 선례 — 알림 조립 직전 화면을 읽어 실패 원인을 덧붙인다.
        struct ScreenHost(&'static str);
        impl HostCall for ScreenHost {
            fn call(
                &self,
                method: &str,
                _params: Value,
            ) -> Result<Value, tasty_plugin_sdk::PluginError> {
                match method {
                    "surface.screen_text" => Ok(json!({ "text": self.0 })),
                    other => panic!("unexpected host call: {other}"),
                }
            }
        }
        let tr = test_translator();
        let with_hint = notify_error_message(
            &tr,
            &ScreenHost("working…\n  API Error: Connection error\n"),
            42,
        );
        assert!(with_hint.contains("42"), "대상 surface 가 문구에 있어야 함");
        assert!(
            with_hint.contains("API Error: Connection error"),
            "에러 줄 힌트: {with_hint}"
        );

        // 화면에 에러 줄이 없으면 힌트 없이 본문만.
        let plain = notify_error_message(&tr, &ScreenHost("all good\n"), 42);
        assert!(!plain.contains("Last error"), "{plain}");
    }

    #[test]
    fn handle_notify_done_does_not_rearm_when_target_exited() {
        let host = MockHost::new();
        let (caller, target) = (7u32, 1650u32);
        host.mark_alive(target);
        register_notify_hooks(&host, caller, target, "spawn");

        // process-exit 로 fire — host 는 이 시점에 이미 동기로 surface 를 닫으므로
        // surface.locate 가 exists:false 를 돌려주는 상황을 재현.
        assert_eq!(host.fire(target, "process-exit"), 1);
        host.mark_dead(target);
        let tr = test_translator();
        handle_notify_done(
            &host,
            &json!({ "caller_surface": caller, "target_surface": target, "command": "spawn" }),
            &tr,
        )
        .unwrap();

        assert!(
            host.done_commands_on(target).is_empty(),
            "죽은 surface 에 재무장하면 좀비 hook: {:?}",
            host.done_commands_on(target)
        );
    }

    // ── 짝 crate 와 갈린 응답 shape 고정 (`tasty_plugin_agent_common` crate doc
    //    "짝이 갈린 채 남는 것") — codex 에 같은 두 물음을 묻는 짝 시험이 있다 ──

    /// 호스트 원본 응답만 돌려주는 mock. 위 `MockHost` 는 hook 사이클을 흉내 내는
    /// 물건이라 `terminal.kill` 성공 응답의 두 필드를 표현하는 축이 없다 — 그
    /// 축만 따로 세운다.
    struct ShapeHost;

    impl HostCall for ShapeHost {
        fn call(
            &self,
            method: &str,
            _params: Value,
        ) -> Result<Value, tasty_plugin_sdk::PluginError> {
            match method {
                "terminal.children" => Ok(json!({ "children": [{
                    "surface_id": 42,
                    "index": 0,
                    "cwd": "/w",
                    "role": "reviewer",
                    "nickname": "nick",
                    "state": "idle",
                    "evidence": "prompt",
                    "confidence": "certain",
                }]})),
                "surface.foreground_process" => Ok(json!({ "name": "claude", "pid": 4242 })),
                "terminal.kill" => Ok(json!({ "killed_surface_id": 42, "child_index": 0 })),
                other => panic!("unexpected host call: {other}"),
            }
        }
    }

    /// 응답은 **bare 배열**이고, 호스트 `surface_id` 는 `child_surface_id` 로
    /// remap 되며, 자식마다 foreground 정보가 덧씌워진다. 짝 crate(codex)는 호스트
    /// 응답을 그대로 흘린다. 기존 공개 응답을 호환성 때문에 유지하므로,
    /// 그 변환이 조용히 바뀌지 않도록 여기서 못박는다.
    #[test]
    fn children_response_is_a_bare_remapped_array() {
        let out = handle_children(&ShapeHost, &json!({ "surface_id": 1 }), &test_translator())
            .expect("handle_children");
        let arr = out
            .as_array()
            .unwrap_or_else(|| panic!("bare 배열이 아니다 (호스트 shape 으로 돌아갔나): {out}"));
        assert_eq!(arr.len(), 1);
        let e = &arr[0];
        assert_eq!(e["child_surface_id"], json!(42), "remap 이 사라졌다: {e}");
        assert!(
            e.get("surface_id").is_none(),
            "호스트 필드명이 그대로 남았다: {e}"
        );
        // 화이트리스트가 판정 근거 3 축을 다 옮기는지(ADR-0072).
        assert_eq!(e["state"], json!("idle"));
        assert_eq!(e["evidence"], json!("prompt"));
        assert_eq!(e["confidence"], json!("certain"));
        assert_eq!(e["foreground_process"], json!("claude"));
        assert_eq!(e["foreground_pid"], json!(4242));
    }

    /// 성공 응답은 `{"killed": true}` 하나로 줄어든다 — 호스트가 실어 보낸
    /// `killed_surface_id`·`child_index` 는 여기서 버려진다. 짝 crate(codex)는 그
    /// 둘을 그대로 흘린다.
    #[test]
    fn kill_response_is_reduced_to_a_killed_flag() {
        let scanner = Arc::new(Mutex::new(ErrorScanner::new()));
        let out = handle_kill(
            &scanner,
            &ShapeHost,
            &json!({ "surface_id": 1, "child_index": 0 }),
            &test_translator(),
        )
        .expect("handle_kill");
        assert_eq!(
            out,
            json!({ "killed": true }),
            "응답 shape 이 바뀌었다 — 짝 crate 와의 차이가 정해지기 전에는 못 바꾼다"
        );
    }
}
