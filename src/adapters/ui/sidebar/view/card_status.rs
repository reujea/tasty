//! 워크스페이스 카드의 점 칸(상태 점·점유 링·로딩 스피너·전환 키캡)과 카드 툴팁.

use super::WorkspaceEntryView;
use crate::adapters::ui::attach_loading as loading;
use crate::theme::Theme;
use tasty_ui_widgets::{LoadingPlacement, RowParent, RowState};

/// 카드 한 장을 그린 결과. 카드 응답 뒤에 툴팁을 등록할 점유 점 칸과 REMOTE 줄 영역을 함께 돌려준다.
pub(super) struct CardPaint {
    pub(super) rect: egui::Rect,
    pub(super) loading: bool,
    pub(super) occupied_dot: Option<egui::Rect>,
    pub(super) running_count: Option<egui::Rect>,
    pub(super) remote_line: Option<egui::Rect>,
}

/// 점 칸과 숫자 칸을 그린 결과. 스피너 표시 여부와 툴팁을 등록할 점 칸, 숫자 칸이다.
#[derive(Default)]
pub(super) struct DotSlot {
    pub(super) loading: bool,
    pub(super) occupied_dot: Option<egui::Rect>,
    pub(super) running_count: Option<egui::Rect>,
}

/// 가로 레이아웃에 점 칸과 숫자 칸을 예약하고 그린다. 점 칸 안의 순서는 받침 위 점, 스피너, 키캡이다.
/// 행 채움(활성·hover·투명)은 스피너 받침에 겹친다. 뒤이어 놓는 항목의 간격은 `workspace-dot-gap`이다.
pub(super) fn paint_dot_slot(
    ui: &mut egui::Ui,
    th: &Theme,
    ws: &WorkspaceEntryView,
    state: RowState,
    // Some(digit) 면 status dot 자리에 숫자 키캡(switch-number overlay)을 그린다.
    switch_digit: Option<&str>,
    // switch-number overlay 등장 페이드 계수(0..=1, motion-ui-fast 90ms).
    switch_fade: f32,
) -> DotSlot {
    // 실행 상태는 점 색, 다른 클라이언트 점유는 링, mirror는 별도 줄로 표시한다.
    // 슬롯은 attached ring 전체를 담는 폭으로 모든 행에서 예약해 이름 시작 위치를 고정한다.
    // 높이는 제목 라벨이 실제로 차지하는 한 줄 높이다. 슬롯이 행을 키우지 않는다.
    let title_row_h =
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(th.font_size_body.value())));
    let dot_slot = egui::vec2(th.workspace_dot_slot().value(), title_row_h);
    ui.spacing_mut().item_spacing.x = th.workspace_running_count_gap().value();
    let (dot_rect, _) = ui.allocate_exact_size(dot_slot, egui::Sense::hover());
    let mut occupied_dot = None;
    if switch_digit.is_none() {
        let dot_color: egui::Color32 = match (ws.busy_count > 0, ws.is_active) {
            (true, true) => th.status_dot_success_active().into(),
            (true, false) => th.status_dot_success().into(),
            (false, true) => th.status_dot_idle_active().into(),
            (false, false) => th.status_dot_idle().into(),
        };
        // 키캡과 배율을 맞추도록 토큰 지름을 쓴다. idle에 맞는 BadgeVariant가 없어 직접 그린다.
        let dot_r = th.badge_dot_size().value() * 0.5;
        ui.painter()
            .circle_filled(dot_rect.center(), dot_r, dot_color);
        // 링의 offset은 점의 바깥쪽에서 링 안쪽까지의 거리다.
        if ws.attached {
            let ring_w = th.status_dot_attached_ring_width().value();
            ui.painter().circle_stroke(
                dot_rect.center(),
                dot_r + th.status_dot_attached_ring_offset().value() + ring_w * 0.5,
                egui::Stroke::new(
                    ring_w,
                    if ws.is_active {
                        th.status_dot_attached_ring_active()
                    } else {
                        th.status_dot_attached_ring()
                    },
                ),
            );
        }
        if ws.attached && ws.busy_count == 0 {
            occupied_dot = Some(dot_rect);
        }
    }
    let loading = loading::paint_sidebar_loading(
        ui,
        th,
        ws.loading_since,
        dot_rect,
        LoadingPlacement::Lnb,
        state.fill(th),
    );
    if let Some(digit) = switch_digit {
        // 같은 슬롯 가운데 키캡을 스피너 위에 그려 이름 시작 위치를 유지한다.
        crate::adapters::ui::switch_overlay::paint_keycap(
            ui.painter(),
            th,
            dot_rect.center(),
            digit,
            ws.is_active,
            switch_fade,
        );
    }
    ui.spacing_mut().item_spacing.x = th.workspace_dot_gap().value();
    // 점과 숫자는 같은 값을 읽는다. 키캡·링·스피너는 점 칸 안에 있어 숫자를 움직이지 않는다.
    let running_count = tasty_ui_widgets::running_count_column(
        ui,
        th,
        ws.busy_count,
        title_row_h,
        state.meta(th, RowParent::Mantle),
    );
    DotSlot {
        loading,
        occupied_dot,
        running_count,
    }
}

/// 숫자 칸의 툴팁 문구. 칸의 `99+`와 달리 정확한 수를 쓴다.
pub(super) fn running_count_tooltip(count: usize) -> String {
    crate::i18n::t_count(
        "sidebar.running_surfaces_tooltip",
        count as u64,
        &[&count.to_string()],
    )
}

/// 카드 응답 뒤에 점유 점·숫자 칸·REMOTE 줄 툴팁과 로딩 카드 툴팁을 등록한다.
pub(super) fn show_card_tooltips(
    ui: &egui::Ui,
    th: &Theme,
    ws: &WorkspaceEntryView,
    card: &CardPaint,
    card_response: &egui::Response,
    occupied_hover: &str,
    mirror_hover: &str,
) {
    // 카드 전체가 클릭 위젯이라 그보다 먼저 등록한 hover 영역은 hover 되지 않는다. 점유 점, 숫자 칸,
    // REMOTE 줄의 툴팁은 카드 응답 뒤에 등록한다.
    let tip = |rect: Option<egui::Rect>, key: &str, text: &str| {
        rect.is_some_and(|r| {
            let zone = ui.interact(r, card_response.id.with(key), egui::Sense::hover());
            zone.on_hover_text(text).hovered()
        })
    };
    let on_occupied_dot = tip(card.occupied_dot, "occupied_dot", occupied_hover);
    let on_remote_pill = tip(card.remote_line, "remote_line", mirror_hover);
    let running = card
        .running_count
        .map(|_| running_count_tooltip(ws.busy_count));
    let on_running_count = tip(
        card.running_count,
        "running_count",
        running.as_deref().unwrap_or_default(),
    );
    // 카드 로딩은 attach 응답 대기에서만 켜진다. 오버레이 자체에는 툴팁이 없고 카드 툴팁에 둘째 줄을 둔다.
    // 점유 점, 숫자 칸, REMOTE 줄에서는 그 툴팁이 뜨므로 카드 툴팁을 겹쳐 띄우지 않는다.
    if card.loading
        && !on_occupied_dot
        && !on_remote_pill
        && !on_running_count
        && tasty_ui_widgets::tooltip_hover_delay_elapsed(
            ui.ctx(),
            th,
            card_response.id,
            card_response.hovered(),
        )
    {
        tasty_ui_widgets::Tooltip::new(&ws.name)
            .second_line(crate::i18n::t("loading.remote_attach"))
            .id_source(card_response.id)
            .show(ui, th, card.rect);
    }
}
