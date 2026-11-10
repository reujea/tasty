# 단축키 (Keybindings)

- **Status**: Implemented
- **주체**: 로컬 사용자
- **설계**: [키 매핑 설계 (운영 상세)](../../design/policies/key-mapping.md) — 파서 위치, 이식 번들, 가져오기 적용 단위. 입력 정책은 [키 매핑](../../design/policies/key-mapping.md)을 따른다.
- **코드**: `crates/tasty-settings/src/keybindings.rs` (+ `crud.rs` · `presets.rs` · `parse.rs`) · 이식 번들 `crates/tasty-host-plugin/src/keybinding_bundle.rs` · 가져오기/내보내기 화면 `src/view/settings/ui/keybindings_tab/import_export.rs`
- **화면**: [설정 창](../settings/screens/settings.md) Keybindings 탭

## 목적

ESC·Enter처럼 당연하게 쓰이는 기본 조작을 제외한 Tasty 자체 **단축키는 `KeybindingSettings` 한 곳에서 정의**되며 코드에 하드코딩되지 않는다([CLAUDE.md](../../../CLAUDE.md) "단축키" 필수 정책). 사용자가 Settings 의 Keybindings 탭에서 액션별 키 조합을 추가/삭제/변경한다. OS 메뉴(macOS NSMenu / Windows AcceleratorTable)의 key equivalent 도 이 binding 을 따라간다.

## 내부 동작

### 액션 ↔ 바인딩 목록

각 액션은 **바인딩 문자열의 `Vec`** 를 가진다(다중 바인딩 — 한 액션에 여러 키 조합 허용). 예: `copy`, `paste`, `enter_copy_mode`, `apply_workspace_preset` 등. 빈 `vec` 이면 그 액션엔 단축키가 없다(메뉴엔 단축키 없는 항목으로 표시). OS 메뉴 항목에는 첫 조합만 key equivalent 로 표시되며, 나머지 조합도 단축키 경로에서 실행된다. macOS 창 최소화·확대·닫기는 첫 조합을 메뉴가, 나머지를 단축키 경로가 실행한다([키 매핑 정책](../../design/policies/key-mapping.md#os-메뉴-key-equivalent)).

바인딩 문자열은 **OS 독립 표기**다 — 위치 기반 추상화로 macOS 에선 `alt`→⌘ 등으로 매핑된다([key-mapping](../../design/policies/key-mapping.md)). 쓸 수 있는 키 토큰(문자 하나, `plus`·`minus`·`equals`, `f1`~`f24` 를 포함한 이름 키)은 [바인딩 문자열 문법](../../design/policies/key-mapping.md#바인딩-문자열-문법)에 있다.

그 문자열을 보조키와 일반 키로 나누는 **파싱**(`parse_binding` · 축 modifier 조합의 `Combo::parse_modifiers`)은 값을 저장하는 크레이트인 `tasty_settings::keybindings::parse` 에 있고, 그 결과를 실제 키 이벤트와 대조하는 **매칭**만 `src/adapters/ui/input/shortcuts/` 에 남는다. 이식 판정처럼 UI 밖에서도 같은 규칙이 필요하기 때문이다([키 매핑 설계 (운영 상세)](../../design/policies/key-mapping.md)).

사이드바 도구 메뉴의 호스트 항목 일곱에는 각각 단축키 필드가 있다:
`toggle_command_palette`, `open_port_scanner`, `open_remote_tool`, `open_preset_window`,
`open_tutorial`, `toggle_dag_list`, `open_file_picker`. 팔레트와 DAG 목록을 제외한 다섯
항목은 네 프리셋 모두 기본값이 비어 있다. 새 기본값을 넣으면 기존 사용자 조합과 충돌할
수 있고, `remove_conflicts_from_defaults`가 충돌한 기본값을 제거할 수 있기 때문이다.
plugin 도구의 단축키는 `[[contributes.commands]]`로 선언하고 Plugins 서브탭에서 관리한다.

### 입력칸 바인딩 — `TEXT_FIELD_BINDING_FIELDS`

`code_area_apply`·`code_area_cancel` 은 여러 줄 코드 입력칸(`tasty_ui_widgets::CodeArea`, 지금은 Settings › Handler › Hook Handlers 의 IpcSequence 편집기)에 포커스가 있을 때만 검사하는 확정·취소 키다. 전역 목록 `GENERAL_BINDING_FIELDS` 가 아니라 `KeybindingSettings::TEXT_FIELD_BINDING_FIELDS` 에 있다.

- 전역 키 경로(메인 윈도우 단축키 매칭·명령 팔레트·modifier hint·webview 키 선점·탐색기 type-ahead)는 `GENERAL_BINDING_FIELDS` 만 읽는다. 그래서 `escape`·`ctrl+enter` 가 터미널이나 webview 에서 가로채이지 않는다.
- 위젯은 키를 정하지 않는다. 호출자가 `CodeAreaKeys` 로 판정 함수를 넘기고, 본체는 `tasty_key_match::consume_binding_egui` 로 그 바인딩과 맞는 키 누름을 TextEdit 보다 먼저 소비한다(확정 키가 줄바꿈으로 들어가지 않게). 그 키 누름 바로 뒤의 글자 이벤트도 함께 지운다. egui-winit 은 ctrl·command 가 없는 누름(Alt, 비-macOS 의 Super, macOS 의 Option)마다 글자를 넣기 때문이다. 맞지 않은 누름의 글자(AltGr 로 친 `@` 등)는 그대로 둔다. 매칭 규칙은 다른 바인딩과 같다(macOS 에서 `alt` = Command).
- 충돌 검사(`find_conflict`)와 기본값 충돌 제거(`remove_conflicts_from_defaults`)는 필드가 속한 목록 안에서만 한다. 입력칸 취소 `escape` 는 전역 `fullscreen_stage_exit` 의 `escape` 와 충돌하지 않는다.
- 설정 화면·프리셋 비교·가져오기/내보내기는 두 목록을 합친 `KeybindingSettings::binding_fields()` 를 읽는다. 두 행은 General 서브탭의 `fullscreen_stage_exit` 다음에 설명과 함께 나온다.
- 녹화 중 `Esc` 는 슬롯 비우기라 `code_area_cancel` 에 `escape` 를 다시 녹화할 수 없다. 프리셋을 재적용해 되돌린다.

### 탐색기 목록 바인딩 — `EXPLORER_LIST_BINDING_FIELDS`

탐색기 칸에 포커스가 있을 때만 검사하는 20 개다. 현재 항목을 옮기는 `explorer_cursor_{up,down,left,right,home,end,page_up,page_down}`(기본 방향키·`home`·`end`·`pageup`·`pagedown`), 같은 이동으로 선택을 넓히는 `explorer_extend_*`(기본 같은 키에 `shift+`), 선택 항목의 `explorer_rename`(기본 `f2`)·`explorer_trash`(Tasty 프리셋 `delete`·`alt+backspace`, Mac `alt+backspace`, Windows·Linux `delete`), 하나만 고른 항목을 여는 `explorer_open`(기본 `enter`), 선택을 푸는 `explorer_clear_selection`(기본 `escape`)이 있다. 동작은 [Explorer](../../surfaces/explorer/index.md#현재-항목--영역-선택)에 있다.

- 전역 키 경로는 이 목록을 읽지 않는다. 방향키·`delete`·`enter`·`escape` 를 터미널·webview 에서 가로채지 않는다.
- 녹화 중 `Esc` 는 슬롯 비우기라 `explorer_clear_selection` 에 `escape` 를 다시 녹화할 수 없다. 프리셋을 재적용해 되돌린다.
- 탐색기 키 경로(`handle_explorer_shortcut`)는 전역 바인딩보다 먼저 키를 본다. 그래서 탐색기에 포커스가 있으면 `f2` 는 `rename_tab` 대신 `explorer_rename` 이다.
- 충돌 검사와 기본값 충돌 제거는 이 목록 안에서만 한다. 기존 설정에 `rename_tab = f2` 가 있어도 `explorer_rename` 의 기본값은 지워지지 않는다.
- 명령 팔레트는 이 목록 가운데 `explorer_rename`·`explorer_trash` 만 싣는다(`PALETTE_EXPLORER_ITEM`).
- 설정 화면에서는 Explorer 서브탭에 이름 변경·휴지통·열기·선택 해제, 이동 8 행, 넓히기 8 행 순으로 나온다. 이동·넓히기 첫 행 아래에 탐색기 포커스에서만 동작한다는 설명이 붙는다.

`open_preset_window`(도구 메뉴의 **프리셋** 윈도우 열기)와 `apply_workspace_preset`/`apply_tab_preset`/`apply_pane_preset`(레이아웃 프리셋 **적용** picker)은 다른 것이다. 이름이 비슷해 섞기 쉬우므로 설정 화면에서도 서브탭이 갈린다 — 앞은 General, 뒤 셋은 대상 스코프를 따라 Workspace/Tab/Pane 이다.

### 공통 새로고침 — `refresh`

`refresh` 는 특정 surface 구현에 묶이지 않은 전역 동작이다. 네 프리셋 모두 기본값은 `f5` 이며 설정의 **Surface** 서브탭에 있다. 키를 누르면 host 가 그 시점의 대상 surface 로 [`surface.refresh_requested`](../../reference/event-catalog.md#surface-scopesurface) 신호를 한 번 발행한다. 신호 발행은 요청 접수이고, 갱신할지와 무엇을 갱신할지는 그 surface 의 처리자가 정한다.

- **대상**: winit 경로는 모델 포커스 surface, native WebView 경로는 키를 받은 WebView 의 surface 다. 대상은 키를 누른 시점의 surface ID·kind·세대(`SurfaceBinding` — activation·PTY 자원·mirror projection 세대)로 고정해 `UiIntent::RequestSurfaceRefresh` 로 넘긴다. Intent 처리와 plugin 전달 직전에 같은 세대·kind 인지 다시 확인하고, 그 사이 닫혔거나 다른 kind 로 변환됐거나 다시 만들어졌으면 버린다. 나중에 포커스가 옮겨도 새 포커스 surface 로 보내지 않는다.
- **처리자**: Markdown 이 첫 소비자다([Markdown 새로고침](../../surfaces/markdown/index.md)). 탐색기는 host 가 처리자라 같은 키 처리 안에서 목록을 한 번 다시 읽고 신호도 낸다. 터미널은 아무것도 하지 않는다 — 키는 공통 동작으로 소비되므로 PTY 로 F5 제어 시퀀스가 가지 않고 화면·PTY·셸에 변화가 없다. 따라서 기본값 `f5` 가 있는 동안 터미널 안 프로그램(mc·htop·vim 매핑 등)은 F5 를 받지 못한다. 이 프로그램에 F5 를 쓰려면 `refresh` 를 해제하거나 다른 키로 바꾼다. 해제하면 F5 는 다시 PTY 로 간다. 다른 kind(html 등)는 신호만 발행되고 처리자가 없으면 아무 일도 없다. host 는 "갱신 성공" 을 표시하지 않는다.
- **탐색기 전용 키와의 공존**: `explorer_refresh` 는 그대로 남는다. 공통 키 검사가 탐색기 전용 키보다 먼저라, 두 필드가 같은 조합(기본 `f5`)이어도 탐색기에서는 한 번만 새로고침한다. 두 필드는 `SHARED_COMBO_PAIRS` 로 같은 조합을 충돌 없이 공유한다. 기존 설정 파일에 `refresh` 가 없고 `explorer_refresh = ["f5"]` 가 있어도 기본 `f5` 는 지워지지 않는다. 다른 동작에 사용자가 `f5` 를 줬다면 기존 규칙대로 기본값에서 걷어 낸다. 탐색기 이름 변경·주소 입력 중에는 입력이 우선이라 두 키 모두 처리하지 않는다.
- **바인딩 변경·해제**: 키를 바꾸면 새 키만 신호를 내고 이전 키는 원래 경로(터미널이면 PTY 입력)로 돌아간다. 빈 목록이면 신호를 내지 않는다.
- **중복 방지와 우선순위**: 한 번의 press 는 한 경로에서만 처리된다. 순서는 페이지 예약 편집 동작(`find`·`copy`·`cut`·`paste`·`select_all`·`page_undo`·`page_redo`) > plugin 명령 > 공통 새로고침·다른 host 동작이다. 설정의 충돌 검사가 예약 동작과 같은 조합을 막고, WebView 정책도 예약 조합과 같은 새로고침 조합은 가져가지 않는다. 그 자리에서 실행 범위에 드는 plugin 명령(포커스가 plugin surface 면 그 plugin 의 명령, 아니면 global 명령)이 같은 키를 쓰면 plugin 명령이 먼저 가져가고 공통 신호는 내지 않는다. 다른 plugin 의 global 명령은 plugin surface 가 포커스인 동안 실행 범위가 아니므로, 그 surface 에서는 같은 키여도 공통 새로고침이 처리된다. native WebView 가 가져간 새로고침 키의 대상 페이지를 잡지 못하면 모델 포커스 surface 로 보내지 않는다. 키를 받은 WebView 가 남아 있는데 그 surface 를 새로고침 대상으로 잡지 못했다면 동작하지 않은 키이므로 Linux 는 그 키를 WebView 에 다시 넣어 페이지·엔진으로 보내고, Windows·macOS 는 가져갈 때 이미 소비했다. 키를 받은 WebView 가 이미 사라졌으면 다시 넣을 곳이 없어 버린다([동작하지 않은 키](../../design/systems/webview.md#편집-액션)). WebView 에서 host 가 가져간 키는 페이지에 전달되지 않으므로 엔진 기본 reload 와 겹치지 않는다. 명령 팔레트의 **refresh** 는 같은 함수를 호출하며 payload 의 `trigger` 만 `palette` 다. OS 메뉴에는 이 항목이 없다. 키를 누르고 있을 때의 반복(auto-repeat)은 경로마다 다르다. host 키 입력(winit — 터미널·탐색기·html 주소 띠)과 Linux native WebView(WebKitGTK)는 반복 이벤트도 각각 하나의 press 로 처리해 반복 수만큼 신호가 난다. Windows(`WasKeyDown`)·macOS(`isARepeat`) native WebView 는 첫 press 만 신호가 되고, host가 가져가는 키의 반복은 다시 실행하지 않고 소비한다(아래 auto-repeat 필터 항목). 처리 중 재요청은 처리자가 정리한다(Markdown mirror 는 새 요청이 이전 요청의 회신을 대신한다).
- **입력 게이트**: 설정·입력 대화상자·포커스된 popup·plugin popup·전체화면 무대가 열려 있으면 다른 단축키와 같이 처리하지 않는다. 설정·플러그인 창 같은 modal 창이 열려 있는 동안에는 main 창의 native WebView 가 키를 받아도 신호를 내지 않는다(아래 native 키 큐의 게이트). Markdown 의 주소칸·검색칸은 WebView 안의 DOM 입력이라 native 키 계층이 먼저 가져가므로 그 Surface 로 전달된다. 단 Markdown 의 갱신은 문서 HTML 을 통째로 다시 싣는다(`load_html`). 그래서 주소칸에 입력하던 draft, 열린 검색 바와 검색어, 선택, 진행 중이던 IME 조합은 초기화되고 스크롤 위치만 best-effort 로 복원된다. 문서 안 Reload 버튼·파일 감시 갱신·`markdown.reload` 와 같은 결과다. html 의 host 주소 띠(egui)는 포커스를 가진 동안 다른 단축키를 막지만, 다른 게이트가 열려 있지 않으면 공통 새로고침 키만 그 띠의 surface 로 보낸다(`try_consume_address_refresh_key`). 띠의 입력 draft·커서·선택은 바꾸지 않는다. 탐색기의 이름 변경·주소 입력 중에는 기존 탐색기 새로고침과 같이 입력이 우선이라 처리하지 않는다. host 가 가져간 새로고침 키는 문자를 만들지 않는다(WebView 위의 보조키 없는 키는 아래 항목). 입력이 초기화되는지는 처리자의 갱신 방식이 정한다.
- **WebView 위의 보조키 없는 키**: 다른 host 동작과 같이 보조키가 없어도 host 가 가져간다. 보조키 없는 문자 키(`r` 등)로 바꾸면 WebView 위에서는 그 글자 입력 대신 새로고침이 된다.
- **외부 자동화**: 이 신호는 사용자 입력에서만 만든다. 에이전트용 IPC·CLI 는 없다(Markdown 은 기존 `markdown.reload` 가 있다).

### 탭/워크스페이스/카테고리 quick-switch (raw 키 + 축별 modifier 조합)

번호 전환·다음/이전 이동은 **콤보가 아니라 raw 키 하나**만 저장하는 별도 필드로 다룬다(단, "개별 지정" 모드 예외 — 아래 참조).
modifier 는 세 축 각자의 독립 필드 `tab_switch_modifier`/`workspace_switch_modifier`/`category_switch_modifier`(각각 기본 `ctrl`/`alt`/`ctrl+shift`)에서 dispatch 시점에 조합되므로, modifier 드롭다운을 바꾸면 그 축의 모든 슬롯이 즉시 재조합된다.
**각 modifier 는 단일 토큰(`"ctrl"`)뿐 아니라 조합(`"ctrl+shift"`)도 허용**하며, 매칭은 일반 바인딩과 동일한 4축 조합 파서(`Combo::parse_modifiers`)를 단일 소스로 쓴다.
카테고리도 1급 축으로 자기 modifier 필드를 갖는다.
이 필드들은 콤보 시스템(`GENERAL_BINDING_FIELDS`/`get_bindings`)과 분리되며 index 기반 accessor(`tab_slot_key`/`set_tab_slot_key` 등, `crud.rs`)로 접근한다.
세 축이 modifier·슬롯 배열·다음/이전을 대칭으로 갖는다는 사실 자체는 `SwitchAxis`/`SwitchStep`(`crud.rs`)이 타입으로 들고 있어, 축을 순회하는 쪽(설정 화면·이식 판정)이 필드 이름을 다시 나열하지 않는다 — 슬롯 수(탭 10 / 워크스페이스 9 / 카테고리 10)도 필드 타입이 쓰는 상수(`TAB_SWITCH_SLOT_COUNT` 등) 하나에서 나온다.

| 필드 | 타입 | 기본값 | 의미 |
|------|------|--------|------|
| `tab_switch_modifier` / `workspace_switch_modifier` / `category_switch_modifier` | `String` | `"ctrl"` / `"alt"` / `"ctrl+shift"` | 축별 modifier 조합. `KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER`(`"individual"`) sentinel 이면 그 축은 규칙 기반이 아니라 "개별 지정" 모드 |
| `tab_switch_slot_keys` | `[String; 10]` | `["1".."9","0"]` | 탭 1~10번 슬롯 |
| `workspace_switch_slot_keys` | `[String; 9]` | `["1".."9"]` | 워크스페이스 1~9번 슬롯(0번 없음) |
| `category_switch_slot_keys` | `[String; 10]` | `["1".."9","0"]` | 카테고리 1~10번 슬롯(reserved normal=1). `category_switch_modifier` 와 합성 |
| `tab_switch_next_key` / `tab_switch_prev_key` | `String` | `"l"` / `"h"` | 탭 다음/이전 |
| `workspace_switch_next_key` / `workspace_switch_prev_key` | `String` | `"j"` / `"k"` | 워크스페이스 다음/이전 |
| `category_switch_next_key` / `category_switch_prev_key` | `String` | `"j"` / `"k"` | 카테고리 다음/이전(다음/이전 **카테고리 자체**로 전환 — 카테고리 내부 워크스페이스 이동인 워크스페이스 축 next/prev 와 다름) |

세 축 모두 슬롯 + 다음/이전을 대칭으로 갖는다. 카테고리 modifier 기본값 `ctrl+shift` 는 macOS 스크린샷 예약(`⌘⇧3/4/5`, tasty 가 가로챌 수 없음)과 겹치지 않게 고른 값이고, 카테고리 next/prev 기본 raw 키 `j`/`k` 는 4 프리셋 전수 대조로 다른 액션과 무충돌임을 확인한 값이다(워크스페이스 축과 문자는 같지만 modifier 가 달라 합성 콤보는 겹치지 않는다 — `ctrl+shift+j` vs `alt+j`). slot/next/prev 필드는 모두 필드별 `#[serde(default = "…")]` 를 가져 신규 필드가 없는 구버전 config 를 읽어도 빈 값이 아니라 위 기본값으로 복원된다(`category_switch_modifier` 도 동일 — `"ctrl+shift"` default). 자유 콤보용 `next_tab`/`prev_tab` 필드와는 별개다(Command Palette·더블탭 경로 전용, quick-switch 가 건드리지 않음).

`explorer_drag_flip_modifier`(`String`, 기본 macOS `"option"`·다른 OS `"ctrl"`)도 콤보 필드가 아닌 modifier 조합이다. explorer 드래그 중 이 조합을 누르고 있으면 놓기 동작(같은 디스크 이동, 다른 디스크 복사)을 뒤집는다. 눌린 modifier 가 이 조합을 모두 포함하면 누른 것으로 본다. 필드가 없는 config 는 기본값으로 읽는다(`#[serde(default)]`). 설정 UI 는 General 서브탭 끝, 엔트리 아래 구분선 뒤의 한 행이다. 라벨 · modifier Select(`field-width-md`) · caption 모양이고, 선택지는 이 OS 의 단일 modifier 를 기본값 먼저 둔 목록이다(macOS Option · Command · Control · Shift, 다른 OS Ctrl · Alt · Win/Super · Shift, `explorer_drag_flip_modifier_options`). config.toml 에 목록 밖의 조합(`alt+shift` 등)이 있으면 목록 끝에 더해 그대로 보이고 유지된다. 라벨 열 폭에도 이 행 라벨이 들어간다. 단축키 가져오기 표에서는 General 그룹의 한 행이다. 다른 OS 에서 가져온 `option` 은 그 OS 의 같은 위치 키(macOS Option · Windows Win · Linux Super)로 동작한다. 동작은 [Explorer 파일 작업](../../surfaces/explorer/file-operations.md#드래그-앤-드롭)에 있다.

#### quick-switch 섹션 UI (Tab/Workspace 서브탭)

Tab 서브탭(탭 축)과 Workspace 서브탭(워크스페이스 축 + 카테고리 축)의 일반 콤보 목록 아래에 **quick-switch 섹션**이 있다(`keybindings_tab/quick_switch.rs`). 구성:

1. **modifier 드롭다운** — 공용 Select(트리거 `select-height` 28, 폭 `field-width-md`, caption 없음 — 드래그 반전 행과 같은 모양)로 해당 축 modifier 를 고른다. 선택지는 `tasty_settings::keybindings::quick_switch_modifier_options` 가 만든다: **OS-aware 허용 조합 리스트**(`parse::all_modifier_combos`, option 축을 포함한 15개 — option 은 macOS Option · Windows Win · Linux Super), 설정 파일에만 있는 목록 밖 값(있으면 그대로 덧붙여 보이고 다른 항목을 고르기 전까지 유지), 마지막에 **"개별 지정" sentinel 옵션**. 표시는 `format_display`(`"ctrl+shift"` → `Ctrl+Shift`), "개별 지정"은 별도 번역 라벨이다. 현재 값은 항상 목록에 있으므로 첫 옵션을 골라도 선택이 바뀐다. 값이 실제로 바뀐 경우에만 슬롯을 이관·복원하며(규칙 → 개별 지정은 합성 조합 저장, 개별 지정 → 규칙은 그 축 기본 슬롯 복원, 규칙끼리는 raw 키 유지), 같은 값을 다시 고르면 슬롯을 건드리지 않는다. 시험: `quick_switch.rs` 의 `select_row` 시험(헤드리스 egui 클릭).
2. **슬롯 1~N 버튼** — 탭 1~10번 / 워크스페이스 1~9번 / 카테고리 1~10번. 규칙 기반 축은 저장된 raw 키를 현재 modifier 조합과 **표시 시점에 합성**한 `"{Modifier}+{Key}"`(예: `Ctrl+Shift+1`) 라벨을 보여주고, 개별 지정 축은 슬롯 필드에 이미 저장된 **완전 콤보**를 그대로 표시한다.
3. **다음/이전 버튼 2개** — `*_next_key`/`*_prev_key`(세 축 모두).

버튼을 누르면 그 축이 규칙 기반이면 **bare-key 녹화**(`capture_bare_key` — modifier 가 하나라도 눌리면 무효, modifier 없는 순수 키 하나만 유효), 개별 지정이면 **일반 콤보 녹화**(`capture_winit_key_combo` — 일반 액션과 동일하게 modifier 포함 자유 조합)로 진입한다. Escape 는 두 경우 모두 슬롯을 비운다. 캡처 분기는 `RecordingSlot.field_kind`(`Combo`/`BareKey(BareTarget)`/`IndividualSlot(BareTarget)`)로 결정되고, `SettingsUiState::recording_is_bare_key()`(`FieldKind::BareKey(_)` 만 매치)가 winit 이벤트 캡처 경로를 가른다(`view/settings.rs`) — `IndividualSlot` 은 이 패턴에 안 걸려 자동으로 `capture_winit_key_combo` 경로를 탄다.

**충돌 검사**는 최종 콤보(규칙 기반은 `"{modifier}+{key}"` 합성값, 개별 지정은 저장값 그대로) 기준으로 두 가지를 본다: ① 일반 액션과의 충돌(`find_conflict` — `next_tab`/`prev_tab` 포함), ② 다른 quick-switch 슬롯과의 중복(슬롯 배열 자체 순회, 탭↔워크스페이스↔카테고리 교차 포함, 개별 지정 축도 동일하게 포함). 두 축이 같은 조합을 갖는 상태는 애초에 저장되지 않는다(`switch_target_for` 에는 우선순위 로직이 없다). 충돌 시 기존 확인 팝업(`PendingBinding`)을 재사용하며, accept 시 상대가 일반 필드면 그 바인딩을, 다른 슬롯이면 그 슬롯을 비운다. 또한 modifier 변경 등으로 현재 슬롯 콤보가 일반 액션과 겹치면 섹션 하단에 경고 라벨을 표시해 조용히 넘기지 않는다.

#### "개별 지정" 모드

modifier 드롭다운에서 **"개별 지정"**(sentinel `KeybindingSettings::INDIVIDUAL_SWITCH_MODIFIER = "individual"`)을 고르면 그 축은 규칙 기반(modifier + raw 키 1개)을 벗어나, 슬롯마다 완전히 독립된 콤보(모디파이어 포함 자유 조합)를 일반 액션처럼 녹화한다. 슬롯 필드의 "의미"가 modifier 값에 따라 갈리는 암묵적 불변식이다 — 규칙 기반이면 raw 키 하나, 개별 지정이면 이미 완성된 콤보 문자열(예: `"ctrl+alt+1"`). sentinel 문자열은 4축 조합 파서(`Combo::parse_modifiers`)가 인식하는 `ctrl`/`shift`/`alt`/`option` 토큰 중 어느 것과도 안 맞아 파싱 실패(`None`)하므로, 파서 수정 없이 "이 축은 규칙 기반이 아니다"를 안전하게 표현한다.

- **모드 전환 시 슬롯 이관/복원** — 규칙 기반 → 개별 지정으로 바꾸면 각 슬롯의 현재 합성 콤보(`구 modifier + raw`)가 그대로 슬롯 필드에 저장돼(`apply_modifier_transition`) 전환 직후 현재 키 조합을 유지한다. 역방향(개별 지정 → 규칙 기반)은 개별 지정 콤보 문자열이 raw 로 역산 불가능(구조적 정보 유실)하므로 이 축을 기본값으로 복원한다(`reset_tab_switch_to_defaults` 등, `crud.rs`).
- **디스패치** — 개별 지정 축은 `switch_target_for` 가 그 축을 절대 반환하지 않으므로(sentinel 파싱 실패) 규칙 기반과 다른 별도 경로를 탄다. `input/shortcuts/numeric.rs` 가 축별 modifier 를 직접 확인해 개별 지정이면 그 축의 다음/이전 raw 값(=완전 콤보)과 슬롯 배열을 `matches_binding` 으로 순회 매칭한다(next/prev 우선순위는 규칙 기반과 동일).
- **switch-number 오버레이 소멸은 의도된 부작용** — 개별 지정 축은 `switch_target_for` 가 그 축을 절대 반환하지 않으므로 탭바/사이드바 숫자 키캡 오버레이가 자동으로 안 뜬다. 슬롯마다 콤보가 달라 통일된 숫자 힌트를 그릴 근거가 없기 때문.
- 프리셋(Tasty/Mac/Windows/Linux)은 항상 규칙 기반 값만 가지며 개별 지정으로 바뀌지 않는다.

#### dispatch — 키 입력 → 전환

실제 키 소비는 `input/shortcuts/numeric.rs`(`handle_numeric_switch_shortcuts`)가 담당한다.
**개별 지정 축**은 규칙 기반 판정보다 먼저 검사한다(세 축 각각 modifier == sentinel 이면 그 축의 next/prev·슬롯을 `matches_binding` 으로 직접 매칭 — 위 "개별 지정 모드" 참조, 카테고리 축은 folders 게이트도 함께 적용).
규칙 기반 축은 `Key::Character` 이면(슬롯 키가 `"q"` 같은 문자일 수 있으므로 숫자 여부를 따지지 않는다) 대상(Tab/Workspace/Category)을 switch-number 오버레이와 **단일 소스**인 `switch_target_for(kb, ctrl, shift, alt, option)` 로 판정한다 — 세 축 각각의 modifier 조합(`Combo::parse_modifiers`)과 현재 눌린 조합이 **정확히 일치**할 때만 그 대상을 선택한다(단일 토큰은 조합의 부분집합이라 그대로 동작, `ctrl` 단독 ≠ `ctrl+shift`).
정확 일치라 다른 대상의 조합과 섞이지 않고 우선순위 로직이 없다.
`alt` 는 `"alt"` 토큰(macOS 물리 ⌘=super, 그 외 Alt), `option` 은 `"option"` 토큰(macOS 물리 ⌥, 그 외 Win·Super)으로 플랫폼 정규화된 값을 받는다(`tasty_key_match::token_axes`).
대상이 잡히면 **next/prev 키를 먼저**(커스텀 슬롯 키가 next/prev 키와 겹칠 때 next/prev 우선), 그 다음 슬롯 배열을 `position` 검색한다.
매칭 결과:

- 탭: next/prev → `next_tab_in_pane`/`prev_tab_in_pane`, 슬롯 index → `goto_tab_in_pane(index)`.
- 워크스페이스: next/prev → `next_workspace_in_active_category`/`prev_workspace_in_active_category`, 슬롯 local → 카테고리 토글 on 이면 `switch_workspace_in_active_category(local)`, off 면 `switch_workspace(local)`.
- 카테고리: next/prev → `next_category`/`prev_category`(`state/workspace.rs` — 카테고리 리스트 안에서 wrap-around ±1 이동한 뒤 `switch_to_category` 재사용, auto-expand + last-active 이동 포함), 슬롯 section → `switch_to_category(section)`. 세 매칭 모두 folders 토글 off 면 no-op·비소비. → [워크스페이스 카테고리](../workspace-category/index.md).

어느 것도 매칭 안 되면 조용히 `false` 를 돌려 다른 단축키 매칭으로 넘긴다. 이 메서드들은 focused pane 의 active_tab · active_workspace(= **사용자 포커스 상태**)를 바꾸므로 **사용자 키 입력 경로에서만** 호출되며 release IPC/CLI 로 노출되지 않는다([포커스 정책](../../design/policies/focus.md)). Command Palette·더블탭 파리티는 범위 밖.

#### switch-number 오버레이 — 표시 = 동작

modifier 홀드 중 탭바(`tab_bar/tab.rs`)·사이드바(`sidebar/view.rs`)에 뜨는 숫자 키캡은 고정 상수가 아니라 **설정된 슬롯 키**를 그린다(`switch_overlay::tab_digit(kb, index)`/`workspace_digit(kb, local_idx)`). 슬롯을 `"q"` 로 바꾸면 키캡도 `Q` 로 뜬다(눌러서 가는 곳 = 표시). 워크스페이스 사이드바는 카테고리 토글 on 시 **active 카테고리 내 로컬 인덱스**로 키캡을 매기고 **비활성 카테고리 행에는 키캡을 표시하지 않는다** — 슬롯 단축키가 active 카테고리 로컬 순서로 전환하기 때문(전역 인덱스로 표시하던 과거 불일치를 제거). 오버레이 modifier 상태는 egui raw_input(실제 사용자 키)만 반영하므로 IPC/에이전트로는 강제 표시할 수 없다. **개별 지정 축은 오버레이 대상에서 자동 제외**된다(위 "개별 지정 모드" 참조).

### 이식 번들 — 구성 전량을 파일 한 장으로

한 tasty 환경의 단축키 구성 **전량**을 파일 하나로 내보내고 다른 환경에서 읽어 들이는 코덱이 있다(`crates/tasty-host-plugin/src/keybinding_bundle.rs`). 옮기는 대상은 두 파일에 흩어져 있다 — `config.toml` 의 `[keybindings]`(`KeybindingSettings` 전량)와 `plugins.toml` 의 `keybindings`(plugin command override). 그래서 번들 타입은 두 타입이 모두 보이는 `tasty-host-plugin` 에 있다.

포맷은 최상위에 **스키마 태그와 버전**이 붙은 TOML 이다.

```toml
schema = "tasty.keybindings"
version = 1

[keybindings]
new_tab = ["alt+t", "ctrl+alt+t"]
tab_switch_modifier = "individual"
# … `KeybindingSettings` 의 필드 전량

[[keybindings.script_bindings]]
script_id = "s1"
combo = "ctrl+f9"

[plugin_keybindings."com.example.explorer"."explorer.refresh"]
mode = "key"
value = ["F6"]
```

번들은 `KeybindingSettings`의 모든 필드와 `PluginsConfig::shortcut_overrides`를 담는다.
화면의 command snapshot에는 비활성·미등록 plugin 설정이 빠질 수 있어 export 원본으로 쓰지 않는다.
코덱은 두 타입을 함께 아는 `tasty-host-plugin::keybinding_bundle`에 있다.

| 입력 상태 | decode 결과 |
|---|---|
| 잘못된 TOML 또는 schema 없음·불일치 | Toml / NotABundle 오류 |
| 모르는 키·필드, 잘못된 필드 모양, 읽을 수 없는 override, 더 높은 버전 | 읽을 수 있는 값을 복원하고 BundleWarning 반환 |
| 미설치 plugin 또는 없는 스크립트 | 해당 항목을 버리고 경고 |

설치 목록과 스크립트 목록은 DecodeEnv로 받는다. known_script_ids가 None이면 스크립트 존재 검사를 생략한다.
keybindings는 기본값에서 필드를 하나씩 복원한다. 필드 하나의 역직렬화가 실패하면 그 필드만 기본값으로 남긴다.
필드 이름과 배열 길이는 설정 타입에서 얻으며 코덱에 같은 목록을 따로 쓰지 않는다.

저장된 문자열의 문법은 `tasty_settings::keybindings::parse`가 소유한다.
parse_binding·ParsedBinding·Combo를 실제 키 매칭과 도움말·번들 변환이 함께 쓴다.
OS별 실제 키 대응은 매칭 계층에 남는다. 일반 바인딩 파서는 shift+shift 같은 modifier 더블탭 표기를 거절하며
더블탭은 전용 처리에서 해석한다. macOS의 `alt+alt`는 Command 두 번이고 Option 두 번은 감지하지 않는다([키 매핑 정책](../../design/policies/key-mapping.md#더블탭-키-대응)). 더블탭을 실행하는 동작의 목록은 실행과 설정 녹화가 함께 쓰며, 목록 밖의 칸은 더블탭 녹화를 거절하고 이미 저장된 값은 경고로 보여 준다.

TOML에는 null이 없다. 평범한 Option 필드는 생략으로 표현할 수 있지만,
Option 시퀀스·튜플 원소와 중첩 Option의 Some(None)은 표현을 다시 설계해야 한다.
맵의 None 값도 키가 사라져 같은 맵으로 복원되지 않는다.
새 최상위 번들 필드는 BUNDLE_KEYS와 함께 수정하고 비호환 형식 변경은 BUNDLE_VERSION을 올린다.

`option` 바인딩은 모든 OS 에서 같은 위치의 키로 동작한다([키 매핑](../../design/policies/key-mapping.md#이식-시-option-처리)).
선택 이유는 [단축키 설정 결정](../../design/policies/key-mapping.md)에 있다.

### webview surface(markdown/html)에서의 단축키 — native 자식 창에서 host 로 포워딩

`rendering = "webview"` kind(`markdown`·`html`)는 host 의 wgpu 표면이 아니라 **OS 자식 창/뷰**
위에 그려진다(X11 child window + WebKitGTK / WKWebView subview / child HWND + WebView2).
그 자식이 키보드 입력을 받으면 winit 최상위 창은 `WindowEvent::KeyboardInput` 을 아예 받지
못하므로, 아무 조치가 없으면 그 상태에서 사용자가 설정한 단축키가 동작하지 않는다. 그래서 세
백엔드가 native 키를 가로채 host 로 올린다([WebView 호스트 계약](../../design/systems/webview.md)).

- **계약은 한 곳**: 백엔드는 자기 native 키 표현(GDK keyval / NSEvent
  `charactersIgnoringModifiers` / Win32 VK)을 winit `Key`+`ModifiersState` 로 정규화해
  `WebViewKeyEvent` 로 만들고 창마다 하나뿐인 `WebViewKeyBridge` 에 올린다
  (`src/host_api/webview/keys.rs`). "host 가 가져가는가" 판정은 그 브리지에서만 하고,
  판정 자체는 콜백 안에서 **동기**라 백엔드가 페이지 전파를 그 자리에서 막는다. 실제 액션은
  host 가 다음 프레임에 큐를 비우며 실행하므로 페이지와 host 가 같은 키를 이중 처리하지 않는다.
  큐를 비울 때 settings·plugin 창 같은 modal 창, keyboard overlay, 전체화면 무대가 열려 있으면
  plugin 명령·공통 새로고침·host 단축키 어느 것도 실행하지 않는다(winit 경로의 modal 입력 차단과 같다).
- **가져갈 콤보는 `KeybindingSettings` + plugin 명령 레지스트리에서 전량 도출**한다 —
  고정 액션 필드 + quick-switch 3 축 합성 콤보 + 스크립트 바인딩 + **활성** plugin
  명령의 effective binding(매니페스트 `default_keybinding` / 사용자 override / host 액션
  상속). 포워딩 계층에 키 리터럴은 없다. 도출은 단축키 계층
  (`src/adapters/ui/input/shortcuts/webview_claims.rs`)이 하고, 브리지의 정책은 그 결과인
  콤보 목록만 받는다([WebView 호스트 계약](../../design/systems/webview.md)).
- **plugin 바인딩은 scope 로 미리 거르지 않되, 비활성 plugin 은 제외한다** — 스냅샷은
  활성 plugin 의 모든 명령을 담는 상위집합이다(키가 host 에 도착하는 시점의 모델 포커스를
  브리지가 claim 시점에는 알 수 없다). 비활성 plugin 명령은 실행할 수 없으므로 claim
  하면 키가 페이지에도 host 에도 안 가고 사라진다 — `is_disabled` 로 거른다. 도달 이후의
  우선순위 판정은 winit 경로와 같은 `dispatch_plugin_shortcut_key` 가 그대로 한다 — 모델
  포커스가 어떤 plugin surface 에 있으면 그 plugin 의 명령만 후보다([key-mapping.md](../../design/policies/key-mapping.md)).
- **페이지 예약 콤보 대조는 콤보 동등성으로 한다** — plugin 매니페스트는 raw 문자열
  (`"Ctrl+F"`)이라 사용자 설정(`"ctrl+f"`)과 대소문자·modifier 순서가 다를 수 있어, 실제
  매칭과 같은 파싱 경로(`shortcuts::bindings_equivalent`)로 정규화해 비교한다. 원시 문자열
  비교면 표기만 다른 예약 콤보가 필터를 뚫어 페이지의 find/copy가 동작하지 않는다.
- **스냅샷은 매 프레임 만들지 않는다** — `sync_webviews` 가 `KeybindingSettings` 값과 두
  epoch(`PluginCommandRegistry::revision()` / `PluginsConfig::shortcut_revision()`)를 직전
  스냅샷과 비교해 달라진 프레임에만 재생성한다. epoch 는 프로세스 전역 단조 증가
  `AtomicU64` 라, plugin 재적재로 레지스트리가 통째로 교체돼도 값이 되돌아가지 않는다.
- **비라틴 레이아웃은 physical key 폴백으로 맞춘다** — 백엔드가 하드웨어 키코드를 함께
  올리고(X11 `hardware_keycode − 8` / Win32 확장 스캔코드 / macOS `kVK_*`, 셋 다 winit
  `PhysicalKeyExtScancode::from_scancode` 표현), 브리지가 winit 경로와 **같은 함수**
  (`shortcuts::physical_key_to_logical`)로 치환한다. 치환은 `ctrl`/`super`/`alt` 중 하나
  이상이 눌린 경우에만 — 수식 없는 키는 페이지 타이핑·IME 조합 그대로다.
- **페이지에 남기는 것** 세 가지: ① 어떤 바인딩에도 없는 키. 웹 화면에서 동작하는 host·plugin 바인딩은 수식키
  유무와 관계없이 host 가 가져간다(예: `rename_tab` 의 `F2`, [공통 새로고침](#공통-새로고침--refresh)의 `F5`).
  설정 캡처가 수식키 없는 글자 키를 받지 않으므로 기본 상태의 타이핑·IME 조합은 페이지에 남는다.
  ② 페이지 예약 액션 `find`·`copy`·`cut`·`paste`·`select_all`·`page_undo`·`page_redo` — host 액션으로 가져가지 않는다.
  `find` 는 페이지가 자체로 구현한다(markdown 의 문서 내 검색 등). 편집 액션의 바인딩은 키를 받은 WebView 의 엔진
  편집 명령으로 바로 실행하고 키를 넘기지 않는다. 엔진이 같은 키(Ctrl+C 등)를 처리해도 Tasty 가 실행한다. 예외로
  Windows 의 실행 취소·다시 실행 엔진 키(Ctrl+Z·Ctrl+Shift+Z·Ctrl+Y)는 엔진과 페이지에 둔다
  ([WebView 편집 액션](../../design/systems/webview.md#편집-액션)). markdown 문서 내 검색은 사용자의 `find` 바인딩을
  `settings.get_keybinding` 으로 읽어 문서 스크립트에 넣으므로 단축키 탭에서 바꾼 조합을 따른다
  ([markdown 문서 내 검색](../../surfaces/markdown/screens/viewer.md#문서-내-검색find-in-page)).
  `find` 에서 뺀 조합을 다른 host 동작에 주면 ①·③에 들지 않는 한 host 가 가져간다. 두 동작이
  같은 조합을 갖는 상태는 단축키 탭의 충돌 확인(`find_conflict`)을 거쳐야 저장된다.
  ③ 웹 화면 포커스에서 동작하지 않는 동작 — explorer 전용 액션 `explorer_undo`·`explorer_redo`·`explorer_back`·`explorer_forward`·`explorer_new_folder`·`explorer_new_file`·`explorer_refresh`·`explorer_go_up`·`explorer_toggle_preview`·`explorer_toggle_hidden`·`explorer_properties`와 `copy_path` 는 explorer 에 포커스가 있을 때만 동작해
  webview 위에서는 할 일이 없다. `fullscreen_stage_exit` 는 넘어온 키로 실행되지 않고, 카테고리를 끈 동안에는 카테고리 전환 축과 `toggle_categories_collapsed` 가 동작하지 않는다.
  explorer 실행 취소의 기본값 `Ctrl+Z`·`Ctrl+Shift+Z` 는 페이지에서 `page_undo`·`page_redo` 가 쓰고 `Alt+←`·`Alt+→` 는 페이지의 뒤로·앞으로 키다.
  plugin 명령은 키를 받은 surface 의 실행 범위(그 plugin 의 surface 면 그 plugin 의 명령, 아니면 Global 명령)에 드는 조합만 가져간다.
  주소창 포커스 `explorer_focus_address` 는 html surface 의 주소 띠에도 쓰이므로 ③에 넣지 않고 host 가 가져간다([html 주소 표시줄](../../surfaces/html/index.md#주소-표시줄)).
- **key-up 은 어느 백엔드도 올리지 않는다**(press 만). host 는 실려온 modifier 값만
  읽고 자기 `base.modifiers` 를 갱신하지 않으므로 modifier stuck 이 생기지 않는다.
- **auto-repeat 필터는 플랫폼이 갈린다** — macOS(`isARepeat`)·Windows(`WasKeyDown`)는
  입력 차단·편집 명령·host 선점에 해당하는 키의 반복을 다시 실행하지 않고 소비한다. 나머지는 엔진·페이지에 둔다
  ([반복 판정](../../design/systems/webview.md)). **Linux/GDK 는 걸러내지 않는다**(GTK `key-press-event` 에 repeat 플래그가 없고,
  press/release 를 직접 세는 대체 판정은 X 서버의 detectable auto-repeat 설정에 따라 정상
  press 를 삼킬 수 있다). host 단축키는 전부 edge 동작이라 반복 입력을 같은 방식으로 처리하고, winit
  터미널 경로도 repeat 를 걸러내지 않아 tasty 내 다른 경로와 일관된다.
- **모델 포커스는 클릭에만 따라간다** — 백엔드가 native 클릭/포커스 획득을 통지하면 host 가
  `focused_pane`/`focused_surface` 를 맞춘다. 키 도착은 근거로 쓰지 않는다(X11 은 포인터가
  자식 창 위이기만 해도 키를 넣으므로, 키를 근거로 삼으면 focus-follows-mouse 가 된다).
- **가림과 포커스 회수**: egui overlay 가 열릴 때(webview 를 숨기는 모드와 보인 채 두는 모드 모두), 그리고 탭·workspace 전환으로
  webview 가 활성 탭에서 빠질 때 키보드 포커스를 host 창으로 돌린다(숨김과 포커스 해제는
  세 OS 모두 별개). 다시 보일 때 자동 복원은 하지 않는다.
  회수는 **창이 활성(`base.focused` 이거나 webview 자식이 OS 포커스를 쥠)이고, 포커스가 실제로 그 webview 자식 안에 있을 때만**
  한다 — overlay 와 탭 전환은 IPC 로도 일어나므로 무조건 회수하면 tasty 가 다른 앱의 OS 키보드
  포커스를 빼앗는다([사용자 통제와 데이터 원칙](../../principles/control-and-data.md)). 시점과 한계는 [WebView 호스트 계약](../../design/systems/webview.md#포커스--회수는-조건부다)이 정본이다.
- **폴링 tick 은 Linux 에서만 세워진다.** GDK 는 winit 과 다른 X 연결로 이벤트를 받아
  루프를 깨우지 못해, 드러난 webview 가 있고 창이 활성(`host_window_has_os_focus` — `base.focused`
  이거나 webview 자식이 X 포커스를 쥠)인 동안만 16ms tick 으로 GTK 를 펌프한다. 자식이 포커스를
  쥐면 winit 은 부모 창에 Focused(false)를 보내므로 `base.focused` 만 보면 그 동안 폴링이 멈추고,
  키와 클릭이 다음 깨움까지 처리되지 않는다(Xvfb 실측: 클릭이 페이지에 반영되기까지 1.46~1.95초,
  이 판정을 쓴 뒤 0.10초). 다른 앱이 X 포커스를 가진 동안에는 폴링하지 않는다 — 창 관리자가 있으면
  클릭이 창을 활성으로 만들어 Focused(true)가 루프를 깨운다. macOS/Windows 는 native 키 콜백이 winit 과 같은 이벤트 루프에서 발생해 폴링이
  필요 없다. **이 한정은 조건부 컴파일이 아니라 런타임 arm 분기다** — `Tick::WebviewKeyPoll`
  과 `WEBVIEW_KEY_POLL_INTERVAL`(`src/app/timers.rs`)은 `#[cfg(feature = "gui")]` 로만 게이트돼
  세 OS 모두 컴파일되고 (출처: 세 OS **컴파일** 채널 — `.github/workflows/crossplatform-check-macos.yml` 의 `check-macos`, `crossplatform-check.yml` 의 `check-windows`·`check-headless`. **런타임 동작은 그 채널 밖이다** — integration·e2e 는 Linux 뿐이다), `reschedule_webview_key_poll` 의
  `let arm = needs_poll && cfg!(target_os = "linux")` 가 Linux 에서만 tick 을 세운다(non-Linux 는
  `hub.cancel` 만 탄다). `cfg!` 은 컴파일타임 상수라 접히므로 다른 두 OS 의 폴링 비용은
  실질 0 이다. `#[cfg(target_os = "linux")]` 가 실제로 걸린 곳은 GTK 펌프 호출
  (`src/app/webview_keys.rs` 의 `pump_gtk_events()`) 하나뿐이다.
- **claim 되고도 소실되는 콤보가 있다** — host 가 가져간 뒤 그 프레임의 게이트(모달/무대/
  kind, plugin 명령이면 포커스된 plugin 게이트)가 거절하면 키는 페이지에도 가지 않는다.
  포워딩이 만든 손실이 아니라 기존 우선순위 규칙이 드러난 것이다(같은 상황을 wgpu 경로에서
  재현해도 결과가 같다).
- 실행 검증은 Linux/X11 만 됐다. Windows 는 `cargo check --target x86_64-pc-windows-gnu`
  타입 검증까지, macOS 는 로컬 크로스 툴체인이 없어 컴파일 검증도 하지 못했다 — 둘 다
  실 기기 미검증이다. 스캔코드 → `PhysicalKey` 변환도 같은 경계다(Linux 만 실측, 나머지 둘은
  winit 구현 대조까지). 비라틴 레이아웃 전환의 실 기기 확인도 Linux/X11 에서만 했다.

### 오버레이가 열려 있을 때 — 단축키는 **전부** 막히고, Escape 가 푼다

설정 창 · 입력 dialog · **포커스된 host popup** · plugin popup 중 하나라도 해당하면 키는
단축키 매처에 **진입하지 않는다**(`view/main/keyboard.rs` 의 `keyboard_overlay_open`).
막히는 것은 어떤 부류가 아니라 **바인딩 테이블 전체**다 — 검색창에 글자를 치는데 `alt+w`
(기본 프리셋의 close_surface)가 탭을 닫아버리면 안 되기 때문이고, 어떤 액션이 "안전한가" 는
프리셋마다 달라 부류로 가를 수 없다. host 로 키를 포워딩하는 webview 경로도 같은 게이트를
먼저 본다.

게이트보다 **앞**에서 도는 것은 셋뿐이다: 전체화면 무대의 종료 키 · double-tap · **Escape**.

게이트에 막힌 단축키는 접수되지 않은 것이므로 그 액션의 거절 안내도 내지 않는다. 예를 들어 `enter_copy_mode`는 접수된 뒤에만 진입·거절 결과를 토스트로 알린다([복사 모드 진입 결과](../clipboard/index.md#복사-모드-진입-결과)).

Escape 는 순서대로 세 가지를 본다.

1. 설정 창이 열려 있으면 닫는다.
2. 알림 패널이 열려 있으면 닫는다.
3. **포커스된 host popup 이 있으면 푼다** — 포커스를 놓고, `close_on_outside_click` 인
   popup 만 닫는다.

Escape는 포커스된 popup 하나에만 바깥 클릭과 같은 처리를 적용한다. 포커스 없는 popup은
그대로 남긴다. 좌표가 있는 바깥 클릭과 달리 여러 popup을 대상으로 삼을 근거가 없기 때문이다.
설정 창과 알림 패널을 먼저 확인하므로 다른 popup에 포커스가 있어도 이 두 화면부터 닫힌다.

### 편집 — 녹화 + 충돌

Settings Keybindings 탭에서 키 조합을 직접 **녹화**해 할당한다. 충돌(같은 조합이 다른 액션에 이미) 시 확인 팝업으로 수락/거부. 편집은 draft 에 쌓이고 Save 시 커밋(`crud.rs`). quick-switch 슬롯의 bare-key 녹화·충돌 흐름은 위 [quick-switch 섹션 UI](#quick-switch-섹션-ui-tabworkspace-서브탭) 참조.

### 프리셋

키바인딩 **프리셋**(기본 세트 전환)을 제공한다(`keybindings/presets.rs`). 레이아웃 프리셋(`tasty-presets` crate, Workspace/Tab/Pane)과는 별개 — 이름만 비슷한 다른 시스템이다.

Settings › Keybindings › **Preset** 서브탭은 **drill-down**(content-swap) 구조다
(`src/view/settings/ui/keybindings_tab/preset.rs`, 공용 위젯
`tasty_ui_widgets::{DrillDown, ListCtrl}` — 디자인 `settings_window.jsx` `PresetSubtab`):

- **List view** — 풀폭 `ListCtrl` 프리셋 목록(Tasty / Mac / Windows / Linux). 각 행: 이름 + 한 줄 설명 + drill-in chevron. 현재 draft 와 모든 일반 바인딩이 일치하는 **사용 중 프리셋**에 trailing "Active" Tag(success·dot) + selected 하이라이트(2px accent 바).
- **Detail view** — 행 클릭 시 콘텐츠 전체 교체(0ms): back bar(← + "{이름} preset" 제목 + **우측 Apply**) 아래 Action / Current / {프리셋} 3열 diff 테이블 — 변경 행은 accent-primary 강조. ← 로 목록 복귀.
- **Apply 범위** — Apply 는 선택 프리셋을 settings **draft** 에 기록(사용 중 프리셋이면 "Applied" 비활성 — 적용할 diff 없음), footer Save 가 draft 전체를 디스크에 커밋. 두 버튼은 물리적으로 분리(back bar vs footer).
- 이 서브탭은 표준 콘텐츠 패딩/스크롤 래퍼를 우회한 **full-bleed** — DrillDown 이 자체 패딩과 내부 스크롤을 소유한다.

### 프리셋 바인딩 — 네 프리셋의 기본값

tasty 는 4개 프리셋을 제공한다. 각 프리셋은 **바인딩 문자열의 집합**이고, 그 문자열이 실제 물리 키에 매핑되는 방식은 [key-mapping](../../design/policies/key-mapping.md) 의 OS별 레이어가 결정한다. 편집은 위 [편집 — 녹화 + 충돌](#편집--녹화--충돌), 프리셋 전환 화면은 바로 위 [프리셋](#프리셋).

| 프리셋 | 참고 | 특징 |
|--------|------|------|
| **Tasty**(기본) | 자체 | 자체 키 조합 |
| **Mac** | iTerm2/Terminal.app | `alt+`(=⌘) 중심 |
| **Windows** | Windows Terminal | `ctrl+shift+` 중심 |
| **Linux** | GNOME Terminal | `ctrl+shift+` 중심 |

프리셋은 **권장**이지 플랫폼 강제가 아니다 — Mac 프리셋의 `alt+c` 는 macOS 에서 ⌘C, Windows 에서 Alt+C 로 동작한다([key-mapping](../../design/policies/key-mapping.md) 의 위치 기반 매핑).

`enter_copy_mode`의 기본값은 네 프리셋 모두 `ctrl+shift+space`다. `ctrl` 토큰이므로 macOS 에서도 Command 가 아니라 Control+Shift+Space 다.

#### 전체 바인딩 비교 (빈 칸 = 바인딩 없음)

##### 생성/닫기
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| new_window | alt+shift+n | alt+shift+n | ctrl+shift+n | ctrl+shift+n |
| new_workspace | alt+n | alt+n | alt+n | alt+n |
| new_tab | alt+t | alt+t | alt+t | alt+t |
| close_active | ctrl+w | | ctrl+w | ctrl+w |
| close_pane | ctrl+shift+w | alt+shift+w | ctrl+shift+w | ctrl+shift+w |
| close_workspace | alt+shift+w | | alt+shift+w | alt+shift+w |
| restore_closed | ctrl+shift+t | ctrl+shift+t | ctrl+shift+t | ctrl+shift+t |

##### 분할 / 포커스
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| split_pane_vertical | alt+e | alt+e | alt+shift+e | alt+shift+e |
| split_pane_horizontal | alt+shift+e | alt+shift+e | alt+shift+d | alt+shift+d |
| split_surface_vertical | alt+d | alt+d | alt+d | alt+d |
| split_surface_horizontal | alt+shift+d | alt+shift+d | alt+e | alt+e |
| focus_pane_next/prev | ctrl+] / ctrl+[ | (동일) | (동일) | (동일) |
| focus_surface_next/prev | alt+] / alt+[ | (동일) | (동일) | (동일) |
| tab_switch_modifier / workspace_switch_modifier | ctrl / alt | (동일) | (동일) | (동일) |
| category_switch_modifier | ctrl+shift | (동일) | (동일) | (동일) |

> Pane/Surface 포커스는 tasty 고유라 전 프리셋 동일(분할은 위 표대로 프리셋마다 다르다). `next_tab`/`prev_tab` 은 `ctrl+tab`/`alt+tab` 이 OS 단축키와 충돌해 기본값 없음.

##### 클립보드 / 줌
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| copy | ctrl+c, alt+c, ctrl+shift+c | alt+c | ctrl+c | ctrl+shift+c |
| paste | ctrl+v, alt+v, ctrl+shift+v | alt+v | ctrl+v | ctrl+shift+v |
| copy_link | (없음) | (없음) | (없음) | (없음) |
| zoom_in/out/reset | ctrl/alt 계열 다중 | alt 계열 | ctrl 계열 | ctrl 계열 |

> Tasty 프리셋 copy 는 세 관례를 다 바인딩 — `ctrl+c` 는 선택 있으면 복사, 없으면 SIGINT([clipboard](../clipboard/index.md)).
>
> `copy_link` 는 마우스 포인터 아래 터미널 링크를 복사한다([terminal-link](../terminal-link/index.md)). 모든 프리셋에서 비어 있다. 대상이 포인터 위치라 명령 팔레트에서는 제외한다.

##### UI 토글 / 종료 / 변환
| 필드 | Tasty | Mac | Windows | Linux |
|------|-------|-----|---------|-------|
| toggle_settings | ctrl+, | alt+, | ctrl+, | ctrl+, |
| toggle_notifications | ctrl+shift+i | alt+shift+i | ctrl+shift+i | ctrl+shift+i |
| toggle_dag_list | ctrl+shift+g | alt+shift+g | ctrl+shift+g | ctrl+shift+g |
| toggle_sidebar / _collapse | ctrl+shift+b / ctrl+b | alt+shift+b / alt+b | (ctrl 계열) | (ctrl 계열) |
| fullscreen_stage_exit | escape | escape | escape | escape |
| code_area_apply | ctrl+enter, alt+enter | alt+enter | ctrl+enter | ctrl+enter |
| code_area_cancel | escape | escape | escape | escape |
| quit | | alt+q | | ctrl+q |
| quit_minimize | | alt+m | | |
| convert_surface | alt+' | alt+' | alt+' | alt+' |

> `fullscreen_stage_exit`(전체화면 무대 종료)은 4 프리셋 공통 `escape` 다 — 무대는
> 플랫폼 관습이 갈리는 영역이 아니고 "덮은 것을 ESC 로 걷는다" 는 관습이 세 OS 에 공통이다.
> modifier 가 없지만 이 바인딩은 **무대가 올라와 있을 때만** 조회되므로 평상시 ESC 동작
> (settings 모달·notifications 팝업 닫기, 터미널 `\x1b` 전달)을 가져가지 않는다. 설정 UI 의
> 녹화 버튼에서는 ESC 가 "슬롯 비우기" 로 예약돼 있어 ESC 를 **다시 지정할 수는 없다** —
> 기본값으로 되돌리려면 프리셋을 재적용한다([key-mapping](../../design/policies/key-mapping.md#바인딩-문자열-문법)).

> `code_area_apply` 의 Tasty 프리셋 두 조합은 어느 OS 에서나 이전 고정 키(egui Mod+Enter — macOS Command, 그 밖 Ctrl)를 포함하려는 것이다. `alt+enter` 는 macOS 에서 Command+Enter 이고 Windows·Linux 에서는 Alt+Enter 로 하나 더 늘어난다. 이 필드가 없는 기존 설정 파일은 다른 누락 필드처럼 Tasty 프리셋 값을 받는다.

> `quit_immediate` 는 실수 방지로 전 프리셋 기본값 없음. `apply_*_preset`(레이아웃 프리셋 적용)도 사용자가 직접 배정하도록 기본값 없음. Windows 는 Alt+F4 가 OS 종료라 `quit` 불요.

> 이 표는 호스트 `KeybindingSettings` 프리셋 필드만 다룬다. 클립보드 뷰어(`toggle_clipboard_viewer`)·git
> viewer 등 플러그인 커맨드의 단축키는 각 플러그인 매니페스트의 `[[contributes.commands]]`
> `default_keybinding`으로 선언되며 프리셋과 무관하다 — [plugin-development](../../dev-guide/plugin-development.md#단축키-commands) 참조.

#### 설계 원칙

1. **수정자 계층** — Mac: `alt`(⌘)=주요 동작 / `alt+shift`(⌘⇧)=보조 / `ctrl`(⌃)=pane 포커스. Windows·Linux: `ctrl+shift`=주요 / `ctrl`=보조/줌 / `alt`=분할·surface 포커스.
2. **tasty 고유 기능**(Pane/Surface 분할·Workspace·변환)은 각 프리셋 수정자 계층에 맞추되 가능하면 문자 키 유지(`d`=분할, `e`=pane 분할).

### 가져오기 / 내보내기

Settings › Keybindings › **Import / Export** 는 위 [이식 번들](#이식-번들--구성-전량을-파일-한-장으로)을 파일로 쓰고 읽는 화면이다(`src/view/settings/ui/keybindings_tab/import_export.rs`, 행 모델 `import_export/model.rs` — 디자인 `kb_import_export.jsx`). L2 목록 **맨 끝**에 있고 위에 separator 가 붙는다(필터 검색 중에는 separator 를 숨긴다). Preset 과 같은 **full-bleed drill-down** 이다.

- **진입 화면** — 안내문 + 행 둘: **Export**(secondary) · **Import**(primary).
- **내보내기** — 설정 창의 로컬 파일 선택([native-file-picker](../native-file-picker/index.md#설정-창에서의-로컬-전용-재사용))을 저장 모드로 연다.
  기본 파일명 `tasty-keybindings-<YYYY-MM-DD>.toml`, `.toml` 필터.
  내보내는 원본은 **현재 draft**(저장 전 편집 포함)와 `plugins.toml` 의 override 전량(`PluginsConfig::shortcut_overrides`)이다.
  소유한 bundle text/path를 공용 file writer에 명령으로 제출한다. 파일 I/O를 기다리는 동안 **Export…** 버튼은 라벨과 폭을 유지한 채 비활성화되고, 버튼 왼쪽에 Spinner와 **Exporting…** 문구를 표시한다. 나머지 UI는 계속 동작한다. 사용자 Export 시도는 snapshot 병합 대상이 아니다.
  알려진 쓰기 성공 receipt를 받은 뒤 원래 설정 윈도우 자체 toast(Success)가 경로를 보여 준다. 닫기·Cancel·재열기·bundle draft 편집 뒤에는 예전 완료를 표시하지 않으며, 이미 접수한 쓰기는 완료한다.
  쓰기에 실패하면 **Export 행 안**에 danger 알림 블록(경로 · 이유 · "Nothing was written.")이 뜨고 **Try again**(같은 경로로 다시 쓰기) · **Choose another location…**(블록을 닫고 저장 선택기를 다시 연다)을 싣는다.
  블록이 떠 있는 동안 행의 **Export…** 버튼은 꺼진다 — 재시도 입구가 둘이 되지 않게.
  실패는 toast 가 아니다: toast 는 저절로 사라지고 재시도를 못 싣는다.
  이유 구절은 **고정 집합 넷**이다 — 읽기 전용 파일시스템(`ie_export_failure_read_only`) · 권한 없음(`_denied`) · 볼륨이 참(`_disk_full`) · 나머지를 접는 catch-all(`_unknown`, "the write didn't finish.").
  집합이 고정인 이유는 그 구절이 모든 내장 언어에서 같은 문장으로 읽혀야 하기 때문이고, 그래서 **OS 가 낸 문장은 문장 안에 들어가지 않는다** — catch-all 갈래에서만 본문 아래 제 줄(mono caption · muted · 한 줄 말줄임 · 전문은 tooltip)로 따라붙는다.
  "볼륨이 참" 은 `io::ErrorKind::StorageFull` 이 아직 nightly 라 raw OS 코드로 본다(`db::disk_full_os_error`, db 초기화 쪽과 같은 상수).
  알려진 실패는 공용 원자 교체의 대상 원본 보호 계약을 따른다. 결과를 확인하지 못한 경우에는 **Export result unknown**과 대상 파일을 확인한 뒤 다시 시도하라는 안내를 표시하며 “Nothing was written.”이라고 단정하지 않는다. 자동 재시도하지 않고, 명시적 Try again만 현재 draft를 다시 인코딩한다.
  기본 fsync는 없고 공용 helper가 지원하는 mode/owner/native 보안 속성만 보존한다. 모든 metadata나 전원 장애 내구성을 보장하지 않는다.
  실패 로그에는 오류와 경로만 남기며 bundle 원문은 남기지 않는다.
- **가져오기** — 같은 선택기를 열기 모드로 열어 고른 파일을 `decode` 한다(`DecodeEnv` = 이 환경에 설치된 plugin id · 스크립트 레지스트리). 성공하면 detail 로 들어간다.
  - **파싱 실패**(`Toml`·`NotABundle`) — detail 에 danger 인라인 블록(파일 이름 · TOML span 에서 계산한 줄 번호)과 "다른 파일 고르기" 가 뜨고 back bar 액션은 숨는다. 줄 번호를 못 얻으면 줄 구절을 빼지 않고 "the file isn't TOML." 로 **바꾼다** — 가운데 문장은 언제나 이유를 말하고 앞뒤 문장은 두 형태에서 같다.
  - **버린 plugin override**(`DroppedUninstalledPlugin`) — 경고가 아니라 muted 정보 줄 하나(개수 · plugin id 목록).
  - **번들 경고** — 경고 톤 알림 블록 **하나**에 한 줄씩 쌓고 헤더에 개수를 단다.
    개수가 **하나면 단수형**이다 — 헤더(`ie_notices_count_one`)도, 모르는 액션 줄(`ie_notice_unknown_actions_one`)도.
    단수형은 en 의 단수 변형이고, ko·ja·zh-Hans 는 기준 키 하나로 쓴다([i18n 가이드](../../dev-guide/i18n.md)의 단수 규칙).
    순서는 원문 순서가 아니라 고정이다: 새 스키마(`NewerVersion`) → 모르는 액션(`UnknownKeybindingField`, 몇 개든 한 줄에 이름 목록) → OS 예약 조합.
    OS 예약 조합 줄은 코덱 경고가 아니라 가져온 본체 바인딩과 plugin Custom 키를 [OS 가 가로채는 조합](../../design/policies/key-mapping.md#os-가-가로채는-조합) 목록과 대조해 만든다(`bundle_notices::os_reserved_notice`). 몇 개든 한 줄이고("Windows may use Win+L, Win+Tab itself, so Tasty might not receive it."), 목록이 없는 macOS 에서는 나오지 않는다. 적용을 막지 않는다.
    세 줄까지 보이고 나머지는 **Show {n} more** 뒤로 접힌다(`import_export/bundle_notices.rs`).
    줄이 하나면 접을 것이 없어 그 링크를 표시하지 않는다.
    정보 줄은 이 블록에 들어가지 않는다 — 정렬 키가 주제가 아니라 톤이다.
    화면 문구가 정해지지 않은 경고(`UnknownTopLevelKey` · `KeybindingFieldShapeMismatch` · `KeybindingsUnreadable` · `UnreadablePluginOverride` · `PluginOverridesUnreadable` · `DroppedUnknownScriptBinding`)는 로그(`tracing::warn!`)로만 남는다.
    "빈 그룹" 줄은 디자인 문구가 있지만 그것을 내는 경고가 코덱에 없다.
- **diff 표** — 선두 **선택 열** + Action / Current / Imported. 행은 네 그룹이고 그룹마다 **헤더 행**(그룹 전체 선택 · 접기 chevron · 그룹명 · `N changed · M total`)이 붙는다.
  - **General** — 일반 콤보 필드 하나가 한 행(콤보 목록 전체). 드래그 반전 modifier 도 이 그룹의 한 행이다.
  - **Quick switch** — 축(tab · workspace · category) 하나가 한 행. modifier · 슬롯 전부 · 다음/이전을 함께 옮긴다 — 슬롯은 raw 키라 modifier 와 떨어지면 뜻이 바뀐다.
  - **Scripts** — 현재와 번들의 **합집합**. 번들에 없는 현재 바인딩은 적용하면 사라지는 행으로 보인다.
  - **Plugin overrides** — **번들에 있는 명령만**. 이 환경에만 있는 override 는 표에 오르지 않고 바뀌지도 않는다.
  - 기본은 변경된 행만 보인다(back bar 의 **Show all {n}** / **Changed only** 토글). 선택은 해제한 행을 기억하는 방식이라 처음에는 모든 행이 선택돼 있다 — 변경 없는 행은 적용해도 값이 같다.
- **Apply** — 고른 행만 settings draft 와 `plugin_shortcuts_draft` 에 쓴다(`apply_rows`). 적용되면 toast 로 알린다. 디스크 커밋은 footer **Save** 가 한다(Preset 과 같은 2 단계).
- **Cancel** — 설정 draft 와 함께 `plugin_shortcuts_draft` 도 버린다. plugin draft 는 **Save 로 닫혔을 때만** 적용된다 — 창 닫기·`toggle_settings` 로 닫혀도 버린다. Plugins 서브탭 편집도 같은 규칙이다.

결정의 근거·대안·재검토 조건은 [키 매핑 설계 (운영 상세)](../../design/policies/key-mapping.md).

### Plugins 서브탭

플러그인 명령의 단축키 override 를 편집한다. 화면은 공용 view `tasty_ui_widgets::kb_plugins_subtab` 이 그리고,
본체 wrapper `draw_plugins_subtab`(`src/view/settings/ui/keybindings_tab/plugins.rs`)은 초안과 저장된 override 를
표시값으로 풀어 넘긴 뒤 바뀐 것만 `plugin_shortcuts_draft` 에 쓴다. 디스크 반영은 위 **Cancel** 규칙과 같다.

- **플러그인 선택** — 이름순 Select. 단축키를 등록한 플러그인이 없으면 "No plugins have registered shortcuts." 한 줄만 보인다.
- **명령 한 줄** — 제목(제목 열 안에서 줄바꿈) · 방식 Select(Inherit / Custom / None) · slot · **Reset**.
  폭이 모자라면 한 흐름으로 줄을 바꾼다. 다음 줄은 방식 Select 의 x 에서 시작하고 **Reset** 은 늘 마지막이다. 키가 많으면 녹화 슬롯이 늘어나 **Reset** 을 오른쪽으로 민다. 키는 모두 보인다.
  - Inherit: slot 은 상속할 호스트 동작 Select(`INHERITABLE_HOST_ACTIONS`). 줄 아래에 그 동작의 현재 키("Inherited (…)", 키가 없으면 "None").
  - Custom: slot 은 다른 서브탭과 같은 녹화 슬롯(`tasty_ui_widgets::kb_record_slot`)이다. 키마다 슬롯 하나를 두고 뒤에 추가(+) 슬롯을 둔다.
    키가 없으면 "None" 슬롯 하나만 둔다. 슬롯을 누르면 "Press key combination..." 으로 바뀌고, 누른 조합이 그 슬롯을 바꾸거나(기존 슬롯) 뒤에 붙는다(추가 슬롯).
    녹화 중 `Esc` 는 기존 슬롯이면 그 키를 지우고 새 슬롯이면 녹화만 취소한다. 키를 모두 지우면 `Key { value: [] }` 라 그 명령은 단축키가 없다.
    캡처는 다른 서브탭과 같은 winit 경로(`capture_winit_key_combo`, `FieldKind::Combo`)를 쓰므로 녹화 규칙도 같다. 결과는 바로 초안에 쓴다.
    녹화 슬롯 id 는 `plugin:<plugin id>/<command id>` 이고, 녹화 상태는 설정 창의 `recording_field` 하나를 다른 서브탭과 함께 쓴다.
    플러그인 · 방식을 바꾸거나 **Reset** 을 누르면 진행 중인 plugin 녹화를 취소한다. 다른 서브탭의 충돌 확인 popup 이 떠 있으면 슬롯은 disabled 다.
    플러그인 키는 본체 키보다 먼저 매칭하는 정책이라 녹화한 조합의 충돌은 확인하지 않는다.
    저장값에 `tasty_key_match::binding_key_recognized` 가 거절하는 키가 있으면(설정 파일을 직접 고친 경우 등) 그 키의 슬롯은 표시용 이름 대신 설정 원문을 오류 테두리(`kb-plugin-error-fg`)로 보이고, 줄 아래에 caption 이 붙는다. 첫 문제 키 하나만 보인다(`tasty_key_match::first_text_key_problem`).
    키에 OS 키 이름(`cmd`·`super`·`win`·`meta`)이 있으면 caption 은 적는 법 안내("Write Win, Super and Option as `option`, and Cmd as `alt`.", 저장 토큰은 code run)이고, 그 밖의 실패는 "Unrecognized key: <키>" 다. 이 표시는 경고이며 값은 그대로 둔다.
    해석 문제가 없고 [OS 가 가로채는 조합](../../design/policies/key-mapping.md#os-가-가로채는-조합) 목록에 맞는 키가 있으면, 슬롯은 그대로 두고 줄 아래에 경고 아이콘과 "Windows may use Win+L itself, so Tasty might not receive it."(Linux 는 "Your desktop may use …") caption 을 `kb-os-reserved-fg` 로 붙인다. 저장은 막지 않는다.
  - None: slot 은 "(Unassigned)".
  - 방식을 바꾸면 Custom 은 이전 Custom 값이나 매니페스트 키, Inherit 는 매니페스트 source 나 화이트리스트 첫 항목으로 시작한다.
- **초안 점** — 초안이 저장값과 다르면 제목 뒤에 점이 선다(같은 값으로 되돌리면 초안 항목이 지워져 점도 사라진다).
- **Reset** — 초안이나 저장값에 override 가 있을 때만 누를 수 있다. 저장된 override 가 있으면 초안에 지움(None)을 넣고, 없으면 초안만 지운다.

치수와 토큰은 [디자인·갤러리 대응표](../../design/systems/design-gallery-mapping.md#settings--keybindings--plugins-kbplugins) 를 따른다.

### 설정 탭 구성 (서브탭·항목 순서)

Settings 의 Keybindings 탭은 액션을 서브탭으로 묶고, 그 순서는 **유비쿼터스 언어 계층**을 따른다. 서브탭 enum: `KeybindingsSubTab`(`src/view/settings/ui/keybindings_tab.rs`).

```
General → Workspace → Pane → Tab → Surface → Clipboard → Zoom → Explorer → Scripts → Preset → Plugins ─ Import / Export
          \________ 계층 순서 ________/
```

- **General / Clipboard / Zoom / Explorer**: 계층에 속하지 않는 전역·기능별 단축키.
- **Workspace → Pane → Tab → Surface**: [구조 계층](../../concepts/hierarchy.md) 순서.
- **Scripts / Preset / Plugins**: 스크립트 바인딩 · 프리셋 적용 · 플러그인 기여 단축키.
- **Import / Export**: 구성 전량의 파일 이식(항상 맨 끝, 위에 separator). [가져오기 / 내보내기](#가져오기--내보내기).

각 서브탭 *내부* 항목 순서: **① 생성/분할 → ② 탐색(next/prev/focus) → ③ 수정(rename/convert) → ④ 닫기 → ⑤ 수식키(modifier, separator 로 구분)**.

**어느 서브탭에 두는가** — 그 동작의 *대상 엔티티* 이름을 가진 서브탭에 둔다. `new_tab`→Tab, `split_pane_*`→Pane, `close_surface`→Surface. 수식키도 대상 엔티티 서브탭(`tab_switch_modifier`→Tab, `workspace_switch_modifier`·`category_switch_modifier`→Workspace). cascade 인 `close_active` 는 가장 먼저 닫히는 대상이 탭이라 Tab. `open_markdown` 은 새 탭으로 열려 Tab.

> explorer 는 host builtin kind 라 `open_explorer`(Tab)·`convert_to_explorer`(Surface) 호스트 키바인딩이 있다. 현재 Surface 의 convert 계열은 `convert_surface`·`convert_to_markdown`·`convert_to_explorer` 다.

가져오기는 고른 행을 그대로 적용하며 충돌을 검사하지 않는다.
일부 행만 선택하면 현재 draft에 남은 바인딩과 같은 조합이 생길 수 있다.
번들과 완전히 같은 plugin override 집합으로 맞추려면 로컬에만 있는 항목은 Plugins 서브탭에서 따로 제거한다.

## 인터페이스

- **사용자**: Settings → Keybindings 탭에서 녹화/편집. (단축키는 사용자 행동이라 release IPC/CLI 로 *발동* 하지 않는다 — 키 주입은 debug 전용, [debug-ipc](../../dev-guide/debug-ipc.md).)

## 비-목표

- 각 액션이 *무엇을 하는가* — 그 도메인 동작은 해당 기능 문서. 여기선 *키 ↔ 액션 매핑* 만.
- 위치 기반 modifier 매핑 규칙 — [design/policies/key-mapping](../../design/policies/key-mapping.md).

## 관련

- [WebView 호스트 계약](../../design/systems/webview.md) — webview 자식 창의 키를 host 로 포워딩하는 결정
- [design/policies/key-mapping](../../design/policies/key-mapping.md) — modifier 매핑(바인딩 문자열 → 물리 키 OS별 매핑)·OS 메뉴 key equivalent 정책
- [settings](../settings/index.md) — 편집 표면
