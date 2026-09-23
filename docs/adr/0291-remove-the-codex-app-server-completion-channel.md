# ADR-0291: Codex 부모의 App Server 완료 전달을 제거한다 — 완료 채널을 로그 하나로 되돌린다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: codex, app-server, completion, outbox, removal, lifecycle, plugin
- **Group**: agent-integration

## Context

[ADR-0288](0288-codex-parent-tool-output-completion.md) 이 부모 Codex 에게 child 상태를 Codex
App Server 의 도구 결과로 전달하기로 정했고, 그 결정이 구현돼 main 에 들어갔다. 그 경로는 부모
종류로 채널을 갈랐다 — Codex 부모는 검증된 App Server thread, Claude 부모는 기존 완료 알림
로그다.

그 구현은 다음을 요구했다: 호스트 안의 SQLite outbox 와 그것을 소비하는 background sender 스레드,
WebSocket/TLS 프로토콜 클라이언트, endpoint 와 thread 소유를 매 연결에서 재검증하는 바인딩 상태
기계, 종료 저장을 되살리는 영속 종료 큐, 그리고 그 전부를 부모·child 의 논리 세션과 실행 세대에
묶는 identity 대조. 사용자가 그 비용 대비 이득을 다시 보고 **기능을 제거하기로 결정했다.**

결정의 대상은 이름으로 다음과 같다. 호스트 IPC `terminal.completion` 과 `terminal.completion_bind`,
plugin IPC `codex.completion`, CLI `tasty codex completion` 의 bind·status·diagnose·retry·unsubscribe,
호스트 쪽 `src/core/completion` 의 outbox·sender·protocol·복구, 호스트 데이터 루트의
`completion.sqlite3` journal, `tasty-plugin-agent-common` 의 완료 전달 어댑터, 그리고 codex
매니페스트가 그 bind 때문에 들고 있던 `network` 권한이다.

## Decision

**App Server 완료 전달 경로를 레포에서 제거하고, 완료 알림 채널을 부모 종류와 무관한 로그 한
줄로 되돌린다.** 위 Context 가 이름으로 열거한 것이 제거 대상 전부다. 그와 함께 그 경로에만
쓰이던 세션 동일성 게이트, `surface.close` 응답의 완료 정리 필드, 관계 세대를 재는 서술이 사라진다.

**유지하는 것을 이름으로 못박는다** — 안 바뀌는 것을 적지 않으면 다음 사람이 유지 대상까지
의심한다.

- `<parent_home>/notify/<caller_surface>.log` append 채널. 그 채널은 **부모 종류를 보지 않는다.**
- 부모 Claude 가 그 로그를 Claude Code 내장 Monitor 로 tail 해 받는 수신 경로.
- `tasty codex tell` · `tasty claude tell` 이 쓰는 `terminal.tell` — 완료 알림이 아니라 메시지 전달
  그 자체다.
- child 상태 hook 6 종과 그것이 심는 surface 상태·meta.
- `tasty codex install` 의 `--codex-home` · `--config-file` — App Server 와 독립인 훅 설치 기능이다.
- `surface.completion` — 이름만 닮은 주의 환기(attention) 메서드이고 권한 경계도 다르다.
- `completion_strategy` 기여점 — DAG 완료 판정 전략이라 이 결정과 무관하다.

[ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) 결정 1 에 대한 0288 의 부분 개정도
함께 철회된다 — Codex 부모의 push 주체를 호스트로 옮긴 그 조항이 대상 없이 남기 때문이다. 0266
결정 1 이 원상 복귀한다.

## Consequences

- **얻은 것**: 호스트가 네트워크 클라이언트를 들고 있지 않게 된다. 완료 채널이 하나라 "어느 부모냐"
  라는 분기가 코드·문서·사용자 가이드 전역에서 사라지고, 실패 모드도 하나가 된다. 바인딩 상태·
  outbox phase·세대 대조가 만들던 상태 공간이 통째로 없어진다.
- **잃은 것**: 부모 Codex 의 **자동 재개**가 없어진다. 완료는 여전히 로그에 남지만, 그것을 읽어
  다음 턴을 여는 도구가 Codex 쪽에는 없다. 부모 Codex 로 무인 흐름을 돌리려면 사람이 폴링하거나
  별도 도구를 붙여야 한다. 또한 기존 설치본의 `<tasty_home>/completion.sqlite3` 는 아무도 읽지
  않는 고아 파일로 디스크에 남는다 — 마이그레이션 삭제 루틴을 만들지 않기로 했다. 남아 있어도
  동작에 영향이 없고, 사용자가 취할 행동이 없어 사용자 가이드에도 적지 않는다.
- **발행 표기를 하지 않는 근거**: 이 기능을 기술한 `CHANGELOG.md` 항목은 **전부 미발행
  (`[Unreleased]`) 구간에 있다.** 어떤 릴리스에도 나간 적이 없으므로 알릴 대상이 없다 — 그래서
  제거 항목을 새로 만들지 않고, 파괴적 변경 표기도 붙이지 않고, major 를 올리지도 않는다.
  해당 항목들은 추가된 적 없는 것처럼 지운다.
- **운영 비용 / 유지 부담**: 줄어든다. 유지해야 할 외부 계약(App Server 버전 기준, historyMode,
  paginated items cursor, 인증 환경변수 수명)이 전부 사라진다. 대신 완료 수신이 부모 쪽 도구에
  의존하게 되므로, 그 도구가 없는 부모에서는 수신이 **사람의 일**이 된다.

## Alternatives Considered

- **A — 기능을 유지하며 개선한다**: unknown 해소율과 재바인딩 절차를 다듬는 방향. 안 고른 이유는
  비용이 구현 품질이 아니라 **외부 계약의 수**에 있기 때문이다. 다듬어도 App Server 버전·
  historyMode·trust 화면이라는 바깥 변수는 그대로 남는다.
- **B — 축소 유지**: 호스트 메서드에서 라우팅 액션 하나만 남기고 바인딩·outbox·복구를 걷어내는
  안. 안 고른 이유는 그 하나가 살아 있으면 journal 과 세대 대조가 함께 살아야 하고, 결국 문서와
  권한 표에 "부모 종류로 갈린다" 는 프레임이 남기 때문이다. 절반만 지우면 비용은 남고 이득만
  준다.
- **C — 유예 기간을 둔다**: 한 릴리스 동안 경고를 띄우고 다음에 제거하는 안. 안 고른 이유는 위
  Consequences 의 발행 표기 근거와 같다 — 발행된 적이 없어 유예를 알릴 수신자가 없다. 유예는
  기간만 늘리고 그동안 두 경로를 다 유지하게 만든다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `crates/tasty-ipc` 의 `METHOD_TABLE` 에 완료 전달용 호스트 메서드가 다시 등재된다. 그 표가
  권한의 단일 정본이라, 어떤 경로로 되살아나든 이름이 거기 들어온다.
- codex 매니페스트의 `permissions` 에 `network` 가 다시 들어온다. 이 결정이 그것을 뺀 이유가
  bind 하나였으므로, 재등장은 같은 성질의 외부 연결이 다시 생겼다는 뜻이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 부모 Codex 로 무인 흐름을 돌리는 사용자가 완료를 놓친다는 보고가 반복된다. 재는 법: 그 보고
  건수를 세고, 같은 기간에 부모 Codex 가 로그를 스스로 tail 할 수단(상류 CLI 의 background task
  수신 도구 등)이 생겼는지 확인한다. 수단이 생겼다면 되살릴 것은 이 경로가 아니라 로그 구독이다.

## References

- 제거 대상: [ADR-0288](0288-codex-parent-tool-output-completion.md) (Codex 부모의 App Server
  도구 결과 전달)
- 0288 이 [ADR-0266](0266-derived-stale-must-reach-the-push-channel.md) 결정 1 에 걸었던 부분
  개정을 철회한다 — 그 조항이 원상 복귀한다.
- 제거는 이 ADR 과 같은 브랜치의 세 커밋으로 나뉘어 착지했다. 커밋 제목은 순서대로
  `refactor(agents): drop the parent-kind gate on completion logging` ·
  `refactor(agents): drop the plugin side of App Server completion delivery` ·
  `refactor(agents): drop the host side of App Server completion delivery` 다. 해시가 아니라
  제목으로 적는 이유는 이 브랜치가 병합 전에 다시 rebase 될 수 있어 해시가 바뀌기 때문이다.
- 살아남는 채널의 동작·운영: [external-interaction 완료 알림](../dev-guide/external-interaction.md#child-완료-알림--completion-log)
