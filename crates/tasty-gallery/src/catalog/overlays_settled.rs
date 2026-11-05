//! 창·팝업의 결정 기록 Spec: 플러그인 식별 표시, Diff 도구 막대, 드래그 손잡이.
//! 시안 `overlays-windows.jsx`의 같은 Spec을 옮긴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, PluginAvatarSize,
    paint_plugin_avatar, plugin_avatar,
};

use crate::catalog::icons;
use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage};
use crate::catalog::widgets::dialog as kit;

/// 플러그인 목록 행 예제의 폭.
const PLUGIN_ROW_W: LogicalPx = LogicalPx(260.0);
/// Diff 도구 막대 예제 카드의 폭.
const DIFF_CARD_W: LogicalPx = LogicalPx(460.0);
/// 드래그 손잡이 예제 팝업의 폭과 본문 높이.
const FAUX_POPUP_W: LogicalPx = LogicalPx(240.0);
const FAUX_REGION_W: LogicalPx = LogicalPx(260.0);
const FAUX_BODY_H: LogicalPx = LogicalPx(150.0);

#[inline]
fn ec(c: impl Into<egui::Color32>) -> egui::Color32 {
    c.into()
}

fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
        .rect
        .width()
}

/// 가로 배치 안에서 폭 `w`의 열을 만들고 내용을 가운데 정렬한다.
/// `vertical_centered`는 남은 폭 전체를 차지하므로 가로 배치 안에서는 쓰지 않는다.
fn centered_column(ui: &mut egui::Ui, w: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(w, 0.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.set_width(w);
            add(ui);
        },
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowState {
    Rest,
    Hover,
    Selected,
}

/// 목록 행 한 줄. 표시의 색은 행 상태와 무관하게 고정된 바탕 위에 섞인다.
fn plugin_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    meta_text: &str,
    state: RowState,
    disabled: bool,
) {
    let pad = theme.spacing_sm.value();
    let avatar = PluginAvatarSize::Row.side().value();
    let h = avatar + pad * 2.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(PLUGIN_ROW_W.value(), h), egui::Sense::hover());
    let p = ui.painter().clone();
    let r = theme.corner_radius.value();
    match state {
        RowState::Rest => {}
        RowState::Hover => {
            p.rect_filled(rect, r, theme.overlay_hover().to_egui_premultiplied());
        }
        RowState::Selected => {
            p.rect_filled(rect, r, ec(theme.surface_active()));
            let edge = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(theme.selection_edge_width.value(), rect.height()),
            );
            p.rect_filled(edge, 0.0, ec(theme.accent_primary()));
        }
    }
    paint_plugin_avatar(
        &p,
        theme,
        egui::pos2(rect.left() + pad + avatar * 0.5, rect.center().y),
        name,
        PluginAvatarSize::Row,
    );
    let x = rect.left() + pad + avatar + pad;
    let (name_fg, meta_fg) = if matches!(state, RowState::Selected) {
        (theme.text_on_active(), theme.text_on_active())
    } else if disabled {
        (theme.text_muted(), theme.text_muted())
    } else {
        (theme.text_primary(), theme.text_muted())
    };
    p.text(
        egui::pos2(x, rect.center().y - theme.spacing_xs.value() * 0.5),
        egui::Align2::LEFT_BOTTOM,
        name,
        egui::FontId::proportional(theme.font_size_body.value()),
        ec(name_fg),
    );
    p.text(
        egui::pos2(x, rect.center().y + theme.spacing_xs.value() * 0.5),
        egui::Align2::LEFT_TOP,
        meta_text,
        egui::FontId::monospace(theme.font_size_micro.value()),
        ec(meta_fg),
    );
}

/// Overlays › Plugins window — 플러그인 식별 표시의 두 크기.
pub fn draw_plugin_identity(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(ec(theme.bg_panel()))
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                        for (size, label) in [
                            (PluginAvatarSize::Row, "32 · list row"),
                            (PluginAvatarSize::Detail, "46 · detail / preview"),
                        ] {
                            let font = egui::FontId::proportional(theme.font_size_caption.value());
                            let w = text_width(ui, label, font).max(size.side().value());
                            centered_column(ui, w, |ui| {
                                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                                plugin_avatar(ui, theme, "git-helper", size);
                                ui.label(
                                    egui::RichText::new(label)
                                        .size(theme.font_size_caption.value())
                                        .color(ec(theme.text_muted())),
                                );
                            });
                        }
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                        plugin_row(
                            ui,
                            theme,
                            "git-helper",
                            "tasty-labs · v1.4.2",
                            RowState::Rest,
                            false,
                        );
                        plugin_row(
                            ui,
                            theme,
                            "ai-review",
                            "tasty-labs · v0.9.0",
                            RowState::Hover,
                            false,
                        );
                        plugin_row(
                            ui,
                            theme,
                            "docker",
                            "community · v2.1.0",
                            RowState::Selected,
                            false,
                        );
                        plugin_row(
                            ui,
                            theme,
                            "vim-mode",
                            "ophen · v3.2.1 · disabled",
                            RowState::Rest,
                            true,
                        );
                    });
                });
            });
    });
    meta(
        ui,
        theme,
        &[
            ("sizes", "sm 32 · lg 46 — the whole roster"),
            ("content", "name initial, uppercase, mono regular"),
            ("initial size", "sm 13 · lg 14 — no UI-cap exception"),
            ("tint bed", "surface-raised — fixed, not the row bg"),
            ("mix", "bg 18% · border 38% of accent-primary"),
            ("row states", "mark unchanged — only the row bg moves"),
            ("disabled row", "whole row at state-disabled-opacity"),
        ],
        &[
            TokenChip::without_color("plugin-avatar-size-sm", "32 — list row"),
            TokenChip::without_color("plugin-avatar-size-lg", "46 — detail / manifest preview"),
            TokenChip::new(
                "plugin-avatar-bg",
                "tinted square",
                ec(theme.plugin_avatar_bg()),
            ),
            TokenChip::new(
                "plugin-avatar-border",
                "1px edge",
                ec(theme.plugin_avatar_border()),
            ),
            TokenChip::new(
                "plugin-avatar-fg",
                "the initial",
                ec(theme.plugin_avatar_fg()),
            ),
            TokenChip::without_color("plugin-avatar-initial-font-size-sm", "→ font-size-body 13"),
            TokenChip::without_color("plugin-avatar-initial-font-size-lg", "→ font-size-max 14"),
            TokenChip::without_color("plugin-avatar-initial-weight", "→ font-weight-normal"),
            TokenChip::without_color("plugin-avatar-border-width", "= border-width"),
        ],
    );
    note(
        ui,
        theme,
        "The mark carries identity, not classification. A manifest has no category field, so the colour is fixed at plugin-avatar-fg for every plugin; state (running / error / needs attention) is carried by the row's status dot and callouts.",
    );
    dont(
        ui,
        theme,
        "Don't mix the tint into the row background to \"blend\" on a selected row — the mark would then shift colour with row state and stop being a stable identity. And don't re-derive the initial's size from the box (round(size × 0.42)): that produced 19px at lg, off the type scale and over the UI cap with no decision behind it.",
    );
}

/// Plugins › Git viewer — 도구 막대는 컨트롤 높이가 아니라 컨테이너 높이다.
pub fn draw_diff_toolbar(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Center, |ui| {
        kit::frame_card_flat(ui, theme, DIFF_CARD_W, ec(theme.bg_panel()), |ui| {
            let (bar, _) = ui.allocate_exact_size(
                egui::vec2(DIFF_CARD_W.value(), theme.git_toolbar_height().value()),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(bar, 0.0, ec(theme.bg_sidebar()));
            ui.painter().hline(
                bar.x_range(),
                bar.bottom(),
                egui::Stroke::new(theme.border_width.value(), ec(theme.border_default())),
            );
            let mut row = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar.shrink2(egui::vec2(theme.spacing_sm.value(), 0.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            row.spacing_mut().item_spacing.x = theme.spacing_xs.value();
            for label in ["Unified", "Split"] {
                Button::new(label)
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(&mut row, theme);
            }
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("+128 −44")
                        .monospace()
                        .size(theme.font_size_caption.value())
                        .color(ec(theme.text_muted())),
                );
            });
            kit::region_sym(ui, theme.spacing_md, theme.spacing_md, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                for (line, color) in [
                    ("@@ -12,7 +12,9 @@ fn draw_path_bar()", theme.text_muted()),
                    (
                        "+    let avail = bar_w - buttons_w - gap;",
                        theme.accent_success(),
                    ),
                    ("-    let avail = popup_w;", theme.accent_danger()),
                ] {
                    ui.label(
                        egui::RichText::new(line)
                            .monospace()
                            .size(theme.font_size_caption.value())
                            .color(ec(color)),
                    );
                }
            });
        });
    });
    meta(
        ui,
        theme,
        &[
            ("height", "32 — unchanged"),
            ("token", "git-toolbar-height"),
            ("holds", "28px controls + 2px air"),
            ("not", "control-height (28, a control's own box)"),
        ],
        &[
            TokenChip::without_color("git-toolbar-height", "toolbar container"),
            TokenChip::without_color("control-height", "the buttons inside it"),
        ],
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    TitleBar,
    Region,
    None,
}

/// 손잡이 영역에 포인터가 올라오면 grab 커서를 보인다. 칠한 손잡이는 없다.
fn grab_zone(ui: &egui::Ui, rect: egui::Rect, id: egui::Id) {
    if ui.interact(rect, id, egui::Sense::hover()).hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
}

fn close_button(ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
    IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(ui, theme, &|ui, rect, c| {
            icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
        })
}

fn faux_popup(ui: &mut egui::Ui, theme: &Theme, handle: Handle, title: &str, body: &str) {
    let w = if handle == Handle::Region {
        FAUX_REGION_W
    } else {
        FAUX_POPUP_W
    };
    kit::frame_card(ui, theme, w, ec(theme.surface_raised()), |ui| {
        let (bar, _) = ui.allocate_exact_size(
            egui::vec2(w.value(), theme.titlebar_height.value()),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(bar, 0.0, ec(theme.bg_app()));
        ui.painter().hline(
            bar.x_range(),
            bar.bottom(),
            egui::Stroke::new(theme.border_width.value(), ec(theme.border_default())),
        );
        let inner = bar.shrink2(egui::vec2(theme.spacing_xs.value(), 0.0));
        match handle {
            Handle::TitleBar | Handle::None => {
                ui.painter().text(
                    bar.center(),
                    egui::Align2::CENTER_CENTER,
                    title,
                    egui::FontId::proportional(theme.font_size_term_sm.value()),
                    ec(theme.titlebar_fg()),
                );
                let mut right = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                let close = close_button(&mut right, theme);
                if handle == Handle::TitleBar {
                    // 닫기 버튼은 손잡이에서 뺀다.
                    let zone =
                        egui::Rect::from_min_max(bar.min, egui::pos2(close.rect.left(), bar.max.y));
                    grab_zone(ui, zone, ui.id().with("titlebar_grab"));
                }
            }
            Handle::Region => {
                let mut row = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                row.spacing_mut().item_spacing.x = theme.spacing_sm.value() * 0.75;
                let band_start = row.cursor().left();
                kit::icon(
                    &mut row,
                    icons::PORT,
                    theme.icon_glyph_size_md,
                    ec(theme.text_muted_raised()),
                );
                row.label(
                    egui::RichText::new("Ports")
                        .size(theme.font_size_term_sm.value())
                        .color(ec(theme.text_secondary_raised())),
                );
                row.add_space(theme.spacing_sm.value());
                let band = egui::Rect::from_min_max(
                    egui::pos2(band_start, bar.top()),
                    egui::pos2(row.cursor().left(), bar.bottom()),
                );
                grab_zone(ui, band, ui.id().with("region_grab"));
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close_button(ui, theme);
                    kit::field(ui, theme, None, "filter…", true, false);
                });
            }
        }
        let (body_rect, _) = ui.allocate_exact_size(
            egui::vec2(w.value(), FAUX_BODY_H.value()),
            egui::Sense::hover(),
        );
        let mut body_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body_rect.shrink(theme.spacing_md.value()))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        body_ui.add(
            egui::Label::new(
                egui::RichText::new(body)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(ec(theme.text_muted_raised())),
            )
            .wrap(),
        );
    });
}

/// Overlays › Move & resize — 팝업을 옮기는 세 방식. 표시는 커서뿐이다.
pub fn draw_drag_handles(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for (i, (caption, handle, title, body)) in [
            (
                "TitleBar — whole bar drags",
                Handle::TitleBar,
                "Listening ports",
                "title centered · close excluded from the handle",
            ),
            (
                "Region — header band drags, widgets win",
                Handle::Region,
                "",
                "left band = grab · search/close = their own cursor",
            ),
            (
                "None — not movable",
                Handle::None,
                "apply preset",
                "no handle · grab cursor never appears",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            ui.push_id(i, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    kit::caption(ui, theme, caption, false);
                    faux_popup(ui, theme, handle, title, body);
                });
            });
        }
    });
    meta(
        ui,
        theme,
        &[
            ("TitleBar", "whole bar drags, close excluded"),
            ("Region", "header left band (headless popups)"),
            ("None", "immovable"),
            ("affordance", "cursor only — no painted grip"),
            (
                "priority",
                "widget > close > resize edge > drag handle > content",
            ),
            ("start gate", "after content render — widget pointer wins"),
        ],
        &[
            TokenChip::new("bg-app", "titlebar fill (mantle)", ec(theme.bg_app())),
            TokenChip::new("surface-raised", "popup body", ec(theme.surface_raised())),
            TokenChip::without_color("titlebar-height", "bar / band height"),
        ],
    );
    dont(
        ui,
        theme,
        "Don't tint or outline the drag band over a widget — the widget always wins the press, so a visual grip there only invites mis-clicks. Let the cursor carry the affordance.",
    );
}
