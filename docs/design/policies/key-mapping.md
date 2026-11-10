# 키 매핑 설계 (운영 상세)

ESC·Enter처럼 당연하게 쓰이는 기본 조작을 제외한 Tasty 자체 단축키는 [`KeybindingSettings`](../../features/settings/index.md) 로 노출되며 코드에 하드코딩되지 않는다. 본 문서는 바인딩 문자열의 OS별 키 매핑과 위치 기반 추상화를 기술한다.

## 핵심 원칙: 물리적 키 위치 일관성

Windows/macOS/Linux 에서 **같은 물리적 키 조합 → 같은 기능**. 표준 키보드 하단 수정자 배치:

```
Windows/Linux:  [Ctrl] [Win/Super] [Alt]  ──  [Alt] [Win/Super] [Ctrl]
macOS:          [Ctrl] [Option]    [Cmd]  ──  [Cmd] [Option]    [Ctrl]
```

macOS 의 **Cmd** 는 Windows/Linux 의 **Alt** 와 같은 물리적 위치다. 사용자가 키보드를 바꿔 써도 같은 손가락 위치에서 같은 동작을 기대하므로, tasty 는 이 물리적 위치를 기준으로 매핑한다.

## 바인딩 토큰 ↔ 실제 키

| 토큰 | Windows | Linux | macOS |
|------|---------|-------|-------|
| `ctrl` | Ctrl | Ctrl | Control (⌃) |
| `alt` | Alt | Alt | **Command (⌘)** |
| `shift` | Shift | Shift | Shift |
| `option` | Win | Super | Option (⌥) |

macOS 에서만 `alt` 토큰이 Cmd(⌘)에 매핑된다(물리 위치가 Win/Linux 의 Alt 와 동일하므로). 같은 이유로 `option` 은 macOS 의 Option 과 Windows·Linux 의 Win·Super 를 가리킨다. 둘 다 Ctrl 과 Alt·Cmd 사이에 있다. 예: 프리셋 `new_tab = "alt+t"` 는 Win/Linux 에서 Alt+T, macOS 에서 ⌘+T 로 눌린다. 프리셋은 **하나의 바인딩 문자열 집합**을 쓰지만 OS별 매핑으로 각 OS 에서 자연스러운 조합으로 느껴진다.

### 캡처(설정 UI) / 매칭(런타임)

- **캡처**: winit `ModifiersState` → 토큰. macOS `super_key() → "alt"` · `alt_key() → "option"`, 기타 `alt_key() → "alt"` · `super_key() → "option"`; `control_key() → "ctrl"`, `shift_key() → "shift"`. macOS 에서 ⌘+N 도, Windows 에서 Alt+N 도 동일하게 `"alt+n"` 저장. Windows 의 Win+K·Linux 의 Super+K·macOS 의 ⌥+K 는 모두 `"option+k"` 다.
- **매칭**: winit `ModifiersState` 와 비교. `"ctrl" → control_key()`, `"alt" → macOS super_key() / 기타 alt_key()`, `"option" → macOS alt_key() / 기타 super_key()`, `"shift" → shift_key()`. 두 축의 대응은 `tasty_key_match::token_axes` 한 곳에서 정하고 녹화·단축키·전환 표시·보조키 도움말이 같이 쓴다.
- **egui 경로**: egui-winit 은 Win·Super 를 macOS 에서만(`mac_cmd`) 담는다. 그래서 `GpuState::handle_egui_event` 가 `ModifiersChanged` 마다 `tasty_key_match::note_modifiers` 로 그 상태를 egui 문맥에 남기고, 창이 포커스를 잃으면 비운다. egui 입력으로 판정하는 단축키(`any_binding_pressed_egui`·`consume_binding_egui`)·워크스페이스 전환 키캡·explorer 드래그 반전은 `super_held` 로 그 값을 읽어 `token_axes_egui` 에 넘긴다.

### OS 키 이름 토큰

저장 토큰은 `ctrl`·`alt`·`option`·`shift` 넷뿐이다. `cmd`·`command`·`super`·`win`·`meta` 같은 OS 키 이름은 받지 않고 `option`·`alt` 로 바꾸지도 않는다. winit 의 `super` 는 macOS 에서 Command(`alt` 위치), 다른 OS 에서 Win·Super(`option` 위치)라 같은 낱말이 OS 마다 다른 위치를 가리키고, `meta` 는 환경마다 Alt 나 Super 를 뜻한다. 이 낱말을 정규화하면 저장 문자열의 뜻이 저장한 OS 에 따라 달라져 설정 파일이 OS 독립이 아니게 된다. 이런 바인딩은 인식되지 않는 키로 남아 어떤 입력과도 맞지 않는다. plugin 단축키처럼 텍스트 값을 보이는 곳은 일반 해석 실패 문구 대신 "Win·Super·Option 은 `option`, Cmd 는 `alt` 로 적는다" 는 안내(`keys.os_key_name`)를 보인다. 판정은 `tasty_settings::keybindings::os_keys::uses_os_key_name` 이 `+` 로 나눈 조각에서 `cmd`·`super`·`win`·`meta` 를 찾는 것이다.

### OS 가 가로채는 조합

Windows 의 Win+L·Win+D·Win+E 같은 셸 예약 조합과 GNOME·KDE 의 Super 단독·Super+문자 다수는 OS 가 먼저 처리해 Tasty 에 오지 않는다. Tasty 가 바꿀 수 없으므로 설정에 노출하는 대상이 아니다. 녹화에는 잡히지 않고, 설정 파일에 적은 경우에는 저장되지만 실행되지 않는다.

예약 조합은 OS·데스크톱마다 달라 완전한 목록을 가질 수 없다. Tasty 는 짧은 힌트 목록만 두고(`tasty_settings::keybindings::os_keys::is_os_reserved`), 텍스트로 들어온 바인딩이 목록에 맞으면 "그럴 수 있다" 는 경고만 보인다. 저장·적용은 막지 않는다. 녹화한 바인딩은 OS 가 가로챈 키가 녹화기에 오지 않으므로 대상이 아니다.

| OS | 힌트 목록 |
|---|---|
| Windows | Win 단독, Win+L · Win+D · Win+E · Win+R · Win+I · Win+Tab (다른 수정자 없이) |
| Linux(모든 데스크톱) | Super 단독 |
| macOS | 없음 |

경고(`keys.os_reserved.windows` / `keys.os_reserved.linux`)가 보이는 곳은 두 군데다.

- 단축키 가져오기의 경고 블록 한 줄.
- 설정 › 단축키 › 플러그인의 Custom 줄 아래 caption(설정 파일에서 들어온 키). 해석 실패나 OS 키 이름 안내가 있으면 그쪽이 먼저다.

## 바인딩 문자열 문법

`+` 는 구분자이자 키 이름이다. 파서는 왼쪽부터 `ctrl+`·`shift+`·`alt+` 프리픽스를 벗기고 남은 전체를 키 토큰으로 본다 — `"ctrl++"` = "Ctrl + `+` 키".

| 바인딩 | 해석 |
|--------|------|
| `ctrl++` / `ctrl+plus` | Ctrl + `+` (문자/이름 별칭 양방향 매칭) |
| `ctrl+-` / `ctrl+minus` | Ctrl + `-` |
| `ctrl+=` / `ctrl+equals` | Ctrl + `=` |
| `ctrl+shift+=` | Shift 까지 함께 요구 |
| `ctrl+` / `ctrl` | 무효 (키 부분 없음 / 모디파이어 단독) |

키 토큰은 문자 하나, 기호 별칭 `plus`·`minus`·`equals`, 이름 키 `tab`·`space`·`enter`·`backspace`·`delete`·`insert`·`home`·`end`·`pageup`·`pagedown`·`up`·`down`·`left`·`right`·`escape`·`f1`~`f24` 다. 매칭(`tasty-key-match` 의 `NAMED_KEY_TOKENS`)·설정 녹화·webview 포커스 중 호스트로 넘기는 네이티브 키 변환·macOS 메뉴 key equivalent 가 같은 이름 집합을 쓴다. 그 밖의 토큰(`f25`, 오타 `shft+h` 등)은 어떤 입력과도 맞지 않는다.

단축키를 찾을 키는 `tasty_key_match::shortcut_lookup_key` 가 정하고 본 창·plugin·webview 경로가 같이 쓴다. 물리 키가 F13~F24 인데 논리 키가 이름 키가 아니면 그 F 키로 본다. macOS winit 은 F21~F24 의 논리 키를 AppKit 사설 영역 문자(`U+F718` 등)로 올리고 물리 키만 F21~F24 로 주기 때문이다. ctrl·alt·super 가 눌렸으면 IME 가 바꾼 문자 대신 물리 키의 US 배열 문자를 쓴다.

- **modifier 없는 일반 키 등록 방지**: 설정 캡처 시 알파벳/숫자/스페이스 등 타이핑 키는 수정자 1개 이상과 함께여야 등록된다(`w` 단독 무시). F1~F24·Tab·Enter 등 비타이핑 키는 수정자 없이 가능.
- **모디파이어 단독 입력(Ctrl/Shift/Alt/Super/Meta/Fn)은 어떤 바인딩과도 매칭 안 됨** — 매처가 구조적으로 차단.
- **Escape 는 설정 UI 녹화에서 "슬롯 비우기"로 예약** — 녹화 중 ESC 를 누르면 그 슬롯이
  지워지고 녹화가 끝난다. 따라서 `escape` 를 값으로 갖는 바인딩은 프리셋/설정 파일로만
  들어오고 녹화 버튼으로는 재지정할 수 없다(현재 해당: `fullscreen_stage_exit`, `code_area_cancel`, `explorer_clear_selection`). 기본값으로
  되돌리려면 프리셋을 재적용한다.
- **modifier 없는 바인딩은 modifier-hint 오버레이에 뜨지 않는다** — 오버레이는 홀드 중인
  modifier 조합에 속한 바인딩만 나열하므로, 조합이 없는 바인딩은 속할 섹션이 없다.
- **`option` 바인딩이 없으면 Win·Super 홀드는 modifier-hint 를 띄우지 않는다**(macOS 밖) — 타일링 WM·Super+드래그로
  이 키를 오래 누르는 사용이 흔하다. Alt·Ctrl·Shift 와 macOS Option 은 빈 조합도 보인다.

## 웹 화면의 키 처리 원칙

native WebView(html·markdown 본문과 그 입력란)가 키를 받으면 Tasty 단축키가 먼저 받는다. 수식키 없는 키와 Shift 전용
키는 공통 새로고침의 기능키를 빼고 페이지 입력에 남긴다.

- 웹 화면 포커스에서 할 일이 없는 동작(Explorer 전용 동작, 꺼진 카테고리 전환, 그 surface에서 실행 범위가 아닌
  plugin 명령 등)의 조합은 처음부터 가져가지 않고 WebView 엔진과 페이지로 넘긴다.
- 바인딩에 없다는 이유로 키를 막지 않는다. 바인딩에서 지운 키는 엔진의 원래 동작을 한다.

html 주소 입력란은 egui 입력란 규칙을 따르며 이 원칙의 대상이 아니다. host 모달·overlay·전체화면 무대가 열린 동안의
차단은 단축키 판정이 아니라 입력 게이트다. 플랫폼별 경로와 남은 한계는 [WebView 키 정책](../systems/webview.md)에 있다.

## 복사/붙여넣기 키 정책

세 방식을 독립적으로 on/off:

| 방식 | 복사 / 붙여넣기 | macOS 실제 키 |
|------|----------------|---------------|
| macOS | `alt+c` / `alt+v` | ⌘+C / ⌘+V |
| Linux | `ctrl+shift+c` / `ctrl+shift+v` | Ctrl+Shift+C/V |
| Windows | `ctrl+c` / `ctrl+v` | Ctrl+C/V |

`copy`·`cut`·`paste`·`select_all` 바인딩은 터미널·Explorer뿐 아니라 native WebView(html·markdown 본문과 그 입력란)와
HTML 주소 입력에도 적용한다. 각 편집 대상이 원래 처리하는 기본 편집 키(WebView 엔진·egui 입력의 Ctrl/⌘+C 등)는
바인딩 해제와 관계없이 남고, Tasty 바인딩은 키를 더할 뿐 같은 입력을 두 번 실행하지 않는다. 대상별 계약은
[WebView 편집 액션](../systems/webview.md#편집-액션)과 [HTML 주소 표시줄](../../surfaces/html/index.md#주소-표시줄)에 있다.
페이지 텍스트의 실행 취소·다시 실행은 같은 대상에 쓰는 별도 바인딩 `page_undo`·`page_redo`다. macOS WKWebView는
Edit 메뉴 없이 ⌘+Z를 처리하지 않으므로 이 바인딩이 그 키를 정한다.

## OS 고유 키 이름 혼용 금지

바인딩 토큰 `ctrl`/`alt`/`shift` 는 **물리적 위치를 추상화한 이름**이며 OS 가 인식하는 키 이름과 다를 수 있다. tasty 는 각 키를 OS 고유 이름으로 인식하고 바인딩 문자열과의 변환만 OS별로 다르게 한다:

- macOS **Command(⌘)** 은 Command 다. Alt 가 아니다. **Option(⌥)** 은 Option 이다. Alt 가 아니다.
- Windows **Alt** 는 Alt, **Win** 은 Win 이다. Linux 의 **Super** 는 Super 다.

토큰 `"alt"` 가 macOS 에서 Command 에 매핑되는 것은 OS 고유 키를 다른 이름으로 부르는 게 아니라 **물리적 위치 기반 추상화**다.

## 설정 파일 이식성

`~/.tasty/config.toml` 의 바인딩 문자열은 **OS 독립**이다 — `"alt+n"` 은 어디서든 `"alt+n"`. 변환은 런타임 OS별 매핑 레이어가 한다. **저장(항상 추상 토큰) ↔ 표시(OS별/사용자 선택 표기)** 를 분리해 이식성을 유지한다.

macOS 사용자를 위한 표시 커스터마이징: `GeneralSettings::{alt,option,shift}_display_style` 3 개 필드(설정 > 일반 > 표시, mac 전용 UI)로 Alt/Option/Shift 토큰의 화면 표기를 텍스트("Alt"/"Option"/"Shift", 기본값)와 macOS 심볼("⌘"/"⌥"/"⇧") 사이에서 독립적으로 고를 수 있다(`alt` 는 추가로 "Cmd" 텍스트도 선택 가능). `KeybindingSettings::format_display`/`format_display_parts` 가 이 설정을 받아 표시 문자열을 만든다 — 필드는 크로스플랫폼으로 존재하지만(직렬화 단순성), 값을 바꿀 수 있는 UI 는 macOS 에서만 노출된다. 저장 포맷(바인딩 문자열)에는 전혀 영향을 주지 않는다.

"symbol" 표시에는 두 경로가 있다. `format_display`·`format_display_parts`는 `⌘`·`⌥`·`⇧` 문자를 반환한다. 명령 팔레트·상태바·단축키 탭 등에서 이 문자열을 쓰며, egui 폰트에 U+2325(⌥) 글리프가 없으면 빈 사각형으로 보일 수 있다. 아직 해결되지 않은 문제다.

설정의 3개 표시 방식 드롭다운과 modifier-hint의 키캡(`combo_keycap_parts`, `src/adapters/ui/modifier_hint_overlay.rs`)은 `tasty_icons::{CMD_KEY,OPTION_KEY,SHIFT_KEY}`와 `tasty_ui_widgets::{KbdKey,kbd_parts}`로 벡터 아이콘을 그린다. 이 경로는 글꼴의 해당 글리프를 필요로 하지 않는다.

### 이식 시 `option` 처리

`option` 은 모든 OS 에서 위 표의 키로 매칭되므로 그대로 옮겨 쓸 수 있다. macOS 에서 ⌥+K 로 녹화한 `option+k` 는 Windows 에서 Win+K, Linux 에서 Super+K 로 실행되고, 반대 방향도 같다. 수정자 조합 목록(`all_modifier_combos`)은 어느 OS 에서나 네 축의 15개다. 설정 가져오기는 `option` 바인딩을 대체 대상으로 보지 않는다.

네 기본 프리셋에는 `option` 바인딩이 없다. explorer 드래그 반전 수정자(`explorer_drag_flip_modifier`)의 기본값은 키 위치가 아니라 각 OS 파일 관리자의 복사 키를 따라 macOS `option`, 다른 OS `ctrl` 이다. macOS 에서 저장한 `option` 을 다른 OS 로 가져오면 Win·Super 를 누른 채 끄는 동작이 된다.

표시는 macOS 에서 `option_display_style`(Option·⌥)을 따르고, 다른 OS 에서는 같은 위치의 키 이름 `Win`(Windows)·`Super`(그 밖)를 쓴다(`KeybindingSettings::option_display_text`). 표시 방식 설정은 macOS 에만 노출된다.

## OS 메뉴 key equivalent

tasty 가 직접 소유하는 OS 메뉴의 key equivalent 도 **`KeybindingSettings` 의 대응 binding 에서 가져온다 — 가져올 수 없으면 비운다.** 지금 key equivalent 를 배선한 메뉴는 macOS NSMenu 하나이고, Windows AcceleratorTable · Linux Wayland 메뉴도 등록하게 되면 같은 규칙을 따른다. 탐색기 우클릭 메뉴(`tasty_platform::native_menu`)는 이름 변경·휴지통 옆에 `explorer_rename`·`explorer_trash` 의 첫 binding 을 표시만 한다(`MenuShortcut`). macOS 는 key equivalent 로, Windows 는 라벨 뒤 탭 열, Linux GTK 는 오른쪽 accelerator 라벨로 그린다. 실행은 단축키 경로가 맡는다([Explorer 파일 작업](../../surfaces/explorer/file-operations.md)). selector 가 OS 표준(`cut:` / `performClose:` 등)이라는 사실이 단축키 하드코딩을 정당화하지 않는다(selector 와 key equivalent 는 독립 결정). binding 이 빈 vec 이면 key equivalent 도 비워 단축키 없는 메뉴 항목으로 둔다.

macOS NSMenu 는 binding 의 첫 항목을 `tasty_platform::menu_key_equivalent` 로 바꾼다. 수식키는 위 표의 macOS 매핑(`alt` → Command)을 따른다. 문자 키는 소문자 그대로 두고 Shift 는 수식키로 나타낸다. 이름 키는 AppKit 문자로 바꾼다:

| 키 토큰 | key equivalent |
|---|---|
| `f1`~`f24` | `NSF1FunctionKey`(U+F704)~`NSF24FunctionKey`(U+F71B) |
| `up`·`down`·`left`·`right` | U+F700~U+F703 |
| `insert`·`home`·`end`·`pageup`·`pagedown` | U+F727·U+F729·U+F72B·U+F72C·U+F72D |
| `backspace`·`delete` | `NSBackspaceCharacter` U+0008(⌫)·`NSDeleteCharacter` U+007F(⌦). NSMenuItem 문서가 메뉴의 두 지우기 키에 이 문자를 쓰라고 정한다 |
| `tab`·`enter`·`escape`·`space` | U+0009·U+000D·U+001B·공백 |
| `plus`·`minus`·`equals` | `+`·`-`·`=` |

파싱할 수 없거나 위 이름 집합 밖인 키(`f25`, 오타 등)는 메뉴로 나타낼 수 없어 수식키까지 비운다. 변환표는 OS 호출이 없는 순수 함수라 모든 OS 에서 시험하고, NSMenu 호출부만 macOS 로 컴파일한다. macOS 시험은 표의 값을 `objc2_app_kit` 상수와 대조한다.

표준 selector 메뉴 항목(`make_keybound_std_item`)은 action 을 먼저 지정하고 key equivalent·수식키를 그 뒤에 넣는다. macOS 27.0.1 의 AppKit 은 `performMiniaturize:` 를 지정하는 순간 key 와 수식키를 ⌘M 으로 덮어쓴다. 순서가 반대면 `alt+shift+m` 이 ⌘M 이 된다. 그런데 `performMiniaturize:` 는 action 뒤에 빈 key·빈 수식키를 넣어도 ⌘M 이 남는다. 그래서 Window 메뉴의 최소화는 NSWindow 의 `miniaturize:` 를 target=nil 로 보낸다. `miniaturize:` 와 `performZoom:`·`performClose:` 는 빈 값과 설정값을 그대로 둔다. `miniaturize:` 는 winit 이 두 번째 이후 조합을 실행할 때 부르는 메서드와 같다. 이 값들은 메뉴 항목의 속성만 읽은 AppKit 프로브로 확인했다. 메뉴 추가와 `setMainMenu` 뒤에도 같았다. 실제 키 입력과 메뉴 실행으로는 확인하지 않았다.

NSMenuItem 은 key equivalent 를 하나만 가진다. 그래서 Window 메뉴의 최소화·확대·창 닫기(`minimize_window`·`maximize_window`·`close_window`)는 첫 조합만 메뉴가 실행하고, 두 번째 이후 조합은 단축키 경로(`shortcuts::window_control`)가 실행한다. 키 경로는 메뉴가 맡은 첫 조합을 건너뛰어 같은 키가 두 번 실행되지 않게 한다. 첫 조합을 메뉴 key equivalent 로 나타낼 수 없으면 메뉴 항목에 단축키가 없으므로 첫 조합도 키 경로가 실행한다. WebView 포커스 중 host 가 가져간 두 번째 이후 조합도 같은 단축키 경로로 실행된다. Windows·Linux 는 메뉴가 없어 모든 조합을 키 경로가 실행한다. `quit`·`new_window` 는 모든 조합을 키 경로에서도 검사한다. macOS 에서 메뉴가 처리한 키가 winit 에 다시 오지 않는다는 전제는 실 기기에서 측정하지 않았다.

메뉴에 묶은 이름 키는 NSMenu 가 먼저 받는다. 수식키 없는 `escape`·`tab`·`enter` 를 메뉴 항목에 묶으면 그 키는 터미널이나 입력칸에 가지 않고 메뉴 항목을 실행한다. 같은 키를 winit 단축키로 묶었을 때도 단축키가 먼저 가로채므로 결과는 같다.

**예외**: OS 자체가 박아 tasty 가 무력화/덮어쓰기/가로채기 모두 불가능한 단축키(macOS Spotlight `Cmd+Space`, OS 전역 윈도우 전환 등)는 정책 범위 밖 — tasty 가 등록할 수도 끌 수도 없다.

**라벨은 `t()` 다** — key equivalent 와 독립으로, OS 메뉴 항목의 **제목**은 `lang/*.toml` 번역 키에서 온다: macOS NSMenu `menu.macos.*`(앱 이름을 결합하는 About / Hide / Quit 은 `{}` placeholder + `t_fmt` 로 언어별 어순 대응), 시스템 트레이 메뉴·툴팁 `tray.*`, Windows Jump List `jump_list.*`. OS 네이티브 표면도 [i18n](../../dev-guide/i18n.md) 예외가 아니다. macOS 메뉴는 `KeybindingSettings` 변경 시 통째로 rebuild 되면서 라벨도 다시 조회하므로 키바인딩 변경 후에도 현재 언어가 유지된다.

## Plugin 커맨드 단축키 우선순위

plugin의 `[[contributes.commands]]`(`CommandDecl`)는 호스트 `KeybindingSettings`보다 먼저 매칭한다. `App::try_plugin_shortcut`이 `dispatch_window_event_to_view`보다 먼저 실행되며, plugin 명령을 찾으면 이벤트를 소비해 호스트로 넘기지 않는다. 사용자가 활성화하고 설정한 plugin의 단축키 선택을 우선하는 정책이다.

매칭 대상은 포커스에 따라 다르다.

| 포커스된 surface | 후보 명령 |
|---|---|
| plugin 소유 `RemoteSurface` | 해당 plugin의 `Global`·`Surface` 명령 |
| plugin surface가 아님 | 등록된 모든 plugin의 `CommandScope::Global` 명령 |

`Surface` 명령은 소유 plugin의 surface가 포커스됐을 때만 실행한다. 여러 plugin이 같은 Global 키를 등록하면 레지스트리 순회에서 처음 일치한 명령을 사용한다. 내부 저장소는 HashMap이므로 plugin 간 등록 순서나 실행 우선순위를 보장하지 않는다. 이 충돌을 해결하는 전용 UI는 아직 없다.

`CommandDecl.action`이 있으면 호스트가 `ToolAction::Event`·`OpenSurface`·`OpenPopup`을 도구 메뉴와 같은 방식으로 실행한다. 이때 `command.invoke`(`handle_command`)는 보내지 않아 중복 실행을 막는다. `action`이 없으면 기존 `handle_command` IPC를 사용한다.

Event Bus의 `command.invoked`는 소유 plugin에 보내는 알림이다. `action`과 대상 surface의 유무에 관계없이 명령이 매칭되면 보낸다.

## 텍스트 입력과 단축키의 우선순위

explorer 같은 egui surface에 포커스가 있으면 한 번의 키 입력이 두 곳에 도달한다.
`handle_event`가 텍스트 이벤트를 egui 큐에 먼저 넣고 그 다음에 단축키를 처리하는데,
단축키가 처리되더라도 이미 큐에 들어간 텍스트 이벤트를 되돌릴 방법이 없다. 그래서 수식
키 없이(또는 shift만 붙여) 영숫자에 등록된 단축키는 그 글자를 텍스트로도 흘려보낸다.

이 중복을 막는 곳은 텍스트를 소비하는 쪽이다. explorer 타입어헤드는
`unmodified_binding_chars`로 수식 키 없이 등록된 영숫자를 모아 두고, 그 글자는 소비하지
않고 단축키에 넘긴다([explorer](../../surfaces/explorer/index.md)의 "타입어헤드로 항목
선택"). Tasty 기본 프리셋에는 그런 단축키가 없으므로 기본 상태에서 넘기는 글자도 없다.

`shift`만 붙은 단축키도 같이 처리한다. shift 조합은 대문자 텍스트 이벤트를 만들어 수식
키가 없을 때와 똑같이 두 번 동작한다. `binding_has_modifier`가 shift를 수식 키로 세지
않는 것이 여기서는 오히려 맞는 판단이다. Ctrl과 Cmd 조합은 egui-winit이 텍스트 이벤트를
아예 만들지 않으므로 이 문제가 없다.

native WebView 위에서는 수식 키 없는 조합을 페이지에 남기는 것이 기본이다. 공통 새로고침(`refresh`)의 `f1`~`f24` 기능키만 예외로 host 가 가져간다. 기능키는 텍스트·IME 입력을 만들지 않아 페이지의 입력칸 문자·커서·조합을 건드리지 않는다. 이 예외를 다른 동작이나 문자 키로 넓히지 않는다([공통 새로고침](../../features/keybindings/index.md#공통-새로고침--refresh)).

## X11 탐색기 조합 종료 키

X11 XIM은 한 물리 키로 조합을 확정하고, 같은 키를 나중에 `KeyboardInput`으로 다시 보낼 수 있다. 빈 Preedit나 Commit 뒤에 도착했다는 사실만으로 새 누름이라고 판단하지 않는다. `DeviceEvent::Key`의 raw 누름이 XIM 필터보다 먼저 도착하는 경로에서 탐색기 목록의 조합 소유권을 물리 키별로 기록하고, 재전달된 목록 동작 키를 소비한다. 이동·열기·선택 해제·이름 변경·휴지통 바인딩을 설정에서 바꿔도 같은 매처를 사용한다.

기록은 새 물리 누름마다 교체하고 창 포커스 변경 때 비운다. XIM이 이전 키를 재전달하지 않았어도 다음 실제 누름을 막지 않는다. raw release가 먼저 도착할 수 있으므로 release만으로 기록을 버리지 않는다. 같은 surface의 조합에만 적용한다. Find 입력은 목록 바인딩과 분리해 조합 종료 Escape만 소비한다. 같은 누름이 검색어까지 지우지 않으며, 다음 실제 Escape는 기존 검색어 지우기·닫기 순서를 따른다. Find의 다른 키, 주소·이름 입력, popup, 터미널과 다른 OS의 키 경로는 이 처리로 바꾸지 않는다.

## 합성 키 이벤트 (winit)

winit은 X11·Windows에서 창이 포커스를 얻으면 이미 눌린 키의 `Pressed`, 잃으면 `Released`를 합성한다(`WindowEvent::KeyboardInput { is_synthetic: true, .. }`). macOS·Wayland에서는 합성하지 않는다.

Tasty는 합성 키를 사용자 입력으로 처리하지 않는다. 예를 들어 다른 앱을 `Alt+F4`로 닫은 뒤 Tasty에 합성 `F4`가 들어오면 사용자가 누르지 않은 `rename_tab`이 실행될 수 있다.

### 차단 지점 — 이벤트 진입부 단 한 곳

`App::window_event`의 진입부에서 `is_synthetic_key_event`로 차단한다. 셸 설정·종료·부팅·모달·plugin 단축키·View 위임보다 먼저 검사해야 한다. 어느 분기든 검사 전에 반환하면 그 경로에서 합성 키가 처리될 수 있다.

입력 처리에는 두 경로가 있다.

- **직접 해석**: `WindowEvent::KeyboardInput`을 읽어 단축키·PTY·녹화로 전달한다. X11에서는 차단 전 합성 `F4`로 탭 이름 변경이 열리고, `Escape`를 누른 채 종료 확인창으로 옮기면 창이 닫혔다. 차단 뒤에는 두 현상이 발생하지 않았고 직접 누른 키는 동작했다.
- **egui 전달**: `handle_egui_event`로 이벤트를 넘긴다. `MainView`·`SettingsView`·`PresetView`·`PluginsView`·`QuitView` 다섯 View가 사용하며 PresetView·PluginsView는 이 경로만 사용한다. `KeyboardInput` 패턴 검색만으로는 찾을 수 없다.

확인한 egui-winit 0.31.1은 합성 `Pressed`를 자체적으로 버리지만 `Released`는 버리지 않는다. 따라서 합성 `Enter`가 egui의 종료 확인 버튼을 누르는 현상은 재현되지 않았다. 다만 이는 의존성의 구현 세부이며 위젯 포커스가 안전을 보장하는 것은 아니다. Tasty는 두 이벤트 모두 자기 진입부에서 차단한다.

이 위치는 새 View에도 적용된다. `crates/tasty-doc-guards/tests/synthetic_key_event_guard.rs`가 검사 위치를 확인한다.

### 버려도 modifier 상태가 깨지지 않는 이유

양쪽 백엔드 모두 합성 키와 **별개로** `ModifiersChanged` 를 보낸다 — X11 은 포커스 획득
처리 말미의 `update_mods_from_query`, Windows 는 `gain_active_focus` 의 `update_modifiers`
가 담당한다. 따라서 합성 modifier Pressed 를 버려도 포커스 획득 직후의 modifier 조합
단축키는 정상 매칭된다.

### 더블탭 키 대응

더블탭 표기는 `shift+shift`·`ctrl+ctrl`·`alt+alt` 세 가지다. 토큰은 위 표와 같은 키를 가리킨다. macOS의 `alt+alt`는 Command(⌘) 두 번이며 Option(⌥) 두 번이 아니다. Windows·Linux의 `alt+alt`는 Alt 두 번이다. `option+option` 표기는 없으므로 macOS Option, Windows Win, Linux Super 두 번은 어떤 더블탭으로도 감지하지 않는다.

macOS에서 Option을 누르고 있는 동안의 Shift·Control·Command 탭은 더블탭으로 세지 않는다. 다른 더블탭 대상 수식키를 누른 채 탭할 때와 같이 조합 입력으로 본다. 두 탭 사이에 Option을 누르면 첫 탭을 버린다. Command 등을 누른 채 Option을 누르고 Option보다 먼저 떼면, 그 누름은 조합으로 보고 첫 탭으로 기록하지 않지만 누름 기록은 정리한다. 남기면 이후 다른 수식키 더블탭이 막힌다. 대응은 `DoubleTapDetector::classify`가 정하고 메인 창 실행과 설정 녹화가 같은 감지기를 쓴다.

더블탭을 실행하는 동작은 `KeybindingSettings::DOUBLE_TAP_FIELDS` 한 곳에서 정한다. 메인 창 실행 경로와 설정 녹화가 같은 목록을 쓴다. 설정 녹화는 이 목록의 일반 칸에서만 더블탭을 값으로 받는다. 목록 밖의 일반 칸·코드 입력칸·탐색기 목록 칸·스크립트·plugin 명령·번호 전환 슬롯에서 더블탭을 감지하면 값을 바꾸지 않고 녹화를 이어 가며 안내 문구를 표시한다. 가져오기나 설정 파일로 이미 저장된 실행 불가 더블탭은 지우지 않고 키 설정 화면 아래에 경고로 나열한다. 대상은 일반·범위 한정 칸과 스크립트, 탭·워크스페이스·카테고리 번호 전환의 슬롯·다음·이전 키다. 전환 키는 저장 값과 수식키를 합친 조합을 모두 본다. plugin 명령은 행이 읽을 수 없는 키로 따로 표시한다. 설정 창 안에서는 `toggle_settings`의 더블탭이 설정 창을 닫는다. 녹화 중에는 닫지 않고 값으로 받는다.

### double-tap detector 는 포커스 전환마다 초기화한다

합성 키를 버리면 포커스 전환 중 누름과 뗌이 짝을 이루지 않을 수 있다. `Alt+Tab`으로 나갈 때 실제 press만 들어오고 합성 release는 버려진다. 돌아온 뒤 Alt를 떼는 입력을 첫 탭으로 오인하면 다음 한 번의 입력이 더블탭으로 처리될 수 있다.

`WindowEvent::Focused`의 획득·상실 양쪽에서 `DoubleTapDetector::reset`을 호출한다. `MainView`와 `SettingsView`는 별도 인스턴스를 가지므로 두 곳 모두 초기화한다.

초기화는 첫 탭·오염 표지와 함께 macOS Option 누름 상태도 지운다. Option을 누른 채 창을 떠나 창 밖에서 떼면 release가 오지 않으므로, 이 상태를 남기면 돌아온 뒤 Command·Shift 더블탭이 계속 막힌다.

### `#[cfg]` 분기를 두지 않는 이유

`is_synthetic` 은 플랫폼 중립 필드고 발현 원인도 winit 의 공통 계약이다. 합성을 하지 않는
macOS·Wayland 에서는 항상 `false` 로 들어와 동작이 바뀌지 않으므로, OS 별 근본 원인이
다를 때 요구되는 분기 케이스가 아니다.

## 코드 위치

- `KeybindingSettings`(바인딩 필드·`format_display`), 캡처/매칭 레이어, 프리셋 기본 바인딩.
- OS 메뉴 배선: macOS NSMenu(`crates/tasty-platform/src/macos_delegate.rs`), key equivalent 변환표(`crates/tasty-platform/src/menu_key_equivalent.rs`).
- 합성 키 차단: `src/adapters/ui/input/synthetic.rs`(`is_synthetic_key_event`),
  `src/app/event_handler.rs`(`App::window_event` 진입부 게이트),
  `src/adapters/ui/input/double_tap.rs`(`DoubleTapDetector::reset`).
- 텍스트 입력과 겹치는 경우의 처리: `src/adapters/ui/surface/explorer/type_ahead.rs`
  (`unmodified_binding_chars`).
- Plugin 커맨드: `crates/tasty-host-plugin/src/command_registry.rs`(`PluginCommandRegistry`,
  `effective_binding`), `src/plugin_bridge/key_dispatch.rs`(`match_plugin_shortcut`/
  `match_global_shortcut`/`dispatch_plugin_command`), `src/app/plugin_glue/shortcut.rs`
  (`App::try_plugin_shortcut`).
