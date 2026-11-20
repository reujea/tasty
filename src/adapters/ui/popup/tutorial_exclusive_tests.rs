//! 튜토리얼 말풍선은 포커스 노드가 팝업이 아닐 때만 키를 받는다. plugin 팝업이 포커스 노드가 되면 정규화가 튜토리얼의
//! 키보드 몫을 놓고, 말풍선은 그리지 않아 egui 포커스를 가진 단추도 Enter 로 눌리지 않는다.

use egui::{Context, Event, Key};

use super::repro::{POPUP, manager_with_r_popup};
use super::{PANE_R, layout, scene};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::state::RequestContext;
use crate::state::focus_node::FocusNode;

/// 창의 한 egui 패스처럼 튜토리얼 말풍선과 plugin 팝업을 같은 입력으로 그린다(말풍선 다음 plugin 렌더).
fn window_frame(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    mgr: &crate::plugin::PluginManager,
    events: Vec<Event>,
) {
    let screen = super::repro::screen();
    let input = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let theme = tasty_type_appearance::theme::Theme::with_colors_and_zoom(
        tasty_themes::mocha_fallback_colors(),
        false,
        1.0,
    );
    crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(state, mgr.popup_instances());
    drop(ctx.run(input, |ctx| {
        crate::adapters::ui::tutorial::draw_tutorial_overlay(
            ctx,
            state,
            engine,
            &layout(),
            screen,
            &theme,
        );
        crate::plugin_bridge::popup_render::draw_plugin_popups(
            ctx,
            state,
            engine,
            Some(crate::app::plugin_display::PluginDisplay::new(mgr)),
            Some(&layout()),
        );
    }));
}

fn key(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    }
}

/// Given 튜토리얼 다음 단추가 egui 키 포커스를 가지고 R 에 plugin 팝업이 열려 있을 때, When 사용자가 연 그 plugin
/// 팝업이 포커스 노드가 되고 Enter 가 오면, Then 말풍선은 그려지지 않아 다음 단추가 눌리지 않고 튜토리얼은 키보드를
/// 놓는다. 말풍선과 plugin 팝업은 창처럼 같은 패스에서 그린다(따로 그리면 egui 가 그리지 않은 단추의 포커스를 푼다).
#[test]
fn enter_does_not_press_the_tutorial_button_after_a_plugin_popup_takes_the_focus_node() {
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    // 첫 주제의 첫 단계는 콘텐츠 영역을 가리키는 읽기 단계라 다음 단추가 늘 켜져 있다.
    state.tutorial.request_start(0);
    state.tutorial.keyboard_focus = true;
    window_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    window_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    assert_eq!(state.tutorial.active.map(|a| a.step), Some(0));
    assert_eq!(state.focus_node(&engine.read()), FocusNode::Tutorial);
    assert!(
        ctx.memory(|m| m.focused().is_some()),
        "다음 단추가 egui 키 포커스를 가진다"
    );

    crate::plugin_bridge::popup_scope::focus_opened_popup(
        &mut state,
        &mgr,
        POPUP,
        crate::plugin_bridge::popup_scope::PopupOpener::UserCommand,
    );
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Plugin(POPUP))
    );
    assert!(
        !state.tutorial.keyboard_focus,
        "팝업이 포커스 노드가 되면 튜토리얼이 키보드 몫을 놓는다"
    );
    window_frame(
        &ctx,
        &mut state,
        &engine.read(),
        &mgr,
        vec![key(Key::Enter, true), key(Key::Enter, false)],
    );
    assert_eq!(
        state.tutorial.active.map(|a| a.step),
        Some(0),
        "Enter 가 튜토리얼 다음 단추를 누르지 않는다"
    );
}
