//! 팝업 본문의 키 입력 문. 키(Escape 포함)·글자·붙여넣기·복사·잘라내기·IME 입력은 이번 프레임 키 주인 본문만
//! 읽는다. 본문이 egui 입력을 직접 읽든(`key_pressed`·`consume_key`·`events`) egui 위젯이 키 포커스로 읽든, 키
//! 주인이 아닌 본문을 그리는 동안에는 그 사건이 입력에 없다. 본문을 그린 뒤 원래 자리에 돌려놓아 뒤에 그리는
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

/// `takes_keys`가 거짓이면 키 입력 사건과 눌린 키 상태(`keys_down`, `key_down` 이 읽는다)를 뺀 채 `body`를 부르고,
/// 끝나면 뺀 사건을 원래 순서 자리에, 눌린 키 상태를 그대로 돌려놓는다.
pub(crate) fn with_body_keys<R>(
    ctx: &egui::Context,
    takes_keys: bool,
    body: impl FnOnce() -> R,
) -> R {
    if takes_keys {
        return body();
    }
    let (hidden, keys_down) = ctx.input_mut(|i| {
        let mut hidden: Vec<(usize, egui::Event)> = Vec::new();
        let mut kept = Vec::with_capacity(i.events.len());
        for (at, event) in std::mem::take(&mut i.events).into_iter().enumerate() {
            if is_key_input(&event) {
                hidden.push((at, event));
            } else {
                kept.push(event);
            }
        }
        i.events = kept;
        (hidden, std::mem::take(&mut i.keys_down))
    });
    let out = body();
    ctx.input_mut(|i| {
        for (at, event) in hidden {
            let at = at.min(i.events.len());
            i.events.insert(at, event);
        }
        i.keys_down = keys_down;
    });
    out
}

/// 창이 처리해 egui 입력에서 뺀 원래 Esc 사건(`handed`, 수식키 포함)을 키 주인을 그리는 동안만 넣는다. host 팝업
/// 층에서는 묶인 키 주인 본문만 읽는다(다른 본문은 문이 가린다). plugin 렌더는 키 주인 팝업에만 키를 보낸다. 팝업 층
/// 보다 먼저 그린 화면과 egui 의 패스 시작 처리(키 포커스 해제)는 이 Esc 를 보지 않는다. 키 주인이 읽지 않고 남긴
/// 사건은 끝나면 뺀다. 뒤에 그리는 것은 보지 않는다.
pub(crate) fn with_handed_escape<R>(
    ctx: &egui::Context,
    handed: &[egui::Event],
    draw: impl FnOnce() -> R,
) -> R {
    hand_escape(ctx, handed);
    let out = draw();
    withdraw_escape(ctx, handed);
    out
}

/// [`with_handed_escape`]의 앞쪽. 그리기가 한 함수 호출이 아닌 host 팝업 층이 쓴다.
pub(crate) fn hand_escape(ctx: &egui::Context, handed: &[egui::Event]) {
    if !handed.is_empty() {
        ctx.input_mut(|i| i.events.extend(handed.iter().cloned()));
    }
}

/// [`with_handed_escape`]의 뒤쪽. 키 주인이 읽지 않고 남긴 사건을 뺀다.
pub(crate) fn withdraw_escape(ctx: &egui::Context, handed: &[egui::Event]) {
    if handed.is_empty() {
        return;
    }
    ctx.input_mut(|i| {
        for event in handed {
            if let Some(at) = i.events.iter().position(|e| e == event) {
                i.events.remove(at);
            }
        }
    });
}

/// 창이 egui 입력에서 빼는 Esc 사건인지.
pub(crate) fn is_escape(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: egui::Key::Escape,
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use super::with_body_keys;

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
                        )
                    })
                });
                assert_eq!(seen, (false, false, false, vec![moved.clone()]));
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
}
