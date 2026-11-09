//! 배율별로 표시 내용을 줄이는 DAG 카드. 레이아웃 크기는 유지한다.
//! 기본 표시에서는 상태색·기호·문구를 함께 쓰고, 가장 작게 보일 때는 색만 남긴다.
//! 이름 앞 아이콘은 상태가 아니라 task 종류를 나타낸다.

use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

use super::model::{DagNodeData, DagStatus, node_duration};
use super::view::Lod;
use crate::adapters::ui::icons;

/// 카드 한 장의 그리기 상태.
pub struct NodeVisual {
    /// 이 배율에서 카드 안에 무엇을 넣을지.
    pub lod: Lod,
    /// 화면 배율 — 폰트/패딩만 따라 커진다. 박스 크기는 이것과 무관하게 고정이다.
    pub zoom: f32,
    /// duration 계산 기준 시각(epoch ms).
    pub now_ms: u64,
    pub selected: bool,
    pub hovered: bool,
    /// 실패·취소·스킵 노드의 하류라 흐리게 표시할지.
    pub dimmed: bool,
    /// 사이클을 이루는 노드.
    pub in_cycle: bool,
}

/// 상태 바 오른쪽 본문 영역의 모서리 곡률 — 오른쪽 두 개만 카드와 같게 깎는다.
fn body_corners(radius: f32) -> egui::CornerRadius {
    let r = radius.round().clamp(0.0, u8::MAX as f32) as u8;
    egui::CornerRadius {
        nw: 0,
        sw: 0,
        ne: r,
        se: r,
    }
}

/// 상태별 (바 색, 배지 배경, 라벨 색).
pub fn status_colors(theme: &Theme, status: DagStatus) -> (HexColor, HexColor, HexColor) {
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
pub fn node_colors(theme: &Theme, node: &DagNodeData) -> (HexColor, HexColor, HexColor) {
    if node.awaiting().is_some() {
        (
            theme.dag_phase_awaiting(),
            theme.dag_phase_awaiting_bg(),
            theme.dag_phase_awaiting_label(),
        )
    } else {
        status_colors(theme, node.status)
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
/// 색 혼합비는 토큰 생성기가 다루지 않아 대응 토큰이 없다(갤러리 specimen 도 같은
/// 값을 직접 쓴다).
const BLOCK_FILL_MIX: f32 = 0.55;

/// 대기·취소·스킵은 중립 테두리, 나머지는 상태색 테두리.
pub fn status_border(theme: &Theme, status: DagStatus) -> HexColor {
    if matches!(
        status,
        DagStatus::Waiting | DagStatus::Cancelled | DagStatus::Skipped
    ) {
        theme.dag_node_border()
    } else {
        status_colors(theme, status).0
    }
}

/// task 종류 5 종의 선두 아이콘. 종류는 캔버스에서 이 아이콘으로만 구분된다.
/// Claude·Codex agent 작업은 provider 와 관계없이 `AGENT` 하나를 쓴다.
pub fn kind_icon(command_kind: &str) -> icons::Icon {
    match command_kind {
        "custom" => icons::PLUG,
        "agent" => icons::AGENT,
        "reduce" => icons::LAYERS,
        "wait_barrier" => icons::LOCK,
        _ => icons::TERMINAL,
    }
}

/// 화면 좌표의 카드에 내용을 그린다. Ui는 아이콘 텍스처에, painter는 나머지에 사용한다.
pub fn paint_node(
    ui: &egui::Ui,
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    node: &DagNodeData,
    vis: &NodeVisual,
) {
    let (accent, status_bg, label_fg) = node_colors(theme, node);
    let dim = vis.dimmed;
    let (lod, zoom, now_ms) = (vis.lod, vis.zoom, vis.now_ms);
    let radius = (theme.dag_node_radius().value() * zoom).round();
    let stroke_w = theme.border_width.value();

    let border = if vis.in_cycle {
        theme.dag_cycle_border()
    } else if lod == Lod::Block || node.awaiting().is_some() {
        accent
    } else {
        status_border(theme, node.status)
    };

    if lod == Lod::Block {
        let fill = theme
            .surface_raised()
            .to_egui()
            .lerp_to_gamma(accent.to_egui(), BLOCK_FILL_MIX);
        painter.rect_filled(
            rect,
            radius,
            if dim {
                fill.gamma_multiply(theme.node_dim_opacity())
            } else {
                fill
            },
        );
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(stroke_w, tone(theme, border, dim)),
            egui::StrokeKind::Inside,
        );
        return;
    }

    painter.rect_filled(rect, radius, tone(theme, status_bg, dim));
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(stroke_w, tone(theme, border, dim)),
        egui::StrokeKind::Inside,
    );

    let bar_w = (theme.dag_node_bar_width().value() * zoom).max(1.0);
    painter.rect_filled(
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + bar_w, rect.max.y)),
        radius,
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
    let meta_font = tasty_ui_widgets::label_font(theme.dag_node_meta_font_size().value() * zoom);
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

    let (title_fg, meta_fg) = node_ink(theme, node.status, vis.hovered);
    kind_icon(node.command_kind)
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
        ellipsize(painter, &node.name, &name_font, inner.max.x - name_x),
        name_font,
        tone(theme, title_fg, dim),
    );

    if lod != Lod::Full {
        return;
    }

    let meta_y = top + name_h + row_gap + meta_h / 2.0;
    let glyph = node.glyph();
    let gw = text_width(painter, glyph, &meta_font) + gap;
    painter.text(
        egui::pos2(inner.min.x, meta_y),
        egui::Align2::LEFT_CENTER,
        glyph,
        meta_font.clone(),
        tone(theme, label_fg, dim),
    );

    // 시간 폭을 먼저 확보한다. 상태는 기호·색으로도 나타내므로 문구를 먼저 줄인다.
    let duration = node_duration(node, now_ms);
    let mut right = inner.max.x;
    if let Some(d) = &duration {
        let duration_font = egui::FontId::monospace(meta_font.size);
        let dw = text_width(painter, d, &duration_font);
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
            ellipsize(painter, &node.status_label(), &meta_font, label_w),
            meta_font,
            tone(theme, label_fg, dim),
        );
    }
}

/// 선택 링은 카드 밖에 간격을 두어 상태 테두리와 구분한다.
pub fn paint_selection_ring(painter: &egui::Painter, theme: &Theme, rect: egui::Rect, zoom: f32) {
    let w = (theme.dag_node_selected_ring_width().value() * zoom).max(1.0);
    let offset = theme.border_width.value() * zoom;
    painter.rect_stroke(
        rect.expand(offset + w / 2.0),
        (theme.dag_node_radius().value() * zoom + offset + w).round(),
        egui::Stroke::new(w, theme.dag_node_selected_ring().to_egui()),
        egui::StrokeKind::Middle,
    );
}

fn text_width(painter: &egui::Painter, text: &str, font: &egui::FontId) -> f32 {
    painter.ctx().fonts(|f| {
        f.layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    })
}

/// 글자 경계를 지키며 폭에 맞춰 줄인다.
fn ellipsize(painter: &egui::Painter, text: &str, font: &egui::FontId, max_w: f32) -> String {
    if max_w <= 0.0 {
        return String::new();
    }
    if text_width(painter, text, font) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = lo.midpoint(hi + 1).min(chars.len());
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if text_width(painter, &cand, font) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
        if mid == lo && lo == hi {
            break;
        }
    }
    if lo == 0 {
        return "…".to_string();
    }
    chars[..lo].iter().collect::<String>() + "…"
}
