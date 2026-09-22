# 독립 검증 — 개발도 Agent 가 스스로 확인할 수 있어야 한다

> dev-guide 의 **가장 핵심 원칙**. Tasty 정체성인 *동시성*([identity.md](../identity.md))이 **개발 환경 자체에 재귀적으로 적용된 것** 이다.

## 원칙

**Tasty 의 모든 기능은, 그것을 개발하는 AI Agent 가 자기 수정을 독립적으로 띄워 검증할 수 있도록 만들어야 한다.**

Tasty 를 개발하는 환경이 곧 Tasty 다 (dogfooding). 보통 사용자·다른 Agent 는 **release** 빌드를 띄워 작업 중이다. 그 위에서 Agent 가 자기가 고친 것을 확인하려면 **debug** 빌드를 동시에 띄워야 하는데, 둘이 같은 자원(포트·상태 파일)을 공유하면 서로 간섭한다.

그래서 Tasty 는 **debug 빌드와 release 빌드의 환경을 격리** 한다 (0% 충돌은 불가능하지만 최대한). 덕분에 Agent 는 *자신이 release tasty 안에서 동작 중이어도*, 자기 debug 빌드를 따로 띄워 release(= 사용자·다른 작업)와 충돌 없이 검증할 수 있다.

> **Agent 는 이것을 반드시 인지한다**: 내가 tasty 안에서 돌고 있어도, 내 수정은 *별도 debug 인스턴스* 로 띄워 검증한다. 돌아가는 release 를 건드리지 않는다.

## debug ↔ release 격리 (현재 구현)

> 이 표가 격리 경로의 **산문 SoT** 다. 다른 문서는 이 표를 풀-복제하지 말고 "debug 는 별도 루트(`~/.tasty-debug/`)로 격리됨" + 이 문서 링크로 참조한다.

**데이터 루트 자체가 갈린다** — debug 빌드는 `~/.tasty-debug/`, release 는 `~/.tasty/` 를 쓴다. 같은 디렉토리 안의 파일명 접미사가 아니라 **루트 디렉토리 분리**라, 포트·layout·scrollback·state.db·memory.db·plugins 가 통째로 별도다:

| 자원 | release (`~/.tasty/`) | debug (`~/.tasty-debug/`) |
|------|---------|-------|
| IPC 포트 파일 | `~/.tasty/tasty.port` | `~/.tasty-debug/tasty.port` |
| scrollback | `~/.tasty/scrollback/` | `~/.tasty-debug/scrollback/` |
| layout | `~/.tasty/layouts/NN.json` | `~/.tasty-debug/layouts/NN.json` |

- 파일명은 양쪽 모두 동일(`tasty.port` 등) — 구분은 **루트** 가 한다. (`-debug` 파일명 접미사는 쓰지 않는다.)
- `target/debug/tasty` (debug 바이너리)는 `~/.tasty-debug/` 루트를 읽으므로 CLI 조작이 **debug 인스턴스에만** 간다. 사용자의 release 인스턴스는 건드리지 않는다.
- **`TASTY_HOME` env override**: 비어있지 않으면 그 경로를 루트로 강제한다(테스트/샌드박스/다중 인스턴스용) — debug/release 자동 분기보다 우선.
- 구현(SoT): `crates/tasty-utils/src/path.rs` (`tasty_home()` — `TASTY_HOME` 우선, 없으면 `cfg!(debug_assertions)`→`.tasty-debug`), `crates/tasty-ipc/src/port_file.rs`, `src/store/scrollback.rs`, `src/core/layout_persistence.rs`.

## 새 기능 추가 시 적용

- 영속 상태(파일/소켓/포트 등)를 새로 추가하면 **debug/release 분리 패턴을 따른다** (debug 는 `-debug` suffix). 안 그러면 debug 검증이 release 데이터를 오염시킨다.
- 동작은 IPC/CLI 로 트리거 가능하게 만든다 (headless 동작-우선) — 그래야 Agent 가 GUI 없이 검증한다. → [identity.md](../identity.md) §2.2.

## 한계 / 주의

- 격리는 **tasty 의 상태**(루트 디렉토리)만 가른다. OS 열기(브라우저 · 파일 관리자)는 사용자 데스크톱의 이미 떠 있는 브라우저에 닿는다 — 검증 인스턴스는 `TASTY_DEBUG_OS_OPEN_LOG` 와 가짜 브라우저로 띄운다([self-verification.md](self-verification.md), [debug-ipc.md](debug-ipc.md)).
- 격리는 **debug ↔ release** 기준이다. 두 debug 인스턴스를 동시에 띄우면 같은 `~/.tasty-debug/` 루트(포트파일 `~/.tasty-debug/tasty.port`)를 공유하므로 충돌한다 — 이때는 `TASTY_HOME` 으로 루트를 분리하거나 별도 checkout/worktree 를 운용한다.

## 관련

- [`self-verification.md`](self-verification.md) — 실제 검증 절차 (cargo run & + CLI 시나리오)
- [`debug-ipc.md`](debug-ipc.md) — debug 전용 IPC (사용자 입력 재현, release 미노출)
- [`e2e-tests.md`](e2e-tests.md) — 테스트 환경 격리 정책
- [identity.md](../identity.md) — 동시성 정체성 (이 원칙의 뿌리)
