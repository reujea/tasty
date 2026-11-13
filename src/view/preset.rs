//! Modeless 에디터 윈도우 — Workspace/Tab/Pane preset 편집.
//!
//! `SettingsView` 구조를 따르되 modal 이 아닌 modeless 로 동작:
//! - 다른 윈도우 입력을 차단하지 않음
//! - Esc 로 닫히지 않음
//! - 엔진 전역 단일 인스턴스는 `App.preset_view_id` 가 관리
//! - 구조 편집은 값 요청을 App에 반환해 저장한다(별도 save 버튼 없음). surface 파라미터는
//!   설정 화면의 draft 로 고치고 확인을 눌러야 저장된다

pub(crate) mod draft;

use std::sync::Arc;

use draft::{PresetApplied, PresetDrafts, PresetEdit};

use winit::event::WindowEvent;

use tasty_presets::PresetKind;
use tasty_settings::KeybindingSettings;

use crate::adapters::ui::preset::demo_layout::KindCatalog;
use crate::adapters::ui::preset::surface_settings::SurfaceCfg;
use crate::adapters::ui::{LayoutContext, ToastManager, ToastScope};
use crate::gpu::GpuState;
use crate::i18n::t;
use crate::runtime::kind_catalog::KindCatalog as SurfaceKindCatalog;
use crate::view::ui::{View, sealed};
use crate::view::{ViewAction, ViewBase, ViewCtx};

pub struct PresetView {
    pub base: ViewBase,
    drafts: PresetDrafts,
    /// 등록된 surface kind의 공유 목록. 매 프레임 읽어 plugin 활성 상태를 반영한다.
    /// 목록이 없으면 정적 기본 목록을 사용한다.
    surface_registry: Option<SurfaceKindCatalog>,
    /// 창을 열 때 가져온 편집 단축키 설정. 변경된 설정은 다시 열어야 반영된다.
    keybindings: KeybindingSettings,
    active_kind: PresetKind,
    selected_workspace: Option<String>,
    selected_tab: Option<String>,
    selected_pane: Option<String>,
    /// WYSIWYG 편집 모드 여부(Edit↔Done 토글).
    editing: bool,
    /// 편집 모드에서 선택된 surface leaf 의 안정 id.
    selected_node: Option<usize>,
    /// 열려 있는 surface 설정 화면(대상 leaf + draft). `None` 이면 미리보기가 보인다.
    surface_cfg: Option<SurfaceCfg>,
    file_chooser: crate::view::file_chooser::WindowFileChooser,
    pending_browse: Option<crate::adapters::ui::preset::surface_settings::FieldBrowseRequest>,
    pub(crate) toasts: ToastManager,
    /// This window's clipboard: egui widget copies go through the copy door with it.
    clipboard: crate::clipboard::terminal::TerminalClipboard,
    pending_presentation: Option<PendingPresentation>,
    shown: bool,
}

struct PendingPresentation {
    workspace: Option<String>,
    tab: Option<String>,
    pane: Option<String>,
    editing: bool,
    node: Option<usize>,
    cfg: Option<SurfaceCfg>,
    toolbar: crate::adapters::ui::preset::ToolbarDraft,
}

impl PresetView {
    pub fn new(
        gpu: GpuState,
        winit: Arc<winit::window::Window>,
        clipboard: crate::clipboard::terminal::TerminalClipboard,
        drafts: PresetDrafts,
        surface_registry: Option<SurfaceKindCatalog>,
        keybindings: KeybindingSettings,
    ) -> Self {
        Self {
            base: ViewBase::new(gpu, winit),
            clipboard,
            drafts,
            surface_registry,
            keybindings,
            active_kind: PresetKind::Workspace,
            selected_workspace: None,
            selected_tab: None,
            selected_pane: None,
            editing: false,
            selected_node: None,
            surface_cfg: None,
            file_chooser: Default::default(),
            pending_browse: None,
            toasts: ToastManager::new(),
            pending_presentation: None,
            shown: false,
        }
    }

    pub(crate) fn refresh_drafts(&mut self, drafts: PresetDrafts) {
        if !self.drafts.has_edits() {
            self.drafts = drafts;
        }
    }

    pub(crate) fn take_edits(&mut self) -> Vec<PresetEdit> {
        if self.drafts.is_pending() {
            return Vec::new();
        }
        let edits = self.drafts.take_edits();
        if !edits.is_empty() {
            self.drafts.mark_pending();
        }
        edits
    }

    pub(crate) fn accept_edits(
        &mut self,
        drafts: PresetDrafts,
        error: Option<String>,
        applied: &[PresetApplied],
    ) {
        self.drafts = drafts;
        if error.is_some() {
            // Admission updated local selection and buffers optimistically. Restore them only
            // after the App reports failure so rename/delete cannot discard the user's editing state.
            if let Some(mut previous) = self.pending_presentation.take() {
                self.selected_workspace = previous.workspace;
                self.selected_tab = previous.tab;
                self.selected_pane = previous.pane;
                self.editing = previous.editing;
                self.selected_node = previous.node;
                self.surface_cfg = previous.cfg;
                previous.toolbar.reconcile(applied);
                previous.toolbar.restore(&self.base.gpu.egui_ctx);
                for result in applied {
                    let kind = match result {
                        PresetApplied::Rename { kind, .. }
                        | PresetApplied::Delete { kind, .. }
                        | PresetApplied::Save { kind, .. } => *kind,
                    };
                    let selected = match kind {
                        PresetKind::Workspace => &mut self.selected_workspace,
                        PresetKind::Tab => &mut self.selected_tab,
                        PresetKind::Pane => &mut self.selected_pane,
                    };
                    match result {
                        PresetApplied::Rename { from, to, .. } => {
                            if selected.is_none() || selected.as_deref() == Some(from.as_str()) {
                                *selected = Some(to.clone());
                            }
                            if let Some(cfg) = &mut self.surface_cfg {
                                cfg.remap_preset(kind, from, to);
                            }
                        }
                        PresetApplied::Delete { name, .. } => {
                            if selected.as_deref() == Some(name.as_str()) {
                                *selected = None;
                                self.selected_node = None;
                            }
                            if self
                                .surface_cfg
                                .as_ref()
                                .is_some_and(|cfg| cfg.belongs_to(kind, name))
                            {
                                self.surface_cfg = None;
                            }
                        }
                        PresetApplied::Save {
                            name,
                            created: true,
                            ..
                        } => {
                            *selected = Some(name.clone());
                        }
                        _ => {}
                    }
                }
            }
            self.toasts.push(
                t("preset.toast.save_failed"),
                crate::adapters::ui::ToastKind::Error,
                ToastScope::Window,
            );
        } else {
            self.pending_presentation = None;
        }
        self.mark_dirty();
    }

    /// 외부에서 특정 프리셋을 선택한다. 열려 있던 surface 설정 초안은 버린다.
    pub fn select(&mut self, kind: PresetKind, name: String) {
        self.surface_cfg = None;
        self.active_kind = kind;
        match kind {
            PresetKind::Workspace => self.selected_workspace = Some(name),
            PresetKind::Tab => self.selected_tab = Some(name),
            PresetKind::Pane => self.selected_pane = Some(name),
        }
        self.mark_dirty();
    }
}

impl View for PresetView {
    fn poll_local_reads(&mut self, owner: &mut crate::app::local_reads::ReadRequests) {
        if self.file_chooser.poll(owner) {
            self.mark_dirty();
        }
    }

    fn base(&self) -> &ViewBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut ViewBase {
        &mut self.base
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_event(&mut self, event: WindowEvent, _ctx: &mut ViewCtx<'_>) -> ViewAction {
        // RedrawRequested 를 egui 에 넘기면 항상 repaint=true → mark_dirty →
        // request_redraw → RedrawRequested 무한 루프(busy-loop)가 된다. 렌더는
        // 아래 RedrawRequested arm 이 담당하므로 egui input 으로 넘기지 않는다.
        let repaint = if matches!(&event, WindowEvent::RedrawRequested) {
            false
        } else {
            self.base.gpu.handle_egui_event(&self.base.winit, &event).1
        };
        if repaint {
            self.mark_dirty();
        }

        match event {
            WindowEvent::CloseRequested => {
                return ViewAction::Close;
            }
            WindowEvent::Resized(size) => {
                self.base.gpu.resize(size);
                self.mark_dirty();
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::CursorMoved { .. } => {
                self.mark_dirty();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.base.state.modifiers = m.state();
            }
            _ => {}
        }
        ViewAction::None
    }

    fn render(&mut self) {
        if !self.base.state.dirty {
            return;
        }
        self.base.begin_frame();

        let raw_input = self.base.gpu.take_egui_input(&self.base.winit);
        let drafts = &mut self.drafts;
        if !drafts.is_pending() {
            self.pending_presentation = Some(PendingPresentation {
                workspace: self.selected_workspace.clone(),
                tab: self.selected_tab.clone(),
                pane: self.selected_pane.clone(),
                editing: self.editing,
                node: self.selected_node,
                cfg: self.surface_cfg.clone(),
                toolbar: crate::adapters::ui::preset::ToolbarDraft::capture(
                    &self.base.gpu.egui_ctx,
                ),
            });
        }
        // registry 스냅샷을 프레임마다 파생 — 미주입이면 빈 catalog(정적 fallback).
        let catalog = self
            .surface_registry
            .as_ref()
            .map(KindCatalog::from_registry)
            .unwrap_or_default();
        let active_kind = &mut self.active_kind;
        let sel_ws = &mut self.selected_workspace;
        let sel_tab = &mut self.selected_tab;
        let sel_pane = &mut self.selected_pane;
        let editing = &mut self.editing;
        let selected_node = &mut self.selected_node;
        let surface_cfg = &mut self.surface_cfg;
        let chooser = &mut self.file_chooser;
        let pending_browse = &mut self.pending_browse;
        let toasts = &mut self.toasts;
        let keybindings = &self.keybindings;

        let full_output = self.base.gpu.run_egui(raw_input, |ctx| {
            reconcile_browse(surface_cfg, chooser, pending_browse, &catalog);
            chooser.mark_input_owner(ctx);
            crate::preset_ui::draw_preset_panel(
                ctx,
                drafts,
                active_kind,
                sel_ws,
                sel_tab,
                sel_pane,
                editing,
                selected_node,
                surface_cfg,
                toasts,
                &catalog,
                keybindings,
            );
            if let Some(request) = surface_cfg
                .as_mut()
                .and_then(SurfaceCfg::take_browse_request)
                && chooser.open("preset.field", request.mode.clone(), &request.start)
            {
                *pending_browse = Some(request);
            }
            chooser.draw(ctx);
            if let Some(outcome) = chooser.take_outcome("preset.field")
                && let (Some(request), Some(cfg), Some(path)) = (
                    pending_browse.take(),
                    surface_cfg.as_mut(),
                    outcome.into_first_path(),
                )
            {
                cfg.apply_browse(&request, path.to_string_lossy().into_owned(), &catalog);
            }

            let empty_layout = LayoutContext {
                active_workspace: 0,
                active_workspace_id: 0,
                pane_rects: Vec::new(),
                surface_rects: Vec::new(),
                active_tabs: Vec::new(),
                active_tab_ids: Vec::new(),
                workspace_rect: None,
            };
            toasts.draw(ctx, &empty_layout, false);
        });

        if let Some(previous) = &mut self.pending_presentation {
            previous.toolbar.retain_input(&self.base.gpu.egui_ctx);
            if let Some(submitted) =
                crate::adapters::ui::preset::take_confirmed_surface_cfg(&self.base.gpu.egui_ctx)
            {
                previous.cfg = Some(submitted);
            }
        }

        self.base
            .gpu
            .finish_egui_frame(&self.base.winit, full_output);
        if crate::clipboard::copy::copy_egui_text(
            &mut self.base.gpu,
            &mut self.clipboard,
            &mut self.toasts,
        ) {
            self.mark_dirty();
        }

        if !self.shown {
            self.base.winit.set_visible(true);
            self.shown = true;
        }

        if self.base.state.dirty {
            self.base.winit.request_redraw();
        }
    }
}

impl sealed::Sealed for PresetView {}

fn reconcile_browse(
    cfg: &Option<SurfaceCfg>,
    chooser: &mut crate::view::file_chooser::WindowFileChooser,
    pending: &mut Option<crate::adapters::ui::preset::surface_settings::FieldBrowseRequest>,
    catalog: &KindCatalog,
) {
    if pending.as_ref().is_some_and(|request| {
        !cfg.as_ref()
            .is_some_and(|cfg| cfg.accepts_browse(request, catalog))
    }) {
        chooser.cancel();
        chooser.take_outcome("preset.field");
        *pending = None;
    }
}
