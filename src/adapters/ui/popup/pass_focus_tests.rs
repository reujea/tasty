//! 그리기 중 surface 본문과 html 주소 입력칸이 키를 받을지는 그리는 입력 몫의 키 노드로 정한다
//! (`MainViewState::pass_key_node`). 뒤 몫의 사건이 옮긴 지금 포커스로 앞 몫의 키를 판정하지 않는다. 탐색기 타입어헤드와
//! 주소 입력칸은 그 값을 읽는다(`egui_panels` 와 같은 식). 주소 띠의 소유권은 입력 시점에 놓고, 입력칸의 편집은 놓기
//! 전 몫을 그린 뒤에 끝낸다.
//!
//! 입력은 창과 같은 끊기·게이트(`WindowInput`), 패스는 egui 패스와 같은 분할(`WindowInput::take_pass`)로 밟는다.
//! 그리기는 창처럼 surface 본문(본문 키 게이트 `with_body_keys` 안) → host 팝업 층 → plugin 팝업 순서다.

use std::collections::HashSet;
use std::path::PathBuf;

use egui::Event;

use super::press_cut::{Scene, activate, open_scene, terminal_point_r, type_text};
use super::repro::screen;
use super::{PANE_L, PANE_R, SURFACE_L, SURFACE_R, layout, surface_rect_l, surface_rect_r};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::frame::draw_popup_layer;
use crate::adapters::ui::popup::window_input_test_support::WindowInput;
use crate::adapters::ui::surface::html_address::HtmlAddressSource;
use crate::explorer_ui::ExplorerInput;
use crate::explorer_ui::view::{DirEntryInfo, ExplorerView, LoadState};
use crate::plugin_bridge::popup_render::draw_plugin_popups;
use crate::state::focus_node::FocusNode;
use crate::webview::WebViewUserNav;

/// L·R 에 그리는 탐색기 하나. 항목은 apple.txt·banana.txt 다.
struct Explorer {
    panel: tasty_model::ExplorerPanel,
    view: ExplorerView,
    /// 그리기가 낸 액션(파일 열기 등)의 Debug 글.
    actions: Vec<String>,
}

impl Explorer {
    fn new(id: u32) -> Self {
        crate::i18n::init("en");
        let dir = format!("/tasty-test-no-such-dir-{id}");
        let panel = tasty_model::ExplorerPanel::new(id, PathBuf::from(&dir));
        let mut view = ExplorerView::new();
        view.sync(&panel, None);
        view.state = LoadState::Ok;
        view.entries = ["apple.txt", "banana.txt"]
            .into_iter()
            .map(|name| DirEntryInfo {
                path: PathBuf::from(format!("{dir}/{name}")),
                name: name.into(),
                is_dir: false,
                size: 0,
                modified: None,
                ext: "txt".into(),
                ..Default::default()
            })
            .collect();
        Self {
            panel,
            view,
            actions: Vec::new(),
        }
    }

    /// 고른 항목의 이름들.
    fn selected(&self) -> String {
        self.view.selected_paths_text().unwrap_or_default()
    }
}

/// surface 본문 자리에 그릴 것.
enum Bodies<'a> {
    Explorers(&'a mut Explorer, &'a mut Explorer),
    Address(&'a mut Vec<WebViewUserNav>),
}

/// 창의 egui 패스 하나: 다음 몫 입력을 꺼내 surface 본문 → host 팝업 층 → plugin 팝업을 그리고 몫을 닫는다.
fn pass(s: &mut Scene, input: &mut WindowInput, bodies: &mut Bodies<'_>) {
    pass_with(s, input, bodies, &mut |_| {});
}

/// `pass` 와 같고, 그리는 중(plugin 팝업 뒤) `during` 을 부른다. 그리기 중 무대를 여닫는 버튼(무대 종료 버튼,
/// 팝업 무대 버튼)처럼 패스 안에서 상태를 바꾸는 자리다. 이 패스에 실제로 든 사건을 돌려준다.
fn pass_with(
    s: &mut Scene,
    input: &mut WindowInput,
    bodies: &mut Bodies<'_>,
    during: &mut dyn FnMut(&mut crate::state::RequestContext),
) -> Vec<Event> {
    let events = input.take_pass(&mut s.state, &s.ctx);
    let drawn = events.clone();
    let screen_drawn = s.state.fullscreen_stage_id();
    let engine = s.session.borrow_mut();
    let read = engine.read();
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        events,
        ..Default::default()
    };
    let state = &mut s.state;
    crate::plugin_bridge::popup_scope::sync_plugin_popup_scopes(state, s.mgr.popup_instances());
    drop(s.ctx.run(raw, |ctx| {
        let take = state.surfaces_take_keys();
        // `egui_panels` 와 같은 값: 그 몫의 키를 받는 포커스 노드.
        let key_node = state.pass_key_node(&read);
        let notice = state.pass_notice_focus();
        let mut bands = std::mem::take(&mut state.html_address);
        crate::adapters::ui::popup::body_keys::with_body_keys(ctx, take, || match bodies {
            Bodies::Explorers(l, r) => {
                for (sid, rect, ex) in [
                    (SURFACE_L, surface_rect_l(), &mut **l),
                    (SURFACE_R, surface_rect_r(), &mut **r),
                ] {
                    draw_explorer(
                        ctx,
                        sid,
                        rect,
                        ex,
                        crate::state::focus_node::explorer_takes_keys(Some(sid), key_node, notice),
                    );
                }
            }
            Bodies::Address(navs) => {
                let info = tasty_model::WebViewNavInfo::default();
                let src = HtmlAddressSource {
                    nav: crate::model::NavState::Done,
                    info: &info,
                    host_url: None,
                    cwd: None,
                    home: None,
                    mirror: false,
                    received: None,
                    remote_marker: None,
                    web_page: false,
                    native: &[],
                };
                let rect = surface_rect_l();
                egui::Area::new(egui::Id::new("pass_focus_address"))
                    .fixed_pos(rect.min)
                    .show(ctx, |ui| {
                        ui.set_width(rect.width());
                        let band =
                            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 40.0));
                        let theme = tasty_themes::mocha_fallback();
                        let out = bands.draw(
                            ui,
                            &theme,
                            SURFACE_L,
                            band,
                            &src,
                            key_node == Some(FocusNode::HtmlAddress(SURFACE_L)),
                        );
                        navs.extend(out.nav);
                    });
            }
        });
        bands.finish_frame();
        state.html_address = bands;
        draw_popup_layer(ctx, state, &read, &layout());
        draw_plugin_popups(
            ctx,
            state,
            &read,
            Some(crate::app::plugin_display::PluginDisplay::new(&s.mgr)),
            Some(&layout()),
        );
        during(state);
    }));
    input.close_pass(state, &s.ctx, screen_drawn);
    drawn
}

fn draw_explorer(
    ctx: &egui::Context,
    sid: u32,
    rect: egui::Rect,
    ex: &mut Explorer,
    takes_keys: bool,
) {
    let font = crate::settings::EffectiveFont {
        font_family: String::new(),
        font_size: 13.0,
        custom_font_path: String::new(),
        line_height: 1.0,
        font_scale_mode: String::new(),
    };
    let chars = HashSet::new();
    egui::Area::new(egui::Id::new(("pass_focus_explorer", sid)))
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            ui.set_width(rect.width());
            ui.set_height(rect.height());
            let action = crate::explorer_ui::draw_explorer(
                ui,
                &ex.panel,
                &mut ex.view,
                &font,
                &sid.to_string(),
                &[],
                &HashSet::new(),
                &[],
                None,
                &ExplorerInput {
                    takes_keys,
                    shortcut_chars: &chars,
                },
            );
            ex.actions.extend(action.map(|a| format!("{a:?}")));
        });
}

fn drain(s: &mut Scene, input: &mut WindowInput, bodies: &mut Bodies<'_>) {
    while !input.is_empty() {
        pass(s, input, bodies);
    }
}

/// L 을 눌러 포커스 노드를 L 로 옮긴다.
fn focus_left(s: &mut Scene, input: &mut WindowInput, bodies: &mut Bodies<'_>) {
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), surface_rect_l().center());
    }
    activate(s, PANE_L);
    drain(s, input, bodies);
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_L))
    );
}

/// Given L 탐색기가 포커스 노드이고 R 에 검색창이 열려 있을 때, When 한 묶음에 [L 목록 글자 b, R 검색창 누름]이
/// 오면, Then b 를 실은 앞 몫을 그릴 때 L 타입어헤드가 b 로 banana 를 고른다. 누름이 옮긴 지금 포커스(검색창)는
/// 앞 몫의 판정에 쓰지 않는다.
#[test]
fn a_letter_before_a_popup_press_selects_in_the_left_explorer() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    let mut input = WindowInput::default();
    focus_left(&mut s, &mut input, &mut bodies);

    assert!(type_text(&mut s, &mut input, true, "b").is_some());
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.search_field.center());
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Popup(PopupNode::Host(s.search))
        );
    }
    pass(&mut s, &mut input, &mut bodies);
    let Bodies::Explorers(l, r) = &bodies else {
        unreachable!()
    };
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
    assert_eq!(r.selected(), "", "R 탐색기는 L 몫 글자를 읽지 않는다");
    drain(&mut s, &mut input, &mut bodies);
    assert_eq!(
        s.state.search.query, "",
        "검색창은 누름 앞 글자를 받지 않는다"
    );
}

/// Given L 탐색기가 포커스 노드일 때, When 한 묶음에 [L 글자 b, R 탐색기 누름, 글자 a]가 오면, Then b 는 L 이, a 는 R
/// 이 받는다. 앞 몫을 그릴 때 지금 포커스(R)가 L 몫 글자를 읽지 않는다.
#[test]
fn letters_around_a_press_on_another_explorer_go_to_their_own_explorer() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut bodies = Bodies::Explorers(&mut l, &mut r);
    let mut input = WindowInput::default();
    focus_left(&mut s, &mut input, &mut bodies);
    let point_r = terminal_point_r(&s);

    assert!(type_text(&mut s, &mut input, true, "b").is_some());
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point_r);
    }
    activate(&mut s, PANE_R);
    assert!(type_text(&mut s, &mut input, true, "a").is_some());
    pass(&mut s, &mut input, &mut bodies);
    {
        let Bodies::Explorers(l, r) = &bodies else {
            unreachable!()
        };
        assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
        assert!(!r.selected().contains("banana"), "R 이 L 몫 글자를 읽었다");
    }
    drain(&mut s, &mut input, &mut bodies);
    let Bodies::Explorers(l, r) = &bodies else {
        unreachable!()
    };
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
    assert!(r.selected().contains("apple"), "R: {:?}", r.selected());
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Surface(Some(SURFACE_R))
    );
}

/// Given L html 주소 띠를 편집 중일 때, When 한 묶음에 [Enter, R 검색창 누름]이 오면, Then 누름이 띠의 입력 소유권을
/// 바로 놓아도 Enter 를 실은 앞 몫(주소 입력칸 노드)을 그릴 때 입력칸이 Enter 를 받아 주소를 연다.
#[test]
fn an_address_enter_before_a_popup_press_opens_the_address() {
    let mut s = open_scene(|_| {});
    let mut navs = Vec::new();
    let mut bodies = Bodies::Address(&mut navs);
    let mut input = WindowInput::default();
    focus_left(&mut s, &mut input, &mut bodies);
    s.state.html_address.request_focus(SURFACE_L);
    pass(&mut s, &mut input, &mut bodies);
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::HtmlAddress(SURFACE_L)
        );
    }
    assert_eq!(
        type_text(&mut s, &mut input, false, "https://example.com/"),
        Some(false)
    );
    drain(&mut s, &mut input, &mut bodies);

    let enter = Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    {
        let engine = s.session.borrow_mut();
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, enter)
                .is_some()
        );
        input.press(&mut s.state, &engine.read(), s.search_field.center());
    }
    drain(&mut s, &mut input, &mut bodies);
    let Bodies::Address(navs) = &bodies else {
        unreachable!()
    };
    assert_eq!(
        **navs,
        [WebViewUserNav::Open("https://example.com/".into())]
    );
    assert!(!s.state.html_address.has_keyboard_focus());
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(s.search))
    );
}

/// Given 메인 창에 debug 주입으로 [포인터 누름, 키 Enter]를 넣으면, Then 키 주입도 창의 키처럼 누름 뒤에서 입력 몫을
/// 끊어(`debug_input::inject_key` → `key_focus`) 두 패스로 나뉜다. 누름 패스에는 키가 없고 Enter 는 다음 패스 몫이다.
#[cfg(debug_assertions)]
#[test]
fn an_injected_key_after_an_injected_press_is_the_next_pass() {
    use crate::view::main::debug_input::{PushedKey, inject_key};
    let mut s = open_scene(|_| {});
    let mut input = WindowInput::default();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.search_field.center());
        assert_eq!(
            inject_key(
                &mut s.state,
                &engine.read(),
                &mut input,
                "Enter",
                true,
                None
            ),
            Some(PushedKey::Key)
        );
    }
    let is_key = |e: &Event| matches!(e, Event::Key { .. });
    let first = input.take_pass(&mut s.state, &s.ctx);
    assert!(
        first
            .iter()
            .any(|e| matches!(e, Event::PointerButton { pressed: true, .. })),
        "{first:?}"
    );
    assert!(
        !first.iter().any(is_key),
        "누름 패스에 키가 있다: {first:?}"
    );
    s.state.close_input_pass(&s.ctx);
    let second = input.take_pass(&mut s.state, &s.ctx);
    assert!(
        matches!(
            second[..],
            [Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                ..
            }]
        ),
        "{second:?}"
    );
    s.state.close_input_pass(&s.ctx);
    assert!(input.is_empty());
    let engine = s.session.borrow_mut();
    assert_eq!(
        inject_key(
            &mut s.state,
            &engine.read(),
            &mut input,
            "NoSuchKey",
            true,
            None
        ),
        None
    );
    assert!(input.is_empty(), "매핑 불가 키는 끊지도 넣지도 않는다");
}

/// L·R 탐색기를 그린 화면에서 `name` 항목 줄의 가운데.
fn row_center(s: &mut Scene, l: &mut Explorer, r: &mut Explorer, name: &str) -> egui::Pos2 {
    let raw = egui::RawInput {
        screen_rect: Some(screen()),
        ..Default::default()
    };
    let out = s.ctx.run(raw, |ctx| {
        for (sid, rect, ex) in [
            (SURFACE_L, surface_rect_l(), &mut *l),
            (SURFACE_R, surface_rect_r(), &mut *r),
        ] {
            draw_explorer(ctx, sid, rect, ex, false);
        }
    });
    out.shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t)
                if t.galley.text() == name && surface_rect_l().contains(t.pos) =>
            {
                Some(t.visual_bounding_rect().center())
            }
            _ => None,
        })
        .expect("L 에 항목 줄이 그려진다")
}

/// 창의 패스 하나를 창과 같은 경로 선택(`pass_draws_stage`, 지금 무대 상태)으로 그린다. 무대면 무대 그리기, 아니면 탐색기 화면이다. 패스
/// 입력은 창처럼 몫 노드가 그 화면에 없으면 사용자 입력을 뺀 것이다(`WindowInput::take_pass`).
fn stage_or_window_pass(
    s: &mut Scene,
    input: &mut WindowInput,
    l: &mut Explorer,
    r: &mut Explorer,
) -> bool {
    stage_or_window_pass_with(s, input, l, r, &mut |_| {}).0
}

/// `stage_or_window_pass` 와 같고, 그리는 중 `during` 을 부른다(그리기 중 화면 전환). 무대 패스였는지와 이 패스에
/// 실제로 든 사건을 돌려준다.
fn stage_or_window_pass_with(
    s: &mut Scene,
    input: &mut WindowInput,
    l: &mut Explorer,
    r: &mut Explorer,
    during: &mut dyn FnMut(&mut crate::state::RequestContext),
) -> (bool, Vec<Event>) {
    let stage = s.state.pass_draws_stage();
    if stage {
        let events = input.take_pass(&mut s.state, &s.ctx);
        let drawn = events.clone();
        let screen_drawn = s.state.fullscreen_stage_id();
        let engine = s.session.borrow_mut();
        let read = engine.read();
        let raw = egui::RawInput {
            screen_rect: Some(screen()),
            events,
            ..Default::default()
        };
        let state = &mut s.state;
        drop(s.ctx.run(raw, |ctx| {
            crate::adapters::ui::draw_fullscreen_stage(ctx, state, &read);
            during(state);
        }));
        input.close_pass(state, &s.ctx, screen_drawn);
        (stage, drawn)
    } else {
        let drawn = pass_with(s, input, &mut Bodies::Explorers(l, r), during);
        (stage, drawn)
    }
}

/// Given 무대가 창을 덮고 L 탐색기가 그 뒤에 있을 때, When 그리기 전 한 묶음에 [글자 b, 무대 안 누름(뒤 L 의 banana
/// 줄 자리), 무대 종료 키]가 오고 종료 키가 무대를 닫으면, Then 세 사건의 몫(글자·누름·종료 키, 모두 무대 노드에서
/// 들어왔다)은 닫힌 무대 대신 일반 화면으로 그리지만 그 입력은 빠져 L 은 글자도 누름도 받지 않는다.
#[test]
fn segments_of_the_closed_stage_draw_the_window_without_their_input() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
    }
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));

    assert!(type_text(&mut s, &mut input, true, "b").is_some());
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), banana);
    }
    input.queue_mut().push(Event::PointerButton {
        pos: banana,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    let escape = Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    {
        let engine = s.session.borrow_mut();
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, escape)
                .is_some()
        );
    }
    // 창의 무대 종료 키 처리(`try_consume_fullscreen_stage_key`)가 무대를 닫는다.
    assert!(s.state.close_fullscreen_stage());

    let letter = stage_or_window_pass(&mut s, &mut input, &mut l, &mut r);
    let press = stage_or_window_pass(&mut s, &mut input, &mut l, &mut r);
    let exit = stage_or_window_pass(&mut s, &mut input, &mut l, &mut r);
    assert_eq!(
        l.selected(),
        "",
        "무대 몫의 글자·누름이 뒤 L 탐색기에 닿았다"
    );
    assert!(
        !letter && !press && !exit,
        "닫힌 무대의 몫은 일반 화면으로 그린다"
    );
    assert!(input.is_empty());
    assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    assert_eq!(l.selected(), "");
    // 대조: 일반 화면에서 같은 자리를 누르면 L 이 banana 를 고른다(누름이 닿으면 보이는 자리다). 무대 몫의 누름과
    // egui 더블클릭(0.3초)·세 번 클릭(0.6초) 간격 안에 묶이면 열기 갈래로 가므로, 시간을 준 적 없는 패스마다 egui 가
    // 예측 간격(1/60초)만큼 시간을 진행하는 것을 써서 빈 패스로 그 간격을 넘긴다.
    for _ in 0..45 {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), banana);
    }
    input.queue_mut().push(Event::PointerButton {
        pos: banana,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    while !input.is_empty() {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    assert!(l.selected().contains("banana"), "L: {:?}", l.selected());
}

/// Given 무대가 창을 덮고 L 탐색기가 그 뒤에 있을 때, When 그리기 전 한 묶음에 누름 없이 [목록 글자 b, 무대 종료 Esc]가
/// 오고 종료 키가 무대를 닫으면, Then 두 사건은 무대 노드에서 든 한 몫이고 그 패스는 일반 화면을 그리지만(무대
/// 렌더러로 가지 않는다) L 타입어헤드는 b 를 읽지 않는다. 그 뒤 글자 a 는 사건 시점의 노드(L)가 무대와 달라 새 몫이
/// 되고 L 이 받는다.
#[test]
fn a_letter_and_the_stage_exit_key_without_a_press_draw_the_window_without_the_letter() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
    }
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));

    assert!(type_text(&mut s, &mut input, true, "b").is_some());
    let escape = Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    {
        let engine = s.session.borrow_mut();
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, escape)
                .is_some()
        );
    }
    // 창의 무대 종료 키 처리(`try_consume_fullscreen_stage_key`)가 무대를 닫는다. 전이는 즉시 적용된다.
    assert!(s.state.close_fullscreen_stage());
    assert!(type_text(&mut s, &mut input, true, "a").is_some());

    {
        let engine = s.session.borrow_mut();
        assert_eq!(s.state.pass_focus_node(&engine.read()), FocusNode::Stage);
        assert_eq!(s.state.pass_key_node(&engine.read()), None);
    }
    assert!(
        !stage_or_window_pass(&mut s, &mut input, &mut l, &mut r),
        "글자와 종료 키 몫(무대 노드)은 닫힌 무대 대신 일반 화면으로 그린다"
    );
    assert_eq!(l.selected(), "", "무대 몫의 글자가 뒤 L 탐색기에 닿았다");
    while !input.is_empty() {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    assert!(
        l.selected().contains("apple"),
        "무대 뒤 글자는 L 이 받는다: {:?}",
        l.selected()
    );
}

/// 왼쪽 버튼 뗌.
fn release_at(pos: egui::Pos2) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }
}

/// 창의 키 누름 하나(`key`).
fn key_press(key: egui::Key) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Given 무대가 창을 덮을 때, When 무대 안 누름을 무대 패스로 그린 뒤 [무대 종료 Esc, 버튼 뗌]이 오고 종료 키가 무대를
/// 닫으면, Then 뗌은 닫힌 무대 몫(일반 화면으로 그린다)에서도 빠지지 않아 무대에서 시작한 egui 의 눌림이 풀린다.
#[test]
fn a_release_after_the_stage_closed_ends_the_press_drawn_on_the_stage() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
    }
    assert!(s.state.open_fullscreen_stage("blank"));
    assert!(stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    let point = screen().center();
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), point);
    }
    assert!(
        stage_or_window_pass(&mut s, &mut input, &mut l, &mut r),
        "누름은 무대 패스로 그린다"
    );
    assert!(
        s.ctx.input(|i| i.pointer.any_down()),
        "무대가 누름을 받았다"
    );
    {
        let engine = s.session.borrow_mut();
        let escape = key_press(egui::Key::Escape);
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, escape)
                .is_some()
        );
    }
    // 창의 무대 종료 키 처리(`try_consume_fullscreen_stage_key`)가 무대를 닫는다.
    assert!(s.state.close_fullscreen_stage());
    input.queue_mut().push(release_at(point));
    while !input.is_empty() {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    assert!(
        !s.ctx.input(|i| i.pointer.any_down()),
        "닫힌 무대 몫의 뗌이 빠져 버튼이 눌린 채로 남았다"
    );
    assert_eq!(l.selected(), "", "무대 몫의 누름이 뒤 L 탐색기에 닿았다");
}

/// Given 일반 화면에서 L 의 banana 줄을 누른 채일 때, When 키 하나가 무대를 열고 버튼 뗌이 오면, Then 키 몫(L 노드)은
/// 열린 무대로 그려 키는 빠지지만 뗌은 남아 일반 화면에서 시작한 egui 의 눌림이 풀린다.
#[test]
fn a_release_after_a_key_opened_the_stage_ends_the_press_drawn_on_the_window() {
    let mut s = open_scene(|_| {});
    let (mut l, mut r) = (Explorer::new(1), Explorer::new(2));
    let mut input = WindowInput::default();
    {
        let mut bodies = Bodies::Explorers(&mut l, &mut r);
        focus_left(&mut s, &mut input, &mut bodies);
    }
    let banana = row_center(&mut s, &mut l, &mut r, "banana.txt");
    for _ in 0..45 {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), banana);
    }
    while !input.is_empty() {
        assert!(!stage_or_window_pass(&mut s, &mut input, &mut l, &mut r));
    }
    assert!(s.ctx.input(|i| i.pointer.any_down()), "L 이 누름을 받았다");
    {
        let engine = s.session.borrow_mut();
        let key = key_press(egui::Key::F11);
        assert!(input.key(&mut s.state, &engine.read(), true, key).is_some());
    }
    // 창의 키 처리가 무대를 연다. 전이는 즉시 적용된다.
    assert!(s.state.open_fullscreen_stage("blank"));
    input.queue_mut().push(release_at(banana));
    assert!(
        stage_or_window_pass(&mut s, &mut input, &mut l, &mut r),
        "키 몫은 열린 무대로 그린다"
    );
    while !input.is_empty() {
        stage_or_window_pass(&mut s, &mut input, &mut l, &mut r);
    }
    assert!(
        !s.ctx.input(|i| i.pointer.any_down()),
        "열린 무대로 그린 몫의 뗌이 빠져 버튼이 눌린 채로 남았다"
    );
}

/// 주소 띠를 그리는 L 에 포커스를 두고 띠에 키 포커스를 준다.
fn focus_address(s: &mut Scene, input: &mut WindowInput, bodies: &mut Bodies<'_>) {
    focus_left(s, input, bodies);
    s.state.html_address.request_focus(SURFACE_L);
    pass(s, input, bodies);
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::HtmlAddress(SURFACE_L)
    );
}

/// Given L html 주소 띠에 있는 로컬 파일 경로를 입력했을 때, When 한 묶음에 [Enter, R 검색창 누름]이 오면, Then Enter
/// 를 실은 앞 몫을 그릴 때 띠가 확정해 파일 확인을 시작하고, 누름이 띠의 소유권을 놓아 편집이 끝나도 확인 결과가 오면
/// 그 파일을 연다.
#[test]
fn a_local_file_address_enter_before_a_popup_press_opens_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("page.html");
    std::fs::write(&file, "<html></html>").expect("page");
    let mut s = open_scene(|_| {});
    let mut navs = Vec::new();
    let mut bodies = Bodies::Address(&mut navs);
    let mut input = WindowInput::default();
    focus_address(&mut s, &mut input, &mut bodies);
    let typed = file.to_string_lossy().into_owned();
    assert_eq!(type_text(&mut s, &mut input, false, &typed), Some(false));
    drain(&mut s, &mut input, &mut bodies);

    let enter = Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    {
        let engine = s.session.borrow_mut();
        assert!(
            input
                .key(&mut s.state, &engine.read(), false, enter)
                .is_some()
        );
        input.press(&mut s.state, &engine.read(), s.search_field.center());
        assert!(
            !s.state.html_address.has_keyboard_focus(),
            "누름이 띠의 입력 소유권을 바로 놓는다"
        );
    }
    drain(&mut s, &mut input, &mut bodies);
    // 파일 확인은 작업 스레드가 한다. 결과가 올 때까지 빈 패스로 띠를 다시 그린다.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Bodies::Address(navs) = &bodies
            && !navs.is_empty()
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "파일 확인 결과로 열지 않았다"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        pass(&mut s, &mut input, &mut bodies);
    }
    let Bodies::Address(navs) = &bodies else {
        unreachable!()
    };
    let url = url::Url::from_file_path(&file)
        .expect("file url")
        .to_string();
    assert_eq!(**navs, [WebViewUserNav::Open(url)]);
    let engine = s.session.borrow_mut();
    assert_eq!(
        s.state.focus_node(&engine.read()),
        FocusNode::Popup(PopupNode::Host(s.search))
    );
}

/// Given L html 주소 띠를 편집 중이고 R 에 덮지 않는 검색창이 있을 때, When 그리기 전 한 묶음에 [주소 글자, 검색창
/// 누름, R 터미널 누름, 글자]가 오면, Then 첫 누름이 띠의 입력 소유권을 바로 놓으므로 뒤 글자는 입력 시점에 egui 로
/// 가지 않고 R 터미널(PTY) 갈래로 간다. 주소 글자는 놓기 전 몫을 그릴 때 입력칸에 들어간다.
#[test]
fn a_letter_after_a_popup_press_and_a_terminal_press_goes_to_the_terminal() {
    use crate::state::input_destination::InputDestination;
    let mut s = open_scene(|_| {});
    let mut navs = Vec::new();
    let mut bodies = Bodies::Address(&mut navs);
    let mut input = WindowInput::default();
    focus_address(&mut s, &mut input, &mut bodies);
    let point_r = terminal_point_r(&s);

    assert_eq!(type_text(&mut s, &mut input, false, "q"), Some(false));
    {
        let engine = s.session.borrow_mut();
        input.press(&mut s.state, &engine.read(), s.search_field.center());
        input.press(&mut s.state, &engine.read(), point_r);
    }
    activate(&mut s, PANE_R);
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state.focus_node(&engine.read()),
            FocusNode::Surface(Some(SURFACE_R))
        );
        assert!(!s.state.html_address.has_keyboard_focus());
    }
    assert_eq!(
        type_text(&mut s, &mut input, false, "z"),
        None,
        "뒤 글자는 egui 로 가지 않는다"
    );
    {
        let engine = s.session.borrow_mut();
        assert_eq!(
            s.state
                .input_destination(&engine.read(), s.state.window_holds_terminal_keys()),
            InputDestination::Terminal(SURFACE_R),
            "뒤 글자 갈래는 R 터미널(PTY)이다"
        );
    }
    drain(&mut s, &mut input, &mut bodies);
    assert!(
        !s.state.html_address.has_keyboard_focus(),
        "앞 몫을 그린 띠가 입력 소유권을 다시 얻지 않는다"
    );
}

#[path = "pass_focus_release_tests.rs"]
mod release;

#[path = "pass_focus_notice_tests.rs"]
mod notice;

#[path = "pass_focus_transition_tests.rs"]
mod transition;

#[path = "pass_focus_paste_tests.rs"]
mod paste;

#[path = "pass_focus_seal_tests.rs"]
mod seal;
