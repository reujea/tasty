use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Input};

use super::choice::{Choice, choice_select};
use crate::adapters::ui::icons;
use crate::i18n::t;
use crate::settings::Settings;
use tasty_settings::ShellPathStatus;
use tasty_ui_widgets::{SettingsRow, settings_label_column, vspace};

pub fn draw_terminal_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    if let Some((notice, fails)) = shell_notice(settings.general.shell_path_status()) {
        // 대체 셸로 열리는 경우는 경고, 새 터미널이 실패하는 경우는 오류 색이다.
        let color = if fails {
            th.accent_danger_raised()
        } else {
            th.accent_warning_raised()
        };
        ui.label(egui::RichText::new(notice).color(color));
        vspace(ui, th.spacing_xs);
    }

    let shell = SettingsRow::new(t("settings.terminal.shell_label"));
    #[cfg(windows)]
    let shell_mode = SettingsRow::new(t("settings.terminal.shell_mode_label"));
    let startup = SettingsRow::new(t("settings.terminal.startup_command_label"));
    let scrollback_row = SettingsRow::new(t("settings.terminal.scrollback_lines_label"));
    let confirm_close = SettingsRow::new(t("settings.terminal.confirm_close_label"));
    let inherit_cwd = SettingsRow::new(t("settings.terminal.inherit_cwd_label"));
    let reverse_screen = SettingsRow::new(t("settings.terminal.reverse_screen_label"));
    let bell = SettingsRow::new(t("settings.terminal.bell_notification_label"));
    let link_modifier = SettingsRow::new(t("settings.terminal.link_modifier_label"));
    #[cfg(target_os = "macos")]
    let option_as_meta = SettingsRow::new(t("settings.terminal.option_as_meta_label"));
    let col = settings_label_column(
        ui,
        &th,
        [
            &shell,
            #[cfg(windows)]
            &shell_mode,
            &startup,
            &scrollback_row,
            &confirm_close,
            &inherit_cwd,
            &reverse_screen,
            &bell,
            &link_modifier,
            #[cfg(target_os = "macos")]
            &option_as_meta,
        ],
    );
    ui.spacing_mut().item_spacing.y = th.settings_row_gap().value();

    // 쓸 수 없는 경로도 사용자가 고칠 때까지 그대로 보여 준다. 실행 시 대체 셸은 ShellConfig가 고른다.
    shell.show(ui, &th, col, |ui| {
        ui.text_edit_singleline(&mut settings.general.shell);
    });

    // 셸 모드는 Windows 전용 — OSC7/MSYS PATH 빌트인 적용 여부 결정.
    // 비-Windows 에서는 의미가 없어 UI 에서도 노출하지 않는다.
    #[cfg(windows)]
    shell_mode.show(ui, &th, col, |ui| {
        choice_select(
            ui,
            &th,
            "shell_mode",
            &mut settings.general.shell_mode,
            &[
                Choice {
                    value: "default",
                    label: t("settings.terminal.shell_mode_default"),
                },
                Choice {
                    value: "tasty",
                    label: t("settings.terminal.shell_mode_tasty"),
                },
            ],
            th.field_width_lg,
        );
    });

    startup.show(ui, &th, col, |ui| {
        ui.text_edit_singleline(&mut settings.general.startup_command);
    });

    scrollback_row.show(ui, &th, col, |ui| {
        let mut scrollback = settings.general.scrollback_lines as i128;
        if super::number::integer_field(
            ui,
            &th,
            "terminal_scrollback_lines",
            &super::number::NumberSpec::int(0.0, 100_000.0),
            &mut scrollback,
        ) {
            // 확정값은 범위 안이다.
            settings.general.scrollback_lines = usize::try_from(scrollback).unwrap_or(usize::MAX);
        }
    });

    confirm_close.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(
            ui,
            &th,
            &mut settings.general.confirm_close_running,
            None,
            true,
        );
    });

    inherit_cwd.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(ui, &th, &mut settings.general.inherit_cwd, None, true);
    });

    // DECSCNM(화면 반전, mode 5) 렌더 토글. off 면 셸의 visible-bell 등이
    // `\e[?5h` 를 보내도 전체 화면 반전 플래시를 그리지 않는다(모드 플래그는
    // 여전히 추적 — 프로그램 조회 응답은 정상).
    reverse_screen.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(
            ui,
            &th,
            &mut settings.general.reverse_screen_enabled,
            None,
            true,
        );
    });

    // BEL 토스트·소리를 꺼도 사용자가 등록한 bell 훅은 계속 실행한다.
    bell.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(ui, &th, &mut settings.general.bell_notification, None, true);
    });

    link_modifier.show(ui, &th, col, |ui| {
        choice_select(
            ui,
            &th,
            "link_modifier",
            &mut settings.general.link_click_modifier,
            &[
                Choice {
                    value: "ctrl",
                    label: t("settings.terminal.link_modifier_ctrl"),
                },
                Choice {
                    value: "alt",
                    label: t("settings.terminal.link_modifier_alt"),
                },
                Choice {
                    value: "none",
                    label: t("settings.terminal.link_modifier_none"),
                },
            ],
            th.field_width_md,
        );
    });

    // Option as Meta 는 macOS 전용 — 다른 OS 에는 Option 키가 없어 노출하지 않는다.
    #[cfg(target_os = "macos")]
    option_as_meta.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(ui, &th, &mut settings.general.option_as_meta, None, true);
    });
}

/// TUI의 OSC 52 클립보드 읽기 허용 설정과 권한 안내. 기본값은 꺼짐이다.
pub fn draw_terminal_tui_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    // OSC 52 클립보드 읽기 허용 토글과 그 행의 경고 callout(스위치 상태와 관계없이 항상 표시).
    let alert = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        icons::ALERT_TRIANGLE
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    let row = SettingsRow::new(t("settings.terminal.allow_clipboard_read_label"))
        .warning(t("settings.terminal.allow_clipboard_read_notice"), &alert);
    let col = settings_label_column(ui, &th, [&row]);
    row.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(
            ui,
            &th,
            &mut settings.general.allow_clipboard_read,
            None,
            true,
        );
    });
}

/// 마우스 캡처 안내와 제외 목록을 편집한다.
/// "클릭을 Tasty가 처리할 프로그램" 목록과 "캡처 안내를 다시 보지 않을 프로그램" 목록은 별도로 적용한다.
pub fn draw_terminal_mouse_capture_tab(ui: &mut egui::Ui, settings: &mut Settings) {
    let th = crate::theme::theme();
    vspace(ui, th.spacing_sm);

    // 안내 배너 표시 토글.
    let row = SettingsRow::new(t("settings.terminal.mouse_capture_hint_label"));
    let col = settings_label_column(ui, &th, [&row]);
    row.show(ui, &th, col, |ui| {
        tasty_ui_widgets::switch(
            ui,
            &th,
            &mut settings.general.mouse_capture_hint,
            None,
            true,
        );
    });

    // 번역 문구에서 **로 감싼 Shift 부분만 강조한다.
    vspace(ui, th.spacing_xs);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let note = t("settings.terminal.mouse_capture_hint_note");
        // `**` 마커 기준 분할 — 홀수 인덱스 구간이 강조(bold) 대상.
        for (i, seg) in note.split("**").enumerate() {
            if seg.is_empty() {
                continue;
            }
            let mut rich = egui::RichText::new(seg).small().color(th.text_muted());
            if i % 2 == 1 {
                rich = rich.strong();
            }
            ui.label(rich);
        }
    });

    // 안내 토글+Note 블록과 블랙리스트 블록 사이 1px 구분선 (디자인 jsx:613,
    // `--tasty-separator` 색). Theme 의 separator 토큰을 1px stroke 로 전사한다.
    vspace(ui, th.spacing_md);
    ui.scope(|ui| {
        ui.visuals_mut().widgets.noninteractive.bg_stroke = egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        );
        ui.separator();
    });
    vspace(ui, th.spacing_md);

    // 제외 패턴을 편집한다. 패턴의 trim·대소문자·와일드카드 처리는 별도 헬퍼가 한다.
    ui.label(t("settings.terminal.mouse_capture_blacklist_label"));
    vspace(ui, th.spacing_xs);

    if settings.general.mouse_capture_blacklist.is_empty() {
        // 빈 상태 — neutral 톤(경고색 아님).
        ui.label(
            egui::RichText::new(t("settings.terminal.mouse_capture_blacklist_empty"))
                .small()
                .color(th.text_muted()),
        );
    } else {
        let mut remove_idx: Option<usize> = None;
        for (i, pattern) in settings.general.mouse_capture_blacklist.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(pattern)
                        .monospace()
                        .color(th.text_primary()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, &th, &|ui, rect, c| {
                            icons::CLOSE.image(rect.height(), c).paint_at(ui, rect);
                        })
                        .clicked()
                    {
                        remove_idx = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove_idx {
            settings.general.mouse_capture_blacklist.remove(i);
        }
    }

    // Add 행 — 입력 필드(남는 폭) + Add 버튼(입력 비면 disabled). 입력 버퍼는
    // 프레임 간 egui temp memory 에 보관한다(Settings 모델은 확정 패턴만 담는다).
    vspace(ui, th.spacing_sm);
    let add_id = ui.id().with("mouse_capture_blacklist_add");
    let mut add_buf: String = ui.data_mut(|d| d.get_temp::<String>(add_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let can_add = !add_buf.trim().is_empty();
        // 우측 Add 버튼을 먼저 배치 → Input 이 남는 폭을 채운다(디자인 block).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add_clicked =
                Button::new(t("settings.terminal.mouse_capture_blacklist_add_button"))
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .enabled(can_add)
                    .show(ui, &th)
                    .clicked();
            let resp = Input::new()
                .placeholder(t(
                    "settings.terminal.mouse_capture_blacklist_add_placeholder",
                ))
                .mono(true)
                .show(ui, &th, &mut add_buf);
            let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (add_clicked || submit) && !add_buf.trim().is_empty() {
                settings
                    .general
                    .mouse_capture_blacklist
                    .push(add_buf.trim().to_string());
                add_buf.clear();
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(add_id, add_buf));

    // match-rule notice — accent-warning 톤(빈 상태 neutral 아님).
    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.terminal.mouse_capture_blacklist_notice"))
            .small()
            .color(th.accent_warning_raised()),
    );

    // 캡처 블랙리스트와 배너 억제 블랙리스트를 구분하는 1px 구분선(위와 동일 패턴).
    vspace(ui, th.spacing_md);
    ui.scope(|ui| {
        ui.visuals_mut().widgets.noninteractive.bg_stroke = egui::Stroke::new(
            th.border_width.value(),
            th.separator.to_egui_premultiplied(),
        );
        ui.separator();
    });
    vspace(ui, th.spacing_md);

    // 마우스 캡처 "배너만 억제" 블랙리스트 — 캡처 블랙리스트 블록과 동일 패턴(패턴
    // mono + × 제거 + 하단 Add 입력/버튼), 데이터 소스만 `mouse_capture_banner_blacklist`.
    // 매칭 결과가 다르다: 캡처는 그대로 두고 안내 배너만 안 뜨게 한다.
    ui.label(t("settings.terminal.mouse_capture_banner_blacklist_label"));
    vspace(ui, th.spacing_xs);

    if settings.general.mouse_capture_banner_blacklist.is_empty() {
        ui.label(
            egui::RichText::new(t("settings.terminal.mouse_capture_banner_blacklist_empty"))
                .small()
                .color(th.text_muted()),
        );
    } else {
        let mut remove_idx: Option<usize> = None;
        for (i, pattern) in settings
            .general
            .mouse_capture_banner_blacklist
            .iter()
            .enumerate()
        {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(pattern)
                        .monospace()
                        .color(th.text_primary()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, &th, &|ui, rect, c| {
                            icons::CLOSE.image(rect.height(), c).paint_at(ui, rect);
                        })
                        .clicked()
                    {
                        remove_idx = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove_idx {
            settings.general.mouse_capture_banner_blacklist.remove(i);
        }
    }

    vspace(ui, th.spacing_sm);
    let banner_add_id = ui.id().with("mouse_capture_banner_blacklist_add");
    let mut banner_add_buf: String =
        ui.data_mut(|d| d.get_temp::<String>(banner_add_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let can_add = !banner_add_buf.trim().is_empty();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add_clicked = Button::new(t(
                "settings.terminal.mouse_capture_banner_blacklist_add_button",
            ))
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Sm)
            .enabled(can_add)
            .show(ui, &th)
            .clicked();
            let resp = Input::new()
                .placeholder(t(
                    "settings.terminal.mouse_capture_banner_blacklist_add_placeholder",
                ))
                .mono(true)
                .show(ui, &th, &mut banner_add_buf);
            let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (add_clicked || submit) && !banner_add_buf.trim().is_empty() {
                settings
                    .general
                    .mouse_capture_banner_blacklist
                    .push(banner_add_buf.trim().to_string());
                banner_add_buf.clear();
            }
        });
    });
    ui.data_mut(|d| d.insert_temp(banner_add_id, banner_add_buf));

    vspace(ui, th.spacing_xs);
    ui.label(
        egui::RichText::new(t("settings.terminal.mouse_capture_banner_blacklist_notice"))
            .small()
            .color(th.accent_warning_raised()),
    );
}

/// 셸 경로 판정에 맞는 안내와, 새 터미널이 실패하는 판정인지. 쓸 수 있는 셸이면 `None`.
/// macOS·Linux 는 로그인 셸 등을 대신 쓴다. Windows 는 빈 값일 때만 탐지한 Git Bash 를 대신 쓴다.
fn shell_notice(status: ShellPathStatus) -> Option<(&'static str, bool)> {
    match status {
        ShellPathStatus::Usable => None,
        ShellPathStatus::Directory => Some((t("settings.terminal.shell_is_directory"), true)),
        ShellPathStatus::NotExecutable => Some((t("settings.terminal.shell_not_executable"), true)),
        #[cfg(windows)]
        ShellPathStatus::Missing => Some((t("settings.terminal.shell_missing_windows"), true)),
        #[cfg(windows)]
        ShellPathStatus::NotShell => Some((t("settings.terminal.shell_not_git_bash"), true)),
        #[cfg(windows)]
        ShellPathStatus::Empty => crate::settings::GeneralSettings::detect_bash()
            .is_none()
            .then(|| (shell_not_found_notice(), true)),
        #[cfg(not(windows))]
        ShellPathStatus::Empty | ShellPathStatus::Missing | ShellPathStatus::NotShell => {
            Some((shell_not_found_notice(), false))
        }
    }
}

fn shell_not_found_notice() -> &'static str {
    if cfg!(windows) {
        t("settings.terminal.shell_not_found_windows")
    } else {
        t("settings.terminal.shell_not_found_unix")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Git Bash 안내는 Windows 에서만 쓴다. 다른 OS 는 대체 셸을 찾는다.
    #[test]
    fn the_shell_notice_mentions_git_bash_only_on_windows() {
        let notice = shell_not_found_notice();
        assert_eq!(notice.contains("Git Bash"), cfg!(windows), "{notice}");
    }

    /// Windows 는 빈 값 말고는 대체하지 않는다. 판정마다 원인을 말하는 실패 안내다.
    #[cfg(windows)]
    #[test]
    fn the_windows_shell_notice_reports_the_cause_as_a_failure() {
        assert_eq!(shell_notice(ShellPathStatus::Usable), None);
        assert_eq!(
            shell_notice(ShellPathStatus::Missing),
            Some((t("settings.terminal.shell_missing_windows"), true))
        );
        assert_eq!(
            shell_notice(ShellPathStatus::NotShell),
            Some((t("settings.terminal.shell_not_git_bash"), true))
        );
        let empty = crate::settings::GeneralSettings::detect_bash()
            .is_none()
            .then(|| (shell_not_found_notice(), true));
        assert_eq!(shell_notice(ShellPathStatus::Empty), empty);
    }

    /// 대체 셸로 열리는 판정은 경고, 실행할 수 없는 경로는 원인별 실패 안내다.
    #[cfg(not(windows))]
    #[test]
    fn the_shell_notice_tells_a_fallback_from_a_failure() {
        assert_eq!(shell_notice(ShellPathStatus::Usable), None);
        for status in [
            ShellPathStatus::Empty,
            ShellPathStatus::Missing,
            ShellPathStatus::NotShell,
        ] {
            assert_eq!(
                shell_notice(status),
                Some((shell_not_found_notice(), false))
            );
        }
        assert_eq!(
            shell_notice(ShellPathStatus::Directory),
            Some((t("settings.terminal.shell_is_directory"), true))
        );
        assert_eq!(
            shell_notice(ShellPathStatus::NotExecutable),
            Some((t("settings.terminal.shell_not_executable"), true))
        );
    }

    use crate::view::settings::ui::select_probe as probe;

    fn draw(settings: &mut Settings) -> impl FnMut(&mut egui::Ui) + '_ {
        move |ui| draw_terminal_tab(ui, settings)
    }

    /// 링크 수식키 트리거는 Select 상자(높이 select-height, 폭 field-width-md)다.
    #[test]
    fn link_modifier_trigger_is_a_medium_select() {
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let out = probe::frame(&ctx, &mut draw(&mut settings));
        let link = probe::trigger_around(&out, t("settings.terminal.link_modifier_ctrl"))
            .expect("link modifier is drawn as a Select trigger");
        assert_eq!(link.height(), th.select_height().value());
        assert_eq!(link.width(), th.field_width_md.value());
    }

    /// 고른 옵션은 기존과 같은 문자열로 저장되고, 목록 밖 값은 0번 옵션으로 바로잡을 수 있다.
    #[test]
    fn picking_link_modifier_stores_the_same_strings() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.general.link_click_modifier = "meta".into();
        let missing = format!("meta {}", t("settings.choice_missing_suffix"));
        let out = probe::frame(&ctx, &mut draw(&mut settings));
        assert!(probe::trigger_around(&out, &missing).is_some());
        assert_eq!(settings.general.link_click_modifier, "meta");
        for (from, to, stored) in [
            (
                missing.as_str(),
                "settings.terminal.link_modifier_ctrl",
                "ctrl",
            ),
            (
                t("settings.terminal.link_modifier_ctrl"),
                "settings.terminal.link_modifier_alt",
                "alt",
            ),
            (
                t("settings.terminal.link_modifier_alt"),
                "settings.terminal.link_modifier_none",
                "none",
            ),
        ] {
            probe::pick(&ctx, &mut draw(&mut settings), from, t(to));
            assert_eq!(settings.general.link_click_modifier, stored);
        }
    }

    /// Windows 셸 모드 트리거는 Select 상자(폭 field-width-lg)이고 기존 문자열로 저장한다.
    #[cfg(windows)]
    #[test]
    fn shell_mode_trigger_is_a_large_select() {
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.general.shell_mode = "default".into();
        let out = probe::frame(&ctx, &mut draw(&mut settings));
        let mode = probe::trigger_around(&out, t("settings.terminal.shell_mode_default"))
            .expect("shell mode is drawn as a Select trigger");
        assert_eq!(mode.height(), th.select_height().value());
        assert_eq!(mode.width(), th.field_width_lg.value());
        probe::pick(
            &ctx,
            &mut draw(&mut settings),
            t("settings.terminal.shell_mode_default"),
            t("settings.terminal.shell_mode_tasty"),
        );
        assert_eq!(settings.general.shell_mode, "tasty");
    }
}
