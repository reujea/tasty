//! Preset 데모 레이아웃 미리보기 위젯 (본체). read-only 미리보기(`show`)와
//! 편집 모드(`show_edit` — 선택·핸들 클러스터(설정·remove)·키보드 단축키)를
//! 모두 지원한다. leaf 파라미터는 칸 안에서 고치지 않는다 — 설정 핸들·더블클릭이
//! [`ShowOutcome::OpenSettings`] 를 돌려주고, 편집은 [`LeafDraft`] 를 받은 설정 화면
//! (`preset/surface_settings.rs`)이 한다.
//!
//! 저장된 `Preset*` 트리를 받아 **구조만** 축소 렌더한다 — pane split(상위) /
//! tab strip / surface split(하위) / surface leaf(kind 표시명)을 서로 다른 시각
//! weight 로 그려 계층이 라벨 없이 읽히게 한다. 라이브 surface 렌더(터미널 GPU
//! 패스 / WebView)는 **재사용하지 않는다** — 전용 placeholder 위젯이다.
//!
//! 갤러리 specimen `crates/tasty-gallery/src/catalog/components/preset_editor.rs`
//! 의 시각 구조를 본체 데이터·아이콘으로 1:1 대응시킨다. 차이점:
//!  - 입력이 정적 샘플이 아니라 실제 `WorkspacePreset`/`TabPreset`/`PanePreset`.
//!  - leaf 라벨을 주입된 resolver(런타임 kind→표시명)로 해석.
//!  - mini-tab 이 **live** — 클릭 시 미리보기의 active 탭만 바꾼다(저장본 불변).
//!
//! 3종 구조 레벨의 시각 weight:
//!  - Pane split (상위) → 테두리 카드 + **5px bg-app gap** (무거운 divider).
//!  - Surface split (하위) → **1px border-default hairline** (가벼운 divider).
//!  - Surface leaf → kind 아이콘(accent) + 표시명(가운데, mono). 내용 렌더 안 함.
//!  - Mini tab strip → 20px, bg-sidebar. 활성 = bg-panel + 2px accent 하단 bar + kind 아이콘.

use tasty_presets::{
    LayoutPreset, PanePreset, PresetPane, PresetPaneNode, PresetSplitDirection, PresetSurface,
    PresetSurfaceLayout, PresetTab, TabPreset, WorkspacePreset,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::adapters::ui::icons::{self, Icon};
use crate::core::surface_registry::{
    PresetFieldInput, PresetFieldSpec, PresetFieldTarget, SurfaceKindRegistry,
};
use crate::i18n::t;

mod surface_draft;
pub use surface_draft::{LeafDraft, LeafLocation};

// 미리보기 치수를 배율에 올리는 공용 헬퍼. 이 파일 전용이 아니라
// `adapters::ui` 가 들고 있다 — 같은 사정의 자리가 여럿이라 사본을 두지 않는다.
use crate::adapters::ui::zoomed_px as z;

// 디자인 고정 px (Theme 에 대응 토큰 없는 preview 전용 치수 — specimen 과 동일).
//
// **값은 배율 1 기준이고, 쓸 때는 [`z`] 로 UI 배율을 올린다.** 대응 토큰이 없다는 것이
// 리터럴로 두어도 된다는 뜻은 아니다 — 이 미리보기 안의 글자·아이콘은 `Theme` 에서
// 오므로 배율을 타는데 그것들을 담는 상자만 안 타면 1.2 에서 20px 띠 안에 17px 아이콘이
// 들어간다. 같은 사정의 선례가 `tasty-type-appearance` 의 "컴포넌트 치수(디자인 export 에
// 아직 토큰이 없는 자리)" 블록이고, 판정 규칙은 ADR-0126 "그릇과 내용은 같은 편이어야
// 한다" 다. 갤러리 쌍둥이(`preset_editor.rs`)는 전역 zoom 을 쓰므로 이 처리를 하지
// **않는다** — 거기선 리터럴도 함께 커진다.
/// 상위(pane) divider = bordered 카드 사이 bg-app 공백.
const PANE_GAP: LogicalPx = LogicalPx(5.0);
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
/// mini tab 아이콘↔라벨 gap.
const TAB_GAP: LogicalPx = LogicalPx(5.0);
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

/// registry 미주입 컨텍스트(갤러리·테스트·main 부재)에서 쓰는 정적 kind 후보 +
/// builtin 정렬 기준. registry 가 주입되면 [`KindCatalog::from_registry`] 가 실제
/// 등록 kind 로 대체하고, 이 배열은 빈 catalog 의 graceful fallback 으로만 남는다.
/// `empty` 는 제외(capture/apply 정규화 정책과 정합).
const EDIT_KINDS: &[&str] = &["terminal", "markdown", "image", "explorer", "html"];

/// 편집기 kind 드롭다운에서 숨길 시스템 kind. `empty` 는 사용자가
/// 직접 만들 수 없는 내부 상태라 capture/apply 정규화와 정합하게 후보에서 제외한다.
const HIDDEN_EDIT_KINDS: &[&str] = &["empty"];

/// registry 에서 파생한 경량 kind 스냅샷 — 편집기 렌더/mutation 이 engine 타입에
/// 의존하지 않도록 순수 데이터만 담는다(build 시 1회 추출, `방식 B`).
///
/// `specs` 는 편집 드롭다운 후보(등록 순서, `HIDDEN_EDIT_KINDS` 제외)와 각 kind 의
/// 해석된 표시명 쌍이다. 비어 있으면 registry 미주입으로 보고 정적 [`EDIT_KINDS`]
/// 로 graceful fallback 한다.
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
    pub fn from_registry(registry: &SurfaceKindRegistry) -> Self {
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

    /// 테스트/데모용 — (kind, label) 쌍에서 직접 catalog 구성. 필드는 kind 별
    /// [`fallback_fields`] 로 채워 registry 미주입 상황을 결정적으로 재현한다.
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

    /// kind → 표시명. catalog 에 있으면 registry 기준 표시명을, 없으면(미등록/미주입)
    /// [`fallback_kind_label`] 로 떨어진다.
    pub(super) fn label(&self, kind: &str) -> String {
        self.specs
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.label.clone())
            .unwrap_or_else(|| fallback_kind_label(kind))
    }

    /// kind → 편집 필드 스키마. catalog 에 등록돼 있으면 registry 스냅샷을, 미등록/
    /// 미주입(빈 catalog)이면 [`fallback_fields`] 로 떨어진다 — 갤러리·테스트·registry
    /// 미주입 컨텍스트에서도 kind 별 폼이 결정적으로 그려진다.
    pub(super) fn fields(&self, kind: &str) -> Vec<PresetFieldSpec> {
        self.specs
            .iter()
            .find(|s| s.kind == kind)
            .map(|s| s.fields.clone())
            .unwrap_or_else(|| fallback_fields(kind))
    }

    /// kind → leading 아이콘. registry `SurfaceKindDef.icon` 이름을 host 아이콘 세트로
    /// 해석한다(하드코딩 없음). 미등록/미선언(빈 catalog 포함)이면 중립 `FILE`.
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

// ── kind accent 시각 매핑 ───────────────────────────────────────────────
//
// 아이콘은 registry `SurfaceKindDef.icon`([`KindCatalog::kind_icon`])으로 해석한다.
// accent 색은 아직 registry/theme 에 대응 토큰이 없다(SurfaceTheme 은 bg/fg 만) — 프리셋
// 편집기 leaf 를 장식하는 host 시각 선택이라 kind 별로 직접 결정한다. 미지정 kind 는
// 중립(text-secondary)으로 떨어진다. per-surface accent 토큰이 디자인에 추가되면
// registry 조회로 이관 예정.

pub(super) fn kind_accent(theme: &Theme, kind: &str) -> egui::Color32 {
    match kind {
        "terminal" => theme.accent_success().to_egui(),
        "markdown" => theme.accent_primary().to_egui(),
        "image" => theme.accent_info().to_egui(),
        "explorer" => theme.accent_agent().to_egui(),
        // 미지정 kind: accent 없이 중립(라벨과 같은 secondary).
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

/// registry 미주입/미등록 kind 의 표시명 fallback.
///
/// `surface.kind.<kind>` i18n 키를 시도하고(= registry `display_name_i18n_key`
/// 규약과 동일 키. builtin/plugin 모두 이 네임스페이스를 쓴다), 미번역이면 kind
/// 첫 글자를 대문자로. registry 가 주입되면 [`KindCatalog`] 가 우선하고, 이 함수는
/// catalog 에 없는 kind(비활성 plugin·`empty` 등)의 안전망으로만 쓰인다.
fn fallback_kind_label(kind: &str) -> String {
    label_from_i18n_key(&format!("surface.kind.{kind}"), kind)
}

// ── 정규화된 preview 모델 ───────────────────────────────────────────────
//
// 3종 preset(`Workspace`/`Tab`/`Pane`)을 공통 preview 모델로 정규화한다 — 위젯은
// 단일 타입만 받아 분기를 최소화한다(Codex 크로스체크 제안). 라벨은 build 시
// resolver 로 미리 해석해 둔다(렌더러는 registry/i18n 비의존).

/// surface leaf — kind 식별자 + 미리 해석된 표시명 + 편집용 round-trip 필드.
///
/// read-only 미리보기는 `kind`/`label` 만 쓰지만, 편집 모드가 트리를 모델로
/// 되돌려(`rebuild_*`) 디스크에 쓰려면 `cwd`/`startup`/`params` 를 손실 없이 보관해야
/// 한다.
///
/// `id` 는 **영속 surface id**(`PresetSurface.id`)를 그대로 채택한 값이다 — build 시
/// 재부여하지 않는다. 그래서 load→편집→save→재load 를 관통해 같은 surface 가 같은 id
/// 를 유지한다(세션 한정이 아님). 편집 선택/핸들 대상 지정 + 모델 round-trip(`surf_to_model`)
/// 에 쓰인다. 신규 leaf(split/add-tab)만 [`DemoLayout::alloc_id`] 로 새 id 를 받는다.
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

/// 편집 대상 preset scope. `split_pane` 유효성 판별 전용 태그다.
///
/// Workspace 만 상위 pane 트리를 분할할 수 있다. Pane scope 는 단일 pane 고정
/// (round-trip `rebuild_single_pane` 이 `Root::Panes(PaneNode::Leaf)` 단일만 인정)
/// 이라 pane split 이 무효고, Tab scope 는 애초에 pane 이 없다. 그런데 `from_workspace`
/// 와 `from_pane` 은 **둘 다 `Root::Panes`** 로 정규화해 Root 모양만으로는 Workspace/Pane
/// 을 구분할 수 없다 — 그래서 별도 scope 태그로 구분한다. (`Root::TabFrame` 은 Tab 을
/// 구분하지만 태그를 셋 다 명시해 의미를 일관되게 둔다.)
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
    /// 편집 중 새 노드(신규 leaf·신규 pane)에 부여할 다음 id. build 시 pane 세션 id 와
    /// leaf 영속 id **양쪽 domain 의 상한 + 1** 로 초기화한다([`max_node_id`]) — 새 노드가
    /// 어느 domain 과도 겹치지 않게.
    next_id: usize,
}

/// build 동안 **pane 에만** 세션 id 를 부여하는 카운터. leaf 는 대신 영속 surface id
/// (`PresetSurface.id`)를 채택하므로 이 카운터는 pane 전용이다.
///
/// **id-space 결정(계획 미명시 함정):** pane 세션 id 와 leaf 영속 id 는 **별개 domain**
/// 이다 — 둘은 상호 겹칠 수 있으나(예: pane 0 과 leaf 0 공존) 무해하다. 이유: (1) 선택
/// (`selected: Option<usize>`)·조회는 leaf 트리만, pane 조작은 pane id 만 다뤄 domain 을
/// 섞지 않고, (2) egui interaction salt 가 접두사(`"preset_leaf"` vs `"preset_demo_tab"`
/// 등)로 네임스페이스를 분리한다. 유일하게 지켜야 할 영속 불변식은 "**leaf id 만** load/
/// save/편집을 관통해 보존된다"이며, pane id 는 세션마다 재부여돼도 무방하다.
struct IdGen(usize);
impl IdGen {
    fn next(&mut self) -> usize {
        let id = self.0;
        self.0 += 1;
        id
    }
}

/// leaf 는 **영속 surface id 를 채택**한다(build 카운터 미사용). from_* 이 build 전에
/// `normalize_surface_ids` 로 모든 surface 에 id 를 채우므로 여기서는 항상 `Some` 이다.
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
    // explicit_name 우선, 없으면 대표 surface 의 표시명(디자인의 자동 탭 이름 규칙).
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
    // pane 은 영속 id 가 없으므로 세션 카운터로 부여(leaf 와 별개 domain — IdGen 주석).
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

/// 빌드된 preview 트리에서 pane 세션 id·leaf 영속 id 를 통틀어 최대값. 신규 노드
/// id 초기화(`next_id = max + 1`)용 — 두 domain 어느 것과도 안 겹치게 한다.
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

/// 라이브 모델 의미(`tasty-type-geometry::SplitDirection`)와 동일하게:
/// `Vertical` = 폭 분할(좌우, row), `Horizontal` = 높이 분할(상하, column).
/// capture/apply 와 일치시켜 미리보기가 실제 적용 결과와 같은 방향으로 읽히게 한다.
fn is_row(d: PresetSplitDirection) -> bool {
    matches!(d, PresetSplitDirection::Vertical)
}

impl DemoLayout {
    pub fn from_workspace(p: &WorkspacePreset, catalog: &KindCatalog) -> Self {
        // 영속 surface id 를 보장하기 위해 build 전 정규화(멱등 — 이미 정규화된 로드
        // 경로에선 no-op). 편집기는 preset 을 소유하지 않으므로 clone 후 정규화한다.
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
        // cx 가 catalog 참조를 잡으므로 블록으로 borrow 를 닫고 act 만 꺼낸다
        // (아래 self mutation 전에 immutable borrow 종료).
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
        // 배경 클릭 = 선택 해제(specimen 의 onClick→setSel(null)). leaf/위젯 interact
        // 가 뒤에 추가되어 위에 얹히므로, 그것들을 누르면 bg.clicked() 는 false 가 된다.
        let bg = ui.interact(rect, ui.id().with("preset_demo_bg"), egui::Sense::click());

        // cx 가 catalog 참조를 잡으므로 블록으로 borrow 를 닫고 act 만 꺼낸다
        // (아래 self mutation 전에 immutable borrow 종료).
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

    /// 한 프레임에 수집된 편집 의도([`Act`])를 트리 mutation 으로 실행한다.
    /// 마우스 UI([`show_edit`])와 키보드 단축키([`apply_shortcut`])가 공유하는
    /// 단일 배선 지점 — 변형·선택 정리·저장 필요성 판정을 여기서 일원화한다.
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
                    // 제거된 pane 안의 leaf 가 선택돼 있었으면 해제(사라진 leaf 는
                    // contains_leaf 로 판별 — 다른 pane 의 선택은 보존).
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

    /// 편집기 표준 단축키([`ShortcutAction`])를 focus(선택 leaf) 기준으로 실행한다.
    /// 선택이 없으면 no-op([`ShowOutcome::None`]) — WYSIWYG 에서 임의 대상 조작은
    /// 위험하므로 명시 선택된 leaf 에만 작용한다(원칙3 무관: 전역 포커스가 아니라
    /// 편집기 내부 선택).
    ///
    /// 대상 해석: surface 액션은 선택 leaf 자체, pane/tab 액션은 그 leaf 가 속한
    /// pane. scope 유효성(Pane/Tab scope 의 pane split 무효 등)은 하위 mutation
    /// (`split_pane`/`remove_pane`)이 자체 판별하므로 여기선 그대로 위임한다.
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

    // ── 편집 mutation ────────────────────────────────────────────────────

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
                // 선언되지 않은 params 키 정리(kind 변경 시에만).
                if let Some(obj) = l.params.as_object_mut() {
                    obj.retain(|k, _| param_keys.contains(k.as_str()));
                }
                // 새 kind 필드의 default 로 빈 값 초기화.
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

    /// leaf 를 split 한다. `before == true` 면 새 leaf 가 first(좌/상), false 면
    /// second(우/하). 키보드 경로는 after(기본 false), 디자인의 경계 존(좌/상 클릭)은
    /// before(true) — `preset-edit-03`(마우스) 에서 사용.
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

    /// 대상 pane leaf 를 `PaneNode::Split{ratio 0.5}` 로 교체하고 형제로 terminal
    /// 탭 1개짜리 새 pane 을 붙인다(새 pane 이 second). **Workspace scope 에서만
    /// 유효** — Pane/Tab scope 는 no-op(false).
    ///
    /// Pane scope 를 여기서 차단하는 이유: Pane preset 은 저장 시 단일 pane 만
    /// 인정(`rebuild_single_pane`)해 split 해도 조용히 저장에서 누락된다. 그 "조용한
    /// 누락" 대신 호출부(mutation)에서 명시적으로 막아, 변형이 저장되지 않을 조작을
    /// 애초에 수행하지 않는다(계획 §scope별 유효성). Workspace/Pane 은 둘 다
    /// `Root::Panes` 라 Root 모양으로는 구분 불가 → [`Scope`] 태그로 판별.
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

    /// pane_id 의 idx 탭을 제거하고 `active` 를 유효 범위로 클램프. **마지막 탭
    /// (len==1)이면 no-op(false)** — "pane 은 항상 탭 ≥1 유지"가 디자인 확정.
    /// 탭→pane 폴백은 여기 없다(키보드 `close_active` 디스패치 계층 `preset-edit-02`
    /// 의 몫).
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

    // ── 모델 reconstruction (편집 결과 → 디스크 저장용) ──────────────────

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

/// 편집기 표준 단축키가 표현하는 focus 기반 편집 액션. adapter 층(preset.rs)이
/// `KeybindingSettings` 매칭 결과로 이 값을 만들어 [`DemoLayout::apply_shortcut`] 에
/// 넘긴다. 대상 해석(leaf→pane)·scope 유효성은 apply_shortcut/하위 mutation 이
/// 담당하므로 이 enum 은 "무엇을" 만 지시하고 "어디에" 는 담지 않는다.
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
    // ── pane 계층 변형 · 탭 삭제. 키보드 단축키([`apply_shortcut`], `preset-edit-02`)가
    // 이 variant 를 생성한다. 마우스 UI(pane 핸들 · 탭 close ×)는 `preset-edit-03`.
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

// ── 트리 워크 헬퍼 (편집) ───────────────────────────────────────────────

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

// ── 모델 reconstruction ─────────────────────────────────────────────────

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
                // leaf.id 는 채택한 영속 surface id — 되써서 round-trip 보존(재load 시
                // 같은 surface = 같은 id). 신규 leaf 도 alloc_id 가 유일값을 줬으므로 안전.
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

// ── rect 분할 헬퍼 (specimen 과 동일) ───────────────────────────────────

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

// ── 재귀 렌더 ───────────────────────────────────────────────────────────

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

/// surface leaf — bg-app fill, 가운데 kind 아이콘(accent) + 표시명(mono, secondary) +
/// 값 요약. 선택 여부와 무관하게 같은 요약을 그린다 — 칸 안에는 잘릴 수 있는 폼을
/// 두지 않는다. 편집 모드: 1px separator inset outline(편집 가능 영역 표시), 선택 시
/// 2px accent inset outline + 핸들 클러스터(설정 · remove). 더블클릭은 설정 화면을 연다.
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

    // 편집 모드: 경계 hover-split 존 판정 + 클릭 라우팅. 존 활성이면 split, 아니면
    // 선택(배경 deselect 보다 위에 얹혀 우선). 선택된 leaf 에선 존 판정 안 함 —
    // split 불가(배경 클릭 deselect 후 가능).
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
            // 가운데 더블클릭 = 설정 열기. 첫 클릭이 이미 선택했다. 경계 존 위의
            // 더블클릭은 아래 clicked 갈래로 가서 split 을 한 번 더 한다(빠른 연속 split).
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

    // outline: 선택=2px accent, 편집 일반=1px separator.
    if selected {
        let w = theme.tab_indicator_width.value();
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
            egui::Stroke::new(w, theme.separator.to_egui()),
            egui::StrokeKind::Inside,
        );
    }

    // 활성 split 존 overlay — 콘텐츠·outline 위에 얹는다(존은 !selected 에서만 활성).
    if let Some(z) = zone {
        draw_split_zone_overlay(ui, theme, rect, z);
    }

    // 선택 leaf: 우상단 설정 · remove 핸들.
    if selected {
        draw_handle_cluster(ui, theme, rect, leaf.id, cx);
    }
}

/// leaf 미리보기(선택 여부 무관) — 가운데 kind 아이콘(accent) + kind명(mono, secondary) +
/// 그 아래 값 요약 블록(중앙 정렬). 값 요약은 kind 필드 중 값이 비지 않은 것을
/// `키 값` 한 줄로 그린다(라벨=`field.id` mono muted, 값 mono secondary).
///
/// degrade(빈 leaf 박스 크기 기준): 박스 <96×72 → 요약 숨김(아이콘 + kind명);
/// 짧은 축 <46 → kind명도 숨김(아이콘만). 임계는 [`LEAF_SUMMARY_MIN_W`]/
/// [`LEAF_SUMMARY_MIN_H`]/[`LEAF_ICON_ONLY_MIN`].
fn draw_leaf_preview(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    leaf: &Leaf,
    catalog: &KindCatalog,
) {
    let icon = theme.icon_glyph_size_md;
    let label_h = theme.font_size_caption;
    // summary-gap = 행↔행 gap, kind명↔요약 gap, 라벨↔값 gap 모두 space-xs.
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

    // 아이콘 + kind명 + 요약 전체 높이를 계산해 세로 중앙 정렬.
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
        // painter_at 가 rect 로 clip 하므로 좁은 leaf 에서도 라벨이 넘치지 않는다.
        ui.painter_at(rect).text(
            egui::pos2(cx_x.value(), (y + label_h.scaled(0.5)).value()),
            egui::Align2::CENTER_CENTER,
            &leaf.label,
            egui::FontId::monospace(label_h.value()),
            theme.text_secondary().to_egui(),
        );
        y += label_h;
    }

    if !rows.is_empty() {
        y += gap;
        let label_font = egui::FontId::monospace(theme.font_size_micro.value());
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

/// leaf 미리보기(선택 여부 무관)에 표시할 값 요약 행 목록(순수 함수 — 렌더 무관, 테스트 대상).
/// catalog 의 kind 필드를 등록 순서대로 순회하며 값이 비지 않은(공백 제외) 필드만 남긴다.
/// 라벨은 설정 화면의 번역 필드 라벨이 아니라 `field.id`(cwd/startup/
/// file/url) 소문자 키다. kind 하드코딩 없이 [`KindCatalog::fields`] 에 의존하므로
/// plugin kind 도 자체 필드 선언대로 요약된다.
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

/// 한 줄 ellipsis. `front=true` 면 선두를 잘라 앞에 `…`(경로 꼬리 유지), false 면
/// 말미를 잘라 뒤에 `…`. egui painter clip 은 뒤자름만 되고 앞자름은 불가하므로
/// (CSS `direction: rtl` 트릭 없음) FontId 로 폭을 측정해 직접 글자를 잘라낸다.
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

/// 정규화 커서 좌표(nx,ny ∈ 0..1)와 leaf 픽셀 크기(w,h)로 활성 split 존을 고른다.
/// 4변까지 거리 후보(left=nx, right=1-nx, top=ny, bottom=1-ny) 중 최솟값이
/// [`SPLIT_ZONE_EDGE`] 미만인 변이 활성. 축 길이가 `min_axis` 미만이면 그 축
/// 후보(좌우는 w, 상하는 h)를 제외한다(degrade — 좁은 leaf 의 중앙 선택 보장).
///
/// 임계를 상수로 읽지 않고 **인자로 받는다** — [`SPLIT_ZONE_MIN`] 은 배율을 타야 하는
/// 값이고([`z`]), 이 함수는 순수하게 남겨 테스트가 임계를 직접 지정할 수 있게 한다.
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
    let divider = theme.tab_indicator_width.value(); // 2px 분할선(accent bar 와 동일 굵기).
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

/// 우상단 핸들 클러스터 — `[설정(톱니)] [remove(danger)]`, remove 가 오른쪽 끝.
/// split-right/down 핸들은 경계 hover-split 존이 대체해 제거됐다.
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
        theme.accent_danger().to_egui()
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

/// 상위 레이아웃(pane split). Leaf = pane 카드, Split = 5px bg-app gap.
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
            // divider(PANE_GAP)는 칠하지 않는다 — bg-app 공백이 무거운 상위 divider.
            let (r1, _gap, r2) = split_rects(rect, *row, *ratio, z(theme, PANE_GAP).value());
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
    let sep = theme.separator.to_egui();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, radius, theme.bg_app().to_egui());

    // mini tab strip 배경.
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
        // 편집 && 탭>1 → close × 영역 예약(우측 패딩 9→3 + marginLeft 1 + 14 히트).
        // 탭 1개면 × 숨김(pane 은 항상 탭 ≥1) → 폭도 rest 그대로.
        let show_close = cx.edit && pane.tabs.len() > 1;
        let tw = if show_close {
            z(theme, TAB_PAD_X)
                + icon_sz
                + z(theme, TAB_GAP)
                + lw
                + z(theme, CLOSE_MARGIN)
                + z(theme, CLOSE_HIT)
                + z(theme, CLOSE_TAB_PAD)
        } else {
            z(theme, TAB_PAD_X) + icon_sz + z(theme, TAB_GAP) + lw + z(theme, TAB_PAD_X)
        };
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(x.value(), strip.min.y),
            egui::vec2(tw.value(), z(theme, STRIP_H).value()),
        );

        // 클릭 상호작용 — active 가 아닌 탭만 pointer + 클릭(SetActive, 아래에서 판정).
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
            // 2px accent 하단 bar.
            let bar = egui::Rect::from_min_size(
                egui::pos2(
                    tab_rect.min.x,
                    tab_rect.max.y - theme.tab_indicator_width.value(),
                ),
                egui::vec2(tw.value(), theme.tab_indicator_width.value()),
            );
            p.rect_filled(bar, 0.0, theme.accent_primary().to_egui());
        }
        if i > 0 {
            // 탭 사이 separator(borderRight).
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
                tab_rect.min.x + (z(theme, TAB_PAD_X) + icon_sz + z(theme, TAB_GAP)).value(),
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
                        theme.overlay_active().to_egui(),
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

    // 편집 모드: strip 끝에 add-tab "+" 버튼(디자인 22×20 — strip 높이보다 2px 넓다).
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
            // hover = overlay_hover fill + text_secondary 글리프.
            ui.painter_at(strip)
                .rect_filled(add, 0.0, theme.overlay_hover().to_egui());
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

    // strip border-bottom.
    ui.painter_at(rect)
        .hline(strip.x_range(), strip.max.y, egui::Stroke::new(bw, sep));

    // 활성 탭 본문 — padding 3, bg-app.
    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, strip.max.y), rect.max);
    let inner = body.shrink(z(theme, BODY_PAD).value());
    if let Some(t) = pane.tabs.get(pane.active).or_else(|| pane.tabs.first()) {
        draw_surf(ui, theme, inner, &t.layout, cx);
    }

    // 카드 외곽 border.
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

/// 아이콘 한 변을 `LogicalPx` 로 받는다. 호출 다섯 자리가 전부 이 파일 안이고,
/// f32 로 받으면 그 다섯이 각자 벗겨서 넘겨야 한다 — 벗기는 자리를 egui 로 나가는
/// 이 본문 두 줄로 모은다.
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
                // id:None — from_* 이 build 전 normalize 로 채운다(로드 경로와 동형).
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
        // registry 파생 catalog 는 등록 kind 를 후보로 노출하고, 현재 kind 가 목록에
        // 없으면 덧붙인다. HIDDEN(empty)은 from_registry 에서 이미 걸러진다.
        let cat = KindCatalog::from_pairs(vec![
            ("terminal".to_string(), "Terminal".to_string()),
            ("foo".to_string(), "Foo Surface".to_string()),
        ]);
        let cands = cat.candidates("terminal");
        assert!(cands.contains(&"foo".to_string()));
        assert!(cands.contains(&"terminal".to_string()));
        // 목록에 없는 현재 kind 는 덧붙는다(편집 중 유실 방지).
        let cands = cat.candidates("bar");
        assert!(cands.contains(&"bar".to_string()));
        // 등록 kind 의 라벨은 registry 표시명, 미등록은 fallback.
        assert_eq!(cat.label("foo"), "Foo Surface");
        assert_eq!(cat.label("terminal"), "Terminal");
    }

    #[test]
    fn empty_catalog_falls_back_to_static_kinds() {
        // registry 미주입(빈 catalog)이면 정적 EDIT_KINDS 로 fallback.
        let cat = KindCatalog::default();
        let cands = cat.candidates("terminal");
        for k in EDIT_KINDS {
            assert!(cands.contains(&k.to_string()), "missing static kind {k}");
        }
        // 정적 목록에 없는 현재 kind 도 덧붙는다.
        let cands = cat.candidates("plugin-kind");
        assert!(cands.contains(&"plugin-kind".to_string()));
    }

    #[test]
    fn vertical_split_is_row_horizontal_is_column() {
        // 라이브 모델 의미와 일치: Vertical=좌우(row), Horizontal=상하(column).
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
        // explicit_name 없으면 대표(첫) leaf 의 표시명을 탭 이름으로.
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
        // pane id 0 (첫 pane). 0→1 변경 = true, 다시 1→1 = false.
        assert!(dl.set_active(0, 1));
        assert!(!dl.set_active(0, 1));
        // 존재하지 않는 pane id → false.
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
        // 모델 round-trip 에도 반영.
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
        // unknown param 은 kind 미변경 편집에서 보존.
        assert_eq!(params.get("legacy_x").and_then(|v| v.as_i64()), Some(7));

        // 빈 값 write 는 param 키를 제거(값 부재 round-trip).
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

    /// kind 변경 시 이전 kind 의 stale params/전용 컬럼을 정리한다: markdown(params.file
    /// + legacy) → terminal 이면 params 는 비워지고(terminal 은 params 필드 없음),
    /// cwd 컬럼은 terminal 이 쓰므로 보존된다.
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
                // markdown 의 params(file/legacy)는 새 kind 가 선언하지 않아 정리됨.
                assert!(
                    surface
                        .params
                        .as_object()
                        .map(|o| o.is_empty())
                        .unwrap_or(true),
                    "stale params must be cleared on kind change: {:?}",
                    surface.params
                );
                // cwd/startup 은 terminal 이 쓰는 컬럼이라 보존.
                assert_eq!(surface.cwd.as_deref(), Some("/keep"));
                assert_eq!(surface.startup_command.as_deref(), Some("stale"));
            }
            _ => panic!(),
        }

        // 반대로 terminal → markdown: cwd/startup 컬럼은 markdown 이 안 써서 정리된다.
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
        // 새 leaf 는 고유 id.
        let mut after = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert_ne!(after[0], after[1]);
    }

    /// 편집 세션을 관통하는 영속 id 안정성: load(build) → split → rebuild(model) →
    /// 재load(재build, 정규화 포함) 후에도 기존 leaf 는 같은 id, 신규 leaf 는 겹치지
    /// 않는 id 를 유지한다(계획 `ids_survive_edit_round_trip`).
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

        // rebuild → 모델에 id 기록 → 새 DemoLayout build(디스크 재load + 정규화 모사).
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
        // 단일 surface 는 제거 불가.
        assert!(!dl.remove_leaf(sole, &tc()));

        dl.split_leaf(sole, false, false, &tc());
        let mut after = Vec::new();
        surf_leaf_ids(first_surf(&dl), &mut after);
        assert_eq!(after.len(), 2);
        // 하나 제거 → 형제로 collapse.
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
        // pane 은 leaf 와 별개의 세션 카운터(IdGen)로 0,1... 부여받는다(leaf 는 영속 id
        // 채택). 방문 순서로 첫 pane=0, 둘째 pane=1.
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
        // Root 는 이제 pane Split, 형제 2개.
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
        // 새 pane 은 고유 id.
        let mut after = Vec::new();
        pane_ids(root_pane(&dl), &mut after);
        assert_eq!(after.len(), 2);
        assert_ne!(after[0], after[1]);
        // 모델 round-trip 에 Split 반영.
        assert!(matches!(
            dl.rebuild_pane_node(),
            Some(PresetPaneNode::Split { .. })
        ));

        // Pane scope 에선 split_pane 무효(no-op).
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
        // 루트 단일 pane 은 제거 불가.
        assert!(!dl.remove_pane(sole, &tc()));

        // split 후 하나 제거 → 형제로 collapse.
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

        // active=1 탭 삭제 → 남은 탭 1개, active 는 0 으로 클램프.
        assert!(dl.remove_tab(pane_id, 1));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => {
                assert_eq!(pp.tabs.len(), 1);
                assert_eq!(pp.active, 0);
            }
            _ => panic!(),
        }

        // 마지막 탭은 삭제 불가(no-op) — pane 은 항상 탭 ≥1.
        assert!(!dl.remove_tab(pane_id, 0));
        match &dl.root {
            Root::Panes(PaneNode::Leaf(pp)) => assert_eq!(pp.tabs.len(), 1),
            _ => panic!(),
        }
    }

    // ── apply_shortcut (키보드 focus 디스패치, preset-edit-02) ──────────────

    #[test]
    fn pane_id_of_leaf_resolves_owning_pane() {
        // Workspace 2-pane split: 각 pane 안의 leaf id 로 소속 pane id 를 역추적.
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
        // leaf 는 영속 surface id 를 채택(방문 순서 0,1), pane 은 별개 세션 카운터(0,1).
        // pane0(id0) 안의 leaf=surface0, pane1(id1) 안의 leaf=surface1.
        assert_eq!(dl.pane_id_of_leaf(0), Some(0));
        assert_eq!(dl.pane_id_of_leaf(1), Some(1));
        // 존재하지 않는 leaf → None.
        assert_eq!(dl.pane_id_of_leaf(99), None);
        // Tab scope 는 pane 없음 → None.
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
        // 트리 불변.
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
        // 단일 surface 선택 → close 는 no-op(마지막 1장 가드), 선택 유지.
        let mut sel = Some(sole);
        assert!(matches!(
            dl.apply_shortcut(ShortcutAction::CloseSurface, &mut sel, &tc()),
            ShowOutcome::None
        ));
        assert_eq!(sel, Some(sole));

        // split 후 새 leaf 선택 → close → 형제로 collapse + 선택 해제.
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
        // 첫 탭(active=0)의 leaf 선택.
        let leaf_id = match &dl.root {
            Root::Panes(PaneNode::Leaf(p)) => {
                let mut ids = Vec::new();
                surf_leaf_ids(&p.tabs[0].layout, &mut ids);
                ids[0]
            }
            _ => panic!(),
        };
        let mut sel = Some(leaf_id);
        // active 탭 제거 → 남은 탭 1개.
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
        // 두 번째 pane(단일 탭)의 leaf 선택.
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
        // pane 이 제거돼 루트가 단일 pane 으로 collapse, 선택도 해제.
        assert!(matches!(root_pane(&dl), PaneNode::Leaf(_)));
        assert_eq!(sel, None);
        assert!(!dl.contains_leaf(second_leaf));
        let _ = second_pane;
    }

    #[test]
    fn apply_shortcut_split_pane_only_in_workspace_scope() {
        // Workspace scope: split_pane 유효.
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

        // Pane scope: split_pane 무효(no-op).
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
        // 두 번째 pane 의 leaf 선택 후 close_pane.
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
        // surface split 은 Tab scope 에서도 유효.
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

        // before=true → 새 leaf 가 first, 기존 leaf 가 second.
        dl.split_leaf(leaf_id, true, true, &tc());
        match first_surf(&dl) {
            SurfNode::Split { first, second, .. } => {
                // 기존 leaf id 는 second 에 보존, first 는 새 leaf.
                assert!(matches!(second.as_ref(), SurfNode::Leaf(l) if l.id == leaf_id));
                assert!(matches!(first.as_ref(), SurfNode::Leaf(l) if l.id != leaf_id));
            }
            _ => panic!("expected split after split_leaf"),
        }
    }

    #[test]
    fn pick_zone_edges_center_and_degrade() {
        // 중앙 → 존 없음(중앙 클릭 = 선택).
        assert_eq!(pick_zone(0.5, 0.5, 200.0, 200.0, SPLIT_ZONE_MIN), None);
        // 좌측 경계 안쪽 → Left.
        assert_eq!(
            pick_zone(0.1, 0.5, 200.0, 200.0, SPLIT_ZONE_MIN),
            Some(SplitZone::Left)
        );
        // nx=0.3 정확히 경계(미만 아님) → 활성 아님.
        assert_eq!(pick_zone(0.3, 0.5, 200.0, 200.0, SPLIT_ZONE_MIN), None);
        // 하단 → Bottom.
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
        // 좌측 존 클릭과 동형(row=true, before=true): 기존 leaf id 는 보존되고 새
        // leaf 만 신규 id 를 받는다("leaf id 만 보존" 불변식 — 존 클릭 경로에서도 성립).
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
        // 기존 id 가 여전히 트리에 존재.
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
        // cwd 는 path-like(Dir) → 앞자름 대상.
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
