# 레이아웃 영속화 (Layout persistence)

## 원칙과 현재 동작

[복구 원칙](../../principles/feedback-and-recovery.md)은 일부 손실 시 가능한 부분 복원, 전체 읽기 불가 시 오류 알림·앱 우측 상단 비포커스 팝업 후 초기 상태에서 계속 작업을 요구한다. surface 하나의 복원 자료나 레이아웃 stream 하나의 기록을 읽지 못하면 그 범위만 경고와 함께 잃고 나머지를 복원한다. 부팅 중 저장소 전체를 열지 못하면(읽기 권한 없음, 저장소가 아닌 바이트 등) 첫 창은 이번 실행에만 쓰는 빈 임시 저장소로 기본 workspace와 셸을 연 뒤 오류 알림과 오른쪽 위 비포커스 안내(원인·원본 경로)를 띄운다. 원인 문구는 읽을 권한이 없으면 읽기 거절, 파일 머리가 SQLite 저장소가 아니면 손상, 그 밖(머리는 맞지만 열리지 않음, 쓰기 거절, 파일 없음 등)은 손상으로 단정하지 않는 "열지 못함"이며 실제 오류는 알림 항목 본문에만 붙으므로 그 문구는 알림 항목을 가리킨다. 원본 `structure/journal.db`와 부속 파일은 바꾸거나 옮기거나 지우지 않는다. 원인을 가리려고 `journal.db` 머리 16바이트만 읽는다. 이번 실행의 구조 journal 기록(workspace·탭·레이아웃 배치와 복원용 상태)은 임시 저장소에만 쌓이고 정상 종료할 때 지운다(강제 종료되면 OS 임시 폴더의 `tasty-ephemeral-journal-*`에 남을 수 있다). 터미널에서 프로그램이 바꾼 파일과는 무관하다. 그래서 다음 실행은 원본을 다시 읽어 읽을 수 있게 되었으면 원래 배치를 복원한다. 첫 View와 IPC 공개 전에는 binding 이후의 복원·첫 workspace 저장 실패도 임시 상태로 계속한다.
이때 원 worker를 중단하고 종료 checkpoint를 생략한다. Creation·후보·설치된 PTY와 plugin 자원을
기존 소유권 회수 경로로 분리한 뒤 binding과 projection을 비운다.
미완 reap/plugin 응답은 별도 회수 컨테이너가 계속 보관·관측하며 새 기본 터미널을 막지 않는다.
원본의 미완 effect는 성공으로 만들지 않으며 저장된 복원 명령을 다시 실행하지 않는다.
임시 상태는 legacy 자료를 재수입하지 않는 새 기본 workspace다. 원본 크기가 커도 정상 복원이
가능하면 그대로 복원한다. 실제 저장 오류는 실패로 남기고 원인을 알린다.
다른 프로세스가 같은 홈을 쓰는 경우(홈 사용 중)와 임시 저장소도 실패한 경우는 부팅 오류 화면에 남는다. 엔진이 저장소에 묶인 뒤의 실행 중 halt는 이 경로를 쓰지 않고 [실행 중 halt 안내](#실행-중-halt-안내)로 알린다.

- **Status**: Implemented
- **주체**: 로컬 사용자 (설정 토글)
- **설계**: [포커스 정책 (운영 상세)](../../design/policies/focus.md) — 슬롯 점유 모델
- **코드**: `src/app/journal.rs`, `src/runtime/journal_product/view_record.rs`, legacy 입력 `src/core/layout_persistence/`
- **화면**: 없음 (앱 시작 시 자동 복원)

## 목적

`general.restore_layout`(기본 on)이 켜져 있으면 journal의 구조와 별도 View checkpoint를 함께 사용해 배치를 복원한다. 저장 원본은 domain journal·immutable surface payload·DB restore manifest다. 기존 슬롯 JSON과 scrollback 파일은 최초 이관 및 호환 읽기 자료이며 현재 저장 원본을 덮어쓰지 않는다.

최초 이관은 슬롯 stream에 기록이 아직 없을 때 `layouts/NN.json`을 한 번 읽는다. 실패해도 엔진 열기는 실패하지 않으며 원본 JSON과 scrollback 파일은 그대로 둔다.

- 파일을 레이아웃으로 해석하지 못하거나(텍스트 아님·JSON 아님) 지원하지 않는 version이면 경고 알림(파일 경로·원인)을 남기고 그 슬롯을 초기 상태로 시작한다.
- 권한 문제 등으로 파일을 읽지 못하면 오류 알림과 오른쪽 위 비포커스 안내(원인·파일 경로)를 남기고 초기 상태로 시작한다. 오류 알림 본문에도 나중에 파일을 읽을 수 있게 되어도 다시 가져오지 않는다는 사실을 적는다.
- 이관은 성공했지만 참조한 scrollback 파일이 없거나 읽히지 않은 터미널이 있으면 그 수를 적은 경고 알림을 남긴다. 그 터미널은 이전 내용 없이 복원한다.
- 초기 상태로 시작하면 그 슬롯 stream에 기록이 생기므로, 나중에 파일을 고쳐도 다시 가져오지 않는다. 알림 문구에도 이 사실을 적는다. 다시 가져오려면 그 슬롯을 쓰지 않은 새 데이터 홈에서 시작해야 한다.

## Legacy 파일 형식과 이전 저장 흐름

이 절의 슬롯 JSON writer, 슬롯 파일 삭제, persist_id별 scrollback 파일 쓰기·교체 설명은 이전 바이너리의 동작이다. 현재 제품은 아래 Journal 절의 불변 payload와 참조 보존 계약을 따르며, 이 형식은 최초 이관 입력으로 읽는다. 아래 [Surface 내용 복원](#surface-내용-복원-현재-터미널-scrollback) 절은 현재 동작을 적는다.

현재 바이너리는 legacy `scrollback/*.bin`을 쓰지 않지만 다음 세 경로에서 지운다.

- 부팅 GC: `restore_layout`이 켜져 있으면 engine 생성 전에 슬롯 JSON과 남은 옛 `layout.json`이 참조하지 않는 `.bin`을 지운다(GUI·headless 부팅 공통).
- surface 자원 회수: 닫기·교체로 터미널 자원을 정리할 때 그 터미널이 이관 자료에서 받은 legacy 파일 참조의 `.bin`을 지운다.
- 설정 끄기: 설정 화면에서 "재시작 시 서피스 내용 복원"을 on→off로 저장하면 `scrollback/*.bin` 전체를 지운다.

### 저장 대상 / 타이밍

다음 정보를 저장한다.

- 워크스페이스 이름·부제·설명, 페인 분할 방향과 비율, 탭 이름과 선택 상태
- 서피스 배치와 종류별 복원 정보: Terminal의 cwd·`restore.command`·`scrollback_ref`, Markdown·Image의 path, Explorer의 root, Html의 url
- 활성 워크스페이스와 포커스 페인 인덱스

구조가 바뀌면 dirty로 표시하고 500ms 디바운스 뒤 저장한다. 종료 시에는 디바운스를 건너뛰며, `restore_surface_content`가 켜져 있으면 구조가 바뀌지 않았어도 저장을 시도한다.

저장 함수 `save_slot`은 파일 I/O를 동기로 수행하지만 성공 여부를 호출자에게 반환하지 않는다. 호출자는 함수를 부른 뒤 dirty를 지우며 저장을 생략한 경우에도 `LayoutSaved`를 반환한다. 따라서 이 이벤트나 종료 flush 완료가 파일 저장 성공을 뜻하지는 않는다. 실패 로그와 실제 파일을 함께 확인해야 한다.

레이아웃 복원은 engine을 만들 때 한 번 수행한다. 앱의 첫 윈도우뿐 아니라 나중에 여는 윈도우도 해당한다. 파일이 없거나 읽지 못하면 기본 `Workspace 1`로 시작한다. 개별 surface를 복원하지 못하면 해당 항목은 건너뛰고, 복원한 workspace가 하나도 없으면 기본 workspace를 만든다.

복원할 레이아웃이 있는 engine에는 기본 workspace를 미리 만들지 않는다. 기본 터미널의 셸이 복원 후 소속 없이 남는 것을 피하기 위해서다. 복원이 끝난 뒤 view 상태를 구성한다.

### 슬롯 파일

슬롯 파일 하나에 engine 하나의 전체 레이아웃을 저장한다. 파일명의 숫자로 목록과 순서를 정하며 별도 인덱스 파일은 없다. 번호는 최소 2자리(`01.json`)로 표시하고 100 이상은 필요한 자릿수를 사용한다.

`NN.json.tmp`에 쓴 뒤 rename으로 슬롯 파일을 교체한다. 쓰기나 rename이 실패하면 로그를 남긴다. 이 방식이 디스크 오류나 전원 중단 뒤의 저장까지 보장하는 것은 아니다.

각 engine은 자신이 점유한 슬롯에 저장한다. 종료 flush도 윈도우별 파일에 나누어 쓴다. 한 `TASTY_HOME`의 슬롯 파일은 그 홈을 쓰는 GUI 프로세스 하나만 저장한다. 두 번째 GUI 프로세스는 아래 [데이터 홈 단일 소유](#데이터-홈-단일-소유)에 따라 슬롯 파일을 읽거나 쓰기 전에 멈춘다([슬롯 설계](../../design/policies/focus.md)).

**레거시 마이그레이션** — 단일 파일 시절의 `~/.tasty/layout.json` 은 부팅 때 `layouts/01.json` 으로 **이동**(rename)된다. `layouts/` 가 없거나, 있어도 슬롯 파일이 하나도 없다고 확인되면 이동한다. 뒤의 경우는 앞선 부팅에서 디렉터리만 만들고 rename이 실패한 상태를 다음 부팅마다 다시 시도하는 것이다. 슬롯 파일이 있거나 `layouts/` 목록을 읽지 못하면 이동하지 않고 남은 레거시 파일을 로그로 알린다. 이동하지 못한 `layout.json`은 지우지 않으며, 아래 부팅 GC가 그 파일의 scrollback 참조를 보호한다.

### 읽지 못한 슬롯

파일이 없는 경우와 읽지 못한 경우를 구분한다. 읽기 실패를 빈 슬롯으로 취급하면 이어지는 저장이 사용자의 기존 레이아웃을 덮어쓸 수 있기 때문이다.

| 상태 | 그 슬롯에 저장 | 원본 |
|------|----------------|------|
| 없음 | 한다 | — |
| 파싱 실패 | 한다 — **저장 직전에** `NN.json.bak` 으로 옮긴 뒤 쓴다 | 백업에 남는다 |
| 파싱 실패 + 백업 실패 | **안 한다** | 그 자리에 그대로 |
| 읽기 실패 (권한 · IO) | **안 한다** | 그 자리에 그대로 |
| version 이 이 빌드보다 높음 | **안 한다** | 그 자리에 그대로 |

로드는 파일을 이동하지 않는다. 손상 파일 백업은 저장 직전에 수행한다. GC와 engine 복원 등 여러 경로가 같은 슬롯을 읽으므로, 읽는 중 파일을 옮기면 뒤의 호출자는 실패 원인 대신 파일 없음만 보게 된다.

읽기 오류가 난 파일은 내용이 손상됐는지 확인할 수 없어 옮기지 않는다. 더 높은 version의 파일도 새 빌드가 읽을 수 있으므로 현재 빌드가 백업하거나 덮어쓰지 않는다.

백업 파일명은 `NN.json.bak` 이고 이미 있으면 `NN.json.bak.2` … `NN.json.bak.9` 로 늘어난다.
9개가 차면 백업을 만들지 않고 저장을 막는 쪽을 택한다. 저장이 막힌 창에는 토스트로 알린다.

### 슬롯 배정

윈도우와 연결된 engine이 슬롯 하나를 점유한다. 새 윈도우는 다른 engine이 쓰지 않는 슬롯을 선택한다.

규칙 — **실제 존재하는 슬롯 파일** 중 점유되지 않은 가장 낮은 번호를 쓴다.

- 번호 공백은 채우지 않는다: 파일이 `02.json`·`03.json` 뿐이고 점유가 없으면 답은 `1` 이 아니라 **2** 다. 저장된 레이아웃을 건너뛰고 빈 창을 띄우지 않기 위한 것이다.
- 기존 슬롯 파일이 전부 점유면 `max(파일 슬롯 ∪ 점유 슬롯) + 1` 로 **새 슬롯**을 만든다. 파일이 없으므로 그 창은 기본 워크스페이스 하나로 시작한다.
- 파일도 점유도 없으면(첫 설치) `1`.

저장된 슬롯이 여러 개여도 **부팅 시 창은 1개**다. 나머지 슬롯은 free 로 남아 있다가 창을 더 열면 순서대로 복원된다.

headless 빌드(`--no-default-features`)는 레이아웃을 영속하지 않는다 — 슬롯을 점유하지도, 저장하지도, 복원하지도 않고, 워크스페이스는 프로세스 수명 동안만 산다. `general.restore_layout` 이 켜져 있으면 부팅 때 그 설정이 무시된다는 warn 을 한 줄 남긴다([헤드리스 IPC 지원 범위](../../dev-guide/headless-ipc-surface.md)). (gui 빌드에 `--headless` 를 주면 헤드리스가 아니라 GUI 로 폴백하므로 이 절이 아니라 위 규칙을 따른다.)

### 슬롯 점유

슬롯 점유는 디스크에 기록하지 않는다. 현재 engine들이 가진 슬롯 번호로 판단하며 앱을 다시 시작하면 점유가 초기화된다. 윈도우가 사라져도 parked engine이 남아 있으면 같은 슬롯을 계속 점유한다([멀티 윈도우 구조](../../architecture/multi-window.md)). 점유 판단은 한 프로세스 안의 engine만 본다. 다른 프로세스와 같은 홈을 나누어 쓰지 않는 것은 아래 데이터 홈 단일 소유가 보장한다.

#### 데이터 홈 단일 소유

GUI 부팅은 가장 먼저 데이터 홈의 구조 저널 writer 잠금(`<홈>/structure/journal.db.writer-lock`)을 얻는다. 잠금이 잡혀 있으면 저널 worker의 writer 잠금과 같은 재시도 구간(`WRITER_LOCK_WAIT`, 2초) 동안 10ms부터 두 배씩 최대 100ms 간격으로 다시 시도한다. 비정상 종료한 이전 인스턴스의 잠금이 그 안에 풀리면 그대로 부팅한다. 재시도는 창을 만들기 전에 끝나므로 멈춘 창이 보이지 않는다. 호스트 로그 파일, `memory.db`, 설정 저장, 플러그인, 테마, 저널, 슬롯 파일, IPC, 트레이보다 앞선다.

- Given 다른 프로세스가 같은 홈의 writer 잠금을 재시도 구간 내내 쥐고 있을 때, When release GUI를 실행하면, Then 두 번째 프로세스는 창 없이 실행 중인 Tasty에 요청을 넘기고 끝난다(아래 "같은 홈의 다시 실행").
- Given debug GUI를 실행할 때, Then 증거 환경변수(`XDG_ACTIVATION_TOKEN`·`DESKTOP_STARTUP_ID`)를 시작 직후 지우고, 잠금을 얻으면 `tasty.instance`를 쓰고 종료 때 지우는 것은 release와 같다. 두 번째 실행을 넘기는 동작과 D-Bus 등록만 release 전용이다.
- Given 같은 조건에서, When debug GUI를 실행하면, Then 그 홈의 파일을 만들거나 바꾸지 않고 창과 GPU만 만들어 "이미 사용 중인 데이터 폴더" 부팅 오류 화면(`boot.home_in_use.*`)을 띄운다. 본문에 홈 경로를 넣고 같은 문구를 `tracing::error!`로 stderr에 남긴다. Quit 버튼이나 창 닫기로 종료 코드 1로 끝난다.
- Given 잠금을 얻었을 때, When 부팅이 이어지면, Then 그 잠금을 저널 worker에 넘겨 저장소가 그 잠금으로 writer가 된다. 저널 schema migration은 잠금을 쥔 뒤에만 실행된다.
- Given 잠금 시도를 판정할 수 없을 때(홈을 알 수 없거나 잠금 파일을 쓸 수 없음), When 부팅이 이어지면, Then 저널 worker가 저장소를 연 뒤 다시 잠근다. 이때 잠금이 이미 쥐어져 있거나 다른 프로세스가 저널 binding(`journal.binding-lock`)을 writer 잠금과 같은 재시도 구간 내내 초기화하고 있으면 worker는 실패 종류를 "홈 사용 중"으로 보내고, 부팅 오류 화면은 엔진 오류 대신 같은 "이미 사용 중" 문구를 고른다. 오류 문자열은 해석하지 않는다.
- Given 같은 프로세스가 fork한 자식이 exec 전까지 방금 놓은 binding 잠금의 열린 파일 설명을 공유할 때, When worker가 저장소를 다시 열면, Then 재시도 구간 안에 풀리는 잠금을 기다려 얻고 "홈 사용 중"으로 보내지 않는다. writer 잠금도 같은 이유로 같은 재시도를 쓴다.

헤드리스 빌드는 첫머리 선점을 하지 않는다. 같은 홈의 두 번째 헤드리스 프로세스는 저널 worker의 잠금에서 시작에 실패한다.

#### 같은 홈의 다시 실행

release는 같은 홈에 프로세스 하나만 둔다. 잠금을 얻은 인스턴스는 `<홈>/tasty.instance`에 PID·프로세스 시작 시각을 쓰고, IPC 서버가 열리면 포트를 넣어 다시 쓴다(임시 파일 뒤 rename). 정상 종료 때 저널 잠금을 놓기 전에 지운다. 비정상 종료로 남은 파일은 두 번째 실행과 CLI가 PID와 시작 시각으로 알아본다. CLI는 그 판별로 남은 포트 파일을 구분한다([남은 포트 파일](../remote-attach/index.md#남은-포트-파일)). Linux release는 세션 버스에 `io.github.zilhak.tasty`(정규화한 기본 홈) 또는 `io.github.zilhak.tasty.h<정규화 경로 해시>`(다른 홈) 이름으로 `org.freedesktop.Application`을 등록한다. debug는 `TASTY_DEBUG_SINGLE_INSTANCE=1`일 때만 등록한다.

- Given 잠금이 다른 프로세스에 있고 인스턴스 파일의 PID·시작 시각이 살아 있는 프로세스와 같고 포트가 있을 때, When release GUI를 사용자 실행 증거 없이 실행하면, Then `window.create`로 새 View 하나를 요청하고 종료 코드 0으로 끝난다. 새 View는 `tasty new window`와 같은 에이전트 창이며 슬롯 규칙도 같다.
- Given 같은 조건에서 실행기가 준 `XDG_ACTIVATION_TOKEN`·`DESKTOP_STARTUP_ID`(Linux)나 `AllowSetForegroundWindow` 성공(Windows)이 있을 때, When release GUI를 실행하면, Then 실행 중인 Tasty가 숨기거나 최소화한 View를 다시 보이고 마지막 포커스 View의 활성화를 OS에 요청한다. MainView가 없으면 새 창을 연다. 두 번째 프로세스는 종료 코드 0으로 끝난다.
- Given 네이티브 Wayland(`DISPLAY` 없음)에서 실행기 토큰이 있을 때, When release GUI를 실행하면, Then 실행 중인 Tasty가 그 토큰으로 마지막 포커스 View의 활성화를 컴포지터에 요청한다(`xdg_activation_v1`). 앞으로 올릴지는 컴포지터가 정한다.
- Given 네이티브 Wayland(`DISPLAY` 없음)에서 실행기 토큰 없이 `DESKTOP_STARTUP_ID`만 있을 때, When 증거가 있는 다시 실행을 받으면, Then 기존 View는 그대로 두고 토큰 없는 새 창 하나를 연다. 앞으로 올 근거가 없으므로 컴포지터가 정한다.
- Given 네이티브 Wayland(`DISPLAY` 없음)에서 실행기 토큰이 있지만 컴포지터가 `xdg_activation_v1`을 제공하지 않을 때, When 다시 실행을 받으면, Then 기존 View는 그대로 두고 토큰을 실은 새 창 하나를 연다.
- Given 네이티브 Wayland(`DISPLAY` 없음)에서 실행기 토큰이 있고 MainView가 없을 때, When 다시 실행을 받으면, Then 토큰을 새 창의 생성 속성에만 싣는다. X11처럼 등록 뒤 활성화를 다시 요청하지 않는다(토큰은 한 번 쓰면 무효가 될 수 있다).
- Given 인스턴스 파일의 PID·시작 시각이 살아 있는 프로세스와 같고 포트가 있을 때, When release GUI를 실행하면, Then writer 잠금 재시도(`WRITER_LOCK_WAIT`)를 기다리지 않고 바로 넘긴다.
- Given 인스턴스 파일이 없거나 포트가 없거나 PID·시작 시각이 맞지 않을 때, When release GUI를 실행하면, Then 잠금 재시도 뒤 20초까지 기다린다. 그 사이 잠금이 풀리면 평소처럼 부팅한다.
- Given 넘기는 중에 연결이 끊기거나 D-Bus 이름 소유자가 사라졌을 때, When 기록의 프로세스가 더 이상 살아 있지 않으면, Then 두 번째 프로세스는 기한을 기다리지 않고 기록과 writer 잠금을 다시 확인하고, 잠금을 얻으면 평소처럼 부팅한다(실행 요청이 사라지지 않는다).
- Given `window.create`나 D-Bus `Activate`를 보냈지만 기한 안에 응답이 없을 때, When 실행 중인 인스턴스가 살아 있으면, Then 메시지 상자 없이 `launch.log`에 경고 한 줄만 남기고 종료 코드 0으로 끝난다. 실행 중인 쪽이 늦게라도 처리한다. 기능 확인 요청에 답하지 않아 `window.create`를 보내지 못한 경우는 "보냈다"로 보지 않는다.
- Given 요청을 넘기지 못했고(기다림 초과, 세션 버스·D-Bus 이름 없음, IPC 연결 실패) 다시 확인해도 잠금을 얻지 못했을 때, When 두 번째 프로세스가 끝나면, Then Tasty 창 없이 OS 메시지 상자(`app.name`, `boot.already_running.body`)를 띄우고 종료 코드 1로 끝난다. Linux에서 `zenity`가 없으면 데스크톱 알림으로 대신한다.
- Given 두 번째 실행이 있을 때, Then 그 과정이 `<홈>/launch.log`(256 KiB 상한, `launch.log.lock`으로 직렬화)와 stderr에 한 줄씩 남는다. 증거는 종류만 남기고 값은 남기지 않는다. 실행 중인 인스턴스의 `debug.log`는 열지 않는다.

증거 판정과 포커스 규칙은 [포커스 정책](../../design/policies/focus.md#같은-홈으로-다시-실행했을-때-release-단일-실행), 근거와 한계는 [포커스 정책 (운영 상세)](../../design/policies/focus.md)에 있다.

#### 점유 조회

`window.list` IPC 또는 `tasty list windows`의 `layout_slot` 필드로 윈도우가 쓰는 슬롯을 조회한다.

```json
[
  { "id": 1, "focused": true,  "title": "Tasty",       "layout_slot": 1 },
  { "id": 2, "focused": false, "title": "Workspace 1", "layout_slot": 2 }
]
```

`layout_slot` 은 슬롯을 잡지 않는 engine 에서 `null` 이다(headless). 순수 read 라 포커스·선택 등 사용자 상태를 건드리지 않는다.

Parked engine은 슬롯을 점유하지만 윈도우 ID가 없어 `window.list`에 포함되지 않는다. 따라서 이 응답만으로 전체 슬롯 점유 집합을 알 수는 없다.

### 창 닫힘 시 슬롯 처리

윈도우를 닫으면서 engine도 제거하면 슬롯 점유가 해제된다. 제거 직전에는 `general.restore_layout`에 따라 파일을 처리한다.

| `restore_layout` | 슬롯 파일 |
|---|---|
| **on** | 500ms 디바운스를 건너뛰고 저장을 시도한 뒤 슬롯을 남긴다. 저장에 성공한 내용은 다음 윈도우에서 복원할 수 있다. |
| **off** | 슬롯 파일 삭제를 시도한다. |

마지막 창을 닫아 engine 이 파킹되는 경우(그리고 macOS 최소화)는 engine 이 살아 있으므로 이 처리를 하지 않는다 — 슬롯 점유가 유지되고 다시 창을 열면 같은 슬롯을 이어쓴다.

윈도우 정리에서 legacy scrollback `.bin`을 따로 지우지는 않는다. 남아 있는 슬롯의 참조와 다음 부팅의 GC 결과에 따라 정리된다.

### Surface 내용 복원 (현재: 터미널 scrollback)

`general.restore_surface_content`(기본 on)가 켜져 있으면 터미널 capture가 scrollback과 화면 라인을 journal의 불변 surface payload에 담는다. payload는 cwd·`restore.command`와 함께 직렬화한 scrollback 바이트(`TSSB` 형식)를 가지며, 저장할 때마다 새 payload를 만들고 기존 payload를 고쳐 쓰지 않는다.
`TSSB`는 버전 4이고 버전 3 payload도 그대로 읽는다. 버전 3에는 밑줄 종류(단일 외), 밑줄 색, 반전, 깜빡임, 숨김, 윗줄이 없어 그 줄은 기본값으로 복원된다. 저장 복원과 디스크 스크롤백 모두 위 값과 굵게·dim·기울임·취소선·색을 지키고, vertical_align과 하이퍼링크는 저장하지 않는다. 이 값들이 없는 런은 버전 3과 크기가 같고, 있는 런은 2~5바이트가 는다. 공백 칸뿐인 마지막 행은 저장에서 잘리지만 밑줄·취소선·윗줄·반전이 있는 칸은 공백으로 치지 않는다(배경색만 있는 칸은 공백으로 친다). 행 축소도 같은 판정을 쓴다. 버전 4를 모르는 옛 앱은 경고만 남기고 해당 스크롤백을 복원하지 않으며 저장된 payload를 고치지 않는다. 다만 옛 앱을 실행한 뒤 새 앱으로 돌아오면 그 스크롤백은 복원되지 않는다.

전체 capture의 복사·직렬화·worker 쓰기 비용과 격리 측정 방법은 [capture 비용](../../architecture/surface-capture-cost.md)에 있다.
전체 capture를 저장하는 코덱은 유지한다. 부분 scrollback delta로 바꾸면 복원·undo·pin 계약을
함께 바꿔야 하므로 현재 압축과는 별도 최적화 영역이다. 정상 압축은 최근 유효 snapshot 두 개와
그 사이 tail, 현재 holder의 참조만 남기며 단일 capture의 64 MiB 요청 한도는 그대로 적용한다. 복원 시 이전 scrollback과 화면 내용을 새 터미널의 출력 앞에 넣는다. legacy `~/.tasty/scrollback/<persist_id>.bin`은 최초 이관 때 슬롯 JSON의 `scrollback_ref`로 읽어 payload에 복사하는 입력이며, 현재 capture가 쓰는 곳이 아니다.

옵션이 꺼져 있으면 새 capture에 scrollback을 담지 않는다. 복원할 때도 payload에 남아 있는 scrollback을 터미널에 넣지 않으며, 배치·cwd·`restore.command`는 그대로 복원한다. 닫은 항목 복원도 같은 기준을 따른다.

설정을 on→off로 바꾸면(설정 화면이든 다른 설정 적용 경로든) 창과 parked engine 모두를 dirty로 표시한다. 그래서 구조가 바뀌지 않아도 다음 저장 주기와 종료 저장에서 새 capture가 생긴다. 이 capture는 live 터미널을 scrollback 없이 다시 기록한다. 아직 활성화하지 않은 지연 탭처럼 capture가 관측하지 못한 터미널은 기존 payload에서 scrollback과 legacy 파일 참조만 뺀 새 payload를 현재 activation에 기록한다. 이후 현재 surface가 가리키는 payload에는 이전 scrollback이 없다.

이전 payload는 이때 지우지 않는다. 소비하지 않은 닫은 항목 기록이나 보존 중인 snapshot 같은 다른 holder가 잡고 있으면 남고, 아래 [Journal snapshot 보존](#journal-snapshot-보존)에 따라 모든 pin이 사라지면 지워진다. 남아 있는 동안에도 설정이 꺼져 있으면 그 scrollback을 복원에 쓰지 않는다. 설정을 다시 켜면 이후 capture부터 scrollback을 저장하고, 아직 남은 닫은 항목 기록을 복원하면 그 기록의 scrollback이 다시 쓰인다. 설정 화면에서 on→off로 저장하면 legacy `scrollback/*.bin`도 지운다.

legacy `.bin`의 정리는 위 [세 경로](#legacy-파일-형식과-이전-저장-흐름)를 따른다. 그중 부팅 GC는 슬롯 파일들과 아직 남아 있는 옛 `layout.json`의 `scrollback_ref` 합집합을 구해 참조되지 않는 `.bin`을 지운다. 참조 집합을 확정하지 못하면 그 회차 GC를 건너뛰고 아무 파일도 지우지 않는다.

- `layouts/` 디렉터리가 있지만 목록을 읽지 못했거나 개별 항목을 읽지 못해 목록이 불완전할 수 있을 때
- 열거한 슬롯 하나라도 로드하지 못했을 때(읽기 실패·해석 실패·더 높은 version·열거 뒤 사라짐)
- 남아 있는 옛 `layout.json`을 읽거나 해석하지 못했을 때

`layouts/` 디렉터리와 옛 `layout.json`이 모두 없으면 참조가 없다고 확정되므로 모든 `.bin`을 지운다. GC를 건너뛴 사실은 warn 로그로 남긴다. 사용자 알림은 띄우지 않는다. 건너뛰어도 잃는 자료가 없고 복원과 이후 터미널 작업에 영향이 없으며, 남는 것은 지우지 못한 파일뿐이기 때문이다.

### Plugin surface 복원 (hello 창)

Markdown·Image처럼 플러그인이 제공하는 surface는 hello와 kind 등록이 끝나기 전에 레이아웃 복원이 시작될 수 있다. 이때 kind와 snapshot을 가진 placeholder로 위치를 남겨 둔다. kind가 등록되면 화면 갱신 경로에서 실제 surface로 바꾼다.

plugin이 시작하지 못하거나 해당 kind를 더 이상 제공하지 않으면 placeholder를 유지한다.
화면에는 빈 자리로 보이고, `tasty list tree`에는 원래 kind와 `ready: false`,
`pending_reason: "plugin_not_loaded"`가 표시된다. surface가 존재하는 것과 사용할 준비가
된 것을 구분하며, 화면과 API를 수정할 때 이 의미가 일치하도록 유지한다.

### TUI 세션 복원 (`restore.command`)

플러그인이 세션 시작 훅에서 surface meta의 `restore.command`를 기록하면 호스트는
그 문자열을 터미널 복원에 사용한다. 에이전트 종류나 명령 내용은 호스트가 해석하지 않는다.
Claude 프로필이 붙은 경우 `claude -r <id> --settings "<프로필 경로>"`처럼 기록한다.
프로필 정보 복구는 [Claude 가이드](../../plugins/claude/index.md)의 해당 절을 따른다.

호스트는 명령 뒤에 `\r`을 붙여 `TerminalConfig.initial_input`으로 전달한다.
자식 셸을 생성한 뒤 writer 스레드를 시작하기 전에 PTY에 동기로 쓰고 flush한다.
쓰기·flush 실패는 경고를 남기며, 이미 실행 중인 셸의 입력 초기화로 바이트가 사라질 수도 있다.
따라서 첫 stdin 읽기에 반드시 도착하거나 spawn과 동시에 명령이 실행된다는 보장은 없다.

앱 재시작과 [닫힌 항목 복원](../closed-tab-restore/index.md)(Ctrl+Shift+T)에서 사용한다.

## 현재 Journal 저장

### Journal의 지연 surface 캡처

Preset 캡처 초안은 구조·선택·실행 중 kind snapshot을 동결하고, 지연 placeholder는
기존 불변 payload 참조를 worker에 넘긴다. worker는 최신 캡처를 우선 읽으며 없으면
생성 seed의 원래 kind·params·CWD를 해석한다. 터미널의 과거 시작 입력·복원 명령·
scrollback은 preset에 복사하지 않는다. 등록되지 않은 지연 plugin의 opaque snapshot도
빈 surface로 치환하지 않는다. 이 읽기는 PTY나 plugin을 활성화하지 않는다.
동기 캡처 경로는 아직 해석하지 않은 참조를 저장하지 않고 오류로 반환한다.
App의 preset capture continuation이 worker 결과와 원 reply를 연결한다. 호환 layout export leaf는 canonical 모델과
별도 View checkpoint를 합성하고, 기존 JSON version/key와 pane·tab 선택 index를 보존한다.
terminal 캡처의 scrollback은 새 legacy ID의 소유 bytes로 반환하며 기존 파일을 덮어쓰지 않는다.
이 값 변환은 파일 게시나 새 export IPC/CLI를 수행하지 않는다.

### 실행 중 surface 캡처의 크기 상한

구조가 dirty가 되면 디바운스 뒤 engine 하나의 모든 surface를 한 요청으로 캡처한다.
`restore_surface_content`가 켜져 있으면 터미널마다 디스크로 내보낸 줄을 포함한 전체
scrollback과 화면 줄을 복사해 직렬화한다. 이 요청은 worker queue의 64 MiB 상한
(`tasty_ipc::admission::QUEUED_BYTES_LIMIT`)을 따른다.

- 복사하기 전에 engine의 터미널 scrollback 직렬화 크기 합을 구한다. 디스크 줄은
  scrollback 파일에서 디스크 보관 상한으로 버리고 남은 레코드의 바이트로, 메모리 줄은 각 줄의 구성 요소 길이로 계산하며 디스크를 읽지
  않는다. 각 surface의 저장 bytes가 이 scrollback 직렬화를 그대로 포함하므로 이 합은
  요청 크기의 하한이다. 하한이 상한을 넘으면 scrollback을 복사하지 않는다.
- 하한이 상한 이하여도 실제로 만든 요청이 혼자 상한을 넘으면 결과를 버린다.
- 위 두 경우 그 engine의 capture 전체를 저장하지 않는다. 상한을 넘지 않는 다른
  surface의 구조·내용도 이번 저장에서 빠진다. 이전 불변 자료를 유지하고 dirty는 남긴다.
  warn 로그는 거절할 때 한 번 남긴다. 다음 변경 전까지 다시 시도하지 않으므로 처음 거절할 때
  경고 알림도 남긴다([저장 실패 알림](#저장-실패-알림)).
- 거절한 뒤에는 dirty 세대가 같고 scrollback 크기 합이 거절 시점 이상이면 다시 캡처하지
  않는다. 구조가 다시 바뀌어 dirty 세대가 새로 생기거나, `CSI 3J`(ED3) 등으로 scrollback이
  줄어들면 다음 저장 주기에 다시 시도한다.
- 진행 중인 다른 capture가 잡고 있는 bytes 때문에 상한을 넘은 경우는 그 capture가 끝나면
  풀리므로 거절을 기록하지 않고 다음 저장 주기에 다시 시도한다.
- 사용자 닫기 경로의 선택 캡처는 engine 단위로 거절하지 않는다. terminal마다 같은 하한으로 판정해 예산 안의 terminal만 내용을 담고 나머지는 내용 없이 기록한 뒤 닫는다([닫힌 항목 복원](../closed-tab-restore/index.md#내부-동작)).

### 저장 실패 알림

주기 저장은 사용자 입력에서 시작하지 않으므로, 저장이 멈췄거나 계속 실패하면
[피드백 원칙](../../principles/feedback-and-recovery.md)에 따라 경고 알림으로 알린다. 알림은
복원 알림과 같은 경로로 그 엔진의 창에 들어가고 같은 내용을 warn 로그로 남긴다. 상한 값,
dirty 재시도, 저장 방식은 위 절과 같다.

- 상한 초과 거절: 처음 거절할 때 바로 경고 하나를 남긴다. 본문에는 그 엔진의 workspace
  이름 전체(capture는 엔진의 workspace를 한 번에 저장한다), 요청 크기, 상한(MiB)을 적고,
  변경 뒤 다시 시도하며 상한 이하일 때 저장된다고 적는다.
- 반복 실패: 같은 엔진의 capture 실패(App에서 surface를 복사하지 못함, worker가 실패를
  돌려줌)나 같은 View stream의 checkpoint 쓰기 실패가 성공 없이 3번 이어지면 경고 하나를
  남긴다. 실패한 저장은 다음 저장 주기에 다시 시도하므로 한두 번의 일시 실패는 그 사이에
  성공해 알리지 않는다. 본문에는 workspace 이름, 연속 실패 횟수, 마지막 원인을 적는다.
- 같은 원인으로는 다시 알리지 않는다. 엔진의 capture가 성공하면 그 엔진의 상한 거절·반복 실패
  기록을 지우고, View 쓰기가 성공하면 그 stream의 기록을 지운다. 그 뒤의 실패는 처음부터 다시
  센다.
- 진행 중인 다른 capture 때문에 queue bytes가 모자란 거절은 그 capture가 끝나면 풀리므로 세지
  않는다. 손상 범위 때문에 미룬 View 저장(`ViewDeferred`)도 실패로 세지 않는다.
- 같은 엔진의 저장 경고가 다른 복원·저장 알림과 한 번에 전달되면 기존 규칙대로 한 항목으로
  묶는다. 묶은 제목은 복원과 저장을 함께 가리키는 문구이고 각 제목은 본문 줄에 남는다.
- 판정은 `src/app/journal/save_failures.rs`에 있다.

디스크 scrollback의 연속 구간은 파일을 한 번 열고 순차로 읽는다. 전체 scrollback 복사,
scrollback 검색, 화면 텍스트의 history 채움, 스크롤한 화면의 history 행 렌더링, 선택
텍스트 추출(일반·줄·블록 모드), vi 복사 모드의 단어 이동이 이 읽기를 쓴다. 줄 수와
관계없이 한 번의 읽기에 파일을 여는 횟수가 일정하다. 디스크 보관 상한으로 앞쪽 줄을 버린 뒤에도
남은 첫 레코드의 오프셋에서 같은 순차 읽기를 시작한다
([디스크 보관 상한](../../surfaces/terminal/index.md#스크롤백)). 링크 조회의 행 읽기는
[터미널 링크](../terminal-link/index.md#소프트-wrap멀티라인-링크)에 있다.

### Journal snapshot 보존

Journal의 domain snapshot은 구조 이벤트의 파생 cache이며 terminal·plugin 콘텐츠와
View 선택 checkpoint의 원본을 대신하지 않는다. 저장 worker의 maintenance는 확정된
모든 엔진 모델의 payload 참조, snapshot, maintenance cursor를 한 transaction으로
교체한다. 실패하면 이전 참조와 cursor를 유지한다. 참조 집합에는 미완·Uncertain
operation, assembly, 소비하지 않은 undo가 포함된다.

최근 checksum 검증을 통과한 snapshot 두 개를 남기며, snapshot 본문에 외부 holder가
있으면 해당 snapshot도 보존한다. 본문뿐 아니라 그 snapshot이 pin한 콘텐츠도 검증한다.
Admission·import·View holder는 이 정리에서 해제하지 않으며, event holder는 보존 경계
이전의 이벤트 본문을 지우는 transaction에서만 해제한다. 모든 pin이 사라진 payload만 지운다. 원 요청 키, 최초 최종 응답 및 effect 시도 기록은 삭제하지 않는다. 이벤트 본문은 두 번째 유효 snapshot cut까지의 연속 prefix만 정리한다.
Batch 식별과 stream revision 헤더는 유지한다. 오래된 cursor 읽기는 빈 성공 대신
`ResyncRequired`를 반환하며, projection은 snapshot의 전체 출력과 cursor를 함께 교체해
재동기화한다. 적재가 손상을 만나면 손상 범위만 빼고 연다. 자세한 동작은 아래 "손상된 journal 기록의 적재"에 있다.
App의 읽기 lease에 잡힌 payload는 Busy 대기와 늦은 완료 동안 snapshot pin에 포함한다.
공통 worker는 capture·retirement의 전체 publication ACK 뒤와 정상 종료에 checkpoint를 실행한다. halted projection이나 공개되지 않은 cut은 checkpoint하지 않는다.

### 손상된 journal 기록의 적재

구조 journal은 모든 엔진 stream을 한 번에 적재한다. checksum은 맞지만 해석하거나 적용할 수 없는 기록을 만나면 그 범위만 빼고 나머지를 연다. 원본 행은 지우거나 고치지 않는다.

- 한 stream만 담은 batch가 손상된 경우: 그 stream을 손상으로 표시하고 모델에서 뺀다. 다른 stream은 그대로 적재한다.
- 여러 stream을 함께 바꾸는 batch가 손상된 경우: batch는 원자 단위이므로 일부 stream에만 적용하지 않는다. 그 batch가 담은 모든 구조 stream을 손상으로 표시한다. 손상 batch 직전 상태로 열면 이후 새 기록이 읽지 못한 원본 기록 위에 이어지므로 그렇게 열지 않는다.
- 손상된 stream을 다시 바꾸는 뒤 batch도 적용하지 않고, 그 batch의 다른 stream도 손상으로 표시한다. 건너뛴 batch도 batch 위치는 지나가므로 남은 stream의 적용 위치는 정상으로 이어진다.
- 최신 snapshot을 해석할 수 없거나 snapshot의 cut이 모델과 다르면 그 snapshot을 건너뛰고 더 이전 snapshot이나 로그 처음부터 다시 만든다. 그 뒤 기록을 모두 다시 적용하므로 잃는 변경은 없다. 보존 경계 위에 기반이 남지 않으면 저장소 전체를 읽을 수 없는 경우로 시작에 실패한다.
- 손상된 stream마다 경고 알림 하나(레이아웃 슬롯 번호와 원인), snapshot을 건너뛰었으면 경고 하나를 남기고 같은 내용을 warn 로그로 남긴다.
- 손상된 슬롯 번호는 이번 실행에서 고르지 않는다. 그 슬롯을 복원하려던 윈도우는 어떤 기록에도 쓰이지 않은 새 슬롯 번호로 새 레이아웃을 시작한다. 새 윈도우의 슬롯 배정도 손상된 번호를 점유로 센다. worker도 손상된 슬롯의 엔진 열기를 거절한다.
- 손상 범위가 있는 동안 checkpoint(snapshot 저장·로그 압축·payload GC·manifest 정리)를 하지 않는다. 새 snapshot은 빠진 stream을 담지 못하고 압축은 읽지 못한 원본을 지울 수 있기 때문이다. 다음 실행도 같은 손상을 다시 발견하므로 원본은 계속 남는다.
- 해석하지 못해 건너뛴 snapshot이 있을 때도 손상 범위로 본다. 이 동안 View 선택 저장도 하지 않는다. View 저장은 지금 적재한 모델로 domain snapshot을 함께 쓰고 cache snapshot을 정리하므로, 손상 stream이 빠진 snapshot을 남기거나 건너뛴 snapshot 행을 지울 수 있기 때문이다. 이 실행에서 바꾼 선택은 다음 실행에 복원되지 않으며 이전에 저장한 선택은 그대로 남는다.
- headless는 과거 stream을 복원하지 않으므로 손상과 관계없이 새 stream으로 시작한다.

View checkpoint는 journal의 restore manifest가 별도 원본으로 보관한다. domain snapshot,
View 바이트, slot binding 및 payload pin은 한 transaction으로 저장한다. incarnation·runtime
세대·sequence가 역행하는 저장과 같은 sequence로 내용을 바꾸는 저장은 거절한다.
DB manifest가 없을 때만 이전 sidecar를 초기 자료로 읽으며, 첫 저장 뒤에는 DB가 원본이다.
일치하는 DB manifest가 손상돼도 sidecar로 돌아가 선택을 바꾸지 않는다. View 원본(DB manifest나 sidecar)을
해석하거나 검증하지 못하면 엔진은 구조를 그대로 복원하고 선택만 기본값(첫 workspace와 탭)으로 둔다. 경고 알림에
슬롯 번호·보존 위치·원인을 적고 같은 내용을 warn 로그로 남긴다. DB manifest는 다음 View 저장이 같은 key를
바꾸므로 그 전에 View 바이트와 domain snapshot 바이트를 데이터 홈의 `structure/damaged/`에
`view-<stream>-incarnation-<N>-<해시>.view`·`.snapshot`으로 복사한다. 해석은 되지만 검증에 실패한 sidecar도
`.legacy.json`으로 복사한다. 해시는 함께 복사하는 본문 전체(`.view`와 `.snapshot` 둘 다)로 정하며, 같은 이름에 내용이 다른
사본이나 읽어서 대조할 수 없는 파일이 이미 있으면 `-2`, `-3`처럼 다음 번호를 붙인다. 읽어서 같은 바이트임을 확인한 사본만 다시 쓰지 않으며, `structure/damaged/` 폴더 자체를 나열할 수 없으면 이름을 고르지 않고 보존 실패로 처리한다. 이미 있는 파일은 바꾸지 않고 자동으로 지우지 않는다.
읽기 권한이 없는 sidecar는 복사할 수 없으므로 원래 자리를 보존 위치로 알린다. 저장은 sidecar를 바꾸지 않는다.
열 때 거부한 sidecar(읽지 못함·해석 실패·지원하지 않는 version·검증 실패)는 이후 View 저장의 순서를 정하지 않는다.
받아들인 sidecar가 더 새로운 선택을 담고 있으면 그보다 오래된 저장은 여전히 거절한다. manifest의 View·snapshot 본문 checksum이 맞지 않거나, 본문 행이나 manifest가 가리키는 snapshot 행이 없으면
남아 있는 원래 바이트를 검증하지 않고 같은 위치로 복사하고 기본 선택으로 계속한다. 행이 없는 본문은 복사하지 않으며,
복사할 본문이 하나도 없으면 데이터 홈의 `structure/journal.db`를 보존 위치로 알린다. 그 밖의 저장소 오류는 엔진 열기
실패로 남는다. 새 incarnation은
옛 incarnation의 손상된 View 바이트를 읽지 않는다. retirement는 해당 incarnation의
manifest만 해제하고 미완·Uncertain domain 의무의 참조는 계속 유지한다.
구조 변경 없이 View만 반복 저장해도 같은 transaction에서 이전 cache snapshot을 정리하며,
현재 manifest·외부 holder·anchor가 잡은 snapshot은 남긴다.
휴면 슬롯의 전체 stream 상태와 revision이 checkpoint에서도 같으면 manifest의 View 바이트·
incarnation·runtime epoch·sequence를 유지한 채 최신 domain snapshot으로 연결한다.
미완 operation과 undo를 포함해 상태가 다르거나 View 검증이 실패하면 이전 연결을 유지한다.
고정 전송 alias는 옮기지 않는다. 따라서 열리지 않은 두 번째 슬롯도 선택과 내용을 유지하면서
활성 슬롯의 capture 이력이 정리된다. 재연결하지 않은 manifest의 snapshot은 보존 경계의 하한이며, 선택 자료만 domain event에서
재생하거나 최신 선택으로 다시 생성하지 않는다.

새 journal import의 payload 복사는 source import holder를 먼저 영속화하고 destination에
독립 BLOB과 destination holder를 한 transaction으로 만든다. 복사 API는 source holder를
해제하지 않는다. destination 초기 이벤트와 manifest 전환이 끝난 호출자가 해제하며,
실패 시 source를 계속 읽을 수 있다. 내부 journal import는 명시 source restore manifest를 전송 ID별 고정 alias로 pin한다.
대상은 현재 데이터 홈의 명시된 새 슬롯이며, 새 ID mapping·copied payload 참조·초기 batch·
최초 응답·domain snapshot·View manifest를 한 transaction으로 확정한다. 동일 전송 ID의
재시도는 원 mapping을 반환한다. source가 이후 사라져도 완료한 destination을 다시 만들지 않는다.
source pin 해제 실패는 보수적으로 참조를 남기고 다음 같은 전송 재시도에서 정리한다.
정상 시작의 단일 journal 정책과 공개 IPC/CLI는 바꾸지 않는다.

## 오른쪽 위 비포커스 안내

복원 자료를 읽지 못한 경우처럼 오류 알림을 남기는 사건은 같은 원인·원본 경로를 창 오른쪽 위 안내 열에도 띄운다. 일부 손실(경고)은 알림 항목만 남긴다.

- 위치: 작업 영역 오른쪽 위, 탭 바 아래에서 `notice-column-inset`만큼 들어간 열. 창 범위(View·Workspace) 배너나 열과 가로로 겹치는 열린 검색 막대가 열 시작점 위에서 시작하면 열은 그 아래 + inset에서 시작하며, 배너·검색 막대는 옮기지 않는다. pane·tab·surface 범위 배너는 열을 옮기지 않는다. 폭은 `notice-column-width`이며 작업 영역 − 2 × inset을 넘지 않는다. 여러 안내는 `notice-column-gap` 간격으로 쌓고, 제목·원인·경로가 같은 안내는 다시 쌓지 않는다.
- 입력: 나타날 때 포커스를 가져가지 않고 scrim도 없다. 바깥의 키(Enter·Esc 포함)·선택·스크롤은 터미널로 그대로 간다. 안내 상자만 포인터를 받는다. 안내 상자 위의 누름은 그 아래 surface·pane을 활성화하거나 선택·분할선 드래그를 시작하지 않으며, 판정은 직전 프레임에 그린 카드 사각형과 현재 포인터 좌표로 한다. 안내 안쪽을 누르면 누른 컨트롤(컨트롤이 아닌 자리면 Continue)이 키보드 포커스를 받고, 그동안에만 키(Esc·Tab·Enter 등)가 안내로 간다. 안내 바깥을 누르면 안쪽 포커스를 내려놓고 키는 다시 터미널로 간다.
- 응답: ×, Continue, 안내 안쪽 컨트롤(×·경로·복사·Continue)에 포커스가 있을 때의 Esc는 같은 응답이다. 안내를 지우고 그대로 계속한다. 원본은 바꾸지 않으며 오류 항목은 알림에 남는다. 경로 복사 버튼은 원본 경로를 클립보드에 넣는다.
- 안내를 받을 엔진이 아직 창에 붙기 전이면 창이 생길 때까지 알림과 안내를 미룬다.
- native WebView(HTML·Markdown surface)가 안내와 겹치면 WebView의 bounds·표시·키보드 포커스는 그대로 두고, 안내 카드와 겹친 부분만 그 native 창의 그림과 포인터 입력에서 뺀다(`state::webview_edges::notice_clip_holes`, `PlatformWebView::set_native_holes`). 좁은 pane 전체가 안내 아래에 있어도 카드가 덮는 부분만 가려지고 안내가 보이며 닫을 수 있다. 카드 밖의 페이지는 계속 보이고 입력·스크롤을 받는다. 카드 사이 간격은 빼지 않는다. 카드 경계는 바깥쪽 정수 물리 px로 넓힌다. 안내를 닫으면 다음 프레임에 원래 사각형으로 돌아가고 페이지 폭은 바뀌지 않는다. Linux(X11)는 XFixes `ShapeBounding`·`ShapeInput`으로, Windows는 child HWND의 `CombineRgn(RGN_DIFF)`+`SetWindowRgn`으로, macOS는 `KeyWebView` layer의 `CAShapeLayer` mask(구멍을 뺀 겹치지 않는 사각형들)와 `hitTest:` 제외로 뺀다. Linux와 Windows는 격리 debug 인스턴스에서 측정했다. Windows 11 기기에서 markdown 페이지 위 안내 카드 하나만 띄웠을 때 WebView child HWND의 창 region은 카드 상자만 뺀 사각형이었고, 그 자리에 주 표면이 그린 카드가 보였다. 카드 안 점의 `WindowFromPoint`는 winit 창, 카드 밖 페이지 점은 WebView2 렌더 창이었다. 안내를 닫자 region이 지워지고 같은 점이 WebView2로 돌아왔다. 실제 마우스 장치 입력은 쓰지 않았다(OS hit test와 debug 입력 주입). macOS는 native 컴파일·검사를 통과했고 mask 계산을 입력 없는 소유 probe로 확인했다. Tasty 안내 카드 단독의 실제 동작과 입력은 측정하지 않았다. 적용에 실패하면 warn을 남기며 그 경우 페이지가 안내를 덮을 수 있다.

### 실행 중 halt 안내

엔진이 저장소에 묶인 뒤 publication이 halt하면(확정한 batch를 화면 상태에 공개하지 못함, 저장소 쓰기 실패 등) App은 halt 사유를 담은 오류 알림 하나를 남기고 같은 안내 열에 비포커스 안내를 띄운다. halt 조건, halt 중 IPC 요청 거절, 입력을 처리하지 않는 동작은 바꾸지 않는다.

- 내용: 제목은 재시작이 필요하다는 문구, 원인은 저장 실패로 변경 적용을 멈췄다는 문구, 경로는 이 실행의 `structure/journal.db`, 본문은 이 안내와 창 닫기 버튼 말고는 입력과 클릭에 반응하지 않으며 Tasty를 종료하고 다시 시작해야 한다는 문구이고, 그 아래에 halt 사유를 그대로 붙인다. 알림 본문과 오류 로그에도 같은 사유가 남는다.
- 한 번만: halt 하나에 알림과 안내를 한 번만 남긴다. halt 중 요청이 더 거절돼도 다시 알리지 않는다. 종료 대기 시간 초과로 생긴 halt는 사용자가 이미 종료를 골랐으므로 알리지 않는다.
- 전달: 알림은 dispatch를 거치지 않고 알림 저장소에 바로 넣으므로 halt가 dispatch를 멈춰도 사라지지 않는다. halt 중에는 알림 영역을 열 수 없고 `tasty list notifications` 같은 IPC 조회도 거절되므로, 사유는 안내 본문에서 읽는다.
- 그리기: halt 중 창은 엔진을 읽기만 하는 프레임으로 다시 그린다. 이 프레임이 만든 요청은 적용하지 않고 버린다. 프레임마다 활성 native WebView의 bounds는 유지하고 안내 카드와 겹치는 그림·입력 영역만 다시 맞추며, WebView 생성·주소 갱신·이동·설정 적용은 하지 않는다. Linux는 native 페이지가 보이면 halt 중에도 일반 webview 폴링과 같은 16ms 간격으로 GTK 대기열을 비워, 안내를 닫은 자리를 페이지가 다시 그리게 한다. 그 전에 모든 페이지의 키·포인터·터치 이벤트를 GTK 신호 단계에서 버리므로 페이지와 host 어느 쪽에도 입력이 가지 않는다. 키·포커스 전달, 탐색 요청, 설정 적용은 이 처리에서 하지 않는다. 페이지가 스스로 시작한 탐색은 halt 중에도 WebKit 기본 결정으로 진행될 수 있다. Linux 격리 debug 인스턴스에서 측정했고, Windows·macOS에서는 이 처리를 하지 않는다.
- 입력: 포인터는 안내 상자와 타이틀바 닫기 버튼 위에 있을 때만 받는다. 키 입력은 받지 않으므로 포커스는 halt 전 위치에 남는다. 안내의 ×·Continue는 안내만 지운다.
- 창 닫기: 타이틀바 닫기 버튼과 OS 닫기 요청은 창을 닫는다. 종료 확인 창은 halt 중 입력을 받지 못하므로, 마지막 창이면 확인 없이 종료한다. 창이 여럿이면 그 창만 닫는다.

## 저장하지 않는 것

실행 중인 PTY 프로세스와 환경변수, 팝업 상태는 저장하지 않는다. `restore.command`는 기존 프로세스를 보존하는 기능이 아니라 새 터미널에서 명령을 다시 실행하기 위한 정보다.

## 관련

- [closed-tab-restore](../closed-tab-restore/index.md) · [layout-presets](../layout-presets/index.md) · [terminal](../../surfaces/terminal/index.md)(scrollback)
- [포커스 정책 (운영 상세)](../../design/policies/focus.md) — 슬롯 모델을 고른 이유·대안·재검토 조건
- [멀티 윈도우 아키텍처](../../architecture/multi-window.md) — GUI 저장 슬롯의 조건부 대응과 parked·retiring engine, headless 수명


### 부분 projection 위치와 저장 압력

선택한 stream만 읽는 영속 projection은 consumer/version에 고정한 scope와 실제 batch cut을
저장한다. 출력과 위치는 함께 확정하며, 보존 경계보다 이전 위치는 전체 snapshot으로
재동기화한다. global 위치와 부분 위치를 같은 consumer/version에서 혼용하지 않는다.

신규 구조 명령은 누적 DB·WAL 크기로 거절하지 않는다. 미확정 준비의 credit만 제한하며
앞선 명령의 credit이 풀릴 때까지 도착 순서대로 기다린다. 준비 한도와 실제 OS 저장 오류는
서로 다르다. 이미 확정된 명령의 같은 key 조회와 효과 완료·정리·복구·checkpoint는 새 준비
credit을 요구하지 않는다. payload GC는 checkpoint에서 reader lease를 잠그고 최신 참조를
durable pin으로 옮긴 뒤 수행한다. admission이 그 동기화 없이 독립 GC를 실행하지 않는다.
정확한 준비 한도와 진단용 파일 사용량 API는 [저장소 계약](../../architecture/event-store.md)에 있다.

### 복원 입력과 View 저장의 수명

App의 `Restorations`가 payload 읽기 대기·접수된 ticket·사용 가능한 입력과 bootstrap 단계를
관리한다. 엔진 종료는 새 입력 접수를 막고 대기 입력을 버리지만, 이미 접수한 읽기 ticket은
완료 응답을 소비할 때까지 유지한다. 늦은 성공·실패가 종료한 엔진을 되살리지 않는다.
비활성 surface의 입력은 bootstrap 완료 후에도 남을 수 있으며, 실제 activation 뒤 제거한다.

`ViewWrites`는 stream별 최신 대기 snapshot과 이미 접수한 쓰기를 구분한다. 두 값은 함께
존재할 수 있다. 이전 쓰기의 실패는 더 최신 snapshot의 재시도·성공을 덮지 않으며,
엔진 종료 뒤 도착한 완료는 소비만 하고 재사용한 stream에 실패 상태를 붙이지 않는다.

### surface 하나의 복원 자료를 읽지 못한 경우

복원할 surface의 capture나 생성 seed를 읽지 못하면 그 surface만 잃은 것으로 처리한다. 같은 엔진의 다른 surface와 다른 엔진의 복원은 계속하며 journal은 halt하지 않는다.

- 손상으로 보는 경우: payload 행이 없음(`PayloadMissing`), checksum 불일치(`PayloadCorrupt`), seed JSON 해석 실패, seed kind 불일치, `SurfaceData` 해석 실패, 저장된 kind와 다른 capture.
- 손상된 surface는 그 자리에서 빈 상태로 다시 시작한다. 터미널은 현재 셸 설정으로 새로 열고 이전 CWD·`restore.command`·scrollback을 쓰지 않는다. 다른 kind는 저장된 복원 값 없이 시작한다. 활성화 요청은 원본 capture를 다시 읽지 않는다.
- 원본 payload는 지우거나 덮어쓰지 않는다. `restore-failure` holder로 pin해 두므로 다음 capture가 surface의 참조를 새 자료로 바꾼 뒤에도 GC되지 않는다. 이 holder는 자동으로 풀지 않는다. 행 자체가 없는 경우에는 보존할 바이트가 없다. 있는 행에 pin을 쓰지 못하면(디스크 가득 참 등) worker가 그 payload를 기억하고 다음 checkpoint마다 pin을 다시 건다. 다시 걸 때까지 checkpoint(snapshot·압축·GC)를 건너뛰고, surface의 자료 참조를 바꾸거나 없앨 수 있는 쓰기(capture·닫은 surface capture·구조 명령·엔진 retirement)도 그 직전에 pin을 먼저 다시 걸며 실패하면 거절한다. 그래서 보존이 확정되기 전에는 원 참조가 바뀌지 않는다. 이 기억은 프로세스 안에만 있지만, 참조가 그대로이므로 재시작하면 같은 surface가 같은 원본을 다시 읽어 같은 실패를 다시 보존한다.
- workspace 이름·surface ID·원인을 담은 경고 알림을 남기고 같은 내용을 warn 로그로 남긴다. 한 번에 여러 surface가 실패하면 같은 엔진의 경고 하나로 묶는다.
- 복원 실패 원본의 미보존 ID 집합과 구조 적재 보고서는 락 poison 뒤에도 기존 값을 유지하며 첫 poison을 오류 로그로 남긴다. ID 집합의 단일 갱신과 락 밖에서 완성한 보고서의 전체 교체만 수행하므로, poison을 빈 집합·깨끗한 보고서로 바꾸어 원본 보호나 손상 stream의 쓰기 차단을 잃지 않는다.

- 손상이 아닌 경쟁 상태는 결과를 버린다. 읽는 사이 엔진이 닫혔거나 surface가 더 이상 그 capture를 기다리지 않으면 아무것도 하지 않는다. 읽는 사이 capture가 바뀌었으면 새 capture를 다시 읽는다.
- 저장소 자체를 더 쓸 수 없는 읽기 오류(SQLite I/O 오류 등)와 결과 종류 불일치 같은 내부 오류만 halt로 남는다.
