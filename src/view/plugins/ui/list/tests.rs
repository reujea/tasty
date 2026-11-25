use super::*;
use crate::view::plugins::ui::PluginEntry;
use crate::view::plugins::ui::text_probe::visible_text_rects;

/// 공백 없이 어느 글꼴에서도 720·880 창의 상세 열보다 긴 경로.
const INSTALL_DIR: &str = "/home/tasty/.local/share/tasty/plugins/com.example.image-viewer-with-a-long-plugin-identifier/versions/1.0.0-with-a-long-build-suffix/unpacked-bundle-directory";

fn entry() -> PluginEntry {
    PluginEntry {
        id: "com.example.image-viewer-with-a-long-plugin-identifier".into(),
        name: "Image viewer".into(),
        version: "1.0.0".into(),
        description: String::new(),
        authors: Vec::new(),
        homepage: String::new(),
        enabled: true,
        running: false,
        health_error: false,
        builtin: false,
        surface_kinds: Vec::new(),
        commands: Vec::new(),
        log_path: format!("{INSTALL_DIR}/plugin.log"),
        install_dir: INSTALL_DIR.into(),
    }
}

/// 긴 설치 경로에서도 `Open folder` 와 경로 전체가 창 안에 보인다. 버튼을 경로 줄에 두거나
/// 경로를 한 줄로 늘이면 720·880 폭에서 둘 중 하나가 창 밖으로 나간다.
#[test]
fn open_folder_and_long_paths_stay_inside_narrow_windows() {
    // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
    crate::i18n::init("en");
    let snapshot = PluginsSnapshot {
        plugins: vec![entry()],
        ..Default::default()
    };
    let log_line = format!("{}: {INSTALL_DIR}/plugin.log", t("plugins.log_path"));
    for width in [720.0, 880.0] {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 1200.0));
        let mut ui_state = PluginsUiState::default();
        let mut actions = Vec::new();
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let output = ctx.run(raw, |ctx| {
            draw_list_tab(ctx, &snapshot, &mut ui_state, &mut actions);
        });
        let texts = visible_text_rects(&output, screen);
        for label in [
            t("plugins.open_folder").to_string(),
            INSTALL_DIR.into(),
            log_line.clone(),
        ] {
            assert!(
                texts.iter().any(|(text, _)| *text == label),
                "width {width}: {label:?} is not wholly visible inside the window"
            );
        }
    }
}

/// 상세가 창보다 길어도 액션 바(스위치 라벨 · Configure · Uninstall)는 본문 스크롤 밖에 남아 창 안에 보인다.
#[test]
fn the_action_bar_stays_inside_the_window_below_a_long_detail() {
    crate::i18n::init("en");
    let mut long = entry();
    long.description = "A plugin with a long detail. ".repeat(40);
    long.surface_kinds = (0..30).map(|i| format!("kind-{i}")).collect();
    long.commands = (0..20)
        .map(|i| crate::view::plugins::ui::PluginCommandEntry {
            title_key: format!("command {i}"),
            keybinding: Some("Ctrl+Alt+G".into()),
        })
        .collect();
    let snapshot = PluginsSnapshot {
        plugins: vec![long],
        ..Default::default()
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
    let mut ui_state = PluginsUiState::default();
    let mut actions = Vec::new();
    let ctx = egui::Context::default();
    let mut output = None;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        output = Some(ctx.run(raw, |ctx| {
            draw_list_tab(ctx, &snapshot, &mut ui_state, &mut actions);
        }));
    }
    let texts = visible_text_rects(&output.expect("drawn"), screen);
    for label in [
        t("plugins.enabled"),
        t("plugins.configure"),
        t("plugins.uninstall"),
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == label),
            "{label:?} is not wholly visible inside the window"
        );
    }
}

/// Uninstall 을 누르면 액션 바 자리가 그 자리에서 확인 바로 바뀐다. 긴 상세에서도 질문·안내·두 버튼이
/// 창 안에 보이고, 평소 바의 스위치 라벨은 사라진다. 다른 plugin 의 확인 상태면 평소 바를 그린다.
#[test]
fn uninstall_confirmation_replaces_the_action_bar_in_place() {
    crate::i18n::init("en");
    let mut long = entry();
    long.description = "A plugin with a long detail. ".repeat(40);
    long.surface_kinds = (0..30).map(|i| format!("kind-{i}")).collect();
    let id = long.id.clone();
    let name = long.name.clone();
    let snapshot = PluginsSnapshot {
        plugins: vec![long],
        ..Default::default()
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
    let draw = |confirm: Option<String>| {
        let ctx = egui::Context::default();
        let mut ui_state = PluginsUiState {
            confirm_uninstall_id: confirm,
            ..Default::default()
        };
        let mut actions = Vec::new();
        let mut output = None;
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            output = Some(ctx.run(raw, |ctx| {
                draw_list_tab(ctx, &snapshot, &mut ui_state, &mut actions);
            }));
        }
        visible_text_rects(&output.expect("drawn"), screen)
    };
    let has = |texts: &[(String, egui::Rect)], label: &str| texts.iter().any(|(t, _)| t == label);

    let confirming = draw(Some(id));
    let title = crate::i18n::t_fmt("plugins.uninstall_confirm_title", &name);
    for label in [
        title.as_str(),
        t("plugins.uninstall_note"),
        t("button.cancel"),
        t("plugins.uninstall"),
    ] {
        assert!(
            has(&confirming, label),
            "{label:?} is not wholly visible in the confirm bar"
        );
    }
    assert!(
        !has(&confirming, t("plugins.enabled")),
        "the normal bar is still drawn while confirming"
    );

    let other = draw(Some("another.plugin".into()));
    assert!(has(&other, t("plugins.enabled")));
    assert!(!has(&other, &title));
}

/// 두 plugin 이 있는 목록을 `events` 를 넣어 한 프레임 그린다.
fn run_frame(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
    let raw = egui::RawInput {
        screen_rect: Some(screen),
        events,
        ..Default::default()
    };
    let output = ctx.run(raw, |ctx| draw_list_tab(ctx, snapshot, ui_state, actions));
    visible_text_rects(&output, screen)
}

fn two_plugins() -> PluginsSnapshot {
    let mut b = entry();
    b.id = "com.example.second".into();
    b.name = "Second".into();
    PluginsSnapshot {
        plugins: vec![entry(), b],
        ..Default::default()
    }
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 다른 plugin 을 고르면 제거 확인이 취소되어, 원래 plugin 으로 돌아와도 평소 바가 보인다.
#[test]
fn changing_the_selection_cancels_the_uninstall_confirm() {
    crate::i18n::init("en");
    let snapshot = two_plugins();
    let first = snapshot.plugins[0].id.clone();
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    let mut ui_state = PluginsUiState {
        selected_id: Some(first.clone()),
        confirm_uninstall_id: Some(first.clone()),
        ..Default::default()
    };
    run_frame(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
    assert_eq!(
        ui_state.confirm_uninstall_id.as_deref(),
        Some(first.as_str())
    );

    ui_state.selected_id = Some(snapshot.plugins[1].id.clone());
    run_frame(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
    ui_state.selected_id = Some(first);
    let texts = run_frame(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
    assert_eq!(ui_state.confirm_uninstall_id, None);
    assert!(texts.iter().any(|(t, _)| t == t_enabled()), "{texts:?}");
}

fn t_enabled() -> &'static str {
    t("plugins.enabled")
}

/// 확인이 열리면 Cancel 에 포커스가 간다. 이어서 Enter 를 누르면 제거 대신 확인이 닫힌다.
#[test]
fn the_uninstall_confirm_focuses_cancel_when_it_opens() {
    crate::i18n::init("en");
    let snapshot = two_plugins();
    let first = snapshot.plugins[0].id.clone();
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    let mut ui_state = PluginsUiState {
        selected_id: Some(first.clone()),
        confirm_uninstall_id: Some(first),
        confirm_focus_pending: true,
        ..Default::default()
    };
    run_frame(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
    assert!(!ui_state.confirm_focus_pending);
    assert!(ctx.memory(|m| m.focused()).is_some(), "nothing has focus");
    run_frame(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        vec![key(egui::Key::Enter)],
    );
    run_frame(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
    assert_eq!(ui_state.confirm_uninstall_id, None);
    assert!(
        actions.is_empty(),
        "Enter uninstalled instead of cancelling: {actions:?}"
    );
}

/// Commands 절 키캡은 snapshot 의 표시 스타일을 따른다. macOS 낱말 표기면 매니페스트 `alt` 가 `Cmd`
/// 로 보이고, 기본(비-macOS)이면 `Alt` 로 보인다.
#[test]
fn command_keycaps_follow_the_snapshot_style() {
    crate::i18n::init("en");
    let mut plugin = entry();
    plugin.commands = vec![crate::view::plugins::ui::PluginCommandEntry {
        title_key: "plugins.commands".into(),
        keybinding: Some("alt + k".into()),
    }];
    let id = plugin.id.clone();
    let draw = |style: tasty_ui_widgets::PluginKeycapStyle| {
        let snapshot = PluginsSnapshot {
            plugins: vec![plugin.clone()],
            keycap_style: style,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut ui_state = PluginsUiState {
            selected_id: Some(id.clone()),
            ..Default::default()
        };
        let mut actions = Vec::new();
        let mut texts = Vec::new();
        for _ in 0..2 {
            texts = run_frame(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
        }
        texts
    };
    let has = |texts: &[(String, egui::Rect)], label: &str| texts.iter().any(|(t, _)| t == label);
    let mac = draw(tasty_ui_widgets::PluginKeycapStyle::from_setting_names(
        true, "cmd", "option", "shift",
    ));
    assert!(has(&mac, "Cmd") && !has(&mac, "Alt"), "{mac:?}");
    let other = draw(tasty_ui_widgets::PluginKeycapStyle::default());
    assert!(has(&other, "Alt") && !has(&other, "Cmd"), "{other:?}");
}

/// 목록 패널은 첫 행 위에 `spacing_sm` 만 둔다(정본 목록 상자 위 여백 space-sm). 행 안 이름은 그 행 위에서 다시
/// `spacing_sm` 아래다.
#[test]
fn the_installed_list_starts_space_sm_below_the_panel_top() {
    crate::i18n::init("en");
    let th = theme::theme();
    let snapshot = PluginsSnapshot {
        plugins: vec![entry()],
        ..Default::default()
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
    let ctx = egui::Context::default();
    let mut ui_state = PluginsUiState::default();
    let mut actions = Vec::new();
    let mut output = None;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        output = Some(ctx.run(raw, |ctx| {
            draw_list_tab(ctx, &snapshot, &mut ui_state, &mut actions);
        }));
    }
    let texts = visible_text_rects(&output.expect("drawn"), screen);
    let name = texts
        .iter()
        .find(|(text, _)| text == "Image viewer")
        .map(|(_, r)| *r)
        .expect("목록 행 이름이 없다");
    let want = th.spacing_sm.value() * 2.0;
    assert!(
        (name.top() - screen.top() - want).abs() < 0.5,
        "목록 첫 행 이름 {name:?} 의 위가 패널 위에서 spacing_sm × 2 = {want} 가 아니다"
    );
}
