use termwiz::cell::CellAttributes;
use termwiz::color::ColorAttribute;
use termwiz::escape::csi::{
    DecPrivateMode, DecPrivateModeCode, Mode as CsiMode, TerminalMode, TerminalModeCode,
};
use termwiz::surface::{Change, CursorVisibility, Position, Surface};

use super::{CursorShape, MouseTrackingMode, TerminalState};

/// Independent bits for modes 1000, 1002, and 1003. The broadest enabled mode wins.
/// Disabling one mode leaves the others unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct MouseTrackingRegisters {
    /// 1000 — 버튼 press/release 만.
    pub(crate) click: bool,
    /// 1002 — 버튼이 눌린 동안의 셀 이동까지.
    pub(crate) cell_motion: bool,
    /// 1003 — 버튼과 무관한 모든 이동까지.
    pub(crate) any_event: bool,
}

impl MouseTrackingRegisters {
    /// 켜진 비트 중 가장 넓은 것 = 실효 트래킹 레벨.
    pub(crate) fn effective(self) -> MouseTrackingMode {
        if self.any_event {
            MouseTrackingMode::AllMotion
        } else if self.cell_motion {
            MouseTrackingMode::CellMotion
        } else if self.click {
            MouseTrackingMode::Click
        } else {
            MouseTrackingMode::None
        }
    }
}

impl TerminalState {
    /// Handle DECSET/DECRST mode changes.
    pub(crate) fn handle_mode(&mut self, mode: &CsiMode) {
        match mode {
            CsiMode::SetDecPrivateMode(DecPrivateMode::Code(code)) => {
                self.set_dec_mode(code, true);
            }
            CsiMode::ResetDecPrivateMode(DecPrivateMode::Code(code)) => {
                self.set_dec_mode(code, false);
            }
            CsiMode::SetDecPrivateMode(DecPrivateMode::Unspecified(_))
            | CsiMode::ResetDecPrivateMode(DecPrivateMode::Unspecified(_)) => {
                // Unknown mode, ignore
            }
            CsiMode::SetMode(TerminalMode::Code(code)) => {
                self.set_standard_mode(code, true);
            }
            CsiMode::ResetMode(TerminalMode::Code(code)) => {
                self.set_standard_mode(code, false);
            }
            CsiMode::SetMode(TerminalMode::Unspecified(_))
            | CsiMode::ResetMode(TerminalMode::Unspecified(_)) => {
                // Unknown standard mode, ignore
            }
            _ => {}
        }
    }

    /// Handle standard ANSI mode (SM/RM, no `?` prefix) changes.
    pub(crate) fn set_standard_mode(&mut self, code: &TerminalModeCode, enable: bool) {
        match *code {
            TerminalModeCode::Insert => {
                // IRM: subsequent prints shift existing cells right (see action_to_changes).
                self.insert_mode = enable;
            }
            TerminalModeCode::ShowCursor => {
                // Standard mode 25 mirrors DEC private 25 (DECTCEM).
                self.set_dec_mode(&DecPrivateModeCode::ShowCursor, enable);
            }
            _ => {
                // KAM/SRM/LNM/BiDi: not supported, ignore.
            }
        }
    }

    pub(crate) fn set_dec_mode(&mut self, code: &DecPrivateModeCode, enable: bool) {
        match *code {
            DecPrivateModeCode::ApplicationCursorKeys => {
                self.application_cursor_keys = enable;
            }
            DecPrivateModeCode::StartBlinkingCursor => {
                // Cursor blink -- no-op for now (rendering doesn't support blink)
            }
            DecPrivateModeCode::ShowCursor => {
                self.cursor_visible = enable;
                let vis = if enable {
                    CursorVisibility::Visible
                } else {
                    CursorVisibility::Hidden
                };
                self.apply_or_stage_change(Change::CursorVisibility(vis));
            }
            DecPrivateModeCode::ClearAndEnableAlternateScreen => {
                // Mode 1049: save cursor, switch to alt screen, clear it
                if enable {
                    let pos = self.primary_surface.cursor_position();
                    self.alt_saved_cursor = Some((pos.0, pos.1));
                    if self.alternate_surface.is_none() {
                        self.alternate_surface = Some(Surface::new(self.cols, self.rows));
                        self.alternate_epoch = crate::ContentEpoch::fresh();
                    }
                    // Stash the primary pen and adopt the alt surface's pen mirror
                    // (only on a real primary→alt transition).
                    if !self.use_alternate {
                        self.swap_pen_for_surface_switch();
                    }
                    self.use_alternate = true;
                    self.alternate_epoch = crate::ContentEpoch::fresh();
                    if let Some(alt) = &mut self.alternate_surface {
                        alt.add_change(Change::ClearScreen(ColorAttribute::Default));
                        alt.add_change(Change::CursorPosition {
                            x: Position::Absolute(0),
                            y: Position::Absolute(0),
                        });
                    }
                    // The clear above resets the alt surface pen to default
                    // (ClearScreen bypasses `apply_change`/`mirror_pen`).
                    self.current_pen = CellAttributes::default();
                } else {
                    // Leave alternate screen — restore the primary pen mirror.
                    if self.use_alternate {
                        self.swap_pen_for_surface_switch();
                    }
                    self.use_alternate = false;
                    if let Some((x, y)) = self.alt_saved_cursor.take() {
                        self.apply_or_stage_change(Change::CursorPosition {
                            x: Position::Absolute(x),
                            y: Position::Absolute(y),
                        });
                    }
                }
            }
            DecPrivateModeCode::EnableAlternateScreen
            | DecPrivateModeCode::OptEnableAlternateScreen => {
                // Mode 47 / 1047: switch without save/clear
                if enable {
                    if self.alternate_surface.is_none() {
                        self.alternate_surface = Some(Surface::new(self.cols, self.rows));
                        self.alternate_epoch = crate::ContentEpoch::fresh();
                    }
                    // No clear here: adopt the alt surface's retained pen mirror.
                    if !self.use_alternate {
                        self.swap_pen_for_surface_switch();
                    }
                    self.use_alternate = true;
                } else {
                    if self.use_alternate {
                        self.swap_pen_for_surface_switch();
                    }
                    self.use_alternate = false;
                }
            }
            DecPrivateModeCode::SaveCursor => {
                // Mode 1048: save/restore cursor
                if enable {
                    let pos = self.surface().cursor_position();
                    self.saved_cursor = Some((pos.0, pos.1));
                } else if let Some((x, y)) = self.saved_cursor {
                    self.apply_or_stage_change(Change::CursorPosition {
                        x: Position::Absolute(x),
                        y: Position::Absolute(y),
                    });
                }
            }
            DecPrivateModeCode::BracketedPaste => {
                self.bracketed_paste = enable;
            }
            DecPrivateModeCode::MouseTracking => {
                self.update_mouse_tracking(|r| r.click = enable);
            }
            DecPrivateModeCode::ButtonEventMouse => {
                self.update_mouse_tracking(|r| r.cell_motion = enable);
            }
            DecPrivateModeCode::AnyEventMouse => {
                self.update_mouse_tracking(|r| r.any_event = enable);
            }
            DecPrivateModeCode::SGRMouse => {
                self.sgr_mouse = enable;
            }
            DecPrivateModeCode::FocusTracking => {
                self.focus_tracking = enable;
            }
            DecPrivateModeCode::SynchronizedOutput => {
                if enable && !self.synchronized_output {
                    self.synchronized_output_block += 1;
                }
                self.synchronized_output = enable;
            }
            DecPrivateModeCode::ReverseVideo => {
                // DECSCNM (mode 5): reverse the whole screen. Stored as a flag the
                // renderer consumes by swapping the default fg/bg; the grid content
                // is untouched.
                self.reverse_screen = enable;
            }
            DecPrivateModeCode::OriginMode => {
                // DECOM (mode 6): origin mode. Setting or resetting it always homes
                // the cursor — to the region top-left in origin mode, else to the
                // screen top-left. Absolute positioning honors this via
                // resolve_origin_row(); relative-move confinement to the region is
                // not modeled (rarely relied upon).
                self.origin_mode = enable;
                let home_row = self.origin_home_row();
                self.apply_or_stage_change(Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(home_row),
                });
            }
            DecPrivateModeCode::AutoWrap => {
                // AutoWrap is handled by termwiz Surface internally, ignore for now
            }
            _ => {
                // Unknown/unsupported mode, ignore
            }
        }
    }
}

impl TerminalState {
    /// Whether application cursor keys mode is active (DECCKM).
    pub fn application_cursor_keys(&self) -> bool {
        self.application_cursor_keys
    }

    /// Whether the cursor is visible (DECTCEM).
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Current cursor shape (DECSCUSR).
    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor_shape
    }

    /// Whether bracketed paste mode is active.
    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// 켜진 세 모드 중 가장 넓은 마우스 트래킹 범위.
    pub fn mouse_tracking(&self) -> MouseTrackingMode {
        self.mouse_tracking.effective()
    }

    /// 모드 하나를 갱신한다. 전체 트래킹이 처음 켜질 때만 안내를 준비하고,
    /// 모두 꺼지면 해제한다. 켜진 모드 사이의 전환은 안내를 다시 준비하지 않는다.
    fn update_mouse_tracking(&mut self, f: impl FnOnce(&mut MouseTrackingRegisters)) {
        let before = self.mouse_tracking.effective();
        f(&mut self.mouse_tracking);
        let after = self.mouse_tracking.effective();
        if before == MouseTrackingMode::None && after != MouseTrackingMode::None {
            self.mouse_capture_hint_armed = true;
        } else if before != MouseTrackingMode::None && after == MouseTrackingMode::None {
            self.mouse_capture_hint_armed = false;
        }
    }

    /// 첫 캡처 안내를 소비한다. 같은 트래킹 세션에서는 처음 호출한 쪽만 true를 받는다.
    pub fn take_mouse_capture_hint(&mut self) -> bool {
        let armed = self.mouse_capture_hint_armed;
        self.mouse_capture_hint_armed = false;
        armed
    }

    /// Whether SGR mouse encoding is active.
    pub fn sgr_mouse(&self) -> bool {
        self.sgr_mouse
    }

    /// Whether focus tracking is active.
    pub fn focus_tracking(&self) -> bool {
        self.focus_tracking
    }

    /// Whether the alternate screen is active.
    pub fn is_alternate_screen(&self) -> bool {
        self.use_alternate
    }

    /// Whether synchronized output mode (DEC 2026) is active.
    pub fn synchronized_output(&self) -> bool {
        self.synchronized_output
    }

    /// The current synchronized output block, or `None` outside a block. Each block entered
    /// gets a new value, so two reads in different blocks never match.
    pub fn synchronized_output_block(&self) -> Option<u64> {
        self.synchronized_output
            .then_some(self.synchronized_output_block)
    }

    /// Whether reverse-screen mode (DECSCNM, DEC private mode 5) is active. The
    /// renderer swaps the default foreground/background when this is set.
    pub fn screen_reverse(&self) -> bool {
        self.reverse_screen
    }

    /// Scan the active surface for an isolated reverse-video cell.
    ///
    /// Some TUIs (notably Ink-based ones like Claude Code) hide the real terminal
    /// cursor with `\e[?25l` and draw their own "fake cursor" by emitting a single
    /// cell with the reverse-video attribute (`\e[7m`). This scan detects that cell
    /// so we can use its position as the IME preedit anchor.
    ///
    /// Returns the cell position only when a **single** reverse-video cell exists.
    /// Multi-cell reverse regions (selection highlight, inverse-painted UI) are
    /// ambiguous and return None.
    pub fn find_fake_cursor_cell(&self) -> Option<(usize, usize)> {
        let surface = self.surface();
        let mut found: Option<(usize, usize)> = None;
        for (row_idx, line) in surface.screen_lines().iter().enumerate() {
            for cell_ref in line.visible_cells() {
                if cell_ref.attrs().reverse() {
                    if found.is_some() {
                        return None; // two or more — ambiguous
                    }
                    found = Some((cell_ref.cell_index(), row_idx));
                }
            }
        }
        found
    }
}
