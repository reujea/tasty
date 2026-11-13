# 클립보드 (Clipboard)

- **Status**: Implemented
- **주체**: 로컬 사용자 (복사/붙여넣기는 사용자 행동)
- **설계**: [플러그인 권한 모델](../../dev-guide/plugin-permissions.md) (뷰어의 plugin 직접-read 모델)
- **코드**: 시스템 클립보드 `arboard`(네이티브 Wayland 터미널은 `src/clipboard/terminal.rs`); 복사/붙여넣기/선택 = `src/view/main/`
- **화면**: 현재 클립보드 뷰어는 [clipboard-viewer plugin](../../plugins/clipboard-viewer/index.md)

## 목적

터미널 텍스트의 **복사/붙여넣기/선택**, OSC 52 클립보드 설정. 복사/붙여넣기는 사용자 행동이라 토스트로 피드백하고, 에이전트(IPC) 경로는 사용자 시각 상태를 건드리지 않는다. OSC 52 쓰기는 토스트를 낸다 — 보이지 않는 클립보드 쓰기를 사용자에게 보이게 하는 목적이다(PTY 출력이라 origin 을 가를 수 없어 에이전트가 셸에 찍게 해도 뜬다 — [toast](../../design/systems/toast.md) 트리거 정책의 알려진 예외).

현재 클립보드 내용만 [clipboard-viewer plugin](../../plugins/clipboard-viewer/index.md)이
읽기 전용으로 보여준다. 클립보드를 주기적으로 수집하거나 과거 내용을 저장하지 않는다.

## 내부 동작

### 복사 / 붙여넣기 — KeybindingSettings 경유

`copy`/`paste` 바인딩 목록 중 하나와 매칭되면 동작(다중 바인딩). 기본값(tasty 프리셋)은 OS 와 무관하게 세 조합을 모두 묶는다(`ctrl+c`·`alt+c`·`ctrl+shift+c`, paste 동형). OS 별 조합은 Mac/Windows/Linux 프리셋을 고를 때의 값이다. 바인딩 편집은 [keybindings](../keybindings/index.md). 위치 기반 매핑은 [key-mapping](../../design/policies/key-mapping.md).

- **소프트 랩 인지 복사**: 셸이 너비에 맞춰 자동 줄바꿈한 라인은 복사 시 한 줄로 합쳐지고, 진짜 hard newline 은 보존.
- **붙여넣기**: bracketed paste(DECSET 2004) 지원. 텍스트 없고 이미지가 있으면 PNG 로 저장 후 경로를 붙여넣기(AI 에이전트가 이미지 참조 가능). 로컬 이미지는 요청마다 별도 임시 디렉터리에 저장해 같은 파일명의 붙여넣기가 서로 덮어쓰지 않는다. 받는 셸이 나중에 읽을 수 있도록 경로를 입력한 파일은 임시 디렉터리 정리 시점까지 유지한다. 아래 "로컬 이미지 붙여넣기 저장" 절의 규칙을 따른다.
- **Paste 후 Ctrl+C 보호(500ms)**: paste 직후 500ms 내 Ctrl+C 는 무시(SIGINT·복사 안 함) — Ctrl+V 옆 키 오타로 입력을 날리는 사고 방지, 무시 시 토스트.

### 접근 실패와 피드백

호스트 클립보드는 초기화 실패를 보관하고 읽기를 `Result<Option<T>, ClipboardError>`,
쓰기를 `Result`로 전달한다. 해당 형식이 없는 경우만 `None`이며 접근 실패와 구분한다.
선택·줄바꿈 없는 복사·vi register·링크·경로·서피스 ID 복사는 쓰기 성공 뒤에만 복사
토스트를 표시한다. 실패는 기존 Info(접근 불가·미지원) 또는 Error(전송 실패) 토스트로
알린다. 선택이 있는 Copy는 실패해도 처리된 입력이므로 Ctrl+C를 PTY의 SIGINT로
넘기지 않는다. 선택과 포커스는 유지하며 vi 복사도 실패하면 모드를 유지한다.

붙여넣기 읽기 오류는 한 번 알리고 입력을 보내지 않는다. 텍스트 형식이 없을 때만
이미지를 읽으며 빈 텍스트나 빈 클립보드는 조용한 no-op이다. 오류 뒤 bracketed-paste
시작·종료 바이트나 일부 텍스트를 보내지 않는다. mirror 터미널에서 원격 캡처나 로컬 캡처 경로를 다루는 예외는 [원격 스크린샷](../remote-screenshot-clipboard/index.md#mirror-터미널에-붙여넣기)에 있다.

Linux 호스트가 X11·XWayland로 떴으면 터미널 텍스트와 Explorer 파일 전송은 X11 경로다. 네이티브 Wayland로 떴으면 터미널 텍스트는 아래 data device로 오가고 Explorer 파일은 여전히 X11 경로만 쓴다. `DISPLAY`가 있으면 Tasty는
X11로 뜨고([백엔드 선택](../../design/systems/linux-display-backend.md)) Wayland 앱과의 왕복은 XWayland가 잇는다. 격리 GNOME 세션에서
GTK 앱·GNOME Files와 양방향으로 확인했고 실제 사용자 세션의 확인을 대신하지는 않는다([측정 한계](../../ai-verification/gnome-session-verification.md#정리와-한계)). 네이티브 Wayland(`DISPLAY`가 없거나
그 X 서버에 연결하지 못함)에서는 터미널 복사·붙여넣기와 egui 입력 위젯이 Tasty의 data device로 외부 앱과
주고받는다. 다른 앱의 선택은 winit 스레드를 막지 않고 읽어 요청한 창이 키보드 포커스를 유지하고 요청 때 포커스였던 surface가 그대로일 때만 넣는다
([데이터 장치](../../design/systems/linux-display-backend.md#네이티브-wayland의-데이터-장치)). 그 장치가 없는 컴포지터에서는
외부 앱과 주고받지 못하고 원인과 해결책을 warning 토스트로 알린다. `DISPLAY`가 없으면(XWayland 없음)
`toast.clipboard_wayland_unsupported`, `DISPLAY`의 X 서버에 연결하지 못해 내려갔으면
`toast.clipboard_wayland_x_failed`다([표시 경로 안내](../../design/systems/linux-display-backend.md#표시-경로-안내)).
장치가 있으면 vi `"*` register(primary selection) 쓰기만 외부 앱에 닿지 않고, 같은 원인 구분으로
`toast.clipboard_primary_wayland_unsupported`·`toast.clipboard_primary_wayland_x_failed`를 보인다.
OSC 52 쓰기·읽기, IPC `clipboard.set_text`, 캡처 게시도 같은 장치를 쓴다. OSC 52 읽기는 다른 앱의 선택을
읽는 동안 기다렸다가 답한다. plugin 자체 클립보드는
별도 경로다. 원격 출처·내부 파일 목록은 [Explorer 파일 작업](../../surfaces/explorer/file-operations.md#os-파일-클립보드)을 따른다.

### 로컬 이미지 붙여넣기 저장

로컬(비-mirror) 터미널의 이미지 붙여넣기는 PNG 인코딩까지 View 에서 하고, 임시 디렉터리 생성과 파일 쓰기는 App 의 이미지 worker(`src/app/image_jobs.rs`)가 한다. GUI 이벤트 루프는 파일 I/O 를 기다리지 않는다. PNG 인코딩은 아직 View 의 호출 스택에서 한다. 네이티브 Wayland 에서 다른 앱의 `image/png` 를 받으면 그 PNG 를 RGBA 로 푸는 디코딩도 winit 스레드의 같은 호출 스택에서 한다. 읽기 상한 64 MiB 는 압축 바이트의 상한이고 디코딩·재인코딩에 드는 CPU 시간의 상한은 아니다.

- **계약**: 붙여넣기 시점의 대상 binding(surface 와 activation·자원 세대), bracketed 여부, 파일 이름, PNG 바이트를 고정한다(`image_jobs::paste::PendingImageSave`). 붙여넣기 뒤 포커스가 바뀌어도 대상은 바뀌지 않는다.
- **상한**: worker 스레드 하나가 접수 순서대로 한 건씩 실행한다. GUI 서버가 받은 원격 캡처의 저장·디코딩([remote-screenshot-clipboard](../remote-screenshot-clipboard/index.md))도 같은 worker 와 건수 상한을 쓴다. 결과를 회수하지 않은 작업은 최대 8건(`MAX_JOBS`), 메모리에 든 입력 바이트 합은 최대 256 MiB(`MAX_HELD_BYTES`)다. 넘으면 저장하지 않고 요청한 창의 그 터미널 범위에 경고 토스트(`toast.image_paste_busy`)를 띄운다.
- **완료**: App 이 원 대상의 세대와 hard 점유를 다시 확인한 뒤에만 경로를 입력한다. 터미널이 닫혔거나 같은 번호의 새 터미널로 바뀌었거나, 다른 client 가 점유 중이면 입력하지 않는다.
- **큰 이미지**: 대기·실행 중인 작업이 없으면 256 MiB 를 넘는 이미지 한 건도 저장한다. 그 건이 끝날 때까지 다른 요청은 상한에 걸려 거절된다.
- **입력 순서**: 경로는 저장이 끝난 시점에 입력한다. 저장이 끝나기 전에 친 글자는 경로보다 먼저 들어갈 수 있다(mirror 이미지 업로드와 같다). 이미지 붙여넣기 두 건은 접수 순서대로 입력한다.
- **파일 수명**: 경로를 입력한 파일은 받는 셸이 나중에 읽으므로 남긴다. 입력하지 않은 파일(대상 소멸·교체·점유, engine 소멸, 종료 중 도착한 결과)은 그 자리에서 디렉터리째 지운다.
- **실패**: 디렉터리 생성·쓰기 실패는 경고 로그와 오류 토스트(`toast.image_paste_save_failed`)로 알린다. 요청한 View 가 닫혔으면 안내만 버리고, 저장과 경로 입력은 원 대상 기준으로 그대로 진행한다.
- **종료**: 종료가 시작되면 새 저장을 받지 않고 시작하지 않은 저장은 실행하지 않는다. 종료 기한(5초) 안에 worker 를 회수한다.

### 선택 좌표와 내용 일관성

`SelectionPoint`는 보존 이력의 안정된 행 좌표와 content epoch를 가진다. 선택·word
확장·vi 이동은 표시면의 `TerminalReadView`에서 계산하고 복사는 같은 epoch의 살아
있는 행만 추출한다. 앞쪽 trim으로 삭제된 행이 다른 내용으로 재해석되지 않으며 ED3,
terminal 교체, alternate clear 뒤의 옛 선택은 표시하거나 복사하지 않는다.

IME preedit와 OS 후보창 위치도 같은 epoch·행 좌표를 사용한다. 내용 세대가 바뀌면
기존 조합 anchor와 에코 보정을 폐기한다. IME Commit·단축키 앞 flush·debug Commit은
공통 사용자 전송 경로에서 해당 View를 live로 돌린다. Agent 전송에는 적용하지 않는다.

### 텍스트 선택

마우스 드래그(Normal) / 더블클릭(Word) / 트리플클릭(Line) / vi 복사 모드의 `Ctrl+v`(Block). 선택은 화면↔스크롤백을 넘나들고 전각(CJK) 2셀 폭을 정확히 처리. vi 스타일 키보드 복사 모드(`enter_copy_mode` 액션)는 hjkl 이동·visual 선택·`/`·`?` 검색·`y` 복사를 제공.

#### 복사 모드 진입 결과

`enter_copy_mode`는 단축키와 명령 팔레트에서 실행한다. 요청을 접수하면 결과마다 해당 surface 범위(surface가 없으면 창 범위)의 Info 토스트를 하나 낸다. 진입 이외의 결과는 화면·선택·PTY를 바꾸지 않는다.

| 결과 | 조건 | 토스트 |
|---|---|---|
| 진입 | 포커스된 surface가 terminal이고 대체 화면이 아니다 | `toast.vi_copy_entered` — 이동·선택·복사·종료 키. 복사용 커서는 viewport 맨 위 행의 0열 |
| 이미 활성 | 복사 모드가 이미 켜져 있다 | `toast.vi_copy_already_active` |
| 터미널 없음 | 포커스된 surface가 없거나 terminal이 아니다 | `toast.vi_copy_needs_terminal`. 키를 받는 WebView surface에서는 가려진다(아래) |
| 대체 화면 | terminal이 alternate screen(DECSET 1049 등)을 쓰고 있다 | `toast.vi_copy_blocked_alt_screen` |

진입 토스트의 키는 복사 모드 키 처리(`src/view/main/vi_copy.rs`의 `handle_vi_key`)에 고정된 vi 키이며 `KeybindingSettings`로 바꾸지 않는다. 토스트는 입력을 받지 않으므로 이후 키는 그대로 복사 모드가 처리한다. 대체 화면 판정은 OS 윈도우의 최대화·전체 화면과 무관하다.

모달, 포커스된 팝업, fullscreen stage처럼 단축키 처리보다 먼저 키를 가져가는 입력 격리 상태에서는 요청 자체가 접수되지 않아 토스트도 없다. OS나 IME가 키 이벤트를 앱에 전달하지 않은 경우도 같다.

**한계 — 키를 받는 WebView 위에서는 사유가 보이지 않는다.** markdown·html surface가 키를 받는 중에 진입 키를 누르면 요청은 접수되고(host 포워딩 경로) `toast.vi_copy_needs_terminal`이 그 surface 범위에 쌓인다. 그러나 [토스트](../../design/systems/toast.md#native-webview-위의-토스트) 정책상 지금 키를 받는 WebView는 숨기지 않으므로 카드가 WebView 아래에 가려 사용자에게 보이지 않는다. 진입하지 않고 화면·데이터도 바뀌지 않는다는 점은 같다. 이 판단은 코드와 토스트 정책 문서를 읽은 결과이며 실제 실행으로 확인하지 않았다. 표시 방식은 토스트 설계의 과제로 남긴다.

마우스 선택은 기본적으로 **마우스 트래킹이 꺼진 화면에서만** 동작한다. 앱이 마우스 트래킹(DECSET 1000/1002/1003)을 켜면(vim `:set mouse=a`, htop, Claude Code 등) 마우스 이벤트를 앱에 전달하므로 plain 좌클릭 드래그는 앱으로 보고된다 — 근거: [터미널 (Terminal)](../../surfaces/terminal/index.md).

**트래킹 ON 에서도 `Shift`+좌클릭 드래그로 로컬 텍스트 선택이 가능하다** (xterm/iTerm2 표준 modifier 우회). Shift 여부는 press 시점에 1회만 판정해 release 까지 유지하므로, 드래그 도중 Shift 를 떼도 선택이 깨지지 않는다. `Shift`+더블/트리플클릭은 word/line 선택. 선택 후 복사 단축키로 클립보드에 복사된다. 일반 좌클릭은 계속 앱으로 전달된다 (우클릭 `Shift` 우회는 [터미널 (Terminal)](../../surfaces/terminal/index.md) 의 동일 패턴).

### OSC 52

앱 서비스는 RPC·OSC52 요청 사이에 클립보드 소유자를 유지한다. `clipboard.set_text` 실패는
기존 JSON-RPC `-32000`과 백엔드 사유로만 응답하며 사용자 토스트나 성공 이벤트를 만들지 않는다.

**쓰기(set)**: 터미널 프로그램이 OSC 52 로 시스템 클립보드에 텍스트를 설정할 수 있다(termwiz `SetSelection` → 시스템 클립보드 반영. X11·XWayland 로 뜬 Linux 와 macOS·Windows 는 arboard 소유자, 네이티브 Wayland 로 뜬 Linux 는 이벤트 루프의 data device 소유자다). **쓰기 성공 후에만 성공 토스트를 낸다**(`toast.copied_osc52`, 그 surface 스코프). 네이티브 Wayland 에서 이 성공은 `set_selection` 요청을 보낸 시점이다. 창이 키보드 포커스를 잃은 뒤의 쓰기는 컴포지터가 거절해도 Tasty 는 알 수 없다([Core 쪽 쓰기](../../design/systems/linux-display-backend.md#네이티브-wayland의-데이터-장치)). 실패는 접근 불가·실패 토스트로 알리며 결과 이벤트는 원 터미널 generation에 묶인다 — 보이지 않는 클립보드 쓰기를 사용자에게 보이게 하는 목적이다. 같은 surface 에서 500 ms 안에 반복된 쓰기는 토스트 coalesce 로 한 장에 합쳐진다.

**읽기(query)**: `OSC 52 ; c ; ? ST` 클립보드 읽기 질의는 설정 토글 `general.allow_clipboard_read`(기본 **off**)로 게이트된다. off 면 **무응답**(1바이트도 내보내지 않음) — 터미널 안의 임의 프로그램(원격/SSH 프로세스 포함)이 로컬 클립보드(비밀번호·토큰)를 조용히 탈취하는 것을 차단한다(xterm/iTerm 계열 정책). on 이면 시스템 클립보드를 base64 로 인코딩해 `OSC 52 ; c ; <base64> ST` 로 회신. 네이티브 Wayland 에서 다른 앱의 선택은 비차단으로 읽은 뒤 같은 터미널 세대이고 설정이 그대로 켜져 있을 때만 회신한다([데이터 장치](../../design/systems/linux-display-backend.md#네이티브-wayland의-데이터-장치)). 경로: 터미널 크레이트가 `TerminalEventKind::ClipboardQuery` 이벤트만 발생(설정과 클립보드 접근은 호스트가 담당) → host(`Core::drain_terminal_events`)가 게이트·읽기·인코딩 후 해당 surface 의 PTY 로 `send_bytes`. 설정 UI 는 Terminal › TUI 섹션(토글 + 바로 아래 bordered warning callout). 토스트 없음.

### egui-mesh plugin 의 텍스트 선택 복사 (`egui_copy` capability)

host 가 아니라 plugin 자신의 egui `Context` 로 텍스트를 그리는 kind(`rendering = "egui-mesh"`)는, 매니페스트에서 `egui_copy = true` 를 선언하면 copy 단축키(위 `KeybindingSettings` 바인딩)가 그 surface 에 `Copy` wire 이벤트로 forward된다(`src/adapters/ui/input/shortcuts/copy_paste.rs` → `src/view/main/egui_mesh.rs`). host 자신의 top-level egui `Context` 는 plugin 위젯을 갖고 있지 않으므로 대상이 될 수 없다 — 반드시 포커스된 egui-mesh surface 자신에게 보내야 한다.

plugin 쪽(`tasty-plugin-sdk`)은 이 wire 이벤트를 `egui::Event::Copy` 로 매핑해 자기 `Context::run` 에 흘린다. selectable label/`TextEdit` 등 egui 내장 선택-복사 로직이 텍스트를 만들면 SDK 의 `paint`·`repaint_last` 가 그 값을 `CopyText` 알림으로 host 에 보내고, host 는 plugin id 와 함께 일회성 큐에 담는다([egui-mesh 채널](../../dev-guide/egui-mesh-channel.md)). 네이티브 Wayland 에서는 selection 을 Tasty 프로세스가 소유하므로 plugin 이 직접 쓰지 않는다. 이 큐를 꺼내 OS 클립보드에 쓰는 host 경로는 아직 없다. 메커니즘 자체는 host 코드([`src/view/main/egui_mesh.rs`](../../../src/view/main/egui_mesh.rs) 등)에 남아 있으나, `markdown` 이 webview 로 전환된 뒤([WebView 호스트 계약](../../design/systems/webview.md)) 현재 이를 선언하는 번들 plugin 은 없다 — webview surface 는 native WebView 가 Ctrl+C 를 자체 처리하므로 이 wire 이벤트 자체가 불필요하다.

### egui-mesh plugin 의 붙여넣기 (`egui_paste` capability)

매니페스트에서 `egui_paste = true` 를 선언한 kind 에 포커스가 있으면, paste 단축키(위 `KeybindingSettings` 바인딩)와 팔레트의 붙여넣기는 터미널로 가지 않고 그 surface 에 `Paste` wire 이벤트로 전달된다(`src/adapters/ui/input/shortcuts/copy_paste.rs` `run_paste` → `src/view/main/egui_mesh.rs`). 이벤트에는 클립보드 내용이 없다. plugin 이 이 이벤트를 보고 필요한 형식을 **자기 프로세스에서 직접** 읽는다(위 복사와 같은 docs/dev-guide/plugin-permissions.md 근거). 현재 이를 선언하는 번들 plugin 은 image 다.

### 현재 클립보드 뷰어

지금 시스템 클립보드에 담긴 내용은 [clipboard-viewer plugin](../../plugins/clipboard-viewer/index.md) 이 popup 으로 보여준다. host 백엔드 없이 **plugin 프로세스가 `arboard` 로 직접 read** 한다([플러그인 권한 모델](../../dev-guide/plugin-permissions.md) — plugin 은 비-샌드박스 OS 프로세스라 host 가 OS 클립보드 접근을 막을 수 없으므로, 단발 read 는 host 를 경유하지 않는다). 히스토리 누적·재복사는 없다.

## 인터페이스

- **사용자**: 복사/붙여넣기/선택(위), 현재 클립보드 내용은 plugin 뷰어 팝업.
- **AI Agent / CLI**: 단발 클립보드 **read** 는 각 에이전트 프로세스의 직접 접근 영역(docs/dev-guide/plugin-permissions.md) — host 가 read IPC 를 노출하지 않는다. **write** 는 `clipboard.set_text` IPC(`Permission::ClipboardWrite`)/CLI `tasty clipboard set-text <text>` 로 host 가 노출한다 — [remote-screenshot-clipboard](../remote-screenshot-clipboard/index.md) 의 원격 캡처 반영도 같은 `ClipboardSystem` 을 쓰되, 이 IPC 를 거치지 않고 경로 텍스트와 이미지를 함께 올린다(`write_text_and_image`).

## 비-목표

- 클립보드 **히스토리** 누적·재복사.
- 현재 클립보드 **뷰어 UI** — 빌트인 [clipboard-viewer plugin](../../plugins/clipboard-viewer/index.md) 이 popup 으로 제공.
- IME(한글/CJK) 입력 파이프라인 — 별도 영역.

## 관련

- [clipboard-viewer plugin](../../plugins/clipboard-viewer/index.md) · [keybindings](../keybindings/index.md) · [settings](../settings/index.md)
