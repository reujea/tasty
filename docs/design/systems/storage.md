# 저장소 시스템 (Storage System)

tasty 의 영속 데이터는 텍스트 파일과 SQLite에 나눠 저장된다. 사용자가 직접 편집·버전관리할 대상은 텍스트(TOML / 쉘 스크립트)로, 앱이 자동 누적·갱신하는 데이터는 SQLite 로 담는다.

모든 데이터는 `~/.tasty/` 아래에 모인다(Windows: `%USERPROFILE%\.tasty\`). 홈 디렉터리 결정은 `tasty_utils::path::tasty_home()`.

## 저장 위치

| 경로 | 포맷 | 내용 | 관리 주체 | 코드 |
|------|------|------|-----------|------|
| `structure/journal.db` (+ `-wal`/`-shm`) | SQLite | 확정 구조 이벤트·명령 결과·effect 상태·불변 payload·snapshot·View restore manifest | App journal worker | `src/runtime/journal_product` · `crates/tasty-event-store` |
| `state.db` (+ `-wal`/`-shm`) | SQLite | 종류별 최근 파일·폴더·튜토리얼 진행 | 앱 | `src/db.rs` |
| `memory.db` (+ `-wal`/`-shm`) | SQLite | 에이전트 메모리 (별도 스키마·연결) | 앱 | `crates/tasty-memory/` |
| `config.toml` | TOML | 사용자 설정(셸·외관·단축키·언어 등) | 사용자 | `crates/tasty-settings/` |
| `remote-profiles.toml` (+ `passkeys.toml`) | TOML | 원격 접속 프로필(`ssh`/`tasty-attach` kind) + 자격증명 — `config.toml` 과 분리해 손편집 보존 | 사용자 | `crates/tasty-remote-profiles/` |
| `file-handlers.toml` | TOML | 파일 detector / handler / 확장자 매핑 | 사용자 | `crates/tasty-file-handler/src/` |
| `themes/<id>.toml` | TOML | 테마 (id = 파일명 stem) | 사용자 / 앱 | `crates/tasty-themes/src/store.rs` |
| `bashrc.default` | 쉘 스크립트 | 컴파일된 빌트인 rc(빌트인 앞부분 + 사용자 셸 rc + 빌트인 프롬프트 훅) — bash를 `--rcfile` 로 띄움 | 앱 (빌드 산출물) | `crates/tasty-settings/src/general.rs` |
| `presets/{workspace,tab,pane}/<name>.toml` | TOML | 레이아웃 프리셋 (탭/페인/서피스 구조) | 사용자 / 앱 | `crates/tasty-presets/` |

- **plugin 데이터는 여기 없다.** 각 plugin 은 자기 `TASTY_PLUGIN_DATA_DIR` 아래에 보관한다. host `state.db` 에 plugin 데이터를 넣지 않는다.

## 테마 파일 준비와 runtime fallback

첫 로딩 프레임 전에 `ThemeFiles`가 내장 테마를 준비한다. 같은 디렉터리 임시 파일·참여 잠금·읽은 원본과 링크 대상 비교를 사용하며 기존 mode/ACL을 보존한다. 읽기 실패나 잘못된 UTF-8 원본은 덮어쓰지 않는다. 읽을 수 있는 내장 파일의 drift·구문 오류는 기존 내장 동기화 정책대로 복구한다. TOML이 없는 초기 폴더는 두 내장 테마를 seed하고, 다른 테마가 남은 폴더에서 삭제한 Latte는 다시 만들지 않는다. 별도 사용자 테마 파일은 바꾸지 않는다.

실행 중 적용과 추가 창은 캐시된 색상 또는 내장 Mocha만 사용하며 디스크 복구를 실행하지 않는다. 명시 rescan은 읽기 전용이다. bootstrap 준비는 완료 barrier라 동기 호출이고, 파일 저장을 접수만 한 채 첫 렌더로 진행하는 비동기 저장으로 해석하지 않는다. 여러 테마 파일 사이 transaction이나 외부 비참여자의 최종 확인–교체 race 방지는 보장하지 않는다.

## IPC 포트 발견의 완료 경계

IPC는 listener 준비와 notify 정리 뒤 `tasty.port`를 공용 안전 교체로 완전히 공개하고 시작 성공을 반환한다. 접수만 한 저장을 발견 성공으로 취급하지 않는다. 새 Unix 파일은 0600이며 기존 mode/ACL과 살아 있는 symlink는 공용 교체 정책을 따른다. 읽을 수 없거나 관측 뒤 바뀐 원본은 덮어쓰지 않고 시작 실패로 돌려준다. accept worker 시작 실패는 방금 공개한 포트를 정리한다.

listener는 열린 파일 handle로 공개 세대의 OS identity를 보관한다. Drop은 같은 참여 잠금을 한 번 비차단 시도하며 경합이면 파일을 남긴다. 잠금을 얻은 동안 대상·identity·정확한 숫자 내용이 자신의 공개와 같을 때만 삭제한다. 같은 숫자로 재공개해도 이전 세대는 새 파일을 지우지 않는다. identity를 확인할 수 없으면 공개 성공은 유지하되 그 결과로 삭제하지 않는다. 잠금 참여자의 교체와 cleanup은 직렬화한다. 마지막 확인과 unlink 사이 비참여 외부 변경을 막는 원자적 조건부 삭제는 아니다. instance lock과 launch.log 프로토콜은 별개이며 그대로 유지한다.

## 구조 journal과 복원 원본

로컬 engine/workspace/pane/tab/surface 구조는 데이터 홈의 단일 journal이 원본이다. 각 engine은 별도 stream을 사용한다. `JournalModel`과 domain snapshot은 확정 이벤트에서 도출하며, CoreState는 실행 객체를 소유하지 않는 읽기 projection이다. terminal·plugin 콘텐츠 payload와 View 선택 checkpoint는 별도 원본이다. mirror 트리는 로컬 구조 journal에 편입하지 않는다.

App worker가 commit한 batch를 모든 대상 엔진에 공개하고 ACK를 받은 뒤 응답·checkpoint를 진행한다. 연속해서 준비된 순수 구조 요청은 물리 transaction을 공유하되 command·논리 batch cut·최초 응답은 각각 유지한다. 같은 stream의 뒤 판단은 앞 staged 결과를 보고, 공개·ACK·완료는 원 요청 순서를 따른다. capture의 memory 대기·ID 예약·claim·외부 실행과 복구·정비는 수집 barrier다. `state.db`와 `memory.db`의 SQLite 설정이나 복구 정책을 journal에 그대로 적용하지 않는다. journal에는 독점 writer fencing, 원자 명령·effect 기록, payload pin·GC와 별도 저장 예산이 있다. 세부 계약은 [레이아웃 저장](../../features/layout-persistence/index.md)과 [이벤트 저장소 (`tasty-event-store`)](../../architecture/event-store.md)을 따른다.

## SQLite `state.db`

### 단일 schema 모델

증분 마이그레이션 체인은 **없다**(0.4 fresh-start 정책). `src/db/migrations.rs`:

- `SCHEMA_VERSION` 상수 하나. `ensure_schema()` 가 `PRAGMA user_version` 을 보고 분기:
  - `0`(새 DB) → `SCHEMA_SQL` 1회 적용 + `user_version` 을 `SCHEMA_VERSION`으로 설정.
  - `== SCHEMA_VERSION` → **additive ensure**. `CREATE TABLE IF NOT EXISTS` 만 다시 돌린다.
  - 그 외 → `SchemaMismatch{expected, found}` 에러. 호출자는 아래 초기화 실패 절처럼 경고를 남기고 DB 없이 계속한다.

기존 버전의 DB에도 `CREATE TABLE IF NOT EXISTS`로 테이블을 추가할 수 있다. `tutorial_progress`와 `recent_files`·`recent_file_order`가 이 방식을 사용한다(이 문서). 버전값만으로는 추가 여부를 알 수 없으므로 `an_existing_database_still_gets_the_tutorial_table`이 기존 DB에도 테이블이 생기는지 확인한다.

컬럼 변경이나 데이터 이동을 이 경로에 넣지는 않는다. 그런 마이그레이션이 필요하면 SCHEMA_VERSION 변경과 새 DB 시작 정책을 함께 검토한다.

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
CREATE TABLE recent_file_order ( -- 종류별 목록 순서(rank 0이 최신)
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    opened_at INTEGER NOT NULL,
    rank INTEGER NOT NULL,
    PRIMARY KEY(kind, path)
);
```

- `recent_files`에 현재 목록을 기록한다(`src/store/recent_files.rs`). `recent_markdown`은 레거시 호환 테이블이며 최초 로드 시 한 번만 이관한다.
- 파일 종류는 `kind`로 구분한다. 탐색기의 최근 방문 폴더도 `directory` kind로 같은 테이블을 쓴다.
- **목록 순서**: `opened_at`은 연 시각(초) 그대로다. 순서는 저장할 때 같은 transaction에서 `recent_file_order`에 rank로 기록한다. 부팅 읽기는 그 종류의 모든 행에 경로와 `opened_at`이 일치하는 rank가 있을 때만 rank 순서를 쓰고, 하나라도 없으면 `opened_at`·rowid 최신순으로 읽는다. 순서 정보가 없는 기존 DB와, 이 표를 모르는 이전 바이너리가 `recent_files`를 바꾼 경우가 뒤쪽에 해당한다. 이런 경우도 그 종류의 다음 저장이 rank를 새로 기록한다. 이 표는 additive ensure로 기존 DB에 추가되며 `user_version`은 바꾸지 않는다.
- **경로 dedup**: 같은 파일의 다른 표기(구분자 `\`↔`/`, `\\?\` verbatim, `.`/`..`,
  Windows 대소문자 차)를 정규화 키(`strip_verbatim_prefix`+`lexically_normalize`+Windows
  case fold)로 접는다. PK 는 여전히 raw path(표시·열기용)이며 정규화 키는 비교 전용.
  저장은 그 종류의 목록 전체를 다시 쓰므로 같은 키의 옛 행이 남지 않는다. 마이그레이션 체인이
  없는 fresh-start 정책이라 GUI 부팅의 한 번 읽기가 같은 transaction 안에서 레거시 이관과
  정규화 dedup으로 기존 중복을 접는다. 상한 밖의 옛 행은 그 종류의 첫 저장이 정리한다.
- **기록 진입점**: markdown-open 이 수렴하는 인텐트 계층(`Intent::NewTab`/
  `ConvertSurface`, file-dispatch 직접 `CreateTab`)에서 `MainViewState::record_recent`
  로 1회 기록한다 — 파일-열기 팝업·주소창 navigate·링크 클릭이 모두 반영된다.

### 최근 목록의 창 간 일관성

최근 파일 캐시는 GUI 부팅이 state.db에서 한 번 읽어 만들고 모든 창이 공유 핸들로 조회한다. 종류별 최신순·중복 제거·10개 상한을 유지한다. add는 같은 잠금 안에서 메모리를 갱신하고 그 종류의 목록 전체를 state.db writer에 접수만 하며 SQL과 결과를 기다리지 않는다. 그래서 창마다 캐시와 저장 접수 순서가 같고, 저장이 실패해도 화면 목록은 창마다 같다. 화면에 보인 목록(표시 revision)과 디스크가 확정한 목록(확정 revision)은 종류별로 따로 세며, 확정은 메인 루프가 결과를 거둘 때 commit 성공만 반영한다. 같은 초에 연 항목도 다시 켰을 때 목록 순서가 같다. 저장 규칙은 [저장 계약의 state.db 실행기](storage-contract.md#statedb-실행기)에 있다. 조회는 최대 10개 경로 스냅샷을 반환하며 매번 DB를 다시 읽지 않는다. DB 없는 테스트 캐시는 독립적이다. 다른 프로세스의 변경 감지는 보장하지 않으며 그런 요구가 생기면 재검토한다.

### 튜토리얼 진행

튜토리얼 진행은 state.db writer를 통해 state.db의 tutorial_progress에 주제·콘텐츠 개정 번호·완료 여부·재개 단계·행 버전을 저장한다. 스키마 검증을 통과한 DB에 중복 없이 테이블을 추가하고 버전 불일치 보호를 유지한다. 완료는 되돌리지 않고 재개 위치는 낙관적 버전 검사로 오래된 View의 덮어쓰기를 막는다. 런타임 객체 ID는 저장하지 않아 실습 재개 시 준비부터 시작한다. 설정·레이아웃과 진행 기록을 섞지 않는다. 외부 콘텐츠 기여·정식 migration·객체 복원 요구가 생기면 소유권과 revision을 검토한다.

### 접근 규칙

state.db는 GUI 부팅만 열고, 연결은 state.db writer의 worker만 가진다. 다른 스레드는 공유 최근 목록 캐시(`db::shared_recent_files`)와 요청 큐(`db::shared_queue`)만 받는다. None은 현재 열려 있지 않다는 뜻이며 헤드리스인지 GUI 초기화 실패인지 원인을 구분하지 못한다. GUI 초기화가 실패해도 앱은 계속 실행되므로 빈 최근 목록만으로 실행 형태를 판단하지 않는다. DB가 없으면 `RecentFiles::load()`가 공유되지 않는 빈 캐시를 돌려주므로 recent.query는 빈 목록에서 시작하고, 이후에는 응답한 윈도우의 캐시에 이번 실행에서 들어간 항목을 돌려준다(아래 초기화 실패 절). state DB를 여는 범위나 초기화 오류 보존 방식이 바뀌면 이 의미를 다시 검토한다.

CLI는 DB를 직접 열지 않고 IPC로 호스트에 요청한다. state.db와 memory.db의 연결·스키마·실패 정책은 서로 다르다.

### 적용된 SQLite 설정

memory.db와 state.db는 공용 apply_connection_pragmas를 사용한다. pragma 호출이 성공해도 적용값은 다를 수 있어 값을 다시 읽는다. 파일 DB의 WAL과 in-memory의 memory journal은 각 모드의 정상값이다. 적용 실패만으로 열린 DB를 버리지 않는다. synchronous NORMAL은 전원 장애 때 최신 commit의 보존을 약속하지 않는다. 설정 상수만 공유하고 적용 함수를 복제하면 양쪽 구현이 갈라지므로 함수 자체를 공유한다.

AppliedPragmas는 journal_mode·synchronous·foreign_keys·journal_size_limit의 요청값, 실제값, 오류와 took를 보존한다. 하나라도 적용되지 않으면 열린 DB를 degraded로 표시하고 계속 사용한다. system.pressure.db_pragmas가 memory_db·state_db를 제공하며 미개방 DB는 null이다. 이 값은 프로세스 누계가 아니라 연결을 열 때의 상태다. 로그는 발생 시점, IPC는 상태 조회를 담당한다. 파일 journal의 memory 값을 정상으로 받아들이지 않는다.

`journal_size_limit`은 checkpoint 후 WAL을 재사용할 때 남길 크기의 제한이다. 활성 WAL은 긴 읽기 트랜잭션이나 큰 쓰기 중 이 값을 넘을 수 있다. 두 DB는 공용 `WAL_SIZE_LIMIT_BYTES`를 사용한다.

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
  지금 코드는 적용 여부만 본다([저장 계약과 writer 명부](storage-contract.md)).
- **전원 장애 쪽은 측정된 적이 없다.** 그 줄은 SQLite 문서의 계약이다. 재는 법은 commit 직후
  전원을 끊고 재시작해 마지막 commit 의 생존을 보는 것인데 — **이 레포에 그 장비는 없다.**
- **프로세스 kill 쪽은 쟀다**(2026-09-21, Linux, 격리 홈의 GUI debug 인스턴스): `memory.put` 다섯
  건이 응답한 직후 `SIGKILL`, 같은 홈으로 재시작해 `memory.get` — 다섯 건 전부 남아 있었다. 한
  번의 관측이지 확률의 측정은 아니다.

### 저장 실패의 의미 — `memory.db`

초기화와 쓰기 오류는 StorageFailure의 busy·disk_full·io·corrupt·permission_denied·other 분류를 공유한다.
IPC는 기존 오류 코드·문구를 유지하고 error.data.storage_failure를 추가한다.
NotFound·CAS 충돌·quota 거절은 저장장치 오류가 아니다.
실패한 commit은 quota 카운터와 pending_changes를 갱신하지 않으며 트랜잭션이 롤백된다.
현재 CANTOPEN은 permission_denied, READONLY는 other이고 초기화의 Io는 기존 안내 Other로 매핑된다.
이 값은 계약이므로 분류를 바꿀 때 소비자 호환을 검토한다.

fallback에서 memory.db를 쓰는 memory·agent·approval·surface.meta·telemetry·session 메서드의 성공 객체에는 durable:false를 추가한다. 표상 비Read라도 저장하지 않는 memory.export·agent.task_run·agent.task_reduce는 제외한다. 정상 저장소는 기존 응답을 유지하므로 필드 부재만으로 구 호스트와 정상 영속을 구분하지 못한다. dry-run에도 fallback 상태가 표시될 수 있고 Read로 분류한 조회의 부수 정리 쓰기에는 표시하지 않는다. 새 저장 이름공간은 명시적으로 목록과 검증에 추가한다.

### 터미널 출력 observer 의 memory sink — 저장 계약

`output.observe_start`의 memory sink는 파싱한 항목을 `memory.db`에 쓴다.
`src/runtime/output_observer/memory_sink.rs`가 채널에 쌓인 항목을 최대 256건씩 꺼내 [memory writer](storage-contract.md#저장-실행기)에 독립 요청 묶음(`Batch`, `Independent`) 하나로 넣는다. 함께 꺼낸 항목은 commit 한 번으로 저장된다. sink는 결과를 기다리지 않고 다음 항목을 받으며, 끝난 결과는 접수 순서대로 거둔다.

- **키**: `global` 범위에 `tasty.observer.<id>.<ms>.<seq>`로 저장하고 owner는 `_host`다. `<ms>`는 묶음을 만든 시각의 밀리초, `<seq>`는 sink별로 0부터 세는 순번이며 최소 6자리로 표시한다. 같은 밀리초의 여러 항목은 순번으로 구분한다. 시계가 역행하지 않고 순번이 6자리 범위에 있으면 키의 정렬 순서가 도착 순서와 같아 `memory.list --prefix tasty.observer.<id>.`로 시간순 조회할 수 있다(저장·보존 설계). 모든 항목이 자기 키를 받으므로 같은 키로 합쳐지는 기록은 없다.
- **`max_records`**: 저장이 확정된 키만 순서대로 기억하고 최근 N건을 넘으면 넘친 옛 키의 삭제를 다음 묶음에 싣는다. 채널이 끊기면 남은 삭제를 마지막 묶음으로 보낸다. 이미 없는 키는 조용히 넘어가고 그 밖의 삭제 실패는 경고만 남긴다. 삭제한 키는 다시 시도하지 않으므로 N건보다 많이 남을 수 있다. 확정 여부를 모르는 기록도 세지 않으므로 그만큼 더 남을 수 있다. 재시작하면 이 목록은 비워지며 이전 실행이 남긴 키에는 이 상한을 적용하지 않는다.
- **실패 집계**: 세 종류를 따로 센다. 채널이 가득 차 넣지 못한 항목은 `dropped`, writer가 접수하지 않은 항목(큐 포화·종료 중)은 `submit_rejected`, writer가 거절했거나(크기·quota) 확정하지 못했거나 확정 여부를 모르는 항목은 `store_failed`다. 세 값 모두 `output.observe_list`·`output.observe_info` 응답에 있다. 접수하지 않은 항목을 sink가 대신 저장하지 않으며, 실패한 항목은 경고를 남기고 버린다. 소비자에게 gap을 보내지 않는다.
- **잠금**: sink는 저장소 잠금을 잡지 않는다. 저장소에 닿는 것은 writer worker다. writer 없는 시험 구성에서는 같은 실행 규칙으로 sink 스레드에서 바로 실행하며, 이때 잠금 poison은 `tasty_utils::poison::recover_mutex`로 복구하고 `tasty_memory::STORE_LOCK_WHAT`·`STORE_LOCK_POISONED`를 쓴다. `core`의 `MEMORY_WHAT`·`MEMORY_POISONED`는 같은 static을 재수출한다.
- **종료**: surface가 닫히면 sender를 해제하고 워커 join은 미룬다. 워커는 채널에 남은 항목을 넣고 결과를 기다린 뒤(최대 30초) 종료한다. observer 워커가 정상적으로 끝났으면 그 결과를 회수한 뒤 writer를 drain한다. observer 시한을 넘겼으면 worker가 끝났다고 간주하지 않고 새 제출만 차단한 뒤 writer drain으로 진행한다. 앱 종료는 완료 worker만 폴링하며 5초 기한 뒤 미회수 수를 남긴다. 그때 memory writer에 새로 제출할 수 없도록 접수를 차단한다. 명시적 앱 종료를 시작한 라우터의 Drop은 다시 기다리지 않고 미회수 경고를 남긴다. 워커가 끝나기 전에 프로세스가 종료되면 남은 항목을 잃을 수 있다.

저장 실패 처리의 근거는 [저장 계약과 writer 명부](storage-contract.md)에 있다.

### 관측 로그 보존

관측 로그는 공용 정책 테이블에서 개수와 시간으로 정리한다.
audit는 Deny만 영속하고 5만건·50시간, telemetry raw event는 최근 2만건, anomaly는 5천건·50시간이다.
raw event의 별도 rollup은 구현하지 않아 조회 가능 기간은 이벤트 유입량에 따라 달라진다.
부팅·추가 저장·주기 타이머가 같은 정리 함수를 쓰며, 정리는 한 시간 간격으로 제한한다.
따라서 순간 행 수는 상한에 정리 사이 유입량이 더해질 수 있다.
Allow 미기록으로 정상 호출의 사후 행동 감사가 불가능한 대가를 수용한다.
사고 조사·장기 비용 조회·자동 이상 대응·로그 저장소 분리가 필요해지면 보존 정책을 재검토한다.

### 초기화 실패 — `state.db` 는 없이 계속, `memory.db` 는 in-memory 대체 + degraded

두 DB 모두 열지 못해도 앱을 종료하지 않는다. 둘 다 터미널 작업과 레이아웃 복원에 쓰이지 않으므로 [피드백과 복구 원칙](../../principles/feedback-and-recovery.md)의 "알림을 남기고 계속한다"를 따른다. 원인 분류는 같은 표를 쓰지만 실패한 뒤에 쓰는 저장소가 다르다.

#### `state.db` — 인메모리 폴백 없이 계속

`db::init()`은 GUI 부팅에서 실행당 한 번만 시도한다. 실패하면 `:memory:` 폴백 없이 DB를 열지 않은 상태로 계속한다. 같은 실행에서 새 창을 열어도 다시 시도하지 않으므로 이미 열린 창에 영향이 없고 안내도 쌓이지 않는다. 다시 열려면 원인을 해소하고 Tasty를 다시 실행한다. 원본 파일은 열기만 시도하며 지우거나 새 파일로 덮지 않는다.

실패는 첫 engine의 알림 목록에 경고 하나(`storage_notice.state_db_title`·`storage_notice.state_db_body`)로 남고 같은 내용이 warn 로그(`state.db init failed: …`)에 남는다. 본문은 아래 원인 문구와 영향(이전에 저장한 최근 목록·튜토리얼 진행을 불러오지 못하고 이번 실행의 변경을 저장하지 않음)을 함께 싣는다. 이번 실행의 변경이 화면에 보이지 않는다고 단정하지 않는다. writer를 시작하지 못한 경우도 같은 경고로 다룬다. 이후 소비자는 공유 핸들의 None을 받아 다음처럼 동작한다.

- 최근 파일·Markdown 목록: 저장된 목록을 불러오지 못해 빈 목록으로 시작한다. DB가 없으면 `RecentFiles::load()`가 윈도우마다 공유되지 않는 빈 캐시를 만든다. `RecentFiles::add`는 캐시를 갱신하고 저장 접수만 건너뛴다. 그래서 사용자가 Explorer로 이동한 폴더(`directory` 종류)는 그 윈도우의 Explorer 최근 폴더 목록과 그 윈도우가 응답한 `recent.query`에 실행 중 나타난다. `records_recent` 종류(Markdown 등)의 파일 열기는 그때 새로 만든 캐시에 기록하므로 어느 목록에도 나타나지 않는다. 어느 쪽도 다음 실행에는 남지 않는다.
- 튜토리얼: 진행은 이번 실행의 메모리에만 있고, 목록에 기존 저장 실패 문구(`tutorial.save_error`)가 보인다.
- `system.pressure.db_pragmas.state_db`·`system.pressure.state_writer`·`system.pressure.state_db`: `null`.

원인 variant 별 i18n key 는 다음과 같다:

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

대체 상태는 다음과 같이 알린다:

- `system.pressure` 의 `db_pragmas.memory_db` 가 `degraded: true` · `in_memory: true` 이고
  `init_failure: {cause, error}` 가 원인을 싣는다. `cause` 는 `MemoryInitError::cause()` 의 이름
  (`home_missing` · `permission_denied` · `busy` · `disk_full` · `corrupt` · `schema_mismatch` ·
  `other`)이다. 정상이면 `init_failure` 는 `null` 이다.
- `memory.db` 에 쓰는 IPC 쓰기 계열(`memory.*` · `agent.*` · `approval.*` · `surface.meta.*` ·
  `telemetry.*` · `session.*`)의 성공 응답이 `durable: false` 를 더한다(위 "저장 실패의 의미").
- 로그에 `memory.db falls back to an in-memory store (cause=…) — writes will not survive a
  restart` 가 한 줄 더 남는다.
- 첫 engine이 준비되면 그 알림 목록에 경고 하나(`storage_notice.memory_fallback_title`·`storage_notice.memory_fallback_body`)를 남긴다. 본문은 원인(`init_failure.error`와 같은 문구)과 영향(이후 저장한 memory·agent 작업·승인·surface 메타데이터·텔레메트리·세션이 종료하면 사라짐)을 싣는다. 대체 사실은 부팅 초기에 `AppServices`가 보관하므로 알림 저장소가 생기기 전의 사실을 잃지 않는다. GUI는 첫 창의 부팅 완료, headless는 첫 engine 준비 직후 한 번 남긴다. InfoModal·toast·우측 상단 notice는 띄우지 않는다(복원 데이터가 아니므로 경고 수준이다).
- 대체 저장소마저 열지 못하면 `AppServices` 구성이 기본 원인(`other`, "no memory store was handed over at boot")으로 한 번 더 대체를 시도하고, 그 원인이 알림에 실린다. 첫 원인은 로그(`in-memory fallback for memory.db failed to open`)에만 남는다. 두 번째 시도도 실패하면 부팅이 실패한다.

근거·대안(fatal · 쓰기 거절)·재검토 조건은
[저장 계약과 writer 명부](storage-contract.md).

## 저장 기준선

현재 저장 경로의 commit 수와 지연, 종료 시간을 잰 값이다. 소비자별 계약과 writer 명부는 [저장 계약과 writer 명부](storage-contract.md)에 있다.

### 엔진 harness

`crates/tasty-memory/examples/commit_baseline.rs`는 실제 파일 `memory.db`를 열고 직접 잠금 대조군과 writer가 `Arc<Mutex<MemoryStore>>`를 공유한 채 시나리오를 실행한다. 공유 구성은 `test-support` 기능이 있어야 컴파일된다. 결과는 시나리오마다 JSON 한 줄로 인자로 받은 결과 파일에 쓴다. 표준 출력에는 쓰지 않는다.

```bash
cargo build --release -p tasty-memory --example commit_baseline --features test-support
target/release/examples/commit_baseline <빈 디렉터리> <결과.jsonl>
```

| 시나리오 | 내용 |
|---|---|
| `setup` | 키 1000개를 미리 넣고 되읽은 PRAGMA와 payload 크기를 기록한다 |
| `single` | 5ms 간격의 고립된 put 200건 |
| `burst100_concurrent` | 100개 스레드가 장벽에서 동시에 출발해 put 한 번씩. 잠금 대기를 따로 잰다 |
| `burst100_sequential` | 한 스레드가 put 100건을 연달아 실행 |
| `inflow` | 3초 동안 2ms 간격 put. 대리 메인 루프가 1ms마다 잠금을 잡고 get을 한다 |
| `slow_burst100_concurrent`·`slow_inflow` | put마다 잠금을 쥔 채 5ms를 더한 느린 저장 대조군 |
| `maintenance_paths` | TTL 정리·prune·scope 정리를 실행하고 호출마다 늘어난 commit 계측을 본다 |
| `close` | 앞 시나리오의 쓰기를 모두 받은 `fast.db` 연결을 drop하는 시간. 파일의 마지막 연결이라 WAL checkpoint가 포함된다 |
| `batch100_independent`·`batch100_atomic`·`unit100` | 같은 파일을 다시 열어 100건을 `write_batch`(독립·원자) 또는 `run_unit` 하나로 쓰는 회차를 20번 반복한다 |
| `close_reopened` | 같은 파일을 다시 열어 batch·실행 단위와 `maintenance_paths`를 실행한 연결을 drop하는 시간 |
| `writer_burst100_concurrent`·`writer_inflow` | 따로 연 `writer.db`에서 burst100 동시와 inflow의 쓰기를 memory writer에 접수한다. 수집 창(`max_delay`) 1·2·5ms마다 실행한다. 대리 메인 루프는 같은 저장소 잠금을 직접 잡는다 |
| `writer_gate_inflow` | `writer_inflow`와 같은 쓰기 유입 중에 대리 메인 루프가 1ms마다 결과를 기다리지 않고 확인만 한다. 진행 중인 게이트가 없으면 세션 판정 대리(읽기 실행 단위)를 넣고, 끝나면 기록 대리(읽기와 쓰기 실행 단위)를 넣는다 |
| `writer_slow_*` | 같은 부하를 `writer_slow.db`에서 묶음(commit)마다 잠금을 쥔 채 5ms를 더해 실행한다 |

2026-10-12 Linux(ext4, NVMe, 다른 빌드가 함께 도는 공유 컴퓨터)에서 3회 실행한 값이다. close 행은 같은 날 별도로 3회 실행한 값이다. 되읽은 설정은 WAL·NORMAL·foreign_keys ON·journal_size_limit 4096000이고 degraded가 아니었다. payload는 236바이트 JSON, owner는 `_host`, scope는 global이다.

| 시나리오 | commit 수 | 지연(3회 범위) |
|---|---|---|
| single 200 | 200 | p50 129~170µs, p95 201~216µs |
| burst100 동시 | 100 | 전체 2.1~14.2ms, 요청 p50 0.18~6.4ms. commit 시간 합은 1.8~2.4ms이고 나머지는 잠금 대기다 |
| burst100 연속 | 100 | 전체 5.5~12.4ms |
| inflow | 쓰기 수와 같음(1234~1349) | 대리 루프의 get(잠금 대기 포함) p95 19~85µs |
| 느린 저장 burst100 동시 | 100 | 전체 531~555ms, 요청 p50 269~288ms |
| 느린 저장 inflow | 쓰기 수와 같음(380~394) | 대리 루프의 get p95 5.8~6.2ms |
| close(쓰기를 모두 받은 연결, drop 직전 WAL 4.10~4.14MB) | - | 1.1~13.8ms |

- 모든 시나리오에서 요청 하나가 commit 하나다. 느린 저장 대조군은 저장 지연이 같은 잠금을 쓰는 다른 호출자에게 그대로 전해짐을 보인다.
- strace로 1회 센 결과 전체 실행의 commit 약 3,800회에 `fsync`는 68회였다(`fdatasync` 0회). WAL·NORMAL은 commit마다 fsync하지 않으므로 commit 수와 fsync 수를 따로 센다.
- 지연의 절댓값은 컴퓨터 부하에 따라 크게 달라진다. 다른 측정과 비교할 때는 같은 컴퓨터에서 같은 시점에 번갈아 실행한 값을 쓴다.

실행 단위·쓰기 묶음을 도입한 뒤 같은 날 같은 컴퓨터에서 도입 전 harness와 도입 후 harness를 3회씩 번갈아 실행했다. 단건 시나리오는 같은 코드 경로의 전후 비교이고, batch 시나리오는 도입 후에만 있다.

| 시나리오 | 도입 전 | 도입 후 |
|---|---|---|
| single 200: commit 수, p50, p95 | 200, 141~159µs, 180~257µs | 200, 137~147µs, 170~173µs |
| burst100 연속: commit 수, 전체 | 100, 6.4~23.1ms | 100, 4.3~378.8ms(최댓값 회차는 commit 시간 합이 377ms) |
| 100건 묶음 20회: commit 수, 회차 p50 | - | 독립 20, 0.74~1.38ms · 원자 20, 1.35~1.40ms · 실행 단위 20, 0.67~1.42ms |

- 100건을 묶으면 commit이 100회에서 1회가 된다. 도입 후 3회에서 묶음 회차의 p50(0.67~1.42ms)은 단건 100회 연속 전체의 최솟값(4.3ms)보다 짧았다. 회차의 최댓값(4.6~17.9ms)은 그렇지 않았다. 단건 연속 전체가 4.3ms였던 회에서 독립·원자·실행 단위 회차의 최댓값은 각각 5.1·16.4·6.4ms였다.
- burst100 연속의 큰 편차는 commit 자체의 시간이다(commit 시간 합이 전체와 거의 같다). WAL이 자동 checkpoint 크기에 닿는 회차와 공유 컴퓨터의 부하가 겹친 것으로 보이나 원인은 확인하지 않았다.
- 실행 순서를 바꾼 추가 2회에서 같은 방식의 회차 p50이 0.69~1.37ms로 겹쳐, 독립·원자 묶음과 실행 단위 사이의 차이는 이 측정으로 구분되지 않는다.

### 저장 실행기 측정

memory writer를 도입한 뒤 2026-10-12 같은 컴퓨터(20코어, 1분 부하 평균 3.8~10.4)에서 도입 전 harness(직접 잠금 시나리오만)와 도입 후 harness를 3회씩 번갈아 실행했다. writer는 호스트처럼 `SharedStore`로 저장소에 닿고 대리 메인 루프는 같은 잠금을 직접 잡는다. `submit`은 호출 스레드가 접수에서 돌아오기까지, `completion`은 접수부터 결과가 준비되기까지의 시간이다. 값은 3회의 범위다.

직접 잠금 시나리오는 두 harness에서 같은 코드다. 전후 범위가 겹쳐 writer 코드가 직접 경로를 느리게 하지 않았음을 보인다.

| 직접 잠금 시나리오 | 도입 전 | 도입 후 |
|---|---|---|
| burst100 동시: 전체, 요청 p99 | 8.2~14.1ms, 7.5~9.7ms | 7.8~9.5ms, 7.4~8.7ms |
| 느린 저장 burst100 동시: 전체, 요청 p50 | 530~583ms, 270~295ms | 524~536ms, 270~273ms |
| inflow: 쓰기 수, 대리 루프 get p99 | 1223~1341, 93~526µs | 1220~1361, 151~245µs |
| 느린 저장 inflow: 쓰기 수, 쓰기 p50, 대리 루프 get p95 | 383~406, 5.3ms, 5.7~6.0ms | 380~406, 5.2~5.3ms, 5.6~6.3ms |

| writer 시나리오(수집 창) | commit 수 | submit p99 | completion p50 · p99 | 대리 루프 get p95 · p99 | 최대 잡힌 항목 · 바이트 |
|---|---|---|---|---|---|
| burst100 동시(1ms) | 1~3 | 8~35µs | 1.8~2.8ms · 2.0~3.1ms | - | 94~100 · 23,970~25,500 |
| burst100 동시(2ms) | 1 | 9~43µs | 3.0~4.0ms · 3.4~6.5ms | - | 100 · 25,500 |
| burst100 동시(5ms) | 1 | 7~53µs | 6.7~7.0ms · 7.2~9.8ms | - | 100 · 25,500 |
| inflow(1ms), 쓰기 1316~1421 | 1251~1408 | 25~295µs | 1.2~1.3ms · 2.5~11.3ms | 0.1ms 미만 · 0.1~0.3ms | 4~8 · 768~2,048 |
| inflow(2ms), 쓰기 1328~1439 | 926~1043 | 11~26µs | 2.2ms · 5.3~15.0ms | 0.1ms 미만 · 0.1~0.2ms | 5~11 · 1,280~2,816 |
| inflow(5ms), 쓰기 1303~1440 | 453~479 | 19~703µs | 3.2ms · 5.6~15.5ms | 0.1ms 미만 · 0.1~0.3ms | 7~11 · 1,792~2,560 |
| 느린 저장 burst100 동시(1·2·5ms) | 1~2 | 7~77µs | 7.1~12.4ms · 8.2~13.9ms | - | 100 · 25,500 |
| 느린 저장 inflow(1ms), 쓰기 1310~1437 | 515~563 | 19~22µs | 8.1~8.8ms · 11.3~26.0ms | 26~125ms · 42~358ms | 8~17 · 2,048~4,352 |
| 느린 저장 inflow(2ms), 쓰기 1321~1424 | 506~554 | 17~22µs | 8.3~8.7ms · 11.3~22.7ms | 21~226ms · 29~376ms | 8~14 · 2,048~3,584 |
| 느린 저장 inflow(5ms), 쓰기 1318~1411 | 448~469 | 21~33µs | 8.4~8.8ms · 11.6~22.4ms | 5.6~12.2ms · 5.9~19.6ms | 8~16 · 2,048~4,096 |

- 호출 스레드는 접수에서 수십 µs 안에 돌아온다(p99 최댓값 703µs, 단일 최댓값 3.3ms). 느린 저장에서도 같다. 직접 잠금의 느린 저장 inflow는 쓰기마다 호출자가 5.3ms를 기다려 3초에 380~406건만 썼고, writer는 같은 시간에 1310~1437건을 접수했다.
- 동시 burst100은 commit 100회가 1회가 되고 완료 p99가 직접 잠금의 요청 p99(7.4~9.7ms)보다 짧거나 비슷하다. 느린 저장에서는 직접 잠금 요청 p50 270~295ms가 완료 p99 8~14ms로 줄었다.
- 2ms 간격 유입에서 commit은 수집 창 1ms에서 거의 줄지 않고 2ms에서 25~30%, 5ms에서 65~67% 줄었다(1ms는 0.4~4.9%). 완료 p50은 대략 수집 창 길이만큼 늘어난다. 느린 저장 유입은 묶음 실행이 밀려 수집 창과 관계없이 61~67% 줄었다.
- 느린 저장에서 수집 창이 1·2ms이면 같은 잠금을 직접 잡는 대리 루프의 get이 크게 늦었다(p99 최대 376ms, 최댓값 565ms). 묶음 실행이 밀려 worker가 잠금을 풀자마자 다시 잡는 것으로 추정하며 원인은 확인하지 않았다. writer와 직접 잠금 소비자가 같은 잠금을 나누던 구성에서 나타났다. 제품 구성은 이제 writer가 저장소를 소유하며, harness는 그 비교를 위해 시험 구성(`WriterStore::shared`)으로 같은 잠금을 나눈다. 소비자를 옮긴 뒤 다시 잰 값은 아래 [직접 잠금 읽기 측정](#직접-잠금-읽기-측정)에 있다.
- 측정 중 예산 거절과 busy 재시도는 0건이었다. 최대 잡힌 양은 100건·25,500바이트로 기본 예산(4096건·16MiB)과 묶음 한도(256건·1MiB)에 닿지 않았다. 이 한도들은 측정으로 고른 값이 아니라 상한이다.
- 종료(`begin_shutdown`부터 join까지)는 36회 중 35회가 1.1~3.3ms였고, 느린 저장 inflow(1ms) 1회는 63µs였다(harness의 1ms 확인 간격 포함).

#### 직접 잠금 읽기 측정

IPC 응답 경로를 writer로 옮긴 뒤에도 메인 루프에 공유 잠금을 직접 잡는 읽기가 남아 있던 때(요청 게이트의 세션·cap·rate limit 판정, nickname 대상 찾기) 2026-10-12 같은 harness를 3회 실행해(실행 직후 1분 부하 평균 2.96~3.21) writer가 쓰는 동안 대리 루프의 직접 get과 writer 요청의 완료 시간을 비교했다. 값은 3회의 범위다.

| 시나리오(수집 창) | 대리 루프 직접 get p50 · p95 · p99 · 최댓값 | writer completion p50 · p99 |
|---|---|---|
| inflow(1·2·5ms) | 6~12µs · 26~39µs · 115~231µs · 16.9~28.9ms | 1.2~3.3ms · 12.5~15.3ms |
| 느린 저장 inflow(1ms) | 4.4~36.0ms · 121~621ms · 367~926ms · 404~926ms | 8.1~8.2ms · 20.8~23.6ms |
| 느린 저장 inflow(2ms) | 4.3~4.4ms · 201~447ms · 272~664ms · 293~664ms | 8.1~8.2ms · 20.2~21.0ms |
| 느린 저장 inflow(5ms) | 5.1ms · 5.4ms · 5.6~5.7ms · 37.1~38.3ms | 8.4ms · 20.7~22.8ms |

- 느린 저장에서 수집 창이 1·2ms이면 직접 잠금 읽기가 수백 ms를 기다린다. 같은 부하에서 writer에 넣은 요청의 완료 p99는 20.2~23.6ms였다. 직접 읽기를 남기면 writer 도입 전보다 메인 루프 대기가 길어질 수 있다.
- 빠른 저장에서는 직접 읽기 p99가 0.3ms 미만이다.

요청 게이트는 이제 writer 결과를 기다리지 않고 회차마다 확인한다([요청 게이트](storage-contract.md#요청-게이트)). harness의 `gate_inflow`는 같은 유입 중에 대리 루프가 1ms마다 접수·확인만 하고, 세션 판정 대리(G: 읽기 실행 단위)가 끝나면 기록 대리(R: 읽기와 쓰기 실행 단위)를 넣는다. 2026-10-21 같은 컴퓨터에서 3회 실행했다(1분 부하 평균 0.80~1.19). 같은 실행의 직접 get을 함께 적는다. 값은 3회의 범위다.

| 시나리오(수집 창) | 직접 get p99 | 게이트 대리 루프 회차 p99 · 최댓값 | G 완료 p50 · p99 | G+R 완료 p50 · p99 |
|---|---|---|---|---|
| inflow(1ms) | 92~179µs | 4~5µs · 10~13µs | 1.0ms · 2.0~2.1ms | 3.0ms · 7.0ms |
| inflow(2ms) | 88~176µs | 3~6µs · 10~17µs | 1.0ms · 3.0ms | 3.0ms · 7.0ms |
| inflow(5ms) | 124~131µs | 5µs · 10µs | 5.0ms · 6.1~8.0ms | 11.0ms · 12.0~13.0ms |
| 느린 저장 inflow(1ms) | 1.05~2.06s | 3~4µs · 8~11µs | 9.0~10.0ms · 11~18ms | 20.0ms · 26~28ms |
| 느린 저장 inflow(2ms) | 99~906ms | 1~3µs · 6~8µs | 10.0ms · 11~15ms | 20.0~21.0ms · 25~26ms |
| 느린 저장 inflow(5ms) | 5.8~11.2ms | 4~7µs · 7~12µs | 11.0ms · 11.0~15.0ms | 18.0ms · 24~27ms |

- 메인 루프 대리는 느린 저장에서도 회차마다 7µs 이하를 쓴다. 직접 get은 같은 부하에서 최대 2초를 기다렸다.
- 대신 세션 토큰 요청 하나가 writer 왕복 두 번을 기다린다. 빠른 저장의 기본 수집 창에서 p99 7ms, 느린 저장에서 25~28ms다. 직접 읽기는 빠른 저장에서 0.2ms 미만이었으므로 요청 하나의 지연은 늘었다.
- 완료 시간은 대리 루프의 1ms 확인 간격으로 잰 값이다. 호스트는 writer가 묶음을 끝낼 때 메인 루프를 깨운다([깨우기 묶음](#깨우기-묶음)).

#### 깨우기 묶음

worker는 묶음·flush마다 메인 루프를 깨운다. 2ms 간격 유입이면 묶음 수만큼 깨우기가 생긴다. 2026-10-12 harness에 호스트와 같은 `WakeLatch`와 대리 메인 루프(깨어날 때마다 확인한 뒤 회차 시간만큼 쉰다)를 넣어 3초 유입의 알림 수와 실제 깨우기 수를 1회씩 쟀다(1분 부하 평균 6.3~11.8).

| 대리 루프 회차 | inflow(1ms) 알림 → 깨우기 | inflow(2ms) | inflow(5ms) | 느린 저장 inflow(1·2·5ms) |
|---|---|---|---|---|
| 1ms | 1398 → 1397 | 1111 → 1109 | 479 → 479 | 551 → 551, 522 → 522, 474 → 474 |
| 8ms | 1382 → 368 | 1191 → 369 | 480 → 371 | 564 → 371, 559 → 370, 469 → 368 |

- 메인 루프가 묶음 간격보다 빠르면 깨우기는 묶음 수와 같다(2ms 창에서 초당 약 370회). 결과를 받아 응답을 보내려면 묶음마다 한 회차가 필요하므로 그보다 줄이지 않았다.
- 메인 루프가 느리면 확인 전에 끝난 묶음이 깨우기 하나에 합쳐져 깨우기 수가 회차 수로 묶인다(8ms 회차에서 초당 약 123회). 이벤트 큐에 깨우기가 밀려 쌓이지 않는다.
- 일정 간격보다 자주 깨우지 않는 방식(throttle)은 모든 응답에 그 간격만큼 지연을 더하므로 택하지 않았다.

### 앱 수준 측정

격리 `TASTY_HOME`으로 debug 바이너리를 띄우고 TCP IPC로 같은 payload의 `memory.put`을 보냈다. 시나리오마다 `system.pressure`의 차분을 기록하고 마지막에 `system.shutdown`(debug 전용) 요청부터 프로세스 종료까지를 쟀다. GUI는 전용 Xvfb에서, headless는 `--no-default-features` 바이너리로 각 3회 실행했다. 둘 다 최적화하지 않은 debug 빌드라 엔진 harness의 release 값과 직접 비교하지 않는다.

| 항목 | GUI | headless |
|---|---|---|
| put 1000건의 `db.commits` 증가 | 1001 | 1001 |
| single 200: commit 수, 요청 p50 | 200, 0.9~2.4ms | 200, 0.9~1.0ms |
| burst100 동시: commit 수, 전체 | 100, 26~31ms | 100, 65~89ms |
| burst100 동시: commit 시간 합, 핸들러 시간 합 | 1.5~5.8ms, 3.4~13.3ms | 2.5~2.7ms, 7.7~8.0ms |
| inflow 3초: commit 수 | 쓰기 수와 같음(351~577) | 쓰기 수와 같음(936~1004) |
| 쓰기 중 `memory.get` 왕복 p95 / 쓰기 없을 때 | 5.6~10.7ms / 1.5~12.6ms | 0.9~1.0ms / 1.0~1.2ms |
| `system.shutdown` 요청부터 프로세스 종료까지 | 118~281ms | 32~65ms |

- 앱에서도 요청 하나가 commit 하나다. 1000건에 1회가 더 늘어난 것은 같은 구간의 다른 memory.db 쓰기다.
- 요청 지연에서 commit이 차지하는 몫은 작다. GUI는 IPC 명령이 메인 루프에서 꺼내지기까지의 대기(`queue_before_gate`)가, headless의 동시 요청은 큐 대기 누계 최댓값(21~43ms)보다 큰 연결·읽기 단계 대기가 지연의 대부분이다.

### 계측 범위

memory·state·file writer의 공통 관측은 [저장 실행기 관측](storage-contract.md#저장-실행기-관측)에 있다. 입력 바이트, 묶음 대기·전체 실행·executor 호출의 고정 크기 분포, 종결 결과, 미완료 요청 수·바이트·최고 나이를 구분한다. `held_bytes`는 읽지 않은 결과까지 보관하는 예산이라 `pending_bytes`와 다르다. generic pending 나이는 소비자 dirty 상태나 SQL commit 시간이 아니다.

- `system.pressure.file_writer`는 config·즐겨찾기·파일/hook registry snapshot과 프리셋·원격 profile/passkey ordered command를 실행하는 같은 writer의 접수·묶음·대체·예산 통계다. `file_writes`의 replaced/failed/unknown은 설정·snapshot backend의 성공한 파일 교체·저장 실패·결과 불명 횟수이며 묶음 수와 다르다. 복합 명령의 여러 교체·삭제를 이 수에 한 번으로 넣지 않는다. `file_commands.presets`의 accepted/rejected/completed/unknown/pending은 명령·결과 수이고 completed에는 알려진 저장소 오류도 들어가므로 물리 교체·성공 수로 해석하지 않는다. GUI 부팅의 최종 설정 snapshot도 같은 writer에 제출하므로 이 통계에 포함한다. 아직 writer로 옮기지 않은 파일 소비자의 쓰기는 포함하지 않는다. writer가 없는 시험 구성은 null이다. 이 항목은 새 성능 기준선이나 실제 메인 루프 지연 측정을 뜻하지 않는다.

- `system.pressure.file_operations`는 file worker 실행 범위의 공용 교체·사용자 파일 삭제 helper를 replace/delete별로 센다. 작업 시도·성공·실패·불명, 제출/게시 바이트, 종료 시간 분포와 고정 오류 분류이며 부팅 load·CLI·다른 스레드 호출은 제외한다. 명령의 backend 결과 분류와 partial은 executed_commands로 따로 제공한다. 분류 없는 명령은 unclassified이며 명령 한 개를 물리 작업 한 개로 세지 않는다. 계측 경계와 미포함 작업은 [저장 계약](storage-contract.md#file-worker의-물리-파일-작업-계측)을 따른다.

- `system.pressure.file_snapshots`의 각 소비자는 visible/committed/pending과 함께 dirty/dirty_age_us를 제공한다. dirty age는 미확정 변경이 이어진 시간이며 실패로 pending이 0이 되어도 유지한다. 최신 revision의 성공을 수거해야 null이 된다. 조회는 결과를 소비하지 않으며 이 값으로 저장 완료나 전원 장애 내구성을 추정하지 않는다.

- `system.pressure.db`는 memory.db, `system.pressure.state_db`는 state.db writer의 commit을 같은 `DbLatencyStats` 형식(commit 수·시간 합·최댓값·평균·`writes_per_commit`)으로 센다. journal.db의 commit은 이 값에 없다.
- journal.db는 `EventStore::commit_statistics`와 `TASTY_LOG=tasty::journal_metrics=debug`의 별도 로그로 실제 성공 SQL COMMIT을 센다. physical은 초기화·epoch·pin·claim 등 보조 쓰기도 포함하고 structural은 구조 transaction, logical은 그 안의 확정 요청을 센다. 그룹 구간의 증분과 전체 누계, queue→durable-ready·publication 후 완료·SQL COMMIT 시간을 구분한다. 겹치는 요청별 구간을 합산하거나 이 값을 App 전체 루프 지연으로 해석하지 않는다([이벤트 저장소](../../architecture/event-store.md#쓰기-계측)).
- state.db의 commit 계측은 writer 묶음의 commit만 센다. 부팅의 한 번 읽기·스키마 생성과 transaction 없이 혼자 실행하는 튜토리얼 읽기는 세지 않는다. `writes_per_commit`은 대체된 목록을 뺀, 그 commit에서 실행한 요청 수다. 접수·묶음·대체·재시도·거절 수는 `system.pressure.state_writer`가 memory writer와 같은 형식으로 싣는다.
- commit 계측은 `put`·`delete`·`put_secret`·`delete_secret`·`purge_expired`·`purge_scope`·`prune_prefix_keep_recent`·`prune_prefix_older_than`의 명시 transaction을 센다. `import_regular`는 항목마다 `put`을 거치므로 세어진다. 정리 연산은 행을 하나 이상 지웠을 때만 센다.
- 실행 단위와 쓰기 묶음은 확정된 쓰기가 있으면 요청 수와 관계없이 commit 1회로 센다. 단위 안에서 실행한 정리 연산도 그 commit에 포함된다([저장 계약](storage-contract.md#실행-단위와-쓰기-묶음)).
- `vacuum_if_fragmented`의 VACUUM은 commit 수에 넣지 않고 `vacuums`·`vacuum_us_sum`으로 따로 센다. harness의 `maintenance_paths`는 정리 호출마다 늘어난 commit 수를 기록한다. 2026-10-12 debug 빌드 1회 실행에서 `purge_expired` 1행, prune 190행, `purge_scope` 1행 삭제가 각각 commit 1회로 세어졌다.
- commit 하나에 확정된 쓰기 수는 `writes_per_commit` 분포(구간 상한 1·4·16·64·256)로 센다. 단독 쓰기와 묶음을 구분할 수 있다.
- `queue_before_gate`와 `handler_after_gate`는 IPC 대기·처리 시간이다. debug GUI의 opt-in bounded 콜백 원시 표본과 writer별 첫 cut→실제 join/timeout 계측은 [종료 계측](../../architecture/shutdown-sequence.md#writer-종료-시간과-콜백-원시-표본)을 따른다. callback 비용·기존 timer wake 지연·렌더 비용·네트워크 RTT는 서로 대체하지 않는다.

## 텍스트 파일을 SQLite 로 옮기지 않는 이유

`config.toml` / `remote-profiles.toml` / `file-handlers.toml` / `themes/*.toml` 는 **사용자 편집·버전관리 대상**이다. 주석·diff 추적에 텍스트가 적합하고 앱이 자동으로 덮어쓰지 않으므로 SQLite 로 옮길 이점이 없다. 반대로 최근 파일처럼 앱이 자동 누적하는 데이터는 SQLite 가 맞다.

## 백업

`state.db` 는 WAL 덕분에 단일 프로세스 종료 시점에 일관성이 보장된다. 별도 주기 백업은 만들지 않는다. 수동 백업은 앱이 종료된 일관된 상태에서 `state.db`, `state.db-wal`, `state.db-shm` 세 파일을 함께 복사한다.

**해석하지 못한 사용자 파일은 덮어쓰기 전에 보존한다.** `config.toml` 과 `layouts/NN.json` 은 앱이 다시 쓰는 파일이라, 파싱에 실패한 뒤 기본값으로 폴백하면 다음 저장이 원본을 지운다. 그래서 **저장 직전에** 원본을 `<파일명>.bak`(중복이면 `.bak.2` … `.bak.9`)으로 **rename** 해 자리를 비운 뒤 쓴다. copy 가 아니라 rename 인 이유는 원본이 자리를 떠야 이어지는 write 가 데이터를 지우지 않기 때문이다.

`gc_scrollback_orphans_all_slots_in`은 참조 집합을 확정하지 못하면 그 회차의 scrollback GC를 건너뛴다. `layouts/` 목록을 읽지 못했거나 개별 항목 오류로 목록이 불완전할 수 있을 때, 열거된 슬롯 하나라도 읽거나 해석하지 못했을 때, 아직 이관되지 않은 옛 `layout.json`을 읽거나 해석하지 못했을 때가 해당한다. 참조를 알 수 없는 원본의 `.bin`을 지우지 않기 위한 처리다. 자세한 범위는 [레이아웃 영속화](../../features/layout-persistence/index.md#surface-내용-복원-현재-터미널-scrollback)를 참고한다.

**보존 시점은 로드가 아니라 저장이다.** 로드는 "해석하지 못했다" 는 사실만 값에 실어 돌려주고 파일은 그대로 둔다. 부팅 중 같은 파일을 읽는 곳이 여럿이고(설정은 런처와 GUI 가 각각, 레이아웃 슬롯은 scrollback GC 와 engine 이 각각) 그 둘은 별개 프로세스라, 읽는 쪽이 파일을 옮기면 나중에 읽는 쪽은 "파일 없음" 만 보게 된다 — 사용자에게 알릴 주체가 사건을 모르게 되고, 나중에 읽은 쪽이 잘못된 이유로 저장을 금지할 수도 있다.

보존이 실패했거나 **읽기가 실패한**(권한 · IO) 경우에는 파일을 건드리지 않고 그 대상에 대한 저장을 막는다. 내용을 확인하지 못한 파일을 옮기면 일시적 오류에도 사용자 데이터가 자리를 뜨기 때문이다. 공용 헬퍼는 `tasty_utils::path::preserve_corrupt_file`.

같은 `TASTY_HOME`을 공유하는 여러 인스턴스는 지원하지 않는다. 그래도 저장 직전에 파일을 다시 읽어 여전히 파싱할 수 없는지 확인한다. 부팅 이후 다른 인스턴스가 정상 파일을 썼을 수 있기 때문이다.

재확인과 rename 사이에는 잠금이 없어 그사이 정상 파일이 쓰이면 백업으로 옮겨질 수 있다. 지원하지 않는 공유 구성에서 남아 있는 경합이며 파일은 삭제되지 않고 백업 한 칸(최대 9개)을 사용한다.

## 테스트

- 스키마 로직: `:memory:` Connection 으로 단위 테스트(`src/db/migrations.rs` 의 `tests` — fresh init / 재호출이 버전을 안 바꿈 / additive ensure 가 기존 DB 에 닿음 / mismatch 두 갈래).
- 에러 분류: 표 자체는 `crates/tasty-memory/src/failure.rs` 의 `each_sqlite_code_lands_in_its_own_branch`,
  `state.db` 쪽 옮기기는 `classify_sql` 단위 테스트(busy / corrupt / notadb), `user_message_i18n` key 안정성 테스트.
- 저장 실패: `crates/tasty-memory/src/tests.rs` 가 잠금(`busy`)과 페이지 상한(`disk_full`)을 실제로
  일으켜 원인과 메모리 쪽 상태를 본다. `io` 는 합성 오류 코드로 표만 본다.
- 정리 연산: 같은 파일이 `BEFORE DELETE` trigger로 secret DELETE를 실패시켜 `purge_scope`·`purge_expired`가
  regular 삭제까지 되돌리고 카운터·변경 버퍼·commit 계측을 바꾸지 않는지 본다.
- 실행 단위·쓰기 묶음: `crates/tasty-memory/src/batch/batch_tests.rs` 가 묶음 중간 요청의 페이지 상한
  (`disk_full`), 다른 연결의 쓰기 잠금(`busy`)을 실제로 일으킨다. Unix에서는 파일 크기 상한(`ulimit -f`)을
  건 자식 프로세스에서 commit의 WAL 쓰기를 실패시켜 commit 단계 오류 뒤 카운터·변경 버퍼·commit 계측이
  그대로인지 본다. 정수가 아닌 version 행으로 단위 안 조회의 실제 저장 실패를, 단위 안의 직접 ROLLBACK으로
  SQLite가 transaction을 끝낸 상태를 만들어 앞선 쓰기까지 되돌리는지 본다. 시험용 저장소와 운영 저장소의
  묶음 판정 대조도 같은 파일에 있다.
- 적용값: `an_open_store_carries_the_pragmas_that_took_in_each_mode` ·
  `a_read_only_database_reports_itself_degraded` 가 모드별 허용 결과와 degraded 판정을 본다.

## 관련

- [storage-contract.md](storage-contract.md) — 저장 경로별 계약·writer 명부·종료 순서
- [memory.md](memory.md) — 에이전트 메모리(`memory.db`) 두 계층·소유 모델
- [`features/layout-presets`](../../features/layout-presets/index.md) — `presets/` 프리셋 적용
- [theme.md](theme.md) — `themes/*.toml` 토큰 모델

일반 세션 retirement의 Router Drop은 새 제출을 차단하거나 worker를 분리하지 않고 accepted 작업 완료를 기다린다. 명시적 앱 종료의 polling·timeout 이후 Drop과 구분한다. 저장 오류나 writer의 접수 거절을 성공으로 보장하지 않는다.
