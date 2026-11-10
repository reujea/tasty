//! DAG 헤더·실행 상태·사이클 배너·빈 상태 안내. 캔버스 위 버튼·미니맵은 공통 그래프 뷰가 그린다.
//! 폭이 좁으면 헤더를 두 줄로 나누고 러너 상태와 순환 관계 경고는 남긴다.

use tasty_model::DagDirection;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    ControlSize, GRAPH_NARROW_DETAIL_SHEET as NARROW_DETAIL_SHEET, GraphChromeAction,
    GraphChromeLabels, GraphView, IconButton, IconButtonVariant, draw_zoom_cluster,
    graph_zoom_cluster_size, hspace, margin_sym,
};

use super::model::{DagData, DagGraphData, RunnerBadgeData};
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

impl From<GraphChromeAction> for ChromeAction {
    fn from(action: GraphChromeAction) -> Self {
        match action {
            GraphChromeAction::ToggleDirection => ChromeAction::ToggleDirection,
            GraphChromeAction::Fit => ChromeAction::Fit,
            GraphChromeAction::Zoom(steps) => ChromeAction::Zoom(steps),
        }
    }
}

/// 캔버스 버튼 툴팁과 LOD 칩 문구.
pub fn graph_chrome_labels() -> GraphChromeLabels<'static> {
    GraphChromeLabels {
        zoom_in: t("dag.zoom.in"),
        zoom_out: t("dag.zoom.out"),
        fit: t("dag.zoom.fit"),
        direction: t("dag.zoom.direction"),
        lod_compact: t("dag.lod.compact"),
        lod_block: t("dag.lod.block"),
    }
}

/// DAG 방향을 공통 그래프 뷰의 방향으로.
pub fn orientation_of(direction: DagDirection) -> tasty_dag_layout::Orientation {
    match direction {
        DagDirection::LeftRight => tasty_dag_layout::Orientation::LeftRight,
        DagDirection::TopDown => tasty_dag_layout::Orientation::TopDown,
    }
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

/// 팝업 back bar의 줌·러너 표시. 제목이 대상을 나타내고 주기적으로 조회하므로
/// DAG 선택기·새로고침 버튼은 중복으로 두지 않는다.
pub fn draw_detail_backbar_actions(
    ui: &mut egui::Ui,
    theme: &Theme,
    data: &DagData,
    view: &GraphView,
    direction: DagDirection,
) -> Option<ChromeAction> {
    runner_badge(ui, theme, &data.runner);
    let (rect, _) =
        ui.allocate_exact_size(graph_zoom_cluster_size(theme, true), egui::Sense::hover());
    let labels = graph_chrome_labels();
    draw_zoom_cluster(ui, theme, rect, view, orientation_of(direction), &labels)
        .map(ChromeAction::from)
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

/// DAG가 없거나 지정한 DAG가 사라진 경우를 구분해 가운데 안내한다.
pub fn draw_empty(
    ui: &mut egui::Ui,
    theme: &Theme,
    canvas_bg: egui::Color32,
    data: &DagData,
    dag_id: Option<&str>,
) {
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
    egui::Frame::NONE.fill(canvas_bg).show(ui, |ui| {
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
