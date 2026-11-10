//! 토스트·배너·단축키 도움말·튜토리얼을 그린다.
//! 팝업과 같은 LayoutContext를 공유하되 입력 처리는 각 오버레이의 규칙을 따른다.

use crate::state::MainViewState;

/// 더보기 메뉴가 열려 있는 배너의 범위. 팝업 상태와 대상 surface를 함께 확인한다.
fn mouse_capture_more_menu_open_for(
    state: &MainViewState,
) -> Option<crate::adapters::ui::banner::BannerScope> {
    if !state
        .popups
        .is_open(crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID)
    {
        return None;
    }
    state
        .dialogs
        .mouse_capture_banner_menu_target
        .map(crate::adapters::ui::banner::BannerScope::Surface)
}

/// 오버레이를 그린다. Foreground 레이어의 상대 순서는 egui_bridge에서도 조정한다.
pub(crate) fn draw_overlays(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    draw_ctx: &crate::adapters::ui::LayoutContext,
    terminal_rect: crate::model::PhysicalRect,
    scale_factor: f32,
) {
    let reduced_motion = engine.settings.accessibility.reduced_motion;
    state
        .toasts
        .set_lifetime_ms(engine.settings.overlay.toast_duration_ms);
    state.toasts.draw(ctx, draw_ctx, reduced_motion);

    // View 배너의 기본 표시 영역은 탭바 아래다. 배너는 마우스만 소비한다.
    let th = crate::theme::theme();
    let screen = ctx.screen_rect();
    let view_placeholder = Some(egui::Rect::from_min_max(
        egui::pos2(screen.left(), screen.top() + th.tab_bar_height.value()),
        screen.max,
    ));
    // 메뉴가 열려 있는 동안에는 포인터가 떠나도 더보기 버튼을 강조한다.
    let more_menu_open_for = mouse_capture_more_menu_open_for(state);
    let active_workspace = engine
        .core
        .workspace_at(draw_ctx.active_workspace)
        .map(|ws| (draw_ctx.active_workspace, ws.id));
    state
        .attach_notices
        .sync_banner(&mut state.banners, active_workspace);
    let tab_name = |surface: u32| {
        let layout = engine
            .core
            .workspace_at(draw_ctx.active_workspace)?
            .pane_layout();
        let tab = layout
            .all_pane_ids()
            .into_iter()
            .filter_map(|id| layout.find_pane(id))
            .flat_map(|pane| pane.tabs.iter())
            .find(|tab| tab.contains_surface(surface))?;
        Some(engine.tab_display_name(tab, Some(surface)))
    };
    state
        .attach_size_syncs
        .sync_banner(&mut state.banners, active_workspace, tab_name);
    let banner_result = state.banners.draw(
        ctx,
        draw_ctx,
        &th,
        view_placeholder,
        reduced_motion,
        more_menu_open_for.as_ref(),
    );
    state.banner_hovered = banner_result.hovered;
    state.banner_layer = Some(banner_result.layer);
    if let Some((scope, trigger_rect)) = banner_result.more_clicked {
        crate::adapters::ui::mouse_capture_menu::open(state, engine, ctx, &scope, trigger_rect);
    }
    if let Some((workspace_id, clicks)) = banner_result.attach_refusal
        && clicks.dismiss
    {
        state.attach_notices.dismiss(workspace_id);
    }

    if let Some((workspace_id, clicks)) = banner_result.attach_size_sync {
        use crate::intent::AttachSizeSyncAction;
        let action = if clicks.retry {
            AttachSizeSyncAction::Retry { workspace_id }
        } else {
            AttachSizeSyncAction::Dismiss { workspace_id }
        };
        state.dispatch_intent(
            crate::intent::Intent::AttachSizeSync(action).from_user_menu("attach_size_sync_banner"),
        );
    }

    // 단축키 도움말은 키보드 포커스를 받지 않으며, 홀드 조합에 따라 표시 지연이 다르다.
    let hint_result = crate::adapters::ui::modifier_hint_overlay::draw_modifier_hint(
        ctx,
        &mut state.modifier_hint,
        engine.settings,
        &th,
        reduced_motion,
    );
    state.modifier_hint_hovered = hint_result.hovered;
    state.modifier_hint_layer = hint_result.layer;

    // 튜토리얼은 말풍선만 마우스를 소비하며 마커 위치는 매 프레임 다시 계산한다.
    let content_area = crate::adapters::ui::to_egui_rect(terminal_rect, scale_factor);
    crate::adapters::ui::tutorial::draw_tutorial_overlay(
        ctx,
        state,
        engine,
        draw_ctx,
        content_area,
        &th,
    );

    if let Some((pos, size)) = hint_result.persist {
        // 드래그를 놓았을 때 위치·크기를 저장한다. 다른 창과 공유하며 마지막 저장값이 남는다.
        let mut new_settings = engine.settings.clone();
        new_settings.modifier_hint.pos = Some(pos);
        new_settings.modifier_hint.size = Some(size);
        state.dispatch_intent(
            crate::app::command::DomainIntent::UpdateSettings(new_settings)
                .from_user_menu("modifier_hint.geometry"),
        );
    }
}
