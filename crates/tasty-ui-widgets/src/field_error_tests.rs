use std::time::Duration;

use tasty_type_geometry::length::LogicalPx;

use super::FieldErrorState::{self, Clear, Due, Held, Shown};
use super::{field_error_state, paint_field_error_line, press_is_stale};
use crate::control::Ground;
use crate::input::Input;
use crate::toggle::checkbox;

const NO_BOX: [FieldErrorState; 2] = [Clear, Due];
const BOX: [FieldErrorState; 2] = [Held, Shown];
const ANY: [bool; 2] = [false, true];

/// (지난 상태들, active, pointer_held, rejected, 다음 상태)
type Row = (
    [FieldErrorState; 2],
    &'static [bool],
    &'static [bool],
    &'static [bool],
    FieldErrorState,
);

/// 디자인 전이표의 행 순서 그대로다.
const TABLE: [Row; 10] = [
    (NO_BOX, &[true], &ANY, &ANY, Clear),
    (BOX, &[true], &ANY, &ANY, Held),
    (NO_BOX, &[false], &[true], &[true], Due),
    (NO_BOX, &[false], &[true], &[false], Clear),
    (BOX, &[false], &[true], &[true], Shown),
    (BOX, &[false], &[true], &[false], Held),
    (NO_BOX, &[false], &[false], &[true], Shown),
    (BOX, &[false], &[false], &[true], Shown),
    (NO_BOX, &[false], &[false], &[false], Clear),
    (BOX, &[false], &[false], &[false], Clear),
];

#[test]
fn every_row_of_the_transition_table_holds() {
    let mut cases = 0;
    for (lasts, actives, helds, rejecteds, next) in TABLE {
        for last in lasts {
            for &active in actives {
                for &held in helds {
                    for &rejected in rejecteds {
                        assert_eq!(
                            last.step(active, held, rejected),
                            next,
                            "{last:?} active={active} held={held} rejected={rejected}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    // 네 상태와 세 입력의 모든 조합이다. 표에 빠진 조합이 있으면 이 수가 달라진다.
    assert_eq!(cases, 4 * 8);
}

#[test]
fn each_state_draws_what_the_design_says() {
    assert_eq!(Clear.line("x"), None);
    assert!(!Clear.invalid());
    assert_eq!(Due.line("x"), None);
    assert!(Due.invalid());
    assert!(Held.line("x").is_some_and(|l| !l.ink));
    assert!(!Held.invalid());
    assert!(Shown.line("x").is_some_and(|l| l.ink));
    assert!(Shown.invalid());
}

/// `(was_stale, focused, down, pressed)` → 묵은 눌림인지.
#[test]
fn a_press_the_window_lost_track_of_goes_stale_until_the_next_real_press() {
    for (was, focused, down, pressed, stale) in [
        (false, true, true, true, false),
        (false, true, true, false, false),
        (false, false, true, false, true),
        (true, true, true, false, true),
        (true, true, true, true, false),
        (true, true, false, false, false),
        (true, false, false, false, false),
    ] {
        assert_eq!(
            press_is_stale(was, focused, down, pressed),
            stale,
            "was={was} focused={focused} down={down} pressed={pressed}"
        );
    }
}

fn input(events: Vec<egui::Event>, focused: bool) -> egui::RawInput {
    egui::RawInput {
        events,
        focused,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    }
}

fn button(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

struct Rig {
    ctx: egui::Context,
    value: String,
    rejected: bool,
    checked: bool,
    /// 마지막 프레임의 상태와 체크박스 영역.
    state: FieldErrorState,
    check: egui::Rect,
    repaint_now: bool,
}

impl Rig {
    fn new(rejected: bool) -> Self {
        Self {
            ctx: egui::Context::default(),
            value: String::new(),
            rejected,
            checked: false,
            state: Clear,
            check: egui::Rect::NOTHING,
            repaint_now: false,
        }
    }

    /// 본체 행과 같은 순서로 입력, 오류 줄, 아래 체크박스를 그린다.
    fn frame(&mut self, raw: egui::RawInput) {
        let th = tasty_themes::mocha_fallback();
        let id = egui::Id::new("field");
        let Self {
            ctx,
            value,
            rejected,
            checked,
            state,
            check,
            repaint_now,
        } = self;
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let response = Input::new().id(id).show(ui, &th, value);
                *state = field_error_state(ctx, id, response.has_focus(), *rejected);
                if let Some(line) = state.line("the file is not a usable font") {
                    paint_field_error_line(ui, &th, LogicalPx(0.0), line);
                }
                *check = checkbox(ui, &th, checked, "below", true, Ground::Panel).rect;
            });
        });
        *repaint_now = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay == Duration::ZERO);
    }

    /// 판정이 자리 잡은 뒤 입력에 포커스를 넣는다.
    fn settle_and_focus(&mut self) {
        self.frame(input(vec![], true));
        self.frame(input(vec![], true));
        self.ctx
            .memory_mut(|m| m.request_focus(egui::Id::new("field")));
        self.frame(input(vec![], true));
        self.frame(input(vec![], true));
    }

    /// 아래 체크박스를 눌렀다 떼며 세 프레임의 (상태, y)를 돌려준다.
    fn click_below(&mut self) -> [(FieldErrorState, f32); 3] {
        let at = self.check.center();
        self.frame(input(button(at, true), true));
        let press = (self.state, self.check.top());
        self.frame(input(button(at, false), true));
        let release = (self.state, self.check.top());
        assert!(
            self.repaint_now,
            "egui must ask for the frame the deferred box needs"
        );
        self.frame(input(vec![], true));
        [press, release, (self.state, self.check.top())]
    }
}

#[test]
fn a_click_below_an_emptied_field_lands_and_the_box_closes_after_it() {
    let mut rig = Rig::new(true);
    rig.settle_and_focus();
    assert_eq!(rig.state, Held);
    rig.rejected = false;
    let top = rig.check.top();

    let [press, release, next] = rig.click_below();
    assert_eq!(press, (Held, top));
    assert_eq!(release, (Held, top));
    assert!(rig.checked, "the click must land on the checkbox");
    assert_eq!(next.0, Clear);
    assert!(next.1 < top, "the row rises after the click");
}

#[test]
fn a_click_below_a_newly_rejected_field_lands_and_the_box_opens_after_it() {
    let mut rig = Rig::new(false);
    rig.settle_and_focus();
    assert_eq!(rig.state, Clear);
    rig.rejected = true;
    let top = rig.check.top();

    let [press, release, next] = rig.click_below();
    assert_eq!(press, (Due, top));
    assert_eq!(release, (Due, top));
    assert!(rig.checked, "the click must land on the checkbox");
    assert_eq!(next.0, Shown);
    assert!(next.1 > top, "the row drops after the click");
}

#[test]
fn losing_the_window_during_a_press_does_not_lock_the_box() {
    let mut rig = Rig::new(true);
    rig.settle_and_focus();
    rig.rejected = false;

    rig.frame(input(button(rig.check.center(), true), true));
    assert_eq!(rig.state, Held);
    // 뗌이 오지 않은 채 포커스를 잃어도, 돌아와도 칸이 잠기지 않는다.
    rig.frame(input(vec![], false));
    assert_eq!(rig.state, Clear);
    assert!(rig.ctx.input(|i| i.pointer.any_down()));
    rig.frame(input(vec![], true));
    assert_eq!(rig.state, Clear);
    rig.rejected = true;
    rig.frame(input(vec![], true));
    assert_eq!(
        rig.state, Shown,
        "a press from before the loss holds nothing"
    );
}

#[test]
fn a_keyboard_exit_moves_the_box_on_the_same_frame() {
    let mut rig = Rig::new(true);
    rig.settle_and_focus();
    rig.rejected = false;
    rig.ctx
        .memory_mut(|m| m.surrender_focus(egui::Id::new("field")));
    rig.frame(input(vec![], true));
    assert_eq!(rig.state, Clear);
}
