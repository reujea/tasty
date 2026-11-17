//! Kind metadata and opaque registration identity for presentation. Factories stay in the registry.
use super::surface_registry::{
    KindSource, PresetFieldSpec, RegisteredRendering, SurfaceKindDef, SurfaceKindRegistry,
};
use std::sync::Arc;
#[cfg(feature = "gui")]
use std::sync::Weak;
#[derive(Clone)]
pub(crate) struct KindCatalog(Arc<SurfaceKindRegistry>);
#[cfg(feature = "gui")]
#[derive(Clone, Debug)]
pub(crate) struct Registration(Weak<SurfaceKindDef>);
#[cfg(feature = "gui")]
impl Registration {
    pub(crate) fn matches(&self, definition: &Arc<SurfaceKindDef>) -> bool {
        self.0.ptr_eq(&Arc::downgrade(definition))
    }
}
#[derive(Clone)]
pub(crate) struct KindMetadata {
    pub(crate) rendering: RegisteredRendering,
    pub(crate) source: KindSource,
    pub(crate) display_name_i18n_key: &'static str,
    pub(crate) icon: Option<String>,
    pub(crate) preset_fields: Vec<PresetFieldSpec>,
    /// host 기능 능력. 선언이 없을 때의 해석은 등록 때 이미 끝났다.
    #[cfg(feature = "gui")]
    pub(crate) capabilities: super::surface_registry::KindCapabilities,
    #[cfg(feature = "gui")]
    pub(crate) consumes_egui_input: bool,
    #[cfg(feature = "gui")]
    pub(crate) zoomable: bool,
    /// 폐기 예정. 회신(`edit_outcome`)을 약속한 plugin 의 surface 에는 효과가 없다(편집 키 판정이 먼저 받는다). 약속하지 않은 plugin 의 surface 에만 복사 단축키를 egui `Copy` 로 보낸다.
    #[cfg(feature = "gui")]
    pub(crate) egui_copy: bool,
    #[cfg(feature = "gui")]
    pub(crate) copy_path: bool,
    #[cfg(feature = "gui")]
    pub(crate) egui_paste: bool,
    #[cfg(feature = "gui")]
    pub(crate) convert_requires_input: bool,
    #[cfg(feature = "gui")]
    pub(crate) convert_input_popup: Option<String>,
    #[cfg(feature = "gui")]
    registration: Registration,
}
impl KindMetadata {
    #[cfg(feature = "gui")]
    pub(crate) fn registration(&self) -> Registration {
        self.registration.clone()
    }
    pub(crate) fn required_params(&self) -> impl Iterator<Item = &str> {
        self.preset_fields
            .iter()
            .filter_map(|field| match &field.target {
                super::surface_registry::PresetFieldTarget::Params(key) if field.required => {
                    Some(key.as_str())
                }
                _ => None,
            })
    }
}

impl KindCatalog {
    pub(crate) fn new(registry: Arc<SurfaceKindRegistry>) -> Self {
        Self(registry)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn get(&self, kind: &str) -> Option<Arc<KindMetadata>> {
        self.0.metadata(kind, false)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn get_live(&self, kind: &str) -> Option<Arc<KindMetadata>> {
        self.0.metadata(kind, true)
    }
    pub(crate) fn kinds_snapshot(&self) -> Vec<(&'static str, Arc<KindMetadata>)> {
        self.0.metadata_snapshot()
    }
    pub(super) fn metadata(def: Arc<SurfaceKindDef>) -> KindMetadata {
        KindMetadata {
            rendering: def.rendering.clone(),
            source: def.source.clone(),
            display_name_i18n_key: def.display_name_i18n_key,
            icon: def.icon.clone(),
            preset_fields: def.preset_fields.clone(),
            #[cfg(feature = "gui")]
            capabilities: def.capabilities,
            #[cfg(feature = "gui")]
            consumes_egui_input: def.consumes_egui_input,
            #[cfg(feature = "gui")]
            zoomable: def.zoomable,
            #[cfg(feature = "gui")]
            egui_copy: def.egui_copy,
            #[cfg(feature = "gui")]
            copy_path: def.copy_path,
            #[cfg(feature = "gui")]
            egui_paste: def.egui_paste,
            #[cfg(feature = "gui")]
            convert_requires_input: def.convert_requires_input,
            #[cfg(feature = "gui")]
            convert_input_popup: def.convert_input_popup.clone(),
            #[cfg(feature = "gui")]
            registration: Registration(Arc::downgrade(&def)),
        }
    }
}
