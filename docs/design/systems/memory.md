# 에이전트 메모리 시스템

`~/.tasty/memory.db` (SQLite WAL 단일 파일)에 저장되는 영속 키-값 스토어. AI 에이전트·plugin 이 작업 도중 누적·검색·공유하는 데이터의 backing store 다. 본 바이너리는 `init_with_config` 로 연 store 를 `Arc<Mutex<dyn MemoryStorage>>` 로 Core 에 주입해 동기 접근한다(`crates/tasty-memory/`). 이 문서는 **가시성·소유권 모델**을 정의한다. 암호화 안 하는 결정의 근거는 [ADR-0005](../../adr/0005-memory-secret-not-a-vault.md), IPC trust boundary 는 [ADR-0004](../../adr/0004-ipc-transport-tcp.md).

## 책임 범위

에이전트·plugin 작업 데이터의 1차 저장소다. 수 KB(토큰)부터 수백 KB~1 MiB(캐시된 JSON, 누적 상태)까지 자연스럽게 다루지만 **만능 저장소는 아니다.** cap 을 넘는 데이터는 **파일로 분리하고 그 경로를 memory entry 로** 저장한다(memory.db 비대 방지, 명시적 lifecycle, OS 도구 호환).

| 책임진다 | 책임지지 않는다 |
|---|---|
| 작업 상태·진행 메타데이터, 토큰·설정, cap 안의 캐시/중간결과, 가벼운 공유 데이터 | 큰 binary asset(이미지/모델/미디어), 영구 로그, 임의 크기 plugin 간 교환(파일 참조/`tasty-shm`) |

## 두 계층: regular / secret

같은 `memory.db` 안이지만 테이블·IPC 네임스페이스·가시성이 다르다.

| 영역 | IPC | 권한 토큰 | 가시성 | 쓰기 |
|------|-----|-----------|--------|------|
| Regular | `memory.*` | `memory.read` / `memory.write` | **모든 plugin 이 모든 entry 읽기** | **owner 본인** 또는 host |
| Secret | `memory.secret.*` | `memory.secret` | **각 plugin 은 자기 영역만** | 자기 영역만 |

둘 다 같은 `Scope`(`global`/`account:<u>`/`window:<id>`/`workspace:<id>`/`surface:<id>`)와 키 규칙(1..=256자 `[a-z0-9._-]+`)을 공유한다. 차이는 **`owner` 차원** 하나다.

`surface:<id>` 의 `<id>` 는 **surface id 공간**(`< 0x8000_0000`)이어야 한다 — 그 이상은 headless PTY id 공간이라 실재하는 surface 가 가질 수 없는 값이고, IPC 가 `invalid_params` 로 거부한다([ADR-0094](../../adr/0094-surface-id-space-bounded-below-pty-base.md)).

## owner — 숨겨진 host 전용 차원

`owner` 는 모든 entry 에 붙지만 **plugin 에게는 보이지 않는다** — IPC schema 에 `owner` 인자가 없다. host 가 caller 에서 자동 도출한다:

```
CallerContext::Plugin(id) → owner = id        (예: "com.tasty.claude")
CallerContext::Local      → owner = "_host"    (HOST_OWNER, CLI·사용자)
```

`_host`(underscore sentinel, plugin 이 가질 수 없는 id)는 **root** — 모든 entry 의 owner check 를 통과한다. 사용자/CLI 가 plugin 이 만든 잘못된 데이터를 정리할 수 있어야 하므로 설계상의 root 권한이다(우회로 아님).

## Regular — 공유 네임스페이스 + owner enforcement

`(scope, key)` 가 **전역 unique**. 신규 `put` 시 owner 를 host 가 도장찍고(caller 가 명시 불가), 갱신·삭제 시 owner check:

| Caller | Entry owner | 결과 |
|--------|-------------|------|
| Plugin A | Plugin A | OK |
| Plugin A | Plugin B / `_host` | `OwnedByOther` (`-32006`) |
| `_host` | anything | OK (root) |

read(`get`/`list`/`exists`/`count`/`scopes`/`stats`)는 caller 무관 전체를 보고, 응답에 `owner` 가 포함된다("누가 만들었나"). 권한 토큰(`memory.read`/`write`)은 *메서드 호출 가능 여부*, owner check 는 *그 entry 권한* — 둘 다 통과해야 쓰기 성공.

## Secret — plugin 별 사전 분할

권한 체크가 아니라 **아키텍처적 분할**이다. plugin 입장에서 secret 은 자기만의 우주 — *다른 plugin 의 secret 은 개념 자체가 없다*(존재를 숨기는 게 아니라 접근 경로가 IPC 에 없다). PK 가 `(owner, scope, key)` 라 owner 다른 두 plugin 이 같은 `(scope, key)` 를 충돌 없이 쓴다. host 가 모든 secret 쿼리에 `WHERE owner = :caller_owner` 를 자동 부착하고, 응답에 `owner` 를 **포함하지 않는다**(추상화 누수 방지). R/W 를 토큰 하나(`memory.secret`)로 묶는다 — 항상 "자기 영역 only" 라 분리할 이유가 없다.

## CLI 표면

```text
tasty memory {put|get|delete|list|exists|count|scopes|stats} ...          # regular (_host=root)
tasty memory secret {put|get|delete|list|exists|count|scopes|stats} ...   # secret (_host 자기 영역)
tasty memory {bb|plan|cache} ... --workspace <id>                         # workspace 스코프 오버레이
tasty memory goal {set|get|clear} [--surface <id>]                        # surface 스코프 오버레이
```

`--owner` 플래그는 없다. 특정 plugin 의 regular entry 만 보려면 응답의 owner 를 grep/jq 로 사후 필터.

regular 와 secret 은 **짝마다 같은 params 를 받는다** — 핸들러가 같은 키를 읽고, secret 쪽은
owner 자동 부착과 응답의 owner 생략만 다르다. 그래서 **CLI 플래그도 짝마다 같게 유지한다.**
한쪽에만 플래그를 더하면 서버는 받는데 CLI 로는 닿을 길이 없는 자리가 생기고, 그 어긋남은
두 `--help` 를 나란히 놓고 읽기 전에는 보이지 않는다.

scope 선택자(`--scope` 와 alias 다섯)는 두 계열의 모든 자리가 **한 벌을 공유한다**. 자리마다
따로 적으면 설명·충돌 규칙이 자리 수만큼 갈린다 — `crates/tasty-cli/src/commands/memory.rs`
의 `ScopeArgs` 가 그 한 벌이고, 같은 파일의 테스트가 모든 자리에서 여섯이 온전한지·설명이
있는지·서로 배타인지를 clap 명령 트리에서 직접 확인한다.

## 스코프 확장 구조

같은 store 위에 도메인별 구조가 예약 키로 얹혀 IPC 로 제공된다. owner 규칙은 전부 regular 와 동일.

| 오버레이 | IPC | 예약 키 | 스코프 | 카디널리티 | TTL | 구현 |
|---|---|---|---|---|---|---|
| blackboard | `memory.bb_*` | `tasty.bb.<name>.*` | workspace | scope 당 다중 | 없음 | `crates/tasty-memory/src/blackboard.rs` |
| plan | `memory.plan_*` | `tasty.plan.<plan_id>` | workspace | scope 당 다중 | 없음 | `crates/tasty-memory/src/plan.rs` |
| cache | `memory.cache_*` | `tasty.cache.<key>` | workspace | scope 당 다중 | **필수** | `crates/tasty-memory/src/cache.rs` |
| goal | `memory.goal_*` | `tasty.goal` | surface | scope 당 **단일** | 없음 | `crates/tasty-memory/src/goal.rs` |

**goal** 은 surface(=에이전트 세션) 단위 단일 목표 문장이다 — 에이전트가 받은 goal 을 소비자(Stop-훅 게이트 등)가 읽을 수 있도록 키를 코드에 고정한 자리. prefix 가 아니라 완전한 단일 키인 것은 surface 당 하나뿐이라는 요구에서 따라온다. 상속은 없다(부모 surface 의 goal 이 자식에 보이지 않는다 — 자식이 자기 subtask 를 끝내고도 부모 goal 을 이유로 계속 도는 것을 막는다). 빈/공백-only goal 은 등록 시점에 거부한다.

goal 에 TTL 이 없는 이유: surface 스코프 데이터는 surface 가 닫힐 때(`purge_surface_memory_scope`) 와 앱 시작 시 복원되지 않은 surface 정리(`purge_dead_surfaces`) 로 scope 통째로 삭제된다. 두 경로 모두 키 필터가 없어 goal 도 자동 포함되므로 **goal 수명 = surface 수명** 이다.

스코프 인자는 IPC params 필수값이다 — 활성 workspace/surface 를 참조하지 않는다([focus](../policies/focus.md)). CLI 의 `tasty memory goal` 만 `--surface` 생략 시 caller 의 `TASTY_SURFACE_ID` env 로 채운다(에이전트가 자기 자신에 대해 호출하는 것이 주 용례).

## 용량 제한

| 제한 | 대상 | 기본값 | 초과 |
|------|------|--------|------|
| 단일 entry | `value` byte | 1 MiB | `ValueTooLarge` |
| Plugin secret quota | plugin 별 secret 합 | 10 MiB | `QuotaExceeded { scope: "secret" }` |
| Regular global quota | regular 전체 합 | 1 GiB | `QuotaExceeded { scope: "regular" }` |

`~/.tasty/config.toml` `[memory]` 의 `entry_max_mb`/`secret_quota_mb_per_plugin`/`regular_quota_mb_total` 로 재정의. **eviction 안 함** — 초과 시 명시적 에러(데이터가 말없이 사라지는 surprise 방지). `_host` 도 quota 를 받는다(root 는 owner check 에만 적용, 비대 방지는 동일).

## 파일 위생 — WAL 크기와 부팅 정리

`memory.db` 는 WAL 모드라 `memory.db-wal`(로그) · `memory.db-shm`(WAL-index) 을 함께 갖는다. 이 두 파일은 **가만히 두면 줄지 않는다** — SQLite 는 체크포인트 후에도 재사용을 위해 WAL 파일 크기를 유지하기 때문이다.

- **되감기 한도**: `prepare()` 가 `journal_size_limit` 을 `WAL_SIZE_LIMIT_BYTES`(= `wal_autocheckpoint` 임계와 정확히 같은 1000 페이지 × 4096B)로 건다. 한도를 넘긴 WAL 은 다음 되감기에서 잘려 나간다. **이 값은 활성 WAL 의 상한이 아니다** — 되감기가 일어날 때 남길 크기일 뿐이라, 읽는 쪽이 오래된 스냅샷을 쥐고 있어 되감기가 막힌 동안이나 큰 트랜잭션 도중에는 WAL 이 이 값을 넘어 계속 자란다(실측 2026-09-22, 격리 홈의 headless debug 인스턴스: 다른 연결이 읽기 트랜잭션을 쥔 채 900 000 B 값 6 건을 쓰면 활성 WAL 이 16 512 → 5 582 632 B 까지 컸고, 읽기를 푼 뒤의 다음 쓰기에서 정확히 4 096 000 B 로 잘렸다). 이 pragma 가 없으면 큰 트랜잭션이나 VACUUM 으로 한 번 부푼 WAL 이 프로세스 수명 내내 고착되고, `wal_autocheckpoint` 임계를 영구 초과한 상태가 되어 **커밋마다 그 크기만큼 WAL-index 를 훑는다**(실측: 169MB WAL 이 메인 스레드 CPU 를 상시 점유).
- **회수**: 한도는 되감기 때만 작동하므로, 이미 커진 WAL 은 `MemoryStore::checkpoint_truncate()` 로 되감기를 한 번 강제해야 줄어든다. 부팅 위생 정리(`src/boot.rs::maintain_memory_at_boot`)가 이것을 **VACUUM 뒤에** 1 회 수행한다 — VACUUM 은 DB 를 통째로 다시 쓰므로 그 자체로 WAL 을 크게 부풀린다.
- 같은 상한이 `state.db` 에도 적용된다 — 두 DB 가 **같은 함수**(`tasty_memory::pragma::apply_connection_pragmas`)를 부른다([storage](storage.md)).
- **위 문단의 WAL 은 파일 DB 일 때의 이야기다.** `MemoryStore::open_in_memory` 로 연 DB 는
  파일이 없어 SQLite 가 WAL 을 못 쓰고, 요청은 조용히 거절돼 `journal_mode` 가 `memory` 로
  남는다(반환값은 성공이다). 그 모드에는 `-wal`·`-shm` 도, 여기 적은 위생 문제도 없다.
  그래서 그 값은 실패가 아니라 정상 결과로 규정하고 경고하지 않는다 —
  [ADR-0316](../../adr/0316-a-database-reports-the-pragma-that-took-not-the-one-requested.md).
  반대로 **파일 DB 가 `memory` 로 서면 정상이 아니다** — 허용 결과는 모드별이다. 열린
  스토어는 되읽은 결과를 `MemoryStore::applied_pragmas()` 로 들고 있고, 실행 중에는
  `system.pressure` 의 `db_pragmas.memory_db`(CLI `tasty list pressure`)로 조회한다
  ([ADR-0376](../../adr/0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md)).
- **내구성 범위**(`synchronous=NORMAL` — 프로세스 kill 은 견디고 전원 장애는 최신 commit 을
  약속하지 않는다)와 **저장 실패의 의미**(원인 분류 · 실패한 쓰기는 quota 카운터와 변경
  버퍼를 안 옮긴다)는 [storage](storage.md) 의 두 절이 정본이다.
- **부팅이 `memory.db` 를 못 열면 앱은 in-memory 대체로 계속 뜬다** — `state.db` 와 달리 종료하지
  않는다. 그 상태의 쓰기는 재시작에 사라지므로 `db_pragmas.memory_db` 가 `degraded: true` 와
  `init_failure` 로, 쓰기 응답이 `durable: false` 로 그 사실을 말한다. 정본은 [storage](storage.md)
  "초기화 실패" 절의 `memory.db` 항이고 근거는
  [ADR-0485](../../adr/0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md).

## 보안·신뢰 모델

Secret 의 격리 약속은 **"plugin 간 IPC 격리" 하나로 좁혀져 있다.** 자세한 위협 모델·왜 암호화 안 하는가·sandbox 도입 시 미래 경로는 [ADR-0005](../../adr/0005-memory-secret-not-a-vault.md).

| 시나리오 | Regular | Secret |
|---|---|---|
| Plugin A → Plugin B 의 entry 갱신/secret 요청 | **차단**(`OwnedByOther`) | **차단**(owner 분리) |
| 사용자/host 가 모든 entry 조회·수정 | 허용 | 허용 |
| plugin/타 프로세스가 `memory.db` 파일 직접 열기 | 평문(책임 밖) | 평문(책임 밖) |

> 정말 민감한 데이터(master password, OAuth refresh token, 결제 key)는 secret 영역에 두지 *말고* OS keyring/외부 보관소를 쓴다 — [plugin-development 민감 데이터](../../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택).

## 관련

- 코드: `crates/tasty-memory/`
- [ADR-0005](../../adr/0005-memory-secret-not-a-vault.md) · [ADR-0004](../../adr/0004-ipc-transport-tcp.md)
- [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-development 민감 데이터](../../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)
- 저장 위치 규칙: [storage.md](storage.md) (`~/.tasty/` 전체 저장소 지도; `memory.db` 는 `state.db` 와 별도 연결)
