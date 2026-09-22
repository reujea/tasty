# ADR-0512: CLI 는 IPC 오류의 `error.data` 를 stderr 둘째 줄에 원형 그대로 싣는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: cli, ipc, error-message, parity, wire-format, stderr
- **Group**: cli-logging

## Context

호스트의 JSON-RPC 오류 응답은 `code` · `message` 외에 `data` 를 실을 수 있고, 실제로
호출자가 분기할 분류를 거기 싣는다 — 메모리 저장 실패의 `storage_failure`
(`src/adapters/ipc/handler/memory.rs`), 출력 읽기 거절의 `reason`([ADR-0341](0341-a-terminal-output-read-answers-from-a-position-the-consumer-holds.md)),
권한 거부의 `approval_id` · `permission` · `method`(`docs/features/capability-elevation/index.md`).

이 결정 전에는 CLI 가 그 값을 볼 수 없었다. 클라이언트 연결(`tasty_ipc::client::IpcConnection::send`)이
오류 응답을 `JsonRpcCallError { code, message }` 로 옮기면서 `data` 를 버렸고, CLI 는
`Error (<code>): <message>` 한 줄만 찍었다. IPC 로는 얻는 실패 분류를 CLI 로는 얻지 못하는
상태 — [정체성 원칙](../identity.md) 2(IPC + CLI 양면)의 결손이다.

그 한 줄은 이미 사용자 가이드가 약속한 형식이다(`site/content/agents/cli.md` 의 `Error (-32065): …`
등). 첫 줄을 파싱하는 스크립트가 있다고 봐야 한다.

CLI 에는 IPC 명령 전반에 걸친 `--json` 플래그가 없다. 성공 응답은 이미 JSON 으로 stdout 에
나가고(`format.rs`), 오류만 사람이 읽는 한 줄로 stderr 에 나간다.

## Decision

**첫 줄은 그대로 두고, `error.data` 가 있으면(`null` 이 아니면) stderr 둘째 줄에
`data: <compact JSON>` 으로 원형 그대로 싣는다.** 사람은 둘째 줄을 읽고, 스크립트는
`data: ` 접두를 떼어 JSON 으로 파싱한다 — 한 형식이 두 독자를 함께 맡는다.

- `JsonRpcCallError` 에 `data: Option<serde_json::Value>` 를 더한다. `Display` 는 바꾸지 않는다
  (hook 실패 기록의 `reason=` 등 그 문자열을 쓰는 다른 자리를 움직이지 않는다).
- 단발 RPC · plugin 동적 명령 · `auto_wait` · 폴링 · 계약 확인 실패는 모두
  `crates/tasty-cli/src/rpc_error.rs` 의 `exit_with` 로 간다.
- **스트리밍 두 명령(`events follow` · `plugin audit-follow`)은 아직 `data` 를 싣지 않는다.**
  두 명령은 호스트 오류를 `main` 까지 올려 std 가 `Error: Error (<code>): <message>` 한 줄로
  찍는다(`crates/tasty-cli/src/events.rs` · `crates/tasty-cli/src/plugin.rs`). `exit_with` 로 옮기면
  첫 줄 접두가 `Error (` 로 바뀌므로, 첫 줄 호환을 지키려고 이 결정에서는 옮기지 않았다.
  (이 불릿은 최초 결정 시점의 서술이다 — 두 명령이 `data` 를 싣게 된 현재 형태는 아래 "보강" 이 정한다.)
- 접두 `data: ` 는 번역하지 않는다 — `Error (` 와 같은 응답 형식의 일부다(`docs/dev-guide/i18n.md`).

### 보강 — 스트리밍 두 명령도 `data` 를 싣는다 (같은 날 후속 트랙, 구현 확정)

최초 결정은 두 스트리밍 명령을 첫 줄 호환 때문에 범위 밖에 두었다. 후속 트랙이 첫 줄을 건드리지
않는 형태를 찾아 그 빈자리를 채웠다 — 결정("첫 줄 불변, `data` 는 둘째 줄") 자체는 그대로다.

- **스트리밍 두 명령은 `exit_with` 로 옮기지 않고 같은 두 줄을 `main` 으로 올린다.** 두 명령은
  호스트 오류를 `main` 까지 올려 std 가 찍으므로 첫 줄이 `Error: Error (<code>): <message>` 다 —
  `exit_with` 로 옮기면 그 접두 `Error: ` 가 빠진다. 그래서 올라가는 값의 문구만 `render` 의 두 줄로
  바꾼다(`rpc_error::with_data_line`, 호출 자리는 `crates/tasty-cli/src/events.rs` ·
  `crates/tasty-cli/src/plugin.rs`). 첫 줄은 그대로이고 둘째 줄 `data: ` 가 같은 모양으로 붙으며,
  `data` 가 없으면 값을 건드리지 않는다.
- **유지 부담이 한 갈래 는다**: 새 오류 출력 자리가 오류를 `main` 까지 올리는 자리면 `exit_with` 가
  아니라 `with_data_line` 을 불러야 한다. 부르지 않으면 그 자리만 `data` 를 다시 버린다(아래
  Consequences 의 "운영 비용" 과 같은 성질).
- 시험: `tests/cli_streaming_error_data.rs` 가 실제 바이너리를 가짜 호스트에 붙여 두 명령의 stderr 를
  잰다(첫 줄 불변 · `data` 줄 · `data` 없음/`null` 이면 한 줄).

## Consequences

- **얻은 것**: IPC 호출자와 CLI 호출자가 같은 실패 분류로 분기한다. `data` 가 없는 오류의
  출력은 한 글자도 안 바뀐다.
- **잃은 것**: stderr 가 한 줄이라고 가정하고 **전체**를 한 문장으로 쓰던 스크립트는 `data` 가
  있는 오류에서 둘째 줄을 받는다. 첫 줄만 읽는 쪽은 영향이 없다.
- **운영 비용 / 유지 부담**: 새 오류 출력 자리는 `exit_with` 를 불러야 한다. 부르지 않으면
  그 자리만 `data` 를 다시 버린다.

## Alternatives Considered

- **첫 줄에 이어 붙인다**(`Error (…): msg {"reason":…}`) — 첫 줄 형식이 바뀌어 기존 파서가
  message 끝을 잘못 자른다.
- **오류 전체를 JSON 한 줄로 낸다** — 계약 거절(`refusal_line`)은 이미 그 모양이지만, 모든
  호스트 오류를 바꾸면 가이드가 약속한 `Error (…)` 형식이 깨진다.
- **새 `--json` 플래그로 갈라 낸다** — IPC 명령 전반에 그런 플래그가 없고, 성공 출력은 이미
  JSON 이라 오류에만 모드를 두는 것은 표면만 늘린다. 둘째 줄 JSON 이 기계 판독을 이미 맡는다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- CLI 에 IPC 명령 전반의 출력 모드 플래그(`--json` 등)가 생긴다. 그때 오류도 그 모드를 따를지 다시 정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- stderr 를 한 줄로 가정한 외부 스크립트가 둘째 줄 때문에 깨졌다는 보고가 온다. 재는 법: 이슈·사용자 보고.

## References

- 코드 근거(결정이 실현된 현재 위치): `tasty_ipc::client::JsonRpcCallError` · `crates/tasty-cli/src/rpc_error.rs` 의 `render` / `exit_with` / `with_data_line`
- 시험: `crates/tasty-cli/src/rpc_error.rs` 의 단위 시험 · `tests/cli_streaming_error_data.rs`(스트리밍 두 명령의 실제 stderr — 첫 줄 불변과 `data` 줄)
- [`docs/dev-guide/cli-structure.md`](../dev-guide/cli-structure.md) "호스트 오류 출력"
- [ADR-0164](0164-hook-failure-locale-invariance-rests-on-fields.md) — 같은 타입에 `code` 를 데이터로 둔 선례
