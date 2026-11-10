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
