# ADR-0517: 에이전트 primitive 의 이름은 memory 키가 되기 전에 호출자 값 기준으로 판정한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: agent, ipc, error-code, memory, key, semaphore, barrier, rate-limit, task
- **Group**: agent-integration

## Context

`tasty-agent` 의 semaphore · barrier · rate-limit · task store 는 호출자가 준 이름(또는 id)을
접두사 뒤에 그대로 붙여 memory 키를 만든다(`tasty.agent.semaphore.<name>` 등). memory 키 규칙
(`tasty_memory::validate_key` — 소문자 `a-z`·`0-9`·`.`·`_`·`-`, 256 바이트 이하)을 어기는 이름은
memory 층에서 `MemoryError::InvalidKey` 로 떨어지고, `AgentError::Memory` 를 거쳐 IPC 에서
`-32603`(internal)으로 나갔다. 실측: `agent.semaphore_create {name:"v6S"}` →
`-32603 memory: invalid key: invalid char at 24: 'S'`. 입력 오류가 내부 오류 코드로 나가고,
위치 24 는 접두사를 붙인 내부 키의 좌표라 호출자는 자기 값의 어디가 틀렸는지 알 수 없다.

lease 는 이미 다른 길을 택했다 — `resource` 를 `encode_key_component` 로 허용 문자만 쓰는 토큰으로
바꿔 키에 넣으므로 어떤 문자열이든 받는다.

## Decision

네 store 의 키 생성 함수가 키를 만들기 전에 값을 판정하고, 어기면 `AgentError::InvalidArgument`
(IPC `-32602`)로 돌려준다. 메시지는 **호출자가 준 값 기준 문자 좌표와 그 문자 자체**, 허용 문자 ·
그 종류에서 남는 바이트 상한(256 − 접두사 길이)을 싣는다. 문자 단위로 다시 재는 것은
`validate_key` 가 바이트를 세고 첫 바이트를 문자로 찍기 때문이다 — 그대로 옮기면 `aé` 가
`invalid char at 1: 'Ã'` 로 나가 호출자가 자기 이름에서 그 글자를 못 찾는다(위반 앞의 문자는 전부
ASCII 라 좌표 수는 같고, 틀리는 것은 찍히는 문자다). 판정과 허용 문자 문구는 `tasty_memory` 의 것
(`validate_key` · `KEY_ALLOWED_CHARS` · `MAX_KEY_LEN`)을 그대로 쓰고 집합을 다시 적지 않는다 — 한
문자의 허용 여부도 그 문자 하나를 `validate_key` 에 넘겨 묻는다.
판정은 생성뿐 아니라 그 값으로 키를 만드는 모든 경로(get · delete · acquire 등)에 같다. 빈 값은
종전대로 통과시킨다(접두사만으로 유효한 키가 된다).

## Consequences

- **얻은 것**: 이름 규칙 위반이 입력 오류 코드와 호출자가 읽을 수 있는 메시지로 나간다.
  규칙을 지키는 이름의 동작과 저장 키 형식은 그대로다 — 이미 저장된 레코드를 옮길 일이 없다.
  규칙을 어긴 task id 로 조회(`task_get` 등)해도 같다 — `MemoryStore::get` 도 키를 검증하므로
  전에는 `-32603` 이었고, 이제 `-32602` 다(`-32004` not found 가 나간 적은 없다).
- **잃은 것**: 이름에 대문자 · 공백 · `/` 등을 쓸 수 없다는 제한은 그대로 남는다(lease 와 비대칭).
- **운영 비용 / 유지 부담**: 새 primitive 가 호출자 값을 키에 넣을 때는 같은 헬퍼
  (`tasty_agent` 의 `component_key`)를 거쳐야 한다. 거치지 않으면 다시 `-32603` 이 나간다.

## Alternatives Considered

- **lease 처럼 인코딩해 어떤 이름이든 받는다** — 저장 키 형식이 바뀐다. `_` 가 `__` 로 이중화되므로
  이미 `_` 가 든 이름으로 저장된 semaphore · barrier · task 가 새 키로 안 찾아진다. 마이그레이션이
  필요하고, 규칙을 지키던 기존 호출의 동작까지 건드린다. 호환 보존이 가장 적은 쪽이라 고르지 않았다.
- **IPC 층에서 `MemoryError::InvalidKey` 를 `-32602` 로 옮겨 싣는다** — 코드는 맞아지지만 메시지의
  좌표는 여전히 내부 키 기준이다. 그리고 `InvalidKey` 는 호출자 입력이 아닌 원인(호스트가 만든 키의
  결함)에서도 나므로 그것까지 입력 오류로 보이게 된다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `tasty-agent` 에 호출자 값을 키에 넣는 새 store 가 생겼는데 `component_key` 를 안 거친다.
  재는 법: 그 크레이트에서 `_KEY_PREFIX}{` 형태의 `format!` 을 찾는다.
- memory 키 규칙(`validate_key`)이 넓어지거나 좁아진다 — 메시지 문구는 따라가지만 이 ADR 의
  "잃은 것" 서술이 낡는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 호출자들이 대문자 · 공백이 든 이름을 계속 요구한다(이슈 · 에이전트 로그에서 이 거절이 반복된다).
  그때는 인코딩 + 마이그레이션을 다시 저울질한다.

## References

- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-agent/src/lib.rs` 의 `component_key`,
  `crates/tasty-memory/src/scope.rs` 의 `validate_key` · `KEY_ALLOWED_CHARS` · `MAX_KEY_LEN`,
  IPC 매핑 `src/adapters/ipc/handler/agent.rs` 의 `agent_err_to_response`.
- 기능 문서: [features/agent-collaboration](../features/agent-collaboration/index.md) "에러 코드".
- 키 규칙: [design/systems/memory](../design/systems/memory.md).
