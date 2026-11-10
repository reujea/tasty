use super::*;

const FADE: f64 = 0.12;

fn phase_after(phase: BannerPhase, memo: FadeMemo, now: f64) -> (Option<Shown>, FadeMemo) {
    resolve(phase, memo, now, FADE)
}

#[test]
fn blocked_and_reloading_show_at_full_opacity() {
    let (shown, _) = phase_after(BannerPhase::Blocked, FadeMemo::default(), 0.0);
    assert_eq!(
        shown,
        Some(Shown {
            state: HtmlScriptBannerState::Blocked,
            alpha: 1.0
        })
    );
    let (shown, memo) = phase_after(BannerPhase::Reloading, FadeMemo::default(), 0.0);
    assert_eq!(
        shown.map(|s| s.state),
        Some(HtmlScriptBannerState::Reloading)
    );
    assert!(memo.showing_reload);
}

#[test]
fn loading_shows_the_disabled_allow_at_full_opacity() {
    let (shown, memo) = phase_after(BannerPhase::Loading, FadeMemo::default(), 0.0);
    assert_eq!(
        shown,
        Some(Shown {
            state: HtmlScriptBannerState::Loading,
            alpha: 1.0
        })
    );
    assert_eq!(memo, FadeMemo::default());
}

#[test]
fn a_committed_reload_fades_out_over_the_banner_fade() {
    let (_, memo) = phase_after(BannerPhase::Reloading, FadeMemo::default(), 1.0);
    let (shown, memo) = phase_after(BannerPhase::Hidden, memo, 2.0);
    let first = shown.expect("fade starts");
    assert_eq!(first.state, HtmlScriptBannerState::Reloading);
    assert_eq!(first.alpha, 1.0);
    let (shown, memo) = phase_after(BannerPhase::Hidden, memo, 2.0 + FADE / 2.0);
    let half = shown.expect("still fading").alpha;
    assert!((half - 0.5).abs() < 1e-3, "{half}");
    let (shown, memo) = phase_after(BannerPhase::Hidden, memo, 2.0 + FADE);
    assert_eq!(shown, None);
    assert_eq!(memo, FadeMemo::default());
}

#[test]
fn dismissing_or_a_new_document_hides_without_a_fade() {
    let (_, memo) = phase_after(BannerPhase::Blocked, FadeMemo::default(), 1.0);
    let (shown, _) = phase_after(BannerPhase::Hidden, memo, 1.01);
    assert_eq!(shown, None);
}

const MARGIN: LogicalPx = LogicalPx(8.0);
const GAP: LogicalPx = LogicalPx(8.0);

/// 그린 안내와 그 패널 위쪽 오프셋을 기록하며 정해진 높이를 돌려준다.
fn stack_with(
    size: Option<LogicalPx>,
    heights: &[(Notice, f32)],
) -> (Vec<(Notice, f32)>, LogicalPx) {
    let mut placed = Vec::new();
    let end = stack_notices(MARGIN, GAP, size, |notice, top| {
        let h = heights.iter().find(|(n, _)| *n == notice)?.1;
        placed.push((notice, top.value()));
        Some(LogicalPx(h))
    });
    (placed, end)
}

#[test]
fn notices_stack_size_then_remote_then_script() {
    // 크기 카드 40, 원격 패널 높이(margin 8 + 카드 50 + gap 8) 66, 스크립트 패널 높이 58.
    let (placed, end) = stack_with(
        Some(LogicalPx(40.0)),
        &[(Notice::Script, 58.0), (Notice::Remote, 66.0)],
    );
    // 원격은 크기 카드 아래(margin 8 + 40)에서, 스크립트는 원격 카드의 간격 끝에서 gap만큼 올라가 시작한다.
    assert_eq!(
        placed,
        vec![(Notice::Remote, 48.0), (Notice::Script, 106.0)]
    );
    assert_eq!(end, LogicalPx(164.0));
}

#[test]
fn an_absent_notice_takes_no_room() {
    let (placed, end) = stack_with(None, &[(Notice::Script, 58.0)]);
    assert_eq!(placed, vec![(Notice::Script, 0.0)]);
    assert_eq!(end, LogicalPx(58.0));
    let (placed, end) = stack_with(Some(LogicalPx(40.0)), &[]);
    assert!(placed.is_empty());
    assert_eq!(
        end,
        LogicalPx(56.0),
        "size card alone keeps margin + card + gap"
    );
}

#[test]
fn the_address_marker_waits_until_the_remote_notice_is_off_screen() {
    let blocked = Some(RemoteMarker::Blocked);
    let allowed = Some(RemoteMarker::Allowed);
    for phase in [
        BannerPhase::Blocked,
        BannerPhase::Loading,
        BannerPhase::Reloading,
    ] {
        assert_eq!(
            address_marker(phase, false, false, allowed),
            None,
            "{phase:?}"
        );
    }
    assert_eq!(
        address_marker(BannerPhase::Hidden, false, true, allowed),
        None,
        "fading notice is still on screen"
    );
    assert_eq!(
        address_marker(BannerPhase::Hidden, false, false, allowed),
        Some(HtmlRemoteMarkerKind::Allowed)
    );
    assert_eq!(
        address_marker(BannerPhase::Hidden, false, false, blocked),
        Some(HtmlRemoteMarkerKind::Blocked)
    );
    assert_eq!(
        address_marker(BannerPhase::Hidden, false, false, None),
        None
    );
    // 실패 상태에서는 안내를 그리지 않으므로 표지는 모델 값을 따른다.
    assert_eq!(
        address_marker(BannerPhase::Blocked, true, true, blocked),
        Some(HtmlRemoteMarkerKind::Blocked)
    );
}

/// `http(s)` 페이지는 페이지 문구를 쓰고 `body_remote`를 쓰지 않는다. 로컬 문서 문구는 그대로다.
#[test]
fn a_web_page_reads_the_page_copy_and_never_the_remote_body() {
    assert_eq!(
        script_copy(true, true),
        (
            "banner.html_script.title_web",
            "banner.html_script.body_web",
            "banner.html_script.action_web",
        )
    );
    assert_eq!(script_copy(false, true).1, "banner.html_script.body_remote");
    assert_eq!(
        script_copy(false, false),
        (
            "banner.html_script.title",
            "banner.html_script.body",
            "banner.html_script.action",
        )
    );
    assert_eq!(
        remote_copy(true, true),
        (
            "banner.html_remote.title_web",
            "banner.html_remote.body_web"
        )
    );
    assert_eq!(
        remote_copy(true, false),
        (
            "banner.html_remote.title_web",
            "banner.html_remote.body_web_sandbox_off"
        ),
        "without the sandbox there is no script notice to promise"
    );
    for sandbox in [true, false] {
        assert_eq!(
            remote_copy(false, sandbox),
            ("banner.html_remote.title", "banner.html_remote.body")
        );
    }
}
