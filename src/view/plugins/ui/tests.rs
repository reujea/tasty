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
    let mut raw = egui::RawInput {
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
    // 창 `render`처럼 W1 값을 넣고 bare Esc 는 사건 입구처럼 키 주인에 넘긴다.
    crate::view::window_input::WindowPopups::window_popups(&mut ui_state.folder_chooser)
        .raw_pass_keys(ctx, owner, &mut raw.events);
    drop(ctx.run(raw, |ctx| {
        draw_plugins_panel(ctx, snapshot, ui_state, actions);
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
    let mut raw = egui::RawInput {
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
    // 창 `render`처럼 W1 값을 넣고 bare Esc 는 사건 입구처럼 키 주인에 넘긴다.
    crate::view::window_input::WindowPopups::window_popups(&mut ui_state.folder_chooser)
        .raw_pass_keys(ctx, owner, &mut raw.events);
    ctx.run(raw, |ctx| {
        draw_plugins_panel(ctx, snapshot, ui_state, actions);
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

/// Given 폴더 선택 창이 열리고 패스 시작 포커스가 창의 필터 입력칸, When Esc, Then 창을 덮은 폴더 선택 창이 키
/// 주인이라 제거 확인은 열린 채다.
#[test]
fn uninstall_confirm_ignores_escape_over_an_outside_focus_under_the_chooser() {
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
    assert!(
        next_owner(&ctx, &state).is_some(),
        "the covering chooser owns the keys over the filter focus"
    );
    run_window(&ctx, &snapshot, &mut state, &mut actions, vec![escape()]);
    assert_eq!(
        state.confirm_uninstall_id.as_deref(),
        Some("com.example.first")
    );
    assert!(actions.is_empty());
}

/// 플러그인 창 탭 막대의 탭은 Tab으로 포커스를 받고 탭에서 focus-ring-offset 띄운 border-focus 링을 그린다
/// (정본 window-tab-focus.jsx 플러그인 탭).
#[test]
fn tab_rings_the_plugin_segment_tab_one_offset_off_its_box() {
    let th = theme::Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let ctx = egui::Context::default();
    let tab = egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    };
    let frame = |events: Vec<egui::Event>| {
        let mut cells = Vec::new();
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 120.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    ui.horizontal(|ui| {
                        for (i, l) in ["Installed", "Attention", "Add plugin"].iter().enumerate() {
                            let left = ui.cursor().min.x;
                            segment_tab(ui, &th, l, None, false, i == 0);
                            cells.push(egui::Rect::from_x_y_ranges(
                                left..=ui.min_rect().right(),
                                ui.min_rect().y_range(),
                            ));
                        }
                    });
                });
            },
        );
        let rings: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r)
                    if r.stroke.width == th.focus_ring_width.value()
                        && r.stroke_kind == egui::StrokeKind::Outside =>
                {
                    Some((r.rect, r.stroke.color))
                }
                _ => None,
            })
            .collect();
        (cells, rings)
    };
    let (_, rest) = frame(vec![]);
    assert!(rest.is_empty(), "{rest:?}");
    frame(vec![tab.clone()]);
    frame(vec![tab]);
    let (cells, rings) = frame(vec![]);
    let gap = th.focus_ring_offset.value();
    assert_eq!(rings.len(), 1, "{rings:?}");
    assert_eq!(rings[0].1, th.border_focus().to_egui());
    assert_eq!(rings[0].0.left(), cells[1].left() - gap);
}

/// 창 전체를 두 번 그려(글리프 텍스처가 자리 잡게) 헤더 밴드 안에 그린 도형을 모은다.
fn painted_header() -> (theme::Theme, Vec<egui::Shape>) {
    let th = theme::theme().clone();
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 칠해지지 않는다.
    egui_extras::install_image_loaders(&ctx);
    let snapshot = PluginsSnapshot {
        plugins: vec![plugin("com.example.first"), plugin("com.example.second")],
        ..Default::default()
    };
    let mut state = PluginsUiState::default();
    let mut actions = Vec::new();
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let out = draw_window(&ctx, &snapshot, &mut state, &mut actions);
        shapes = out.shapes.into_iter().map(|c| c.shape).collect();
    }
    let band = th.plugins_header_height().value();
    shapes.retain(|s| s.visual_bounding_rect().bottom() <= band);
    (th, shapes)
}

/// 헤더 plug 글리프는 정본 `icon-size-md` 크기로 그린다.
#[test]
fn the_header_glyph_is_icon_size_md() {
    let (th, shapes) = painted_header();
    let glyph = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_some() => Some(r.rect),
            _ => None,
        })
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("헤더 글리프가 없다");
    let md = th.icon_glyph_size_md.value();
    assert_eq!(
        (glyph.width(), glyph.height()),
        (md, md),
        "헤더 글리프 {glyph:?} 가 icon_glyph_size_md {md} 정사각이 아니다"
    );
}

/// 세그먼트 탭 하나를 그리고 그 글자 도형(왼쪽 위치, 배치 폭)을 문자열로 돌려준다.
fn segment_tab_texts(th: &theme::Theme, count: usize) -> Vec<(String, f32, f32)> {
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..2 {
        let out = ctx.run(egui::RawInput::default(), |c| {
            egui::CentralPanel::default().show(c, |ui| {
                segment_tab(ui, th, "Installed", Some(count), false, true);
            });
        });
        texts = out
            .shapes
            .into_iter()
            .filter_map(|s| match s.shape {
                egui::Shape::Text(t) => {
                    Some((t.galley.text().to_string(), t.pos.x, t.galley.size().x))
                }
                _ => None,
            })
            .collect();
    }
    texts
}

/// 세그먼트 탭의 개수 글자는 라벨 오른쪽에서 정본 `space-sm` 만큼 떨어져 시작한다.
#[test]
fn a_segment_tab_count_starts_space_sm_after_its_label() {
    for zoom in [0.85, 1.0, 1.2] {
        let th =
            theme::Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
        let texts = segment_tab_texts(&th, 7);
        let find = |needle: &str| {
            texts
                .iter()
                .find(|(t, _, _)| t == needle)
                .unwrap_or_else(|| panic!("{needle:?} 글자 도형이 없다: {texts:?}"))
        };
        let (_, label_x, label_w) = find("Installed");
        let (_, count_x, _) = find("7");
        let gap = count_x - (label_x + label_w);
        assert!(
            (gap - th.spacing_sm.value()).abs() < 0.01,
            "배율 {zoom}: 라벨과 개수 사이 {gap} 가 spacing_sm {} 가 아니다",
            th.spacing_sm.value()
        );
    }
}
