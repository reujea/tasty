//! 범위에 들어가지 않는 팝업을 접어 "팝업 보기" 버튼만 남기는 정적 예제.
//! 정본은 tasty-page 갤러리 Overlays › Dialogs 의
//! `collapse-a-surface-popup-that-does-not-fit-leaves-a-show-button` Spec 이다.
//! 접힘 판정·버튼 모양·자리는 [`place`] 하나로 계산하고 프레임마다 다른 surface 폭만 준다.
//! 본체는 아직 접지 않는다. 입력 차단·Esc·펼침 전이는 이 예제에서 검증하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant};

use super::scrim_scope::{SURFACE_RADIUS, paint_card, paint_shell, paint_surface, shell_size};
use crate::catalog::icons::FIT;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::{t, t_lang};

const LABEL_KEY: &str = "popup.collapsed_button.label";
const TOOLTIP_KEY: &str = "popup.collapsed_button.tooltip";
const TITLE_KEY: &str = "convert_popup.title";

/// 넓은 프레임에서 팝업 소속 surface 가 차지하는 body 비율. 필요 크기가 들어간다.
const WIDE_FRACTION: f32 = 0.6;
/// 좁은 프레임의 비율. 필요 크기는 들어가지 않고 라벨 프레임은 들어간다.
const NARROW_FRACTION: f32 = 0.45;
/// 정본 `leftWidth: 64` — 라벨 프레임은 안 들어가고 아이콘 프레임은 들어가는 surface 폭(무대 치수).
const ICON_ONLY_OWNER_WIDTH: LogicalPx = LogicalPx(64.0);
/// 정본 `leftWidth: 24` — 아이콘 프레임도 안 들어가 surface 사각형에서 잘리는 폭(무대 치수).
const CLIPPED_OWNER_WIDTH: LogicalPx = LogicalPx(24.0);

/// 예제 팝업의 필요 크기. 넓은 프레임의 bounds 에는 들어가고 좁은 프레임에는 들어가지 않는다.
fn needed_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.spacing_xl.value() * 7.0,
        theme.spacing_xl.value() * 2.0,
    )
}

/// 창이 팝업보다 작은 프레임의 창 크기. 폭은 필요 폭보다 작고 높이는 크롬 셋뿐이다.
fn small_window_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.spacing_xl.value() * 5.0,
        theme.titlebar_height.value()
            + theme.tab_bar_height.value()
            + theme.status_bar_height.value(),
    )
}

/// 접힌 팝업의 표시 형태.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// 필요 크기가 bounds 에 들어가 팝업이 범위 안에 펼쳐진다.
    Expanded,
    /// 라벨 버튼 프레임.
    Label,
    /// 아이콘 버튼 프레임. 축마다 들어가지 않으면 bounds 시작에 붙고 surface 사각형에서 잘린다.
    Icon,
}

/// 범위 안 배치 결과.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placed {
    pub form: Form,
    pub rect: egui::Rect,
}

/// `scope_bounds` 의 inset: `max(0, min(cap, 폭/2, 높이/2))`, 네 변 같다.
pub(crate) fn scope_inset(scope: egui::Rect, cap: f32) -> f32 {
    cap.min(scope.width() / 2.0)
        .min(scope.height() / 2.0)
        .max(0.0)
}

/// 정본 규칙으로 팝업의 표시 형태와 사각형을 정한다. 앵커는 bounds 가운데(host 팝업)다.
/// - 필요 크기가 두 축 모두 bounds 에 들어가면 펼친다. 줄이는 구간은 없다.
/// - 아니면 라벨 프레임이 두 축 모두 들어갈 때 라벨, 아니면 아이콘 프레임.
/// - 아이콘 프레임이 들어가지 않는 축은 bounds 시작에 붙인다. 끝은 호출자가 surface 로 자른다.
pub(crate) fn place(
    scope: egui::Rect,
    inset_cap: f32,
    needed: egui::Vec2,
    label_frame: egui::Vec2,
    icon_frame: egui::Vec2,
) -> Placed {
    let bounds = scope.shrink(scope_inset(scope, inset_cap));
    let fits = |size: egui::Vec2| size.x <= bounds.width() && size.y <= bounds.height();
    let centered = |size: egui::Vec2| egui::Rect::from_center_size(bounds.center(), size);
    if fits(needed) {
        return Placed {
            form: Form::Expanded,
            rect: centered(needed),
        };
    }
    if fits(label_frame) {
        return Placed {
            form: Form::Label,
            rect: centered(label_frame),
        };
    }
    let axis = |start: f32, len: f32, center: f32, size: f32| {
        if size <= len {
            center - size / 2.0
        } else {
            start
        }
    };
    let min = egui::pos2(
        axis(
            bounds.left(),
            bounds.width(),
            bounds.center().x,
            icon_frame.x,
        ),
        axis(
            bounds.top(),
            bounds.height(),
            bounds.center().y,
            icon_frame.y,
        ),
    );
    Placed {
        form: Form::Icon,
        rect: egui::Rect::from_min_size(min, icon_frame),
    }
}

/// 버튼 둘레의 프레임 두께(테두리 + content margin) 한 변.
fn frame_pad(theme: &Theme) -> f32 {
    theme.border_width.value() + theme.popup_content_margin().value()
}

fn label_button<'a>(label: &'a str, theme: &Theme, icon: &'a FitIcon) -> Button<'a> {
    Button::new(label)
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Md)
        .leading_icon(icon)
        .leading_icon_size(theme.icon_glyph_size_sm)
}

type FitIcon = dyn Fn(&mut egui::Ui, egui::Rect, egui::Color32);

/// 정본은 `fit` 글리프를 14(icon-size-sm)로 그린다. 위젯이 준 칸 가운데에 그 크기로 둔다.
fn fit_icon(theme: &Theme) -> impl Fn(&mut egui::Ui, egui::Rect, egui::Color32) + use<> {
    let side = theme.icon_glyph_size_sm.value();
    move |ui, rect, c| {
        let glyph = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(side));
        FIT.image(side, c).paint_at(ui, glyph);
    }
}

/// 라벨 버튼 프레임과 아이콘 버튼 프레임의 바깥 크기.
fn frame_sizes(ui: &egui::Ui, theme: &Theme, label: &str) -> (egui::Vec2, egui::Vec2) {
    let icon = fit_icon(theme);
    let pad = frame_pad(theme) * 2.0;
    let height = ControlSize::Md.height(theme);
    let label_w = label_button(label, theme, &icon)
        .desired_width(ui, theme)
        .value();
    (
        egui::vec2(label_w + pad, height + pad),
        egui::Vec2::splat(height + pad),
    )
}

/// 접힌 팝업 자체 프레임과 버튼. `clip` 은 surface 사각형이다.
fn paint_collapsed(
    ui: &mut egui::Ui,
    theme: &Theme,
    placed: Placed,
    clip: egui::Rect,
    label: &str,
    on_scrim: bool,
    ring: bool,
) {
    let painter = ui.painter_at(clip);
    let radius = theme.corner_radius.value();
    let shadow = if on_scrim {
        theme.shadow_modal()
    } else {
        theme.shadow_popover()
    };
    painter.add(shadow.to_egui().as_shape(placed.rect, radius));
    painter.rect(
        placed.rect,
        radius,
        theme.bg_panel().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
        egui::StrokeKind::Inside,
    );

    let inner = placed.rect.shrink(frame_pad(theme));
    let icon = fit_icon(theme);
    let resp = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.set_clip_rect(clip.intersect(ui.clip_rect()));
            match placed.form {
                Form::Label => label_button(label, theme, &icon).show(ui, theme),
                _ => IconButton::new()
                    .variant(IconButtonVariant::Solid)
                    .size(ControlSize::Md)
                    .show(ui, theme, &icon),
            }
        })
        .inner;
    if ring {
        // `:focus-visible` 짝: 버튼 바깥 border_width 만큼 띄운 focus_ring_width 링.
        let offset = theme.border_width.value();
        painter.rect_stroke(
            resp.rect.expand(offset),
            theme.corner_radius.value() + offset,
            egui::Stroke::new(theme.focus_ring_width.value(), theme.border_focus()),
            egui::StrokeKind::Outside,
        );
    }
    resp.on_hover_text(t(TOOLTIP_KEY));
}

/// 소속 surface 폭을 정하는 방법.
#[derive(Clone, Copy)]
enum OwnerWidth {
    Fraction(f32),
    Fixed(LogicalPx),
}

/// 한 프레임의 조건.
#[derive(Clone, Copy)]
enum Frame {
    /// surface 범위 팝업. `ring` 은 사용자가 열어 키 포커스가 있는지(passive 면 거짓).
    Surface { owner: OwnerWidth, ring: bool },
    /// 버튼을 눌러 창 범위로 펼친 팝업. `size` 가 있으면 그 창 크기(팝업보다 작다).
    Pressed { size: Option<egui::Vec2> },
}

fn split(body: egui::Rect, owner: OwnerWidth, divider: f32) -> (egui::Rect, egui::Rect) {
    let width = match owner {
        OwnerWidth::Fraction(f) => (body.width() - divider) * f,
        OwnerWidth::Fixed(w) => w.value(),
    };
    let cut = body.left() + width;
    (
        egui::Rect::from_min_max(body.min, egui::pos2(cut, body.bottom())),
        egui::Rect::from_min_max(egui::pos2(cut + divider, body.top()), body.max),
    )
}

fn shell_frame(ui: &mut egui::Ui, theme: &Theme, frame: Frame) {
    let size = match frame {
        Frame::Pressed { size: Some(size) } => size,
        _ => shell_size(theme),
    };
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    let body = paint_shell(&p, theme, rect);
    let divider = theme.border_width.value();
    let owner_width = match frame {
        Frame::Surface { owner, .. } => owner,
        Frame::Pressed { .. } => OwnerWidth::Fraction(NARROW_FRACTION),
    };
    let (owner, adjacent) = split(body, owner_width, divider);
    paint_surface(&p, theme, owner);
    paint_surface(&p, theme, adjacent);

    let label = t(LABEL_KEY);
    match frame {
        Frame::Surface { ring, .. } => {
            // 접힌 상태도 그 팝업의 scrim 명부를 그대로 따른다. 예제는 범위를 덮는 팝업이다.
            p.rect_filled(owner, SURFACE_RADIUS.value(), theme.scrim().to_egui());
            let (label_frame, icon_frame) = frame_sizes(ui, theme, label);
            let placed = place(
                owner,
                theme.spacing_sm.value(),
                needed_size(theme),
                label_frame,
                icon_frame,
            );
            if placed.form == Form::Expanded {
                paint_card(
                    &ui.painter_at(owner),
                    theme,
                    placed.rect,
                    theme.bg_panel().to_egui(),
                    t(TITLE_KEY),
                );
            } else {
                paint_collapsed(ui, theme, placed, owner, label, true, ring);
            }
        }
        Frame::Pressed { size } => {
            p.rect_filled(rect, theme.corner_radius.value(), theme.scrim().to_egui());
            let needed = needed_size(theme);
            // 창이 팝업보다 작으면 크기를 줄이지 않고 왼쪽·위를 창 안쪽 가장자리에 둔다.
            let card = if size.is_some() {
                egui::Rect::from_min_size(rect.shrink(theme.border_width.value()).min, needed)
            } else {
                egui::Rect::from_center_size(rect.center(), needed)
            };
            paint_card(&p, theme, card, theme.bg_panel().to_egui(), t(TITLE_KEY));
        }
    }
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let frames: [(&str, Frame); 7] = [
        (
            "wide surface — popup expanded in its surface",
            Frame::Surface {
                owner: OwnerWidth::Fraction(WIDE_FRACTION),
                ring: true,
            },
        ),
        (
            "narrow surface — collapsed, opened by the user (key focus)",
            Frame::Surface {
                owner: OwnerWidth::Fraction(NARROW_FRACTION),
                ring: true,
            },
        ),
        (
            "narrow surface — collapsed, opened by an agent (no focus)",
            Frame::Surface {
                owner: OwnerWidth::Fraction(NARROW_FRACTION),
                ring: false,
            },
        ),
        (
            "very narrow — label frame does not fit, icon only",
            Frame::Surface {
                owner: OwnerWidth::Fixed(ICON_ONLY_OWNER_WIDTH),
                ring: true,
            },
        ),
        (
            "narrower than the icon frame — cut at the surface rect",
            Frame::Surface {
                owner: OwnerWidth::Fixed(CLIPPED_OWNER_WIDTH),
                ring: true,
            },
        ),
        (
            "pressed — the popup over the window",
            Frame::Pressed { size: None },
        ),
        (
            "pressed, window smaller than the popup — right and bottom cut",
            Frame::Pressed {
                size: Some(small_window_size(theme)),
            },
        ),
    ];
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (i, (label, frame)) in frames.into_iter().enumerate() {
            ui.push_id(("collapsed-frame", i), |ui| {
                spec::cluster(ui, theme, label, |ui| shell_frame(ui, theme, frame));
            });
        }
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (i, code) in std::iter::once("en")
            .chain(crate::i18n::translated_languages())
            .enumerate()
        {
            let label = t_lang(code, LABEL_KEY);
            ui.push_id(("collapsed-label", i), |ui| {
                spec::cluster(ui, theme, code, |ui| {
                    let (label_frame, icon_frame) = frame_sizes(ui, theme, label);
                    for (form, size) in [(Form::Label, label_frame), (Form::Icon, icon_frame)] {
                        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                        paint_collapsed(
                            ui,
                            theme,
                            Placed { form, rect },
                            rect,
                            label,
                            false,
                            false,
                        );
                    }
                });
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "collapses when",
                "the popup's needed size is larger than the surface bounds (scope_bounds: the surface rect less the inset) on either axis · expanded only when both fit · no shrink range · judged every frame",
            ),
            (
                "needed size",
                "host popup: that frame's sizer result (the user's size if resized) · no sizer: default_size × ui_zoom · plugin popup: its size_hint, 360 × 200 without one",
            ),
            (
                "inset",
                "max(0, min(spacing_sm, width / 2, height / 2)) of the surface rect, the same on all four sides",
            ),
            (
                "frame",
                "the popup's own frame: bg-panel · 1px border-frame · 4 radius · padding popup-content-margin 4 · the button only",
            ),
            (
                "shadow",
                "shadow-modal on a scrim · shadow-popover when the popup has no scrim",
            ),
            (
                "button",
                "Button.secondary md 28 · leading glyph fit 14 · label popup.collapsed_button.label",
            ),
            (
                "too narrow for the label",
                "label frame wider or taller than the bounds → icon frame: IconButton solid 28, same glyph, tooltip unchanged",
            ),
            (
                "too small for the icon",
                "on each axis the icon frame does not fit, its start edge sits on the bounds' start edge and the surface rect cuts its end · never omitted",
            ),
            (
                "title",
                "not shown — the tooltip popup.collapsed_button.tooltip explains the button",
            ),
            (
                "scrim",
                "follows the popup's own scrim entry, the same in both forms",
            ),
            (
                "focus",
                "border-focus · focus-ring-width 2 · 1px offset on the button · passive (agent-opened): no ring",
            ),
            (
                "pressed",
                "window scope: centred, full-window scrim · window too small: size kept, left and top on the window's inner edge, right and bottom cut",
            ),
            (
                "search bar",
                "not collapsed — it shrinks to its surface as before",
            ),
        ],
        &[
            TokenChip::new("bg-panel", "collapsed frame", theme.bg_panel().into()),
            TokenChip::new("border-frame", "frame edge", theme.border_frame().into()),
            TokenChip::without_color("popup-content-margin", "4 around the button"),
            TokenChip::new(
                "border-focus",
                "button key focus",
                theme.border_focus().into(),
            ),
            TokenChip::without_color("focus-ring-width", "2"),
            TokenChip::without_color("shadow-modal", "on a scrim"),
            TokenChip::without_color("shadow-popover", "without a scrim"),
            TokenChip::new("scrim-bg", "unchanged by collapsing", theme.scrim().into()),
        ],
    );

    spec::note(
        ui,
        theme,
        "접힌 형태는 그 팝업 자체를 작게 그린 것이다. 범위·scrim·바깥 클릭과 Esc 규칙·키 포커스를 그대로 가진다. 버튼을 누르거나 Enter·Space를 누르면 팝업이 창 가운데에 창 scrim과 함께 펼쳐진다. 접힌 형태에는 닫기 표시를 두지 않는다.",
    );
    spec::dont(
        ui,
        theme,
        "팝업을 줄여 맞추거나, 프레임 없이 버튼만 scrim 위에 두거나, 아이콘 프레임도 안 들어간다고 버튼을 숨기지 마라. 열린 팝업은 늘 자리를 보인다.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames() -> (egui::Vec2, egui::Vec2) {
        // 라벨 프레임 en 134 × 38, 아이콘 프레임 38 × 38(정본 실측값).
        (egui::vec2(134.0, 38.0), egui::vec2(38.0, 38.0))
    }

    fn surface(w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(w, h))
    }

    /// Given 필요 크기가 inset 뺀 bounds 에 두 축 모두 들어갈 때, Then 펼친다.
    #[test]
    fn a_popup_that_fits_the_bounds_stays_expanded() {
        let (label, icon) = frames();
        let placed = place(
            surface(300.0, 200.0),
            8.0,
            egui::vec2(284.0, 184.0),
            label,
            icon,
        );
        assert_eq!(placed.form, Form::Expanded);
        assert_eq!(placed.rect.size(), egui::vec2(284.0, 184.0));
    }

    /// Given 높이만 모자랄 때, Then 접힌다(어느 한 축이면 충분하다).
    #[test]
    fn height_alone_is_enough_to_collapse() {
        let (label, icon) = frames();
        let placed = place(
            surface(400.0, 100.0),
            8.0,
            egui::vec2(200.0, 90.0),
            label,
            icon,
        );
        assert_eq!(placed.form, Form::Label);
    }

    /// Given 라벨 프레임이 bounds 폭보다 넓을 때, Then 아이콘 프레임으로 바뀌고 가운데에 둔다.
    #[test]
    fn a_label_frame_wider_than_the_bounds_becomes_the_icon_frame() {
        let (label, icon) = frames();
        let scope = surface(64.0, 200.0);
        let placed = place(scope, 8.0, egui::vec2(300.0, 100.0), label, icon);
        assert_eq!(placed.form, Form::Icon);
        assert_eq!(placed.rect.center(), scope.center());
    }

    /// Given 아이콘 프레임도 폭이 들어가지 않을 때, Then 그 축은 bounds 시작에 붙고 세로는 가운데다.
    #[test]
    fn an_icon_frame_too_wide_sits_on_the_bounds_start_on_that_axis() {
        let (label, icon) = frames();
        let scope = surface(24.0, 200.0);
        let placed = place(scope, 8.0, egui::vec2(300.0, 100.0), label, icon);
        assert_eq!(placed.form, Form::Icon);
        assert_eq!(placed.rect.left(), scope.left() + 8.0);
        assert_eq!(placed.rect.center().y, scope.center().y);
    }

    /// Given 작은 surface, Then inset 은 폭·높이의 절반까지 줄어든다.
    #[test]
    fn the_inset_shrinks_to_half_a_small_surface() {
        assert_eq!(scope_inset(surface(10.0, 200.0), 8.0), 5.0);
        assert_eq!(scope_inset(surface(200.0, 6.0), 8.0), 3.0);
        assert_eq!(scope_inset(surface(200.0, 200.0), 8.0), 8.0);
    }

    /// Given 갤러리 셸의 실제 치수와 테마, Then 다섯 surface 프레임이 정본의 형태대로 나온다.
    #[test]
    fn the_stage_frames_show_the_canonical_forms() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut forms = Vec::new();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (label, icon) = frame_sizes(ui, &theme, t(LABEL_KEY));
                let shell = egui::Rect::from_min_size(egui::Pos2::ZERO, shell_size(&theme));
                let body = paint_shell(&ui.painter_at(shell), &theme, shell);
                for owner in [
                    OwnerWidth::Fraction(WIDE_FRACTION),
                    OwnerWidth::Fraction(NARROW_FRACTION),
                    OwnerWidth::Fixed(ICON_ONLY_OWNER_WIDTH),
                    OwnerWidth::Fixed(CLIPPED_OWNER_WIDTH),
                ] {
                    let (scope, _) = split(body, owner, theme.border_width.value());
                    let placed = place(
                        scope,
                        theme.spacing_sm.value(),
                        needed_size(&theme),
                        label,
                        icon,
                    );
                    forms.push((placed.form, scope.contains_rect(placed.rect)));
                }
            });
        });
        assert_eq!(
            forms,
            vec![
                (Form::Expanded, true),
                (Form::Label, true),
                (Form::Icon, true),
                (Form::Icon, false),
            ]
        );
    }
}
