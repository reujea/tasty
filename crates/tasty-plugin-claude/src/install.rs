//! `~/.claude/settings.json` 머지 로직.
//!
//! 호스트에 있던 install/uninstall helper 를 1:1 옮긴 것. cutover 로 호스트
//! 측은 제거됐고 본 모듈이 단일 출처다.
//!
//! `is_tasty_stop_hook_installed`는 tasty Stop hook 설치 여부를 점검하는
//! 별도 노출 함수다 — 실사용 소비자가 생긴 적은 없다.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Value, json};
use tasty_plugin_sdk::i18n::Translator;

/// tasty가 자동으로 등록하는 Claude Code hook 이벤트 목록.
/// `(claude_event_name, tasty_hook_token, matcher)` 3-튜플. 정의는 이 목록 하나뿐이다 —
/// `profile.rs`(내장 훅 나열) · `hook.rs` 는 이것을 읽을 뿐 사본을 두지 않는다.
///
/// `UserPromptSubmit` 은 child 가 *2 번째 이후 prompt* 를 받을 때 ClaudeState 의
/// idle=true (직전 Stop hook 잔재) 를 clear 하는 데 필수. 미등록 시 multi-round
/// 대화에서 idle 상태 조회(`terminal.children` 등)가 *진짜 active 인 child* 를
/// idle 로 잘못 보고하는 transient state bug 발생 (구현 중 확인됨).
///
/// `PreToolUse`/`PostToolUse` 는 matcher `"AskUserQuestion"` 으로 좁혀 그 툴
/// 호출에만 발화한다(나머지는 matcher `""` 로 전부 받는다). 실측(실제 Claude Code
/// 를 띄워 hook stdin payload 를 덤프해 확인)으로 근거를 얻었다:
/// - `AskUserQuestion` 답변은 `UserPromptSubmit` 을 발생시키지 않는다(같은 프롬프트
///   turn 안의 tool 상호작용이라 새 프롬프트로 카운트되지 않음) — 그래서 기존
///   `UserPromptSubmit`(→active) 만으로는 needs_input 해제 시점을 잡을 수 없다.
///   `PostToolUse`/`AskUserQuestion` 이 답변 즉시(관찰상 `duration_ms: 0`) 발화해
///   그 해제 신호를 정확히 제공한다 — 그래서 `PreToolUse` 단독이 아니라 반드시
///   짝을 이뤄 추가한다.
/// - `PreToolUse`/`AskUserQuestion` 은 인터랙티브 선택 UI 가 뜨기 **전에** 발화하고
///   `tool_input.questions` 를 담고 있어, "질문을 막 띄우려는 참"을 구조적으로
///   (matcher 로 tool 이름 자체를 보증) 정밀하게 잡는다.
///
/// `StopFailure` 는 API 에러(재시도를 다 쓴 529 · rate limit · 인증 실패 …)로 턴이 끝날
/// 때 `Stop` **대신** 발화한다. 이것이 없으면 그 턴은 끝났다는 신호가 하나도 오지 않아
/// 상태가 직전 `UserPromptSubmit` 이 남긴 `active` 에 머물고, 부모 완료 알림도 나가지
/// 않는다 — `UserPromptSubmit` 미등록 때와 같은 부류의 상태 오보고다. matcher 는 `""`
/// (에러 종류 전부) — 종류는 stdin payload 의 `error` 로 받아 plugin 이 가른다.
pub const MANAGED_HOOKS: &[(&str, &str, &str)] = &[
    ("Stop", "stop", ""),
    ("Notification", "notification", ""),
    ("SessionEnd", "session-end", ""),
    ("SubagentStop", "subagent-stop", ""),
    ("SessionStart", "session-start", ""),
    ("UserPromptSubmit", "prompt-submit", ""),
    ("PreToolUse", "pre-tool-use", "AskUserQuestion"),
    ("PostToolUse", "post-tool-use", "AskUserQuestion"),
    ("StopFailure", "stop-failure", ""),
];

/// `entry_matches_marker`가 식별자로 사용하는 substring.
fn tasty_hook_marker(event_token: &str) -> String {
    format!("tasty claude hook {}", event_token)
}

/// `settings.json` / 세션 프로필에 기록되는 hook 명령의 **단일 출처**.
///
/// 형태: `if [ -n "$TASTY_SURFACE_ID" ]; then <argv> || true; fi`
///
/// **왜 `A && B || true` 가 아니라 `if` 인가** — 옛 형태에서 `|| true` 는 서로 다른
/// 두 가지를 동시에 삼켰다:
/// 1. `$TASTY_SURFACE_ID` 미설정(= tasty 밖에서 Claude Code 를 쓰는 사용자) →
///    조용히 성공해야 하는 **정당한 경우**.
/// 2. `tasty claude hook` 자체의 실패 → 상태 push 유실인데 아무 흔적도 안 남는
///    **문제인 경우**.
///
/// 두 경우가 한 연산자에 묶여 있어 분리 없이 `|| true` 를 떼면 1 번이 깨진다.
/// 가드를 `if` 로 올려 두면 1 번은 블록에 진입조차 하지 않고(가드 자체가 명시적
/// 성공 종료), 안쪽 `|| true` 는 오직 2 번만 담당한다 — 실패 처리 정책을 바꿀 때
/// 손댈 지점이 한 군데로 좁혀진다.
///
/// **안쪽 `|| true` 를 지금 떼지 않는 이유**: hook 의 비-0 exit 를 Claude Code 가
/// 어떻게 다루는지(무시/경고/턴 차단)는 외부 도구 런타임 동작이라 이 저장소에서
/// 확인할 수 없다. 실패의 **기록**은 CLI 쪽(`tasty_cli::hook_failure`)이 IPC 와
/// 무관한 로컬 파일로 남기므로, exit code 를 노출하지 않아도 관측 가능성 목적은
/// 달성된다. 노출 여부는 런타임 실측 뒤에 결정한다.
///
/// **marker 호환**: [`tasty_hook_marker`] 는 `"tasty claude hook <token>"` substring
/// 으로 기존 entry 를 찾는다. 이 형태는 그 substring 을 그대로 포함하므로 기존
/// 설치본의 in-place upgrade 경로가 계속 발동한다(깨지면 옛 entry 가 남은 채 새
/// entry 가 추가돼 hook 이 두 번 발화한다).
pub(crate) fn tasty_guarded_command(argv: &str) -> String {
    format!("if [ -n \"$TASTY_SURFACE_ID\" ]; then {argv} || true; fi")
}

/// settings.json에 실제로 기록되는 명령 문자열.
///
/// `session_id` 와 `message` 등 hook 별 가변 데이터는 Claude Code 가 stdin 으로
/// 흘려보내는 JSON payload 에서 `tasty claude hook` CLI 가 직접 읽어 채운다
/// (매니페스트 `stdin_json = true` + `stdin_field` 매핑). 그래서 명령 문자열은
/// 어느 event 에서도 동일한 형태로 충분하다.
fn tasty_hook_command(event_token: &str) -> String {
    tasty_guarded_command(&format!("tasty claude hook {event_token}"))
}

pub(crate) fn claude_settings_path(tr: &Translator) -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!(tr.t("claude.install.no_home_dir").to_string()))?;
    Ok(base.home_dir().join(".claude").join("settings.json"))
}

fn entry_matches_marker(entry: &Value, marker: &str) -> bool {
    entry
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(marker))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// 비-테스트 빌드에서 유일한 호출자가 `is_tasty_stop_hook_installed`(그 자체도
/// 실사용처 없음)뿐이라 함께 개별 억제한다. 테스트에서는 직접 호출된다.
#[allow(dead_code)]
pub(crate) fn is_marker_installed_in_value(root: &Value, event_name: &str, marker: &str) -> bool {
    let Some(hooks) = root.get("hooks").and_then(|h| h.as_object()) else {
        return false;
    };
    let Some(arr) = hooks.get(event_name).and_then(|v| v.as_array()) else {
        return false;
    };
    arr.iter().any(|entry| entry_matches_marker(entry, marker))
}

/// `~/.claude/settings.json`을 읽어 tasty Stop hook이 설치돼 있는지. 파일이
/// 없으면 false. 별도 노출 함수로 만들어졌으나 실사용 소비자가 생긴 적이
/// 없다 — 삭제 대신 유지, 개별 억제만 부여.
#[allow(dead_code)]
pub(crate) fn is_tasty_stop_hook_installed(tr: &Translator) -> Result<bool> {
    let path = claude_settings_path(tr)?;
    if !path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&path)?;
    let root: Value = serde_json::from_str(&content)?;
    let marker = tasty_hook_marker("stop");
    Ok(is_marker_installed_in_value(&root, "Stop", &marker))
}

/// settings.json 루트 값에 hook을 idempotent하게 추가.
pub(crate) fn install_hooks_in_value(
    root: &mut Value,
    tr: &Translator,
) -> Result<Vec<&'static str>> {
    let root_obj = root.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!(tr.t("claude.install.settings_root_not_object").to_string())
    })?;

    let hooks_obj = root_obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(tr.t("claude.install.hooks_not_object").to_string()))?;

    let mut added: Vec<&'static str> = Vec::new();

    for (event_name, event_token, matcher) in MANAGED_HOOKS {
        let marker = tasty_hook_marker(event_token);
        let command = tasty_hook_command(event_token);

        let arr = hooks_obj
            .entry((*event_name).to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                anyhow::anyhow!(tr.t_fmt("claude.install.hooks_event_not_array", event_name))
            })?;

        // 기존에 marker 가 일치하는 entry 가 있으면, 그 entry 의 matcher 와 그 안의
        // hook 명령 문자열을 canonical 한 새 값으로 갱신한다. 옛 버전이 설치한 잘못된
        // 명령(예: `tasty claude hook session-start --session ${CLAUDE_SESSION_ID}`)
        // 이나 옛 matcher 가 그대로 남아 있어도, install 을 재실행하면 자동으로 최신
        // 형태로 upgrade 된다(matcher 비교를 넣지 않으면 command 문자열만 최신화되고
        // matcher 는 옛 값에 고정돼버린다 — `PreToolUse`/`PostToolUse` 처럼 matcher 가
        // 의미를 갖는 항목의 향후 matcher 변경 시 이 경로가 필요하다).
        let mut upgraded = false;
        for entry in arr.iter_mut() {
            if !entry_matches_marker(entry, &marker) {
                continue;
            }
            upgraded = true;
            if let Some(obj) = entry.as_object_mut() {
                let matcher_needs_update = obj
                    .get("matcher")
                    .and_then(|m| m.as_str())
                    .map(|m| m != *matcher)
                    .unwrap_or(true);
                if matcher_needs_update {
                    obj.insert("matcher".into(), Value::String((*matcher).to_string()));
                }
            }
            if let Some(hooks) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                for h in hooks.iter_mut() {
                    let needs_update = h
                        .get("command")
                        .and_then(|c| c.as_str())
                        .map(|c| c.contains(&marker) && c != command)
                        .unwrap_or(false);
                    if needs_update && let Some(obj) = h.as_object_mut() {
                        obj.insert("command".into(), Value::String(command.clone()));
                    }
                }
            }
        }
        if upgraded {
            continue;
        }

        arr.push(json!({
            "matcher": matcher,
            "hooks": [
                {
                    "type": "command",
                    "command": command
                }
            ]
        }));
        added.push(*event_name);
    }

    Ok(added)
}

/// settings.json 루트 값에서 tasty hook entry를 제거.
pub(crate) fn uninstall_hooks_from_value(root: &mut Value) -> Vec<&'static str> {
    let Some(root_obj) = root.as_object_mut() else {
        return Vec::new();
    };
    let Some(hooks) = root_obj.get_mut("hooks") else {
        return Vec::new();
    };
    let Some(hooks_obj) = hooks.as_object_mut() else {
        return Vec::new();
    };

    let mut removed: Vec<&'static str> = Vec::new();

    for (event_name, event_token, _matcher) in MANAGED_HOOKS {
        let marker = tasty_hook_marker(event_token);

        let Some(arr) = hooks_obj
            .get_mut(*event_name)
            .and_then(|v| v.as_array_mut())
        else {
            continue;
        };

        let before_len = arr.len();
        arr.retain(|entry| !entry_matches_marker(entry, &marker));
        let changed = arr.len() != before_len;

        if changed {
            removed.push(*event_name);
        }

        if arr.is_empty() {
            hooks_obj.remove(*event_name);
        }
    }

    if hooks_obj.is_empty() {
        root_obj.remove("hooks");
    }

    removed
}

/// `claude.install` IPC 핸들러. ~/.claude/settings.json을 idempotent하게 갱신.
pub(crate) fn run_install(tr: &Translator) -> Result<Vec<&'static str>> {
    let path = claude_settings_path(tr)?;

    let mut root: Value = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        json!({})
    };

    let added = install_hooks_in_value(&mut root, tr)?;
    if added.is_empty() {
        return Ok(added);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let output = serde_json::to_string_pretty(&root)?;
    std::fs::write(&path, output)?;
    Ok(added)
}

/// `claude.uninstall` IPC 핸들러. 파일이 없으면 빈 목록 반환.
pub(crate) fn run_uninstall(tr: &Translator) -> Result<Vec<&'static str>> {
    let path = claude_settings_path(tr)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    let mut root: Value = serde_json::from_str(&content)?;
    let removed = uninstall_hooks_from_value(&mut root);

    if removed.is_empty() {
        return Ok(removed);
    }
    let output = serde_json::to_string_pretty(&root)?;
    std::fs::write(&path, output)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_translator() -> Translator {
        let lang_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang");
        Translator::load(&lang_dir, "en")
    }

    fn count_managed_entries(root: &Value, event_name: &str, marker: &str) -> usize {
        root.get("hooks")
            .and_then(|h| h.as_object())
            .and_then(|h| h.get(event_name))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|e| entry_matches_marker(e, marker))
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn install_in_empty_value_adds_all_events() {
        let mut root = json!({});
        let added = install_hooks_in_value(&mut root, &test_translator()).expect("install");
        assert_eq!(added.len(), MANAGED_HOOKS.len());
        for (event_name, token, _matcher) in MANAGED_HOOKS {
            let marker = tasty_hook_marker(token);
            assert_eq!(
                count_managed_entries(&root, event_name, &marker),
                1,
                "missing event {} after install",
                event_name
            );
        }
    }

    /// 위 테스트와 `install_is_idempotent` 는 `MANAGED_HOOKS` 를 순회해 기대값을 만들므로 항목이 빠져도
    /// 초록이다. 이름으로 박아 두어야 `StopFailure` 배선이 사라질 때 이 자리가 빨개진다.
    #[test]
    fn install_adds_stop_failure_entry_exactly_once() {
        let mut root = json!({});
        install_hooks_in_value(&mut root, &test_translator()).expect("install 1");
        install_hooks_in_value(&mut root, &test_translator()).expect("install 2");
        assert_eq!(
            count_managed_entries(&root, "StopFailure", &tasty_hook_marker("stop-failure")),
            1
        );
    }

    #[test]
    fn install_upgrades_stale_command() {
        // 옛 install 이 남긴 잘못된 SessionStart command 문자열이, 재 install 시
        // canonical 한 새 형태로 자동 갱신되어야 한다. 사용자가 uninstall→install
        // 수작업을 하지 않아도 복원이 정상화되도록.
        let stale_command = "[ -n \"$TASTY_SURFACE_ID\" ] && tasty claude hook session-start --session ${CLAUDE_SESSION_ID} || true";
        let mut root = json!({
            "hooks": {
                "SessionStart": [
                    {
                        "matcher": "",
                        "hooks": [{ "type": "command", "command": stale_command }]
                    }
                ]
            }
        });
        let added = install_hooks_in_value(&mut root, &test_translator()).expect("install");
        // 신규 추가가 아니라 in-place upgrade 라서 added 에 SessionStart 는 없다.
        assert!(!added.contains(&"SessionStart"));
        let arr = root["hooks"]["SessionStart"].as_array().unwrap();
        // 중복 entry 가 추가되지 않고 한 개만 남는다.
        let tasty_count = arr
            .iter()
            .filter(|e| {
                e["hooks"][0]["command"]
                    .as_str()
                    .map(|c| c.contains("tasty claude hook session-start"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            tasty_count, 1,
            "stale entry should be upgraded, not duplicated"
        );
        let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(cmd, tasty_hook_command("session-start"));
        assert!(!cmd.contains("${CLAUDE_SESSION_ID}"));
    }

    #[test]
    fn install_sets_pre_post_tool_use_matcher_to_ask_user_question() {
        let mut root = json!({});
        install_hooks_in_value(&mut root, &test_translator()).expect("install");
        assert_eq!(root["hooks"]["PreToolUse"][0]["matcher"], "AskUserQuestion");
        assert_eq!(
            root["hooks"]["PostToolUse"][0]["matcher"],
            "AskUserQuestion"
        );
        // 나머지는 matcher `""` 로 전부 받는다(`StopFailure` 는 에러 종류 전부).
        for event_name in [
            "Stop",
            "Notification",
            "SessionEnd",
            "SubagentStop",
            "SessionStart",
            "UserPromptSubmit",
            "StopFailure",
        ] {
            assert_eq!(
                root["hooks"][event_name][0]["matcher"], "",
                "{event_name} matcher should remain empty"
            );
        }
    }

    #[test]
    fn install_upgrades_stale_matcher() {
        // 멱등성 점검: matcher 없이(빈 문자열로) 깔려있던 옛 entry 를 재-install 하면
        // canonical matcher("AskUserQuestion")로 갱신돼야 한다 — command 문자열만
        // 비교하던 옛 upgrade 로직이라면 이 매처 드리프트를 감지하지 못했을 것이다.
        let mut root = json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "",
                        "hooks": [{ "type": "command", "command": tasty_hook_command("pre-tool-use") }]
                    }
                ]
            }
        });
        let added = install_hooks_in_value(&mut root, &test_translator()).expect("install");
        assert!(
            !added.contains(&"PreToolUse"),
            "in-place upgrade, not a fresh add"
        );
        let arr = root["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(arr.len(), 1, "no duplicate entry");
        assert_eq!(arr[0]["matcher"], "AskUserQuestion");
    }

    #[test]
    fn install_is_idempotent() {
        let mut root = json!({});
        install_hooks_in_value(&mut root, &test_translator()).expect("install 1");
        let added2 = install_hooks_in_value(&mut root, &test_translator()).expect("install 2");
        assert!(added2.is_empty(), "second install should add nothing");
        for (event_name, token, _matcher) in MANAGED_HOOKS {
            let marker = tasty_hook_marker(token);
            assert_eq!(count_managed_entries(&root, event_name, &marker), 1);
        }
    }

    #[test]
    fn install_preserves_other_hooks() {
        // `PreToolUse` 는 이제 tasty 도 관리하는 event(matcher "AskUserQuestion")라,
        // 사용자가 그 아래 다른 matcher("Bash")로 넣어둔 entry 와 공존해야 한다 —
        // tasty entry 는 *추가*될 뿐 사용자 entry 를 건드리거나 대체하지 않는다.
        let mut root = json!({
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "echo user" }] }
                ],
                "Stop": [
                    { "matcher": "", "hooks": [{ "type": "command", "command": "echo user-stop" }] }
                ]
            }
        });
        install_hooks_in_value(&mut root, &test_translator()).expect("install");

        let pretool_arr = root["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(
            pretool_arr.len(),
            2,
            "user's Bash-matcher entry preserved + tasty's AskUserQuestion-matcher entry added"
        );
        assert_eq!(pretool_arr[0]["matcher"], "Bash");
        assert_eq!(pretool_arr[0]["hooks"][0]["command"], "echo user");
        let stop_arr = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_arr.len(), 2);
    }

    #[test]
    fn uninstall_removes_all() {
        let mut root = json!({});
        install_hooks_in_value(&mut root, &test_translator()).expect("install");
        let removed = uninstall_hooks_from_value(&mut root);
        assert_eq!(removed.len(), MANAGED_HOOKS.len());
        assert!(root.get("hooks").is_none(), "empty hooks should be removed");
    }

    #[test]
    fn uninstall_preserves_user_entries() {
        let mut root = json!({
            "hooks": {
                "Stop": [
                    { "matcher": "", "hooks": [{ "type": "command", "command": "echo user-stop" }] }
                ]
            }
        });
        install_hooks_in_value(&mut root, &test_translator()).expect("install");
        // 반환값(제거된 항목 목록)은 다음 assert가 root 구조로 검증하므로 무시.
        uninstall_hooks_from_value(&mut root);
        let stop_arr = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_arr.len(), 1);
        let cmd = stop_arr[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("user-stop"));
    }

    #[test]
    fn uninstall_on_empty_settings_returns_empty() {
        let mut root = json!({});
        let removed = uninstall_hooks_from_value(&mut root);
        assert!(removed.is_empty());
    }

    #[test]
    fn is_marker_installed_in_value_works() {
        let mut root = json!({});
        let marker = tasty_hook_marker("stop");
        assert!(!is_marker_installed_in_value(&root, "Stop", &marker));
        install_hooks_in_value(&mut root, &test_translator()).expect("install");
        assert!(is_marker_installed_in_value(&root, "Stop", &marker));
    }

    /// 모든 hook event 의 명령 문자열이 동일한 단순 형태인지 검증한다. session_id
    /// 등 가변 데이터는 stdin JSON 으로 흐르므로, 명령 자체는 event 토큰만 다르다.
    /// (옛 버전이 `--session ${CLAUDE_SESSION_ID}` 같은 쉘 확장에 의존하다 동작
    /// 실패했던 회귀를 막는다.)
    #[test]
    fn hook_command_matches_host_format() {
        assert_eq!(
            tasty_hook_command("stop"),
            "if [ -n \"$TASTY_SURFACE_ID\" ]; then tasty claude hook stop || true; fi"
        );
        assert_eq!(
            tasty_hook_command("session-start"),
            "if [ -n \"$TASTY_SURFACE_ID\" ]; then tasty claude hook session-start || true; fi"
        );
        assert_eq!(
            tasty_hook_command("notification"),
            "if [ -n \"$TASTY_SURFACE_ID\" ]; then tasty claude hook notification || true; fi"
        );
    }

    /// 가드는 **`if` 블록**이어야 한다 — `A && B || true` 형태로 되돌아가면
    /// "TASTY_SURFACE_ID 미설정"(정당한 침묵)과 "hook 명령 실패"(관측해야 할 유실)가
    /// 다시 한 연산자에 묶여 실패 처리 정책을 분리할 수 없게 된다.
    #[test]
    fn hook_command_separates_guard_from_failure_handling() {
        for (_, token, _) in MANAGED_HOOKS {
            let cmd = tasty_hook_command(token);
            assert!(
                cmd.starts_with("if [ -n \"$TASTY_SURFACE_ID\" ]; then "),
                "가드가 if 블록이 아니다: {cmd}"
            );
            assert!(cmd.ends_with("; fi"), "블록이 닫히지 않았다: {cmd}");
            assert!(
                !cmd.contains("] && "),
                "옛 `A && B || true` 형태로 회귀했다: {cmd}"
            );
        }
    }

    /// 명령 문자열이 바뀌어도 marker 매칭은 계속 성립해야 한다. 깨지면 기존
    /// 설치본의 in-place upgrade 경로가 발동하지 않아 **옛 entry 가 남은 채 새
    /// entry 가 추가되고 hook 이 두 번 발화**한다.
    #[test]
    fn new_command_still_contains_marker() {
        for (_, token, _) in MANAGED_HOOKS {
            let marker = tasty_hook_marker(token);
            assert!(
                tasty_hook_command(token).contains(&marker),
                "marker '{marker}' 를 잃었다"
            );
        }
    }

    /// 옛 프로덕션 문자열(`[ -n ... ] && tasty claude hook <t> || true`)이 박힌
    /// settings.json 에 install 을 다시 걸면, entry 가 늘지 않고 command 만 새
    /// 형태로 **제자리 갱신**된다. 이 저장소가 실제로 배포했던 문자열이라
    /// 기존 사용자 전원이 통과하는 경로다.
    #[test]
    fn install_upgrades_previous_production_command_in_place() {
        let legacy = "[ -n \"$TASTY_SURFACE_ID\" ] && tasty claude hook stop || true";
        let mut root = json!({
            "hooks": {
                "Stop": [{
                    "matcher": "",
                    "hooks": [{ "type": "command", "command": legacy }]
                }]
            }
        });
        let added = install_hooks_in_value(&mut root, &test_translator()).expect("install");
        assert!(
            !added.contains(&"Stop"),
            "신규 추가가 아니라 upgrade 여야 한다"
        );

        let arr = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(arr.len(), 1, "중복 entry 가 생기면 hook 이 두 번 발화한다");
        assert_eq!(
            arr[0]["hooks"][0]["command"].as_str().unwrap(),
            tasty_hook_command("stop")
        );

        // 두 번째 install 도 멱등.
        install_hooks_in_value(&mut root, &test_translator()).expect("install again");
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    /// 가드 동작 회귀 — `$TASTY_SURFACE_ID` 가 없으면 조용히 exit 0 이어야 한다
    /// (tasty 밖에서 Claude Code 를 쓰는 사용자에게 소음을 내지 않는다).
    /// 실제 `sh` 로 실행해 형태가 아니라 **동작**을 고정한다.
    #[cfg(unix)]
    #[test]
    fn guard_exits_silently_without_surface_id() {
        // `tasty` 가 PATH 에 없어도(=실행되면 반드시 실패) 가드가 막아 exit 0.
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(tasty_hook_command("stop"))
            .env_remove("TASTY_SURFACE_ID")
            .env("PATH", "/nonexistent")
            .output()
            .expect("/bin/sh");
        assert!(out.status.success(), "가드 통과 시 exit 0 이어야 한다");
        assert!(out.stdout.is_empty(), "stdout 무소음");
        assert!(
            out.stderr.is_empty(),
            "stderr 무소음: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// 반대쪽 — surface id 가 있으면 블록에 진입한다. 안쪽 `|| true` 때문에 최종
    /// exit 는 여전히 0 이지만(에이전트 턴 방해 금지), 명령이 **실행은 됐다**는 것을
    /// stderr 로 확인한다(가드가 과하게 막고 있지 않다는 증거).
    #[cfg(unix)]
    #[test]
    fn guard_runs_command_when_surface_id_present() {
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(tasty_hook_command("stop"))
            .env("TASTY_SURFACE_ID", "999")
            .env("PATH", "/nonexistent")
            .output()
            .expect("/bin/sh");
        assert!(out.status.success(), "hook 실패는 턴을 막지 않는다(exit 0)");
        assert!(
            !out.stderr.is_empty(),
            "블록에 진입해 명령을 시도했어야 한다"
        );
    }

    /// SessionStart hook 도 다른 hook 과 동일한 단순 명령으로 설치되어야 한다.
    #[test]
    fn session_start_hook_uses_simple_command() {
        let mut root = json!({});
        install_hooks_in_value(&mut root, &test_translator()).expect("install");
        let arr = root["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let cmd = arr[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            !cmd.contains("${CLAUDE_SESSION_ID}"),
            "SessionStart hook must not contain shell-expansion placeholder anymore (session_id arrives via stdin JSON), got: {}",
            cmd
        );
        assert!(
            !cmd.contains("--session"),
            "SessionStart hook must not pass --session via CLI (session_id is read from stdin), got: {}",
            cmd
        );
        assert!(cmd.contains("tasty claude hook session-start"));
    }
}
