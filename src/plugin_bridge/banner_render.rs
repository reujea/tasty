//! 플러그인 배너의 콘텐츠 영역과 입력을 전달하고 GPU 합성 영역을 기록한다.
//! 호스트 배너를 그린 뒤 호출하며, 실제 mesh는 호스트 egui 렌더 뒤에 합성한다.
//! 배너에는 키보드 포커스를 주지 않고 포인터·스크롤만 전달한다.

use egui::{Context, Event, Pos2, Rect};
use tasty_plugin_protocol::{
    BannerCloseReason, BannerSetContextParams, ModifiersWire, PointerButtonWire, RawInputEventWire,
    RawInputWire, ThemeWire,
};

use crate::adapters::ui::PluginBannerCloseKind;
use crate::app::plugin_display::PluginDisplay;
use crate::model::LogicalPx;
use crate::plugin_bridge::wire_scroll;
use crate::state::RequestContext;

/// 호스트에서 닫힌 배너와 플러그인 매니저에서 사라진 배너를 양쪽에 반영한다.
pub fn draw_plugin_banners(
    ctx: &Context,
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    plugin_manager: Option<PluginDisplay<'_>>,
) {
    state.plugin_mesh_banner_regions.clear();

    let slots = state.banners.take_plugin_mesh_slots();
    let closed = state.banners.drain_closed_plugin_banners();

    let Some(mgr) = plugin_manager else {
        state.plugin_mesh_banner_forward.clear();
        return;
    };

    // 렌더 중 매니저를 변경하지 않고, 메인 루프에 닫기 처리를 요청한다.
    for (instance_id, kind) in closed {
        let reason = match kind {
            PluginBannerCloseKind::Ttl => BannerCloseReason::Ttl,
            PluginBannerCloseKind::UserClose => BannerCloseReason::UserClose,
        };
        state.plugin_banner_closes.push((instance_id, reason));
    }

    let live_in_mgr: std::collections::HashSet<u64> =
        mgr.banner_instances().map(|(iid, _)| iid).collect();
    let orphan_ui: Vec<u64> = state
        .banners
        .plugin_instances()
        .filter(|iid| !live_in_mgr.contains(iid))
        .collect();
    for iid in orphan_ui {
        state.banners.close_by_instance(iid);
    }

    let live_slots: std::collections::HashSet<u64> = slots.iter().map(|s| s.instance_id).collect();
    state
        .plugin_mesh_banner_forward
        .retain(|k, _| live_slots.contains(k));

    if slots.is_empty() {
        return;
    }

    let ppp = ctx.pixels_per_point().max(f32::EPSILON);
    let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
    let current_theme = {
        let th = crate::theme::theme();
        ThemeWire {
            colors: th.to_colors(),
            is_light: th.is_light,
            ui_zoom: engine.settings.appearance.ui_scale_factor(),
        }
    };

    for slot in &slots {
        let content_rect = slot.content_rect;
        // 팝업 상자나 덮는 팝업이 덮은 자리는 배너 것이 아니다(포인터 대상이 팝업이다).
        let mut route = state
            .plugin_mesh_banner_forward
            .get(&slot.instance_id)
            .map(|f| f.pointer.clone())
            .unwrap_or_default();
        let raw_input = collect_mesh_banner_input(
            ctx,
            content_rect,
            pointer_pos,
            &|p| state.popup_owns_point(p),
            &mut route,
        );

        let physical = crate::plugin_bridge::mesh_region_of(content_rect, ppp);
        let w_px = physical.width.value().round().max(1.0) as u32;
        let h_px = physical.height.value().round().max(1.0) as u32;
        let geom = (w_px, h_px, ppp.to_bits());
        let has_input = !raw_input.events.is_empty();
        let has_frame = mgr.banner_mesh_frame(slot.instance_id).is_some();
        // 입력이나 크기 변경 없이 요청한 repaint도 처리한다.
        // fwd를 가변 차용하기 전에 RequestContext의 요청 집합에서 꺼낸다.
        let need_repaint = state
            .plugin_mesh_banner_pending_repaint
            .remove(&slot.instance_id);
        let fwd = state
            .plugin_mesh_banner_forward
            .entry(slot.instance_id)
            .or_default();
        fwd.pointer = route;
        let need_bootstrap = fwd.need_bootstrap(has_frame);
        fwd.watch_blank(
            has_frame,
            format_args!(
                "banner instance {} (plugin '{}')",
                slot.instance_id, slot.plugin_id
            ),
            &slot.plugin_id,
        );
        let geom_changed = fwd.geom_changed(geom);
        let theme_changed = fwd.theme_changed(&current_theme);
        let need_full = fwd.take_pending_full();
        if geom_changed || has_input || need_bootstrap || theme_changed || need_full || need_repaint
        {
            fwd.record_sent(geom, &current_theme, has_frame);
            state.dispatch_intent(
                crate::intent::Intent::Frame(
                    crate::app::frame_effect::DrawEffect::PluginDisplay(
                        crate::app::plugin_display::PluginDisplayRequest::Banner {
                            plugin: slot.plugin_id.clone(),
                            params: BannerSetContextParams {
                                instance_id: slot.instance_id,
                                width_px: w_px,
                                height_px: h_px,
                                pixels_per_point: ppp,
                                raw_input,
                                theme: Some(current_theme.clone()),
                                need_full_textures: need_full,
                            },
                        },
                    )
                    .into(),
                )
                .from_user_menu("plugin-render-context"),
            );
        }

        state
            .plugin_mesh_banner_regions
            .push((slot.instance_id, physical));

        let cut = mgr
            .banner_mesh_frame(slot.instance_id)
            .and_then(|f| f.banner_body_cut.as_ref());
        // 툴팁도 포인터 대상이 배너일 때만 띄운다.
        let tip_pointer = pointer_pos.filter(|p| !state.popup_owns_point(*p));
        show_body_cut_tooltip(ctx, slot, cut, tip_pointer);
    }
}

/// plugin이 말줄임한 본문의 전문을 배너 카드 아래 툴팁으로 보인다. plugin mesh 안이 아니라 호스트가 그리므로
/// 콘텐츠 영역에 잘리지 않고 포인터를 덮지 않는다. 규칙: docs/design/systems/banner.md#본문-줄-수.
fn show_body_cut_tooltip(
    ctx: &Context,
    slot: &crate::adapters::ui::PluginBannerMeshSlot,
    cut: Option<&tasty_plugin_protocol::BannerBodyCutWire>,
    pointer_pos: Option<Pos2>,
) {
    let id = egui::Id::new(("plugin_banner_body_tooltip", slot.instance_id));
    let theme = crate::theme::theme();
    let Some(cut) = cut else {
        // 잘림이 사라진 배너의 대기 타이머를 지운다.
        tasty_ui_widgets::show_plugin_banner_body_tooltip(
            ctx,
            &theme,
            id,
            "",
            Rect::NOTHING,
            slot.card_rect,
            None,
        );
        return;
    };
    let r = cut.body_rect;
    let body = Rect::from_min_size(
        slot.content_rect.min + egui::vec2(r.x, r.y),
        egui::vec2(r.width, r.height),
    )
    .intersect(slot.content_rect);
    tasty_ui_widgets::show_plugin_banner_body_tooltip(
        ctx,
        &theme,
        id,
        &cut.text,
        body,
        slot.card_rect,
        pointer_pos,
    );
}

/// 콘텐츠 기준 논리 좌표로 포인터·스크롤을 전달한다. 키·텍스트는 전달하지 않는다.
/// PointerGone은 콘텐츠 영역 포함 여부와 관계없이 전달해 이전 hover 상태를 지운다.
/// `on_popup`이 참인 좌표는 팝업이 받으므로 배너에 보내지 않는다. 뗌은 이 배너에서 누른 버튼만 보낸다
/// (`route`). 포인터가 배너를 떠나거나 덮이면 PointerGone을 보낸다.
fn collect_mesh_banner_input(
    ctx: &Context,
    content_rect: Rect,
    pointer_pos: Option<Pos2>,
    on_popup: &dyn Fn(Pos2) -> bool,
    route: &mut crate::plugin_bridge::MeshPointerRoute,
) -> RawInputWire {
    let origin = content_rect.min;
    let mine = |p: Pos2| content_rect.contains(p) && !on_popup(p);
    let pointer_inside = pointer_pos.is_some_and(mine);
    // ctx.input 안에서 같은 컨텍스트의 다른 잠금을 잡지 않도록 먼저 읽는다.
    let line = wire_scroll::line_scroll(ctx);
    ctx.input(|i| {
        let modifiers = map_modifiers(&i.modifiers);
        let mut events: Vec<RawInputEventWire> = Vec::new();
        for ev in &i.events {
            match ev {
                Event::PointerMoved(p) if mine(*p) => {
                    events.push(RawInputEventWire::PointerMoved {
                        x: p.x - origin.x,
                        y: p.y - origin.y,
                    });
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: m,
                } if if *pressed {
                    route.press(*button, mine(*pos))
                } else {
                    route.release(*button)
                } =>
                {
                    if let Some(button) = map_button(*button) {
                        events.push(RawInputEventWire::PointerButton {
                            x: pos.x - origin.x,
                            y: pos.y - origin.y,
                            button,
                            pressed: *pressed,
                            modifiers: map_modifiers(m),
                        });
                    }
                }
                // wire Scroll은 논리 포인트이므로 Line 입력은 노치 거리로 환산한다.
                Event::MouseWheel { unit, delta, .. } if pointer_inside => {
                    let (dx, dy) = wire_scroll::wheel_delta_to_points(
                        *unit,
                        *delta,
                        LogicalPx(i.screen_rect().height()),
                        line,
                    );
                    events.push(RawInputEventWire::Scroll {
                        x: dx.value(),
                        y: dy.value(),
                    });
                }
                Event::PointerGone => events.push(RawInputEventWire::PointerGone),
                _ => {}
            }
        }
        if route.hover(pointer_inside)
            && !events
                .iter()
                .any(|e| matches!(e, RawInputEventWire::PointerGone))
        {
            events.push(RawInputEventWire::PointerGone);
        }
        RawInputWire {
            time: None,
            focused: false,
            modifiers,
            events,
        }
    })
}

fn map_modifiers(m: &egui::Modifiers) -> ModifiersWire {
    ModifiersWire {
        alt: m.alt,
        ctrl: m.ctrl,
        shift: m.shift,
        mac_cmd: m.mac_cmd,
        command: m.command,
    }
}

fn map_button(b: egui::PointerButton) -> Option<PointerButtonWire> {
    match b {
        egui::PointerButton::Primary => Some(PointerButtonWire::Primary),
        egui::PointerButton::Secondary => Some(PointerButtonWire::Secondary),
        egui::PointerButton::Middle => Some(PointerButtonWire::Middle),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 잘린 본문의 콘텐츠 로컬 좌표를 창 좌표로 옮겨, 포인터가 본문 위에 머물면 카드 아래에 전문 툴팁을 그린다.
    fn body_cut_tooltip_text_at(pointer: Pos2) -> Option<Pos2> {
        let slot = crate::adapters::ui::PluginBannerMeshSlot {
            plugin_id: "com.test.banner".to_owned(),
            instance_id: 4,
            content_rect: Rect::from_min_size(Pos2::new(40.0, 20.0), egui::vec2(300.0, 72.0)),
            card_rect: Rect::from_min_max(Pos2::new(8.0, 8.0), Pos2::new(400.0, 100.0)),
        };
        let cut = tasty_plugin_protocol::BannerBodyCutWire {
            text: "the whole plugin body".to_owned(),
            body_rect: tasty_plugin_protocol::RectWire {
                x: 10.0,
                y: 4.0,
                width: 200.0,
                height: 45.0,
            },
        };
        let ctx = Context::default();
        let mut at = None;
        for i in 0..20 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                time: Some(f64::from(i) * 0.25),
                ..Default::default()
            };
            let out = ctx.run(input, |ctx| {
                show_body_cut_tooltip(ctx, &slot, Some(&cut), Some(pointer));
            });
            at = out.shapes.iter().find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text() == cut.text => Some(t.pos),
                _ => None,
            });
        }
        at
    }

    #[test]
    fn a_cut_plugin_body_gets_its_tooltip_below_the_card_at_the_body_left() {
        // 본문은 창 좌표 (50, 24)부터다.
        let at = body_cut_tooltip_text_at(Pos2::new(60.0, 30.0)).expect("tooltip");
        assert!(at.x > 50.0 && at.x < 50.0 + 12.0, "{at:?}");
        assert!(at.y > 100.0, "{at:?}");
    }

    #[test]
    fn a_pointer_outside_the_cut_body_shows_no_tooltip() {
        // 콘텐츠 영역 안이지만 본문 밖이다.
        assert!(body_cut_tooltip_text_at(Pos2::new(300.0, 80.0)).is_none());
    }

    fn collected_scroll(unit: egui::MouseWheelUnit, delta: egui::Vec2) -> Option<(f32, f32)> {
        collected_scroll_with_notch(unit, delta, tasty_settings::DEFAULT_WHEEL_LINE_SCROLL)
    }

    fn collected_scroll_with_notch(
        unit: egui::MouseWheelUnit,
        delta: egui::Vec2,
        notch: f32,
    ) -> Option<(f32, f32)> {
        let ctx = Context::default();
        // 호스트의 노치 거리를 지정하지 않으면 egui 기본값 40이 남는다.
        ctx.options_mut(|o| o.line_scroll_speed = notch);
        let content_rect =
            Rect::from_min_size(Pos2::new(40.0, 40.0), egui::Vec2::new(400.0, 120.0));
        let pointer = Pos2::new(100.0, 80.0);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            events: vec![Event::MouseWheel {
                unit,
                delta,
                modifiers: egui::Modifiers::default(),
            }],
            ..Default::default()
        };
        let mut wire = None;
        // 실제 입력 처리를 실행하되 렌더 결과는 이 검사에 사용하지 않는다.
        let _full_output = ctx.run(input, |ctx| {
            wire = Some(collect_mesh_banner_input(
                ctx,
                content_rect,
                Some(pointer),
                &|_| false,
                &mut Default::default(),
            ));
        });
        wire?.events.iter().find_map(|e| match e {
            RawInputEventWire::Scroll { x, y } => Some((*x, *y)),
            _ => None,
        })
    }

    #[test]
    fn a_wheel_notch_reaches_the_wire_as_the_shared_line_scroll_distance() {
        let (dx, dy) = collected_scroll(egui::MouseWheelUnit::Line, egui::Vec2::new(0.0, -1.0))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(dx, 0.0);
        assert_eq!(dy, -tasty_settings::DEFAULT_WHEEL_LINE_SCROLL);
        assert_ne!(dy, -1.0);
    }

    #[test]
    fn the_wire_distance_follows_the_host_option() {
        let (_, slow) = collected_scroll_with_notch(
            egui::MouseWheelUnit::Line,
            egui::Vec2::new(0.0, -1.0),
            20.0,
        )
        .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        let (_, fast) = collected_scroll_with_notch(
            egui::MouseWheelUnit::Line,
            egui::Vec2::new(0.0, -1.0),
            120.0,
        )
        .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(slow, -20.0);
        assert_eq!(fast, -120.0);
    }

    #[test]
    fn a_trackpad_delta_reaches_the_wire_unchanged() {
        let (dx, dy) = collected_scroll(egui::MouseWheelUnit::Point, egui::Vec2::new(1.5, -9.0))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!((dx, dy), (1.5, -9.0));
    }

    /// Given 배너 위 좌표를 팝업(상자나 덮는 범위)이 차지할 때, When 그 자리에서 누르고 휠을 굴리면, Then 배너에는
    /// 아무 입력도 가지 않는다. 같은 좌표가 팝업 것이 아니면 간다.
    #[test]
    fn a_banner_under_a_popup_gets_no_pointer_input() {
        let content_rect =
            Rect::from_min_size(Pos2::new(40.0, 40.0), egui::Vec2::new(400.0, 120.0));
        let pointer = Pos2::new(100.0, 80.0);
        let collect = |covered: bool| {
            let ctx = Context::default();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    egui::Vec2::new(1280.0, 800.0),
                )),
                events: vec![
                    Event::PointerMoved(pointer),
                    Event::PointerButton {
                        pos: pointer,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::Vec2::new(0.0, -3.0),
                        modifiers: egui::Modifiers::default(),
                    },
                ],
                ..Default::default()
            };
            let mut wire = None;
            drop(ctx.run(input, |ctx| {
                wire = Some(collect_mesh_banner_input(
                    ctx,
                    content_rect,
                    Some(pointer),
                    &|_| covered,
                    &mut Default::default(),
                ));
            }));
            wire.map_or(0, |w| w.events.len())
        };
        assert_eq!(collect(true), 0);
        assert!(collect(false) >= 3);
    }

    /// 이벤트 하나를 모은 배너 입력. `route`는 프레임을 넘어 이어진다.
    fn banner_events(
        route: &mut crate::plugin_bridge::MeshPointerRoute,
        pointer: Pos2,
        covered: bool,
        events: Vec<Event>,
    ) -> Vec<RawInputEventWire> {
        let content_rect =
            Rect::from_min_size(Pos2::new(40.0, 40.0), egui::Vec2::new(400.0, 120.0));
        let ctx = Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                egui::Vec2::new(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        };
        let mut wire = None;
        drop(ctx.run(input, |ctx| {
            wire = Some(collect_mesh_banner_input(
                ctx,
                content_rect,
                Some(pointer),
                &|_| covered,
                route,
            ));
        }));
        wire.map(|w| w.events).unwrap_or_default()
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        }
    }

    /// Given 덮인 자리에서 누른 뒤, When 드러난 배너 위에서 떼면, Then 배너는 그 뗌을 받지 않는다. Given 배너에서
    /// 누른 뒤, When 덮인 자리에서 떼면, Then 배너가 그 뗌을 받는다(뗌은 누름이 간 곳으로 간다).
    #[test]
    fn a_banner_gets_the_release_only_of_its_own_press() {
        let on_banner = Pos2::new(100.0, 80.0);
        let mut route = crate::plugin_bridge::MeshPointerRoute::default();
        assert!(
            banner_events(&mut route, on_banner, true, vec![button(on_banner, true)]).is_empty()
        );
        let release = banner_events(&mut route, on_banner, false, vec![button(on_banner, false)]);
        assert!(
            !release
                .iter()
                .any(|e| matches!(e, RawInputEventWire::PointerButton { .. })),
            "{release:?}"
        );

        let mut route = crate::plugin_bridge::MeshPointerRoute::default();
        let press = banner_events(&mut route, on_banner, false, vec![button(on_banner, true)]);
        assert!(
            press
                .iter()
                .any(|e| matches!(e, RawInputEventWire::PointerButton { pressed: true, .. }))
        );
        let release = banner_events(&mut route, on_banner, true, vec![button(on_banner, false)]);
        assert!(
            release
                .iter()
                .any(|e| matches!(e, RawInputEventWire::PointerButton { pressed: false, .. }))
        );
    }

    /// Given 포인터가 배너 위에 있을 때, When 팝업이 그 자리를 덮으면, Then 배너에 PointerGone을 보내 hover를 푼다.
    /// 한 번만 보낸다.
    #[test]
    fn a_banner_covered_under_the_pointer_gets_pointer_gone() {
        let on_banner = Pos2::new(100.0, 80.0);
        let mut route = crate::plugin_bridge::MeshPointerRoute::default();
        let moved = banner_events(
            &mut route,
            on_banner,
            false,
            vec![Event::PointerMoved(on_banner)],
        );
        assert!(
            !moved
                .iter()
                .any(|e| matches!(e, RawInputEventWire::PointerGone))
        );
        let covered = banner_events(&mut route, on_banner, true, vec![]);
        assert!(
            covered
                .iter()
                .any(|e| matches!(e, RawInputEventWire::PointerGone))
        );
        assert!(banner_events(&mut route, on_banner, true, vec![]).is_empty());
    }
}
