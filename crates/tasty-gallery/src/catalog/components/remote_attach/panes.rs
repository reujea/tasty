//! Profile and workspace pane assembly, including connection states.

use super::new_row::new_ws_row;
use super::rows::{empty_line, profile_row, ws_row};
use super::{CAPS_HEADER_H, LEFT_W, NewRow, PROFILES, RaState, WORKSPACES, Ws};
use crate::catalog::icons;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, CenterState, ControlSize};

/// 시안 plan A 글리프 확대 비율(`transform: scale(1.4)`). 대응 토큰이 없다.
/// transform 은 배치에 영향이 없으므로 배치 높이는 원래 글리프 크기다.
const PLAN_A_GLYPH_SCALE: f32 = 1.4;
/// 시안 plan A 보조 줄의 줄 높이 비율(`lineHeight: 1.5`). 대응 토큰이 없다.
const PLAN_A_SUB_LINE_HEIGHT_RATIO: f32 = 1.5;

pub(super) fn left_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, state: RaState) {
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    col.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let hdr =
        egui::Rect::from_min_size(rect.min, egui::vec2(LEFT_W.value(), CAPS_HEADER_H.value()));
    let (_, _) = col.allocate_exact_size(
        egui::vec2(LEFT_W.value(), CAPS_HEADER_H.value()),
        egui::Sense::hover(),
    );
    col.painter().text(
        egui::pos2(
            hdr.left() + theme.spacing_md.value(),
            hdr.top() + theme.spacing_md.value(),
        ),
        egui::Align2::LEFT_TOP,
        "ATTACH PROFILES",
        tasty_ui_widgets::label_font(theme.font_size_micro.value()),
        theme.text_muted().to_egui(),
    );
    let sel_name = match state {
        RaState::Error => "legacy-attach",
        RaState::Connecting => "gb10",
        RaState::Empty | RaState::EmptyPlanA => "media-nas",
        _ => "prod-web",
    };
    for p in PROFILES {
        profile_row(&mut col, theme, p, p.name == sel_name);
    }
}

pub(super) fn right_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, state: RaState) {
    match state {
        // 빈 목록은 새 워크스페이스 행을 미리 선택해 생성할 수 있게 한다.
        RaState::Loaded => loaded_pane(
            ui,
            theme,
            rect,
            "prod-web",
            NewRow::Rest,
            WORKSPACES,
            Some("agents-prod"),
        ),
        RaState::Empty => loaded_pane(ui, theme, rect, "media-nas", NewRow::Selected, &[], None),
        // 채택하지 않은 비교안 — 가운데 상태에 생성 버튼을 단다.
        // 시안 `RemoteAttachFrame` 의 `center()` 를 그대로 옮긴다. 공용 CenterState 와 값이 다르다.
        RaState::EmptyPlanA => plan_a_center(ui, theme, rect),
        RaState::Initial => {
            CenterState::empty(
                icons::REMOTE,
                crate::i18n::t("remote_attach.select_profile"),
            )
            .sub_line(Some(crate::i18n::t("remote_attach.select_profile_hint")))
            .show_in(ui, theme, rect);
        }
        RaState::Connecting => {
            CenterState::loading("Connecting…")
                .sub_line(Some(
                    "Establishing the SSH tunnel to gb10 and listing workspaces. If the host does not \
                     respond, the lookup stops on its own after 20s.",
                ))
                .show_in(ui, theme, rect);
        }
        // 본체와 같은 원격 인스턴스 미실행 오류 예제. 내부 stderr·포트 경로는 표시하지 않는다.
        RaState::Error => {
            CenterState::error("Can't connect")
                .sub_line(Some(crate::i18n::t(
                    "ssh.port_discovery.instance_not_running",
                )))
                .action("Retry", Some(icons::REFRESH))
                .show_in(ui, theme, rect);
        }
    }
}

/// 목록이 비어도 헤더와 새 워크스페이스 행을 표시한다.
fn loaded_pane(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    profile: &str,
    new_row: NewRow,
    ws: &[Ws],
    sel_ws: Option<&str>,
) {
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    col.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let (hdr, _) = col.allocate_exact_size(
        egui::vec2(rect.width(), CAPS_HEADER_H.value()),
        egui::Sense::hover(),
    );
    let base_x = hdr.left() + theme.spacing_md.value();
    let y = hdr.top() + theme.spacing_md.value();
    let caps = col.painter().layout_no_wrap(
        "REMOTE WORKSPACES".to_owned(),
        tasty_ui_widgets::label_font(theme.font_size_micro.value()),
        theme.text_muted().to_egui(),
    );
    let caps_w = caps.rect.width();
    col.painter()
        .galley(egui::pos2(base_x, y), caps, theme.text_muted().to_egui());
    col.painter().text(
        egui::pos2(base_x + caps_w + theme.spacing_sm.value(), y),
        egui::Align2::LEFT_TOP,
        format!("· {profile}"),
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
    new_ws_row(&mut col, theme, new_row);
    if ws.is_empty() {
        empty_line(&mut col, theme, profile);
    } else {
        for w in ws {
            ws_row(&mut col, theme, w, sel_ws == Some(w.name), false);
        }
    }
}

/// 시안 plan A 가운데 블록 — 글리프 · 제목 · 보조 줄 · 생성 버튼을 한 묶음으로 세로 가운데에
/// 둔다. 안쪽 여백은 `space-xl` / `space-lg`, 항목 사이는 `space-sm`, 버튼 위는 `space-xs` 를 더한다.
fn plan_a_center(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let inner = rect.shrink2(egui::vec2(
        theme.spacing_lg.value(),
        theme.spacing_xl.value(),
    ));
    let gap = theme.spacing_sm;
    let muted = theme.text_muted().to_egui();
    let title = {
        let mut job = egui::text::LayoutJob::simple(
            "No workspaces on this remote yet".to_owned(),
            egui::FontId::proportional(theme.font_size_body.value()),
            muted,
            inner.width(),
        );
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    };
    let sub = {
        let size = theme.font_size_caption.value();
        let fmt = |font: egui::FontId| egui::TextFormat {
            font_id: font,
            color: muted,
            line_height: Some(size * PLAN_A_SUB_LINE_HEIGHT_RATIO),
            ..Default::default()
        };
        let mut job = egui::text::LayoutJob::default();
        job.append("media-nas", 0.0, fmt(egui::FontId::monospace(size)));
        job.append(
            " is reachable. Create one there and mirror it here.",
            0.0,
            fmt(egui::FontId::proportional(size)),
        );
        job.wrap.max_width = theme.center_state_max_width().value().min(inner.width());
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    };
    let glyph_box = theme.icon_glyph_size_md;
    let button_h = LogicalPx(ControlSize::Sm.height(theme));
    let block_h = glyph_box
        + gap
        + LogicalPx(title.rect.height())
        + gap
        + LogicalPx(sub.rect.height())
        + gap
        + theme.spacing_xs
        + button_h;
    let cx = inner.center().x;
    let mut y = inner.center().y - (block_h * 0.5).value();

    let painted = glyph_box.value() * PLAN_A_GLYPH_SCALE;
    let glyph_rect = egui::Rect::from_center_size(
        egui::pos2(cx, y + glyph_box.value() * 0.5),
        egui::vec2(painted, painted),
    );
    icons::PANE_EMPTY
        .image(painted, theme.text_placeholder().to_egui())
        .paint_at(ui, glyph_rect);
    y += (glyph_box + gap).value();
    let title_h = title.rect.height();
    // 가운데 정렬 job 은 원점이 줄의 가운데이므로 블록 가운데에 둔다.
    ui.painter().galley(egui::pos2(cx, y), title, muted);
    y += title_h + gap.value();
    let sub_h = sub.rect.height();
    ui.painter().galley(egui::pos2(cx, y), sub, muted);
    y += sub_h + (gap + theme.spacing_xs).value();

    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                egui::pos2(inner.left(), y),
                egui::pos2(inner.right(), y + button_h.value()),
            ))
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    let plus = |ui: &mut egui::Ui, r: egui::Rect, c: egui::Color32| {
        icons::PLUS.image(r.height(), c).paint_at(ui, r);
    };
    Button::new("New workspace")
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Sm)
        .leading_icon(&plus)
        .show(&mut col, theme);
}
