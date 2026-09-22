//! `dynamic` 파이프라인 전체의 회귀. 픽스처(`sample_entry` 등)를 세 모듈이
//! 공유하므로 테스트는 모듈별로 쪼개지 않고 여기 모아 둔다 — 쪼개면 같은
//! 픽스처가 세 벌이 된다.

use super::build::*;
use super::request::*;
use super::stdin::*;
use super::*;
use clap::{ArgMatches, CommandFactory};
use std::collections::HashMap;
use tasty_plugin_manifest::{AutoWaitDecl, CompletionStrategyDecl};
use tasty_plugin_manifest::{CliArg, CliArgGroup, CliArgType, CliSubcommandDecl};

fn new_subcommand(name: &str, ipc_method: &str, args: &str) -> CliSubcommandDecl {
    CliSubcommandDecl {
        name: name.into(),
        ipc_method: ipc_method.into(),
        args: args.into(),
        description: None,
        description_i18n_key: None,
        stdin_json: false,
        polling: None,
        auto_wait: None,
    }
}

fn sample_entry() -> PluginCliEntry {
    let mut arg_groups: HashMap<String, CliArgGroup> = HashMap::new();
    arg_groups.insert(
        "spawn_args".into(),
        CliArgGroup {
            positional: vec![],
            flags: vec![
                CliArg {
                    name: "surface".into(),
                    ty: CliArgType::U32,
                    flag: Some("--surface".into()),
                    required: false,
                    default: None,
                    help: None,
                    help_i18n_key: None,
                    stdin_field: None,
                    path_kind: None,
                    reject_repeat: false,
                },
                CliArg {
                    name: "prompt".into(),
                    ty: CliArgType::String,
                    flag: Some("--prompt".into()),
                    required: false,
                    default: None,
                    help: None,
                    help_i18n_key: None,
                    stdin_field: None,
                    path_kind: None,
                    reject_repeat: false,
                },
                CliArg {
                    name: "force".into(),
                    ty: CliArgType::Bool,
                    flag: Some("--force".into()),
                    required: false,
                    default: None,
                    help: None,
                    help_i18n_key: None,
                    stdin_field: None,
                    path_kind: None,
                    reject_repeat: false,
                },
            ],
        },
    );
    arg_groups.insert(
        "broadcast_args".into(),
        CliArgGroup {
            positional: vec![CliArg {
                name: "text".into(),
                ty: CliArgType::String,
                flag: None,
                required: true,
                default: None,
                help: None,
                help_i18n_key: None,
                stdin_field: None,
                path_kind: None,
                reject_repeat: false,
            }],
            flags: vec![CliArg {
                name: "timeout".into(),
                ty: CliArgType::U32,
                flag: Some("--timeout".into()),
                required: false,
                default: Some(toml::Value::Integer(60)),
                help: None,
                help_i18n_key: None,
                stdin_field: None,
                path_kind: None,
                reject_repeat: false,
            }],
        },
    );
    PluginCliEntry {
        cli: CliCommandDecl {
            name: "codex".into(),
            description: None,
            description_i18n_key: None,
            subcommands: vec![
                new_subcommand("spawn", "codex.spawn", "spawn_args"),
                new_subcommand("broadcast", "codex.broadcast", "broadcast_args"),
            ],
            arg_groups,
        },
    }
}

fn parse(args: &[&str]) -> ArgMatches {
    let entry = sample_entry();
    let augmented = build_augmented_cli(&[entry]);
    augmented
        .try_get_matches_from(std::iter::once("tasty").chain(args.iter().copied()))
        .expect("parse")
}

#[test]
fn merge_stdin_uses_stdin_field_alias() {
    // stdin JSON 의 키 이름이 CLI arg name 과 다른 경우 (`session_id` →
    // `session`) stdin_field 매핑이 적용되는지 확인. Claude Code hook payload
    // 의 session_id 가 `--session` 인자로 들어오는 동작이 이걸로 보장된다.
    let group = CliArgGroup {
        positional: vec![],
        flags: vec![
            CliArg {
                name: "session".into(),
                ty: CliArgType::String,
                flag: Some("--session".into()),
                required: false,
                default: None,
                help: None,
                help_i18n_key: None,
                stdin_field: Some("session_id".into()),
                path_kind: None,
                reject_repeat: false,
            },
            CliArg {
                name: "message".into(),
                ty: CliArgType::String,
                flag: Some("--message".into()),
                required: false,
                default: None,
                help: None,
                help_i18n_key: None,
                stdin_field: None,
                path_kind: None,
                reject_repeat: false,
            },
        ],
    };
    let stdin = serde_json::json!({
        "session_id": "abc-123",
        "message": "hi",
        "irrelevant": 42
    });
    let mut params = Map::new();
    merge_stdin_params(&mut params, &group, &stdin).expect("이 회차의 stdin 값은 선언 타입과 맞다");
    assert_eq!(params["session"], Value::String("abc-123".into()));
    assert_eq!(params["message"], Value::String("hi".into()));
    // CLI arg 에 없는 stdin 키는 params 에 들어오지 않는다.
    assert!(!params.contains_key("irrelevant"));
}

#[test]
fn merge_stdin_does_not_override_cli_explicit() {
    // CLI 로 명시된 값이 stdin 보다 우선.
    let group = CliArgGroup {
        positional: vec![],
        flags: vec![CliArg {
            name: "session".into(),
            ty: CliArgType::String,
            flag: Some("--session".into()),
            required: false,
            default: None,
            help: None,
            help_i18n_key: None,
            stdin_field: Some("session_id".into()),
            path_kind: None,
            reject_repeat: false,
        }],
    };
    let stdin = serde_json::json!({ "session_id": "from-stdin" });
    let mut params = Map::new();
    params.insert("session".into(), Value::String("from-cli".into()));
    merge_stdin_params(&mut params, &group, &stdin).expect("이 회차의 stdin 값은 선언 타입과 맞다");
    assert_eq!(params["session"], Value::String("from-cli".into()));
}

#[test]
fn merge_stdin_ignores_null_fields() {
    // stdin JSON 에 키가 있어도 값이 null 이면 params 에 넣지 않는다.
    let group = CliArgGroup {
        positional: vec![],
        flags: vec![CliArg {
            name: "session".into(),
            ty: CliArgType::String,
            flag: Some("--session".into()),
            required: false,
            default: None,
            help: None,
            help_i18n_key: None,
            stdin_field: Some("session_id".into()),
            path_kind: None,
            reject_repeat: false,
        }],
    };
    let stdin = serde_json::json!({ "session_id": null });
    let mut params = Map::new();
    merge_stdin_params(&mut params, &group, &stdin).expect("이 회차의 stdin 값은 선언 타입과 맞다");
    assert!(!params.contains_key("session"));
}

#[test]
fn flag_with_value_maps_to_params() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--surface", "5", "--prompt", "hello"]);
    let (req, _polling, _auto) = matches_to_request(&entries, &m).unwrap();
    assert_eq!(req.method, "codex.spawn");
    let p = req.params.as_object().unwrap();
    assert_eq!(p["surface"], Value::from(5_u32));
    assert_eq!(p["prompt"], Value::String("hello".into()));
}

/// 숫자 플래그에 비수치가 오면 **거부**한다.
///
/// 예전에는 `parse().ok()` 로 `None` 이 되어 하류에서 "플래그 없음" 과 같아졌고,
/// 그 자리에 없을 때 도는 기본값이 들어갔다. `--surface` 의 기본값은 호출자 자신이라
/// 명령이 **자기에게 배달**됐다 — 종료코드 0, 오류 없음. 실제로 그렇게 잃은 적이 있다.
/// stdin JSON 경로는 `extract_value` 를 지나지 않는다. 강제를 한쪽 문에만 두면
/// 같은 `CliArg` 선언이 들어온 문에 따라 다른 뜻이 된다 — 그 비대칭을 고정한다.
fn spawn_group(entry: &PluginCliEntry) -> &CliArgGroup {
    entry
        .cli
        .arg_groups
        .get("spawn_args")
        .expect("전제: sample_entry 에 spawn_args 가 있다")
}

#[test]
fn stdin_json_number_flag_takes_a_number_and_a_numeric_string() {
    tasty_i18n::init("en");
    let entry = sample_entry();
    let g = spawn_group(&entry);

    let mut params = Map::new();
    let stdin = serde_json::json!({ "surface": 42 });
    merge_stdin_params(&mut params, g, &stdin).expect("숫자는 통과해야 한다");
    assert_eq!(params.get("surface"), Some(&Value::from(42u32)));

    // 문자열이라도 숫자로 읽히면 `--surface 42` 와 같게 다룬다 — 두 문의 규칙이
    // 달라지면 그 자체가 다음 오보의 자리가 된다.
    let mut params = Map::new();
    let stdin = serde_json::json!({ "surface": "42" });
    merge_stdin_params(&mut params, g, &stdin).expect("숫자 문자열도 통과해야 한다");
    assert_eq!(params.get("surface"), Some(&Value::from(42u32)));
}

#[test]
fn stdin_json_non_numeric_value_for_a_number_flag_is_rejected() {
    tasty_i18n::init("en");
    let entry = sample_entry();
    let g = spawn_group(&entry);

    for bad in [
        serde_json::json!({ "surface": "conductor" }),
        serde_json::json!({ "surface": 1.5 }),
        serde_json::json!({ "surface": true }),
        serde_json::json!({ "surface": { "id": 1 } }),
    ] {
        let mut params = Map::new();
        let err = merge_stdin_params(&mut params, g, &bad)
            .expect_err("비수치 stdin 값은 오류여야 한다: {bad}");
        let msg = err.to_string();
        assert!(msg.contains("surface"), "어느 인자인지 담아야 한다: {msg}");
        assert!(
            params.get("surface").is_none(),
            "거부된 값이 params 에 남으면 안 된다"
        );
    }
}

#[test]
fn stdin_json_does_not_override_a_value_the_cli_already_gave() {
    tasty_i18n::init("en");
    let entry = sample_entry();
    let g = spawn_group(&entry);

    // CLI 가 이미 채운 키는 stdin 이 무엇을 싣고 오든 건드리지 않는다. 그래서
    // 그 값이 비수치여도 여기서는 오류가 나지 않는다 — `--flag` 경로가 이미
    // 검사한 뒤이기 때문이다(같은 값을 두 번 판정하지 않는다).
    let mut params = Map::new();
    params.insert("surface".into(), Value::from(7u32));
    params.insert("prompt".into(), Value::String("hi".into()));
    let stdin = serde_json::json!({ "surface": "conductor", "prompt": "bye" });
    merge_stdin_params(&mut params, g, &stdin).expect("CLI 값이 우선이라 통과한다");
    assert_eq!(params.get("surface"), Some(&Value::from(7u32)));
    assert_eq!(params.get("prompt"), Some(&Value::String("hi".into())));
}

#[test]
fn stdin_json_string_and_bool_args_pass_through_unchanged() {
    tasty_i18n::init("en");
    let entry = sample_entry();
    let g = spawn_group(&entry);

    let mut params = Map::new();
    let stdin = serde_json::json!({ "prompt": "hi", "force": true });
    merge_stdin_params(&mut params, g, &stdin).expect("문자열·불리언은 그대로");
    assert_eq!(params.get("prompt"), Some(&Value::String("hi".into())));
    assert_eq!(params.get("force"), Some(&Value::Bool(true)));
}

#[test]
fn non_numeric_value_for_a_number_flag_is_rejected_not_dropped() {
    // `tasty_i18n::init` 은 프로세스당 1 회 `OnceLock` 이고, 이 바이너리의 다른
    // 테스트(`run.rs`)도 "en" 으로 초기화한다. 여기서 먼저 부르는 것은 값을 바꾸는
    // 것이 아니라 **순서 경합을 없애는 것**이다 — 부르지 않으면 언어팩 로드 여부가
    // 스레드 순서에 달려 메시지가 키(미로드)와 영문(로드) 사이에서 흔들린다.
    tasty_i18n::init("en");
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--surface", "conductor", "--prompt", "hi"]);
    let err = matches_to_request(&entries, &m).expect_err("비수치 --surface 는 오류여야 한다");
    let msg = err.to_string();
    assert_ne!(
        msg, "cli.plugin_cli.flag_not_a_number",
        "번역 키가 그대로 새어 나오면 안 된다"
    );
    assert!(
        msg.contains("surface"),
        "어느 플래그인지 담아야 한다: {msg}"
    );
    assert!(msg.contains("conductor"), "받은 값을 담아야 한다: {msg}");
}

/// **숫자인데 범위 밖**인 것은 "숫자가 아니다" 와 다른 문구여야 한다.
///
/// 한 문구로 답하면 `4294967297` 을 준 사용자가 자기 오타를 찾으러 간다 — 실제로는
/// 값이 크기만 한 것이고 고칠 방법이 다르다. 실측으로 이 자리에서 두 경우가 같은
/// 문구를 받고 있었다.
#[test]
fn an_out_of_range_number_is_not_reported_as_a_non_number() {
    tasty_i18n::init("en");
    let entries = vec![sample_entry()];

    let over = format!("{}", u64::from(u32::MAX) + 2);
    let m = parse(&["codex", "spawn", "--surface", &over, "--prompt", "hi"]);
    let msg = matches_to_request(&entries, &m)
        .expect_err("u32 범위 밖 --surface 는 오류여야 한다")
        .to_string();
    assert!(msg.contains(&over), "받은 값을 담아야 한다: {msg}");
    assert!(msg.contains("range"), "범위 문제라고 말해야 한다: {msg}");
    assert!(
        !msg.contains("not a number"),
        "숫자인데 숫자가 아니라고 답한다: {msg}"
    );

    // 대우 — 진짜 숫자가 아닌 것은 종전 문구 그대로다.
    let m = parse(&["codex", "spawn", "--surface", "conductor", "--prompt", "hi"]);
    let msg = matches_to_request(&entries, &m)
        .expect_err("비수치는 오류")
        .to_string();
    assert!(msg.contains("not a number"), "{msg}");
}

/// 위 테스트의 대우 — 플래그가 **아예 없는** 것은 여전히 오류가 아니다.
/// 둘을 가르지 못하는 것이 원래 결함이었으므로 양쪽을 함께 박는다.
#[test]
fn an_absent_number_flag_is_still_not_an_error() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--prompt", "hi"]);
    let (req, _polling, _auto) =
        matches_to_request(&entries, &m).expect("없는 플래그는 오류가 아니다");
    assert_eq!(
        req.params.as_object().unwrap()["prompt"],
        Value::String("hi".into())
    );
}

#[test]
fn bool_flag_present_serializes_true() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--force"]);
    let (req, _polling, _auto) = matches_to_request(&entries, &m).unwrap();
    let p = req.params.as_object().unwrap();
    assert_eq!(p["force"], Value::Bool(true));
}

/// 정적 명령 이름을 하나씩 다 흉내 내 본다 — 손목록에 우연히 들어 있는 이름
/// 몇 개가 아니라 **실제 명령 집합 전체**가 대상이다. 어느 하나라도 등록을
/// 통과하면 release 에서는 도달 불가능한 중복이 얹히고 debug 에서는 clap 의
/// `assert_app` 이 CLI 전체를 패닉시킨다.
#[test]
fn no_host_command_name_can_be_shadowed_by_a_plugin() {
    let host = host_command_names(&<crate::Cli as CommandFactory>::command());
    assert!(
        host.len() > 20,
        "정적 명령 집합이 {} 개다 — 도출이 깨졌으면 이 테스트는 아무것도 재지 않는다",
        host.len()
    );
    for name in &host {
        let mut entry = sample_entry();
        entry.cli.name = name.clone();
        let augmented = build_augmented_cli(&[entry]);
        let hits = augmented
            .get_subcommands()
            .filter(|c| c.get_name() == name)
            .count();
        assert_eq!(hits, 1, "'{name}' 이 중복 등록됐다");
        // clap 이 debug 빌드에서 실제로 패닉하던 그 경로를 직접 밟는다.
        augmented.debug_assert();
    }
}

/// 겹치지 않는 이름은 그대로 등록된다 — 위 필터가 전부를 막아 버리는
/// 형태였다면 이쪽이 빨개진다.
#[test]
fn a_plugin_name_that_does_not_collide_is_still_registered() {
    let entry = sample_entry();
    let name = entry.cli.name.clone();
    let host = host_command_names(&<crate::Cli as CommandFactory>::command());
    assert!(
        !host.contains(&name),
        "표본이 이미 호스트 명령이면 대조가 안 된다"
    );
    let augmented = build_augmented_cli(&[entry]);
    assert_eq!(
        augmented
            .get_subcommands()
            .filter(|c| c.get_name() == name)
            .count(),
        1
    );
}

#[test]
fn bool_flag_absent_serializes_false() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn"]);
    let (req, _polling, _auto) = matches_to_request(&entries, &m).unwrap();
    let p = req.params.as_object().unwrap();
    assert_eq!(p["force"], Value::Bool(false));
}

#[test]
fn default_value_applied_when_missing() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "broadcast", "hello"]);
    let (req, _polling, _auto) = matches_to_request(&entries, &m).unwrap();
    let p = req.params.as_object().unwrap();
    assert_eq!(p["text"], Value::String("hello".into()));
    assert_eq!(p["timeout"], Value::from(60_u32));
}

#[test]
fn positional_required() {
    let entries = vec![sample_entry()];
    let augmented = build_augmented_cli(&entries);
    let err = augmented.try_get_matches_from(["tasty", "codex", "broadcast"]);
    assert!(err.is_err(), "missing required positional should error");
}

#[test]
fn unknown_top_level_subcommand_errors() {
    let entries = vec![sample_entry()];
    let augmented = build_augmented_cli(&entries);
    let res = augmented.try_get_matches_from(["tasty", "nonexistent", "spawn"]);
    assert!(res.is_err());
}

fn sample_auto_wait_decl() -> AutoWaitDecl {
    let mut map_from_response = HashMap::new();
    map_from_response.insert("child_surface_id".into(), "surface_id".into());
    let mut map_from_request = HashMap::new();
    map_from_request.insert("surface".into(), "surface".into());
    AutoWaitDecl {
        method: "claude.wait_by_surface".into(),
        map_from_response,
        map_from_request,
        polling: Some(PollingDecl {
            state_field: "state".into(),
            terminal_states: vec!["idle".into(), "exited".into()],
            interval_ms: 100,
            timeout_field: Some("timeout".into()),
        }),
        strategy: None,
        no_wait_field: "no_wait".into(),
        timeout_field: "timeout".into(),
    }
}

fn empty_strategies() -> HashMap<String, CompletionStrategyDecl> {
    HashMap::new()
}

#[test]
fn auto_wait_skipped_when_no_wait_flag() {
    // --no-wait 가 params 에 true 로 들어오면 AutoWaitPlan.skipped = true.
    let aw = sample_auto_wait_decl();
    let mut params = Map::new();
    params.insert("no_wait".into(), Value::Bool(true));
    params.insert("surface".into(), Value::from(7_u32));
    let plan = build_auto_wait_plan(&aw, &params, &empty_strategies()).unwrap();
    assert!(plan.skipped, "no_wait=true should skip chain");
    assert_eq!(plan.method, "claude.wait_by_surface");
}

#[test]
fn auto_wait_not_skipped_when_no_wait_absent_or_false() {
    // no_wait 키 부재 / false 면 chain 진행.
    let aw = sample_auto_wait_decl();
    let mut params = Map::new();
    params.insert("surface".into(), Value::from(7_u32));
    let plan = build_auto_wait_plan(&aw, &params, &empty_strategies()).unwrap();
    assert!(!plan.skipped);

    let mut params2 = Map::new();
    params2.insert("no_wait".into(), Value::Bool(false));
    let plan2 = build_auto_wait_plan(&aw, &params2, &empty_strategies()).unwrap();
    assert!(!plan2.skipped);
}

#[test]
fn auto_wait_plan_snapshots_request_params() {
    // build_auto_wait_plan 은 1 차 요청 params 를 그대로 snapshot 해 둔다 —
    // 나중에 build_wait_params 가 map_from_request 매핑에 사용.
    let aw = sample_auto_wait_decl();
    let mut params = Map::new();
    params.insert("surface".into(), Value::from(42_u32));
    params.insert("prompt".into(), Value::String("hi".into()));
    let plan = build_auto_wait_plan(&aw, &params, &empty_strategies()).unwrap();
    assert_eq!(
        plan.request_params.get("surface"),
        Some(&Value::from(42_u32))
    );
    assert_eq!(
        plan.request_params.get("prompt"),
        Some(&Value::String("hi".into()))
    );
    // map_from_response / map_from_request 는 그대로 복사.
    assert_eq!(
        plan.map_from_response.get("child_surface_id"),
        Some(&"surface_id".into())
    );
    assert_eq!(
        plan.map_from_request.get("surface"),
        Some(&"surface".into())
    );
}

#[test]
fn auto_wait_plan_carries_polling_and_timeout_field() {
    // polling 사양 + timeout_field 가 그대로 plan 에 전파되는지.
    let aw = sample_auto_wait_decl();
    let plan = build_auto_wait_plan(&aw, &Map::new(), &empty_strategies()).unwrap();
    assert_eq!(plan.polling.state_field, "state");
    assert_eq!(plan.polling.terminal_states, vec!["idle", "exited"]);
    assert_eq!(plan.polling.interval_ms, 100);
    assert_eq!(plan.timeout_field, "timeout");
}

#[test]
fn auto_wait_custom_no_wait_field_name() {
    // manifest 가 `no_wait_field` 를 커스텀으로 지정한 경우 그 키를 본다.
    let mut aw = sample_auto_wait_decl();
    aw.no_wait_field = "skip_chain".into();
    let mut params = Map::new();
    params.insert("skip_chain".into(), Value::Bool(true));
    // 표준 "no_wait" 키는 true 가 아니므로 만약 잘못 보면 skipped=false.
    let plan = build_auto_wait_plan(&aw, &params, &empty_strategies()).unwrap();
    assert!(
        plan.skipped,
        "custom no_wait_field='skip_chain' should be honored"
    );
}

fn sample_auto_wait_decl_with_strategy(strategy: &str) -> AutoWaitDecl {
    let mut aw = sample_auto_wait_decl();
    aw.polling = None;
    aw.strategy = Some(strategy.into());
    aw
}

#[test]
fn resolve_auto_wait_polling_finds_registered_strategy() {
    let aw = sample_auto_wait_decl_with_strategy("com.example.x/wait-ready");
    let decl: CompletionStrategyDecl = toml::from_str(
        r#"
            poll_method = "ex.wait"
            state_field = "state"
            terminal_states = ["idle"]
            interval_ms = 250
        "#,
    )
    .unwrap();
    let mut strategies = HashMap::new();
    strategies.insert("com.example.x/wait-ready".to_string(), decl);
    let plan = build_auto_wait_plan(&aw, &Map::new(), &strategies).unwrap();
    assert_eq!(plan.polling.state_field, "state");
    assert_eq!(plan.polling.terminal_states, vec!["idle"]);
    assert_eq!(plan.polling.interval_ms, 250);
    assert_eq!(
        plan.polling.timeout_field, None,
        "named-strategy resolution does not carry a CLI --timeout override"
    );
}

#[test]
fn resolve_auto_wait_polling_errors_on_unknown_strategy() {
    // 에러 본문은 i18n 키를 거친다 — en 테이블을 올려 실제 문구(= 키 존재)로 검사한다.
    tasty_i18n::init("en");
    let aw = sample_auto_wait_decl_with_strategy("com.example.x/wait-ready");
    let err = build_auto_wait_plan(&aw, &Map::new(), &empty_strategies())
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown strategy"), "got: {err}");
}

#[test]
fn discover_skips_invalid_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plugin_a = dir.path().join("a");
    std::fs::create_dir_all(&plugin_a).unwrap();
    std::fs::write(
        plugin_a.join("tasty-plugin.toml"),
        r#"
manifest_version = 1
id = "com.example.a"
name = "A"
version = "0.1.0"
api_version = "1"

[entry]
type = "process"
command = "x"

[[contributes.ipc_namespace]]
prefix = "a"

[[contributes.cli]]
name = "a"
subcommands = [
  { name = "ping", ipc_method = "a.ping", args = "empty" },
]

[contributes.cli.arg_groups.empty]
"#,
    )
    .unwrap();

    let plugin_bad = dir.path().join("bad");
    std::fs::create_dir_all(&plugin_bad).unwrap();
    std::fs::write(plugin_bad.join("tasty-plugin.toml"), "not toml at all = {").unwrap();

    let entries = discover_plugin_clis(dir.path());
    let names: Vec<&str> = entries.iter().map(|e| e.cli.name.as_str()).collect();
    assert_eq!(names, vec!["a"]);
}

/// 실제 claude 매니페스트의 `hook_args` 로 StopFailure payload 를 병합한다. 픽스처를
/// 손으로 짓지 않는 이유: 물음이 "`stdin_field` 가 동작하나" 가 아니라 "**그 매니페스트가**
/// `error` 를 받을 자리를 선언했나" 이기 때문이다 — 선언이 빠지면 stdin 의 `error` 는
/// params 에 실리지 않고, plugin 은 에러 종류를 모른 채 `unknown` 을 기록한다.
#[test]
fn claude_hook_args_carry_the_stop_failure_error_from_stdin() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tasty-plugin-claude");
    let manifest =
        tasty_plugin_manifest::Manifest::load(&dir).expect("claude manifest should load");
    let cli = manifest
        .contributes
        .cli
        .iter()
        .find(|c| c.name == "claude")
        .expect("claude cli decl");
    let group = cli.arg_groups.get("hook_args").expect("hook_args group");

    let mut params = Map::new();
    params.insert("event".into(), Value::String("stop-failure".into()));
    let stdin = serde_json::json!({
        "session_id": "s",
        "hook_event_name": "StopFailure",
        "error": "overloaded",
        "agent_id": "sub-1",
    });
    merge_stdin_params(&mut params, group, &stdin).expect("string fields pass through");

    assert_eq!(
        params.get("error"),
        Some(&Value::String("overloaded".into()))
    );
    assert_eq!(params.get("session"), Some(&Value::String("s".into())));
    // 서브에이전트 실패를 메인 턴 종료와 가르는 필드 — 빠지면 plugin 이 둘을 구별 못 한다.
    assert_eq!(params.get("agent_id"), Some(&Value::String("sub-1".into())));
}
