//! Parent-specific completion notification channel; Claude legacy hooks remain isolated here.
use serde_json::{Value, json};
use tasty_plugin_agent_common::host_call::{HostCall, cleanup_sibling_hooks, surface_is_alive};
use tasty_plugin_sdk::{IpcMethodError, i18n::Translator};

/// caller_surface 에 spawn/tell 완료를 알리는 command 문자열 — 등록 시점과
/// 정리 시점에 동일하게 재생성해야 하므로 순수 함수로 분리한다. 3개 형제 hook이
/// 전부 이 문자열을 그대로 command 로 쓰므로, 서로의 hook_id 를 몰라도
/// (target_surface, command 일치) 기준으로 서로를 찾아 정리할 수 있다.
pub(crate) fn notify_done_command(
    caller_surface: u32,
    target_surface: u32,
    command_name: &str,
) -> String {
    format!(
        "tasty claude notify-done --caller-surface {caller_surface} --target-surface {target_surface} --command {command_name}"
    )
}

/// caller 에게 보여줄 완료 알림 문구 — "그 child 가 맡은 작업이 끝났다"를 앞세운다.
/// 과거 `"{command_name} 완료: surface {target_surface}"` 형태는 spawn/tell 자체가
/// (호출이) 완료됐다는 뜻으로 오독되기 쉬워, conductor 가 실제 작업 완료 알림을
/// "spawn 접수 확인" 정도로 여기고 계속 무시하는 사고로 이어졌다. `command_name`은
/// 호출 방식(spawn/tell)일 뿐 완료의 주어가 아니므로 괄호로 분리한다.
pub(crate) fn notify_done_message(
    tr: &Translator,
    command_name: &str,
    target_surface: u32,
) -> String {
    tr.t("claude.notify.done_message")
        .replacen("{}", &target_surface.to_string(), 1)
        .replacen("{}", command_name, 1)
}

/// 완료 알림 문구에 "API 에러로 턴이 끝났다" 를 덧붙인다 — `error` 는 대상 surface 의
/// [`crate::hook::STOP_FAILURE_META_KEY`] 값(없으면 `None` → 문구 그대로).
///
/// 본문 앞부분은 건드리지 않고 **뒤에 붙인다**: 부모는 이 한 줄을 이미 "작업 완료" 로
/// 읽고 있고(Monitor 필터 · 사람 눈), 에러로 끝난 턴도 턴이 끝나 입력을 기다린다는
/// 점에서는 같은 전환이다. 다른 것은 "왜 끝났나" 하나라 그것만 덧붙인다.
pub(crate) fn with_stop_failure_hint(
    tr: &Translator,
    mut message: String,
    error: Option<&str>,
) -> String {
    if let Some(error) = error.filter(|e| !e.is_empty()) {
        message.push_str(
            &tr.t("claude.notify.stop_failure_hint")
                .replacen("{}", error, 1),
        );
    }
    message
}

/// 대상 surface 의 직전 턴이 API 에러로 끝났으면 그 종류를 읽는다. 조회 실패(이미
/// 닫힌 surface — process-exit 로 발화한 경우)는 기록 없음과 같게 본다: 그때는 붙일
/// 힌트가 없을 뿐 알림 자체는 나가야 한다.
fn last_stop_failure<H: HostCall>(host: &H, target_surface: u32) -> Option<String> {
    host.call(
        "surface.meta.get",
        json!({ "surface_id": target_surface, "key": crate::hook::STOP_FAILURE_META_KEY }),
    )
    .ok()
    .and_then(|r| r.get("value").and_then(|v| v.as_str()).map(String::from))
    .filter(|s| !s.is_empty())
}

/// spawn/tell 완료 시 caller 에게 1회성으로 알려줄 3개의 형제 hook
/// (claude-idle / needs-input / process-exit) 을 target_surface 에 등록한다.
/// 등록 자체는 best-effort — 실패해도 spawn/tell 성공 자체를 막지 않는다.
///
/// 에러 축([`register_error_notify_hook`])은 이 형제 그룹에 **넣지 않는다** —
/// 수명이 다르기 때문이다. 상태 전환 hook 은 "전환했으니 알리고 그룹째 정리 후
/// 재무장" 하는 once 사이클을 도는데, 에러 정지는 그 사이클과 무관하게 반복될 수
/// 있어 같은 그룹에 끼우면 서로의 정리 대상이 되어 사이클이 꼬인다.
pub(crate) fn register_notify_hooks<H: HostCall>(
    host: &H,
    caller_surface: u32,
    target_surface: u32,
    command_name: &str,
) {
    let command = notify_done_command(caller_surface, target_surface, command_name);
    // 이벤트 집합은 이 plugin 의 매니페스트(`contributes.hook_events`)가 근거다 —
    // 등록 루프만 공유하고 목록은 각자 갖는다.
    tasty_plugin_agent_common::host_call::register_completion_hooks(
        host,
        target_surface,
        &command,
        &["claude-idle", "needs-input", "process-exit"],
        "claude",
    );
    register_error_notify_hook(host, caller_surface, target_surface);
}

/// 에러 정지 알림 hook 의 command 문자열. 완료 알림([`notify_done_command`])과 **다른**
/// 문자열이라 `cleanup_sibling_hooks` 의 정리 대상(command 완전 일치)에 걸리지 않는다 —
/// 형제 그룹이 fire·정리·재무장을 반복해도 이 hook 은 건드려지지 않는다.
pub(crate) fn notify_error_command(caller_surface: u32, target_surface: u32) -> String {
    format!(
        "tasty claude notify-error --caller-surface {caller_surface} --target-surface {target_surface}"
    )
}

/// `claude-error-stalled` 를 구독하는 **상시**(once 아님) hook 을 등록한다.
///
/// once 가 아닌 이유: 한 번 알리고 사라지면 그 뒤의 정지는 놓치는데, 재무장을 붙이면
/// 형제 그룹의 once 사이클을 그대로 복제해야 한다. 상시 hook 이면 재무장 자체가
/// 필요 없고, 발사 빈도 상한은 발신 측(`error_scan.rs` 의 쿨다운·에피소드 1회)이
/// 이미 갖고 있다.
///
/// 재등록은 멱등하다 — command 문자열이 (caller, target) 만으로 정해지므로 같은 짝의
/// 재등록은 `cleanup_sibling_hooks` 가 옛것을 걷어내고 같은 자리에 다시 건다.
pub(crate) fn register_error_notify_hook<H: HostCall>(
    host: &H,
    caller_surface: u32,
    target_surface: u32,
) {
    let command = notify_error_command(caller_surface, target_surface);
    cleanup_sibling_hooks(host, target_surface, &command);
    // best-effort — spawn/tell은 유지하되 관측 등록 실패를 기록한다.
    if let Err(error) = host.call(
        "hook.set",
        json!({
            "surface_id": target_surface,
            "event": crate::error_scan::STALLED_EVENT,
            "command": command,
            "once": false,
        }),
    ) {
        tracing::warn!("completion error observation hook registration failed: {error}");
    }
}

/// `tasty claude notify-done` — 형제 once-hook 중 하나가 fire 되어 실행되는
/// 커맨드. caller_surface 의 완료 로그에 완료 상태를 append한 뒤, target_surface 에 남아있는
/// (아직 fire 되지 않은) 나머지 형제 hook 들을 command 문자열 일치로 찾아 정리한다.
pub(crate) fn handle_notify_done<H: HostCall>(
    host: &H,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let caller_surface = params
        .get("caller_surface")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            IpcMethodError::invalid_params(tr.t("claude.params.missing_caller_surface"))
        })? as u32;
    let target_surface = params
        .get("target_surface")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            IpcMethodError::invalid_params(tr.t("claude.params.missing_target_surface"))
        })? as u32;
    let command_name = params
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IpcMethodError::invalid_params(tr.t("claude.params.missing_command")))?;

    // 1) 부모가 Monitor 로 tail 하는 완료 로그에 append 한다 — 부모 종류를 묻지 않는다.
    let message = with_stop_failure_hint(
        tr,
        notify_done_message(tr, command_name, target_surface),
        last_stop_failure(host, target_surface).as_deref(),
    );
    if let Err(e) = tasty_utils::notify::append_notify_line(caller_surface, &message) {
        tracing::warn!("claude notify-done completion-log append failed: {e}");
    }

    // 2) 남은 형제 hook 정리 — 나 자신은 이미 once=true 로 fire 시 자동 제거됐으므로
    //    hook.list 시점엔 나머지(0~2개)만 남아있다. command 문자열 완전 일치로 식별하되
    //    반드시 target_surface 로 필터해 다른 child(=다른 surface)의 hook 은 건드리지
    //    않는다. 상태 없는(clobber 불가) 순수 선택이라 concurrent 등록에도 안전.
    let expected_command = notify_done_command(caller_surface, target_surface, command_name);
    cleanup_sibling_hooks(host, target_surface, &expected_command);

    // 3) target_surface 가 아직 살아있다면(이번 fire 가 process-exit 가 아니었다면)
    //    형제 hook 을 다시 3개 등록해 다음 idle/needs-input 전환에도 알림이 오도록
    //    자기재무장한다 — "spawn/tell 당 알림 1회" 가 아니라 "child 가 살아있는 동안
    //    상태 전환마다 알림"으로 바뀐다. claude-idle/needs-input 은 일시적 상태 전환일
    //    수 있어(예: 애매한 지시에 되묻고 다시 작업 재개) 여기서 멈추면 진짜 완료를
    //    영영 놓친다.
    rearm_if_still_alive(host, caller_surface, target_surface, command_name);

    Ok(json!({}))
}

/// `tasty claude notify-error` — `claude-error-stalled` 상시 hook 이 fire 되어 실행되는
/// 커맨드. caller_surface 의 알림 로그에 "자식이 에러 후 멈췄다" 를 append 한다.
///
/// 완료 알림(`notify-done`)과 달리 **형제 정리도 재무장도 하지 않는다** — 상시 hook 이라
/// 그대로 남아 다음 정지도 받는다. 상태 축도 건드리지 않는다(`terminal.set_state` 미호출):
/// 에러는 재시도로 복구될 수 있어 상태로 승격하면 오탐이 되고, 파생 상태는 관측 융합의
/// 출력 전용 계약이다(`docs/adr/0072-child-state-hook-observation-fusion.md`).
pub(crate) fn handle_notify_error<H: HostCall>(
    host: &H,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let caller_surface = params
        .get("caller_surface")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            IpcMethodError::invalid_params(tr.t("claude.params.missing_caller_surface"))
        })? as u32;
    let target_surface = params
        .get("target_surface")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            IpcMethodError::invalid_params(tr.t("claude.params.missing_target_surface"))
        })? as u32;

    let message = notify_error_message(tr, host, target_surface);
    if let Err(e) = tasty_utils::notify::append_notify_line(caller_surface, &message) {
        tracing::warn!("claude notify-error completion-log append failed: {e}");
    }
    Ok(json!({}))
}

/// 정지 알림 문구 — 알림 조립 직전 대상 화면을 읽어 에러 줄을 힌트로 덧붙인다(codex
/// `notify-caller` 와 같은 방식). 화면 조회는 best-effort 라, 실패하면 힌트 없이
/// 본문만 보낸다.
pub(crate) fn notify_error_message<H: HostCall>(
    tr: &Translator,
    host: &H,
    target_surface: u32,
) -> String {
    let screen = host
        .call(
            "surface.screen_text",
            json!({ "surface_id": target_surface }),
        )
        .ok()
        .and_then(|r| r.get("text").and_then(|t| t.as_str()).map(str::to_string));
    let error_line = screen
        .as_deref()
        .and_then(crate::error_scan::first_error_line);
    // **원인을 문구가 가른다.** 발사 이벤트 키는 하나라(개명하면 부모가 이미 등록해
    // 둔 훅이 전부 깨진다) 에러 뒤 정지와 에러 없는 정지가 같은 키로 온다 — 부모는
    // 둘에 다르게 대응하므로 한 줄 안에서 구별돼야 한다.
    // `docs/adr/0266-derived-stale-must-reach-the-push-channel.md` 결정 2.
    let key = if error_line.is_some() {
        "claude.notify.stalled_message"
    } else {
        "claude.notify.stalled_no_error_message"
    };
    let mut message = tr.t(key).replacen("{}", &target_surface.to_string(), 1);
    if let Some(line) = error_line {
        // 화면 한 줄이 그대로 알림에 실린다 — 로그 한 줄 형식을 깨지 않도록 길이를 자른다.
        let hint: String = line.chars().take(160).collect();
        message.push_str(&tr.t("claude.notify.stalled_hint").replacen("{}", &hint, 1));
    }
    message
}

/// `target_surface` 가 host 트리에 여전히 존재하면(=이번 fire 가 process-exit 가
/// 아니었다면) 형제 hook 3개를 재등록한다. process-exit 로 fire 된 경우 host 는 hook
/// 발화 직후 동기로 그 surface 를 닫으므로(`close_surface_by_id_no_snapshot`) 이
/// 시점엔 이미 사라져 있고, 반대로 claude-idle/needs-input 은 surface 가 살아있는
/// 상태에서만 나는 이벤트라 재등록이 안전하다. **조회 실패를 어느 쪽으로 볼지는
/// [`surface_is_alive`] 가 한 곳에서 정한다** — 그 사유의 사본을 여기 두지 않는다.
///
/// ★ 짝 crate(codex)에 **본문이 같은** 함수가 있고 합치지 않았다. 이유는 부르는
/// `register_notify_hooks` 가 crate 마다 다른 이벤트 목록·다른 command 문자열을 쓰기
/// 때문이다 — 그것을 클로저로 주입하면 공용 함수에 남는 것이 `if 조건 { f() }` 뿐이라
/// 아무 판정도 들고 가지 않는다. 갈릴 수 있는 판정(생존 읽기)은 이미
/// [`surface_is_alive`] 한 벌이고, 이벤트 목록이 갈린 근거는 각자의 매니페스트
/// `contributes.hook_events` 다(`tasty_plugin_agent_common` crate doc).
pub(crate) fn rearm_if_still_alive<H: HostCall>(
    host: &H,
    caller_surface: u32,
    target_surface: u32,
    command_name: &str,
) {
    if surface_is_alive(host, target_surface) {
        register_notify_hooks(host, caller_surface, target_surface, command_name);
    }
}
