//! 트리 행의 셰브런 칸·글리프·이름·meta 사이 간격(정본 TreeRow 의 gap = tree-row-gap). 그린 도형에서 잰다.

use egui::{Color32, Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{tree_row, tree_row_icon_center};

const LABEL: &str = "src";
const META: &str = "12";
/// 시험 아이콘이 칠하는 색. 행이 그리는 다른 도형과 겹치지 않는다.
const ICON_MARK: Color32 = Color32::GOLD;
/// 셰브런 칸 폭. 위젯이 배율과 무관하게 고정으로 둔다.
const CHEVRON_SLOT: f32 = 14.0;

fn theme_at(zoom: f32) -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom)
}

/// 한 행에서 잰 도형 위치.
struct Drawn {
    /// 선택 바탕 사각형(행 전체).
    row: Rect,
    /// 셰브런 꺾은선의 가로 가운데.
    chevron_center_x: f32,
    /// 시험 아이콘이 칠한 사각형.
    icon: Rect,
    /// 이름 글자를 놓은 x.
    label_x: f32,
    /// 이름 글자를 자르는 영역의 오른쪽 끝.
    label_clip_right: f32,
    /// meta 글자의 왼쪽 끝.
    meta_left: f32,
}

/// 펼칠 수 있는 선택 행을 두 번 그리고(첫 프레임은 글자 배치가 비어 있을 수 있다) 마지막 프레임에서 잰다.
fn drawn(th: &Theme, depth: u16) -> Drawn {
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let paint_icon = |ui: &mut egui::Ui, rect: Rect, _: Color32| {
        ui.painter().rect_filled(rect, 0.0, ICON_MARK);
    };
    for _ in 0..2 {
        let out = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(c, |ui| {
                        tree_row(
                            ui,
                            th,
                            depth,
                            true,
                            false,
                            Some(&paint_icon),
                            LABEL,
                            Some(META),
                            true,
                        );
                    });
            },
        );
        shapes.clear();
        for s in out.shapes {
            flatten(s.clip_rect, s.shape, &mut shapes);
        }
    }
    let bg: Color32 = th.tree_row_bg_active().into();
    let rect_filled = |fill: Color32| {
        shapes.iter().find_map(|(_, s)| match s {
            egui::Shape::Rect(r) if r.fill == fill => Some(r.rect),
            _ => None,
        })
    };
    let text = |label: &str| {
        shapes.iter().find_map(|(clip, s)| match s {
            egui::Shape::Text(t) if t.galley.text() == label => Some((*clip, t.pos.x)),
            _ => None,
        })
    };
    let chevron_xs: Vec<f32> = shapes
        .iter()
        .find_map(|(_, s)| match s {
            egui::Shape::Path(p) => Some(p.points.iter().map(|p| p.x).collect()),
            _ => None,
        })
        .expect("셰브런 꺾은선이 없다");
    let (min, max) = chevron_xs
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    let (label_clip, label_x) = text(LABEL).expect("이름 글자가 없다");
    let (_, meta_x) = text(META).expect("meta 글자가 없다");
    Drawn {
        row: rect_filled(bg).expect("선택 바탕이 없다"),
        chevron_center_x: (min + max) * 0.5,
        icon: rect_filled(ICON_MARK).expect("아이콘이 없다"),
        label_x,
        label_clip_right: label_clip.right(),
        meta_left: meta_x,
    }
}

fn flatten(clip: Rect, shape: egui::Shape, out: &mut Vec<(Rect, egui::Shape)>) {
    match shape {
        egui::Shape::Vec(v) => {
            for s in v {
                flatten(clip, s, out);
            }
        }
        other => out.push((clip, other)),
    }
}

/// 이름 x(행 왼쪽 기준)는 배율 1에서 깊이 0 이 40, 깊이 1 이 52 다(정본 킷 사이드바 측정과 같다).
#[test]
fn the_name_starts_at_the_canon_x_for_depth_zero_and_one() {
    for (zoom, depth0, depth1) in [(0.85, 37.0, 47.0), (1.0, 40.0, 52.0), (1.2, 43.0, 57.0)] {
        let th = theme_at(zoom);
        for (depth, want) in [(0, depth0), (1, depth1)] {
            let d = drawn(&th, depth);
            let x = d.label_x - d.row.left();
            assert!(
                (x - want).abs() < 0.5,
                "배율 {zoom} 깊이 {depth}: 이름 x {x} 가 {want} 가 아니다"
            );
        }
    }
}

/// 셰브런 칸 뒤, 글리프 뒤, meta 앞 간격은 모두 `tree_row_gap` 이다.
#[test]
fn the_row_parts_are_tree_row_gap_apart() {
    for zoom in [0.85, 1.0, 1.2] {
        let th = theme_at(zoom);
        let gap = th.tree_row_gap().value();
        for depth in [0, 1] {
            let d = drawn(&th, depth);
            let after_chevron = d.icon.left() - (d.chevron_center_x + CHEVRON_SLOT * 0.5);
            let after_glyph = d.label_x - d.icon.right();
            let before_meta = d.meta_left - d.label_clip_right;
            for (part, got) in [
                ("셰브런 칸 뒤", after_chevron),
                ("글리프 뒤", after_glyph),
                ("meta 앞", before_meta),
            ] {
                assert!(
                    (got - gap).abs() < 0.5,
                    "배율 {zoom} 깊이 {depth}: {part} 간격 {got} 가 tree_row_gap {gap} 가 아니다"
                );
            }
        }
    }
}

/// `tree_row_icon_center` 는 깊이 0 행이 그린 아이콘 가운데와 같다(배율 1에서 29).
/// 이름 바꾸기 줄의 글리프가 이 값으로 아래 행 아이콘에 맞춘다.
#[test]
fn the_icon_center_helper_is_where_the_row_draws_its_icon() {
    for zoom in [0.85, 1.0, 1.2] {
        let th = theme_at(zoom);
        let d = drawn(&th, 0);
        let drawn_center = d.icon.center().x - d.row.left();
        let helper = tree_row_icon_center(&th).value();
        assert!(
            (drawn_center - helper).abs() < 0.5,
            "배율 {zoom}: 그린 아이콘 가운데 {drawn_center} 와 tree_row_icon_center {helper} 가 다르다"
        );
    }
    let at_one = tree_row_icon_center(&theme_at(1.0)).value();
    assert!(
        (at_one - 29.0).abs() < 0.5,
        "배율 1: {at_one} 가 29 가 아니다"
    );
}
