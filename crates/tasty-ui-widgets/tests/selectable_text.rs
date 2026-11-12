//! 선택 가능한 표시 텍스트는 편집 위젯이 아니다. 실제 egui Context로 프레임을 돌려
//! 클릭 뒤 캐럿이 없고, 키 입력이 글자를 바꾸지 않고, 드래그 선택과 복사는 되는지 본다.
//! 공용 `selectable_text`와 원격 행 제목(`remote_row_title` 안의 title_text)을 같은 단언으로 본다.

// 이유: 테스트에서는 사용하지 않는 반환값을 버리는 것을 허용한다.
#![allow(clippy::let_underscore_must_use)]
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Stroke, pos2, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{TextWrap, remote_row_title, selectable_text};

const TEXT: &str = "gx10 deploy@10.0.4.12";
const NAME: &str = "gx10";
const LABEL: &str = "lab";
/// 캐럿 막대만 칠하는 색. 다른 도형과 섞이지 않게 테마에 없는 egui 상수를 쓴다.
const CARET: Color32 = Color32::GOLD;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Title,
}

impl Kind {
    fn shown(self) -> String {
        match self {
            Kind::Plain => TEXT.to_owned(),
            Kind::Title => format!("{NAME}  ({LABEL})"),
        }
    }
}

struct Frame {
    rect: Rect,
    texts: Vec<String>,
    caret_painted: bool,
    ime: bool,
    copied: Vec<String>,
}

struct Harness {
    ctx: egui::Context,
    theme: Theme,
    kind: Kind,
}

impl Harness {
    fn new(kind: Kind) -> Self {
        let ctx = egui::Context::default();
        let theme = tasty_themes::mocha_fallback();
        ctx.style_mut(|s| {
            s.visuals.text_cursor.blink = false;
            s.visuals.text_cursor.stroke = Stroke::new(2.0, CARET);
            s.animation_time = 0.0;
        });
        Self { ctx, theme, kind }
    }

    fn frame(&mut self, events: Vec<Event>) -> Frame {
        let mut rect = Rect::NOTHING;
        let out = self.ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 200.0))),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::Area::new(egui::Id::new("host"))
                    .fixed_pos(pos2(20.0, 20.0))
                    .fade_in(false)
                    .show(ctx, |ui| {
                        rect = match self.kind {
                            Kind::Plain => {
                                selectable_text(
                                    ui,
                                    TEXT,
                                    self.theme.text_muted(),
                                    self.theme.font_size_caption,
                                    false,
                                    false,
                                    TextWrap::Truncate(LogicalPx(300.0)),
                                )
                                .rect
                            }
                            Kind::Title => {
                                ui.scope(|ui| {
                                    remote_row_title(
                                        ui,
                                        &self.theme,
                                        NAME,
                                        Some(LABEL),
                                        false,
                                        &[],
                                        300.0,
                                    );
                                })
                                .response
                                .rect
                            }
                        };
                    });
            },
        );
        let mut texts = Vec::new();
        let mut caret_painted = false;
        for s in &out.shapes {
            match &s.shape {
                egui::Shape::Text(t) => texts.push(t.galley.job.text.clone()),
                egui::Shape::LineSegment { stroke, .. } => caret_painted |= stroke.color == CARET,
                egui::Shape::Rect(r) => caret_painted |= r.fill == CARET,
                _ => {}
            }
        }
        let copied = out
            .platform_output
            .commands
            .iter()
            .filter_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        Frame {
            rect,
            texts,
            caret_painted,
            ime: out.platform_output.ime.is_some(),
            copied,
        }
    }

    /// Area 의 첫 프레임은 배치를 재는 프레임이라 위치가 다르다. 두 번째 프레임의 영역을 쓴다.
    fn settled_rect(&mut self) -> Rect {
        self.frame(vec![]);
        self.frame(vec![]).rect
    }

    fn focused(&self) -> bool {
        self.ctx.memory(|m| m.focused().is_some())
    }
}

fn press(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// 첫 프레임으로 배치를 잡고 글자 안쪽을 눌렀다 뗀다. 클릭 뒤 프레임을 돌려준다.
fn clicked(h: &mut Harness) -> Frame {
    let rect = h.settled_rect();
    assert!(rect.is_positive(), "위젯 영역이 비었다");
    let at = pos2(rect.left() + 6.0, rect.center().y);
    h.frame(vec![Event::PointerMoved(at)]);
    h.frame(vec![press(at, true)]);
    h.frame(vec![press(at, false)]);
    let after = h.frame(vec![]);
    assert!(
        h.focused(),
        "클릭이 포커스를 주지 않았다 — 시험이 아무것도 가르지 못한다"
    );
    after
}

fn no_caret_after_click(kind: Kind) {
    let mut h = Harness::new(kind);
    let after = clicked(&mut h);
    assert!(!after.caret_painted, "표시 텍스트에 캐럿이 그려졌다");
    assert!(
        !after.ime,
        "표시 텍스트가 IME 출력을 냈다 — 한글 조합이 들어간다"
    );
}

fn keys_do_not_change_text(kind: Kind) {
    let mut h = Harness::new(kind);
    clicked(&mut h);
    let shown = kind.shown();
    for events in [
        vec![Event::Text("a".into())],
        vec![
            Event::Ime(egui::ImeEvent::Enabled),
            Event::Ime(egui::ImeEvent::Preedit("한".into())),
        ],
        vec![Event::Ime(egui::ImeEvent::Commit("한".into()))],
        vec![],
    ] {
        let f = h.frame(events);
        assert_eq!(f.texts, vec![shown.clone()], "키 입력이 표시 글자를 바꿨다");
    }
}

fn drag_selects_and_copies(kind: Kind) {
    let mut h = Harness::new(kind);
    let rect = h.settled_rect();
    let start = pos2(rect.left() + 1.0, rect.center().y);
    // 포인터가 위젯 밖 오른쪽으로 크게 나가도 선택은 줄 끝까지 이어진다(egui #3816).
    let outside = pos2(rect.right() + 200.0, rect.center().y);
    h.frame(vec![Event::PointerMoved(start)]);
    h.frame(vec![press(start, true)]);
    h.frame(vec![Event::PointerMoved(pos2(rect.center().x, start.y))]);
    h.frame(vec![Event::PointerMoved(outside)]);
    h.frame(vec![press(outside, false)]);
    h.frame(vec![]);
    let f = h.frame(vec![Event::Copy]);
    assert_eq!(
        f.copied,
        vec![kind.shown()],
        "드래그 선택 뒤 복사가 줄 전체를 내지 않았다"
    );
}

#[test]
fn a_click_on_display_text_paints_no_caret() {
    no_caret_after_click(Kind::Plain);
}

#[test]
fn a_click_on_a_row_title_paints_no_caret() {
    no_caret_after_click(Kind::Title);
}

#[test]
fn keys_and_ime_do_not_change_display_text() {
    keys_do_not_change_text(Kind::Plain);
}

#[test]
fn keys_and_ime_do_not_change_a_row_title() {
    keys_do_not_change_text(Kind::Title);
}

#[test]
fn display_text_drag_selects_past_its_edge_and_copies() {
    drag_selects_and_copies(Kind::Plain);
}

#[test]
fn a_row_title_drag_selects_past_its_edge_and_copies() {
    drag_selects_and_copies(Kind::Title);
}
