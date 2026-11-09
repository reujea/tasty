//! 본체와 갤러리가 공유하는 토스트 카드 그리기와 페이드 계산.
//! 수명 관리·중복 합치기·범위 조회는 호출자가 맡고 계산된 불투명도와 영역을 전달한다.

use std::time::Duration;

use tasty_type_appearance::theme::Theme;
use tasty_type_appearance::toast_kind::ToastKind;

use crate::chip::{KbdKey, kbd_parts_width, kbd_text_parts_painted};
use crate::tokens::{
    TOAST_GAP, TOAST_HINT_GAP as HINT_GAP, TOAST_MIN_INNER_WIDTH as MIN_TOAST_INNER_WIDTH,
    TOAST_PADDING_X as PADDING_X, TOAST_PADDING_Y as PADDING_Y, TOAST_SCOPE_MARGIN as SCOPE_MARGIN,
    TOAST_STACK_OFFSET_BOTTOM,
};

/// 등장 페이드 시간(ms).
pub const FADE_IN_MS: f32 = 80.0;
/// 소멸 페이드 시간(ms). 부르는 쪽이 "언제까지 살려 둘지" 를 이 값으로 정하므로 공개한다.
pub const FADE_OUT_MS: f32 = 160.0;

/// 그릴 토스트의 데이터. 불투명도는 호출자가 미리 계산한다.
#[derive(Clone, Debug)]
pub struct ToastEntryView {
    pub kind: ToastKind,
    pub message: String,
    /// 알림을 낸 동작의 단축키 키캡(표시 문자열). 비어 있으면 hint를 그리지 않는다.
    /// 메뉴·마우스로 실행했고 binding이 있을 때만 호출자가 채운다.
    pub hint: Vec<String>,
    /// [0.0, 1.0] — 0 이면 스킵.
    pub alpha: f32,
}

/// 스택 맨 아래 카드를 스코프 하단에서 띄우는 기준.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastStackBottom {
    /// pane·surface처럼 화면 일부를 덮는 스코프. 가장자리 여백(space-md)만 둔다.
    #[default]
    ScopeMargin,
    /// 메인 창 전체를 덮는 스코프. 창 하단에서 `toast-stack-offset-bottom`(배율 없음)만큼 띄워
    /// 상태바 위에 쌓는다.
    Window,
    /// Settings 창 전체를 덮는 스코프. 창 하단에서 `toast-stack-offset-bottom-settings`(배율
    /// 적용)만큼 띄워 하단 버튼 줄(Cancel · Save) 위에 쌓는다.
    SettingsWindow,
}

/// 스택 맨 아래 카드의 오른쪽 아래 모서리. 같은 점에 놓이는 스택은 호출자가 한 열로 합친다.
pub fn stack_anchor(th: &Theme, scope_rect: egui::Rect, bottom: ToastStackBottom) -> egui::Pos2 {
    let bottom_offset = match bottom {
        ToastStackBottom::ScopeMargin => SCOPE_MARGIN,
        ToastStackBottom::Window => TOAST_STACK_OFFSET_BOTTOM,
        ToastStackBottom::SettingsWindow => th.toast_stack_offset_bottom_settings().value(),
    };
    egui::pos2(
        scope_rect.max.x - SCOPE_MARGIN,
        scope_rect.max.y - bottom_offset,
    )
}

/// 생성 순서(ID 오름차순)의 토스트 목록. 역순으로 그려 최신 항목을 오른쪽 아래에 놓는다.
#[derive(Clone, Debug)]
pub struct ToastScopeView {
    pub scope_rect: egui::Rect,
    pub bottom: ToastStackBottom,
    pub entries: Vec<ToastEntryView>,
}

/// 전체 scope 의 그룹 리스트 + theme.
pub struct ToastViewProps<'a> {
    pub theme: &'a Theme,
    pub scopes: &'a [ToastScopeView],
}

/// kind → 좌측 accent 바 색.
pub fn accent_color(kind: ToastKind, th: &Theme) -> egui::Color32 {
    match kind {
        ToastKind::Info => th.toast_accent_info().into(),
        ToastKind::Success => th.toast_accent_success().into(),
        ToastKind::Warning => th.toast_accent_warning().into(),
        ToastKind::Error => th.toast_accent_danger().into(),
    }
}

/// 생성 후 경과 시간과 수명으로 페이드 불투명도를 계산한다.
pub fn fade_alpha(age: Duration, lifetime: Duration, reduced_motion: bool) -> f32 {
    let age_ms = age.as_secs_f32() * 1000.0;
    let life_ms = lifetime.as_secs_f32() * 1000.0;

    if reduced_motion {
        return if age_ms < life_ms { 1.0 } else { 0.0 };
    }

    if age_ms < FADE_IN_MS {
        (age_ms / FADE_IN_MS).clamp(0.0, 1.0)
    } else if age_ms < life_ms {
        1.0
    } else {
        let fade_out = (age_ms - life_ms) / FADE_OUT_MS;
        (1.0 - fade_out).clamp(0.0, 1.0)
    }
}

/// 호출자가 선택한 레이어의 painter에 토스트를 그린다. 사용자 입력이나 동작은 처리하지 않는다.
/// 그린 카드의 사각형을 스코프 경계로 자른 값으로 돌려준다. 호출자는 이 영역과 겹치는
/// native 화면을 카드가 떠 있는 동안 숨길 수 있다.
pub fn draw_toast_scopes(painter: &egui::Painter, props: &ToastViewProps<'_>) -> Vec<egui::Rect> {
    let th = props.theme;
    let ctx = painter.ctx().clone();
    let mut drawn = Vec::new();

    for scope in props.scopes {
        let scope_rect = scope.scope_rect;
        // 토스트가 이웃 영역을 덮지 않도록 스코프 경계로 자른다.
        let painter = painter.with_clip_rect(scope_rect);
        let anchor = stack_anchor(th, scope_rect, scope.bottom);
        let mut cursor_y = anchor.y;

        // 새것부터 그리며 위로 올라간다 (id 오름차순으로 받았으므로 reverse).
        for entry in scope.entries.iter().rev() {
            let alpha = entry.alpha;
            if alpha <= 0.0 {
                continue;
            }

            // 카드마다 내용 폭을 쓰고 상한은 `toast_max_width`다. 공유 스택 폭이 없어 카드가
            // 들고 나도 다른 카드가 다시 흐르지 않는다. 좁은 영역에서는 왼쪽 여백도 넘지 않는다.
            let inner_limit = (scope_rect.width() - SCOPE_MARGIN * 2.0).max(MIN_TOAST_INNER_WIDTH);
            let max_width = th.toast_max_width.value().min(inner_limit);
            let (galley, size) =
                layout_card(&ctx, th, entry.message.clone(), &entry.hint, max_width);
            let (toast_w, toast_h) = (size.x, size.y);

            let max_x = anchor.x;
            let bottom_y = cursor_y;
            let top_y = bottom_y - toast_h;
            // 위쪽 경계를 넘는 카드부터는 그리지 않는다.
            if top_y < scope_rect.min.y {
                break;
            }
            let left_x = max_x - toast_w;

            let rect =
                egui::Rect::from_min_max(egui::pos2(left_x, top_y), egui::pos2(max_x, bottom_y));

            draw_card(
                &painter,
                th,
                rect,
                card_colors(th, entry.kind, alpha),
                galley,
                &entry.hint,
                alpha,
            );
            drawn.push(rect.intersect(scope_rect));

            cursor_y = top_y - TOAST_GAP;
        }
    }
    drawn
}

/// 배경·테두리·본문색은 alpha를 적용한 뒤 egui 색으로 변환한다. 강조색은 변환 후 곱한다.
/// 변환 뒤 Color32::gamma_multiply를 적용하면 연산 공간이 달라 결과가 달라질 수 있다.
pub fn card_colors(theme: &Theme, kind: ToastKind, alpha: f32) -> CardColors {
    CardColors {
        bg: theme.surface_raised().gamma_multiply(alpha).into(),
        // toast 보더 — canonical `toast-border`.
        border: theme.toast_border().gamma_multiply(alpha).into(),
        accent: accent_color(kind, theme).gamma_multiply(alpha),
        text: theme.toast_fg().gamma_multiply(alpha).into(),
    }
}

/// hint 키캡이 차지하는 폭과 본문 사이 간격의 합. hint가 없으면 0이다.
fn hint_reserve(ctx: &egui::Context, theme: &Theme, hint: &[String]) -> f32 {
    if hint.is_empty() {
        return 0.0;
    }
    let keys: Vec<KbdKey<'_>> = hint.iter().map(|k| KbdKey::Text(k)).collect();
    kbd_parts_width(ctx, theme, &keys).value() + HINT_GAP
}

/// 카드 폭 상한에 맞춰 본문과 크기를 계산한다. 상한은 호출자가 정한다.
/// hint는 줄지 않으므로 본문 줄바꿈 폭에서 먼저 뺀다(본문이 먼저 줄바꿈된다).
/// 본문은 고정색으로 배치해 페이드하지 않고 배경·테두리·강조 막대만 페이드한다.
pub fn layout_card(
    ctx: &egui::Context,
    theme: &Theme,
    message: String,
    hint: &[String],
    max_width: f32,
) -> (std::sync::Arc<egui::Galley>, egui::Vec2) {
    let accent_w = theme.toast_accent_width.value();
    let reserve = hint_reserve(ctx, theme, hint);
    // wrap_width 음수 방지(스코프 클램프로 max_width 가 작아질 때).
    let wrap_width = (max_width - PADDING_X * 2.0 - accent_w - reserve).max(1.0);
    // 본문의 백틱 구간은 code run 이다.
    let job = crate::ui_code::ui_copy_job(
        theme,
        &message,
        theme.font_size_body,
        theme.text_primary().into(),
        wrap_width,
    );
    let galley = ctx.fonts(|f| f.layout_job(job));
    let text_w = crate::ui_code::ui_copy_size(theme, &galley).x;
    let toast_w = (text_w + PADDING_X * 2.0 + accent_w + reserve).min(max_width);
    let hint_h = if hint.is_empty() {
        0.0
    } else {
        theme.kbd_size().value()
    };
    let toast_h = galley.size().y.max(hint_h) + PADDING_Y * 2.0;
    (galley, egui::vec2(toast_w, toast_h))
}

/// 스코프 없이 단일 카드를 그린다. 본체 스택과 같은 크기·색 계산을 사용한다.
pub fn draw_single_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    kind: ToastKind,
    message: &str,
    hint: &[String],
    alpha: f32,
) -> egui::Response {
    let (galley, size) = layout_card(
        ui.ctx(),
        theme,
        message.to_string(),
        hint,
        theme.toast_max_width.value(),
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    draw_card(
        &painter,
        theme,
        rect,
        card_colors(theme, kind, alpha),
        galley,
        hint,
        alpha,
    );
    response
}

/// 카드 chrome 색 묶음 (부르는 쪽이 alpha 반영 후 최종 색을 채워 넘긴다).
#[derive(Clone, Copy)]
pub struct CardColors {
    /// 카드 배경.
    pub bg: egui::Color32,
    /// 카드 border.
    pub border: egui::Color32,
    /// 좌측 accent bar.
    pub accent: egui::Color32,
    /// galley 텍스트 색 (galley 자체도 이 색으로 layout 돼 있어야 한다).
    pub text: egui::Color32,
}

/// 지정된 사각형 안에 카드의 배경·테두리·강조 막대와 본문, hint 키캡을 그린다.
/// hint는 오른쪽 끝에서 본문 첫 줄의 세로 중심에 맞춘다. 키캡 색에도 `alpha`를 곱한다.
pub fn draw_card(
    painter: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    colors: CardColors,
    galley: std::sync::Arc<egui::Galley>,
    hint: &[String],
    alpha: f32,
) {
    painter.rect_filled(rect, theme.corner_radius.value(), colors.bg);
    painter.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), colors.border),
        egui::StrokeKind::Inside,
    );

    let accent_w = theme.toast_accent_width.value();
    let bar_rect =
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + accent_w, rect.max.y));
    let bar_radius = egui::CornerRadius {
        nw: theme.corner_radius.value() as u8,
        sw: theme.corner_radius.value() as u8,
        ne: 0,
        se: 0,
    };
    painter.rect_filled(bar_rect, bar_radius, colors.accent);

    let text_pos = egui::pos2(rect.min.x + accent_w + PADDING_X, rect.min.y + PADDING_Y);
    if !hint.is_empty() {
        let first_line_center = galley
            .rows
            .first()
            .map_or(galley.size().y * 0.5, |row| row.rect.center().y);
        let keys: Vec<&str> = hint.iter().map(String::as_str).collect();
        kbd_text_parts_painted(
            painter,
            theme,
            &keys,
            rect.max.x - PADDING_X,
            text_pos.y + first_line_center,
            alpha,
        );
    }
    // 카드가 surface-raised 라 code run 채움은 한 단계 다른 bg-panel 이다.
    crate::ui_code::paint_ui_copy(
        painter,
        theme,
        crate::ui_code::UiCodeContainer::Raised,
        text_pos,
        galley,
    );
}

/// 반환한 사각형이 실제로 그린 카드와 같고, 그리지 않은 카드(alpha 0)는 빠지며, 모든 값이
/// 스코프 안에 있는지 검사한다. 호스트는 이 값으로 카드와 겹치는 native WebView만 숨긴다.
#[cfg(test)]
mod card_rect_tests {
    use super::{
        ToastEntryView, ToastScopeView, ToastStackBottom, ToastViewProps, card_colors,
        draw_toast_scopes,
    };
    use egui::{Pos2, RawInput, Rect, vec2};
    use tasty_type_appearance::toast_kind::ToastKind;

    fn entry(message: &str, alpha: f32) -> ToastEntryView {
        ToastEntryView {
            kind: ToastKind::Info,
            message: message.into(),
            hint: Vec::new(),
            alpha,
        }
    }

    /// 두 프레임을 그려 마지막 프레임의 반환값과 카드 배경 사각형을 돌려준다.
    fn draw(scope_rect: Rect, entries: Vec<ToastEntryView>) -> (Vec<Rect>, Vec<Rect>) {
        let theme = tasty_themes::mocha_fallback();
        let bg = card_colors(&theme, ToastKind::Info, 1.0).bg;
        let (returned, shapes) = paint(&theme, scope_rect, entries);
        let painted: Vec<Rect> = shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::epaint::Shape::Rect(r) if r.fill == bg => Some(r.rect),
                _ => None,
            })
            .collect();
        (returned, painted)
    }

    /// 두 프레임을 그려 마지막 프레임의 반환값과 도형을 돌려준다.
    fn paint(
        theme: &tasty_type_appearance::theme::Theme,
        scope_rect: Rect,
        entries: Vec<ToastEntryView>,
    ) -> (Vec<Rect>, Vec<egui::epaint::ClippedShape>) {
        let ctx = egui::Context::default();
        let scopes = [ToastScopeView {
            scope_rect,
            bottom: ToastStackBottom::ScopeMargin,
            entries,
        }];
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
        let mut returned = Vec::new();
        let mut shapes = Vec::new();
        // 첫 프레임은 폰트 준비 전이라 두 번 돌린다.
        for _ in 0..2 {
            let out = ctx.run(
                RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Tooltip,
                        egui::Id::new("toast_card_rects"),
                    ));
                    returned = draw_toast_scopes(
                        &painter,
                        &ToastViewProps {
                            theme,
                            scopes: &scopes,
                        },
                    );
                },
            );
            shapes = out.shapes;
        }
        (returned, shapes)
    }

    /// 끊을 곳 없는 긴 값도 줄이 줄바꿈 폭을 넘지 않고 카드가 `toast-max-width` 를 넘지 않는다.
    #[test]
    fn an_unbroken_long_value_wraps_inside_the_card() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        // 폰트 상태는 첫 pass 에서 채워진다. 빈 pass 의 산출물은 볼 것이 없다.
        let _unused = ctx.run(RawInput::default(), |_| {});
        let max = theme.toast_max_width.value();
        let value = "a".repeat(100);
        let message = format!("Could not load the font: {value}. Using another font.");
        let (galley, size) = super::layout_card(&ctx, &theme, message, &[], max);
        assert!(size.x <= max, "카드가 상한을 넘었다: {size:?}");
        assert!(galley.rows.len() > 1, "줄을 바꾸지 않았다");
        let wrap = galley.job.wrap.max_width;
        for row in &galley.rows {
            assert!(row.rect.width() <= wrap, "줄이 폭을 넘었다: {:?}", row.rect);
        }
    }

    /// 카드가 surface-raised 라 code run 채움은 `ui-code-bg-on-raised` 다. 두 테마 모두
    /// 카드 색(`ui-code-bg` 와 같은 surface-raised)과 다르다.
    #[test]
    fn a_code_run_on_the_card_uses_the_on_raised_fill() {
        let latte = {
            let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT)
                .expect("builtin latte.toml ships valid");
            let (partial, is_light) = file.to_partial();
            let mut colors = tasty_themes::mocha_fallback_colors();
            colors.apply_partial(&partial);
            tasty_type_appearance::theme::Theme::with_colors(colors, is_light.unwrap_or(true))
        };
        for theme in [tasty_themes::mocha_fallback(), latte] {
            let on_raised: egui::Color32 = theme.ui_code_bg_on_raised().into();
            let panel_fill: egui::Color32 = theme.ui_code_bg().into();
            assert_ne!(on_raised, panel_fill);
            let scope = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
            let (_, shapes) = paint(
                &theme,
                scope,
                vec![entry("Run `tasty agent task-list` now", 1.0)],
            );
            let fills: Vec<egui::Color32> = shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::epaint::Shape::Rect(r) => Some(r.fill),
                    _ => None,
                })
                .collect();
            assert!(fills.contains(&on_raised), "{fills:?}");
            // surface-raised 는 카드 배경 하나뿐이다. run 채움이 같은 색이면 둘이 된다.
            let raised = fills.iter().filter(|f| **f == panel_fill).count();
            assert_eq!(raised, 1, "{fills:?}");
        }
    }

    fn sorted(mut rects: Vec<Rect>) -> Vec<Rect> {
        rects.sort_by(|a, b| a.top().total_cmp(&b.top()));
        rects
    }

    #[test]
    fn returned_rects_match_the_painted_cards() {
        let scope = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
        let (returned, painted) = draw(scope, vec![entry("first", 1.0), entry("second", 1.0)]);
        assert_eq!(returned.len(), 2, "{returned:?}");
        assert_eq!(sorted(returned), sorted(painted));
    }

    #[test]
    fn faded_out_cards_are_not_returned() {
        let scope = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
        let (returned, _) = draw(scope, vec![entry("gone", 0.0), entry("shown", 1.0)]);
        assert_eq!(returned.len(), 1, "{returned:?}");
    }

    #[test]
    fn returned_rects_stay_inside_the_scope() {
        // 카드 최소 폭보다 좁은 스코프라 자르지 않으면 카드가 왼쪽 경계 밖으로 나간다.
        let scope = Rect::from_min_size(egui::pos2(600.0, 60.0), vec2(40.0, 400.0));
        let (returned, _) = draw(scope, vec![entry("ok", 1.0)]);
        assert!(!returned.is_empty());
        for r in &returned {
            assert!(scope.contains_rect(*r), "{r:?} not inside {scope:?}");
        }
    }

    #[test]
    fn no_entries_return_nothing() {
        let scope = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
        let (returned, _) = draw(scope, Vec::new());
        assert!(returned.is_empty());
    }
}
