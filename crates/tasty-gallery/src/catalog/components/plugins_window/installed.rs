//! 설치된 플러그인의 목록·상세·제거 확인 예제.
//! 본체 상세는 스크롤하지만 갤러리는 전체 내용을 비교할 수 있게 예제 높이를 늘린다.

use crate::catalog::spec::{self, StageVariant, TokenChip};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::{PLUGIN_LIST_ROW_HEIGHT, STRUCT_GAP_2};
use tasty_ui_widgets::{
    PluginAvatarSize, PluginDetailBarView, PluginIdentityView, PluginInstallPathsView,
    PluginKeycapStyle, PluginMetaView, PluginUninstallConfirmView, TagVariant, margin_sym,
    paint_plugin_avatar, plugin_command_row, plugin_detail_bar, plugin_detail_bar_height,
    plugin_detail_description, plugin_detail_identity, plugin_detail_section,
    plugin_detail_section_gap, plugin_install_paths, plugin_uninstall_confirm_bar,
    plugin_uninstall_confirm_bar_height, tag_mono,
};

/// 상세 컬럼이 그릴 것 — 본체는 선택 상태와 uninstall 확인 상태로 갈린다.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Detail {
    /// `ui_state.selected_id` 가 비었을 때. 본체 `plugins.none_selected`.
    None,
    /// 선택된 행의 전체 메타.
    Selected(usize),
    /// `Uninstall` 을 누른 뒤 — 액션 바 자리가 질문 · 안내 · Cancel · Uninstall 로 바뀐다.
    ConfirmUninstall(usize),
}

impl Detail {
    /// 목록에서 강조할 행. 빈 상태에는 강조가 없다.
    pub(super) fn selected_row(self) -> Option<usize> {
        match self {
            Detail::None => None,
            Detail::Selected(i) | Detail::ConfirmUninstall(i) => Some(i),
        }
    }
}

/// 목록 행 하나의 표시 데이터 — 본체 `PluginsSnapshot.plugins` 항목과 동형.
pub(super) struct Row {
    pub(super) name: &'static str,
    version: &'static str,
    builtin: bool,
    enabled: bool,
    running: bool,
    health_error: bool,
    id: &'static str,
    description: &'static str,
    authors: &'static str,
    homepage: &'static str,
    surface_kinds: &'static [&'static str],
    permissions: &'static [&'static str],
    commands: &'static [(&'static str, &'static str)],
    install_dir: &'static str,
    log_path: &'static str,
}

pub(super) const ROWS: &[Row] = &[
    Row {
        name: "Clipboard viewer",
        version: "0.4.2",
        builtin: true,
        enabled: true,
        running: true,
        health_error: false,
        id: "com.tasty.clipboard-viewer",
        description: "Shows the clipboard history in a popup, grouped by content type.",
        authors: "tasty",
        homepage: "https://github.com/zilhak/tasty",
        surface_kinds: &["clipboard-viewer"],
        permissions: &["clipboard", "surface:read"],
        // 매니페스트 원문 그대로. 키캡은 `Ctrl` `Shift` `V` 로 다듬어 그린다.
        commands: &[("Open clipboard viewer", "ctrl + shift + v")],
        install_dir: "~/.tasty/plugins/com.tasty.clipboard-viewer",
        log_path: "~/.tasty/logs/com.tasty.clipboard-viewer.log",
    },
    Row {
        name: "Git viewer",
        version: "0.3.1",
        builtin: true,
        enabled: true,
        running: false,
        health_error: true,
        id: "com.tasty.git-viewer",
        description: "Shows the working tree and staged diff for the surface's directory.",
        authors: "tasty",
        homepage: "",
        surface_kinds: &[],
        permissions: &["fs:read", "process:read"],
        commands: &[],
        install_dir: "~/.tasty/plugins/com.tasty.git-viewer",
        log_path: "~/.tasty/logs/com.tasty.git-viewer.log",
    },
    Row {
        name: "Markdown",
        version: "0.9.0",
        builtin: false,
        enabled: false,
        running: false,
        health_error: false,
        id: "com.tasty.markdown",
        description: "Renders markdown files in a native WebView surface.",
        authors: "tasty",
        homepage: "",
        surface_kinds: &["markdown"],
        permissions: &["fs:read"],
        commands: &[],
        install_dir: "~/.tasty/plugins/com.tasty.markdown",
        log_path: "~/.tasty/logs/com.tasty.markdown.log",
    },
];

/// 목록 행의 높이는 아바타와 위아래 패딩으로 정한다.
pub(super) fn list_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, detail: Detail) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let row_h = PLUGIN_LIST_ROW_HEIGHT.value();
    let pad = egui::vec2(theme.spacing_sm.value(), theme.spacing_sm.value());
    let avatar = PluginAvatarSize::Row.side().value();
    let mut y = rect.min.y + theme.spacing_sm.value();

    for (i, row) in ROWS.iter().enumerate() {
        let r =
            egui::Rect::from_min_size(egui::pos2(rect.min.x, y), egui::vec2(rect.width(), row_h));
        if detail.selected_row() == Some(i) {
            p.rect(
                r,
                theme.corner_radius.value(),
                theme.surface_active().to_egui(),
                egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
                egui::StrokeKind::Inside,
            );
        }

        let name = if row.builtin {
            format!("{}  •", row.name)
        } else {
            row.name.to_string()
        };
        paint_plugin_avatar(
            &p,
            theme,
            egui::pos2(r.min.x + pad.x + avatar * 0.5, r.center().y),
            row.name,
            PluginAvatarSize::Row,
        );

        let on_row = detail.selected_row() == Some(i);
        let name_pos = r.min + pad + egui::vec2(avatar + theme.spacing_sm.value(), 0.0);
        p.text(
            name_pos,
            egui::Align2::LEFT_TOP,
            &name,
            egui::FontId::proportional(theme.font_size_body.value()),
            if on_row {
                theme.text_on_active()
            } else if row.enabled {
                theme.text_primary()
            } else {
                theme.text_muted()
            }
            .to_egui(),
        );

        let mut sub = format!("v{}", row.version);
        if !row.enabled {
            sub.push_str("  ·  Disabled");
        } else if row.running {
            sub.push_str("  ·  Running");
        }
        p.text(
            name_pos + egui::vec2(0.0, theme.spacing_lg.value() + STRUCT_GAP_2.value()),
            egui::Align2::LEFT_TOP,
            &sub,
            egui::FontId::proportional(theme.font_size_micro.value()),
            if on_row {
                theme.text_on_active()
            } else {
                theme.text_muted()
            }
            .to_egui(),
        );

        if row.health_error && row.enabled {
            p.circle_filled(
                egui::pos2(r.max.x - theme.spacing_md.value(), r.center().y),
                theme.status_dot_size.value() * 0.5,
                theme.accent_danger().to_egui(),
            );
        }
        y += row_h + STRUCT_GAP_2.value();
    }
}

fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_body.value())
            .color(theme.text_primary().to_egui()),
    );
}

/// 활성 플러그인의 실행 오류만 강조한다. 사용자가 비활성화한 상태는 오류로 표시하지 않는다.
fn health_box(ui: &mut egui::Ui, theme: &Theme) {
    let danger = theme.accent_danger().to_egui();
    egui::Frame::new()
        .fill(danger.gamma_multiply(theme.tint_fill_alpha()))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            danger.gamma_multiply(theme.tint_border_alpha()),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(margin_sym(theme.spacing_md, theme.spacing_sm))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("plugins.health_error"))
                    .size(theme.font_size_body.value())
                    .color(danger),
            );
        });
}

/// 하단 액션 바 — 본체와 같은 공용 위젯. 스위치·Configure·Uninstall.
fn action_bar(ui: &mut egui::Ui, theme: &Theme, row: &Row) {
    plugin_detail_bar(
        ui,
        theme,
        &PluginDetailBarView {
            enabled: row.enabled,
            enabled_label: crate::i18n::t("plugins.enabled"),
            disabled_label: crate::i18n::t("plugins.disabled"),
            configure: crate::i18n::t("plugins.configure"),
            uninstall: crate::i18n::t("plugins.uninstall"),
        },
    );
}

/// Tag 를 줄바꿈해 늘어놓는 절 — 디자인 `Mono` 머리글 + Tag 묶음. 비면 본체처럼 `(none)` 을 그린다.
fn tag_section(ui: &mut egui::Ui, theme: &Theme, label: &str, values: &[&str]) {
    plugin_detail_section(ui, theme, label, |ui| {
        if values.is_empty() {
            caption(ui, theme, "(none)");
        } else {
            ui.horizontal_wrapped(|ui| {
                for v in values {
                    tag_mono(ui, theme, v, TagVariant::Default, false);
                }
            });
        }
    });
}

/// `Commands` — 행마다 mono 제목과 단축키 Kbd, 아래 구분선. 명령이 없으면 절 자체가 안 나온다.
fn commands(ui: &mut egui::Ui, theme: &Theme, row: &Row) {
    if row.commands.is_empty() {
        return;
    }
    plugin_detail_section_gap(ui, theme);
    plugin_detail_section(ui, theme, "Commands", |ui| {
        for (title, kb) in row.commands {
            // 본체 비-macOS 기본 표기. macOS 표기는 `keycap_platforms` 예제에서 따로 보인다.
            plugin_command_row(ui, theme, title, Some(kb), &PluginKeycapStyle::default());
        }
    });
}

/// 설치 경로 + 로그 경로. 본체와 같은 공용 위젯 — `Open folder` 는 머리글 줄 오른쪽, 경로는 줄바꿈.
fn paths(ui: &mut egui::Ui, theme: &Theme, row: &Row) {
    plugin_detail_section_gap(ui, theme);
    plugin_install_paths(
        ui,
        theme,
        &PluginInstallPathsView {
            label: "Install path",
            open_folder: "Open folder",
            install_dir: row.install_dir,
            log_line: &format!("Log: {}", row.log_path),
        },
    );
}

/// 제거 확인 바의 문구 — 본체와 같은 번역 키.
fn confirm_view<'a>(row: &Row, title: &'a str) -> PluginUninstallConfirmView<'a> {
    let note = if row.builtin {
        crate::i18n::t("plugins.uninstall_builtin_note")
    } else {
        crate::i18n::t("plugins.uninstall_note")
    };
    PluginUninstallConfirmView {
        title,
        note,
        cancel: crate::i18n::t("button.cancel"),
        uninstall: crate::i18n::t("plugins.uninstall"),
        // 정적 예제라 포커스를 옮기지 않는다. 다른 카드의 포커스를 빼앗지 않도록 한다.
        focus_cancel: false,
        busy: None,
        wrap: false,
    }
}

/// 우측 상세 — 본체 `CentralPanel` 블록 전량. 액션 바는 열 바닥에 열 폭 전체로 붙는다.
pub(super) fn detail_pane(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, detail: Detail) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let inner = rect.shrink(theme.spacing_md.value());
    let Some(i) = detail.selected_row() else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        child.add_space(theme.spacing_xl.value());
        caption(&mut child, theme, crate::i18n::t("plugins.none_selected"));
        return;
    };
    let row = &ROWS[i];

    // 바는 상세 열의 여백 밖, 열 폭 전체를 쓰며 아래 끝에 붙는다. 본문만 여백 안에 둔다.
    // 제거 확인은 바의 내용을 그 자리에서 바꾸며, 글이 컨트롤보다 크면 바만 그만큼 커진다.
    let title = crate::i18n::t_fmt("plugins.uninstall_confirm_title", row.name);
    let confirm = matches!(detail, Detail::ConfirmUninstall(_)).then(|| confirm_view(row, &title));
    let bar_h = match &confirm {
        Some(view) => plugin_uninstall_confirm_bar_height(ui, theme, view, rect.width()),
        None => plugin_detail_bar_height(theme),
    };
    let split = rect.max.y - bar_h;
    let body_rect = egui::Rect::from_min_max(inner.min, egui::pos2(inner.max.x, split));
    let bar_rect = egui::Rect::from_min_max(egui::pos2(rect.min.x, split), rect.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
    child.spacing_mut().item_spacing.y = theme.spacing_sm.value();

    plugin_detail_identity(
        &mut child,
        theme,
        &PluginIdentityView {
            name: row.name,
            version: row.version,
            builtin_tag: row.builtin.then(|| crate::i18n::t("plugins.builtin_badge")),
            meta: PluginMetaView {
                authors: row.authors,
                id: row.id,
                homepage: row.homepage,
                wrap_long: false,
            },
        },
    );
    plugin_detail_description(&mut child, theme, row.description);

    if row.health_error && row.enabled {
        health_box(&mut child, theme);
    }

    // 디자인 순서: Permissions → Command → Surface kinds(있을 때만) → Install path.
    plugin_detail_section_gap(&mut child, theme);
    tag_section(&mut child, theme, "Permissions", row.permissions);

    commands(&mut child, theme, row);

    if !row.surface_kinds.is_empty() {
        plugin_detail_section_gap(&mut child, theme);
        tag_section(&mut child, theme, "Surface kinds", row.surface_kinds);
    }

    paths(&mut child, theme, row);

    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
    match &confirm {
        Some(view) => {
            plugin_uninstall_confirm_bar(&mut bar, theme, view);
        }
        None => action_bar(&mut bar, theme, row),
    }
}

/// 시안 Spec "Installed detail — install path" 의 두 상세 열 폭. 720(최소)·880(기본) 창의 상세 열에
/// 해당하는 예제 무대 전용 값이며 역할 토큰이 없다.
const DETAIL_WIDTHS: [LogicalPx; 2] = [LogicalPx(380.0), LogicalPx(540.0)];

/// 긴 설치 경로의 예시. 공백이 없어 아무 문자에서 줄바꿈하는 것을 보인다.
const LONG_ID: &str = "com.example.image-viewer-with-a-long-plugin-identifier";

/// 설치 경로 절 — 두 열 폭에서 `Open folder` 가 머리글 줄 오른쪽에 온전히 남고 경로가 줄바꿈한다.
pub fn draw_install_paths(ui: &mut egui::Ui, theme: &Theme) {
    let install_dir = format!("/home/tasty/.local/share/tasty/plugins/{LONG_ID}");
    let log_line = format!("Log: /home/tasty/.local/state/tasty/plugins/{LONG_ID}/plugin.log");
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
            for width in DETAIL_WIDTHS {
                egui::Frame::new()
                    .fill(theme.bg_panel().to_egui())
                    .stroke(egui::Stroke::new(
                        theme.border_width.value(),
                        theme.border_default().to_egui(),
                    ))
                    .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
                    .show(ui, |ui| {
                        ui.set_width(width.value() - 2.0 * theme.spacing_lg.value());
                        plugin_install_paths(
                            ui,
                            theme,
                            &PluginInstallPathsView {
                                label: "Install path",
                                open_folder: "Open folder",
                                install_dir: &install_dir,
                                log_line: &log_line,
                            },
                        );
                    });
            }
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "order",
                "… Permissions · Command · Surface kinds · Install path",
            ),
            (
                "caption row",
                "INSTALL PATH (mono 10 caps) · flex · Open folder",
            ),
            (
                "Open folder",
                "Button secondary sm · folder icon · opens the OS file manager",
            ),
            (
                "path",
                "mono caption 11 · text-muted · break-all · selectable",
            ),
            ("log", "same style, “Log: ” prefix, own line"),
            ("widths", "left 380 ≈ 720 window · right 540 ≈ 880 window"),
            ("shown", "installed plugins only"),
        ],
        &[
            TokenChip::new("text-muted", "path text", theme.text_muted().to_egui()),
            TokenChip::without_color("font-size-caption", "11 path"),
            TokenChip::without_color("space-sm", "8 row gap"),
        ],
    );
}

/// 디자인 Spec "Installed detail — command keycaps per platform". 매니페스트 `ctrl + shift + h` 와
/// `alt + k` 를 Windows·Linux, macOS 낱말 표기, macOS 기호 표기로 그린다. 키캡은 본체와 같은
/// [`plugin_keycap_parts`] 를 거친다.
pub(super) fn keycap_platforms(ui: &mut egui::Ui, theme: &Theme) {
    use tasty_ui_widgets::{PluginKeycap, kbd_parts, plugin_keycap_parts};
    let rows = [
        ("Windows / Linux", PluginKeycapStyle::default()),
        (
            "macOS · style text",
            PluginKeycapStyle::from_setting_names(true, "cmd", "option", "shift"),
        ),
        (
            "macOS · style symbol",
            PluginKeycapStyle::from_setting_names(true, "symbol", "symbol", "symbol"),
        ),
    ];
    egui::Grid::new("plugins_keycap_platforms")
        .spacing(egui::vec2(
            theme.spacing_lg.value(),
            theme.spacing_md.value(),
        ))
        .show(ui, |ui| {
            for (label, style) in rows {
                ui.label(
                    egui::RichText::new(label)
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted().to_egui()),
                );
                for chord in ["ctrl + shift + h", "alt + k"] {
                    let caps = plugin_keycap_parts(chord, &style);
                    let keys: Vec<_> = caps.iter().map(PluginKeycap::as_kbd_key).collect();
                    ui.horizontal(|ui| {
                        kbd_parts(ui, theme, &keys, tasty_ui_widgets::Ground::Panel)
                    });
                }
                ui.end_row();
            }
        });
}
