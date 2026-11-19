//! 플러그인 팝업의 배경·테두리를 그리고 입력과 콘텐츠 크기를 전달한다.
//! 콘텐츠 mesh는 GPU에서 별도로 합성한다.
//! 플러그인 팝업은 동적 instance_id를 사용하므로 고정 PopupId를 쓰는 호스트 팝업과 따로 관리한다.

use std::collections::HashSet;

use super::popup_scope::popup_scope;
use egui::{Context, Event, Id, ImeEvent, Order, Pos2, Rect, Stroke, Vec2};
use tasty_plugin_manifest::PopupRendering;
#[cfg(test)]
use tasty_plugin_manifest::PopupScopeDecl;
use tasty_plugin_protocol::{
    ImeWire, ModifiersWire, PointerButtonWire, PopupCloseReason, PopupSetContextParams,
    RawInputEventWire, RawInputWire, ThemeWire,
};

use crate::adapters::ui::LayoutContext;
use crate::adapters::ui::popup::display::{self, CollapsedDraw};
use crate::adapters::ui::popup::occlusion::{Occluder, PointOwnership, point_ownership};
use crate::adapters::ui::popup::{self, PopupManager, PopupScope, press};
use crate::app::plugin_display::PluginDisplay;
use crate::model::LogicalPx;
use crate::plugin::manifest::PopupAnchor;
use crate::plugin_bridge::wire_scroll;
use crate::state::RequestContext;
use tasty_ui_widgets::CollapsedPlacement;

/// `size_hint`를 생략한 팝업의 크기. 실행 중 선언과 같은 값을 쓰도록 매니페스트 크레이트의 상수를 읽는다.
fn default_popup_size() -> Vec2 {
    let s = tasty_plugin_manifest::DEFAULT_POPUP_SIZE;
    Vec2::new(s.width.value(), s.height.value())
}

/// 호스트 팝업과 같은 범위·가시성 규칙으로 배치하고 닫기 요청을 모은다.
/// 보이지 않는 범위는 렌더·입력 대상에서 제외하지만 인스턴스와 전송 상태는 유지한다.
/// layout에는 같은 프레임에서 호스트 팝업이 사용한 값을 받는다.
pub fn draw_plugin_popups(
    ctx: &Context,
    state: &mut RequestContext,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    plugin_manager: Option<PluginDisplay<'_>>,
    layout: Option<&LayoutContext>,
) {
    // 조기 반환 때도 이전 프레임의 합성·입력·IME 상태가 남지 않도록 먼저 비운다.
    state.plugin_mesh_popup_regions.clear();
    state.plugin_popup_layers.clear();
    state.plugin_popup_open = false;
    state.plugin_popup_hittest.clear();
    state.plugin_popup_covers.clear();
    state.plugin_popup_key_candidates.clear();
    state.plugin_popup_replies.clear();
    state.plugin_popup_ime_cursor_area = None;

    let Some(mgr) = plugin_manager else {
        state.webview_overlay.prev_plugin_rects.clear();
        state.plugin_mesh_popup_forward.clear();
        state.plugin_popup_user_activated.clear();
        state.plugin_popup_edit_sent.clear();
        return;
    };

    // 이 창에 결속하지 않은 창 범위 인스턴스는 다른 창의 것이다. 닫힌 인스턴스의 결속은 지운다.
    let open_ids: HashSet<u64> = mgr.popup_instances().map(|(id, _)| id).collect();
    state
        .plugin_window_popups
        .retain(|id| open_ids.contains(id));
    let window_popups = &state.plugin_window_popups;
    let mesh_snaps = mesh_snapshots(mgr.popup_instances().filter(|(id, inst)| {
        crate::plugin_bridge::popup_scope::drawn_in_window(
            window_popups,
            *id,
            inst.contribute.scope,
        )
    }));

    let live_mesh: HashSet<u64> = mesh_snaps.iter().map(|s| s.instance_id).collect();
    state
        .plugin_mesh_popup_forward
        .retain(|k, _| live_mesh.contains(k));
    // 사용자 활성화 기록은 조회로 소비하지 않고 팝업이 닫힐 때 제거한다.
    // 열린 동안 같은 팝업을 근거로 여러 번 사용자 요청을 보낼 수 있다(docs/features/file-handler/index.md).
    state
        .plugin_popup_user_activated
        .retain(|k, _| live_mesh.contains(k));
    // 닫힌 팝업은 다시 열면 범위 판정부터 한다.
    state
        .plugin_popup_expanded
        .retain(|k| live_mesh.contains(k));
    // 닫힌 팝업에 줄 선 편집 요청은 받을 곳이 없다. 그 보류는 App이 대상 닫힘으로 끝낸다.
    state
        .plugin_popup_edit_requests
        .retain(|(k, _)| live_mesh.contains(k));
    state
        .plugin_popup_edit_sent
        .retain(|k, _| live_mesh.contains(k));

    let screen_rect = ctx.screen_rect();
    let press = crate::adapters::ui::popup::frame::frame_press(ctx, state, _engine, layout);
    // 누른 프레임에는 상자 안 판정도 누름 스냅샷의 좌표를 쓴다. host 팝업과 같은 점이다.
    let pointer_pos = press
        .map(|p| p.pos)
        .or_else(|| ctx.input(|i| i.pointer.hover_pos()));
    let escape_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    // 다른 팝업에 가린 좌표를 판정하려면 모든 팝업의 배치가 먼저 필요하다.
    let placed = place_visible(
        mesh_snaps,
        layout,
        screen_rect,
        pointer_pos,
        &state.plugin_popup_expanded,
        display::collapsed_frames(ctx),
    );
    state.plugin_popup_key_candidates.extend(
        placed
            .iter()
            .map(|(snap, _)| (snap.instance_id, snap.z_seq)),
    );
    state.plugin_popup_replies.extend(
        placed
            .iter()
            .filter(|(snap, _)| mgr.supports_edit_outcome(&snap.plugin_id))
            .map(|(snap, _)| snap.instance_id),
    );

    if placed.is_empty() {
        state.webview_overlay.prev_plugin_rects.clear();
        return;
    }
    // 불투명 모드 WebView 페이지(직전 동기화 값). 비어 있으면 모든 셸이 원래 모양이다.
    let pages = tasty_egui_theme::overlay_edge::opaque_pages(ctx);
    let prev_plugin_rects = std::mem::take(&mut state.webview_overlay.prev_plugin_rects);

    // host/plugin은 같은 z_seq를 사용한다. 호스트 rect는 현재 프레임의 값이며,
    // 반대로 호스트가 읽는 플러그인 rect는 이전 프레임의 값이다.
    let mut occluders: Vec<Occluder> = state
        .host_popup_hittest
        .iter()
        .map(|h| Occluder {
            rect: h.rect,
            z_seq: h.z_seq,
        })
        .collect();
    occluders.extend(state.host_popup_covers.iter().copied());
    occluders.extend(placed.iter().map(|(s, r)| Occluder {
        rect: *r,
        z_seq: s.z_seq,
    }));
    // 더 큰 z_seq만 가림으로 보므로 자기 rect는 자신을 가리지 않는다.
    // 입력은 팝업 쌍별로 비교하지만 렌더는 host/plugin 두 그룹을 비교하므로
    // 여러 팝업의 z가 교차하면 렌더 순서와 입력 순서가 다를 수 있다.
    // 제한: docs/design/systems/popup.md의 Host ↔ Plugin popup z-order.

    state
        .plugin_popup_hittest
        .extend(placed.iter().map(|(s, r)| Occluder {
            rect: *r,
            z_seq: s.z_seq,
        }));

    state.plugin_popup_open = true;

    // host/plugin 전체에서 최상단 팝업만 키·Escape를 받는다. 창의 키 처리도 같은 판정을 쓴다.
    let key_target = state.plugin_popup_key_target_in_render();

    let (scrim_rects, scrim_paints) = scrim_plan(&placed, layout, screen_rect);
    // 덮는 팝업의 덮는 사각형(배치 범위)도 가림으로 본다.
    let covers: Vec<Occluder> = placed
        .iter()
        .zip(&scrim_rects)
        .filter(|((s, _), _)| s.covers())
        .map(|((s, _), r)| Occluder {
            rect: *r,
            z_seq: s.z_seq,
        })
        .collect();
    occluders.extend(covers.iter().copied());
    state.plugin_popup_covers.extend(covers);

    let ppp = ctx.pixels_per_point().max(f32::EPSILON);
    let current_theme = {
        let th = crate::theme::theme();
        ThemeWire {
            colors: th.to_colors(),
            is_light: th.is_light,
            ui_zoom: _engine.settings.appearance.ui_scale_factor(),
        }
    };
    for (idx, (snap, rect)) in placed.iter().enumerate() {
        let rect = *rect;
        let scope_rect = scrim_rects[idx];
        let ownership = pointer_pos.map(|p| point_ownership(rect, snap.z_seq, &occluders, p));

        let layer_id = egui::LayerId::new(
            Order::Foreground,
            Id::new("plugin_mesh_popup").with(snap.instance_id),
        );
        // 덮는 범위의 아래 위젯 입력은 팝업 바로 아래 막는 레이어가 받는다.
        if snap.covers() {
            state
                .plugin_popup_layers
                .push(crate::adapters::ui::popup::press::block_pointer(
                    ctx,
                    layer_id.id.with("cover_block"),
                    scope_rect,
                ));
        }
        state.plugin_popup_layers.push(layer_id);
        let has_key_focus = key_target == Some(snap.instance_id);

        if let SnapDisplay::Collapsed(frame) = snap.display {
            // 접힌 팝업은 보기 버튼 프레임만 그리고 플러그인에 크기·입력을 보내지 않는다.
            let look = CollapsedDraw {
                scrim: (scrim_paints[idx]
                    && !tasty_egui_theme::overlay_edge::scrim_skipped(scope_rect, &pages))
                .then_some(scope_rect),
                on_scrim: snap.layout.covers_scope,
                focused: has_key_focus,
            };
            state
                .webview_overlay
                .targets
                .push(display::collapsed_overlay_target(
                    crate::state::webview_overlay::TargetKind::PluginPopup,
                    layer_id,
                    frame.rect,
                    scope_rect,
                    &look,
                    ppp,
                ));
            let th = crate::theme::theme();
            if display::draw_collapsed(ctx, &th, layer_id, frame, scope_rect, look) {
                state.plugin_popup_expanded.insert(snap.instance_id);
                state.plugin_popup_focus_bumps.push(snap.instance_id);
            }
            drop(th);
            route_press(
                state,
                snap,
                rect,
                scope_rect,
                &occluders,
                press.as_ref(),
                ownership,
            );
            if escape_pressed && has_key_focus && snap.closes_on_escape() {
                state
                    .plugin_popup_closes
                    .push((snap.instance_id, PopupCloseReason::Escape));
            }
            continue;
        }

        let expanded = snap.display == SnapDisplay::Expanded;
        // 펼친 팝업은 위쪽 host 제목줄 아래가 콘텐츠다.
        let shell_rect = if expanded {
            Rect::from_min_max(display::host_bar_rect(rect).left_bottom(), rect.max)
        } else {
            rect
        };
        let content_rect = shell_rect.shrink(popup::content_margin().value());
        // surface 범위를 벗어난 팝업 배경·테두리가 이웃 영역에 보이지 않도록 자른다.
        let painter = ctx.layer_painter(layer_id).with_clip_rect(scope_rect);
        let th = crate::theme::theme();
        // 페이지와 겹친 셸은 전체가 각진 모서리·그림자 없음이고, 범위에 페이지가 보이면 scrim 을
        // 그리지 않는다(docs/design/systems/webview.md).
        let edge = tasty_egui_theme::overlay_edge::edge_for(rect, false, &pages);
        let radius = edge.radius(&th, th.corner_radius).value();
        let scrim = (scrim_paints[idx]
            && !tasty_egui_theme::overlay_edge::scrim_skipped(scope_rect, &pages))
        .then_some(scope_rect);
        if let Some(scrim_rect) = scrim {
            painter.rect_filled(scrim_rect, 0.0, th.scrim().to_egui());
        }
        // 어두운 배경에서도 경계가 보이도록 팝업 배경보다 먼저 그림자를 그린다.
        let shadow = edge.shadow(&th, th.shadow_modal()).to_egui();
        painter.add(shadow.as_shape(rect, radius));
        let target_rect =
            crate::state::webview_overlay::plugin_box(layer_id, rect, &prev_plugin_rects);
        state
            .webview_overlay
            .targets
            .push(crate::state::webview_overlay::OverlayTarget {
                kind: crate::state::webview_overlay::TargetKind::PluginPopup,
                layer: layer_id,
                rect: target_rect,
                opaque_rect: crate::state::webview_overlay::outlined_box(
                    target_rect,
                    th.border_width,
                    scope_rect,
                    ppp,
                ),
                paint_rect: (target_rect + shadow.margin()).intersect(scope_rect),
                scrim,
            });
        state
            .webview_overlay
            .prev_plugin_rects
            .push((layer_id, rect));
        paint_shell_background_excluding_content(
            &painter,
            rect,
            content_rect,
            radius,
            th.bg_panel().to_egui(),
        );
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(th.border_width.value(), th.border_default().to_egui()),
            egui::StrokeKind::Outside,
        );

        if expanded {
            let btn = display::title_end_btn_rect(display::host_bar_rect(rect));
            let hovered = ownership == Some(PointOwnership::Mine)
                && pointer_pos.is_some_and(|p| btn.contains(p));
            display::paint_host_bar(
                ctx,
                &painter,
                &th,
                layer_id,
                display::HostBar {
                    box_rect: rect,
                    radius,
                    title_edge: th.border_frame().to_egui(),
                    hovered,
                    pressed: hovered && ctx.input(|i| i.pointer.primary_down()),
                    tip_id: Id::new("plugin_popup.collapse_tooltip").with(snap.instance_id),
                },
            );
            if hovered && press.as_ref().is_some_and(|p| p.is_primary()) {
                state.plugin_popup_expanded.remove(&snap.instance_id);
            }
        } else if has_key_focus && snap.scope != PopupScope::Window {
            // 키 주인인 좁은 범위 팝업은 셸 위쪽에 포커스 가장자리를 켠다.
            tasty_ui_widgets::paint_popup_focus_edge(&painter, &th, rect);
        }

        // 아래 팝업에 키가 전달되면 Escape 하나로 여러 팝업이 닫힐 수 있다.
        let mut route = state
            .plugin_mesh_popup_forward
            .get(&snap.instance_id)
            .map(|f| f.pointer.clone())
            .unwrap_or_default();
        let mut raw_input = collect_mesh_popup_input(
            ctx,
            content_rect,
            pointer_pos,
            ownership == Some(PointOwnership::OccludedByHigher),
            has_key_focus,
            &mut route,
        );
        // 펼친 팝업의 Esc는 다시 접기로 소비한다. 내용(markdown 파일 열기 등)이 받으면 취소로 닫힌다.
        let escape = escape_action(
            escape_pressed && has_key_focus,
            expanded,
            snap.closes_on_escape(),
        );
        if escape == EscapeAction::Refold {
            drop_escape_keys(&mut raw_input);
        }
        // 보류한 편집 키의 요청은 이 프레임 입력 뒤에 붙인다. 그 뒤의 입력은 보류 뒤에 줄을 서 있다.
        raw_input.events.extend(take_edit_requests(
            &mut state.plugin_popup_edit_requests,
            snap.instance_id,
        ));

        // 첫 프레임을 받기 전 context를 반복 요청하면 폰트 atlas가 포함된
        // 프레임이 후속 프레임에 대체될 수 있어 초기 요청은 한 번만 보낸다.
        let physical = crate::plugin_bridge::mesh_region_of(content_rect, ppp);
        let w_px = physical.width.value().round().max(1.0) as u32;
        let h_px = physical.height.value().round().max(1.0) as u32;
        let geom = (w_px, h_px, ppp.to_bits());
        let has_input = !raw_input.events.is_empty();
        if sends_edit(&raw_input) {
            state
                .plugin_popup_edit_sent
                .insert(snap.instance_id, next_edit_seq());
        }
        if is_user_activation(&raw_input) {
            state
                .plugin_popup_user_activated
                .insert(snap.instance_id, snap.plugin_id.clone());
        }
        let has_frame = mgr.popup_mesh_frame(snap.instance_id).is_some();
        let fwd = state
            .plugin_mesh_popup_forward
            .entry(snap.instance_id)
            .or_default();
        fwd.pointer = route;
        let need_bootstrap = fwd.need_bootstrap(has_frame);
        fwd.watch_blank(
            has_frame,
            format_args!(
                "popup instance {} (plugin '{}')",
                snap.instance_id, snap.plugin_id
            ),
            &snap.plugin_id,
        );
        let geom_changed = fwd.geom_changed(geom);
        let theme_changed = fwd.theme_changed(&current_theme);
        let need_full = fwd.take_pending_full();
        // 비동기 조회 결과와 플러그인 자체 repaint 요청도 크기·입력 변경 없이 전달한다.
        let need_repaint = state
            .plugin_mesh_popup_pending_repaint
            .remove(&snap.instance_id);
        if geom_changed || has_input || need_bootstrap || theme_changed || need_full || need_repaint
        {
            state
                .plugin_mesh_popup_forward
                .entry(snap.instance_id)
                .or_default()
                .record_sent(geom, &current_theme, has_frame);
            state.dispatch_intent(
                crate::intent::Intent::Frame(
                    crate::app::frame_effect::DrawEffect::PluginDisplay(
                        crate::app::plugin_display::PluginDisplayRequest::Popup {
                            plugin: snap.plugin_id.clone(),
                            params: PopupSetContextParams {
                                instance_id: snap.instance_id,
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
            .plugin_mesh_popup_regions
            .push((snap.instance_id, physical));

        // 키 포커스가 있는 팝업의 IME 캐럿만 창 좌표로 저장한다.
        if has_key_focus
            && let Some(ime) = mgr
                .popup_mesh_frame(snap.instance_id)
                .and_then(|f| f.ime_cursor.as_ref())
        {
            state.plugin_popup_ime_cursor_area = Some(crate::plugin_bridge::mesh_ime_cursor_area(
                physical, ime, ppp,
            ));
        }

        route_press(
            state,
            snap,
            rect,
            scope_rect,
            &occluders,
            press.as_ref(),
            ownership,
        );
        match escape {
            // 다시 접기는 닫기 사건이 없다.
            EscapeAction::Refold => {
                state.plugin_popup_expanded.remove(&snap.instance_id);
            }
            EscapeAction::Close => state
                .plugin_popup_closes
                .push((snap.instance_id, PopupCloseReason::Escape)),
            EscapeAction::None => {}
        }
    }

    // egui는 누른 영역(덮는 범위를 막는 레이어)을 맨 위로 올린다. 모든 레이어를 함께 올려 막는 레이어가 팝업
    // 아래에 남게 한다. 함께 올린 레이어끼리는 egui가 기존 순서를 유지한다.
    if press.is_some() {
        for layer in &state.plugin_popup_layers {
            ctx.move_to_top(*layer);
        }
    }
}

/// 누름을 팝업에 나눈다. 상자 안이면 앞으로 가져오고, 상자 밖이면 host와 같은 범위 규칙(`press::outside_press`)대로
/// 닫거나 앞으로 가져온다. plugin 팝업은 포커스 해제 상태가 없어 `Unfocus`는 아무 일도 하지 않는다. 렌더 중에는
/// 매니저를 변경하지 않고 메인 루프에 요청한다.
fn route_press(
    state: &mut RequestContext,
    snap: &MeshSnap,
    box_rect: Rect,
    cover_rect: Rect,
    occluders: &[Occluder],
    press: Option<&crate::adapters::ui::popup::route::RoutedPress>,
    ownership: Option<PointOwnership>,
) {
    let Some(press) = press else {
        return;
    };
    if ownership == Some(PointOwnership::Mine) {
        state.plugin_popup_focus_bumps.push(snap.instance_id);
    }
    let target = outside_target(
        snap,
        box_rect,
        cover_rect,
        state.plugin_popup_has_open_child(snap.instance_id),
    );
    match press::outside_press(&target, press, occluders) {
        press::OutsidePress::Close => state
            .plugin_popup_closes
            .push((snap.instance_id, PopupCloseReason::OutsideClick)),
        press::OutsidePress::Focus => state.plugin_popup_focus_bumps.push(snap.instance_id),
        press::OutsidePress::None | press::OutsidePress::Unfocus => {}
    }
}

/// plugin 팝업 하나를 바깥 누름 판정 입력으로 옮긴다.
fn outside_target(
    snap: &MeshSnap,
    box_rect: Rect,
    cover_rect: Rect,
    has_open_child: bool,
) -> press::OutsideTarget<'_> {
    press::OutsideTarget {
        placement_scope: &snap.scope,
        logical_scope: &snap.logical_scope,
        z_seq: snap.z_seq,
        box_rect: Some(box_rect),
        cover_rect: snap.covers().then_some(cover_rect),
        closes_on_outside_click: snap.dismiss_on_outside_click,
        has_open_child,
    }
}

struct MeshSnap {
    instance_id: u64,
    plugin_id: String,
    anchor: PopupAnchor,
    /// 배치 범위. 좁은 범위면 창으로 바뀐다.
    scope: PopupScope,
    /// 선언·결속으로 정한 논리 범위. 바뀌지 않는다.
    logical_scope: PopupScope,
    size: Vec2,
    dismiss_on_outside_click: bool,
    /// 덮음·좁은 범위·Esc 선언.
    layout: tasty_plugin_manifest::PopupLayout,
    z_seq: u64,
    display: SnapDisplay,
}

impl MeshSnap {
    /// 배치 범위를 덮는지. 펼친 팝업은 선언과 무관하게 창을 덮는다(host `PopupState::covers`와 같다).
    fn covers(&self) -> bool {
        self.layout.covers_scope || self.display == SnapDisplay::Expanded
    }

    /// Esc가 이 팝업을 닫는지. plugin 팝업에는 포커스 해제 상태가 없어 `unfocus`도 닫지 않는다.
    fn closes_on_escape(&self) -> bool {
        self.layout.escape == tasty_plugin_protocol::popup_decl::PopupEscape::Close
    }
}

/// 플러그인 팝업의 이번 프레임 표시 상태. host 팝업의 `PopupDisplay`와 같은 전이를 따른다.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SnapDisplay {
    /// 범위 안에 그대로 그린다.
    Fits,
    /// 범위 안의 보기 버튼 프레임만 그린다.
    Collapsed(CollapsedPlacement),
    /// 창 위로 펼쳐 host 제목줄을 붙여 그린다.
    Expanded,
}

// 숨겨진 범위의 인스턴스도 유지해야 다시 보일 때 전송 상태를 복원할 수 있다.
fn mesh_snapshots<'a>(
    instances: impl Iterator<Item = (u64, &'a tasty_host_plugin::PopupInstance)>,
) -> Vec<MeshSnap> {
    let mut mesh_snaps: Vec<MeshSnap> = Vec::new();
    for (id, inst) in instances {
        let size = inst
            .contribute
            .size_hint
            .map(|s| Vec2::new(s.width as f32, s.height as f32))
            .unwrap_or_else(default_popup_size);
        // 대상 없는 surface 선언 인스턴스는 그릴 범위가 없다. 여는 경로가 이런 인스턴스를 만들지 않는다.
        let Some(scope) = popup_scope(inst.contribute.scope, inst.scope_surface) else {
            tracing::debug!(
                "popup '{}/{}' has no surface target; not drawn",
                inst.plugin_id,
                inst.popup_id
            );
            continue;
        };
        match inst.contribute.rendering {
            PopupRendering::EguiMesh => mesh_snaps.push(MeshSnap {
                instance_id: id,
                plugin_id: inst.plugin_id.clone(),
                anchor: inst.contribute.anchor,
                scope: scope.clone(),
                logical_scope: scope,
                size,
                dismiss_on_outside_click: inst.contribute.dismiss_on_outside_click,
                layout: inst.contribute.layout,
                z_seq: inst.z_seq,
                display: SnapDisplay::Fits,
            }),
        }
    }
    // HashMap 순회에 의존하지 않도록 콘텐츠 합성 순서를 z_seq로 정렬한다.
    // 직접 그리는 셸 레이어의 순서는 이 정렬만으로 정해지지 않는다.
    // gfx/gpu/egui_bridge.rs의 enforce_host_plugin_popup_z_order를 함께 사용한다.
    mesh_snaps.sort_by_key(|s| s.z_seq);
    mesh_snaps
}

/// 표시 상태를 정하고 배치한다. 논리 범위가 숨으면 펼친 팝업도 그리지 않는다. 펼친 팝업은 배치 범위가
/// 창이고 크기를 유지한 채 창 가운데(넘치면 왼쪽·위)에 둔다. 범위가 선언 크기보다 작으면 범위 안 보기 버튼
/// 프레임으로 접는다. `frames`는 라벨·아이콘 버튼 프레임 크기다.
fn place_visible(
    snaps: Vec<MeshSnap>,
    layout: Option<&LayoutContext>,
    screen_rect: Rect,
    pointer_pos: Option<Pos2>,
    expanded: &HashSet<u64>,
    frames: (Vec2, Vec2),
) -> Vec<(MeshSnap, Rect)> {
    snaps
        .into_iter()
        .filter_map(|mut snap| {
            if !PopupManager::is_scope_visible(&snap.logical_scope, layout) {
                return None;
            }
            if expanded.contains(&snap.instance_id) {
                snap.scope = PopupScope::Window;
                snap.display = SnapDisplay::Expanded;
                let size = snap.size + Vec2::new(0.0, popup::title_bar_height().value());
                let centered = Rect::from_center_size(screen_rect.center(), size).min;
                let pos = clamp_to_bounds(centered, size, screen_rect);
                return Some((snap, Rect::from_min_size(pos, size)));
            }
            let input = display::FitInput {
                scope_rect: (snap.scope != PopupScope::Window)
                    .then(|| PopupManager::scope_rect(&snap.scope, layout))
                    .flatten(),
                fit: snap.layout.fit,
                scope_inset: snap.layout.scope_inset,
                needed: snap.size,
                anchor: (snap.anchor == PopupAnchor::Cursor)
                    .then_some(pointer_pos)
                    .flatten(),
            };
            let inset_cap = crate::theme::theme().spacing_sm.value();
            if let Some(frame) = display::collapse_placement(input, inset_cap, frames) {
                snap.display = SnapDisplay::Collapsed(frame);
                return Some((snap, frame.rect));
            }
            let rect = place_popup(
                snap.anchor,
                &snap.scope,
                snap.size,
                layout,
                screen_rect,
                pointer_pos,
            )?;
            Some((snap, rect))
        })
        .collect()
}

/// 창마다 플러그인 팝업에 편집을 보낸 순서. 모든 창이 함께 쓰므로 가장 최근에 보낸 창을 고를 수 있다.
static EDIT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_edit_seq() -> u64 {
    EDIT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// 복사 결과가 돌아올 편집(복사·잘라내기)을 보내는지.
fn sends_edit(raw: &RawInputWire) -> bool {
    raw.events
        .iter()
        .any(|ev| matches!(ev, RawInputEventWire::Copy | RawInputEventWire::Cut))
}

/// App 이 `instance`에 보내려고 줄 세운 편집 요청을 넣은 순서대로 꺼낸다. 다른 팝업의 요청은 남긴다.
fn take_edit_requests(
    queued: &mut Vec<(u64, RawInputEventWire)>,
    instance: u64,
) -> Vec<RawInputEventWire> {
    let mut taken = Vec::new();
    queued.retain(|(id, event)| {
        if *id == instance {
            taken.push(event.clone());
            false
        } else {
            true
        }
    });
    taken
}

/// 버튼·키 누름이 전달될 때 사용자 활성화로 기록한다.
/// 포인터 이동·휠·떼기는 포함하지 않는다.
fn is_user_activation(raw: &RawInputWire) -> bool {
    raw.events.iter().any(|ev| {
        matches!(
            ev,
            RawInputEventWire::PointerButton { pressed: true, .. }
                | RawInputEventWire::Key { pressed: true, .. }
        )
    })
}

/// 키 주인 plugin 팝업이 받은 Esc 의 처리. 펼친 팝업은 선언과 관계없이 다시 접고 내용에 Esc 를 보내지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeAction {
    None,
    Refold,
    Close,
}

fn escape_action(owner_escape: bool, expanded: bool, closes_on_escape: bool) -> EscapeAction {
    match (owner_escape, expanded, closes_on_escape) {
        (false, ..) => EscapeAction::None,
        (true, true, _) => EscapeAction::Refold,
        (true, false, true) => EscapeAction::Close,
        (true, false, false) => EscapeAction::None,
    }
}

/// 다시 접기로 소비한 Esc 키를 mesh 입력에서 뺀다. 다른 키·텍스트는 그대로 둔다.
fn drop_escape_keys(raw: &mut RawInputWire) {
    raw.events.retain(
        |e| !matches!(e, RawInputEventWire::Key { key, .. } if key == egui::Key::Escape.name()),
    );
}

/// 포인터는 가리지 않은 콘텐츠 영역의 입력만 전달하며 좌표는 콘텐츠 기준 논리 포인트다. 뗌은 이 팝업에서
/// 누른 버튼만 보낸다(`route`). PointerGone은 영역과 관계없이 전달하고, 포인터가 콘텐츠를 떠나거나 가려지면
/// 보낸다. 키·텍스트·IME·편집(붙여넣기·복사·잘라내기)은 최상단
/// 팝업에만 보낸다. 빈 붙여넣기는 보내지 않는다.
fn collect_mesh_popup_input(
    ctx: &Context,
    content_rect: Rect,
    pointer_pos: Option<Pos2>,
    pointer_occluded: bool,
    has_key_focus: bool,
    route: &mut crate::plugin_bridge::MeshPointerRoute,
) -> RawInputWire {
    let origin = content_rect.min;
    let pointer_inside = !pointer_occluded && pointer_pos.is_some_and(|p| content_rect.contains(p));
    let accepts_pointer = |p: Pos2| !pointer_occluded && content_rect.contains(p);
    // ctx.input 안에서 같은 컨텍스트의 다른 잠금을 잡지 않도록 먼저 읽는다.
    let line = wire_scroll::line_scroll(ctx);
    ctx.input(|i| {
        let modifiers = map_modifiers(&i.modifiers);
        let mut events: Vec<RawInputEventWire> = Vec::new();
        for ev in &i.events {
            match ev {
                Event::PointerMoved(p) if accepts_pointer(*p) => {
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
                    route.press(*button, accepts_pointer(*pos))
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
                Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers: m,
                    ..
                } if has_key_focus => {
                    events.push(RawInputEventWire::Key {
                        key: key.name().to_string(),
                        pressed: *pressed,
                        repeat: *repeat,
                        modifiers: map_modifiers(m),
                    });
                }
                Event::Text(t) if has_key_focus => {
                    events.push(RawInputEventWire::Text { text: t.clone() })
                }
                // TextEdit이 조합 상태를 이어받도록 Commit뿐 아니라 네 종류의 IME 이벤트를 전달한다.
                Event::Ime(ime) if has_key_focus => {
                    events.push(RawInputEventWire::Ime {
                        event: match ime {
                            ImeEvent::Enabled => ImeWire::Enabled,
                            ImeEvent::Preedit(text) => ImeWire::Preedit { text: text.clone() },
                            ImeEvent::Commit(text) => ImeWire::Commit { text: text.clone() },
                            ImeEvent::Disabled => ImeWire::Disabled,
                        },
                    });
                }
                // 편집 이벤트는 창의 키 처리가 단축키 설정대로 바꿔 둔 것이다. 여기서는 다시 판정하지 않는다.
                Event::Paste(text) if has_key_focus && !text.is_empty() => {
                    events.push(RawInputEventWire::PasteText { text: text.clone() });
                }
                Event::Copy if has_key_focus => events.push(RawInputEventWire::Copy),
                Event::Cut if has_key_focus => events.push(RawInputEventWire::Cut),
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
            focused: has_key_focus,
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

/// 콘텐츠 영역을 비우고 주변을 네 띠로 칠한다. 상·하단의 바깥 모서리만 둥글게 그린다.
/// 모서리가 상·하단 띠 안에 들어가도록 content_margin() >= corner_radius를 전제로 한다.
fn paint_shell_background_excluding_content(
    painter: &egui::Painter,
    rect: Rect,
    content_rect: Rect,
    corner_radius: f32,
    bg_fill: egui::Color32,
) {
    let cr = corner_radius as u8;
    painter.rect_filled(
        Rect::from_min_max(rect.min, Pos2::new(rect.max.x, content_rect.min.y)),
        egui::CornerRadius {
            nw: cr,
            ne: cr,
            sw: 0,
            se: 0,
        },
        bg_fill,
    );
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(rect.min.x, content_rect.max.y), rect.max),
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: cr,
            se: cr,
        },
        bg_fill,
    );
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.min.x, content_rect.min.y),
            Pos2::new(content_rect.min.x, content_rect.max.y),
        ),
        0.0,
        bg_fill,
    );
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(content_rect.max.x, content_rect.min.y),
            Pos2::new(rect.max.x, content_rect.max.y),
        ),
        0.0,
        bg_fill,
    );
}

/// 팝업 범위별 배경과 중복 표시 여부를 호스트의 공통 함수로 구한다.
fn scrim_plan(
    placed: &[(MeshSnap, Rect)],
    layout: Option<&LayoutContext>,
    screen_rect: Rect,
) -> (Vec<Rect>, Vec<bool>) {
    let rects: Vec<Rect> = placed
        .iter()
        .map(|(snap, _)| PopupManager::scope_rect(&snap.scope, layout).unwrap_or(screen_rect))
        .collect();
    // scrim 겹침은 덮는 팝업끼리만 고른다. 덮지 않는 팝업은 scrim이 없다.
    let covering: Vec<usize> = (0..placed.len())
        .filter(|&i| placed[i].0.covers())
        .collect();
    let picked =
        PopupManager::pick_scrim_layers(&covering.iter().map(|&i| rects[i]).collect::<Vec<_>>());
    let mut paints = vec![false; placed.len()];
    for (i, p) in covering.into_iter().zip(picked) {
        paints[i] = p;
    }
    (rects, paints)
}

/// 범위가 보이지 않으면 None을 반환한다. 경계는 호스트 팝업과 같은 scope_bounds를 쓰며
/// surface 안쪽 여백은 spacing_sm으로 정한다.
fn place_popup(
    anchor: PopupAnchor,
    scope: &PopupScope,
    size: Vec2,
    layout: Option<&LayoutContext>,
    screen_rect: Rect,
    pointer_pos: Option<Pos2>,
) -> Option<Rect> {
    if !PopupManager::is_scope_visible(scope, layout) {
        return None;
    }
    let bounds = PopupManager::scope_bounds(
        scope,
        layout,
        screen_rect,
        crate::theme::theme().spacing_sm.value(),
    );
    // GPU에서 합성하는 콘텐츠는 egui layer clip만으로 잘리지 않으므로 크기도 경계 안으로 줄인다.
    let size = Vec2::new(size.x.min(bounds.width()), size.y.min(bounds.height()));
    let pos = clamp_to_bounds(anchor_pos(anchor, size, bounds, pointer_pos), size, bounds);
    Some(Rect::from_min_size(pos, size))
}

fn clamp_to_bounds(pos: Pos2, size: Vec2, bounds: Rect) -> Pos2 {
    egui::pos2(
        pos.x
            .clamp(bounds.min.x, (bounds.max.x - size.x).max(bounds.min.x)),
        pos.y
            .clamp(bounds.min.y, (bounds.max.y - size.y).max(bounds.min.y)),
    )
}

// 가운데 정렬은 화면이 아닌 소속 범위를 기준으로 한다.
fn anchor_pos(anchor: PopupAnchor, size: Vec2, bounds: Rect, pointer_pos: Option<Pos2>) -> Pos2 {
    let centered = egui::pos2(
        bounds.center().x - size.x / 2.0,
        bounds.center().y - size.y / 2.0,
    );
    match anchor {
        PopupAnchor::ScreenCenter => centered,
        PopupAnchor::Cursor => pointer_pos.unwrap_or(centered),
        // 대상을 다시 찾지 않고 바인딩된 scope의 중앙을 사용한다.
        PopupAnchor::ActiveSurfaceCenter => centered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(events: Vec<RawInputEventWire>) -> RawInputWire {
        RawInputWire {
            events,
            ..RawInputWire::default()
        }
    }

    #[test]
    fn only_a_press_is_a_user_activation() {
        use tasty_plugin_protocol::PointerButtonWire;
        let press = |pressed| RawInputEventWire::PointerButton {
            x: 1.0,
            y: 1.0,
            button: PointerButtonWire::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let key = |pressed| RawInputEventWire::Key {
            key: "Enter".into(),
            pressed,
            repeat: false,
            modifiers: Default::default(),
        };
        assert!(is_user_activation(&wire(vec![press(true)])));
        assert!(is_user_activation(&wire(vec![key(true)])));
        assert!(!is_user_activation(&wire(vec![press(false), key(false)])));
        assert!(!is_user_activation(&wire(vec![
            RawInputEventWire::PointerMoved { x: 1.0, y: 1.0 },
            RawInputEventWire::Scroll { x: 0.0, y: 3.0 },
            RawInputEventWire::PointerGone,
        ])));
        assert!(!is_user_activation(&wire(Vec::new())));
    }

    const SCREEN: Rect = Rect {
        min: Pos2::new(0.0, 0.0),
        max: Pos2::new(1600.0, 1000.0),
    };
    const SURFACE_7: Rect = Rect {
        min: Pos2::new(800.0, 500.0),
        max: Pos2::new(1600.0, 1000.0),
    };

    /// 라벨 버튼 프레임 134×38, 아이콘 버튼 프레임 38×38(정본 실측값).
    const FRAMES: (Vec2, Vec2) = (Vec2::new(134.0, 38.0), Vec2::new(38.0, 38.0));

    fn place(
        snaps: Vec<MeshSnap>,
        layout: &LayoutContext,
        pointer: Option<Pos2>,
        expanded: &[u64],
    ) -> Vec<(MeshSnap, Rect)> {
        let expanded: HashSet<u64> = expanded.iter().copied().collect();
        place_visible(snaps, Some(layout), SCREEN, pointer, &expanded, FRAMES)
    }

    fn layout_with_surface_7(visible: bool) -> LayoutContext {
        LayoutContext {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: Vec::new(),
            surface_rects: if visible {
                vec![(7, SURFACE_7)]
            } else {
                Vec::new()
            },
            active_tabs: Vec::new(),
            active_tab_ids: Vec::new(),
            workspace_rect: None,
            surface_owners: Vec::new(),
            tab_content_rects: Vec::new(),
        }
    }

    fn instance(
        scope: PopupScopeDecl,
        scope_surface: Option<u32>,
        anchor: PopupAnchor,
    ) -> tasty_host_plugin::PopupInstance {
        tasty_host_plugin::PopupInstance {
            plugin_id: "com.example.p".into(),
            popup_id: "file-open".into(),
            contribute: tasty_plugin_manifest::PopupContribute {
                id: "file-open".into(),
                trigger: tasty_plugin_manifest::PopupTrigger::Ipc,
                size_hint: Some(tasty_plugin_manifest::PopupSizeHint {
                    width: 400,
                    height: 200,
                }),
                anchor,
                scope,
                dismiss_on_outside_click: true,
                rendering: PopupRendering::EguiMesh,
                layout: Default::default(),
            },
            z_seq: 1,
            scope_surface,
        }
    }

    fn placed_rect(
        inst: &tasty_host_plugin::PopupInstance,
        layout: &LayoutContext,
        pointer: Option<Pos2>,
    ) -> Option<Rect> {
        let snaps = mesh_snapshots(std::iter::once((1, inst)));
        place(snaps, layout, pointer, &[])
            .into_iter()
            .next()
            .map(|(_, r)| r)
    }

    #[test]
    fn a_surface_scoped_popup_centers_on_its_surface() {
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let rect = placed_rect(&inst, &layout_with_surface_7(true), None)
            .expect("surface 가 보이면 그려져야 한다");
        assert_eq!(rect.center(), SURFACE_7.center());
    }

    #[test]
    fn a_surface_scoped_popup_is_not_placed_while_its_surface_is_hidden() {
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        assert_eq!(
            placed_rect(&inst, &layout_with_surface_7(false), None),
            None
        );
    }

    #[test]
    fn the_scrim_of_a_surface_scoped_plugin_popup_covers_its_surface_once() {
        let layout = layout_with_surface_7(true);
        let bound = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let placed = place(
            mesh_snapshots([(1, &bound), (2, &bound)].into_iter()),
            &layout,
            None,
            &[],
        );
        assert_eq!(
            placed.len(),
            2,
            "배경 중복 표시 검사를 위해 인스턴스 두 개가 필요하다"
        );
        let (rects, paints) = scrim_plan(&placed, Some(&layout), SCREEN);
        assert_eq!(rects, vec![SURFACE_7, SURFACE_7]);
        assert_eq!(
            paints,
            vec![true, false],
            "같은 범위의 배경을 중복해서 그리지 않는다"
        );
    }

    /// 덮지 않는다고 선언한 plugin 팝업은 scrim이 없다. 같은 범위의 덮는 팝업은 그대로 칠한다.
    #[test]
    fn a_plugin_popup_declared_not_to_cover_paints_no_scrim() {
        let layout = layout_with_surface_7(true);
        let covering = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let mut plain = covering.clone();
        plain.contribute.layout.covers_scope = false;
        let placed = place(
            mesh_snapshots([(1, &plain), (2, &covering)].into_iter()),
            &layout,
            None,
            &[],
        );
        assert!(!placed[0].0.covers());
        assert!(placed[1].0.covers());
        let (_, paints) = scrim_plan(&placed, Some(&layout), SCREEN);
        assert_eq!(paints, vec![false, true]);
    }

    /// Esc는 선언이 닫기일 때만 plugin 팝업을 닫는다.
    #[test]
    fn escape_closes_a_plugin_popup_only_when_declared_to_close() {
        use tasty_plugin_protocol::popup_decl::PopupEscape;
        let layout = layout_with_surface_7(true);
        for (escape, closes) in [
            (PopupEscape::Close, true),
            (PopupEscape::Unfocus, false),
            (PopupEscape::Ignore, false),
        ] {
            let mut inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
            inst.contribute.layout.escape = escape;
            let placed = place(
                mesh_snapshots(std::iter::once((1, &inst))),
                &layout,
                None,
                &[],
            );
            assert_eq!(placed[0].0.closes_on_escape(), closes, "{escape:?}");
        }
    }

    #[test]
    fn the_scrim_of_a_window_scoped_plugin_popup_stays_the_whole_screen() {
        let hidden = layout_with_surface_7(false);
        let inst = instance(PopupScopeDecl::Window, Some(7), PopupAnchor::ScreenCenter);
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &hidden,
            None,
            &[],
        );
        let (rects, paints) = scrim_plan(&placed, Some(&hidden), SCREEN);
        assert_eq!(rects, vec![SCREEN]);
        assert_eq!(paints, vec![true]);
    }

    /// 대상 없는 surface 선언 인스턴스는 창 범위로 바꿔 그리지 않는다.
    #[test]
    fn an_unbound_surface_popup_is_not_drawn_on_the_window() {
        let inst = instance(PopupScopeDecl::Surface, None, PopupAnchor::ScreenCenter);
        assert!(mesh_snapshots(std::iter::once((1, &inst))).is_empty());
        assert_eq!(placed_rect(&inst, &layout_with_surface_7(true), None), None);
    }

    #[test]
    fn a_surface_scoped_popup_is_clamped_inside_its_surface_inset() {
        let inset = crate::theme::theme().spacing_sm.value();
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::Cursor);
        let rect = placed_rect(
            &inst,
            &layout_with_surface_7(true),
            Some(Pos2::new(10.0, 10.0)),
        )
        .expect("surface 가 보이면 그려져야 한다");
        assert_eq!(rect.min, SURFACE_7.shrink(inset).min);
        assert!(SURFACE_7.contains_rect(rect));
    }

    fn layout_with_surface(id: u32, rect: Rect) -> LayoutContext {
        LayoutContext {
            surface_rects: vec![(id, rect)],
            ..layout_with_surface_7(false)
        }
    }

    /// Given surface 폭이 선언 크기(400×200)보다 좁을 때, Then 범위 안 보기 버튼 프레임으로 접히고 scrim 은
    /// 그 surface 를 덮는다.
    #[test]
    fn a_surface_narrower_than_the_declared_size_collapses_the_popup_in_it() {
        let narrow = Rect::from_min_size(Pos2::new(1400.0, 0.0), Vec2::new(200.0, 1000.0));
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let layout = layout_with_surface(7, narrow);
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout,
            None,
            &[],
        );
        assert_eq!(placed.len(), 1);
        let SnapDisplay::Collapsed(frame) = placed[0].0.display else {
            panic!("collapsed expected: {:?}", placed[0].0.display);
        };
        assert_eq!(frame.form, tasty_ui_widgets::CollapsedForm::Label);
        assert_eq!(placed[0].1, frame.rect);
        assert!(narrow.contains_rect(frame.rect));
        assert_eq!(frame.rect.center(), narrow.center());
        assert_eq!(placed[0].0.scope, PopupScope::Surface(7));
        let (rects, _) = scrim_plan(&placed, Some(&layout), SCREEN);
        assert_eq!(rects, vec![narrow], "scrim 은 논리 범위를 덮는다");
    }

    /// Given 줄이는(shrink) 선언, Then 좁은 surface에서도 접지 않는다(host와 같은 판정 함수).
    #[test]
    fn a_plugin_popup_declared_to_shrink_does_not_collapse() {
        let narrow = Rect::from_min_size(Pos2::new(1400.0, 0.0), Vec2::new(200.0, 1000.0));
        let mut inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        inst.contribute.layout.fit = tasty_plugin_protocol::popup_decl::PopupFit::Shrink;
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout_with_surface(7, narrow),
            None,
            &[],
        );
        assert_eq!(placed[0].0.display, SnapDisplay::Fits);
    }

    /// Given 포인터 기준(Cursor) 팝업이 접힐 때, Then 보기 버튼 프레임은 포인터 자리에 둔다(정본 collapsed
    /// popup place).
    #[test]
    fn a_collapsed_cursor_popup_puts_its_frame_at_the_pointer() {
        let narrow = Rect::from_min_size(Pos2::new(1400.0, 0.0), Vec2::new(200.0, 1000.0));
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::Cursor);
        let pointer = Pos2::new(1500.0, 200.0);
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout_with_surface(7, narrow),
            Some(pointer),
            &[],
        );
        let SnapDisplay::Collapsed(frame) = placed[0].0.display else {
            panic!("collapsed expected: {:?}", placed[0].0.display);
        };
        assert_eq!(frame.rect.center(), pointer);
    }

    /// Given surface 높이만 모자랄 때, Then 접힌다(한 축이면 충분하다).
    #[test]
    fn a_surface_lower_than_the_declared_size_collapses_the_popup() {
        let low = Rect::from_min_size(Pos2::new(0.0, 900.0), Vec2::new(1600.0, 100.0));
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout_with_surface(7, low),
            None,
            &[],
        );
        assert!(matches!(placed[0].0.display, SnapDisplay::Collapsed(_)));
    }

    /// Given 테두리 inset 을 뺀 안쪽이 선언 크기보다 작을 때, Then 범위 밖으로 넘치지 않고 접힌다.
    #[test]
    fn a_surface_that_fits_only_with_no_inset_still_collapses() {
        let inset = crate::theme::theme().spacing_sm.value();
        let exact = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(400.0 + inset, 600.0));
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout_with_surface(7, exact),
            None,
            &[],
        );
        assert!(matches!(placed[0].0.display, SnapDisplay::Collapsed(_)));
    }

    /// Given 펼친 팝업, Then 배치 범위가 창이고 host 제목줄만큼 커진 상자를 창 가운데에 두며 scrim 은 창을
    /// 덮는다. 논리 범위는 surface 그대로다. 범위가 넓어져도 펼친 채다.
    #[test]
    fn an_expanded_popup_opens_over_the_window_with_a_host_bar() {
        let narrow = Rect::from_min_size(Pos2::new(1400.0, 0.0), Vec2::new(200.0, 1000.0));
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let bar = popup::title_bar_height().value();
        for layout in [layout_with_surface(7, narrow), layout_with_surface_7(true)] {
            let placed = place(
                mesh_snapshots(std::iter::once((1, &inst))),
                &layout,
                None,
                &[1],
            );
            assert_eq!(placed[0].0.display, SnapDisplay::Expanded);
            assert_eq!(placed[0].0.scope, PopupScope::Window);
            assert_eq!(placed[0].0.logical_scope, PopupScope::Surface(7));
            assert_eq!(placed[0].1.size(), Vec2::new(400.0, 200.0 + bar));
            assert_eq!(placed[0].1.center(), SCREEN.center());
            let (rects, _) = scrim_plan(&placed, Some(&layout), SCREEN);
            assert_eq!(rects, vec![SCREEN]);
        }
    }

    /// Given 창보다 큰 펼친 팝업, Then 크기를 유지하고 왼쪽·위에 맞춘다.
    #[test]
    fn an_expanded_popup_larger_than_the_window_keeps_its_size_at_the_top_left() {
        let mut inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        inst.contribute.size_hint = Some(tasty_plugin_manifest::PopupSizeHint {
            width: 2000,
            height: 1200,
        });
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout_with_surface_7(true),
            None,
            &[1],
        );
        assert_eq!(placed[0].1.min, SCREEN.min);
        assert_eq!(placed[0].1.width(), 2000.0);
    }

    /// Given 펼친 팝업의 논리 범위가 숨으면, Then 펼친 팝업도 그리지 않는다.
    #[test]
    fn an_expanded_popup_hides_with_its_logical_scope() {
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let placed = place(
            mesh_snapshots(std::iter::once((1, &inst))),
            &layout_with_surface_7(false),
            None,
            &[1],
        );
        assert!(placed.is_empty());
    }

    #[test]
    fn window_scope_keeps_the_screen() {
        let hidden = layout_with_surface_7(false);
        let inst = instance(PopupScopeDecl::Window, Some(7), PopupAnchor::ScreenCenter);
        let rect = placed_rect(&inst, &hidden, None).expect("창 범위는 항상 그려진다");
        assert_eq!(rect.center(), SCREEN.center());
    }

    fn collected_scroll(unit: egui::MouseWheelUnit, delta: Vec2) -> Option<(f32, f32)> {
        collected_scroll_with_notch(unit, delta, tasty_settings::DEFAULT_WHEEL_LINE_SCROLL)
    }

    fn collected_scroll_with_notch(
        unit: egui::MouseWheelUnit,
        delta: Vec2,
        notch: f32,
    ) -> Option<(f32, f32)> {
        let ctx = Context::default();
        // 호스트의 노치 거리를 지정하지 않으면 egui 기본값 40이 남는다.
        ctx.options_mut(|o| o.line_scroll_speed = notch);
        let content_rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(300.0, 200.0));
        let pointer = Pos2::new(150.0, 150.0);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(1280.0, PAGE_HEIGHT),
            )),
            events: vec![Event::MouseWheel {
                unit,
                delta,
                modifiers: egui::Modifiers::default(),
            }],
            ..Default::default()
        };
        let mut wire = None;
        // 실제 입력 처리를 실행하되 렌더 결과는 사용하지 않는다.
        let _full_output = ctx.run(input, |ctx| {
            wire = Some(collect_mesh_popup_input(
                ctx,
                content_rect,
                Some(pointer),
                false,
                true,
                &mut Default::default(),
            ));
        });
        wire?.events.iter().find_map(|e| match e {
            RawInputEventWire::Scroll { x, y } => Some((*x, *y)),
            _ => None,
        })
    }

    // Page 단위를 환산할 화면 높이.
    const PAGE_HEIGHT: f32 = 800.0;

    #[test]
    fn a_wheel_notch_reaches_the_wire_as_the_shared_line_scroll_distance() {
        let (dx, dy) = collected_scroll(egui::MouseWheelUnit::Line, Vec2::new(0.0, -1.0))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(dx, 0.0);
        assert_eq!(dy, -tasty_settings::DEFAULT_WHEEL_LINE_SCROLL);
        assert_ne!(dy, -1.0);
    }

    #[test]
    fn the_wire_distance_follows_the_host_option() {
        let (_, slow) =
            collected_scroll_with_notch(egui::MouseWheelUnit::Line, Vec2::new(0.0, -1.0), 20.0)
                .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        let (_, fast) =
            collected_scroll_with_notch(egui::MouseWheelUnit::Line, Vec2::new(0.0, -1.0), 120.0)
                .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(slow, -20.0);
        assert_eq!(fast, -120.0);
    }

    #[test]
    fn a_trackpad_delta_reaches_the_wire_unchanged() {
        let (dx, dy) = collected_scroll(egui::MouseWheelUnit::Point, Vec2::new(2.0, -17.5))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!((dx, dy), (2.0, -17.5));
    }

    #[test]
    fn a_page_delta_reaches_the_wire_scaled_by_the_screen_height() {
        let (_, dy) = collected_scroll(egui::MouseWheelUnit::Page, Vec2::new(0.0, -1.0))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(dy, -PAGE_HEIGHT);
    }

    fn collected_events(events: Vec<Event>, has_key_focus: bool) -> Vec<RawInputEventWire> {
        let ctx = Context::default();
        let content_rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(300.0, 200.0));
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(1280.0, PAGE_HEIGHT),
            )),
            events,
            ..Default::default()
        };
        let mut wire = None;
        let _full_output = ctx.run(input, |ctx| {
            wire = Some(collect_mesh_popup_input(
                ctx,
                content_rect,
                None,
                false,
                has_key_focus,
                &mut Default::default(),
            ));
        });
        wire.expect("수집 함수가 와이어를 만들어야 한다").events
    }

    fn collected_ime(events: Vec<Event>, has_key_focus: bool) -> Vec<ImeWire> {
        collected_events(events, has_key_focus)
            .into_iter()
            .filter_map(|e| match e {
                RawInputEventWire::Ime { event } => Some(event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn all_four_ime_stages_reach_the_wire() {
        let got = collected_ime(
            vec![
                Event::Ime(ImeEvent::Enabled),
                Event::Ime(ImeEvent::Preedit("ㅎ".into())),
                Event::Ime(ImeEvent::Commit("한".into())),
                Event::Ime(ImeEvent::Disabled),
            ],
            true,
        );
        assert_eq!(
            got,
            vec![
                ImeWire::Enabled,
                ImeWire::Preedit { text: "ㅎ".into() },
                ImeWire::Commit { text: "한".into() },
                ImeWire::Disabled,
            ]
        );
    }

    #[test]
    fn ime_is_dropped_without_key_focus() {
        let got = collected_ime(
            vec![
                Event::Ime(ImeEvent::Enabled),
                Event::Ime(ImeEvent::Preedit("ㅎ".into())),
                Event::Ime(ImeEvent::Commit("한".into())),
                Event::Ime(ImeEvent::Disabled),
            ],
            false,
        );
        assert!(
            got.is_empty(),
            "포커스 없는 팝업에 IME 입력이 전달됐다: {got:?}"
        );
    }

    /// 보류한 편집 키의 요청은 그 팝업 입력에만 넣은 순서대로 가고, 다른 팝업의 요청은 남는다.
    #[test]
    fn edit_requests_reach_only_their_popup_in_order() {
        use tasty_plugin_protocol::EditRequestWire;
        let req = |edit_id, edit| RawInputEventWire::EditRequest { edit_id, edit };
        let mut queued = vec![
            (1, req(1, EditRequestWire::Copy)),
            (2, req(2, EditRequestWire::Cut)),
            (1, req(3, EditRequestWire::Cut)),
        ];
        assert_eq!(
            take_edit_requests(&mut queued, 1),
            vec![req(1, EditRequestWire::Copy), req(3, EditRequestWire::Cut)]
        );
        assert_eq!(queued, vec![(2, req(2, EditRequestWire::Cut))]);
    }

    /// 붙여넣기·복사·잘라내기는 키를 받는 팝업에만 그 순서대로 가고, 빈 붙여넣기는 버린다.
    #[test]
    fn edit_events_reach_only_the_key_focused_popup() {
        let edits = || {
            vec![
                Event::Paste("a".into()),
                Event::Paste(String::new()),
                Event::Copy,
                Event::Cut,
            ]
        };
        assert_eq!(
            collected_events(edits(), true),
            vec![
                RawInputEventWire::PasteText { text: "a".into() },
                RawInputEventWire::Copy,
                RawInputEventWire::Cut,
            ]
        );
        assert!(collected_events(edits(), false).is_empty());
    }

    /// 펼친 키 주인 팝업의 Esc 는 선언이 닫기여도 다시 접기이고, 그 Esc 는 mesh 내용에 가지 않는다.
    /// markdown 파일 열기처럼 내용이 Esc 를 취소로 받는 팝업이 닫히지 않는다.
    #[test]
    fn escape_on_an_expanded_popup_refolds_and_never_reaches_its_content() {
        assert_eq!(escape_action(true, true, true), EscapeAction::Refold);
        assert_eq!(escape_action(true, true, false), EscapeAction::Refold);
        assert_eq!(escape_action(true, false, true), EscapeAction::Close);
        assert_eq!(escape_action(true, false, false), EscapeAction::None);
        assert_eq!(escape_action(false, true, true), EscapeAction::None);

        let esc = |pressed| Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut raw = wire(collected_events(
            vec![esc(true), Event::Text("a".into()), esc(false)],
            true,
        ));
        assert!(
            raw.events
                .iter()
                .any(|e| matches!(e, RawInputEventWire::Key { .. })),
            "키 주인 팝업은 빼기 전 Esc 를 수집한다"
        );
        drop_escape_keys(&mut raw);
        assert!(
            raw.events
                .iter()
                .all(|e| !matches!(e, RawInputEventWire::Key { .. })),
            "{:?}",
            raw.events
        );
        assert!(
            raw.events
                .iter()
                .any(|e| matches!(e, RawInputEventWire::Text { text } if text == "a"))
        );
    }

    #[test]
    fn plain_text_still_reaches_the_wire() {
        let got = collected_events(vec![Event::Text("a".into())], true);
        assert!(
            got.contains(&RawInputEventWire::Text { text: "a".into() }),
            "영문 텍스트가 전달되지 않았다: {got:?}"
        );
    }
}

// 플러그인 프로세스 없이 렌더 루프의 사용자 활성화 기록과 정리를 검사한다.
#[cfg(test)]
#[path = "popup_render_activation_tests.rs"]
mod activation_tests;
