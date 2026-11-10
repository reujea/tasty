//! html surface의 원격 콘텐츠·스크립트 차단 배너를 webview chrome 위에 inset으로 그린다
//! (docs/surfaces/html/index.md). 크기 안내 아래에 원격 → 스크립트 순서로 쌓는다.
//!
//! 배너 단계는 `tasty_model::html_script`가 정하고 여기서는 읽기만 한다. 그린 카드 아래
//! `banner_inset_gap`까지의 높이를 돌려주면 호출자가 native WebView를 그만큼 내린다.
//! 재로드가 commit되어 단계가 사라지면 재로드 중 배너를 `banner_fade` 동안 흐리게 지운다.
//! 로드가 실패하면 배너를 페이드 없이 바로 내린다.
//! 배너는 키보드 포커스를 가져가지 않고, 클릭은 사용자 조작으로만 상태를 바꾼다.

use tasty_model::html_script::{BannerPhase, RemoteMarker};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlRemoteBannerView, HtmlRemoteMarkerKind, HtmlScriptBannerState, HtmlScriptBannerView,
    html_remote_banner, html_script_banner, html_script_banner_is_narrow, inset_banner_zone,
};

use crate::runtime::html_script::{HtmlAction, HtmlActionKind, HtmlSnapshot};

/// 이전 프레임에서 이어받는 페이드 상태.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct FadeMemo {
    /// 지난 프레임에 재로드 중 배너를 보였다.
    showing_reload: bool,
    /// 재로드 중 단계가 끝나 페이드를 시작한 시각(egui 시계, 초).
    fade_start: Option<f64>,
}

/// 이번 프레임에 그릴 배너와 불투명도.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    state: HtmlScriptBannerState,
    alpha: f32,
}

/// 모델 단계와 지난 페이드 상태로 이번 프레임의 표시를 정한다.
/// 닫기(×)와 문서 교체는 즉시 사라지고, 재로드 중에서 사라질 때만 페이드한다.
fn resolve(
    phase: BannerPhase,
    memo: FadeMemo,
    now: f64,
    fade_secs: f64,
) -> (Option<Shown>, FadeMemo) {
    match phase {
        BannerPhase::Blocked => (
            Some(Shown {
                state: HtmlScriptBannerState::Blocked,
                alpha: 1.0,
            }),
            FadeMemo::default(),
        ),
        BannerPhase::Loading => (
            Some(Shown {
                state: HtmlScriptBannerState::Loading,
                alpha: 1.0,
            }),
            FadeMemo::default(),
        ),
        BannerPhase::Reloading => (
            Some(Shown {
                state: HtmlScriptBannerState::Reloading,
                alpha: 1.0,
            }),
            FadeMemo {
                showing_reload: true,
                fade_start: None,
            },
        ),
        BannerPhase::Hidden if memo.showing_reload => {
            let start = memo.fade_start.unwrap_or(now);
            let progress = if fade_secs > 0.0 {
                (now - start) / fade_secs
            } else {
                1.0
            };
            if progress >= 1.0 {
                return (None, FadeMemo::default());
            }
            (
                Some(Shown {
                    state: HtmlScriptBannerState::Reloading,
                    alpha: (1.0 - progress) as f32,
                }),
                FadeMemo {
                    showing_reload: true,
                    fade_start: Some(start),
                },
            )
        }
        BannerPhase::Hidden => (None, FadeMemo::default()),
    }
}

/// 같은 inset 묶음에 쌓는 안내의 종류. 쌓는 순서는 원격 → 스크립트다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notice {
    Remote,
    Script,
}

impl Notice {
    fn memo_id(self, surface_id: u32) -> egui::Id {
        match self {
            Self::Remote => egui::Id::new(("html_remote_banner_fade", surface_id)),
            Self::Script => egui::Id::new(("html_script_banner_fade", surface_id)),
        }
    }

    fn phase(self, snapshot: &HtmlSnapshot) -> BannerPhase {
        match self {
            Self::Remote => snapshot.remote_phase,
            Self::Script => snapshot.phase,
        }
    }
}

/// 원격 안내가 화면에 있는지. 실패 상태에서는 안내를 그리지 않고, 재로드 뒤 페이드 중이면 아직 화면에 있다.
fn remote_notice_shown(phase: BannerPhase, load_failed: bool, fading: bool) -> bool {
    !load_failed && (phase != BannerPhase::Hidden || fading)
}

/// 주소 칸 끝 표지. 원격 안내가 화면에 있는 동안에는 모델 표지가 있어도 그리지 않는다.
fn address_marker(
    phase: BannerPhase,
    load_failed: bool,
    fading: bool,
    marker: Option<RemoteMarker>,
) -> Option<HtmlRemoteMarkerKind> {
    if remote_notice_shown(phase, load_failed, fading) {
        return None;
    }
    marker.map(|marker| match marker {
        RemoteMarker::Blocked => HtmlRemoteMarkerKind::Blocked,
        RemoteMarker::Allowed => HtmlRemoteMarkerKind::Allowed,
    })
}

/// 이번 프레임에 주소 칸 끝에 그릴 원격 표지. 원격 안내(재로드 뒤 페이드 포함)가 화면에 없을 때만 있다.
pub fn remote_address_marker(
    ctx: &egui::Context,
    snapshot: &HtmlSnapshot,
    surface_id: u32,
) -> Option<HtmlRemoteMarkerKind> {
    let fading = ctx
        .data(|d| d.get_temp::<FadeMemo>(Notice::Remote.memo_id(surface_id)))
        .is_some_and(|m| m.showing_reload);
    address_marker(
        snapshot.remote_phase,
        snapshot.load_failed,
        fading,
        snapshot.remote_marker,
    )
}

/// inset 안내 묶음의 세로 배치. 크기 안내가 있으면 그 카드 뒤에서 시작하고, 원격 → 스크립트 순서로
/// `place(안내, 패널 위쪽 오프셋)`을 부른다. `place`는 그 안내를 그렸으면 패널 위쪽에서 카드 아래
/// `banner_inset_gap`까지의 거리를 돌려준다. 각 카드는 자기 위에 `banner_margin`을 두므로 다음 패널은
/// 앞 카드의 간격 끝에서 `gap`만큼 올라가 시작한다. 반환값은 페이지가 시작할 오프셋이다.
fn stack_notices(
    margin: LogicalPx,
    gap: LogicalPx,
    size: Option<LogicalPx>,
    mut place: impl FnMut(Notice, LogicalPx) -> Option<LogicalPx>,
) -> LogicalPx {
    let mut next = LogicalPx(0.0);
    let mut end = LogicalPx(0.0);
    if let Some(height) = size {
        next = margin + height;
        end = next + gap;
    }
    for notice in [Notice::Remote, Notice::Script] {
        if let Some(h) = place(notice, next) {
            end = next + h;
            next = end - gap;
        }
    }
    end
}

/// 주소 띠 아래 inset 안내 묶음(크기 → 원격 → 스크립트)을 그리고 `below` 위쪽에서 페이지가
/// 시작할 곳까지의 거리를 돌려준다. `size`는 이미 그린 크기 안내 카드의 높이다.
/// 각 카드는 자기 위에 `banner_margin`을 두므로 앞 카드의 끝이 다음 카드 영역의 시작이 된다.
pub fn draw_stack(
    ui: &mut egui::Ui,
    theme: &Theme,
    snapshot: Option<&HtmlSnapshot>,
    actions: &mut Vec<HtmlAction>,
    surface_id: u32,
    below: egui::Rect,
    size: Option<LogicalPx>,
) -> LogicalPx {
    stack_notices(
        theme.banner_margin(),
        theme.banner_inset_gap(),
        size,
        |notice, top| {
            let snapshot = snapshot?;
            let panel = below.with_min_y(below.min.y + top.value());
            draw_one(ui, theme, snapshot, actions, surface_id, panel, notice)
        },
    )
}

/// 스크립트 안내의 제목·본문·버튼 문구 키. `http(s)` 페이지(`web`)는 페이지 문구를 쓰고, 두 안내가
/// 함께 보이지 않으므로 `body_remote`를 쓰지 않는다.
fn script_copy(web: bool, remote_body: bool) -> (&'static str, &'static str, &'static str) {
    if web {
        return (
            "banner.html_script.title_web",
            "banner.html_script.body_web",
            "banner.html_script.action_web",
        );
    }
    let body = if remote_body {
        "banner.html_script.body_remote"
    } else {
        "banner.html_script.body"
    };
    (
        "banner.html_script.title",
        body,
        "banner.html_script.action",
    )
}

/// 원격 안내의 제목·본문 문구 키. 버튼과 재로드 문구는 로컬 문서와 공유한다.
/// sandbox가 꺼진 페이지는 스크립트 안내가 없으므로 네트워크 스크립트를 차단 목록에 넣은 본문을 쓴다.
/// 매 프레임 스냅샷 값으로 고르므로 안내가 떠 있는 동안 설정이 바뀌면 다음 프레임에 바뀐다.
fn remote_copy(web: bool, sandbox: bool) -> (&'static str, &'static str) {
    if web {
        (
            "banner.html_remote.title_web",
            if sandbox {
                "banner.html_remote.body_web"
            } else {
                "banner.html_remote.body_web_sandbox_off"
            },
        )
    } else {
        ("banner.html_remote.title", "banner.html_remote.body")
    }
}

/// `panel`에 안내 하나를 그리고 패널 위쪽에서 카드 아래 `banner_inset_gap`까지의 거리를 돌려준다.
/// 안내가 없으면 `None`이다. 허용·닫기는 표시한 문서에 묶인 App 요청으로 반환한다.
fn draw_one(
    ui: &mut egui::Ui,
    theme: &Theme,
    snapshot: &HtmlSnapshot,
    actions: &mut Vec<HtmlAction>,
    surface_id: u32,
    panel: egui::Rect,
    notice: Notice,
) -> Option<LogicalPx> {
    let memo_id = notice.memo_id(surface_id);
    let memo = ui
        .ctx()
        .data(|d| d.get_temp::<FadeMemo>(memo_id))
        .unwrap_or_default();
    let now = ui.ctx().input(|i| i.time);
    // 실패 상태의 배너는 이전 문서를 가리키므로 재로드 중 배너도 페이드 없이 내린다.
    let (shown, memo) = if snapshot.load_failed {
        (None, FadeMemo::default())
    } else {
        resolve(
            notice.phase(snapshot),
            memo,
            now,
            theme.banner_fade().to_secs_f64(),
        )
    };
    ui.ctx().data_mut(|d| d.insert_temp(memo_id, memo));
    let shown = shown?;
    if shown.alpha < 1.0 {
        ui.ctx().request_repaint();
    }

    let t = crate::i18n::t;
    let narrow = html_script_banner_is_narrow(panel.width(), theme);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inset_banner_zone(panel, theme)));
    child.set_clip_rect(panel);
    child.set_opacity(shown.alpha);
    let (out, allow, dismiss) = match notice {
        Notice::Script => {
            let (title, body, action) = script_copy(snapshot.web, snapshot.remote_body);
            let view = HtmlScriptBannerView {
                title: t(title),
                body: t(body),
                action: t(action),
                reloading: t("banner.html_script.reloading"),
                loading_tooltip: t("banner.html_script.action_loading"),
                state: shown.state,
                narrow,
                force_hover: false,
            };
            (
                html_script_banner(&mut child, theme, &view),
                HtmlActionKind::Allow,
                HtmlActionKind::Dismiss,
            )
        }
        Notice::Remote => {
            let (title, body) = remote_copy(snapshot.web, snapshot.sandbox);
            let view = HtmlRemoteBannerView {
                title: t(title),
                body: t(body),
                action: t("banner.html_remote.action"),
                reloading: t("banner.html_remote.reloading"),
                loading_tooltip: t("banner.html_remote.action_loading"),
                state: shown.state,
                narrow,
                force_hover: false,
            };
            (
                html_remote_banner(&mut child, theme, &view),
                HtmlActionKind::RemoteAllow,
                HtmlActionKind::RemoteDismiss,
            )
        }
    };
    if shown.alpha >= 1.0 {
        if out.allow_clicked {
            actions.push(snapshot.request(allow));
        } else if out.dismiss_clicked {
            tracing::debug!("html {notice:?} banner: surface {surface_id} dismissed by the user");
            actions.push(snapshot.request(dismiss));
        }
    }
    Some(LogicalPx(
        out.rect.bottom() + theme.banner_inset_gap().value() - panel.top(),
    ))
}

#[cfg(test)]
mod tests;
