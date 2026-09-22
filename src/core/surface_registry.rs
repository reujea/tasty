//! Surface 종류별 메타·동작 정의 레지스트리.
//!
//! 본체 4종(Terminal/Markdown/Html/Empty)이 부팅 시 등록된다. Explorer/Image
//! 등은 plugin이 hello 시점에 같은 레지스트리에 자기 kind를 추가한다.
//!
//! # 등록하는 것과 아직 없는 것
//!
//! - kind 마다 `create` / `restore` / `snapshot` 함수 포인터를 등록한다.
//!   `SavedSurface::Generic`이 snapshot/restore를 호출하고, surface 생성 funnel
//!   (`CoreState::create_surface_via_registry`)이 create를 호출한다.
//! - **아직 없음**: render 함수 + RenderStores + SurfaceCtx + SurfaceAction. 이것이
//!   들어오면 `egui_panels::draw_egui_panels`의 다운캐스트 분기를 dispatch로 대체하고
//!   다운캐스트 메서드 6종을 제거한다.

pub mod builtins;
pub mod meta;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::model::{Surface, SurfaceId};

pub use builtins::register_builtin_kinds;

/// `ClosedPanel::from_*` 가족이 받는 snapshot 클로저를 registry 위에서 만든다.
/// 호출자는 결과를 `&mut`로 가지고 한 capture 트랜잭션 동안 reuse한다.
pub fn snapshot_fn_for(
    registry: &SurfaceKindRegistry,
) -> impl FnMut(&dyn Surface) -> Option<serde_json::Value> + '_ {
    move |s| registry.get(s.kind()).and_then(|def| (def.snapshot)(s))
}

/// surface 의 직렬화 가능한 영속 데이터를 추출하는 콜백 타입. `None` 은
/// 영속화 제외 (휘발성 surface). [`SurfaceKindDef::snapshot`] 이 사용한다.
pub type SurfaceSnapshotFn = Arc<dyn Fn(&dyn Surface) -> Option<serde_json::Value> + Send + Sync>;

/// 편집기가 쓰는 프리셋 필드 값의 저장 대상. plugin kind 는 항상 `Params(param_key)`
/// (= `PresetSurface.params.<key>`) 로 write 하지만, builtin terminal 의 cwd/startup 은
/// `params` 가 아니라 `PresetSurface` 의 전용 컬럼이라 별도 target 으로 라우팅한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetFieldTarget {
    /// `PresetSurface.params.<key>` 에 문자열로 write.
    Params(String),
    /// `PresetSurface.cwd` 전용 컬럼 (builtin terminal/explorer).
    Cwd,
    /// `PresetSurface.startup_command` 전용 컬럼 (builtin terminal).
    Startup,
}

/// [`PresetFieldSpec::input`] — 편집 위젯 결정. 매니페스트 `PresetFieldInputType`
/// 의 host 측 미러.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetFieldInput {
    Text,
    FilePath,
    Dir,
    Url,
}

/// [`SurfaceKindDef`] 에 실리는 프리셋 편집 필드의 host 측 런타임 표현. 매니페스트
/// `PresetFieldDecl` 을 [`PresetFieldSpec::from_decl`] 로 변환하거나, builtin kind 는
/// 코드에서 직접 구성한다.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetFieldSpec {
    /// kind 내 항목 식별자 (egui salt / 안정 참조).
    pub id: String,
    /// 항목 라벨 i18n 키.
    pub label_key: String,
    /// 값 저장 대상 (params 키 / cwd / startup).
    pub target: PresetFieldTarget,
    /// 편집 위젯 결정.
    pub input: PresetFieldInput,
    /// 프리셋 적용에 필수인지 (편집기 표시/검증 힌트).
    pub required: bool,
    /// 빈 입력 placeholder i18n 키.
    pub placeholder_key: Option<String>,
    /// kind 로 새로 전환 시 초기값.
    pub default: Option<String>,
    /// 적용 시 이 필드 경로의 부모 디렉토리를 cwd 로 파생 (file_path + params 전용).
    pub derive_cwd: bool,
}

impl PresetFieldSpec {
    /// 매니페스트 `PresetFieldDecl` → host spec. plugin 선언은 항상 `param_key` 로
    /// params 에 write 하므로 target 은 언제나 [`PresetFieldTarget::Params`].
    pub fn from_decl(decl: &tasty_plugin_manifest::PresetFieldDecl) -> Self {
        use tasty_plugin_manifest::PresetFieldInputType as It;
        let input = match decl.input_type {
            It::Text => PresetFieldInput::Text,
            It::FilePath => PresetFieldInput::FilePath,
            It::Dir => PresetFieldInput::Dir,
            It::Url => PresetFieldInput::Url,
        };
        Self {
            id: decl.id.clone(),
            label_key: decl.label_key.clone(),
            target: PresetFieldTarget::Params(decl.param_key.clone()),
            input,
            required: decl.required,
            placeholder_key: decl.placeholder_key.clone(),
            default: decl.default.clone(),
            derive_cwd: decl.derive_cwd,
        }
    }

    /// 매니페스트 decl 슬라이스 → host spec vec (등록 경로 3곳 공용 헬퍼).
    pub fn from_decls(decls: &[tasty_plugin_manifest::PresetFieldDecl]) -> Vec<Self> {
        decls.iter().map(Self::from_decl).collect()
    }

    /// `fields` 중 `derive_cwd = true` 인 `file_path` 필드가 있고 그 `param_key` 로
    /// `params` 에 경로가 들어 있으면, 경로의 부모 디렉토리를 cwd 로 유도한다.
    ///
    /// url/text/dir 은 제외(경로 파생 무의미). 부모가 없는 경로(파일명만·빈 부모)는
    /// 파생하지 않고 다음 필드로 넘어간다. 프리셋 적용(`state/preset_apply.rs`)과
    /// surface 생성 시 cwd 상속 fallback(`plugin_bridge/remote_kind.rs`) 양쪽이
    /// 공유하는 단일 소스 — markdown 처럼 `EguiMeshSurface` 시절엔 `Surface::source_cwd()`
    /// 가 자체 file 필드에서 직접 파생했지만, `RemoteSurface`(remote/webview kind)는
    /// 그 필드가 없어 창조 시점에 이 파생을 거쳐야 동일 cwd 상속 동작을 유지한다.
    pub fn derive_cwd(fields: &[Self], params: &serde_json::Value) -> Option<std::path::PathBuf> {
        for f in fields {
            if !f.derive_cwd || f.input != PresetFieldInput::FilePath {
                continue;
            }
            let PresetFieldTarget::Params(key) = &f.target else {
                continue;
            };
            let Some(s) = params.get(key).and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(parent) = Path::new(s).parent()
                && !parent.as_os_str().is_empty()
            {
                return Some(parent.to_path_buf());
            }
        }
        None
    }
}

/// 등록된 kind 를 host 가 **실제로** 어떻게 그리는가. 매니페스트의 선언
/// (`SurfaceKindRendering`) 과 이름이 겹치지만 같은 값이 아니다 — 선언은 plugin 이
/// 요청한 것이고, 이 값은 등록 경로가 그 요청을 받아들인 결과다. 둘이 갈리는 자리가
/// 있다: 헤드리스는 `webview`/`remote` 선언을 등록하지 않고
/// (`boot/headless_plugins.rs`), egui-mesh 는 화이트리스트·api_version 게이트를 통과한
/// 것만 등록한다(`surface_registry/egui_mesh.rs`). 그래서 이 값은 **등록된 kind 에만**
/// 존재한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisteredRendering {
    /// host 가 자기 egui 로 직접 그린다 — builtin 4 종.
    HostEgui,
    /// plugin 프로세스가 tessellate 한 mesh 를 host 가 합성한다.
    EguiMesh,
    /// host 가 OS native WebView overlay 를 붙인다.
    Webview,
    /// plugin 이 트리를 보내오는 일반 remote surface.
    Remote,
}

impl RegisteredRendering {
    /// 와이어 값. 매니페스트 쪽 `SurfaceKindRendering` 의 serde 키와 같은 표기를 쓰되
    /// (`egui-mesh`), 매니페스트에 없는 host 렌더는 `host-egui` 다.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HostEgui => "host-egui",
            Self::EguiMesh => "egui-mesh",
            Self::Webview => "webview",
            Self::Remote => "remote",
        }
    }
}

/// 등록된 kind 를 **누가** 등록했는가.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KindSource {
    /// 부팅 시 host 가 직접 등록 — 어느 `plugin.*` 조회에도 안 나온다.
    HostBuiltin,
    /// plugin 이 hello 에서 등록. 값은 그 plugin 의 id.
    Plugin(String),
}

impl KindSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HostBuiltin => "host",
            Self::Plugin(_) => "plugin",
        }
    }

    /// plugin 출처면 그 id, host builtin 이면 `None`.
    pub fn plugin_id(&self) -> Option<&str> {
        match self {
            Self::HostBuiltin => None,
            Self::Plugin(id) => Some(id.as_str()),
        }
    }
}

/// surface 종류별 메타 + 동작 함수 묶음.
///
/// 모든 함수는 `Send + Sync + 'static`이며, `Arc<SurfaceKindDef>` 단위로 보관되어
/// 매 프레임 lookup 비용을 Arc clone 한 번으로 제한한다.
pub struct SurfaceKindDef {
    /// 안정 식별자 (lowercase snake_case). 예: `"terminal"`, `"markdown"`.
    pub kind: &'static str,

    /// host 가 이 kind 를 실제로 어떻게 그리는가 — **사실**이다. 매니페스트 선언이
    /// 아니라 등록 경로가 정한다. [`RegisteredRendering`] 의 주석 참조.
    pub rendering: RegisteredRendering,

    /// 누가 이 kind 를 등록했는가 — host builtin 인가 어느 plugin 인가.
    pub source: KindSource,

    /// 사용자에게 표시되는 표시명 i18n 키. 예: `"surface.kind.markdown"`.
    /// 지금은 자리만 둔다 — 현재 표시명은 surface 자체의 `display_name()` 메서드를 사용.
    pub display_name_i18n_key: &'static str,

    /// 탭/프리셋 leading 아이콘의 아이콘 **이름**(매니페스트 `icon`). host 는 이 이름을
    /// `icons::from_name` 으로 glyph 에 매핑한다 — 본체의 `match kind { "markdown" => MD }`
    /// 하드코딩을 generic 화. builtin 은 등록 코드에서, plugin kind 는 decl 에서 채운다.
    /// `None` 이면 UI fallback(FILE).
    pub icon: Option<String>,

    /// 새 surface 인스턴스를 만든다. IPC handler / `add_kind_tab` /
    /// `split_pane_targeted` 가 이 함수를 호출하여 종류별 분기를 일원화한다.
    ///
    /// `cwd` 는 호출자가 결정한 *carry cwd* — 사용자가 보고 있던 surface 의 source_cwd
    /// 를 그대로 전달한다. surface 생성자가 사용 여부를 결정 (예: explorer 는 root,
    /// terminal 은 spawn 시 working_dir). Surface cwd invariant: 호출자는 *반드시*
    /// resolve 후 명시 전달. 자세한 규칙은 `docs/architecture/invariants/surface-cwd.md`.
    ///
    /// `params`는 IPC/CLI에서 받은 JSON. 종류별로 필요한 키가 다르다 (예: markdown은 `"file"`,
    /// html은 `"url"`).
    #[allow(clippy::type_complexity)]
    pub create: Arc<
        dyn Fn(SurfaceId, Option<&Path>, &serde_json::Value) -> anyhow::Result<Box<dyn Surface>>
            + Send
            + Sync,
    >,

    /// 영속화된 데이터(`SavedSurface::Generic.data`)에서 surface를 복원한다.
    /// layout 복원(`layout_persistence::restore`)이 `SavedSurface::Generic` 을 만나면 호출한다.
    ///
    /// Terminal은 PTY spawn이 호스트 책임이라 별도 경로(`SavedSurface::Terminal`)를 거치며,
    /// terminal builtin의 `restore`는 호출되지 않는다 (안전한 sentinel을 반환).
    #[allow(clippy::type_complexity)]
    pub restore: Arc<
        dyn Fn(SurfaceId, &serde_json::Value) -> anyhow::Result<Box<dyn Surface>> + Send + Sync,
    >,

    /// surface의 직렬화 가능한 영속 데이터를 반환한다. `None`이면 영속화에서 제외.
    /// `SavedSurface::capture_surface`가 호출한다. 휘발성 surface는 `None`을
    /// 반환하여 layout 저장에서 빠진다.
    pub snapshot: SurfaceSnapshotFn,

    /// 프리셋 편집기가 이 kind 를 편집할 때 노출할 입력 필드 스키마. plugin kind 는
    /// 매니페스트 `preset_fields` 에서, builtin 은 등록 코드에서 채운다. 빈 vec 이면
    /// kind 전용 필드가 없다(편집기 fallback 이 kind 별 기본 필드로 떨어짐).
    pub preset_fields: Vec<PresetFieldSpec>,

    /// surface 열기 요청 params 의 key alias → canonical 키 매핑. caller 가 옛 키로
    /// 넘기면 host 가 canonical 키로 정규화한다(매니페스트 `param_aliases`). 본체의
    /// `kind == "markdown"` 일 때 `file_path`→`file` 정규화 같은 결합을 generic 화한다.
    /// builtin 은 등록 코드에서, plugin kind 는 decl 에서 채운다.
    pub param_aliases: HashMap<String, String>,

    /// surface 생성 시 params 에 없으면 host 가 주입하는 kind별 기본값(매니페스트
    /// `default_params`). 값은 리터럴이거나 정책 토큰(`@settings.explorer_view_mode`,
    /// `@home`)이다. 정책 토큰 해석은 host 정책이라 host 에 남고, "어느 kind 가 어떤
    /// 기본키를 요구하는가"만 decl 로 옮긴 것. 본체의 `kind == "explorer"` 기본값 주입
    /// 하드코딩을 generic 화한다. builtin 은 등록 코드에서, plugin kind 는 decl 에서 채운다.
    pub default_params: HashMap<String, String>,

    /// capability flags — host 의 `kind == "..."` 입력/줌/복사 게이트를 generic 화한다.
    /// 각 의미는 매니페스트 [`tasty_plugin_manifest::SurfaceKindDecl`] 의 동명 필드 참조.
    /// builtin 은 등록 코드에서, plugin kind 는 decl 에서 채운다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "plugin 매니페스트의 SurfaceKindDecl 에서 그대로 복사되는 값이다. \
                      복사는 headless 에서도 일어나고, 읽는 쪽만 GUI 의 입력·줌·복사 \
                      게이트다. 칸을 빼면 그 빌드가 매니페스트의 그 키를 잃는다"
        )
    )]
    pub consumes_egui_input: bool,
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "위 consumes_egui_input 과 같다 — 매니페스트에서 복사되고 읽는 쪽만 GUI 다"
        )
    )]
    pub zoomable: bool,
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "위 consumes_egui_input 과 같다 — 매니페스트에서 복사되고 읽는 쪽만 GUI 다"
        )
    )]
    pub egui_copy: bool,
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "위 consumes_egui_input 과 같다 — 매니페스트에서 복사되고 읽는 쪽만 GUI 다"
        )
    )]
    pub copy_path: bool,
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "위 consumes_egui_input 과 같다 — 매니페스트에서 복사되고 읽는 쪽만 GUI 다"
        )
    )]
    pub egui_paste: bool,

    /// 자동 탭 명명에 basename 을 파생할 params 키(매니페스트 `name_from_param`).
    /// `Some("file")` 이면 params 의 `file` 값 basename 을 표시명으로 쓴다. `None` 이면
    /// 파생 없이 kind 의 표시명 fallback. 본체의 `kind == "markdown"` basename 명명
    /// 하드코딩을 generic 화한다. builtin 은 등록 코드에서, plugin kind 는 decl 에서 채운다.
    pub name_from_param: Option<String>,

    /// 이 kind 의 surface 를 파일로 열 때 host 가 "최근 연 파일" 목록에 기록할지
    /// (매니페스트 `records_recent`). `true` 면 파일-open 진입점에서 kind 별 최근 목록에
    /// 기록한다. host 본체의 `kind == "markdown"` recent 기록 분기를 generic 화한다.
    /// builtin 은 등록 코드에서, plugin kind 는 decl 에서 채운다.
    pub records_recent: bool,

    /// 이 kind 로 convert 하려면 host 가 먼저 파일 입력 팝업을 띄워야 하는지
    /// (매니페스트 `convert_requires_input`). `true` 면 convert 팝업에서 이 kind 선택
    /// 시 즉시 빈 params 변환 대신 `convert_input_popup` 팝업을 연다. host 본체의
    /// `kind == "markdown"` convert 분기를 generic 화. builtin 은 false.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "위 consumes_egui_input 과 같다 — 매니페스트에서 복사되고 읽는 쪽만 GUI 다"
        )
    )]
    pub convert_requires_input: bool,

    /// `convert_requires_input == true` 일 때 host 가 열 plugin file-input 팝업의
    /// **qualified id** (`"<plugin_id>/<popup_id>"`). host 는 이를 split 해
    /// `open_popup_instance` 로 연다. 등록 시점(egui_mesh.rs/remote_kind.rs)에
    /// 소유 plugin_id 로 qualify 한다 — host 는 kind 이름을 몰라도 이 데이터만 따른다.
    /// builtin 은 `None`.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "위 consumes_egui_input 과 같다 — 매니페스트에서 복사되고 읽는 쪽만 GUI 다"
        )
    )]
    pub convert_input_popup: Option<String>,
}

impl SurfaceKindDef {
    /// 이 kind 의 surface 생성에 반드시 필요한 params 키 목록.
    ///
    /// `preset_fields[].required=true` 가 단일 진실원(매니페스트 `validate.rs` 가
    /// 강제)이며, 값이 params 로 흐르는 필드(target=Params)만 해당한다. terminal 의
    /// cwd/startup 처럼 전용 컬럼(Cwd/Startup)으로 라우팅되는 필드는 params 키가
    /// 아니므로 제외된다.
    pub fn required_params(&self) -> impl Iterator<Item = &str> {
        self.preset_fields.iter().filter_map(|f| match &f.target {
            PresetFieldTarget::Params(key) if f.required => Some(key.as_str()),
            _ => None,
        })
    }

    /// `params` 에서 값이 비었거나 없는 첫 required param 키를 반환. 없으면 `None`.
    /// surface 생성 IPC 핸들러가 명확한 에러 메시지를 위해 선검증할 때 사용한다.
    pub fn first_missing_required_param(&self, params: &serde_json::Value) -> Option<&str> {
        self.required_params().find(|key| {
            params
                .get(*key)
                .and_then(|v| v.as_str())
                .map(str::is_empty)
                .unwrap_or(true)
        })
    }

    /// `params` 의 옛 키를 canonical 키로 정규화한다(alias 매핑). canonical 키가 이미
    /// 있으면 alias 는 무시(명시 우선). object 가 아니거나 alias 가 없으면 no-op.
    pub fn normalize_param_aliases(&self, params: &mut serde_json::Value) {
        if self.param_aliases.is_empty() {
            return;
        }
        let Some(obj) = params.as_object_mut() else {
            return;
        };
        for (alias, canonical) in &self.param_aliases {
            if !obj.contains_key(canonical.as_str())
                && let Some(v) = obj.remove(alias)
            {
                obj.insert(canonical.clone(), v);
            }
        }
    }
}

/// surface 종류 lookup 테이블. `Arc<SurfaceKindRegistry>` 단위로 CoreState에 보관되어
/// 매 프레임 dispatch에 사용된다.
///
/// 내부적으로 `RwLock`을 사용하여 plugin이 부팅 후 동적으로 kind를 등록할 수 있게
/// 한다. Builtin은 부팅 시 한 번 register되고 read만 일어나는 hot path는 read-lock
/// 한 번이므로 사실상 lock-free에 가깝다.
///
/// # 철회(withdrawn)
///
/// plugin 을 disable · remove 하면 그 plugin 이 등록한 kind 는 **지우지 않고 철회로
/// 표시한다**([ADR-0534](../../docs/adr/0534-a-disabled-plugins-surface-kinds-are-withdrawn-not-erased.md)).
/// 이미 열린 surface 는 자기 kind 의 `snapshot` · 아이콘 · 입력 플래그를 계속 읽어야
/// 하므로(지우면 layout 저장이 그 surface 를 `empty` 로 떨어뜨린다) [`Self::get`] 은
/// 철회된 것도 돌려준다. 새로 만드는 쪽 — 생성 funnel · 복원 · 목록 — 은
/// [`Self::get_live`] · [`Self::contains`] · [`Self::kinds_snapshot`] 을 읽어 철회된
/// kind 를 "지금 없는 kind" 로 본다. 같은 kind 가 다시 [`Self::register`] 되면(plugin 을
/// 다시 켜 hello 가 오면) 철회가 풀린다.
#[derive(Default)]
pub struct SurfaceKindRegistry {
    kinds: RwLock<HashMap<&'static str, KindEntry>>,
    /// poison 을 보고했는가(첫 1 회만). poison 은 sticky 인데 조회가 매 프레임
    /// dispatch 에서 도는 hot path 라, 매번 남기면 그 로그가 자기 자신에 묻힌다.
    poison_reported: std::sync::atomic::AtomicBool,
}

/// registry 한 칸 — 정의와, 철회됐다면 그것을 등록했던 plugin id.
struct KindEntry {
    def: Arc<SurfaceKindDef>,
    withdrawn_by: Option<String>,
}

/// 철회된 kind 로 새 surface 를 만들려 한 요청의 거절 사유.
///
/// `unknown surface kind` 와 가르는 이유: 그 kind 는 **있었고**, 그것을 제공하던 plugin 이
/// 꺼졌다. 사용자가 할 일(그 plugin 을 다시 켠다)이 다르다. 철회는 다시 켠 뒤 hello 가 와야
/// 풀리므로, 켰지만 아직 연결되지 않은 동안도 이 사유다 — 문장이 "꺼졌다" 로 단정하지 않고
/// 그 갈래까지 적는 이유다. IPC 응답은 [`std::fmt::Display`]
/// 문장을, 사용자 발화 intent 는 i18n toast(`surface.kind_toast.withdrawn`)를 받는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceKindWithdrawn {
    pub kind: String,
    pub plugin_id: String,
}

impl std::fmt::Display for SurfaceKindWithdrawn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "surface kind '{}' is unavailable: the plugin '{}' that provides it is disabled or \
             removed, or has not reconnected since it was enabled again",
            self.kind, self.plugin_id
        )
    }
}

impl std::error::Error for SurfaceKindWithdrawn {}

impl SurfaceKindRegistry {
    pub fn new() -> Self {
        Self {
            kinds: RwLock::new(HashMap::new()),
            poison_reported: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Poison 을 복구해 read guard 를 잡는다.
    ///
    /// 이전에는 `read().ok()?` · `map(...).unwrap_or_default()` 로 **조용히** 빈 결과를
    /// 돌려줬다. 그러면 등록된 kind 가 통째로 사라진 것처럼 보이는데 — surface 생성이
    /// "알 수 없는 kind" 로 실패하고, `sync_webviews` 가 이 kind 에 native webview 를
    /// 안 붙인다 — 관측 지점이 0 이라 왜 그런지 알 방법이 없다.
    ///
    /// 다른 자리도 같은 이유로 복구로 바뀌었는데, 그 동일성을 지키는 것은 **여기 적힌
    /// 수가 아니라** [`crate::poison`] 이다: 이 저장소의 poison 복구는 전부 그 헬퍼
    /// 하나를 거치고, "복구는 첫 1 회라도 보고한다" 는 규칙은
    /// `crates/tasty-doc-guards` 의 `no_silent_poison_recovery` 가 저장소 전체에서
    /// 강제한다. 그래서 몇 군데인지는 세어 적지 않는다 — 그 수는 자리가 하나 늘 때마다
    /// 낡고, 낡았는지 확인하려면 저장소 전체를 훑어야 해서 아무도 확인하지 않는다.
    ///
    /// 임계구역은 `HashMap` 의 삽입·조회·순회뿐이라 패닉이 나도 불변식이 성립한다.
    /// 조회는 **메인 스레드의 매 프레임 dispatch** 가 부르므로 패닉은 프로세스 전체를
    /// 죽인다 — 두 질문 모두 복구를 가리킨다
    /// ([`error-handling.md`](../../docs/dev-guide/error-handling.md) "락 poison").
    fn lock_read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<&'static str, KindEntry>> {
        crate::poison::recover_read(
            self.kinds.read(),
            "surface kind registry",
            &self.poison_reported,
        )
    }

    /// Poison 을 복구해 write guard 를 잡는다. 근거는 [`Self::lock_read`] 와 같다.
    fn lock_write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<&'static str, KindEntry>> {
        crate::poison::recover_write(
            self.kinds.write(),
            "surface kind registry",
            &self.poison_reported,
        )
    }

    /// `&self`만 받으므로 `Arc<SurfaceKindRegistry>` 너머에서도 호출 가능.
    /// plugin 매니저가 hello 받은 후 호출. 철회돼 있던 kind 면 철회가 풀린다.
    pub fn register(&self, def: SurfaceKindDef) {
        let kind = def.kind;
        let mut map = self.lock_write();
        let entry = KindEntry {
            def: Arc::new(def),
            withdrawn_by: None,
        };
        match map.insert(kind, entry) {
            Some(old) if old.withdrawn_by.is_some() => {
                tracing::info!(
                    "SurfaceKindRegistry: withdrawn kind '{}' registered again",
                    kind
                );
            }
            Some(_) => tracing::warn!("SurfaceKindRegistry: kind '{}' overwritten", kind),
            None => {}
        }
    }

    /// `plugin_id` 가 등록한 kind 를 전부 철회로 표시하고, 이번에 새로 철회된 kind 를
    /// 이름순으로 돌려준다. 정의는 지우지 않는다 — 열린 surface 가 계속 읽는다(타입 문서
    /// "철회" 절). host builtin 과 다른 plugin 의 kind 는 건드리지 않는다.
    pub fn withdraw_plugin(&self, plugin_id: &str) -> Vec<&'static str> {
        let mut map = self.lock_write();
        let mut withdrawn: Vec<&'static str> = map
            .iter_mut()
            .filter(|(_, e)| {
                e.withdrawn_by.is_none() && e.def.source.plugin_id() == Some(plugin_id)
            })
            .map(|(k, e)| {
                e.withdrawn_by = Some(plugin_id.to_string());
                *k
            })
            .collect();
        drop(map);
        withdrawn.sort_unstable();
        if !withdrawn.is_empty() {
            tracing::info!(
                "SurfaceKindRegistry: withdrew kind(s) {:?} of plugin '{}'",
                withdrawn,
                plugin_id
            );
        }
        withdrawn
    }

    /// kind 정의 — **철회된 것도 돌려준다.** 이미 있는 surface 에 대한 물음(snapshot ·
    /// 아이콘 · 입력 플래그 · 표시명)이 쓴다. 새로 만들 수 있는가를 물으려면
    /// [`Self::get_live`].
    pub fn get(&self, kind: &str) -> Option<Arc<SurfaceKindDef>> {
        self.lock_read().get(kind).map(|e| e.def.clone())
    }

    /// 지금 새 surface 를 만들 수 있는 kind 의 정의. 철회된 kind 는 `None` — 생성 funnel ·
    /// 복원 · kind 등록 대기가 "아직(다시) 없는 kind" 로 다룬다.
    pub fn get_live(&self, kind: &str) -> Option<Arc<SurfaceKindDef>> {
        self.lock_read()
            .get(kind)
            .filter(|e| e.withdrawn_by.is_none())
            .map(|e| e.def.clone())
    }

    /// 철회된 kind 면 그것을 등록했던 plugin id.
    pub fn withdrawn_by(&self, kind: &str) -> Option<String> {
        self.lock_read()
            .get(kind)
            .and_then(|e| e.withdrawn_by.clone())
    }

    /// 지금 쓸 수 있는(철회되지 않은) kind 인가.
    pub fn contains(&self, kind: &str) -> bool {
        self.get_live(kind).is_some()
    }

    /// 지금 쓸 수 있는 kind 목록을 스냅샷으로 반환 (lock 해제 후 안전히 사용). 철회된
    /// kind 는 빠진다 — 이 목록의 소비자는 전부 "무엇을 만들 수 있는가" 를 묻는다.
    pub fn kinds_snapshot(&self) -> Vec<(&'static str, Arc<SurfaceKindDef>)> {
        self.lock_read()
            .iter()
            .filter(|(_, e)| e.withdrawn_by.is_none())
            .map(|(k, e)| (*k, e.def.clone()))
            .collect()
    }

    // 이유: 현재 실제 호출처 없음(clippy len_without_is_empty 대응용 pair) — 과거
    // engine.rs → core/ 재배치로 core 가 pub(crate) 로 캡슐화되며 드러남.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.lock_read().len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.lock_read().is_empty()
    }
}

impl tasty_plugin_protocol::host_port::SurfaceRegistry for SurfaceKindRegistry {
    fn contains(&self, kind: &str) -> bool {
        SurfaceKindRegistry::contains(self, kind)
    }
}

/// kind+params로부터 합리적인 탭 표시명을 도출한다.
///
/// kind 가 매니페스트 `name_from_param`(registry `SurfaceKindDef.name_from_param`)을
/// 선언하면 그 params 키의 값 basename 을 표시명으로 쓴다(예: markdown="file" →
/// `README.md`). 미선언이거나 그 키가 params 에 없으면 kind 의 표시명 fallback
/// (`display_name_i18n_key` 번역, 미등록이면 kind 문자열)으로 떨어진다. 본체의
/// `kind == "markdown"` basename 명명 하드코딩을 generic 화한다.
pub(crate) fn default_tab_name_for_kind(
    kind: &str,
    params: &serde_json::Value,
    def: Option<&SurfaceKindDef>,
) -> String {
    fn basename_or(path: &str, fallback: &str) -> String {
        path.split(['/', '\\'])
            .rfind(|s| !s.is_empty())
            .unwrap_or(fallback)
            .to_string()
    }
    // kind 표시명 fallback: registry display_name_i18n_key 번역(미등록이면 kind 그대로).
    let fallback = || {
        def.map(|d| crate::i18n::t(d.display_name_i18n_key).to_string())
            .unwrap_or_else(|| kind.to_string())
    };
    if let Some(key) = def.and_then(|d| d.name_from_param.as_deref())
        && let Some(p) = params.get(key).and_then(|v| v.as_str())
    {
        let fb = fallback();
        return basename_or(p, &fb);
    }
    fallback()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 등록·조회가 poison 후에도 **살아남는다**.
    ///
    /// 이전에는 write 가 no-op 으로 빠지고 read 가 빈 결과를 돌려줘서, 등록된 kind 가
    /// 통째로 사라진 것처럼 보였다 — 그리고 그게 왜인지 알 관측 지점이 없었다.
    #[test]
    fn registry_survives_a_poisoned_lock() {
        let reg = std::sync::Arc::new(SurfaceKindRegistry::new());

        // 다른 스레드가 write guard 를 든 채 패닉 → poison.
        let held = std::sync::Arc::clone(&reg);
        let joined = std::thread::spawn(move || {
            let _g = held.kinds.write().expect("fresh lock");
            panic!("poison the registry");
        })
        .join();
        assert!(joined.is_err());

        // 실제 등록 경로(builtin 등록)를 그대로 태운다.
        crate::core::surface_registry::builtins::register_builtin_kinds(&reg);
        assert!(reg.contains("terminal"), "poison 후에도 등록이 반영된다");
        assert!(reg.get("terminal").is_some());
        assert!(!reg.kinds_snapshot().is_empty());
        assert!(reg.len() > 0);
        assert!(!reg.is_empty());
    }

    fn dummy_def(kind: &'static str) -> SurfaceKindDef {
        SurfaceKindDef {
            kind,
            rendering: RegisteredRendering::HostEgui,
            source: KindSource::HostBuiltin,
            display_name_i18n_key: "test.dummy",
            icon: None,
            create: Arc::new(|_, _, _| Err(anyhow::anyhow!("dummy"))),
            restore: Arc::new(|_, _| Err(anyhow::anyhow!("dummy"))),
            snapshot: Arc::new(|_| None),
            preset_fields: Vec::new(),
            param_aliases: HashMap::new(),
            default_params: HashMap::new(),
            consumes_egui_input: false,
            zoomable: false,
            egui_copy: false,
            copy_path: false,
            egui_paste: false,
            name_from_param: None,
            records_recent: false,
            convert_requires_input: false,
            convert_input_popup: None,
        }
    }

    fn plugin_def(kind: &'static str, plugin_id: &str) -> SurfaceKindDef {
        SurfaceKindDef {
            source: KindSource::Plugin(plugin_id.to_string()),
            ..dummy_def(kind)
        }
    }

    /// 철회(ADR-0534)는 정의를 지우지 않는다 — 열린 surface 가 `get` 으로 snapshot 을 계속
    /// 읽는다. 새로 만들 수 있는가를 묻는 셋(`get_live` · `contains` · `kinds_snapshot`)과
    /// 생성 funnel 은 철회된 kind 를 없는 것으로 보고, funnel 은 그 사유를 따로 낸다.
    #[test]
    fn a_withdrawn_kind_keeps_its_definition_but_refuses_creation() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let engine = crate::core::CoreState::new(80, 24, waker).expect("engine");
        let reg = &engine.surface_registry;
        reg.register(plugin_def("probe_kind", "com.x.probe"));
        assert!(
            engine
                .create_surface_via_registry("probe_kind", 1, None, &serde_json::json!({}))
                .is_err_and(|e| e.downcast_ref::<SurfaceKindWithdrawn>().is_none())
        );

        assert_eq!(reg.withdraw_plugin("com.x.probe"), vec!["probe_kind"]);

        assert!(
            reg.get("probe_kind").is_some(),
            "열린 surface 용 정의는 남는다"
        );
        assert!(reg.get_live("probe_kind").is_none());
        assert!(!reg.contains("probe_kind"));
        assert!(reg.kinds_snapshot().iter().all(|(k, _)| *k != "probe_kind"));
        assert_eq!(
            reg.withdrawn_by("probe_kind").as_deref(),
            Some("com.x.probe")
        );
        let err = engine
            .create_surface_via_registry("probe_kind", 1, None, &serde_json::json!({}))
            .err()
            .expect("철회된 kind 는 만들지 않는다");
        assert_eq!(
            err.downcast_ref::<SurfaceKindWithdrawn>(),
            Some(&SurfaceKindWithdrawn {
                kind: "probe_kind".to_string(),
                plugin_id: "com.x.probe".to_string(),
            })
        );
        assert!(err.to_string().contains("com.x.probe"), "{err}");
        // 두 번째 철회는 새로 철회한 것이 없다.
        assert!(reg.withdraw_plugin("com.x.probe").is_empty());
    }

    #[test]
    fn registering_a_withdrawn_kind_again_lifts_the_withdrawal() {
        let reg = SurfaceKindRegistry::new();
        reg.register(plugin_def("probe_kind", "com.x.probe"));
        reg.withdraw_plugin("com.x.probe");
        reg.register(plugin_def("probe_kind", "com.x.probe"));
        assert!(reg.get_live("probe_kind").is_some());
        assert_eq!(reg.withdrawn_by("probe_kind"), None);
    }

    #[test]
    fn withdrawal_touches_only_the_plugins_own_kinds() {
        let reg = SurfaceKindRegistry::new();
        reg.register(dummy_def("host_kind"));
        reg.register(plugin_def("mine", "com.x.a"));
        reg.register(plugin_def("theirs", "com.x.b"));
        assert_eq!(reg.withdraw_plugin("com.x.a"), vec!["mine"]);
        assert!(reg.get_live("host_kind").is_some());
        assert!(reg.get_live("theirs").is_some());
        assert!(reg.get_live("mine").is_none());
    }

    #[test]
    fn register_and_lookup() {
        let reg = SurfaceKindRegistry::new();
        reg.register(dummy_def("alpha"));
        reg.register(dummy_def("beta"));
        assert!(reg.contains("alpha"));
        assert!(reg.contains("beta"));
        assert!(!reg.contains("gamma"));
        assert_eq!(reg.get("alpha").unwrap().kind, "alpha");
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn duplicate_register_overwrites() {
        let reg = SurfaceKindRegistry::new();
        reg.register(dummy_def("x"));
        reg.register(dummy_def("x"));
        assert!(reg.contains("x"));
        assert_eq!(reg.len(), 1);
    }

    /// required_params 는 preset_fields 의 required=true + target=Params 만 노출한다.
    fn def_with_file_required(kind: &'static str) -> SurfaceKindDef {
        let mut d = dummy_def(kind);
        d.preset_fields = vec![PresetFieldSpec {
            id: "file".to_string(),
            label_key: "preset.field.file".to_string(),
            target: PresetFieldTarget::Params("file".to_string()),
            input: PresetFieldInput::FilePath,
            required: true,
            placeholder_key: None,
            default: None,
            derive_cwd: true,
        }];
        d
    }

    #[test]
    fn required_params_derived_from_preset_fields() {
        let d = def_with_file_required("markdown");
        assert_eq!(d.required_params().collect::<Vec<_>>(), vec!["file"]);
        // cwd/startup (target=Cwd/Startup) 는 params 키가 아니라 required_params 제외.
        let reg = SurfaceKindRegistry::new();
        register_builtin_kinds(&reg);
        let term = reg.get("terminal").unwrap();
        assert_eq!(term.required_params().count(), 0);
    }

    #[test]
    fn first_missing_required_param_detects_absent_and_empty() {
        let d = def_with_file_required("markdown");
        assert_eq!(
            d.first_missing_required_param(&serde_json::json!({})),
            Some("file")
        );
        assert_eq!(
            d.first_missing_required_param(&serde_json::json!({"file": ""})),
            Some("file")
        );
        assert_eq!(
            d.first_missing_required_param(&serde_json::json!({"file": "/a/b.md"})),
            None
        );
    }

    #[test]
    fn normalize_param_aliases_moves_old_key() {
        let mut d = dummy_def("markdown");
        d.param_aliases = HashMap::from([("file_path".to_string(), "file".to_string())]);
        // 옛 키 → canonical 로 이동.
        let mut p = serde_json::json!({"file_path": "/a/b.md"});
        d.normalize_param_aliases(&mut p);
        assert_eq!(p, serde_json::json!({"file": "/a/b.md"}));
        // canonical 키가 이미 있으면 alias 무시(명시 우선). alias 키는 그대로 남되
        // 다운스트림 create 가 canonical 만 읽으므로 무해(옛 동작과 동일).
        let mut p2 = serde_json::json!({"file": "/keep.md", "file_path": "/drop.md"});
        d.normalize_param_aliases(&mut p2);
        assert_eq!(p2["file"], "/keep.md");
    }

    #[test]
    fn default_tab_name_uses_name_from_param() {
        let mut d = dummy_def("markdown");
        d.name_from_param = Some("file".to_string());
        // name_from_param 키가 params 에 있으면 basename.
        assert_eq!(
            super::default_tab_name_for_kind(
                "markdown",
                &serde_json::json!({"file": "/a/b/README.md"}),
                Some(&d),
            ),
            "README.md"
        );
        // 키가 없으면 display_name_i18n_key 번역(테스트: lang 미로드 → 키 그대로).
        assert_eq!(
            super::default_tab_name_for_kind("markdown", &serde_json::json!({}), Some(&d),),
            "test.dummy"
        );
        // name_from_param 미선언이면 파생 없이 fallback.
        let plain = dummy_def("empty");
        assert_eq!(
            super::default_tab_name_for_kind(
                "empty",
                &serde_json::json!({"file": "/x/y.md"}),
                Some(&plain),
            ),
            "test.dummy"
        );
        // def 미등록이면 kind 문자열 그대로(catch-all 보존).
        assert_eq!(
            super::default_tab_name_for_kind("plugin_x", &serde_json::json!({}), None),
            "plugin_x"
        );
    }

    #[test]
    fn builtin_registers_host_kinds() {
        let reg = SurfaceKindRegistry::new();
        register_builtin_kinds(&reg);
        // image는 com.tasty.image plugin이, markdown은 com.tasty.markdown plugin이
        // 각각 hello 시 egui-mesh whitelist 경유로 등록한다. explorer/dag_graph 는
        // host builtin surface 로 부팅 시 직접 등록된다.
        for kind in ["terminal", "empty", "explorer", "dag_graph"] {
            assert!(reg.contains(kind), "missing builtin kind: {kind}");
        }
        assert_eq!(reg.len(), 4);
        assert!(!reg.contains("image"));
        assert!(!reg.contains("markdown"));
        assert!(!reg.contains("clipboard_viewer"));
    }
}
pub mod egui_mesh;
pub mod webview_kind;
