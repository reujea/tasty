# 수정 후 자체 검증 (Self-verification)

수정이 직접 확인 가능한 종류라면 **사용자에게 "확인해 보세요" 라고 떠넘기지 않고 본인이, 커밋 전에 확인한다.**

가능한 검증은 커밋 전에 마치고, 확인한 범위와 실행할 수 없었던 이유를 함께 보고한다.

## 원칙

1. **검증 가능 여부를 먼저 판단.** IPC/CLI/스크립트로 트리거 가능하면 검증 가능. GUI 입력도 사용 가능한 디버그 입력 도구로 직접 재현한다. 실제 하드웨어나 접근 권한이 없어 확인할 수 없는 부분만 구체적으로 보고한다.
2. **검증은 커밋 전.** 빌드와 단위 테스트 통과는 그 검사 범위의 근거이며 전체 사용자 시나리오를 보장하지 않는다 — 별도 시나리오 재현이 필요.
3. **확인 안 됐으면 "확인 안 됨" 이라고 말한다.** 추측으로 "동작할 거예요" 보고 금지.
4. **검증 인스턴스는 자동 격리된다.** debug 빌드(`cargo run` / `target/debug/tasty`)는 `~/.tasty-debug/` 루트를, release 사용자 세션은 `~/.tasty/` 를 쓴다 — TASTY_HOME override가 없을 때 두 빌드의 데이터 경로가 다르다. 검증에는 고유한 TASTY_HOME을 사용해 다른 debug 인스턴스와도 나눈다. (상세 [아래 절](#독립-검증--개발도-agent-가-스스로-확인할-수-있어야-한다))

### 독립 검증 — 개발도 Agent 가 스스로 확인할 수 있어야 한다

> dev-guide 의 **가장 핵심 원칙**. 한 Tasty를 사용자와 여러 에이전트가 함께 쓰고([주체](../concepts/actors.md)), debug 빌드에서 사용자 조작을 재현할 수 있어야 한다는 [사용자 통제와 데이터 원칙](../principles/control-and-data.md#사용자에이전트원격-사용자)을 **개발 환경 자체에 적용한 것** 이다.

#### 원칙

**Tasty 의 모든 기능은, 그것을 개발하는 AI Agent 가 자기 수정을 독립적으로 띄워 검증할 수 있도록 만들어야 한다.**

Tasty 를 개발하는 환경이 곧 Tasty 다 (dogfooding). 보통 사용자·다른 Agent 는 **release** 빌드를 띄워 작업 중이다. 그 위에서 Agent 가 자기가 고친 것을 확인하려면 **debug** 빌드를 동시에 띄워야 하는데, 둘이 같은 자원(포트·상태 파일)을 공유하면 서로 간섭한다.

그래서 Tasty 는 **debug 빌드와 release 빌드의 환경을 격리** 한다 (0% 충돌은 불가능하지만 최대한). 덕분에 Agent 는 *자신이 release tasty 안에서 동작 중이어도*, 자기 debug 빌드를 따로 띄워 release(= 사용자·다른 작업)와 사용자 데이터를 분리해 검증할 수 있다.

> **Agent 는 이것을 반드시 인지한다**: 내가 tasty 안에서 돌고 있어도, 내 수정은 *별도 debug 인스턴스* 로 띄워 검증한다. 돌아가는 release 를 건드리지 않는다.

#### debug ↔ release 격리 (현재 구현)

> 이 표가 격리 경로의 **기준 설명**이다. 다른 문서는 이 표를 풀-복제하지 말고 "debug 는 별도 루트(`~/.tasty-debug/`)로 격리됨" + 이 문서 링크로 참조한다.

**데이터 루트 자체가 갈린다** — debug 빌드는 `~/.tasty-debug/`, release 는 `~/.tasty/` 를 쓴다. 같은 디렉토리 안의 파일명 접미사가 아니라 **루트 디렉토리 분리**라, 포트·layout·scrollback·state.db·memory.db·plugins 가 통째로 별도다:

| 자원 | release (`~/.tasty/`) | debug (`~/.tasty-debug/`) |
|------|---------|-------|
| IPC 포트 파일 | `~/.tasty/tasty.port` | `~/.tasty-debug/tasty.port` |
| scrollback | `~/.tasty/scrollback/` | `~/.tasty-debug/scrollback/` |
| layout | `~/.tasty/layouts/NN.json` | `~/.tasty-debug/layouts/NN.json` |

- 파일명은 양쪽 모두 동일(`tasty.port` 등) — 구분은 **루트** 가 한다. (`-debug` 파일명 접미사는 쓰지 않는다.)
- 웹훅 리스너 포트는 포트를 정하지 않으면 28429부터 비어 있는 포트를 찾으므로, 사용자 인스턴스가 떠 있어도 검증 인스턴스는 다음 빈 포트에 웹훅 리스너를 띄운다. 실제 포트는 `tasty webhook port`로 확인한다. 고정 포트가 필요하면 `--webhook-port <빈 포트>`로 실행한다. 그 포트가 막혀 있으면 리스너 없이 실행되고 경고 알림이 남는다([리스너 주소와 포트](../features/webhook/index.md#리스너-주소와-포트)).
- `target/debug/tasty` (debug 바이너리)는 `~/.tasty-debug/` 루트를 읽으므로 CLI 조작이 **debug 인스턴스에만** 간다. 사용자의 release 인스턴스는 건드리지 않는다.
- **`TASTY_HOME` env override**: 비어있지 않으면 그 경로를 루트로 강제한다(테스트/샌드박스/다중 인스턴스용) — debug/release 자동 분기보다 우선.
- 구현: `crates/tasty-utils/src/path.rs` (`tasty_home()` — `TASTY_HOME` 우선, 없으면 `cfg!(debug_assertions)`→`.tasty-debug`), `crates/tasty-ipc/src/port_file.rs`, `src/store/scrollback.rs`, `src/core/layout_persistence.rs`.

#### 새 기능 추가 시 적용

- 영속 상태(파일/소켓/포트 등)를 새로 추가하면 **debug/release 분리 패턴을 따른다** (`tasty_home()` 루트 아래에 둔다). 안 그러면 debug 검증이 release 데이터를 오염시킨다.
- 동작은 IPC/CLI 로 트리거 가능하게 만든다 (headless 동작-우선) — 그래야 Agent 가 GUI 없이 검증한다. → [IPC·CLI 제공 기준](api-conventions.md#ipc-메서드를-cli로-제공하는-기준), [헤드리스 IPC 지원 범위](headless-ipc-surface.md)

#### 한계 / 주의

- 격리는 **tasty 의 상태**(루트 디렉토리)만 가른다. OS 열기(브라우저 · 파일 관리자)는 사용자 데스크톱의 이미 떠 있는 브라우저에 닿는다 — 검증 인스턴스는 `TASTY_DEBUG_OS_OPEN_LOG` 와 가짜 브라우저로 띄운다([아래 "tasty 에서 직접 검증"](#tasty-에서-직접-검증), [debug-ipc.md](debug-ipc.md)).
- 격리는 **debug ↔ release** 기준이다. 두 debug 인스턴스를 동시에 띄우면 같은 `~/.tasty-debug/` 루트(포트파일 `~/.tasty-debug/tasty.port`)를 공유하므로 충돌한다 — 이때는 `TASTY_HOME` 으로 루트를 분리한다. checkout/worktree만 나눠서는 사용자 데이터 경로가 달라지지 않는다.
- release GUI는 데이터 홈 하나에 프로세스 하나만 둔다. release GUI 바이너리를 `TASTY_HOME` 없이 실행하면 사용자의 `~/.tasty`를 쓰는 실행 중 Tasty로 요청이 넘어가 사용자 화면에 새 창이 생긴다. release 검증은 반드시 고유한 `TASTY_HOME`으로 띄운다. 같은 홈의 두 번째 실행 과정은 `<홈>/launch.log`에 남는다([같은 홈의 다시 실행](../features/layout-persistence/index.md#같은-홈의-다시-실행)).
- 단일 실행의 D-Bus 서비스는 debug에서 기본으로 등록하지 않는다. debug 격리 인스턴스로 `gdbus`를 시험하려면 `TASTY_DEBUG_SINGLE_INSTANCE=1`로 띄운다. debug GUI의 두 번째 실행은 이 값과 무관하게 "홈 사용 중" 화면으로 끝난다. 헤드리스 빌드(debug·release)의 두 번째 실행은 넘기지 않는다. writer 잠금이 다른 인스턴스에 잡혀 있음을 확인하면(`Held`) 같은 문구를 stderr에 쓴 뒤 종료 코드 1로 끝나며 실행 중인 인스턴스의 홈을 바꾸지 않는다. 잠금 상태를 확인하지 못하면(`Unchecked`) 지금처럼 부팅을 진행하고, 그 경로는 홈을 바꿀 수 있다.

#### 관련

- [아래 "tasty 에서 직접 검증"](#tasty-에서-직접-검증) — 실제 검증 절차 (cargo run & + CLI 시나리오)
- [`debug-ipc.md`](debug-ipc.md) — debug 전용 IPC (사용자 입력 재현, release 미노출)
- [`e2e-tests.md`](e2e-tests.md) — 테스트 환경 격리 정책
- [사용자 통제와 데이터](../principles/control-and-data.md#사용자에이전트원격-사용자) · [주체](../concepts/actors.md) — 이 원칙의 뿌리

## tasty 에서 직접 검증

대부분의 동작은 tasty CLI로 재현할 수 있다. GUI 검증은 전용 디스플레이를 준비하고,
OS 열기를 실행하는 시나리오는 아래 기록 설정을 먼저 적용한다.

```bash
cargo build --locked --bin tasty
VERIFY_HOME=$(mktemp -d)
env -u TASTY_SURFACE_ID -u TASTY_SESSION_TOKEN -u TASTY_AGENT_ID -u TASTY_LOCALE \
  -u TASTY_PARENT_HOME TASTY_HOME="$VERIFY_HOME" target/debug/tasty --launch &
MY_APP=$!
printf '%s\n' "$MY_APP" > "$VERIFY_HOME/verification.pid"
ready=0
for attempt in $(seq 1 40); do
  if env -u TASTY_SESSION_TOKEN -u TASTY_SURFACE_ID -u TASTY_PARENT_HOME TASTY_HOME="$VERIFY_HOME" target/debug/tasty list info; then ready=1; break; fi
  kill -0 "$MY_APP" 2>/dev/null || break
  sleep 1
done
if [ "$ready" -eq 1 ]; then
  env -u TASTY_SESSION_TOKEN -u TASTY_SURFACE_ID -u TASTY_PARENT_HOME TASTY_HOME="$VERIFY_HOME" target/debug/tasty list surfaces
  env -u TASTY_SESSION_TOKEN -u TASTY_SURFACE_ID -u TASTY_PARENT_HOME TASTY_HOME="$VERIFY_HOME" target/debug/tasty list tree
fi
```

준비 확인 뒤 같은 TASTY_HOME으로 시나리오를 실행한다. 예제의 준비 대기 횟수는 제품
타임아웃을 바꾸는 설정이 아니다. 종료할 때는 기록한 PID가 이 세션이 실행한 프로세스인지
확인한 뒤 `kill "$MY_APP"`과 `wait "$MY_APP"`로 회수한다. GUI의 디스플레이 격리와
OS 열기 기록 설정도 아래 절차에 따라 함께 적용한다.


라이브 검증 전에는 제품 바이너리를 직접 빌드한다. `cargo test -p tasty --lib`는 테스트
하네스를 만들며 `target/debug/tasty`를 갱신하는 명령이 아니다. 루트 유닛 시험을
`--bin tasty`로 좁히면 의도한 lib 시험을 실행하지 않을 수 있으므로 실제 시험 수를 확인한다.
번들 플러그인은 `PROFILE=<profile> just build-plugins`로 준비한다. 공식 플러그인 변경은 [플러그인 빌드 절차](plugin-development.md)에 따라 그 레포 체크아웃에서 빌드해 놓는다.

종료는 직접 실행한 PID와 소유 관계를 확인한 뒤에만 한다. 이름·명령줄 패턴으로 Tasty를
일괄 종료하지 않는다. PID를 잃었다면 [격리 실행 절차](../ai-verification/screenshot-methods.md)의
소유 확인을 따른다. 다른 debug 인스턴스와도 분리하려면 고유한 TASTY_HOME을 사용한다.

**격리 홈도 전용 디스플레이도 OS 열기를 격리하지 않는다 — 검증 인스턴스는 OS 열기를 기록만 하게 띄운다.** 디렉토리 dispatch · 링크 클릭 · "OS 기본 앱으로 열기" 는 `xdg-open`/브라우저를 부르고, 브라우저는 이미 떠 있는 자기 인스턴스에 URL 을 넘기는 원격 제어 채널(DBus · 소켓)을 가져서 `DISPLAY` 와 무관하게 **사용자 브라우저에 탭이 열린다**(격리 `TASTY_HOME` + 전용 Xvfb 로 띄운 인스턴스에서 실제로 났다). 그래서 두 겹으로 막는다:

```bash
SB=$(mktemp -d)                                   # 가짜 브라우저 자리
for n in xdg-open gio open sensible-browser x-www-browser firefox firefox-bin google-chrome chromium; do
  printf '#!/bin/sh\necho "%s $*" >> %s/fake-open.log\n' "$n" "$SB" > "$SB/$n"; chmod +x "$SB/$n"
done
export TASTY_DEBUG_OS_OPEN_LOG="$SB/os-open.log"
export PATH="$SB:$PATH" BROWSER="$SB/firefox"
```

- **`TASTY_DEBUG_OS_OPEN_LOG`** — tasty 자신의 OS 열기 자리가 프로세스를 띄우지 않고 그 파일에 `<via>\t<대상>` 을 붙인다. "무엇이 열리려 했나" 의 판정은 이 줄로 한다. 덮는 자리와 성질은 [debug-ipc.md](debug-ipc.md) "OS 열기를 띄우지 않고 기록하기".
- **가짜 브라우저 `PATH` + `BROWSER`** — tasty 가 띄운 **다른 프로세스**(PTY 안 셸의 `xdg-open`, 스스로 OS 열기를 부르는 plugin — 번들 plugin 은 host 를 거쳐 위 스위치 안이다)는 위 스위치를 안 읽는다. 그쪽이 부르는 것을 기록만 하는 가짜로 가로챈다. 여기 기록이 생기면 그 프로세스가 연 것이다.
- **`BROWSER=`(빈 값)은 막지 않는다.** `webbrowser` 크레이트(1.2.4 소스)는 빈 항목을 건너뛰고 xdg 설정의 기본 브라우저 desktop entry 를 **직접** 실행한다 — `PATH` 앞의 가짜 `xdg-open` 도 거치지 않는다. `BROWSER` 에는 기록하는 가짜의 경로를 준다.

**`--headless` 는 기본 빌드에서 headless 로 동작하지 않는다.** 기본 빌드는 `gui` feature 가 켜져 있고 그 빌드에는 headless 모드가 들어 있지 않아, `--headless` 를 줘도 **GUI 로 폴백해 실제 창을 띄운다.** 로그에 이렇게 남는다:

```
--headless requested in gui build; gui build does not embed headless mode.
Build with --no-default-features to enable headless. Falling back to run_gui.
```

즉 `--headless` 만 믿고 "창은 안 뜬다" 고 가정하면 **세마포어 없이 공용 디스플레이에 창을 띄우게 된다**(실제로 그렇게 밟은 적이 있다). headless 검증에는 `cargo build --no-default-features` 로 만든 바이너리를 쓴다. GUI 가 떠도 되는 검증이면 폴백을 그대로 써도 되지만, 그때는 GUI 검증 규약(디스플레이 직렬화, 종료 확인)을 따른다.

<a id="headless-빌드에서-무엇이-없는가--재기-전에-알아야-할-세-가지"></a>

#### 진짜 headless 로 검증하는 절차

```bash
cargo build --no-default-features
env -u TASTY_SURFACE_ID -u TASTY_SESSION_TOKEN -u TASTY_AGENT_ID -u TASTY_LOCALE \
  TASTY_HOME=<격리 홈> ./target/debug/tasty &
MY_APP=$!
```

GUI와 headless 기본 빌드는 같은 target/debug/tasty 경로를 사용한다.
두 바이너리를 함께 유지해야 하면 별도 CARGO_TARGET_DIR을 사용하고 실제 실행 경로를 기록한다.
mtime을 보존해 사본을 되돌린 뒤에는 Cargo가 옛 산출물을 최신으로 보지 않도록 주의한다.
자식의 준비·소유 확인과 회수는 위 절차를 따른다.

#### headless 빌드에서 확인할 것

헤드리스는 부팅 시 플러그인 메타데이터를 읽고 필요한 프로세스를 나중에 시작한다.
plugin enable은 지목한 플러그인, surface kind 생성은 활성 소유자, namespace 호출은
활성 소유자와 해당 IPC hook에 필요한 활성 extension을 준비한다. attach mesh mirror는
더 넓은 기동을 요구할 수 있으므로 단일 플러그인의 기동 수 측정과 구분한다.
미등록 prefix·disabled owner·게이트 거부는 기동하지 않고 이미 실행 중이면 재시작하지 않는다.
등록 prefix 안의 오타는 완전한 메서드 목록이 없으므로 소유 플러그인이 판정한다.

선언된 kind는 소유 플러그인이 시작할 때 webview·remote·egui-mesh 모두 등록한다.
선언과 실제 등록은 다른 상태다. 플러그인이 아직 시작되지 않았으면 매니페스트 선언만으로
생성 가능하다고 판단하지 않는다. 지원 IPC와 미지원 메서드의 -32017 응답은
[헤드리스 IPC 범위](headless-ipc-surface.md)를 따른다. 헤드리스 등록이 로컬 GUI 렌더링을
의미하지는 않는다. 원문 mirror는 클라이언트가 표시한다.

### 자주 쓰는 시나리오

- **PTY 입출력**: `send text` → `read screen` 으로 echo/명령 결과 확인. `read screen`(`surface.screen_text`/`pty.read`)은 dim(ghost-suggestion, 예: Claude Code CLI 가 그리는 미제출 자동완성 제안) 셀을 기본 제외한다 — 제안 텍스트가 실제 입력된 것처럼 오독되는 걸 막기 위함. 제안까지 보고 싶으면 `--show-dim`.
  - `--lines N` 은 **내용 기준 마지막 N 줄**이다 — grid 하단 N 행이 아니다. 내용 아래의 공백 행은 건너뛰고, 화면 내용이 N 에 모자라면 스크롤백에서 채운다. 따라서 N 의 크기와 무관하게 의미가 같고, 빈 결과만으로 surface 종료를 판단하지 말고 `--lines` 없는 전체 화면과 대조한다.
- **레이아웃 저장/복원**: dirty 트리거 발생 → 슬롯 파일 확인(debug 검증이면 `~/.tasty-debug/layouts/NN.json`) → kill → 재시작 → `read screen` 으로 복원 확인.
- **Surface meta**: `surface-meta set/get/list` 로 키-값 확인.
- **Hook/플러그인**: `tasty list hooks` · `tasty plugin list` 로 등록 상태, 호출 결과는 plugin 로그(`<tasty_home>/plugins-logs/`).
- **레이아웃 트리 변형**: split/close/new → `list tree` 로 구조 변화 확인.

### debug 전용 IPC 로만 가능한 검증

사용자 입력 재현(키/마우스 주입, popup 강제 open/close, 도구 메뉴 클릭)이나 렌더 셀 덤프는 release 표면에 없다 — debug 빌드의 `debug.*` 로 구동한다. [debug-ipc.md](debug-ipc.md) 참조.

### GUI 시각 검증

색상·정렬·폰트처럼 스크린샷이 필요한 변경은 CLI 만으로 잡지 못한다 — [`ai-verification/screenshot-methods` 시각 판정 체크리스트](../ai-verification/screenshot-methods.md#시각-판정-체크리스트) 체크리스트를 따른다.

**스크린샷은 OS 화면 캡처(`screencapture` / PowerShell `CopyFromScreen` 등)보다 tasty 자체 `ui.screenshot` IPC 를 먼저 쓴다.** OS 화면 캡처는 화면 녹화 권한이 필요해 *빌드할 때마다 사용자가 권한을 다시 풀어주지 않는 한 막힌다* — 자기검증 흐름이 권한 프롬프트에서 멈춘다. `ui.screenshot` 은 tasty 가 실제 렌더한 프레임을 권한 없이 PNG 로 떨구므로 자동 검증에 적합하다(다른 윈도우 가림·포커스 상태에도 영향 없음).

**다만 `ui.screenshot` 이 모든 화면의 상위 채널은 아니다 — 무엇을 그리느냐가 어느 캡처로 보이느냐를 정한다.** native WebView 로 그리는 surface(`markdown` · `html`)는 wgpu 스왑체인 **밖의 OS 자식 윈도우**라 그 캡처에 담기지 않는다(찍으면 그 자리에 host 가 그린 빈 boundary 타일만 나온다). 그쪽은 OS 화면 캡처가 폴백이 아니라 **유일 채널**이다. 대상별로 어느 채널에 있고 어느 채널에 없는지는 [`ai-verification/screenshot-methods`](../ai-verification/screenshot-methods.md) 맨 위 표가 정본이다 — **"OS 캡처는 최후 폴백" 으로만 읽으면 그 두 kind 의 시각 검증을 통째로 건너뛰게 된다.** 호출법·격리 실행도 같은 문서.

### Linux 개발 환경

tasty 를 개발하는 AI 에이전트용 Linux 환경 가이드. (경로 예시는 Linux dev 머신 기준 — 본인 환경에 맞춰 치환.)

<a id="gui-실행--준비-대기--pid-를-저장한다이름패턴으로-찾지-않는다"></a>
<a id="실행-여부"></a>
<a id="다중-인스턴스--루트를-분리한다"></a>

#### 바이너리 / 실행

개발 바이너리는 target/debug/tasty, 릴리스 바이너리는 target/release/tasty다.
PATH 설치 여부를 가정하지 말고 검증할 파일의 경로를 명시한다.
별도 실행 예제를 반복하지 않고 위의 격리 홈·PID 기록·준비 확인 절차를 따른다.

종료 IPC system.shutdown은 debug 전용이며 CLI 명령은 없다.
격리 인스턴스의 포트로 raw JSON-RPC를 보내거나 소유가 확인된 PID를 회수한다.
공용 ~/.tasty-debug/tasty.port를 임의로 삭제하지 않는다.

#### 빌드 후 재시작

Linux에서 빌드가 실행 파일을 교체해도 실행 중인 프로세스는 이전 파일을 사용한다.
새 동작을 확인하려면 직접 실행한 인스턴스를 회수하고 새 바이너리로 다시 시작한다.

#### 스크린샷

GUI의 ui.screenshot으로 PNG를 저장할 수 있다. hover와 애니메이션은 실제 입력과 타이밍을
재현한 뒤 캡처한다. 조건을 임시로 고정한 캡처는 렌더링 모양만 확인하며 입력 동작의 증거가
아니다. 임시 변경은 원복한다. 자세한 절차는
[시각 판정 체크리스트](../ai-verification/screenshot-methods.md#시각-판정-체크리스트)를 따른다.

<a id="가드를-검증할-때--세-가지-침묵은-다른-물음이다"></a>
<a id="①-양성-대조가-안-죽는다--표적이-아니라-모형부터"></a>
<a id="②-술어가-0-을-낸다--잔여가-아니라-술어부터"></a>
<a id="무엇을-돌렸는지의-단위는-패키지--타깃--필터-다"></a>
<a id="왜-셋을-합치지-않는가"></a>
<a id="시험을-골라-돌릴-때--술어를-철자로-쓰지-마라"></a>
<a id="왜-고르나-그리고-고를-때-먼저-가려야-할-둘"></a>
<a id="소스-철자로-물으면-틀린다"></a>
<a id="바른-술어--cargo-가-이미-적어-둔-d"></a>
<a id="이-시험을-돌리면-하네스가-딸려-오는가--앱-인스턴스가-뜨는가"></a>
<a id="이-술어의-경계--링크를-답하지-판정-대상을-답하지-않는다"></a>
<a id="그리고-d-자신도-철자다"></a>
<a id="안-돈다-에는-두-층이-있다"></a>
<a id="좁은-술어는-언제-맞고-언제-틀리는가--두-줄"></a>
<a id="술어의-실패율은-대상의-문체-다양성에-비례한다"></a>
<a id="그래서-보고는-세-칸으로-한다"></a>
<a id="명시적-제외는-좁은-술어보다-나쁘다"></a>
<a id="멈추는-자리--두-수를-나란히-찍는다"></a>

## 가드가 실제로 검사하는지 확인한다

| 관측 | 확인할 내용 |
|---|---|
| 알려진 위반을 넣어도 통과 | 입력이 실제 검사 범위에 들어오는지, 파서가 위반을 읽는지 |
| 결과가 0 | 정상적으로 대상이 없는지, 수집이나 필터가 전부 제외했는지 |
| 시험을 실행하지 않음 | 패키지·타깃·필터·빌드 조합이 맞는지 |

이 셋을 모두 '통과'로 처리하지 않는다. 저장소에 알려진 위반을 만들기 어려우면 같은
검사 경로를 사용하는 합성 입력으로 검증한다. 캐시나 제외 디렉터리에 넣은 위반이
검사에 닿지 않은 결과는 본문 분석기의 오류를 증명하지 않는다.

빈 디렉터리와 정상 입력, 위반 입력을 각각 확인한다. const 배열이나 포함된 문서 목록은
순회 시작점을 비워도 바뀌지 않으므로 실제 입력을 비운다.
가드가 스스로 실행하지 않았거나 다른 컴파일 오류가 먼저 실패했다면 원하는 검사 결과가 아니다.
변이 전·변이 후·원복 후 결과와 실제 실패 타깃을 기록한다.

### 실행할 시험을 고르는 기준

검증 단위는 패키지·타깃·이름 필터다. `--lib`와 `--bin`은 루트 통합 타깃을 포함하지 않으며
패키지만 골라 실행하면 다른 패키지의 소스 가드를 놓칠 수 있다. `-- --list`와 실행 로그에서
의도한 이름과 개수를 확인한다. 0개 실행은 성공 검증으로 보고하지 않는다.

소스에서 spawn 문자열이 없다고 프로세스를 띄우지 않는다고 단정하지 않는다.
공용 하네스·다른 모듈을 거치는 호출도 확인한다. `.d`는 컴파일에 사용된 파일을 찾는 데
도움이 되지만, 포함된 모든 함수가 실행된다는 증거나 런타임 검사 대상 목록은 아니다.
상대 경로를 정규화하고 실제 산출물의 해시를 사용한다. 오래된 해시 파일을 모두 더해
타깃 수로 보고하지 않는다.

이미 만든 바이너리는 검사 구현과 의존 코드·컴파일 입력·빌드 설정이 바뀌지 않았을 때만
현재 파일을 읽는 검사에 재사용할 수 있다. include_str로 옛 입력을 포함했다면 다시 빌드해야 한다.
자세한 조건은 [CI 가이드](ci-gates.md)를 따른다.

검색은 후보를 찾는 과정이다. 이름으로 찾은 것, 실제 실행할 것, 제외한 것을 구분하고
각 제외 이유를 남긴다. 단순 문자열 검색이 모두를 찾았다는 보장은 없다.
검사 대상 집합은 빠진 경로와 추가된 경로로 대조하고, 같은 빈 집합끼리 일치하는 경우를
막기 위해 하한도 확인한다. 결과 0만으로 환경·상태·성능의 영향을 배제하지 않는다.

## 검사 결과를 읽는다

빌드·테스트·lint 의 통과 여부는 검사 명령 자신의 종료 코드와 출력으로 판정한다.
아래 함정은 실패한 검사를 통과로 읽게 만든다. 판독 결과가 통과로 나왔다는 사실만으로는 판독 방법이 맞았다고 볼 수 없다.

- **파이프라인의 종료 코드는 마지막 명령의 것이다.** `cmd | tail`, `| head`, `| grep -c`, `| awk`, `| wc -l`, `| tee` 뒤에서 읽은 `$?` 는 그 마지막 명령의 결과다. 앞의 검사가 실패해도 0이 나올 수 있다. 명령 이름과 관계없이, 파이프 뒤에서 종료 코드나 수를 읽는 형태 전체가 이 규칙의 대상이다. 파이프 오른쪽이 먼저 닫히면 앞 명령이 중단될 수도 있다([셸 스크립트 규약](shell-scripts.md)).
- **긴 명령은 로그 파일로 보내고 자기 종료 코드를 함께 남긴다.**

  ```bash
  { cargo test --workspace --locked --no-fail-fast > LOG 2>&1; echo "rc=$?" >> LOG; }
  ```

- **수를 필드 위치로 읽지 않는다.** 로그 파일에서 읽어도 마찬가지다. 구분자 정의에 따라 빈 필드가 생기면 위치가 밀린다.

  ```console
  $ echo 'test result: FAILED. 25 passed; 1 failed; …' | awk -F'[ ;]' '{print $4, $6, $7}'
  25  1
  ```

  `-F'[ ;]'` 는 `; ` 사이에 빈 필드를 만든다. 그래서 `$6` 은 failed 수가 아니라 빈 값이고, 이 값을 더하면 무엇이 실패해도 0이 된다.
- **수를 세지 말고 ok 가 아닌 `test result` 줄을 직접 본다.**

  ```bash
  grep '^test result:' LOG | grep -v '^test result: ok\.'
  ```

  이 출력이 비어 있고, `test result` 줄이 1개 이상이며, 로그의 `rc=` 가 0일 때만 통과로 기록한다. `test result` 줄이 하나도 없으면 통과가 아니다. 대상이 실행되지 않은 것이다.
  통과한 실행에서 `test result` 줄이 실행 타깃(`Running`·`Doc-tests` 줄)보다 많으면 자식 시험 프로세스의 출력이 섞인 것이다. [유닛 테스트 격리](unit-test-isolation.md)의 "시험 바이너리를 자식으로 다시 띄울 때는 출력을 캡처한다"에 따라 그 시험을 고친다.
- **여러 타깃은 `--no-fail-fast` 로 실행한다.** 이 옵션이 없으면 cargo 는 첫 실패 타깃에서 멈춘다. 그 뒤 타깃은 실패가 아니라 미측정이며 `test result` 줄도 남기지 않는다.

예를 들어 다음 두 판독은 실제로 실패를 통과로 보고했다.

- `cargo test … | grep '^test result:' | awk -F'[ ;]' '{f+=$6} END {print f}'` 는 실패한 타깃이 있는데도 failed 0 을 출력했다.
- `cargo clippy … 2>&1 | grep -cE '^(warning|error)'; echo "rc=$?"` 의 `rc=0` 은 grep 의 종료 코드였다. clippy 자신은 컴파일 실패로 rc=101 이었다.

### 측정값을 기록할 때

- **실제로 잰 값만 기록한다.** 재지 않은 항목은 미측정으로 적고, 미측정 값을 대조군이나 정정문에 넣지 않는다. 값에는 등급(측정·유도·추론·미측정)을 그 값을 낸 쪽이 붙인다. 남에게서 받은 값에는 준 쪽의 등급을 요구한다.
- **완주 모수는 `test result` 줄 수다.** 대상 전체가 실행됐는지는 빌드한 시험 바이너리의 `-- --list` 개수와 `passed + ignored`를 대조해 판정한다. 두 실행의 차이를 타깃에 귀속할 때는 완주 로그의 타깃별 수를 비교하고, 키는 빌드 해시를 뺀 타깃 이름에 출현 순번을 붙인다.
- **"이것뿐"·"유일"은 술어를 뿌리 전체에 돌린 뒤에만 쓴다.** 선언을 세는 술어는 빌드한 시험 바이너리의 `--list` 총수와 맞춘다. 줄 단위 정규식이나 awk로 중첩 구조(mod 귀속 등)를 세지 않는다.
- **"이 좌변으로 세라"는 base와 HEAD 양쪽에서 돌려 본 뒤 처방한다.** 두 결과의 차가 그 변경이 지우거나 더한 자리와 같은지 확인한다.
- **모수는 물음마다 다시 잡는다.** 직전 push 지점이나 다른 물음에서 쓴 차분을 재사용하지 않는다. 어떤 변경이 기준 브랜치에 아직 없는지는 기준 브랜치 전체를 모수로 커밋 제목을 대조해 판정하고, `git cherry`(patch-id) 비교로 바꾸지 않는다.
- **손으로 옮긴 git 해시는 `git cat-file -t <해시>`로 실재를 확인한다.**
- **`echo "$(cmd) rc=$?"`의 `$?`는 명령 치환 안의 rc다.** 검사 명령 바로 뒤에서 `r=$?`로 받아 둔다.

## 커밋별로 검사할 때

커밋마다 단독 트리를 검사할 때는 `git rebase --exec`(`-x`) 안에서 시험을 돌리지 않는다.
linked worktree 에서 rebase 는 exec 명령에 `GIT_DIR=<공용 저장소>/.git/worktrees/<이름>` 을 넘긴다.
이를 상속한 시험이 임시 폴더에서 `git init --bare` 를 실행하면 임시 저장소가 아니라 공용 저장소의
config 에 `core.bare=true` 를 써서 모든 worktree 의 git 이 멈춘다. `git add`·`git commit` 도
바깥 저장소의 index 와 이력으로 들어간다.
병합 단계의 `git rebase --exec 'bash scripts/plugin-bump-fixup.sh'` 는 지금 저장소의 커밋을 고치는 도구라
exec 로 실행한다([Git 훅](git-hooks.md)). 시험과 `cargo test` 는 exec 에 넣지 않는다.

커밋별 측정은 다음 둘 중 하나로 한다.

- 별도 임시 worktree: `git worktree add --detach <tmp> <sha>` 로 만들고 `CARGO_TARGET_DIR=<tmp>/target` 으로
  빌드한다. 다른 worktree 의 target 을 공유하지 않는다. 끝나면 `git worktree remove` 로 지운다.
- 같은 worktree 에서 `git switch --detach <sha>` 로 한 커밋씩 옮겨 가며 검사한다. 미커밋 변경이 없을 때만 한다.

임시 폴더에 Git 저장소를 만들거나 그 안에서 git·게이트 스크립트·git 을 부르는 바이너리를 띄우는 시험은
`tasty_doc_guards::git_env::command` 로 자식을 만든다. 이 함수는 `git rev-parse --local-env-vars` 가
알려 주는 저장소 지정 변수(GIT_DIR·GIT_WORK_TREE·GIT_INDEX_FILE·GIT_COMMON_DIR 등)를 지운다.
작업 트리의 저장소를 읽기만 하는 자리는 넘어온 값이 같은 저장소를 가리키므로 그대로 둔다.
셸 시험은 `scripts/tests/pre-push-git-environment.sh` 처럼 시작할 때 같은 목록을 `unset` 한다.

`bash scripts/tests/git-env-isolation.sh` 는 그런 시험 타깃을 바깥 픽스처 저장소의 GIT_DIR 를 넘긴 채
실행하고, 바깥 저장소의 config·index·HEAD 가 그대로인지 비교한다. 타깃 목록은 스크립트 안에 있으므로
임시 저장소를 만드는 시험 타깃을 더하면 목록에도 더한다. CI 는 이 스크립트를 실행하지 않는다.
`TASTY_E2E_DISPLAY` 가 있으면 GUI 서버를 띄우는 `attach_git_query_loopback` 도 함께 실행한다.

## 길이 가드의 사각 계수가 달라졌을 때

`on_scale_length_literal::the_blind_spots_stay_within_their_budgets`는 비교에서 제외한 자리를 따로 센다.
이 값은 토큰 위반 수가 아니다. 테스트 전용이 아닌 코드의 0과 테스트 전용 길이는 코드가 늘면 함께 느는
관측값이라 수를 기록하지 않고 0이 되면 실패한다. 값 1의 하한과 정규화 좌표는 상한을 넘거나 0이 되면
실패한다. 실패하면 `scan(false)`와 `scan(true)`의 자리 목록을 도입 커밋 전후로 비교하고, 값·호출 위치·
테스트 전용 여부를 확인한다.

갤러리 영역과 갤러리 후보의 (이름·디자인 언급) 분류도 수를 기록하지 않는다. 갤러리에 남은 수 검사는
`the_gallery_share_splits_into_four_kinds`의 두 가지다.

- 같은 숫자의 Theme 값과 같은 인라인 자리(`named_value`)의 상한. 갤러리의 새 인라인 리터럴은 대개
  이 분류로 들어와 상한을 넘으면 실패한다. 상한을 계산으로 바꾸지 않은 것은 의도다.
- 같은 숫자의 Theme 값이 없는 후보(`nameless`)가 0이어야 한다는 규칙. 지금 스케일의 0 아닌 값은
  모두 같은 숫자의 Theme 값이 있어 실패할 수 없다. Theme 값이 없는 size 가 스케일에 생기면 의미가 생긴다.

그래서 다음은 검출하지 못한다.

- 갤러리에 이름 붙은 `LogicalPx`·`PhysicalPx` 상수 선언으로 새 스케일 값이 생긴 것(undecided 분류).
- 같은 setter의 반복이라 전시 후보로 분류되는 새 값.
- 제품 리터럴이 테스트 전용으로 잘못 분류된 것. 테스트 전용 수에 기록이 없어 늘어난 수가 드러나지 않는다.
  분류 오류는 `detector` 픽스처가 형태별로만 잡는다.

파일 피커의 `entry_row`는 고정 열을 뺀 파일명 폭을 0 이상으로 제한한다. 이 0은
새 디자인 치수가 아니므로 기존 zero 분류에 남는다. 좁은 폭 회귀시험의 viewport는
테스트 분류에 남으며, `size-*`에 속하는 400만 계상되고 360은 이 가족에 없다.
제품 기하를 바꾸거나 토큰을 붙여 상한 안으로 맞추지 않는다.

계수를 갱신할 때는 같은 위치의 0을 실제 스케일 값으로 바꾼 대조와 테스트 게이트를
제거한 대조도 실행한다. 전자는 테스트 전용이 아닌 치수의 판정에 나타나야 하고, 후자는 같은 기하가
테스트 전용 분류에서 제품 코드 분류로 이동해야 한다. detector의 두 회귀시험은 이 경계를 검사하며,
전체 source scan은 현재 카탈로그와 파일 단위 test-only 판정까지 포함한다.

## 안티패턴 / 패턴

- ❌ "빌드 통과했어요, 확인해 주세요" — 빌드는 검증이 아니다.
- ❌ "테스트 545개 통과, 커밋했어요" — 테스트가 cover 못 하는 통합 동작이 있다.
- ❌ "동작할 것으로 보입니다" — 직접 돌려본 결과를 보고한다.
- ✅ 수정 → 빌드 → 단위 테스트 → **시나리오 재현** → 결과 보고 → 커밋.
- ✅ "재현 시나리오를 못 만들어 확인 못 했습니다" 를 인정하면 사용자가 검증을 도울 수 있다.

## 관련

- [`documentation-model.md`](../documentation-model.md) §6 — 여기의 변이 절차를 **문서의
  채널 주장**("배선돼 있다 / 이것이 본다")에 적용하는 규칙의 정본. 변이를 못 붙일 때
  무엇을 대신 적는지도 거기다.
- [debug-ipc.md](debug-ipc.md) — debug 전용 IPC (사용자 입력 재현)
- [`ai-verification/screenshot-methods` 시각 판정 체크리스트](../ai-verification/screenshot-methods.md#시각-판정-체크리스트) — 시각 검증 · [`ai-verification/screenshot-methods`](../ai-verification/screenshot-methods.md) — `ui.screenshot`(markdown·html 은 OS 캡처)

## 시간 측정과 실패 진단

검사하려는 것이 시간이면 지연을 측정한다. 값의 변경, 호출 여부, 작업 간 순서가
목적이면 그 사실을 직접 확인한다. 예를 들어 실행 불가능한 명령을 준비하면
명령을 실행한 경우와 실행 전에 취소한 경우를 오류 값으로 구별할 수 있다.

지연 대조군을 새로 쓸 때는 다음을 각각 확인한다.

- 검사 대상과 같은 자원이 바빠지면 대조군도 느려진다.
- 검사 대상 코드에만 지연을 넣으면 대조군은 변하지 않는다.
- 부하가 없는 상태의 변동이 판정 기준보다 작다.

CPU 실행 시간이나 sleep 지연은 디스크 writeback 때문에 느려진 IPC의 대조군이 될 수 없다.
`load average` 하나에도 서로 다른 대기 원인이 섞인다. 적절한 대조군이 없으면
성능 회귀인지 환경 문제인지 구분하지 못했다고 쓴다. 측정 중 패닉이 나도 수집한 값이
남도록 정리 경로에서 출력한다. 불확실한 구간은 `Undecidable`로 처리한다.

PTY 종료 검사의 실패 메시지에는 검사 반복 횟수, 자식 생존 여부, 화면 꼬리를 남긴다.
`alive=true`이면 자식이 아직 종료되지 않은 것이고, `alive=false`이면 종료 감지 경로를 본다.
PTY owner가 `Reaped`인데 대기가 만료됐다면 종료가 상한 바로 뒤에 발생했는지 확인한다.
화면에 입력이 보이는 것만으로 셸 기동을 확인하지 않는다. PTY 자체가 입력을 에코할 수 있다.
에코 판정에 쓰는 입력 문자열과 테스트가 보내는 문자열은 함께 갱신한다.

### 준비 부족과 제품 결함을 구분한다

같은 오류가 준비 부족과 코드 결함 양쪽에서 나올 수 있으면 실패 메시지에 구분 근거와
복구 명령을 붙인다. 번들 플러그인을 쓰는 테스트의 `Method not found`는 실행 파일 옆
`builtin-plugins/`에 공식 플러그인이 준비됐는지도 확인한다. `staged_bundle_note`는 등록한 스위트에서
매니페스트가 없는 공식 플러그인 id를 들고 준비 명령(`just build-plugins`)을 안내한다.
매니페스트 유무만 보며 실행 파일이나 계약 호환까지 검출하지는 않는다.
관련 없는 테스트까지 초기화 단계에서 중단하지 않고 실제 실패 지점에 진단을 붙인다.
하네스 안에서 cargo를 다시 실행하면 외부 cargo와 락을 다툴 수 있으므로 자동 빌드는 하지 않는다.

거절 로그는 반환 전에 찍힐 수 있다. 호출을 막았는지 확인하려면 반환 뒤의 완료 기록과
실제 호출 횟수를 함께 본다. Self-attach 검사는 요청별 완료 기록, connector 0회,
점유가 생기지 않았음, 정상 대상 재attach를 확인한다. RTT는 진단값으로만 남긴다.
유한 로그 버퍼에서 기록이 사라졌다면 관측 실패로 처리한다.

### OS 열기와 지연 주입

검증용 debug 인스턴스는 `TASTY_DEBUG_OS_OPEN_LOG`를 지정한다.
호스트가 OS에 열기를 요청하는 대신 `<via>\t<대상>`을 기록한다.
환경변수가 비어 있거나 파일 기록이 실패해도 실제 앱을 열지는 않는다.
격리 홈만 설정하면 이미 실행 중인 사용자 브라우저로 요청이 전달될 수 있다.
PTY나 별도 프로세스의 열기는 같은 범위가 아니므로 가짜 `BROWSER`/`PATH`도 함께 사용한다.
현재 markdown 외부 링크는 호스트를 경유한다. 임의의 다른 프로세스까지 이 스위치로
통제한다고 주장하지 않는다. 자세한 설정은 [debug IPC](debug-ipc.md)를 따른다.

지연을 의도적으로 넣어 검출력을 확인할 때는 그 실행에서 보정한 기준선과 같은 단위로
주입량을 정한다. `tasty-latency-control`은 기준선의 `2 × MUTATION_MARGIN`배를 주입하고,
비교용 가상 표본에도 같은 양을 더한다. 기준선·주입 시간·예상 비율을 통과 때도 남긴다.
고정 주입 시간만 늘리면 더 느린 환경에서 다시 검출력이 부족해진다.
이 방식은 양성 대조의 검출력을 보장하기 위한 것이다. 부하가 크면 주입 시간도 늘어난다.

음성 대조는 회차마다 주입 전·주입 뒤·직후 재측정의 세 표본을 잰다.

- 주입 뒤 표본이 `MUTATION_MARGIN` 아래이면 통과한다.
- 주입 뒤 표본이 문턱 이상이고 앞이나 뒤 표본도 정상 구간(2배)을 벗어났으면 크기와 관계없이 판단 불가 회차다. 다음 회차를 잰다. 시험 도중 시작한 부하에서 이웃 표본이 기준선의 10~19.3배까지 관측돼 크기로 부하와 결함을 가를 수 없었다.
- 앞뒤 표본은 정상 구간인데 주입 뒤 표본만 문턱 이상이면 대조군이 측정 경로를 지난 것으로 보고 바로 실패한다.
- 모든 회차가 판단 불가이면 `[latency-control] 측정 생략` 한 줄을 표준 오류에 직접 쓰고 통과한다. 이 줄은 기본 `cargo test` 실행에서도 보인다. 회차별 판단 불가 사유와 `skipped`·`undecidable` 값은 `--show-output`이나 `--nocapture`로 실행할 때만 보인다.
- 주입 뒤 표본 하나의 측정 시간 안에만 머무른 부하는 결함과 구분하지 못한다.
- 이웃 표본을 2배 이상으로 흔드는 부하에서는 대조군이 측정 경로를 지나는 결함도 판단 불가 회차가 되어 측정 생략으로 끝날 수 있다. 시험 도중 시작해 계속되는 부하가 대표적인 예이고, 시험 전부터 걸린 고른 부하에서도 일부 회차가 그랬다. 이 조건에서는 그 결함을 검출하지 못한다.
- 주입의 영향이 직후 재측정이나 다음 회차의 주입 전 표본까지 남는 결함은 검출하지 못한다. 이 결함은 환경과 관계없이 매 실행 측정 생략으로 끝나므로, 조용한 조건에서도 측정 생략 줄이 매번 보이면 대조군 구현을 확인한다.
- 합성 표본으로 판정 함수를 확인하는 양성 대조는 측정 생략과 관계없이 실행한다.

주입한 지연이 실제로 흘렀는지도 단언한다. 주입이 사라지면 음성 대조는 아무것도 재지 않고 통과한다.
