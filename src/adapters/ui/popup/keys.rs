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

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
