# 부팅 시퀀스 (첫 윈도우) — 부팅 상태 머신

첫 윈도우는 `src/app/boot_machine.rs`의 `BootPhase` 상태 머신으로 부팅한다. 숨긴 창에서 첫 로딩 프레임을 그린 뒤 창을 표시한다. 대기는 sleep 대신 프레임마다 진행 상태를 확인하는 방식으로 처리한다.

## 시퀀스

```text
resumed → 숨긴 Window/GPU 준비 → begin_boot
  App.boot: BootResources(window, gpu, worker receiver, pending events)
  App.state.boot: BootProgress(settings, phase, restore presentation)
  첫 loading present 뒤 Window 표시

GpuInit → WaitingEngine
  worker가 EngineSession과 PluginManager를 준비
  결과를 pending engine 관계에 설치
WaitingJournal
  journal binding·초기 projection → 필요한 kind 확인
  → WaitingPlugins(필요한 경우) → WaitingJournal로 복귀
  → 선택한 자료의 activation/복원 완료를 poll
RestoringLayout
  native/plugin 복원 응답을 관측 → finish_boot
finish_boot
  MainViewState 조립 → IPC 서버와 window 등록 → startup_complete
  → 보류된 AppEvent 처리 → 정상 프레임
```

`BootResources`의 handle과 `BootProgress`의 값은 소유자가 다르다. worker 반환형도 CoreState 한 개가 아니라 EngineSession과 PluginManager다. journal의 초기 projection만 재생하는 단계와 실제 자원 activation을 구분한다. 오류 화면을 만들 수 있는 Window/GPU가 있으면 진단을 표시하고, 초기 View 조립 실패로 기존 journal stream을 삭제하지 않는다.

복구 가능한 journal 실패는 아래 임시 상태 경로로 계속한다. App·그래픽 초기화처럼 계속할 세션을 만들 수 없는 내부 실패는 원인을 보이고 실패 코드 1로 끝낸다. 같은 내용은 error 로그에 남고, 이 실행이 쓴 instance 파일(`tasty.instance`)은 끝나기 전에 지운다.

- `App::new` 실패(데이터 홈 부재, `structure-journal` worker·`TerminalSpawner` 스레드 생성 실패): 이벤트 루프는 프로세스당 하나뿐이므로 이미 만든 루프로 부팅 오류 화면(`boot.app_error.*`)을 띄운다(`home_in_use::run_screen_on`).
- `assemble_app_state` 실패: 부팅 창과 GPU가 준비돼 있으므로 엔진 오류와 같은 부팅 오류 화면 모드로 넘긴다. 문구는 `boot.app_error.*`다.
- 오류 화면용 창이나 GPU를 만들지 못하면 같은 문구를 OS 메시지 상자로 보인다. Linux는 zenity가 없으면 데스크톱 알림이다(`single_instance::second::show_notice`). 데이터 폴더 사용 중 화면도 같은 폴백을 쓴다.
- debug 빌드는 `TASTY_DEBUG_BOOT_FAULT=app_new|assemble`로 두 실패를 주입할 수 있다(`src/boot/debug_boot_fault.rs`). release에는 이 경로가 없다.

일반 부팅과 shell setup 완료는 같은 begin_boot로 합류한다. 새 창·parked 복원은 `window_lifecycle/pending.rs`의 pending 관계와 journal 준비 관측을 사용한다. 공유 registry는 AppServices의 같은 Arc를 주입한다. 특정 첫 View의 실행 자원을 전역 서비스 원본으로 사용하지 않는다.

## 부팅 가드 (bootstrap 불변식)

초기 journal projection과 activation을 공개하기 전에는 일반 입력·변경 처리를 보류한다. 부팅 미완 동안:

- **window event** 는 전부 소비한다 (`handle_boot_window_event` — RedrawRequested
  = 스텝 구동, Resized = gpu.resize, Focused = egui의 OS 포커스 갱신, CloseRequested = 종료, 그 외 무시).
  부팅 중 받은 포커스를 버리면 첫 화면의 입력칸이 계속 비포커스로 판정되어 Enter 확정이 막히므로,
  포커스는 입력 실행과 분리해 로딩 화면부터 보존한다.
  CloseRequested 시 `shutdown_step_reclaim_boot_worker` 가 먼저 불려 —
  `WaitingEngine` 체류 중이면 워커 결과(최대 5s 대기)를 회수해 그 안의
  `PluginManager` 를 graceful shutdown 한다(잔존 plugin 자식 프로세스 방지).
  WaitingEngine 이 아니거나 부팅 완료 후(steady-state 종료 cascade)에는 no-op.
  이 구간의 계측은 종료 쪽 마커 `S2 boot_worker_reclaim` 이다 —
  [shutdown-sequence](shutdown-sequence.md).
- **AppEvent** 는 종료 계열(Shutdown/QuitRequested)만 즉시 처리하고 나머지는
  `BootResources.pending_events` 에 지연 → Ready 후 도착 순서대로 재생한다. 특히
  `TerminalOutput` 을 부팅 중 소비하면 대상 engine 이 아직 창에 붙기 전의 임시 관계
  (`App.engines`)에 있어 waker dedup 게이트가 닫힌 채 wake 가 유실된다.
- **IPC** 는 서버 자체가 `finish_boot` 에서 시작하므로 부팅 중 유입이 구조적으로
  없다. `about_to_wait` 의 steady-state 파이프라인(plugin pump / intent drain 등)도
  부팅 미완 동안 타지 않는다.
- `resumed()` 재진입(macOS 등)은 `boot.is_some()` 가드로 창 중복 생성을 막는다.

## 부팅에 걸리는 일 (트리거와 무관한 일)

첫 요청이 없어도 필요한 초기화는 부팅에서 수행한다. 플러그인 설치·namespace 표 등록과 프로세스 기동은 구분한다. 판단 기준은 [플러그인 제작 가이드](../dev-guide/plugin-development.md)에 있다.

`run_headless`의 메모리 저장소 초기화는 `src/boot/headless_memory.rs`의 `boot_memory`가 맡는다. 기존 부팅 정비와 오류 시 대체 저장소 연결을 수행한 뒤 같은 위치에서 App을 생성하며, IPC·플러그인·엔진 초기화 순서는 유지한다.

지금 명부에 오른 일과 조합별 자리:

| 일 | headless | gui |
|----|----------|-----|
| 번들 plugin 설치 (`install_builtins_if_needed`) | `run_headless` (`src/boot.rs`) | `prepare_plugin_manager` (`src/app/window_lifecycle.rs`) |
| namespace 소유 표 설치 (`install_namespace_table`) | `run_headless` (`src/boot.rs`) | `prepare_plugin_manager` (`src/app/window_lifecycle.rs`) |
| 공용 표 반영: 상속 환경변수 제거 규칙·namespace 소유 상태 (`publish_shared_tables`) | `run_headless` (`src/boot.rs`) | `prepare_plugin_manager` (`src/app/window_lifecycle.rs`) |
| agent 재시작 기록 정리·핸들 재적재 (`purge_stale_agent_state_on_boot`) | `bootstrap_engine` (`src/boot.rs`) | `finish_boot` (`src/app/boot_machine.rs`) |

소유 표 설치는 `PluginManager`가 사용하는 표의 핸들을 `tasty-ipc`에 전달하는 작업이다. 두 크레이트는 같은 표를 읽으며, `refresh_packages`가 설치된 매니페스트에서 표 내용을 갱신한다. 표를 복사하지 않는 이유는 [플러그인 제작 가이드](../dev-guide/plugin-development.md)에 있다.

설치와 namespace 표 등록 자체는 프로세스를 시작하지 않는다. 헤드리스는 이 메타데이터만 준비하고 플러그인이 필요한 첫 호출까지 기동을 미룬다. GUI는 `prepare_plugin_manager`로 메타데이터를 준비하고 엔진을 만든 뒤 `start_plugin_manager`(`discover_and_start`)로 활성 플러그인을 기동한다.

두 환경 모두 메타데이터 준비(번들 설치·갱신, namespace 표, 설치 목록, 상속 환경변수 제거 규칙과 namespace 소유 상태 반영)를 엔진 생성보다 먼저 한다. 엔진은 생성 중에 복원 셸이나 기본 셸을 만들 수 있으므로, 활성 플러그인이 선언한 제거 규칙([플러그인 제작 가이드](../dev-guide/plugin-development.md#상속-환경변수-제거-contributesstrip_inherited_env))이 그 첫 셸부터 적용되고, agent task 생성·실행의 provider 자격 판정([작업 러너 §agent task](../dev-guide/agent-runner.md#agent-task))도 표 발행 뒤에만 일어난다. 헤드리스는 엔진과 매니저가 같은 waker factory를 쓴다. 가드는 이 순서도 본문 안 첫 등장 위치로 확인한다. agent 러너 스레드는 두 환경 모두 `agent.task_run --action start` 전까지 시작하지 않는다([작업 러너](../dev-guide/agent-runner.md)).

`src/source_guards/jobs_anchored_at_boot.rs`는 GUI와 헤드리스의 지정된 부팅 함수가 필요한 초기화를 호출하는지 확인한다. 한쪽 환경의 호출이 빠져도 실패한다.

## 로딩 프레임

`GpuState::render_loading`(`src/gfx/gpu/loading.rs`) — 배경(theme `bg_app` 토큰)
위에 워드마크 → 스피너 → phase 문구를 세로 중앙 스택으로 그리는 egui 프레임.
hidden 창은 `RedrawRequested` 를 못 받을 수 있으므로 첫 프레임은 `begin_boot`
가 이벤트 대기 없이 직접 그리고, 이후는 `about_to_wait` 의 WaitUntil 워치독이
스텝을 보장한다.

- **워드마크** — 수박 마크(64px) + `tasty.` mono(38px, `.` 는 `--tasty-brand-melon-flesh`)
  브랜드 락업(`guidelines/brand-logo.html` verbatim, 14px UI 폰트 상한의 sanctioned
  예외). `src/adapters/ui/brand.rs::draw_wordmark` — 사이드바 헤더(22px/17px)와
  같은 함수를 크기와 자간만 다르게 호출해 공유한다. 자간은 락업이
  `loading_lockup_tracking`(−1px, UI 배율을 따른다), 사이드바가
  `sidebar_wordmark_tracking`(0)이다.
- **스피너** — `tasty-ui-widgets::Spinner`(공용 위젯) 재사용, 크기 32(기본
  16→boot hero), 색 `accent_primary()` 명시 지정(미지정 시 기본은
  `text_muted()`). 부팅 시작 `Instant` 가 아니라 egui `ctx().input(|i| i.time)`
  경과 기반 등속 회전 — 프레임 드랍에도 각도는 시간에 비례한다.
- **phase 문구** — `BootPhase` → i18n 키 매핑(`boot_phase_text_key`,
  `GpuInit`/`WaitingEngine` 은 문구 공유): `boot.phase_gpu_init`("Initializing graphics…") /
  `boot.phase_waiting_plugins`("Loading plugins…") /
  `boot.phase_restoring_layout`("Restoring layout…"). 스피너 아래 고정 높이
  슬롯(16px)에 그려 문구 유무와 무관하게 레이아웃이 흔들리지 않는다(첫 설치는
  `RestoringLayout` 을 스킵할 수 있어 문구 순서 불연속 허용).
- **레이아웃은 창 크기 불변** — 1280×720 기본 창과 640×480 최소 창 모두 동일
  절대 크기의 중앙 스택(반응형 축소 없음), 남는 공간만큼 `top_pad` 로 수직
  중앙 정렬.
- **전환** — Ready 도달 시 즉시 스냅(0ms). 로딩 프레임과 첫 실 UI 프레임은
  서로 다른 렌더 경로(`render_loading` → `finish_boot` → 통상 프레임 파이프라인)라
  크로스페이드는 두 경로를 한 프레임에서 합성해야 하는 구조적 비용이 크고,
  부팅 자체가 대개 1초 미만이라 디자인이 허용한 저비용 폴백을 채택했다
  (페이드 채택 시 재검토 지점 — `--tasty-motion-ui-fade` 200ms).
- **테마** — 하드코딩 다크 없음, `boot_apply_theme()` 가 첫 present 전에 저장된
  테마(Mocha/Latte)를 적용하므로 GPU clear color 도 resolved theme 을 그대로
  따라간다.
- **종료와 공유한다** — `render_loading` 은 phase 타입이 아니라 i18n 키를 받으므로
  종료 상태 머신도 같은 함수로 같은 락업을 그린다(문구만 다르다). 종료 쪽은
  [shutdown-sequence "종료 화면"](shutdown-sequence.md) · [종료 시퀀스 — 종료 cascade + Drop tail](shutdown-sequence.md).
- 갤러리 specimen: `crates/tasty-gallery/src/catalog/chrome_loading.rs`
  (Chrome 카테고리) — 부팅 5종(기본/최소창/phase 문구 3종/문구 없음/Latte) +
  종료 2종(기본/phase 문구 4종) + 첫 실행 셸 설정 화면 14장(Windows 판정 4종, macOS 4종 — 파일 없음·디렉터리·실행 권한 없음·fish 확정, Latte 2장, 저장값이 거부되고 기본 위치에 Git Bash가 있는 Windows 4장)
  + 부팅 오류 화면 4장(엔진 오류, 데이터 폴더 사용 중, 웹훅 포트, Latte 데이터 폴더 사용 중).
  웹훅 포트 장은 디자인 시안 순서를 지키는 긴 CLI 안내 문구 예시이며, 본체는 이 경우 화면을 띄우지 않는다.
- **첫 실행 셸 설정 화면** — `render_shell_setup` 은 공용 view
  `tasty_ui_widgets::shell_setup_screen` 을 부르고 갤러리도 같은 함수를 그린다. 부팅 화면과
  같은 `bg-app` 채움(GPU clear 색 포함)과 락업 아래에 `size-360` 폭 폼을 세로 가운데에 쌓는다.
  카드는 없다. 폼은 제목 "Choose a shell", 부제, Windows Git Bash 안내 줄(상자 없는
  경고색 caption, `alertTriangle`), mono 경로 `Input`, 검증 줄, 오른쪽 정렬 버튼 줄이며
  간격은 `spacing_sm` 이고 버튼 줄 위에 `spacing_sm` 을 한 번 더 둔다.
  - 검증 줄은 호스트 판정마다 한 문구다. 빈 경로는 문구 없이 caption 한 줄 높이만 잡고,
    파일이 없으면 "No file at this path", Windows 에서 경로에 무언가 있지만 Git Bash 가 아니면
    "Not Git Bash. Use bash.exe from Git for Windows", macOS·Linux 에서 디렉터리면 "This path is a folder, not a
    program", 실행 권한이 없으면 "This file has no permission to run"(모두 `alertCircle`·accent-danger),
    쓸 수 있으면 macOS·Linux 는 "Shell found", Windows 는 "Git Bash found"(`check`·accent-success)다. 판정은 설정의 `ShellPathStatus` 를 그대로
    옮긴다. macOS·Linux 는 이름을 보지 않으므로 fish 같은 셸도 확정할 수 있다.
  - 이 화면은 설정 셸을 쓸 수 없고 대체할 셸도 없을 때, macOS·Linux 에서 설정 셸이 디렉터리이거나
    실행 권한 없는 파일일 때, Windows 에서 저장값이 Git Bash 가 아니거나 없는 경로일 때(대체하지 않는 판정) 열린다([터미널 surface "실행할 셸"](../surfaces/terminal/index.md)).
    입력란은 설정한 셸 경로로 채워 열리므로 판정 줄이 곧바로 원인을 보인다. 빈 설정이면 빈 칸이다.
    Windows 안내 줄은 항상 보인다. 기본 위치에서 Git Bash 를 찾지 못했으면 "Git Bash was not found in the
    default install locations. Install Git for Windows, or enter the path to bash.exe if it is elsewhere.", 찾았으면
    "On Windows, Tasty runs only Git Bash. Enter the path to its bash.exe." 다. 이 탐색 결과만 따르므로 입력 중에
    판정이 바뀌어도 그대로다.
  - 버튼은 공용 `Button` Secondary "Quit"(앱 종료)과 Primary "Use this shell"(md)이다.
    판정이 유효할 때만 확인이 활성이며 Enter 확인도 같은 조건을 따른다. disabled 모양은
    공용 규칙(중립 상자와 disabled ink)을 따른다.
  - 경로 판정 시험은 플랫폼별로 나뉜다. `src/gfx/gpu/shell_setup.rs`의 시험 모듈은 Unix
    실행 권한 fixture를 쓰므로 Windows 빌드에서 제외한다. Windows의 Git Bash 배치 조건은
    `tasty-utils`의 `shell_family`와 `tasty-settings`의 `shell_path` 시험으로 확인하며, 이 시험들은 실제 화면 입력 검증을 대신하지 않는다.
- **부팅 오류 화면** — `render_boot_error` 는 공용 view `tasty_ui_widgets::boot_error_screen` 을
  부르고 갤러리도 같은 함수를 그린다. 로딩·셸 설정 화면과 같은 구조로 `bg-app` 채움 위에 락업 →
  `spacing_xl` → `boot-form-width`(360) 폼을 세로 가운데에 쌓는다. 카드와 그림자는 없다. 폼은
  제목 줄(alertCircle 16, `boot-error-glyph` → accent-danger, 간격 `spacing_sm`, 제목 14 text-primary,
  줄바꿈) · 본문(body, text-secondary) · 안내(caption, text-muted) · 오른쪽 정렬 Quit(공용 `Button`
  Secondary md)이며 항목 사이는 `spacing_sm`, Quit 위는 `spacing_sm`을 한 번 더 둔다. 본문과 안내의
  백틱 구간은 code run 이다. 문구는 자르지 않는다. 경로처럼 공백 없이 긴 낱말은 폼 폭에서 끊는다.
  쌓은 높이가 창 높이 − 2 × `spacing_xl`보다 크면 락업을 먼저 빼고, 그래도 넘치면 제목·본문·안내를
  스크롤하고 Quit은 그 아래에 고정한다. 엔진 생성 실패(첫 workspace를 만들지 못함 포함), App 생성·첫 화면 조립 실패, 다른 프로세스가
  쓰는 데이터 폴더에서 뜨고 Quit·Esc·Enter·창 닫기가 종료 코드 1로 끝낸다. 키 판정은 호스트가 한다.
  열지 못한 명시 웹훅 포트는 실행을 막지 않고 경고 알림으로 남긴다([웹훅](../features/webhook/index.md#리스너-주소와-포트)).
- **저장소를 열지 못한 부팅** — 첫 엔진의 journal 시작 또는 첫 View 공개 전 복원이 실패하고 홈 사용 중이
  아니면 부팅 오류 화면 대신 `JournalApplication::poll_boot_recovery`로 빈 임시 저장소를 열어
  같은 엔진을 다시 시작한다. binding 이후에는 먼저 원 worker를 종료 checkpoint 없이 중단하고
  Creation·설치된 PTY·plugin을 별도 회수 컨테이너에 분리한 뒤 binding·projection을 비운다.
  정확한 reap·plugin 응답이 미완이면 그 컨테이너를 보관하고 새 터미널과 별도로 관측한다.
  임시 상태는 원본을 재수입하지 않고 저장된 복원 명령을 재실행하지 않는다. 기본 workspace가 생기면 평소처럼 창을 보이고, 원인·원본 경로를 담은
  오류 알림과 오른쪽 위 비포커스 안내를 전달한다. 임시 저장소도 실패하면 부팅 오류 화면으로 간다.
  판정은 `src/app/boot_machine.rs`의 `journal_failure_falls_back`.
- **View 밖 창의 다시 그리기** — 첫 실행 셸 설정 창과 부팅 오류 화면은 View 밖에 있어 dirty
  표시나 렌더 예약을 받지 않는다. 클릭·Enter 처리는 렌더 안에서 일어나므로 창이 스스로 redraw 를
  요청한다. egui 가 입력에 repaint 를 답하거나 즉시 repaint 를 요청하면(`AppEvent::EguiRepaint`)
  다시 그리고, `Resized`·`ScaleFactorChanged` 에서는 surface 를 다시 구성한 뒤 그린다. 렌더가
  `Lost`·`Outdated` 로 실패하면 surface 를 다시 구성하고 한 번 더 그린다. 판정은
  `src/app/offview_window_events.rs`. 지연 repaint(툴팁 표시 지연 등)는 다른 창과 같이
  `Tick::EguiRepaint(창)` 타이머로 예약해 그 시각에 다시 그린다.

## 부팅 계측 (target: `tasty::boot`)

부팅 경로는 tracing으로 단계별 소요 시간을 기록한다. debug 빌드는
`$TASTY_HOME/debug-dev.log`(debug 레벨 file layer)에 수집되고, stderr 기본 필터가
warn 이라 콘솔 노이즈는 없다. release 검증은 `TASTY_LOG=info` 로 실행한다.

| 마커 | 구간 |
|------|------|
| T1 window_create | `resumed()` 진입 → `create_window` 반환 (hidden) |
| T2 gpu_init | `create_gpu_state` |
| T2.5 db_theme | `begin_boot` 진입 → 첫 로딩 프레임 직전 (db::init + theme apply) |
| T2.9 window_visible | 부팅 시작 → `set_visible(true)` (첫 로딩 프레임 present 후) |
| resumed_total | `resumed()` 전체 (T1~T2.5 + 첫 프레임 — 메인 스레드 점유 구간) |
| T2.6 engine_init | EngineSession 실행 owner 준비 (**부팅 워커 스레드**에서 계측; journal 선택·복원은 별도 대기) |
| T3a/T3b | plugin discovery(엔진 생성 전) / spawn(엔진 생성 뒤) (**부팅 워커 스레드**) |
| T2.7 engine_wait | `WaitingEngine` 체류 — 메인이 워커 결과를 기다린 시간. `frames` 필드 = 그 동안 돈 로딩 프레임 스텝 수(로딩 프레임이 실제로 갱신됐다는 계측 증거) |
| T4 layout_wait_plugins | WaitingPlugins 체류 (탈출 사유 satisfied/deadline) |
| T5 layout_apply | ApplyPendingLayoutRestore |
| T6 remote_surface_wait | RestoringLayout 체류 (탈출 사유 satisfied/deadline) |
| boot_total | 부팅 시작 → Ready |
| T7 first_paint | Ready → 첫 실 UI present (`src/boot/trace.rs` 원샷) |

실측 기준치(Windows/7950X3D, debug): T2(GPU init, 메인 고정) ≈ 540~570ms, T3(plugin
discovery/spawn) ≈ 430~465ms(플러그인 11개 설치 시; 첫 설치는 ≈0.6ms)가 지배
구간이고 T4/T6 은 <3ms(satisfied)다.

- **T2 는 메인 스레드 고정**이다 — wgpu surface 가 winit window 핸들에 결합돼
  있어 워커로 옮길 수 없다. 이 구간은 로딩 프레임 자체가 아직 없으므로(첫 present
  이전) 스피너 정지가 발생하지 않는다.
- **T2.6+T3(≈470ms)는 `WaitingEngine` 워커 스레드로 옮겨졌다** — 원자 스텝이지만
  메인은 이 구간에도 `about_to_wait` 워치독(16ms)이 매 프레임 로딩 렌더를
  지속하므로 스피너가 멈추지 않는다(T2.7 의 `frames` 로 실측 확인).
- 대기 루프(T4·T6)는 이미 <3ms 수준이라 워커로 옮길 대상이 아니다.
