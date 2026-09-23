# 설정

셸, 글꼴, 화면 배치 등 자주 쓰는 환경을 내 작업 방식에 맞춰보세요. 설정 윈도우에서 항목을 바꾸거나 `~/.tasty/config.toml`을 직접 편집할 수 있습니다. 키 조합과 색을 바꾸는 방법은 [단축키](keybindings.md)와 [테마](themes.md)에서 자세히 안내합니다.

## 설정 윈도우 열기

사이드바 맨 아래 **설정** <!-- en: Settings --> 버튼을 누르거나 `Ctrl+,` 를 누릅니다.
같은 단축키를 한 번 더 누르면 설정 윈도우가 닫힙니다. <!-- en: Press the same shortcut again to close the settings window. -->

<!-- tasty-diagram: settings-window -->
```
┌─────────────────────────────────────────────────────────────┐
│ [일반] [터미널] [외관] [단축키] [핸들러] [기타] [플러그인]  │  상단 탭
├──────────────┬──────────────────────────────────────────────┤
│ 섹션 검색…   │                                              │
│ ▸ 일반       │   선택한 섹션의 설정 항목                    │
│   알림       │                                              │
│   접근성     │                                              │
├──────────────┴──────────────────────────────────────────────┤
│                                        [ 취소 ]  [ 저장 ]   │
└─────────────────────────────────────────────────────────────┘
```

- 상단 탭 7개: **일반** <!-- en: General --> · **터미널** <!-- en: Terminal --> · **외관** <!-- en: Appearance --> · **단축키** <!-- en: Keybindings --> · **핸들러** <!-- en: Handler --> · **기타** <!-- en: Misc --> · **플러그인** <!-- en: Plugins -->.
- 왼쪽 목록은 그 탭의 섹션입니다. 위의 **섹션 검색…** <!-- en: Filter sections… --> 에 글자를 넣으면 목록이 걸러집니다. 탭을 바꾸면 검색어는 지워집니다.
- 바꾼 내용은 **저장** <!-- en: Save --> 을 눌러야 파일에 쓰이고 화면에 반영됩니다. **취소** <!-- en: Cancel --> 하면 모두 버려집니다. 헤더에 닫기 버튼은 없습니다.
- **언어** <!-- en: Language --> 변경만은 저장 후 Tasty 를 다시 시작해야 적용됩니다.
- 숫자 칸은 **입력**해서 바꿉니다. 드래그해서 올리거나 내리는 칸은 없습니다. 치는 동안에는 값이 끌려가지 않고, 칸을 떠나거나 `Enter` 를 누를 때 허용 범위 안으로 맞춰집니다 — 범위를 벗어난 값을 쳐 두면 칸이 빨개지고 그 아래에 허용 범위와 실제로 저장될 값이 한 줄로 나옵니다.

## 탭별 항목

### 일반

| 섹션 | 항목 |
|------|------|
| **일반** | **시작 시 레이아웃 복원** <!-- en: Restore layout on startup --> · **재시작 시 Surface 내용 복원** <!-- en: Restore surface content on restart --> (터미널 스크롤백) · **워크스페이스 카테고리(폴더)** <!-- en: Workspace categories (folders) --> · **다음/이전 워크스페이스가 카테고리 경계를 넘음** <!-- en: Next/prev workspace crosses categories --> · **종료 동작** <!-- en: Close behavior --> (물어보기 / 백그라운드로 최소화 / 종료) · **휠 스크롤 거리** <!-- en: Wheel scroll distance --> (10~200pt, 기본 50) — 휠 한 칸이 스크롤하는 거리이며 윈도우 안 모든 곳에 같이 적용됩니다 · **언어** (English / 한국어 / 日本語 + 설치한 [언어팩](#언어-추가하기-언어팩)) |
| **알림** <!-- en: Notifications --> | **알림 활성화** <!-- en: Notifications enabled --> · **소리** <!-- en: Sound --> · **알림 병합 간격 (ms)** <!-- en: Coalesce interval (ms) --> |
| **접근성** <!-- en: Accessibility --> | **모션 줄이기** <!-- en: Reduced motion --> (토스트·오버레이 페이드, 로딩 스피너 회전, 모달 흔들기를 끕니다) · **수정자 키 힌트 표시** <!-- en: Show modifier key hints --> |
| **오버레이** <!-- en: Overlay --> | **토스트 표시 시간** <!-- en: Toast duration --> (1~10초) |
| **원격 전송** <!-- en: Remote transfer --> | **저장 폴더** <!-- en: Save folder --> (기본 `~/.tasty/transfers/`) · **최대 용량** <!-- en: Maximum size --> (MiB) — 원격 워크스페이스에서 받은 파일이 저장되는 곳 |
| **표시** <!-- en: Display --> (macOS 만) | **Alt 키 표시** · **Option 키 표시** · **Shift 키 표시** — 단축키 표기를 텍스트 / 심볼 중에서 고릅니다 |
| **권한** <!-- en: Permissions --> (macOS 만) | **전체 디스크 접근 권한** <!-- en: Full Disk Access --> · **화면 기록** <!-- en: Screen recording --> 상태와 시스템 설정 열기 |

### 터미널

| 섹션 | 항목 |
|------|------|
| **일반** | **셸** <!-- en: Shell --> · **시작 명령어** <!-- en: Startup command --> · **스크롤백 줄 수** <!-- en: Scrollback lines --> (기본 10000) · **실행 중 프로세스 닫기 확인** <!-- en: Confirm close running process --> · **작업 디렉토리 상속** <!-- en: Inherit working directory --> · **화면 반전 플래시 (DECSCNM)** <!-- en: Reverse-screen flash (DECSCNM) --> · **벨 알림 표시** <!-- en: Show bell notification --> · **링크 클릭 수식키** <!-- en: Link click modifier --> (Ctrl / Alt / 좌클릭 열기 끄기) · macOS: **Option 을 Meta 로 사용** <!-- en: Use Option as Meta --> · Windows: **셸 모드** <!-- en: Shell mode --> |
| **마우스 캡처** <!-- en: Mouse Capture --> | **마우스 캡처 안내 표시** <!-- en: Show mouse-capture hint --> · **다음 프로그램에서 마우스 캡처 비활성화** <!-- en: Disable mouse capture for these programs --> · **다음 프로그램에서 캡처 안내 배너만 억제** <!-- en: Suppress the capture hint banner for these programs --> — 프로세스 이름 또는 `ht*` 같은 패턴 |
| **TUI** | **클립보드 읽기 허용 (OSC 52)** <!-- en: Allow clipboard read (OSC 52) --> — 기본 꺼짐. 켜면 터미널 안 프로그램이 클립보드를 읽을 수 있습니다 |
| **성능** <!-- en: Performance --> | **선택적 PTY 폴링** <!-- en: Targeted PTY polling --> · **스크롤백 디스크 스왑** <!-- en: Scrollback disk swap --> — 둘 다 재시작 후 적용 |

**링크 클릭 수식키**를 **좌클릭 열기 끄기** <!-- en: Disable left-click opening --> 로 두면 좌클릭으로 링크를 열지 않습니다. 수식키 없이 링크에 마우스를 올려 표시할 수 있고 링크 위 우클릭 메뉴는 계속 사용할 수 있습니다. 팝업이나 배너 위에서는 링크 표시가 차단될 수 있습니다. 마우스 캡처 항목의 의미는 [터미널 다루기](../using/terminal.md).

### 외관

| 섹션 | 항목 |
|------|------|
| **테마** <!-- en: Theme --> | 테마 카드 목록 — [테마](themes.md) |
| **색상** <!-- en: Colors --> | 현재 테마의 색을 항목별로 덮어쓰기 — [테마](themes.md) |
| **일반** | **기본 폰트 설정** <!-- en: Default Font Settings -->: **폰트** <!-- en: Font family --> · **커스텀 폰트 파일** <!-- en: Custom font file --> · **글자 크기** <!-- en: Font size --> (기본 14) · **줄 높이** <!-- en: Line height --> · **폰트 DPI 스케일링** <!-- en: Font DPI scaling --> (자동 / 고정). 그리고 **합자(Ligatures)** <!-- en: Ligatures --> · **배경 투명도** <!-- en: Background opacity --> |
| **디스플레이** <!-- en: Display --> | **UI 배율** <!-- en: UI Scale --> — 작게 / 보통 / 크게 |
| **Tasty** | 앱 크롬 색 — **액센트** <!-- en: Accent --> · **사이드바 배경** <!-- en: Sidebar background --> · **활성 탭 인디케이터** <!-- en: Active tab indicator --> (밑줄 / 채움 / 점) |
| **터미널** | 터미널 서피스의 **포커스 배경** <!-- en: Focused background --> · **비포커스 배경** <!-- en: Unfocused background --> 과 폰트 override |
| **탐색기** <!-- en: Explorer --> | 탐색기 전용 폰트 override |
| **Markdown** · **HTML** | 플러그인이 추가한 페이지 — 마크다운 폰트 override 와 **원격 콘텐츠 허용** <!-- en: Allow remote content -->, HTML 뷰어의 **기본 확대** <!-- en: Default zoom --> · **색 구성표** <!-- en: Color scheme --> · **원격 콘텐츠 허용** · **스크립트 샌드박스** <!-- en: Sandbox scripts --> |

폰트 설정은 **일반** 의 기본값이 터미널 · 마크다운 · 탐색기에 일괄 적용되고, 각 섹션에서 **기본값 사용** <!-- en: Use default --> 을 끄면 그 종류만 다른 값을 씁니다. **폰트 DPI 스케일링** 이 **자동** 이면 모니터가 달라도 글자의 물리 크기가 같고, **고정** 이면 픽셀 크기가 같아 고해상도 모니터에서 글자가 작아집니다.

### 단축키

[단축키](keybindings.md) 에서 다룹니다.

### 핸들러

파일을 열 때 "어떤 파일인지 식별" 하는 규칙과 "무엇으로 열지" 를 정하는 표입니다. 기본값으로도 마크다운 · 이미지 · HTML 은 알아서 열리므로 보통 손댈 일이 없습니다.

- **파일 확장자 매핑** <!-- en: File Extension Mapping --> — 같은 확장자를 여러 디텍터가 가져갈 때 우선순위.
- **파일 디텍터** <!-- en: File Detectors --> — 확장자 · 경로 패턴으로 파일 종류를 식별하는 규칙. 사용자 규칙 추가 가능.
- **파일 핸들러** <!-- en: File Handlers --> — 식별된 종류를 어떤 서피스로 열지, 또는 OS 기본 앱으로 넘길지.
- **훅 핸들러** <!-- en: Hook Handlers --> — 훅 · 웹훅 이벤트가 왔을 때 실행할 셸 명령. [훅 · 알림 · 웹훅](../agents/hooks-notifications.md).

이 표는 `config.toml` 이 아니라 `~/.tasty/file-handlers.toml` 과 `~/.tasty/hook-handlers.toml` 에 저장됩니다.

### 기타

- **스크립트** <!-- en: Scripts --> — 단축키나 이벤트로 실행할 Lua 스크립트를 등록합니다 ([Lua 스크립트](scripts.md)). 파일 경로(예: `~/.tasty/scripts/my-script.lua`)와 표시 이름을 넣고, 단축키는 **단축키** > **스크립트 실행** 에서 붙입니다. 등록 뒤 파일이 바뀌면 **변경됨** <!-- en: changed --> 표시가 붙고 다음 실행 때 확인을 묻습니다.
- Windows 에서는 **Tastyrc** 섹션이 추가됩니다. 다른 OS 에서는 이 탭에 스크립트만 있습니다.

### 플러그인

플러그인이 추가한 설정 페이지가 모입니다. 기본 설치 상태에서는 **Claude Code** 와 **Codex** 페이지가 있습니다 — [플러그인](../plugins/index.md).

## 언어 추가하기 (언어팩)

Tasty 에 들어 있는 언어는 English · 한국어 · 日本語 셋입니다. 그 밖의 언어는 **언어팩**을
직접 두면 언어 목록에 함께 나타납니다.

### 언어팩 만들기

`~/.tasty/lang/` 아래에 **언어 코드 이름의 폴더**를 만들고 그 안에 `pack.toml` 을 둡니다.
예를 들어 프랑스어라면 `~/.tasty/lang/fr/pack.toml` 입니다.

```toml
[meta]
name = "Français"          # 언어 목록에 보일 이름 (생략하면 폴더 이름)

[font]                     # 필수 — 아래 넷 중 하나만
builtin = true             # Tasty 기본 글꼴로 충분한 문자만
# file = "fonts/x.ttf"     # 팩 폴더 안에 함께 둔 글꼴 파일
# family = "Noto Sans"     # 컴퓨터에 설치된 글꼴 이름
# candidates = ["fonts/x.ttf", "Noto Sans"]   # 위에서부터 차례로 시도

[button]                   # 여기서부터는 번역할 문구
ok = "OK"
cancel = "Annuler"
```

- **`[font]` 은 생략할 수 없습니다.** 문구만 있고 글꼴 약속이 없으면 화면에 글자가 □ 로
  깨질 수 있어서, 팩을 만든 사람이 어느 글꼴로 볼지 밝히도록 했습니다.
- **선언한 글꼴은 UI 전체에 적용됩니다.** `file` · `family` · `candidates` 로 밝힌
  글꼴이 기본 글꼴 뒤에 더해져, 설정·팝업은 물론 플러그인 화면(클립보드 뷰어 ·
  마크다운 등)까지 그 언어의 글자가 함께 나옵니다. `builtin = true` 는 "기본 글꼴로 충분하니
  더하지 않는다" 는 뜻입니다.
- **전부 번역하지 않아도 됩니다.** 적지 않은 문구는 영어로 나옵니다. 값을 빈 문자열
  (`""`)로 두는 것도 "번역하지 않음" 으로 보고 영어로 나옵니다 — 화면에 빈 칸이
  생기지 않습니다.
- 문구의 이름(`[button] ok` 같은 것)은 Tasty 에 들어 있는 언어 파일과 같은 짜임새입니다.
  기존 언어 파일을 복사해서 시작하는 편이 빠릅니다.
- `{}` 가 들어 있는 문구는 실행할 때 값이 채워지는 자리입니다. **개수를 그대로 두어야
  합니다** — 지우면 그 값이 사라지고, 더 넣으면 `{}` 가 화면에 그대로 보입니다.

### 터미널 명령의 도움말도 번역됩니다

`tasty --help` 처럼 터미널에서 보는 도움말도 고른 언어로 나옵니다. 언어팩에서는
`[cli.help]` 아래의 문구가 그 자리입니다.

```toml
[cli.help._root]
about = "Émulateur de terminal accéléré par GPU"

[cli.help._root.new]
about = "Créer une ressource"
```

- 여기서도 **전부 번역하지 않아도 됩니다.** 적지 않은 명령은 영어 설명이 그대로 나오고,
  번역한 것과 섞여 보입니다.
- 도움말의 사용법·명령·옵션 제목, 기본값 안내와 인자 오류도 고른 언어로 표시됩니다.
  `tasty help help`와 플러그인의 `help help`도 같은 언어를 사용합니다. 인자 설명이
  없는 플러그인에서도 기본값 안내는 표시됩니다.
  명령 이름·옵션·허용 값은 그대로 사용합니다. 하위 파서가 제공한 상세 오류와 서버의
  오류 원문은 영어로 남을 수 있습니다.
- 플러그인 명령의 설명과 인자 도움말도 아래의 플러그인 번역 파일에서 바꿀 수 있습니다.
  예를 들어 한국어 Codex 도움말 제목은 `~/.tasty/lang/plugins/com.tasty.codex/ko.toml`에
  `[codex.cli]`와 `desc = "나의 Codex 명령"`을 넣어 바꿉니다. 번역 키가 없는 이전
  플러그인은 기존 설명을 유지합니다.

### 고르기

설정 윈도우 › **일반** › **언어** 목록에 팩이 함께 나옵니다. 고르고 **저장** 한 다음 Tasty 를
다시 시작하면 적용됩니다.

### 플러그인 문구 바꾸기

플러그인 설치 폴더를 고치지 않고도 문구를 덮어쓸 수 있습니다. 한국어라면
`~/.tasty/lang/plugins/com.tasty.clipboard-viewer/ko.toml`에 다음처럼 적으세요.

```toml
[clipboard_viewer.command]
open_viewer = "내 클립보드 열기"

[clipboard_viewer.popup]
title = "내 클립보드"
```

플러그인의 도구 항목과 내부 화면이 같은 파일을 읽습니다. 폴더 이름은 플러그인 ID이며,
다른 플러그인을 바꾸려면 그 플러그인의 ID와 번역 항목 이름을 사용하세요. 새 언어팩
`fr`에서는 `~/.tasty/lang/fr/plugins/com.tasty.clipboard-viewer.toml`에 둡니다.
먼저 `fr/pack.toml`을 만들고 그 언어를 선택해야 합니다.

설치본 영어 → 설치본 선택 언어 → 사용자 파일 순서로 덮습니다. 빈 값이나 빠진 항목은
앞 단계의 문구를 유지합니다. 파일이 잘못된 TOML이거나 2 MiB를 넘으면 무시하고 경고를
남깁니다. 수정 후 Tasty를 다시 시작하세요. 이 파일은 플러그인을 업그레이드해도
보존됩니다. 같은 플러그인 항목을 본체의 `ko.toml`이나 `pack.toml`에도 중복해서 넣지 마세요.

`TASTY_HOME`을 상대경로로 지정해 실행해도 플러그인은 Tasty를 실행한 디렉터리 기준의 사용자 번역을 읽습니다.
기본 번들 플러그인도 이 홈에서 기동하며, 사용자 번역에 없는 문구는 설치본 번역으로 표시됩니다.

### 이미 들어 있는 언어의 문구만 바꾸고 싶다면

폴더가 아니라 **파일 하나**를 둡니다 — `~/.tasty/lang/ko.toml` 처럼. 적은 문구만 기본값을
덮어쓰며 `[font]` 도 필요 없습니다. 이 방식은 English · 한국어 · 日本語 세 언어에만 씁니다.
새 언어를 이 방식으로 두면 목록에 나타나지 않습니다.

값을 빈 문자열(`""`)로 두면 언어팩과 마찬가지로 "바꾸지 않음" 으로 봅니다 — 원래 들어
있던 그 언어의 문구가 그대로 나오고, 화면에 빈 칸이 생기지 않습니다.

### 잘 안 될 때

- **목록에 안 보입니다** — 폴더 이름이 언어 코드인지, 그 안의 파일 이름이 정확히
  `pack.toml` 인지, `[font]` 이 있는지 확인합니다. 설정 윈도우를 닫았다가 다시 엽니다
  (목록은 윈도우를 열 때 한 번 읽습니다). `pack.toml` 은 **2 MiB 를 넘을 수 없습니다** — 내장
  언어 파일이 400 KiB 도 안 되므로 정상적인 번역은 여기 걸리지 않습니다.
- **영어로 뜨고 경고가 나옵니다** — 고른 언어의 팩을 찾지 못했거나 파일이 잘못됐다는
  뜻입니다. 경고에 어느 경로를 찾았는지 나옵니다. **설정은 그대로 남으므로**, 팩을 고쳐
  두고 다시 시작하면 그 언어로 돌아옵니다.
- **글자가 □ 로 나오고 글꼴 경고가 뜹니다** — 팩과 문구는 실렸지만 `[font]` 이
  가리키는 글꼴을 찾지 못했거나 열 수 없다는 뜻입니다. 문구는 그대로 나오고 글꼴만
  안 붙습니다. `file` 경로(팩 폴더 기준)나 `family` 이름을 확인하고 다시 시작합니다.
- 자세한 사유는 `~/.tasty/debug.log` 에 한 줄로 남습니다.

## 설정 파일 `~/.tasty/config.toml`

설정 윈도우의 내용은 전부 이 파일 하나에 저장됩니다. 없는 키는 기본값으로 읽히므로 필요한 것만 적어도 됩니다.

```toml
[general]
language = "ko"                  # "en" | "ko" | "ja"
close_behavior = "ask"           # "ask" | "minimize" | "quit"
wheel_line_scroll = 50.0         # 휠 한 칸이 스크롤하는 거리(pt), 10~200
restore_layout = true
restore_surface_content = true
scrollback_lines = 10000
inherit_cwd = true
confirm_close_running = true
link_click_modifier = "ctrl"     # "ctrl" | "alt" | "none"
allow_clipboard_read = false
bell_notification = true
workspace_categories_enabled = false
mouse_capture_blacklist = ["htop"]
shell = ""                       # 비우면 자동 감지
startup_command = ""

[appearance]
theme = "mocha"                  # ~/.tasty/themes/<id>.toml
ui_scale = "medium"              # "small" | "medium" | "large"
ligatures = true
background_opacity = 1.0
active_tab_indicator = "underline"   # "underline" | "fill" | "dot"

[appearance.default_font]
font_family = ""
font_size = 14.0
line_height = 1.0
font_scale_mode = "auto"         # "auto" | "fixed"

[appearance.terminal_font]       # 비워 두면 default_font 를 따름
font_size = 15.0

[notification]
enabled = true
sound = false
coalesce_ms = 500

[accessibility]
reduced_motion = false

[modifier_hint]
enabled = true

[overlay]
toast_duration_ms = 2000

[performance]
targeted_pty_polling = true
scrollback_disk_swap = false

[remote_transfer]
dir = ""                         # 비우면 ~/.tasty/transfers/
max_mb = 500

[keybindings]
new_tab = ["alt+t"]              # 나머지는 keybindings.md
```

- `[appearance]` 아래에는 이 밖에도 테마 색 전체(`theme_base`)와 색상 탭에서 덮어쓴 값(`theme_overrides`)이 저장됩니다. 손으로 고치기보다 설정 윈도우를 쓰는 편이 안전합니다.
- `[plugin_settings."com.tasty.html"]` 처럼 플러그인 설정 페이지의 값은 플러그인 id 를 키로 한 절에 들어갑니다.
- Tasty 는 실행 중에 이 파일을 감시하지 않습니다. 손으로 고친 값은 다음 시작 때 읽히고, 그 전에 설정 윈도우에서 저장하면 파일 전체가 다시 쓰여 손편집이 사라집니다. 파일을 직접 편집할 때는 Tasty 를 종료한 뒤 합니다.

원격 전송 항목만은 CLI 로도 읽고 쓸 수 있습니다.

```sh
tasty settings get-remote-transfer
tasty settings set-remote-transfer --dir ~/incoming --max-mb 1000
```

## `~/.tasty` 폴더에 있는 것

| 경로 | 내용 |
|------|------|
| `config.toml` | 위 설정 |
| `themes/` | 테마 파일 — [테마](themes.md) |
| `lang/` | 언어팩과 언어 덮어쓰기 파일 — [언어 추가하기](#언어-추가하기-언어팩) |
| `plugins/` · `plugins-logs/` | 설치된 플러그인과 로그 — [플러그인](../plugins/index.md) |
| `file-handlers.toml` · `hook-handlers.toml` | 핸들러 탭의 사용자 항목 |
| `remote-profiles.toml` | 원격 연결 프로필 — [원격 작업 이어가기](../remote/attach.md) |
| `scripts/` | Lua 스크립트를 두는 관례 위치 — [Lua 스크립트](scripts.md) |
| `transfers/` | 원격에서 받은 파일 기본 저장 폴더 |
| `tasty.port` · `debug.log` | 실행 중 인스턴스의 포트, 경고 이상 로그 — [문제 해결](../help/troubleshooting.md) |

<a id="다음-읽을-것"></a>

## 함께 살펴보기

- [단축키](keybindings.md) — 기본 표 · 프리셋 · 녹화.
- [테마](themes.md) — 테마 전환 · 색 덮어쓰기 · 직접 만들기.
- [플러그인](../plugins/index.md) — 플러그인 설정 페이지와 관리 윈도우.
