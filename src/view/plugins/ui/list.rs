use crate::i18n::t;
use crate::theme;

use super::{PluginsAction, PluginsSnapshot, PluginsUiState};
use tasty_ui_widgets::tokens::{PLUGIN_LIST_ROW_HEIGHT, STRUCT_GAP_2};
use tasty_ui_widgets::{
    PluginAvatarSize, PluginDetailBarView, PluginIdentityView, PluginInstallPathsView,
    PluginKeycapStyle, PluginMetaView, PluginUninstallConfirmView, TagVariant, margin_sym,
    paint_plugin_avatar, plugin_command_row, plugin_detail_bar, plugin_detail_bar_height,
    plugin_detail_description, plugin_detail_identity, plugin_detail_section,
    plugin_detail_section_gap, plugin_install_paths, plugin_uninstall_confirm_bar,
    plugin_uninstall_confirm_bar_height, tag_mono, vspace,
};

pub(super) fn draw_list_tab(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
) {
    let th = theme::theme();

    if ui_state.selected_id.is_none() {
        ui_state.selected_id = snapshot.plugins.first().map(|p| p.id.clone());
    } else if let Some(id) = &ui_state.selected_id
        && !snapshot.plugins.iter().any(|p| &p.id == id)
    {
        ui_state.selected_id = snapshot.plugins.first().map(|p| p.id.clone());
    }

    // name / authors / description 부분일치 필터 (대소문자 무시).
    let needle = ui_state.filter.trim().to_lowercase();
    let visible: Vec<_> = snapshot
        .plugins
        .iter()
        .filter(|e| {
            if needle.is_empty() {
                return true;
            }
            let hay =
                format!("{} {} {}", e.name, e.authors.join(" "), e.description).to_lowercase();
            hay.contains(&needle)
        })
        .collect();

    egui::SidePanel::left("plugins_list")
        .exact_width(th.plugins_side_panel_width().value())
        .resizable(false)
        .show(ctx, |ui| {
            vspace(ui, th.spacing_sm);
            if snapshot.plugins.is_empty() {
                vspace(ui, th.spacing_lg);
                ui.label(
                    egui::RichText::new(t("plugins.empty"))
                        .color(egui::Color32::from(th.text_muted())),
                );
                return;
            }
            if visible.is_empty() {
                vspace(ui, th.spacing_lg);
                ui.label(
                    egui::RichText::new(t("plugins.no_matches"))
                        .color(egui::Color32::from(th.text_muted())),
                );
                return;
            }
            egui::ScrollArea::vertical()
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    for entry in &visible {
                        let selected = ui_state.selected_id.as_ref() == Some(&entry.id);
                        let name_text = if entry.builtin {
                            format!("{}  •", entry.name)
                        } else {
                            entry.name.clone()
                        };
                        let mut sub = format!("v{}", entry.version);
                        if !entry.enabled {
                            sub.push_str(&format!("  ·  {}", t("plugins.disabled")));
                        } else if entry.running {
                            sub.push_str(&format!("  ·  {}", t("plugins.running")));
                        }

                        // 이름과 버전 두 줄을 한 클릭 영역으로 묶기 위해 직접 그린다.
                        let row_h = PLUGIN_LIST_ROW_HEIGHT.value();
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_h),
                            egui::Sense::click(),
                        );
                        let visuals = ui.style().interact_selectable(&resp, selected);
                        if selected || resp.hovered() {
                            ui.painter().rect(
                                rect,
                                visuals.corner_radius,
                                visuals.weak_bg_fill,
                                visuals.bg_stroke,
                                egui::StrokeKind::Inside,
                            );
                        }
                        let pad = egui::vec2(th.spacing_sm.value(), th.spacing_sm.value());
                        let avatar = PluginAvatarSize::Row.side().value();
                        paint_plugin_avatar(
                            ui.painter(),
                            &th,
                            egui::pos2(rect.min.x + pad.x + avatar * 0.5, rect.center().y),
                            &entry.name,
                            PluginAvatarSize::Row,
                        );
                        // 텍스트 열은 아바타 다음 — 디자인 flex 행의 `gap: space-sm`.
                        let name_pos =
                            rect.min + pad + egui::vec2(avatar + th.spacing_sm.value(), 0.0);
                        ui.painter().text(
                            name_pos,
                            egui::Align2::LEFT_TOP,
                            &name_text,
                            egui::FontId::proportional(th.font_size_body.value()),
                            // 꺼진 plugin 도 고를 수 있으므로 이름은 읽는 글자다. 선택 잉크가 앞선다.
                            if entry.enabled || selected {
                                visuals.text_color()
                            } else {
                                th.text_muted().to_egui()
                            },
                        );
                        let sub_pos = name_pos + egui::vec2(0.0, 18.0);
                        ui.painter().text(
                            sub_pos,
                            egui::Align2::LEFT_TOP,
                            &sub,
                            egui::FontId::proportional(th.font_size_micro.value()),
                            egui::Color32::from(th.text_muted()),
                        );
                        // 디자인 StatusDot(danger): spawn 반복 실패로 자동 비활성화된
                        // plugin 은 행 우측에 빨간 dot 을 그린다. 상세 경고 박스와
                        // 동일하게 enable 상태인 error plugin 에만 표시한다 (사용자가
                        // 끈 plugin 은 정상 종료이므로 error 아님).
                        if entry.health_error && entry.enabled {
                            let dot_center = egui::pos2(rect.max.x - 12.0, rect.center().y);
                            tasty_ui_widgets::paint_badge_dot(
                                ui.painter(),
                                &th,
                                dot_center,
                                tasty_ui_widgets::BadgeVariant::Danger,
                            );
                        }
                        if resp.clicked() {
                            ui_state.selected_id = Some(entry.id.clone());
                        }
                        vspace(ui, STRUCT_GAP_2);
                    }
                });
        });

    // 제거 확인은 같은 plugin id 에 묶인다. 선택이 바뀌면 확인을 취소해, 다시 돌아와도 평소 바가 보인다.
    if ui_state.confirm_uninstall_id.is_some()
        && ui_state.confirm_uninstall_id != ui_state.selected_id
    {
        ui_state.cancel_uninstall_confirm();
    }

    let selected_entry = ui_state
        .selected_id
        .as_ref()
        .and_then(|id| snapshot.plugins.iter().find(|p| &p.id == id))
        .cloned();
    let confirming = selected_entry
        .as_ref()
        .is_some_and(|e| ui_state.confirm_uninstall_id.as_ref() == Some(&e.id));
    let focus_cancel = confirming && std::mem::take(&mut ui_state.confirm_focus_pending);

    let confirm_title = selected_entry
        .as_ref()
        .map(|e| crate::i18n::t_fmt("plugins.uninstall_confirm_title", &e.name))
        .unwrap_or_default();
    let confirm_view = selected_entry
        .as_ref()
        .filter(|_| confirming)
        .map(|e| uninstall_confirm_view(e, &confirm_title, focus_cancel));

    // 액션 바는 상세 열의 여백 밖, 열 폭 전체에 붙인다. 본문만 기본 패널 여백 안에 둔다.
    // 키보드 초점 순서가 화면 순서(본문 → 바)를 따르도록 같은 패널 안에서 본문을 먼저 만든다.
    let panel_frame = egui::Frame::central_panel(&ctx.style());
    let margin = panel_frame.inner_margin;
    egui::CentralPanel::default()
        .frame(panel_frame.inner_margin(egui::Margin::ZERO))
        .show(ctx, |ui| {
            let full = ui.max_rect();
            let bar_h = match (&selected_entry, &confirm_view) {
                (None, _) => 0.0,
                (Some(_), Some(view)) => {
                    plugin_uninstall_confirm_bar_height(ui, &th, view, full.width())
                }
                (Some(_), None) => plugin_detail_bar_height(&th),
            };
            let split = full.max.y - bar_h;
            let body_rect = egui::Rect::from_min_max(
                full.min + egui::vec2(margin.leftf(), margin.topf()),
                egui::pos2(full.max.x - margin.rightf(), split),
            );
            let mut body = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(body_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let Some(entry) = &selected_entry else {
                vspace(&mut body, th.spacing_xl);
                body.label(t("plugins.none_selected"));
                return;
            };
            vspace(&mut body, th.spacing_sm);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(&mut body, |ui| {
                    draw_detail_body(ui, &th, entry, &snapshot.keycap_style, actions);
                });

            let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(
                egui::Rect::from_min_max(egui::pos2(full.min.x, split), full.max),
            ));
            // 제거 확인은 바의 내용을 그 자리에서 바꾼다. 본문 끝 블록이나 popup 을 쓰지 않는다.
            if let Some(view) = &confirm_view {
                let clicks = plugin_uninstall_confirm_bar(&mut bar_ui, &th, view);
                if clicks.uninstall {
                    actions.push(PluginsAction::Uninstall {
                        id: entry.id.clone(),
                    });
                    ui_state.cancel_uninstall_confirm();
                } else if clicks.cancel {
                    ui_state.cancel_uninstall_confirm();
                }
                return;
            }
            let bar = plugin_detail_bar(
                &mut bar_ui,
                &th,
                &PluginDetailBarView {
                    enabled: entry.enabled,
                    enabled_label: t("plugins.enabled"),
                    disabled_label: t("plugins.disabled"),
                    configure: t("plugins.configure"),
                    uninstall: t("plugins.uninstall"),
                },
            );
            if bar.toggled {
                actions.push(PluginsAction::SetEnabled {
                    id: entry.id.clone(),
                    enabled: !entry.enabled,
                });
            }
            if bar.configure {
                actions.push(PluginsAction::OpenSettings);
            }
            if bar.uninstall {
                ui_state.confirm_uninstall_id = Some(entry.id.clone());
                ui_state.confirm_focus_pending = true;
            }
        });
}

/// 제거 확인 바의 문구. 안내는 built-in 여부로 갈린다.
fn uninstall_confirm_view<'a>(
    entry: &super::PluginEntry,
    title: &'a str,
    focus_cancel: bool,
) -> PluginUninstallConfirmView<'a> {
    let note = if entry.builtin {
        t("plugins.uninstall_builtin_note")
    } else {
        t("plugins.uninstall_note")
    };
    PluginUninstallConfirmView {
        title,
        note,
        cancel: t("button.cancel"),
        uninstall: t("plugins.uninstall"),
        focus_cancel,
        busy: None,
        wrap: false,
    }
}

/// 상세 본문 — identity, 설명, 오류 상자, 절들, 설치 경로.
fn draw_detail_body(
    ui: &mut egui::Ui,
    th: &theme::Theme,
    entry: &super::PluginEntry,
    keycap_style: &PluginKeycapStyle,
    actions: &mut Vec<PluginsAction>,
) {
    // identity — 아바타 오른쪽에 이름 줄과 `작성자 · id · homepage` 메타 줄. Attention 과 같은 위젯이다.
    let authors = entry.authors.join(", ");
    let open_homepage = plugin_detail_identity(
        ui,
        th,
        &PluginIdentityView {
            name: &entry.name,
            version: &entry.version,
            builtin_tag: entry.builtin.then(|| t("plugins.builtin_badge")),
            meta: PluginMetaView {
                authors: &authors,
                id: &entry.id,
                homepage: &entry.homepage,
                wrap_long: false,
            },
        },
    );
    if open_homepage {
        actions.push(PluginsAction::OpenHomepage {
            url: entry.homepage.clone(),
        });
    }
    vspace(ui, th.spacing_sm);

    if !entry.description.is_empty() {
        plugin_detail_description(ui, th, &entry.description);
        vspace(ui, th.spacing_sm);
    }

    // 디자인 error 경고 박스: spawn 반복 실패로 자동 비활성화된 plugin 에
    // 빨간 박스로 안내. config 상 enable 상태일 때만 (사용자가 끈 plugin 은
    // 정상 종료이므로 error 가 아님).
    if entry.health_error && entry.enabled {
        let danger = egui::Color32::from(th.accent_danger());
        // tinted 채움/테두리 짝 — `tint-fill-alpha` / `tint-border-alpha`.
        egui::Frame::new()
            .fill(danger.gamma_multiply(th.tint_fill_alpha()))
            .stroke(egui::Stroke::new(
                th.border_width.value(),
                danger.gamma_multiply(th.tint_border_alpha()),
            ))
            .corner_radius(th.corner_radius.value())
            .inner_margin(margin_sym(th.spacing_md, th.spacing_sm))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(t("plugins.health_error"))
                        .color(egui::Color32::from(th.accent_danger_raised())),
                );
            });
        vspace(ui, th.spacing_sm);
    }

    // 디자인 상세는 절 사이에 구분선 없이 space-lg 만 띄운다.
    // 순서: Permissions → Commands → Surface kinds(있을 때만) → 설치 경로.
    plugin_detail_section_gap(ui, th);
    plugin_detail_section(ui, th, t("plugins.permissions"), |ui| {
        if entry.manifest_permissions.is_empty() {
            ui.label(t("plugins.none"));
        } else {
            ui.horizontal_wrapped(|ui| {
                for token in &entry.manifest_permissions {
                    tag_mono(ui, th, token, TagVariant::Default, false);
                }
            });
        }
    });

    if !entry.commands.is_empty() {
        plugin_detail_section_gap(ui, th);
        plugin_detail_section(ui, th, t("plugins.commands"), |ui| {
            for cmd in &entry.commands {
                plugin_command_row(
                    ui,
                    th,
                    t(&cmd.title_key),
                    cmd.keybinding.as_deref(),
                    keycap_style,
                );
            }
        });
    }

    if !entry.surface_kinds.is_empty() {
        plugin_detail_section_gap(ui, th);
        plugin_detail_section(ui, th, t("plugins.surface_kinds"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for kind in &entry.surface_kinds {
                    tag_mono(ui, th, kind, TagVariant::Default, false);
                }
            });
        });
    }

    plugin_detail_section_gap(ui, th);
    let log_line = format!("{}: {}", t("plugins.log_path"), entry.log_path);
    let open_folder = plugin_install_paths(
        ui,
        th,
        &PluginInstallPathsView {
            label: t("plugins.install_path"),
            open_folder: t("plugins.open_folder"),
            install_dir: &entry.install_dir,
            log_line: &log_line,
        },
    );
    if open_folder {
        actions.push(PluginsAction::OpenInstallDir {
            path: entry.install_dir.clone(),
        });
    }
}

#[cfg(test)]
mod tests;
