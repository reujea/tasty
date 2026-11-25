//! plugin 팝업 키 결속의 이 패스 몫: 키 사건 목록(`pass_keys`), 사건마다 받는 인스턴스(`KeyRoutes`), 이 패스에 입력을
//! 모으지 않은 인스턴스에 보내는 뗌(`send_undrawn_releases`).

use std::collections::HashSet;

use egui::{Context, Event};
use tasty_plugin_protocol::{PopupSetContextParams, RawInputEventWire, RawInputWire};

use super::map_modifiers;
use crate::state::RequestContext;

/// 이 패스 키 사건마다(사건 순서) 받는 인스턴스(`PluginKeyBindings::route`)와 모으는 팝업.
#[derive(Clone, Copy)]
pub(super) struct KeyRoutes<'a> {
    pub(super) routes: &'a [Option<u64>],
    pub(super) instance: u64,
}

impl KeyRoutes<'_> {
    /// 이 패스 `n` 번째 키 사건이 이 팝업에 가는지.
    pub(super) fn takes(self, n: usize) -> bool {
        self.routes.get(n) == Some(&Some(self.instance))
    }
}

/// 이 패스 egui 입력의 키 사건 하나(사건 순서).
#[derive(Clone, Copy)]
pub(super) struct PassKeyEvent {
    pub(super) key: egui::Key,
    pub(super) pressed: bool,
    repeat: bool,
    modifiers: egui::Modifiers,
}

/// 이 패스 egui 입력의 키 사건(사건 순서). 키 결속(`PluginKeyBindings::route`)과 그리지 않는 인스턴스의 뗌 전송
/// (`send_undrawn_releases`)이 이 한 목록을 읽는다.
pub(super) fn pass_keys(ctx: &Context) -> Vec<PassKeyEvent> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|ev| match ev {
                Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } => Some(PassKeyEvent {
                    key: *key,
                    pressed: *pressed,
                    repeat: *repeat,
                    modifiers: *modifiers,
                }),
                _ => None,
            })
            .collect()
    })
}

/// 이 패스에 입력을 모으지 않은(범위 숨김·접힘) 살아 있는 인스턴스에, 그 인스턴스가 받은 누름의 뗌만 보낸다
/// (`PluginKeyBindings`). 크기는 마지막으로 보낸 값이다. 보낸 적 없는 인스턴스는 받은 누름도 없다.
pub(super) fn send_undrawn_releases(
    ctx: &Context,
    state: &mut RequestContext,
    live: &std::collections::BTreeMap<u64, String>,
    collected: &HashSet<u64>,
    keys: &[PassKeyEvent],
    routes: &[Option<u64>],
) {
    let modifiers = ctx.input(|i| map_modifiers(&i.modifiers));
    for (instance, plugin) in live {
        if collected.contains(instance) {
            continue;
        }
        let events: Vec<RawInputEventWire> = keys
            .iter()
            .zip(routes)
            .filter(|(k, to)| !k.pressed && **to == Some(*instance))
            .map(|(k, _)| RawInputEventWire::Key {
                key: k.key.name().to_string(),
                pressed: k.pressed,
                repeat: k.repeat,
                modifiers: map_modifiers(&k.modifiers),
            })
            .collect();
        let Some((w_px, h_px, ppp)) = state
            .plugin_mesh_popup_forward
            .get(instance)
            .and_then(|f| f.last_geom)
        else {
            continue;
        };
        if events.is_empty() {
            continue;
        }
        let theme = state
            .plugin_mesh_popup_forward
            .get(instance)
            .and_then(|f| f.last_theme.clone());
        state.dispatch_intent(
            crate::intent::Intent::Frame(
                crate::app::frame_effect::DrawEffect::PluginDisplay(
                    crate::app::plugin_display::PluginDisplayRequest::Popup {
                        plugin: plugin.clone(),
                        params: PopupSetContextParams {
                            instance_id: *instance,
                            width_px: w_px,
                            height_px: h_px,
                            pixels_per_point: f32::from_bits(ppp),
                            raw_input: RawInputWire {
                                time: None,
                                focused: false,
                                modifiers,
                                events,
                            },
                            theme,
                            need_full_textures: false,
                        },
                    },
                )
                .into(),
            )
            .from_user_menu("plugin-render-context"),
        );
    }
}
