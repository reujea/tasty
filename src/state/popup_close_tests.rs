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

/// 창 범위 팝업을 고정 위치에 인스턴스 데이터와 함께 포커스를 주어 연다.
fn open_with_payload(
    state: &mut crate::state::RequestContext,
    id: PopupId,
    payload: crate::adapters::ui::popup::PopupPayload,
) {
    state.popups.open_request(
        &crate::intent::OpenRequest::new(id, crate::model::popup_kind::PopupScope::Window)
            .at(FIXED_POS)
            .focused()
            .with_payload(payload),
        true,
    );
}

/// 이 선언의 열린 인스턴스 중 데이터를 가진 것이 있는지.
fn holds_payload(state: &crate::state::RequestContext, id: PopupId) -> bool {
    state
        .popups
        .instances_of(id)
        .any(|p| !matches!(p.payload, crate::adapters::ui::popup::PopupPayload::None))
}

/// 변환 팝업을 고정 위치에 대상 데이터와 함께 연다. 선택이 있는 상태다.
fn open_convert_with_selection(state: &mut crate::state::RequestContext, surface_id: u32) {
    let request = crate::intent::OpenRequest::new(
        CONVERT_SURFACE_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
    )
    .at(FIXED_POS)
    .focused()
    .with_payload(crate::adapters::ui::popup::PopupPayload::Convert(
        crate::adapters::ui::popup::convert::ConvertPayload {
            surface_id,
            selected: Some(0),
        },
    ));
    state.popups.open_request(&request, true);
}

/// 닫힌 뒤 남은 항목(닫힌 원본)에 대상·선택이 남지 않았는지 본다.
fn convert_data_left(state: &crate::state::RequestContext) -> bool {
    state
        .popups
        .entries_of(CONVERT_SURFACE_POPUP_ID)
        .any(|p| !matches!(p.payload, crate::adapters::ui::popup::PopupPayload::None))
}

#[test]
fn convert_surface_escape_close_drops_the_instance_data() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_id = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    open_convert_with_selection(&mut state, surface_id);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(CONVERT_SURFACE_POPUP_ID));
    assert!(!convert_data_left(&state));
}

#[test]
fn convert_surface_outside_click_drops_the_instance_data() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_id = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    open_convert_with_selection(&mut state, surface_id);

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(CONVERT_SURFACE_POPUP_ID));
    assert!(!convert_data_left(&state));
}

#[test]
fn convert_surface_close_intent_drops_the_instance_data() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let surface_id = engine
        .workspace_at(0)
        .expect("workspace index is valid")
        .all_surface_ids()[0];
    open_convert_with_selection(&mut state, surface_id);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: CONVERT_SURFACE_POPUP_ID,
        }
        .from_user_menu("test"),
    );

    assert!(!state.popups.is_open(CONVERT_SURFACE_POPUP_ID));
    assert!(!convert_data_left(&state));
}

#[test]
fn rename_escape_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    crate::adapters::ui::rename_popup::open_for_test(
        &mut state,
        crate::adapters::ui::rename_popup::RenamePayload::new(
            RenameTarget::NewCategory,
            "abc".to_string(),
        ),
        crate::model::popup_kind::PopupScope::Window,
        Some(FIXED_POS),
    );

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(!crate::adapters::ui::rename_popup::data_left(&state));
}

#[test]
fn rename_x_button_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    crate::adapters::ui::rename_popup::open_for_test(
        &mut state,
        crate::adapters::ui::rename_popup::RenamePayload::new(
            RenameTarget::NewCategory,
            "abc".to_string(),
        ),
        crate::model::popup_kind::PopupScope::Window,
        Some(FIXED_POS),
    );
    let (pos, size) = primed_popup_geometry(RENAME_POPUP_ID, &mut state, &mut engine);

    run_frame(
        press_input(close_button_point(pos, size)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(!crate::adapters::ui::rename_popup::data_left(&state));
}

#[test]
fn rail_category_escape_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    open_with_payload(
        &mut state,
        RAIL_CATEGORY_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::RailCategory(cat_id),
    );

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
    assert!(!holds_payload(&state, RAIL_CATEGORY_POPUP_ID));
}

#[test]
fn rail_category_close_intent_now_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    open_with_payload(
        &mut state,
        RAIL_CATEGORY_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::RailCategory(cat_id),
    );

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: RAIL_CATEGORY_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
    assert!(!holds_payload(&state, RAIL_CATEGORY_POPUP_ID));

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
}

#[test]
fn rail_category_outside_click_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    open_with_payload(
        &mut state,
        RAIL_CATEGORY_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::RailCategory(cat_id),
    );

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(RAIL_CATEGORY_POPUP_ID));
    assert!(!holds_payload(&state, RAIL_CATEGORY_POPUP_ID));
}

// 타이틀바와 바깥 클릭 닫기가 없는 진행 팝업은 빈 rows로 내용의 Close를 유도한다.

#[test]
fn transfer_progress_empty_rows_close_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_progress = Some(TransferProgress { rows: vec![] });
    state.popups.open_at_focused(
        TRANSFER_PROGRESS_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!state.popups.is_open(TRANSFER_PROGRESS_POPUP_ID));
    assert!(state.dialogs.transfer_progress.is_none());
}

#[test]
fn transfer_progress_close_intent_now_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.transfer_progress = Some(TransferProgress { rows: vec![] });
    state.popups.open_at_focused(
        TRANSFER_PROGRESS_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
        scope: crate::model::popup_kind::PopupScope::Surface(1),
        name: name.to_string(),
        reason: "boom".to_string(),
        retry: None,
        kind: crate::adapters::ui::popup::transfer::TransferErrorKind::Upload,
    }
}

#[test]
fn transfer_error_escape_close_pops_single_entry() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state
        .dialogs
        .transfer_error
        .push_back(xfer_err("a.txt").into());
    state.popups.open_at_focused(
        TRANSFER_ERROR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(TRANSFER_ERROR_POPUP_ID));
    assert!(state.dialogs.transfer_error.is_empty());
}

#[test]
fn transfer_error_outside_click_with_single_entry_closes_without_reopen() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state
        .dialogs
        .transfer_error
        .push_back(xfer_err("a.txt").into());
    state.popups.open_at_focused(
        TRANSFER_ERROR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
    state
        .dialogs
        .transfer_error
        .push_back(xfer_err("a.txt").into());
    state
        .dialogs
        .transfer_error
        .push_back(xfer_err("b.txt").into());
    state.popups.open_at_focused(
        TRANSFER_ERROR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
    state
        .dialogs
        .transfer_error
        .push_back(xfer_err("a.txt").into());
    state
        .dialogs
        .transfer_error
        .push_back(xfer_err("b.txt").into());
    state.popups.open_at_focused(
        TRANSFER_ERROR_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
    open_with_payload(
        &mut state,
        SCRIPT_CHANGED_CONFIRM_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ScriptConfirm(Box::new(pending_script_confirm(
            None,
        ))),
    );

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: SCRIPT_CHANGED_CONFIRM_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(SCRIPT_CHANGED_CONFIRM_POPUP_ID));
    assert!(!holds_payload(&state, SCRIPT_CHANGED_CONFIRM_POPUP_ID));

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(state.dialogs.approved_script.is_none());
}

// Run 결과는 다음 App 처리에서 실행하므로 닫기 훅이 지우면 안 된다.
#[test]
fn script_changed_confirm_close_intent_preserves_decided_result_for_dispatch() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_with_payload(
        &mut state,
        SCRIPT_CHANGED_CONFIRM_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ScriptConfirm(Box::new(pending_script_confirm(
            Some(true),
        ))),
    );

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
        .approved_script
        .as_ref()
        .expect("Run 결과는 실행 요청 처리까지 남아 있어야 한다");
    assert_eq!(pending.result, Some(true));
}

#[test]
fn command_palette_outside_click_close_resets_query_and_selection() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_with_payload(
        &mut state,
        COMMAND_PALETTE_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::CommandPalette(
            crate::state::command_palette::PaletteInput {
                query: "workspace".to_string(),
                selected: 2,
            },
        ),
    );

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(COMMAND_PALETTE_POPUP_ID));
    assert!(!holds_payload(&state, COMMAND_PALETTE_POPUP_ID));
    state.popups.open(
        COMMAND_PALETTE_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
    );
    assert!(!holds_payload(&state, COMMAND_PALETTE_POPUP_ID));
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
    state.popups.open_at_focused(
        PORT_SCANNER_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
        .push_back(info_modal_entry("a").into());
    state
        .dialogs
        .info_modal_queue
        .push_back(info_modal_entry("b").into());
    state.popups.open_at_focused(
        INFO_MODAL_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
        .push_back(info_modal_entry("only").into());
    state.popups.open_at_focused(
        INFO_MODAL_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
    open_with_payload(
        &mut state,
        CONFIRM_DELETE_CATEGORY_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ConfirmDeleteCategory(cat_id),
    );

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
    assert!(!holds_payload(&state, CONFIRM_DELETE_CATEGORY_POPUP_ID));
}

#[test]
fn confirm_delete_category_outside_click_clears_dialog_state() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = 1;
    open_with_payload(
        &mut state,
        CONFIRM_DELETE_CATEGORY_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ConfirmDeleteCategory(cat_id),
    );

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
    assert!(!holds_payload(&state, CONFIRM_DELETE_CATEGORY_POPUP_ID));
}

#[test]
fn confirm_delete_category_close_intent_drops_its_target_at_once() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = 1;
    open_with_payload(
        &mut state,
        CONFIRM_DELETE_CATEGORY_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ConfirmDeleteCategory(cat_id),
    );

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: CONFIRM_DELETE_CATEGORY_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
    assert!(!holds_payload(&state, CONFIRM_DELETE_CATEGORY_POPUP_ID));

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!state.popups.is_open(CONFIRM_DELETE_CATEGORY_POPUP_ID));
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
    open_with_payload(
        state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ConfirmForceDetach(ws_id),
    );
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
    assert!(!holds_payload(
        &state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID
    ));
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
    assert!(!holds_payload(
        &state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID
    ));
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
}

#[test]
fn confirm_force_detach_close_intent_drops_its_target_at_once() {
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
    assert!(!holds_payload(
        &state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID
    ));

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!holds_payload(
        &state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID
    ));
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
    assert!(!holds_payload(
        &state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID
    ));
}

#[test]
fn confirm_force_detach_closes_when_the_workspace_is_gone() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_with_payload(
        &mut state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
        crate::adapters::ui::popup::PopupPayload::ConfirmForceDetach(u32::MAX),
    );

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(
        !state
            .popups
            .is_open(CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID)
    );
    assert!(!holds_payload(
        &state,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID
    ));
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
        ws_id,
    );
    let intents = state.take_pending_intents();
    assert_eq!(intents.len(), 1);
    assert!(intents[0].origin.is_user());
    assert!(
        matches!(&intents[0].body, Intent::Engine(crate::app::engine_action::EngineAction::DetachWorkspace { workspace, holder: 7, grant: original }) if *workspace == ws_id && *original == grant)
    );
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
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

    crate::adapters::ui::popup::confirm_force_detach_workspace::apply_force_detach(
        &mut state,
        &engine.read(),
        ws_id,
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
fn confirm_force_detach_for_an_unheld_workspace_detaches_nothing() {
    let (mut state, mut owner) = test_state();
    let mut engine = owner.borrow_mut();
    let (ws_id, _) = occupied_workspace_with_popup(&mut state, &mut engine);
    crate::adapters::ui::popup::confirm_force_detach_workspace::apply_force_detach(
        &mut state,
        &engine.read(),
        u32::MAX,
    );
    assert!(state.take_pending_intents().is_empty());
    assert_eq!(engine.live.occupancy.workspace_holder(ws_id), Some(7));
}

/// Given 세 확인 팝업이 각자 대상을 가진 채 열려 있을 때, When 모두 닫으면, Then 어느 인스턴스에도
/// 대상이 남지 않고 같은 선언을 다시 열어도 앞 대상이 보이지 않는다.
#[test]
fn a_closed_confirm_leaves_no_target_behind() {
    use crate::adapters::ui::popup::PopupPayload;
    use crate::adapters::ui::popup::confirm_close_remote::{
        CONFIRM_CLOSE_REMOTE_POPUP_ID, PendingCloseRemote,
    };
    let (mut state, mut owner) = test_state();
    let mut engine = owner.borrow_mut();
    let close = crate::app::attach_client::workspace_close::SavedRowClose {
        row: 9,
        last: false,
    };
    let opened = [
        (
            CONFIRM_DELETE_CATEGORY_POPUP_ID,
            PopupPayload::ConfirmDeleteCategory(1),
        ),
        (
            CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
            PopupPayload::ConfirmForceDetach(1),
        ),
        (
            CONFIRM_CLOSE_REMOTE_POPUP_ID,
            PopupPayload::ConfirmCloseRemote(Box::new(PendingCloseRemote::new(
                close,
                crate::app::command::DomainIntent::CloseWorkspace { workspace_id: 9 },
            ))),
        ),
    ];
    let ids: Vec<PopupId> = opened.iter().map(|(id, _)| *id).collect();
    for (id, payload) in opened {
        open_with_payload(&mut state, id, payload);
        assert!(holds_payload(&state, id), "{id}");
    }
    for id in &ids {
        state.popups.close(id);
    }
    run_frame(empty_input(), &mut state, &mut engine);
    for id in ids {
        assert!(!state.popups.is_open(id), "{id}");
        state
            .popups
            .open(id, crate::model::popup_kind::PopupScope::Window);
        assert!(!holds_payload(&state, id), "{id}");
    }
}

// 파일 핸들러 피커는 타이틀바와 바깥 클릭 닫기가 없어 해당 포인터 경로 시험은 없다.

fn mk_picker_data() -> FileHandlerPickerData {
    FileHandlerPickerData {
        origin_surface_id: None,
        result_place: Default::default(),
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
        opened_in: None,
        row_link: None,
    }
}

#[test]
fn file_handler_picker_escape_close_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_picker_at_fixed_pos(&mut state);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(PICKER_POPUP_ID));
    assert!(matches!(
        finished_picker_results(&state)[..],
        [Some(FileHandlerPickerResult::Cancelled)]
    ));
}

#[test]
fn file_handler_picker_close_intent_now_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_picker_at_fixed_pos(&mut state);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: PICKER_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(PICKER_POPUP_ID));
    assert!(finished_picker_results(&state).is_empty());

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(matches!(
        finished_picker_results(&state)[..],
        [Some(FileHandlerPickerResult::Cancelled)]
    ));
}

fn open_picker_at_fixed_pos(state: &mut crate::state::MainViewState) {
    crate::intent::popup::reopen_for_test(
        state,
        &crate::intent::OpenRequest::new(
            PICKER_POPUP_ID,
            crate::model::popup_kind::PopupScope::Window,
        )
        .at(FIXED_POS)
        .focused()
        .with_payload(crate::adapters::ui::popup::PopupPayload::HandlerPicker(
            Box::new(mk_picker_data()),
        )),
    );
}

fn finished_picker_results(
    state: &crate::state::MainViewState,
) -> Vec<Option<FileHandlerPickerResult>> {
    state
        .dialogs
        .finished_handler_pickers
        .iter()
        .map(|p| p.result.clone())
        .collect()
}

fn mk_file_picker_data() -> FilePickerData {
    FilePickerData {
        save_name: None,
        folder: false,
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
    open_file_picker_at_fixed_pos(&mut state);

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert_eq!(
        finished_file_picker_results(&state),
        [FilePickerResult::Cancelled]
    );
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
    let picker = state
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .map(|p| p.instance)
        .expect("an open picker");
    crate::adapters::ui::popup::header_drag::reported(&ctx, picker)
        .expect("file picker 뷰가 헤더 줄을 보고해야 한다")
}

/// 셸 타이틀바가 없어 닫기 ×는 위젯 헤더 오른쪽 끝에 있다. 그 버튼으로 닫아도 헤더 드래그가
/// 가로채지 않고 Cancelled 가 남는다. 실제 클릭처럼 버튼 위에 먼저 올린 뒤 누른다 — 매니저는
/// 직전 프레임에 위젯이 포인터를 가졌는지로 버튼과 드래그 영역을 가른다.
#[test]
fn file_picker_header_close_button_marks_cancelled() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_file_picker_at_fixed_pos(&mut state);
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
    assert_eq!(
        finished_file_picker_results(&state),
        [FilePickerResult::Cancelled]
    );
}

/// 헤더의 빈 곳(제목 오른쪽)을 끌면 popup 이 이동하고 닫히지 않는다.
#[test]
fn file_picker_header_drag_moves_the_popup() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    open_file_picker_at_fixed_pos(&mut state);
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
    open_file_picker_at_fixed_pos(&mut state);

    crate::intent::popup::handle(
        &mut state,
        &mut engine,
        &UiIntent::ClosePopup {
            id: FILE_PICKER_POPUP_ID,
        }
        .from_user_menu("test"),
    );
    assert!(!state.popups.is_open(FILE_PICKER_POPUP_ID));
    assert!(finished_file_picker_results(&state).is_empty());

    run_frame(empty_input(), &mut state, &mut engine);

    assert_eq!(
        finished_file_picker_results(&state),
        [FilePickerResult::Cancelled]
    );
}

fn open_file_picker_at_fixed_pos(state: &mut crate::state::MainViewState) {
    crate::intent::popup::reopen_for_test(
        state,
        &crate::intent::OpenRequest::new(
            FILE_PICKER_POPUP_ID,
            crate::model::popup_kind::PopupScope::Window,
        )
        .at(FIXED_POS)
        .focused()
        .with_payload(crate::adapters::ui::popup::PopupPayload::FilePicker(
            Box::new(mk_file_picker_data()),
        )),
    );
}

fn finished_file_picker_results(state: &crate::state::MainViewState) -> Vec<FilePickerResult> {
    state
        .dialogs
        .finished_file_pickers
        .iter()
        .map(|f| f.result.clone())
        .collect()
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
    state.popups.open_at_focused(
        APPROVAL_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!state.popups.is_open(APPROVAL_POPUP_ID));
    assert!(state.dialogs.approval_comment_buffer.is_empty());
}

#[test]
fn approval_x_button_close_with_empty_queue_does_not_refire() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    state.dialogs.approval_comment_buffer = "draft comment".to_string();
    state.popups.open_at_focused(
        APPROVAL_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );
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
            .any(|d| matches!(&d.body, Intent::Ui(UiIntent::OpenPopup(crate::intent::OpenRequest { id, .. })) if *id == APPROVAL_POPUP_ID)),
        "empty queue must not refire OpenPopup"
    );
}

#[test]
fn approval_x_button_close_with_pending_queue_refires_open_popup() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    push_approval(&mut engine, &mut state, "req-1");
    push_approval(&mut engine, &mut state, "req-2");
    state.popups.open_at_focused(
        APPROVAL_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );
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
            .any(|d| matches!(&d.body, Intent::Ui(UiIntent::OpenPopup(crate::intent::OpenRequest { id, .. })) if *id == APPROVAL_POPUP_ID)),
        "non-empty queue must refire OpenPopup for the next head"
    );
}

#[test]
fn approval_close_intent_now_refires_open_popup() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    push_approval(&mut engine, &mut state, "req-1");
    push_approval(&mut engine, &mut state, "req-2");
    state.popups.open_at_focused(
        APPROVAL_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

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
            .any(|d| matches!(&d.body, Intent::Ui(UiIntent::OpenPopup(crate::intent::OpenRequest { id, .. })) if *id == APPROVAL_POPUP_ID)),
        "non-empty queue must refire OpenPopup for the next head"
    );
}

#[test]
fn close_intent_now_clears_cleanup_after_next_frame() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    crate::adapters::ui::rename_popup::open_for_test(
        &mut state,
        crate::adapters::ui::rename_popup::RenamePayload::new(
            RenameTarget::NewCategory,
            "abc".to_string(),
        ),
        crate::model::popup_kind::PopupScope::Window,
        Some(FIXED_POS),
    );

    let dispatched = UiIntent::ClosePopup {
        id: RENAME_POPUP_ID,
    }
    .from_user_menu("test");
    crate::intent::popup::handle(&mut state, &mut engine, &dispatched);

    assert!(!state.popups.is_open(RENAME_POPUP_ID));
    assert!(!crate::adapters::ui::rename_popup::data_left(&state));

    run_frame(empty_input(), &mut state, &mut engine);

    assert!(!crate::adapters::ui::rename_popup::data_left(&state));
}

// 프리셋 팝업 셋이 같은 닫기 훅을 사용해 workspace 팝업으로 공통 상태 정리를 검사한다.

#[test]
fn preset_apply_x_button_close_clears_selection_and_target_category() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.preset_apply_target_category = Some(cat_id);
    state.dialogs.preset_picker_selection.set(
        tasty_presets::PresetKind::Workspace,
        Some("my-preset".to_string()),
    );
    state.popups.open_at_focused(
        APPLY_WORKSPACE_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );
    let (pos, size) = primed_popup_geometry(APPLY_WORKSPACE_POPUP_ID, &mut state, &mut engine);

    run_frame(
        press_input(close_button_point(pos, size)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(APPLY_WORKSPACE_POPUP_ID));
    assert!(state.dialogs.preset_apply_target_category.is_none());
    assert!(
        state
            .dialogs
            .preset_picker_selection
            .get(tasty_presets::PresetKind::Workspace)
            .is_none()
    );
}

#[test]
fn preset_apply_outside_click_close_clears_selection_and_target_category() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.preset_apply_target_category = Some(cat_id);
    state.dialogs.preset_picker_selection.set(
        tasty_presets::PresetKind::Workspace,
        Some("my-preset".to_string()),
    );
    state.popups.open_at_focused(
        APPLY_WORKSPACE_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

    run_frame(
        press_input(outside_point(FIXED_POS)),
        &mut state,
        &mut engine,
    );

    assert!(!state.popups.is_open(APPLY_WORKSPACE_POPUP_ID));
    assert!(state.dialogs.preset_apply_target_category.is_none());
    assert!(
        state
            .dialogs
            .preset_picker_selection
            .get(tasty_presets::PresetKind::Workspace)
            .is_none()
    );
}

#[test]
fn preset_apply_cancel_action_close_clears_selection_and_target_category() {
    let (mut state, mut engine_session) = test_state();
    let mut engine = engine_session.borrow_mut();
    let cat_id = engine.categories()[0].id;
    state.dialogs.preset_apply_target_category = Some(cat_id);
    state.dialogs.preset_picker_selection.set(
        tasty_presets::PresetKind::Workspace,
        Some("my-preset".to_string()),
    );
    state.popups.open_at_focused(
        APPLY_WORKSPACE_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
        FIXED_POS,
    );

    run_frame(key_input(egui::Key::Escape), &mut state, &mut engine);

    assert!(!state.popups.is_open(APPLY_WORKSPACE_POPUP_ID));
    assert!(state.dialogs.preset_apply_target_category.is_none());
    assert!(
        state
            .dialogs
            .preset_picker_selection
            .get(tasty_presets::PresetKind::Workspace)
            .is_none()
    );
}

/// 다음 실패는 그 실패의 범위로 다시 연다.
#[test]
fn the_next_transfer_error_opens_in_its_own_surface() {
    let (mut state, mut engine_session) = test_state();
    let engine = engine_session.borrow_mut();
    let mut first = xfer_err("a.txt");
    first.scope = crate::model::popup_kind::PopupScope::Surface(7);
    let mut second = xfer_err("b.txt");
    second.scope = crate::model::popup_kind::PopupScope::Workspace(8);
    state.dialogs.transfer_error.push_back(first.into());
    state.dialogs.transfer_error.push_back(second.into());
    crate::adapters::ui::popup::transfer::on_close_transfer_error(
        &egui::Context::default(),
        &mut state,
        &engine.read(),
        &crate::adapters::ui::popup::ClosedPopup::test("test"),
    );
    let popup = state
        .popups
        .get_mut(TRANSFER_ERROR_POPUP_ID)
        .expect("transfer error is registered");
    assert!(popup.open);
    assert_eq!(
        popup.scope,
        crate::model::popup_kind::PopupScope::Workspace(8)
    );
}

fn xfer_row(id: u64, surface_id: u32) -> crate::adapters::ui::popup::transfer::TransferRow {
    crate::adapters::ui::popup::transfer::TransferRow {
        id,
        surface_id,
        name: "x".into(),
        sent: 0,
        total: 1,
        rate: String::new(),
    }
}

/// 업로드 진행 팝업은 처음 연 surface 범위에 고정된다. 행이 끝나거나 새 업로드가 시작돼도
/// 범위를 다시 쓰지 않는다(App 의 업로드 시작·종료 경로).
#[test]
fn the_upload_progress_stays_in_the_surface_it_opened_in() {
    use crate::adapters::ui::popup::transfer::{
        TRANSFER_PROGRESS_POPUP_ID, begin_progress_row, finish_progress_row,
    };
    use crate::model::popup_kind::PopupScope;
    let (mut state, _engine_session) = test_state();
    begin_progress_row(&mut state, xfer_row(1, 7));
    begin_progress_row(&mut state, xfer_row(2, 8));
    assert_eq!(
        state.popups.open_scope(TRANSFER_PROGRESS_POPUP_ID),
        Some(&PopupScope::Surface(7))
    );
    finish_progress_row(&mut state, 1);
    assert_eq!(
        state.popups.open_scope(TRANSFER_PROGRESS_POPUP_ID),
        Some(&PopupScope::Surface(7))
    );
    finish_progress_row(&mut state, 2);
    assert!(!state.popups.is_open(TRANSFER_PROGRESS_POPUP_ID));
}

/// 업로드 실패는 surface 가 있으면 그 surface, 닫혔으면 그 mirror workspace 범위로 알린다.
#[test]
fn an_upload_failure_falls_back_to_its_mirror_workspace() {
    use crate::adapters::ui::popup::transfer::upload_error_scope;
    use crate::model::popup_kind::PopupScope;
    assert_eq!(
        upload_error_scope(7, true, 3, true),
        Some(PopupScope::Surface(7))
    );
    assert_eq!(
        upload_error_scope(7, false, 3, true),
        Some(PopupScope::Workspace(3))
    );
    assert_eq!(upload_error_scope(7, false, 3, false), None);
}

/// 실패 항목은 재시도 내용과 함께 큐에 남고 그 항목의 범위로 실패 팝업을 연다.
#[test]
fn a_pushed_failure_opens_in_its_own_scope() {
    use crate::adapters::ui::popup::transfer::push_error;
    use crate::model::popup_kind::PopupScope;
    let (mut state, _engine_session) = test_state();
    let mut err = xfer_err("a.png");
    err.scope = PopupScope::Workspace(3);
    push_error(&mut state, err);
    let intents = state.take_pending_intents();
    assert!(
        matches!(
            &intents[..],
            [crate::intent::DispatchedIntent {
                body: crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup(
                    crate::intent::OpenRequest {
                        id: TRANSFER_ERROR_POPUP_ID,
                        scope: PopupScope::Workspace(3),
                        placement: crate::intent::PopupPlacement::ScopeCenter,
                        focus: true,
                        ..
                    }
                )),
                ..
            }]
        ),
        "{intents:?}"
    );
    assert_eq!(state.dialogs.transfer_error.len(), 1);
}
