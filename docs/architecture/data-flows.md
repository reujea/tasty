# 데이터 흐름

입력·출력·IPC·알림·설정이 모듈 사이를 이동하는 경로를 파일과 함수로 설명한다. Intent 큐의 처리 구조는 [action-dispatch](../design/flows/action-dispatch.md)를 참고한다.

`src/boot.rs::run()`은 winit 이벤트와 `AppEvent`의 TerminalOutput·IpcReady·StreamReady를 처리한다. 이벤트에 따른 큐 처리는 `dispatch_pending_*`에서, 시간에 따른 작업은 중앙 타이머 허브에서 관리한다. GUI의 `about_to_wait`와 헤드리스의 `recv_timeout` 루프는 실행 시각이 된 키만 처리한다([타이머 허브](../dev-guide/timer-hub.md)).

---

## 1. 키보드 입력 → 터미널 → 화면

```
winit KeyEvent / Ime
  → app/event_handler.rs (ApplicationHandler::window_event)
  → view/main/ (overlay/host-egui surface 면 egui 가 먼저 소비 — input-layer)
  → view/main/keyboard.rs (handle_keyboard_input) · view/main/ime.rs (handle_event)
      ├── 단축키·vi·escape 매칭 → origin과 원 대상을 담은 Intent 값
      └── 그 외 키 → 포커스 surface 로 분배:
          ├── Terminal → 원 surface/generation에 묶인 입력 요청 → App/Engine 실행 → PTY stdin → 셸
          └── egui-mesh(image, 그리고 markdown 의 확인 팝업 2개) → egui_mesh_push_key/text/ime
              → set_context.raw_input forward → plugin egui TextEdit (egui-mesh-channel)
```

셸 출력은 비동기로 돌아온다(흐름 2). View는 입력 값을 만들고 App/Engine adapter가 원 binding을 검사해 Terminal 또는 plugin에 전달한다. 단축키의 구조 변경과 고빈도 raw 입력은 같은 영속 사건으로 취급하지 않는다. (`markdown` 은 [WebView 호스트 계약](../design/systems/webview.md) 로 webview 전환됨 — 본문은 `set_context`/`paint` 를 아예 받지 않고, 네이티브 WebView 가 자체적으로 입력을 처리한다. 위 경로는 markdown 의 대용량/파일열기 확인 팝업 2개에만 해당.)

---

## 2. PTY 출력 → 파싱 → 렌더링

```
PTY stdout
  → Pty reader → 원 resource generation의 TerminalIngest
  → TerminalState 잠금 안에서 termwiz Parser → vte_handler/ (CSI/OSC/ESC 처리)
  → dirty/waker → AppEvent::TerminalOutput(id) → App의 도메인 관측
          └── osc.rs: OSC 7(cwd) · OSC 133(prompt boundary) · 알림 OSC → TerminalEvent
  → view/main/redraw.rs (handle_redraw)
  → gfx/gpu/render_pass.rs — accumulator 렌더 (단일 pass):
      begin_frame() → 각 surface append_terminal_viewport() → flush_buffers() → render_all()
      → wgpu submit + present
```

accumulator 모델(surface 별 submit 아님)·scissor·multi-page atlas 상세는 [gpu-rendering](../dev-guide/gpu-rendering.md).

---

## 3. IPC 요청 → 처리 → 응답

```
tasty-cli (또는 외부 프로그램)
  → TCP 연결 (~/.tasty/tasty.port)
  → hub IPC 서버 (수신 스레드) → JSON-RPC 파싱 → mpsc → AppEvent::IpcReady
  → boot.rs 메인 루프 → app/ipc.rs (process_ipc)
      → adapters/ipc/handler/ (도메인별 핸들러 + 권한 게이트 + audit)
      → RequestScope로 대상 해소; 구조 변경은 journal admission/commit/publication, 실행은 EngineMut adapter (origin=Agent)
      → JsonRpcResponse → TCP 회신
  → tasty-cli 결과 포맷 출력
```

플러그인 namespace 메서드(`claude.*` 등)는 `plugin_bridge/` 를 거쳐 plugin 프로세스로 위임된다. attach 스트리밍은 별도 `StreamReady` 경로(`tasty_ipc::stream_hub::StreamHub`).

**한 회차에는 두 예산이 있다.** gui 의 `process_ipc` 와 headless 의 `pump_ipc` 가 같은 규칙
(`app::ipc_round::IpcRound`)을 쓴다 — 큐에서 명령을 **하나 꺼내 끝까지 처리하고 다음 것을
꺼내며**, 명령 수가 `DRAIN_BUDGET_PER_ROUND`(동시 연결 상한에서 파생,
[IPC 서버](ipc-server.md))에 닿거나 경과 시간이
`ROUND_TIME_BUDGET`(16 ms, [IPC 서버](ipc-server.md))에
닿으면 멈춘다. 첫 명령은 시간과 무관하게 늘 처리한다. 남은 것은 **큐에 그대로** 있다가 다음
회차가 집는다. headless 는 이벤트 채널에 `IpcReady` 를 하나만 둔다(생산자 쪽 게이트) — 루프가 회차를
열기 전에 게이트를 풀고, 회차가 끝났을 때 큐에 명령이 남았으면 루프를 한 번 더 깨운다. 명령마다 wake 를
두면 지속 부하에서 채널에 적체가 쌓여 같은 채널의 plugin·PTY wake 가 그 뒤에서 굶는다
([IPC 서버](ipc-server.md)). 예산은 명령 사이에서만 보므로 **이미 실행 중인 handler 는 끊지 못한다.**

gui 에서는 회차가 `about_to_wait`(iteration 마다 한 번)와 `IpcReady` 사용자 이벤트 두 자리에서 돈다.
winit 은 사용자 이벤트를 큐가 빌 때까지 처리한 뒤에야 `about_to_wait` 로 넘어가므로, 사용자 이벤트가
늘 회차를 열면 지속 부하에서 타이머·입력·렌더가 굶는다. 그래서 사용자 이벤트는 직전 회차가 끝난 뒤
한 회차 예산이 지났을 때만 회차를 열고(`app::ipc::IpcPacer`), 예산에서 멈춘 회차는 루프를 스스로 한
번 더 깨운다 — 건너뛴 wake 가 남은 명령의 몫이었을 수 있어서다. 그 재깨움은 양보 규칙을
`about_to_wait` 한 번 사이에 한 번 건너뛴다([IPC 서버](ipc-server.md)).

순서는 도착 순이다. 연결 하나는 응답을 받을 때까지 다음 요청을 안 보내므로 큐에 한 번에 하나만
올리고, 그래서 어떤 요청 앞에 설 수 있는 명령 수는 연결 상한과 주입 깊이 상한으로 유한하다.
호출자별 스케줄링은 없다(docs/architecture/ipc-server.md).

호출자가 요청에 응답 대기 상한을 지정하면 명령은 **기한**(큐 진입 + 상한)을 든다. 회차는 명령을
꺼낸 직후, 게이트보다 앞에서 기한을 보고 지났으면 실행하지 않고 `-32067` 로 답한다. 응답을
기다리는 연결 스레드와 "시작했는가" 를 상태 칸 하나로 정하므로, 시작 뒤의 만료만 `-32061`(결과
불명)이다([IPC 서버](ipc-server.md)).

큐에서 **꺼낸** 쪽의 누계 — 회차가 멈춘 이유 · 실행 전 만료 수 · 지금 실행 중인(in-flight) 요청
수 — 는 `tasty_ipc::dispatch::DispatchStats`(`AppServices`의 dispatch 계측)에 있고, 큐에 **든** 쪽(입장 장부)과
함께 `CommandQueueSnapshot::read` 한 자리에서 읽는다. in-flight는 실행을 시작했고
명령 처리 또는 응답 대기가 끝나지 않은 요청이다([IPC 서버](ipc-server.md)). 응답 대기가 먼저 끝나도 명령을 처리 중이면 집계에 남는다.
두 값은 `system.pressure`(CLI `tasty list pressure`)의 `queue_admission` · `queue_dispatch` 덩어리로
나간다([IPC 서버](ipc-server.md)).

종료 중의 drain 은 이 정책을 따르지 않는다 — 남은 요청을 거절하며 비워야 한다
([shutdown-sequence](shutdown-sequence.md)).

### 스트림 회차

attach 스트림 연결의 읽기 스레드는 프레임을 `StreamInbound` 채널에 넣고 메인 루프를 깨운다.
프레임마다 `StreamReady` 를 보내면 원격 클라이언트 하나가 사용자 이벤트 큐를 계속 채울 수 있다.
winit 은 그 큐가 빌 때까지 다음 단계로 넘어가지 않으므로 키 입력·redraw·IPC 가 모두 굶는다.
그래서 스트림도 IPC 회차와 같은 규칙을 따른다.

- **깨움은 처리 전 하나만 둔다.** `tasty_ipc::stream_hub::StreamWakeGate` 가 처리 전인 깨움 표시를
  든다. 표시가 이미 서 있으면 읽기 스레드는 이벤트를 보내지 않는다. GUI 와 headless 가 같은 표시를 쓴다.
- **표시는 회차를 열 때 내린다.** 채널을 읽기 전에 내려야 그 뒤에 들어온 프레임이 새 깨움을 만든다.
- **한 회차는 `PUMP_MESSAGE_BUDGET` 개까지만 꺼낸다.** 남은 메시지는 채널에 두고 표시를 다시 세워
  다음 회차가 처리한다.
- **GUI 의 사용자 이벤트는 간격을 지킨다.** `StreamReady` 처리기는 직전 스트림 회차가 끝난 뒤
  `ROUND_TIME_BUDGET` 이 지났을 때만 회차를 연다(`app::stream_round::StreamPacer`). 건너뛴 이벤트는 표시를
  남겨 두므로 같은 iteration 에 새 이벤트가 쌓이지 않는다. 미룬 회차는 `about_to_wait` 가 처리한다.
  그래서 한 iteration 안의 스트림 회차는 많아야 두 번이다.
- **회차 뒤의 일은 바뀐 것만 다룬다.** GUI 회차는 끝에서 점유 화면 사본(`refresh_readonly_views`)을
  확인한다. 원본 터미널의 `ContentCut` 이 사본을 만들 때와 같으면 직렬화와 재해석을 건너뛴다.
  바뀐 것이 없을 때 회차 비용이 수신 메시지 수에만 비례하게 하려는 것이다.

mirror 연결(이쪽이 원격 workspace를 attach한 클라이언트)의 수신도 같은 규칙을 따른다. 연결마다 있는
읽기 스레드가 `App.attach_client_wake` 하나를 공유하므로 처리 전인 `AttachClientData` 도 하나뿐이다.
처리기는 같은 간격으로 회차를 열고 미룬 회차는 `about_to_wait` 가 처리한다. 한 회차는 연결마다 쌓인
출력을 모두 적용하며, 그 출력 큐는 연결마다 개수와 바이트 상한이 있고 넘치면 재동기화로 바뀐다.

이 두 이벤트를 게이트 밖에서 직접 보내는 코드는 `src/source_guards/stream_wakes_go_through_the_gate.rs`
가 찾는다. 이벤트 이름을 쓰는 줄만 보므로 별칭이나 매크로로 만든 이벤트는 찾지 못한다. 다른 주기적
깨움 가운데 journal·memory·DB·파일 writer는 `tasty_utils::batch_writer::WakeLatch` 로, PTY 출력은
surface별 gate로, IPC는 `IpcPacer` 로 같은 문제를 막는다.

---

## 4. 알림 발생 → 저장 → 표시

```
알림 소스 (tasty-terminal):
  ├── vte_handler OSC 9/99/777 → TerminalEvent::Notification
  ├── BEL → BellRing
  └── 프로세스 종료 → ProcessExited

수집/저장:
  → core 가 이벤트 수집 → store/notification.rs (NotificationStore::add)
      ├── 병합: 같은 소스 coalesce 윈도우 내 → body 합치기
      └── FIFO 상한 초과 시 pop_front
  → tasty-hooks (Notification 이벤트 훅)

표시:
  → adapters/ui/ 사이드바 워크스페이스 배지 + 알림 패널(adapters/ui/notification.rs, popup 시스템에
    등록된 headless popup — 렌더 루프 자체는 adapters/ui/popup/frame.rs)
```

알림이 발생하면 Store를 갱신한다. popup은 자동으로 열지 않고 사용자가 요청했을 때 표시한다([팝업 정책](../design/systems/popup.md)).

---

## 5. 설정 로드 → 적용

```
시작 시:
  → tasty-settings Settings::load()
      → ~/.tasty/config.toml → toml::from_str, 없거나 실패 시 default 폴백
      → #[serde(default)] 부분 TOML 지원
  → boot/core 초기화 시 GpuState/MainViewState 에 반영 (font·theme·opacity·shell·scrollback)

런타임 변경:
  → SettingsView에서 draft 편집 → footer Save로 SettingsEdits 회수
  → App이 원 SettingsEditOwner와 함께 적용·저장하고 MainView에 설정 변경 반영
  → Cancel/일반 close는 Save 실행 초안을 만들지 않음

즉시 반영: font·theme·opacity(렌더러 재초기화/테마 전환), notification·keybindings(매 프레임/이벤트 참조).
새 터미널부터 반영: shell·scrollback (`ShellConfig::from_settings` 가 `runtime_shell` 로 실행 셸을 정하고 그 셸 기준 `shell_args_for` — bash 합성 rc의 --rcfile 주입은 플랫폼별).
```

설정의 첫 창 부팅 저장은 정규화·셸 확인·테마 조정을 마친 snapshot 한 건을 같은 file writer에 제출한다. 로딩 프레임을 계속 그리면서 receipt를 거두고 나서 engine/plugin 초기화를 시작한다. 첫 engine은 저장 결과와 관계없이 이 제출 snapshot으로 시작한다. 워커가 결과를 돌려주지 못해 동기로 다시 만들 때도 같은 값을 쓴다. 추가 창은 저장 중이거나 저장에 실패한 디스크 사본 대신 마지막 제출 설정을 사용한다. 그 창의 GPU와 engine은 보정을 마친 같은 값을 받고, 보정 결과도 같은 큐에 제출한다. 제출 이력이 없을 때만 `config.toml`을 읽는다. engine 생성(`EngineSession::new_with_ids`)은 설정을 인자로 받고 파일을 직접 읽지 않는다.
헤드리스 부팅도 파일을 한 번 읽고 정규화한 snapshot을 engine과 같은 file writer에 전달한다. 잘못된 anomaly 설정 절은 기본값으로 복구하며, 이후 변경 검사는 복구된 실행 설정을 기준으로 한다. 저장 실패는 기존 writer 진단으로 남기고 실행 설정을 되돌리지 않는다.


### 에이전트·plugin 설정 변경 요청의 완료 의미

`settings.set_remote_transfer`, `settings.set_input_rule`·`remove_input_rule`·`initialize_input_rule`, `webhook.config`의 `allow_external`, debug 전용 `debug.settings.apply`는 세 단계를 구분한다.

`debug.settings.apply`가 쓰는 `SettingsChange::Replace`와 전체 교체 처리 분기는 핸들러와 같은 `cfg(debug_assertions)` 조건으로 빌드한다.

- 접수: 핸들러가 인자를 검사하고 바꿀 값만 담은 `DomainIntent::ChangeSettings`를 요청 큐에 넣는다. 인자 오류는 큐에 넣지 않고 바로 `-32602`로 응답한다(`debug.settings.apply`의 `Settings::validate_runtime` 검사 포함: `task_pipeline`·`anomaly_detection`).
- 메모리 적용: 큐 처리에서 `AppServices::apply_live`가 실행 시점의 최신 설정 사본에 그 값만 적용하고 `Settings::validate_runtime()`으로 `task_pipeline`·`anomaly_detection`을 검사한다. 핸들러와 실행 사이에 바뀐 다른 필드는 덮지 않는다. `debug.settings.apply`만 핸들러가 합친 전체 설정으로 바꾼다.
- 저장: 적용 단계는 불변 snapshot을 file writer에 제출한다. writer가 주입된 `SettingsStorage`로 `config.toml`을 교체하고 메인 루프가 receipt 결과를 거둔다.

응답은 메모리 적용과 저장이 모두 끝난 뒤 원 요청에 한 번 간다. 핸들러는 결과를 기다리는 continuation을 지연 응답 기구(`ipc::handler::deferred`)에 맡기고, 네 진입 경로(GUI 외부 IPC, GUI plugin, headless 외부 IPC, headless plugin)는 memory writer 결과와 같은 방식으로 결과가 나온 뒤 응답한다. 결과를 넣으면 이벤트 루프를 깨워 응답을 거둔다. GUI 외부 IPC가 응답 처리 뒤에 큐를 처리해도 응답은 결과를 기다린다.

| 결과 | 응답 |
|---|---|
| 적용·저장 완료 | 성공. `set_remote_transfer`는 적용 뒤의 `remote_transfer`, 입력 규칙은 실행 시점에 정한 `changed` |
| 바꿀 것이 없음 | 성공 `changed: false`. 저장하지 않는다(이미 채운 `initialize_input_rule`) |
| 적용됐지만 저장 실패 | `-32603` "settings applied for this run but not saved: …", `data: {applied: true, saved: false}` |
| 검사·admission·category reset 실패 | `-32603` "settings not applied: …", `data: {applied: false}`. 설정은 그대로다 |
| 더 새 설정 변경에 밀림 | `-32603` "settings change was replaced by a newer change before it finished", `data: {applied: false, superseded: true}` |
| 결과 없이 요청이 버려짐 | `-32603` "settings change was dropped before it finished" |

큐 처리 안의 순서는 다음과 같다.

- GUI 적용: `cascade_settings_updated`가 설정 세대를 하나 올린 뒤 `apply_settings_after_structure`를 실행한다. task report·anomaly 기준 반영 → 창·parked engine 전체의 메모리 설정 교체 → 저장 접수 → 전역 Theme 설치와 창·터미널 색 갱신, plugin `theme.changed`·`language.changed` 발행 순서다. 저장 결과는 후속 회차에 원 요청으로 돌려주고, 실패해도 이미 적용한 설정과 Theme를 되돌리지 않는다.
- GUI category reset: 현재 설정에서 켜져 있던 `workspace_categories_enabled`를 끄는 변경은 모든 engine의 `ResetCategories`를 한 journal batch로 admission하고 그 자리에서 끝난다. 위 적용 단계와 결과 전달은 batch commit 뒤 `finish_settings_results`에서 실행된다. reset이 실패하면 오류를 보고하고 설정은 적용하지 않으며, 기다리는 요청은 거절 결과를 받는다.
  완료가 도착했을 때 그 사이에 다른 설정 변경이 세대를 올렸다면 오래된 완료의 설정 사본은 버리고 기다리는 요청에는 밀림 결과를 준다. 이미 commit된 reset 결과는 유지한다. 대기 중 들어온 최신 설정 의도가 이긴다는 이 동작은 `app::journal::commands::tests::stale_settings_reset_completion_cannot_overwrite_a_newer_settings_intent`가 고정한다.
- headless 적용: `intent::headless::apply_settings`가 task report·anomaly 기준 반영 → engine 메모리 설정 교체 → 저장 접수 순서로 실행하고, 이후 receipt 완료를 거두어 결과를 돌려준다. category reset journal 경로는 없다.

설정 창의 Save는 같은 GUI 적용 경로를 쓰고 응답을 기다리는 요청이 없다. 사용자 출처 변경의 결과는 기존 토스트로 보인다. 검사·admission 실패는 오류 토스트 "설정을 적용하지 못했습니다", 적용됐지만 저장하지 못한 경우는 경고 토스트 "이번 실행에만 적용됐고 저장하지 못해 다시 시작하면 사라집니다"이다. 포커스된 메인 창(없으면 첫 메인 창)에 띄운다. category reset 실패는 위 공통 보고(`report_apply_error`)가 맡는다. 창은 Save 직후 닫힌다.

config snapshot이 같은 묶음에서 대체되면 대체한 저장의 결과로 원 요청을 완료한다. 접수 거절도 적용 후 저장 실패이며, 적용 전 category reset의 Superseded와 구분한다. 다른 파일 저장의 현재 스레드와 종료 순서는 [저장 계약](../design/systems/storage-contract.md)에 있다.

저장소 전반(state.db / config.toml / presets / themes)은 [storage](../design/systems/storage.md), 설정 창 IA 는 [features/settings](../features/settings/index.md).

## 관련

- [action-dispatch](../design/flows/action-dispatch.md) — Intent 큐 디스패치 모델
- [아키텍처 개요](index.md) — App/process · EngineSession/domain/runtime · View 소유와 모듈 배치

## 테마 파일의 첫 렌더 경계

`init_boot_db_and_theme`는 첫 로딩 프레임에 쓸 사용자 테마를 정하기 전에 내장 파일 seed/sync와 읽기 완료를 확인한다. 읽기에 실패한 원본은 보호하며, 요청한 테마를 적용할 수 없으면 내장 Mocha로 계속한다. 추가 창과 runtime 테마 적용은 이 파일 준비를 다시 호출하지 않는다. 캐시/내장 색상만 적용하고, 설정 snapshot 저장은 기존 file writer의 완료 계약을 따른다. 디스크 목록 rescan은 파일 쓰기 없는 읽기다.

## IPC 발견 파일의 공개 순서

`TcpIpcServer::start_with_port_file`은 bind/nonblocking 준비 → 해당 데이터 루트 notify 정리 → `publish_port_file_to`의 안전 교체 완료 → accept worker 시작 → 성공 반환 순서다. 외부 데이터 루트의 port override는 다른 루트 notify를 지우지 않는다. 실패는 시작 호출에 전파하고, 공개 뒤 worker를 시작하지 못하면 자신의 port 결과를 정리한다. 정상 Drop은 같은 참여 잠금을 비차단 획득한 동안 열린 파일 identity와 숫자 내용을 비교한다. 경합이면 남기고, 같은 숫자라도 새 공개 세대면 지우지 않는다. 비참여 외부 편집의 마지막 확인–unlink race는 별도 한계다.
