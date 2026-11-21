//! 설정 창 파일 선택기 popup 의 셸 구조. 메인 파일 선택기처럼 셸 타이틀바가 없고
//! 뷰가 그린 헤더 한 줄이 이동 손잡이다. 설정 창 PopupManager 로 실제 등록값을 그린다.
use super::*;

const CONSUMER: &str = "file_chooser_popup_test";

fn screen_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(2000.0, 1500.0),
        )),
        ..Default::default()
    }
}

fn pointer_input(pos: egui::Pos2, pressed: Option<bool>) -> egui::RawInput {
    let mut raw = screen_input();
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

fn opened() -> SettingsUiState {
    let mut st = SettingsUiState::new();
    st.open_file_chooser(
        CONSUMER,
        file_chooser::FileChooserMode::Open,
        Vec::new(),
        None,
    );
    assert!(st.popups.is_open(FILE_PICKER_POPUP_ID));
    st
}

/// 설정 창 프레임의 파일 선택 부분만 같은 context 로 이어 그린다. 누름·뗌은 여러 프레임에 걸친다.
fn run_frames(ctx: &egui::Context, st: &mut SettingsUiState, inputs: Vec<egui::RawInput>) {
    let th = crate::theme::theme();
    for raw in inputs {
        drop(ctx.run(raw, |ctx| {
            let chooser = &mut st.file_chooser;
            let closed = st.popups.draw_view_local(ctx, &mut |cx, ui, owns_escape| {
                cx.id == FILE_PICKER_POPUP_ID && chooser.draw(ui, &th, owns_escape).done
            });
            if closed.contains(&FILE_PICKER_POPUP_ID) {
                st.file_chooser.cancel();
            }
        }));
    }
}

fn header_rect(ctx: &egui::Context, st: &mut SettingsUiState) -> egui::Rect {
    run_frames(ctx, st, vec![screen_input()]);
    crate::adapters::ui::popup::reported_header_drag_rect(ctx, FILE_PICKER_POPUP_ID)
        .expect("설정 창 파일 선택기 뷰가 헤더 줄을 보고해야 한다")
}

/// 셸 타이틀바가 없으니 제목은 한 번만 칠해지고 popup 윗변의 헤더 줄 안에 있다.
#[test]
fn the_settings_chooser_has_no_shell_title_bar() {
    // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 그리기 전에 초기화한다.
    crate::i18n::init("en");
    let mut st = opened();
    let ctx = egui::Context::default();
    let header = header_rect(&ctx, &mut st);
    let p = st
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .expect("등록돼 있다");
    let popup = egui::Rect::from_min_size(p.pos, p.size);
    assert_eq!(header.top(), popup.top(), "헤더가 popup 윗변에서 시작한다");
    let title = file_chooser::chooser_title(false);
    let out = ctx.run(screen_input(), |ctx| {
        let th = crate::theme::theme();
        let chooser = &mut st.file_chooser;
        st.popups.draw(
            ctx,
            &mut |id, ui| {
                if id.id == FILE_PICKER_POPUP_ID {
                    chooser.draw(ui, &th, true);
                }
            },
            None,
            &[],
        );
    });
    let hits: Vec<egui::Rect> = out
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == title => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 1, "제목은 한 번만 칠해진다: {hits:?}");
    assert!(
        header.contains_rect(hits[0]),
        "제목 {hits:?} 이 헤더 {header:?} 밖에 있다"
    );
}

/// 헤더 오른쪽 끝 ✕ 로 닫으면 드래그가 가로채지 않고 결과는 Cancelled 다.
#[test]
fn the_settings_chooser_header_close_button_cancels() {
    let mut st = opened();
    let ctx = egui::Context::default();
    let header = header_rect(&ctx, &mut st);
    let close = egui::pos2(header.right() - header.height() / 2.0, header.center().y);
    run_frames(
        &ctx,
        &mut st,
        vec![
            screen_input(),
            pointer_input(close, None),
            pointer_input(close, Some(true)),
            pointer_input(close, Some(false)),
        ],
    );
    assert!(!st.popups.is_open(FILE_PICKER_POPUP_ID));
    assert_eq!(
        st.file_chooser.take_outcome(CONSUMER),
        Some(file_chooser::FileChooserOutcome::Cancelled)
    );
}

/// 헤더의 빈 곳을 끌면 popup 이 이동하고 닫히지 않는다.
#[test]
fn the_settings_chooser_header_drag_moves_the_popup() {
    let mut st = opened();
    let ctx = egui::Context::default();
    let header = header_rect(&ctx, &mut st);
    let before = st
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .expect("등록돼 있다")
        .pos;
    let grab = header.center();
    let delta = egui::vec2(-60.0, -40.0);
    run_frames(
        &ctx,
        &mut st,
        vec![
            screen_input(),
            pointer_input(grab, Some(true)),
            pointer_input(grab + delta * 0.5, None),
            pointer_input(grab + delta, None),
            pointer_input(grab + delta, Some(false)),
        ],
    );
    assert!(st.popups.is_open(FILE_PICKER_POPUP_ID));
    let after = st
        .popups
        .get_mut(FILE_PICKER_POPUP_ID)
        .expect("등록돼 있다")
        .pos;
    assert!(
        (after - before - delta).length() < 1.0,
        "헤더를 {delta:?} 끌었는데 popup 위치가 {before:?} → {after:?}"
    );
    assert_eq!(st.file_chooser.take_outcome(CONSUMER), None);
}

#[test]
fn remote_transfer_folder_size_and_cancel_preserve_the_draft() {
    let mut st = SettingsUiState::new();
    st.draft = Some(Settings::default());
    st.draft.as_mut().unwrap().remote_transfer.dir = "/original".into();
    st.open_file_chooser_at(
        "remote_transfer.folder",
        file_chooser::FileChooserMode::Folder,
        Vec::new(),
        None,
        Some(std::path::PathBuf::from("/")),
    );
    assert_eq!(
        st.popups.get_mut(FILE_PICKER_POPUP_ID).unwrap().size,
        file_chooser::folder_size()
    );
    st.file_chooser.cancel();
    apply_file_chooser_outcomes(&mut st);
    assert_eq!(st.draft.as_ref().unwrap().remote_transfer.dir, "/original");
}

/// Given 충돌 확인 위에 파일 선택이 열린 설정 창, When 그리면, Then Esc 주인은 포커스 인스턴스인 파일 선택이다.
/// When 파일 선택이 닫히면 Then 포커스 인스턴스가 없어 다음 프레임에는 Esc 주인이 없다.
#[test]
fn the_settings_escape_goes_to_the_focused_of_conflict_and_chooser() {
    let mut st = SettingsUiState::new();
    // intent-exempt: 설정 창 자체 관리자의 시험.
    st.popups.open_centered_focused(
        KEYBINDING_CONFLICT_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
    );
    st.open_file_chooser(
        CONSUMER,
        file_chooser::FileChooserMode::Open,
        Vec::new(),
        None,
    );
    let ctx = egui::Context::default();
    let owners = |st: &mut SettingsUiState| {
        let mut owners = Vec::new();
        drop(ctx.run(screen_input(), |ctx| {
            st.popups.draw_view_local(ctx, &mut |cx, _ui, owns_escape| {
                if owns_escape {
                    owners.push(cx.id);
                }
                false
            });
        }));
        owners
    };
    assert_eq!(owners(&mut st), [FILE_PICKER_POPUP_ID]);
    st.popups.close(FILE_PICKER_POPUP_ID);
    let owners_after = owners(&mut st);
    assert!(owners_after.is_empty(), "{owners_after:?}");
}

// ── 별도 창 키 주인(W1). 시험은 창 `render`처럼 직전 패스 뒤 포커스 기록으로 값을 정해 패스마다 넣는다. ──

use crate::adapters::ui::popup::PopupInstanceId;
use crate::adapters::ui::popup::window_keys::{WindowFocusRecord, focusable_at_text};

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn keys_input(events: Vec<egui::Event>) -> egui::RawInput {
    let mut raw = screen_input();
    raw.events = events;
    raw
}

/// 직전 패스 뒤 포커스 기록으로 정한 이 창의 W1 값.
fn next_owner(ctx: &egui::Context, st: &SettingsUiState) -> Option<PopupInstanceId> {
    st.window_key_owner(ctx, WindowFocusRecord::read(ctx))
}

/// 설정 창 `render`처럼 W1 값을 `run` 전에 정해 패스마다 넣고 `draw`로 그린다.
fn keyed_run(
    ctx: &egui::Context,
    st: &mut SettingsUiState,
    raw: egui::RawInput,
    mut draw: impl FnMut(&egui::Context, &mut SettingsUiState),
) -> egui::FullOutput {
    let owner = next_owner(ctx, st);
    ctx.run(raw, |ctx| {
        st.put_window_key_owner(owner);
        draw(ctx, st);
    })
}

/// 창 위젯(`outside`)을 그린 뒤 설정 창 관리자의 팝업을 그린다. 충돌 팝업 본문은 `inside`가 그린다. 이 패스에
/// 세 번째 인자(키 주인)가 참이었던 팝업 id를 돌려준다.
fn conflict_pass(
    ctx: &egui::Context,
    st: &mut SettingsUiState,
    events: Vec<egui::Event>,
    outside: &mut dyn FnMut(&mut egui::Ui),
    inside: &mut dyn FnMut(&mut egui::Ui),
) -> (Vec<PopupId>, egui::FullOutput) {
    let mut owners = Vec::new();
    let out = keyed_run(ctx, st, keys_input(events), |ctx, st| {
        egui::CentralPanel::default().show(ctx, |ui| outside(ui));
        st.popups.draw_view_local(ctx, &mut |cx, ui, owns_keys| {
            if owns_keys {
                owners.push(cx.id);
            }
            if cx.id == KEYBINDING_CONFLICT_POPUP_ID {
                inside(ui);
            }
            false
        });
    });
    (owners, out)
}

fn conflict_open() -> SettingsUiState {
    let mut st = SettingsUiState::new();
    // intent-exempt: 설정 창 자체 관리자의 시험.
    st.popups.open_centered_focused(
        KEYBINDING_CONFLICT_POPUP_ID,
        crate::model::popup_kind::PopupScope::Window,
    );
    st
}

fn conflict_instance(st: &SettingsUiState) -> PopupInstanceId {
    st.popups
        .focused_dismissal_target()
        .map(|(_, iid, _)| iid)
        .expect("the conflict popup is focused")
}

fn outside_edit(id: egui::Id) -> impl FnMut(&mut egui::Ui) {
    let mut text = String::new();
    move |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).id(id));
    }
}

fn focused(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(|m| m.focused())
}

/// Given 충돌 팝업 + 창 위젯에 포커스, When 같은 패스에 팝업 안을 누르면, Then 그 패스 키는 밖이고 다음 패스부터 팝업이다.
#[test]
fn a_click_into_the_conflict_takes_keys_from_the_next_pass() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let outside_id = egui::Id::new("w1-outside");
    let mut outside = outside_edit(outside_id);
    let mut inside = |ui: &mut egui::Ui| {
        ui.label("inside label");
    };
    // 연 첫 패스는 본문을 칠하지 않는다. 다음 패스 출력에서 글자 자리를 찾는다.
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    let (_, out) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    ctx.memory_mut(|m| m.request_focus(outside_id));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    assert_eq!(next_owner(&ctx, &st), None, "the pass starts outside");
    let label = out
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "inside label" => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center())
            }
            _ => None,
        })
        .expect("the conflict body is painted");
    let click = vec![
        egui::Event::PointerMoved(label),
        egui::Event::PointerButton {
            pos: label,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: label,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    let (owners, _) = conflict_pass(&ctx, &mut st, click, &mut outside, &mut inside);
    assert!(
        owners.is_empty(),
        "the click pass keeps the start owner: {owners:?}"
    );
    let (owners, _) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    assert_eq!(owners, [KEYBINDING_CONFLICT_POPUP_ID]);
}

/// Given 충돌 팝업 + 창 위젯에 포커스, When Tab으로 팝업 안에 들어가면, Then 그 패스 키는 밖이고 다음 패스부터 팝업이다.
#[test]
fn tab_into_the_conflict_takes_keys_from_the_next_pass() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let outside_id = egui::Id::new("w1-outside");
    let mut outside = outside_edit(outside_id);
    let mut inside = |ui: &mut egui::Ui| {
        drop(ui.button("inside"));
    };
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    ctx.memory_mut(|m| m.request_focus(outside_id));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    assert_eq!(next_owner(&ctx, &st), None, "the pass starts outside");
    let tab = vec![key_event(egui::Key::Tab, egui::Modifiers::NONE)];
    let (owners, _) = conflict_pass(&ctx, &mut st, tab, &mut outside, &mut inside);
    assert!(
        owners.is_empty(),
        "the Tab pass keeps the start owner: {owners:?}"
    );
    let layer = WindowFocusRecord::read(&ctx).layer();
    assert_eq!(
        layer.map(|l| l.order),
        Some(egui::Order::Foreground),
        "Tab moved the focus into the popup"
    );
    let (owners, _) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    assert_eq!(owners, [KEYBINDING_CONFLICT_POPUP_ID]);
}

/// Given 키 주인인 충돌 팝업 안 버튼에 포커스, When Tab, Then 포커스가 다음 버튼으로 간다(패스 시작 raw 처리).
#[test]
fn tab_moves_between_conflict_buttons() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let mut outside = |_: &mut egui::Ui| {};
    let mut inside = |ui: &mut egui::Ui| {
        drop(ui.button("first"));
        drop(ui.button("second"));
    };
    // 연 첫 패스는 본문을 칠하지 않는다. 다음 패스 출력에서 글자 자리를 찾는다.
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    let (_, out) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    let first = focusable_at_text(&ctx, &out, "first", egui::Order::Foreground);
    let second = focusable_at_text(&ctx, &out, "second", egui::Order::Foreground);
    ctx.memory_mut(|m| m.request_focus(first));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    let tab = vec![key_event(egui::Key::Tab, egui::Modifiers::NONE)];
    let (owners, _) = conflict_pass(&ctx, &mut st, tab, &mut outside, &mut inside);
    assert_eq!(
        owners,
        [KEYBINDING_CONFLICT_POPUP_ID],
        "keys are the popup's"
    );
    assert_eq!(focused(&ctx), Some(second));
}

/// Given 앞 패스에 포커스를 가진 창 위젯, When 이번 패스에 그 위젯을 그리지 않으면, Then `end_pass`가 포커스를
/// 풀어 기록이 없고 키 주인은 포커스 인스턴스다.
#[test]
fn a_vanished_focus_widget_counts_as_no_record() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let outside_id = egui::Id::new("w1-outside");
    let mut outside = outside_edit(outside_id);
    let mut none = |_: &mut egui::Ui| {};
    let mut inside = |_: &mut egui::Ui| {};
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    ctx.memory_mut(|m| m.request_focus(outside_id));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    assert_eq!(
        next_owner(&ctx, &st),
        None,
        "the drawn widget holds the focus"
    );
    drop(conflict_pass(&ctx, &mut st, vec![], &mut none, &mut inside));
    assert_eq!(focused(&ctx), None);
    assert_eq!(WindowFocusRecord::read(&ctx).layer(), None);
    assert_eq!(next_owner(&ctx, &st), Some(conflict_instance(&st)));
    let (owners, _) = conflict_pass(&ctx, &mut st, vec![], &mut none, &mut inside);
    assert_eq!(owners, [KEYBINDING_CONFLICT_POPUP_ID]);
}

/// Given 충돌 팝업 버튼에 포커스, When 아래 방향키, Then `end_pass`가 팝업 밖 입력칸으로 옮긴 포커스를 기록이 읽어
/// 다음 패스에는 키 주인이 없다.
#[test]
fn focus_moved_by_the_pass_end_is_read_next_pass() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let outside_id = egui::Id::new("w1-below");
    let mut inside = |ui: &mut egui::Ui| {
        drop(ui.button("only"));
    };
    let mut nothing = |_: &mut egui::Ui| {};
    // 연 첫 패스는 본문을 칠하지 않는다. 다음 패스 출력에서 글자 자리를 찾는다.
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut nothing,
        &mut inside,
    ));
    let (_, out) = conflict_pass(&ctx, &mut st, vec![], &mut nothing, &mut inside);
    let popup = {
        let p = st
            .popups
            .get_mut(KEYBINDING_CONFLICT_POPUP_ID)
            .expect("registered");
        egui::Rect::from_min_size(p.pos, p.size)
    };
    let button = focusable_at_text(&ctx, &out, "only", egui::Order::Foreground);
    // 방향 탐색은 ±45° 원뿔 안만 본다. 입력칸을 버튼과 같은 가로 범위로 팝업 바로 아래에 둔다.
    let button_rect = ctx
        .viewport(|v| v.prev_pass.widgets.get(button).map(|w| w.rect))
        .expect("the button was drawn");
    let below = egui::Rect::from_center_size(
        egui::pos2(button_rect.center().x, popup.bottom() + 40.0),
        egui::vec2(button_rect.width(), 24.0),
    );
    let mut text = String::new();
    let mut outside = |ui: &mut egui::Ui| {
        ui.put(below, egui::TextEdit::singleline(&mut text).id(outside_id));
    };
    ctx.memory_mut(|m| m.request_focus(button));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    assert_eq!(next_owner(&ctx, &st), Some(conflict_instance(&st)));
    let down = vec![key_event(egui::Key::ArrowDown, egui::Modifiers::NONE)];
    drop(conflict_pass(
        &ctx,
        &mut st,
        down,
        &mut outside,
        &mut inside,
    ));
    assert_eq!(
        focused(&ctx),
        Some(outside_id),
        "the pass end moved the focus"
    );
    assert!(ctx.viewport(|v| v.prev_pass.widgets.get(outside_id).is_some()));
    let (owners, _) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    assert!(owners.is_empty(), "{owners:?}");
}

/// Given 충돌 팝업 안 첫 위젯에 포커스, When Shift+Tab(안→밖), Then 그 패스 뒤 기록은 아직 안쪽이라 다음 패스는
/// 팝업 것이고 그다음 패스부터 밖이다.
#[test]
fn shift_tab_out_of_the_conflict_is_one_pass_late() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let outside_id = egui::Id::new("w1-outside");
    let mut outside = outside_edit(outside_id);
    let mut inside = |ui: &mut egui::Ui| {
        drop(ui.button("first"));
    };
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    let first_inside = ctx.viewport(|v| {
        let widgets = &v.prev_pass.widgets;
        widgets
            .layer_ids()
            .filter(|l| l.order == egui::Order::Foreground)
            .flat_map(|l| widgets.get_layer(l))
            .find(|w| w.sense.is_focusable())
            .map(|w| w.id)
            .expect("the popup has a focusable widget")
    });
    ctx.memory_mut(|m| m.request_focus(first_inside));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut outside,
        &mut inside,
    ));
    assert_eq!(next_owner(&ctx, &st), Some(conflict_instance(&st)));
    let back = vec![key_event(egui::Key::Tab, egui::Modifiers::SHIFT)];
    drop(conflict_pass(
        &ctx,
        &mut st,
        back,
        &mut outside,
        &mut inside,
    ));
    assert_eq!(
        focused(&ctx),
        Some(first_inside),
        "Previous applies next pass"
    );
    let (owners, _) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    assert_eq!(owners, [KEYBINDING_CONFLICT_POPUP_ID]);
    assert_ne!(focused(&ctx), Some(first_inside));
    let (owners, _) = conflict_pass(&ctx, &mut st, vec![], &mut outside, &mut inside);
    assert!(owners.is_empty(), "{owners:?}");
}

/// Given 패스 N-1에 창 위젯 레이어에 그린 id, When 패스 N에 같은 id를 충돌 팝업 안에 그리고 포커스가 남으면,
/// Then 기록은 패스 N 레이어(팝업)를 본다. `read_response`는 N-1 레이어를 먼저 돌려준다.
#[test]
fn the_record_reads_this_pass_layer_not_the_one_before() {
    let mut st = conflict_open();
    let ctx = egui::Context::default();
    let moving = egui::Id::new("w1-moving");
    let mut draw_here = |ui: &mut egui::Ui| {
        let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(40.0, 20.0));
        drop(ui.interact(rect, moving, egui::Sense::click()));
    };
    let mut nothing = |_: &mut egui::Ui| {};
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut draw_here,
        &mut nothing,
    ));
    ctx.memory_mut(|m| m.request_focus(moving));
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut draw_here,
        &mut nothing,
    ));
    assert_eq!(next_owner(&ctx, &st), None, "drawn in the window layer");
    let mut draw_inside = |ui: &mut egui::Ui| {
        let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(40.0, 20.0));
        drop(ui.interact(rect, moving, egui::Sense::click()));
    };
    drop(conflict_pass(
        &ctx,
        &mut st,
        vec![],
        &mut nothing,
        &mut draw_inside,
    ));
    assert_eq!(focused(&ctx), Some(moving));
    assert_eq!(
        ctx.read_response(moving).map(|r| r.layer_id.order),
        Some(egui::Order::Background),
        "read_response still answers the pass before"
    );
    assert_eq!(next_owner(&ctx, &st), Some(conflict_instance(&st)));
}

// ── 설정 창 전체(`draw_settings_panel`)로 그리는 충돌 질문 장면 ──

struct Panel {
    settings: Settings,
    tap: Option<String>,
    formats: FileFormatRegistry,
    handlers: FileHandlerRegistry,
}

/// `new_tab`에 `new_workspace`의 키를 녹화해 충돌 확인을 기다리는 설정 창.
fn conflicting_panel() -> (Panel, SettingsUiState) {
    crate::i18n::init("en");
    let settings = Settings::default();
    let combo = settings
        .keybindings
        .get_bindings("new_workspace")
        .and_then(|b| b.first())
        .cloned()
        .expect("new_workspace has a default key");
    let (conflicting, conflicting_idx) = settings
        .keybindings
        .find_conflict("new_tab", &combo)
        .expect("the key conflicts");
    let mut st = SettingsUiState::new();
    st.font_families = Some(Vec::new());
    st.pending_binding = Some(PendingBinding {
        target_field: "new_tab".into(),
        target_idx: 0,
        combo,
        conflicting_field: conflicting.into(),
        conflicting_idx,
        bare_target: None,
        bare_raw_key: String::new(),
        conflicting_bare: None,
        conflicting_label: None,
    });
    let panel = Panel {
        settings,
        tap: None,
        formats: FileFormatRegistry::new(std::sync::Arc::new(
            tasty_file_format::FileFormatRegistry::new(),
        )),
        handlers: FileHandlerRegistry::new(std::sync::Arc::new(
            tasty_file_handler::FileHandlerRegistry::new(),
        )),
    };
    (panel, st)
}

fn panel_pass(
    ctx: &egui::Context,
    p: &mut Panel,
    st: &mut SettingsUiState,
    raw: egui::RawInput,
) -> egui::FullOutput {
    keyed_run(ctx, st, raw, |ctx, st| {
        let action = draw_settings_panel(
            ctx,
            SettingsPanelCtx {
                settings: &mut p.settings,
                ui_state: st,
                captured_double_tap: &mut p.tap,
                file_format: &p.formats,
                file_handler: &p.handlers,
            },
        );
        assert_eq!(action, None, "the panel was not saved or cancelled");
    })
}

/// 충돌 확인을 열고 L2 사이드바 필터 입력칸에 포커스를 준다. 그 뒤 패스 시작 키 주인은 없다.
fn conflict_with_outside_focus(
    ctx: &egui::Context,
    p: &mut Panel,
    st: &mut SettingsUiState,
    screen: egui::Vec2,
) {
    let raw = || {
        let mut raw = screen_input();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen));
        raw
    };
    let out = panel_pass(ctx, p, st, raw());
    assert!(st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    let filter = focusable_at_text(
        ctx,
        &out,
        t("settings.filter.sections"),
        egui::Order::Background,
    );
    ctx.memory_mut(|m| m.request_focus(filter));
    drop(panel_pass(ctx, p, st, raw()));
    assert_eq!(focused(ctx), Some(filter));
    assert_eq!(
        WindowFocusRecord::read(ctx).layer().map(|l| l.order),
        Some(egui::Order::Background)
    );
    assert_eq!(
        next_owner(ctx, st),
        None,
        "the pass starts at the filter input"
    );
}

/// Given 충돌 질문 + 패스 시작 포커스가 다른 입력칸, When Esc, Then 팝업은 열린 채이고 입력칸이 Esc를 받는다.
#[test]
fn escape_goes_to_the_input_that_holds_focus_over_a_conflict() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    conflict_with_outside_focus(&ctx, &mut p, &mut st, egui::vec2(2000.0, 1500.0));
    let esc = keys_input(vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)]);
    drop(panel_pass(&ctx, &mut p, &mut st, esc));
    assert_eq!(focused(&ctx), None, "the input took the Esc");
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert!(st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert!(st.pending_binding.is_some());
}

/// Given 충돌 질문(`pending_binding` 있음) + 패스 시작 포커스가 밖 입력칸, When Enter 한 패스와 빈 패스, Then 팝업과
/// 대기 단축키가 남고 draft 단축키는 그대로다.
#[test]
fn conflict_body_gets_no_keys_when_focus_is_outside() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    conflict_with_outside_focus(&ctx, &mut p, &mut st, egui::vec2(2000.0, 1500.0));
    let before = st.draft.as_ref().expect("draft").keybindings.clone();
    let message = conflict_message_text(
        st.pending_binding.as_ref().expect("pending"),
        &p.settings.general,
    );
    let enter = keys_input(vec![key_event(egui::Key::Enter, egui::Modifiers::NONE)]);
    let out = panel_pass(&ctx, &mut p, &mut st, enter);
    assert_eq!(out.platform_output.num_completed_passes, 1);
    assert!(
        out.shapes.iter().any(|c| matches!(
            &c.shape,
            egui::epaint::Shape::Text(t) if t.galley.text() == message
        )),
        "the conflict body was drawn"
    );
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert!(st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert!(st.pending_binding.is_some());
    assert_eq!(st.draft.as_ref().expect("draft").keybindings, before);
}

/// Given 충돌 질문 + 밖 입력칸 포커스, When 단축키 행 줄 쌓기가 바뀌어 같은 입력으로 다시 그리는 패스, Then 반환 뒤
/// 기록은 밖 입력칸이고 다음 패스 뒤 팝업과 대기 단축키가 남는다.
#[test]
fn a_discarded_pass_keeps_the_explicit_no_owner() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    // 이 폭에서는 단축키 탭을 처음 그리는 패스에 행이 쌓기로 바뀐다(측정 560 → 2패스, 640 이상 → 1패스).
    let screen = egui::vec2(560.0, 1500.0);
    conflict_with_outside_focus(&ctx, &mut p, &mut st, screen);
    let raw = || {
        let mut raw = screen_input();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen));
        raw
    };
    // 단축키 탭의 행은 처음 그릴 때 좁은 폭에서 쌓기로 바뀌어 `request_discard`를 부른다.
    st.active_tab = SettingsTab::Keybindings;
    let out = panel_pass(&ctx, &mut p, &mut st, raw());
    assert_eq!(out.platform_output.num_completed_passes, 2);
    assert_eq!(
        WindowFocusRecord::read(&ctx).layer().map(|l| l.order),
        Some(egui::Order::Background),
        "the record after the return is the outside input"
    );
    assert_eq!(next_owner(&ctx, &st), None);
    drop(panel_pass(&ctx, &mut p, &mut st, raw()));
    assert!(st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert!(st.pending_binding.is_some());
}
