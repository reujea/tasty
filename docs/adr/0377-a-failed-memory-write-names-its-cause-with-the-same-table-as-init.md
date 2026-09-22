# ADR-0377: 실패한 메모리 쓰기는 초기화와 같은 표로 원인을 말하고, 메모리 쪽 상태는 실패 전 그대로다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: sqlite, storage, memory, error-handling, ipc, compatibility
- **Group**: storage

## Context

SQLite 오류의 원인 분류(`Busy` · `DiskFull` · `Corrupt` · `PermissionDenied` · `Other`)는
**초기화 경로에만** 있었고, 그것도 두 벌이었다 — `memory.db` 의 `MemoryInitError` 와 본
바이너리 `state.db` 의 `DbInitError` 가 같은 `ErrorCode` 표를 글자 그대로 각자 들고 있었다.

저장 경로에는 분류가 없었다. `MemoryStore` 의 트랜잭션 쓰기 네 자리(`put` · `delete` ·
`put_secret` · `delete_secret`)는 실패하면 `MemoryError::Db(rusqlite::Error)` 를 그대로
올리고, IPC 는 그것을 `-32603 "memory db error: …"` 로 냈다. 호출자는 "다른 연결이 잠금을
쥐고 있었다" · "디스크가 찼다" · "I/O 가 거절됐다" · "파일이 깨졌다" 를 문장 파싱 없이는
고를 수 없었다. 처방은 넷이 다 다르다(재시도 · 공간 확보 · 디스크 점검 · 복구).

또 "저장 실패 뒤 메모리 쪽 상태가 무엇인가" 가 문서에 없었다. 코드는 quota 카운터
(`regular_used_bytes`)와 변경 버퍼(`pending_changes`)를 **commit 이 성공한 뒤에만**
움직이지만, 그것을 고정하는 시험도 적은 문서도 없었다.

## Decision

- **원인 표를 하나로 둔다** — `tasty_memory::StorageFailure`. `Busy` · `DiskFull` · `Io` ·
  `Corrupt` · `PermissionDenied` · `Other` 여섯 갈래이고, 두 초기화 오류 타입은 그 결과를 자기
  variant 로 옮기기만 한다. 새 갈래 `Io`(`SQLITE_IOERR`)는 초기화 안내에 따로 된 문구가 없어
  초기화 쪽에서는 전과 같이 `Other` 로 간다.
- **저장 경로가 같은 표를 쓴다** — `MemoryError::storage_failure()` 가 `Db` 오류를 분류한다.
  요청 거부(`NotFound` · `CasConflict` · quota 등)는 저장소가 멀쩡히 답한 것이라 `None` 이다.
- **IPC 는 코드와 문장을 그대로 두고 원인을 `error.data.storage_failure` 에 덧붙인다**
  (`busy` · `disk_full` · `io` · `corrupt` · `permission_denied` · `other`). 이 문자열은 계약이다.
- **실패한 쓰기는 메모리 쪽 상태를 안 옮긴다.** quota 카운터와 변경 버퍼는 commit 성공 뒤에만
  움직이고, 실패하면 트랜잭션은 롤백돼 디스크에도 남지 않는다. 실패한 commit 을 성공으로
  돌려주는 경로는 없다 — `commit()` 의 `Err` 가 그대로 호출자에게 간다.

## Consequences

- **얻은 것**: 에이전트가 `memory.*` 실패를 받아 재시도할지 멈출지를 `data` 한 필드로 가른다.
- **얻은 것**: 두 DB 의 원인 표가 갈릴 수 없다.
- **잃은 것**: `SQLITE_CANTOPEN` 이 저장 경로에서도 `permission_denied` 로 나간다. 초기화
  안내가 권한으로 묶어 온 것을 한 표로 합치며 그대로 가져왔다 — 쓰기 중 `CANTOPEN` 은 드물다.
- **잃은 것**: `SQLITE_READONLY`(읽기 전용 파일·마운트에 쓰기)는 표에 없어 `other` 로 나간다
  (실측: 읽기 전용 `memory.db` 에 `memory.put` → `storage_failure: "other"`). 표에 넣지 않은 것은
  표가 초기화와 공유되기 때문이다 — `permission_denied` 로 넣으면 초기화 안내의 분류가
  `Other` → `PermissionDenied` 로 바뀌는 동작 변경이 된다.
- **운영 비용**: `as_str` 문자열을 바꾸면 소비자가 깨진다. 이름을 바꾸지 말라는 말이 그 enum
  의 doc 에 있다.

## Alternatives Considered

- **A: `MemoryError` 에 `Busy` · `DiskFull` … variant 를 더한다** — 타입이 더 곧다. 안 골랐다:
  `MemoryError` 를 match 하는 소비자(IPC 매핑 · audit · session · agent 크레이트)가 전부
  바뀌고, 오류 문장이 달라져 기존 IPC 소비자가 깨진다. 분류를 메서드로 두면 기존 표면이 하나도
  안 바뀐다.
- **B: IPC 오류 코드를 원인마다 새로 나눈다** — 안 골랐다: `-32603` 으로 분기하던 소비자가
  깨진다. `data` 덧붙이기가 호환을 가장 많이 보존한다.
- **C: 초기화 표는 두 벌로 두고 저장 경로에만 새 표를 만든다** — 안 골랐다: 표가 셋이 된다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 표의 한 줄이 다른 갈래로 새면 `each_sqlite_code_lands_in_its_own_branch` 가 빨개진다.
- IPC 문장·코드가 바뀌거나 `data` 가 빠지면 `a_storage_failure_keeps_its_message_and_adds_its_cause`
  가 빨개진다.
- 실패한 쓰기가 카운터나 변경 버퍼를 옮기면
  `a_write_to_a_locked_database_fails_as_busy_and_leaves_the_cache_untouched` ·
  `a_write_past_the_page_ceiling_fails_as_disk_full_and_leaves_the_cache_untouched` 가 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **실제 I/O 오류가 이 표대로 분류되는가.** 시험은 잠금과 페이지 상한으로 `busy` ·
  `disk_full` 을 실제로 일으키지만, `io` 는 오류 코드를 합성해 표만 본다. 재는 법: 쓰기 중
  디스크를 떼거나 파일시스템을 읽기 전용으로 다시 마운트하고 `memory.put` 의
  `error.data.storage_failure` 를 본다.

## References

- 결정이 실현된 현재 위치: `crates/tasty-memory/src/failure.rs` 의 `StorageFailure`,
  `crates/tasty-memory/src/lib.rs` 의 `MemoryError::storage_failure` · `classify_sql`,
  `src/db.rs` 의 `classify_sql`, IPC 매핑은 `src/adapters/ipc/handler/memory.rs` 의 `map_error`
- 짝 결정: [ADR-0376](0376-a-database-that-opened-with-pragmas-that-did-not-take-is-degraded-not-fatal.md)
- 정본 문서: [storage](../design/systems/storage.md) · [memory](../design/systems/memory.md)
