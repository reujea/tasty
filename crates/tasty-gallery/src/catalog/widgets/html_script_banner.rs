//! HTML surface의 스크립트 차단 안내 — 배너 시스템의 첫 inset 배치 예제.
//! 배너·마커는 본체와 같은 `tasty_ui_widgets` 함수로 그리고, 탭 스트립과 WebView 자리만 여기서 흉내 낸다.
//! 상태를 나란히 보여 주는 정적 예제라 재로드 완료 뒤 페이드와 실제 WebView는 재현하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlScriptBannerState, HtmlScriptBannerView, HtmlScriptMarkerKind, Tooltip, html_script_banner,
    html_script_banner_is_narrow, html_script_marker, inset_banner_zone, inset_content_rect,
};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 디자인 `HtmlSurfaceG`·`TermSurfaceG`의 기본 높이. 두 surface를 나란히 보이는 전시 공간이다.
const SURFACE_STAGE_H: LogicalPx = LogicalPx(260.0);
/// 좁은 surface 예제의 폭. 디자인은 `--tasty-size-360`을 쓰며 `banner-narrow-below`(440)보다 좁다.
const NARROW_SURFACE_W: LogicalPx = LogicalPx(360.0);
/// 좁은 surface 예제의 높이. 두 줄로 늘어난 배너 아래에도 페이지 자리가 남도록 디자인이 정했다.
const NARROW_SURFACE_H: LogicalPx = LogicalPx(300.0);
/// 로드 실패 예제 surface의 높이. 디자인 `HtmlSurfaceG`의 `height={220}`이다.
const FAILED_SURFACE_H: LogicalPx = LogicalPx(220.0);
/// 마커 예제 surface의 폭. 디자인은 `--tasty-size-240`을 쓴다.
const MARKER_SURFACE_W: LogicalPx = LogicalPx(240.0);
/// 마커 예제 surface의 높이. 디자인 `HtmlSurfaceG`의 `height={120}`이며 배너가 없어 탭 스트립과 페이지 윗부분만 보인다.
const MARKER_SURFACE_H: LogicalPx = LogicalPx(120.0);
/// 탭 스트립 툴팁 예제 창의 폭. 디자인은 `--tasty-size-320`을 쓴다.
const STRIP_TOOLTIP_W: LogicalPx = LogicalPx(320.0);
/// 탭 스트립 툴팁 예제의 WebView 자리 높이. 디자인은 `--tasty-size-96`을 쓴다.
const STRIP_TOOLTIP_WEBVIEW_H: LogicalPx = LogicalPx(96.0);
/// html pane을 쌓은 툴팁 예제의 위·아래 WebView 자리 높이. 디자인은 `--tasty-size-64`를 쓴다.
const STRIP_TOOLTIP_STACKED_WEBVIEW_H: LogicalPx = LogicalPx(64.0);
/// 탭 스트립 툴팁 Stage의 오른쪽 여백. 디자인은 `--tasty-size-120`을 쓰며, 예제 창 밖으로 나간 버블이 이 안에 든다.
const STRIP_TOOLTIP_STAGE_PAD_RIGHT: LogicalPx = LogicalPx(120.0);
/// 상태 예제 한 장의 최대 폭. 디자인은 `--tasty-size-600`을 쓴다.
const STATE_CARD_MAX_W: LogicalPx = LogicalPx(600.0);
/// 배너 버튼 예제의 테마별 패널 폭. 디자인은 `--tasty-size-560`을 쓴다.
const BUTTON_PANEL_W: LogicalPx = LogicalPx(560.0);
/// 페이지 자리의 가짜 본문 막대 폭 비율. 디자인 `HsPage`의 60%·85%·70%다.
const PAGE_BAR_FRACTIONS: [f32; 3] = [0.60, 0.85, 0.70];

/// 문안은 본체와 같은 번역 키에서 읽는다. 사본을 두면 확정 문안과 어긋날 수 있다.
const TITLE: &str = "banner.html_script.title";
const BODY: &str = "banner.html_script.body";
const BODY_REMOTE: &str = "banner.html_script.body_remote";
const ACTION: &str = "banner.html_script.action";
const RELOADING: &str = "banner.html_script.reloading";
const ACTION_LOADING: &str = "banner.html_script.action_loading";
const MARKER_BLOCKED: &str = "banner.html_script.marker_blocked";
const MARKER_ALLOWED: &str = "banner.html_script.marker_allowed";
const LOAD_FAILED: &str = "webview.error";
/// 실패 예제의 URL. 디자인 `HsFailed`의 기본값이다.
const FAILED_URL: &str = "file:///Users/me/report.html";

fn view(state: HtmlScriptBannerState, remote: bool, hover: bool) -> HtmlScriptBannerView<'static> {
    HtmlScriptBannerView {
        title: t(TITLE),
        body: t(if remote { BODY_REMOTE } else { BODY }),
        action: t(ACTION),
        reloading: t(RELOADING),
        loading_tooltip: t(ACTION_LOADING),
        state,
        narrow: false,
        force_hover: hover,
    }
}

pub(crate) fn mocha() -> Theme {
    tasty_themes::mocha_fallback()
}

/// 디자인 Stage의 `bg-app` 바탕과 `space-lg` 여백.
pub(crate) fn app_backdrop(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.bg_app().to_egui())
        .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

pub(crate) fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 스트립 왼쪽 끝에 탭 하나(디자인 `HsTab`)를 그린다.
/// 라벨 뒤 오른쪽 끝에 고정 cluster [마커 · 닫기 슬롯]을 두고, 라벨이 먼저 말줄임된다.
pub(crate) fn tab(
    ui: &mut egui::Ui,
    theme: &Theme,
    strip: egui::Rect,
    label: &str,
    glyph: MockGlyph,
    active: bool,
    marker: Option<(HtmlScriptMarkerKind, &str)>,
) -> Option<egui::Rect> {
    let w = theme.tab_width.value().min(strip.width());
    let rect = egui::Rect::from_min_size(strip.min, egui::vec2(w, strip.height()));
    let fg = if active {
        theme.text_primary()
    } else {
        theme.text_muted()
    }
    .to_egui();
    let painter = ui.painter().clone();
    if active {
        painter.rect_filled(rect, 0.0, theme.bg_panel().to_egui());
        let bar = theme.tab_indicator_width().value();
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - bar), rect.max),
            0.0,
            theme.accent_primary().to_egui(),
        );
    }
    let cy = rect.center().y;
    let status_gap = theme.tab_status_gap().value();
    let close = theme.tab_close_size().value();
    let hit = theme.html_script_marker_hit().value();

    // cluster는 오른쪽 끝에서 왼쪽으로 쌓는다: 닫기 슬롯, 그 왼쪽에 마커.
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.right() - theme.spacing_xs.value() - close,
            cy - close / 2.0,
        ),
        egui::vec2(close, close),
    );
    if active {
        let x = theme.icon_glyph_size_xs.value();
        tasty_icons::CLOSE
            .image(x, theme.text_muted().to_egui())
            .paint_at(
                ui,
                egui::Rect::from_center_size(close_rect.center(), egui::vec2(x, x)),
            );
    }
    let mut cluster_left = close_rect.left();
    let mut marker_rect = None;
    if let Some((kind, tip)) = marker {
        let m = egui::Rect::from_min_size(
            egui::pos2(cluster_left - status_gap - hit, cy - hit / 2.0),
            egui::vec2(hit, hit),
        );
        let mut mui = ui.new_child(egui::UiBuilder::new().max_rect(m));
        html_script_marker(&mut mui, theme, kind, tip, Some(rect), &[]);
        cluster_left = m.left();
        marker_rect = Some(m);
    }

    let icon = theme.tab_icon_size().value();
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + theme.tab_padding_x().value(), cy - icon / 2.0),
        egui::vec2(icon, icon),
    );
    glyph.image(icon, fg).paint_at(ui, icon_rect);
    let gap = theme.tab_gap().value();
    let label_x = icon_rect.right() + gap;
    let label_w = (cluster_left - gap - label_x).max(0.0);
    let mut job = egui::text::LayoutJob::single_section(
        label.to_owned(),
        egui::TextFormat::simple(
            egui::FontId::proportional(theme.font_size_caption.value()),
            fg,
        ),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(label_w);
    let galley = ui.fonts(|f| f.layout_job(job));
    painter.galley(egui::pos2(label_x, cy - galley.size().y / 2.0), galley, fg);
    painter.vline(
        rect.right(),
        rect.y_range(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    marker_rect
}

/// 탭 스트립을 그리고 그 아래 콘텐츠 rect를 돌려준다.
pub(crate) fn tab_strip(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    label: &str,
    glyph: MockGlyph,
    active: bool,
    marker: Option<(HtmlScriptMarkerKind, &str)>,
) -> egui::Rect {
    let bw = theme.border_width.value();
    let strip = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width(), theme.tab_height().value()),
    );
    ui.painter()
        .rect_filled(strip, 0.0, theme.bg_sidebar().to_egui());
    tab(ui, theme, strip, label, glyph, active, marker);
    ui.painter().hline(
        strip.x_range(),
        strip.bottom() + bw / 2.0,
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );
    egui::Rect::from_min_max(egui::pos2(rect.left(), strip.bottom() + bw), rect.max)
}

/// WebView가 그릴 페이지 자리(디자인 `HsPage`). tasty chrome이 아닌 중립 대역이다.
pub(crate) fn page_stand_in(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, label: &str) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.surface("terminal").focused_bg.to_egui());
    let pad = theme.spacing_md.value();
    let gap = theme.spacing_sm.value();
    let inner_w = rect.width() - pad * 2.0;
    let label = painter.text(
        egui::pos2(rect.left() + pad, rect.top() + pad),
        egui::Align2::LEFT_TOP,
        format!("WebView — {label}"),
        egui::FontId::monospace(theme.font_size_micro.value()),
        theme.text_placeholder().to_egui(),
    );
    let mut y = label.bottom() + gap;
    let heights = [
        theme.spacing_md.value(),
        theme.spacing_sm.value(),
        theme.spacing_sm.value(),
    ];
    for (h, f) in heights.into_iter().zip(PAGE_BAR_FRACTIONS) {
        let bar =
            egui::Rect::from_min_size(egui::pos2(rect.left() + pad, y), egui::vec2(inner_w * f, h));
        painter.rect_filled(
            bar,
            theme.corner_radius_sm.value(),
            theme.surface_raised().to_egui(),
        );
        y = bar.bottom() + gap;
    }
}

/// 로드 실패 상태(디자인 `HsFailed`). 본체 webview chrome의 실패 상태와 같은 배치다.
fn failed_stand_in(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let pad = theme.spacing_md.value();
    let gap = theme.spacing_sm.value();
    let inner = rect.shrink(pad);
    let glyph = theme.icon_glyph_size_md.value();
    let painter = ui.painter_at(rect);
    let title = painter.layout_no_wrap(
        t(LOAD_FAILED).to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.accent_danger().to_egui(),
    );
    let mut url_job = egui::text::LayoutJob::simple_singleline(
        FAILED_URL.to_owned(),
        egui::FontId::monospace(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
    url_job.wrap.max_width = inner.width();
    url_job.wrap.max_rows = 1;
    url_job.wrap.break_anywhere = true;
    let url = ui.fonts(|f| f.layout_job(url_job));
    let block_h = glyph + gap + title.size().y + gap + url.size().y;
    let mut y = inner.center().y - block_h / 2.0;
    let cx = inner.center().x;
    tasty_icons::ALERT_CIRCLE
        .image(glyph, theme.accent_danger_raised().to_egui())
        .paint_at(
            ui,
            egui::Rect::from_min_size(egui::pos2(cx - glyph / 2.0, y), egui::vec2(glyph, glyph)),
        );
    y += glyph + gap;
    let title_w = title.size().x;
    let title_h = title.size().y;
    painter.galley(
        egui::pos2(cx - title_w / 2.0, y),
        title,
        theme.accent_danger().to_egui(),
    );
    y += title_h + gap;
    let url_w = url.size().x;
    painter.galley(
        egui::pos2(cx - url_w / 2.0, y),
        url,
        theme.text_muted().to_egui(),
    );
}

/// HTML surface 하나(디자인 `HtmlSurfaceG`): 탭 스트립 → inset 배너 → 줄어든 WebView 자리.
/// `failed`면 페이지 자리에 로드 실패 상태를 그린다.
fn html_surface(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    banner: Option<HtmlScriptBannerView<'_>>,
    marker: Option<(HtmlScriptMarkerKind, &str)>,
    failed: bool,
) {
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let scope = tab_strip(ui, theme, rect, "report.html", icons::HTML, true, marker);
    let page = match banner {
        Some(mut v) => {
            v.narrow = html_script_banner_is_narrow(rect.width(), theme);
            let mut child =
                ui.new_child(egui::UiBuilder::new().max_rect(inset_banner_zone(scope, theme)));
            child.set_clip_rect(rect);
            let out = html_script_banner(&mut child, theme, &v);
            inset_content_rect(scope, out.rect, theme)
        }
        None => scope,
    };
    if failed {
        failed_stand_in(ui, theme, page);
    } else {
        page_stand_in(ui, theme, page, "report.html");
    }
}

/// 옆 터미널 surface(디자인 `TermSurfaceG`). 배너 스코프가 surface라 여기에는 배너가 없다.
fn term_surface(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let term = theme.surface("terminal");
    ui.painter()
        .rect_filled(rect, 0.0, term.unfocused_bg.to_egui());
    let body = tab_strip(ui, theme, rect, "zsh", icons::TERMINAL, false, None);
    let pad = theme.spacing_md.value();
    ui.painter_at(body).text(
        body.min + egui::vec2(pad, pad),
        egui::Align2::LEFT_TOP,
        "~/tasty $ open report.html",
        egui::FontId::monospace(theme.font_size_term_sm.value()),
        term.unfocused_fg.to_egui(),
    );
}

/// `border-frame` 1px 테두리와 모서리로 감싼 액자 안에 `paint`를 그린다.
pub(crate) fn framed(
    ui: &mut egui::Ui,
    theme: &Theme,
    size: egui::Vec2,
    paint: impl FnOnce(&mut egui::Ui, egui::Rect),
) {
    let bw = theme.border_width.value();
    let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let frame = theme.border_frame().to_egui();
    let radius = theme.corner_radius.value();
    ui.painter().rect_filled(outer, radius, frame);
    let inner = outer.shrink(bw);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner);
    paint(&mut child, inner);
}

/// Spec 1 — inset 배치. 두 surface를 나란히 두고 HTML 쪽에만 배너를 그린다.
pub fn draw_placement(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
            for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                    caption(
                        ui,
                        th,
                        &format!("{name} · two surfaces side by side, banner on the HTML one only"),
                    );
                    let w = ui.available_width();
                    framed(
                        ui,
                        th,
                        egui::vec2(w, SURFACE_STAGE_H.value()),
                        |ui, inner| {
                            let bw = th.border_width.value();
                            let half = (inner.width() - bw) / 2.0;
                            let left = egui::Rect::from_min_size(
                                inner.min,
                                egui::vec2(half, inner.height()),
                            );
                            let right = egui::Rect::from_min_max(
                                egui::pos2(left.right() + bw, inner.top()),
                                inner.max,
                            );
                            html_surface(
                                ui,
                                th,
                                left,
                                Some(view(HtmlScriptBannerState::Blocked, false, false)),
                                None,
                                false,
                            );
                            term_surface(ui, th, right);
                        },
                    );
                });
            }
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("approach", "a — egui banner + WebView rect inset"),
            ("placement", "inset (banner variant) · floating unchanged"),
            ("margin", "8 top / sides / bottom"),
            ("scope", "the HTML surface only"),
            ("focus", "never takes it · clicks don't reach the page"),
            ("TTL", "none — stays until allow or ×"),
        ],
        &[
            TokenChip::without_color("banner-inset-gap", "8 — banner → page (new)"),
            TokenChip::without_color("banner-margin", "8 top / sides"),
            TokenChip::new("banner-bg", "shell", theme.banner_bg().to_egui()),
            TokenChip::new("banner-border", "edge", theme.banner_border().to_egui()),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't inject the notice into the document DOM (approach c): the page could hide or forge it, \
         and its button can't run with scripts off. Don't build three native overlays (approach b) for one notice.",
    );
}

/// Spec 2 — 배너 상태. 기본·hover·재로드·로드 중·원격 본문 분기와 좁은 surface.
pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    let cases: [(&str, HtmlScriptBannerView<'static>); 5] = [
        (
            "1 · blocked (default)",
            view(HtmlScriptBannerState::Blocked, false, false),
        ),
        (
            "2 · hover — × revealed",
            view(HtmlScriptBannerState::Blocked, false, true),
        ),
        (
            "3 · after Allow — reloading",
            view(HtmlScriptBannerState::Reloading, false, false),
        ),
        (
            "4 · surface loading a new document — Allow disabled until commit (tooltip on hover)",
            view(HtmlScriptBannerState::Loading, false, false),
        ),
        (
            "remote content also blocked — body branch",
            view(HtmlScriptBannerState::Blocked, true, false),
        ),
    ];
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            for (cap, v) in cases {
                ui.vertical(|ui| {
                    ui.set_max_width(STATE_CARD_MAX_W.value());
                    ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                    caption(ui, theme, cap);
                    html_script_banner(ui, theme, &v);
                });
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                caption(
                    ui,
                    theme,
                    "6 · narrow surface (360) — action wraps under the body",
                );
                framed(
                    ui,
                    theme,
                    egui::vec2(NARROW_SURFACE_W.value(), NARROW_SURFACE_H.value()),
                    |ui, inner| {
                        html_surface(
                            ui,
                            theme,
                            inner,
                            Some(view(HtmlScriptBannerState::Blocked, false, false)),
                            None,
                            false,
                        );
                    },
                );
            });
        });
    });

    let title_spec = format!("\"{}\" · 13/600", t(TITLE));
    spec::meta(
        ui,
        theme,
        &[
            ("title", title_spec.as_str()),
            ("body", "caption · banner-body-fg · wraps, never clamps"),
            ("remote branch", "network scripts stay blocked — says so"),
            ("action", "Secondary / Sm · no wrap"),
            ("reloading", "spinner + label · no × · fade out on commit"),
            (
                "loading (still blocked)",
                "Allow disabled from load start to commit · no delay · tooltip (top) \"Available when loading finishes\" · × kept",
            ),
            (
                "narrow",
                "surface width < 440 (banner-narrow-below) → action on its own line, body-aligned",
            ),
            ("glyph nudge", "1 · banner-glyph-offset"),
            ("title ↔ body", "2 · banner-text-gap (every banner)"),
        ],
        &[
            TokenChip::new(
                "html-script-banner-glyph",
                "lock → accent-info",
                theme.html_script_banner_glyph().to_egui(),
            ),
            TokenChip::without_color("banner-title-font-size", "13"),
            TokenChip::without_color("banner-body-font-size", "11"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Informational, not a warning: blocking is the default working as intended. One action, \
         no second one (the global switch lives in Settings › Appearance › HTML). After Allow the \
         document reloads once and the banner fades out (120ms) when the new load commits — the \
         fade is not reproduced in this static specimen.",
    );
}

/// Spec 3 — 배너 셸 위 Secondary 버튼. 모든 배너 공통 규칙을 HTML 스크립트 배너로 Mocha·Latte에 보인다.
pub fn draw_banner_button(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
            for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                egui::Frame::new()
                    .fill(th.bg_app().to_egui())
                    .stroke(egui::Stroke::new(
                        th.border_width.value(),
                        th.border_default().to_egui(),
                    ))
                    .corner_radius(th.corner_radius.value())
                    .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                    .show(ui, |ui| {
                        let inner = BUTTON_PANEL_W.value() - 2.0 * th.spacing_md.value();
                        ui.set_width(inner);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                            caption(ui, th, &format!("{name} · blocked, hover the button"));
                            html_script_banner(
                                ui,
                                th,
                                &view(HtmlScriptBannerState::Blocked, false, false),
                            );
                            caption(ui, th, &format!("{name} · loading — disabled on the shell"));
                            html_script_banner(
                                ui,
                                th,
                                &view(HtmlScriptBannerState::Loading, false, false),
                            );
                        });
                    });
            }
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("where", "Secondary inside any banner shell"),
            ("fill", "surface-hover (one step above banner-bg)"),
            ("edge", "border-frame (one step above banner-border)"),
            ("hover", "overlay-hover · edge unchanged"),
            ("variant", "none new — context rule in Button"),
            (
                "plugin",
                "same box — the SDK opens the context for the whole banner frame, never for the plugin's surfaces or popups",
            ),
        ],
        &[
            TokenChip::new(
                "banner-button-bg",
                "→ surface-hover",
                theme.banner_button_bg().to_egui(),
            ),
            TokenChip::new(
                "banner-button-border",
                "→ border-frame",
                theme.banner_button_border().to_egui(),
            ),
            TokenChip::new("banner-bg", "shell", theme.banner_bg().to_egui()),
        ],
    );
}

/// Spec 4 — 닫은 뒤와 허용한 뒤의 탭 스트립 마커.
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
                        egui::Frame::new()
                            .fill(th.bg_app().to_egui())
                            .stroke(egui::Stroke::new(
                                th.border_width.value(),
                                th.border_default().to_egui(),
                            ))
                            .corner_radius(th.corner_radius.value())
                            .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = th.spacing_md.value();
                                    for (cap, kind, tip) in [
                                        (
                                            "4a · dismissed — lock, click to re-show",
                                            HtmlScriptMarkerKind::Blocked,
                                            t(MARKER_BLOCKED),
                                        ),
                                        (
                                            "4b · allowed this session",
                                            HtmlScriptMarkerKind::Allowed,
                                            t(MARKER_ALLOWED),
                                        ),
                                    ] {
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                                            caption(ui, th, &format!("{name} · {cap}"));
                                            framed(
                                                ui,
                                                th,
                                                egui::vec2(
                                                    MARKER_SURFACE_W.value(),
                                                    MARKER_SURFACE_H.value(),
                                                ),
                                                |ui, inner| {
                                                    html_surface(
                                                        ui,
                                                        th,
                                                        inner,
                                                        None,
                                                        Some((kind, tip)),
                                                        false,
                                                    );
                                                },
                                            );
                                        });
                                    }
                                });
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
            ("dismissed", "lock · glyph-dim · click → banner again"),
            ("allowed", "scriptFile · text-muted · tooltip only"),
            ("size", "12 (icon-size-xs) in the 24 strip"),
            ("cleared by", "navigation to another document · app restart"),
            ("kept on", "#fragment moves"),
            (
                "tooltip placement",
                "top → bottom → inside the strip (1px border-width tolerance) · none clears → inside the strip (Tab strips › Tooltips in the strip open upward)",
            ),
            (
                "no banner when",
                "no runnable script · already allowed · sandbox off",
            ),
        ],
        &[
            TokenChip::new(
                "html-script-marker-fg",
                "→ glyph-dim",
                theme.html_script_marker_fg().to_egui(),
            ),
            TokenChip::new(
                "html-script-marker-allowed-fg",
                "→ text-muted",
                theme.html_script_marker_allowed_fg().to_egui(),
            ),
            TokenChip::without_color("html-script-marker-size", "→ icon-size-xs"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Firing: only when the user views the document (opened it, or selected the surface). \
         Agent/IPC opens, session restore and background loads keep a \"has scripts\" flag and \
         show the banner the first time the user looks at that surface.",
    );
}

/// Spec — 로드 실패. 테마마다 현재(배너·표지가 실패 위에 남음)와 결정(실패 상태만)을 나란히 보인다.
pub fn draw_load_failed(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        app_backdrop(ui, theme, |ui| {
            // 테마 패널 둘은 한 줄에 들어가지 않아 시안의 flex-wrap처럼 다음 줄로 내린다.
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                    egui::Frame::new()
                        .fill(th.bg_app().to_egui())
                        .stroke(egui::Stroke::new(
                            th.border_width.value(),
                            th.border_default().to_egui(),
                        ))
                        .corner_radius(th.corner_radius.value())
                        .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = th.spacing_md.value();
                                for (cap, stale) in [
                                    ("current — stale banner over the failure (wrong)", true),
                                    ("decided — failure state only", false),
                                ] {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                                        caption(ui, th, &format!("{name} · {cap}"));
                                        framed(
                                            ui,
                                            th,
                                            egui::vec2(
                                                NARROW_SURFACE_W.value(),
                                                FAILED_SURFACE_H.value(),
                                            ),
                                            |ui, inner| {
                                                html_surface(
                                                    ui,
                                                    th,
                                                    inner,
                                                    stale.then(|| {
                                                        view(
                                                            HtmlScriptBannerState::Blocked,
                                                            false,
                                                            false,
                                                        )
                                                    }),
                                                    stale.then(|| {
                                                        (
                                                            HtmlScriptMarkerKind::Blocked,
                                                            t(MARKER_BLOCKED),
                                                        )
                                                    }),
                                                    true,
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
            ("banner", "removed while failed"),
            ("tab marker", "lock / script glyph removed while failed"),
            (
                "failure state",
                "alertCircle md + Failed to load (accent-danger) · URL mono caption text-disabled",
            ),
            (
                "reload commits",
                "banner + marker re-derived from the new document",
            ),
        ],
        &[
            TokenChip::new(
                "accent-danger",
                "glyph + title",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new("text-muted", "URL", theme.text_muted().to_egui()),
        ],
    );
}

/// 시안 `Themed`: bg-app 카드(border-default 테두리, radius, 여백·간격 space-md)에 예제를 세로로 담는다.
pub(crate) fn themed_card(ui: &mut egui::Ui, theme: &Theme, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme.bg_app().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
                add(ui);
            });
        });
}

/// 탭 스트립 툴팁 예제 창 하나. 툴팁은 본체와 같은 규칙으로 WebView 자리를 피한다.
/// `stacked`가 거짓이면 제목 영역 → 탭 스트립 → WebView 자리라 버블이 위로 뜬다.
/// 참이면 위 pane의 WebView → 탭 스트립 → 자기 WebView라 위·아래가 막혀 버블이 스트립 안에 뜬다.
/// 이때 스트립 아래 separator는 없고 자기 WebView가 스트립 바로 아래에서 시작한다(앱 기하).
fn strip_tooltip_window(ui: &mut egui::Ui, theme: &Theme, id: &str, stacked: bool) {
    let bw = theme.border_width.value();
    let strip_h = theme.tab_height().value();
    let (above_h, below_h) = if stacked {
        // 시안은 border-box라 위 pane 자리 64 안에 아래 separator 1px가 들어간다.
        (
            STRIP_TOOLTIP_STACKED_WEBVIEW_H.value(),
            STRIP_TOOLTIP_STACKED_WEBVIEW_H.value(),
        )
    } else {
        (
            theme.titlebar_height.value(),
            STRIP_TOOLTIP_WEBVIEW_H.value(),
        )
    };
    let strip_rule = if stacked { 0.0 } else { bw };
    let size = egui::vec2(
        STRIP_TOOLTIP_W.value(),
        above_h + strip_h + strip_rule + below_h + bw * 2.0,
    );
    let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter().clone();
    let radius = theme.corner_radius.value();
    painter.rect_filled(outer, radius, theme.border_frame().to_egui());
    let inner = outer.shrink(bw);
    let mono = egui::FontId::monospace(theme.font_size_micro.value());

    let above = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), above_h));
    let mut native = Vec::new();
    if stacked {
        let upper =
            egui::Rect::from_min_max(above.min, egui::pos2(above.right(), above.bottom() - bw));
        painter.rect_filled(upper, 0.0, theme.bg_panel().to_egui());
        painter.text(
            upper.center(),
            egui::Align2::CENTER_CENTER,
            "upper pane WebView",
            mono.clone(),
            theme.text_muted_raised().to_egui(),
        );
        painter.hline(
            above.x_range(),
            above.bottom() - bw / 2.0,
            egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
        );
        native.push(upper);
    } else {
        painter.rect_filled(above, 0.0, theme.bg_app().to_egui());
        painter.text(
            egui::pos2(above.left() + theme.spacing_sm.value(), above.center().y),
            egui::Align2::LEFT_CENTER,
            "title area",
            mono.clone(),
            theme.text_muted_raised().to_egui(),
        );
    }

    let strip = egui::Rect::from_min_size(above.left_bottom(), egui::vec2(inner.width(), strip_h));
    painter.rect_filled(strip, 0.0, theme.surface_raised().to_egui());
    let tip = t(MARKER_BLOCKED);
    let marker = tab(
        ui,
        theme,
        strip,
        "report.html",
        icons::HTML,
        true,
        Some((HtmlScriptMarkerKind::Blocked, tip)),
    );
    let cell = egui::Rect::from_min_size(
        strip.min,
        egui::vec2(theme.tab_width.value().min(strip.width()), strip.height()),
    );
    let second = egui::Rect::from_min_max(egui::pos2(cell.right(), strip.top()), strip.max);
    tab(ui, theme, second, "shell", icons::TERMINAL, false, None);
    // 쌓인 예제는 앱 기하를 따른다. 자기 WebView가 스트립 바로 아래에서 시작한다(간격 0, separator 없음).
    if !stacked {
        painter.hline(
            strip.x_range(),
            strip.bottom() + bw / 2.0,
            egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
        );
    }

    let webview = egui::Rect::from_min_max(
        egui::pos2(inner.left(), strip.bottom() + strip_rule),
        inner.max,
    );
    painter.rect_filled(webview, 0.0, theme.bg_panel().to_egui());
    painter.text(
        webview.center(),
        egui::Align2::CENTER_CENTER,
        if stacked {
            "own WebView"
        } else {
            "WebView — native, drawn above Tasty"
        },
        mono,
        theme.text_muted_raised().to_egui(),
    );
    native.push(webview);

    if let Some(m) = marker {
        // 강제 표시 버블은 창 안으로 당겨지므로 예제가 스크롤로 가려졌을 때는 그리지 않는다.
        if !ui.is_rect_visible(m) {
            return;
        }
        // 강제 hover 상태: 활성 탭 배경 위에 hover 채움을 깔고 lock 글리프를 그 위에 다시 그린다.
        painter.rect_filled(
            m,
            theme.corner_radius_sm.value(),
            theme.html_script_marker_hover_bg().to_egui_premultiplied(),
        );
        let size = theme.html_script_marker_size().value();
        tasty_icons::LOCK
            .image(size, theme.html_script_marker_fg().to_egui())
            .paint_at(
                ui,
                egui::Rect::from_center_size(m.center(), egui::vec2(size, size)),
            );
        Tooltip::new(tip)
            .id_source(("strip_tooltip", id, stacked))
            .placement_clear_of_native(
                ui.ctx(),
                theme,
                m,
                Some(cell),
                ui.ctx().screen_rect(),
                &native,
            )
            .show(ui, theme, m);
    }
}

/// 탭 스트립·pane 머리 툴팁은 위로 연다. 아래의 네이티브 WebView가 egui 위에 그려지기 때문이다.
pub fn draw_strip_tooltips(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = mocha();
    spec::stage(ui, theme, StageVariant::Tight, |ui| {
        // 시안 Stage: bg-app, 여백 space-lg, 오른쪽만 size-120. 두 테마를 세로로 쌓는다.
        let lg = theme.spacing_lg.value() as i8;
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .inner_margin(egui::Margin {
                left: lg,
                right: STRIP_TOOLTIP_STAGE_PAD_RIGHT.value() as i8,
                top: lg,
                bottom: lg,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                    for (name, th) in [("Mocha", &mocha), ("Latte", &latte)] {
                        themed_card(ui, th, |ui| {
                            caption(ui, th, &format!("{name} · html tab active, lock hovered"));
                            strip_tooltip_window(ui, th, name, false);
                            caption(
                                ui,
                                th,
                                &format!(
                                    "{name} · html pane under an html pane — fallback inside the strip"
                                ),
                            );
                            strip_tooltip_window(ui, th, name, true);
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
                "scope",
                "every tooltip anchored in a pane tab strip or pane head",
            ),
            (
                "placement",
                "top → bottom → inside the strip; first that clears all native rects",
            ),
            (
                "inside the strip",
                "centred on the strip row · beside the anchor cell (right, then left) · tooltip-offset from the cell · may cover neighbour tabs",
            ),
            (
                "overlap tolerance (b2)",
                "the in-strip candidate is judged against native rects with a tolerance of border-width (1px) per edge — a one-line bubble (25) on the 24 strip passes; nothing else changes size",
            ),
            (
                "last resort (b2)",
                "if every candidate fails, the in-strip placement is used anyway (it is the one Tasty paints over); clamped top is no longer the final fallback",
            ),
            (
                "specimen (b2)",
                "themes stacked vertically; real copy; the bubble may run past the example window inside the stage padding — the app clamps to the app window, not to a pane",
            ),
            (
                "none clears",
                "superseded 2026-10-06 b2 → in-strip placement (see last resort)",
            ),
            ("horizontal", "clamped to window edges · tooltip-offset 4"),
            ("WebView hiding", "not used for tooltips"),
            ("delay · copy · click", "unchanged"),
        ],
        &[
            TokenChip::without_color("tooltip-offset", "→ space-xs 4 · anchor gap + edge clamp"),
            TokenChip::new("tooltip-bg", "bubble", theme.tooltip_bg().to_egui()),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_copy_key_resolves_to_english_text() {
        for key in [
            TITLE,
            BODY,
            BODY_REMOTE,
            ACTION,
            RELOADING,
            MARKER_BLOCKED,
            MARKER_ALLOWED,
            LOAD_FAILED,
        ] {
            let text = t(key);
            assert_ne!(text, key, "{key} is missing from lang/en.toml");
            assert!(!text.is_empty(), "{key}");
        }
        assert_eq!(t(TITLE), crate::i18n::t("banner.html_script.title"));
    }
}
