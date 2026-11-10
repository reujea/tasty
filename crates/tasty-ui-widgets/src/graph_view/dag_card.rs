//! 배율별로 표시 내용을 줄이는 DAG 카드. 레이아웃 크기는 유지한다.
//! 기본 표시에서는 상태색·기호·문구를 함께 쓰고, 가장 작게 보일 때는 색만 남긴다.
//! 이름 앞 아이콘은 상태가 아니라 task 종류를 나타낸다. 문구는 호출자가 [`DagCardView`]로 넘긴다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::canvas::{NodeVisual, ellipsize, text_width};
use super::state::Lod;

/// 색·기호·라벨로 함께 표시하는 DAG 노드 상태. 라벨 문구는 호출자가 고른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DagStatus {
    Waiting,
    Ready,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    Unknown,
    /// DAG 요약 전용(`rollup_state: partially_failed`). task 하나의 상태로는 나오지 않는다.
    PartiallyFailed,
}

impl DagStatus {
    /// 개별 task 상태 전체와 표시 순서. DAG 목록 필터는 ROLLUP_ALL을 사용한다.
    pub const ALL: [DagStatus; 8] = [
        DagStatus::Waiting,
        DagStatus::Ready,
        DagStatus::Running,
        DagStatus::Succeeded,
        DagStatus::Failed,
        DagStatus::Cancelled,
        DagStatus::Skipped,
        DagStatus::Unknown,
    ];

    /// DAG 집계에서 반환하는 일곱 상태와 목록 필터 순서. Cancelled는 Skipped로, Unknown은
    /// Waiting으로 집계되므로 목록 필터에서는 두 개별 상태를 제외한다.
    pub const ROLLUP_ALL: [DagStatus; 7] = [
        DagStatus::Waiting,
        DagStatus::Ready,
        DagStatus::Running,
        DagStatus::Succeeded,
        DagStatus::PartiallyFailed,
        DagStatus::Failed,
        DagStatus::Skipped,
    ];

    /// `rollup_state` 문자열(`DagSummary`)에서. 알 수 없으면 `Unknown`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "waiting" => DagStatus::Waiting,
            "ready" => DagStatus::Ready,
            "running" => DagStatus::Running,
            "succeeded" => DagStatus::Succeeded,
            "failed" => DagStatus::Failed,
            "cancelled" => DagStatus::Cancelled,
            "skipped" => DagStatus::Skipped,
            "partially_failed" => DagStatus::PartiallyFailed,
            _ => DagStatus::Unknown,
        }
    }

    /// 글꼴 누락과 컬러 이모지 대체를 피하려 기하·수학 기호로 상태를 구분한다.
    pub fn glyph(self) -> &'static str {
        match self {
            DagStatus::Waiting => "\u{25E6}",   // ◦ 흰 불릿
            DagStatus::Ready => "\u{25B7}",     // ▷ 흰 삼각(다음 차례)
            DagStatus::Running => "\u{25D1}",   // ◑ 반쯤 채운 원
            DagStatus::Succeeded => "\u{25CF}", // ● 채운 원
            DagStatus::Failed => "\u{00D7}",    // × 곱셈 기호
            DagStatus::Cancelled => "\u{2212}", // − 빼기 기호
            // 두 건너뜀 이유(미선택·선행 결과 없음)가 같은 글리프를 쓴다.
            DagStatus::Skipped => "\u{2298}", // ⊘ 사선 원
            DagStatus::Unknown => "?",
            DagStatus::PartiallyFailed => "\u{25D2}", // ◒ 아래 반을 채운 원
        }
    }

    /// 취소·스킵 상태의 카드는 흐리게 표시한다.
    pub fn is_dimmed(self) -> bool {
        matches!(self, DagStatus::Cancelled | DagStatus::Skipped)
    }

    /// 하류 경로를 흐리게 표시할 시작 상태. 실행 정책을 평가하는 판정은 아니다.
    pub fn kills_outgoing(self) -> bool {
        self.is_dimmed() || self == DagStatus::Failed
    }

    /// 화면에서 완료된 것으로 세는 상태.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            DagStatus::Succeeded
                | DagStatus::Failed
                | DagStatus::PartiallyFailed
                | DagStatus::Cancelled
                | DagStatus::Skipped
        )
    }
}

/// 상태 바 오른쪽 본문 영역의 모서리 곡률 — 오른쪽 두 개만 카드와 같게 깎는다.
fn body_corners(radius: LogicalPx) -> egui::CornerRadius {
    let r = radius.value().round().clamp(0.0, u8::MAX as f32) as u8;
    egui::CornerRadius {
        nw: 0,
        sw: 0,
        ne: r,
        se: r,
    }
}

/// 상태별 (바 색, 배지 배경, 라벨 색).
pub fn dag_status_colors(theme: &Theme, status: DagStatus) -> (HexColor, HexColor, HexColor) {
    match status {
        DagStatus::Waiting => (
            theme.dag_status_waiting(),
            theme.dag_status_waiting_bg(),
            theme.dag_status_waiting_label(),
        ),
        DagStatus::Ready => (
            theme.dag_status_ready(),
            theme.dag_status_ready_bg(),
            theme.dag_status_ready_label(),
        ),
        DagStatus::Running => (
            theme.dag_status_running(),
            theme.dag_status_running_bg(),
            theme.dag_status_running_label(),
        ),
        DagStatus::Succeeded => (
            theme.dag_status_succeeded(),
            theme.dag_status_succeeded_bg(),
            theme.dag_status_succeeded_label(),
        ),
        DagStatus::Failed => (
            theme.dag_status_failed(),
            theme.dag_status_failed_bg(),
            theme.dag_status_failed_label(),
        ),
        DagStatus::Cancelled => (
            theme.dag_status_cancelled(),
            theme.dag_status_cancelled_bg(),
            theme.dag_status_cancelled_label(),
        ),
        DagStatus::Skipped => (
            theme.dag_status_skipped(),
            theme.dag_status_skipped_bg(),
            theme.dag_status_skipped_label(),
        ),
        DagStatus::Unknown => (
            theme.dag_status_unknown(),
            theme.dag_status_unknown_bg(),
            theme.dag_status_unknown_label(),
        ),
        DagStatus::PartiallyFailed => (
            theme.dag_status_partially_failed(),
            theme.dag_status_partially_failed_bg(),
            theme.dag_status_partially_failed_label(),
        ),
    }
}

/// 노드 한 장의 (바 색, 배경, 라벨 색). 입력 대기는 상태 대신 needs-input 색 전체를 쓴다.
pub fn dag_card_colors(
    theme: &Theme,
    status: DagStatus,
    awaiting: bool,
) -> (HexColor, HexColor, HexColor) {
    if awaiting {
        (
            theme.dag_phase_awaiting(),
            theme.dag_phase_awaiting_bg(),
            theme.dag_phase_awaiting_label(),
        )
    } else {
        dag_status_colors(theme, status)
    }
}

/// dim 을 적용해 egui 색으로. `dimmed` 가 아니면 원색 그대로다.
/// 제목과 메타 글자색. 불투명 상태 틴트와 hover 채움 위에서는 띠가 달라진다.
fn node_ink(theme: &Theme, status: DagStatus, hovered: bool) -> (HexColor, HexColor) {
    if hovered {
        (theme.text_on_active(), theme.text_on_active())
    } else if matches!(status, DagStatus::Running | DagStatus::Failed) {
        (theme.text_on_active(), theme.dag_node_meta_status_fg())
    } else if matches!(status, DagStatus::Unknown | DagStatus::PartiallyFailed) {
        (theme.text_primary(), theme.dag_node_meta_status_fg())
    } else {
        (theme.dag_node_fg(), theme.dag_node_meta_fg())
    }
}

fn tone(theme: &Theme, color: HexColor, dimmed: bool) -> egui::Color32 {
    let c = color.to_egui();
    if dimmed {
        c.gamma_multiply(theme.node_dim_opacity())
    } else {
        c
    }
}

/// block 티어 채움 농도 — 시안 `color-mix(in srgb, <accent> 55%, surface-raised)`.
/// 색 혼합비는 토큰 생성기가 다루지 않아 대응 토큰이 없다.
const BLOCK_FILL_MIX: f32 = 0.55;

/// 대기·취소·스킵은 중립 테두리, 나머지는 상태색 테두리.
pub fn dag_status_border(theme: &Theme, status: DagStatus) -> HexColor {
    if matches!(
        status,
        DagStatus::Waiting | DagStatus::Cancelled | DagStatus::Skipped
    ) {
        theme.dag_node_border()
    } else {
        dag_status_colors(theme, status).0
    }
}

/// task 종류 5 종의 선두 아이콘. 종류는 캔버스에서 이 아이콘으로만 구분된다.
/// Claude·Codex agent 작업은 provider 와 관계없이 `AGENT` 하나를 쓴다.
pub fn dag_kind_icon(command_kind: &str) -> tasty_icons::Icon {
    match command_kind {
        "custom" => tasty_icons::PLUG,
        "agent" => tasty_icons::AGENT,
        "reduce" => tasty_icons::LAYERS,
        "wait_barrier" => tasty_icons::LOCK,
        _ => tasty_icons::TERMINAL,
    }
}

/// DAG 카드 한 장에 그릴 내용. 문구는 호출자가 고른다.
pub struct DagCardView<'a> {
    pub name: &'a str,
    pub status: DagStatus,
    /// agent 세션이 사람의 입력을 기다린다. 상태 대신 입력 대기 색을 쓴다.
    pub awaiting: bool,
    /// task 종류. [`dag_kind_icon`]으로 이름 앞 아이콘을 고른다.
    pub command_kind: &'a str,
    /// 상태 기호.
    pub glyph: &'a str,
    pub status_label: &'a str,
    pub duration: Option<&'a str>,
    /// 사이클을 이루는 노드.
    pub in_cycle: bool,
}

/// 화면 좌표의 카드에 내용을 그린다. Ui는 아이콘 텍스처에, painter는 나머지에 사용한다.
pub fn paint_dag_card(
    ui: &egui::Ui,
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    card: &DagCardView<'_>,
    vis: &NodeVisual,
) {
    let (accent, status_bg, label_fg) = dag_card_colors(theme, card.status, card.awaiting);
    let dim = vis.dimmed;
    let (lod, zoom) = (vis.lod, vis.zoom);
    let radius = theme.dag_node_radius() * zoom;
    // egui 모서리 값은 정수 px로 반올림해 넘긴다.
    let corner = radius.value().round();
    let stroke_w = theme.border_width.value();

    let border = if card.in_cycle {
        theme.dag_cycle_border()
    } else if lod == Lod::Block || card.awaiting {
        accent
    } else {
        dag_status_border(theme, card.status)
    };

    if lod == Lod::Block {
        let fill = theme
            .surface_raised()
            .to_egui()
            .lerp_to_gamma(accent.to_egui(), BLOCK_FILL_MIX);
        painter.rect_filled(
            rect,
            corner,
            if dim {
                fill.gamma_multiply(theme.node_dim_opacity())
            } else {
                fill
            },
        );
        painter.rect_stroke(
            rect,
            corner,
            egui::Stroke::new(stroke_w, tone(theme, border, dim)),
            egui::StrokeKind::Inside,
        );
        return;
    }

    painter.rect_filled(rect, corner, tone(theme, status_bg, dim));
    painter.rect_stroke(
        rect,
        corner,
        egui::Stroke::new(stroke_w, tone(theme, border, dim)),
        egui::StrokeKind::Inside,
    );

    let bar_w = (theme.dag_node_bar_width().value() * zoom).max(1.0);
    painter.rect_filled(
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + bar_w, rect.max.y)),
        corner,
        tone(theme, accent, dim),
    );

    // hover wash 는 바를 덮지 않는다 — 상태 바는 hover 중에도 원색이어야 한다.
    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x + bar_w, rect.min.y), rect.max);
    if vis.hovered {
        // hover 배경이 둥근 모서리 밖으로 나가지 않게 오른쪽 모서리를 맞춘다.
        // overlay 색은 premultiplied로 읽는다.
        painter.rect_filled(
            body,
            body_corners(radius),
            theme.dag_node_hover_bg().to_egui_premultiplied(),
        );
    }

    let pad_x = theme.dag_node_padding_x().value() * zoom;
    let pad_y = theme.dag_node_padding_y().value() * zoom;
    let inner = egui::Rect::from_min_max(
        egui::pos2(body.min.x + pad_x, body.min.y + pad_y),
        egui::pos2(body.max.x - pad_x, body.max.y - pad_y),
    );
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return;
    }

    let name_font = egui::FontId::proportional(theme.dag_node_name_font_size().value() * zoom);
    let meta_font = crate::label_font(theme.dag_node_meta_font_size().value() * zoom);
    let gap = theme.dag_node_gap().value() * zoom;
    let row_gap = theme.dag_node_row_gap().value() * zoom;
    let icon_side = theme.icon_glyph_size_sm.value() * zoom;
    let name_h = painter
        .ctx()
        .fonts(|f| f.row_height(&name_font))
        .max(icon_side);
    let meta_h = painter.ctx().fonts(|f| f.row_height(&meta_font));

    // compact 티어는 이름 행만 남는다 — 좌하단 LOD 칩이 "상태는 줌인" 이라고 알린다.
    let total = if lod == Lod::Full {
        name_h + row_gap + meta_h
    } else {
        name_h
    };
    let top = inner.center().y - total / 2.0;

    let (title_fg, meta_fg) = node_ink(theme, card.status, vis.hovered);
    dag_kind_icon(card.command_kind)
        .image(icon_side, tone(theme, meta_fg, dim))
        .paint_at(
            ui,
            egui::Rect::from_min_size(
                egui::pos2(inner.min.x, top + (name_h - icon_side) / 2.0),
                egui::vec2(icon_side, icon_side),
            ),
        );
    let name_x = inner.min.x + icon_side + gap;
    painter.text(
        egui::pos2(name_x, top + name_h / 2.0),
        egui::Align2::LEFT_CENTER,
        ellipsize(
            painter,
            card.name,
            &name_font,
            LogicalPx(inner.max.x - name_x),
        ),
        name_font,
        tone(theme, title_fg, dim),
    );

    if lod != Lod::Full {
        return;
    }

    let meta_y = top + name_h + row_gap + meta_h / 2.0;
    let glyph = card.glyph;
    let gw = text_width(painter, glyph, &meta_font).value() + gap;
    painter.text(
        egui::pos2(inner.min.x, meta_y),
        egui::Align2::LEFT_CENTER,
        glyph,
        meta_font.clone(),
        tone(theme, label_fg, dim),
    );

    // 시간 폭을 먼저 확보한다. 상태는 기호·색으로도 나타내므로 문구를 먼저 줄인다.
    let mut right = inner.max.x;
    if let Some(d) = card.duration {
        let duration_font = egui::FontId::monospace(meta_font.size);
        let dw = text_width(painter, d, &duration_font).value();
        if dw <= inner.width() - gw {
            painter.text(
                egui::pos2(right, meta_y),
                egui::Align2::RIGHT_CENTER,
                d,
                duration_font,
                tone(theme, meta_fg, dim),
            );
            right -= dw + gap;
        }
    }
    let label_w = (right - inner.min.x - gw).max(0.0);
    if label_w > 0.0 {
        painter.text(
            egui::pos2(inner.min.x + gw, meta_y),
            egui::Align2::LEFT_CENTER,
            ellipsize(painter, card.status_label, &meta_font, LogicalPx(label_w)),
            meta_font,
            tone(theme, label_fg, dim),
        );
    }
}
