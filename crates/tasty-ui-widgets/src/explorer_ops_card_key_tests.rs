//! 충돌 카드는 키를 읽지 않는다. Enter 는 호출자가 넘긴 확정 판정값으로만 받는다.
use super::*;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

fn props() -> ConflictProps<'static> {
    ConflictProps {
        title: "a.txt already exists",
        in_folder: "/w",
        existing_label: "Existing",
        existing: "1 B",
        incoming_label: "Incoming",
        incoming: "1 B",
        no_merge: None,
        apply_all: None,
        cancel_rest: "Cancel the rest",
        skip: "Skip",
        replace: Some("Replace"),
        keep_both: "Keep both",
    }
}

/// 카드만 한 프레임 그린다. `events` 는 그 프레임 입력이다.
fn card(events: Vec<egui::Event>, confirm: bool) -> Option<ConflictPick> {
    let ctx = egui::Context::default();
    let theme = theme();
    let mut pick = None;
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut apply_all = false;
                pick = conflict_card(ui, &theme, &props(), &mut apply_all, confirm);
            });
        },
    ));
    pick
}

fn press(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// T-6b-1c: Given 충돌 카드, When Esc 누름이 그 프레임 입력에 있으면, Then 카드는 답을 고르지 않는다.
#[test]
fn the_conflict_card_does_not_read_escape() {
    assert_eq!(card(vec![press(egui::Key::Escape)], false), None);
}

/// T-6b-1d: Given 충돌 카드, When 호출자가 확정 판정값을 넘기면, Then Keep both 를 고른다.
#[test]
fn the_conflict_card_keeps_both_on_confirm() {
    assert_eq!(card(Vec::new(), true), Some(ConflictPick::KeepBoth));
}
