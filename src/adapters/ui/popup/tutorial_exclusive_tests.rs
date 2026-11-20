//! 튜토리얼 말풍선은 포커스 노드가 팝업이 아닐 때만 키를 받는다. plugin 팝업이 포커스 노드가 되면 정규화가 튜토리얼의
//! 키보드 몫을 놓고, 말풍선은 그리지 않아 egui 포커스를 가진 단추도 Enter 로 눌리지 않는다.

use egui::{Context, Event, Key};

use super::repro::{manager_with_r_popup, plugin_frame};
use super::{PANE_R, layout, scene};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::state::RequestContext;
use crate::state::focus_node::FocusNode;

fn tutorial_frame(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
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
    drop(ctx.run(input, |ctx| {
        crate::adapters::ui::tutorial::draw_tutorial_overlay(
            ctx,
            state,
            engine,
            &layout(),
            screen,
            &theme,
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

#[test]
fn enter_does_not_press_the_tutorial_button_after_a_plugin_popup_takes_the_focus_node() {
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    let ctx = Context::default();
    // 첫 주제의 첫 단계는 콘텐츠 영역을 가리키는 읽기 단계라 다음 단추가 늘 켜져 있다.
    state.tutorial.request_start(0);
    state.tutorial.keyboard_focus = true;
    tutorial_frame(&ctx, &mut state, &engine.read(), Vec::new());
    tutorial_frame(&ctx, &mut state, &engine.read(), Vec::new());
    assert_eq!(state.tutorial.active.map(|a| a.step), Some(0));
    assert_eq!(state.focus_node(&engine.read()), FocusNode::Tutorial);
    assert!(
        ctx.memory(|m| m.focused().is_some()),
        "다음 단추가 egui 키 포커스를 가진다"
    );

    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    let placed = state.plugin_popup_hittest[0].rect;
    let mut input = WindowInput::default();
    input.press(&mut state, &engine.read(), placed.center());
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Plugin(super::repro::POPUP))
    );
    assert!(
        !state.tutorial.keyboard_focus,
        "팝업이 포커스 노드가 되면 튜토리얼이 키보드 몫을 놓는다"
    );
    drop(input.take_pass(&mut state, &ctx));
    state.end_press_frame();

    tutorial_frame(
        &ctx,
        &mut state,
        &engine.read(),
        vec![key(Key::Enter, true), key(Key::Enter, false)],
    );
    assert_eq!(
        state.tutorial.active.map(|a| a.step),
        Some(0),
        "Enter 가 튜토리얼 다음 단추를 누르지 않는다"
    );
}
