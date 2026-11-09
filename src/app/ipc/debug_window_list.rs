//! `debug.window.list` — 메인 창과 보조 창(설정·Preset·Plugins·종료 확인)을 모두 나열한다.
//! release `window.list` 는 메인 창만 다룬다(docs/ai-verification/screenshot-methods.md). 보조 창 ID 는 그 창에 입력을 주입하는
//! debug 경로에만 필요하므로 이 목록도 debug 빌드에만 둔다.

use crate::app::App;
use crate::app::ipc::IpcStep;
use crate::ipc::protocol::JsonRpcResponse;
use crate::ipc::server::{IpcCommand, send_response};
use crate::view::ui::View;

impl App {
    pub(super) fn ipc_handle_debug_window_list(&self, cmd: &IpcCommand) -> IpcStep {
        let id = cmd.request.id.clone().unwrap_or(serde_json::Value::Null);
        let focused = self.view.focused_view_id;
        let modal = self.view.active_modal_id();
        let mut windows: Vec<_> = self
            .view
            .views
            .iter()
            .map(|(wid, view)| {
                let winit = &view.base().winit;
                let size = winit.inner_size();
                // winit 은 X11·Wayland 에서 제목을 돌려주지 않는다(빈 문자열). 그때는 null 이다.
                let title = Some(winit.title()).filter(|t| !t.is_empty());
                // 창 내용 영역의 화면 좌표(물리 px). OS 수준 입력으로 창 안을 누르는 시험이 쓴다 — native WebView 는
                // egui · 터미널 주입 경로로 닿지 않는다. 플랫폼이 알려 주지 않으면(Wayland) null 이다.
                let position = winit
                    .inner_position()
                    .ok()
                    .map(|p| serde_json::json!({ "x": p.x, "y": p.y }));
                serde_json::json!({
                    "window_id": u64::from(*wid),
                    "kind": view_kind(view.as_ref()),
                    "title": title,
                    "inner_size": { "width": size.width, "height": size.height },
                    "inner_position": position,
                    "scale_factor": winit.scale_factor(),
                    "focused": focused == Some(*wid),
                    "modal": modal == Some(*wid),
                })
            })
            .collect();
        windows.sort_by_key(|w| w["window_id"].as_u64());
        send_response(
            &cmd.response_tx,
            JsonRpcResponse::success(id, serde_json::json!({ "windows": windows })),
        );
        IpcStep::Handled
    }
}

/// 창 종류 이름. `debug.toast` 응답의 `view` 와 같은 이름을 쓴다.
fn view_kind(view: &dyn View) -> &'static str {
    let any = view.as_any();
    if any.is::<crate::view::main::MainView>() {
        "main"
    } else if any.is::<crate::view::settings::SettingsView>() {
        "settings"
    } else if any.is::<crate::view::preset::PresetView>() {
        "preset"
    } else if any.is::<crate::view::plugins::PluginsView>() {
        "plugins"
    } else if any.is::<crate::view::quit::QuitView>() {
        "quit"
    } else {
        "unknown"
    }
}
