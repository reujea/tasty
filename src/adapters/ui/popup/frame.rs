use crate::adapters::ui::popup::{PopupCloseHook, PopupCloseReason, PopupInstanceId};
use crate::state::MainViewState;

/// 훅이 서로 팝업을 계속 닫는 경우를 막기 위한 반복 상한.
const ON_CLOSE_DRAIN_MAX_ROUNDS: u32 = 8;

/// 닫힌 팝업의 on_close 훅을 실행한다. 훅이 다른 팝업을 닫으면 이어 처리하되 상한을 둔다.
fn drain_on_close_hooks(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    drain_on_close_hooks_with_lookup(ctx, state, engine, |id| {
        crate::adapters::ui::popup::defs::find(id).and_then(|def| def.on_close)
    });
}

/// 창을 뺀 범위의 대상 ID가 아직 엔진에 있는지 확인한다. 창 범위는 대상 소멸로 닫지 않는다.
/// surface는 비활성 탭에 있어도 살아 있는 것으로 본다. 변환은 같은 surface ID를 유지한다.
fn scope_target_exists(
    scope: &crate::adapters::ui::popup::PopupScope,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    use crate::adapters::ui::popup::PopupScope;
    match scope {
        PopupScope::Workspace(ws_id) => engine.find_workspace_index_for_id(*ws_id).is_some(),
        PopupScope::Tab(tab_id) => engine.find_pane_for_tab(*tab_id).is_some(),
        PopupScope::Surface(sid) => engine.find_workspace_index_for_surface(*sid).is_some(),
        PopupScope::Pane(pane_id) => engine.has_pane(*pane_id),
        PopupScope::Window => true,
    }
}

/// 대상 생존 판정. 범위가 살아 있으면 참일 때만 팝업을 남긴다.
type TargetAlive = fn(&MainViewState, &crate::runtime::engine_read::EngineRead<'_>) -> bool;

/// 범위와 작용 대상이 다른 팝업(대상이 보이는 상위 자리로 여는 팝업)과 그 대상 생존 판정.
/// 범위가 살아 있어도 대상이 사라지면 닫는다. 범위가 보이지 않으면 그리기 단계의 대상 검사가 돌지 않으므로
/// 이 판정은 그리기와 무관하게 정리 단계에서 한다.
const TARGET_CHECKS: [(crate::adapters::ui::popup::PopupId, TargetAlive); 4] = [
    (
        crate::adapters::ui::popup::preset_apply::APPLY_TAB_POPUP_ID,
        crate::adapters::ui::popup::preset_apply::tab_popup_target_alive,
    ),
    (
        crate::adapters::ui::popup::preset_apply::APPLY_PANE_POPUP_ID,
        crate::adapters::ui::popup::preset_apply::pane_popup_target_alive,
    ),
    (
        crate::adapters::ui::popup::confirm_force_detach_workspace::CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
        crate::adapters::ui::popup::confirm_force_detach_workspace::target_alive,
    ),
    (
        crate::adapters::ui::popup::confirm_close_remote::CONFIRM_CLOSE_REMOTE_POPUP_ID,
        crate::adapters::ui::popup::confirm_close_remote::target_alive,
    ),
];

/// 범위 대상이나 작용 대상이 사라진 열린 팝업을 닫는다. 매 프레임 그리기 전에 돈다.
/// 닫힌 팝업은 closed_queue에 들어가 같은 프레임의 on_close 훅이 대상·대기 상태를 비운다.
fn close_orphans(state: &mut MainViewState, engine: &crate::runtime::engine_read::EngineRead<'_>) {
    state
        .popups
        .close_scope_orphans(|scope| scope_target_exists(scope, engine));
    let gone: Vec<crate::adapters::ui::popup::PopupId> = TARGET_CHECKS
        .iter()
        .filter(|(id, alive)| state.popups.is_open(id) && !alive(state, engine))
        .map(|(id, _)| *id)
        .collect();
    for id in gone {
        // intent-exempt: 작용 대상이 사라진 팝업의 수명 정리. 범위 고아 정리와 같은 단계이며 포커스·선택은 바꾸지 않는다.
        state.popups.close_with(id, PopupCloseReason::TargetGone);
    }
    // 이름 바꾸기는 인스턴스마다 대상이 달라 인스턴스 데이터로 판정한다.
    let gone_renames: Vec<crate::adapters::ui::popup::PopupInstanceId> = state
        .popups
        .instances_of(crate::adapters::ui::rename_popup::RENAME_POPUP_ID)
        .filter(|p| !crate::adapters::ui::dialog::rename_popup_target_alive(&p.payload, engine))
        .map(|p| p.instance)
        .collect();
    for iid in gone_renames {
        // intent-exempt: 작용 대상이 사라진 팝업의 수명 정리. 범위 고아 정리와 같은 단계이며 포커스·선택은 바꾸지 않는다.
        state
            .popups
            .close_instance(iid, PopupCloseReason::TargetGone);
    }
}

/// 테스트에서 별도 정의 목록을 쓸 수 있도록 훅 조회 함수를 받는다.
fn drain_on_close_hooks_with_lookup(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    lookup: impl Fn(crate::adapters::ui::popup::PopupId) -> Option<PopupCloseHook>,
) {
    let mut round = 0u32;
    loop {
        let queue = state.popups.take_closed_queue();
        if queue.is_empty() {
            return;
        }
        round += 1;
        if round > ON_CLOSE_DRAIN_MAX_ROUNDS {
            tracing::warn!(
                "popup on_close hook drain exceeded {ON_CLOSE_DRAIN_MAX_ROUNDS} rounds — \
                 aborting (hooks may be closing each other in a loop)"
            );
            return;
        }
        for closed in queue {
            if let Some(hook) = lookup(closed.id) {
                hook(ctx, state, engine, &closed);
            }
        }
    }
}

/// 팝업 정의에 따라 그리고 닫기 훅을 처리한다. 오버레이와 같은 LayoutContext를 쓴다.
pub(crate) fn draw_popup_layer(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    draw_ctx: &crate::adapters::ui::LayoutContext,
) {
    // intent-exempt: 저장된 원격 행의 연결 팝업은 연결 상태를 따라 매 프레임 열고 닫는다. 사용자 포커스는 옮기지 않는다.
    crate::adapters::ui::popup::remote_row::sync(state, engine, draw_ctx);

    // intent-exempt: 매 프레임 번역·크기를 렌더링 전에 갱신한다. Intent 큐는 한 프레임 지연을 만든다.
    for def in crate::adapters::ui::popup::defs::all_defs() {
        let fixed_title = def
            .title_fn
            .is_none()
            .then(|| crate::i18n::t(def.title_key).to_string());
        // 제목·크기 함수는 `state`를 읽고 인스턴스는 `state.popups` 안에 있어 값을 먼저 모은다.
        let updates: Vec<(Option<String>, Option<egui::Vec2>)> = state
            .popups
            .entries_of(def.id)
            .map(|p| {
                (
                    def.title_fn.map(|f| f(state, engine, &p.payload)),
                    def.sizer.map(|f| f(state, engine, &p.payload)),
                )
            })
            .collect();
        for (p, (title, size)) in state.popups.entries_of_mut(def.id).zip(updates) {
            match (title, &fixed_title) {
                (Some(t), _) => p.title = t,
                (None, Some(t)) => p.title.clone_from(t),
                (None, None) => {}
            }
            if let Some(sz) = size
                && !p.size_user_overridden
            {
                p.size = sz;
            }
        }
    }

    // intent-exempt: 범위 대상이나 작용 대상이 사라진 팝업의 수명 정리. 포커스·선택은 바꾸지 않는다.
    close_orphans(state, engine);

    // host·plugin 중 최상단 팝업 하나가 Escape를 처리한다. plugin 순번은 직전 프레임
    // 값이므로 plugin 팝업을 처음 연 프레임에는 host가 대상으로 남을 수 있다.
    let plugin_top_z = state.plugin_popup_hittest.iter().map(|o| o.z_seq).max();
    state.popup_escape_owner = state
        .popups
        .topmost_visible_open(Some(draw_ctx))
        .filter(|(_, _, z)| plugin_top_z.is_none_or(|pz| *z > pz))
        .map(|(id, _, _)| id);

    let mut popups = std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());

    // content_fn이 state를 가변 대여하므로 plugin 팝업 영역을 미리 복사한다.
    let plugin_occluders: Vec<crate::adapters::ui::popup::occlusion::Occluder> =
        state.plugin_popup_hittest.clone();

    let mut dispatch_closed: Vec<(PopupInstanceId, PopupCloseReason)> = Vec::new();
    let mut dispatch_unfocused: Vec<PopupInstanceId> = Vec::new();
    let draw_result = popups.draw(
        ctx,
        &mut |cx, ui| {
            if let Some(def) = crate::adapters::ui::popup::defs::find(cx.id) {
                match (def.draw_fn)(ui, state, engine, cx) {
                    crate::adapters::ui::PopupAction::Close => {
                        dispatch_closed.push((cx.instance, PopupCloseReason::Content))
                    }
                    crate::adapters::ui::PopupAction::CloseBy(reason) => {
                        dispatch_closed.push((cx.instance, reason))
                    }
                    crate::adapters::ui::PopupAction::ReleaseFocus => {
                        dispatch_unfocused.push(cx.instance)
                    }
                    crate::adapters::ui::PopupAction::None => {}
                }
            }
        },
        Some(draw_ctx),
        &plugin_occluders,
    );

    state.popup_hovered = draw_result.hovered;
    // egui_bridge가 최종 레이어 순서를 정할 때 사용한다.
    state.popup_layers = draw_result.layers;
    state.host_popup_hittest = draw_result.hit_rects;
    // 대상 목록은 host 팝업이 먼저 채우고 plugin 팝업·드롭다운이 뒤에 더한다.
    state
        .webview_overlay
        .targets
        .extend(draw_result.overlay_targets);

    state.popups = popups;
    for iid in dispatch_unfocused {
        state.popups.set_instance_focused(iid, false);
    }

    // 무대는 별도 콘텐츠이며 원본 팝업은 열린 채 아래에 남는다.
    if let Some(stage) = draw_result.fullscreen_requested {
        if state.open_fullscreen_stage(stage) {
            // 렌더링 시작 뒤 무대 상태가 바뀌었으므로 표시할 다음 프레임을 요청한다.
            ctx.request_repaint();
        } else {
            tracing::warn!("popup fullscreen button targets unknown stage '{stage}'");
        }
    }

    // 팝업 자체 닫기는 Intent를 거치지 않고 즉시 처리한다. 그리기가 닫은 인스턴스는 이미 닫혔다.
    for (iid, reason) in dispatch_closed {
        state.popups.close_instance(iid, reason); // intent-exempt: popup self-close lifecycle.
    }

    drain_on_close_hooks(ctx, state, engine);
}

#[cfg(test)]
mod on_close_drain_tests {
    use super::*;
    use crate::adapters::ui::popup::ClosedPopup;
    use crate::state::tests::test_state;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn ctx() -> egui::Context {
        egui::Context::default()
    }

    /// 실제 정의 목록 대신 테스트용 조회 함수를 넣어 닫기 반복·상한을 검사한다.
    type Lookup = HashMap<crate::adapters::ui::popup::PopupId, PopupCloseHook>;

    fn lookup_from(
        map: Lookup,
    ) -> impl Fn(crate::adapters::ui::popup::PopupId) -> Option<PopupCloseHook> {
        move |id| map.get(id).copied()
    }

    static PLAIN_HOOK_FIRES: AtomicU32 = AtomicU32::new(0);
    fn plain_hook(
        _ctx: &egui::Context,
        _state: &mut MainViewState,
        _engine: &crate::runtime::engine_read::EngineRead<'_>,
        _closed: &ClosedPopup,
    ) {
        PLAIN_HOOK_FIRES.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn drain_fires_hook_once_for_queued_close() {
        PLAIN_HOOK_FIRES.store(0, Ordering::SeqCst);
        let (mut state, mut engine_session) = test_state();
        let engine = engine_session.borrow_mut();
        state.popups.open(
            "notifications",
            crate::model::popup_kind::PopupScope::Window,
        ); // close() 는 open 이었던 popup 만 큐에 push.
        state.popups.close("notifications");

        let mut map: Lookup = HashMap::new();
        map.insert("notifications", plain_hook);
        drain_on_close_hooks_with_lookup(&ctx(), &mut state, &engine.read(), lookup_from(map));

        assert_eq!(PLAIN_HOOK_FIRES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn reentrant_close_from_hook_fires_the_other_hook() {
        static A_FIRES: AtomicU32 = AtomicU32::new(0);
        static B_FIRES: AtomicU32 = AtomicU32::new(0);
        A_FIRES.store(0, Ordering::SeqCst);
        B_FIRES.store(0, Ordering::SeqCst);

        fn hook_a(
            _ctx: &egui::Context,
            state: &mut MainViewState,
            _engine: &crate::runtime::engine_read::EngineRead<'_>,
            _closed: &ClosedPopup,
        ) {
            A_FIRES.fetch_add(1, Ordering::SeqCst);
            state.popups.close("search_bar");
        }
        fn hook_b(
            _ctx: &egui::Context,
            _state: &mut MainViewState,
            _engine: &crate::runtime::engine_read::EngineRead<'_>,
            _closed: &ClosedPopup,
        ) {
            B_FIRES.fetch_add(1, Ordering::SeqCst);
        }

        let (mut state, mut engine_session) = test_state();
        let engine = engine_session.borrow_mut();
        state
            .popups
            .open("search_bar", crate::model::popup_kind::PopupScope::Window); // hook_a 가 닫을 대상 — 먼저 열어둬야 close() 가 큐에 push.
        state.popups.open(
            "notifications",
            crate::model::popup_kind::PopupScope::Window,
        ); // 최초 트리거 대상도 open 이어야 close() 가 큐에 push.
        state.popups.close("notifications"); // 최초 트리거.

        let mut map: Lookup = HashMap::new();
        map.insert("notifications", hook_a);
        map.insert("search_bar", hook_b);
        drain_on_close_hooks_with_lookup(&ctx(), &mut state, &engine.read(), lookup_from(map));

        assert_eq!(A_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(B_FIRES.load(Ordering::SeqCst), 1);
    }

    /// 훅이 자기 자신을 매 라운드 재오픈+재닫음하면 무한 재진입이 되므로,
    /// `ON_CLOSE_DRAIN_MAX_ROUNDS` 를 넘기면 경고 후 중단해야 한다(무한루프 방지).
    #[test]
    fn self_reopening_hook_is_capped_by_max_rounds() {
        static LOOP_FIRES: AtomicU32 = AtomicU32::new(0);
        LOOP_FIRES.store(0, Ordering::SeqCst);

        fn looping_hook(
            _ctx: &egui::Context,
            state: &mut MainViewState,
            _engine: &crate::runtime::engine_read::EngineRead<'_>,
            _closed: &ClosedPopup,
        ) {
            LOOP_FIRES.fetch_add(1, Ordering::SeqCst);
            state.popups.open(
                "notifications",
                crate::model::popup_kind::PopupScope::Window,
            );
            state.popups.close("notifications");
        }

        let (mut state, mut engine_session) = test_state();
        let engine = engine_session.borrow_mut();
        state.popups.open(
            "notifications",
            crate::model::popup_kind::PopupScope::Window,
        ); // close() 는 open 이었던 popup 만 큐에 push.
        state.popups.close("notifications"); // 최초 트리거 — 1라운드째 큐에 이미 있음.

        let mut map: Lookup = HashMap::new();
        map.insert("notifications", looping_hook);
        drain_on_close_hooks_with_lookup(&ctx(), &mut state, &engine.read(), lookup_from(map));

        assert_eq!(LOOP_FIRES.load(Ordering::SeqCst), ON_CLOSE_DRAIN_MAX_ROUNDS);
        // 상한을 넘긴 마지막 배치는 이미 큐에서 꺼냈으며 다시 넣지 않는다.
        assert!(state.popups.take_closed_ids().is_empty());
    }
}

#[cfg(test)]
#[path = "frame_orphan_tests.rs"]
mod orphan_tests;
