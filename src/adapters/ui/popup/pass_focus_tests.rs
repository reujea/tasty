//! 그리기 중 surface 본문이 키를 받을지는 그리는 입력 몫의 포커스 노드로 정한다(`MainViewState::pass_key_surface`).
//! 뒤 몫의 누름이 옮긴 지금 포커스로 앞 몫의 키를 판정하지 않는다. 탐색기 타입어헤드는 그 값을 읽고(`egui_panels` 와
//! 같은 식), html 주소 띠의 포커스 해제는 그 해제를 일으킨 몫의 패스에서 적용된다.
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
        Self { panel, view }
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
    let events = input.take_pass(&mut s.state, &s.ctx);
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
        // `egui_panels` 와 같은 값: 그 몫의 키를 본문으로 받는 surface.
        let key_surface = state.pass_key_surface(&read);
        let mut bands = std::mem::take(&mut state.html_address);
        crate::adapters::ui::popup::body_keys::with_body_keys(ctx, take, || match bodies {
            Bodies::Explorers(l, r) => {
                for (sid, rect, ex) in [
                    (SURFACE_L, surface_rect_l(), &mut **l),
                    (SURFACE_R, surface_rect_r(), &mut **r),
                ] {
                    draw_explorer(ctx, sid, rect, ex, key_surface == Some(sid));
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
                        let out = bands.draw(ui, &theme, SURFACE_L, band, &src);
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
    }));
    state.close_input_pass(&s.ctx);
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
            crate::explorer_ui::draw_explorer(
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

/// Given L html 주소 띠를 편집 중일 때, When 한 묶음에 [Enter, R 검색창 누름]이 오면, Then Enter 를 실은 앞 몫을 그릴
/// 때 띠가 아직 키 포커스를 가져 주소를 연다. 누름이 띠의 포커스를 푸는 것은 누름 몫의 패스에서다.
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
