//! 포커스 창을 사용하는 debug 입력·조회 메서드. release에서는 처리하지 않는다.
//! ID로 대상을 지정하는 ui.screenshot은 app_methods에 있다.

#[cfg(debug_assertions)]
use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
#[cfg(debug_assertions)]
use crate::ipc as host_ipc;
use crate::ipc::server::IpcCommand;
#[cfg(debug_assertions)]
use crate::ipc::server::send_response;

/// 잘못된 단위를 기본값으로 바꾸면 다른 주입 경로를 시험하게 되므로 거절한다.
#[cfg(debug_assertions)]
fn reject_unknown_scroll_unit(cmd: &IpcCommand) -> IpcStep {
    let response = host_ipc::protocol::JsonRpcResponse::error(
        cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
        -32602,
        "unknown scroll unit: expected \"line\", \"point\" or \"page\"",
    );
    send_response(&cmd.response_tx, response);
    IpcStep::Handled
}

#[cfg(debug_assertions)]
fn reject_bad_params(cmd: &IpcCommand, msg: &str) -> IpcStep {
    let response = host_ipc::protocol::JsonRpcResponse::error(
        cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
        -32602,
        msg,
    );
    send_response(&cmd.response_tx, response);
    IpcStep::Handled
}

/// 두 포인터 주입 경로에서 숫자 인자를 같은 방식으로 읽는다.
/// 알 수 없는 버튼 번호는 Left, event_type은 Move가 된다.
#[cfg(debug_assertions)]
pub(super) fn read_pointer_params(
    p: &serde_json::Value,
    unit: crate::view::main::debug_input::ScrollUnit,
) -> Result<(f32, f32, crate::view::main::debug_input::InjectPointer), String> {
    use crate::view::main::debug_input::InjectPointer;

    // 좌표는 비율로 받으며 여기서 0..1 범위로 제한하지는 않는다.
    let fx = params::read_f64(p, "fx")?.unwrap_or(0.5) as f32;
    let fy = params::read_f64(p, "fy")?.unwrap_or(0.5) as f32;
    let button = match params::read_int::<u64>(p, "button")?.unwrap_or(0) {
        1 => winit::event::MouseButton::Middle,
        2 => winit::event::MouseButton::Right,
        _ => winit::event::MouseButton::Left,
    };
    let event_type = p
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("move");
    let action = match event_type {
        "press" => InjectPointer::Button {
            button,
            pressed: true,
        },
        "release" => InjectPointer::Button {
            button,
            pressed: false,
        },
        "scroll" => InjectPointer::Scroll {
            dx: params::read_f64(p, "scroll_dx")?.unwrap_or(0.0) as f32,
            dy: params::read_f64(p, "scroll_dy")?.unwrap_or(0.0) as f32,
            unit,
        },
        _ => InjectPointer::Move,
    };
    Ok((fx, fy, action))
}

/// `modifiers` 가 없으면 `None`(현재 수식 키 유지), 배열이면 그 키만 누른 상태다.
/// 이름은 `shift`·`ctrl`·`alt`·`command` 이고 빈 배열은 모두 뗀다.
#[cfg(debug_assertions)]
pub(super) fn read_egui_modifiers(
    p: &serde_json::Value,
) -> Result<Option<egui::Modifiers>, String> {
    let Some(v) = p.get("modifiers") else {
        return Ok(None);
    };
    let names = v
        .as_array()
        .ok_or_else(|| "modifiers must be an array of strings".to_string())?;
    let mut m = egui::Modifiers::default();
    for name in names {
        match name.as_str() {
            Some("shift") => m.shift = true,
            Some("alt") => m.alt = true,
            // egui-winit 과 같이 macOS 가 아니면 Ctrl 이 command 다.
            Some("ctrl") => {
                m.ctrl = true;
                m.command |= !cfg!(target_os = "macos");
            }
            // egui-winit 은 macOS 에서 Cmd 를, 그 밖에서는 Ctrl 을 command 로 본다.
            Some("command") => {
                if cfg!(target_os = "macos") {
                    m.mac_cmd = true;
                } else {
                    m.ctrl = true;
                }
                m.command = true;
            }
            _ => {
                return Err(format!(
                    "unknown modifier {name}; use shift, ctrl, alt or command"
                ));
            }
        }
    }
    Ok(Some(m))
}

/// `menu`(메뉴 종류, 필수)와 `item`(항목 id)·`label`(표시 문구)·`dismiss`(true) 중 하나를 받는다.
#[cfg(debug_assertions)]
fn read_menu_answer(
    p: &serde_json::Value,
) -> Result<crate::view::main::debug_menu::MenuAnswer, String> {
    use crate::view::main::debug_menu::{MENU_KINDS, MenuAnswer, MenuChoice};
    let menu = p.get("menu").and_then(|v| v.as_str()).unwrap_or_default();
    let Some(menu) = MENU_KINDS.iter().copied().find(|k| *k == menu) else {
        return Err(format!("'menu' must be one of {}", MENU_KINDS.join(", ")));
    };
    let item = params::read_u32(p, "item")?;
    let label = p.get("label").and_then(|v| v.as_str());
    let dismiss = p.get("dismiss").and_then(|v| v.as_bool()).unwrap_or(false);
    let choice = match (item, label, dismiss) {
        (Some(id), None, false) => MenuChoice::Item(id),
        (None, Some(text), false) => MenuChoice::Label(text.to_owned()),
        (None, None, true) => MenuChoice::Dismiss,
        _ => return Err("give exactly one of 'item', 'label' or 'dismiss: true'".to_owned()),
    };
    Ok(MenuAnswer { menu, choice })
}

impl App {
    #[cfg(not(debug_assertions))]
    pub(crate) fn ipc_step_window_required(&mut self, _cmd: &IpcCommand) -> IpcStep {
        IpcStep::NotHandled
    }

    #[cfg(debug_assertions)]
    pub(crate) fn ipc_step_window_required(&mut self, cmd: &IpcCommand) -> IpcStep {
        // 새 창 의존 메서드를 추가하면 이 분류도 갱신해야 한다.
        let is_window_required = cmd.request.method.starts_with("surface.ime_")
            || cmd.request.method == "debug.info"
            || cmd.request.method == "debug.inject_window_mouse"
            || cmd.request.method == "debug.inject_egui_mouse"
            || cmd.request.method == "debug.inject_egui_key"
            || cmd.request.method == "debug.inject_egui_text"
            || cmd.request.method == "debug.selection"
            || cmd.request.method == "debug.pending_menu"
            || cmd.request.method == "debug.native_menu.answer"
            || cmd.request.method == "debug.pending_move"
            || cmd.request.method == "debug.focused_surface"
            || cmd.request.method == "debug.surface_rect";
        if !is_window_required {
            return IpcStep::NotHandled;
        }
        let focused_id = match self.view.focused_view_id {
            Some(id) => id,
            None => {
                let response = host_ipc::protocol::JsonRpcResponse::error(
                    cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                    -32000,
                    "No window available for this command",
                );
                send_response(&cmd.response_tx, response);
                return IpcStep::Handled;
            }
        };
        self.ipc_step_main_window(cmd, focused_id, None)
    }

    /// 메인 창 `window` 에서 창 의존 메서드를 처리한다. 포커스 창 경로와 `window_id` 로 메인 창을 지정한 egui 주입
    /// (`debug_methods`)이 같은 진입(포인터 `press_focus`, 글자·키 `key_focus`)을 거친다. `echo_window` 를 주면 egui
    /// 주입 응답에 그 창 ID 를 싣는다.
    #[cfg(debug_assertions)]
    pub(crate) fn ipc_step_main_window(
        &mut self,
        cmd: &IpcCommand,
        focused_id: winit::window::WindowId,
        echo_window: Option<u64>,
    ) -> IpcStep {
        let echo = |mut body: serde_json::Value| {
            if let Some(wid) = echo_window {
                body["window_id"] = serde_json::json!(wid);
            }
            body
        };
        let (w, mut engine) = match self.engines_mut().window_pair(focused_id) {
            Some(pair) => pair,
            // 현재 ID가 MainView를 가리키지 않으면 별도 응답 없이 처리됨으로 반환한다.
            None => return IpcStep::Handled,
        };

        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.info" {
            let debug_data = crate::debug_info::collect(
                &w.state,
                &engine.read(),
                Some(&w.base.gpu),
                w.ime_active,
            );
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                debug_data,
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.inject_window_mouse" {
            use crate::view::main::debug_input::ScrollUnit;
            let params = &cmd.request.params;
            let Some(unit) = ScrollUnit::from_name(
                params
                    .get("unit")
                    .and_then(|v| v.as_str())
                    .unwrap_or("line"),
            ) else {
                return reject_unknown_scroll_unit(cmd);
            };
            let surface_id = match params::read_u32(params, "surface_id") {
                Ok(v) => v.unwrap_or(0),
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let (fx, fy, action) = match read_pointer_params(params, unit) {
                Ok(v) => v,
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let ok = w.debug_inject_mesh_pointer(&engine.read(), surface_id, fx, fy, action);
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                serde_json::json!({ "injected": ok }),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // winit 이벤트 대신 egui 입력 큐에 넣어 popup 입력 전달을 시험한다.
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.inject_egui_mouse" {
            use crate::view::main::debug_input::ScrollUnit;
            let params = &cmd.request.params;
            let Some(unit) = ScrollUnit::from_name(
                params
                    .get("unit")
                    .and_then(|v| v.as_str())
                    .unwrap_or("point"),
            ) else {
                return reject_unknown_scroll_unit(cmd);
            };
            let (fx, fy, action) = match read_pointer_params(params, unit) {
                Ok(v) => v,
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let surface_id = match params::read_u32(params, "surface_id") {
                Ok(v) => v,
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let modifiers = match read_egui_modifiers(params) {
                Ok(v) => v,
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let ok =
                w.debug_inject_egui_pointer(&engine.read(), fx, fy, surface_id, action, modifiers);
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                echo(serde_json::json!({ "injected": ok })),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // TextEdit의 문자 입력은 키 이벤트와 별도의 Text 이벤트다.
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.inject_egui_text" {
            let params = &cmd.request.params;
            let Some(text) = params.get("text").and_then(|v| v.as_str()) else {
                return reject_bad_params(cmd, "missing or non-string 'text'");
            };
            let ok = w.debug_inject_egui_text(&engine.read(), text);
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                echo(serde_json::json!({ "injected": ok })),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.inject_egui_key" {
            // 메인 창의 키 주입은 창의 키처럼 입력 몫을 끊고 넣는다(`inject_key`).
            let read = engine.read();
            let body = match super::debug_egui_key::egui_key_body_with(
                &cmd.request.params,
                |key, pressed, modifiers| {
                    crate::view::main::debug_input::inject_key(
                        &mut w.state,
                        &read,
                        &mut w.base.gpu,
                        key,
                        pressed,
                        modifiers,
                    )
                },
            ) {
                Ok(body) => body,
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                echo(body),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.selection" {
            let sel = w.text_selection.as_ref();
            let body = match sel {
                Some(s) => {
                    let n = s.normalized();
                    serde_json::json!({
                        "present": true,
                        "surface_id": s.surface_id,
                        "mode": format!("{:?}", s.mode),
                        "dragging": s.dragging,
                        "empty": s.is_empty(),
                        "anchor": { "col": s.anchor.col, "row": s.anchor.absolute_row },
                        "cursor": { "col": s.cursor.col, "row": s.cursor.absolute_row },
                        "start": { "col": n.start.col, "row": n.start.absolute_row },
                        "end": { "col": n.end.col, "row": n.end.absolute_row },
                    })
                }
                None => serde_json::json!({ "present": false }),
            };
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                body,
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.pending_menu" {
            // 실제 열린 메뉴가 없으면 주입 시 캡처한 메뉴 정보를 조회한다.
            let menu = w
                .state
                .dialogs
                .pending_native_menu
                .as_ref()
                .or(w.debug_captured_menu.as_ref());
            let body = match menu {
                Some(menu) => {
                    let (kind, surface_id) = crate::view::main::debug_menu::menu_kind(menu);
                    let mut obj = serde_json::json!({ "present": true, "kind": kind });
                    if let Some(sid) = surface_id {
                        obj["surface_id"] = serde_json::json!(sid);
                    }
                    obj
                }
                None => serde_json::json!({ "present": false }),
            };
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                body,
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // 다음 native 메뉴의 선택을 재현한다. 메뉴는 이후 egui·window 우클릭 주입이 연다.
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.native_menu.answer" {
            let answer = match read_menu_answer(&cmd.request.params) {
                Ok(a) => a,
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let replaced = w.debug_set_menu_answer(answer);
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                serde_json::json!({ "armed": true, "replaced": replaced }),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.pending_move" {
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                w.debug_pending_move(),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // engine 목록과 별개인 view의 포커스 surface를 조회한다.
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.focused_surface" {
            let focused = w.state.focused_surface_id(engine.core);
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                serde_json::json!({ "surface_id": focused }),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        // 화면 배치를 읽는다. 비-터미널 surface 도 같은 leaf 사각형을 쓴다.
        #[cfg(debug_assertions)]
        if cmd.request.method == "debug.surface_rect" {
            let surface_id = match params::read_u32(&cmd.request.params, "surface_id") {
                Ok(Some(v)) => v,
                Ok(None) => return reject_bad_params(cmd, "missing 'surface_id'"),
                Err(msg) => return reject_bad_params(cmd, &msg),
            };
            let scale_factor = w.base.gpu.scale_factor();
            let rect = w.state.surface_rect_by_id(
                engine.core,
                surface_id,
                w.compute_terminal_rect(),
                scale_factor,
            );
            // 띠 kind면 띠 행의 아래 경계(물리 px), 아니면 null. 주소 띠·배너 높이(물리 px)는 그 아래에 더한다.
            let edge_bottom = rect
                .filter(|_| crate::state::surface_focus::reserves_edge(&engine.read(), surface_id))
                .map(|r| {
                    crate::state::surface_focus::edge_rows(&crate::theme::theme(), r, scale_factor)
                        .bottom
                });
            let content_top = w.html_content_top(surface_id, f64::from(scale_factor));
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                serde_json::json!({
                    "surface_id": surface_id,
                    "scale_factor": scale_factor,
                    "edge_bottom": edge_bottom,
                    "content_top": content_top,
                    "rect": rect.map(|r| serde_json::json!({
                        "x": r.x.value(),
                        "y": r.y.value(),
                        "width": r.width.value(),
                        "height": r.height.value(),
                    })),
                }),
            );
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        if cmd.request.method.starts_with("surface.ime_") {
            let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
            let response = host_ipc::handler::ime::handle_ime_method(
                w,
                &mut engine,
                &cmd.request.method,
                &cmd.request.params,
                id,
            );
            send_response(&cmd.response_tx, response);
            w.base.state.dirty = true;
        }
        IpcStep::Handled
    }
}
