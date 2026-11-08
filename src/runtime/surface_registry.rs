//! Surface 종류의 생성·복원·snapshot 함수와 메타데이터를 등록한다.
//! host 내장 종류는 부팅 때, plugin 종류는 hello를 처리할 때 등록한다.

pub mod builtins;
pub mod meta;
mod prepared;
pub use prepared::{PreparedKind, PublicationAction};

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::model::{Surface, SurfaceId};

pub use builtins::register_builtin_kinds;

/// 직렬화할 데이터. None은 해당 surface를 영속화에서 제외한다.
pub type SurfaceSnapshotFn = Arc<dyn Fn(&dyn Surface) -> Option<serde_json::Value> + Send + Sync>;

/// create 에 넘길 cwd 와 params 로 탭 이름을 고른다. None 이면 종류 표시명을 쓴다.
pub type SurfaceTabNameFn = fn(Option<&Path>, &serde_json::Value) -> Option<String>;

/// plugin 필드는 params에, 내장 terminal의 cwd·startup은 PresetSurface 전용 필드에 저장한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetFieldTarget {
    Params(String),
    Cwd,
    Startup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetFieldInput {
    Text,
    FilePath,
    Dir,
    Url,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresetFieldSpec {
    /// kind 내부의 항목 ID. 위젯 식별에도 사용한다.
    pub id: String,
    pub label_key: String,
    pub target: PresetFieldTarget,
    pub input: PresetFieldInput,
    pub required: bool,
    pub placeholder_key: Option<String>,
    /// 편집기에서 종류를 새로 선택할 때 넣을 초기값.
    pub default: Option<String>,
    /// Params 대상의 FilePath 필드에서 부모 디렉터리를 cwd로 파생할지 여부.
    pub derive_cwd: bool,
}

impl PresetFieldSpec {
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

    pub fn from_decls(decls: &[tasty_plugin_manifest::PresetFieldDecl]) -> Vec<Self> {
        decls.iter().map(Self::from_decl).collect()
    }

    /// derive_cwd가 켜진 FilePath 필드에서 절대 경로의 부모를 처음 찾으면 반환한다.
    /// 상대 경로는 기준을 모르므로 건너뛴다. 그대로 쓰면 호스트 시작 폴더 기준으로 해석된다.
    /// 경로 존재는 검사하지 않는다.
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
            let path = Path::new(s);
            if path.is_absolute()
                && let Some(parent) = path.parent()
            {
                return Some(parent.to_path_buf());
            }
        }
        None
    }
}

/// 등록 과정에서 host가 선택한 렌더링 방식. plugin의 요청 선언과 구분한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisteredRendering {
    HostEgui,
    EguiMesh,
    Webview,
    Remote,
}

impl RegisteredRendering {
    /// 전송용 이름. host-egui는 host 내장 종류에 사용하는 값이다.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HostEgui => "host-egui",
            Self::EguiMesh => "egui-mesh",
            Self::Webview => "webview",
            Self::Remote => "remote",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KindSource {
    HostBuiltin,
    Plugin(String),
}

impl KindSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HostBuiltin => "host",
            Self::Plugin(_) => "plugin",
        }
    }

    pub fn plugin_id(&self) -> Option<&str> {
        match self {
            Self::HostBuiltin => None,
            Self::Plugin(id) => Some(id.as_str()),
        }
    }
}

/// 종류별 메타데이터와 동작. Arc로 보관하며 콜백은 Send + Sync + static이다.
pub struct SurfaceKindDef {
    pub kind: &'static str,

    /// 등록 과정에서 선택한 렌더링 방식.
    pub rendering: RegisteredRendering,

    pub source: KindSource,

    /// 종류 이름의 번역 키. 개별 surface의 표시명과는 별개다.
    pub display_name_i18n_key: &'static str,

    /// 아이콘 이름. None이면 UI의 기본 파일 아이콘을 사용한다.
    pub icon: Option<String>,

    /// 호출자가 정한 cwd와 종류별 params로 새 surface를 만든다.
    /// cwd 사용 여부는 각 종류가 결정하며 포커스에서 암묵적으로 가져오지 않는다.
    #[allow(clippy::type_complexity)]
    pub create: Arc<
        dyn Fn(SurfaceId, Option<&Path>, &serde_json::Value) -> anyhow::Result<PreparedKind>
            + Send
            + Sync,
    >,

    /// Generic의 저장 데이터로 복원한다. PTY 생성이 필요한 Terminal은 별도 복원 경로를 쓴다.
    #[allow(clippy::type_complexity)]
    pub restore:
        Arc<dyn Fn(SurfaceId, &serde_json::Value) -> anyhow::Result<PreparedKind> + Send + Sync>,

    /// None이면 layout 저장에서 제외한다.
    pub snapshot: SurfaceSnapshotFn,

    /// 종류별 프리셋 편집 필드. plugin은 매니페스트에서, host 내장은 코드에서 채운다.
    pub preset_fields: Vec<PresetFieldSpec>,

    /// 요청 파라미터의 별칭과 표준 키 대응.
    pub param_aliases: HashMap<String, String>,

    /// 없는 params 키에 넣을 기본값. @settings.explorer_view_mode와 @home은 host가 해석한다.
    pub default_params: HashMap<String, String>,

    /// 입력·줌·복사 기능. 매니페스트 선언 또는 내장 등록 값으로 설정한다.
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "headless에서도 매니페스트 값을 저장하지만 읽는 곳은 GUI의 입력·줌·복사 처리다"
        )
    )]
    pub consumes_egui_input: bool,
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "매니페스트 값은 두 빌드에서 저장하고 GUI에서만 읽는다"
        )
    )]
    pub zoomable: bool,
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "매니페스트 값은 두 빌드에서 저장하고 GUI에서만 읽는다"
        )
    )]
    pub egui_copy: bool,
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "매니페스트 값은 두 빌드에서 저장하고 GUI에서만 읽는다"
        )
    )]
    pub copy_path: bool,
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(
            dead_code,
            reason = "매니페스트 값은 두 빌드에서 저장하고 GUI에서만 읽는다"
        )
    )]
    pub egui_paste: bool,

    /// 이 params 키의 파일 이름을 탭 이름으로 사용한다. 없으면 종류 표시명을 쓴다.
    pub name_from_param: Option<String>,

    /// `name_from_param` 키가 params 에 없을 때 create 에 넘길 cwd 와 params 로 탭 이름을 고른다.
    /// capture 한 preset 처럼 snapshot 을 params 로 받는 종류가 쓴다. `None` 이면 종류 표시명을 쓴다.
    pub tab_name_from_params: Option<SurfaceTabNameFn>,

    /// 파일 열기 진입점에서 최근 파일 목록에 기록할지 여부.
    pub records_recent: bool,

    /// 종류 변환 전에 파일 입력 팝업을 열어야 하는지 여부.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "매니페스트 값은 두 빌드에서 저장하고 GUI에서만 읽는다"
        )
    )]
    pub convert_requires_input: bool,

    /// 변환 입력 팝업의 plugin_id/popup_id. 등록 시 소유 plugin ID를 붙인다.
    #[cfg_attr(
        not(feature = "gui"),
        expect(
            dead_code,
            reason = "매니페스트 값은 두 빌드에서 저장하고 GUI에서만 읽는다"
        )
    )]
    pub convert_input_popup: Option<String>,
}

impl SurfaceKindDef {
    /// 필수인 Params 대상 필드의 키. cwd·startup 전용 필드는 제외한다.
    pub fn required_params(&self) -> impl Iterator<Item = &str> {
        self.preset_fields.iter().filter_map(|f| match &f.target {
            PresetFieldTarget::Params(key) if f.required => Some(key.as_str()),
            _ => None,
        })
    }

    /// 문자열이 없거나 빈 필수 키를 처음 찾으면 반환한다. 공백만 있는 문자열은 통과한다.
    pub fn first_missing_required_param(&self, params: &serde_json::Value) -> Option<&str> {
        self.required_params().find(|key| {
            params
                .get(*key)
                .and_then(|v| v.as_str())
                .map(str::is_empty)
                .unwrap_or(true)
        })
    }

    /// 표준 키가 없을 때 별칭을 옮긴다. 표준 키가 이미 있으면 별칭도 그대로 남긴다.
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

/// RwLock으로 보호하는 종류 등록부. 조회도 락을 얻으며 읽기가 대기하지 않는다는 보장은 없다.
/// 철회된 정의는 열린 surface의 snapshot·메타데이터 조회를 위해 남긴다.
/// get은 철회된 정의도 반환하고 get_live·contains·kinds_snapshot은 제외한다.
#[derive(Default)]
pub struct SurfaceKindRegistry {
    kinds: RwLock<HashMap<&'static str, KindEntry>>,
    /// 반복 조회 중 같은 poison 로그를 계속 남기지 않기 위한 표식.
    poison_reported: std::sync::atomic::AtomicBool,
}

struct KindEntry {
    metadata: Arc<super::kind_catalog::KindMetadata>,
    def: Arc<SurfaceKindDef>,
    withdrawn_by: Option<String>,
}

/// 미등록 종류와 달리 제공 plugin을 다시 연결해야 하는 상태임을 안내한다.
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

    /// poison 때문에 등록 목록 전체가 없는 것처럼 보이지 않도록 첫 로그 후 기존 값을 읽는다.
    /// 논리적 상태를 재검증하거나 부분 갱신을 되돌리는 복구는 아니다.
    fn lock_read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<&'static str, KindEntry>> {
        crate::poison::recover_read(
            self.kinds.read(),
            "surface kind registry",
            &self.poison_reported,
        )
    }

    fn lock_write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<&'static str, KindEntry>> {
        crate::poison::recover_write(
            self.kinds.write(),
            "surface kind registry",
            &self.poison_reported,
        )
    }

    /// 같은 kind는 덮어쓰고 철회 표시도 해제한다. 소유자 충돌 검사는 등록 호출자 몫이다.
    pub fn register(&self, def: SurfaceKindDef) {
        let kind = def.kind;
        let mut map = self.lock_write();
        let def = Arc::new(def);
        let metadata = Arc::new(super::kind_catalog::KindCatalog::metadata(def.clone()));
        let entry = KindEntry {
            metadata,
            def,
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

    /// 해당 plugin의 정의를 남긴 채 철회한다. 이번에 새로 철회한 이름을 정렬해 반환한다.
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

    /// 열린 surface의 snapshot·메타데이터 조회용. 철회된 정의도 반환한다.
    pub fn get(&self, kind: &str) -> Option<Arc<SurfaceKindDef>> {
        self.lock_read().get(kind).map(|e| e.def.clone())
    }

    /// 철회되지 않은 정의만 반환한다. plugin의 현재 응답 가능 여부까지 확인하지는 않는다.
    pub fn get_live(&self, kind: &str) -> Option<Arc<SurfaceKindDef>> {
        self.lock_read()
            .get(kind)
            .filter(|e| e.withdrawn_by.is_none())
            .map(|e| e.def.clone())
    }

    pub fn withdrawn_by(&self, kind: &str) -> Option<String> {
        self.lock_read()
            .get(kind)
            .and_then(|e| e.withdrawn_by.clone())
    }

    pub fn contains(&self, kind: &str) -> bool {
        self.get_live(kind).is_some()
    }

    /// 철회되지 않은 정의의 Arc 사본. 반환 뒤 등록·철회가 바뀔 수 있다.
    #[cfg(test)]
    pub fn kinds_snapshot(&self) -> Vec<(&'static str, Arc<SurfaceKindDef>)> {
        self.lock_read()
            .iter()
            .filter(|(_, e)| e.withdrawn_by.is_none())
            .map(|(k, e)| (*k, e.def.clone()))
            .collect()
    }

    // 이유: 현재 호출자는 없지만 len/is_empty 조회 쌍을 유지한다.
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

/// name_from_param의 마지막 경로 성분을 우선한다. 그 키가 없으면 종류의 tab_name_from_params 를,
/// 그것도 없으면 번역된 종류 이름 또는 kind 문자열을 쓴다.
pub(crate) fn default_tab_name_for_kind(
    kind: &str,
    cwd: Option<&Path>,
    params: &serde_json::Value,
    def: Option<&SurfaceKindDef>,
) -> String {
    fn basename_or(path: &str, fallback: &str) -> String {
        path.split(['/', '\\'])
            .rfind(|s| !s.is_empty())
            .unwrap_or(fallback)
            .to_string()
    }
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
    if let Some(name) = def
        .and_then(|d| d.tab_name_from_params)
        .and_then(|name_of| name_of(cwd, params))
    {
        return name;
    }
    fallback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_survives_a_poisoned_lock() {
        let reg = std::sync::Arc::new(SurfaceKindRegistry::new());

        let held = std::sync::Arc::clone(&reg);
        let joined = std::thread::spawn(move || {
            let _g = held.kinds.write().expect("fresh lock");
            panic!("poison the registry");
        })
        .join();
        assert!(joined.is_err());

        crate::runtime::surface_registry::builtins::register_builtin_kinds(&reg);
        assert!(reg.contains("terminal"), "poison 후에도 등록이 반영된다");
        assert!(reg.get("terminal").is_some());
        assert!(!reg.kinds_snapshot().is_empty());
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
            tab_name_from_params: None,
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

    #[test]
    fn a_withdrawn_kind_retains_definition_but_is_absent_from_live_catalog() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut engine_session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine");
        let engine = engine_session.borrow_mut();
        let reg = &engine.runtime.surface_registry;
        reg.register(plugin_def("probe_kind", "com.x.probe"));
        let original = reg.get_live("probe_kind").expect("live definition");

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
        assert!(Arc::ptr_eq(&original, &reg.get("probe_kind").unwrap()));
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
        let mut p = serde_json::json!({"file_path": "/a/b.md"});
        d.normalize_param_aliases(&mut p);
        assert_eq!(p, serde_json::json!({"file": "/a/b.md"}));
        // 표준 키가 있으면 별칭을 지우지 않는 현재 동작도 보존한다.
        let mut p2 = serde_json::json!({"file": "/keep.md", "file_path": "/drop.md"});
        d.normalize_param_aliases(&mut p2);
        assert_eq!(p2["file"], "/keep.md");
    }

    #[test]
    fn default_tab_name_uses_name_from_param() {
        let mut d = dummy_def("markdown");
        d.name_from_param = Some("file".to_string());
        assert_eq!(
            super::default_tab_name_for_kind(
                "markdown",
                None,
                &serde_json::json!({"file": "/a/b/README.md"}),
                Some(&d),
            ),
            "README.md"
        );
        assert_eq!(
            super::default_tab_name_for_kind("markdown", None, &serde_json::json!({}), Some(&d),),
            "test.dummy"
        );
        let plain = dummy_def("empty");
        assert_eq!(
            super::default_tab_name_for_kind(
                "empty",
                None,
                &serde_json::json!({"file": "/x/y.md"}),
                Some(&plain),
            ),
            "test.dummy"
        );
        assert_eq!(
            super::default_tab_name_for_kind("plugin_x", None, &serde_json::json!({}), None),
            "plugin_x"
        );
    }

    #[test]
    fn builtin_registers_host_kinds() {
        let reg = SurfaceKindRegistry::new();
        register_builtin_kinds(&reg);
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

impl SurfaceKindRegistry {
    #[cfg(feature = "gui")]
    pub(super) fn metadata(
        &self,
        kind: &str,
        live: bool,
    ) -> Option<Arc<super::kind_catalog::KindMetadata>> {
        self.lock_read()
            .get(kind)
            .filter(|entry| !live || entry.withdrawn_by.is_none())
            .map(|entry| entry.metadata.clone())
    }
    pub(super) fn metadata_snapshot(
        &self,
    ) -> Vec<(&'static str, Arc<super::kind_catalog::KindMetadata>)> {
        self.lock_read()
            .iter()
            .filter(|(_, entry)| entry.withdrawn_by.is_none())
            .map(|(kind, entry)| (*kind, entry.metadata.clone()))
            .collect()
    }
}
