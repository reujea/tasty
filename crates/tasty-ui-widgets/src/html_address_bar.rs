//! Host-only HTML address band. It emits actions; it never navigates, looks up files,
//! changes a document, or acquires native keyboard focus. Labels are caller supplied.

use crate::{
    ControlSize, HtmlRemoteMarkerView, IconButton, Input, Spinner, Tooltip, TooltipPlacement,
    tooltip_hover_delay_elapsed,
};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlAddressKind {
    Empty,
    Local,
    Web,
    Mirror,
    MirrorPlaceholder,
}

#[derive(Clone, Debug, Default)]
pub struct HtmlAddressState {
    pub buffer: String,
    pub editing: bool,
    pub error: Option<String>,
}

impl HtmlAddressState {
    /// Commit updates do not overwrite a user's draft or a rejected input.
    pub fn synchronize(&mut self, current: &str) {
        if !self.editing && self.error.is_none() {
            self.buffer = current.into();
        }
    }
    pub fn reject(&mut self, message: String) {
        self.error = Some(message);
        self.editing = true;
    }
    pub fn accepted(&mut self) {
        self.error = None;
        self.editing = false;
    }
    pub fn revert(&mut self, current: &str) {
        self.buffer = current.into();
        self.error = None;
        self.editing = false;
    }
}

pub struct HtmlAddressLabels<'a> {
    pub placeholder: &'a str,
    pub back: &'a str,
    pub forward: &'a str,
    pub reload: &'a str,
    pub disconnected: &'a str,
}

pub struct HtmlAddressView<'a> {
    /// Display form of the committed address, or the attempted address while loading/failed.
    /// The host keeps these separate in its navigation model.
    pub address: &'a str,
    /// Latest committed display address; Esc never restores a failed/pending attempt.
    pub committed_address: &'a str,
    pub kind: HtmlAddressKind,
    pub can_back: bool,
    pub can_forward: bool,
    pub loading: bool,
    pub disconnected: bool,
    /// Received-design trailing skipped tag and top tooltip, caller supplied.
    pub skipped: Option<(&'a str, &'a str)>,
    /// Remote content marker at the end of the field, after the skipped Tag. The host shows it
    /// only while the remote content notice is not on screen.
    pub remote_marker: Option<HtmlRemoteMarkerView<'a>>,
    /// One-frame, user-origin focus request. Never set this for an agent navigation.
    pub focus: bool,
    pub labels: HtmlAddressLabels<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlAddressAction {
    None,
    Back,
    Forward,
    Reload,
    Open(String),
    Revert,
}

pub struct HtmlAddressOutput {
    pub rect: egui::Rect,
    pub field_rect: Option<egui::Rect>,
    pub forward_rect: Option<egui::Rect>,
    pub reload_rect: Option<egui::Rect>,
    /// Hit cell of the remote content marker when one was drawn.
    pub remote_marker_rect: Option<egui::Rect>,
    /// The blocked remote content marker was clicked; the host shows the notice again.
    pub remote_marker_clicked: bool,
    pub action: HtmlAddressAction,
    /// Host uses this to keep keyboard input out of the native WebView.
    pub has_keyboard_focus: bool,
}

/// Separator is painted inside the allocated band. An error adds exactly one error row.
pub fn html_address_height(theme: &Theme, kind: HtmlAddressKind, error: bool) -> LogicalPx {
    if kind == HtmlAddressKind::MirrorPlaceholder {
        LogicalPx(0.0)
    } else {
        theme.html_address_bar_height()
            + if error {
                theme.html_address_error_height()
            } else {
                LogicalPx(0.0)
            }
    }
}

/// Shared inset arithmetic; banner heights are measured by their views in size→script order.
pub fn html_address_content_inset(
    theme: &Theme,
    band: LogicalPx,
    banners: &[LogicalPx],
) -> LogicalPx {
    if banners.is_empty() {
        band
    } else {
        banners.iter().fold(band + theme.banner_margin(), |h, b| {
            h + *b + theme.banner_inset_gap()
        })
    }
}

fn nav(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: tasty_icons::Icon,
    enabled: bool,
    label: &str,
) -> egui::Response {
    let resp = IconButton::new()
        .size(ControlSize::Sm)
        .enabled(enabled)
        .show(ui, theme, &|ui, rect, color| {
            icon.image(rect.height(), color).paint_at(ui, rect);
        });
    if resp.has_focus() {
        ui.painter().rect_stroke(
            resp.rect,
            theme.icon_button_radius().value(),
            egui::Stroke::new(theme.focus_ring_width.value(), theme.border_focus()),
            egui::StrokeKind::Outside,
        );
    }
    if tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered()) {
        Tooltip::new(label)
            .placement(TooltipPlacement::Top)
            .id_source(resp.id)
            .show(ui, theme, resp.rect);
    }
    resp
}

fn left_ellipsis(ui: &egui::Ui, theme: &Theme, text: &str, width: LogicalPx) -> String {
    let font = egui::FontId::monospace(theme.input_font_size().value());
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.into(), font.clone(), egui::Color32::PLACEHOLDER)
            .rect
            .width()
            <= width.value()
    };
    if fits(text) {
        return text.into();
    }
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut lo = 0;
    let mut hi = boundaries.len() - 1;
    while lo < hi {
        let mid = (lo + hi) / 2;
        if fits(&format!("…{}", &text[boundaries[mid]..])) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    format!("…{}", &text[boundaries[lo]..])
}

/// Egui id of the address field inside the band `id`. The host uses it to give up the
/// field's keyboard focus when the page takes it or after an address is opened.
pub fn html_address_field_id(id: egui::Id) -> egui::Id {
    id.with("address")
}

/// Gives up keyboard focus held by the field or a button of the band `id`.
pub fn html_address_surrender_focus(ui: &egui::Ui, id: egui::Id) {
    let focusable: Vec<egui::Id> = ui
        .data(|d| d.get_temp(id.with("focusable")))
        .unwrap_or_else(|| vec![html_address_field_id(id)]);
    ui.memory_mut(|m| {
        if let Some(f) = m.focused().filter(|f| focusable.contains(f)) {
            m.surrender_focus(f);
        }
    });
}

/// Uses the same Input box as PathField, without its Go/completion composite. Explicit
/// child rectangles keep nav cells fixed even below the minimum supported field width.
pub fn html_address_bar(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: egui::Id,
    width: LogicalPx,
    state: &mut HtmlAddressState,
    view: &HtmlAddressView<'_>,
) -> HtmlAddressOutput {
    let height = html_address_height(theme, view.kind, state.error.is_some());
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), height.value()),
        egui::Sense::hover(),
    );
    let mut out = HtmlAddressOutput {
        rect,
        field_rect: None,
        forward_rect: None,
        reload_rect: None,
        remote_marker_rect: None,
        remote_marker_clicked: false,
        action: HtmlAddressAction::None,
        has_keyboard_focus: false,
    };
    if view.kind == HtmlAddressKind::MirrorPlaceholder {
        return out;
    }
    state.synchronize(view.address);
    let mirror = view.kind == HtmlAddressKind::Mirror;
    let forward = !mirror && width >= theme.banner_narrow_below();
    ui.painter()
        .rect_filled(rect, 0.0, theme.html_address_bar_bg().to_egui());
    let row_height = theme.html_address_bar_height();
    let focused_at_start = ui.memory(|m| m.focused());
    let (mut x, mut focusable) = nav_buttons(ui, theme, id, rect, view, forward, &mut out);
    x += theme.html_address_bar_gap() - theme.html_address_bar_nav_gap();
    let field_width = (LogicalPx(rect.right()) - theme.html_address_bar_padding_x() - x)
        .max(theme.html_address_field_min_width());
    let field_rect = egui::Rect::from_min_size(
        egui::pos2(
            x.value(),
            (LogicalPx(rect.top()) + (row_height - theme.input_height()) / 2.0).value(),
        ),
        egui::vec2(field_width.value(), theme.input_height().value()),
    );
    let mut field = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(field_rect));
    field.set_clip_rect(ui.clip_rect().intersect(rect));
    let field_id = html_address_field_id(id);
    let had_focus = ui.memory(|m| m.has_focus(field_id));
    let was_focused = ui.data(|d| {
        d.get_temp::<bool>(field_id.with("previous_focus"))
            .unwrap_or(false)
    });
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    // The marker cell ends `space-xs` into the field's right padding (design margin-right).
    let marker_rect = view.remote_marker.map(|_| {
        let hit = crate::html_remote_marker_hit(theme);
        let right =
            (LogicalPx(field_rect.right()) - theme.input_padding_x() + theme.spacing_xs).value();
        egui::Rect::from_min_size(
            egui::pos2(right - hit, field_rect.center().y - hit / 2.0),
            egui::vec2(hit, hit),
        )
    });
    let on_marker = marker_rect.is_some_and(|m| ui.rect_contains_pointer(m));
    let clicked_field = ui.input(|i| i.pointer.primary_pressed())
        && ui.rect_contains_pointer(field_rect.intersect(rect))
        && !on_marker
        && ui.is_enabled();
    if clicked_field && !mirror {
        state.editing = true;
        ui.memory_mut(|m| m.request_focus(field_id));
    }
    if had_focus && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, .. } if !matches!(key, egui::Key::Escape | egui::Key::Enter)))) {
        state.error = None;
    }

    if view.focus && !mirror && ui.is_enabled() {
        state.editing = true;
        ui.memory_mut(|m| m.request_focus(field_id));
    }
    let icon = match view.kind {
        HtmlAddressKind::Local => tasty_icons::FILE,
        HtmlAddressKind::Web => tasty_icons::LINK,
        HtmlAddressKind::Empty => tasty_icons::HTML,
        _ => tasty_icons::REMOTE,
    };
    let paint = |ui: &mut egui::Ui, r: egui::Rect, c| {
        let size = theme.icon_glyph_size_sm;
        icon.image(size.value(), c).paint_at(
            ui,
            egui::Rect::from_center_size(r.center(), egui::vec2(size.value(), size.value())),
        );
    };
    // Keep the real buffer intact. The idle label alone is shortened, so focusing/copying
    // always sees the complete address and agent updates cannot rewrite an active draft.
    let idle = !state.editing && !had_focus && !view.focus;
    let tag_width = view
        .skipped
        .map_or(0.0, |(label, _)| crate::tag_width(ui, theme, label));
    // The marker's visible share inside the content box; its last `space-xs` sits in the padding.
    let marker_width = marker_rect.map_or(0.0, |m| m.width() - theme.spacing_xs.value());
    let between = if tag_width > 0.0 && marker_width > 0.0 {
        theme.input_gap().value()
    } else {
        0.0
    };
    let trailing_box = tag_width + between + marker_width;
    let trailing = if trailing_box > 0.0 {
        LogicalPx(trailing_box) + theme.input_gap()
    } else {
        LogicalPx(0.0)
    };
    let text_width = field_width
        - theme.input_padding_x() * 2.0
        - theme.icon_glyph_size_sm
        - theme.input_gap()
        - trailing;
    let paint_tag = |ui: &mut egui::Ui, rect: egui::Rect, _| {
        if let Some((label, tooltip)) = view.skipped {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let response = crate::tag(&mut child, theme, label, crate::TagVariant::Default, false);
            if tooltip_hover_delay_elapsed(ui.ctx(), theme, response.id, response.hovered()) {
                Tooltip::new(tooltip)
                    .placement(TooltipPlacement::Top)
                    .id_source(response.id)
                    .show(ui, theme, response.rect);
            }
        }
    };
    let mut display = if idle {
        left_ellipsis(ui, theme, &state.buffer, text_width)
    } else {
        state.buffer.clone()
    };
    let mut input = Input::new()
        .id(field_id)
        .width(field_width.value())
        .mono(!display.is_empty())
        .font_size(theme.input_font_size())
        .icon_size(theme.icon_glyph_size_sm)
        .text_margin(LogicalPx(0.0))
        .placeholder(view.labels.placeholder)
        .icon(&paint)
        .read_only(mirror)
        .invalid(state.error.is_some())
        .focus_look(state.editing)
        // Idle shows the real address in its own value ink; a read-only mirror keeps the
        // Input read-only ink, and placeholder/disabled keep the Input roles.
        .text_color(
            if state.editing {
                theme.input_fg()
            } else {
                theme.html_address_value_fg()
            }
            .to_egui(),
        );
    if trailing_box > 0.0 {
        input = input.trailing_box(&paint_tag, LogicalPx(trailing_box), theme.input_height());
    }
    let response = input.show(&mut field, theme, &mut display);
    out.field_rect = Some(field_rect);
    if let (Some(marker), Some(m)) = (view.remote_marker, marker_rect) {
        let mut cell = field.new_child(egui::UiBuilder::new().id_salt("remote_marker").max_rect(m));
        out.remote_marker_clicked =
            crate::html_remote_marker(&mut cell, theme, m, &marker).clicked();
        out.remote_marker_rect = Some(m);
    }
    if view.focus && !mirror && ui.is_enabled() {
        select_all(ui, field_id, state.buffer.chars().count());
    }
    if response.changed() && !mirror && !idle {
        state.buffer = display;
        state.error = None;
    }
    let focused = ui.memory(|m| m.has_focus(field_id));
    if !mirror {
        if (had_focus || focused || was_focused) && escape {
            state.revert(view.committed_address);
            response.surrender_focus();
            out.action = HtmlAddressAction::Revert;
        } else if had_focus && enter {
            state.editing = true;
            response.request_focus();
            out.action = HtmlAddressAction::Open(state.buffer.clone());
        } else if focused {
            state.editing = true;
        } else if response.lost_focus() && state.error.is_none() {
            state.revert(view.committed_address);
        }
    }
    focusable.push(field_id);
    let band_focus = cycle_band_focus(
        ui,
        state,
        field_id,
        mirror,
        focused_at_start,
        &focusable,
        view.committed_address,
    );
    let field_focused = ui.memory(|m| m.has_focus(field_id));
    out.has_keyboard_focus = band_focus.is_some();
    ui.data_mut(|d| {
        d.insert_temp(field_id.with("previous_focus"), field_focused);
        d.insert_temp(id.with("focusable"), focusable);
    });
    if let Some(error) = &state.error {
        error_row(ui, theme, rect, error);
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - theme.border_width.value() / 2.0,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.html_address_bar_border().to_egui_premultiplied(),
        ),
    );
    out
}

/// The error row under the field row: alert glyph and one truncated caption line.
fn error_row(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, error: &str) {
    let row_height = theme.html_address_bar_height();
    let top = LogicalPx(rect.top()) + row_height;
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                egui::pos2(
                    (LogicalPx(rect.left()) + theme.html_address_bar_padding_x()).value(),
                    top.value(),
                ),
                egui::pos2(
                    (LogicalPx(rect.right()) - theme.html_address_bar_padding_x()).value(),
                    rect.bottom(),
                ),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.set_clip_rect(ui.clip_rect().intersect(rect));
    row.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    row.add(tasty_icons::ALERT_CIRCLE.image(
        theme.icon_glyph_size_sm.value(),
        theme.html_address_error_glyph().to_egui(),
    ));
    row.add(
        egui::Label::new(
            egui::RichText::new(error)
                .size(theme.font_size_caption.value())
                .color(theme.text_primary().to_egui()),
        )
        .truncate(),
    );
}

/// Selects the whole address when the band takes focus from a shortcut.
fn select_all(ui: &egui::Ui, field_id: egui::Id, chars: usize) {
    if let Some(mut edit) = egui::TextEdit::load_state(ui.ctx(), field_id) {
        edit.cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(chars),
            )));
        edit.store(ui.ctx(), field_id);
    }
}

/// Tab and Shift+Tab cycle through the enabled buttons and the field inside the band
/// instead of leaving for other host widgets, so keys keep reaching the band. Returns the
/// band widget holding keyboard focus.
fn cycle_band_focus(
    ui: &egui::Ui,
    state: &mut HtmlAddressState,
    field_id: egui::Id,
    mirror: bool,
    focused_at_start: Option<egui::Id>,
    focusable: &[egui::Id],
    committed_address: &str,
) -> Option<egui::Id> {
    let tab = ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key: egui::Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.ctrl && !modifiers.command && !modifiers.mac_cmd && !modifiers.alt => {
                Some(modifiers.shift)
            }
            _ => None,
        })
    });
    let from = focused_at_start.and_then(|f| focusable.iter().position(|b| *b == f));
    if let (Some(back), Some(from)) = (tab, from) {
        let n = focusable.len();
        let next = focusable[if back {
            (from + n - 1) % n
        } else {
            (from + 1) % n
        }];
        ui.memory_mut(|m| m.request_focus(next));
        // Focus moves within this frame, so the field never sees lost_focus. Leave it the
        // way losing focus elsewhere does: an unrejected draft returns to the address.
        if focusable[from] == field_id && next != field_id && !mirror && state.error.is_none() {
            state.revert(committed_address);
        }
        if next == field_id && !mirror {
            state.editing = true;
            select_all(ui, field_id, state.buffer.chars().count());
        }
        // The lock filter below applies only after a frame of focus. Draw that frame now so
        // the next Tab is not taken by egui's own traversal out of the band.
        ui.ctx().request_repaint();
    }
    let band_focus = ui.memory(|m| m.focused().filter(|f| focusable.contains(f)));
    if let Some(f) = band_focus {
        // Every band widget keeps Tab for the cycle above and the arrows, so egui's own
        // traversal never moves focus to a host widget outside the band.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                f,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        });
    }
    band_focus
}

/// Back, Forward and Reload cells from the left padding. Returns the x after the last cell
/// and its nav gap, with the ids of the enabled buttons in order. Loading shows a Spinner
/// in the Reload cell instead of the button.
fn nav_buttons(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: egui::Id,
    rect: egui::Rect,
    view: &HtmlAddressView<'_>,
    forward: bool,
    out: &mut HtmlAddressOutput,
) -> (LogicalPx, Vec<egui::Id>) {
    let mirror = view.kind == HtmlAddressKind::Mirror;
    let row_height = theme.html_address_bar_height();
    let side = LogicalPx(ControlSize::Sm.height(theme));
    let mut x = LogicalPx(rect.left()) + theme.html_address_bar_padding_x();
    let y = LogicalPx(rect.top()) + (row_height - side) / 2.0;
    let mut focusable = Vec::new();
    let buttons = [
        (
            tasty_icons::CHEVRON_LEFT,
            !mirror,
            view.can_back,
            view.labels.back,
            HtmlAddressAction::Back,
        ),
        (
            tasty_icons::CHEVRON_RIGHT,
            forward,
            view.can_forward,
            view.labels.forward,
            HtmlAddressAction::Forward,
        ),
        (
            tasty_icons::REFRESH,
            true,
            !view.disconnected,
            if view.disconnected {
                view.labels.disconnected
            } else {
                view.labels.reload
            },
            HtmlAddressAction::Reload,
        ),
    ];
    for (icon, visible, enabled, label, action) in buttons {
        if !visible {
            continue;
        }
        let cell = egui::Rect::from_min_size(
            egui::pos2(x.value(), y.value()),
            egui::vec2(side.value(), side.value()),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt((id, label)).max_rect(cell));
        child.set_clip_rect(ui.clip_rect().intersect(rect));
        if action == HtmlAddressAction::Forward {
            out.forward_rect = Some(cell);
        }
        if action == HtmlAddressAction::Reload {
            out.reload_rect = Some(cell);
        }
        if action == HtmlAddressAction::Reload && view.loading {
            let spin = theme.icon_glyph_size_sm;
            Spinner::new().paint_in(
                &mut child,
                theme,
                egui::Rect::from_center_size(cell.center(), egui::vec2(spin.value(), spin.value())),
            );
        } else {
            let resp = nav(&mut child, theme, icon, enabled, label);
            if enabled {
                focusable.push(resp.id);
            }
            if resp.clicked() {
                out.action = action;
            }
        }
        x += side + theme.html_address_bar_nav_gap();
    }
    (x, focusable)
}
