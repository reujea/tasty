# 크래시 & 에러 진단

tasty 가 죽거나 멈췄을 때 무엇이 어디에 기록되는지, 빌드 모드(release / dev)에 따라 어떤 추가 정보를 어디서 얻는지를 정리한다. 메커니즘은 `crates/tasty-platform/src/crash_report.rs`, 부팅 1단계(`boot.rs` → `os::init_crash_report` → `crash_report::init`)에서 설치된다(공유 로그 **파일**만 host 확정 후 `os::enable_host_file_log` 에서 열린다 — 아래 "파일 로그를 쓰는 프로세스는 host 뿐이다").

## panic 발생 시 (모든 빌드)

부팅 때 설치된 panic hook 이 panic 을 잡아 **두 곳**에 남긴다 (release·dev·dist 공통, panic 발생 시 기록):

1. **crash report 파일** — `~/.tasty/crash-reports/crash-<YYYY-MM-DDTHH-MM-SS>-p<PID>-<패닉 순번 10자리>.log`. 패닉마다 새 파일을 만들고 이미 있는 파일은 비우거나 덮어쓰지 않는다. PID 재사용이나 시계 역행으로 이름이 겹치면 끝에 `-<번호>`를 붙인 다른 이름을 쓴다.
2. **stderr** — `Tasty crashed! Report saved to: <경로>` + `panic: <info>` + 전체 backtrace

### crash report 내용

```
=== Tasty Crash Report ===
Timestamp: 2026:06:17 19:23:07
Version: 0.8.4                      ← 본체(tasty) 버전 — `os::init_crash_report` 가 본체 크레이트의 CARGO_PKG_VERSION 을 `crash_report::init` 에 넘긴다
OS: macos aarch64                  ← std::env::consts::OS / ARCH

=== Process ===
PID: 41200
Panic sequence: 2                  ← 이 프로세스에서 몇 번째 패닉인가
First panic location: src/view/main/redraw.rs:98   ← 이 프로세스의 첫 패닉. 순번이 1이면 이 두 줄은 없다
First panic message: index out of bounds: ...

=== Panic ===
Location: src/view/main/redraw.rs:142   ← info.location() (있을 때)
Message: called `Result::unwrap()` on an `Err` value: ...
Display: <panic 의 Display 표현>

=== Backtrace ===
   0: std::backtrace::Backtrace::force_capture
   ...
```

backtrace 는 `Backtrace::force_capture()` 로 항상 수집되므로 `RUST_BACKTRACE` 설정 없이도 채워진다.

```bash
ls ~/.tasty/crash-reports/
cat ~/.tasty/crash-reports/crash-*.log
```

### 패닉이 연달아 날 때

한 프로세스에서 패닉이 이어지면 보고서도 여러 개 생긴다. 예를 들어 해제 중 소멸자가 다시 패닉하면 프로세스는 실패로 끝나고, 남는 파일 수는 OS마다 다르다. Linux와 macOS CI에서는 원인 패닉, 소멸자 패닉, 언어 런타임의 `panic in a destructor during cleanup` 순서로 파일 3개가 남았다. Windows(`x86_64-pc-windows-msvc`) CI에서는 원인 패닉, 소멸자 패닉 순서로 파일 2개가 남았다. 어느 쪽이든 마지막 파일의 `Message:`는 원인이 아니다.

- **무엇을 먼저 읽는가**: 어느 파일이든 `First panic location:`과 `First panic message:`가 그 프로세스의 첫 패닉이다. `Panic sequence: 1`인 파일은 그 패닉 자체가 첫 패닉이라 이 줄이 없고 `=== Panic ===` 절을 읽는다. 같은 프로세스의 파일은 이름의 `p<PID>`가 같다.
- **순서**: 패닉이 한 스레드에서 이어지면 이름순이 패닉 순서다. 여러 스레드가 동시에 패닉해 초 경계에 걸리면 이름순과 어긋날 수 있으므로 `Panic sequence:`를 기준으로 삼는다. 프로세스 사이의 순서와 시계가 뒤로 간 때의 순서는 이름으로 알 수 없다.
- **첫 패닉의 뜻**: 훅에 먼저 들어온 패닉이다. 다른 스레드와 동시에 패닉하면 첫 패닉이 인과상의 원인이라는 보장은 없다.
- **생성률 제한**: 프로세스 하나가 초마다 만드는 보고서는 4개까지다(`REPORT_FILES_PER_SECOND`). 회복되는 패닉이 폭주할 때 파일이 늘어나는 속도를 제한할 뿐 디스크 사용량의 상한은 아니다. 폭주가 이어지면 파일은 계속 쌓인다. 넘친 패닉은 `Report saved` 줄 없이 stderr에만 남는다. 첫 패닉은 제한과 무관하게 쓴다. 프로세스 전체 개수에는 상한이 없고 자동 삭제도 없다.
- **제한의 한계**: 회복되는 패닉이 폭주하는 같은 초에 치명 패닉이 나면 그 패닉은 파일에서 빠질 수 있다. 이때도 남은 파일에 그 프로세스의 첫 패닉 요약이 있고 치명 패닉은 stderr에 나간다. 다만 stderr를 받을 곳이 없으면 치명 패닉의 위치와 메시지는 어디에도 남지 않는다. Windows release GUI는 `windows_subsystem = "windows"`라 콘솔에서 띄우지 않으면 받을 곳이 없고, 앱 번들로 띄운 macOS도 사용자가 보기 어렵다.

## tracing 로그 — `TASTY_LOG`

런타임 로그 레벨은 **`TASTY_LOG`** 환경변수로 제어한다(`RUST_LOG` 아님). 미설정 시 기본 필터: `warn,wgpu_hal=error,wgpu_core=error,naga=error,egui_winit::clipboard=off`.

```bash
TASTY_LOG=debug tasty 2>/tmp/tasty.log                          # 전체 debug
TASTY_LOG=tasty::adapters::ipc=debug,tasty::boot=debug tasty 2>/tmp/tasty.log  # 모듈별
```

tracing 출력처는 **모든 빌드 모드에서 stderr + 파일**이다. 파일 필터는 `TASTY_LOG`(stderr 필터)와 독립적으로 고정된다:

| 빌드 | tracing 출력 | 파일 경로 | 파일 필터 |
|------|--------------|-----------|-----------|
| **release / dist** | stderr + 파일 | `~/.tasty/debug.log` | `warn,wgpu_hal=warn,wgpu_core=warn,naga=warn` |
| **dev** | stderr + 파일 | `~/.tasty-debug/debug-dev.log` | `debug,wgpu_hal=warn,wgpu_core=warn,naga=warn` |

release 파일 필터가 `warn` 이상으로 제한된 이유는 상시 전체 debug 로깅에 따른 디스크 사용량을 피하면서도, attach disconnect 같은 진단 가치가 있는 로그는 release 실사용자 환경에서도 사후 확인 가능하게 하기 위함이다(dist 도 동일). 두 파일 모두 ANSI 없음, **host 프로세스가 뜰 때마다 새로 truncate**(rotation 없음). 디렉토리 생성/파일 생성 실패 시 stderr-only 로 자동 폴백.

### 파일 로그를 쓰는 프로세스는 host 뿐이다

`tasty` 바이너리는 GUI(host)와 CLI 클라이언트를 겸한다. **파일 로그는 host(GUI / headless)만 연다** — `tasty list info` 같은 CLI 서브커맨드는 stderr 로만 로깅하고 공유 로그 파일을 건드리지 않는다. 그래서 에이전트가 CLI 를 아무리 자주 호출해도 실행 중인 host 의 로그는 그대로 남는다. 근거·대안은 [CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md).

- **CLI 프로세스의 진단**은 stderr(대화형 실패는 사용자가 즉시 본다)와, agent hook 전달 실패 전용 append-only 기록 `$TASTY_HOME/hook-failures.log` 로 한다. 공유 로그에서 찾지 않는다.
- **CLI 의 stderr 로그 수준도 `TASTY_LOG` 를 따르고 기본은 `warn` 이다.** 그래서 `tasty claude hook`·`tasty agent task-submit` 같은 에이전트용 명령은 평소 stderr 에 성공 출력 외 줄을 내지 않는다(설정·번역 적재 같은 `info` 줄은 숨는다). `TASTY_LOG` 는 터미널·runner 자식 환경에서 지우는 변수가 아니므로, host 를 `TASTY_LOG=info` 등으로 띄우면 그 값이 터미널 셸과 그 안의 에이전트·후처리 프로세스로 상속돼 CLI 도 같은 수준으로 stderr 에 쓴다. stderr 를 오류로 읽는 도구가 터미널 안에 있으면 host 를 `TASTY_LOG` 없이 띄우거나 그 셸에서 변수를 지운다.
- 파일에 남는 host 로그는 여전히 **직전 host 실행분**이다(host 가 뜰 때 truncate). tasty 를 재시작하면 이전 실행의 로그는 사라지므로, 재시작을 넘겨 보존해야 하는 증거는 전용 파일(`crash-*.log` / `hang-*.log` / `hook-failures.log`)로 남긴다.

```bash
cargo run                          # dev (host)
cat ~/.tasty-debug/debug-dev.log   # 직전 host 실행의 전체 debug 로그

cargo build --release && ./target/release/tasty
cat ~/.tasty/debug.log             # 직전 host 실행의 warn 이상 로그
```

### webview surface 가 비어 보일 때

markdown / html surface 는 mesh 를 그리지 않고 native webview overlay 로만 보인다. 그 자리가 비면(검은 사각형, 또는 host chrome 만) 어느 단계에서 멈췄는지는 다음 줄들로 가른다.

| 단계 | 남는 곳 | 레벨 |
|------|---------|------|
| plugin 이 HTML 을 실었는가 | `plugins-logs/<plugin id>.log` — `markdown surface <id>: loaded N bytes of HTML` · 실패 시 `webview.set_url failed` / `no document registered` / `theme.query failed` / `malformed theme.query response` / `no host handle yet` | info / warn |
| host 가 그 URL 을 받았는가 | 호스트 로그 — `webview.set_url: surface <id> not found ...` / `... is not a webview-enabled RemoteSurface` (성공 시 조용) | warn |
| native webview 가 만들어졌는가 | 호스트 로그 — `WebView surface <id>: created (visible=…, bounds=…, url=…)` · 실패 시 `Failed to create WebView for surface <id>` / `Giving up on the WebView ...`, 만들어졌지만 실을 URL 이 없으면 `WebView surface <id>: created without a URL; nothing will be loaded` | debug / warn |
| 페이지가 로드됐는가 | 호스트 로그 — `WebView surface <id>: load started` / `load finished`, 실패 시 백엔드별로 `WKWebView navigation failed` · `WKWebView provisional navigation failed`(macOS) / `WebView2 navigation failed`(Windows) / `WebKitGTK load-failed`(Linux), Linux 는 `WebKit web process terminated` 도. 로드 호출 자체가 실패하는 경우는 반환값이 있는 Windows 뿐이고 그때는 `WebView2 Navigate failed` / `WebView2 NavigateToString failed` — 이 둘은 surface id 를 안 싣는다 | debug / warn |
| 로드가 안 끝나 안 보이는가 | 호스트 로그 — `WebView surface <id>: still hidden ... (nav_state=…)` — 드러나야 할 자리에 놓였는데 nav 가 `Done` 이 아닌 채로 이어질 때 surface 당 한 번 | warn |
| 로드는 끝났는데 안 보이는가 | 호스트 로그 — Linux 에서 부모 창 밖에 그려지는 경우 `WebView surface <id>: GTK window realized without a GDK window` (navigation 은 정상 완료하므로 위 보류 줄은 안 남는다). Windows 의 배치·표시 API 실패는 `WebView2 SetBounds failed` / `SetWindowPos failed` / `WebView2 SetIsVisible failed` — 이 세 줄은 surface id 를 안 싣고, navigation 완료 여부와 독립적으로 발생한다 | warn |
| 위가 다 성공했는데 자리를 못 채우는가 (Linux) | **해당 크기를 남기는 로그가 없다.** `xwininfo` 로 직접 잰다 (아래 "렌더 타깃 크기를 재는 법") | 없음 |

보류 줄(`still hidden`)의 판정은 **redraw 안에서** 일어난다 — 매 프레임 "드러나야 할 자리에 있는데 nav 가 `Done` 이 아닌" surface 를 모아 시간을 재는 방식이다. 그래서 이 줄이 보장하는 범위는 **프레임이 도는 동안까지**다: 렌더 루프가 그 지점에 닿지 못하면(프레임이 아예 안 돌면) 자리가 비어 있어도 warn 은 침묵한다. 그때 비어 있음의 흔적은 이 표가 아니라 hang 진단(`hang-*.log`) 쪽에 남는다.

성공 줄이 `debug` 라 **release 파일 로그(`warn` 이상)에는 실패·보류 줄만 남는다.** 단계별 성공까지 보려면 dev 빌드(`debug-dev.log`)나 `TASTY_LOG=debug` 로 stderr 를 받는다.

#### 렌더 타깃 크기를 재는 법 (Linux)

내용이 보이는데 **잘려 있거나 여백이 남으면** 담는 창과 그리는 창의 크기가 갈린 것이다.
Linux 에서 담는 것은 host 창 안의 X11 자식창이고, 그리는 것은 그 안에 GTK/WebKit 이 만든
GdkWindow 다. 둘은 별개 창이라 `xwininfo` 의 트리로 바로 비교된다 — 이 갈림은 어떤 로그
줄로도 안 드러나므로 **재지 않으면 알 수 없다.**

```bash
xwininfo -id <host 창 xid> -tree   # 자식(X11) 아래 손자(GDK) 의 Width/Height 를 본다
```

host 창의 xid 는 `xdotool search --pid <tasty pid>` 로 찾는다(`IsViewable` 인 것). 두 줄의
`WxH` 가 **같아야** 정상이다 — 다르면 `PlatformWebView::set_bounds` 가 렌더 타깃에 크기를
전달하지 못한 것이다. 그 전달이 왜 별도 호출이어야 하는지는
[WebView 호스트 계약](../design/systems/webview.md).

## 에러 루프 자동 감지 (dev 전용)

GUI dev 빌드는 **`ErrorLoopDetector`** 를 가진다 — 같은 에러 메시지가 **1초 내 100회 이상** 반복되면 의도적으로 panic 을 발생시켜 crash report 로 떨군다. 무한 에러 루프(GPU 재시도 폭주 등)를 영원히 도는 대신 즉시 멈춰 흔적을 남기기 위함.

```
Error loop detected! The following error repeated 100 times in 1s:
<반복된 에러 메시지>
```

이 감지기는 GUI 렌더·이벤트 루프 전용으로 컴파일된다. headless·CLI도 panic 리포터와
host 역할의 파일 로그는 공용 부팅 경로에서 설치한다.

호출 지점은 재발 가능성이 높은 루프 — 렌더 루프(`src/view/main/redraw.rs`, 예: "GPU out of memory")와 이벤트 루프(`src/app/event_handler.rs`). 호출 API 는 `crash_report::record_error(msg)` 이며 **GUI release 에서는 no-op**(`#[inline(always)]` 빈 함수)이라 비용·동작이 없다.

## 빌드 모드별 차이 요약

| | release / dist | dev |
|---|---|---|
| crash report 파일 | ✅ | ✅ |
| stderr panic + backtrace | ✅ | ✅ |
| 파일 tracing | ✅ `debug.log`(warn 이상) | ✅ `debug-dev.log`(전체 debug) |
| 에러 루프 자동 감지 | GUI는 no-op; headless는 제외 | GUI에서만 동작 |
| 심볼 / backtrace 품질 | `strip = true` 라 함수명 제한 → 주소만 보일 수 있음 | 미최적화·전 심볼 → 정확한 스택트레이스 |

**release 에서 backtrace 가 주소만 나올 때**: `RUST_BACKTRACE=full tasty` 로 강화하되, strip 된 상태에선 한계가 있다 — 정확한 함수명이 필요하면 dev 빌드로 재현한다.

## panic 없이 멈춤 (이벤트 루프 stall / 무한루프 / 데드락)

panic 이 아니라 hang 이면 **panic hook 이 발동하지 않으므로 `crash-*.log` 는 생기지 않는다.** 다만 winit 콜백이 반환하지 않거나, 처리할 IPC 작업이 큐에 남은 채 콜백 바깥에서 진행이 멎은 경우는 워치독이 별도로 잡아 `hang-*.log` 를 남긴다.

### 이벤트 루프 stall — `hang-*.log` (모든 빌드, 자동)

콜백(`resumed` / `window_event` / `redraw` / `user_event` / `about_to_wait` / `device_event` / `new_events`)이 **5 초** 안에 반환하지 않으면 독립 워치독이 `~/.tasty/crash-reports/hang-<YYYY-MM-DDTHH-MM-SS>.log`를 남긴다. 콜백 바깥에서도 아직 메인 루프가 꺼내야 할 IPC 명령이 있고 관측한 진행이 5초 동안 없으면 `outside_callback`으로 보고한다. 이때는 마지막 콜백 진행과 연속 pending 기간 시작 중 더 늦은 시각부터 잰다. 정상 idle에는 pending이 없으므로 시간만으로 보고하지 않는다.

콜백이 모두 짧게 끝나도 한 iteration이 `about_to_wait`에 닿지 못한 채 5초가 지나면 `starved_iteration`으로 보고한다. winit은 사용자 이벤트 큐가 빌 때까지 `about_to_wait`로 넘어가지 않으므로, 이벤트가 처리 속도보다 빨리 쌓이면 콜백 하나의 시간으로는 보이지 않는 정지가 생긴다. 시간은 직전 `about_to_wait`가 끝난 뒤 처음 들어온 콜백부터 잰다. `Stuck for`는 그 시간이고 `Last callback`은 표본 시점의 마지막 콜백이다. 이벤트 원천을 좁히려면 아래의 외부 도구로 붙는다. winit이 `about_to_wait`를 부르지 않는 OS 모달 루프(창 이동·live resize 등)가 5초를 넘으면 이 보고가 나올 수 있다. 실제 OS에서 그 경우를 측정하지 않았다.

```
=== Tasty Hang Report ===
Timestamp: 2026:08:30 01:24:09
Version: 0.10.2
OS: linux aarch64

=== Stall ===
Callback: redraw                    ← 어느 winit 콜백이 안 돌아왔나
Render phase: present               ← none / acquire / submit / present / configure
Stuck for: 5745 ms                  ← 최초 탐지 시점 기준
Inside callback: true
Pending main-loop work: 1
Last progress age: 5745 ms
Pending period age: 5745 ms
Last callback: redraw
Last recorded render phase: present (5745 ms ago)
```

- `Render phase`는 현재 계측 구간이다. `acquire`/`submit`/`present`/`configure`도 드라이버 원인을 확정하는 증거는 아니다. 콜백 바깥에서는 `none`이며 `Last callback`과 `Last recorded render phase`는 나이를 함께 표시한 과거 관측이다. 이를 현재 GPU/IME 호출의 스택으로 해석하지 않는다.
- pending은 응답을 기다리는 호출자 수가 아니라 성공적으로 큐에 게시된 메인 루프 처리 의무다. send 성공 뒤 기존 wake 전에 시작하며 dequeue·claim·실행 전 철회·명령 drop·확인한 응답으로 한 번만 끝난다. 이미 시작한 비동기 작업이나 늦은 응답 소유권은 pending을 유지하지 않는다. 기한 없이 보낸 명령은 호출자가 떠나도 실제 dequeue/drop까지 남는다. pending 나이는 개별 최장 요청이 아니라 큐 의무가 연속해서 남아 있는 기간이다.
- probe는 고정 메타데이터만 읽고 요청을 소비하거나 저장소를 조회하지 않는다. 요청 본문·URL·인증 정보는 보고하지 않는다. 호스트 주입 대기 상한은 `tasty::ipc_wait`의 숫자 request_seq·timeout_ms 및 기존 slow-request 결과와 함께 대조한다. 대기 상한은 실행 취소나 완료 확인을 뜻하지 않는다. 소켓·호스트 주입의 기존 미실행/결과 불명/늦은 응답 계약은 유지한다.
- **파일은 stall 당 1 개**이고 `Stuck for` 는 최초 탐지 시점(≈5~6 초)의 값이다. 총 지속 시간이 실린 30 초 주기 재보고는 `tracing` 으로만 나가는데(`target: "tasty::stall"`), 그 로그 파일은 host 가 뜰 때마다 truncate 되므로 행을 겪고 강제 종료 후 다시 띄우면 사라진다. 즉 **재시작을 넘겨 남는 증거는 "어디서 멎었나" 까지이고 "얼마나 오래 멎었나" 는 아니다**(행이 진행되는 동안에는 파일에서 읽을 수 있다 — CLI 실행이 지우지는 않는다).
- **워치독은 복구하지 않는다.** 기록만 남기며 프로세스를 종료하지도, 응답성을 되돌리지도 않는다 — 사용자는 여전히 강제 종료해야 한다. 근거·대안(렌더 스레드 분리 / 자동 종료)의 기각 사유는 [종료 시퀀스 — 종료 cascade + Drop tail](../architecture/shutdown-sequence.md).
- native 파일 다이얼로그처럼 **의도적으로** 메인 스레드를 막는 구간은 pending 여부와 관계없이 보고 대상에서 빠지고, 구간 종료 뒤부터 다시 잰다(`stall_watchdog::without_stall_watch`) — 그런 리포트가 섞이면 이 디렉토리가 신호를 잃기 때문이다.
- debug 빌드에서는 `tasty debug gpu-stall --ms N` 으로 재현할 수 있다(다음 프레임의 `present` 직전을 1 회 블로킹).

### 그 밖의 hang (무한루프 / 데드락)

콜백 바깥에 처리할 IPC 작업이 없으면 이 watchdog만으로 정지를 구별할 수 없다. `outside_callback` 또는 `Render phase: none`도 원인 위치를 확정하지 않으므로 외부 도구로 붙는다. 별도 main-loop probe나 주기적 wake를 추가하지 않으며 실제 X11/IME/RDP 재연결 경로는 주입 시험과 구분한다:

```bash
# 멈춘 프로세스
gdb -p <직접 실행했거나 사용자가 지정한 PID>
(gdb) thread apply all bt        # 모든 스레드 backtrace
(gdb) thread 3
(gdb) bt full
# 또는 syscall 추적
strace -p <PID> -f -e trace=write,read 2>/tmp/tasty-strace.log
```

- **데드락**: lock 을 잡고 대기하는 지점이 backtrace 에 보인다.
- **무한루프**: 반복되는 호출 패턴이 backtrace 에 보인다.

dev 빌드가 심볼이 온전해 위치 파악이 쉽다.

## 진단 파일 위치 요약

| 파일 / 경로 | 빌드 | 내용 |
|-------------|------|------|
| `~/.tasty/crash-reports/crash-*.log` | 모두 | panic 시 자동, 패닉마다 새 파일(버전·OS·PID·순번·첫 패닉 요약·위치·메시지·backtrace). 프로세스당 초당 4개까지 |
| `~/.tasty/crash-reports/hang-*.log` | 모두 | 이벤트 루프 stall 시 자동(콜백 안팎·pending·진행 나이·기록된 GPU 구간). stall 당 1 개, 복구는 하지 않음 |
| `~/.tasty-debug/debug-dev.log` | dev 만 | 전체 debug tracing(host 프로세스 전용, host 시작 시 truncate) |
| `~/.tasty/debug.log` | release / dist 만 | warn 이상 tracing(host 프로세스 전용, host 시작 시 truncate) |
| `~/.tasty/hook-failures.log` | 모두 | agent hook 전달 실패(CLI 프로세스가 tasty 에 닿지 못한 기록). append-only + 256KB 회전. 한 줄은 `<UTC> method=… event=… surface=… code=… reason=…`. **로케일 무관성은 좌표 필드(`method`/`event`/`surface`/`code`)가 지고 `reason` 산문은 그 문구를 만든 쪽의 언어를 따른다**([CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md)). CLI 가 만드는 두 갈래(미실행·연결 실패)는 영어이고 타입이 그것을 지킨다. 오류 응답을 싣는 셋째 갈래는 답한 쪽이 문구를 만들며 `claude.hook`·`codex.hook` 은 설정 언어로 답한다 |
| stderr | 모두 | panic 메시지 + backtrace, `TASTY_LOG` 레벨의 tracing (CLI 프로세스는 이쪽만) |

## 관련

- [CLI 크레이트 내부 구조 — 세 갈래 대칭](cli-structure.md) — 파일 로그를 host 프로세스로 한정한 결정
- [build.md](build.md) — release(`strip = true`) / dev / dist 프로필
- [error-handling.md](error-handling.md) — `Result` 처리·로그 레벨 정책 (애초에 crash 를 줄이는 쪽)
- [self-verification.md](self-verification.md) · [e2e-tests.md](e2e-tests.md) — 재현·검증
- [종료 시퀀스 — 종료 cascade + Drop tail](../architecture/shutdown-sequence.md) · [gpu-rendering.md](gpu-rendering.md) — 이벤트 루프 stall 워치독의 결정 근거와 렌더 구조
- [memory-leak-soak.md](memory-leak-soak.md) — 죽음/멈춤이 아니라 **메모리가 새는** 증상일 때 (soak 테스트 + 계층별 판정)
