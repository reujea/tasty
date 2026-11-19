//! 제목바와 콘텐츠 영역을 그리는 공용 팝업 프레임.
//! 콘텐츠 여백과 그림자는 호출자가 팝업 종류에 맞게 지정한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    ControlSize, IconButtonState, elide_popup_title, paint_icon_button_state,
    paint_popup_title_glyph, popup_title_font, popup_title_text_rect, show_popup_title_tooltip,
    tooltip_hover_delay_elapsed,
};

use crate::catalog::icons;

/// 본체 popup 상수 — 제목바 높이.
pub const TITLE_BAR_HEIGHT: LogicalPx = LogicalPx(28.0);
/// 본체 popup 콘텐츠의 위아래 여백. 본체 Theme.spacing_xs와 같은 토큰을 읽는다.
/// 갤러리는 Theme 치수에 배율을 곱하지 않고 egui 전역 배율을 사용한다.
pub const CONTENT_MARGIN: LogicalPx = tasty_design_tokens::generated::semantic::SPACE_XS;

/// 타이틀바 우측 버튼 세트. 본체 `PopupManager` 구성과 같다 — close(X) 는 타이틀바가
/// 있는 모든 popup 에, fullscreen 은 **전체화면 무대를 선언한 popup** 에만 붙는다
/// (`PopupDef.fullscreen_stage`).
#[derive(Clone, Copy, Default)]
pub struct TitleButtons {
    /// 버튼 묶음 맨 앞의 접기 버튼(디자인 `shrink` 글리프). 창 위로 펼친 팝업에만 붙는다.
    pub collapse: bool,
    /// close 왼쪽의 전체화면 버튼(디자인 `fit` 글리프).
    pub fullscreen: bool,
    /// 타이틀바 우측 끝의 X.
    pub close: bool,
}

impl TitleButtons {
    /// 버튼 없음 — 이 헬퍼를 쓰는 기존 popup frame 데모의 기본값.
    pub const NONE: Self = Self {
        collapse: false,
        fullscreen: false,
        close: false,
    };
    /// X 만.
    pub const CLOSE: Self = Self {
        collapse: false,
        fullscreen: false,
        close: true,
    };
    /// 전체화면 + X — 무대를 선언한 popup(현재 `notifications`).
    pub const FULLSCREEN_AND_CLOSE: Self = Self {
        collapse: false,
        fullscreen: true,
        close: true,
    };
}

/// 타이틀바 제목을 본체와 같은 글꼴(`popup_title_font`)과 대칭 영역 계산으로 스트립 가운데에 그린다. 넘치면 본체와
/// 같이 말줄임하고, 잘렸을 때만 제목 띠를 돌려준다([`title_tooltip`]에 넘긴다).
pub fn draw_title_text(
    painter: &egui::Painter,
    theme: &Theme,
    title_rect: egui::Rect,
    buttons_left_x: f32,
    title: &str,
) -> Option<egui::Rect> {
    let font = popup_title_font(theme);
    let area = popup_title_text_rect(title_rect, buttons_left_x, theme.popup_title_text_gap());
    let shown = elide_popup_title(painter.ctx(), title, font.clone(), area.width());
    let cut = shown != title;
    painter.with_clip_rect(area).text(
        area.center(),
        egui::Align2::CENTER_CENTER,
        shown,
        font,
        theme.text_primary().into(),
    );
    cut.then_some(area)
}

/// 잘린 제목(`draw_title_text`가 돌려준 띠)에 전체 제목 Tooltip을 붙인다. 본체처럼 띠 위에서
/// `tooltip-delay`만큼 머물면 뜨고, `open`이면 호버 없이 표시한다(specimen).
pub fn title_tooltip(
    ui: &egui::Ui,
    theme: &Theme,
    title: &str,
    band: Option<egui::Rect>,
    open: bool,
) {
    let Some(band) = band else { return };
    // 한 페이지에 타이틀바가 여러 개라 띠 위치로 버블·타이머를 구분한다.
    let id = egui::Id::new("gallery.popup_title_tip")
        .with(band.min.x.to_bits())
        .with(band.min.y.to_bits());
    let hovered = ui.rect_contains_pointer(band);
    // 강제 표시는 띠가 다 보일 때만 — 스크롤로 가려진 띠의 버블이 창 안으로 당겨져 떠 있지 않게.
    let forced = open && ui.clip_rect().contains_rect(band);
    if forced || tooltip_hover_delay_elapsed(ui.ctx(), theme, id, hovered) {
        show_popup_title_tooltip(ui.ctx(), theme, id, title, band);
    }
}

/// 타이틀바 버튼을 painter로 그리고 버튼 영역의 왼쪽 끝을 반환한다. 본체 `PopupManager`와
/// 같은 토큰(`popup-title-btn-size`·`-btn-gap`·`-edge-inset`)으로 배치한다.
/// 버튼은 IconButton sm 규칙(호버·누름 배경, 글리프 색)으로 그리고 포인터 위치로 상태를 정한다.
/// 제목은 이 경계를 넘지 않도록 줄여야 한다.
pub fn draw_title_buttons(
    ctx: &egui::Context,
    painter: &egui::Painter,
    theme: &Theme,
    title_rect: egui::Rect,
    buttons: TitleButtons,
) -> f32 {
    let size = theme.popup_title_btn_size();
    let close_rect = egui::Rect::from_center_size(
        egui::pos2(
            title_rect.max.x - (size.scaled(0.5) + theme.popup_title_edge_inset()).value(),
            title_rect.center().y,
        ),
        egui::Vec2::splat(size.value()),
    );
    let (hover_pos, down) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_down()));
    let button = |rect: egui::Rect, icon: icons::MockGlyph, name: &str| {
        let hovered = hover_pos.is_some_and(|p| rect.contains(p));
        let tint = paint_icon_button_state(
            painter,
            theme,
            rect,
            IconButtonState::ghost(hovered, hovered && down),
        );
        let glyph = ControlSize::Sm.icon_glyph(theme);
        let glyph_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(glyph));
        if let Err(err) = paint_popup_title_glyph(ctx, painter, icon, glyph_rect, tint) {
            tracing::warn!("gallery popup {name} glyph failed to load: {err}");
        }
    };
    // 오른쪽 끝(close 자리)부터 왼쪽으로 close · 전체화면 · 접기 순서로 놓는다. 앞 버튼이 없으면 그 자리를 쓴다.
    let step = (size + theme.popup_title_btn_gap()).value();
    let mut slot = close_rect;
    let mut left = title_rect.max.x;
    for (on, icon, name) in [
        (buttons.close, icons::CLOSE, "close"),
        (buttons.fullscreen, icons::FIT, "fit"),
        (buttons.collapse, icons::SHRINK, "shrink"),
    ] {
        if !on {
            continue;
        }
        if name == "shrink" {
            // 접기 버튼은 본체와 같은 공용 위젯이다(접근 이름·키 포커스·툴팁 포함).
            let hovered = hover_pos.is_some_and(|p| slot.contains(p));
            let out = tasty_ui_widgets::popup_collapse_button(
                ctx,
                painter,
                theme,
                tasty_ui_widgets::CollapseButton {
                    id: egui::Id::new("gallery.popup_collapse").with(slot.min.x.to_bits()),
                    layer_id: painter.layer_id(),
                    rect: slot,
                    label: crate::i18n::t("popup.collapse_button.label"),
                    tooltip: crate::i18n::t("popup.collapse_button.tooltip"),
                    hovered,
                    pressed: hovered && down,
                    takes_keys: true,
                },
            );
            if let Some(err) = out.glyph_error {
                tracing::warn!("gallery popup {name} glyph failed to load: {err}");
            }
        } else {
            button(slot, icon, name);
        }
        left = slot.min.x;
        slot = slot.translate(egui::vec2(-step, 0.0));
    }
    left
}

/// 콘텐츠 영역 inset (호출부별 차이를 명시).
#[derive(Clone, Copy)]
pub struct ContentInset {
    /// 좌우 가로 inset (px).
    pub horizontal: f32,
    /// 제목바 아래 콘텐츠 상단에 추가로 더하는 여백 (px).
    pub top_extra: f32,
}

impl ContentInset {
    /// approval / dialog 변형 — 좌우 8px, 상단 +4px.
    pub const INSET: Self = Self {
        horizontal: 8.0,
        top_extra: 4.0,
    };
    /// convert / file_handler_picker 변형 — 좌우 flush, 상단 추가 없음.
    pub const FLUSH: Self = Self {
        horizontal: 0.0,
        top_extra: 0.0,
    };
}

/// 호출자가 지정한 높이·여백·그림자로 프레임을 그린다.
/// paint는 콘텐츠 영역의 child Ui를 받는다. 그림자가 없는 표본도 허용한다.
#[allow(clippy::too_many_arguments)] // reason: popup frame 은 chrome 파라미터가 본래 많다.
pub fn draw(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    width: LogicalPx,
    total_h: LogicalPx,
    inset: ContentInset,
    buttons: TitleButtons,
    shadow: Option<tasty_type_appearance::theme::ShadowToken>,
    paint: impl FnOnce(&mut egui::Ui),
) {
    draw_frame(
        ui,
        theme,
        title,
        width,
        total_h,
        FrameStyle {
            inset,
            buttons,
            shadow,
            panel: false,
        },
        paint,
    );
}

/// Flush bg-panel content with the host notification popup's sidebar title bar.
pub fn draw_panel(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    width: LogicalPx,
    total_h: LogicalPx,
    buttons: TitleButtons,
    paint: impl FnOnce(&mut egui::Ui),
) {
    draw_frame(
        ui,
        theme,
        title,
        width,
        total_h,
        FrameStyle {
            inset: ContentInset::FLUSH,
            buttons,
            shadow: Some(theme.shadow_modal()),
            panel: true,
        },
        paint,
    );
}

struct FrameStyle {
    inset: ContentInset,
    buttons: TitleButtons,
    shadow: Option<tasty_type_appearance::theme::ShadowToken>,
    panel: bool,
}

fn draw_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    width: LogicalPx,
    total_h: LogicalPx,
    style: FrameStyle,
    paint: impl FnOnce(&mut egui::Ui),
) {
    let FrameStyle {
        inset,
        buttons,
        shadow,
        panel,
    } = style;
    let (frame_rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), total_h.value()),
        egui::Sense::hover(),
    );
    let painter = if panel {
        ui.painter().clone()
    } else {
        ui.painter_at(frame_rect)
    };

    let bg: egui::Color32 = if panel {
        theme.bg_panel()
    } else {
        theme.surface_raised()
    }
    .into();
    let title_bg: egui::Color32 = if panel {
        theme.bg_sidebar()
    } else {
        theme.surface_hover()
    }
    .into();
    let border: egui::Color32 = theme.border_frame().into();

    if let Some(shadow) = shadow {
        painter.add(
            shadow
                .to_egui()
                .as_shape(frame_rect, theme.corner_radius.value()),
        );
    }
    painter.rect_filled(frame_rect, theme.corner_radius.value(), bg);
    painter.rect_stroke(
        frame_rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), border),
        egui::StrokeKind::Inside,
    );

    let title_rect = egui::Rect::from_min_size(
        frame_rect.min,
        egui::vec2(frame_rect.width(), TITLE_BAR_HEIGHT.value()),
    );
    painter.rect_filled(
        title_rect,
        egui::CornerRadius {
            nw: theme.corner_radius.value() as u8,
            ne: theme.corner_radius.value() as u8,
            sw: 0,
            se: 0,
        },
        title_bg,
    );
    let buttons_left = draw_title_buttons(ui.ctx(), &painter, theme, title_rect, buttons);
    let cut_band = draw_title_text(&painter, theme, title_rect, buttons_left, title);
    title_tooltip(ui, theme, title, cut_band, false);

    if panel {
        painter.hline(
            title_rect.x_range(),
            title_rect.bottom(),
            egui::Stroke::new(theme.border_width.value(), border),
        );
    }
    let vertical = if panel {
        LogicalPx(0.0)
    } else {
        CONTENT_MARGIN
    };
    let content_top = LogicalPx(title_rect.bottom()) + vertical;
    let content_rect = egui::Rect::from_min_max(
        egui::pos2(
            frame_rect.min.x + inset.horizontal,
            content_top.value() + inset.top_extra,
        ),
        egui::pos2(
            frame_rect.max.x - inset.horizontal,
            frame_rect.max.y - vertical.value(),
        ),
    );
    // 상위 가로 레이아웃을 상속하지 않도록 콘텐츠의 세로 방향을 명시한다.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    paint(&mut child);
}
