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
    assert!(st.popups.is_open(file_chooser::FILE_CHOOSER_POPUP_ID));
    st
}

/// 설정 창 프레임의 파일 선택 부분만 같은 context 로 이어 그린다. 누름·뗌은 여러 프레임에 걸친다.
fn run_frames(ctx: &egui::Context, st: &mut SettingsUiState, inputs: Vec<egui::RawInput>) {
    let th = crate::theme::theme();
    for raw in inputs {
        drop(ctx.run(raw, |ctx| {
            let mut done = false;
            let chooser = &mut st.file_chooser;
            let result = st.popups.draw(
                ctx,
                &mut |id, ui| {
                    if id.id == file_chooser::FILE_CHOOSER_POPUP_ID {
                        done |= chooser.draw(ui, &th, true);
                    }
                },
                None,
                &[],
            );
            settle_file_chooser(st, done, &result.closed);
        }));
    }
}

fn header_rect(ctx: &egui::Context, st: &mut SettingsUiState) -> egui::Rect {
    run_frames(ctx, st, vec![screen_input()]);
    crate::adapters::ui::popup::reported_header_drag_rect(ctx, file_chooser::FILE_CHOOSER_POPUP_ID)
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
        .get_mut(file_chooser::FILE_CHOOSER_POPUP_ID)
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
                if id.id == file_chooser::FILE_CHOOSER_POPUP_ID {
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
    assert!(!st.popups.is_open(file_chooser::FILE_CHOOSER_POPUP_ID));
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
        .get_mut(file_chooser::FILE_CHOOSER_POPUP_ID)
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
    assert!(st.popups.is_open(file_chooser::FILE_CHOOSER_POPUP_ID));
    let after = st
        .popups
        .get_mut(file_chooser::FILE_CHOOSER_POPUP_ID)
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
        st.popups
            .get_mut(file_chooser::FILE_CHOOSER_POPUP_ID)
            .unwrap()
            .size,
        file_chooser::folder_size()
    );
    st.file_chooser.cancel();
    apply_file_chooser_outcomes(&mut st);
    assert_eq!(st.draft.as_ref().unwrap().remote_transfer.dir, "/original");
}
