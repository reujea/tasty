# ADR-0004: IPC transport = 127.0.0.1 loopback TCP (동적 포트)

- **Status**: Accepted
- **Date**: 2026-06-16
- **Tags**: ipc, transport, tcp, loopback, security, trust-boundary, cross-platform
- **Group**: ipc-transport

> IPC=TCP 채택은 아주 초기 결정이라 원본 커밋을 특정하지 못했다. 본 ADR 은 현존하는 최선 출처 (`docs/design/systems/memory.md` "IPC transport 의 trust boundary", `crates/tasty-ipc/`, `src/core/attach.rs`) 에서 결정을 응축한 것이다.

## Context

Tasty 의 호스트 프로세스는 CLI 서브커맨드와 plugin 으로부터 IPC 요청을 받는다. 이 채널은 Windows / macOS / Linux 세 OS 에서 동일하게 동작해야 한다 (원칙 4 크로스 플랫폼).

OS 별 로컬 IPC 의 표준 수단은 갈린다 — Unix 계열은 unix domain socket, Windows 는 named pipe. 각각 권한 모델 (socket file mode, pipe ACL) 도 다르다. 세 OS 를 한 코드 경로로 다루려면 플랫폼 분기를 어디까지 감수할지가 문제였다.

또한 IPC 는 "누가 `_host` 권한으로 붙을 수 있나" 라는 trust boundary 를 직접 규정한다. Tasty 의 타깃은 **단일 사용자 데스크탑/노트북** 이고, 같은 OS user 의 프로세스는 어차피 `~/.tasty/memory.db` 와 OS keyring 에 직접 접근할 수 있다 — 즉 같은 user 안에서는 IPC 에 별도 인증 레이어를 둬도 보호 가치가 없다.

## Decision

IPC transport 로 **`127.0.0.1` loopback TCP** 를 쓴다. 서버는 동적 포트 (`TcpListener::bind("127.0.0.1:0")`, OS 할당) 로 listen 하고, 할당된 포트를 **포트 파일** (`~/.tasty/tasty.port`, debug 빌드는 `tasty-debug.port`) 에 기록한다. CLI/plugin 클라이언트는 이 포트 파일을 읽어 호스트를 디스커버리한다. wire 형식은 JSON-RPC 2.0.

**자체 인증/토큰 레이어를 두지 않는다.** 연결 경계 보안은 OS user 격리와 SSH 에 위임한다 (원격 접속은 SSH 터널 너머의 loopback 으로만 도달). 소켓에 도달한 caller (별도 인스턴스의 `Local`, 인증된 agent) 는 모두 `_host` 권한으로 동작한다. 이는 `src/core/attach.rs` 의 **decision 5** ("자체 인증/토큰 레이어 없음 — SSH + 127.0.0.1 loopback 위임") 와 동일한 신뢰 모델이다.

## Consequences

- **얻은 것**: 세 OS 동일 코드 경로 (출처: 세 OS **컴파일** 채널 — `.github/workflows/crossplatform-check.yml` 의 `check-macos`·`check-windows`·`check-headless`. **런타임 동작은 그 채널 밖이다** — integration·e2e 는 Linux 뿐이다) — 분기가 소스에 없다는 것은 소스로 참이고, 세 조합이 컴파일되는 것은 그 채널이 본다 — named pipe / unix socket 의 플랫폼별 권한 분기를 회피한다. 동적 포트라 고정 포트 충돌이 구조적으로 없다 (OS 가 빈 포트를 할당). 포트 파일 디스커버리로 클라이언트가 서버 인스턴스를 찾는다.
- **잃은 것**: 같은 OS user 의 임의 프로세스가 포트만 알면 `Local` caller 로 붙어 `_host` 권한을 얻는다 (`memory.md` "위협 모델"). 단일 사용자 가정 하에서는 OS user 격리와 trust boundary 가 일치하므로 수용하지만, 공유 머신/멀티테넌트에서는 깨진다.
- **운영 비용 / 유지 부담**: 포트 파일 생명주기 관리 (서버 Drop 시 삭제). loopback TCP 는 unix socket file mode 0600 / named pipe ACL 같은 user-level owner 분리를 제공하지 못한다 — 이 격차는 의식적으로 미구현 상태로 둔다 (아래 Reconsideration 참조).

## Alternatives Considered

- **Unix domain socket (file mode 0600) + named pipe ACL (Windows)**: user-level owner 분리가 가능해 공유 머신에서도 다른 OS user 의 접근을 막을 수 있다. 그러나 플랫폼별 분기를 들여야 하고, 현재 단일 사용자 데스크탑 타깃에서는 불필요한 복잡도다. tmux 류 attach/detach 모델을 시도하다 포기한 상태라 우선순위가 낮아 보류 (`memory.md` "IPC transport 의 trust boundary"). 검토는 됐으나 채택하지 않음.
- **고정 포트**: 동적 포트 대신 고정 포트를 쓰면 포트 파일 디스커버리가 불필요해진다. 그러나 포트 충돌 가능성이 생기고, 동적 포트 + 포트 파일이 이미 충돌 없는 디스커버리를 제공한다. (동적 포트를 *선택한 명시적 근거* 는 코드/문서에 기록되어 있지 않다 — bind 가 `:0` 인 사실만 확인됨. 충돌 회피는 그 구조적 귀결이다.)

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다 (`memory.md` "IPC transport 의 trust boundary" 의 핀과 동일).

- **공유 머신에서 다른 OS user 가 tasty 인스턴스에 접근** (예: 원격 SSH 서버에 user A 가 tasty 데몬을 띄워두고 user B 가 같은 머신을 사용). loopback TCP 는 user 격리를 하지 않으므로 user B 가 user A 의 메모리/secret 을 읽을 수 있다.
- **multi-tenant 데몬 모델** (한 tasty 인스턴스를 여러 사용자가 공유).

이 시나리오가 실제로 들어오면 transport 를 Unix socket + 0600 (Windows: named pipe ACL) 로 바꾸고, 필요 시 user-level owner 분리까지 도입한다.

## References

- [`design/systems/memory.md`](../design/systems/memory.md) — "보안·신뢰 모델" 위협 모델 (IPC transport trust boundary)
- [`index.md`](../index.md) (docs 루트) — 기술 스택 표의 IPC 항목 (TCP 127.0.0.1 동적 포트, `~/.tasty/tasty.port`)
- 코드: `crates/tasty-ipc/src/{port_file,server,method_meta}.rs`, `src/adapters/production/tcp_ipc_server.rs` (`bind("127.0.0.1:0")`)
- `src/core/attach.rs` decision 5 / `crates/tasty-ipc/src/method_meta.rs:256` — attach 보안의 SSH + loopback 위임 (동일 신뢰 모델)
- 관련: [`0005-memory-secret-not-a-vault.md`](0005-memory-secret-not-a-vault.md) — 같은 trust boundary 위에서 secret 저장 보호 범위를 정한 결정
