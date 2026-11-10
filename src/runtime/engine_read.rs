//! Borrowed display queries. No runtime owner, transport sender, writer, waker or task control escapes.
use super::{
    engine_access::{EngineRef, ObservedPresentation},
    terminal_store::TerminalStore,
};
#[cfg(feature = "gui")]
use crate::model::Tab;
use crate::{
    core::{CoreState, live::LiveDomainState},
    model::{StructurePresentation, Surface},
};
use std::{
    collections::{HashMap, HashSet},
    ops::Deref,
};

#[derive(Clone)]
pub(crate) struct EngineRead<'a> {
    pub(crate) core: &'a CoreState,
    pub(crate) layout_slot: Option<crate::core::layout_persistence::LayoutSlotId>,
    pub(crate) live: &'a LiveDomainState,
    pub(crate) settings: &'a crate::settings::Settings,
    pub(crate) terminals: TerminalRead<'a>,
    pub(crate) surface_registry: super::kind_catalog::KindCatalog,
    #[cfg(feature = "gui")]
    pub(crate) file_handler: HandlerCatalog<'a>,
    #[cfg(feature = "gui")]
    pub(crate) explorer_favorites: &'a [crate::core::explorer_favorites::ExplorerFavorite],
    #[cfg(feature = "gui")]
    pub(crate) port_favorites: &'a [crate::core::port_favorites::PortFavorite],
    #[cfg(feature = "gui")]
    pub(crate) attach_mesh_frames: &'a crate::remote::mesh_frames::AttachMeshFrameStore,
    #[cfg(feature = "gui")]
    explorer_writable: &'a HashSet<u32>,
    #[cfg(feature = "gui")]
    explorer_hosts: &'a HashMap<u32, crate::remote::state::ExplorerRemoteHost>,
    #[cfg(feature = "gui")]
    mirror_locations: &'a HashMap<u32, crate::remote::state::MirrorLocation>,
    /// 저장된 원격 행의 연결 상태 이름(`workspace.list`의 `remote_state`와 같은 값).
    #[cfg(feature = "gui")]
    row_states: &'a HashMap<u32, &'static str>,
    #[cfg(feature = "gui")]
    mirror_capture_pastes: &'a HashMap<u32, crate::remote::state::MirrorCapturePaste>,
    surfaces: &'a HashMap<u32, Box<dyn Surface>>,
    mirror_cwd: &'a HashMap<u32, crate::core::state::RemoteCwd>,
    #[cfg(feature = "gui")]
    html_entry_hosts: &'a HashSet<u32>,
    mirror_busy: &'a HashSet<u32>,
    #[cfg(feature = "gui")]
    mirror_foreground: &'a crate::core::state::MirrorForegrounds,
    #[cfg(feature = "gui")]
    readonly: &'a HashMap<u32, tasty_terminal::Terminal>,
    #[cfg(feature = "gui")]
    dag_source: super::dag_query::DagSource<'a>,
}
#[derive(Clone, Copy)]
pub(crate) struct TerminalRead<'a>(&'a TerminalStore);
impl<'a> TerminalRead<'a> {
    #[cfg(feature = "gui")]
    pub(crate) fn get(&self, id: u32) -> Option<&'a tasty_terminal::Terminal> {
        self.0.get(id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn contains(&self, id: u32) -> bool {
        self.0.contains(id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn generation(&self, id: u32) -> Option<tasty_terminal::ResourceGeneration> {
        self.0.generation(id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn has_pty(&self, id: u32) -> bool {
        self.0.pty(id).is_some()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn foreground_process_name(&self, id: u32) -> Option<String> {
        self.0
            .pty(id)?
            .foreground_process_info()
            .map(|info| info.name)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn process_id(&self, id: u32) -> Option<u32> {
        self.0.pty(id)?.process_id()
    }
    pub(crate) fn cwd(&self, id: u32) -> Option<std::path::PathBuf> {
        self.0.cwd(id)
    }
}
impl<'a> EngineRef<'a> {
    pub(crate) fn read(&self) -> EngineRead<'a> {
        EngineRead {
            core: self.core,
            layout_slot: self.persistence.slot,
            live: self.live,
            settings: &self.runtime.settings,
            terminals: TerminalRead(&self.runtime.terminals),
            surface_registry: super::kind_catalog::KindCatalog::new(
                self.runtime.surface_registry.clone(),
            ),
            #[cfg(feature = "gui")]
            file_handler: HandlerCatalog(&self.runtime.file_handler),
            #[cfg(feature = "gui")]
            explorer_favorites: &self.runtime.explorer_favorites.items,
            #[cfg(feature = "gui")]
            port_favorites: &self.runtime.port_favorites.items,
            #[cfg(feature = "gui")]
            attach_mesh_frames: &self.remote.attach_mesh_frames,
            #[cfg(feature = "gui")]
            explorer_writable: &self.remote.explorer_writable,
            #[cfg(feature = "gui")]
            explorer_hosts: &self.remote.explorer_hosts,
            #[cfg(feature = "gui")]
            mirror_locations: &self.remote.mirror_locations,
            #[cfg(feature = "gui")]
            row_states: &self.remote.row_states,
            #[cfg(feature = "gui")]
            mirror_capture_pastes: &self.remote.mirror_capture_pastes,
            surfaces: &self.runtime.surfaces,
            mirror_cwd: &self.remote.mirror_surface_cwd,
            #[cfg(feature = "gui")]
            html_entry_hosts: &self.remote.html_entry_hosts,
            mirror_busy: &self.remote.mirror_busy_surfaces,
            #[cfg(feature = "gui")]
            mirror_foreground: &self.remote.mirror_foreground,
            #[cfg(feature = "gui")]
            readonly: &self.runtime.readonly_views,
            #[cfg(feature = "gui")]
            dag_source: self.runtime.dag_reads.source(self.journal_binding),
        }
    }
}
#[cfg(feature = "gui")]
impl EngineRead<'_> {
    /// surface 가 든 mirror workspace 의 로컬 ID. 로컬 surface 거나 찾지 못하면 None.
    pub(crate) fn mirror_workspace_of(&self, surface_id: u32) -> Option<u32> {
        let (idx, _) = self.find_workspace_index_for_surface(surface_id)?;
        self.workspace_at(idx)
            .filter(|ws| ws.mirror)
            .map(|ws| ws.id)
    }

    /// surface 가 든 mirror workspace 의 원격 위치. 로컬 surface 이거나 연결 정보가 없으면 None.
    /// 저장된 원격 행의 연결 상태. 아직 상태가 실리지 않은 행은 연결 전으로 본다.
    pub(crate) fn remote_row_state(&self, workspace_id: u32) -> tasty_ui_widgets::RemoteRowState {
        use tasty_ui_widgets::RemoteRowState as S;
        match self.row_states.get(&workspace_id).copied() {
            Some("connected") => S::Connected,
            Some("connecting") => S::Connecting,
            Some("reconnecting") => S::Reconnecting,
            Some("disconnected") => S::Disconnected,
            _ => S::NotConnected,
        }
    }

    pub(crate) fn mirror_location_of(
        &self,
        surface_id: u32,
    ) -> Option<&crate::remote::state::MirrorLocation> {
        self.mirror_workspace_of(surface_id)
            .and_then(|ws| self.mirror_locations.get(&ws))
    }

    /// 원격 위치 표시(링크 툴팁, 원격 picker·다운로드 대화상자 경로 줄 앞 표지)의 호스트 글자
    /// ([원격 위치 표시](../../docs/features/remote-attach/index.md#원격-위치-표시)).
    /// 연결 정보가 아직 없으면 빈 글자이고 화면은 `Remote` 낱말을 쓴다.
    pub(crate) fn mirror_host_text(&self, surface_id: u32) -> String {
        self.mirror_location_of(surface_id)
            .map(|location| location.host.clone())
            .unwrap_or_default()
    }

    /// 이 mirror workspace에 아직 붙여넣을 수 있는 스크린샷 기록.
    #[cfg(feature = "gui")]
    pub(crate) fn mirror_capture_paste(
        &self,
        workspace: u32,
    ) -> Option<&crate::remote::state::MirrorCapturePaste> {
        self.mirror_capture_pastes.get(&workspace)
    }

    /// 이 explorer 칸의 파일을 바꿀 수 없는가. 원격 칸인데 그 연결의 서버가 원격 쓰기를 알리지 않았거나
    /// 연결이 끊긴 때다. 로컬 칸은 false 다.
    pub(crate) fn blocks_explorer_write(&self, surface_id: u32) -> bool {
        self.mirror_workspace_of(surface_id)
            .is_some_and(|ws| !self.explorer_writable.contains(&ws))
    }

    /// 이 explorer 칸의 쓰기를 거절하는 이유. 쓸 수 있는 칸이면 None 이다.
    pub(crate) fn explorer_write_refusal(&self, surface_id: u32) -> Option<String> {
        if !self.blocks_explorer_write(surface_id) {
            return None;
        }
        let host = self
            .mirror_workspace_of(surface_id)
            .and_then(|ws| self.explorer_hosts.get(&ws));
        Some(crate::remote::state::explorer_write_refusal(host))
    }
}

impl Deref for EngineRead<'_> {
    type Target = CoreState;
    fn deref(&self) -> &CoreState {
        self.core
    }
}
impl<'a> EngineRead<'a> {
    #[cfg(feature = "gui")]
    pub(crate) fn html_entry_available(&self, workspace: u32) -> bool {
        self.html_entry_hosts.contains(&workspace)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn html_receive(
        &self,
        sid: u32,
    ) -> Option<crate::runtime::html_receive::ReceiveSnapshot> {
        let remote = self
            .surfaces
            .get(&sid)?
            .as_any()
            .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()?;
        let target = crate::runtime::surface_binding::SurfaceBinding::capture(self, sid)?;
        crate::runtime::html_receive::snapshot(remote, target)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn html_script(
        &self,
        sid: u32,
    ) -> Option<crate::runtime::html_script::HtmlSnapshot> {
        let remote = self
            .surfaces
            .get(&sid)?
            .as_any()
            .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()?;
        if remote.kind_static != "html" {
            return None;
        }
        let target = crate::runtime::surface_binding::SurfaceBinding::capture(self, sid)?;
        Some(crate::runtime::html_script::snapshot(remote, target))
    }
    #[cfg(feature = "gui")]
    pub(crate) fn as_ref(&self) -> Self {
        self.clone()
    }
    pub(crate) fn find_surface_by_id(&self, id: u32) -> Option<SurfaceRead<'a>> {
        self.surfaces.get(&id).map(|surface| SurfaceRead {
            inner: surface.as_ref(),
        })
    }
    #[cfg(feature = "gui")]
    pub(crate) fn find_terminal_by_id(&self, id: u32) -> Option<&'a tasty_terminal::Terminal> {
        self.terminals.get(id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn visible_terminal(&self, id: u32) -> Option<&'a tasty_terminal::Terminal> {
        if self.live.occupancy.is_hard_occupied(id) {
            self.readonly.get(&id)
        } else {
            self.terminals.get(id)
        }
    }
    #[cfg(feature = "gui")]
    pub(crate) fn tab_display_name(&self, tab: &Tab, surface: Option<u32>) -> String {
        tab.display_name(surface.and_then(|id| self.live.surface_titles.get(&id)))
    }
    pub(crate) fn observed_presentation<'b>(
        &'b self,
        selection: &'b dyn StructurePresentation,
    ) -> ObservedPresentation<'b> {
        ObservedPresentation::new(selection, &self.live.surface_titles)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn surface_display_path(
        &self,
        id: u32,
        selection: &dyn StructurePresentation,
    ) -> Option<tasty_core::SurfaceDisplayPath> {
        self.core
            .surface_display_path(id, &self.observed_presentation(selection))
    }
    pub(crate) fn surface_cwd(&self, id: u32) -> Option<crate::core::state::SurfaceCwd> {
        use crate::core::state::{RemoteCwd, SurfaceCwd};
        if let Some(cwd) = self.mirror_cwd.get(&id) {
            return Some(SurfaceCwd::Remote(cwd.clone()));
        }
        let surface = self.find_surface_by_id(id)?;
        let cwd = if surface.kind() == "terminal" {
            self.terminals.cwd(id)
        } else {
            surface.source_cwd()
        }?;
        Some(if self.is_mirror_surface(id) {
            SurfaceCwd::Remote(RemoteCwd::new(cwd.to_string_lossy().into_owned()))
        } else {
            SurfaceCwd::Local(cwd)
        })
    }
    pub(crate) fn local_surface_cwd(&self, id: u32) -> Option<std::path::PathBuf> {
        self.surface_cwd(id)
            .and_then(crate::core::state::SurfaceCwd::into_local)
    }
    pub(crate) fn is_surface_busy(&self, id: u32) -> bool {
        self.live.busy_surfaces.contains(&id) || self.mirror_busy.contains(&id)
    }
    pub(crate) fn busy_count(&self, ids: &[u32]) -> usize {
        ids.iter().filter(|id| self.is_surface_busy(**id)).count()
    }
    /// 로컬 폴링 값이 없으면 원격 mirror가 받은 값을 돌려준다.
    #[cfg(feature = "gui")]
    pub(crate) fn foreground_name(&self, id: u32) -> Option<&str> {
        self.live
            .foreground_names
            .get(&id)
            .map(String::as_str)
            .or_else(|| self.mirror_foreground.name(id))
    }
    /// mirror는 로컬 PTY가 없어 1Hz 캐시 값만 있다. 로컬은 PTY를 직접 조회한다.
    #[cfg(feature = "gui")]
    pub(crate) fn foreground_process_name_now(&self, id: u32) -> Option<String> {
        self.terminals
            .foreground_process_name(id)
            .or_else(|| self.mirror_foreground.name(id).map(str::to_owned))
    }
    #[cfg(feature = "gui")]
    pub(crate) fn foreground_generation(&self, id: u32) -> u64 {
        self.live
            .foreground_generation
            .get(&id)
            .copied()
            .or_else(|| self.mirror_foreground.generation(id))
            .unwrap_or(0)
    }
    pub(crate) fn is_surface_mouse_capture_disabled(&self, id: u32) -> bool {
        self.live.mouse_capture_disabled_surfaces.contains(&id)
    }
    #[cfg(feature = "gui")]
    pub(crate) fn dag_source(&self) -> super::dag_query::DagSource<'_> {
        self.dag_source
    }
}

#[cfg(feature = "gui")]
#[derive(Clone, Copy)]
pub(crate) struct HandlerCatalog<'a>(&'a crate::file::handler::FileHandlerRegistry);
#[cfg(feature = "gui")]
impl HandlerCatalog<'_> {
    pub(crate) fn handler(
        &self,
        id: &tasty_file_handler::HandlerId,
    ) -> Option<tasty_file_handler::FileHandler> {
        self.0.handler(id)
    }

    pub(crate) fn all_handlers(&self) -> Vec<tasty_file_handler::FileHandler> {
        self.0.all_handlers()
    }
}

/// Borrowed display capability. No Any cast or runtime object reference escapes this wrapper.
#[derive(Clone, Copy)]
pub(crate) struct SurfaceRead<'a> {
    inner: &'a dyn Surface,
}
impl<'a> SurfaceRead<'a> {
    pub(crate) fn kind(&self) -> &'static str {
        self.inner.kind()
    }
    pub(crate) fn source_cwd(&self) -> Option<std::path::PathBuf> {
        self.inner.source_cwd()
    }
    pub(crate) fn to_tree_json(self) -> serde_json::Value {
        self.inner.to_tree_json()
    }
    // These builtin models contain ordinary values; their mutation requires &mut ownership.
    #[cfg(feature = "gui")]
    pub(crate) fn empty(&self) -> Option<&'a crate::model::EmptySurface> {
        self.inner.as_any().downcast_ref()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn explorer(&self) -> Option<&'a crate::model::ExplorerPanel> {
        self.inner.as_any().downcast_ref()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn dag(&self) -> Option<&'a crate::model::DagGraphSurface> {
        self.inner.as_any().downcast_ref()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn mesh(&self) -> Option<&'a super::egui_mesh_surface::EguiMeshSurface> {
        self.inner.as_any().downcast_ref()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn attach_mesh(&self) -> Option<&'a crate::model::AttachMeshSurface> {
        self.inner.as_any().downcast_ref()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn remote_webview(&self) -> Option<WebviewRead<'a>> {
        self.inner
            .as_any()
            .downcast_ref()
            .map(|inner| WebviewRead { inner })
    }
}
#[cfg(feature = "gui")]
#[derive(Clone, Copy)]
pub(crate) struct WebviewRead<'a> {
    inner: &'a crate::plugin_bridge::remote_surface::RemoteSurface,
}
#[cfg(feature = "gui")]
impl WebviewRead<'_> {
    pub(crate) fn kind(&self) -> &'static str {
        self.inner.kind_static
    }
    pub(crate) fn plugin_id(&self) -> &str {
        &self.inner.plugin_id
    }
    pub(crate) fn url(&self) -> Option<String> {
        self.inner.webview_url()
    }
    pub(crate) fn label(&self) -> Option<String> {
        self.inner.webview_label()
    }
    pub(crate) fn nav_state(&self) -> crate::model::NavState {
        self.inner.nav_state()
    }
    /// backend가 관측한 commit 주소와 히스토리 이동 가능 여부의 사본.
    pub(crate) fn nav_info(&self) -> tasty_model::WebViewNavInfo {
        self.inner.webview_nav()
    }
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    /// 클릭 커서 이동의 셸 판정: mirror 는 로컬 PTY 가 없어 서버가 보낸 이름을 쓴다.
    #[test]
    fn foreground_process_name_now_falls_back_to_the_mirror_name() {
        let (_, mut session) = crate::state::tests::test_state();
        let mut e = session.borrow_mut();
        assert_eq!(e.read().foreground_process_name_now(4242), None);
        e.set_mirror_surface_foreground(4242, Some("zsh".into()));
        assert_eq!(
            e.read().foreground_process_name_now(4242).as_deref(),
            Some("zsh")
        );
        e.set_mirror_surface_foreground(4242, None);
        assert_eq!(e.read().foreground_process_name_now(4242), None);
    }
}
