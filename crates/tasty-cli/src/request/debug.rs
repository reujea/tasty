//! `tasty debug ...` CLI → JsonRpcRequest 매핑 (debug + popup + tool + extension + event_bus).

#![cfg(debug_assertions)]

use crate::commands::{DebugCommands, EventBusCommands};

use super::resolve_surface_id;

pub(super) fn debug_command_to_method_params(
    command: &DebugCommands,
) -> (&'static str, serde_json::Value) {
    match command {
        // 로컬 attach 의 non-force 경로는 run_client 에서 raw 스트림으로 선처리된다.
        // 여기 도달하는 건 `--force-detach` 뿐이다. 로컬 사용자의 강제 끊기 재현이라
        // release 표면이 아닌 debug 메서드로 보낸다.
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
                    "debug.attach.force_detach_workspace",
                    serde_json::json!({ "workspace_id": ws }),
                )
            } else {
                (
                    "debug.attach.force_detach",
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
        DebugCommands::Toast {
            message,
            scope,
            target_id,
            window_id,
            kind,
            hint,
        } => (
            "debug.toast",
            serde_json::json!({
                "message": message,
                "scope": scope,
                "target_id": target_id,
                "window_id": window_id,
                "kind": kind,
                "hint": hint,
            }),
        ),
        DebugCommands::RemoteDownload(sub) => remote_download_debug_command_to_method_params(sub),
        DebugCommands::Banner(sub) => banner_debug_command_to_method_params(sub),
        DebugCommands::Settings(sub) => settings_debug_command_to_method_params(sub),
        DebugCommands::Modal(sub) => modal_debug_command_to_method_params(sub),
        DebugCommands::Lua(sub) => lua_debug_command_to_method_params(sub),
        DebugCommands::FocusedSurface => ("debug.focused_surface", serde_json::json!({})),
        DebugCommands::Windows => ("debug.window.list", serde_json::json!({})),
        DebugCommands::SurfaceRect { surface } => (
            "debug.surface_rect",
            serde_json::json!({ "surface_id": surface }),
        ),
        DebugCommands::Selection => ("debug.selection", serde_json::json!({})),
        DebugCommands::PendingMenu => ("debug.pending_menu", serde_json::json!({})),
        DebugCommands::MenuAnswer {
            menu,
            item,
            label,
            dismiss,
        } => {
            let params = match (item, label) {
                (Some(id), _) => serde_json::json!({ "menu": menu, "item": id }),
                (None, Some(text)) => serde_json::json!({ "menu": menu, "label": text }),
                (None, None) => serde_json::json!({ "menu": menu, "dismiss": *dismiss }),
            };
            ("debug.native_menu.answer", params)
        }
        DebugCommands::PendingMove => ("debug.pending_move", serde_json::json!({})),
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
        DebugCommands::WebviewHistory {
            surface,
            action,
            url,
        } => (
            "debug.webview.history",
            serde_json::json!({
                "surface_id": resolve_surface_id(*surface),
                "action": action,
                "url": url,
            }),
        ),
        DebugCommands::WebviewOverlay { force } => (
            "debug.webview.overlay",
            serde_json::json!({ "force": force }),
        ),
        DebugCommands::StreamEcho { .. } => {
            unreachable!("debug stream-echo is dispatched before request mapping")
        }
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
        SettingsDebugCommands::Close => ("debug.settings.close", serde_json::json!({})),
        // 서버가 JSON 값을 받으므로 CLI에서 먼저 파싱한다. 실패는 stderr에 알리고 exit 1로 끝낸다.
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
        // 파일은 CLI가 읽는다. 실패는 stderr에 알리고 exit 1로 끝낸다.
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

/// 사용자 입력 재현은 debug 빌드에서만 제공한다.
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
            modifiers,
            window_id,
        } => {
            let mut params = pointer_params(
                *surface, *fx, *fy, event_type, *button, unit, *scroll_dx, *scroll_dy,
            );
            // 창 기준 좌표에는 surface가 없다. TASTY_SURFACE_ID로 채운 값도 보내지 않는다.
            if let Some(wid) = window_id {
                if let Some(obj) = params.as_object_mut() {
                    obj.remove("surface_id");
                }
                params["window_id"] = serde_json::json!(wid);
            }
            if let Some(names) = modifiers {
                let held: Vec<&str> = names
                    .iter()
                    .map(String::as_str)
                    .filter(|n| *n != "none")
                    .collect();
                params["modifiers"] = serde_json::json!(held);
            }
            ("debug.inject_egui_mouse", params)
        }
        InjectDebugCommands::EguiKey {
            key,
            pressed,
            window_id,
        } => {
            let mut params = serde_json::json!({ "key": key, "pressed": pressed });
            if let Some(wid) = window_id {
                params["window_id"] = serde_json::json!(wid);
            }
            ("debug.inject_egui_key", params)
        }
        InjectDebugCommands::EguiText { text, window_id } => {
            let mut params = serde_json::json!({ "text": text });
            if let Some(wid) = window_id {
                params["window_id"] = serde_json::json!(wid);
            }
            ("debug.inject_egui_text", params)
        }
    }
}

/// 두 포인터 주입 경로가 같은 파라미터를 보내도록 공용으로 조립한다.
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

#[cfg(all(test, debug_assertions))]
mod tests {
    use clap::Parser;
    use serde_json::json;

    fn egui_text(args: &[&str]) -> (String, serde_json::Value) {
        let mut argv = vec!["tasty", "debug", "inject", "egui-text", "--text", "ab"];
        argv.extend_from_slice(args);
        let cli = crate::Cli::try_parse_from(argv).expect("파싱");
        let r = crate::request::command_to_request(&cli.command.expect("명령"));
        (r.method, r.params)
    }

    /// 창을 지정한 포인터 주입은 `window_id`만 싣고 `surface_id`는 보내지 않는다.
    #[test]
    fn egui_mouse_with_a_window_id_drops_the_surface_id() {
        use clap::Parser;
        let cli = crate::Cli::try_parse_from([
            "tasty",
            "debug",
            "inject",
            "egui-mouse",
            "--window-id",
            "9",
            "--fx",
            "0.1",
            "--event-type",
            "press",
        ])
        .expect("파싱");
        let r = crate::request::command_to_request(&cli.command.expect("명령"));
        assert_eq!(r.method, "debug.inject_egui_mouse");
        assert_eq!(r.params["window_id"], 9);
        assert!(r.params.get("surface_id").is_none(), "{}", r.params);
        assert!(
            crate::Cli::try_parse_from([
                "tasty",
                "debug",
                "inject",
                "egui-mouse",
                "--window-id",
                "9",
                "--surface",
                "3",
            ])
            .is_err()
        );
    }

    fn egui_key(args: &[&str]) -> (String, serde_json::Value) {
        let mut argv = vec!["tasty", "debug", "inject", "egui-key", "--key", "Enter"];
        argv.extend_from_slice(args);
        let cli = crate::Cli::try_parse_from(argv).expect("파싱");
        let r = crate::request::command_to_request(&cli.command.expect("명령"));
        (r.method, r.params)
    }

    /// 키 주입도 창을 지정할 때만 `window_id`가 실린다.
    #[test]
    fn egui_key_carries_the_window_id_only_when_given() {
        assert_eq!(
            egui_key(&[]),
            (
                "debug.inject_egui_key".to_string(),
                json!({ "key": "Enter", "pressed": true })
            )
        );
        assert_eq!(
            egui_key(&["--window-id", "7"]),
            (
                "debug.inject_egui_key".to_string(),
                json!({ "key": "Enter", "pressed": true, "window_id": 7u64 })
            )
        );
    }

    /// `--pressed` 는 값을 받는다. 생략하면 누름, 값 없이 주어도 누름, `false` 면 뗌이다.
    #[test]
    fn egui_key_pressed_takes_true_or_false() {
        for (args, pressed) in [
            (&[][..], true),
            (&["--pressed"][..], true),
            (&["--pressed", "true"][..], true),
            (&["--pressed", "false"][..], false),
            (&["--pressed=false"][..], false),
        ] {
            assert_eq!(egui_key(args).1["pressed"], pressed, "{args:?}");
        }
        let bad = [
            "tasty",
            "debug",
            "inject",
            "egui-key",
            "--key",
            "Enter",
            "--pressed",
            "no",
        ];
        assert!(crate::Cli::try_parse_from(bad).is_err());
    }

    /// 휠 단위는 서버가 받는 이름만 통과한다. egui 레벨은 `page` 까지, winit 레벨은 `line`·`point`
    /// 뿐이다. 서버가 모르는 `pixel` 은 CLI 에서 거절한다.
    #[test]
    fn scroll_units_match_what_the_server_accepts() {
        let parse = |sub: &str, unit: &str| {
            crate::Cli::try_parse_from([
                "tasty",
                "debug",
                "inject",
                sub,
                "--surface",
                "1",
                "--event-type",
                "scroll",
                "--unit",
                unit,
            ])
            .map(|cli| crate::request::command_to_request(&cli.command.expect("명령")).params)
        };
        for unit in ["line", "point", "page"] {
            assert_eq!(parse("egui-mouse", unit).expect(unit)["unit"], unit);
        }
        for unit in ["line", "point"] {
            assert_eq!(parse("window-mouse", unit).expect(unit)["unit"], unit);
        }
        assert!(parse("window-mouse", "page").is_err());
        for sub in ["egui-mouse", "window-mouse"] {
            assert!(parse(sub, "pixel").is_err(), "{sub}");
        }
    }

    /// 창을 지정하지 않은 호출은 예전과 같은 요청이고, 지정하면 `window_id`가 실린다.
    #[test]
    fn egui_text_carries_the_window_id_only_when_given() {
        assert_eq!(
            egui_text(&[]),
            (
                "debug.inject_egui_text".to_string(),
                json!({ "text": "ab" })
            )
        );
        assert_eq!(
            egui_text(&["--window-id", "4194311"]),
            (
                "debug.inject_egui_text".to_string(),
                json!({ "text": "ab", "window_id": 4194311u64 })
            )
        );
    }
}

#[cfg(debug_assertions)]
fn remote_download_debug_command_to_method_params(
    command: &crate::RemoteDownloadDebugCommands,
) -> (&'static str, serde_json::Value) {
    use crate::RemoteDownloadDebugCommands;
    match command {
        RemoteDownloadDebugCommands::Start { workspace, path } => (
            "debug.remote_download.start",
            serde_json::json!({ "workspace_id": workspace, "path": path }),
        ),
        RemoteDownloadDebugCommands::Confirm { id } => (
            "debug.remote_download.confirm",
            serde_json::json!({ "download_id": id }),
        ),
        RemoteDownloadDebugCommands::Cancel { id } => (
            "debug.remote_download.cancel",
            serde_json::json!({ "download_id": id }),
        ),
    }
}
