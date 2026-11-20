//! 한 이벤트 묶음에 키와 누름이 섞여 들어올 때 키 하나의 주인은 그 키가 들어온 시점의 포커스 노드다. 창은 키가 실린
//! 몫 뒤의 누름에서 egui 입력을 끊고(`MainViewState::cut_key_frame`), 패스마다 앞 몫 하나를 그린다.
//!
//! 누름은 debug 주입과 같은 함수(`input_segment::press_focus`)로, 패스 입력은 egui 패스와 같은 분할 함수
//! (`InputCuts::split_pass`)로 밟는다(`window_input_test_support`). 창 winit 경로가 같은 함수를 부르는지는 소스 가드
//! `source_guards::press_focus_call_sites` 가 본다.

use egui::{Context, Event};

use super::repro::{
    POPUP, closes_of_popup, manager_with_r_popup, open_host_popup, plugin_frame,
    window_frame_with_find,
};
use super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, scene, surface_rect_l, surface_rect_r};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::adapters::ui::popup::{PopupScope, defs};
use crate::adapters::ui::search_bar::SEARCH_BAR_POPUP_ID;
use crate::plugin::PluginManager;
use crate::state::RequestContext;
use crate::state::focus_node::FocusNode;

fn text(s: &str) -> Event {
    Event::Text(s.into())
}

/// R 에 포커스를 가진 검색창을 열고 L 찾기 입력란을 그린 장면. 찾기 입력란과 검색창 입력칸의 자리를 돌려준다.
struct Scene {
    state: RequestContext,
    session: crate::runtime::engine_session::EngineSession,
    mgr: PluginManager,
    ctx: Context,
    search: crate::adapters::ui::popup::PopupInstanceId,
    find: egui::Rect,
    search_field: egui::Rect,
    query: String,
}

fn open_scene(extra: impl FnOnce(&mut RequestContext)) -> Scene {
    let (mut state, mut session) = scene(PANE_R);
    for def in defs::all_defs() {
        state.popups.register_def(def, 1.0);
    }
    // plugin 팝업이 없는 창이다. R 의 plugin 팝업은 R 을 덮어 R 의 누름을 모두 가져간다.
    let mgr = PluginManager::with_registries(
        std::sync::Arc::new(tasty_terminal::waker_factory::NoopWakerFactory),
        std::sync::Arc::new(crate::file::format::FileFormatRegistry::new()),
        std::sync::Arc::new(crate::file::handler::FileHandlerRegistry::new()),
    );
    let search = state
        .popups
        .open_request(
            &crate::intent::OpenRequest::new(SEARCH_BAR_POPUP_ID, PopupScope::Surface(SURFACE_R))
                .focused(),
            true,
        )
        .unwrap();
    extra(&mut state);
    let ctx = Context::default();
    let mut query = String::new();
    let (find, search_field) = {
        let engine = session.borrow_mut();
        let (_, find) = window_frame_with_find(
            &ctx,
            &mut state,
            &engine.read(),
            &mgr,
            Vec::new(),
            &mut query,
            true,
        );
        let find = find
            .and_then(|id| ctx.read_response(id))
            .map(|r| r.rect)
            .expect("찾기 입력란이 그려진다");
        window_frame_with_find(
            &ctx,
            &mut state,
            &engine.read(),
            &mgr,
            Vec::new(),
            &mut query,
            false,
        );
        // 포커스를 가진 검색창이 입력칸에 egui 키 포커스를 준다.
        let search_field = ctx
            .memory(|m| m.focused())
            .and_then(|id| ctx.read_response(id))
            .map(|r| r.rect)
            .expect("검색창 입력칸이 키 포커스를 가진다");
        assert!(surface_rect_l().contains(find.center()));
        assert!(surface_rect_r().contains(search_field.center()));
        assert_eq!(
            state.focus_node(&engine.read()),
            FocusNode::Popup(PopupNode::Host(search))
        );
        (find, search_field)
    };
    Scene {
        state,
        session,
        mgr,
        ctx,
        search,
        find,
        search_field,
        query,
    }
}

/// R 터미널의 빈 자리. 검색창 입력칸이 아니고 어느 팝업 노드(상자·덮는 범위)의 포인터 대상도 아니다.
fn terminal_point_r(scene: &Scene) -> egui::Pos2 {
    let r = surface_rect_r();
    let covered = |p: egui::Pos2| scene.search_field.contains(p) || scene.state.popup_owns_point(p);
    (1..20)
        .flat_map(|i| (1..20).map(move |j| (i, j)))
        .map(|(i, j)| {
            egui::pos2(
                r.min.x + r.width() * i as f32 / 20.0,
                r.min.y + r.height() * j as f32 / 20.0,
            )
        })
        .find(|p| !covered(*p))
        .expect("R 에 팝업이 덮지 않은 자리가 있다")
}

/// 창이 받은 글자 하나(`WindowInput::key`: 끊기 → egui 게이트 → 큐). 게이트 결과를 돌려준다.
fn type_text(s: &mut Scene, input: &mut WindowInput, egui_surface: bool, t: &str) -> Option<bool> {
    let engine = s.session.borrow_mut();
    input.key(&mut s.state, &engine.read(), egui_surface, text(t))
}

/// 창이 다음 패스 입력을 꺼내 한 프레임을 그린다.
fn pass(scene: &mut Scene, input: &mut WindowInput) {
    let events = input.take_pass(&mut scene.state, &scene.ctx);
    let engine = scene.session.borrow_mut();
    window_frame_with_find(
        &scene.ctx,
        &mut scene.state,
        &engine.read(),
        &scene.mgr,
        events,
        &mut scene.query,
        false,
    );
}

/// 남은 몫을 모두 그린다.
fn drain(scene: &mut Scene, input: &mut WindowInput) {
    while !input.is_empty() {
        pass(scene, input);
    }
}

/// L 을 누른 뒤의 click-to-activate(엔진 포커스 이동)를 창처럼 반영한다.
fn activate(scene: &mut Scene, pane: u32) {
    let engine = scene.session.borrow_mut();
    let read = engine.read();
    scene
        .state
        .navigation
        .select_pane(read.workspace_at(0).unwrap(), pane);
    scene.state.normalize_focus(&read);
}

/// (b) Given R 검색창이 포커스 노드일 때, When 한 묶음에 [R 키 x, L 찾기 입력란 누름, 글자 a]가 오면, Then x 는 R
/// 검색어에 실제로 들어가고 a 는 L 입력란에 들어간다. 창은 누름 자리와 누름 뒤 글자 자리에서 끊어 세 패스로 그린다.
#[test]
fn a_key_before_a_press_reaches_the_popup_and_the_letter_after_reaches_the_left_field() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    assert_eq!(type_text(&mut s, &mut input, false, "x"), Some(true));
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.find.center());
    }
    activate(&mut s, PANE_L);
    assert_eq!(
        type_text(&mut s, &mut input, true, "a"),
        Some(false),
        "누름 뒤 글자에는 팝업 키 주인이 없다"
    );
    assert!(!s.state.window_holds_terminal_keys());

    pass(&mut s, &mut input);
    assert_eq!(
        s.state.search.query, "x",
        "누름 앞 키는 R 검색어에 들어간다"
    );
    assert_eq!(s.query, "", "R 몫 키는 L 입력란에 들어가지 않는다");
    assert!(!input.is_empty(), "누름부터는 다음 패스 몫이다");
    pass(&mut s, &mut input);
    assert_eq!(s.query, "", "누름 몫에는 글자가 없다");
    assert!(!input.is_empty(), "누름 뒤 글자는 셋째 패스 몫이다");
    pass(&mut s, &mut input);
    assert_eq!(s.query, "a", "누름 뒤 글자는 L 입력란에 들어간다");
    assert_eq!(s.state.search.query, "x");
    assert!(input.is_empty());
    assert!(s.state.popups.instance(s.search).is_some_and(|p| p.open));
    assert_eq!(closes_of_popup(&s.state), []);
}

/// (a) Given L 찾기 입력란이 포커스 노드이고 R 검색창이 열린 채 포커스가 없을 때, When 한 묶음에 [L 글자 b, R 검색창
/// 누름, 글자 c]가 오면, Then b 는 L 입력란에, c 는 R 검색창 입력칸에 들어간다. 앞 몫의 키가 팝업에 묶이지 않았어도
/// 누름에서 끊고, 누름 뒤 글자에서 다시 끊는다. 누름 몫의 패스에서 입력칸이 egui 포커스를 얻은 뒤 c 를 실은 패스를
/// 그린다(egui 는 누름이 있는 패스에 직전 패스까지 포커스가 없던 입력칸에 그 패스의 글자를 넣지 않는다).
#[test]
fn a_letter_before_a_press_on_the_right_search_stays_with_the_left_field() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.find.center());
    }
    activate(&mut s, PANE_L);
    assert_eq!(type_text(&mut s, &mut input, true, "a"), Some(false));
    drain(&mut s, &mut input);
    assert_eq!(s.query, "a");
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Surface(Some(SURFACE_L))
        );
    }

    assert_eq!(type_text(&mut s, &mut input, true, "b"), Some(false));
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.search_field.center());
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Popup(PopupNode::Host(s.search))
        );
    }
    assert_eq!(
        type_text(&mut s, &mut input, false, "c"),
        Some(true),
        "누름 뒤 글자는 R 검색창에 묶인다"
    );

    pass(&mut s, &mut input);
    assert_eq!(s.query, "ab", "누름 앞 글자는 L 입력란에 들어간다");
    assert_eq!(
        s.state.search.query, "",
        "누름 앞 글자는 R 검색창에 들어가지 않는다"
    );
    assert_eq!(
        s.state.popups.frame_bound_key_owner(),
        Some(PopupNode::Host(s.search)),
        "누름 몫(다음 그리기)은 R 검색창 몫이다"
    );
    pass(&mut s, &mut input);
    assert!(!input.is_empty(), "누름 뒤 글자는 셋째 패스 몫이다");
    pass(&mut s, &mut input);
    assert!(input.is_empty());
    assert_eq!(
        s.state.search.query, "c",
        "누름 뒤 글자는 R 검색창 입력칸에 들어간다"
    );
    assert_eq!(s.query, "ab", "누름 뒤 글자는 L 입력란에 들어가지 않는다");
}

/// 전체화면 무대 패스는 팝업을 그리지 않지만 패스 하나가 몫 하나를 닫는다. Given L 찾기 입력란이 포커스 노드일 때, When
/// 무대 패스를 사이에 두고 [글자 s, L 찾기 입력란 누름]을 세 번 받은 뒤 무대를 나가 R 검색창을 누르고 글자 q 를 치면,
/// Then 무대 뒤 남은 몫은 없고 q 는 R 검색어에 들어간다.
#[test]
fn stage_passes_close_their_segments_so_a_key_after_the_stage_reaches_the_popup() {
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.find.center());
    }
    activate(&mut s, PANE_L);
    pass(&mut s, &mut input);
    for _ in 0..3 {
        assert_eq!(type_text(&mut s, &mut input, true, "s"), Some(false));
        {
            let engine = s.session.borrow_mut();
            input.press(&mut s.state, &engine.read(), s.find.center());
        }
        // 무대 패스: 입력을 꺼내고 팝업 층 없이 닫는다(`render_fullscreen_stage`).
        while !input.is_empty() {
            drop(input.take_pass(&mut s.state, &s.ctx));
            s.state.close_input_pass();
        }
    }
    assert!(
        s.state.segments_after_cut.is_empty(),
        "무대 뒤 남은 누름 몫이 없다"
    );
    assert!(
        s.state.popups.cut_segments_left() == 0,
        "무대 뒤 남은 몫이 없다"
    );

    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.search_field.center());
    }
    pass(&mut s, &mut input);
    pass(&mut s, &mut input);
    assert_eq!(type_text(&mut s, &mut input, false, "q"), Some(true));
    pass(&mut s, &mut input);
    assert_eq!(
        s.state.search.query, "q",
        "무대 뒤 글자는 R 검색창에 들어간다"
    );
}

/// 창의 한 패스처럼 다음 몫의 입력을 꺼내 host 팝업 층을 그리고(`frame::draw_popup_layer` 순서) 그 몫을 닫는다.
/// 본문 자리마다 그 본문이 받는 글자 사건을 기록해 돌려준다(키 주인이 아닌 본문은 키 문이 글자를 가린다). 정의 본문은
/// 팝업 층 밖에서 부르지 않으므로(`source_guards::popup_body_key_gate`) 본문 자리가 받는 입력을 기록한다.
fn recording_pass(
    s: &mut Scene,
    input: &mut WindowInput,
) -> Vec<(crate::adapters::ui::popup::PopupInstanceId, String)> {
    let events = input.take_pass(&mut s.state, &s.ctx);
    let engine = s.session.borrow_mut();
    let read = engine.read();
    let state = &mut s.state;
    let raw = egui::RawInput {
        screen_rect: Some(super::repro::screen()),
        events,
        ..Default::default()
    };
    let mut seen = Vec::new();
    let layout = super::layout();
    drop(s.ctx.run(raw, |ctx| {
        let press =
            crate::adapters::ui::popup::frame::frame_press(ctx, state, &read, Some(&layout));
        state.popups.prepare_frame_for_draw(ctx, Some(&layout));
        state.decide_frame_key_owner(Some(&layout));
        let mut popups =
            std::mem::replace(&mut state.popups, crate::adapters::ui::PopupManager::new());
        let result = popups.draw_routed(
            ctx,
            &mut |cx, ui| {
                ui.input(|i| {
                    for e in &i.events {
                        if let Event::Text(t) = e {
                            seen.push((cx.instance, t.clone()));
                        }
                    }
                });
            },
            Some(&layout),
            &[],
            press.as_ref(),
        );
        state.popups = popups;
        state.host_popup_hittest = result.hit_rects;
        state.host_popup_covers = result.covers;
    }));
    s.state.close_input_pass();
    seen
}

/// (c) Given R 검색창이 포커스 노드이고 L 에 덮는 팝업이 열려 있을 때, When 한 묶음에 [R 키 x, L 팝업 누름, 키 y, R
/// 터미널 누름, 글자 z]가 오면, Then 창은 누름과 누름 뒤 키마다 끊어 x 는 R 검색창 본문이, y 는 L 팝업 본문이 받고,
/// z 는 창의 게이트가 egui 로 보내지 않아 어느 팝업 본문도 받지 않는다.
#[test]
fn every_press_after_a_key_cuts_and_the_last_letter_reaches_the_terminal() {
    let mut cover_iid = None;
    let mut s = open_scene(|state| {
        cover_iid = Some(open_host_popup(state, SURFACE_L, false));
    });
    let cover_iid = cover_iid.unwrap();
    let cover = s
        .state
        .host_popup_hittest
        .iter()
        .map(|o| o.rect)
        .find(|r| surface_rect_l().contains(r.center()))
        .expect("L 의 덮는 팝업이 그려진다");
    let mut input = WindowInput::default();
    assert_eq!(type_text(&mut s, &mut input, false, "x"), Some(true));
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), cover.center());
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Popup(PopupNode::Host(cover_iid)),
            "L 팝업이 포커스 노드다"
        );
    }
    assert_eq!(type_text(&mut s, &mut input, false, "y"), Some(true));
    let terminal = terminal_point_r(&s);
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), terminal);
    }
    activate(&mut s, PANE_R);
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Surface(Some(SURFACE_R))
        );
    }
    assert_eq!(
        type_text(&mut s, &mut input, false, "z"),
        None,
        "마지막 글자는 egui 로 가지 않는다"
    );
    assert!(
        !s.state.window_holds_terminal_keys(),
        "마지막 글자는 터미널 게이트를 지난다"
    );

    let mut seen = Vec::new();
    while !input.is_empty() {
        seen.extend(recording_pass(&mut s, &mut input));
    }
    assert_eq!(
        seen,
        [(s.search, "x".to_string()), (cover_iid, "y".to_string())],
        "x 는 R 검색창 본문, y 는 L 팝업 본문이 받고 z 는 어느 본문도 받지 않는다"
    );
}

/// (d) Given R 의 plugin 팝업이 포커스 노드이고 창이 그 팝업에 키 x 를 묶었을 때, When 같은 묶음에서 L 터미널을
/// 누르면, Then 누름 뒤 글자와 IME 는 L 터미널(PTY) 쪽으로 간다. 입력 시점 판정(`plugin_popup_key_target`, 창 게이트,
/// IME 오버레이 게이트)은 지금 받는 몫을 읽고 앞 몫의 R 결속을 읽지 않는다. 앞 몫은 그 그리기에서 R 이 받는다.
#[test]
fn a_press_on_the_left_terminal_after_a_plugin_popup_key_sends_the_next_letter_to_the_terminal() {
    let (mut state, mut session) = scene(PANE_R);
    let engine = session.borrow_mut();
    let mgr = manager_with_r_popup(tasty_host_plugin::next_popup_z_seq());
    let ctx = Context::default();
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, Vec::new());
    let placed = state.plugin_popup_hittest[0].rect;
    let mut input = WindowInput::default();
    input.press(&mut state, &engine.read(), placed.center());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Plugin(POPUP))
    );
    let events = input.take_pass(&mut state, &ctx);
    plugin_frame(&ctx, &mut state, &engine.read(), &mgr, events);
    state.close_input_pass();

    assert_eq!(state.window_feeds_egui(false, true), Some(true));
    input.push(text("x"));
    assert_eq!(state.plugin_popup_key_target(), Some(POPUP));
    input.press(&mut state, &engine.read(), surface_rect_l().center());
    {
        let read = engine.read();
        state
            .navigation
            .select_pane(read.workspace_at(0).unwrap(), PANE_L);
    }
    state.normalize_focus(&engine.read());
    assert_eq!(
        state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
    assert_eq!(
        state.plugin_popup_key_target(),
        None,
        "입력 시점의 plugin 키 대상은 지금 몫을 읽는다"
    );
    assert_eq!(
        state.window_feeds_egui(false, true),
        None,
        "글자와 IME 는 egui 로 가지 않는다(IME 경로가 읽는 오버레이 게이트가 닫혀 있다)"
    );
    assert!(
        !state.window_holds_terminal_keys(),
        "글자는 터미널 게이트를 지난다"
    );
    assert_eq!(
        state.popups.frame_bound_key_owner(),
        Some(PopupNode::Plugin(POPUP)),
        "앞 몫은 그 그리기에서 R 이 받는다"
    );
}
