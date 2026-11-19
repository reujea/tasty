//! 창 전체와 서피스 범위의 배경 어둡게 하기(scrim)를 비교하는 정적 예제.
//! 서피스 테두리는 포함하고 인접 서피스와 창 UI는 제외한다. 모서리는 범위의 형태를 따른다.
//! 부모와 자식 팝업이 범위를 공유하면 같은 알파로 한 번만 그린다. 팝업은 범위 안쪽에 배치한다.
//! 각 서피스에는 버튼이 하나 있다. 덮인 범위 안의 버튼은 hover·누름을 받지 않고, 범위 밖 버튼은 그대로 동작한다.
//! 닫기 순서·초안 수명은 이 예제에서 검증하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 셸 목업의 바깥 치수. 폭은 measure-md, 높이는 크롬 셋에 body 를 얹은 값이다.
pub(super) fn shell_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.measure_md.value(),
        theme.titlebar_height.value()
            + theme.tab_bar_height.value()
            + theme.status_bar_height.value()
            + theme.spacing_xl.value() * 8.0,
    )
}

/// 부모 popup 카드 크기 — surface 한 칸에 8px inset 까지 두고 들어가는 크기.
fn parent_card_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.spacing_xl.value() * 6.0,
        theme.spacing_xl.value() * 2.0,
    )
}

/// 자식 picker 카드 크기 — 부모보다 작아야 겹쳤을 때 z 순서가 눈에 남는다.
fn child_card_size(theme: &Theme) -> egui::Vec2 {
    egui::vec2(
        theme.spacing_xl.value() * 4.0,
        theme.spacing_lg.value() * 2.0,
    )
}

/// `clamp` 변형이 요구하는 폭 — 좁은 칸보다 커서 inset 까지 눌리는 값.
fn clamped_card_width(theme: &Theme) -> f32 {
    theme.spacing_xl.value() * 13.0
}

/// 오늘의 셸에서 surface 자신의 radius. scope rect 의 radius 가 이것을 따른다.
pub(super) const SURFACE_RADIUS: LogicalPx = LogicalPx(0.0);
/// `narrow` 일 때 소속 surface 가 차지하는 body 비율.
const NARROW_FRACTION: f32 = 0.32;
/// surface 안에 그리는 가짜 터미널 줄. dim 이 눈에 남게 하는 데만 쓴다.
const FAKE_LINES: [&str; 4] = ["$ cargo test", "  running 12 tests", "  ok", "$ _"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// 창 전체 — 커맨드 팔레트 · 설정 · target 바인딩 없는 호환 경로.
    Window,
    /// 소속 surface 만 — convert_surface 처럼 surface 에 묶인 popup.
    Surface,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Split {
    /// 좌우 분할 — surface 둘이 가로로 눕는다.
    Horizontal,
    /// 상하 분할 — surface 둘이 세로로 쌓인다.
    Vertical,
}

#[derive(Clone, Copy)]
struct ShellProps {
    scope: Scope,
    split: Split,
    child: bool,
    narrow: bool,
    clamp: bool,
}

impl ShellProps {
    fn new(scope: Scope) -> Self {
        Self {
            scope,
            split: Split::Horizontal,
            child: false,
            narrow: false,
            clamp: false,
        }
    }
    fn split(mut self, split: Split) -> Self {
        self.split = split;
        self
    }
    fn child(mut self) -> Self {
        self.child = true;
        self
    }
    fn narrow_clamp(mut self) -> Self {
        self.narrow = true;
        self.clamp = true;
        self
    }
}

/// body 를 두 surface rect 로 가른다. 첫째가 popup 의 소속 surface다.
/// 분할선은 `border_width` 한 줄 — 두 rect 는 그 줄을 나눠 갖지 않는다(인접 제외).
fn split_body(body: egui::Rect, props: ShellProps, divider: f32) -> (egui::Rect, egui::Rect) {
    let fraction = if props.narrow { NARROW_FRACTION } else { 0.5 };
    match props.split {
        Split::Horizontal => {
            let cut = body.left() + (body.width() - divider) * fraction;
            (
                egui::Rect::from_min_max(body.min, egui::pos2(cut, body.bottom())),
                egui::Rect::from_min_max(egui::pos2(cut + divider, body.top()), body.max),
            )
        }
        Split::Vertical => {
            let cut = body.top() + (body.height() - divider) * fraction;
            (
                egui::Rect::from_min_max(body.min, egui::pos2(body.right(), cut)),
                egui::Rect::from_min_max(egui::pos2(body.left(), cut + divider), body.max),
            )
        }
    }
}

/// 배경을 어둡게 한 차이를 볼 수 있도록 터미널 내용을 그린다.
/// 두 서피스는 같은 색으로 칠해 포커스 색상 차이가 비교에 섞이지 않게 한다.
pub(super) fn paint_surface(p: &egui::Painter, theme: &Theme, rect: egui::Rect) {
    let term = theme.surface("terminal");
    let (fill, fg) = (term.focused_bg, term.focused_fg);
    p.rect_filled(rect, SURFACE_RADIUS.value(), egui::Color32::from(fill));
    p.rect_stroke(
        rect,
        SURFACE_RADIUS.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );

    let pad = theme.spacing_sm.value();
    let line_h = theme.font_size_micro.value() + theme.spacing_xs.value();
    let font = egui::FontId::monospace(theme.font_size_micro.value());
    let inner = p.with_clip_rect(rect.shrink(theme.border_width.value()));
    for (i, line) in FAKE_LINES.iter().enumerate() {
        let y = rect.top() + pad + line_h * i as f32;
        if y + line_h > rect.bottom() {
            break;
        }
        inner.text(
            egui::pos2(rect.left() + pad, y),
            egui::Align2::LEFT_TOP,
            line,
            font.clone(),
            egui::Color32::from(fg),
        );
    }
}

/// 크롬 띠 한 줄 — 채움 + micro 라벨. 타이틀바 · 탭바 · 상태바가 공유한다.
fn paint_chrome(
    p: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    radius: f32,
    fill: egui::Color32,
    label: &str,
) {
    p.rect_filled(rect, radius, fill);
    p.text(
        egui::pos2(rect.left() + theme.spacing_sm.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(theme.font_size_micro.value()),
        theme.text_muted().to_egui(),
    );
}

/// 창 중앙 팝업 형태의 카드이므로 modal 그림자를 사용한다.
pub(super) fn paint_card(
    p: &egui::Painter,
    theme: &Theme,
    rect: egui::Rect,
    fill: egui::Color32,
    label: &str,
) {
    let radius = theme.corner_radius.value();
    p.add(theme.shadow_modal().to_egui().as_shape(rect, radius));
    p.rect_filled(rect, radius, fill);
    p.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
        egui::StrokeKind::Inside,
    );
    // 자식 카드가 겹쳐도 부모의 제목이 보이도록 왼쪽 위에 놓는다.
    p.text(
        rect.min + egui::vec2(theme.spacing_sm.value(), theme.spacing_sm.value()),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(theme.font_size_micro.value()),
        theme.text_primary().to_egui(),
    );
}

/// 셸 크롬 — 창 바탕·테두리·타이틀바·레일·탭바·상태바를 그리고 surface 가 놓일 body 를 돌려준다.
/// 접힌 팝업 예제(`collapsed_popup`)가 같은 셸을 그리려고 함께 쓴다.
pub(super) fn paint_shell(p: &egui::Painter, theme: &Theme, rect: egui::Rect) -> egui::Rect {
    let radius = theme.corner_radius.value();

    p.rect_filled(rect, radius, theme.bg_app().to_egui());
    p.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
        egui::StrokeKind::Inside,
    );

    let titlebar = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width(), theme.titlebar_height.value()),
    );
    paint_chrome(
        p,
        theme,
        titlebar,
        radius,
        theme.bg_panel().to_egui(),
        "tasty",
    );
    let status = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - theme.status_bar_height.value()),
        rect.max,
    );
    paint_chrome(
        p,
        theme,
        status,
        radius,
        theme.bg_panel().to_egui(),
        "status bar",
    );
    let rail = egui::Rect::from_min_max(
        egui::pos2(rect.left(), titlebar.bottom()),
        egui::pos2(
            rect.left() + theme.sidebar_collapsed_slot_width.value(),
            status.top(),
        ),
    );
    p.rect_filled(rail, 0.0, theme.bg_sidebar().to_egui());
    let tabs = egui::Rect::from_min_max(
        egui::pos2(rail.right(), titlebar.bottom()),
        egui::pos2(
            rect.right(),
            titlebar.bottom() + theme.tab_bar_height.value(),
        ),
    );
    paint_chrome(p, theme, tabs, 0.0, theme.bg_panel().to_egui(), "pane tabs");

    egui::Rect::from_min_max(
        egui::pos2(rail.right(), tabs.bottom()),
        egui::pos2(rect.right(), status.top().max(tabs.bottom())),
    )
}

/// 서피스 안의 버튼 하나. 덮인 범위 밖이면 hover·누름이 보인다.
fn surface_button(ui: &mut egui::Ui, theme: &Theme, surface: egui::Rect) -> egui::Response {
    let pad = theme.spacing_sm.value();
    let height = tasty_ui_widgets::ControlSize::Sm.height(theme);
    let slot = egui::Rect::from_min_max(
        egui::pos2(surface.left() + pad, surface.bottom() - pad - height),
        egui::pos2(surface.right() - pad, surface.bottom() - pad),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(slot), |ui| {
        ui.set_clip_rect(surface.intersect(ui.clip_rect()));
        tasty_ui_widgets::Button::new("Run")
            .variant(tasty_ui_widgets::ButtonVariant::Secondary)
            .size(tasty_ui_widgets::ControlSize::Sm)
            .show(ui, theme)
    })
    .inner
}

/// 카드에 그리는 셸 목업.
fn shell(ui: &mut egui::Ui, theme: &Theme, props: ShellProps) {
    shell_mock(ui, theme, props);
}

/// 셸 목업 — 크롬 · surface 둘 · scope scrim · popup. scrim 은 **한 번만** 깔린다.
/// 덮인 범위는 그 아래 위젯의 포인터 입력을 가져간다(본체 덮는 팝업과 같은 규칙).
/// 돌려주는 값은 두 서피스 버튼의 응답(소속 서피스, 옆 서피스)이다.
fn shell_mock(
    ui: &mut egui::Ui,
    theme: &Theme,
    props: ShellProps,
) -> (egui::Response, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(shell_size(theme), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let radius = theme.corner_radius.value();
    let divider = theme.border_width.value();
    let body = paint_shell(&p, theme, rect);
    let (owner, adjacent) = split_body(body, props, divider);
    paint_surface(&p, theme, owner);
    paint_surface(&p, theme, adjacent);
    let input = (
        surface_button(ui, theme, owner),
        surface_button(ui, theme, adjacent),
    );

    // 같은 범위의 부모·자식 팝업에는 scrim을 한 번만 그린다.
    let (scrim_rect, scrim_radius) = match props.scope {
        Scope::Window => (rect, radius),
        Scope::Surface => (owner, SURFACE_RADIUS.value()),
    };
    let p = ui.painter_at(rect);
    p.rect_filled(scrim_rect, scrim_radius, theme.scrim().to_egui());
    // 덮인 범위 위에 놓인 막이 아래 버튼의 hover·누름을 가져간다. 범위 밖 버튼은 막지 않는다.
    ui.interact(
        scrim_rect,
        ui.id()
            .with(("scrim_block", rect.min.x.to_bits(), rect.min.y.to_bits())),
        egui::Sense::click_and_drag(),
    );

    // popup 레이어는 scope rect 로 클립되고, 배치는 8px inset 안쪽에 눌린다.
    let clipped = ui.painter_at(scrim_rect);
    let bounds = scrim_rect.shrink(theme.spacing_sm.value());
    let want = parent_card_size(theme);
    let want_w = if props.clamp {
        clamped_card_width(theme)
    } else {
        want.x
    };
    let parent = egui::Rect::from_center_size(
        bounds.center(),
        egui::vec2(want_w.min(bounds.width()), want.y.min(bounds.height())),
    );
    paint_card(
        &clipped,
        theme,
        parent,
        theme.bg_panel().to_egui(),
        "Convert surface",
    );
    if props.child {
        let offset = theme.spacing_lg.value();
        let want = child_card_size(theme);
        let child = egui::Rect::from_center_size(
            bounds.center() + egui::vec2(offset, offset),
            egui::vec2(want.x.min(bounds.width()), want.y.min(bounds.height())),
        );
        paint_card(
            &clipped,
            theme,
            child,
            theme.surface_raised().to_egui(),
            "Choose file",
        );
    }
    input
}

pub fn draw_scope(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "window scope · full-window scrim", |ui| {
            shell(ui, theme, ShellProps::new(Scope::Window))
        });
        spec::cluster(ui, theme, "surface scope · owning surface only", |ui| {
            shell(ui, theme, ShellProps::new(Scope::Surface))
        });
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "surface scope · top/bottom split", |ui| {
            shell(
                ui,
                theme,
                ShellProps::new(Scope::Surface).split(Split::Vertical),
            )
        });
        spec::cluster(ui, theme, "narrow surface · clamped to 8px inset", |ui| {
            shell(ui, theme, ShellProps::new(Scope::Surface).narrow_clamp())
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "window scope",
                "scrim = window rect, radius = window radius",
            ),
            (
                "surface scope",
                "scrim = owning surface rect, radius = surface radius (0 today)",
            ),
            ("boundary", "border included · adjacent surface excluded"),
            (
                "excluded",
                "sidebar rail · pane tab bar · status bar · titlebar",
            ),
            ("inset", "spacing-sm (8px) between scope rect and popup"),
            ("clip", "popup layer is clipped to the scope rect"),
            ("alpha", "one scrim value for both scopes"),
            (
                "input",
                "covered scope takes hover and press · outside widgets work",
            ),
        ],
        &[
            TokenChip::new(
                "scrim-bg",
                "same value in both scopes",
                theme.scrim().into(),
            ),
            TokenChip::new(
                "bg-sidebar",
                "rail left lit under a surface scrim",
                theme.bg_sidebar().into(),
            ),
            TokenChip::new(
                "border-default",
                "surface edge — inside the scrim rect",
                theme.border_default().into(),
            ),
            TokenChip::new(
                "bg-panel",
                "chrome left lit under a surface scrim",
                theme.bg_panel().into(),
            ),
            TokenChip::without_color("shadow-modal", "the popup keeps its modal shadow"),
            TokenChip::new(
                "border-frame",
                "divider / surface edge, undimmed",
                theme.border_frame().into(),
            ),
        ],
    );

    spec::do_(
        ui,
        theme,
        "surface 에 묶인 popup 은 그 surface rect 만 어둡게 한다 — 옆 surface 에서 도는 \
         작업이 계속 읽힌다.",
    );
    spec::dont(
        ui,
        theme,
        "scope 가 둘이라고 알파를 두 벌로 나누지 마라. 값은 한 벌이고 덮는 rect 만 다르다.",
    );

    spec::note(
        ui,
        theme,
        "Window 예제는 창 전체를, Surface 예제는 해당 서피스와 테두리를 어둡게 한다. 인접 서피스와 사이드바·탭바·상태바는 Surface 범위에서 제외한다. 서피스 모서리는 직각이며 팝업은 spacing-sm만큼 안쪽에 배치하고 범위를 넘는 부분은 자른다. 각 서피스의 Run 버튼에 포인터를 올려 입력 차단을 확인한다. 덮인 범위 안의 버튼은 hover·누름을 받지 않고 범위 밖의 버튼은 그대로 동작한다. 닫기 순서는 별도 동작이다.",
    );
}

pub fn draw_child(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "parent only", |ui| {
            shell(ui, theme, ShellProps::new(Scope::Surface))
        });
        spec::cluster(ui, theme, "parent + child picker · one scrim", |ui| {
            shell(ui, theme, ShellProps::new(Scope::Surface).child())
        });
    });

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "window scope · parent + child", |ui| {
            shell(ui, theme, ShellProps::new(Scope::Window).child())
        });
        spec::cluster(ui, theme, "top/bottom split · parent + child", |ui| {
            shell(
                ui,
                theme,
                ShellProps::new(Scope::Surface)
                    .split(Split::Vertical)
                    .child(),
            )
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("scrim count", "one per scope, not one per popup"),
            ("z order", "scrim 0 · parent 1 · child 2"),
            ("child scope", "inherited from the parent popup"),
            ("child scrim", "none — the parent's scrim already covers it"),
            (
                "draw paths",
                "host popups and plugin popups share the scope's single scrim",
            ),
            ("parent under child", "full ink, not dimmed further"),
            (
                "clamp",
                "popup shrinks to the surface rect, min 320 → surface width wins",
            ),
            (
                "surface hidden",
                "popup shell AND scrim hide with it; both return on re-show",
            ),
            ("host popups", "same rule when they declare a surface scope"),
            (
                "input",
                "one block for the scope · widgets below get no hover or press",
            ),
        ],
        &[
            TokenChip::new(
                "scrim-bg",
                "painted once — a second pass would darken the background",
                theme.scrim().into(),
            ),
            TokenChip::new("bg-panel", "parent card fill", theme.bg_panel().into()),
            TokenChip::new(
                "surface-raised",
                "child picker fill",
                theme.surface_raised().into(),
            ),
            TokenChip::without_color("fp-popup-min-width", "picker floor, surface overrides it"),
            TokenChip::without_color("shadow-modal", "both shells"),
        ],
    );

    spec::dont(
        ui,
        theme,
        "자식 picker 를 띄울 때 scrim 을 한 겹 더 깔지 마라 — 같은 토큰이 두 번 곱해져 \
         부모만 있을 때보다 어두워진다.",
    );

    spec::note(
        ui,
        theme,
        "부모 팝업과 자식 파일 선택기는 같은 범위를 공유한다. 배경은 한 번만 어둡게 하고 그 위에 부모와 자식을 순서대로 그린다. 따라서 부모만 있을 때와 자식이 함께 있을 때의 배경 밝기가 같아야 한다.",
    );
    spec::note(
        ui,
        theme,
        "convert_surface and any other host popup bound to a surface gain this scrim; host \
         popups with no target binding stay on the window scrim. Child-lifetime and draft \
         policy are the host's, unchanged by this visual rule.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 서피스 범위 카드: 포인터를 덮인 소속 서피스의 버튼에 두면 hover 가 없고, 옆 서피스 버튼은 hover 를
    /// 받는다. 창 범위 카드는 두 버튼 모두 막는다.
    #[test]
    fn a_covered_scope_takes_the_pointer_from_widgets_below_and_leaves_the_rest() {
        let theme = tasty_themes::mocha_fallback();
        for (scope, owner_hover, adjacent_hover) in
            [(Scope::Surface, false, true), (Scope::Window, false, false)]
        {
            let ctx = egui::Context::default();
            let mut centers = None;
            let mut hovered = (false, false);
            for pointer in [None, Some(0), Some(0), Some(1), Some(1)] {
                let events = match (pointer, centers) {
                    (Some(i), Some((a, b))) => {
                        vec![egui::Event::PointerMoved(if i == 0 { a } else { b })]
                    }
                    _ => Vec::new(),
                };
                drop(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1600.0, 1200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (owner, adjacent) = shell_mock(ui, &theme, ShellProps::new(scope));
                            centers = Some((owner.rect.center(), adjacent.rect.center()));
                            match pointer {
                                Some(0) => hovered.0 = owner.hovered(),
                                Some(1) => hovered.1 = adjacent.hovered(),
                                _ => {}
                            }
                        });
                    },
                ));
            }
            assert_eq!(hovered, (owner_hover, adjacent_hover), "{scope:?}");
        }
    }
}
