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
            let closed = st.popups.draw_view_local(ctx, &mut |cx, ui, _| {
                if cx.id != FILE_PICKER_POPUP_ID {
                    return false;
                }
                let escape = !crate::adapters::ui::popup::child_popup_open(ui.ctx(), cx.instance)
                    && cx
                        .keys
                        .consume(crate::adapters::ui::popup::keys::BasicKey::Escape);
                chooser.draw(ui, &th, &mut cx.keys, escape).done
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
                    chooser.draw(ui, &th, &mut id.keys, false);
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
use crate::adapters::ui::popup::escape_step::WindowPassKeys;
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
    st.popups
        .window_key_owner(ctx, WindowFocusRecord::read(ctx))
}

/// 설정 창 `render`처럼 W1 값과 이번 입력의 Esc 표지(모두 수식키 없음)를 `run` 전에 정하고, 패스마다 `draw`에
/// 같은 값을 넘긴다. 관리자에 넣는 일은 `draw`가 한다(제품의 관리자 그리기 함수와 같다).
fn keyed_run(
    ctx: &egui::Context,
    st: &mut SettingsUiState,
    raw: egui::RawInput,
    mut draw: impl FnMut(&egui::Context, &mut SettingsUiState, &WindowPassKeys),
) -> egui::FullOutput {
    let keys = WindowPassKeys {
        owner: next_owner(ctx, st),
        marks: crate::adapters::ui::popup::body_keys::escape_marks_in(
            &raw.events,
            0,
            tasty_key_match::KeyMods::default(),
        ),
    };
    ctx.run(raw, |ctx| draw(ctx, st, &keys))
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
    let out = keyed_run(ctx, st, keys_input(events), |ctx, st, keys| {
        st.popups.put_window_pass_keys(ctx, keys);
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
    keyed_run(ctx, st, raw, |ctx, st, keys| {
        let action = draw_settings_panel(
            ctx,
            SettingsPanelCtx {
                settings: &mut p.settings,
                ui_state: st,
                captured_double_tap: &mut p.tap,
                file_format: &p.formats,
                file_handler: &p.handlers,
                pass_keys: keys,
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

// ── 충돌 질문·설정 창 선택기의 Esc(중앙 단계). 패스마다 `draw_settings_panel`을 부르고, 표지는 창 `render`처럼
// 입력 묶음마다 정한다. ──

/// `panel_pass`와 같고 Esc 표지를 직접 준다. 그리기마다(다시 그리는 패스 포함) 같은 값을 넘긴다. 패스마다 그 패스
/// `raw.events` 수를 돌려준다.
#[cfg(not(target_os = "macos"))]
fn marked_pass(
    ctx: &egui::Context,
    p: &mut Panel,
    st: &mut SettingsUiState,
    raw: egui::RawInput,
    marks: Vec<tasty_key_match::KeyMods>,
) -> (egui::FullOutput, Vec<usize>) {
    let keys = WindowPassKeys {
        owner: next_owner(ctx, st),
        marks,
    };
    let mut raws = Vec::new();
    let out = ctx.run(raw, |ctx| {
        raws.push(ctx.input(|i| i.raw.events.len()));
        let action = draw_settings_panel(
            ctx,
            SettingsPanelCtx {
                settings: &mut p.settings,
                ui_state: st,
                captured_double_tap: &mut p.tap,
                file_format: &p.formats,
                file_handler: &p.handlers,
                pass_keys: &keys,
            },
        );
        assert_eq!(action, None, "the panel was not saved or cancelled");
    });
    (out, raws)
}

fn escape_key() -> egui::Event {
    key_event(egui::Key::Escape, egui::Modifiers::NONE)
}

/// 충돌 질문을 열고 두 패스 그려(처음 나타난 영역은 크기만 잰다) 키 주인이 충돌 질문임을 단정한다. 녹화한 단축키와
/// 마지막 출력을 돌려준다.
fn conflict_shown(
    ctx: &egui::Context,
    p: &mut Panel,
    st: &mut SettingsUiState,
) -> (String, egui::FullOutput) {
    let combo = st.pending_binding.as_ref().expect("pending").combo.clone();
    drop(panel_pass(ctx, p, st, screen_input()));
    let out = panel_pass(ctx, p, st, screen_input());
    assert!(st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert_eq!(
        next_owner(ctx, st),
        Some(conflict_instance(st)),
        "W1 키 주인이 충돌 질문이다"
    );
    (combo, out)
}

/// 녹화한 단축키가 대상 칸(`new_tab` 첫 칸)에 들어갔는지.
fn applied(st: &SettingsUiState, combo: &String) -> bool {
    st.draft
        .as_ref()
        .and_then(|d| d.keybindings.get_bindings("new_tab"))
        .and_then(|b| b.first())
        == Some(combo)
}

fn text_rect_in(out: &egui::FullOutput, wanted: &str) -> egui::Rect {
    fn walk(shape: &egui::epaint::Shape, wanted: &str, found: &mut Option<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == wanted => {
                *found = Some(egui::Rect::from_min_size(t.pos, t.galley.size()));
            }
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, wanted, found)),
            _ => {}
        }
    }
    let mut found = None;
    for c in &out.shapes {
        walk(&c.shape, wanted, &mut found);
    }
    found.unwrap_or_else(|| panic!("{wanted} was painted"))
}

fn button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// T-4a3-7: Given 키 주인 충돌 질문, When Esc 가 오면, Then 그 패스에 닫히고 대기 단축키가 비었다.
#[test]
fn conflict_escape_clears_the_pending_binding_in_the_same_pass() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    conflict_shown(&ctx, &mut p, &mut st);
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![escape_key()]),
    ));
    assert!(
        !st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
        "그 패스에 닫힌다"
    );
    assert!(st.pending_binding.is_none(), "대기 단축키가 빈다");
}

/// T-4a3-K3c: Given 키 주인 충돌 질문, When 표지 [bare]인 Esc 패스를 Super 가 눌린 채 그리면, Then 그 패스에 닫히고
/// 대기 단축키가 비었다(중앙 사건은 표지 수식키로 정한다).
#[cfg(not(target_os = "macos"))]
#[test]
fn a_bare_escape_closes_the_conflict_even_if_super_is_down_at_render() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    conflict_shown(&ctx, &mut p, &mut st);
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::SUPER);
    assert!(
        tasty_key_match::super_held(&ctx),
        "render 시점에 Super 가 눌려 있다"
    );
    drop(marked_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![escape_key()]),
        vec![tasty_key_match::KeyMods::default()],
    ));
    assert!(!st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert!(st.pending_binding.is_none());
}

/// T-4a3-K3d: Given 키 주인 충돌 질문, When 표지 [Super]인 Esc 를 Super 를 뗀 뒤 그리면(그리기 시점 값으로는 bare 로
/// 보인다), Then 첫 패스 뒤 거부 표지가 서지 않고 열린 채이며, 다음 프레임 뒤에도 열린 채 대기 단축키가 남는다.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_super_escape_does_not_close_the_conflict_after_super_is_released() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    conflict_shown(&ctx, &mut p, &mut st);
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::empty());
    assert!(
        !tasty_key_match::super_held(&ctx),
        "render 시점에 Super 가 놓였다"
    );
    let super_mark = tasty_key_match::KeyMods {
        super_: true,
        ..Default::default()
    };
    drop(marked_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![escape_key()]),
        vec![super_mark],
    ));
    assert!(!st.conflict_cancelled, "거부 표지가 서지 않는다");
    assert!(st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    drop(marked_pass(
        &ctx,
        &mut p,
        &mut st,
        screen_input(),
        Vec::new(),
    ));
    assert!(
        st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
        "다음 프레임에도 열린 채다"
    );
    assert!(st.pending_binding.is_some());
}

/// T-4a3-K3e: Given 키 주인 충돌 질문, When 단축키 행 줄 쌓기가 바뀌어 같은 입력을 다시 그리는 프레임에 표지 [bare]인
/// Esc 가 오면, Then 첫 패스 뒤 닫히고 대기 단축키가 비며, 다시 그린 패스는 다시 닫거나 적용하지 않는다.
#[test]
fn a_discarded_pass_does_not_close_or_apply_again() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    let screen = egui::vec2(560.0, 1500.0);
    let raw = |events: Vec<egui::Event>| {
        let mut raw = screen_input();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen));
        raw.events = events;
        raw
    };
    let combo = st.pending_binding.as_ref().expect("pending").combo.clone();
    drop(panel_pass(&ctx, &mut p, &mut st, raw(Vec::new())));
    drop(panel_pass(&ctx, &mut p, &mut st, raw(Vec::new())));
    assert_eq!(next_owner(&ctx, &st), Some(conflict_instance(&st)));
    let before = st.draft.as_ref().expect("draft").keybindings.clone();
    st.active_tab = SettingsTab::Keybindings;
    let mut closed_after_first = None;
    let keys = WindowPassKeys {
        owner: next_owner(&ctx, &st),
        marks: vec![tasty_key_match::KeyMods::default()],
    };
    let mut raws = Vec::new();
    let out = ctx.run(raw(vec![escape_key()]), |ctx| {
        raws.push(ctx.input(|i| i.raw.events.len()));
        if raws.len() == 2 {
            closed_after_first = Some((
                st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
                st.pending_binding.is_some(),
            ));
        }
        let action = draw_settings_panel(
            ctx,
            SettingsPanelCtx {
                settings: &mut p.settings,
                ui_state: &mut st,
                captured_double_tap: &mut p.tap,
                file_format: &p.formats,
                file_handler: &p.handlers,
                pass_keys: &keys,
            },
        );
        assert_eq!(action, None);
    });
    assert_eq!(out.platform_output.num_completed_passes, 2);
    assert_eq!(raws, vec![1, 0], "둘째 패스 raw 입력은 비었다");
    assert_eq!(
        closed_after_first,
        Some((false, false)),
        "첫 패스 뒤 닫히고 대기가 빈다"
    );
    assert!(!st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert!(st.pending_binding.is_none());
    assert_eq!(
        st.draft.as_ref().expect("draft").keybindings,
        before,
        "적용하지 않았다"
    );
    assert!(!applied(&st, &combo));
}

/// T-4a3-K3h: Given 키 주인 충돌 질문, When 적용 버튼 누름 다음 패스에 [적용 버튼 위 뗌, bare Esc] 를 Super 가 눌린 채
/// 그리면, Then 본문이 적용 표지를 세우고 Esc 를 소비해 그 패스에 닫히지 않으며, 다음 패스에 바인딩이 적용된다.
#[cfg(not(target_os = "macos"))]
#[test]
fn an_apply_click_and_a_bare_escape_apply_even_if_super_is_down_at_render() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    let (combo, out) = conflict_shown(&ctx, &mut p, &mut st);
    let apply = text_rect_in(&out, t("settings.keybindings.conflict_apply")).center();
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![egui::Event::PointerMoved(apply)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(apply, true)]),
    ));
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::SUPER);
    assert!(
        tasty_key_match::super_held(&ctx),
        "뗌 패스 render 시점에 Super 가 눌려 있다"
    );
    drop(marked_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(apply, false), escape_key()]),
        vec![tasty_key_match::KeyMods::default()],
    ));
    assert!(st.conflict_accepted, "적용 표지가 섰다");
    assert!(
        st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
        "그 패스에 닫히지 않는다"
    );
    assert!(st.pending_binding.is_some());
    tasty_key_match::note_modifiers(&ctx, winit::keyboard::ModifiersState::empty());
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert!(applied(&st, &combo), "다음 패스에 적용된다");
}

/// T-4a3-21y2: Given 키 주인 충돌 질문, When 한 패스 [적용 버튼 누름·뗌, bare Esc] 가 오면, Then 다음 패스에 바인딩이
/// 적용되고 충돌 질문이 닫히며 대기 단축키는 적용 갈래가 비웠다.
#[test]
fn the_apply_button_wins_over_escape_in_the_same_pass() {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    let (combo, out) = conflict_shown(&ctx, &mut p, &mut st);
    let apply = text_rect_in(&out, t("settings.keybindings.conflict_apply")).center();
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![egui::Event::PointerMoved(apply)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![
            button_event(apply, true),
            button_event(apply, false),
            escape_key(),
        ]),
    ));
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert!(applied(&st, &combo), "바인딩이 적용된다");
    assert!(!st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID));
    assert!(st.pending_binding.is_none());
}

/// T-4a3-21y3: Given 키 주인 충돌 질문, When 한 패스에 Y·Enter 와 bare Esc 가 함께 오면(순서 무관), Then 다음 패스에
/// 바인딩이 적용되고 닫히며 Esc 패스에 대기 단축키가 비지 않는다. 연속 [Y], [Y] 는 한 번만 적용한다. 제목줄 닫기
/// 버튼 누름과 Y 가 한 패스에 오면 닫히고 대기와 적용 표지가 모두 비며, 남은 표지가 새 대기를 적용하지 않는다.
#[test]
fn a_yes_key_and_escape_in_one_pass_apply_the_binding() {
    let yes = || key_event(egui::Key::Y, egui::Modifiers::NONE);
    let enter = || key_event(egui::Key::Enter, egui::Modifiers::NONE);
    for (name, events) in [
        ("Y, Esc", vec![yes(), escape_key()]),
        ("Esc, Y", vec![escape_key(), yes()]),
        ("Enter, Esc", vec![enter(), escape_key()]),
    ] {
        let (mut p, mut st) = conflicting_panel();
        let ctx = egui::Context::default();
        let (combo, _) = conflict_shown(&ctx, &mut p, &mut st);
        drop(panel_pass(&ctx, &mut p, &mut st, keys_input(events)));
        assert!(
            st.pending_binding.is_some(),
            "{name}: Esc 패스에 대기가 남는다"
        );
        drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
        assert!(applied(&st, &combo), "{name}: 다음 패스에 적용된다");
        assert!(
            !st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
            "{name}: 닫힌다"
        );
    }

    // 연속 [Y], [Y]: 둘째 패스가 첫 표지를 적용하고 충돌 질문을 닫아 둘째 Y 는 읽히지 않는다.
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    let (combo, _) = conflict_shown(&ctx, &mut p, &mut st);
    drop(panel_pass(&ctx, &mut p, &mut st, keys_input(vec![yes()])));
    drop(panel_pass(&ctx, &mut p, &mut st, keys_input(vec![yes()])));
    assert!(!st.conflict_accepted, "둘째 패스 뒤 적용 표지가 없다");
    assert!(applied(&st, &combo));
    let once = st.draft.as_ref().expect("draft").keybindings.clone();
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert_eq!(
        st.draft.as_ref().expect("draft").keybindings,
        once,
        "한 번만 바뀐다"
    );

    // [제목줄 닫기 누름, Y]: 닫기가 대기와 표지를 지운다. 새 대기는 남은 표지로 적용되지 않는다.
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    let (combo, _) = conflict_shown(&ctx, &mut p, &mut st);
    let pending = st.pending_binding.clone();
    let close = st
        .popups
        .instance(conflict_instance(&st))
        .expect("conflict")
        .close_btn_rect()
        .center();
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![egui::Event::PointerMoved(close)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(close, true), yes()]),
    ));
    assert!(
        !st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
        "닫기 누름에 닫힌다"
    );
    assert!(st.pending_binding.is_none());
    assert!(!st.conflict_accepted, "적용 표지가 남지 않는다");
    let before = st.draft.as_ref().expect("draft").keybindings.clone();
    st.pending_binding = pending;
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert!(
        st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
        "다시 연 질문은 열린 채다"
    );
    assert_eq!(st.draft.as_ref().expect("draft").keybindings, before);
    assert!(!applied(&st, &combo), "새 대기는 적용되지 않는다");
}

/// 대기 단축키가 없는 설정 창.
fn plain_panel() -> (Panel, SettingsUiState) {
    let (p, mut st) = conflicting_panel();
    st.pending_binding = None;
    (p, st)
}

fn chooser_instance(st: &SettingsUiState) -> PopupInstanceId {
    st.popups
        .instances_of(FILE_PICKER_POPUP_ID)
        .next()
        .expect("chooser")
        .instance
}

/// 설정 창 선택기(폴더 모드)를 `start` 에서 열고 두 패스 그려 W1 키 주인임을 단정한다. 마지막 출력을 돌려준다.
fn chooser_shown(
    ctx: &egui::Context,
    p: &mut Panel,
    st: &mut SettingsUiState,
    start: Option<std::path::PathBuf>,
) -> egui::FullOutput {
    st.open_file_chooser_at(
        CONSUMER,
        file_chooser::FileChooserMode::Folder,
        Vec::new(),
        None,
        start,
    );
    drop(panel_pass(ctx, p, st, screen_input()));
    let out = panel_pass(ctx, p, st, screen_input());
    assert_eq!(
        next_owner(ctx, st),
        Some(chooser_instance(st)),
        "W1 키 주인이 선택기다"
    );
    out
}

/// T-4a3-21w: Given 설정 창 선택기의 숨은 경로 메뉴가 열려 있을 때, When Esc 가 오면, Then 메뉴만 닫히고 선택기는 열린
/// 채이며 취소 결과가 없다.
#[test]
fn escape_closes_only_the_hidden_crumbs_menu() {
    let root = tempfile::tempdir().expect("임시 폴더");
    let mut deep = root.path().to_path_buf();
    for i in 0..40 {
        deep.push(format!("d{i:02}"));
    }
    std::fs::create_dir_all(&deep).expect("깊은 폴더");
    let (mut p, mut st) = plain_panel();
    let ctx = egui::Context::default();
    let out = chooser_shown(&ctx, &mut p, &mut st, Some(deep));
    let crumbs = text_rect_in(&out, "…").center();
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![egui::Event::PointerMoved(crumbs)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(crumbs, true)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(crumbs, false)]),
    ));
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    assert!(ctx.memory(|m| m.any_popup_open()), "경로 메뉴가 열렸다");
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![escape_key()]),
    ));
    assert!(!ctx.memory(|m| m.any_popup_open()), "메뉴가 닫힌다");
    assert!(
        st.popups.is_open(FILE_PICKER_POPUP_ID),
        "선택기는 열린 채다"
    );
    assert_eq!(
        st.file_chooser.take_outcome(CONSUMER),
        None,
        "취소하지 않았다"
    );
}

/// T-4a3-21x: Given 설정 창 선택기가 W1 키 주인이고 선택기 자식이 아닌 egui 팝업 id 가 열려 있을 때(그 패스에 그리지
/// 않는다), When Esc 가 오면, Then 그 id 는 열린 채이고 선택기는 자기 자식만 세어 닫히며 취소 결과를 남긴다.
#[test]
fn escape_with_another_window_dropdown_open() {
    let (mut p, mut st) = plain_panel();
    let ctx = egui::Context::default();
    chooser_shown(&ctx, &mut p, &mut st, None);
    let other = egui::Id::new("settings_other_dropdown_fixture");
    ctx.memory_mut(|m| m.open_popup(other));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![escape_key()]),
    ));
    assert!(
        ctx.memory(|m| m.is_popup_open(other)),
        "픽스처 id 는 열린 채다"
    );
    assert!(!st.popups.is_open(FILE_PICKER_POPUP_ID), "선택기가 닫힌다");
    assert_eq!(
        st.file_chooser.take_outcome(CONSUMER),
        Some(file_chooser::FileChooserOutcome::Cancelled)
    );
}

/// T-4a3-21y1: Given 설정 창 선택기, When 확정 버튼 누름 다음 패스에 [확정 버튼 뗌, bare Esc] 가 오면, Then 결과는
/// 취소다(보기의 Esc 갈래가 확정보다 먼저다).
#[test]
fn escape_and_a_confirm_click_in_one_pass_cancel_the_chooser() {
    let (mut p, mut st) = plain_panel();
    let ctx = egui::Context::default();
    let out = chooser_shown(&ctx, &mut p, &mut st, None);
    let confirm = text_rect_in(&out, t("picker.folder.confirm")).center();
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![egui::Event::PointerMoved(confirm)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(confirm, true)]),
    ));
    drop(panel_pass(
        &ctx,
        &mut p,
        &mut st,
        keys_input(vec![button_event(confirm, false), escape_key()]),
    ));
    assert_eq!(
        st.file_chooser.take_outcome(CONSUMER),
        Some(file_chooser::FileChooserOutcome::Cancelled)
    );
}

/// 키 주인 충돌 질문에 `held` 를 붙인 `key` 한 패스와 빈 패스를 넣는다. (적용됐는지, 열린 채인지, 대기가 남았는지).
fn conflict_after_key(
    key: egui::Key,
    held: Option<crate::adapters::ui::popup::window_input_test_support::Held>,
) -> (bool, bool, bool) {
    let (mut p, mut st) = conflicting_panel();
    let ctx = egui::Context::default();
    let (combo, _) = conflict_shown(&ctx, &mut p, &mut st);
    let press = crate::adapters::ui::popup::window_input_test_support::held_press(&ctx, key, held);
    drop(panel_pass(&ctx, &mut p, &mut st, keys_input(vec![press])));
    drop(panel_pass(&ctx, &mut p, &mut st, screen_input()));
    (
        applied(&st, &combo),
        st.popups.is_open(KEYBINDING_CONFLICT_POPUP_ID),
        st.pending_binding.is_some(),
    )
}

/// T-9a: Given 키 주인 충돌 질문, When 수식키 없는 Enter·Y 를 누르면, Then 적용하고 닫는다. 수식키 없는 N 은 적용하지
/// 않고 닫는다.
#[test]
fn bare_enter_y_and_n_answer_the_conflict() {
    for key in [egui::Key::Enter, egui::Key::Y] {
        assert_eq!(
            conflict_after_key(key, None),
            (true, false, false),
            "{key:?}"
        );
    }
    assert_eq!(
        conflict_after_key(egui::Key::N, None),
        (false, false, false)
    );
}

/// T-9a: Given 키 주인 충돌 질문, When Shift·Ctrl·Alt·Cmd(비-macOS Super)+Enter·Y·N 을 누르면, Then 질문과 대기가
/// 그대로다.
#[test]
fn modified_enter_y_and_n_are_ignored_by_the_conflict() {
    use crate::adapters::ui::popup::window_input_test_support::Held;
    for held in Held::ALL {
        for key in [egui::Key::Enter, egui::Key::Y, egui::Key::N] {
            assert_eq!(
                conflict_after_key(key, Some(held)),
                (false, true, true),
                "{held:?} {key:?}"
            );
        }
    }
}

/// 창 위젯(`outside`)을 그린 뒤 설정 창 관리자의 chooser 를 그린다. 이 패스에 chooser 가 키 주인이었는지와 출력을
/// 돌려준다.
fn chooser_pass(
    ctx: &egui::Context,
    st: &mut SettingsUiState,
    events: Vec<egui::Event>,
    outside: &mut dyn FnMut(&mut egui::Ui),
) -> (bool, egui::FullOutput) {
    let th = crate::theme::theme();
    let mut owned = false;
    let out = keyed_run(ctx, st, keys_input(events), |ctx, st, keys| {
        st.popups.put_window_pass_keys(ctx, keys);
        egui::CentralPanel::default().show(ctx, |ui| outside(ui));
        let chooser = &mut st.file_chooser;
        let closed = st.popups.draw_view_local(ctx, &mut |cx, ui, owns_keys| {
            if cx.id != FILE_PICKER_POPUP_ID {
                return false;
            }
            owned |= owns_keys;
            let escape = !crate::adapters::ui::popup::child_popup_open(ui.ctx(), cx.instance)
                && cx
                    .keys
                    .consume(crate::adapters::ui::popup::keys::BasicKey::Escape);
            chooser.draw(ui, &th, &mut cx.keys, escape).done
        });
        assert!(closed.is_empty(), "the chooser stays open: {closed:?}");
    });
    (owned, out)
}

/// T-F3-1: Given 설정 창 저장 모드 chooser 와 egui 포커스를 가진 창 위젯 입력칸(그 패스 W1 키 주인 없음), When 한
/// 패스에 chooser 의 이름 입력칸을 누르면, Then 그 패스 chooser 는 키 주인이 아니고, 누른 위젯이 이름 입력칸이며,
/// 패스 끝 egui 포커스가 그 입력칸에 남는다(이번 패스 누름 예외). 뗀 다음 패스부터 chooser 가 키 주인이다. 별도 창은
/// 패스 전에 키 주인을 정하므로 누름 패스의 chooser 는 아직 주인이 아니다. 예외를 끄면(M-10b-3) 포커스가 풀린다.
#[test]
fn a_press_on_the_save_name_keeps_its_focus_in_the_non_owner_pass() {
    use crate::adapters::ui::popup::window_input_test_support::click_widget_at;
    let mut st = SettingsUiState::new();
    st.open_file_chooser(
        CONSUMER,
        file_chooser::FileChooserMode::Save {
            default_name: "kept-name.txt".into(),
        },
        Vec::new(),
        None,
    );
    assert!(st.popups.is_open(FILE_PICKER_POPUP_ID));
    let ctx = egui::Context::default();
    let outside_id = egui::Id::new("f3-outside");
    let mut outside = outside_edit(outside_id);
    drop(chooser_pass(&ctx, &mut st, vec![], &mut outside));
    let (_, out) = chooser_pass(&ctx, &mut st, vec![], &mut outside);
    let name = text_rect_in(&out, "kept-name.txt").center();
    let name_id = click_widget_at(&ctx, name).expect("이름 입력칸");
    ctx.memory_mut(|m| m.request_focus(outside_id));
    drop(chooser_pass(&ctx, &mut st, vec![], &mut outside));
    assert_eq!(focused(&ctx), Some(outside_id), "창 위젯이 포커스를 가진다");
    assert_eq!(
        next_owner(&ctx, &st),
        None,
        "누르는 패스는 키 주인 없이 시작한다"
    );

    let (owned, _) = chooser_pass(
        &ctx,
        &mut st,
        vec![egui::Event::PointerMoved(name), button_event(name, true)],
        &mut outside,
    );
    assert!(!owned, "누른 패스에 chooser 는 키 주인이 아니다");
    assert_eq!(
        focused(&ctx),
        Some(name_id),
        "누른 이름 입력칸이 패스 끝 포커스를 가진다"
    );
    assert!(
        egui::TextEdit::load_state(&ctx, name_id).is_some(),
        "포커스 위젯은 텍스트 입력칸이다"
    );
    drop(chooser_pass(
        &ctx,
        &mut st,
        vec![button_event(name, false)],
        &mut outside,
    ));
    let (owned, _) = chooser_pass(&ctx, &mut st, vec![], &mut outside);
    assert!(owned, "다음 패스부터 chooser 가 키 주인이다");
    assert_eq!(
        focused(&ctx),
        Some(name_id),
        "이름 입력칸이 포커스를 지킨다"
    );
}
