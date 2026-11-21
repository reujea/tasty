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
    pub(crate) const ALL: [BasicKey; 9] = [
        BasicKey::Escape,
        BasicKey::Confirm,
        BasicKey::Expand,
        BasicKey::Up,
        BasicKey::Down,
        BasicKey::Left,
        BasicKey::Right,
        BasicKey::Tab,
        BasicKey::BackTab,
    ];

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

    /// winit 키 사건이 이 조작인지. 키 경로(단축키 판정 앞)가 쓴다.
    pub(crate) fn matches(
        self,
        key: &winit::keyboard::Key,
        mods: winit::keyboard::ModifiersState,
    ) -> bool {
        tasty_key_match::matches_any_binding(&self.owned_bindings(), key, mods)
    }

    /// 이번 egui 프레임에 이 조작이 눌렸는지. `super_held`는 [`tasty_key_match::super_held`]로 구한다.
    pub(crate) fn pressed(self, input: &egui::InputState, super_held: bool) -> bool {
        tasty_key_match::any_binding_pressed_egui(&self.owned_bindings(), input, super_held)
    }

    /// 이번 egui 프레임의 이 조작 누름을 입력에서 지우고, 있었는지 돌려준다.
    pub(crate) fn consume(self, input: &mut egui::InputState, super_held: bool) -> bool {
        tasty_key_match::consume_binding_egui(&self.owned_bindings(), input, super_held)
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

/// 글자·IME 조합과 egui 편집 사건(`Event::Paste`·`Copy`·`Cut`)을 소비하는지. `Text`만 소비한다. 키 누름으로
/// 오는 편집 바인딩은 [`consumes_key`]가 판정한다.
pub(crate) fn consumes_text(keyboard: super::PopupKeyboard) -> bool {
    keyboard == super::PopupKeyboard::Text
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
/// 모든 조회에서 빈 값을 받는다. 소비 집합 밖 키도 빈 값이다. 소비한 키는 기록해 관리자가 남은 키를
/// 선언대로 처리하거나 조상으로 올릴 때 쓴다.
pub(crate) struct PopupKeys {
    ctx: egui::Context,
    receives: bool,
    keyboard: super::PopupKeyboard,
    super_held: bool,
    edits: EditBindings,
    consumed: Vec<ConsumedKey>,
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
        }
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

    /// 기본 조작이 이번 프레임에 눌렸는지. 소비하지 않는다.
    pub(crate) fn pressed(&self, basic: BasicKey) -> bool {
        self.receives && self.ctx.input(|i| basic.pressed(i, self.super_held))
    }

    /// 기본 조작 누름을 소비한다. 소비했으면 기록하고 참이다.
    pub(crate) fn consume(&mut self, basic: BasicKey) -> bool {
        if !self.receives {
            return false;
        }
        let super_held = self.super_held;
        let hit = self.ctx.input_mut(|i| basic.consume(i, super_held));
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

    /// 이번 프레임에 들어온 글자. 글자를 소비하는 선언(`Text`)이 아니면 비어 있다.
    pub(crate) fn text(&self) -> String {
        if !self.receives || !consumes_text(self.keyboard) {
            return String::new();
        }
        self.ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect()
        })
    }

    /// 이번 프레임에 소비한 키. 소비한 순서다.
    pub(crate) fn consumed(&self) -> &[ConsumedKey] {
        &self.consumed
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "keys_passage_tests.rs"]
mod passage_tests;
