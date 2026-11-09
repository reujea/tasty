# 원격 스크린샷 → 클립보드 (Remote screenshot to clipboard)

- **Status**: Implemented
- **주체**: 로컬 사용자 (단축키 트리거)
- **설계**: 없음 (기존 [원격 접속 프로필 + Passkey (remote_tool)](../remote-profiles/index.md) attach 채널을 그대로 재사용, 신규 프로토콜/enum 없음)
- **코드**: `crates/tasty-platform/src/screen_capture.rs`(OS 캡처 + `CaptureError`), `crates/tasty-platform/src/macos_permissions.rs`(화면 기록 권한 preflight), `src/app/screenshot_capture.rs`(폴링/스레드·결과 안내), `crates/tasty-settings/src/keybindings.rs`(`screenshot_to_clipboard`), `src/adapters/ui/input/shortcuts/keybinding.rs`(`match_capture_bindings`), `src/app/attach_client/bulk.rs`(원격 전송), `src/app/attach_client/output.rs`(`capture_result` 안내), `src/remote/capture_upload.rs` + `src/remote/server.rs`(`admit_capture_upload`·`prepare_capture`·`publish_capture`·`push_capture_result`, 서버측 수신) + `src/app/image_jobs/capture.rs`(GUI 서버의 worker 실행), `crates/tasty-ipc/src/stream_hub.rs`(`CaptureUploadMsg`), `crates/tasty-ipc/src/method_meta.rs`(`clipboard.set_text`), `src/app/ipc/app_methods.rs`(`ipc_handle_clipboard_set_text`)
- **화면**: 없음 — 트리거는 단축키, 피드백은 요청한 윈도우의 결과 토스트(아래 "결과 안내"; 기존 [토스트](../../design/systems/toast.md) 재사용)

## 목적

포커스된 surface 가 **mirror(원격 attach) 워크스페이스**에 속할 때, 인터랙티브 화면 캡처 결과를 로컬이 아니라 **원격 인스턴스의 클립보드**에 반영한다. 로컬 surface 를 보며 찍은 스크린샷은 로컬 클립보드에 저장 경로 텍스트와 이미지로 올리고, mirror 포커스일 때만 캡처 파일을 attach 채널로 원격에 올려 원격 클립보드에 같은 두 형식으로 올린다 — "찍은 화면이 실제로 보고 있는 머신의 클립보드로 간다"는 사용자 기대를 만족시키기 위함.

## 내부 동작 (headless-valid)

### 로컬 vs mirror 판별

키바인딩 매치 시점에 읽기 projection에서 원 surface와 mirror workspace를 고정하고 `EngineAction::Screenshot`에 원 View를 담는다. App은 원 target이 유효한 경우에만 해당 `EngineSession.remote.pending_screenshot_captures`에 `(mirror_workspace, view)`를 넣는다. 현재 focus를 나중에 다시 조회해 목적지를 바꾸지 않는다.

### 캡처 (OS 네이티브, 인터랙티브)

`about_to_wait` 에서 큐를 drain해 요청마다 백그라운드 스레드로 OS 캡처를 실행(`crate::platform::screen_capture::capture_interactive`, 블로킹 UI 스레드 방지 — App 소유 pending 요청과 결과 채널 사용).

- **macOS**: `screencapture -i` (사용자가 영역 선택, Esc 로 취소 가능).
- **Linux**: Wayland 는 `grim`+`slurp` → `gnome-screenshot -a` 순, X11 은 `gnome-screenshot -a` → `scrot -s` → ImageMagick `import` 순으로 시도한다. 후보는 `PATH` 에서 실행 가능한 파일을 찾아 고른다. `grim`+`slurp` 는 둘 다 있을 때만 후보가 되므로 `slurp` 만 있으면 선택 UI를 열지 않고 다음 후보로 넘어간다.
- **Windows**: 인터랙티브 영역선택에 쓸 만한 OS 표준 동기 CLI 가 없어(Snipping Tool `ms-screenclip:` 은 파일이 아니라 클립보드 이미지로만 출력) PowerShell + `System.Drawing`/`System.Windows.Forms.SystemInformation.VirtualScreen` 로 **전체 가상화면**을 캡처한다(알려진 한계, 인터랙티브 영역선택 아님).
- 저장 위치: `~/.tasty/screenshots/screenshot-<epoch_ms>.png`.

### 결과 분류

캡처 결과는 파일 존재, 종료 상태, 도구가 내는 취소 메시지를 함께 보고 `CaptureError` 로 분류한다. 취소의 종료 코드는 도구와 버전마다 달라, 도구가 취소를 구별 가능한 신호로 알리지 않는 결과는 사용자 취소로 확정하지 않는다.

| 결과 | 판정 | 다음 후보 |
|---|---|---|
| 성공 | 정상 종료하고 파일을 남겼으며 PNG 서명과 마지막 IEND 청크가 맞다 | — |
| 손상 파일 | 정상 종료했지만 PNG 서명이나 IEND 청크가 없다(잘린 파일·다른 형식) | 파일을 지우고 시도 |
| 도구 없음 | 실행 파일이 없거나 실행 권한이 없다. 선택 UI를 열지 않는다 | 시도 |
| 도구 실패 | 비정상 종료했고 취소 메시지가 없다(백엔드 미지원, 저장 실패 등). 남은 부분 파일은 지운다 | 시도 |
| 사용자 취소 (`Cancelled`) | 비정상 종료와 도구의 취소 메시지가 함께 있다 | 멈춤 |
| 출력 없음 (`NoOutput`) | 파일이 없고 취소 신호도 없다. 취소인지 출력 없는 실패인지 구별할 수 없다 | 멈춤 |

- 모든 후보가 없으면 `Unavailable`, 하나 이상 실행했지만 모두 실패하면 각 후보의 실패 사유를 모은 `Tool` 오류다.
- 저장 디렉터리를 만들지 못하거나 저장된 파일을 다시 읽지 못하면(로컬 디코딩·mirror 전송 모두) `Storage` 다. 도구가 지정 경로에 쓰지 못한 경우는 도구 실패로 나타난다.
- 사용자 취소와 출력 없음에서 멈추는 것은 사용자가 이미 닫은 선택 UI 뒤에 다른 도구의 선택 UI를 연달아 열지 않기 위해서다. 두 경우 모두 클립보드를 바꾸지 않는다.

도구별 판정은 다음과 같다.

| 도구 | 사용자 취소 | 출력 없음 | 근거 |
|---|---|---|---|
| `scrot -s` | 종료 코드 1과 `aborting shot` 메시지 | 정상 종료인데 파일 없음 | scrot 1.10 실측: Esc 에 `Key was pressed, aborting shot` 와 종료 코드 1, 저장 경로 오류에 `Saving to file … failed` 와 종료 코드 1, X 디스플레이 없음에 종료 코드 1 |
| `slurp` | 종료 코드 1과 `selection cancelled` 메시지 | 정상 종료인데 선택 영역이 비었음 | 소스 기준. Wayland 도구가 없는 환경이라 실제 실행은 하지 않았다 |
| `grim` | 없음 | 정상 종료인데 파일 없음 | 비정상 종료는 영역 선택 이후의 도구 실패로 보고 다음 후보를 시도한다 |
| `gnome-screenshot -a` | 없음 | 정상 종료인데 파일 없음 | gnome-screenshot 41.0 실측: Esc 에 종료 코드 0과 파일 없음. 종료 상태로 취소와 실패를 구별할 수 없다 |
| `import` | 없음 | 정상 종료인데 파일 없음 | 설치되지 않은 환경이라 실제 실행은 하지 않았다 |
| `screencapture -i` (macOS) | 없음 | 파일 없음(종료 상태와 무관) | 취소의 종료 코드가 macOS 버전마다 달라 파일이 없으면 출력 없음으로 둔다. 실 기기 측정은 하지 않았다 |
| PowerShell (Windows) | 없음(선택 UI 없음) | 없음 | 비정상 종료, 그리고 정상 종료인데 파일 없음은 모두 도구 실패다. 실 기기 측정은 하지 않았다 |

### 화면 기록 권한 (macOS)

macOS 에서 `screencapture` 는 **화면 기록(Screen Recording) 권한**을 요구한다. 권한이 없으면 파일을 전혀 남기지 않으므로, "파일 존재 여부" 판정만으로는 **사용자 취소와 구분되지 않는다** — 사용자에겐 둘 다 "아무 일도 안 일어남"으로 보인다.

그래서 권한 미승인은 캡처 **직전** `CGPreflightScreenCaptureAccess()` 가 false 일 때 `PermissionDenied` 로 따로 분류하고 도구를 실행하지 않는다. 처리는 `toast.screen_recording_permission_required` 안내 토스트와 `warn!` 이다. 권한이 있는데 파일이 없으면 위 "결과 분류" 의 출력 없음이다.

preflight 는 **캡처 직전**에 부른다 — 부팅 시점 값을 캐시해두면 그 사이 사용자가 시스템 설정에서 권한을 바꾼 경우를 잘못 판정한다. 안내 토스트는 아래 "결과 안내" 규칙대로 요청한 View 에 띄운다(다중 윈도우 세션에서 엉뚱한 윈도우에 뜨지 않게).

권한 프롬프트는 자동으로 뜨지 않는다. 사용자가 설정 > 일반 > 권한에서 [모든 권한 요청하기]를 눌렀을 때만 나타난다([macOS 권한](../macos-permissions/index.md)). 그 화면을 한 번도 열지 않았다면 캡처 시점에 권한이 없을 수 있고, 그때는 캡처가 "화면 기록 권한이 필요합니다" 안내로 끝난다. macOS가 아닌 환경에서는 이 검사를 하지 않으며 preflight가 항상 승인됨을 반환한다.

### 로컬 케이스

캡처 성공 시 캡처 워커가 저장 파일을 읽어 PNG 를 디코딩하고(`clipboard::capture_image::decode_png`), UI 스레드는 `ports::clipboard::publish_text_with_image` 로 저장 경로 텍스트와 이미지를 로컬 클립보드에 함께 올린다.

### 클립보드 게시 (경로 텍스트 + 이미지)

로컬과 원격 서버가 같은 규칙으로 게시한다.

- `ClipboardSystem::write_text_and_image` 가 한 번의 게시로 두 형식을 올린다. arboard 의 setter 는 부를 때마다 이전 형식을 지우므로 쓰지 않고, Explorer 파일 클립보드와 같은 플랫폼 소유자(`src/clipboard/files.rs::write_text_and_image`)를 쓴다.

| OS | 올리는 형식 |
|---|---|
| Linux (X11·XWayland) | 한 CLIPBOARD 소유자가 `image/png`(저장 파일과 같은 바이트), `UTF8_STRING`, `text/plain;charset=utf-8` 를 함께 제공한다. PNG 가 X11 요청 하나(`maximum_request_bytes` − 64)를 넘으면 소유하지 않고 실패한다(INCR 로 나눠 보내지 않는다) |
| Windows | 클립보드를 한 번 열어 `CF_UNICODETEXT`, 등록 형식 `PNG`, `CF_DIB`(32비트 BGRA, 아래 행부터)를 올린다 |
| macOS | 일반 pasteboard 를 비운 뒤 `NSPasteboardTypeString` 과 `NSPasteboardTypePNG` 를 올린다(`tasty-platform::pasteboard`) |

- 결과는 `CaptureCopy` 셋 중 하나다. 경로 텍스트와 이미지를 함께 올리면 `TextAndImage`. 디코딩 실패, 함께 게시 실패, 이미지 미지원 어댑터면 경로 텍스트만 다시 써서 `TextOnly`(이미지 실패 사유 포함). 텍스트도 쓰지 못하면 `Failed`. 일부 형식만 남았을 수 있는 실패를 성공으로 보고하지 않으려고 텍스트를 다시 쓴다.
- 헤드리스 빌드의 클립보드 어댑터는 이미지를 지원하지 않고 텍스트 쓰기도 실패하므로 `Failed` 다. 성공을 가장하지 않는다.
- 터미널 붙여넣기는 텍스트 형식을 먼저 읽으므로(`ClipboardContext::read_paste`) 저장 경로가 들어가고, 이미지 붙여넣기를 받는 앱은 이미지 형식을 읽는다. 원격(mirror) 터미널의 예외는 아래 "mirror 터미널에 붙여넣기"에 있다. Clipboard Viewer 는 열 때 읽는 snapshot 에 Text 와 Image 를 함께 보인다.
- 클립보드 소유는 다른 앱이 복사하거나 Tasty 가 다른 내용을 쓸 때까지 이어진다. Linux 는 그동안 소유 스레드가 PNG 바이트를 들고 있다.
- 디코딩 비용: 3840×2160 PNG(무작위 픽셀, 33MB)에 약 26~30ms(이 저장소의 dev 프로필은 의존 크레이트를 최적화한다). 로컬은 캡처 워커에서 디코딩한다. GUI 원격 서버는 저장·재읽기·PNG 디코딩을 App 의 이미지 worker(`src/app/image_jobs.rs`, 아래 "서버측 처리")에서 하므로 서버 화면이 그동안 멈추지 않는다. 클립보드 게시는 macOS·Linux 는 worker 에서, Windows 는 이벤트 루프에서 한다(아래 "스레드 제약"). 헤드리스 서버는 이미지를 디코딩하지 않고 저장까지 그 자리에서 끝낸다.
- 스레드 제약: Windows 의 클립보드 소유 창은 호출 스레드마다 만드는 메시지 전용 창(`thread_local`)이다. 다른 프로그램이 `EmptyClipboard` 하면 소유 창에 `WM_DESTROYCLIPBOARD` 가 가는데, worker 스레드는 작업을 기다리며 막혀 창 메시지를 처리하지 않으므로 소유자가 되면 그 프로그램의 복사가 늦어지거나 멈출 수 있다. 그래서 Windows 는 worker 가 저장·디코딩만 하고, 게시는 완료를 받은 이벤트 루프(메시지를 처리하는 스레드)가 한다(`image_jobs::capture::PUBLISH_ON_EVENT_LOOP`). macOS 는 `objc2-app-kit` 이 `NSPasteboard` 를 main thread 전용으로 표시하지 않고, Linux X11 은 선택 소유를 별도 스레드가 맡으므로 worker 에서 게시한다. Windows 의 이벤트 루프 게시와 macOS 의 worker 게시는 실 기기에서 재지 않았다.
- X11 은 한 요청에 담기는 크기(약 16 MiB)를 넘는 PNG 를 올리지 못해 경로만 올린다(`TextOnly`).

### 원격(mirror) 케이스 — 기존 attach 채널 확장 (신규 프로토콜 없음)

**채널**: attach 세션의 기존 writer/reader와 `StreamTag::Control`을 재사용한다.
`StreamControl` enum에 없는 이벤트를 별도로 파싱해 아래 세 메시지를 처리한다.

- `capture_chunk` { `upload_id`, `seq`, `total`, `data_b64` } — 캡처 파일을 raw 700KiB 청크 단위로 base64 인코딩해 전송(base64 인플레이션 후에도 `MAX_FRAME_LEN`(1MiB) 아래 유지).
- `capture_commit` { `upload_id`, `file_name` } — 마지막 청크 뒤 1회, 업로드 종료를 알림.
- `capture_result` { `upload_id`, `ok`, `path?`, `reason?`, `image?`, `image_reason?` } — 서버(원격)→client 회신. `ok` 는 경로 텍스트를 원격 클립보드에 쓴 것이고, `image` 는 이미지도 함께 올렸는지다. 저장은 했지만 클립보드에 쓰지 못하면 `ok:false` 와 함께 저장 `path` 를 싣는다. `image` 를 싣지 않는 옛 서버의 `ok:true` 는 경로만 쓴 성공으로 읽는다.

client 측(`src/app/attach_client/bulk.rs::forward_capture_to_remote_clipboard`)은 로컬 워크스페이스 id 로 해당 attach 세션을 찾아 청크+커밋을 순서대로 보낸다. 서버(원격 인스턴스) 측은 `stream_hub.rs::pump_inbound` 가 `StreamControl` 파싱 실패 시 `CaptureUploadMsg` 로 재시도해 분류하고, `CaptureUploadRegistry`(`(client_id, upload_id)` 키)에 청크를 누적한다. 커밋을 받으면 GUI 서버는 `src/app/event_handler.rs::apply_capture_upload_msg` 에서 `src/remote/server.rs::admit_capture_upload` 로 접수한 뒤 `src/app/image_jobs/capture.rs` 의 `store_capture_off_thread` 로 worker 에 넘기고, 헤드리스 서버는 `src/remote/server.rs::finalize_capture_upload`(헤드리스 전용)로 그 자리에서 끝낸다.

**서버측 처리**: 접수(`admit_capture_upload`)는 이벤트 루프에서, 저장·디코딩(`prepare_capture`)은 GUI 서버에서 App 이미지 worker 로, 게시(`publish_capture`)는 macOS·Linux 는 worker, Windows 는 이벤트 루프에서, 회신은 `push_capture_result` 가 한다. 헤드리스 서버는 `finalize_capture_upload` 가 모든 단계를 이어서 부른다.
1. 그 client 가 해당 engine의 workspace를 hard 점유 중인지(`OccupancyRegistry::client_holds_workspace`) 확인 — attach 연결 자체가 이미 권한 경계이므로 별도 인증 계층을 새로 만들지 않는다(구조 변경 forward 와 동일한 신뢰 모델).
2. `file_name` 을 **basename 만** 취해(path traversal 방지) `~/.tasty/screenshots/<name>` 에 저장.
3. 저장 파일을 디코딩해 `publish_text_with_image` 로 **원격 인스턴스의** 클립보드에 경로 텍스트와 이미지를 올린다(위 "클립보드 게시"). 텍스트 쓰기는 `clipboard.set_text` IPC 와 같은 `ClipboardSystem` 을 쓰지만, attach 미니 프로토콜은 이 IPC 를 거치지 않고 같은 프로세스 안에서 직접 호출한다(JSON-RPC 왕복이 불필요).
4. 결과를 `capture_result` 프레임으로 client 에 회신. GUI 서버는 worker 가 끝난 뒤 업로드를 시작한 연결(같은 client 번호의 같은 등록)이 그대로일 때만 회신한다. 끊긴 뒤 같은 번호로 새로 붙은 연결에는 보내지 않는다. 그 사이 grant 가 바뀌어도 이미 한 저장·게시의 결과를 그대로 알린다.
5. worker 가 받지 못하면(결과를 회수하지 않은 작업 8건 상한, 종료 중) 저장하지 않고 `ok:false` 와 사유(`server is busy storing other captures` 등)로 바로 회신한다. 업로드 바이트는 디스크 spool 에 있어 메모리 바이트 상한에는 세지 않는다. 같은 worker 가 로컬 이미지 붙여넣기 저장도 접수 순서대로 실행한다.

client 는 `capture_commit` 까지 보낸 업로드를 세션의 `capture_uploads`(`upload_id` → 요청한 View)에 넣고, `capture_result` 를 받으면 그 항목을 꺼내 요청한 View 에 토스트를 띄운다. `ok:false` 는 `attach.toast.mirror_capture_failed`, 이미지까지 올렸으면 `attach.toast.mirror_capture_saved`, 경로만 올렸으면 `attach.toast.mirror_capture_path_only`(이미지 실패 사유), `image` 가 없는 옛 서버의 성공은 `attach.toast.mirror_capture_path_only_old_server` 다.

- 목록에 없는 `upload_id` 의 회신(이미 답한 업로드의 중복 회신, 연결 정리 뒤의 늦은 회신)은 안내하지 않는다. 동시에 올린 여러 캡처는 회신 순서와 관계없이 각자의 요청에 대응한다.
- 회신 전에 연결이 끊기면(`enter_reconnecting`, 끊김으로 인한 mirror 정리, 재연결·재동기화로 transport 교체) 남은 업로드마다 `attach.toast.mirror_capture_unconfirmed` 를 요청한 View 에 띄운다. 사용자가 mirror 를 직접 닫은 경우는 끊김 안내와 마찬가지로 띄우지 않는다.

### mirror 터미널에 붙여넣기

Tasty 의 붙여넣기 단축키는 항상 로컬 클립보드를 읽고, 그 키를 원격 앱에 전달하지 않는다. 그래서 원격 클립보드에만 올라간 캡처는 단축키로 꺼낼 수 없다. 이 틈을 다음 규칙으로 메운다.

- **원격으로 보낸 캡처**: 캡처 워커는 화면을 찍기 전에 로컬 클립보드 내용의 지문(`clipboard::context::paste_fingerprint`)을 구한다. 업로드를 보내면 mirror workspace 별 기록(`RemoteState.mirror_capture_pastes`)에 지문을 남기고, 회신에 저장 경로가 있으면 기록에 더한다. 클립보드 쓰기만 실패한 회신(`ok:false`)에도 경로가 있으면 쓴다.
- **판정**: 그 mirror 의 터미널에 붙여넣을 때 지금 로컬 클립보드의 지문이 기록과 같으면 로컬 내용 대신 원격 경로를 bracketed paste 규칙대로 입력한다. 다르면 사용자가 캡처 뒤에 새로 복사한 것이므로 로컬 내용을 쓴다. 기록은 새 캡처가 덮어쓰고 mirror 를 제거할 때 지운다. 같은 캡처를 여러 번 붙여넣을 수 있다.
- **로컬에서 찍은 캡처**: 로컬 캡처는 이 머신의 저장 경로를 텍스트로 올리므로 원격은 그 경로를 읽지 못한다. mirror 터미널에 붙여넣을 때 텍스트가 이 머신의 캡처 저장 폴더 바로 아래 경로이고 이미지를 읽을 수 있으면, 이미지 붙여넣기와 같이 업로드해 원격 경로를 입력한다.
- **한계**: 내용 지문이라 캡처 전과 같은 내용을 다시 복사하면 바뀌지 않은 것으로 본다. 캡처 전에 로컬 클립보드를 읽지 못하면 그 캡처는 기록하지 않는다. RDP 처럼 원격 클립보드를 로컬로 옮기는 프로그램이 있으면 지문이 달라져 옮겨 온 로컬 내용을 쓴다.

### 결과 안내 (토스트)

단축키 요청의 결과는 **요청한 View** 에만 Window 스코프 토스트로 알린다(`App::toast_screenshot_origin`). 캡처가 사용자 선택을 기다리는 동안 포커스·탭·윈도우가 바뀌어도 대상은 그대로다. 요청한 View 가 닫혔거나 같은 OS 윈도우가 새 View 로 바뀌었으면 다른 윈도우를 대신 고르지 않고 로그만 남긴다. 토스트는 입력 포커스를 가져가지 않는 기존 오버레이다. 경로·실패 사유는 `t_fmt_fit` / `fit_fragment` 로 가운데를 줄여 200자 상한 안에 넣는다.

| 결과 | 종류 | 키 |
|---|---|---|
| 로컬: 경로와 이미지 게시 | Success | `toast.screenshot_copied` |
| 로컬: 경로만 게시(`TextOnly`) | Warning | `toast.screenshot_path_only`(이미지 실패 사유) |
| 로컬: 저장했지만 클립보드 게시 실패(`Failed`) | Error | `toast.screenshot_copy_failed`(저장 경로) |
| 사용자 취소(`Cancelled`) | 없음 | — |
| 파일 없이 끝남(`NoOutput`, 취소일 수 있음) | Info | `toast.screenshot_not_taken` |
| 권한 거절(`PermissionDenied`, macOS) | Warning | `toast.screen_recording_permission_required` |
| 도구 없음(`Unavailable`) · 도구 실패(`Tool`) · 저장 실패(`Storage`) | Error | `toast.screenshot_tool_unavailable` · `toast.screenshot_tool_failed` · `toast.screenshot_save_failed`(사유) |
| 진행 중 캡처가 상한(4개)이라 시작 거절 | Warning | `toast.screenshot_busy` |
| 캡처 스레드를 만들지 못함 | Error | `toast.screenshot_start_failed`(사유) |
| 앱 종료 중이라 시작 거절 | 없음 | — |
| mirror: 단축키 처리 시점에 연결이 이미 없음(캡처하지 않음) | Warning | `attach.toast.mirror_capture_unavailable` |
| mirror: 캡처 뒤 연결이 바뀌었거나 전송 실패(로컬에만 저장) | Warning | `attach.toast.mirror_capture_not_sent`(로컬 저장 경로) |
| mirror: 원격 회신 | 위 `capture_result` 규칙 | `attach.toast.mirror_capture_*` |

mirror 캡처는 전송을 접수한 것만으로 성공을 알리지 않는다. 결과는 원격 회신이나 위 연결 종료 안내가 알린다. 에이전트가 쓰는 CLI `tasty screenshot`·IPC `ui.screenshot`·`clipboard.set_text` 는 이 경로를 지나지 않으므로 사용자 토스트를 만들지 않는다.

`NoOutput` 을 Info 로 알리는 이유: gnome-screenshot 처럼 Esc 취소도 정상 종료·파일 없음으로 끝나는 도구가 있어 실패로 단정할 수 없지만, 아무 안내가 없으면 도구가 조용히 실패한 경우를 사용자가 알 수 없다.

## 인터페이스

- **사용자 트리거**: `screenshot_to_clipboard` 키바인딩(`KeybindingSettings`, 기본 `ctrl+alt+s`, 전 프리셋 동일값). Settings → Keybindings → Clipboard 탭에서 편집.
- **AI Agent (IPC/CLI)**: `clipboard.set_text` — 로컬 클립보드에 텍스트를 쓴다. IPC `{"method":"clipboard.set_text","params":{"text":"..."}}`(plugin 은 `Permission::ClipboardWrite` 필요 — plugin 호출은 App 층 IPC 분기를 거치지 않으므로 GUI · 헤드리스가 함께 쓰는 engine 라우터가 같은 처리로 답한다) / CLI `tasty clipboard set-text <text>`. 원격 mirror 캡처 전송 경로가 원격 인스턴스에서 최종적으로 클립보드에 반영하는 것과 동일한 하부 동작(`ClipboardSystem::write_text`)을 노출한 것 — attach 미니 프로토콜 자체는 이 IPC 를 경유하지 않는다(위 참고).
- **원격 / 점유**: mirror 워크스페이스에 대해서만 원격 전송이 발생한다. 대상 attach 세션이 이미 그 워크스페이스를 점유하고 있어야 하며(mirror 존재의 전제조건), 점유가 아니면(이론상 불가능한 상태) 서버가 거부한다.

## 비-목표 (Out of scope)

- **원격 스크린샷 파일 자체를 로컬로 가져오는 것** — 반대 방향(원격→로컬)이며 이 기능의 범위가 아니다.
- **scp/ssh-exec 등 attach 와 무관한 별도 전송 채널** — 명시적으로 채택하지 않음(기존 attach 채널 재사용으로 확정).
- **`StreamControl` enum 확장** — 별도의 이벤트 태그를 같은 `StreamTag::Control` 채널에서 처리한다.
- **비-mirror(일반 attach 없는) 원격 클립보드 반영** — mirror 판별은 오직 `Workspace.mirror` 로만 하며, surface 단위 attach(워크스페이스 아님)는 이 판별 대상이 아니다.

## Acceptance Criteria

- Given 로컬(비-mirror) surface 에 포커스 When `screenshot_to_clipboard` 트리거 Then OS 인터랙티브 캡처 후 캡처 파일 경로 텍스트와 같은 그림의 이미지가 로컬 클립보드에 함께 올라간다.
- Given 저장 PNG 를 디코딩하지 못하거나 이미지 게시가 실패 When 캡처를 게시 Then 경로 텍스트만 올라가고 결과는 경로만 복사한 부분 성공이다.
- Given mirror 워크스페이스의 surface 에 포커스 When `screenshot_to_clipboard` 트리거 Then 캡처 파일이 attach 채널로 원격에 전송되고, 원격 인스턴스의 클립보드에 그 원격 경로 텍스트와 이미지가 올라간다(로컬 클립보드는 바뀌지 않는다).
- Given mirror 에서 캡처했고 그 뒤 로컬 클립보드가 그대로임 When 그 mirror 의 터미널에서 붙여넣기 Then 로컬 내용이 아니라 원격 저장 경로가 입력된다.
- Given mirror 에서 캡처한 뒤 로컬에서 다른 내용을 복사함 When 그 mirror 의 터미널에서 붙여넣기 Then 새로 복사한 로컬 내용이 입력된다.
- Given mirror 에서 캡처했고 원격 클립보드 쓰기만 실패함 When 로컬 클립보드가 그대로인 채 붙여넣기 Then 원격 저장 경로가 입력된다.
- Given 로컬 surface 에서 캡처해 로컬 클립보드에 이 머신의 경로와 이미지가 있음 When mirror 터미널에서 붙여넣기 Then 이미지를 원격에 올리고 원격 경로가 입력된다.
- Given 원격 서버가 `capture_result` 에 `image` 를 싣지 않는 옛 버전 When mirror 캡처가 성공 Then 클라이언트는 이미지까지 복사한 성공이 아니라 경로만 복사한 성공으로 안내한다.
- Given 캡처가 사용자에 의해 취소됨(Esc 등) Then 로컬/원격 어느 클립보드도 바뀌지 않는다.
- Given Linux 에서 첫 캡처 도구가 비정상 종료하고 다음 후보가 설치됨 When 캡처 트리거 Then 다음 후보를 실행하고 그 결과를 사용한다. 모든 후보가 실패하면 결과는 취소가 아니라 도구 실패다.
- Given 캡처 도구가 정상 종료했지만 파일을 남기지 않음 When 캡처 트리거 Then 다음 후보를 실행하지 않고 어느 클립보드도 바꾸지 않으며, 결과는 사용자 취소가 아니라 출력 없음으로 분류된다.
- Given 캡처 도구가 잘린 PNG 를 남김 When 캡처 트리거 Then 그 파일을 지우고 다음 후보를 시도한다.
- Given macOS 에서 화면 기록 권한이 미승인 When 캡처 트리거 Then 취소와 구분되는 권한 안내 토스트가 뜬다(클립보드는 바뀌지 않는다).
- Given 로컬 캡처가 경로와 이미지를 함께 게시 When 결과를 처리 Then 요청한 윈도우에 성공 토스트가 한 번 뜬다. 경로만 게시했으면 성공이 아니라 경고 토스트다.
- Given 캡처 도구가 없거나 실패하거나 저장에 실패 When 캡처 트리거 Then 요청한 윈도우에 사유가 담긴 오류 토스트가 뜨고, 사용자 취소에는 토스트가 없다.
- Given 캡처를 기다리는 동안 다른 윈도우로 포커스를 옮김 When 결과가 도착 Then 토스트는 요청한 윈도우에 뜬다. 요청한 윈도우가 닫혔으면 어느 윈도우에도 뜨지 않는다.
- Given 동시에 올린 두 mirror 캡처 When 회신이 역순으로 오거나 같은 upload_id 의 회신이 두 번 옴 Then 각 요청은 자기 회신으로 한 번만 안내되고 기다리지 않는 회신은 안내되지 않는다.
- Given mirror 업로드를 보낸 뒤 회신 전에 연결이 끊김 Then 요청한 윈도우에 복사가 확인되지 않았다는 안내가 뜬다.
- Given `clipboard.set_text` IPC 호출(text 파라미터 포함) Then 로컬 클립보드가 그 텍스트로 바뀐다.
- Given plugin 이 `ClipboardWrite` 권한 없이 `clipboard.set_text` 호출 Then permission_denied.
- Given `ClipboardWrite` 권한을 가진 plugin 의 `clipboard.set_text` 호출 Then 호스트 클립보드가 그 텍스트로 바뀌고 plugin 은 `{"ok": true}` 를 받는다.

### 검증 범위

같은 머신의 독립 인스턴스 두 개로 attach 전송·메시지 분류·서버 저장·클립보드 반영을
확인한 이력이 있다. 물리적으로 다른 원격 머신에서 사용자가 붙여넣는 최종 동작은
그 검증에 포함되지 않았다.

로컬 캡처와 취소는 Xvfb 환경에서 입력을 주입하고 X11 클립보드를 읽어 확인했다.
당시 GNOME 캡처 포털이 없어 테스트용 shim과 `scrot`으로 성공·파일 미생성을 재현했으므로,
이 결과가 네이티브 영역 선택 UI의 검증을 뜻하지는 않는다. `clipboard.set_text`는 실제 CLI로,
권한 거절은 `plugin_missing_clipboard_write_denied_for_clipboard_set_text` 단위 시험으로 확인했다.
plugin 호출 경로는 헤드리스 dispatch 시험(`a_plugin_clipboard_write_reaches_the_host_clipboard`, 가짜 클립보드)과
macOS 가상머신의 GUI 인스턴스에서 에이전트 세션 상태줄 복사 뒤 `pbpaste` 로 확인했다.

결과 분류의 후보 순서·폴백·멈춤은 임시 `PATH` 의 도구 shim 으로 단위 시험한다
(`crates/tasty-platform/src/screen_capture/linux_tests.rs`). shim 결과는 OS 네이티브 취소의 검증이 아니다.
실제 도구로는 Linux X11(Xvfb)에서 scrot 1.10 의 Esc 취소·영역 선택 성공, 그리고 실행 중인
gnome-screenshot 41.0 이 비정상 종료한 뒤 scrot 으로 넘어가 Esc 로 취소되는 흐름을 확인했다.
Wayland 도구(grim·slurp)와 macOS 는 실제 실행하지 않았고 소스 기준이다.

Windows client 에서 Xvfb 디스플레이의 Linux GUI 서버로 SSH 터널을 거쳐 mirror 캡처와 붙여넣기를 실행해, 원격 경로가 bracketed paste 로 입력되고 원격 X11 클립보드에 `image/png` 와 텍스트가 올라가는 것을 확인했다. 캡처 뒤 로컬 복사가 우선하는 판정과 기록 갱신은 단위 시험으로 확인한다. 로컬 캡처를 mirror 에 붙여넣는 경로와 macOS client 는 실제 실행하지 않았다.

## 구현

- 캡처: `crates/tasty-platform/src/screen_capture.rs`(`capture_interactive`, 플랫폼별 `capture_to_path`, `validate_png`, Linux 후보 실행 `linux::run_candidates`).
- App 폴링/스레딩: `src/app/screenshot_capture.rs`(`poll_screenshot_captures`/`trigger_pending_screenshot_captures`/`drain_screenshot_capture_results`), `src/app.rs`(`screenshot_capture_tx/rx`), `src/app/event.rs`(`AppEvent::ScreenshotCaptureReady`).
- 큐: `src/remote/state.rs`의 `RemoteState.pending_screenshot_captures` — 원 mirror workspace와 View를 보관하며 `src/app/engine_action.rs`에서 채운다.
- mirror 붙여넣기: `src/view/main/clipboard.rs`(`paste_to_terminal`, `replace_local_capture_path`), `src/clipboard/context.rs`(`paste_fingerprint`), `src/remote/state.rs`(`MirrorCapturePaste`), `src/app/screenshot_capture.rs`(`remember_mirror_capture`), `src/app/attach_client/output.rs`(`apply_capture_result`).
- 키바인딩: `crates/tasty-settings/src/keybindings.rs`(`screenshot_to_clipboard` 필드 + `default_screenshot_to_clipboard`), `presets.rs`(4 프리셋 공통값), `crud.rs`(`GENERAL_BINDING_FIELDS`/`get_bindings(_mut)`), 매치는 `src/adapters/ui/input/shortcuts/keybinding.rs`(`match_capture_bindings`). Settings UI: `src/view/settings/ui/keybindings_tab.rs`(Clipboard 서브탭).
- 클립보드 게시: `src/ports/clipboard.rs`(`ClipboardImage`, `CaptureCopy`, `publish_text_with_image`), `src/clipboard/capture_image.rs`(`decode_png`), `src/clipboard/files.rs`(`write_text_and_image`)와 `files/x11.rs`·`files/windows.rs`·`files/format.rs`(`dib`, `unicode_text`), `crates/tasty-platform/src/pasteboard.rs`(macOS), `src/adapters/production/arboard_clip.rs`.
- 원격 전송(client): `src/app/attach_client/bulk.rs`(`forward_capture_to_remote_clipboard`), `src/app/attach_client/wire.rs`(`send_control_frame`, `parse_capture_result`), `src/app/attach_client/output/apply_event.rs`(`MirrorEvent::CaptureResult`).
- 원격 수신(server): `crates/tasty-ipc/src/stream_hub.rs`(`CaptureUploadMsg`, `pump_inbound` 분류), `src/remote/capture_upload.rs`(`CaptureUploadRegistry`), `src/remote/server.rs`(`admit_capture_upload`, `prepare_capture`, `publish_capture`, `push_capture_result`, headless 전용 `finalize_capture_upload`), `src/app/image_jobs/capture.rs`(GUI worker 제출, Windows 게시, 회신 대조). GUI(`src/app/event_handler.rs::apply_capture_upload_msg`)와 headless(`src/boot/headless_stream.rs::apply_capture_uploads`) 양쪽 진입점이 같은 접수·저장·회신 함수를 쓴다.
- IPC/CLI: `crates/tasty-ipc/src/method_meta.rs`(`clipboard.set_text` → `Permission::ClipboardWrite`), `src/app/ipc/app_methods.rs`(`ipc_handle_clipboard_set_text`), plugin 호출 `src/adapters/ipc/handler.rs`(`route_engine_handler`), `crates/tasty-cli/src/commands/clipboard.rs`(`ClipboardCommands::SetText`), `crates/tasty-cli/src/request/clipboard.rs`.
- 테스트: `crates/tasty-ipc/src/method_meta_tests.rs`(`clipboard_set_text_is_release`), `crates/tasty-ipc/src/stream_hub.rs`(`pump_inbound_classifies_capture_chunk_and_commit`), `src/remote/capture_upload.rs`(누적/격리 단위 테스트), `crates/tasty-platform/src/screen_capture.rs`(경로·PNG 검사 단위 테스트), `crates/tasty-platform/src/screen_capture/linux_tests.rs`(후보 폴백·취소·출력 없음 분류 shim 시험).
