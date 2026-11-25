//! 버튼·아이콘 버튼·체크박스·스위치의 키보드 포커스 링(정본 base.css `:focus-visible`, Checkbox.jsx·Switch.jsx).
//! 상자에서 focus-ring-offset(Switch는 그 두 배) 띄운 focus-ring-width 링이고, 색은 놓인 바탕으로 정한다.
//! Tab 키로 포커스를 옮긴 실제 egui 프레임에서 링 사각형을 읽는다.

use egui::{Event, Modifiers, Pos2, RawInput, Rect, vec2};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Button, ButtonVariant, Ground, IconButton, checkbox, pinned_focus, switch};

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

fn tab() -> Event {
    Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

/// 한 패스를 그리고 (위젯이 알려 준 기준 사각형, 링 획 목록)을 돌려준다. 링은 focus_ring_width Outside 획이다.
fn pass(
    ctx: &egui::Context,
    th: &Theme,
    events: Vec<Event>,
    draw: &mut dyn FnMut(&mut egui::Ui) -> Rect,
) -> (Rect, Vec<(Rect, f32, egui::Color32)>) {
    let mut target = Rect::NOTHING;
    let out = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0))),
            focused: true,
            events,
            ..Default::default()
        },
        |c| {
            egui::CentralPanel::default().show(c, |ui| target = draw(ui));
        },
    );
    let rings = out
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.stroke.width == th.focus_ring_width.value()
                    && r.stroke_kind == egui::StrokeKind::Outside =>
            {
                Some((r.rect, r.corner_radius.nw as f32, r.stroke.color))
            }
            _ => None,
        })
        .collect();
    (target, rings)
}

/// 쉬는 패스에는 링이 없고, Tab으로 포커스를 준 뒤에는 `target.expand(gap)`에 `color` 링 하나가 있다.
fn assert_ring_after_tab(
    th: &Theme,
    gap: f32,
    color: egui::Color32,
    draw: &mut dyn FnMut(&mut egui::Ui) -> Rect,
) {
    let ctx = egui::Context::default();
    let (_, rest) = pass(&ctx, th, vec![], draw);
    assert!(rest.is_empty(), "no ring at rest: {rest:?}");
    pass(&ctx, th, vec![tab()], draw);
    let (target, rings) = pass(&ctx, th, vec![], draw);
    let got: Vec<_> = rings.iter().map(|(r, _, c)| (*r, *c)).collect();
    assert_eq!(got, vec![(target.expand(gap), color)]);
}

#[test]
fn a_focused_button_draws_the_ring_one_offset_off_its_box() {
    let th = theme();
    let gap = th.focus_ring_offset.value();
    for variant in [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Ghost,
    ] {
        assert_ring_after_tab(&th, gap, th.border_focus().to_egui(), &mut |ui| {
            Button::new("Save").variant(variant).show(ui, &th).rect
        });
    }
}

#[test]
fn a_button_on_an_active_ground_takes_the_raised_accent_ring() {
    let th = theme();
    assert_ring_after_tab(
        &th,
        th.focus_ring_offset.value(),
        th.accent_primary_raised().to_egui(),
        &mut |ui| {
            Button::new("Open")
                .variant(ButtonVariant::Ghost)
                .ground(Ground::Active)
                .show(ui, &th)
                .rect
        },
    );
}

/// 기본 제공 Latte. Mocha에서는 border-focus와 accent-primary-raised가 같은 blue라 바탕별 링 색을 가르지 못한다.
fn latte() -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT)
        .expect("builtin latte.toml ships valid");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors(colors, is_light.unwrap_or(true))
}

/// 앱의 버튼·아이콘 버튼은 panel(crust·mantle·base)과 raised 바탕에 놓인다. 두 바탕의 링은 border-focus이고
/// active 바탕만 accent-primary-raised다(정본 RULES focus-ring 표). 앱 자리별 링 아래 바탕은 보고서 표에 있다.
#[test]
fn the_ring_colour_follows_the_ground_under_it() {
    let th = latte();
    let focus = th.border_focus().to_egui();
    let on_active = th.accent_primary_raised().to_egui();
    assert_ne!(focus, on_active, "latte tells the two ring colours apart");
    let gap = th.focus_ring_offset.value();
    for (ground, color) in [
        (Ground::Panel, focus),
        (Ground::Raised, focus),
        (Ground::Active, on_active),
    ] {
        assert_ring_after_tab(&th, gap, color, &mut |ui| {
            Button::new("Open")
                .variant(ButtonVariant::Ghost)
                .ground(ground)
                .show(ui, &th)
                .rect
        });
        assert_ring_after_tab(&th, gap, color, &mut |ui| {
            IconButton::new()
                .ground(ground)
                .show(ui, &th, &|_, _, _| {})
                .rect
        });
    }
}

#[test]
fn a_focused_icon_button_draws_the_ring_one_offset_off_its_box() {
    let th = theme();
    assert_ring_after_tab(
        &th,
        th.focus_ring_offset.value(),
        th.border_focus().to_egui(),
        &mut |ui| IconButton::new().show(ui, &th, &|_, _, _| {}).rect,
    );
}

#[test]
fn a_focused_checkbox_draws_the_ring_one_offset_off_its_box() {
    let th = theme();
    let size = th.checkbox_size().value();
    let mut on = false;
    assert_ring_after_tab(
        &th,
        th.focus_ring_offset.value(),
        th.border_focus().to_egui(),
        &mut |ui| {
            let r = checkbox(ui, &th, &mut on, "Wrap", true, Ground::Panel).rect;
            Rect::from_min_size(
                egui::pos2(r.left(), r.center().y - size * 0.5),
                vec2(size, size),
            )
        },
    );
}

#[test]
fn a_focused_switch_draws_the_ring_two_offsets_off_its_track() {
    let th = theme();
    let (w, h) = (
        th.switch_track_width().value(),
        th.switch_track_height().value(),
    );
    let mut on = false;
    assert_ring_after_tab(
        &th,
        th.focus_ring_offset.value() * 2.0,
        th.border_focus().to_egui(),
        &mut |ui| {
            let r = switch(ui, &th, &mut on, Some("Blink"), true).rect;
            Rect::from_min_size(egui::pos2(r.left(), r.center().y - h * 0.5), vec2(w, h))
        },
    );
}

/// 초기 포커스를 요청한 버튼은 그 첫 패스에 링이 있다.
#[test]
fn a_button_that_requests_focus_shows_the_ring_in_the_same_pass() {
    let th = theme();
    let ctx = egui::Context::default();
    let (target, rings) = pass(&ctx, &th, vec![], &mut |ui| {
        Button::new("Cancel")
            .variant(ButtonVariant::Ghost)
            .request_focus(true)
            .show(ui, &th)
            .rect
    });
    let gap = th.focus_ring_offset.value();
    assert_eq!(rings.len(), 1, "{rings:?}");
    assert_eq!(rings[0].0, target.expand(gap));
}

/// 고정 표본 범위 안의 활성 컨트롤은 포커스 없이도 링을 그리고, 비활성 컨트롤은 그리지 않는다.
#[test]
fn pinned_focus_draws_the_ring_on_enabled_controls_only() {
    let th = theme();
    let ctx = egui::Context::default();
    for enabled in [true, false] {
        let (target, rings) = pass(&ctx, &th, vec![], &mut |ui| {
            pinned_focus(ui, |ui| {
                Button::new("Save").enabled(enabled).show(ui, &th).rect
            })
        });
        if enabled {
            assert_eq!(rings.len(), 1);
            assert_eq!(rings[0].0, target.expand(th.focus_ring_offset.value()));
        } else {
            assert!(rings.is_empty());
        }
    }
}

/// 링을 끈 버튼(접힌 팝업이 자기 조건으로 그린다)은 포커스가 있어도 그리지 않는다.
#[test]
fn a_button_with_the_ring_turned_off_draws_none() {
    let th = theme();
    let ctx = egui::Context::default();
    let (_, rings) = pass(&ctx, &th, vec![], &mut |ui| {
        Button::new("Show")
            .focus_ring(false)
            .request_focus(true)
            .show(ui, &th)
            .rect
    });
    assert!(rings.is_empty());
}
