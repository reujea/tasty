//! HTML surface의 스크립트 차단 안내 배너와 탭 스트립 마커.
//! 배너는 inset 배치로 그리므로 WebView를 덮지 않는다. 문자열은 호출자가 주입한다.
//! 상자 경계는 셸 > 행 [글리프 | 본문 | 액션] + 우상단 절대 위치 닫기 슬롯이다.
//! 좁은 surface에서는 액션이 본문 왼쪽 가장자리에 맞춰 다음 줄로 내려간다.

use tasty_type_appearance::theme::Theme;

use crate::banner::banner_shell;
use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::icon_button::{IconButton, IconButtonVariant};
use crate::spinner::Spinner;
use crate::tooltip::{Tooltip, TooltipPlacement, tooltip_hover_delay_elapsed};

/// surface 폭이 `banner_narrow_below`보다 좁으면 액션을 본문 아래 줄로 내린다.
/// surface 크기가 바뀔 때마다 다시 판정하며 내용 길이로 임계값을 바꾸지 않는다.
pub fn html_script_banner_is_narrow(surface_width: f32, theme: &Theme) -> bool {
    crate::banner::banner_is_narrow(surface_width, theme)
}

/// 배너가 표시하는 단계. 닫힘·허용 뒤에는 배너 대신 탭 마커가 남는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlScriptBannerState {
    /// 스크립트가 차단된 기본 상태. 액션 버튼을 보이고 hover 때 닫기 버튼을 드러낸다.
    Blocked,
    /// 차단된 채 surface가 새 문서를 로드하는 중(commit 전). 액션 버튼을 비활성으로 그리고
    /// hover 때 위쪽 툴팁으로 이유를 보인다. 닫기 버튼은 Blocked와 같다.
    Loading,
    /// 허용을 누른 뒤 한 번 다시 읽는 중. 액션 자리를 스피너와 라벨이 대신하고 닫기 버튼을 숨긴다.
    Reloading,
}

/// 배너 입력값. 문자열은 본체가 번역해 넣고 갤러리는 확정 영어 문안을 넣는다.
pub struct HtmlScriptBannerView<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub action: &'a str,
    pub reloading: &'a str,
    /// Loading 단계의 비활성 버튼 툴팁.
    pub loading_tooltip: &'a str,
    pub state: HtmlScriptBannerState,
    /// surface가 좁아 액션을 본문 아래 줄로 내린다.
    pub narrow: bool,
    /// 포인터와 무관하게 hover 상태로 그린다. 상태를 나란히 보여 주는 갤러리에서만 쓴다.
    pub force_hover: bool,
}

/// 그린 카드 영역과 이번 프레임의 클릭.
pub struct HtmlScriptBannerOutput {
    pub rect: egui::Rect,
    /// 액션 자리(버튼, 재로드 중에는 스피너와 라벨). 비활성 버튼의 툴팁을 연 채로 보여 줄 때 기준이 된다.
    pub action_rect: egui::Rect,
    pub hovered: bool,
    pub allow_clicked: bool,
    pub dismiss_clicked: bool,
}

fn text_galley(
    ui: &egui::Ui,
    theme: &Theme,
    text: &str,
    size: f32,
    color: egui::Color32,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            line_height: Some(size * theme.line_height_ui),
            ..Default::default()
        },
    );
    // Tasty 고정 문구라 줄 수를 제한하지 않는다. 길면 카드가 늘어난다(docs/design/systems/banner.md#본문-줄-수).
    job.wrap.max_width = wrap_width;
    ui.fonts(|f| f.layout_job(job))
}

/// 액션 자리 크기. 버튼은 `Button`과 같은 식으로 잰다.
fn action_size(ui: &egui::Ui, theme: &Theme, view: &HtmlScriptBannerView<'_>) -> egui::Vec2 {
    let height = ControlSize::Sm.height(theme);
    let width = match view.state {
        HtmlScriptBannerState::Blocked | HtmlScriptBannerState::Loading => {
            let font = egui::FontId::proportional(ControlSize::Sm.font_size(theme));
            let w = ui.fonts(|f| {
                f.layout_no_wrap(view.action.to_owned(), font, egui::Color32::PLACEHOLDER)
                    .rect
                    .width()
            });
            w + 2.0 * ControlSize::Sm.pad_x(theme)
        }
        HtmlScriptBannerState::Reloading => {
            let font = egui::FontId::proportional(theme.banner_body_font_size().value());
            let w = ui.fonts(|f| {
                f.layout_no_wrap(view.reloading.to_owned(), font, egui::Color32::PLACEHOLDER)
                    .rect
                    .width()
            });
            theme.icon_glyph_size_sm.value() + theme.spacing_sm.value() + w
        }
    };
    egui::vec2(width, height)
}

fn draw_action(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &HtmlScriptBannerView<'_>,
    rect: egui::Rect,
) -> (bool, egui::Rect) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    match view.state {
        HtmlScriptBannerState::Blocked => {
            let resp = Button::new(view.action)
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .show(&mut child, theme);
            (resp.clicked(), resp.rect)
        }
        HtmlScriptBannerState::Loading => {
            // 전환은 잉크만 바뀐다. 지연이나 유예 없이 로드 시작부터 commit까지 비활성이다.
            let resp = Button::new(view.action)
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .enabled(false)
                .show(&mut child, theme);
            if tooltip_hover_delay_elapsed(ui.ctx(), theme, resp.id, resp.hovered()) {
                Tooltip::new(view.loading_tooltip)
                    .placement(TooltipPlacement::Top)
                    .id_source(resp.id)
                    .show(ui, theme, resp.rect);
            }
            (false, resp.rect)
        }
        HtmlScriptBannerState::Reloading => {
            let spin = theme.icon_glyph_size_sm.value();
            let spin_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.center().y - spin / 2.0),
                egui::vec2(spin, spin),
            );
            let mut spin_ui = child.new_child(egui::UiBuilder::new().max_rect(spin_rect));
            Spinner::new().size(spin).show(&mut spin_ui, theme);
            child.painter().text(
                egui::pos2(
                    spin_rect.right() + theme.spacing_sm.value(),
                    rect.center().y,
                ),
                egui::Align2::LEFT_CENTER,
                view.reloading,
                egui::FontId::proportional(theme.banner_body_font_size().value()),
                theme.banner_body_fg().to_egui(),
            );
            (false, rect)
        }
    }
}

/// 행 내용을 그리고 액션 클릭 여부와 액션 자리를 돌려준다. `ui`는 셸 안쪽 여백을 뺀 영역이다.
fn draw_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &HtmlScriptBannerView<'_>,
    glyph_icon: tasty_icons::Icon,
    glyph_color: egui::Color32,
) -> (bool, egui::Rect) {
    // 오른쪽은 닫기 슬롯 몫을 비운다. 셸이 이미 좌우 padding을 넣었으므로 그 몫은 뺀다.
    let reserve =
        (theme.icon_button_size_sm().value() + theme.spacing_sm.value() + theme.spacing_xs.value()
            - theme.banner_padding_x().value())
        .max(0.0);
    let row_w = (ui.available_width() - reserve).max(0.0);
    let glyph = theme.icon_glyph_size_md.value();
    let gap = theme.banner_gap().value();
    let nudge = theme.banner_glyph_offset().value();
    let text_gap = theme.banner_text_gap().value();
    let action = action_size(ui, theme, view);

    let body_w = if view.narrow {
        row_w - glyph - gap
    } else {
        row_w - glyph - gap - gap - action.x
    }
    .max(0.0);
    let title = text_galley(
        ui,
        theme,
        view.title,
        theme.banner_title_font_size().value(),
        theme.banner_fg().to_egui(),
        body_w,
    );
    let body = text_galley(
        ui,
        theme,
        view.body,
        theme.banner_body_font_size().value(),
        theme.banner_body_fg().to_egui(),
        body_w,
    );
    let text_h = title.size().y + text_gap + body.size().y;

    let first_line_h = (glyph + nudge).max(text_h);
    let (row_h, text_y, action_pos) = if view.narrow {
        // flex-wrap: 줄 사이에도 같은 gap을 두고 액션은 본문 왼쪽 가장자리에서 시작한다.
        (
            first_line_h + gap + action.y,
            0.0,
            egui::vec2(glyph + gap, first_line_h + gap),
        )
    } else {
        let h = first_line_h.max(action.y);
        (
            h,
            (h - text_h) / 2.0,
            egui::vec2(row_w - action.x, (h - action.y) / 2.0),
        )
    };

    let (row, _) = ui.allocate_exact_size(egui::vec2(row_w, row_h), egui::Sense::hover());
    let glyph_rect =
        egui::Rect::from_min_size(row.min + egui::vec2(0.0, nudge), egui::vec2(glyph, glyph));
    glyph_icon
        .image(glyph, glyph_color)
        .paint_at(ui, glyph_rect);

    let text_x = row.left() + glyph + gap;
    let painter = ui.painter();
    let title_h = title.size().y;
    painter.galley(
        egui::pos2(text_x, row.top() + text_y),
        title,
        egui::Color32::PLACEHOLDER,
    );
    painter.galley(
        egui::pos2(text_x, row.top() + text_y + title_h + text_gap),
        body,
        egui::Color32::PLACEHOLDER,
    );

    let action_rect = egui::Rect::from_min_size(row.min + action_pos, action);
    draw_action(ui, theme, view, action_rect)
}

/// 스크립트 차단 안내 배너를 현재 `ui` 폭에 맞춰 그린다. inset 배치는 호출자가
/// [`crate::inset_banner_zone`]과 [`crate::inset_content_rect`]로 정한다.
pub fn html_script_banner(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &HtmlScriptBannerView<'_>,
) -> HtmlScriptBannerOutput {
    html_notice_banner(
        ui,
        theme,
        view,
        tasty_icons::LOCK,
        theme.html_script_banner_glyph().to_egui(),
    )
}

/// 스크립트 안내와 원격 콘텐츠 안내가 함께 쓰는 배너 본체. 두 안내는 글리프만 다르고
/// 단계·배치·닫기 규칙이 같다(디자인 `HtmlRemoteBanner`가 `HtmlScriptBanner`와 같은 시각 규칙을 쓴다).
pub(crate) fn html_notice_banner(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &HtmlScriptBannerView<'_>,
    glyph: tasty_icons::Icon,
    glyph_color: egui::Color32,
) -> HtmlScriptBannerOutput {
    let mut allow_clicked = false;
    let mut action_rect = egui::Rect::NOTHING;
    let rect = banner_shell(ui, theme, 1.0, |ui| {
        (allow_clicked, action_rect) = draw_row(ui, theme, view, glyph, glyph_color);
    });
    let hovered = view.force_hover || ui.rect_contains_pointer(rect);
    let mut dismiss_clicked = false;
    if hovered && view.state != HtmlScriptBannerState::Reloading {
        let side = theme.icon_button_size_sm().value();
        let slot = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - theme.spacing_sm.value() - side,
                rect.top() + theme.banner_padding_y().value(),
            ),
            egui::vec2(side, side),
        );
        let mut slot_ui = ui.new_child(egui::UiBuilder::new().max_rect(slot));
        dismiss_clicked = IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .show(&mut slot_ui, theme, &|ui, r, c| {
                tasty_icons::CLOSE.image(r.height(), c).paint_at(ui, r)
            })
            .clicked();
    }
    HtmlScriptBannerOutput {
        rect,
        action_rect,
        hovered,
        allow_clicked,
        dismiss_clicked,
    }
}

/// 탭 라벨 뒤에 붙는 스크립트 상태 표지.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlScriptMarkerKind {
    /// 차단된 채 배너를 닫았다. 클릭하면 배너를 다시 보인다(직접 허용하지 않는다).
    Blocked,
    /// 이 문서에 스크립트를 허용했다. 툴팁만 있고 클릭할 수 없다.
    Allowed,
}

/// 마커를 `html_script_marker_hit` 정사각 칸 가운데에 그린다. hover 때 `tooltip`을 보인다.
/// Blocked 마커만 클릭을 받고 hover 채움을 가진다. Allowed 마커는 툴팁만 있다.
/// 툴팁은 탭 스트립 규칙([`Tooltip::placement_clear_of_native`])으로 `native` 영역을 피해 배치한다.
/// `cell`은 마커가 든 탭 칸이며 위·아래가 모두 막혔을 때 스트립 안 후보의 기준이 된다.
pub fn html_script_marker(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: HtmlScriptMarkerKind,
    tooltip: &str,
    cell: Option<egui::Rect>,
    native: &[egui::Rect],
) -> egui::Response {
    let hit = theme.html_script_marker_hit().value();
    let size = theme.html_script_marker_size().value();
    let (glyph, color, sense) = match kind {
        HtmlScriptMarkerKind::Blocked => (
            tasty_icons::LOCK,
            theme.html_script_marker_fg(),
            egui::Sense::click(),
        ),
        HtmlScriptMarkerKind::Allowed => (
            tasty_icons::SCRIPT,
            theme.html_script_marker_allowed_fg(),
            egui::Sense::hover(),
        ),
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(hit, hit), sense);
    let color = if kind == HtmlScriptMarkerKind::Blocked && resp.hovered() {
        theme.html_script_marker_fg_hover()
    } else {
        color
    };
    if kind == HtmlScriptMarkerKind::Blocked && resp.hovered() {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.html_script_marker_hover_bg().to_egui_premultiplied(),
        );
    }
    let glyph_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(size, size));
    glyph.image(size, color.to_egui()).paint_at(ui, glyph_rect);
    if resp.hovered() {
        let window = ui.ctx().screen_rect();
        Tooltip::new(tooltip)
            .id_source(resp.id)
            .placement_clear_of_native(ui.ctx(), theme, rect, cell, window, native)
            .show(ui, theme, rect);
    }
    if kind == HtmlScriptMarkerKind::Blocked {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 배너를 한 번 그려 모든 도형을 돌려준다.
    fn banner_shapes(state: HtmlScriptBannerState, narrow: bool) -> (Theme, Vec<egui::Shape>) {
        let (theme, _, shapes) = banner_render(state, narrow, false);
        (theme, shapes)
    }

    /// 배너를 한 번 그려 배너 rect와 모든 도형을 돌려준다.
    fn banner_render(
        state: HtmlScriptBannerState,
        narrow: bool,
        force_hover: bool,
    ) -> (Theme, egui::Rect, Vec<egui::Shape>) {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let view = HtmlScriptBannerView {
            title: "Scripts in this document are blocked",
            body: "Buttons and menus that need JavaScript may not respond.",
            action: "Allow for this document",
            reloading: "Reloading with scripts allowed",
            loading_tooltip: "Available when the document finishes loading",
            state,
            narrow,
            force_hover,
        };
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = html_script_banner(ui, &theme, &view).rect;
            });
        });
        (
            theme,
            rect,
            out.shapes.into_iter().map(|c| c.shape).collect(),
        )
    }

    /// hover 때 닫기(×) 슬롯 안에 그려진 도형이 있는지 본다. 슬롯 위치는 본문과 같은 식으로 구한다.
    fn draws_in_the_close_slot(state: HtmlScriptBannerState) -> bool {
        let (theme, rect, shapes) = banner_render(state, false, true);
        let side = theme.icon_button_size_sm().value();
        let slot = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - theme.spacing_sm.value() - side,
                rect.top() + theme.banner_padding_y().value(),
            ),
            egui::vec2(side, side),
        );
        shapes.iter().any(|s| {
            let b = s.visual_bounding_rect();
            b.is_positive() && slot.expand(0.5).contains_rect(b)
        })
    }

    fn has_rect(shapes: &[egui::Shape], pred: impl Fn(&egui::epaint::RectShape) -> bool) -> bool {
        shapes
            .iter()
            .any(|s| matches!(s, egui::Shape::Rect(r) if pred(r)))
    }

    #[test]
    fn allow_button_uses_the_banner_button_box() {
        for narrow in [false, true] {
            let (theme, shapes) = banner_shapes(HtmlScriptBannerState::Blocked, narrow);
            let bg = theme.banner_button_bg().to_egui();
            let border = theme.banner_button_border().to_egui();
            assert!(has_rect(&shapes, |r| r.fill == bg), "narrow={narrow}");
            assert!(
                has_rect(&shapes, |r| r.stroke.color == border
                    && r.stroke.width > 0.0),
                "narrow={narrow}"
            );
        }
    }

    #[test]
    fn loading_draws_allow_in_the_disabled_box_instead_of_the_banner_box() {
        for narrow in [false, true] {
            let (theme, shapes) = banner_shapes(HtmlScriptBannerState::Loading, narrow);
            let disabled_bg = theme.button_disabled_bg().to_egui();
            let disabled_border = theme.button_disabled_border().to_egui();
            assert!(
                has_rect(&shapes, |r| r.fill == disabled_bg),
                "narrow={narrow}"
            );
            assert!(
                has_rect(&shapes, |r| r.stroke.color == disabled_border
                    && r.stroke.width > 0.0),
                "narrow={narrow}"
            );
            let banner_bg = theme.banner_button_bg().to_egui();
            assert!(
                !has_rect(&shapes, |r| r.fill == banner_bg),
                "narrow={narrow}"
            );
        }
    }

    #[test]
    fn loading_keeps_the_close_button_on_hover() {
        assert!(draws_in_the_close_slot(HtmlScriptBannerState::Blocked));
        assert!(draws_in_the_close_slot(HtmlScriptBannerState::Loading));
        assert!(!draws_in_the_close_slot(HtmlScriptBannerState::Reloading));
    }

    /// 좁은 화면에 긴 제목·본문을 그려 각 글 도형의 줄 수와 말줄임 여부를 돌려준다.
    fn long_text_rows(width: f32) -> Vec<(usize, bool)> {
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let long = "This sentence keeps going so that a narrow banner has to wrap it onto \
                    more rows than the old two-row limit allowed, and then a few more.";
        let view = HtmlScriptBannerView {
            title: long,
            body: long,
            action: "Allow for this document",
            reloading: "Reloading with scripts allowed",
            loading_tooltip: "Available when the document finishes loading",
            state: HtmlScriptBannerState::Blocked,
            narrow: true,
            force_hover: false,
        };
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.0),
            )),
            ..Default::default()
        };
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                html_script_banner(ui, &theme, &view);
            });
        });
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == long => {
                    Some((t.galley.rows.len(), t.galley.elided))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn long_title_and_body_wrap_past_two_rows_without_an_ellipsis() {
        let texts = long_text_rows(240.0);
        assert_eq!(texts.len(), 2, "title and body");
        for (rows, elided) in texts {
            assert!(rows > 2, "rows = {rows}");
            assert!(!elided);
        }
    }

    #[test]
    fn narrow_starts_just_below_the_token_width() {
        let t = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let below = t.banner_narrow_below().value();
        assert!(html_script_banner_is_narrow(below - 1.0, &t));
        assert!(!html_script_banner_is_narrow(below, &t));
        assert!(!html_script_banner_is_narrow(below + 1.0, &t));
    }
}
