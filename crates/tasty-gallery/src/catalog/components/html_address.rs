//! Shared address-band specimens. Page pixels are illustrative host placeholders, not WebViews.
use crate::catalog::spec::{self, StageVariant};
use crate::i18n::{t, t_fmt};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlAddressAction, HtmlAddressKind as Kind, HtmlAddressLabels, HtmlAddressState,
    HtmlAddressView, HtmlScriptBannerState, HtmlScriptBannerView, html_address_bar,
    html_address_content_inset, html_script_banner,
};

pub(crate) mod remote;

// Specimen frame sizes from the address-band design; these are not product control sizes.
const W: LogicalPx = LogicalPx(460.0);
const NARROW: LogicalPx = LogicalPx(360.0);
const H: LogicalPx = LogicalPx(240.0);
const STACK_W: LogicalPx = LogicalPx(560.0);
const STACK_H: LogicalPx = LogicalPx(300.0);

fn labels() -> HtmlAddressLabels<'static> {
    HtmlAddressLabels {
        placeholder: t("html_address.placeholder"),
        back: t("html_address.back"),
        forward: t("html_address.forward"),
        reload: t("html_address.reload"),
        disconnected: t("html_address.disconnected"),
    }
}

pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (index, label, kind, address, page, editing, error) in [
            (0, "1 · empty tab", Kind::Empty, "", "empty", false, None),
            (
                1,
                "2 · local file",
                Kind::Local,
                "~/work/tasty/target/report.html",
                "page",
                false,
                None,
            ),
            (
                2,
                "3 · web page",
                Kind::Web,
                "https://docs.example.com/guide/getting-started#install",
                "page",
                false,
                None,
            ),
            (
                3,
                "4 · loading — Reload slot spins",
                Kind::Web,
                "https://docs.example.com/guide/",
                "loading",
                false,
                None,
            ),
            (
                4,
                "5 · load failed — attempted address",
                Kind::Web,
                "https://offline.example",
                "failed",
                false,
                None,
            ),
            (
                5,
                "6 · editing — draft preserved",
                Kind::Local,
                "~/work/tasty/target/rep",
                "page",
                true,
                None,
            ),
            (
                6,
                "7a · input error — missing file",
                Kind::Local,
                "/tmp/x/missing.html",
                "page",
                true,
                Some(t_fmt("html_address.not_found", "/tmp/x/missing.html")),
            ),
            (
                7,
                "7b · input error — no base",
                Kind::Empty,
                "docs/index.html",
                "empty",
                true,
                Some(t("html_address.no_base").into()),
            ),
        ] {
            spec::cluster(ui, theme, label, |ui| {
                let mut state = HtmlAddressState {
                    buffer: address.into(),
                    editing,
                    error,
                };
                tile(
                    ui,
                    theme,
                    index,
                    W,
                    H,
                    &mut state,
                    &HtmlAddressView {
                        address,
                        committed_address: if page == "failed" {
                            "https://example.com/previous"
                        } else {
                            address
                        },
                        kind,
                        can_back: index > 0,
                        can_forward: index == 2,
                        loading: page == "loading",
                        disconnected: false,
                        skipped: (kind == Kind::Mirror)
                            .then_some(("1 skipped", "Declined: hero-video.mp4")),
                        remote_marker: None,
                        focus: false,
                        labels: labels(),
                    },
                    page,
                    0,
                );
            });
        }
    });
}

pub fn draw_placement(ui: &mut egui::Ui, theme: &Theme) {
    remote::draw_states(ui, theme);
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (index, label, width, height, kind, banners) in [
            (
                8,
                "8 · band → script banner → page",
                STACK_W,
                STACK_H,
                Kind::Local,
                1,
            ),
            (9, "mirror · read-only + Reload", W, H, Kind::Mirror, 0),
            (10, "narrow · Forward hidden", NARROW, STACK_H, Kind::Web, 1),
            (
                11,
                "mirror placeholder · band hidden",
                W,
                H,
                Kind::MirrorPlaceholder,
                0,
            ),
            (
                12,
                "band → size → script → page",
                STACK_W,
                STACK_H,
                Kind::Mirror,
                2,
            ),
        ] {
            spec::cluster(ui, theme, label, |ui| {
                let address = match kind {
                    Kind::Mirror => "build-eu:~/work/site/index.html",
                    Kind::Web => "https://docs.example.com/guide/getting-started",
                    _ => "~/work/tasty/target/report.html",
                };
                tile(
                    ui,
                    theme,
                    index,
                    width,
                    height,
                    &mut HtmlAddressState::default(),
                    &HtmlAddressView {
                        address,
                        committed_address: address,
                        kind,
                        can_back: true,
                        can_forward: true,
                        loading: false,
                        disconnected: false,
                        skipped: (kind == Kind::Mirror)
                            .then_some(("1 skipped", "Declined: hero-video.mp4")),
                        remote_marker: None,
                        focus: false,
                        labels: labels(),
                    },
                    "page",
                    banners,
                );
            });
        }
    });
}

pub fn draw_live(ui: &mut egui::Ui, theme: &Theme) {
    let id = ui.id().with("live_html_address");
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<HtmlAddressState>(id).unwrap_or_default());
    let mut address = ui.ctx().data_mut(|d| {
        d.get_temp::<String>(id.with("commit"))
            .unwrap_or_else(|| "https://example.com/a/".into())
    });
    let out = html_address_bar(
        ui,
        theme,
        id,
        LogicalPx(ui.available_width()),
        &mut state,
        &HtmlAddressView {
            address: &address,
            committed_address: &address,
            kind: Kind::Web,
            can_back: false,
            can_forward: false,
            loading: false,
            disconnected: false,
            skipped: None,
            remote_marker: None,
            focus: false,
            labels: labels(),
        },
    );
    if let HtmlAddressAction::Open(input) = out.action {
        match tasty_utils::html_address::resolve_html_address(&input, Some(&address), None, None) {
            Ok(target) if target.local_path.is_none() => {
                address = target.display(None);
                state.accepted();
            }
            // This gallery never touches the filesystem or opens a WebView.
            _ => state.reject(t("html_address.invalid").into()),
        }
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(id, state);
        d.insert_temp(id.with("commit"), address);
    });
    spec::note(
        ui,
        theme,
        "Live draft / Enter / Esc; HTTP(S) resolution only. No document or filesystem is opened.",
    );
}

#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: usize,
    width: LogicalPx,
    height: LogicalPx,
    state: &mut HtmlAddressState,
    view: &HtmlAddressView<'_>,
    page: &str,
    banners: usize,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), height.value()),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        rect,
        theme.corner_radius.value(),
        theme.bg_panel().to_egui(),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    child.spacing_mut().item_spacing.y = 0.0;
    // Tab strip is a specimen frame, not a second product implementation.
    let (tab, _) = child.allocate_exact_size(
        egui::vec2(width.value(), theme.tab_bar_height.value()),
        egui::Sense::hover(),
    );
    child
        .painter()
        .rect_filled(tab, 0.0, theme.bg_sidebar().to_egui());
    child.painter().text(
        tab.left_center() + egui::vec2(theme.spacing_sm.value(), 0.0),
        egui::Align2::LEFT_CENTER,
        "report.html",
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
    );
    let out = html_address_bar(&mut child, theme, ui.id().with(id), width, state, view);
    let mut heights = Vec::new();
    let mut top = LogicalPx(out.rect.bottom()) + theme.banner_margin();
    for i in 0..banners {
        let area = egui::Rect::from_min_max(
            egui::pos2(
                (LogicalPx(rect.left()) + theme.banner_margin()).value(),
                top.value(),
            ),
            egui::pos2(
                (LogicalPx(rect.right()) - theme.banner_margin()).value(),
                rect.bottom(),
            ),
        );
        let mut b = child.new_child(egui::UiBuilder::new().id_salt((id, i)).max_rect(area));
        let banner = if banners == 2 && i == 0 {
            remote::size_card(
                &mut b,
                theme,
                width < theme.banner_narrow_below(),
                false,
                false,
            )
        } else {
            html_script_banner(
                &mut b,
                theme,
                &HtmlScriptBannerView {
                    title: t("banner.html_script.title"),
                    body: t("banner.html_script.body"),
                    action: t("banner.html_script.action"),
                    reloading: t("banner.html_script.reloading"),
                    loading_tooltip: t("banner.html_script.action_loading"),
                    state: HtmlScriptBannerState::Blocked,
                    narrow: width < theme.banner_narrow_below(),
                    force_hover: false,
                },
            )
            .rect
        };
        heights.push(LogicalPx(banner.height()));
        top = LogicalPx(banner.bottom()) + theme.banner_inset_gap();
    }
    let inset = html_address_content_inset(theme, LogicalPx(out.rect.height()), &heights);
    let page_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), (LogicalPx(tab.bottom()) + inset).value()),
        rect.max,
    );
    page_placeholder(&mut child, theme, page_rect, page, view.address);
    child.painter().rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// 시안 `HtmlAddressSurfaceG`: 주소 띠와 페이지 자리만 있는 표본.
// 이유: 시안 prop 구성을 유지해 비교하기 쉽게 한다.
#[allow(clippy::too_many_arguments)]
pub(super) fn band_surface(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: &str,
    width: LogicalPx,
    height: LogicalPx,
    kind: Kind,
    address: &str,
    editing: bool,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), height.value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    child.spacing_mut().item_spacing.y = 0.0;
    let mut state = HtmlAddressState {
        buffer: address.into(),
        editing,
        error: None,
    };
    let view = HtmlAddressView {
        address,
        committed_address: address,
        kind,
        can_back: true,
        can_forward: false,
        loading: false,
        disconnected: false,
        skipped: None,
        remote_marker: None,
        focus: false,
        labels: labels(),
    };
    let out = html_address_bar(
        &mut child,
        theme,
        ui.id().with(id),
        width,
        &mut state,
        &view,
    );
    let page = egui::Rect::from_min_max(egui::pos2(rect.left(), out.rect.bottom()), rect.max);
    page_placeholder(ui, theme, page, "page", address);
}

/// 페이지 영역의 상태 표본(디자인 `HtmlPageState`). `page`는 `page`·`empty`·`loading`·`failed`다.
pub(crate) fn page_placeholder(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    page: &str,
    address: &str,
) {
    let mut body = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    body.set_clip_rect(ui.clip_rect().intersect(rect));
    body.spacing_mut().item_spacing.y = theme.spacing_sm.value();
    let sub = match page {
        "empty" => Some(t_fmt(
            "html_address.empty_hint",
            if cfg!(target_os = "macos") {
                "⌘L"
            } else {
                "Alt+L"
            },
        )),
        "failed" => Some(address.into()),
        _ => None,
    };
    let glyph = theme.icon_glyph_size_md;
    let block = if page == "page" {
        theme.font_size_body
    } else {
        glyph
            + theme.spacing_sm
            + theme.font_size_body
            + if sub.is_some() {
                theme.spacing_sm + theme.font_size_caption
            } else {
                LogicalPx(0.0)
            }
    };
    body.add_space(
        ((LogicalPx(rect.height()) - block) / 2.0)
            .max(LogicalPx(0.0))
            .value(),
    );
    match page {
        "empty" => {
            body.add(tasty_icons::HTML.image(glyph.value(), theme.text_disabled().to_egui()));
        }
        "loading" => {
            tasty_ui_widgets::Spinner::new()
                .size(glyph.value())
                .show(&mut body, theme);
        }
        "failed" => {
            body.add(
                tasty_icons::ALERT_CIRCLE
                    .image(glyph.value(), theme.accent_danger_raised().to_egui()),
            );
        }
        _ => {}
    }
    let title = match page {
        "empty" => t("webview.no_page"),
        "loading" => t("webview.loading"),
        "failed" => t("webview.error"),
        _ => "Native page area",
    };
    body.label(
        egui::RichText::new(title)
            .size(theme.font_size_body.value())
            .color(
                if page == "failed" {
                    theme.accent_danger()
                } else {
                    theme.text_muted()
                }
                .to_egui(),
            ),
    );
    if let Some(sub) = sub {
        let font = if page == "failed" {
            egui::FontId::monospace(theme.font_size_caption.value())
        } else {
            egui::FontId::proportional(theme.font_size_caption.value())
        };
        body.add(
            egui::Label::new(
                egui::RichText::new(sub)
                    .font(font)
                    .color(theme.text_muted().to_egui()),
            )
            .truncate(),
        );
    }
}
