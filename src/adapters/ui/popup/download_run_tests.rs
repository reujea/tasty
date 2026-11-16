use super::*;

#[test]
fn sizes_round_up_by_a_tenth_and_switch_to_gib_at_1024_mib() {
    // 1 MiB 를 1 바이트 넘는 파일은 1.0 MiB 로 보이지 않는다.
    assert_eq!(format_size_up(1_048_577), "1.1 MiB");
    assert_eq!(format_size_up(3 * 1_048_576 + 1), "3.1 MiB");
    assert_eq!(format_size_up(3_355_443), "3.2 MiB");
    // MiB 로 올림한 값이 1024.0 이면 GiB 로 다시 올림한다.
    assert_eq!(format_size_up(1_073_741_800), "1.0 GiB");
    assert_eq!(format_size_up(1_073_741_825), "1.1 GiB");
    assert_eq!(format_size_up(1_288_490_188), "1.2 GiB");
    assert_eq!(format_size_up(1_288_490_189), "1.3 GiB");
}

fn every_failure() -> Vec<Failure> {
    let mut all = vec![
        Failure::Unsupported,
        Failure::Disconnected,
        Failure::TimedOut,
        Failure::Protocol,
        Failure::LocalWrite,
        Failure::Cancelled,
    ];
    all.extend(
        [
            DownloadRefusal::Directory,
            DownloadRefusal::NotRegular,
            DownloadRefusal::NotFound,
            DownloadRefusal::PermissionDenied,
            DownloadRefusal::Changed,
            DownloadRefusal::NotAttached,
            DownloadRefusal::Io,
        ]
        .map(Failure::Refused),
    );
    all
}

#[test]
fn every_failure_but_a_stop_has_its_own_sentence() {
    let mut seen = Vec::new();
    for failure in every_failure() {
        let key = failure_key(&failure);
        if failure == Failure::Cancelled {
            assert_eq!(key, None, "a stop is not reported");
            continue;
        }
        let key = key.unwrap_or_else(|| panic!("{failure:?} has no sentence"));
        assert_ne!(t(key), key, "{key} is missing from the language files");
        assert!(!seen.contains(&key), "{failure:?} shares {key}");
        seen.push(key);
    }
    assert_eq!(seen.len(), every_failure().len() - 1);
}

/// 실패 종류가 늘면 여기서 컴파일이 멈춰 위 목록과 문장을 함께 고치게 한다.
#[expect(
    dead_code,
    reason = "exists only to make a new failure kind fail to compile here"
)]
fn every_failure_is_listed(failure: &Failure) {
    match failure {
        Failure::Unsupported
        | Failure::Disconnected
        | Failure::TimedOut
        | Failure::Protocol
        | Failure::LocalWrite
        | Failure::Cancelled
        | Failure::Refused(
            DownloadRefusal::Directory
            | DownloadRefusal::NotRegular
            | DownloadRefusal::NotFound
            | DownloadRefusal::PermissionDenied
            | DownloadRefusal::Changed
            | DownloadRefusal::NotAttached
            | DownloadRefusal::Io,
        ) => {}
    }
}

#[test]
fn the_registration_placeholder_is_the_first_frame_size_of_the_sizer() {
    // 배율 1에서 transfer-popup-width 400, 첫 프레임 높이는 그 절반이다.
    let th = theme::Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    assert_eq!(measured(None, &th), egui::vec2(400.0, 200.0));
    // 배율이 바뀌면 placeholder 도 sizer 와 같은 폭을 따른다.
    let zoomed =
        theme::Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.5);
    assert_eq!(
        measured(None, &zoomed).x,
        zoomed.transfer_popup_width().value()
    );
}

fn opened_scope(state: &mut MainViewState, id: &str) -> crate::model::popup_kind::PopupScope {
    let intents = state.take_pending_intents();
    match &intents[..] {
        [
            crate::intent::DispatchedIntent {
                body:
                    crate::intent::Intent::Ui(
                        crate::intent::UiIntent::OpenPopup {
                            id: opened,
                            mode: crate::intent::OpenPopupMode::CenteredFocused(scope),
                        }
                        | crate::intent::UiIntent::ReopenPopup {
                            id: opened,
                            mode: crate::intent::OpenPopupMode::CenteredFocused(scope),
                        },
                    ),
                ..
            },
        ] if *opened == id => scope.clone(),
        other => panic!("expected one {id} open, got {other:?}"),
    }
}

/// 묻기 대화상자는 원격 파일을 연 surface 범위로 열고, 그 surface 를 상태에 둔다.
#[test]
fn the_ask_opens_in_the_surface_that_opened_the_remote_file() {
    let (mut state, _session) = crate::state::tests::two_pane_fixture();
    ask(
        &mut state,
        1,
        101,
        "/remote/a.bin".into(),
        String::new(),
        crate::file::dispatch::FileDispatchOrigin::User,
    );
    assert_eq!(
        opened_scope(&mut state, DOWNLOAD_RUN_POPUP_ID),
        crate::model::popup_kind::PopupScope::Surface(101)
    );
    assert_eq!(
        state.dialogs.download_run.ask.as_ref().map(|a| a.surface),
        Some(101)
    );
}

/// 실패 팝업은 다운로드를 시작한 surface 범위로 연다.
#[test]
fn a_failure_opens_in_the_surface_that_started_the_download() {
    let (mut state, _session) = crate::state::tests::two_pane_fixture();
    report_failure(&mut state, 101, "a.bin".into(), &Failure::TimedOut);
    assert_eq!(
        opened_scope(&mut state, super::super::transfer::TRANSFER_ERROR_POPUP_ID),
        crate::model::popup_kind::PopupScope::Surface(101)
    );
    assert_eq!(
        state
            .dialogs
            .transfer_error
            .front()
            .map(|e| e.scope.clone()),
        Some(crate::model::popup_kind::PopupScope::Surface(101))
    );
}

/// 크기 확인을 닫으면 다음 확인을 그 다운로드의 surface 범위로 연다.
#[test]
fn the_next_size_confirm_opens_in_its_own_surface() {
    let (mut state, session) = crate::state::tests::two_pane_fixture();
    for (id, surface) in [(1, 201), (2, 101)] {
        state
            .dialogs
            .download_run
            .large
            .push_back(DownloadRunLarge {
                id,
                surface,
                name: format!("{id}.bin"),
                size: 2 << 20,
                confirmed: true,
                content_height: None,
            });
    }
    on_close_download_run_large(&egui::Context::default(), &mut state, &session.read());
    let popup = state
        .popups
        .get_mut(DOWNLOAD_RUN_LARGE_POPUP_ID)
        .expect("the size confirm is registered");
    assert!(popup.open);
    assert_eq!(
        popup.scope,
        crate::model::popup_kind::PopupScope::Surface(101)
    );
}

/// S1 에서 연 묻기 대화상자에 S2 요청이 오면 범위와 대상이 모두 S2 다.
#[test]
fn a_second_ask_from_another_surface_moves_the_dialog_there() {
    let (mut state, mut session) = crate::state::tests::two_pane_fixture();
    for (surface, path) in [(101, "/remote/a.bin"), (201, "/remote/b.bin")] {
        ask(
            &mut state,
            1,
            surface,
            path.into(),
            String::new(),
            crate::file::dispatch::FileDispatchOrigin::User,
        );
        crate::intent::popup::apply_pending(&mut state, &mut session);
    }
    assert_eq!(
        state.popups.open_scope(DOWNLOAD_RUN_POPUP_ID),
        Some(&crate::model::popup_kind::PopupScope::Surface(201))
    );
    let ask = state.dialogs.download_run.ask.as_ref().unwrap();
    assert_eq!((ask.surface, ask.path.as_str()), (201, "/remote/b.bin"));
    assert!(state.popups.take_closed_queue().is_empty());
}

fn run(id: u64, surface: u32, receiving: bool) -> DownloadRunEntry {
    DownloadRunEntry {
        id,
        surface,
        name: String::new(),
        received: 0,
        total: 0,
        receiving,
        started: None,
    }
}

/// 첫 크기 확인은 그 다운로드를 시작한 surface 범위로 연다(App 의 크기 질문 경로).
#[test]
fn the_first_size_confirm_opens_in_the_surface_that_started_it() {
    let (mut state, _session) = crate::state::tests::two_pane_fixture();
    state.dialogs.download_run.runs.push(run(5, 201, false));
    queue_large_confirm(&mut state, 5, "big.bin".into(), 2 << 20);
    assert_eq!(
        opened_scope(&mut state, DOWNLOAD_RUN_LARGE_POPUP_ID),
        crate::model::popup_kind::PopupScope::Surface(201)
    );
    // 두 번째 확인은 큐에서 기다린다.
    state.dialogs.download_run.runs.push(run(6, 101, false));
    queue_large_confirm(&mut state, 6, "big2.bin".into(), 2 << 20);
    assert!(state.take_pending_intents().is_empty());
}

/// 진행 팝업은 처음 받는 다운로드의 surface 범위로 열고, 뒤에 다른 다운로드가 받기 시작해도
/// 범위를 바꾸지 않는다(App 의 진행 동기화 경로).
#[test]
fn the_progress_opens_in_the_first_receiving_surface_and_stays() {
    let (mut state, _session) = crate::state::tests::two_pane_fixture();
    state.dialogs.download_run.runs.push(run(5, 201, false));
    assert!(!open_progress_for_receiving(&mut state));
    state.dialogs.download_run.runs[0].receiving = true;
    assert!(open_progress_for_receiving(&mut state));
    state.dialogs.download_run.runs.push(run(6, 101, true));
    assert!(!open_progress_for_receiving(&mut state));
    assert_eq!(
        state.popups.open_scope(DOWNLOAD_RUN_PROGRESS_POPUP_ID),
        Some(&crate::model::popup_kind::PopupScope::Surface(201))
    );
}
