//! Modeless 에디터 윈도우 — Workspace/Tab/Pane preset 편집.
//!
//! `SettingsView` 구조를 따르되 modal 이 아닌 modeless 로 동작:
//! - 다른 윈도우 입력을 차단하지 않음
//! - Esc 로 닫히지 않음
//! - 엔진 전역 단일 인스턴스는 `App.preset_view_id` 가 관리
//! - 구조 편집은 즉시 store 가 디스크 동기화 (별도 save 버튼 없음). surface 파라미터는
//!   설정 화면의 draft 로 고치고 확인을 눌러야 저장된다

use std::sync::{Arc, Mutex};

use winit::event::WindowEvent;

use tasty_presets::{PresetKind, PresetStore};
use tasty_settings::KeybindingSettings;

use crate::adapters::ui::preset::demo_layout::KindCatalog;
use crate::adapters::ui::preset::surface_settings::SurfaceCfg;
use crate::adapters::ui::{LayoutContext, ToastManager, ToastScope};
use crate::core::surface_registry::SurfaceKindRegistry;
use crate::gpu::GpuState;
use crate::i18n::t;
use crate::view::ui::{View, sealed};
use crate::view::{ViewAction, ViewBase, ViewCtx};

pub struct PresetView {
    pub base: ViewBase,
    store: Arc<Mutex<PresetStore>>,
    /// 편집기 kind 드롭다운/라벨의 진실 소스. main engine 의 공유 Arc 를 clone 해
    /// 받는다(부재 시 `None` → 빈 catalog → 정적 fallback). 프레임마다 스냅샷을
    /// 파생해 런타임 등록 kind(플러그인 on/off)를 즉시 반영한다.
    surface_registry: Option<Arc<SurfaceKindRegistry>>,
    /// 편집 모드 표준 단축키 매칭용 스냅샷. open 시 focused window 설정에서 clone
    /// (appearance 주입과 동일 전례) — 설정 변경은 창 재오픈 시 반영되는 기존 한계.
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
    toasts: ToastManager,
    shown: bool,
}

impl PresetView {
    pub fn new(
        gpu: GpuState,
        winit: Arc<winit::window::Window>,
        store: Arc<Mutex<PresetStore>>,
        surface_registry: Option<Arc<SurfaceKindRegistry>>,
        keybindings: KeybindingSettings,
    ) -> Self {
        Self {
            base: ViewBase::new(gpu, winit),
            store,
            surface_registry,
            keybindings,
            active_kind: PresetKind::Workspace,
            selected_workspace: None,
            selected_tab: None,
            selected_pane: None,
            editing: false,
            selected_node: None,
            surface_cfg: None,
            toasts: ToastManager::new(),
            shown: false,
        }
    }

    /// 우클릭/IPC 진입 시 특정 preset 선택 상태로 열기 위한 helper.
    ///
    /// 창 밖에서 선택이 바뀌는 경로다. 열려 있던 surface 설정 화면의 draft 는 취소와
    /// 똑같이 버리고 묻지 않는다 — 사용자가 다른 동작을 시작했고, draft 는 저장된다고
    /// 약속된 적이 없다.
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
                self.base.modifiers = m.state();
            }
            _ => {}
        }
        ViewAction::None
    }

    fn render(&mut self) {
        if !self.base.dirty {
            return;
        }
        self.base.begin_frame();

        let raw_input = self.base.gpu.take_egui_input(&self.base.winit);
        let store_arc = self.store.clone();
        // registry 스냅샷을 프레임마다 파생 — 미주입이면 빈 catalog(정적 fallback).
        let catalog = self
            .surface_registry
            .as_ref()
            .map(|r| KindCatalog::from_registry(r))
            .unwrap_or_default();
        let active_kind = &mut self.active_kind;
        let sel_ws = &mut self.selected_workspace;
        let sel_tab = &mut self.selected_tab;
        let sel_pane = &mut self.selected_pane;
        let editing = &mut self.editing;
        let selected_node = &mut self.selected_node;
        let surface_cfg = &mut self.surface_cfg;
        let toasts = &mut self.toasts;
        let keybindings = &self.keybindings;

        let full_output = self.base.gpu.run_egui(raw_input, |ctx| {
            let mut store_guard = crate::poison::recover_mutex(
                store_arc.lock(),
                crate::core::PRESET_STORE_WHAT,
                &crate::core::PRESET_STORE_POISONED,
            );
            crate::preset_ui::draw_preset_panel(
                ctx,
                &mut store_guard,
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
            drop(store_guard);

            let empty_layout = LayoutContext {
                active_workspace: 0,
                pane_rects: Vec::new(),
                surface_rects: Vec::new(),
                active_tabs: Vec::new(),
            };
            toasts.draw(ctx, &empty_layout, false);
        });

        let has_copy = full_output
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::CopyText(_)));
        if has_copy {
            self.toasts.push_info(t("toast.copied"), ToastScope::Window);
            self.mark_dirty();
        }

        self.base
            .gpu
            .finish_egui_frame(&self.base.winit, full_output);

        if !self.shown {
            self.base.winit.set_visible(true);
            self.shown = true;
        }

        if self.base.dirty {
            self.base.winit.request_redraw();
        }
    }
}

impl sealed::Sealed for PresetView {}
