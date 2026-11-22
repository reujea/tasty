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

/// winit 수식키를 창의 Esc 판정 값(`tasty_key_match::KeyMods`)으로 바꾼다. 축은 `tasty_key_match::token_axes` 와
/// 같다: macOS 는 Super 가 Command 이고, 다른 OS 는 Win·Super 다.
pub(crate) fn window_key_mods(mods: winit::keyboard::ModifiersState) -> tasty_key_match::KeyMods {
    let macos = cfg!(target_os = "macos");
    tasty_key_match::KeyMods {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key(),
        command: macos && mods.super_key(),
        super_: !macos && mods.super_key(),
    }
}

/// egui 수식키를 창의 Esc 판정 값으로 바꾼다(시험 입구 `WindowInput::escape_with`). egui 는 비-macOS 의 Win·Super 를
/// 담지 않으므로 그 값은 `super_held`(`tasty_key_match::super_held`)로 받는다. 비-macOS 의 `Modifiers::command` 는
/// Ctrl 과 같은 값이라 쓰지 않는다.
#[cfg(test)]
pub(crate) fn egui_window_key_mods(
    mods: egui::Modifiers,
    super_held: bool,
) -> tasty_key_match::KeyMods {
    let macos = cfg!(target_os = "macos");
    tasty_key_match::KeyMods {
        shift: mods.shift,
        ctrl: mods.ctrl,
        alt: mods.alt,
        command: macos && mods.mac_cmd,
        super_: !macos && super_held,
    }
}

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
/// 모든 조회에서 빈 값을 받는다. 소비 집합 밖 키도 기본 조작·콘텐츠 고유 키 모두 빈 값이고 입력에 남는다.
/// 키 누름은 사건 하나씩 그 사건의 수정키로 판정한다. 소비한 키는 기록해 관리자가 남은 키를
/// 선언대로 처리하거나 조상으로 올릴 때 쓴다.
///
/// 이번 패스 키 주인의 통로는 Esc 표지(`escape_marks`)를 가진다. 그 통로의 `BasicKey::Escape`는 표지 수식키가 없는
/// 누름만 맞고, 소비 집합도 표지 값으로 판정한다. 창이 사건 시점에 정한 판정을 그대로 쓴다.
pub(crate) struct PopupKeys {
    ctx: egui::Context,
    receives: bool,
    keyboard: super::PopupKeyboard,
    super_held: bool,
    edits: EditBindings,
    consumed: Vec<ConsumedKey>,
    escape_marks: Option<super::body_keys::EscapeMarks>,
}

/// 판정한 키 누름 하나와 그 판정에 쓴 수식키.
#[derive(Clone, Copy)]
struct Press {
    key: egui::Key,
    modifiers: egui::Modifiers,
    super_held: bool,
}

/// 창 판정 수식키의 egui 꼴. 비-macOS Win·Super 는 egui 수식키에 없으므로 `super_` 는 따로 쓴다.
fn egui_modifiers(mods: tasty_key_match::KeyMods) -> egui::Modifiers {
    egui::Modifiers {
        alt: mods.alt,
        ctrl: mods.ctrl,
        shift: mods.shift,
        mac_cmd: mods.command,
        command: if cfg!(target_os = "macos") {
            mods.command
        } else {
            mods.ctrl
        },
    }
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
            escape_marks: None,
        }
    }

    /// 이번 패스 키 주인 통로로 만든다. `marks`는 패스 `events` 의 Esc 누름마다 창이 정한 표지다.
    pub(crate) fn with_escape_marks(mut self, marks: super::body_keys::EscapeMarks) -> Self {
        self.escape_marks = Some(marks);
        self
    }

    /// 남은 Esc 표지를 돌려준다(소비한 누름의 표지는 지웠다). 관리자가 넘긴 Esc 를 거둘 때 쓴다.
    pub(crate) fn take_escape_marks(&mut self) -> Option<super::body_keys::EscapeMarks> {
        self.escape_marks.take()
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

    /// 사건이 기본 조작 `basic`의 누름이면 그 키와 판정 수식키. 프레임 수정키는 보지 않는다. `escape` 는 그 사건이
    /// Esc 누름일 때 패스 `events` 의 Esc 누름 가운데 순번이다. 표지가 있는 통로의 Esc 는 사건 값이 아니라 그 순번의
    /// 표지로 판정한다.
    fn basic_press(
        &self,
        basic: BasicKey,
        event: &egui::Event,
        escape: Option<usize>,
    ) -> Option<Press> {
        if let (Some(marks), Some(n)) = (&self.escape_marks, escape) {
            let mark = marks.get(n).copied().flatten()?;
            return (basic == BasicKey::Escape && tasty_key_match::is_window_escape(true, mark))
                .then_some(Press {
                    key: egui::Key::Escape,
                    modifiers: egui_modifiers(mark),
                    super_held: mark.super_,
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
                let mut escapes = 0;
                i.events.iter().any(|e| {
                    let escape = super::body_keys::is_escape_press(e).then(|| {
                        escapes += 1;
                        escapes - 1
                    });
                    self.basic_press(basic, e, escape)
                        .is_some_and(|p| self.in_set(p))
                })
            })
    }

    /// 기본 조작 누름 가운데 소비 집합 안의 것을 소비한다. 소비했으면 기록하고 참이다. 지운 누름 바로 뒤의
    /// 글자 사건도 지운다(`tasty_key_match::consume_binding_egui`와 같은 규칙). 지운 Esc 누름의 표지는 끝에 함께
    /// 지운다(순번은 지우기 전 값이다).
    pub(crate) fn consume(&mut self, basic: BasicKey) -> bool {
        if !self.receives {
            return false;
        }
        let mut removed_escapes = Vec::new();
        let hit = self.ctx.input_mut(|i| {
            let mut hit = false;
            let mut n = 0;
            let mut escapes = 0;
            while n < i.events.len() {
                let escape = super::body_keys::is_escape_press(&i.events[n]).then(|| {
                    escapes += 1;
                    escapes - 1
                });
                let take = self
                    .basic_press(basic, &i.events[n], escape)
                    .is_some_and(|p| self.in_set(p));
                if !take {
                    n += 1;
                    continue;
                }
                i.events.remove(n);
                removed_escapes.extend(escape);
                hit = true;
                if matches!(i.events.get(n), Some(egui::Event::Text(_))) {
                    i.events.remove(n);
                }
            }
            hit
        });
        if let Some(marks) = &mut self.escape_marks {
            for n in removed_escapes.into_iter().rev() {
                if n < marks.len() {
                    marks.remove(n);
                }
            }
        }
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
