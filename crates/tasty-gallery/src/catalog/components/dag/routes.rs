//! 전이 엣지의 선택 상태 네 가지와, 경로가 선택되지 않아 건너뛴 노드를 보인다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use super::edges::{self, EdgeLook};
use super::node::{NodeVis, node_box, sample};
use super::{Kind, Rel, Skip, Status};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 전이 엣지의 선택 상태. 모양(색·파선)은 같고 굵기와 불투명도만 다르다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Pending,
    Selected,
    NotSelected,
    Unavailable,
}

impl Selection {
    pub const ALL: [Selection; 4] = [
        Selection::Pending,
        Selection::Selected,
        Selection::NotSelected,
        Selection::Unavailable,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Selection::Pending => "pending",
            Selection::Selected => "selected",
            Selection::NotSelected => "not_selected",
            Selection::Unavailable => "unavailable",
        }
    }

    /// 선 굵기. 선택된 경로만 `dag-edge-selected-width` 로 굵게 그린다.
    pub fn width(self, theme: &Theme) -> LogicalPx {
        match self {
            Selection::Selected => theme.dag_edge_selected_width(),
            _ => theme.dag_edge_width(),
        }
    }

    /// 선 잉크. 선택되지 않은 경로와 쓸 수 없는 경로는 죽은 경로와 같은 `dag-edge-dim` 이다.
    pub fn ink(self, theme: &Theme) -> egui::Color32 {
        match self {
            Selection::Pending | Selection::Selected => theme.dag_edge_transition(),
            Selection::NotSelected | Selection::Unavailable => theme.dag_edge_dim(),
        }
        .to_egui()
    }
}

fn route_node(
    status: Status,
    name: &str,
    kind: Kind,
    dur: Option<&str>,
    skip: Option<Skip>,
) -> super::Node {
    let mut n = sample(status, name, dur);
    n.kind = kind;
    n.skip = skip;
    n
}

/// `routes` 섹션 Spec — 전이 선택 상태와 미선택 노드.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for sel in Selection::ALL {
            let stroke = EdgeLook {
                color: Rel::Transition.color(theme).to_egui(),
                ink: sel.ink(theme),
                dash: Rel::Transition.dash(),
                width: sel.width(theme),
            };
            edges::specimen(ui, theme, stroke, sel.key(), None);
        }
    });
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        let cards = [
            (
                route_node(
                    Status::Succeeded,
                    "route:pick",
                    Kind::Custom,
                    Some("1s"),
                    None,
                ),
                "producer \u{b7} succeeded",
            ),
            (
                route_node(
                    Status::Running,
                    "deploy:canary",
                    Kind::Run,
                    Some("12s"),
                    None,
                ),
                "branch selected",
            ),
            (
                route_node(
                    Status::Skipped,
                    "deploy:full",
                    Kind::Run,
                    None,
                    Some(Skip::BranchNotSelected),
                ),
                "skipped \u{2014} branch not selected",
            ),
            (
                route_node(
                    Status::Skipped,
                    "notify:ops",
                    Kind::Custom,
                    None,
                    Some(Skip::UpstreamUnavailable {
                        source: "deploy:canary".into(),
                        state: "failed".into(),
                    }),
                ),
                "skipped \u{2014} upstream unavailable",
            ),
        ];
        for (node, caption) in &cards {
            node_box(
                ui,
                theme,
                node,
                NodeVis {
                    dimmed: node.status.is_dim(),
                    ..NodeVis::default()
                },
                caption,
            );
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("pending", "1px \u{b7} full opacity"),
            ("selected", "2px \u{b7} dag-edge-selected-width"),
            ("not_selected", "1px \u{b7} dag-edge-dim \u{b7} long dash"),
            (
                "unavailable",
                "1px \u{b7} dag-edge-dim, as from a failed task",
            ),
            (
                "not-selected node",
                "skipped look \u{b7} label NOT SELECTED \u{b7} tooltip = kit nodeTitle(): \u{201c}{name} \u{2014} not selected\u{201d} + \u{201c}Why: Not selected by the upstream result\u{201d}",
            ),
            (
                "upstream-unavailable node",
                "skipped look \u{b7} label SKIPPED \u{b7} tooltip = kit nodeTitle(): \u{201c}{name} \u{2014} skipped\u{201d} + \u{201c}Why: An upstream task did not succeed\u{201d}",
            ),
            ("rollup", "succeeded + not selected only \u{2192} succeeded"),
            (
                "DAG list",
                "no new filter; the skipped count reads \u{201c}{n} skipped ({k} not selected)\u{201d}",
            ),
        ],
        &[
            TokenChip::new(
                "dag-edge-transition",
                "route",
                theme.dag_edge_transition().to_egui(),
            ),
            TokenChip::without_color("dag-edge-selected-width", "\u{2192} focus-ring-width 2"),
            TokenChip::new(
                "dag-edge-dim",
                "not selected / unavailable",
                theme.dag_edge_dim().to_egui(),
            ),
            TokenChip::new(
                "dag-status-skipped",
                "both skip reasons",
                theme.dag_status_skipped().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "A branch that was not taken is a normal outcome, so it gets no new colour: it shares \
         the skipped card and differs only in its spelled label and tooltip.",
    );
}
