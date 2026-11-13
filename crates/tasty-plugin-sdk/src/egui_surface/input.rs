//! 호스트 입력을 egui 입력과 휠 이벤트로 변환한다.

use super::*;

/// [`RawInputWire`] + 크기/ppp 를 egui [`RawInput`] 으로 매핑한다. 좌표는 surface-local
/// 논리 포인트(좌상단 0,0)로 들어오므로 그대로 쓰고, screen_rect 는 물리 px / ppp 로 계산한다.
/// surface 와 popup 이 공유한다(키잉 id 만 다르고 렌더 컨텍스트 구조는 동일).
pub(super) fn build_raw_input(
    width_px: u32,
    height_px: u32,
    ppp: f32,
    input: &RawInputWire,
) -> RawInput {
    let mut raw = RawInput::default();

    let ppp = if ppp > 0.0 { ppp } else { 1.0 };
    // 물리 픽셀 → 논리 포인트. egui 레이아웃은 포인트 단위다.
    let width_pt = width_px as f32 / ppp;
    let height_pt = height_px as f32 / ppp;
    raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(width_pt, height_pt)));

    // ppp 는 viewport 의 native_pixels_per_point 로 전달 → ctx.pixels_per_point 가 이 값을
    // 따르고, full_output.pixels_per_point 도 동일해져 tessellate/encode 와 정합한다.
    let viewport_id = raw.viewport_id;
    raw.viewports
        .entry(viewport_id)
        .or_default()
        .native_pixels_per_point = Some(ppp);

    raw.time = input.time;
    raw.focused = input.focused;
    raw.modifiers = map_modifiers(&input.modifiers);
    raw.events = expand_events(&input.events);
    raw
}

/// egui 0.31 이 휠 델타를 "이미 부드럽다" 고 판정하는 상한(포인트). `Point` 단위
/// `MouseWheel` 의 델타 길이가 이 값 **미만**이면 egui 는 그 프레임에서 델타를 전부
/// `smooth_scroll_delta` 에 반영하고, 이상이면 `unprocessed_scroll_delta` 에 적립해
/// 여러 프레임에 걸쳐 지수완화로 소진한다(`egui-0.31.1/src/input_state/mod.rs` 의
/// `is_smooth` 판정과 그 아래 drain 루프). 소진이 끝날 때까지 egui 는 매 pass
/// `wants_repaint_after() == ZERO` 를 돌려준다.
pub(super) const EGUI_SMOOTH_WHEEL_LIMIT: f32 = 8.0;

/// 쪼갠 조각 하나의 목표 길이. 판정선 바로 아래가 아니라 여유를 둬서, 나눗셈의
/// 부동소수 오차로 한 조각이 판정선에 걸리는 일이 없게 한다.
pub(super) const SCROLL_SPLIT_STEP: f32 = EGUI_SMOOTH_WHEEL_LIMIT * 0.9;

/// 한 스크롤 이벤트를 쪼갤 조각 수 상한. 이 이상이 필요한 극단적 델타
/// (> 460pt, 한 프레임에 몰린 플링)은 쪼개지 않고 그대로 넘긴다 — 이벤트 폭증을
/// 막기 위해서이며, 그 경우에만 egui 기본 스무딩으로 되돌아간다(변경 전과 동일 동작).
pub(super) const SCROLL_SPLIT_MAX_PARTS: usize = 64;

/// 와이어 이벤트 목록을 egui 이벤트 목록으로 펼친다. 스크롤만 1:N 이고
/// ([`push_scroll_events`]) 나머지는 [`map_event`] 의 1:1 매핑이다.
pub(super) fn expand_events(events: &[RawInputEventWire]) -> Vec<Event> {
    let mut out = Vec::with_capacity(events.len());
    for e in events {
        match e {
            RawInputEventWire::Scroll { x, y } => push_scroll_events(&mut out, vec2(*x, *y)),
            other => out.extend(map_event(other)),
        }
    }
    out
}

/// 휠 델타를 egui의 smooth 판정 기준보다 작은 조각으로 나눠 같은 프레임에 넣는다.
/// 큰 델타의 다중 프레임 스무딩에 필요한 호스트 왕복을 줄이기 위한 처리다.
pub(super) fn push_scroll_events(out: &mut Vec<Event>, delta: Vec2) {
    let len = delta.length();
    let parts = if len.is_finite() && len > 0.0 {
        (len / SCROLL_SPLIT_STEP).ceil() as usize
    } else {
        1
    };
    if parts <= 1 || parts > SCROLL_SPLIT_MAX_PARTS {
        out.push(wheel_event(delta));
        return;
    }
    let step = delta / parts as f32;
    for _ in 0..parts - 1 {
        out.push(wheel_event(step));
    }
    // 마지막 조각은 뺄셈으로 만든다 — 조각 합이 원본 델타에서 멀어지지 않게 한다.
    out.push(wheel_event(delta - step * (parts - 1) as f32));
}

pub(super) fn wheel_event(delta: Vec2) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        modifiers: Modifiers::default(),
    }
}

pub(super) fn map_modifiers(m: &ModifiersWire) -> Modifiers {
    Modifiers {
        alt: m.alt,
        ctrl: m.ctrl,
        shift: m.shift,
        mac_cmd: m.mac_cmd,
        command: m.command,
    }
}

pub(super) fn map_button(b: PointerButtonWire) -> PointerButton {
    match b {
        PointerButtonWire::Primary => PointerButton::Primary,
        PointerButtonWire::Secondary => PointerButton::Secondary,
        PointerButtonWire::Middle => PointerButton::Middle,
    }
}

/// 한 와이어 이벤트를 egui [`Event`] 로 매핑한다. 매핑 불가한 키 이름은 `None`(드롭) —
/// 프로토콜 계약([`RawInputEventWire::Key`])대로 plugin 이 무시한다.
pub(super) fn map_event(e: &RawInputEventWire) -> Option<Event> {
    Some(match e {
        RawInputEventWire::PointerMoved { x, y } => Event::PointerMoved(Pos2::new(*x, *y)),
        RawInputEventWire::PointerButton {
            x,
            y,
            button,
            pressed,
            modifiers,
        } => Event::PointerButton {
            pos: Pos2::new(*x, *y),
            button: map_button(*button),
            pressed: *pressed,
            modifiers: map_modifiers(modifiers),
        },
        RawInputEventWire::PointerGone => Event::PointerGone,
        // 단독 호출 시의 1:1 매핑. 실제 입력 경로는 [`expand_events`] 가 가로채
        // [`push_scroll_events`] 로 쪼갠다.
        RawInputEventWire::Scroll { x, y } => wheel_event(vec2(*x, *y)),
        RawInputEventWire::Key {
            key,
            pressed,
            repeat,
            modifiers,
        } => Event::Key {
            key: Key::from_name(key)?,
            physical_key: None,
            pressed: *pressed,
            repeat: *repeat,
            modifiers: map_modifiers(modifiers),
        },
        RawInputEventWire::Text { text } => Event::Text(text.clone()),
        RawInputEventWire::Ime { event } => Event::Ime(match event {
            ImeWire::Enabled => ImeEvent::Enabled,
            ImeWire::Preedit { text } => ImeEvent::Preedit(text.clone()),
            ImeWire::Commit { text } => ImeEvent::Commit(text.clone()),
            ImeWire::Disabled => ImeEvent::Disabled,
        }),
        RawInputEventWire::Copy => Event::Copy,
        RawInputEventWire::Paste => Event::Paste(String::new()),
        RawInputEventWire::PasteText { text } => Event::Paste(text.clone()),
        RawInputEventWire::Cut => Event::Cut,
        RawInputEventWire::Unknown => return None,
    })
}
