//! 포커스된 surface의 native WebView에 편집 명령을 실행한다.
//! native 자식이 키를 받는 경우는 backend가 직접 실행하고, 여기서는 host 창이 키를 받았거나
//! 명령 팔레트에서 실행한 경우를 처리한다. 계약: `docs/design/systems/webview.md`.

use crate::runtime::engine_read::EngineRead;
use crate::view::main::MainView;
use crate::webview::WebViewEditAction;
use winit::keyboard::{Key, ModifiersState};

impl MainView {
    /// 포커스 surface에 native WebView가 있으면 실행하고 true다. 터미널이나 다른 surface로 넘기지 않는다.
    pub(crate) fn run_webview_edit(
        &mut self,
        engine: &EngineRead<'_>,
        action: WebViewEditAction,
    ) -> bool {
        let Some(sid) = self.state.focused_surface_id(engine) else {
            return false;
        };
        let Some(webview) = self.webviews.get(&sid) else {
            return false;
        };
        webview.execute_edit(action);
        true
    }

    /// `sid` 가 WebView 면 그 페이지에 편집을 실행한다.
    pub(crate) fn run_webview_edit_on(&self, sid: u32, action: WebViewEditAction) -> bool {
        let Some(webview) = self.webviews.get(&sid) else {
            return false;
        };
        webview.execute_edit(action);
        true
    }

    /// host 창이 받은 키가 편집 바인딩이고 포커스 surface가 WebView이면 그 편집이다.
    /// 우선순위는 페이지 편집 > plugin 명령 > 그 밖의 host 액션이며 native 경로와 같다.
    pub(crate) fn webview_edit_for_key(
        &self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> Option<WebViewEditAction> {
        webview_edit_for_surface_key(
            &self.state,
            |sid| self.webviews.contains_key(&sid),
            engine,
            key,
            mods,
        )
    }

    /// host 창이 받은 키가 포커스 WebView의 편집 바인딩이면 실행한다.
    pub(super) fn handle_webview_edit_shortcut(
        &mut self,
        engine: &EngineRead<'_>,
        key: &Key,
        mods: ModifiersState,
    ) -> bool {
        let Some(action) = self.webview_edit_for_key(engine, key, mods) else {
            return false;
        };
        self.run_webview_edit(engine, action)
    }
}

/// 클립보드 편집 뒤에 고르는 WebView 편집 액션.
const AFTER_CLIPBOARD_EDITS: [WebViewEditAction; 3] = [
    WebViewEditAction::SelectAll,
    WebViewEditAction::Undo,
    WebViewEditAction::Redo,
];

/// 한 키가 여러 편집 바인딩에 걸릴 때 고르는 순서. 앞의 셋은 공통 판정의 순서
/// ([`crate::adapters::ui::input::edit_key::EditKey::ORDER`])다.
pub(crate) fn webview_edit_order() -> impl Iterator<Item = WebViewEditAction> {
    crate::adapters::ui::input::edit_key::EditKey::ORDER
        .into_iter()
        .map(WebViewEditAction::from)
        .chain(AFTER_CLIPBOARD_EDITS)
}

/// `webview_edit_for_key` 의 판정. 포커스 surface 에 native WebView 가 있고(`has_webview`) 키가 편집 바인딩이면 그
/// 편집이다. 창 view 가 없는 plugin 단축키 단계도 같은 판정을 쓴다.
pub(crate) fn webview_edit_for_surface_key(
    state: &crate::state::MainViewState,
    has_webview: impl Fn(u32) -> bool,
    engine: &EngineRead<'_>,
    key: &Key,
    mods: ModifiersState,
) -> Option<WebViewEditAction> {
    let sid = state.focused_surface_id(engine)?;
    if !has_webview(sid) {
        return None;
    }
    edit_binding_for_key(&engine.settings.keybindings, key, mods)
}

/// 키가 가리키는 편집 액션. host 창 경로에서 plugin 명령보다 먼저 검사한다.
pub(crate) fn edit_binding_for_key(
    kb: &crate::settings::KeybindingSettings,
    key: &Key,
    mods: ModifiersState,
) -> Option<WebViewEditAction> {
    crate::adapters::ui::input::edit_key::edit_binding(kb, key, mods)
        .map(WebViewEditAction::from)
        .or_else(|| edit_binding_after_clipboard(kb, key, mods))
}

/// 클립보드 편집(복사·잘라내기·붙여넣기)을 뺀 편집 액션. 공통 판정이 클립보드 편집을 이미 판정해
/// 넘긴 키는 이 범위만 고른다.
pub(crate) fn edit_binding_after_clipboard(
    kb: &crate::settings::KeybindingSettings,
    key: &Key,
    mods: ModifiersState,
) -> Option<WebViewEditAction> {
    AFTER_CLIPBOARD_EDITS.into_iter().find(|a| {
        super::matches_any_binding(kb.get_bindings(a.field_id()).unwrap_or(&[]), key, mods)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    fn ch(c: &str) -> Key {
        Key::Character(SmolStr::new(c))
    }

    /// 설정의 `alt` 토큰. macOS에서는 Command다.
    fn alt() -> ModifiersState {
        if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ModifiersState::ALT
        }
    }

    /// 편집 바인딩에 걸리는 키는 plugin이 같은 조합을 가져도 편집으로 판정한다.
    /// 바인딩을 옮기면 판정도 따라가고, 편집에 없는 조합은 plugin·host 판정으로 남는다.
    #[test]
    fn edit_bindings_win_and_follow_the_settings() {
        let mut kb = crate::settings::KeybindingSettings {
            copy: vec!["alt+c".into()],
            page_undo: vec!["alt+u".into()],
            page_redo: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            edit_binding_for_key(&kb, &ch("c"), alt()),
            Some(WebViewEditAction::Copy)
        );
        assert_eq!(
            edit_binding_for_key(&kb, &ch("u"), alt()),
            Some(WebViewEditAction::Undo)
        );
        assert_eq!(edit_binding_for_key(&kb, &ch("q"), alt()), None);
        kb.copy = vec!["ctrl+shift+c".into()];
        assert_eq!(edit_binding_for_key(&kb, &ch("c"), alt()), None);
    }

    /// 고르는 순서는 모든 편집 액션을 한 번씩 담는다.
    #[test]
    fn the_order_covers_every_edit_action_once() {
        let order: Vec<_> = webview_edit_order().collect();
        assert_eq!(order.len(), WebViewEditAction::ALL.len());
        for action in WebViewEditAction::ALL {
            assert_eq!(
                order.iter().filter(|a| **a == action).count(),
                1,
                "{action:?}"
            );
        }
    }
}
