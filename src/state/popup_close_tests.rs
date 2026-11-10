//! 팝업을 내용의 Close 반환, 바깥 클릭·닫기 버튼, Intent로 닫은 뒤 상태를 검사한다.
//! egui 프레임을 실행하며 OS 창은 만들지 않는다.
//! 고정 위치에 열고 크기 계산이 필요한 팝업은 입력 없는 프레임 뒤에 버튼 좌표를 구한다.

fn test_state() -> (
    crate::state::RequestContext,
    crate::runtime::engine_session::EngineSession,
) {
    use tasty_core::{DomainEvent as E, SurfaceSpec};
    crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::CategoryCreated {
            id: 1,
            name: "Services".into(),
            index: 1,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "workspace".into(),
            category: 0,
            index: 0,
            pane: 1,
        },
        E::TabCreated {
            id: 1,
            pane: 1,
            index: 0,
            name: "terminal".into(),
            surface: SurfaceSpec {
                id: 1,
                kind: "terminal".into(),
                data: None,
            },
        },
    ]))
}
use crate::adapters::ui::draw_popups;
use crate::adapters::ui::info_modal::{INFO_MODAL_ID, InfoModal};
use crate::adapters::ui::popup::approval::APPROVAL_POPUP_ID;
use crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID;
use crate::adapters::ui::popup::confirm_delete_category::CONFIRM_DELETE_CATEGORY_POPUP_ID;
use crate::adapters::ui::popup::confirm_force_detach_workspace::CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID;
use crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID;
use crate::adapters::ui::popup::file_picker::FILE_PICKER_POPUP_ID;
use crate::adapters::ui::popup::port_scanner::{
    PORT_SCANNER_POPUP_ID, PortRowView, PortScanState, SourceTag,
};
use crate::adapters::ui::popup::preset_apply::APPLY_WORKSPACE_POPUP_ID;
use crate::adapters::ui::popup::rail_category::RAIL_CATEGORY_POPUP_ID;
use crate::adapters::ui::popup::transfer::{
    TRANSFER_ERROR_POPUP_ID, TRANSFER_PROGRESS_POPUP_ID, TransferError, TransferProgress,
};
use crate::adapters::ui::popup::{PopupId, title_bar_height};
use crate::intent::{Intent, UiIntent};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::state::{
    FileHandlerPickerData, FileHandlerPickerResult, FilePickerData, FilePickerResult, FpLoadState,
    PendingScriptConfirm, RenameTarget,
};
use tasty_approval::{ApprovalId, ApprovalRequest, Requester, Severity};

const CONVERT_SURFACE_POPUP_ID: PopupId = "convert_surface";
const RENAME_POPUP_ID: PopupId = "rename";
const SCRIPT_CHANGED_CONFIRM_POPUP_ID: PopupId = "script_changed_confirm";

pub(super) fn term_rect() -> PhysicalRect {
    PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(1920.0),
        height: PhysicalPx(1080.0),
    }
}

fn screen_rect() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0))
}

fn empty_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(screen_rect()),
        ..Default::default()
    }
}

fn key_input(key: egui::Key) -> egui::RawInput {
    let mut raw = empty_input();
    raw.events.push(egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    raw
}

fn press_input(pos: egui::Pos2) -> egui::RawInput {
    let mut raw = empty_input();
    raw.events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    raw
}

fn outside_point(popup_pos: egui::Pos2) -> egui::Pos2 {
    egui::pos2(
        (popup_pos.x - 200.0).max(0.0),
        (popup_pos.y - 200.0).max(0.0),
    )
}

/// 공개 위치·크기로 닫기 버튼의 중앙을 구한다.
fn close_button_point(popup_pos: egui::Pos2, popup_size: egui::Vec2) -> egui::Pos2 {
    egui::pos2(
        popup_pos.x + popup_size.x - 14.0,
        popup_pos.y + title_bar_height().value() / 2.0,
    )
}

fn run_frame(
    raw: egui::RawInput,
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) {
    let ctx = egui::Context::default();
    drop(ctx.run(raw, |ctx| {
        draw_popups(ctx, state, &engine.read(), &[], term_rect(), 1.0);
    }));
}

fn primed_popup_geometry(
    id: PopupId,
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> (egui::Pos2, egui::Vec2) {
    run_frame(empty_input(), state, engine);
    let p = state.popups.get_mut(id).expect("popup registered");
    (p.pos, p.size)
}

const FIXED_POS: egui::Pos2 = egui::pos2(500.0, 500.0);

#[test]
fn convert_surface_escape_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_id = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    state.dialogs.convert_popup = Some(surface_id);
    state.dialogs.convert_popup_selected = Some(0);
    state
        .popups
        .open_at_focused(CONVERT_SURFACE_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(CONVERT_SURFACE_POPUP_ID));
    assert!(state.dialogs.convert_popup.is_none());
    assert!(state.dialogs.convert_popup_selected.is_none());
}

#[test]
fn convert_surface_outside_click_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_id = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    state.dialogs.convert_popup = Some(surface_id);
    state.dialogs.convert_popup_selected = Some(0);
    state
        .popups
        .open_at_focused(CONVERT_SURFACE_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(CONVERT_SURFACE_POPUP_ID));
    assert!(state.dialogs.convert_popup.is_none());
    assert!(state.dialogs.convert_popup_selected.is_none());
}

// Intent는 닫기를 큐에 넣으므로 다음 렌더 프레임까지 실행해 후속 정리를 검사한다.
#[test]
fn convert_surface_close_intent_now_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_id = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    state.dialogs.convert_popup = Some(surface_id);
    state.dialogs.convert_popup_selected = Some(0);
    state
        .popups
        .open_at_focused(CONVERT_SURFACE_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: CONVERT_SURFACE_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(CONVERT_SURFACE_POPUP_ID));
    assert!(state.dialogs.convert_popup.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.convert_popup.is_none());
    assert!(state.dialogs.convert_popup_selected.is_none());
}

#[test]
fn rename_escape_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.rename = Some((RenameTarget::NewCategory, "abc".to_string()));
    state.popups.open_at_focused(RENAME_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(state.dialogs.rename.is_none());
}

#[test]
fn rename_x_button_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.rename = Some((RenameTarget::NewCategory, "abc".to_string()));
    state.popups.open_at_focused(RENAME_POPUP_ID, FIXED_POS);
    let (pos, size) = primed_popup_geometry(RENAME_POPUP_ID, &mut state, &mut engine);

    run_frame(
        press_input(close_button_point(pos, size)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(state.dialogs.rename.is_none());
}

#[test]
fn rail_category_escape_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.rail_category_popup = Some(cat_id);
    state
        .popups
        .open_at_focused(RAIL_CATEGORY_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
    assert!(state.dialogs.rail_category_popup.is_none());
}

#[test]
fn rail_category_close_intent_now_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.rail_category_popup = Some(cat_id);
    state
        .popups
        .open_at_focused(RAIL_CATEGORY_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: RAIL_CATEGORY_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
    assert!(state.dialogs.rail_category_popup.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.rail_category_popup.is_none());
}

#[test]
fn rail_category_outside_click_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.rail_category_popup = Some(cat_id);
    state
        .popups
        .open_at_focused(RAIL_CATEGORY_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
    assert!(state.dialogs.rail_category_popup.is_none());
}

// 타이틀바와 바깥 클릭 닫기가 없는 진행 팝업은 빈 rows로 내용의 Close를 유도한다.

#[test]
fn transfer_progress_empty_rows_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_progress = Some(TransferProgress { rows: vec![] });
    state
        .popups
        .open_at_focused(TRANSFER_PROGRESS_POPUP_ID, FIXED_POS);

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!state.popups.is_open(TRANSFER_PROGRESS_POPUP_ID));
    assert!(state.dialogs.transfer_progress.is_none());
}

#[test]
fn transfer_progress_close_intent_now_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_progress = Some(TransferProgress { rows: vec![] });
    state
        .popups
        .open_at_focused(TRANSFER_PROGRESS_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: TRANSFER_PROGRESS_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(TRANSFER_PROGRESS_POPUP_ID));
    assert!(state.dialogs.transfer_progress.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.transfer_progress.is_none());
}

fn xfer_err(name: &str) -> TransferError {
    TransferError {
        name: name.to_string(),
        reason: "boom".to_string(),
        retry: None,
    }
}

#[test]
fn transfer_error_escape_close_pops_single_entry() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_error.push_back(xfer_err("a.txt"));
    state
        .popups
        .open_at_focused(TRANSFER_ERROR_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
    assert!(state.dialogs.transfer_error.is_empty());
}

#[test]
fn transfer_error_outside_click_with_single_entry_closes_without_reopen() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_error.push_back(xfer_err("a.txt"));
    state
        .popups
        .open_at_focused(TRANSFER_ERROR_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
    assert!(state.dialogs.transfer_error.is_empty());
}

#[test]
fn transfer_error_outside_click_with_two_entries_pops_head_and_reopens() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_error.push_back(xfer_err("a.txt"));
    state.dialogs.transfer_error.push_back(xfer_err("b.txt"));
    state
        .popups
        .open_at_focused(TRANSFER_ERROR_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert_eq!(state.dialogs.transfer_error.len(), 1);
    assert_eq!(state.dialogs.transfer_error.front().unwrap().name, "b.txt");
    assert!(state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
}

#[test]
fn transfer_error_close_intent_now_pops_head_and_reopens() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_error.push_back(xfer_err("a.txt"));
    state.dialogs.transfer_error.push_back(xfer_err("b.txt"));
    state
        .popups
        .open_at_focused(TRANSFER_ERROR_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: TRANSFER_ERROR_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
    assert_eq!(state.dialogs.transfer_error.len(), 2);

    run_frame(empty_input(), &mut state, &mut engine);

    assert_eq!(state.dialogs.transfer_error.len(), 1);
    assert_eq!(state.dialogs.transfer_error.front().unwrap().name, "b.txt");
    assert!(state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
}

fn pending_script_confirm(result: Option<bool>) -> PendingScriptConfirm {
    PendingScriptConfirm {
        script_id: "on_startup".to_string(),
        name: "deploy.lua".to_string(),
        source: "-- lua source".to_string(),
        new_hash: "deadbeef".to_string(),
        result,
        content_height: None,
    }
}

#[test]
fn script_changed_confirm_close_intent_now_clears_undecided_pending() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.pending_script_confirm = Some(pending_script_confirm(None));
    state
        .popups
        .open_at_focused(SCRIPT_CHANGED_CONFIRM_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: SCRIPT_CHANGED_CONFIRM_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(SCRIPT_CHANGED_CONFIRM_POPUP_ID));
    assert!(state.dialogs.pending_script_confirm.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.pending_script_confirm.is_none());
}

// Run 결과는 다음 App 처리에서 실행하므로 닫기 훅이 지우면 안 된다.
#[test]
fn script_changed_confirm_close_intent_preserves_decided_result_for_dispatch() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.pending_script_confirm = Some(pending_script_confirm(Some(true)));
    state
        .popups
        .open_at_focused(SCRIPT_CHANGED_CONFIRM_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: SCRIPT_CHANGED_CONFIRM_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    run_frame(empty_input(), &mut state, &mut engine);

    let pending = state
        .dialogs
        .pending_script_confirm
        .as_ref()
        .expect("Run 결과는 실행 요청 처리까지 남아 있어야 한다");
    assert_eq!(pending.result, Some(true));
}

#[test]
fn command_palette_outside_click_close_resets_query_and_selection() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.command_palette.query = "workspace".to_string();
    state.command_palette.selected = 2;
    state
        .popups
        .open_at_focused(COMMAND_PALETTE_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(COMMAND_PALETTE_POPUP_ID));
    assert!(state.command_palette.query.is_empty());
    assert_eq!(state.command_palette.selected, 0);
}

// 바깥 클릭은 결과를 유지하며, 팝업의 Close 버튼 경로만 Idle로 초기화한다.
#[test]
fn port_scanner_outside_click_close_preserves_scan_results() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.port_scan = PortScanState::Ready {
        rows: vec![PortRowView {
            port: 8080,
            addr_display: "127.0.0.1".to_string(),
            pid: Some(1234),
            process_name: Some("node".to_string()),
            source: SourceTag::External,
            state: tasty_portscan::PortState::Listen,
            favorited: false,
        }],
        scope: crate::adapters::ui::popup::port_scanner::ScanScope::Tasty,
    };
    state
        .popups
        .open_at_focused(PORT_SCANNER_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(PORT_SCANNER_POPUP_ID));
    match &state.port_scan {
        PortScanState::Ready { rows, .. } => assert_eq!(rows.len(), 1),
        _ => panic!("expected Ready to survive close, got a different state variant instead"),
    }
}

fn info_modal_entry(body: &str) -> InfoModal {
    InfoModal {
        title: "Boot".to_string(),
        body: body.to_string(),
        extra_buttons: Vec::new(),
        emphasis: false,
    }
}

#[test]
fn info_modal_close_intent_now_pops_head_and_reopens() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state
        .dialogs
        .info_modal_queue
        .push_back(info_modal_entry("a"));
    state
        .dialogs
        .info_modal_queue
        .push_back(info_modal_entry("b"));
    state.popups.open_at_focused(INFO_MODAL_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup { id: INFO_MODAL_ID }.from_user_menu("test"),
    );
    assert!(!state.popups.is_open(INFO_MODAL_ID));
    assert_eq!(state.dialogs.info_modal_queue.len(), 2);

    run_frame(empty_input(), &mut state, &mut engine);

    assert_eq!(state.dialogs.info_modal_queue.len(), 1);
    assert_eq!(state.dialogs.info_modal_queue.front().unwrap().body, "b");
    assert!(state.popups.is_open(INFO_MODAL_ID));
}

#[test]
fn info_modal_close_intent_with_single_entry_pops_and_does_not_reopen() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state
        .dialogs
        .info_modal_queue
        .push_back(info_modal_entry("only"));
    state.popups.open_at_focused(INFO_MODAL_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup { id: INFO_MODAL_ID }.from_user_menu("test"),
    );
    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.info_modal_queue.is_empty());
    assert!(!state.popups.is_open(INFO_MODAL_ID));
}

#[test]
fn confirm_delete_category_escape_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = 1;
    state.dialogs.pending_category_delete = Some(cat_id);
    state
        .popups
        .open_at_focused(CONFIRM_DELETE_CATEGORY_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
    assert!(state.dialogs.pending_category_delete.is_none());
}

#[test]
fn confirm_delete_category_outside_click_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = 1;
    state.dialogs.pending_category_delete = Some(cat_id);
    state
        .popups
        .open_at_focused(CONFIRM_DELETE_CATEGORY_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
    assert!(state.dialogs.pending_category_delete.is_none());
}

#[test]
fn confirm_delete_category_close_intent_now_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = 1;
    state.dialogs.pending_category_delete = Some(cat_id);
    state
        .popups
        .open_at_focused(CONFIRM_DELETE_CATEGORY_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: CONFIRM_DELETE_CATEGORY_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
    assert!(state.dialogs.pending_category_delete.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.pending_category_delete.is_none());
}

/// client 7이 워크스페이스를 hard 점유하고 확인 팝업이 열린 상태를 만든다.
fn occupied_workspace_with_popup(
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> (crate::model::WorkspaceId, Vec<u32>) {
    let ws_id = engine.workspace_at(0).expect("workspace index is valid").id;
    let members = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids();
    engine
        .live
        .occupancy
        .acquire_workspace(ws_id, &members, &members, 7)
        .expect("workspace is free in the fixture");
    state.dialogs.pending_force_detach_workspace = Some(ws_id);
    state
        .popups
        .open_at_focused(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID, FIXED_POS);
    (ws_id, members)
}

#[test]
fn confirm_force_detach_escape_clears_state_and_keeps_the_occupancy() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let (ws_id, members) = occupied_workspace_with_popup(&mut state, &mut engine);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(
        !state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );
    assert!(state.dialogs.pending_force_detach_workspace.is_none());
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
    for sid in &members {
        assert!(
            engine.live.occupancy.is_hard_occupied(*sid),
            "surface {sid} 가 풀렸다"
        );
    }
}

#[test]
fn confirm_force_detach_outside_click_clears_state_and_keeps_the_occupancy() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let (ws_id, _) = occupied_workspace_with_popup(&mut state, &mut engine);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(
        !state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );
    assert!(state.dialogs.pending_force_detach_workspace.is_none());
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
}

#[test]
fn confirm_force_detach_close_intent_clears_state_after_next_frame() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let (ws_id, _) = occupied_workspace_with_popup(&mut state, &mut engine);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(
        !state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );
    assert!(state.dialogs.pending_force_detach_workspace.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.pending_force_detach_workspace.is_none());
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
}

#[test]
fn confirm_force_detach_closes_when_the_occupancy_is_already_gone() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let (ws_id, _) = occupied_workspace_with_popup(&mut state, &mut engine);

    assert_eq!(engine.force_detach_workspace(ws_id), Some(7));
    assert!(
        state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(
        !state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );
    assert!(state.dialogs.pending_force_detach_workspace.is_none());
}

#[test]
fn confirm_force_detach_closes_when_the_workspace_is_gone() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.pending_force_detach_workspace = Some(u32::MAX);
    state
        .popups
        .open_at_focused(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID, FIXED_POS);

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(
        !state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );
    assert!(state.dialogs.pending_force_detach_workspace.is_none());
}

// 이 하네스는 프레임마다 Context를 새로 만들어 버튼 클릭 대신 실행 함수를 직접 검사한다.
#[test]
fn confirm_force_detach_confirm_queues_the_original_grant_without_releasing_it() {
    let (mut state, mut owner) = test_state();
    let mut engine = owner.borrow_mut();
    let (ws_id, members) = occupied_workspace_with_popup(&mut state, &mut engine);
    assert!(!members.is_empty());
    let grant = engine
        .live
        .occupancy
        .workspaces_snapshot()
        .into_iter()
        .find(|(id, _)| *id == ws_id)
        .unwrap()
        .1
        .granted_seq;
    crate::adapters::ui::popup::confirm_force_detach_workspace::apply_force_detach(
        &mut state,
        &engine.read(),
    );
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert!(intents[0].origin.is_user());
    assert!(
        matches!(&intents[0].body, Intent::Engine(crate::app::engine_action::EngineAction::DetachWorkspace { workspace, holder: 7, grant: original }) if *workspace == ws_id && *original == grant)
    );
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
    assert!(state.dialogs.pending_force_detach_workspace.is_none());
}

// release에는 강제 끊기 IPC가 없으므로 로컬 GUI 확인이 점유를 푸는 유일한 공개 경로다.
#[test]
fn confirm_force_detach_applied_intent_releases_and_notifies_the_holder() {
    use tasty_ipc::stream::StreamTag;
    let (mut state, mut owner) = test_state();
    let mut engine = owner.borrow_mut();
    let hub = tasty_ipc::stream_hub::StreamHub::new();
    let holder = hub.alloc_id();
    let rx = hub.register(holder);
    engine.remote.set_notifier(hub);
    let ws_id = engine.workspace_at(0).expect("workspace index is valid").id;
    let members = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids();
    engine
        .live
        .occupancy
        .acquire_workspace(ws_id, &members, &members, holder)
        .expect("workspace is free in the fixture");
    state.dialogs.pending_force_detach_workspace = Some(ws_id);

    crate::adapters::ui::popup::confirm_force_detach_workspace::apply_force_detach(
        &mut state,
        &engine.read(),
    );
    let intents = state.take_pending_intents();
    let [intent] = intents.as_slice() else {
        panic!("one detach intent expected: {}", intents.len());
    };
    let Intent::Engine(action) = &intent.body else {
        panic!("engine intent expected");
    };
    action.apply(&mut engine);

    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), None);
    for sid in &members {
        assert!(!engine.live.occupancy.is_hard_occupied(*sid), "{sid}");
    }
    let control = rx.recv().expect("control frame");
    assert_eq!(control.tag, StreamTag::Control);
    assert!(String::from_utf8_lossy(&control.payload).contains("force_detached"));
    assert_eq!(rx.recv().expect("detach frame").tag, StreamTag::Detach);
}

// 점유 테두리의 강제 끊기 버튼이 보내는 surface 해제도 점유를 풀고 holder에 알린다.
#[test]
fn force_detach_button_intent_releases_and_notifies_the_holder() {
    use tasty_ipc::stream::StreamTag;
    let (_state, mut owner) = test_state();
    let mut engine = owner.borrow_mut();
    let hub = tasty_ipc::stream_hub::StreamHub::new();
    let holder = hub.alloc_id();
    let rx = hub.register(holder);
    engine.remote.set_notifier(hub);
    let sid = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    let grant = engine
        .live
        .occupancy
        .acquire(sid, holder)
        .expect("surface is free in the fixture")
        .granted_seq;

    crate::app::engine_action::EngineAction::DetachSurface {
        surface: sid,
        grant,
    }
    .apply(&mut engine);

    assert!(!engine.live.occupancy.is_hard_occupied(sid));
    assert_eq!(rx.recv().expect("control frame").tag, StreamTag::Control);
    assert_eq!(rx.recv().expect("detach frame").tag, StreamTag::Detach);
}

#[test]
fn confirm_force_detach_with_no_pending_target_detaches_nothing() {
    let (mut state, mut owner) = test_state();
    let mut engine = owner.borrow_mut();
    let (ws_id, _) = occupied_workspace_with_popup(&mut state, &mut engine);
    state.dialogs.pending_force_detach_workspace = None;
    crate::adapters::ui::popup::confirm_force_detach_workspace::apply_force_detach(
        &mut state,
        &engine.read(),
    );
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
}

// 파일 핸들러 피커는 타이틀바와 바깥 클릭 닫기가 없어 해당 포인터 경로 시험은 없다.

fn mk_picker_data() -> FileHandlerPickerData {
    FileHandlerPickerData {
        origin_surface_id: None,
        dispatch_origin: crate::file::dispatch::FileDispatchOrigin::Agent,
        target: crate::file::format::FileTarget::new("/tmp/popup-close-test.txt").into(),
        target_display: "/tmp/popup-close-test.txt".to_string(),
        detector: None,
        candidates: vec![],
        candidates_are_fallback: false,
        recent: vec![],
        default_handler: None,
        selected: None,
        result: None,
        ignore_size_limit: false,
        run: None,
        remote_host: None,
    }
}

#[test]
fn file_handler_picker_escape_close_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.file_handler_picker = Some(mk_picker_data());
    state.popups.open_at_focused(PICKER_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(PICKER_POPUP_ID));
    assert!(matches!(
        state.dialogs.file_handler_picker.as_ref().unwrap().result,
        Some(FileHandlerPickerResult::Cancelled)
    ));
}

#[test]
fn file_handler_picker_close_intent_now_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.file_handler_picker = Some(mk_picker_data());
    state.popups.open_at_focused(PICKER_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: PICKER_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(PICKER_POPUP_ID));
    assert!(
        state
            .dialogs
            .file_handler_picker
            .as_ref()
            .unwrap()
            .result
            .is_none()
    );

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(matches!(
        state.dialogs.file_handler_picker.as_ref().unwrap().result,
        Some(FileHandlerPickerResult::Cancelled)
    ));
}

fn mk_file_picker_data() -> FilePickerData {
    FilePickerData {
        save_name: None,
        request_id: crate::state::FilePickerRequestId(1),
        mirror_ws_id: None,
        remote_host: None,
        current_dir: "/tmp".to_string(),
        load: FpLoadState::Loaded,
        local_query: None,
        entries: vec![],
        selected: vec![],
        result: None,
        requester: None,
        filters: vec![],
    }
}

#[test]
fn file_picker_escape_close_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.file_picker = Some(mk_file_picker_data());
    state
        .popups
        .open_at_focused(FILE_PICKER_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert!(matches!(
        state.dialogs.file_picker.as_ref().unwrap().result,
        Some(FilePickerResult::Cancelled)
    ));
}

/// 한 egui context 로 프레임을 이어 그린다. 위젯 버튼 클릭과 드래그는 누름·뗌이 같은
/// context 의 여러 프레임에 걸쳐야 인식된다.
fn run_frames(
    inputs: Vec<egui::RawInput>,
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> egui::Context {
    let ctx = egui::Context::default();
    for raw in inputs {
        drop(ctx.run(raw, |ctx| {
            draw_popups(ctx, state, &engine.read(), &[], term_rect(), 1.0);
        }));
    }
    ctx
}

fn pointer_input(pos: egui::Pos2, pressed: Option<bool>) -> egui::RawInput {
    let mut raw = empty_input();
    raw.events.push(egui::Event::PointerMoved(pos));
    if let Some(pressed) = pressed {
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    raw
}

/// 뷰가 첫 프레임에 보고한 file picker 헤더 줄.
fn file_picker_header_rect(
    state: &mut crate::state::RequestContext,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
) -> egui::Rect {
    let ctx = run_frames(vec![empty_input()], state, engine);
    crate::adapters::ui::popup::reported_header_drag_rect(&ctx, FILE_PICKER_POPUP_ID)
        .expect("file picker 뷰가 헤더 줄을 보고해야 한다")
}

/// 셸 타이틀바가 없어 닫기 ×는 위젯 헤더 오른쪽 끝에 있다. 그 버튼으로 닫아도 헤더 드래그가
/// 가로채지 않고 Cancelled 가 남는다. 실제 클릭처럼 버튼 위에 먼저 올린 뒤 누른다 — 매니저는
/// 직전 프레임에 위젯이 포인터를 가졌는지로 버튼과 드래그 영역을 가른다.
#[test]
fn file_picker_header_close_button_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.file_picker = Some(mk_file_picker_data());
    state
        .popups
        .open_at_focused(FILE_PICKER_POPUP_ID, FIXED_POS);
    let header = file_picker_header_rect(&mut state, &mut engine);
    let close = egui::pos2(header.right() - header.height() / 2.0, header.center().y);

    run_frames(
        vec![
            empty_input(),
            pointer_input(close, None),
            pointer_input(close, Some(true)),
            pointer_input(close, Some(false)),
        ],
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert!(matches!(
        state.dialogs.file_picker.as_ref().unwrap().result,
        Some(FilePickerResult::Cancelled)
    ));
}

/// 헤더의 빈 곳(제목 오른쪽)을 끌면 popup 이 이동하고 닫히지 않는다.
#[test]
fn file_picker_header_drag_moves_the_popup() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.file_picker = Some(mk_file_picker_data());
    state
        .popups
        .open_at_focused(FILE_PICKER_POPUP_ID, FIXED_POS);
    let header = file_picker_header_rect(&mut state, &mut engine);
    let before = state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().pos;
    let grab = header.center();
    let delta = egui::vec2(-60.0, -40.0);

    run_frames(
        vec![
            empty_input(),
            pointer_input(grab, Some(true)),
            pointer_input(grab + delta * 0.5, None),
            pointer_input(grab + delta, None),
            pointer_input(grab + delta, Some(false)),
        ],
        &mut state,
        &mut engine,
    );

    assert!(state.popups.is_open(FILE_PICKER_POPUP_ID));
    let after = state.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().pos;
    assert!(
        (after - before - delta).length() < 1.0,
        "헤더를 {delta:?} 끌었는데 popup 위치가 {before:?} → {after:?}"
    );
}

#[test]
fn file_picker_close_intent_now_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.file_picker = Some(mk_file_picker_data());
    state
        .popups
        .open_at_focused(FILE_PICKER_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: FILE_PICKER_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert!(state.dialogs.file_picker.as_ref().unwrap().result.is_none());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(matches!(
        state.dialogs.file_picker.as_ref().unwrap().result,
        Some(FilePickerResult::Cancelled)
    ));
}

fn push_approval(
    _engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    state: &mut crate::state::RequestContext,
    id: &str,
) {
    let record = tasty_approval::ApprovalRecord {
        request: ApprovalRequest {
            id: ApprovalId(id.into()),
            requester: Requester::Plugin { id: "test".into() },
            workspace_id: None,
            surface_id: None,
            title: "Test approval".into(),
            body: None,
            choices: vec![],
            default_choice: None,
            timeout_ms: None,
            severity: Severity::Info,
            created_at: 1,
            metadata: serde_json::Value::Null,
            host_elevation: None,
        },
        state: tasty_approval::ApprovalState::Pending,
        history: vec![],
    };
    state
        .dialogs
        .pending_approval_ids
        .push_back(record.request.id.clone());
    state
        .dialogs
        .approval_records
        .insert(record.request.id.clone(), record);
}

#[test]
fn approval_empty_queue_close_clears_comment_buffer() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.approval_comment_buffer = "draft comment".to_string();
    state.popups.open_at_focused(APPROVAL_POPUP_ID, FIXED_POS);

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!state.popups.is_open(APPROVAL_POPUP_ID));
    assert!(state.dialogs.approval_comment_buffer.is_empty());
}

#[test]
fn approval_x_button_close_with_empty_queue_does_not_refire() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.approval_comment_buffer = "draft comment".to_string();
    state.popups.open_at_focused(APPROVAL_POPUP_ID, FIXED_POS);
    push_approval(&mut engine, &mut state, "req-1");
    let (pos, size) = primed_popup_geometry(APPROVAL_POPUP_ID, &mut state, &mut engine);
    state.dialogs.pending_approval_ids.clear();

    run_frame(
        press_input(close_button_point(pos, size)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(APPROVAL_POPUP_ID));
    assert!(state.dialogs.approval_comment_buffer.is_empty());
    let pending = state.take_pending_intents();
    assert!(
        !pending
            .iter()
            .any(|d| matches!(&d.body, Intent::Ui(UiIntent::OpenPopup { id, .. }) if *id == APPROVAL_POPUP_ID)),
        "empty queue must not refire OpenPopup"
    );
}

#[test]
fn approval_x_button_close_with_pending_queue_refires_open_popup() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    push_approval(&mut engine, &mut state, "req-1");
    push_approval(&mut engine, &mut state, "req-2");
    state.popups.open_at_focused(APPROVAL_POPUP_ID, FIXED_POS);
    let (pos, size) = primed_popup_geometry(APPROVAL_POPUP_ID, &mut state, &mut engine);

    run_frame(
        press_input(close_button_point(pos, size)),
        &mut state,
        &mut engine,
    );

    // X 버튼은 내용을 거치지 않으므로 응답 대기 큐는 유지한다.
    assert_eq!(state.dialogs.pending_approval_ids.len(), 2);
    let pending = state.take_pending_intents();
    assert!(
        pending
            .iter()
            .any(|d| matches!(&d.body, Intent::Ui(UiIntent::OpenPopup { id, .. }) if *id == APPROVAL_POPUP_ID)),
        "non-empty queue must refire OpenPopup for the next head"
    );
}

#[test]
fn approval_close_intent_now_refires_open_popup() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    push_approval(&mut engine, &mut state, "req-1");
    push_approval(&mut engine, &mut state, "req-2");
    state.popups.open_at_focused(APPROVAL_POPUP_ID, FIXED_POS);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: APPROVAL_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(APPROVAL_POPUP_ID));
    assert!(state.take_pending_intents().is_empty());

    run_frame(empty_input(), &mut state, &mut engine);

    assert_eq!(state.dialogs.pending_approval_ids.len(), 2);
    let pending = state.take_pending_intents();
    assert!(
        pending
            .iter()
            .any(|d| matches!(&d.body, Intent::Ui(UiIntent::OpenPopup { id, .. }) if *id == APPROVAL_POPUP_ID)),
        "non-empty queue must refire OpenPopup for the next head"
    );
}

#[test]
fn close_intent_now_clears_cleanup_after_next_frame() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.rename = Some((RenameTarget::NewCategory, "abc".to_string()));
    state.popups.open_at_focused(RENAME_POPUP_ID, FIXED_POS);

    let dispatched = UiIntent::ClosePopup {
        id: RENAME_POPUP_ID,
    }
    .from_user_menu("test");
    crate::intent::popup::handle(&mut state, &mut engine, &dispatched);

    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(state.dialogs.rename.is_some());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.rename.is_none());
}

// 프리셋 팝업 셋이 같은 닫기 훅을 사용해 workspace 팝업으로 공통 상태 정리를 검사한다.

#[test]
fn preset_apply_x_button_close_clears_selection_and_target_category() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.preset_apply_target_category = Some(cat_id);
    state.dialogs.preset_picker_selected = Some("my-preset".to_string());
    state
        .popups
        .open_at_focused(APPLY_WORKSPACE_POPUP_ID, FIXED_POS);
    let (pos, size) = primed_popup_geometry(APPLY_WORKSPACE_POPUP_ID, &mut state, &mut engine);

    run_frame(
        press_input(close_button_point(pos, size)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(APPLY_WORKSPACE_POPUP_ID));
    assert!(state.dialogs.preset_apply_target_category.is_none());
    assert!(state.dialogs.preset_picker_selected.is_none());
}

#[test]
fn preset_apply_outside_click_close_clears_selection_and_target_category() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.preset_apply_target_category = Some(cat_id);
    state.dialogs.preset_picker_selected = Some("my-preset".to_string());
    state
        .popups
        .open_at_focused(APPLY_WORKSPACE_POPUP_ID, FIXED_POS);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(APPLY_WORKSPACE_POPUP_ID));
    assert!(state.dialogs.preset_apply_target_category.is_none());
    assert!(state.dialogs.preset_picker_selected.is_none());
}

#[test]
fn preset_apply_cancel_action_close_clears_selection_and_target_category() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.preset_apply_target_category = Some(cat_id);
    state.dialogs.preset_picker_selected = Some("my-preset".to_string());
    state
        .popups
        .open_at_focused(APPLY_WORKSPACE_POPUP_ID, FIXED_POS);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(APPLY_WORKSPACE_POPUP_ID));
    assert!(state.dialogs.preset_apply_target_category.is_none());
    assert!(state.dialogs.preset_picker_selected.is_none());
}
