use super::*;
use crate::view::plugins::ui::text_probe::visible_text_rects;

fn entry(kind: AttentionKind) -> AttentionEntry {
    AttentionEntry {
        id: "com.example.log-tailer".into(),
        name: "Log tailer".into(),
        version: "0.1.4".into(),
        authors: vec!["example".into()],
        description: "Tails log files.".into(),
        homepage: "https://example.com/log-tailer".into(),
        builtin: false,
        kind,
        fingerprint: None,
        permissions_added: (0..40).map(|i| format!("perm:{i}")).collect(),
        permissions_removed: Vec::new(),
        health_detail: None,
        cause: None,
        unverified: None,
        manifest_sha256: String::new(),
        folder: None,
    }
}

fn reserved(folder: &str, id: &str) -> AttentionEntry {
    AttentionEntry {
        id: id.into(),
        name: folder.into(),
        version: String::new(),
        authors: Vec::new(),
        description: String::new(),
        homepage: String::new(),
        builtin: false,
        kind: AttentionKind::ReservedId,
        fingerprint: None,
        permissions_added: Vec::new(),
        permissions_removed: Vec::new(),
        health_detail: None,
        cause: None,
        unverified: None,
        manifest_sha256: String::new(),
        folder: Some(std::path::PathBuf::from(format!("/plugins/{folder}"))),
    }
}

fn draw(entry: AttentionEntry) -> Vec<(String, egui::Rect)> {
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
    visible_text_rects(&draw_output(entry), screen)
}

fn draw_output(entry: AttentionEntry) -> egui::FullOutput {
    let snapshot = PluginsSnapshot {
        attention: vec![entry],
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
            draw_attention_tab(ctx, &snapshot, &mut ui_state, &mut actions);
        }));
    }
    output.expect("drawn")
}

/// Attention 의 액션 바도 Installed 처럼 본문 스크롤 밖 열 바닥에 붙어, 상세가 길어도 상태 문구와
/// 사유별 버튼이 창 안에 보인다. 정체 블록의 메타 줄은 `작성자 · id · homepage` 다.
#[test]
fn the_attention_bar_stays_inside_the_window_below_a_long_detail() {
    crate::i18n::init("en");
    let texts = draw(entry(AttentionKind::PermissionsChanged));
    let has = |label: &str| texts.iter().any(|(t, _)| t == label);
    for label in [
        t("plugins.attn_needs_review"),
        t("plugins.attn_reapprove"),
        "example",
        "com.example.log-tailer",
        "example.com/log-tailer",
    ] {
        assert!(
            has(label),
            "{label:?} is not wholly visible inside the window"
        );
    }
}

/// 상세는 정체 블록 · 설명 · 사유 배너 · 사유 detail 을 `space-lg` 간격으로 쌓는다. 배너 안 글은
/// 테두리와 `space-md` 여백 안쪽에 있으므로 글 사이 거리에서 그 몫을 뺀다.
#[test]
fn the_attention_detail_stacks_its_blocks_a_large_space_apart() {
    crate::i18n::init("en");
    let th = theme::theme();
    let texts = draw(entry(AttentionKind::PermissionsChanged));
    // 목록 행에도 사유 문구가 있으므로 설명과 같은 열(상세 열)에서 시작하는 글자만 본다.
    let detail_x = texts
        .iter()
        .find(|(t, _)| t == "Tails log files.")
        .map(|(_, r)| r.min.x)
        .expect("description is drawn");
    let rect = |label: &str| {
        texts
            .iter()
            .find(|(t, r)| t.eq_ignore_ascii_case(label) && r.min.x >= detail_x - 0.5)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{label:?} is not drawn"))
    };
    let inset = th.border_width.value() + th.spacing_md.value();
    let lg = th.spacing_lg.value();
    let description = rect("Tails log files.");
    let title = rect(t("plugins.attn_perm_changed_label"));
    let blurb = rect(t("plugins.attn_perm_changed_blurb"));
    let header = rect(t("plugins.attn_permission_changes"));
    for (what, gap, want) in [
        (
            "description → banner",
            title.min.y - description.max.y,
            lg + inset,
        ),
        ("banner → detail", header.min.y - blurb.max.y, inset + lg),
    ] {
        assert!((gap - want).abs() < 0.5, "{what}: {gap} != {want}");
    }
}

/// 서명이 깨진 번들의 매니페스트 글은 보이지 않는다. 나머지 사유는 보인다.
#[test]
fn only_a_broken_signature_hides_the_manifest_text() {
    assert!(!AttentionKind::SignatureInvalid.shows_manifest_text());
    for kind in [
        AttentionKind::UnknownKey,
        AttentionKind::PermissionsChanged,
        AttentionKind::HealthError,
    ] {
        assert!(kind.shows_manifest_text(), "{kind:?}");
    }
}

/// 목록 행 끝의 severity 점과 액션 바의 점은 같은 status-dot-size 지름이다.
#[test]
fn every_severity_dot_is_status_dot_sized() {
    crate::i18n::init("en");
    let th = theme::theme();
    let color = egui::Color32::from(th.accent_warning());
    let out = draw_output(entry(AttentionKind::PermissionsChanged));
    fn walk(shape: &egui::Shape, color: egui::Color32, out: &mut Vec<f32>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, color, out)),
            egui::Shape::Circle(c) if c.fill == color => out.push(c.radius),
            _ => {}
        }
    }
    let mut radii = Vec::new();
    for c in &out.shapes {
        walk(&c.shape, color, &mut radii);
    }
    assert_eq!(radii.len(), 2, "list row dot and action bar dot: {radii:?}");
    let want = th.status_dot_size.value() * 0.5;
    for r in radii {
        assert!((r - want).abs() < 0.01, "dot radius {r} != {want}");
    }
}

/// 서명이 깨진 사유는 설명과 homepage 를 숨기고 설명 자리에 숨김 안내를 둔다. 신뢰하지 않은 키는
/// 둘 다 보인다. 웹 주소가 아닌 homepage 는 Attention 메타 줄에 싣지 않는다.
#[test]
fn a_broken_signature_hides_the_description_and_homepage() {
    crate::i18n::init("en");
    let has = |texts: &[(String, egui::Rect)], label: &str| texts.iter().any(|(t, _)| t == label);
    let hidden = t("plugins.attn_desc_hidden");

    let broken = draw(entry(AttentionKind::SignatureInvalid));
    assert!(has(&broken, hidden), "{broken:?}");
    assert!(!has(&broken, "Tails log files."), "{broken:?}");
    assert!(!has(&broken, "example.com/log-tailer"), "{broken:?}");

    let unknown = draw(entry(AttentionKind::UnknownKey));
    assert!(!has(&unknown, hidden), "{unknown:?}");
    assert!(has(&unknown, "Tails log files."), "{unknown:?}");
    assert!(has(&unknown, "example.com/log-tailer"), "{unknown:?}");

    let mut plain = entry(AttentionKind::UnknownKey);
    plain.homepage = "file:///tmp/log-tailer".into();
    let plain = draw(plain);
    assert!(
        !plain.iter().any(|(t, _)| t.contains("/tmp/log-tailer")),
        "{plain:?}"
    );
}

/// 예약 id 사유는 danger 이고 매니페스트 글을 숨긴다.
#[test]
fn the_reserved_reason_is_danger_and_hides_manifest_text() {
    assert!(AttentionKind::ReservedId.is_danger());
    assert!(!AttentionKind::ReservedId.shows_manifest_text());
}

fn window_step(
    ctx: &egui::Context,
    snapshot: &PluginsSnapshot,
    ui_state: &mut PluginsUiState,
    actions: &mut Vec<PluginsAction>,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(720.0, 480.0),
        )),
        events,
        ..Default::default()
    };
    ctx.run(raw, |ctx| {
        crate::plugins_ui::draw_plugins_panel(ctx, snapshot, ui_state, actions);
    })
}

fn two_folders_snapshot() -> PluginsSnapshot {
    PluginsSnapshot {
        attention: vec![
            reserved("one", "com.tasty.same"),
            reserved("two", "com.tasty.same"),
        ],
        ..Default::default()
    }
}

fn attention_state() -> PluginsUiState {
    PluginsUiState {
        active_tab: super::super::PluginsTab::Attention,
        ..Default::default()
    }
}

/// 상세에는 폴더 이름 · id · 숨김 안내 · 배너 · Folder 절 · Remove 만 있다.
#[test]
fn the_reserved_detail_offers_only_remove() {
    crate::i18n::init("en");
    let texts = draw(reserved("com.tasty.agent-stream-plus", "com.tasty.fake"));
    let has = |label: &str| texts.iter().any(|(t, _)| t == label);
    for label in [
        "com.tasty.fake",
        t("plugins.attn_reserved_desc_hidden"),
        t("plugins.attn_not_registered"),
        t("plugins.attn_remove"),
        "/plugins/com.tasty.agent-stream-plus",
    ] {
        assert!(has(label), "{label:?}: {texts:?}");
    }
    for label in [
        t("plugins.attn_register_anyway"),
        t("plugins.attn_reapprove"),
        t("plugins.configure"),
        t("plugins.open_folder"),
        t("plugins.attn_desc_hidden"),
    ] {
        assert!(!has(label), "{label:?}: {texts:?}");
    }
}

/// 같은 id 의 두 폴더를 각각 고를 수 있다.
#[test]
fn two_folders_with_one_id_are_selected_separately() {
    crate::i18n::init("en");
    let snapshot = two_folders_snapshot();
    let mut ui_state = attention_state();
    ui_state.attention_selected = Some(snapshot.attention[1].key());
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    for _ in 0..2 {
        drop(window_step(
            &ctx,
            &snapshot,
            &mut ui_state,
            &mut actions,
            Vec::new(),
        ));
    }
    assert_eq!(
        ui_state.attention_selected,
        Some(snapshot.attention[1].key())
    );
    assert_ne!(snapshot.attention[0].key(), snapshot.attention[1].key());
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

/// 왼쪽 목록의 `index` 행을 누르는 이벤트. 행 높이와 시작 위치는 공용 상수에서 가져온다.
fn click_row_events(ctx: &egui::Context, index: usize) -> Vec<egui::Event> {
    let th = theme::theme();
    let top = ctx.available_rect().top() + th.spacing_sm.value();
    let row = PLUGIN_LIST_ROW_HEIGHT.value() + STRUCT_GAP_2.value();
    let at = egui::pos2(
        th.spacing_lg.value() * 2.0,
        top + row * index as f32 + th.spacing_lg.value(),
    );
    vec![
        egui::Event::PointerMoved(at),
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

/// 지우는 중에는 Esc 를 소비하지 않고 다른 행을 눌러도 선택과 진행 상태가 그대로다.
#[test]
fn esc_and_selection_are_ignored_while_removing() {
    crate::i18n::init("en");
    let snapshot = two_folders_snapshot();
    let first = snapshot.attention[0].folder.clone();
    let mut ui_state = attention_state();
    ui_state.attention_selected = Some(snapshot.attention[0].key());
    ui_state.confirm_remove_folder = first.clone();
    ui_state.removing_folder = first.clone();
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        Vec::new(),
    ));

    let mut left = false;
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(720.0, 480.0),
        )),
        events: vec![escape()],
        ..Default::default()
    };
    drop(ctx.run(raw, |ctx| {
        crate::plugins_ui::draw_plugins_panel(ctx, &snapshot, &mut ui_state, &mut actions);
        left = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    }));
    assert!(left, "Esc must stay unconsumed while removing");
    assert_eq!(ui_state.confirm_remove_folder, first);

    let events = click_row_events(&ctx, 1);
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        events,
    ));
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        Vec::new(),
    ));
    assert_eq!(
        ui_state.attention_selected,
        Some(snapshot.attention[0].key())
    );
    assert_eq!(ui_state.removing_folder, first);
    assert!(actions.is_empty(), "{actions:?}");
}

/// 지우기 전에는 Esc 와 다른 행 선택이 확인을 닫는다.
#[test]
fn esc_and_a_new_selection_cancel_the_confirm() {
    crate::i18n::init("en");
    let snapshot = two_folders_snapshot();
    let first = snapshot.attention[0].folder.clone();
    let ctx = egui::Context::default();
    let mut actions = Vec::new();

    let mut ui_state = attention_state();
    ui_state.attention_selected = Some(snapshot.attention[0].key());
    ui_state.confirm_remove_folder = first.clone();
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        Vec::new(),
    ));
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        vec![escape()],
    ));
    assert_eq!(ui_state.confirm_remove_folder, None);

    ui_state.confirm_remove_folder = first;
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        Vec::new(),
    ));
    let events = click_row_events(&ctx, 1);
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        events,
    ));
    drop(window_step(
        &ctx,
        &snapshot,
        &mut ui_state,
        &mut actions,
        Vec::new(),
    ));
    assert_eq!(ui_state.confirm_remove_folder, None);
    assert_eq!(
        ui_state.attention_selected,
        Some(snapshot.attention[1].key())
    );
}

/// 제거로 행이 빠지면 선택이 첫 항목으로 옮겨 가고 제거 상태도 지워진다.
#[test]
fn a_removed_row_clears_the_removing_state() {
    crate::i18n::init("en");
    let before = two_folders_snapshot();
    let first = before.attention[0].folder.clone();
    let mut ui_state = attention_state();
    ui_state.attention_selected = Some(before.attention[0].key());
    ui_state.confirm_remove_folder = first.clone();
    ui_state.removing_folder = first;
    let after = PluginsSnapshot {
        attention: vec![before.attention[1].clone()],
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut actions = Vec::new();
    drop(window_step(
        &ctx,
        &after,
        &mut ui_state,
        &mut actions,
        Vec::new(),
    ));
    assert_eq!(ui_state.removing_folder, None);
    assert_eq!(ui_state.confirm_remove_folder, None);
    assert_eq!(ui_state.attention_selected, Some(after.attention[0].key()));
}

/// 실패로 끝나면 바가 평소로 돌아간다.
#[test]
fn finishing_a_removal_returns_the_bar_to_rest() {
    let mut ui_state = attention_state();
    let dir = Some(std::path::PathBuf::from("/plugins/one"));
    ui_state.confirm_remove_folder = dir.clone();
    ui_state.removing_folder = dir;
    ui_state.cancel_remove_confirm();
    assert!(ui_state.confirm_remove_folder.is_some(), "no cancel path");
    ui_state.finish_remove_reserved();
    assert_eq!(ui_state.confirm_remove_folder, None);
    assert_eq!(ui_state.removing_folder, None);
}
