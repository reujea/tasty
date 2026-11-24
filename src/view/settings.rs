#[cfg(target_os = "linux")]
mod chrome;
pub mod ui;

#[cfg(target_os = "linux")]
pub(crate) use chrome::fit_window_height;

use std::sync::Arc;

use winit::event::WindowEvent;

use crate::adapters::ui::{LayoutContext, ToastManager, ToastScope};
use crate::gpu::GpuState;
use crate::runtime::file_catalog::FormatCatalog as FileFormatRegistry;
use crate::runtime::file_catalog::HandlerCatalog as FileHandlerRegistry;
use crate::settings::Settings;
use crate::settings_ui::{self, PluginShortcutSnapshot, SettingsUiState};
use crate::view::ui::{View, sealed};
use crate::view::{ModalView, ViewAction, ViewBase, ViewCtx};

/// 설정 모달 윈도우. egui 기반 설정 UI를 렌더한다.
pub struct SettingsView {
    pub base: ViewBase,
    pub settings: Settings,
    settings_ui_state: SettingsUiState,
    /// 직전 `run_egui` 뒤의 egui 포커스 기록. 다음 프레임 키 주인(W1)을 정한다.
    key_record: crate::adapters::ui::popup::window_keys::WindowFocusRecord,
    /// `FileHandler` 탭의 Extension Mapping sub-tab 에서 사용. Save 시 user TOML 에 직접 저장.
    file_format: FileFormatRegistry,
    /// 같은 사용자 TOML의 handler 섹션을 함께 내보내 저장 시 보존한다.
    file_handler: FileHandlerRegistry,
    shown: bool,
    double_tap: crate::double_tap::DoubleTapDetector,
    captured_double_tap: Option<String>,
    should_close: bool,
    /// footer Save 로 닫혔는가. plugin override draft 는 이것이 참일 때만 회수된다 —
    /// Cancel · 창 닫기 · 설정 토글 키로 닫으면 그 draft 는 버려진다.
    committed: bool,
    original_restore_content: bool,
    /// 창을 열 때의 설정. 저장할 때 창이 바꾸지 않은 값을 그사이 다른 경로가 바꿨는지 가린다.
    opened_settings: Settings,
    pub(crate) toasts: ToastManager,
    /// This window's clipboard: egui widget copies go through the copy door with it.
    clipboard: crate::clipboard::terminal::TerminalClipboard,
    /// Linux는 OS 장식을 끄고 제목 줄·창 가장자리를 앱이 맡는다.
    #[cfg(target_os = "linux")]
    chrome: chrome::SettingsChrome,
    /// 지금 처리하는 창 사건이 저널 보류 뒤 재생된 것인지(`set_input_replayed`).
    #[cfg(target_os = "linux")]
    input_replayed: bool,
}

impl SettingsView {
    pub(crate) fn take_file_requests(
        &mut self,
    ) -> Vec<crate::app::settings_files::SettingsFileRequest> {
        self.settings_ui_state.take_file_requests()
    }

    pub(crate) fn accept_file_result(
        &mut self,
        result: crate::app::settings_files::SettingsFileResult,
    ) {
        self.settings_ui_state.accept_file_result(result);
        self.mark_dirty();
    }

    pub fn new(
        gpu: GpuState,
        winit: Arc<winit::window::Window>,
        clipboard: crate::clipboard::terminal::TerminalClipboard,
        settings: Settings,
        file_format: FileFormatRegistry,
        file_handler: FileHandlerRegistry,
    ) -> Self {
        settings_ui::install_monitor_probe(&gpu.egui_ctx, &winit);
        Self {
            #[cfg(target_os = "linux")]
            chrome: chrome::SettingsChrome::new(&winit),
            #[cfg(target_os = "linux")]
            input_replayed: false,
            base: ViewBase::new(gpu, winit),
            clipboard,
            original_restore_content: settings.general.restore_surface_content,
            opened_settings: settings.clone(),
            settings,
            settings_ui_state: SettingsUiState::new(),
            key_record: Default::default(),
            file_format,
            file_handler,
            shown: false,
            double_tap: crate::double_tap::DoubleTapDetector::new(),
            captured_double_tap: None,
            should_close: false,
            committed: false,
            toasts: ToastManager::for_settings_window(),
        }
    }

    /// halt 중 제목 줄 닫기 버튼 클릭인지. 참이면 호출자가 모달을 닫는다(초안 폐기).
    #[cfg(target_os = "linux")]
    pub(crate) fn halted_close_click(&mut self, event: &WindowEvent) -> bool {
        // halt 동안 이 창의 Resized는 처리하지 않아 GPU 크기가 낡을 수 있다. 실제 창 크기를 읽는다.
        let width = self.base.winit.inner_size().width;
        self.chrome.halted_close_click(event, width)
    }

    /// 다음에 넘길 창 사건이 저널 보류 뒤 재생된 것인지 알린다. 재생된 누름으로는 OS 이동·리사이즈를 시작하지 않는다.
    #[cfg(target_os = "linux")]
    pub(crate) fn set_input_replayed(&mut self, replayed: bool) {
        self.input_replayed = replayed;
    }

    /// 창을 열 때의 설정.
    pub(crate) fn opened_settings(&self) -> &Settings {
        &self.opened_settings
    }

    /// Plugins 서브탭에서 표시할 plugin command snapshot을 주입한다.
    pub fn set_plugin_shortcuts(&mut self, snapshot: PluginShortcutSnapshot) {
        self.settings_ui_state.plugin_shortcuts = snapshot;
    }

    /// 첫 진입 탭을 `Plugin` 으로 설정 (Plugins 모달의 `Configure` 진입점).
    pub fn focus_plugin_tab(&mut self) {
        self.settings_ui_state.select_plugin_tab();
    }

    /// 첫 진입 탭을 `FileHandler` 로 설정 (file handler picker popup 의
    /// "설정에서 핸들러 등록" 진입점).
    pub fn focus_file_handler_tab(&mut self) {
        self.settings_ui_state.select_file_handler_tab();
    }

    /// 설정 창의 첫 화면을 일반 > 권한으로 맞춘다. 부팅 권한 안내의 [권한 설정 열기]가
    /// 들어오는 지점이다. 위의 두 함수는 L1 탭만 정하지만 여기서는 L2까지 함께 정한다.
    /// L1만 맞추면 권한 화면이 아니라 일반 탭의 기본 화면이 열리는데, 창이 열린 것은
    /// 마찬가지여서 잘못된 것을 알아채기 어렵다.
    pub fn focus_macos_permissions_tab(&mut self) {
        self.settings_ui_state.select_macos_permissions_tab();
    }

    /// debug 전용 — `debug.settings.open` 의 `tab`·`subtab` 을 적용한다. 새로 연 창과
    /// 이미 열린 창이 같은 함수를 쓴다. 바뀐 탭은 다음 프레임에 그린다.
    #[cfg(debug_assertions)]
    pub(crate) fn apply_debug_tabs(
        &mut self,
        tab: Option<&str>,
        subtab: Option<&str>,
    ) -> crate::settings_ui::DebugTabsApplied {
        let applied = self.settings_ui_state.apply_debug_tabs(tab, subtab);
        self.base.state.dirty = true;
        self.base.winit.request_redraw();
        applied
    }

    /// 단축키 가져오기/내보내기가 쓰는 plugin override 원본 · 설치 plugin 을 주입한다.
    pub fn set_plugin_bundle_context(&mut self, ctx: crate::settings_ui::PluginBundleContext) {
        self.settings_ui_state.set_plugin_bundle_context(ctx);
    }

    /// Plugin 이 contribute 한 settings sub-page 스냅샷을 주입한다. 모달 오픈 직전에
    /// host App 이 호출. 빈 vec 으로 호출하면 plugin sub-tab 이 사라진다.
    pub fn set_plugin_settings_pages(&mut self, pages: Vec<tasty_host_plugin::SettingsPageEntry>) {
        self.settings_ui_state.set_settings_pages(pages);
    }

    pub(crate) fn take_execution_edits(
        &mut self,
    ) -> Option<crate::app::settings_edit::SettingsEdits> {
        self.committed.then(|| {
            self.settings_ui_state.take_execution_edits(
                self.original_restore_content && !self.settings.general.restore_surface_content,
            )
        })
    }

    /// Save로 닫았을 때만 plugin 단축키 변경 초안을 반환한다.
    /// 취소·닫기 버튼·설정 단축키로 닫으면 초안을 버린다.
    pub fn take_plugin_shortcut_draft(
        &mut self,
    ) -> std::collections::BTreeMap<
        (String, String),
        Option<crate::plugin::registry_state::ShortcutOverride>,
    > {
        let draft = std::mem::take(&mut self.settings_ui_state.plugin_shortcuts_draft);
        if self.committed {
            draft
        } else {
            std::collections::BTreeMap::new()
        }
    }
}

impl View for SettingsView {
    fn poll_local_reads(&mut self, owner: &mut crate::app::local_reads::ReadRequests) {
        if self.settings_ui_state.poll_local_reads(owner) {
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
        if matches!(event, WindowEvent::RedrawRequested)
            && let Some(action) =
                crate::view::replay_edit_keys(self, Self::handle_replayed_edit_event)
        {
            return action;
        }
        #[cfg(target_os = "linux")]
        if self.chrome.take_event(
            &event,
            &self.base.winit,
            chrome::LiveInput {
                size: self.base.gpu.size(),
                edit_keys_busy: self.base.gpu.edit_keys_busy(),
                replayed: self.input_replayed,
            },
        ) {
            return ViewAction::None;
        }
        #[cfg(target_os = "linux")]
        if is_left_press(&event) {
            self.chrome.note_left_press(self.input_replayed);
        }
        self.handle_judged_event(event, None)
    }

    fn render(&mut self) {
        if !self.base.state.dirty {
            return;
        }
        self.base.begin_frame();

        let raw_input = self.base.gpu.take_egui_input(&self.base.winit);
        let marks = self.base.gpu.take_window_escape_marks();
        // 키 주인과 Esc 표지는 입력을 열기 전에 한 번 정해 다시 그리는 패스에도 같은 값을 넣는다.
        let pass_keys = crate::adapters::ui::popup::escape_step::WindowPassKeys {
            owner: self
                .settings_ui_state
                .window_key_owner(&self.base.gpu.egui_ctx, self.key_record),
            marks,
        };
        let mut settings = self.settings.clone();
        let reduced_motion = settings.accessibility.reduced_motion;
        let ui_state = &mut self.settings_ui_state;
        let captured_dt = &mut self.captured_double_tap;
        let toasts = &mut self.toasts;
        let mut action: Option<bool> = None;

        let file_format = self.file_format.clone();
        let file_handler = self.file_handler.clone();
        #[cfg(target_os = "linux")]
        let (chrome, winit, size) = (
            &mut self.chrome,
            self.base.winit.clone(),
            self.base.gpu.size(),
        );
        // 한 프레임의 패스가 여럿이어도 앞 패스의 닫기 누름을 잃지 않도록 모은다.
        // 이유: 제목 줄을 그리는 곳이 Linux뿐이라 다른 OS에서는 값이 바뀌지 않는다.
        #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
        let mut titlebar_close = false;
        // 팝업이 키 주인이면 제목 줄도 본문처럼 키를 보지 않는다(`draw_settings_panel`의 `with_body_keys`).
        #[cfg(target_os = "linux")]
        let titlebar_keys = pass_keys.owner.is_none();
        let full_output = self.base.gpu.run_egui(raw_input, |ctx| {
            #[cfg(target_os = "linux")]
            {
                titlebar_close |= chrome.draw_titlebar(ctx, &winit, size, titlebar_keys);
            }
            action = settings_ui::draw_settings_panel(
                ctx,
                settings_ui::SettingsPanelCtx {
                    settings: &mut settings,
                    ui_state,
                    captured_double_tap: captured_dt,
                    file_format: &file_format,
                    file_handler: &file_handler,
                    pass_keys: &pass_keys,
                },
            );

            let empty_layout = LayoutContext {
                active_workspace: 0,
                active_workspace_id: 0,
                pane_rects: Vec::new(),
                surface_rects: Vec::new(),
                active_tabs: Vec::new(),
                active_tab_ids: Vec::new(),
                workspace_rect: None,
                surface_owners: Vec::new(),
                tab_content_rects: Vec::new(),
            };
            toasts.draw(ctx, &empty_layout, reduced_motion);
            #[cfg(target_os = "linux")]
            chrome.finish_pass(ctx, &winit, size);
        });
        self.key_record = crate::adapters::ui::popup::window_keys::WindowFocusRecord::read(
            &self.base.gpu.egui_ctx,
        );

        self.settings_ui_state.invalidate_edited_export();
        self.settings = settings;
        if action.is_some() {
            self.should_close = true;
            self.committed = action == Some(true);
        } else if titlebar_close {
            // 제목 줄 닫기는 OS 닫기와 같다. 초안을 버린다.
            self.should_close = true;
        }
        if let Some(msg) = self.settings_ui_state.take_import_export_toast() {
            self.toasts.push(
                msg,
                crate::adapters::ui::ToastKind::Success,
                ToastScope::Window,
            );
            self.mark_dirty();
        }

        self.base.finish_egui_frame(full_output);
        if crate::clipboard::copy::copy_egui_text(
            &mut self.base.gpu,
            &mut self.clipboard,
            &mut self.toasts,
        ) {
            self.mark_dirty();
        }

        self.reveal_after_first_render();

        if self.base.state.dirty {
            self.base.winit.request_redraw();
        }
    }
}

impl ModalView for SettingsView {
    fn shown(&self) -> bool {
        self.shown
    }
    fn set_shown(&mut self, v: bool) {
        self.shown = v;
    }
}

impl sealed::Sealed for SettingsView {}

/// 좌버튼 누름인지.
#[cfg(target_os = "linux")]
fn is_left_press(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::MouseInput {
            state: winit::event::ElementState::Pressed,
            button: winit::event::MouseButton::Left,
            ..
        }
    )
}

impl SettingsView {
    /// 편집 키 보류가 풀려 다시 온 사건. 재생된 좌버튼 누름은 OS 창 이동을 시작하지 않도록 출처를 적는다.
    fn handle_replayed_edit_event(
        &mut self,
        event: WindowEvent,
        judged: Option<crate::adapters::ui::input::edit_key::EditKey>,
    ) -> ViewAction {
        #[cfg(target_os = "linux")]
        if is_left_press(&event) {
            self.chrome.note_left_press(true);
        }
        self.handle_judged_event(event, judged)
    }

    /// 창 이벤트 하나. 편집 키 판정이 가져간 이벤트는 egui와 창 자체 처리 모두 건너뛰고, 보류가 풀리면
    /// `replay_edit_keys`로 다시 온다. `judged`는 그때 원래 키가 판정받은 편집이다.
    fn handle_judged_event(
        &mut self,
        event: WindowEvent,
        judged: Option<crate::adapters::ui::input::edit_key::EditKey>,
    ) -> ViewAction {
        // 녹화 중이면 키보드 이벤트를 egui에 전달하지 않는다.
        // egui가 Cmd+C 등을 시맨틱 커맨드(Copy)로 소비하면 캡처가 안 되기 때문.
        let is_recording = self.settings_ui_state.is_recording();
        let skip_egui = is_recording && matches!(&event, WindowEvent::KeyboardInput { .. });

        // RedrawRequested는 아래에서 렌더링한다. egui 입력에도 넣으면 재그리기가 반복된다.
        let is_redraw = matches!(&event, WindowEvent::RedrawRequested);
        crate::view::judged_window_event(
            self,
            event,
            judged,
            !skip_egui && !is_redraw,
            Self::own_event,
        )
    }

    /// 편집 키 판정과 egui를 지난 이벤트의 창 자체 처리.
    fn own_event(&mut self, event: WindowEvent) -> ViewAction {
        // egui에 넘기는 일은 입력을 쌓기만 하므로 녹화 상태는 판정 앞에서 읽은 값과 같다.
        let is_recording = self.settings_ui_state.is_recording();
        match event {
            WindowEvent::CloseRequested => {
                self.should_close = true;
                return ViewAction::Close;
            }
            WindowEvent::Resized(new_size) => {
                self.base.gpu.resize(new_size);
                self.mark_dirty();
            }
            WindowEvent::RedrawRequested => {
                #[cfg(target_os = "linux")]
                if self.chrome.decorations_changed(&self.base.winit) {
                    self.mark_dirty();
                }
                self.render();
            }
            WindowEvent::CursorMoved { .. } => {
                self.mark_dirty();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.base.state.modifiers = modifiers.state();
            }
            WindowEvent::Focused(focused) => {
                // MainView 와 **별개 인스턴스**의 detector 라 여기서도 따로 지운다.
                // 이유는 `DoubleTapDetector::reset` 주석 참조.
                self.double_tap.reset();
                if focused {
                    // 권한 상태를 바꾸려면 시스템 설정에 다녀와야 하고, 이 창으로
                    // 돌아오는 것이 그 과정의 끝이다. 권한 화면이 낡은 값을 보여주지
                    // 않도록 여기서 다시 측정한다. macOS가 아니면 아무 일도 하지 않는다.
                    crate::macos_permissions::refresh_permission_snapshot();
                    self.mark_dirty();
                }
            }
            WindowEvent::KeyboardInput { ref event, .. } => {
                use winit::event::ElementState;

                // 모달은 자체 키 이벤트를 받으므로 설정 열기 단축키로 여기서 닫는다.
                // 단축키 녹화 중에는 키를 소비하지 않는다.
                // Escape는 텍스트 편집 취소와 충돌하므로 별도 닫기 키로 처리하지 않는다.
                // 메인 창의 Escape 처리는 아직 열지 않은 설정 요청의 취소만 담당한다.
                if !is_recording
                    && event.state == ElementState::Pressed
                    && crate::adapters::ui::input::shortcuts::matches_any_binding(
                        &self.settings.keybindings.toggle_settings,
                        &event.logical_key,
                        self.base.state.modifiers,
                    )
                {
                    self.should_close = true;
                    return ViewAction::Close;
                }

                self.double_tap
                    .on_key_event(&event.logical_key, event.state == ElementState::Pressed);
                if event.state == ElementState::Pressed
                    && let Some(dt) = self.double_tap.take()
                {
                    // 메인 창에서 설정을 여는 더블탭은 설정 창 안에서 같은 입력으로 닫는다.
                    if !is_recording
                        && self
                            .settings
                            .keybindings
                            .toggle_settings
                            .iter()
                            .any(|b| b == dt.binding_str())
                    {
                        self.should_close = true;
                        return ViewAction::Close;
                    }
                    self.captured_double_tap = Some(dt.binding_str().to_string());
                    self.mark_dirty();
                }

                // 녹화 중이면 winit에서 직접 키 조합 캡처. quick-switch bare-key 슬롯은
                // modifier 금지 규칙(capture_bare_key), 그 외 일반 콤보 슬롯은 modifier
                // 필수 규칙(capture_winit_key_combo)으로 분기.
                if is_recording {
                    let combo = if self.settings_ui_state.recording_is_bare_key() {
                        crate::settings_ui::capture_bare_key(event, self.base.state.modifiers)
                    } else {
                        crate::settings_ui::capture_winit_key_combo(
                            event,
                            self.base.state.modifiers,
                        )
                    };
                    if !matches!(combo, crate::settings_ui::KeyCapture::None) {
                        self.settings_ui_state.captured_winit_combo = Some(combo);
                        self.mark_dirty();
                    }
                }
            }
            _ => {}
        }

        if self.should_close {
            ViewAction::Close
        } else {
            ViewAction::None
        }
    }
}
