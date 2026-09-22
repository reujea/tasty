//! CLI 진입점이 만든 요청을 **프로덕션 핸들러에 그대로 먹여** 본다.
//!
//! `crates/tasty-cli` 의 `command_to_request` 는 clap 서브커맨드를 `(메서드,
//! params)` 로 옮기는 기계적 매핑이다. 기계적 매핑이 틀리는 방식은 정확히
//! **문자열 오타** 이고, 그건 컴파일러가 잡지 못한다 — `"surface_id"` 를
//! `"surface"` 로 적어도 빌드는 통과하고 런타임에 "Missing required parameter"
//! 가 될 뿐이다.
//!
//! 기존 가드는 이 방향을 덮지 않는다: `tests/cli_naming_count_drift.rs` 는
//! `METHOD_TABLE` 의 **개수**만 세고, `tests/ipc_router_table_parity.rs` 는
//! **라우터 ↔ 표**만 본다. CLI 가 보내는 params 를 보는 것은 아무것도 없다.
//!
//! 그래서 여기서는 CLI 가 조립한 params 를 **핸들러가 실제로 읽는지**를
//! 확인한다 — 파라미터 이름이 틀리면 핸들러가 `Missing`/`Invalid` 로 떨어지므로,
//! "파라미터 에러가 아니다" 를 단언하는 것만으로 오타가 잡힌다. 파괴적인
//! 핸들러(`respawn_terminal`)는 존재하지 않는 surface 로 불러 **대상 조회까지
//! 도달했다**는 것만 확인한다.
//!
//! `window.close` 만 예외다 — App 레벨(`app/ipc/app_methods.rs`)에서 창 목록을
//! 훑어 처리해서 `(AppState, CoreState)` 만으로는 부를 수 없다. 메서드 이름과
//! params 키가 그 핸들러가 읽는 것과 같은지를 대신 확인한다.

use serde_json::json;
use tasty_cli::request::command_to_request;
use tasty_cli::{
    CloseCommands, Commands, HookHandlerCommands, SendCommands, SessionCommands, SurfaceCommands,
};

use crate::ipc::caller::CallerContext;

/// 응답이 **파라미터 계열 에러가 아님**을 단언한다. 성공이든 "대상 없음" 이든
/// 상관없다 — 여기서 보는 것은 "핸들러가 CLI 가 보낸 키를 읽어 그 다음 단계까지
/// 갔는가" 뿐이다.
fn assert_params_were_understood(method: &str, resp: &tasty_ipc::protocol::JsonRpcResponse) {
    if let Some(err) = &resp.error {
        let m = &err.message;
        assert!(
            !(m.contains("Missing") || m.contains("missing") || m.contains("Invalid 'surface_id'")),
            "{method}: CLI 가 보낸 params 를 핸들러가 못 읽었다 — 이름이 어긋났을 \
             가능성이 크다: {m}"
        );
    }
}

/// 어댑터를 전부 테스트 더블로 채운 `Core`.
///
/// `pub(crate)`: sibling 핸들러 테스트(`surface::close` 의 하드 점유 회귀)가 같은 구성을
/// 재사용한다 — 핸들러 테스트마다 어댑터 열 개를 다시 조립하면 그중 하나가 조용히
/// 달라진다.
pub(crate) fn test_core() -> crate::core::Core {
    test_core_builder().build().expect("test Core")
}

/// [`test_core`] 의 조립을 `build` 직전에서 돌려준다 — 한두 칸만 바꾼 `Core` 가 필요한
/// 시험(memory 대체 저장소 표시 등)이 어댑터 열 개를 다시 조립하지 않게 한다.
pub(crate) fn test_core_builder() -> crate::core::builder::CoreBuilder {
    use std::sync::{Arc, Mutex};
    crate::core::builder::CoreBuilder::new()
        .with_fs(Arc::new(crate::adapters::test::mem_fs::MemFileSystem::new()))
        .with_clock(Arc::new(
            crate::adapters::test::fake_clock::FakeClock::default(),
        ))
        .with_clipboard(Arc::new(
            crate::adapters::test::mock_clipboard::MockClipboard::default(),
        ))
        .with_process(Arc::new(
            crate::adapters::test::mock_process::MockProcessSpawner,
        ))
        .with_home(Arc::new(crate::adapters::test::tmp_home::TmpHome::new(
            tempfile::tempdir().expect("tmp").keep(),
        )))
        .with_sound_player(Arc::new(crate::ports::notification_sound::NoopPlayer))
        .with_memory(Arc::new(Mutex::new(
            tasty_memory::testing::InMemoryStorage::new(),
        )))
        .with_themes(Arc::new(tasty_themes::ThemeStore::new()))
        .with_preset_store(Arc::new(Mutex::new(
            tasty_presets::PresetStore::load_default(),
        )))
        .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
}

/// `tasty surface cursor-position|foreground-process|locate` — 조회 셋.
/// 살아 있는 surface 를 넘겨 **성공까지** 확인한다.
#[test]
fn surface_query_cli_entry_points_reach_their_handlers() {
    let (_state, engine) = crate::state::tests::test_state();
    let surface = engine.workspaces[0]
        .all_surface_ids()
        .first()
        .copied()
        .expect("워크스페이스에 surface 가 있어야 한다");

    let cases: Vec<(SurfaceCommands, &str)> = vec![
        (
            SurfaceCommands::CursorPosition { surface },
            "surface.cursor_position",
        ),
        (
            SurfaceCommands::ForegroundProcess { surface },
            "surface.foreground_process",
        ),
        (SurfaceCommands::Locate { surface }, "surface.locate"),
    ];

    for (command, expected_method) in cases {
        let req = command_to_request(&Commands::Surface { command });
        assert_eq!(req.method, expected_method);

        let resp = match expected_method {
            "surface.cursor_position" => {
                super::surface::handle_cursor_position(&engine, json!(1), &req.params)
            }
            "surface.foreground_process" => {
                super::surface::handle_foreground_process(&engine, json!(1), &req.params)
            }
            _ => super::surface::handle_surface_locate(&engine, json!(1), &req.params),
        };
        assert_params_were_understood(expected_method, &resp);
        assert!(
            resp.result.is_some(),
            "{expected_method}: 살아 있는 surface 에 대한 조회는 성공해야 한다: {:?}",
            resp.error
        );
    }
}

/// `tasty surface respawn-terminal` — 파괴적이라 **없는 surface** 로 부른다.
/// 파라미터를 못 읽으면 `Missing`, 읽었으면 대상 조회 실패다.
#[test]
fn respawn_terminal_cli_entry_point_reaches_target_lookup() {
    let mut core = test_core();
    let (_state, mut engine) = crate::state::tests::test_state();
    let missing = 999_999u32;

    let req = command_to_request(&Commands::Surface {
        command: SurfaceCommands::RespawnTerminal { surface: missing },
    });
    assert_eq!(req.method, "surface.respawn_terminal");

    let resp = super::surface::handle_surface_respawn_terminal(
        &mut core,
        &mut engine,
        json!(1),
        &req.params,
    );
    assert_params_were_understood("surface.respawn_terminal", &resp);
    assert!(
        resp.error.is_some(),
        "없는 surface 는 에러여야 한다(파라미터는 읽혔다)"
    );
}

/// `tasty surface fire-hook` — `surface_id` 와 `event` 두 키를 함께 확인한다.
/// 훅이 하나도 등록돼 있지 않아도 파라미터 단계는 지나야 한다.
#[test]
fn fire_hook_cli_entry_point_reaches_its_handler() {
    let mut core = test_core();
    let (mut state, mut engine) = crate::state::tests::test_state();
    let surface = engine.workspaces[0].all_surface_ids()[0];

    let req = command_to_request(&Commands::Surface {
        command: SurfaceCommands::FireHook {
            surface,
            event: "process-exit".to_string(),
        },
    });
    assert_eq!(req.method, "surface.fire_hook");
    assert_eq!(req.params["event"], json!("process-exit"));

    let resp = super::hooks::handle_surface_fire_hook(
        &mut core,
        &mut state,
        &mut engine,
        json!(1),
        &req.params,
    );
    assert_params_were_understood("surface.fire_hook", &resp);
}

/// `tasty send text --wait-idle` — 플래그가 메서드를 가르고, params 는 그대로다.
#[test]
fn send_text_wait_idle_cli_entry_point_switches_method_and_reaches_its_handler() {
    let (_state, mut engine) = crate::state::tests::test_state();
    let surface = engine.workspaces[0].all_surface_ids()[0];

    let plain = command_to_request(&Commands::Send {
        command: SendCommands::Text {
            text: "echo hi".to_string(),
            surface: Some(surface),
            wait_idle: false,
        },
    });
    assert_eq!(plain.method, "surface.send");

    let req = command_to_request(&Commands::Send {
        command: SendCommands::Text {
            text: "echo hi".to_string(),
            surface: Some(surface),
            wait_idle: true,
        },
    });
    assert_eq!(req.method, "surface.send_wait_idle");

    let resp = super::handle_send_wait_idle(&mut engine, json!(1), &req.params);
    assert_params_were_understood("surface.send_wait_idle", &resp);
    assert!(
        resp.result.is_some(),
        "유휴 상태의 살아 있는 surface 로 보내면 성공해야 한다: {:?}",
        resp.error
    );
}

/// `tasty session issue|revoke|list` — 셋 다 프로덕션 핸들러까지 간다.
#[test]
fn session_cli_entry_points_reach_their_handlers() {
    let core = test_core();
    let caller = CallerContext::Local;

    let issue = command_to_request(&Commands::Session {
        command: SessionCommands::Issue {
            agent_id: "probe-agent".to_string(),
            permissions: vec!["surface.read".to_string()],
            ttl_ms: Some(60_000),
        },
    });
    assert_eq!(issue.method, "session.issue");
    let resp = super::session::handle_issue(&core, &caller, json!(1), &issue.params);
    assert_params_were_understood("session.issue", &resp);
    assert!(
        resp.result.is_some(),
        "토큰 발급은 성공해야 한다: {:?}",
        resp.error
    );

    let revoke = command_to_request(&Commands::Session {
        command: SessionCommands::Revoke {
            token: "not-a-real-token".to_string(),
        },
    });
    assert_eq!(revoke.method, "session.revoke");
    let resp = super::session::handle_revoke(&core, json!(1), &revoke.params);
    assert_params_were_understood("session.revoke", &resp);

    let list = command_to_request(&Commands::Session {
        command: SessionCommands::List,
    });
    assert_eq!(list.method, "session.list");
    let resp = super::session::handle_list(&core, json!(1));
    assert!(resp.result.is_some(), "목록 조회는 성공해야 한다");
}

/// `tasty close window --id <N>` — 핸들러가 App 레벨이라 여기서 부를 수 없다.
/// 대신 메서드 이름과 params 키가 `App::ipc_handle_window_close` 가 읽는 것과
/// 같은지를 고정한다(`cmd.request.params.get("id").and_then(as_u64)`).
#[test]
fn close_window_cli_entry_point_matches_what_the_app_handler_reads() {
    let req = command_to_request(&Commands::Close {
        command: CloseCommands::Window { id: 3 },
    });
    assert_eq!(req.method, "window.close");
    assert_eq!(
        req.params.get("id").and_then(|v| v.as_u64()),
        Some(3),
        "App 핸들러는 `id` 를 u64 로 읽는다"
    );
}

/// `tasty hook-handler get|upsert|remove` — 레지스트리를 **안 건드리는 갈래**로만
/// 부른다. 그 셋은 프로세스 전역 싱글턴과 `~/.tasty/hook-handlers.toml` 을 건드리므로
/// 성공 갈래를 테스트에서 부르면 실행한 사람의 실제 설정을 고친다.
///
/// 그래서 각 갈래가 **자기 키를 읽었을 때만 나올 수 있는 거절문**을 고정한다 —
/// `assert_params_were_understood` 의 "Missing 아님" 만으로는 키 오타가 안 잡힌다.
/// CLI 가 `action` 을 다른 이름으로 보냈다면 서버는 "nothing to patch" 라고 답하지
/// `'action' must be …` 라고 답하지 않는다.
#[test]
fn hook_handler_edit_cli_entry_points_reach_their_handlers() {
    let absent = "user/definitely-not-registered-in-this-test";

    let req = command_to_request(&Commands::HookHandler {
        command: HookHandlerCommands::Get {
            id: absent.to_string(),
        },
    });
    assert_eq!(req.method, "hook_handler.get");
    let resp = super::hook_handler::handle_get(json!(1), &req.params);
    let msg = resp.error.expect("없는 id 는 거절된다").message;
    assert!(msg.contains("not found"), "id 키가 안 읽혔다: {msg}");

    // --source 를 잘못 적으면 그 키를 읽은 핸들러만 이 문장을 낸다.
    let req = command_to_request(&Commands::HookHandler {
        command: HookHandlerCommands::Upsert {
            id: "user/x".into(),
            source: Some("bogus".into()),
            priority: None,
            display_name_key: None,
            disabled: None,
            action: None,
            calls: None,
        },
    });
    assert_eq!(req.method, "hook_handler.upsert");
    let resp = super::hook_handler::handle_upsert(json!(1), &req.params);
    let msg = resp.error.expect("잘못된 source 는 거절된다").message;
    // **거절문의 낱말로 판정하되, 그 낱말이 다른 거절문에 없어야 한다.** 키가 안 닿으면
    // 서버는 "nothing to patch — give at least one of 'source', …" 로 답하는데 그 문장에도
    // `'source'` 가 들어 있다. 그 낱말로 재면 단언이 공허해진다 — 실제로 그랬다(키를
    // `actions` 로 바꾼 변이가 살아남았다). `hook|webhook|any` 는 이 갈래에만 있다.
    assert!(
        msg.contains("hook|webhook|any"),
        "source 키가 안 읽혔다: {msg}"
    );

    // --calls 는 action 자리로 들어간다. 스키마에 안 맞는 것을 보내 그 자리를 고정한다.
    let req = command_to_request(&Commands::HookHandler {
        command: HookHandlerCommands::Upsert {
            id: "user/x".into(),
            source: None,
            priority: None,
            display_name_key: None,
            disabled: None,
            action: None,
            calls: Some(r#"[{"no_method_key":1}]"#.into()),
        },
    });
    let resp = super::hook_handler::handle_upsert(json!(1), &req.params);
    let msg = resp
        .error
        .expect("스키마에 안 맞는 calls 는 거절된다")
        .message;
    // 위와 같은 이유로 `'action'` 이 아니라 이 갈래에만 있는 스키마 힌트로 잰다.
    assert!(msg.contains("ipc_sequence"), "action 키가 안 읽혔다: {msg}");

    let req = command_to_request(&Commands::HookHandler {
        command: HookHandlerCommands::Remove {
            id: absent.to_string(),
        },
    });
    assert_eq!(req.method, "hook_handler.remove");
    let resp = super::hook_handler::handle_remove(json!(1), &req.params);
    let result = resp.result.expect("없는 id 제거는 오류가 아니다");
    assert_eq!(result["id"], absent, "id 키가 안 읽혔다");
    assert_eq!(result["existed"], false);
}
