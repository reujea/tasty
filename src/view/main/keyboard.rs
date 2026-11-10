use crate::runtime::engine_read::EngineRead;
use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::MainView;
use crate::app::command::{DomainIntent, SendPayload};
use crate::state::FocusedSurfaceType;
use crate::view::ui::View;

/// `decide_key_to_terminal` 의 입력 — 현재 focused terminal 의 read-only 상태.
/// UI 가 sequence 결정에 필요한 정보만 추출. terminal mut borrow 불필요.
struct KeyboardReadState {
    shift_enter_newline: bool,
    app_cursor: bool,
    is_alt_screen: bool,
    scroll_offset: usize,
    rows: usize,
    /// macOS "Option as Meta" 설정 값. 호출부가 cfg 분기로 채운다(비-macOS=false).
    /// 켜져 있으면 Option(Alt)+문자가 `ESC` + base 문자 Meta 시퀀스로 인코딩된다.
    option_as_meta: bool,
    /// 입력을 읽는 PTY가 Windows 콘솔(ConPTY)이다. mirror는 클라이언트가 아니라 서버 OS를 따른다.
    win32_console: bool,
}

/// 키 처리 뒤 호출자가 적용할 터미널 스크롤 동작. PTY 전송과 별도로 처리한다.
enum KeyboardScrollAction {
    None,
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollToBottom,
}

/// `decide_key_to_terminal` 의 반환. PTY input 은 `payloads` 로 Intent 큐잉,
/// 터미널 자체 mutate 는 `scroll_action` 으로 호출자가 처리.
struct KeyboardSendOutcome {
    payloads: Vec<SendPayload>,
    scroll_action: KeyboardScrollAction,
    dirty: bool,
    sent: bool,
}

impl MainView {
    /// Resolve tutorial ownership before feeding egui, so Escape cannot also reach the PTY.
    pub(super) fn route_tutorial_input(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent;
        if let WindowEvent::KeyboardInput { event, .. } = event
            && event.state == winit::event::ElementState::Pressed
            && event.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
            && self.state.tutorial.active.is_some()
            && !self.state.popups.has_focused()
            && !self.state.fullscreen_stage_active()
            && !self.state.has_input_dialog_open()
            && !self.state.plugin_popup_open
            && !self.state.settings_open_requested
        {
            crate::adapters::ui::tutorial::interrupt_and_reopen(&mut self.state);
            self.mark_dirty();
            return true;
        }
        if let WindowEvent::MouseInput {
            state: winit::event::ElementState::Pressed,
            ..
        } = event
        {
            // Let egui report whether this click belonged to the callout in its next pass.
            self.state.tutorial.keyboard_focus = false;
        }
        false
    }

    pub(super) fn handle_keyboard_input(
        &mut self,
        engine: &EngineRead<'_>,
        event: &winit::event::KeyEvent,
        _egui_consumed: bool,
    ) {
        // Feed all key events (Press + Release) to the double-tap detector
        self.double_tap
            .on_key_event(&event.logical_key, event.state == ElementState::Pressed);

        if event.state != ElementState::Pressed {
            return;
        }

        tracing::trace!(target: "tasty::explorer::perf", "key_received");
        // 전체화면 무대가 열려 있으면 배경으로 키를 보내지 않는다.
        if self.try_consume_fullscreen_stage_key(engine, event) {
            return;
        }

        if self.try_consume_double_tap_key(engine) {
            return;
        }

        if self.try_consume_escape_key(event) {
            return;
        }

        // Modals, dialogs, focused host popups, plugin egui-mesh popups block
        // keyboard input to the terminal. 판정은 egui feed 게이트(`view::main`)와
        // 같은 `MainViewState::keyboard_overlay_open` 을 쓴다.
        let overlay_open = self.state.keyboard_overlay_open();

        if !overlay_open && self.try_consume_shortcut_key(engine, event) {
            return;
        }
        if overlay_open && self.try_consume_address_refresh_key(engine, event) {
            return;
        }

        // vi-style 키보드 복사 모드가 활성이면 키를 가로채 PTY 로 보내지 않는다.
        if self.vi_copy.is_some() && self.try_consume_vi_key(engine, event) {
            return;
        }

        if overlay_open {
            return;
        }

        let surface_type = self.state.focused_surface_type(engine);
        let typing_surface_id = self.state.focused_surface_id(engine);

        match surface_type {
            FocusedSurfaceType::Terminal => self.forward_key_to_terminal(engine, event),
            _ => {
                // markdown/image 등 egui-mesh surface 면 plugin 으로 Key/Text forward.
                // html/empty/None 등 비-mesh surface 는 여전히 no-op.
                if let Some(sid) = self.focused_egui_mesh_surface_id(engine) {
                    self.forward_key_to_egui_mesh(sid, event);
                } else if let Some(sid) = self.focused_attach_mesh_surface_id(engine) {
                    // attach mesh mirror surface — 위와 동형이되 목적지가 원격.
                    self.forward_key_to_attach_mesh(sid, event);
                }
            }
        }

        if let Some(sid) = typing_surface_id {
            self.record_typing_intent(engine, sid);
        }
    }

    /// 단축키를 찾을 키. 규칙은 [`crate::shortcuts::shortcut_lookup_key`] 가 정한다.
    fn shortcut_lookup_key(&self, event: &winit::event::KeyEvent) -> Key {
        crate::shortcuts::shortcut_lookup_key(
            &event.logical_key,
            &event.physical_key,
            self.base.state.modifiers,
        )
    }

    /// 전체화면 무대가 열려 있으면 배경 단축키와 Escape 처리를 막는다.
    /// 무대 콘텐츠로의 키·IME 전달은 handle_event의 egui 입력 경로에서 처리한다.
    fn try_consume_fullscreen_stage_key(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        event: &winit::event::KeyEvent,
    ) -> bool {
        // 사용자 지정 종료 키도 일반 단축키와 같은 규칙으로 찾는다.
        let key = self.shortcut_lookup_key(event);
        let decision = stage_key_decision(
            self.state.fullscreen_stage_active(),
            &engine.settings.keybindings.fullscreen_stage_exit,
            &key,
            self.base.state.modifiers,
        );
        match decision {
            StageKeyDecision::PassThrough => false,
            StageKeyDecision::ExitStage => {
                self.discard_pending_double_tap();
                self.state.close_fullscreen_stage();
                self.mark_dirty();
                true
            }
            StageKeyDecision::ConsumeForStage => {
                self.discard_pending_double_tap();
                true
            }
        }
    }

    /// 무대 중에는 double-tap 결과만 버린다.
    /// 검출기에는 press/release를 계속 전달해 실제 키 상태를 유지한다.
    fn discard_pending_double_tap(&mut self) {
        let _discarded = self.double_tap.take();
    }

    /// double-tap 수식키 단축키를 처리했으면 true를 반환한다.
    fn try_consume_double_tap_key(&mut self, engine: &EngineRead<'_>) -> bool {
        if let Some(dt) = self.double_tap.take()
            && self.handle_double_tap_shortcut(engine, dt)
        {
            self.reset_modifier_hint_reveal_timer();
            self.mark_dirty();
            return true;
        }
        false
    }

    /// 키보드 단축키를 사용하면 수식키 도움말의 표시 지연을 다시 시작한다.
    /// 팔레트 실행에는 적용하지 않으며 수식키를 누르고 있지 않으면 아무것도 하지 않는다.
    fn reset_modifier_hint_reveal_timer(&mut self) {
        let theme = crate::theme::theme();
        self.state
            .modifier_hint
            .reset_reveal_timer_if_not_shown(&theme);
    }

    /// Escape로 설정 요청·알림을 처리하고 팝업 포커스를 해제한다.
    /// 바깥 클릭으로 닫을 수 있는 팝업만 닫는다.
    fn try_consume_escape_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if event.logical_key == Key::Named(NamedKey::Escape) {
            if self.state.settings_open_requested {
                // 아직 처리하지 않은 설정 창 열기 요청을 취소한다.
                self.state.settings_open_requested = false;
                self.mark_dirty();
                return true;
            }
            if self.state.popups.is_open("notifications") {
                self.state.dispatch_intent(
                    crate::intent::UiIntent::ClosePopup {
                        id: "notifications",
                    }
                    .from_user_shortcut("escape_close_notifications"),
                );
                self.mark_dirty();
                return true;
            }
            // 설정 요청과 알림을 먼저 처리한 뒤, 포커스된 팝업 하나의 포커스를 해제한다.
            // 바깥 클릭과 같이 close_on_outside_click인 팝업만 닫는다.
            // 순서 검증: crates/tasty-doc-guards/tests/escape_dismisses_the_focused_popup.rs.
            if let Some((id, closes)) = self.state.popups.focused_dismissal_target() {
                self.state.popups.set_focused(id, false);
                if closes {
                    self.state.dispatch_intent(
                        crate::intent::UiIntent::ClosePopup { id }
                            .from_user_shortcut("escape_dismiss_focused_popup"),
                    );
                }
                self.mark_dirty();
                return true;
            }
        }
        false
    }

    /// 오버레이가 없을 때 단축키를 처리하고 IME 조합 문자를 정리한다.
    fn try_consume_shortcut_key(
        &mut self,
        engine: &EngineRead<'_>,
        event: &winit::event::KeyEvent,
    ) -> bool {
        let shortcut_key = self.shortcut_lookup_key(event);
        if self.handle_shortcut(engine, &shortcut_key, self.base.state.modifiers) {
            self.after_shortcut_consumed(engine);
            return true;
        }
        false
    }

    /// html 주소 띠만 키보드 포커스를 가졌으면 공통 새로고침 키를 그 띠의 surface 로 보낸다.
    /// 다른 입력 게이트가 열렸으면 처리하지 않는다. 띠의 입력 문자·커서·IME 는 egui 가 그대로 받는다.
    fn try_consume_address_refresh_key(
        &mut self,
        engine: &EngineRead<'_>,
        event: &winit::event::KeyEvent,
    ) -> bool {
        let Some(sid) = self.state.html_address.focused_surface() else {
            return false;
        };
        if self.state.keyboard_overlay_open_besides_html_address()
            || self.state.fullscreen_stage_active()
        {
            return false;
        }
        // 같은 키가 편집이나 주소창 포커스에도 묶였으면 그쪽이 이기고 새로고침하지 않는다.
        let key = self.shortcut_lookup_key(event);
        if super::html_address_keys::address_key_action(
            &engine.settings.keybindings,
            &key,
            self.base.state.modifiers,
        ) != Some(super::html_address_keys::AddressKeyAction::Refresh)
        {
            return false;
        }
        if !crate::adapters::ui::input::shortcuts::request_surface_refresh(
            &mut self.state,
            engine,
            sid,
            crate::intent::RefreshTrigger::Shortcut,
        ) {
            return false;
        }
        tracing::debug!(
            target: "tasty::refresh",
            "html address band key requested a refresh of surface {sid}"
        );
        self.mark_dirty();
        true
    }

    /// winit과 native webview가 단축키 처리 뒤 함께 사용하는 후처리.
    pub(crate) fn after_shortcut_consumed(&mut self, engine: &EngineRead<'_>) {
        self.reset_modifier_hint_reveal_timer();
        if self.ime_preedit.is_some() {
            // 팝업을 여는 단축키는 조합 문자를 버리고, 그 외에는 PTY로 확정 전송한다.
            // plugin 팝업은 아직 캐시에 없을 수 있으므로 열기 요청 큐도 확인한다.
            if self.state.popups.has_focused()
                || self.state.plugin_popup_open
                || !self.state.pending_popup_opens.is_empty()
            {
                self.clear_ime_preedit(engine);
            } else {
                self.flush_ime_preedit(engine);
            }
        }
        self.try_enter_vi_copy_mode(engine);
        self.mark_dirty();
    }

    /// vi 복사 모드의 키를 처리한다. physical key 대체는 Ctrl만 누른 경우에 적용한다.
    fn try_consume_vi_key(
        &mut self,
        engine: &EngineRead<'_>,
        event: &winit::event::KeyEvent,
    ) -> bool {
        let vi_key = if self.base.state.modifiers.control_key() {
            crate::shortcuts::physical_key_to_logical(&event.physical_key)
                .unwrap_or_else(|| event.logical_key.clone())
        } else {
            event.logical_key.clone()
        };
        if self.try_handle_vi_key(engine, &vi_key, self.base.state.modifiers) {
            self.mark_dirty();
            return true;
        }
        false
    }

    /// 포커스된 터미널로 키를 보낸다. IME 조합 문자는 Commit에서 보낸다.
    fn forward_key_to_terminal(&mut self, engine: &EngineRead<'_>, event: &winit::event::KeyEvent) {
        // Commit이 따로 오지 않는 ASCII 문자와 구두점은 여기서 전달한다.
        let text_for_terminal = if self.ime_active {
            match &event.text {
                Some(t) if t.as_str().is_ascii() => &event.text,
                _ => &None,
            }
        } else {
            &event.text
        };
        // IME 조합 중에도 Ctrl+문자 입력을 위해 물리 키를 사용한다.
        let terminal_key = self.shortcut_lookup_key(event);

        // Option as Meta 설정은 macOS에만 있으므로 다른 플랫폼은 false를 쓴다.
        #[cfg(target_os = "macos")]
        let option_as_meta = engine.settings.general.option_as_meta;
        #[cfg(not(target_os = "macos"))]
        let option_as_meta = false;

        let shift_enter_newline =
            shift_enter_newline_for(engine, self.state.focused_surface_id(engine));

        let surface_id = self.state.focused_surface_id(engine);
        let read_state = surface_id.and_then(|sid| {
            engine.as_ref().visible_terminal(sid).map(|t| {
                let win32_console = t.win32_console_input();
                t.with_view(&self.state.terminal_views.get(engine, sid), |view| {
                    KeyboardReadState {
                        shift_enter_newline,
                        app_cursor: view.application_cursor_keys(),
                        is_alt_screen: view.is_alternate_screen(),
                        scroll_offset: view.scroll_offset(),
                        rows: view.rows(),
                        option_as_meta,
                        win32_console,
                    }
                })
            })
        });

        if let (Some(rs), Some(sid)) = (read_state, surface_id) {
            let outcome = Self::decide_key_to_terminal(
                rs,
                &terminal_key,
                text_for_terminal,
                self.base.state.modifiers,
            );

            for payload in outcome.payloads {
                self.state.dispatch_intent(
                    DomainIntent::SendToSurface {
                        surface_id: sid,
                        payload,
                    }
                    .from_user_shortcut("keyboard_input"),
                );
            }

            self.apply_keyboard_scroll_action(engine, outcome.scroll_action);

            if outcome.dirty {
                self.base.state.dirty = true;
            }

            if outcome.sent {
                self.ime_cursor_advance = 0;
                if self.text_selection.is_some() {
                    self.text_selection = None;
                    self.base.state.dirty = true;
                }
            }
        }
    }

    /// 포커스된 egui-mesh surface로 Key와 허용된 Text 이벤트를 보낸다.
    /// should_forward_text가 수식키·제어문자·IME 조합 문자를 걸러낸다.
    fn forward_key_to_egui_mesh(&mut self, surface_id: u32, event: &winit::event::KeyEvent) {
        self.egui_mesh_push_key(surface_id, event);

        if let Some(text) = &event.text {
            let is_cmd =
                self.base.state.modifiers.control_key() || self.base.state.modifiers.super_key();
            if should_forward_text(text.as_str(), is_cmd, self.ime_active) {
                self.egui_mesh_push_text(surface_id, text.as_str());
            }
        }
        self.mark_dirty();
    }

    /// [`Self::forward_key_to_egui_mesh`]의 attach mesh mirror 대응 — 목적지가
    /// 원격 plugin 이라는 점만 다르다.
    fn forward_key_to_attach_mesh(&mut self, surface_id: u32, event: &winit::event::KeyEvent) {
        self.attach_mesh_push_key(surface_id, event);

        if let Some(text) = &event.text {
            let is_cmd =
                self.base.state.modifiers.control_key() || self.base.state.modifiers.super_key();
            if should_forward_text(text.as_str(), is_cmd, self.ime_active) {
                self.attach_mesh_push_text(surface_id, text.as_str());
            }
        }
        self.mark_dirty();
    }

    /// 터미널의 읽기 차용이 끝난 뒤 스크롤 상태를 변경한다.
    fn apply_keyboard_scroll_action(
        &mut self,
        engine: &EngineRead<'_>,
        action: KeyboardScrollAction,
    ) {
        let Some(sid) = self.state.focused_surface_id(engine) else {
            return;
        };
        self.state
            .terminal_views
            .update(&engine.as_ref(), sid, |viewport, cut| match action {
                KeyboardScrollAction::None => {}
                KeyboardScrollAction::ScrollUp(n) => viewport.scroll_up(cut, n),
                KeyboardScrollAction::ScrollDown(n) => viewport.scroll_down(cut, n),
                KeyboardScrollAction::ScrollToBottom => viewport.scroll_to_bottom(),
            });
    }

    /// 키를 PTY 전송 데이터와 스크롤 동작으로 변환한다. 실제 반영은 호출자가 한다.
    fn decide_key_to_terminal(
        state: KeyboardReadState,
        key: &Key,
        text: &Option<winit::keyboard::SmolStr>,
        modifiers: ModifiersState,
    ) -> KeyboardSendOutcome {
        let mut payloads: Vec<SendPayload> = Vec::new();
        let mut scroll_action = KeyboardScrollAction::None;
        let mut dirty = false;
        let mut sent = false;

        let is_scrollback_key = !state.is_alt_screen
            && matches!(
                key.as_ref(),
                Key::Named(NamedKey::PageUp) | Key::Named(NamedKey::PageDown)
            );

        let push_bytes = |payloads: &mut Vec<SendPayload>, bytes: &[u8]| {
            payloads.push(SendPayload::Bytes(bytes.to_vec()));
        };

        match key.as_ref() {
            Key::Named(NamedKey::Enter) => {
                if modifiers == ModifiersState::SHIFT && state.shift_enter_newline {
                    push_bytes(&mut payloads, b"\n");
                } else if modifiers.shift_key() {
                    if state.win32_console {
                        push_bytes(&mut payloads, &win32_key_press(13, 13, modifiers));
                    } else {
                        // Unix PTYs pass CSI-u through to the application.
                        // Kitty keyboard protocol: CSI 13 ; 2 u (Shift+Enter)
                        push_bytes(&mut payloads, b"\x1b[13;2u");
                    }
                } else {
                    push_bytes(&mut payloads, b"\r");
                }
                sent = true;
            }
            Key::Named(NamedKey::Backspace) => {
                push_bytes(&mut payloads, b"\x7f");
                sent = true;
            }
            Key::Named(NamedKey::Tab) => {
                if modifiers.shift_key() {
                    push_bytes(&mut payloads, b"\x1b[Z");
                } else {
                    push_bytes(&mut payloads, b"\t");
                }
                sent = true;
            }
            Key::Named(NamedKey::Escape) => {
                push_bytes(&mut payloads, b"\x1b");
                sent = true;
            }
            Key::Named(
                arrow @ (NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowRight
                | NamedKey::ArrowLeft),
            ) => {
                let suffix = match arrow {
                    NamedKey::ArrowUp => 'A',
                    NamedKey::ArrowDown => 'B',
                    NamedKey::ArrowRight => 'C',
                    NamedKey::ArrowLeft => 'D',
                    _ => unreachable!(),
                };
                // xterm cursor modifiers: 1 + Shift + 2*Alt + 4*Control.
                // Use physical Alt (Option on macOS), not the host binding's
                // `alt` token (Command on macOS). Super keeps its old behavior.
                // Option-as-Meta only affects characters, not cursor keys.
                let parameter = 1
                    + u8::from(modifiers.shift_key())
                    + 2 * u8::from(modifiers.alt_key())
                    + 4 * u8::from(modifiers.control_key());
                let sequence = if parameter == 1 {
                    let prefix = if state.app_cursor { 'O' } else { '[' };
                    format!("\x1b{prefix}{suffix}")
                } else {
                    // Modified arrows use CSI even in application cursor mode.
                    format!("\x1b[1;{parameter}{suffix}")
                };
                push_bytes(&mut payloads, sequence.as_bytes());
                sent = true;
            }
            Key::Named(NamedKey::Home) => {
                push_bytes(&mut payloads, b"\x1b[H");
                sent = true;
            }
            Key::Named(NamedKey::End) => {
                push_bytes(&mut payloads, b"\x1b[F");
                sent = true;
            }
            Key::Named(NamedKey::PageUp) => {
                if state.is_alt_screen {
                    push_bytes(&mut payloads, b"\x1b[5~");
                    sent = true;
                } else {
                    scroll_action = KeyboardScrollAction::ScrollUp(state.rows);
                    dirty = true;
                }
            }
            Key::Named(NamedKey::PageDown) => {
                if state.is_alt_screen {
                    push_bytes(&mut payloads, b"\x1b[6~");
                    sent = true;
                } else {
                    scroll_action = KeyboardScrollAction::ScrollDown(state.rows);
                    dirty = true;
                }
            }
            Key::Named(NamedKey::Insert) => {
                push_bytes(&mut payloads, b"\x1b[2~");
                sent = true;
            }
            Key::Named(NamedKey::Delete) => {
                push_bytes(&mut payloads, b"\x1b[3~");
                sent = true;
            }
            Key::Named(NamedKey::F1) => {
                push_bytes(&mut payloads, b"\x1bOP");
                sent = true;
            }
            Key::Named(NamedKey::F2) => {
                push_bytes(&mut payloads, b"\x1bOQ");
                sent = true;
            }
            Key::Named(NamedKey::F3) => {
                push_bytes(&mut payloads, b"\x1bOR");
                sent = true;
            }
            Key::Named(NamedKey::F4) => {
                push_bytes(&mut payloads, b"\x1bOS");
                sent = true;
            }
            Key::Named(NamedKey::F5) => {
                push_bytes(&mut payloads, b"\x1b[15~");
                sent = true;
            }
            Key::Named(NamedKey::F6) => {
                push_bytes(&mut payloads, b"\x1b[17~");
                sent = true;
            }
            Key::Named(NamedKey::F7) => {
                push_bytes(&mut payloads, b"\x1b[18~");
                sent = true;
            }
            Key::Named(NamedKey::F8) => {
                push_bytes(&mut payloads, b"\x1b[19~");
                sent = true;
            }
            Key::Named(NamedKey::F9) => {
                push_bytes(&mut payloads, b"\x1b[20~");
                sent = true;
            }
            Key::Named(NamedKey::F10) => {
                push_bytes(&mut payloads, b"\x1b[21~");
                sent = true;
            }
            Key::Named(NamedKey::F11) => {
                push_bytes(&mut payloads, b"\x1b[23~");
                sent = true;
            }
            Key::Named(NamedKey::F12) => {
                push_bytes(&mut payloads, b"\x1b[24~");
                sent = true;
            }
            _ => {
                // Ctrl+letter → send control character (0x01-0x1A)
                if modifiers.control_key()
                    && !modifiers.alt_key()
                    && let Key::Character(c) = key
                    && let Some(ch) = c.chars().next()
                    && ch.is_ascii_alphabetic()
                {
                    let ctrl_char = (ch.to_ascii_lowercase() as u8) - b'a' + 1;
                    if state.win32_console && ctrl_char == b'\n' {
                        // ConPTY decodes bare LF as Ctrl+Enter (VK_RETURN), not
                        // Ctrl+J. Native console readers need the original VK_J.
                        push_bytes(&mut payloads, &win32_key_press(b'J', ctrl_char, modifiers));
                    } else {
                        push_bytes(&mut payloads, &[ctrl_char]);
                    }
                    sent = true;
                    return KeyboardSendOutcome {
                        payloads,
                        scroll_action,
                        dirty,
                        sent,
                    };
                }
                // Option as Meta는 조합된 문자 대신 물리 키의 US 문자를 사용한다.
                if state.option_as_meta
                    && modifiers.alt_key()
                    && !modifiers.control_key()
                    && !modifiers.super_key()
                    && let Key::Character(c) = key
                    && let Some(ch) = c.chars().next()
                {
                    let mut buf = vec![0x1b_u8];
                    let mut utf8 = [0u8; 4];
                    buf.extend_from_slice(ch.encode_utf8(&mut utf8).as_bytes());
                    push_bytes(&mut payloads, &buf);
                    sent = true;
                    // text 분기로 내려가 특수문자가 중복 전송되지 않도록 early return.
                    return KeyboardSendOutcome {
                        payloads,
                        scroll_action,
                        dirty,
                        sent,
                    };
                }
                if let Some(text) = text {
                    let s = text.as_str();
                    if !s.is_empty() {
                        payloads.push(SendPayload::Text(s.to_string()));
                        sent = true;
                    }
                }
            }
        }
        // Scroll to bottom only when actual content was sent to the terminal,
        // not on modifier-only keypresses (Ctrl, Cmd, Shift, Alt).
        // PageUp/PageDown (scrollback) 의 scroll_action 을 덮어쓰지 않도록 None 일 때만.
        if sent
            && !is_scrollback_key
            && state.scroll_offset > 0
            && matches!(scroll_action, KeyboardScrollAction::None)
        {
            scroll_action = KeyboardScrollAction::ScrollToBottom;
            dirty = true;
        }

        KeyboardSendOutcome {
            payloads,
            scroll_action,
            dirty,
            sent,
        }
    }

    pub(super) fn handle_ime(
        &mut self,
        engine: &EngineRead<'_>,
        ime_event: winit::event::Ime,
        egui_consumed: bool,
    ) {
        super::ime::handle_event(self, engine, ime_event, egui_consumed);
    }
}

/// 앱별 Shift+Enter 규칙. 이름은 로컬 폴링 값이고 mirror면 서버가 보낸 원격 전경 이름이다.
/// 규칙은 인코딩하는 이 클라이언트의 설정으로 판단한다.
fn shift_enter_newline_for(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: Option<u32>,
) -> bool {
    engine
        .settings
        .terminal_input
        .shift_enter_newline(surface_id.and_then(|sid| engine.foreground_name(sid)))
}

/// egui-mesh surface 로 `Text` wire 이벤트를 forward 할지 판정(egui-winit 미러 +
/// IME 억제). 억제 조건: 빈 문자열 / 제어·사설영역 문자(Delete 의 `\u{f728}` 등) /
/// command modifier 동반(Ctrl+key 는 문자 삽입 아님) / IME 조합 중 non-ASCII(조합 결과는
/// IME `Commit` 으로 도착하므로 중복 방지 — ASCII 숫자·기호는 조합을 안 거쳐 통과).
fn should_forward_text(text: &str, is_cmd: bool, ime_active: bool) -> bool {
    if text.is_empty() || is_cmd {
        return false;
    }
    // IME 조합 중 non-ASCII 는 Commit 으로 도착하므로 여기선 억제.
    if ime_active && !text.is_ascii() {
        return false;
    }
    text.chars().all(is_printable_char)
}

// 키 처리 결과는 GPU 없이 확인한다. 실제 이벤트 전달은 별도 검증 대상이다.
#[derive(Debug, PartialEq, Eq)]
enum StageKeyDecision {
    /// 무대 없음 — 기존 파이프라인(1단계~)으로 그대로 흘린다.
    PassThrough,
    /// 무대 있음 + 종료 키 — 무대를 닫고 소비. 뒤로 전파하지 않는다.
    ExitStage,
    /// 무대 있음 + 그 외 키 — 무대 콘텐츠(egui)가 이미 받았다. 뒤로 전파하지 않는다.
    ConsumeForStage,
}

fn stage_key_decision(
    stage_active: bool,
    exit_bindings: &[String],
    key: &Key,
    mods: ModifiersState,
) -> StageKeyDecision {
    if !stage_active {
        return StageKeyDecision::PassThrough;
    }
    if stage_exit_key_matches(exit_bindings, key, mods) {
        StageKeyDecision::ExitStage
    } else {
        StageKeyDecision::ConsumeForStage
    }
}

// 무대가 없으면 무대 종료 키를 소비하지 않는다. 종료 키가 비어 있으면 버튼으로 닫는다.
fn stage_exit_key_matches(exit_bindings: &[String], key: &Key, mods: ModifiersState) -> bool {
    crate::shortcuts::matches_any_binding(exit_bindings, key, mods)
}

/// egui-winit `is_printable_char` 미러 — ASCII 제어문자와 유니코드 사설 사용 영역
/// (일부 플랫폼이 Delete/기능키를 이 영역 문자로 보냄)을 비인쇄로 걸러낸다.
fn is_printable_char(chr: char) -> bool {
    let is_in_private_use_area = ('\u{e000}'..='\u{f8ff}').contains(&chr)
        || ('\u{f0000}'..='\u{ffffd}').contains(&chr)
        || ('\u{100000}'..='\u{10fffd}').contains(&chr);
    !is_in_private_use_area && !chr.is_ascii_control()
}

/// ConPTY's win32-input sequence: VK;scan;Unicode;down;control-state;repeat.
/// CSI-u is swallowed by ConPTY, whereas this encoding preserves modified
/// Enter and Ctrl+J for native ReadConsoleInput consumers. Emit a balanced pair
/// because the terminal forwarding path only receives pressed winit events.
/// The target PTY decides this, not the build OS: a mirror forwards to its server.
fn win32_key_press(virtual_key: u8, character: u8, modifiers: ModifiersState) -> Vec<u8> {
    // Win32 KEY_EVENT_RECORD flags: LEFT_ALT_PRESSED, LEFT_CTRL_PRESSED,
    // SHIFT_PRESSED. Winit's aggregate modifiers do not retain left/right.
    let control_state = (u8::from(modifiers.alt_key()) * 2)
        | (u8::from(modifiers.control_key()) * 8)
        | (u8::from(modifiers.shift_key()) * 16);
    format!(
        "\x1b[{virtual_key};0;{character};1;{control_state};1_\x1b[{virtual_key};0;{character};0;{control_state};1_"
    )
    .into_bytes()
}

#[cfg(test)]
mod shift_enter_rule_tests {
    use super::shift_enter_newline_for;

    /// mirror는 서버가 보낸 원격 이름으로 규칙을 적용하고, 이름이 사라지면 기본 인코딩으로 돌아간다.
    #[test]
    fn a_mirror_follows_the_rule_of_the_pushed_remote_name() {
        let (_, mut session) = crate::state::tests::test_mirror_state();
        let mut e = session.borrow_mut();
        let sid = e
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        e.runtime.settings.terminal_input.rules =
            vec![tasty_settings::terminal_input::TerminalInputRule {
                app: "claude".into(),
                shift_enter_newline: true,
            }];
        assert!(!shift_enter_newline_for(&e.read(), Some(sid)), "첫 값 전");
        e.set_mirror_surface_foreground(sid, Some("claude.exe".into()));
        assert!(shift_enter_newline_for(&e.read(), Some(sid)));
        e.set_mirror_surface_foreground(sid, Some("bash".into()));
        assert!(!shift_enter_newline_for(&e.read(), Some(sid)));
        e.set_mirror_surface_foreground(sid, None);
        assert!(!shift_enter_newline_for(&e.read(), Some(sid)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esc() -> Key {
        Key::Named(NamedKey::Escape)
    }

    fn no_mods() -> ModifiersState {
        ModifiersState::empty()
    }

    // macOS의 Command와 다른 플랫폼의 Alt를 구분한다.
    #[cfg(target_os = "macos")]
    const BINDING_ALT: ModifiersState = ModifiersState::SUPER;
    #[cfg(not(target_os = "macos"))]
    const BINDING_ALT: ModifiersState = ModifiersState::ALT;

    // 기본 설정의 실제 종료 키와 처리 결과를 대조한다.
    fn default_exit_bindings() -> Vec<String> {
        crate::settings::KeybindingSettings::default().fullscreen_stage_exit
    }

    // 배경 단축키 차단 순서는 fullscreen_stage_input_gate 검사도 확인한다.
    #[test]
    fn stage_gate_blocks_escape_from_reaching_popup_close() {
        assert_eq!(
            stage_key_decision(true, &default_exit_bindings(), &esc(), no_mods()),
            StageKeyDecision::ExitStage
        );
    }

    /// 무대가 없으면 0단계는 no-op — ESC 가 기존 4단계 경로로 그대로 내려간다.
    #[test]
    fn stage_gate_is_noop_when_no_stage() {
        assert_eq!(
            stage_key_decision(false, &default_exit_bindings(), &esc(), no_mods()),
            StageKeyDecision::PassThrough
        );
        assert_eq!(
            stage_key_decision(
                false,
                &default_exit_bindings(),
                &Key::Character("a".into()),
                no_mods()
            ),
            StageKeyDecision::PassThrough
        );
    }

    /// 종료 키가 아닌 키는 무대가 소비한다 — 단축키(6단계)·vi(7단계)·터미널
    /// forward(9단계) 어디로도 내려가지 않는다.
    #[test]
    fn stage_swallows_every_other_key() {
        for key in [
            Key::Character("a".into()),
            Key::Named(NamedKey::Enter),
            Key::Named(NamedKey::Tab),
            Key::Named(NamedKey::ArrowDown),
        ] {
            assert_eq!(
                stage_key_decision(true, &default_exit_bindings(), &key, no_mods()),
                StageKeyDecision::ConsumeForStage,
                "무대가 열려 있는데 배경으로 {key:?} 입력이 전달됐다"
            );
        }
    }

    /// 종료 키 판정은 한 곳(`stage_exit_key_matches`)에만 있고, 값은
    /// `KeybindingSettings` 에서 온다 — 기본 프리셋이 ESC 다.
    #[test]
    fn stage_exit_key_is_escape_by_default() {
        let kb = default_exit_bindings();
        assert!(stage_exit_key_matches(&kb, &esc(), no_mods()));
        assert!(!stage_exit_key_matches(
            &kb,
            &Key::Named(NamedKey::Enter),
            no_mods()
        ));
    }

    // 설정에서 읽은 종료 키를 사용한다.
    #[test]
    fn stage_exit_follows_the_configured_binding() {
        let rebound = vec!["ctrl+alt+q".to_string()];
        let ctrl_alt = ModifiersState::CONTROL | BINDING_ALT;
        assert_eq!(
            stage_key_decision(true, &rebound, &Key::Character("q".into()), ctrl_alt),
            StageKeyDecision::ExitStage
        );
        assert_eq!(
            stage_key_decision(true, &rebound, &esc(), no_mods()),
            StageKeyDecision::ConsumeForStage,
            "재바인딩 후에도 ESC 가 무대를 닫았다 — 하드코딩이 남아 있다"
        );
    }

    /// 바인딩이 비면 키보드 종료 수단만 사라진다 — 다른 키가 뒤로 새지 않는 성질은
    /// 그대로다. 탈출은 무대 셸이 항상 그리는 종료 버튼이 담당한다.
    #[test]
    fn empty_binding_leaves_the_stage_up_without_leaking_keys() {
        assert_eq!(
            stage_key_decision(true, &[], &esc(), no_mods()),
            StageKeyDecision::ConsumeForStage
        );
        assert_eq!(
            stage_key_decision(true, &[], &Key::Named(NamedKey::Enter), no_mods()),
            StageKeyDecision::ConsumeForStage
        );
    }

    fn read_state(option_as_meta: bool) -> KeyboardReadState {
        KeyboardReadState {
            shift_enter_newline: false,
            app_cursor: false,
            is_alt_screen: false,
            scroll_offset: 0,
            rows: 24,
            option_as_meta,
            win32_console: false,
        }
    }

    /// payload 들을 평탄화해 바이트 열로 모은다(Bytes/Text 모두 UTF-8 바이트로).
    fn collect_bytes(payloads: &[SendPayload]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in payloads {
            match p {
                SendPayload::Bytes(b) | SendPayload::Bound { bytes: b, .. } => {
                    out.extend_from_slice(b)
                }
                SendPayload::Text(s) => out.extend_from_slice(s.as_bytes()),
            }
        }
        out
    }

    #[test]
    fn newline_rule_only_changes_unmodified_shift_enter() {
        for (key, modifiers, expected) in [
            (Key::Named(NamedKey::Enter), ModifiersState::SHIFT, b'\n'),
            (Key::Named(NamedKey::Enter), ModifiersState::empty(), b'\r'),
            (Key::Character("c".into()), ModifiersState::CONTROL, 3),
        ] {
            let mut state = read_state(false);
            state.shift_enter_newline = true;
            let out = MainView::decide_key_to_terminal(state, &key, &None, modifiers);
            assert_eq!(collect_bytes(&out.payloads), [expected]);
        }
        let mut state = read_state(false);
        state.shift_enter_newline = true;
        let modifiers = ModifiersState::SHIFT | ModifiersState::CONTROL;
        let out =
            MainView::decide_key_to_terminal(state, &Key::Named(NamedKey::Enter), &None, modifiers);
        assert_ne!(collect_bytes(&out.payloads), b"\n");
    }

    #[test]
    fn newline_keys_preserve_enter_and_other_control_characters() {
        let cases = [
            (Key::Named(NamedKey::Enter), ModifiersState::empty(), b'\r'),
            (Key::Character("m".into()), ModifiersState::CONTROL, b'\r'),
            (Key::Character("i".into()), ModifiersState::CONTROL, b'\t'),
            (Key::Character("c".into()), ModifiersState::CONTROL, 3),
        ];
        for (key, modifiers, expected) in cases {
            let out = MainView::decide_key_to_terminal(read_state(false), &key, &None, modifiers);
            assert_eq!(collect_bytes(&out.payloads), [expected]);
            assert!(out.sent);
        }
    }

    #[test]
    fn newline_keys_follow_the_target_console_encoding() {
        // The target PTY decides, not the build OS: a Windows client attached to a
        // Linux server must send VT bytes, and the reverse must send win32-input.
        let cases = [
            (Key::Named(NamedKey::Enter), ModifiersState::SHIFT),
            (Key::Character("j".into()), ModifiersState::CONTROL),
        ];
        let win32: [&[u8]; 2] = [
            b"\x1b[13;0;13;1;16;1_\x1b[13;0;13;0;16;1_",
            b"\x1b[74;0;10;1;8;1_\x1b[74;0;10;0;8;1_",
        ];
        let vt: [&[u8]; 2] = [b"\x1b[13;2u", b"\n"];
        for (win32_console, expected) in [(true, win32), (false, vt)] {
            for ((key, modifiers), expected) in cases.clone().into_iter().zip(expected) {
                let mut state = read_state(false);
                state.scroll_offset = 12;
                state.win32_console = win32_console;
                let out = MainView::decide_key_to_terminal(
                    state,
                    &key,
                    &Some("ignored".into()),
                    modifiers,
                );
                assert_eq!(
                    collect_bytes(&out.payloads),
                    expected,
                    "win32={win32_console}"
                );
                assert_eq!(out.payloads.len(), 1);
                assert!(out.sent);
            }
        }
    }

    #[test]
    fn newline_rule_wins_over_the_console_encoding() {
        let mut state = read_state(false);
        state.win32_console = true;
        state.shift_enter_newline = true;
        let out = MainView::decide_key_to_terminal(
            state,
            &Key::Named(NamedKey::Enter),
            &None,
            ModifiersState::SHIFT,
        );
        assert_eq!(collect_bytes(&out.payloads), b"\n");
    }

    // Independent xterm PC-style cursor-key table: modifier parameter 2..8.
    // https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys
    #[test]
    fn arrow_modifiers_follow_xterm_in_both_cursor_modes() {
        let cases = [
            (ModifiersState::SHIFT, "2"),
            (ModifiersState::ALT, "3"),
            (ModifiersState::SHIFT | ModifiersState::ALT, "4"),
            (ModifiersState::CONTROL, "5"),
            (ModifiersState::SHIFT | ModifiersState::CONTROL, "6"),
            (ModifiersState::ALT | ModifiersState::CONTROL, "7"),
            (
                ModifiersState::SHIFT | ModifiersState::ALT | ModifiersState::CONTROL,
                "8",
            ),
        ];
        for (key, suffix) in [
            (NamedKey::ArrowUp, "A"),
            (NamedKey::ArrowDown, "B"),
            (NamedKey::ArrowRight, "C"),
            (NamedKey::ArrowLeft, "D"),
        ] {
            for app_cursor in [false, true] {
                for option_as_meta in [false, true] {
                    for is_alt_screen in [false, true] {
                        for (mods, parameter) in cases {
                            let mut state = read_state(option_as_meta);
                            state.app_cursor = app_cursor;
                            state.is_alt_screen = is_alt_screen;
                            let out = MainView::decide_key_to_terminal(
                                state,
                                &Key::Named(key),
                                &None,
                                mods,
                            );
                            assert_eq!(
                                collect_bytes(&out.payloads),
                                format!("\x1b[1;{parameter}{suffix}").as_bytes()
                            );
                            assert_eq!(out.payloads.len(), 1);
                            assert!(out.sent);
                            assert!(!out.dirty);
                            assert!(matches!(out.scroll_action, KeyboardScrollAction::None));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn arrows_without_terminal_modifiers_preserve_decckm() {
        for (key, normal, application) in [
            (NamedKey::ArrowUp, b"\x1b[A", b"\x1bOA"),
            (NamedKey::ArrowDown, b"\x1b[B", b"\x1bOB"),
            (NamedKey::ArrowRight, b"\x1b[C", b"\x1bOC"),
            (NamedKey::ArrowLeft, b"\x1b[D", b"\x1bOD"),
        ] {
            for app_cursor in [false, true] {
                for option_as_meta in [false, true] {
                    // Super (macOS Command) is not physical Option/Alt. Preserve its
                    // previous terminal behavior when no host shortcut consumes it.
                    for mods in [ModifiersState::empty(), ModifiersState::SUPER] {
                        let mut state = read_state(option_as_meta);
                        state.app_cursor = app_cursor;
                        let out =
                            MainView::decide_key_to_terminal(state, &Key::Named(key), &None, mods);
                        assert_eq!(
                            collect_bytes(&out.payloads),
                            if app_cursor { application } else { normal }
                        );
                        assert!(out.sent);
                    }
                }
            }
        }
    }

    #[test]
    fn modified_arrow_sends_once_and_returns_scrollback_to_bottom() {
        let mut state = read_state(false);
        state.scroll_offset = 12;
        let out = MainView::decide_key_to_terminal(
            state,
            &Key::Named(NamedKey::ArrowUp),
            &Some("ignored".into()),
            ModifiersState::ALT,
        );
        assert_eq!(collect_bytes(&out.payloads), b"\x1b[1;3A");
        assert_eq!(out.payloads.len(), 1);
        assert!(out.sent && out.dirty);
        assert!(matches!(
            out.scroll_action,
            KeyboardScrollAction::ScrollToBottom
        ));
    }

    // option_as_meta = true, Option(Alt)+'a' → ESC + base 'a' (합성문자 'å' 아님).
    #[test]
    fn option_as_meta_prefixes_esc() {
        let key = Key::Character("a".into());
        let text: Option<winit::keyboard::SmolStr> = Some("å".into());
        let out =
            MainView::decide_key_to_terminal(read_state(true), &key, &text, ModifiersState::ALT);
        assert_eq!(collect_bytes(&out.payloads), vec![0x1b, b'a']);
        assert!(out.sent);
    }

    // option_as_meta = false 이면 기존 동작(합성문자 'å' text 전송) 유지.
    #[test]
    fn option_as_meta_off_keeps_compose() {
        let key = Key::Character("a".into());
        let text: Option<winit::keyboard::SmolStr> = Some("å".into());
        let out =
            MainView::decide_key_to_terminal(read_state(false), &key, &text, ModifiersState::ALT);
        assert_eq!(collect_bytes(&out.payloads), "å".as_bytes());
        assert!(out.sent);
    }

    // option_as_meta = true 라도 Ctrl 이 함께 눌리면 Meta 분기로 빠지지 않는다
    // (Ctrl+letter control char 우선).
    #[test]
    fn option_as_meta_with_ctrl_is_control_char() {
        let key = Key::Character("a".into());
        let text: Option<winit::keyboard::SmolStr> = None;
        let out = MainView::decide_key_to_terminal(
            read_state(true),
            &key,
            &text,
            ModifiersState::ALT | ModifiersState::CONTROL,
        );
        // Ctrl 분기는 alt 가 눌리면 배제(`control_key() && !alt_key()`)되고, Meta
        // 분기는 control 이 눌리면 배제(`!control_key()`)되므로 둘 다 안 타고,
        // text 가 None 이라 아무것도 전송되지 않는다.
        assert!(out.payloads.is_empty());
        assert!(!out.sent);
    }

    // egui-mesh Text forward 게이트 — 일반 문자는 통과.
    #[test]
    fn forward_text_passes_plain_char() {
        assert!(should_forward_text("a", false, false));
        assert!(should_forward_text("가", false, false));
    }

    // command modifier(Ctrl/Cmd) 동반 문자는 억제(단축키·제어 삽입 방지).
    #[test]
    fn forward_text_suppresses_command_modifier() {
        assert!(!should_forward_text("a", true, false));
    }

    // 제어문자·사설영역 문자(예: macOS Delete `\u{f728}`)는 억제.
    #[test]
    fn forward_text_suppresses_control_and_private_use() {
        assert!(!should_forward_text("\u{7f}", false, false)); // DEL
        assert!(!should_forward_text("\u{f728}", false, false)); // macOS delete glyph
        assert!(!should_forward_text("", false, false)); // 빈 문자열
    }

    // IME 조합 중: non-ASCII 는 억제(조합 결과는 Commit 으로 도착), ASCII 는 통과.
    #[test]
    fn forward_text_ime_suppresses_non_ascii_only() {
        assert!(!should_forward_text("한", false, true));
        assert!(should_forward_text("1", false, true));
        assert!(should_forward_text(",", false, true));
        // IME 비활성이면 non-ASCII 도 통과(합성 문자 직접 입력).
        assert!(should_forward_text("한", false, false));
    }
}
