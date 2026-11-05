//! 튜토리얼 안내 말풍선. 화면 공간에 맞춰 방향을 고르고 넘치면 위치를 조정한다.
//! 배치는 place_callout에서 계산하며 표시 값은 Theme와 공용 버튼을 사용한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;

/// 기준폭. 좁은 화면에서는 줄이고 긴 본문은 스크롤한다.
pub const CALLOUT_W: LogicalPx = LogicalPx(244.0);
/// tail 삼각 크기(12px diamond 전사).
const TAIL: LogicalPx = LogicalPx(12.0);
/// up/down tail 의 좌측 기준 앵커 offset(디자인 left:28).
const TAIL_OFF_H: LogicalPx = LogicalPx(28.0);
/// left/right tail 의 상단 기준 앵커 offset(디자인 top:24).
const TAIL_OFF_V: LogicalPx = LogicalPx(24.0);

/// 단계 표시 점의 지름. 상태 점과 역할이 달라 같은 숫자의 다른 토큰으로 대체하지 않는다.
const STEP_RAIL_DOT_SIZE: LogicalPx = LogicalPx(5.0);

/// 마커가 말풍선의 어느 쪽에 있는지 = tail 방향.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tail {
    Up,
    Down,
    Left,
    Right,
}

/// 배치 결과 — 말풍선 좌상단 pos + tail 방향 + tail 앵커 offset(clamp 반영).
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub pos: egui::Pos2,
    pub tail: Tail,
    /// up/down 이면 좌측에서의 x offset, left/right 이면 상단에서의 y offset.
    pub tail_offset: LogicalPx,
}

fn fits(rect: egui::Rect, safe: egui::Rect) -> bool {
    rect.min.x >= safe.min.x
        && rect.min.y >= safe.min.y
        && rect.max.x <= safe.max.x
        && rect.max.y <= safe.max.y
}

/// edge-avoidance 배치 — 선호순서 below→above→right→left 중 안전영역에 들어가는
/// 첫 후보를 쓰고, 아무것도 안 맞으면 below 를 안전영역으로 clamp 한다. tail 은
/// clamp 후에도 마커 중심을 조준하도록 offset 을 재계산한다.
pub fn place_callout(
    marker: egui::Rect,
    size: egui::Vec2,
    screen: egui::Rect,
    gap: f32,
    safe: f32,
) -> Placement {
    let safe_rect = screen.shrink(safe);
    let below = (
        egui::pos2(
            marker.center().x - TAIL_OFF_H.value(),
            marker.bottom() + gap,
        ),
        Tail::Up,
    );
    let above = (
        egui::pos2(
            marker.center().x - TAIL_OFF_H.value(),
            marker.top() - gap - size.y,
        ),
        Tail::Down,
    );
    let right = (
        egui::pos2(marker.right() + gap, marker.center().y - TAIL_OFF_V.value()),
        Tail::Left,
    );
    let left = (
        egui::pos2(
            marker.left() - gap - size.x,
            marker.center().y - TAIL_OFF_V.value(),
        ),
        Tail::Right,
    );

    let (mut pos, tail) = [below, above, right, left]
        .into_iter()
        .find(|(pos, _)| fits(egui::Rect::from_min_size(*pos, size), safe_rect))
        .unwrap_or(below);

    pos.x = pos.x.clamp(
        safe_rect.min.x,
        (safe_rect.max.x - size.x).max(safe_rect.min.x),
    );
    pos.y = pos.y.clamp(
        safe_rect.min.y,
        (safe_rect.max.y - size.y).max(safe_rect.min.y),
    );

    let tail_offset = match tail {
        Tail::Up | Tail::Down => {
            LogicalPx(marker.center().x - pos.x).clamp(TAIL, LogicalPx(size.x) - TAIL)
        }
        Tail::Left | Tail::Right => {
            LogicalPx(marker.center().y - pos.y).clamp(TAIL, LogicalPx(size.y) - TAIL)
        }
    };

    Placement {
        pos,
        tail,
        tail_offset,
    }
}

/// Measure title and body with the same fonts and width as the rendered card.
pub fn measure_callout(
    ctx: &egui::Context,
    theme: &Theme,
    title: &str,
    body: &str,
    available: egui::Vec2,
) -> egui::Vec2 {
    let width = CALLOUT_W.value().min(
        (available.x - theme.spacing_lg.value() * 2.0)
            .max(theme.item_height_interactive.value() * 2.0),
    );
    let content_width = (width - theme.spacing_lg.value() * 2.0).max(theme.spacing_lg.value());
    let text_height = ctx.fonts(|f| {
        f.layout(
            title.to_owned(),
            egui::FontId::proportional(theme.font_size_body.value()),
            theme.text_primary().to_egui(),
            content_width,
        )
        .size()
        .y + f
            .layout(
                body.to_owned(),
                egui::FontId::proportional(theme.font_size_caption.value()),
                theme.text_secondary().to_egui(),
                content_width,
            )
            .size()
            .y
    });
    let height =
        text_height + theme.item_height_interactive.value() * 3.0 + theme.spacing_lg.value() * 3.0;
    egui::vec2(
        width,
        height.min(
            (available.y - theme.spacing_lg.value() * 2.0)
                .max(theme.item_height_interactive.value() * 3.0),
        ),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CalloutClick {
    None,
    Next,
    Back,
    Skip,
    Practice,
}

pub struct CalloutProps<'a> {
    pub step: usize,
    pub total: usize,
    pub title: &'a str,
    pub body: &'a str,
    pub first: bool,
    pub last: bool,
    pub ready: bool,
    pub prepare: bool,
    pub size: egui::Vec2,
    pub keyboard_focus: bool,
    pub anchored: bool,
}

pub struct CalloutResponse {
    pub action: CalloutClick,
    pub rect: egui::Rect,
    pub clicked: bool,
}

/// 말풍선 Area의 레이어. 스포트라이트 scrim과 마커도 이 레이어에 말풍선보다 먼저 그려 말풍선
/// 아래에 합성한다.
pub fn callout_layer() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tutorial_callout"))
}

pub fn draw_callout(
    ctx: &egui::Context,
    theme: &Theme,
    placement: Placement,
    props: CalloutProps<'_>,
) -> CalloutResponse {
    let mut action = CalloutClick::None;
    let layer = callout_layer();
    let area = egui::Area::new(layer.id)
        .order(layer.order)
        .fixed_pos(placement.pos)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme.surface_raised().to_egui())
                .stroke(egui::Stroke::new(
                    theme.border_width.value(),
                    theme.border_strong().to_egui(),
                ))
                .corner_radius(theme.corner_radius_lg.value())
                .shadow(theme.shadow_popover().to_egui())
                .inner_margin(tasty_ui_widgets::margin_sym(
                    theme.spacing_lg,
                    theme.spacing_md,
                ))
                .show(ui, |ui| {
                    ui.set_width(
                        (props.size.x - theme.spacing_lg.value() * 2.0)
                            .max(theme.spacing_lg.value()),
                    );
                    ui.spacing_mut().item_spacing =
                        egui::vec2(theme.spacing_sm.value(), theme.spacing_xs.value());
                    ui.label(
                        egui::RichText::new(format!("{} / {}", props.step, props.total))
                            .monospace()
                            .size(theme.font_size_micro.value())
                            .color(theme.accent_primary_raised().to_egui()),
                    );
                    let body_height = (props.size.y
                        - theme.item_height_interactive.value() * 3.0
                        - theme.spacing_lg.value() * 2.0)
                        .max(theme.font_size_body.value());
                    egui::ScrollArea::vertical()
                        .id_salt("tutorial_body")
                        .max_height(body_height)
                        .drag_to_scroll(false)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(props.title)
                                    .size(theme.font_size_body.value())
                                    .strong()
                                    .color(theme.text_primary().to_egui()),
                            );
                            ui.label(
                                egui::RichText::new(props.body)
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_secondary_raised().to_egui()),
                            );
                        });
                    ui.horizontal(|ui| {
                        for i in 0..props.total {
                            let (r, _) = ui.allocate_exact_size(
                                egui::vec2(STEP_RAIL_DOT_SIZE.value(), STEP_RAIL_DOT_SIZE.value()),
                                egui::Sense::hover(),
                            );
                            let color = if i + 1 == props.step {
                                theme.accent_primary()
                            } else {
                                theme.surface_active()
                            };
                            ui.painter().circle_filled(
                                r.center(),
                                STEP_RAIL_DOT_SIZE.value() * 0.5,
                                color.to_egui(),
                            );
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if !props.ready
                            && !props.prepare
                            && Button::new(t("tutorial.btn_try"))
                                .variant(ButtonVariant::Secondary)
                                .size(ControlSize::Sm)
                                .show(ui, theme)
                                .clicked()
                        {
                            ui.memory_mut(|m| {
                                if let Some(id) = m.focused() {
                                    m.surrender_focus(id);
                                }
                            });
                            action = CalloutClick::Practice;
                        }

                        if Button::new(t("tutorial.btn_skip"))
                            .variant(ButtonVariant::Ghost)
                            .size(ControlSize::Sm)
                            .show(ui, theme)
                            .clicked()
                        {
                            action = CalloutClick::Skip;
                        }
                        if !props.first
                            && Button::new(t("tutorial.btn_back"))
                                .variant(ButtonVariant::Secondary)
                                .size(ControlSize::Sm)
                                .show(ui, theme)
                                .clicked()
                        {
                            action = CalloutClick::Back;
                        }
                        let label = if props.prepare {
                            t("tutorial.btn_prepare")
                        } else if props.last {
                            t("tutorial.btn_done")
                        } else {
                            t("tutorial.btn_next")
                        };
                        let next = Button::new(label)
                            .variant(ButtonVariant::Primary)
                            .size(ControlSize::Sm)
                            .enabled(props.ready)
                            .show(ui, theme);
                        if props.keyboard_focus && ui.memory(|m| m.focused().is_none()) {
                            next.request_focus();
                        }
                        if next.clicked() {
                            action = CalloutClick::Next;
                        }
                    });
                })
                .response
                .rect
        });
    // Do not draw a misleading pointer on an unanchored summary/recovery card.
    if props.anchored {
        paint_tail(
            &ctx.layer_painter(area.response.layer_id),
            area.inner,
            theme,
            placement,
        );
    }
    let clicked = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| area.inner.contains(p))
    });
    CalloutResponse {
        action,
        rect: area.inner,
        clicked,
    }
}

/// tail 삼각형 — bubble 모서리에서 마커 방향으로 튀어나온다. 외곽 2변만 stroke.
fn paint_tail(p: &egui::Painter, bubble: egui::Rect, theme: &Theme, pl: Placement) {
    let fill = theme.surface_raised().to_egui();
    let stroke = egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui());
    let h = TAIL / 2.0;
    let off = pl.tail_offset;
    let (a, b, apex) = match pl.tail {
        Tail::Up => {
            let cx = LogicalPx(bubble.min.x) + off;
            (
                egui::pos2((cx - h).value(), bubble.min.y),
                egui::pos2((cx + h).value(), bubble.min.y),
                egui::pos2(cx.value(), bubble.min.y - h.value()),
            )
        }
        Tail::Down => {
            let cx = LogicalPx(bubble.min.x) + off;
            (
                egui::pos2((cx - h).value(), bubble.max.y),
                egui::pos2((cx + h).value(), bubble.max.y),
                egui::pos2(cx.value(), bubble.max.y + h.value()),
            )
        }
        Tail::Left => {
            let cy = LogicalPx(bubble.min.y) + off;
            (
                egui::pos2(bubble.min.x, (cy - h).value()),
                egui::pos2(bubble.min.x, (cy + h).value()),
                egui::pos2(bubble.min.x - h.value(), cy.value()),
            )
        }
        Tail::Right => {
            let cy = LogicalPx(bubble.min.y) + off;
            (
                egui::pos2(bubble.max.x, (cy - h).value()),
                egui::pos2(bubble.max.x, (cy + h).value()),
                egui::pos2(bubble.max.x + h.value(), cy.value()),
            )
        }
    };
    p.add(egui::Shape::convex_polygon(
        vec![a, apex, b],
        fill,
        egui::Stroke::NONE,
    ));
    p.line_segment([a, apex], stroke);
    p.line_segment([apex, b], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 800.0))
    }
    const SIZE: egui::Vec2 = egui::Vec2 { x: 244.0, y: 150.0 };

    #[test]
    fn prefers_below_when_room() {
        let marker = egui::Rect::from_min_size(egui::pos2(400.0, 40.0), egui::vec2(120.0, 60.0));
        let p = place_callout(marker, SIZE, screen(), 12.0, 8.0);
        assert_eq!(p.tail, Tail::Up);
        assert!(p.pos.y > marker.bottom(), "callout below marker");
    }

    #[test]
    fn flips_above_when_no_room_below() {
        let marker = egui::Rect::from_min_size(egui::pos2(400.0, 720.0), egui::vec2(120.0, 60.0));
        let p = place_callout(marker, SIZE, screen(), 12.0, 8.0);
        assert_eq!(p.tail, Tail::Down);
        assert!(p.pos.y + SIZE.y <= marker.top(), "callout above marker");
    }

    #[test]
    fn clamps_into_safe_area() {
        let marker = egui::Rect::from_min_size(egui::pos2(0.0, 40.0), egui::vec2(40.0, 40.0));
        let p = place_callout(marker, SIZE, screen(), 12.0, 8.0);
        assert!(p.pos.x >= 8.0, "clamped to left safe margin: {}", p.pos.x);
    }

    #[test]
    fn tail_keeps_aiming_after_clamp() {
        let marker = egui::Rect::from_min_size(egui::pos2(0.0, 40.0), egui::vec2(40.0, 40.0));
        let p = place_callout(marker, SIZE, screen(), 12.0, 8.0);
        assert!(p.tail_offset >= TAIL && p.tail_offset <= LogicalPx(SIZE.x) - TAIL);
        let aim_x = LogicalPx(p.pos.x) + p.tail_offset;
        assert!(
            (aim_x - LogicalPx(marker.center().x)).abs() <= LogicalPx(SIZE.x),
            "tail aims near marker center"
        );
    }
    #[test]
    fn long_translations_keep_actions_inside_a_small_viewport() {
        // 다른 시험의 전역 번역 초기화와 경쟁하지 않도록 크기를 재기 전에 초기화한다.
        crate::i18n::init("en");
        let theme = crate::theme::theme();
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 300.0));
        for title in [
            "A long title that wraps across several lines",
            "워크스페이스와 페인 안에서 서피스를 분할하는 방법",
            "ワークスペースのサーフェスを分割する方法",
        ] {
            for _ in 0..3 {
                let mut rect = egui::Rect::NOTHING;
                let _output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| {
                        let body = "A long explanation. ".repeat(40);
                        let size = measure_callout(ctx, &theme, title, &body, screen.size());
                        let placement = place_callout(
                            egui::Rect::from_center_size(screen.center(), egui::Vec2::ZERO),
                            size,
                            screen,
                            theme.spacing_md.value(),
                            theme.spacing_sm.value(),
                        );
                        rect = draw_callout(
                            ctx,
                            &theme,
                            placement,
                            CalloutProps {
                                step: 2,
                                total: 6,
                                title,
                                body: &body,
                                first: false,
                                last: false,
                                ready: false,
                                prepare: false,
                                size,
                                keyboard_focus: false,
                                anchored: false,
                            },
                        )
                        .rect;
                    },
                );
                assert!(screen.contains_rect(rect), "{title}: {rect:?}");
            }
        }
    }
}
