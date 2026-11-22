# 이벤트 저장소 (`tasty-event-store`)

## 원칙과 현재 동작

[복구 원칙](../principles/feedback-and-recovery.md)은 복원 데이터를 읽지 못해도 오류 안내 후 초기 상태에서 작업을 계속할 수 있어야 한다. 아래 저장소의 잠금·schema·내구성 검사는 원본 보호를 위한 현재 계약이다. GUI에서는 부팅 중 저장소를 열지 못하거나 첫 View 공개 전에 복원이 실패하면 App이 원본을 보존하고 이번 실행에만 쓰는 임시 저장소로 계속한다([레이아웃 저장](../features/layout-persistence/index.md#원칙과-현재-동작)). 홈 사용 중 거절은 원본 보호를 위해 GUI에서 부팅 오류로 남는다. 헤드리스는 임시 저장소로 바꾸지 않는다. 부팅 첫머리에서 writer 잠금이 잡혀 있음을 확인하면 저장소를 열기 전에 stderr 문구와 종료 코드 1로 거절하고, 잠금을 판정하지 못해 worker가 실패하면 그 부팅 오류를 호출자에게 돌려주고 끝난다([데이터 홈 단일 소유](../features/layout-persistence/index.md#데이터-홈-단일-소유)). 엔진이 묶인 뒤의 실행 중 halt는 사유와 재시작 필요를 담은 오류 알림 하나와 오른쪽 위 비포커스 안내로 알린다([레이아웃 저장](../features/layout-persistence/index.md#실행-중-halt-안내)). halt 뒤에도 입력과 작업을 계속하게 하는 경로는 아직 제품 원칙의 미충족이며, 저장소 검사를 우회하거나 원본을 덮어쓰라는 뜻은 아니다.

데이터 홈 구조 journal의 저장 계약 중 수치·형식·보조 규칙과, App이 그 journal을 원본으로 연결한 현재 상태를 적는다. 저장 형식과 제약의 이유는 아래 각 절에,
저장소가 제공하는 기능의 개요는 [아키텍처](index.md)의 `tasty-event-store` 문단에 있다. 구현은 `crates/tasty-event-store/src/`다.

### 여러 논리 요청의 물리 transaction 공유

`begin_commit_group`은 기존 writer 잠금·epoch 검사를 거친 Immediate transaction을 연다.
`CommitGroup::append`는 요청마다 savepoint를 두고 기존 command key/digest 조회, effect 한도,
쓰기 크기, batch·revision·command·effect·pin 쓰기를 그대로 수행한다. 같은 transaction 안의
앞 요청도 key 조회와 누적 effect 한도에 포함된다. 각 요청의 command ID·batch cut·최초 응답은
독립적으로 남으며, 거절된 요청의 부분 쓰기는 그 savepoint까지 되돌린다.

SQLite 쓰기·private 조회(`command`, key, `effect`, batch) 오류나 자동 transaction rollback이
발생하면 그룹은 더 읽거나 쓰거나 확정할 수 없다. 조회의 정상적인 없음·digest 불일치·알려진
요청 검증 오류는 이 fatal 상태와 구분한다. completion builder가 오류를 요청용 문자열로 바꿔도
executor는 transaction의 fatal 상태를 우선 확인하여 앞선 사적 상태·성공 응답을 함께 버리고
전체 그룹을 실패·halt 처리한다. COMMIT 전에는 어떤 앞선 응답도 공개하지 않는다.
`finish`의 물리 COMMIT 오류는 자동 재실행하지 않는다. 그룹을 버리면 아직 확정하지 않은
모든 요청을 되돌린다. 중복 요청만 있는 그룹에는 성공 COMMIT을 추가하지 않는다.
단건 `commit`도 이 경계를 사용하며, restore import와 checkpoint의 원자 경계는 유지한다.

`fits`는 기존 논리 쓰기 64 MiB 계산으로 내부 그룹을 나눌 시점을 알려 준다. false는 해당
요청을 거절하라는 뜻이 아니라 현재 그룹을 확정한 뒤 새 그룹에서 처리하라는 뜻이다.
transaction 안에서 읽은 실제 batch cut은 private staging에만 쓰고, COMMIT 전에는 projection,
성공 응답, effect claim 또는 외부 실행에 공개하지 않는다. 이 Store API만으로 worker의 요청
수집이나 batch별 publication ACK가 이루어지지는 않는다.

### worker 수집과 공개 경계

기존 `structure-journal` worker는 첫 순수 요청을 받은 뒤 이미 도착한 연속 후보만 비차단으로
가져온다. 빈 큐·첫 barrier·기존 queue 건수 상한에서 수집을 끝내며 새 대기 시간이나 사용자
설정을 두지 않는다. 큐의 byte-budget token은 staging과 publication이 끝날 때까지 보유한다.
Resolve와 검증된 effect 결과의 순수 commit 부분만 후보이며, 실패 복원 원본의 pin 재시도가
남아 있는 Resolve는 기존 단건 경로를 쓴다. capture의 memory writer 대기, ID 예약, claim,
외부 실행, 조회, import, 복구와 maintenance는 앞 그룹을 확정·공개한 뒤 처리하는 barrier다.

executor는 그룹 전용 모델에서 앞 요청의 실제 저장 batch를 적용한 뒤 다음 decide를 수행한다.
거절된 요청은 다음 상태에 남지 않으며, 물리 COMMIT이 실패하면 이 사적 모델을 버리고
공개·실행을 멈춘다. COMMIT 결과가 불명일 때 저장 사실을 확인하지 않고 다시 실행하지 않는다.
기존 in-flight leader를 기다린 뒤 그룹 gate를 잡고, 그룹 안의 같은 key는 자기 leader를 기다리는
대신 transaction 안의 첫 기록을 읽는다.

각 논리 요청은 원 ticket·followers·optional batch cut·응답을 유지한다. 공개는 그 요청의 cut까지만
진행하고 정확한 ACK 뒤에 Finished를 보낸다. 이벤트 없는 기록은 뒤 요청의 batch를 대신 공개하지
않는다. 하나의 물리 transaction에서 B1/B2/B3가 확정된 뒤 B1 ACK와 응답을 마치고 B2 공개가
실패하면, B1 응답은 유지하고 B2/B3는 durable인 채 공개·성공 응답·새 claim을 멈춘다. admission
holder만 정리하고 durable event/effect pin은 유지한다. 아직 공개하지 않은 cut을 checkpoint·GC의
완료 cut으로 취급하지 않는다. 재열기는 기존 미실행 취소·결과 불명 대조 계약을 따르며 역사적
생성이나 외부 입력을 자동 재실행하지 않는다.

App은 명시 ID로 고정된 로컬 `workspace.update`의 name/subtitle/description patch만 앞의 같은
종류 입력과 함께 제출할 수 있다. GUI/headless dispatch의 최초 엔진 바인딩에서 이 범위를
확정하며 내부 해소 단계의 뒤 검사까지 기다리지 않는다. 이 patch들은 존재 여부·순서·암묵 대상을 바꾸지 않으므로,
앞 ACK 전에도 대상을 고정하고 worker가 FIFO로 판단한다. 응답은 각 private predecessor를
반영해 저장하고 기존 공개·ACK 경계에서 돌려준다. index 대상, mirror 표시 변경, mapping과
category 변경, 생성·닫기·forward 및 아직 scope가 정해지지 않은 요청은 이 예외를 통과하지 않는다.

## 쓰기 계측

Worker Work/command/물리 COMMIT 구분, 비소비 snapshot과 age 의미는 [journal 계측](journal-metrics.md)을 따른다.

`commit_metrics_reader`의 clone 가능한 atomic reader는 SQLite 연결에 접근하지 않고 Store가 닫힌 뒤에도 누계를 읽는다. 실제 SQL COMMIT 성공·실패 시간은 별도 고정 histogram으로 보존하고, 논리 요청이 둘 이상인 구조 transaction의 건수·요청 수도 구분한다.

`EventStore::commit_statistics`는 연결을 연 뒤 성공한 실제 SQLite 쓰기 transaction COMMIT을 센다.
초기 migration/binding과 writer epoch, ID·payload pin·claim·checkpoint 등 보조 쓰기도 physical에 포함한다.
`structural_commits`는 commit 및 import checkpoint transaction만, `logical_requests`는 그 안의 확정 요청만 센다.
중복 key 반환, decide·쓰기 검증 오류와 rollback은 성공 commit 수를 늘리지 않는다. `failed_commits`는
SQL COMMIT 자체가 실패한 횟수이며 모든 사전 거절 수가 아니다. PRAGMA의 암묵적 쓰기와 read transaction은 범위 밖이다.
시간 합계·최대값은 COMMIT 호출만 재며 WAL·FULL 정책을 바꾸지 않는다.

`TASTY_LOG=tasty::journal_metrics=debug`를 지정한 격리 실행은 worker 작업 종류별 physical/structural/logical
증분과 SQL COMMIT 시간, queue 접수부터 durable-ready(명령 처리·확정 모델 적용 포함), publication 후 finished까지의
시간을 기록한다. 이는 IPC 전체 latency나 App 전체 loop latency가 아니다. diagnostics/pressure의 공개 필드와 무관한
진단 로그이며 계측의 활성화 자체는 요청 순서·수집·ACK 동작을 바꾸지 않는다.
공유 transaction의 요청별 증분 구간은 서로 겹칠 수 있으므로 합산하지 않는다. 전체 physical/structural
누계의 차이와 logical 누계·요청 구간 시간을 따로 대조한다. `group_finished` 로그는 공개·admission
반환 전의 단일 그룹 구간을 따로 재며, Resolve/effect 결과 수와 실제 물리·구조·논리 증분을 기록한다.

## import의 전송 identity

내부 import는 전송 ID로 source snapshot/View alias를 고정한다. alias의 최초 source key와 전체 요청 digest도
같은 fenced transaction에 저장한다(schema V8의 restore_alias_inputs). alias/pin만 남고 입력 identity가
빠지는 중간 상태는 commit하지 않는다. 같은 입력 재시도는 source의 최신 manifest 대신 원 고정 cut을
재사용하며, 같은 source journal에 이미 있는 alias의 source key나 destination slot 등 입력이 달라지면
명시 충돌이다. 다른 source DB에 alias가 없는 미완 전송까지 전역 선점하는 정책은 두지 않는다. 입력 identity가 없는
과거 alias는 현재 요청에 소급 귀속하지 않고 보존한 채 거절한다. 일반 View 저장은 고정 alias를
덮어쓸 수 없고 alias 삭제는 그 입력 identity도 FK cascade로 해제한다. destination의 새 ID mapping·초기 events·
최초 응답·domain snapshot·View manifest를 같은 transaction에 확정한 뒤 source alias를 해제한다.
실패한 준비 payload는 admission holder로 보존하며 다음 fenced writer가 미수락 잔여 pin을 정리한다.
destination commit 뒤 응답 유실은 같은 전송 키 조회로 합류하고 source의 command/effect/cleanup을 복사하지 않는다.

## 잠금 없이 하는 쓰기

잠금 없이 쓰는 것은 journal을 열 때의 WAL 전환(`PRAGMA journal_mode = WAL`), schema migration, journal ID 기록뿐이며, migration과 journal ID 기록은 하나의 SQLite 쓰기 transaction으로 함께 직렬화된다.
journal을 열 때 migration 버전 확인·적용과 journal ID 기록은 그 한 transaction에서 함께 한다. 여러 연결이 빈 파일을 동시에 처음 열어도 migration은 한 번만 적용되고 모든 open이 성공한다.
WAL 전환은 transaction 밖에서 하며, 다른 연결의 전환과 겹쳐 잠금 오류가 나면 busy 대기 시간 안에서 다시 시도한다.

## ID 예약의 경계값

- 첫 ID는 1이다. 0은 호출자가 예약 상수로 쓸 수 있도록 내주지 않는다.
- 호출자가 준 상한과 SQLite 정수에 저장할 수 있는 상한 중 작은 값을 넘는 예약은 되감지 않고 거절하며 상태를 바꾸지 않는다. 0개 예약도 거절한다.

## 로그 보존

로그 보존은 batch/revision 헤더를 남기고 검증된 fallback snapshot까지의 이벤트 본문만 정리한다.
`retention_anchor`가 snapshot과 batch cut을 연결하고 `retained_stream_revisions`가 stream별
재동기화 하한을 보관한다. 이벤트 본문·그 event holder·보존 경계 갱신은 한 transaction이다.
명령 identity·최초 응답·effect와 시도 기록·ID 예약은 정리하지 않는다. 삭제한 이벤트의 ID도
별도 명부에 남기며 같은 ID를 새 이벤트로 다시 넣는 쓰기를 거절한다. snapshot/live/undo/import/
View 및 읽기 lease의 payload pin도 남는다. snapshot 두 개의 본문과 참조 checksum을 검증하며,
외부 holder가 잡은 더 오래된 snapshot이 있으면 보존 경계를 그 cut 이하로 제한한다.
단, 제품 checkpoint는 View 슬롯의 전체 stream 모델(미완 operation·undo·payload 참조 포함)이
기존 snapshot과 같고(다른 stream 사건도 전진시키는 global applied batch만 정규화) View binding도 유효할 때 manifest를 최신 완전 snapshot으로 재연결한다.
저장소는 두 cut의 해당 stream revision이 같고 존재하는지도 검사한다. View 바이트와
incarnation·runtime epoch·sequence·명시 pin은 바꾸지 않고 snapshot body pin만 원자적으로
이전한다. schema V9의 `restore_manifest_refs`는 명시한 바이트 참조를 별도로 기록하므로,
명시 참조가 옛 snapshot body와 겹쳐도 재연결·snapshot 행 삭제·GC 뒤 바이트가 유지된다.
현재 manifest의 snapshot 포인터는 전체 복원 cut을 보존하지만, 다른 snapshot body에 대한
명시 바이트 참조만으로 그 snapshot 행이나 event tail을 보존하지는 않는다. 일반 외부 holder와
고정 alias의 원 cut 보존은 그대로다.

V8까지의 합쳐진 holder 집합에서는 중복 소유 의도를 알아낼 수 없다. V9 migration은 그 집합
전체를 명시 바이트 소유로 보존적으로 이관하며 없던 정보를 추측하지 않는다. 따라서 기존
manifest당 최초 source body 하나를 추가 보존할 수 있다. 이 참조는 별도 표에서 조회할 수 있고,
다음 명시 manifest 저장/삭제가 대체·해제한다. 재연결만 반복하면 새 implicit body가 더 쌓이지
않는다. 이 호환 보존도 옛 snapshot 행이나 event tail의 압축을 막지 않는다.
따라서 휴면 슬롯이 다른 stream의 capture tail을 붙잡지 않는다. stream이 바뀌었거나
해석·검증에 실패하면 옮기지 않는다. 고정 import alias와 다른 외부 holder는 원 cut을 유지한다.
재연결 실패는 이전 manifest를 보존하며 압축보다 먼저 재시도한다. 개발 로그에는 재연결된 슬롯과
옮기지 않은 변경 stream을 기록한다. 사용자가 조치할 수 없는 보존 판단을 반복 알림으로 만들지 않는다.
보존 경계 이전 cursor는 명시적 재동기화 오류다. snapshot이 손상돼도 보존 로그 없이 처음부터
재생하는 성공 fallback을 만들지 않는다. `snapshot_and_tail_excluding`은 호출자가 해석하지 못한
snapshot을 건너뛰고 더 이전 snapshot이나 로그 처음을 기반으로 돌려준다. 보존 경계 위에 남은 기반이
없으면 이 함수도 `ResyncRequired`다. `restore_manifest_raw`는 manifest가 가리키는 View·snapshot 본문을 checksum 검증 없이 읽으며, 검증에 실패한 원본을 보존할 때만 쓴다. manifest가 가리키는 snapshot 행이 없으면 `restore_manifest`는 그 manifest 하나의 손상으로 `RestoreSnapshotMissing`을 돌려준다. 헤더까지 제거하는 안은 역사 cut과 effect 외래 키를
다시 쓰게 하므로 채택하지 않았다. 이 방식은 이벤트 본문과 고아 payload 공간을 회수하지만
batch/revision/명령/effect 헤더의 크기까지 제한하지 않는다.

## projection 출력과 consumer 위치

projection 출력은 consumer·projection version별 key→바이트 행으로 저장하고, 행 변경과 consumer 위치(batch 단위)를 한 transaction으로 확정한다.
위치가 뒤로 가거나 batch가 없으면 행 변경도 반영하지 않는다. projection 행을 가진 consumer는 위치만 저장하는 API로 위치를 옮길 수 없고, 행 변경과 함께 확정하는 API로만 옮긴다.
부분 stream 소비자는 `ScopedProjectionWrite`에 비어 있지 않은 stream 집합과 실제 batch ID를 지정한다.
저장소가 그 batch의 revision vector에서 선택한 stream만 도출하므로 서로 다른 시점의 임의
revision을 조합해 원자 cut으로 저장할 수 없다. cut에 아직 없는 stream도 거절한다.
출력·scope·cursor·재시도 digest는 같은 transaction이다. 이 cut은 선택 범위의 적용 위치이며
다른 stream이나 외부 DB의 적용을 보장하지 않는다.

consumer ID와 projection version의 scope는 첫 기록 뒤 바꾸지 않는다. global consumer를
같은 version의 scoped consumer로 전환하거나 반대로 읽기/쓰기할 수 없다. scope/코덱을
변경하면 새 projection version을 쓰고 전체 snapshot으로 초기화한다. 같은 batch의 같은
write만 retry로 받아들이며 다른 output 변경은 거절한다. digest에는 모드와 컬렉션 개수,
각 field 길이를 넣어 upsert/delete 경계를 모호하게 합치지 않는다.
선택 stream의 이전 cursor가 retention floor보다 뒤처지면 incremental write와 read는
`ResyncRequired`다. `replace_scoped_projection`만 전체 선택 출력으로 재동기화할 수 있고,
위치 역행이나 scope 변경은 허용하지 않는다. 관련 없는 stream의 compaction만으로 부분
consumer를 재동기화시키지는 않는다. 기존 global projection API의 cut 의미는 유지한다.

## 신규 admission과 저장량

누적 DB·WAL 크기는 신규 명령·engine·tab·selected restore를 거절하는 기준이 아니다.
`AdmissionUsage`는 활성 DB 페이지, 재사용 가능한 빈 페이지, 실제 DB·WAL 파일 바이트를
진단용으로 제공한다. 오래된 DB가 커도 복원·새 터미널 생성과 checkpoint를 계속한다.
용량 때문에 강제 GC·WAL truncate를 admission의 선행 조건으로 실행하지 않는다.
실제 OS FULL·I/O·쓰기 권한 오류는 여전히 실패이며 저장 성공으로 처리하지 않는다.

`AdmissionBudget`은 동시에 미확정인 준비만 제한한다. 명령 하나의 준비 payload 누계는
64 MiB이고, 미확정 명령 credit의 합계는 14 × 64 MiB다. 이 값은 저장된 이력과 무관하며
큐가 임시 준비 작업을 무제한으로 받아들이지 않게 하는 내부 경계다. durable key hit와
follower 합류를 먼저 처리하고 credit이 부족하면 앞선 Resolve·Cancel·준비 오류로 credit이
풀릴 때까지 FIFO로 기다린다. 대기도 worker 64건에 포함된다. 구조 ID 예약은 명령당
16,384개다. credit 반환은 payload 삭제나 확정 효과 취소를 뜻하지 않는다.
`open_with_admission_budget`은 이 준비 한도와 미완료 effect 한도를 검증하여 받는다.

기존 stream resume, 이미 수락된 효과의 결과·cleanup·Recovery·capture·checkpoint·GC는
새 준비 credit을 요구하지 않는다. checkpoint는 초과한 기존 DB에서도 admission 없이
동일 stream의 manifest 재연결·압축·GC를 진행한다. 파일 전체 크기 자체는 빈 페이지와
batch/command/effect 헤더를 포함하므로 압축 후에도 줄어들지 않을 수 있다.

기존 worker queue는 64건/64 MiB, 공개 원 요청은 8 MiB이며 App command 대기는 64건이다.
주기 surface capture가 이 64 MiB를 넘어 거절되거나 계속 실패할 때의 경고 알림은
[레이아웃 복원의 저장 실패 알림](../features/layout-persistence/index.md#저장-실패-알림)에 있다.
이 한도와 별도로 DB의 Pending/Deferred/Running/Uncertain effect 총수 기본 상한은 4,096개다.
`AdmissionBudget.max_pending_effects`로 정하며 실제 신규 `NewEffect` 개수가 고정된 commit의
write transaction에서 기존 총수와 더해 검사한다. 같은 key의 최초 결과는 이 검사보다 먼저
돌려준다. 기존 effect 전이만 있는 완료·cleanup·reconciliation은 상한을 넘은 기존 DB에서도
허용한다. 기록 삭제나 task/OS 취소로 개수를 줄이지 않는다. 한 batch에서 종결과 새 효과를
동시에 요구하면 종결 예정분을 미리 차감하지 않는 보수적 admission이다.

효과 완료 Work의 기존 64 MiB queued-request 상한과 별도로 저장소 쓰기 경계에도
64 MiB 논리 batch 상한과 64 MiB 단일 BLOB 상한을 둔다. batch 계산은 실제 생성된
CommitRequest의 모든 문자열·바이트 필드와 참조당 8 bytes, envelope/record당 256 bytes를
합산한다. stream append뿐 아니라 command 원입력·응답·갱신·effect 본문·전이 결과도 포함한다.
중복 key 결과는 이 검사보다 먼저 반환한다. immutable payload 공통 insert가 단일 BLOB을
검사하므로 capture·snapshot·View manifest·import도 우회하지 않는다. snapshot/manifest는
본문과 참조 목록의 논리 합계도 검사하고, import의 commit+snapshot+manifest 및 payload 복사
transaction도 합계를 제한한다. 단독 effect 전이도 같은 논리 상한을 적용한다.
Global/scoped projection의 incremental/replace도 같은 64 MiB 논리 쓰기 상한을 쓴다.
consumer·scope 이름·upsert key와 row payload·모든 delete key를 합산하고 각 항목의
고정 비용도 센다. row payload는 단일 BLOB 상한도 검사한다. scope/digest 구성과 기존
출력 삭제 전에 거절하므로 초과 요청이 cursor나 이전 출력을 바꾸지 않는다.
이 제한은 SQLite 파일이나 메모리 할당의 hard cap이 아니며 이미 생성한 입력을 쓰기 전에
거절하는 경계다. 초과하면 WriteSizeExceeded로 transaction을 되돌린다. 기존 의무/claim을
없애거나 결과 bytes를 잘라 성공으로 저장하지 않는다. effect 오류·Uncertain 대조와
checkpoint의 이전 snapshot/pin 유지·다음 기회 재시도는 기존 오류 처리 경로를 따른다.

## 구조 journal의 App 연결

원본과 projection의 관계는 이 문서가 정한다.

`src/app/journal.rs`가 데이터 홈의 worker 하나와 엔진별 비동기 continuation을 연결한다.
worker의 순수 구조 모델을 초기 논리 projection과 확정 batch 적용에 사용하며,
App이 별도 가변 JournalModel 원본을 유지하지 않는다. 준비 요청·완료 채널과 복원 읽기 개수,
App 한 회의 완료 처리량은 제한한다.

생성·변환은 Pending operation과 outbox를 확정하고 claim한 뒤 후보를 만든다.
준비 성공 다음 commit은 설치 권한·옛 owner 정리 의무만 확정한다. 외부 게시·설치와 정확한
옛 PTY 회수 뒤 최종 commit이 구조 변경·Ready·원 요청 완료를 함께 확정하고 공개한다.
설치 전 kind 철회·등록 교체는 후보를 폐기하고 기존 인스턴스를 유지한다.
외부 게시 이후의 불명 결과는 Recovery의 원 attempt·receipt 증거와 대조하며, 알려진 실패로
바꿔 자동 재실행하지 않는다.

표시면은 첫 capture 전에도 생성 자료 참조를 유지한다. 복원은 snapshot을 우선하고,
없으면 generic 생성 params/CWD를 사용한다. terminal은 현재 셸 설정과 저장된 명시적
복원 명령을 사용하며 과거 실행 인자·입력을 자동 재전송하지 않는다.
복원 capture는 기존 DataRef로 읽고, 같은 자료를 준비 요청에 복사해 다시 저장하지 않는다.
선택되지 않은 terminal과 아직 등록되지 않은 kind는 지연 활성화하며, 일반 명령도 필요한 원 대상의 activation에 합류한다. source 경계가 존재한다는 사실을 모든 writer·장애 복구 시나리오의 검증 완료로 해석하지 않는다.

선택한 legacy slot은 worker가 원본 파일과 자료를 읽어 한 import batch로 확정하고 초기 View의
ID 대응을 함께 보존한다. 이미 journal stream이 있으면 legacy 파일을 다시 원본으로 읽지 않는다.
CoreState 생성자도 제품에서 legacy 파일을 선행 해석하지 않는다. 기존 위치가 범위를 벗어나면
workspace와 tab은 마지막 항목, pane은 첫 항목을 고르는 복원 규칙을 유지한다.

정상 resume는 journal/stream/incarnation과 확정 cut/revision을 붙인 최신 View checkpoint를 초기 import 선택보다 우선한다. 이 선택은 구조 이벤트가 아니다. `src/runtime/journal_product/view_record.rs`는 DB restore manifest를 먼저 읽고 그 domain checkpoint와 View의 binding을 대조한다. DB 원본이 없는 경우에만 legacy sidecar를 최초 이관 자료로 읽으며, DB 자료가 손상됐다고 옛 sidecar로 조용히 돌아가지 않는다.

현재 incarnation을 선택하지 않은 시작에서는 과거 View를 읽지 않는다. worker는 옛 incarnation과 더 늦게 도착한 과거 sequence의 저장을 거절한다. 저장 실패의 후보와 dirty 상태는 재시도를 위해 남기며 종료는 기존 tick 저장과 별도로 최신 final capture를 요청한다. View manifest와 도메인 checkpoint·payload 참조를 연결하는 pin/보존 구현은 저장 계층에 있다(이 문서). 해당 코드의 존재가 crash·전원 장애 검증을 완료했다는 뜻은 아니다.
