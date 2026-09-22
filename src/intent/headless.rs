//! Headless 실행 형태의 Intent 큐 drain + 적용.
//!
//! gui 는 `App::dispatch_pending_intents`(`src/app/dispatch/intents.rs`)가 프레임
//! 끝마다 모든 window / parked state 의 큐를 비운다. 그 경로는 window·view 의존이
//! 커서 통째로 `#[cfg(feature = "gui")]` 이므로, headless 는 같은 계약을 engine
//! 하나짜리로 좁혀 여기서 수행한다. 설계: `docs/design/flows/action-dispatch.md`,
//! 결정 근거: `docs/adr/0111-headless-drains-the-intent-queue.md`.
//!
//! **gui 빌드의 시험 구성에서도 컴파일한다**(모듈 선언의 `cfg(any(not(feature = "gui"), test))`).
//! 호출은 headless boot
//! (`src/boot/headless_dispatch.rs` · `src/boot/headless_plugins.rs` · `src/boot.rs`)
//! 에서만 하지만, 기본(gui) 빌드의 `cargo test` 가 이 경로를 회귀 검증할 수 있어야
//! 하기 때문이다 — 큐 누적 회귀 테스트가 기본 테스트 실행에서 빠지면 그 회귀는
//! `--no-default-features` 를 따로 돌린 사람만 보게 된다.
//!
//! cascade 범위는 gui 의 `handle_core_event`(`src/app/dispatch_domain.rs`)에서
//! **engine 상태로 완결되는 부분만** 가져온다. view redraw·toast·NSMenu 재구성처럼
//! 소비처가 GUI 인 것은 headless 에 소비자가 없어 생략한다. host event 발화도
//! 생략하는데, 그 근거는 "drain 주체가 없다" 가 아니다 — 그 사실은 headless 의
//! 모든 enqueue 지점에 똑같이 참이라 무엇을 생략하고 무엇을 넣을지 가르지
//! 못한다. **가르는 것은 이벤트 종류의 소비자 집합이다.**
//!
//! `dispatch_pending_host_events`(`src/app/dispatch/host_events.rs`, gui 전용)의
//! 소비자는 세 갈래다 — ① 모든 종류를 plugin event bus 로 내보내는 `emit_*`,
//! ② `HookFired` 만 받는 `resolve_hook_fired_task_waits`(`agent.task_await`
//! 대기자를 깨운다), ③ `SurfaceFocused` 만 받는 OSC 제목 재투영. 여기서
//! 생략하는 `NotificationCreated` 의 소비자는 ① 하나뿐이라, 생략으로 잃는 것은
//! bus 전달 하나다. 반대로 ②를 가진 종류는 생략이 곧 기능 정지가 된다.
//!
//! 큐 적재는 별개 축이다 — `pending_host_events` 는 headless 에서 **이미**
//! 자라고 있다(`src/boot.rs` 의 idle-timeout 경로,
//! `src/adapters/ipc/handler/hooks.rs` 의 `surface.fire_hook`). 그러니 여기서
//! 넣지 않는 것은 적재를 막는 것이 아니라 적재율을 올리지 않는 것이다.

use crate::core::intent::CoreEvent;
use crate::core::{AttentionKind, Core, CoreState};
use crate::intent::{DispatchedIntent, Intent};
use crate::state::AppState;

/// 한 번의 drain 이 도는 최대 라운드. 적용 도중 새로 발화된 intent 까지 이어서
/// 처리하되, 서로를 무한히 재발화하는 조합이 생겨도 루프에 갇히지 않게 한다.
/// 상한에 걸린 나머지는 버리지 않고 다음 drain 이 처리한다(큐 길이는 여전히
/// "처리 중인 작업량" 에만 비례한다).
const MAX_DRAIN_ROUNDS: usize = 8;

/// `AppState` 의 pending intent 큐를 비우고 각 intent 를 engine 에 적용한다.
///
/// 호출 시점은 gui 와 같은 계약이다 — IPC 요청 하나를 처리한 뒤 **응답을 보내기
/// 전에** 부른다(gui `App::dispatch_with_caller` 가 응답 반환 전에
/// `dispatch_pending_intents` 를 부르는 것과 동형). 그래야 `surface.set_mark` 응답을
/// 받은 호출자가 곧바로 `surface.read_since_mark` 를 물었을 때 mark 가 이미 서 있다.
pub(crate) fn drain_pending_intents(core: &mut Core, state: &mut AppState, engine: &mut CoreState) {
    for _ in 0..MAX_DRAIN_ROUNDS {
        let batch = state.take_pending_intents();
        if batch.is_empty() {
            return;
        }
        for dispatched in batch {
            apply_one(core, state, engine, dispatched);
        }
    }
    if !state.pending_intents.is_empty() {
        tracing::warn!(
            remaining = state.pending_intents.len(),
            "headless intent drain hit the round cap; the rest run on the next drain"
        );
    }
}

/// `AppState` 의 pending host event 큐를 비우고, **비-bus 소비자가 있는 종류만**
/// 적용한다.
///
/// 오늘 그 조건을 만족하는 것은 `HookFired` 하나다 — push 완료 전략
/// (`core::agent::runner_host::dispatch_push_strategy`)이 훅을 걸고
/// `hook_task_waits` 에 등록한 task 를 이 발화가 마감한다. 그 배선이 없으면
/// headless 에서 `agent.task_await` 대기자는 훅이 발화해도 깨어나지 않고
/// deadline 까지 간 뒤 `runner_thread::expire_overdue_hook_waits` 가 Failed 로
/// 마감한다 — 무응답이 아니라 **틀린 결과**다.
///
/// 나머지 종류는 소비자가 plugin event bus 뿐이라 여기서 버린다. gui 의
/// `emit_*`(`src/app/dispatch/host_events/`)를 headless 로 끌어오려면 window /
/// lua autofire 컨텍스트가 따라와야 하는데 그 소비처는 headless 에 없다. 버려도
/// 오늘 잃는 것은 없다 — 이 큐는 headless 에서 애초에 아무도 비우지 않았으므로
/// bus 로 나간 적이 없다. 다만 **번들 plugin 중 이 이벤트들을 구독하는 것이
/// 0건**이라는 사실 위에 선 판단이라, 구독 요구가 실제로 생기면 그때 bus 배선을
/// 별도로 검토한다.
///
/// 비우는 것 자체가 두 번째 목적이다. 이 큐는 idle-timeout 훅 발화(`src/boot.rs`)
/// 와 `surface.fire_hook`(`src/adapters/ipc/handler/hooks.rs`)이 계속 밀어넣는데
/// headless 에 빼 가는 쪽이 없어 프로세스 수명 동안 자라고 있었다.
pub(crate) fn drain_pending_host_events(core: &Core, state: &mut AppState, engine: &CoreState) {
    for event in state.take_pending_host_events() {
        if let crate::state::PendingHostEvent::HookFired {
            hook_id, exit_code, ..
        } = event
        {
            core.resolve_hook_task_wait(engine, hook_id, exit_code);
        }
    }
}

/// PTY drain 이 낸 OSC 7 cwd 변경을 적용한다 — gui 의 두 단 cascade
/// (`TerminalCwdChanged` → `DomainIntent::SurfaceCwdChanged` → `cascade_surface_cwd_changed`)
/// 중 **engine 에 완결되는 부분**이다. 탭 이름을 그 탭의 focused surface 의 cwd 로 다시 매기고
/// 레이아웃 dirty 를 세운다. dirty 는 렌더용만이 아니다 — 원격 attach mirror 로 나가는 스냅샷
/// diff 가 이 플래그를 본다.
///
/// intent 큐를 거치지 않는다. gui 가 거치는 `SurfaceCwdChanged` 는 view redraw 를 함께 싣는
/// gui 전용 intent 이고, headless 에서 그 intent 가 할 일은 이 두 줄뿐이다. 이 배선이 없던 동안
/// headless 는 OSC 7 을 받아도 탭 이름이 안 바뀌어 `tasty list tree` 와 attach client 가 옛
/// 이름을 봤다(docs/dev-guide/headless-build-boundaries.md "두 조합이 같게 하는 것").
pub(crate) fn apply_terminal_cwd_changed(engine: &mut CoreState, surface_id: u32) {
    if !engine.has_surface(surface_id) {
        return;
    }
    engine.refresh_tab_display_name(surface_id);
    engine.mark_layout_dirty();
}

/// 단일 intent 적용. gui 의 분류(`App::classify_intent`)와 같은 경계다 —
/// `Intent::Domain` 은 `Core::apply` + cascade, 나머지는 도메인 핸들러 직결.
fn apply_one(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    dispatched: DispatchedIntent,
) {
    if !matches!(dispatched.body, Intent::Domain(_)) {
        route_non_domain(core, state, engine, &dispatched);
        return;
    }
    let Intent::Domain(domain) = dispatched.body else {
        // 위 matches! 가 이미 걸러낸 분기 — 도달하지 않는다.
        return;
    };
    match core.apply(engine, domain) {
        Ok(events) => {
            for event in events {
                handle_core_event(engine, event);
            }
        }
        Err(e) => tracing::warn!("headless domain intent failed: {e}"),
    }
}

/// non-Domain intent 라우팅 — gui `App::dispatch_one_intent` 와 같은 표.
///
/// headless 의 IPC 표면이 지금 실제로 발화하는 것은 `Intent::Domain` 뿐이지만
/// (나머지 발화점은 gui 전용 view/단축키 계층), 같은 큐를 공유하는 이상 분류는
/// gui 와 같은 자리에 두어야 나중에 발화점이 생겼을 때 조용히 누락되지 않는다.
fn route_non_domain(
    core: &mut Core,
    state: &mut AppState,
    engine: &mut CoreState,
    dispatched: &DispatchedIntent,
) {
    match &dispatched.body {
        Intent::Ui(_) => crate::intent::popup::handle(state, dispatched),
        Intent::ApplyPreset { .. } | Intent::SavePreset { .. } => {
            crate::intent::preset::handle(core, state, engine, dispatched);
        }
        Intent::SplitSurface { .. } | Intent::ConvertSurface { .. } => {
            crate::intent::surface::handle(core, state, engine, dispatched);
        }
        Intent::NewTab { .. } => crate::intent::tab::handle(core, state, engine, dispatched),
        Intent::SplitPane { .. } => crate::intent::pane::handle(core, state, engine, dispatched),
        Intent::NewWorkspace { .. } => {
            crate::intent::workspace::handle(core, state, engine, dispatched);
        }
        Intent::RestoreClosedItem => {
            crate::intent::closed_item::handle(core, state, engine, dispatched);
        }
        // 호출 전에 걸러진다(`apply_one`).
        Intent::Domain(_) => {}
    }
}

/// `Core::apply` 가 낸 `CoreEvent` 중 **engine 상태로 완결되는 것** 을 적용한다.
/// gui `App::handle_core_event` 의 headless 대응 — 소비처가 view 인 cascade
/// (redraw / toast / theme 재설치 / NSMenu)는 여기에 없다.
fn handle_core_event(engine: &mut CoreState, event: CoreEvent) {
    match event {
        CoreEvent::SettingsUpdated(new_settings) => apply_settings(engine, new_settings),
        CoreEvent::NotificationPushRequested {
            ws_id,
            surface_id,
            title,
            body,
            // 알림음 판정에만 쓰이는 필드 — headless 는 재생하지 않는다(아래 참조).
            source: _,
        } => push_notification(engine, ws_id, surface_id, title, body),
        CoreEvent::TerminalMarkSet { surface_id } => {
            if let Some(t) = engine.find_terminal_by_id_mut(surface_id) {
                t.set_mark();
            }
        }
        CoreEvent::SurfaceCompletionRequested { surface_id, kind } => {
            if engine.has_surface(surface_id) {
                engine.raise_attention(surface_id, kind);
                // 레이아웃 dirty 는 렌더용만이 아니다 — 원격 attach mirror 로 나가는
                // 스냅샷 diff 가 이 플래그를 본다.
                engine.mark_layout_dirty();
            }
        }
        // 나머지는 headless 의 발화점이 만들지 않는 이벤트다. close 계열
        // (`SurfaceClosed` / `PaneClosed` / `TabClosed`)이 여기 없는 것은 생략이 아니라
        // **발화점이 없는 것**이다 — `DomainIntent::Close*` 를 만드는 자리는 IPC 핸들러
        // 셋(`handler/surface/close.rs` · `handler/pane.rs` · `handler/tab.rs`)뿐이고,
        // 셋 다 큐를 거치지 않고 `Core::apply` 를 직접 부른 뒤 `core::structural_cascade` 의
        // cascade 로 자원을 회수한다. 그러니 이 분기에 close 를 넣으면 회수가 두 번 돈다.
        // **큐를 거치는 close 발화점이 생기면 그때 여기에 회수를 배선해야 한다** — 안 하면
        // 닫힌 surface 의 PTY·memory scope 가 조용히 남는다(패닉도 로그도 없다).
        // gui/headless 가 의도적으로 갈리는 지점의 표는
        // `docs/architecture/close-sequence.md` "자원 회수의 소유".
        // 큐 유계성은 이 분기에서도 유지되므로 debug 로그만 남긴다.
        other => tracing::debug!(
            event = ?std::mem::discriminant(&other),
            "headless drain: no cascade for this CoreEvent"
        ),
    }
}

/// gui `App::cascade_settings_updated` 중 engine 에 완결되는 부분. theme 재설치·
/// plugin theme/language 이벤트·NSMenu 재구성은 소비처가 GUI 라 제외한다.
fn apply_settings(engine: &mut CoreState, new_settings: tasty_settings::Settings) {
    engine.settings = new_settings.clone();
    if let Err(e) = new_settings.save() {
        tracing::warn!("failed to save settings: {e}");
    }
}

/// gui `App::cascade_notification_pushed` 중 engine 에 완결되는 부분.
///
/// 알림음은 재생하지 않는다 — headless 빌드는 `src/boot/wiring.rs` 가 NoopPlayer 를
/// 명시 주입해 재생 자체를 지원하지 않고, sound port 접근자도 gui 전용이다.
/// host event 발화도 하지 않는다(모듈 주석의 `pending_host_events` 사유).
fn push_notification(
    engine: &mut CoreState,
    ws_id: u32,
    surface_id: u32,
    title: String,
    body: String,
) {
    if !engine.has_workspace(ws_id) {
        tracing::warn!(ws_id, "NotificationPushRequested: workspace not found");
        return;
    }
    if engine
        .notifications
        .add(ws_id, surface_id, title, body)
        .is_some()
    {
        // 신규 발화(coalesce 아님)만 attention 을 올린다 — gui cascade 와 동형.
        engine.raise_attention(surface_id, AttentionKind::Completion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::builder::CoreBuilder;
    use crate::ipc::caller::CallerContext;
    use crate::ipc::protocol::JsonRpcRequest;
    use std::sync::{Arc, Mutex};

    /// 반복 횟수. "요청 수에 비례해 쌓이는가" 를 보는 것이 목적이라 큐 상한(1)과
    /// 확실히 구분되는 크기면 충분하다.
    const N: usize = 128;

    fn test_core() -> Core {
        CoreBuilder::new()
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
            .build()
            .expect("test Core")
    }

    fn fixture() -> (Core, AppState, CoreState, u32) {
        let (state, engine) = crate::state::tests::test_state();
        let surface_id = *state
            .active_workspace(&engine)
            .all_surface_ids()
            .first()
            .expect("fixture workspace has a surface");
        (test_core(), state, engine, surface_id)
    }

    fn request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            response_timeout_ms: None,
            idempotency_key: None,
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: Some(serde_json::json!(1)),
            session_token: None,
        }
    }

    fn send(
        core: &mut Core,
        state: &mut AppState,
        engine: &mut CoreState,
        method: &str,
        params: serde_json::Value,
    ) {
        let req = request(method, params);
        let resp = crate::ipc::handler::handle_with_caller(
            core,
            state,
            engine,
            &req,
            &CallerContext::Local,
        );
        assert!(resp.error.is_none(), "{method} failed: {:?}", resp.error);
    }

    /// **본 트랙의 회귀 테스트.** headless 는 IPC 요청을 몇 번 받든 Intent 큐가
    /// 유계여야 한다. drain 주체가 사라지면(호출부 제거·cfg 로 가려짐) 큐 길이가
    /// 요청 수에 비례해 자라며, 그 순간 이 테스트가 깨진다.
    #[test]
    fn repeated_ipc_requests_leave_the_intent_queue_bounded() {
        let (mut core, mut state, mut engine, sid) = fixture();
        for i in 0..N {
            send(
                &mut core,
                &mut state,
                &mut engine,
                "surface.set_mark",
                serde_json::json!({ "surface_id": sid }),
            );
            assert!(
                !state.pending_intents.is_empty(),
                "핸들러는 요청마다 최소 하나를 발화한다 (i={i})"
            );
            drain_pending_intents(&mut core, &mut state, &mut engine);
            assert!(
                state.pending_intents.is_empty(),
                "drain 후 큐가 남아 있으면 안 된다 (i={i})"
            );
        }
        assert!(
            state.pending_intents.is_empty(),
            "{N} 번의 IPC 요청 뒤에도 큐는 비어 있어야 한다"
        );
    }

    /// 위 테스트가 무엇을 잡는지 고정하는 대조군 — drain 이 없으면 큐는 요청 수만큼
    /// 자란다(수정 전 headless 의 실제 상태). 이 성질이 사라지면 위 테스트도 더는
    /// 누적을 검증하지 못하므로 함께 둔다.
    #[test]
    fn without_a_drain_the_queue_grows_with_every_request() {
        let (mut core, mut state, mut engine, sid) = fixture();
        for _ in 0..N {
            send(
                &mut core,
                &mut state,
                &mut engine,
                "surface.set_mark",
                serde_json::json!({ "surface_id": sid }),
            );
        }
        // `>=` 인 이유: 요청 자체가 발화하는 1 건 외에, telemetry 이상탐지
        // (`handler/telemetry/anomaly.rs`)가 같은 메서드 반복 호출에 알림 intent 를
        // 추가로 얹는다. 여기서 고정하려는 성질은 정확한 개수가 아니라 "요청 수에
        // 비례해 자란다" 는 것이다.
        assert!(
            state.pending_intents.len() >= N,
            "drain 이 없으면 큐는 요청 수만큼 자란다 (got {})",
            state.pending_intents.len()
        );
    }

    /// drain 이 큐를 *비우기만* 하는 게 아니라 실제로 적용까지 한다는 것을 고정한다 —
    /// 비우기만 하면 큐는 유계지만 headless 의 에이전트 표면은 여전히 죽어 있다.
    #[test]
    fn drain_applies_attention_and_notifications() {
        let (mut core, mut state, mut engine, sid) = fixture();
        assert!(engine.attention_kind(sid).is_none());

        send(
            &mut core,
            &mut state,
            &mut engine,
            "surface.completion",
            serde_json::json!({ "surface_id": sid }),
        );
        drain_pending_intents(&mut core, &mut state, &mut engine);
        assert_eq!(
            engine.attention_kind(sid),
            Some(crate::core::AttentionKind::Completion),
            "surface.completion 은 headless 에서도 attention 을 올려야 한다"
        );

        let before = engine.notifications.unread_count();
        send(
            &mut core,
            &mut state,
            &mut engine,
            "notification.create",
            serde_json::json!({ "surface_id": sid, "title": "t", "body": "b" }),
        );
        drain_pending_intents(&mut core, &mut state, &mut engine);
        assert!(
            engine.notifications.unread_count() > before,
            "notification.create 는 headless 에서도 알림을 적재해야 한다"
        );
    }

    /// 대상 surface 에 `command-completed` 훅을 하나 건다(1회성 아님 — 반복 발화가
    /// 매번 큐에 한 건씩 넣어야 큐 성장을 관측할 수 있다). `hook_id` 반환.
    fn set_a_hook(core: &mut Core, state: &mut AppState, engine: &mut CoreState, sid: u32) -> u64 {
        let resp = crate::ipc::handler::handle_with_caller(
            core,
            state,
            engine,
            &request(
                "hook.set",
                serde_json::json!({
                    "surface_id": sid,
                    "event": "command-completed",
                    "command": "true",
                }),
            ),
            &CallerContext::Local,
        );
        resp.result
            .as_ref()
            .and_then(|v| v.get("hook_id"))
            .and_then(|v| v.as_u64())
            .expect("hook.set returns hook_id")
    }

    /// **재현 — 훅이 발화해도 그것을 기다리던 agent task 가 마감되지 않는다.**
    ///
    /// push 완료 전략(`runner_host.rs::dispatch_push_strategy`)은 대상 surface 에
    /// 1회성 훅을 걸고 그 `hook_id` 를 `hook_task_waits` 에 등록한 뒤 task 를
    /// `AwaitExternal` 로 둔다. 종결은 훅 발화가 낳는
    /// `PendingHostEvent::HookFired` 를 소비하는 쪽의 몫인데, headless 에는 그
    /// 소비자가 없다. 여기서는 등록을 dispatch 없이 직접 흉내 내고(그 함수는
    /// plugin IPC 왕복이 필요하다) 발화만 실제 IPC 경로로 낸다.
    #[test]
    fn a_fired_hook_completes_the_task_that_waited_on_it() {
        use tasty_agent::TaskState;
        use tasty_agent::task::{OnFailure, TaskCommand};

        let (mut core, mut state, mut engine, sid) = fixture();
        let ws = state.active_workspace(&engine).id;

        let task_id = core
            .task_create(
                &engine,
                tasty_agent::task::TaskCreateOpts {
                    workspace_id: ws,
                    name: "push-wait".to_string(),
                    command: TaskCommand::Run {
                        command: vec!["true".into()],
                        workspace_id: ws,
                        cwd: None,
                    },
                    depends_on: Vec::new(),
                    on_failure: OnFailure::default(),
                    metadata: serde_json::Value::Null,
                    now_ms: 1,
                },
                false,
            )
            .expect("task_create")
            .id;
        core.task_set_state(&engine, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

        // 훅 등록은 실제 IPC 경로로 — hook_id 가 진짜여야 발화가 그 id 를 싣는다.
        let resp = crate::ipc::handler::handle_with_caller(
            &mut core,
            &mut state,
            &mut engine,
            &request(
                "hook.set",
                serde_json::json!({
                    "surface_id": sid,
                    "event": "command-completed",
                    "command": "true",
                    "once": true,
                }),
            ),
            &CallerContext::Local,
        );
        let hook_id = resp
            .result
            .as_ref()
            .and_then(|v| v.get("hook_id"))
            .and_then(|v| v.as_u64())
            .expect("hook.set returns hook_id");

        // dispatch_push_strategy 가 하는 등록과 같은 것.
        core.hook_task_waits
            .register(hook_id, ws, task_id.clone(), u64::MAX);

        send(
            &mut core,
            &mut state,
            &mut engine,
            "surface.fire_hook",
            serde_json::json!({ "surface_id": sid, "event": "command-completed:0" }),
        );
        drain_pending_intents(&mut core, &mut state, &mut engine);
        drain_pending_host_events(&core, &mut state, &engine);

        let task = core
            .task_get(&engine, ws, &task_id)
            .expect("task_get")
            .expect("task exists");
        assert!(
            matches!(task.state, TaskState::Succeeded),
            "훅이 발화했는데 기다리던 task 가 마감되지 않았다 (state={:?})",
            task.state
        );
    }

    /// host event 큐도 유계여야 한다. `surface.fire_hook` 은 발화한 훅마다 한 건씩
    /// 넣으므로, 빼 가는 쪽이 없으면 요청 수에 비례해 자란다.
    #[test]
    fn repeated_hook_fires_leave_the_host_event_queue_bounded() {
        let (mut core, mut state, mut engine, sid) = fixture();
        set_a_hook(&mut core, &mut state, &mut engine, sid);

        for i in 0..N {
            send(
                &mut core,
                &mut state,
                &mut engine,
                "surface.fire_hook",
                serde_json::json!({ "surface_id": sid, "event": "command-completed:0" }),
            );
            drain_pending_host_events(&core, &mut state, &engine);
            assert!(
                state.pending_host_events.is_empty(),
                "drain 후 host event 큐가 남아 있으면 안 된다 (i={i})"
            );
        }
    }

    /// 위 테스트가 무엇을 잡는지 고정하는 대조군 — drain 이 없으면 host event 큐는
    /// 발화 수만큼 자란다(수정 전 headless 의 실제 상태).
    #[test]
    fn without_a_drain_the_host_event_queue_grows_with_every_fire() {
        let (mut core, mut state, mut engine, sid) = fixture();
        set_a_hook(&mut core, &mut state, &mut engine, sid);

        for _ in 0..N {
            send(
                &mut core,
                &mut state,
                &mut engine,
                "surface.fire_hook",
                serde_json::json!({ "surface_id": sid, "event": "command-completed:0" }),
            );
        }
        assert_eq!(
            state.pending_host_events.len(),
            N,
            "drain 이 없으면 큐는 발화 수만큼 자란다"
        );
    }

    /// `NewTab` 을 drain 한 뒤 focused pane 의 `(탭 수, 활성 탭)`.
    fn new_empty_tab_then_selection(
        dispatched: impl FnOnce(Intent) -> DispatchedIntent,
    ) -> (usize, usize) {
        let (mut core, mut state, mut engine, _sid) = fixture();
        state.dispatch_intent(dispatched(Intent::NewTab {
            kind: Some("empty".to_string()),
            params: serde_json::json!({}),
        }));
        drain_pending_intents(&mut core, &mut state, &mut engine);
        let pane_id = state.active_workspace(&engine).focused_pane;
        let pane = engine.find_pane_by_id(pane_id).expect("focused pane");
        (pane.tabs.len(), pane.active_tab)
    }

    /// 사용자가 발화한 `NewTab` 은 새 탭을 선택한다.
    #[test]
    fn a_user_new_tab_selects_it() {
        assert_eq!(
            new_empty_tab_then_selection(|i| i.from_user_menu("test")),
            (2, 1)
        );
    }

    /// 에이전트 라벨의 `NewTab` 은 탭을 붙이기만 하고 사용자가 보던 탭을 바꾸지 않는다
    /// (ADR-0502 · ADR-0526). 사용자의 markdown 파일열기 팝업은 자기 popup 을 실어 보내
    /// 사용자 라벨로 도착하므로 이 갈래에 오지 않는다.
    #[test]
    fn an_agent_labelled_new_tab_keeps_the_users_tab() {
        assert_eq!(new_empty_tab_then_selection(Intent::from_agent_ipc), (2, 0));
    }

    /// OSC 7 이 가리킨 cwd 가 탭 이름이 된다 — gui cascade 와 같은 engine 결과다. 레이아웃
    /// dirty 도 선다(attach mirror 로 나가는 스냅샷 diff 가 이것을 본다).
    #[test]
    fn an_osc7_cwd_renames_the_tab_and_marks_the_layout_dirty() {
        let (_core, state, mut engine, sid) = fixture();
        let mut terminal = tasty_terminal::Terminal::new_detached(80, 24);
        terminal.feed_bytes(b"\x1b]7;file://localhost/tmp/tasty-osc7-probe\x07");
        engine.terminals.insert(sid, terminal);
        engine.layout_dirty.clear();
        let tab_name = |engine: &CoreState| {
            state
                .active_workspace(engine)
                .pane_layout()
                .all_pane_ids()
                .iter()
                .find_map(|&pid| {
                    let pane = state
                        .active_workspace(engine)
                        .pane_layout()
                        .find_pane(pid)?;
                    pane.tabs
                        .iter()
                        .find(|t| t.contains_surface(sid))
                        .map(|t| t.display_name())
                })
                .expect("fixture tab")
        };
        assert_ne!(
            tab_name(&engine),
            "tasty-osc7-probe",
            "전제: 아직 cwd 이름이 아니다"
        );

        apply_terminal_cwd_changed(&mut engine, sid);

        assert_eq!(tab_name(&engine), "tasty-osc7-probe");
        assert!(
            engine.layout_dirty.is_dirty(),
            "attach 스냅샷 diff 가 볼 dirty 가 서야 한다"
        );
    }

    /// 배선 가드 — headless PTY drain 이 위 함수를 실제로 부르는지 소스에서 확인한다. 끊기면
    /// 위 시험은 그대로 통과하고 헤드리스의 탭 이름만 조용히 멈춘다. 두 조합의 실제 동작은
    /// `tests/e2e_tests.rs` 의 `an_osc7_cwd_becomes_the_tab_name` 이 잰다.
    #[test]
    fn the_headless_pty_drain_applies_the_cwd_change() {
        assert!(
            include_str!("../boot.rs").contains("headless::apply_terminal_cwd_changed"),
            "src/boot.rs 의 PTY drain 이 OSC 7 cwd 변경을 적용하지 않는다"
        );
    }

    /// 배선 가드 — headless 진입점이 drain 을 실제로 부르는지 소스에서 확인한다.
    /// 위 테스트들은 drain 함수 자체의 계약만 보므로, 호출부가 빠지면(가장 그럴듯한
    /// 회귀 형태다) 그것만으로는 잡히지 않는다.
    #[test]
    fn headless_entry_points_call_the_drain() {
        for (path, src) in [
            (
                "src/boot/headless_dispatch.rs",
                include_str!("../boot/headless_dispatch.rs"),
            ),
            (
                "src/boot/headless_plugins.rs",
                include_str!("../boot/headless_plugins.rs"),
            ),
            ("src/boot.rs", include_str!("../boot.rs")),
        ] {
            assert!(
                src.contains("headless::drain_pending_intents"),
                "{path} 이 Intent 큐 drain 을 호출하지 않는다 — headless 큐 누적 회귀"
            );
            assert!(
                src.contains("headless::drain_pending_host_events"),
                "{path} 이 host event 큐 drain 을 호출하지 않는다 — 훅을 기다리는 \
                 agent task 가 headless 에서 마감되지 않는다"
            );
        }
    }
}
