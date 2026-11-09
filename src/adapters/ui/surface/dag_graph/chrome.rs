//! DAG 헤더·실행 상태·이동 및 배율 버튼·미니맵·안내 표시.
//! 폭이 좁으면 줌 숫자·미니맵부터 숨기고 러너 상태와 순환 관계 경고는 남긴다.

use tasty_dag_layout::GraphLayout;
use tasty_model::DagDirection;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{ControlSize, IconButton, IconButtonVariant, hspace, margin_sym};

use super::model::{DagData, DagGraphData, RunnerBadgeData};
use super::view::{DagGraphView, Lod, ZOOM_MAX, ZOOM_MIN};
use crate::adapters::ui::icons;
use crate::i18n::{t, t_count, t_fmt, t_fmt2};

/// 헤더·크롬에서 나온 조작.
pub enum ChromeAction {
    SelectDag(String),
    ToggleDirection,
    Fit,
    /// 줌 단계 변경. 앵커는 **캔버스 중심**이어야 하므로 여기서 적용하지 않고
    /// 캔버스 크기를 아는 호출자에게 넘긴다.
    Zoom(f32),
    /// 폴링 주기를 기다리지 않고 지금 다시 읽는다.
    Refresh,
}

/// 좁으면 DAG 이름·진행률과 러너 상태·새로고침을 두 줄로 나눈다.
pub fn draw_header(
    ui: &mut egui::Ui,
    theme: &Theme,
    data: &DagData,
    surface_width: f32,
) -> Option<ChromeAction> {
    let mut action = None;
    let stacked = surface_width < NARROW_DETAIL_SHEET.value();
    let row_h = theme.dag_chrome_height().value();

    // 헤더는 사이드바 계열 토큰을 사용한다. 줌 버튼 토큰과 현재 값이 같아도 역할은 다르다.
    egui::Frame::NONE
        .fill(theme.bg_sidebar().to_egui())
        .inner_margin(margin_sym(theme.spacing_sm, theme.spacing_xs))
        .show(ui, |ui| {
            ui.set_height(if stacked { row_h * 2.0 } else { row_h });
            if stacked {
                ui.vertical(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| identity_group(ui, theme, data, &mut action),
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            // 좁은 헤더에서는 잘린 명령이 보이지 않도록 재개 안내를 숨긴다.
                            runner_badge(ui, theme, &data.runner);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if refresh_button(ui, theme) {
                                        action = Some(ChromeAction::Refresh);
                                    }
                                },
                            );
                        },
                    );
                });
            } else {
                ui.horizontal_centered(|ui| {
                    identity_group(ui, theme, data, &mut action);
                    // 오른쪽부터 배치하므로 읽는 순서의 역순으로 넣는다.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for item in header_right_paint_order() {
                            match item {
                                HeaderRightItem::Refresh => {
                                    if refresh_button(ui, theme) {
                                        action = Some(ChromeAction::Refresh);
                                    }
                                }
                                HeaderRightItem::ResumeHint => resume_hint(ui, theme, &data.runner),
                                HeaderRightItem::Pill => runner_badge(ui, theme, &data.runner),
                            }
                        }
                    });
                });
            }
        });

    // separator는 이미 알파가 곱해진 색이므로 premultiplied로 읽는다.
    let (sep, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        sep.x_range(),
        sep.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    action
}

/// 헤더 좌측 그룹 — 어떤 DAG 를, 얼마나.
fn identity_group(
    ui: &mut egui::Ui,
    theme: &Theme,
    data: &DagData,
    action: &mut Option<ChromeAction>,
) {
    ui.add(icons::GIT_TREE.image(
        theme.icon_glyph_size_sm.value(),
        theme.dag_chrome_fg().to_egui(),
    ));
    hspace(ui, theme.spacing_xs);
    if let Some(picked) = dag_picker(ui, theme, data) {
        *action = Some(ChromeAction::SelectDag(picked));
    }
    if let Some(graph) = &data.current {
        hspace(ui, theme.spacing_sm);
        let (skipped, not_selected) = graph.skip_counts();
        ui.label(
            egui::RichText::new(format!(
                "{}{}",
                t_fmt2(
                    "dag.header.progress",
                    &graph.done.to_string(),
                    &graph.total().to_string(),
                ),
                super::model::skip_count_suffix(skipped, not_selected)
            ))
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.font_size_caption.value())
            .color(theme.dag_header_count_fg().to_egui()),
        );
    }
}

/// 폴링을 기다리지 않고 지금 다시 읽는다. 눌렸으면 `true`.
fn refresh_button(ui: &mut egui::Ui, theme: &Theme) -> bool {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(ui, theme, &|ui, rect, c| {
            icons::REFRESH.image(rect.height(), c).paint_at(ui, rect)
        })
        .on_hover_text(t("dag.header.refresh"))
        .clicked()
}

/// DAG 선택 드롭다운. 목록이 하나뿐이면 그냥 이름만 보인다.
fn dag_picker(ui: &mut egui::Ui, theme: &Theme, data: &DagData) -> Option<String> {
    let current_name = data
        .current
        .as_ref()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| t("dag.header.no_dag").to_string());

    if data.dags.len() <= 1 {
        ui.label(
            egui::RichText::new(current_name)
                .size(theme.font_size_body.value())
                .color(theme.dag_chrome_fg().to_egui()),
        );
        return None;
    }

    let mut picked = None;
    tasty_egui_theme::with_popover_frame(ui, theme, |ui| {
        egui::ComboBox::from_id_salt("dag_picker")
            .selected_text(current_name)
            .show_ui(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for entry in &data.dags {
                    let selected = data.current.as_ref().is_some_and(|g| g.id == entry.id);
                    let label = t_count(
                        "dag.header.entry",
                        entry.task_count as u64,
                        &[&entry.name, &entry.task_count.to_string()],
                    );
                    let text = format!("{} {}", entry.rollup.glyph(), label);
                    if tasty_ui_widgets::menu_option(ui, theme, &text, selected).clicked() {
                        picked = Some(entry.id.clone());
                    }
                }
            })
    });
    picked
}

/// 점과 문구로 러너 상태를 함께 표시한다. 재개 안내는 별도로 그린다.
fn runner_badge(ui: &mut egui::Ui, theme: &Theme, runner: &RunnerBadgeData) {
    let stalled = runner.is_stalled();
    let (bg, border, fg, dot, text) = if runner.crashed {
        (
            theme.dag_runner_crashed_bg(),
            theme.dag_runner_crashed_border(),
            theme.dag_runner_crashed_fg(),
            theme.status_dot_danger(),
            t_fmt("dag.runner.crashed", &runner.ready.to_string()),
        )
    } else if stalled {
        (
            theme.dag_runner_stalled_bg(),
            theme.dag_runner_stalled_border(),
            theme.dag_runner_stalled_fg(),
            theme.status_dot_warning(),
            t_fmt("dag.runner.stalled", &runner.ready.to_string()),
        )
    } else if runner.running {
        (
            theme.dag_runner_bg(),
            theme.dag_runner_border(),
            theme.dag_runner_fg(),
            theme.status_dot_success(),
            t_fmt2(
                "dag.runner.running",
                &runner.running_count.to_string(),
                &runner.ready.to_string(),
            ),
        )
    } else {
        (
            theme.dag_runner_bg(),
            theme.dag_runner_border(),
            theme.dag_runner_idle_fg(),
            theme.status_dot_idle(),
            t("dag.runner.idle").to_string(),
        )
    };

    let resp = egui::Frame::NONE
        .fill(bg.to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            border.to_egui(),
        ))
        .corner_radius(theme.dag_runner_radius().value())
        .inner_margin(margin_sym(theme.dag_runner_padding_x(), theme.spacing_xs))
        .show(ui, |ui| {
            // 필요한 폭만 확보해 헤더를 밀지 않는다.
            let font = tasty_ui_widgets::label_font(theme.font_size_caption.value());
            let galley = ui.painter().layout_no_wrap(text, font, fg.to_egui());
            let d = theme.status_dot_size().value();
            let gap = theme.dag_runner_gap().value();
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(
                    d + gap + galley.size().x,
                    theme.dag_runner_height().value().max(galley.size().y),
                ),
                egui::Sense::hover(),
            );
            ui.painter().circle_filled(
                egui::pos2(rect.min.x + d / 2.0, rect.center().y),
                d / 2.0,
                dot.to_egui(),
            );
            ui.painter().galley(
                egui::pos2(
                    rect.min.x + d + gap,
                    rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                fg.to_egui(),
            );
        })
        .response;

    if runner.crashed || stalled {
        resp.on_hover_text(resume_hint_text());
    }
}

/// 헤더 오른쪽 묶음의 조각들.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HeaderRightItem {
    /// 러너 알약.
    Pill,
    /// 재개 힌트 캡션.
    ResumeHint,
    /// 새로고침 버튼.
    Refresh,
}

/// 헤더 오른쪽 묶음을 **화면에서 읽히는 순서**(왼→오른)로. 오른쪽부터 채우는 줄에
/// 넣을 때는 [`header_right_paint_order`] 가 이것을 뒤집는다.
const HEADER_RIGHT_READING_ORDER: [HeaderRightItem; 3] = [
    HeaderRightItem::Pill,
    HeaderRightItem::ResumeHint,
    HeaderRightItem::Refresh,
];

/// 툴팁과 화면이 공유하는 재개 안내 순서와 고정폭 글꼴 여부.
const RESUME_HINT_PARTS: [(&str, bool); 2] = [
    ("dag.runner.resume_hint_lead", false),
    ("dag.runner.resume_hint_command", true),
];

/// 오른쪽부터 배치할 때 쓰는 역순.
fn header_right_paint_order() -> impl Iterator<Item = HeaderRightItem> {
    HEADER_RIGHT_READING_ORDER.into_iter().rev()
}

/// 같은 뒤집기의 재개 힌트 판.
fn resume_hint_paint_order() -> impl Iterator<Item = &'static (&'static str, bool)> {
    RESUME_HINT_PARTS.iter().rev()
}

/// 호버 툴팁에 싣는 재개 힌트 한 줄 — **읽는 순서 그대로** 이어 붙인다.
fn resume_hint_text() -> String {
    RESUME_HINT_PARTS
        .iter()
        .map(|(key, _)| t(key).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 좁은 헤더가 아니면 배지 옆에 재개 명령을 표시한다.
fn resume_hint(ui: &mut egui::Ui, theme: &Theme, runner: &RunnerBadgeData) {
    if !runner.crashed && !runner.is_stalled() {
        return;
    }
    let caption = |ui: &mut egui::Ui, text: String, mono: bool| {
        let mut rich = egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui());
        if mono {
            rich = rich.monospace();
        }
        ui.label(rich);
    };
    for (key, mono) in resume_hint_paint_order() {
        caption(ui, t(key).to_string(), *mono);
    }
    hspace(ui, theme.spacing_sm);
}

/// 줌 퍼센트 판독창 폭.
const ZOOM_READOUT_WIDTH: LogicalPx = LogicalPx(46.0);
/// 이 폭보다 좁으면 줌 숫자를 숨긴다. 대응 디자인 토큰은 없다.
const NARROW_ZOOM_LABEL: LogicalPx = LogicalPx(400.0);
/// 상세를 우측 패널로 둘 수 있는 최소 surface 폭. 그 아래는 하단 시트다.
pub const NARROW_DETAIL_SHEET: LogicalPx = LogicalPx(640.0);

/// 줌 클러스터 전체 크기 — 버튼 4 개 + (판독창) + 1px 구분선.
fn zoom_cluster_size(theme: &Theme, compact: bool) -> egui::Vec2 {
    let h = theme.dag_chrome_height().value();
    let mut w = h * 4.0 + theme.border_width.value();
    if !compact {
        w += ZOOM_READOUT_WIDTH.value();
    }
    egui::vec2(w, h)
}

/// 캔버스 안에서 줌 클러스터가 차지하는 자리. 캔버스 인터랙션이 이 영역을 비켜가야
/// 하므로 그리기 전에 먼저 알아야 한다.
pub fn zoom_cluster_rect(theme: &Theme, canvas: egui::Rect) -> egui::Rect {
    let inset = theme.dag_chrome_inset().value();
    let size = zoom_cluster_size(theme, canvas.width() < NARROW_ZOOM_LABEL.value());
    egui::Rect::from_min_size(
        egui::pos2(canvas.max.x - inset - size.x, canvas.max.y - inset - size.y),
        size,
    )
}

/// 클러스터 셀 하나. 눌렸으면 `true`.
fn cluster_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    salt: &'static str,
    icon: icons::Icon,
    enabled: bool,
    tooltip: &str,
) -> bool {
    let resp = ui.interact(
        rect,
        ui.id().with(("dag_zoom_cell", salt)),
        egui::Sense::click(),
    );
    if enabled && resp.hovered() {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.overlay_hover().to_egui_premultiplied(),
        );
    }
    let color = if enabled {
        theme.dag_chrome_fg()
    } else {
        theme.text_disabled()
    };
    let side = theme.icon_glyph_size_sm.value();
    icon.image(side, color.to_egui()).paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(side, side)),
    );
    enabled && resp.on_hover_text(tooltip).clicked()
}

/// 캔버스 위의 줌·맞춤·방향 버튼.
fn draw_zoom_cluster(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    view: &DagGraphView,
    direction: DagDirection,
) -> Option<ChromeAction> {
    let compact = rect.width() < zoom_cluster_size(theme, false).x;
    let radius = theme.corner_radius.value();
    let border = theme.dag_chrome_border().to_egui();
    ui.painter()
        .rect_filled(rect, radius, theme.dag_chrome_bg().to_egui());
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), border),
        egui::StrokeKind::Inside,
    );

    let side = theme.dag_chrome_height();
    let mut x = rect.min.x;
    // LogicalPx를 받아 egui 계산에서만 숫자로 꺼낸다.
    let mut cell = |w: LogicalPx| {
        let r = egui::Rect::from_min_size(
            egui::pos2(x, rect.min.y),
            egui::vec2(w.value(), side.value()),
        );
        x += w.value();
        r
    };

    let mut action = None;
    let minus = cell(side);
    if cluster_cell(
        ui,
        theme,
        minus,
        "out",
        icons::MINUS,
        view.zoom > ZOOM_MIN,
        t("dag.zoom.out"),
    ) {
        action = Some(ChromeAction::Zoom(-1.0));
    }
    if !compact {
        let readout = cell(ZOOM_READOUT_WIDTH);
        ui.painter().text(
            readout.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}%", (view.zoom * 100.0).round() as i32),
            egui::FontId::monospace(theme.font_size_caption.value()),
            theme.dag_chrome_fg().to_egui(),
        );
    }
    let plus = cell(side);
    if cluster_cell(
        ui,
        theme,
        plus,
        "in",
        icons::PLUS,
        view.zoom < ZOOM_MAX,
        t("dag.zoom.in"),
    ) {
        action = Some(ChromeAction::Zoom(1.0));
    }

    let sep = cell(theme.border_width);
    ui.painter().rect_filled(sep, 0.0, border);

    if cluster_cell(
        ui,
        theme,
        cell(side),
        "fit",
        icons::MOVE,
        true,
        t("dag.zoom.fit"),
    ) {
        action = Some(ChromeAction::Fit);
    }
    let dir_icon = match direction {
        DagDirection::LeftRight => icons::ARROW_RIGHT,
        DagDirection::TopDown => icons::ARROW_DOWN,
    };
    if cluster_cell(
        ui,
        theme,
        cell(side),
        "dir",
        dir_icon,
        true,
        t("dag.zoom.direction"),
    ) {
        action = Some(ChromeAction::ToggleDirection);
    }
    action
}

/// 캔버스의 미니맵·줌 버튼·축약 표시.
// 이유: 갤러리에서도 같은 인자 구성을 사용해 렌더링을 대조한다.
#[allow(clippy::too_many_arguments)]
pub fn draw_canvas_chrome(
    ui: &mut egui::Ui,
    theme: &Theme,
    canvas: egui::Rect,
    // 팝업이 back bar에 줌 버튼을 두면 미니맵은 캔버스 하단에 놓는다.
    cluster: Option<egui::Rect>,
    view: &DagGraphView,
    graph: &DagGraphData,
    layout: &GraphLayout,
    direction: DagDirection,
    lod: Lod,
) -> Option<ChromeAction> {
    let graph_size = egui::vec2(layout.width.value(), layout.height.value());
    let stack_bottom = match cluster {
        Some(c) => c.min.y,
        None => canvas.max.y - theme.dag_chrome_inset().value(),
    };
    let plot = MinimapPlot {
        graph,
        layout,
        graph_size,
    };
    paint_minimap(ui, theme, canvas, stack_bottom, view, plot);
    let action = cluster.and_then(|c| draw_zoom_cluster(ui, theme, c, view, direction));
    paint_lod_chip(&ui.painter_at(canvas), theme, canvas, lod);
    action
}

/// 팝업 back bar의 줌·러너 표시. 제목이 대상을 나타내고 주기적으로 조회하므로
/// DAG 선택기·새로고침 버튼은 중복으로 두지 않는다.
pub fn draw_detail_backbar_actions(
    ui: &mut egui::Ui,
    theme: &Theme,
    data: &DagData,
    view: &DagGraphView,
    direction: DagDirection,
) -> Option<ChromeAction> {
    runner_badge(ui, theme, &data.runner);
    let (rect, _) = ui.allocate_exact_size(zoom_cluster_size(theme, true), egui::Sense::hover());
    draw_zoom_cluster(ui, theme, rect, view, direction)
}

/// 사이클 배너. 사이클이면 그래프는 그리되 "이 그래프는 완주할 수 없다" 를 알린다.
pub fn draw_cycle_banner(ui: &mut egui::Ui, theme: &Theme, cycle: &[String]) {
    let fg = theme.dag_cycle_fg().to_egui();
    let dock = egui::Frame::NONE
        .fill(theme.dag_cycle_bg().to_egui())
        .inner_margin(margin_sym(theme.spacing_md, theme.spacing_xs))
        .show(ui, |ui| {
            ui.set_height(theme.dag_cycle_height().value());
            ui.horizontal_centered(|ui| {
                ui.add(icons::ALERT_TRIANGLE.image(theme.icon_glyph_size_sm.value(), fg));
                hspace(ui, theme.spacing_sm);
                ui.label(
                    egui::RichText::new(t_count(
                        "dag.cycle.lead",
                        cycle.len() as u64,
                        &[&cycle.len().to_string()],
                    ))
                    .size(theme.font_size_caption.value())
                    .color(fg),
                );
                // 첫 ID를 끝에 다시 붙여 순환 관계를 표시한다.
                let path = match cycle.first() {
                    Some(head) => format!("{} \u{2192} {head}", cycle.join(" \u{2192} ")),
                    None => String::new(),
                };
                hspace(ui, theme.spacing_xs);
                ui.label(
                    egui::RichText::new(path)
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(theme.text_secondary().to_egui()),
                );
            });
        })
        .response
        .rect;

    ui.painter().hline(
        dock.x_range(),
        dock.max.y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.dag_cycle_border().to_egui(),
        ),
    );
}

/// 좌하단 LOD 칩 — 지금 카드가 왜 축약돼 보이는지 알린다.
pub fn paint_lod_chip(painter: &egui::Painter, theme: &Theme, rect: egui::Rect, lod: Lod) {
    if lod == Lod::Full {
        return;
    }
    let text = match lod {
        Lod::Compact => t("dag.lod.compact"),
        _ => t("dag.lod.block"),
    };
    let pad = theme.spacing_xs.value();
    let font = egui::FontId::proportional(theme.font_size_micro.value());
    let galley = painter.layout_no_wrap(text.to_string(), font, theme.dag_chrome_fg().to_egui());
    let size = galley.size() + egui::vec2(pad * 2.0, pad);
    let chip = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + pad, rect.max.y - size.y - pad),
        size,
    );
    painter.rect_filled(
        chip,
        theme.dag_runner_radius().value(),
        theme.dag_chrome_bg().to_egui(),
    );
    painter.galley(
        chip.min + egui::vec2(pad, pad / 2.0),
        galley,
        theme.dag_chrome_fg().to_egui(),
    );
}

/// 미니맵이 그릴 그래프. `layout.nodes` 는 `graph.nodes` 와 순서가 같다.
struct MinimapPlot<'a> {
    graph: &'a DagGraphData,
    layout: &'a GraphLayout,
    graph_size: egui::Vec2,
}

/// 좁은 화면에서는 숨기는 미니맵. 줌 버튼 위에 배치한다.
fn paint_minimap(
    ui: &egui::Ui,
    theme: &Theme,
    canvas: egui::Rect,
    stack_bottom: f32,
    view: &DagGraphView,
    plot: MinimapPlot<'_>,
) {
    let MinimapPlot {
        graph,
        layout,
        graph_size,
    } = plot;
    if canvas.width() < theme.dag_minimap_min_surface().value()
        || graph_size.x <= 0.0
        || graph_size.y <= 0.0
    {
        return;
    }
    let painter = ui.painter_at(canvas);
    let gap = theme.spacing_sm.value();
    let size = egui::vec2(
        theme.dag_minimap_width().value(),
        theme.dag_minimap_height().value(),
    );
    let map = egui::Rect::from_min_size(
        egui::pos2(
            canvas.max.x - size.x - theme.dag_chrome_inset().value(),
            stack_bottom - gap - size.y,
        ),
        size,
    );
    painter.rect_filled(
        map,
        theme.dag_node_radius().value(),
        theme.dag_minimap_bg().to_egui(),
    );

    let k = (size.x / graph_size.x).min(size.y / graph_size.y);
    let base = map.min + (size - graph_size * k) / 2.0;
    let node = theme.dag_minimap_node().to_egui();
    let (nw, nh) = (
        theme.dag_node_width().value() * k,
        theme.dag_node_height().value() * k,
    );
    let node_dim = theme.dag_minimap_node_dim().to_egui();
    for (n, data) in layout.nodes.iter().zip(&graph.nodes) {
        painter.rect_filled(
            egui::Rect::from_min_size(
                base + egui::vec2(n.x.value() * k, n.y.value() * k),
                egui::vec2(nw.max(1.0), nh.max(1.0)),
            ),
            0.0,
            if data.status.is_dimmed() {
                node_dim
            } else {
                node
            },
        );
    }

    // 현재 뷰포트가 그래프의 어디를 보고 있는지. 화면 좌표 → 그래프 좌표 역변환.
    let vp = egui::Rect::from_min_size(
        base + egui::vec2(
            -view.offset.x / view.zoom * k,
            -view.offset.y / view.zoom * k,
        ),
        egui::vec2(
            canvas.width() / view.zoom * k,
            canvas.height() / view.zoom * k,
        ),
    );
    painter.rect_stroke(
        vp.intersect(map),
        0.0,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.dag_minimap_viewport().to_egui(),
        ),
        egui::StrokeKind::Inside,
    );
}

/// DAG가 없거나 지정한 DAG가 사라진 경우를 구분해 가운데 안내한다.
pub fn draw_empty(ui: &mut egui::Ui, theme: &Theme, data: &DagData, dag_id: Option<&str>) {
    let (icon, title, hint) = if data.target_missing {
        (
            icons::SEARCH,
            t_fmt("dag.empty.missing", dag_id.unwrap_or("")),
            t("dag.empty.missing_hint").to_string(),
        )
    } else {
        (
            icons::GIT_TREE,
            t("dag.empty.none").to_string(),
            t_fmt("dag.empty.none_hint", &data.workspace_id.to_string()),
        )
    };
    egui::Frame::NONE
        .fill(theme.dag_canvas_bg().to_egui())
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            let view = tasty_ui_widgets::DagEmptyView {
                icon,
                title: &title,
                hint: &hint,
            };
            tasty_ui_widgets::paint_dag_empty(ui, theme, ui.max_rect(), &view);
        });
}

/// 그래프 데이터가 비어 있는지(노드 0 개).
pub fn is_empty(graph: Option<&DagGraphData>) -> bool {
    graph.is_none_or(|g| g.nodes.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_right_group_is_painted_in_reverse_reading_order() {
        let painted: Vec<HeaderRightItem> = header_right_paint_order().collect();
        assert_eq!(
            painted,
            [
                HeaderRightItem::Refresh,
                HeaderRightItem::ResumeHint,
                HeaderRightItem::Pill,
            ],
            "오른쪽부터 배치하므로 새로고침을 먼저 추가해야 상태 표시 오른쪽에 놓인다"
        );
    }

    /// 재개 힌트 캡션도 같은 줄에 놓이므로 같은 뒤집기를 받는다.
    #[test]
    fn the_resume_hint_captions_are_painted_in_reverse_reading_order() {
        let painted: Vec<&str> = resume_hint_paint_order().map(|(key, _)| *key).collect();
        assert_eq!(
            painted,
            [
                "dag.runner.resume_hint_command",
                "dag.runner.resume_hint_lead",
            ],
            "명령을 먼저 추가해야 오른쪽부터 그린 결과가 안내문·명령 순서가 된다"
        );
    }

    /// 실제 그리기 순서가 읽는 순서의 역순인지 확인한다.
    #[test]
    fn both_paint_orders_are_exact_reverses_of_their_rosters() {
        let mut painted: Vec<HeaderRightItem> = header_right_paint_order().collect();
        painted.reverse();
        assert_eq!(painted, HEADER_RIGHT_READING_ORDER.to_vec());

        let mut hint: Vec<&str> = resume_hint_paint_order().map(|(key, _)| *key).collect();
        hint.reverse();
        let reading: Vec<&str> = RESUME_HINT_PARTS.iter().map(|(key, _)| *key).collect();
        assert_eq!(hint, reading);
    }

    /// 툴팁은 화면 배치와 달리 읽는 순서대로 이어 붙인다.
    #[test]
    fn the_tooltip_reads_forwards_while_the_line_paints_backwards() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 툴팁 문구를 만들기 전에 초기화한다.
        crate::i18n::init("en");
        let reading: Vec<&str> = RESUME_HINT_PARTS.iter().map(|(key, _)| *key).collect();
        let painted: Vec<&str> = resume_hint_paint_order().map(|(key, _)| *key).collect();
        assert_ne!(
            reading, painted,
            "화면 배치 순서는 툴팁 읽기 순서와 반대여야 한다"
        );

        let text = resume_hint_text();
        let lead = t(RESUME_HINT_PARTS[0].0).to_string();
        let command = t(RESUME_HINT_PARTS[1].0).to_string();
        let at_lead = text.find(&lead).expect("툴팁에 안내문이 없다");
        let at_command = text.find(&command).expect("툴팁에 명령이 없다");
        assert!(
            at_lead < at_command,
            "툴팁이 화면과 다른 순서로 읽힌다: {text}"
        );
    }

    /// 명령만 등폭이다 — 셸에 그대로 붙여 넣는 문자열이라 인자 경계가 눈에 잡혀야 한다.
    #[test]
    fn only_the_command_part_is_monospaced() {
        let mono: Vec<&str> = RESUME_HINT_PARTS
            .iter()
            .filter(|(_, mono)| *mono)
            .map(|(key, _)| *key)
            .collect();
        assert_eq!(mono, ["dag.runner.resume_hint_command"]);
    }
}
