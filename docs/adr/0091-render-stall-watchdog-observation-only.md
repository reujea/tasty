# ADR-0091: GPU 호출 행(hang)은 관측 워치독으로만 다룬다 — 렌더 스레드 분리는 채택하지 않는다

- **Status**: Accepted
- **Date**: 2026-08-30
- **Tags**: gpu, wgpu, winit, event-loop, hang, watchdog, diagnostics, crash-report, render-thread
- **Group**: window-workspace-lifecycle

## Context

GPU 드라이버 업데이트 도중 tasty 창이 클릭·키입력에 전혀 반응하지 않아 사용자가 작업관리자로 강제 종료한 사건이 있었다. 그 시각의 `~/.tasty/crash-reports/` 에는 아무 파일도 없었다 — panic hook 이 발동하지 않았다는 뜻이고, 따라서 **panic 이 아니라 행(hang)** 이다.

### 1. 이벤트 펌프와 GPU 렌더가 한 스레드에 묶여 있다

`boot.rs` 의 `event_loop.run_app(&mut app)` 은 winit `ApplicationHandler` 콜백(`resumed` / `window_event` / `user_event` / `about_to_wait`)을 **단일 스레드에서 순서대로 동기 호출**한다. `WindowEvent::RedrawRequested` 처리 안에서 GPU 렌더(`GpuState::render`)가 같은 스레드로 돌고, 그 안에 `surface.get_current_texture()` · `queue.submit(...)` · `output.present()` 가 전부 들어 있다.

IPC 도 같은 스레드다. `TcpIpcServer::dispatch_and_await` 는 모든 요청을 메인 스레드로 넘기고 응답 채널을 기다리며, 메인 스레드는 `AppEvent::IpcReady` → `process_ipc()` 로 그것을 처리한다. 즉 렌더가 반환하지 않으면 **키·마우스·IPC 가 전부 함께 멎는다.**

### 2. 실측 — 주장은 재현으로 확인했다

`present()` 직전을 인위적으로 블로킹하는 결함 주입(현재는 `tasty debug gpu-stall`, debug 빌드 전용)으로 측정했다. 지표는 IPC 왕복 시간(`tasty list info`)이다.

| 구간 | IPC 왕복 |
|---|---|
| 정상 | 96 ~ 99 ms |
| 프레임마다 1000 ms 블로킹 중 (8회 연속 측정) | 1001 ~ 1016 ms |
| 블로킹 해제 후 | 97 ~ 98 ms |

지연이 블로킹 길이와 1:1 로 따라붙고, 블로킹 사이의 틈(약 20 ms)에서만 큐가 배출된다. 8000 ms 1회 주입에서는 그 순간의 IPC 호출 하나가 8007 ms 를 흡수했다. **GPU 호출 하나가 안 돌아오면 이벤트 펌프 전체가 그만큼 멎는다**는 것이 실측으로 확인됐다.

### 3. wgpu 24 에서 상한이 있는 호출과 없는 호출

- `get_current_texture()` 는 **상한이 있다.** wgpu-core 가 `FRAME_TIMEOUT_MS = 1000` 을 걸어 acquire 를 호출하고(DX12 는 `WaitForSingleObject(waitable, 1000)`, Vulkan 은 `vkWaitSemaphores` + `vkAcquireNextImageKHR` 각각 1 초), 초과하면 `SurfaceError::Timeout` 으로 **반환된다**.
- `queue.submit(...)` 과 `present()` 에는 **어떤 상한도 없다.** DX12 는 `IDXGISwapChain3::Present`, Vulkan 은 `vkQueuePresentKHR` 를 그대로 호출하고, wgpu 에는 이 호출을 취소하거나 시간 제한을 거는 공개 API 가 없다.

### 4. 행이었다는 증거 — 에러 반환 경로였다면 crash report 가 남았다

`SurfaceTexture::present()` 는 실패 시 wgpu 내부에서 `handle_error_fatal` 로 들어가 **무조건 `panic!`** 한다(에러 스코프·커스텀 핸들러를 타지 않는 경로다). 그리고 tasty 는 `on_uncaptured_error` 도 device-lost 콜백도 설치하지 않으므로, 다른 GPU 에러 역시 wgpu 기본 핸들러(`default_error_handler` → `panic!`)로 떨어진다.

즉 드라이버가 `DXGI_ERROR_DEVICE_REMOVED` 를 **돌려주기만 했다면** tasty 는 panic 했을 것이고, panic hook 이 `crash-reports/` 에 파일을 남겼을 것이다. 남지 않았다 — 따라서 그 호출은 **에러를 반환한 게 아니라 반환 자체를 하지 않았다.**

### 5. Windows TDR 은 이 시나리오를 커버하지 않는다 (조사 완료)

- **TDR 의 트리거는 "로드된 드라이버가 제출한 개별 task"** 다. MS 문서상 GPU 스케줄러(`Dxgkrnl.sys`)가 task 초과를 감지해 preempt 를 시도하고, 그 preempt 대기(기본 2 초)가 실패하면 GPU 가 얼어붙은 것으로 판정해 드라이버를 재초기화한다. 드라이버 언로드→재로드 과정 자체를 다루는 메커니즘이 아니다.
- **드라이버 업그레이드는 별개 경로**이고, MS 는 그것을 "그래픽 어댑터 가용성 변화"로 분류하면서 `The graphics driver is upgraded.` 를 명시적 사례로 든다. 이때 앱이 기대해야 하는 동작은 **`Present` 가 `DXGI_ERROR_DEVICE_REMOVED` 를 반환하는 것**이다(`The graphics device has been physically removed, turned off, or a driver upgrade has occurred.`).

정리하면 "TDR 이 드라이버 재설치를 커버하지 않는다"는 맞지만, "그래서 DXGI 호출이 블로킹한다"는 **MS 문서가 뒷받침하지 않는다** — 문서화된 기대 동작은 에러 반환이다. 그럼에도 §4 가 보여주듯 실제로는 반환이 없었으므로, 이 사건은 **문서화된 기대 동작을 벗어난 드라이버/OS 상태**였다고 볼 수밖에 없다. 그 부분은 tasty 가 통제할 수 없는 외부 요인이다.

### 6. 그래서 남는 문제

tasty 가 통제할 수 있는 것은 "그런 일이 벌어졌을 때 무엇이 남는가" 뿐이다. 현재는 **아무것도 남지 않는다.** `redraw.rs` 의 `SurfaceError` 분기는 전부 "호출이 반환됨"을 전제하므로 행 상황에는 도달하지 않고, 반복 에러를 panic 으로 승격시키는 `crash_report::record_error` 는 애초에 debug 전용(release no-op)이다.

## Decision

**이벤트 루프 stall 을 별도 스레드에서 관측해 기록만 하는 워치독(`platform::stall_watchdog`)을 release 포함 전 빌드에 둔다. 렌더를 별도 스레드로 분리하지 않고, GPU 호출에 애플리케이션 레벨 타임아웃도 씌우지 않으며, 워치독이 프로세스를 자동 종료하지도 않는다.**

- **관측 지점**: 네 winit 콜백 진입/이탈을 RAII 가드로 표시한다(`resumed` / `window_event` / `redraw` / `user_event` / `about_to_wait` — `RedrawRequested` 는 GPU 가 도는 경로라 따로 구분한다). 메인 창 렌더 경로는 `acquire` / `submit` / `present` 세부 단계까지 표시한다.
- **판정**: 콜백이 **5 초** 안에 반환하지 않으면 보고하고, 같은 stall 이 이어지면 **30 초**마다 재보고한다. 정상 프레임은 수 ms 이고 slow-render 경고선조차 30 ms 라, 5 초는 "느린 프레임"으로 설명되지 않는 영역이다.
- **기록처 2 곳**: `tracing::error!(target: "tasty::stall")` (stderr + `~/.tasty/debug.log`) 과 **`~/.tasty/crash-reports/hang-<ts>.log`** (stall 당 1 개). 후자를 따로 쓰는 이유는 ① 사용자가 실제로 확인하는 곳이 거기이고, ② 공유 로그는 프로세스 시작마다 truncate 되므로 행 상태에서 `tasty` CLI 가 한 번이라도 실행되면 지워지기 때문이다.
- **복구는 하지 않는다.** 워치독은 상태를 읽고 쓰기만 하며, 렌더·입력·프로세스 수명에 관여하지 않는다.
- **재현 수단**: `debug.gpu.stall` IPC (`tasty debug gpu-stall --ms N`, debug 빌드 전용)가 다음 프레임의 `present` 직전을 1 회 블로킹한다. 실제 드라이버 행을 결정적으로 재현할 수 없으므로, 같은 구조를 인위적으로 만들어 워치독을 검증하는 용도다.

### 보강 — 의도적 블로킹 구간은 보고 대상에서 뺀다 (구현 확정)

메인 스레드에서 **동기로** 열리는 native 모달은 사용자가 선택을 마칠 때까지 콜백을 붙잡는다. tasty 의 파일·폴더 선택(`rfd::FileDialog`)이 그렇다 — macOS 의 "native 다이얼로그는 메인 스레드" 요구 때문에 의도적으로 그렇게 설계돼 있다(`adapters/ipc/handler/fs.rs`). 이건 설계된 동작이지 행이 아닌데, 위 판정만으로는 구분되지 않는다: 사용자가 파일 선택에 5 초를 쓰면 `crash-reports/` 에 `site=user_event` / `phase=none` 리포트가 남는다(실측 확인).

그러면 **이 결정의 전제가 무너진다.** 워치독을 둔 이유는 "`crash-reports/` 가 비어 있으면 아무 일도 없었던 것" 이라는 판독을 성립시키기 위함인데, 일상 조작이 리포트를 만들면 반대 방향 오독("파일이 있어도 행이 아니다")이 생겨 디렉토리 자체가 신호를 잃는다.

그래서 `stall_watchdog::without_stall_watch(f)` 로 그런 구간을 감싼다. 구간 안에서는 보고하지 않고, 구간을 벗어나면 진입 시각을 다시 잡아 모달에 쓴 시간이 stall 로 누적되지 않게 한다. 현재 적용 지점은 `rfd::FileDialog` 호출 4 곳(설정 scripts / remote-transfer · 프리셋 필드 · 플러그인 추가)이다 — 전부 **사용자가 버튼을 눌러 여는** 경로다(IPC 로 열던 `fs.pick_file` 은 ADR-0162 로 제거됐다).

**메인 스레드를 막는 동기 호출을 새로 추가하면 이 래퍼를 통과시킨다.** 빠뜨리면 오탐이 그대로 `crash-reports/` 에 쌓인다.

## Consequences

- **얻은 것**: 재발 시 `crash-reports/hang-*.log` 에 "어느 콜백의 어느 GPU 단계에서 몇 초째 멎었는가"가 남는다. 이 사건에서 원인 규명을 막았던 "증거가 하나도 없다"는 상태가 해소된다. `phase` 가 `present`/`submit`/`acquire` 로 찍히면 tasty 로직이 아니라 드라이버 쪽이라는 판단이 즉시 선다.
- **잃은 것**: 행 자체는 그대로다. 워치독은 응답성을 되돌려주지 않으며, 사용자는 여전히 프로세스를 강제 종료해야 한다. `present`/`submit` 이 반환하지 않는 한 화면도 갱신되지 않는다.
- **기록의 해상도 한계**: 파일 리포트는 stall 당 1 개이고, 거기 실리는 `Stuck for` 는 **최초 탐지 시점(≈5~6 초)** 의 값이다. 총 지속 시간이 들어간 30 초 주기 재보고는 `tracing` 으로만 나가는데 그 로그 파일은 프로세스 시작마다 truncate 되므로, 행 중에 `tasty` CLI 가 한 번이라도 실행되면 사라진다. 결과적으로 **살아남는 증거로는 "6 초 멎었다" 와 "6 시간 멎었다" 를 구분할 수 없다** — 행의 발생과 위치는 남지만 길이는 남지 않는다. 파일당 1 개를 유지하는 이유는 반대쪽(30 초마다 파일 생성)이 `crash-reports/` 를 리포트로 채워 디렉토리 자체를 못 쓰게 만들기 때문이다.
- **운영 비용 / 유지 부담**: 스레드 1 개(1 초 주기 폴링)와 콜백당 원자 연산 3~4 회. `Instant` 가 절전 시간을 포함하는 플랫폼(Windows)에서는 절전 복귀 직후 한 번 오탐 로그가 남을 수 있다 — 로그·파일 기록뿐이라 무해하지만, `crash-reports/` 에 무의미한 `hang-*.log` 가 섞일 수 있다. 콜백이 중첩 호출되는 플랫폼 상황이 있으면 시퀀스 홀짝이 깨져 **탐지를 놓치는 쪽**으로 실패한다(오탐이 아니라 미탐 — 안전한 방향).

## Alternatives Considered

- **A. 아무것도 하지 않는다** — 발생 빈도가 낮고 원인이 외부(드라이버)라는 점에서 합리적으로 보이지만, 그러면 재발 시에도 증거가 남지 않아 지금과 똑같은 자리로 돌아온다. 이 결정이 필요해진 이유 자체가 "증거가 없었다"이므로 기각.
- **B. 렌더를 별도 스레드로 분리한다** — 세 가지 이유로 기각.
  1. **분리 경계가 렌더러 전체다.** `acquire → 렌더 패스 기록 → submit → present` 는 한 덩어리라 쪼갤 수 없고, egui 는 `take_egui_input(window)` / `handle_platform_output(window, …)` 로 창과 직접 결합돼 있어 프레임 준비 단계도 같이 넘어가야 한다.
  2. **macOS 에서 결합이 되살아난다.** winit `Window` 는 `Send + Sync` 지만, macOS·iOS·Web 에서는 비메인 스레드 호출이 메인 스레드로 스케줄되고 **호출 스레드가 그동안 블록된다.** 렌더 스레드가 창을 만지는 순간 메인 스레드와 다시 묶인다.
  3. **얻는 것이 제한적이다.** 스레드를 나눠도 마지막 프레임이 화면에 남으므로 사용자에게는 여전히 "얼어붙은 창"이고, 종료 화면조차 렌더가 필요하다([ADR-0077](0077-shutdown-loading-screen.md)). 비용은 아키텍처 전면, 이득은 "보이지 않는 응답성"이라 균형이 맞지 않는다.
- **C. GPU 호출에 애플리케이션 레벨 타임아웃을 씌운다** — wgpu 24 에 취소·타임아웃 API 가 없다. 상한이 필요한 `submit`/`present` 에는 걸 방법이 아예 없고, 유일하게 상한이 있는 `get_current_texture` 는 이미 wgpu-core 가 1 초를 걸고 있다. "다른 스레드에서 호출하고 호출자가 타임아웃한다"는 변형은 결국 B 이며, 게다가 그 스레드는 영구히 누수된다. 기각.
- **D. 워치독이 임계 초과 시 프로세스를 강제 종료한다** — 사용자가 어차피 강제 종료한다는 점에서 매력적으로 보이지만, 살아 있는 PTY 세션을 전부 죽인다. 절전 복귀·디버거 정지 같은 오탐 한 번이면 사용자 작업이 날아간다. 관측 전용이 안전한 계층이고, 자동 종료는 오탐률을 실측으로 확인한 뒤에 논할 문제다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **행이 실제로 반복 관측될 때**: `hang-*.log` 가 실사용에서 누적되고 빈도가 무시할 수 없으면, 대안 B(렌더 스레드 분리)의 비용/이득 균형이 바뀐다.
- **wgpu 가 취소·타임아웃 API 를 제공할 때**: `present`/`submit` 에 상한을 걸 수 있게 되면 대안 C 가 실행 가능해진다.
- **`SurfaceError::Timeout` 이 실사용에서 반복될 때**: 현재 이 변형은 `redraw.rs` 의 catch-all 분기에 떨어져 경고 후 프레임을 포기한다(재시도하지 않는다). 반복 관측되면 전용 처리(재시도/디바이스 재생성)를 검토한다.
- **오탐이 `crash-reports/` 를 오염시킬 때**: 절전 복귀 등으로 무의미한 `hang-*.log` 가 쌓이면 임계값 조정 또는 플랫폼별 절전 시간 보정을 검토한다.
- **워치독이 정작 필요할 때 기록에 실패할 때**: 행 중에는 디스크 I/O 도 영향을 받을 수 있다. 리포트가 비어 있거나 잘린 사례가 나오면 기록 방식(사전 할당·mmap 등)을 재검토한다.

## References

- 구현: `crates/tasty-platform/src/stall_watchdog.rs`, `crates/tasty-platform/src/crash_report.rs`(`write_hang_report`), `src/app/event_handler.rs`(콜백 가드), `src/gfx/gpu.rs`(렌더 단계 표시).
- 관련 문서: [`dev-guide/gpu-rendering.md`](../dev-guide/gpu-rendering.md)(렌더가 이벤트 스레드에서 도는 구조), [`dev-guide/debug-ipc.md`](../dev-guide/debug-ipc.md)(`debug.gpu.stall`), [`dev-guide/error-handling.md`](../dev-guide/error-handling.md).
- 관련 ADR: [ADR-0077](0077-shutdown-loading-screen.md)(종료 화면도 렌더를 요구한다).
- 관련 ADR: [ADR-0092](0092-file-log-host-process-only.md) — 공유 로그 파일을 host 프로세스로 한정한 결정. **본 ADR 본문의 "공유 로그는 프로세스 시작마다 truncate 되므로 행 상태에서 `tasty` CLI 가 한 번이라도 실행되면 지워진다" 는 서술(Decision·Consequences)은 0092 이후 더 이상 성립하지 않는다** — CLI 실행은 공유 로그를 건드리지 않고, truncate 는 host 가 뜰 때만 일어난다. `hang-*.log` 를 별도 파일로 남기는 결정 자체는 나머지 근거("사용자가 실제로 들여다보는 곳이 `crash-reports/` 다")로 그대로 유효하다. 현행 서술은 [`dev-guide/crash-diagnostics.md`](../dev-guide/crash-diagnostics.md) 를 본다.
- 외부 자료:
  - [WDDM Support for Timeout Detection and Recovery (TDR)](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/timeout-detection-and-recovery) — TDR 의 트리거가 "GPU 스케줄러가 감지한 task 초과 + preempt 대기(기본 2 초) 실패"임.
  - [Handle device removed scenarios in Direct3D 11](https://learn.microsoft.com/en-us/windows/uwp/gaming/handling-device-lost-scenarios) — `The graphics driver is upgraded.` 를 어댑터 가용성 변화 사례로 명시하고, 앱은 `Present` 반환값에서 `DXGI_ERROR_DEVICE_REMOVED` 를 검사하라고 규정.
