//! 범위에 들어가지 않는 팝업을 접어 "팝업 보기" 버튼만 남기는 정적 예제.
//! 정본은 tasty-page 갤러리 Overlays › Dialogs 의
//! `collapse-a-surface-popup-that-does-not-fit-leaves-a-show-button` Spec 과 popup focus Spec 이다.
//! 접힘 판정·버튼 모양·자리·포커스 가장자리는 본체와 같은 `tasty_ui_widgets` 함수로 계산하고 그린다.
//! 프레임마다 다른 surface 폭만 준다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    CollapsedForm as Form, CollapsedLook, CollapsedPlacement as Placed, collapsed_frame_sizes,
    paint_collapsed_popup, paint_popup_focus_edge, place_collapsed, scope_inset,
};

use super::scrim_scope::{SURFACE_RADIUS, paint_card, paint_shell, paint_surface, shell_size};
use crate::catalog::popup_frame::{self, TITLE_BAR_HEIGHT, TitleButtons};
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

/// 범위 사각형에서 판정 경계를 만들어 형태와 자리를 정한다.
fn place(
    scope: egui::Rect,
    inset_cap: f32,
    needed: egui::Vec2,
    label_frame: egui::Vec2,
    icon_frame: egui::Vec2,
) -> Placed {
    let bounds = scope.shrink(scope_inset(scope, inset_cap));
    place_collapsed(bounds, needed, label_frame, icon_frame)
}

fn paint_collapsed(
    ui: &mut egui::Ui,
    theme: &Theme,
    placed: Placed,
    clip: egui::Rect,
    label: &str,
    on_scrim: bool,
    focus: Focus,
) {
    paint_collapsed_popup(
        ui,
        theme,
        placed,
        clip,
        CollapsedLook {
            label,
            tooltip: t(TOOLTIP_KEY),
            on_scrim,
            ring: focus == Focus::Held,
            edge: focus == Focus::Held,
        },
    );
}

/// 창 위로 펼친 팝업의 프레임: 제목줄(접기·닫기 버튼) + 콘텐츠. 창이 작으면 프레임을 창에 맞춰 제목줄과 버튼은
/// 온전히 남기고, 내용은 필요 크기를 유지해 오른쪽·아래가 잘린다(`place_expanded`). `title` 이 없으면 headless 팝업에 붙는 host 제목줄(제목·닫기 없음)이다.
fn paint_expanded_card(
    ui: &egui::Ui,
    theme: &Theme,
    card: egui::Rect,
    clip: egui::Rect,
    title: Option<&str>,
) {
    let p = ui.painter_at(clip);
    let radius = theme.corner_radius.value();
    p.add(theme.shadow_modal().to_egui().as_shape(card, radius));
    p.rect_filled(card, radius, theme.bg_panel().to_egui());
    let bar =
        egui::Rect::from_min_size(card.min, egui::vec2(card.width(), TITLE_BAR_HEIGHT.value()));
    let r = radius as u8;
    p.rect_filled(
        bar,
        egui::CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        theme.bg_sidebar().to_egui(),
    );
    p.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
    );
    let buttons = TitleButtons {
        collapse: true,
        fullscreen: false,
        close: title.is_some(),
    };
    let left = popup_frame::draw_title_buttons(ui.ctx(), &p, theme, bar, buttons);
    if let Some(title) = title {
        popup_frame::draw_title_text(&p, theme, bar, left, title);
    }
    p.rect_stroke(
        card,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// 팝업의 키 포커스 상태.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    /// 사용자가 열어 키 포커스가 있다. 버튼 링과 포커스 가장자리를 그린다.
    Held,
    /// 열려 있지만 키는 다른 곳(옆 surface 입력, 또는 agent 가 연 passive)에 있다.
    Elsewhere,
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
    /// surface 범위 팝업과 그 키 포커스 상태.
    Surface { owner: OwnerWidth, focus: Focus },
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
        Frame::Surface { focus, .. } => {
            // 접힌 상태도 그 팝업의 scrim 명부를 그대로 따른다. 예제는 범위를 덮는 팝업이다.
            p.rect_filled(owner, SURFACE_RADIUS.value(), theme.scrim().to_egui());
            let (label_frame, icon_frame) = collapsed_frame_sizes(ui, theme, label);
            let placed = place(
                owner,
                theme.spacing_sm.value(),
                needed_size(theme),
                label_frame,
                icon_frame,
            );
            if placed.form == Form::Expanded {
                let painter = ui.painter_at(owner);
                paint_card(
                    &painter,
                    theme,
                    placed.rect,
                    theme.bg_panel().to_egui(),
                    t(TITLE_KEY),
                );
                if focus == Focus::Held {
                    paint_popup_focus_edge(&painter, theme, placed.rect);
                }
            } else {
                paint_collapsed(ui, theme, placed, owner, label, true, focus);
            }
        }
        Frame::Pressed { .. } => {
            p.rect_filled(rect, theme.corner_radius.value(), theme.scrim().to_egui());
            let needed = needed_size(theme);
            // 창 범위로 펼친 팝업은 포커스 가장자리를 그리지 않는다. 프레임은 본체와 같은 공용 배치로 창에
            // 맞추고(제목줄과 버튼은 온전히) 넘친 내용의 오른쪽·아래만 잘린다.
            let inner = rect.shrink(theme.border_width.value());
            let centered = egui::Rect::from_center_size(inner.center(), needed).min;
            let card = tasty_ui_widgets::place_expanded(inner, centered, needed).frame;
            paint_expanded_card(ui, theme, card, inner, Some(t(TITLE_KEY)));
        }
    }
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let surface = |owner, focus| Frame::Surface { owner, focus };
    let frames: [(&str, Frame); 8] = [
        (
            "wide surface — popup expanded in its surface",
            surface(OwnerWidth::Fraction(WIDE_FRACTION), Focus::Held),
        ),
        (
            "narrow surface — collapsed, opened by the user (key focus)",
            surface(OwnerWidth::Fraction(NARROW_FRACTION), Focus::Held),
        ),
        (
            "narrow surface — collapsed, the user typing in the other surface (no ring, no edge)",
            surface(OwnerWidth::Fraction(NARROW_FRACTION), Focus::Elsewhere),
        ),
        (
            "narrow surface — collapsed, opened by an agent (no focus)",
            surface(OwnerWidth::Fraction(NARROW_FRACTION), Focus::Elsewhere),
        ),
        (
            "very narrow — label frame does not fit, icon only",
            surface(OwnerWidth::Fixed(ICON_ONLY_OWNER_WIDTH), Focus::Held),
        ),
        (
            "narrower than the icon frame — cut at the surface rect",
            surface(OwnerWidth::Fixed(CLIPPED_OWNER_WIDTH), Focus::Held),
        ),
        (
            "pressed — the popup over the window, with the shrink button",
            Frame::Pressed { size: None },
        ),
        (
            "pressed, window smaller than the popup — fitted to the window, the bar and shrink button whole, the bottom cut",
            Frame::Pressed {
                size: Some(small_window_size(theme)),
            },
        ),
    ];
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (i, (label, frame)) in frames.into_iter().enumerate() {
            // 줄바꿈 줄은 cluster 묶음 단위로 자리를 넘긴다. id 구분은 묶음 안쪽에서 한다.
            spec::cluster(ui, theme, label, |ui| {
                ui.push_id(("collapsed-frame", i), |ui| shell_frame(ui, theme, frame));
            });
        }
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (i, code) in std::iter::once("en")
            .chain(crate::i18n::translated_languages())
            .enumerate()
        {
            let label = t_lang(code, LABEL_KEY);
            spec::cluster(ui, theme, code, |ui| {
                let (label_frame, icon_frame) = collapsed_frame_sizes(ui, theme, label);
                for (j, (form, size)) in [(Form::Label, label_frame), (Form::Icon, icon_frame)]
                    .into_iter()
                    .enumerate()
                {
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.push_id(("collapsed-label", i, j), |ui| {
                        paint_collapsed(
                            ui,
                            theme,
                            Placed { form, rect },
                            rect,
                            label,
                            false,
                            Focus::Elsewhere,
                        );
                    });
                }
            });
        }
    });

    // 펼친 형태의 제목줄: headless 팝업에 붙는 host 제목줄(제목·닫기 없음, 접기 버튼만)과 제목줄이 있는 팝업.
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        let width = theme.spacing_xl.value() * 9.0;
        let height = TITLE_BAR_HEIGHT.value() + theme.spacing_xl.value();
        for (i, (cap, title)) in [
            (
                "headless popup — host bar, no title, only the shrink button",
                None,
            ),
            (
                "popup with its own title bar — shrink first, then close",
                Some(t(TITLE_KEY)),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            spec::cluster(ui, theme, cap, |ui| {
                ui.push_id(("expanded-bar", i), |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                    paint_expanded_card(
                        ui,
                        theme,
                        rect,
                        rect.expand(theme.spacing_md.value()),
                        title,
                    );
                });
            });
        }
    });

    // 포커스 가장자리 확대: 키 포커스를 가진 프레임과 열려 있지만 포커스가 다른 곳인 프레임.
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        let label = t(LABEL_KEY);
        for (i, (cap, focus)) in [
            ("holds key focus", Focus::Held),
            ("open, focus elsewhere", Focus::Elsewhere),
        ]
        .into_iter()
        .enumerate()
        {
            spec::cluster(ui, theme, cap, |ui| {
                let (label_frame, _) = collapsed_frame_sizes(ui, theme, label);
                let (rect, _) = ui.allocate_exact_size(label_frame, egui::Sense::hover());
                ui.push_id(("focus-edge", i), |ui| {
                    paint_collapsed(
                        ui,
                        theme,
                        Placed {
                            form: Form::Label,
                            rect,
                        },
                        rect.expand(theme.spacing_sm.value()),
                        label,
                        false,
                        focus,
                    );
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
                "border-focus · focus-ring-width 2 · 1px offset on the button · passive (agent-opened): no ring · the frame lights the popup focus edge while it holds key focus",
            ),
            (
                "popup focus edge",
                "popup-focus-edge-focused · popup-focus-edge-width 2 across the top inside the 1px border, clipped by the radius · scopes narrower than the window only · no row reserved · unfocused: nothing else changes",
            ),
            (
                "pressed",
                "shown in window scope (logical scope unchanged): centred, full-window scrim, shadow-modal, no focus edge · window too small: left and top on the window's inner edge, frame fitted to the window, the title bar and its buttons whole, the content cut at the bottom",
            ),
            (
                "collapse back",
                "over the window only · the shrink button or Esc · IconButton sm 24, glyph shrink, first in the title bar's button cluster · accessible name popup.collapse_button.label, tooltip popup.collapse_button.tooltip · a headless popup gets the host title bar with no title and only this button · keeps the popup's state",
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
            TokenChip::new(
                "popup-focus-edge-focused",
                "the lit bar",
                theme.popup_focus_edge_focused().into(),
            ),
            TokenChip::without_color("popup-focus-edge-width", "2"),
            TokenChip::new(
                "bg-sidebar",
                "host title bar of a headless expanded popup",
                theme.bg_sidebar().into(),
            ),
            TokenChip::without_color("popup-title-btn-size", "24 — the shrink button"),
        ],
    );

    spec::note(
        ui,
        theme,
        "접힌 형태는 그 팝업 자체를 작게 그린 것이다. 범위·scrim·바깥 클릭과 Esc 규칙·키 포커스를 그대로 가진다. 버튼을 누르거나 Enter·Space를 누르면 팝업이 창 가운데에 창 scrim과 함께 펼쳐진다. 펼친 팝업은 제목줄의 접기 버튼이나 Esc로 다시 접히고 닫히지 않는다. 접힌 형태에는 닫기 표시를 두지 않는다.",
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

    /// Given 갤러리 셸의 실제 치수와 테마, Then 다섯 surface 프레임이 정본의 형태대로 나온다.
    #[test]
    fn the_stage_frames_show_the_canonical_forms() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut forms = Vec::new();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (label, icon) = collapsed_frame_sizes(ui, &theme, t(LABEL_KEY));
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
