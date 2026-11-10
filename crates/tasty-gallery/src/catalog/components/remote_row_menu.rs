//! 저장된 원격 행의 우클릭 메뉴(연결 상태별)와 "원격 워크스페이스 닫기" 확인 대화.
//! 시안 overlays-popups `remote-row-context-menu-by-connection-state`와 overlays-dialogs
//! `close-remote-workspace-destructive-confirm`을 옮긴다. 켜짐 규칙과 확인 콘텐츠는 본체와 같은
//! `tasty_ui_widgets::remote_row_menu`·`close_remote_confirm`을 쓴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    CloseRemoteConfirmView, MenuItemVariant, REMOTE_ROW_CLOSE_LOCAL_KEY,
    REMOTE_ROW_CLOSE_REMOTE_KEY, REMOTE_ROW_DISCONNECT_KEY, RemoteRowState, close_remote_confirm,
    menu_item, menu_separator, remote_row_menu,
};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use crate::i18n::{t_lang, translated_languages};

/// 시안 메뉴 최소 폭(`minWidth: 176`, 테두리·안쪽 여백 포함). 사이드바 메뉴와 같다.
const MENU_MIN_W: LogicalPx = LogicalPx(176.0);
/// 캡션과 메뉴 사이 간격 6(시안 stage gap).
const CAPTION_GAP: LogicalPx = LogicalPx(6.0);
/// 확인 대화 무대 높이(시안 Backdrop 300 · last 320).
const CONFIRM_STAGE_H: LogicalPx = LogicalPx(300.0);
const CONFIRM_LAST_STAGE_H: LogicalPx = LogicalPx(340.0);
/// 예시 행 이름·프로필(시안 기본값).
const ROW_NAME: &str = "tasty-core";
const PROFILE: &str = "gx10";

enum Row {
    Item(String, bool),
    Sep,
}

/// 저장 행 메뉴의 항목. 순서는 본체 `saved_row_menu_items`와 같다.
fn rows(lang: &str, state: RemoteRowState, categories: &[&str]) -> Vec<Row> {
    let on = remote_row_menu(state);
    let tx = |key: &'static str| t_lang(lang, key).to_owned();
    let mut rows = vec![
        Row::Item(tx("context_menu.rename_title"), true),
        Row::Item(tx("context_menu.rename_subtitle"), true),
        Row::Sep,
        Row::Item(tx("context_menu.move_up"), true),
        Row::Item(tx("context_menu.move_down"), true),
        Row::Sep,
        Row::Item(tx("preset.context.save_as_workspace_preset"), true),
    ];
    if !categories.is_empty() {
        rows.push(Row::Sep);
        rows.push(Row::Item(tx("workspace_category.move_to_category"), false));
        rows.extend(categories.iter().map(|c| Row::Item((*c).to_owned(), true)));
        rows.push(Row::Sep);
        rows.push(Row::Item(tx("workspace_category.new_category"), true));
    }
    rows.extend([
        Row::Sep,
        Row::Item(tx(on.connect_key), on.connect),
        Row::Item(tx(REMOTE_ROW_DISCONNECT_KEY), on.disconnect),
        Row::Sep,
        Row::Item(tx(REMOTE_ROW_CLOSE_LOCAL_KEY), on.close_local),
        Row::Item(tx(REMOTE_ROW_CLOSE_REMOTE_KEY), on.close_remote),
    ]);
    rows
}

fn menu(ui: &mut egui::Ui, theme: &Theme, caption: &str, rows: &[Row]) {
    spec::wrap_item(ui, |ui| {
        ui.spacing_mut().item_spacing.y = CAPTION_GAP.value();
        ui.label(
            egui::RichText::new(caption)
                .monospace()
                .size(theme.font_size_micro.value())
                .color(theme.text_muted_raised().to_egui()),
        );
        kit::frame_card_menu(ui, theme, MENU_MIN_W, |ui| {
            let ring = theme.popup_content_margin().value();
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(ring as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row in rows {
                        match row {
                            Row::Sep => menu_separator(ui, theme),
                            Row::Item(label, enabled) => {
                                menu_item(
                                    ui,
                                    theme,
                                    None,
                                    label,
                                    None,
                                    MenuItemVariant::Normal,
                                    false,
                                    *enabled,
                                );
                            }
                        }
                    }
                });
        });
    });
}

const STATES: [(&str, RemoteRowState); 5] = [
    ("connecting", RemoteRowState::Connecting),
    ("reconnecting", RemoteRowState::Reconnecting),
    (
        "disconnected (last screen kept)",
        RemoteRowState::Disconnected,
    ),
    (
        "not connected (after a restart)",
        RemoteRowState::NotConnected,
    ),
    ("connected", RemoteRowState::Connected),
];

/// Overlays 페이지의 원격 닫기 확인 구역.
pub fn close_remote_section() -> crate::catalog::Section {
    crate::catalog::section(
        "closeonremote",
        "Remote workspace close confirmation",
        vec![crate::catalog::spec(
            "close-remote-workspace-destructive-confirm",
            "Saved remote row — destructive confirm before closing it on the remote",
            Some("440px · danger glyph and button · Cancel has default focus · last-tab line"),
            draw_close_remote,
        )],
    )
}

/// 사이드바 메뉴 구역에 놓는 저장된 원격 행 메뉴 표본.
pub fn menu_spec() -> crate::catalog::Spec {
    crate::catalog::spec(
        "remote-row-context-menu-by-connection-state",
        "Saved remote workspace — the row menu by connection state",
        Some("OS menu · connection group and close group at the end · disabled, not hidden"),
        draw_menu,
    )
}

/// Overlays › Workspace categories — 저장된 원격 행의 우클릭 메뉴, 연결 상태별.
pub fn draw_menu(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(theme.spacing_xl.value(), 0.0);
                menu(
                    ui,
                    theme,
                    "connected · categories on",
                    &rows("en", RemoteRowState::Connected, &["Workspaces", "Services"]),
                );
                for (caption, state) in &STATES[..4] {
                    menu(ui, theme, caption, &rows("en", *state, &[]));
                }
            },
        );
    });
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(theme.spacing_xl.value(), 0.0);
                for lang in translated_languages() {
                    for (word, state) in [
                        ("connected", RemoteRowState::Connected),
                        ("disconnected", RemoteRowState::Disconnected),
                    ] {
                        menu(
                            ui,
                            theme,
                            &format!("{lang} · {word}"),
                            &rows(lang, state, &[]),
                        );
                    }
                }
            },
        );
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "order",
                "Rename · Change Subtitle | Move Up · Move Down | Save as workspace preset | (categories) | (Force detach) | Connect or Reconnect · Disconnect | Close locally · Close remote workspace…",
            ),
            (
                "Connect / Reconnect",
                "enabled: disconnected, not connected · disabled: connected, connecting, reconnecting · Reconnect when the row had a screen in this run",
            ),
            (
                "Disconnect",
                "enabled: connected, connecting, reconnecting · while connecting or reconnecting it cancels the attempt in progress and leaves the row disconnected by the user · no automatic reconnect after it",
            ),
            (
                "Close locally",
                "always · the remote workspace keeps running · no confirmation",
            ),
            (
                "Close remote workspace…",
                "connected only · opens the confirmation (Close remote workspace)",
            ),
            (
                "replaces",
                "Disconnect Workspace and Close Workspace on a saved remote row",
            ),
            (
                "Force detach",
                "unchanged — only when this Tasty serves the workspace to another client",
            ),
            ("agent mirror", "today's menu, unchanged"),
        ],
        &[],
    );
    spec::note(
        ui,
        theme,
        "The app draws this menu with the OS menu, so only order, wording, separators and disabled items carry over. Each connection item is disabled, not hidden, where it cannot act. The menu opens with or without categories; the category block appears only with categories on. Copy: remote.row.popup.connect · child_graph.reconnect · remote.row.menu.disconnect · remote.row.menu.close_local · remote.row.menu.close_remote.",
    );
    spec::dont(
        ui,
        theme,
        "Don't label the local one plain “Close”. On a remote row the word alone reads as closing the remote workspace, which is the other item.",
    );
}

fn confirm_card(ui: &mut egui::Ui, theme: &Theme, lang: &str, last: bool) {
    let title = t_lang(lang, "remote.row.close_remote.title").replacen("{}", ROW_NAME, 1);
    let body = t_lang(lang, "remote.row.close_remote.body").replacen("{}", PROFILE, 1);
    let glyph = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        icons::ALERT_TRIANGLE
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    kit::frame_card(
        ui,
        theme,
        theme.info_modal_width(),
        theme.bg_panel().to_egui(),
        |ui| {
            close_remote_confirm(
                ui,
                theme,
                &CloseRemoteConfirmView {
                    title: &title,
                    last: last.then(|| t_lang(lang, "remote.row.close_remote.last")),
                    body: &body,
                    cancel: t_lang(lang, "button.cancel"),
                    confirm: t_lang(lang, "remote.row.close_remote.confirm"),
                    glyph: &glyph,
                    focus_cancel: false,
                },
            );
        },
    );
}

/// Overlays › Close remote workspace — 원격 닫기 확인. 메뉴와 마지막 탭·pane 닫기 두 형태.
pub fn draw_close_remote(ui: &mut egui::Ui, theme: &Theme) {
    let stage_w = theme.info_modal_width() + theme.spacing_xl * 2.0;
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "from the row menu", |ui| {
            kit::scrim_backdrop(
                ui,
                theme,
                stage_w,
                CONFIRM_STAGE_H,
                theme.spacing_xl,
                |ui| confirm_card(ui, theme, "en", false),
            );
        });
        spec::cluster(ui, theme, "closing the last tab or pane", |ui| {
            kit::scrim_backdrop(
                ui,
                theme,
                stage_w,
                CONFIRM_STAGE_H,
                theme.spacing_xl,
                |ui| confirm_card(ui, theme, "en", true),
            );
        });
    });
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for lang in translated_languages() {
            spec::cluster(ui, theme, &format!("{lang} · last tab"), |ui| {
                kit::scrim_backdrop(
                    ui,
                    theme,
                    stage_w,
                    CONFIRM_LAST_STAGE_H,
                    theme.spacing_xl,
                    |ui| confirm_card(ui, theme, lang, true),
                );
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("scope", "Window · centred on the scrim · shadow-modal"),
            ("width", "info-modal-width 440, at most the window"),
            (
                "head",
                "alertTriangle 16 in accent-danger-raised · title (font-size-max, semibold) with the workspace name",
            ),
            (
                "body",
                "last-tab line (only when opened by closing the last tab or pane) · then what ends on which profile · text-secondary",
            ),
            (
                "buttons",
                "Cancel ghost (default focus) · danger button with the action",
            ),
            ("dismiss", "Esc, Cancel — nothing happens"),
            (
                "opened by",
                "Close remote workspace… in the row menu · closing the last tab or pane of a connected saved remote row",
            ),
            (
                "not for",
                "Close locally · a local workspace's last tab · an agent mirror",
            ),
        ],
        &[
            TokenChip::new(
                "accent-danger",
                "glyph + button",
                theme.accent_danger_raised().to_egui(),
            ),
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::without_color("info-modal-width", "width 440"),
        ],
    );
    spec::note(
        ui,
        theme,
        "After it closes: when this Tasty closes the workspace on the remote, from this confirm or by closing the last tab or pane, the row goes away with it. When the remote workspace is closed elsewhere, the row stays and says why (Remote workspace closed over the workspace-missing reason).",
    );
    spec::dont(
        ui,
        theme,
        "Don't skip the confirm for the last tab because a local workspace does. Locally it only drops a layout; here it ends processes on another machine.",
    );
}
