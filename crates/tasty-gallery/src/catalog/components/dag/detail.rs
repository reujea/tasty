//! 넓은 화면은 오른쪽 패널, 좁은 화면은 하단 영역에 노드 상세를 표시한다.
//! 긴 오류·출력 로그는 높이를 제한한 영역 안에서 스크롤한다.

use tasty_design_tokens::generated::semantic::CONTROL_HEIGHT_TREE;
use tasty_icons as icons;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, IconButton, TagVariant, tag};

use super::{Graph, Rel, Status};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 상세가 붙는 자리.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dock {
    /// 우측 288px 패널 — 왼쪽에 세로 구분선.
    Side,
    /// 하단 220px 시트 — 윗변에 가로 구분선.
    Sheet,
}

fn caption(theme: &Theme) -> f32 {
    theme.font_size_caption.value()
}

/// 상세 캡션은 번역 문구의 대소문자를 그대로 쓰고 caption 크기로 그린다.
fn block_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(caption(theme))
            .color(theme.text_muted().to_egui()),
    );
}

fn code_block(ui: &mut egui::Ui, theme: &Theme, id: (&str, &str), text: &str, max_h: Option<f32>) {
    egui::Frame::new()
        .fill(theme.dag_detail_log_bg().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius_sm.value())
        .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let body = |ui: &mut egui::Ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(caption(theme))
                        .monospace()
                        .color(theme.text_secondary().to_egui()),
                );
            };
            match max_h {
                Some(h) => {
                    egui::ScrollArea::vertical()
                        .id_salt(id)
                        .max_height(h)
                        .drag_to_scroll(false)
                        .show(ui, body);
                }
                None => body(ui),
            }
        });
}

/// 라벨 + 복사 버튼 + 스크롤 블록 (`LogBlock`).
fn log_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: (&str, &str),
    label: &str,
    text: &str,
    max_h: f32,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        block_label(ui, theme, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            IconButton::new()
                .size(ControlSize::Sm)
                .show(ui, theme, &|ui, rect, c| {
                    icons::COPY.image(rect.height(), c).paint_at(ui, rect);
                });
        });
    });
    ui.add_space(theme.spacing_xs.value());
    code_block(ui, theme, id, text, Some(max_h));
}

/// 의존 한 줄 — 글리프 + 이름 + 관계 라벨. 클릭하면 그 task 로 선택이 옮겨간다.
fn dependency_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    status: Status,
    name: &str,
    rel: Rel,
) -> egui::Response {
    let h = CONTROL_HEIGHT_TREE.value();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.listctrl_row_bg_hover().to_egui_premultiplied(),
        );
    }
    let pad = theme.spacing_xs.value();
    let gap = theme.spacing_sm.value();
    let mono = egui::FontId::monospace(caption(theme));
    let glyph_w = theme.spacing_md.value();
    ui.painter().text(
        egui::pos2(rect.min.x + pad, rect.center().y),
        egui::Align2::LEFT_CENTER,
        status.glyph(),
        mono.clone(),
        status.label_fg(theme).to_egui(),
    );
    let rel_font = tasty_ui_widgets::label_font(theme.font_size_micro.value());
    let rel_w = super::node::text_width(ui, rel.label(), &rel_font);
    ui.painter().text(
        egui::pos2(rect.max.x - pad, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        rel.label(),
        rel_font,
        theme.text_muted().to_egui(),
    );
    let name_font = egui::FontId::proportional(caption(theme));
    let name_x = rect.min.x + pad + glyph_w + gap;
    let avail = rect.max.x - pad - rel_w - gap - name_x;
    ui.painter().text(
        egui::pos2(name_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        super::node::ellipsize(ui, name, &name_font, avail),
        name_font,
        theme.text_secondary().to_egui(),
    );
    resp
}

/// 상세 패널 본문. 반환값은 "여기로 점프" 로 고른 task id.
pub fn draw_body(ui: &mut egui::Ui, theme: &Theme, graph: &Graph, id: &str) -> Option<String> {
    let node = graph.node(id)?;
    let mut jump = None;
    // 상위 예제의 0 간격을 그대로 쓰면 라벨이 붙으므로 본문 간격을 다시 지정한다.
    ui.spacing_mut().item_spacing = egui::vec2(theme.spacing_sm.value(), theme.spacing_md.value());

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
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
                        .strong()
                        .color(theme.text_primary().to_egui()),
                );
            },
        );
        IconButton::new()
            .size(ControlSize::Sm)
            .show(ui, theme, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect);
            });
    });

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(format!("{} {}", node.glyph(), node.status_label()))
                .size(caption(theme))
                .family(tasty_ui_widgets::label_font_family())
                .color(node.label_fg(theme).to_egui()),
        );
        tag(ui, theme, node.kind.label(), TagVariant::Default, false);
    });

    // 입력 대기 — 누가 무엇을 기다리는지와 그 세션을 여는 버튼. 사용자 조작 전용이다.
    if let Some(notice) = node.awaiting_notice() {
        egui::Frame::NONE
            .fill(theme.dag_phase_awaiting_bg().to_egui())
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                theme.dag_phase_awaiting().to_egui(),
            ))
            .corner_radius(theme.corner_radius_sm.value())
            .inner_margin(theme.spacing_sm.value())
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(notice)
                            .size(caption(theme))
                            .color(theme.text_primary().to_egui()),
                    )
                    .wrap(),
                );
                Button::new("Open session")
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
            });
    }

    if let Some((reason, retry)) = super::phase::unknown_lines(node) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
            block_label(ui, theme, "Why unknown");
            ui.add(
                egui::Label::new(
                    egui::RichText::new(reason)
                        .size(caption(theme))
                        .color(theme.text_secondary().to_egui()),
                )
                .wrap(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(retry)
                        .size(caption(theme))
                        .color(theme.text_muted().to_egui()),
                )
                .wrap(),
            );
        });
    }

    egui::Grid::new(("dag_detail_dl", id))
        .num_columns(2)
        .spacing(egui::vec2(
            theme.spacing_md.value(),
            theme.spacing_xs.value(),
        ))
        .show(ui, |ui| {
            let rows = [
                (
                    "Started",
                    node.started.clone().unwrap_or_else(|| "—".into()),
                ),
                ("Duration", node.dur.clone().unwrap_or_else(|| "—".into())),
                (
                    "Exit code",
                    node.exit
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "—".into()),
                ),
                ("Task id", node.id.clone()),
            ];
            for (k, v) in rows {
                ui.label(
                    egui::RichText::new(k)
                        .size(caption(theme))
                        .color(theme.text_muted().to_egui()),
                );
                ui.label(
                    egui::RichText::new(v)
                        .size(caption(theme))
                        .monospace()
                        .color(theme.text_secondary().to_egui()),
                );
                ui.end_row();
            }
        });

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        block_label(ui, theme, "Command");
        code_block(
            ui,
            theme,
            ("dag_detail_cmd", node.id.as_str()),
            &node.cmd,
            None,
        );
    });

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        if node.deps.is_empty() {
            block_label(ui, theme, t("dag.detail.dependencies"));
            ui.label(
                egui::RichText::new("root task — no dependencies")
                    .size(caption(theme))
                    .color(theme.text_placeholder().to_egui()),
            );
        } else {
            block_label(
                ui,
                theme,
                &format!(
                    "{label} \u{b7} {count}",
                    label = t("dag.detail.dependencies"),
                    count = node.deps.len()
                ),
            );
            for (from, rel) in &node.deps {
                if let Some(up) = graph.node(from)
                    && dependency_row(ui, theme, up.status, &up.name, *rel).clicked()
                {
                    jump = Some(up.id.clone());
                }
            }
        }
    });

    if let Some(err) = &node.err {
        ui.vertical(|ui| {
            log_block(
                ui,
                theme,
                ("dag_detail_err", node.id.as_str()),
                "Error",
                err,
                theme.dag_detail_log_max_height().value(),
            );
        });
    }
    if !matches!(node.status, Status::Waiting | Status::Ready) {
        let tail = format!(
            "$ {}\n… {}",
            node.cmd,
            if node.status == Status::Running {
                "streaming"
            } else {
                "last 200 lines retained"
            }
        );
        ui.vertical(|ui| {
            log_block(
                ui,
                theme,
                ("dag_detail_out", node.id.as_str()),
                "Output tail",
                &tail,
                theme.dag_detail_out_max_height().value(),
            );
        });
    }
    jump
}

/// 상세를 도킹 프레임에 넣어 그린다. 구분선은 도킹 변에만 붙는다.
pub fn draw_docked(
    ui: &mut egui::Ui,
    theme: &Theme,
    graph: &Graph,
    id: &str,
    dock: Dock,
    size: egui::Vec2,
) -> Option<String> {
    let mut jump = None;
    let stroke = egui::Stroke::new(
        theme.border_width.value(),
        theme.dag_detail_border().to_egui(),
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.dag_detail_bg().to_egui());
    match dock {
        Dock::Side => {
            ui.painter().vline(rect.min.x, rect.y_range(), stroke);
        }
        Dock::Sheet => {
            ui.painter().hline(rect.x_range(), rect.min.y, stroke);
        }
    }
    let pad = theme.dag_detail_padding().value();
    let inner = rect.shrink(pad);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect);
    egui::ScrollArea::vertical()
        .id_salt(("dag_detail_scroll", id))
        .drag_to_scroll(false)
        .show(&mut child, |ui| {
            ui.set_width(inner.width());
            jump = draw_body(ui, theme, graph, id);
        });
    jump
}

/// `detail` 섹션 Spec — 실패 노드와 실행 중 노드를 나란히.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let graph = super::build_dag();
    let h = theme.dag_popup_height().value();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for id in ["build_linux", "unit"] {
                draw_docked(
                    ui,
                    theme,
                    &graph,
                    id,
                    Dock::Side,
                    egui::vec2(theme.dag_detail_width().value(), h),
                );
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("panel", "288 wide · 12 padding"),
            ("sheet", "220 tall (narrow)"),
            ("error tail", "max 160, scrolls, copy button"),
            ("output tail", "max 112, scrolls"),
            ("deps", "22px rows, click to jump"),
        ],
        &[
            TokenChip::new(
                "dag-detail-bg",
                "panel fill",
                theme.dag_detail_bg().to_egui(),
            ),
            TokenChip::new(
                "dag-detail-log-bg",
                "log bed",
                theme.dag_detail_log_bg().to_egui(),
            ),
            TokenChip::without_color("dag-detail-log-max-height", "160 error tail"),
            TokenChip::new(
                "dag-detail-border",
                "dock hairline",
                theme.dag_detail_border().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Dependency rows are the graph's keyboard-free navigation: each row shows the upstream \
         task's glyph + relation and jumps the selection there, so a failure can be walked back \
         to its cause without hunting on the canvas.",
    );
}
