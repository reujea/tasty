# ADR-0284: Self-attach 거절은 RTT가 아니라 connector 진입 사건으로 판정한다

- **Status**: Accepted
- **Date**: 2026-09-15
- **Tags**: testing, attach, flake, diagnostics, connector, adr-0181
- **Group**: testing

## Context

Self-attach는 GUI 메인 루프가 자기 핸드셰이크 응답을 기다리는 동기 호출이므로 client
진입 단계에서 거절한다(ADR-0116). 기존 통합시험의 `ui.state` 최악 RTT 2초 단언은
거절 누락과 러너 지연을 함께 검출했다. 점유를 폴링하는 동안 루프가 막히면 그 구간의
점유를 읽지 못하고, 뒤의 정상 재attach도 성공할 수 있어 RTT만 지우면 관측을 잃는다.

기존 로그의 거절 문구도 반환 전에 찍혔다. 그 문구의 존재만 확인하면 뒤의 return을
제거한 변경을 놓친다. IPC의 queued 응답은 dispatch 전이며, headless는 그 GUI 큐를
처리하지 않으므로 같은 초록을 낼 수 있다.

## Decision

IPC와 사용자 GUI attach는 하나의 private dispatcher로 자기 포트 비교와 connector
진입을 묶는다. 결과는 `RejectedSelf` 또는 실제 connector의 `Result`다. 자기 요청에서는
connector 0회, 그 외에는 1회와 성공값/오류 전달을 직접 시험한다.

Debug GUI 통합시험은 dispatcher **반환 뒤**의 요청별 완료 기록과 실제 connector
진입 횟수를 확인한다. 진입 횟수는 connector를 감싼 호출에서 세므로, 거절 결과만 잘못
돌려줘도 호출1회가 감춰지지 않는다. 관측은 별도 debug-only 모듈의 로컬 기록이며 새
release IPC나 전역 ticker를 만들지 않는다. 기록이 없으면 큐잉 성공만으로 통과하지 않는다.

점유 없음/정상 재attach와 기존 RTT 진단 열은 유지한다. RTT는 정확성 gate에서 제외하며,
기존 관측 창과 IPC 응답 안전망을 늘리지 않는다. Headless의 GUI 큐 미처리 검증은 별도
시험으로 구분한다. ADR-0181의 구조를 사건으로 검사하는 처방을 이 자리에 적용한다.

자기 포트 거절 정책, IPC 포커스 독립성, 사용자 성공의 포커스 이동, 터널 소유권,
서버측 점유/핸드셰이크 순서와 raw stream self-client 계약은 변경하지 않는다.

## Consequences

- **얻은 것**: gate 또는 return을 제거하면 connector 호출 사건으로 실패한다. 느린 정상
  IPC 왕복만으로 self-attach 정확성 시험을 실패시키지 않는다.
- **잃은 것**: 이 시험은 RTT 성능 상한을 보장하지 않는다. 그 값은 진단으로 남는다.
- **운영 비용**: debug 완료 기록은 기존 유한 stderr ring을 이용하므로 통합시험이 해당
  요청 기록을 발견하면 보존해야 한다. 기록이 유실되면 성공이 아니라 관측 실패로 끝난다.

## Alternatives Considered

- **RTT 상한 인상/재시도/ignore** — 원인을 분리하지 않고 관측을 약화한다. 기각.
- **sleep 드리프트나 전역 ticker** — 자기 응답 대기와 OS 스케줄 지연의 원인 분리를
  단독으로 보장하지 않는다. 정확한 connector 경계가 있으므로 추가하지 않는다.
- **기존 거절 경고만 확인** — 경고 뒤 return 제거를 놓친다. 기각.
- **순수 포트 비교만 시험** — 실제 connector 호출 배치가 검사 밖으로 빠진다. 기각.
- **새 release 상태/조작 API** — debug 검증의 요구이며 사용자/에이전트 계약 확장이
  필요하지 않다. 기존 stderr 경로로 한정한다.

## Reconsideration Triggers

**채널이 붙는 것**

- 공통 dispatcher에 두 source 값을 주었을 때 Self 요청이 connector에 들어가거나 결과 전달이 달라지면
  `both_sources_reject_self_without_entering_the_connector`와
  `both_sources_enter_once_and_return_the_connector_result`가 실패한다.
- 실제 GUI 요청의 완료/거절/0회 기록이 사라지면 통합시험
  `self_attach_is_rejected_before_it_can_take_occupancy`가 실패한다.

**원리적으로 안 붙는 것**

- 완료 로그가 정상 실행에서도 ring에서 자주 유실되면 관측 경로를 재검토한다.
  재는 법: 실패 실행의 전체 stderr와 요청 port/workspace를 보존하고 dispatcher 미실행과
  기록 퇴출을 구분한다. RTT 상한을 다시 정확성 gate로 쓰지는 않는다.

## References

- [attach 동작/검증](../dev-guide/attach-behavior.md#self-attach-거절-검증)
- [ADR-0116](0116-attach-handshake-validated-before-occupancy.md) — self 거절과 점유 전 검증
- [ADR-0181](0181-a-latency-assertion-must-carry-a-control-that-load-moves-and-code-does-not.md) — 구조는 사건으로 검사
- 코드 근거(현재 구현): `src/app/attach_client.rs`의 두 `try_dispatch_one_gui_attach_*`,
  `src/app/attach_client/dispatch.rs`, `src/app/attach_client/dispatch/debug_completion.rs`,
  `tests/attach_silent_disconnect.rs`
