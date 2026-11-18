//! plugin이 실행 중에 팝업을 선언하고 여는 요청의 wire 타입.
//!
//! plugin은 [`PopupDecl`] 전체가 아니라 생략할 수 있는 부분 선언 [`PopupDeclSpec`]을 보낸다.
//! host는 [`PopupDecl::plugin`] 기본값 위에 plugin이 준 필드만 덮는다. 그래서 host가
//! `PopupDecl`에 필드를 더해도 옛 plugin의 요청은 그대로 읽힌다.
//!
//! plugin이 정하지 않는 필드: `scope_target`(열 때 `popup.show`의 대상으로 정한다),
//! `chrome`·`drag_handle`·`shell`·`shadow`·`content_margin`(셸 모양은 host가 정한다).
//! 모르는 필드는 무시한다.

use serde::{Deserialize, Serialize};

use crate::popup_decl::{
    PopupAnchor, PopupDecl, PopupEscape, PopupFit, PopupInstances, PopupKeyboard,
    PopupOutsideClick, PopupScopeKind, PopupSize,
};

/// plugin → host: 팝업 선언을 등록하거나 바꾼다. params는 [`PopupDeclareParams`], 응답은 빈 객체.
pub const METHOD_POPUP_DECLARE: &str = "popup.declare";
/// plugin → host: 선언한 팝업의 인스턴스를 연다. params는 [`PopupShowParams`],
/// 응답은 [`PopupShowResult`].
pub const METHOD_POPUP_SHOW: &str = "popup.show";

/// plugin이 보내는 부분 선언. `scope_kind`와 `size`만 필수다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PopupDeclSpec {
    pub scope_kind: PopupScopeKind,
    pub size: PopupSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instances: Option<PopupInstances>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_scope: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<PopupFit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<PopupAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_inset: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_on_open: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<PopupKeyboard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape: Option<PopupEscape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside_click: Option<PopupOutsideClick>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticky_focus: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resizable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_size: Option<PopupSize>,
}

impl PopupDeclSpec {
    /// 범위 종류와 크기만 정한 부분 선언.
    pub const fn new(scope_kind: PopupScopeKind, size: PopupSize) -> Self {
        Self {
            scope_kind,
            size,
            instances: None,
            covers_scope: None,
            fit: None,
            anchor: None,
            scope_inset: None,
            focus_on_open: None,
            keyboard: None,
            escape: None,
            outside_click: None,
            sticky_focus: None,
            resizable: None,
            min_size: None,
        }
    }

    /// [`PopupDecl::plugin`] 위에 준 필드만 덮은 선언. 크기 검사는 하지 않는다
    /// ([`PopupDecl::check_sizes`]).
    pub fn to_decl(&self) -> PopupDecl {
        let mut d = PopupDecl::plugin(self.scope_kind, self.size);
        if let Some(v) = self.instances {
            d.instances = v;
        }
        if let Some(v) = self.covers_scope {
            d.covers_scope = v;
        }
        if let Some(v) = self.fit {
            d.fit = v;
        }
        if let Some(v) = self.anchor {
            d.anchor = v;
        }
        if let Some(v) = self.scope_inset {
            d.scope_inset = v;
        }
        if let Some(v) = self.focus_on_open {
            d.focus_on_open = v;
        }
        if let Some(v) = self.keyboard {
            d.keyboard = v;
        }
        if let Some(v) = self.escape {
            d.escape = v;
        }
        if let Some(v) = self.outside_click {
            d.outside_click = v;
        }
        if let Some(v) = self.sticky_focus {
            d.sticky_focus = v;
        }
        if let Some(v) = self.resizable {
            d.resizable = v;
        }
        if self.min_size.is_some() {
            d.min_size = self.min_size;
        }
        d
    }
}

/// `popup.declare` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupDeclareParams {
    /// plugin 안에서 고유한 팝업 id. 형식은 매니페스트 팝업 id와 같다.
    pub popup_id: String,
    pub decl: PopupDeclSpec,
}

/// `popup.show`가 팝업을 놓을 범위 대상. 종류는 선언의 `scope_kind`와 같아야 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PopupShowTarget {
    /// 창 범위. 창은 `via_surface`가 있는 창이다.
    Window {
        via_surface: u32,
    },
    Workspace {
        id: u32,
    },
    Pane {
        id: u32,
    },
    Tab {
        id: u32,
    },
    Surface {
        id: u32,
    },
}

impl PopupShowTarget {
    pub const fn scope_kind(&self) -> PopupScopeKind {
        match self {
            Self::Window { .. } => PopupScopeKind::Window,
            Self::Workspace { .. } => PopupScopeKind::Workspace,
            Self::Pane { .. } => PopupScopeKind::Pane,
            Self::Tab { .. } => PopupScopeKind::Tab,
            Self::Surface { .. } => PopupScopeKind::Surface,
        }
    }
}

/// `popup.show` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupShowParams {
    pub popup_id: String,
    pub target: PopupShowTarget,
    /// host가 `popup.open`의 `context`로 그대로 넘긴다. 없으면 Null.
    #[serde(default)]
    pub context: serde_json::Value,
}

/// `popup.show` 응답. 인스턴스 정책이 이미 열린 인스턴스를 고르면 그 id이고,
/// 그때 host는 `popup.open`을 다시 보내지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupShowResult {
    pub instance_id: u64,
}

#[cfg(test)]
#[path = "popup_spec_tests.rs"]
mod tests;
