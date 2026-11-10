//! 입력 재현과 UI 상태 조회를 위한 디버그 IPC. release에서는 모듈을 컴파일하지 않는다.

#![cfg(debug_assertions)]

use super::params::{self, p_try};
use crate::runtime::engine_access::EngineMut;
use serde_json::json;

use crate::state::RequestContext;
use tasty_ipc::protocol::JsonRpcResponse;

use super::require_surface_id;

pub(super) fn require_input_simulation(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: &serde_json::Value,
) -> Result<(), JsonRpcResponse> {
    if !engine.runtime.input_simulation_enabled {
        Err(JsonRpcResponse::error(
            id.clone(),
            -32001,
            "input simulation not enabled. Launch tasty with --enable-input-simulation",
        ))
    } else {
        Ok(())
    }
}

/// Inject a mouse event into a surface's PTY as if the terminal received it.
/// Encodes as SGR mouse (mode 1006) bytes: ESC [ < Cb ; Cx ; Cy M/m
#[cfg(debug_assertions)]
pub(super) fn handle_debug_inject_mouse(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    if let Err(e) = require_input_simulation(&engine.as_ref(), &id) {
        return e;
    }
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    // 입력 좌표는 0부터 시작한다. SGR로 보낼 때 1을 더한다.
    let col = match p_try!(params::opt_int::<u64>(params, "col", &id)) {
        Some(c) => c,
        None => return JsonRpcResponse::invalid_params(id, "Missing 'col' parameter"),
    };
    let row = match p_try!(params::opt_int::<u64>(params, "row", &id)) {
        Some(r) => r,
        None => return JsonRpcResponse::invalid_params(id, "Missing 'row' parameter"),
    };
    // 버튼 번호: 왼쪽 0, 가운데 1, 오른쪽 2.
    let button = p_try!(params::opt_int::<u64>(params, "button", &id)).unwrap_or(0);
    let event_type = params
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("press");

    let cb = match event_type {
        "press" | "release" => button as u8,
        "move" => 32 + button as u8,
        _ => return JsonRpcResponse::invalid_params(id, "event_type must be press/release/move"),
    };
    let bytes = tasty_terminal::encode_mouse_report(
        true,
        cb,
        (col + 1) as usize,
        (row + 1) as usize,
        event_type == "release",
    );

    if let Some(terminal) = engine.find_terminal_by_id_mut(surface_id) {
        terminal.send_bytes(&bytes);
        JsonRpcResponse::success(id, json!({"sent": true}))
    } else {
        JsonRpcResponse::invalid_params(id, format!("Surface {} not found", surface_id))
    }
}

/// 호스트에 정의된 팝업 목록. 플러그인 팝업은 debug.popup.list로 조회한다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_host_popup_list(
    state: &RequestContext,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let items: Vec<_> = crate::adapters::ui::popup::defs::all_defs()
        .iter()
        .map(|def| {
            // z_seq는 플러그인 팝업과 공유하므로 두 목록의 겹침 순서를 비교할 수 있다.
            let geom = state.popups.open_geometry(def.id);
            json!({
                "id": def.id,
                "title_key": def.title_key,
                "headless": def.headless,
                "close_on_outside_click": def.close_on_outside_click,
                "open": geom.is_some(),
                "z_seq": geom.map(|(z, _)| z),
                "rect": geom.map(|(_, r)| {
                    json!({ "x": r.min.x, "y": r.min.y, "w": r.width(), "h": r.height() })
                }),
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "popups": items }))
}

/// 사용자 클릭 없이 호스트 팝업을 연다. 디버그 빌드에서만 허용한다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_host_popup_open(
    state: &mut RequestContext,
    engine: &crate::core::CoreState,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(popup_id) = params.get("popup_id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'popup_id' parameter");
    };
    let Some(def) = crate::adapters::ui::popup::defs::find(popup_id) else {
        return JsonRpcResponse::error(id, -32602, format!("host popup '{popup_id}' not found"));
    };
    // 기본 창 범위 대신 workspace 범위를 지정해 가시성 조건을 시험한다.
    let workspace_scope = params
        .get("workspace_scope")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // surface 범위의 scrim과 여백을 시험한다. 포커스된 surface가 없으면 창 범위를 유지한다.
    let surface_scope = params
        .get("surface_scope")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let bound_surface = surface_scope
        .then(|| state.focused_surface_id(engine))
        .flatten();
    let mode = match (workspace_scope, bound_surface) {
        (_, Some(sid)) => crate::intent::OpenPopupMode::WithScope(
            crate::adapters::ui::popup::PopupScope::Surface(sid),
        ),
        (true, None) => crate::intent::OpenPopupMode::WithScope(
            crate::adapters::ui::popup::PopupScope::Workspace(state.active_workspace(engine).id),
        ),
        (false, None) => crate::intent::OpenPopupMode::CenteredFocused,
    };
    // 변환 팝업은 범위 외에도 dialogs에서 대상 surface를 읽는다.
    if def.id == "convert_surface" {
        state.dialogs.convert_popup = bound_surface;
        state.dialogs.convert_popup_selected = None;
    }
    // 사용자 조작 재현이므로 사용자 출처로 열어 포커스를 준다.
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup { id: def.id, mode }.from_user_menu("debug.popup.open"),
    );
    JsonRpcResponse::success(
        id,
        json!({
            "opened": def.id,
            "workspace_scope": workspace_scope,
            "surface_scope": bound_surface,
        }),
    )
}

#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_host_popup_close(
    state: &mut RequestContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(popup_id) = params.get("popup_id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'popup_id' parameter");
    };
    let Some(def) = crate::adapters::ui::popup::defs::find(popup_id) else {
        return JsonRpcResponse::error(id, -32602, format!("host popup '{popup_id}' not found"));
    };
    state.dispatch_intent(crate::intent::UiIntent::ClosePopup { id: def.id }.from_agent_ipc());
    JsonRpcResponse::success(id, json!({ "closed": def.id }))
}

/// modifier 홀드 상태와 경과 시간을 지정한다. 생략한 키는 false이며 모두 false면 해제한다.
/// 사용자 입력을 재현하는 디버그 기능이고, 응답은 state 조회와 같은 형식이다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_modhint_hold(
    state: &mut RequestContext,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let axis = |k: &str| params.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    state
        .modifier_hint
        .update_hold(axis("ctrl"), axis("alt"), axis("option"), axis("shift"));
    if let Some(ms) = p_try!(params::opt_int::<u64>(params, "elapsed_ms", &id)) {
        state
            .modifier_hint
            .debug_backdate(std::time::Duration::from_millis(ms));
    }
    let theme = crate::theme::theme();
    let reduced_motion = engine.runtime.settings.accessibility.reduced_motion;
    let dump = crate::adapters::ui::modifier_hint_overlay::debug_state_json(
        &state.modifier_hint,
        &engine.runtime.settings,
        &theme,
        reduced_motion,
    );
    JsonRpcResponse::success(id, dump)
}

/// draw와 같은 함수로 오버레이 상태를 계산해 반환한다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_modhint_state(
    state: &RequestContext,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let theme = crate::theme::theme();
    let reduced_motion = engine.runtime.settings.accessibility.reduced_motion;
    let dump = crate::adapters::ui::modifier_hint_overlay::debug_state_json(
        &state.modifier_hint,
        &engine.runtime.settings,
        &theme,
        reduced_motion,
    );
    JsonRpcResponse::success(id, dump)
}

/// 빌트인 배너 정의와 표시·대기 중인 배너의 상태를 반환한다.
/// rect는 egui 논리 좌표이고 content_rect는 플러그인 콘텐츠의 물리 픽셀 영역이다.
/// 호스트 배너의 content_rect는 null이다. 카드 rect는 직전 프레임 값이며 첫 프레임에는 null이다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_banner_list(
    state: &RequestContext,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let defs: Vec<_> = crate::adapters::ui::banner::defs::all_defs()
        .iter()
        .map(|def| {
            json!({
                "id": def.id,
                "ttl_seconds": def.ttl_seconds,
            })
        })
        .collect();
    let shown: Vec<_> = state
        .banners
        .shown_banners()
        .map(|b| {
            let queued: Vec<&str> = state
                .banners
                .queued_banners(&b.scope)
                .map(|q| q.id)
                .collect();
            let rect = state
                .banners
                .card_rect(&b.scope, &b.key())
                .map(|r| json!({ "x": r.min.x, "y": r.min.y, "w": r.width(), "h": r.height() }));
            let content_rect = match &b.content {
                crate::adapters::ui::banner::BannerContentSource::PluginMesh {
                    instance_id,
                    ..
                } => state
                    .plugin_mesh_banner_regions
                    .iter()
                    .find(|(inst, _)| inst == instance_id)
                    .map(|(_, r)| {
                        json!({
                            "x": r.x.value(),
                            "y": r.y.value(),
                            "w": r.width.value(),
                            "h": r.height.value(),
                        })
                    }),
                crate::adapters::ui::banner::BannerContentSource::Host
                | crate::adapters::ui::banner::BannerContentSource::AttachSizeSync { .. } => None,
            };
            json!({
                "id": b.id,
                "scope": b.scope.to_token(),
                "remaining_seconds": b.remaining_seconds(),
                "queued": queued,
                "rect": rect,
                "content_rect": content_rect,
            })
        })
        .collect();
    JsonRpcResponse::success(
        id,
        json!({
            "defs": defs,
            "shown": shown,
            "total_queued": state.banners.total_queued(),
            "coords": { "rect": "logical", "content_rect": "physical" },
        }),
    )
}

/// 지정 범위에 배너를 표시한다. scope는 view/workspace/pane/tab/surface 토큰이며
/// 배너 정의의 TTL을 적용한다. 사용자 동작을 재현하므로 디버그 빌드에서만 허용한다.
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_banner_show(
    state: &mut RequestContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(banner_id) = params.get("banner_id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'banner_id' parameter");
    };
    let Some(scope_token) = params.get("scope").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'scope' parameter");
    };
    let Some(def) = crate::adapters::ui::banner::defs::find(banner_id) else {
        return JsonRpcResponse::error(id, -32602, format!("banner '{banner_id}' not found"));
    };
    let Some(scope) = crate::adapters::ui::BannerScope::from_token(scope_token) else {
        return JsonRpcResponse::error(id, -32602, format!("invalid scope '{scope_token}'"));
    };
    let banner = crate::adapters::ui::BannerState::from_def(def, scope);
    let outcome = state.banners.push(banner);
    JsonRpcResponse::success(id, json!({ "outcome": format!("{outcome:?}") }))
}

/// `debug.banner.close` — `{ banner_id }` 로 배너를 닫는다 (표시 중이면 큐 head 승격).
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_banner_close(
    state: &mut RequestContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(banner_id) = params.get("banner_id").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'banner_id' parameter");
    };
    let Some(def) = crate::adapters::ui::banner::defs::find(banner_id) else {
        return JsonRpcResponse::error(id, -32602, format!("banner '{banner_id}' not found"));
    };
    let closed = state.banners.close_by_id(def.id);
    JsonRpcResponse::success(id, json!({ "closed": closed }))
}

/// `debug.banner.set_countdown` — `{ scope, seconds }` 로 표시 중 TTL 배너의
/// 남은 시간을 강제 설정한다 (만료 직전 상태 등 시각 검증용).
#[cfg(all(debug_assertions, feature = "gui"))]
pub(super) fn handle_debug_banner_set_countdown(
    state: &mut RequestContext,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(scope_token) = params.get("scope").and_then(|v| v.as_str()) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'scope' parameter");
    };
    let Some(seconds) = p_try!(params::opt_int::<u64>(params, "seconds", &id)) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'seconds' parameter");
    };
    let Some(scope) = crate::adapters::ui::BannerScope::from_token(scope_token) else {
        return JsonRpcResponse::error(id, -32602, format!("invalid scope '{scope_token}'"));
    };
    let applied = state.banners.set_countdown(&scope, seconds as u32);
    JsonRpcResponse::success(id, json!({ "applied": applied }))
}

/// Inject a key event into a surface's PTY.
#[cfg(debug_assertions)]
pub(super) fn handle_debug_inject_key(
    engine: &mut EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    if let Err(e) = require_input_simulation(&engine.as_ref(), &id) {
        return e;
    }
    let surface_id = match require_surface_id(params, &id) {
        Ok(sid) => sid,
        Err(e) => return e,
    };
    let bytes = match params.get("bytes").and_then(|v| v.as_str()) {
        Some(hex) => {
            let hex = hex.trim();
            let mut result = Vec::new();
            for i in (0..hex.len()).step_by(2) {
                match u8::from_str_radix(&hex[i..i.min(hex.len()).max(i + 2)], 16) {
                    Ok(b) => result.push(b),
                    Err(_) => return JsonRpcResponse::invalid_params(id, "Invalid hex in 'bytes'"),
                }
            }
            result
        }
        None => match params.get("text").and_then(|v| v.as_str()) {
            Some(t) => t.as_bytes().to_vec(),
            None => {
                return JsonRpcResponse::invalid_params(id, "Missing 'bytes' or 'text' parameter");
            }
        },
    };

    if let Some(terminal) = engine.find_terminal_by_id_mut(surface_id) {
        terminal.send_key(&String::from_utf8_lossy(&bytes));
        JsonRpcResponse::success(id, json!({"sent": true}))
    } else {
        JsonRpcResponse::invalid_params(id, format!("Surface {} not found", surface_id))
    }
}

/// 다음 프레임의 present 직전에 ms만큼 멈춰 GPU 응답 지연 감시를 시험한다.
/// 실제 드라이버의 멈춤을 재현하는 대신 이벤트 루프의 같은 위치를 지연시킨다.
pub(super) fn handle_debug_gpu_stall(
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(ms) = params.get("ms").and_then(serde_json::Value::as_u64) else {
        return JsonRpcResponse::invalid_params(id, "Missing required 'ms' parameter (u64)");
    };
    crate::stall_watchdog::arm_debug_stall(ms);
    JsonRpcResponse::success(id, serde_json::json!({ "armed_ms": ms }))
}
