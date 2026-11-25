//! 별도 창 입력 패스(§1.1) 장면. 창 대역은 사건마다 제품 단계를 밟는다: 단계 3~5(`settle_window_input`), egui 큐 적재,
//! 적재한 사건의 몫 결속과 bare Esc 적용(`bind_window_input`). 패스는 제품과 같은 순서 함수(`run_input_pass`)와 창
//! 상태(`WindowPass`)로 돌고, 입력은 끊은 자리에서 나눈다. 그리기는 설정 창처럼 관리자에 패스 키 값을 넣고 창 위젯을
//! 그린 뒤 팝업을 그린다.

use super::*;
use crate::adapters::ui::popup::window_segments::WindowKey;
use crate::adapters::ui::popup::{PopupId, PopupScope, PopupState};
use crate::gfx::gpu::input_pass::{InputPassHost, run_input_pass};
use crate::gfx::gpu::text_selection::HeldPastes;
use crate::intent::OpenRequest;

const A: PopupId = "window_input_a";
const B: PopupId = "window_input_b";

/// 입력칸 이름. 창 위젯 `w`, 팝업 A 의 `a1`·`a2`, 팝업 B 의 `b`.
const FIELDS: [&str; 4] = ["w", "a1", "a2", "b"];

/// 시험 창: 관리자, 입력칸 글자와 마지막 패스의 자리, 이번 패스 키 사건을 본 노드.
struct TestWindow {
    popups: PopupManager,
    text: std::collections::HashMap<&'static str, String>,
    rects: std::collections::HashMap<&'static str, egui::Rect>,
    /// 키 X 누름을 본 노드(`w`·`A`·`B`). 패스마다 쌓는다.
    saw_x: Vec<&'static str>,
    /// 다음 그리기 중에 포커스를 주며 열 팝업(설정 충돌 질문처럼 그리기 중에 연다).
    open_while_drawing: Option<PopupId>,
    /// 참이면 팝업 A 가 상자 밖 `CHILD_MENU_POS` 에 등록 자식 메뉴(egui Foreground Area)를 그린다.
    child_menu: bool,
}

impl WindowPopups for TestWindow {
    fn window_popups(&mut self) -> &mut PopupManager {
        &mut self.popups
    }
}

impl TestWindow {
    fn new() -> Self {
        let mut popups = PopupManager::new();
        for (id, title) in [(A, "a"), (B, "b")] {
            popups.register(PopupState::new(
                id,
                title,
                egui::vec2(220.0, 120.0),
                PopupScope::Window,
            ));
        }
        Self {
            popups,
            text: FIELDS.iter().map(|f| (*f, String::new())).collect(),
            rects: Default::default(),
            saw_x: Vec::new(),
            open_while_drawing: None,
            child_menu: false,
        }
    }

    fn open(&mut self, id: PopupId, pos: egui::Pos2) {
        let request = OpenRequest::new(id, PopupScope::Window).at(pos).focused();
        assert!(self.popups.open_request(&request, true).is_some(), "{id}");
    }

    fn text(&self, field: &str) -> &str {
        &self.text[field]
    }

    fn center(&self, field: &str) -> egui::Pos2 {
        self.rects
            .get(field)
            .unwrap_or_else(|| panic!("{field} was not drawn"))
            .center()
    }
}

/// 이 `ui` 가 이번 패스에 키 X 누름을 보는지.
fn sees_x(ui: &egui::Ui) -> bool {
    ui.input(|i| {
        i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::X,
                    pressed: true,
                    ..
                }
            )
        })
    })
}

fn field(
    ui: &mut egui::Ui,
    name: &'static str,
    text: &mut std::collections::HashMap<&'static str, String>,
    rects: &mut std::collections::HashMap<&'static str, egui::Rect>,
) {
    let value = text.get_mut(name).expect("field");
    let response = ui.add(egui::TextEdit::singleline(value).id(egui::Id::new(name)));
    rects.insert(name, response.rect);
}

/// 팝업 A 의 등록 자식 메뉴 자리. 두 팝업(`two_popups`)의 상자 밖이다.
const CHILD_MENU_POS: egui::Pos2 = egui::pos2(40.0, 420.0);

/// `open` 이면 상자 밖으로 나오는 자식 메뉴(파일 선택기 숨은 경로 메뉴처럼)를 egui 팝업으로 열어 그리고 `instance` 의
/// 자식 팝업 id 로 보고한다. 아니면 그 보고를 지운다.
fn child_menu(ui: &egui::Ui, instance: crate::adapters::ui::popup::PopupInstanceId, open: bool) {
    let id = egui::Id::new("window_input_child_menu");
    if open {
        ui.ctx().memory_mut(|m| {
            if !m.is_popup_open(id) {
                m.open_popup(id);
            }
        });
        egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(CHILD_MENU_POS)
            .show(ui.ctx(), |ui| {
                ui.allocate_exact_size(egui::vec2(120.0, 40.0), egui::Sense::click());
            });
    }
    crate::adapters::ui::popup::report_child_popup_id(
        ui.ctx(),
        instance,
        "window_input_child_menu",
        open.then_some(id),
    );
}

/// 설정 창 그리기와 같은 순서: 패스 키 값 넣기 → 창 위젯(차단 값) → 그리기 중 열기 → 팝업.
fn draw(ctx: &egui::Context, pass: &mut WindowPass<'_, TestWindow>) {
    let window = &mut *pass.state;
    let takes = window.popups.window_widget_takes_keys(ctx);
    crate::adapters::ui::popup::body_keys::with_body_keys(ctx, takes, || {
        egui::CentralPanel::default().show(ctx, |ui| {
            if sees_x(ui) {
                window.saw_x.push("w");
            }
            field(ui, "w", &mut window.text, &mut window.rects);
        });
    });
    if let Some(id) = window.open_while_drawing.take() {
        window.open(id, egui::pos2(300.0, 300.0));
    }
    let child = window.child_menu;
    let (text, rects, saw) = (&mut window.text, &mut window.rects, &mut window.saw_x);
    window.popups.draw_view_local(ctx, &mut |cx, ui, _| {
        if sees_x(ui) {
            saw.push(match cx.id {
                A => "A",
                B => "B",
                _ => "C",
            });
        }
        if cx.id == A {
            field(ui, "a1", text, rects);
            field(ui, "a2", text, rects);
            child_menu(ui, cx.instance, child);
        } else if cx.id == B {
            field(ui, "b", text, rects);
        }
        false
    });
}

/// 창 하나의 egui 입력 큐·끊은 자리·Esc 표지(제품 `GpuState` 의 대역). 창 상태는 팝업 관리자를 가진 어느 창이든
/// 된다(`WindowPopups`, 설정 창 시험도 쓴다).
pub(crate) struct Harness {
    pub(crate) ctx: egui::Context,
    queue: Vec<egui::Event>,
    pub(crate) cuts: InputCuts,
    held: HeldPastes,
}

impl EguiPressSink for Harness {
    fn queued_events(&mut self) -> &mut Vec<egui::Event> {
        &mut self.queue
    }

    fn cut_queue_here(&mut self) {
        self.cuts.cut_at(self.queue.len());
    }
}

impl<'s, T: WindowPopups> InputPassHost<WindowPass<'s, T>> for Harness {
    fn escape_held(&self) -> bool {
        self.ctx.input(|i| i.key_down(egui::Key::Escape))
    }

    fn held_pastes(&mut self) -> &mut HeldPastes {
        &mut self.held
    }

    fn take_input(&mut self) -> egui::RawInput {
        let mut events = std::mem::take(&mut self.queue);
        self.cuts.split_pass(&mut events, &mut self.queue);
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn egui_ctx(&self) -> &egui::Context {
        &self.ctx
    }

    fn input_dropped(&mut self) {}

    fn focus_tag(&self) -> u64 {
        self.ctx.memory(|m| m.focused()).map_or(0, |id| id.value())
    }

    fn run(
        &mut self,
        raw_input: egui::RawInput,
        state: &mut WindowPass<'s, T>,
        mut ui: impl FnMut(&egui::Context, &mut WindowPass<'s, T>),
    ) -> egui::FullOutput {
        state.begin_run(&self.ctx);
        self.ctx.run(raw_input, |ctx| ui(ctx, state))
    }

    fn cuts_and_raw_left(&mut self) -> (&mut InputCuts, usize) {
        (&mut self.cuts, self.queue.len())
    }
}

impl Harness {
    pub(crate) fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            queue: Vec::new(),
            cuts: InputCuts::default(),
            held: HeldPastes::default(),
        }
    }

    fn settle(
        &mut self,
        window: &mut impl WindowPopups,
        incoming: Incoming,
        key: Option<WindowKey>,
    ) -> bool {
        let ctx = self.ctx.clone();
        settle_window_input(self, &ctx, window.window_popups(), incoming, key)
    }

    /// 포인터를 옮기고(끊지 않는 사건) 누른다.
    pub(crate) fn press(&mut self, window: &mut impl WindowPopups, pos: egui::Pos2) {
        self.queue.push(egui::Event::PointerMoved(pos));
        self.settle(window, Incoming::Press, None);
        self.queue.push(button(pos, true));
    }

    /// 누른 버튼을 뗀다(끊지 않는 사건).
    pub(crate) fn release(&mut self, pos: egui::Pos2) {
        self.queue.push(button(pos, false));
    }

    pub(crate) fn click(&mut self, window: &mut impl WindowPopups, pos: egui::Pos2) {
        self.press(window, pos);
        self.queue.push(button(pos, false));
    }

    /// 수식키 없는 키 하나. 글자가 있으면 같은 사건이 글자도 싣는다.
    pub(crate) fn key(
        &mut self,
        window: &mut impl WindowPopups,
        key: egui::Key,
        text: Option<&str>,
    ) {
        self.key_with(window, key, egui::Modifiers::NONE, text);
    }

    /// `modifiers` 를 실은 키 하나. 글자가 있으면 같은 사건이 글자도 싣는다. 창 노드(창 위젯과 창 자체 처리)가 그 키를
    /// 받으면 참이다(단계 4).
    pub(crate) fn key_with(
        &mut self,
        window: &mut impl WindowPopups,
        key: egui::Key,
        modifiers: egui::Modifiers,
        text: Option<&str>,
    ) -> bool {
        let route = crate::adapters::ui::input::key_route::RouteKey {
            key: Some(key),
            modifiers,
            super_held: false,
        };
        let incoming = if route.bare_escape() {
            Incoming::Escape
        } else {
            Incoming::Key
        };
        let window_receives = self.settle(
            window,
            incoming,
            Some(WindowKey::Path {
                key: route,
                own: false,
            }),
        );
        self.queue.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        if let Some(text) = text {
            self.queue.push(egui::Event::Text(text.into()));
        }
        let ctx = self.ctx.clone();
        let held = ctx.input(|i| i.key_down(egui::Key::Escape));
        bind_window_input(
            &ctx,
            window.window_popups(),
            incoming,
            &mut self.queue,
            held,
        );
        window_receives
    }

    /// 창 자체 처리의 바인딩(engine level)에 맞는 키 하나. 창 노드가 그 키를 받으면 참이다(단계 4).
    fn own_key(
        &mut self,
        window: &mut impl WindowPopups,
        key: egui::Key,
        modifiers: egui::Modifiers,
    ) -> bool {
        let route = crate::adapters::ui::input::key_route::RouteKey {
            key: Some(key),
            modifiers,
            super_held: false,
        };
        self.settle(
            window,
            Incoming::Key,
            Some(WindowKey::Path {
                key: route,
                own: true,
            }),
        )
    }

    fn type_x(&mut self, window: &mut TestWindow) {
        self.key(window, egui::Key::X, Some("x"));
    }

    /// 제품과 같은 순서 함수로 `draw` 를 그리는 패스 하나.
    pub(crate) fn run_pass<'s, T: WindowPopups>(
        &mut self,
        window: &'s mut T,
        draw: impl FnMut(&egui::Context, &mut WindowPass<'s, T>),
    ) -> egui::FullOutput {
        let mut state = WindowPass::new(window);
        run_input_pass(self, &mut state, |_| {}, draw)
    }

    /// 그릴 몫이 남았는지(꺼내지 않은 사건이나 끊은 자리).
    pub(crate) fn has_input(&self) -> bool {
        !self.queue.is_empty() || !self.cuts.is_empty()
    }

    /// 패스 하나. 이번 패스에 키 X 를 본 노드를 돌려준다.
    fn pass(&mut self, window: &mut TestWindow) -> Vec<&'static str> {
        window.saw_x.clear();
        drop(self.run_pass(window, draw));
        std::mem::take(&mut window.saw_x)
    }

    /// 남은 몫을 모두 그리고 빈 패스 하나를 더 그린다. 패스마다 키 X 를 본 노드를 돌려준다.
    fn drain(&mut self, window: &mut TestWindow) -> Vec<Vec<&'static str>> {
        let mut seen = Vec::new();
        while self.has_input() {
            seen.push(self.pass(window));
        }
        seen.push(self.pass(window));
        seen
    }
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 팝업 A(입력칸 a1·a2)와 B(입력칸 b)를 연 창. 마지막에 연 B 가 관리자 포커스다. 두 패스로 자리를 잡는다.
fn two_popups() -> (Harness, TestWindow) {
    let mut h = Harness::new();
    let mut w = TestWindow::new();
    w.open(A, egui::pos2(300.0, 60.0));
    w.open(B, egui::pos2(300.0, 320.0));
    h.drain(&mut w);
    h.drain(&mut w);
    (h, w)
}

/// `field` 를 눌러 egui 포커스를 주고 패스를 모두 그린다.
fn focus_field(h: &mut Harness, w: &mut TestWindow, field: &str) {
    let pos = w.center(field);
    h.click(w, pos);
    h.drain(w);
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        Some(egui::Id::new(field)),
        "{field} has the egui focus"
    );
}

/// Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 같은 팝업의 a2 를 누르고 바로 x 를
/// 치면, Then x 는 누른 a2 에 들어간다. 누름 뒤 키 앞에서 끊어 누름을 처리한 패스 뒤에 키를 받는다.
#[test]
fn a_key_right_after_a_press_on_a_separate_window_popup_reaches_that_popup() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    let a2 = w.center("a2");
    h.click(&mut w, a2);
    h.type_x(&mut w);
    h.drain(&mut w);
    assert_eq!(
        (w.text("a1"), w.text("a2")),
        ("", "x"),
        "the key reaches the field the press focused"
    );
}

/// Given 창 위젯 w 가 egui 포커스를 가졌고 팝업이 열려 있다, When 누름 없이 x 를 치면, Then x 는 창
/// 위젯에 들어가고 팝업은 그 키를 보지 않는다.
#[test]
fn a_key_without_a_press_stays_with_the_separate_window_widget() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "w");
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!(w.text("w"), "x");
    assert!(
        seen.iter().flatten().all(|n| *n == "w"),
        "only the window widget sees the key: {seen:?}"
    );
}

/// Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 팝업 B 의 b 를 누르고 x 를 치면,
/// Then 그 누름이 노드를 B 로 옮겨 x 는 b 에 들어가고 A 는 x 를 보지 않는다.
#[test]
fn a_press_on_another_popup_moves_the_key_owner_in_that_pass() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    let b = w.center("b");
    h.click(&mut w, b);
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!(
        (w.text("a1"), w.text("b")),
        ("", "x"),
        "the new owner gets the key"
    );
    assert!(
        seen.iter().flatten().all(|n| *n == "B"),
        "only B sees the key: {seen:?}"
    );
}

/// Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 창 위젯 w 를 누르고 x 를 치면, Then
/// 노드는 창 위젯이고 x 는 w 에 들어간다.
#[test]
fn a_press_outside_any_popup_leaves_the_window_widget_as_the_node() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    let field = w.center("w");
    h.click(&mut w, field);
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!((w.text("a1"), w.text("w")), ("", "x"));
    assert!(
        seen.iter().flatten().all(|n| *n == "w"),
        "only the window widget sees the key: {seen:?}"
    );
}

/// Given 팝업 B 가 키 주인, When 한 입력 묶음에 B 를 누르고 떼고 수식키 없는 Esc 를 치면, Then 누름 몫과 Esc
/// 몫이 나뉘고 Esc 표지는 Esc 몫에 가서 B 가 선언대로 닫힌다.
#[test]
fn an_escape_after_a_click_segment_in_a_separate_window_still_reaches_its_owner() {
    let (mut h, mut w) = two_popups();
    let b = w.center("b");
    h.click(&mut w, b);
    h.key(&mut w, egui::Key::Escape, None);
    assert_eq!(h.cuts.len(), 1, "the click and the Escape are two segments");
    h.drain(&mut w);
    assert!(!w.popups.is_open(B), "the Escape closes its owner");
    assert!(w.popups.is_open(A));
}

/// Given 팝업 B 가 키 주인, When 수식키 없는 Esc 만 치면, Then B 가 닫힌다.
#[test]
fn an_escape_alone_in_a_separate_window_reaches_its_owner() {
    let (mut h, mut w) = two_popups();
    h.key(&mut w, egui::Key::Escape, None);
    h.drain(&mut w);
    assert!(!w.popups.is_open(B), "the Escape closes its owner");
    assert!(w.popups.is_open(A));
}

/// Given egui 포커스가 없는 창, When 그리기 중에 포커스를 주며 팝업 A 를 열고(설정 충돌 질문) 다음 묶음에 x 를
/// 치면, Then 포커스 기록이 없으므로 노드는 막 연 A 이고 A 가 x 를 본다.
#[test]
fn a_focused_popup_opened_while_drawing_takes_the_next_key() {
    let mut h = Harness::new();
    let mut w = TestWindow::new();
    h.drain(&mut w);
    w.open_while_drawing = Some(A);
    h.drain(&mut w);
    assert!(w.popups.is_open(A));
    assert_eq!(h.ctx.memory(|m| m.focused()), None, "no egui focus");
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!(
        seen.iter().flatten().copied().collect::<Vec<_>>(),
        ["A"],
        "the popup just opened takes the key"
    );
}

/// Given 팝업 A 의 a1 에 포커스가 있었고 그 뒤 창 위젯 w 를 눌러 포커스가 팝업을 떠났다, When 다음 묶음에 x 를
/// 치면, Then 노드는 창 위젯이고 x 는 w 에 들어간다.
#[test]
fn a_key_after_focus_left_the_popup_stays_with_the_window_widget() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    focus_field(&mut h, &mut w, "w");
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!((w.text("a1"), w.text("w")), ("", "x"));
    assert!(
        seen.iter().flatten().all(|n| *n == "w"),
        "only the window widget sees the key: {seen:?}"
    );
}

/// Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 x, 창 위젯 w 누름, y 가 오면, Then
/// x 는 적재 때 A 에 묶인 몫이라 그 몫을 그리는 패스의 키 주인이 A 이고 a1 에 들어간다. y 는 w 에 들어간다. 그 패스 시작의
/// 노드(누름이 정한 창 위젯)로 덮지 않는다.
#[test]
fn a_key_bound_in_the_feed_step_reaches_the_same_popup_when_drawing() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    h.type_x(&mut w);
    let field = w.center("w");
    h.click(&mut w, field);
    h.key(&mut w, egui::Key::Y, Some("y"));
    h.drain(&mut w);
    assert_eq!((w.text("a1"), w.text("w")), ("x", "y"));
}

/// Given 팝업 A 의 a1 이 키 주인이고 A 가 상자 밖에 등록 자식 메뉴를 열었다, When 그 메뉴를 누르고 다음 묶음에 x 를
/// 치면, Then 사건 입구가 그 누름을 A 의 누름으로 정했고 그 누름을 그리는 패스는 포커스를 다시 정하지 않으므로 x 는 A 가
/// 받는다. 창 위젯 w 는 그 키를 보지 않는다.
#[test]
fn a_press_on_a_child_menu_outside_the_popup_box_keeps_that_popup_the_node() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    w.child_menu = true;
    h.drain(&mut w);
    h.drain(&mut w);
    assert_eq!(
        h.ctx
            .layer_id_at(CHILD_MENU_POS + egui::vec2(20.0, 20.0))
            .map(|l| l.order),
        Some(egui::Order::Foreground),
        "the child menu is drawn"
    );
    let menu = CHILD_MENU_POS + egui::vec2(20.0, 20.0);
    h.click(&mut w, menu);
    h.drain(&mut w);
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert!(
        seen.iter().flatten().all(|n| *n == "A") && seen.iter().flatten().count() > 0,
        "only popup A sees the key: {seen:?}"
    );
    assert_eq!(w.text("w"), "", "the window widget gets no text");
}

/// 팝업 A 의 a1 이 키 주인이고 A 의 등록 자식 메뉴가 열린 창. A 인스턴스를 돌려준다.
fn child_menu_open() -> (
    Harness,
    TestWindow,
    crate::adapters::ui::popup::PopupInstanceId,
) {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    w.child_menu = true;
    h.drain(&mut w);
    h.drain(&mut w);
    let a = w.popups.instances_of(A).next().expect("A").instance;
    assert!(w.popups.is_instance_focused(a));
    (h, w, a)
}

/// Given 팝업 A 가 키 주인이고 A 의 등록 자식 메뉴가 열려 있다, When 수식키 없는 Esc 가 오면, Then 그 Esc 는 자식이
/// 받으므로 사건 시점에 A 의 포커스를 풀지 않는다.
#[test]
fn an_escape_with_a_child_menu_open_keeps_the_popup_focused() {
    let (mut h, mut w, a) = child_menu_open();
    h.key(&mut w, egui::Key::Escape, None);
    assert!(
        w.popups.is_instance_focused(a),
        "the child takes the Escape"
    );
}

/// Given 팝업 A 가 키 주인이고 A 의 등록 자식 메뉴가 열려 있다, When 그리기 전에 수식키 없는 Esc 가 두 번 오면, Then
/// 첫 Esc 를 자식이 받았으므로 둘째 Esc 는 본문 Esc 라 사건 시점에 A 의 포커스를 푼다.
#[test]
fn the_second_escape_of_a_batch_after_a_child_menu_releases_the_popup() {
    let (mut h, mut w, a) = child_menu_open();
    h.key(&mut w, egui::Key::Escape, None);
    h.key(&mut w, egui::Key::Escape, None);
    assert!(
        !w.popups.is_instance_focused(a),
        "the second Escape is the body's"
    );
}

/// 덮는 팝업 C(설정 창 단축키 충돌 확인 선언)를 등록한 창. 창 위젯 w 에 egui 포커스를 준 뒤 C 를 포커스를 주며 연다.
fn covered_widget() -> (Harness, TestWindow) {
    let mut h = Harness::new();
    let mut w = TestWindow::new();
    w.popups
        .register_def(&crate::view::settings::ui::keybinding_conflict_def(), 1.0);
    h.drain(&mut w);
    focus_field(&mut h, &mut w, "w");
    let request = OpenRequest::new(
        crate::view::settings::ui::KEYBINDING_CONFLICT_POPUP_ID,
        PopupScope::Window,
    )
    .at(egui::pos2(500.0, 400.0))
    .focused();
    assert!(w.popups.open_request(&request, true).is_some());
    h.drain(&mut w);
    h.drain(&mut w);
    (h, w)
}

/// Given 창 위젯 w 에 egui 포커스가 남은 채 창을 덮는 팝업 C 가 포커스를 가지고 열렸다, When 누름 없이 x 를 치면,
/// Then 포커스 기록이 덮인 w 여도 덮인 노드는 노드가 될 수 없으므로 C 가 받고 w 는 그 키를 보지 않는다.
#[test]
fn a_key_over_a_covering_popup_does_not_reach_the_widget_holding_the_old_focus() {
    let (mut h, mut w) = covered_widget();
    assert_eq!(
        h.ctx.memory(|m| m.focused()),
        Some(egui::Id::new("w")),
        "w keeps the egui focus"
    );
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!(w.text("w"), "", "the covered widget gets no text");
    assert!(
        seen.iter().flatten().all(|n| *n == "C") && seen.iter().flatten().count() > 0,
        "only the covering popup sees the key: {seen:?}"
    );
}

/// Given 창을 덮는 팝업 C(선언 Keys)가 노드, When C 의 소비 집합 밖 키 Ctrl+X 를 치면, Then 창 노드(덮인 창 위젯과
/// 창 자체 처리)는 그 키를 받지 않는다. 덮음 막이를 먼저 눌러 노드를 C 로 둔다.
#[test]
fn a_key_a_covering_popup_does_not_use_reaches_no_window_node() {
    let (mut h, mut w) = covered_widget();
    let field = w.center("w");
    h.click(&mut w, field);
    h.drain(&mut w);
    assert!(!h.key_with(&mut w, egui::Key::X, egui::Modifiers::CTRL, None));
}

/// Given 창을 덮는 팝업 C 가 노드, When 창 자체 처리의 바인딩(engine level)에 맞는 키를 치면, Then 창 자체 처리가
/// 그 키를 받는다(덮인 창에서도 engine level 후보는 받는다).
#[test]
fn an_engine_key_under_a_covering_popup_reaches_the_window_s_own_handling() {
    let (mut h, mut w) = covered_widget();
    let field = w.center("w");
    h.click(&mut w, field);
    h.drain(&mut w);
    assert!(h.own_key(&mut w, egui::Key::F7, egui::Modifiers::CTRL));
}

/// Given 덮지 않는 팝업 A(선언 Keys)의 a1 이 키 주인, When A 의 소비 집합 밖 키 Ctrl+X 를 치면, Then 메인 창의 덮지
/// 않는 팝업처럼 그 키는 위로 올라가 받는 노드가 없다. 창 노드는 받지 않는다.
#[test]
fn a_key_a_non_covering_popup_does_not_use_reaches_no_window_node() {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "a1");
    assert!(!h.key_with(&mut w, egui::Key::X, egui::Modifiers::CTRL, None));
}

/// Given 팝업 노드가 없는 창, When Ctrl+X 를 치면, Then 창 노드(창 위젯 내용)가 받는다.
#[test]
fn a_key_without_a_popup_node_reaches_the_window_node() {
    let mut h = Harness::new();
    let mut w = TestWindow::new();
    h.drain(&mut w);
    assert!(h.key_with(&mut w, egui::Key::X, egui::Modifiers::CTRL, None));
}

/// Given 창 위젯 w 위를 덮는 팝업 C 가 포커스를 가지고 열려 있다, When 한 묶음에 덮인 w 자리를 누르고 x 를 치면, Then
/// 그 누름은 C 의 덮음 막이를 누른 것이라 사건 시점 노드는 C 이고 w 는 그 키를 보지 않는다.
#[test]
fn a_press_on_the_cover_of_a_separate_window_popup_keeps_that_popup_the_node() {
    let (mut h, mut w) = covered_widget();
    let field = w.center("w");
    h.click(&mut w, field);
    h.type_x(&mut w);
    let seen = h.drain(&mut w);
    assert_eq!(w.text("w"), "", "the covered widget gets no text");
    assert!(
        seen.iter().flatten().all(|n| *n == "C"),
        "only the covering popup sees the key: {seen:?}"
    );
}

/// debug 주입 대역: egui 입력 큐와 끊은 자리는 하네스 것이다. 보류 줄 검사는 주입 입구 맨 앞
/// (`enter_window_debug`)이 이미 했으므로 넣은 사건은 큐 끝에 든다. 클립보드는 비어 있다.
#[cfg(debug_assertions)]
struct Injection<'h> {
    harness: &'h mut Harness,
    held: egui::Modifiers,
}

#[cfg(debug_assertions)]
impl EguiPressSink for Injection<'_> {
    fn queued_events(&mut self) -> &mut Vec<egui::Event> {
        &mut self.harness.queue
    }

    fn cut_queue_here(&mut self) {
        self.harness.cut_queue_here();
    }
}

#[cfg(debug_assertions)]
impl crate::view::main::debug_input::EguiKeySink for Injection<'_> {
    fn held_modifiers(&self) -> egui::Modifiers {
        self.held
    }

    fn hold_modifiers(&mut self, modifiers: egui::Modifiers) {
        self.held = modifiers;
    }

    fn clipboard_text(&mut self) -> Option<String> {
        None
    }

    fn push_events(&mut self, events: Vec<egui::Event>) {
        self.harness.queue.extend(events);
    }
}

/// 창 위젯 w 가 받은 Esc 누름이 아직 그리지 않은 egui 입력에 있고 같은 묶음의 누름이 노드를 팝업 B 로 옮긴 뒤 debug
/// 키 하나(`key`, `modifiers`)를 주입한다. 주입 전 큐 길이와 끊은 자리 수, 주입 뒤 하네스를 돌려준다.
#[cfg(debug_assertions)]
fn inject_after_a_press(key: &str, modifiers: Option<egui::Modifiers>) -> (usize, usize, Harness) {
    let (mut h, mut w) = two_popups();
    focus_field(&mut h, &mut w, "w");
    h.key(&mut w, egui::Key::Escape, None);
    let b = w.center("b");
    h.click(&mut w, b);
    let ctx = h.ctx.clone();
    let before = h.queue.len();
    let cuts = h.cuts.len();
    let pushed = inject_window_key(
        &ctx,
        &mut w.popups,
        &mut Injection {
            harness: &mut h,
            held: egui::Modifiers::NONE,
        },
        key,
        true,
        modifiers,
    );
    assert!(pushed.is_some());
    (before, cuts, h)
}

/// 큐에 남은 Esc 누름 수.
#[cfg(debug_assertions)]
fn queued_escapes(h: &Harness) -> usize {
    h.queue
        .iter()
        .filter(|e| crate::adapters::ui::popup::body_keys::is_escape_press(e))
        .count()
}

/// Given 같은 장면, When 빈 클립보드로 debug 붙여넣기 키를 주입하면(실을 사건이 없다), Then 실을지를 먼저 정하므로
/// 몫을 끊지도 묶지도 않는다: 큐와 끊은 자리가 그대로이고 앞 Esc 누름은 w 몫으로 egui 입력에 남는다.
#[cfg(debug_assertions)]
#[test]
fn a_debug_key_that_loads_nothing_does_not_cut_or_bind() {
    let (before, cuts, h) = inject_after_a_press("V", Some(egui::Modifiers::COMMAND));
    assert_eq!(h.queue.len(), before, "nothing is loaded");
    assert_eq!(h.cuts.len(), cuts, "no segment is cut for it");
    assert_eq!(
        queued_escapes(&h),
        1,
        "the earlier Escape stays in the egui input"
    );
}

/// Given 같은 장면, When 보류 줄 없이 debug Esc 를 주입하면, Then 실린 그 Esc 는 사건 시점 노드 B 의 몫으로 묶여 egui
/// 입력에서 빠지고(B 에 넘긴다) 앞 Esc 누름은 w 몫으로 남는다.
#[cfg(debug_assertions)]
#[test]
fn a_loaded_debug_escape_goes_to_its_own_node() {
    let (before, _, h) = inject_after_a_press("Escape", None);
    assert_eq!(h.queue.len(), before, "the injected Escape is handed to B");
    assert_eq!(
        queued_escapes(&h),
        1,
        "the earlier Escape stays in the egui input"
    );
}

/// Given 팝업 관리자가 없는 종료 확인 창, When 수식키 없는 Esc 누름, Then 편집 판정을 받아 `EditKeyState::judge`
/// 보류(누름이 먼저 처리되기를 기다림)를 거친다.
#[test]
fn a_bare_escape_in_the_unmanaged_quit_window_takes_the_edit_judgement() {
    assert!(window_edit_target(Some(Incoming::Escape), false));
}

/// Given 팝업 관리자를 가진 창, When 수식키 없는 Esc 누름, Then 편집 판정을 건너 수신자의 Esc 처리로 간다.
#[test]
fn a_bare_escape_in_a_managed_window_skips_the_edit_judgement() {
    assert!(!window_edit_target(Some(Incoming::Escape), true));
}

/// Given 팝업 관리자를 가진 창, When Esc 가 아닌 키·누름·기타 사건, Then 편집 판정을 받는다.
#[test]
fn a_non_escape_event_in_a_managed_window_takes_the_edit_judgement() {
    for incoming in [Some(Incoming::Key), Some(Incoming::Press), None] {
        assert!(window_edit_target(incoming, true));
    }
}
