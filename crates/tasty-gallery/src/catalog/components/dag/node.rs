//! 상태 바, 작업 이름, 상태 라벨, 소요 시간을 표시하는 DAG 노드 예제.
//! 카드는 본체와 같은 공통 DAG 카드(`tasty_ui_widgets::paint_dag_card`)로 그린다.
//! 표시 상세도는 카드의 내용만 바꾸며 카드 크기는 유지한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{DagCardView, GraphNodeVisual, graph_paint_selection_ring, paint_dag_card};

use super::{Kind, Node, STATUS_ORDER, Status};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 줌 배율이 정하는 카드 내용 티어 — 본체와 같은 공통 그래프 뷰의 단계.
pub use tasty_ui_widgets::GraphLod as Lod;

/// 카드 한 장의 그리기 상태.
#[derive(Debug, Clone, Copy)]
pub struct NodeVis {
    pub lod: Lod,
    /// 캔버스 배율. 박스와 폰트·패딩이 함께 스케일된다(CSS `transform: scale`).
    pub zoom: f32,
    pub selected: bool,
    pub hovered: bool,
    pub dimmed: bool,
}

impl Default for NodeVis {
    fn default() -> Self {
        Self {
            lod: Lod::Full,
            zoom: 1.0,
            selected: false,
            hovered: false,
            dimmed: false,
        }
    }
}

impl NodeVis {
    /// 공통 그래프 뷰의 카드 상태.
    pub fn visual(self) -> GraphNodeVisual {
        GraphNodeVisual {
            lod: self.lod,
            zoom: self.zoom,
            selected: self.selected,
            hovered: self.hovered,
            dimmed: self.dimmed,
        }
    }
}

/// 공통 DAG 카드에 넘길 내용. 갤러리 표본의 라벨은 카드처럼 대문자로 쓴다.
pub fn with_card<R>(node: &Node, f: impl FnOnce(&DagCardView<'_>) -> R) -> R {
    let label = node.status_label().to_uppercase();
    let card = DagCardView {
        name: &node.name,
        status: node.status.card(),
        awaiting: node.glyph() == "!",
        command_kind: node.kind.key(),
        glyph: node.glyph(),
        status_label: &label,
        duration: node.dur.as_deref(),
        in_cycle: false,
    };
    f(&card)
}

/// 카드 한 장. `rect` 는 이미 화면 좌표로 변환된 박스다. 선택 링도 함께 그린다.
pub fn paint_card(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, node: &Node, vis: NodeVis) {
    let painter = ui.painter().clone();
    with_card(node, |card| {
        paint_dag_card(ui, &painter, theme, rect, card, &vis.visual());
    });
    if vis.selected {
        graph_paint_selection_ring(&painter, theme, rect, vis.zoom);
    }
}

pub(super) fn row_height(ui: &egui::Ui, font: &egui::FontId) -> f32 {
    ui.fonts(|f| f.row_height(font))
}

pub(super) fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    tasty_ui_widgets::graph_text_width(ui.painter(), text, font)
}

/// 폭에 맞춰 말줄임. char 경계를 지킨다 — 바이트로 자르면 한글 이름에서 패닉한다.
pub(super) fn ellipsize(ui: &egui::Ui, text: &str, font: &egui::FontId, max_w: f32) -> String {
    tasty_ui_widgets::graph_ellipsize(ui.painter(), text, font, max_w)
}

/// 줄바꿈 배치가 크기를 알 수 있도록 중첩 Ui 대신 한 번에 카드 영역을 예약한다.
pub(super) fn node_box(ui: &mut egui::Ui, theme: &Theme, node: &Node, vis: NodeVis, caption: &str) {
    let w = theme.dag_node_width().value();
    let h = theme.dag_node_height().value();
    let gap = theme.spacing_sm.value();
    let cap_font = tasty_ui_widgets::label_font(theme.font_size_micro.value());
    let cap_h = row_height(ui, &cap_font);
    // 선택 링은 카드 바깥에 그려지므로 그만큼 자리를 미리 비워 둔다.
    let pad = if vis.selected {
        theme.dag_node_selected_ring_width().value() + theme.border_width.value()
    } else {
        0.0
    };
    let (outer, response) = ui.allocate_exact_size(
        egui::vec2(w + pad * 2.0, h + pad * 2.0 + gap + cap_h),
        egui::Sense::hover(),
    );
    response.on_hover_text(node.hover_text());
    let rect = egui::Rect::from_min_size(outer.min + egui::vec2(pad, pad), egui::vec2(w, h));
    paint_card(ui, theme, rect, node, vis);
    ui.painter().text(
        egui::pos2(outer.min.x, rect.max.y + pad + gap),
        egui::Align2::LEFT_TOP,
        caption,
        cap_font,
        theme.text_muted().to_egui(),
    );
}

pub(super) fn sample(status: Status, name: &str, dur: Option<&str>) -> Node {
    let mut n = super::Node {
        id: "spec".into(),
        name: name.into(),
        kind: Kind::Run,
        status,
        dur: dur.map(str::to_owned),
        started: Some("10:31:29".into()),
        exit: None,
        cmd: "cargo build".into(),
        err: None,
        deps: Vec::new(),
        skip: None,
        phase: None,
        reason: None,
    };
    n.kind = Kind::Run;
    n
}

/// `node` 섹션 Spec 1 — 8 상태 전부.
pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for s in STATUS_ORDER {
            let dur = if matches!(s, Status::Waiting | Status::Ready) {
                None
            } else {
                Some("12s")
            };
            let node = sample(s, &format!("task:{}", s.key()), dur);
            node_box(
                ui,
                theme,
                &node,
                NodeVis {
                    dimmed: s.is_dim(),
                    ..NodeVis::default()
                },
                s.key(),
            );
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("card", "168 × 48 · radius 4"),
            ("padding", "4 / 8 · row gap 4"),
            ("status bar", "3px, full height"),
            ("name", "13 medium, 1 line, ellipsis"),
            ("meta row", "10 caps · duration mono, right"),
            ("dimmed", "skipped + cancelled at 0.4"),
        ],
        &STATUS_ORDER
            .iter()
            .map(|s| {
                TokenChip::new(
                    match s {
                        Status::Waiting => "dag-status-waiting",
                        Status::Ready => "dag-status-ready",
                        Status::Running => "dag-status-running",
                        Status::Succeeded => "dag-status-succeeded",
                        Status::Failed => "dag-status-failed",
                        Status::Cancelled => "dag-status-cancelled",
                        Status::Skipped => "dag-status-skipped",
                        Status::Unknown => "dag-status-unknown",
                        Status::PartiallyFailed => "dag-status-partially-failed",
                    },
                    s.key(),
                    s.accent(theme).to_egui(),
                )
            })
            .collect::<Vec<_>>(),
    );
    spec::note(
        ui,
        theme,
        "Colour vs. text: the state colour is the recede channel — bar, border, glyph. \
         The spelled label reads from its own dag-status-<state>-label role: waiting takes \
         text-muted, and the two dimmed states take text-secondary, which still measures \
         4.74:1 after the 0.75 card dim.",
    );
    spec::note(
        ui,
        theme,
        "Truncation rule (i18n): the task name is a single line with ellipsis at the card edge — \
         never wraps, never shrinks the font. On the meta row the status label yields first: it \
         ellipses before the duration is dropped, so a long state string can't push the timing out.",
    );
}

/// `node` 섹션 Spec 2 — task 종류 5 종.
pub fn draw_kinds(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for k in [
            Kind::Run,
            Kind::Custom,
            Kind::Reduce,
            Kind::WaitBarrier,
            Kind::Agent,
        ] {
            let mut node = sample(
                Status::Succeeded,
                &format!("{}:step", k.label()),
                Some("4s"),
            );
            node.kind = k;
            node_box(ui, theme, &node, NodeVis::default(), k.key());
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("glyph", "14px, muted, leading"),
            ("run", "terminal"),
            ("custom", "plug"),
            ("reduce", "layers"),
            ("wait_barrier", "lock"),
            (
                "agent",
                "agent · stroke = icon-stroke-width like the rest · detail kind label “Agent”",
            ),
        ],
        &[TokenChip::new(
            "dag-node-meta-fg",
            "kind glyph",
            theme.dag_node_meta_fg().to_egui(),
        )],
    );
    spec::note(
        ui,
        theme,
        "agent is a speech bubble carrying the prompt chevron — one Claude or Codex turn; the two \
         providers share it and the provider stays a detail-panel field. terminal, plug, layers and \
         lock carry the other four. A dedicated barrier glyph would read better than lock if one is \
         ever drawn.",
    );
}

/// `node` 섹션 Spec 3 — LOD 3 티어 · 선택 · 오버플로.
pub fn draw_lod(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (lod, caption) in [
            (Lod::Full, "full ≥ 70%"),
            (Lod::Compact, "compact ≥ 40%"),
            (Lod::Block, "block < 40%"),
        ] {
            let node = sample(Status::Running, "build:linux-x86_64", Some("12s"));
            node_box(
                ui,
                theme,
                &node,
                NodeVis {
                    lod,
                    ..NodeVis::default()
                },
                caption,
            );
        }
        let long = sample(
            Status::Failed,
            "build:linux-x86_64-with-a-very-long-target-triple",
            Some("12s"),
        );
        node_box(ui, theme, &long, NodeVis::default(), "overflow → ellipsis");
        let sel = sample(Status::Succeeded, "build:linux-x86_64", Some("12s"));
        node_box(
            ui,
            theme,
            &sel,
            NodeVis {
                selected: true,
                ..NodeVis::default()
            },
            "selected (2px ring)",
        );
        let hot = sample(Status::Running, "build:linux-x86_64", Some("12s"));
        node_box(
            ui,
            theme,
            &hot,
            NodeVis {
                hovered: true,
                ..NodeVis::default()
            },
            "hover wash",
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            ("full", "zoom ≥ 0.7"),
            ("compact", "0.4 – 0.7"),
            ("block", "< 0.4"),
            ("selection", "2px accent outline, 1px offset"),
            ("hover", "derived overlay-hover on the body"),
        ],
        &[
            TokenChip::new(
                "dag-node-selected-ring",
                "selection",
                theme.dag_node_selected_ring().to_egui(),
            ),
            TokenChip::without_color("dag-node-hover-bg", "hover wash"),
            TokenChip::without_color("dag-node-dim-opacity", "skipped path"),
        ],
    );
    super::canvas::dense_stage(ui, theme);
    spec::note(
        ui,
        theme,
        "55 nodes, auto-fitted on open — the canvas lands in the compact or block tier and says so \
         in the bottom-left hint chip, so a user who sees colour blocks knows why and knows the way back.",
    );
}
