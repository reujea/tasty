//! 주소 칸으로 연 top-level `http(s)` 페이지의 원격·스크립트 안내(디자인 `#htmlremote`의
//! `http-s-page-*` 두 Spec). 셸·글리프·단계·표지·토큰·쌓임 순서는 로컬 문서 안내와 같고 문구만
//! `_web` 키를 쓴다. 두 안내는 페이지에서 함께 보이지 않고, 스크립트 안내는 원격 재로드가 commit된 뒤
//! 같은 자리에 나온다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlRemoteBannerState, HtmlRemoteMarkerKind, HtmlScriptBannerState, HtmlScriptMarkerKind,
    Tooltip, TooltipPlacement, html_remote_banner, html_script_banner,
};

use super::html_remote_banner::{
    BODY_WEB_SANDBOX_OFF, SCRIPT_MARKER_ALLOWED_WEB, SCRIPT_MARKER_BLOCKED, Variant, WEB_LABEL,
    address_band, marker_view, remote_surface, remote_view, script_view, web_address_view,
};
use super::html_script_banner::{app_backdrop, caption, framed, mocha, tab, themed_card};
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant};
use crate::i18n::t;

/// 상태 표본의 두 폭. 디자인 `--tasty-size-560`(넓음)과 `--tasty-size-360`(좁음, 440 미만).
const WIDE_W: LogicalPx = LogicalPx(560.0);
const NARROW_W: LogicalPx = LogicalPx(360.0);
/// 표지 표본의 폭. 디자인 `--tasty-size-460`.
const MARKER_W: LogicalPx = LogicalPx(460.0);
/// 흐름 프레임의 크기. 디자인 `--tasty-size-480`, `height={320}`.
const FLOW_W: LogicalPx = LogicalPx(480.0);
const FLOW_H: LogicalPx = LogicalPx(320.0);
/// 좁은 흐름 프레임 1·2의 높이. 디자인 `height={400}`·`height={360}`.
const NARROW_ONE_H: LogicalPx = LogicalPx(400.0);
const NARROW_TWO_H: LogicalPx = LogicalPx(360.0);
/// 툴팁 프레임 위쪽 여백. 디자인 `paddingTop: --tasty-size-64`(버블이 위로 열린다).
const TIP_TOP: LogicalPx = LogicalPx(64.0);

/// 상태 표본 한 줄: 캡션과 그릴 안내.
#[derive(Clone, Copy)]
enum Row {
    Remote(HtmlRemoteBannerState, bool),
    /// 전역 script sandbox 꺼짐의 원격 안내(blocked).
    RemoteSandboxOff,
    Script(HtmlScriptBannerState),
}

const ROWS: [(&str, Row); 8] = [
    (
        "remote · blocked",
        Row::Remote(HtmlRemoteBannerState::Blocked, false),
    ),
    (
        "remote · hover — × revealed",
        Row::Remote(HtmlRemoteBannerState::Blocked, true),
    ),
    (
        "remote · loading — Allow disabled until commit",
        Row::Remote(HtmlRemoteBannerState::Loading, false),
    ),
    (
        "remote · reloading",
        Row::Remote(HtmlRemoteBannerState::Reloading, false),
    ),
    (
        "remote · blocked · script sandbox off — network scripts listed, no script sentence",
        Row::RemoteSandboxOff,
    ),
    (
        "script · blocked (after the remote reload)",
        Row::Script(HtmlScriptBannerState::Blocked),
    ),
    (
        "script · loading — Allow disabled until commit",
        Row::Script(HtmlScriptBannerState::Loading),
    ),
    (
        "script · reloading",
        Row::Script(HtmlScriptBannerState::Reloading),
    ),
];

fn draw_row(ui: &mut egui::Ui, theme: &Theme, row: Row, narrow: bool) {
    match row {
        Row::Remote(state, hover) => {
            let mut view = remote_view(state, hover, true);
            view.narrow = narrow;
            html_remote_banner(ui, theme, &view);
        }
        Row::RemoteSandboxOff => {
            let mut view = remote_view(HtmlRemoteBannerState::Blocked, false, true);
            view.body = t(BODY_WEB_SANDBOX_OFF);
            view.narrow = narrow;
            html_remote_banner(ui, theme, &view);
        }
        Row::Script(state) => {
            let mut view = script_view(state, false, true);
            view.narrow = narrow;
            html_script_banner(ui, theme, &view);
        }
    }
}

/// 탭 스트립 한 칸(디자인 `HsTab` `markerWeb`). 표지 rect를 돌려준다.
fn web_tab(ui: &mut egui::Ui, theme: &Theme, kind: HtmlScriptMarkerKind) -> Option<egui::Rect> {
    let tip = t(script_marker_tip(kind));
    let mut out = None;
    framed(
        ui,
        theme,
        egui::vec2(MARKER_W.value(), theme.tab_height().value()),
        |ui, inner| {
            ui.painter()
                .rect_filled(inner, 0.0, theme.bg_sidebar().to_egui());
            out = tab(
                ui,
                theme,
                inner,
                WEB_LABEL,
                icons::HTML,
                true,
                Some((kind, tip)),
            );
        },
    );
    out
}

fn script_marker_tip(kind: HtmlScriptMarkerKind) -> &'static str {
    match kind {
        HtmlScriptMarkerKind::Blocked => SCRIPT_MARKER_BLOCKED,
        HtmlScriptMarkerKind::Allowed => SCRIPT_MARKER_ALLOWED_WEB,
    }
}

/// web 주소 띠(끝에 원격 표지). 표지 rect를 돌려준다.
fn web_band(
    ui: &mut egui::Ui,
    theme: &Theme,
    salt: &str,
    kind: HtmlRemoteMarkerKind,
) -> Option<egui::Rect> {
    let view = web_address_view(false, Some(marker_view(kind, false, true)));
    let mut out = None;
    framed(
        ui,
        theme,
        egui::vec2(MARKER_W.value(), theme.html_address_bar_height().value()),
        |ui, inner| {
            out = address_band(ui, theme, salt, LogicalPx(inner.width()), &view);
        },
    );
    out
}

/// Spec — 페이지에서 두 안내의 상태(넓음·좁음)와 두 표지. 시안처럼 테마마다 상태 패널을 쌓고 그 뒤에
/// 표지 패널 둘을 나란히 둔다.
pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
            for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                themed_card(ui, th, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing =
                            egui::vec2(th.spacing_lg.value(), th.spacing_lg.value());
                        for (w, narrow, width) in [
                            ("wide (≥ 440)", false, WIDE_W),
                            ("narrow (< 440)", true, NARROW_W),
                        ] {
                            spec::wrap_item(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(width.value());
                                    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                                    for (cap, row) in ROWS {
                                        caption(ui, th, &format!("{name} · {w} · {cap}"));
                                        draw_row(ui, th, row, narrow);
                                    }
                                });
                            });
                        }
                    });
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
                for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                    spec::wrap_item(ui, |ui| {
                        themed_card(ui, th, |ui| marker_panel(ui, th, name));
                    });
                }
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "address field",
                "kind web · link glyph · remote marker at the field end (unchanged slot)",
            ),
            (
                "remote notice",
                "globeOff · title_web / body_web · Allow remote content · blocked / loading / reloading",
            ),
            (
                "script notice",
                "lock · title_web / body_web · action_web · blocked / loading / reloading",
            ),
            (
                "body_remote",
                "never on a web page (the notices never stack)",
            ),
            (
                "markers",
                "same glyphs and inks · allowed tooltips read the _web copy",
            ),
            (
                "narrow",
                "< 440 → action wraps under the body, same as local",
            ),
            (
                "lifetime",
                "this showing · cleared by the user's reload, navigation, Back, restart",
            ),
            (
                "sandbox off",
                "remote notice body → body_web_sandbox_off · no script notice, no tab marker",
            ),
            ("new tokens", "none"),
        ],
        &[],
    );
    spec::note(
        ui,
        theme,
        "Shared with local documents: remote notice action, reloading, action_loading, \
         marker_blocked · script notice reloading, action_loading, marker_blocked only. The \
         script notice's button on a web page is action_web, not action.",
    );
    spec::dont(
        ui,
        theme,
        "Don't show the script notice under the remote notice on a web page, and don't let one \
         Allow open both.",
    );
}

/// 페이지의 두 표지(디자인 `RcWebMarkerGrid`): 원격 = web 주소 칸 끝, 스크립트 = 탭 칸 오른쪽 묶음.
fn marker_panel(ui: &mut egui::Ui, th: &Theme, name: &str) {
    ui.vertical(|ui| {
        ui.set_width(MARKER_W.value());
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        for (cap, kind) in [
            (
                "remote marker · blocked (notice dismissed)",
                HtmlRemoteMarkerKind::Blocked,
            ),
            (
                "remote marker · allowed (this showing)",
                HtmlRemoteMarkerKind::Allowed,
            ),
        ] {
            caption(ui, th, &format!("{name} · {cap}"));
            web_band(ui, th, &format!("{name}-{cap}"), kind);
        }
        for (cap, kind) in [
            (
                "script marker · blocked (notice dismissed)",
                HtmlScriptMarkerKind::Blocked,
            ),
            (
                "script marker · allowed (this showing)",
                HtmlScriptMarkerKind::Allowed,
            ),
        ] {
            caption(ui, th, &format!("{name} · {cap}"));
            web_tab(ui, th, kind);
        }
    });
}

/// 흐름 프레임 하나(디자인 `RC_WEB_FLOW`). 원격 차단·sandbox 켬 기준이다.
fn flow() -> Vec<(&'static str, Variant)> {
    use HtmlRemoteBannerState as R;
    use HtmlRemoteMarkerKind as M;
    use HtmlScriptBannerState as S;
    let v = Variant {
        web: true,
        ..Variant::default()
    };
    vec![
        (
            "1 · page viewed — the remote notice first shows when the load commits, already blocked · no marker · no script notice",
            Variant {
                remote: Some(R::Blocked),
                ..v
            },
        ),
        (
            "2 · Allow remote content — reloading",
            Variant {
                remote: Some(R::Reloading),
                loading: true,
                ..v
            },
        ),
        (
            "2 · reload committed — remote marker allowed, script notice takes the same slot",
            Variant {
                script: Some(S::Blocked),
                remote_marker: Some(M::Allowed),
                styled: true,
                ..v
            },
        ),
        (
            "3 · Allow scripts on this page — reloading, remote stays allowed",
            Variant {
                script: Some(S::Reloading),
                remote_marker: Some(M::Allowed),
                styled: true,
                loading: true,
                ..v
            },
        ),
        (
            "3 · reload committed — no notice · both markers allowed",
            Variant {
                remote_marker: Some(M::Allowed),
                script_marker: Some(HtmlScriptMarkerKind::Allowed),
                styled: true,
                ..v
            },
        ),
        (
            "4a · user reload before Allow — the notice stays, loading: Allow disabled, Reload busy, until the reload commits",
            Variant {
                remote: Some(R::Loading),
                loading: true,
                ..v
            },
        ),
        (
            "4b · user reload after Allow (from step 3) — load in flight: no notice, markers stay allowed, Reload busy",
            Variant {
                remote_marker: Some(M::Allowed),
                script_marker: Some(HtmlScriptMarkerKind::Allowed),
                styled: true,
                loading: true,
                ..v
            },
        ),
        (
            "4 · reload committed (4a or 4b; also another page, Back, restart) — both allows cleared, remote notice blocked as at step 1, shown even if dismissed before",
            Variant {
                remote: Some(R::Blocked),
                ..v
            },
        ),
        (
            "× at step 1 — globeOff marker only · no script notice while remote is blocked",
            Variant {
                remote_marker: Some(M::Blocked),
                ..v
            },
        ),
        (
            "× at step 2 — remote marker allowed · tab lock · marker click shows the notice again",
            Variant {
                remote_marker: Some(M::Allowed),
                script_marker: Some(HtmlScriptMarkerKind::Blocked),
                styled: true,
                ..v
            },
        ),
        (
            "global Allow remote content on · sandbox on — script notice at once, no remote notice or marker",
            Variant {
                script: Some(S::Blocked),
                styled: true,
                ..v
            },
        ),
        (
            "script sandbox off · step 1 — remote notice with the sandbox-off body (network scripts listed, no script sentence)",
            Variant {
                remote: Some(R::Blocked),
                sandbox_off: true,
                ..v
            },
        ),
        (
            "script sandbox off · remote reload committed — remote marker allowed · no script notice · no tab marker",
            Variant {
                remote_marker: Some(M::Allowed),
                styled: true,
                ..v
            },
        ),
    ]
}

fn flow_cell(ui: &mut egui::Ui, theme: &Theme, cap: &str, size: egui::Vec2, v: Variant) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        caption(ui, theme, cap);
        framed(ui, theme, size, |ui, inner| {
            remote_surface(ui, theme, inner, v, cap);
        });
    });
}

/// 툴팁을 연 표지 프레임. 버블은 위로 열린다.
fn tip_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    cap: &str,
    draw: impl FnOnce(&mut egui::Ui) -> (Option<egui::Rect>, &'static str),
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        caption(ui, theme, cap);
        ui.add_space(TIP_TOP.value());
        let (rect, key) = draw(ui);
        // 스크롤로 벗어나면 버블이 창 가장자리로 끌려오므로 보일 때만 그린다.
        if let Some(rect) = rect.filter(|r| ui.is_rect_visible(*r)) {
            Tooltip::new(t(key))
                .placement(TooltipPlacement::Top)
                .id_source(("html_web_page_tip", cap))
                .show(ui, theme, rect);
        }
    });
}

/// Spec — 페이지 흐름. 단계마다 고정 프레임으로 두고 페이지 테마를 따른다.
pub fn draw_flow(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.spacing_md.value(), theme.spacing_md.value());
                for (cap, v) in flow() {
                    spec::wrap_item(ui, |ui| {
                        flow_cell(
                            ui,
                            theme,
                            cap,
                            egui::vec2(FLOW_W.value(), FLOW_H.value()),
                            v,
                        );
                    });
                }
                let web = Variant {
                    web: true,
                    ..Variant::default()
                };
                for (cap, v, h) in [
                    (
                        "1 · narrow (360) — remote notice, action under the body",
                        Variant {
                            remote: Some(HtmlRemoteBannerState::Blocked),
                            ..web
                        },
                        NARROW_ONE_H,
                    ),
                    (
                        "2 · narrow (360) — script notice after the remote reload",
                        Variant {
                            script: Some(HtmlScriptBannerState::Blocked),
                            remote_marker: Some(HtmlRemoteMarkerKind::Allowed),
                            styled: true,
                            ..web
                        },
                        NARROW_TWO_H,
                    ),
                ] {
                    spec::wrap_item(ui, |ui| {
                        flow_cell(ui, theme, cap, egui::vec2(NARROW_W.value(), h.value()), v);
                    });
                }
                for (cap, kind) in [
                    (
                        "remote marker · allowed tooltip",
                        HtmlRemoteMarkerKind::Allowed,
                    ),
                    (
                        "remote marker · blocked tooltip",
                        HtmlRemoteMarkerKind::Blocked,
                    ),
                ] {
                    spec::wrap_item(ui, |ui| {
                        tip_cell(ui, theme, cap, |ui| {
                            let rect = web_band(ui, theme, cap, kind);
                            let key = match kind {
                                HtmlRemoteMarkerKind::Allowed => {
                                    "banner.html_remote.marker_allowed_web"
                                }
                                HtmlRemoteMarkerKind::Blocked => {
                                    "banner.html_remote.marker_blocked"
                                }
                            };
                            (rect, key)
                        });
                    });
                }
                for (cap, kind) in [
                    (
                        "script marker · allowed tooltip",
                        HtmlScriptMarkerKind::Allowed,
                    ),
                    (
                        "script marker · blocked tooltip",
                        HtmlScriptMarkerKind::Blocked,
                    ),
                ] {
                    spec::wrap_item(ui, |ui| {
                        tip_cell(ui, theme, cap, |ui| {
                            (web_tab(ui, theme, kind), script_marker_tip(kind))
                        });
                    });
                }
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("1", "remote notice blocked · no markers"),
            (
                "2",
                "remote reloading → commit → remote marker allowed + script notice blocked",
            ),
            (
                "3",
                "script reloading → commit → no notice · both markers allowed",
            ),
            (
                "4a",
                "reload before Allow → notice stays, loading · Allow disabled · Reload busy",
            ),
            (
                "4b",
                "reload after Allow → no notice · markers stay allowed · Reload busy",
            ),
            (
                "4",
                "commit of 4a or 4b → both allows cleared · remote notice blocked, even after ×",
            ),
            ("×", "marker blocked remains · click → that notice again"),
            ("loading", "Allow disabled until commit · Reload busy"),
        ],
        &[],
    );
    spec::note(
        ui,
        theme,
        "The notice first appears when the page commits, so a first load shows no loading frame. \
         A reload of the same page has two branches until it commits: before Allow the notice \
         stays in its loading state (4a), after Allow there is no notice and the markers stay \
         allowed (4b). The notices' loading tooltip is the shared action_loading copy and is not \
         drawn open here.",
    );
}
