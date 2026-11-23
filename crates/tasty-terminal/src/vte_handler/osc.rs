//! OSC metadata, clipboard events, and terminal queries.

use std::sync::Arc;

use termwiz::cell::{AttributeChange, CellAttributes};
use termwiz::escape::OperatingSystemCommand;
use termwiz::escape::csi::{Device, Window};
use termwiz::escape::osc::ColorOrQuery;
use termwiz::surface::{Change, CursorVisibility};

use crate::foreground_process::is_known_shell_name;
use crate::{TerminalEvent, TerminalEventKind, TerminalState};

/// Ignore shell executable paths used as default titles. Require both a path separator
/// or colon and a known shell basename, so bare shell names and prompt titles remain valid.
fn looks_like_shell_exe_path(title: &str) -> bool {
    if !title.contains(['/', '\\', ':']) {
        return false;
    }
    let basename = title.rsplit(['/', '\\']).next().unwrap_or(title);
    is_known_shell_name(basename)
}

impl TerminalState {
    /// 제목 OSC (0/1/2/Sun) 공통 처리 — 셸 exe 경로 제목은 소스에서 무시.
    fn set_title_from_osc(&mut self, title: String) {
        if looks_like_shell_exe_path(&title) {
            tracing::debug!("ignoring shell-exe-path OSC title: {title:?}");
            return;
        }
        self.current_title = Some(title.clone());
        self.events.push(TerminalEvent {
            generation: self.connection.generation(),
            surface_id: 0,
            kind: TerminalEventKind::TitleChanged(title),
            attach_streamed: false,
        });
    }

    #[allow(clippy::cognitive_complexity)] // complexity-exempt: OSC 커맨드 평면 match 디스패치 — 시퀀스별 arm 나열, 중첩 얕음
    pub(crate) fn map_osc(&mut self, osc: OperatingSystemCommand) {
        match osc {
            OperatingSystemCommand::SetIconNameAndWindowTitle(title)
            | OperatingSystemCommand::SetWindowTitle(title)
            | OperatingSystemCommand::SetWindowTitleSun(title) => {
                self.set_title_from_osc(title);
            }
            OperatingSystemCommand::CurrentWorkingDirectory(url) => {
                let path = if let Some(stripped) = url.strip_prefix("file://") {
                    if let Some(slash_pos) = stripped.find('/') {
                        stripped[slash_pos..].to_string()
                    } else {
                        stripped.to_string()
                    }
                } else {
                    url.clone()
                };
                // On Windows, OSC 7 paths arrive as "/C:/foo/bar" (URI form);
                // strip the leading slash so PathBuf yields a valid drive path.
                #[cfg(windows)]
                let path = {
                    let bytes = path.as_bytes();
                    if bytes.len() >= 4
                        && bytes[0] == b'/'
                        && bytes[2] == b':'
                        && bytes[3] == b'/'
                        && bytes[1].is_ascii_alphabetic()
                    {
                        path[1..].replace('/', "\\")
                    } else {
                        path
                    }
                };
                // Cache the reported directory for get_cwd.
                self.cached_cwd = Some(std::path::PathBuf::from(&path));
                self.events.push(TerminalEvent {
                    generation: self.connection.generation(),
                    surface_id: 0,
                    kind: TerminalEventKind::CwdChanged(path),
                    attach_streamed: false,
                });
            }
            OperatingSystemCommand::SystemNotification(body) => {
                self.events.push(TerminalEvent {
                    generation: self.connection.generation(),
                    surface_id: 0,
                    kind: TerminalEventKind::Notification {
                        title: "Terminal".to_string(),
                        body,
                    },
                    attach_streamed: false,
                });
            }
            OperatingSystemCommand::RxvtExtension(parts) => {
                if parts.first().map(|s| s.as_str()) == Some("notify") {
                    let title = parts.get(1).cloned().unwrap_or_default();
                    let body = parts.get(2).cloned().unwrap_or_default();
                    self.events.push(TerminalEvent {
                        generation: self.connection.generation(),
                        surface_id: 0,
                        kind: TerminalEventKind::Notification { title, body },
                        attach_streamed: false,
                    });
                }
            }
            // Store OSC 8 in the surface pen so subsequent cells inherit the link.
            // An empty URI closes it; full/soft reset clears the pen hyperlink.
            OperatingSystemCommand::SetHyperlink(opt) => {
                self.apply_or_stage_change(Change::Attribute(AttributeChange::Hyperlink(
                    opt.map(Arc::new),
                )));
            }
            // Dynamic color queries use base + entry index. Answer only known colors
            // from the host palette; ignore color assignments. Replies end with ST.
            OperatingSystemCommand::ChangeDynamicColors(first_color, colors) => {
                let Some(palette) = self.color_palette.clone() else {
                    return;
                };
                let base = first_color as u8;
                for (offset, color) in colors.iter().enumerate() {
                    if !matches!(color, ColorOrQuery::Query) {
                        continue;
                    }
                    let number = base + offset as u8;
                    if let Some(rgb) = palette.dynamic_color(number) {
                        self.send_terminal_response(&format!(
                            "\x1b]{};{}\x1b\\",
                            number,
                            rgb.to_x11_16bit()
                        ));
                    }
                }
            }
            // OSC 4 answers theme palette queries for indices 0..16. Ignore assignments
            // and indices in the fixed xterm cube/grayscale range.
            OperatingSystemCommand::ChangeColorNumber(pairs) => {
                let Some(palette) = self.color_palette.clone() else {
                    return;
                };
                for pair in &pairs {
                    if !matches!(pair.color, ColorOrQuery::Query) {
                        continue;
                    }
                    if let Some(rgb) = palette.ansi_color(pair.palette_index) {
                        self.send_terminal_response(&format!(
                            "\x1b]4;{};{}\x1b\\",
                            pair.palette_index,
                            rgb.to_x11_16bit()
                        ));
                    }
                }
            }
            OperatingSystemCommand::SetSelection(_selection, data) => {
                self.events.push(TerminalEvent {
                    generation: self.connection.generation(),
                    surface_id: 0,
                    kind: TerminalEventKind::ClipboardSet(data),
                    attach_streamed: false,
                });
            }
            // OSC 52 read query (`OSC 52 ; c ; ? ST`). Emit an event only — the
            // terminal crate has no access to the system clipboard or the user's
            // privacy setting. The host decides whether to answer (gated on
            // `allow_clipboard_read`, default off → no reply). The selection kind
            // is ignored; the host always answers for the clipboard (`c`).
            OperatingSystemCommand::QuerySelection(_selection) => {
                self.events.push(TerminalEvent {
                    generation: self.connection.generation(),
                    surface_id: 0,
                    kind: TerminalEventKind::ClipboardQuery,
                    attach_streamed: false,
                });
            }
            // termwiz handles standard OSC 133 markers here. Additional tokens can
            // fall back to Unspecified below. Both paths emit PromptBoundary.
            OperatingSystemCommand::FinalTermSemanticPrompt(prompt) => {
                use termwiz::escape::osc::FinalTermSemanticPrompt as Ftsp;
                let (phase, payload) = match prompt {
                    Ftsp::FreshLineAndStartPrompt { .. } => ('A', String::new()),
                    Ftsp::MarkEndOfPromptAndStartOfInputUntilNextMarker => ('B', String::new()),
                    Ftsp::MarkEndOfInputAndStartOfOutput { .. } => ('C', String::new()),
                    Ftsp::CommandStatus { status, .. } => ('D', status.to_string()),
                    // FreshLine / MarkEndOfCommandWithFreshLine / StartPrompt /
                    // MarkEndOfPromptAndStartOfInputUntilEndOfLine 은 tasty 의
                    // A/B/C/D 4-phase 모델 밖(다른 셸 통합 확장 마커) — 무시.
                    _ => return,
                };
                self.events.push(TerminalEvent {
                    generation: self.connection.generation(),
                    surface_id: 0,
                    kind: TerminalEventKind::PromptBoundary { phase, payload },
                    attach_streamed: false,
                });
            }
            OperatingSystemCommand::Unspecified(params) => {
                if let Some(first) = params.first() {
                    if first == b"133" {
                        // Fallback for OSC 133 markers that termwiz did not parse into its
                        // dedicated variant, such as B with additional cmd tokens.
                        if let Some(second) = params.get(1)
                            && let Some(&phase_byte) = second.first()
                            && matches!(phase_byte, b'A' | b'B' | b'C' | b'D')
                        {
                            let phase = phase_byte as char;
                            // Build payload as `<second_rest>[;param2][;param3]...`
                            let mut payload = String::new();
                            let second_str = String::from_utf8_lossy(second);
                            if second_str.len() > 1 {
                                // e.g. second = "D;0" → second_rest = "0"
                                payload.push_str(&second_str[1..]);
                                // Strip a leading ';' if present so the payload is
                                // semicolon-joined without sentinels.
                                payload = payload.trim_start_matches(';').to_string();
                            }
                            for extra in params.iter().skip(2) {
                                if !payload.is_empty() {
                                    payload.push(';');
                                }
                                payload.push_str(&String::from_utf8_lossy(extra));
                            }
                            self.events.push(TerminalEvent {
                                generation: self.connection.generation(),
                                surface_id: 0,
                                kind: TerminalEventKind::PromptBoundary { phase, payload },
                                attach_streamed: false,
                            });
                        }
                        return;
                    }
                    if first == b"99" {
                        let mut title = String::new();
                        let mut body = String::new();
                        for param in params.iter().skip(1) {
                            let s = String::from_utf8_lossy(param);
                            if let Some(val) = s.strip_prefix("t=") {
                                title = val.to_string();
                            } else if let Some(val) = s.strip_prefix("d=0;") {
                                body = val.to_string();
                            } else if let Some(val) = s.strip_prefix("d=1;") {
                                body = val.to_string();
                            } else if !s.contains('=') {
                                body = s.to_string();
                            }
                        }
                        if title.is_empty() {
                            title = "Terminal".to_string();
                        }
                        self.events.push(TerminalEvent {
                            generation: self.connection.generation(),
                            surface_id: 0,
                            kind: TerminalEventKind::Notification { title, body },
                            attach_streamed: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// Answer XtGetTcap with unsupported status 0 and the requested hex name.
    /// No capability database is provided; replying avoids leaving the query unanswered.
    pub(crate) fn handle_xtgettcap(&mut self, names: &[String]) {
        for name in names {
            let mut response = String::from("\x1bP0+r");
            for &b in name.as_bytes() {
                response.push_str(&format!("{b:02x}"));
            }
            response.push_str("\x1b\\");
            self.send_terminal_response(&response);
        }
    }

    /// Answer cell-size reports and title-stack operations. Ignore window manipulation
    /// and window position/state/title probes to keep terminal output from controlling
    /// the user's window. Pixel-size queries are unsupported; pixel metrics belong to the renderer.
    pub(crate) fn handle_window(&mut self, window: Window) {
        match window {
            // Text area / screen size in character cells: `CSI 8 ; rows ; cols t`
            // and `CSI 9 ; rows ; cols t`.
            Window::ReportTextAreaSizeCells => {
                self.send_terminal_response(&format!("\x1b[8;{};{}t", self.rows, self.cols));
            }
            Window::ReportScreenSizeCells => {
                self.send_terminal_response(&format!("\x1b[9;{};{}t", self.rows, self.cols));
            }
            // Title stack: push saves the current title; pop restores it and
            // re-emits a TitleChanged event. The single title backs all three
            // (icon / window / both) variants. The stack is bounded.
            Window::PushWindowTitle | Window::PushIconAndWindowTitle | Window::PushIconTitle => {
                self.title_stack.push(self.current_title.clone());
                if self.title_stack.len() > 64 {
                    self.title_stack.remove(0);
                }
            }
            Window::PopWindowTitle | Window::PopIconAndWindowTitle | Window::PopIconTitle => {
                if let Some(restored) = self.title_stack.pop() {
                    self.current_title = restored.clone();
                    if let Some(title) = restored {
                        self.events.push(TerminalEvent {
                            generation: self.connection.generation(),
                            surface_id: 0,
                            kind: TerminalEventKind::TitleChanged(title),
                            attach_streamed: false,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_device(&mut self, device: Device) {
        match device {
            Device::StatusReport => self.send_terminal_response("\x1b[0n"),
            Device::RequestPrimaryDeviceAttributes => {
                self.send_terminal_response("\x1b[?1;2c");
            }
            // DA2 (CSI > c): secondary device attributes — `CSI > Pp ; Pv ; Pc c`.
            // Pp = terminal type (0, xterm-compatible class), Pv = version code,
            // Pc = ROM cartridge (0). Apps identify the terminal by name via
            // XTVERSION; DA2 just needs to answer with a recognizable triple.
            Device::RequestSecondaryDeviceAttributes => {
                self.send_terminal_response("\x1b[>0;10;0c");
            }
            // XTVERSION (CSI > q): terminal name and version, reported as a DCS
            // string `DCS > | tasty(<version>) ST`. The name "tasty" is the
            // identifier modern apps (neovim, tmux) key on. The version is the
            // tasty-terminal crate version (policy: identification, not exact
            // build); ST (ESC \) terminator per xterm convention.
            Device::RequestTerminalNameAndVersion => {
                self.send_terminal_response(concat!(
                    "\x1bP>|tasty(",
                    env!("CARGO_PKG_VERSION"),
                    ")\x1b\\"
                ));
            }
            // DA3 (CSI = c): tertiary device attributes — `DCS ! | <unit id> ST`.
            // Unit id is a fixed hex string (very rarely queried).
            Device::RequestTertiaryDeviceAttributes => {
                self.send_terminal_response("\x1bP!|54415354\x1b\\");
            }
            // DECSTR (CSI ! p): soft terminal reset. Unlike RIS (ESC c), this does
            // NOT clear the screen, switch the alternate screen, or touch the
            // palette/tab stops — screen content and alt-screen state are preserved.
            // It restores the cursor/SGR/mode subset apps rely on at init time.
            Device::SoftReset => {
                // Margins → full screen (DECSTBM).
                self.scroll_region = None;
                // Saved cursor → cleared (DECSC store).
                self.saved_cursor = None;
                // Application cursor keys (DECCKM) → reset.
                self.application_cursor_keys = false;
                // Origin mode (DECOM) → reset (cursor positioning becomes absolute).
                self.origin_mode = false;
                // Insert/replace mode (IRM) → replace.
                self.insert_mode = false;
                // Text cursor enable (DECTCEM) → visible.
                self.cursor_visible = true;
                // DECSCUSR cursor shape is intentionally NOT reset by DECSTR
                // (matches xterm — only RIS restores the default shape).
                // SGR → default, cursor → visible. Applied via the surface since
                // handle_device returns (); screen content stays intact.
                self.apply_or_stage_change(Change::AllAttributes(CellAttributes::default()));
                self.apply_or_stage_change(Change::CursorVisibility(CursorVisibility::Visible));
            }
            _ => {}
        }
    }
}
