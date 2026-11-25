//! 원격 창 탭 줄의 탭 크기와 간격(정본 remote_tool.jsx TabBtn). 그린 도형에서 잰다.

use egui::{Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{TabStripData, draw_tab_strip};

const LABELS: [&str; 3] = ["Remote profiles", "Attach", "Passkeys"];

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 탭 줄을 두 번 그리고(첫 프레임은 글자 배치가 비어 있을 수 있다) 마지막 프레임의 도형을 평탄화한다.
fn painted(th: &Theme, active: usize) -> Vec<egui::Shape> {
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let out = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 200.0))),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(c, |ui| {
                        let x_range = ui.max_rect().x_range();
                        draw_tab_strip(
                            ui,
                            th,
                            &TabStripData {
                                labels: &LABELS,
                                active,
                                x_range,
                            },
                        );
                    });
            },
        );
        shapes.clear();
        for s in out.shapes {
            flatten(s.shape, &mut shapes);
        }
    }
    shapes
}

fn flatten(shape: egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(v) => {
            for s in v {
                flatten(s, out);
            }
        }
        other => out.push(other),
    }
}

/// `label` 글자 도형의 화면 사각형.
fn label_rect(shapes: &[egui::Shape], label: &str) -> Rect {
    shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::Text(t) if t.galley.text() == label => {
                Some(t.galley.rect.translate(t.pos.to_vec2()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{label}` 탭 글자가 없다"))
}

/// 활성 탭 밑줄(accent_primary 가로선)의 가로 범위. 밑줄은 탭 사각형 폭 전체다.
fn underline(shapes: &[egui::Shape], th: &Theme) -> egui::Rangef {
    let accent: egui::Color32 = th.accent_primary().into();
    shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::LineSegment { points, stroke } if stroke.color == accent => {
                Some(egui::Rangef::new(points[0].x, points[1].x))
            }
            _ => None,
        })
        .expect("활성 탭 밑줄이 없다")
}

/// 탭 하나의 폭은 글자 폭에 좌우 `spacing_md` 를 더한 값이고, 글자는 탭 가운데에 있다.
#[test]
fn a_tab_pads_its_label_by_space_md_on_each_side() {
    let th = theme();
    let pad = th.spacing_md.value();
    for (i, label) in LABELS.iter().enumerate() {
        let shapes = painted(&th, i);
        let tab = underline(&shapes, &th);
        let text = label_rect(&shapes, label);
        assert!(
            (text.left() - tab.min - pad).abs() < 0.5 && (tab.max - text.right() - pad).abs() < 0.5,
            "`{label}` 탭 {tab:?} 안 글자 {:?} 의 좌우 여백이 spacing_md {pad} 가 아니다",
            text.x_range()
        );
    }
}

/// 이웃한 두 탭 사이는 `remote_tab_gap` 이다. 탭마다 활성으로 그려 밑줄로 탭 사각형을 잰다.
#[test]
fn neighbouring_tabs_are_remote_tab_gap_apart() {
    let th = theme();
    let tabs: Vec<egui::Rangef> = (0..LABELS.len())
        .map(|i| underline(&painted(&th, i), &th))
        .collect();
    for pair in tabs.windows(2) {
        let gap = pair[1].min - pair[0].max;
        assert!(
            (gap - th.remote_tab_gap().value()).abs() < 0.5,
            "탭 {:?} 와 {:?} 사이 {gap} 가 remote_tab_gap 이 아니다",
            pair[0],
            pair[1]
        );
    }
}
