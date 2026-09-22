# ADR-0062: Surface attention 상태를 kind-aware `AttentionStore` 로 확장한다

- **Status**: Accepted
- **Date**: 2026-08-08
- **Tags**: surface-highlight, attention, notification, state, adr-0039
- **Group**: attention-notification

## Context

ADR-0039 는 surface 의 "확인 대기(주의 환기)" 상태를 `CoreState.highlighted_surfaces:
HashSet<u32>` 로 두는 producer 중립 공유 primitive 를 확립했다. 이 자료구조는 surface
가 highlight 됐는지 **여부**만 표현할 뿐 **왜** 됐는지는 표현하지 못한다.

이 한계가 실제 부채로 드러난 지점이 두 곳이다.

1. 향후 "확인이 필요함"(승인 대기 등, `NeedsInput`)을 완료(`Completion`)와 **다른 색/우선순위**로
   구분해 보여줘야 하는 요구가 예정돼 있다. bool 집합 위에서는 이 분기를 표현할 수 없어,
   세 번째 저장소를 새로 얹거나 소비처마다 별도 registry 를 조회해 조합해야 한다 —
   동기화 조합이 순식간에 N 개로 늘어난다.
2. `highlighted_surfaces` 와 `NotificationStore` 가 사실상 같은 사실("이 surface 에 볼
   것이 있다")을 두 곳에 복제하고, `mark_notification_read`/`mark_all_notifications_read`
   가 그 복제를 손으로 동기화하는 코드( `has_unread_for_surface` 확인 후
   `clear_surface_highlight` 호출)를 담고 있었다. `NotificationStore` 없이도 attention 을
   발동하는 producer(OSC 133 명령 완료 등)가 이미 존재해 "둘은 같은 사실"이라는 전제 자체가
   깨져 있었다 — 미러링을 정리하지 않은 채 kind 를 얹으면 부채가 배가된다.

## Decision

`highlighted_surfaces: HashSet<u32>` 를 제거하고 `AttentionStore`(`CoreState.attention`)
로 대체한다. 레코드는 `surface_id → { kind: AttentionKind, raised_at: Instant }` —
이 시점 `AttentionKind` 는 `Completion` 1종이며, `NeedsInput` 은 후속 작업이 추가한다.

kind → 효과는 host/cascade 에 의존하지 않는 순수 함수 `effects_of(kind) -> AttentionEffects
{ level, panel_item, os_notify, sound }` 로 분리한다(`crates/tasty-plugin-claude/src/hook.rs`
의 `apply_hook` 과 동형 패턴). `panel_item` 이 "이 kind 의 attention 레코드가 알림 패널
아이템을 함의하는가"를 표현해 `AttentionStore` 와 `NotificationStore` 를 명시적으로 별개
저장소로 유지한다 — attention 레코드가 곧 패널 아이템은 아니다. 패널 노출이 실제로 필요한
producer(toast, windows resume)는 지금처럼 `NotificationStore::add()` 를 직접 호출한다.

Core 상태·API 이름은 `Attention`(`raise_attention`/`clear_attention`/`has_attention`/
`any_attention`/`attention_count`/`attention_kind`/`attention_count_of_kind`)으로 통일한다.
View 계층(테두리·탭 제목·워크스페이스 배지 렌더 함수·타입명)은 `Highlight` 이름을 그대로
유지한다 — Attention 이 View 에 투영되는 결과물의 이름이 Highlight 라는 관계이지, 동의어
치환이 아니다(`docs/concepts/ubiquitous-language.md` 참고).

이 결정은 ADR-0039 의 "producer 중립 공유 primitive" 정의를 뒤집지 않는다 — 자료구조를
`HashSet<u32>` 에서 `surface → kind` 로 넓히는 **연장**이다.

## Consequences

- **얻은 것**: kind 조회(`attention_kind`)와 kind 별 카운트(`attention_count_of_kind`)
  API 가 이미 존재해, 후속으로 `NeedsInput` kind 를 추가할 때 상태 모델을 다시 설계할
  필요 없이 `effects_of` 의 match 분기와 소비처 색 로직만 확장하면 된다. `mark_notification_read`
  /`mark_all_notifications_read` 의 수동 동기화가 `AttentionStore`/`NotificationStore` 가
  독립 저장소라는 사실을 전제로 재작성돼, "같은 사실의 복제"라는 잘못된 전제가 코드에서
  사라졌다.
- **잃은 것**: producer 4곳 + consumer 3곳 + focus 해제 1곳의 호출부가 전부 재배선됐다
  (ADR-0039 때와 동일하게 mechanical 하지만 범위가 넓다). Core 이름(Attention)과 View
  이름(Highlight)이 갈라져, 이 관계를 모르는 새 기여자는 두 이름이 왜 다른지 문서
  (`ubiquitous-language.md`)를 먼저 봐야 한다.
- **운영 비용 / 유지 부담**: 낮음. `NotificationStore` 와 구조적으로 대응하는 독립 struct
  라 유지 관례가 이미 있다(같은 `CoreState` 가 두 저장소를 나란히 보유). kind 추가는
  `effects_of` 의 match arm 과 소비처 색 분기 추가로 국한된다.

## Alternatives Considered

- **`needs_input_surfaces: HashSet<u32>` 를 별도로 병렬 추가**: bool 집합을 kind 수만큼
  늘리는 방식. 소비처가 N 개 집합을 순회해 우선순위를 직접 조합해야 하고, `NeedsInput` 이후
  세 번째 kind 가 생기면 조합이 다시 배가된다 — kind 가 하나뿐인 지금 미리 정리하는 이번
  기회를 놓치면 부채가 누적되므로 기각.
- **kind 를 `NotificationStore` 의 `Notification` 레코드에 필드로 얹는다**: `Notification`
  이 없는 producer(OSC 133 명령 완료, 승인 없이 끝난 completion IPC 등)가 이미 존재해
  전제가 성립하지 않는다 — attention 은 알림이 아닌데도 발동해야 하므로 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- kind 가 3종 이상으로 늘어나 `effects_of` 의 단순 match 로 우선순위/조합을 표현하기
  어려워질 때(정책 테이블·우선순위 그래프 등 별도 구조가 필요해질 때).
- View 계층의 `Highlight` 이름과 Core 의 `Attention` 이름이 분리돼 있는 데서 오는 혼동
  비용이 실제로 반복 보고될 때 — 그 경우 View 계층도 `Attention` 으로 개명하는 별도
  ADR 을 고려한다.
- `AttentionStore` 와 `NotificationStore` 를 다시 합쳐야 할 요구(예: 패널 아이템 없는
  attention 이 더 이상 필요 없어질 때)가 생길 때.

## References

- ADR-0039 [Surface highlight 는 producer 중립 공유 primitive](0039-surface-highlight-shared-primitive.md) — 본 ADR 이 연장하는 원 결정.
- 기능 문서: [`features/surface-highlight`](../features/surface-highlight/index.md)
- 용어: [`concepts/ubiquitous-language.md`](../concepts/ubiquitous-language.md) (Attention / Highlight / Completion)
- 순수 정책 함수 선례: `crates/tasty-plugin-claude/src/hook.rs` (`apply_hook`)
