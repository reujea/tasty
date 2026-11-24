//! 원격 대기열 머리 줄의 세 조각(앞 문구 · 호스트 표시 · 뒤 문구) 사이가 explorer-remote-queue-head-gap 인지 본다.

use std::cell::Cell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{HostMark, QueueHeader, QueueRowProps, op_queue_popover};

#[test]
fn queue_header_pieces_sit_one_head_gap_apart() {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let glyph = Cell::new(egui::Rect::NOTHING);
    let paint_glyph = |_: &mut egui::Ui, rect: egui::Rect, _: egui::Color32| glyph.set(rect);
    let ctx = egui::Context::default();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            op_queue_popover(
                ui,
                &theme,
                egui::Id::new("queue"),
                egui::pos2(400.0, 400.0),
                &[QueueRowProps {
                    title: "row",
                    sub: "sub",
                    running: true,
                    remove_tip: "Cancel",
                }],
                Some(&QueueHeader {
                    before: "On ",
                    host: HostMark {
                        name: "hostname",
                        empty_label: "Remote",
                        paint_glyph: &paint_glyph,
                    },
                    after: " · one at a time",
                }),
                &|_, _, _| {},
                &|_, _, _| {},
            );
        });
    });
    let text_rect = |wanted: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == wanted => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text shape {wanted:?}"))
    };
    let gap = theme.explorer_remote_queue_head_gap().value();
    let before = text_rect("On");
    let host = text_rect("hostname");
    let after = text_rect("· one at a time");
    let glyph = glyph.get();
    assert!(
        (glyph.left() - before.right() - gap).abs() < 0.5,
        "{before:?} {glyph:?}"
    );
    assert!(
        (after.left() - host.right() - gap).abs() < 0.5,
        "{host:?} {after:?}"
    );
}
