# ADR-0064: modifier-hint 표시 지연 타이머는 등록된 단축키가 실제로 소비되면 리셋한다

- **Status**: Accepted
- **Date**: 2026-08-08
- **Tags**: modifier-hint, overlay, reveal-delay, keybindings, discovery, user-agent-separation, adr-0035
- **Group**: keybindings

## Context

modifier(Ctrl/Alt/Option/Shift)를 홀드하면 500ms(Shift 단독은 1200ms) 뒤 단축키 도움말 오버레이(modifier-hint)가 뜬다. [ADR-0035](0035-modifier-hint-combo-narrowing-and-shift-delay.md) 결정 (A)는 이 지연 타이머(`hold_since`)를 "최초 press 에만 시작하고 조합이 바뀌어도 리셋하지 않는다"고 명시했다 — 그 결정의 맥락은 "Ctrl → Ctrl+Shift 로 조합을 좁혀 눌렀을 때 타이머가 되감기면 표시가 지연되는 문제"였다.

그런데 **단축키 실행 자체는 그 결정의 고려 대상이 아니었다**(ADR-0035 본문·대안 어디에도 언급 없음). 그 결과 홀드를 유지한 채 이미 등록된 단축키를 계속 실행 중인 사용자에게도 500ms 가 지나는 순간 도움말 패널이 불쑥 뜬다 — 대표 시나리오는 Ctrl 을 누른 채 `Ctrl+1` → `Ctrl+2` → `Ctrl+3` 으로 탭을 연속 전환하는 경우다. 오버레이의 목적은 단축키를 모르는 사용자에게 발견성(discovery)을 제공하는 것이므로, 등록된 단축키를 실제로 실행한 사용자에게 그 시점의 도움말은 불필요하고 방해가 된다.

ADR-0035 의 Reconsideration Trigger 는 이미 "게이트 정책(예: 타이핑 감지 연동) 재설계"를 재검토 조건으로 열거하고 있다 — 이번 변경은 그 계열(홀드 게이트를 사용자 활동 신호와 연동)에 해당하며, 조합 좁힘·Shift 단독 지연이라는 ADR-0035 의 기존 결정과 충돌하지 않고 **그 결정이 다루지 않은 축을 추가**한다.

불가침 원칙1(사용자↔에이전트 분리) 상 홀드 상태는 실제 사용자 키 입력(winit `ModifiersChanged`/`KeyboardInput`)만 반영해야 한다. `MainView::dispatch_action_by_id`(`src/adapters/ui/input/shortcuts/dispatch.rs`)는 Command Palette 경로와 윈도우 컨트롤 단축키가 함께 쓰는 공용 진입점이므로, 여기에 리셋을 걸면 키 홀드와 무관한 경로까지 사용자 홀드 상태를 건드리게 된다.

## Decision

**`ModifierHintRuntime::reset_reveal_timer_if_not_shown`을 신설하고, 키 입력 경로에서 등록된 단축키가 실제로 소비된 지점에서만 호출한다.** 판정은 순수하다 — 현재 `held` 조합의 `reveal_delay_ms` 를 기준으로 아직 지연 게이트를 통과하지 않았으면(=패널이 아직 안 뜬 상태) `hold_since` 를 지금 시각으로 다시 세팅하고, 이미 통과한(표시 중인) 홀드는 건드리지 않는다. 홀드 중이 아니면(`held`/`hold_since` 가 `None`) no-op — modifier 없이 실행된 단축키가 새 홀드를 만들지 않는다. `dismissed`(X dismiss 세션)와 `working`(드래그 중 rect)은 리셋 대상에서 제외한다.

호출 지점은 두 곳으로 한정한다 — 둘 다 **실제 키 입력 경로**이고 등록된 단축키가 소비를 확정하는 지점이다:

- `MainView::handle_shortcut`(`dispatch.rs`)이 `true` 를 반환하는 지점(`keyboard.rs` 의 `try_consume_shortcut_key` 안 단일 지점) — `handle_copy_shortcut`/`handle_explorer_shortcut`/`handle_window_control_shortcuts`/`handle_keybinding_shortcuts`/`try_dispatch_script_shortcut`/`handle_numeric_switch_shortcuts`/`handle_paste_shortcut`/`handle_zoom_shortcut` 8경로 전량을 이 한 지점이 커버한다.
- 더블탭 modifier 단축키(`shift+shift` 등, `try_consume_double_tap_key`)가 성공하는 지점.

`dispatch_action_by_id`(Command Palette 공용 진입점) 내부에는 이 호출을 넣지 않는다 — 원칙1 위반이 된다. PTY 로 포워딩되는 키, vi copy-mode 내부 키, Escape 닫기 경로도 "등록된 단축키 소비"가 아니므로 대상에서 제외한다.

## Consequences

- **얻은 것**: 홀드를 유지한 채 단축키를 계속 쓰는 동안에는 도움말 패널이 뜨지 않는다 — 이미 단축키를 아는 사용자를 방해하지 않는다. 판정 로직이 순수 함수 조합(`hold_since` 경과시간 + `reveal_delay_ms`)이라 단위 테스트로 완전히 고정 가능하고, `debug.modifier_hint.state` 로 실행 중 수치 검증도 가능하다.
- **잃은 것**: 없음 — 이미 표시된 패널은 건드리지 않고(숨겼다 다시 띄우지 않음), 조합 좁힘 시 타이머 유지(ADR-0035 A)도 그대로 보존된다.
- **운영 비용 / 유지 부담**: 새로운 "등록 단축키 소비 지점"이 앞으로 추가될 때(예: 새 입력 파이프라인 단계) 이 리셋 호출을 잊으면 그 경로만 조용히 예전 동작(리셋 없음)으로 남는다 — 회귀라기보다 발견성 개선 누락이라 사용자 관점에서 치명적이진 않지만, `handle_shortcut`/더블탭 두 지점에 집중돼 있어 새 단축키 경로가 생겨도 대부분 기존 8경로 안에 들어간다.

## Alternatives Considered

- **A — `hold_since` 를 매 키 입력마다 무조건 리셋**: 조합을 좁혀 누르는 것(Ctrl→Ctrl+Shift)도 키 입력이므로 ADR-0035 A(좁힘 시 타이머 유지)와 충돌한다. 등록된 단축키 "소비"와 "조합 변경"을 구분하지 못해 기각.
- **B — `dispatch_action_by_id`(Command Palette 공용 진입점)에 리셋을 건다**: 구현이 더 단순해 보이지만, Command Palette 로 실행된 액션은 실제 키 홀드와 무관한 마우스/커맨드 팔레트 경로다. 여기에 리셋을 걸면 홀드 상태가 실제 사용자 키 입력이 아닌 신호로도 바뀌게 되어 원칙1(사용자↔에이전트 분리, 및 입력 경로 순수성)을 어긴다. 기각.
- **C — 표시된 뒤에도(게이트 통과 후) 단축키 실행 시 패널을 다시 숨긴다**: 사용자 확정 범위 밖 — 이미 뜬 패널을 단축키 실행마다 숨겼다 다시 띄우면 오히려 깜빡임으로 방해가 된다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 리셋 대상 판정("등록된 단축키 소비")이 실제로는 더 넓거나 좁아야 한다는 사용자 피드백이 나올 때(예: PTY 로 포워딩된 키도 리셋 대상이어야 한다는 요구).
- macOS NSMenu key equivalent 로 소비되는 단축키(winit `KeyboardInput` 이 오지 않는 경로)에 대해서도 리셋이 필요해질 때 — 현재는 해당 액션이 창을 닫거나 앱을 떠나는 성격이라 홀드 세션이 이어지지 않는다는 전제로 범위 밖에 둔다.

## References

- [ADR-0035](0035-modifier-hint-combo-narrowing-and-shift-delay.md) — 조합 좁힘 + Shift 단독 1200ms 지연. 결정 (A)의 "타이머는 리셋하지 않는다"가 이번 변경으로 조합 변경에 한해서만 참이 됨(0035 본문은 Accepted 후 불변 정책에 따라 그대로 둔다). Reconsideration Trigger "게이트 정책 재설계"가 이 변경의 근거.
- [`docs/features/accessibility/index.md`](../features/accessibility/index.md) §Modifier key hints — 사용자 대상 동작 서술
- 코드: `src/adapters/ui/modifier_hint_overlay.rs`(`ModifierHintRuntime::reset_reveal_timer_if_not_shown`) · `src/view/main/keyboard.rs`(`reset_modifier_hint_reveal_timer` 호출 지점 2곳) · `src/adapters/ui/input/shortcuts/dispatch.rs`(`handle_shortcut`, 리셋을 걸지 않는 `dispatch_action_by_id`)
- 원칙1(사용자↔에이전트 분리): [`docs/identity.md`](../identity.md)
