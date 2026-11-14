//! 플러그인 팝업·egui-mesh surface 입력칸에 보낼 편집 키를 플러그인 요청과 보류로 바꾼다. 창 view가 판정해
//! 남긴 편집을 view 처리 직후(플러그인 단축키 단계 뒤) 꺼낸다. 회신을 약속한 플러그인이면 요청 id를 붙여 그
//! 대상의 입력으로 보내고 원래 키를 그 창에 붙잡는다. 약속하지 않았거나 대상이 사라졌으면 원래 키를 넘긴
//! 편집으로 한 번 재생한다(팝업은 egui-winit 기본 처리, surface 는 플러그인에 가는 `Key`). 팔레트처럼 키 없이
//! 보낸 편집은 붙잡을 키가 없어 요청만 기록하고, 회신의 복사 글자만 쓴다. 붙여넣을 글자를 아직 읽는 붙여넣기는
//! 요청 id 만 받아 두고 읽기가 끝날 때까지 기다린다(`plugin_paste_await`).
//! 계약: `docs/design/policies/key-mapping.md`의 "플러그인 입력칸" 절.

use std::time::Instant;

use tasty_plugin_protocol::RawInputEventWire;
use winit::event::WindowEvent;
use winit::window::WindowId;

use super::App;
use super::edit_key_replay::ReplayStep;
use crate::adapters::ui::input::edit_key::EditKey;
use crate::plugin::PluginManager;
use crate::plugin::manager::{PluginCopyTarget, PluginEditRequest};
use crate::view::main::plugin_popup_keys::PluginEditAsk;

/// 창 view가 남긴 플러그인 편집을 어떻게 처리할지.
#[derive(Debug)]
enum PopupEditPlan {
    /// 원래 키를 넘긴 편집으로 재생한다.
    Replay(Vec<ReplayStep<WindowEvent>>),
    /// 요청을 대상 입력에 붙이고 원래 키를 회신까지 붙잡는다.
    Hold(Box<PopupEditHold>),
    /// 붙여넣을 글자를 읽는 중이다. 요청은 아직 보내지 않고 원래 키를 읽기가 끝날 때까지 붙잡는다.
    Await(Box<PopupEditAwait>),
}

/// 글자를 기다리는 붙여넣기. 읽기 `tag`가 끝나면 `request`의 id 로 보낸다.
#[derive(Debug)]
struct PopupEditAwait {
    request: PluginEditRequest,
    tag: u64,
    original: Option<WindowEvent>,
    format_paste: bool,
}

/// `wire`를 `target`의 다음 입력에 붙이고 원래 키를 `request`의 회신까지 붙잡는다.
#[derive(Debug)]
struct PopupEditHold {
    target: PluginCopyTarget,
    wire: RawInputEventWire,
    request: PluginEditRequest,
    /// 붙잡을 원래 키. 팔레트 편집은 없고 회신의 복사 글자만 쓴다.
    original: Option<WindowEvent>,
    edit: EditKey,
}

impl App {
    /// 창 view가 남긴 플러그인 편집을 요청과 보류로 바꾼다.
    pub(super) fn begin_plugin_field_edit(&mut self, id: WindowId) {
        let Some(ask) = self
            .view
            .views
            .get_mut(&id)
            .and_then(|v| v.as_main_mut())
            .and_then(|main| main.take_plugin_edit_ask())
        else {
            return;
        };
        // 요청을 만들지 못해 재생하는 붙여넣기는 시작한 읽기를 거둔다. 그 결과는 받을 곳이 없다.
        let read = ask.awaiting.map(|awaiting| awaiting.tag);
        match popup_edit_plan(self.plugin_manager.as_mut(), ask) {
            PopupEditPlan::Replay(steps) => {
                if let Some(tag) = read
                    && let Some(view) = self.view.views.get_mut(&id)
                {
                    view.base_mut().gpu.cancel_plugin_paste(tag);
                }
                self.replay_edit_steps(id, steps);
            }
            PopupEditPlan::Await(waiting) => {
                let PopupEditAwait {
                    request,
                    tag,
                    original,
                    format_paste,
                } = *waiting;
                self.plugin_edit_holds.await_paste(
                    id,
                    request,
                    tag,
                    original,
                    format_paste,
                    Instant::now(),
                );
            }
            PopupEditPlan::Hold(hold) => {
                let PopupEditHold {
                    target,
                    wire,
                    request,
                    original,
                    edit,
                } = *hold;
                if let Some(main) = self.view.views.get_mut(&id).and_then(|v| v.as_main_mut()) {
                    main.queue_plugin_edit_request(target, wire);
                }
                let now = Instant::now();
                match original {
                    Some(original) => self
                        .plugin_edit_holds
                        .hold(id, request, original, edit, now),
                    None => self.plugin_edit_holds.track(id, request, edit, now),
                }
            }
        }
    }
}

/// 편집 `ask`의 처리. 대상의 플러그인이 회신을 약속했으면 요청을 만들어 보류한다. 회신을 약속하지 않은 플러그인
/// (번들 플러그인 포함)이나 그 사이 닫힌 팝업이면 원래 키를 넘긴 편집으로 한 번 재생한다.
fn popup_edit_plan(mgr: Option<&mut PluginManager>, ask: PluginEditAsk) -> PopupEditPlan {
    let Some(request) = mgr.and_then(|mgr| edit_request(mgr, &ask)) else {
        let edit = ask.edit;
        return PopupEditPlan::Replay(
            ask.original
                .map(|original| ReplayStep::Original(original, edit))
                .into_iter()
                .collect(),
        );
    };
    if let Some(awaiting) = ask.awaiting {
        return PopupEditPlan::Await(Box::new(PopupEditAwait {
            request,
            tag: awaiting.tag,
            original: ask.original,
            format_paste: awaiting.format_paste,
        }));
    }
    PopupEditPlan::Hold(Box::new(PopupEditHold {
        target: ask.target,
        wire: RawInputEventWire::EditRequest {
            edit_id: request.edit_id,
            edit: ask.wire(),
        },
        request,
        original: ask.original,
        edit: ask.edit,
    }))
}

/// `ask` 대상의 플러그인에 보낼 편집 요청. 팝업은 열린 팝업에서 플러그인을 찾고, surface 는 view 가 키 시점에
/// 읽은 플러그인이다. 팝업이 닫혔거나 그 플러그인이 회신을 약속하지 않았으면 `None`이고 원래 키를 넘긴다.
fn edit_request(mgr: &mut PluginManager, ask: &PluginEditAsk) -> Option<PluginEditRequest> {
    let plugin_id = match ask.target {
        PluginCopyTarget::Popup(instance) => mgr
            .popup_instances()
            .find(|(id, _)| *id == instance)
            .map(|(_, inst)| inst.plugin_id.clone())?,
        PluginCopyTarget::Surface(_) => ask.plugin_id.clone()?,
    };
    mgr.begin_edit_request(&plugin_id, ask.target)
}

#[cfg(test)]
#[path = "plugin_popup_edit_tests.rs"]
mod tests;
