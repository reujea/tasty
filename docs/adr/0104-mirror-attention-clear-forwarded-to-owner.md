# ADR-0104: attention 은 소유자만 발동하고, 확인(해제)은 실제로 본 주체가 한다 — 미러의 해제 edge 를 소유 인스턴스로 전달한다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: attention, surface-highlight, remote-attach, mirror, clear, edge-trigger, occupancy, stream-control
- **Group**: attention-notification

## Context

[ADR-0098](0098-mirror-local-attention-raise-suppressed.md) 이 attention 의 **발동** 권한을
surface 소유 인스턴스로 단일화했다. 미러는 서버 push 를 반영만 하고 자기 판단으로 레코드를
만들지 않는다.

해제 축에는 대칭되는 장치가 없었다. attention 해제 producer 는 둘뿐이고 **둘 다 그 인스턴스의
로컬 GUI 사건**이다 — 렌더 시점 실-포커스(`src/gfx/gpu.rs`)와 알림 읽음 처리
(`mark_notification_read` / `mark_all_notifications_read`). 미러 사용자의 어떤 행동(포커스,
입력, 스크롤)도 서버의 이 두 경로를 발동시키지 못하고, 서버 사용자는 하드 점유된 surface 에
대해 readonly 라 그 surface 를 굳이 포커스할 이유가 없다. 결과적으로 원격 attach 중에는
**해제 주체가 아예 없어** 서버 사이드바의 배지가 영구히 남는다.

해제 규칙 자체는 이미 하나로 정해져 있다 — "실제 렌더 시점 포커스 = 확인". 필요한 것은 새
정책이 아니라, 그 판정이 미러에서 일어났을 때 **결과를 소유 인스턴스로 옮기는 통로**다.

## Decision

**미러에서 발생한 attention 해제 edge 를 `StreamControl::ClientAttentionClear`(client→server)로
소유 인스턴스에 1 회 전달하고, 서버가 자기 레코드를 지운다.** 발동은 소유자만 하고, 확인은
그 surface 를 실제로 보고 있는 주체가 한다 — ADR-0098 과 방향이 반대인 것이 아니라, 두 축의
**권한 주체가 원래 다르다**는 사실을 그대로 옮긴 것이다.

세부 결정 셋:

1. **edge 는 반환값으로 도출한다.** `CoreState::clear_attention` 이 "실제로 레코드를
   제거했는지" 를 반환하고 그 `true` 가 곧 edge 다. 레코드가 없는 상태의 호출은 전부 no-op
   이라, 매 프레임 도는 실-포커스 해제에서도 상태가 바뀐 순간에만 신호가 나간다.
   **포커스 상태를 주기적으로 보내지 않는다.** 별도 last-sent 추적이 필요 없다.
2. **큐 push 는 호출부가 아니라 `clear_attention` 안에서 한다.** 세 호출부 모두 미러에서
   발생할 수 있다 — 미러 로컬 Bell/OSC 9 로 만들어진 알림을 미러 알림 패널에서 읽음 처리하는
   경우가 포함된다. 포커스 경로에만 큐잉하면 알림으로 확인한 경우가 전달되지 않아 다음
   push 에서 배지가 되살아난다.
3. **인가는 기존 모델 그대로 — "attach 하드 점유 = 그 워크스페이스의 주체"**(ADR-0040).
   서버는 anchor surface 의 워크스페이스 holder 인지 검증한 뒤에만 적용한다. `ClientResize` /
   `StructuralOp` 가 이미 같은 원칙으로 서버 상태를 바꾼다.
4. **forward 가 걸리는 판정은 "미러 사용자의 실 렌더 포커스(및 미러 로컬 알림 읽음)" 하나
   뿐이다.** 에이전트가 IPC 로 해제를 요청하는 표면(`surface.attention_clear`)은 mirror
   surface 를 대상으로 하면 **거절되며**, 따라서 이 forward 를 타지 않는다 — 미러
   인스턴스의 에이전트는 원격 surface 를 소유하지도, 그것을 실제로 보고 있지도 않다.
   ADR-0098 이 발동 축에서 내린 판단("미러 인스턴스의 에이전트는 표시 권한이 없다 —
   필요하면 서버 IPC 로 원격 id 를 지정")과 대칭이다. 그 거절의 집행은 해당 IPC 를
   도입하는 트랙이 담당한다.

**후속 작업에 거는 제약 — 점유 게이트를 `clear_attention` 안에 두지 말 것.** 하드 점유 중
해제 권한을 홀더로 제한하는 후속 작업이 raise 게이트와 같은 "단일 진입점" 패턴을 따라
`CoreState::clear_attention` 안에 `is_hard_occupied` 검사를 넣으면, 서버측 적용
(`apply_attached_attention_clear` → `clear_attention`)이 곧바로 막혀 **해제 주체가 다시 0 이
된다**(서버 surface 는 점유돼 있고, 요청자는 이미 holder 로 검증된 뒤다). 그 게이트는
호출부(실-포커스 `gpu.rs`, 알림 읽음)에 두어야 한다. 부득이 진입점에 두어야 한다면
`apply_attached_attention_clear` 가 게이트를 우회하는 전용 경로를 쓰도록 함께 바꾼다.

불가침 원칙 1(사용자 행동 / 에이전트 행동 분리) 검토: 미러의 실-포커스는 그 워크스페이스를
하드 점유한 정당한 주체의 **사용자 행동**이고(에이전트 주입 포커스가 아니다), 전달되는 효과는
producer-중립 공유 상태인 attention 레코드 제거 하나뿐이다. 서버 사용자의 포커스·선택·스크롤·
닫은 항목 히스토리 어디에도 닿지 않는다.

## Consequences

- **얻은 것**: 원격 attach 중에도 해제 주체가 존재한다. 미러에서 확인한 surface 의 배지가
  서버에서도 사라진다. ADR-0098 과 합쳐, 미러 store 를 바꾸는 로컬 경로가 남지 않아 미러와
  서버의 attention 이 정의상 일치한다.
- **에코 루프 없음**: 미러 clear → 서버 clear → 다음 diff 가 `kind: null` 을 미러로 push →
  미러엔 이미 레코드가 없어 edge 가 없다. idempotent 하게 수렴한다. 서버 push 적용
  (`set_mirror_surface_attention`)과 teardown(`forget_mirror_surface_attention`)이
  `clear_attention` 을 타지 않는 것이 이 성질의 구조적 근거다.
- **알려진 엣지(의도된 동작)**: 미러가 그 surface 를 **이미 포커스한 상태**에서 서버가 새
  raise 를 push 하면, 미러는 레코드를 심자마자 다음 렌더 프레임에서 지우고 해제 edge 를 보내
  배지가 사실상 뜨지 않는다. 단일 인스턴스 로컬 동작과 같은 규칙이므로(포커스된 surface 에
  raise 하면 지금도 다음 프레임에 지워진다) 그대로 둔다.
- **잃은 것**: 서버 사용자가 "미러 사용자가 아직 안 봤다" 를 배지로 알 수 없다 — 확인 주체가
  누구든 배지는 하나이고 함께 사라진다. 관측자별 attention 이라는 개념 자체가 없으므로 이는
  현재 모델의 귀결이지 이 결정이 새로 만든 손실이 아니다.
- **운영 비용 / 유지 부담**: 전송 실패는 재시도 없이 drop 한다 — edge 신호라 큐를 쌓아 두면
  뒤늦게 엉뚱한 시점에 지우게 되고, 세션이 끊기는 중이면 서버측 점유도 곧 풀린다.
  `clear_attention` 의 반환값이 프로토콜 의미를 갖게 됐으므로, 새 해제 경로를 추가할 때 이
  함수를 우회하면 그 경로의 확인이 서버에 전달되지 않는다.
- **검증 범위(무엇이 고정되고 무엇이 안 되는가)**: 서버 attention 을 직접 조회하는 IPC 가
  없어, loopback e2e 는 diff push 의 성질로 관측한다 — 해제 프레임을 보내면 서버가
  `kind: null` 을 되돌려 push 하고, 같은 kind 로 재발동했을 때 프레임이 다시 도착한다
  (레코드가 남아 있었다면 dedup 이 재전송을 막는다).
  - **고정되는 것**: (미러측) 제거 edge 판정과 forward 큐 적재 — 세 해제 경로 전부, 그리고
    반복 호출이 edge 를 다시 만들지 않는다는 성질까지 단위 테스트가 값으로 고정한다
    (`attention_forwards_only_on_change` · `attention_forwards_resets_on_reacquire` ·
    `mirror_apply_does_not_touch_forward_cache`).
    (서버측) wire 형태의 역직렬화·분류, holder 연결의 요청이 검증을 통과해 레코드가 실제로
    지워진다는 것, 빈 레코드에 반복 요청이 와도 프레임이 나가지 않는다는 것 — loopback
    e2e 가 실제 인스턴스에서 고정한다.
  - **고정되지 않는 것**: 미러측 큐 → drain → 소켓 전송 구간(`about_to_wait` 결선 포함).
    loopback client 는 raw `TcpStream` 이라 미러 `AttentionStore` 자체가 없어 그 구간을
    실행하지 않고, 프레임을 손으로 만들어 보낸다. holder 가 아닌 client 의 요청이 무시되는
    음성 경로도 테스트가 없다. 같은 패턴을 쓰는 다른 client→server forward 5 종
    (`ClientResize` 등)도 동일한 상태다 — 이 구간의 검증에는 GUI 두 인스턴스 attach 가
    필요하고, 헤드리스 작업 환경에서는 실행할 수 없다.

## Alternatives Considered

- **A: 미러의 포커스 상태를 주기적으로 서버에 보낸다** — 서버가 "지금 미러가 이 surface 를
  보고 있다" 를 알면 해제를 서버가 판단할 수 있다. 채택하지 않았다: 1Hz 든 프레임 단위든
  아무 일도 없는 동안 계속 트래픽이 나가고, 서버에 미러 포커스 상태를 저장하는 새 상태가
  생긴다. 필요한 정보는 "확인했다" 는 **사건 하나**뿐이라 edge 로 충분하다.
- **B: 호출부(`gpu.rs`)에서만 forward 큐에 넣는다** — 변경 범위가 가장 작다. 채택하지 않은
  이유는 위 Decision 2 — 미러 알림 패널의 읽음 처리가 누락돼 배지가 되살아난다.
- **C: 서버가 미러의 입력·스크롤을 보고 스스로 해제한다** — 새 프레임이 필요 없다. 채택하지
  않았다: "확인" 판정 규칙이 소유 인스턴스와 미러에서 서로 달라지고(입력 없이 포커스만 해도
  확인인데 서버는 그걸 볼 수 없다), 단일 규칙("실 렌더 포커스 = 확인")이 깨진다.
- **D: 미러에서는 해제를 아예 막는다**(로컬 발동 억제와 대칭으로) — 일관돼 보이지만 미러
  사용자가 자기 화면의 배지를 영영 지울 수 없게 된다. 발동과 해제는 권한 주체가 다르다는
  것이 이 ADR 의 요지다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **관측자별 attention** 이 필요해진다 — 서버 사용자와 미러 사용자가 각자 확인 상태를 갖는
  모델로 가면 "하나의 레코드를 누가 지우든 함께 사라진다" 는 전제가 무너진다.
- 하드 점유 없이 mirror 를 만드는 모드가 생긴다 — holder 검증이 통과할 주체가 없어 해제
  전달 경로가 끊긴다(같은 조건에서 서버→미러 push 도 소스를 잃는다).
- 해제 판정 규칙이 "실 렌더 포커스" 외로 확장된다(예: 타이머 만료, 명시적 확인 액션) — 그때는
  어떤 판정이 소유 인스턴스로 전달될 자격이 있는지 다시 정해야 한다.
- 서버 attention 을 직접 조회하는 IPC 표면이 생긴다 — e2e 의 간접 관측을 직접 assert 로
  바꾼다.

## References

- [ADR-0098](0098-mirror-local-attention-raise-suppressed.md) — 발동 축의 대칭 결정(미러 로컬 발동 억제)
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — 하드 점유 = 홀더가 그 surface 의 주체
- [ADR-0039](0039-surface-highlight-shared-primitive.md) · [ADR-0062](0062-attention-store-kind-aware-primitive.md) — attention 공유 primitive 와 kind 정책
- [features/surface-highlight](../features/surface-highlight/index.md) — 해제 규칙과 mirror 전파
- [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) — "주의 환기(attention) 전파" 의 결선 상세
