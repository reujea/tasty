//! step 2: 호스트 자체 메서드.
//!
//! - `system.shutdown` (debug)
//! - `system.gpu_stats` (read-only GPU 리소스 카운트 — 메모리 누수 soak 검증)
//! - `timer.list` (read-only 타이머 허브 스냅샷 — 무엇이 인스턴스를 깨우는가)
//! - `window.create` / `window.close` / `window.focus` / `window.list`
//! - `plugin.*` (15개 메서드)
//! - `approval.await` (blocking — worker thread 위임)

mod remote;

use crate::AppEvent;
use crate::adapters::ipc::handler::params;
use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc as host_ipc;
use crate::ipc::server::{IpcCommand, send_response};

impl App {
    pub(crate) fn ipc_step_app_methods(
        &mut self,
        cmd: &IpcCommand,
        caller: &host_ipc::caller::CallerContext,
    ) -> IpcStep {
        // 멱등 키를 실은 `Mutate` 는 보존소를 먼저 지난다 — 이 층의 메서드는 engine
        // 라우터에 안 닿으므로 거기 보존소가 못 본다. 이 층이 그 이름을 안 맡으면
        // (`NotHandled`) 연 자리를 닫고 다음 층이 다시 판정한다(ADR-0421).
        if let Some(step) = host_ipc::handler::idempotency::run_app_layer(
            caller,
            cmd,
            IpcStep::Handled,
            |step| !matches!(step, IpcStep::NotHandled),
            |relayed| self.ipc_step_app_methods(relayed, caller),
        ) {
            return step;
        }
        #[cfg(debug_assertions)]
        if cmd.request.method == "system.shutdown" {
            let response = host_ipc::protocol::JsonRpcResponse::success(
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
                serde_json::json!({"shutdown": true}),
            );
            send_response(&cmd.response_tx, response);
            crate::shortcuts::send_app_event(&self.view.proxy, AppEvent::Shutdown);
            return IpcStep::Shutdown;
        }
        if cmd.request.method == "system.gpu_stats" {
            return self.ipc_handle_system_gpu_stats(cmd);
        }
        if cmd.request.method == "timer.list" {
            return self.ipc_handle_timer_list(cmd);
        }
        if cmd.request.method == "window.create" || cmd.request.method == "view.create" {
            // 완료 채널을 실어 보내고 응답은 defer 한다 — winit 핸들러가 창 생성
            // 성공/실패를 이 채널로 돌려준다(ADR-0122). 즉시 `{"scheduled": true}` 를
            // 돌려주던 fire-and-forget 은 실패를 요청자에게 못 알려, 실패가 사용자
            // toast 로만 새어나갔다(요청하지도 않은 일의 실패 통지 — 원칙 1 위반).
            let completion = crate::app::event::IpcCompletion::new(
                cmd.response_tx.clone(),
                cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            );
            crate::shortcuts::send_app_event(
                &self.view.proxy,
                AppEvent::CreateWindow(
                    crate::app::event::WindowRequestOrigin::Agent,
                    Some(completion),
                ),
            );
            return IpcStep::Handled;
        }
        if cmd.request.method == "window.close" || cmd.request.method == "view.close" {
            return self.ipc_handle_window_close(cmd);
        }
        // window.focus 는 사용자 입력 재현 (단축키/마우스 클릭 영역) 으로 분류.
        // CLAUDE.md: "CLI/IPC로 포커스·활성 탭·활성 워크스페이스를 전환하는 명령은
        // 존재하지 않는다." → release 빌드에 노출 안 함. debug 빌드만 유지.
        #[cfg(debug_assertions)]
        if cmd.request.method == "window.focus" || cmd.request.method == "view.focus" {
            return self.ipc_handle_window_focus(cmd);
        }
        if cmd.request.method == "window.list" || cmd.request.method == "view.list" {
            return self.ipc_handle_window_list(cmd);
        }
        if cmd.request.method == "ui.screenshot" {
            return self.ipc_handle_ui_screenshot(cmd);
        }
        if cmd.request.method == "clipboard.set_text" {
            return self.ipc_handle_clipboard_set_text(cmd);
        }
        if cmd.request.method.starts_with("plugin.") {
            return self.ipc_dispatch_plugin_method(cmd, caller);
        }
        if cmd.request.method == "approval.await" {
            self.ipc_dispatch_approval_await(cmd);
            return IpcStep::Handled;
        }
        if cmd.request.method == "events.fetch" {
            self.ipc_dispatch_events_fetch(cmd);
            return IpcStep::Handled;
        }
        if cmd.request.method == "agent.task_await" {
            self.ipc_dispatch_task_await(cmd);
            return IpcStep::Handled;
        }
        // remote.workspaces / remote.attach — 원격 워크스페이스 브라우징·attach 능력의
        // 로컬 IPC 노출(원칙 2: 에이전트가 CLI 없이 소켓만으로도 수행 가능). 블로킹 SSH
        // I/O 는 워커 스레드로 돌려 이벤트루프를 막지 않는다. CLI(`remote workspaces`)와
        // 동일한 `tasty_remote::browse` 코어를 공유한다.
        if cmd.request.method == "remote.workspaces" {
            self.ipc_dispatch_remote_workspaces(cmd);
            return IpcStep::Handled;
        }
        if cmd.request.method == "remote.attach" {
            self.ipc_dispatch_remote_attach(cmd);
            return IpcStep::Handled;
        }
        IpcStep::NotHandled
    }

    /// `timer.list` — 중앙 타이머 허브의 read-only 스냅샷.
    ///
    /// 일반 IPC 핸들러(`src/adapters/ipc/handler/`)가 아니라 여기 있는 이유: 허브는
    /// `App` 필드고 plugin manager 는 자기 허브를 따로 소유한다. `CoreState` 만 받는
    /// 핸들러에서는 둘 중 어느 쪽에도 닿지 못해 "무엇이 깨우고 있는가" 에 답할 수 없다.
    /// headless 는 같은 이유로 dispatch pump 에서 같은 함수를 부른다.
    ///
    /// 순수 조회 — 사용자 상태에 닿지 않고(원칙 1), 대상 지정이 필요 없는 전역
    /// 스냅샷이라 포커스 독립(원칙 3). local_only — plugin 미노출.
    fn ipc_handle_timer_list(&self, cmd: &IpcCommand) -> IpcStep {
        let response = host_ipc::protocol::JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            self.timer_list_json(std::time::Instant::now()),
        );
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    /// `system.gpu_stats` — GPU 리소스 카운트 read-only 조회 (메모리 누수 soak 검증용).
    ///
    /// 반환: wgpu 전역 리포트(`Instance::generate_report()` — 모든 창 합산 buffers/
    /// textures/texture_views/bind_groups … 의 live 카운트) + 창별 `GpuState` 카운트
    /// (egui-mesh target 맵 3종 len, atlas, draw calls) + 창별 explorer view 수
    /// (`explorer_views` — main 이 아닌 창은 null). soak 하네스가 "surface 를
    /// 닫았는데 카운트가 기준선으로 복귀하지 않음" 유형의 누수를 판정하는 1차 소스.
    ///
    /// 순수 조회 — 사용자 상태(focus/선택/스크롤)에 닿지 않는다(원칙 1). IPC+CLI
    /// 양면 노출(원칙 2: `tasty list gpu-stats`), 대상 지정 불필요한 전역 스냅샷이라
    /// 포커스 독립(원칙 3). local_only — plugin 미노출.
    fn ipc_handle_system_gpu_stats(&self, cmd: &IpcCommand) -> IpcStep {
        let response_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let windows: Vec<_> = self
            .view
            .views
            .iter()
            .map(|(id, w)| {
                serde_json::json!({
                    "window_id": u64::from(*id),
                    "main": w.as_main().is_some(),
                    "stats": w.base().gpu.resource_stats(),
                    // 호스트 view store(explorer)의 view 수 — GPU 가 아니라 창의 AppState 에
                    // 있다. main 이 아닌 창은 그 store 가 없어 null 이다.
                    "explorer_views": w.as_main().map(|m| m.state.explorer_views.view_count()),
                })
            })
            .collect();
        // RegistryReport 를 JSON 으로 — 타입 경로(wgpu-core 재수출)에 의존하지 않도록
        // 필드 접근만 하는 macro 로 변환한다. `num_allocated` 가 live 카운트.
        macro_rules! reg {
            ($r:expr) => {
                serde_json::json!({
                    "allocated": $r.num_allocated,
                    "kept_from_user": $r.num_kept_from_user,
                    "released_from_user": $r.num_released_from_user,
                })
            };
        }
        let wgpu_report = self.gpu_instance.generate_report().map(|r| {
            serde_json::json!({
                "surfaces": reg!(r.surfaces),
                "hub": {
                    "devices": reg!(r.hub.devices),
                    "queues": reg!(r.hub.queues),
                    "buffers": reg!(r.hub.buffers),
                    "textures": reg!(r.hub.textures),
                    "texture_views": reg!(r.hub.texture_views),
                    "samplers": reg!(r.hub.samplers),
                    "bind_groups": reg!(r.hub.bind_groups),
                    "bind_group_layouts": reg!(r.hub.bind_group_layouts),
                    "pipeline_layouts": reg!(r.hub.pipeline_layouts),
                    "shader_modules": reg!(r.hub.shader_modules),
                    "render_pipelines": reg!(r.hub.render_pipelines),
                    "compute_pipelines": reg!(r.hub.compute_pipelines),
                    "command_buffers": reg!(r.hub.command_buffers),
                    "render_bundles": reg!(r.hub.render_bundles),
                    "query_sets": reg!(r.hub.query_sets),
                },
            })
        });
        let response = host_ipc::protocol::JsonRpcResponse::success(
            response_id,
            serde_json::json!({
                "windows": windows,
                "wgpu": wgpu_report,
            }),
        );
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    /// `window.close` / `view.close`: main view 만 대상, 마지막 main 은 거부.
    /// CLAUDE.md "포커스 독립": id 로 직접 지정, focused 의존 금지.
    fn ipc_handle_window_close(&mut self, cmd: &IpcCommand) -> IpcStep {
        // 관문을 거친다 — 값이 왔는데 안 읽히는 것을 "안 왔다" 로 답하면 호출자가
        // 자기가 준 값을 안 의심한다.
        let target_id = params::read_int::<u64>(&cmd.request.params, "id");
        let response_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let response = match target_id {
            Err(msg) => host_ipc::protocol::JsonRpcResponse::error(response_id, -32602, &msg),
            Ok(None) => host_ipc::protocol::JsonRpcResponse::error(
                response_id,
                -32602,
                "Missing 'id' parameter (u64). focused 의존은 금지.",
            ),
            Ok(Some(id_u64)) => {
                // main view 만 대상 — `window.list` 가 노출하는 범위와 동일.
                // (modal/preset 은 사용자 조작 영역이라 IPC close 대상이 아님.)
                let mains: Vec<_> = self
                    .view
                    .views
                    .iter()
                    .filter(|(_, w)| w.as_main().is_some())
                    .map(|(id, _)| *id)
                    .collect();
                let target = mains.iter().copied().find(|w| u64::from(*w) == id_u64);
                match target {
                    Some(_) if mains.len() <= 1 => host_ipc::protocol::JsonRpcResponse::error(
                        response_id,
                        -32000,
                        "Cannot close the last main window via IPC — quitting the app is a user action",
                    ),
                    Some(tid) => {
                        // GUI request_close_window 와 공통 helper — window.closed
                        // plugin event + window.delete.post Lua fire 포함.
                        self.close_main_window(tid, tasty_plugin_protocol::LifecycleReason::Ipc);
                        host_ipc::protocol::JsonRpcResponse::success(
                            response_id,
                            serde_json::json!({"closed": true, "id": id_u64}),
                        )
                    }
                    None => host_ipc::protocol::JsonRpcResponse::error(
                        response_id,
                        -32602,
                        format!("Window id {id_u64} not found"),
                    ),
                }
            }
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    /// `window.focus` / `view.focus` (debug 전용): 사용자 입력 재현이라 release 미노출.
    #[cfg(debug_assertions)]
    fn ipc_handle_window_focus(&mut self, cmd: &IpcCommand) -> IpcStep {
        let target_id = params::read_int::<u64>(&cmd.request.params, "id");
        let response_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let response = match target_id {
            Err(msg) => host_ipc::protocol::JsonRpcResponse::error(response_id, -32602, &msg),
            Ok(None) => host_ipc::protocol::JsonRpcResponse::error(
                response_id,
                -32602,
                "Missing 'id' parameter (u64)",
            ),
            Ok(Some(id_u64)) => {
                let mut found = false;
                for (id, w) in &self.view.views {
                    if w.as_main().is_none() {
                        continue;
                    }
                    if u64::from(*id) == id_u64 {
                        w.base().winit.focus_window();
                        self.view.focused_view_id = Some(*id);
                        found = true;
                        break;
                    }
                }
                host_ipc::protocol::JsonRpcResponse::success(
                    response_id,
                    serde_json::json!({"focused": found, "id": id_u64}),
                )
            }
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    /// `window.list` / `view.list`: 전 view 순회, main view 만 노출.
    ///
    /// `layout_slot` 은 이 창이 점유한 레이아웃 슬롯 번호다 — "새 창을 열면 어떤
    /// 레이아웃이 뜰지" 를 결정하는 상태라, 관측 수단이 없으면 에이전트가 창 생성
    /// 결과를 예측·검증할 수 없다. headless engine 은 슬롯을 잡지 않으므로 `null`.
    ///
    /// **parked engine 은 여기 섞지 않는다.** 파킹된 engine 도 슬롯을 점유하지만
    /// 창이 아니라 창 id 가 없고, `{id, focused, title}` 계약이 깨진다. 점유 현황
    /// 전체(파킹 포함)를 봐야 할 일이 생기면 `layout.slots` 같은 별도 조회
    /// 메서드로 분리한다.
    fn ipc_handle_window_list(&self, cmd: &IpcCommand) -> IpcStep {
        let focused_id = self.view.focused_view_id;
        let list: Vec<_> = self
            .view
            .views
            .iter()
            .filter_map(|(id, w)| {
                let main = w.as_main()?;
                let mut info = host_ipc::handler::system_info_fields(&main.state, &main.core_state);
                info["id"] = serde_json::json!(u64::from(*id));
                info["focused"] = serde_json::json!(focused_id == Some(*id));
                info["title"] =
                    serde_json::json!(main.state.active_workspace(&main.core_state).name);
                Some(info)
            })
            .collect();
        let response = host_ipc::protocol::JsonRpcResponse::success(
            cmd.request.id.clone().unwrap_or(serde_json::Value::Null),
            serde_json::json!(list),
        );
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    /// `ui.screenshot` — 정식 release 기능. focus 독립(원칙 3): 대상 window/surface 를
    /// ID 로 직접 지정하고 focused_view_id 에 의존하지 않는다. 에이전트가 자기 작업을
    /// 관찰하는 캡처라 사용자 상태(focus/가시 탭)를 건드리지 않는다(원칙 1·2). local_only
    /// (파일 쓰기 표면) — plugin 미노출.
    ///
    /// params: { path (필수), surface_id? (u32), window_id? (u64) }
    /// - surface_id → 해당 터미널 surface 를 오프스크린 렌더로 캡처(가시성/포커스 무관).
    /// - 아니면 window 프레임 캡처: window_id 를 명시하면 **모달·preset 창을 포함한 모든
    ///   창**, 미지정이면 main 창이 정확히 하나일 때만, 그 외는 에러. 두 갈래가 비대칭인
    ///   근거는 [`resolve_screenshot_window`] 문서.
    fn ipc_handle_ui_screenshot(&mut self, cmd: &IpcCommand) -> IpcStep {
        let response_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let path = match cmd.request.params.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => {
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::error(
                        response_id,
                        -32602,
                        "Missing 'path' parameter (string)",
                    ),
                );
                return IpcStep::Handled;
            }
        };
        let surface_id = match crate::adapters::ipc::handler::params::read_u32(
            &cmd.request.params,
            "surface_id",
        ) {
            Ok(v) => v,
            Err(msg) => {
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::invalid_params(response_id, msg),
                );
                return IpcStep::Handled;
            }
        };
        let window_id = match params::read_int::<u64>(&cmd.request.params, "window_id") {
            Ok(v) => v,
            Err(msg) => {
                send_response(
                    &cmd.response_tx,
                    host_ipc::protocol::JsonRpcResponse::error(response_id, -32602, &msg),
                );
                return IpcStep::Handled;
            }
        };

        // ── surface 지정 오프스크린 캡처 (focus 독립) ──
        if let Some(sid) = surface_id {
            return self.ipc_screenshot_surface(cmd, response_id, &path, sid);
        }

        // ── window(tasty 자체 화면) 캡처 (focus 독립) ──
        let ids: Vec<_> = self.view.views.keys().copied().collect();
        let kinds: Vec<(u64, bool)> = ids
            .iter()
            .map(|id| {
                let is_main = self
                    .view
                    .views
                    .get(id)
                    .is_some_and(|w| w.as_main().is_some());
                (u64::from(*id), is_main)
            })
            .collect();
        let response = match resolve_screenshot_window(&kinds, window_id) {
            Ok(pos) => {
                let tid = ids[pos];
                match self.view.views.get_mut(&tid) {
                    Some(w) => {
                        let base = w.base_mut();
                        base.gpu.pending_screenshot = Some(std::path::PathBuf::from(&path));
                        base.dirty = true;
                        base.winit.request_redraw();
                        host_ipc::protocol::JsonRpcResponse::success(
                            response_id,
                            serde_json::json!({
                                "path": path,
                                "window_id": u64::from(tid),
                                "scheduled": true,
                            }),
                        )
                    }
                    // `ids` 는 바로 위에서 같은 맵에서 뽑았으므로 도달하지 않는다.
                    None => host_ipc::protocol::JsonRpcResponse::error(
                        response_id,
                        -32603,
                        "Target window disappeared while resolving",
                    ),
                }
            }
            Err((code, msg)) => host_ipc::protocol::JsonRpcResponse::error(response_id, code, msg),
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }
    /// `clipboard.set_text` — 본체는 두 조합이 공유한다
    /// (`crate::core::app_surface`). 여기서는 응답을 회신만 한다.
    fn ipc_handle_clipboard_set_text(&mut self, cmd: &IpcCommand) -> IpcStep {
        let response_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let resp = crate::core::app_surface::clipboard_set_text(
            &self.core,
            response_id,
            &cmd.request.params,
        );
        send_response(&cmd.response_tx, resp);
        IpcStep::Handled
    }

    /// `ui.screenshot` 의 surface 지정 오프스크린 캡처 경로. 소유 window 를 ID 로 순회
    /// 해소(focus 무관)하고 terminal surface 만 캡처 스케줄한다.
    fn ipc_screenshot_surface(
        &mut self,
        cmd: &IpcCommand,
        response_id: serde_json::Value,
        path: &str,
        sid: u32,
    ) -> IpcStep {
        // 소유 window(창별 CoreState)를 ID 로 순회 해소 — focus 무관.
        let owner = self.view.views.values_mut().find_map(|w| {
            let m = w.as_main_mut()?;
            if m.core_state.has_surface(sid) {
                Some(m)
            } else {
                None
            }
        });
        let response = match owner {
            None => host_ipc::protocol::JsonRpcResponse::error(
                response_id,
                -32602,
                format!("Surface {sid} not found"),
            ),
            Some(m) => {
                let kind = m.core_state.find_surface_by_id(sid).map(|s| s.kind());
                if kind == Some("terminal") {
                    m.base.gpu.pending_surface_screenshot =
                        Some((sid, std::path::PathBuf::from(path)));
                    m.base.dirty = true;
                    m.base.winit.request_redraw();
                    host_ipc::protocol::JsonRpcResponse::success(
                        response_id,
                        serde_json::json!({
                            "path": path,
                            "surface_id": sid,
                            "scheduled": true,
                        }),
                    )
                } else {
                    host_ipc::protocol::JsonRpcResponse::error(
                        response_id,
                        -32000,
                        format!(
                            "Surface {sid} is kind '{}' — only terminal surfaces can be captured (egui panels / plugin / webview are out of scope)",
                            kind.unwrap_or("unknown")
                        ),
                    )
                }
            }
        };
        send_response(&cmd.response_tx, response);
        IpcStep::Handled
    }

    /// `plugin.*` 메서드 한 묶음. 라이프사이클 변경 메서드는 HandledDirty 반환.
    fn ipc_dispatch_plugin_method(
        &mut self,
        cmd: &IpcCommand,
        caller: &host_ipc::caller::CallerContext,
    ) -> IpcStep {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        // 읽기 전용 조회는 헤드리스와 **공유하는 한 함수**가 답한다. 여기에 같은 표를
        // 다시 두면 한쪽만 고쳐지는 순간 갈라지므로, 라우팅 표는 그 함수에만 있다
        // (`crate::adapters::ipc::handler::plugin::dispatch_readonly`).
        let surface_registry = self.core_state().surface_registry.clone();
        if let Some(response) = host_ipc::handler::plugin::dispatch_readonly(
            &self.core,
            self.plugin_manager.as_ref(),
            &surface_registry,
            cmd.request.method.as_str(),
            id.clone(),
            &cmd.request.params,
        ) {
            send_response(&cmd.response_tx, response);
            return IpcStep::Handled;
        }
        let response = match cmd.request.method.as_str() {
            "plugin.install" => {
                let path = match cmd.request.params.get("path").and_then(|v| v.as_str()) {
                    Some(p) => std::path::PathBuf::from(p),
                    None => {
                        send_response(
                            &cmd.response_tx,
                            host_ipc::protocol::JsonRpcResponse::invalid_params(
                                id,
                                "Missing 'path' parameter",
                            ),
                        );
                        return IpcStep::Handled;
                    }
                };
                match self.plugin_install(path) {
                    Ok(events) => {
                        // 첫 event 의 plugin_id 가 응답 페이로드용 (Installed CoreEvent).
                        let installed_id = events
                            .iter()
                            .find_map(|ev| match ev {
                                crate::core::intent::CoreEvent::PluginRegistryChanged {
                                    plugin_id,
                                    ..
                                } => Some(plugin_id.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        self.cascade_plugin_events(events);
                        host_ipc::protocol::JsonRpcResponse::success(
                            id,
                            serde_json::json!({ "installed": installed_id }),
                        )
                    }
                    Err(e) => host_ipc::protocol::JsonRpcResponse::error(id, -32000, e.to_string()),
                }
            }
            "plugin.remove" => {
                let plugin_id = match cmd.request.params.get("id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => {
                        send_response(
                            &cmd.response_tx,
                            host_ipc::protocol::JsonRpcResponse::invalid_params(
                                id,
                                "Missing 'id' parameter",
                            ),
                        );
                        return IpcStep::Handled;
                    }
                };
                let pid_for_response = plugin_id.clone();
                match self.plugin_remove(plugin_id) {
                    Ok(events) => {
                        self.cascade_plugin_events(events);
                        host_ipc::protocol::JsonRpcResponse::success(
                            id,
                            serde_json::json!({ "removed": pid_for_response }),
                        )
                    }
                    Err(e) => host_ipc::protocol::JsonRpcResponse::error(id, -32000, e.to_string()),
                }
            }
            // 두 조합이 **같은 함수**를 부른다 — 헤드리스 pump 도 이것을 부른다
            // (`src/boot/headless_dispatch.rs`). 여기 남는 차이는 낸 이벤트의
            // 소비처뿐이다: gui 는 창 큐로 cascade 하고 헤드리스는 그 자리에서 낸다.
            "plugin.enable" | "plugin.disable" => {
                let Some((response, events)) = host_ipc::handler::plugin::dispatch_lifecycle_toggle(
                    self.plugin_manager.as_mut(),
                    cmd.request.method.as_str(),
                    id,
                    &cmd.request.params,
                ) else {
                    // 위 arm 이 이름을 못 걸러 냈다는 뜻 — 표와 arm 이 갈렸다.
                    unreachable!("plugin.enable/disable 이 토글 표에 없다");
                };
                self.cascade_plugin_events(events);
                response
            }
            "plugin.grant" => {
                let plugin_id = match cmd.request.params.get("id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => {
                        send_response(
                            &cmd.response_tx,
                            host_ipc::protocol::JsonRpcResponse::invalid_params(
                                id,
                                "Missing 'id' parameter",
                            ),
                        );
                        return IpcStep::Handled;
                    }
                };
                let token = match cmd
                    .request
                    .params
                    .get("permission")
                    .and_then(|v| v.as_str())
                {
                    Some(s) => s.to_string(),
                    None => {
                        send_response(
                            &cmd.response_tx,
                            host_ipc::protocol::JsonRpcResponse::invalid_params(
                                id,
                                "Missing 'permission' parameter",
                            ),
                        );
                        return IpcStep::Handled;
                    }
                };
                let pid_for_response = plugin_id.clone();
                let perm_for_response = token.clone();
                match self.plugin_grant(plugin_id, token) {
                    Ok(events) => {
                        self.cascade_plugin_events(events);
                        host_ipc::protocol::JsonRpcResponse::success(
                            id,
                            serde_json::json!({
                                "id": pid_for_response,
                                "permission": perm_for_response,
                            }),
                        )
                    }
                    Err(e) => host_ipc::protocol::JsonRpcResponse::error(id, -32000, e.to_string()),
                }
            }
            "plugin.revoke" => {
                let plugin_id = match cmd.request.params.get("id").and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => {
                        send_response(
                            &cmd.response_tx,
                            host_ipc::protocol::JsonRpcResponse::invalid_params(
                                id,
                                "Missing 'id' parameter",
                            ),
                        );
                        return IpcStep::Handled;
                    }
                };
                let token = match cmd
                    .request
                    .params
                    .get("permission")
                    .and_then(|v| v.as_str())
                {
                    Some(s) => s.to_string(),
                    None => {
                        send_response(
                            &cmd.response_tx,
                            host_ipc::protocol::JsonRpcResponse::invalid_params(
                                id,
                                "Missing 'permission' parameter",
                            ),
                        );
                        return IpcStep::Handled;
                    }
                };
                let pid_for_response = plugin_id.clone();
                let perm_for_response = token.clone();
                match self.plugin_revoke(plugin_id, token) {
                    Ok(events) => {
                        self.cascade_plugin_events(events);
                        host_ipc::protocol::JsonRpcResponse::success(
                            id,
                            serde_json::json!({
                                "id": pid_for_response,
                                "permission": perm_for_response,
                            }),
                        )
                    }
                    Err(e) => host_ipc::protocol::JsonRpcResponse::error(id, -32000, e.to_string()),
                }
            }
            "plugin.grant_agent_permission" => {
                host_ipc::handler::session::handle_grant_agent_permission(
                    &self.core,
                    id,
                    &cmd.request.params,
                )
            }
            "plugin.revoke_agent_permission" => {
                host_ipc::handler::session::handle_revoke_agent_permission(
                    &self.core,
                    id,
                    &cmd.request.params,
                )
            }
            "plugin.upgrade_builtins" => {
                let force = cmd
                    .request
                    .params
                    .get("force")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let restore_removed: Vec<String> = cmd
                    .request
                    .params
                    .get("restore_removed")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let restore_all = cmd
                    .request
                    .params
                    .get("restore_all")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let restart_running = cmd
                    .request
                    .params
                    .get("restart_running")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                match self.plugin_upgrade_builtins(
                    force,
                    restore_removed,
                    restore_all,
                    restart_running,
                ) {
                    Ok((report, events)) => {
                        self.cascade_plugin_events(events);
                        match serde_json::to_value(&report) {
                            Ok(v) => host_ipc::protocol::JsonRpcResponse::success(id, v),
                            Err(e) => host_ipc::protocol::JsonRpcResponse::error(
                                id,
                                -32603,
                                format!("serialize report failed: {e}"),
                            ),
                        }
                    }
                    Err(e) => host_ipc::protocol::JsonRpcResponse::error(id, -32000, e.to_string()),
                }
            }
            "plugin.audit_follow" => {
                host_ipc::handler::audit::handle_follow(&self.core, id, &cmd.request.params)
            }
            "plugin.audit_clear" => {
                host_ipc::handler::audit::handle_clear(&self.core, id, &cmd.request.params)
            }
            "plugin.request_permission" => {
                // 첫 main window 의 state 를 빌려 사용 (모든 window 가 같은 approval_store
                // Arc 공유). main 이 하나도 없으면 elevation popup 표시 자체가 의미 없으므로
                // internal_error.
                //
                // **공유가 어디서 만들어지는지를 함께 적는다.** `CoreState` 의 생성자는
                // engine 마다 `approval_store: Arc::new(...)` 로 새로 만들고, 두 번째
                // main window 를 세울 때 `App::ensure_engine_and_plugins`
                // (`app/window_lifecycle.rs`)가 첫 engine 의 Arc 로 **덮어쓴다**
                // (`any_main_engine` 이 넘겨주는 아홉 중 하나). 그래서 "공유" 는
                // 생성자만 읽으면 거짓으로 보이고 창 생성 경로까지 읽어야 참이다 —
                // 좌표를 안 적어 두면 다음 사람이 생성자에서 멈춘다.
                let core = &mut self.core;
                let main = self.view.views.values_mut().find_map(|w| w.as_main_mut());
                match main {
                    Some(m) => host_ipc::handler::session::handle_request_permission(
                        core,
                        &mut m.state,
                        &mut m.core_state,
                        caller,
                        id,
                        &cmd.request.params,
                    ),
                    None => host_ipc::protocol::JsonRpcResponse::error(
                        id,
                        -32603,
                        "no main window available for elevation popup",
                    ),
                }
            }
            other => host_ipc::protocol::JsonRpcResponse::method_not_found(id, other),
        };
        let dirty = matches!(
            cmd.request.method.as_str(),
            "plugin.install"
                | "plugin.remove"
                | "plugin.enable"
                | "plugin.disable"
                | "plugin.grant"
                | "plugin.revoke"
                | "plugin.upgrade_builtins"
        );
        send_response(&cmd.response_tx, response);
        if dirty {
            IpcStep::HandledDirty
        } else {
            IpcStep::Handled
        }
    }
    /// `events.fetch`: `wait_ms` 가 있으면 블로킹이라 **워커로 내보낸다** — 프레임
    /// 루프에서 기다리면 그만큼 창이 멈춘다(`agent.task_await` 와 같은 이유).
    /// 버스는 `Clone` 이고 안이 `Arc` 라 사본이 같은 링을 본다.
    fn ipc_dispatch_events_fetch(&mut self, cmd: &IpcCommand) {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let Some(mgr) = self.plugin_manager.as_ref() else {
            send_response(&cmd.response_tx, crate::ipc::handler::events::no_bus(id));
            return;
        };
        let bus = mgr.event_bus.clone();
        let args = match crate::ipc::handler::events::FetchParams::parse(&cmd.request.params, &id) {
            Ok(a) => a,
            Err(resp) => {
                send_response(&cmd.response_tx, resp);
                return;
            }
        };
        if args.wait.is_zero() {
            send_response(
                &cmd.response_tx,
                crate::ipc::handler::events::fetch(&bus, &args, id),
            );
            return;
        }
        let response_tx = cmd.response_tx.clone();
        std::thread::spawn(move || {
            let resp = crate::ipc::handler::events::fetch(&bus, &args, id);
            send_response(&response_tx, resp);
        });
    }

    /// `agent.task_await`: 블로킹. **어느 engine 의 허브인지 고르는 것만** 여기 있다 —
    /// 창 → parked → (헤드리스 전용) 단일 engine 순으로 훑는다. 고른 뒤의 대기는 두
    /// 조합이 공유한다(`crate::ipc::handler::agent::task::spawn_task_await`).
    fn ipc_dispatch_task_await(&mut self, cmd: &IpcCommand) {
        let hub_opt = self
            .view
            .views
            .values()
            .find_map(|w| w.as_main().map(|w| w.core_state.task_waker_hub.clone()))
            .or_else(|| {
                self.parked_states
                    .first()
                    .map(|(_, e)| e.task_waker_hub.clone())
            })
            .or_else(|| self.core_state.as_ref().map(|e| e.task_waker_hub.clone()));
        let seq_opt = self
            .view
            .views
            .values()
            .find_map(|w| w.as_main().map(|w| w.core_state.agent_seq.clone()))
            .or_else(|| self.parked_states.first().map(|(_, e)| e.agent_seq.clone()))
            .or_else(|| self.core_state.as_ref().map(|e| e.agent_seq.clone()));
        let memory = self.core.memory_arc();
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        match (hub_opt, seq_opt) {
            (Some(hub), Some(seq)) => crate::ipc::handler::agent::task::spawn_task_await(
                hub,
                memory,
                seq,
                rpc_id,
                cmd.request.params.clone(),
                &cmd.response_tx,
            ),
            _ => send_response(
                &cmd.response_tx,
                crate::core::app_surface::no_application_state(rpc_id),
            ),
        }
    }
    /// `approval.await`: 블로킹. store 선택만 여기 있고 대기는 공유한다
    /// (`ipc_dispatch_task_await` 와 같은 구조).
    ///
    /// **어느 engine 을 고르든 같은 store 다** — 창 생성 경로가 첫 engine 의
    /// `approval_store` Arc 를 넘겨준다(위 `plugin.request_permission` 의 주석).
    /// 그래서 이 선택은 대기 대상을 가르지 않는다.
    fn ipc_dispatch_approval_await(&mut self, cmd: &IpcCommand) {
        let store_opt = self
            .view
            .views
            .values()
            .find_map(|w| w.as_main().map(|w| w.core_state.approval_store.clone()))
            .or_else(|| {
                self.parked_states
                    .first()
                    .map(|(_, e)| e.approval_store.clone())
            })
            .or_else(|| self.core_state.as_ref().map(|e| e.approval_store.clone()));
        let memory = self.core.memory_arc();
        let rpc_id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        match store_opt {
            Some(store) => crate::ipc::handler::approval::spawn_approval_await(
                store,
                memory,
                rpc_id,
                cmd.request.params.clone(),
                &cmd.response_tx,
            ),
            None => send_response(
                &cmd.response_tx,
                crate::core::app_surface::no_application_state(rpc_id),
            ),
        }
    }
}

/// `ui.screenshot` 의 대상 창 결정.
///
/// `windows` 는 `(window id, main view 인가)` 를 창 하나당 하나씩 담는다. 성공하면
/// 그 슬라이스의 **인덱스**를 돌려준다(호출자가 같은 순서의 `WindowId` 배열에서
/// 되찾을 수 있게 — u64 로 다시 찾는 왕복을 없앤다). 실패는 `(JSON-RPC code, message)`.
///
/// 두 갈래가 의도적으로 비대칭이다.
///
/// - **`window_id` 를 명시하면 모든 창이 대상이다** — 설정·플러그인·종료 확인 모달과
///   preset 창을 포함한다. 캡처는 이미 그려진 프레임의 readback 이라 사용자 상태
///   (포커스 / 선택 / 스크롤 / 커서)를 바꾸지 않으므로 원칙 1 의 금지 대상이 아니고,
///   `ui.screenshot` 은 `local_only`(plugin 미노출)라 호출자는 이미 사용자 권한으로
///   `config.toml` 과 PTY 를 읽을 수 있다. 근거 전체는
///   `docs/ai-verification/screenshot-methods.md` "무엇을 캡처할 수 있는가".
/// - **미지정 시의 자동 선택은 main 창이 정확히 하나일 때뿐이다** — 모달로는 절대
///   폴백하지 않고 포커스도 보지 않는다(원칙 3). "지금 보고 있는 창" 에 기대는 순간
///   같은 명령이 사용자 상태에 따라 다른 것을 캡처한다.
///
/// 캡처가 **행동** 대상 집합을 넓히는 것은 아니다 — `window.list` 와 `window.close` 는
/// 종전대로 main 창만 다룬다(모달·preset 은 사용자 조작 영역).
///
/// 결정의 근거·기각 대안·재검토 조건:
/// `docs/adr/0118-screenshot-reads-any-window-explicit-id-only.md`.
fn resolve_screenshot_window(
    windows: &[(u64, bool)],
    requested: Option<u64>,
) -> Result<usize, (i32, String)> {
    match requested {
        Some(wid) => windows
            .iter()
            .position(|(id, _)| *id == wid)
            .ok_or_else(|| (-32602, format!("Window id {wid} not found"))),
        None => {
            let mains: Vec<usize> = windows
                .iter()
                .enumerate()
                .filter(|(_, (_, is_main))| *is_main)
                .map(|(i, _)| i)
                .collect();
            match mains.len() {
                1 => Ok(mains[0]),
                0 => Err((-32000, "No main window open; specify 'window_id'".to_string())),
                _ => Err((
                    -32000,
                    "Multiple windows open; specify 'window_id' (focus-independent). Use 'window.list' to enumerate.".to_string(),
                )),
            }
        }
    }
}

#[cfg(test)]
mod screenshot_target_tests {
    use super::resolve_screenshot_window;

    /// main 2 개(10, 11) + 설정 모달(20) + 종료 확인 모달(21).
    const MIXED: &[(u64, bool)] = &[(10, true), (20, false), (11, true), (21, false)];
    /// main 1 개(10) + 설정 모달(20).
    const ONE_MAIN: &[(u64, bool)] = &[(10, true), (20, false)];

    #[test]
    fn an_explicit_id_can_name_a_modal_window() {
        // 이것이 이 함수의 존재 이유다 — 되돌리면(모달을 후보에서 빼면) 여기서 깨진다.
        assert_eq!(resolve_screenshot_window(MIXED, Some(20)), Ok(1));
        assert_eq!(resolve_screenshot_window(MIXED, Some(21)), Ok(3));
        assert_eq!(resolve_screenshot_window(ONE_MAIN, Some(20)), Ok(1));
    }

    #[test]
    fn an_explicit_id_still_names_a_main_window() {
        assert_eq!(resolve_screenshot_window(MIXED, Some(10)), Ok(0));
        assert_eq!(resolve_screenshot_window(MIXED, Some(11)), Ok(2));
    }

    #[test]
    fn an_unknown_id_is_a_parameter_error_naming_the_id() {
        let (code, msg) = resolve_screenshot_window(MIXED, Some(999)).unwrap_err();
        assert_eq!(code, -32602);
        assert!(
            msg.contains("999"),
            "메시지가 문제의 id 를 담아야 한다: {msg}"
        );
    }

    #[test]
    fn without_an_id_a_lone_main_window_is_picked_and_the_modal_is_not() {
        assert_eq!(resolve_screenshot_window(ONE_MAIN, None), Ok(0));
    }

    #[test]
    fn without_an_id_a_modal_is_never_the_automatic_target() {
        // main 이 없고 모달만 떠 있어도 "그거라도 찍는다" 로 가지 않는다 — 자동 선택이
        // 사용자가 무엇을 열어뒀는지에 의존하기 시작하면 원칙 3 이 깨진다.
        let only_modals: &[(u64, bool)] = &[(20, false), (21, false)];
        let (code, _) = resolve_screenshot_window(only_modals, None).unwrap_err();
        assert_eq!(code, -32000);
    }

    #[test]
    fn without_an_id_multiple_mains_is_an_error_not_a_focus_fallback() {
        let (code, msg) = resolve_screenshot_window(MIXED, None).unwrap_err();
        assert_eq!(code, -32000);
        assert!(
            msg.contains("window_id"),
            "무엇을 하라는 것인지 메시지가 말해야 한다: {msg}"
        );
    }

    #[test]
    fn no_window_at_all_is_an_error() {
        let (code, _) = resolve_screenshot_window(&[], None).unwrap_err();
        assert_eq!(code, -32000);
        let (code, _) = resolve_screenshot_window(&[], Some(10)).unwrap_err();
        assert_eq!(code, -32602);
    }
}
