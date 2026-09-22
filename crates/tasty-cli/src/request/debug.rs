//! `tasty debug ...` CLI → JsonRpcRequest 매핑 (debug + popup + tool + extension + event_bus).

#![cfg(debug_assertions)]

use crate::commands::{DebugCommands, EventBusCommands};

use super::resolve_surface_id;

pub(super) fn debug_command_to_method_params(
    command: &DebugCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        // 로컬 attach 의 non-force 경로는 run_client 에서 raw 스트림으로 선처리된다.
        // 여기 도달하는 건 `--force-detach` 뿐 → workspace 면 force_detach_workspace,
        // 아니면 surface force_detach IPC.
        DebugCommands::Attach {
            surface,
            workspace,
            force_detach,
            ..
        } => {
            debug_assert!(
                *force_detach,
                "non-force debug attach is dispatched before request mapping"
            );
            if let Some(ws) = workspace {
                (
                    "attach.force_detach_workspace",
                    serde_json::json!({ "workspace_id": ws }),
                )
            } else {
                (
                    "attach.force_detach",
                    serde_json::json!({ "surface_id": surface }),
                )
            }
        }
        DebugCommands::Info => ("debug.info", serde_json::json!({})),
        DebugCommands::GpuStall { ms } => ("debug.gpu.stall", serde_json::json!({ "ms": ms })),
        DebugCommands::ImeEnable => ("surface.ime_enable", serde_json::json!({})),
        DebugCommands::ImeDisable => ("surface.ime_disable", serde_json::json!({})),
        DebugCommands::ImePreedit { text, cursor } => (
            "surface.ime_preedit",
            serde_json::json!({ "text": text, "cursor": cursor }),
        ),
        DebugCommands::ImeCommit { text } => {
            ("surface.ime_commit", serde_json::json!({ "text": text }))
        }
        DebugCommands::ImeStatus => ("surface.ime_status", serde_json::json!({})),
        DebugCommands::CellInfo { row, col, surface } => (
            "debug.cell_info",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "row": row,
                "col": col,
            }),
        ),
        DebugCommands::ScreenAttrs { row, surface } => (
            "debug.screen_attrs",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "row": row,
            }),
        ),
        DebugCommands::GlyphColor {
            row,
            col,
            surface,
            bg_mode,
        } => (
            "debug.glyph_color",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "row": row,
                "col": col,
                "bg_mode": bg_mode,
            }),
        ),
        DebugCommands::SwitchInputSource { source_id } => (
            "surface.switch_input_source",
            serde_json::json!({ "source_id": source_id }),
        ),
        DebugCommands::RawKey { keycode } => {
            ("surface.raw_key", serde_json::json!({ "keycode": keycode }))
        }
        DebugCommands::EventBus(sub) => event_bus_command_to_method_params(sub),
        DebugCommands::Extension(sub) => extension_debug_command_to_method_params(sub),
        DebugCommands::Tool(sub) => tool_debug_command_to_method_params(sub),
        DebugCommands::Popup(sub) => popup_debug_command_to_method_params(sub),
        DebugCommands::HostPopup(sub) => host_popup_debug_command_to_method_params(sub),
        DebugCommands::Fullscreen(sub) => fullscreen_debug_command_to_method_params(sub),
        DebugCommands::ModifierHint(sub) => modifier_hint_debug_command_to_method_params(sub),
        DebugCommands::Banner(sub) => banner_debug_command_to_method_params(sub),
        DebugCommands::Settings(sub) => settings_debug_command_to_method_params(sub),
        DebugCommands::Modal(sub) => modal_debug_command_to_method_params(sub),
        DebugCommands::Lua(sub) => lua_debug_command_to_method_params(sub),
        DebugCommands::FocusedSurface => ("debug.focused_surface", serde_json::json!({})),
        DebugCommands::Selection => ("debug.selection", serde_json::json!({})),
        DebugCommands::PendingMenu => ("debug.pending_menu", serde_json::json!({})),
        DebugCommands::UiState => ("ui.state", serde_json::json!({})),
        DebugCommands::SwitchWorkspace { index } => (
            "debug.switch_workspace",
            serde_json::json!({ "index": index }),
        ),
        DebugCommands::SwitchTab { index } => {
            ("debug.switch_tab", serde_json::json!({ "index": index }))
        }
        DebugCommands::CloseWorkspace { index } => (
            "debug.close_workspace",
            serde_json::json!({ "index": index }),
        ),
        DebugCommands::FeedBytes {
            surface,
            text,
            bytes,
        } => (
            "debug.feed_bytes",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "text": text,
                "bytes": bytes,
            }),
        ),
        DebugCommands::Inject(sub) => inject_debug_command_to_method_params(sub),
        DebugCommands::PluginBanner(sub) => plugin_banner_debug_command_to_method_params(sub),
        // stream-echo is a raw framed exchange, not a JSON-RPC request — it is
        // handled directly in `run_client` before request mapping is reached.
        DebugCommands::StreamEcho { .. } => {
            unreachable!("debug stream-echo is dispatched before request mapping")
        }
        // sim emits raw VTE locally — handled directly in `run_client` before
        // request mapping is reached.
        DebugCommands::Sim { .. } => {
            unreachable!("debug sim is dispatched before request mapping")
        }
    }
}

#[cfg(debug_assertions)]
pub(super) fn popup_debug_command_to_method_params(
    command: &crate::PopupDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::PopupDebugCommands;
    match command {
        PopupDebugCommands::List => ("debug.popup.list", serde_json::json!({})),
        PopupDebugCommands::Open {
            plugin_id,
            popup_id,
            context,
        } => {
            let ctx_value: serde_json::Value = match context {
                Some(s) => serde_json::from_str(s).unwrap_or(serde_json::Value::Null),
                None => serde_json::Value::Null,
            };
            (
                "debug.popup.open",
                serde_json::json!({
                    "plugin_id": plugin_id,
                    "popup_id": popup_id,
                    "context": ctx_value,
                }),
            )
        }
        PopupDebugCommands::Close { instance_id } => (
            "debug.popup.close",
            serde_json::json!({ "instance_id": instance_id }),
        ),
    }
}

#[cfg(debug_assertions)]
pub(super) fn host_popup_debug_command_to_method_params(
    command: &crate::HostPopupDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::HostPopupDebugCommands;
    match command {
        HostPopupDebugCommands::List => ("debug.host_popup.list", serde_json::json!({})),
        HostPopupDebugCommands::Open {
            popup_id,
            workspace_scope,
            surface_scope,
        } => (
            "debug.host_popup.open",
            serde_json::json!({
                "popup_id": popup_id,
                "workspace_scope": workspace_scope,
                "surface_scope": surface_scope,
            }),
        ),
        HostPopupDebugCommands::Close { popup_id } => (
            "debug.host_popup.close",
            serde_json::json!({ "popup_id": popup_id }),
        ),
    }
}

#[cfg(debug_assertions)]
pub(super) fn fullscreen_debug_command_to_method_params(
    command: &crate::FullscreenDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::FullscreenDebugCommands;
    match command {
        FullscreenDebugCommands::List => ("debug.fullscreen.list", serde_json::json!({})),
        FullscreenDebugCommands::Open {
            stage_id,
            window_id,
        } => (
            "debug.fullscreen.open",
            serde_json::json!({ "stage_id": stage_id, "window_id": window_id }),
        ),
        FullscreenDebugCommands::Close { window_id } => (
            "debug.fullscreen.close",
            serde_json::json!({ "window_id": window_id }),
        ),
        FullscreenDebugCommands::State { window_id } => (
            "debug.fullscreen.state",
            serde_json::json!({ "window_id": window_id }),
        ),
    }
}

#[cfg(debug_assertions)]
pub(super) fn modifier_hint_debug_command_to_method_params(
    command: &crate::ModifierHintDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::ModifierHintDebugCommands;
    match command {
        ModifierHintDebugCommands::Hold {
            ctrl,
            alt,
            option,
            shift,
            elapsed_ms,
        } => (
            "debug.modifier_hint.hold",
            serde_json::json!({
                "ctrl": ctrl,
                "alt": alt,
                "option": option,
                "shift": shift,
                "elapsed_ms": elapsed_ms,
            }),
        ),
        ModifierHintDebugCommands::State => ("debug.modifier_hint.state", serde_json::json!({})),
    }
}

#[cfg(debug_assertions)]
pub(super) fn banner_debug_command_to_method_params(
    command: &crate::BannerDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::BannerDebugCommands;
    match command {
        BannerDebugCommands::List => ("debug.banner.list", serde_json::json!({})),
        BannerDebugCommands::Show { banner_id, scope } => (
            "debug.banner.show",
            serde_json::json!({ "banner_id": banner_id, "scope": scope }),
        ),
        BannerDebugCommands::Close { banner_id } => (
            "debug.banner.close",
            serde_json::json!({ "banner_id": banner_id }),
        ),
        BannerDebugCommands::SetCountdown { scope, seconds } => (
            "debug.banner.set_countdown",
            serde_json::json!({ "scope": scope, "seconds": seconds }),
        ),
    }
}

#[cfg(debug_assertions)]
pub(super) fn modal_debug_command_to_method_params(
    command: &crate::ModalDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::ModalDebugCommands;
    match command {
        ModalDebugCommands::CloseRequest => ("debug.modal.close_request", serde_json::json!({})),
    }
}

#[cfg(debug_assertions)]
pub(super) fn settings_debug_command_to_method_params(
    command: &crate::SettingsDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::SettingsDebugCommands;
    match command {
        SettingsDebugCommands::Open { tab, subtab } => (
            "debug.settings.open",
            serde_json::json!({ "tab": tab, "subtab": subtab }),
        ),
        // raw 문자열을 그대로 싣지 않고 CLI 단에서 1차 파싱해 Value object 로 넘긴다
        // (서버는 `params.get("settings")` 로 object 를 기대). 이 fn 은 Result 를
        // 반환하지 못하므로 파싱/파일 에러는 errln + exit(1) 로 처리한다
        // (normalize_cwd_arg 와 동일한 CLI 에러 선례).
        SettingsDebugCommands::Apply { json, file } => {
            let raw = match (file, json) {
                (Some(path), _) => std::fs::read_to_string(path).unwrap_or_else(|e| {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t_fmt2("cli.debug.file_read_failed", path, &e.to_string())
                    );
                    std::process::exit(1);
                }),
                (None, Some(s)) => s.clone(),
                (None, None) => {
                    crate::out::errln!(
                        "{}",
                        tasty_i18n::t("cli.debug.settings_apply_source_required")
                    );
                    std::process::exit(1);
                }
            };
            let patch: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|e| {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt("cli.debug.settings_patch_not_json", &e.to_string())
                );
                std::process::exit(1);
            });
            (
                "debug.settings.apply",
                serde_json::json!({ "settings": patch }),
            )
        }
    }
}

#[cfg(debug_assertions)]
pub(super) fn lua_debug_command_to_method_params(
    command: &crate::LuaDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::LuaDebugCommands;
    match command {
        LuaDebugCommands::Eval { source } => {
            ("debug.lua.eval", serde_json::json!({ "source": source }))
        }
        // 파일은 CLI 단에서 읽어 source 로 넘긴다(Apply --file 선례). 이 fn 은 Result 를
        // 반환하지 못하므로 읽기 실패는 errln + exit(1) 로 처리한다.
        LuaDebugCommands::EvalFile { path } => {
            let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
                crate::out::errln!(
                    "{}",
                    tasty_i18n::t_fmt2("cli.debug.lua_eval_file_failed", path, &e.to_string())
                );
                std::process::exit(1);
            });
            ("debug.lua.eval", serde_json::json!({ "source": source }))
        }
    }
}

#[cfg(debug_assertions)]
pub(super) fn tool_debug_command_to_method_params(
    command: &crate::ToolDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::ToolDebugCommands;
    match command {
        ToolDebugCommands::List => ("debug.tool.list", serde_json::json!({})),
        ToolDebugCommands::Invoke { key } => {
            ("debug.tool.invoke", serde_json::json!({ "key": key }))
        }
    }
}

#[cfg(debug_assertions)]
pub(super) fn extension_debug_command_to_method_params(
    command: &crate::ExtensionDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::ExtensionDebugCommands;
    match command {
        ExtensionDebugCommands::InvokeHook {
            extension_id,
            kind,
            phase,
            mode,
            target,
            payload,
        } => {
            let parsed_payload: serde_json::Value =
                serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
            (
                "debug.extension.invoke_hook",
                serde_json::json!({
                    "extension_id": extension_id,
                    "kind": kind,
                    "phase": phase,
                    "mode": mode,
                    "target": target,
                    "payload": parsed_payload,
                }),
            )
        }
    }
}

#[cfg(debug_assertions)]
pub(super) fn event_bus_command_to_method_params(
    command: &EventBusCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        EventBusCommands::ListSubscribers { key } => (
            "debug.event_bus.list_subscribers",
            serde_json::json!({ "key": key }),
        ),
        EventBusCommands::Publish {
            key,
            payload,
            scope,
        } => (
            "debug.event_bus.publish",
            serde_json::json!({
                "key": key,
                "payload": payload,
                "scope": scope,
            }),
        ),
        EventBusCommands::Trace { trace_id } => (
            "debug.event_bus.trace",
            serde_json::json!({ "trace_id": trace_id }),
        ),
    }
}

/// 입력 주입 — 사용자 입력 재현이라 debug 격리 안에서만 존재한다(원칙 1).
/// 파라미터 이름은 핸들러가 실제로 읽는 것과 같아야 한다 — 이름이 어긋나면 잎이
/// 있는데 기본값으로만 도는, 없느니만 못한 진입점이 된다.
#[cfg(debug_assertions)]
pub(super) fn inject_debug_command_to_method_params(
    command: &crate::InjectDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::InjectDebugCommands;
    match command {
        InjectDebugCommands::Key {
            surface,
            text,
            bytes,
        } => (
            "debug.inject_key",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "text": text,
                "bytes": bytes,
            }),
        ),
        InjectDebugCommands::Mouse {
            surface,
            row,
            col,
            event_type,
            button,
        } => (
            "debug.inject_mouse",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "row": row,
                "col": col,
                "event_type": event_type,
                "button": button,
            }),
        ),
        InjectDebugCommands::WindowMouse {
            surface,
            fx,
            fy,
            event_type,
            button,
            unit,
            scroll_dx,
            scroll_dy,
        } => (
            "debug.inject_window_mouse",
            pointer_params(
                *surface, *fx, *fy, event_type, *button, unit, *scroll_dx, *scroll_dy,
            ),
        ),
        InjectDebugCommands::EguiMouse {
            surface,
            fx,
            fy,
            event_type,
            button,
            unit,
            scroll_dx,
            scroll_dy,
        } => (
            "debug.inject_egui_mouse",
            pointer_params(
                *surface, *fx, *fy, event_type, *button, unit, *scroll_dx, *scroll_dy,
            ),
        ),
        InjectDebugCommands::EguiKey { key, pressed } => (
            "debug.inject_egui_key",
            serde_json::json!({ "key": key, "pressed": pressed }),
        ),
        InjectDebugCommands::EguiText { text } => (
            "debug.inject_egui_text",
            serde_json::json!({ "text": text }),
        ),
    }
}

/// 두 포인터 주입(`window_mouse` · `egui_mouse`)은 같은 파라미터 집합을 받는다.
/// 한 벌로 두지 않으면 한쪽만 고쳐지는 순간 두 진입점이 갈린다.
#[cfg(debug_assertions)]
#[allow(clippy::too_many_arguments)]
fn pointer_params(
    surface: Option<u32>,
    fx: f64,
    fy: f64,
    event_type: &str,
    button: u64,
    unit: &str,
    scroll_dx: f64,
    scroll_dy: f64,
) -> serde_json::Value {
    serde_json::json!({
        "surface_id": resolve_surface_id(surface),
        "fx": fx,
        "fy": fy,
        "event_type": event_type,
        "button": button,
        "unit": unit,
        "scroll_dx": scroll_dx,
        "scroll_dy": scroll_dy,
    })
}

#[cfg(debug_assertions)]
pub(super) fn plugin_banner_debug_command_to_method_params(
    command: &crate::PluginBannerDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::PluginBannerDebugCommands;
    match command {
        PluginBannerDebugCommands::Open { banner_id, surface } => (
            "debug.plugin_banner.open",
            serde_json::json!({
                "banner_id": banner_id,
                "surface_id": resolve_surface_id(*surface),
            }),
        ),
        PluginBannerDebugCommands::Close { instance_id } => (
            "debug.plugin_banner.close",
            serde_json::json!({ "instance_id": instance_id }),
        ),
    }
}
