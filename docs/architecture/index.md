# 아키텍처 개요

Tasty는 본 바이너리(`src/`)와 63 개 크레이트(`crates/*`)로 구성된 Cargo workspace다. 도메인 로직은 GUI 없이 동작하고, GUI·IPC·OS 연동은 port와 adapter로 연결한다.

## 기술 스택

| 역할 | 라이브러리 |
|------|-----------|
| 윈도우/입력 | winit |
| GPU 렌더링 | wgpu |
| UI 위젯 | egui + egui-wgpu + egui-winit |
| VTE 파싱 | termwiz |
| PTY | portable-pty (Windows ConPTY / Unix) |
| 폰트 래스터라이징 | cosmic-text + swash |
| IPC | TCP (127.0.0.1, 동적 포트, `~/.tasty/tasty.port`) + JSON-RPC 2.0 (serde_json) |
| CLI | clap |
| 설정 | toml + directories |
| 공유 메모리 | `tasty-shm`(자체) — POSIX shm + SCM_RIGHTS / Windows DuplicateHandle |

## headless 분리 — `gui` feature

본 바이너리는 `gui` feature(`default = ["gui"]`)로 로컬 View/GPU 표면을 켠다. headless는 이 feature를 끄고 App·EngineSession·journal·외부 통신과 실행 서비스를 사용한다. 원격 표시 자료나 plugin mesh 중계가 있다는 이유로 로컬 GUI를 만든 것은 아니다([headless 경계](../dev-guide/headless-build-boundaries.md)).

| 소유자 | 역할 | GUI와의 관계 |
|---|---|---|
| AppServices | process 공유 port·저장소·registry·TaskService | GUI/headless 공용 |
| AppState | 시작·종료·요청 진행 값 | GUI 값은 cfg로 구분 |
| Hub | IPC 서버·포트 파일 | GUI/headless 공용 |
| JournalApplication | CommandExecutor worker·batch publication·응답/효과 조정 | GUI/headless 공용 |
| EngineSession | CoreState projection·live/runtime/task/hook/remote와 원 자원 receipt | GUI에서는 EngineRegistry, headless에서는 실행 루프가 소유 |
| ViewRegistry | View 목록·active_modal·focused_view_id | gui 전용 |

### 도메인과 실행 경계

순수 구조 계층인 `tasty-core`는 Command/Event·JournalModel·decide/evolve와 CoreState projection을 소유하며 GUI·PTY·SQL을 직접 의존하지 않는다. `tasty-event-store`는 저장 계약, `tasty-task-runtime`은 작업 실행·완료 대기를 맡는다. root의 App/runtime adapter가 저장·실행·View를 조립한다(이 문서).

View에는 `EngineRead`, 실행 adapter에는 `EngineRef`/`EngineMut`를 전달한다. Core에 View 전체나 상위 EngineSession을 넘기지 않는다. `src/core`에 남은 호스트 adapter를 pure domain crate와 같은 소유자로 보지 않는다. PTY 출력 처리·resize·이벤트 수집·유휴 회수의 Engine 실행 메서드는 `src/runtime/pty.rs`에 있으며 TerminalStore 원본을 사용한다. 구조 실행과 IPC의 대여·요청 값은 [App·Engine·View 상태 소유권](../dev-guide/app-state-ownership.md)을 따른다.

manifest와 source 경계 검사는 실제 feature 통합·플랫폼 빌드·동작 검증을 대신하지 않는다. 옛 경로와 mutable Core 필드를 전제한 fixture는 현재 소유 계약에 맞춰 별도로 정합해야 한다.

### 크레이트를 나누는 기준

크레이트를 나눌 때는 의존 방향을 먼저 확인한다. 재사용하는 코드가 작더라도 기존 크레이트에 불필요한 의존성을 들여오면 분리한다. `tasty-remote`가 SSH와 IPC를 함께 사용하되 `tasty-ssh`는 IPC를 모르도록 한 것이 이 기준의 예다. 줄 수는 빌드 성능을 개선할 후보를 찾는 참고값이며, 분리를 금지하는 기준은 아니다. 소비자가 하나로 줄거나 크레이트 관리·링크 비용이 분리 이득보다 커지면 다시 검토한다.

파일 형식·핸들러 레지스트리의 plugin port 구현은 타입을 소유한 크레이트에 둔다. wire 계약인 trait을 도메인으로 옮기면 protocol이 도메인 구현을 의존하게 된다. Rust 고아 규칙 때문에 제3의 adapter에 두기도 어렵다. `tasty-file-format → tasty-plugin-protocol`을 명시적 계층 예외로 관리하고, 예외 목록과 아키텍처 설명을 함께 고친다. protocol 사용이 port 구현을 넘어 커지면 경계를 재검토한다.

파일 핸들러 정책과 레지스트리는 `tasty-file-handler`에, 실제 파일 실행은 호스트에 둔다. 번들 기본값은 `HOST_DEFAULTS_TOML`로 한 번 포함하고 소비자는 상수를 사용한다. `tasty-file-handler → tasty-plugin-protocol`도 같은 port 예외다. protocol을 낮은 계층으로 옮기면 의존 예외는 줄지만 plugin 프로토콜·프로세스 경계 설명이 흐려지므로 현재 소속을 유지한다. 도메인-IO 예외가 넷째로 늘거나 새로운 예외가 기존 이유로 설명되지 않으면 trait 배치를 다시 검토한다.

선택 계산은 `tasty-selection`에 두고 GUI와 헤드리스 모두 `tasty-cell-width`의 Unicode 폭을 사용한다. GUI가 없다는 이유로 비ASCII 문자를 전부 2칸으로 세지 않는다. 크레이트 이동 때 이전 feature 조건을 복사하지 말고 그 조건이 막던 의존성이 아직 필요한지 확인한다. 문자 폭 계산이 GUI 의존을 갖게 되거나 헤드리스 소비자에서 폭 차이가 관측되면 계산 공유 범위를 재검토한다.

링크 검출과 결과 타입은 `tasty-terminal-link`에서 GUI 없이 계산하고, 브라우저를 여는 `open_uri`는 UI adapter에 둔다. 타입 복제나 무의미한 gui feature 대신 계산과 부수효과를 분리한다. GUI 결합은 파일 내부의 cfg나 직접 의존만으로 판단하지 않는다. 상위 모듈 선언과 전이 의존성까지 확인한다. 링크 크레이트에 OS·프로세스 부수효과가 들어가거나 adapter의 역할이 커지면 분리 기준을 다시 확인한다.

루트 패키지는 라이브러리와 얇은 실행 파일로 나눈다. 모듈 트리는 `src/lib.rs`가 소유하고 실행 파일은 시작 함수와 실행 파일 전용 설정을 둔다. 이 분리는 외부 소비자가 사용할 타깃을 만들지만 도메인 API 공개를 자동으로 허용하지 않는다. 기존 pub 재수출도 실제 공개 API가 되므로 소스 선언을 함께 점검한다. 루트 단위 테스트는 lib 타깃에 있으며 bin만 검사하면 0건일 수 있다. 실행 파일 테스트를 라이브러리 테스트로 바꾸면 검증 범위도 달라진다.

공용 타입 크레이트의 기본 feature는 GUI 의존을 켜지 않는다. egui 변환이 필요한 소비자가 `egui-compat`을 명시하고 루트의 GUI 전용 크레이트와 아이콘 feature는 `gui`에서만 켠다. 모든 소비자에게 default-features 비활성화를 요구하면 새 소비자가 놓치기 쉬워 기본값 자체를 비운다. 빌드 성공과 GUI 의존 제거는 별도로 확인하며, feature를 변경할 때 전이 의존 그래프를 비교한다.

플랫폼 코드는 OS 호출과 신호 수신을 담당한다. 신호를 `AppEvent`나 사용자 동작으로 해석하는 코드는 호출자가 전달한 callback에 둔다. OS를 호출하지 않는 앱 상태 진단은 platform 밖에 둔다. 단순 callback 알림에 별도 채널을 추가하면 버퍼와 수명 관리만 늘어나므로 사용하지 않는다. 플랫폼 동작을 바꿀 때는 해당 OS의 컴파일과 실제 resume·dock·종료 동작을 따로 확인한다.

`tasty-font`는 GPU device를 요구하는 atlas 모듈만 기본 비활성 `gpu` feature 뒤에 둔다. 폰트 설정, metrics, 패킹·LRU·프레임 시계는 device 없이 사용·테스트할 수 있다. 헤드리스의 폰트 이름 해석은 `FontConfig`를 사용하므로 cosmic-text는 남는다. 폰트 해석 수단이 바뀌면 이 의존도 검토한다. 재수출을 지워서는 의존 그래프가 바뀌지 않으므로 manifest의 feature와 전이 의존을 확인한다.

셀 렌더러는 선택·링크·폭·폰트·모델 타입을 소유한 크레이트를 직접 참조한다. 호스트의 재수출 경로에 기대면 실제 소속과 feature 경계를 알아보기 어렵다. 호스트 접착 코드인 gpu 모듈은 앱 상태 참조를 유지하고, 공용 셀 색 계산은 본체 모듈을 함께 사용할 수 있다. 텍스트 검색은 별칭·상대 경로 의존을 놓칠 수 있어 의존 경계의 완전한 증명으로 사용하지 않는다.

OS 호출은 `tasty-platform` 크레이트에 둬 본체 타입에 직접 의존하지 못하게 한다. 본체의 별칭은 기존 호출 경로를 유지할 수 있지만 크레이트 자체는 공용 하위 라이브러리만 사용한다. 창·메뉴·트레이 기능은 gui feature 뒤에, crash report는 그 밖에 둔다. OS 경계는 egui 위젯 계층과 구분한다. 공개 API를 넓힐 때 실제 외부 호출자를 확인하고 테스트용 구현은 private로 둔다. 교차 타깃 check는 링크와 OS callback 실행을 검증하지 않는다.

`StreamHub`는 bounded sink, 프레임 분류, 손실 집계를 제공하는 공용 전송 계약이므로 `tasty-ipc`에 둔다. TCP 소켓 accept와 읽기·쓰기는 production adapter에 둔다. core는 허브를 직접 참조하고 adapter 재수출을 거치지 않는다. 파일 조립까지 확인하는 테스트는 도메인 쪽에, 분류 순서 테스트는 IPC 크레이트에 둔다. 다른 전송 수단이 추가돼 허브 자체 동작이 달라져야 한다면 sink 추상을 검토한다.

셀 렌더링에서 반복 호출하는 tasty-cell-width, tasty-terminal-link, tasty-selection은 dev에서도 opt-level 3으로 빌드한다. workspace 멤버는 외부 의존용 별표 설정에 포함되지 않아 개별 등록이 필요하다. 선택 기준은 실행 시간 감소와 수정 후 재컴파일 시간 증가를 각각 비교하는 것이다. 폭 계산·링크 검출의 반복 비용 감소가 작은 추가 컴파일 비용보다 커 채택했다. 새 크레이트를 무조건 같은 수준으로 최적화하지는 않는다. 호출이 캐시되거나 해당 크레이트 수정 빈도가 늘면 번갈아 빌드·실행해 다시 비교한다.

## 워크스페이스 크레이트 (63)

아래 목록은 낮은 계층부터 나열한다. 의존은 상위에서 하위로 향하며 순환을 허용하지 않는다. `architecture_layer_order_holds`가 매니페스트 의존과 순서를 대조한다. 크레이트 소속은 각 절 첫 문단에서 백틱 이름으로 시작하는 항목을 읽는다. 순서와 다른 의존을 발견하면 실제 의존과 문서의 계층 순서를 함께 확인한다.

계층 예외는 도메인-IO의 세 의존이다: `tasty-remote → tasty-ipc`, `tasty-file-format → tasty-plugin-protocol`, `tasty-file-handler → tasty-plugin-protocol`. 이유는 해당 절에서 설명한다.

`architecture_crate_list_complete`가 `crates/*/`의 전체 이름과 개수를 비교한다. `doc-guards.yml`이 경로 필터 없이 main push·PR에서 두 검사를 실행하며, 크레이트를 추가할 때는 push 전에 직접 확인한다([CI 가이드](../dev-guide/ci-gates.md)).

### type-\* / primitive (leaf)
`tasty-type-geometry`(길이·도형 타입: LogicalPx·PhysicalPx·Rect, 의존 없음) · `tasty-type-appearance`(색·테마 스키마·ToastKind. 그리기와 이벤트 처리에서 같은 타입 사용. egui 변환은 egui-compat, → type-geometry) · `tasty-design-tokens`(DTCG 디자인 토큰 사본·코드 생성, → type-geometry) · `tasty-utils`(경로 등 공용 함수, 저장소별 전담 writer가 공유하는 접수·묶음·완료·종료 실행기 `batch_writer`) · `tasty-cell-width`(렌더러·선택·링크가 공유하는 코드포인트별 셀 폭, 의존 없음) · `tasty-ansi`(CSI·OSC 제거 정규식. terminal의 strip-ansi와 output 파서가 공유, → regex, docs/architecture/index.md) · `tasty-timer`(주기 작업을 키로 등록하고 drain_due로 실행. 다음 기한까지 기다리는 waker 스레드 1개, → utils) · `tasty-shm`(공유 메모리와 FD/HANDLE 전달. POSIX shm·SCM_RIGHTS·Windows DuplicateHandle, workspace 및 외부 의존 없음)

이 절 안에서만 의존 가능("type-\*" 는 절 이름이고 규칙의 단위는 **절 소속**이다 — `tasty-utils`·`tasty-ansi`·`tasty-timer`·`tasty-design-tokens` 처럼 이름이 `tasty-type-` 으로 시작하지 않는 것도 이 절이다). 도메인/IO crate 의존 금지(그룹 내 순환도 금지). — [typed-length](../concepts/typed-length.md)

`tasty-shm`은 Tasty의 도메인 상태를 모르고 공유 메모리 전송만 담당하며 workspace 의존도 없다. syscall 사용 여부가 아니라 역할과 의존 방향으로 이 계층에 둔다. SDK가 shm을 사용하는 것을 도메인 의존 예외로 만들 필요도 없다.

### 도메인-IO
`tasty-themes`(전역 Theme·TOML IO) · `tasty-settings`(설정 스키마·직렬화) · `tasty-font`(글리프 atlas, → type-geometry/utils) · `tasty-terminal`(PTY·termwiz) · `tasty-hooks`(Surface Hook) · `tasty-memory`(memory.db와 그 전담 writer 실행기 `writer`, → utils) · `tasty-event-store`(SQLite event journal 저장 계약. bootstrap 제품 연결, workspace 의존 없음) · `tasty-telemetry`(사용량·진단, → memory) · `tasty-output`(출력 파서) · `tasty-approval`(승인) · `tasty-agent`(세션·수명, → memory) · `tasty-presets`(레이아웃 프리셋) · `tasty-portscan`(포트 조회) · `tasty-reaper`(자식 정리. 호스트 수명 결속은 Windows Job Object와 Linux `spawn_bound_to_host`의 PDEATHSIG이고 다른 OS에는 없다. `terminate_process_group`은 Unix에서 그룹 리더로 띄운 자식의 그룹을 SIGTERM 뒤 SIGKILL로 끝내고 Windows에서는 자식을 바로 끝낸다. `defer_termination_signals`는 Unix에서 소유한 자식을 정리하는 동안 SIGINT·SIGHUP·SIGTERM을 기록만 하게 한다) · `tasty-lua`(격리 워커·고정 host API, docs/features/hooks/index.md) · `tasty-i18n`(번역) · `tasty-remote-profiles`(연결 프로필·passkey. attach·explorer·plugin이 공유, docs/features/remote-profiles/index.md) · `tasty-ssh`(시스템 ssh 실행·터널·포트 탐색·재시도·취소. SSH 프로토콜은 구현하지 않음, → remote-profiles/i18n/utils/reaper) · `tasty-remote`(원격 workspace 조회·생성 및 연결/session·송수신·SSH 시도 수명. App/View 의존 없음, → ssh/ipc/remote-profiles/model/utils, docs/architecture/index.md) · `tasty-model`(workspace·pane·tab·surface 모델, GUI·PTY 의존 없음, → type-appearance/type-geometry/utils) · `tasty-core`(구조 Command/Event·JournalModel·codec·decide/evolve 및 CoreState의 committed projection·순수 조회·canonical 비교. GUI·PTY·이벤트 저장소 의존 없음, → model) · `tasty-dag-layout`(Sugiyama 방식의 DAG 좌표 계산. 본체·갤러리 공용, egui·Theme 의존 없음, → type-geometry. [설명](../dev-guide/dag-layout.md)) · `tasty-git-core`(git2 읽기 전용 래퍼. repo·status·log·diff·worktrees, 원격 host와 git-viewer 공용, → utils, docs/features/remote-attach/index.md) · `tasty-file-format`(detector·규칙·Lua·구조 평가. 호스트·GUI·IPC 구현 의존 없음, → utils/plugin-protocol) · `tasty-file-handler`(detector와 handler 연결·사용자 설정·plugin 등록·최근 선택. 호스트·GUI·IPC 구현 의존 없음, → utils/file-format/plugin-protocol) · `tasty-selection`(격자 선택·픽셀 변환·적중 판정·텍스트 추출. 렌더러·View 공용, → terminal/type-geometry/cell-width) · `tasty-terminal-link`(URL·파일 경로·여러 줄 링크와 강조 범위 계산. 실제 열기는 호스트 GUI에서 처리, → terminal/type-appearance/cell-width)

이 절 + type-\*/primitive 절만 의존 가능(위와 같이 판정 단위는 절 소속이다). **예외 셋** — 첫째, `tasty-remote` 는 plugin host 의 `tasty-ipc` 에 의존한다: 원격 client 능력이 IPC 호출이고, 합칠 후보 둘(`tasty-ssh` 와 `tasty-ipc`)이 각각 더 나쁜 의존을 들여 기각됐다.
그 방향은 이 문서 의 결정이다.
둘째, `tasty-file-format` 은 plugin 경계의 `tasty-plugin-protocol` 에 의존한다: plugin 이 형식 레지스트리를 조회하는 port trait 이 그 wire 크레이트에 살고, trait 소유 크레이트가 구체 레지스트리에 역의존하지 않도록 impl은 타입 소유 쪽에 둔다.
의존은 trait 정의 한 개뿐이고 역방향 호출은 없다.
그 방향은 이 문서 의 결정이다.
셋째, `tasty-file-handler` 도 같은 이유로 `tasty-plugin-protocol` 에 의존한다: plugin 이 handler 레지스트리를 조회하는 port trait 이 그 wire 크레이트에 있고, 같은 역의존을 피하려고 impl을 타입 소유자 쪽에 둔다.
둘째와 같은 형태이고 같은 결정(이 문서)의 적용이다.
이 절의 다른 크레이트에는 예외가 없다.

`tasty-event-store`는 데이터 홈 하나의 SQLite 구조 journal 저장 계약을 구현한다. App 초기 engine 생성·선택 슬롯 import·복원·슬롯 폐기는 저장 worker에 연결돼 있다. 일반 구조 명령도 App admission과 journal publication을 사용한다. source 연결과 전체 실행 검증은 구별한다. 한 journal 파일 안에서 다음을 제공한다.

- stream(엔진)별 revision과 expected revision 검사, 여러 stream을 한 batch로 묶는 원자 commit. batch는 번호와 stream별 revision vector로 식별한다.
- 이벤트·명령 기록(재시도 키·요청 digest·해소한 대상·진행 상태·응답)·effect 의무를 한 transaction으로 확정한다. 하나라도 실패하면 아무것도 남지 않는다. 재시도 키로 저장된 대상·결과를 조회할 수 있고, 같은 키·같은 요청의 재제출은 새로 쓰지 않고 기존 기록을 돌려준다. 같은 키의 다른 요청은 충돌로 거절한다.
- writer 독점 잠금과 세대(epoch) fencing. 쓰기는 journal 옆 잠금 파일(`<journal>.writer-lock`)의 OS 독점 잠금을 얻은 저장소만 하며, 잠금을 얻지 못하거나 잠금을 쓸 수 없는 환경이면 writer가 되지 않는다. 자식 프로세스는 생성부터 exec까지 방금 놓은 잠금의 파일 설명을 복제해 가질 수 있으므로, 잠금이 잡혀 있으면 최대 2초 동안 다시 시도한 뒤 실패로 돌려준다. 잠금 없이 연 저장소는 읽기만 한다. 같은 저장소가 새 세대를 등록하면 이전 세대의 쓰기를 거절한다.
- effect 상태(Pending·Running·Deferred·Succeeded·Failed·Cancelled·Superseded·Uncertain)의 허용 전이, activation claim과 attempt 기록, 이전 attempt·generation의 늦은 결과 거절.
- domain snapshot(파생 cache)과 snapshot+tail 읽기, consumer checkpoint, 불변 payload와 참조 기반 GC. manifest의 전체 복원 cut 소유와 명시 바이트 소유를 구별하여, 옛 body의 명시 참조를 지키면서 snapshot 행·tail을 정리한다. retention anchor 이후 재구성 가능한 snapshot만 fallback 후보로 사용한다. 필요한 history가 이미 정리됐으면 전체 로그가 있는 것처럼 성공하지 않고 resync 또는 복구 오류를 반환한다.
- projection 출력 행(consumer·projection version별 key→바이트)과 consumer 위치를 한 transaction으로 확정한다. 전역 cut API와 고정 stream scope API를 구별하며 부분 cut도 실제 batch에서만 추출한다. scope 변경에는 새 version이 필요하고, 같은 batch의 다른 write·위치 역행·없는 batch는 행 변경 없이 거절한다. 선택 stream의 retention floor 이전 cursor는 ResyncRequired이며 명시한 전체 출력 교체로 재동기화한다. 행을 가진 consumer는 위치만 저장하는 API로 위치를 옮길 수 없다.
- 새 명령 admission은 미확정 준비 credit만 제한하며 누적 DB·WAL 크기로 터미널 사용을 거절하지 않는다. 신규 effect commit은 Pending·Deferred·Running·Uncertain 총수 한도를 검사한다. 원 key 응답·이미 수락된 효과의 전이·cleanup은 이 압력 때문에 차단하지 않는다. 물리 디스크 hard cap은 아니며 준비 한도·동등한 휴면 슬롯 manifest 재연결·reader pin을 지키는 GC 경계는 [이벤트 저장소](event-store.md)에 있다.
- kind별 영속 ID 예약. 예약한 범위는 재오픈 뒤에도 다시 내주지 않으며, 예약 뒤 commit이 실패해 쓰지 않은 구간은 빈 채로 남는다. 상한을 넘는 예약은 되감지 않고 거절한다.

이 크레이트는 도메인 타입을 모른다. 이벤트·effect·snapshot 내용은 type tag·schema version·바이트로 저장하고 해석은 호출자의 codec이 맡는다. WAL과 `synchronous=FULL`이 실제로 적용되지 않거나 journal의 스키마 버전이 이 빌드보다 새로우면 열지 않는다. 비어 있지 않은데 journal 버전 표가 없는 SQLite 파일은 설정을 바꾸기 전에 거절하며 파일을 변경하지 않는다. memory.db·state.db와 독립된 저장소이며 그 DB들과의 원자성은 없다.

이 크레이트의 강제 종료 시험(`crates/tasty-event-store/tests/crash.rs`)은 시험 바이너리를 자식 프로세스로 다시 실행해 지정 지점에서 `abort`시키고, 부모가 journal을 다시 열어 판정한다. 판정 지점은 commit 직후(batch·명령·effect가 모두 남는다), 다음 commit 준비 뒤·확정 전(확정한 batch만 남는다), effect를 Running으로 기록한 직후(새 writer 세대가 그 attempt를 Running으로 보고 대조 전이로 닫을 수 있다), 두 프로세스의 동시 첫 열기(파일이 손상되지 않고 모든 migration이 한 번씩 적용된다)다. 프로세스 종료만 재현하며 전원 차단이나 OS 충돌에 의한 쓰기 유실은 재현하지 않는다.

CoreState의 로컬 트리는 `local_workspaces`, 원격 mirror 트리는 `mirror_workspaces`에 별도로 있다. 렌더·입력·IPC는 빌린 합성 목록을 조회하고 로컬 저장·digest는 로컬 목록만 읽는다. 혼합 표시 순서는 비영속 projection으로 관리한다([Attach 메커니즘](../dev-guide/attach-behavior.md)).

앱 진행 값은 `app::state::AppState`, 공유 저장소·OS port·작업 실행 서비스는 `app::services::AppServices`가 소유한다. 부팅 phase와 복원 요청값은 `BootProgress`, OS 창·GPU·engine worker receiver는 `BootResources`로 분리한다. 열린 View와 engine 연결 관계는 기존 registry가 원본이며 AppState에 ID 목록을 복제하지 않는다.

`SurfaceLayout::Leaf`는 ID·kind·확정 activation을 가진 `SurfaceDescriptor`다. 실행 kind 객체는 `EngineRuntime.surfaces` 한 컬렉션에 있고 Terminal/Pty 원본은 같은 engine의 TerminalStore에 있다. waker와 개별 실행 자원은 EngineRuntime에 속한다. kind·파일 형식·파일 처리기·plugin hook event 등록부는 AppServices.registries가 단일 Arc를 소유하고 EngineRuntime과 PluginManager가 같은 인스턴스를 공유한다. 전역 등록·철회는 첫 View나 살아 있는 Engine을 요구하지 않는다. `LiveDomainState`는 점유·busy·attention·입력/출력 관측을 담으며, 점유 값 변경 자체는 전송하지 않는다. Remote 모듈이 허브와 전송 큐를 맡고 로컬 readonly Terminal 표시는 EngineRuntime에 남는다. 외부 client session은 Remote가 소유하며 session 값과 sender/outbox/SSH tunnel 자원을 구분한다. socket handshake·reader/writer·heartbeat와 connection/session owner는 기존 `tasty-remote` 크레이트에 있고 App을 참조하지 않는 wake 콜백을 받는다. App은 명시 엔진/표시 대상에 수신 값을 적용한다. mirror의 논리 ID는 로컬 구조 명령과 같은 홈 journal 예약 권한에서 받는다. 실행 측에는 미리 확정된 범위만 두고 부족하면 원 Engine·연결 세대에 묶인 continuation이 기다린다. 아직 발급하지 않은 lease suffix만 같은 bank에 돌려주며 사용한 ID는 되돌리지 않는다. 연결 sender는 교체 슬롯이 아니라 고정 epoch에 묶이며 재연결 전 입력·응답 큐를 새 연결로 재해소하지 않는다. 송신 큐 압력은 연결 종료로 관측하며 남은 입력을 재전송하지 않는다. Terminal의 외부 sink는 그 bounded sender를 직접 호출하므로 별도 무제한 입력 forwarder 큐를 두지 않는다. 연결의 I/O/SSH worker는 Remote가 join 수명을 보관하고, 종료 관측은 join·실패·기한 초과를 구별한다. 연결 종료는 수신자 해제와 worker 회수를 구별하며, 기한 초과를 join 완료로 취급하지 않는다.

닫기 이행 경로는 확정 tombstone과 cleanup outbox를 기록한 뒤 정확한 옛 kind/Pty owner를 engine의 operation에 보관한다. 자원 생성의 activation claim과 종료 의무의 obligation claim은 별개이며, cleanup은 실제 회수 증거 없이 성공으로 바뀌지 않는다. 메모리 scope·관측·점유·shell hint 정리는 Engine 실행 경계가 맡고, View의 선택·화면 cache 정리와 구별한다. host/lifecycle 큐도 EngineRuntime 소유다. 앱 전역 plugin 통지는 AppState의 값 큐에 남으며 View가 없는 engine의 필수 통지와 bound retirement가 끝나기 전에 owner를 놓지 않는다. 일반 close와 Remote 구조 effect도 이 회수 경계를 사용한다. 사용자 undo는 보존된 payload에서 새 activation을 준비하고 확정 후 공개한다.

App의 저장 타이머와 종료 요청은 worker에 활성 surface의 불변 capture를 제출한다. 구조는 이미 확정 journal 원본에 있으므로 legacy layout JSON을 현재 트리로 다시 덮어쓰지 않는다. lazy leaf는 기존 data·creation seed 참조를 유지한다. capture의 content sequence는 activation과 별도로 영속 예약하며, command 기록에는 본문 해시만 남기고 내용은 BLOB에 저장한다. 사용자 닫기 undo는 snapshot 자체와 참조한 payload 목록을 함께 pin한다. 완료된 operation의 역사 참조와 현재 undo/pending cleanup의 live 참조는 구별한다. retention anchor와 reader pin이 보존할 이력·payload를 제한하고 GC는 참조가 풀린 자료만 회수한다. 보존해야 할 명령·효과 증거를 임의로 모두 삭제하는 정책은 아니다.


공통 `ViewState`는 focus·modifiers·dirty/repaint·닫기 요청과 View 수명 식별자를 가지며 OS 창·GPU는 ViewBase 자원이다. 구조 생성의 사용자 선택 continuation은 원래 View 수명과 선택 세대가 유지될 때만 적용하고 저장/replay 대상에는 넣지 않는다. View 이벤트 문맥에는 engine 수명 owner 전체 대신 실행에 필요한 대여를 전달한다. 구조 변경은 journal에 요청하고 View의 선택 후처리는 원 View와 선택 세대를 확인한 확정 결과에서 수행한다.

`tasty-core`은 순수 구조 판단과 재구성 모델을 구현한다. 연결된 bootstrap·복원 경로의 원본은 worker의 JournalModel이며, CoreState에는 확정 batch의 live projection을 적용한다. 일반 구조 생성·변경·닫기도 같은 명령 admission과 publication 경계를 사용한다. 의존 방향은 `tasty-core` → `tasty-model`이고 `tasty-event-store`에는 의존하지 않는다. 두 크레이트를 연결하는 것은 본 바이너리의 runtime 모듈이다.

- 엔진별 구조 stream: journal 하나에 엔진마다 구조 stream이 하나 있고 이름은 `structure:` 접두로 시작한다. 이 접두가 없는 stream은 구조 이벤트로 해석하지 않는다.
- 저널 전용 구조 모델(엔진 하나): workspace·category·pane·tab·surface 트리, 이름, 소속, 분할 비율, surface kind와 자료 참조. workspace의 부제·설명과 tab의 사용자 지정 이름은 typed 필드이고, metadata는 사용자 정의 키만 담는다. 선택·포커스·접힘 같은 View 상태는 담지 않는다. workspace·category·pane·tab·surface ID와 분할 방향은 `tasty-model`의 타입을 쓰고, revision·batch 번호·분할 비율 비트·surface 자료 참조처럼 저널에만 필요한 값은 이 크레이트에 둔다. 새 ID는 worker가 영속 예약한 고정 값으로 받으며 decide 중 저장소를 호출하지 않는다.
- 구조 이벤트(create·split·move·rename·close, workspace 부제·설명 설정, 저장된 원격 행 설정·삭제, tab 사용자 지정 이름 설정, metadata 설정·삭제)와 이벤트 본문 codec. 이벤트마다 type tag와 schema version을 붙이고 모르는 tag·version은 오류로 중단한다. 분할 비율은 f32 비트를 그대로 저장한다. 복원 assembly의 ID별 입력·구조·선택 map은 JSON 객체의 정규 십진수 문자열 key를 u32로 검증해 읽는다(0은 허용하고 추가 선행 0은 거절). 이벤트의 내부 type tag 아래에서도 같은 형식으로 왕복하며 범위를 벗어나거나 숫자가 아닌 key는 거절한다. snapshot 본문은 한 batch 위치의 모든 엔진 모델이며 model version과 함께 해석한다. 저장 봉투·저장 형식 버전·migration은 이벤트 저장소가 정한다.
- pure evolve: 저장 batch가 아니라 해석을 마친 도메인 batch(batch 번호와 엔진 stream별로 revision이 붙은 구조 이벤트 목록)를 받는다. batch 하나를 모든 엔진 모델 사본에 적용한 뒤 교체하므로 한 엔진만 반영되는 일이 없으며, batch 순서와 엔진 stream별 revision의 연속성을 검사한다. 이벤트가 없는 엔진 모델도 적용 위치를 옮긴다.
- decide 계약: 상태·명령만 보고 이벤트·effect·응답을 정한다. 새 ID는 예약 완료된 명령의 고정값이며 시각·명령 identity는 결정 문맥으로 받는다.

본 바이너리의 구조 저널 runtime 모듈(`src/runtime/`)은 두 크레이트와 App의 bootstrap·복원 경계를 연결한다.

- 엔진의 구조 stream 이름을 레이아웃 슬롯 번호로 정한다(`structure:slot-<번호>`). 저장 batch에서 엔진 구조 stream 이벤트만 stream별로 해석해 도메인 batch를 만들고, 도메인 이벤트 본문을 저장 봉투에 담는다. 전체 로그 replay와 snapshot+tail 재구성은 모든 엔진에서 같은 모델·ID·revision을 만든다. snapshot 하나가 모든 엔진 모델과 그 surface 자료 참조를 함께 pin한다.
- decide 계약에 대해 generic한 command executor: 재시도 키 조회를 대상 해소보다 먼저 하고, 새 요청만 decide한 뒤 명령·이벤트·effect를 한 transaction으로 확정한다. 확정에 성공한 뒤에만 메모리 상태에 적용하고 응답한다. 확정이 실패하면 상태를 바꾸지 않고 응답하지 않는다. revision 충돌이면 저장소에서 상태를 다시 읽어 정해진 횟수까지 다시 decide한다. 같은 프로세스에서 진행 중인 같은 키는 첫 실행에 합류하고 다른 요청이면 충돌로 거절한다. writer 잠금을 잃거나 fencing되면 이후 쓰기를 멈춘다. 공개 구조 요청의 도메인 거절은 이벤트·효과 없이 Failed 명령과 원래 wire 오류를 확정한다. 공개 응답 계약 없는 내부 판단 API의 거절은 저장하지 않는다.

순수 명령은 metadata·명시 ID 재정렬·대상 leaf를 대조하는 split 비율과 종류별 생성 준비·설치 완료를 결정한다. operation 준비/결과·activation·kind 변환·capture 세대는 별도 확정 사실이며 같은 activation의 늦은 capture도 거절한다. snapshot 모델은 v4이며 이전 버전의 선택 필드는 기본값으로 보완한다. 생성 준비는 activation high-water를 소비하고 단일 outbox claim 아래 설치·이전 owner 정리를 계속한다. 준비 성공 batch와 최종 완료 batch를 구분하며, 최종 commit 전에는 새 surface를 트리에 공개하지 않고 convert·restore의 기존 activation을 유지한다. 예약 generation과 claim은 operation에 귀속되며 원 명령은 InProgress다. `live_projection`은 기존 kind 객체와 SplitNodeId를 보존하며 확정 사실을 적용한다. `effect_runner`는 고정된 launch input으로 비공개 후보를 만들고 설치한다. 터미널 후보의 PTY 시작은 bounded 전용 준비 스레드가 맡고, 결과는 App이 claim 때의 대상과 대조한 뒤 후보에 붙인다([자원 회수](resource-recovery.md#생성의-완료-조건)). GUI/headless bootstrap과 workspace/tab/split 생성·변환·respawn·standalone 채택은 이 경계를 사용한다. 생성 명령의 고정 입력·공개 응답 틀은 command resolution에, 진행 상태와 먼저 완료한 부분 응답은 InProgress 기록에 보존한다. 최종 wire는 모든 부분이 끝나야 확정되며 재시도에서 live tree로 다시 만들지 않는다. 비저널 입력·관측 서비스와 구조 명령을 구별하며, 복구 시 미확정 외부 결과를 성공이나 실패로 추정하지 않는다. 미확정 생성 continuation은 이미 설치된 owner와 거절 후보의 폐기 receipt를 배타적인 소유 값으로 보관한다. 설치 도중 실패한 후보는 owner를 바꾸기 전이면 알려진 거절로 폐기하고, 그 뒤면 미공개 새 owner를 회수한 receipt와 함께 유지하며 최종 engine release에 각 원 receipt를 넘긴다. 교체한 이전 owner의 회수는 구조 공개 뒤에 관측하며 원 명령 응답만 그 회수를 기다린다([자원 회수](resource-recovery.md#생성의-완료-조건)).

standalone 채택은 같은 Terminal/Pty와 physical generation을 operation에 잠시 옮긴다. 설치 직전 종료를 다시 확인하고, 취소되면 원 키·standalone metadata·scrollback 참조를 복구한다. child terminal 생성은 같은 구조 생성 경계에서 관계·soft 점유를 설치한다. child 명령 본문은 현재 bounded 요청 continuation에만 보관하며 journal에는 hash와 입력 존재 여부만 남긴다. 이전 runtime의 채택이나 일회 child 입력을 재시작 때 자동 실행하지 않는다. 자원 교체 없는 child interrupt·tell·관계 연결은 비저널 서비스다.

`src/runtime/journal_product`의 저장 worker는 bounded 요청/완료 채널과 별도 publication ACK를 사용한다. 원본 key 조회에서 같은 진행 중 요청을 합치고, ID 예약은 decide 밖에서 수행한다. 저장소의 canonical 적용 뒤에도 App의 전체 batch 적용 ACK 전에는 성공 응답과 다음 변경을 내보내지 않는다. 부팅 replay cut도 초기 projection ACK가 필요하다. App이 worker를 소유하고 각 EngineSession에 명시 binding을 둔다. 추가 창은 초기 구조와 선택 자원이 준비된 뒤 성공 응답을 내며, 전체 batch 공개 실패는 App의 조회·명령·렌더를 멈춘다. 승인된 외부 설치와 최종 구조 공개 사이에도 App이 관측을 보류한다. IPC·plugin·remote의 기존 수신 큐와 별도로 OS 입력만 수신 당시 View/대상 binding을 가진 한시적 bounded 큐에 보관하며 raw 입력을 journal에 저장하지 않는다. geometry 입력은 구조 revision, surface 키 입력은 확정 activation·물리 generation·kind binding, 호스트 popup 입력은 View의 입력 수신자를 대조한다. 종료·복구에서도 원 binding과 publication ACK를 대조한다. 이 연결이 모든 App 장애 지점이나 모든 OS의 crash 내구성을 실측했다는 뜻은 아니다.

공개 카테고리 변경과 workspace metadata·순서 요청은 GUI/headless·plugin 공통 journal admission을 사용한다. 현재 권한 검사를 먼저 하고 원래 요청의 key를 조회한 뒤 miss만 대상을 고정한다. 동일 요청은 진행 중에도 합류하며, 공개 응답과 거절은 commit에 고정한다. InProgress로 대기하는 명령과 합류한 요청은 생성·원격 전달·자원 정리가 끝날 때마다 명령 기록을 다시 조회한다. 명령마다 worker에서 처리 중인 요청은 하나뿐이며, 앞선 요청의 완료를 받기 전에는 같은 명령의 새 요청을 제출하지 않는다. worker는 credit을 기다리는 admission을 제외한 요청을 제출 순서대로 처리하고 응답한다. 명령 기록 조회와 이 조회를 부르는 완료는 credit을 기다리지 않으므로, 처리 중인 조회는 그 앞에 끝난 완료를 이미 반영한다. byte pressure와 요청 크기 거절은 해당 요청만 끝내고, canonical 적용/공개 실패는 typed halt로 App까지 전달한다. 어느 소유자(command·자원 요청·restoration·creation·opening)도 받지 않은 worker 완료는 소유자 장부의 불변식 위반이라 같은 halt로 처리한다. 이 halt 사유에는 ticket, 응답 종류(오류면 문구 앞 160자), 그 ticket으로 command가 마지막에 제출한 작업 종류(최근 제출 256건 안에서 찾는다), 진행 중인 opening·command·creation 수가 들어가며 `journal publication halted` 오류 로그에 그대로 남는다. engine binding 응답이 어느 opening과도 맞지 않으면 opening 불일치로 따로 적는다. halt 뒤 IPC는 `system.shutdown`을 빼고 `-32000`으로 거절하며, 메시지에 재시작 안내와 halt 사유 앞 512자를 담는다. 설정의 category reset은 모든 engine의 한 batch이며 늦은 완료가 더 최신 설정을 덮지 않도록 continuation 세대를 대조한다.

mirror 이름·부제·설명·분류와 혼합 표시 순서는 비영속 App continuation이다. 로컬 상대 순서는 확정 모델을 따르며, continuation은 local tree를 재정렬하지 않는다. mirror 교체 때 발급한 token으로 delta·재연결 이후의 오래된 표시 변경을 버리고, 저장 응답 재시도에는 표시 변경과 host 알림을 재발행하지 않는다. 이 token은 transport reconnect epoch와 별개다.

직접 이름 변경 팝업은 확정 후 origin에 맞는 host 알림을 보낸다. 탭 이름 해제 알림은 그 시점의 View 선택 surface 제목을 읽는다. 분할선 드래그는 View 미리보기이며 시작 revision과 명시 split 대상을 고정한 순수 명령으로 끝낸다. `CoreState.committed_structure_revision`은 검증된 live projection의 읽기용 적용 위치이고 명령 원본 모델은 아니다. 로컬 탭 이동과 공개 tab.move도 고정 TabId를 journal로 넘긴다. 원격 구조 수신은 원 holder registration의 응답을 delta·새 terminal tap보다 먼저 보낸다. 재연결의 새 grant에 이전 입력·응답을 재사용하지 않는다.

정상 slot resume는 구조 ID와 incarnation을 유지한다. 복원을 끄고 버리는 engine은 retirement fact를 확정한 뒤 정확한 실행 owner를 회수한다. 폐기된 slot 재사용은 새 incarnation이며 과거 ID·명령·미완 효과는 되감지 않는다. 최초 창은 worker의 journal 슬롯 목록을 받은 뒤 활성 저장 슬롯을 고르고, View 선택은 해당 binding과 공개 cut을 가진 별도 checkpoint로 저장한다([이벤트 저장소 (`tasty-event-store`)](event-store.md)).

데이터 홈의 명시 binding은 `structure/journal.json`, 저장소는 `structure/journal.db`다. 초기화 잠금 아래 Preparing→Ready로 확정한다. Ready binding에서 DB가 사라지면 빈 journal을 만들지 않는다. 파일을 flush한 뒤 Unix에서는 상위 디렉터리를 sync하고 Windows에서는 write-through rename을 사용한다. 이 절은 전원 차단이나 미실행 OS에서의 내구성을 실측했다는 뜻이 아니다.

기존 layout 슬롯 importer(`src/core/layout_persistence/import.rs`)는 선택한 slot stream이 없는 첫 bootstrap에서만 호출한다. 이미 journal이 있는 정상 resume는 남은 legacy 파일을 다시 읽지 않는다. 슬롯 JSON 하나를 기존 슬롯 판정(높은 version·해석 실패 거절)으로 읽고, 그 슬롯 엔진의 구조 stream에 이벤트 batch 하나와 명령 기록으로 확정한다. 새 ID는 journal의 ID 예약에서 받으므로 여러 슬롯을 가져와도 엔진 사이에서 겹치지 않으며, surface ID는 standalone PTY ID 기준값 아래에서만 받는다. 슬롯 안의 위치(workspace 순서, 깊이 우선 leaf pane 순서, tab 순서, 깊이 우선 surface 순서)와 새 ID의 대응은 결과와 명령 기록에 남기며, 같은 슬롯을 같은 내용으로 다시 가져오면 저장된 대응을 돌려주고 다른 내용이면 거절한다. workspace 부제·설명과 tab의 사용자 지정 이름은 전용 이벤트로 기록한다. 옛 슬롯의 원격 attach 매핑은 읽고 버린다. terminal의 cwd·복원 명령·scrollback과 plugin surface 자료는 이벤트가 아니라 surface 저장 자료 payload 하나로 저장하고 이벤트에서 pin하며, terminal에 저장할 값이 없으면 자료 참조를 만들지 않는다. Generic의 null도 restore 호출을 유지하도록 명시 payload로 남긴다. scrollback 파일이 없으면 저장 자료에 참조만 남긴다. 선택 workspace·focus pane·선택 tab·카테고리 접힘은 이벤트로 만들지 않고 결과로만 돌려준다.

구조 digest(`crates/tasty-core/src/canonical.rs`, CoreState 쪽은 `crates/tasty-core/src/canonical/live.rs`)는 CoreState와 엔진 구조 stream 하나의 journal 모델이 같은 구조를 나타내는지 비교하는 도구다. 제품 incremental projection의 전후 구조 확인에도 쓰며, [이벤트 저장소 (`tasty-event-store`)](event-store.md)의 전체 writer shadow 대조·활성화 완료와는 구별한다.

- 비교 범위: 두 쪽을 같은 정규 표현으로 옮긴다. category 순서와 이름, workspace 순서·이름·소속 category·부제·설명·metadata, pane 분할 트리(방향, 비율의 f32 비트), pane마다 tab 순서·이름·사용자 지정 이름, tab마다 surface 분할 트리, surface kind·metadata·저장 자료를 담는다. digest는 정규 표현 직렬화의 FNV-1a 64비트 해시이고, 차이를 찾을 때는 경로별 차이 목록을 쓴다. journal 쪽은 순서 목록에서 닿지 않는 항목과 부모 역참조가 맞지 않는 항목도 결함으로 담는다.
- ID: 원래 ID로 비교하거나, category·workspace는 표시 순서로, pane·tab·surface는 전체 깊이 우선 순서로 번호를 다시 매겨 비교한다. importer는 새 ID를 받으므로 CoreState와 가져온 모델은 순서 번호로 비교한다.
- 저장 자료: payload 번호가 아니라 내용으로 비교한다. 바이트 길이와 해시로 비교하거나, surface 저장 자료 형식으로 해석해 비교한다(scrollback은 길이와 해시). 비교에서 뺄 수도 있다.
- CoreState 쪽 정규 표현은 현재 descriptor 트리를 읽는다. journal의 전체 로그 replay와 snapshot+tail 재구성 비교는 `src/runtime/tests/shadow_digest.rs`가 맡는다. 저장 형식 호환은 legacy JSON schema/import 시험으로 대조하며, 옛 mutable CoreState capture/restore 구현을 별도 정본으로 실행하지 않는다.
- 비교에서 빼는 값: `SurfaceLayout::Split.node_id`(프로세스 내부 호환 projection 식별자), `Workspace.mirror`(원격 구조는 로컬 비교에서 통째로 제외), `JournalModel.applied`(journal 위치), `JournalModel.remote_rows`(저장된 원격 행은 원격 구조를 가리키는 값이라 로컬 비교에서 제외). 사용자 선택·접힘·탭바 스크롤은 CoreState 원본에 없으므로 이 제외 명부의 필드가 아니다.

### UI primitive
`tasty-egui-theme`(Theme를 egui Visuals/Style로 변환) · `tasty-ui-widgets`(본체·갤러리 공용 egui 위젯·배치 함수. [설명](ui-widgets-crate.md)) · `tasty-icons`(line/fill SVG. 본체·갤러리와 plugin 빌드가 공유) · `tasty-key-match`(바인딩과 키 이벤트 대조. 단축키·webview 공용, egui 입력은 egui-input feature, → settings/winit)

### OS 경계
`tasty-platform`(panic·hang 진단, 호스트 로그, 네이티브 메뉴, 트레이, jump list, 전원 재개, 창 크롬, OS 화면 캡처, macOS 권한·Dock·메뉴바, 이벤트 루프 watchdog. App 해석은 호출자가 callback으로 전달. 창·GTK·AppKit·트레이는 gui feature 뒤이며 headless에는 crash 진단이 포함됨, → i18n/settings/utils, 이 문서)

`tasty-platform`은 본 바이너리가 사용하고 도메인-IO와 primitive 계층에만 의존한다. 네이티브 메뉴·트레이는 egui 위젯이 아니므로 본체·갤러리 공용 UI 계층과 구분한다.

### plugin protocol / SDK (프로토콜·프로세스 경계)
`tasty-plugin-protocol`(호스트·plugin 전송 타입, → type-appearance) · `tasty-plugin-sdk`(외부 plugin SDK, → protocol/shm/utils/i18n) · `tasty-plugin-sdk-wasm`(WASM 타깃 SDK) · `tasty-plugin-agent-common`(Claude·Codex 공용 prompt 임시파일·훅 정리·children 응답·reboot 인자. 매니페스트가 없어 번들 plugin은 아님, → sdk)

이 계층은 protocol·SDK를 통해 호스트와 통신하며 도메인-IO에 직접 의존하지 않는다. 이 경계가 OS 샌드박스를 뜻하지는 않는다. **예외 하나** — `tasty-plugin-sdk` → `tasty-i18n`: 호스트와 SDK 가 설치·사용자 plugin 카탈로그 로딩을 공유한다.

### plugin host (IPC 인프라)
`tasty-plugin-manifest`(매니페스트 스키마·파서) · `tasty-ipc`(JSON-RPC 메시지·caller·audit·method_meta·port trait·클라이언트 연결·HostIpcInjector·StreamHub. TCP 소켓 처리는 본체 tcp_ipc_server adapter, 이 문서) · `tasty-host-plugin`(호스트의 plugin manager·process·event bus·registry)

### 실행 runtime
`tasty-task-runtime`(TaskService/TaskScope·runner thread·작업 실행·완료 대기 hub/feed와 hook wait. task 원본은 tasty-agent/MemoryStorage이며 App/Core/View 의존 없음, → agent/memory/ipc/utils). App은 실제 completion registry resolver와 명시 DAG 대상 값을 주입한다. runner stop, task 취소, OS child 종료는 별개 계약이다.

### 번들 plugin (bin 크레이트, 모두 `tasty-plugin-sdk` 의존)
`tasty-plugin-claude` · `tasty-plugin-codex` · `tasty-plugin-git-viewer` · `tasty-plugin-clipboard-viewer` · `tasty-plugin-image` · `tasty-plugin-html` · `tasty-plugin-markdown` · `tasty-plugin-agent-stream` · `tasty-plugin-mesh-demo`(+ manifest). 뒤의 둘은 `bundle = false` 라 배포 패키징에서는 빠지고 dev 번들 sync 로만 붙는다. — [concepts/plugins](../concepts/plugins.md)

### 도구 / standalone
`tasty-tui-simulator`(E2E TUI 시뮬레이터, lib + `tasty-tui-sim` binary — 로직은 lib 공유, debug 빌드에선 `tasty debug sim` 으로도 노출) · `tasty-gallery`(ui-widgets 데모 바이너리, `cargo run -p tasty-gallery` — 본체 빌드와 분리)

### CLI client
`tasty-cli`(clap CLI — request/format/transport/dynamic plugin subcommand. → ipc/host-plugin/terminal/approval/remote-profiles/remote/ssh/i18n/plugin-manifest/plugin-protocol/tui-simulator/utils)

### 테스트·가드 전용 (제품 산출물 밖)
`tasty-latency-control`(CPU 작업 또는 자식 프로세스 시작으로 지연의 대조군 제공. 실패 메시지는 사용한 종류를 표시. 의존 없음, dev-dependencies로만 사용, [수정 후 자체 검증 (Self-verification)](../dev-guide/self-verification.md)) · `tasty-doc-guards`(문서·소스·workflow를 대조. 의존 없이 빠르게 실행해 문서만 바뀐 main push도 검사, [가드 검증](../dev-guide/guard-verification.md)) · `tasty-test-support`(테스트 동안 전역 env·TASTY_HOME을 바꾸고 Drop에서 복원하는 RAII 가드, ETXTBSY 없이 시험용 실행 파일을 쓰는 `write_executable`(unix), `/proc/<pid>/stat`으로 프로세스 종료를 읽는 `proc_stat_reads_finished`, 시험 하나를 자기 바이너리의 새 프로세스에서 혼자 다시 실행하는 `rerun_alone`과 그 자식 안인지 확인하는 `is_rerun_alone`, → utils/tempfile. dev-dependencies로만 사용)

### 본 바이너리 (`tasty`)
위 크레이트를 의존하며 App/View/GPU/IPC 라우터/부팅을 제공.

## 본 바이너리 모듈 (`src/`)

ports-and-adapters 배치:

| 모듈 | 역할 |
|------|------|
| `boot/` | `fn main` 부팅 시퀀스(`run()` 진입점) — event_loop, headless_{dispatch,stream,plugins}, cli_routing, wiring, locale, trace(부팅 계측) |
| `app/` | `App`(winit `ApplicationHandler`) — window_lifecycle, boot_machine(첫 윈도우 부팅 상태 머신 — [boot-sequence](boot-sequence.md)), shutdown_cascade(종료 cascade — [shutdown-sequence](shutdown-sequence.md)), modal, ipc dispatch, attach, journal admission/publication·retirement receipt — [close-sequence](close-sequence.md) |
| `core/` | 호스트 측 도메인 adapter·live 정책·legacy layout 이관. 순수 구조 원본과 projection은 tasty-core에 있다 |
| `runtime/` | EngineSession, EngineRead/EngineRef/EngineMut, TerminalStore, effect·자원 receipt, journal worker adapter. tasty-core과 tasty-event-store 사이의 batch 변환·replay·snapshot·command executor·projection 연결 |
| `remote/` | engine별 attach 구독·표시·전송 adapter. socket/SSH owner는 tasty-remote |
| `hub.rs` | **외부 통신**(`Hub`) — IPC 서버, 포트 파일 |
| `view/` | **GUI**(gui-gated) — `View` sealed trait 계층 + MainView/SettingsView/QuitView/PluginsView/PresetView. — [multi-window](multi-window.md) |
| `state/` | View별 navigation·viewport·편집·표시 상태. Headless는 별도 CommandContext를 사용하고 요청 presentation은 RequestScope가 빌린다 |
| `gfx/` | GPU — `GpuState`, renderer(셀 렌더), screenshot, perf. — [gpu-rendering](../dev-guide/gpu-rendering.md) |
| `adapters/` | 외부 경계 구현 — `ui`(egui 컴포넌트·popup), `ipc`(handler), `production`/`test`(port 구현체), `cli`, `plugin` |
| `ports/` | **의존성 역전 trait** — ipc_server, clipboard, clock, home, notification_sound (production/test adapter 가 구현 → headless·테스트 교체). 도메인의 일부다 |
| `intent/` | **Intent 큐** — 호스트 내부 동작 디스패치. — [action-dispatch](../design/flows/action-dispatch.md) |
| `host_api/` | 호스트가 외부(plugin/agent)에 제공하는 인터페이스 — Lua hooks, webview |
| `hook_runtime/` | 엔진별 훅 등록·감시 상태(`HookRuntimeState` — surface 훅·전역 훅)와 발화한 훅의 실행(바인딩 실행 · 전역 훅 셸 실행 · IpcSequence worker). 공유 handler 정의 registry는 `hook_handler/` |
| `plugin_bridge/` | 호스트 측 plugin 라우팅 facade |
| `store/` | 인메모리 스토어 — notification, state.db 수명의 창 간 공유 recent_files |
| `db/` | SQLite `state.db`. — [storage](../design/systems/storage.md) |
| `file/` · `clipboard/` | 파일 핸들러/디스패치(형식 식별 자체는 `tasty-file-format`) · 클립보드. OS 경계(crash_report·native menu 등)는 `src/` 를 떠나 `tasty-platform` 크레이트에 있고, 본체는 `crate::platform::…` 별칭으로 부른다 |

## 데이터 흐름

주요 흐름 5종(키 입력→렌더, PTY 출력→파싱→렌더, IPC 요청→응답, 알림, 설정 로드→적용)의 단계별 경로는 [data-flows](data-flows.md). 호스트 내부 동작이 Intent 큐로 통일된 디스패치 모델은 [action-dispatch](../design/flows/action-dispatch.md).

## 하위 문서

| 문서 | 설명 |
|------|------|
| [boot-sequence](boot-sequence.md) | 첫 윈도우 부팅 상태 머신(BootPhase) — hidden 생성→로딩 프레임→표시, 로딩 중 OS 포커스 보존, 프레임 구동 대기, 헤드리스 메모리 초기화 모듈, 부팅 계측(T1~T7) |
| [shutdown-sequence](shutdown-sequence.md) | 정상·오류 모달의 공통 종료와 exit status, native webview 숨김·cascade·Drop tail, observer의 일반 retirement 완료 대기·앱 종료 비동기 회수와 timeout, writer별 실제 flush·join과 cut 이후 시간 관측·debug 콜백 raw 표본, GPU 해제까지 유지하는 native display 수명, 종료 계측(S1~S5), file receipt·C1 live effect·preset·plugin continuation과 원 요청 응답 순서 |
| [close-sequence](close-sequence.md) | 확정 닫기 · 원 자원 retirement receipt · 불명 결과와 명령 완료 · engine/슬롯 해제 |
| [event-store](event-store.md) | 구조 journal 저장 계약의 수치·보조 규칙 — 요청별 savepoint·물리 transaction 공유·private SQL 조회 실패의 전체 rollback·batch별 ACK·최초 엔진 binding의 고정 metadata 제출, 실제 SQL commit·worker 구간 계측, import 전송 identity, 잠금 없는 쓰기, ID 예약 경계, 로그 보존, projection 출력, 신규 admission 예산·쓰기 크기 상한, 구조 journal의 App 연결 |
| [journal-metrics](journal-metrics.md) | GUI/headless system.pressure journal cache: Work/queue/bytes estimates, actual SQL COMMIT counters, fixed histograms and pending/publication ages |
| [surface-capture-cost](surface-capture-cost.md) | Full immutable terminal capture: parser-lock copies, serialization, worker writes/maintenance, bounds and an isolated production-path measurement |
| [multi-window](multi-window.md) | AppServices·EngineSession·ViewRegistry 소유, parked/pending/retiring, 모달과 읽기 대여 |
| [input-layer](input-layer.md) | X11 조합 종료 키와 마우스 입력 z-order 계층 — 소비/버블링 + 커서 결정 |
| [data-flows](data-flows.md) | 주요 데이터 흐름 (파일+함수 기준), 설정 저장 receipt와 부팅 순서 |
| [ipc-server](ipc-server.md) | IPC 서버가 요청을 받아들이고 처리하는 쪽의 규칙 — 전송·저장·읽기 상한의 소유 · journal 오류 분류 · dispatch 회차 예산 · 기한 · wake · 요청 압력 게이지 |
| [ui-widgets-crate](ui-widgets-crate.md) | `tasty-ui-widgets` — 본체·갤러리 공유 UI primitive |
| [Invariants](#invariants) | 깨지면 안 되는 시스템 조건 (surface-cwd 등) — 아래 절 |

## Invariants

*깨지면 안 되는 시스템 조건* — 코드 변경 시 가장 먼저 점검할 리스트. 각 invariant 는 가능하면 컴파일/CI 로 강제하고, 그게 불가능하면 review 로 지킨다.

| Invariant | 적용 시점 | 강제 기제 |
|-----------|----------|----------|
| [surface-cwd](../design/policies/cwd.md#surface-cwd-invariant) | surface 생성/변환 | `Surface::source_cwd()` default 없음 — compile-time |
| 포커스 독립성 | 모든 CLI/IPC 명령 | review (전 워크스페이스 순회·ID 직접 지정) — [focus 정책](../design/policies/focus.md) |
| 사용자/에이전트 행동 분리 | release 빌드 IPC 노출 | review + `#[cfg(debug_assertions)]` 격리 — [debug-ipc](../dev-guide/debug-ipc.md) |
| Intent 디스패치 규율 | 호스트 내부 동작 | `check-intent-discipline.sh` 소스 검사 — [action-dispatch](../design/flows/action-dispatch.md) |

> 새 invariant 는 *위반이 조용히 통과하면 큰 회귀* 인 약속만 등재한다. 일반 코딩 규칙은 [CLAUDE.md](../../CLAUDE.md)/dev-guide 로.

결정의 *근거/대안/재검토 조건*(보류 결정 포함)은 [ADR](../adr/index.md).

### View 명령 출구와 구조 writer 이행

구조 생성·이동·닫기의 제품 원본은 journal 명령 경계이며 옛 `structural_exec`/`CascadeWindow`와 AppServices의 직접 구조 writer는 제거했다. AppServices의 `apply_live`는 비저널 입력·관측·알림만 처리한다. View는 고정 pane/surface 대상으로 생성·변경 요청을 만들고, App이 확정 결과를 받은 뒤 원 View identity와 선택 세대를 대조하여 팝업·포커스만 갱신한다. 이미 요청한 이미지 생성 등 실행 의무는 이 표시 continuation과 분리한다.

탭의 OSC/CWD 제목 관측은 Engine의 LiveDomainState가 보유한다. 순수 Tab은 명시 이름·기본 이름을 가지고, 조회 때 별도로 받은 관측 값을 합성한다. 지연 surface 준비는 표시 중인 IDs를 받은 App materialization에서 실행하며 View가 PTY/factory를 직접 만들지 않는다. `image.open`의 외부 요청은 기존 plugin namespace를 거치고 소유 plugin의 host fallback에서 journal 변환을 실행한다. 외부 namespace 키 계약을 전체 경로 durable 보장으로 확대하지 않는다.

ViewCtx는 Core/Live 관측과 표시용 query를 빌리는 EngineRead를 받는다. surface 조회는 내부 실행 객체를 숨긴 SurfaceRead를 반환하며 Any downcast를 제공하지 않는다. 내장 값 모델의 불변 표시 참조와 RemoteSurface의 URL·탐색 상태 getter만 열고, HTML gate와 native navigation 실행은 App adapter가 EngineMut와 View의 native handle을 함께 빌려 수행한다. TerminalRead는 PTY 소유자·입력·resize·tap 쓰기를 노출하지 않으며 DAG는 목록·runner 상태 값만 조회한다. kind catalog는 등록 시 만든 immutable metadata Arc와 불투명 등록 identity만 제공하고 factory·등록·철회는 실행 owner에 남긴다. RequestContext의 저장소 잠금 bridge는 제거했다. IPC engine handler는 동기 RequestScope의 borrowed 선택·권한 근거와 요청별 출력 큐를 사용하고 GUI/debug 조작은 별도 경로다. 설정 editor는 편집값만 반환하며 App이 모달을 열 때 잡은 registry owner로 저장한다. PluginManager 표시 조회는 PluginDisplay로 제한하고 전송 요청은 App의 프레임 효과(FrameEffect) 출구로 보낸다. preset editor는 값 초안과 비교 후 저장 요청만 보유하며 App이 저장한다. lazy preset capture는 원 payload read lease를 유지한 worker 읽기와 App 저장 완료를 거쳐 응답한다. capture/export는 고정된 원본과 payload lease를 사용하고 Recovery는 원 자원 회수 증거와 미확정 결과를 구별한다. 이러한 연결과 개별 검증이 모든 OS·장애 조합의 실행 검증을 뜻하지는 않는다.

지연 입력과 attach도 App이 materialization 완료를 기다린 뒤 실행한다. public wake의 완료와 메모리에만 남는 원 입력을 구별하며, 입력은 원 Engine/PTY generation과 현재 점유를 다시 확인한다. terminal tell·broadcast와 cwd 없는 respawn은 요청 시점에 IPC 봉투가 wire를 해석하고, App의 `PreparedTerminalInput`(`src/app/terminal_input.rs`)이 대상(child의 부모·index·mirror 연결)과 본문·옵션을 값으로 고정해 보관한다. 재개 때 원 요청은 호출자 확인과 응답 봉투로만 쓰며 params를 다시 해석하지 않는다. 본문 사본은 대기 무게에 포함한다. 실행 함수는 typed 값과 입력에 필요한 능력만 여는 `TerminalInputAccess`를 받고 결과를 IPC가 기존 응답·오류로 바꾼다. 그 밖의 surface 입력은 원 요청을 일반 요청 경로로 다시 실행한다. terminal tell의 body-ack/settle/CR은 Engine이 보유하고 App deadline에서 진행한다. child 생성의 일회 입력도 같은 body-ack/settle 시계(`PendingSubmit`)를 쓰지만, 구조를 공개한 뒤 생성 continuation이 CR을 보내고 그 결과를 원 요청 응답에 반영한다. attach는 연결 registration과 grant readiness를 보관하여 초기 descriptor보다 delta/입력이 앞서지 않게 한다. 이 대기 관계는 journal의 구조 원본이나 과거 raw 입력 재생 목록이 아니다.

탭 이동 host event는 committed projection 또는 Remote replacement의 이전/이후 값에서 생성한다. View polling에 lifecycle 통지 의무를 남기지 않으며, 사용자 생성의 tutorial/팝업·선택은 원 View가 살아 있을 때만 수행하는 별도 표시 동작이다.

탐색기·DAG kind 변경, 클립보드 이미지 파일 저장·업로드, 점유 해제·즐겨찾기 저장은 View에서 고정한 대상 binding을 받아 App이 실행한다. Explorer의 copy/move/trash/rename/open은 고정 경로·User origin·SurfaceBinding·View identity를 담은 별도 파일 요청으로 전달한다. App의 `explorer_files`가 View별 대기 8개·요청당 1MiB 한도와 동시 worker 1개로 실행하며 시작과 완료에 원 대상을 대조한다. cut clipboard와 Explorer 선택 identity가 바뀌면 늦은 결과가 새 clipboard나 선택을 지우지 않는다. 시작한 작업은 View close로 취소하지 않으며 종료는 실제 join을 관측하고 5초 기한 초과를 완료로 간주하지 않는다. 로컬 디렉터리 목록·Git HEAD·스크립트 읽기는 별도 App read worker로 실행한다. View는 자체 선택/cache만 바꾸며 structural tab 순서는 journal 명령으로 요청한다. RSS 이상 감지·순번·저장은 창 유무와 독립적인 App 서비스이고, View가 있을 때의 알림 대상 선택은 그 뒤의 표시 정책이다.

App의 window event provider는 EngineRegistry에서 불변 EngineRead를 만든다. 프레임은 View의 geometry 계산과 App 실행 적용을 분리하며, 설정·kind owner가 필요한 부팅/추가 창 경로는 EngineSession을 만들고 AppServices의 registry Arc를 주입한다. system.info와 tree/Lua snapshot의 공통 query는 표시 선택·active index와 EngineRead를 받아 pure Core에 실행 필드를 다시 넣지 않는다.

저장 checkpoint는 성공한 capture/engine retirement의 전체 publication ACK 뒤와 정상 worker 종료에 실행한다. canonical cut과 published cut이 다르거나 projection이 halted면 수행하지 않는다. checkpoint 오류는 이전 snapshot/live pin을 유지한 유지보수 실패이며 이미 확정된 명령 결과를 바꾸지 않는다. 다음 capture/retirement/정상 종료에서 다시 시도한다.

비동기 프리셋 draft의 immutable payload 참조는 App이 freeze와 같은 turn에 JournalWorker의 read lease로 보유한다. worker checkpoint는 같은 reader 집합의 락을 유지하며 snapshot pin에 참조를 포함한 뒤 GC/retention을 적용한다. 단순 capture cut이나 DataRef 값 복사만으로 payload 보존을 대신하지 않는다. View의 포커스 관측도 bound 프레임 효과(InputEffect::FocusObserved)로 전달하고 App이 같은 대상 세대일 때 attention/soft occupancy를 갱신한다.

구조 journal의 batch 적용은 변경 stream의 private candidate만 복제해 검증한 뒤 함께 공개한다. 변경 없는 stream은 내용을 복제하지 않고 applied batch 위치만 이동한다. publication 전후 대조 사본도 해당 작업의 stream으로 제한하며, 여러 stream이 바뀌면 모든 적용과 ACK를 한 공개 단위로 유지한다.

명령 해소는 engine 범위를 고정한다. 확정 후 외부 effect를 기다리는 명령은 같은 engine의 후속 변경을 막으며, 범위가 확인된 다른 engine의 명령은 독립적으로 진행할 수 있다. 범위 미확정·다중 stream 작업은 전역 순서를 유지한다. 저장 worker의 commit·publication ACK 순서는 바꾸지 않는다.

### 생성 continuation의 불확실 상태

`Creation::Stage::Uncertain`은 원래 설치·폐기·실패한 설치의 자원 소유자와 저널 확정 대기,
재확인 시각을 함께 보관한다. 부분 실패한 설치 owner는 오류 경로에서만 별도 할당해
일반 단계 값의 크기를 늘리지 않는다. 확정 응답 전에는 외부 자원 재확인을 시작하지 않는다.
엔진 종료 시 이 단계가 보유한 원 retirement receipt를 release owner로 넘기며,
저널 완료나 요청자 취소만으로 물리 자원이 해제됐다고 판단하지 않는다.

준비 응답 수신 여부는 `Prepared` 단계에 속하며 설치·폐기 단계가 그 값을 넘겨받는다.
설치 완료 응답과 최종 publication은 서로 다른 조건이다. assembly 응답이 먼저 도착하면
`AwaitPublication`에서 기다리고, publication이 먼저 도착하면 `Finish` 안에 적용 사실을
보관한다. 두 조건을 충족한 assembly만 `Published`에서 전체 batch ACK 뒤 해제된다.

Creation은 ticket 하나로 모든 단계를 진행하므로 한 번에 요청 하나만 worker에 제출한다.
제출한 요청의 종류를 기록하고, 응답이 오기 전에 생긴 다음 요청은 대기열에 두었다가 응답을
받은 뒤 제출한다. Publish 처리 중 설치나 후보 폐기가 실패하면 준비 응답이 아직 오지 않았을 수
있다. 이때 불확실 기록은 준비 응답을 받은 뒤 제출한다. 준비 응답은 불확실 기록의 확정으로
처리하지 않으며, 재확인 요청은 불확실 기록의 응답을 받은 뒤에만 제출한다. 응답은 기록한
요청 종류와 짝지어 판정한다. 종류가 맞지 않는 응답은 단계 밖 응답으로 거부한다.

### Engine 실행 어댑터의 위치

터미널 접근·출력 읽기·전경 프로세스와 busy 관측은 `runtime`의
`terminal_access`·`terminal_output`·`terminal_activity`가 맡는다. 타이머에서 훅 실행으로
연결하는 어댑터는 `hook_runtime::engine`에 있다. `runtime::child_observation`은 실제
TerminalStore·hook 보고에서 근거를 모으고, `core::state::child_liveness`의 순수 판정 함수에
전달한다. 판정 우선순위와 IPC 응답 문자열은 실행 어댑터의 위치와 독립된 계약이다.

## 실행 경계의 배치 기준

상태 객체마다 crate를 나누거나 모든 trait을 모으는 공용 ports crate를 만들지 않는다. port는 이를 소비하는 실행부가 소유한다. 소비자 없이 공용화하면 모든 crate가 추상 계약 하나에 의존하며 서로 다른 수명이 섞이기 때문이다.

TaskService와 HookRuntime은 서로 의존하지 않고 App이 port로 연결한다. 작은 훅 조건 계산 라이브러리에 plugin·IPC 실행을 넣거나 거대한 HostServices 하나로 합치지 않는다. 크레이트 분리는 의존 경계이며 프로세스·스레드 분리나 OS 샌드박스를 뜻하지 않는다.
