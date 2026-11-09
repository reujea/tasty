//! 공통 매니페스트 픽스처로 동적 명령 구성과 요청 변환을 검사한다.

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
        internal: false,
        polling: None,
        auto_wait: None,
        output: None,
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
            help_group: None,
            subcommands: vec![
                new_subcommand("spawn", "codex.spawn", "spawn_args"),
                new_subcommand("broadcast", "codex.broadcast", "broadcast_args"),
            ],
            arg_groups,
        },
        strict_caller_contexts: Vec::new(),
        remote_methods: Vec::new(),
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
fn explicit_port_file_in_augmented_cli_is_a_string_and_survives_parsing() {
    let matches = parse(&["--port-file", "/isolated path/tasty.port", "codex", "spawn"]);
    assert_eq!(
        matches.get_one::<String>("port_file").map(String::as_str),
        Some("/isolated path/tasty.port")
    );
}

#[test]
fn merge_stdin_uses_stdin_field_alias() {
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
    let (req, _polling, _auto) = matches_to_request(&entries, &[], &m, &mut |_| false).unwrap();
    assert_eq!(req.method, "codex.spawn");
    let p = req.params.as_object().unwrap();
    assert_eq!(p["surface"], Value::from(5_u32));
    assert_eq!(p["prompt"], Value::String("hello".into()));
}

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

    // 명시한 CLI 값은 이미 검사됐으며 stdin보다 우선한다.
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
    // 이유: 다른 시험과 같은 OnceLock 초기값을 사용해 번역 로드 순서에 의존하지 않는다.
    tasty_i18n::init("en");
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--surface", "conductor", "--prompt", "hi"]);
    let err = matches_to_request(&entries, &[], &m, &mut |_| false)
        .expect_err("비수치 --surface 는 오류여야 한다");
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

#[test]
fn an_out_of_range_number_is_not_reported_as_a_non_number() {
    tasty_i18n::init("en");
    let entries = vec![sample_entry()];

    let over = format!("{}", u64::from(u32::MAX) + 2);
    let m = parse(&["codex", "spawn", "--surface", &over, "--prompt", "hi"]);
    let msg = matches_to_request(&entries, &[], &m, &mut |_| false)
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
    let msg = matches_to_request(&entries, &[], &m, &mut |_| false)
        .expect_err("비수치는 오류")
        .to_string();
    assert!(msg.contains("not a number"), "{msg}");
}

#[test]
fn an_absent_number_flag_is_still_not_an_error() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--prompt", "hi"]);
    let (req, _polling, _auto) =
        matches_to_request(&entries, &[], &m, &mut |_| false).expect("없는 플래그는 오류가 아니다");
    assert_eq!(
        req.params.as_object().unwrap()["prompt"],
        Value::String("hi".into())
    );
}

#[test]
fn bool_flag_present_serializes_true() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "spawn", "--force"]);
    let (req, _polling, _auto) = matches_to_request(&entries, &[], &m, &mut |_| false).unwrap();
    let p = req.params.as_object().unwrap();
    assert_eq!(p["force"], Value::Bool(true));
}

/// 호스트 명령 전체에 대해 중복 플러그인 등록을 거절하는지 확인한다.
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
        // help는 clap이 build 때 만드는 예약 이름이라 build 전 트리에는 아직 없다.
        let expected = usize::from(name != "help");
        assert_eq!(hits, expected, "'{name}' 이 중복 등록됐다");
        augmented.debug_assert();
    }
}

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
    let (req, _polling, _auto) = matches_to_request(&entries, &[], &m, &mut |_| false).unwrap();
    let p = req.params.as_object().unwrap();
    assert_eq!(p["force"], Value::Bool(false));
}

#[test]
fn default_value_applied_when_missing() {
    let entries = vec![sample_entry()];
    let m = parse(&["codex", "broadcast", "hello"]);
    let (req, _polling, _auto) = matches_to_request(&entries, &[], &m, &mut |_| false).unwrap();
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

/// 실제 매니페스트가 실패 종류를 받을 stdin_field를 선언했는지 확인한다.
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

/// Stop payload 의 배열·bool 필드가 모양을 바꾸지 않고 plugin 인자로 넘어가는지 확인한다.
/// plugin 은 background_tasks 배열의 항목 status 로 백그라운드 대기 Stop 을 가린다.
#[test]
fn claude_hook_args_carry_background_work_fields_from_stdin_unchanged() {
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

    // Claude Code 2.1.283 이 백그라운드 셸을 남긴 채 보낸 Stop payload 의 모양.
    let background = serde_json::json!([{
        "id": "bsav6ob35",
        "type": "shell",
        "status": "running",
        "description": "sleep 15",
        "command": "sleep 15; echo done-bg",
    }]);
    let mut params = Map::new();
    params.insert("event".into(), Value::String("stop".into()));
    let stdin = serde_json::json!({
        "session_id": "s",
        "hook_event_name": "Stop",
        "stop_hook_active": false,
        "background_tasks": background,
        "waiting_on_background_work": true,
    });
    merge_stdin_params(&mut params, group, &stdin).expect("array and bool pass through");

    assert_eq!(params.get("background_tasks"), Some(&background));
    assert_eq!(
        params.get("waiting_on_background_work"),
        Some(&Value::Bool(true))
    );

    // 필드가 없는 옛 payload 에서는 인자도 생기지 않는다.
    let mut params = Map::new();
    merge_stdin_params(
        &mut params,
        group,
        &serde_json::json!({ "session_id": "s" }),
    )
    .expect("no background fields");
    assert!(!params.contains_key("background_tasks"));
    assert!(!params.contains_key("waiting_on_background_work"));
}

/// 내부 연동 하위 명령은 마지막 그룹에 따로 표시되며 실행 트리에는 그대로 남는다.
#[test]
fn internal_subcommands_are_listed_last_and_stay_runnable() {
    let mut entry = sample_entry();
    entry.cli.subcommands[1].internal = true;
    entry.cli.help_group = Some("agents".into());
    let mut cmd = build_augmented_cli(std::slice::from_ref(&entry));
    let root = cmd.render_help().to_string();
    let agents = tasty_i18n::t("cli.help_group.agents");
    let section = root
        .split(agents)
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    assert!(section.contains("codex"), "{root}");
    let top = cmd.find_subcommand_mut("codex").unwrap();
    let help = top.render_help().to_string();
    let internal = tasty_i18n::t("cli.help_group.internal");
    let (normal, tail) = help.split_once(internal).expect(&help);
    assert!(
        normal.contains("spawn") && !normal.contains("broadcast"),
        "{help}"
    );
    assert!(tail.contains("broadcast"), "{help}");
    let matches = cmd
        .try_get_matches_from(["tasty", "codex", "broadcast", "hi"])
        .map(|m| m.subcommand_name().map(str::to_string));
    assert_eq!(matches.unwrap().as_deref(), Some("codex"));
}

fn path_entry() -> PluginCliEntry {
    let arg = |name: &str, ty, path_kind: Option<&str>| CliArg {
        name: name.into(),
        ty,
        flag: Some(format!("--{name}")),
        required: false,
        default: None,
        help: None,
        help_i18n_key: None,
        stdin_field: None,
        path_kind: path_kind.map(str::to_owned),
        reject_repeat: false,
    };
    let mut arg_groups: HashMap<String, CliArgGroup> = HashMap::new();
    arg_groups.insert(
        "place_args".into(),
        CliArgGroup {
            positional: vec![],
            flags: vec![
                arg("workspace", CliArgType::String, None),
                arg("cwd", CliArgType::String, Some("directory")),
            ],
        },
    );
    PluginCliEntry {
        cli: CliCommandDecl {
            name: "pathy".into(),
            description: None,
            description_i18n_key: None,
            help_group: None,
            subcommands: vec![
                new_subcommand("spawn", "pathy.spawn", "place_args"),
                new_subcommand("launch", "pathy.launch", "place_args"),
            ],
            arg_groups,
        },
        strict_caller_contexts: Vec::new(),
        remote_methods: vec!["pathy.spawn".into()],
    }
}

fn path_request(args: &[&str], remote: bool) -> (anyhow::Result<Value>, Vec<Map<String, Value>>) {
    let entries = vec![path_entry()];
    let m = build_augmented_cli(&entries)
        .try_get_matches_from(std::iter::once("tasty").chain(args.iter().copied()))
        .expect("parse");
    let mut asked = Vec::new();
    let result = matches_to_request(&entries, &[], &m, &mut |q| {
        asked.push(q.clone());
        remote
    })
    .map(|(req, _, _)| req.params);
    (result, asked)
}

#[test]
fn a_path_for_an_attach_mirror_is_sent_unchanged_for_the_server_to_check() {
    let (params, asked) = path_request(
        &[
            "pathy",
            "spawn",
            "--workspace",
            "7",
            "--cwd",
            "/only/on/the/server",
        ],
        true,
    );
    assert_eq!(params.unwrap()["cwd"], "/only/on/the/server");
    assert_eq!(
        asked,
        vec![
            serde_json::json!({ "workspace": 7 })
                .as_object()
                .unwrap()
                .clone()
        ]
    );
}

#[test]
fn a_path_for_a_local_target_is_still_resolved_and_checked_here() {
    let (params, _) = path_request(
        &[
            "pathy",
            "spawn",
            "--workspace",
            "7",
            "--cwd",
            "/only/on/the/server",
        ],
        false,
    );
    assert!(params.is_err(), "a missing local directory is refused");
    let here = std::env::current_dir().unwrap();
    let (params, _) = path_request(&["pathy", "spawn", "--workspace", "7", "--cwd", "."], false);
    assert_eq!(params.unwrap()["cwd"], here.to_string_lossy().as_ref());
}

#[test]
fn the_host_is_not_asked_without_a_path_or_a_numeric_placement() {
    let (_, asked) = path_request(&["pathy", "spawn", "--workspace", "7"], true);
    assert!(asked.is_empty());
    let (params, asked) = path_request(
        &["pathy", "spawn", "--workspace", "name", "--cwd", "."],
        true,
    );
    assert!(
        asked.is_empty(),
        "a workspace name cannot point at a mirror"
    );
    let here = std::env::current_dir().unwrap();
    assert_eq!(params.unwrap()["cwd"], here.to_string_lossy().as_ref());
}

#[test]
fn a_method_without_remote_execution_keeps_local_paths_even_for_a_numeric_workspace() {
    // launch 의 --workspace 는 새 workspace 이름이라 숫자여도 mirror 를 가리키지 않는다.
    let (params, asked) =
        path_request(&["pathy", "launch", "--workspace", "7", "--cwd", "."], true);
    assert!(asked.is_empty());
    let here = std::env::current_dir().unwrap();
    assert_eq!(params.unwrap()["cwd"], here.to_string_lossy().as_ref());
}

fn request_for(method: &str, params: serde_json::Value) -> tasty_ipc::protocol::JsonRpcRequest {
    tasty_ipc::protocol::JsonRpcRequest {
        caller_agent_id: None,
        response_timeout_ms: None,
        idempotency_key: None,
        jsonrpc: "2.0".into(),
        method: method.into(),
        params,
        id: None,
        session_token: None,
    }
}

#[test]
fn only_a_method_that_declares_the_child_table_gets_it() {
    let mut entry = sample_entry();
    let sub = &mut entry.cli.subcommands[0];
    let declared = sub.ipc_method.clone();
    sub.output = Some(tasty_plugin_manifest::CliOutputDecl::ChildTable);
    let entries = [entry];
    let wants = |method: &str, params| wants_child_table(&entries, &request_for(method, params));

    assert!(wants(&declared, serde_json::json!({})).unwrap());
    assert!(!wants(&declared, serde_json::json!({ "format": "json" })).unwrap());
    assert!(wants(&declared, serde_json::json!({ "format": "dot" })).is_err());
    // 선언하지 않은 메서드는 --format 이라는 이름의 인자가 있어도 건드리지 않는다.
    assert!(!wants("other.method", serde_json::json!({ "format": "dot" })).unwrap());
}

/// 표 분기는 매니페스트 선언을 지나므로 번들 플러그인의 선언이 빠지면 JSON 으로 돌아간다.
#[test]
fn bundled_children_commands_declare_the_child_table_and_the_format_flag() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for (plugin, method) in [
        ("tasty-plugin-claude", "claude.children"),
        ("tasty-plugin-codex", "codex.children"),
    ] {
        let manifest =
            tasty_plugin_manifest::Manifest::load(&crates.join(plugin)).expect("manifest");
        let cli = &manifest.contributes.cli[0];
        let sub = cli
            .subcommands
            .iter()
            .find(|sub| sub.ipc_method == method)
            .expect("children subcommand");
        assert_eq!(
            sub.output,
            Some(tasty_plugin_manifest::CliOutputDecl::ChildTable),
            "{plugin}"
        );
        let group = cli.arg_groups.get(&sub.args).expect("arg group");
        assert!(group.flags.iter().any(|a| a.name == "format"), "{plugin}");
        let others = cli.subcommands.iter().filter(|s| s.ipc_method != method);
        assert!(others.into_iter().all(|s| s.output.is_none()), "{plugin}");
    }
}
