# ADR-0117: 창·모달 생성 실패는 패닉이 아니다 — 부팅은 진단 후 종료, 그 외는 취소 후 안내하며 안내 채널은 요청 origin 이 가른다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: error-handling, window, modal, boot, panic, toast, info-modal, focus, identity-principle-1, i18n, shutdown
- **Group**: window-workspace-lifecycle

## Context

창을 여는 모든 경로가 실패를 `expect` 로 처리하고 있었다. 호스트 측 12 개 지점 —
새 창(`window_lifecycle.rs` 의 창 생성 · GPU 초기화 · 엔진 생성), 설정 · 플러그인 ·
종료 확인 모달 각각의 창 생성 · GPU 초기화, 부팅 창 생성, 그리고 스레드 spawn 2 종
(`output_observer` 의 sink 워커, `agent_runner` 러너) — 이 전부 패닉했다.

패닉의 피해가 그 창 하나로 끝나지 않는다는 것이 문제였다. 이 코드는 **이미 터미널
세션이 여러 개 떠 있는 상태**에서 돌기 때문에, 새 창 하나가 안 열린 결과가 프로세스
전체의 죽음이고 곧 **실행 중인 모든 창의 작업 유실**이다.

가장 위험한 지점은 엔진 생성이었다. `CoreState::new_with_ids` 의 유일한 `?` 는
`Pane::spawn_terminal` 이고 그 인자는 `ShellConfig::from_settings` 다. 즉 사용자
`config.toml` 의 셸 경로 오타나 PTY/fd 고갈이 "새 창 열기" 한 번으로 전 세션을 날렸다.
**사용자 입력이 직접 패닉으로 이어지는 경로**였다.

부팅 경로만 예외적으로 GPU 어댑터 부재를 graceful 하게 처리하고 있었고(진단 후 정상
종료), 그 처리가 새 창·모달 경로로 전파되지 않은 상태였다.

## Decision

창·GPU·엔진·스레드 생성 실패를 **전부 `Result` 로 표면화**하고, 실패 지점의 성격에
따라 셋으로 나눠 처리한다.

**1. 부팅 경로 — 진단 후 정상 종료.** 첫 창이 없으면 안내를 띄울 대상 자체가 없다.
패닉(= 가짜 크래시 리포트)이 아니라 사람이 읽을 진단 3 줄(title/body/hint)을
`tracing::error!` 한 이벤트로 내고 `exit(1)` 한다. `tracing::error!` 를 쓰는 이유는
stderr(터미널에서 실행한 사용자에게 보임)와 **공유 로그 파일 양쪽**에 나가기 때문이다 —
`eprintln!` 은 파일에 남지 않아 사후 진단이 불가능하고, pre-commit C.11 이 금지하는
형태이기도 하다. 부팅 창 생성 실패 · 부팅 엔진 생성 실패 · GPU 어댑터 부재가 모두 이
처리를 공유한다. 어댑터 부재가 **아닌** 예상 밖의 GPU 실패는 예외로 패닉을 유지한다 —
그건 환경 문제가 아니라 버그이므로 크래시 리포팅 경로에 남아야 한다.

> **2026-09-04 갱신 (결정 변경 — 부팅 엔진 실패에 한함):**
> 세 부팅 실패 중 **부팅 엔진 생성 실패만** "진단 후 `exit(1)`" 에서 **창에 실패 화면을
> 그려 유지** 하는 쪽으로 바꾼다. 이 단계는 부팅 GPU init 이후라 **GPU 와 창이 이미
> 살아있어** 그릴 수단이 있고, 실패 원인은 대개 사용자 `config.toml` 의 셸 경로 오타다.
> 처음 이 결정은 세 경우를 한 처리로 묶었지만, dock/시작 메뉴/런처로 실행한 사용자는
> stderr 를 볼 수 없어 "창이 잠깐 떴다 사라지는 것" 이 전부였다(아래 재검토 트리거
> 마지막 항목이 예견한 문제). `boot_fatal_engine_error(err) -> !` 를
> `boot_engine_error_info(err) -> BootErrorInfo`(종료하지 않고 진단만 반환)로 바꾸고,
> `drive_boot_frame` 이 그 진단을 보면 `enter_boot_error_mode` 로 전환한다 —
> `enter_shell_setup_mode` 가 부팅 시점에 egui 첫 프레임을 직접 그리는 선례를 그대로
> 따른다. 사용자가 "종료"(또는 Esc/Enter)를 누를 때까지 화면을 유지하고, 그때 `exit(1)`
> 한다(종료 코드 유지 — 엔진이 없어 정상 진행은 여전히 불가능). stderr·파일 로그
> 진단은 그대로 남긴다. **GPU 어댑터 부재와 부팅 창 생성 실패는 §1 그대로** — 그릴 GPU
> 나 창 자체가 없어 화면을 띄울 수단이 없으므로 진단 후 `exit(1)` 를 유지한다(그 두
> 경우의 "보이는 안내" 는 OS 네이티브 다이얼로그가 필요한 3-플랫폼 작업이라 별건으로
> 남긴다). 회귀 방어: 단위 테스트(`boot_engine_error_info` 세 진단이 distinct) +
> 소스 형태 가드(`crates/tasty-doc-guards/tests/boot_engine_failure_stays_visible.rs`).

**2. 새 창 · 설정 · 플러그인 모달 — 그 창만 취소하고 안내.** 이미 떠 있는 창들은 건드리지
않는다. 안내는 실패한 창이 아니라 **살아 있는 메인 창**에 띄운다(실패한 창은 엔진도 GPU 도
없어 그릴 수 없다). 포커스된 뷰가 모달이면 남은 메인 창 중 하나로 폴백한다 — "포커스가
마침 모달에 있었다" 는 이유로 안내가 조용히 사라지지 않게 한다. 안내 문구는 지점마다
다른 i18n 키를 쓴다(새 창 / 설정 / 플러그인) — 설정 창이 안 열렸는데 "새 창을 열 수
없습니다" 가 뜨면 그건 틀린 안내다.

**3. 안내 채널은 요청 origin 이 가른다 — 사용자는 모달, 에이전트는 요청자에게 직접.**
`AppEvent::CreateWindow` 가 `WindowRequestOrigin`(User / Agent)을 싣는다. 메뉴 · 단축키 ·
dock · tray 발 실패는 사용자가 방금 한 조작의 결과이므로 `InfoModal` 로 확실히 알린다.
근거는 핵심 원칙 1 이다 — 에이전트 행동의 부수효과는 사용자 상태(포커스)에 닿지 않는다.
`InfoModal` 은 `OpenPopupMode::CenteredFocused` 로 포커스를 가져가므로, 에이전트가 부른
작업의 실패 통지 때문에 사용자가 하던 일의 포커스를 잃게 된다. "실패를 알린다" 는 선의는
이 원칙의 예외 사유가 되지 못한다.

> **2026-09-04 갱신 (결정 변경, [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md)):**
> 에이전트(`window.create` / `view.create` IPC) 발 실패는 이제 **요청자에게 IPC 응답
> 에러로 직접 돌려주고, 사용자 화면에는 아무것도 띄우지 않는다**(toast 제거). 처음 이
> ADR 은 응답 계약이 fire-and-forget(`{"scheduled": true}`)이라 실패를 요청자에게 돌려줄
> 길이 없어 차선책으로 toast 를 골랐다(아래 Alternatives 의 "동기 응답" 항목을 범위 밖으로
> 남겼던 이유). 그러나 사용자가 요청하지도 않은 일의 실패 통지가 화면에 뜨는 것 자체가
> 원칙 1 이 막으려는 바로 그 부수효과였다 — toast 는 포커스를 안 가져갈 뿐 사용자 상태에
> 닿지 않는 것은 아니다. ADR-0122 로 완료 채널(동기 응답) 왕복이 생겨 그 차선책이
> 불필요해졌으므로, 정확한 해법(요청자에게 직접)으로 바꾼다. 사용자 발 실패의 `InfoModal`
> 은 그대로다.

**종료 확인 모달만 다르게 다룬다 — 확인을 건너뛰고 종료한다.** 다른 창들처럼 "취소 후
안내" 하면 사용자가 **앱을 끌 수 없는 상태**에 갇힌다. 창 생성이 실패하는 환경은 이미
degraded 라 남는 수단이 프로세스 강제 종료뿐이다. 사용자가 표명한 의도는 이미 "종료"이고
확인 모달은 그 의도를 되묻는 장치일 뿐 종료를 막는 장치가 아니므로, `begin_shutdown()`
으로 폴백한다. 확인 절차가 생략됐다는 사실은 toast 와 `tracing::error!` 로 알린다.

**스레드 spawn 실패**도 같은 원칙을 따른다. sink 워커는 `ObserverError::ThreadSpawn` 으로
반환해 `observe.start` 의 JSON-RPC 에러로 에이전트에게 도달시키고(같은 함수의 파일 열기
실패와 비대칭이던 것을 해소), agent runner 는 로그를 남기고 등록하지 않아 `status` 의
`running: false` 로 드러나게 한다.

**plugin 프로세스의 스레드 spawn 은 패닉을 유지한다.** 폭발 반경이 다르다 — 호스트가
아니라 그 plugin 프로세스만 죽고, 호스트는 plugin 사망을 이미 감지·복구한다.

## Consequences

- **얻은 것**: 사용자 `config.toml` 의 셸 경로 오타가 더 이상 전 세션을 날리지 않는다.
  새 창만 취소되고 나머지 창의 터미널은 계속 동작한다. 실패 원인이 모달/toast 본문과
  파일 로그 양쪽에 남아 "방금 왜 안 됐지" 에 답할 수 있다. 에이전트가 부른 작업의 실패가
  사용자 포커스를 빼앗지 않는다.
- **잃은 것**: 실패 처리 분기가 늘어 창 생성 경로가 길어졌다. origin 을 싣기 위해
  `AppEvent::CreateWindow` 가 페이로드를 갖게 됐고 모든 producer 가 origin 을 명시해야
  한다. 종료 확인 모달 실패 시에는 확인 절차 없이 종료되므로, 확인 화면이 잡아주던
  "정말 끌 것인가" 가 그 상황에서만 생략된다.
- **운영 비용 / 유지 부담**: 안내 문구가 지점별로 갈라져 i18n 키가 늘었다(3 개 지점 ×
  title/body + 종료 폴백 1 개, 세 언어). 종료 폴백의 toast 는 종료 화면이 곧바로 덮으므로
  **best-effort** 이고, 사후 진단의 실체는 파일 로그의 error 라인이다.
- **회귀 방어**: 이 경로들은 winit `ActiveEventLoop` 와 GPU 상태가 있어야 돌아가 행동
  테스트로 감쌀 수 없다. 그래서 두 겹으로 고정한다 — 테스트 가능한 seam 은 단위 테스트
  (엔진 생성이 잘못된 셸에서 `Err` 를 내는지, `ThreadSpawn` 이 JSON-RPC 에러로 매핑되는지),
  감쌀 수 없는 분기는 소스 형태 가드(`crates/tasty-doc-guards/tests/no_panic_in_window_creation.rs`)로 막는다.

## Alternatives Considered

- **전부 모달로 안내한다** — 구현이 가장 단순하고 실패를 확실히 알린다. 그러나 에이전트
  IPC 발 실패까지 사용자 포커스를 가져가 핵심 원칙 1 을 어긴다. 사용자는 자기가 하지 않은
  일의 실패 통지 때문에 하던 일을 뺏긴다.
- **전부 toast 로 안내한다** — 포커스를 전혀 건드리지 않아 원칙 위반이 없다. 그러나
  사용자가 방금 "새 창" 을 눌렀는데 아무 일도 안 일어나고 곧 사라지는 toast 만 뜨면
  실패를 놓치기 쉽다. 사용자 조작의 직접적 결과는 확실히 알려야 한다.
- **현행 패닉 유지** — 크래시 리포트가 남아 개발자 진단은 쉽다. 그러나 사용자 설정 오타의
  대가가 전 세션 유실인 것은 어떤 진단 편의로도 정당화되지 않는다.
- **종료 확인 모달 실패 시에도 취소 후 안내** — 다른 창들과 일관된다. 그러나 사용자를
  "앱을 끌 수 없는 상태" 에 가둔다. 일관성보다 탈출구가 우선이다.
- **IPC 응답을 동기로 바꿔 에이전트에게 실패를 직접 돌려준다** — 가장 정확한 해법이다.
  이 ADR 을 처음 쓸 때는 `window.create` 가 `{"scheduled": true}` fire-and-forget 계약이라
  응답 계약 변경이 필요했고 창 생성 실패 처리와 독립된 작업이라 범위 밖으로 남겼다.
  **2026-09-04 채택됨 — [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md).**
  완료 채널 왕복으로 응답 계약을 바꿔 위 §3 갱신대로 에이전트 발 실패를 요청자에게 직접
  돌려주고 toast 를 제거했다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- ~~`window.create` / `view.create` 의 응답 계약이 동기로 바뀐다~~ — **발생했고 조치됨
  (2026-09-04, [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md)).** 에이전트
  발 실패를 응답 에러로 돌려주고 사용자에게 아무것도 띄우지 않는 쪽을 택했다(§3 갱신).
- 설정 · 플러그인 창을 여는 **release** 경로가 에이전트에게 열린다 — 지금은
  `debug.settings.open` 이 debug 빌드 전용이라 그 세 지점을 `User` 로 고정했다. release
  에 에이전트 경로가 생기면 origin 을 실제로 실어야 한다.
- 종료 확인 생략이 실사용에서 작업 유실로 보고된다 — 그때는 toast 대신 확인 없이 끌 수
  없게 하는 다른 탈출구(예: 확인을 in-window `InfoModal` 로 그려 OS 창 생성을 요구하지
  않게 하는 방식)를 검토한다.
- 창이 하나도 없는 상태에서 창 생성 실패가 실사용에서 관측된다 — 지금은 안내를 띄울 메인
  창이 없으면 로그만 남긴다. 부팅 중에는 `AppEvent` 가 지연 재생되고 부팅 후에는 메인
  창이 최소 하나 있다는 전제 위에 서 있다.
- ~~부팅 실패 진단이 런처로 실행한 사용자에게 닿지 않는다~~ — **발생했고 부팅 엔진 생성
  실패에 한해 조치됨 (2026-09-04).** `enter_shell_setup_mode` 선례대로 부팅 화면에
  실패를 그려 유지하는 방식을 택했다(위 §1 갱신 노트). **GPU 어댑터 부재·부팅 창 생성
  실패는 여전히 열려 있다** — 그릴 GPU/창이 없어 이 방식이 닿지 않고, "보이는 안내" 는
  OS 네이티브 다이얼로그(3-플랫폼)가 필요해 별건이다. 그 두 경우가 런처 사용자에게
  실사용에서 문제로 보고되면, OS 네이티브 다이얼로그 대 "로그(`~/.tasty/debug.log`)로
  안내" 최소 개선을 저울질해 재검토한다.

## References

- [`docs/dev-guide/error-handling.md`](../dev-guide/error-handling.md) — 로그 레벨 · 의도적
  무시 · stdout 금지(C.11) 규칙
- [`docs/architecture/boot-sequence.md`](../architecture/boot-sequence.md) — `WaitingEngine`
  의 세 결과(Ok / Err / disconnect)
- [`docs/identity.md`](../identity.md) — 핵심 원칙 1 (사용자 행동 ↔ 에이전트 행동 분리)
- [`docs/design/flows/action-dispatch.md`](../design/flows/action-dispatch.md) — origin 별
  정책 차이. dispatcher 는 origin 정책을 강제하지 않고 호출자가 mode 를 고른다
- `crates/tasty-doc-guards/tests/no_panic_in_window_creation.rs` — 창·모달 생성 경로 패닉 재유입 가드
