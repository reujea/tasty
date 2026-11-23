//! Control characters and SGR attributes.

use termwiz::cell::{AttributeChange, CellAttributes};
use termwiz::escape::ControlCode;
use termwiz::escape::csi::{CSI, Sgr};
use termwiz::surface::{Change, Position};

use crate::{TerminalEvent, TerminalEventKind, TerminalState};

impl TerminalState {
    pub(crate) fn map_control(&mut self, code: ControlCode) -> Vec<Change> {
        match code {
            ControlCode::LineFeed | ControlCode::VerticalTab | ControlCode::FormFeed => {
                self.perform_index()
            }
            ControlCode::CarriageReturn => vec![Change::Text("\r".into())],
            ControlCode::Backspace => vec![Change::CursorPosition {
                x: Position::Relative(-1),
                y: Position::Relative(0),
            }],
            ControlCode::HorizontalTab => {
                // HT advances the cursor to the next tab stop (default every 8
                // columns), clamped at the right margin. termwiz `print_text` does
                // not expand "\t", so emitting a literal tab would advance only one
                // column — move the cursor explicitly instead.
                let (cx, _cy) = self.surface().cursor_position();
                let target = self.next_tab_stop(cx);
                vec![Change::CursorPosition {
                    x: Position::Absolute(target),
                    y: Position::Relative(0),
                }]
            }
            ControlCode::ShiftOut => {
                // SO: invoke G1 into GL.
                self.charset_active_g1 = true;
                vec![]
            }
            ControlCode::ShiftIn => {
                // SI: invoke G0 into GL.
                self.charset_active_g1 = false;
                vec![]
            }
            ControlCode::Bell => {
                self.events.push(TerminalEvent {
                    generation: self.connection.generation(),
                    surface_id: 0,
                    kind: TerminalEventKind::BellRing,
                    attach_streamed: false,
                });
                vec![]
            }
            _ => vec![],
        }
    }

    pub(crate) fn map_csi(&mut self, csi: CSI) -> Vec<Change> {
        match csi {
            CSI::Sgr(sgr) => self.map_sgr(sgr),
            CSI::Cursor(cursor) => self.map_cursor(cursor),
            CSI::Edit(edit) => self.map_edit(edit),
            CSI::Mode(_mode) => {
                // Mode actions are handled by ingest before this dispatch.
                vec![]
            }
            CSI::Device(device) => {
                self.handle_device(*device);
                vec![]
            }
            CSI::Mouse(_) => vec![],
            CSI::Window(window) => {
                self.handle_window(*window);
                vec![]
            }
            CSI::Keyboard(_) => vec![],
            _ => vec![],
        }
    }

    pub(crate) fn map_sgr(&self, sgr: Sgr) -> Vec<Change> {
        match sgr {
            Sgr::Reset => vec![Change::AllAttributes(CellAttributes::default())],
            Sgr::Intensity(intensity) => {
                vec![Change::Attribute(AttributeChange::Intensity(intensity))]
            }
            Sgr::Underline(underline) => {
                vec![Change::Attribute(AttributeChange::Underline(underline))]
            }
            Sgr::Italic(on) => vec![Change::Attribute(AttributeChange::Italic(on))],
            Sgr::Blink(blink) => vec![Change::Attribute(AttributeChange::Blink(blink))],
            Sgr::Inverse(on) => vec![Change::Attribute(AttributeChange::Reverse(on))],
            Sgr::Invisible(on) => vec![Change::Attribute(AttributeChange::Invisible(on))],
            Sgr::StrikeThrough(on) => {
                vec![Change::Attribute(AttributeChange::StrikeThrough(on))]
            }
            Sgr::Foreground(color_spec) => {
                vec![Change::Attribute(AttributeChange::Foreground(
                    color_spec.into(),
                ))]
            }
            Sgr::Background(color_spec) => {
                vec![Change::Attribute(AttributeChange::Background(
                    color_spec.into(),
                ))]
            }
            Sgr::Font(_) => {
                // CellInfo has no font field, so dropping this keeps reporting
                // consistent. Not commonly needed for basic terminal emulation.
                vec![]
            }
            // These SGR fields have no single-field AttributeChange. Update a clone
            // of the mirrored pen and replace all attributes to preserve the other fields.
            Sgr::Overline(on) => {
                let mut pen = self.current_pen.clone();
                pen.set_overline(on);
                vec![Change::AllAttributes(pen)]
            }
            Sgr::UnderlineColor(color_spec) => {
                let mut pen = self.current_pen.clone();
                pen.set_underline_color(color_spec);
                vec![Change::AllAttributes(pen)]
            }
            Sgr::VerticalAlign(align) => {
                let mut pen = self.current_pen.clone();
                pen.set_vertical_align(align);
                vec![Change::AllAttributes(pen)]
            }
        }
    }
}
