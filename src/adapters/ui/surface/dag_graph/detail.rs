//! 선택 노드의 상세 내용. 위치·구분선·바탕 틀은 공통 그래프 뷰의 상세 자리가 정한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, TagVariant, drawer_body, drawer_mono_row,
    drawer_row as row, drawer_section as section, drawer_text_block, margin_all, tag, vspace,
};

use super::model::{
    DagGraphData, DagNodeData, DagStatus, format_clock, format_duration_ms, kind_label,
    node_duration,
};
use super::node::{node_colors, status_colors};
use crate::adapters::ui::icons;
use crate::i18n::{t, t_fmt, t_fmt2};

/// 상세 패널에서 나온 사용자 조작.
pub enum DetailAction {
    /// 의존성 행을 눌러 그 노드로 선택을 옮긴다.
    Select(String),
    /// 닫기 — 선택을 풀어 패널 자체를 접는다.
    Close,
    /// 입력을 기다리는 agent 세션의 surface 를 사용자 앞으로 가져온다. 사용자 조작 전용이다.
    OpenSession(u32),
}

/// 상세 콘텐츠를 `ui` 안에 채운다.
pub fn draw_detail(
    ui: &mut egui::Ui,
    theme: &Theme,
    graph: &DagGraphData,
    node: &DagNodeData,
    now_ms: u64,
) -> Option<DetailAction> {
    let mut action = None;

    drawer_body(ui, theme, |ui| {
        if header(ui, theme, node) {
            action = Some(DetailAction::Close);
        }
        vspace(ui, theme.spacing_sm);
        if let Some(surface) = awaiting_notice(ui, theme, node, now_ms) {
            action = Some(DetailAction::OpenSession(surface));
        }
        unknown_reason(ui, theme, node);

        row(
            ui,
            theme,
            t("dag.detail.on_failure"),
            &on_failure_label(node.on_failure_kind),
        );
        if let Some(started) = node.started_at {
            row(ui, theme, t("dag.detail.started"), &format_clock(started));
        }
        if let Some(d) = node_duration(node, now_ms) {
            row(ui, theme, t("dag.detail.duration"), &d);
        }
        if let Some(code) = node.exit_code {
            row(ui, theme, t("dag.detail.exit_code"), &code.to_string());
        }
        drawer_mono_row(ui, theme, t("dag.detail.task_id"), &node.id);

        vspace(ui, theme.spacing_sm);
        drawer_text_block(
            ui,
            theme,
            t("dag.detail.command"),
            &node.command_text,
            theme.dag_detail_out_max_height(),
            None,
        );

        if !node.incoming.is_empty() {
            vspace(ui, theme.spacing_sm);
            section(
                ui,
                theme,
                &format!(
                    "{label} \u{b7} {count}",
                    label = t("dag.detail.dependencies"),
                    count = node.incoming.len()
                ),
            );
            for (idx, rel) in &node.incoming {
                let Some(dep) = graph.nodes.get(*idx) else {
                    continue;
                };
                if dependency_row(ui, theme, dep, rel.label()) {
                    action = Some(DetailAction::Select(dep.id.clone()));
                }
            }
        }

        if let Some(err) = &node.error_tail {
            vspace(ui, theme.spacing_sm);
            drawer_text_block(
                ui,
                theme,
                t("dag.detail.error"),
                err,
                theme.dag_detail_log_max_height(),
                Some(t("dag.detail.copy")),
            );
        }
        if let Some(out) = &node.output_tail {
            vspace(ui, theme.spacing_sm);
            drawer_text_block(
                ui,
                theme,
                t("dag.detail.output"),
                out,
                theme.dag_detail_out_max_height(),
                Some(t("dag.detail.copy")),
            );
        }
    });

    action
}

/// 이름·상태·종류·ID와 닫기 버튼.
fn header(ui: &mut egui::Ui, theme: &Theme, node: &DagNodeData) -> bool {
    let (bar, _, label_fg) = node_colors(theme, node);
    let mut close = false;
    ui.horizontal_top(|ui| {
        let btn = theme.dag_chrome_height().value();
        ui.allocate_ui_with_layout(
            egui::vec2(
                (ui.available_width() - btn - theme.spacing_sm.value()).max(0.0),
                btn,
            ),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.label(
                    egui::RichText::new(&node.name)
                        .size(theme.font_size_body.value())
                        .color(theme.text_primary().to_egui()),
                );
            },
        );
        close = IconButton::new()
            .size(ControlSize::Sm)
            .show(ui, theme, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
            })
            .on_hover_text(t("dag.detail.close"))
            .clicked();
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(node.glyph())
                .monospace()
                .size(theme.font_size_caption.value())
                .color(bar.to_egui()),
        );
        ui.label(
            egui::RichText::new(node.status_label())
                .size(theme.font_size_caption.value())
                .color(label_fg.to_egui()),
        );
        tag(
            ui,
            theme,
            kind_label(node.command_kind),
            TagVariant::Default,
            false,
        );
    });
    close
}

/// 입력 대기 알림 — 누가 무엇을 기다리는지와 그 세션을 여는 버튼. 눌리면 세션 surface id.
fn awaiting_notice(
    ui: &mut egui::Ui,
    theme: &Theme,
    node: &DagNodeData,
    now_ms: u64,
) -> Option<u32> {
    let (provider, surface, since) = node.awaiting()?;
    let mut open = false;
    egui::Frame::NONE
        .fill(theme.dag_phase_awaiting_bg().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.dag_phase_awaiting().to_egui(),
        ))
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(margin_all(theme.spacing_sm))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.add(
                egui::Label::new(
                    egui::RichText::new(t_fmt2(
                        "dag.detail.awaiting_notice",
                        provider,
                        &format_duration_ms(now_ms.saturating_sub(since)),
                    ))
                    .size(theme.font_size_caption.value())
                    .color(theme.text_primary().to_egui()),
                )
                .wrap(),
            );
            open = Button::new(t("dag.detail.open_session"))
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .show(ui, theme)
                .clicked();
        });
    vspace(ui, theme.spacing_sm);
    open.then_some(surface)
}

/// 알 수 없음의 이유와 그래프를 잇는 방법.
fn unknown_reason(ui: &mut egui::Ui, theme: &Theme, node: &DagNodeData) {
    if node.status != DagStatus::Unknown {
        return;
    }
    let Some(reason) = &node.unknown_reason else {
        return;
    };
    section(ui, theme, t("dag.detail.why_unknown"));
    ui.add(
        egui::Label::new(
            egui::RichText::new(reason)
                .size(theme.font_size_caption.value())
                .color(theme.text_secondary().to_egui()),
        )
        .wrap(),
    );
    ui.add(
        egui::Label::new(
            egui::RichText::new(t("dag.why.retry_or_cancel"))
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        )
        .wrap(),
    );
    vspace(ui, theme.spacing_sm);
}

/// 의존성 한 줄. 눌리면 `true`.
fn dependency_row(ui: &mut egui::Ui, theme: &Theme, dep: &DagNodeData, rel: &str) -> bool {
    let (bar, _, _) = status_colors(theme, dep.status);
    let resp = ui
        .horizontal(|ui| {
            let label = format!("{}  {}", dep.status.glyph(), dep.name);
            let r = ui.add(
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(theme.font_size_caption.value())
                        .color(bar.to_egui()),
                )
                .sense(egui::Sense::click()),
            );
            ui.label(
                egui::RichText::new(rel)
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted().to_egui()),
            );
            r
        })
        .inner;
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(t_fmt("dag.detail.jump_hint", &dep.name))
        .clicked()
}

fn on_failure_label(kind: &str) -> String {
    match kind {
        "continue_downstream" => t("dag.on_failure.continue_downstream").to_string(),
        "fallback" => t("dag.on_failure.fallback").to_string(),
        _ => t("dag.on_failure.abort").to_string(),
    }
}
