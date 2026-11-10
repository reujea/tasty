//! 구조 digest: CoreState와 구조 journal 모델이 같은 구조를 나타내는지 비교하는 시험 전용 도구.
//!
//! 두 쪽을 같은 정규 표현([`Canonical`])으로 옮긴 뒤 비교한다. 정규 표현은 category·workspace 순서,
//! 이름, 소속, pane·surface 분할 트리(방향·비율 비트), tab 순서와 이름, surface kind, workspace
//! 부제·설명·attach 매핑, tab 명시 이름, metadata, surface 저장 자료를 담는다. 선택·포커스·접힘과
//! 파생 캐시는 담지 않는다([`DIGEST_EXCLUDED`]).
//!
//! - ID는 그대로 두거나([`Canonical`] 기본) [`Canonical::positional`]로 순회 순서 번호로 바꾼다.
//!   importer는 새 ID를 받으므로 CoreState와 가져온 모델을 비교할 때는 순서 번호를 쓴다.
//! - surface 저장 자료는 [`ResolveData`]가 내용으로 바꾼다. payload 번호가 아니라 내용을 비교하므로
//!   다른 journal의 같은 자료도 같다. [`Canonical::without_data`]는 자료를 비교에서 뺀다.
//! - [`Canonical::digest`]는 정규 표현 직렬화의 FNV-1a 64비트 해시다. 차이를 찾을 때는
//!   [`differences`]로 경로별 차이를 본다.
//!
//! Committed projection uses the same pure comparison as import diagnostics.

pub mod live;

use std::collections::{BTreeMap, BTreeSet};

use crate::{DataRef, JournalModel, SplitTree};
use serde::Serialize;
use tasty_model::SplitDirection;

/// 비교에서 뺀 CoreState 자료와 그 이유. 문서의 제외 목록과 같다.
pub const DIGEST_EXCLUDED: &[(&str, &str)] = &[
    (
        "SurfaceLayout::Split.node_id",
        "process-local compatibility projection identity",
    ),
    (
        "Workspace.mirror",
        "mirror workspaces are remote structure, not local",
    ),
    ("JournalModel.applied", "journal position, not structure"),
    (
        "JournalModel.remote_rows",
        "saved remote rows point at remote structure, not local",
    ),
];

/// CoreState와 가져온 journal 모델 사이의 알려진 불일치. 문서의 목록과 같다.
/// 판정은 CoreState 쪽이 왼쪽인 [`Difference`]에 대해 한다.
pub const KNOWN_MISMATCHES: &[KnownMismatch] = &[KnownMismatch {
    id: "unregistered-surface-kind",
    reason: "layout capture saves a surface of an unregistered kind as kind empty",
    applies: |d, kept| {
        d.field() == "kind"
            && d.right == "empty"
            && d.left
                .as_str()
                .is_some_and(|kind| kind != "empty" && !kept(kind))
    },
}];

/// capture가 kind를 그대로 저장하는지 답한다. 판정 원본은 CoreState 쪽이 준다.
pub type KeptKind<'a> = &'a dyn Fn(&str) -> bool;

#[derive(Debug, Clone, Copy)]
pub struct KnownMismatch {
    pub id: &'static str,
    pub reason: &'static str,
    pub applies: fn(&Difference, KeptKind<'_>) -> bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IdMode {
    /// 원래 ID.
    Exact,
    /// 순회 순서 번호. category·workspace는 표시 순서, pane·tab·surface는 전체 깊이 우선 순서다.
    Positional,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Canonical {
    pub ids: IdMode,
    pub categories: Vec<CanonCategory>,
    pub workspaces: Vec<CanonWorkspace>,
    /// 순서 목록에서 닿지 않는 항목과 부모 역참조가 맞지 않는 항목. 원래 ID로 적는다.
    pub defects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanonCategory {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanonWorkspace {
    pub id: u32,
    pub name: String,
    pub category: u32,
    pub subtitle: String,
    pub description: String,
    pub attach_mapping: Option<serde_json::Value>,
    pub metadata: BTreeMap<String, String>,
    pub layout: CanonTree,
    /// layout의 leaf 순서.
    pub panes: Vec<CanonPane>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanonPane {
    pub id: u32,
    pub tabs: Vec<CanonTab>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanonTab {
    pub id: u32,
    pub name: String,
    pub explicit_name: Option<String>,
    pub layout: CanonTree,
    /// layout의 leaf 순서.
    pub surfaces: Vec<CanonSurface>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CanonSurface {
    pub id: u32,
    pub kind: String,
    pub data: CanonData,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CanonTree {
    Leaf(u32),
    Split {
        direction: &'static str,
        /// f32 비트. 값이 같아도 비트가 다르면 다르다.
        ratio: u32,
        first: Box<CanonTree>,
        second: Box<CanonTree>,
    },
}

/// surface 저장 자료의 내용.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum CanonData {
    /// 이 비교에서 뺐다.
    NotCompared,
    /// 자료가 없다.
    Absent,
    /// 바이트 길이와 FNV-1a 64비트 해시.
    Bytes { len: usize, hash: String },
    /// kind 소유자가 해석한 내용. 객체 키는 정렬한다.
    Decoded(serde_json::Value),
    /// 읽지 못했다.
    Unreadable(String),
}

/// 자료 참조를 내용으로 바꾼다.
pub trait ResolveData {
    fn resolve(&self, data: DataRef) -> CanonData;

    /// 자료가 없는 surface도 포함한다. 자료를 비교하지 않으면 없는 자료도 비교하지 않는다.
    fn resolve_optional(&self, data: Option<DataRef>) -> CanonData {
        data.map_or(CanonData::Absent, |d| self.resolve(d))
    }
}

/// 자료를 비교에서 뺀다.
pub struct SkipData;

impl ResolveData for SkipData {
    fn resolve(&self, _data: DataRef) -> CanonData {
        CanonData::NotCompared
    }

    fn resolve_optional(&self, _data: Option<DataRef>) -> CanonData {
        CanonData::NotCompared
    }
}

pub fn direction_name(direction: SplitDirection) -> &'static str {
    match direction {
        SplitDirection::Horizontal => "horizontal",
        SplitDirection::Vertical => "vertical",
    }
}

/// JSON 값의 객체 키를 정렬한다. serde_json의 순서 보존 기능이 켜져 있어도 같은 값이 같은 바이트가 된다.
pub fn sorted_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<&String, serde_json::Value> =
                map.iter().map(|(k, v)| (k, sorted_value(v))).collect();
            serde_json::Value::Object(sorted.into_iter().map(|(k, v)| (k.clone(), v)).collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sorted_value).collect())
        }
        other => other.clone(),
    }
}

pub fn fnv_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn tree<Id: Copy + Into<u32>>(node: &SplitTree<Id>) -> CanonTree {
    match node {
        SplitTree::Leaf(id) => CanonTree::Leaf((*id).into()),
        SplitTree::Split {
            direction,
            ratio,
            first,
            second,
        } => CanonTree::Split {
            direction: direction_name(*direction),
            ratio: ratio.bits(),
            first: Box::new(tree(first)),
            second: Box::new(tree(second)),
        },
    }
}

impl Canonical {
    /// journal 모델 하나(엔진 stream 하나)의 정규 표현. ID는 원래 값이다.
    pub fn of_journal(model: &JournalModel, data: &dyn ResolveData) -> Self {
        let mut defects = Vec::new();
        let mut seen_categories = BTreeSet::new();
        let categories = model
            .category_order
            .iter()
            .map(|id| {
                seen_categories.insert(*id);
                let name = match model.categories.get(id) {
                    Some(category) => category.name.clone(),
                    None => {
                        defects.push(format!("category {id} is ordered but missing"));
                        String::new()
                    }
                };
                CanonCategory { id: *id, name }
            })
            .collect();
        for id in model.categories.keys() {
            if !seen_categories.contains(id) {
                defects.push(format!("category {id} is not ordered"));
            }
        }

        let mut seen_panes = BTreeSet::new();
        let mut seen_tabs = BTreeSet::new();
        let mut seen_surfaces = BTreeSet::new();
        let mut workspaces = Vec::new();
        for ws_id in &model.workspace_order {
            let Some(ws) = model.workspaces.get(ws_id) else {
                defects.push(format!("workspace {ws_id} is ordered but missing"));
                continue;
            };
            let mut panes = Vec::new();
            for pane_id in ws.layout.leaves() {
                seen_panes.insert(pane_id);
                let Some(pane) = model.panes.get(&pane_id) else {
                    defects.push(format!("pane {pane_id} is in a layout but missing"));
                    continue;
                };
                if pane.workspace != *ws_id {
                    defects.push(format!(
                        "pane {pane_id} is in workspace {ws_id} but names {}",
                        pane.workspace
                    ));
                }
                let mut tabs = Vec::new();
                for tab_id in &pane.tabs {
                    seen_tabs.insert(*tab_id);
                    let Some(tab) = model.tabs.get(tab_id) else {
                        defects.push(format!("tab {tab_id} is listed but missing"));
                        continue;
                    };
                    if tab.pane != pane_id {
                        defects.push(format!(
                            "tab {tab_id} is in pane {pane_id} but names {}",
                            tab.pane
                        ));
                    }
                    let mut surfaces = Vec::new();
                    for surface_id in tab.layout.leaves() {
                        seen_surfaces.insert(surface_id);
                        let Some(surface) = model.surfaces.get(&surface_id) else {
                            defects
                                .push(format!("surface {surface_id} is in a layout but missing"));
                            continue;
                        };
                        if surface.tab != *tab_id {
                            defects.push(format!(
                                "surface {surface_id} is in tab {tab_id} but names {}",
                                surface.tab
                            ));
                        }
                        surfaces.push(CanonSurface {
                            id: surface_id,
                            kind: surface.kind.clone(),
                            data: data.resolve_optional(surface.data),
                            metadata: surface.metadata.clone(),
                        });
                    }
                    tabs.push(CanonTab {
                        id: *tab_id,
                        name: tab.name.clone(),
                        explicit_name: tab.explicit_name.clone(),
                        layout: tree(&tab.layout),
                        surfaces,
                    });
                }
                panes.push(CanonPane { id: pane_id, tabs });
            }
            workspaces.push(CanonWorkspace {
                id: *ws_id,
                name: ws.name.clone(),
                category: ws.category,
                subtitle: ws.subtitle.clone(),
                description: ws.description.clone(),
                attach_mapping: ws.attach_mapping.as_ref().map(|m| {
                    sorted_value(&serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                }),
                metadata: ws.metadata.clone(),
                layout: tree(&ws.layout),
                panes,
            });
        }
        let ordered: BTreeSet<_> = model.workspace_order.iter().collect();
        for id in model.workspaces.keys() {
            if !ordered.contains(id) {
                defects.push(format!("workspace {id} is not ordered"));
            }
        }
        for id in model.panes.keys().filter(|id| !seen_panes.contains(*id)) {
            defects.push(format!("pane {id} is unreachable"));
        }
        for id in model.tabs.keys().filter(|id| !seen_tabs.contains(*id)) {
            defects.push(format!("tab {id} is unreachable"));
        }
        for id in model
            .surfaces
            .keys()
            .filter(|id| !seen_surfaces.contains(*id))
        {
            defects.push(format!("surface {id} is unreachable"));
        }
        Self {
            ids: IdMode::Exact,
            categories,
            workspaces,
            defects,
        }
    }

    /// ID를 순회 순서 번호로 바꾼다. 트리 leaf와 workspace의 category도 같은 번호로 옮긴다.
    pub fn positional(&self) -> Self {
        let mut out = self.clone();
        out.ids = IdMode::Positional;
        let category_ids: BTreeMap<u32, u32> = self
            .categories
            .iter()
            .enumerate()
            .map(|(index, c)| (c.id, index as u32))
            .collect();
        for (index, category) in out.categories.iter_mut().enumerate() {
            category.id = index as u32;
        }
        let (mut pane_n, mut tab_n, mut surface_n) = (0u32, 0u32, 0u32);
        for (index, ws) in out.workspaces.iter_mut().enumerate() {
            ws.id = index as u32;
            // 목록에 없는 category는 목록 뒤 번호로 두어 기존 번호와 섞이지 않게 한다.
            ws.category = category_ids.get(&ws.category).copied().unwrap_or(u32::MAX);
            let mut panes = BTreeMap::new();
            for pane in &mut ws.panes {
                panes.insert(pane.id, pane_n);
                pane.id = pane_n;
                pane_n += 1;
                for tab in &mut pane.tabs {
                    tab.id = tab_n;
                    tab_n += 1;
                    let mut surfaces = BTreeMap::new();
                    for surface in &mut tab.surfaces {
                        surfaces.insert(surface.id, surface_n);
                        surface.id = surface_n;
                        surface_n += 1;
                    }
                    relabel(&mut tab.layout, &surfaces);
                }
            }
            relabel(&mut ws.layout, &panes);
        }
        out
    }

    /// surface 저장 자료를 비교에서 뺀다.
    pub fn without_data(&self) -> Self {
        let mut out = self.clone();
        for surface in out
            .workspaces
            .iter_mut()
            .flat_map(|w| w.panes.iter_mut())
            .flat_map(|p| p.tabs.iter_mut())
            .flat_map(|t| t.surfaces.iter_mut())
        {
            surface.data = CanonData::NotCompared;
        }
        out
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("canonical form is plain data")
    }

    /// 정규 표현 직렬화의 해시. 같은 구조면 같은 값이다.
    pub fn digest(&self) -> String {
        fnv_hex(&serde_json::to_vec(self).expect("canonical form is plain data"))
    }
}

/// 트리 leaf를 새 번호로 바꾼다. 목록에 없는 leaf는 그대로 둔다.
fn relabel(node: &mut CanonTree, ids: &BTreeMap<u32, u32>) {
    match node {
        CanonTree::Leaf(id) => {
            if let Some(new) = ids.get(id) {
                *id = *new;
            }
        }
        CanonTree::Split { first, second, .. } => {
            relabel(first, ids);
            relabel(second, ids);
        }
    }
}

/// 정규 표현 한 경로의 차이. `left`·`right`는 [`differences`]에 준 순서다.
#[derive(Debug, Clone, PartialEq)]
pub struct Difference {
    pub path: String,
    pub left: serde_json::Value,
    pub right: serde_json::Value,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} != {}", self.path, self.left, self.right)
    }
}

impl Difference {
    /// 경로의 마지막 필드 이름.
    pub fn field(&self) -> &str {
        self.path.rsplit('.').next().unwrap_or_default()
    }
}

/// 두 정규 표현의 경로별 차이. 같으면 비어 있다. 길이가 다른 목록은 목록 전체를 한 차이로 본다.
pub fn differences(left: &Canonical, right: &Canonical) -> Vec<Difference> {
    let mut out = Vec::new();
    diff_value("", &left.to_value(), &right.to_value(), &mut out);
    out
}

fn diff_value(
    path: &str,
    left: &serde_json::Value,
    right: &serde_json::Value,
    out: &mut Vec<Difference>,
) {
    use serde_json::Value;
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let null = Value::Null;
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                diff_value(
                    &child,
                    a.get(key).unwrap_or(&null),
                    b.get(key).unwrap_or(&null),
                    out,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                diff_value(&format!("{path}[{index}]"), x, y, out);
            }
        }
        _ if left != right => out.push(Difference {
            path: path.to_owned(),
            left: left.clone(),
            right: right.clone(),
        }),
        _ => {}
    }
}

/// CoreState 쪽을 왼쪽, 가져온 journal 쪽을 오른쪽에 둔 차이가 알려진 불일치인지.
/// `kept`는 capture가 그 kind를 그대로 저장하는지 답한다. 그대로 저장하는 kind가 empty가 됐다면
/// 알려진 불일치가 아니라 회귀다.
pub fn known_mismatch(
    difference: &Difference,
    kept: KeptKind<'_>,
) -> Option<&'static KnownMismatch> {
    KNOWN_MISMATCHES
        .iter()
        .find(|k| (k.applies)(difference, kept))
}
