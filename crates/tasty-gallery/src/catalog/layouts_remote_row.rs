//! Layouts › Saved remote workspace. 시안 `layouts/remote-row.jsx`의 네 Spec 을 옮긴다.
//! 연결 팝업 콘텐츠, 문구 선택, REMOTE 줄, 레일 칩은 본체와 같은 `tasty_ui_widgets` 함수로 그린다.
//! 마지막 화면(탭 바·pane)은 무대용 모형이다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    REMOTE_ROW_POPUP_WIDTH, REMOTE_ROW_USER_BODY_NEXT, REMOTE_ROW_USER_BODY_STAY,
    RemoteRowPopupBody, RemoteRowPopupKind, RemoteRowPopupView, RemoteRowReason, RemoteRowState,
    Spinner, attach_refusal_mark, paint_attach_refusal_chip, paint_workspace_mirror_chip,
    paint_workspace_mirror_chip_offline, remote_row_popup, remote_row_popup_copy,
    remote_row_state_word_key, remote_row_tag_line,
};

use crate::catalog::icons::{ALERT_TRIANGLE, REMOTE};
use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};
use crate::catalog::widgets::dialog as kit;
use crate::catalog::{Section, Spec};
use crate::i18n::{t_lang, translated_languages};

/// 시안 무대 크기(`size-480` × `size-288`, 좁은 무대 `size-320`).
const FRAME_W: LogicalPx = LogicalPx(480.0);
const FRAME_H: LogicalPx = LogicalPx(288.0);
const NARROW_W: LogicalPx = LogicalPx(320.0);
/// 행 무대 폭(`size-240`)과 레일 아바타(`size-36`), 레일 칸 폭(`size-64`).
const ROWS_W: LogicalPx = LogicalPx(240.0);
const AVATAR: LogicalPx = LogicalPx(36.0);
const RAIL_CELL_W: LogicalPx = LogicalPx(64.0);
/// 첫 프레임의 팝업 높이 어림. 이후 프레임은 잰 값을 쓴다.
const POPUP_H_GUESS: LogicalPx = LogicalPx(160.0);

const NAME: &str = "tasty-core";
const PROFILE: &str = "gx10";

fn langs() -> impl Iterator<Item = &'static str> {
    std::iter::once("en").chain(translated_languages())
}

pub fn section() -> Section {
    Section {
        id: "remoteworkspace",
        title: "Saved remote workspace",
        specs: vec![
            Spec {
                id: "remote-row-popup-on-the-bare-workspace",
                title: "Not connected yet — the bare workspace and its connection popup",
                when: Some(
                    "after a restart the row has no tabs · the popup is the only place that says how it stands · Workspace-scope, never takes focus, no close button",
                ),
                draw: draw_bare,
            },
            Spec {
                id: "remote-row-popup-over-the-last-screen",
                title: "Dropped while running — the last screen under a scrim, the same popup",
                when: Some(
                    "only a row connected in this run keeps its last screen · the scrim takes the pointer (R-58 Q1) · closed elsewhere keeps the row with Reconnect disabled",
                ),
                draw: draw_last_screen,
            },
            Spec {
                id: "remote-row-marks-sidebar-and-rail",
                title: "Row and rail — the connection state at a glance",
                when: Some(
                    "REMOTE tag stays · off the live state the tag turns neutral and a status word follows · spinner while trying · warning mark on a reason",
                ),
                draw: draw_marks,
            },
            Spec {
                id: "remote-row-user-pending-l1-l4",
                title: "Waiting on the user — L1 to L4",
                when: Some(
                    "L1 and L4 change the first popup, so both answers are drawn · the app builds answer a until the user decides",
                ),
                draw: draw_pending,
            },
        ],
    }
}

#[derive(Clone, Copy)]
struct Pop {
    lang: &'static str,
    kind: RemoteRowPopupKind,
    reason: Option<RemoteRowReason>,
    user_body: &'static str,
    remote: &'static str,
    shape: bool,
}

impl Pop {
    fn new(kind: RemoteRowPopupKind, shape: bool) -> Self {
        Self {
            lang: "en",
            kind,
            reason: None,
            user_body: REMOTE_ROW_USER_BODY_STAY,
            remote: NAME,
            shape,
        }
    }
    fn reason(mut self, reason: RemoteRowReason) -> Self {
        self.reason = Some(reason);
        self
    }
    fn lang(mut self, lang: &'static str) -> Self {
        self.lang = lang;
        self
    }
}

fn fill(text: &str, arg: &str) -> String {
    text.replacen("{}", arg, 1)
}

fn popup_content(ui: &mut egui::Ui, theme: &Theme, pop: Pop) {
    let tx = |key: &'static str| t_lang(pop.lang, key);
    let copy = remote_row_popup_copy(pop.kind, pop.shape, pop.user_body);
    let reason = if pop.kind == RemoteRowPopupKind::Closed {
        Some(RemoteRowReason::WorkspaceMissing)
    } else {
        pop.reason
    };
    let profile = if reason == Some(RemoteRowReason::ProfileMissing) {
        "gx10-old"
    } else {
        PROFILE
    };
    let body = match copy.body {
        RemoteRowPopupBody::None => None,
        RemoteRowPopupBody::Seconds(key) => Some(fill(tx(key), "300")),
        RemoteRowPopupBody::Plain(key) => Some(tx(key).to_owned()),
        RemoteRowPopupBody::Reason => reason.map(|r| {
            fill(
                tx(r.key()),
                if r.names_profile() {
                    profile
                } else {
                    pop.remote
                },
            )
        }),
    };
    let remote_glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        REMOTE.image(rect.height(), c).paint_at(ui, rect);
    };
    let warn_glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        ALERT_TRIANGLE.image(rect.height(), c).paint_at(ui, rect);
    };
    remote_row_popup(
        ui,
        theme,
        &RemoteRowPopupView {
            name: NAME,
            profile_label: tx("remote.row.popup.profile"),
            profile,
            remote_label: tx("remote.row.popup.remote_workspace"),
            remote: pop.remote,
            title: copy.title.map(tx),
            body: body.as_deref(),
            button: tx(copy.button),
            copy,
            remote_glyph: &remote_glyph,
            warn_glyph: &warn_glyph,
        },
    );
}

/// 마지막 화면 모형: 탭 바 두 탭과 pane 두 개.
fn paint_last_screen(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let p = ui.painter_at(rect);
    let tab_h = theme.tab_height().value();
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), tab_h));
    p.rect_filled(bar, 0.0, theme.bg_sidebar().to_egui());
    let caption = egui::FontId::proportional(theme.font_size_caption.value());
    let mut x = bar.left();
    for (i, tab) in ["build", "logs"].iter().enumerate() {
        let galley = p.layout_no_wrap(
            (*tab).to_owned(),
            caption.clone(),
            if i == 0 {
                theme.text_primary().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );
        let w = galley.size().x + theme.spacing_md.value() * 2.0;
        let cell = egui::Rect::from_min_size(egui::pos2(x, bar.top()), egui::vec2(w, tab_h));
        if i == 0 {
            p.rect_filled(cell, 0.0, theme.tab_bg_active().to_egui());
        }
        p.galley(
            egui::pos2(
                cell.left() + theme.spacing_md.value(),
                cell.center().y - galley.size().y * 0.5,
            ),
            galley,
            egui::Color32::PLACEHOLDER,
        );
        x += w;
    }
    let sep = theme.tab_separator().to_egui_premultiplied();
    p.hline(
        bar.x_range(),
        bar.bottom(),
        egui::Stroke::new(theme.border_width.value(), sep),
    );
    let body = egui::Rect::from_min_max(egui::pos2(rect.left(), bar.bottom()), rect.max);
    let mid = body.center().x;
    let mono = egui::FontId::monospace(theme.font_size_caption.value());
    let panes: [(egui::Rect, egui::Color32, &[&str]); 2] = [
        (
            egui::Rect::from_min_max(body.min, egui::pos2(mid, body.bottom())),
            theme.surface_bed_focused().to_egui(),
            &[
                "$ cargo build --release",
                "   Compiling tasty-core v0.9.0",
                "   Compiling tasty-remote v0.9.0",
                "error: mismatched types at src/main.rs:42",
            ],
        ),
        (
            egui::Rect::from_min_max(egui::pos2(mid, body.top()), body.max),
            theme.surface_bed_unfocused().to_egui(),
            &[
                "$ tail -f deploy.log",
                "12:04:31 rollout 3/5",
                "12:04:40 rollout 4/5",
            ],
        ),
    ];
    for (pane, bed, lines) in panes {
        p.rect_filled(pane, 0.0, bed);
        let mut y = pane.top() + theme.spacing_sm.value();
        for line in lines {
            let g = p.layout_no_wrap(
                (*line).to_owned(),
                mono.clone(),
                theme.text_primary().to_egui(),
            );
            let h = g.size().y;
            p.galley(
                egui::pos2(pane.left() + theme.spacing_sm.value(), y),
                g,
                egui::Color32::PLACEHOLDER,
            );
            y += h;
        }
    }
    p.vline(
        mid,
        body.y_range(),
        egui::Stroke::new(theme.border_width.value(), sep),
    );
}

/// workspace 콘텐츠 영역 무대. 형상이 있으면 마지막 화면 위에 scrim 을 덮는다. 팝업은 가운데다.
fn frame(ui: &mut egui::Ui, theme: &Theme, width: LogicalPx, pop: Pop) {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width.value(), FRAME_H.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, theme.corner_radius.value(), theme.bg_app().to_egui());
    if pop.shape {
        paint_last_screen(ui, theme, rect.shrink(theme.border_width.value()));
        p.rect_filled(rect, 0.0, theme.scrim().to_egui());
    }
    p.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
        egui::StrokeKind::Inside,
    );
    let md = theme.spacing_md.value();
    let w = (REMOTE_ROW_POPUP_WIDTH.value() * theme.ui_zoom)
        .round()
        .min(rect.width() - md * 2.0);
    let h_id = resp.id.with("popup_h");
    let h = ui
        .data(|d| d.get_temp::<f32>(h_id))
        .unwrap_or(POPUP_H_GUESS.value());
    let top = rect.center().y - h * 0.5;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
        egui::pos2(rect.center().x - w * 0.5, top),
        egui::vec2(w, rect.height()),
    )));
    let card = |ui: &mut egui::Ui| popup_content(ui, theme, pop);
    let fill = theme.bg_panel().to_egui();
    if pop.shape {
        kit::frame_card(&mut child, theme, LogicalPx(w), fill, card);
    } else {
        kit::frame_card_popover(&mut child, theme, LogicalPx(w), fill, card);
    }
    let measured = child.min_rect().height();
    if (measured - h).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(h_id, measured));
        ui.ctx().request_repaint();
    }
}

fn labelled(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    cluster(ui, theme, label, add);
}

fn draw_bare(ui: &mut egui::Ui, theme: &Theme) {
    use RemoteRowPopupKind as K;
    stage(ui, theme, StageVariant::Wrap, |ui| {
        labelled(
            ui,
            theme,
            "connecting — opened by the user (or right after a restart, L4 a)",
            |ui| {
                frame(ui, theme, FRAME_W, Pop::new(K::Connecting, false));
            },
        );
        labelled(
            ui,
            theme,
            "failed — the remote workspace is in use by another client",
            |ui| {
                frame(
                    ui,
                    theme,
                    FRAME_W,
                    Pop::new(K::Failed, false).reason(RemoteRowReason::Occupied),
                );
            },
        );
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for reason in [
            RemoteRowReason::ProfileMissing,
            RemoteRowReason::WorkspaceMissing,
            RemoteRowReason::Occupied,
            RemoteRowReason::Chain,
            RemoteRowReason::Endpoint,
            RemoteRowReason::SelfInstance,
        ] {
            labelled(ui, theme, &format!("reason: {reason:?}"), |ui| {
                frame(
                    ui,
                    theme,
                    FRAME_W,
                    Pop::new(K::Failed, false).reason(reason),
                );
            });
        }
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for lang in langs() {
            labelled(ui, theme, &format!("{lang} · connecting"), |ui| {
                frame(
                    ui,
                    theme,
                    FRAME_W,
                    Pop::new(K::Connecting, false).lang(lang),
                );
            });
            labelled(ui, theme, &format!("{lang} · failed · chain"), |ui| {
                frame(
                    ui,
                    theme,
                    FRAME_W,
                    Pop::new(K::Failed, false)
                        .reason(RemoteRowReason::Chain)
                        .lang(lang),
                );
            });
        }
        labelled(
            ui,
            theme,
            "narrow workspace — the width less space-md each side",
            |ui| {
                let mut pop = Pop::new(K::Failed, false).reason(RemoteRowReason::WorkspaceMissing);
                pop.remote = "release-pipeline-watch-eu-west";
                frame(ui, theme, NARROW_W, pop);
            },
        );
    });
    meta(
        ui,
        theme,
        &[
            (
                "ground",
                "the workspace content area in bg-app — no tab bar, no pane, no scrim",
            ),
            (
                "popup",
                "Workspace-scope, centred in the content area · 360 wide, at most the area less space-md each side · no title bar, no ×, no drag",
            ),
            (
                "frame",
                "bg-panel · 1px border-strong · radius · shadow-popover (no scrim under it)",
            ),
            (
                "inset · gaps",
                "space-md all round and between blocks · space-xs between the two fields",
            ),
            (
                "head",
                "remote glyph 16 in accent-remote-raised + the row name, body semibold, end-elided",
            ),
            (
                "fields",
                "Profile / Remote workspace — caption labels text-muted · values mono caption text-secondary, end-elided",
            ),
            (
                "state block",
                "title body semibold text-primary + body text-secondary · warning glyph 14 (accent-warning-glyph) only for failed, timed out and closed elsewhere",
            ),
            (
                "action",
                "one primary sm button, right: Connect · while trying: disabled, Spinner 12, Connecting…",
            ),
            (
                "close",
                "not in the popup — Close locally is in the row menu",
            ),
            (
                "focus",
                "never moved: an agent that starts a connection changes the row mark only",
            ),
        ],
        &[
            TokenChip::new("bg-panel", "popup", theme.bg_panel().to_egui()),
            TokenChip::new(
                "border-strong",
                "popup edge",
                theme.border_strong().to_egui(),
            ),
            TokenChip::without_color("shadow-popover", "popup lift, no scrim"),
            TokenChip::new(
                "accent-remote-raised",
                "head glyph",
                theme.accent_remote_raised().to_egui(),
            ),
            TokenChip::new(
                "accent-warning-glyph",
                "failed glyph",
                theme.accent_warning_glyph().to_egui(),
            ),
            TokenChip::without_color("size-360", "popup width"),
        ],
    );
    note(
        ui,
        theme,
        "Copy keys are remote.row.popup.* and remote.row.reason.*. Connect is for a row that has not connected in this run; a row that had a screen says Reconnect, the child graph's word (child_graph.reconnect), because it is the same action.",
    );
    dont(
        ui,
        theme,
        "Don't put Close locally beside Connect. The popup is about the connection; a removal next to the main button turns a missed click into a lost row.",
    );
}

fn draw_last_screen(ui: &mut egui::Ui, theme: &Theme) {
    use RemoteRowPopupKind as K;
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, pop) in [
            (
                "reconnecting — automatic, up to the set time",
                Pop::new(K::Reconnecting, true),
            ),
            ("disconnected — the time ran out", Pop::new(K::GaveUp, true)),
            (
                "disconnected — the user chose Disconnect (L1 a)",
                Pop::new(K::User, true),
            ),
            (
                "failed Reconnect — the remote workspace is gone",
                Pop::new(K::Failed, true).reason(RemoteRowReason::WorkspaceMissing),
            ),
            (
                "closed elsewhere — another client or the remote Tasty closed the workspace",
                Pop::new(K::Closed, true),
            ),
        ] {
            labelled(ui, theme, label, |ui| frame(ui, theme, FRAME_W, pop));
        }
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for lang in langs() {
            for (word, kind) in [
                ("reconnecting", K::Reconnecting),
                ("timed out", K::GaveUp),
                ("closed elsewhere", K::Closed),
            ] {
                labelled(ui, theme, &format!("{lang} · {word}"), |ui| {
                    frame(ui, theme, FRAME_W, Pop::new(kind, true).lang(lang));
                });
            }
        }
    });
    meta(
        ui,
        theme,
        &[
            (
                "shown",
                "a row that was connected in this run and is not now — never after a restart",
            ),
            (
                "last screen",
                "drawn as it was, no blur, no change to terminal content (0 ms) · inert under the scrim (R-58 Q1)",
            ),
            (
                "cover",
                "scrim-bg clipped to the workspace content area (tab bar and panes) — sidebar, status bar and other windows stay clear",
            ),
            (
                "pointer",
                "the scrim takes it · no selection, copy, scroll, link or picker on the last screen while the row is not connected, reconnecting and resync included",
            ),
            (
                "popup",
                "same as on the bare workspace · shadow-modal, because it is centred over a scrim",
            ),
            (
                "reconnecting",
                "title Connection lost · body: the reconnect time in s · button disabled, Spinner 12, Reconnecting…",
            ),
            (
                "timed out",
                "warning glyph · Reconnection timed out · how long it tried · Reconnect",
            ),
            (
                "user disconnected",
                "no glyph · Disconnected · user_body_stay (L1 a) · Reconnect",
            ),
            (
                "failed",
                "warning glyph · Connection failed · reason · Reconnect",
            ),
            (
                "closed elsewhere",
                "warning glyph · remote.row.popup.closed_title · body remote.row.reason.workspace_missing · Reconnect disabled, not hidden",
            ),
            (
                "reconnect time 0",
                "no reconnecting state: the drop goes straight to timed out with 0 s",
            ),
            (
                "links, picker",
                "a link tooltip or picker open when the row drops (into reconnecting too) closes, and does not reopen when the row connects again (Q1-a)",
            ),
        ],
        &[
            TokenChip::new(
                "scrim-bg",
                "cover over the last screen",
                theme.scrim().to_egui(),
            ),
            TokenChip::without_color("shadow-modal", "popup over the scrim"),
        ],
    );
    note(
        ui,
        theme,
        "Toasts: a saved remote row shows no connection toasts (mirror_disconnected, mirror_reconnecting, mirror_reconnected, mirror_reconnect_giveup). The row mark and the popup tell it. An agent mirror does not reconnect, so it gets none of the three reconnect toasts; when it drops it only shows mirror_disconnected.",
    );
    dont(
        ui,
        theme,
        "Don't animate the cover or the screen under it. The only motion is the button's spinner while Tasty is trying.",
    );
}

/// 행 무대의 경우: 설명, 상태, 이유 유무, 실행 수.
const ROWS: [(&str, RemoteRowState, bool); 7] = [
    ("connected", RemoteRowState::Connected, false),
    ("connecting", RemoteRowState::Connecting, false),
    ("reconnecting", RemoteRowState::Reconnecting, false),
    (
        "disconnected (last screen kept)",
        RemoteRowState::Disconnected,
        false,
    ),
    (
        "not connected (after a restart)",
        RemoteRowState::NotConnected,
        false,
    ),
    ("disconnected + reason", RemoteRowState::Disconnected, true),
    ("not connected + reason", RemoteRowState::NotConnected, true),
];

fn row(
    ui: &mut egui::Ui,
    theme: &Theme,
    lang: &'static str,
    state: RemoteRowState,
    warn: bool,
    active: bool,
) {
    let fill = if active {
        theme.overlay_active().to_egui_premultiplied()
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(
            theme.workspace_row_padding_x().value() as i8,
            theme.spacing_xs.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ROWS_W.value() - theme.workspace_row_padding_x().value() * 2.0);
            ui.horizontal(|ui| {
                let slot = theme.workspace_dot_slot().value();
                let (dot, _) = ui.allocate_exact_size(egui::vec2(slot, slot), egui::Sense::hover());
                if state.attempting() {
                    Spinner::new()
                        .size(theme.icon_glyph_size_sm.value())
                        .paint_in(ui, theme, dot);
                } else {
                    let color = match (state == RemoteRowState::Connected, active) {
                        (true, true) => theme.status_dot_success_active(),
                        (true, false) => theme.status_dot_success(),
                        (false, true) => theme.status_dot_idle_active(),
                        (false, false) => theme.status_dot_idle(),
                    };
                    ui.painter().circle_filled(
                        dot.center(),
                        theme.badge_dot_size().value() * 0.5,
                        color.to_egui(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if warn {
                        attach_refusal_mark(ui, theme, t_lang(lang, "remote.row.reason.occupied"));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(NAME)
                                .size(theme.font_size_body.value())
                                .color(theme.text_secondary().to_egui()),
                        );
                    });
                });
            });
            ui.horizontal(|ui| {
                ui.add_space(theme.workspace_dot_slot().value() + ui.spacing().item_spacing.x);
                ui.spacing_mut().item_spacing.x = theme.workspace_mirror_gap().value();
                let glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                    REMOTE.image(rect.height(), c).paint_at(ui, rect);
                };
                let live = state == RemoteRowState::Connected;
                let word_color = if active {
                    theme.text_on_active()
                } else {
                    theme.text_muted()
                };
                remote_row_tag_line(
                    ui,
                    theme,
                    &glyph,
                    theme.workspace_mirror_icon_size(),
                    if live {
                        theme.workspace_mirror_fg().to_egui()
                    } else {
                        theme.tag_fg().to_egui()
                    },
                    "remote",
                    remote_row_state_word_key(state)
                        .map(|k| (t_lang(lang, k), word_color.to_egui())),
                );
            });
        });
}

fn rows(ui: &mut egui::Ui, theme: &Theme, lang: &'static str, active: bool) {
    ui.allocate_ui_with_layout(
        egui::vec2(ROWS_W.value(), 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(ROWS_W.value());
            egui::Frame::new()
                .fill(theme.bg_sidebar().to_egui())
                .stroke(egui::Stroke::new(
                    theme.border_width.value(),
                    theme.border_frame().to_egui(),
                ))
                .corner_radius(theme.corner_radius.value())
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (_, state, warn) in ROWS {
                        row(ui, theme, lang, state, warn, active);
                    }
                });
        },
    );
}

fn rail(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::new()
        .fill(theme.bg_sidebar().to_egui())
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, state, warn) in ROWS {
                    ui.allocate_ui(
                        egui::vec2(RAIL_CELL_W.value(), AVATAR.value() * 2.0),
                        |ui| {
                            ui.vertical_centered(|ui| {
                                ui.set_max_width(RAIL_CELL_W.value());
                                let (avatar, _) = ui.allocate_exact_size(
                                    egui::vec2(AVATAR.value(), AVATAR.value()),
                                    egui::Sense::hover(),
                                );
                                let p = ui.painter();
                                p.text(
                                    avatar.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "T",
                                    egui::FontId::monospace(theme.font_size_body.value()),
                                    theme.text_muted().to_egui(),
                                );
                                if state.attempting() {
                                    Spinner::new()
                                        .size(AVATAR.value() * 0.78)
                                        .stroke(1.72)
                                        .paint_in(ui, theme, avatar);
                                }
                                if state == RemoteRowState::Connected {
                                    paint_workspace_mirror_chip(ui, theme, avatar);
                                } else {
                                    paint_workspace_mirror_chip_offline(ui, theme, avatar);
                                }
                                if warn {
                                    paint_attach_refusal_chip(
                                        ui,
                                        theme,
                                        avatar,
                                        theme.bg_sidebar().into(),
                                    );
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(label)
                                            .monospace()
                                            .size(theme.font_size_micro.value())
                                            .color(theme.text_muted().to_egui()),
                                    )
                                    .wrap(),
                                );
                            });
                        },
                    );
                }
            });
        });
}

fn draw_marks(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Wrap, |ui| {
        labelled(ui, theme, "inactive rows", |ui| {
            rows(ui, theme, "en", false)
        });
        labelled(ui, theme, "active rows", |ui| rows(ui, theme, "en", true));
        labelled(ui, theme, "rail", |ui| {
            ui.set_max_width(RAIL_CELL_W.value() * 4.5);
            rail(ui, theme);
        });
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for lang in langs() {
            labelled(ui, theme, lang, |ui| rows(ui, theme, lang, false));
        }
    });
    meta(
        ui,
        theme,
        &[
            (
                "connected",
                "REMOTE info Tag in the remote hue + the workspace's own dot and running count — unchanged",
            ),
            (
                "off the live state",
                "REMOTE default Tag + status word: caption, text-muted (text-on-active on the active row), end-elided",
            ),
            (
                "words",
                "connecting · reconnecting (status_bar.remote_reconnecting) · disconnected (status_bar.remote_disconnected) · not connected",
            ),
            (
                "dot slot",
                "trying: the LNB loading spinner · otherwise not live: idle dot · running count hidden",
            ),
            (
                "reason",
                "alertTriangle in attach-refusal-glyph after the name · tooltip: what it is, profile → remote workspace, the reason",
            ),
            (
                "rail BR chip",
                "live: unchanged · not live: 1px border-control, glyph-dim remote glyph on workspace-mirror-chip-bg",
            ),
            (
                "rail trying",
                "the 28 loading ring around the initial; corner chips draw above it",
            ),
            ("rail TL chip", "the refusal chip on a reason"),
            (
                "agent use (N2)",
                "the row goes to connecting and back; focus stays where it is",
            ),
        ],
        &[
            TokenChip::new("tag-fg", "REMOTE, not live", theme.tag_fg().to_egui()),
            TokenChip::new("text-muted", "status word", theme.text_muted().to_egui()),
            TokenChip::new(
                "attach-refusal-glyph",
                "reason mark",
                theme.attach_refusal_glyph().to_egui(),
            ),
            TokenChip::new(
                "border-control",
                "rail chip edge, not live",
                theme.border_control().to_egui(),
            ),
            TokenChip::new(
                "glyph-dim",
                "rail chip glyph, not live",
                theme.glyph_dim().to_egui(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "The auto-attach refusal banner does not show on a saved remote row; the popup carries the reason. The row mark and rail chip are the refusal's.",
    );
    dont(
        ui,
        theme,
        "Don't drop the REMOTE tag when the connection is down. It is what the row is; the word next to it says how it stands.",
    );
}

fn draw_pending(ui: &mut egui::Ui, theme: &Theme) {
    use RemoteRowPopupKind as K;
    stage(ui, theme, StageVariant::Wrap, |ui| {
        let mut next = Pop::new(K::User, true);
        next.user_body = REMOTE_ROW_USER_BODY_NEXT;
        for (label, pop) in [
            (
                "L1 a — opened again: still disconnected, Reconnect only",
                Pop::new(K::User, true),
            ),
            (
                "L1 b — opened again: reconnects at once",
                Pop::new(K::Reconnecting, true),
            ),
            (
                "L1 b — the line under Disconnected while it is still on screen",
                next,
            ),
            (
                "L4 a — on screen after a restart: connecting",
                Pop::new(K::Connecting, false),
            ),
            (
                "L4 b — on screen after a restart: Not connected + Connect",
                Pop::new(K::Idle, false),
            ),
        ] {
            labelled(ui, theme, label, |ui| frame(ui, theme, FRAME_W, pop));
        }
    });
    meta(
        ui,
        theme,
        &[
            (
                "L1 a",
                "user_body_stay · menu Reconnect enabled · agent request: row mark unchanged (disconnected) — built",
            ),
            (
                "L1 b",
                "user_body_next while on screen · opening it again: reconnecting popup · agent request: row mark connecting",
            ),
            (
                "L2",
                "one form — the connecting mark can last up to 35 s (decided)",
            ),
            (
                "L3",
                "one form — list queries leave not connected as it is (decided)",
            ),
            (
                "L4 a",
                "first screen Connecting… (also after window focus and a closed neighbour) — built",
            ),
            ("L4 b", "first screen remote.row.popup.idle_title + Connect"),
        ],
        &[],
    );
}
