//! 플러그인 egui-mesh surface 가 키를 받는 동안의 편집 키(복사·잘라내기·붙여넣기) 판정. 팝업과 같이
//! egui-winit 과 단축키 디스패치보다 먼저 한 번 한다. 회신을 약속한 플러그인의 surface 만 판정하고, 그 밖의
//! surface 는 기존 단축키 경로(`run_copy`·`run_paste`)가 받는다. 원격 mirror surface 의 편집 키는 원격 결과를
//! 받을 길이 없어 넘긴다. 계약: `docs/design/policies/key-mapping.md`의 "플러그인 입력칸" 절.

use winit::event::{ElementState, WindowEvent};

use super::MainView;
use super::plugin_popup_keys::{AwaitedPaste, PluginEditAsk, popup_capability, popup_key};
use crate::adapters::ui::input::edit_key::{Capability, EditBindings, EditKey, verdict};
use crate::gfx::gpu::edit_keys::EguiKey;
use crate::gfx::gpu::plugin_paste_read::PluginClip;
use crate::plugin::manager::PluginCopyTarget;
use crate::runtime::engine_read::EngineRead;

/// 키를 받는 surface 중 이 판정이 맡는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SurfaceEditTarget {
    /// 회신을 약속한 플러그인의 로컬 egui-mesh surface. `format_paste`는 그 kind 가 `egui_paste`를 선언해
    /// 플러그인이 글자 밖의 형식(이미지 등)을 클립보드에서 직접 읽는지다.
    Local {
        surface: u32,
        plugin_id: String,
        format_paste: bool,
    },
    /// 원격 egui-mesh mirror. 편집 이벤트를 보내지 않고 원래 키를 원격 플러그인에 넘긴다.
    Mirror,
}

/// 키를 받는 surface 의 편집 판정 대상. 키가 호스트 egui(팝업·모달·무대)로 가거나, 포커스 surface 가
/// egui-mesh 가 아니거나, 그 플러그인이 회신을 약속하지 않았으면(직전 렌더 기준) 없다.
pub(super) fn surface_edit_target(
    overlay: bool,
    local: Option<(u32, &str, bool, bool)>,
    mirror: bool,
) -> Option<SurfaceEditTarget> {
    if overlay {
        return None;
    }
    if let Some((surface, plugin_id, reports, format_paste)) = local {
        return reports.then(|| SurfaceEditTarget::Local {
            surface,
            plugin_id: plugin_id.to_string(),
            format_paste,
        });
    }
    mirror.then_some(SurfaceEditTarget::Mirror)
}

/// mirror 의 편집 바인딩은 넘긴다. 원격 결과를 받을 길이 없어 편집 이벤트를 보내지 않고, 원래 키가 원격
/// 플러그인에 `Key`로 간다. 바인딩이 아니면 판정하지 않는다.
fn mirror_key(binding: Option<EditKey>) -> EguiKey {
    popup_key(verdict(binding, |_| Capability::No)).0
}

/// 로컬 surface 대상의 능력. 팝업과 같되, `egui_paste` surface 는 글자가 없어도 플러그인이 다른 형식을 붙일 수
/// 있어 회신을 봐야 안다. 그때 요청은 글자 없는 형식 붙여넣기(`paste`)다.
fn surface_capability(edit: EditKey, clip: &PluginClip, format_paste: bool) -> Capability {
    match edit {
        EditKey::Paste if format_paste => Capability::Unknown,
        _ => popup_capability(edit, clip),
    }
}

/// surface 판정이 키에 하는 일과 남길 편집 요청. 대상이 없으면 `None`이고 다음 판정이 받는다. 뗌은 그대로
/// 보낸다. mirror 는 넘기고, 회신을 약속한 로컬 surface 는 팝업과 같은 규칙으로 판정해 기다리는 키를 Tasty 가
/// 가져가고 원래 키를 붙잡은 요청을 남긴다. 붙여넣을 글자는 붙여넣기 바인딩일 때만 읽는다. `egui_paste`
/// surface 는 글자가 없어도 형식 붙여넣기 요청을 남긴다. 글자를 읽는 중이면 그 기다림을 남긴다.
pub(super) fn surface_key(
    target: Option<SurfaceEditTarget>,
    pressed: bool,
    binding: Option<EditKey>,
    paste_clip: impl FnOnce() -> PluginClip,
    original: &WindowEvent,
) -> Option<(EguiKey, Option<PluginEditAsk>)> {
    let target = target?;
    if !pressed {
        return Some((EguiKey::Free, None));
    }
    let (surface, plugin_id, format_paste) = match target {
        SurfaceEditTarget::Mirror => return Some((mirror_key(binding), None)),
        SurfaceEditTarget::Local {
            surface,
            plugin_id,
            format_paste,
        } => (surface, plugin_id, format_paste),
    };
    let mut clip = PluginClip::Empty;
    let judged = verdict(binding, |edit| {
        if edit == EditKey::Paste {
            clip = paste_clip();
        }
        surface_capability(edit, &clip, format_paste)
    });
    let (key, asked) = popup_key(judged);
    let ask = asked.map(|edit| PluginEditAsk {
        target: PluginCopyTarget::Surface(surface),
        plugin_id: Some(plugin_id),
        edit,
        text: clip.text().map(str::to_string),
        original: Some(original.clone()),
        awaiting: AwaitedPaste::of(&clip, format_paste),
    });
    Some((key, ask))
}

/// 팔레트 편집 명령이 포커스 surface 에 하는 일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PaletteSurfaceEdit {
    /// 이 판정의 대상이 아니다. 기존 팔레트 경로(`run_copy`·탐색기·`run_paste`)가 받는다.
    NotMine,
    /// 대상이지만 보낼 것이 없다(`egui_paste`가 아닌 surface 에 빈 클립보드의 붙여넣기). 터미널 붙여넣기로
    /// 넘기지 않는다. 글자를 읽는 중이면 `Ask`이고 App 이 읽기를 기다린다.
    Nothing,
    /// 이 surface 의 플러그인에 편집 요청을 보낸다.
    Ask { surface: u32, plugin_id: String },
}

/// 팔레트 편집 명령의 대상. 회신을 약속한 플러그인의 로컬 surface 만 맡는다. mirror 와 약속하지 않은 플러그인은
/// 기존 경로로 간다. 키가 없으므로 할 수 없음을 알면 보내지 않고 끝낸다.
pub(super) fn palette_surface_edit(
    target: Option<SurfaceEditTarget>,
    edit: EditKey,
    clip: &PluginClip,
) -> PaletteSurfaceEdit {
    let Some(SurfaceEditTarget::Local {
        surface,
        plugin_id,
        format_paste,
    }) = target
    else {
        return PaletteSurfaceEdit::NotMine;
    };
    match surface_capability(edit, clip, format_paste) {
        Capability::No => PaletteSurfaceEdit::Nothing,
        _ => PaletteSurfaceEdit::Ask { surface, plugin_id },
    }
}

impl MainView {
    /// 팔레트의 복사·잘라내기·붙여넣기를 회신을 약속한 플러그인 surface 에 요청으로 보낸다. 맡았으면 true 이고
    /// 기존 팔레트 경로를 타지 않는다. 키가 없으므로 원래 키 없이 남기고, App 이 회신의 복사 글자만 쓴다.
    /// 글자가 없는 붙여넣기는 `egui_paste` surface 에만 형식 붙여넣기 요청으로 보낸다.
    pub(crate) fn ask_plugin_surface_palette_edit(
        &mut self,
        engine: &EngineRead<'_>,
        edit: EditKey,
    ) -> bool {
        let target = self.surface_edit_target_now(engine);
        let (clip, format_paste) = match (&target, edit) {
            (Some(SurfaceEditTarget::Local { format_paste, .. }), EditKey::Paste) => {
                (self.base.gpu.plugin_paste(true), *format_paste)
            }
            _ => (PluginClip::Empty, false),
        };
        match palette_surface_edit(target, edit, &clip) {
            PaletteSurfaceEdit::NotMine => false,
            PaletteSurfaceEdit::Nothing => true,
            PaletteSurfaceEdit::Ask { surface, plugin_id } => {
                self.plugin_edit_ask = Some(PluginEditAsk {
                    target: PluginCopyTarget::Surface(surface),
                    plugin_id: Some(plugin_id),
                    edit,
                    text: clip.text().map(str::to_string),
                    original: None,
                    awaiting: AwaitedPaste::of(&clip, format_paste),
                });
                true
            }
        }
    }

    /// 창의 키를 지금 받는 로컬 egui-mesh surface. 호스트 egui 가 키를 받는 동안(팝업·모달·무대)은 없다.
    /// 보류의 대상이 아직 키를 받는지 App 이 이것으로 확인한다.
    pub(crate) fn plugin_surface_key_target(&self, engine: &EngineRead<'_>) -> Option<u32> {
        if self.host_egui_has_keys() {
            return None;
        }
        self.focused_egui_mesh_surface_id(engine)
    }

    /// 호스트 egui(팝업·모달·무대)가 키를 받는지. 무대는 키보드 오버레이 술어에 없어 함께 본다.
    fn host_egui_has_keys(&self) -> bool {
        self.state.keyboard_overlay_open() || self.state.fullscreen_stage_active()
    }

    fn surface_edit_target_now(&self, engine: &EngineRead<'_>) -> Option<SurfaceEditTarget> {
        let overlay = self.host_egui_has_keys();
        let local = self.focused_egui_mesh_surface_id(engine).and_then(|sid| {
            let plugin_id = engine.find_surface_by_id(sid)?.mesh()?.plugin_id.clone();
            let reports = self
                .egui_mesh
                .get(&sid)
                .is_some_and(|st| st.reports_edit_outcomes);
            Some((sid, plugin_id, reports, self.egui_paste_kind(engine, sid)))
        });
        let mirror = self.focused_attach_mesh_surface_id(engine).is_some();
        let local = local
            .as_ref()
            .map(|(sid, plugin_id, reports, format_paste)| {
                (*sid, plugin_id.as_str(), *reports, *format_paste)
            });
        surface_edit_target(overlay, local, mirror)
    }

    /// surface 의 kind 가 `egui_paste`를 선언했는지. 판정한 그 surface 의 kind 로 직접 찾는다.
    fn egui_paste_kind(&self, engine: &EngineRead<'_>, sid: u32) -> bool {
        engine
            .find_surface_by_id(sid)
            .and_then(|surface| engine.surface_registry.get(surface.kind()))
            .is_some_and(|kind| kind.egui_paste)
    }

    /// 키를 받는 surface 가 이 판정의 대상일 때의 편집 키 판정. 다른 대상이면 `None`이고 다음 판정이 받는다.
    /// 호스트 입력칸의 보류가 있으면 그 판정이 이 키를 줄 세우도록 `None`이다.
    pub(super) fn judge_plugin_surface_edit_key(
        &mut self,
        engine: &EngineRead<'_>,
        event: &WindowEvent,
    ) -> Option<EguiKey> {
        let WindowEvent::KeyboardInput { event: key, .. } = event else {
            return None;
        };
        if self.base.gpu.edit_keys_busy() {
            return None;
        }
        let target = self.surface_edit_target_now(engine);
        let pressed = key.state == ElementState::Pressed;
        let binding = if target.is_some() && pressed {
            let mods = self.base.state.modifiers;
            let lookup =
                crate::shortcuts::shortcut_lookup_key(&key.logical_key, &key.physical_key, mods);
            EditBindings::from_settings(&engine.settings.keybindings).binding(&lookup, mods)
        } else {
            None
        };
        let gpu = &mut self.base.gpu;
        let (judged, ask) =
            surface_key(target, pressed, binding, || gpu.plugin_paste(true), event)?;
        if let Some(ask) = ask {
            self.plugin_edit_ask = Some(ask);
        }
        Some(judged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 회신을 약속한 플러그인의 로컬 surface 만 판정한다. 약속하지 않은 플러그인(번들 image 등)의 surface 는
    /// 기존 단축키 경로로 가고, mirror 는 넘기는 대상이다. 호스트 egui 가 키를 받는 동안은 판정하지 않는다.
    #[test]
    fn only_surfaces_of_plugins_that_reply_or_mirrors_are_judged() {
        assert_eq!(
            surface_edit_target(false, Some((4, "demo", true, false)), false),
            Some(SurfaceEditTarget::Local {
                surface: 4,
                plugin_id: "demo".into(),
                format_paste: false,
            })
        );
        assert_eq!(
            surface_edit_target(false, Some((4, "demo", true, true)), false),
            Some(SurfaceEditTarget::Local {
                surface: 4,
                plugin_id: "demo".into(),
                format_paste: true,
            })
        );
        assert_eq!(
            surface_edit_target(false, Some((4, "image", false, true)), false),
            None
        );
        assert_eq!(
            surface_edit_target(false, None, true),
            Some(SurfaceEditTarget::Mirror)
        );
        assert_eq!(surface_edit_target(true, None, true), None);
        assert_eq!(surface_edit_target(false, None, false), None);
    }

    /// mirror 의 편집 바인딩은 넘긴다(원래 키가 원격 플러그인에 `Key`로 간다). 바인딩이 아니면 판정하지 않는다.
    #[test]
    fn a_mirror_passes_its_edit_keys() {
        assert_eq!(
            mirror_key(Some(EditKey::Paste)),
            EguiKey::Pass(EditKey::Paste)
        );
        assert_eq!(
            mirror_key(Some(EditKey::Copy)),
            EguiKey::Pass(EditKey::Copy)
        );
        assert_eq!(mirror_key(Some(EditKey::Cut)), EguiKey::Pass(EditKey::Cut));
        assert_eq!(mirror_key(None), EguiKey::Free);
    }

    fn local() -> Option<SurfaceEditTarget> {
        Some(SurfaceEditTarget::Local {
            surface: 4,
            plugin_id: "demo".into(),
            format_paste: false,
        })
    }

    /// `egui_paste`를 선언한 kind 의 surface.
    fn format_paste_local() -> Option<SurfaceEditTarget> {
        Some(SurfaceEditTarget::Local {
            surface: 4,
            plugin_id: "demo".into(),
            format_paste: true,
        })
    }

    fn judge(
        target: Option<SurfaceEditTarget>,
        pressed: bool,
        binding: Option<EditKey>,
        clipboard: Option<&str>,
    ) -> Option<(EguiKey, Option<PluginEditAsk>)> {
        let clip = match clipboard {
            Some(text) => PluginClip::Text(text.to_string()),
            None => PluginClip::Empty,
        };
        judge_clip(target, pressed, binding, clip)
    }

    fn judge_clip(
        target: Option<SurfaceEditTarget>,
        pressed: bool,
        binding: Option<EditKey>,
        clip: PluginClip,
    ) -> Option<(EguiKey, Option<PluginEditAsk>)> {
        surface_key(
            target,
            pressed,
            binding,
            || clip,
            &WindowEvent::Focused(true),
        )
    }

    /// 회신을 약속한 로컬 surface 의 편집 키는 Tasty 가 가져가고, 그 surface 에 원래 키를 붙잡은 요청을 남긴다.
    /// 빈 클립보드의 붙여넣기와 바인딩이 아닌 키는 요청 없이 egui-winit 으로 가고, 뗌은 그대로 보낸다.
    #[test]
    fn a_surface_that_replies_takes_its_edit_keys_and_holds_the_original() {
        for (edit, clip, text) in [
            (EditKey::Copy, None, None),
            (EditKey::Cut, None, None),
            (EditKey::Paste, Some("hi"), Some("hi")),
        ] {
            let (key, ask) = judge(local(), true, Some(edit), clip).expect("판정 대상");
            assert_eq!(key, EguiKey::Taken, "{edit:?}");
            let ask = ask.expect("요청");
            assert_eq!(ask.target, PluginCopyTarget::Surface(4));
            assert_eq!(ask.plugin_id.as_deref(), Some("demo"));
            assert_eq!(ask.edit, edit);
            assert_eq!(ask.text.as_deref(), text);
            assert_eq!(ask.original, Some(WindowEvent::Focused(true)));
        }
        let (key, ask) = judge(local(), true, Some(EditKey::Paste), None).expect("판정 대상");
        assert_eq!((key, ask.is_none()), (EguiKey::Pass(EditKey::Paste), true));
        let (key, ask) = judge(local(), true, None, Some("hi")).expect("판정 대상");
        assert_eq!((key, ask.is_none()), (EguiKey::Free, true));
        let (key, ask) = judge(local(), false, Some(EditKey::Copy), None).expect("판정 대상");
        assert_eq!((key, ask.is_none()), (EguiKey::Free, true));
    }

    /// `egui_paste` surface 는 글자 없는 붙여넣기도 가져가 원래 키를 붙잡은 형식 붙여넣기(`paste`) 요청을 남긴다.
    /// 글자가 있으면 글자 붙여넣기 요청 하나만 남기고, 붙여넣기가 아닌 편집과 뗌은 다른 surface 와 같다.
    #[test]
    fn an_egui_paste_surface_asks_a_format_paste_when_there_is_no_text() {
        use tasty_plugin_protocol::EditRequestWire as W;
        let (key, ask) =
            judge(format_paste_local(), true, Some(EditKey::Paste), None).expect("판정 대상");
        assert_eq!(key, EguiKey::Taken);
        let ask = ask.expect("요청");
        assert_eq!(ask.target, PluginCopyTarget::Surface(4));
        assert_eq!(ask.plugin_id.as_deref(), Some("demo"));
        assert_eq!(ask.wire(), W::Paste);
        assert_eq!(ask.original, Some(WindowEvent::Focused(true)));
        let (key, ask) =
            judge(format_paste_local(), true, Some(EditKey::Paste), Some("hi")).expect("판정 대상");
        assert_eq!(key, EguiKey::Taken);
        assert_eq!(
            ask.expect("요청").wire(),
            W::PasteText { text: "hi".into() }
        );
        let (key, ask) =
            judge(format_paste_local(), true, Some(EditKey::Copy), None).expect("판정 대상");
        assert_eq!((key, ask.expect("요청").wire()), (EguiKey::Taken, W::Copy));
        let (key, ask) =
            judge(format_paste_local(), false, Some(EditKey::Paste), None).expect("판정 대상");
        assert_eq!((key, ask.is_none()), (EguiKey::Free, true));
    }

    /// 붙여넣을 글자를 읽는 중이면(네이티브 Wayland 의 다른 앱 선택) 키를 가져가고 원래 키를 붙잡은 기다림을
    /// 남긴다. 글자 없이 끝났을 때 형식 붙여넣기를 보낼지는 대상의 `egui_paste`다.
    #[test]
    fn a_pending_read_takes_the_key_and_leaves_an_await() {
        for (target, format_paste) in [(local(), false), (format_paste_local(), true)] {
            let (key, ask) = judge_clip(target, true, Some(EditKey::Paste), PluginClip::Pending(9))
                .expect("판정 대상");
            assert_eq!(key, EguiKey::Taken);
            let ask = ask.expect("요청");
            assert_eq!(ask.text, None);
            assert_eq!(ask.original, Some(WindowEvent::Focused(true)));
            assert_eq!(
                ask.awaiting,
                Some(AwaitedPaste {
                    tag: 9,
                    format_paste
                })
            );
        }
        let (_, ask) = judge(local(), true, Some(EditKey::Paste), Some("hi")).expect("판정 대상");
        assert_eq!(ask.expect("요청").awaiting, None);
        assert_eq!(
            palette_surface_edit(local(), EditKey::Paste, &PluginClip::Pending(9)),
            PaletteSurfaceEdit::Ask {
                surface: 4,
                plugin_id: "demo".into()
            }
        );
    }

    /// mirror 는 요청 없이 넘기고, 대상이 없으면 이 판정이 받지 않는다.
    #[test]
    fn a_mirror_passes_without_a_request_and_no_target_is_not_judged() {
        let (key, ask) = judge(
            Some(SurfaceEditTarget::Mirror),
            true,
            Some(EditKey::Paste),
            Some("hi"),
        )
        .expect("판정 대상");
        assert_eq!((key, ask.is_none()), (EguiKey::Pass(EditKey::Paste), true));
        assert!(judge(None, true, Some(EditKey::Copy), Some("hi")).is_none());
    }

    /// 판정 입구는 대상·눌림·바인딩을 모아 판정에 넘기고, 판정 결과를 그대로 돌려주며 남길 요청만 기록한다.
    /// 약속 surface 의 키를 판정 전에 돌려보내거나, egui-mesh 의 `Copy`·`Paste` 나 단축키 경로로 함께 보내는
    /// 코드가 이 자리에 생기면 실패한다.
    #[test]
    fn the_surface_gate_only_records_the_request() {
        let squash = |s: &str| s.split_whitespace().collect::<String>();
        let file = include_str!("plugin_surface_keys.rs");
        let (_, body) = file
            .split_once("pub(super) fn judge_plugin_surface_edit_key(")
            .expect("판정 입구");
        let (body, _) = body.split_once("\n    }\n").expect("판정 입구의 끝");
        let body: String = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//"))
            .collect();
        assert_eq!(
            squash(&body),
            squash(
                "&mut self, engine: &EngineRead<'_>, event: &WindowEvent, ) -> Option<EguiKey> {
                let WindowEvent::KeyboardInput { event: key, .. } = event else { return None; };
                if self.base.gpu.edit_keys_busy() { return None; }
                let target = self.surface_edit_target_now(engine);
                let pressed = key.state == ElementState::Pressed;
                let binding = if target.is_some() && pressed {
                    let mods = self.base.state.modifiers;
                    let lookup = crate::shortcuts::shortcut_lookup_key(&key.logical_key, &key.physical_key, mods);
                    EditBindings::from_settings(&engine.settings.keybindings).binding(&lookup, mods)
                } else { None };
                let gpu = &mut self.base.gpu;
                let (judged, ask) = surface_key(target, pressed, binding, || gpu.plugin_paste(true), event)?;
                if let Some(ask) = ask { self.plugin_edit_ask = Some(ask); }
                Some(judged)"
            ),
            "판정 입구가 판정 결과 밖의 일을 한다"
        );
    }

    /// 팔레트 편집은 회신을 약속한 로컬 surface 만 요청으로 보낸다. 빈 클립보드의 붙여넣기는 `egui_paste` surface 에만
    /// 형식 붙여넣기로 보내고 그 밖에는 보내지 않고 끝내며
    /// (터미널로 넘기지 않는다) mirror·약속 없는 플러그인·대상 없음은 기존 경로로 간다.
    #[test]
    fn a_palette_edit_asks_only_a_surface_that_replies() {
        let text = || PluginClip::Text("hi".into());
        let ask = PaletteSurfaceEdit::Ask {
            surface: 4,
            plugin_id: "demo".into(),
        };
        assert_eq!(
            palette_surface_edit(local(), EditKey::Copy, &PluginClip::Empty),
            ask
        );
        assert_eq!(
            palette_surface_edit(local(), EditKey::Cut, &PluginClip::Empty),
            ask
        );
        assert_eq!(palette_surface_edit(local(), EditKey::Paste, &text()), ask);
        assert_eq!(
            palette_surface_edit(local(), EditKey::Paste, &PluginClip::Empty),
            PaletteSurfaceEdit::Nothing
        );
        assert_eq!(
            palette_surface_edit(format_paste_local(), EditKey::Paste, &PluginClip::Empty),
            ask
        );
        assert_eq!(
            palette_surface_edit(format_paste_local(), EditKey::Paste, &text()),
            ask
        );
        assert_eq!(
            palette_surface_edit(Some(SurfaceEditTarget::Mirror), EditKey::Paste, &text()),
            PaletteSurfaceEdit::NotMine
        );
        assert_eq!(
            palette_surface_edit(None, EditKey::Copy, &PluginClip::Empty),
            PaletteSurfaceEdit::NotMine
        );
    }

    /// 연결은 `MainView` 없이 시험할 수 없어 소스로 고정한다. 판정 입구는 재생 표시와 팝업 판정 다음, host 입력칸
    /// 판정 앞에서 이 판정을 부르고, 렌더는 surface 마다 그 플러그인의 회신 약속을 기록한다.
    #[test]
    fn the_gate_and_the_render_feed_this_judgment() {
        let squash = |s: &str| s.split_whitespace().collect::<String>();
        let gate = squash(include_str!("edit_keys.rs"));
        let popup = gate
            .find(&squash("self.judge_plugin_popup_edit_key(engine, event)"))
            .expect("팝업 판정");
        let surface = gate
            .find(&squash(
                "if let Some(judged) = self.judge_plugin_surface_edit_key(engine, event) { return judged; }",
            ))
            .expect("surface 판정이 판정 입구에 없다");
        let host = gate
            .find(&squash("let entry = edit_key_entry("))
            .expect("host 판정");
        assert!(popup < surface && surface < host, "판정 순서가 바뀌었다");
        let render = squash(include_str!("egui_mesh.rs"));
        assert!(
            render.contains(&squash(
                "st.reports_edit_outcomes = mgr.supports_edit_outcome(&plugin_id);"
            )),
            "렌더가 회신 약속을 기록하지 않는다"
        );
    }

    /// 팔레트 연결도 소스로 고정한다. 팔레트 편집 명령은 기존 경로보다 먼저 이 판정을 묻고, App 은 팔레트가
    /// 실행되는 렌더 뒤에 편집 요청을 꺼낸다.
    #[test]
    fn the_palette_asks_this_judgment_first_and_the_frame_takes_it() {
        let squash = |s: &str| s.split_whitespace().collect::<String>();
        let dispatch = squash(include_str!(
            "../../adapters/ui/input/shortcuts/dispatch.rs"
        ));
        let ask = dispatch
            .find(&squash(
                "let deferred = deferred.filter(|action| { !action .edit() .is_some_and(|edit| self.ask_plugin_surface_palette_edit(engine, edit)) });",
            ))
            .expect("팔레트가 surface 판정을 묻지 않는다");
        let routes = dispatch
            .find(&squash("match deferred {"))
            .expect("팔레트 기존 경로");
        assert!(ask < routes, "기존 경로가 먼저다");
        let frame: String = include_str!("../../app/view_frame.rs")
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//"))
            .collect();
        let frame = squash(&frame);
        let draw = frame
            .find(&squash("view.finish_redraw("))
            .expect("렌더 마무리");
        let take = frame
            .find(&squash(
                "self.process_remote_tool_requests(id); self.begin_plugin_field_edit(id); self.poll_port_scans();",
            ))
            .expect("프레임이 편집 요청을 꺼내지 않는다");
        assert!(draw < take, "팔레트 실행 전에 꺼낸다");
    }
}
