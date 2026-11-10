//! DAG 목록과 선택한 그래프를 같은 팝업에서 보여 준다.
//! 팝업은 열 당시 워크스페이스에 속해 전환 시 숨고, 돌아오면 보던 상태로 표시된다.
//! 목록은 기본적으로 모든 워크스페이스를 포함하며 현재 워크스페이스만 고를 수도 있다.
//! release IPC로 팝업을 강제로 열지는 않는다. 에이전트는 agent.dag_list/get으로 데이터를 읽는다.

use crate::runtime::engine_read::EngineRead;
use std::time::Instant;

use tasty_icons as icons;
use tasty_model::DagDirection;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, DrillDown, DrillDownView, Input, ListCtrl, ListCtrlItem,
    MultiSelectLabels, TagVariant, checkbox, hspace, margin_sym, multi_select,
    multi_select_popup_id, tag,
};

use super::PopupAction;
use crate::adapters::ui::surface::dag_graph::{
    DagChrome, DagTarget,
    chrome::{ChromeAction, draw_detail_backbar_actions},
    draw_dag_graph,
    model::{DagStatus, DagStatusText as _, format_clock, skip_count_suffix},
    node::status_colors,
    view::{DagGraphView, DagSourceTarget, POLL_INTERVAL},
};
use crate::adapters::ui::surface::graph_source::GraphSource as _;
use crate::i18n::{t, t_count, t_count_template, t_fmt, t_fmt2};
use crate::state::MainViewState;

pub const DAG_LIST_POPUP_ID: &str = "dag_list";

/// 상태 필터 드롭다운의 위젯 id salt + 자식 오버레이 레지스트리 키. egui popup id 는
/// 규약을 복제하지 않고 `multi_select_popup_id(ui, salt)` 로 위젯에게 물어본다.
const STATUS_SELECT_SALT: &str = "dag_list_status";
const STATUS_SELECT_OVERLAY_KEY: &str = "dag_list_status";

/// 배율이 적용된 Theme 토큰으로 팝업 크기를 계산한다.
pub fn dag_list_sizer(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let th = crate::theme::theme();
    egui::vec2(th.dag_popup_width().value(), th.dag_popup_height().value())
}

/// 조회한 DAG 정보와 워크스페이스 이름을 함께 보관한 화면용 행.
pub struct DagRow {
    workspace_id: u32,
    workspace_name: String,
    id: String,
    name: String,
    /// `source == "derived"` — 사용자가 묶은 게 아니라 의존 연결성에서 도출된
    /// 그룹. 행 끝에 태그로 표시한다.
    derived: bool,
    rollup: DagStatus,
    done: usize,
    total: usize,
    /// 건너뛴 task 수와 그중 경로가 선택되지 않은 수. 완료/전체 뒤에 붙인다.
    skipped: usize,
    not_selected: usize,
    /// agent 세션이 사람의 입력을 기다리는 task 수. rollup 은 running 그대로이므로 따로 알린다.
    awaiting: usize,
    updated_at: u64,
}

/// updated_at 내림차순으로 표시해 최근 생성·진행한 DAG를 위에 둔다.
/// 동률은 ID 내림차순, 워크스페이스 ID 오름차순으로 정한다. explicit ID는
/// 워크스페이스마다 같을 수 있으므로 둘 다 비교한다. IPC 응답 순서는 바꾸지 않는다.
fn sort_recent_first(rows: &mut [DagRow]) {
    rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.id.cmp(&a.id))
            .then_with(|| a.workspace_id.cmp(&b.workspace_id))
    });
}

/// popup 의 전 상태. `on_close` 가 통째로 되돌린다(`DialogState` 관례).
#[derive(Default)]
pub struct DagListState {
    view: DrillDownView,
    /// 선택한 DAG는 워크스페이스와 ID의 쌍으로 식별한다. 그래프 선택기는 같은 워크스페이스 안에서만 바꾼다.
    open_workspace: Option<u32>,
    open_dag: Option<String>,
    query: String,
    this_workspace_only: bool,
    /// 상태 필터 — `DagStatus::ROLLUP_ALL` 과 같은 길이·순서의 on/off 배열.
    /// 기본값(전부 off)은 "모든 상태" 와 같은 의미로, 필터가 아무것도 거르지 않는다.
    status_filter: [bool; DagStatus::ROLLUP_ALL.len()],
    direction: DagDirection,
    graph: DagGraphView,
    rows: Vec<DagRow>,
    last_list_poll: Option<Instant>,
    list_query: Option<crate::runtime::dag_query::DagQuery>,
    list_workspaces: Vec<u32>,
}

impl DagListState {
    /// Tick::DagListPopup의 다음 조회 시각. 상세 화면이 닫혀 있으면 그래프 시각은 제외한다.
    /// 조회하지 않는 그래프의 None을 포함하면 매 프레임 즉시 조회를 예약하게 된다.
    pub(crate) fn next_poll_at(&self, now: Instant) -> Instant {
        let list = if self
            .list_query
            .as_ref()
            .is_some_and(|query| query.is_pending())
        {
            now + POLL_INTERVAL
        } else {
            self.last_list_poll.map_or(now, |t| t + POLL_INTERVAL)
        };
        if self.open_workspace.is_some() && self.open_dag.is_some() {
            list.min(self.graph.source.next_poll_at().unwrap_or(now))
        } else {
            list
        }
    }

    /// 목록도 그래프와 같은 주기로만 다시 읽는다.
    fn list_is_stale(&self, now: Instant) -> bool {
        self.last_list_poll
            .is_none_or(|t| now.duration_since(t) >= POLL_INTERVAL)
    }

    fn poll_list(&mut self, engine: &EngineRead<'_>) {
        let now = Instant::now();
        let source = engine.dag_source();
        let workspaces = crate::app::task_completion::dag_workspaces(
            engine
                .workspaces()
                .into_iter()
                .map(|workspace| workspace.id),
            None,
        );
        if self.list_workspaces != workspaces
            || self
                .list_query
                .as_ref()
                .is_some_and(|query| !source.matches(query))
        {
            self.list_query = None;
            self.last_list_poll = None;
            self.rows.clear();
            self.list_workspaces = workspaces;
        }
        let result = self
            .list_query
            .as_mut()
            .and_then(|query| query.poll(source));
        if let Some(result) = result {
            self.last_list_poll = Some(now);
            match result.map(|snapshots| {
                snapshots
                    .into_iter()
                    .flat_map(|snapshot| snapshot.dags)
                    .collect::<Vec<_>>()
            }) {
                Ok(summaries) => {
                    self.rows = summaries
                        .into_iter()
                        .map(|s| {
                            let c = &s.state_counts;
                            DagRow {
                                workspace_name: engine
                                    .workspaces()
                                    .into_iter()
                                    .find(|w| w.id == s.workspace_id)
                                    .map(|w| w.name.clone())
                                    // 조회 중 워크스페이스가 사라졌으면 이름 대신 ID를 표시한다.
                                    .unwrap_or_else(|| s.workspace_id.to_string()),
                                workspace_id: s.workspace_id,
                                id: s.id,
                                name: s.name,
                                derived: s.source == "derived",
                                rollup: DagStatus::from_name(s.rollup_state),
                                done: c.succeeded + c.failed + c.cancelled + c.skipped,
                                total: s.task_count,
                                skipped: c.skipped,
                                not_selected: c.not_selected,
                                awaiting: c.awaiting_input,
                                updated_at: s.updated_at,
                            }
                        })
                        .collect();
                    sort_recent_first(&mut self.rows);
                }
                Err(e) => {
                    if matches!(e, crate::runtime::dag_query::ReadError::Retired) {
                        self.rows.clear();
                    }
                    // 일시적인 실패에는 마지막으로 읽은 목록을 유지한다.
                    tracing::warn!(target: "tasty::dag", "dag list poll failed: {e}");
                }
            }
        }
        if self
            .list_query
            .as_ref()
            .is_none_or(|query| !query.is_pending())
            && self.list_is_stale(now)
        {
            self.list_query = Some(source.request(self.list_workspaces.clone()));
            self.last_list_poll = Some(now);
        }
    }

    /// 필터를 통과한 행의 인덱스.
    fn visible(&self, active_workspace_id: Option<u32>) -> Vec<usize> {
        let needle = self.query.trim().to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                if self.this_workspace_only && active_workspace_id != Some(r.workspace_id) {
                    return false;
                }
                if !status_matches(&self.status_filter, r.rollup) {
                    return false;
                }
                needle.is_empty()
                    || r.name.to_lowercase().contains(&needle)
                    || r.workspace_name.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }
}

/// ROLLUP_ALL 순서의 선택 배열. 모두 꺼져 있으면 전체, 하나라도 켜져 있으면 선택한 상태 중 하나와 일치해야 한다.
fn status_matches(selected: &[bool], rollup: DagStatus) -> bool {
    if !selected.iter().any(|on| *on) {
        return true;
    }
    DagStatus::ROLLUP_ALL
        .iter()
        .zip(selected)
        .any(|(s, on)| *on && *s == rollup)
}

/// popup 본문.
pub fn draw_dag_list_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
) -> PopupAction {
    let active_workspace_id = engine
        .workspace_at(state.active_workspace_index(engine))
        .map(|w| w.id);
    let dag = &mut state.dialogs.dag_list;

    // 드롭다운이 열려 있으면 부모 닫기만 건너뛴다. 본문은 그려야 드롭다운이 Escape를 처리한다.
    let esc = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
    let esc_owned_by_dropdown =
        esc && super::child_overlay_open(ui.ctx(), DAG_LIST_POPUP_ID, STATUS_SELECT_OVERLAY_KEY);
    if esc && !esc_owned_by_dropdown {
        if dag.view.is_detail() {
            back_to_list(dag);
            return PopupAction::None;
        }
        return PopupAction::Close;
    }

    dag.poll_list(&engine.as_ref());
    if let (Some(ws), Some(id)) = (dag.open_workspace, dag.open_dag.clone()) {
        dag.graph.source.poll_if_stale(
            &engine.as_ref(),
            DagSourceTarget {
                workspace_id: ws,
                dag_id: Some(id.as_str()),
            },
        );
    }

    // 주기 조회는 egui 지연 repaint 가 아니라 타이머 허브로 다음 조회를 예약한다. 지연 repaint 는 그려질 때만 갱신된다.

    let th = crate::theme::theme();
    let theme = &th;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

    let visible = dag.visible(active_workspace_id);
    let total = dag.rows.len();
    let title = dag
        .open_dag
        .as_ref()
        .and_then(|id| dag.rows.iter().find(|r| &r.id == id))
        .map(|r| r.name.clone())
        .unwrap_or_default();

    let mut close = false;
    let view = dag.view;
    // back bar에서 받은 동작을 본문에 전달한다.
    let backbar_action: std::cell::RefCell<Option<ChromeAction>> = std::cell::RefCell::new(None);
    // DrillDown은 두 Fn 클로저 중 하나만 실행하므로 공유 상태를 RefCell로 빌린다.
    let cell = std::cell::RefCell::new(dag);
    let open_session = std::cell::Cell::new(None);
    let actions = |ui: &mut egui::Ui, theme: &Theme| {
        let dag = cell.borrow();
        let Some(data) = dag.graph.source.data.clone() else {
            return;
        };
        let direction = dag.direction;
        if let Some(action) =
            draw_detail_backbar_actions(ui, theme, &data, &dag.graph.canvas, direction)
        {
            *backbar_action.borrow_mut() = Some(action);
        }
    };
    let out = DrillDown::new("dag_list")
        .view(view)
        .title(&title)
        .back_label(t("dag_list.back"))
        .show(
            ui,
            theme,
            |ui, theme| close |= draw_list(ui, theme, &mut cell.borrow_mut(), &visible, total),
            |ui, theme| {
                let pending = backbar_action.borrow_mut().take();
                open_session.set(draw_detail_graph(
                    ui,
                    theme,
                    &mut cell.borrow_mut(),
                    pending,
                ));
            },
            Some(&actions),
        );

    if out.back_clicked {
        back_to_list(cell.into_inner());
    }

    // 사용자가 상세에서 누른 세션 열기다. 세션이 보이도록 포커스를 옮기고 popup 을 닫는다.
    if let Some(session) = open_session.get() {
        if state.reveal_surface(engine, session) {
            return PopupAction::Close;
        }
        tracing::warn!(session, "DAG open session: surface not found");
    }

    if close {
        PopupAction::Close
    } else {
        PopupAction::None
    }
}

/// 디테일 → 목록. 그래프 쪽 상태도 함께 놓아준다.
fn back_to_list(dag: &mut DagListState) {
    dag.view = DrillDownView::List;
    dag.open_workspace = None;
    dag.open_dag = None;
    dag.graph = DagGraphView::default();
}

/// 목록 뷰 — 검색·필터 줄 · 토글 줄 · 행 목록 · 푸터. 반환값은 "닫기" 눌림.
fn draw_list(
    ui: &mut egui::Ui,
    theme: &Theme,
    dag: &mut DagListState,
    visible: &[usize],
    total: usize,
) -> bool {
    // `separator` 는 **미리 곱해진**(premultiplied) 반투명 색이다 — `to_egui()` 로
    // 읽으면 알파가 한 번 더 곱해져 배경보다 어두운, 사실상 보이지 않는 선이 된다.
    let sep = egui::Stroke::new(
        theme.border_width.value(),
        theme.separator.to_egui_premultiplied(),
    );
    let full = ui.available_rect_before_wrap();
    let x_range = full.x_range();

    let search_ir = egui::Frame::NONE
        .inner_margin(margin_sym(theme.spacing_md, theme.spacing_sm))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let filter_w = theme.field_width_md.value();
                let search_w =
                    (ui.available_width() - filter_w - theme.spacing_sm.value()).max(0.0);
                Input::new()
                    .icon(&|ui, rect, color| {
                        icons::SEARCH.image(rect.height(), color).paint_at(ui, rect);
                    })
                    .placeholder(t("dag_list.search_placeholder"))
                    .width(search_w)
                    .show(ui, theme, &mut dag.query);
                hspace(ui, theme.spacing_sm);
                // DAG 집계 상태만 표시한다. 개별 task 전용 cancelled/unknown은 제외한다.
                let labels: Vec<&str> = DagStatus::ROLLUP_ALL.iter().map(|s| s.label()).collect();
                let summary = MultiSelectLabels {
                    none: t("dag_list.status_any"),
                    some: t("dag_list.status_some"),
                    some_one: t_count_template("dag_list.status_some", 1),
                    all: t("dag_list.status_all"),
                };
                multi_select(
                    ui,
                    theme,
                    STATUS_SELECT_SALT,
                    &mut dag.status_filter,
                    &labels,
                    None,
                    &summary,
                    None,
                    filter_w,
                    true,
                );
                // 팝업 밖으로 나온 드롭다운도 안쪽 클릭으로 인식하도록 영역을 보고한다.
                let overlay_id = multi_select_popup_id(ui, STATUS_SELECT_SALT);
                super::report_dropdown_overlay(
                    ui,
                    DAG_LIST_POPUP_ID,
                    STATUS_SELECT_OVERLAY_KEY,
                    overlay_id,
                );
            });
        });
    ui.painter()
        .hline(x_range, search_ir.response.rect.bottom(), sep);

    let toggle_ir = egui::Frame::NONE
        .inner_margin(margin_sym(theme.spacing_md, theme.spacing_xs))
        .show(ui, |ui| {
            checkbox(
                ui,
                theme,
                &mut dag.this_workspace_only,
                t("dag_list.this_workspace_only"),
                true,
                tasty_ui_widgets::Ground::Panel,
            );
        });
    ui.painter()
        .hline(x_range, toggle_ir.response.rect.bottom(), sep);

    let footer_h = ControlSize::Md.height(theme) + theme.spacing_sm.value() * 2.0;
    let list_h = (full.bottom() - ui.cursor().top() - footer_h).max(0.0);

    let mut picked = None;
    ui.allocate_ui(egui::vec2(full.width(), list_h), |ui| {
        if visible.is_empty() {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(full.width(), list_h), egui::Sense::hover());
            draw_empty(ui, theme, rect, total, &dag.query);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("dag_list_rows")
            .auto_shrink([false, false])
            .drag_to_scroll(false)
            .show(ui, |ui| {
                // ListCtrlItem이 빌리는 문자열·클로저를 항목보다 먼저 만든다.
                let prepared: Vec<(String, &DagRow)> = visible
                    .iter()
                    .map(|&i| {
                        let r = &dag.rows[i];
                        let meta = t_fmt2(
                            "dag_list.row_meta",
                            &r.workspace_name,
                            &format_clock(r.updated_at),
                        );
                        (meta, r)
                    })
                    .collect();
                let icon = |ui: &mut egui::Ui, rect: egui::Rect, color: egui::Color32| {
                    icons::GIT_TREE
                        .image(rect.height(), color)
                        .paint_at(ui, rect);
                };
                type Trailing<'a> = Box<dyn Fn(&mut egui::Ui, &Theme) + 'a>;
                let trailings: Vec<Trailing<'_>> = prepared
                    .iter()
                    .map(|(_, r)| -> Trailing<'_> {
                        Box::new(move |ui, theme| draw_row_trailing(ui, theme, r))
                    })
                    .collect();
                let items: Vec<ListCtrlItem<'_>> = prepared
                    .iter()
                    .zip(&trailings)
                    .map(|((meta, r), trailing)| {
                        ListCtrlItem::new(&r.name)
                            .description(meta)
                            .icon(&icon)
                            .trailing(&**trailing)
                    })
                    .collect();
                if let Some(hit) = ListCtrl::new().show(ui, theme, &items, None).clicked {
                    picked = visible.get(hit).copied();
                }
            });
    });

    if let Some(i) = picked {
        let r = &dag.rows[i];
        dag.open_workspace = Some(r.workspace_id);
        dag.open_dag = Some(r.id.clone());
        // 이전 DAG의 선택·배율·레이아웃·조회 시각을 새 그래프에 넘기지 않는다.
        dag.graph = DagGraphView::default();
        dag.view = DrillDownView::Detail;
    }

    let mut close = false;
    ui.painter().hline(x_range, ui.cursor().top(), sep);
    ui.allocate_ui(egui::vec2(full.width(), footer_h), |ui| {
        egui::Frame::NONE
            .inner_margin(margin_sym(theme.spacing_md, theme.spacing_sm))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t_count(
                            "dag_list.count",
                            total as u64,
                            &[&visible.len().to_string(), &total.to_string()],
                        ))
                        .family(tasty_ui_widgets::label_font_family())
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted().to_egui()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = Button::new(t("dag_list.close"))
                            .variant(ButtonVariant::Secondary)
                            .show(ui, theme)
                            .clicked();
                    });
                });
            });
    });
    close
}

/// 출처·상태와 정확한 완료/전체 개수, 건너뛴 수를 표시한다.
fn draw_row_trailing(ui: &mut egui::Ui, theme: &Theme, row: &DagRow) {
    // trailing 영역은 오른쪽부터 채우므로 화면 순서의 역순으로 그린다.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.dag_row_summary_gap().value();
        ui.label(
            egui::RichText::new(format!(
                "{}/{}{}",
                row.done,
                row.total,
                skip_count_suffix(row.skipped, row.not_selected)
            ))
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.dag_row_count_font_size().value())
            .color(theme.dag_row_count_fg().to_egui()),
        );
        // 작은 글자의 대비를 확보하는 상태별 label 색을 사용한다.
        let (_, _, label_fg) = status_colors(theme, row.rollup);
        ui.label(
            egui::RichText::new(format!("{} {}", row.rollup.glyph(), row.rollup.label()))
                .family(tasty_ui_widgets::label_font_family())
                .size(theme.font_size_caption.value())
                .color(label_fg.to_egui()),
        );
        if row.awaiting > 0 {
            ui.label(
                egui::RichText::new(format!(
                    "! {}",
                    t_count(
                        "dag_list.awaiting",
                        row.awaiting as u64,
                        &[&row.awaiting.to_string()]
                    )
                ))
                .family(tasty_ui_widgets::label_font_family())
                .size(theme.font_size_caption.value())
                .color(theme.dag_phase_awaiting_label().to_egui()),
            );
        }
        if row.derived {
            tag(ui, theme, t("dag_list.derived"), TagVariant::Default, false);
        }
    });
}

/// DAG가 없는 경우와 필터 결과가 없는 경우를 구분해 목록 영역 가운데에 안내한다.
/// 검색어가 있으면 그 검색어를 되비추고, 검색어 없이 상태·범위 필터만 걸렀으면 필터 무매치 문구를 쓴다.
fn draw_empty(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, total: usize, query: &str) {
    let (icon, title, hint) = empty_copy(total, query);
    let view = tasty_ui_widgets::DagEmptyView {
        icon,
        title: &title,
        hint: &hint,
    };
    tasty_ui_widgets::paint_dag_empty(ui, theme, rect, &view);
}

/// 빈 목록의 글리프와 문구. `total`은 필터 전 DAG 수다.
fn empty_copy(total: usize, query: &str) -> (icons::Icon, String, String) {
    let needle = query.trim();
    if total == 0 {
        (
            icons::GIT_TREE,
            t("dag_list.empty_none").to_owned(),
            t("dag_list.empty_none_hint").to_owned(),
        )
    } else if needle.is_empty() {
        // 검색어가 없으니 원인은 상태 필터나 범위다. 글리프도 검색이 아니라 필터다.
        (
            icons::FUNNEL,
            t("dag.popup.no_match_filtered").to_owned(),
            t("dag.popup.no_match_hint").to_owned(),
        )
    } else {
        (
            icons::SEARCH,
            t_fmt("dag.popup.no_match", needle),
            t("dag.popup.no_match_hint").to_owned(),
        )
    }
}

/// surface와 같은 그래프를 그린다. 노드 상세의 배치는 가용 폭에 따라 정해진다.
fn draw_detail_graph(
    ui: &mut egui::Ui,
    theme: &Theme,
    dag: &mut DagListState,
    pending: Option<ChromeAction>,
) -> Option<u32> {
    if dag.open_dag.is_none() {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, theme.dag_canvas_bg().to_egui());
        return None;
    }
    let target = DagTarget {
        dag_id: &mut dag.open_dag,
        direction: &mut dag.direction,
    };
    // 크롬은 back bar 가 든다 — 헤더도 캔버스 줌 클러스터도 여기서는 안 그린다.
    draw_dag_graph(ui, target, &mut dag.graph, DagChrome::Popup(pending))
}

/// 닫힘 정리 — 어떤 경로로 닫히든 다음 open 은 **목록 뷰**에서 시작한다.
pub fn on_close_dag_list_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.dialogs.dag_list = DagListState::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_copy_picks_none_then_filter_then_query_echo() {
        let (_, title, hint) = empty_copy(0, "deploy");
        assert_eq!(title, t("dag_list.empty_none"));
        assert_eq!(hint, t("dag_list.empty_none_hint"));

        let (icon, title, hint) = empty_copy(3, "   ");
        assert_eq!(icon.svg, icons::FUNNEL.svg);
        assert_eq!(title, t("dag.popup.no_match_filtered"));
        assert_eq!(hint, t("dag.popup.no_match_hint"));

        let (_, title, hint) = empty_copy(3, " deploy ");
        assert_eq!(title, t_fmt("dag.popup.no_match", "deploy"));
        assert_eq!(hint, t("dag.popup.no_match_hint"));
    }

    fn row(id: &str, updated_at: u64, workspace_id: u32) -> DagRow {
        DagRow {
            workspace_id,
            workspace_name: String::new(),
            id: id.to_string(),
            name: String::new(),
            derived: id.starts_with("c:"),
            rollup: DagStatus::Waiting,
            done: 0,
            total: 0,
            skipped: 0,
            not_selected: 0,
            awaiting: 0,
            updated_at,
        }
    }

    fn ids(rows: &[DagRow]) -> Vec<&str> {
        rows.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn closing_and_reopening_the_popup_retires_its_old_list_receipt() {
        let (_, engine) = crate::state::tests::test_state();
        let mut popup = DagListState::default();
        popup.poll_list(&engine.read());
        let old = engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap();
        popup = DagListState::default();
        assert!(!old.is_live());
        popup.poll_list(&engine.read());
        old.complete(Err("closed popup result".into()));
        engine
            .runtime
            .dag_reads
            .take(engine.journal_binding.as_ref())
            .unwrap()
            .complete(Ok(Vec::new()));
        popup.poll_list(&engine.read());
        assert!(!popup.list_query.as_ref().unwrap().is_pending());
        assert!(popup.rows.is_empty());
    }

    #[test]
    fn 최근_갱신이_맨_위로_온다() {
        let mut rows = vec![
            row("c:t-100", 100, 1),
            row("c:t-300", 300, 1),
            row("c:t-200", 200, 1),
        ];
        sort_recent_first(&mut rows);
        assert_eq!(
            rows.iter().map(|r| r.updated_at).collect::<Vec<_>>(),
            [300, 200, 100]
        );
    }

    #[test]
    fn 갱신시각_동률은_id_내림차순으로_끊는다() {
        let mut rows = vec![
            row("c:t-1", 500, 1),
            row("d:beta", 500, 1),
            row("c:t-2", 500, 1),
        ];
        sort_recent_first(&mut rows);
        assert_eq!(ids(&rows), ["d:beta", "c:t-2", "c:t-1"]);
    }

    #[test]
    fn 갱신시각과_id_가_모두_같으면_workspace_로_끊는다() {
        let mut rows = vec![row("d:same", 500, 7), row("d:same", 500, 2)];
        sort_recent_first(&mut rows);
        assert_eq!(
            rows.iter().map(|r| r.workspace_id).collect::<Vec<_>>(),
            [2, 7]
        );
    }

    #[test]
    fn 반복_정렬은_같은_결과를_낸다() {
        let build = || {
            vec![
                row("d:alpha", 500, 3),
                row("c:t-9", 500, 1),
                row("c:t-1", 900, 2),
                row("d:zeta", 100, 1),
            ]
        };
        let mut once = build();
        sort_recent_first(&mut once);
        let mut twice = once;
        sort_recent_first(&mut twice);
        let mut from_scratch = build();
        sort_recent_first(&mut from_scratch);
        assert_eq!(ids(&twice), ids(&from_scratch));
    }

    /// 생성 순서와 무관하게 최근 갱신한 DAG가 먼저 나온다.
    #[test]
    fn 오래_전에_만들어졌어도_방금_움직였으면_맨_위() {
        let mut rows = vec![
            row("c:t-1000000000001", 999, 1), // 가장 오래 전 생성, 방금 갱신
            row("c:t-1000000000002", 200, 1),
            row("c:t-1000000000003", 300, 1), // 가장 최근 생성
        ];
        sort_recent_first(&mut rows);
        assert_eq!(
            ids(&rows),
            [
                "c:t-1000000000001",
                "c:t-1000000000003",
                "c:t-1000000000002"
            ]
        );
    }

    #[test]
    fn derived_와_explicit_이_출처와_무관하게_섞인다() {
        let mut rows = vec![
            row("d:old-explicit", 100, 1),
            row("c:t-new-derived", 900, 1),
            row("d:new-explicit", 800, 1),
            row("c:t-old-derived", 200, 1),
        ];
        sort_recent_first(&mut rows);
        assert_eq!(
            ids(&rows),
            [
                "c:t-new-derived",
                "d:new-explicit",
                "c:t-old-derived",
                "d:old-explicit"
            ]
        );
    }

    #[test]
    fn workspace_경계를_넘어_전역으로_정렬된다() {
        let mut rows = vec![
            row("c:t-a", 100, 1),
            row("c:t-b", 400, 1),
            row("c:t-c", 200, 2),
            row("c:t-d", 300, 2),
        ];
        sort_recent_first(&mut rows);
        assert_eq!(
            rows.iter()
                .map(|r| (r.workspace_id, r.updated_at))
                .collect::<Vec<_>>(),
            [(1, 400), (2, 300), (2, 200), (1, 100)]
        );
    }

    /// `ROLLUP_ALL` 순서에 맞춘 on/off 배열을 만든다.
    fn filter(on: &[DagStatus]) -> [bool; DagStatus::ROLLUP_ALL.len()] {
        let mut f = [false; DagStatus::ROLLUP_ALL.len()];
        for (i, s) in DagStatus::ROLLUP_ALL.iter().enumerate() {
            f[i] = on.contains(s);
        }
        f
    }

    #[test]
    fn 아무것도_안_켜면_전체_통과() {
        let none = filter(&[]);
        for s in DagStatus::ROLLUP_ALL {
            assert!(status_matches(&none, s), "{s:?} 가 걸러졌다");
        }
    }

    #[test]
    fn 하나만_켜면_그것만_통과() {
        let only_running = filter(&[DagStatus::Running]);
        assert!(status_matches(&only_running, DagStatus::Running));
        for s in DagStatus::ROLLUP_ALL {
            if s != DagStatus::Running {
                assert!(!status_matches(&only_running, s), "{s:?} 가 통과했다");
            }
        }
    }

    #[test]
    fn 여러_개를_켜면_or_로_통과() {
        let unfinished = filter(&[DagStatus::Waiting, DagStatus::Ready, DagStatus::Running]);
        for s in [DagStatus::Waiting, DagStatus::Ready, DagStatus::Running] {
            assert!(status_matches(&unfinished, s), "{s:?} 가 걸러졌다");
        }
        for s in [DagStatus::Succeeded, DagStatus::Failed, DagStatus::Skipped] {
            assert!(!status_matches(&unfinished, s), "{s:?} 가 통과했다");
        }
    }

    /// `rollup()` 은 카운터를 `> 0` / `== 0` 와 skipped·not_selected, failed·recovered,
    /// waiting·blocked 의 대소로만 보므로, 8 개 카운터와 not_selected(skipped 의 부분)·
    /// recovered(failed 의 부분)·blocked(waiting 의 부분)·succeeded_ends 를 각각 0/1 로 둔 4096
    /// 조합이 도달 가능한 분기를 **전부** 훑는다.
    fn all_rollup_outputs() -> std::collections::BTreeSet<&'static str> {
        let mut out = std::collections::BTreeSet::new();
        for bits in 0u32..4096 {
            let c = tasty_agent::DagStateCounts {
                waiting: usize::from(bits & 1 != 0) + usize::from(bits & 1024 != 0),
                ready: usize::from(bits & 2 != 0),
                running: usize::from(bits & 4 != 0),
                succeeded: usize::from(bits & 8 != 0),
                failed: usize::from(bits & 16 != 0) + usize::from(bits & 512 != 0),
                cancelled: usize::from(bits & 32 != 0),
                skipped: usize::from(bits & 64 != 0) + usize::from(bits & 256 != 0),
                unknown: usize::from(bits & 128 != 0),
                not_selected: usize::from(bits & 256 != 0),
                recovered: usize::from(bits & 512 != 0),
                blocked: usize::from(bits & 1024 != 0),
                succeeded_ends: usize::from(bits & 2048 != 0),
                awaiting_input: 0,
            };
            out.insert(c.rollup());
        }
        out
    }

    /// 부분 실패는 요약 전용 상태다. task 상태로 읽히지 않고 자기 글리프·라벨을 가진다.
    #[test]
    fn partially_failed_has_its_own_display_and_is_not_a_task_state() {
        assert!(all_rollup_outputs().contains("partially_failed"));
        let s = DagStatus::from_name("partially_failed");
        assert_eq!(s, DagStatus::PartiallyFailed);
        assert!(!DagStatus::ALL.contains(&s));
        assert_eq!(s.glyph(), "\u{25D2}");
        assert_ne!(s.label(), DagStatus::Unknown.label());
    }

    /// rollup이 반환하는 상태는 모두 필터에 있어야 한다.
    #[test]
    fn rollup_이_내는_값은_전부_필터_목록에_있다() {
        for name in all_rollup_outputs() {
            let s = DagStatus::from_name(name);
            assert!(
                DagStatus::ROLLUP_ALL.contains(&s),
                "rollup 이 내는 '{name}' 이 ROLLUP_ALL 에 없다"
            );
        }
    }

    /// 필터의 모든 상태는 실제 rollup 결과로 나올 수 있어야 한다.
    #[test]
    fn 필터_목록의_각_항목은_rollup_이_실제로_낸다() {
        let produced: Vec<DagStatus> = all_rollup_outputs()
            .into_iter()
            .map(DagStatus::from_name)
            .collect();
        for s in DagStatus::ROLLUP_ALL {
            assert!(
                produced.contains(&s),
                "{s:?} 는 rollup 이 내지 않는 죽은 선택지다"
            );
        }
    }
}
