use crate::state::AppState;

/// 마우스 캡처 배너 "더보기" 메뉴가 (액션 클릭이든 outside click/Esc 든) 닫혔으면
/// 대상 surface 필드를 비운다. 매번 확인해도 무해(idempotent)하다.
fn cleanup_mouse_capture_menu_target(
    state: &mut AppState,
    dispatch_closed: &[&'static str],
    draw_result_closed: &[&'static str],
) {
    let id = crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID;
    if dispatch_closed.contains(&id) || draw_result_closed.contains(&id) {
        state.dialogs.mouse_capture_banner_menu_target = None;
    }
}

/// on_close 훅 drain 이 상한 없이 서로를 계속 닫는 논리 오류를 방지하는 라운드
/// 상한. 초과 시 그 라운드는 발화하지 않고 경고 로그 후 중단한다.
const ON_CLOSE_DRAIN_MAX_ROUNDS: u32 = 8;

/// `PopupManager.closed_queue` 를 drain 하며 등록된 `on_close` 훅을 발화한다.
/// 훅이 (재발화 등으로) 다른 popup 을 닫으면 그 close 도 큐에 쌓이므로, 큐가
/// 마를 때까지 반복한다 — 단 훅 2개가 서로를 계속 닫는 등의 논리 오류를 대비해
/// 상한을 둔다.
///
/// 6개 close 경로가 전부 이 큐를 채우는지는 `src/adapters/ui/popup.rs`
/// (경로 2: 외부 클릭)와 `src/intent/popup.rs`(경로 3/4: `ClosePopup`/
/// `TogglePopup`)의 단위 테스트로 개별 확인한다. 경로 1(draw_fn Close)과
/// 경로 5(App 직접 호출)는 둘 다 결국 동일한 `state.popups.close(id)` 호출로
/// 귀결되므로 별도 테스트가 필요 없다 — 아래 `popup::close()` 자체의 단위
/// 테스트가 그 호출 경로를 이미 검증한다. 경로 6(debug IPC)은 `defs::find`
/// 로 popup 존재를 확인한 뒤 `UiIntent::ClosePopup` 을 dispatch 할 뿐이라
/// 구조적으로 경로 3 과 동일하다(`adapters/ipc/handler/debug.rs` 의
/// `handle_debug_host_popup_close` 참고).
fn drain_on_close_hooks(
    ctx: &egui::Context,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
) {
    drain_on_close_hooks_with_lookup(ctx, state, engine, |id| {
        crate::adapters::ui::popup::defs::find(id).and_then(|def| def.on_close)
    });
}

/// [`drain_on_close_hooks`] 의 실제 루프 — 훅 조회를 `lookup` 클로저로 분리해
/// 실제 `defs::all_defs()` 정적 레지스트리에 의존하지 않고 단위 테스트할 수 있게
/// 한다(레지스트리는 컴파일 타임 고정이라 테스트 전용 더미 popup 을 못 끼워 넣음).
fn drain_on_close_hooks_with_lookup(
    ctx: &egui::Context,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    lookup: impl Fn(
        crate::adapters::ui::popup::PopupId,
    ) -> Option<fn(&egui::Context, &mut AppState, &mut crate::core::CoreState)>,
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
        for id in queue {
            if let Some(hook) = lookup(id) {
                hook(ctx, state, engine);
            }
        }
    }
}

/// 범용 popup 렌더 루프 — 등록된 모든 `PopupDef` 를 그리고, 6개 close 경로
/// (draw_fn Close / X버튼·외부클릭 / `ClosePopup`·`TogglePopup` / App 직접 호출 /
/// debug IPC) 어디로 닫히든 `on_close` 훅을 drain 한다. `draw_ctx` 는 popup 과
/// 오버레이 체인(`overlay::draw_overlays`)이 같은 프레임에 공유하므로 호출자
/// (`ui::draw_popups`)가 한 번만 만들어 넘긴다.
pub(crate) fn draw_popup_layer(
    ctx: &egui::Context,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    draw_ctx: &crate::adapters::ui::LayoutContext,
) {
    // Refresh popup titles (i18n) and dynamic sizes each frame. Sizers read
    // in-memory caches so this is cheap.
    // intent-exempt: 매 프레임 i18n title / size 재계산은 mutation 이 아닌 draw-prep.
    // Intent 큐로 보내면 1프레임 지연 + 매 프레임 enqueue 라 부적절.
    for def in crate::adapters::ui::popup::defs::all_defs() {
        let new_title = if let Some(title_fn) = def.title_fn {
            (title_fn)(state, engine)
        } else {
            crate::i18n::t(def.title_key).to_string()
        };
        let new_size = def.sizer.map(|f| f(state, engine));
        if let Some(p) = state.popups.get_mut(def.id) {
            p.title = new_title;
            // 사용자가 직접 리사이즈한 팝업은 sizer 가 크기를 되돌리지 않는다
            // (size_user_overridden 가드 — popup close 시 리셋되어 다음 open 에 복원).
            if let Some(sz) = new_size
                && !p.size_user_overridden
            {
                p.size = sz;
            }
        }
    }

    // Esc 소유권(규칙 7 의 키보드 판, ADR-0084) — host/plugin 통틀어 최상단인 popup
    // 하나만 Esc 를 소비한다. plugin popup 이 위면 `None` 이고 그 프레임의 Esc 는
    // `plugin_bridge::popup_render` 쪽이 가져간다. plugin z 는 1 프레임 stale 이라
    // (아래 `plugin_occluders` 와 같은 재료) plugin popup 이 막 열린 첫 프레임에는
    // host 가 소유자로 남을 수 있다 — Esc 는 사용자가 누르는 이산 입력이라 실사용에서
    // 이 한 프레임에 걸릴 확률이 없다.
    let plugin_top_z = state.plugin_popup_hittest.iter().map(|o| o.z_seq).max();
    state.popup_escape_owner = state
        .popups
        .topmost_visible_open(Some(draw_ctx))
        .filter(|(_, z)| plugin_top_z.is_none_or(|pz| *z > pz))
        .map(|(id, _)| id);

    // Temporarily take the popup manager to avoid borrow conflicts with AppState.
    let mut popups = std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());

    // 규칙 7 — 이 좌표를 나보다 위의 plugin popup 이 덮는가. `content_fn` 이 `state` 를
    // 가변으로 잡으므로 미리 복사해 둔다(Copy 요소 몇 개짜리 Vec — 비용 무시 가능).
    let plugin_occluders: Vec<crate::adapters::ui::popup::occlusion::Occluder> =
        state.plugin_popup_hittest.clone();

    let mut dispatch_closed: Vec<&'static str> = Vec::new();
    let draw_result = popups.draw(
        ctx,
        &mut |id, ui| {
            if let Some(def) = crate::adapters::ui::popup::defs::find(id)
                && matches!(
                    (def.draw_fn)(ui, state, engine),
                    crate::adapters::ui::PopupAction::Close
                )
            {
                dispatch_closed.push(def.id);
            }
        },
        Some(draw_ctx),
        &plugin_occluders,
    );

    // Update input layer state: popup hover blocks mouse events to lower layers
    state.popup_hovered = draw_result.hovered;
    // `enforce_foreground_z_order`(`src/gfx/gpu/egui_bridge.rs`)가 이번 프레임 popup
    // Area 들을 순서대로 최상단으로 올릴 때 읽는다.
    state.popup_layers = draw_result.layers;
    // plugin popup 판정이 같은 프레임에 읽는다(`draw_plugin_popups`).
    state.host_popup_hittest = draw_result.hit_rects;

    state.popups = popups;

    // 타이틀바 전체화면 버튼 → 무대 진입. popup 은 **열린 채로 둔다**(무대가 덮으므로
    // 보이지 않을 뿐이고, 무대를 나오면 그대로 다시 보인다). 무대에 올라가는 것은 이
    // popup 인스턴스가 아니라 같은 형상의 별개 콘텐츠다
    // (`docs/design/systems/fullscreen-stage.md` §모델).
    if let Some(stage) = draw_result.fullscreen_requested {
        if state.open_fullscreen_stage(stage) {
            // 무대 진입은 **이 프레임 렌더가 이미 시작된 뒤** 상태만 바꾼다 — 화면이
            // 바뀌려면 프레임이 한 번 더 필요하고, 클릭이 세운 dirty 는 이 프레임에서
            // 이미 소비됐다. 프레임을 유도하지 않으면 다음 입력이 올 때까지 무대가
            // 보이지 않는다(실측으로 잡힌 회귀).
            ctx.request_repaint();
        } else {
            tracing::warn!("popup fullscreen button targets unknown stage '{stage}'");
        }
    }

    // Close popups requested by draw dispatch or X button / outside click.
    // popup self-close (draw_fn 이 Close 반환 / X 버튼 / 외부 클릭) 는 popup 시스템
    // 자체의 lifecycle. Intent 큐를 거치면 시각적 close 가 1프레임 지연되어 X 버튼
    // 클릭이 즉시 반응하지 않는 UX 결함이 생긴다.
    for id in dispatch_closed.iter().chain(draw_result.closed.iter()) {
        state.popups.close(id); // intent-exempt: popup self-close lifecycle.
    }

    // `PopupManager::close()` 는 모든 close 경로(draw_fn Close / X버튼·외부클릭 /
    // UiIntent::ClosePopup·TogglePopup / App 직접 호출 / debug IPC)가 거치는
    // 유일한 지점이므로, 여기서 `on_close` 훅을 drain하면 각 popup 모듈이 소유한
    // 뒷정리가 어떤 경로로 닫히든 돈다 — dispatch_closed/draw_result.closed 두
    // 경로에만 붙던 옛 방식과 달리 intent/App/debug IPC 경로도 커버한다.
    drain_on_close_hooks(ctx, state, engine);

    // 마우스 캡처 배너 "더보기" 메뉴 — outside click/Esc로 닫히면(액션 클릭이 아니라)
    // 대상 필드를 정리한다(`draw_popup_layer` 의 인지 복잡도 예산을 넘지 않도록 helper 로 분리).
    cleanup_mouse_capture_menu_target(state, &dispatch_closed, &draw_result.closed);
}

#[cfg(test)]
mod on_close_drain_tests {
    use super::*;
    use crate::state::tests::test_state;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn ctx() -> egui::Context {
        egui::Context::default()
    }

    /// `defs::all_defs()` 는 컴파일 타임에 고정된 정적 레지스트리라 테스트 전용
    /// 더미 popup 을 끼워 넣을 수 없다 — `drain_on_close_hooks_with_lookup` 을
    /// 직접 호출해 `lookup` 을 이 HashMap 으로 대체함으로써 실제 레지스트리와
    /// 무관하게 drain 루프(재진입/상한) 자체를 검증한다.
    type Lookup = HashMap<
        crate::adapters::ui::popup::PopupId,
        fn(&egui::Context, &mut AppState, &mut crate::core::CoreState),
    >;

    fn lookup_from(
        map: Lookup,
    ) -> impl Fn(
        crate::adapters::ui::popup::PopupId,
    ) -> Option<fn(&egui::Context, &mut AppState, &mut crate::core::CoreState)> {
        move |id| map.get(id).copied()
    }

    static PLAIN_HOOK_FIRES: AtomicU32 = AtomicU32::new(0);
    fn plain_hook(
        _ctx: &egui::Context,
        _state: &mut AppState,
        _engine: &mut crate::core::CoreState,
    ) {
        PLAIN_HOOK_FIRES.fetch_add(1, Ordering::SeqCst);
    }

    /// 큐에 담긴 id 하나당 훅이 정확히 1회 발화한다.
    #[test]
    fn drain_fires_hook_once_for_queued_close() {
        PLAIN_HOOK_FIRES.store(0, Ordering::SeqCst);
        let (mut state, mut engine) = test_state();
        state.popups.open("notifications"); // close() 는 open 이었던 popup 만 큐에 push.
        state.popups.close("notifications");

        let mut map: Lookup = HashMap::new();
        map.insert("notifications", plain_hook);
        drain_on_close_hooks_with_lookup(&ctx(), &mut state, &mut engine, lookup_from(map));

        assert_eq!(PLAIN_HOOK_FIRES.load(Ordering::SeqCst), 1);
    }

    /// 훅이 다른 popup 을 닫으면(재진입) 그 훅도 같은 drain 호출 안에서 발화한다.
    #[test]
    fn reentrant_close_from_hook_fires_the_other_hook() {
        static A_FIRES: AtomicU32 = AtomicU32::new(0);
        static B_FIRES: AtomicU32 = AtomicU32::new(0);
        A_FIRES.store(0, Ordering::SeqCst);
        B_FIRES.store(0, Ordering::SeqCst);

        fn hook_a(
            _ctx: &egui::Context,
            state: &mut AppState,
            _engine: &mut crate::core::CoreState,
        ) {
            A_FIRES.fetch_add(1, Ordering::SeqCst);
            // 재진입 표현 — "notifications" 훅이 발화하며 다른 popup("search_bar")
            // 을 닫아 그 훅(hook_b)도 같은 drain 호출 안에서 연쇄 발화하게 한다.
            state.popups.close("search_bar");
        }
        fn hook_b(
            _ctx: &egui::Context,
            _state: &mut AppState,
            _engine: &mut crate::core::CoreState,
        ) {
            B_FIRES.fetch_add(1, Ordering::SeqCst);
        }

        let (mut state, mut engine) = test_state();
        state.popups.open("search_bar"); // hook_a 가 닫을 대상 — 먼저 열어둬야 close() 가 큐에 push.
        state.popups.open("notifications"); // 최초 트리거 대상도 open 이어야 close() 가 큐에 push.
        state.popups.close("notifications"); // 최초 트리거.

        let mut map: Lookup = HashMap::new();
        map.insert("notifications", hook_a);
        map.insert("search_bar", hook_b);
        drain_on_close_hooks_with_lookup(&ctx(), &mut state, &mut engine, lookup_from(map));

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
            state: &mut AppState,
            _engine: &mut crate::core::CoreState,
        ) {
            LOOP_FIRES.fetch_add(1, Ordering::SeqCst);
            // 매번 재오픈 후 재닫음 — dedup 가드(open 이었을 때만 push)를 매 라운드
            // 통과시켜 큐를 계속 채운다(무한 재진입 시뮬레이션).
            state.popups.open("notifications");
            state.popups.close("notifications");
        }

        let (mut state, mut engine) = test_state();
        state.popups.open("notifications"); // close() 는 open 이었던 popup 만 큐에 push.
        state.popups.close("notifications"); // 최초 트리거 — 1라운드째 큐에 이미 있음.

        let mut map: Lookup = HashMap::new();
        map.insert("notifications", looping_hook);
        drain_on_close_hooks_with_lookup(&ctx(), &mut state, &mut engine, lookup_from(map));

        // 정확히 ON_CLOSE_DRAIN_MAX_ROUNDS 라운드만큼만 발화하고 중단해야 한다
        // (그 이상이면 상한이 실제로 작동하지 않는 것).
        assert_eq!(LOOP_FIRES.load(Ordering::SeqCst), ON_CLOSE_DRAIN_MAX_ROUNDS);
        // 상한을 넘긴 마지막 배치는 `take_closed_queue()` 로 이미 꺼내진 뒤(그래야
        // 그 라운드가 "비어있지 않음"을 판정할 수 있다) 처리 없이 버려진다 — 큐 자체는
        // 빈 채로 남는다(유실된 배치가 재시도 대상으로 남지 않음, 순수 backstop).
        assert!(state.popups.take_closed_queue().is_empty());
    }
}
