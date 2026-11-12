//! Held pastes against real egui frames: the input of each frame goes through `HeldPastes::frame`
//! as `GpuState::take_egui_input` does, then through `egui::Context::run` with three single-line
//! fields B, A, C in that focus order. A asked for the paste.
use super::super::HeldPastes;
use std::time::Instant;

const PASTED: &str = "PASTED";

struct Fields {
    ctx: egui::Context,
    held: HeldPastes,
    b: String,
    a: String,
    c: String,
    rect_b: egui::Rect,
}

fn id(name: &str) -> egui::Id {
    egui::Id::new(name)
}

impl Fields {
    /// Runs two frames and leaves A focused.
    fn new() -> Self {
        let mut fields = Self {
            ctx: egui::Context::default(),
            held: HeldPastes::default(),
            b: String::new(),
            a: String::new(),
            c: String::new(),
            rect_b: egui::Rect::NOTHING,
        };
        fields.frame(Vec::new());
        fields.ctx.memory_mut(|m| m.request_focus(id("a")));
        fields.frame(Vec::new());
        assert_eq!(fields.focused(), Some(id("a")));
        fields
    }

    fn focused(&self) -> Option<egui::Id> {
        self.ctx.memory(|m| m.focused())
    }

    /// The paste for A finished reading; `App` polls before the next frame takes its input.
    fn arrive(&mut self) {
        self.held
            .add(vec![(id("a").value(), PASTED.to_string())], Instant::now());
    }

    fn frame(&mut self, mut events: Vec<egui::Event>) {
        let focused = self.focused().map_or(0, |id| id.value());
        let pastes = self.held.frame(&events, focused, Instant::now());
        events.extend(pastes.into_iter().map(egui::Event::Paste));
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        let (b, a, c) = (&mut self.b, &mut self.a, &mut self.c);
        let mut rect_b = egui::Rect::NOTHING;
        let _output = self.ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect_b = ui.add(egui::TextEdit::singleline(b).id(id("b"))).rect;
                ui.add(egui::TextEdit::singleline(a).id(id("a")));
                ui.add(egui::TextEdit::singleline(c).id(id("c")));
            });
        });
        self.rect_b = rect_b;
    }

    fn frames(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(Vec::new());
        }
    }
}

fn shift_tab() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    }
}

fn tab() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn click_on(rect: egui::Rect) -> Vec<egui::Event> {
    let pos = rect.center();
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(pos), button(true), button(false)]
}

#[test]
fn a_paste_reaches_the_field_that_asked_when_focus_stays() {
    let mut f = Fields::new();
    f.arrive();
    f.frames(2);
    assert_eq!(f.a, PASTED);
    assert_eq!((f.b.as_str(), f.c.as_str()), ("", ""));
}

/// egui moves focus for Shift+Tab only at the start of the next frame.
#[test]
fn shift_tab_in_the_frame_a_paste_arrives_does_not_hand_it_to_the_field_before() {
    // The paste arrives with the Shift+Tab frame, and right after it.
    for arrive_after in [false, true] {
        let mut f = Fields::new();
        if !arrive_after {
            f.arrive();
        }
        f.frame(vec![shift_tab()]);
        if arrive_after {
            f.arrive();
        }
        f.frames(4);
        assert_eq!(f.focused(), Some(id("b")), "Shift+Tab moved focus to B");
        assert_eq!(f.b, "", "arrive_after={arrive_after}");
        assert_eq!((f.a.as_str(), f.c.as_str()), ("", ""));
        assert!(f.held.is_empty());
    }
}

#[test]
fn tab_in_the_frame_a_paste_arrives_does_not_hand_it_to_the_next_field() {
    let mut f = Fields::new();
    f.arrive();
    f.frame(vec![tab()]);
    f.frames(4);
    assert_eq!(f.focused(), Some(id("c")), "Tab moved focus to C");
    assert_eq!((f.b.as_str(), f.a.as_str(), f.c.as_str()), ("", "", ""));
    assert!(f.held.is_empty());
}

#[test]
fn a_click_on_another_field_in_the_frame_a_paste_arrives_drops_it() {
    let mut f = Fields::new();
    f.arrive();
    let click = click_on(f.rect_b);
    f.frame(click);
    f.frames(4);
    assert_eq!(f.focused(), Some(id("b")), "the click moved focus to B");
    assert_eq!((f.b.as_str(), f.a.as_str(), f.c.as_str()), ("", "", ""));
    assert!(f.held.is_empty());
}
