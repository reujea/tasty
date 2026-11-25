//! 설정 창 L1 탭과 L2 사이드바 행의 키보드 포커스 링(정본 window-tab-focus.jsx). 링 조건은 egui `has_focus()`다.
//! 이 탭과 행은 클릭으로 포커스를 요청하지 않으므로 그 포커스는 Tab·Shift+Tab·화살표 이동에서만 생긴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// L1 탭 링을 그리고 탭이 눌렸는지 돌려준다. 링은 탭 안쪽에 그리고 활성 밑줄(`underline` 두께)과
/// focus-ring-offset 만큼 떨어진다.
pub(super) fn l1_ring_then_clicked(
    ui: &egui::Ui,
    th: &Theme,
    resp: &egui::Response,
    underline: LogicalPx,
) -> bool {
    if resp.has_focus() {
        tasty_ui_widgets::paint_tab_inset_ring(
            ui.painter(),
            th,
            resp.rect,
            underline,
            th.border_focus().to_egui(),
        );
    }
    resp.clicked()
}

/// L2 행 링을 그리고 행이 눌렸는지 돌려준다. 링은 행에서 focus-ring-offset 띄운 컨트롤 링이다. 행 사이
/// 간격(spacing-xs)이 링 폭 이상이라 행 안에서 그려도 이웃 행 채움에 덮이지 않는다.
pub(super) fn l2_ring_then_clicked(
    ui: &egui::Ui,
    th: &Theme,
    resp: &egui::Response,
    radius: f32,
) -> bool {
    reveal_if_newly_focused(ui, th, resp);
    if resp.has_focus() {
        tasty_ui_widgets::paint_control_ring(
            ui.painter(),
            th,
            resp.rect,
            radius,
            tasty_ui_widgets::control_ring_gap(th),
            th.border_focus().to_egui(),
        );
    }
    resp.clicked()
}

/// 직전 패스까지 본 포커스 행을 적는 egui 임시 데이터 키.
fn seen_focus_id() -> egui::Id {
    egui::Id::new("tasty::settings::l2_seen_focus")
}

/// 키보드 포커스가 새로 들어온 L2 행을 링 바깥까지 보이게 스크롤한다. 스크롤 영역 안 행은 화면 밖에서도 Tab
/// 포커스를 받는다. 본 포커스 행과 비교하므로 Tab과 화살표 이동을 모두 잡고, 포커스가 그 행을 떠나면 기록을 지워
/// 같은 행으로 돌아올 때 다시 보인다.
fn reveal_if_newly_focused(ui: &egui::Ui, th: &Theme, resp: &egui::Response) {
    let key = seen_focus_id();
    let seen = ui.ctx().data(|d| d.get_temp::<egui::Id>(key)) == Some(resp.id);
    if resp.has_focus() && !seen {
        let reach = tasty_ui_widgets::control_ring_gap(th) + th.focus_ring_width;
        ui.scroll_to_rect(resp.rect.expand(reach.value()), None);
        ui.ctx().data_mut(|d| d.insert_temp(key, resp.id));
    } else if !resp.has_focus() && seen {
        ui.ctx().data_mut(|d| d.remove::<egui::Id>(key));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{l1_tab_button, sidebar_row};
    use tasty_type_appearance::theme::Theme;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    fn tab() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    /// 한 패스를 그리고 (그린 칸 사각형들, focus_ring_width 획 목록)을 돌려준다.
    fn pass(
        ctx: &egui::Context,
        th: &Theme,
        events: Vec<egui::Event>,
        draw: &mut dyn FnMut(&mut egui::Ui) -> Vec<egui::Rect>,
    ) -> (
        Vec<egui::Rect>,
        Vec<(egui::Rect, egui::StrokeKind, egui::Color32)>,
    ) {
        let mut cells = Vec::new();
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| cells = draw(ui));
            },
        );
        let rings = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.stroke.width == th.focus_ring_width.value() => {
                    Some((r.rect, r.stroke_kind, r.stroke.color))
                }
                _ => None,
            })
            .collect();
        (cells, rings)
    }

    fn l1_tabs(ui: &mut egui::Ui, th: &Theme) -> Vec<egui::Rect> {
        ui.horizontal(|ui| {
            ui.set_height(40.0);
            ["General", "Appearance"]
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let left = ui.cursor().min.x;
                    l1_tab_button(ui, th, l, i == 1);
                    egui::Rect::from_x_y_ranges(
                        left..=ui.min_rect().right(),
                        ui.min_rect().y_range(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .inner
    }

    /// L1 탭은 선택 여부와 무관하게 탭 안쪽, 밑줄 두께 + focus-ring-offset 만큼 들어온 자리에 링을 그린다.
    #[test]
    fn tab_rings_the_l1_tab_inside_clear_of_the_underline() {
        let th = theme();
        let ctx = egui::Context::default();
        let underline = super::super::SETTINGS_TAB_UNDERLINE.value();
        let inset = underline + th.focus_ring_offset.value();
        let mut draw = |ui: &mut egui::Ui| l1_tabs(ui, &th);
        let (_, rest) = pass(&ctx, &th, vec![], &mut draw);
        assert!(rest.is_empty(), "{rest:?}");
        // 첫 Tab은 선택 안 된 General, 둘째 Tab은 선택된 Appearance에 간다.
        for k in 0..2 {
            pass(&ctx, &th, vec![tab()], &mut draw);
            let (cells, rings) = pass(&ctx, &th, vec![], &mut draw);
            assert_eq!(
                rings,
                vec![(
                    cells[k].shrink(inset),
                    egui::StrokeKind::Inside,
                    th.border_focus().to_egui()
                )]
            );
        }
    }

    /// L2 행은 Tab으로 포커스를 받고 행에서 focus-ring-offset 띄운 링을 그린다.
    #[test]
    fn tab_rings_the_l2_row_one_offset_off_its_box() {
        let th = theme();
        let ctx = egui::Context::default();
        let mut draw = |ui: &mut egui::Ui| {
            ["Font", "Cursor", "Bell"]
                .iter()
                .map(|l| {
                    let top = ui.cursor().min;
                    sidebar_row(ui, &th, l, false, *l == "Font");
                    egui::Rect::from_min_max(
                        top,
                        egui::pos2(ui.max_rect().right(), ui.min_rect().bottom()),
                    )
                })
                .collect()
        };
        let (_, rest) = pass(&ctx, &th, vec![], &mut draw);
        assert!(rest.is_empty());
        pass(&ctx, &th, vec![tab()], &mut draw);
        pass(&ctx, &th, vec![tab()], &mut draw);
        let (cells, rings) = pass(&ctx, &th, vec![], &mut draw);
        assert_eq!(rings.len(), 1, "{rings:?}");
        let (r, kind, color) = rings[0];
        assert_eq!(kind, egui::StrokeKind::Outside);
        assert_eq!(color, th.border_focus().to_egui());
        assert_eq!(r.min.y, cells[1].min.y - th.focus_ring_offset.value());
    }

    /// 스크롤 영역 밖 행으로 Tab이 오면 그 행의 링 바깥까지 보이게 스크롤한다.
    #[test]
    fn a_newly_focused_l2_row_scrolls_into_view_with_its_ring() {
        let th = theme();
        let ctx = egui::Context::default();
        let labels: Vec<String> = (0..40).map(|i| format!("Row {i}")).collect();
        let clip = std::cell::Cell::new(egui::Rect::NOTHING);
        let frame = |events: Vec<egui::Event>| {
            drop(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300.0, 120.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |c| {
                    egui::CentralPanel::default().show(c, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            clip.set(ui.clip_rect());
                            for l in &labels {
                                sidebar_row(ui, &th, l, false, false);
                            }
                        });
                    });
                },
            ));
        };
        let focused_rect = |ctx: &egui::Context| {
            ctx.memory(|m| m.focused())
                .and_then(|id| ctx.read_response(id))
                .expect("a row has focus")
                .rect
        };
        frame(vec![]);
        for _ in 0..12 {
            frame(vec![tab()]);
        }
        let before = focused_rect(&ctx);
        assert!(
            !clip.get().contains_rect(before),
            "row 11 starts out of view"
        );
        frame(vec![]);
        frame(vec![]);
        let reach = th.focus_ring_offset.value() + th.focus_ring_width.value();
        let row = focused_rect(&ctx);
        assert!(
            clip.get().contains_rect(row.expand(reach)),
            "{row:?} in {:?}",
            clip.get()
        );
    }
}
