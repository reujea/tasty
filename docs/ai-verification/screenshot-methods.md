# 스크린샷 방법

`ui.screenshot`은 Tasty가 렌더한 프레임을 PNG로 저장한다. OS 화면 녹화 권한이 필요 없고 다른 창의 가림·포커스·최대화에 영향을 받지 않는다. native WebView는 이 프레임에 포함되지 않으므로 OS 화면 캡처를 사용한다.

대상에 따라 캡처 방법을 고른다. 자체 캡처에 나오지 않는다고 화면을 캡처할 수 없는 것은 아니다.

| 찍으려는 것 | `ui.screenshot`(CLI `tasty screenshot`) | OS 화면 캡처 |
|---|---|---|
| terminal surface | **있다** — `--surface` 로 grid 크기 오프스크린, `--window` 로도 | 있다 |
| host egui(chrome · explorer · dag_graph · empty) | **있다** — `--window` | 있다 |
| 별도 winit 창(설정 · plugin · preset · 종료 확인) | **있다** — `--window <id>` 명시 | 있다 |
| plugin egui-mesh(image · mesh_demo surface · popup · banner) | **있다** — `--window` | 미측정 (surface 축만 쟀다) |
| **native WebView(markdown · html)** | **없다** — host chrome 만 나온다 | **있다 — 여기뿐** |
| `tasty-gallery` | 없다(갤러리는 IPC 가 없다) | `TASTY_GALLERY_SHOT` env GPU 캡처 — 아래 |
| 셸 설정 모드 등 IPC 를 못 쓰는 상태 | 없다 | 있다 |

출처: 실측 2026-09-07, Xvfb :90 · aarch64 · WebKitGTK 4.1 — 한 창을 같은 순간 두 채널로
찍어 대조했다. **하드웨어 GPU 가 붙은 X 서버는 이 표에서 미측정이다**(그쪽은 OS 캡처가
GPU 창에서 검게 나온다고 알려져 있고, 이 실측은 그 조건이 아니다).

시각 판단 휴리스틱·체크리스트는 아래 [시각 판정 체크리스트](#시각-판정-체크리스트).

## `tasty screenshot` CLI / `ui.screenshot` IPC (권장)

캡처는 release에서 제공하는 관찰 기능이다. 창·surface를 ID로 지정하며 사용자의 포커스·가시 탭·선택을 바꾸지 않는다. 파일을 임의 경로에 쓰는 기능이므로 local caller만 호출할 수 있고 플러그인에는 노출하지 않는다.

- GUI 모드 전용(headless 불가).
- 두 가지 대상:
  - **surface 캡처** `--surface <id>` — 그 **터미널** surface 를 **오프스크린 렌더**로 그 자체
    grid 크기(cols×rows)에 캡처한다. 배경 탭·다른 workspace·비-focus 창의 surface 도 찍히며
    swapchain/present/가시 프레임/focus 를 전혀 건드리지 않는다(`pending_surface_screenshot`
    → `GpuState::capture_surface_to_png`). 소유 창은 surface_id 로 자동 해소(창별 CoreState 순회).
    **터미널만 지원**(v1). egui 패널(explorer/markdown/image/html)·plugin·webview surface 는
    범위 밖 — 명확한 에러 반환. **거절됐다고 그 화면을 못 찍는 것이 아니다** — 위 표대로
    비-terminal surface 는 `--window` 로, webview 는 OS 화면 캡처로 찍는다.
  - **window 캡처** `--window <id>`(없고 main 창이 1개면 그 창; 그 외는 에러 — focus 기본값
    금지) — 그 창의 전체 프레임(chrome 포함)을 swapchain readback 으로 캡처
    (`pending_screenshot`). 명시한 id 는 **설정·플러그인·종료 확인 모달과 preset 창**도
    가리킬 수 있다(아래 "무엇을 캡처할 수 있는가").
- 응답 `{ "path": .., ("surface_id"|"window_id"): .., "scheduled": true }` — **다음 프레임에
  캡처 예약**(비동기). 호출 직후 잠깐 기다렸다 파일을 읽는다. 대상 창은 자동으로 redraw 를
  요청받아(비-focus 창도) 캡처가 발화한다.

```bash
# 터미널 surface 를 ID 로 (focus 무관, 배경 탭도 가능)
tasty screenshot --path /abs/out.png --surface 5
# 창 전체 프레임 (창 여러 개면 --window 필수; list windows 로 ID 조회)
tasty screenshot --path /abs/win.png --window 2
```

### 무엇을 캡처할 수 있는가 (경계)

**명시한 `--window <id>` 는 모든 창을 가리킬 수 있다** — main 창뿐 아니라 설정 · 플러그인 ·
종료 확인 모달과 preset 창까지. 별도 winit 창으로 뜨는 UI 를 자동 시각 검증할 수 있어야
디자인 정합 확인이 사람 눈에 의존하지 않는다. 하드웨어 GPU가 붙은 X 서버에서는 캡처가 검게 나올 수 있으므로 실제 결과를 확인한다. 아래 Xvfb 관측을 모든 환경에 적용하지 않는다.

경계는 **창의 종류가 아니라 두 가지 다른 축**에 있다 (근거·기각 대안·재검토 조건:
이 문서).

1. **명시 지정만 넓어진다.** `--window` 를 생략했을 때의 자동 선택은 종전대로 **main 창이
   정확히 하나일 때뿐**이고, 모달로 폴백하지도 포커스를 보지도 않는다([포커스 정책](../design/policies/focus.md)).
   자동 선택이 "사용자가 무엇을 열어두었는가" 에 의존하기 시작하면 같은 명령이 실행할
   때마다 다른 것을 찍는다.
2. **캡처는 읽기이지 행동이 아니다.** `window.list` 와 `window.close` 는 종전대로 main 창만
   다룬다 — 모달 · preset 은 사용자 조작 영역이라 에이전트 **행동** 대상이 아니다. 캡처가
   가능해져도 그 집합은 넓어지지 않는다.

[포커스 정책](../design/policies/focus.md)은 에이전트 행동의 부수효과가 **사용자 상태**(포커스 / 닫은 항목 히스토리 / 선택 ·
스크롤 · 커서)에 닿는 것을 금지한다. 캡처는 이미 그려진 프레임의 readback + 리페인트
요청이고 리페인트는 멱등이라 그중 무엇도 바꾸지 않는다. 그리고 `ui.screenshot` 은
`local_only`(plugin 미노출)라 호출자는 이미 사용자 권한으로 `config.toml`(설정 창이 그리는
내용 전부)과 PTY 를 읽을 수 있다 — 모달 캡처를 막아도 새로 감춰지는 정보가 없고, 자동
검증만 잃는다.

**한계 — 다시 그리기 누락은 이 캡처로 재지 않는다.** 캡처가 리페인트를 요청하므로, 창이 다시 그려지지 않아 화면에 늦게 나오는 결함도 캡처에는 바로 보인다. 그런 결함은 프레임을 만들지 않는 OS 캡처(Xvfb 의 `ffmpeg -f x11grab` 등)로 확인한다.

### 무엇을 그리느냐가 어느 캡처로 보이느냐를 정한다 — webview 는 `ui.screenshot` 에 안 담긴다

렌더링 경로에 따라 사용할 수 있는 캡처 방법이 다르다.

| 렌더 경로 | 무엇이 그것으로 그려지나 | `ui.screenshot --window` | OS 화면 캡처 |
|---|---|---|---|
| GPU 셰이더 | `terminal` | 콘텐츠 나옴 | 콘텐츠 나옴 |
| host egui | `empty` · `explorer` · `dag_graph` · chrome 전부 · 별도 winit 창(설정·plugin·preset·종료 확인) | 콘텐츠 나옴 | 콘텐츠 나옴 |
| plugin egui-mesh | `image` · `mesh_demo` surface, plugin popup, plugin banner | 콘텐츠 나옴 | surface 확인. popup·banner는 미측정 |
| **native WebView** | **`markdown` · `html`** | **안 나옴** — 아래 chrome 만 | 콘텐츠 나옴 |

**webview 만 갈리는 이유는 환경이 아니라 구조다.** `ui.screenshot` 은 wgpu swapchain 을
readback 하는데(`gfx/gpu/screenshot.rs`), native WebView overlay 는 그 swapchain 이 아니라
**별개의 OS 자식 창**이다(Linux 는 X11 자식 창 안의 GTK 창 — `host_api/webview/linux.rs`).
readback 이 그 자리에서 읽는 것은 overlay 아래에 host 가 그려 둔 chrome
(`adapters/ui/surface/webview_chrome.rs`가 공용 `tasty_ui_widgets::webview_page_tile`로 그린다)이다. 페이지를 불러온 surface 는 boundary 상태라
캡처에는 **글리프·라벨·URL 없는 빈 타일**(`bg-panel` + 1px 테두리)만 담긴다. 빈 타일은
WebView 렌더링 실패를 뜻하지 않는다.

**처방**: webview 화면의 픽셀이 필요하면 `ui.screenshot` 이 아니라 **OS 화면 캡처**를 쓴다.
실측(2026-09-07, Xvfb :90 · aarch64 · WebKitGTK 4.1): 한 창을 같은 순간 두 채널로 찍어
`ui.screenshot` 은 host chrome 만, `scrot` 은 렌더된 페이지(제목·본문·한글
글리프·색 블록)를 그대로 냈다. 네 갈래를 한 탭에 surface split 으로 나란히 놓고 한 장에
담으면 대조가 한 번에 끝난다.

같은 Xvfb 관측에서는 scrot으로 터미널·host egui·plugin mesh surface·설정 모달을 캡처할 수 있었다. Xvfb는 소프트웨어 렌더링을 사용한다. 하드웨어 GPU가 연결된 X 서버는 이 비교에서 측정하지 않았으므로, OS 캡처가 항상 되거나 항상 검게 나온다고 일반화하지 않는다.

**함정 하나 더**: plugin egui-mesh popup 은 **첫 프레임에 콘텐츠가 없다.** 셸(scrim +
border)만 그려진 상태로 찍히면 "mesh 가 안 온다" 로 오진한다 — 실측에서 팝업을 연 직후
캡처가 빈 사각형이었고, 몇 초 뒤 같은 팝업이 본문을 그렸다. 열자마자 찍지 말고 한 번 더
찍어 같은지 본다.

### 모달 창의 ID 를 얻는 법

`list windows`(`window.list`)는 위 2 번 때문에 **main 창만** 열거한다. 설정·Preset·Plugins·종료
확인 같은 보조 창의 id 는 debug 빌드의 `tasty debug windows`(`debug.window.list`)로 얻는다.
OS 창 목록을 거치지 않으므로 X11·macOS·Windows 에서 같은 절차다.

```bash
# 모달을 띄우고(debug 빌드 전용) 창 목록에서 kind 로 고른다
tasty debug settings open --tab general
# 창 생성은 예약만 된다 — 나올 때까지 최대 20회 × 100ms 다시 조회하고, 없으면 멈춘다
S=""
for _ in $(seq 1 20); do
  S=$(tasty debug windows | python3 -c "import json,sys; print(next((w['window_id'] for w in json.load(sys.stdin)['windows'] if w['kind']=='settings'), ''))")
  [ -n "$S" ] && break
  sleep 0.1
done
[ -n "$S" ] || { echo "settings window did not appear in debug windows" >&2; exit 1; }
tasty screenshot --path /abs/settings.png --window "$S"
```

`debug settings open` 은 창 생성을 예약만 하고 돌아오므로 위 블록은 `settings` 가 나올 때까지
짧게 다시 조회한다. 설정 창이 이미 열려 있으면 `debug settings open --tab … --subtab …` 은 예약 없이
그 창의 탭을 바로 바꾸고(응답 `scheduled:false`), 같은 `window_id` 로 다음 화면을 캡처할 수 있다.
다 찍은 뒤 `debug settings close` 로 저장하지 않고 닫으면 인스턴스를 다시 띄우지 않고 다음 캡처를
이어 간다.

실측(Xvfb, 2026-10-09): 한 인스턴스에서 `open --tab appearance --subtab colors` → `open --tab terminal --subtab tui`
→ `open --subtab performance` 순서로 같은 창(`window_id` 불변)이 Appearance › Colors, Terminal › TUI,
Terminal › Performance 로 바뀐 것을 캡처로 확인했다. `close` 뒤 `debug windows` 에 `settings` 가 없었고,
다시 연 창은 기본 탭(General › General)으로 열렸다. 응답 `windows[]` 의 필드와 `kind` 값은 [debug-ipc.md](../dev-guide/debug-ipc.md)의
`debug.window.list` 행이 정본이다. 창은 `kind` 로 고른다. `title` 은 winit 이 제목을 돌려주는
플랫폼에서만 채워지고 X11·Wayland 에서는 `null` 이다.

실측(Xvfb, 2026-10-09): 설정 창을 연 직후 첫 조회에 `kind:"settings"`(1100x700, `modal:true`)가
나왔고, 그 `window_id` 는 `xdotool search --pid` 가 `Tasty Settings` 로 보여 준 X11 창 id 와
같았다(X11 에서 winit `WindowId` 는 X11 window id 그 자체다). 같은 id 로 `screenshot --window`,
`debug toast --window-id`, `debug inject egui-key --window-id` 가 그 창에 닿았다.

존재하지 않는 id 는 `Window id <id> not found` 로 거절된다.

CLI 없이 raw JSON-RPC(개행 구분)를 포트로 직접 보낼 수도 있다. 포트 파일은 debug 빌드면
**debug 루트** `~/.tasty-debug/tasty.port`, release 면 `~/.tasty/tasty.port` 다(루트 분리 —
격리 표 [self-verification 독립 검증](../dev-guide/self-verification.md#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다), 구현
`crates/tasty-ipc/src/port_file.rs`).

검증 인스턴스의 홈을 TASTY_HOME으로 지정한 뒤 실행한다. TCP 응답은 여러 번에 나뉘어
도착할 수 있으므로 개행까지 읽는다. 서버가 연결을 유지해도 EOF를 기다리지 않는다.

```python
import json
import os
import socket
from pathlib import Path

port_file = Path(os.environ["TASTY_HOME"]) / "tasty.port"
port = int(port_file.read_text().strip())
req = {"jsonrpc": "2.0", "id": 1, "method": "ui.screenshot",
       "params": {"path": "/abs/out.png", "surface_id": 5}}
with socket.create_connection(("127.0.0.1", port), timeout=8) as sock:
    sock.sendall((json.dumps(req) + "\n").encode())
    with sock.makefile("rb") as response:
        line = response.readline()
    if not line.endswith(b"\n"):
        raise EOFError("IPC response ended before the newline")
    print(json.loads(line))
```

### 사용자 세션을 건드리지 않고 격리 실행

검증할 때는 고유한 TASTY_HOME과 전용 디스플레이를 사용한다. debug와 release의 기본
데이터 경로는 다르지만, 다른 debug 인스턴스까지 자동으로 분리되지는 않는다.
TASTY_HOME은 빌드별 기본 경로보다 우선한다.

실행·PID 기록·준비 대기·회수는
[직접 검증 절차](../dev-guide/self-verification.md#tasty-에서-직접-검증)를 따른다.
그 절차는 자식 생존 여부를 확인하며 준비 조회를 최대 40회 시도한다. 준비를 확인하지
못하면 캡처 단계로 진행하지 않고 로그를 확인한다.

격리 홈과 전용 디스플레이는 OS 열기를 격리하지 않는다. TASTY_DEBUG_OS_OPEN_LOG와
가짜 브라우저 PATH/BROWSER 설정도 같은 문서에 따라 적용한다.
캡처 IPC는 그 검증 홈의 tasty.port로 보낸다.

TASTY_HOME은 휴지통도 격리하지 않는다. Linux에서 Explorer의 휴지통 이동은 `$XDG_DATA_HOME/Trash`(없으면 `~/.local/share/Trash`)를 사용한다. 휴지통에 닿는 실행에서는 XDG_DATA_HOME을 검증용 폴더로 지정해 사용자 휴지통에 파일이 남지 않게 한다.

격리 CLI는 바깥의 `TASTY_SESSION_TOKEN`·`TASTY_SURFACE_ID`·`TASTY_PARENT_HOME`을 제거하고 실행한다. 외부 토큰은 격리 인스턴스에 등록되지 않아 permission_denied가 발생하며, readiness 검사에서는 기동 실패처럼 보일 수 있다. 연결한 절차처럼 launcher와 CLI 모두 환경을 분리한다. 세션 토큰이 없는 로컬 CLI는 local caller로 접속한다.

직접 시작하면서 저장한 PID만 종료한다. 이름이나 명령줄 패턴으로 찾은 프로세스를 한꺼번에 종료하지 않는다. 정리할 디렉터리도 이번 검증에서 만든 경로인지 확인하고 `${VERIFY_HOME:?}`처럼 빈 값이 전달되지 않게 한다.

PID는 실행 직후 기록한다. 기록을 놓쳤다면 이번 실행 로그에서 PID를 확인한 뒤 Linux의 `/proc/<pid>/environ`에서 `TASTY_HOME` 값만 읽어 격리 홈과 대조한다. 환경 전체를 출력하지 않는다. 실행 파일과 실행 기록까지 확인해 이번 세션이 직접 시작한 인스턴스임을 알 수 있을 때만 종료 대상으로 삼는다.

macOS에서도 기록한 PID와 이번 실행 기록, 실행 파일 경로로 소유 관계를 확인한다. 어느 플랫폼이든 이번 검증의 프로세스임을 확인하지 못하면 종료하지 않는다. 다른 인스턴스를 잘못 종료했다면 시각·대상과 영향을 받은 검증을 기록하고, 해당 결과를 유효한 것으로 보고하지 않는다.

테스트 격리용 `--port-file <PATH>` 옵션도 있다(클라이언트가 읽을 포트 파일 지정).

### 측정 전에 — 대상 바이너리가 최신인지 확인한다 (plugin)

루트의 `cargo build`만으로 플러그인 실행 파일이 다시 만들어지지는 않는다. `cargo build --workspace` 또는 `cargo build -p tasty-plugin-<name>`으로 해당 바이너리를 빌드한다.

여기에 스테이징이 겹친다. host 는 **debug 빌드에서 부팅할 때만** `copy_if_newer` 로
`target/debug/builtin-plugins/` 를 갱신한다. release/dist 는 빌드 단계에서 스테이징한다. 모든 프로필은 번들에서 `<TASTY_HOME>/plugins/` 로
sync 한다(`crates/tasty-host-plugin/src/builtin.rs`). debug 스테이징 판정(`copy_if_newer`)은
**mtime 우선**이다(번들 → 홈 sync 는 내용 판정이다) — src 가 새것이면 내용을 안 보고 옮기고, dest 가 새것이면 내용을 안 보고
건너뛰며, **시각이 같을 때만 내용을 본다.** 그래서 "시각이 같아 조용히 건너뛴다" 는 갈래는
닫혔지만, **dest 가 더 새것인데 내용이 다른 경우**(src 가 `cp -p` 복원 등으로 더 옛 시각을 달고
바뀐 경우)는 여전히 못 본다.
빌드하지 않은 바이너리는 스테이징이 정상이어도 이전 코드다. 빌드 완료와 스테이징 완료를 따로 확인한다.

빌드나 스테이징이 빠지면 이전 플러그인을 실행해 변경 결과를 잘못 판단할 수 있다. 플러그인 바이너리와 번들 사본을 모두 확인한다.

기동 전에 다음 중 하나를 돌린다:

```bash
PROFILE=debug just build-plugins        # 정식 절차 — 빌드 + 스테이징까지
cargo build --workspace                 # 최소한 이것 (스테이징은 부팅이 한다)
```

확인은 mtime·크기 비교가 제일 싸다. **묻는 것은 "스테이징이 반영했나" 가 아니라
"빌드가 산출물을 다시 만들었나" 다** — 산출물이 소스보다 뒤면 만들어진 것이고, 그 뒤는
부팅이 위 판정으로 옮긴다:

```bash
ls -la target/debug/tasty-plugin-<name> \
       target/debug/builtin-plugins/<manifest-id>/tasty-plugin-<name>
```

### Xvfb 에서 실제 입력(휠·클릭)을 굴릴 때

전용 Xvfb에서 `xdotool`로 X11 입력을 보내면 winit·egui·plugin을 거치는 실제 입력 경로를 확인할 수 있다. 먼저 최신 플러그인 바이너리를 준비하고, 다음 환경 조건을 확인한다.

앞의 네 항목은 접속·창 선택·캡처 조건이고, 다섯째는 키보드 입력 조건이다. 도구의 종료 코드만으로 입력 전달이나 화면 갱신이 성공했다고 판단하지 않는다.

**1. `xdotool` 은 `xvfb-run` 이 만든 Xauthority 없이는 붙지 못한다.** `DISPLAY` 만 넘기면
`Authorization required, but no authorization protocol specified` 뒤에
`Can't open display: (null)` 로 죽는다. `xvfb-run` 은 임시 `Xauthority` 를 만들어 자식
env 에만 넣으므로, 그 값을 프로세스에서 되읽어 함께 export 한다.

```bash
DISPLAY=$(tr '\0' '\n' < /proc/$HOST_PID/environ | grep '^DISPLAY=' | cut -d= -f2)
XAUTHORITY=$(tr '\0' '\n' < /proc/$HOST_PID/environ | grep '^XAUTHORITY=' | cut -d= -f2)
export DISPLAY XAUTHORITY
```

**2. 창 id 는 `tasty list windows` 에서 받는다 — `xdotool search --pid` 로 고르지 않는다.**
그 검색은 main 창이 아닌 작은 창들까지 뱉는다. 실측하면 창이 셋
나오고, 그중 둘이 main 창이 아니다:

| `getwindowgeometry` | 정체 |
|---|---|
| 화면 크기와 같은 큰 값 | main 창 — 이것만 캡처·입력 대상이다 |
| **16x16** (이름 `tray-icon tray app <pid>-N`) | 시스템 트레이 아이콘 창 |
| **10x10** | GTK 내부 헬퍼 창 |

아무 것이나 집으면 그 작은 값으로 포인터 좌표를 계산하게 되고, 음수가 나와
`mousemove: unrecognized option '-104'` 로 끝난다 — 창을 잘못 골랐다는 신호다. main 창은
IPC 가 직접 알려주므로 추측할 이유가 없다(X11 에서 winit `WindowId` = X11 window id).

**작은 창들의 크기를 판별 기준으로 삼지 않는다 — 그 값은 환경에 따라 변한다.** 이 둘은
main 창과 **다른 X 클라이언트**(GTK 연결)에 속하고, 따라서 winit 의 DPI 배율이 아니라
**GDK 의 배율**을 따른다. 실측: `WINIT_X11_SCALE_FACTOR=2` 만 주면 main 창만 두 배가 되고
작은 창들은 16x16 · 10x10 그대로지만, 여기에 `GDK_SCALE=2` 를 더하면 작은 창들이
**32x32 · 20x20** 이 된다. 위 표의 두 값은 `GDK_SCALE` 이 1 일 때의 값이다.

**믿을 수 있는 판별은 크기가 아니라 출처다.** main 창은 `tasty list windows` 가 알려주고,
X 리소스 id 대역도 갈린다 — main 창은 winit 의 연결(예 `0x2xxxxx`), 작은 창들과 GTK 가
띄우는 네이티브 메뉴는 GTK 의 연결(예 `0xaxxxxx`)에 있다.

**3. WM 이 없으므로 `windowactivate` 는 실패한다 — 그리고 필요도 없다.** 맨 Xvfb 에는 창
관리자가 없어 `_NET_ACTIVE_WINDOW` 가 없고, `xdotool windowactivate` 는
`Your windowmanager claims not to support _NET_ACTIVE_WINDOW` 로 거절된다. 여기서 멈추면 안
된다 — X11 기본 포커스 모델(PointerRoot)에서는 **포인터가 얹힌 창이 입력을 받으므로**
activate 없이 절대 좌표로 `mousemove` 한 뒤 `click` 하면 그대로 전달된다. (데스크톱 X 서버의
재현 절차가 activate 를 요구하는 것은 그쪽에 WM 이 있기 때문이고, 그 단계를 Xvfb 에 그대로
옮기면 실패를 오진하게 된다.)

**4. 캡처 전에 포인터를 한 번 움직여 재렌더를 유발한다.** 부팅 동안에는 `about_to_wait` 가
`WaitUntil(+16ms)` 로 스스로 프레임을 돌리지만(`src/app/boot_machine.rs`), **부팅이 끝나면
재렌더는 이벤트 구동**이 된다 — 입력이나 PTY 출력 같은 무언가가 `request_redraw` 를 부를
때만 다시 그린다. WM 이 없는 Xvfb 에는 그 이벤트를 만들어 줄 주체가 없어 **마지막으로
그려진 프레임이 그대로 남는다.** 그 상태로 캡처하면 그 뒤에 바뀐 화면을 못 보고, 남아 있는
것이 로딩 프레임이면 "아무것도 안 그려졌다" 로 오진한다. 캡처 직전에 `xdotool mousemove` 로
포인터를 한 번 움직인 뒤 찍는다. (입력을 굴리는 검증은 `click` 이 포인터 이벤트를 동반해 이
조건을 우연히 만족한다 — 하지만 **우연에 기대지 않는다.** "입력 전" 기준 화면을 찍는 순간이
정확히 이 함정에 걸리는 구간이다.)

**5. Tasty 가 그리는 화면의 키 입력은 main 창에 X 포커스를 준 뒤에만 들어간다 — 없으면 조용히 실패한다.** 위 3 번의
PointerRoot 모델이 구해 주는 것은 **포인터**뿐이다. 클릭만으로는 키보드 포커스가 생기지 않아
`XGetInputFocus` 가 `PointerRoot`(1)를 돌려주고, winit 은 포커스 없는 창에 키 이벤트를
올리지 않는다. `xdotool type --window <id>` 로 창을 지목해도 같다 — 합성 이벤트가 그
경로를 타지 못한다. **그리고 조용하다**: `xdotool` 은 그 경고를 stderr 로 흘리면서 `0` 으로
끝나고, 직후 캡처는 치기 전 프레임과 **바이트까지 같다**(실측: 같은 `md5sum`). 종료 코드로
판정하면 "쳤는데 필터가 안 먹는다" 로 오진한다.

`xdotool windowfocus --sync <main 창 id>` 로 X 포커스를 main 창에 주면 `XGetInputFocus` 가 그
창을 돌려주고, `xdotool key`·`xdotool type` 이 들어간다. 실측: 포커스를 준 뒤
`key ctrl+shift+p` 로 명령 팔레트가 열리고 `type zzq` 가 egui `TextEdit` 에 들어갔으며, WebView
의 `<input>` 에도 `type` 으로 친 글자가 들어갔다. 포커스를 주지 않으면 같은 `key` 로 팔레트가
열리지 않고 `type` 은 rc=0 으로 끝난다. 그래서 키 입력 재현은 포커스를 먼저 주고, 친 결과를
캡처로 확인한다.

**호스트 단축키(`xdotool key alt+w` 등)도 같다.** 포커스를 준 뒤에는 클릭한 surface 에
`xdotool key` 로 보낸 단축키가 `KeybindingSettings` 경로로 처리된다. 포커스를 빼면
WebView surface(html·markdown)와 Tasty 가 그리는 surface(터미널·image 같은 egui-mesh·DAG)의
결과가 갈린다. PointerRoot 에서 키는 포인터 아래 X 창으로 가는데 WebView 는 자기 X 창이라 키를
받아 호스트로 넘기고, Tasty 가 그리는 surface 는 포커스 없는 main 창이라 키를 받지 못한다.
그래서 "markdown 은 닫히는데 image 는 안 닫힌다" 같은 surface 종류별 결함처럼 보인다. 포커스를
준 뒤에도 결과가 다를 때만 surface 종류의 결함으로 판정한다. `debug inject key` 는 PTY 에
바이트를 넣는 경로라 호스트 단축키를 재현하지 못한다.

포커스와 무관하게 `TextEdit` 에 쿼리를 넣어 **목록이 줄어든 화면**을 찍으려면 egui 입력 큐로
직접 주입한다(`tasty debug inject egui-text`). 설정 같은 별도 창은 `--window-id <id>` 로
그 창을 지목한다(창 ID는 위 "모달 창의 ID 를 얻는 법"). 그 창의 입력칸 포커스·토글·버튼 클릭은
`xdotool` 대신 `tasty debug inject egui-mouse --window-id <id> --fx .. --fy ..`(창 기준 정규화
좌표)로, Enter·Backspace 는 `egui-key --window-id <id>` 로 넣는다. 창 ID 를 얻는 것부터 입력까지
debug IPC 만으로 끝나므로 X11 이 아닌 환경에서도 같은 절차다. 절차·거절 조건은
[debug-ipc.md](../dev-guide/debug-ipc.md) "문자 주입은 키 주입과 다른 채널이다" 가 정본이다.

그 밖에 이 조합에서 지키는 것:

- **기동은 절대경로로**(`/abs/worktree/target/debug/tasty`). 프로세스 소유를 나중에 가릴 때
  1차 기준이 cmdline 의 바이너리 경로인데, `./target/debug/tasty` 로 띄우면 그 경로가
  남지 않아 기준 자체가 무력해진다.
- **`xvfb-run` 의 `$!` 는 래퍼 PID 다.** 그것만 죽이면 안의 tasty 가 고아로 남는다 —
  포트 파일이 생긴 뒤 위 1 번과 같은 방식으로 호스트 PID 를 따로 잡아 **둘 다** 정리한다.
- 스크롤 결과 판정은 스크린샷 **픽셀 diff** 로 한다(`ImageChops.difference(...).getbbox()`).
  "달라 보인다" 로 멈추지 말고, **수정을 되돌린 빌드로 같은 절차를 한 번 더 돌려** 그 쪽에서
  `bbox=None` 이 나오는 것까지 확인하면 인과가 닫힌다.

## `tasty-gallery` 캡처 (`TASTY_GALLERY_SHOT`)

갤러리는 별도 바이너리로 IPC를 제공하지 않는다. `TASTY_GALLERY_SHOT`을 사용하면 창을 선택한 뒤 GPU 프레임을 PNG로 저장할 수 있다. 본체와 같은 swapchain readback을 사용하며 BGRA를 RGB로 변환하고 행을 256바이트에 맞춘다. OS 화면 녹화 권한은 필요하지 않다. 캡처할 대상은 정해져 있어도 렌더링 잡음 때문에 결과 픽셀이 매번 같지는 않다. 아래 픽셀 비교 절차를 함께 따른다.

- 형식: `TASTY_GALLERY_SHOT=<idx>[@<y>]:<png>[,...]` — **배치**. 콤마로 여러 항목을 주면 **한 인스턴스에서** 순차로 선택→4프레임 settle→캡처하고 마지막에 **자체 종료**한다(콜드스타트 1회. `crates/tasty-gallery/src/main.rs`).
  **settle 은 프레임 수지 시간이 아니다**(`plan.frame >= 4`) — 벽시계로 도는 애니메이션은 이 대기로 가라앉지 않는다(아래 절).
- `idx` 는 **페이지(Category) index**(0-base, `catalog::pages()` 순서 = Foundations 0 · Components 1 · Icons 2 · Overlays 3 · Layouts 4 · Plugins 5 · Chrome 6).
- `@<y>` 는 본문 **스크롤 오프셋(px)** 이다. 한 페이지에 섹션이 여러 개 쌓이면 상단 뷰포트만으로는 아래쪽 specimen 을 찍을 수 없으므로 그 자리로 강제 스크롤한다 — 임시 섹션을 꽂았다 되돌리는 우회가 필요 없다. 정확한 y 를 모르면 여러 오프셋을 한 배치로 훑고 맞는 컷을 고른다.
- 창 크기는 `TASTY_GALLERY_SIZE=<w>x<h>` 로 덮어쓴다(기본 1100×720). 문서 컬럼이 최대 1080 이라 기본 창에서는 우측이 잘린다 — specimen 전폭을 담으려면 넓혀서 찍는다.
- 시작 테마는 `TASTY_GALLERY_THEME=mocha|latte` 로 고른다(대소문자 무시, 기본 Mocha). 모르는 값은 경고 로그를 남기고 Mocha 로 시작한다. 툴바의 Theme 세그먼트를 누른 것과 같으며, 테마를 고정해 그리는 예제(Latte 전용 카드 등)는 이 값과 관계없다.
- 표본 언어와 라벨 글꼴은 `TASTY_LOCALE=<언어 코드>`로 고른다. 값이 없으면 `en`이다. 단위 시험은 이 값과 관계없이 `en`으로 그린다.
- 갤러리는 캡처 후 스스로 종료하므로 `timeout` 불필요(macOS 엔 `timeout` 명령도 없다).

```bash
B="${TMPDIR:-/tmp}/tasty-shots"; mkdir -p "$B"
# 여러 페이지 한 방에 (init 1회): idx 1=Components(Button·Badge·MenuItem…), 3=Overlays, 6=Chrome
TASTY_GALLERY_SHOT="1:$B/components.png,3:$B/overlays.png,6:$B/chrome.png" ./target/debug/tasty-gallery

# 페이지 중간 섹션(Layouts 페이지의 Task DAG)을 전폭으로
TASTY_GALLERY_SIZE=1360x1000 \
  TASTY_GALLERY_SHOT="4@5200:$B/dag-canvas.png,4@11000:$B/dag-surface.png" ./target/debug/tasty-gallery

# 같은 페이지를 Latte 로
TASTY_GALLERY_THEME=latte TASTY_GALLERY_SHOT="3:$B/overlays-latte.png" ./target/debug/tasty-gallery
# 윈도우 1100x720, 1:1(논-레티나) → 좌측 사이드바 ~240px, 우측이 specimen 패널
```

## OS 화면 캡처

- **Linux** `scrot` — native WebView 화면은 이 채널로만 찍힌다(위 "무엇을 그리느냐가 어느 캡처로 보이느냐를 정한다").
- **macOS** `screencapture` — 해당 프로세스에 화면 녹화 권한 필요(없으면 `could not create image from display` 실패 → `ui.screenshot` 또는 갤러리는 `TASTY_GALLERY_SHOT` 사용).
- **Windows** PowerShell `CopyFromScreen`. 윈도우가 가려져 있으면 `ShowWindow`+`SetForegroundWindow` 로 최대화 후 캡처. tasty.exe 실행 중이면 빌드가 파일을 덮어쓰지 못할 수 있다. 이 세션에서 직접 시작한 검증 인스턴스만 PID를 확인해 종료한다(`Stop-Process -Force`). 사용자 인스턴스는 종료하지 않는다.

## 시각 판정 체크리스트

스크린샷의 "눈으로 보이는지" 에만 의존하지 말고, **코드 레벨에서 논리적으로** + **픽셀 수치로** 검증한다. 자체 검증 일반 절차는 [dev-guide/self-verification](../dev-guide/self-verification.md).

### 스크린샷은 tasty 자체 `ui.screenshot` 우선

화면 캡처는 OS 캡처 도구가 아니라 **tasty 의 `ui.screenshot`**(CLI `tasty screenshot`)를 우선 쓴다 — 정확한 윈도우/surface 영역을 결정적으로 얻고, 좌표가 tasty 내부 레이아웃과 일치한다(OS 캡처는 데코·DPI·다른 창 혼입 위험). focus-독립 정식 기능이라 release 에서도 동작하고 `--surface <id>` 로 특정 터미널 surface 도 캡처 가능 → [screenshot-methods](#). 셀 색 검증은 `debug.glyph_color`(렌더러가 GPU 에 push 하는 실제 RGBA, debug 빌드)도 함께.

native WebView로 표시하는 Markdown·HTML은 자체 캡처에 포함되지 않는다. 해당 콘텐츠를 검증하려면 OS 화면 캡처를 사용한다. 대상별 방법은 문서 첫 표를 참고한다.

### 체크리스트 ("보인다"고 말하기 전에)

#### 1. 색상 대비

새 UI 요소의 색과 배경색의 RGB 차이를 계산한다. 배경과 같거나 유사하면 **확대해도 안 보인다.** 각 채널(R/G/B) 중 하나 이상이 충분히 차이나야 한다.

```
예: 배경 rgb(26,26,30) 에 경계선 rgb(60,60,75) → 차이 최대 45 (겨우 보임)
```

(실제 색은 Theme 토큰에서 — 하드코딩 금지, [theme › 색 생성 정책](../design/systems/theme.md#색-생성-정책). 대비 기준은 [theme.md](../design/systems/theme.md) "텍스트 대비 4.5:1".)

#### 2. 렌더 레이어/순서

요소가 실제로 화면에 나타나는지 렌더 파이프라인 순서를 코드에서 추적한다.

- egui `LayerId::background()` 는 모든 패널 **뒤** → 터미널 위에 그리려면 `Order::Foreground`.
- 터미널 GPU 렌더는 **누적(accumulate) → flush 1회 → 단일 패스 + per-surface scissor** 모델이다. 한 surface 의 인스턴스가 다른 surface range 를 침범하지 않는지(scissor rect / instance range)를 확인한다 — 상세 [gpu-rendering](../dev-guide/gpu-rendering.md).

#### 3. 픽셀 수치 검증

스크린샷(또는 `ui.screenshot`) 후 해당 영역의 RGB 를 직접 읽어 기대값과 비교한다.

```python
for x in range(expected_x - 5, expected_x + 5):
    idx = (y * width + x) * 3
    print(f'pixel({x},{y}) = rgb({data[idx]},{data[idx+1]},{data[idx+2]})')
```

기대 색이 그 좌표에 실제로 있는지 확인한 뒤에야 "보인다" 고 판단한다.

체크: ① 색이 배경과 다른가(코드) ② 레이어가 위에 그려지나(렌더 순서) ③ 픽셀 RGB 가 기대값과 일치하나(수치). 셋 다 통과 안 하면 "보인다" 고 말하지 않는다.

**전후 캡처를 비교해 "안 바뀌었다" 를 판정하는 것은 위와 다른 축이다.** 그쪽은 양성 대조와
노이즈 바닥을 먼저 세워야 하고, "바이트 동일" 은 판정 기준이 될 수 없다 —
절차는 [screenshot-methods](#) "픽셀 diff 판정 전" 절이 정본이다.

### 스크린샷 판단 휴리스틱 (인지 함정 방지)

픽셀 검증과 별개로, 눈으로 볼 때 빠지는 함정:

1. **전체를 훑지 말 것.** 변경 영역을 좌표로 특정하고 그 영역만 집중해서 본다 — 전체 인상으로 판단하면 변경분이 묻힌다.
2. **"안 보인다" 단정 전 좌표 재확인.** 작은 요소는 축소 스크린샷에서 안 보인다. "안 보인다" 는 자주 "내가 못 찾았다" 다 — 잘라 확대하거나 픽셀을 읽는다.
3. **코드 수치와 스크린샷 대조.** 알파 12 오버레이가 배경 위에서 실제로 어떤 차이를 내는지 직접 본다 — "12 면 약하다/강하다" 를 코드만 보고 추측하지 않는다.
4. **불확실하면 "잘 모르겠다" 라고 말한다.** 틀린 확신이 신뢰를 깎는다.

체크리스트는 *기준*, 휴리스틱은 *판단 과정의 함정 방지* 다.

### 관련

- 이 문서의 앞 절들 — 캡처 수단 · Xvfb 함정, 바로 아래 [픽셀 diff 판정 전](#픽셀-diff-판정-전--통제군으로-노이즈-바닥을-먼저-재라) — **전후 diff 판정의 노이즈 바닥과 양성 대조**
- [dev-guide/debug-ipc](../dev-guide/debug-ipc.md) — `ui.screenshot` / `debug.glyph_color`
- [dev-guide/gpu-rendering](../dev-guide/gpu-rendering.md) · [design/systems/theme › 색 생성 정책](../design/systems/theme.md#색-생성-정책) · [design/systems/theme](../design/systems/theme.md)

## 픽셀 diff 판정 전 — 통제군으로 노이즈 바닥을 먼저 재라

두 트리(before/after)의 스크린샷을 픽셀 diff 로 비교해 변화를 판정할 때, **diff 가 0 이 아니라는 것이 곧 코드 변화라는 뜻은 아니다.** llvmpipe(소프트웨어 GPU) 텍스트 안티앨리어싱은 같은 바이너리·같은 화면이라도 런마다 미세하게 다를 수 있다. 그 런-간 노이즈보다 작은 변화는 픽셀 diff 로 판별할 수 없고, 노이즈를 실제 변화로 오해하면 거짓 결함이 된다.

**규칙: before/after 를 비교하기 전에, 동일 바이너리로 같은 화면을 2회 캡처해 그 둘의 diff(=노이즈 바닥)를 먼저 재라.** before/after diff 가 그 바닥보다 확실히 크고 변화가 예상 영역(bbox)에 국한될 때만 실제 변화로 판정한다.

**노이즈 바닥은 화면 내용에 따라 다르다 — 반드시 측정하고 고정값을 가정하지 마라.**

- 애니메이션이 있는 화면(예: 부팅 로딩 스피너)은 노이즈가 크다 — 한 측정에서 동일 바이너리 통제군 diff 가 ~548px 였다.
- 정적 specimen(예: 갤러리 모달 dialog)은 측정한 통제군에서 잡음이 0px(diff bbox `None`)였다. 같은 환경을 유지한 비교에서는 작은 차이도 변경 근거로 살펴볼 수 있다.

같은 llvmpipe 환경에서도 이렇게 갈리므로 수치를 재사용하지 말고 **매번** 통제군을 찍는다.

### 의도적인 변경이 캡처에 나타나는지 먼저 확인한다

잡음 측정은 작은 픽셀 차이를 실제 변경으로 오해하지 않게 한다. 반대로 확인할 대상이 프레임 밖이면 아무리 값을 바꿔도 캡처가 같을 수 있다. 따라서 의도적으로 바꾼 값이 화면에 나타나는지 먼저 확인해야 한다.

그래서 순서를 고정한다.

1. **양성 대조부터.** 값을 일부러 바꾼 빌드로 찍어 **실제로 달라지는지** 먼저 본다.
   여기서 안 달라지면 오프셋·창 크기가 틀린 것이지 변경이 없는 것이 아니다.
2. **그 다음 노이즈 바닥.** 같은 바이너리로 두 번 찍는다.
3. **그 다음에야 before/after.**

**"바이트 동일" 을 판정 기준으로 쓰지 않는다.** PNG 바이트는 인코더 상태에 따라 흔들리고,
반대로 바이트가 같아도 위처럼 아무것도 안 담긴 컷일 수 있다. 판정은 디코드한 RGB 의
diff bbox 와 채널 델타로 한다.

### 잡음은 두 크기로 갈린다 — 픽셀 수가 아니라 **채널 델타**로 가른다

같은 바이너리 2 회 캡처의 diff 를 픽셀 **수**로만 보면 두 가지가 섞인다. 한 측정
(갤러리 Overlays 페이지, 8 오프셋)에서 3 컷은 bbox `None`, 5 컷이 달랐는데 그 5 가
같은 종류가 아니었다.

| 종류 | 최대 채널 델타 | 다른 픽셀 | 정체 |
|---|---|---|---|
| 하위 지각 잡음 | 1 ~ 13 | 14 ~ 428 | 래스터화·합성 반올림. 눈에 안 보인다 |
| 애니메이션 위상 | 141 | 2380 (25×537) | 회전 중인 스피너가 다른 각도에서 잡혔다 |

**픽셀 수는 둘을 못 가른다** — 428px 짜리가 델타 8(안 보임)이고 2380px 짜리가 델타
141(명백히 다름)이었다. 그러니 diff 를 셀 때 **채널 델타의 최대·중앙값을 함께 낸다.**

애니메이션 위상 쪽은 **settle 을 늘려도 안 없어진다.** 갤러리의 settle 은 프레임
수(`plan.frame >= 4`)인데 `Spinner`(`crates/tasty-ui-widgets/src/spinner.rs`)의 호 위상은
`ctx.input(|i| i.time)` 즉 **벽시계**에서 나오고 `request_repaint()` 로 계속 돈다 — 4 프레임이
걸린 시간이 런마다 다르면 위상도 다르다. 처방은 둘 중 하나다: 애니메이션이 없는 오프셋을
고르거나, **그 bbox 를 판정에서 제외하고 나머지로 판정한다.**

**애니메이션의 출처를 specimen 소스에서만 찾지 않는다.** 위 스피너는 갤러리 소스에
없다 — 공용 위젯 크레이트에서 온다. 갤러리만 훑으면 "애니메이션 없음" 이라는 틀린
결론이 나온다.

전후 차이가 잡음과 같은 bbox 안에 있고 크기도 비슷하면 픽셀 비교만으로 실제 변경인지 구분하기 어렵다. 잡음의 크기뿐 아니라 위치도 대조한다. 한 관측에서는 잡음이 428px@(627,420,652,444), 전후 차이가 356px@(628,421,651,443)여서 변경 효과를 분리할 수 없었다.

**소스로 증명되는 더 강한 경로가 있으면 우선한다.** 픽셀 대조가 노이즈 바닥 근처라 애매할 때, 코드상 값 델타가 0(동일값 const 인라인)이거나 변경이 한 필드로 국한되는 등 소스로 자명한 경로가 있으면 그쪽이 픽셀 대조보다 강하다 — 픽셀 대조는 그 경우 보조 확인이다.
