//! Received-design size decisions and page state; all transport effects remain App-owned.
use crate::runtime::html_receive::{ReceiveAction, ReceiveActionKind, ReceiveSnapshot};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlReceiveKind, HtmlReceiveView, HtmlSizeAction, HtmlSizeBannerView, HtmlSizeDetail,
    html_receive_state, html_size_banner, inset_banner_zone,
};

pub(crate) fn size_banner(
    ui: &mut egui::Ui,
    theme: &Theme,
    snapshot: &ReceiveSnapshot,
    panel: egui::Rect,
    actions: &mut Vec<ReceiveAction>,
) -> Option<LogicalPx> {
    let size = snapshot.size_notice;
    if size.is_none() && !snapshot.paused {
        return None;
    }
    let bytes = size.unwrap_or(snapshot.received);
    let count = decimal_bytes(bytes);
    let title = if size.is_some() {
        crate::i18n::t("html_remote.size_title").to_owned()
    } else {
        crate::i18n::t_fmt("html_remote.limit_title", &count)
    };
    let waiting = (snapshot.waiting > 0)
        .then(|| crate::i18n::t_fmt("html_remote.waiting", &snapshot.waiting.to_string()));
    let view = HtmlSizeBannerView {
        title: &title,
        detail: if size.is_some() {
            HtmlSizeDetail::File {
                path: &snapshot.notice_path,
                size: &count,
                waiting: waiting.as_deref(),
            }
        } else {
            HtmlSizeDetail::Limit {
                body: crate::i18n::t("html_remote.limit_body"),
            }
        },
        decline: crate::i18n::t(if size.is_some() {
            "html_remote.decline"
        } else {
            "html_remote.stop"
        }),
        allow: crate::i18n::t(if size.is_some() {
            "html_remote.allow"
        } else {
            "html_remote.continue"
        }),
        narrow: LogicalPx(panel.width()) < theme.banner_narrow_below(),
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inset_banner_zone(panel, theme)));
    child.set_clip_rect(ui.clip_rect().intersect(panel));
    let result = html_size_banner(&mut child, theme, &view);
    let kind = match (result.action, size.is_some()) {
        (HtmlSizeAction::Allow, true) => Some(ReceiveActionKind::Allow),
        (HtmlSizeAction::Allow, false) => Some(ReceiveActionKind::Continue),
        (HtmlSizeAction::Decline, true) => Some(ReceiveActionKind::Decline),
        (HtmlSizeAction::Decline, false) => Some(ReceiveActionKind::Stop),
        _ => None,
    };
    if let Some(action) = kind.and_then(|kind| snapshot.action(kind)) {
        actions.push(action);
    }
    Some(LogicalPx(result.rect.height()))
}
pub(crate) fn page(ui: &mut egui::Ui, theme: &Theme, snapshot: &ReceiveSnapshot, rect: egui::Rect) {
    if snapshot.ready && (!snapshot.receiving || !snapshot.connected) {
        return;
    }
    let progress = crate::i18n::t_count(
        "html_remote.progress",
        snapshot.files as u64,
        &[
            &decimal_bytes(snapshot.received),
            &snapshot.files.to_string(),
        ],
    );
    let paused = snapshot.paused || snapshot.size_notice.is_some();
    let (kind, title, subtitle) = if !snapshot.connected {
        (
            HtmlReceiveKind::Disconnected,
            crate::i18n::t("html_address.disconnected"),
            crate::i18n::t("html_remote.disconnected_sub"),
        )
    } else if snapshot.receiving {
        (
            if paused {
                HtmlReceiveKind::Paused
            } else {
                HtmlReceiveKind::Receiving
            },
            crate::i18n::t("html_remote.receiving"),
            if paused {
                crate::i18n::t("html_remote.paused")
            } else {
                &progress
            },
        )
    } else {
        return;
    };
    html_receive_state(
        ui,
        theme,
        rect,
        &HtmlReceiveView {
            kind,
            title,
            subtitle,
        },
    );
}

// Transfer limits and the received design use decimal KB/MB.
fn decimal_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// 받은 원격 entry 자리에 그릴 WebView 상태 타일. 전송이 실패했거나 받기를 마친 본문의 native 로드가
/// 실패했으면 Failed(오류 타일), 받은 문서를 띄울 수 없거나 본문 없이 받기가 끝났으면 Idle, 그 밖에는 없다.
/// 받기를 마친 본문은 host 수신 화면(`page`)이 그리지 않고 native 실패 페이지는 숨겨지므로 여기서 실패를 보인다.
pub(crate) fn received_chrome_state(
    snapshot: &ReceiveSnapshot,
    unshowable: bool,
    nav: crate::webview::NavState,
) -> Option<crate::webview::NavState> {
    chrome_state(
        ReceivedTile {
            failed: snapshot.failed,
            ready: snapshot.ready,
            receiving: snapshot.receiving,
            connected: snapshot.connected,
        },
        unshowable,
        nav,
    )
}

#[derive(Clone, Copy)]
struct ReceivedTile {
    failed: bool,
    ready: bool,
    receiving: bool,
    connected: bool,
}

fn chrome_state(
    r: ReceivedTile,
    unshowable: bool,
    nav: crate::webview::NavState,
) -> Option<crate::webview::NavState> {
    use crate::webview::NavState;
    if r.failed || (r.ready && !r.receiving && nav == NavState::Failed) {
        Some(NavState::Failed)
    } else if unshowable || (!r.receiving && !r.ready && r.connected) {
        Some(NavState::Idle)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{ReceivedTile, chrome_state};
    use crate::webview::NavState;

    const RECEIVED: ReceivedTile = ReceivedTile {
        failed: false,
        ready: true,
        receiving: false,
        connected: true,
    };

    /// Given 받기를 마친 원격 entry(ready, 받는 중 아님, 전송 실패 아님), When native 로드가 실패함,
    /// Then 빈 본문 대신 오류 타일을 그린다. 연결이 끊겨 보존한 본문도 같다.
    #[test]
    fn a_received_entry_whose_page_fails_shows_the_error_tile() {
        assert_eq!(
            chrome_state(RECEIVED, false, NavState::Failed),
            Some(NavState::Failed)
        );
        let kept = ReceivedTile {
            connected: false,
            ..RECEIVED
        };
        assert_eq!(
            chrome_state(kept, false, NavState::Failed),
            Some(NavState::Failed)
        );
    }

    /// Given 받기를 마친 원격 entry, When native가 로드 중이거나 끝남, Then 타일 없이 페이지 표시에 맡긴다.
    #[test]
    fn a_received_entry_that_loads_draws_no_tile() {
        assert_eq!(chrome_state(RECEIVED, false, NavState::Loading), None);
        assert_eq!(chrome_state(RECEIVED, false, NavState::Done), None);
    }

    /// Given 아직 받는 중인 entry, When native 로드가 실패함, Then 수신 화면이 그 자리를 그리므로 타일이 없다.
    /// 전송 실패는 native 상태와 관계없이 오류 타일이다.
    #[test]
    fn transfer_state_still_decides_before_the_body_is_received() {
        let receiving = ReceivedTile {
            ready: false,
            receiving: true,
            ..RECEIVED
        };
        assert_eq!(chrome_state(receiving, false, NavState::Failed), None);
        let failed = ReceivedTile {
            failed: true,
            ready: false,
            ..RECEIVED
        };
        assert_eq!(
            chrome_state(failed, false, NavState::Idle),
            Some(NavState::Failed)
        );
        let empty = ReceivedTile {
            ready: false,
            ..RECEIVED
        };
        assert_eq!(
            chrome_state(empty, false, NavState::Idle),
            Some(NavState::Idle)
        );
        assert_eq!(
            chrome_state(RECEIVED, true, NavState::Idle),
            Some(NavState::Idle)
        );
    }
}
