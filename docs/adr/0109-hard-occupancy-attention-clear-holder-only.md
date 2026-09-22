# ADR-0109: 하드 점유 중인 surface 의 attention 은 홀더만 해제한다 — 서버 로컬 포커스·알림 읽음은 게이트된다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: attention, surface-highlight, occupancy, hard-occupy, remote-attach, readonly, adr-0040, adr-0049
- **Group**: attention-notification

## Context

[ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) 은 하드 점유된 surface 에 대해
로컬 사용자·AI 에이전트를 **readonly** 로 정의했다. 주체는 홀더다.
[ADR-0104](0104-mirror-attention-clear-forwarded-to-owner.md) 는 그 원칙을 attention 해제 축에
적용해, 미러(홀더) 쪽에서 발생한 해제 edge 를 소유 인스턴스로 전달했다.

반대 방향이 열려 있었다. 해제 producer 는 둘이고 **둘 다 그 인스턴스의 로컬 GUI 사건**이다 —
매 렌더 프레임의 실-포커스(`src/gfx/gpu.rs`)와 알림 읽음 처리(`mark_notification_read` /
`mark_all_notifications_read`). 이 둘이 점유 중에도 그대로 돌아, 서버 사용자가 그 surface 를
포커스하거나 알림 패널에서 "모두 읽음" 을 누르면 홀더에게 보내려던 신호가 사라졌다.
서버→미러 push 가 생긴 뒤에는 그 해제가 **미러까지 전파**되어, 원격 사용자가 배지를 보기 전에
없어질 수 있다. "모두 읽음" 은 안읽음이었던 모든 surface 를 한 번에 지우므로 점유 중 배지
전부를 날리는 단일 클릭 구멍이었다.

`OccupancyRegistry::is_hard_occupied(surface_id)` 가 정확히 필요한 판정이다 — 워크스페이스
단위 attach 도 멤버 surface 마다 surface lock 을 걸므로 같은 술어로 덮인다.

## Decision

**하드 점유 중인 surface 의 attention 은 홀더만 해제한다.** 그 인스턴스의 로컬 사용자 사건
(실 렌더 포커스 · 알림 읽음)은 점유 중 attention 을 지우지 못한다.

세부 결정 넷:

1. **게이트는 로컬 축 진입점(`CoreState::clear_attention_local`)에 둔다.** 해제 producer 는
   둘(실-포커스 · 알림 읽음)이고 그 **호출부는 셋**이다(`gpu.rs` · `mark_notification_read` ·
   `mark_all_notifications_read`) — 셋 전부가 이 래퍼를 지나고 술어
   (`local_attention_clear_allowed`)는 한 곳에서 평가된다.
   게이트를 primitive 인 `clear_attention` 안에 두면 홀더의 해제를 적용하는 서버측 경로
   (`apply_attached_attention_clear` → `clear_attention`)까지 막혀 **해제 주체가 0 이 된다** —
   ADR-0104 가 이 작업에 걸어둔 제약이고, 회귀 테스트로 고정했다.
2. **알림의 읽음 플래그와 attention 해제를 분리한다.** 점유 중이어도 알림은 그대로 읽음
   처리된다. 읽음은 이 인스턴스 사용자의 알림 패널 상태이고, attention 은 홀더와 **공유하는**
   상태다. `mark_notification_read` 가 이미 `mark_read` 와 조건부 clear 로 분리된 구조라 clear
   쪽만 게이트한다.
3. **soft 점유는 대상이 아니다.** ADR-0040 의 약한 점유는 로컬 사용자를 배제하지 않는다
   (write 제한 없음). 술어가 `is_hard_occupied` 하나뿐인 것이 그 집행이다. 같은 실-포커스
   블록에 있는 soft 점유 지연 청소(`reconcile_soft_occupancy_on_focus`)는 attention 해제 권한과
   무관한 동작이라 게이트 밖에 둔다.
4. **미러 인스턴스에서는 이 게이트가 걸리지 않는다.** 점유는 surface 를 **소유한** 인스턴스가
   기록하므로 미러의 `OccupancyRegistry` 에는 그 lock 이 없다. 미러 사용자의 확인은 그대로
   제거 edge 를 만들어 서버로 forward 된다(ADR-0104) — 두 ADR 이 합쳐 "해제 주체는 정확히
   홀더 하나" 를 만든다.

**데드락이 없다.** 점유가 풀리면(정상 detach / force-detach / 연결 끊김) 서버 로컬 포커스가
**자동으로** 해제 주체로 복귀한다. 홀더가 배지를 남긴 채 나가도 서버 사용자가 그 surface 를
포커스하는 순간 정리된다 — 게이트는 상태를 저장하지 않고 매 호출 `is_hard_occupied` 를 다시
묻기 때문이다. `OccupancyRegistry` 의 lock 해제 경로 넷 — holder 본인 해제 `release`,
연결 종료(EOF) 시 그 client 의 lock 일괄 해제 `release_all_for_client`, 서버 권한 강제 해제
`force_detach` / `force_detach_workspace` — 이 이미 그 전이를 만든다.

### ADR-0049 와 다른 축이다

[ADR-0049](0049-hard-occupancy-selection-exception.md) 는 하드 점유의 readonly 를
**"PTY/TUI 상호작용인가, 순수 로컬 동작인가"** 축으로 갈랐다 — 드래그 선택·복사는 PTY 에
아무것도 보내지 않으므로 예외로 허용했다. 그 축만 적용하면 attention 해제도 PTY 를 건드리지
않으므로 허용 쪽으로 떨어진다.

이 결정이 쓰는 축은 다르다: **그 상태가 관측자 로컬인가, 홀더와 공유되는가.** selection 은
로컬 사용자 자기 화면·클립보드에만 존재해 홀더가 보는 것을 바꾸지 않는다. attention 레코드는
그 반대로, 서버→미러 push 채널이 **같은 레코드**를 홀더에게 실어 보낸다 — 로컬에서 지우면
홀더의 신호가 사라진다. 그래서 ADR-0049 의 "순수 로컬 동작" 예외가 이 케이스로 확장되지
않는다. ADR-0049 가 "새 상호작용을 추가할 때마다 재판단하라" 고 남긴 조건의 첫 적용이고,
그 재판단 결과가 이 ADR 이다.

ADR-0040 의 테두리 우선순위와도 정합한다 — 점유 테두리가 완료 알림 테두리를 덮으므로 점유 중
남은 레코드가 서버 화면에서 테두리로 겹쳐 보이지 않고, 탭 제목·워크스페이스 배지 채널로만
남는다(ADR-0040 이 그 두 채널은 점유와 무관하게 동작한다고 이미 규정했다).

## Consequences

- **얻은 것**: 하드 점유의 "주체는 홀더" 원칙이 attention 축에서도 성립한다. 홀더가 보기 전에
  신호가 사라지지 않는다. ADR-0104 와 합쳐 해제 주체가 정확히 하나(홀더)로 확정된다.
- **잃은 것 / 의도된 변화**: 점유 중 서버 GUI 에서 그 surface 를 봐도 배지가 남는다. 서버
  사용자에게는 "봤는데 안 없어진다" 로 보일 수 있다 — 점유 마커(테두리)가 그 이유를 이미
  표시하고 있고, 점유가 풀리면 다음 포커스에서 정리되므로 stale 이 영구화되지는 않는다.
  "모두 읽음" 을 눌러도 점유 중 surface 의 배지는 남는다(알림 자체는 읽음 처리된다).
- **surface 단위 attach 홀더는 해제 채널이 없다.** `stream.open{target}` 으로 surface 하나만
  점유한 홀더는 그 surface 의 attention 을 해제할 수단이 없다 — 해제 요청을 받는
  `apply_attached_attention_clear` 가 `workspace_of_surface` → `workspace_holder` 로 인가하는데
  surface 단위 `acquire` 는 그 역매핑을 만들지 않고(ADR-0104 가 정한 인가 모델), raw 스트림
  클라이언트는 `ClientAttentionClear` 를 보내지도 않는다(GUI mirror 만 보낸다). 그래서 그
  경우 점유 중 해제 주체가 **0** 이 된다. 이 결정의 귀결이고 영구 고착은 아니다 — detach 하면
  서버 로컬 포커스가 해제 주체로 복귀해 정리된다. 워크스페이스 단위 attach(미러 사용자가 실제로
  화면을 보는 경로)에는 해당하지 않는다.
- **운영 비용 / 유지 부담**: 새 해제 producer 를 추가할 때 `clear_attention_local` 을 지나야
  한다 — primitive `clear_attention` 을 직접 부르면 게이트를 우회한다. 반대로 홀더의 해제를
  적용하는 경로는 **반드시** primitive 를 직접 불러야 한다(게이트에 자기 요청이 막힌다).
  두 함수의 역할 분리가 이 결정의 유지 조건이다.
- **검증 범위**: 렌더 경로는 GPU 없이 실행할 수 없으므로 게이트 술어와 로컬 축 진입점을 단위
  테스트가 값으로 고정한다(점유 중 유지 / 점유 해제 후 제거 / soft 무영향 / 알림 read 플래그
  독립 / 홀더 경로 비차단 / 미러 비차단) — `local_clear_is_disallowed_exactly_while_hard_occupied`
  와 `hard_occupied_surface_survives_the_local_focus_clear` 가 그 축의 진입점이다. 실제 렌더 포커스가 게이트를 밟는 것까지는 loopback
  e2e 가 실행한다 — GUI 서버 인스턴스의 활성 워크스페이스를 점유된 워크스페이스로 전환해
  `gpu.rs` 의 매 프레임 해제를 실제로 태우고, 해제가 일어났다면 반드시 따라오는 `kind: null`
  diff push 의 부재로 관측한다(서버 attention 을 직접 조회하는 IPC 가 없다). 게이트를 제거하면
  그 e2e 가 실제로 `kind: null` 프레임을 받아 실패한다.

## Alternatives Considered

- **A: 포커스 경로만 게이트하고 알림 읽음은 그대로 둔다** — 변경 범위가 가장 작다. 채택하지
  않았다: 서버 알림 패널의 "모두 읽음" 한 번으로 점유 중 배지가 전부 사라지는 구멍이 남아
  규칙이 둘로 갈린다.
- **B: 점유 중 알림 읽음 자체를 막는다** — 규칙이 단순해 보이지만 알림 패널은 이 인스턴스
  사용자의 것이고, 남의 점유 때문에 자기 알림을 정리할 수 없게 되는 것은 readonly 의 범위를
  넘는다(ADR-0040 은 PTY 조작만 막는다). 기각.
- **C: 점유 중에는 서버 로컬 포커스가 그 surface 에 걸리지 않게 한다** — 해제가 원천적으로
  일어나지 않는다. 기각: 포커스는 사용자 상태이고 점유는 그것을 뺏지 않는다(ADR-0040 은 관찰
  권한을 유지한다). 포커스에 붙은 다른 동작(선택·스크롤·soft 점유 청소)까지 함께 죽는다.
- **D: 관측자별 attention 레코드를 둔다** — 서버 사용자와 홀더가 각자 확인 상태를 갖게 하면
  게이트 자체가 불필요하다. 기각: `AttentionStore` 는 surface 당 레코드 1 개인 공유 primitive
  이고(ADR-0039/0062), 관측자 축을 도입하면 세 소비처(테두리·탭 제목·배지)와 push 프로토콜이
  전부 관측자 인자를 받아야 한다. 지금 필요한 것은 "주체가 누구인가" 하나다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **관측자별 attention** 이 필요해진다(위 대안 D) — 게이트의 전제("레코드는 하나, 주체도
  하나")가 무너진다. ADR-0104 의 같은 트리거와 연동된다.
- 하드 점유 중 서버 사용자에게 "홀더 대신 확인" 을 허용할 요구가 생긴다 — force-detach 없이
  배지만 정리하고 싶은 경우. 그때는 명시적 사용자 액션(컨텍스트 메뉴 등)으로 열지, 실-포커스
  같은 암묵적 판정으로 열지를 정해야 한다.
- 해제 판정 규칙이 "실 렌더 포커스 / 알림 읽음" 외로 확장된다 — 새 producer 가 로컬 축인지
  홀더 축인지 판정해 `clear_attention_local` 과 `clear_attention` 중 어디를 부를지 정해야 한다.
- 하드 점유 없이 mirror 를 만드는 모드가 생긴다 — `is_hard_occupied` 가 false 인 채로 원격
  관측자가 존재하게 되어 게이트가 걸리지 않는다(ADR-0104 의 forward 경로도 같은 조건에서
  홀더 검증 주체를 잃는다).

## References

- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 2계층 점유, 하드 점유의 주체와 readonly 정의
- [ADR-0049](0049-hard-occupancy-selection-exception.md) — 하드 점유 readonly 의 "PTY 상호작용 vs 순수 로컬" 축(이 ADR 은 다른 축)
- [ADR-0104](0104-mirror-attention-clear-forwarded-to-owner.md) — 미러의 해제 edge 를 소유 인스턴스로 전달(대칭 결정 + 게이트 위치 제약)
- [ADR-0039](0039-surface-highlight-shared-primitive.md) · [ADR-0062](0062-attention-store-kind-aware-primitive.md) — attention 공유 primitive 와 알림 읽음 엣지 케이스
- [features/surface-highlight](../features/surface-highlight/index.md) — 해제 권한 규칙
- [features/remote-attach](../features/remote-attach/index.md) — "점유 중 격리"
- [features/notifications](../features/notifications/index.md) — 알림 읽음 처리
