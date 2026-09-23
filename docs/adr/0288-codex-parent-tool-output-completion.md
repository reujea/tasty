# ADR-0288: Codex 부모에게 child 상태를 App Server 도구 결과로 전달한다 — ADR-0266의 Codex 부모 push 주체 개정

- **Status**: Superseded by [ADR-0291](0291-remove-the-codex-app-server-completion-channel.md) — Codex App Server 완료 전달 경로가 제거되어 본 결정의 대상이 사라짐
- **Date**: 2026-09-16
- **Tags**: codex, app-server, completion, outbox, lifecycle, plugin
- **Group**: agent-integration

## Context

child 완료를 부모 PTY에 넣으면 사용자 발화와 구분되지 않는다. Claude 부모는 Monitor가
completion-log를 구독하는 경로가 있지만 Codex 부모의 재개를 그 선례로 보장할 수 없다.
Codex 0.154.0의 App Server는 standalone toolOutput으로 idle 턴을 시작하고 regular
busy 턴에 결과를 큐잉한다. 같은 endpoint의 명시 remote TUI에서 이 동작이 관측됐다.
일반 TUI와 별도 daemon의 디스크 이력이 같아도 loaded 실행 주체가 같지는 않다.

busy turn의 ACK 뒤 interrupt 또는 daemon crash에서 queued output이 후속 이력과 모델
입력에 남지 않은 실측이 있다. 재송신도 멱등하지 않다. 따라서 수락·이력·소비를 같은
완료 상태로 합칠 수 없다. terminal.release는 surface를 남기므로 살아 있다는 판정만으로
spawn 구독을 유지하면 예전 부모에게 새 작업의 상태가 전달된다.

## Decision

ADR-0266 결정 1의 plugin 전용 push 주체를 Codex 부모에 한해 개정한다. 호스트가 영속 전이를 보관하고 파생 stale도 outbox로 보낸다. Claude 부모의 plugin 폴링·로그/Monitor 경로, claude-error-stalled 이벤트 이름, 상태를 성공으로 추정하지 않는 원칙, 기존 출력 스캐너의 시간 임계값은 개정하지 않는다.

부모 Codex에는 동일 endpoint의 정확한 loaded thread를 검증한 뒤
`turn/start`의 빈 input과 toolOutput을 보낸다. child Codex/Claude 모두 같은 호스트
outbox를 사용한다. 부모 Claude의 기존 Monitor/로그 경로는 유지한다. 사용자 메시지나
PTY 입력으로 완료를 대신하지 않는다.

호스트가 SQLite journal과 단일 background sender를 소유한다. plugin은 session·구독·상태
원인을 IPC로 보고한다. 송신 전 영속 claim, 관계 release, 명시 tell 종료를 같은 잠금에서
판정한다. 수락 불명은 자동 재송신하지 않고 기록 근거와 대조한다. 서버가 이미 수락한
결과를 구독 종료로 회수했다고 표시하지 않는다. 모델 소비는 별도 관측 없이는 주장하지 않는다.

hook session_id, thread.sessionId, thread.id를 별도 필드로 보관한다. 연결마다 loaded 목록과
실제 thread 응답을 대조하고 구독을 복원한다. 처음 보는 endpoint에 저장 이력을 resume하여
현재 TUI를 대신하지 않는다. surface는 재시작 후 주소이므로 세션과 기존 바인딩으로 재검증한다.

## Consequences

- **얻은 것**: 사용자 입력과 구분된 tool output, 부모별 채널 유지, 상태와 실패의 영속 조회, release 이후 오배송 방지.
- **잃은 것**: 연결 정보를 모르는 일반/private TUI를 자동으로 깨울 수 없다. 수락 불명 구간에 exactly-once를 약속하지 않는다.
- **운영 비용 / 유지 부담**: 지원 버전과 transport·history 계약을 직접 검증한다. 자동 재시도 상한 이후의 진단과 재바인딩이 필요하다. 이력 저장 여부와 모델 소비 여부를 분리해서 운영한다.

## Alternatives Considered

- **PTY tell / 사용자 message 큐** — 자동 상태를 사용자 발화로 바꾸므로 제외한다.
- **모든 부모에 completion-log** — Codex 부모의 자동 재개 계약을 충족하지 못한다.
- **child plugin마다 독립 sender** — 양 plugin과 release 사이에 영속 상태·경합 처리가 중복된다.
- **ACK를 완료로 간주하고 unknown을 재송신** — 큐 영속성과 멱등 수락이 보장되지 않아 채택하지 않는다.

## Reconsideration Triggers

**채널이 붙는 것** — 전송 코드가 사용자 input을 포함하거나, release와 송신 claim이 다른
저장소를 사용하게 되면 재검토한다. 현재 구현의 wire·lifecycle 시험과 호출 관계를 대조한다.
시험 초록만으로 모든 네트워크 중단점의 안전성을 주장하지 않는다.

**원리적으로 안 붙는 것** — upstream이 멱등 수락/영속 큐 receipt를 보장하거나 일반 TUI의
공유 접속 계약을 바꾸면 재검토한다. 새 버전의 실제 daemon/TUI에 격리 장애·재연결 시험을
수행하고 공식 계약과 대조한다.

## References

- 개정 대상: [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) (Codex 부모의 push 주체)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)

- [App Server 공식 문서](https://learn.chatgpt.com/docs/app-server)
- [Claude 부모 로그](../dev-guide/external-interaction.md#child-완료-알림--completion-log)
