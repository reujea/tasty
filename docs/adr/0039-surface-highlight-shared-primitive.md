# ADR-0039: Surface highlight 는 producer 중립 공유 primitive

- **Status**: Accepted
- **Date**: 2026-07-07
- **Tags**: surface-highlight, notification, ipc, cli, state, focus-independence
- **Group**: attention-notification

## Context

surface 의 "확인 대기(주의 환기)" 상태는 세 곳(테두리 강조 · 탭 제목 강조 · 워크스페이스
개수 배지)에 동시에 투영된다. 이 상태는 원래 `NotificationStore`(toast store) 안의
`highlighted_surfaces` 필드에 물리적으로 얹혀 있었고, **toast 알림의 `add()` 내부에서만**
발동됐다. 즉 highlight 가 toast producer 에 종속돼 있었다.

그러나 highlight 를 발동시킬 주체는 toast 하나가 아니다 — 이번에 추가하는 **completion**
(surface 가 작업을 완료했다는 IPC/CLI 신호)을 포함해, 후속으로 hook · 명령완료 자동감지 ·
plugin 등 여러 producer 가 같은 시각 효과를 재사용해야 한다. highlight 를 toast store 에
가둔 채로 두면 새 producer 마다 toast 를 우회해 세 소비처에 개별 배선하거나, highlight 를
producer 별로 분리해 소비처에서 N중으로 합성해야 한다.

## Decision

highlight 를 **producer 중립 공유 primitive** 로 둔다. 상태를 `NotificationStore` 에서
`CoreState.highlighted_surfaces`(기존 `busy_surfaces` 와 같은 위치·형태)로 옮기고, 공개
헬퍼 `raise_surface_highlight` / `clear_surface_highlight` / `is_surface_highlighted` /
`has_highlight` / `highlight_count` 를 노출한다. 어떤 producer(toast · completion · 후속)
든 `raise_surface_highlight` 로 발동하고, 세 소비처는 producer 를 구분하지 않고 이 단일
상태만 읽는다. 해제는 **실제 렌더 시점 포커스**(`gpu.rs`)로만 자동 수행한다.

completion 은 highlight 를 발동하는 **producer 중 하나**로 얹는다(`surface.completion`
IPC / `tasty surface completion`). completion ≠ highlight — 향후 completion 고유 효과가
필요하면 그 cascade 를 확장하되 highlight 발동은 공유 API 재사용을 유지한다.

## Consequences

- **얻은 것**: 새 producer 는 소비처를 건드리지 않고 `raise_surface_highlight` 한 줄로
  동일한 3채널 효과를 얻는다. highlight 의미가 toast 와 분리돼 "toast 없이 highlight" 가
  자연스럽다. 개수 배지(`highlight_count`)가 surface 단위로 정확히 세어진다.
- **잃은 것**: highlight 상태의 물리적 위치가 이동해 toast store 를 읽던 코드 경로가
  재배선됐다(2 producer 호출처 + focus 해제 + 3 소비처). 상태 이전 시 한 곳이라도 누락하면
  toast highlight 무음 회귀 위험이 있어 mechanical 하지만 범위가 넓었다.
- **운영 비용 / 유지 부담**: 낮음. `busy_surfaces` 와 동일 패턴(HashSet + 조회 헬퍼)이라
  유지 관례가 이미 있다. producer 추가는 순수 가산.

## Alternatives Considered

- **highlight 를 NotificationStore 에 그대로 두고 completion 이 toast 를 발행**: completion
  이 "알림 엔트리"를 만들지 않는데 toast 를 발행하면 알림 패널·unread 카운트가 오염된다.
  또한 highlight 가 계속 toast 종속이라 후속 producer 마다 같은 우회가 필요 — 기각.
- **highlight 를 producer 별 분리 축으로 유지**(toast-highlight, completion-highlight, …):
  소비처가 축을 N중으로 OR 합성해야 하고, 색/형태가 producer 별로 갈리면 시각 충돌이 난다.
  사용자 요구는 "producer 무관 단일 주의 상태" 라 기각.

## Reconsideration Triggers

- producer 별로 **다른** 시각 효과(색·아이콘·우선순위)가 요구되어 단일 상태로 표현이 불가능해질 때.
- highlight 에 카테고리/의미(예: "확인필요" vs "완료")가 추가돼 단순 bool 집합으로 부족할 때.
- 해제 규칙이 실 포커스 외(예: 타이머·명시적 clear IPC)로 확장되며 상태 모델 재설계가 필요할 때.
  - 2026-07-31: 읽음 처리로 해제 규칙이 확장됐으나 상태 모델 변경 없이 기존
    `clear_surface_highlight` 재사용으로 해결 — 이 트리거는 미발동으로 판단.

## References

- 기능 문서: [`features/surface-highlight`](../features/surface-highlight/index.md)
- 용어: [`concepts/ubiquitous-language.md`](../concepts/ubiquitous-language.md) (Highlight / Completion / Toast)
- 불가침 원칙 1(사용자 행동 ↔ 에이전트 행동 분리): [`identity.md`](../identity.md) · [`dev-guide/debug-ipc.md`](../dev-guide/debug-ipc.md)
- 미러 패턴: `src/core/state/busy.rs` (상태·헬퍼) · `SetTerminalMark`(intent/cascade/IPC/CLI)
- 후속: [ADR-0062](0062-attention-store-kind-aware-primitive.md) — 본 ADR 의 `HashSet<u32>`
  자료구조를 kind-aware `AttentionStore`(surface → `{ kind, raised_at }`)로 연장(대체 아님).
