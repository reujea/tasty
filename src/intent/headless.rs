//! 헤드리스 인스턴스의 Intent와 호스트 이벤트 큐 처리.
//!
//! GUI의 창 순회 대신 engine 하나에 명령을 적용한다.
//! redraw·토스트·메뉴 갱신은 제외하며, 알림을 플러그인 이벤트 버스로 보내지 않는다.
//! HookFired는 완료를 기다리는 작업을 깨워야 하므로 여기서 처리한다.
//! 기본 GUI 시험에서도 이 모듈을 컴파일해 큐 처리를 검증한다.
//! 설계: docs/design/flows/action-dispatch.md, docs/dev-guide/headless-ipc-surface.md.

use crate::app::command::CoreEvent;
use crate::app::services::AppServices;
use crate::core::AttentionKind;
#[cfg(test)]
use crate::core::CoreState;
use crate::intent::{DispatchedIntent, Intent};
use crate::runtime::engine_access::EngineMut;
use crate::state::RequestContext;

/// 한 번에 처리할 묶음 수. 처리 중 명령이 계속 추가돼도 루프를 빠져나올 수 있게 한다.
/// 남은 명령은 다음 호출에서 처리하며 큐의 크기 자체를 제한하는 값은 아니다.
const MAX_DRAIN_ROUNDS: usize = 8;

/// IPC 응답을 보내기 전에 대기 명령을 적용한다.
/// 예를 들어 surface.set_mark 응답 후에는 surface.read_since_mark로 그 마커를 읽을 수 있어야 한다.
#[cfg(test)]
pub(crate) fn drain_pending_intents(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
) {
    drain_with(core, state, engine, |_, _| false);
}

#[cfg(not(feature = "gui"))]
pub(crate) fn drain_pending_intents_in_app(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    journal: &mut crate::app::journal::JournalApplication,
) {
    let engine_id = state.engine_id;
    for _ in 0..MAX_DRAIN_ROUNDS {
        let batch = state.take_pending_intents();
        if batch.is_empty() {
            return;
        }
        for mut dispatched in batch {
            match crate::app::creation_intent::resolve(
                state,
                &engine.as_ref(),
                &dispatched.body,
                &dispatched.origin,
            ) {
                Ok(Some(intent)) => dispatched.body = Intent::Domain(intent),
                Ok(None) => {}
                Err(error) => {
                    super::report_apply_error(
                        None,
                        &dispatched.origin,
                        super::FailureContext::General,
                        &error,
                    );
                    continue;
                }
            }
            if let Intent::Domain(crate::app::command::DomainIntent::RetireExitedSurface {
                surface_id,
                generation,
            }) = &dispatched.body
                && !engine
                    .runtime
                    .terminals
                    .matches_generation(*surface_id, *generation)
            {
                continue;
            }
            let handled = match (engine_id, &dispatched.body) {
                (Some(id), Intent::Domain(intent)) => {
                    journal.admit_metadata_intent(id, engine.core, intent, &dispatched.origin, None)
                }
                (Some(id), Intent::DirectRename(rename)) => {
                    journal.admit_direct_rename(id, rename, &dispatched.origin);
                    true
                }
                (None, Intent::Domain(_) | Intent::DirectRename(_)) => {
                    tracing::error!("headless structural intent has no application engine owner");
                    true
                }
                _ => false,
            };
            if !handled {
                apply_one(core, state, engine, dispatched);
            }
        }
    }
}

#[cfg(test)]
fn drain_with(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    mut journal_admission: impl FnMut(&CoreState, &DispatchedIntent) -> bool,
) {
    for _ in 0..MAX_DRAIN_ROUNDS {
        let batch = state.take_pending_intents();
        if batch.is_empty() {
            return;
        }
        for dispatched in batch {
            if !journal_admission(engine.core, &dispatched) {
                apply_one(core, state, engine, dispatched);
            }
        }
    }
    if !state.pending_intents.is_empty() {
        tracing::warn!(
            remaining = state.pending_intents.len(),
            "headless intent drain hit the round cap; the rest run on the next drain"
        );
    }
}

/// 호스트 이벤트 큐를 비우고 HookFired로 완료를 기다리는 작업을 처리한다.
/// 나머지 이벤트는 여기서 버리며 플러그인 이벤트 버스로 전달하지 않는다.
/// 헤드리스에서도 이 이벤트를 구독해야 한다면 별도의 전달 경로가 필요하다.
pub(crate) fn drain_pending_host_events(core: &AppServices, engine: &mut EngineMut<'_>) {
    engine.runtime.pending_lifecycle_events.clear();
    for event in engine.take_pending_host_events() {
        if let crate::state::PendingHostEvent::HookFired {
            hook_id, exit_code, ..
        } = event
        {
            core.tasks.resolve_hook_task_wait(
                engine.task_scope,
                hook_id,
                exit_code,
                core.now_unix_millis() as u64,
            );
        }
    }
}

/// OSC 7의 cwd 변경을 탭 이름에 반영한다.
/// 레이아웃 dirty는 원격 attach의 스냅샷 차이 계산에도 필요하다.
pub(crate) fn apply_terminal_cwd_changed(engine: &mut EngineMut<'_>, surface_id: u32) {
    if !engine.has_surface(surface_id) {
        return;
    }
    engine.refresh_tab_display_name(surface_id);
    engine.mark_layout_dirty();
}

/// 시험이 intent 하나를 headless 적용 경로로 실행한다.
#[cfg(test)]
pub(crate) fn apply_one_for_test(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    dispatched: DispatchedIntent,
) {
    apply_one(core, state, engine, dispatched);
}

fn apply_one(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    dispatched: DispatchedIntent,
) {
    if !matches!(dispatched.body, Intent::Domain(_)) {
        route_non_domain(core, state, engine, &dispatched);
        return;
    }
    let Intent::Domain(domain) = dispatched.body else {
        return;
    };
    engine.refresh_attach_presentation(&state.navigation);
    match core.apply_live(engine, domain) {
        Ok(events) => {
            for event in events {
                handle_core_event(core, engine, event);
            }
        }
        Err(e) => tracing::warn!("headless domain intent failed: {e}"),
    }
}

// GUI와 같은 Intent 변종을 처리해 큐에 들어온 요청이 빠지지 않도록 한다.
fn route_non_domain(
    core: &mut AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    dispatched: &DispatchedIntent,
) {
    match &dispatched.body {
        Intent::ApplyPreset { .. } | Intent::CapturePreset { .. } => {
            crate::intent::preset::handle(core, state, engine, dispatched);
        }
        Intent::SplitSurface { .. }
        | Intent::ConvertSurface { .. }
        | Intent::NewTab { .. }
        | Intent::SplitPane { .. }
        | Intent::NewWorkspace { .. } => {
            reject_non_domain("structural intent bypassed journal admission")
        }
        Intent::RestoreClosedItem => {
            reject_non_domain("undo intent bypassed journal admission");
        }
        Intent::DirectRename(_) => {
            reject_non_domain("direct rename bypassed journal admission");
        }
        Intent::Domain(_) => {}
        #[cfg(feature = "gui")]
        Intent::CommitDivider(_)
        | Intent::MouseCaptureHint { .. }
        | Intent::RemoteBrowser(_)
        | Intent::AttachSizeSync(_)
        | Intent::DisconnectMirror { .. }
        | Intent::SavedRow { .. }
        | Intent::PatchSettings(_)
        | Intent::Engine(_)
        | Intent::Frame(_)
        | Intent::ForwardMirror { .. }
        | Intent::RespondApproval { .. }
        | Intent::Ui(_)
        | Intent::NewTabWithFollowup { .. }
        | Intent::PrepareTutorial { .. } => {
            reject_non_domain("intent requires the GUI application adapter")
        }
    }
}

fn reject_non_domain(reason: &str) {
    tracing::error!("{reason}");
}

/// CoreEvent에서 engine 상태 변경만 처리한다. 창 갱신과 토스트는 제외한다.
fn handle_core_event(core: &mut AppServices, engine: &mut EngineMut<'_>, event: CoreEvent) {
    match event {
        CoreEvent::SettingsUpdated { settings, done } => {
            apply_settings(core, engine, settings, done)
        }
        CoreEvent::NotificationPushRequested {
            level,
            ws_id,
            surface_id,
            title,
            body,
            // 헤드리스에서는 알림음을 재생하지 않는다.
            source: _,
            merge,
        } => push_notification(engine, ws_id, surface_id, title, body, level, merge),
        CoreEvent::TerminalMarkSet { surface_id } => {
            if let Some(t) = engine.find_terminal_by_id_mut(surface_id) {
                t.set_mark();
            }
        }
        CoreEvent::SurfaceCompletionRequested { surface_id, kind } => {
            if engine.has_surface(surface_id) {
                engine.raise_attention(surface_id, kind);
                // 원격 attach의 스냅샷 차이 계산에도 필요하다.
                engine.mark_layout_dirty();
            }
        }
        // 닫기 IPC는 journal의 확정 삭제와 원 자원 회수 완료 뒤 응답한다.
        // 닫기 요청을 이 큐로도 받게 되면 해당 정리 경로를 연결해야 한다.
        // 정리 주체는 docs/architecture/close-sequence.md를 참고한다.
        other => tracing::debug!(
            event = ?std::mem::discriminant(&other),
            "headless drain: no cascade for this CoreEvent"
        ),
    }
}

/// 메모리에 적용한 뒤 저장을 file writer에 넣는다. 저장을 기다리지 않는다. 요청이 기다리면
/// 저장 결과가 나온 회차에 [`settle_settings_saves`]가 메모리 적용과 저장 결과를 함께 돌려준다.
fn apply_settings(
    core: &mut AppServices,
    engine: &mut EngineMut<'_>,
    new_settings: tasty_settings::Settings,
    done: Option<crate::app::settings_change::SettingsDone>,
) {
    crate::app::task_report::apply(core, &new_settings);
    crate::app::anomaly_settings::apply(core, &new_settings);
    crate::app::attach_heartbeat::apply(core, &new_settings);
    engine.runtime.settings = new_settings.clone();
    core.submit_settings_save(new_settings, None, done);
    // 접수 거절이나 writer 없는 구성의 결과는 바로 나와 있다.
    settle_settings_saves(core);
}

/// 끝난 설정 저장의 결과를 기다리는 요청에 돌려준다. 헤드리스 루프가 회차마다 부른다.
pub(crate) fn settle_settings_saves(core: &mut AppServices) {
    for settled in core.take_settled_settings() {
        let crate::app::file_writer::SaveReply {
            origin: _origin,
            done,
            settings,
        } = settled.reply;
        if let Some(done) = done {
            done.finish(crate::app::settings_change::SettingsOutcome::Applied {
                changed: true,
                saved: settled.saved,
                settings,
            });
        }
    }
}

/// 작업 스레드가 남긴 시스템 알림을 알림 intent로 바꾼다. surface를 지정했으면 그 workspace가,
/// 아니면 활성 workspace가 받는다. workspace가 없으면 그대로 둔다.
#[cfg(not(feature = "gui"))]
pub(crate) fn queue_system_notices(state: &mut RequestContext, engine: &EngineMut<'_>) {
    let Some(active) = engine
        .workspace_at(state.active_workspace_index(engine.core))
        .map(|ws| ws.id)
    else {
        return;
    };
    for notice in crate::system_notice::take() {
        let owner = (notice.surface_id != 0)
            .then(|| {
                engine
                    .core
                    .find_workspace_index_for_surface(notice.surface_id)
            })
            .flatten()
            .and_then(|(index, _)| engine.workspace_at(index))
            .map(|ws| ws.id);
        let (ws_id, surface_id) = owner.map_or((active, 0), |ws| (ws, notice.surface_id));
        state.dispatch_intent(
            crate::app::command::DomainIntent::PushNotification {
                level: notice.level,
                ws_id,
                surface_id,
                title: notice.title,
                body: notice.body,
                source: notice.source.to_string(),
                merge: crate::app::command::NotificationMerge::Separate,
            }
            .from_system(),
        );
    }
}

// 헤드리스는 알림을 저장하지만 알림음과 NotificationCreated 이벤트 전송은 생략한다.
fn push_notification(
    engine: &mut EngineMut<'_>,
    ws_id: u32,
    surface_id: u32,
    title: String,
    body: String,
    level: crate::notification::NotificationLevel,
    merge: crate::app::command::NotificationMerge,
) {
    if !engine.has_workspace(ws_id) {
        tracing::warn!(ws_id, "NotificationPushRequested: workspace not found");
        return;
    }
    if merge
        .add(
            &mut engine.live.notifications,
            ws_id,
            surface_id,
            title,
            body,
            level,
        )
        .is_some()
    {
        // 기존 알림에 합친 경우에는 attention을 다시 올리지 않는다.
        engine.raise_attention(surface_id, AttentionKind::Completion);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::builder::AppServicesBuilder;
    use crate::ipc::caller::CallerContext;
    use crate::ipc::protocol::JsonRpcRequest;
    use std::sync::{Arc, Mutex};

    // 큐 처리 유무에 따른 누적 차이가 드러나도록 요청을 반복한다.
    const N: usize = 128;

    fn test_core() -> AppServices {
        AppServicesBuilder::new()
            .with_clock(Arc::new(
                crate::adapters::test::fake_clock::FakeClock::default(),
            ))
            .with_clipboard(Arc::new(
                crate::adapters::test::mock_clipboard::MockClipboard::default(),
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
            .expect("test AppServices")
    }

    fn fixture() -> (
        AppServices,
        RequestContext,
        crate::runtime::engine_session::EngineSession,
        u32,
    ) {
        let (state, mut engine_session) = crate::state::tests::test_state();
        let engine = engine_session.borrow_mut();
        let surface_id = *state
            .active_workspace(&engine)
            .all_surface_ids()
            .first()
            .expect("fixture workspace has a surface");
        (test_core(), state, engine_session, surface_id)
    }

    fn request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            caller_agent_id: None,
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
        core: &mut AppServices,
        state: &mut RequestContext,
        engine: &mut EngineMut<'_>,
        method: &str,
        params: serde_json::Value,
    ) {
        let req = request(method, params);
        let resp = crate::ipc::handler::handle_with_caller(
            core,
            state,
            engine,
            &req,
            &CallerContext::local(),
        );
        assert!(resp.error.is_none(), "{method} failed: {:?}", resp.error);
    }

    #[test]
    fn repeated_ipc_requests_leave_the_intent_queue_bounded() {
        let (mut core, mut state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
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
                "핸들러는 요청마다 명령을 최소 하나 추가한다 (i={i})"
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

    // 큐를 처리하지 않는 대조군에서는 요청마다 명령이 쌓여야 한다.
    #[test]
    fn without_a_drain_the_queue_grows_with_every_request() {
        let (mut core, mut state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
        for _ in 0..N {
            send(
                &mut core,
                &mut state,
                &mut engine,
                "surface.set_mark",
                serde_json::json!({ "surface_id": sid }),
            );
        }
        // 이상 탐지에서도 알림 명령을 추가할 수 있어 정확한 개수 대신 하한을 검사한다.
        assert!(
            state.pending_intents.len() >= N,
            "drain 이 없으면 큐는 요청 수만큼 자란다 (got {})",
            state.pending_intents.len()
        );
    }

    // 큐를 비우기만 하지 않고 attention과 알림 상태까지 바꾸는지 검사한다.
    #[test]
    fn drain_applies_attention_and_notifications() {
        let (mut core, mut state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
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

        let before = engine.live.notifications.unread_count();
        send(
            &mut core,
            &mut state,
            &mut engine,
            "notification.create",
            serde_json::json!({ "surface_id": sid, "title": "t", "body": "b" }),
        );
        drain_pending_intents(&mut core, &mut state, &mut engine);
        assert!(
            engine.live.notifications.unread_count() > before,
            "notification.create 는 headless 에서도 알림을 적재해야 한다"
        );
    }

    /// 시스템 사건은 간격 안에 같은 workspace·level로 이어져도 각각 남고, 터미널 쪽 병합은 그대로다.
    #[test]
    fn system_notices_stay_separate_while_terminal_notices_still_merge() {
        use crate::app::command::{DomainIntent, NotificationMerge};
        use crate::notification::NotificationLevel;
        let (mut core, mut state, mut engine_session, _sid) = fixture();
        let mut engine = engine_session.borrow_mut();
        let ws_id = engine.workspace_at(0).expect("workspace").id;
        let push = |state: &mut RequestContext, title: &str, level, merge| {
            state.dispatch_intent(
                DomainIntent::PushNotification {
                    level,
                    ws_id,
                    surface_id: 0,
                    title: title.into(),
                    body: format!("{title} cause"),
                    source: "test".into(),
                    merge,
                }
                .from_system(),
            );
        };
        let warning = NotificationLevel::Warning;
        push(&mut state, "hook A", warning, NotificationMerge::Separate);
        push(
            &mut state,
            "webhook B",
            warning,
            NotificationMerge::Separate,
        );
        push(&mut state, "state.db", warning, NotificationMerge::Separate);
        drain_pending_intents(&mut core, &mut state, &mut engine);
        let titles: Vec<_> = engine
            .live
            .notifications
            .all()
            .map(|n| (n.title.clone(), n.body.clone()))
            .collect();
        assert_eq!(
            titles,
            [
                ("hook A".to_string(), "hook A cause".to_string()),
                ("webhook B".to_string(), "webhook B cause".to_string()),
                ("state.db".to_string(), "state.db cause".to_string()),
            ]
        );
        assert_eq!(engine.live.notifications.unread_count(), 3);

        let info = NotificationLevel::Info;
        push(&mut state, "osc 1", info, NotificationMerge::Coalesce);
        push(&mut state, "osc 2", info, NotificationMerge::Coalesce);
        drain_pending_intents(&mut core, &mut state, &mut engine);
        assert_eq!(engine.live.notifications.unread_count(), 4);
    }

    // 반복해서 큐에 넣도록 once가 아닌 훅을 등록한다.
    fn set_a_hook(
        core: &mut AppServices,
        state: &mut RequestContext,
        engine: &mut EngineMut<'_>,
        sid: u32,
    ) -> u64 {
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
            &CallerContext::local(),
        );
        resp.result
            .as_ref()
            .and_then(|v| v.get("hook_id"))
            .and_then(|v| v.as_u64())
            .expect("hook.set returns hook_id")
    }

    // 플러그인 IPC를 호출하지 않고 완료 대기 등록을 직접 구성한다.
    // 훅 등록과 실행은 실제 IPC를 사용한다.
    #[test]
    fn a_fired_hook_completes_the_task_that_waited_on_it() {
        use tasty_agent::TaskState;
        use tasty_agent::task::{OnFailure, TaskCommand};

        let (mut core, mut state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
        let ws = state.active_workspace(&engine).id;

        let task_id = core
            .tasks
            .task_create(
                engine.task_scope,
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
        core.tasks
            .task_set_state(engine.task_scope, ws, &task_id, TaskState::Running, 2)
            .expect("Ready -> Running");

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
            &CallerContext::local(),
        );
        let hook_id = resp
            .result
            .as_ref()
            .and_then(|v| v.get("hook_id"))
            .and_then(|v| v.as_u64())
            .expect("hook.set returns hook_id");

        core.tasks
            .hook_task_waits()
            .register(hook_id, ws, task_id.clone(), u64::MAX);

        send(
            &mut core,
            &mut state,
            &mut engine,
            "surface.fire_hook",
            serde_json::json!({ "surface_id": sid, "event": "command-completed:0" }),
        );
        drain_pending_intents(&mut core, &mut state, &mut engine);
        drain_pending_host_events(&core, &mut engine);

        let task = core
            .tasks
            .task_get(engine.task_scope, ws, &task_id)
            .expect("task_get")
            .expect("task exists");
        assert!(
            matches!(task.state, TaskState::Succeeded),
            "훅을 실행했지만 대기하던 작업이 종료되지 않았다 (state={:?})",
            task.state
        );
    }

    #[test]
    fn repeated_hook_fires_leave_the_host_event_queue_bounded() {
        let (mut core, mut state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
        set_a_hook(&mut core, &mut state, &mut engine, sid);

        for i in 0..N {
            send(
                &mut core,
                &mut state,
                &mut engine,
                "surface.fire_hook",
                serde_json::json!({ "surface_id": sid, "event": "command-completed:0" }),
            );
            drain_pending_host_events(&core, &mut engine);
            assert!(
                engine.runtime.pending_host_events.is_empty(),
                "drain 후 host event 큐가 남아 있으면 안 된다 (i={i})"
            );
        }
    }

    // 이벤트를 처리하지 않는 대조군에서는 실행한 훅 수만큼 큐가 쌓인다.
    #[test]
    fn without_a_drain_the_host_event_queue_grows_with_every_fire() {
        let (mut core, mut state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
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
            engine.runtime.pending_host_events.len(),
            N,
            "큐를 처리하지 않으면 실행한 훅 수만큼 이벤트가 쌓인다"
        );
    }

    #[test]
    fn an_osc7_cwd_renames_the_tab_and_marks_the_layout_dirty() {
        let (_core, state, mut engine_session, sid) = fixture();
        let mut engine = engine_session.borrow_mut();
        let mut terminal = tasty_terminal::Terminal::new_detached(80, 24);
        terminal.feed_bytes(b"\x1b]7;file://localhost/tmp/tasty-osc7-probe\x07");
        engine.runtime.terminals.insert(sid, terminal, None);
        engine.persistence.dirty.clear();
        let tab_name = |engine: &EngineMut<'_>| {
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
                    pane.tabs.iter().find(|t| t.contains_surface(sid)).map(|t| {
                        t.display_name(
                            state
                                .navigation
                                .surface_id(t)
                                .and_then(|sid| engine.live.surface_titles.get(&sid)),
                        )
                    })
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
            engine.persistence.dirty.is_dirty(),
            "attach 스냅샷 갱신에 필요한 layout dirty를 설정해야 한다"
        );
    }

    // 소스에 호출 문자열이 있는지만 검사한다. 실제 GUI·헤드리스 동작은
    // tests/e2e_tests.rs의 an_osc7_cwd_becomes_the_tab_name이 검사한다.
    #[test]
    fn the_headless_pty_drain_applies_the_cwd_change() {
        assert!(
            include_str!("../boot.rs").contains("headless::apply_terminal_cwd_changed"),
            "src/boot.rs 의 PTY drain 이 OSC 7 cwd 변경을 적용하지 않는다"
        );
    }

    // 함수 자체의 시험과 별개로 진입점 소스에 처리 함수 이름이 있는지 확인한다.
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
                "{path}에 헤드리스 Intent 큐 처리 함수 이름이 없다"
            );
            assert!(
                src.contains("headless::drain_pending_host_events"),
                "{path}에 헤드리스 호스트 이벤트 큐 처리 함수 이름이 없다"
            );
        }
    }
}
