//! 팝업의 기본 조작 키. Esc·Enter·Space·화살표·Tab은 키 코드를 직접 비교하지 않고 단축키와 같은
//! 바인딩 판정(`tasty_key_match`)으로 고른다. 바인딩은 설정 단축키 탭에 노출하지 않는 고정 값이라
//! 사용자가 바꿀 수 없다. 숫자·글자 단축키는 콘텐츠 고유 키라 이 표에 두지 않는다.
// 이유: 팝업 관리자가 이 표로 키를 고르기 전까지 시험 밖 호출자가 없다.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "popups still read keys directly until the manager hands them this table"
    )
)]

use crate::adapters::ui::input::edit_key::{EditBindings, EditKey};

/// 팝업이 바인딩으로 판정하는 기본 조작.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BasicKey {
    /// 닫기·접기 등 선언이 정한 Esc 동작.
    Escape,
    /// 선택한 항목 확정.
    Confirm,
    /// 접힌 팝업 펼치기.
    Expand,
    Up,
    Down,
    Left,
    Right,
    /// 다음 칸으로.
    Tab,
    /// 앞 칸으로.
    BackTab,
}

impl BasicKey {
    /// 고정 바인딩. 문법은 `KeybindingSettings`와 같다.
    pub(crate) fn bindings(self) -> &'static [&'static str] {
        match self {
            BasicKey::Escape => &["escape"],
            BasicKey::Confirm => &["enter"],
            BasicKey::Expand => &["enter", "space"],
            BasicKey::Up => &["up"],
            BasicKey::Down => &["down"],
            BasicKey::Left => &["left"],
            BasicKey::Right => &["right"],
            BasicKey::Tab => &["tab"],
            BasicKey::BackTab => &["shift+tab"],
        }
    }

    fn owned_bindings(self) -> Vec<String> {
        self.bindings().iter().map(|b| (*b).to_string()).collect()
    }

    /// egui 키 누름 하나가 이 조작인지. 입력을 바꾸지 않는다. 키 경로(`route_key`)가 쓴다.
    pub(crate) fn matches_egui(
        self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        super_held: bool,
    ) -> bool {
        tasty_key_match::binding_matches_egui(&self.owned_bindings(), key, modifiers, super_held)
    }
}

/// 키 누름 하나가 선언 `keyboard`의 소비 집합에 드는지. 집합 밖 키는 팝업이 쓰지 않으므로 조상으로 올라간다.
/// Ctrl·Cmd·Alt·Option(비-macOS의 Win·Super는 `super_held`)이 붙은 조합은 편집 바인딩이 아니면 어느 집합에도
/// 들지 않는다. `Keys`는 수정키 없는 키와 Shift+Tab, `Text`는 거기에 Shift+키를 더한다. 편집 바인딩
/// (`edits`의 복사·잘라내기·붙여넣기)에 맞는 누름은 수정키와 관계없이 `Text`의 집합 안이고 `Keys`의 집합 밖이다.
/// 판정에 쓰는 수정키는 `modifiers`다.
pub(crate) fn consumes_key(
    keyboard: super::PopupKeyboard,
    key: egui::Key,
    modifiers: egui::Modifiers,
    super_held: bool,
    edits: &EditBindings,
) -> bool {
    use super::PopupKeyboard;
    let edit = EditKey::ORDER.iter().any(|e| {
        tasty_key_match::binding_matches_egui(edits.combos(*e), key, modifiers, super_held)
    });
    let command = modifiers.ctrl || modifiers.alt || modifiers.mac_cmd || super_held;
    match keyboard {
        PopupKeyboard::None => false,
        PopupKeyboard::Keys => !edit && !command && (!modifiers.shift || key == egui::Key::Tab),
        PopupKeyboard::Text => edit || !command,
    }
}

/// 팝업이 소비한 키 하나.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConsumedKey {
    Basic(BasicKey),
    Key {
        key: egui::Key,
        modifiers: egui::Modifiers,
    },
}

/// 팝업 그리기가 키를 읽는 통로. 키를 받지 않는 인스턴스(`receives`가 거짓이거나 선언이 `None`)는
/// 모든 조회에서 빈 값을 받는다. 소비 집합 밖 키도 기본 조작·콘텐츠 고유 키 모두 빈 값이고 입력에 남는다.
/// 키 누름은 사건 하나씩 그 사건의 수정키로 판정한다. 소비한 키는 기록해 관리자가 남은 키를
/// 선언대로 처리하거나 조상으로 올릴 때 쓴다.
pub(crate) struct PopupKeys {
    ctx: egui::Context,
    receives: bool,
    keyboard: super::PopupKeyboard,
    super_held: bool,
    edits: EditBindings,
    consumed: Vec<ConsumedKey>,
    /// 이 인스턴스가 펼친 묶음 안이라 본문이 쓰지 않은 Esc 를 중앙 단계가 다시 접는다(`refolding`).
    refolds: bool,
    /// 이번 몫의 결속이 이 인스턴스의 `Bound::Popup` 인지(`FrameKeys::bound`). 같은 노드의 누름 끊기도 결속을 만들고
    /// `Bound::NoPopup` 은 결속이 아니다.
    routed: bool,
    /// 창이 사건 시점에 수식키 없는 Esc 로 정해 이 인스턴스에 넘긴 Esc 가 이번 패스에 있는지(`body_keys::hand_escape`).
    handed_escape: bool,
}

/// 판정한 키 누름 하나와 그 판정에 쓴 수식키.
#[derive(Clone, Copy)]
struct Press {
    key: egui::Key,
    modifiers: egui::Modifiers,
    super_held: bool,
}

impl PopupKeys {
    /// `receives`는 이 인스턴스가 이번 프레임에 키를 받는지다. 누가 받는지는 호출자가 정한다. `edits`는 그 창에
    /// 적용된 편집 바인딩이다.
    pub(crate) fn new(
        ctx: &egui::Context,
        receives: bool,
        keyboard: super::PopupKeyboard,
        edits: &EditBindings,
    ) -> Self {
        Self {
            ctx: ctx.clone(),
            receives: receives && keyboard != super::PopupKeyboard::None,
            keyboard,
            super_held: tasty_key_match::super_held(ctx),
            edits: edits.clone(),
            consumed: Vec::new(),
            refolds: false,
            routed: false,
            handed_escape: false,
        }
    }

    /// 이번 그리기 몫이 이 인스턴스에 결속됐는지(`FrameKeys::bound`) 싣는다. 관리자가 본문을 부르기 전에 정한다.
    pub(crate) fn with_window_route(mut self, routed: bool) -> Self {
        self.routed = routed;
        self
    }

    /// 창이 이번 패스에 이 인스턴스로 넘긴 Esc 가 있는지 싣는다. 관리자가 본문을 부르기 전에 정한다.
    pub(crate) fn with_handed_escape(mut self, handed: bool) -> Self {
        self.handed_escape = handed;
        self
    }

    /// 이번 그리기 몫이 이 인스턴스에 결속됐는지. 결속된 몫의 키는 본문 위젯이 아직 egui 포커스를 얻기 전(연 뒤 첫
    /// 그리기)에도 이 본문 것이다.
    pub(crate) fn routed_by_window(&self) -> bool {
        self.routed
    }

    /// 이 인스턴스가 펼친 묶음 안인지 싣는다. 관리자가 본문을 부르기 전에 정한다.
    pub(crate) fn refolding(mut self, expanded: bool) -> Self {
        self.refolds = expanded;
        self
    }

    /// 이 패스 Esc 가 펼친 묶음을 다시 접는 몫인지. 참이면 본문은 Esc 로 취소·닫기 같은 자기 동작을 하지 않고 남겨,
    /// 중앙 단계가 묶음 뿌리부터 접게 한다.
    pub(crate) fn escape_refolds(&self) -> bool {
        self.refolds
    }

    /// 아무 키도 주지 않는 통로.
    pub(crate) fn none(ctx: &egui::Context) -> Self {
        Self::new(
            ctx,
            false,
            super::PopupKeyboard::None,
            &EditBindings::default(),
        )
    }

    /// 이 통로가 키를 주는지.
    pub(crate) fn receives(&self) -> bool {
        self.receives
    }

    /// 사건이 기본 조작 `basic`의 누름이면 그 키와 판정 수식키. 프레임 수정키는 보지 않는다. 창이 결속한 몫의 Esc
    /// 누름은 사건 시점 판정을 따른다. 창이 수식키 없는 Esc 로 정해 넘긴 누름만 `BasicKey::Escape` 이고 그 판정은
    /// 수식키 없음이다. 넘기지 않은 Esc 누름(비-macOS Win·Super 를 누른 채 친 Esc 등)은 그리기 시점에 그 키를 뗐어도
    /// 기본 조작이 아니다.
    fn basic_press(&self, basic: BasicKey, event: &egui::Event) -> Option<Press> {
        if self.routed && super::body_keys::is_escape_press(event) {
            return (basic == BasicKey::Escape && self.handed_escape).then_some(Press {
                key: egui::Key::Escape,
                modifiers: egui::Modifiers::NONE,
                super_held: false,
            });
        }
        match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if tasty_key_match::binding_matches_egui(
                &basic.owned_bindings(),
                *key,
                *modifiers,
                self.super_held,
            ) =>
            {
                Some(Press {
                    key: *key,
                    modifiers: *modifiers,
                    super_held: self.super_held,
                })
            }
            _ => None,
        }
    }

    /// 키 누름 하나가 이 통로 선언의 소비 집합에 드는지.
    fn in_set(&self, press: Press) -> bool {
        consumes_key(
            self.keyboard,
            press.key,
            press.modifiers,
            press.super_held,
            &self.edits,
        )
    }

    /// 기본 조작이 이번 프레임에 눌렸는지. 소비 집합 안의 누름만 센다. 소비하지 않는다.
    pub(crate) fn pressed(&self, basic: BasicKey) -> bool {
        self.receives
            && self.ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| self.basic_press(basic, e).is_some_and(|p| self.in_set(p)))
            })
    }

    /// 기본 조작 누름 가운데 소비 집합 안의 것을 소비한다. 소비했으면 기록하고 참이다. 지운 누름 바로 뒤의
    /// 글자 사건도 지운다(`tasty_key_match::consume_binding_egui`와 같은 규칙).
    pub(crate) fn consume(&mut self, basic: BasicKey) -> bool {
        if !self.receives {
            return false;
        }
        let hit = self.ctx.input_mut(|i| {
            let mut hit = false;
            let mut n = 0;
            while n < i.events.len() {
                let take = self
                    .basic_press(basic, &i.events[n])
                    .is_some_and(|p| self.in_set(p));
                if !take {
                    n += 1;
                    continue;
                }
                i.events.remove(n);
                hit = true;
                if matches!(i.events.get(n), Some(egui::Event::Text(_))) {
                    i.events.remove(n);
                }
            }
            hit
        });
        if hit {
            self.consumed.push(ConsumedKey::Basic(basic));
        }
        hit
    }

    /// 콘텐츠 고유 키(숫자·글자 단축키)를 소비한다. 소비 집합 밖 조합은 소비하지 않는다.
    pub(crate) fn consume_key(&mut self, modifiers: egui::Modifiers, key: egui::Key) -> bool {
        if !self.receives
            || !consumes_key(self.keyboard, key, modifiers, self.super_held, &self.edits)
        {
            return false;
        }
        // egui `consume_key`는 Alt·Shift가 더 붙은 누름도 잡는다. 그 조합은 집합 밖이라 올라가야 하므로
        // 수식키가 정확히 같은 누름만 지운다.
        let hit = self.ctx.input_mut(|i| {
            let before = i.events.len();
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { key: k, modifiers: m, pressed: true, .. }
                        if *k == key && *m == modifiers
                )
            });
            i.events.len() != before
        });
        if hit {
            self.consumed.push(ConsumedKey::Key { key, modifiers });
        }
        hit
    }

    /// 이번 프레임에 소비한 키. 소비한 순서다.
    pub(crate) fn consumed(&self) -> &[ConsumedKey] {
        &self.consumed
    }

    /// 콘텐츠 고유 글자 단축키를 물리 키로 소비한다. 사건의 `physical_key` 가 `candidates` 에 드는 누름 가운데 소비
    /// 집합 안의 것을 모두 지우고 사건 순서상 마지막 적중의 물리 키를 돌려준다. 후보와는 물리 키로 맞추고(배열·IME와
    /// 무관하게 같은 자리), 집합 판정은 다른 통로 함수처럼 사건의 `key`·수식키·`super_held` 로 한다.
    pub(crate) fn consume_physical(&mut self, candidates: &[egui::Key]) -> Option<egui::Key> {
        if !self.receives {
            return None;
        }
        let (keyboard, super_held, edits) = (self.keyboard, self.super_held, &self.edits);
        let mut last = None;
        let mut taken = Vec::new();
        self.ctx.input_mut(|i| {
            i.events.retain(|e| {
                let egui::Event::Key {
                    key,
                    physical_key: Some(physical),
                    pressed: true,
                    modifiers,
                    ..
                } = e
                else {
                    return true;
                };
                if !candidates.contains(physical)
                    || !consumes_key(keyboard, *key, *modifiers, super_held, edits)
                {
                    return true;
                }
                last = Some(*physical);
                taken.push(ConsumedKey::Key {
                    key: *key,
                    modifiers: *modifiers,
                });
                false
            });
        });
        self.consumed.extend(taken);
        last
    }

    /// 노드 키맵 읽기: `modifiers`+`key` 누름 하나를 egui `consume_key`와 같은 매처(`matches_logically`, 패턴에
    /// 없는 Shift·Alt 는 허용)로 소비한다. 소비 집합 판정을 거치지 않는다. 폴더 키처럼 집합 밖 조합(Alt+↑)과 수식키를
    /// 너그럽게 받는 조작이 그 노드의 키맵으로 쓴다. 소비했으면 패턴을 기록한다.
    pub(crate) fn consume_keymap_key(
        &mut self,
        modifiers: egui::Modifiers,
        key: egui::Key,
    ) -> bool {
        if !self.receives {
            return false;
        }
        let hit = self.ctx.input_mut(|i| i.consume_key(modifiers, key));
        if hit {
            self.consumed.push(ConsumedKey::Key { key, modifiers });
        }
        hit
    }

    /// 노드 키맵 읽기: 설정 바인딩 목록(`bindings`) 가운데 하나에 맞는 누름을 단축키 매처(`consume_binding_egui`)로
    /// 소비한다. 소비 집합 판정을 거치지 않는다. 검색 띠의 find 처럼 사용자가 정한 단축키를 그 노드가 쓴다.
    pub(crate) fn consume_keymap_binding(&mut self, bindings: &[String]) -> bool {
        if !self.receives {
            return false;
        }
        let super_held = self.super_held;
        self.ctx
            .input_mut(|i| tasty_key_match::consume_binding_egui(bindings, i, super_held))
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "keys_passage_tests.rs"]
mod passage_tests;
