//! 별도 창 입력 패스(§1.1) 장면. 창 대역은 사건마다 제품 단계를 밟는다: 단계 3~5(`settle_window_input`), egui 큐 적재
//! (Esc 누름이면 그 사건 시점 수식키 표지), 적재한 키의 몫 결속(`PopupManager::bind_window_key`). 패스는 제품과 같은 순서
//! 함수(`run_input_pass`)와 창 상태(`WindowPass`)로 돌고, 입력은 끊은 자리에서 나누며 표지도 제품과 같은 함수로 몫별로
//! 나눈다(`split_pass_marks`). 그리기는 설정 창처럼 관리자에 패스 키 값을 넣고 창 위젯을 그린 뒤 팝업을 그린다.

use super::*;
use crate::adapters::ui::popup::escape_step::split_pass_marks;
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

/// 설정 창 그리기와 같은 순서: 패스 키 값 넣기 → 창 위젯(차단 값) → 그리기 중 열기 → 팝업.
fn draw(ctx: &egui::Context, pass: &mut WindowPass<'_, TestWindow>) {
    let window = &mut *pass.state;
    let takes = window.popups.put_window_pass_keys(ctx, &pass.keys);
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
    let (text, rects, saw) = (&mut window.text, &mut window.rects, &mut window.saw_x);
    window.popups.draw_view_local(ctx, &mut |cx, ui, _| {
        if sees_x(ui) {
            saw.push(if cx.id == A { "A" } else { "B" });
        }
        if cx.id == A {
            field(ui, "a1", text, rects);
            field(ui, "a2", text, rects);
        } else {
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
    loaded_marks: Vec<tasty_key_match::KeyMods>,
    pass_marks: Vec<tasty_key_match::KeyMods>,
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
        let split = self.cuts.split_pass(&mut events, &mut self.queue);
        self.pass_marks = split_pass_marks(&mut self.loaded_marks, &events, split);
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
        state.begin_run(&self.ctx, std::mem::take(&mut self.pass_marks));
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
            loaded_marks: Vec::new(),
            pass_marks: Vec::new(),
            held: HeldPastes::default(),
        }
    }

    fn settle(&mut self, window: &mut impl WindowPopups, incoming: Incoming) {
        let ctx = self.ctx.clone();
        settle_window_input(self, &ctx, window.window_popups(), incoming);
    }

    /// 포인터를 옮기고(끊지 않는 사건) 누른다.
    fn press(&mut self, window: &mut impl WindowPopups, pos: egui::Pos2) {
        self.queue.push(egui::Event::PointerMoved(pos));
        self.settle(window, Incoming::Press);
        self.queue.push(button(pos, true));
    }

    pub(crate) fn click(&mut self, window: &mut impl WindowPopups, pos: egui::Pos2) {
        self.press(window, pos);
        self.queue.push(button(pos, false));
    }

    /// 수식키 없는 키 하나. 글자가 있으면 같은 사건이 글자도 싣는다. Esc 누름이면 표지를 남긴다.
    pub(crate) fn key(
        &mut self,
        window: &mut impl WindowPopups,
        key: egui::Key,
        text: Option<&str>,
    ) {
        let incoming = if key == egui::Key::Escape {
            Incoming::Escape
        } else {
            Incoming::Key
        };
        self.settle(window, incoming);
        let from = self.queue.len();
        self.queue.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        if let Some(text) = text {
            self.queue.push(egui::Event::Text(text.into()));
        }
        self.loaded_marks
            .extend(crate::adapters::ui::popup::body_keys::escape_marks_in(
                &self.queue,
                from,
                tasty_key_match::KeyMods::default(),
            ));
        bind_window_input(&self.ctx, window.window_popups(), incoming);
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

/// M-W2a-1 F: Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 같은 팝업의 a2 를 누르고 바로 x 를
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

/// M-W2a-1 P, M-W2a-5 P: Given 창 위젯 w 가 egui 포커스를 가졌고 팝업이 열려 있다, When 누름 없이 x 를 치면, Then x 는 창
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

/// M-W2a-2 F: Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 팝업 B 의 b 를 누르고 x 를 치면,
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

/// M-W2a-2 P: Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 창 위젯 w 를 누르고 x 를 치면, Then
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

/// M-W2a-3 F: Given 팝업 B 가 키 주인, When 한 입력 묶음에 B 를 누르고 떼고 수식키 없는 Esc 를 치면, Then 누름 몫과 Esc
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

/// M-W2a-3 P: Given 팝업 B 가 키 주인, When 수식키 없는 Esc 만 치면, Then B 가 닫힌다.
#[test]
fn an_escape_alone_in_a_separate_window_reaches_its_owner() {
    let (mut h, mut w) = two_popups();
    h.key(&mut w, egui::Key::Escape, None);
    h.drain(&mut w);
    assert!(!w.popups.is_open(B), "the Escape closes its owner");
    assert!(w.popups.is_open(A));
}

/// M-W2a-4 F: Given egui 포커스가 없는 창, When 그리기 중에 포커스를 주며 팝업 A 를 열고(설정 충돌 질문) 다음 묶음에 x 를
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

/// M-W2a-4 P: Given 팝업 A 의 a1 에 포커스가 있었고 그 뒤 창 위젯 w 를 눌러 포커스가 팝업을 떠났다, When 다음 묶음에 x 를
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

/// M-W2a-5 F: Given 팝업 A 의 a1 이 egui 포커스를 가진 키 주인, When 한 입력 묶음에 x, 창 위젯 w 누름, y 가 오면, Then
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
