//! HTML surface의 원격 콘텐츠 차단 안내와 주소 칸 끝 표지.
//! 배너·표지·주소 띠·크기 안내는 본체와 같은 `tasty_ui_widgets` 함수로 그리고,
//! 탭 스트립과 WebView 자리만 스크립트 안내 예제의 대역을 빌려 흉내 낸다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlAddressKind, HtmlAddressLabels, HtmlAddressState, HtmlAddressView, HtmlRemoteBannerState,
    HtmlRemoteBannerView, HtmlRemoteMarkerKind, HtmlRemoteMarkerView, HtmlScriptBannerState,
    HtmlScriptBannerView, HtmlScriptMarkerKind, html_address_bar, html_address_content_inset,
    html_remote_banner, html_script_banner, paint_dashed_outline,
};

use super::html_script_banner::{
    app_backdrop, caption, framed, mocha, page_stand_in, tab_strip, themed_card,
};
use crate::catalog::components::html_address::remote::size_card;
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 배너 상태 패널의 폭. 디자인은 `--tasty-size-560`을 쓴다.
const STATE_PANEL_W: LogicalPx = LogicalPx(560.0);
/// 표지 패널의 폭. 디자인은 `--tasty-size-460`을 쓴다.
const MARKER_PANEL_W: LogicalPx = LogicalPx(460.0);
/// 변형 그리드 한 칸의 폭. 디자인은 `--tasty-size-480`을 쓴다.
const VARIANT_W: LogicalPx = LogicalPx(480.0);
/// 변형 surface의 높이. 디자인 `HtmlRemoteSurfaceG`의 `height={300}`, 두 안내가 쌓이면 400이다.
const VARIANT_H: LogicalPx = LogicalPx(300.0);
const VARIANT_STACKED_H: LogicalPx = LogicalPx(400.0);
/// 좁은 surface의 폭(`--tasty-size-360`)과 9a·9b의 높이(340, 520).
const NARROW_W: LogicalPx = LogicalPx(360.0);
const NARROW_ONE_H: LogicalPx = LogicalPx(340.0);
const NARROW_TWO_H: LogicalPx = LogicalPx(520.0);
/// 세 안내를 쌓은 mirror 예제의 크기. 디자인은 `--tasty-size-560`과 `height={460}`을 쓴다.
const MIRROR_STACK_W: LogicalPx = LogicalPx(560.0);
const MIRROR_STACK_H: LogicalPx = LogicalPx(460.0);
/// 원격 CSS가 빠진 페이지 대역의 막대 폭 비율. 디자인 `RcUnstyledPage`의 40%·92%·75%다.
const UNSTYLED_BAR_FRACTIONS: [f32; 3] = [0.40, 0.92, 0.75];
/// 받지 못한 이미지 자리의 폭과 높이. 디자인은 `--tasty-size-64`와 `--tasty-size-40`을 쓴다.
const BROKEN_IMAGE_W: LogicalPx = LogicalPx(64.0);
const BROKEN_IMAGE_H: LogicalPx = LogicalPx(40.0);

/// 문안은 본체와 같은 번역 키에서 읽는다.
const TITLE: &str = "banner.html_remote.title";
const BODY: &str = "banner.html_remote.body";
const ACTION: &str = "banner.html_remote.action";
const RELOADING: &str = "banner.html_remote.reloading";
const ACTION_LOADING: &str = "banner.html_remote.action_loading";
const MARKER_BLOCKED: &str = "banner.html_remote.marker_blocked";
const MARKER_ALLOWED: &str = "banner.html_remote.marker_allowed";
const SCRIPT_MARKER_ALLOWED: &str = "banner.html_script.marker_allowed";
/// `http(s)` 페이지(web) 문구. 그 밖의 키는 로컬 문서와 공유한다.
const TITLE_WEB: &str = "banner.html_remote.title_web";
const BODY_WEB: &str = "banner.html_remote.body_web";
/// 전역 script sandbox가 꺼진 페이지의 원격 안내 본문. 네트워크 스크립트를 목록에 넣는다.
pub(super) const BODY_WEB_SANDBOX_OFF: &str = "banner.html_remote.body_web_sandbox_off";
const MARKER_ALLOWED_WEB: &str = "banner.html_remote.marker_allowed_web";
pub(super) const SCRIPT_MARKER_BLOCKED: &str = "banner.html_script.marker_blocked";
pub(super) const SCRIPT_MARKER_ALLOWED_WEB: &str = "banner.html_script.marker_allowed_web";

/// web 예제의 탭 라벨·주소·페이지 라벨. 디자인 `RC_WEB`.
pub(super) const WEB_LABEL: &str = "Getting started";
pub(super) const WEB_ADDRESS: &str = "https://docs.example.com/guide/getting-started";
const WEB_PAGE: &str = "docs.example.com";

const LOCAL_ADDRESS: &str = "~/reports/q3/report.html";
const MIRROR_ADDRESS: &str = "build-eu:~/reports/q3/report.html";

/// 원격 안내. `web`이면 `http(s)` 페이지 문구(제목·본문)를 쓴다.
pub(super) fn remote_view(
    state: HtmlRemoteBannerState,
    hover: bool,
    web: bool,
) -> HtmlRemoteBannerView<'static> {
    HtmlRemoteBannerView {
        title: t(if web { TITLE_WEB } else { TITLE }),
        body: t(if web { BODY_WEB } else { BODY }),
        action: t(ACTION),
        reloading: t(RELOADING),
        loading_tooltip: t(ACTION_LOADING),
        state,
        narrow: false,
        force_hover: hover,
    }
}

/// 스크립트 안내. 원격 안내가 함께 보이면 `body_remote`를 쓴다. `web`이면 페이지 문구를 쓰고
/// `body_remote`는 쓰지 않는다(페이지에서는 두 안내가 함께 보이지 않는다).
pub(super) fn script_view(
    state: HtmlScriptBannerState,
    with_remote: bool,
    web: bool,
) -> HtmlScriptBannerView<'static> {
    HtmlScriptBannerView {
        title: t(if web {
            "banner.html_script.title_web"
        } else {
            "banner.html_script.title"
        }),
        body: t(if web {
            "banner.html_script.body_web"
        } else if with_remote {
            "banner.html_script.body_remote"
        } else {
            "banner.html_script.body"
        }),
        action: t(if web {
            "banner.html_script.action_web"
        } else {
            "banner.html_script.action"
        }),
        reloading: t("banner.html_script.reloading"),
        loading_tooltip: t("banner.html_script.action_loading"),
        state,
        narrow: false,
        force_hover: false,
    }
}

/// 주소 칸 끝 표지. `web`이면 allowed 툴팁만 페이지 문구다.
pub(super) fn marker_view(
    kind: HtmlRemoteMarkerKind,
    hover: bool,
    web: bool,
) -> HtmlRemoteMarkerView<'static> {
    HtmlRemoteMarkerView {
        kind,
        tooltip: t(match kind {
            HtmlRemoteMarkerKind::Blocked => MARKER_BLOCKED,
            HtmlRemoteMarkerKind::Allowed if web => MARKER_ALLOWED_WEB,
            HtmlRemoteMarkerKind::Allowed => MARKER_ALLOWED,
        }),
        force_hover: hover,
        native: &[],
    }
}

pub(super) fn labels() -> HtmlAddressLabels<'static> {
    HtmlAddressLabels {
        placeholder: t("html_address.placeholder"),
        back: t("html_address.back"),
        forward: t("html_address.forward"),
        reload: t("html_address.reload"),
        disconnected: t("html_address.disconnected"),
    }
}

fn address_view(
    mirror: bool,
    skipped: Option<(&'static str, &'static str)>,
    marker: Option<HtmlRemoteMarkerView<'static>>,
) -> HtmlAddressView<'static> {
    let address = if mirror {
        MIRROR_ADDRESS
    } else {
        LOCAL_ADDRESS
    };
    HtmlAddressView {
        address,
        committed_address: address,
        kind: if mirror {
            HtmlAddressKind::Mirror
        } else {
            HtmlAddressKind::Local
        },
        can_back: true,
        can_forward: false,
        loading: false,
        disconnected: false,
        skipped,
        remote_marker: marker,
        focus: false,
        labels: labels(),
    }
}

/// `http(s)` 페이지의 주소 띠(kind `Web`). `loading`이면 새로고침이 진행 중으로 보인다.
pub(super) fn web_address_view(
    loading: bool,
    marker: Option<HtmlRemoteMarkerView<'static>>,
) -> HtmlAddressView<'static> {
    HtmlAddressView {
        address: WEB_ADDRESS,
        committed_address: WEB_ADDRESS,
        kind: HtmlAddressKind::Web,
        can_back: true,
        can_forward: false,
        loading,
        disconnected: false,
        skipped: None,
        remote_marker: marker,
        focus: false,
        labels: labels(),
    }
}

/// 주소 띠 하나를 `width` 폭으로 그린다. 예제라 편집 상태는 매 프레임 새로 만든다.
/// 원격 표지를 그렸으면 그 칸을 돌려준다.
pub(super) fn address_band(
    ui: &mut egui::Ui,
    theme: &Theme,
    salt: &str,
    width: LogicalPx,
    view: &HtmlAddressView,
) -> Option<egui::Rect> {
    let mut state = HtmlAddressState::default();
    html_address_bar(ui, theme, ui.id().with(salt), width, &mut state, view).remote_marker_rect
}

/// 원격 CSS·이미지가 빠진 페이지 자리(디자인 `RcUnstyledPage`). tasty chrome이 아닌 중립 대역이다.
fn unstyled_page(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, label: &str) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.surface("terminal").focused_bg.to_egui());
    let pad = theme.spacing_md.value();
    let gap = theme.spacing_sm.value();
    let inner_w = rect.width() - pad * 2.0;
    let label = painter.text(
        egui::pos2(rect.left() + pad, rect.top() + pad),
        egui::Align2::LEFT_TOP,
        format!("WebView — {label} · remote CSS + images not loaded"),
        egui::FontId::monospace(theme.font_size_micro.value()),
        theme.text_placeholder().to_egui(),
    );
    let bar = |y: f32, f: f32| {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.left() + pad, y),
            egui::vec2(inner_w * f, gap),
        );
        painter.rect_filled(
            r,
            theme.corner_radius_sm.value(),
            theme.surface_raised().to_egui(),
        );
        r.bottom() + gap
    };
    let mut y = label.bottom() + gap;
    y = bar(y, UNSTYLED_BAR_FRACTIONS[0]);
    y = bar(y, UNSTYLED_BAR_FRACTIONS[1]);
    // 받지 못한 이미지 두 칸: 점선 상자 가운데 image 글리프.
    let side = BROKEN_IMAGE_W.value();
    let box_h = BROKEN_IMAGE_H.value();
    let glyph = theme.icon_glyph_size_sm.value();
    for i in 0..2 {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.left() + pad + i as f32 * (side + gap), y),
            egui::vec2(side, box_h),
        );
        paint_dashed_outline(
            &painter,
            theme,
            r,
            theme.corner_radius_sm.value(),
            theme.border_default().to_egui(),
        );
        tasty_icons::IMAGE
            .image(glyph, theme.text_disabled().to_egui())
            .paint_at(
                ui,
                egui::Rect::from_center_size(r.center(), egui::vec2(glyph, glyph)),
            );
    }
    bar(y + box_h + gap, UNSTYLED_BAR_FRACTIONS[2]);
}

/// 변형 하나의 입력(디자인 `HtmlRemoteSurfaceG`의 prop).
#[derive(Clone, Copy, Default)]
pub(super) struct Variant {
    pub(super) remote: Option<HtmlRemoteBannerState>,
    pub(super) script: Option<HtmlScriptBannerState>,
    pub(super) remote_marker: Option<HtmlRemoteMarkerKind>,
    pub(super) script_marker: Option<HtmlScriptMarkerKind>,
    pub(super) size: bool,
    pub(super) styled: bool,
    pub(super) mirror: bool,
    /// `http(s)` 페이지: 주소 칸 kind `Web`, 두 안내와 allowed 툴팁의 페이지 문구.
    pub(super) web: bool,
    /// 로드가 commit 전이라 주소 띠의 새로고침이 진행 중으로 보인다.
    pub(super) loading: bool,
    /// 전역 script sandbox 꺼짐. web 원격 안내가 `body_web_sandbox_off` 본문을 쓴다.
    pub(super) sandbox_off: bool,
}

/// surface 하나: 탭 스트립 → 주소 띠(끝에 원격 표지) → inset 안내 묶음(크기 → 원격 → 스크립트) → 페이지.
pub(super) fn remote_surface(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    v: Variant,
    salt: &str,
) {
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let marker = v.script_marker.map(|k| {
        let tip = match k {
            HtmlScriptMarkerKind::Blocked if v.web => SCRIPT_MARKER_BLOCKED,
            HtmlScriptMarkerKind::Allowed if v.web => SCRIPT_MARKER_ALLOWED_WEB,
            _ => SCRIPT_MARKER_ALLOWED,
        };
        (k, t(tip))
    });
    let label = if v.web { WEB_LABEL } else { "report.html" };
    let scope = tab_strip(ui, theme, rect, label, icons::HTML, true, marker);
    let mut band = ui.new_child(
        egui::UiBuilder::new()
            .id_salt((salt, "band"))
            .max_rect(scope)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let marker = v.remote_marker.map(|k| marker_view(k, false, v.web));
    let view = if v.web {
        web_address_view(v.loading, marker)
    } else {
        address_view(v.mirror, None, marker)
    };
    let mut state = HtmlAddressState::default();
    let band_rect = html_address_bar(
        &mut band,
        theme,
        ui.id().with((salt, "address")),
        LogicalPx(scope.width()),
        &mut state,
        &view,
    )
    .rect;

    let narrow = rect.width() < theme.banner_narrow_below().value();
    let margin = theme.banner_margin().value();
    let mut top = band_rect.bottom() + margin;
    let mut heights = Vec::new();
    let mut place = |ui: &mut egui::Ui, i: usize, draw: &dyn Fn(&mut egui::Ui) -> egui::Rect| {
        let area = egui::Rect::from_min_max(
            egui::pos2(rect.left() + margin, top),
            egui::pos2(rect.right() - margin, rect.bottom()),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt((salt, i)).max_rect(area));
        child.set_clip_rect(rect);
        let card = draw(&mut child);
        heights.push(LogicalPx(card.height()));
        top = card.bottom() + theme.banner_inset_gap().value();
    };
    if v.size {
        place(ui, 0, &|ui| size_card(ui, theme, narrow, false, false));
    }
    if let Some(state) = v.remote {
        place(ui, 1, &|ui| {
            let mut view = remote_view(state, false, v.web);
            if v.web && v.sandbox_off {
                view.body = t(BODY_WEB_SANDBOX_OFF);
            }
            view.narrow = narrow;
            html_remote_banner(ui, theme, &view).rect
        });
    }
    if let Some(state) = v.script {
        let with_remote = v.remote.is_some();
        place(ui, 2, &|ui| {
            let mut view = script_view(state, with_remote, v.web);
            view.narrow = narrow;
            html_script_banner(ui, theme, &view).rect
        });
    }
    let inset = html_address_content_inset(theme, LogicalPx(band_rect.height()), &heights);
    let page = egui::Rect::from_min_max(
        egui::pos2(rect.left(), (LogicalPx(scope.top()) + inset).value()),
        rect.max,
    );
    let page_label = if v.web { WEB_PAGE } else { "report.html" };
    if v.styled {
        page_stand_in(ui, theme, page, page_label);
    } else {
        unstyled_page(ui, theme, page, page_label);
    }
}

/// Spec — 배너 상태. 테마마다 blocked·hover·loading·reloading을 세로로 보인다.
pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
                for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                    spec::wrap_item(ui, |ui| {
                        themed_card(ui, th, |ui| {
                            ui.set_width(STATE_PANEL_W.value());
                            for (cap, state, hover) in [
                                ("blocked", HtmlRemoteBannerState::Blocked, false),
                                ("hover — × revealed", HtmlRemoteBannerState::Blocked, true),
                                (
                                    "loading — Allow disabled until commit",
                                    HtmlRemoteBannerState::Loading,
                                    false,
                                ),
                                (
                                    "reloading — spinner, no ×",
                                    HtmlRemoteBannerState::Reloading,
                                    false,
                                ),
                            ] {
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                                    caption(ui, th, &format!("{name} · {cap}"));
                                    html_remote_banner(ui, th, &remote_view(state, hover, false));
                                });
                            }
                        });
                    });
                }
            });
        });
    });
    let title_spec = format!("\"{}\" · 13/600 · wraps", t(TITLE));
    spec::meta(
        ui,
        theme,
        &[
            (
                "glyph",
                "globeOff 16 · html-remote-banner-glyph → accent-info",
            ),
            ("title", title_spec.as_str()),
            (
                "body",
                "kinds, no counts · 11 · banner-body-fg · wraps · scope and lifetime last",
            ),
            ("action", "Secondary / Sm · no wrap"),
            (
                "loading",
                "disabled from load start to commit · tooltip top · × kept",
            ),
            (
                "reloading",
                "Spinner 14 + label · title and body keep their ink · no × · fades out on commit",
            ),
            (
                "×",
                "hover only · hides the notice for this document, block stays",
            ),
            ("focus", "never taken · click only"),
        ],
        &[
            TokenChip::new(
                "html-remote-banner-glyph",
                "globeOff → accent-info",
                theme.html_remote_banner_glyph().to_egui(),
            ),
            TokenChip::new("banner-bg", "shell", theme.banner_bg().to_egui()),
            TokenChip::new(
                "banner-button-bg",
                "Allow",
                theme.banner_button_bg().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Same shell, life rules and state names as the script notice; told apart by the globeOff \
         glyph. The action label differs from the script notice so two stacked notices never show \
         the same button.",
    );
}

/// Spec — 주소 칸 끝 표지. 테마마다 blocked·hover·allowed와 mirror의 Tag 뒤 표지를 보인다.
pub fn draw_markers(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing =
                    egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
                for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                    spec::wrap_item(ui, |ui| {
                        themed_card(ui, th, |ui| {
                            ui.set_width(MARKER_PANEL_W.value());
                            let cases = [
                                (
                                    "blocked",
                                    address_view(
                                        false,
                                        None,
                                        Some(marker_view(
                                            HtmlRemoteMarkerKind::Blocked,
                                            false,
                                            false,
                                        )),
                                    ),
                                ),
                                (
                                    "blocked · hover",
                                    address_view(
                                        false,
                                        None,
                                        Some(marker_view(
                                            HtmlRemoteMarkerKind::Blocked,
                                            true,
                                            false,
                                        )),
                                    ),
                                ),
                                (
                                    "allowed",
                                    address_view(
                                        false,
                                        None,
                                        Some(marker_view(
                                            HtmlRemoteMarkerKind::Allowed,
                                            false,
                                            false,
                                        )),
                                    ),
                                ),
                                (
                                    "mirror · after a declined large file — Tag, then marker",
                                    address_view(
                                        true,
                                        Some((
                                            "2 skipped",
                                            "Declined: hero-video.mp4, fonts/Inter.woff2",
                                        )),
                                        Some(marker_view(
                                            HtmlRemoteMarkerKind::Blocked,
                                            false,
                                            false,
                                        )),
                                    ),
                                ),
                            ];
                            for (i, (cap, view)) in cases.into_iter().enumerate() {
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                                    caption(ui, th, &format!("{name} · {cap}"));
                                    let h = th.html_address_bar_height().value();
                                    framed(
                                        ui,
                                        th,
                                        egui::vec2(MARKER_PANEL_W.value(), h),
                                        |ui, inner| {
                                            address_band(
                                                ui,
                                                th,
                                                &format!("{name}-{i}"),
                                                LogicalPx(inner.width()),
                                                &view,
                                            );
                                        },
                                    );
                                });
                            }
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
            (
                "slot",
                "address field end · after skipped Tag · the address ellipsises first",
            ),
            ("cell", "20 (html-remote-marker-hit) · glyph 14 · radius-sm"),
            (
                "blocked",
                "globeOff · glyph-dim · hover fill · click → notice",
            ),
            ("allowed", "globe · text-muted · tooltip only"),
            ("shown when", "notice not on screen · never with the notice"),
            (
                "with script marker",
                "independent places: script = tab cluster, remote = address field",
            ),
            (
                "cleared by",
                "another document committing · failed load · app restart",
            ),
        ],
        &[
            TokenChip::new(
                "html-remote-marker-fg",
                "→ glyph-dim",
                theme.html_remote_marker_fg().to_egui(),
            ),
            TokenChip::new(
                "html-remote-marker-allowed-fg",
                "→ text-muted",
                theme.html_remote_marker_allowed_fg().to_egui(),
            ),
            TokenChip::without_color("html-remote-marker-size", "→ icon-size-sm 14"),
            TokenChip::without_color("html-remote-marker-hit", "→ size-20"),
            TokenChip::new(
                "html-remote-marker-hover-bg",
                "→ overlay-hover",
                theme.html_remote_marker_hover_bg().to_egui_premultiplied(),
            ),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't make the blocked marker allow directly. It re-shows the notice, which explains \
         scope before offering Allow — the same rule as the script lock.",
    );
    spec::note(
        ui,
        theme,
        "Tasty draws the marker only for local documents: a mirror view never opens remote \
         content per document, so the host shows neither notice nor marker there. The mirror row \
         above is the design's field composition specimen (Tag, then marker).",
    );
}

/// 디자인 변형 1–10. 캡션과 prop은 디자인 `RC_VARIANTS`를 따른다.
fn variants() -> Vec<(&'static str, Variant)> {
    use HtmlRemoteBannerState as R;
    use HtmlRemoteMarkerKind as M;
    use HtmlScriptBannerState as S;
    let v = Variant::default();
    vec![
        (
            "1 · remote CSS + images, no scripts — blocked",
            Variant {
                remote: Some(R::Blocked),
                ..v
            },
        ),
        (
            "2 · loading a new document — Allow disabled (tooltip top)",
            Variant {
                remote: Some(R::Loading),
                ..v
            },
        ),
        (
            "3 · after Allow — reloading",
            Variant {
                remote: Some(R::Reloading),
                ..v
            },
        ),
        (
            "4 · allowed — no banner, globe marker in the address field",
            Variant {
                remote_marker: Some(M::Allowed),
                styled: true,
                ..v
            },
        ),
        (
            "5 · × dismissed — still blocked, globeOff marker (click → banner)",
            Variant {
                remote_marker: Some(M::Blocked),
                ..v
            },
        ),
        (
            "6a · scripts + remote — both notices, remote first",
            Variant {
                remote: Some(R::Blocked),
                script: Some(S::Blocked),
                ..v
            },
        ),
        (
            "6b · scripts allowed only — remote notice stays",
            Variant {
                remote: Some(R::Blocked),
                script_marker: Some(HtmlScriptMarkerKind::Allowed),
                ..v
            },
        ),
        (
            "6c · remote allowed only — script notice stays",
            Variant {
                script: Some(S::Blocked),
                remote_marker: Some(M::Allowed),
                styled: true,
                ..v
            },
        ),
        (
            "7 · only network-src scripts — remote notice only (nothing runnable yet)",
            Variant {
                remote: Some(R::Blocked),
                ..v
            },
        ),
        (
            "7′ · …after remote Allow — scripts now runnable, script notice (plain body)",
            Variant {
                script: Some(S::Blocked),
                remote_marker: Some(M::Allowed),
                styled: true,
                ..v
            },
        ),
        (
            "8a · missed by the scan, block OBSERVED (Windows) — marker only",
            Variant {
                remote_marker: Some(M::Blocked),
                ..v
            },
        ),
        (
            "8b · missed by the scan, no OS signal (macOS) — nothing to show",
            v,
        ),
        (
            "10 · global “Allow remote content” on — no notice, no marker",
            Variant { styled: true, ..v },
        ),
    ]
}

fn variant_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    cap: &str,
    size: egui::Vec2,
    v: Variant,
    salt: &str,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        caption(ui, theme, cap);
        framed(ui, theme, size, |ui, inner| {
            remote_surface(ui, theme, inner, v, salt);
        });
    });
}

/// Spec — surface 변형 1–10과 스크립트·크기 안내와의 쌓기 순서.
pub fn draw_variants(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
            for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                themed_card(ui, th, |ui| {
                    caption(ui, th, name);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing =
                            egui::vec2(th.spacing_md.value(), th.spacing_md.value());
                        for (cap, v) in variants() {
                            let h = if v.remote.is_some() && v.script.is_some() {
                                VARIANT_STACKED_H
                            } else {
                                VARIANT_H
                            };
                            spec::wrap_item(ui, |ui| {
                                variant_cell(
                                    ui,
                                    th,
                                    cap,
                                    egui::vec2(VARIANT_W.value(), h.value()),
                                    v,
                                    &format!("{name}-{cap}"),
                                );
                            });
                        }
                        for (cap, v, h) in [
                            (
                                "9a · narrow (360) — variant 1",
                                Variant {
                                    remote: Some(HtmlRemoteBannerState::Blocked),
                                    ..Variant::default()
                                },
                                NARROW_ONE_H,
                            ),
                            (
                                "9b · narrow (360) — variant 6a",
                                Variant {
                                    remote: Some(HtmlRemoteBannerState::Blocked),
                                    script: Some(HtmlScriptBannerState::Blocked),
                                    ..Variant::default()
                                },
                                NARROW_TWO_H,
                            ),
                        ] {
                            spec::wrap_item(ui, |ui| {
                                variant_cell(
                                    ui,
                                    th,
                                    cap,
                                    egui::vec2(NARROW_W.value(), h.value()),
                                    v,
                                    &format!("{name}-{cap}"),
                                );
                            });
                        }
                    });
                });
            }
            variant_cell(
                ui,
                theme,
                "mirror · all three notices — size → remote → script (design specimen)",
                egui::vec2(MIRROR_STACK_W.value(), MIRROR_STACK_H.value()),
                Variant {
                    size: true,
                    remote: Some(HtmlRemoteBannerState::Blocked),
                    script: Some(HtmlScriptBannerState::Blocked),
                    mirror: true,
                    ..Variant::default()
                },
                "mirror-stack",
            );
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "order",
                "address band → size (mirror) → remote → script → WebView",
            ),
            (
                "gaps",
                "banner-margin 8 top/sides · banner-inset-gap 8 between + below",
            ),
            ("merge", "never — two cards, two independent allows"),
            (
                "script notice",
                "judged on runnable scripts (network scripts need remote allowed)",
            ),
            ("body_remote", "shown only alongside the remote notice"),
            ("static ref", "notice (when the user views)"),
            ("observed block, no ref", "marker only (Windows)"),
            ("no signal, no ref", "nothing (macOS)"),
            (
                "narrow",
                "< 440 → each card wraps its action, stack order unchanged",
            ),
            ("global on", "no notice · no marker"),
        ],
        &[
            TokenChip::without_color("banner-inset-gap", "8 between / below"),
            TokenChip::without_color("banner-margin", "8 top / sides"),
            TokenChip::without_color("banner-narrow-below", "→ size-440"),
        ],
    );
}
