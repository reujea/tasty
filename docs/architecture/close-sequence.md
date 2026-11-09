# 닫기와 실행 자원 회수

로컬 구조 닫기는 확정된 구조 변경, 원 실행 자원의 회수, 명령 완료를 구분한다. View가 사라졌다는 사실이나 논리 트리에서 surface가 빠졌다는 사실만으로 PTY·plugin 자원 회수가 끝난 것은 아니다. 계약의 근거는 [이벤트 저장소 (`tasty-event-store`)](event-store.md), [이벤트 저장소 (`tasty-event-store`)](event-store.md), [이벤트 저장소 (`tasty-event-store`)](event-store.md)다.

## 명령에서 완료까지

1. GUI producer는 사용자 입력 시 대상 ID와 origin을 고정한다. keyed IPC는 원 method/params의 digest와 재시도 key를 먼저 admission에 넘긴다. key hit는 저장된 결과를 반환하고, miss의 `NeedsResolution`에서만 존재 여부·기본 대상을 해소한다. 원격 요청도 원 연결·origin binding을 유지한다. 수락한 대상은 나중의 포커스로 바꾸지 않는다.
2. worker는 닫을 대상과 cleanup 의무를 journal에 확정한다. App의 publication은 원 descriptor의 kind·activation, engine incarnation과 runtime epoch를 검사하고, 제거할 surface box와 Terminal/Pty를 `ResourceRetirement`에 넘긴다.
3. 논리 projection의 batch 적용과 ACK 뒤, cleanup effect의 원 attempt를 claim하여 실행한다. `ResourceRetirement`는 숫자 ID로 후속 자원을 다시 찾아 파괴하지 않고 이미 보유한 원 owner를 처리한다.
4. PTY retirement receipt와 원 plugin process·surface instance에 묶인 ACK/retirement receipt를 관측한다. metadata 정리와 회수 결과를 확정하고 명령의 모든 member 결과를 집계한다. 필요한 batch publication까지 완료한 뒤 응답을 공개한다.

## 관측 일시정지 범위

App은 journal 사실을 게시하는 동안 관측을 멈춘다. 이 동안 IPC·창 입력을 보류하고 plugin pump를 돌리지 않는다. 닫기에서는 다음 구간만 멈춘다(이 문서).

- 커밋 전(대상·cleanup 의무 확정과 batch 게시).
- 정리 항목의 claim 단계와 결과 확정 단계.
- 결과 확정 이후 응답까지.

3·4단계 사이의 receipt 대기에서는 관측을 멈추지 않는다. 닫히는 surface는 이미 구조와 View에서 빠졌으므로, 이때의 조회와 입력은 닫기가 반영된 구조를 본다. 닫힘 이벤트와 닫기 응답은 결과 확정 뒤에 나간다. receipt 대기로 넘어가는 즉시 plugin 회수 게시를 적용한다. 그래서 claim 이후에 처리되는 enable은 파괴 중인 surface를 새 프로세스에 다시 게시하지 않는다.

닫기는 접수한 뒤 대상을 해석하기까지 시간이 걸린다. 그 사이에 plugin 프로세스가 새로 떠도 접수된 닫기가 회수할 수 있는 surface는 다시 게시하지 않는다. App은 IPC 명령마다 처리 직전에, 그리고 journal 턴마다 이 목록을 plugin host에 넘긴다. 아직 해석하지 않은 닫기는 현재 구조로 해석해 대상을 구한다. 닫기가 그 surface를 회수하지 않고 끝나면(실패·취소) 목록에서 빠질 때 다시 게시한다. Tasty가 닫기보다 enable을 먼저 처리했다면 그 시점의 surface는 살아 있으므로 새 프로세스에 게시한다. 이후의 닫기는 새 프로세스에 파괴 요청을 보내고 그 응답으로 확정하므로, 닫힌 surface의 인스턴스는 새 프로세스에 남지 않는다. 서로 다른 IPC 연결로 보낸 요청은 보낸 순서대로 처리된다는 보장이 없고, Tasty는 그 순서를 맞추지 않는다(이 문서의 다른 연결 절).

앱 종료는 receipt 대기도 기다린다. 닫기가 한 번 응답하고 결과(시한이 지나면 `Uncertain`)를 기록한 뒤 종료가 진행된다([종료 시퀀스](shutdown-sequence.md)).

생산 경로는 `src/app/journal/commands/close.rs`, `src/runtime/resource_retirement.rs`, `src/app/journal/resource_cleanup.rs`다. GUI의 cache·선택 보정과 toast는 이 실행 원본을 대신하지 않는다.

## 같은 자원을 다루는 다른 명령

receipt 대기 중 관측이 재개돼도 같은 자원을 다루는 다른 명령에 대해 다음을 보장한다(이 문서).

| 상황 | 보장 | 근거 |
|---|---|---|
| 같은 surface·workspace id | 회수 중인 id는 다시 쓰이지 않는다 | id 예약은 단조 증가하며 되감지 않는다. 닫은 항목 복원도 새 id를 받는다. metadata 정리는 id가 다시 살아 있으면 거절한다 |
| 같은 plugin disable·재기동 | 먼저 보낸 파괴 요청은 원 세대 회수로 확정된다. 닫기를 접수한 뒤에 처리되는 enable은 그 닫기가 회수할 수 있는 surface를 새 프로세스에 다시 게시하지 않는다. 접수 뒤 해석 전까지는 접수된 닫기 목록이, claim 이후에는 회수 게시 즉시 적용이 막는다. 닫기보다 먼저 처리된 enable의 경우는 아래 행 | 세대 회수 확정, 접수된 닫기의 재게시 보류, 회수 게시 즉시 적용 |
| 다른 연결의 enable이 닫기 접수보다 먼저 처리됨 | 닫힌 surface의 인스턴스가 새 프로세스에 남지 않는다. 새 프로세스가 잠깐 열었다가 파괴 요청으로 닫는다 | enable 시점에 살아 있는 surface를 게시하고, 뒤이은 닫기는 새 세대에 `surface.destroy`를 보내 그 응답으로 확정한다. plugin SDK는 호스트 요청을 한 워커에서 순서대로 처리한다 |
| 엔진 은퇴 | receipt가 남은 엔진은 해제하지 않는다 | 엔진 해제는 정리 항목·보류 회수가 빌 때까지 기다린다 |
| 같은 워크스페이스의 다른 닫기 | 같은 자원을 두 번 회수하지 않는다 | 회수 대상은 이미 구조에서 빠졌다. 뒤 명령의 `Claim`·`Finish`는 여전히 일시정지한다 |
| 사용자 창 입력 | 입력은 닫기가 반영된 구조를 대상으로 바로 처리된다 | 닫힌 surface는 이미 View·구조에 없다. 일시정지 단계의 입력 보류·재검사는 그대로다 |
| 앱 종료 | 종료가 걸린 닫기도 응답을 한 번 보내고, 종료 전에 결과를 기록한다 | 종료는 `Running` 정리를 기다리고 그동안 회수 응답을 직접 처리한다. 상한은 receipt 시한이다 |
| 레이아웃 저장·surface capture·preset capture | 닫히는 surface를 저장하거나 되살리지 않는다 | 닫히는 surface는 구조와 runtime 표에서 빠져 `ResourceRetirement`만 쥐고 있다. 저장·capture는 닫기가 반영된 상태를 읽는다 |
| 원격 attach 갱신·해석 대기 요청 | 클라이언트가 닫기 중간 상태를 받지 않는다 | 구조 변경은 이미 확정·게시됐다. 클라이언트는 닫기가 반영된 구조를 받는다. 닫힘 이벤트는 결과 확정 뒤에 나간다 |

## 실패와 불명 결과

| 관측 | 의미 |
|---|---|
| 요청 접수·Running claim | 실행할 권한과 원 attempt를 고정했다. 회수 완료가 아니다 |
| 실제 child reap 또는 원 plugin receipt 완료 | 해당 물리 자원의 회수 증거다. 별도 metadata·명령 완료 의무가 남을 수 있다 |
| surface 를 만든 원 plugin 프로세스의 회수 완료 | 그 프로세스 안의 surface 인스턴스도 남지 않았다는 증거다. disable·무응답 재시작·교체로 원 프로세스가 회수된 뒤 그 surface 를 닫으면 파괴 요청 없이 성공으로 확정한다. 회수 중이면 회수가 끝날 때 확정한다. 닫기 정리의 짧은 게시 단계로 관측이 멈춘 동안에는 pump가 돌지 않으므로, 정리 폴링이 끝난 회수를 거둬 확정하고 다시 띄우기는 관측이 재개된 뒤로 미룬다. 회수한 세대는 등록된 surface나 mesh bootstrap이 가리키는 동안만 기록에 남고, 다음 회수 때 가리키는 것이 없는 세대를 지운다. 새 프로세스가 뜨면 남은 surface 를 다시 게시하므로([플러그인 개발](../dev-guide/plugin-development.md)의 Surface kind 절) 그 뒤의 닫기는 새 프로세스에 파괴 요청을 보낸다. 다시 게시되지 않은 surface 만 옛 세대의 회수로 확정한다. 먼저 보낸 파괴 요청의 응답을 원 프로세스 회수(disable·교체·무응답 재시작·채널 끊김 뒤 회수) 때문에 더 받을 수 없게 되면, 그 요청도 같은 세대 회수 증거로 확정한다. receipt 대기 중에는 관측이 멈추지 않으므로 닫기를 기다리는 동안 보낸 disable도 바로 처리되고, 그 회수가 시한 안에 닫기를 확정한다. 회수가 닫기 응답 시한 뒤에 일어나면 응답은 `Uncertain`이고 이후 재조정에서 성공으로 확정된다 |
| timeout·연결 유실·원 plugin 응답 부재(회수 기록이 없는 원 프로세스 부재 포함) | 성공 또는 알려진 실패로 단정하지 않는다. operation의 `Uncertain`과 receipt의 불명 관측을 유지한다 |
| 뒤늦게 도착한 정확한 원 receipt | 같은 attempt의 reconciliation 근거다. 새 자원에 cleanup을 다시 실행하는 근거가 아니다 |

`OperationOutcome`의 Succeeded/Failed/Cancelled/Superseded/Uncertain과 물리 receipt 상태는 같은 enum이 아니다. 원 실행 결과가 불명인데 자동 재실행해 성공으로 덮지 않는다. `resource_cleanup.rs`는 대조가 필요한 owner와 receipt를 계속 보유한다.

## 사용자 기록과 headless

사용자 닫기·IPC/에이전트 닫기·프로세스 종료의 origin은 다르다. 사용자 선택 및 닫은 항목 복원 기록은 해당 origin과 명령의 기록 정책을 따른다. 원격 mirror의 구조 변경은 로컬 구조 writer로 실행하지 않고 서버에 전달하며, 기존 user/agent wire 호환을 유지한다([원격 attach](../features/remote-attach/index.md)).

GUI와 headless 모두 동일한 domain commit·effect·회수 의무를 수행한다. headless에는 로컬 View cache·선택·popup 후처리가 없다. 일반 host-event plugin bus나 headless attach client를 추가로 지원한다는 뜻은 아니다([headless 경계](../dev-guide/headless-build-boundaries.md)).

## Engine과 workspace 수명

workspace가 확정 구조에서 실제로 제거되면 그 TaskScope의 해당 runner에 stop을 요청한다. 같은 engine 안의 순서 변경이나 View 개폐는 stop 사유가 아니다. runner stop, task cancel, OS 자식 kill은 별개의 계약이다.

창을 닫을 때 engine을 보존하는 park와 engine 자체를 은퇴시키는 retiring을 구분한다. retiring owner는 `EngineRegistry.sessions`에 남는다. Preserve는 final View checkpoint를 기다리고, Discard는 필요한 stream retirement를 확정한다. View 생성 실패로 owner만 회수하는 경우에는 정상적으로 열린 기존 journal stream을 삭제하지 않는다.

Releasing 단계는 원 runner의 실제 join과 `EngineRelease`의 물리 자원 receipt를 기다린 뒤 owner와 슬롯을 해제한다. 일반 engine 해제는 timeout으로 완료를 가장하지 않는다. 앱 전체 종료의 대기 상한은 미회수 경고와 함께 별도로 처리한다([종료 시퀀스](shutdown-sequence.md)).

## 확인 범위

위 내용은 현재 소스의 책임과 순서다. PTY reap, plugin ACK 유실, View 소멸, crash 뒤 Recovery, GUI/headless의 응답 시점을 실행으로 확인하는 일은 별도다. 과거 cascade 계측값이나 옛 ownership fixture의 성공을 현재 경로의 검증 근거로 사용하지 않는다.

## 과거 측정 기록의 해석

아래는 이전 cascade 구현에서 남긴 계측 정의와 실제 측정 기록이다. 수치와 조건은 이력으로 보존하지만, 옛 함수·단계 이름을 현재 실행 경로로 읽거나 이 측정으로 journal/retirement 경계가 검증됐다고 해석하지 않는다. 현재 소유와 완료 순서는 위 절을 따른다.

<a id="close-계측-target-tastyclose"></a>

## 이전 cascade 계측 (target: `tasty::close`)

부팅·종료 계측과 같이 항상 기록하며, 레벨 `info!`, 소요는 `ms` 필드(f64
밀리초). debug 빌드는 `$TASTY_HOME/debug-dev.log`(debug 레벨 file layer)에
수집되고 stderr 기본 필터가 warn 이라 콘솔 노이즈는 없다. release 검증은
`TASTY_LOG=info`.

| 마커 | 구간 | 추가 필드 |
|------|------|-----------|
| C1 snapshot | `capture_workspace_snapshot` / `ClosedItem::from_workspace`. 터미널 값은 호스트가 `TerminalStore::closed_capture`로 읽고 스크롤백을 디스크 형식 바이트(`ScrollbackBlob`)로 인코딩해 넘긴다. model은 `Terminal`을 받지 않고 바이트를 해석하지 않는다 | `surfaces`, `lines` = 캡처된 인라인 스크롤백 라인 총합 |
| C2 push_closed_item | `CoreState::push_closed_item` 전체 | `restore_inject_ms`(C2a) · `scrollback_persist_ms`(C2b) · `evict_ms`(C2c) |
| C3 collect_targets | `collect_workspace_close_targets` | `surfaces` |
| C4 ws_memory_purge | `purge_scope(Scope::Workspace)` | — |
| C5 cleanup_targets | cleanup 루프 전체 | `surfaces` · `scrollback_delete_ms`(C5a) · `terminal_drop_ms`(C5b) · `indices_drop_ms`(C5c) · `memory_purge_ms`(C5d) |
| close_total | close 진입 → 완료 | `surfaces`, `snapshot`(C1/C2 를 탔는지) |

모든 마커는 `path` 필드(`gui`/`inline`/`cascade`)를 함께 찍는다.

읽는 법:

- **C5a~C5d 는 surface 마다가 아니라 합계다.** 종료 계측 S5b(`Pty::drop`
  누적) 선례와 같다. 탭 30개를 surface 단위로 찍으면 로그 150줄이 close 구간
  *안에서* 발생해 그 write 비용이 측정을 왜곡한다. N 에 대한 선형성은 `surfaces`
  필드와 합계 ms 의 조합으로 판정한다.
- **`lines` 는 C1 시점의 인라인 라인 수다.** C2b(`persist_closed_scrollback`)가
  라인을 디스크로 내리면 0 이 되므로 캡처 직후에만 의미가 있다.
- **C2b 쓰기가 실패하면 스크롤백은 인코딩된 채 메모리(Inline)에 남는다.** 그 항목을 복원하면 디스크 형식이 보존하는 속성(text, 셀 폭, wrapped, fg/bg, bold/half/italic, 밑줄 종류와 색, strikethrough, reverse, blink, invisible, overline)만 남고 vertical_align·하이퍼링크는 복원되지 않는다.
- **`close_total` ≥ 단계 합**이며, 차이가 크면 계측이 덮지 않은 구간이 있다는
  뜻이다(예: `enqueue_surface_closed`, `surface_kind` 재조회, workspace 벡터
  remove).
- **`snapshot=false` 는 C1/C2 마커가 아예 없는 상태를 뜻한다.** "안 걸렸다" 와
  "계측이 없다" 를 로그만으로 구분하기 위한 필드다.
- **surface 가 0 개인 워크스페이스는 만들 수 없다** — 마지막 탭은 닫히지 않는다
  (`close tab` 이 "cannot close the last tab" 으로 거절). C3/C5 는 대상이 0 이어도
  기록하도록 되어 있지만, 실제로 `surfaces=0` 을 관측하려면 layout 이 깨진
  상태여야 한다.
- **앱 quit 과는 중복되지 않는다.** 종료 cascade(`src/app/shutdown_cascade.rs`)는
  surface 마다 lifecycle 이벤트를 *큐에 넣기만* 하고 `cleanup_surface` /
  `close_workspace_at` 을 호출하지 않는다 — 실측에서도 quit 시 `tasty::shutdown`
  마커만 나오고 `tasty::close` 로그는 나오지 않는다.
- **C5b 는 `Pty::drop` 전체다.** `master` 해제(Windows 는 여기서
  `ClosePseudoConsole` 이 자식 종료를 기다린다)를 포함하도록 `master` 를
  `Option` 으로 두고 drop 본문 안에서 `take()` 한다 — 필드 자연 해제에 맡기면 그
  비용이 계측 구간 밖으로 새어나간다. 종료 계측 S5b 도 같은 누적기를 쓴다.
- **C5b 는 자식이 죽기를 기다리지 않는다** — unix 는 SIGHUP 만 보내고 유예 폴링과
  SIGKILL escalation 을 detached reap 스레드에 넘긴다([종료 시퀀스 — 종료 cascade + Drop tail](shutdown-sequence.md)).
  SIGHUP 을 무시하는 자식은 이 escalation 으로만 끝나며, tasty-terminal 의
  `retiring_a_child_that_ignores_hangup_kills_and_reaps_it` 가 그 경로를 지킨다(SIGKILL 을 빼면
  자식이 스스로 정상 종료해 실패한다). 마스터를 닫을 때 커널이 보내는 SIGHUP 으로 죽는 자식만 쓰는
  다른 retire 시험들은 이 경로가 없어도 통과한다.
  그래서 C5b 는 "종료 신호 발사 + master 해제" 비용이지 "자식 종료 확인" 비용이
  아니다. 자식이 실제로 회수됐는지는 이 마커로 판정할 수 없다.
- **C5c 는 observer 워커를 join 하지 않는다** — surface close 로 인한 자동 해제는
  sender 만 떨어뜨리고 join 을 종료 시퀀스(S3b)로 미룬다(같은 ADR). 명시 해제
  (`output.observe_stop`)만 그 자리에서 join 한다.

### 계측 재현

`gui` 경로는 사용자 메뉴/단축키로만 트리거되므로 release IPC 로 도달할 수 없다.
debug 빌드의 `debug.close_workspace`(`index`)가 그 메뉴 항목을 재현한다
([debug-ipc](../dev-guide/debug-ipc.md)). 마지막 workspace 는 거절한다 — GUI 는 그
경우 창까지 닫지만 debug IPC 는 창 종료를 재현하지 않아, 그대로 두면 workspace 가
0 개인 상태로 다음 redraw 가 패닉한다.

<a id="실측-기준선"></a>

## 이전 cascade 실측 기준선

Linux(X11) / debug 빌드 / 번들 plugin 전부 활성 / `TASTY_LOG=info` / 격리
`TASTY_HOME`. `path="gui"`, `debug.close_workspace` 로 close. 스크롤백 "만재" 는
surface 마다 `seq 1 20000`(기본 상한 10000 줄까지 채워짐). 각 조건 1 회 측정이라
절대값이 아니라 구간 비율과 N 에 대한 기울기로 읽는다. 단위 ms.

아래는 **벌크 캡처(C1) · surface purge 중복 제거(C5) ·
[종료 시퀀스 — 종료 cascade + Drop tail](shutdown-sequence.md)(C5b) 이 모두
적용된 뒤의 측정 기록**이다. 당시 환경의 성능을 보장하는 값은 아니다.
측정 당시 C1은 화면도 복제했고 스크롤백 인코딩은 C2b에서 했다. 지금은 C1이 화면을 복제하지
않고 스크롤백 인코딩을 맡으며 C2b는 파일 쓰기만 한다. 그래서 아래 C1·C2b 값의 배분은 당시
구조에 그대로 적용되지 않는다(합계 작업량은 화면 복제만큼 줄었다, 미측정).

| 탭 수 | 스크롤백 | close_total | C1 snapshot | C2b sb_persist | C3 collect | C4 ws_purge | C5 cleanup | (C5b terminal_drop) |
|-------|----------|-------------|-------------|----------------|-----------|-------------|------------|---------------------|
| 1  | 없음 | **1.1** | 0.63 | 0.0002 | 0.007 | 0.048 | 0.30 | 0.13 |
| 1  | 만재(10k) | **15** | 2.9 | 8.2 | 0.013 | 0.14 | 3.2 | 2.3 |
| 10 | 없음 | **6.8** | 4.7 | 0.0002 | 0.014 | 0.050 | 1.9 | 1.3 |
| 10 | 만재(100k) | **109** | 40 | 45 | 0.022 | 0.10 | 24 | 18 |
| 30 | 없음 | **33** | 20 | 0.0004 | 0.036 | 0.094 | 12 | 7.1 |
| 30 | 만재(300k) | **403** | 97 | 234 | 0.038 | 0.12 | 71 | 53 |

<a id="docs/dev-guide/plugin-permissions.md-전후-같은-조건-스크롤백-없음"></a>

#### 자식 종료 대기 분리 전후 (같은 조건, 스크롤백 없음)

| 탭 수 | close_total (전 → 후) | C5 cleanup (전 → 후) | C5b terminal_drop (전 → 후) |
|-------|-----------------------|----------------------|------------------------------|
| 1  | 52 → **1.1** | 51 → **0.30** | 50 → **0.13** |
| 10 | 513 → **6.8** | 507 → **1.9** | 505 → **1.3** |
| 30 | 1541 → **33** | 1528 → **12** | 1518 → **7.1** |

이 측정에서는 C3/C4가 모두 0.2ms 미만이고, `close_total`과 단계 합의 차이가
1ms 미만이었다. 단계 밖의 작업이 없다는 뜻은 아니다.

- 스크롤백이 찬 30탭에서는 C2b(디스크 쓰기)가 234ms로 가장 컸다. 이 조건을
  3회 반복한 범위는 close_total 403~406 / C2b 196~234 / C1 97~129 / C5 70~81ms이며,
  모두 C2b가 가장 컸다. 디스크 쓰기는 close 프레임에서 동기로 실행된다.
- C1은 화면과 스크롤백을 복제하므로 데이터 양에 영향을 받는다. 스크롤백이 없는
  30탭에서는 화면 rows × cols 복제에 20ms가 걸렸다.
- C5b에는 `Terminal`이 가진 스크롤백 메모리를 해제하는 비용도 포함된다.
  자식 종료 대기는 별도 스레드에서 처리하므로 이 값으로 자식 회수 완료를 판단하지 않는다.
- C5a의 legacy scrollback 파일 삭제가 실패하면 다음 시작의 부팅 GC가 회수한다. 부팅 GC는 `restore_layout`이 켜져 있고 슬롯 참조 집합을 확정한 회차에만 지운다([레이아웃 영속화](../features/layout-persistence/index.md#surface-내용-복원-현재-터미널-scrollback)).
- C5c는 observer 워커를 join하지 않는다. 회수는 종료 단계 S3b에서 진행한다.
- C5d는 memory.db 크기의 영향을 받는다. 위 표는 새 `TASTY_HOME`에서 측정했으므로
  큰 DB의 비용과 같다고 볼 수 없다.

### 캡처 비용 (C1)

C1 은 surface 마다 화면(rows x cols)과 스크롤백 전량을 `ClosedItem` 으로 복제한다.
스크롤백 쪽은 라인 단위가 아니라 **벌크**로 가져온다 —
`Terminal::scrollback_lines_all()` 이 terminal state mutex 를 한 번만 잡고
스크롤백의 저장 표현인 `ScrollbackLine`(단일 text 버퍼 + cell 길이 + RLE 속성 런)을
그대로 복제한다. 라인당 비용이 헤더 3개 복제로 고정돼 cell 수에 비례하지 않는다.

라인당 경로(`scrollback_line_full`)도 남아 있지만 selection / search / link 처럼
소수 라인만 만지는 소비자용이다. 벌크 캡처에 쓰면 두 가지가 겹쳐 비싸진다:

- 라인마다 state mutex — 파서 스레드가 `ingest` 로 잡는 것과 같은 lock(docs/surfaces/terminal/index.md)
  이라, 만재 스크롤백 캡처가 파서와 수만 회 경합한다.
- 디스크 영역 라인은 `line_owned` / `line_wrapped` 가 같은 인덱스를 독립적으로
  읽어 `File::open` 이 라인당 2회가 된다(당시는 `line_full` 단일 조회로 1회).

`layout_persistence::scrollback` 의 캡처도 같은 벌크 경로를 쓴다.

캡처 표현이 원본과 셀 단위로 동일하다는 것(그래핌 / cell 속성 / `wrapped`)은
`crates/tasty-terminal/tests/scrollback_bulk_capture.rs` 가, 그 표현이 디스크
왕복을 거쳐도 복원 payload 를 바꾸지 않는다는 것은
`src/store/scrollback.rs` 의 `capture_persist_restore_round_trip_preserves_lines`
가 고정한다.

### memory.db 크기 의존

memory.db 를 24276 엔트리(3.6MB)까지 채우고 탭 10개·스크롤백 없음으로 close 한
결과(위 표 3행과 비교). **아래는 surface scope purge 중복을 걷어내기 전 측정이다**
— 당시엔 `SurfaceMetaStore::remove` 와 `purge_surface_memory_scope` 가 같은
`purge_scope(Scope::Surface)` 를 surface 당 2회 불러 두 단계로 잡혔다:

| | C4 ws_purge | (구) C5c meta_remove | (구) C5e memory_purge |
|---|---|---|---|
| 기본 상태 | 0.057 | 3.1 | 0.33 |
| 24k 엔트리 | 3.0 | 23 | 20 |

당시는 surface scope purge를 C5d에서 한 번만 수행한다. 위 표는 중복 제거 전의
기록이므로 당시 C5d 비용이나 전체 close 시간으로 환산하지 않는다. 큰 DB에서
close가 느리면 C4와 C5d를 따로 확인한다.

### `scrollback_disk_swap`

탭 10개·스크롤백 만재(`seq 1 30000`), `performance.scrollback_disk_swap = true`,
새 `TASTY_HOME`. disk swap 이 켜지면 상한 10000 줄을 넘겨 유지하므로 캡처되는
`lines` 자체가 크게 늘어난다.

| | lines | close_total | C1 | C2b | C5b |
|---|---|---|---|---|---|
| off | 100000 | 601 | 33 | 41 | 521 |
| on | 279622 | 1445 | 544 | 246 | 547 |

> 이 표는 자식 종료 대기를 별도 스레드로 옮기기 전의 기록이다. 당시 C5b에는
> surface당 약 50ms의 대기가 포함됐다. 당시 값은 이 수치를 단순히 빼서 구할 수 없다.

이 측정 당시에는 disk 영역 라인을 캡처할 때 메모리 복제가 아니라 **라인마다 파일을
열어 read** 했고, C1 의 라인당 단가가 memory-only 대비 한 자릿수 배 높았다(10k 라인당
3ms → 19ms). 현재 캡처는 disk 영역을 파일 한 번 열기와 순차 read 로 읽는다
([레이아웃 보존](../features/layout-persistence/index.md)). 현재 단가는 다시 재지 않았다.
켜고 쓸 때 캡처 비용을 비교할 때는 전체 라인 수와 디스크 영역 라인 수를 함께 확인한다.

