# 저장소 시스템 (Storage System)

tasty 의 영속 데이터는 **텍스트 파일과 SQLite 하이브리드**로 나뉜다. 사용자가 직접 편집·버전관리할 대상은 텍스트(TOML / 쉘 스크립트)로, 앱이 자동 누적·갱신하는 데이터는 SQLite 로 담는다.

모든 데이터는 `~/.tasty/` 아래에 모인다(Windows: `%USERPROFILE%\.tasty\`). 홈 디렉터리 결정은 `tasty_utils::path::tasty_home()`.

## 저장 위치

| 경로 | 포맷 | 내용 | 관리 주체 | 코드 |
|------|------|------|-----------|------|
| `state.db` (+ `-wal`/`-shm`) | SQLite | 종류별 최근 파일·폴더 | 앱 | `src/db.rs` |
| `memory.db` (+ `-wal`/`-shm`) | SQLite | 에이전트 메모리 (별도 스키마·연결) | 앱 | `crates/tasty-memory/` |
| `config.toml` | TOML | 사용자 설정(셸·외관·단축키·언어 등) | 사용자 | `crates/tasty-settings/` |
| `remote-profiles.toml` (+ `passkeys.toml`) | TOML | 원격 접속 프로필(`ssh`/`tasty-attach` kind) + 자격증명 — `config.toml` 과 분리해 손편집 보존 | 사용자 | `crates/tasty-remote-profiles/` |
| `file-handlers.toml` | TOML | 파일 detector / handler / 확장자 매핑 | 사용자 | `crates/tasty-file-handler/src/` |
| `themes/<id>.toml` | TOML | 테마 (id = 파일명 stem) | 사용자 / 앱 | `crates/tasty-settings/src/appearance.rs` |
| `bashrc` / `bashrc.default` | 쉘 스크립트 | 컴파일된 빌트인 rc (tasty 모드 / default 모드) — 셸을 `--rcfile` 로 띄움 | 앱 (빌드 산출물) | `crates/tasty-settings/src/general.rs` |
| `bashrc.user` | 쉘 스크립트 | 사용자가 직접 편집하는 fragment (빌트인 사이에 끼워짐) | 사용자 | 〃 |
| `presets/{workspace,tab,pane}/<name>.toml` | TOML | 레이아웃 프리셋 (탭/페인/서피스 구조) | 사용자 / 앱 | `crates/tasty-presets/` |

- **plugin 데이터는 여기 없다.** 각 plugin 은 자기 `TASTY_PLUGIN_DATA_DIR` 아래에 보관한다 (예: explorer 의 북마크 = `<data_dir>/bookmarks.json`). host `state.db` 에 plugin 데이터를 넣지 않는다.

## SQLite `state.db`

### 단일 schema 모델

증분 마이그레이션 체인은 **없다**(0.4 fresh-start 정책). `src/db/migrations.rs`:

- `SCHEMA_VERSION` 상수 하나. `ensure_schema()` 가 `PRAGMA user_version` 을 보고 분기:
  - `0`(새 DB) → `SCHEMA_SQL` 1회 적용 + `user_version` 을 `SCHEMA_VERSION` 으로 박음.
  - `== SCHEMA_VERSION` → **additive ensure**. `CREATE TABLE IF NOT EXISTS` 만 다시 돌린다.
  - 그 외 → `SchemaMismatch{expected, found}` 에러 → 호출자가 사용자에게 안내 후 종료.

**additive ensure 가 이 정책의 유일한 예외 통로다.** v1 이 나간 뒤에 생긴 테이블
(`tutorial_progress` — [ADR-0260](../../adr/0260-tutorial-progress-belongs-to-user-state.md))은
버전을 올리지 않고 그 갈래로 기존 DB 에 닿는다. 그래서 **거기에 얹는 스키마 변경은 버전
값으로 아무 신호를 내지 않는다** — 실측하면 그 줄을 지워도 나머지 시험이 전부 초록이었다.
지금은 `an_existing_database_still_gets_the_tutorial_table` 이 그 갈래를 고정한다. 새
테이블을 이 통로로 더하면 그 시험을 함께 넓혀라. **`CREATE TABLE IF NOT EXISTS` 가 아닌
것**(컬럼 변경·데이터 이동)을 여기 넣는 것은 마이그레이션이고, 그것이 필요하면
`SCHEMA_VERSION` 을 올려 fresh-start 를 정면으로 다시 논의해야 한다.

### v1 테이블

```sql
CREATE TABLE meta (              -- 스키마 메타데이터 (key-value)
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE recent_files (      -- 종류별 최근 경로
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    opened_at INTEGER NOT NULL,
    PRIMARY KEY(kind, path)
);
```

- `recent_files`에 현재 목록을 기록한다(`src/store/recent_files.rs`). `recent_markdown`은 레거시 호환 테이블이며 최초 로드 시 한 번만 이관한다.
- 파일 종류는 `kind`로 구분한다. 탐색기의 최근 방문 폴더도 `directory` kind로 같은 테이블을 쓴다.
- **경로 dedup**: 같은 파일의 다른 표기(구분자 `\`↔`/`, `\\?\` verbatim, `.`/`..`,
  Windows 대소문자 차)를 정규화 키(`strip_verbatim_prefix`+`lexically_normalize`+Windows
  case fold)로 접는다. PK 는 여전히 raw path(표시·열기용)이며 정규화 키는 비교 전용.
  `RecentFiles::add` 가 같은 키의 옛 행을 제거 후 저장하고, `load()` 는 마이그레이션 체인이
  없는 fresh-start 정책이라 로드 시 1회 정규화 dedup 패스로 기존 중복을 접는다.
- **기록 진입점**: markdown-open 이 수렴하는 인텐트 계층(`Intent::NewTab`/
  `ConvertSurface`, file-dispatch 직접 `CreateTab`)에서 `AppState::record_recent`
  로 1회 기록한다 — 파일-열기 팝업·주소창 navigate·링크 클릭이 모두 반영된다.

### 최근 목록의 창 간 일관성

`state.db`의 `Db`가 최근 목록 캐시를 한 벌 소유하고, `RecentFiles::load`는 그 캐시의
공유 핸들을 반환한다. 먼저 열린 창, 나중에 열린 창, 창이 닫힌 뒤 다시 열린 창 모두
같은 인스턴스의 목록을 본다. 종류별 최신순·정규화 중복 제거·10개 상한과 raw 경로 표기는
유지한다. 읽기는 최대 10개 경로의 스냅샷이며 DB 재조회나 목록 갱신을 하지 않는다.

기록은 캐시 갱신과 기존 DB 저장을 같은 캐시 락 안에서 순서대로 수행한다. DB 저장에
실패해도 이미 반영된 메모리 목록은 모든 창에 동일하게 남고, 오류는 기존 저장 경로가
로그로 남긴다. 인스턴스 간 공유나 외부 프로세스의 DB 직접 수정 감지는 지원하지 않는다.
근거는 [최근 캐시 소유권](../../adr/0275-recent-cache-belongs-to-the-state-database.md).

### 접근 규칙

- **메인 프로세스 단독 접근.** 자식 CLI 프로세스는 DB 를 직접 열지 않고 IPC 로 메인에 위임한다.
- 전역 싱글톤. `db::init()` 선행 호출 후 `db::with_state_db(|db| { ... })` 로 접근(미초기화면 `None`).
  접근자 이름이 `with_db` 가 아닌 것은 `memory.db` 쪽 접근자(`with_memory`)와 호출부에서
  구분되게 하기 위해서다 — 둘이 같은 파일에 함께 나오는 자리가 없어 오독이 조용하다.
- **여는 쪽은 GUI 부팅 하나다.** `init()`·`default_db_path()` 는 `cfg(feature = "gui")` 라
  헤드리스 빌드에는 이 DB 를 여는 코드가 컴파일되지 않는다. 그래서 헤드리스 데몬의 홈에는
  `memory.db` 만 생기고 `state.db` 는 파일도 로그도 남지 않는다.
- **`with_state_db` 의 `None` 은 "열려 있지 않다" 이고, 출처가 둘인데 이 값으로는 안
  갈린다.** 락 poison 은 복구되어 `Some` 이라 여기 오지 않는다. 남는 둘은 ① 헤드리스라 여는
  코드가 없다 ② GUI 가 열다 실패했다 — **②를 "앱이 끝나니까 안 보인다" 로 배제할 수 없다.**
  안내 모달의 `on_close` 가 `Exit(1)` 이라 종료는 사용자 확인 시점이고, 모달이 뜨기 전에
  `AppState::new` 의 최근 파일 로드가 이미 그 구간을 지난다. 최근 파일 조회는 두 경우 모두
  오류가 아니라 빈 목록으로 답한다.
- 그래서 **`None` 을 원인 판정에 쓰지 않는다.** "저장소가 없는 빌드니까 안내하지 않는다"
  류의 분기는 DB 가 깨진 사용자에게서 안내를 빼앗는다. 구분에 필요한 `DbInitError` 는 부팅이
  모달로 바꾼 뒤 버린다. 근거는 [저장소 소유권](../../adr/0335-the-state-database-is-opened-by-gui-boot-alone.md).
- PRAGMA: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `journal_size_limit`.
  네 pragma 는 `memory.db` 와 **같은 함수**(`tasty_memory::pragma::apply_connection_pragmas`)가
  건다 — 자유도 없는 사본이라 두 곳에 두지 않는다.
  - **요청한 값과 실제 적용된 값은 다른 축이다.** `journal_mode` 는 요청이 거절돼도 성공을
    반환하므로(실측: in-memory DB 에 WAL 을 요청하면 `Ok` 이고 값은 `memory`), 위 목록은
    *요청* 이고 실제 값은 되읽어야 안다. 그 함수가 매 연결마다 **넷 다** 되읽어 대조하고,
    어긋나면 `tracing::warn!` 을 남기며, 결과를 `AppliedPragmas` 값으로 돌려준다.
    근거는 [ADR-0316](../../adr/0316-a-database-reports-the-pragma-that-took-not-the-one-requested.md)
    과 그 보고 채널을 개정한 [ADR-0376](../../adr/0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md).
  - **허용 결과는 모드별이다.** 파일 DB 의 `journal_mode` 는 `wal` 만, in-memory DB 는
    `memory` 만 정상이다. `synchronous` · `foreign_keys` · `journal_size_limit` 은 두 모드 모두
    요청값 그대로여야 한다. `state.db` 는 항상 파일 DB 라 `journal_mode` 의 정상값이 `wal` 하나다.
  - **하나라도 안 섰으면 그 DB 는 `degraded` 다 — 치명이 아니다.** DB 는 열린 채로 쓰인다(열기
    자체의 실패는 아래 "초기화 실패" 절이 다루는 다른 축이다). 두 DB 의 결과는 실행 중에
    `system.pressure` 의 `db_pragmas` 덩어리(`memory_db` · `state_db`)로 조회한다 — CLI
    `tasty list pressure`. `state_db` 가 `null` 이면 이 프로세스에 열려 있지 않다는 뜻이고,
    출처가 둘인 것은 위 `with_state_db` 의 `None` 과 같다. `memory_db` 는 파일을 못 열어
    in-memory 대체로 떴을 때도 `degraded` 이고, 그때 `init_failure` 가 원인을 싣는다(아래
    "초기화 실패" 절의 `memory.db` 항).
  - `journal_size_limit` 은 WAL 을 **되감을 때 남길 크기의 한도**다 — 활성 WAL 의 상한이 아니다. 되감기가 막힌 동안(읽는 쪽이 오래된 스냅샷을 쥐고 있을 때)이나 큰 트랜잭션 도중에는 WAL 이 이 값을 넘어 자라고, 다음 되감기에서 이 크기로 잘린다(실측은 [memory](memory.md) "파일 위생"). **이 pragma 가 없으면 WAL 은 한 번 커진 크기를 영구히 유지한다** — SQLite 가 재사용을 위해 체크포인트 후에도 파일을 줄이지 않기 때문이다. 그러면 `wal_autocheckpoint` 임계를 영구 초과한 상태가 되어 커밋마다 체크포인트가 트리거되고 그 비용은 WAL 크기에 비례한다. 값은 임계와 정확히 같은 `tasty_memory::WAL_SIZE_LIMIT_BYTES`(= 1000 페이지 × 4096B)이고, 두 DB 가 그 상수를 쓰는 **같은 함수**를 부른다. 한때는 `src/db.rs` 와 `crates/tasty-memory/` 가 같은 네 줄을 각자 박아 두어 한쪽만 고치면 다른 쪽이 그대로 자랐다 — 그래서 사본을 없앴다.
- 쓰기는 `Connection::transaction()` 패턴. 실패 시 `tracing::warn!`/`error!` 기록 후 진행.

### 보장 범위 — `synchronous=NORMAL` 이 약속하는 것과 안 하는 것

두 DB 는 `journal_mode=WAL` + `synchronous=NORMAL` 로 연다(위 PRAGMA 목록). 이 조합의 내구성은
장애 종류에 따라 갈린다(<https://sqlite.org/pragma.html#pragma_synchronous> ·
<https://www.sqlite.org/wal.html>).

- **프로세스 crash · kill**: commit 이 돌아온 트랜잭션은 남는다. WAL 에 쓴 내용은 OS 페이지
  캐시에 있고, 프로세스가 죽어도 OS 가 그것을 파일로 내보낸다. 다음 열기가 WAL 을 되감아
  일관된 상태로 연다.
- **전원 장애 · OS crash**: **최신 commit 의 보존을 약속하지 않는다.** NORMAL 은 commit 마다
  WAL 을 fsync 하지 않고 체크포인트 때만 한다. 그래서 전원이 나간 순간 fsync 되지 않은 최근
  commit 들은 잃을 수 있다. DB 가 깨지지는 않는다 — 잃는 것은 끝부분의 commit 이고, 남은
  것은 일관된 이전 상태다.
- 이 값을 바꾸지 않는다. FULL 로 올리는 것은 commit 마다 fsync 비용을 받는 **별도 결정**이고,
  지금 코드는 적용 여부만 본다([ADR-0316](../../adr/0316-a-database-reports-the-pragma-that-took-not-the-one-requested.md)).
- **전원 장애 쪽은 측정된 적이 없다.** 그 줄은 SQLite 문서의 계약이다. 재는 법은 commit 직후
  전원을 끊고 재시작해 마지막 commit 의 생존을 보는 것인데 — **이 레포에 그 장비는 없다.**
- **프로세스 kill 쪽은 쟀다**(2026-09-21, Linux, 격리 홈의 GUI debug 인스턴스): `memory.put` 다섯
  건이 응답한 직후 `SIGKILL`, 같은 홈으로 재시작해 `memory.get` — 다섯 건 전부 남아 있었다. 한
  번의 관측이지 확률의 측정은 아니다.

### 저장 실패의 의미 — `memory.db`

`MemoryStore` 의 트랜잭션 쓰기(`put` · `delete` · `put_secret` · `delete_secret`)가 실패하면
그 호출은 `Err` 로 돌아온다. **실패한 commit 을 성공으로 돌려주는 경로는 없다.**

- **원인은 초기화와 같은 표로 갈린다** — `tasty_memory::StorageFailure`(`busy` · `disk_full` ·
  `io` · `corrupt` · `permission_denied` · `other`). `MemoryError::storage_failure()` 가 그 값을
  주고, 요청 거부(`NotFound` · `CasConflict` · quota 등)는 저장 실패가 아니라 `None` 이다.
- **IPC** 는 코드 `-32603` 과 문장 `memory db error: …` 을 그대로 두고
  `error.data.storage_failure` 에 원인을 싣는다.
- **메모리 쪽 상태는 실패 전 그대로다.** quota 카운터(`regular_used_bytes`)와 변경
  버퍼(`pending_changes`, `memory.changed` 이벤트의 원천)는 commit 성공 뒤에만 움직인다.
  트랜잭션은 롤백돼 디스크에도 안 남는다. 그래서 실패한 쓰기는 quota 를 먹지 않고 변경
  알림도 내지 않는다.
- **저장소가 파일이 아니면 성공도 durable 이 아니다.** 부팅이 `memory.db` 를 못 열어 in-memory
  대체로 떴으면(아래 "초기화 실패" 절) commit 은 성공하지만 프로세스와 함께 사라진다. 그때
  `memory.db` 에 쓰는 IPC 쓰기 계열의 성공 응답은 `ok` 를 그대로 두고 `durable: false` 를 더한다 —
  `memory.*` 뿐 아니라 같은 저장소에 쓰는 `agent.*` · `approval.*` · `surface.meta.*` ·
  `telemetry.*` · `session.*` 도 같다([ADR-0611](../../adr/0611-every-ipc-write-to-memory-db-says-when-it-is-not-durable.md)).
  파일 DB 에서는 이 칸이 없다 — 칸이 없는 성공은 위 "보장 범위" 가 말하는 durable 이다.
- 근거는 [ADR-0377](../../adr/0377-a-failed-memory-write-names-its-cause-with-the-same-table-as-init.md).

### 터미널 출력 observer 의 memory sink — 저장 계약

`output.observe_start` 로 등록한 observer 중 sink 가 memory 인 것은 파싱된 항목을 `memory.db` 에
쓴다(`src/core/output_observer.rs` 의 `run_memory_sink`). 이 sink 는 **store 의 port
(`MemoryStorage`)만 본다** — 도메인 `core` 를 참조하지 않는다.

- **키**: `global` 스코프의 `tasty.observer.<id>.<ms>`, owner 는 `_host`. `<ms>` 는 쓰는 순간의
  밀리초라 **같은 밀리초에 온 두 항목은 한 키로 겹친다**(뒤엣것이 덮어쓴다). 한 줄에서 여러
  항목이 나오면 흔히 겹친다.
- **상한 `max_records` 는 근사이고, 겹친 키와 만나면 레코드를 전부 잃을 수 있다.** sink 는 자기가
  쓴 키를 순서대로 기억해 넘치면 가장 오래된 것부터 지우는데, 겹친 키는 그 기억에 **여러 번**
  들어가 있다. 그래서 오래된 한 칸을 지우는 삭제가 같은 이름의 **지금 살아 있는** 레코드를
  지운다. 실측(격리 홈, `--max-records 2`, 한 번의 `send text` 로 url 여섯 항목): `total_out` 6 인데
  남은 키 0. 삭제는 best-effort 다 — 실패해도 경고 없이 넘어간다. sink 가 재시작하면 기억이
  비므로 이전 실행이 남긴 키는 이 상한의 대상이 아니다.
- **put 실패는 그 항목만 버린다.** `tracing::warn!` 을 한 줄 남기고 다음 항목으로 간다 — sink 가
  멈추지 않는다. **소비자에게 gap 신호는 가지 않는다**: observer 의 `dropped` 는 채널 역압으로
  못 넣은 항목만 세고 put 실패는 세지 않는다. 원인은 위 "저장 실패의 의미" 의 표로 갈리지만 이
  sink 는 그것을 로그 문장에만 싣는다.
- **락 poison**: sink 는 store 락을 `tasty_utils::poison::recover_mutex` 로 복구해 계속 쓰고, 보고
  좌표는 store 의 port 가 준다(`tasty_memory::STORE_LOCK_WHAT` · `STORE_LOCK_POISONED`). 본체
  `core` 의 `MEMORY_WHAT` · `MEMORY_POISONED` 는 **같은 static 의 재수출**이라 프로세스에 첫-1 회
  플래그가 하나다.
- **종료 계약**: surface 가 닫히면 sink 의 sender 만 떨어뜨리고 join 은 미룬다 — 채널에 들어간
  항목은 std mpsc 계약상 워커가 끝까지 비운 뒤 끝나므로 잃지 않는다. 앱 종료 경로가
  `join_retired` 로 남은 워커를 회수하고, 그 호출을 빠뜨린 경로에서도 라우터의 `Drop` 이 같은
  회수를 한다(마지막 방어선). 유일한 유실 경로는 워커가 다 쓰기 전에 프로세스가 죽는 것이다.
- 근거는 [ADR-0378](../../adr/0378-the-poison-coordinate-of-the-memory-store-lives-at-its-port.md).

### 초기화 실패 — `state.db` 는 종료, `memory.db` 는 in-memory 대체 + degraded

**두 DB 의 정책이 반대다.** 원인 분류는 같은 표를 쓰지만, 실패한 뒤에 하는 일은 다르다.

#### `state.db` — 인메모리 폴백 없음

`db::init()` 실패는 **치명적**이다. `:memory:` 폴백을 두지 않는다 — `DbInitError` 로 분류해 사용자에게 InfoModal 로 안내한 뒤 앱을 종료한다(`src/app/window_lifecycle.rs`). variant 별로 i18n key 를 가진다:

| variant | 의미 | i18n key |
|---------|------|----------|
| `HomeDirMissing` | 홈 디렉터리 미확인 | `db_error.home_missing` |
| `PermissionDenied(path)` | 권한 거부 / CANTOPEN | `db_error.permission_denied` |
| `Busy(path)` | DB lock/busy | `db_error.busy` |
| `DiskFull` | 디스크 가득 | `db_error.disk_full` |
| `Corrupt(path)` | 손상 / NotADatabase | `db_error.corrupt` |
| `SchemaMismatch{expected,found}` | user_version 불일치 | `db_error.schema_mismatch` |
| `Other(msg)` | 그 외 | `db_error.other` |

`memory.db`(`crates/tasty-memory/`)는 같은 variant 를 가진 자기 타입(`MemoryInitError`)을
쓴다. **원인 표는 둘이 공유하는 하나다** — `tasty_memory::StorageFailure` 가 SQLite 오류를
분류하고, 두 타입은 그 결과를 자기 variant 로 옮기기만 한다. 그 표의 `io` 갈래는 초기화
안내에 따로 된 문구가 없어 `Other` 로 간다.

#### `memory.db` — in-memory 대체로 계속 뜨고, degraded 로 말한다

`tasty_memory::init_with_config` 가 실패해도 부팅은 **종료하지 않는다.** 로그에
`memory.db init at boot failed: <원인>` 을 남기고 in-memory 저장소로 대체해 계속 뜬다(GUI ·
headless 같은 동작). 손상된 파일로도 앱을 쓸 수 있게 하는 것이 목적이다. 대체 저장소는 기본
config 로 열리고, 원래 파일은 건드리지 않는다(손상 파일은 그 자리에 그대로 남는다).

그 상태는 조용하지 않다:

- `system.pressure` 의 `db_pragmas.memory_db` 가 `degraded: true` · `in_memory: true` 이고
  `init_failure: {cause, error}` 가 원인을 싣는다. `cause` 는 `MemoryInitError::cause()` 의 이름
  (`home_missing` · `permission_denied` · `busy` · `disk_full` · `corrupt` · `schema_mismatch` ·
  `other`)이다. 정상이면 `init_failure` 는 `null` 이다.
- `memory.db` 에 쓰는 IPC 쓰기 계열(`memory.*` · `agent.*` · `approval.*` · `surface.meta.*` ·
  `telemetry.*` · `session.*`)의 성공 응답이 `durable: false` 를 더한다(위 "저장 실패의 의미").
- 로그에 `memory.db falls back to an in-memory store (cause=…) — writes will not survive a
  restart` 가 한 줄 더 남는다.
- **화면 안내는 없다** — `state.db` 와 달리 InfoModal 도 toast 도 뜨지 않는다.

근거·대안(fatal · 쓰기 거절)·재검토 조건은
[ADR-0485](../../adr/0485-a-memory-db-that-failed-to-open-falls-back-in-memory-and-says-so.md).

## 텍스트 파일을 SQLite 로 옮기지 않는 이유

`config.toml` / `remote-profiles.toml` / `file-handlers.toml` / `themes/*.toml` / `bashrc.user` 는 **사용자 편집·버전관리 대상**이다. 주석·diff 추적에 텍스트가 적합하고 앱이 자동으로 덮어쓰지 않으므로 SQLite 로 옮길 이점이 없다. 반대로 최근 파일처럼 앱이 자동 누적하는 데이터는 SQLite 가 맞다.

## 백업

`state.db` 는 WAL 덕분에 단일 프로세스 종료 시점에 일관성이 보장된다. 별도 주기 백업은 만들지 않는다. 수동 복사 시 `state.db`, `state.db-wal`, `state.db-shm` 세 파일을 함께 복사한다.

**해석하지 못한 사용자 파일은 덮어쓰기 전에 보존한다.** `config.toml` 과 `layouts/NN.json` 은 앱이 다시 쓰는 파일이라, 파싱에 실패한 뒤 기본값으로 폴백하면 다음 저장이 원본을 지운다. 그래서 **저장 직전에** 원본을 `<파일명>.bak`(중복이면 `.bak.2` … `.bak.9`)으로 **rename** 해 자리를 비운 뒤 쓴다. copy 가 아니라 rename 인 이유는 원본이 자리를 떠야 이어지는 write 가 데이터를 지우지 않기 때문이다.

**손상 슬롯이 하나라도 있으면 스크롤백 GC 는 통째로 멈춘다.** `gc_scrollback_orphans_all_slots_in` 은 슬롯 하나라도 읽거나 해석하지 못하면 그 회차의 GC 를 포기하고 모든 `.bin` 을 남긴다 — 그 슬롯이 무엇을 참조했는지 모르는 채로 지우면 백업(`NN.json.bak`)에서 되살릴 때 스크롤백만 빈 채로 복원되기 때문이다("모르면 지우지 않는다"). 절충은 사용자가 손상 슬롯을 방치하는 동안 스크롤백이 계속 쌓인다는 것이다. 디스크가 왜 줄지 않는지, GC 가 왜 안 도는지 의심될 때는 `layouts/` 에 해석되지 않는 슬롯이 남아 있는지부터 본다.

**보존 시점은 로드가 아니라 저장이다.** 로드는 "해석하지 못했다" 는 사실만 값에 실어 돌려주고 파일은 그대로 둔다. 부팅 중 같은 파일을 읽는 곳이 여럿이고(설정은 런처와 GUI 가 각각, 레이아웃 슬롯은 scrollback GC 와 engine 이 각각) 그 둘은 별개 프로세스라, 읽는 쪽이 파일을 옮기면 나중에 읽는 쪽은 "파일 없음" 만 보게 된다 — 사용자에게 알릴 주체가 사건을 모르게 되고, 경합에 진 쪽이 애먼 저장 금지를 걸기도 한다.

보존이 실패했거나 **읽기가 실패한**(권한 · IO) 경우에는 파일을 건드리지 않고 그 대상에 대한 저장을 막는다. 내용을 확인하지 못한 파일을 옮기면 일시적 오류에도 사용자 데이터가 자리를 뜨기 때문이다. 공용 헬퍼는 `tasty_utils::path::preserve_corrupt_file`.

**같은 `TASTY_HOME` 을 두 인스턴스가 쓰면 창이 남는다.** 부팅 판정과 첫 저장 사이(수 분)에 다른 인스턴스가 정상 파일을 써 넣을 수 있으므로, 옮기기 직전에 파일을 다시 읽어 지금도 해석되지 않는지 확인한다. 다만 그 **재확인(read)과 옮기기(rename)는 별개 syscall 이고 사이에 잠금이 없다** — 그 사이에 끼어든 write 는 여전히 정상 파일을 `.bak` 으로 흘린다. 파일 잠금을 도입하지 않은 것은 같은 홈의 다중 인스턴스가 지원 구성이 아니고(슬롯 점유는 프로세스 안에서만 본다), 남은 창의 폭이 두 syscall 사이라 실무상 도달하기 어렵기 때문이다. 데이터가 사라지는 것이 아니라 백업 예산(9개)이 한 칸 깎이는 형태로만 드러난다.

## 테스트

- 스키마 로직: `:memory:` Connection 으로 단위 테스트(`src/db/migrations.rs` 의 `tests` — fresh init / 재호출이 버전을 안 바꿈 / additive ensure 가 기존 DB 에 닿음 / mismatch 두 갈래).
- 에러 분류: 표 자체는 `crates/tasty-memory/src/failure.rs` 의 `each_sqlite_code_lands_in_its_own_branch`,
  `state.db` 쪽 옮기기는 `classify_sql` 단위 테스트(busy / corrupt / notadb), `user_message_i18n` key 안정성 테스트.
- 저장 실패: `crates/tasty-memory/src/tests.rs` 가 잠금(`busy`)과 페이지 상한(`disk_full`)을 실제로
  일으켜 원인과 메모리 쪽 상태를 본다. `io` 는 합성 오류 코드로 표만 본다.
- 적용값: `an_open_store_carries_the_pragmas_that_took_in_each_mode` ·
  `a_read_only_database_reports_itself_degraded` 가 모드별 허용 결과와 degraded 판정을 본다.

## 관련

- [memory.md](memory.md) — 에이전트 메모리(`memory.db`) 두 계층·소유 모델
- [`features/layout-presets`](../../features/layout-presets/index.md) — `presets/` 프리셋 적용
- [theme.md](theme.md) — `themes/*.toml` 토큰 모델
