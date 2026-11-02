//! Received remote HTML states, using the same widgets as the host.
use crate::catalog::spec::{self, StageVariant};
use crate::i18n::{t, t_fmt, t_fmt2};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    HtmlReceiveKind, HtmlReceiveView, HtmlSizeBannerView, HtmlSizeDetail, html_receive_state,
    html_size_banner,
};

pub(crate) fn size_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    narrow: bool,
    limit: bool,
    waiting: bool,
) -> egui::Rect {
    let title = if limit {
        t_fmt("html_remote.limit_title", "2.1 MB")
    } else {
        t("html_remote.size_title").into()
    };
    let waiting = waiting.then(|| t_fmt("html_remote.waiting", "2"));
    html_size_banner(
        ui,
        theme,
        &HtmlSizeBannerView {
            title: &title,
            detail: if limit {
                HtmlSizeDetail::Limit {
                    body: t("html_remote.limit_body"),
                }
            } else {
                HtmlSizeDetail::File {
                    path: "assets/media/marketing/2026/launch/hero-background-loop-4k.mp4",
                    size: "12.4 MB",
                    waiting: waiting.as_deref(),
                }
            },
            decline: t(if limit {
                "html_remote.stop"
            } else {
                "html_remote.decline"
            }),
            allow: t(if limit {
                "html_remote.continue"
            } else {
                "html_remote.allow"
            }),
            narrow,
        },
    )
    .rect
}
// Showcase widths come from the received wide/narrow specimens, not product control dimensions.
const WIDE: LogicalPx = LogicalPx(560.0);
const NARROW: LogicalPx = LogicalPx(360.0);
const PAGE_HEIGHT: LogicalPx = LogicalPx(180.0);
pub(super) fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for width in [WIDE, NARROW] {
            spec::wrap_item(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(width.value());
                    for (limit, waiting) in [(false, false), (false, true), (true, false)] {
                        size_card(
                            ui,
                            theme,
                            width < theme.banner_narrow_below(),
                            limit,
                            waiting,
                        );
                    }
                });
            });
        }
        let progress = t_fmt2("html_remote.progress", "1.4 MB", "12");
        for (kind, title, subtitle) in [
            (
                HtmlReceiveKind::Receiving,
                t("html_remote.receiving"),
                progress.as_str(),
            ),
            (
                HtmlReceiveKind::Paused,
                t("html_remote.receiving"),
                t("html_remote.paused"),
            ),
            (
                HtmlReceiveKind::Disconnected,
                t("html_address.disconnected"),
                t("html_remote.disconnected_sub"),
            ),
            (
                HtmlReceiveKind::UnsupportedHttp,
                t("html_remote.macos_http"),
                t("html_remote.macos_http_sub"),
            ),
        ] {
            spec::wrap_item(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(NARROW.value(), PAGE_HEIGHT.value()),
                    egui::Sense::hover(),
                );
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
            });
        }
        ui.label(t_fmt("html_remote.skipped", "3"));
    });
}
