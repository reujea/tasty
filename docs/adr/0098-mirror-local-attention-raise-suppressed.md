# ADR-0098: mirror surface 의 attention 은 서버 push 만을 소스로 갖는다 — 로컬 발동은 억제하고 forward 하지 않는다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: attention, surface-highlight, remote-attach, mirror, source-of-truth, osc133, notification, ipc
- **Group**: attention-notification

## Context

`AttentionStore` 는 인스턴스 로컬 상태이고, 원격 attach 로 워크스페이스를 mirror 하는 쪽은
서버(surface 를 소유한 인스턴스)가 push 한 값을 자기 store 에 반영한다
([features/surface-highlight](../features/surface-highlight/index.md) "원격 attach mirror 로의 전파").
진실 원천은 surface 를 소유한 인스턴스다 — producer 4 종(완료 IPC/CLI, Claude 플러그인 훅,
OSC 133 명령 완료, toast)이 전부 PTY 가 있는 쪽에서 돌기 때문이다.

문제는 **미러 쪽에서도 같은 producer 들이 실제로 발화한다**는 점이다. 미러 터미널은 로컬 PTY 가
없지만 서버가 흘려준 바이트를 그대로 파싱하고, 그 파서 이벤트는 `CoreState::collect_events` 가
mirror 를 포함한 모든 terminal 을 순회해 수집한다. 그래서 OSC 133 D(명령 완료)·Bell·
OSC 9/777 알림이 미러에서도 나온다. 여기에 더해 `surface.completion` IPC/CLI 는 미러 인스턴스
에서 도는 에이전트·플러그인이 **로컬 mirror surface id** 를 대상으로 직접 호출할 수 있다
(핸들러는 surface_id 만 검증하고 mirror 여부를 보지 않는다).

그대로 두면 같은 사건 하나에 서버와 미러가 **각각 별개의 레코드**를 갖는 이중 상태가 된다.
미러 로컬 raise 는 서버가 모르므로 미러에서 지워도 서버엔 영향이 없고, 이는 "미러가 지운 값이
서버에 닿지 않는다" 는 원래 결함의 축소판이 그대로 남는 것이다.

같은 판단이 busy 축에는 이미 서 있다 — `busy_activity_forwards`(`src/core/state/busy.rs`)는
"attach lock 은 이 인스턴스가 호스팅하는 surface 에만 걸리므로 `mirror_busy_surfaces` 는 여기서
구조적으로 무관" 이라고 못 박고, 미러의 busy 는 push 로만 들어온다. attention 에는 그 원칙이
없었다.

## Decision

**mirror surface 에 대한 로컬 attention 발동을 억제한다.** 게이트는 producer cascade 마다가
아니라 로컬 producer 축의 **단일 진입점** `CoreState::raise_attention`
(`src/core/state/attention.rs`)에 둔다 — `is_mirror_surface` 가 참이면 no-op 으로 끝낸다.
그래서 OSC 133 자동 경로·알림 생성 cascade·`surface.completion` IPC/CLI·Windows resume 헬스
패스와 앞으로 추가될 producer 까지 한 번에 덮인다. 서버 push 적용은 이 함수를 타지 않는 원격
전용 진입점(`set_mirror_surface_attention`)이라 게이트에 막히지 않는다.

**미러를 대상으로 한 `surface.completion` 도 억제한다 — 서버로 forward 하지 않는다.**

억제 대상은 **attention 레코드 한 줄뿐**이다. 같은 cascade 가 함께 만드는 알림 패널 아이템·
토스트·`HookEvent` 발화는 게이트 밖이므로 미러에서도 그대로 동작한다.

## Consequences

- **얻은 것**: 미러의 attention 은 서버 push 를 유일한 소스로 갖는다 — 이중 레코드가 사라지고,
  미러 배지 상태가 서버 값과 정의상 일치한다. 게이트가 한 곳이라 새 producer 가 추가돼도
  자동으로 적용된다. 이 결정이 닫는 것은 **발동 축**이고, 반대 방향인 해제 축은
  [ADR-0104](0104-mirror-attention-clear-forwarded-to-owner.md)가 미러의 해제 edge 를 소유
  인스턴스로 전달해 닫는다 — 두 결정이 함께 미러 store 를 바꾸는 로컬 경로를 없앤다.
- **잃은 것**: 미러 인스턴스에서 도는 에이전트가 자기 화면의 mirror surface 를 스스로 표시할
  방법이 없다. 그 에이전트는 원격 surface 를 소유하지 않으므로 표시 권한도 없다고 본다 —
  필요하면 서버 인스턴스의 IPC 로 원격 surface id 를 대상으로 호출하면 된다.
- **검증 방식**: 미러 측 억제는 `AppState`/`CoreState` 단위 테스트로 고정한다(`local_attention_raise_is_suppressed_on_mirror_surface` · `mirror_attention_lands_in_the_same_store_consumers_read` · `mirror_apply_does_not_touch_forward_cache`). loopback e2e
  로 대체하지 않은 이유는 두 가지다 — 헤드리스 환경에서 GUI 두 인스턴스를 실제로 attach 할
  수 없고(기존 loopback client 는 raw `TcpStream` 이라 미러 store 자체가 없다), "미러 포커스
  → 양쪽 해제" 는 해제 forward(mirror→server)가 들어오기 전에는 정의상 성립하지 않는 후속
  범위다.
- **운영 비용 / 유지 부담**: `raise_attention` 호출마다 `is_mirror_surface`(워크스페이스 순회)
  가 한 번 더 돈다. attention raise 는 사람이 인지하는 사건 단위라 빈도가 낮아 무시 가능하다.
  주의점은 하나 — **새 원격 적용 경로를 만들 때 `raise_attention` 을 재사용하면 안 된다**
  (게이트에 자기 push 가 막힌다). 원격 적용은 항상 mirror 전용 진입점을 쓴다.

## Alternatives Considered

- **A: producer cascade 마다 게이트를 넣는다** — 처음 지목된 3 개 호출부에 각각
  `is_mirror_surface` 분기를 둔다. 실제로는 `cascade_surface_completion` 까지 4 개 호출부이고,
  새 producer 가 추가될 때마다 누락 위험이 생긴다. 억제 이유가 producer 별 사정이 아니라
  "미러는 소유자가 아니다" 라는 단일 사실이므로 단일 진입점이 맞다.
- **B: 미러의 로컬 발동을 서버로 forward 한다**(`surface.completion` 을 client→server 로 보냄) —
  진실 원천 단일화라는 목적 자체는 만족하지만, ① 새 client→server Control 변형과 서버측 권한
  검증(홀더만 남의 surface 에 attention 을 걸 수 있는가)이 필요해 범위가 크고, ② OSC 133·Bell
  경로는 forward 하면 **서버가 이미 같은 사건으로 발동한 것과 중복**된다(서버도 같은 바이트를
  보고 있다). forward 로 이득이 있는 것은 미러 인스턴스 고유의 신호뿐인데, 그런 신호를 내는
  producer 는 현재 없다.
- **C: 미러에서 발동은 두되 소비처에서 가린다** — 표시만 숨기고 store 에는 남긴다. 이중 상태를
  그대로 두는 것이라 `attention_count_of_kind` 등 다른 소비처·IPC 노출에서 다시 새어 나온다.
- **D: IPC 경계에서 명시 거부** — store 게이트는 그대로 두고 `surface.completion` 핸들러가
  mirror 대상 호출을 사유와 함께 `invalid_params` 로 거부한다(ADR-0086 이 mirror 워크스페이스
  `terminal.spawn` 을 거부한 선례와 정합). 지금 채택하지 않은 이유는 **결정 자체가 아니라 그
  결정의 IPC 경계 노출 방식**이라, 억제 정책과 분리해 attention 소유권 모델을 확정하는 후속
  ADR 에서 함께 정하는 편이 낫기 때문이다.
- **E: 응답에 `suppressed: true` 필드를 실어 알린다** — 하위 호환을 지키면서 호출자가 억제를
  판별할 수 있다. D 와 같은 이유로 함께 후속 결정으로 넘긴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 미러 인스턴스에서만 관측 가능한 attention 신호를 내는 producer 가 생긴다(원격에는 정보가
  없어 서버가 같은 사건을 발동할 수 없는 경우) — 그때는 B(forward)가 필요해진다.
- 미러 사용자가 자기 화면에서만 유효한 표시를 남길 수 있어야 한다는 요구가 생긴다 — 서버와
  분리된 mirror-local 표시 축(별도 store)이 필요해진다.
- attention 이 surface 소유와 무관한 축(예: 워크스페이스 단위 신호)으로 확장된다.
- **IPC 경계의 관측 가능성 결정이 내려진다** — 현재 mirror 대상 `surface.completion` 은
  `ok: true` 를 돌려주고 조용히 no-op 한다(억제 사실은 `tracing::trace!` 로만 남는다).
  위 대안 D(명시 거부) 또는 E(`suppressed` 필드) 중 하나가 채택되면 본 ADR 의 Decision 에
  경계 동작을 보강한다. 이 판단은 attention 소유권 모델을 확정하는 후속 ADR 이 다룬다.

## References

- [features/surface-highlight](../features/surface-highlight/index.md) — Producer 목록과 mirror 항목
- [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) — "주의 환기(attention) 전파"
- [ADR-0086](0086-reject-terminal-spawn-into-mirror-workspace.md) — mirror 워크스페이스에 로컬
  부수효과를 만들지 않는다는 선례
- [ADR-0062](0062-attention-store-kind-aware-primitive.md) — `AttentionStore` 가 producer 중립
  공유 primitive 라는 결정
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — hard/soft 점유와 홀더 권한
