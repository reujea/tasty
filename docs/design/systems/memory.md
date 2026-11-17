# 에이전트 메모리 시스템

## 원칙과 현재 동작

[데이터 원칙](../../principles/control-and-data.md)은 터미널에서 읽을 수 있는 자료의 접근·전송을 별도로 제한하지 않는다. 아래 secret owner·Passkey API 마스킹은 현행 구현 범위이며 별도 실행 보안 경계나 향후 Tasty 로그인 예외로 승인된 것은 아니다.

`~/.tasty/memory.db`는 AI 에이전트와 plugin의 작업 데이터를 저장하는 SQLite WAL 기반 키-값 저장소다. 본체는 `init_with_config`로 열고 `Arc<Mutex<dyn MemoryStorage>>`로 Core에 전달해 동기 접근한다(`crates/tasty-memory/`).

이 문서는 조회 범위와 소유권을 설명한다. 저장 암호화를 제공하지 않는 이유는 이 문서, IPC의 신뢰 범위는 [IPC 서버](../../architecture/ipc-server.md)을 따른다.

## 책임 범위

수 KB의 토큰부터 수백 KB~1 MiB의 캐시·JSON·누적 상태를 저장한다. 용량 상한을 넘는 데이터는 파일로 분리하고 메모리에는 경로를 저장한다. 큰 파일의 수명을 따로 관리하고 OS 도구로도 사용할 수 있다.

| 책임진다 | 책임지지 않는다 |
|---|---|
| 작업 상태·진행 메타데이터, 토큰·설정, cap 안의 캐시/중간결과, 가벼운 공유 데이터 | 큰 binary asset(이미지/모델/미디어), 영구 로그, 임의 크기 plugin 간 교환(파일 참조/`tasty-shm`) |

## 두 계층: regular / secret

같은 `memory.db` 안이지만 테이블·IPC 네임스페이스·가시성이 다르다.

| 영역 | IPC | Agent 권한 토큰 | 가시성 | 쓰기 |
|------|-----|-----------|--------|------|
| Regular | `memory.*` | `memory.read` / `memory.write` | **모든 plugin 이 모든 entry 읽기** | **owner 본인** 또는 host |
| Secret | `memory.secret.*` | `memory.secret` | **각 plugin 은 자기 영역만** | 자기 영역만 |

둘 다 같은 `Scope`(`global`/`account:<u>`/`window:<id>`/`workspace:<id>`/`surface:<id>`)와 키 규칙(1..=256자 `[a-z0-9._-]+`)을 공유한다. 차이는 **`owner` 차원** 하나다.

`surface:<id>` 의 `<id>` 는 **surface id 공간**(`< 0x8000_0000`)이어야 한다 — 그 이상은 headless PTY id 공간이라 실재하는 surface 가 가질 수 없는 값이고, IPC 가 `invalid_params` 로 거부한다([포커스 정책 (운영 상세)](../policies/focus.md)).

`surface:<id>` scope 에 항목을 새로 만드는 쓰기(`memory.put` · `memory.secret.put`)는 **지금 열린 surface** 에만 받는다.
- 라우터가 그 scope 의 surface 를 요청 대상으로 읽어 소유 창·parked engine 으로 보낸다. 어느 engine 에도 없으면 다른 surface 대상 요청과 같은 `-32602`(`no live surface <id> (named by 'memory.put')`)로 거절한다. 핸들러도 받은 engine 에 그 surface 가 없으면 `-32602 Surface <id> not found` 로 한 번 더 거절한다.
- 읽기·삭제(`get` · `list` · `exists` · `count` · `delete` 와 secret 의 같은 메서드)는 surface 가 없어도 받는다. 닫힌 surface 의 scope 에 남은 항목을 확인하고 지울 수 있게 하기 위해서다.
- surface 를 닫으면 그 surface 의 scope 를 regular·secret 모두 지운다(`purge_scope`, 닫기의 metadata 정리). 그래서 닫힌 surface 의 scope 에 항목이 남는 것은 이 검사가 생기기 전에 쓴 항목뿐이다. 이런 항목은 자동으로 지우지 않으며 `memory list --surface <id>` 로 찾아 `memory delete` 로 지운다. surface id 는 재사용하지 않으므로 남은 항목이 새 surface 에 붙지는 않는다.
- `memory.import` 도 regular 영역에 항목을 새로 만드는 쓰기라 같은 조건을 따른다. 쓰기 전에 모든 항목의 scope 를 파싱하고 PTY id 공간과 열린 surface 를 검사하며, 한 항목이라도 걸리면 요청 전체를 `-32602`(`invalid scope: …` 또는 `Surface <id> not found`)로 거절하고 아무 항목도 넣지 않는다. 그 항목만 건너뛰고 `skipped` 로 세는 방식은 쓰지 않는다 — put 과 같은 오류로 맞추고, 백업 파일에 그 인스턴스에 없는 surface 의 항목이 섞여 있으면 조용히 빠지는 대신 호출자가 알고 그 항목을 걸러 다시 넣게 하기 위해서다.
- import 는 항목마다 scope 가 달라 요청 대상 하나로 라우팅할 수 없다. GUI 라우터는 대상 없는 요청처럼 포커스 창의 engine 으로 보내면서 모든 창·parked·임시 engine 의 열린 surface 를 요청에 붙이고, 핸들러는 그 집합과 받은 engine 으로 판정한다. 그래서 다른 창의 열린 surface 항목은 거절하지 않는다. 집합을 붙이지 않는 경로(engine 이 하나인 headless)는 받은 engine 만 본다. import 는 저널이 미뤘다가 다시 실행하는 요청(저널 재개) 대상이 아니므로 그 경로에서 집합이 빠지는 일은 없다.

## owner — 숨겨진 host 전용 차원

`owner`는 호출자가 지정할 수 없는 내부 소유자 값이다. 호스트가 caller에서 정한다. regular 조회 응답에는 소유자가 표시되고 secret 응답에는 표시되지 않는다:

```
CallerContext::Plugin(id) → owner = id        (예: "com.tasty.claude")
CallerContext::Agent(id)  → owner = id
CallerContext::Local      → owner = "_host"    (HOST_OWNER, CLI·사용자)
```

`_host`(underscore sentinel, plugin 이 가질 수 없는 id)는 **root** — 모든 entry 의 owner check 를 통과한다. 사용자/CLI 가 plugin 이 만든 잘못된 데이터를 정리할 수 있어야 하므로 설계상의 root 권한이다(우회로 아님).

## Regular — 공유 네임스페이스 + owner enforcement

`(scope, key)` 가 **전역 unique**. 신규 `put` 시 owner 를 호스트가 지정하고(호출자가 지정할 수 없음), 갱신·삭제 시 owner check:

| Caller | Entry owner | 결과 |
|--------|-------------|------|
| Plugin A | Plugin A | 허용 |
| Plugin A | Plugin B / `_host` | `OwnedByOther` (`-32006`) |
| `_host` | 모든 소유자 | 허용(관리자) |

regular 읽기는 공유 데이터를 조회하되, plugin·agent caller의 raw KV 요청에서는 `tasty.` 호스트 예약 키를 숨긴다([권한 규칙](../../dev-guide/plugin-permissions.md#호스트-키-namespace-는-memory-권한으로-열리지-않는다)). 응답에 `owner` 가 포함된다("누가 만들었나"). Agent 의 권한 토큰(`memory.read`/`write`)은 *메서드 호출 가능 여부*, owner check 는 *그 entry 권한* — 둘 다 통과해야 쓰기 성공. plugin 은 토큰 없이 호출하고 owner check 만 받는다.

## Secret — plugin 별 사전 분할

Secret 쿼리는 caller의 owner로 제한한다. plugin은 다른 owner의 키나 존재 여부를 조회할 수 없다. PK 가 `(owner, scope, key)` 라 owner 다른 두 plugin 이 같은 `(scope, key)` 를 충돌 없이 쓴다. host 가 모든 secret 쿼리에 `WHERE owner = :caller_owner` 를 자동 부착하고, 응답에 `owner` 를 **포함하지 않는다**(추상화 누수 방지). R/W 를 토큰 하나(`memory.secret`)로 묶는다 — 항상 "자기 영역 only" 라 분리할 이유가 없다.

## CLI 표면

```text
tasty memory {put|get|delete|list|exists|count|scopes|stats} ...          # regular (_host=root)
tasty memory secret {put|get|delete|list|exists|count|scopes|stats} ...   # secret (_host 자기 영역)
tasty memory {bb|plan|cache} ... --workspace <id>                         # workspace 스코프 오버레이
tasty memory goal {set|get|clear} [--surface <id>]                        # surface 스코프 오버레이
```

`--owner` 플래그는 없다. 특정 plugin 의 regular entry 만 보려면 응답의 owner 를 grep/jq 로 사후 필터.

regular와 secret의 대응 메서드는 같은 인자를 받는다. secret의 소유자 조건과 응답에서 owner를 생략하는 점만 다르므로 CLI 플래그도 같게 유지한다.

범위 선택은 `crates/tasty-cli/src/commands/memory.rs`의 `ScopeArgs`를 공유한다. `--scope`와 별칭 다섯 개를 각 명령에서 반복 정의하지 않는다. 같은 파일의 테스트가 clap 명령 트리에서 여섯 선택자의 존재·도움말·상호 배타 여부를 확인한다.

## 스코프 확장 구조

같은 store 위에 도메인별 구조가 예약 키로 얹혀 IPC 로 제공된다. owner 규칙은 전부 regular 와 동일.

| 오버레이 | IPC | 예약 키 | 스코프 | 카디널리티 | TTL | 구현 |
|---|---|---|---|---|---|---|
| blackboard | `memory.bb_*` | `tasty.bb.<name>.*` | workspace | scope 당 다중 | 없음 | `crates/tasty-memory/src/blackboard.rs` |
| plan | `memory.plan_*` | `tasty.plan.<plan_id>` | workspace | scope 당 다중 | 없음 | `crates/tasty-memory/src/plan.rs` |
| cache | `memory.cache_*` | `tasty.cache.<key>` | workspace | scope 당 다중 | **필수** | `crates/tasty-memory/src/cache.rs` |
| goal | `memory.goal_*` | `tasty.goal` | surface | scope 당 **단일** | 없음 | `crates/tasty-memory/src/goal.rs` |

**goal** 은 surface(=에이전트 세션) 단위 단일 목표 문장이다 — 에이전트가 받은 goal 을 소비자(Stop-훅 게이트 등)가 읽을 수 있도록 키를 코드에 고정한 자리. prefix 가 아니라 완전한 단일 키인 것은 surface 당 하나뿐이라는 요구에서 따라온다. 상속은 없다(부모 surface 의 goal 이 자식에 보이지 않는다 — 자식이 자기 subtask 를 끝내고도 부모 goal 을 이유로 계속 도는 것을 막는다). 빈/공백-only goal 은 등록 시점에 거부한다.

goal 에 TTL 이 없는 이유: surface 스코프 데이터는 surface 가 닫힐 때(`purge_surface_memory_scope`) scope 통째로 삭제된다. 이 경로는 키 필터가 없어 goal 도 자동 포함되므로 **goal 수명 = surface 수명** 이다. 앱 시작 시 복원되지 않은 surface 의 scope 를 지우는 경로는 없다(`SurfaceMetaStore::purge_dead_surfaces` 는 시험 전용이다). 닫힌 surface 의 scope 에 새 항목이 생기지 않도록 쓰기(put · secret.put · import)가 열린 surface 만 받는다.

스코프 인자는 IPC params 필수값이다 — 활성 workspace/surface 를 참조하지 않는다([focus](../policies/focus.md)). CLI 의 `tasty memory goal` 만 `--surface` 생략 시 caller 의 `TASTY_SURFACE_ID` env 로 채운다(에이전트가 자기 자신에 대해 호출하는 것이 주 용례).

## 용량 제한

| 제한 | 대상 | 기본값 | 초과 |
|------|------|--------|------|
| 단일 entry | `value` byte | 1 MiB | `ValueTooLarge` |
| Plugin secret quota | plugin 별 secret 합 | 10 MiB | `QuotaExceeded { scope: "secret" }` |
| Regular global quota | regular 전체 합 | 1 GiB | `QuotaExceeded { scope: "regular" }` |

`~/.tasty/config.toml` `[memory]` 의 `entry_max_mb`/`secret_quota_mb_per_plugin`/`regular_quota_mb_total` 로 재정의. 초과하면 기존 데이터를 자동 삭제하지 않고 오류를 반환한다. `_host` 도 quota 를 받는다(root 는 owner check 에만 적용, 비대 방지는 동일).

## 만료(TTL)

`expires_at <= now` 인 항목은 조회·목록·개수와 쓰기 판정 모두에서 없는 것으로 본다. 정리되지 않은 만료 행 위의 put 은 신규 생성이다 — 다른 owner 도 쓸 수 있고 version 은 1 부터, cas 는 actual 0 으로 판정한다. regular 전체 quota 와 secret owner 별 quota 는 만료 행 크기를 세지 않는다. 디스크의 만료 행은 로그 정리 틱(10분)마다 기다리지 않는 writer 요청이 지우고, `memory.gc` 로 바로 지울 수도 있다. 만료 행을 덮어쓴 put 은 regular 에서 `memory.changed` 를 `expired`·`created` 순서로 낸다. 판정과 이벤트 순서의 계약은 [저장 계약](storage-contract.md)에 있다.

## 파일 위생 — WAL 크기와 부팅 정리

`memory.db` 는 WAL 모드라 `memory.db-wal`(로그) · `memory.db-shm`(WAL-index) 을 함께 갖는다. SQLite는 체크포인트 뒤에도 재사용을 위해 WAL 파일 크기를 유지하므로 체크포인트 성공이 곧 파일 축소를 뜻하지는 않는다.

- **되감기 한도**: `prepare()`는 `journal_size_limit`을 `WAL_SIZE_LIMIT_BYTES`로 설정한다. 값은 `wal_autocheckpoint` 기준과 같은 1000페이지 × 4096B다. 이보다 큰 WAL은 다음 되감기에서 줄어든다. **활성 WAL의 크기 상한은 아니다.** 오래된 읽기 스냅샷이 되감기를 막거나 큰 트랜잭션을 처리하는 동안에는 더 커질 수 있다. 한도를 두지 않으면 커진 WAL이 계속 남아 커밋마다 WAL-index를 확인하는 비용이 커질 수 있다.
- **회수**: 한도는 되감기 때만 작동하므로, 이미 커진 WAL 은 `MemoryStore::checkpoint_truncate()` 로 되감기를 한 번 강제해야 줄어든다. 부팅 위생 정리(`src/boot.rs::maintain_memory_at_boot`)가 이것을 **VACUUM 뒤에** 1 회 수행한다 — VACUUM 은 DB 를 통째로 다시 쓰므로 그 자체로 WAL 을 크게 부풀린다.
- 같은 상한이 `state.db` 에도 적용된다 — 두 DB 가 **같은 함수**(`tasty_memory::pragma::apply_connection_pragmas`)를 부른다([storage](storage.md)).
- **위 문단의 WAL 은 파일 DB 일 때의 이야기다.** `MemoryStore::open_in_memory` 로 연 DB 는
  파일이 없어 SQLite 가 WAL 을 못 쓰고, 요청은 조용히 거절돼 `journal_mode` 가 `memory` 로
  남는다(반환값은 성공이다). 그 모드에는 `-wal`·`-shm` 도, 여기 적은 위생 문제도 없다.
  그래서 그 값은 실패가 아니라 정상 결과로 규정하고 경고하지 않는다 —
  [저장 계약과 writer 명부](storage-contract.md).
  반대로 **파일 DB가 `memory` 모드로 열리면 정상이 아니다** — 허용 결과는 모드별이다. 열린
  스토어는 되읽은 결과를 `MemoryStore::applied_pragmas()` 로 들고 있고, 실행 중에는
  `system.pressure` 의 `db_pragmas.memory_db`(CLI `tasty list pressure`)로 조회한다
  ([저장 계약과 writer 명부](storage-contract.md)).
- **내구성 범위**(`synchronous=NORMAL` — 프로세스 kill 은 견디고 전원 장애는 최신 commit 을
  약속하지 않는다)와 **저장 실패의 의미**(원인 분류 · 실패한 쓰기는 quota 카운터와 변경
  알림 버퍼를 갱신하지 않는다)는 [storage](storage.md) 의 두 절이 정본이다.
- **부팅이 `memory.db` 를 못 열면 앱은 in-memory 대체로 계속 뜬다** — 종료하지
  않는다. 그 상태의 쓰기는 재시작에 사라지므로 `db_pragmas.memory_db` 가 `degraded: true` 와
  `init_failure` 로, 쓰기 응답이 `durable: false` 로, 사용자에게는 경고 알림 하나로 그 사실을 말한다. 정본은 [storage](storage.md)
  "초기화 실패" 절의 `memory.db` 항이고 근거는
  [저장 계약과 writer 명부](storage-contract.md).

## 보안·신뢰 모델

memory secret은 평문 BLOB이며 plugin별 IPC owner 격리만 제공한다. 다른 plugin의 존재 조회도 막지만 같은 OS 사용자 프로세스의 DB 직접 접근·백업·장치 도난을 막는 암호화 보관소는 아니다. OS sandbox 없는 plugin에서 host가 keyring 암호화만 추가해도 프로세스 격리가 완성되지 않아 AES-GCM과 평문 fallback 혼합은 채택하지 않았다. 실제 민감 자격증명은 plugin이 OS keyring 또는 외부 저장소 정책으로 다룬다. sandbox나 저장 시 암호화 요구가 생기면 실제 접근 제한을 검증한 뒤 보호 약속을 다시 정한다.

Local TCP 연결의 신뢰 범위는 [IPC 서버](../../architecture/ipc-server.md#연결과-신뢰-범위)를 따른다.

### Passkey 저장과 열람

Passkey는 프로필에서 이름으로 참조하며 passkeys.toml에는 name·kind·path만 저장한다. path는 사용자 파일을 참조하고 inline은 Tasty 소유 파일로 만들어 삭제 시 함께 지운다. Unix 파일 0600·디렉터리 0700으로 보호하고 대화형 이름은 허용 문자 밖을 거절하되 migration 이름은 치환한다. IPC·CLI는 내용을 반환하지 않고 마스킹한다. 로컬 GUI와 승인된 plugin의 선언 타입 열람은 설치 신뢰에 기반한 편의이며 OS 수준 격리를 뜻하지 않는다. 저장 암호화·마스터 암호는 headless 자동 접속과 플랫폼 비용 때문에 현재 보류다.

## 관련

- 코드: `crates/tasty-memory/`
- 이 문서 · [IPC 서버](../../architecture/ipc-server.md)
- [plugin-permissions](../../dev-guide/plugin-permissions.md) · [plugin-development 민감 데이터](../../dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)
- 저장 위치 규칙: [storage.md](storage.md) (`~/.tasty/` 전체 저장소 지도; `memory.db` 는 `state.db` 와 별도 연결)
