//! 프리셋 구조 미리보기·편집 위젯. 실제 터미널·webview 대신 pane·탭·surface 구조를 그린다.
//! 보기 모드의 탭 전환은 저장하지 않는다. 편집 모드의 설정 요청은 LeafDraft를 사용하는
//! 별도 화면으로 넘기며 leaf 안에 입력 폼을 넣지 않는다.

use tasty_presets::{
    LayoutPreset, PanePreset, PresetPane, PresetPaneNode, PresetSplitDirection, PresetSurface,
    PresetSurfaceLayout, PresetTab, TabPreset, WorkspacePreset,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::adapters::ui::icons::{self, Icon};
use crate::i18n::t;
use crate::runtime::surface_registry::{PresetFieldInput, PresetFieldSpec, PresetFieldTarget};

mod surface_draft;
pub use surface_draft::{LeafDraft, LeafLocation};

use crate::adapters::ui::zoomed_px as z;

// 이 화면 전용 치수는 배율 1 기준이며 사용할 때 z로 UI 배율을 적용한다.
// 갤러리는 전역 zoom을 사용하므로 같은 치수에 별도 배율을 곱하지 않는다.
/// mini tab strip height.
const STRIP_H: LogicalPx = LogicalPx(20.0);
/// add-tab `+` 버튼 폭(디자인 22×20 — strip 높이보다 2px 넓다).
const ADD_TAB_W: LogicalPx = LogicalPx(22.0);
/// 활성 탭 본문 padding.
const BODY_PAD: LogicalPx = LogicalPx(3.0);
/// surface leaf 아이콘↔라벨 gap.
const LEAF_GAP: LogicalPx = LogicalPx(6.0);
/// mini tab 좌우 padding.
const TAB_PAD_X: LogicalPx = LogicalPx(9.0);
/// mini tab close `×` 히트영역 한 변(14×14).
const CLOSE_HIT: LogicalPx = LogicalPx(14.0);
/// close `×` 왼쪽 margin(라벨과의 간격).
const CLOSE_MARGIN: LogicalPx = LogicalPx(1.0);
/// close `×` 노출 시 탭 우측 패딩(9→3 축소).
const CLOSE_TAB_PAD: LogicalPx = LogicalPx(3.0);
/// 편집 모드 선택 핸들(remove) 한 변 크기.
const HANDLE_SZ: LogicalPx = LogicalPx(18.0);
/// 핸들 클러스터 모서리 inset.
const HANDLE_INSET: LogicalPx = LogicalPx(4.0);
/// 경계 hover-split 존 밴드 폭 비율(변 기준 바깥 30%). 길이가 아니라 배율이라
/// `LogicalPx` 가 아니다 — `rect.width()` 에 곱해져 길이를 만드는 쪽이다.
const SPLIT_ZONE_EDGE: f32 = 0.3;
/// split 존 최소 축 길이(px). 축이 이 값 미만이면 그 축 밴드는 소멸(degrade)해
/// 좁은 leaf 에서도 중앙 선택이 항상 가능하다.
const SPLIT_ZONE_MIN: LogicalPx = LogicalPx(46.0);
/// leaf 미리보기 값 요약 표시 임계(구조 상수 — 토큰 아님, `SPLIT_ZONE_MIN` 동류).
/// 빈 leaf 박스가 이 너비/높이 미만이면 요약을 숨기고 아이콘 + kind명만 남긴다.
const LEAF_SUMMARY_MIN_W: LogicalPx = LogicalPx(96.0);
const LEAF_SUMMARY_MIN_H: LogicalPx = LogicalPx(72.0);
/// leaf 짧은 축이 이 값 미만이면 kind명까지 숨기고 아이콘만 남긴다(icon-only degrade).
/// `SPLIT_ZONE_MIN` 과 같은 46px 구조 상수 계열.
const LEAF_ICON_ONLY_MIN: LogicalPx = LogicalPx(46.0);
/// 선택 leaf 핸들(설정 · remove) 사이 간격 — 디자인 `gap: 2`.
const HANDLE_GAP: LogicalPx = tasty_ui_widgets::tokens::STRUCT_GAP_2;

/// registry가 없을 때의 후보와 기본 정렬 순서. 사용자가 만들 수 없는 empty는 제외한다.
const EDIT_KINDS: &[&str] = &["terminal", "markdown", "image", "explorer", "html"];

/// 편집기 kind 드롭다운에서 숨길 시스템 kind. `empty` 는 사용자가
/// 직접 만들 수 없는 내부 상태라 capture/apply 정규화와 정합하게 후보에서 제외한다.
const HIDDEN_EDIT_KINDS: &[&str] = &["empty"];

/// 편집기가 앱 상태 없이 사용할 kind·표시명·필드 스냅샷.
/// 비어 있으면 EDIT_KINDS를 사용한다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KindCatalog {
    specs: Vec<KindSpec>,
}

#[derive(Clone, Debug, PartialEq)]
struct KindSpec {
    kind: String,
    label: String,
    /// leading 아이콘 이름(registry `SurfaceKindDef.icon` 스냅샷). `None` 이면 FILE.
    icon: Option<String>,
    /// 이 kind 를 편집할 때 노출할 필드 스키마(registry `preset_fields` 스냅샷).
    fields: Vec<PresetFieldSpec>,
}

impl KindCatalog {
    /// registry 스냅샷에서 편집기 kind catalog 를 만든다.
    /// - `HIDDEN_EDIT_KINDS`(`empty`) 는 제외.
    /// - 순서: builtin 우선([`EDIT_KINDS`] 순), 그 외 plugin kind 는 알파벳순.
    /// - 표시명: registry `display_name_i18n_key` 번역 우선, 미번역/미등록이면 capitalize.
    pub fn from_registry(registry: &crate::runtime::kind_catalog::KindCatalog) -> Self {
        let snapshot = registry.kinds_snapshot();
        let mut kinds: Vec<&'static str> = snapshot
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !HIDDEN_EDIT_KINDS.contains(k))
            .collect();
        kinds.sort_by(|a, b| {
            let ia = EDIT_KINDS.iter().position(|p| p == a);
            let ib = EDIT_KINDS.iter().position(|p| p == b);
            match (ia, ib) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b),
            }
        });
        let specs = kinds
            .iter()
            .map(|k| {
                let def = registry.get(k);
                KindSpec {
                    kind: (*k).to_string(),
                    label: def
                        .as_ref()
                        .map(|def| label_from_i18n_key(def.display_name_i18n_key, k))
                        .unwrap_or_else(|| fallback_kind_label(k)),
                    icon: def.as_ref().and_then(|def| def.icon.clone()),
                    fields: def
                        .as_ref()
                        .map(|def| def.preset_fields.clone())
                        .unwrap_or_default(),
                }
            })
            .collect();
        Self { specs }
    }

    /// 테스트·데모용 표시명과 fallback 필드로 만든 catalog.
    #[cfg(test)]
    fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self {
            specs: pairs
                .into_iter()
                .map(|(kind, label)| KindSpec {
                    fields: fallback_fields(&kind),
                    icon: None,
                    kind,
                    label,
                })
                .collect(),
        }
    }

    /// 현재 kind 를 반드시 포함한 편집 드롭다운 후보. 빈 catalog(registry 미주입)면
    /// 정적 [`EDIT_KINDS`] 로 fallback 하고, 현재 leaf 의 kind 가 목록에 없으면
    /// 덧붙여 plugin/unknown kind 가 편집 중 유실되지 않게 한다.
    pub(super) fn candidates(&self, current: &str) -> Vec<String> {
        let mut v: Vec<String> = if self.specs.is_empty() {
            EDIT_KINDS.iter().map(|s| s.to_string()).collect()
        } else {
            self.specs.iter().map(|s| s.kind.clone()).collect()
        };
        if !v.iter().any(|k| k == current) {
            v.push(current.to_string());
        }
        v
    }

    /// 등록된 표시명을 사용하고 없으면 fallback_kind_label로 구한다.
    pub(super) fn label(&self, kind: &str) -> String {
        self.specs
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.label.clone())
            .unwrap_or_else(|| fallback_kind_label(kind))
    }

    /// registry 필드를 사용하고 없으면 fallback_fields로 구한다.
    pub(super) fn fields(&self, kind: &str) -> Vec<PresetFieldSpec> {
        self.specs
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.fields.clone())
            .unwrap_or_else(|| fallback_fields(kind))
    }

    /// registry의 아이콘 이름을 해석한다. 없으면 FILE이다.
    pub(super) fn kind_icon(&self, kind: &str) -> Icon {
        self.specs
            .iter()
            .find(|s| s.kind == kind)
            .and_then(|s| s.icon.as_deref())
            .map(icons::from_name)
            .unwrap_or(icons::FILE)
    }
}

/// registry 미주입/미등록 kind 의 편집 필드 fallback. builtin/plugin 이 registry 에
/// 선언하는 스키마와 동형이라 registry 주입 여부와 무관하게 같은 폼을 그린다.
///
/// - `terminal`: cwd(dir) + startup(text).
/// - `explorer`: cwd(dir) 루트.
/// - `markdown`/`image`: file(file_path, derive_cwd) — `PresetSurface.params.file`.
/// - `html`: url(url) — `PresetSurface.params.url`.
/// - 그 외/미지정: cwd(dir) 만(안전한 기본값).
fn fallback_fields(kind: &str) -> Vec<PresetFieldSpec> {
    fn cwd_field() -> PresetFieldSpec {
        PresetFieldSpec {
            id: "cwd".to_string(),
            label_key: "preset.edit.cwd".to_string(),
            target: PresetFieldTarget::Cwd,
            input: PresetFieldInput::Dir,
            required: false,
            placeholder_key: None,
            default: None,
            derive_cwd: false,
        }
    }
    match kind {
        "terminal" => vec![
            cwd_field(),
            PresetFieldSpec {
                id: "startup".to_string(),
                label_key: "preset.edit.startup".to_string(),
                target: PresetFieldTarget::Startup,
                input: PresetFieldInput::Text,
                required: false,
                placeholder_key: Some("preset.edit.startup_hint".to_string()),
                default: None,
                derive_cwd: false,
            },
        ],
        "explorer" => vec![cwd_field()],
        "markdown" | "image" => vec![PresetFieldSpec {
            id: "file".to_string(),
            label_key: "preset.field.file".to_string(),
            target: PresetFieldTarget::Params("file".to_string()),
            input: PresetFieldInput::FilePath,
            required: true,
            placeholder_key: Some("preset.field.file_hint".to_string()),
            default: None,
            derive_cwd: true,
        }],
        "html" => vec![PresetFieldSpec {
            id: "url".to_string(),
            label_key: "preset.field.url".to_string(),
            target: PresetFieldTarget::Params("url".to_string()),
            input: PresetFieldInput::Url,
            required: true,
            placeholder_key: Some("preset.field.url_hint".to_string()),
            default: None,
            derive_cwd: false,
        }],
        _ => vec![cwd_field()],
    }
}

// 아이콘은 registry에서 읽는다. 종류별 accent는 대응 토큰이 없어 여기서 정하며 미지정 종류는 중립색이다.

pub(super) fn kind_accent(theme: &Theme, kind: &str) -> egui::Color32 {
    match kind {
        "terminal" => theme.accent_success().to_egui(),
        "markdown" => theme.accent_primary().to_egui(),
        "image" => theme.accent_info().to_egui(),
        "explorer" => theme.accent_agent().to_egui(),
        _ => theme.text_secondary().to_egui(),
    }
}

/// kind 첫 글자를 대문자로(`convert.rs::resolve_label` 의 capitalize fallback 패턴).
fn capitalize_first(kind: &str) -> String {
    let mut c = kind.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
    }
}

/// registry `display_name_i18n_key` 로 표시명을 구한다. 키가 미번역(=키 그대로
/// 반환)이면 kind 를 capitalize 한다(자리표시자 키 방어 — 빈/미번역 fallback).
fn label_from_i18n_key(key: &str, kind: &str) -> String {
    let tr = t(key);
    if tr != key {
        return tr.to_string();
    }
    capitalize_first(kind)
}

/// registry에 없는 kind는 surface.kind.<kind> 번역을 시도하고 없으면 첫 글자를 대문자로 바꾼다.
fn fallback_kind_label(kind: &str) -> String {
    label_from_i18n_key(&format!("surface.kind.{kind}"), kind)
}

// 세 종류 프리셋을 공통 미리보기 모델로 바꾸고 표시명도 미리 해석한다.

/// surface의 종류·표시명과 저장할 필드. 편집하지 않은 cwd·startup·params도 보존한다.
/// leaf ID는 PresetSurface의 영속 ID이며 불러올 때 바꾸지 않는다. 새 leaf에만 ID를 부여한다.
#[derive(Clone, Debug, PartialEq)]
struct Leaf {
    id: usize,
    kind: String,
    label: String,
    cwd: Option<String>,
    startup: Option<String>,
    params: serde_json::Value,
}

impl Leaf {
    /// mem::replace 자리채움용 빈 leaf (즉시 덮어써져 drop 됨).
    fn placeholder() -> Self {
        Leaf {
            id: 0,
            kind: String::new(),
            label: String::new(),
            cwd: None,
            startup: None,
            params: serde_json::Value::Null,
        }
    }
}

/// 하위 레이아웃(탭 안의 surface split).
#[derive(Clone, Debug, PartialEq)]
enum SurfNode {
    Leaf(Leaf),
    Split {
        row: bool,
        ratio: f32,
        first: Box<SurfNode>,
        second: Box<SurfNode>,
    },
}

impl SurfNode {
    /// 탭 대표 kind = 첫 leaf (디자인 `activeKind` — mini-tab 아이콘 구동).
    fn rep_kind(&self) -> &str {
        let mut n = self;
        loop {
            match n {
                SurfNode::Leaf(l) => return &l.kind,
                SurfNode::Split { first, .. } => n = first,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PreviewTab {
    /// 표시명 — `explicit_name` 우선, 없으면 대표 leaf 의 kind 표시명(자동).
    name: String,
    /// round-trip 용 사용자 지정 이름(자동 이름이면 None).
    explicit_name: Option<String>,
    layout: SurfNode,
}

#[derive(Clone, Debug, PartialEq)]
struct PreviewPane {
    /// 안정 식별자(build 시 부여) — 탭 클릭 상호작용 id + active override 키.
    id: usize,
    tabs: Vec<PreviewTab>,
    active: usize,
}

impl PreviewPane {
    /// mem::replace 자리채움용 빈 pane (즉시 덮어써져 drop 됨).
    fn placeholder() -> Self {
        PreviewPane {
            id: 0,
            tabs: Vec::new(),
            active: 0,
        }
    }
}

/// 상위 레이아웃(pane split).
#[derive(Clone, Debug, PartialEq)]
enum PaneNode {
    Leaf(PreviewPane),
    Split {
        row: bool,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

/// scope variant — Workspace/Pane 은 pane 트리, Tab 은 단일 surface-split 프레임.
#[derive(Clone, Debug, PartialEq)]
enum Root {
    Panes(PaneNode),
    /// Tab scope: strip 없이 단일 탭 본문처럼 프레임.
    TabFrame(SurfNode),
}

/// pane 분할을 허용할 프리셋 범위. Workspace와 Pane은 같은 Root::Panes를 쓰지만
/// Workspace만 분할할 수 있어 별도 범위 값이 필요하다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Workspace,
    Pane,
    Tab,
}

/// 정규화된 preview 트리. 라이브 상호작용(active 탭)은 트리 안에 보관된다 —
/// 호출자가 프레임 간 인스턴스를 유지(`Clone`)하면 클릭 전환이 지속된다.
#[derive(Clone, Debug, PartialEq)]
pub struct DemoLayout {
    root: Root,
    /// 편집 대상 scope — pane split 유효성 판별용([`Scope`]).
    scope: Scope,
    /// 기존 pane·leaf ID 모두보다 큰 새 노드 ID.
    next_id: usize,
}

/// pane의 세션 ID 생성기. leaf는 영속 ID를 사용한다. 두 종류 ID가 겹쳐도
/// 조회 대상과 egui ID 접두사가 구분되며 저장 시 보존해야 하는 것은 leaf ID다.
struct IdGen(usize);
impl IdGen {
    fn next(&mut self) -> usize {
        let id = self.0;
        self.0 += 1;
        id
    }
}

/// 정규화로 채운 surface 영속 ID를 사용한다.
fn norm_surf(node: &PresetSurfaceLayout, resolve: &dyn Fn(&str) -> String) -> SurfNode {
    match node {
        PresetSurfaceLayout::Leaf { surface } => SurfNode::Leaf(Leaf {
            id: surface
                .id
                .expect("normalize_surface_ids assigns every surface an id before build")
                as usize,
            kind: surface.kind.clone(),
            label: resolve(&surface.kind),
            cwd: surface.cwd.clone(),
            startup: surface.startup_command.clone(),
            params: surface.params.clone(),
        }),
        PresetSurfaceLayout::Split {
            direction,
            ratio,
            first,
            second,
        } => SurfNode::Split {
            row: is_row(*direction),
            ratio: *ratio,
            first: Box::new(norm_surf(first, resolve)),
            second: Box::new(norm_surf(second, resolve)),
        },
    }
}

fn norm_tab(tab: &PresetTab, resolve: &dyn Fn(&str) -> String) -> PreviewTab {
    let layout = norm_surf(&tab.layout, resolve);
    let name = tab
        .explicit_name
        .clone()
        .unwrap_or_else(|| resolve(layout.rep_kind()));
    PreviewTab {
        name,
        explicit_name: tab.explicit_name.clone(),
        layout,
    }
}

fn norm_pane(
    pane: &PresetPane,
    resolve: &dyn Fn(&str) -> String,
    pane_ids: &mut IdGen,
) -> PreviewPane {
    let id = pane_ids.next();
    let tabs: Vec<PreviewTab> = pane.tabs.iter().map(|t| norm_tab(t, resolve)).collect();
    let active = pane.active_tab.min(tabs.len().saturating_sub(1));
    PreviewPane { id, tabs, active }
}

fn norm_pane_node(
    node: &PresetPaneNode,
    resolve: &dyn Fn(&str) -> String,
    pane_ids: &mut IdGen,
) -> PaneNode {
    match node {
        PresetPaneNode::Leaf { pane } => PaneNode::Leaf(norm_pane(pane, resolve, pane_ids)),
        PresetPaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } => PaneNode::Split {
            row: is_row(*direction),
            ratio: *ratio,
            first: Box::new(norm_pane_node(first, resolve, pane_ids)),
            second: Box::new(norm_pane_node(second, resolve, pane_ids)),
        },
    }
}

/// 새 ID를 정하기 위한 pane·leaf 전체의 최대 ID.
fn max_node_id(root: &Root) -> usize {
    fn surf(n: &SurfNode) -> usize {
        match n {
            SurfNode::Leaf(l) => l.id,
            SurfNode::Split { first, second, .. } => surf(first).max(surf(second)),
        }
    }
    fn pane(n: &PaneNode) -> usize {
        match n {
            PaneNode::Leaf(p) => {
                let tab_max = p.tabs.iter().map(|t| surf(&t.layout)).max().unwrap_or(0);
                p.id.max(tab_max)
            }
            PaneNode::Split { first, second, .. } => pane(first).max(pane(second)),
        }
    }
    match root {
        Root::Panes(n) => pane(n),
        Root::TabFrame(s) => surf(s),
    }
}

/// 실제 레이아웃과 같은 방향: Vertical은 좌우, Horizontal은 상하 분할이다.
fn is_row(d: PresetSplitDirection) -> bool {
    matches!(d, PresetSplitDirection::Vertical)
}

impl DemoLayout {
    pub fn from_workspace(p: &WorkspacePreset, catalog: &KindCatalog) -> Self {
        // 원본을 바꾸지 않도록 사본에서 영속 ID를 정규화한다.
        let mut model = p.clone();
        model.normalize_surface_ids();
        let resolve = |k: &str| catalog.label(k);
        let mut pane_ids = IdGen(0);
        let root = Root::Panes(norm_pane_node(&model.layout, &resolve, &mut pane_ids));
        let next_id = max_node_id(&root) + 1;
        Self {
            root,
            scope: Scope::Workspace,
            next_id,
        }
    }

    pub fn from_tab(p: &TabPreset, catalog: &KindCatalog) -> Self {
        let mut model = p.clone();
        model.normalize_surface_ids();
        let resolve = |k: &str| catalog.label(k);
        let root = Root::TabFrame(norm_surf(&model.tab.layout, &resolve));
        let next_id = max_node_id(&root) + 1;
        Self {
            root,
            scope: Scope::Tab,
            next_id,
        }
    }

    pub fn from_pane(p: &PanePreset, catalog: &KindCatalog) -> Self {
        let mut model = p.clone();
        model.normalize_surface_ids();
        let resolve = |k: &str| catalog.label(k);
        let mut pane_ids = IdGen(0);
        let root = Root::Panes(PaneNode::Leaf(norm_pane(
            &model.pane,
            &resolve,
            &mut pane_ids,
        )));
        let next_id = max_node_id(&root) + 1;
        Self {
            root,
            scope: Scope::Pane,
            next_id,
        }
    }

    /// read-only 미리보기를 그리고 탭 클릭 상호작용을 처리한다.
    /// 탭 클릭으로 active 가 바뀌면 `true` 를 반환한다(호출자 repaint 신호).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        rect: egui::Rect,
        catalog: &KindCatalog,
    ) -> bool {
        let act = {
            let mut cx = DrawCtx {
                edit: false,
                sel: None,
                act: None,
                catalog,
            };
            self.draw(ui, theme, rect, &mut cx);
            cx.act
        };
        match act {
            Some(Act::SetActive { pane, idx }) => self.set_active(pane, idx),
            _ => false,
        }
    }

    /// 편집 모드로 그린다. 선택(`selected`)은 leaf id. 트리/필드/active 가 바뀌어
    /// 디스크 동기화가 필요하면 [`ShowOutcome::Mutated`], 선택만 바뀌면
    /// [`ShowOutcome::Repaint`] 를 반환한다.
    pub fn show_edit(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        rect: egui::Rect,
        selected: &mut Option<usize>,
        catalog: &KindCatalog,
    ) -> ShowOutcome {
        // 뒤에 등록하는 leaf 위젯이 입력을 받으면 배경 클릭으로 처리하지 않는다.
        let bg = ui.interact(rect, ui.id().with("preset_demo_bg"), egui::Sense::click());

        let act = {
            let mut cx = DrawCtx {
                edit: true,
                sel: *selected,
                act: None,
                catalog,
            };
            self.draw(ui, theme, rect, &mut cx);
            cx.act
        };

        let act = act.or_else(|| bg.clicked().then_some(Act::Deselect));
        match act {
            None => ShowOutcome::None,
            Some(act) => self.dispatch(act, selected, catalog),
        }
    }

    /// 마우스·단축키 동작을 공통으로 적용하고 선택 정리와 저장 필요 여부를 반환한다.
    fn dispatch(
        &mut self,
        act: Act,
        selected: &mut Option<usize>,
        catalog: &KindCatalog,
    ) -> ShowOutcome {
        match act {
            Act::Select(id) => {
                *selected = Some(id);
                ShowOutcome::Repaint
            }
            Act::Deselect => {
                if selected.is_some() {
                    *selected = None;
                    ShowOutcome::Repaint
                } else {
                    ShowOutcome::None
                }
            }
            Act::SetActive { pane, idx } => {
                if self.set_active(pane, idx) {
                    ShowOutcome::Mutated
                } else {
                    ShowOutcome::None
                }
            }
            Act::OpenSettings(id) => {
                *selected = Some(id);
                ShowOutcome::OpenSettings(id)
            }
            Act::Split { id, row, before } => {
                // 경계 hover-split 존은 좌/상 클릭 시 before(새 leaf first), 우/하는
                // after. 키보드 단축키(apply_shortcut)는 항상 after(before=false).
                self.split_leaf(id, row, before, catalog);
                ShowOutcome::Mutated
            }
            Act::Remove { id } => {
                if self.remove_leaf(id, catalog) {
                    if *selected == Some(id) {
                        *selected = None;
                    }
                    ShowOutcome::Mutated
                } else {
                    ShowOutcome::None
                }
            }
            Act::AddTab { pane } => {
                self.add_tab(pane, catalog);
                ShowOutcome::Mutated
            }
            Act::SplitPane { id, row } => {
                if self.split_pane(id, row, catalog) {
                    ShowOutcome::Mutated
                } else {
                    ShowOutcome::None
                }
            }
            Act::RemovePane { id } => {
                if self.remove_pane(id, catalog) {
                    if selected.is_some_and(|s| !self.contains_leaf(s)) {
                        *selected = None;
                    }
                    ShowOutcome::Mutated
                } else {
                    ShowOutcome::None
                }
            }
            Act::RemoveTab { pane, idx } => {
                if self.remove_tab(pane, idx) {
                    if selected.is_some_and(|s| !self.contains_leaf(s)) {
                        *selected = None;
                    }
                    ShowOutcome::Mutated
                } else {
                    ShowOutcome::None
                }
            }
        }
    }

    /// 선택 leaf에만 단축키를 적용한다. pane·탭 동작은 leaf의 소속 pane을 대상으로 한다.
    /// 선택이 없으면 아무것도 하지 않으며 범위별 허용 여부는 각 편집 함수에서 확인한다.
    pub fn apply_shortcut(
        &mut self,
        action: ShortcutAction,
        selected: &mut Option<usize>,
        catalog: &KindCatalog,
    ) -> ShowOutcome {
        let Some(leaf_id) = *selected else {
            return ShowOutcome::None;
        };
        match action {
            ShortcutAction::SplitSurfaceVertical => self.dispatch(
                Act::Split {
                    id: leaf_id,
                    row: true,
                    before: false,
                },
                selected,
                catalog,
            ),
            ShortcutAction::SplitSurfaceHorizontal => self.dispatch(
                Act::Split {
                    id: leaf_id,
                    row: false,
                    before: false,
                },
                selected,
                catalog,
            ),
            ShortcutAction::CloseSurface => {
                self.dispatch(Act::Remove { id: leaf_id }, selected, catalog)
            }
            ShortcutAction::NewTab => {
                let Some(pane) = self.pane_id_of_leaf(leaf_id) else {
                    return ShowOutcome::None;
                };
                self.dispatch(Act::AddTab { pane }, selected, catalog)
            }
            ShortcutAction::CloseActive => {
                let Some(pane) = self.pane_id_of_leaf(leaf_id) else {
                    return ShowOutcome::None;
                };
                let Some(idx) = self.active_tab_of(pane) else {
                    return ShowOutcome::None;
                };
                // 라이브 close_active 의 탭→pane 체인과 동형: 마지막 탭이면
                // remove_tab 이 no-op(None)이므로 pane 제거로 폴백한다.
                match self.dispatch(Act::RemoveTab { pane, idx }, selected, catalog) {
                    ShowOutcome::None => {
                        self.dispatch(Act::RemovePane { id: pane }, selected, catalog)
                    }
                    other => other,
                }
            }
            ShortcutAction::SplitPaneVertical => {
                let Some(pane) = self.pane_id_of_leaf(leaf_id) else {
                    return ShowOutcome::None;
                };
                self.dispatch(
                    Act::SplitPane {
                        id: pane,
                        row: true,
                    },
                    selected,
                    catalog,
                )
            }
            ShortcutAction::SplitPaneHorizontal => {
                let Some(pane) = self.pane_id_of_leaf(leaf_id) else {
                    return ShowOutcome::None;
                };
                self.dispatch(
                    Act::SplitPane {
                        id: pane,
                        row: false,
                    },
                    selected,
                    catalog,
                )
            }
            ShortcutAction::ClosePane => {
                let Some(pane) = self.pane_id_of_leaf(leaf_id) else {
                    return ShowOutcome::None;
                };
                self.dispatch(Act::RemovePane { id: pane }, selected, catalog)
            }
        }
    }

    /// 선택 leaf id 가 속한 pane 의 id. surface split 안 어디에 있든 그 leaf 를 탭
    /// 레이아웃에 포함하는 pane 을 반환. Tab scope(pane 없음)면 None.
    fn pane_id_of_leaf(&self, leaf_id: usize) -> Option<usize> {
        fn in_surf(node: &SurfNode, id: usize) -> bool {
            match node {
                SurfNode::Leaf(l) => l.id == id,
                SurfNode::Split { first, second, .. } => in_surf(first, id) || in_surf(second, id),
            }
        }
        fn walk(node: &PaneNode, id: usize) -> Option<usize> {
            match node {
                PaneNode::Leaf(pane) => pane
                    .tabs
                    .iter()
                    .any(|t| in_surf(&t.layout, id))
                    .then_some(pane.id),
                PaneNode::Split { first, second, .. } => {
                    walk(first, id).or_else(|| walk(second, id))
                }
            }
        }
        match &self.root {
            Root::Panes(n) => walk(n, leaf_id),
            Root::TabFrame(_) => None,
        }
    }

    /// pane_id 의 현재 active 탭 인덱스. 없으면 None(Tab scope · 미존재 pane).
    fn active_tab_of(&self, pane_id: usize) -> Option<usize> {
        fn walk(node: &PaneNode, id: usize) -> Option<usize> {
            match node {
                PaneNode::Leaf(pane) => (pane.id == id).then_some(pane.active),
                PaneNode::Split { first, second, .. } => {
                    walk(first, id).or_else(|| walk(second, id))
                }
            }
        }
        match &self.root {
            Root::Panes(n) => walk(n, pane_id),
            Root::TabFrame(_) => None,
        }
    }

    fn draw(&self, ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, cx: &mut DrawCtx<'_>) {
        match &self.root {
            Root::Panes(node) => draw_pane_tree(ui, theme, rect, node, cx),
            Root::TabFrame(node) => draw_tab_frame(ui, theme, rect, node, cx),
        }
    }

    /// pane_id 의 active 탭을 idx 로 바꾼다. 실제로 변하면 true.
    fn set_active(&mut self, pane_id: usize, idx: usize) -> bool {
        for_each_pane_mut(&mut self.root, &mut |pane| {
            if pane.id == pane_id && idx < pane.tabs.len() && pane.active != idx {
                pane.active = idx;
                true
            } else {
                false
            }
        })
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// kind 를 바꾸고 stale 값을 정리한다.
    ///
    /// - kind/label 교체.
    /// - 새 kind 필드가 쓰지 않는 전용 컬럼(cwd/startup)은 비운다(이전 kind 잔류 제거).
    /// - `params` 는 새 kind 가 선언한 param_key 만 남기고 제거한다 — 같은 kind 로의
    ///   load/편집에서는 unknown params 를 round-trip 보존하지만, **kind 자체가 바뀔
    ///   때만** 정리한다(플러그인 버전 차이 대비는 kind 미변경 경로에서).
    /// - 새 kind 필드의 `default` 로 빈 값을 초기화.
    fn set_kind(&mut self, id: usize, kind: &str, catalog: &KindCatalog) {
        let label = catalog.label(kind);
        let fields = catalog.fields(kind);
        let keeps_cwd = fields
            .iter()
            .any(|f| matches!(f.target, PresetFieldTarget::Cwd));
        let keeps_startup = fields
            .iter()
            .any(|f| matches!(f.target, PresetFieldTarget::Startup));
        let param_keys: std::collections::HashSet<&str> = fields
            .iter()
            .filter_map(|f| match &f.target {
                PresetFieldTarget::Params(k) => Some(k.as_str()),
                _ => None,
            })
            .collect();
        for_each_surf_root_mut(&mut self.root, &mut |node| {
            if let Some(l) = find_leaf_mut(node, id) {
                l.kind = kind.to_string();
                l.label = label.clone();
                if !keeps_cwd {
                    l.cwd = None;
                }
                if !keeps_startup {
                    l.startup = None;
                }
                if let Some(obj) = l.params.as_object_mut() {
                    obj.retain(|k, _| param_keys.contains(k.as_str()));
                }
                for f in &fields {
                    let Some(def) = &f.default else { continue };
                    match &f.target {
                        PresetFieldTarget::Cwd => {
                            if l.cwd.is_none() {
                                l.cwd = Some(def.clone());
                            }
                        }
                        PresetFieldTarget::Startup => {
                            if l.startup.is_none() {
                                l.startup = Some(def.clone());
                            }
                        }
                        PresetFieldTarget::Params(k) => {
                            let absent = l
                                .params
                                .get(k)
                                .and_then(|v| v.as_str())
                                .is_none_or(str::is_empty);
                            if absent {
                                set_param(&mut l.params, k, def);
                            }
                        }
                    }
                }
            }
        });
        self.refresh_auto_names(catalog);
    }

    /// 선언 필드 하나의 값을 target(params 키 / cwd / startup)에 write. 빈 문자열은
    /// 값 제거(전용 컬럼은 None, params 는 키 삭제)로 처리해 round-trip 이 값 부재를
    /// 그대로 보존하게 한다.
    fn set_field(&mut self, id: usize, target: &PresetFieldTarget, value: String) {
        let v = if value.is_empty() { None } else { Some(value) };
        for_each_surf_root_mut(&mut self.root, &mut |node| {
            if let Some(l) = find_leaf_mut(node, id) {
                match target {
                    PresetFieldTarget::Cwd => l.cwd = v.clone(),
                    PresetFieldTarget::Startup => l.startup = v.clone(),
                    PresetFieldTarget::Params(k) => match &v {
                        Some(s) => set_param(&mut l.params, k, s),
                        None => remove_param(&mut l.params, k),
                    },
                }
            }
        });
    }

    /// before면 새 leaf를 왼쪽·위에, 아니면 오른쪽·아래에 둔다. 키보드는 뒤쪽에 추가한다.
    fn split_leaf(&mut self, id: usize, row: bool, before: bool, catalog: &KindCatalog) {
        let leaf_id = self.alloc_id();
        let new_leaf = Leaf {
            id: leaf_id,
            kind: "terminal".to_string(),
            label: catalog.label("terminal"),
            cwd: None,
            startup: None,
            params: serde_json::Value::Null,
        };
        let mut slot = Some(new_leaf);
        for_each_surf_root_mut(&mut self.root, &mut |node| {
            split_node(node, id, row, before, &mut slot);
        });
        self.refresh_auto_names(catalog);
    }

    /// Workspace에서만 pane을 0.5 비율로 나누고 뒤쪽에 터미널 하나의 pane을 추가한다.
    /// Pane 프리셋은 단일 pane만 저장할 수 있어 분할하지 않는다.
    fn split_pane(&mut self, pane_id: usize, row: bool, catalog: &KindCatalog) -> bool {
        if self.scope != Scope::Workspace {
            return false;
        }
        let term_label = catalog.label("terminal");
        let pane_new_id = self.alloc_id();
        let leaf_id = self.alloc_id();
        let new_pane = PreviewPane {
            id: pane_new_id,
            tabs: vec![PreviewTab {
                name: term_label.clone(),
                explicit_name: None,
                layout: SurfNode::Leaf(Leaf {
                    id: leaf_id,
                    kind: "terminal".to_string(),
                    label: term_label,
                    cwd: None,
                    startup: None,
                    params: serde_json::Value::Null,
                }),
            }],
            active: 0,
        };
        let mut slot = Some(new_pane);
        let did = match &mut self.root {
            Root::Panes(node) => split_pane_node(node, pane_id, row, &mut slot),
            Root::TabFrame(_) => false,
        };
        if did {
            self.refresh_auto_names(catalog);
        }
        did
    }

    /// pane 을 제거하고 부모 pane split 을 형제로 collapse. 루트 단일 pane(형제 없음)
    /// 은 제거 불가 — 그 경우 false(빈 preset 방지).
    fn remove_pane(&mut self, pane_id: usize, catalog: &KindCatalog) -> bool {
        let removed = match &mut self.root {
            Root::Panes(node) => remove_pane_node(node, pane_id),
            Root::TabFrame(_) => false,
        };
        if removed {
            self.refresh_auto_names(catalog);
        }
        removed
    }

    /// 탭을 지우고 active 인덱스를 조정한다. 마지막 탭이면 지우지 않는다.
    /// 그때 pane까지 지울지는 단축키 처리에서 결정한다.
    fn remove_tab(&mut self, pane_id: usize, idx: usize) -> bool {
        for_each_pane_mut(&mut self.root, &mut |pane| {
            if pane.id == pane_id && idx < pane.tabs.len() && pane.tabs.len() > 1 {
                pane.tabs.remove(idx);
                // active 클램프: 제거로 범위를 벗어나면 마지막으로, 제거된 탭보다
                // 뒤였으면 같은 탭을 계속 가리키도록 한 칸 당긴다.
                if pane.active >= pane.tabs.len() {
                    pane.active = pane.tabs.len() - 1;
                } else if pane.active > idx {
                    pane.active -= 1;
                }
                true
            } else {
                false
            }
        })
    }

    /// 트리에 leaf id 가 아직 존재하는지 — 구조 제거 후 선택 유효성 검사용.
    fn contains_leaf(&self, id: usize) -> bool {
        fn in_surf(node: &SurfNode, id: usize) -> bool {
            match node {
                SurfNode::Leaf(l) => l.id == id,
                SurfNode::Split { first, second, .. } => in_surf(first, id) || in_surf(second, id),
            }
        }
        fn in_pane(node: &PaneNode, id: usize) -> bool {
            match node {
                PaneNode::Leaf(p) => p.tabs.iter().any(|t| in_surf(&t.layout, id)),
                PaneNode::Split { first, second, .. } => in_pane(first, id) || in_pane(second, id),
            }
        }
        match &self.root {
            Root::Panes(n) => in_pane(n, id),
            Root::TabFrame(s) => in_surf(s, id),
        }
    }

    /// leaf 를 제거하고 부모 split 을 형제로 collapse. 단일 surface(루트 leaf)는
    /// 제거 불가 — 그 경우 false.
    fn remove_leaf(&mut self, id: usize, catalog: &KindCatalog) -> bool {
        let mut removed = false;
        for_each_surf_root_mut(&mut self.root, &mut |node| {
            if remove_node(node, id) {
                removed = true;
            }
        });
        if removed {
            self.refresh_auto_names(catalog);
        }
        removed
    }

    fn add_tab(&mut self, pane_id: usize, catalog: &KindCatalog) {
        let leaf_id = self.alloc_id();
        let term_label = catalog.label("terminal");
        for_each_pane_mut(&mut self.root, &mut |pane| {
            if pane.id == pane_id {
                pane.tabs.push(PreviewTab {
                    name: term_label.clone(),
                    explicit_name: None,
                    layout: SurfNode::Leaf(Leaf {
                        id: leaf_id,
                        kind: "terminal".to_string(),
                        label: term_label.clone(),
                        cwd: None,
                        startup: None,
                        params: serde_json::Value::Null,
                    }),
                });
                pane.active = pane.tabs.len() - 1;
                true
            } else {
                false
            }
        });
    }

    /// 자동 이름(explicit_name 없는) 탭의 표시명을 대표 kind 로 갱신 — kind/구조
    /// 변경 후 mini-tab 라벨 live-update.
    fn refresh_auto_names(&mut self, catalog: &KindCatalog) {
        fn fix_pane(node: &mut PaneNode, catalog: &KindCatalog) {
            match node {
                PaneNode::Leaf(pane) => {
                    for t in &mut pane.tabs {
                        if t.explicit_name.is_none() {
                            t.name = catalog.label(t.layout.rep_kind());
                        }
                    }
                }
                PaneNode::Split { first, second, .. } => {
                    fix_pane(first, catalog);
                    fix_pane(second, catalog);
                }
            }
        }
        if let Root::Panes(node) = &mut self.root {
            fix_pane(node, catalog);
        }
    }

    /// Workspace/Pane scope 의 상위 pane 트리를 모델로 되돌린다. Tab scope 면 None.
    pub fn rebuild_pane_node(&self) -> Option<PresetPaneNode> {
        match &self.root {
            Root::Panes(n) => Some(pane_node_to_model(n)),
            Root::TabFrame(_) => None,
        }
    }

    /// Pane scope 의 단일 pane 을 모델로 되돌린다. 그 외 None.
    pub fn rebuild_single_pane(&self) -> Option<PresetPane> {
        match &self.root {
            Root::Panes(PaneNode::Leaf(p)) => Some(pane_to_model(p)),
            _ => None,
        }
    }

    /// Tab scope 의 surface 레이아웃을 모델로 되돌린다. 그 외 None.
    pub fn rebuild_surface_layout(&self) -> Option<PresetSurfaceLayout> {
        match &self.root {
            Root::TabFrame(s) => Some(surf_to_model(s)),
            _ => None,
        }
    }
}

/// 선택 leaf를 대상으로 하는 편집 단축키. 대상 해석과 범위 검사는 DemoLayout에서 한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutAction {
    /// 선택 surface 를 좌우(row)로 분할.
    SplitSurfaceVertical,
    /// 선택 surface 를 상하(column)로 분할.
    SplitSurfaceHorizontal,
    /// 선택 surface 를 닫는다(마지막 1장이면 no-op).
    CloseSurface,
    /// 선택 surface 가 속한 pane 에 terminal 탭 추가.
    NewTab,
    /// 그 pane 의 active 탭을 닫는다(마지막 탭이면 pane 제거로 폴백).
    CloseActive,
    /// 그 pane 을 좌우로 분할(Workspace scope 한정).
    SplitPaneVertical,
    /// 그 pane 을 상하로 분할(Workspace scope 한정).
    SplitPaneHorizontal,
    /// 그 pane 을 닫는다(루트 단일 pane 이면 no-op).
    ClosePane,
}

/// [`DemoLayout::show_edit`] 결과 — 호출자 repaint/persist 분기용.
pub enum ShowOutcome {
    /// 변화 없음.
    None,
    /// 선택만 바뀜 — repaint 만, 저장 안 함.
    Repaint,
    /// 트리/필드/active 변경 — 디스크 동기화 필요.
    Mutated,
    /// 이 leaf 의 설정 화면을 열어 달라는 요청(선택은 이미 그 leaf 로 바뀌었다).
    /// 트리는 바뀌지 않았으니 저장하지 않는다.
    OpenSettings(usize),
}

/// 한 프레임에 수집되는 편집 의도(specimen `applyAction` 의 액션).
enum Act {
    Select(usize),
    Deselect,
    SetActive {
        pane: usize,
        idx: usize,
    },
    /// 설정 핸들 · 더블클릭 — 그 leaf 를 선택하고 설정 화면을 연다.
    OpenSettings(usize),
    Split {
        id: usize,
        row: bool,
        before: bool,
    },
    Remove {
        id: usize,
    },
    AddTab {
        pane: usize,
    },
    SplitPane {
        id: usize,
        row: bool,
    },
    RemovePane {
        id: usize,
    },
    RemoveTab {
        pane: usize,
        idx: usize,
    },
}

/// draw 재귀에 전달되는 편집 컨텍스트.
struct DrawCtx<'a> {
    edit: bool,
    sel: Option<usize>,
    act: Option<Act>,
    /// kind 드롭다운 후보/라벨 소스(registry 스냅샷). 프레임마다 fresh 하게 주입되어
    /// 편집 중 후보 목록이 런타임 등록 kind 를 즉시 반영한다.
    catalog: &'a KindCatalog,
}

/// 모든 pane 을 순회하며 `f` 를 호출, 하나라도 true 면 true(첫 적용에서 멈추지 않음).
fn for_each_pane_mut(root: &mut Root, f: &mut impl FnMut(&mut PreviewPane) -> bool) -> bool {
    fn walk(node: &mut PaneNode, f: &mut impl FnMut(&mut PreviewPane) -> bool) -> bool {
        match node {
            PaneNode::Leaf(pane) => f(pane),
            PaneNode::Split { first, second, .. } => {
                let a = walk(first, f);
                let b = walk(second, f);
                a || b
            }
        }
    }
    match root {
        Root::Panes(node) => walk(node, f),
        Root::TabFrame(_) => false,
    }
}

/// 각 탭의 surface 레이아웃 루트(또는 TabFrame 루트)마다 `f` 를 호출.
fn for_each_surf_root_mut(root: &mut Root, f: &mut impl FnMut(&mut SurfNode)) {
    fn walk(node: &mut PaneNode, f: &mut impl FnMut(&mut SurfNode)) {
        match node {
            PaneNode::Leaf(pane) => {
                for t in &mut pane.tabs {
                    f(&mut t.layout);
                }
            }
            PaneNode::Split { first, second, .. } => {
                walk(first, f);
                walk(second, f);
            }
        }
    }
    match root {
        Root::Panes(node) => walk(node, f),
        Root::TabFrame(s) => f(s),
    }
}

fn find_leaf_mut(node: &mut SurfNode, id: usize) -> Option<&mut Leaf> {
    match node {
        SurfNode::Leaf(l) => (l.id == id).then_some(l),
        SurfNode::Split { first, second, .. } => {
            if let Some(l) = find_leaf_mut(first, id) {
                Some(l)
            } else {
                find_leaf_mut(second, id)
            }
        }
    }
}

/// id 의 leaf 를 split(leaf, new_leaf)로 교체. `slot` 에서 new_leaf 를 take.
/// `before == true` 면 새 leaf 가 first, 아니면 second. 기존 leaf id 는 보존
/// (`std::mem::replace` 로 이동만).
fn split_node(
    node: &mut SurfNode,
    id: usize,
    row: bool,
    before: bool,
    slot: &mut Option<Leaf>,
) -> bool {
    match node {
        SurfNode::Leaf(l) => {
            if l.id == id
                && let Some(nl) = slot.take()
            {
                let old = std::mem::replace(l, Leaf::placeholder());
                let (first, second) = if before {
                    (SurfNode::Leaf(nl), SurfNode::Leaf(old))
                } else {
                    (SurfNode::Leaf(old), SurfNode::Leaf(nl))
                };
                *node = SurfNode::Split {
                    row,
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                return true;
            }
            false
        }
        SurfNode::Split { first, second, .. } => {
            split_node(first, id, row, before, slot) || split_node(second, id, row, before, slot)
        }
    }
}

/// pane_id 의 pane leaf 를 split(pane, new_pane)로 교체. `slot` 에서 new_pane 을
/// take(새 pane 이 second). 기존 pane 서브트리 id 는 보존(`std::mem::replace` 이동).
fn split_pane_node(
    node: &mut PaneNode,
    id: usize,
    row: bool,
    slot: &mut Option<PreviewPane>,
) -> bool {
    match node {
        PaneNode::Leaf(pane) => {
            if pane.id == id
                && let Some(np) = slot.take()
            {
                let old = std::mem::replace(pane, PreviewPane::placeholder());
                *node = PaneNode::Split {
                    row,
                    ratio: 0.5,
                    first: Box::new(PaneNode::Leaf(old)),
                    second: Box::new(PaneNode::Leaf(np)),
                };
                return true;
            }
            false
        }
        PaneNode::Split { first, second, .. } => {
            split_pane_node(first, id, row, slot) || split_pane_node(second, id, row, slot)
        }
    }
}

/// id 의 pane 을 제거하고 부모 split 을 형제로 collapse(`remove_node` 의 pane 버전).
/// 루트 leaf pane 이면 false. 형제 서브트리 id 는 보존(`std::mem::replace` 이동).
fn remove_pane_node(node: &mut PaneNode, id: usize) -> bool {
    let PaneNode::Split { first, second, .. } = node else {
        return false;
    };
    let first_is = matches!(first.as_ref(), PaneNode::Leaf(p) if p.id == id);
    if first_is {
        let sibling =
            std::mem::replace(second.as_mut(), PaneNode::Leaf(PreviewPane::placeholder()));
        *node = sibling;
        return true;
    }
    let second_is = matches!(second.as_ref(), PaneNode::Leaf(p) if p.id == id);
    if second_is {
        let sibling = std::mem::replace(first.as_mut(), PaneNode::Leaf(PreviewPane::placeholder()));
        *node = sibling;
        return true;
    }
    remove_pane_node(first, id) || remove_pane_node(second, id)
}

/// id 의 leaf 를 제거하고 부모 split 을 형제로 collapse. 루트 leaf 면 false.
fn remove_node(node: &mut SurfNode, id: usize) -> bool {
    let SurfNode::Split { first, second, .. } = node else {
        return false;
    };
    let first_is = matches!(first.as_ref(), SurfNode::Leaf(l) if l.id == id);
    if first_is {
        let sibling = std::mem::replace(second.as_mut(), SurfNode::Leaf(Leaf::placeholder()));
        *node = sibling;
        return true;
    }
    let second_is = matches!(second.as_ref(), SurfNode::Leaf(l) if l.id == id);
    if second_is {
        let sibling = std::mem::replace(first.as_mut(), SurfNode::Leaf(Leaf::placeholder()));
        *node = sibling;
        return true;
    }
    remove_node(first, id) || remove_node(second, id)
}

/// `row` → split 방향. is_row 의 역: row(좌우)=Vertical, column(상하)=Horizontal.
fn dir_from_row(row: bool) -> PresetSplitDirection {
    if row {
        PresetSplitDirection::Vertical
    } else {
        PresetSplitDirection::Horizontal
    }
}

/// `params`(serde_json::Value)의 `key` 에 문자열을 write. object 가 아니면(Null 등)
/// object 로 승격한 뒤 삽입 — unknown params 를 통째 갈아치우지 않고 key 만 갱신해
/// round-trip 보존을 유지한다.
fn set_param(params: &mut serde_json::Value, key: &str, value: &str) {
    if !params.is_object() {
        *params = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(obj) = params.as_object_mut() {
        obj.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

/// `params` object 에서 `key` 를 제거(값 부재로 만든다). object 가 아니면 no-op.
fn remove_param(params: &mut serde_json::Value, key: &str) {
    if let Some(obj) = params.as_object_mut() {
        obj.remove(key);
    }
}

fn surf_to_model(node: &SurfNode) -> PresetSurfaceLayout {
    match node {
        SurfNode::Leaf(l) => PresetSurfaceLayout::Leaf {
            surface: PresetSurface {
                id: Some(l.id as u32),
                kind: l.kind.clone(),
                cwd: l.cwd.clone(),
                startup_command: l.startup.clone(),
                params: l.params.clone(),
            },
        },
        SurfNode::Split {
            row,
            ratio,
            first,
            second,
        } => PresetSurfaceLayout::Split {
            direction: dir_from_row(*row),
            ratio: *ratio,
            first: Box::new(surf_to_model(first)),
            second: Box::new(surf_to_model(second)),
        },
    }
}

fn tab_to_model(t: &PreviewTab) -> PresetTab {
    PresetTab {
        explicit_name: t.explicit_name.clone(),
        layout: surf_to_model(&t.layout),
    }
}

fn pane_to_model(p: &PreviewPane) -> PresetPane {
    PresetPane {
        tabs: p.tabs.iter().map(tab_to_model).collect(),
        active_tab: p.active,
    }
}

fn pane_node_to_model(n: &PaneNode) -> PresetPaneNode {
    match n {
        PaneNode::Leaf(p) => PresetPaneNode::Leaf {
            pane: pane_to_model(p),
        },
        PaneNode::Split {
            row,
            ratio,
            first,
            second,
        } => PresetPaneNode::Split {
            direction: dir_from_row(*row),
            ratio: *ratio,
            first: Box::new(pane_node_to_model(first)),
            second: Box::new(pane_node_to_model(second)),
        },
    }
}

/// `rect` 를 비율로 나눈다. divider 만큼을 가운데 띠로 빼고 first/second 분배.
/// 반환 = (first, divider, second).
fn split_rects(
    rect: egui::Rect,
    row: bool,
    ratio: f32,
    divider: f32,
) -> (egui::Rect, egui::Rect, egui::Rect) {
    if row {
        let avail = (rect.width() - divider).max(0.0);
        let fw = avail * ratio;
        let first = egui::Rect::from_min_size(rect.min, egui::vec2(fw, rect.height()));
        let mid = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + fw, rect.min.y),
            egui::vec2(divider, rect.height()),
        );
        let second = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + fw + divider, rect.min.y),
            egui::vec2(avail - fw, rect.height()),
        );
        (first, mid, second)
    } else {
        let avail = (rect.height() - divider).max(0.0);
        let fh = avail * ratio;
        let first = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), fh));
        let mid = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + fh),
            egui::vec2(rect.width(), divider),
        );
        let second = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + fh + divider),
            egui::vec2(rect.width(), avail - fh),
        );
        (first, mid, second)
    }
}

/// 하위 레이아웃(surface split). Leaf = kind 박스, Split = 1px hairline.
fn draw_surf(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    node: &SurfNode,
    cx: &mut DrawCtx<'_>,
) {
    match node {
        SurfNode::Leaf(l) => draw_surface_box(ui, theme, rect, l, cx),
        SurfNode::Split {
            row,
            ratio,
            first,
            second,
        } => {
            let (r1, line, r2) = split_rects(rect, *row, *ratio, theme.border_width.value());
            draw_surf(ui, theme, r1, first, cx);
            ui.painter_at(rect)
                .rect_filled(line, 0.0, theme.border_default().to_egui());
            draw_surf(ui, theme, r2, second, cx);
        }
    }
}

/// surface 종류와 값 요약을 그린다. 편집 중에는 선택 테두리·설정·삭제 손잡이를 표시한다.
/// 더블클릭하면 별도 설정 화면을 연다.
fn draw_surface_box(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    leaf: &Leaf,
    cx: &mut DrawCtx<'_>,
) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_app().to_egui());

    let selected = cx.edit && cx.sel == Some(leaf.id);

    // 선택되지 않은 leaf의 경계는 분할, 중앙은 선택에 사용한다.
    let mut zone: Option<SplitZone> = None;
    if cx.edit {
        let resp = ui.interact(
            rect,
            ui.id().with(("preset_leaf", leaf.id)),
            egui::Sense::click(),
        );
        if !selected
            && rect.width() > 0.0
            && rect.height() > 0.0
            && let Some(pos) = resp.hover_pos()
        {
            let nx = (pos.x - rect.min.x) / rect.width();
            let ny = (pos.y - rect.min.y) / rect.height();
            zone = pick_zone(
                nx,
                ny,
                rect.width(),
                rect.height(),
                z(theme, SPLIT_ZONE_MIN),
            );
        }
        if resp.hovered() {
            let cursor = if zone.is_some() {
                egui::CursorIcon::Crosshair
            } else {
                egui::CursorIcon::PointingHand
            };
            ui.ctx().set_cursor_icon(cursor);
        }
        if resp.double_clicked() && zone.is_none() {
            // 중앙 더블클릭은 설정을 열고 경계의 연속 클릭은 분할을 반복한다.
            cx.act = Some(Act::OpenSettings(leaf.id));
        } else if resp.clicked() {
            match zone {
                Some(z) => {
                    cx.act = Some(Act::Split {
                        id: leaf.id,
                        row: z.row(),
                        before: z.before(),
                    });
                }
                None if !selected => cx.act = Some(Act::Select(leaf.id)),
                None => {}
            }
        }
    }

    draw_leaf_preview(ui, theme, rect, leaf, cx.catalog);

    if selected {
        let w = theme.preset_leaf_selected_ring_width().value();
        ui.painter_at(rect).rect_stroke(
            rect.shrink(w * 0.5),
            0.0,
            egui::Stroke::new(w, theme.accent_primary().to_egui()),
            egui::StrokeKind::Inside,
        );
    } else if cx.edit {
        let w = theme.border_width.value();
        ui.painter_at(rect).rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(w, theme.separator.to_egui_premultiplied()),
            egui::StrokeKind::Inside,
        );
    }

    if let Some(z) = zone {
        draw_split_zone_overlay(ui, theme, rect, z);
    }

    if selected {
        draw_handle_cluster(ui, theme, rect, leaf.id, cx);
    }
}

/// 종류와 채워진 필드 값을 표시한다. 작은 영역은 요약을 숨기고 더 작으면 아이콘만 남긴다.
/// 기준은 LEAF_SUMMARY_MIN_W/H와 LEAF_ICON_ONLY_MIN이다.
fn draw_leaf_preview(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    leaf: &Leaf,
    catalog: &KindCatalog,
) {
    let icon = theme.icon_glyph_size_md;
    let label_h = theme.font_size_caption;
    let gap = theme.spacing_xs;
    let row_h = theme.font_size_caption;

    let short_axis = rect.width().min(rect.height());
    let show_kind = short_axis >= z(theme, LEAF_ICON_ONLY_MIN).value();
    let show_summary = show_kind
        && rect.width() >= z(theme, LEAF_SUMMARY_MIN_W).value()
        && rect.height() >= z(theme, LEAF_SUMMARY_MIN_H).value();

    let rows = if show_summary {
        leaf_summary_rows(leaf, catalog)
    } else {
        Vec::new()
    };

    let mut total = icon;
    if show_kind {
        total += z(theme, LEAF_GAP) + label_h;
    }
    if !rows.is_empty() {
        total += gap + row_h * rows.len() as f32 + gap * (rows.len() as f32 - 1.0);
    }

    let cx_x = LogicalPx(rect.center().x);
    let mut y = LogicalPx(rect.center().y) - total.scaled(0.5);

    paint_icon(
        ui,
        catalog.kind_icon(&leaf.kind),
        egui::pos2(cx_x.value(), (y + icon.scaled(0.5)).value()),
        icon,
        kind_accent(theme, &leaf.kind),
    );
    y += icon;

    if show_kind {
        y += z(theme, LEAF_GAP);
        ui.painter_at(rect).text(
            egui::pos2(cx_x.value(), (y + label_h.scaled(0.5)).value()),
            egui::Align2::CENTER_CENTER,
            &leaf.label,
            tasty_ui_widgets::label_font(label_h.value()),
            theme.text_secondary().to_egui(),
        );
        y += label_h;
    }

    if !rows.is_empty() {
        y += gap;
        let label_font = tasty_ui_widgets::label_font(theme.font_size_micro.value());
        let value_font = egui::FontId::monospace(row_h.value());
        let inner_w = (LogicalPx(rect.width()) - gap.scaled(2.0)).max(LogicalPx(0.0));
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                y += gap;
            }
            let row_cy = y + row_h.scaled(0.5);
            let label_w = LogicalPx(text_width(ui, &row.label, label_font.clone()));
            let avail = (inner_w - label_w - gap).max(LogicalPx(0.0));
            let value = elide_to_width(ui, &row.value, value_font.clone(), avail, row.front_elide);
            let value_w = LogicalPx(text_width(ui, &value, value_font.clone()));
            let line_w = label_w + gap + value_w;
            let start_x = cx_x - line_w.scaled(0.5);
            let p = ui.painter_at(rect);
            p.text(
                egui::pos2(start_x.value(), row_cy.value()),
                egui::Align2::LEFT_CENTER,
                &row.label,
                label_font.clone(),
                theme.preset_leaf_label_fg().to_egui(),
            );
            p.text(
                egui::pos2((start_x + label_w + gap).value(), row_cy.value()),
                egui::Align2::LEFT_CENTER,
                &value,
                value_font.clone(),
                theme.preset_leaf_value_fg().to_egui(),
            );
            y += row_h;
        }
    }
}

/// leaf 미리보기 값 요약의 한 행 — 라벨(소문자 필드 키) + 값 + 앞자름 여부.
#[derive(Clone, Debug, PartialEq)]
struct LeafSummaryRow {
    label: String,
    value: String,
    /// path-like(Dir/FilePath) 필드는 앞자름(경로 꼬리 유지), 그 외는 뒤자름.
    front_elide: bool,
}

/// catalog 순서대로 공백이 아닌 필드만 요약한다. 라벨은 번역명이 아니라 field.id를 사용한다.
fn leaf_summary_rows(leaf: &Leaf, catalog: &KindCatalog) -> Vec<LeafSummaryRow> {
    catalog
        .fields(&leaf.kind)
        .iter()
        .filter_map(|f| {
            let value = field_value(leaf, &f.target);
            if value.trim().is_empty() {
                return None;
            }
            let front_elide = matches!(f.input, PresetFieldInput::Dir | PresetFieldInput::FilePath);
            Some(LeafSummaryRow {
                label: f.id.clone(),
                value,
                front_elide,
            })
        })
        .collect()
}

/// 글자 폭을 재서 줄이고 …를 붙인다. 경로는 앞을, 다른 값은 끝을 줄인다.
fn elide_to_width(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    max_w: LogicalPx,
    front: bool,
) -> String {
    if max_w <= LogicalPx(0.0) {
        return String::new();
    }
    if LogicalPx(text_width(ui, text, font.clone())) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    if front {
        for start in 1..chars.len() {
            let candidate: String = std::iter::once('…')
                .chain(chars[start..].iter().copied())
                .collect();
            if LogicalPx(text_width(ui, &candidate, font.clone())) <= max_w {
                return candidate;
            }
        }
        "…".to_string()
    } else {
        for end in (1..chars.len()).rev() {
            let candidate: String = chars[..end]
                .iter()
                .copied()
                .chain(std::iter::once('…'))
                .collect();
            if LogicalPx(text_width(ui, &candidate, font.clone())) <= max_w {
                return candidate;
            }
        }
        "…".to_string()
    }
}

/// 경계 hover-split 존 — 활성 변. 좌/우 = row(좌우) split, 상/하 = column split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitZone {
    Left,
    Right,
    Top,
    Bottom,
}

impl SplitZone {
    /// 이 존이 만드는 split 방향(row=좌우). 좌/우 존 → row, 상/하 존 → column.
    fn row(self) -> bool {
        matches!(self, SplitZone::Left | SplitZone::Right)
    }
    /// 새 leaf 가 first(좌/상)인지. 좌·상 존은 before(새 leaf 가 first 쪽).
    fn before(self) -> bool {
        matches!(self, SplitZone::Left | SplitZone::Top)
    }
}

/// 가장 가까운 변이 SPLIT_ZONE_EDGE 안에 있으면 분할 영역으로 고른다.
/// 축 길이가 min_axis보다 짧으면 그 방향은 제외해 중앙 선택 공간을 남긴다.
/// min_axis는 UI 배율을 적용한 값을 호출부에서 전달한다.
fn pick_zone(nx: f32, ny: f32, w: f32, h: f32, min_axis: LogicalPx) -> Option<SplitZone> {
    let mut best: Option<(f32, SplitZone)> = None;
    let mut consider = |dist: f32, zone: SplitZone| {
        if dist < SPLIT_ZONE_EDGE && best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, zone));
        }
    };
    if w >= min_axis.value() {
        consider(nx, SplitZone::Left);
        consider(1.0 - nx, SplitZone::Right);
    }
    if h >= min_axis.value() {
        consider(ny, SplitZone::Top);
        consider(1.0 - ny, SplitZone::Bottom);
    }
    best.map(|(_, z)| z)
}

/// 활성 split 존의 밴드(변 쪽 30% 영역)를 `preset_split_zone_bg` 로 채우고, 안쪽
/// 변(분할선이 될 변)에 2px `preset_split_zone_border` 를 그린다. 좌/상 존은 밴드의
/// 우/하 변, 우/하 존은 좌/상 변. pointer 이벤트 없음(`painter_at` clip).
fn draw_split_zone_overlay(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, zone: SplitZone) {
    let bg = theme.preset_split_zone_bg().to_egui();
    let border = theme.preset_split_zone_border().to_egui();
    let divider = theme.preset_split_divider_width().value();
    let stroke = egui::Stroke::new(divider, border);
    let p = ui.painter_at(rect);
    match zone {
        SplitZone::Left => {
            let x = rect.min.x + rect.width() * SPLIT_ZONE_EDGE;
            let band = egui::Rect::from_min_max(rect.min, egui::pos2(x, rect.max.y));
            p.rect_filled(band, 0.0, bg);
            p.vline(x, band.y_range(), stroke);
        }
        SplitZone::Right => {
            let x = rect.max.x - rect.width() * SPLIT_ZONE_EDGE;
            let band = egui::Rect::from_min_max(egui::pos2(x, rect.min.y), rect.max);
            p.rect_filled(band, 0.0, bg);
            p.vline(x, band.y_range(), stroke);
        }
        SplitZone::Top => {
            let y = rect.min.y + rect.height() * SPLIT_ZONE_EDGE;
            let band = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, y));
            p.rect_filled(band, 0.0, bg);
            p.hline(band.x_range(), y, stroke);
        }
        SplitZone::Bottom => {
            let y = rect.max.y - rect.height() * SPLIT_ZONE_EDGE;
            let band = egui::Rect::from_min_max(egui::pos2(rect.min.x, y), rect.max);
            p.rect_filled(band, 0.0, bg);
            p.hline(band.x_range(), y, stroke);
        }
    }
}

/// 오른쪽 위 설정·삭제 손잡이. 삭제가 오른쪽에 놓인다.
fn draw_handle_cluster(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    leaf_id: usize,
    cx: &mut DrawCtx<'_>,
) {
    let remove = egui::Rect::from_min_size(
        egui::pos2(
            rect.max.x - (z(theme, HANDLE_INSET) + z(theme, HANDLE_SZ)).value(),
            rect.min.y + z(theme, HANDLE_INSET).value(),
        ),
        egui::vec2(z(theme, HANDLE_SZ).value(), z(theme, HANDLE_SZ).value()),
    );
    let settings = remove.translate(egui::vec2(
        -(z(theme, HANDLE_SZ) + z(theme, HANDLE_GAP)).value(),
        0.0,
    ));
    if mini_handle(
        ui,
        theme,
        settings,
        icons::SETTINGS,
        false,
        ("cfg", leaf_id),
    )
    .on_hover_text(t("preset.settings.open"))
    .clicked()
    {
        cx.act = Some(Act::OpenSettings(leaf_id));
    }
    if mini_handle(ui, theme, remove, icons::TRASH, true, ("rm", leaf_id)).clicked() {
        cx.act = Some(Act::Remove { id: leaf_id });
    }
}

/// 작은 핸들 버튼. surface-raised bg + border-strong, danger 면 아이콘 danger.
fn mini_handle(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    icon: Icon,
    danger: bool,
    salt: (&'static str, usize),
) -> egui::Response {
    let resp = ui.interact(rect, ui.id().with(salt), egui::Sense::click());
    let radius = theme.corner_radius_sm.value();
    let bg = if resp.hovered() {
        theme.surface_active().to_egui()
    } else {
        theme.surface_raised().to_egui()
    };
    let p = ui.painter_at(rect);
    p.rect(
        rect,
        radius,
        bg,
        egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
        egui::StrokeKind::Inside,
    );
    let color = if danger {
        theme.accent_danger_raised().to_egui()
    } else {
        theme.text_secondary().to_egui()
    };
    let glyph = z(theme, HANDLE_SZ).scaled(0.62);
    paint_icon(ui, icon, rect.center(), glyph, color);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// target(cwd/startup/params[key])에서 현재 문자열 값을 읽는다(부재면 빈 문자열).
fn field_value(leaf: &Leaf, target: &PresetFieldTarget) -> String {
    match target {
        PresetFieldTarget::Cwd => leaf.cwd.clone().unwrap_or_default(),
        PresetFieldTarget::Startup => leaf.startup.clone().unwrap_or_default(),
        PresetFieldTarget::Params(k) => leaf
            .params
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    }
}

/// 상위 레이아웃(pane split). Leaf = pane 카드, Split = `spacing_xs` bg-app gap.
fn draw_pane_tree(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    node: &PaneNode,
    cx: &mut DrawCtx<'_>,
) {
    match node {
        PaneNode::Leaf(pane) => draw_pane_card(ui, theme, rect, pane, cx),
        PaneNode::Split {
            row,
            ratio,
            first,
            second,
        } => {
            let (r1, _gap, r2) = split_rects(rect, *row, *ratio, theme.spacing_xs.value());
            draw_pane_tree(ui, theme, r1, first, cx);
            draw_pane_tree(ui, theme, r2, second, cx);
        }
    }
}

/// pane 카드 = 테두리 카드 + mini tab strip(클릭 가능) + 활성 탭의 surface 레이아웃.
fn draw_pane_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    pane: &PreviewPane,
    cx: &mut DrawCtx<'_>,
) {
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    let sep = theme.separator.to_egui_premultiplied();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, radius, theme.bg_app().to_egui());

    let strip = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width(), z(theme, STRIP_H).value()),
    );
    p.rect_filled(strip, 0.0, theme.bg_sidebar().to_egui());

    let tab_font = egui::FontId::proportional(theme.font_size_caption.value());
    let icon_sz = theme.icon_glyph_size_sm;
    let mut x = LogicalPx(strip.min.x);
    for (i, t) in pane.tabs.iter().enumerate() {
        let on = i == pane.active;
        let lw = LogicalPx(text_width(ui, &t.name, tab_font.clone()));
        // 편집 중이고 탭이 둘 이상일 때만 닫기 버튼 폭을 확보한다.
        let show_close = cx.edit && pane.tabs.len() > 1;
        let tw = if show_close {
            z(theme, TAB_PAD_X)
                + icon_sz
                + theme.spacing_xs
                + lw
                + z(theme, CLOSE_MARGIN)
                + z(theme, CLOSE_HIT)
                + z(theme, CLOSE_TAB_PAD)
        } else {
            z(theme, TAB_PAD_X) + icon_sz + theme.spacing_xs + lw + z(theme, TAB_PAD_X)
        };
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(x.value(), strip.min.y),
            egui::vec2(tw.value(), z(theme, STRIP_H).value()),
        );

        let resp = ui.interact(
            tab_rect,
            ui.id().with(("preset_demo_tab", pane.id, i)),
            egui::Sense::click(),
        );
        if !on && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let rep = t.layout.rep_kind();
        let p = ui.painter_at(strip);
        if on {
            p.rect_filled(tab_rect, 0.0, theme.bg_panel().to_egui());
            let bar = egui::Rect::from_min_size(
                egui::pos2(
                    tab_rect.min.x,
                    tab_rect.max.y - theme.tab_indicator_width().value(),
                ),
                egui::vec2(tw.value(), theme.tab_indicator_width().value()),
            );
            p.rect_filled(bar, 0.0, theme.accent_primary().to_egui());
        }
        if i > 0 {
            p.vline(x.value(), strip.y_range(), egui::Stroke::new(bw, sep));
        }
        let icon_c = egui::pos2(
            tab_rect.min.x + (z(theme, TAB_PAD_X) + icon_sz.scaled(0.5)).value(),
            tab_rect.center().y,
        );
        let icon_color = if on {
            kind_accent(theme, rep)
        } else {
            theme.text_muted().to_egui()
        };
        paint_icon(ui, cx.catalog.kind_icon(rep), icon_c, icon_sz, icon_color);
        ui.painter_at(strip).text(
            egui::pos2(
                tab_rect.min.x + (z(theme, TAB_PAD_X) + icon_sz + theme.spacing_xs).value(),
                tab_rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            &t.name,
            tab_font.clone(),
            if on {
                theme.text_primary().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );

        // close `×` — 편집 && 탭>1 && (active || pointer). hover = overlay_active fill +
        // text_primary, rest = text_muted. contains_pointer 로 × 위 이동 중 소멸 방지.
        let mut close_clicked = false;
        if show_close {
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(
                    tab_rect.max.x - (z(theme, CLOSE_TAB_PAD) + z(theme, CLOSE_HIT)).value(),
                    tab_rect.center().y - z(theme, CLOSE_HIT).scaled(0.5).value(),
                ),
                egui::vec2(z(theme, CLOSE_HIT).value(), z(theme, CLOSE_HIT).value()),
            );
            let close_resp = ui.interact(
                close_rect,
                ui.id().with(("preset_demo_tabclose", pane.id, i)),
                egui::Sense::click(),
            );
            if on || resp.contains_pointer() {
                let hovering = close_resp.hovered();
                if hovering {
                    ui.painter_at(strip).rect_filled(
                        close_rect,
                        theme.corner_radius_sm.value(),
                        theme.overlay_active().to_egui_premultiplied(),
                    );
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let col = if hovering {
                    theme.text_primary().to_egui()
                } else {
                    theme.text_muted().to_egui()
                };
                paint_icon(
                    ui,
                    icons::CLOSE,
                    close_rect.center(),
                    z(theme, CLOSE_HIT).scaled(0.5),
                    col,
                );
            }
            close_clicked = close_resp.clicked();
        }

        // RemoveTab 이 SetActive 보다 우선 — × 클릭이 탭 전환을 유발하지 않게 한다.
        if close_clicked {
            cx.act = Some(Act::RemoveTab {
                pane: pane.id,
                idx: i,
            });
        } else if resp.clicked() {
            cx.act = Some(Act::SetActive {
                pane: pane.id,
                idx: i,
            });
        }

        x += tw;
    }

    if cx.edit {
        let add = egui::Rect::from_min_size(
            egui::pos2(x.value(), strip.min.y),
            egui::vec2(z(theme, ADD_TAB_W).value(), z(theme, STRIP_H).value()),
        );
        let resp = ui.interact(
            add,
            ui.id().with(("preset_demo_addtab", pane.id)),
            egui::Sense::click(),
        );
        if resp.hovered() {
            ui.painter_at(strip).rect_filled(
                add,
                0.0,
                theme.overlay_hover().to_egui_premultiplied(),
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let col = if resp.hovered() {
            theme.text_secondary().to_egui()
        } else {
            theme.text_muted().to_egui()
        };
        paint_icon(ui, icons::PLUS, add.center(), icon_sz, col);
        if resp.clicked() {
            cx.act = Some(Act::AddTab { pane: pane.id });
        }
    }

    ui.painter_at(rect)
        .hline(strip.x_range(), strip.max.y, egui::Stroke::new(bw, sep));

    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, strip.max.y), rect.max);
    let inner = body.shrink(z(theme, BODY_PAD).value());
    if let Some(t) = pane.tabs.get(pane.active).or_else(|| pane.tabs.first()) {
        draw_surf(ui, theme, inner, &t.layout, cx);
    }

    ui.painter_at(rect).rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// Tab scope — strip 없이 단일 탭 본문처럼 프레임(테두리 + radius + padding 3).
fn draw_tab_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    node: &SurfNode,
    cx: &mut DrawCtx<'_>,
) {
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, radius, theme.bg_app().to_egui());
    draw_surf(ui, theme, rect.shrink(z(theme, BODY_PAD).value()), node, cx);
    ui.painter_at(rect).rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// 아이콘 크기는 LogicalPx로 받고 egui에 넘길 때만 숫자로 꺼낸다.
fn paint_icon(
    ui: &mut egui::Ui,
    icon: Icon,
    center: egui::Pos2,
    size: LogicalPx,
    color: egui::Color32,
) {
    let r = egui::Rect::from_center_size(center, egui::vec2(size.value(), size.value()));
    icon.image(size.value(), color).paint_at(ui, r);
}

fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tasty_presets::{PresetPane, PresetSurface, PresetSurfaceLayout, PresetTab};

    fn surf(kind: &str) -> PresetSurfaceLayout {
        PresetSurfaceLayout::Leaf {
            surface: PresetSurface {
                id: None,
                kind: kind.into(),
                cwd: None,
                startup_command: None,
                params: serde_json::Value::Null,
            },
        }
    }

    fn ssplit(
        d: PresetSplitDirection,
        r: f32,
        a: PresetSurfaceLayout,
        b: PresetSurfaceLayout,
    ) -> PresetSurfaceLayout {
        PresetSurfaceLayout::Split {
            direction: d,
            ratio: r,
            first: Box::new(a),
            second: Box::new(b),
        }
    }

    /// 테스트용 resolver — registry 없이 kind 를 그대로 대문자 라벨로.
    fn up(kind: &str) -> String {
        let mut c = kind.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        }
    }

    /// 테스트용 catalog — `EDIT_KINDS` 를 capitalize 라벨로 담아 registry 없이
    /// 결정적으로 kind→표시명을 해석한다(`up` 과 동일 규칙).
    fn tc() -> KindCatalog {
        KindCatalog::from_pairs(EDIT_KINDS.iter().map(|k| (k.to_string(), up(k))).collect())
    }

    #[test]
    fn catalog_candidates_reflect_registered_kinds() {
        let cat = KindCatalog::from_pairs(vec![
            ("terminal".to_string(), "Terminal".to_string()),
            ("foo".to_string(), "Foo Surface".to_string()),
        ]);
        let cands = cat.candidates("terminal");
        assert!(cands.contains(&"foo".to_string()));
        assert!(cands.contains(&"terminal".to_string()));
        let cands = cat.candidates("bar");
        assert!(cands.contains(&"bar".to_string()));
        assert_eq!(cat.label("foo"), "Foo Surface");
        assert_eq!(cat.label("terminal"), "Terminal");
    }

    #[test]
    fn empty_catalog_falls_back_to_static_kinds() {
        let cat = KindCatalog::default();
        let cands = cat.candidates("terminal");
        for k in EDIT_KINDS {
            assert!(cands.contains(&k.to_string()), "missing static kind {k}");
        }
        let cands = cat.candidates("plugin-kind");
        assert!(cands.contains(&"plugin-kind".to_string()));
    }

    #[test]
    fn vertical_split_is_row_horizontal_is_column() {
        assert!(is_row(PresetSplitDirection::Vertical));
        assert!(!is_row(PresetSplitDirection::Horizontal));
    }

    #[test]
    fn normalizes_tab_preset_and_resolves_labels() {
        let p = TabPreset {
            name: "t".into(),
            tab: PresetTab {
                explicit_name: None,
                layout: ssplit(
                    PresetSplitDirection::Vertical,
                    0.5,
                    surf("terminal"),
                    surf("markdown"),
                ),
            },
        };
        let dl = DemoLayout::from_tab(&p, &tc());
        match &dl.root {
            Root::TabFrame(SurfNode::Split {
                row,
                ratio,
                first,
                second,
            }) => {
                assert!(*row);
                assert_eq!(*ratio, 0.5);
                assert!(matches!(first.as_ref(), SurfNode::Leaf(l) if l.label == "Terminal"));
                assert!(matches!(second.as_ref(), SurfNode::Leaf(l) if l.label == "Markdown"));
            }
            _ => panic!("expected TabFrame split"),
        }
    }

    #[test]
    fn tab_name_falls_back_to_rep_kind_label() {
        let pane = PresetPane {
            tabs: vec![PresetTab {
                explicit_name: None,
                layout: ssplit(
                    PresetSplitDirection::Horizontal,
                    0.5,
                    surf("markdown"),
                    surf("terminal"),
                ),
            }],
            active_tab: 0,
        };
        let p = PanePreset {
            name: "p".into(),
            pane,
        };
        let dl = DemoLayout::from_pane(&p, &tc());
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => {
                assert_eq!(pp.tabs[0].name, "Markdown");
            }
            _ => panic!("expected single pane"),
        }
    }

    #[test]
    fn active_tab_is_clamped_to_range() {
        let pane = PresetPane {
            tabs: vec![
                PresetTab {
                    explicit_name: Some("a".into()),
                    layout: surf("terminal"),
                },
                PresetTab {
                    explicit_name: Some("b".into()),
                    layout: surf("terminal"),
                },
            ],
            active_tab: 9, // 범위 밖
        };
        let p = PanePreset {
            name: "p".into(),
            pane,
        };
        let dl = DemoLayout::from_pane(&p, &tc());
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => assert_eq!(pp.active, 1),
            _ => panic!(),
        }
    }

    #[test]
    fn set_active_switches_only_on_real_change() {
        let pane = PresetPane {
            tabs: vec![
                PresetTab {
                    explicit_name: Some("a".into()),
                    layout: surf("terminal"),
                },
                PresetTab {
                    explicit_name: Some("b".into()),
                    layout: surf("markdown"),
                },
            ],
            active_tab: 0,
        };
        let p = PanePreset {
            name: "p".into(),
            pane,
        };
        let mut dl = DemoLayout::from_pane(&p, &tc());
        assert!(dl.set_active(0, 1));
        assert!(!dl.set_active(0, 1));
        assert!(!dl.set_active(99, 0));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => assert_eq!(pp.active, 1),
            _ => panic!(),
        }
    }

    /// 단일 leaf pane preset 빌더 (kind 지정).
    fn single_pane(kind: &str) -> PanePreset {
        PanePreset {
            name: "p".into(),
            pane: PresetPane {
                tabs: vec![PresetTab {
                    explicit_name: None,
                    layout: surf(kind),
                }],
                active_tab: 0,
            },
        }
    }

    /// surf 트리에서 leaf id 를 방문 순서로 수집.
    fn surf_leaf_ids(node: &SurfNode, out: &mut Vec<usize>) {
        match node {
            SurfNode::Leaf(l) => out.push(l.id),
            SurfNode::Split { first, second, .. } => {
                surf_leaf_ids(first, out);
                surf_leaf_ids(second, out);
            }
        }
    }

    fn first_surf(dl: &DemoLayout) -> &SurfNode {
        match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => &p.tabs[0].layout,
            _ => panic!("expected single pane"),
        }
    }

    #[test]
    fn set_kind_updates_leaf_and_label() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let leaf_id = ids[0];

        dl.set_kind(leaf_id, "markdown", &tc());
        match first_surf(&dl) {
            SurfNode::Leaf(l) => {
                assert_eq!(l.kind, "markdown");
                assert_eq!(l.label, "Markdown");
            }
            _ => panic!(),
        }
        let pane = dl.rebuild_single_pane().unwrap();
        match &pane.tabs[0].layout {
            PresetSurfaceLayout::Leaf { surface } => assert_eq!(surface.kind, "markdown"),
            _ => panic!(),
        }
    }

    /// 마크다운 leaf 에 선언 필드(`file`)를 write 하면 params.file 이 채워지고,
    /// 선언에 없는 unknown params(legacy_x)는 kind 미변경 편집에서 round-trip 보존된다.
    #[test]
    fn set_field_writes_param_and_preserves_unknown_params() {
        let pane = PresetPane {
            tabs: vec![PresetTab {
                explicit_name: Some("t".into()),
                layout: PresetSurfaceLayout::Leaf {
                    surface: PresetSurface {
                        id: None,
                        kind: "markdown".into(),
                        cwd: None,
                        startup_command: None,
                        params: serde_json::json!({ "legacy_x": 7 }),
                    },
                },
            }],
            active_tab: 0,
        };
        let mut dl = DemoLayout::from_pane(
            &PanePreset {
                name: "p".into(),
                pane,
            },
            &tc(),
        );
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let leaf_id = ids[0];

        dl.set_field(
            leaf_id,
            &PresetFieldTarget::Params("file".into()),
            "/a/b/x.md".into(),
        );
        let pane = dl.rebuild_single_pane().unwrap();
        let params = match &pane.tabs[0].layout {
            PresetSurfaceLayout::Leaf { surface } => &surface.params,
            _ => panic!(),
        };
        assert_eq!(
            params.get("file").and_then(|v| v.as_str()),
            Some("/a/b/x.md")
        );
        assert_eq!(params.get("legacy_x").and_then(|v| v.as_i64()), Some(7));

        dl.set_field(
            leaf_id,
            &PresetFieldTarget::Params("file".into()),
            String::new(),
        );
        let pane = dl.rebuild_single_pane().unwrap();
        if let PresetSurfaceLayout::Leaf { surface } = &pane.tabs[0].layout {
            assert!(surface.params.get("file").is_none());
            assert_eq!(
                surface.params.get("legacy_x").and_then(|v| v.as_i64()),
                Some(7)
            );
        }
    }

    /// Markdown에서 terminal로 바꾸면 file과 legacy params를 지운다.
    /// Terminal에서도 사용하는 cwd는 유지한다.
    #[test]
    fn set_kind_cleans_stale_params_keeps_used_columns() {
        let pane = PresetPane {
            tabs: vec![PresetTab {
                explicit_name: Some("t".into()),
                layout: PresetSurfaceLayout::Leaf {
                    surface: PresetSurface {
                        id: None,
                        kind: "markdown".into(),
                        cwd: Some("/keep".into()),
                        startup_command: Some("stale".into()),
                        params: serde_json::json!({ "file": "/a/x.md", "legacy": 1 }),
                    },
                },
            }],
            active_tab: 0,
        };
        let mut dl = DemoLayout::from_pane(
            &PanePreset {
                name: "p".into(),
                pane,
            },
            &tc(),
        );
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let leaf_id = ids[0];

        dl.set_kind(leaf_id, "terminal", &tc());
        let pane = dl.rebuild_single_pane().unwrap();
        match &pane.tabs[0].layout {
            PresetSurfaceLayout::Leaf { surface } => {
                assert_eq!(surface.kind, "terminal");
                assert!(
                    surface
                        .params
                        .as_object()
                        .map(|o| o.is_empty())
                        .unwrap_or(true),
                    "stale params must be cleared on kind change: {:?}",
                    surface.params
                );
                assert_eq!(surface.cwd.as_deref(), Some("/keep"));
                assert_eq!(surface.startup_command.as_deref(), Some("stale"));
            }
            _ => panic!(),
        }

        dl.set_kind(leaf_id, "markdown", &tc());
        let pane = dl.rebuild_single_pane().unwrap();
        if let PresetSurfaceLayout::Leaf { surface } = &pane.tabs[0].layout {
            assert_eq!(surface.kind, "markdown");
            assert!(
                surface.cwd.is_none(),
                "cwd cleared when new kind lacks Cwd field"
            );
            assert!(surface.startup_command.is_none());
        }
    }

    #[test]
    fn split_leaf_creates_sibling() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let leaf_id = ids[0];

        dl.split_leaf(leaf_id, true, false, &tc());
        match first_surf(&dl) {
            SurfNode::Split {
                row, first, second, ..
            } => {
                assert!(*row);
                assert!(matches!(first.as_ref(), SurfNode::Leaf(_)));
                assert!(matches!(second.as_ref(), SurfNode::Leaf(_)));
            }
            _ => panic!("expected split after split_leaf"),
        }
        let mut after = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert_ne!(after[0], after[1]);
    }

    /// 저장하고 다시 읽어도 기존·신규 leaf ID가 유지된다.
    #[test]
    fn ids_survive_edit_round_trip() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let orig_id = {
            let mut v = Vec::new();
            surf_leaf_ids(first_surf(&dl), &mut v);
            v[0]
        };

        dl.split_leaf(orig_id, true, false, &tc());
        let mut after = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert!(
            after.contains(&orig_id),
            "기존 leaf id 는 split 후에도 보존"
        );
        let new_id = *after
            .iter()
            .find(|&&i| i != orig_id)
            .expect("distinct new id");

        let pane = dl.rebuild_single_pane().unwrap();
        let reloaded_dl = DemoLayout::from_pane(
            &PanePreset {
                name: "p".into(),
                pane,
            },
            &tc(),
        );
        let mut reloaded = Vec::new();
        surf_leaf_ids(first_surf(&reloaded_dl), &mut reloaded);
        reloaded.sort_unstable();
        let mut expected = vec![orig_id, new_id];
        expected.sort_unstable();
        assert_eq!(reloaded, expected, "재load 후 기존·신규 id 모두 안정");
    }

    #[test]
    fn remove_leaf_collapses_parent_and_guards_sole() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let sole = ids[0];
        assert!(!dl.remove_leaf(sole, &tc()));

        dl.split_leaf(sole, false, false, &tc());
        let mut after = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert!(dl.remove_leaf(after[1], &tc()));
        let mut final_ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut final_ids);
        assert_eq!(final_ids, vec![after[0]]);
    }

    #[test]
    fn add_tab_appends_terminal_and_activates() {
        let mut dl = DemoLayout::from_pane(&single_pane("markdown"), &tc());
        let pane_id = match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => p.id,
            _ => panic!(),
        };
        dl.add_tab(pane_id, &tc());
        match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => {
                assert_eq!(p.tabs.len(), 2);
                assert_eq!(p.active, 1);
                assert_eq!(p.tabs[1].layout.rep_kind(), "terminal");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn panes_get_unique_ids() {
        let p = WorkspacePreset {
            name: "w".into(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Split {
                direction: PresetSplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PresetPaneNode::Leaf {
                    pane: PresetPane {
                        tabs: vec![PresetTab {
                            explicit_name: Some("a".into()),
                            layout: surf("terminal"),
                        }],
                        active_tab: 0,
                    },
                }),
                second: Box::new(PresetPaneNode::Leaf {
                    pane: PresetPane {
                        tabs: vec![PresetTab {
                            explicit_name: Some("b".into()),
                            layout: surf("markdown"),
                        }],
                        active_tab: 0,
                    },
                }),
            },
        };
        let dl = DemoLayout::from_workspace(&p, &tc());
        let mut ids = Vec::new();
        fn collect(node: &PaneNode, ids: &mut Vec<usize>) {
            match node {
                PaneNode::Leaf(p) => ids.push(p.id),
                PaneNode::Split { first, second, .. } => {
                    collect(first, ids);
                    collect(second, ids);
                }
            }
        }
        if let Root::Panes(node) = &dl.root {
            collect(node, &mut ids);
        }
        assert_eq!(ids, vec![0, 1]);
    }

    /// 단일 leaf pane 을 담은 Workspace preset 빌더 (Workspace scope 검증용).
    fn single_pane_workspace(kind: &str) -> WorkspacePreset {
        WorkspacePreset {
            name: "w".into(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Leaf {
                pane: PresetPane {
                    tabs: vec![PresetTab {
                        explicit_name: None,
                        layout: surf(kind),
                    }],
                    active_tab: 0,
                },
            },
        }
    }

    /// pane 트리에서 pane id 를 방문 순서로 수집.
    fn pane_ids(node: &PaneNode, out: &mut Vec<usize>) {
        match node {
            PaneNode::Leaf(p) => out.push(p.id),
            PaneNode::Split { first, second, .. } => {
                pane_ids(first, out);
                pane_ids(second, out);
            }
        }
    }

    fn root_pane(dl: &DemoLayout) -> &PaneNode {
        match &dl.root {
            Root::Panes(n) => n,
            _ => panic!("expected pane root"),
        }
    }

    #[test]
    fn split_pane_creates_sibling_pane_and_rebuilds() {
        let mut dl = DemoLayout::from_workspace(&single_pane_workspace("terminal"), &tc());
        let mut ids = Vec::new();
        pane_ids(root_pane(&dl), &mut ids);
        let pane_id = ids[0];

        assert!(dl.split_pane(pane_id, true, &tc()));
        match root_pane(&dl) {
            PaneNode::Split {
                row, first, second, ..
            } => {
                assert!(*row);
                assert!(matches!(first.as_ref(), PaneNode::Leaf(_)));
                assert!(matches!(second.as_ref(), PaneNode::Leaf(_)));
            }
            _ => panic!("expected pane split after split_pane"),
        }
        let mut after = Vec::new();
        pane_ids(root_pane(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert_ne!(after[0], after[1]);
        assert!(matches!(
            dl.rebuild_pane_node(),
            Some(PresetPaneNode::Split { .. })
        ));

        let mut pane_dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let pid = match &pane_dl.root {
            Root::Panes(PaneNode::Leaf(p)) => p.id,
            _ => panic!(),
        };
        assert!(!pane_dl.split_pane(pid, true, &tc()));
        assert!(matches!(pane_dl.root, Root::Panes(PaneNode::Leaf(_))));
    }

    #[test]
    fn remove_pane_collapses_and_guards_root() {
        let mut dl = DemoLayout::from_workspace(&single_pane_workspace("terminal"), &tc());
        let mut ids = Vec::new();
        pane_ids(root_pane(&dl), &mut ids);
        let sole = ids[0];
        assert!(!dl.remove_pane(sole, &tc()));

        assert!(dl.split_pane(sole, false, &tc()));
        let mut after = Vec::new();
        pane_ids(root_pane(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert!(dl.remove_pane(after[1], &tc()));
        let mut final_ids = Vec::new();
        pane_ids(root_pane(&dl), &mut final_ids);
        assert_eq!(final_ids, vec![after[0]]);
        assert!(matches!(root_pane(&dl), PaneNode::Leaf(_)));
    }

    #[test]
    fn remove_tab_clamps_active_and_guards_last() {
        let pane = PresetPane {
            tabs: vec![
                PresetTab {
                    explicit_name: Some("a".into()),
                    layout: surf("terminal"),
                },
                PresetTab {
                    explicit_name: Some("b".into()),
                    layout: surf("markdown"),
                },
            ],
            active_tab: 1,
        };
        let p = PanePreset {
            name: "p".into(),
            pane,
        };
        let mut dl = DemoLayout::from_pane(&p, &tc());
        let pane_id = match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => pp.id,
            _ => panic!(),
        };

        assert!(dl.remove_tab(pane_id, 1));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => {
                assert_eq!(pp.tabs.len(), 1);
                assert_eq!(pp.active, 0);
            }
            _ => panic!(),
        }

        assert!(!dl.remove_tab(pane_id, 0));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => assert_eq!(pp.tabs.len(), 1),
            _ => panic!(),
        }
    }

    #[test]
    fn pane_id_of_leaf_resolves_owning_pane() {
        let p = WorkspacePreset {
            name: "w".into(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Split {
                direction: PresetSplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PresetPaneNode::Leaf {
                    pane: PresetPane {
                        tabs: vec![PresetTab {
                            explicit_name: Some("a".into()),
                            layout: surf("terminal"),
                        }],
                        active_tab: 0,
                    },
                }),
                second: Box::new(PresetPaneNode::Leaf {
                    pane: PresetPane {
                        tabs: vec![PresetTab {
                            explicit_name: Some("b".into()),
                            layout: surf("markdown"),
                        }],
                        active_tab: 0,
                    },
                }),
            },
        };
        let dl = DemoLayout::from_workspace(&p, &tc());
        assert_eq!(dl.pane_id_of_leaf(0), Some(0));
        assert_eq!(dl.pane_id_of_leaf(1), Some(1));
        assert_eq!(dl.pane_id_of_leaf(99), None);
        let tp = TabPreset {
            name: "t".into(),
            tab: PresetTab {
                explicit_name: None,
                layout: surf("terminal"),
            },
        };
        let tdl = DemoLayout::from_tab(&tp, &tc());
        assert_eq!(tdl.pane_id_of_leaf(0), None);
    }

    #[test]
    fn apply_shortcut_no_selection_is_noop() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let mut sel = None;
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::SplitSurfaceVertical, &mut sel, &tc()),
            ShowOutcome::None
        ));
        assert!(matches!(first_surf(&dl), SurfNode::Leaf(_)));
    }

    #[test]
    fn apply_shortcut_split_surface_targets_selected_leaf() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let leaf_id = {
            let mut ids = Vec::new();
            surf_leaf_ids(first_surf(&dl), &mut ids);
            ids[0]
        };
        let mut sel = Some(leaf_id);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::SplitSurfaceVertical, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        match first_surf(&dl) {
            SurfNode::Split { row, .. } => assert!(*row),
            _ => panic!("expected surface split"),
        }
    }

    #[test]
    fn apply_shortcut_close_surface_clears_selection() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let sole = {
            let mut ids = Vec::new();
            surf_leaf_ids(first_surf(&dl), &mut ids);
            ids[0]
        };
        let mut sel = Some(sole);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::CloseSurface, &mut sel, &tc()),
            ShowOutcome::None
        ));
        assert_eq!(sel, Some(sole));

        dl.split_leaf(sole, true, false, &tc());
        let ids = {
            let mut v = Vec::new();
            surf_leaf_ids(first_surf(&dl), &mut v);
            v
        };
        let new_leaf = ids[1];
        let mut sel = Some(new_leaf);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::CloseSurface, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        assert_eq!(sel, None);
    }

    #[test]
    fn apply_shortcut_new_tab_adds_to_owning_pane() {
        let mut dl = DemoLayout::from_pane(&single_pane("markdown"), &tc());
        let leaf_id = {
            let mut ids = Vec::new();
            surf_leaf_ids(first_surf(&dl), &mut ids);
            ids[0]
        };
        let mut sel = Some(leaf_id);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::NewTab, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => {
                assert_eq!(p.tabs.len(), 2);
                assert_eq!(p.active, 1);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn apply_shortcut_close_active_removes_tab_then_falls_back_to_pane() {
        // 2탭 pane: close_active 는 active 탭 제거. 마지막 남은 탭에서 다시 하면
        // remove_tab 이 no-op → Workspace 단일 pane 이라 remove_pane 도 no-op(루트 가드).
        let pane = PresetPane {
            tabs: vec![
                PresetTab {
                    explicit_name: Some("a".into()),
                    layout: surf("terminal"),
                },
                PresetTab {
                    explicit_name: Some("b".into()),
                    layout: surf("markdown"),
                },
            ],
            active_tab: 0,
        };
        let ws = WorkspacePreset {
            name: "w".into(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Leaf { pane },
        };
        let mut dl = DemoLayout::from_workspace(&ws, &tc());
        let leaf_id = match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => {
                let mut ids = Vec::new();
                surf_leaf_ids(&p.tabs[0].layout, &mut ids);
                ids[0]
            }
            _ => panic!(),
        };
        let mut sel = Some(leaf_id);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::CloseActive, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => assert_eq!(p.tabs.len(), 1),
            _ => panic!(),
        }

        // 남은 leaf 선택 후 close_active → remove_tab no-op → remove_pane 폴백도
        // 루트 단일 pane 이라 no-op → None.
        let last_leaf = match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => {
                let mut ids = Vec::new();
                surf_leaf_ids(&p.tabs[0].layout, &mut ids);
                ids[0]
            }
            _ => panic!(),
        };
        let mut sel = Some(last_leaf);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::CloseActive, &mut sel, &tc()),
            ShowOutcome::None
        ));
    }

    #[test]
    fn apply_shortcut_close_active_last_tab_falls_back_to_pane_removal() {
        // 2-pane workspace 에서, 단일 탭 pane 의 leaf 를 close_active 하면
        // remove_tab no-op → remove_pane 폴백이 성공(형제 pane 존재).
        let mut dl = DemoLayout::from_workspace(&single_pane_workspace("terminal"), &tc());
        let sole_pane = {
            let mut ids = Vec::new();
            pane_ids(root_pane(&dl), &mut ids);
            ids[0]
        };
        assert!(dl.split_pane(sole_pane, true, &tc()));
        let (second_pane, second_leaf) = match root_pane(&dl) {
            PaneNode::Split { second, .. } => match second.as_ref() {
                PaneNode::Leaf(p) => {
                    let mut ids = Vec::new();
                    surf_leaf_ids(&p.tabs[0].layout, &mut ids);
                    (p.id, ids[0])
                }
                _ => panic!(),
            },
            _ => panic!(),
        };
        let mut sel = Some(second_leaf);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::CloseActive, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        assert!(matches!(root_pane(&dl), PaneNode::Leaf(_)));
        assert_eq!(sel, None);
        assert!(!dl.contains_leaf(second_leaf));
        let _ = second_pane;
    }

    #[test]
    fn apply_shortcut_split_pane_only_in_workspace_scope() {
        let mut ws = DemoLayout::from_workspace(&single_pane_workspace("terminal"), &tc());
        let leaf_id = {
            let mut ids = Vec::new();
            surf_leaf_ids(
                match &ws.root {
                    Root::Panes(PaneNode::Leaf(p)) => &p.tabs[0].layout,
                    _ => panic!(),
                },
                &mut ids,
            );
            ids[0]
        };
        let mut sel = Some(leaf_id);
        assert!(matches!(
            ws.apply_shortcut(ShortcutAction::SplitPaneVertical, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        assert!(matches!(root_pane(&ws), PaneNode::Split { .. }));

        let mut pane_dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let pleaf_id = {
            let mut ids = Vec::new();
            surf_leaf_ids(first_surf(&pane_dl), &mut ids);
            ids[0]
        };
        let mut sel = Some(pleaf_id);
        assert!(matches!(
            pane_dl.apply_shortcut(ShortcutAction::SplitPaneHorizontal, &mut sel, &tc()),
            ShowOutcome::None
        ));
        assert!(matches!(pane_dl.root, Root::Panes(PaneNode::Leaf(_))));
    }

    #[test]
    fn apply_shortcut_close_pane_collapses_and_clears_selection() {
        let mut dl = DemoLayout::from_workspace(&single_pane_workspace("terminal"), &tc());
        let sole = {
            let mut ids = Vec::new();
            pane_ids(root_pane(&dl), &mut ids);
            ids[0]
        };
        assert!(dl.split_pane(sole, false, &tc()));
        let second_leaf = match root_pane(&dl) {
            PaneNode::Split { second, .. } => match second.as_ref() {
                PaneNode::Leaf(p) => {
                    let mut ids = Vec::new();
                    surf_leaf_ids(&p.tabs[0].layout, &mut ids);
                    ids[0]
                }
                _ => panic!(),
            },
            _ => panic!(),
        };
        let mut sel = Some(second_leaf);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::ClosePane, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
        assert!(matches!(root_pane(&dl), PaneNode::Leaf(_)));
        assert_eq!(sel, None);
    }

    #[test]
    fn apply_shortcut_tab_scope_pane_actions_are_noop() {
        // Tab scope: pane 없음 → new_tab/split_pane/close_pane/close_active 모두 no-op.
        let tp = TabPreset {
            name: "t".into(),
            tab: PresetTab {
                explicit_name: None,
                layout: surf("terminal"),
            },
        };
        let mut dl = DemoLayout::from_tab(&tp, &tc());
        let leaf_id = match &dl.root {
            Root::TabFrame(SurfNode::Leaf(l)) => l.id,
            _ => panic!(),
        };
        for action in [
            ShortcutAction::NewTab,
            ShortcutAction::CloseActive,
            ShortcutAction::SplitPaneVertical,
            ShortcutAction::ClosePane,
        ] {
            let mut sel = Some(leaf_id);
            assert!(
                matches!(
                    dl.apply_shortcut(action, &mut sel, &tc()),
                    ShowOutcome::None
                ),
                "expected no-op for {action:?} in Tab scope"
            );
        }
        let mut sel = Some(leaf_id);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::SplitSurfaceVertical, &mut sel, &tc()),
            ShowOutcome::Mutated
        ));
    }

    #[test]
    fn split_leaf_before_places_new_leaf_first() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let leaf_id = ids[0];

        dl.split_leaf(leaf_id, true, true, &tc());
        match first_surf(&dl) {
            SurfNode::Split { first, second, .. } => {
                assert!(matches!(second.as_ref(), SurfNode::Leaf(l) if l.id == leaf_id));
                assert!(matches!(first.as_ref(), SurfNode::Leaf(l) if l.id != leaf_id));
            }
            _ => panic!("expected split after split_leaf"),
        }
    }

    #[test]
    fn pick_zone_edges_center_and_degrade() {
        assert_eq!(pick_zone(0.5, 0.5, 200.0, 200.0, SPLIT_ZONE_MIN), None);
        assert_eq!(
            pick_zone(0.1, 0.5, 200.0, 200.0, SPLIT_ZONE_MIN),
            Some(SplitZone::Left)
        );
        // nx=0.3 정확히 경계(미만 아님) → 활성 아님.
        assert_eq!(pick_zone(0.3, 0.5, 200.0, 200.0, SPLIT_ZONE_MIN), None);
        assert_eq!(
            pick_zone(0.5, 0.95, 200.0, 200.0, SPLIT_ZONE_MIN),
            Some(SplitZone::Bottom)
        );
        // 폭 46px 미만 → 좌우 밴드 소멸. ny=0.5 면 상하도 비활성 → 중앙 선택 가능.
        assert_eq!(pick_zone(0.05, 0.5, 40.0, 200.0, SPLIT_ZONE_MIN), None);
        // 같은 좁은 leaf 라도 상단이면 Top 은 유효(짧은 축은 폭뿐, 높이는 충분).
        assert_eq!(
            pick_zone(0.05, 0.1, 40.0, 200.0, SPLIT_ZONE_MIN),
            Some(SplitZone::Top)
        );
    }

    #[test]
    fn split_zone_row_and_before_mapping() {
        // 좌/우 = row(좌우) split, 상/하 = column. 좌·상 = before(새 leaf first).
        assert!(SplitZone::Left.row() && SplitZone::Left.before());
        assert!(SplitZone::Right.row() && !SplitZone::Right.before());
        assert!(!SplitZone::Top.row() && SplitZone::Top.before());
        assert!(!SplitZone::Bottom.row() && !SplitZone::Bottom.before());
    }

    #[test]
    fn zone_before_split_preserves_existing_leaf_id() {
        let mut dl = DemoLayout::from_pane(&single_pane("terminal"), &tc());
        let mut ids = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut ids);
        let leaf_id = ids[0];

        let mut sel = Some(leaf_id);
        let out = dl.dispatch(
            Act::Split {
                id: leaf_id,
                row: true,
                before: true,
            },
            &mut sel,
            &tc(),
        );
        assert!(matches!(out, ShowOutcome::Mutated));
        match first_surf(&dl) {
            SurfNode::Split { first, second, .. } => {
                assert!(matches!(second.as_ref(), SurfNode::Leaf(l) if l.id == leaf_id));
                assert!(matches!(first.as_ref(), SurfNode::Leaf(l) if l.id != leaf_id));
            }
            _ => panic!("expected split"),
        }
        let mut after = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut after);
        assert!(after.contains(&leaf_id));
    }

    /// leaf 를 직접 만드는 테스트 헬퍼(요약 대상 선정 검증용).
    fn leaf_of(kind: &str, cwd: Option<&str>, startup: Option<&str>) -> Leaf {
        Leaf {
            id: 1,
            kind: kind.into(),
            label: up(kind),
            cwd: cwd.map(str::to_string),
            startup: startup.map(str::to_string),
            params: serde_json::Value::Null,
        }
    }

    /// 터미널 leaf 요약은 값이 채워진 필드만 `field.id` 라벨로 산출한다 —
    /// cwd 만 채우고 startup 을 비우면 startup 행이 빠진다.
    #[test]
    fn summary_rows_include_only_nonempty_fields_with_id_labels() {
        let leaf = leaf_of("terminal", Some("/x"), None);
        let rows = leaf_summary_rows(&leaf, &tc());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "cwd"); // field.id, 편집 폼 헤더 아님
        assert_eq!(rows[0].value, "/x");
        assert!(rows[0].front_elide);
    }

    /// cwd·startup 둘 다 채우면 등록 순서대로 두 행, startup 은 뒤자름(command).
    #[test]
    fn summary_rows_keep_field_order_and_elide_direction() {
        let leaf = leaf_of("terminal", Some("/work/dir"), Some("cargo run"));
        let rows = leaf_summary_rows(&leaf, &tc());
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].label.as_str(), rows[0].front_elide), ("cwd", true));
        assert_eq!(
            (rows[1].label.as_str(), rows[1].front_elide),
            ("startup", false) // Text 입력 → 뒤자름
        );
    }

    /// 공백만 있는 값은 행에서 제외한다(placeholder 표시 안 함).
    #[test]
    fn summary_rows_drop_blank_values() {
        let leaf = leaf_of("terminal", Some("   "), Some(""));
        let rows = leaf_summary_rows(&leaf, &tc());
        assert!(rows.is_empty());
    }

    /// markdown leaf 의 params.file 은 `file` 라벨 + FilePath 앞자름으로 요약된다.
    #[test]
    fn summary_rows_read_params_file_as_front_elide() {
        let leaf = Leaf {
            id: 1,
            kind: "markdown".into(),
            label: "Markdown".into(),
            cwd: None,
            startup: None,
            params: serde_json::json!({ "file": "/a/b/readme.md" }),
        };
        let rows = leaf_summary_rows(&leaf, &tc());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "file");
        assert_eq!(rows[0].value, "/a/b/readme.md");
        assert!(rows[0].front_elide); // FilePath → 앞자름
    }

    /// html leaf 의 params.url 은 `url` 라벨 + 뒤자름(Url 입력)으로 요약된다.
    #[test]
    fn summary_rows_read_params_url_as_end_elide() {
        let leaf = Leaf {
            id: 1,
            kind: "html".into(),
            label: "Html".into(),
            cwd: None,
            startup: None,
            params: serde_json::json!({ "url": "https://example.com/path" }),
        };
        let rows = leaf_summary_rows(&leaf, &tc());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "url");
        assert!(!rows[0].front_elide); // Url → 뒤자름
    }
}
