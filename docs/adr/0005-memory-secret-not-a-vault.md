# ADR-0005: memory secret 영역은 "안전 보관소" 가 아니다

- **Status**: Accepted
- **Date**: 2026-06-16
- **Tags**: memory, secret, security, encryption, plugin, trust-boundary
- **Group**: permission-security

> 초기 설계는 secret 을 AES-256-GCM + OS keyring 으로 암호화하려 했다 (도입 커밋 `e562fd53` → 철회 `34c01afe`). 본 ADR 은 "암호화하지 않는다" 는 현재 결정을 응축한 것이다. 결정 배경의 상세 산문은 `docs/design/systems/memory.md` "왜 암호화를 하지 않는가" 에 있다.

## Context

Tasty 의 메모리 시스템 (`~/.tasty/memory.db`) 은 `memory.secret.*` 영역을 둔다. "secret" 이라는 이름은 *데이터-앳-레스트 안전 보관소* 라는 오해를 준다.

초기 설계는 이 영역을 **AES-256-GCM + OS keyring** 으로 암호화하려 했다. 목적은 *plugin process 가 IPC 를 우회해 sqlite 파일을 직접 열어 다른 plugin 의 secret 을 빼가는 것* 을 막는 것이었다. 그러나 이 모델은 현재 plugin 실행 모델과 trust boundary 가 맞지 않는다 — plugin 은 OS-level sandbox 없이 호스트와 같은 user 권한의 일반 프로세스로 실행되기 때문이다 (`Command::new(entry_path).spawn()`). 그래서 암호화 설계는 철회됐다 (커밋 `34c01afe`).

## Decision

**secret value 를 평문 BLOB 으로 저장한다.** 데이터-앳-레스트 암호화 (AES-GCM/keyring) 를 하지 않는다. secret 영역의 격리 약속은 **"plugin 간 IPC 격리" 한 가지로 좁힌다** — plugin A 는 IPC 표면에서 plugin B 의 secret 의 *존재 자체* 를 볼 수 없다 (owner 가 PK 일부).

디스크 파일 직접 노출 / 디스크 도난 / 백업·클라우드 sync 같은 시나리오는 **명시적으로 책임지지 않는다.** 정말 민감한 데이터 (master password, OAuth refresh token, 결제 key 등) 는 plugin 이 secret 영역에 두지 *말고* 자체적으로 OS keyring 을 호출하거나 외부 보관소에 두도록 권고한다 (`docs/dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택`).

이 결정은 코드·테스트·문서로 실현되어 있다: `crates/tasty-memory/src/lib.rs:17` (모듈 주석), `migrations.rs:46` (평문 BLOB 스키마), 회귀 테스트 `tests.rs:525 secret_at_rest_is_plaintext()`.

## Consequences

- **얻은 것**: *false sense of security* 제거 — 지킬 수 없는 약속 ("secret 은 안전하다") 을 하지 않는다. 보호 범위를 정직하게 좁혀 plugin 개발자/사용자에게 잘못된 신호를 주지 않는다. 향후 plugin sandbox (sandbox-exec / landlock / AppContainer) 가 도입되면 **추가 코드 없이 자동으로 강해진다** — sandbox 가 디스크/keyring 접근 자체를 막으므로 AES-GCM 재도입이 불필요하다.
- **잃은 것**: secret 의 보호 수준이 "plugin 간 IPC 격리" 한 가지로 좁아진다. 보호 수준 표 (`plugin-development.md` 의 민감 데이터 절) 대로, DB 파일을 직접 여는 행위자 / 백업 sync / 디바이스 도난 시 secret 은 평문으로 노출된다. 사용자/host 는 모든 secret 을 자유롭게 조회한다.
- **운영 비용 / 유지 부담**: plugin 개발자에게 "secret 영역은 안전 보관소가 아니다 + 진짜 민감 데이터는 keyring 권고" 를 문서로 지속 안내해야 한다 (`plugin-development.md` 의 민감 데이터 절).

## Alternatives Considered

- **AES-256-GCM + OS keyring (초기 설계)**: 세 가지 이유로 기각 (`memory.md` "왜 암호화를 하지 않는가"). ① plugin sandbox 부재 — 같은 user 프로세스가 sqlite 파일을 직접 열 수 있다. ② keyring 도 우회 가능 — 같은 user 권한이면 plugin 이 OS keyring API 를 직접 호출해 master key 를 빼내 복호할 수 있어 보호가 결정적이지 않다. ③ 환경 흔들림 — keyring 가용성이 환경 (Linux 헤드리스/WSL/CI) 에 따라 변해, 평문 폴백을 옵트인으로 두면 row 별로 암호화/평문이 섞여 데이터 손상 위험이 생긴다.
- **평문 폴백 옵트인**: 환경 전환 시 row 별 암호화/평문 혼재 → 데이터 손상 위험. 기각.

## Reconsideration Triggers

다음이 충족되면 본 ADR 을 재검토한다.

- **plugin sandbox 도입** (sandbox-exec / landlock / AppContainer 등). 그 시점에 plugin 이 `~/.tasty/memory.db` 와 OS keyring 에 직접 접근할 수 없게 되어, secret 영역의 IPC 격리만으로 진짜 격리가 완성된다 — "이제 secret 도 안전한가" 를 재평가한다. (단 AES-GCM 재도입은 불필요: sandbox 가 디스크 접근 자체를 차단.)

transport 차원의 노출 (같은 머신의 다른 OS user 가 IPC 로 secret 을 읽는 경우) 은 본 ADR 이 아니라 [`0004-ipc-transport-tcp.md`](0004-ipc-transport-tcp.md) 의 trust boundary 소관이다.

## References

- [`design/systems/memory.md`](../design/systems/memory.md) — "보안·신뢰 모델" 위협 모델 + regular/secret 가시성 모델
- [`dev-guide/plugin-development.md`](../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택) — plugin 개발자용 가이드 + 보호 수준 표 + keyring 권고
- 코드: `crates/tasty-memory/src/lib.rs:17`, `migrations.rs:46`, 회귀 테스트 `tests.rs:525 secret_at_rest_is_plaintext()`
- 커밋: AES-GCM 도입 `e562fd53` → 철회 `34c01afe`
- 관련: [`0004-ipc-transport-tcp.md`](0004-ipc-transport-tcp.md) — secret 이 깔고 앉은 IPC trust boundary
