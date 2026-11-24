use super::*;
use crate::adapters::ui::popup::body_keys::with_body_keys;
use crate::adapters::ui::popup::window_keys::WindowFocusRecord;
use crate::adapters::ui::preset::demo_layout::DemoLayout;
use serde_json::json;

fn cfg(kind: &str, leaf: usize) -> SurfaceCfg {
    let preset: tasty_presets::TabPreset = serde_json::from_value(json!({"name":"test","tab":{"layout":{"type":"leaf","surface":{"id":leaf,"kind":kind,"cwd":"/original"}}}})).unwrap();
    let model = DemoLayout::from_tab(&preset, &KindCatalog::default());
    SurfaceCfg::open("tab:test".into(), leaf, model.leaf_draft(leaf).unwrap())
}
fn request(cfg: &mut SurfaceCfg) -> FieldBrowseRequest {
    let fields = KindCatalog::default().fields(cfg.draft.kind());
    let field = fields
        .iter()
        .find(|f| matches!(f.input, PresetFieldInput::Dir | PresetFieldInput::FilePath))
        .unwrap();
    cfg.request_browse(field);
    cfg.take_browse_request().unwrap()
}
#[test]
fn preset_field_receipt_updates_only_its_target_once() {
    let catalog = KindCatalog::default();
    let mut cfg = cfg("terminal", 7);
    let request = request(&mut cfg);
    assert!(cfg.apply_browse(&request, "/chosen".into(), &catalog));
    assert_eq!(cfg.draft.value(&PresetFieldTarget::Cwd), "/chosen");
    assert_eq!(cfg.draft.value(&PresetFieldTarget::Startup), "");
    assert!(!cfg.apply_browse(&request, "/replay".into(), &catalog));
}
#[test]
fn preset_field_result_arriving_after_kind_change_is_dropped() {
    let catalog = KindCatalog::default();
    let mut cfg = cfg("terminal", 7);
    let request = request(&mut cfg);
    cfg.switch_kind("image", &catalog);
    cfg.switch_kind("terminal", &catalog);
    let before = cfg.draft.clone();
    assert!(!cfg.apply_browse(&request, "/late".into(), &catalog));
    assert_eq!(cfg.draft, before);
}
#[test]
fn rename_reopen_and_other_leaf_cannot_receive_old_field_results() {
    let catalog = KindCatalog::default();
    let mut original = cfg("terminal", 7);
    let request = request(&mut original);
    original.remap_preset(tasty_presets::PresetKind::Tab, "test", "renamed");
    assert!(!original.apply_browse(&request, "/late".into(), &catalog));
    for mut next in [cfg("terminal", 7), cfg("terminal", 8)] {
        let before = next.draft.clone();
        assert!(!next.apply_browse(&request, "/late".into(), &catalog));
        assert_eq!(next.draft, before);
    }
}
#[test]
fn a_new_browse_supersedes_the_old_request_and_file_fields_use_open_mode() {
    let catalog = KindCatalog::default();
    let mut cfg = cfg("image", 7);
    let old = request(&mut cfg);
    let new = request(&mut cfg);
    assert_eq!(new.mode, crate::view::file_chooser::FileChooserMode::Open);
    assert!(!cfg.apply_browse(&old, "old.png".into(), &catalog));
    assert!(cfg.apply_browse(&new, "chosen.png".into(), &catalog));
    assert_eq!(
        cfg.draft.value(&PresetFieldTarget::Params("file".into())),
        "chosen.png"
    );
}

#[test]
fn escape_in_the_chooser_does_not_cancel_the_surface_settings() {
    let ctx = egui::Context::default();
    let mut cfg = cfg("terminal", 7);
    let catalog = KindCatalog::default();
    let mut chooser = crate::view::file_chooser::WindowFileChooser::default();
    assert!(chooser.open(
        "preset.field",
        crate::view::file_chooser::FileChooserMode::Folder,
        ""
    ));
    for pressed in [false, true] {
        let mut outcome = CfgOutcome::None;
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 720.0),
            )),
            ..Default::default()
        };
        if pressed {
            raw.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        // 창 `render`처럼 직전 패스 뒤 포커스 기록으로 키 주인을 정하고, 주인이 있으면 설정 화면을 가린다.
        let owner = chooser.key_owner(&ctx, WindowFocusRecord::read(&ctx));
        // 창 `render`처럼 W1 값을 넣고 bare Esc 는 사건 입구처럼 키 주인에 넘긴다.
        crate::view::window_input::WindowPopups::window_popups(&mut chooser).raw_pass_keys(
            &ctx,
            owner,
            &mut raw.events,
        );
        drop(ctx.run(raw, |ctx| {
            with_body_keys(ctx, owner.is_none(), || {
                egui::CentralPanel::default().show(ctx, |ui| {
                    outcome = draw_surface_settings(
                        ui,
                        &crate::theme::theme(),
                        ui.available_rect_before_wrap(),
                        &mut cfg,
                        &catalog,
                        &[],
                    );
                });
            });
            chooser.draw(ctx);
        }));
        assert!(matches!(outcome, CfgOutcome::None));
    }
    assert!(!chooser.is_open());
    assert_eq!(cfg.draft.value(&PresetFieldTarget::Cwd), "/original");
}

// ── preset 창 키 주인(W1). 시험은 preset `render`의 그리기·열기 순서(설정 화면 → 찾아보기 요청을 꺼내 열기 →
// 파일 선택 창)를 따르고, 직전 패스 뒤 포커스 기록으로 정한 값을 패스마다 넣는다. ──

use crate::adapters::ui::popup::window_keys::focusable_at_text;
use crate::view::file_chooser::WindowFileChooser;

struct PresetPass {
    outcome: CfgOutcome,
    out: egui::FullOutput,
    opened: bool,
}

fn next_owner(
    ctx: &egui::Context,
    chooser: &WindowFileChooser,
) -> Option<crate::adapters::ui::popup::PopupInstanceId> {
    chooser.key_owner(ctx, WindowFocusRecord::read(ctx))
}

fn preset_pass(
    ctx: &egui::Context,
    cfg: &mut SurfaceCfg,
    chooser: &mut WindowFileChooser,
    events: Vec<egui::Event>,
) -> PresetPass {
    let catalog = KindCatalog::default();
    let owner = next_owner(ctx, chooser);
    let mut outcome = CfgOutcome::None;
    let mut opened = false;
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 720.0),
        )),
        events,
        ..Default::default()
    };
    // 창 `render`처럼 W1 값을 넣고 bare Esc 는 사건 입구처럼 키 주인에 넘긴다.
    crate::view::window_input::WindowPopups::window_popups(&mut *chooser).raw_pass_keys(
        ctx,
        owner,
        &mut raw.events,
    );
    let out = ctx.run(raw, |ctx| {
        with_body_keys(ctx, owner.is_none(), || {
            egui::CentralPanel::default().show(ctx, |ui| {
                outcome = draw_surface_settings(
                    ui,
                    &crate::theme::theme(),
                    ui.available_rect_before_wrap(),
                    cfg,
                    &catalog,
                    &[],
                );
            });
        });
        if let Some(request) = cfg.take_browse_request()
            && chooser.open("preset.field", request.mode.clone(), &request.start)
        {
            opened = true;
        }
        chooser.draw(ctx);
    });
    PresetPass {
        outcome,
        out,
        opened,
    }
}

fn key_press(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 처음 칠한 `text`의 가운데.
fn text_center(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
    out.shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{text:?} was not painted"))
}

/// startup 값을 바꿔 draft 를 더럽힌 terminal 설정.
fn dirty_terminal() -> SurfaceCfg {
    let mut cfg = cfg("terminal", 7);
    cfg.draft
        .set_value(&PresetFieldTarget::Startup, "echo hi".into());
    assert!(cfg.draft.is_dirty(&cfg.orig, &KindCatalog::default()));
    cfg
}

fn open_folder(chooser: &mut WindowFileChooser) {
    assert!(chooser.open(
        "preset.field",
        crate::view::file_chooser::FileChooserMode::Folder,
        ""
    ));
}

/// Given 파일 선택 창이 열리고 포커스가 수정한 입력칸, When Enter, Then surface 설정이 받아 확인한다.
#[test]
fn surface_settings_take_enter_when_focus_is_outside_the_chooser() {
    crate::i18n::init("en");
    let ctx = egui::Context::default();
    let mut cfg = dirty_terminal();
    let mut chooser = WindowFileChooser::default();
    open_folder(&mut chooser);
    let first = preset_pass(&ctx, &mut cfg, &mut chooser, vec![]);
    let input = focusable_at_text(&ctx, &first.out, "echo hi", egui::Order::Background);
    ctx.memory_mut(|m| m.request_focus(input));
    drop(preset_pass(&ctx, &mut cfg, &mut chooser, vec![]));
    assert_eq!(
        next_owner(&ctx, &chooser),
        None,
        "the pass starts at the input"
    );
    let enter = preset_pass(
        &ctx,
        &mut cfg,
        &mut chooser,
        vec![key_press(egui::Key::Enter)],
    );
    assert!(matches!(enter.outcome, CfgOutcome::Confirm));
}

/// Given 파일 선택 창이 키 주인, When Enter·Esc, Then surface 설정은 받지 않는다.
#[test]
fn surface_settings_ignore_keys_while_the_chooser_owns_them() {
    crate::i18n::init("en");
    let ctx = egui::Context::default();
    let mut cfg = dirty_terminal();
    let mut chooser = WindowFileChooser::default();
    open_folder(&mut chooser);
    drop(preset_pass(&ctx, &mut cfg, &mut chooser, vec![]));
    assert!(
        next_owner(&ctx, &chooser).is_some(),
        "the chooser owns the keys"
    );
    let keys = vec![key_press(egui::Key::Enter), key_press(egui::Key::Escape)];
    let pass = preset_pass(&ctx, &mut cfg, &mut chooser, keys);
    assert!(matches!(pass.outcome, CfgOutcome::None));
}

/// 찾아보기 버튼을 누르는 패스까지 그린다. 버튼을 뗄 자리를 돌려준다.
fn press_browse(
    ctx: &egui::Context,
    cfg: &mut SurfaceCfg,
    chooser: &mut WindowFileChooser,
) -> egui::Pos2 {
    let first = preset_pass(ctx, cfg, chooser, vec![]);
    let at = text_center(&first.out, t("preset.field.browse"));
    let press = vec![egui::Event::PointerMoved(at), button(at, true)];
    assert!(!preset_pass(ctx, cfg, chooser, press).opened);
    at
}

/// Given 깨끗한 draft, When 같은 패스에 찾아보기로 파일 선택 창을 열고 Enter, Then 그 패스 키는 surface 설정 쪽이라
/// 결과가 없고 파일 선택 창은 그 Enter 로 확정되지 않는다.
#[test]
fn the_pass_that_opens_the_chooser_keeps_the_keys_at_surface_settings() {
    crate::i18n::init("en");
    let ctx = egui::Context::default();
    let mut cfg = cfg("terminal", 7);
    let mut chooser = WindowFileChooser::default();
    assert!(!cfg.draft.is_dirty(&cfg.orig, &KindCatalog::default()));
    let at = press_browse(&ctx, &mut cfg, &mut chooser);
    assert_eq!(
        next_owner(&ctx, &chooser),
        None,
        "the pass starts at surface settings"
    );
    let open = preset_pass(
        &ctx,
        &mut cfg,
        &mut chooser,
        vec![button(at, false), key_press(egui::Key::Enter)],
    );
    assert!(open.opened, "the browse request was taken in this pass");
    assert!(chooser.is_open());
    assert!(matches!(open.outcome, CfgOutcome::None));
    assert_eq!(chooser.take_outcome("preset.field"), None);
    assert!(chooser.is_open());
}

/// Given 여는 패스 끝에 밖 포커스가 없음, When 다음 패스에 Enter·Esc, Then 파일 선택 창이 키 주인이고 surface
/// 설정은 받지 않는다.
#[test]
fn the_chooser_takes_the_keys_when_the_opening_pass_left_no_outside_focus() {
    crate::i18n::init("en");
    let ctx = egui::Context::default();
    let mut cfg = cfg("terminal", 7);
    let mut chooser = WindowFileChooser::default();
    let at = press_browse(&ctx, &mut cfg, &mut chooser);
    let open = preset_pass(&ctx, &mut cfg, &mut chooser, vec![button(at, false)]);
    assert!(open.opened && chooser.is_open());
    assert_eq!(
        WindowFocusRecord::read(&ctx).layer(),
        None,
        "the opening pass left no focus"
    );
    assert!(
        next_owner(&ctx, &chooser).is_some(),
        "the chooser owns the keys"
    );
    let keys = vec![key_press(egui::Key::Enter), key_press(egui::Key::Escape)];
    let pass = preset_pass(&ctx, &mut cfg, &mut chooser, keys);
    assert!(matches!(pass.outcome, CfgOutcome::None));
}

/// Given 수정한 startup 입력칸 포커스가 남은 채 찾아보기로 연 파일 선택 창, When 다음 패스 Enter, Then 입력칸이 받아
/// 확인한다. 대신 파일 선택 창 안을 누르면 그다음 패스부터 파일 선택 창이 키 주인이다.
#[test]
fn an_outside_focus_left_by_the_opening_pass_keeps_the_keys_outside() {
    crate::i18n::init("en");
    for click_inside in [false, true] {
        let ctx = egui::Context::default();
        let mut cfg = dirty_terminal();
        let mut chooser = WindowFileChooser::default();
        let first = preset_pass(&ctx, &mut cfg, &mut chooser, vec![]);
        let input = focusable_at_text(&ctx, &first.out, "echo hi", egui::Order::Background);
        let at = press_browse(&ctx, &mut cfg, &mut chooser);
        ctx.memory_mut(|m| m.request_focus(input));
        let open = preset_pass(&ctx, &mut cfg, &mut chooser, vec![button(at, false)]);
        assert!(open.opened && chooser.is_open());
        assert_eq!(ctx.memory(|m| m.focused()), Some(input));
        assert_eq!(
            WindowFocusRecord::read(&ctx).layer().map(|l| l.order),
            Some(egui::Order::Background)
        );
        assert_eq!(next_owner(&ctx, &chooser), None);
        if click_inside {
            let inside = egui::pos2(500.0, 360.0);
            let click = vec![
                egui::Event::PointerMoved(inside),
                button(inside, true),
                button(inside, false),
            ];
            drop(preset_pass(&ctx, &mut cfg, &mut chooser, click));
            assert!(next_owner(&ctx, &chooser).is_some());
        } else {
            let enter = preset_pass(
                &ctx,
                &mut cfg,
                &mut chooser,
                vec![key_press(egui::Key::Enter)],
            );
            assert!(matches!(enter.outcome, CfgOutcome::Confirm));
        }
    }
}
