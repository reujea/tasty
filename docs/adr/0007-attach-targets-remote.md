# ADR-0007: attach 는 원격을 대상으로 한다 (로컬 self-attach 는 debug 격리)

- **Status**: Accepted
- **Date**: 2026-06-17
- **Tags**: attach, remote, debug-isolation, cli, user-agent-separation, security
- **Group**: remote-attach

## Context

attach 의 **server 는 transport 를 모르고 항상 `127.0.0.1` 로만 client 를 받는다** — 로컬에서 붙든 SSH 터널 너머에서 붙든 서버 입장엔 전부 loopback 이다(상세 [`0004-ipc-transport-tcp`](0004-ipc-transport-tcp.md)). 따라서 "로컬 attach" 와 "원격 attach" 의 차이는 *서버* 가 아니라 **client 측 진입점**에만 있다 — 로컬=포트파일 직결, 원격=`ssh -L` 터널 후 터널 localport 직결.

이 구조에서 "같은 머신 self-attach"(자기 인스턴스의 surface 를 자기가 mirror)는 무엇인가를 정해야 했다. self-attach 는 사용자가 GUI 에서 하는 mirror 조작을 *연결을 통해 자동 재현* 하는 성격이다 — 즉 **사용자 행동의 재현**이지 에이전트가 자기 작업에 필요한 동작이 아니다([identity](../identity.md) §2.1 사용자 행동 ↔ 에이전트 행동 분리). 반면 원격 attach 는 *다른 호스트의 surface/workspace 를 mirror* 하는 에이전트의 정당한 행동이다.

무엇보다 같은 머신에서 자기 화면을 자기가 mirror 하는 것은 **실질적으로 쓸모가 없다** — 이미 GUI 로 그 화면을 보고 있다. 따라서 이를 *명시적으로* 수행하려는 호출은 거의 **실수**다.

또한 "단발 화면 읽기"를 attach 로 해결할 것인지도 함께 정해야 했다 — attach 는 점유 세션을 여는 무거운 경로다.

## Decision

**attach 의 release CLI 표면은 원격(다른 호스트/인스턴스)만 대상으로 한다.**

- 원격 attach 는 release 에 노출한다 — `tasty remote attach …` / `tasty remote check …`.
- **로컬 loopback self-attach 는 release 표면에서 제거하고 debug 빌드로 격리한다** — `tasty attach` → `tasty debug attach`. debug 에 남기는 이유는 **e2e 테스트 등 attach 파이프라인 검증**을 위해서다(원칙 1 ②, [`dev-guide/debug-ipc`](../dev-guide/debug-ipc.md)).
- **"로컬 attach 제거" 는 서버를 바꾼 게 아니라 client 의 로컬 진입점만 제거한 것이다.** 서버의 attach 수신 경로(`attach.*` IPC, `run_attach_*` 세션 머신)는 로컬/원격 공용으로 그대로 보존된다 — 원격 attach 도 결국 loopback 이라 같은 경로를 탄다.
- **막는 강도 — hard-block 이 아니라 "제공하지 않음"이다.** 로컬 self-attach 는 *쓸모없는 행위*라 release 에 **진입점을 두지 않을 뿐**이고, 명시적으로 수행하려는 시도를 실수로 간주해 막는 수준이다. 사용자가 우회(예: `127.0.0.1:PORT` loopback 직결로 원격 경로 타기)로 굳이 self-attach 하는 것은 **따로 막지 않는다** — 서버 경로가 공용으로 보존되므로 가능하며, 이는 의도된 비강제다.
- **단발 화면 읽기는 attach 가 아니다** — 정식 경로는 `tasty read screen` / `tasty read since-mark`. attach 의 `--dump-after` 는 mirror 파이프라인 *검증용*이지 일반 스크래핑 용도가 아니다.

`remote` / `debug attach` 는 IPC 네임스페이스가 아니라 `attach.*`(+`system.info`) 위에서 원격성·debug 격리만 분기하는 **CLI 디스패치 계층**이다.

## Consequences

- **얻은 것**: 사용자 행동 재현(self mirror)이 release 표면에서 사라져 [identity](../identity.md) §2.1 "사용자 행동은 debug 격리" 와 일관된다. 에이전트의 정당한 행동(원격 mirror)만 release 에 남아 표면이 정직해진다. 서버 경로를 로컬/원격 공용으로 단일 유지해 분기 비용이 없다.
- **잃은 것**: 같은 머신 self-attach 를 release 빌드에서 직접 쓸 수 없다 — debug 빌드(`tasty debug attach`)가 필요하다. (동일 머신 다중 *인스턴스* attach 는 `127.0.0.1:PORT` loopback 직결로 원격 경로를 통해 여전히 가능.)
- **유지 부담**: "attach 는 원격 대상" 이라는 표면 규칙을 CLI/문서에서 지속 유지해야 한다 — 로컬 진입점을 release 에 되살리지 않는다.

## Alternatives Considered

- **release 에 로컬 attach 유지** (`tasty attach`): 기각. self-attach 는 사용자 mirror 조작의 자동 재현이라 사용자 행동 ↔ 에이전트 행동 분리(원칙 1)를 깬다 — 에이전트 표면(IPC/CLI)이 사용자 조작을 재현하면 안 된다.
- **서버에서 로컬 수신 경로 자체를 제거**: 기각. 원격 attach 도 서버 입장엔 loopback 이라 같은 수신 경로를 탄다 — 로컬만 골라 제거하는 것은 불가능하고 무의미하다. 제거 대상은 *클라이언트 진입점* 뿐이다.
- **단발 읽기를 attach 로 제공**: 기각. 화면 스크래핑은 점유 세션 없이 `read screen` / `read since-mark` 로 충분하다. attach 를 읽기 용도로 노출하면 무거운 경로를 일반 용도로 오용하게 된다.

## Reconsideration Triggers

- 사용자 행동 ↔ 에이전트 행동 분리(원칙 1)가 바뀌거나, 로컬 self-attach 가 debug 검증 외 *release 사용자에게 정당한* 용례가 생기면 재검토한다.

## References

- [`identity.md`](../identity.md) §2.1 — 사용자 행동 ↔ 에이전트 행동 분리, 원칙 1 ②(debug 격리)
- [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — "서버/클라이언트 계층", "로컬 attach 제거의 정확한 의미"
- [`features/remote-attach/`](../features/remote-attach/index.md) — 원격 attach 동작·CLI 표면
- [`dev-guide/debug-ipc`](../dev-guide/debug-ipc.md) — debug 격리 정책
- [`0004-ipc-transport-tcp.md`](0004-ipc-transport-tcp.md) — attach 가 깔고 앉은 loopback trust boundary (SSH 위임 보안)
- 코드: `crates/tasty-cli/src/commands/remote.rs`(clap 선언) · `crates/tasty-cli/src/local/attach.rs`(`run_attach_*` 공용) · `crates/tasty-cli/src/local/debug/`(로컬 격리)
</content>
