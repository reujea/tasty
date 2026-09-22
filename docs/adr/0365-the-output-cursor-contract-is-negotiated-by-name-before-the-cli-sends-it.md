# ADR-0365: 출력 위치 계약은 이름으로 협상하고, CLI 는 그 이름을 확인한 뒤에만 위치를 싣는다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, capability, compatibility, cli, terminal, output, cursor, adr-0307, adr-0312, adr-0341
- **Group**: event-feed

## Context

[ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md) 이
`surface.read_since_mark` 에 인자 셋(`cursor` · `stream` · `max_bytes`)을 더했다. 인자를 더한
것은 이름을 늘리지 않으려는 선택이었고, 그 선택의 값이 하나 남아 있었다 — **그 인자를 모르는
서버는 그것을 조용히 버린다.** 인자 객체에는 모르는 키 거절이 없으므로 구 서버는 공유 마크에서
읽어 성공으로 답한다. 응답에는 `text` 가 있고 `next_cursor` 가 없다. 호출자는 자기 위치가
적용됐다고 믿지만 실제로는 다른 소비자가 미는 창을 읽었다.

이 물음에 답하는 자리는 이미 있다. 서버는 `system.info` 의 `capabilities` 로 협상 가능한
계약을 선언하고([ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md)),
client 쪽에는 보내기 전에 묻는 기구(`IpcConnection::require_capability` 와 그 실패 타입
`UnsupportedCapability`)가 있다. 비어 있던 것은 둘이다.

- 선언 목록에 **위치 계약의 이름이 없었다.** 응답 대기 상한(`ipc.response-timeout`)과 멱등
  키(`ipc.idempotency-key`)는 이름을 받았는데 세 번째 새 계약은 못 받았다.
- **그 기구를 부르는 소비자가 0 이었다.** CLI 에는 위치 인자 자체가 없었다 —
  `tasty read since-mark` 는 `--surface` · `--strip-ansi` 둘뿐이라 IPC 로만 할 수 있는 에이전트
  기능이었다([identity §2.2](../identity.md) 원칙 2).

## Decision

**위치 계약에 `ipc.output-cursor` 라는 이름을 주고, 판은 인자 이름과 같은 모듈에서 싣는다.
CLI 는 위치 인자를 실은 요청을 보내기 전에 그 이름을 묻고, 없으면 요청을 내보내지 않은 채
구조화된 거절을 낸다.**

- **이름과 판과 인자 이름이 한 모듈에 있다** — `tasty-ipc` 의 `output_cursor`. 서버의 인자
  파서가 그 모듈의 이름으로 인자를 읽고, `capability::CAPABILITIES` 가 그 모듈의 `VERSION` 을
  싣고, CLI 가 그 모듈의 이름으로 인자를 싣는다. 셋이 각자 리터럴을 적으면 셋이 갈린다.
  `ipc.stream` 이 `STREAM_PROTO` 를 그대로 싣는 것과 같은 형태다.
- **요구 여부는 요청에서 판정한다.** 메서드가 `surface.read_since_mark` 이고 세 인자 중 하나라도
  실렸으면 그 요청은 계약을 요구한다(`output_cursor::requested_by`). 인자가 하나도 없는 요청은
  예전 그대로 마크를 읽고, 구 서버에서도 뜻이 같으므로 **묻지 않는다** — 물으면 구 서버에 대한
  기존 호출이 한 번의 왕복만큼 느려지고, 막으면 깨진다.
- **CLI 인자**: `tasty read since-mark` 에 `--cursor <N>` · `--stream <TOKEN>` · `--max-bytes <N>`.
  `--cursor` 는 `--stream` 을 요구한다(clap `requires`) — 서버가 `cursor_without_stream` 으로
  거절할 조합을 통신 전에 막는다. **`--stream` 단독은 허용한다.** 서버가 그것을 "마크에서 읽되
  그 사이 터미널이 바뀌지 않았는가를 함께 묻는다" 로 받으므로, CLI 가 짝을 양방향으로 묶으면
  IPC 에 있는 형태 하나가 CLI 에서 사라진다.
- **출력은 응답 그대로다.** `read since-mark` 는 응답 JSON 을 그대로 찍으므로 `next_cursor` ·
  `skipped` · `stream` 이 이미 같은 출력에 있다. 사람용으로 다시 모양을 만들지 않는다 — 이어
  읽기의 주체는 다음 호출에 그 값을 넘길 에이전트다.
- **거절 모양**: 요청을 안 보냈을 때 CLI 는 stderr 에 **JSON 한 줄**을 쓰고 종료 코드 1 로
  끝난다.

      {"error":{"kind":"unsupported_capability","capability":"ipc.output-cursor","required":1,"found":null,"sent":false,"message":"…"}}

  `found` 는 그 이름을 **다른 판으로** 선언했으면 그 판, 아예 없으면 `null` 이다(서버를 올릴지
  요구를 내릴지가 갈린다). `sent:false` 가 이 거절의 요점이다 — 호스트가 답한 실패
  (`Error (<code>): …`)와 달리 **아무것도 일어나지 않았다.** `message` 는 사용자 언어를 탄다.
- **`tasty read since-scan-mark` 는 만들지 않는다.** `surface.read_since_scan_mark` 는 읽으면
  커서가 전진하는 스캐너 전용 커서이고, CLI 동사를 열면 사용자 한 줄이 번들 claude plugin 의
  에러 감시에서 바이트를 조용히 가져간다. 그 결정은
  [ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md) 이 이미 했고, 에이전트가 여럿이
  같은 출력을 각자 읽는 기능은 이 ADR 의 위치 인자가 준다 — 원칙 2 가 요구하는 것은 그 기능의
  CLI 면이지 스캐너 커서의 CLI 면이 아니다.

## Consequences

- **얻은 것**: 위치를 든 이어 읽기가 CLI 로 된다. 에이전트가 셋이어도 각자 `next_cursor` 를
  들고 서로의 창을 안 민다.
- **얻은 것**: 구 서버에서 위치가 **조용히 무시되는** 갈래가 없어졌다. CLI 쪽에서는 요청이
  나가기 전에 멈추고, 그 거절이 "보냈는데 실패했다" 와 값으로 갈린다.
- **잃은 것**: 위치 인자를 실은 호출은 연결마다 `system.info` 왕복이 하나 는다. 인자 없는
  호출은 늘지 않는다.
- **잃은 것 / 한계**: 이 이름은 **CLI 가 묻는 것**까지다. 같은 인자를 직접 싣는 다른 client
  (plugin · 외부 스크립트)는 스스로 물어야 하고, 안 물으면 구 서버에서 예전 결함을 그대로
  밟는다. plugin wire 쪽 선언 자리는 이 결정 밖이다.
- **운영 비용**: 이 계약의 인자를 더하면 판이 아니라 **새 이름**이다(구 client 는 모르는 인자를
  안 싣고 구 서버는 버리므로, 판을 올리면 기존 인자까지 막힌다). 인자의 **뜻**이 좁아질 때만
  `output_cursor::VERSION` 을 올린다.

## Alternatives Considered

- **A: 이름 없이 응답의 `next_cursor` 유무로 사후 판정한다** — 안 골랐다. 판정이 나는 시점에
  이미 공유 마크를 읽었다. 읽기라 부수효과는 없지만, 받은 `text` 는 호출자가 물은 구간이
  아니고 그것을 버리라고 말할 자리가 호출자마다 따로 생긴다.
- **B: `ipc.method-since` 로 메서드가 있는지만 본다** — 안 골랐다. 메서드는 인자보다 먼저
  있었다. 메서드가 있다는 사실은 인자를 읽는다는 사실을 말하지 않는다.
- **C: 스캐너 커서에 이름을 준다(`ipc.scan-cursor`)** — 안 골랐다. 새 계약은 소비자 보유
  위치이고 스캐너 커서는 그 전부터 있던 별개 메서드다. 이름 없는 서버에서 그 메서드는 "알 수
  없는 메서드" 로 이미 거절된다.
- **D: 거절에 별도 종료 코드를 준다** — 안 골랐다. 종료 코드 1 은 이 CLI 에서 "요청이 실패했다"
  의 모든 갈래가 쓰는 값이고, 셸에서 `|| …` 로 받는 호출자는 그 뜻으로 읽는다. "아무것도 안
  보냈다" 는 그보다 좁은 사실이라 JSON 의 `sent` 로 싣는다.
- **E: `--stream` 도 `--cursor` 를 요구하게 짝을 양방향으로 묶는다** — 안 골랐다. 위 Decision
  의 이유(IPC 에 있는 형태 하나가 CLI 에서 사라진다).

## Reconsideration Triggers

**채널이 붙는 것**

- `output_cursor::PARAMS` 와 서버 인자 파서가 읽는 이름이 갈렸을 때. 좌변:
  `src/adapters/ipc/handler/surface/mark.rs` 의 시험
  `the_declared_cursor_contract_names_the_arguments_this_parser_reads`.
- CLI 가 요구하는 이름이 선언 목록에서 빠졌을 때. 좌변: `tasty-ipc` `capability.rs` 의 시험
  `every_name_a_client_requires_is_declared_at_the_version_it_requires`.
- `surface.read_since_scan_mark` 에 CLI 진입점이 생겼을 때 — 이 ADR 과 ADR-0307 의 전제가 함께
  무너진다. 좌변: `tests/cli_method_table_parity.rs` 의 면제 표.

**원리적으로 안 붙는 것**

- plugin 이 위치 인자를 직접 싣기 시작했을 때. 그때는 plugin wire 쪽에도 같은 물음의 자리가
  필요하다. 재는 법: `git grep -n 'read_since_mark' crates/tasty-plugin-*` 의 호출 자리에
  `cursor` 가 실리는지 본다.

## References

- [ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md) — 이
  계약의 본문
- [ADR-0312](0312-the-server-declares-what-it-can-negotiate-not-what-version-it-is.md) — 선언 자리
- [ADR-0307](0307-the-output-scanner-reads-its-own-cursor.md) — 스캐너 커서에 CLI 동사가 없는 이유
- [api-conventions](../dev-guide/api-conventions.md) — 호환 협상
- 코드 근거(결정이 실현된 현재 위치): `tasty-ipc` 의 `output_cursor` 모듈 ·
  `capability::CAPABILITIES` · `client::IpcConnection::require_capability`, 본체의
  `OutputReadParams::parse`, `tasty-cli` 의 `contract` 모듈
