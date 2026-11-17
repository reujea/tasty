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

native WebView(html·markdown 본문과 그 입력란)가 키를 받으면 수식키 유무와 관계없이 Tasty 단축키가 먼저 받는다.

- Tasty 단축키가 동작했으면 그 키를 페이지·엔진에 넘기지 않는다.
- 동작하지 않았으면 키를 WebView 엔진과 페이지로 넘긴다. 웹 화면 포커스에서 할 일이 없는 동작(Explorer 전용 동작,
  꺼진 카테고리 전환, 그 surface에서 실행 범위가 아닌 plugin 명령 등)의 조합은 처음부터 가져가지 않는다.
- 바인딩에 없다는 이유로 키를 막지 않는다. 바인딩에서 지운 키는 엔진의 원래 동작을 한다.
- 편집 바인딩 중 Tasty에 웹뷰용 동작이 있는 것(복사·잘라내기·붙여넣기·전체 선택)은 엔진이 같은 키를 처리하더라도
  Tasty가 실행한다. 실행 조건을 채우지 못하면(Linux에서 선택 없는 복사·잘라내기) 동작하지 않은 키로 넘긴다.

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

## 편집 키 판정

복사·잘라내기·붙여넣기(`copy`·`cut`·`paste`) 바인딩의 원칙은 이렇다. Tasty에 설정된 키는 동작할 수 있으면 동작하고
Tasty가 소비한다. 설정되지 않았거나 동작하지 못했으면 원래 키 그대로 다음 처리자에 넘기고 막지 않는다. 소비 조건은
"바인딩 일치"가 아니라 "편집이 실제로 일어났음"이다.

### 판정 함수

`src/adapters/ui/input/edit_key.rs`가 모든 입력 대상의 공통 판정을 둔다.

- 바인딩 고르기: 한 키가 여러 편집에 묶이면 복사 → 잘라내기 → 붙여넣기 순서로 하나만 고른다(`EditKey::ORDER`).
  HTML 주소 띠, host 창의 WebView 편집 키, native WebView 정책 목록이 같은 순서를 쓰며, 그 뒤에 전체 선택·실행
  취소·다시 실행이 온다(`webview_edit_order`).
- 판정 결과(`Verdict`)는 네 가지다.
  - `NoBinding`: 편집 바인딩이 아니다. 키는 원래 경로로 간다.
  - `Act`: 지금 동작할 수 있음을 안다. 실행하고 소비한다.
  - `Hold`: 결과를 봐야 안다. 원래 키를 붙잡고 편집을 시도한다.
  - `Pass`: 지금 동작할 수 없음을 안다. 원래 키를 다음 처리자에 넘긴다.
- 능력을 알 수 없으면 추정으로 소비하지 않는다. `Hold`로 결과를 기다린다.

### 한 번 판정

판정은 키 하나에 한 번만 한다.

- `Act`·`Hold`로 Tasty가 가져간 키는 뒤 단축키 디스패치를 거치지 않는다.
- `Pass`로 넘긴 키는 원래 경로로 가되, 뒤 단계가 같은 키를 클립보드 편집 바인딩으로 다시 고르지 않는다. 메인 창
  디스패치의 WebView 편집·복사·탐색기 파일 복사·잘라내기·붙여넣기·붙여넣기 매칭과 HTML 주소 띠의 클립보드 편집 선택이
  건너뛴다. 그 밖의 바인딩(창 제어, 주소 띠의 주소창 포커스·새로고침 등)은 그대로 판정한다.
- 같은 키를 `copy`와 `paste`에 묶고 선택이 없으면 복사만 판정해 `Pass`이고, 붙여넣기를 다시 시도하지 않는다.

### 보류와 재생

`Hold`인 키는 원래 창 이벤트를 붙잡는다(`EditKeyHold`).

- 결과가 `Acted`면 원래 키를 버린다(소비).
- `NotActed`면 원래 키를 다음 처리자에 한 번 재생한다. 재생한 키는 편집 판정을 다시 하지 않는다.
- 보류 중 같은 창에 온 키 누름·떼기·IME·modifier 변화는 보류가 풀릴 때까지 받은 순서대로 줄을 선다. 풀리면
  재생한 원래 키 뒤에 평소 경로(판정 포함)로 처리한다.
- 포인터 입력도 같은 줄에 선다. 포인터 위치에서 동작하는 winit 창 이벤트 전부다: 커서 이동·들어옴·나감, 버튼, 휠,
  터치, 핀치·팬·회전·두 번 탭 제스처, 터치패드 압력, 포인터 위치에 끌어 온·놓은 파일. 분류는 winit `WindowEvent`의
  모든 변종을 이름으로 나누므로(`held_order`) winit에 새 이벤트가 생기면 컴파일이 분류를 요구한다. 앞선 입력의 패스에는 키보다 먼저 온 입력만,
  편집의 패스에는 편집만 들어가므로 키 뒤의 클릭이 편집 대상을 바꾸거나 편집의 결과로 세어지지 않는다. 예외는 Wayland
  읽기를 기다리는 동안이다. 그동안의 포인터 입력은 평소대로 처리하고, 클릭으로 포커스가 바뀌었으면 사용자가 그 입력칸을
  떠난 것으로 보고 붙여넣기와 원래 키를 버린다([네이티브 Wayland 붙여넣기](#네이티브-wayland-붙여넣기)). 다만 이미 줄 선
  이벤트가 있으면 포인터 입력도 그 뒤에 줄을 서서 앞지르지 않는다.
- 자체 키 처리를 가진 보조 창(설정 창의 설정 열기 키, 종료 확인 창의 Escape 등)은 판정이 가져간 키와 그 뒤에 줄 선
  이벤트를 egui에도 창 자체 처리에도 주지 않는다. 보류가 풀리면 원래 키(재생할 때)와 줄 선 이벤트를 받은 순서대로
  egui와 창 자체 처리에 함께 다시 준다.
- 시간 초과로 재생하는 것은 대상이 그 편집을 실행하지 않았음이 보장될 때만이다(`replays_on_timeout`). 실행됐을 수
  있는 대상은 키를 소비하고 경고를 남긴다. 붙여넣기가 두 번 되는 것을 막기 위해서다.
- 결과가 오기 전에 대상이 바뀌거나 사라지면(`TargetGone`) 실행 여부를 모르므로 재생하지 않는다.

### host egui 입력칸

메인 창의 오버레이(host 팝업·모달·HTML 주소 띠·전체화면 무대)와 egui로 그리는 탐색기 surface의 입력칸(주소·이름),
설정·Preset·Plugins·종료 확인 창, 셸 경로 설정·부팅 오류·홈 사용 중 화면이 대상이다. 판정은 egui-winit이 키를
자기 기본 편집으로 바꾸기 전에 한다(`src/gfx/gpu/edit_keys.rs`). egui-winit의 기본 키 변환은 걷어내지 않는다.

- 바인딩이 없으면 `NoBinding`이다. egui-winit 기본 키(Ctrl/⌘+C·X·V 등)는 바인딩에서 지워도 egui 입력의 원래 동작을 한다.
- 키 시점 판정은 직전 프레임까지 처리된 egui 상태로 한다.
  - 복사·잘라내기: 포커스 입력칸의 선택도 label 선택도 없으면 `Pass`다.
  - 붙여넣기: 포커스 입력칸이 없거나 클립보드 텍스트가 비었으면 `Pass`다.
  - 포커스 입력칸 없이 label 선택만 있는 복사는 `Act`다. label 선택은 egui가 항상 복사한다.
  - 그 밖은 `Hold`다. egui 입력에 `Copy`·`Cut`·`Paste(text)`를 넣는다.
- 메인 창은 탐색기 surface의 입력칸 여부를 직전 렌더가 남긴 상태로 안다. 탐색기 파일 목록에 포커스가 있는데 egui가
  아직 처리하지 않은 입력(입력칸 클릭 등)이 있으면 그 상태가 낡았을 수 있으므로, 편집 바인딩 키를 보류해 그 입력의
  패스를 먼저 돌린 뒤 원래 키를 평소 이벤트 경로로 돌려보내 새로 판정한다. 입력칸이 포커스를 받았으면 위 판정을 하고,
  파일 목록이면 파일 액션 디스패치가 받는다.
- egui가 아직 처리하지 않은 입력(클릭·Tab·Ctrl+A·입력한 글자)이 있으면 키 시점 상태가 낡았으므로 판정하지 않고 키를
  보류한다. 앞선 입력만 한 패스로 먼저 처리하고, 그 패스가 끝난 상태에서 위 키 시점 판정을 다시 한다. `Pass`면 원래
  키를 재생하고 `Hold`면 그때 편집을 넣는다. 앞선 입력과 편집이 한 패스에 섞이면 앞선 입력이 바꾼 텍스트를 편집의
  결과로 세기 때문이다. 다시 판정할 때 새 미처리 입력이 있으면 한 번 더 기다린다.
- Tab은 포커스 이동이 한 패스 늦다. egui 0.31은 Shift+Tab의 포커스 이동을 다음 패스 시작에 적용하고, 마지막 입력칸에서
  누른 Tab도 다음 패스에서 첫 입력칸으로 포커스를 넘긴다. 그래서 Tab이 든 패스 직후 읽은 포커스는 편집이 실제로 갈
  입력칸이 아니다.
  - 편집 키보다 먼저 온 미처리 입력에 Tab이 있으면, 그 입력의 패스 뒤 빈 패스를 한 번 더 돌린 뒤 판정한다.
  - 보류가 없을 때도 창은 마지막으로 가져간 프레임 입력에 Tab이 있었는지 기억한다. 그 패스가 끝나고 다음 프레임 전에
    편집 키가 오면(미처리 입력은 없다) 키 시점에 판정하지 않고 보류해 빈 패스 한 번 뒤 판정한다. 메인 창의 탐색기
    입력칸 여부 판단도 이 상태를 미처리 입력처럼 다룬다.
- `Hold`의 결과는 편집을 넣은 그 egui 패스의 출력으로 정한다.
  - 복사는 그 패스에 복사 출력(`CopyText`)이 났으면 `Acted`다. 비밀번호 입력칸은 복사하지 않으므로 `NotActed`다.
  - 잘라내기는 그 패스에서 입력칸 텍스트가 바뀌었으면 `Acted`다. egui가 바뀐 `TextEdit`마다 출력 이벤트에 바뀌기
    전·후 텍스트(비밀번호는 가린 글자)를 남기므로 둘을 비교한다. 비어 있지 않은 선택을 지우면 길이가 줄어 늘 다르다.
    읽기 전용 입력칸은 바뀜 표시만 하고 텍스트가 같으므로 `NotActed`다.
  - 붙여넣기는 텍스트 비교로 판정하지 않는다. 같은 텍스트로 덮어쓰거나 비밀번호를 같은 길이로 바꾸면 전·후가 같기
    때문이다. 편집을 넣을 때 포커스 입력칸의 선택 시작(선택이 없으면 커서) 위치를 적어 두고, 그 패스에서 입력칸이 편집을
    실행했으며 패스 뒤 커서가 적어 둔 위치보다 뒤에 있으면 `Acted`다. egui 0.31은 붙여넣은 글자 뒤에 커서를 두고, 읽기
    전용 입력칸은 커서를 선택 시작에 둔다. 편집 실행은 그 패스 출력의 `TextEdit` 값 바뀜 이벤트(`ValueChanged`)로 본다.
    egui는 그 패스에 그려지고 입력을 받으며(interactive) 포커스를 가진 입력칸이 편집을 적용했을 때만 이 이벤트를 낸다(읽기
    전용 입력칸도 낸다). 그 패스에 비활성이거나 그려지지 않은 입력칸은 저장된 선택이 그대로 남아 커서만 보면 실행한
    것처럼 보이지만, 이 이벤트가 없으므로 `NotActed`다.
    입력칸에 아직 커서 상태가 없어 위치를 적지 못했으면(선택도 없다) 붙여넣기는 텍스트를 늘리므로 텍스트 비교로 판정한다.
  - 텍스트를 지우지 못한 잘라내기는 egui가 만든 복사 출력도 버린다. 넘긴 원래 키를 받은 다음 처리자(egui-winit 기본
    Cut)가 복사하는 것은 그 처리자의 동작이므로 막지 않는다.
  - `Acted`인 복사·잘라내기의 복사 출력은 다른 egui 복사와 같이 그 창의 복사 문([클립보드](../../features/clipboard/index.md))이
    쓰고 토스트를 띄운다.
- 결과는 다음 프레임에 나므로 그리는 창에는 시간 초과가 없다. 프레임이 늦게 와도(멈춤·부하) 그 패스 결과로 판정한다.
  프레임이 오지 않는 창(최소화·숨김)에서만, 편집을 egui 입력에 넣은 때(키 시점, 다시 판정한 때, Wayland 텍스트가
  도착한 때)부터 1초가 지난 뒤 그 창에 이벤트가 오면 넣은 편집을 거둬들이고 원래 키를 재생한다. 앞선 입력의 패스를
  기다리던 키도 같은 조건에서 키 시점부터 1초가 지나면 재생한다. 프레임이 오지 않는지는 winit의 최소화·표시 여부로
  본다. Wayland처럼 답하지 않는 환경과 가려진 창은 그리는 창으로 본다.
- 재생은 창마다 다르다. 키를 egui에만 주는 창(셸 경로 설정·부팅 오류·홈 사용 중 화면)은 그 창의 GPU 상태가 다음
  프레임 입력을 가져가기 전에 egui-winit에 넘긴다. 자체 키 처리를 가진 보조 창(설정·Preset·Plugins·종료 확인)은 다음
  그리기 전에 그 창이 egui와 자체 처리에 넘긴다. 메인 창은 App이 다음 그리기 전에 그 창의 평소 이벤트 경로로 넘겨
  egui-winit과 Tasty 단축키가 받는다.
- 판정 순서는 소스 가드(`src/source_guards/edit_key_judgment_order.rs`)가 호출 위치로 확인한다. egui 전용 창의 두
  입구와 메인 창 이벤트 경로에서 판정이 egui-winit 호출보다 앞에 있는지, 보조 창이 공통 흐름
  (`src/view/judged_event.rs`)으로만 egui에 넘기는지를 본다. 조건 분기의 실제 실행과 이 목록 밖의 창은 보지 않는다.
  보조 창의 판정 결과 → 자체 처리 분기는 그 공통 흐름의 시험이 가짜 창으로 동작을 확인한다. 판정이 가져간 이벤트는
  창 자체 처리에 가지 않고, 넘긴 이벤트는 egui 뒤 자체 처리에 간다.
- 바인딩 출처: 메인 창은 그 창 engine의 설정, 보조 창은 최신 적용 설정(이벤트마다 맞춘다), 부팅 오류 화면은 그 화면에
  들어갈 때의 최신 적용 설정, 셸 경로
  설정 화면은 부팅 설정, 홈 사용 중 화면은 읽은 설정 파일(읽지 못하면 기본값)이다. 설정 창의 편집 초안은 쓰지 않는다.
  설정 창이 단축키를 녹화하는 동안에는 판정하지 않는다.

### 네이티브 Wayland 붙여넣기

네이티브 Wayland 시작의 host egui 붙여넣기는 selection 읽기의 네 결과를 구별한다.

- 자기 selection(`Ready`): 비어 있지 않으면 위 `Hold`다.
- 텍스트 offer 없음(`Empty`)과 읽기 실패: `Pass`다.
- 다른 앱 selection(`Pending`): 원래 키를 보류하고 읽기를 기다린다. 도착한 텍스트가 있으면 `Paste`를 넣어 위처럼
  결과로 판정한다. 비었거나 실패했으면 재생한다. 읽기가 끝나지 않으면 selection 읽기 상한(3초)에 0.5초를 더한 뒤 읽기를
  취소하고 재생한다. 도착했을 때 포커스 입력칸이 바뀌었으면 읽기 결과(텍스트·빈 결과·실패)와 관계없이 텍스트를 버리고
  원래 키도 재생하지 않는다. 사용자가 이미 다른 곳으로 옮겼기 때문이다. 읽는 동안 통과시킨 포인터 입력(클릭)을 egui가
  아직 처리하지 않았으면 그 입력의 패스를 먼저 돌려 포커스가 정해진 뒤 이 판단을 한다. 도착 시점의 포커스는 그 클릭이
  옮기기 전의 것이기 때문이다.

플러그인 입력칸의 같은 경우는 [플러그인 입력칸](#플러그인-입력칸)의 기다림이다.

### 플러그인 입력칸

플러그인 egui-mesh 팝업·surface의 입력칸은 다른 프로세스에 있으므로 편집이 일어났는지는 플러그인이 회신한다
([플러그인 개발 — 편집 결과 회신](../../dev-guide/plugin-development.md#편집-결과-회신)).

- 회신을 약속하지 않은 플러그인(hello 에 `edit_outcome` 없음)에는 편집 요청을 보내지 않고 `Pass`다. 번들 플러그인은 지금
  약속하지 않는다.
- 약속한 플러그인에는 요청 id 가 붙은 편집을 보내고 원래 키를 그 창에 붙잡는다(`src/app/plugin_edit_holds.rs`). 창마다
  보류는 하나다. 보류 중 그 창에 온 키·IME·modifier·포인터 입력은 플러그인 단축키 처리보다 먼저 줄을 선다. 보류가 풀리면
  원래 키를 재생하고, 그 뒤에 줄 선 입력을 받은 순서대로 플러그인 단축키 처리부터 다시 거치게 한다. 보류가 없을 때와 같은
  단계다. 원래 키는 플러그인 단축키 처리를 이미 지나 창 view 에서 붙잡혔으므로 view 로 넘긴다
  (`src/app/edit_key_replay.rs`의 `replay_steps`). 재생 중에 새 보류가 생기면 남은 입력은 그 뒤에 줄을 선다.
- App 은 pump 직후 회신을 꺼내 보낸 창의 보류를 푼다(`settle_plugin_edit_holds`).
  - `acted`: 소비한다. 복사 글자가 있으면 그 창의 복사 문이 쓰고 토스트를 띄운다.
  - `not_acted`: 원래 키를 한 번 재생한다.
  - `unknown`: 실행됐을 수 있으므로 소비하고 재생하지 않는다. 복사 글자가 있으면 쓴다.
  - 플러그인 실행이 끝남(종료·비활성화·재시작·연결 끊김): `TargetGone`이다. 재생하지 않는다.
- 회신이 1초 안에 오지 않으면 소비하고 경고를 남긴다(`replays_on_timeout = false`). 그 요청은 더 기다리지 않고 늦은
  회신은 버린다. 이 시한에 맞춰 이벤트 루프가 깨어난다.
- 회신은 요청을 받은 플러그인 실행·대상·id 가 모두 맞을 때만 그 요청의 결과다. 다른 플러그인·대상·이전 실행의 회신,
  중복·늦은 회신, 연결이 끊긴 배치의 회신은 버린다(`crates/tasty-host-plugin/src/manager/edit_outcome.rs`). 결과를
  기다리는 요청이 64개를 넘으면 가장 오래된 요청이 `unknown`으로 끝난다.
- 보류가 풀리기 전에 창이 닫혔으면 복사 글자와 재생할 입력은 받을 곳이 없어 버려진다.
- 플러그인은 실행 중인데 대상 팝업이 닫혔거나 그 창의 키를 다른 팝업(호스트 팝업 포함)이 받게 됐으면, App 이 pump 직후
  그 보류를 바로 끝낸다(`end_lost_targets`, 판정은 `popup_target_live`). 실행 여부를 모르므로 원래 키는 재생하지 않고 경고도
  남기지 않으며, 그 요청은 더 기다리지 않는다. 줄 선 입력은 그때 바로 재생한다. 이 판정은 같은 pump 의 회신보다 먼저
  한다(`PluginEditHolds::settle`). 그래서 같은 pump 에 닫힘과 회신이 겹쳐도 그 회신은 원래 키를 재생하거나 복사 글자를
  쓰지 않고 버려진다. surface 대상 보류는 그 창의 키를 지금도 그 surface 가 받을 때만 살아 있다
  (`hold_target_live`, `MainView::plugin_surface_key_target`). 닫힌 surface 는 포커스를 가질 수 없으므로 같은 판정으로 끝난다.
- headless 실행은 키를 붙잡는 창이 없으므로 회신을 pump 직후 꺼내 버린다(`src/boot/headless_plugins.rs`).
- 플러그인 팝업이 키를 받으면 메인 창이 egui-winit 보다 먼저 판정한다(`src/view/main/plugin_popup_keys.rs`). 키를 받는
  팝업은 렌더와 같은 판정(`MainViewState::popup_key_owner`)으로 정한다. 호스트는 팝업 안 입력칸의 선택을 모르므로
  `Act`를 내지 않는다.
  - 바인딩이 없으면 `NoBinding`이다. egui-winit 기본 편집 키는 egui-winit 이 `Cut`·`Copy`·`Paste`로 바꿔 그 팝업에 간다.
  - 붙여넣기인데 클립보드에 글자가 없으면 `Pass`다. selection 읽기 오류도 경고 로그를 남기고 `Pass`다(호스트 egui
    입력칸과 같은 처리다). 네이티브 Wayland 의 다른 앱 selection 을 읽는 중이면 아래 기다림이다.
  - 그 밖은 원래 키와 편집을 창에 남기고 App 이 그 창 view 처리 직후(플러그인 단축키 단계 뒤) 꺼낸다
    (`src/app/plugin_popup_edit.rs`). 팝업의 플러그인이 회신을 약속했으면 요청을 다음 렌더의 그 팝업 입력에 붙이고 원래
    키를 보류한다. 약속하지 않았거나 팝업이 그 사이 닫혔으면 원래 키를 넘긴 편집으로 한 번 재생한다(`Pass`와 같다).
  - 회신과 재생은 키를 보낸 때의 팝업 instance 로 짝짓는다. 요청과 보류는 그 instance 를 대상으로 갖는다. 회신을 적용하기
    전에 그 팝업이 열려 있고 그 창의 키를 지금도 받는지 확인한다. 키 소유자가 바뀌었으면(다른 plugin 팝업이나 호스트 팝업이
    위로 왔다) 그 보류는 원래 키 재생과 복사 없이 끝나고 회신은 버려진다.
- 플러그인 egui-mesh surface 가 키를 받으면 팝업 판정 다음, 호스트 입력칸 판정 앞에서 판정한다
  (`src/view/main/plugin_surface_keys.rs`). 호스트 egui(팝업·모달·무대)가 키를 받는 동안은 판정하지 않는다.
  - 회신을 약속한 플러그인의 로컬 surface 만 판정한다. 약속 여부와 그 surface 의 플러그인은 직전 렌더가 기록한 값을 키
    시점에 읽는다. 판정은 팝업과 같다(바인딩 없음은 `NoBinding`, 빈 클립보드의 붙여넣기는 `Pass`, 그 밖은 원래 키를 App 에
    맡겨 요청과 보류로 바꾼다). 넘긴 키는 플러그인에 `Key`로 간다. 예외로 `egui_paste` kind 의 surface 는 글자가 없어도
    플러그인이 다른 형식을 붙일 수 있어 빈 클립보드의 붙여넣기도 보류하고 형식 붙여넣기(`paste`) 요청을 보낸다. 회신이
    소비·넘김을 정한다.
  - 회신을 약속하지 않은 플러그인(번들 image 포함)의 surface 는 판정하지 않고 기존 단축키 경로로 간다(`egui_copy` kind 의
    `Copy`, `egui_paste` kind 의 `Paste` 전달, 그 밖은 터미널 붙여넣기).
  - 원격 mirror surface 는 원격 결과를 받을 길이 없어 편집 바인딩도 `Pass`다. 편집 이벤트를 보내지 않고 원래 키가 원격
    플러그인에 `Key`로 간다.
  - surface 의 편집이 복사한 글자는 그 창의 복사 문이 쓰고 `toast.copied`를 그 surface 범위에 띄운다. 팝업의 편집은 창
    범위다(`copy_scope`).
- 명령 팔레트의 복사·잘라내기·붙여넣기는 포커스 surface 가 회신을 약속한 플러그인의 로컬 surface 이면 기존 팔레트 경로보다
  먼저 그 플러그인에 요청을 보낸다(`MainView::ask_plugin_surface_palette_edit`). 붙여넣기인데 클립보드에 글자가 없으면 보내지
  않고 끝난다(터미널로 넘기지 않는다). 키가 없으므로 붙잡을 키도 재생할 키도 없다. App 은 팔레트가 실행되는 렌더 뒤에
  요청을 꺼내 키 없는 요청으로 기록하고(`PluginEditHolds::track`), 창 입력은 줄 세우지 않는다. 회신의 복사 글자만 쓰며,
  대상을 잃거나 1초가 지나면 결과 없이 끝난다.
- 네이티브 Wayland 에서 다른 앱의 selection 을 읽는 중(`Pending`)이면 회신을 약속한 플러그인 팝업·로컬 surface·팔레트
  붙여넣기는 원래 키를 붙잡고 읽기를 기다린다(`src/app/plugin_paste_await.rs`). 플러그인에는 아직 아무것도 보내지 않는다.
  키 시점에 편집 요청 id 를 받아 두고, 읽기에는 요청마다 새 tag 를 붙여 결과가 호스트 입력칸이나 egui 의 `Paste`로 가지
  않게 한다(`src/gfx/gpu/plugin_paste_read.rs`). 기다리는 동안 그 창의 키·IME·modifier 는 줄을 서고, 포인터 입력은 이미
  줄 선 입력이 있을 때만 그 뒤에 선다. 팔레트 붙여넣기도 기다리는 동안은 창 입력을 줄 세운다.
  - 글자가 오면 그 id 로 `paste_text` 요청을 대상에 붙이고 위의 회신 보류로 옮긴다. 줄 선 입력은 회신 보류 뒤에 그대로
    선다. 팔레트 붙여넣기는 키 없는 요청으로 기록하고 줄 선 입력을 바로 재생한다.
  - 비었거나 읽기가 실패하면 원래 키를 한 번 재생한다. `egui_paste` kind 의 surface 는 형식 붙여넣기(`paste`) 요청을 대신
    보내고 회신 보류로 옮긴다.
  - 호스트 입력칸과 같은 시한(selection 읽기 상한 3초에 0.5초를 더한 값)까지 읽기가 끝나지 않으면 읽기를 취소하고 원래 키를
    재생한다. 플러그인에 보낸 것이 없으므로 두 번 실행되지 않는다.
  - 대상 팝업·surface 가 닫히거나 그 창의 키를 잃으면, 그 창에 포인터 누름이 오거나 창이 포커스를 잃으면, 다른 읽기(다른
    창, 터미널 붙여넣기)가 공유 읽기를 대신하면, 그 플러그인 실행이 끝나면(종료·재시작) 기다림을 끝낸다. 원래 키는 재생하지
    않고 줄 선 입력만 재생한다. 포인터 누름은 줄 선 입력 뒤에 처리한다. 호스트는 대상 안 입력칸의 포커스를 볼 수 없어 누름이
    포커스를 옮겼는지 모르기 때문이다. 대상 상실은 같은 회차에 도착한 읽기 결과보다 먼저 본다.
  - 포인터 누름·포커스 상실로 끝낸 기다림의 줄 선 입력에 붙여넣기 키가 있으면, 그 재생은 다시 읽기를 시작해 새 기다림을
    만들 수 있다. 그 기다림도 같은 이벤트로 끝내며 기다림이 남지 않을 때까지 반복한다(`interrupt_awaits`). 그래서 그
    이벤트 뒤에는 어떤 기다림도 남지 않는다.
  - 끝낸 기다림의 읽기 결과가 늦게 오면 버린다. 다시 포커스를 얻어도 붙여넣지 않는다.
  - 회신을 약속하지 않은 플러그인 팝업은 기다리지 않는다. 읽기를 취소하고 `Pass`이며 egui-winit 기본 붙여넣기 키면
    egui-winit 경로가 다시 읽어 늦게 붙여넣는다. 기다림이 빈 값·시한으로 끝나 팝업에 재생한 기본 붙여넣기 키도 같은 경로를
    탄다.

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

매칭 대상은 포커스에 따라 다르다. native WebView가 넘긴 키는 모델 포커스가 아니라 그 키를 받은 WebView의 surface를 포커스로 보고, 명령 대상도 그 surface로 정한다. attach 생성 대기 초안이 선택돼 있으면 포커스 surface가 없으므로 초안 뒤의 기존 plugin surface가 아니라 global 명령만 후보다.

| 포커스된 surface | 후보 명령 |
|---|---|
| plugin 소유 `RemoteSurface` | 해당 plugin의 `Global`·`Surface` 명령 |
| plugin surface가 아님 | 등록된 모든 plugin의 `CommandScope::Global` 명령 |

`Surface` 명령은 소유 plugin의 surface가 포커스됐을 때만 실행한다. 여러 plugin이 같은 Global 키를 등록하면 레지스트리 순회에서 처음 일치한 명령을 사용한다. 내부 저장소는 HashMap이므로 plugin 간 등록 순서나 실행 우선순위를 보장하지 않는다. 이 충돌을 해결하는 전용 UI는 아직 없다.

`CommandDecl.action`이 있으면 호스트가 `ToolAction::Event`·`OpenSurface`·`OpenPopup`을 도구 메뉴와 같은 방식으로 실행한다. 이때 `command.invoke`(`handle_command`)는 보내지 않아 중복 실행을 막는다. `action`이 없으면 기존 `handle_command` IPC를 사용한다.

Event Bus의 `command.invoked`는 소유 plugin에 보내는 알림이다. 명령을 실제로 실행할 때 보낸다. `action`이 없는 명령은 대상 surface가 없어도 보낸다. 명령의 대상 surface는 키를 누른 시점에 정한다. attach 생성 대기 초안이 선택돼 있으면 popup을 여는 `action`은 초안에 걸어 두고 키를 소비하며, 생성된 surface에서 실행할 때 알린다. 생성이 실패해 버린 명령은 알리지 않는다. 대상 surface도 초안도 없는데 `action`이 `surface` 범위 popup을 열거나, 팔레트 요청을 처리할 때 그 창이 사라졌거나, 고른 surface·생성된 surface가 실행 전에 사라졌거나, 팔레트에서 초안을 대상으로 고른 명령이 입력 뒤 popup을 여는 명령으로 바뀌었으면 실행하지 않고 알리지도 않는다. 단축키는 대상 surface도 초안도 없어 실행할 수 없을 때 키를 다음 처리자에 넘긴다.

내장 단축키도 대상 surface를 키를 누른 시점에 정한다. native WebView가 넘긴 키는 그 키를 받은
WebView의 surface가 대상이다. surface 하나를 대상으로 하는 내장 액션은 그 surface의 kind가 받으면 그
surface에서 실행하고, 받지 않으면 모델 포커스가 다른 surface일 때 그쪽으로 보내지 않고 페이지로 넘긴다.
단 같은 키가 surface 하나를 대상으로 하지 않는 전역 단축키(F2의 `rename_tab` 등)에도 묶여 있으면 페이지로
넘기지 않고 전역 단축키만 처리한다. 이 단축키의 대상은 모델 포커스다. 포커스 surface의 목록 범위 키(탐색기
목록의 `explorer_rename` 등)를 고르는 단계는 거치지 않으므로, 모델 포커스가 탐색기여도 키를 받은 surface가 아닌
그 탐색기의 항목 이름 바꾸기는 실행되지 않는다. 예를 들어 html이 받은 F2는 모델 포커스와 관계없이 `rename_tab`으로
실행된다.
새로고침은 앞 단계가 키를 받은 surface로 처리하고, 페이지 편집 키는 이 판정에서 고르지 않는다. 생성 대기 초안이 선택돼 있으면 surface 하나를
대상으로 하는 내장 액션은 초안이 만들 kind가 받을 때 초안에 걸고 키를 소비하며, 생성된 surface에서
실행한다. 받지 않는 액션은 걸지 않으며 초안 뒤의 기존 surface·탭·workspace를 대상으로 삼지 않는다. 그
키는 다음 처리자로 넘길 수 있다. 초안 뒤 기존 탭의 kind에 따라 대상 없이 소비하는 경우도 있다. 대상
목록과 버림 조건은 [attach 동작](../../dev-guide/attach-behavior.md)에 있다.

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
- 편집 키 판정: `src/adapters/ui/input/edit_key.rs`(판정 함수·보류), `src/gfx/gpu/edit_keys.rs`(host egui 판정·결과·
  Wayland), `src/view/main/edit_keys.rs`(메인 창 입구), `src/app/edit_key_replay.rs`(메인 창 재생·보조 창 바인딩),
  `src/app/plugin_edit_holds.rs`(플러그인 입력칸 보류·회신 분배).
- 텍스트 입력과 겹치는 경우의 처리: `src/adapters/ui/surface/explorer/type_ahead.rs`
  (`unmodified_binding_chars`).
- Plugin 커맨드: `crates/tasty-host-plugin/src/command_registry.rs`(`PluginCommandRegistry`,
  `effective_binding`), `src/plugin_bridge/key_dispatch.rs`(`match_plugin_shortcut`/
  `match_global_shortcut`/`dispatch_plugin_command`), `src/app/plugin_glue/shortcut.rs`
  (`App::try_plugin_shortcut`).
