//! DAG 목록 행의 출처 태그, 요약 상태, 완료 작업 수 예제.

use tasty_icons as icons;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{ListCtrl, ListCtrlItem, TagVariant, tag};

use super::{Graph, Node, Rel, Skip, Status};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 목록 한 줄이 표현하는 DAG — 그래프 + 집계.
pub struct Entry {
    pub graph: Graph,
    /// 사용자가 `metadata.dag` 로 선언한 그룹이 아니라 의존 연결성에서 도출된 것.
    pub derived: bool,
    /// 사람의 입력을 기다리는 task 수(시안 `DAG_LIST.awaiting`). rollup 은 running 그대로다.
    pub awaiting: usize,
}

impl Entry {
    /// 시안 `dagRollup` — 진행이 먼저, 그다음 결과다. running → ready → 실행할 수 있는 waiting →
    /// partially_failed | failed → succeeded | skipped.
    pub fn rollup(&self) -> Status {
        let nodes = &self.graph.nodes;
        let not_selected = |n: &Node| matches!(n.skip, Some(Skip::BranchNotSelected));
        // 폴백 task 가 성공해 대신한 실패는 복구된 것으로 본다.
        let recovered = |n: &Node| {
            nodes.iter().any(|f| {
                f.status == Status::Succeeded
                    && f.deps
                        .iter()
                        .any(|(from, r)| *from == n.id && *r == Rel::Fallback)
            })
        };
        let dead = |n: &Node| match n.status {
            Status::Failed => !recovered(n),
            Status::Cancelled | Status::Unknown => true,
            Status::Skipped => !not_selected(n),
            _ => false,
        };
        let can_run = |n: &Node| {
            n.deps.iter().all(|(from, r)| {
                *r == Rel::Fallback || self.graph.node(from).is_none_or(|f| !dead(f))
            })
        };
        let has = |s: Status| nodes.iter().any(|n| n.status == s);
        if has(Status::Running) {
            return Status::Running;
        }
        if has(Status::Ready) {
            return Status::Ready;
        }
        if nodes
            .iter()
            .any(|n| n.status == Status::Waiting && can_run(n))
        {
            return Status::Waiting;
        }
        // 끝 task = 하류가 없는 task. 경로가 선택되지 않은 skip 은 하류로 세지 않는다.
        let has_downstream = |id: &str| {
            nodes.iter().any(|n| {
                !not_selected(n)
                    && n.deps
                        .iter()
                        .any(|(from, r)| from == id && *r != Rel::Fallback)
            })
        };
        if nodes
            .iter()
            .any(|n| n.status == Status::Failed && !recovered(n))
        {
            let end_ok = nodes.iter().any(|n| {
                !has_downstream(&n.id)
                    && (n.status == Status::Succeeded
                        || (n.status == Status::Failed && recovered(n)))
            });
            return if end_ok {
                Status::PartiallyFailed
            } else {
                Status::Failed
            };
        }
        if has(Status::Succeeded) {
            Status::Succeeded
        } else {
            Status::Skipped
        }
    }

    /// 성공·실패·취소·건너뜀을 포함한 종료 작업 수.
    pub fn done(&self) -> usize {
        super::done_count(&self.graph.nodes)
    }

    pub fn total(&self) -> usize {
        self.graph.nodes.len()
    }
}

/// 디자인 `DAG_LIST` — 이 갤러리가 가진 그래프 4 개를 목록 항목으로 접은 것.
pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            graph: super::build_dag(),
            derived: false,
            awaiting: 0,
        },
        Entry {
            graph: super::index_dag(),
            derived: false,
            awaiting: 1,
        },
        Entry {
            graph: super::dense_dag(),
            derived: true,
            awaiting: 0,
        },
        Entry {
            graph: super::cycle_dag(),
            derived: true,
            awaiting: 0,
        },
        Entry {
            graph: super::partial_dag(),
            derived: false,
            awaiting: 0,
        },
    ]
}

/// 행 끝 클러스터 — 태그 · 상태 · 카운터.
pub fn trailing(ui: &mut egui::Ui, theme: &Theme, entry: &Entry) {
    // `ListCtrl` 의 trailing 슬롯은 오른쪽에서 왼쪽으로 채워진다 — 시안의
    // 왼→오른 순서(태그 · 상태 · 카운터)를 얻으려면 거꾸로 낸다.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.dag_row_summary_gap().value();
        ui.label(
            egui::RichText::new(format!(
                "{}/{}{}",
                entry.done(),
                entry.total(),
                super::skip_count_suffix(&entry.graph.nodes)
            ))
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.dag_row_count_font_size().value())
            .color(theme.dag_row_count_fg().to_egui()),
        );
        let status = entry.rollup();
        ui.label(
            egui::RichText::new(format!("{} {}", status.glyph(), status.label()))
                .family(tasty_ui_widgets::label_font_family())
                .size(theme.font_size_caption.value())
                // 작은 라벨에 맞는 상태별 텍스트 색을 쓴다.
                .color(status.label_fg(theme).to_egui()),
        );
        if entry.awaiting > 0 {
            ui.label(
                egui::RichText::new(format!("! {} needs input", entry.awaiting))
                    .family(tasty_ui_widgets::label_font_family())
                    .size(theme.font_size_caption.value())
                    .color(theme.dag_phase_awaiting_label().to_egui()),
            );
        }
        if entry.derived {
            tag(ui, theme, "derived", TagVariant::Default, false);
        }
    });
}

/// 목록 본문 — 주어진 폭 안에 행을 쌓는다. popup specimen 이 그대로 재사용한다.
/// 클릭된 행 인덱스를 돌려준다.
pub fn list(
    ui: &mut egui::Ui,
    theme: &Theme,
    entries: &[Entry],
    width: f32,
    salt: &str,
) -> Option<usize> {
    let metas: Vec<String> = entries
        .iter()
        .map(|e| format!("{} \u{b7} {}", e.graph.workspace, e.graph.updated))
        .collect();
    let icon = |ui: &mut egui::Ui, rect: egui::Rect, color: egui::Color32| {
        icons::GIT_TREE
            .image(rect.height(), color)
            .paint_at(ui, rect);
    };
    type Trailing<'a> = Box<dyn Fn(&mut egui::Ui, &Theme) + 'a>;
    let trailings: Vec<Trailing<'_>> = entries
        .iter()
        .map(|e| -> Trailing<'_> { Box::new(move |ui, theme| trailing(ui, theme, e)) })
        .collect();
    let items: Vec<ListCtrlItem<'_>> = entries
        .iter()
        .zip(&metas)
        .zip(&trailings)
        .map(|((e, meta), tr)| {
            ListCtrlItem::new(&e.graph.name)
                .description(meta)
                .icon(&icon)
                .trailing(&**tr)
        })
        .collect();
    let mut hit = None;
    ui.push_id(salt, |ui| {
        hit = ListCtrl::new()
            .width(width)
            .show(ui, theme, &items, None)
            .clicked;
    });
    hit
}

/// `rows` 섹션 Spec — 목록 행 4 종.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let entries = entries();
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        let w = ui.available_width();
        list(ui, theme, &entries, w, "dag_rows_spec");
    });
    spec::meta(
        ui,
        theme,
        &[
            ("row height", "36 · listctrl-row-min-height"),
            ("leading", "gitTree icon + name"),
            ("label", "13 — DAG name"),
            ("description", "11 — workspace · last update"),
            (
                "trailing",
                "origin tag · ! n needs input · rollup · done/total",
            ),
            (
                "skip count",
                "after done/total: · {n} skipped ({k} not selected) — only when n > 0, parenthesis only when k > 0 · same mono caption · dag-row-count-fg · not clickable · also on the DAG tab header",
            ),
            ("counter", "mono, never a progress bar"),
            ("scope", "all workspaces, filterable"),
        ],
        &[
            TokenChip::new(
                "dag-row-count-fg",
                "done/total",
                theme.dag_row_count_fg().to_egui(),
            ),
            TokenChip::without_color("listctrl-row-bg-hover", "hover"),
            TokenChip::without_color("dag-row-summary-gap", "trailing gap"),
            TokenChip::new("tag-bg", "derived tag", theme.tag_bg().to_egui()),
            TokenChip::without_color("dag-row-height", "36 row"),
            TokenChip::new(
                "dag-phase-awaiting-label",
                "! n needs input",
                theme.dag_phase_awaiting_label().to_egui(),
            ),
            TokenChip::new(
                "dag-status-partially-failed",
                "◒ rollup",
                theme.dag_status_partially_failed().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "The workspace name is part of the description line, not a separate column — the \
         popup lists every workspace, and a column would waste the width the DAG name needs.",
    );
    spec::note(
        ui,
        theme,
        "Rollup: seven values, in filter order: waiting · ready · running · succeeded · \
         partially_failed · failed · skipped. partially_failed (◒, peach \
         dag-status-partially-failed) = no more progress, an unrecovered failure, and at least \
         one branch succeeded to its end. Precedence is progress first: running → ready → \
         waiting-that-can-run → partially_failed | failed → succeeded | skipped. A failure is \
         recovered when its fallback succeeded; an end task has no downstream task, and a \
         not-selected skip does not count as downstream. done/total counts failed tasks as done. \
         The gallery folds each sample graph with the same rule; docs-publish is partially_failed.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollup_of(graph: Graph) -> Status {
        Entry {
            graph,
            derived: false,
            awaiting: 0,
        }
        .rollup()
    }

    /// 한 갈래가 끝까지 성공하고 다른 갈래가 실패하면 부분 실패다.
    #[test]
    fn a_failure_next_to_a_finished_branch_is_partially_failed() {
        assert_eq!(
            rollup_of(super::super::partial_dag()),
            Status::PartiallyFailed
        );
    }

    /// 끝까지 성공한 갈래가 없으면 실패다.
    #[test]
    fn a_failure_without_a_finished_branch_is_failed() {
        let mut graph = super::super::partial_dag();
        graph
            .nodes
            .retain(|n| !matches!(n.id.as_str(), "docs" | "upload"));
        assert_eq!(rollup_of(graph), Status::Failed);
    }

    /// 진행 중인 task 가 있으면 결과보다 진행이 먼저다.
    #[test]
    fn progress_comes_before_the_outcome() {
        assert_eq!(rollup_of(super::super::index_dag()), Status::Running);
    }
}
