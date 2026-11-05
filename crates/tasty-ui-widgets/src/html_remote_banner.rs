//! HTML surface의 원격 콘텐츠 차단 안내 배너와 주소 칸 끝 표지.
//! 배너는 스크립트 안내와 같은 셸·단계·좁은 배치·닫기 규칙을 쓰고 글리프만 `globeOff`다.
//! 두 배너는 합치지 않고 크기 안내 → 원격 → 스크립트 순서로 쌓는다(호출자가 배치한다).
//! 표지는 배너가 화면에 없을 때 주소 칸 끝(건너뛴 파일 Tag 뒤)에 두며, 차단 표지는 클릭하면 배너를
//! 다시 보이게 할 뿐 직접 허용하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::html_script_banner::{
    HtmlScriptBannerOutput, HtmlScriptBannerState, HtmlScriptBannerView, html_notice_banner,
};
use crate::tooltip::Tooltip;

/// 원격 안내 배너의 단계. 이름과 규칙이 스크립트 안내와 같다.
pub type HtmlRemoteBannerState = HtmlScriptBannerState;
/// 원격 안내 배너 입력값. 문자열은 호출자가 번역해 넣는다.
pub type HtmlRemoteBannerView<'a> = HtmlScriptBannerView<'a>;
/// 그린 카드 영역과 이번 프레임의 클릭.
pub type HtmlRemoteBannerOutput = HtmlScriptBannerOutput;

/// 원격 콘텐츠 차단 안내 배너를 현재 `ui` 폭에 맞춰 그린다. inset 배치는 호출자가 정한다.
pub fn html_remote_banner(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &HtmlRemoteBannerView<'_>,
) -> HtmlRemoteBannerOutput {
    html_notice_banner(
        ui,
        theme,
        view,
        tasty_icons::GLOBE_OFF,
        theme.html_remote_banner_glyph().to_egui(),
    )
}

/// 주소 칸 끝 표지의 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlRemoteMarkerKind {
    /// 차단된 채 배너가 화면에 없다(닫았거나, 정적 참조 없이 OS가 차단을 관측했다).
    /// 클릭하면 배너를 다시 보인다.
    Blocked,
    /// 이 문서에 원격 콘텐츠를 허용했다. 툴팁만 있고 클릭할 수 없다.
    Allowed,
}

/// 표지 입력값.
#[derive(Clone, Copy)]
pub struct HtmlRemoteMarkerView<'a> {
    pub kind: HtmlRemoteMarkerKind,
    pub tooltip: &'a str,
    /// 포인터와 무관하게 hover 채움을 그린다. 상태를 나란히 보이는 갤러리에서만 쓴다.
    pub force_hover: bool,
    /// egui 위에 그려지는 네이티브 콘텐츠(WebView) 영역. 툴팁이 이 영역을 피한다.
    pub native: &'a [egui::Rect],
}

/// 표지가 차지하는 정사각 칸의 한 변(`html-remote-marker-hit`).
pub fn html_remote_marker_hit(theme: &Theme) -> LogicalPx {
    theme.html_remote_marker_hit()
}

/// `rect`(한 변 `html-remote-marker-hit`) 가운데에 표지를 그린다. hover 때 위쪽 툴팁을 보인다.
/// Blocked만 클릭을 받고 hover 채움을 가진다.
pub fn html_remote_marker(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    view: &HtmlRemoteMarkerView<'_>,
) -> egui::Response {
    let size = theme.html_remote_marker_size().value();
    let (glyph, color, sense) = match view.kind {
        HtmlRemoteMarkerKind::Blocked => (
            tasty_icons::GLOBE_OFF,
            theme.html_remote_marker_fg(),
            egui::Sense::click(),
        ),
        HtmlRemoteMarkerKind::Allowed => (
            tasty_icons::GLOBE,
            theme.html_remote_marker_allowed_fg(),
            egui::Sense::hover(),
        ),
    };
    let resp = ui.interact(rect, ui.id().with("html_remote_marker"), sense);
    let blocked = view.kind == HtmlRemoteMarkerKind::Blocked;
    let color = if blocked && resp.hovered() {
        theme.html_remote_marker_fg_hover()
    } else {
        color
    };
    if blocked && (resp.hovered() || view.force_hover) {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.html_remote_marker_hover_bg().to_egui_premultiplied(),
        );
    }
    let glyph_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(size, size));
    glyph.image(size, color.to_egui()).paint_at(ui, glyph_rect);
    if resp.hovered() {
        let window = ui.ctx().screen_rect();
        Tooltip::new(view.tooltip)
            .id_source(resp.id)
            .placement_clear_of_native(ui.ctx(), theme, rect, None, window, view.native)
            .show(ui, theme, rect);
    }
    if blocked {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    fn banner_view(state: HtmlRemoteBannerState) -> HtmlRemoteBannerView<'static> {
        HtmlRemoteBannerView {
            title: "Remote content in this document is blocked",
            body: "Stylesheets, images, fonts, frames and scripts from the network don't load.",
            action: "Allow remote content",
            reloading: "Reloading with remote content allowed",
            loading_tooltip: "Available when the document finishes loading",
            state,
            narrow: false,
            force_hover: false,
        }
    }

    /// 그려진 이미지 도형의 색들을 모은다. 글리프 tint가 배너 토큰을 따르는지 본다.
    fn image_tints(shapes: &[egui::Shape]) -> Vec<egui::Color32> {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) if r.brush.is_some() => Some(r.fill),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_banner_glyph_uses_the_remote_banner_token() {
        let theme = theme();
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                html_remote_banner(ui, &theme, &banner_view(HtmlRemoteBannerState::Blocked));
            });
        });
        let shapes: Vec<_> = out.shapes.into_iter().map(|c| c.shape).collect();
        assert!(
            image_tints(&shapes).contains(&theme.html_remote_banner_glyph().to_egui()),
            "globeOff glyph tinted with html-remote-banner-glyph"
        );
    }

    fn marker_shapes(kind: HtmlRemoteMarkerKind, force_hover: bool) -> (Theme, Vec<egui::Shape>) {
        let theme = theme();
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let hit = html_remote_marker_hit(&theme).value();
                let rect = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(hit, hit));
                html_remote_marker(
                    ui,
                    &theme,
                    rect,
                    &HtmlRemoteMarkerView {
                        kind,
                        tooltip: "tip",
                        force_hover,
                        native: &[],
                    },
                );
            });
        });
        (theme, out.shapes.into_iter().map(|c| c.shape).collect())
    }

    #[test]
    fn blocked_and_allowed_markers_use_their_own_ink() {
        let (theme, blocked) = marker_shapes(HtmlRemoteMarkerKind::Blocked, false);
        assert!(image_tints(&blocked).contains(&theme.html_remote_marker_fg().to_egui()));
        let (theme, allowed) = marker_shapes(HtmlRemoteMarkerKind::Allowed, false);
        assert!(image_tints(&allowed).contains(&theme.html_remote_marker_allowed_fg().to_egui()));
    }

    #[test]
    fn only_the_blocked_marker_takes_the_hover_fill() {
        let fill = |theme: &Theme| theme.html_remote_marker_hover_bg().to_egui_premultiplied();
        let has_fill = |theme: &Theme, shapes: &[egui::Shape]| {
            shapes
                .iter()
                .any(|s| matches!(s, egui::Shape::Rect(r) if r.fill == fill(theme)))
        };
        let (theme, blocked) = marker_shapes(HtmlRemoteMarkerKind::Blocked, true);
        assert!(has_fill(&theme, &blocked));
        let (theme, allowed) = marker_shapes(HtmlRemoteMarkerKind::Allowed, true);
        assert!(!has_fill(&theme, &allowed));
    }
}
