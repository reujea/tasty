//! `MenuItem` — 메뉴/팔레트 행 (디자인 `components/navigation/MenuItem`).
//!
//! 전체폭 행: [icon] label [shortcut]. height control-height(28), pad space-md.
//! hover overlay-hover, active surface-active, danger accent-danger. 아이콘은
//! 호출측 [`IconPainter`] 로 주입(icon-size-md=16, text-muted / danger 면 accent-danger).

use tasty_type_appearance::theme::Theme;

use crate::icon_button::IconPainter;

/// MenuItem variant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuItemVariant {
    /// 글자가 `menu-item-fg`이고 호버·선택 때 `menu-item-fg-hover`로 바뀐다.
    Normal,
    Danger,
}

/// 일반 메뉴의 텍스트 단축키와 팔레트의 키캡 단축키를 구분한다.
enum Shortcut<'a> {
    Text(&'a str),
    Keys(&'a [&'a str]),
}

/// 전체폭 메뉴 행. `active` 면 surface-active 배경(현재 선택). 클릭 응답 반환.
#[allow(clippy::too_many_arguments)] // reason: 디자인 MenuItem 스펙(icon/label/shortcut/variant/active/enabled) 1:1 매핑, 인위적 그룹핑 불필요
pub fn menu_item(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    shortcut: Option<&str>,
    variant: MenuItemVariant,
    active: bool,
    enabled: bool,
) -> egui::Response {
    menu_item_inner(
        ui,
        theme,
        icon,
        label,
        shortcut.map(Shortcut::Text),
        variant,
        active,
        enabled,
        false,
        None,
    )
}

/// [`menu_item`] 에 호버 상태를 직접 지정한다. `hovered` 가 참이면 포인터가 없어도
/// 실제 호버와 같은 배경(`menu-item-bg-hover`)과 글자(`menu-item-fg-hover`)로 그린다.
/// 상태 견본처럼 포인터 없이 호버 모습을 보여 줘야 하는 곳에서 쓴다.
#[allow(clippy::too_many_arguments)] // reason: [`menu_item`] 과 같은 스펙에 호버 상태 하나를 더한다
pub fn menu_item_with_hover(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    shortcut: Option<&str>,
    variant: MenuItemVariant,
    active: bool,
    enabled: bool,
    hovered: bool,
) -> egui::Response {
    menu_item_inner(
        ui,
        theme,
        icon,
        label,
        shortcut.map(Shortcut::Text),
        variant,
        active,
        enabled,
        hovered,
        None,
    )
}

/// 단축키를 키캡으로 그린다. + 키를 구분자와 혼동하지 않도록 이미 나눈 목록을 받는다.
/// 비활성 행에서도 키캡은 흐려지지 않는다.
#[allow(clippy::too_many_arguments)] // reason: [`menu_item`] 과 같은 스펙, 단축키 표현만 다르다
pub fn menu_item_kbd(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    keys: &[&str],
    variant: MenuItemVariant,
    active: bool,
    enabled: bool,
) -> egui::Response {
    menu_item_inner(
        ui,
        theme,
        icon,
        label,
        (!keys.is_empty()).then_some(Shortcut::Keys(keys)),
        variant,
        active,
        enabled,
        false,
        None,
    )
}

/// [`menu_item_kbd`] 에 호버 상태를 직접 지정한다. 호버 견본에서 쓰며 키캡 모양은 바뀌지 않는다.
#[allow(clippy::too_many_arguments)] // reason: [`menu_item_kbd`] 와 같은 스펙에 호버 상태 하나를 더한다
pub fn menu_item_kbd_with_hover(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    keys: &[&str],
    variant: MenuItemVariant,
    active: bool,
    enabled: bool,
    hovered: bool,
) -> egui::Response {
    menu_item_inner(
        ui,
        theme,
        icon,
        label,
        (!keys.is_empty()).then_some(Shortcut::Keys(keys)),
        variant,
        active,
        enabled,
        hovered,
        None,
    )
}

/// 선택 목록(Select · egui ComboBox 열린 목록)의 옵션 행. `selected` 면 현재 값이다 —
/// 글자는 `menu-item-selected-fg`, 오른쪽 끝에 `menu-item-check-fg` 체크를 그리고 채움은 없다.
/// 채움은 호버(`menu-item-bg-hover`)에만 쓴다.
pub fn menu_option(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    selected: bool,
) -> egui::Response {
    menu_option_icon(ui, theme, None, label, selected)
}

/// 앞에 아이콘이 있는 [`menu_option`].
pub fn menu_option_icon(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    selected: bool,
) -> egui::Response {
    // 열린 목록은 트리거 폭을 최소 폭으로 받고 내용에 맞춰 커진다. 행이 가용 폭만 쓰면 목록이 트리거 폭에
    // 묶여 라벨이 잘리므로, 라벨을 줄이지 않고 그리는 폭만큼 목록의 최소 폭을 넓힌다. 체크 자리는 선택과
    // 관계없이 모든 옵션에 잡아 선택이 바뀌어도 목록 폭이 같다.
    ui.set_min_width(menu_option_natural_width(
        ui.ctx(),
        theme,
        icon.is_some(),
        label,
    ));
    menu_item_inner(
        ui,
        theme,
        icon,
        label,
        None,
        MenuItemVariant::Normal,
        false,
        true,
        false,
        Some(selected),
    )
}

/// 폭을 호출자가 정하는 목록(SearchableSelect)의 옵션 행. [`menu_option`] 과 같은 모습에 키보드
/// 활성(`active`), 행 비활성(`enabled`), 강제 호버(`hovered`)를 더하고 목록의 최소 폭은 넓히지 않는다.
pub(crate) fn menu_option_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    selected: bool,
    active: bool,
    enabled: bool,
    hovered: bool,
) -> egui::Response {
    menu_item_inner(
        ui,
        theme,
        None,
        label,
        None,
        MenuItemVariant::Normal,
        active,
        enabled,
        hovered,
        Some(selected),
    )
}

/// 옵션 행을 줄이지 않고 그리는 폭 — 행 패딩 양쪽 + (아이콘 + 간격) + 라벨 + 간격 + 체크.
pub(crate) fn menu_option_natural_width(
    ctx: &egui::Context,
    theme: &Theme,
    has_icon: bool,
    label: &str,
) -> f32 {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let label_w = ctx.fonts(|f| {
        f.layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER)
            .rect
            .width()
    });
    let gap = theme.spacing_sm.value();
    let icon_w = if has_icon {
        theme.icon_glyph_size_md.value() + gap
    } else {
        0.0
    };
    theme.menu_item_padding_x().value() * 2.0
        + icon_w
        + label_w.ceil()
        + gap
        + theme.menu_item_check_size().value()
}

/// [`menu_option`] 을 `egui::Ui::selectable_value` 처럼 쓴다. 누르면 `current` 를 `value` 로 바꾸고
/// 응답을 changed 로 표시한다.
pub fn menu_option_value<V: PartialEq>(
    ui: &mut egui::Ui,
    theme: &Theme,
    current: &mut V,
    value: V,
    label: &str,
) -> egui::Response {
    let mut resp = menu_option(ui, theme, label, *current == value);
    if resp.clicked() && *current != value {
        *current = value;
        resp.mark_changed();
    }
    resp
}

#[allow(clippy::too_many_arguments)] // reason: 공개 함수들이 공유하는 구현이며 그 함수들에 필요한 인자를 받는다.
fn menu_item_inner(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: Option<IconPainter<'_>>,
    label: &str,
    shortcut: Option<Shortcut<'_>>,
    variant: MenuItemVariant,
    active: bool,
    enabled: bool,
    force_hover: bool,
    // 선택 목록의 옵션 행이면 Some(현재 값인가). 옵션 행은 키보드 포커스 행을 active 로 그린다.
    option: Option<bool>,
) -> egui::Response {
    let selected = option == Some(true);
    let height = theme.menu_item_height().value();
    let pad_x = theme.menu_item_padding_x().value();
    // gap/label body/icon 글리프 = 대응 menu-item component 토큰 없음 → semantic.
    let gap = theme.spacing_sm.value();
    let radius = theme.menu_item_radius().value();
    let icon_glyph = theme.icon_glyph_size_md.value();
    let width = ui.available_width();

    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    // 옵션 행은 Tab 으로 포커스를 받고 Space/Enter 로 고른다. egui 선택 행처럼 포커스 행을 보이게
    // 키보드 active(surface-active) 채움으로 그린다. 선택 행이라도 채움은 포커스일 때만이다.
    let active = active || (option.is_some() && resp.has_focus());
    let hovered = enabled && (force_hover || resp.hovered());
    // disabled 항목의 아이콘·라벨·단축키 문구는 opacity 없이 disabled ink를 쓴다.
    let dim = |c: egui::Color32| {
        if enabled {
            c
        } else {
            theme.state_disabled_fg().to_egui()
        }
    };

    if active {
        ui.painter()
            .rect_filled(rect, radius, theme.surface_active().to_egui());
    } else if hovered {
        ui.painter().rect_filled(
            rect,
            radius,
            theme.menu_item_bg_hover().to_egui_premultiplied(),
        );
    }

    // 키보드 활성 행은 surface-active 위이므로 아이콘·단축키·danger 라벨도 같은 글자색이다.
    let active_part = active.then(|| theme.menu_item_part_fg_active().to_egui());
    let fg = match variant {
        MenuItemVariant::Normal if active || hovered => theme.menu_item_fg_hover().to_egui(),
        MenuItemVariant::Normal if selected => theme.menu_item_selected_fg().to_egui(),
        MenuItemVariant::Normal => theme.menu_item_fg().to_egui(),
        MenuItemVariant::Danger if hovered => theme.accent_danger_strong().to_egui(),
        MenuItemVariant::Danger => {
            active_part.unwrap_or_else(|| theme.accent_danger_raised().to_egui())
        }
    };
    let icon_color = active_part.unwrap_or_else(|| match variant {
        MenuItemVariant::Normal => theme.text_secondary().to_egui(),
        MenuItemVariant::Danger if hovered => theme.accent_danger_raised().to_egui(),
        MenuItemVariant::Danger => theme.accent_danger().to_egui(),
    });

    let mut x = rect.left() + pad_x;
    if let Some(paint) = icon {
        let irect = egui::Rect::from_center_size(
            egui::pos2(x + icon_glyph * 0.5, rect.center().y),
            egui::vec2(icon_glyph, icon_glyph),
        );
        paint(ui, irect, dim(icon_color));
        x += icon_glyph + gap;
    }

    let mut right = rect.right() - pad_x;
    // 체크는 단축키 뒤, 행 오른쪽 끝에 둔다.
    if selected {
        let size = theme.menu_item_check_size().value();
        let crect = egui::Rect::from_center_size(
            egui::pos2(right - size * 0.5, rect.center().y),
            egui::vec2(size, size),
        );
        tasty_icons::CHECK
            .image(
                size,
                dim(if hovered && !active {
                    theme.accent_primary_raised().to_egui()
                } else {
                    theme.menu_item_check_fg().to_egui()
                }),
            )
            .paint_at(ui, crect);
        right -= size + gap;
    }
    match shortcut {
        Some(Shortcut::Text(sc)) => {
            let g = ui.painter().layout_no_wrap(
                sc.to_owned(),
                egui::FontId::monospace(theme.menu_item_shortcut_font_size().value()),
                egui::Color32::PLACEHOLDER,
            );
            let pos = egui::pos2(
                right - g.rect.width(),
                rect.center().y - g.rect.height() * 0.5,
            );
            ui.painter().galley(
                pos,
                g.clone(),
                dim(active_part.unwrap_or_else(|| {
                    if hovered {
                        theme.text_on_active().to_egui()
                    } else {
                        theme.text_muted_raised().to_egui()
                    }
                })),
            );
            right -= g.rect.width() + gap;
        }
        Some(Shortcut::Keys(keys)) => {
            let parts: Vec<crate::KbdKey<'_>> =
                keys.iter().map(|k| crate::KbdKey::Text(k)).collect();
            let key_ink = (!enabled).then(|| theme.state_disabled_fg().to_egui());
            let plus = key_ink.unwrap_or_else(|| {
                if active {
                    theme.kbd_plus_fg_active().to_egui()
                } else {
                    theme.kbd_plus_fg().to_egui()
                }
            });
            let w = crate::chip::kbd_parts_at_ink(
                ui,
                theme,
                &parts,
                right,
                rect.center().y,
                key_ink,
                plus,
            );
            right -= w.value() + gap;
        }
        None => {}
    }

    // 디자인 `.tasty-menuitem__label` 처럼 남은 폭을 넘는 라벨은 끝을 말줄임표로 줄인다.
    let ink = dim(fg);
    let g = menu_label_galley(ui, theme, label, ink, right - x);
    let pos = egui::pos2(x, rect.center().y - g.rect.height() * 0.5);
    ui.painter().galley(pos, g, ink);

    resp
}

/// 아이콘·단축키 없는 메뉴 행을 내용 폭에 맞춘 메뉴의 border-box 폭.
/// 가장 넓은 라벨 + 행 패딩(`menu-item-padding-x`) 양쪽 + 안쪽 고리(`popup-content-margin`) 양쪽
/// + 테두리 양쪽을 `min`..`max` 로 제한한다. 상한에 걸린 라벨은 [`menu_label_galley`] 가 끝을 줄인다.
pub fn fit_menu_width<'a>(
    ctx: &egui::Context,
    theme: &Theme,
    labels: impl IntoIterator<Item = &'a str>,
    min: f32,
    max: f32,
) -> f32 {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let widest = ctx.fonts(|f| {
        labels
            .into_iter()
            .map(|l| {
                f.layout_no_wrap(l.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                    .rect
                    .width()
            })
            .fold(0.0_f32, f32::max)
    });
    let chrome = theme.menu_item_padding_x().value() * 2.0
        + theme.popup_content_margin().value() * 2.0
        + theme.border_width.value() * 2.0;
    (widest.ceil() + chrome).clamp(min, max.max(min))
}

/// 한 줄 메뉴 라벨. `max_width` 를 넘으면 끝을 말줄임표로 줄인다.
pub fn menu_label_galley(
    ui: &egui::Ui,
    theme: &Theme,
    label: &str,
    color: egui::Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        color,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.painter().layout_job(job)
}

/// 메뉴 구분선 (디자인 `.tasty-menu-sep` — 1px `menu-separator`, 상하 space-xs).
pub fn menu_separator(ui: &mut egui::Ui, theme: &Theme) {
    let xs = theme.spacing_xs.value();
    ui.add_space(xs);
    let r = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().hline(
        r.x_range(),
        y,
        egui::Stroke::new(theme.border_width.value(), theme.menu_separator().to_egui()),
    );
    ui.add_space(xs);
}

/// 내용 폭 메뉴가 가장 넓은 라벨에 행 패딩·안쪽 고리·테두리를 더해 하한·상한으로 제한하는지,
/// 상한에 걸린 라벨의 끝이 줄어드는지 검사한다.
#[cfg(test)]
mod fit_width_tests {
    use super::{fit_menu_width, menu_label_galley};
    use egui::RawInput;
    use tasty_type_appearance::theme::Theme;

    const LONG: &str = "A plugin tool label that is far too long for any menu to show in full";
    /// 시험용 좁은 행 폭. LONG 라벨보다 좁다.
    const NARROW: f32 = 120.0;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    fn label_width(ctx: &egui::Context, th: &Theme, label: &str) -> f32 {
        ctx.fonts(|f| {
            f.layout_no_wrap(
                label.to_owned(),
                egui::FontId::proportional(th.font_size_body.value()),
                egui::Color32::PLACEHOLDER,
            )
            .rect
            .width()
        })
    }

    /// 폰트가 준비된 Context 에서 `f` 를 실행한다. 첫 프레임은 폰트 준비 전이라 두 번 돌린다.
    fn with_ui<R>(mut f: impl FnMut(&egui::Context, &egui::Ui) -> R) -> R {
        let ctx = egui::Context::default();
        let mut out = None;
        for _ in 0..2 {
            // 프레임 출력(그린 도형)은 이 검사에 필요 없다. 측정값은 `out`으로 받는다.
            let frame = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| out = Some(f(ctx, ui)));
            });
            drop(frame);
        }
        out.expect("frame ran")
    }

    #[test]
    fn menu_width_is_the_widest_row_clamped_between_min_and_max() {
        let th = theme();
        let (min, max) = (
            th.tools_menu_min_width().value(),
            th.tools_menu_max_width().value(),
        );
        let chrome = th.menu_item_padding_x().value() * 2.0
            + th.popup_content_margin().value() * 2.0
            + th.border_width.value() * 2.0;
        let (short, mid_label, mid, long) = with_ui(|ctx, _| {
            let mid_label = "Remote connections… extra";
            (
                fit_menu_width(ctx, &th, ["Git", "Presets"], min, max),
                label_width(ctx, &th, mid_label),
                fit_menu_width(ctx, &th, ["Git", mid_label], min, max),
                fit_menu_width(ctx, &th, ["Git", LONG], min, max),
            )
        });
        assert_eq!(short, min);
        assert!(
            mid_label.ceil() + chrome > min && mid_label.ceil() + chrome < max,
            "{mid_label} + {chrome} not in ({min}, {max})"
        );
        assert_eq!(mid, mid_label.ceil() + chrome);
        assert_eq!(long, max);
    }

    /// 공용 행이 남은 폭보다 넓은 라벨을 잘라 내지 않고 끝을 말줄임표로 줄여 행 안에 그리는지 검사한다.
    #[test]
    fn a_menu_item_row_ellipsizes_an_overflowing_label() {
        let th = theme();
        let ctx = egui::Context::default();
        let mut texts: Vec<(String, egui::Rect)> = Vec::new();
        let mut row_rect = egui::Rect::NOTHING;
        for _ in 0..2 {
            let frame = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.allocate_ui(egui::vec2(NARROW, NARROW), |ui| {
                        ui.set_max_width(NARROW);
                        row_rect = super::menu_item(
                            ui,
                            &th,
                            None,
                            LONG,
                            None,
                            super::MenuItemVariant::Normal,
                            false,
                            true,
                        )
                        .rect;
                    });
                });
            });
            texts = frame
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some((
                        t.galley
                            .rows
                            .iter()
                            .flat_map(|r| r.glyphs.iter().map(|g| g.chr))
                            .collect(),
                        t.visual_bounding_rect(),
                    )),
                    _ => None,
                })
                .collect();
        }
        let (text, rect) = texts
            .iter()
            .find(|(t, _)| t.starts_with('A'))
            .expect("label drawn");
        assert!(text.ends_with('…'), "{text}");
        assert!(
            rect.right() <= row_rect.right() - th.menu_item_padding_x().value() + 0.5,
            "{rect:?} past {row_rect:?}"
        );
    }

    /// 지정한 호버 상태로 행 하나를 그리고 (호버 배경 사각형 수, 라벨 글자색)을 돌려준다.
    /// 포인터는 화면 밖이라 실제 호버는 생기지 않는다.
    fn draw_row_with_hover(th: &Theme, hovered: bool) -> (usize, egui::Color32) {
        let ctx = egui::Context::default();
        let mut out = (0, egui::Color32::TRANSPARENT);
        for _ in 0..2 {
            let frame = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::menu_item_with_hover(
                        ui,
                        th,
                        None,
                        "Split pane",
                        None,
                        super::MenuItemVariant::Normal,
                        false,
                        true,
                        hovered,
                    );
                });
            });
            let bg = th.menu_item_bg_hover().to_egui_premultiplied();
            let fills = frame
                .shapes
                .iter()
                .filter(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.fill == bg))
                .count();
            let ink = frame
                .shapes
                .iter()
                .find_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some(t.fallback_color),
                    _ => None,
                })
                .expect("label drawn");
            out = (fills, ink);
        }
        out
    }

    /// 옵션 행 하나를 그리고 (채움 사각형 수, 라벨 글자색, 체크 도형 수)를 돌려준다.
    fn draw_option(th: &Theme, selected: bool) -> (usize, egui::Color32, usize) {
        let ctx = egui::Context::default();
        let mut out = (0, egui::Color32::TRANSPARENT, 0);
        for _ in 0..2 {
            let frame = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::menu_option(ui, th, "Minimize to background", selected);
                });
            });
            let fills = [
                th.menu_item_bg_hover().to_egui_premultiplied(),
                th.surface_active().to_egui(),
            ];
            let rects = frame
                .shapes
                .iter()
                .filter(|c| matches!(&c.shape, egui::Shape::Rect(r) if fills.contains(&r.fill)))
                .count();
            let ink = frame
                .shapes
                .iter()
                .find_map(|c| match &c.shape {
                    egui::Shape::Text(t) if t.galley.text() == "Minimize to background" => {
                        Some(t.fallback_color)
                    }
                    _ => None,
                })
                .expect("label drawn");
            // 시험 Context 에는 이미지 로더가 없어 체크 글리프는 메시 대신 대체 표시로 그려진다.
            // 라벨이 아닌 글자·메시 도형의 수로 체크를 센다.
            let meshes = frame
                .shapes
                .iter()
                .filter(|c| match &c.shape {
                    egui::Shape::Mesh(_) => true,
                    egui::Shape::Text(t) => t.galley.text() != "Minimize to background",
                    _ => false,
                })
                .count();
            out = (rects, ink, meshes);
        }
        out
    }

    /// 선택 옵션은 채움 없이 selected 글자와 오른쪽 체크로만 구분한다.
    #[test]
    fn a_selected_option_has_selected_ink_and_a_check_and_no_fill() {
        let th = theme();
        let (fills, ink, checks) = draw_option(&th, true);
        assert_eq!(fills, 0, "no fill on a selected row");
        assert_eq!(ink, th.menu_item_selected_fg().to_egui());
        assert_eq!(checks, 1, "one check glyph");
        let (fills, ink, checks) = draw_option(&th, false);
        assert_eq!(fills, 0);
        assert_eq!(ink, th.menu_item_fg().to_egui());
        assert_eq!(checks, 0);
    }

    /// 옵션 두 행을 그리고, `focus` 면 둘째(선택) 행에 키보드 포커스를 준 뒤
    /// (surface-active 채움 사각형 수, 둘째 행 rect 와 겹치는 채움인가)를 돌려준다.
    fn focused_option_fills(th: &Theme, focus: bool) -> (usize, bool) {
        let ctx = egui::Context::default();
        let mut out = (0, false);
        for frame_no in 0..3 {
            let mut second = egui::Rect::NOTHING;
            let frame = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    super::menu_option(ui, th, "Ask", false);
                    let resp = super::menu_option(ui, th, "Minimize to background", true);
                    if focus && frame_no == 0 {
                        resp.request_focus();
                    }
                    second = resp.rect;
                });
            });
            let active = th.surface_active().to_egui();
            let fills: Vec<egui::Rect> = frame
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Rect(r) if r.fill == active => Some(r.rect),
                    _ => None,
                })
                .collect();
            out = (fills.len(), fills.contains(&second));
        }
        out
    }

    /// 열린 목록의 키보드 포커스 행은 active(surface-active) 채움으로 보인다. 선택 행이라도
    /// 포커스가 없으면 채움이 없다.
    #[test]
    fn a_focused_option_row_shows_the_keyboard_active_fill() {
        let th = theme();
        assert_eq!(focused_option_fills(&th, true), (1, true));
        assert_eq!(focused_option_fills(&th, false), (0, false));
    }

    /// 트리거가 좁은 egui ComboBox 를 열어 둔 채 프레임을 돌리고, 목록에 그려진 `label` 행의 글자를 돌려준다.
    /// 열린 목록의 첫 프레임은 egui 가 트리거 폭을 최대 폭으로 주는 크기 측정 패스다.
    fn open_combo_row_text(th: &Theme, label: &str) -> String {
        let ctx = egui::Context::default();
        let mut text = String::new();
        for frame_no in 0..4 {
            let frame = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if frame_no == 0 {
                        // egui ComboBox 가 목록 열림 상태를 저장하는 id(버튼 id 의 "popup").
                        let id = ui.make_persistent_id(egui::Id::new("narrow")).with("popup");
                        ui.memory_mut(|m| m.open_popup(id));
                    }
                    // 본체 ComboBox 처럼 메뉴 틀(둘레 여백이 목록 폭 계산에 들어간다) 안에서 연다.
                    tasty_egui_theme::with_popover_frame(ui, th, |ui| {
                        egui::ComboBox::from_id_salt("narrow")
                            .selected_text("Ask")
                            .width(NARROW)
                            .show_ui(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                super::menu_option(ui, th, "Ask", true);
                                super::menu_option(ui, th, label, false);
                            })
                    });
                });
            });
            text = frame
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) if t.galley.text() == label => Some(
                        t.galley
                            .rows
                            .iter()
                            .flat_map(|r| r.glyphs.iter().map(|g| g.chr))
                            .collect::<String>(),
                    ),
                    _ => None,
                })
                .next_back()
                .unwrap_or_default();
        }
        text
    }

    /// 열린 목록은 트리거보다 긴 라벨에 맞춰 넓어진다. 행이 가용 폭만 쓰면 egui 크기 측정 패스가
    /// 목록 폭을 트리거 폭에 묶어 라벨이 말줄임표로 잘린다.
    #[test]
    fn an_open_list_widens_past_a_narrow_trigger_to_show_each_label() {
        let th = theme();
        let label = "Minimize to background";
        let full = with_ui(|ctx, _| label_width(ctx, &th, label));
        assert!(
            full > NARROW,
            "the label must be wider than the trigger: {full}"
        );
        assert_eq!(open_combo_row_text(&th, label), label);
    }

    /// 상태 견본이 포인터 없이 실제 호버와 같은 배경·글자로 그려지는지 검사한다.
    #[test]
    fn a_forced_hover_row_paints_the_hover_fill_and_hover_ink() {
        let th = theme();
        let (fills, ink) = draw_row_with_hover(&th, true);
        assert_eq!(fills, 1, "hover fill");
        assert_eq!(ink, th.menu_item_fg_hover().to_egui());
        let (fills, ink) = draw_row_with_hover(&th, false);
        assert_eq!(fills, 0, "no hover fill at rest");
        assert_eq!(ink, th.menu_item_fg().to_egui());
    }

    /// 한국어·일본어 라벨도 글자 경계에서 끝을 줄이는지 검사한다. 남은 글자는 원문의 앞부분과
    /// 글자 단위로 같고(멀티바이트 글자가 중간에서 갈라지지 않는다) 말줄임표 하나로 끝난다.
    /// 시험 Context 는 egui 기본 폰트라 CJK 글자는 대체 글리프로 측정되지만, 줄이는 단위는 글자다.
    #[test]
    fn a_cjk_label_past_the_cap_ends_in_an_ellipsis_at_a_char_boundary() {
        const CAP: f32 = 60.0;
        let th = theme();
        for label in ["연결 대기 중인 포트…", "クリップボードビューア"] {
            let (full, cut, cut_w) = with_ui(|ctx, ui| {
                let cut = menu_label_galley(ui, &th, label, egui::Color32::WHITE, CAP);
                (
                    label_width(ctx, &th, label),
                    cut.rows
                        .iter()
                        .flat_map(|r| r.glyphs.iter().map(|g| g.chr))
                        .collect::<String>(),
                    cut.rect.width(),
                )
            });
            assert!(full > CAP, "{label}: {full}");
            assert!(cut_w <= CAP, "{label}: {cut_w}");
            let kept = cut.strip_suffix('…').unwrap_or_else(|| panic!("{cut}"));
            assert!(!kept.is_empty(), "{label}: {cut}");
            assert!(label.starts_with(kept), "{label} -> {cut}");
        }
    }

    #[test]
    fn a_label_past_the_cap_ends_in_an_ellipsis() {
        let th = theme();
        let (full, cut, cut_w, fit) = with_ui(|ctx, ui| {
            let cut = menu_label_galley(ui, &th, LONG, egui::Color32::WHITE, 120.0);
            let fit = menu_label_galley(ui, &th, "Git", egui::Color32::WHITE, 120.0);
            (
                label_width(ctx, &th, LONG),
                cut.rows
                    .iter()
                    .flat_map(|r| r.glyphs.iter().map(|g| g.chr))
                    .collect::<String>(),
                cut.rect.width(),
                fit.rows
                    .iter()
                    .flat_map(|r| r.glyphs.iter().map(|g| g.chr))
                    .collect::<String>(),
            )
        });
        assert!(full > 120.0);
        assert!(cut_w <= 120.0, "{cut_w}");
        assert!(cut.ends_with('…'), "{cut}");
        assert_eq!(fit, "Git");
    }
}
