//! 원격 창 프로토콜 필터 목록의 스크롤 상한(정본 remote-filter-list-max-height). 그린 목록의 잘림 사각형으로 잰다.

use std::collections::HashSet;

use egui::{Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{ProtocolFilterItem, ProtocolFilterLabels, draw_protocol_filter_body};

const NAMES: [&str; 12] = [
    "ssh", "smb", "http", "ftp", "sftp", "rdp", "vnc", "telnet", "mosh", "nfs", "webdav", "s3",
];

/// 항목이 상한보다 많은 목록을 두 번 그리고, 첫 항목 글자가 잘리는 사각형에서 egui 가 위아래로 더하는
/// `clip_rect_margin` 을 걷어 낸 스크롤 뷰포트를 돌려준다.
fn list_viewport(th: &Theme) -> Rect {
    let ctx = egui::Context::default();
    let items: Vec<ProtocolFilterItem<'_>> = NAMES
        .iter()
        .map(|name| ProtocolFilterItem {
            name,
            unknown: false,
        })
        .collect();
    let labels = ProtocolFilterLabels {
        title: "Protocols",
        select_all: "Select all",
        deselect_all: "Deselect all",
        reset: "Reset",
        apply: "Apply",
        unknown: "unknown",
        unknown_hint: "",
    };
    let mut draft: HashSet<String> = NAMES.iter().map(|n| n.to_string()).collect();
    let mut viewport = None;
    for _ in 0..2 {
        let out = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 800.0))),
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    draw_protocol_filter_body(ui, th, &items, &labels, &mut draft);
                });
            },
        );
        viewport = out.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == NAMES[0] => Some(s.clip_rect),
            _ => None,
        });
    }
    let margin = ctx.style().visuals.clip_rect_margin;
    viewport
        .expect("첫 항목 글자가 없다")
        .shrink2(vec2(0.0, margin))
}

/// 목록이 길면 스크롤 뷰포트 높이가 `remote_filter_list_max_height` 다. 상한은 UI 배율을 따른다.
#[test]
fn a_long_protocol_list_scrolls_at_remote_filter_list_max_height() {
    for zoom in [0.85, 1.0, 1.2] {
        let th = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
        let viewport = list_viewport(&th);
        let want = th.remote_filter_list_max_height().value();
        assert!(
            (viewport.height() - want).abs() < 0.5,
            "배율 {zoom}: 목록 뷰포트 {viewport:?} 높이가 remote_filter_list_max_height {want} 가 아니다"
        );
    }
}
