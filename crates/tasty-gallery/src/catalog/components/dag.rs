//! DAG 화면 예제. 캔버스·카드·미니맵·줌 버튼·상세 자리는 본체와 같은 공통 그래프 뷰
//! (`tasty_ui_widgets::draw_graph_canvas`, `paint_dag_card`)를 부르고, 여기에는 표본 데이터와
//! 그 데이터를 공통 뷰에 넘기는 painter가 있다.
//! 본체 바이너리에만 있는 DAG 헤더·러너 배지·사이클 배너·상세 내용·간선 잉크 규칙은 본체를 부를 수
//! 없어 같은 모양으로 다시 그린다.
//! 비례 글꼴에 없는 Dingbats 문자 대신 본체와 같은 기하 문자로 상태를 표시한다.

pub mod canvas;
pub mod chrome;
pub mod detail;
pub mod edges;
pub mod node;
pub mod phase;
pub mod routes;
pub mod rows;
pub mod runner;
pub mod states;
pub mod surface;
pub mod window;

use tasty_dag_layout::{GraphLayout, LayoutConfig, Orientation, layout_dag};
use tasty_icons::Icon;
use tasty_type_appearance::color::HexColor;
use tasty_type_appearance::theme::Theme;

/// 실행 상태 8 종 (`DAG_STATUS` / `DAG_STATUS_ORDER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Ready,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
    Unknown,
    /// DAG 요약 전용 — task 하나의 상태로는 나오지 않는다(`DAG_STATUS.partially_failed`).
    PartiallyFailed,
}

/// 디자인 `DAG_STATUS_ORDER` 와 같은 순서.
pub const STATUS_ORDER: [Status; 8] = [
    Status::Waiting,
    Status::Ready,
    Status::Running,
    Status::Succeeded,
    Status::Failed,
    Status::Cancelled,
    Status::Skipped,
    Status::Unknown,
];

/// DAG 요약 상태 일곱 가지 — 시안 `DAG_ROLLUP_ORDER` 와 같은 필터 순서.
/// 노드 전체 상태는 STATUS_ORDER를 사용한다.
pub const ROLLUP_ORDER: [Status; 7] = [
    Status::Waiting,
    Status::Ready,
    Status::Running,
    Status::Succeeded,
    Status::PartiallyFailed,
    Status::Failed,
    Status::Skipped,
];

impl Status {
    /// 토큰 접미사 (`--tasty-dag-status-<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Status::Waiting => "waiting",
            Status::Ready => "ready",
            Status::Running => "running",
            Status::Succeeded => "succeeded",
            Status::Failed => "failed",
            Status::Cancelled => "cancelled",
            Status::Skipped => "skipped",
            Status::Unknown => "unknown",
            Status::PartiallyFailed => "partially_failed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Waiting => "Waiting",
            Status::Ready => "Ready",
            Status::Running => "Running",
            Status::Succeeded => "Succeeded",
            Status::Failed => "Failed",
            Status::Cancelled => "Cancelled",
            Status::Skipped => "Skipped",
            Status::Unknown => "Unknown",
            Status::PartiallyFailed => "Partially failed",
        }
    }

    /// 색과 함께 상태를 구분하는 기하 문자.
    pub fn glyph(self) -> &'static str {
        match self {
            Status::Waiting => "\u{25E6}",   // ◦
            Status::Ready => "\u{25B7}",     // ▷
            Status::Running => "\u{25D1}",   // ◑
            Status::Succeeded => "\u{25CF}", // ●
            Status::Failed => "\u{00D7}",    // ×
            Status::Cancelled => "\u{2212}", // −
            Status::Skipped => "\u{2298}",   // ⊘
            Status::Unknown => "?",
            Status::PartiallyFailed => "\u{25D2}", // ◒ 아래 반 원
        }
    }

    /// 상태 accent — 좌측 바 · 미니맵 · (대개) 카드 보더.
    pub fn accent(self, theme: &Theme) -> HexColor {
        match self {
            Status::Waiting => theme.dag_status_waiting(),
            Status::Ready => theme.dag_status_ready(),
            Status::Running => theme.dag_status_running(),
            Status::Succeeded => theme.dag_status_succeeded(),
            Status::Failed => theme.dag_status_failed(),
            Status::Cancelled => theme.dag_status_cancelled(),
            Status::Skipped => theme.dag_status_skipped(),
            Status::Unknown => theme.dag_status_unknown(),
            Status::PartiallyFailed => theme.dag_status_partially_failed(),
        }
    }

    /// 카드 바탕 (`sBg`).
    pub fn bg(self, theme: &Theme) -> HexColor {
        match self {
            Status::Waiting => theme.dag_status_waiting_bg(),
            Status::Ready => theme.dag_status_ready_bg(),
            Status::Running => theme.dag_status_running_bg(),
            Status::Succeeded => theme.dag_status_succeeded_bg(),
            Status::Failed => theme.dag_status_failed_bg(),
            Status::Cancelled => theme.dag_status_cancelled_bg(),
            Status::Skipped => theme.dag_status_skipped_bg(),
            Status::Unknown => theme.dag_status_unknown_bg(),
            Status::PartiallyFailed => theme.dag_status_partially_failed_bg(),
        }
    }

    /// 철자 라벨 톤 (`sLabel`) — 10px 에서도 읽히도록 상태별로 따로 잡힌 역할.
    pub fn label_fg(self, theme: &Theme) -> HexColor {
        match self {
            Status::Waiting => theme.dag_status_waiting_label(),
            Status::Ready => theme.dag_status_ready_label(),
            Status::Running => theme.dag_status_running_label(),
            Status::Succeeded => theme.dag_status_succeeded_label(),
            Status::Failed => theme.dag_status_failed_label(),
            Status::Cancelled => theme.dag_status_cancelled_label(),
            Status::Skipped => theme.dag_status_skipped_label(),
            Status::Unknown => theme.dag_status_unknown_label(),
            Status::PartiallyFailed => theme.dag_status_partially_failed_label(),
        }
    }

    /// 카드 보더. 디자인은 waiting / cancelled / skipped 만 중립 보더를 쓰고
    /// 나머지는 상태 accent 를 그대로 두른다.
    pub fn border(self, theme: &Theme) -> HexColor {
        if matches!(self, Status::Waiting | Status::Cancelled | Status::Skipped) {
            theme.dag_node_border()
        } else {
            self.accent(theme)
        }
    }

    /// 실패/취소 상류에 막혀 실행되지 않을 경로 (`DIM_STATUS`).
    pub fn is_dim(self) -> bool {
        matches!(self, Status::Skipped | Status::Cancelled)
    }

    /// 공통 DAG 카드의 상태.
    pub fn card(self) -> tasty_ui_widgets::DagStatus {
        use tasty_ui_widgets::DagStatus as S;
        match self {
            Status::Waiting => S::Waiting,
            Status::Ready => S::Ready,
            Status::Running => S::Running,
            Status::Succeeded => S::Succeeded,
            Status::Failed => S::Failed,
            Status::Cancelled => S::Cancelled,
            Status::Skipped => S::Skipped,
            Status::Unknown => S::Unknown,
            Status::PartiallyFailed => S::PartiallyFailed,
        }
    }
}

/// task 종류 4 종 (`DAG_KIND`) — 이름 행 앞의 글리프로만 구분된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Run,
    Custom,
    Reduce,
    WaitBarrier,
    /// Claude·Codex 한 턴. 두 provider 가 같은 글리프를 쓴다.
    Agent,
}

impl Kind {
    pub fn icon(self) -> Icon {
        match self {
            Kind::Run => tasty_icons::TERMINAL,
            Kind::Custom => tasty_icons::PLUG,
            Kind::Reduce => tasty_icons::LAYERS,
            Kind::WaitBarrier => tasty_icons::LOCK,
            Kind::Agent => tasty_icons::AGENT,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Run => "run",
            Kind::Custom => "custom",
            Kind::Reduce => "reduce",
            Kind::WaitBarrier => "barrier",
            Kind::Agent => "agent",
        }
    }

    /// 명세 키 (`run` / `custom` / `reduce` / `wait_barrier` / `agent`).
    pub fn key(self) -> &'static str {
        match self {
            Kind::WaitBarrier => "wait_barrier",
            other => other.label(),
        }
    }
}

/// 작업 관계 5 종 (`DAG_REL`) — 색과 파선 패턴을 **함께** 써서 구분한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rel {
    DependsOn,
    Fallback,
    Reduce,
    /// 입력 binding — 값이 원본에서 받는 task 로 넘어간다.
    Binding,
    /// 전이 — 원본의 성공 출력으로 고른 경로.
    Transition,
}

impl Rel {
    pub const ALL: [Rel; 5] = [
        Rel::DependsOn,
        Rel::Fallback,
        Rel::Reduce,
        Rel::Binding,
        Rel::Transition,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Rel::DependsOn => "depends_on",
            Rel::Fallback => "fallback",
            Rel::Reduce => "reduce",
            Rel::Binding => "binding",
            Rel::Transition => "transition",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Rel::DependsOn => "depends on",
            Rel::Fallback => "fallback",
            Rel::Reduce => "reduce",
            Rel::Binding => "binds input",
            Rel::Transition => "transition",
        }
    }

    pub fn color(self, theme: &Theme) -> HexColor {
        match self {
            Rel::DependsOn => theme.dag_edge_depends(),
            Rel::Fallback => theme.dag_edge_fallback(),
            Rel::Reduce => theme.dag_edge_reduce(),
            Rel::Binding => theme.dag_edge_binding(),
            Rel::Transition => theme.dag_edge_transition(),
        }
    }

    /// `strokeDasharray` — 선·틈 길이를 번갈아 적은 패턴. 실선이면 `None`.
    /// 값은 토큰 `dag-edge-dash-*` 와 같아야 하며 이 파일의 `dash_tokens` 시험이 대조한다.
    pub fn dash(self) -> Option<&'static [f32]> {
        match self {
            Rel::DependsOn => None,
            Rel::Fallback => Some(&[6.0, 3.0]),
            Rel::Reduce => Some(&[2.0, 3.0]),
            Rel::Binding => Some(&[8.0, 2.0, 2.0, 2.0]),
            Rel::Transition => Some(&[10.0, 4.0]),
        }
    }
}

/// 건너뛴 task 의 이유. 카드 모양은 같고 라벨과 툴팁만 다르다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// 다른 경로가 선택되어 실행하지 않았다.
    BranchNotSelected,
    /// 선행 task 의 결과를 쓸 수 없어 건너뛰었다.
    UpstreamUnavailable { source: String, state: String },
}

/// 완료/전체 뒤의 건너뜀 수 — 본체 `skip_count_suffix` 와 같은 규칙.
/// 건너뛴 task 가 있을 때만 붙고, 괄호는 미선택이 있을 때만 붙는다.
pub fn skip_count_suffix(nodes: &[Node]) -> String {
    let skipped: Vec<&Node> = nodes
        .iter()
        .filter(|n| n.status == Status::Skipped)
        .collect();
    if skipped.is_empty() {
        return String::new();
    }
    let not_selected = skipped
        .iter()
        .filter(|n| matches!(n.skip, Some(Skip::BranchNotSelected)))
        .count();
    let mut text = format!(" \u{b7} {} skipped", skipped.len());
    if not_selected > 0 {
        text.push_str(&format!(" ({not_selected} not selected)"));
    }
    text
}

/// 성공·실패·취소·건너뜀을 포함한 종료 작업 수. 목록 행과 surface 머리글이 함께 쓴다.
pub fn done_count(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .filter(|n| {
            matches!(
                n.status,
                Status::Succeeded | Status::Failed | Status::Cancelled | Status::Skipped
            )
        })
        .count()
}

/// 카드 한 장이 표현하는 task.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub status: Status,
    pub dur: Option<String>,
    pub started: Option<String>,
    pub exit: Option<i32>,
    pub cmd: String,
    pub err: Option<String>,
    pub deps: Vec<(String, Rel)>,
    /// `Skipped` 일 때만 의미가 있다.
    pub skip: Option<Skip>,
    /// `Running` 일 때만 의미가 있다. 없으면 `executing`.
    pub phase: Option<phase::Phase>,
    /// `Unknown` 이 된 이유. 호스트의 자유 문구 그대로다.
    pub reason: Option<String>,
}

impl Node {
    fn new(id: &str, name: &str, kind: Kind, status: Status, cmd: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind,
            status,
            dur: None,
            started: None,
            exit: None,
            cmd: cmd.to_owned(),
            err: None,
            deps: Vec::new(),
            skip: None,
            phase: None,
            reason: None,
        }
    }

    fn ran(mut self, dur: &str, started: &str, exit: i32) -> Self {
        self.dur = Some(dur.to_owned());
        self.started = Some(started.to_owned());
        self.exit = Some(exit);
        self
    }

    fn running_for(mut self, dur: &str, started: &str) -> Self {
        self.dur = Some(dur.to_owned());
        self.started = Some(started.to_owned());
        self
    }

    fn dep(mut self, from: &str, rel: Rel) -> Self {
        self.deps.push((from.to_owned(), rel));
        self
    }

    fn err(mut self, tail: &str) -> Self {
        self.err = Some(tail.to_owned());
        self
    }
}

/// 호스트 러너의 실행 여부와 대기·실행 작업 수.
#[derive(Debug, Clone, Copy)]
pub struct Runner {
    pub running: bool,
    pub crashed: bool,
    pub ready: u32,
    pub active: u32,
}

/// DAG 한 개.
#[derive(Debug, Clone)]
pub struct Graph {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub updated: String,
    /// 사이클을 이루는 task id 들. 있으면 캔버스 상단에 배너가 고정된다.
    pub cycle: Option<Vec<String>>,
    pub nodes: Vec<Node>,
    pub runner: Runner,
}

impl Graph {
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// `(from_idx, to_idx, rel)` — 레이어가 증가하는 방향(의존 대상 → 의존하는 쪽).
    pub fn edges(&self) -> Vec<(usize, usize, Rel)> {
        let mut out = Vec::new();
        for (to, n) in self.nodes.iter().enumerate() {
            for (from, rel) in &n.deps {
                if let Some(from) = self.index_of(from) {
                    out.push((from, to, *rel));
                }
            }
        }
        out
    }

    /// 엣지가 죽은 경로인지 — 도착이 skipped/cancelled 이거나 출발이 실패/skipped/cancelled.
    pub fn edge_is_dim(&self, from: usize, to: usize) -> bool {
        let src = self.nodes[from].status;
        let dst = self.nodes[to].status;
        dst.is_dim() || src == Status::Failed || src.is_dim()
    }
}

impl tasty_ui_widgets::GraphNodeData for Node {
    fn node_id(&self) -> &str {
        &self.id
    }
}

/// 공통 그래프 뷰에 넘기는 간선 하나.
#[derive(Debug, Clone, Copy)]
pub struct PlotEdge {
    pub from: usize,
    pub to: usize,
    pub rel: Rel,
}

impl tasty_ui_widgets::GraphEdgeData for PlotEdge {
    fn ends(&self) -> (usize, usize) {
        (self.from, self.to)
    }
}

/// 표본 그래프를 공통 그래프 뷰가 읽는 스냅샷으로 펼친 것.
pub struct Plot<'a> {
    pub graph: &'a Graph,
    edges: Vec<PlotEdge>,
}

impl<'a> Plot<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        let edges = graph
            .edges()
            .into_iter()
            .map(|(from, to, rel)| PlotEdge { from, to, rel })
            .collect();
        Self { graph, edges }
    }
}

impl tasty_ui_widgets::GraphData for Plot<'_> {
    type Node = Node;
    type Edge = PlotEdge;

    fn graph_key(&self) -> &str {
        &self.graph.id
    }

    fn nodes(&self) -> &[Node] {
        &self.graph.nodes
    }

    fn edges(&self) -> &[PlotEdge] {
        &self.edges
    }
}

/// 레이아웃 설정을 토큰에서 만든다 — 본체와 같은 엔진에 사용할 치수를 전달한다.
pub fn layout_config(theme: &Theme, dir: Orientation) -> LayoutConfig {
    LayoutConfig {
        orientation: dir,
        node_size: (theme.dag_node_width(), theme.dag_node_height()),
        layer_gap: theme.dag_layer_gap(),
        sibling_gap: theme.dag_sibling_gap(),
        ..LayoutConfig::default()
    }
}

/// 그래프 좌표 — `tasty-dag-layout` 실엔진.
pub fn layout(graph: &Graph, theme: &Theme, dir: Orientation) -> GraphLayout {
    let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let edges: Vec<(usize, usize)> = graph.edges().iter().map(|(f, t, _)| (*f, *t)).collect();
    layout_dag(&ids, &edges, &layout_config(theme, dir))
}

const ERR_TAIL: &str = "error: linking with `cc` failed: exit status: 1\n  \
= note: /usr/bin/ld: cannot find -lssl: No such file or directory\n          \
/usr/bin/ld: cannot find -lcrypto: No such file or directory\n          \
collect2: error: ld returned 1 exit status\n\n\
error: could not compile `tasty-host` (bin \"tasty-host\") due to 1 previous error\n\
warning: build failed, waiting for other jobs to finish...\n\
make: *** [Makefile:42: release] Error 101";

/// 디자인 `DAG_BUILD` — 12 노드, 8 상태 전부와 3 관계 전부가 한 번씩 나온다.
pub fn build_dag() -> Graph {
    use Kind::*;
    use Rel::*;
    use Status::*;
    Graph {
        id: "build-and-deploy".into(),
        name: "build-and-deploy".into(),
        workspace: "tasty".into(),
        updated: "12s ago".into(),
        cycle: None,
        nodes: vec![
            Node::new(
                "checkout",
                "checkout",
                Run,
                Succeeded,
                "git fetch --depth 1 && git checkout $SHA",
            )
            .ran("1.2s", "10:31:02", 0),
            Node::new(
                "install",
                "deps:install",
                Run,
                Succeeded,
                "cargo fetch --locked",
            )
            .ran("24s", "10:31:04", 0)
            .dep("checkout", DependsOn),
            Node::new(
                "lint",
                "lint:clippy",
                Run,
                Succeeded,
                "cargo clippy -- -D warnings",
            )
            .ran("8s", "10:31:28", 0)
            .dep("install", DependsOn),
            Node::new("unit", "test:unit", Run, Running, "cargo test --lib")
                .running_for("12s", "10:31:28")
                .dep("install", DependsOn),
            Node::new("e2e", "test:e2e", Run, Ready, "cargo test --test e2e")
                .dep("install", DependsOn),
            Node::new(
                "build_linux",
                "build:linux-x86_64",
                Run,
                Failed,
                "cargo build --release --target x86_64-unknown-linux-gnu",
            )
            .ran("41s", "10:31:29", 101)
            .err(ERR_TAIL)
            .dep("install", DependsOn),
            Node::new(
                "build_retry",
                "build:linux (musl fallback)",
                Run,
                Succeeded,
                "cargo build --release --target x86_64-unknown-linux-musl",
            )
            .ran("58s", "10:32:11", 0)
            .dep("build_linux", Fallback),
            Node::new(
                "build_mac",
                "build:macos-arm64",
                Run,
                Waiting,
                "cargo build --release --target aarch64-apple-darwin",
            )
            .dep("unit", DependsOn),
            Node::new(
                "sign",
                "sign:artifacts",
                Custom,
                Skipped,
                "ipc: signer.sign(artifacts)",
            )
            .dep("build_linux", DependsOn),
            Node::new(
                "package",
                "package:release",
                Kind::Reduce,
                Cancelled,
                "reduce: collect(build_retry, build_mac)",
            )
            .dep("build_retry", Rel::Reduce)
            .dep("build_mac", Rel::Reduce),
            Node::new(
                "gate",
                "publish:gate",
                WaitBarrier,
                Waiting,
                "barrier: await approval",
            )
            .dep("package", DependsOn),
            Node::new(
                "notify",
                "notify:agents",
                Custom,
                Unknown,
                "ipc: agents.broadcast(release)",
            )
            .dep("gate", DependsOn),
        ],
        runner: Runner {
            running: true,
            crashed: false,
            ready: 1,
            active: 1,
        },
    }
}

/// 디자인 `DAG_INDEX` — 5 노드 소형 그래프.
pub fn index_dag() -> Graph {
    use Kind::*;
    use Rel::*;
    use Status::*;
    Graph {
        id: "index-refresh".into(),
        name: "index-refresh".into(),
        workspace: "tasty-docs".into(),
        updated: "2m ago".into(),
        cycle: None,
        nodes: vec![
            Node::new("scan", "scan:sources", Run, Succeeded, "rg --files docs/")
                .ran("3s", "10:22:40", 0),
            Node::new("a", "chunk:a-m", Run, Succeeded, "index chunk a-m")
                .ran("11s", "10:22:43", 0)
                .dep("scan", DependsOn),
            Node::new("b", "chunk:n-z", Run, Running, "index chunk n-z")
                .running_for("9s", "10:22:43")
                .dep("scan", DependsOn),
            Node::new(
                "merge",
                "merge:index",
                Kind::Reduce,
                Waiting,
                "reduce: merge(a, b)",
            )
            .dep("a", Rel::Reduce)
            .dep("b", Rel::Reduce),
            Node::new("swap", "swap:live", Custom, Waiting, "ipc: index.swap()")
                .dep("merge", DependsOn),
        ],
        runner: Runner {
            running: true,
            crashed: false,
            ready: 0,
            active: 1,
        },
    }
}

/// 디자인 `DAG_PARTIAL` — 문서 갈래는 끝까지 성공하고 사이트 배포만 실패했다.
/// 시안 `dagRollup` 으로 접으면 `partially_failed` 다.
pub fn partial_dag() -> Graph {
    use Kind::*;
    use Rel::*;
    use Status::*;
    Graph {
        id: "docs-publish".into(),
        name: "docs-publish".into(),
        workspace: "tasty-docs".into(),
        updated: "8m ago".into(),
        cycle: None,
        nodes: vec![
            Node::new(
                "fetch",
                "fetch:sources",
                Run,
                Succeeded,
                "git pull --ff-only",
            )
            .ran("2s", "10:12:01", 0),
            Node::new("docs", "build:docs", Run, Succeeded, "mdbook build")
                .ran("18s", "10:12:03", 0)
                .dep("fetch", DependsOn),
            Node::new(
                "upload",
                "upload:docs",
                Custom,
                Succeeded,
                "ipc: storage.put(book/)",
            )
            .ran("4s", "10:12:21", 0)
            .dep("docs", DependsOn),
            Node::new("site", "build:site", Run, Succeeded, "zola build")
                .ran("31s", "10:12:03", 0)
                .dep("fetch", DependsOn),
            Node::new(
                "deploy",
                "deploy:site",
                Run,
                Failed,
                "rsync -a public/ web:/srv",
            )
            .ran("1m 2s", "10:12:34", 1)
            .dep("site", DependsOn),
        ],
        runner: Runner {
            running: true,
            crashed: false,
            ready: 0,
            active: 0,
        },
    }
}

/// 디자인 `DAG_CYCLE` — 러너는 거부하지만 서피스는 그대로 그린다.
pub fn cycle_dag() -> Graph {
    use Kind::*;
    use Rel::*;
    use Status::*;
    Graph {
        id: "release-notes".into(),
        name: "release-notes".into(),
        workspace: "tasty".into(),
        updated: "just now".into(),
        cycle: Some(vec!["draft".into(), "review".into(), "revise".into()]),
        nodes: vec![
            Node::new(
                "collect",
                "collect:commits",
                Run,
                Succeeded,
                "git log --oneline",
            )
            .ran("2s", "09:58:10", 0),
            Node::new(
                "draft",
                "draft:notes",
                Custom,
                Unknown,
                "ipc: writer.draft()",
            )
            .dep("collect", DependsOn)
            .dep("revise", DependsOn),
            Node::new(
                "review",
                "review:notes",
                Custom,
                Waiting,
                "ipc: reviewer.review()",
            )
            .dep("draft", DependsOn),
            Node::new(
                "revise",
                "revise:notes",
                Custom,
                Waiting,
                "ipc: writer.revise()",
            )
            .dep("review", DependsOn),
        ],
        runner: Runner {
            running: false,
            crashed: false,
            ready: 2,
            active: 0,
        },
    }
}

/// 디자인 `DAG_DENSE` — 1 + 6 레이어 × 9 = 55 노드. LOD 하위 티어 검증용.
pub fn dense_dag() -> Graph {
    const POOL: [Status; 10] = [
        Status::Succeeded,
        Status::Succeeded,
        Status::Succeeded,
        Status::Running,
        Status::Ready,
        Status::Waiting,
        Status::Failed,
        Status::Skipped,
        Status::Cancelled,
        Status::Unknown,
    ];
    let mut nodes = vec![
        Node::new("n0", "fan:root", Kind::Run, Status::Succeeded, "fan out")
            .ran("1s", "10:00:00", 0),
    ];
    for layer in 1..=6usize {
        for i in 0..9usize {
            let parent = if layer == 1 {
                "n0".to_owned()
            } else {
                format!("n{}_{}", layer - 1, (i + (i % 2)) % 9)
            };
            nodes.push(
                Node::new(
                    &format!("n{layer}_{i}"),
                    &format!("task:{layer}-{i}"),
                    Kind::Run,
                    POOL[(layer * 3 + i) % POOL.len()],
                    &format!("run step {layer}.{i}"),
                )
                .ran(&format!("{}s", layer + i), &format!("10:0{layer}:0{i}"), 0)
                .dep(&parent, Rel::DependsOn),
            );
        }
    }
    Graph {
        id: "wide-fanout".into(),
        name: "wide-fanout".into(),
        workspace: "tasty".into(),
        updated: "4s ago".into(),
        cycle: None,
        nodes,
        runner: Runner {
            running: true,
            crashed: false,
            ready: 6,
            active: 4,
        },
    }
}

/// 파선 배열이 vendor 토큰 `dag-edge-dash-*` 의 strokeStyle 값과 같은지 본다.
/// 본체 `DagRelation::dash` 는 루트 패키지 시험이 같은 토큰과 따로 대조한다.
#[cfg(test)]
mod dash_tokens {
    use std::collections::BTreeMap;

    use super::Rel;

    const PREFIX: &str = "dag-edge-dash-";

    fn dash_tokens() -> BTreeMap<String, Vec<f32>> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tasty-design-tokens/dtcg/tasty.tokens.json"
        );
        let text = std::fs::read_to_string(path).expect("read vendor tokens");
        let root: serde_json::Value = serde_json::from_str(&text).expect("parse vendor tokens");
        let mut out = BTreeMap::new();
        collect(&root, &mut out);
        out
    }

    fn collect(value: &serde_json::Value, out: &mut BTreeMap<String, Vec<f32>>) {
        let Some(map) = value.as_object() else { return };
        for (key, child) in map {
            if let Some(slug) = key.strip_prefix(PREFIX) {
                assert_eq!(child["$type"], "strokeStyle", "{key}");
                let pattern = child["$value"].as_str().expect("strokeStyle string");
                let dashes = pattern
                    .split_whitespace()
                    .map(|n| n.parse::<f32>().expect("dash number"))
                    .collect();
                assert!(
                    out.insert(slug.to_string(), dashes).is_none(),
                    "{key} twice"
                );
            } else {
                collect(child, out);
            }
        }
    }

    fn slug(rel: Rel) -> Option<&'static str> {
        match rel {
            Rel::DependsOn => None,
            Rel::Fallback => Some("fallback"),
            Rel::Reduce => Some("reduce"),
            Rel::Binding => Some("binding"),
            Rel::Transition => Some("transition"),
        }
    }

    #[test]
    fn gallery_edge_dashes_match_the_vendor_tokens() {
        let tokens = dash_tokens();
        let mut seen = Vec::new();
        for rel in Rel::ALL {
            match slug(rel) {
                Some(slug) => {
                    let want = tokens
                        .get(slug)
                        .unwrap_or_else(|| panic!("no token {PREFIX}{slug}"));
                    assert_eq!(rel.dash(), Some(want.as_slice()), "{rel:?}");
                    seen.push(slug.to_string());
                }
                None => assert_eq!(rel.dash(), None, "{rel:?} is drawn solid"),
            }
        }
        seen.sort();
        assert_eq!(
            tokens.keys().cloned().collect::<Vec<_>>(),
            seen,
            "every dash token belongs to one relation"
        );
    }
}
