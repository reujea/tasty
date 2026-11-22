use super::*;

fn plugin(id: &str) -> PluginEntry {
    PluginEntry {
        id: id.into(),
        name: id.into(),
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
        log_path: String::new(),
        install_dir: String::new(),
    }
}

/// 창 전체를 한 프레임 그린다. `events` 를 넣고, 그린 뒤 Esc 가 아직 남아 있는지 돌려준다.
fn run_window(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
    events: Vec<egui::Event>,
) -> bool {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(720.0, 480.0),
        )),
        events,
        ..Default::default()
    };
    let mut esc_left = false;
    // 창 `render`처럼 직전 패스 뒤 포커스 기록으로 키 주인을 정해 패스마다 넣는다.
    let owner = ui_state.folder_chooser.key_owner(
        ctx,
        crate::adapters::ui::popup::window_keys::WindowFocusRecord::read(ctx),
    );
    // 창 `render`처럼 W1 값과 이번 입력의 Esc 표지(모두 수식키 없음)를 묶는다.
    let keys = crate::adapters::ui::popup::escape_step::WindowPassKeys {
        owner,
        marks: crate::adapters::ui::popup::body_keys::escape_marks_in(
            &raw.events,
            0,
            tasty_key_match::KeyMods::default(),
        ),
    };
    drop(ctx.run(raw, |ctx| {
        draw_plugins_panel(ctx, snapshot, ui_state, actions, &keys);
        esc_left = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    }));
    esc_left
}

fn escape() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn confirming(tab: PluginsTab) -> (PluginsSnapshot, PluginsUiState) {
    let snapshot = PluginsSnapshot {
        plugins: vec![plugin("com.example.first"), plugin("com.example.second")],
        ..Default::default()
    };
    let ui_state = PluginsUiState {
        active_tab: tab,
        selected_id: Some("com.example.first".into()),
        confirm_uninstall_id: Some("com.example.first".into()),
        ..Default::default()
    };
    (snapshot, ui_state)
}

/// Installed 탭에서 Esc 는 제거 확인을 닫고 소비된다. 제거 동작은 나가지 않는다.
#[test]
fn escape_cancels_the_uninstall_confirm() {
    crate::i18n::init("en");
    let (snapshot, mut ui_state) = confirming(PluginsTab::List);
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    run_window(&ctx, &snapshot, &mut ui_state, &mut actions, Vec::new());
    let left = run_window(&ctx, &snapshot, &mut ui_state, &mut actions, vec![escape()]);
    assert_eq!(ui_state.confirm_uninstall_id, None);
    assert!(!left, "Esc was not consumed");
    assert!(actions.is_empty(), "{actions:?}");
}

/// 확인이 열린 채 다른 탭으로 옮겨도 Esc 는 가려진 확인을 닫는다. Installed 로 돌아와도 확인이
/// 다시 보이지 않는다.
#[test]
fn escape_cancels_a_confirm_hidden_behind_another_tab() {
    crate::i18n::init("en");
    for tab in [PluginsTab::Attention, PluginsTab::Add] {
        let (snapshot, mut ui_state) = confirming(tab);
        let ctx = egui::Context::default();
        let mut actions = Vec::new();
        let left = run_window(&ctx, &snapshot, &mut ui_state, &mut actions, vec![escape()]);
        assert_eq!(ui_state.confirm_uninstall_id, None, "{tab:?}");
        assert!(!left, "{tab:?}: Esc was not consumed");
    }
}

/// 확인이 없으면 Esc 를 소비하지 않는다. 다른 소비자가 받을 수 있게 남겨 둔다.
#[test]
fn escape_is_left_alone_without_a_confirm() {
    crate::i18n::init("en");
    for tab in [PluginsTab::List, PluginsTab::Attention, PluginsTab::Add] {
        let (snapshot, mut ui_state) = confirming(tab);
        ui_state.confirm_uninstall_id = None;
        let ctx = egui::Context::default();
        let mut actions = Vec::new();
        let left = run_window(&ctx, &snapshot, &mut ui_state, &mut actions, vec![escape()]);
        assert!(left, "{tab:?}: Esc was consumed without a confirm");
    }
}

#[test]
fn folder_chooser_owns_escape_before_the_uninstall_confirm() {
    crate::i18n::init("en");
    let (snapshot, mut state) = confirming(PluginsTab::List);
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    assert!(state.folder_chooser.open(
        "plugins.add",
        crate::view::file_chooser::FileChooserMode::Folder,
        ""
    ));
    run_window(&ctx, &snapshot, &mut state, &mut actions, Vec::new());
    let left = run_window(&ctx, &snapshot, &mut state, &mut actions, vec![escape()]);
    assert!(!left);
    assert!(!state.folder_chooser.is_open());
    assert_eq!(
        state.confirm_uninstall_id.as_deref(),
        Some("com.example.first")
    );
    assert!(actions.is_empty());
    run_window(&ctx, &snapshot, &mut state, &mut actions, vec![escape()]);
    assert!(state.confirm_uninstall_id.is_none());
}

/// 창 전체를 한 프레임 그리고 그 출력을 돌려준다. 키 주인은 `run_window`와 같이 넣는다.
fn draw_window(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(720.0, 480.0),
        )),
        ..Default::default()
    };
    let owner = ui_state.folder_chooser.key_owner(
        ctx,
        crate::adapters::ui::popup::window_keys::WindowFocusRecord::read(ctx),
    );
    // 창 `render`처럼 W1 값과 이번 입력의 Esc 표지(모두 수식키 없음)를 묶는다.
    let keys = crate::adapters::ui::popup::escape_step::WindowPassKeys {
        owner,
        marks: crate::adapters::ui::popup::body_keys::escape_marks_in(
            &raw.events,
            0,
            tasty_key_match::KeyMods::default(),
        ),
    };
    ctx.run(raw, |ctx| {
        draw_plugins_panel(ctx, snapshot, ui_state, actions, &keys);
    })
}

fn next_owner(
    ctx: &egui::Context,
    ui_state: &PluginsUiState,
) -> Option<crate::adapters::ui::popup::PopupInstanceId> {
    ui_state.folder_chooser.key_owner(
        ctx,
        crate::adapters::ui::popup::window_keys::WindowFocusRecord::read(ctx),
    )
}

fn open_chooser(state: &mut PluginsUiState) {
    assert!(state.folder_chooser.open(
        "plugins.add",
        crate::view::file_chooser::FileChooserMode::Folder,
        ""
    ));
}

/// Given 폴더 선택 창이 키 주인인 제거 확인, When Esc, Then 제거 확인은 Esc 를 보지 않는다.
#[test]
fn uninstall_confirm_ignores_escape_while_the_chooser_owns_keys() {
    crate::i18n::init("en");
    let (snapshot, mut state) = confirming(PluginsTab::List);
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    open_chooser(&mut state);
    run_window(&ctx, &snapshot, &mut state, &mut actions, Vec::new());
    assert!(
        next_owner(&ctx, &state).is_some(),
        "the chooser owns the keys"
    );
    run_window(&ctx, &snapshot, &mut state, &mut actions, vec![escape()]);
    assert_eq!(
        state.confirm_uninstall_id.as_deref(),
        Some("com.example.first")
    );
    assert!(actions.is_empty());
}

/// Given 폴더 선택 창이 열리고 패스 시작 포커스가 창의 필터 입력칸, When Esc, Then 제거 확인이 Esc 를 받아 닫히고
/// 폴더 선택 창은 열린 채다.
#[test]
fn uninstall_confirm_takes_escape_when_focus_is_outside_the_chooser() {
    crate::i18n::init("en");
    let (snapshot, mut state) = confirming(PluginsTab::List);
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    open_chooser(&mut state);
    let out = draw_window(&ctx, &snapshot, &mut state, &mut actions);
    let filter = crate::adapters::ui::popup::window_keys::focusable_at_text(
        &ctx,
        &out,
        t("plugins.filter_placeholder"),
        egui::Order::Background,
    );
    ctx.memory_mut(|m| m.request_focus(filter));
    drop(draw_window(&ctx, &snapshot, &mut state, &mut actions));
    assert_eq!(
        next_owner(&ctx, &state),
        None,
        "the pass starts at the filter"
    );
    run_window(&ctx, &snapshot, &mut state, &mut actions, vec![escape()]);
    assert_eq!(state.confirm_uninstall_id, None);
    assert!(state.folder_chooser.is_open());
    assert!(actions.is_empty());
}
