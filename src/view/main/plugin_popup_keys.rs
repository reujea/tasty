//! 플러그인 팝업이 키를 받는 동안의 편집 키(복사·잘라내기·붙여넣기) 판정. 공통 판정과 같이 egui-winit보다
//! 먼저 한 번 한다. 호스트는 팝업 안 입력칸의 선택을 모르므로 미리 소비하지 않는다. 할 수 없음을 아는 경우만
//! 바로 넘기고, 그 밖은 원래 키를 App에 맡겨 플러그인 회신으로 소비·재생을 정한다.
//! 계약: `docs/design/policies/key-mapping.md`의 "플러그인 입력칸" 절.

use winit::event::{ElementState, WindowEvent};

use super::MainView;
use crate::adapters::ui::input::edit_key::{Capability, EditBindings, EditKey, Verdict, verdict};
use crate::gfx::gpu::edit_keys::EguiKey;
use crate::gfx::gpu::plugin_paste_read::PluginClip;
use crate::plugin::manager::PluginCopyTarget;
use crate::runtime::engine_read::EngineRead;

/// 플러그인 팝업·surface 에 보낼 편집. App이 요청을 만들고 원래 키를 보류한다. 플러그인이 회신을 약속하지
/// 않았으면 App이 원래 키를 넘긴 편집으로 재생한다.
#[derive(Debug, Clone)]
pub(crate) struct PluginEditAsk {
    /// 키를 보낸 때 키를 받던 팝업 instance 나 surface. 회신과 재생은 이 대상으로 짝짓는다.
    pub(crate) target: PluginCopyTarget,
    /// surface 대상의 플러그인. 팝업 대상은 App이 열린 팝업에서 찾는다.
    pub(crate) plugin_id: Option<String>,
    pub(crate) edit: EditKey,
    /// 붙여넣을 글자. 붙여넣기에만 있다. 글자 없는 붙여넣기는 형식 붙여넣기다(`egui_paste` surface 만 남긴다).
    pub(crate) text: Option<String>,
    /// 붙잡을 원래 키. 팔레트처럼 키 없이 보낸 편집은 없고, 회신이 와도 재생할 것이 없다.
    pub(crate) original: Option<WindowEvent>,
    /// 붙여넣을 글자를 아직 읽는 중이다(네이티브 Wayland 의 다른 앱 선택). App 은 요청을 보내지 않고 읽기가
    /// 끝날 때까지 기다린다.
    pub(crate) awaiting: Option<AwaitedPaste>,
}

/// 읽는 중인 붙여넣기 글자.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AwaitedPaste {
    /// 읽기의 tag(`GpuState::plugin_paste`).
    pub(crate) tag: u64,
    /// 대상이 글자 없이도 다른 형식을 붙일 수 있다(`egui_paste` surface). 글자 없이 끝나면 형식 붙여넣기를 보낸다.
    pub(crate) format_paste: bool,
}

impl AwaitedPaste {
    /// `clip`이 읽는 중이면 그 기다림.
    pub(crate) fn of(clip: &PluginClip, format_paste: bool) -> Option<Self> {
        match clip {
            PluginClip::Pending(tag) => Some(Self {
                tag: *tag,
                format_paste,
            }),
            _ => None,
        }
    }
}

impl PluginEditAsk {
    /// 플러그인에 보낼 요청 내용. 글자 없는 붙여넣기는 내용 없는 형식 붙여넣기이고 플러그인이 필요한 형식을
    /// 클립보드에서 직접 읽는다.
    pub(crate) fn wire(&self) -> tasty_plugin_protocol::EditRequestWire {
        use tasty_plugin_protocol::EditRequestWire as W;
        match (self.edit, &self.text) {
            (EditKey::Copy, _) => W::Copy,
            (EditKey::Cut, _) => W::Cut,
            (EditKey::Paste, Some(text)) => W::PasteText { text: text.clone() },
            (EditKey::Paste, None) => W::Paste,
        }
    }
}

/// 플러그인 팝업 대상의 능력. 붙여넣기는 클립보드에 글자가 없으면 할 수 없다. 글자를 읽는 중이면 아직 모른다.
/// 그 밖은 회신을 봐야 안다.
pub(super) fn popup_capability(edit: EditKey, clip: &PluginClip) -> Capability {
    match (edit, clip) {
        (EditKey::Paste, PluginClip::Empty) => Capability::No,
        _ => Capability::Unknown,
    }
}

/// 판정이 키에 하는 일. 바인딩이 아니거나 할 수 없음을 아는 키는 egui-winit 에 그대로 간다(기본 편집 키의
/// `Cut`·`Copy`·`Paste` 변환도 그대로다). 그 밖은 Tasty 가 가져가고 플러그인에 보낼 편집을 남긴다.
pub(super) fn popup_key(judged: Verdict) -> (EguiKey, Option<EditKey>) {
    match judged {
        Verdict::NoBinding => (EguiKey::Free, None),
        Verdict::Pass(edit) => (EguiKey::Pass(edit), None),
        Verdict::Act(edit) | Verdict::Hold(edit) => (EguiKey::Taken, Some(edit)),
    }
}

impl MainView {
    /// 키를 받는 팝업이 플러그인 팝업일 때의 편집 키 판정. 다른 대상이면 `None`이고 공통 판정이 받는다.
    /// 호스트 입력칸의 보류가 있으면 그 판정이 이 키를 줄 세우도록 `None`이다.
    pub(super) fn judge_plugin_popup_edit_key(
        &mut self,
        engine: &EngineRead<'_>,
        event: &WindowEvent,
    ) -> Option<EguiKey> {
        let WindowEvent::KeyboardInput { event: key, .. } = event else {
            return None;
        };
        let instance = self.state.plugin_popup_key_target()?;
        if self.base.gpu.edit_keys_busy() {
            return None;
        }
        if key.state != ElementState::Pressed {
            return Some(EguiKey::Free);
        }
        let mods = self.base.state.modifiers;
        let lookup =
            crate::shortcuts::shortcut_lookup_key(&key.logical_key, &key.physical_key, mods);
        let binding =
            EditBindings::from_settings(&engine.settings.keybindings).binding(&lookup, mods);
        // 붙여넣을 글자는 키 시점에 그 팝업을 위해 읽는다(네이티브 Wayland 읽기의 대상 표시). 다른 앱의 선택을
        // 읽는 중이면 회신을 약속한 팝업만 기다린다. 약속하지 않은 팝업은 키를 넘기고 egui-winit 이 다시 읽는다.
        self.base.gpu.set_plugin_paste_target(Some(instance));
        let replies = self.state.plugin_popup_replies.contains(&instance);
        let mut clip = PluginClip::Empty;
        let judged = verdict(binding, |edit| {
            if edit == EditKey::Paste {
                clip = self.base.gpu.plugin_paste(replies);
            }
            popup_capability(edit, &clip)
        });
        let (key, asked) = popup_key(judged);
        if let Some(edit) = asked {
            self.plugin_edit_ask = Some(PluginEditAsk {
                target: PluginCopyTarget::Popup(instance),
                plugin_id: None,
                edit,
                text: clip.text().map(str::to_string),
                original: Some(event.clone()),
                awaiting: AwaitedPaste::of(&clip, false),
            });
        }
        Some(key)
    }

    /// App이 창 view 처리 뒤 꺼내는 플러그인 편집.
    pub(crate) fn take_plugin_edit_ask(&mut self) -> Option<PluginEditAsk> {
        self.plugin_edit_ask.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 빈 클립보드의 붙여넣기는 할 수 없음을 알아 넘긴다. 복사·잘라내기와 글자가 있거나 읽는 중인 붙여넣기는
    /// 플러그인 회신을 기다린다. 바인딩이 아니면 판정하지 않는다.
    #[test]
    fn a_plugin_popup_waits_for_the_reply_unless_the_paste_is_empty() {
        let judge =
            |binding, clip: PluginClip| verdict(binding, |edit| popup_capability(edit, &clip));
        let text = || PluginClip::Text("hi".into());
        assert_eq!(
            judge(Some(EditKey::Paste), PluginClip::Empty),
            Verdict::Pass(EditKey::Paste)
        );
        assert_eq!(
            judge(Some(EditKey::Paste), text()),
            Verdict::Hold(EditKey::Paste)
        );
        assert_eq!(
            judge(Some(EditKey::Paste), PluginClip::Pending(9)),
            Verdict::Hold(EditKey::Paste)
        );
        assert_eq!(
            judge(Some(EditKey::Copy), PluginClip::Empty),
            Verdict::Hold(EditKey::Copy)
        );
        assert_eq!(
            judge(Some(EditKey::Cut), PluginClip::Empty),
            Verdict::Hold(EditKey::Cut)
        );
        assert_eq!(judge(None, text()), Verdict::NoBinding);
    }

    /// 읽는 중인 글자만 기다림이 된다. 형식 붙여넣기 여부는 대상이 정한다.
    #[test]
    fn only_a_pending_clip_becomes_an_awaited_paste() {
        assert_eq!(
            AwaitedPaste::of(&PluginClip::Pending(9), true),
            Some(AwaitedPaste {
                tag: 9,
                format_paste: true
            })
        );
        assert_eq!(AwaitedPaste::of(&PluginClip::Empty, true), None);
        assert_eq!(AwaitedPaste::of(&PluginClip::Text("a".into()), false), None);
    }

    /// 바인딩이 아닌 키와 넘기는 키는 egui-winit 에 그대로 가고 플러그인 편집을 남기지 않는다. 기다리는 키만
    /// Tasty 가 가져간다.
    #[test]
    fn only_a_held_key_is_taken_from_egui_winit() {
        assert_eq!(popup_key(Verdict::NoBinding), (EguiKey::Free, None));
        assert_eq!(
            popup_key(Verdict::Pass(EditKey::Paste)),
            (EguiKey::Pass(EditKey::Paste), None)
        );
        assert_eq!(
            popup_key(Verdict::Hold(EditKey::Cut)),
            (EguiKey::Taken, Some(EditKey::Cut))
        );
    }

    fn ask(edit: EditKey, text: Option<&str>) -> PluginEditAsk {
        PluginEditAsk {
            target: PluginCopyTarget::Popup(7),
            plugin_id: None,
            edit,
            text: text.map(str::to_string),
            original: Some(WindowEvent::Focused(true)),
            awaiting: None,
        }
    }

    /// 편집 키 판정은 egui-winit 앞에서 끝난다. 그래서 키 경로는 egui-winit 이 만든 기본 `Cut`·`Copy`·`Paste`를 지우지
    /// 않는다. egui-winit 결과를 고치는 것은 HTML 주소 띠 재해석 하나뿐이다. 바인딩이 아닌 기본 편집 키를 지우는 코드가
    /// 이 자리에 다시 생기면 실패한다.
    #[test]
    fn the_key_path_keeps_egui_winit_edit_events() {
        let src = include_str!("../main.rs");
        let (_, after) = src
            .split_once("let result = self.base.gpu.handle_egui_event(&self.base.winit, event);")
            .expect("키 경로의 egui-winit 호출을 찾지 못했다");
        let (block, _) = after
            .split_once("\n        result\n")
            .expect("키 경로 블록의 끝을 찾지 못했다");
        let statements: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .collect();
        assert_eq!(
            statements,
            ["self.remap_html_address_keys(engine, event, from, edit_judged);"],
            "egui-winit 뒤 이벤트를 고치는 코드가 늘었다"
        );
    }

    /// 요청 내용은 편집과 붙여넣을 글자 그대로다. 글자 없는 붙여넣기는 형식 붙여넣기다.
    #[test]
    fn the_request_carries_the_edit_and_the_pasted_text() {
        use tasty_plugin_protocol::EditRequestWire as W;
        assert_eq!(ask(EditKey::Copy, None).wire(), W::Copy);
        assert_eq!(ask(EditKey::Cut, None).wire(), W::Cut);
        assert_eq!(
            ask(EditKey::Paste, Some("/tmp/a.md")).wire(),
            W::PasteText {
                text: "/tmp/a.md".into()
            }
        );
        assert_eq!(ask(EditKey::Paste, None).wire(), W::Paste);
    }
}
