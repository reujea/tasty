//! Plugin popup 인스턴스 렌더링.
//!
//! `PluginManager::popup_instances`에 등록된 popup을 매 프레임 그린다.
//! **egui-mesh popup (A2)**: plugin 이 자기 프로세스에서 egui mesh 를 tessellate 하고,
//! host 는 셸(scrim/bg/border/outside-click/Esc)만 그린 뒤 콘텐츠 영역에 plugin mesh 를
//! 합성한다(합성은 `gpu.render` 가 host egui pass 후 수행 — `egui_mesh_prepare`).
//!
//! 호스트 본문 popup(`PopupManager`)과는 별도 경로 — plugin popup은 동적 instance_id를
//! 가지고 `&'static str` 기반 `PopupId`/`PopupDef` 모델에 맞지 않기 때문.
//!
//! 사용자 입력은 `popup.set_context` 의 raw_input 으로 모은 뒤 plugin 에 forward 한다.
//! set_context 송신 자체는 host 렌더 파이프라인의 일부라 사용자 상태에 부수효과가
//! 없다(identity 원칙 1·3).

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
use crate::adapters::ui::popup::occlusion::{Occluder, PointOwnership, point_ownership};
use crate::adapters::ui::popup::{self, PopupManager, PopupScope};
use crate::model::LogicalPx;
use crate::plugin::PluginManager;
use crate::plugin::manifest::PopupAnchor;
use crate::plugin_bridge::wire_scroll;
use crate::state::AppState;

const DEFAULT_POPUP_SIZE: Vec2 = Vec2::new(360.0, 200.0);

/// 매 egui 프레임 호출. plugin popup_instances를 순회하면서:
///  - egui-mesh 인스턴스는 셸을 그리고 `popup.set_context` 를 forward, 합성 영역을
///    `state.plugin_mesh_popup_regions` 에 적재(실 합성은 `gpu.render`),
///  - 외부 클릭/Escape를 감지해 `state.plugin_popup_closes`에 적재.
///
/// 각 인스턴스는 선언한 소속 범위([`popup_scope`])의 가시성·경계를 host popup 과 **같은
/// 판정 함수**(`PopupManager::is_scope_visible`/`scope_rect`)로 따른다. 범위가 안 보이는
/// 인스턴스는 이 frame 에 **존재하지 않는 것**으로 다룬다 — 셸·합성 영역·히트테스트 rect·
/// Esc/outside-click·키 게이트 어디에도 안 들어간다. 안 빠지면 보이지도 않는 rect 가 클릭을
/// 삼킨다. 인스턴스 자신과 forward 추적은 그대로 두어 범위가 다시 보이면 상태째 복원된다.
///
/// `layout` 은 host popup 이 같은 frame 에 쓴 것과 같은 값이다(`ui::draw_popups` 가 돌려준다).
///
/// `mgr`이 `None`이거나 popup_instances가 비어있으면 mesh 영역만 비우고 반환.
pub fn draw_plugin_popups(
    ctx: &Context,
    state: &mut AppState,
    _engine: &mut crate::core::CoreState,
    plugin_manager: Option<&PluginManager>,
    layout: Option<&LayoutContext>,
) {
    // 매 frame mesh 합성 영역/셸 레이어 목록을 새로 수집한다 — 이전 frame 잔재가
    // 합성되거나 `enforce_host_plugin_popup_z_order`(egui_bridge.rs)에 남지 않게.
    state.plugin_mesh_popup_regions.clear();
    state.plugin_popup_layers.clear();
    // 키/IME 게이트가 읽는 캐시(`AppState.plugin_popup_open`)도 여기서 리셋한다 —
    // 아래 두 조기 반환(plugin manager 부재 / mesh popup 없음)이 모두 "popup 없음" 을
    // 뜻하므로 리셋을 이 최상단에 두어야 둘 다 덮인다. stale `true` 가 남으면 키보드가
    // 영영 터미널로 가지 못한다.
    //
    // **open drain 시점에 즉시 세우지 않는 이유**: popup instance 를 실제로 여는 곳은
    // App 메인 루프의 `pending_popup_opens` drain 인데, 그 직후 같은 tick 에서 렌더
    // 프레임이 돌아 이 함수가 캐시를 채운다. 반면 popup 이 *닫히는* 경로는 여러 개라
    // (Esc/outside-click/plugin 요청/host 강제) drain 지점에 set 만 추가하면 리셋
    // 책임이 두 곳으로 갈라진다. 단일 갱신 지점을 유지하고 최대 1 프레임 지연을
    // 받아들인다 — popup 이 열린 프레임에 사용자가 이미 키를 누르고 있을 수는 없다.
    state.plugin_popup_open = false;
    // 히트테스트 rect 도 매 frame 새로 채운다 — 아래 두 조기 반환(plugin manager 부재 /
    // mesh popup 없음) 경로에서도 반드시 비워져야 한다. 남겨두면 이미 닫힌 plugin popup
    // 의 rect 가 host popup 의 outside-click 을 영구히 삼킨다.
    state.plugin_popup_hittest.clear();
    // IME 후보창 위치 캐시도 매 frame 새로 채운다 — 아래 두 조기 반환(plugin manager 부재 /
    // mesh popup 없음)도 이 리셋이 덮는다. stale 값이 남으면 popup 을 닫은 뒤에도 터미널
    // 조합의 후보창이 닫힌 popup 자리에 뜬다.
    state.plugin_popup_ime_cursor_area = None;

    let Some(mgr) = plugin_manager else {
        state.plugin_mesh_popup_forward.clear();
        state.plugin_popup_user_activated.clear();
        return;
    };

    // popup_instances를 즉시 owned snapshot으로 복사. 이후 loop에서 `state`를 mutable로
    // borrow하기 위함.
    let mesh_snaps = mesh_snapshots(mgr.popup_instances());

    // 닫힌 mesh popup 의 forward 추적 정리 — 한 맵이라 칸별로 빠뜨릴 자리가 없다.
    let live_mesh: HashSet<u64> = mesh_snaps.iter().map(|s| s.instance_id).collect();
    state
        .plugin_mesh_popup_forward
        .retain(|k, _| live_mesh.contains(k));
    // 활성화 기록은 여기서, popup 이 닫힐 때만 걷힌다 — `dispatch_origin_of` 는 조회만 하고
    // 소비하지 않으므로 사용자가 한 번 누른 popup 이 열려 있는 동안 그 plugin 은 몇 번이든
    // 사용자 발화를 얻는다. plugin 코드는 이미 사용자 입력을 받는 쪽이라 이 창은 ADR-0526 이
    // 수용했다.
    state
        .plugin_popup_user_activated
        .retain(|k, _| live_mesh.contains(k));

    let screen_rect = ctx.screen_rect();
    let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
    let primary_pressed = ctx.input(|i| i.pointer.primary_pressed());
    let escape_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    // 히트테스트를 하려면 자기 rect 만으로는 부족하다 — "이 좌표를 나보다 위 popup 이
    // 덮는가"를 물어야 하므로 형제 plugin popup 과 host popup 의 rect 가 함께 필요하다.
    // 그래서 셸 rect 를 먼저 전부 확정한 뒤 본 루프를 돈다.
    let placed = place_visible(mesh_snaps, layout, screen_rect, pointer_pos);

    if placed.is_empty() {
        return;
    }

    // 이번 frame 의 occluder 집합. z_seq 는 host/plugin 공용 전역 카운터라
    // (`tasty_host_plugin::next_popup_z_seq`) 두 종류를 한 배열에서 비교할 수 있다.
    //
    // host popup rect 는 같은 frame 의 `ui::draw_popups` 가 이미 채웠으므로 **최신**이다
    // (프레임 순서는 `gfx/gpu/egui_bridge.rs` 참고). 반대 방향(host 가 보는 plugin rect)만
    // 1 frame 뒤처진다.
    let mut occluders: Vec<Occluder> = state
        .host_popup_hittest
        .iter()
        .map(|h| Occluder {
            rect: h.rect,
            z_seq: h.z_seq,
        })
        .collect();
    occluders.extend(placed.iter().map(|(s, r)| Occluder {
        rect: *r,
        z_seq: s.z_seq,
    }));
    // 자기 자신도 배열에 들어있지만 `point_ownership` 은 **엄격히 큰** z_seq 만 가림으로
    // 보므로 자기 rect 가 자기를 가리는 일은 없다. 형제 plugin popup 도 같은 배열에
    // 들어가므로 host↔plugin 뿐 아니라 plugin↔plugin 겹침도 같은 판정으로 덮인다.
    //
    // **`host_popup_on_top` 2그룹 비교의 제약을 물려받지 않는다**: 이 판정은 popup 쌍마다
    // z_seq 를 직접 비교하므로 개별 상하 관계를 정확히 표현한다. 반면 셸 렌더 순서는
    // `gfx/gpu.rs` 의 `host_popup_should_render_on_top(host 최댓값, plugin 최댓값)` 2그룹
    // 비교라 popup 이 3개 이상 섞여 z 가 교차하면 "그려진 순서" 와 "포인터를 가져가는
    // 순서" 가 어긋날 수 있다(`docs/design/systems/popup.md` §Host ↔ Plugin popup z-order
    // "범위"). 현재는 동시에 열리는 egui-mesh popup 이 최대 1개라 교차가 생기지 않는다 —
    // 셸 순서 쪽이 쌍별 비교로 확장되면 이 주석도 함께 걷어낸다.

    // 다음 frame 의 host popup 판정용으로 이번 frame plugin 셸 rect 를 남긴다.
    state
        .plugin_popup_hittest
        .extend(placed.iter().map(|(s, r)| Occluder {
            rect: *r,
            z_seq: s.z_seq,
        }));

    // 열린 mesh popup 이 있다 — 키/IME 를 egui 로 들여보내는 게이트를 연다.
    state.plugin_popup_open = true;

    // Esc 소유권 — 규칙 7 의 키보드 판("최상단 하나만 받는다"). host/plugin 통틀어
    // 이번 프레임 최상단인 popup 하나만 Esc 를 소비한다(ADR-0084). host 쪽 대응은
    // `adapters/ui/popup/frame.rs` 가 `AppState.popup_escape_owner` 로 정한다.
    let top_z = occluders.iter().map(|o| o.z_seq).max();

    let mut any_hovered = false;

    let (scrim_rects, scrim_paints) = scrim_plan(&placed, layout, screen_rect);

    // ── egui-mesh popups (A2) ──
    let ppp = ctx.pixels_per_point().max(f32::EPSILON);
    // 현재 resolved Theme 스냅샷을 1회 만든다(popup 무관). plugin 이 host 와 동일 Theme 으로
    // 재구성하도록 색 집합+is_light+UI zoom 을 운반한다(surface forward 와 동형, ADR-0028 parity).
    let current_theme = {
        let th = crate::theme::theme();
        ThemeWire {
            colors: th.to_colors(),
            is_light: th.is_light,
            ui_zoom: _engine.settings.appearance.ui_scale_factor(),
        }
    };
    for (idx, (snap, rect)) in placed.iter().enumerate() {
        let snap = snap;
        let rect = *rect;
        let scope_rect = scrim_rects[idx];
        // 포인터 좌표의 소유권 — 규칙 7("겹친 영역의 마우스 이벤트는 최상단 팝업만
        // 받는다", `docs/design/systems/popup.md`)을 3-상태로 판정한다.
        let ownership = pointer_pos.map(|p| point_ownership(rect, snap.z_seq, &occluders, p));

        if ownership == Some(PointOwnership::Mine) {
            any_hovered = true;
        }

        let content_rect = rect.shrink(popup::content_margin().value());

        // 셸(chrome)은 host 소유: scrim → bg_panel(content_rect 는 hole) → border. 내용은
        // plugin mesh 가 content_rect 에 합성된다(gpu.render → egui_mesh_prepare). host popup
        // 이 이 popup 보다 위여야 하는 프레임(z_seq 역전)에는 콘텐츠 합성이 host egui pass
        // *전*에 실행되므로, bg_panel 이 content_rect 까지 채우면 그 뒤(같은 pass 안, 이
        // 셸과 함께 그려지는) host popup 유무와 무관하게 방금 합성한 콘텐츠를 덮어버린다.
        // content_rect 를 비워 두면(hole) 어느 순서로 합성되든 셸이 콘텐츠를 가리지 않는다
        // (`gfx/gpu.rs` 의 `render_egui_pass`/`render_egui_mesh_popups` 순서 분기 참고).
        let layer_id = egui::LayerId::new(
            Order::Foreground,
            Id::new("plugin_mesh_popup").with(snap.instance_id),
        );
        state.plugin_popup_layers.push(layer_id);
        // 이 popup 의 layer 는 자기 scope 로 클립된다. 셸이 칸보다 크면(좁은 surface)
        // 배치 clamp 는 좌상단만 붙잡아 우하단이 이웃 칸으로 넘쳐 나간다 — 어둡게 한
        // 자리 밖에 셸이 걸치면 "이 popup 은 이 칸의 것" 이라는 말이 깨진다.
        let painter = ctx.layer_painter(layer_id).with_clip_rect(scope_rect);
        let th = crate::theme::theme();
        // scrim 은 이 popup 이 묶인 scope 의 rect 를 덮는다 — 창 범위면 화면 전체,
        // surface 범위면 그 칸 하나(보더 포함, 인접 칸 제외). 같은 scope 에 popup 이
        // 여럿이면 위에서 고른 하나만 깐다.
        if scrim_paints[idx] {
            painter.rect_filled(scope_rect, 0.0, th.scrim().to_egui());
        }
        // plugin popup 은 예외 없이 scrim 을 깔고 뷰포트를 점유한다 = SCOPE RULE 의
        // modal 갈래(ADR-0254). scrim 은 바닥을 어둡게 할 뿐 엣지를 안 그려서, 이 단차가
        // 없으면 어두운 테마에서 셸 실루엣이 어두워진 바닥에 묻힌다. 배경보다 먼저 —
        // 그림자는 셸 아래에 깔린다.
        painter.add(
            th.shadow_modal()
                .to_egui()
                .as_shape(rect, th.corner_radius.value()),
        );
        paint_shell_background_excluding_content(
            &painter,
            rect,
            content_rect,
            th.corner_radius.value(),
            th.bg_panel().to_egui(),
        );
        painter.rect_stroke(
            rect,
            th.corner_radius.value(),
            Stroke::new(th.border_width.value(), th.border_default().to_egui()),
            egui::StrokeKind::Outside,
        );

        // 키보드는 최상단 popup 하나만 갖는다(규칙 7, ADR-0084). 이 게이트가 없으면
        // 아래 깔린 popup 도 Esc·문자를 받아 자기 UI 로 처리한다 — 실제로 plugin 이
        // 자체 Esc 처리로 스스로 닫아서, host 쪽 Esc 중재만으로는 "한 번의 Esc 로
        // 스택 전체가 닫히는" 현상을 못 막는다.
        let has_key_focus = Some(snap.z_seq) == top_z;
        // 상위 popup 에 가려진 좌표의 포인터 이벤트도 forward 하지 않는다.
        let raw_input = collect_mesh_popup_input(
            ctx,
            content_rect,
            pointer_pos,
            ownership == Some(PointOwnership::OccludedByHigher),
            has_key_focus,
        );

        // set_context forward — geom 변경 / 입력 / bootstrap(미paint) 일 때만 (surface 와 동형).
        // bootstrap 은 1회만: paint frame 이 도착하기 전 매 frame 스팸하면 plugin 이 여러 번
        // paint 하고(첫 frame 의 폰트 atlas delta 가 후속 frame 엔 없음) host 가 최신 frame 만
        // 보관해 atlas 를 못 받는다("Missing texture Managed(0)"). 1회 보내고 frame 을 기다린다.
        let physical = crate::plugin_bridge::mesh_region_of(content_rect, ppp);
        let w_px = physical.width.value().round().max(1.0) as u32;
        let h_px = physical.height.value().round().max(1.0) as u32;
        let geom = (w_px, h_px, ppp.to_bits());
        let has_input = !raw_input.events.is_empty();
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
        let need_bootstrap = fwd.need_bootstrap(has_frame);
        // 건강 상태 반영 + 빈 화면 워치독 — crash 로 frame 이 사라지면 재bootstrap 하도록
        // 무장 해제하고, bootstrap 후 유예를 넘기도록 frame 이 없으면 1회 경고한다
        // (판정도 경고문도 `MeshForwardCommon` 한 곳, surface·banner 와 같다).
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
        // 렌더 prepare 의 textures_delta 체인 단절 감지 — full 재전송 요청을 소비해
        // need_full_textures 를 실어 보낸다(다른 트리거가 없어도 송신).
        let need_full = fwd.take_pending_full();
        // (ADR-0056) 비동기 host→plugin push(예: 원격 git 조회 결과) 도착 후 강제
        // repaint — geom/input/theme 변경 없이도 plugin 이 새 내부 상태로 다시
        // 그리도록 이번 frame 에 set_context 를 보낸다.
        //
        // 이 칸에는 plugin 의 무입력 self-repaint 요청(`PopupInvalidated` →
        // `App::mark_invalidated_popups_dirty`)도 **편승한다** — 요구하는 것이 같은
        // "무입력 재forward" 라 별도 칸을 만들지 않았다. banner 는 편승분만 갖는다
        // (위 ADR-0056 경로는 git-viewer 전용이고 그 plugin 은 banner 를 안 낸다).
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
            mgr.send_popup_set_context(
                &snap.plugin_id,
                &PopupSetContextParams {
                    instance_id: snap.instance_id,
                    width_px: w_px,
                    height_px: h_px,
                    pixels_per_point: ppp,
                    raw_input,
                    theme: Some(current_theme.clone()),
                    need_full_textures: need_full,
                },
            );
        }

        // 합성 영역(물리 px) 적재 — gpu.render 가 이 popup 의 z_seq 와 현재 열린 host popup
        // 최대 z_seq 를 비교해 host egui pass 전/후 중 알맞은 시점에 mesh 를 합성한다.
        state
            .plugin_mesh_popup_regions
            .push((snap.instance_id, physical));

        // OS IME 후보창 위치 — plugin 프로세스의 egui 가 알려온 커서 영역(콘텐츠 로컬)을
        // 창 좌표로 올려 캐시한다. 키 포커스를 가진 popup 만 — 조합을 받는 것이 그 하나뿐이라
        // (`collect_mesh_popup_input` 의 `has_key_focus` 게이트) 후보창도 그 하나를 따라야 한다.
        if has_key_focus
            && let Some(ime) = mgr
                .popup_mesh_frame(snap.instance_id)
                .and_then(|f| f.ime_cursor.as_ref())
        {
            state.plugin_popup_ime_cursor_area = Some(crate::plugin_bridge::mesh_ime_cursor_area(
                physical, ime, ppp,
            ));
        }

        // popup 내부 클릭 시 z-order 승격(규칙 7 "클릭된 것이 앞") — host popup 의
        // `bring_to_front`(click-to-front)와 같은 규칙이다. `mgr` 이 `&PluginManager` 불변
        // 참조라 여기서 직접 갱신할 수 없어 큐에 적재하고 App 메인 루프가 drain한다.
        if primary_pressed && ownership == Some(PointOwnership::Mine) {
            state.plugin_popup_focus_bumps.push(snap.instance_id);
        }

        // outside-click dismiss 는 "모든 popup 바깥" 일 때만. 상위 popup 안을 클릭한
        // 것은 이 popup 의 바깥이긴 해도 "바깥 클릭" 이 아니다 — 그 클릭은 상위 popup
        // 의 것이다.
        // 자식 host popup 이 열려 있는 동안에는 부모가 바깥 클릭으로 닫히지 않는다
        // (스택 유지, ADR-0084) — 부모가 먼저 사라지면 자식이 고아가 되고 그 결과가
        // 조용히 버려진다. popup 은 모달이 아니므로 "부모를 잠그는" 것이 아니라
        // dismiss 대상에서만 빼는 최소 개입이다.
        let has_open_child = state.plugin_popup_has_open_child(snap.instance_id);

        if snap.dismiss_on_outside_click
            && !has_open_child
            && primary_pressed
            && ownership == Some(PointOwnership::OutsideAll)
        {
            state
                .plugin_popup_closes
                .push((snap.instance_id, PopupCloseReason::OutsideClick));
        }
        if escape_pressed && has_key_focus {
            state
                .plugin_popup_closes
                .push((snap.instance_id, PopupCloseReason::Escape));
        }
    }

    if any_hovered {
        state.popup_hovered = true;
    }
}

/// 한 frame 동안 쓰는 popup 인스턴스의 owned 사본.
struct MeshSnap {
    instance_id: u64,
    plugin_id: String,
    anchor: PopupAnchor,
    scope: PopupScope,
    size: Vec2,
    dismiss_on_outside_click: bool,
    z_seq: u64,
}

/// 열린 인스턴스 전부의 사본을 z_seq 오름차순으로 만든다. 범위 가시성은 여기서 거르지
/// 않는다 — forward 추적 정리가 "살아 있는 인스턴스" 전체를 봐야 범위가 다시 보일 때
/// 상태째 복원된다.
fn mesh_snapshots<'a>(
    instances: impl Iterator<Item = (u64, &'a tasty_host_plugin::PopupInstance)>,
) -> Vec<MeshSnap> {
    let mut mesh_snaps: Vec<MeshSnap> = Vec::new();
    for (id, inst) in instances {
        let size = inst
            .contribute
            .size_hint
            .map(|s| Vec2::new(s.width as f32, s.height as f32))
            .unwrap_or(DEFAULT_POPUP_SIZE);
        match inst.contribute.rendering {
            PopupRendering::EguiMesh => mesh_snaps.push(MeshSnap {
                instance_id: id,
                plugin_id: inst.plugin_id.clone(),
                anchor: inst.contribute.anchor,
                scope: popup_scope(inst.contribute.scope, inst.scope_surface),
                size,
                dismiss_on_outside_click: inst.contribute.dismiss_on_outside_click,
                z_seq: inst.z_seq,
            }),
        }
    }
    // `popup_instances` 는 HashMap 이라 순회 순서가 비결정적 — z_seq 오름차순으로 정렬해야
    // `plugin_mesh_popup_regions`(GPU 콘텐츠 합성 순서, 뒤에 push된 것이 위)에서 여러
    // plugin popup 이 동시에 열려 있을 때도 나중에 열리거나 클릭된 것이 콘텐츠 상 위에
    // 온다. 단, 이 정렬은 **셸(scrim/bg/border) 순서에는 영향이 없다** — 셸은
    // `ctx.layer_painter`로 직접 그리는 raw layer 라 `egui::Area`(`Areas::order`)를 거치지
    // 않으므로, 프레임 내 그리기 호출 순서가 최종 페인트 순서를 결정하지 않는다(egui
    // `GraphicLayers::drain` 소스 — order 밖 레이어는 별도 맵 순회로 덧붙여짐, 순서 보장
    // 없음). 여러 plugin popup 이 동시에 열렸을 때 그들끼리의 셸 순서까지 정확히
    // 강제하려면 host↔plugin 관계와 마찬가지로 `set_sublayer` 체인이 필요하지만
    // egui 는 1단 중첩만 지원해 N>2 개에서는 안전하지 않다 — 최소 설계 범위 밖으로 남긴다
    // (`gfx/gpu/egui_bridge.rs` 의 `enforce_host_plugin_popup_z_order` 문서 참고).
    mesh_snaps.sort_by_key(|s| s.z_seq);
    mesh_snaps
}

/// 이번 frame 에 그릴 인스턴스와 그 셸 rect. 범위가 안 보이는 인스턴스는 빠진다
/// (`place_popup` 이 `None`) — 이 목록이 셸·합성 영역·히트테스트·Esc 의 유일한 재료다.
fn place_visible(
    snaps: Vec<MeshSnap>,
    layout: Option<&LayoutContext>,
    screen_rect: Rect,
    pointer_pos: Option<Pos2>,
) -> Vec<(MeshSnap, Rect)> {
    snaps
        .into_iter()
        .filter_map(|snap| {
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

/// forward 할 입력에 사용자의 **확정형** 조작(포인터 버튼 누름 · 키 누름)이 들었는가.
///
/// 포인터 이동·휠·`PointerGone` 은 세지 않는다 — 창 위를 지나가기만 해도 생기고, 무엇을
/// 고른 것이 아니다. 이 판정이 참인 인스턴스만 plugin 이 그 popup 을 근거로 사용자 행동을
/// 주장할 수 있다([`crate::state::AppState::plugin_popup_user_activated`], ADR-0526).
fn is_user_activation(raw: &RawInputWire) -> bool {
    raw.events.iter().any(|ev| {
        matches!(
            ev,
            RawInputEventWire::PointerButton { pressed: true, .. }
                | RawInputEventWire::Key { pressed: true, .. }
        )
    })
}

/// popup 콘텐츠 영역 위 egui 입력을 surface-local 논리 포인트(좌상단 0,0) 와이어로 변환.
///
/// host 가 받은 *실제* 사용자 입력만 forward 한다(identity 원칙 1·3). 포인터 이벤트는
/// 콘텐츠 영역 안의 것만 보내 — 영역 밖 클릭은 plugin 으로 새지 않고 outside-click
/// dismiss 로만 처리된다. `pointer_occluded` 면(이 popup 보다 위 popup 이 포인터 좌표를
/// 덮는다) 콘텐츠 영역 안이라도 포인터 이벤트를 보내지 않는다.
///
/// 키/텍스트/IME 는 `has_key_focus`(= 이번 프레임 최상단 popup) 일 때만 보낸다 — 아래 깔린
/// popup 이 Esc 나 문자를 받아 자기 UI 로 처리하면 규칙 7 이 깨진다. 같은 값을 wire 의
/// `focused` 로도 실어 plugin 쪽 egui 가 커서/포커스 표시를 맞추게 한다.
fn collect_mesh_popup_input(
    ctx: &Context,
    content_rect: Rect,
    pointer_pos: Option<Pos2>,
    pointer_occluded: bool,
    has_key_focus: bool,
) -> RawInputWire {
    let origin = content_rect.min;
    let pointer_inside = !pointer_occluded && pointer_pos.is_some_and(|p| content_rect.contains(p));
    let accepts_pointer = |p: Pos2| !pointer_occluded && content_rect.contains(p);
    // 노치 거리는 `ctx.input` 밖에서 읽는다 — 같은 컨텍스트의 다른 잠금이라
    // 중첩을 만들 이유가 없다.
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
                } if accepts_pointer(*pos) => {
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
                // 와이어 `Scroll` 은 논리 포인트 단위다 — 물리 마우스 휠이 싣고 오는
                // `Line` 단위를 여기서 환산하지 않으면 notch 당 1pt 만 도착해 egui-mesh
                // surface 와 이동량이 갈린다(`wire_scroll` 모듈 문서).
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
                // IME 조합은 네 갈래를 **전부** 나른다. egui 0.31 `TextEdit` 의 Commit
                // 분기는 `state.ime_cursor_range` 가 Enabled/Preedit 에서 세워져 있어야만
                // 텍스트를 삽입하므로, Commit 만 실으면 조합 결과가 조용히 사라진다
                // (화면도 안 깨지고 에러도 없다). egui-winit 이 이미 OS 차이를 흡수해
                // ctx 에 넣어 준 것이라 여기서 플랫폼 분기를 다시 하지 않는다.
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
                Event::PointerGone => events.push(RawInputEventWire::PointerGone),
                _ => {}
            }
        }
        RawInputWire {
            time: None,
            // 최상단 popup 만 키 입력을 받는다(규칙 7) — plugin 쪽 egui 가 커서/포커스
            // 표시를 host 판정과 맞추도록 같은 값을 실어 보낸다.
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

/// `rect` 를 `bg_fill` 로 채우되 `content_rect`(plugin mesh 콘텐츠가 합성될 영역)는 비워
/// 둔다("hole"). 4개의 축정렬 띠로 분해한다 — 상/하단 띠만 `rect`의 바깥쪽 모서리에 맞춰
/// 둥근 모서리를 적용하고(원래 단일 `rect_filled(rect, corner_radius, ..)` 와 동일한
/// 시각 결과), 좌/우 띠는 `content_rect` 상하 범위로 제한되어 둥글릴 모서리가 없다.
/// `content_margin() >= corner_radius` 라 모서리 곡선이 상/하단 띠 폭 안에 완전히
/// 들어온다(현재 테마 기본값: 둘 다 4px).
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

/// 이 frame 에 깔 scrim 자리 — `(인스턴스별 scope rect, 그 자리에서 깐다)`.
///
/// scrim 은 **scope 당 한 번**이다. 인스턴스마다 깔면 같은 알파가 곱해져 popup 이 둘
/// 열린 순간 바닥만 두 배로 어두워진다. 어느 자리가 이기는지는 host popup 과 **같은**
/// 판정기([`PopupManager::pick_scrim_layers`])가 정한다 — 두 경로가 같은 물음에 답을
/// 둘로 만들지 않으려는 것이다. 덮는 자리는 그 popup 이 묶인 scope 의 rect 이고,
/// 바인딩이 없으면(창 범위 · 이 frame 에 없는 칸) 화면이다.
fn scrim_plan(
    placed: &[(MeshSnap, Rect)],
    layout: Option<&LayoutContext>,
    screen_rect: Rect,
) -> (Vec<Rect>, Vec<bool>) {
    let rects: Vec<Rect> = placed
        .iter()
        .map(|(snap, _)| PopupManager::scope_rect(&snap.scope, layout).unwrap_or(screen_rect))
        .collect();
    let paints = PopupManager::pick_scrim_layers(&rects);
    (rects, paints)
}

/// 인스턴스의 셸 rect. 범위가 이 frame 에 안 보이면 `None`.
///
/// 가시성과 경계는 host popup 과 같은 함수로 판정한다. 경계가 없는 범위(`Window`)는 화면이고,
/// surface 범위는 그 칸에서 8px 안쪽이다 — host popup 과 같은 [`PopupManager::scope_bounds`].
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
    // 경계보다 큰 셸은 경계로 줄인다 — host popup 의 `clamp_to_screen` 과 같다. 위치만
    // 붙잡고 크기를 그대로 두면 좁은 칸에서 셸이 이웃 칸으로 넘쳐 나가고, 콘텐츠는
    // egui layer 가 아니라 GPU 합성으로 올라가 layer 클립이 그것까지 잘라 주지 않는다.
    // 여기서 줄이면 plugin 에 forward 되는 콘텐츠 크기도 같은 값으로 따라온다.
    let size = Vec2::new(size.x.min(bounds.width()), size.y.min(bounds.height()));
    let pos = clamp_to_bounds(anchor_pos(anchor, size, bounds, pointer_pos), size, bounds);
    Some(Rect::from_min_size(pos, size))
}

/// 범위 경계 안으로 popup 좌상단을 clamp.
fn clamp_to_bounds(pos: Pos2, size: Vec2, bounds: Rect) -> Pos2 {
    egui::pos2(
        pos.x
            .clamp(bounds.min.x, (bounds.max.x - size.x).max(bounds.min.x)),
        pos.y
            .clamp(bounds.min.y, (bounds.max.y - size.y).max(bounds.min.y)),
    )
}

/// 앵커 위치. 가운데 정렬의 기준은 화면이 아니라 **범위 경계**다(host popup 의
/// `request_center` 와 같다) — `window` 범위에서는 둘이 같다.
fn anchor_pos(anchor: PopupAnchor, size: Vec2, bounds: Rect, pointer_pos: Option<Pos2>) -> Pos2 {
    let centered = egui::pos2(
        bounds.center().x - size.x / 2.0,
        bounds.center().y - size.y / 2.0,
    );
    match anchor {
        PopupAnchor::ScreenCenter => centered,
        PopupAnchor::Cursor => pointer_pos.unwrap_or(centered),
        // 활성 surface 를 따로 찾지 않는다 — 그 surface 에 붙고 싶은 popup 은
        // `scope = "surface"` 를 선언하고, 그러면 경계 자체가 그 surface 라 가운데가 곧
        // surface 가운데다. `window` 범위에서는 화면 가운데와 같다.
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

    /// 누름만 사용자 활성화다 — 지나가는 포인터 · 휠 · 떼기는 무엇을 고른 것이 아니다(ADR-0526).
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
    /// 오른쪽 아래 1/4 에 있는 surface 7.
    const SURFACE_7: Rect = Rect {
        min: Pos2::new(800.0, 500.0),
        max: Pos2::new(1600.0, 1000.0),
    };

    fn layout_with_surface_7(visible: bool) -> LayoutContext {
        LayoutContext {
            active_workspace: 0,
            pane_rects: Vec::new(),
            surface_rects: if visible {
                vec![(7, SURFACE_7)]
            } else {
                Vec::new()
            },
            active_tabs: Vec::new(),
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
            },
            z_seq: 1,
            scope_surface,
        }
    }

    /// 인스턴스 → 사본 → 배치의 **draw 가 쓰는 두 단계 그대로** 통과시켜 셸 rect 를 잰다.
    fn placed_rect(
        inst: &tasty_host_plugin::PopupInstance,
        layout: &LayoutContext,
        pointer: Option<Pos2>,
    ) -> Option<Rect> {
        let snaps = mesh_snapshots(std::iter::once((1, inst)));
        place_visible(snaps, Some(layout), SCREEN, pointer)
            .into_iter()
            .next()
            .map(|(_, r)| r)
    }

    /// surface 범위 popup 은 그 surface 영역 가운데에 놓인다 — 화면 가운데가 아니다.
    #[test]
    fn a_surface_scoped_popup_centers_on_its_surface() {
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let rect = placed_rect(&inst, &layout_with_surface_7(true), None)
            .expect("surface 가 보이면 그려져야 한다");
        assert_eq!(rect.center(), SURFACE_7.center());
    }

    /// surface 가 이 frame layout 에 없으면(다른 워크스페이스·탭) 그리지 않는다 — 셸·히트
    /// 테스트·Esc 의 재료인 배치 목록에서 빠진다.
    #[test]
    fn a_surface_scoped_popup_is_not_placed_while_its_surface_is_hidden() {
        let inst = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        assert_eq!(
            placed_rect(&inst, &layout_with_surface_7(false), None),
            None
        );
    }

    /// 위치 clamp 의 경계는 화면이 아니라 surface 다 — 포인터가 surface 밖 왼쪽 위에 있어도
    /// surface 범위 plugin popup 의 scrim 자리는 **그 칸**이다 — 화면이 아니다.
    /// 창 범위는 종전대로 화면이고, 같은 칸에 둘이 떠도 한 번만 깔린다.
    #[test]
    fn the_scrim_of_a_surface_scoped_plugin_popup_covers_its_surface_once() {
        let layout = layout_with_surface_7(true);
        let bound = instance(PopupScopeDecl::Surface, Some(7), PopupAnchor::ScreenCenter);
        let placed = place_visible(
            mesh_snapshots([(1, &bound), (2, &bound)].into_iter()),
            Some(&layout),
            SCREEN,
            None,
        );
        assert_eq!(placed.len(), 2, "인스턴스 둘이 배치돼야 중복 제거가 재진다");
        let (rects, paints) = scrim_plan(&placed, Some(&layout), SCREEN);
        assert_eq!(rects, vec![SURFACE_7, SURFACE_7]);
        assert_eq!(paints, vec![true, false], "같은 칸에 두 번 깔지 않는다");
    }

    /// 창 범위와 바인딩 없는 surface 선언은 화면 전체를 덮는다 — 종전 그대로.
    #[test]
    fn the_scrim_of_a_window_scoped_plugin_popup_stays_the_whole_screen() {
        let hidden = layout_with_surface_7(false);
        for inst in [
            instance(PopupScopeDecl::Window, Some(7), PopupAnchor::ScreenCenter),
            instance(PopupScopeDecl::Surface, None, PopupAnchor::ScreenCenter),
        ] {
            let placed = place_visible(
                mesh_snapshots(std::iter::once((1, &inst))),
                Some(&hidden),
                SCREEN,
                None,
            );
            let (rects, paints) = scrim_plan(&placed, Some(&hidden), SCREEN);
            assert_eq!(rects, vec![SCREEN]);
            assert_eq!(paints, vec![true]);
        }
    }

    /// popup 좌상단은 surface **안쪽 8pt** 에 머문다 — 칸 보더에 딱 붙지 않는다.
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

    /// 선언이 없거나(`window`) 대상이 바인딩되지 않은 surface 선언은 이전 동작 그대로다 —
    /// 항상 보이고 화면 가운데.
    #[test]
    fn window_scope_and_unbound_surface_scope_keep_the_screen() {
        let hidden = layout_with_surface_7(false);
        for inst in [
            instance(PopupScopeDecl::Window, Some(7), PopupAnchor::ScreenCenter),
            instance(PopupScopeDecl::Surface, None, PopupAnchor::ScreenCenter),
        ] {
            let rect = placed_rect(&inst, &hidden, None).expect("창 범위는 항상 그려진다");
            assert_eq!(rect.center(), SCREEN.center());
        }
    }

    /// popup 콘텐츠 위에서 휠 이벤트 하나를 받았을 때 와이어에 실리는 `Scroll` 값을
    /// **실제 수집 함수**로 재서 돌려준다(논리 포인트). 스크롤 이벤트가 없으면 `None`.
    fn collected_scroll(unit: egui::MouseWheelUnit, delta: Vec2) -> Option<(f32, f32)> {
        collected_scroll_with_notch(unit, delta, tasty_settings::DEFAULT_WHEEL_LINE_SCROLL)
    }

    /// 위와 같되 노치 거리(host 가 egui 옵션에 밀어 넣는 값)를 지정한다 — 그 값이
    /// 실제로 와이어까지 흐르는지 재는 데 쓴다.
    fn collected_scroll_with_notch(
        unit: egui::MouseWheelUnit,
        delta: Vec2,
        notch: f32,
    ) -> Option<(f32, f32)> {
        let ctx = Context::default();
        // host 가 창을 만들 때 하는 것과 같다 — 이 설정 없이는 egui 기본값(40)이 남는다.
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
        // 한 pass 를 실제로 돌려 `InputState` 를 채운다. 렌더 산출물(`FullOutput`)은
        // 이 측정에 쓰지 않는다 — 필요한 것은 수집 함수가 만든 와이어 입력뿐이다.
        let _full_output = ctx.run(input, |ctx| {
            wire = Some(collect_mesh_popup_input(
                ctx,
                content_rect,
                Some(pointer),
                false,
                true,
            ));
        });
        wire?.events.iter().find_map(|e| match e {
            RawInputEventWire::Scroll { x, y } => Some((*x, *y)),
            _ => None,
        })
    }

    /// 위 `collected_scroll` 이 쓰는 화면 높이 — `Page` 단위 기대값 계산에 쓴다.
    const PAGE_HEIGHT: f32 = 800.0;

    /// 물리 마우스 휠 1 notch(egui-winit 이 `Line` 단위로 싣는다)가 egui-mesh surface
    /// 경로와 **같은 거리**로 와이어에 실린다.
    #[test]
    fn a_wheel_notch_reaches_the_wire_as_the_shared_line_scroll_distance() {
        let (dx, dy) = collected_scroll(egui::MouseWheelUnit::Line, Vec2::new(0.0, -1.0))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(dx, 0.0);
        assert_eq!(dy, -tasty_settings::DEFAULT_WHEEL_LINE_SCROLL);
        // 환산 전에는 델타(-1.0)가 그대로 실렸다 — 그 값과 다르다는 것이 이 수정의 요지다.
        assert_ne!(dy, -1.0);
    }

    /// 노치 거리는 이제 상수가 아니라 **host egui 옵션**에서 온다 — 사용자가 설정을
    /// 바꾸면 popup 이 받는 거리도 따라와야 한다. 상수로 되돌아가면 이 테스트가 죽는다.
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

    /// 트랙패드 델타는 이미 논리 포인트(`Point`)라 그대로 실린다 — 환산이 이 경로의
    /// 값을 바꾸지 않는다(회귀 방지).
    #[test]
    fn a_trackpad_delta_reaches_the_wire_unchanged() {
        let (dx, dy) = collected_scroll(egui::MouseWheelUnit::Point, Vec2::new(2.0, -17.5))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!((dx, dy), (2.0, -17.5));
    }

    /// `Page` 는 화면 높이로 환산된다(egui 자신과 같은 규칙).
    #[test]
    fn a_page_delta_reaches_the_wire_scaled_by_the_screen_height() {
        let (_, dy) = collected_scroll(egui::MouseWheelUnit::Page, Vec2::new(0.0, -1.0))
            .expect("휠 이벤트가 와이어 Scroll 로 수집돼야 한다");
        assert_eq!(dy, -PAGE_HEIGHT);
    }

    /// egui ctx 에 들어온 이벤트 목록을 **실제 수집 함수**로 통과시켜 와이어 이벤트를
    /// 돌려준다. `has_key_focus` 를 그대로 노출해 게이트를 함께 잰다.
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
            ));
        });
        wire.expect("수집 함수가 와이어를 만들어야 한다").events
    }

    /// 수집된 와이어에서 IME 갈래만 뽑는다.
    fn collected_ime(events: Vec<Event>, has_key_focus: bool) -> Vec<ImeWire> {
        collected_events(events, has_key_focus)
            .into_iter()
            .filter_map(|e| match e {
                RawInputEventWire::Ime { event } => Some(event),
                _ => None,
            })
            .collect()
    }

    /// 조합 세션의 **네 갈래가 전부** 와이어에 실린다. Commit 만 실으면 egui 0.31
    /// `TextEdit` 이 `state.ime_cursor_range` 를 못 세워 글자를 조용히 버린다 — 그래서
    /// 이 시험의 모수는 프로토콜 쪽 round-trip 시험과 같은 4 단계다.
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

    /// IME 는 `Key`/`Text` 와 **같은 게이트**를 탄다 — 최상단이 아닌 popup 은 조합 문자를
    /// 받지 않는다(규칙 7). 게이트가 빠지면 아래 깔린 popup 이 조합을 먹는다.
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
            "포커스 없는 popup 에 IME 가 실렸다: {got:?}"
        );
    }

    /// 영문 경로(`Event::Text`)는 그대로다 — IME 갈래를 더한 것이 기존 입력을 바꾸지
    /// 않는다(회귀 방지).
    #[test]
    fn plain_text_still_reaches_the_wire() {
        let got = collected_events(vec![Event::Text("a".into())], true);
        assert!(
            got.contains(&RawInputEventWire::Text { text: "a".into() }),
            "영문 텍스트가 와이어에서 사라졌다: {got:?}"
        );
    }
}

// 렌더 루프가 사용자 활성화를 기록하고 걷는지 — plugin 프로세스 없이 인스턴스를 세워 루프를
// 실제로 돌린다(ADR-0526 조건 3).
#[cfg(test)]
#[path = "popup_render_activation_tests.rs"]
mod activation_tests;
