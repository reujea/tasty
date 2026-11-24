//! 키 사건 하나의 수신자를 포커스 노드에서 정한다(`route_key`). 입력은 출발 노드에서 engine 쪽으로 늘어놓은
//! 노드 목록이고, 결과는 그 키를 처음 소비하는 노드(수신자)와 경로의 level 집합이다. 사건 시점 값만 읽는
//! 순수 함수라 목록을 만드는 쪽(메인 창 키 경로)과 표 시험이 같은 함수를 부른다. 규칙:
//! `docs/architecture/input-layer.md`의 "키 경로와 수신자".

use super::edit_key::EditBindings;
use super::key_level::{KeyLevel, PathLevels};
use crate::adapters::ui::popup::focus::PopupNode;
use crate::adapters::ui::popup::keys::{BasicKey, consumes_key};
use crate::adapters::ui::popup::{PopupEscape, PopupInstanceId, PopupKeyboard};

/// egui surface(explorer·dag graph) 내용이 조상 바인딩에 양보하는지. 겹침 측정(키 경로 시험의
/// `egui_surface_content_keys_overlap_surface_bindings`)에서 내용 키와 Surface level 이상 기본 바인딩이 겹쳐서
/// 거짓(내용 먼저)으로 둔다.
pub(crate) const EGUI_SURFACE_YIELDS_TO_ANCESTOR_BINDINGS: bool = false;

/// 판정할 키 누름 하나. egui 키와 그 누름의 수식키다. `super_held`는 비-macOS Win·Super(egui에 없다)다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteKey {
    pub(crate) key: egui::Key,
    pub(crate) modifiers: egui::Modifiers,
    pub(crate) super_held: bool,
}

impl RouteKey {
    /// 수식키 없는 Esc. Esc 선언의 대상이다. 수식키 Esc는 보통 키처럼 오른다. 입력 몫의 조건 8(Esc 누름마다 몫)도
    /// 이 판정을 쓴다.
    pub(crate) fn bare_escape(self) -> bool {
        self.key == egui::Key::Escape && self.modifiers.is_none() && !self.super_held
    }

    /// 명령 수식키(Ctrl·Alt·Cmd·Super)가 붙은 조합.
    fn command(self) -> bool {
        self.modifiers.ctrl || self.modifiers.alt || self.modifiers.mac_cmd || self.super_held
    }

    fn is(self, basic: BasicKey) -> bool {
        basic.matches_egui(self.key, self.modifiers, self.super_held)
    }
}

/// egui 키 사건 하나가 수식키 없는 Esc 누름인지(`RouteKey::bare_escape`). `super_held` 는 비-macOS 실물 Win·Super 다.
/// 시험 대역처럼 egui 사건으로 키를 넣는 입구가 입력 몫의 조건 8 에 쓴다.
#[cfg(test)]
pub(crate) fn is_bare_escape_event(event: &egui::Event, super_held: bool) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } if RouteKey { key: *key, modifiers: *modifiers, super_held }.bare_escape()
    )
}

/// 팝업의 표시 상태. 접힌 대표 버튼은 펼치기 키를, 펼친 팝업은 다시 접는 Esc를 받는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupFold {
    Fits,
    Collapsed,
    Expanded,
}

/// 경로의 팝업 노드. `keymap`은 이 키가 그 노드의 활성 키맵에 맞는지(노드 키맵 수신 판정)다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PopupKeyNode {
    pub(crate) node: PopupNode,
    pub(crate) keyboard: PopupKeyboard,
    pub(crate) escape: PopupEscape,
    pub(crate) fold: PopupFold,
    pub(crate) keymap: bool,
    /// 범위를 덮는다. 덮지 않으면 붙은 노드가 목록의 바로 다음 노드다.
    pub(crate) covers: bool,
}

/// 범위 노드(surface·Tab·Pane·Workspace·Window)의 내용.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScopeContent {
    None,
    /// 조상 바인딩에 양보하고 남은 키를 모두 받는 egui가 아닌 내용(터미널 PTY·mesh·WebView).
    Yielding(u32),
    /// egui surface(explorer·dag graph). `takes`는 내용이 사건 시점에 이 키를 쓰는지다.
    Egui {
        sid: u32,
        takes: bool,
    },
}

/// 범위 노드에 붙은 비덮음 팝업. 노드의 하위에서 올라온 Esc만 받는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttachedPopup {
    pub(crate) node: PopupNode,
    pub(crate) escape: PopupEscape,
}

/// 범위 노드. `attached`는 z 최상단이 먼저다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeKeyNode {
    pub(crate) level: KeyLevel,
    pub(crate) content: ScopeContent,
    pub(crate) attached: Vec<AttachedPopup>,
}

/// 경로의 노드 하나. 목록의 첫 노드가 출발점이다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyNode {
    Popup(PopupKeyNode),
    /// 복원 안내(사건 시점에 안쪽 포커스를 가진 안내 인스턴스).
    Notice(PopupInstanceId),
    Tutorial,
    /// 주소 띠 입력칸. `keymap`은 주소 띠 키맵 수신 판정이다.
    HtmlAddress {
        sid: u32,
        keymap: bool,
    },
    /// 전체화면 무대. `exit`은 이 키가 무대 종료 바인딩에 맞는지다.
    Stage {
        exit: bool,
    },
    Scope(ScopeKeyNode),
}

/// 경로 끝의 목적지. 단축키 후보가 모두 실행을 거절하면 여기로 간다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyEnd {
    Surface(u32),
    Drop,
}

/// 키를 처음 소비하는 노드.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyReceiver {
    Popup(PopupInstanceId),
    PluginPopup(u64),
    /// 사건 시점에 안쪽 포커스를 가진 복원 안내 인스턴스. 그리기는 이 인스턴스만 그 키를 받는다.
    Notice(PopupInstanceId),
    Tutorial,
    HtmlAddress(u32),
    EguiSurface(u32),
    Stage,
    /// 경로의 단축키 단계. 후보는 경로 level의 후보를 실행 순서대로 시도하고 모두 거절하면 `end`로 간다.
    Shortcut {
        end: KeyEnd,
    },
    Surface(u32),
    Drop,
    /// 설정 열기 요청 래치(출발 노드 `FocusNode::SettingsRequest`). 모든 누름이 여기서 끝난다. bare Esc 는 단계 11
    /// 적용 함수가 래치를 취소하고, 다른 키는 단축키·더블탭·편집 판정·끝 목적지 어디에도 가지 않는다.
    SettingsRequest,
}

impl KeyReceiver {
    /// egui 큐에 싣는 수신자.
    pub(crate) fn is_egui(self) -> bool {
        matches!(
            self,
            KeyReceiver::Popup(_)
                | KeyReceiver::PluginPopup(_)
                | KeyReceiver::Notice(_)
                | KeyReceiver::Tutorial
                | KeyReceiver::HtmlAddress(_)
                | KeyReceiver::EguiSurface(_)
                | KeyReceiver::Stage
        )
    }

    fn popup(node: PopupNode) -> Self {
        match node {
            PopupNode::Host(iid) => KeyReceiver::Popup(iid),
            PopupNode::Plugin(iid) => KeyReceiver::PluginPopup(iid),
        }
    }
}

/// `route_key` 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Routed {
    pub(crate) receiver: KeyReceiver,
    /// 경로에 있는 노드의 level. level 필터가 이 집합으로 후보를 고른다.
    pub(crate) levels: PathLevels,
}

/// 키 하나의 수신자. `path`는 출발 노드부터 engine 앞까지의 노드, `matching`은 이 키에 맞는 host 단축키
/// 후보의 level(필터 전)이다. 경로 level 밖의 후보는 보지 않는다.
pub(crate) fn route_key(
    path: &[KeyNode],
    key: RouteKey,
    edits: &EditBindings,
    matching: &[KeyLevel],
) -> Routed {
    let levels = path
        .iter()
        .fold(PathLevels::engine_only(), |levels, n| match n {
            KeyNode::Scope(scope) => levels.with(scope.level),
            _ => levels,
        });
    let shortcut = |level: Option<KeyLevel>| {
        matching
            .iter()
            .any(|l| levels.contains(*l) && level.is_none_or(|level| level == *l))
    };
    let end = match path.first() {
        Some(KeyNode::Scope(ScopeKeyNode {
            content: ScopeContent::Yielding(sid),
            ..
        })) => KeyEnd::Surface(*sid),
        _ => KeyEnd::Drop,
    };
    let up = || {
        if shortcut(None) {
            KeyReceiver::Shortcut { end }
        } else {
            match end {
                KeyEnd::Surface(sid) => KeyReceiver::Surface(sid),
                KeyEnd::Drop => KeyReceiver::Drop,
            }
        }
    };
    let receiver = if key.bare_escape() {
        route_escape(path, &shortcut, end)
    } else {
        match path.first() {
            None => up(),
            Some(node) => {
                let content = start_takes(node, key, edits);
                match node {
                    KeyNode::Popup(p) => {
                        text_or_up(content, p.keyboard, key, KeyReceiver::popup(p.node), &up)
                    }
                    KeyNode::Notice(iid) if content => KeyReceiver::Notice(*iid),
                    KeyNode::Tutorial if content => KeyReceiver::Tutorial,
                    KeyNode::Notice(_) | KeyNode::Tutorial => up(),
                    KeyNode::HtmlAddress { sid, .. } => text_or_up(
                        content,
                        PopupKeyboard::Text,
                        key,
                        KeyReceiver::HtmlAddress(*sid),
                        &up,
                    ),
                    KeyNode::Stage { .. } => {
                        text_or_up(content, PopupKeyboard::Text, key, KeyReceiver::Stage, &up)
                    }
                    KeyNode::Scope(scope) => match scope.content {
                        ScopeContent::None | ScopeContent::Yielding(_) => up(),
                        ScopeContent::Egui { sid, .. } => {
                            if content && !EGUI_SURFACE_YIELDS_TO_ANCESTOR_BINDINGS {
                                KeyReceiver::EguiSurface(sid)
                            } else if shortcut(None) {
                                KeyReceiver::Shortcut { end }
                            } else {
                                KeyReceiver::EguiSurface(sid)
                            }
                        }
                    },
                }
            }
        }
    };
    Routed { receiver, levels }
}

/// 출발 노드의 내용·표시 단계·선언(노드 키맵, 소비 집합, 무대 종료 키)이 이 키를 쓰는지. `Text` 명령 키의
/// 양보 규칙은 보지 않는다.
fn start_takes(node: &KeyNode, key: RouteKey, edits: &EditBindings) -> bool {
    let text = || {
        consumes_key(
            PopupKeyboard::Text,
            key.key,
            key.modifiers,
            key.super_held,
            edits,
        )
    };
    match node {
        KeyNode::Popup(p) => {
            p.keymap
                || (p.fold == PopupFold::Collapsed && key.is(BasicKey::Expand))
                || consumes_key(p.keyboard, key.key, key.modifiers, key.super_held, edits)
        }
        // 안내 위젯은 수식키와 무관하게 이 키들을 읽는다.
        KeyNode::Notice(_) => matches!(
            key.key,
            egui::Key::Escape
                | egui::Key::Enter
                | egui::Key::Space
                | egui::Key::Tab
                | egui::Key::ArrowUp
                | egui::Key::ArrowDown
                | egui::Key::ArrowLeft
                | egui::Key::ArrowRight
        ),
        KeyNode::Tutorial => key.is(BasicKey::Confirm) || key.is(BasicKey::Expand),
        KeyNode::HtmlAddress { keymap, .. } => *keymap || text(),
        // 무대 종료 바인딩이 무대 내용보다 먼저다.
        KeyNode::Stage { exit } => *exit || text(),
        KeyNode::Scope(scope) => matches!(scope.content, ScopeContent::Egui { takes: true, .. }),
    }
}

/// App plugin 판정 게이트: 포커스 노드에서 시작한 경로가 출발 노드에서 이 키를 끝내는지(소비 집합·노드 키맵·
/// 선언). 출발 노드가 범위 노드(surface)면 끝내지 않는다. plugin 명령은 surface 내용보다 먼저다.
pub(crate) fn start_node_ends_key(path: &[KeyNode], key: RouteKey, edits: &EditBindings) -> bool {
    match path.first() {
        None | Some(KeyNode::Scope(_)) => false,
        Some(node) => start_takes(node, key, edits),
    }
}

/// egui 입력칸이 스스로 읽는 명령 조합(egui 0.31.1 `TextEdit`·`CCursorRange`): 전체 선택, 실행 취소·다시 실행,
/// 단어·줄 이동(Shift 포함), 단어 지우기, Ctrl+H·K·U·W 지우기. command 는 Linux·Windows Ctrl, macOS Cmd 다.
pub(crate) fn text_edit_builtin(key: egui::Key, m: egui::Modifiers) -> bool {
    use egui::Key;
    let command = m.command && !m.alt;
    let word = m.alt && !m.command && !m.ctrl;
    match key {
        Key::A | Key::Y | Key::Backspace | Key::Delete | Key::H | Key::K | Key::U | Key::W => {
            command && !m.shift
        }
        Key::Z | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End => command,
        Key::ArrowLeft | Key::ArrowRight => command || word,
        _ => false,
    }
}

/// 노드 내용이 쓰는 키면 그 노드, `Text` 노드의 소비 집합 밖 명령 키는 경로에 맞는 후보가 있을 때만 위로,
/// 그 밖의 집합 밖 키는 위로 보낸다.
fn text_or_up(
    content: bool,
    keyboard: PopupKeyboard,
    key: RouteKey,
    here: KeyReceiver,
    up: &dyn Fn() -> KeyReceiver,
) -> KeyReceiver {
    if content {
        return here;
    }
    let above = up();
    if keyboard == PopupKeyboard::Text
        && key.command()
        && !matches!(above, KeyReceiver::Shortcut { .. })
    {
        here
    } else {
        above
    }
}

/// bare Esc의 수신자. 출발 노드부터 노드마다 내용 → 그 level 단축키 → 붙은 비덮음 팝업 순서로 본다. 덮지
/// 않는 팝업에서 올라온 Esc는 붙은 노드의 내용과 형제 팝업을 건너뛴다.
fn route_escape(
    path: &[KeyNode],
    shortcut: &dyn Fn(Option<KeyLevel>) -> bool,
    end: KeyEnd,
) -> KeyReceiver {
    let mut from_attached = false;
    for node in path {
        match node {
            KeyNode::Popup(p) => {
                if p.escape != PopupEscape::Ignore || p.fold == PopupFold::Expanded {
                    return KeyReceiver::popup(p.node);
                }
                from_attached = !p.covers;
                continue;
            }
            KeyNode::Notice(iid) => return KeyReceiver::Notice(*iid),
            KeyNode::Tutorial => return KeyReceiver::Tutorial,
            KeyNode::HtmlAddress { sid, .. } => return KeyReceiver::HtmlAddress(*sid),
            KeyNode::Stage { .. } => return KeyReceiver::Stage,
            KeyNode::Scope(scope) => {
                if !from_attached {
                    match scope.content {
                        ScopeContent::None => {}
                        ScopeContent::Yielding(sid) => {
                            return if shortcut(None) {
                                KeyReceiver::Shortcut { end }
                            } else {
                                KeyReceiver::Surface(sid)
                            };
                        }
                        ScopeContent::Egui { sid, takes } => {
                            if takes {
                                return KeyReceiver::EguiSurface(sid);
                            }
                        }
                    }
                }
                if shortcut(Some(scope.level)) {
                    return KeyReceiver::Shortcut { end };
                }
                if !from_attached
                    && let Some(a) = scope
                        .attached
                        .iter()
                        .find(|a| a.escape != PopupEscape::Ignore)
                {
                    return KeyReceiver::popup(a.node);
                }
                from_attached = false;
            }
        }
    }
    if shortcut(Some(KeyLevel::Engine)) {
        KeyReceiver::Shortcut { end }
    } else {
        KeyReceiver::Drop
    }
}

#[cfg(test)]
#[path = "key_route_tests.rs"]
mod tests;
