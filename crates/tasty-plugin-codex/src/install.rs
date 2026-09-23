//! Scoped hook configuration installation; never guesses a remote daemon configuration.
use serde_json::{Value, json};
use tasty_plugin_sdk::{IpcMethodError, i18n::Translator};

pub(crate) fn handle_install(params: &Value, tr: &Translator) -> Result<Value, IpcMethodError> {
    let path = selected_path(params, tr)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            IpcMethodError::new(tr.t_replace(
                "codex.install.mkdir_failed",
                "{detail}",
                &e.to_string(),
            ))
        })?;
    }
    let existing = read_config(&path, tr)?;
    let merged = merge_install(existing);
    write_toml(&path, &merged, tr)?;
    let trusted = codex_hooks_all_trusted_in(&merged, &path.to_string_lossy());
    let mut resp = json!({
        "installed": true,
        "path": path.to_string_lossy(),
        "trust_status": if trusted { "trusted" } else { "needs_review" },
    });
    if !trusted {
        resp["note"] = Value::String(
            "Review newly added or changed hooks in Codex using /hooks. Tasty includes \
--dangerously-bypass-hook-trust in launch commands, but Codex may still show hook review \
on resume. This metadata is not runtime trust verification; installation does not approve \
hooks or prove that a remote daemon loaded this configuration."
                .into(),
        );
    }
    Ok(resp)
}

pub(crate) fn handle_uninstall(params: &Value, tr: &Translator) -> Result<Value, IpcMethodError> {
    let path = selected_path(params, tr)?;
    if !path.exists() {
        return Ok(json!({ "uninstalled": true, "path": path.to_string_lossy(), "noop": true }));
    }
    let existing = read_config(&path, tr)?;
    let cleaned = remove_install(existing);
    write_toml(&path, &cleaned, tr)?;
    Ok(json!({ "uninstalled": true, "path": path.to_string_lossy() }))
}

// ───── install/uninstall helpers ─────
//
// Codex CLI 0.130 의 hook 설정은 `~/.codex/config.toml` 의 `[hooks]` 섹션에 박는다.
// 이전 구현은 `~/.codex/settings.json` 에 썼으나 codex 가 그 파일은 *external agent
// config migration* (Claude Code 호환용) 경로에서만 읽고 hook dispatch 에는 쓰지
// 않는다. 그래서 install 했어도 hook 이 한 번도 fire 되지 않았다.
//
// TOML 스키마 (binary strings + 실 동작 검증):
//
// ```toml
// [[hooks.Stop]]                   # MatcherGroup 배열 entry
// # matcher = "..."                # PreToolUse 등에서 tool name regex. Stop 은 omit.
//
// [[hooks.Stop.hooks]]             # HookHandlerConfig 배열
// type = "command"                 # internally tagged enum 의 discriminator
// command = "..."
// # timeout = 5                    # optional, 초 단위
// # async = false                  # optional
// ```
//
// Codex 가 지원하는 event(0.154.0 바이너리 실측): PreToolUse, PermissionRequest,
// PostToolUse, PreCompact, PostCompact, SessionStart, SessionEnd, UserPromptSubmit,
// SubagentStart, SubagentStop, Stop, Interrupt. tasty 는 idle/needs_input/active
// 트래킹에 필요한 7 개만 박는다 ([`HOOK_EVENTS`]).
//
// Trust gate: codex 는 새 hook entry 를 *trust* 하기 전엔 fire 하지 않고 TUI 에
// "1 hook needs review" 표시 후 `/hooks` 명령 승인을 요구한다 (`HookStateToml`
// 의 `trusted_hash` 메커니즘). install 자체는 멱등하게 entry 를 박지만, 승인
// 없이는 hook 이 fire 되지 않는다 — **단, `--dangerously-bypass-hook-trust`
// CLI 플래그(codex 공식 옵션)를 기동 명령에 박으면 이 승인 절차를 우회할 수
// 있다**(`make_codex_command`/`reboot::resume_command` 가 항상 이 플래그를
// 붙인다). tasty 는 자기 hook 을 스스로 심으므로(hook source 를 스스로 vet함)
// 이 플래그의 정당한 사용 대상이다. `codex_hooks_all_trusted*` 는 이제 wait
// 경로가 아니라 `handle_install` 의 안내 문구(수동 승인 여부 표시)에만 쓰인다.
// 0.154 remote resume에서는 플래그가 있어도 검토 화면이 나타날 수 있다.

use std::path::{Path, PathBuf};

pub(crate) const HOOK_MARKER: &str = "tasty codex hook";

/// (camel for `[hooks.<Camel>]` table key, kebab for `tasty codex hook <kebab>` CLI
/// subcommand, snake for `[hooks.state."<path>:<snake>:0:0"]` trust state key).
///
/// 3 컬럼이 다른 케이스를 쓰는 이유: codex 가 같은 event 를 표면별로 다른 표기로
/// 인코딩한다. config table 키는 Rust enum variant 그대로 CamelCase, hook 명령에
/// 넘기는 우리 자체 event 이름은 kebab, codex 가 trust state 를 영속화할 때 쓰는
/// 키는 snake_case lowercase.
/// 매처(`matcher`)는 어느 항목에도 걸지 않는다 — 승인은 **어느 tool 에서도** 요청될 수
/// 있어 tool 이름으로 좁힐 근거가 없다(짝인 claude 는 `AskUserQuestion` 하나로 좁힌다).
/// 그 대가로 `PostToolUse` 는 tool 호출마다 hook 프로세스를 하나 띄운다.
pub(crate) const HOOK_EVENTS: &[(&str, &str, &str)] = &[
    ("Stop", "stop", "stop"),
    ("UserPromptSubmit", "prompt-submit", "user_prompt_submit"),
    ("SessionStart", "session-start", "session_start"),
    ("SessionEnd", "session-end", "session_end"),
    (
        "PermissionRequest",
        "permission-request",
        "permission_request",
    ),
    ("PostToolUse", "post-tool-use", "post_tool_use"),
    ("Interrupt", "interrupt", "interrupt"),
];

/// 경로만 계산한다 — 실패를 문구로 만들지 않으므로 `Translator` 가 없는 자리에서도
/// 쓸 수 있다.
pub(crate) fn config_toml_path_opt() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(home).join("config.toml"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".codex").join("config.toml"))
}

pub(crate) fn codex_config_toml_path(tr: &Translator) -> Result<PathBuf, IpcMethodError> {
    config_toml_path_opt().ok_or_else(|| IpcMethodError::new(tr.t("codex.install.no_home")))
}

fn read_config(path: &Path, tr: &Translator) -> Result<toml::Value, IpcMethodError> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|error| {
            IpcMethodError::new(tr.t_replace(
                "codex.install.read_failed",
                "{detail}",
                &error.to_string(),
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(toml::Value::Table(toml::map::Map::new()))
        }
        Err(error) => Err(IpcMethodError::new(tr.t_replace(
            "codex.install.read_failed",
            "{detail}",
            &error.to_string(),
        ))),
    }
}
fn selected_path(params: &Value, tr: &Translator) -> Result<PathBuf, IpcMethodError> {
    let file = params.get("config_file").filter(|v| !v.is_null());
    let home = params.get("codex_home").filter(|v| !v.is_null());
    if file.is_some() && home.is_some() {
        return Err(IpcMethodError::invalid_params(
            tr.t("codex.install.path_conflict"),
        ));
    }
    let invalid = || IpcMethodError::invalid_params(tr.t("codex.install.home_absolute"));
    if let Some(raw) = file.or(home) {
        let path = raw
            .as_str()
            .map(Path::new)
            .filter(|p| p.is_absolute())
            .ok_or_else(invalid)?;
        return Ok(if file.is_some() {
            path.to_path_buf()
        } else {
            path.join("config.toml")
        });
    }
    let path = codex_config_toml_path(tr)?;
    if !path.is_absolute() {
        return Err(invalid());
    }
    Ok(path)
}

pub(crate) fn write_toml(
    path: &Path,
    value: &toml::Value,
    tr: &Translator,
) -> Result<(), IpcMethodError> {
    let text = toml::to_string_pretty(value).map_err(|e| {
        IpcMethodError::new(tr.t_replace("codex.install.encode_failed", "{detail}", &e.to_string()))
    })?;
    std::fs::write(path, text).map_err(|e| {
        IpcMethodError::new(tr.t_replace("codex.install.write_failed", "{detail}", &e.to_string()))
    })
}

pub(crate) fn hook_command(event_kebab: &str) -> String {
    // TASTY_SURFACE_ID 가 비어있을 때 skip 하는 guard 포함. 가드 없으면 codex 가
    // 변수를 빈 문자열로 치환해 `tasty codex hook X --surface ` 가 실행되어
    // invalid_params 노이즈 발생.
    // CLI 의 host_call_failures 는 Tasty 내부 진단값이지 Codex hook 응답이 아니다.
    // CLI 가 로컬 실패 로그를 기록한 뒤 stdout 만 버리고 Codex 에는 {} 를 보낸다.
    // stderr 는 유지해 실제 전달 오류를 JSON 오류로 덮지 않는다.
    //
    // Windows: codex 는 hook 명령을 PowerShell 로 실행한다(실측 2026-07-12 —
    // 단일따옴표/`#` 주석이 PS 규칙으로 해석되고 순수 PS 구문 명령이 성공).
    // POSIX `[ -n ... ]` 가드는 PS 파서에서 항상 실패해 hook 이 한 번도 성공하지
    // 못하므로 PS 구문으로 발행한다. stdin 의 payload JSON 은 `$input` 으로 tasty
    // CLI 에 그대로 전달한다(session_id 추출용).
    #[cfg(windows)]
    {
        format!(
            "if ($env:TASTY_SURFACE_ID) {{ $input | tasty codex hook {event_kebab} --surface $env:TASTY_SURFACE_ID > $null; '{{}}' }}"
        )
    }
    // POSIX: 가드를 `if` 로 올려 "TASTY_SURFACE_ID 미설정" 과 "hook 명령 실패" 를
    // 분리한다. 옛 형태(`[ -n ... ] && ... || true`)는 둘을 한 `|| true` 로 함께
    // 삼켜서, 상태 push 가 유실돼도 아무 흔적이 남지 않았다. 안쪽 `|| true` 는
    // 이제 후자만 담당한다 — codex 턴을 방해하지 않기 위해 exit 0 은 유지하되,
    // 실패 자체는 CLI(`tasty_cli::hook_failure`)가 IPC 와 무관한 로컬 파일에
    // 기록한다. Windows(PS) 분기는 원래부터 `|| true` 가 없어 형태만 맞춘다.
    // `HOOK_MARKER`("tasty codex hook") substring 을 그대로 포함하므로 기존
    // entry 를 걷어내는 멱등 경로(`merge_install`)는 계속 발동한다.
    #[cfg(not(windows))]
    {
        format!(
            "if [ -n \"$TASTY_SURFACE_ID\" ]; then tasty codex hook {event_kebab} --surface $TASTY_SURFACE_ID > /dev/null || true; printf '{{}}\\n'; fi"
        )
    }
}

pub(crate) fn new_matcher_group(event_kebab: &str) -> toml::Value {
    let mut handler = toml::map::Map::new();
    handler.insert("type".into(), toml::Value::String("command".into()));
    handler.insert(
        "command".into(),
        toml::Value::String(hook_command(event_kebab)),
    );
    let mut group = toml::map::Map::new();
    group.insert(
        "hooks".into(),
        toml::Value::Array(vec![toml::Value::Table(handler)]),
    );
    toml::Value::Table(group)
}

/// 설치된 hook 그룹에 tasty 의 마커가 들어 있는가. `diagnose()` 가 사라진 뒤로
/// 산출물에는 소비자가 없고 설치/제거 시험만 쓴다 — 그래서 시험 빌드에만 둔다.
#[cfg(test)]
pub(crate) fn matcher_group_has_marker(item: &toml::Value, marker: &str) -> bool {
    let Some(group) = item.as_table() else {
        return false;
    };
    let Some(hooks) = group.get("hooks").and_then(|v| v.as_array()) else {
        return false;
    };
    hooks.iter().any(|h| {
        h.as_table()
            .and_then(|t| t.get("command"))
            .and_then(|c| c.as_str())
            .map(|s| s.contains(marker))
            .unwrap_or(false)
    })
}

/// Remove only our handlers, retaining a mixed group's matcher and user metadata.
fn remove_managed_handlers(groups: &mut Vec<toml::Value>) {
    groups.retain_mut(|group| {
        let Some(handlers) = group.get_mut("hooks").and_then(toml::Value::as_array_mut) else {
            return true;
        };
        let before = handlers.len();
        handlers.retain(|handler| {
            !handler
                .get("command")
                .and_then(toml::Value::as_str)
                .is_some_and(|command| command.contains(HOOK_MARKER))
        });
        handlers.len() == before || !handlers.is_empty()
    });
}

/// `[hooks]` 의 각 event 배열에 tasty MatcherGroup 을 멱등하게 박는다. 기존
/// non-tasty entry, 다른 키 (다른 hook event, [hooks] 외 섹션) 는 모두 보존.
pub(crate) fn merge_install(mut value: toml::Value) -> toml::Value {
    let Some(table) = value.as_table_mut() else {
        return value;
    };
    let hooks_table = table
        .entry("hooks".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let Some(hooks) = hooks_table.as_table_mut() else {
        return value;
    };
    for (event_key, kebab, _trust_snake) in HOOK_EVENTS {
        let event_array = hooks
            .entry((*event_key).to_string())
            .or_insert_with(|| toml::Value::Array(Vec::new()));
        let Some(arr) = event_array.as_array_mut() else {
            continue;
        };
        // 기존 tasty marker entry 제거 후 새 entry push — 멱등.
        remove_managed_handlers(arr);
        arr.push(new_matcher_group(kebab));
    }
    value
}

/// 우리가 install 한 3 개 hook 모두 trusted 상태인지 확인.
///
/// codex 는 user 가 `/hooks` 로 trust 한 hook 에 대해 `[hooks.state."<path>:<snake_event>:0:0"]`
/// 섹션에 `trusted_hash = "sha256:..."` 를 박는다. 우리 install entry 가 모두 그
/// 형식으로 등록되어있어야 hook 이 실제 fire 된다.
///
/// 주의: codex 는 부팅 시 stored hash 와 현재 hook command 의 fresh hash 를 비교해서
/// 다르면 invalidate 한다. 본 체크는 키 존재 + sha256: prefix 만 보므로, stale entry
/// 가 있고 codex 가 invalidate 한 케이스는 못 잡는다. 하지만 우리 install 은 멱등하고
/// `hook_command()` 가 static 이라 실제 stale 케이스는 사용자가 config.toml 을 직접
/// 편집한 경우 정도. 이 함수는 설치 안내의 metadata 판정일 뿐이다.
/// remote resume의 훅 검토 화면이나 daemon runtime 신뢰를 증명하지 않는다.
pub(crate) fn codex_hooks_all_trusted_in(value: &toml::Value, source_path: &str) -> bool {
    let Some(state_table) = value
        .get("hooks")
        .and_then(|v| v.get("state"))
        .and_then(|v| v.as_table())
    else {
        return false;
    };
    for (_, _, trust_snake) in HOOK_EVENTS {
        let key = format!("{source_path}:{trust_snake}:0:0");
        let trusted = state_table
            .get(&key)
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("trusted_hash"))
            .and_then(|h| h.as_str())
            .map(|s| s.starts_with("sha256:") && s.len() > "sha256:".len())
            .unwrap_or(false);
        if !trusted {
            return false;
        }
    }
    true
}

pub(crate) fn remove_install(mut value: toml::Value) -> toml::Value {
    let Some(table) = value.as_table_mut() else {
        return value;
    };
    let Some(hooks_table) = table.get_mut("hooks").and_then(|v| v.as_table_mut()) else {
        return value;
    };
    // 각 event 의 array 에서 tasty marker 가진 MatcherGroup 만 제거. `toml::map::Map`
    // 는 values_mut 가 없어 (&Map iter 만 지원) 키 목록을 떠서 우회.
    let event_keys: Vec<String> = hooks_table.keys().cloned().collect();
    for key in event_keys {
        if let Some(arr) = hooks_table.get_mut(&key).and_then(|v| v.as_array_mut()) {
            remove_managed_handlers(arr);
        }
    }
    // 빈 array 가 된 event 키 정리.
    hooks_table.retain(|_, v| !v.as_array().map(|a| a.is_empty()).unwrap_or(false));
    // [hooks] 가 텅 비면 제거.
    if hooks_table.is_empty() {
        table.remove("hooks");
    }
    value
}

#[cfg(test)]
mod path_tests {
    use super::*;
    #[test]
    fn malformed_explicit_context_never_falls_back_to_default_config() {
        let tr = Translator::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("lang"), "en");
        for params in [
            json!({"codex_home":42}),
            json!({"config_file":false}),
            json!({"codex_home":"relative"}),
        ] {
            assert!(selected_path(&params, &tr).is_err());
        }
        // 이유: 경로 문자열로만 쓴다 — 이 자리는 아무것도 만들지도 지우지도 않고,
        //       `selected_path` 는 디렉터리의 실재를 보지 않는 순수 함수다. 필요한 성질은
        //       "절대 경로 하나" 뿐이라 동시에 도는 완주끼리 공유해도 서로를 안 건드린다.
        let dir = std::env::temp_dir();
        assert_eq!(
            selected_path(&json!({"codex_home":dir,"config_file":null}), &tr).unwrap(),
            dir.join("config.toml")
        );
    }
}
