//! 팝업 정의 목록.

use std::sync::OnceLock;

use super::{
    PopupAnchor, PopupChrome, PopupContentMargin, PopupDecl, PopupDef, PopupDragHandle,
    PopupEscape, PopupFit, PopupInstances, PopupKeyboard, PopupOutsideClick, PopupResizeHandle,
    PopupScopeKind, PopupShadow, PopupShell, PopupSize, PopupState, ScrimInput, popup_size,
};

/// host 팝업이 지금 공통으로 갖는 선언 값. `PopupDecl::new`의 기본값과 다른 것은 두 가지다.
/// 덮지 않음(`covers_scope = false`)과 인스턴스 하나(`Single`)다.
/// 바깥 클릭·제목줄·이동·크기 조절처럼 팝업마다 다른 값은 정의마다 다시 적는다.
pub(crate) const fn host_decl(size: PopupSize) -> PopupDecl {
    PopupDecl {
        covers_scope: false,
        instances: PopupInstances::Single,
        ..PopupDecl::new(PopupScopeKind::Window, size)
    }
}

/// 실제 헤더 영역을 아직 보고하지 않은 첫 프레임의 드래그 영역.
/// 다음 프레임부터 뷰가 보고한 영역을 사용하며 겹치는 위젯 입력이 우선한다.
/// 설정 창 파일 선택기도 같은 규칙을 쓴다.
pub(crate) fn panel_header_drag_strip(s: &PopupState) -> egui::Rect {
    egui::Rect::from_min_size(
        s.pos,
        egui::vec2(s.size.x * 0.5, super::title_bar_height().value()),
    )
}

/// 프로세스 수명 내내 살아있는 정적 popup 정의 목록.
pub fn all_defs() -> &'static [PopupDef] {
    static DEFS: OnceLock<Vec<PopupDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        // Theme 읽기 잠금을 아래 vec! 식 전체 동안 잡고 있지 않도록 값만 먼저 꺼낸다.
        let th = crate::theme::theme();
        let palette_width = th.palette_width().value();
        let modhint_size = popup_size(egui::vec2(th.modhint_width().value(), th.modhint_height().value()));
        let modhint_min = popup_size(egui::vec2(
            th.modhint_min_width().value(),
            th.modhint_min_height().value(),
        ));
        let modhint_grip = th.modhint_grip_size();
    let notice_width = th.notice_column_width().value();
        drop(th);
        vec![
            PopupDef {
                id: "notifications",
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    // 사이드바 클릭은 AtFocused로 열어 포커스를 준다. 단축키는 지금 Default로 열어
                    // 포커스를 주지 않는다(열기 경로 사이의 불일치).
                    ..host_decl(popup_size(crate::adapters::ui::notification::notifications_popup_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "notification_panel.window_title",
                fullscreen_stage: Some(
                    crate::adapters::ui::fullscreen::notifications::NOTIFICATIONS_STAGE_ID,
                ),
                title_fn: None,
                sizer: Some(crate::adapters::ui::notification::notifications_popup_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::notification::draw_notification_popup,
                // Preserve the expanded row and popup scroll position across reopen.
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: "convert_surface",
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    shell: PopupShell::Panel,
                    // surface마다 따로 연다. 대상과 선택은 인스턴스 데이터다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(super::convert::convert_popup_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "convert_popup.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::convert::convert_popup_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::convert::draw_convert_popup,
                // 대상과 선택은 인스턴스 데이터라 닫으면 함께 사라진다.
                #[cfg(debug_assertions)]
                debug_payload: Some(super::convert::debug_payload),
                on_close: None,
            },
            PopupDef {
                id: super::explorer_conflict::EXPLORER_CONFLICT_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    content_margin: PopupContentMargin::None,
                    escape: PopupEscape::Unfocus,
                    // 질문을 띄운 칸마다 따로 연다. 칸은 인스턴스 데이터다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(super::explorer_conflict::default_size()))
                },
                scrim_input: ScrimInput::BlocksDrop,
                title_key: "explorer.conflict.popup_title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::explorer_conflict::sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::explorer_conflict::draw,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::explorer_conflict::on_close),
            },
            PopupDef {
                id: super::script_confirm::SCRIPT_CONFIRM_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    content_margin: PopupContentMargin::None,
                    ..host_decl(popup_size(super::script_confirm::script_confirm_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "script.confirm.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::script_confirm::script_confirm_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::script_confirm::draw_script_confirm_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::script_confirm::on_close_script_confirm_popup),
            },
            PopupDef {
                id: "rename",
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    // dialog 라 dialog-bg 다. 그 위라야 읽기 전용 이름 칸의 상자가 보인다.
                    shell: PopupShell::Dialog,
                    content_margin: PopupContentMargin::None,
                    keyboard: PopupKeyboard::Text,
                    // 범위(대상이 보이는 자리)마다 따로 연다. 대상·입력은 인스턴스 데이터다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(crate::adapters::ui::dialog::rename_popup_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "rename_dialog.tab_heading",
                fullscreen_stage: None,
                title_fn: Some(crate::adapters::ui::dialog::rename_popup_title),
                sizer: Some(crate::adapters::ui::dialog::rename_popup_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::dialog::draw_rename_popup,
                // 대상·입력·원격 대기는 인스턴스 데이터라 닫으면 함께 사라진다.
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            // 탐색기 Properties — 그 탐색기 칸 범위, 머리를 직접 그린다(글리프 · 이름 · 닫기).
            PopupDef {
                id: super::explorer_properties::EXPLORER_PROPERTIES_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    // 시안 바탕이 bg-panel 이고 읽기 실패 화면의 Retry 가 Secondary 버튼이다.
                    shell: PopupShell::Panel,
                    escape: PopupEscape::Unfocus,
                    // 탐색기 칸마다 따로 연다. 항목과 읽기는 인스턴스 데이터다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(super::explorer_properties::default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "explorer.properties.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::explorer_properties::sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::explorer_properties::draw,
                // 항목과 읽기는 인스턴스 데이터라 닫으면 함께 사라지고 세기도 멈춘다.
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: "search_bar",
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    // 검색 바 테두리는 search-bar-border 다.
                    shell: PopupShell::SearchBar,
                    shadow: PopupShadow::Popover,
                    anchor: PopupAnchor::ScopeTop,
                    fit: PopupFit::Shrink,
                    // 검색어 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    ..host_decl(popup_size(crate::adapters::ui::search_bar::search_bar_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "search.placeholder",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(crate::adapters::ui::search_bar::search_bar_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::search_bar::draw_search_bar,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(crate::adapters::ui::search_bar::on_close_search_bar),
            },
            PopupDef {
                id: crate::adapters::ui::info_modal::INFO_MODAL_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    // 제목줄 아래 선은 info modal 셸 규칙(info-modal-title-edge)을 따른다.
                    shell: PopupShell::InfoPanel,
                    content_margin: PopupContentMargin::None,
                    // 안내 여럿을 차례로 보인다. 다음 안내는 닫힌 안내의 출처로 연다.
                    instances: PopupInstances::Queue,
                    ..host_decl(popup_size(egui::vec2(440.0, 160.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "button.ok",
                fullscreen_stage: None,
                title_fn: Some(crate::adapters::ui::info_modal::info_modal_title),
                sizer: Some(crate::adapters::ui::info_modal::info_modal_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::info_modal::draw_info_modal,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(crate::adapters::ui::info_modal::on_close_info_modal),
            },
            PopupDef {
                id: super::approval::APPROVAL_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    escape: PopupEscape::Unfocus,
                    focus_on_open: false,
                    // 의견 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    // 승인 요청 여럿을 차례로 보인다. 다음 요청도 에이전트 출처로 연다.
                    instances: PopupInstances::Queue,
                    ..host_decl(popup_size(egui::vec2(480.0, 240.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "approval.popup.title",
                fullscreen_stage: None,
                title_fn: Some(super::approval::approval_popup_title),
                sizer: Some(super::approval::approval_popup_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::approval::draw_approval_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::approval::on_close_approval_popup),
            },
            PopupDef {
                id: super::file_handler_picker::PICKER_POPUP_ID,
                decl: PopupDecl {
                    // scrim 클릭으로는 안 닫힌다 — 닫힘은 Esc 와 Cancel 뿐이다.
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::Region,
                    resizable: false,
                    min_size: None,
                    // 배경은 default Tag 채움색과 달라야 한다(popup_shell_fill_keeps_the_default_tag_visible).
                    shell: PopupShell::Panel,
                    // 선택 대상과 기다리는 요청은 인스턴스 데이터다. 범위마다 따로 연다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(super::file_handler_picker::picker_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "file_handler.picker.title",
                fullscreen_stage: None,
                // 자체 헤더가 있으므로 공통 타이틀바를 추가하지 않는다.
                title_fn: None,
                sizer: Some(super::file_handler_picker::picker_sizer),
                drag_region: Some(panel_header_drag_strip),
                on_open: None,
                draw_fn: super::file_handler_picker::draw_file_handler_picker,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::file_handler_picker::on_close_file_handler_picker),
            },
            PopupDef {
                id: super::file_picker::FILE_PICKER_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    // 위젯이 제목·닫기 헤더를 직접 그리므로 셸 타이틀바를 두지 않는다.
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::Region,
                    resizable: false,
                    min_size: None,
                    // 오류 상태의 Secondary 버튼(surface-raised)이 묻히지 않도록 시안 창 배경 bg-panel 을 쓴다.
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    // 저장 모드의 이름 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    // 탐색 상태와 결과를 받을 요청은 인스턴스 데이터다. 범위마다 피커 하나가 열린다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(egui::vec2(640.0, 480.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "filepicker.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::file_picker::picker_sizer),
                drag_region: Some(panel_header_drag_strip),
                on_open: None,
                draw_fn: super::file_picker::draw_file_picker,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::file_picker::on_close_file_picker),
            },
            PopupDef {
                id: super::port_scanner::PORT_SCANNER_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::Region,
                    resizable: true,
                    min_size: Some(popup_size(egui::vec2(480.0, 320.0))),
                    covers_scope: true,
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    // 검색 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    ..host_decl(popup_size(egui::vec2(660.0, 520.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "port_scanner.heading",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: Some(panel_header_drag_strip),
                on_open: None,
                draw_fn: |ui, state, engine, _| super::port_scanner::draw_port_scanner_popup(ui, state, engine),
                // 닫힌 경로(사유)에 따라 스캔 상태를 남길지 정한다.
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::port_scanner_close::on_close_port_scanner_popup),
            },
            PopupDef {
                id: super::command_palette::COMMAND_PALETTE_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: true,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    content_margin: PopupContentMargin::None,
                    // 검색어 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    // 처음 등록할 때의 전역 UI 배율로 palette-width를 한 번 계산한다. sizer가 매 프레임
                    // 실제 크기로 덮어쓰므로 이후 배율이 바뀌어도 동작에는 영향이 없다.
                    ..host_decl(popup_size(egui::vec2(palette_width, 412.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "command_palette.heading",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::command_palette::command_palette_sizer),
                drag_region: None,
                on_open: Some(super::command_palette::on_open_command_palette_popup),
                draw_fn: super::command_palette::draw_command_palette_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            // 소속 워크스페이스는 여는 시점의 WithScope에서 지정한다.
            PopupDef {
                id: super::dag_list::DAG_LIST_POPUP_ID,
                decl: PopupDecl {
                    // 그래프 pan 드래그가 창 밖에서 끝나는 일이 흔하다 — 바깥 클릭으로
                    // 닫으면 보던 그래프가 사라진다.
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: true,
                    min_size: Some(popup_size(egui::vec2(360.0, 280.0))),
                    // 시안 바탕이 bg-panel 이다. surface-raised 위에서는 검색 칸의 input-bg·input-border 가 바탕과 같아진다.
                    shell: PopupShell::Panel,
                    // 검색 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    // sizer가 배율을 적용한 토큰으로 크기를 계산한다.
                    ..host_decl(popup_size(egui::vec2(560.0, 460.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "dag_list.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::dag_list::dag_list_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::dag_list::draw_dag_list_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: super::remote_tool::REMOTE_TOOL_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::Region,
                    resizable: true,
                    min_size: Some(popup_size(egui::vec2(420.0, 320.0))),
                    covers_scope: true,
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    // 프로필 양식의 입력칸이 글자를 편집한다.
                    keyboard: PopupKeyboard::Text,
                    ..host_decl(popup_size(egui::vec2(520.0, 460.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "remote_tool.heading",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: Some(panel_header_drag_strip),
                on_open: None,
                draw_fn: |ui, state, engine, _| super::remote_tool::draw_remote_tool_popup(ui, state, engine),
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(|ctx, state, engine, _| {
                    super::remote_tool::on_close_remote_tool_popup(ctx, state, engine)
                }),
            },
            PopupDef {
                id: super::remote_attach::REMOTE_ATTACH_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    ..host_decl(popup_size(egui::vec2(680.0, 460.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "remote_attach.heading",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: None,
                on_open: None,
                draw_fn: |ui, state, engine, _| super::remote_attach::draw_remote_attach_popup(ui, state, engine),
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(|ctx, state, engine, _| {
                    super::remote_attach::on_close_remote_attach_popup(ctx, state, engine)
                }),
            },
            // 원격 전송 진행 — surface마다 하나, scrim 중앙 headless, close_on_outside_click=false
            // (전송 중 실수 dismiss 방지), 모든 행 완료 시 draw_fn 이 self-close.
            PopupDef {
                id: super::transfer::TRANSFER_PROGRESS_POPUP_ID,
                decl: PopupDecl {
                    instances: PopupInstances::PerScopeTarget,
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    escape: PopupEscape::Unfocus,
                    focus_on_open: false,
                    ..host_decl(popup_size(egui::vec2(400.0, 180.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "transfer.progress.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::transfer::transfer_progress_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::transfer::draw_transfer_progress,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            // 원격 전송 실패 — scrim 중앙 headless, 기본 dismiss(Esc/scrim). Retry 는
            // 전송 중 실패만(draw_fn 이 판정). danger-fill 버튼 금지.
            PopupDef {
                id: super::transfer::TRANSFER_ERROR_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    shell: PopupShell::Panel,
                    content_margin: PopupContentMargin::None,
                    // 실패 여럿을 차례로 보인다. 다음 실패는 닫힌 팝업의 출처로 연다.
                    instances: PopupInstances::Queue,
                    ..host_decl(popup_size(egui::vec2(400.0, 200.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "transfer.error.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::transfer::transfer_error_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::transfer::draw_transfer_error,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::transfer::on_close_transfer_error),
            },
            // 다운로드 후 실행 대화상자 — Esc·바깥 클릭은 Cancel 과 같다.
            PopupDef {
                id: super::download_run::DOWNLOAD_RUN_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(super::download_run::download_run_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "download_run.dialog.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::download_run::download_run_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::download_run::draw_download_run,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::download_run::on_close_download_run),
            },
            // 1 MiB 초과 확인 — 답하지 않고 닫으면(Cancel·Esc·바깥 클릭) 그 다운로드를 취소한다.
            PopupDef {
                id: super::download_run::DOWNLOAD_RUN_LARGE_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(super::download_run::download_run_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "download_run.large.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::download_run::download_run_large_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::download_run::draw_download_run_large,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::download_run::on_close_download_run_large),
            },
            // 다운로드 진행 — Esc·바깥 클릭으로 닫히지 않는다. 멈추는 것은 Stop download 하나다.
            PopupDef {
                id: super::download_run::DOWNLOAD_RUN_PROGRESS_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    escape: PopupEscape::Unfocus,
                    // 수신을 시작하면 open_centered로 열려 포커스를 받지 않는다.
                    focus_on_open: false,
                    ..host_decl(popup_size(super::download_run::download_run_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "transfer.progress.title_download_run",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::download_run::download_run_progress_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::download_run::draw_download_run_progress,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: super::preset_apply::APPLY_WORKSPACE_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(egui::vec2(360.0, 320.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "preset.popup.apply_workspace_title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: None,
                on_open: None,
                draw_fn: super::preset_apply::draw_apply_workspace_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::preset_apply::on_close_apply_workspace_popup),
            },
            PopupDef {
                id: super::preset_apply::APPLY_TAB_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(egui::vec2(360.0, 320.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "preset.popup.apply_tab_title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: None,
                on_open: None,
                draw_fn: super::preset_apply::draw_apply_tab_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::preset_apply::on_close_apply_tab_popup),
            },
            PopupDef {
                id: super::preset_apply::APPLY_PANE_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::TitleBar,
                    drag_handle: PopupDragHandle::TitleBar,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(egui::vec2(360.0, 320.0)))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "preset.popup.apply_pane_title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: None,
                on_open: None,
                draw_fn: super::preset_apply::draw_apply_pane_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(super::preset_apply::on_close_apply_pane_popup),
            },
            PopupDef {
                id: "tools_menu",
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    // 메뉴형 셸은 egui 팝오버와 같은 menu-border 를 쓴다.
                    shell: PopupShell::Menu,
                    shadow: PopupShadow::Popover,
                    anchor: PopupAnchor::AtPoint,
                    ..host_decl(popup_size(crate::adapters::ui::tools_menu::tools_menu_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "tools_menu.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(crate::adapters::ui::tools_menu::tools_menu_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::tools_menu::draw_tools_menu,
                // pending_open_preset_window는 App이 처리하는 요청이며 팝업이 닫힐 때 지우지 않는다.
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: crate::adapters::ui::mouse_capture_menu::MOUSE_CAPTURE_BANNER_MENU_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    // 배너 더보기 메뉴의 테두리는 banner-more-menu-border 다.
                    shell: PopupShell::BannerMenu,
                    shadow: PopupShadow::Popover,
                    anchor: PopupAnchor::AtPoint,
                    // 배너가 있는 surface마다 따로 연다. 대상은 인스턴스 데이터다.
                    instances: PopupInstances::PerScopeTarget,
                    ..host_decl(popup_size(crate::adapters::ui::mouse_capture_menu::menu_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "popup.mouse_capture_banner_menu.title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(crate::adapters::ui::mouse_capture_menu::menu_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::mouse_capture_menu::draw_menu,
                // 대상·크기는 인스턴스 데이터라 닫으면 함께 사라진다.
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: super::confirm_delete_category::CONFIRM_DELETE_CATEGORY_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(super::confirm_delete_category::confirm_delete_category_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "workspace_category.delete_confirm_title",
                fullscreen_stage: None,
                title_fn: Some(super::confirm_delete_category::confirm_delete_category_title),
                sizer: Some(super::confirm_delete_category::confirm_delete_category_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::confirm_delete_category::draw_confirm_delete_category,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: super::confirm_force_detach_workspace::CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    ..host_decl(popup_size(super::confirm_force_detach_workspace::confirm_force_detach_workspace_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "attach.force_detach_confirm_title",
                fullscreen_stage: None,
                title_fn: Some(
                    super::confirm_force_detach_workspace::confirm_force_detach_workspace_title,
                ),
                sizer: Some(
                    super::confirm_force_detach_workspace::confirm_force_detach_workspace_sizer,
                ),
                drag_region: None,
                on_open: None,
                draw_fn: super::confirm_force_detach_workspace::draw_confirm_force_detach_workspace,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            // 저장된 원격 행을 원격에서 닫기 전 확인 — Window 범위, scrim 가운데, 머리를 직접 그린다.
            PopupDef {
                id: super::confirm_close_remote::CONFIRM_CLOSE_REMOTE_POPUP_ID,
                decl: PopupDecl {
                    // 파괴적 확인이라 Esc·Cancel 로만 닫는다. 바깥 클릭은 무시한다.
                    outside_click: PopupOutsideClick::Unfocus,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: true,
                    ..host_decl(popup_size(super::confirm_close_remote::confirm_close_remote_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "remote.row.close_remote.confirm",
                fullscreen_stage: None,
                title_fn: Some(super::confirm_close_remote::confirm_close_remote_title),
                sizer: Some(super::confirm_close_remote::confirm_close_remote_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::confirm_close_remote::draw_confirm_close_remote,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            super::remote_row::def(),
            PopupDef {
                id: crate::adapters::ui::tutorial::topic_popup::TUTORIAL_TOPICS_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    shell: PopupShell::Panel,
                    ..host_decl(popup_size(crate::adapters::ui::tutorial::topic_popup::tutorial_topics_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "tutorial.popup_title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: None,
                drag_region: None,
                on_open: Some(crate::adapters::ui::tutorial::open_catalog),
                draw_fn: crate::adapters::ui::tutorial::topic_popup::draw_tutorial_topics_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(crate::adapters::ui::tutorial::topic_popup::on_close),
            },
            PopupDef {
                id: super::rail_category::RAIL_CATEGORY_POPUP_ID,
                decl: PopupDecl {
                    outside_click: PopupOutsideClick::Close,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    // 메뉴형 셸은 egui 팝오버와 같은 menu-border 를 쓴다.
                    shell: PopupShell::Menu,
                    shadow: PopupShadow::Popover,
                    anchor: PopupAnchor::AtPoint,
                    ..host_decl(popup_size(super::rail_category::rail_category_default_size()))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "workspace_category.heading",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(super::rail_category::rail_category_sizer),
                drag_region: None,
                on_open: None,
                draw_fn: super::rail_category::draw_rail_category_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            PopupDef {
                id: crate::adapters::ui::modifier_hint_overlay::MODIFIER_HINT_POPUP_ID,
                decl: PopupDecl {
                    // 키를 받지 않는다. 열거나 눌러도 포커스·순서가 바뀌지 않고 다른 팝업 아래에 그린다.
                    keyboard: PopupKeyboard::None,
                    escape: PopupEscape::Ignore,
                    outside_click: PopupOutsideClick::Ignore,
                    focus_on_open: false,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    // 위쪽 띠(닫기 버튼 폭 제외)로 옮기고 오른쪽 아래 그립으로만 크기를 바꾼다.
                    drag_handle: PopupDragHandle::Region,
                    resizable: true,
                    resize_handle: PopupResizeHandle::Grip {
                        size: modhint_grip,
                    },
                    min_size: Some(modhint_min),
                    // 셸·그림자·페이드는 콘텐츠가 modhint 토큰으로 그린다.
                    shell: PopupShell::Content,
                    shadow: PopupShadow::Popover,
                    content_margin: PopupContentMargin::None,
                    anchor: PopupAnchor::AtPoint,
                    ..host_decl(modhint_size)
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "settings.accessibility.modifier_hint",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(crate::adapters::ui::modifier_hint_overlay::default_size),
                drag_region: Some(crate::adapters::ui::modifier_hint_overlay::drag_strip),
                on_open: None,
                draw_fn: crate::adapters::ui::modifier_hint_overlay::draw_hint_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: None,
            },
            // 복원 실패·저장 멈춤 안내. 안내마다 인스턴스 하나를 안내 열에 연 순서로 쌓는다.
            PopupDef {
                id: crate::adapters::ui::restore_notice::RESTORE_NOTICE_POPUP_ID,
                decl: PopupDecl {
                    // 나타날 때나 눌러도 팝업 포커스를 받지 않고 다른 팝업 아래에 그린다. 안쪽 컨트롤의
                    // 키보드 포커스와 Esc는 콘텐츠가 다룬다.
                    keyboard: PopupKeyboard::None,
                    escape: PopupEscape::Ignore,
                    outside_click: PopupOutsideClick::Ignore,
                    focus_on_open: false,
                    sticky_focus: false,
                    chrome: PopupChrome::Headless,
                    drag_handle: PopupDragHandle::None,
                    resizable: false,
                    min_size: None,
                    covers_scope: false,
                    // 카드·그림자·테두리는 콘텐츠가 그린다.
                    shell: PopupShell::Content,
                    shadow: PopupShadow::Popover,
                    content_margin: PopupContentMargin::None,
                    anchor: PopupAnchor::Column,
                    instances: PopupInstances::Multi,
                    ..host_decl(popup_size(egui::vec2(
                        notice_width,
                        crate::adapters::ui::restore_notice::UNMEASURED_HEIGHT.value(),
                    )))
                },
                scrim_input: ScrimInput::VisualOnly,
                title_key: "notice.restore_failed_title",
                fullscreen_stage: None,
                title_fn: None,
                sizer: Some(crate::adapters::ui::restore_notice::notice_size),
                drag_region: None,
                on_open: None,
                draw_fn: crate::adapters::ui::restore_notice::draw_notice_popup,
                #[cfg(debug_assertions)]
                debug_payload: None,
                on_close: Some(crate::adapters::ui::restore_notice::on_close_notice),
            },
        ]
    })
}

/// id로 정의 하나를 찾는다.
pub fn find(id: &str) -> Option<&'static PopupDef> {
    all_defs().iter().find(|d| d.id == id)
}

/// 정의마다 선언 값이 옛 id 명부와 같은지 표로 확인한다.
#[cfg(test)]
#[path = "decl_table_tests.rs"]
mod decl_table_tests;
