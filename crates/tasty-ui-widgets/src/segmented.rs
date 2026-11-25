//! 서로 배타적인 선택지를 한 줄의 알약들로 묶는다. 클릭한 새 인덱스를 반환하며 색과 치수는 Theme에서 읽는다.
//!
//! 정본 배치(원격 연결 폼 Connection·Passkey Kind 토글, tasty-page overlay-frames.jsx `seg`): 칸마다 따로 그린
//! 알약이 6 간격으로 놓인다. 알약은 높이 control-height, 좌우 padding space-md, radius, 글자 body다. 선택 칸은
//! accent-primary 채움과 같은 색 테두리에 text-on-accent, 나머지는 surface-raised 채움과 border-default 테두리에
//! text-secondary(surface-raised 위 대비 값 text-secondary-raised)다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 알약 사이 기본 배율 간격. 정본 `seg` 묶음의 `gap: 6`이며 대응 토큰이 없다. 알약 높이·padding처럼 UI 배율을
/// 따르므로 [`segment_gap`]이 배율을 곱한다.
const SEGMENT_GAP: LogicalPx = LogicalPx(6.0);

/// 지금 UI 배율의 알약 사이 간격.
pub(crate) fn segment_gap(theme: &Theme) -> LogicalPx {
    SEGMENT_GAP * theme.ui_zoom
}

/// 세그먼트 토글. `selected` 가 현재 활성 index, 새로 클릭된 세그먼트 index 를 반환
/// (변경 없으면 `None`). 묶음은 자기 콘텐츠 폭만큼만 차지한다.
pub fn segmented(
    ui: &mut egui::Ui,
    theme: &Theme,
    labels: &[&str],
    selected: usize,
) -> Option<usize> {
    segment_row(ui, theme, labels, selected, egui::Sense::click(), None)
}

/// [`segmented`]와 같은 묶음을 `sense`로 그린다. 고정 표본은 `Sense::hover()`를 넘겨 포커스 정지점을 만들지 않고,
/// `focus_look`으로 키보드 포커스 모양을 줄 칸을 고른다. `focus_look`이 Some이면 실제 포커스는 보지 않는다.
pub fn segment_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    labels: &[&str],
    selected: usize,
    sense: egui::Sense,
    focus_look: Option<usize>,
) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = segment_gap(theme).value();
        for (i, label) in labels.iter().enumerate() {
            let active = i == selected;
            let resp = segment_chip(ui, theme, label, active, sense);
            let focused = match focus_look {
                Some(f) => f == i,
                None => resp.has_focus(),
            };
            if focused {
                crate::focus_ring::paint_control_ring(
                    ui.painter(),
                    theme,
                    resp.rect,
                    theme.corner_radius.value(),
                    crate::focus_ring::control_ring_gap(theme),
                    theme.border_focus().to_egui(),
                );
            }
            if resp.clicked() && !active {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// 알약 하나를 그린다.
fn segment_chip(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    active: bool,
    sense: egui::Sense,
) -> egui::Response {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER);
    let w = galley.rect.width() + theme.spacing_md.value() * 2.0;
    let h = theme.item_height_interactive.value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), sense);
    let (fill, border, fg) = if active {
        (
            theme.accent_primary(),
            theme.accent_primary(),
            theme.text_on_accent(),
        )
    } else {
        (
            theme.surface_raised(),
            theme.border_default(),
            theme.text_secondary_raised(),
        )
    };
    let radius = theme.corner_radius.value();
    ui.painter().rect(
        rect,
        radius,
        fill.to_egui(),
        egui::Stroke::new(theme.border_width.value(), border.to_egui()),
        egui::StrokeKind::Inside,
    );
    if !active && resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, theme.overlay_hover().to_egui_premultiplied());
    }
    let fg = if !active && resp.hovered() {
        theme.text_on_active()
    } else {
        fg
    };
    let pos = egui::pos2(
        rect.center().x - galley.rect.width() * 0.5,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, fg.to_egui());
    resp
}
