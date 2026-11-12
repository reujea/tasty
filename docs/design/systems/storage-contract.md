# 저장 계약과 writer 명부

## 원칙과 현재 동작

[응답성 원칙](../../principles/performance-and-settings.md)에 따라 사용자 경로의 저장 대기를 줄인다. memory.db·state.db와 이행된 파일 소비자는 전담 writer에 요청을 넘기고 완료를 수거한다. 명부에 남아 있는 동기 파일 저장은 현재 한계이며 영구 예외로 승인된 것이 아니다.

호스트가 디스크에 쓰는 경로와 각 경로의 성공·오류·실행 계약을 정리한다.
파일 위치와 SQLite 설정, 실패 분류는 [저장소 시스템](storage.md)에, 측정값은 그 문서의 [저장 기준선](storage.md#저장-기준선)에 있다.

## 네 시점

저장 요청에는 네 시점이 있다. 현재 경로마다 이 시점이 어떻게 겹치는지 적는다.

- 접수: 호출자가 요청을 넘기고 다음 일을 시작할 수 있는 시점.
- commit 완료: SQLite transaction 또는 파일 쓰기가 끝난 시점. 전원 장애 내구성과는 다르다(storage.md의 `synchronous=NORMAL` 절).
- 메모리 표시: 다른 호출자나 화면이 새 값을 보는 시점.
- 외부 실행: 저장 결과에 따라 프로세스 실행·응답 전송 같은 되돌릴 수 없는 일이 일어나는 시점.

| 경로 | 접수 | commit 완료 | 메모리 표시 | 외부 실행 |
|---|---|---|---|---|
| `memory.*`·`memory.secret.*` IPC | 핸들러가 memory writer에 요청을 넣고 응답 continuation을 등록한 뒤 메인 루프가 다음 요청으로 넘어간다 | writer가 묶음으로 commit한다 | DB가 원본이므로 commit과 같다. `memory.changed`는 commit 뒤 변경 버퍼에 쌓였다가 GUI 이벤트 루프·headless 루프가 다음 회차에 발행한다 | 메인 루프가 회차마다 writer 결과를 거두어 원 호출자(IPC 연결·plugin 호출)에게 응답한다. 응답 시점에는 commit이 끝나 있다 |
| `approval.*`·`surface.meta.*` IPC | `memory.*`와 같다 | `memory.*`와 같다 | `memory.*`와 같다 | `memory.*`와 같다. `approval.request`·`respond`·`cancel`은 기록 결과를 기다려 응답하되 기록 실패는 경고만 남기고 성공으로 답한다. 승인 응답의 권한 부여(`apply_elevation_grant_if_any`)는 `approval.respond`와 GUI 승인 팝업 응답 모두 전이 직후 기다리지 않는 writer 요청으로 넣고, 그 뒤에 기록 요청을 넣는다. 둘 뒤에 접수된 요청(그 agent의 다음 요청 게이트 포함)은 부여가 확정됐다면 그 권한을 본다. 두 요청은 같은 commit으로 합쳐질 수 있어 같은 묶음의 저장 실패에 함께 되돌려질 수 있으며, 서로 다른 commit에 들어간다는 보장은 없다. 부여가 실패해도(세션 없음·저장 실패) 승인 응답은 바뀌지 않고 경고만 남는다. GUI 승인 팝업 응답과 cap 승인 요청의 기록은 결과를 기다리지 않고 실패만 로그로 남긴다 |
| `agent.barrier_*`·`agent.semaphore_*`·`agent.lease_*`·`agent.rate_limit_*` IPC | `memory.*`와 같다. 연산 하나가 writer 요청 하나다 | `memory.*`와 같다. 연산이 거절돼도 그 전에 쓴 내용은 남는다 | `memory.*`와 같다 | `memory.*`와 같다. barrier가 닫히면 `BarrierClosed` 이벤트는 commit 뒤 응답을 만들 때 넣는다 |
| `agent.task_*`·`agent.task_graph_*`·`agent.dag_*`·`agent.report_append`·`agent.task_submit` IPC | `memory.*`와 같다. 연산의 판단과 쓰기가 writer 복합 연산 하나다(`TaskTx`) | `memory.*`와 같다. 연산이 거절돼도 그 전에 쓴 내용은 남는다 | `memory.*`와 같다 | `memory.*`와 같다. 그래프 활성화·취소·완료 보고의 대기자 깨우기, 지켜보는 runner가 없는 취소의 정리 시작, 삭제한 task의 부속 키 정리 제출은 메인 루프가 commit된 결과를 거둔 뒤 응답 전에 한다. `agent.task_submit`의 턴 판정은 writer가 읽은 task로 그때 한다. `agent.task_reduce`의 reducer 실행은 writer 밖(응답을 만들 때)이다 |
| telemetry 조회·cap 관리(`telemetry.summary`·`timeseries`·`top`·`session_summary`·`anomaly.list`·`cap.*`)와 감사 조회(`plugin.audit_query`·`audit_summary`, GUI 감사 화면의 follow·clear) IPC | `memory.*`와 같다. 조회와 cap 판정 읽기는 writer 요청 하나 안에서 한다 | `memory.*`와 같다 | `memory.*`와 같다 | `memory.*`와 같다 |
| 세션 IPC(`session.issue`·`revoke`·`list`, `plugin.grant_agent_permission`·`revoke_agent_permission`·`list_agent_permissions`)와 명령 기록 조회(`surface.commands`·`last_command`·`command_at`) | `memory.*`와 같다. 세션의 찾기·변경은 writer 요청 하나 안에서 한다 | `memory.*`와 같다 | `memory.*`와 같다 | `memory.*`와 같다 |
| 요청 게이트(세션 판정, plugin cap·`split` nickname 대상 조회, rate limit 소비·ipc_calls 기록·cap 재평가) | 진입 경로가 단계마다 writer 요청을 넣고 요청을 게이트 대기열에 보관한 뒤 다음 회차로 넘어간다([요청 게이트](#요청-게이트)) | writer가 commit한다 | `memory.*`와 같다 | 단계 결과가 모두 나온 뒤 같은 진입 함수로 다시 들어와 라우팅한다. 기록 단계의 cap 액션·이상 탐지는 결과를 거둔 회차에 실행한다 |
| `telemetry.record`·`record_batch` | `memory.*`와 같다. 기록과 cap 재평가가 writer 실행 단위 하나다 | `memory.*`와 같다 | `memory.*`와 같다 | `memory.*`와 같다. 발동한 cap의 액션과 RSS 보고의 이상 탐지는 응답 continuation이 후속 처리 대기열에 넣고 메인 루프가 다음 회차에 첫 engine 문맥으로 실행한다 |
| 감사(audit)·anomaly 기록 | 기다리지 않는 writer 요청. 기록 시점 로그 정리는 `claim_prune`으로 주기를 가져간 경우에만 별도 요청으로 넣는다 | writer가 commit한다 | `memory.*`와 같다 | 해당 없음. 실패는 경고만 남긴다 |
| observer memory sink | sink 스레드가 채널에 쌓인 항목을 memory writer에 묶음 하나로 넣고 바로 다음 항목을 받는다 | writer가 묶음 하나를 commit 한 번으로 끝낸다 | commit과 같다 | 해당 없음 |
| `approval.await` 결과 기록 | 대기 워커가 memory writer에 넣고 그 결과를 기다린다 | writer가 commit한다 | commit과 같다 | 기록 결과를 받은 뒤 응답한다. 실패는 경고만 남긴다 |
| memory.db background writer(runner 결과·보고) | 각 스레드가 memory writer에 복합 연산을 넣고 결과를 기다린다(`MemoryAccess::run_blocking`) | writer가 commit한다 | commit과 같다 | 해당 없음 |
| task runner 실행 | Ready task를 tick snapshot으로 고른다(writer 요청 하나) | lease·semaphore 획득, 실행 handle 기록, `Running` 전이가 각각 writer 요청 하나이고 runner가 결과를 기다린다 | commit과 같다 | 점유 획득이 확정된 뒤, `Running` 기록 **전에** 프로세스를 시작한다. 획득 결과를 모르면(`Unknown`) 같은 holder의 점유를 돌려주고 다음 tick에 다시 얻으며 시작하지 않는다. `Running` 기록이 실패하거나 결과를 모르면 이미 시작한 실행이므로 handle과 회차를 `RunnerLoop::spawned`에 보관하고 같은 회차를 다시 시작하지 않는다. 다음 tick의 snapshot으로 확정 여부를 다시 확인해 Running이면 그 handle을 지켜보고, Ready면 전이를 다시 시도하며, 종결·회차 변경·Waiting이면 실행을 멈추고 점유를 돌려준다([agent-runner 모델](../../dev-guide/agent-runner.md#모델)) |
| 구조 journal | 전용 worker 채널에 넣는다. surface capture의 terminal `restore.command`는 비워 두고 worker가 memory writer 순서로 읽어 채운다(읽지 못하면 주기 capture는 실패해 앞 capture를 유지하고, 닫기 capture는 그 값 없이 진행한다). engine을 열 때 새 surface ID가 남은 metadata scope와 겹치지 않도록 worker가 memory writer 읽기로 surface scope의 가장 큰 ID를 확인한 뒤 연다. 읽지 못하면 그 engine만 기록 없이 거절되고(창 생성·부팅 실패로 보고) journal은 다른 engine을 위해 계속 동작한다 | worker가 commit한다 | commit된 batch만 CoreState에 적용한다([이벤트 저장소 (`tasty-event-store`)](../../architecture/event-store.md)) | 대상 엔진 공개와 ACK 뒤에 응답·checkpoint를 진행한다 |
| 최근 파일(state.db) | 공유 캐시 잠금 안에서 표시 revision을 올리고 종류별 snapshot을 state writer에 접수한다. SQL과 완료 대기는 하지 않는다 | worker가 묶음 하나를 transaction 하나로 commit한다 | 최신 목록은 접수 전에 보이며, receipt 수거 때 성공한 revision만 확정한다. 실패해도 표시 목록과 dirty는 유지한다 | 해당 없음 |
| 튜토리얼 진행(state.db) | state writer에 넣고 receipt를 받는다 | worker가 저장 묶음 하나를 transaction 하나로 commit한다 | receipt 결과로 반영한다 | 해당 없음 |
| `config.toml` 실행 중 저장 | file writer 큐에 불변 설정 snapshot을 넣고 바로 진행한다 | writer가 같은 대상의 묶음에서 마지막 snapshot을 파일로 교체한다 | 메모리 설정은 접수 전에 적용한다. 표시 revision과 저장 완료 revision을 따로 유지한다 | receipt 완료 뒤 원 요청에 한 번 응답한다. 저장 실패·접수 거절은 `applied:true, saved:false`다 |
| 프리셋 저장·삭제·이름 변경 | 소유된 ordered command를 같은 file writer에 접수한다 | worker가 참여자 잠금 안에서 명령을 실행한다 | 접수 순서로 확정 delta를 캐시에 반영한다 | 캐시 반영 뒤 원 호출자에게 한 번 응답한다 |
| 나머지 파일 | 아래 저장 명부 참조 | 소비자별 현재 경계 | 소비자별 계약 | 소비자별 계약 |

## memory.db 소비자와 계약

제품 구성에서 memory.db 저장소는 memory writer가 소유한다. 부팅은 저장소를 열어 진단 핸들을 꺼낸 뒤 소유를 writer에 넘기고, 다른 사본이 남아 있으면 부팅을 실패시킨다. 호스트 소비자(IPC 핸들러, 요청 게이트, engine, journal worker, `TaskService`, runner 문맥, task 대기)는 저장소 잠금을 잡지 않고 `MemoryAccess`로 writer에 요청을 넣는다. 저장소를 잠금으로 공유하는 경로(`WriterStore::shared`, writer 없이 호출 스레드에서 실행하는 `MemoryAccess::inline`)는 `tasty-memory`의 `test-support` 기능 뒤에 있어 제품 빌드에 없고, 그 밖의 제품 코드가 저장소를 잠금으로 다시 감싸는지는 `memory_store_has_one_owner` 검사가 찾는다.

### 연산별 계약

| 연산 | 성공 | 거절·오류 | 부분 반영 | owner·CAS·TTL |
|---|---|---|---|---|
| `put` | 새 version(신규 1, 갱신 +1) | NotFound는 없음. CasConflict(행이 없는데 cas가 있으면 actual 0), OwnedByOther, QuotaExceeded, Invalid*, ValueTooLarge, Db | 한 transaction이다. 거절과 commit 실패는 롤백되고 quota 카운터·변경 버퍼를 바꾸지 않는다 | `_host`는 모든 owner의 항목을 바꾼다. 아직 정리되지 않은 만료 행(`expires_at <= now`)은 조회와 같이 없는 것으로 판정한다. 그 위의 put은 신규 생성(version 1, owner = 호출자)이며 같은 transaction에서 만료 행을 지우고, 변경 이벤트는 `Expired`·`Created` 순서다. cas가 있으면 actual 0의 CasConflict다. regular quota 판정은 만료 행 크기를 빼지만 quota 카운터는 정리 전까지 디스크 바이트 합으로 남는다 |
| `delete` | `()` | NotFound, CasConflict, OwnedByOther, Db | 한 transaction이다 | put과 같은 owner 규칙. 만료 행도 지울 수 있다 |
| 조회(`get`·`list`·`count`·`query`·`export_regular`) | 값 | Db | 해당 없음 | 만료 행을 제외한다. `scopes`는 만료 행을 포함한다 |
| `import_regular` | applied·skipped 수 | put의 오류 | 항목마다 `put`을 따로 commit한다. 중간 오류가 나면 앞 항목은 반영된 채 오류를 돌려준다 | 항목의 `expires_at`을 그대로 쓰고 cas는 쓰지 않는다. `memory.import` 핸들러는 이 함수를 부르기 전에 모든 항목의 scope를 put과 같은 조건(PTY id 공간, 열린 surface)으로 검사하고, 하나라도 걸리면 아무 항목도 넣지 않고 거절한다 |
| secret 계열 | put·delete와 같다 | 같다 | 같다 | owner가 키의 일부이고 owner별 quota다. 만료 행은 put의 version·CAS 판정과 owner 합계 quota에서 없는 것으로 본다. 변경 이벤트를 만들지 않는다 |
| `purge_scope` | 영역별 삭제 수 | Db | 이벤트 대상 SELECT와 regular·secret DELETE 두 문장이 한 transaction이다. 어느 문장이든 실패하면 전부 롤백되고 quota 카운터·변경 버퍼를 바꾸지 않는다. commit이 성공한 뒤에만 지운 regular 키마다 변경 이벤트를 쌓고 카운터를 다시 센다 | owner를 보지 않는다. 삭제 전 SELECT로 이벤트 대상 키를 모은다 |
| `purge_expired` | 영역별 삭제 수 | Db | `purge_scope`와 같은 구조다 | `expires_at <= now`인 행 |
| `prune_prefix_keep_recent`·`prune_prefix_older_than` | 삭제 수 | Db | 한 문장을 transaction 하나로 commit한다. 행을 지웠을 때만 카운터를 다시 센다 | owner·scope를 보지 않고 변경 이벤트를 만들지 않는다 |
| `take_pending_changes` | 쌓인 변경 | 없음 | 해당 없음 | 버퍼는 commit 성공 뒤에만 쌓인다 |
| `write_batch` | 요청 순서의 요청별 결과(새 version 또는 삭제) | 원자 묶음의 거절, 저장 실패, commit 실패는 묶음 전체의 `BatchFailure` | 한 transaction이다. 아래 "실행 단위와 쓰기 묶음" 절 | 요청마다 단독 연산과 같은 판정을 앞 요청의 반영 위에서 한다 |
| `run_unit` | 본문의 결과 | 본문의 오류, 저장 실패, commit 실패 | 한 transaction이다. 거절된 쓰기만 되돌리고 앞선 쓰기는 commit한다 | 본문 안의 각 연산이 단독 연산과 같은 판정을 받는다 |

`vacuum_if_fragmented`와 `checkpoint_truncate`는 trait 밖의 `MemoryStore` 메서드이며 부팅 정비에서만 호출한다.

### 실행 단위와 쓰기 묶음

여러 쓰기를 SQLite transaction 하나로 확정하는 엔진 API다(`crates/tasty-memory/src/batch.rs`). 아래 "저장 실행기"의 memory writer가 이 API로 묶음을 실행한다. 다른 호스트 소비자는 위의 단독 연산을 쓴다.

- 실행 단위(`run_unit`, 값을 돌려받는 `with_unit`): 본문을 transaction 하나에서 실행하고 끝난 뒤 한 번 commit한다. 직렬화·commit의 단위이며 rollback의 단위가 아니다. 본문 안의 각 쓰기는 savepoint로 감싸 거절된 쓰기만 되돌린다. 본문이 오류를 돌려줘도 그전까지 반영된 쓰기는 commit되므로, 여러 번 쓰는 복합 연산(blackboard 삭제, `import_regular` 등)의 부분 반영 의미가 단독 실행과 같다. 본문 안의 읽기는 앞선 쓰기를 본다. 이미 열린 단위 안에서 다시 부르면 그 단위에 합류한다.
- 쓰기 묶음(`write_batch`): put·delete·secret put·secret delete 요청 목록을 transaction 하나로 실행한다. 원자 묶음(`Atomic`)은 요청 하나라도 거절되면 전체를 되돌리고 거절된 요청의 위치를 알린다. 독립 요청 묶음(`Independent`)은 요청마다 판정해 거절된 요청만 오류 결과를 받는다. 열린 단위 안의 원자 묶음은 묶음만 되돌린다.
- 판정 순서: 요청은 목록 순서대로 앞 요청의 반영을 본 상태에서 판정한다. version 증가, CAS, delete 뒤 재생성(version 1, 새 owner), owner, regular 전체 quota, secret owner별 quota, 정리되지 않은 만료 행을 없는 것으로 보는 put의 기존 행 판정(그 위의 재생성과 `Expired`·`Created` 이벤트)이 단독 연산과 같다. 같은 단위 안의 `purge_expired`·`purge_scope`·`prune_*`도 단위의 transaction에 들어가 뒤 요청이 정리 결과를 본다. 단위 밖의 이 정리 연산은 위 표처럼 각 호출이 자기 transaction 하나로 commit한다.
- 저장 실패: 단위 안의 쓰기·읽기(`get`·`list`·`count`·`scopes`·`stats`·`query`·`export_regular`·secret 조회)에서 `MemoryError::Db`가 나면 단위 전체를 되돌린다. SQLite가 아닌 행 변환 오류도 같다. SQLite는 일부 오류에서 transaction을 스스로 되돌리므로 같은 단위의 이후 쓰기는 실행하지 않고 같은 분류의 저장 실패를 돌려준다. 단위가 열려 있는데 연결이 autocommit 상태가 된 것(SQLite가 transaction을 끝냄)도 저장 실패로 보고 이후 쓰기와 commit을 실행하지 않는다. 본문이 이 오류를 무시해도 단위의 결과는 저장 실패다. 단위 밖 단독 호출의 실패는 그 호출만의 결과다. 묶음에서는 독립 요청 묶음이라도 묶음 전체의 `BatchFailure`이며 index가 원인 요청을 가리킨다. transaction 시작·commit 실패의 index는 없다. commit이 실패하면 열린 transaction을 되돌리고 확정하지 않는다.
- bookkeeping 공개: regular quota 카운터와 변경 버퍼(`memory.changed` 원본)는 단위가 쌓아 두었다가 commit이 성공한 뒤에만 저장소에 반영한다. 되돌린 요청·묶음·단위는 아무것도 남기지 않는다. 본문이 panic하면 단위를 되돌리고 panic을 다시 전파한다.
- commit 계측: `DbLatencyStats.commits`는 물리 commit 수다. 단위·묶음 하나는 쓰기가 하나 이상 확정됐을 때 commit 1회로 센다. 요청 수는 세지 않고, commit 하나에 확정된 쓰기 수를 `writes_per_commit` 분포에 넣는다. 쓰기가 없는 단위와 되돌린 단위는 세지 않는다. 단위 안의 정리 연산은 그 단위의 commit에 포함되어 세어진다. 단위 밖의 정리 연산은 행을 하나 이상 지웠을 때 commit 1회로 세고, 지운 행이 없으면 세지 않는다.
- 시험용 저장소(`InMemoryStorage`): regular put·delete가 CAS·owner·NotFound를 운영 저장소와 같이 판정하고, 쓰기 묶음은 원자 묶음의 되돌림과 독립 묶음의 요청별 판정을, 실행 단위는 부분 반영을 따른다. `purge_scope`는 regular 행을 한 번에 지워 전부 지우거나 아무것도 지우지 않는다. quota·TTL·secret·변경 이벤트·저장 실패·commit은 흉내 내지 않는다. `expires_at`을 저장하지 않고 `purge_expired`는 아무것도 지우지 않으므로, 만료 행 위의 재생성과 만료 크기를 뺀 quota는 운영 저장소 시험으로만 확인한다. 대조 시험이 같은 요청 순서의 결과를 운영 저장소와 비교한다.

### 저장 실행기

호출자가 저장 요청을 큐에 넘기고 바로 진행하며, 저장소를 소유한 전담 worker 한 개가 여러 요청을 묶어 실행하는 실행기다. 접수·수집·순서·결과·예산·재시도·종료 규칙은 저장소와 관계없는 공용 실행기(`tasty_utils::batch_writer`)가 정한다. 무엇을 어떻게 저장하는지는 저장소별 실행기가 정한다. memory.db 실행기는 `tasty_memory::writer`이고, state.db 실행기는 GUI의 `crate::db::writer`다(아래 [state.db 실행기](#statedb-실행기)). GUI·headless 부팅이 `AppServices`에 memory writer(`memory-writer` 스레드)를 만들고 메인 루프와 종료 단계에 연결한다. 호스트 소비자는 `tasty_memory::writer::MemoryAccess`로 요청을 넣는다. writer 없는 시험 구성의 `MemoryAccess`는 같은 실행 규칙으로 호출 스레드에서 바로 실행한다. 요청을 넣는 소비자는 "소비자 분류" 표에 적는다. 이 구조를 택한 근거와 대안은 이 문서에 있다.

#### 접수와 결과

- `submit`은 큐 잠금만 잡고 바로 돌아온다. 디스크 실행, 저장소 잠금, 완료 대기를 하지 않는다. 접수하면 receipt와 접수 순서 번호(`seq`, writer마다 1부터)를 받고, 거절하면 `Closed`(접수 중단) 또는 `Saturated`(예산 초과)를 받는다. 거절된 요청은 번호를 받지 않는다.
- receipt는 요청마다 결과 칸 하나다. `try_take`는 기다리지 않고, `wait_timeout`은 정한 시간까지 기다린다. 결과가 칸에 들어가는 시점은 그 요청이 속한 묶음이 끝난 뒤다. 쓰기는 commit이 끝난 뒤다.
- worker는 묶음이나 flush가 끝날 때마다 소유자가 넘긴 깨우기 함수를 부른다. 결과는 요청마다 칸이 따로 있어 결과를 꺼내지 않는 소비자가 있어도 worker가 막히지 않는다.
- receipt를 버려도 요청은 취소되지 않는다. 버린 것은 완료가 아니며 결과는 버려진다.

| 결과 | 뜻 |
|---|---|
| `Done(Ok)` | 실행했고 쓰기는 확정됐다 |
| `Done(Err)` | 이 요청만 거절됐다. 같은 묶음의 다른 요청은 영향을 받지 않는다 |
| `Superseded` | 같은 묶음의 뒤 요청이 같은 snapshot 키로 대체했다. 대체한 요청의 결과를 싣는다 |
| `Failed` | 확정하지 않았다. 저장 실패, busy 재시도 시한 초과, 요청 본문의 panic |
| `Unknown` | 확정됐는지 알 수 없다. 다시 실행하지 않는다 |
| `Lost` | worker가 결과를 정하지 못하고 끝났다(worker 종료, 실행기 panic). 확정 여부를 알 수 없다 |

#### 수집과 순서

- 수집 창은 큐가 비어 있다가 첫 요청이 들어온 시각에 열린다. 창 경과 `max_delay`, 요청 수 `max_items`, 요청 바이트 `max_bytes` 중 먼저 닿은 조건에서 묶음을 실행한다. 다른 요청이 없어도 단일 요청은 `max_delay` 뒤 혼자 실행된다. 한 요청이 `max_bytes`를 넘으면 그 요청만으로 묶음을 만든다.
- barrier 요청(강한 읽기)이나 flush가 들어오면 창을 바로 닫고 그 앞까지만 묶는다. barrier 요청은 앞선 요청이 끝난 뒤 혼자 실행한다. 묶음은 barrier·flush를 넘지 않는다.
- 요청은 접수 순서대로 실행하고 결과도 그 순서로 정해진다. 같은 snapshot 키의 요청은 한 묶음 안에서만 마지막 요청으로 합친다. 대체된 요청은 대체한 요청이 끝날 때 함께 끝나므로 번호에 구멍이 남지 않는다. memory 요청은 snapshot 키가 없어 합치지 않는다. state.db의 최근 목록 요청은 종류별 snapshot 키로 합친다.

#### 예산

- 접수부터 결과 소비까지 잡힌 요청 수와 바이트를 함께 제한한다(`queue_items`·`queue_bytes`). 큐 대기, 실행 중 묶음, busy 재시도 보관, 꺼내지 않은 결과가 모두 들어간다. 실행 전에는 요청 바이트를, 끝난 뒤에는 결과 바이트를 센다. 결과를 꺼내거나 receipt를 버리면 돌려준다.
- 넘치면 접수를 바로 `Saturated`로 거절한다. 호출 스레드가 대신 저장하는 경로는 없다. 결과 바이트는 끝난 뒤에야 알 수 있어 큰 결과 하나가 잠시 상한을 넘길 수 있다. 그동안 새 접수는 거절된다.
- flush는 예산을 쓰지 않는다.

#### 재시도와 오류

- 실행기가 "아무것도 확정하지 않았고 다시 하면 될 수 있다(busy)"고 알린 묶음만 같은 자리에서 다시 실행한다. 첫 실행부터 `retry_for` 안에서 `retry_backoff`부터 두 배씩 `retry_backoff_max`까지 기다린다. 그동안 뒤 요청은 큐에서 기다리므로 오래된 재시도가 새 요청을 덮지 않는다. 시한을 넘기면 묶음 전체가 `Failed`다.
- 요청 사이에 transaction이 없는 저장소(파일)의 실행기는 요청마다 끝난 결과(`Finished`)를 돌려준다. 요청마다 확정(`Done(Ok)`)·거절(`Done(Err)`)·확정하지 않은 실패(`Failed`)·결과 불명(`Unknown`) 중 하나이고, 앞 요청이 확정된 뒤 뒤 요청이 실패해도 앞 요청은 다시 실행하지 않는다. 결과가 요청 수보다 적으면 남은 요청은 `Lost`다. 대체된 요청은 대체한 요청의 결과를 받는다.
- 확정하지 않은 저장 실패는 묶음 전체가 `Failed`이고, 확정 여부를 모르는 실패는 `Unknown`이며 다시 실행하지 않는다. 요청 본문이 panic하면 그 요청만 `Failed`로 끝내고 나머지를 다시 실행한다.
- 다시 실행될 수 있으므로 요청 본문은 저장소 밖의 상태를 바꾸지 않는다. 결과에 따라 바뀌는 메모리 상태는 결과를 거두는 쪽이 고친다. command index의 행 수 캐시는 메인 루프가 기록 결과를 거둘 때 고치고, 거두기 전에 이어진 기록은 캐시에 진행 중인 기록 수를 더해 한도를 판정한다.
- 실행기 자체가 panic하면 그 묶음은 `Lost`이고 worker는 다음 요청을 계속 처리한다.

#### flush와 종료

- `flush(since)`는 지금까지 접수된 요청 뒤에 cut을 넣는다. cut 앞의 요청이 모두 끝나면 결과가 나온다. 결과는 `since` 뒤부터 cut까지의 요청이 모두 효과를 냈을 때만 `Ok(FlushPoint)`이고, 거절이나 실패가 있으면 그 수를 실은 `NotClean`이다. 다음 flush는 앞 결과의 `FlushPoint`를 `since`로 넘긴다. 다른 호출자의 요청도 구간에 들어간다.
- `close`는 새 접수를 막는다. 이미 접수한 요청은 계속 실행하고 flush는 받는다.
- `begin_shutdown`은 접수를 막고 마지막 flush cut을 넣은 뒤 worker가 남은 요청을 모두 실행하고 끝나게 한다. `poll_shutdown`은 기다리지 않고 남은 worker 수를 돌려주며, join이 끝나면 마지막 flush 결과를 로그에 남긴다. 효과가 없던 요청이 있으면 경고를 남긴다.
- 호스트 연결: worker는 묶음·flush가 끝나면 GUI는 `TimerTick`, headless는 journal 깨우기 이벤트로 메인 루프를 깨운다. 알림은 `WakeLatch`로 묶는다. 메인 루프가 회차 첫머리에 확인(`acknowledge`)하기 전에 끝난 묶음의 알림은 이미 보낸 깨우기 하나에 합친다. 깨우기 수는 알림(묶음) 수를 넘지 않고, 메인 루프 회차 수보다 많아도 하나만 많다(확인과 결과 수거 사이에 끝난 묶음이 다음 회차를 위한 깨우기를 하나 남긴다). 확인 뒤 끝난 묶음은 다시 깨운다. 메인 루프는 회차마다 종료 요청 없이 worker가 끝났는지 확인해 한 번 오류 로그를 남긴다. 종료는 memory.db에 쓰는 producer를 모두 멈춘 뒤 접수 중단 → 마지막 flush → join 순서다. GUI는 `StoppingPlugins` 단계에서 plugin 정리가 끝난 뒤, headless는 runner·자원 회수 뒤에 진행하며 둘 다 5초 기한을 넘기면 경고를 남기고 계속 종료한다. GUI의 state.db writer도 같은 단계에서 같은 순서와 5초 기한·포기 경고로 종료한다. 두 writer는 각자 기한을 따로 재고, 한쪽이 남아 있어도 다른 쪽의 종료를 회차마다 함께 진행한다.
- writer를 drop하면 종료를 시작하지만 기다리지 않는다. 끝나지 않은 worker가 있으면 경고를 남긴다. 프로세스가 그대로 끝나면 확정되지 않은 요청은 사라진다.

#### 한도 기본값

| 한도 | 기본값 |
|---|---|
| `max_delay` | 2ms |
| `max_items` | 256 |
| `max_bytes` | 1MiB |
| `queue_items` · `queue_bytes` | 4096 · 16MiB |
| `retry_for` · `retry_backoff` · `retry_backoff_max` | 1초 · 1ms · 50ms |

`max_delay` 2ms는 [저장 실행기 측정](storage.md#저장-실행기-측정)에서 1·2·5ms를 비교해 정했다. 동시 burst는 세 값 모두 commit 1회로 묶이고, 2ms 간격 유입의 commit은 1ms에서 0.4~4.9%, 2ms에서 25~30% 줄며, 완료 지연은 수집 창 길이만큼 늘어난다. `max_items`·`max_bytes`·예산은 측정에서 닿지 않은 상한이고(최대 100건·25,500바이트), 재시도 한도는 측정하지 않은 기본값이다.

#### memory.db 실행기

- 요청 종류: 쓰기 하나(`Write`), 쓰기 묶음(`Batch`, 원자·독립), 읽기-판단-쓰기를 한 번에 하는 복합 연산(`Unit`), 강한 읽기(`Read`, barrier).
- 묶음 하나는 `run_unit` 하나, 곧 SQLite transaction과 commit 하나다. 요청은 접수 순서대로 앞 요청의 반영을 보고 판정한다. 거절된 요청은 그 요청만 되돌리고, 원자 묶음 요청의 거절은 그 묶음만 되돌린다. 저장 실패는 묶음 전체를 되돌린다.
- 복합 연산은 읽기와 판단과 쓰기를 writer 안에서 한 번에 실행한다. 강한 읽기와 쓰기를 따로 접수해 같은 판단을 나누지 않는다. 여러 키를 읽고 바꾸는 연산도 한 요청이며 transaction 경계는 그 요청이 속한 묶음이다. 본문은 `'static` 값만 잡을 수 있어 호출 스레드의 빌린 상태나 잠금 guard를 실을 수 없다(컴파일 오류). busy나 다른 요청의 panic 뒤 다시 실행될 수 있으므로 저장소 밖의 효과를 내지 않는다.
- 강한 읽기는 앞선 요청의 commit 뒤 transaction 없이 혼자 실행한다.
- 결과를 기다려도 되는 스레드(runner·worker·부팅 정리)는 `run_blocking`으로 복합 연산을 넣고 결과까지 기다린다. 시한이 없다. 시한이 지나도 요청은 취소되지 않아, 시한 뒤 같은 판단을 다시 넣으면 앞 요청과 함께 두 번 확정될 수 있기 때문이다. 확정하지 않은 실패(`Failed`: 저장 실패·busy 시한 초과·같은 묶음의 다른 요청 실패)는 같은 본문을 새 요청으로 세 번까지 넣고, 결과 불명은 다시 넣지 않고 호출자에게 돌려준다. 결과를 거두는 메인 루프는 writer 큐를 기다리지 않는다. 메인 루프가 writer 결과를 거둘 때 그 스레드에 표시를 남기고, 표시된 스레드가 큐를 기다리면 debug 빌드에서 멈춘다(writer 없는 시험 구성은 호출 스레드에서 바로 실행하므로 검사하지 않는다).
- 오류 분류: busy는 단위 전체가 되돌려진 뒤라 다시 실행한다. 원인 요청이 저장 실패를 받았거나 transaction 시작 전에 실패하면 `Failed`다. 모든 요청이 저장 실패 없이 끝났는데 단위가 실패하면 commit 단계일 수 있어 `Unknown`이다. 본문이 저장 실패를 삼킨 경우도 원인 요청을 알 수 없어 `Unknown`으로 보수적으로 다룬다. 요청 본문의 panic은 단위를 되돌린 뒤 그 요청만 실패시키며, 이 재실행은 저장소가 panic한 단위를 되돌린다는 전제다(`MemoryStore`가 그렇게 동작한다).
- 저장소 접근: worker가 저장소를 소유한다(`WriterStore::owned`). writer가 종료되면 저장소도 닫힌다. 시험 구성은 저장소를 직접 준비·확인하려고 같은 잠금을 묶음마다 잡게 할 수 있다(`WriterStore::shared`, `test-support` 기능).
- 예산 크기: 쓰기는 owner·key·값 길이(JSON은 직렬화 길이)를, 복합 연산과 강한 읽기는 요청할 때 정한 크기를 요청과 결과에 모두 쓴다.

#### state.db 실행기

GUI 부팅(`init_boot_db_and_theme`)이 state.db를 실행당 한 번 열고, 같은 자리에서 최근 목록을 한 번 읽은 뒤 연결을 `state-writer` 스레드에 넘긴다. 연결과 튜토리얼 버전 체인은 실행기(`StateExecutor`)만 가진다. writer와 그 계측은 `AppServices`가 소유하고, 창과 파일 열기 경로가 인자 없이 닿도록 최근 목록 캐시와 요청 큐만 담은 공유 핸들을 공개한다(`crate::db::shared_recent_files`, `shared_queue`). 연결·worker는 static에 두지 않는다. headless는 state.db를 열지 않는다.

- 요청 종류: 종류별 최근 목록 전체(`Recent`, snapshot 키 `recent:<kind>`), 튜토리얼 읽기(`TutorialLoad`, barrier), 튜토리얼 재개 위치 저장(`TutorialSave`). 튜토리얼 요청은 View 식별자를 가지고 있어 View가 닫혀도 접수한 저장이 실행된다.
- 묶음 하나는 SQLite transaction과 commit 하나다. 요청은 접수 순서대로 실행한다. 어느 요청이든 SQL 오류를 받으면 묶음 전체를 되돌린다. CAS가 맞지 않은 튜토리얼 저장은 오류가 아니라 `updated: false` 결과다. 같은 묶음 안에서 같은 종류의 최근 목록은 마지막 목록만 저장한다.
- 오류 분류: busy는 되돌린 뒤 다시 실행한다. 나머지 SQL 오류와 transaction 시작 실패는 `Failed`, commit 단계의 오류는 `Unknown`이다. 요청 본문의 panic은 되돌린 뒤 그 요청만 실패시킨다.
- 잠금 대기: 연결은 rusqlite가 여는 연결의 기본 busy 대기(5초)를 그대로 쓴다(memory.db 연결도 같다). 다른 연결이 쓰기 잠금을 쥐고 있으면 worker가 SQLite 안에서 최대 5초 기다린다. 그 안에 풀리면 재시도 없이 확정하고, 넘기면 busy를 받는다. 이때는 이미 `retry_for`(1초)가 지났으므로 다시 실행하지 않고 `Failed`로 끝낸다. 메인 루프는 이 대기에 참여하지 않는다. 종료 중 이 대기가 겹치면 종료 기한(5초)을 넘길 수 있고, 그때는 포기 경고를 남기고 종료를 계속한다.
- 튜토리얼 버전 체인은 묶음마다 작업 사본에서 잇고 commit이 성공한 뒤에만 그 사본을 채택한다. busy·실패·결과 불명·panic이면 사본을 버려 다시 실행하거나 다음 저장이 확정되지 않은 버전을 쓰지 않는다. 읽기는 앞선 저장이 확정된 뒤 혼자 실행하고, 성공했을 때만 그 View의 체인을 비운다.
- 최근 목록: 공유 캐시 잠금 안에서 중복 정리·맨 앞 추가·10개 상한·표시 revision 증가·저장 접수까지 하고 SQL과 결과 대기는 하지 않는다. 두 창의 캐시 순서와 접수 순서가 같다. 저장은 그 종류의 행과 순서 정보를 지우고 목록을 다시 넣는 한 번의 쓰기다. 그래서 같은 키의 다른 표기 정리와 상한 밖 정리가 함께 확정되고, 디스크가 확정한 목록과 같아진다. `opened_at`에는 연 시각을 그대로 두고, 목록 순서는 같은 transaction에서 `recent_file_order`의 rank로 남긴다. 그래서 같은 초에 열었거나 시계가 거꾸로 간 항목도 다시 읽을 때 저장한 순서가 되고, 재실행을 거듭해도 연 시각은 바뀌지 않는다.
- 표시 revision과 확정 revision을 종류별로 따로 센다. 메인 루프가 회차마다 결과를 거둬 `Done(Ok)`인 목록의 revision까지만 확정을 올린다. 접수 거절(`Saturated`·`Closed`)과 `Failed`·`Unknown`·`Lost`는 경고만 남기고 확정을 올리지 않으며 화면 목록은 그대로 둔다. 다시 접수하지 않는다. 뒤의 저장이 성공하면 목록 전체를 담으므로 앞서 저장하지 못한 변경도 함께 확정된다. 대체된 목록은 대체한 목록의 결과로 확정을 판단한다.
- 결과 수거: 최근 목록 receipt는 메인 루프의 `poll_state_writer`와 종료 단계가 거둔다. 튜토리얼 receipt는 각 View가 거두고, 닫힌 View의 receipt는 버려져 결과가 나올 때 예산을 돌려준다.
- 예산 크기: 최근 목록은 종류 이름과 경로 길이에 항목마다 8바이트를, 튜토리얼 요청은 주제·단계 이름 길이에 고정 크기를 더한다. 읽기 결과는 행 수와 재개 단계 길이로 센다.
- 종료: GUI 종료가 시작되면 튜토리얼 입구가 새 튜토리얼 요청만 거절한다. 최근 목록 접수는 writer의 종료가 시작될 때까지 받는다. `StoppingPlugins`에서 writer가 접수를 막고 마지막 flush 뒤 join한다. join이 끝나면 남은 결과를 거두고, 화면에 보였지만 확정하지 못한 종류가 있으면 종류별로 경고한다.

최근 파일 공유 캐시는 종류별 revision과 별개로 캐시 전체의 연속 미확정 기간을 관측한다. `system.pressure.state_snapshots.recent_files`는 `dirty`·`dirty_age_us`를 제공하며 state writer가 없는 GUI·headless에서는 null이다. 첫 표시 revision 증가 시 단조 시계의 시각 하나를 보관하고, 새 종류·새 revision·오래된 성공·접수 거절·실패·결과 불명으로 초기화하지 않는다. 모든 종류의 최신 표시 revision이 확정된 때만 null로 돌아간다. 각 종류의 독립 나이나 현재 가장 오래된 종류의 나이가 아니라, 캐시에 미확정 변경이 연속해서 존재한 기간이다.

조회는 같은 캐시의 scalar와 시계만 읽고 완료 receipt를 수거하거나 DB에 접근하지 않는다. 시간은 포화 변환한 마이크로초다. 이미 준비된 성공 결과도 기존 settle에서 수거하기 전에는 dirty로 남는다. 저장 실패 뒤 pending이 0이어도 나이는 이어지므로 공용 writer의 oldest_pending_age_us와 다르다. 최근 목록 순서·opened_at·rank·병합·접수·재시도·종료 규칙과 튜토리얼 CAS/버전 체인은 이 관측값으로 바꾸지 않는다.

#### 파일 실행기

`AppServices`는 설정 snapshot을 `file-writer`에 제출한다. 실제 backend인 `SettingsStorage::save`는 파일 교체가 끝나야 성공하며, 메인 루프는 receipt를 기다리지 않고 매 회차 결과를 거둔다. 같은 묶음의 config snapshot은 마지막 요청으로 대체되고 앞 요청도 그 저장 결과를 받는다. 이는 메모리 적용 전 category reset이 더 새 요청에 밀린 경우와 다르다.

메모리에 적용된 visible revision과 실제 교체한 committed revision을 구분한다. 오래된 완료는 더 새 dirty를 지우지 않는다. 큐가 가득 차거나 닫혀도 메모리 적용은 유지하고 저장 실패를 원 요청에 보낸다. 종료 전에 미확정 최신 snapshot이 실행 중이지 않으면 한 번 더 제출하되 앞서 실패한 원 요청에 다시 응답하지 않는다. 독립 파일 작업의 성공/실패/불명은 요청별 결과로 끝나며 앞 성공을 묶음째 재실행하지 않는다.

config snapshot은 같은 load에서 나온 원본 관측값을 공유한다. Tasty의 저장이 성공하면 이 관측값을 함께 전진시키고, 외부 편집이나 별도로 읽은 사본이 그 뒤 파일을 바꾸면 덮어쓰지 않고 저장 실패로 보고한다. 저장 잠금은 읽기·원본 보존·교체 전체를 감싼다. 참여하지 않는 편집기의 마지막 확인과 rename 사이 경합은 막지 못한다. 손상 원본의 저장 직전 백업과 읽기 실패 원본의 쓰기 금지는 유지한다.

셸 설정 확인과 승인한 Lua 해시도 같은 큐에 넣는다. Lua는 이미 읽어 승인한 source를 보관했다가 저장 시도가 끝난 뒤 실행하며, 저장 실패가 실행을 막지 않는 기존 계약을 유지한다. 부팅 정규화·셸 확인·테마 초기 적용은 최종 snapshot 하나로 같은 writer에 넣고, 메인 루프를 막지 않는 완료 확인 뒤 다음 부팅 단계로 넘긴다. 다른 파일 소비자의 저장은 아래 명부의 현재 스레드를 따른다.

### 순서가 있는 파일 명령

`FileRequest::Command`는 소유된 작업과 결과 바이트 추정치를 담으며 snapshot 키가 없다. 이름 예약·삭제·이름 변경·외부 효과의 선행 저장은 합치지 않는다. 실행기는 명령마다 한 번 실행하며, 도중 panic은 결과 불명으로 끝내고 재실행하지 않는다. 큐가 없거나 닫혔거나 예산을 넘으면 실행 전에 거절한다. 테스트의 writer 없는 작은 fixture만 즉시 실행 경로를 사용하며 제품 구성의 대체 경로가 아니다. payload의 Debug 표현이나 내용 로그는 없다.

`src/file_command.rs`가 소유된 명령·결과·typed ticket과 비차단 `Port`/`Receipt` 계약을 정의한다. 이 모듈은 App이나 구체 실행기를 참조하지 않는다. 웹훅 실행부는 주입된 `Arc<dyn Port>`만 보유한다. App의 어댑터는 기존 `Handle<FileExecutor>`를 이 port에 연결하고 같은 큐의 receipt를 보관한다. 별도 worker나 동기 대체 실행은 만들지 않으며, 기존 App 소비자의 handle 기반 API도 이 계약으로 접수한다. 큐 예산·완료 보관·Unknown 분류와 producer 종료 순서는 기존 writer가 유지한다.

프리셋 표시 캐시는 worker의 파일 잠금과 별개다. IPC·plugin·GUI 캡처는 journal 캡처 결과를 파일 명령으로 넘긴 뒤 원 호출자·View identity를 유지해 완료한다. 원 observer가 닫혀도 접수된 저장과 캐시 반영은 계속한다. 편집기 배치는 첫 실패에서 멈추며 앞서 확정한 변경을 유지한다. 파일 명령의 완료 건수는 물리 교체 건수가 아니다. `FileWriteStats`의 replaced/failed/unknown은 설정과 snapshot 교체 시도만 세고, `system.pressure.file_commands.presets`는 접수·거절·완료·불명·대기를 보관한다. 완료에는 알려진 저장소 오류도 포함된다.

### file worker의 물리 파일 작업 계측

`system.pressure.file_operations`는 실제 file worker 요청 실행 범위에서 호출한 공용 replace/delete helper만 센다. 부팅 load와 standalone CLI, 다른 스레드의 같은 helper 호출은 포함하지 않는다. 읽기·파싱·참여자 잠금·관측값 검사에서 거절돼 helper를 부르지 않았으면 물리 교체 시도는 0건이다. replace는 고유 임시 파일 준비·권한 보존·내용 쓰기·최종 게시를 포함한 helper 작업 하나이며 개별 OS write/rename/fsync 횟수가 아니다. rename 명령이 새 파일 교체 후 원본을 unlink하면 replace와 delete 두 작업이다. 교체의 내부 임시 파일 정리는 사용자 파일 delete에 넣지 않는다.

각 작업은 attempts/succeeded/failed/unknown, 종료한 작업의 duration(samples/sum_us/max_us/bounds_us/counts), 고정 오류 분류를 보관한다. helper가 Ok/Err로 돌아오면 성공/실패, 돌아오기 전 unwind는 결과 불명이다. 이미 성공한 replace 뒤 명령이 panic해도 그 교체 성공을 취소하지 않는다. payload_bytes는 helper에 제출한 바이트 합이고 published_bytes는 성공한 교체의 바이트 합이다. 실제 장치 IO 바이트나 전원 장애 내구성을 뜻하지 않는다. 삭제 바이트는 파일 내용을 읽어 추정하지 않아 null이다. 실패한 remove의 NotFound는 실패 syscall로 세지만, 상위 명령은 이미 없는 파일의 삭제 목적을 달성한 것으로 처리할 수 있다.

보안 전용 temp/ACL 교체처럼 공용 helper를 쓸 수 없는 backend에는 thread-bound `observe_replacement` guard를 제공한다. 검증·잠금 뒤 실제 임시 파일 준비 직전에 시작하고, 반환 결과의 고정 ErrorKind로 끝낸다. 끝내지 않고 drop하면 Unknown이다. 기존 공용 helper에는 guard를 겹치지 않으며, guard가 연결되지 않은 전용 backend 작업은 이 계측에 포함하지 않는다. duration 표본이 없으면 합·최댓값은 null이다.

executed_commands의 known_success/known_failure/unknown/partial은 명령 backend가 제공한 분류다. 결과 T를 반환했다는 이유로 저장 성공을 추정하지 않는다. 분류 없는 기존 submit은 unclassified로 세고, 접수 후 실행 전에 유실된 작업은 물리 시도가 없어 공용 writer의 lost와 구분한다. 부분 성공은 성공·실패 카운터 차이로 추정하지 않는다. 목적지 교체 뒤 원본 검증에서 실패하면 delete 시도 없이도 partial일 수 있다.

관측은 스레드에 묶인 범위와 고정 원자 카운터·히스토그램만 사용한다. 조회는 파일·DB를 읽거나 receipt를 소비하지 않으며 경로·내용·오류 문구·비밀 값은 보관하지 않는다. 여러 카운터를 한 transaction으로 읽는 것은 아니므로 실행 중 attempts가 종료 결과보다 먼저 보일 수 있다. 아직 공용 helper로 옮기지 않은 backend의 별도 파일·디렉터리 작업은 포함하지 않는다.

### 설정의 미확정 revision 나이

`SettingsSaves`는 설정이 메모리에 적용되어 visible revision이 오르는 첫 순간의 시각 하나를 기존 단조 시계로 보관한다. `system.pressure.file_snapshots.settings`는 GUI·headless 모두 visible/committed/pending/dirty/dirty_age_us를 제공한다. 부팅에서 이미 읽은 설정은 미확정 변경이 아니며, 부팅 중 실제 적용·제출된 변경은 같은 경계를 따른다. 시각과 revision 조회는 캐시만 읽고 receipt를 수거하거나 파일·DB에 접근하지 않는다.

이후 적용, 접수 거절, 저장 실패·결과 불명, 오래된 revision의 성공, 종료 직전 최신 revision 재시도는 첫 시각을 유지한다. 최신 표시 revision의 성공을 소유 객체가 확인한 때만 나이가 null로 돌아간다. worker가 완료했어도 receipt를 거두기 전에는 미확정으로 남는다. 시험용 동기 저장도 같은 성공 확정 규칙을 따른다. 시간은 마이크로초이고 포화 변환하며, 미확정 변경이 없으면 null이다. 결과가 실패로 끝나 pending=0이어도 나이는 계속 흐르므로 공용 writer의 oldest_pending_age_us와 대체해서 읽지 않는다. 설정 적용·응답·부팅 barrier·최신 snapshot·종료 재시도 순서는 이 관측값으로 바꾸지 않는다.

### 공용 즐겨찾기 snapshot

Explorer·포트 즐겨찾기는 프로세스 공용 `SharedList` 원본을 바꾸고 표시 revision을 올린 뒤 불변 TOML snapshot을 접수한다. 목록 잠금 안에서는 메모리 변경·직렬화·접수 순서만 정한다. 디스크 읽기·잠금·교체와 receipt 대기는 worker에서 한다. 같은 파일의 수집 묶음에서는 마지막 snapshot만 교체하며 대체된 요청도 그 결과를 받는다. 메인 루프가 receipt를 거둘 때 성공한 revision만 저장 완료로 확정한다. 실패·접수 거절·결과 불명은 표시 목록을 되돌리지 않고 dirty를 유지하며 경고 알림에 원인과 조치 안내를 남긴다.

부팅 때 읽은 원본이 손상됐거나 읽기 실패였다면 빈 목록으로 시작하되 원본을 덮어쓰지 않는다. 외부 편집이나 symlink 대상 변경도 저장 충돌로 거절한다. 같은 load의 snapshot들은 성공한 자체 저장의 관측값을 공유한다. 참여하지 않는 편집기의 마지막 확인과 rename 사이 경합은 공용 교체 함수와 같은 한계다. 사용자 TOML은 pretty 형식으로 다시 쓰므로 주석 바이트를 보존하는 편집기는 아니다.

종료는 producer 중단 후 남은 receipt를 거두고, 최신 dirty에 실행 중인 요청이 없으면 한 번 더 제출한 뒤 기존 file writer를 close·flush·join한다. 미확정 목록은 종료 로그에도 남긴다. `system.pressure.file_snapshots`의 목록별 visible/committed/pending으로 화면 revision과 저장 완료·대기를 구분한다. 두 즐겨찾기는 GUI 기능이며 headless는 이 목록을 생성하거나 저장하지 않는다.

파일 snapshot의 `dirty`는 visible > committed이며 `dirty_age_us`는 처음 미확정 변경부터 이어진 시간이다. 두 값은 즐겨찾기·파일 연결·hook registry의 같은 상태를 읽는다. 이후 변경, 접수 거절, 직렬화 실패, 저장 실패·결과 불명, 재시도와 오래된 revision의 성공은 이 시간을 초기화하지 않는다. 최신 표시 revision의 성공 receipt를 거두었을 때만 `null`로 바뀐다. pending이 0이어도 dirty일 수 있다. 초기 load나 성공한 명시적 reload는 미확정 변경이 없는 상태로 시작한다. 조회는 receipt를 소비하거나 파일을 읽지 않으며, 누적 시간은 Clock의 monotonic 시각으로 마이크로초를 버림해 센다. 공용 writer의 accepted-work 대기 시간과 의미가 다르다.

### 파일 registry snapshot

`RuntimeRegistries.file_rules`는 `file-handlers.toml`의 읽은 원본·문서·디스크 revision을 소유한다. 부팅은 한 번 읽은 같은 내용으로 detector와 handler를 설치한다. 설정 창 Save는 두 registry의 사용자 contribution과 확장자 우선순위를 하나로 직렬화해 기존 file writer에 제출하며, host/plugin contribution은 저장하지 않는다. 다른 최상위 절은 유지한다. 메모리 적용과 저장 완료는 별개이고 `system.pressure.file_snapshots.file_handlers`가 visible/committed/pending을 표시한다. registry 내부의 effective 값 재계산용 dirty와 디스크 dirty는 다른 상태다.

worker는 공용 참여자 잠금·fingerprint·링크 대상 검사 뒤 교체한다. 읽기 실패·문법/선언 파싱 실패 원본과 외부 편집은 덮지 않는다. 성공 receipt만 committed를 전진시키며 실패·거절·결과 불명은 표시값을 유지하고 dirty를 남긴다. GUI는 최종 저장 실패를 설정 결과 토스트와 파일명·원인·복구 안내가 있는 알림으로 보여준다. 종료 전에 최신 dirty의 pending이 없으면 한 번 재접수하고 같은 writer를 drain한다. GUI·headless 모두 이 경계를 조립한다. 초기 읽기와 명시적 reload는 다음 registry 조회의 입력을 정하는 동기 읽기이며 파일 쓰기는 하지 않는다.

초기 파일/hook registry 읽기·파싱 실패는 파일명과 원인 분류(대상 해석·읽기·UTF-8·형식)를 경고 로그에 남기고 원본을 보호한다. 파서의 원문 발췌나 설정 값은 로그와 이후 저장 오류에 넣지 않는다. 파일 부재는 기본값을 쓰는 정상 경로라 경고하지 않는다. 명시적 reload도 같은 안전한 원인 분류로 오류를 반환한다.

`file_handler.reload`는 해당 프로세스가 처음 읽은 경로에서 한 번 읽어 두 registry를 갱신한다. 저장 pending 중에는 아직 완료되지 않은 snapshot과 충돌하지 않도록 오류로 답한다. 읽기/파싱 실패도 오류로 답하고 이전 registry와 원본 관측을 유지한다. 성공한 명시적 reload는 저장되지 않은 표시 변경을 파일 내용으로 바꾸며, 새로 읽은 상태를 visible=committed의 다음 revision으로 잡는다. 주석·공백·키 순서까지 보존하는 편집기는 아니며 pretty TOML로 저장한다.

### 훅 registry snapshot

프로세스 공용 hook registry는 최초 사용자 파일 읽기에서 원본 관측값과 디스크 revision을 함께 만든다. 런타임 upsert/remove와 설정 창 Save는 사용자 contribution만 불변 snapshot으로 제출한다. effective registry 재계산용 dirty는 디스크 dirty와 분리한다. 같은 파일 묶음은 마지막 snapshot으로 합치며 앞 요청도 그 결과를 받는다. 알 수 없는 최상위 절은 유지하고 TOML은 다시 직렬화한다. action을 표현할 수 없으면 일부 action을 빼고 저장하지 않고 해당 snapshot을 실패로 처리한다.

IPC 응답은 receipt 뒤에 한 번 보내며, 메모리 적용 후 저장 실패는 `applied:true, saved:false`인 오류다. GUI 실패는 설정 결과 토스트와 지속 알림에 남긴다. 원본 읽기·파싱 실패, 외부 편집, symlink 대상 변경은 원본을 덮지 않는다. `hook_handler.reload`는 pending 중이면 거절하며, 읽기·파싱 실패 때 메모리와 관측을 유지한다. 성공한 명시적 reload는 파일 내용으로 사용자 contribution을 바꾸고 visible=committed의 새 revision으로 시작한다. GUI·headless는 같은 writer와 종료 drain을 쓰며 최신 dirty만 한 번 재시도한다. 이미 답한 IPC 요청에는 재시도 결과를 다시 보내지 않는다. `system.pressure.file_snapshots.hook_handlers`가 visible/committed/pending을 표시한다.

## 소비자 분류

| 분류 | 소비자 | 스레드 | 요청 하나 안에서 하는 일 |
|---|---|---|---|
| 단순 쓰기 | `memory.put`·`memory.delete`·secret·approval 기록·surface meta·effect 설치 metadata(writer 요청. 설치 metadata는 기다리지 않는 독립 묶음), audit·anomaly 기록(기다리지 않는 writer 요청) | 메인 루프(IPC) | 한 연산. 기록 시점 로그 정리가 붙는 경로는 prune 후 put |
| 복합 읽기-판단-쓰기 | blackboard·plan·cache·goal·`memory.import`·command index·barrier·semaphore·lease·ratelimit IPC(각각 writer 요청 하나), session, 요청 게이트의 세션 판정과 기록 단계(rate limit 소비·ipc_calls 기록·cap 재평가), `telemetry.record`(기록·cap 재평가) | 메인 루프(IPC·PTY 출력 처리) | 읽고 판단한 뒤 여러 번 쓴다. writer 요청은 그 판단 전체가 한 요청이라 같은 묶음의 앞 요청 반영을 본다. command index는 한도 판정을 writer 요청 안에서 하고 행 수 캐시는 결과를 거둘 때 고치며, 한도 알림은 다음 PTY 출력 처리 회차에 결과를 거둘 때 낸다. semaphore·lease의 저장은 CAS 없이 writer의 직렬 실행에 의존한다. 여러 항목을 지우거나 되돌리는 연산은 원자적이지 않다 |
| task | `TaskService`(생성·graph submit·삭제·purge. IPC는 writer 요청을 넣고 결과를 거둔 뒤 응답한다), 훅 완료 보고(기다리지 않는 writer 요청), runner tick | memory writer, `agent-runner-ws{N}`·부팅 정리 스레드(결과를 기다린다) | graph submit은 plan·stage·activate를 한 요청에서 한다. purge는 계획과 적용을 한 요청에서 한다. runner는 snapshot·점유 획득·상태 전이·완료를 각각 별도 요청으로 한다. 대기자 깨우기·정리 시작 같은 효과는 요청 본문이 모으고 commit된 결과를 받은 쪽이 실행한다. 본문은 다시 실행될 수 있어 효과를 직접 내지 않는다 |
| background 쓰기 | observer memory sink·`approval.await` 결과·Run 결과·후처리·보고(writer 요청) | 각 전용 스레드 | observer는 채널에 쌓인 기록과 넘친 옛 키의 보존 삭제를 독립 요청 묶음 하나로 writer에 넣는다. 보존 삭제는 저장이 확정된 키만 센다 |
| 읽기 | `memory.get`·`list`·`count`·`approval.history`·`approval.summary.get`·`surface.meta.get`·`list`, telemetry·감사·세션·명령 기록 조회(writer 요청), surface capture의 `restore.command`와 engine을 열 때의 surface ID 하한(journal worker가 writer 요청을 넣고 기다린다), 요청 게이트의 cap·nickname 대상 조회(강한 읽기 하나), DAG 화면, task 대기 | memory writer, `dag-read`, task 대기 worker | writer 요청 하나 안의 조회는 한 시점을 본다. 메인 루프는 writer 결과를 기다리지 않고 회차마다 확인한다. 직접 잠금 읽기가 느린 저장에서 수백 ms를 기다리던 측정은 [직접 잠금 읽기 측정](storage.md#직접-잠금-읽기-측정)에 있다. DAG 화면은 모든 대상 workspace의 목록을 writer 요청 하나로 읽고 10ms씩 나눠 기다리며, 종료하거나 요청이 버려지면 기다림을 그만둔다(읽기만 하므로 결과를 버려도 남는 것이 없다). task 대기는 대기자를 먼저 등록한 뒤 writer 요청으로 읽는다 |
| 변경 구독 | `memory.changed` 발행 | GUI 이벤트 루프, headless 루프 | 두 루프가 회차마다 writer가 commit 뒤 옮겨 둔 변경 버퍼(`ChangeFeed`)를 꺼내 plugin에 발행한다. 저장소 잠금을 바로 얻을 때만 그 잠금 안에서 버퍼를 꺼내고 직접 잠금 소비자가 저장소 버퍼에 남긴 변경을 뒤에 붙인다. 잠금이 바쁘면 기다리지 않고 writer가 옮긴 변경만 꺼내며, 남은 변경은 writer의 다음 묶음 끝이나 다음 회차에 같은 순서로 나온다. plugin 매니저가 없으면 꺼낸 변경을 버려 버퍼가 쌓이지 않는다 |
| 삭제 cleanup | surface·workspace 은퇴의 `purge_scope`(writer 요청 하나), 부팅 legacy 범위 밖 surface meta 정리(기다리지 않는 writer 요청), 범위 밖 surface meta 정리 | 메인 루프 | 대상이 다시 살아 있으면 정리를 넣지 않는다. 은퇴 정리는 결과를 다음 폴링 회차에 거두며, 같은 ID의 새 소유자가 쓰는 metadata는 정리 뒤에 접수되므로 지워지지 않는다 |
| TTL 정리 | `Tick::LogPrune` 마다 `purge_expired`를 기다리지 않는 writer 요청 하나로 넣는다. `memory.gc`(Local 전용)도 같은 함수를 writer 요청으로 부른다 | 메인 루프(GUI·headless 같은 틱) | 판정은 만료 행을 이미 없는 것으로 보므로 정리는 디스크·quota 카운터 회수와 `expired` 발행이다. 로그 보존 정리와 별도 `Unit`으로 제출해도 writer 수집 묶음에 합쳐지면 transaction을 공유하고 저장 실패 시 함께 롤백될 수 있다. TTL 청소는 한 시간 claim과 관계없이 다음 `LogPrune` 틱에 다시 제출한다 |
| 로그 prune | 관측 로그 보존(부팅·기록 시점·`Tick::LogPrune`). `Tick::LogPrune`은 기다리지 않는 writer 요청이다 | 메인 루프 | 시간 기준 후 개수 기준. 한 시간 간격 제한은 요청 제출 전에 `claim_prune`으로 판정하며, 성공한 claim은 마지막 시도 시각을 갱신한다. 접수 거절·저장 실패·같은 묶음의 롤백에도 이 시각을 되돌리지 않아, 간격이 지난 뒤 다음 호출에서 다시 claim할 수 있다. 성공한 삭제 시각을 기준으로 재시도하는 주기가 아니다 |
| 정비 | 부팅의 prune·VACUUM·WAL truncate | 부팅 메인 | port로 감싸기 전 구체 `MemoryStore`에서 실행 |

### IPC 응답

writer를 거치는 IPC 요청은 결과가 나온 뒤 응답한다. 핸들러는 요청과 함께 그 결과를 응답으로 바꾸는 continuation을 등록하고, 호출 경로가 원 호출자를 붙인다. `handle_checked_request` 밖의 app 계층 경로(GUI·headless의 plugin 조회 메서드, GUI의 감사 화면 follow·clear와 agent 권한 부여·회수)도 응답을 보낼 때 등록된 continuation이 있으면 원 호출자를 붙인다(`send_handled`). 명령 하나를 마친 뒤 붙지 않은 continuation이 남으면 호출자가 이미 자리표시 응답을 받은 결함이라 오류 로그를 남기고, 시험에서는 실패한다. 응답에 쓰는 요청 ID·caller·권한·대상은 핸들러가 접수할 때 잡은 값이다.

- 메인 루프는 회차마다 writer 결과를 거둔 뒤 continuation으로 응답을 만들어 보낸다. GUI·headless 종료 단계도 writer를 join하며 남은 응답을 보낸다.
- 멱등 키(`idempotency_key`)가 있는 Mutate 요청은 결과를 기다리는 동안 키를 진행 중으로 둔다. 그동안 같은 키로 다시 보낸 요청은 실행하지 않고 `-32061`(결과 불명, 다시 보내지 말고 조회)로 답한다. 결과가 나오면 그 응답을 키에 기록한다.
- 호출자의 응답 시한·연결 종료·plugin 재시작은 이미 접수한 요청을 취소하지 않는다. 응답만 버려진다.
- 성공 응답·version·CAS 판정·`durable:false`·오류 코드는 writer 없이 바로 실행할 때와 같다. writer를 거치며 생기는 경우의 응답은 다음과 같다. 같은 표가 [API 레퍼런스](../../reference/api.md)와 [헤드리스 IPC 지원 범위](../../dev-guide/headless-ipc-surface.md)에 있다.

| 경우 | 코드 | 메시지 | `error.data` |
|---|---|---|---|
| 큐 포화(접수 거절, 다시 시도할 수 있다) | `-32603` | `memory writer queue is full (<요청 수> requests, <바이트> bytes held)` | `{"storage_failure":"busy","writer":"saturated"}` |
| 종료 중 접수 거절 | `-32603` | `memory writer is not accepting requests` | `{"storage_failure":"other","writer":"closed"}` |
| commit 결과 불명(원인 오류 있음) | 원인 오류의 코드 | 원인 오류의 기존 문구(`memory db error: …`) | 원인 오류의 `data`에 `"outcome":"unknown"`을 더한다 |
| worker 소실로 결과 불명 | `-32603` | `memory writer ended before the request finished; it may have been stored` | `{"storage_failure":"other","outcome":"unknown"}` |
| 요청 본문 panic | `-32603` | `memory request panicked` | `{"storage_failure":"other"}` |
| 저장 실패·busy 재시도 시한 초과·요청 거절 | 기존 매핑 그대로 | 기존 매핑 그대로 | 저장 실패만 `storage_failure` |

### 요청 게이트

외부 IPC와 plugin host-call은 라우팅 전에 요청 게이트(`src/adapters/ipc/handler/gate.rs`)를 지난다. 저장이 필요한 단계는 writer에 요청을 넣고, 진입 경로는 요청과 진행 상태를 게이트 대기열(`src/app/request_gate.rs`)에 보관한다. 메인 루프는 결과를 기다리지 않고 회차마다 맨 앞 요청의 결과만 확인한다. 결과가 나오면 같은 진입 함수로 다시 들어와 남은 단계부터 진행한다.

- 접수 순서: 대기열에 요청이 있으면 새 IPC 명령과 plugin 호출은 저장소가 필요 없어도 그 뒤에 선다. 맨 앞 요청이 끝나기 전에는 뒤 요청을 시작하지 않는다. 앞 요청이 바꾼 상태(세션 철회 등)를 뒤 요청의 게이트가 본다.
- 종료: 종료가 시작되면 대기열의 요청은 실행하지 않고 `-32000 host is shutting down`으로 답한다. 이미 넣은 게이트 요청(세션 정리·rate limit 소비)은 확정될 수 있다.
- 토큰 없는 Local 요청은 세션 판정을 하지 않는다. cap 차단은 plugin 호출자만 대상이고(Local alias·Agent는 차단하지 않는다), 기록 단계의 cap 재평가는 `_host`가 아닌 모든 agent ID에 적용한다.
- `split`의 문자열 `target_surface`는 접수할 때 nickname으로 찾아 surface ID로 바꾼다. 이후 소유 창 찾기·journal 해석은 저장소를 읽지 않는다.

단계별 실패 경계는 다음과 같다. 선택 조회와 기록의 실패는 새 거절을 만들지 않는다.

| 단계 | writer 요청 | 확정하지 않음(저장 실패·같은 묶음의 이웃 실패) | 결과 불명 | 접수 거절 |
|---|---|---|---|---|
| 세션 판정(토큰이 있을 때) | 실행 단위. 만료 세션 삭제·만료 grant 정리를 함께 한다 | 같은 판정을 새 요청으로 다시 넣는다(판정은 다시 실행해도 같다). 세 번 모두 확정하지 않으면 쓰기 없는 강한 읽기로 판정한다. 만료 grant를 지워야 하는 세션은 그때 거절한다 | 같은 판정을 다시 넣는다 | `Closed` → `-32000 host is shutting down`, `Saturated` → `-32000 host is busy: …`(`data`: `{"storage_failure":"busy","writer":"saturated"}`) |
| 쓰기 없는 판정 | 강한 읽기(단독 실행) | `-32001` 거절(직접 읽기의 조회 실패와 같다) | 해당 없음 | 위와 같다 |
| cap·nickname 조회 | 강한 읽기 하나 | cap을 차단에 쓰지 않는다(fail-open). nickname은 찾지 못한 대상이 된다 | 해당 없음 | 조회 없이 진행한다 |
| 기록(rate limit 소비·ipc_calls 기록·cap 재평가) | 실행 단위 하나 | 요청을 허용한다. 소비·기록은 남지 않는다 | 요청을 허용한다 | 요청을 허용한다 |

세션 판정 본문 안의 형식 오류처럼 다시 해도 같은 조회 오류는 확정된 판정(세션 없음)으로 바꾸고 경고를 남긴다. 저장 실패만 writer 요청의 실패로 돌려 다시 넣는다. 감사·anomaly 기록과 로그 정리는 기다리지 않는 별도 요청이라 게이트 결과에 영향이 없다. 실패 경계는 실제 SQLite 저장소에 실패를 넣는 시험(`gate_tests.rs`)이 고정한다.

### 보존할 실행 계약

- `agent.task_graph_submit`은 저장소가 in-memory 대체 상태이고 요청 durability가 required이면 잠금을 잡기 전에 `StoreNotDurable`(-32602, `/durability`)로 거절한다. dry-run도 같다.
- runner는 lease·semaphore 획득 결과로 dispatch 여부를 정한다. 획득은 읽기-판단-쓰기를 writer 요청 하나 안에서 하므로 두 runner가 같은 이전 상태를 보고 같은 permit을 얻지 않는다. 이 판단을 읽기와 쓰기로 나누면 이 보장이 사라진다.
- runner는 획득이 확정된 결과로만 프로세스를 시작한다. 접수 거절(종료 중)은 바뀐 것이 없어 다음 tick으로 미루고, 확정되지 않은 저장 실패는 기존 저장 오류와 같이 다룬다. 결과 불명이면 같은 holder의 점유를 돌려준 뒤 다음 tick에 처음부터 얻는다(같은 holder의 재획득은 갱신이라 얻지 못했던 경우에도 무해하다). 후처리 실행은 시작 기록이 확정된 뒤에만 띄운다. permit 획득 단위가 결과 불명으로 끝나도록 저장 실패를 주입한 시험이 그 회차에 프로세스가 없고 점유가 돌아가며 다음 회차에 한 번만 실행됨을 확인한다(`tasty_memory::testing::faults`, `test-support` 기능).
- graph submit의 stage 실패는 롤백하고, activate 뒤 단계 실패는 `GraphPartiallyActivated`로 알린다.
- in-memory 대체 상태의 쓰기 성공 응답은 `durable:false`를 더한다(이 문서).

## writer 명부

### SQLite DB

| DB | 소유자 | 쓰는 스레드 | PRAGMA | transaction | GUI 초기화 | headless 초기화 | 종료 |
|---|---|---|---|---|---|---|---|
| `memory.db` | memory writer(`AppServices`의 writer가 저장소를 소유한다) | `memory-writer`. 메인 루프·runner·결과·보고 스레드·journal worker는 요청만 넣는다 | WAL·NORMAL, 되읽어 degraded 표시 | put·delete·secret·purge·prune은 명시 transaction, VACUUM은 자동 commit. 실행 단위·쓰기 묶음은 transaction 하나(memory writer가 사용) | `run_gui`가 열고 부팅 정비 후 `App::new`에 주입. 실패 시 in-memory 대체하고 첫 창 부팅 완료 때 경고 알림을 남긴다. writer는 `AppServices` 구성 때 시작 | `boot_memory`가 같은 절차로 열고, 대체했으면 첫 engine 준비 직후 경고 알림을 남긴다 | writer는 producer 정리 뒤 drain·join하고, worker가 끝날 때 소유한 연결을 명시 close·checkpoint 없이 drop으로 닫는다 |
| `state.db` | state writer(`AppServices`의 writer가 연결을 소유한다) | `state-writer`. 메인 루프(최근 파일)와 View(튜토리얼)는 요청만 넣는다 | WAL·NORMAL | 스키마 생성과 부팅의 최근 목록 정리는 각각 transaction. 평상시 묶음 하나가 transaction 하나(최근 목록 저장, 튜토리얼 저장) | GUI 부팅의 `init_boot_db_and_theme`에서 실행당 한 번 열고 최근 목록을 읽은 뒤 writer를 시작한다. 열기나 writer 시작에 실패하면 경고 알림을 남기고 DB 없이 계속하며 새 창에서 다시 시도하지 않는다 | 열지 않는다. 최근 목록 저장을 건너뛴다 | writer는 producer 정리 뒤 drain·join하고, worker가 끝날 때 소유한 연결을 drop으로 닫는다 |
| `structure/journal.db` | `JournalWorker`(App의 첫 필드) | 전용 `structure-journal` 스레드 | WAL·FULL이 아니면 열지 않는다([이벤트 저장소 (`tasty-event-store`)](../../architecture/event-store.md)) | 즉시 잠금 transaction과 writer 잠금·세대 검사. 준비된 순수 요청은 요청별 savepoint로 물리 transaction만 공유하고 논리 cut·ACK는 각각 유지. 예약·claim·capture 등은 barrier | 부팅 첫머리에 writer 잠금을 선점하고 `App::new`에서 worker를 만든다 | 선점 없이 `App::new_headless`에서 만든다 | stop 후 Drop에서 join. 확정 모델과 ACK된 cut이 같을 때만 checkpoint하며 publication halt 때는 생략한다 |

### 파일

파일 교체 시험은 Linux의 권한·ACL·확장 속성, Windows의 보호된 security descriptor·High/Low mandatory integrity label과 읽기 전용 거절, macOS의 사용자 ACL·확장 속성을 파일 쓰기 경계에서 대조한다. 이 시험은 앱 실행이나 입력 자동화를 하지 않는다.

설정 파일의 읽기·원본 보호·직렬화·교체 구현은 `tasty-settings`의 `persistence` 모듈에 모여 있다.

원자 교체는 임시 파일 작성 뒤 rename·persist를 뜻한다. 공용 교체 함수(`tasty_utils::file_replace::replace`)는 같은 디렉터리에 고유 이름의 임시 파일을 만들어 쓰고 대상 이름으로 바꾼다. 실패하면 원본을 남기고 임시 파일을 지운다. 대상이 symlink면 링크를 따라가 실제 파일을 교체하고 링크는 그대로 두며, 링크가 가리키는 파일이 없으면 오류다. 교체 전에 기존 대상에 쓰기 접근이 가능한지 확인한다. Unix 임시 파일은 제한된 권한으로 만들고 내용을 쓰기 전에 owner/group·권한을 옮긴다. Linux는 ACL을 포함한 확장 속성도 옮기고, macOS는 native copyfile의 ACL·확장 속성 복사를 쓴다. Windows는 원본 owner/group/DACL과 mandatory integrity label을 임시 파일 생성에 사용한다. 내용을 쓰기 전에 원본과 임시 파일의 실제 label ACE도 비교한다. label 조회·보존에 실패하면 권한을 추가로 얻지 않고 원본을 유지한 채 오류를 반환한다. 이 속성의 보존이 실패하면 원본을 교체하지 않는다. Windows의 대체 데이터 스트림 등 나머지 파일 속성은 이 함수의 보존 범위가 아니다. 기본은 fsync를 하지 않으므로 전원 장애 내구성까지 보장하지 않는다. 같은 모듈의 `lock`은 `<파일>.lock` 옆 파일의 배타 잠금으로 Tasty writer끼리 읽기-변경-교체를 직렬화하고, `Fingerprint`는 길이와 내용 해시로 외부 변경을 확인한다. 잠금에 참여하지 않는 외부 편집기는 막지 못하고, 마지막 확인과 교체 사이의 변경도 막지 못한다.

| 파일 | 쓰는 함수 | 스레드 | 원자 교체 | 다른 writer |
|---|---|---|---|---|
| `config.toml` | `SettingsStorage::save` → `Settings::save` | file writer(부팅 정규화·셸 확인·테마 조정 포함) | 예(공용 교체, fsync 없음). 해석하지 못한 원본은 저장 직전 백업 | 같은 대상 잠금과 원본 변경 관측 |
| 사용자 선택 keybinding bundle(`tasty-keybindings-*.toml`) | Settings Export의 소유된 ordered command | 기존 file writer | 예(공용 교체, 기본 fsync 없음) | 대상 참여 잠금. 설정 snapshot과 병합하지 않음 |
| `presets/**.toml` | `PresetStore`의 관측값 검사·공용 교체 | 실행 중 명령은 file worker, 초기 읽기·ID 보정은 load 호출 스레드 | 예(fsync 없음) | 저장 이름 선택의 참여자 잠금 + 대상 잠금·관측 충돌 검사 |
| `port-favorites.toml`·`explorer-favorites.toml` | `SharedList::update`가 잠금 안에서 snapshot 접수, file writer가 교체 | file writer | 예(공용 교체, fsync 없음) | 참여자 잠금·원본 fingerprint/링크 대상 비교 |
| `child-terminals.json` | engine의 변경 필드·예약·등록·슬롯 정리를 공용 ordered command로 접수 | file writer. 최초 읽기·정리는 부팅 worker/headless bootstrap barrier | 예(공용 교체, fsync 없음) | 참여자 잠금 안에서 최신 읽기·engine 소유 변경 필드 병합·교체 |
| `tasty.port` | `publish_port_file_to` 완료 결과를 listener가 소유 | IPC 시작 완료 barrier | 예(공용 안전 교체, fsync 없음) | notify 정리 → 관측 원본/링크 비교·참여 잠금 → 완료 공개. 서버 Drop은 같은 잠금을 비차단 획득한 동안 열린 파일 identity·내용이 같은 공개 세대만 정리. 경합/identity 불명은 보존 |
| 단일 실행 instance 파일 | 본체 `instance_file`이 소유한 기록을 `tasty_ipc::instance_file::write`로 교체 | 메인 루프 | 예(sync 포함) | GUI만 쓴다 |
| `webhooks.toml` | 호스트 `webhook::persist::WriteRequest`, CLI `webhook_port_file` | runtime port·registry는 기존 file writer, CLI·초기 prepare는 동기 실행 | 예(공용 교체, fsync 없음) | host 배열 교체·CLI port 변경·옛 포트 마이그레이션이 같은 참여자 잠금 안에서 최신 TOML을 읽고 해당 절만 변경 |
| `remote-profiles.toml` | `RemoteFileStore::profile`의 소유 Put/Insert/Patch/Remove/Rename | 실행 중 IPC·GUI·탐지 상태는 file worker, standalone CLI는 호출 스레드 | 예(공용 교체·sync) | 최신 TOML 참여 잠금·조건부 expected patch. 원 receipt 뒤 응답·SSH 효과, 서로 다른 파일의 transaction은 없음 |
| `passkeys.toml`과 `passkeys/<이름>` | `RemoteFileStore` passkey/insert_passkey/rename_passkey | 실행 중 IPC·GUI는 file worker, standalone CLI는 호출 스레드 | 목록은 공용 교체·sync. inline은 private temp·권한/ACL 검증 뒤 write·sync·rename | 목록 → inline 고정 잠금. 새 이름 중복은 payload 전에 거절하며, 목록 저장 뒤 실제 관리파일 정리 결과와 부분 성공을 owned receipt로 구분함 |
| `plugins.toml` | `ConfigUpdate::execute`가 비활성·제거 표시, 권한·단축키 변경을 병합 | 런타임은 공용 file worker; 동기 bootstrap·library 경계는 아래 설명 | 예(공용 안전 교체·sync) | 참여 잠금 안에서 최신 문서에 변경 의도를 반영하고 원본 fingerprint·링크 대상을 확인. receipt 뒤 cache·런타임 효과 적용 |
| `hook-handlers.toml` | hook registry가 사용자 snapshot 접수, file writer가 교체 | file writer | 예(공용 교체, fsync 없음) | 참여자 잠금·원본 fingerprint/링크 대상 비교 |
| `file-handlers.toml` | `FileRules::submit`이 합친 snapshot 접수, file writer가 교체 | file writer | 예(공용 교체, fsync 없음) | 참여자 잠금·원본 fingerprint/링크 대상 비교 |
| `themes/mocha.toml`·`themes/latte.toml` | `ThemeFiles` 초기 seed·sync·명시 복구 | 첫 로딩 프레임 전 bootstrap barrier | 예(공용 안전 교체, fsync 없음) | 참여 잠금·관측한 원본/링크 비교. 읽기·UTF-8 실패 원본 보호. runtime fallback은 내장 색상만 적용 |
| `known-plugins.toml` | `file_persistence::update_known`·`KnownPlugins::save` | 런타임 설치·신뢰·재승인·제거는 같은 file worker; 동기 경계는 아래 설명 | 예(공용 안전 교체·sync) | 참여 잠금 안에서 최신 신뢰 문서를 ID별 병합. 조건부 undo는 이후 선택을 보존하며 두 plugin 파일 사이 transaction은 없음 |
| `debug.log`·crash/hang 보고·`launch.log`·observer file sink | 로그 기록 | 각 기록 스레드 | append | `launch.log`는 두 번째 프로세스가 파일 잠금 안에서 쓴다 |

plugin 설정·신뢰 파일의 [런타임 저장](../../dev-guide/plugin-persistence.md)은 공용 writer의 실제 receipt 뒤 다음 효과로 이어진다. GUI boot worker와 headless pre-loop의 builtin 설치, 최초 headless attach 등을 위한 lazy builtin bootstrap 재시도, 기존 동기 library API는 같은 안전 backend의 파일 완료를 기다리는 lifecycle barrier로 남는다. 모든 동기 호출이 부팅 전에만 실행되거나 공용 writer로 이행된 것은 아니다.

host의 hook 사용자 설정은 위 `hook-handlers.toml` snapshot 경계에 속한다. 기존 `save_user_config`는 test-only이며 초기 사용자 설정 설치는 메모리 구성이다. Claude·Codex의 명시적 훅 install/uninstall은 plugin 프로세스에서 외부 에이전트 설정·셸 연동 파일을 동기로 수정하는 별도 경계다. 이 외부 파일에 host 공용 writer·참여 잠금·원자 교체가 적용됐다는 뜻은 아니다.

원격 inline 비밀 파일은 기존 private temp·권한/ACL 검증·write·sync·rename 경로를 유지하며,
파일 worker의 관측 scope 안에서만 별도 Replace attempt를 기록한다. 검증·원본 읽기·참여 잠금
뒤 temp 생성 직전에 시작하고, temp 생성 실패를 포함한 반환 오류의 I/O 종류로 완료한다.
반환 전에 unwind하면 Unknown이다. 실패한 교체는 published bytes를 늘리지 않는다.
관리 사용자 파일 unlink는 Delete지만 private temp cleanup은 제외한다. CLI·부팅·관측 scope 밖
호환 호출은 이 writer 계측에 더하지 않는다. 여러 파일의 물리적 성공·실패 수는
`MutationFailure`·`ManagedCleanup`의 부분 성공 결과나 host receipt 적용을 대신하지 않는다.
host profile/passkey 명령과 탐지 상태 patch는 같은 worker scope에서 실제 물리 작업과 typed 결과를 별도로 센다. 탐지 준비 읽기는 물리 교체가 아니며, strict read·조건부 patch가 helper 전에 거절하면 시도 수는 0이다. CLI 호출은 worker 통계에 포함하지 않는다.

child registry의 engine별 view는 메모리 원본이며 디스크 작업은 기존 file writer의 소유 명령으로만
실행한다. 메인 루프는 파일·참여자 잠금을 기다리지 않고 receipt를 거둔다. 접수된 revision만
accepted 기준을 전진시키고, 늦게 받은 확정 결과에는 이후 접수한 변경 필드와 아직 접수하지 않은
명시적 의도를 순서대로 다시 반영한다. engine과 응답 수신자가 사라져도 Hub가 접수된 명령의
결과를 거둔다. 보관 중인 delta·완료·close intent는 기존 writer Limits의 항목·바이트 예산으로
제한하며 payload를 로그나 계측에 넣지 않는다.
삭제가 확정된 상태 필드의 transient 대입 metadata는 함께 제거한다. 늦은 ACK를 병합할 때는
이후 접수한 대입과 아직 접수하지 않은 대입을 먼저 재적용하여 현재 필드의 의도는 유지한다.

번호 예약은 실제 교체가 성공한 뒤에만 index를 공개한다. adopt는 최신 번호 할당과 관계 등록을
같은 파일 교체에 넣는다. GUI와 headless는 파일 receipt를 먼저 거두고 현재 engine의 점유
continuation을 적용한 다음 원 호출자 응답을 보낸다. engine이 은퇴했거나 대상이 바뀌면 파일 저장과
live 적용을 구분한 오류로 끝낸다. spawn의 구조 준비는 예약 receipt 뒤, 설치된 child의 command 입력은
관계 receipt 뒤다. 원 호출자의 연결 종료는 이미 접수한 파일 명령을 취소하거나 재실행하지 않는다.
예약을 기다리는 spawn plan은 private pending 요청에만 보관하며 구조 준비나 공개에 사용하지 않는다.
예약 receipt가 확정한 index로 `child{index}` 탭 제목과 명시 이름을 한 번 정한 뒤 구조 ID 예약·준비를
진행한다. 대기 중 요청과 해소된 plan의 실제 직렬화 크기는 기존 pending 바이트 재계산·상한 검사를
각각 통과하며, 아직 알 수 없는 index를 가짜 이름으로 미리 조립하지 않는다.

아직 index가 공개되지 않은 adopt 뒤에 surface가 닫혀도 삭제 의도를 보관한다. file writer가 가득
찼으면 이 의도의 접수만 다시 시도하며 실행 결과가 불명인 명령은 재실행하지 않는다. 종료는 이
producer 의도를 접수한 뒤 기존 writer의 admission을 닫는다. 기존 종료 기한을 넘기면 pending·실패·
결과 불명을 최종 관측에 남기며 join이나 파일 성공을 만들어 내지 않는다. 요청한 프로세스 종료
코드와 파일 명령의 성공은 별개다.

자식 관계의 필드 수정은 최신 파일에서 이미 삭제된 관계를 재생성하지 않는다. 새 관계 등록과
기존 관계 수정은 별개다. 명시적인 idle·needs_input 보고는 캐시와 같은 값이어도 해당 필드에
새 대입 의도를 남기며, active 보고에 포함되는 needs_input 해제도 함께 반영한다. 그 밖의
외부 수정 필드는 최신 값을 보존한다. 초기 읽기와 잠금 안의 최신 읽기는 해석된 대상의
metadata가 일반 파일인지 먼저 검사한다. 없는 파일의 초기화와 일반 파일을 가리키는
symlink는 허용한다. 참여하지 않는 프로세스가 이 검사와 실제 open 사이에 대상을 바꾸는
TOCTOU는 이 검사로 제거되지 않으며 파일 참여자 잠금 계약 밖의 경합이다.

`webhooks.toml` backend는 소유된 webhook 섹션 요청만 받고 전체 TOML snapshot은 접수하지 않는다.
파일 잠금 안에서 최신 원본을 읽고 병합·직렬화하며, 교체 직전 원본 fingerprint와 링크 대상을
다시 확인한다. 실패·손상·읽기 불가는 빈 테이블로 덮지 않는다. 참여하지 않는 편집기의 변경은
마지막 확인까지 감지하지만 확인과 rename 사이 경합은 공용 교체 함수의 한계로 남는다.
새 파일은 Unix 0600이고 기존 파일의 native 권한·소유권을 보존한다. TOML 주석·공백의 바이트
보존은 보장하지 않는다. parse 오류에는 원문을 넣지 않는다.

실행 중 port 조회·변경은 기존 ordered file command로 접수하고 receipt 뒤 원 응답을 확정한다.
조회의 옛 형식 migration도 worker에서 처리한다. 부팅 prepare는 소켓 선점 입력이므로
리스너 시작 전에 포트 읽기·migration을 마치는 동기 barrier다.
backend 결과가 돌아오면 파일 잠금은 이미 풀려 있다. 프로세스 소유 webhook registry는 앞선 receipt 뒤에 다음 변경 절을 만들고, 등록·해제는 저장 성공 뒤에만 공개한다. registry 잠금은 준비·정산에만 쓰고 I/O와 HTTP 요청 스레드의 receipt 대기에는 유지하지 않는다. 원 IPC 호출자는 기존 file continuation으로 한 번 응답받는다. Count/Time·만료 정리의 알려진 backend 저장 실패는 경고 후 메모리 차감·제거와 HTTP ACK/실행을 계속하는 기존 정책을 유지한다. 접수 거절·Unknown에는 적용하지 않으며 HTTP503/실행0, IPC saved:false 또는 saved:null/outcome_unknown:true로 구분한다. Unknown 뒤에는 후속 변경을 재실행하지 않고 새 프로세스 복원 전까지 거절한다. 별도 pending·실행 절·응답과 HTTP 실행 문맥은 개수·추정 직렬화 바이트를 제한하며 결과 소비/실행 종료까지 예산을 유지한다. `system.pressure.webhook_registry`는 해당 접수·정산과 보관 수·바이트, dirty·outcome_unknown을 원문 없이 보여준다. 메모리 차감 뒤 알려진 저장 실패는 dirty로 남고 다음 성공한 절 저장에서 해제한다. 작업 회수와 저장 성공은 별개이며, 종료 시 dirty·Unknown이 남으면 별도 경고한다. 부팅 복원·만료 정리 receipt 뒤에 accept를 시작하고, 종료에는 새 접수를 닫고 해당 producer/receipt를 회수한 뒤 공용 writer를 닫는다.
종료 중 새 HTTP 시퀀스 IPC는 GUI·headless 모두 종료 오류로 답해 실행을 재개하지 않고 대기 producer를 회수한다.
standalone CLI는 파일 처리 결과를 동기로 기다린 뒤 기존 명령 출력을 만든다.
port 변경과 형식 마이그레이션은 한 번에 교체해
마이그레이션만 먼저 저장되는 부분 성공을 만들지 않는다. 포트 저장·삭제는 기존처럼 format=2를
기록하며, host 배열 교체는 원래 format 값을 바꾸지 않는다. 부팅의 옛 포트 읽기에서 마이그레이션
저장만 실패하면 원본은 남기고, 그 읽기에는 기존처럼 메모리에서 해석한 포트를 쓴다. 잠금 파일을
만들거나 열지 못해도 파일을 다시 읽어 최신 유효 문서의 포트만 메모리에서 해석하며, 잠금 밖에서
저장하지 않는다. 최신 읽기에서 삭제·손상·읽기 불가를 확인하면 최초 읽기의 포트를 재사용하지 않는다.

`notify/<surface>.log`는 호스트가 쓰지 않는다. 플러그인 프로세스가 쓰고 호스트는 IPC 시작 때 디렉터리를 정리한다.

Settings Keybindings Export는 현재 draft와 plugin override를 인코딩한 불변 text/path를 기존 file writer의 명령으로 접수한다. 항목·바이트 예산은 receipt 소비까지 유지하며 사용자 Export 명령을 snapshot으로 합치지 않는다. 원래 Settings View의 수명과 요청 identity, bundle draft 지문이 같은 경우에만 완료를 표시한다. Cancel·닫기·새 draft는 표시 observer만 버리고 접수된 쓰기는 취소하지 않는다. 일반 회차와 최종 file drain은 완료를 거두며, 결과 불명은 성공이나 알려진 실패로 바꾸지 않고 자동 재실행하지 않는다. 알려진 성공만 경로 토스트를 표시한다. 거절·저장 실패는 기존 행의 재시도/다른 위치 선택으로, 결과 불명은 대상 확인 안내로 구분한다. 명시적 재시도는 그때의 draft를 한 번 인코딩한다. Import의 파일 읽기·decode는 기존 동기 경로이며 이번 writer 명령이 아니다.

Export는 공용 교체 함수가 지원하는 원본 권한·소유권·native 보안 속성을 보존한다. 지원하지 않는 모든 metadata나 마지막 원본 검사 뒤의 외부 편집 방지, 여러 파일 transaction, 전원 장애 내구성을 약속하지 않는다. 알려진 교체 실패는 대상의 기존 내용을 보존하지만 결과 불명에는 대상 미변경을 주장하지 않는다.

### Child terminal 관계의 파일 병합

Child registry는 읽거나 저장을 완료한 기준값과 비교하여 엔진이 소유한 surface 관계·상태·슬롯의 변경 필드만 최신 파일에 반영한다. 다른 엔진의 기록과 변경하지 않은 필드의 외부 편집은 유지한다. External 부모는 자식 surface 소유권으로 병합하며 다음 번호는 다른 엔진의 미등록 예약까지 보존한다. 전체 파일이나 소유 항목 snapshot의 마지막 값으로 대체하지 않는다. 변경되지 않은 번호는 다시 쓰지 않으며 번호 증가는 최신 값과 큰 값을 따른다. 삭제 상한은 알고 있는 자식 번호에서 구하고, 캐시에 보이는 다른 엔진의 미등록 예약까지 삭제 상한으로 삼지 않는다. 번호 예약도 같은 참여자 잠금 안에서 최신 파일을 읽고 증가·교체까지 끝낸 뒤 번호를 반환한다. 실패한 예약은 번호를 반환하거나 호출자의 캐시를 전진시키지 않는다.

저장은 같은 디렉터리의 임시 파일과 공용 원자 교체 함수를 사용한다. 초기 읽기·파싱 실패는 관찰용 빈 상태로 시작하되 해당 load의 쓰기를 금지한다. 저장 직전의 최신 파일 읽기·파싱 실패도 원본을 보호한다. 초기 읽기 실패를 외부에서 파일을 교체하는 것만으로 해제하지 않는다. 파일이 실제로 없을 때만 빈 registry에서 시작한다. symlink 대상과 내용 fingerprint를 참여자 잠금 획득 뒤 및 교체 직전에 검사한다. 참여하지 않는 편집기의 마지막 확인과 rename 사이 경합은 공용 교체 함수와 같은 한계다. 이 경계의 호출자는 현재 동기로 완료를 기다린다.

### port를 우회하는 직접 호출

- `config.toml` 저장은 부팅·실행 중 모두 `AppServices`의 file writer가 주입된 `SettingsStorage`를 호출한다. 아래 여섯 경로에 동기 저장 예외는 없다. 설정 읽기와 테마 파일 자체의 초기화는 이 설정 writer 범위와 별개다.

| 기존 직접 저장 경로 | 현재 저장 경계와 순서 |
|---|---|
| 첫 창 `boot_load_normalized_settings` | 메모리 정규화만 한다. 최종 테마 조정 뒤 한 snapshot으로 제출한다 |
| 셸 확인 `finish_shell_setup_confirmed` | 확인한 셸을 메모리에 반영하고 부팅에 넘긴다. 테마 조정 전 snapshot을 따로 제출하지 않는다 |
| 부팅 `init_boot_db_and_theme` | 테마를 적용한 뒤 `begin_boot`가 최종 snapshot을 file writer에 제출한다. 로딩 프레임을 계속 그리며 저장 결과를 받은 뒤 engine/plugin producer를 시작한다. 저장 실패는 로그와 dirty로 남기고 메모리 설정으로 부팅을 계속한다 |
| 추가 창의 정규화 저장 | 마지막 제출 설정(저장 대기·실패 상태 포함)을 사용해 정규화한다. 디스크의 이전 snapshot으로 대기 변경을 덮지 않는다 |
| 추가 창의 테마 조정 저장 | 정규화와 테마 조정 결과가 바뀌었으면 같은 writer에 한 번 제출한다. 이벤트 루프는 교체를 기다리지 않는다 |
| Lua `record_script_hash` | 승인한 설정 snapshot을 같은 writer에 제출한다. 이벤트 루프는 멈추지 않고 저장 receipt 결과를 확인한 뒤 승인한 source를 실행한다. 저장 실패가 실행을 막지 않는 기존 계약은 유지한다 |

부팅 barrier는 파일 잠금이 아니라 최종 snapshot의 receipt를 확인한다. 파일 잠금만으로는 먼저 접수한 snapshot과 나중 동기 저장의 의미 순서를 정할 수 없다. 내부 부팅 저장도 출처가 없어도 receipt 결과를 한 번 돌려주며, 실패한 최신 revision은 종료 재시도 대상이다.
- 사용되지 않던 `PresetStorage`의 호스트 주입은 없다. 프리셋 저장·삭제·이름 변경은 같은 file writer의 소유된 ordered command로 실행한다.
- 프리셋의 소유값 저장 명령은 자동 이름 선택부터 교체까지 root 참여자 잠금을 유지하고 성공한 값만 별도 캐시에 메모리 반영할 수 있다. 초기 읽기의 관측값과 손상·읽기 실패 보호는 저장소 clone에도 이어진다. rename/delete도 root 잠금 안에서 원본 관측값을 검사한다. rename은 새 파일 확정 뒤 원본 정리가 실패하면 typed 부분 실패와 확정된 캐시 변경을 반환한다. 파일 사이 transaction이나 rollback은 없다. 호스트는 같은 초기 캐시에서 복사한 worker 전용 저장소로 이 blocking API를 실행하고, receipt를 접수 순서대로 거두어 표시 캐시에 메모리 반영한 뒤 응답한다. rename delta는 kind·이전/새 이름·원본 삭제 확인 여부만 보관하며, 순서가 보장된 표시 캐시에서 값을 이동하거나 부분 성공 시 복사한다. 캐시 반영 자체가 실패하면 저장 결과와 캐시 실패를 구분해 알린다. 초기 load의 ID 보정은 호스트에 노출할 정규화 값을 결정하는 load 단계에서 끝내며, 보정 저장 실패 시 메모리 값을 유지하고 경고한다.
- 테마 seed/sync는 `ThemeFiles` backend를 호출하는 bootstrap 자유 함수로 첫 로딩 프레임 전에 완료한다. 첫 렌더에 사용할 테마를 확정하는 barrier이며 runtime 저장 예외가 아니다. 추가 창은 같은 초기화를 반복하지 않고 캐시/내장 fallback을 적용한다. runtime rescan은 읽기만 하며 누락·잘못된 테마를 자동으로 쓰지 않는다.
- memory.db의 부팅 정비는 port로 감싸기 전 구체 `MemoryStore`를 쓴다.
- state.db에는 port가 없다. 최근 파일과 튜토리얼은 state writer의 요청 타입을 직접 쓴다.

### 종료 순서

GUI는 종료 상태기계가 layout 저장(S1) → 부팅 worker 회수(S2) → surface 닫기와 observer sender 종료(S3) → observer 완료 폴링과 플러그인·보조 worker 정리, 그 뒤 memory writer·state.db writer·file writer의 drain·join(S4) 순으로 진행한 뒤 이벤트 루프를 끝낸다. 그다음 instance 파일을 지우고 App을 drop한다. App의 첫 필드인 journal worker가 먼저 join되고, `services`(memory·state·file writer)와 IPC 서버(포트 파일 삭제)는 그 뒤에 drop된다.

headless는 publication을 비운 뒤 journal worker를 stop하고(join은 App drop 때), runner·보조 worker를 회수한 뒤 file writer, memory writer를 차례로 drain·join한다. layout 저장·state.db writer 단계는 없다. observer producer는 writer drain 전에 완료를 폴링하며, 시한 뒤 새 memory 제출을 차단하고 미회수를 남긴다.

memory.db 쓰기는 모두 memory writer를 지난다. writer는 종료 때 접수를 막고 마지막 flush cut까지 실행한 뒤 join하며, 소유한 저장소를 그때 닫는다. 종료 요청보다 늦게 도착한 요청 게이트 대기 요청은 `-32000`으로 거절한다([요청 게이트](#요청-게이트)).

저장 실행기의 `shutdown_observation()`은 `NotStarted`·`Draining`·실제 join 뒤 `Joined { panicked, flush }`를 보존해 읽기만 한다. 최종 flush receipt는 shutdown polling이 한 번 거두고 결과를 캐시한다. receipt 유실은 `Lost`이며, flush 성공 뒤 worker가 panic한 경우도 `panicked=true`를 유지한다. 프로세스 종료 코드와 writer의 join·저장 성공을 각각 판정한다.

### Task 종결 알림의 영속 의무

알림 전달 의무와 실행 handle 정리는 별개다. 알림의 정확 UUID를 확인 처리해도 남은 handle의
정리 의무는 끝나지 않는다. 러너 없는 취소의 후속 확인은 현재 종결 task 레코드와 남은 handle을
따로 읽어 정리를 시작한다. 다른 요청이 알림을 먼저 소비해도 이 확인을 생략하지 않는다. 정리는
기존 handle의 회차·프로세스를 확인하고 종료가 확인된 뒤 자기 점유만 반환하며 외부 실행을
재시도하지 않는다. 별도 schema나 알림 재발행으로 정리를 대신하지 않는다.
정리 snapshot에는 같은 확인 읽기에서 얻은 task revision·회차와 handle 기록이 함께 들어간다.
신호 전 현재 종결 기록과 handle을 대조하고 캡처한 프로세스만 대상으로 삼으며, 종료 후 같은
기록인지 다시 대조한 단위 안에서 handle CAS 삭제와 자기 점유 반환을 한다. retry로 기록이
바뀌거나 handle이 교체됐으면 그 snapshot은 새 회차의 handle·프로세스·점유에 손대지 않는다.

TaskStore는 비종결에서 종결로 바뀌는 task 레코드와 알림 의무를 같은 atomic batch에 쓴다.
알림은 workspace의 `tasty.agent.terminal.<UUID>` 키에 schema 1, 고유 ID, 완료 통지용 snapshot,
레코드 revision을 담는다. snapshot은 task ID·생성 시각·상태·결과·회차 ID·건너뛴 이유만 담고
실행 정의와 입력·metadata는 복사하지 않는다. 전체 task를 담은 snapshot도 읽을 수 있으며 원본을
다시 쓰지 않는다. 별도 회차는 v1도 새 UUID를 받는다. 저장 용량 부족으로 의무를
쓸 수 없으면 종결 쓰기도 되돌린다. 기존 task 형식은 그대로 읽으며 이전 레코드를 다시 쓰거나
과거 종결을 일괄 발행하지 않는다. 알 수 없는 알림 schema와 손상된 레코드는 보존하고 오류를 남긴다.

호출 결과가 불명이어도 성공으로 바꾸거나 외부 실행을 재시도하지 않는다. task runtime은
후속 강한 읽기로 확인한 의무만 대기 허브와 사건 피드에 전달한다. 전달 후 정확한 UUID만
지운다. 이 확인 쓰기의 응답을 잃어도 같은 허브는 같은 ID를 다시 발행하지 않는다.
같은 허브의 중복 억제 토큰은 의무를 읽는 강한 읽기 안에서 확보한다. 지연된 읽기 결과도
같은 UUID의 공유 전달 상태를 보유하므로, 정확 UUID의 확인 쓰기가 확정되어 registry 항목을
회수한 뒤에도 다시 발행하지 않는다. 확인 실패·결과 불명 때는 토큰을 남기며, 정확한 멱등 확인
쓰기의 성공 또는 뒤이은 강한 읽기의 부재 확인 뒤에만 회수한다. 알 수 없는 schema나 실패한
읽기는 부재 확인으로 취급하지 않는다. 피드 drain·task purge·시간 경과는 회수 근거가 아니다.
registry는 미확인 의무에, 공유 토큰은 진행 중인 읽기 결과에 비례하며, 완료 이력 전체를 허브
수명 동안 보존하지 않는다. HashMap의 capacity는 최대 동시 부하 뒤에도 남을 수 있다.
이 토큰 회수는 취소 task의 handle 정리 의무와 독립이다.

재시작 때 남은 의무를 읽으므로 전달 직후 확인 쓰기 전에 프로세스가 끝난 경우에는 같은
`terminal_id`가 다시 발행될 수 있다. 소비자는 이 ID로 재전달을 구분한다. 피드는 기존의
휘발성 bounded 큐이며 외부 소비자의 수신 확인까지 영속 보장하지 않는다.

## 저장 실행기 관측

memory·state·file writer는 같은 `WriterStatsSnapshot`을 쓰고 `system.pressure`의 각 `*_writer`에 노출한다. 미구성 writer는 `null`이다. 계측은 저장·접수·재시도·ACK·종료 판단에 쓰지 않는다. 스냅샷 필드 전체는 원자적으로 읽히지 않는다.

- `admitted_bytes`는 접수한 요청의 입력 크기 추정 합, `batch_bytes`·`batch_bytes_max`는 snapshot 대체 전 묶음 입력 바이트 합·최댓값이다. 디스크에 쓴 바이트가 아니다. 기존 `batch_requests`와 `batch_sizes`도 대체 전 요청을 센다.
- `batch_wait`는 묶음의 가장 오래된 접수부터 worker의 묶음 처리 시작까지, `batch_execution`은 그 시작부터 결과 전달까지다. 후자는 busy 재시도 대기와 결과 전달을 포함하고 receipt 소비는 포함하지 않는다. `executor_execution`은 실제 executor 호출별 시간이며 재시도 대기를 제외하고 잡힌 panic 호출은 포함한다. 모두 같은 `Clock`을 쓴다.
- 세 시간 분포는 `samples`·`sum_us`·`max_us`와 고정 구간(`le_us`·`counts`)이다. 마이크로초 미만은 0으로 버리되 표본 수에는 포함한다. 경계는 포함 상한이고 마지막 칸은 초과 구간이다. 표본 0의 합·최댓값·칸 수는 0이며 평균이나 percentile을 관측했다고 해석하지 않는다. 원시 표본을 보관하지 않으므로 정확한 percentile은 계산할 수 없다.
- `completed_ok`·`completed_rejected`·`completed_failed`·`completed_unknown`·`completed_lost`는 접수한 요청의 종결 결과다. 대체된 요청도 대체한 요청의 결과로 분류한다. worker가 버린 queued ticket도 lost이며 receipt를 먼저 버려도 접수 작업은 계속된다. flush는 요청·묶음·종결 수에서 제외한다.
- `queued_items`·`queued_bytes`는 아직 묶음 처리에 dispatch하지 않은 요청의 수·입력 바이트다. 실행·재시도 중인 요청과 읽지 않은 완료 결과는 제외한다. flush도 제외한다.
- `pending_items`·`pending_bytes`는 아직 종결되지 않은 요청의 수와 원래 입력 바이트다. `oldest_pending_age_us`는 그중 가장 오래된 접수의 나이이며 없으면 `null`이다. 대기·실행·재시도·대체·receipt 포기를 거쳐 종결될 때까지 유지한다. 읽지 않은 완료 결과는 여기서 빠지지만 기존 `held_*` 예산에는 남는다.
- 고정 크기 원자 카운터·분포와 최대 `queue_items`개의 seq·시각·바이트만 보관한다. 경로·키·내용·오류 본문을 저장하지 않는다. 카운터 누계는 기존 u64 규칙을 따르고 시간 변환은 u64 최댓값으로 제한한다. metrics 잠금 안에서 저장소·디스크 I/O나 결과 대기를 하지 않는다.

generic 요청에는 강한 읽기도 있으므로 pending 나이는 소비자의 dirty revision 나이가 아니다. 실패나 결과 불명 요청은 종결되어 pending에서 빠져도 소비자의 미확정 변경은 남을 수 있다. 물리 commit 수·시간·쓰기 수는 `db`·`state_db`, 파일 교체 결과는 `file_writes`에서 따로 읽는다. `batches`나 `completed_ok`를 물리 commit 수로 해석하지 않는다. 소비자 dirty 상태와 journal 고유 계측은 각각의 소유자가 제공한다.
