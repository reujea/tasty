mod attach_mesh_input;
pub(crate) mod clipboard;
#[cfg(debug_assertions)]
pub(crate) mod debug_input;
#[cfg(debug_assertions)]
pub(crate) mod debug_menu;
mod divider_drag;
mod egui_mesh;
mod explorer_create;
mod explorer_history;
mod explorer_os_items;
mod explorer_show_in_folder;
mod file_drag;
mod file_drop;
mod fullscreen_window;
mod html_address_keys;
mod html_content;
mod keyboard;
mod link_menu;
mod menu_target;
mod mouse;
mod move_menu;
mod preset_actions;
pub(crate) mod redraw;
mod saved_row_menu;
pub(crate) mod selection;
mod shutdown;
pub(crate) mod vi_copy;
mod webview_menu;

pub(crate) mod ime;

pub(crate) use divider_drag::DividerDrag;

use crate::runtime::engine_read::EngineRead;
use std::sync::Arc;

use winit::event::WindowEvent;
use winit::keyboard::ModifiersState;

use crate::gpu::{GpuState, ImePreeditState};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::selection::TextSelection;
use crate::state::MainViewState;
use crate::view::ui::{View, sealed};
use crate::view::{ViewAction, ViewBase, ViewCtx};
use crate::{AppEvent, ClipboardContext};

/// 메인 터미널 윈도우. 워크스페이스/사이드바/탭을 갖고 터미널 계열 Surface를 호스팅한다.
/// `View` + `sealed::Sealed` 를 직접 구현한다.
pub struct MainView {
    pub base: ViewBase,
    pub(crate) state: MainViewState,
    pub(crate) cursor_position: Option<winit::dpi::PhysicalPosition<f64>>,
    pub(crate) dragging_divider: Option<DividerDrag>,
    pub(crate) clipboard: ClipboardContext,
    pub(crate) ime_preedit: Option<ImePreeditState>,
    pub(crate) ime_request_target: Option<ime::CompositionTarget>,
    #[cfg(target_os = "linux")]
    pub(crate) explorer_ime_keys: crate::adapters::ui::input::explorer_ime::ExplorerImeKeys,
    pub(crate) proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    pub(crate) text_selection: Option<TextSelection>,
    /// vi-style 키보드 복사 모드. Some 일 때 키 입력이 PTY 로 전달되지 않고
    /// vi_copy::handle_vi_key 가 가로채 cursor/visual/yank 등을 처리.
    pub(crate) vi_copy: Option<vi_copy::ViCopyMode>,
    pub(crate) left_mouse_down: bool,
    pub(crate) file_drag: file_drag::Gesture,
    /// TUI에 실제 누름을 보낸 버튼·surface 목록. 마지막 항목으로 드래그 대상을 정한다.
    /// 로컬 선택과 분리해 Shift 우회·링크 클릭 등 보고하지 않은 누름의 이동도 보내지 않는다.
    /// 우클릭은 포커스를 옮기지 않으므로 포커스와 다른 surface가 대상일 수 있다.
    pub(crate) report_buttons_down: Vec<(u8, u32)>,
    /// Shift로 TUI 입력을 우회해 시작한 로컬 선택. 드래그 중 Shift를 떼도 release까지 유지한다.
    pub(crate) left_select_bypass: bool,
    /// 링크 열기로 소비한 누름인지. 해당 뗌도 TUI에 보내지 않아 단독 release로 링크가 다시 열리는 것을 막는다.
    pub(crate) link_click_consumed: bool,
    /// 링크 위 우클릭 당시 메뉴 정보. Linux는 뗄 때 메뉴를 열므로 포인터·modifier 변경과 무관하게 보존한다.
    pub(crate) right_link_press: Option<crate::state::TerminalLinkMenu>,
    /// 마지막으로 보고한 surface·셀 위치. surface를 포함해 같은 좌표의 다른 화면 입력까지 중복으로 버리지 않는다.
    pub(crate) last_mouse_report_cell: Option<(u32, usize, usize)>,
    pub(crate) last_click_time: Option<std::time::Instant>,
    pub(crate) last_click_pos: Option<tasty_selection::SelectionPoint>,
    pub(crate) click_count: u8,
    /// Whether IME composition is active (set by Ime::Enabled/Disabled).
    /// When true, KeyboardInput text is ignored — only Ime::Commit sends text.
    pub(crate) ime_active: bool,
    /// Accumulated cursor advance from IME commits (in terminal columns).
    /// After Ime::Commit, the PTY echo hasn't been processed yet, so
    /// cursor_position() returns a stale value. This offset compensates
    /// so the next Preedit anchor appears after the committed text.
    pub(crate) ime_cursor_advance: usize,
    /// Raw cursor position when ime_cursor_advance was last updated.
    /// Used to reconcile: if the raw cursor moved past this point, PTY
    /// echo has caught up and advance should be reduced accordingly.
    pub(crate) ime_advance_base: (usize, usize),
    /// 포커스된 mesh surface가 보낸 IME 커서 영역. 조합 중이 아니어도
    /// 입력 위젯이 포커스 상태이면 OS 후보창 위치에 사용한다.
    pub(crate) egui_mesh_ime_cursor_area: Option<crate::model::PhysicalRect>,
    /// Detector for double-tap modifier shortcuts (e.g. Shift+Shift).
    pub(crate) double_tap: crate::double_tap::DoubleTapDetector,
    /// Native WebView instances keyed by surface ID.
    pub(crate) webviews: std::collections::HashMap<u32, crate::webview::PlatformWebView>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    pub(crate) http_preparing:
        std::collections::HashMap<u32, crate::webview::http_native::PendingNative>,
    pub(crate) webview_runtime:
        std::collections::HashMap<u32, crate::app::html_runtime::NativeWebviewBinding>,
    /// surface 별 마지막으로 webview 에 적용한 HTML 설정 — 변경 시에만 재적용(매 프레임 호출 회피).
    pub(crate) webview_applied_settings:
        std::collections::HashMap<u32, crate::webview::HtmlWebViewSettings>,
    /// surface별 마지막으로 페이지에 둔 바탕 포커스 상태. 바탕 kind WebView만 들어 있다.
    pub(crate) webview_bed_focus: std::collections::HashMap<u32, bool>,
    /// surface 별 마지막으로 webview 에 로드한 URL — `surface.webview_url()` 최신값과 달라지면
    /// (예: `webview.set_url` IPC) 기존 webview 인스턴스에 재로드를 트리거한다(파괴·재생성 없음).
    pub(crate) webview_loaded_urls: std::collections::HashMap<u32, String>,
    /// surface별 webview 생성 시도 수. 실패가 창 이벤트를 만들어 스스로 재시도를
    /// 반복할 수 있어 MAX_WEBVIEW_CREATE_ATTEMPTS로 제한한다.
    pub(crate) webview_create_attempts: std::collections::HashMap<u32, u32>,
    /// 이번 프레임에 만든 lazy kind WebView의 (surface, kind 소유 plugin). App이 프레임 동기화 뒤
    /// `surface.webview_created`로 알리고 비운다 — 그 plugin은 만들어지기 전 메시지를 잃었다.
    pub(crate) lazy_webviews_created: Vec<(u32, String)>,
    /// 탐색 완료를 기다려 아직 표시하지 못한 webview의 시작 시각·경고 여부.
    /// 오래 기다리면 한 번 기록하되 임의 재시도나 표시 방식 변경은 하지 않는다.
    pub(crate) webview_reveal_pending: std::collections::HashMap<u32, (std::time::Instant, bool)>,
    /// native 자식 창에서 받은 키·포커스를 host로 전달하는 브리지.
    pub(crate) webview_key_bridge: std::rc::Rc<crate::webview::WebViewKeyBridge>,
    /// overlay 가 열려 webview 키보드 포커스를 이미 host 로 회수했는지(edge 판정).
    /// overlay 가 닫히면 false 로 돌아가 다음 개폐에 다시 1 회만 회수한다.
    pub(crate) webview_overlay_focus_released: bool,
    /// 직전 동기화에서 활성 탭에 있던 webview surface. 탭·workspace 전환으로
    /// 활성 탭에서 빠진 surface를 한 번만 가려내는 edge 판정에 쓴다.
    pub(crate) webview_prev_active: std::collections::HashSet<u32>,
    /// 배너 발화 판정이 마지막으로 본 포커스 surface. 첫 프레임 전에는 `None`이다.
    pub(crate) html_script_seen_focus: Option<Option<u32>>,
    /// html surface별 지금 보여야 할 배너 단계. 숨김은 넣지 않는다.
    pub(crate) html_script_phases:
        std::collections::HashMap<u32, tasty_model::html_script::BannerPhase>,
    /// 활성 탭에서 빠졌지만 창이 OS 포커스를 갖지 않아 아직 키보드 포커스를 회수하지 않은 surface.
    pub(crate) webview_focus_release_pending: std::collections::HashSet<u32>,
    /// 마지막 동기화에서 실제 표시 중인 native webview가 있는지. Linux 키 조회 타이머에 사용한다.
    pub(crate) webview_any_visible: bool,
    /// 바인딩 정책을 매 프레임 만들지 않도록 마지막 원본 설정과 카테고리 사용 여부를 보관한다.
    pub(crate) webview_policy_src: Option<(crate::settings::KeybindingSettings, bool)>,
    /// 같은 스냅샷을 만들 때 본 plugin 쪽 epoch 쌍
    /// (`PluginCommandRegistry::revision`, `PluginsConfig::shortcut_revision`).
    /// plugin manager 가 아직 없으면 `None`. 두 값 모두 프로세스 전역 단조 증가라
    /// registry 가 통째로 재생성돼도 이전 값과 겹치지 않는다.
    pub(crate) webview_policy_plugin_epoch: Option<(u64, u64)>,
    /// 현재 마우스 hover 중이고 수식키 조건을 만족한 링크. 렌더 및 클릭에 사용.
    pub(crate) hovered_link: Option<HoveredLink>,
    /// 가장 최근에 터미널에 paste한 시각. Ctrl+V 직후 사용자가 옆 키 Ctrl+C를 잘못 눌러
    /// 입력을 날려버리는 사고를 막기 위해 cooldown 구간 안의 Ctrl+C는 무시한다.
    pub(crate) last_terminal_paste_at: Option<std::time::Instant>,
    /// 로컬 mesh surface에 보낸 컨텍스트 상태.
    pub(crate) egui_mesh: std::collections::HashMap<u32, egui_mesh::MeshForwardState>,
    /// attach mesh mirror surface(`AttachMeshSurface`) 별 client→server MeshContext/
    /// MeshInput forward 추적(`docs/dev-guide/egui-mesh-channel.md`의 "attach mesh
    /// mirror 소비 경로" 참고). `egui_mesh`의 attach 대응 — `attach_mesh_input.rs` 참고.
    pub(crate) attach_mesh_input:
        std::collections::HashMap<u32, attach_mesh_input::AttachMeshForwardState>,
    /// 마지막으로 pointer_moved 를 forward 한 mesh surface — `CursorLeft` 및
    /// surface 전환 시 `PointerGone` 1 회 forward 판정에 쓴다. `mouse.rs::update_mesh_hover`.
    pub(crate) mesh_pointer_hover: Option<MeshHoverTarget>,
    /// Linux 비동기 네이티브 메뉴와 결과 처리. macOS·Windows는 호출에서 결과가 돌아온다.
    pub(crate) pending_menu: Option<PendingNativeMenuSlot>,
    /// 메뉴를 닫는 클릭의 누름·뗌을 모두 소비한다. 누름만 막으면 egui가 다음 프레임에 클릭으로 처리할 수 있다.
    pub(crate) menu_dismiss_swallow: Vec<winit::event::MouseButton>,
    /// 직전 무대 활성 상태. 진입한 프레임에서 IME·드래그·메뉴 등 진행 중 입력을 정리한다.
    pub(crate) stage_was_active: bool,
    /// debug 입력으로 만든 메뉴 요청. 실제 OS 메뉴 대신 보관해 테스트에서 조회한다.
    /// 실제 사용자 우클릭과 release 빌드에는 사용하지 않는다.
    #[cfg(debug_assertions)]
    pub(crate) debug_captured_menu: Option<crate::state::PendingNativeMenu>,
    /// debug IPC가 둔 다음 native 메뉴의 응답. 실제 사용자 우클릭과 release 빌드에는 사용하지 않는다.
    #[cfg(debug_assertions)]
    pub(crate) debug_menu_answer: Option<debug_menu::MenuAnswer>,
    /// `process_pending_native_menu`가 지금 여는 메뉴의 종류. 응답을 그 종류에만 쓰는 데 사용한다.
    #[cfg(debug_assertions)]
    pub(crate) debug_opening_menu: Option<&'static str>,
    /// 무대가 OS 전체화면에 들어가기 전의 창 상태. fullscreen_window에서 복원한다.
    pub(crate) stage_saved_window_mode: Option<fullscreen_window::SavedWindowMode>,
}

/// 화면에 떠 있는 네이티브 컨텍스트 메뉴 핸들 + 그 결과를 받을 continuation.
/// `MainView::pending_menu` 슬롯의 내용물.
pub(crate) type PendingNativeMenuSlot = (
    crate::platform::native_menu::MenuHandle,
    Box<dyn FnOnce(&mut MainView, &EngineRead<'_>, Option<u32>)>,
);

/// Ctrl+V 직후 Ctrl+C를 SIGINT로 흘려보내지 않을 보호 시간.
pub(crate) const PASTE_CTRL_C_COOLDOWN: std::time::Duration = std::time::Duration::from_millis(500);

/// 마우스가 위에 있고 설정된 수식키 조건을 만족한 링크.
#[derive(Debug, Clone)]
pub(crate) struct HoveredLink {
    pub revision: u64,
    pub surface_id: u32,
    pub uri: String,
    pub highlight: crate::terminal_link::LinkHighlight,
    /// mirror 의 파일 경로 링크이면 원격 위치 툴팁의 자리와 호스트 글자. 로컬·URL 은 None.
    pub remote_tip: Option<crate::adapters::ui::link_remote_tip::LinkRemoteAnchor>,
}

/// 마지막 mesh hover 대상. 로컬·원격 경로가 배타적이므로 하나만 보관하고 대상 변경 때 PointerGone을 보낸다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshHoverTarget {
    Local(u32),
    Attach(u32),
}

impl MainView {
    pub(crate) fn new(
        gpu: GpuState,
        mut state: MainViewState,
        window: Arc<winit::window::Window>,
        proxy: winit::event_loop::EventLoopProxy<AppEvent>,
        display_owner: Option<winit::event_loop::OwnedDisplayHandle>,
    ) -> Self {
        let display = crate::clipboard::error::DisplayBackend::of(&window);
        state.file_clipboard.set_display(display);
        let base = ViewBase::new(gpu, window);
        state.webview_identity = base.state.identity();
        let file_drag = file_drag::Gesture::new(base.winit.clone(), display_owner);
        Self {
            base,
            state,
            cursor_position: None,
            dragging_divider: None,
            clipboard: ClipboardContext::new(display),
            ime_preedit: None,
            #[cfg(target_os = "linux")]
            explorer_ime_keys: Default::default(),
            ime_request_target: None,
            proxy,
            text_selection: None,
            vi_copy: None,
            left_mouse_down: false,
            file_drag,
            report_buttons_down: Vec::new(),
            left_select_bypass: false,
            link_click_consumed: false,
            right_link_press: None,
            last_mouse_report_cell: None,
            last_click_time: None,
            last_click_pos: None,
            click_count: 0,
            ime_active: false,
            ime_cursor_advance: 0,
            egui_mesh_ime_cursor_area: None,
            ime_advance_base: (0, 0),
            double_tap: crate::double_tap::DoubleTapDetector::new(),
            webviews: std::collections::HashMap::new(),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            http_preparing: std::collections::HashMap::new(),
            webview_runtime: std::collections::HashMap::new(),
            webview_applied_settings: std::collections::HashMap::new(),
            webview_bed_focus: std::collections::HashMap::new(),
            webview_loaded_urls: std::collections::HashMap::new(),
            webview_create_attempts: std::collections::HashMap::new(),
            lazy_webviews_created: Vec::new(),
            webview_reveal_pending: std::collections::HashMap::new(),
            webview_key_bridge: std::rc::Rc::new(crate::webview::WebViewKeyBridge::new()),
            webview_overlay_focus_released: false,
            webview_prev_active: std::collections::HashSet::new(),
            html_script_seen_focus: None,
            html_script_phases: std::collections::HashMap::new(),
            webview_focus_release_pending: std::collections::HashSet::new(),
            webview_any_visible: false,
            webview_policy_src: None,
            webview_policy_plugin_epoch: None,
            hovered_link: None,
            last_terminal_paste_at: None,
            egui_mesh: std::collections::HashMap::new(),
            attach_mesh_input: std::collections::HashMap::new(),
            mesh_pointer_hover: None,
            pending_menu: None,
            menu_dismiss_swallow: Vec::new(),
            stage_was_active: false,
            #[cfg(debug_assertions)]
            debug_captured_menu: None,
            #[cfg(debug_assertions)]
            debug_menu_answer: None,
            #[cfg(debug_assertions)]
            debug_opening_menu: None,
            stage_saved_window_mode: None,
        }
    }

    /// 아직 결과가 안 나온 네이티브 컨텍스트 메뉴가 이 창에 떠 있는지.
    /// 이벤트 루프가 폴링 주기를 예약할지 판단하는 데 쓴다
    /// (`app/event_handler.rs::about_to_wait`).
    pub(crate) fn has_pending_native_menu(&self) -> bool {
        self.pending_menu.is_some()
    }

    /// Request this window to close (will be handled by the event loop).
    pub(crate) fn request_close(&mut self) {
        self.base.state.close_requested = true;
    }

    pub fn compute_terminal_rect(&self) -> PhysicalRect {
        let size = self.base.gpu.size();
        crate::model::compute_terminal_rect(
            PhysicalPx(size.width as f32),
            PhysicalPx(size.height as f32),
            self.state.sidebar_width,
            crate::adapters::ui::titlebar::top_inset(self.base.gpu.scale_factor()),
            crate::adapters::ui::status_bar_bottom_inset(self.base.gpu.scale_factor()),
            self.base.gpu.scale_factor(),
        )
    }

    /// 현재 preedit이 있으면 원래 surface에 확정 전송하고 IME 상태를 리셋한다.
    /// 단축키 소비/포커스 전환 직전에 호출.
    pub(crate) fn flush_ime_preedit(&mut self, engine: &EngineRead<'_>) {
        ime::flush_preedit(self, engine);
    }

    /// 현재 preedit을 PTY로 보내지 않고 버린다.
    /// 팝업/오버레이가 열릴 때 사용.
    pub(crate) fn clear_ime_preedit(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        ime::clear_preedit(self, engine);
    }

    /// PTY 출력 처리 후 cursor가 움직였을 수 있을 때 preedit anchor를 재계산한다.
    pub(crate) fn recalc_ime_preedit_anchor(&mut self, engine: &EngineRead<'_>) {
        ime::recalc_anchor(self, engine);
    }

    /// 조합 입력 대상과 같은 순서로 plugin 팝업·mesh surface·터미널의 IME 후보창 위치를 고른다.
    /// plugin 위젯은 host의 PlatformOutput에 없으므로 mesh 프레임으로 받은 위치를 사용한다.
    pub(crate) fn update_ime_cursor_area(&self, engine: &EngineRead<'_>) {
        // 무대 중에는 보이지 않는 배경 surface의 IME 위치를 사용하지 않는다.
        if self.state.fullscreen_stage_active() {
            return;
        }
        if let Some(area) = self.state.plugin_popup_ime_cursor_area {
            self.set_ime_cursor_area(area);
            return;
        }
        if let Some(area) = self.egui_mesh_ime_cursor_area {
            self.set_ime_cursor_area(area);
            return;
        }
        let Some(preedit) = &self.ime_preedit else {
            return;
        };
        let Some(terminal) = engine.visible_terminal(preedit.surface_id) else {
            return;
        };
        let display_row = terminal.with_view(
            &self.state.terminal_views.get(engine, preedit.surface_id),
            |view| {
                if preedit.anchor.epoch != view.cut().epoch {
                    return None;
                }
                preedit
                    .anchor
                    .absolute_row
                    .checked_sub(view.viewport().top_row)
                    .filter(|row| *row < view.rows())
            },
        );
        let Some(display_row) = display_row else {
            return;
        };
        let terminal_rect = self.compute_terminal_rect();
        let Some(cell_rect) = self.state.surface_cell_rect(
            engine,
            terminal_rect,
            preedit.surface_id,
            preedit.anchor.col,
            display_row,
            self.base.gpu.cell_width(),
            self.base.gpu.cell_height(),
            self.base.gpu.scale_factor(),
        ) else {
            return;
        };
        self.set_ime_cursor_area(cell_rect);
    }

    /// 후보창 영역을 최소 1px로 만들어 OS에 전달하고 trace로 기록한다.
    /// OS 후보창이 앱 캡처에 없을 때 위치 확인에 사용한다.
    fn set_ime_cursor_area(&self, area: crate::model::PhysicalRect) {
        use winit::dpi::{PhysicalPosition, PhysicalSize};
        tracing::trace!(
            "ime cursor area: x={} y={} w={} h={}",
            area.x.value(),
            area.y.value(),
            area.width.value(),
            area.height.value()
        );
        self.base.winit.set_ime_cursor_area(
            PhysicalPosition::new(area.x.value().round() as i32, area.y.value().round() as i32),
            PhysicalSize::new(
                area.width.value().max(1.0).round() as u32,
                area.height.value().max(1.0).round() as u32,
            ),
        );
    }

    /// 표면과 터미널 격자를 맞출 창 크기. Windows 의 장식 없는 창은 최소화 중 작업 표시줄 아이콘
    /// 크기(측정 160x28)를 client 크기로 보고하므로, 그동안은 지금 표면 크기를 그대로 쓴다. 표면과
    /// PTY 를 그 크기로 줄이지 않고, 복원 때 오는 크기로 맞춘다.
    fn window_surface_size(&self) -> winit::dpi::PhysicalSize<u32> {
        if cfg!(windows) && self.base.winit.is_minimized() == Some(true) {
            self.base.gpu.size()
        } else {
            self.base.winit.inner_size()
        }
    }

    /// 창의 engine과 함께 창 이벤트를 처리한다. engine은 App registry가 창 ID로 찾아 넘긴다.
    fn handle_engine_event(
        &mut self,
        engine: &EngineRead<'_>,
        event: WindowEvent,
        ctx: &mut ViewCtx<'_>,
    ) -> ViewAction {
        if self.outbound_drag_event(engine, &event, ctx.modal_active) {
            return ViewAction::None;
        }
        // If a modal is active, block all input events before they reach egui.
        // Only allow non-input events (resize, redraw, scale factor, focus) through.
        if modal_blocks_input(ctx.modal_active, &event) {
            return ViewAction::None;
        }

        #[cfg(target_os = "linux")]
        if self.consume_explorer_ime_key(engine, &event) {
            return ViewAction::None;
        }

        // 팝업·host 위젯의 키 입력만 egui에 전달하고 나머지는 중앙 디스패처로 보낸다.
        if self.route_tutorial_input(&event) {
            return ViewAction::None;
        }
        let is_keyboard_event = matches!(
            &event,
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
        );
        let is_modifiers_event = matches!(&event, WindowEvent::ModifiersChanged(_));

        // 무대 입력은 egui에 전달하며 배경으로 보내지 않는 검사는 keyboard.rs에서 한다.
        let overlay_open =
            self.state.keyboard_overlay_open() || self.state.fullscreen_stage_active();

        // registry의 consumes_egui_input인 host 위젯만 egui에 직접 보낸다.
        // plugin mesh 입력은 별도 전달 경로를 사용해 host가 먼저 소비하지 않게 한다.
        let egui_surface = self
            .state
            .focused_surface_type(engine)
            .kind_capability(engine, |d| d.consumes_egui_input);

        let is_redraw_event = matches!(&event, WindowEvent::RedrawRequested);

        // 메뉴를 닫는 클릭은 누름부터 뗌까지 egui와 Tasty 모두에 전달하지 않는다.
        // 두 경로가 같은 판단을 쓰도록 egui 전달 전에 한 번 판정한다.
        let menu_dismiss_swallow = self.begin_menu_dismiss_swallow(&event);

        let (egui_consumed, egui_repaint) = if is_redraw_event {
            // RedrawRequested를 egui에 넣으면 다시 그리기를 반복 요청하므로 제외한다.
            (false, false)
        } else if is_keyboard_event {
            if overlay_open || egui_surface {
                let from = self.base.gpu.egui_input_events_mut().len();
                let result = self.base.gpu.handle_egui_event(&self.base.winit, &event);
                self.remap_html_address_keys(engine, &event, from);
                result
            } else {
                (false, false)
            }
        } else if is_modifiers_event {
            let (_, repaint) = self.base.gpu.handle_egui_event(&self.base.winit, &event);
            (false, repaint)
        } else if menu_dismiss_swallow {
            (false, true)
        } else {
            self.base.gpu.handle_egui_event(&self.base.winit, &event)
        };

        if egui_repaint {
            self.mark_dirty();
        }

        let was_dirty = self.base.state.dirty;

        match event {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.base.gpu.sync_scale_factor(&self.base.winit);
                let new_size = self.window_surface_size();
                self.base.gpu.resize(new_size);
                if self.state.fullscreen_stage_active() {
                    // 무대 중에는 배경 PTY 크기를 유지한다. 나간 뒤 첫 프레임에서 현재 영역으로 맞춘다.
                    self.state.stage_deferred_grid_resync = true;
                } else {
                    let terminal_rect = self.compute_terminal_rect();
                    let (cols, rows) = self.base.gpu.grid_size_for_rect(&terminal_rect);
                    self.state.dispatch_intent(
                        crate::intent::Intent::Frame(
                            crate::app::frame_effect::GeometryEffect::DefaultGrid { cols, rows }
                                .into(),
                        )
                        .from_user_shortcut("window-resize"),
                    );
                    let cell_w = self.base.gpu.cell_width();
                    let cell_h = self.base.gpu.cell_height();
                    let scale_factor = self.base.gpu.scale_factor();
                    self.state
                        .resize_all(engine, terminal_rect, cell_w, cell_h, scale_factor);
                }
                self.mark_dirty();
            }
            WindowEvent::Focused(focused) => {
                self.base.state.focused = focused;
                // 포커스가 바뀌면 누름·뗌 짝이 끊길 수 있어 double-tap 추적을 초기화한다.
                self.double_tap.reset();
                if !focused {
                    if self.ime_preedit.is_some() {
                        self.flush_ime_preedit(engine);
                    }
                    self.base.state.modifiers = ModifiersState::empty();
                    self.state.clear_switch_overlay();
                    self.state.modifier_hint.clear();
                }
                self.mark_dirty();
            }
            WindowEvent::Occluded(false) => {
                self.mark_dirty();
            }
            // 오버레이 host 가 별도 최상위 창인 OS 는 주 창 이동을 직접 따라가야 한다.
            WindowEvent::Moved(_) => self.base.gpu.follow_overlay_host(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.base.state.modifiers = modifiers.state();
                let mut dirty = self.update_hovered_link(engine);
                // modifier 변화에 따라 키캡 표시 대상과 다시 그리기를 갱신한다.
                let mods = self.base.state.modifiers;
                let ctrl = mods.control_key();
                let shift = mods.shift_key();
                // `alt`·`option` 토큰 축(키 위치 기준). switch-overlay·modifier-hint 공통 축.
                let (alt, option) = tasty_key_match::token_axes(mods);
                let kb = &engine.settings.keybindings;
                if self
                    .state
                    .update_switch_overlay(engine, kb, ctrl, shift, alt, option)
                {
                    dirty = true;
                }
                // 홀드 조합을 갱신한다. 조합별 표시 지연과 페이드는 draw_modifier_hint가 처리한다.
                if self
                    .state
                    .modifier_hint
                    .update_hold(ctrl, alt, option, shift)
                {
                    dirty = true;
                }
                if dirty {
                    self.mark_dirty();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard_input(engine, &event, egui_consumed);
            }
            WindowEvent::Ime(ime_event) => {
                self.handle_ime(engine, ime_event, egui_consumed);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(engine, position, egui_consumed);
            }
            WindowEvent::CursorLeft { .. } => {
                self.handle_cursor_left();
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                self.handle_mouse_input(
                    engine,
                    button_state,
                    button,
                    egui_consumed,
                    menu_dismiss_swallow,
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(engine, delta, egui_consumed);
            }
            WindowEvent::HoveredFile(path) => {
                self.handle_hovered_file(path);
            }
            WindowEvent::HoveredFileCancelled => {
                self.handle_hovered_file_cancelled();
            }
            WindowEvent::DroppedFile(path) => {
                self.handle_dropped_file(path);
            }
            WindowEvent::RedrawRequested => {
                // App owns prepare → execute geometry/input → render ordering.
                self.mark_dirty();
            }
            _ => {}
        }

        if self.base.state.dirty && !was_dirty {
            self.base.winit.request_redraw();
        }

        ViewAction::None
    }
}

impl View for MainView {
    fn poll_local_reads(&mut self, owner: &mut crate::app::local_reads::ReadRequests) {
        self.poll_local_read_results(owner);
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

    fn handle_event(&mut self, event: WindowEvent, ctx: &mut ViewCtx<'_>) -> ViewAction {
        let Some(engine) = ctx.engine.take() else {
            tracing::warn!("main window event without an engine: {event:?}");
            return ViewAction::None;
        };
        self.handle_engine_event(&engine, event, ctx)
    }

    fn render(&mut self) {
        // 메인 창은 별도 handle_redraw 경로로 그린다.
    }
}

impl sealed::Sealed for MainView {}

impl MainView {
    pub(super) fn record_typing_intent(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
    ) {
        if let Some(target) =
            crate::runtime::surface_binding::SurfaceBinding::capture(engine, surface)
        {
            self.state.dispatch_intent(
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::RecordTyping {
                        target,
                        at: std::time::Instant::now(),
                    },
                )
                .from_user_shortcut("typing"),
            );
        }
    }
}

fn modal_blocks_input(modal: bool, event: &WindowEvent) -> bool {
    modal
        && !matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::RedrawRequested
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::ModifiersChanged(_)
                | WindowEvent::Focused(_)
        )
}
