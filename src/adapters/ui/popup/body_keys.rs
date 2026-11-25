//! 팝업 본문의 키 입력 문. 키(Escape 포함)·글자·붙여넣기·복사·잘라내기·IME 입력은 이번 프레임 키 주인 본문만
//! 읽는다. 본문이 egui 입력을 직접 읽든(`key_pressed`·`key_down`·`consume_key`·`events`·`raw.events`) egui 위젯이
//! 키 포커스로 읽든, 키 주인이 아닌 본문을 그리는 동안에는 그 사건과 눌린 키 상태가 입력에 없다. 본문을 그린 뒤 원래 자리에 돌려놓아 뒤에 그리는
//! 키 주인 본문과 plugin 팝업 렌더가 받는다.

/// 키 주인만 읽는 입력 사건인지.
fn is_key_input(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key { .. }
            | egui::Event::Text(_)
            | egui::Event::Paste(_)
            | egui::Event::Copy
            | egui::Event::Cut
            | egui::Event::Ime(_)
    )
}

/// `takes_keys`가 거짓이면 키 입력 사건(`events`·`raw.events` 둘 다)과 눌린 키 상태(`keys_down`, `key_down` 이
/// 읽는다)를 뺀 채 `body`를 부르고, 끝나면 뺀 사건을 원래 순서 자리에, 눌린 키 상태를 그대로 돌려놓는다.
pub(crate) fn with_body_keys<R>(
    ctx: &egui::Context,
    takes_keys: bool,
    body: impl FnOnce() -> R,
) -> R {
    if takes_keys {
        return body();
    }
    let (hidden, hidden_raw, keys_down) = ctx.input_mut(|i| {
        (
            hide_key_input(&mut i.events),
            hide_key_input(&mut i.raw.events),
            std::mem::take(&mut i.keys_down),
        )
    });
    let out = body();
    ctx.input_mut(|i| {
        restore_key_input(&mut i.events, hidden);
        restore_key_input(&mut i.raw.events, hidden_raw);
        i.keys_down = keys_down;
    });
    out
}

/// `list` 에서 키 입력 사건을 빼고 원래 자리와 함께 돌려준다.
fn hide_key_input(list: &mut Vec<egui::Event>) -> Vec<(usize, egui::Event)> {
    let mut hidden = Vec::new();
    let mut kept = Vec::with_capacity(list.len());
    for (at, event) in std::mem::take(list).into_iter().enumerate() {
        if is_key_input(&event) {
            hidden.push((at, event));
        } else {
            kept.push(event);
        }
    }
    *list = kept;
    hidden
}

/// [`hide_key_input`]이 뺀 사건을 원래 자리에 돌려놓는다.
fn restore_key_input(list: &mut Vec<egui::Event>, hidden: Vec<(usize, egui::Event)>) {
    for (at, event) in hidden {
        let at = at.min(list.len());
        list.insert(at, event);
    }
}

/// 창이 처리해 egui 입력에서 뺀 원래 Esc 사건과 그 자리. egui 가 그 Esc 를 패스 시작에서 처리했을 때와 같은 모습으로
/// 키 주인에게 되넣는 데 쓴다.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HandedEscape {
    /// 이번 프레임 사건보다 앞선 Escape 눌림 상태(직전 패스 끝의 `keys_down`). egui 의 반복 판정 출발점이다.
    held_before: bool,
    /// 뺀 사건과 그 앞에 있던 빼지 않은 사건 수(원래 자리). 수식키·눌림 여부는 원래 사건 그대로다.
    events: Vec<(usize, egui::Event)>,
}

impl HandedEscape {
    /// 창이 이번 egui 입력에서 Esc 를 뺄 때 부른다. 프레임의 첫 몫이면 `held_before` 를 기록한다.
    pub(crate) fn push(&mut self, held_before: bool, taken: Vec<(usize, egui::Event)>) {
        if self.events.is_empty() {
            self.held_before = held_before;
        }
        self.events.extend(taken);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 이 몫을 실은 egui 패스가 시작할 때의 Escape 눌림 상태로 반복 판정의 출발점을 맞춘다.
    pub(crate) fn start_from(&mut self, held_before: bool) {
        self.held_before = held_before;
    }

    /// 앞의 `by` 사건이 이번 그리기 몫으로 빠졌다. 원래 자리를 그만큼 앞당긴다.
    pub(crate) fn shifted_back(mut self, by: usize) -> Self {
        for (at, _) in &mut self.events {
            *at = at.saturating_sub(by);
        }
        self
    }
}

/// 창이 처리해 egui 입력에서 뺀 원래 Esc 사건(`handed`, 수식키 포함)을 키 주인을 그리는 동안만 넣는다. host 팝업
/// 층에서는 묶인 키 주인 본문만 읽는다(다른 본문은 문이 가린다). plugin 렌더는 키 주인 팝업에만 키를 보낸다. 팝업 층
/// 보다 먼저 그린 화면과 egui 의 패스 시작 처리(키 포커스 해제)는 이 Esc 를 보지 않는다. 그린 뒤
/// [`withdraw_escape`]가 키 주인이 읽지 않고 남긴 사건을 뺀다.
///
/// 사건을 원래 자리(같은 프레임의 뗌보다 앞)에 `events`·`raw.events` 둘 다 넣고, egui 패스 시작과 같은 규칙으로
/// Escape 의 반복 여부와 `keys_down` 을 정한다(눌림은 이미 눌려 있었으면 반복, 뗌은 놓음). 넣은 사건(반복 여부를
/// 정한 값)을 돌려준다. [`withdraw_escape`]에 넘긴다.
pub(crate) fn hand_escape(ctx: &egui::Context, handed: &HandedEscape) -> PutEscape {
    if handed.is_empty() {
        return PutEscape::default();
    }
    ctx.input_mut(|i| {
        let (events, down) = place_escape(&mut i.events, handed);
        let (raw, _) = place_escape(&mut i.raw.events, handed);
        if down {
            i.keys_down.insert(egui::Key::Escape);
        } else {
            i.keys_down.remove(&egui::Key::Escape);
        }
        PutEscape { events, raw }
    })
}

/// [`hand_escape`]가 넣은 사건. [`withdraw_escape`]가 남은 것을 뺀다. 넣은 사건마다 그 목록 Esc 누름 가운데 순번과
/// 넣은 값을 둔다.
#[derive(Debug, Default)]
pub(crate) struct PutEscape {
    events: Vec<(usize, egui::Event)>,
    raw: Vec<(usize, egui::Event)>,
}

impl PutEscape {
    /// 넘긴 사건이 있는지.
    pub(crate) fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// `events` 에 넣은 사건마다 그 목록 Esc 누름 가운데 순번과 넣은 값. plugin 렌더가 그 누름만 wire 에서 건너뛸 때
    /// 쓴다(값은 확인에만 쓴다).
    pub(crate) fn handed_presses(&self) -> &[(usize, egui::Event)] {
        &self.events
    }
}

/// `list` 에서 `n` 번째 Esc 누름의 자리.
fn escape_press_at(list: &[egui::Event], n: usize) -> Option<usize> {
    list.iter()
        .enumerate()
        .filter(|(_, e)| is_escape_press(e))
        .nth(n)
        .map(|(at, _)| at)
}

/// `list` 의 원래 자리에 넘긴 사건을 넣고 egui 패스 시작 규칙으로 Escape 반복 여부를 정한다. 넣은 사건(그 목록 Esc 누름
/// 가운데 순번과 값)과 끝의 Escape 눌림 상태를 돌려준다.
fn place_escape(
    list: &mut Vec<egui::Event>,
    handed: &HandedEscape,
) -> (Vec<(usize, egui::Event)>, bool) {
    let mut handed_at = Vec::with_capacity(handed.events.len());
    for (n, (slot, event)) in handed.events.iter().enumerate() {
        let at = (slot + n).min(list.len());
        list.insert(at, event.clone());
        handed_at.push(at);
    }
    let mut down = handed.held_before;
    for (at, event) in list.iter_mut().enumerate() {
        if let egui::Event::Key {
            key: egui::Key::Escape,
            pressed,
            repeat,
            ..
        } = event
        {
            if *pressed {
                if handed_at.contains(&at) {
                    *repeat = down;
                }
                down = true;
            } else {
                down = false;
            }
        }
    }
    let put = handed_at
        .iter()
        .map(|&at| {
            let n = list[..at].iter().filter(|e| is_escape_press(e)).count();
            (n, list[at].clone())
        })
        .collect();
    (put, down)
}

/// [`hand_escape`]의 뒤쪽. 키 주인이 읽지 않고 남긴 넘긴 누름을 넣은 자리로 뺀다. `consumed` 면 키 주인 통로가 처리된
/// 목록(`events`)에서 그 누름을 이미 소비했으므로 그 목록은 두고, 원시 목록(`raw.events`)에서는 넣은 자리를 뺀다. 값으로
/// 고르지 않으므로 넘기지 않은 같은 값의 누름은 남는다. 눌린 키 상태는 그대로 둔다(egui 가 이 Esc 를 처리했을 때처럼
/// 다음 프레임 판정의 출발점이 된다).
pub(crate) fn withdraw_escape(ctx: &egui::Context, put: &PutEscape, consumed: bool) {
    if put.events.is_empty() && put.raw.is_empty() {
        return;
    }
    ctx.input_mut(|i| {
        if !consumed {
            let events: Vec<usize> = put.events.iter().map(|(n, _)| *n).collect();
            remove_escape_presses(&mut i.events, &events, Some(&put.events));
        }
        let raw: Vec<usize> = put.raw.iter().map(|(n, _)| *n).collect();
        remove_escape_presses(&mut i.raw.events, &raw, Some(&put.raw));
    });
}

/// `list` 에서 Esc 누름 순번 `ordinals` 의 누름을 뺀다. `put` 이 있으면 뺀 사건이 넣은 값과 같은지 본다.
fn remove_escape_presses(
    list: &mut Vec<egui::Event>,
    ordinals: &[usize],
    put: Option<&[(usize, egui::Event)]>,
) {
    let at: Vec<usize> = ordinals
        .iter()
        .filter_map(|n| escape_press_at(list, *n))
        .collect();
    for (k, &a) in at.iter().enumerate().rev() {
        let event = list.remove(a);
        if let Some(put) = put {
            debug_assert_eq!(Some(&event), put.get(k).map(|(_, e)| e));
        }
    }
}

/// 창이 처리하면 egui 입력에서 빼는 Esc 누름인지.
pub(crate) fn is_escape_press(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: egui::Key::Escape,
            pressed: true,
            ..
        }
    )
}

/// egui 로 보내지 않은 Esc 뗌을 egui 의 눌린 키 상태에 반영한다. 넘긴 Esc 누름은 egui 의 Escape 눌림으로 남으므로,
/// 그 뗌이 터미널 등 egui 밖으로 가도 다음 패스의 반복 판정(`Gpu::egui_escape_held`)이 낡은 눌림을 읽지 않는다.
pub(crate) fn release_escape(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        i.keys_down.remove(&egui::Key::Escape);
    });
}

#[cfg(test)]
mod tests {
    use super::{HandedEscape, hand_escape, with_body_keys, withdraw_escape};

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Given 키·글자·포인터 사건이 섞인 프레임, When 키 주인이 아닌 본문을 그리면, Then 그 본문은 키·글자와 눌린 키
    /// 상태를 보지 못하고 포인터 사건은 본다. 그린 뒤에는 같은 순서의 사건이 다시 있어 키 주인 본문이 읽는다.
    #[test]
    fn only_the_key_owner_body_sees_key_input_and_the_rest_is_kept_in_order() {
        let ctx = egui::Context::default();
        let moved = egui::Event::PointerMoved(egui::pos2(3.0, 4.0));
        let events = vec![
            key(egui::Key::Escape),
            moved.clone(),
            egui::Event::Text("a".into()),
            key(egui::Key::Enter),
        ];
        drop(ctx.run(
            egui::RawInput {
                events: events.clone(),
                ..Default::default()
            },
            |ctx| {
                assert!(
                    ctx.input(|i| i.key_down(egui::Key::Enter)),
                    "Enter 가 눌려 있다"
                );
                let seen = with_body_keys(ctx, false, || {
                    ctx.input(|i| {
                        (
                            i.key_pressed(egui::Key::Escape),
                            i.key_pressed(egui::Key::Enter),
                            i.key_down(egui::Key::Enter),
                            i.events.clone(),
                            i.raw.events.clone(),
                        )
                    })
                });
                assert_eq!(
                    seen,
                    (
                        false,
                        false,
                        false,
                        vec![moved.clone()],
                        vec![moved.clone()]
                    )
                );
                assert_eq!(
                    ctx.input(|i| i.raw.events.clone()),
                    events,
                    "raw 사건도 원래 순서로 돌려놓는다"
                );
                assert!(
                    ctx.input(|i| i.key_down(egui::Key::Enter)),
                    "눌린 키 상태를 돌려놓는다"
                );
                let owner = with_body_keys(ctx, true, || {
                    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                });
                assert!(owner, "키 주인 본문은 읽는다");
                assert_eq!(
                    ctx.input(|i| i.events.clone()),
                    vec![
                        key(egui::Key::Escape),
                        moved.clone(),
                        egui::Event::Text("a".into())
                    ],
                    "키 주인이 소비한 Enter 만 빠진다"
                );
            },
        ));
    }

    /// Given 창이 뺀 Esc 누름(그 앞에 포인터 사건 하나, 뒤에 같은 프레임의 뗌), When 키 주인을 그리는 동안 되넣으면,
    /// Then 누름은 원래 자리(뗌 앞)에 `events`·`raw.events` 둘 다 들어가고, 키 주인 본문은 그 프레임에 Escape 를 눌린
    /// 키로 본다. 키 주인이 아닌 본문은 보지 못한다. 끝나면 남은 사건을 빼고 눌린 키 상태는 egui 가 처리했을 때 값
    /// (뗌 뒤라 놓임)으로 둔다.
    #[test]
    fn a_handed_escape_goes_back_to_its_place_and_is_held_for_the_owner_body() {
        let ctx = egui::Context::default();
        let moved = egui::Event::PointerMoved(egui::pos2(3.0, 4.0));
        let release = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        for (later, held_after) in [(vec![], true), (vec![release.clone()], false)] {
            let mut handed = HandedEscape::default();
            handed.push(false, vec![(1, key(egui::Key::Escape))]);
            let mut events = vec![moved.clone()];
            events.extend(later.clone());
            drop(ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    let put = hand_escape(ctx, &handed);
                    let (owner, other, seen, raw) = {
                        let owner = with_body_keys(ctx, true, || {
                            ctx.input(|i| {
                                (
                                    i.key_pressed(egui::Key::Escape),
                                    i.key_down(egui::Key::Escape),
                                )
                            })
                        });
                        let other = with_body_keys(ctx, false, || {
                            ctx.input(|i| {
                                (
                                    i.key_pressed(egui::Key::Escape),
                                    i.key_down(egui::Key::Escape),
                                )
                            })
                        });
                        let (seen, raw) = ctx.input(|i| (i.events.clone(), i.raw.events.clone()));
                        (owner, other, seen, raw)
                    };
                    withdraw_escape(ctx, &put, false);
                    let mut expected = vec![moved.clone(), key(egui::Key::Escape)];
                    expected.extend(later.clone());
                    assert_eq!(seen, expected, "원래 자리");
                    assert_eq!(raw, expected, "raw 도 원래 자리");
                    assert_eq!(owner, (true, held_after), "키 주인 본문");
                    assert_eq!(other, (false, false), "키 주인이 아닌 본문");
                    let mut left = vec![moved.clone()];
                    left.extend(later.clone());
                    assert_eq!(ctx.input(|i| i.events.clone()), left, "끝나면 뺀다");
                    assert_eq!(
                        ctx.input(|i| i.key_down(egui::Key::Escape)),
                        held_after,
                        "눌린 키 상태는 egui 가 처리했을 때 값"
                    );
                },
            ));
            // 다음 시도는 놓인 상태에서 시작한다.
            drop(ctx.run(
                egui::RawInput {
                    events: vec![release.clone()],
                    ..Default::default()
                },
                |_| {},
            ));
        }
    }

    fn escape(pressed: bool, repeat: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed,
            repeat,
            modifiers,
        }
    }

    /// 넘긴 bare Esc 를 포인터 이동 뒤 자리에 넣고, 그 뒤에 넘기지 않은 Shift+Esc 누름이 있는 패스에서 `consume` 이면
    /// 키 주인 통로가 넘긴 누름을 소비한 뒤 거둔다. 처리된 목록과 원시 목록을 돌려준다.
    fn withdraw_after(consume: bool) -> (Vec<egui::Event>, Vec<egui::Event>) {
        let moved = egui::Event::PointerMoved(egui::pos2(3.0, 4.0));
        let other = escape(true, false, egui::Modifiers::SHIFT);
        let ctx = egui::Context::default();
        let mut lists = (Vec::new(), Vec::new());
        drop(ctx.run(
            egui::RawInput {
                events: vec![moved.clone(), other.clone()],
                ..Default::default()
            },
            |ctx| {
                let mut handed = HandedEscape::default();
                handed.push(false, vec![(1, escape(true, false, egui::Modifiers::NONE))]);
                let put = hand_escape(ctx, &handed);
                assert_eq!(put.handed_presses().len(), 1);
                if consume {
                    let mut keys = crate::adapters::ui::popup::keys::PopupKeys::new(
                        ctx,
                        true,
                        crate::adapters::ui::popup::PopupKeyboard::Keys,
                        &Default::default(),
                    );
                    assert!(
                        keys.consume(crate::adapters::ui::popup::keys::BasicKey::Escape),
                        "키 주인이 넘긴 누름을 소비한다"
                    );
                }
                withdraw_escape(ctx, &put, consume);
                lists = ctx.input(|i| (i.events.clone(), i.raw.events.clone()));
            },
        ));
        assert_eq!(lists.0, vec![moved.clone(), other.clone()], "events");
        assert_eq!(lists.1, vec![moved, other], "raw");
        lists
    }

    /// Given 넘긴 bare Esc 와 그 뒤의 넘기지 않은 Esc 누름, When 키 주인 본문이 넘긴 Esc 를 소비한 뒤 거두면,
    /// Then 처리된 목록에서는 소비한 누름을 다시 빼지 않아 다른 Esc 가 남고, 원시 목록에서는 넣은 누름만 빠진다.
    #[test]
    fn an_escape_the_body_used_is_not_withdrawn_and_another_escape_stays() {
        withdraw_after(true);
    }

    /// Given 같은 패스, When 키 주인 본문이 넘긴 Esc 를 쓰지 않고 그린 뒤 거두면, Then 두 목록 모두 넣은
    /// 누름만 빠지고 다른 Esc 는 남는다.
    #[test]
    fn an_unused_escape_is_withdrawn_after_drawing() {
        withdraw_after(false);
    }
}
