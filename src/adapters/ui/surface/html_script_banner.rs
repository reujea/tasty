//! html surface의 스크립트 차단 배너를 webview chrome 위에 inset으로 그린다(docs/surfaces/html/index.md).
//!
//! 배너 단계는 `tasty_model::html_script`가 정하고 여기서는 읽기만 한다. 그린 카드 아래
//! `banner_inset_gap`까지의 높이를 돌려주면 호출자가 native WebView를 그만큼 내린다.
//! 재로드가 commit되어 단계가 사라지면 재로드 중 배너를 `banner_fade` 동안 흐리게 지운다.
//! 로드가 실패하면 배너를 페이드 없이 바로 내린다.
//! 배너는 키보드 포커스를 가져가지 않고, 클릭은 사용자 조작으로만 상태를 바꾼다.

use tasty_model::html_script::BannerPhase;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlScriptBannerState, HtmlScriptBannerView, html_script_banner, html_script_banner_is_narrow,
    inset_banner_zone,
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

/// `panel`에 배너를 그리고 패널 위쪽에서 페이지가 시작할 곳까지의 거리를 돌려준다.
/// 배너가 없으면 `None`이다. 허용·닫기는 표시한 문서에 묶인 App 요청으로 반환한다.
pub fn draw(
    ui: &mut egui::Ui,
    theme: &Theme,
    snapshot: &HtmlSnapshot,
    actions: &mut Vec<HtmlAction>,
    surface_id: u32,
    panel: egui::Rect,
) -> Option<LogicalPx> {
    let phase = snapshot.phase;
    let remote_body = snapshot.remote_body;
    let memo_id = egui::Id::new(("html_script_banner_fade", surface_id));
    let memo = ui
        .ctx()
        .data(|d| d.get_temp::<FadeMemo>(memo_id))
        .unwrap_or_default();
    let now = ui.ctx().input(|i| i.time);
    // 실패 상태의 배너는 이전 문서를 가리키므로 재로드 중 배너도 페이드 없이 내린다.
    let (shown, memo) = if snapshot.load_failed {
        (None, FadeMemo::default())
    } else {
        resolve(phase, memo, now, theme.banner_fade().to_secs_f64())
    };
    ui.ctx().data_mut(|d| d.insert_temp(memo_id, memo));
    let shown = shown?;
    if shown.alpha < 1.0 {
        ui.ctx().request_repaint();
    }

    let t = crate::i18n::t;
    let view = HtmlScriptBannerView {
        title: t("banner.html_script.title"),
        body: t(if remote_body {
            "banner.html_script.body_remote"
        } else {
            "banner.html_script.body"
        }),
        action: t("banner.html_script.action"),
        reloading: t("banner.html_script.reloading"),
        loading_tooltip: t("banner.html_script.action_loading"),
        state: shown.state,
        narrow: html_script_banner_is_narrow(panel.width(), theme),
        force_hover: false,
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inset_banner_zone(panel, theme)));
    child.set_clip_rect(panel);
    child.set_opacity(shown.alpha);
    let out = html_script_banner(&mut child, theme, &view);
    if shown.alpha >= 1.0 {
        if out.allow_clicked {
            actions.push(snapshot.request(HtmlActionKind::Allow));
        } else if out.dismiss_clicked {
            tracing::debug!("html script banner: surface {surface_id} dismissed by the user");
            actions.push(snapshot.request(HtmlActionKind::Dismiss));
        }
    }
    Some(LogicalPx(
        out.rect.bottom() + theme.banner_inset_gap().value() - panel.top(),
    ))
}

#[cfg(test)]
mod tests;
