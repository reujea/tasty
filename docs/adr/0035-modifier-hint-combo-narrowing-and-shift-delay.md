# ADR-0035: modifier-hint 오버레이 — 눌린 조합으로 섹션 좁힘 + Shift 단독 표시 지연 1.2초

- **Status**: Accepted
- **Date**: 2026-07-05
- **Tags**: modifier-hint, overlay, keybindings, combo, subset, reveal-delay, shift, design-token, accessibility, debug-ipc, adr-0510

## Context

modifier-hint 오버레이는 사용자가 modifier 를 홀드하면 "그 조합으로 가능한 단축키·역할"을 목록으로 보여준다. 초기 구현은 홀드 상태를 **단일 anchor**(`Option<HeldModifier>`)로 모델링했다. anchor 는 "먼저 눌린 축"으로 고정(sticky)되고, 그 anchor 를 **포함하는 모든 상위 조합**을 나열했다.

이 sticky anchor 모델은 두 문제를 낳았다.

1. **좁힘 부재(핵심 버그)**: Ctrl 을 홀드한 뒤 Shift 를 추가해도 anchor 는 Ctrl 로 남는다. `update_hold` 는 `prev != held`(둘 다 Ctrl)로 **dirty 를 반환하지 않아** 콘텐츠가 갱신되지 않고, Ctrl 단독의 넓은 목록이 그대로 유지됐다. 사용자가 조합을 좁혀 눌러도 화면은 좁혀지지 않는다.
2. **표시 지연의 획일성**: 모든 modifier 홀드가 동일한 500ms 지연 게이트를 통과하면 떴다. 그러나 Shift 는 대문자·기호 입력에 상시 눌리는 축이라, 타이핑 중 500ms 이상 Shift 를 누르는 일이 잦고 오버레이가 원치 않게 튀어 올랐다.

동시에 두 불가침 원칙이 걸려 있었다. **원칙1**(사용자↔에이전트 분리): 오버레이 홀드 상태는 실제 사용자 입력(winit `ModifiersChanged`)으로만 바뀌어야 하고 IPC 로 강제 표시할 수 없다 — 그래서 이 변경의 자동 검증 수단이 마땅치 않았다. **CLAUDE.md UI 규칙**: 시간 값을 코드에 하드코딩하지 않고 Theme 토큰으로 노출해야 한다.

## Decision

**(A) 홀드 모델을 단일 anchor 에서 4축 조합(`Combo`) 부분집합 기반으로 바꾼다.** `held: Option<Combo>` 가 현재 눌린 4축을 그대로 담고, `update_hold` 는 조합이 바뀌면 **항상 dirty** 를 반환해 즉시 목록을 좁힌다. 노출 대상은 `Combo::contains_all`(눌린 셋 ⊆ 조합) 로 필터한 `combos_containing_all(held)` — 눌린 축을 **모두** 포함하는 조합만 남는다. 타이머(`hold_since`)는 최초 press 에만 시작하고 조합이 바뀌어도 리셋하지 않는다. 스트립 헤더와 섹션 헤더는 `combo_keycaps`(전체 조합 키캡)를 공유한다. 역할 주입은 이미 좁혀진 섹션 위에서 판정하므로 좁힘과 자동 정합한다(추가 로직 없음).

**(B) Shift 단독 홀드에 한해 표시 지연을 1200ms 로 둔다.** `reveal_delay_ms(held, theme)` 헬퍼가 "shift 만 눌리고 ctrl/alt/option 모두 미눌림"이면 `motion_hold_reveal_shift()`(1200ms), 그 외에는 `modhint_hold_delay()`(500ms)를 돌려준다. (사실 정정: 이 절은 1200ms 를 `--tasty-duration-1200` primitive 라고 적었으나 **그 토큰은 존재한 적이 없다** — DTCG 원본의 duration primitive 는 0/90/120/150/200/500/900/1600 여덟 개뿐이고, 1200 은 코드에만 있다.) (최초 2000ms 로 도입했으나 "너무 길다"는 피드백에 따라 Reconsideration Trigger #1 대로 1200ms 로 조정 — 토큰이라 값만 바꿨다.) 매 프레임 현재 조합으로 재평가되므로 Shift 단독 대기 중 다른 modifier 를 추가하면 지연이 500ms 로 떨어지고 경과 시간이 이미 그를 넘었으면 즉시 표시된다. 지연 값은 하드코딩하지 않고 Theme 토큰·접근자로만 노출한다.

**(C) 검증은 debug 격리 IPC 로 한다.** 원칙1 을 지키면서 자동 단정을 가능케 하려고 `debug.modifier_hint.hold`(홀드 조합 force-state + 타이머 백데이트) / `debug.modifier_hint.state`(draw 경로와 동일 로직의 렌더 상태 덤프) 를 `#[cfg(all(debug_assertions, feature = "gui"))]` 로 신설한다. `host_popup.open` 과 동일하게 오버레이 내부 상태만 세팅하는 force-state 라 debug 격리로 충분하며 release 엔 노출되지 않는다.

## Consequences

- **얻은 것**:
  - 조합을 좁혀 누르면 목록이 즉시 좁혀진다(핵심 버그 해소). 다축 홀드 시 첫 섹션이 홀드 조합 자신이라 헤더와 일치해 자연스럽다.
  - 타이핑 중 Shift 스침으로 오버레이가 튀는 문제가 완화된다. 의도적 조합(Ctrl+Shift 등)은 기존 500ms 를 유지해 반응성 손실이 없다.
  - anchor sticky 로직·`pick_anchor`·`HeldModifier`·`combos_containing`·`held_label` 이 사라져 모델이 단순해진다(단일 진실: 눌린 4축).
  - 순수 함수(`contains_all`·`combos_containing_all`·`reveal_delay_ms`·`hold_reveal_alpha`)라 단위 테스트로 좁힘·지연 분기를 완전 고정. gui 실행 중 동작도 debug IPC 로 스크린샷 없이 자동 단정.
- **잃은 것**:
  - `--tasty-duration-1200` 은 기존 토큰 체인(500/200/1600)에 없던 신규 primitive라 design-token-mapping 에 행을 추가해야 했다.
  - Shift 지연은 UX 가치판단이 들어간 값이다(초기 2000ms → 현재 1200ms). 여전히 길다/짧다는 피드백 여지가 남는다(토큰이라 조정은 쉽다).
- **운영 비용 / 유지 부담**:
  - blast radius 는 두 파일(`modifier_hint.rs` 모델 + `modifier_hint_overlay.rs` 런타임)에 국한, 프로덕션 호출부는 `draw_modifier_hint` 1곳. 나머지는 테스트·debug IPC.
  - debug IPC 2개는 release 에 컴파일되지 않으므로 배포 표면 증가 없음.

## Alternatives Considered

- **(A-1) anchor 를 유지하되 조합 변경 시 dirty 만 반환**: 좁힘 없이 갱신만 되어 목록이 넓은 채로 남는다 — 근본 문제(어떤 조합을 보여줄지)를 안 푼다.
- **(A-2) "정확히 그 조합"만 노출(부분집합 아닌 완전일치)**: Ctrl 홀드 시 Ctrl+Shift·Ctrl+Alt 같은 "다음에 뭘 더 누르면 되는지"를 안내하지 못해, 오버레이의 발견성(discovery) 가치가 사라진다. 부분집합(상위 조합 노출)이 원래 설계 의도.
- **(B-1) 모든 홀드에 지연을 늘림**: Ctrl/Alt 조합 반응성까지 희생 — Shift 만 문제인데 전체를 느리게 함.
- **(B-2) Shift 단독을 아예 표시하지 않음**: Shift 단독에도 마우스 캡처 우회 등 유효 역할이 있어 정보 손실. 지연으로 "의도적 홀드"만 거르는 편이 정보를 지킨다.
- **(C-1) release IPC 로 오버레이 강제 표시**: 원칙1(사용자↔에이전트 분리) 위반. 에이전트가 사용자 홀드를 재현하는 것은 release 에 있어선 안 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- Shift 단독 1200ms 가 "여전히 방해된다" 또는 "너무 길어 못 쓴다"는 사용자 피드백이 반복될 때 — 토큰 값 조정 또는 게이트 정책(예: 타이핑 감지 연동) 재설계. (2000ms→1200ms 1차 조정 이력 있음.)
- 부분집합 노출이 조합 수가 많은 환경(대량 plugin 바인딩)에서 목록을 과도하게 키운다는 문제가 드러날 때 — 좁힘 기준(예: 실제 바인딩 있는 조합만) 재고.
- macOS Option 축까지 포함한 실기기 검증에서 조합 정렬·좁힘이 기대와 어긋날 때.

## References

- 구현 커밋: `feat(modifier-hint): 눌린 조합으로 섹션 좁힘` · `feat(modifier-hint): Shift 단독 표시 지연 2초` · `feat(debug-ipc): modifier-hint 홀드 주입/상태 덤프`
- [`docs/design/systems/design-token-mapping.md`](../design/systems/design-token-mapping.md) — `--tasty-motion-hold-reveal-shift` / `duration-1200` 행
- [`docs/dev-guide/debug-ipc.md`](../dev-guide/debug-ipc.md) — `debug.modifier_hint.hold` / `.state`
- 코드: `src/adapters/ui/input/shortcuts/modifier_hint.rs`(모델) · `src/adapters/ui/modifier_hint_overlay.rs`(런타임/draw) · `crates/tasty-type-appearance/src/theme.rs`(토큰)
- 원칙1(사용자↔에이전트 분리): [`docs/identity.md`](../identity.md) · [`docs/dev-guide/debug-ipc.md`](../dev-guide/debug-ipc.md)
