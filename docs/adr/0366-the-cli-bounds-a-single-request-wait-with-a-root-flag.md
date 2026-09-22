# ADR-0366: CLI 는 단발 요청의 응답 대기를 루트 플래그로 자르고, 못 거는 상대와 못 싣는 명령은 거절한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, cli, envelope, response-timeout, capability, compatibility, adr-0312, adr-0328, adr-0365
- **Group**: ipc-transport

## Context

[ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md)
은 응답 대기 상한을 **요청 봉투**에 실었다(`response_timeout_ms`). 없으면 상한이 없고, 있으면
서버가 그 시간에 응답 통로를 놓고 `-32061`(결과 불명)로 답한다. 그 ADR 은 "오늘의 호출자는
아무도 이 필드를 안 보낸다" 를 한계로 적고, CLI·plugin client 가 싣는 것을 각자의 후속
결정으로 남겼다.

CLI 에는 그 필드를 채우는 자리가 **여섯** 있었고 전부 `None` 이었다 — 정적 CLI 의 단발 RPC
하나, plugin CLI 의 단발 요청 하나, 그리고 요청을 여러 번 보내는 루프 셋(사건 따라가기 ·
감사 따라가기 · plugin 의 자동 대기)과 SSH 너머 원격을 한 번 찔러 보는 조회 하나. 상한을
걸고 싶은 에이전트는 CLI 로는 그럴 수 없었다([identity §2.2](../identity.md) 원칙 2).

기존 `--timeout-ms`(`approval`·`agent` 의 몇 명령)는 **메서드 인자**라 다른 축이다 — 서버가
그 메서드 안에서 기다리는 시간이고, 봉투 상한은 전송 계층이 응답을 기다리는 시간이다.

## Decision

**루트 플래그 `--response-timeout-ms <MS>` 를 두고, 요청 하나로 끝나는 명령의 봉투에만
싣는다. 상대가 `ipc.response-timeout` 을 선언하지 않으면 보내지 않고, 요청을 하나로 못 싣는
명령은 플래그를 거절한다.**

- **자리는 루트다** — `tasty --response-timeout-ms 500 read screen`. `--port-file` 과 같은
  자리이고 같은 이유다: 명령이 아니라 **이 호출의 전송**에 대한 값이다. clap `global` 로
  서브커맨드 뒤에도 받게 하지 않는다 — plugin 매니페스트가 선언하는 인자와 이름이 부딪히면
  plugin CLI 트리 조립이 깨진다.
- **싣는 자리**: 정적 CLI 의 단발 RPC 와 plugin CLI 의 단발 요청, 둘. 봉투에 싣는 것은
  요청을 만든 뒤 한 번이다(`contract::Envelope::apply`) — 요청 빌더 여섯 자리를 각각 고치지
  않는다.
- **`0` 은 없는 것과 같다.** 봉투 규약상 `Some(0)` 은 상한 없음이라 싣지 않는다. 실으면 구
  서버에 계약 확인만 붙고 뜻은 같다.
- **못 거는 상대는 거절한다.** 플래그가 실린 요청은 [ADR-0365](0365-the-output-cursor-contract-is-negotiated-by-name-before-the-cli-sends-it.md)
  의 사전 확인을 거친다. 선언이 없으면 요청을 내보내지 않고 같은 구조화 거절
  (`{"error":{"kind":"unsupported_capability",…,"sent":false}}`, 종료 코드 1)로 끝난다.
- **못 싣는 명령은 거절한다.** 클라이언트 주도 명령(루프 · 스트림 · 로컬 처리 · SSH 조회)과
  plugin 의 폴링·자동 대기 명령은 플래그를 받으면 통신을 시작하기 전에 사용 오류(종료 코드
  2)로 끝난다. 봉투 상한은 요청 하나의 대기를 자르는 값이라 요청이 여럿인 루프에 걸 뜻이
  정해지지 않고, `events.fetch --wait-ms` 같은 긴 폴링은 걸면 매 회 `-32061` 로 끊긴다.
  **서브커맨드가 없는 호출**(GUI 기동 · augmented help)도 실을 요청이 없으므로 진입점
  라우팅에서 같은 exit 2 로 거절한다.
- **만료는 그대로 전달한다.** `-32061` 은 다른 호스트 오류와 같은 모양(`Error (-32061): …`,
  종료 코드 1)이다. 그 코드가 "결과 불명 — 부수효과가 남는 메서드는 상태를 먼저 읽어라" 를
  말한다.

## Consequences

- **얻은 것**: 에이전트가 CLI 로 자기 노출을 자를 수 있다. 굳은 핸들러에 걸린 호출이 그 시간
  뒤에 결과 불명으로 돌아온다.
- **얻은 것**: 플래그가 **조용히 무시되는 갈래가 없다** — 상대가 못 읽으면 안 보내고, 명령이
  못 실으면 안 돈다.
- **잃은 것**: 플래그를 준 호출은 연결마다 `system.info` 왕복이 하나 는다.
- **잃은 것 / 한계**: 루프 명령에는 상한을 걸 방법이 없다. 필요하면 그 명령의 한 회차에 대한
  뜻을 먼저 정해야 한다.
- **운영 비용**: 새 클라이언트 주도 명령은 자동으로 거절 쪽에 들어간다(`run_client` 의 분기가
  명령 종류가 아니라 `Dispatch` 갈래로 판정한다). 새 단발 명령은 자동으로 싣는 쪽이다.

## Alternatives Considered

- **A: 상대가 선언이 없으면 상한 없이 보내고 경고만 낸다** — 안 골랐다. 플래그를 준 호출자는
  "이 시간 뒤에는 돌아온다" 를 전제로 다음 일을 짠다. 그 전제가 조용히 사라진 채 무한 대기에
  들어가는 것이 ADR-0328 이 적은 결함 그대로다. 보내지 않는 쪽은 호출자가 즉시 안다.
- **B: 환경변수(`TASTY_RESPONSE_TIMEOUT_MS`)** — 안 골랐다. 자식 에이전트에게 상속돼, 그
  에이전트가 부르는 `agent task-await` 같은 일부러 긴 호출까지 잘린다. 플래그는 그 호출에만
  걸린다.
- **C: 서브커맨드 뒤에도 받는 `global` 플래그** — 안 골랐다(위 Decision).
- **D: 루프 명령에서는 조용히 무시한다** — 안 골랐다. 위 A 와 같은 결함을 CLI 가 스스로
  만든다.
- **E: 기존 `--timeout-ms` 와 합친다** — 안 골랐다. 서버 안에서 기다리는 시간과 전송이 응답을
  기다리는 시간은 다른 값이고, 합치면 `approval await --timeout-ms 0`(사람을 끝까지 기다림)이
  봉투 상한 0 과 같은 뜻인지 다른 뜻인지 매번 다시 정해야 한다.

## Reconsideration Triggers

**채널이 붙는 것**

- `--response-timeout-ms` 가 루트가 아닌 자리로 옮겨질 때. 좌변: `tasty-cli` `contract.rs` 의
  시험 `the_root_flag_parses_before_the_subcommand`.
- `0` 이 봉투에 실리기 시작할 때. 좌변: 같은 파일의 `a_zero_bound_is_not_carried`.
- 서브커맨드 없는 호출이 플래그를 조용히 버리기 시작할 때. 좌변: `tests/cli_help_locales.rs` 의
  `a_reply_bound_without_a_command_is_refused_instead_of_dropped`.

**원리적으로 안 붙는 것**

- 루프 명령에도 상한이 필요하다는 요청이 반복될 때. 재는 법: 사용자·에이전트가 그 명령에
  플래그를 줘 종료 코드 2 를 받은 보고가 나오는지 본다.
- plugin client 가 이 필드를 싣기 시작할 때. plugin wire 쪽 선언 자리는 이 결정 밖이다.
  재는 법: `git grep -n 'response_timeout_ms: Some' crates/tasty-plugin-*`.

## References

- [ADR-0328](0328-the-response-wait-is-bounded-by-the-caller-and-expiry-means-the-outcome-is-unknown.md)
  — 봉투 상한과 결과 불명의 뜻. 이 결정은 그 ADR 이 후속으로 남긴 "CLI client 가 싣는 것" 이다.
- [ADR-0365](0365-the-output-cursor-contract-is-negotiated-by-name-before-the-cli-sends-it.md)
  — 보내기 전 확인과 구조화 거절의 모양
- [ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md) — 선언 자리
- 부분 개정: [0452](0452-the-cli-capability-check-spends-the-same-response-bound.md) (사전 확인에 시간 상한이 없던 것 — 조항 개정)
- 코드 근거(결정이 실현된 현재 위치): `tasty-cli` 의 `Cli::response_timeout_ms` ·
  `contract::Envelope` · `run::run_client_with`, 본체의 `boot::cli_routing::Routed::Subcommand`
