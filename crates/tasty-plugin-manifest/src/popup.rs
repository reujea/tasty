//! 매니페스트 팝업 선언을 공통 선언 [`PopupDecl`]로 바꾸고, 실행 중 선언과 같은 규칙으로 검사한다.
//!
//! 변환 값은 지금 plugin 팝업 렌더러의 동작을 그대로 옮긴 것이다. 렌더러가 아직 읽지 않는 필드는
//! [`PopupDecl::plugin`]의 기본값이다.

use tasty_plugin_protocol::popup_decl::{
    PopupAnchor as DeclAnchor, PopupDecl, PopupEscape, PopupFit, PopupInstances, PopupKeyboard,
    PopupOutsideClick, PopupScopeKind, PopupSize,
};
use tasty_type_geometry::length::LogicalPx;

use crate::types::{PopupAnchor, PopupContribute, PopupScopeDecl};

/// `size_hint`를 생략한 매니페스트 팝업의 크기.
pub const DEFAULT_POPUP_SIZE: PopupSize = PopupSize {
    width: LogicalPx(360.0),
    height: LogicalPx(200.0),
};

/// 렌더러가 읽는 덮음·좁은 범위·Esc 선언. 매니페스트에는 이 항목이 없어 [`PopupDecl::plugin`]의 기본값이고,
/// 실행 중 선언은 그 값을 그대로 옮긴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupLayout {
    pub covers_scope: bool,
    pub fit: PopupFit,
    pub scope_inset: bool,
    pub escape: PopupEscape,
}

impl PopupLayout {
    /// 선언에서 이 값들을 옮긴다.
    pub const fn of(decl: &PopupDecl) -> Self {
        Self {
            covers_scope: decl.covers_scope,
            fit: decl.fit,
            scope_inset: decl.scope_inset,
            escape: decl.escape,
        }
    }
}

impl Default for PopupLayout {
    fn default() -> Self {
        Self::of(&PopupDecl::plugin(
            PopupScopeKind::Window,
            DEFAULT_POPUP_SIZE,
        ))
    }
}

impl PopupContribute {
    /// 공통 선언으로 바꾼다.
    ///
    /// - 두 가운데 기준(`screen-center`, `active-surface-center`)은 모두 범위 가운데다.
    /// - `dismiss_on_outside_click`이 거짓이면 바깥 클릭을 무시한다.
    /// - 인스턴스는 (plugin, popup)당 하나이고, 열면 키를 받으며 입력칸을 가질 수 있다.
    pub fn to_decl(&self) -> PopupDecl {
        let size = self.size_hint.map_or(DEFAULT_POPUP_SIZE, |s| PopupSize {
            width: LogicalPx(s.width as f32),
            height: LogicalPx(s.height as f32),
        });
        let scope_kind = match self.scope {
            PopupScopeDecl::Window => PopupScopeKind::Window,
            PopupScopeDecl::Surface => PopupScopeKind::Surface,
        };
        let mut d = PopupDecl::plugin(scope_kind, size);
        d.anchor = match self.anchor {
            PopupAnchor::ScreenCenter | PopupAnchor::ActiveSurfaceCenter => DeclAnchor::ScopeCenter,
            PopupAnchor::Cursor => DeclAnchor::Cursor,
        };
        d.outside_click = if self.dismiss_on_outside_click {
            PopupOutsideClick::Close
        } else {
            PopupOutsideClick::Ignore
        };
        d.instances = PopupInstances::Single;
        d.keyboard = PopupKeyboard::Text;
        d.covers_scope = self.layout.covers_scope;
        d.fit = self.layout.fit;
        d.scope_inset = self.layout.scope_inset;
        d.escape = self.layout.escape;
        d
    }
}

/// 팝업 id 형식: 소문자 ascii·숫자·`-`, 글자로 시작, 길이 64 이하. 매니페스트 도구 id와 같다.
pub fn is_valid_popup_id(id: &str) -> bool {
    crate::validators::is_valid_tool_id(id)
}

/// plugin 팝업 선언을 검사한다. 매니페스트 선언과 실행 중 선언이 같은 규칙을 쓴다.
///
/// `at_point` 기준은 거절한다. plugin 팝업을 여는 요청에는 기준 점이 없다.
pub fn validate_popup_decl(id: &str, decl: &PopupDecl) -> Result<(), String> {
    if !is_valid_popup_id(id) {
        return Err(format!(
            "invalid popup id '{id}': must be lowercase ascii + digits + '-', \
             start with a letter, length ≤ 64"
        ));
    }
    decl.check_sizes()
        .map_err(|e| format!("popup '{id}': {e}"))?;
    if decl.anchor == DeclAnchor::AtPoint {
        return Err(format!(
            "popup '{id}': anchor 'at_point' needs a point and plugin popups are opened without one"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "popup_tests.rs"]
mod tests;
