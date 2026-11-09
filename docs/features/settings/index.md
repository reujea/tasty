# 설정 (Settings)

- **Status**: Implemented
- **주체**: 로컬 사용자 (원격 접속 사용자는 mirror 로 봄)
- **설계**: 없음
- **코드**: `src/view/settings.rs`, `src/view/settings/ui/`
- **화면**: [screens/settings.md](screens/settings.md)

## 목적

[사이드바](../sidebar/index.md) 설정 버튼이 여는 **설정 창**. tasty 의 환경설정을 2단계 메뉴(상단 L1 탭 + 좌측 L2 섹션)로 편집한다. `SettingsView`(모달 계열 View, [구조 계층](../../concepts/hierarchy.md))이다.

## 내부 동작

### 2-level IA — L1 7탭 × L2 섹션

상단 L1 7탭, 각 탭은 좌측에 L2 섹션 목록을 가진다 (이 순서):

- **General**
  - General(레이아웃 복원 · 카테고리 · 닫기 동작 · **언어** — 내장 `en`/`ko`/`ja` + `~/.tasty/lang/<code>/pack.toml` 언어팩을 한 콤보에 노출, 라벨은 `[meta] name` 없으면 코드, 설정값이 목록에 없으면 `<code> (not found)` 행으로 유지하고 덮어쓰지 않음, 변경은 재시작 후 반영 → [language-packs](../language-packs/index.md))
  - Notifications
  - Accessibility
  - Overlay(오버레이류 표시 설정 — 현재는 토스트 자동 소멸 시간 `Toast duration` 1~10s 1행 → [toast](../../design/systems/toast.md))
  - Remote transfer(원격 mirror 파일 전송 수신측 저장 정책 — `Save folder`(mono 경로 Input + Browse… Tasty 폴더 피커, 기본 `~/.tasty/transfers/`) + `Maximum size`(정수 mono Input + 정적 `MiB` suffix, 기본 500 MiB) 2행, `RemoteTransferSettings{dir, max_mb}` 편집 → [remote-attach](../remote-attach/index.md))
  - Remote attach(원격 attach 연결 — `Attach connections` 헤딩 아래 `Disconnect after`(정수 mono Input 오른쪽 정렬 + 정적 mono `s` suffix, 10~600, 기본 30) 1행, 범위 밖 입력은 공용 danger 문장. `RemoteAttachSettings{heartbeat_timeout_secs}` 편집. 저장한 값은 재시작 없이 새 연결부터 적용 → [remote-attach](../remote-attach/index.md#무응답-끊김-시간))
  - Display(macOS 전용 — Alt/Option/Shift 단축키 표시 스타일 드롭다운 3개. 텍스트("Alt"/"Option"/"Shift", 기본값)와 macOS 심볼("⌘"/"⌥"/"⇧") 중 독립 선택, `alt` 는 "Cmd" 텍스트도 선택 가능. 저장 포맷에는 영향 없음 → [key-mapping](../../design/policies/key-mapping.md))
  - Permissions(macOS 전용 — 권한 상태 표시, [모든 권한 요청하기](파일 폴더와 화면 기록을 한 번에 요청하며 요청 중에는 비활성), 전체 디스크 접근 권한 패널 바로가기. 권한 요청이 시작되는 유일한 화면이다 → [macos-permissions](../macos-permissions/index.md)).
- **Terminal** — L2: General(터미널 동작 설정 — 셸/셸 모드(Windows 전용)/스타트업/스크롤백/링크 수식키, macOS 빌드는 "Use Option as Meta" 토글 추가 → [terminal](../../surfaces/terminal/index.md) 키보드 입력) / Input(앱 실행 파일명별 Shift+Enter LF 전송 토글·추가·삭제, 저장 시 적용) / Mouse Capture(마우스 캡처 안내 배너 토글 + Shift 우회 Note + 캡처 비활성화 블랙리스트 에디터) / TUI(OSC 52 클립보드 읽기 허용 토글 + 바로 아래 bordered warning callout → [clipboard](../clipboard/index.md)) / Performance.
- **Appearance** — L2: Theme / Colors / General / Display / Tasty / Terminal / Explorer + 플러그인 기여 페이지(동적). Display = UI 스케일(sm/md/lg) 전용. Tasty = 앱 크롬 색상(accent / sidebar bg / active tab indicator). Explorer는 내장 파일 관리자 전용 폰트 override 섹션이다. surface 폰트 override 섹션(Terminal · Explorer · plugin `font_override`)은 행마다 라벨 · 자기 필드 폭의 컨트롤 · 뒤따르는 **Use default** 체크박스를 두는 색 override 행과 같은 형태다. 체크박스가 남은 폭에 들어가지 않으면 컨트롤 아래 줄로 내려간다. 미리보기는 창 폭과 관계없이 격자 아래에 있다. Focused · Unfocused 칸을 나란히 두되 한 칸이 200 보다 좁아지면 위아래로 쌓고, 칸마다 그 surface 의 실제 배경과 효과 글꼴로 표본 네 줄(라틴·한글·숫자·가나)을 그린다. 그 아래 글꼴 요약 한 줄을 둔다. General 의 기본 글꼴도 같은 미리보기를 글꼴 행 아래 콘텐츠 전폭에 두고, 그 뒤 구분선 아래에 합자·배경 투명도 행이 온다. HTML viewer 설정은 호스트 고정 탭이 아니라 `com.tasty.html` 플러그인이 기여하는 동적 페이지다.
- **Keybindings** — L2: General / Workspace / Pane / Tab / Surface / Clipboard / Zoom / Explorer / Scripts / Preset / Plugins / Import / Export(separator 아래). 단축키 편집 (아래).
- **FileHandler**(표시 라벨 **Handler** — 내부 enum 키는 `FileHandler` 유지) — L2: File Extension Mapping / File Detectors / File Handlers / Hook Handlers. File Extension Mapping 은 확장자마다 머리줄과 detector 순서 목록을 그린다. 머리줄 오른쪽 끝에는 사용자 순서가 있는 확장자에 **Reset**(설치 순서로 되돌림, 툴팁 유지), 켜진 detector 가 없는 확장자에 **Remove** 가 ghost Button sm 으로 온다. 미설치 확장자는 머리줄만 남고(`.ext` 흐린 글자 + disabled Tag "not installed" + 아래 구분선) detector 행이 없다. 머리줄 높이는 버튼 높이 이상이라 Reset 이 나타나도 행이 움직이지 않는다. 두 버튼 모두 초안만 바꾸며 확인 창이 없다. 누른 뒤 Save 전까지 머리줄은 남고, 누른 버튼은 같은 자리의 **Undo** 가 되며 disabled Tag 가 Save 때 할 일("reset on save" · "removed on save")을 적는다. Remove 대기 중인 `.ext` 에는 취소선이 그어진다. Reset 대기 중에도 행은 이미 설치 순서로 보이고, 그 상태에서 순서를 다시 바꾸면 대기가 끝나 새 사용자 순서가 된다. Undo 는 그 확장자 하나의 초안을 누르기 전 값으로 되돌리고, Save 는 둘을 적용하며(미설치 묶음과 Reset 버튼이 사라진다) Cancel 은 대기 전부를 되돌린다. File Detectors · File Handlers 는 표 대신 두 줄 행 목록이다. detector 행은 판정과 같은 설치 순서(같으면 id 순)로 놓이고, 추가 대기 중인 새 id 는 맨 뒤에 온다. handler 행은 실행 순서(priority 오름차순 → owner `user > plugin > host` → id)로 놓여 한 detector 의 행을 위에서부터 읽으면 실행 순서이고, 추가 · 교체 · 삭제 대기 행도 저장 뒤 실행될 자리에 선다. 설명 문구도 이 규칙을 적는다. 첫 줄은 id · 출처 Tag · (handler 는 `prio N`) · Save 대기 Tag · Switch · 끝 칸이고, 둘째 줄은 mono 한 줄 요약(detector 는 출처별 규칙을 병합 순서 Host → Plugin → User 로 `  ·  ` 로 잇고, handler 는 `<detector>  →  <action>`)이며 잘리면 툴팁에 전체 값을 둔다. detector 는 출처마다 Tag 를 달고(규칙 없이 켜기만 바꾼 사용자 기여는 "user override") 있다. 첫 줄이 좁아 id 가 말줄임될 때는 정해진 순서로 자리를 내준다 — 여러 출처 Tag 를 "N sources" Tag 하나로 접고, 그래도 넘치면 Save 대기 Tag 를 첫 줄 아래 자기 줄로 내리며(그 줄도 좁으면 말줄임 + 툴팁), 그 뒤에야 id 를 96px(id 가 더 짧으면 그 폭) 하한까지 말줄임한다. 단계마다 밟을 때의 폭을 기억했다가 열이 그 폭으로 넓어져야 되돌려 폭 경계에서 흔들리지 않는다. Switch · 끝 칸 · `prio N` 은 줄지 않는다. 끝 칸은 사용자 기여가 있으면 Remove, Save 대기 중이면 같은 자리의 Undo, 사용자 기여가 없으면 자물쇠 글리프(툴팁에 제공 출처)다. detector 의 Remove 는 **사용자 기여만** 지운다 — 다른 출처가 남는 행은 사용자 Tag 와 사용자 규칙 조각에만 취소선을 긋고 Switch 는 Save 뒤 상태로 비활성이며, 사용자 기여뿐인 행은 id 전체에 취소선을 긋는다. 머리의 "Add detector" · "Add handler" 는 설명과 목록 사이에 추가 카드를 연다(카드가 열린 동안 버튼은 비활성). 카드는 surface-raised 라 그 위의 Input · Select 는 raised 바탕 상자(bg-panel · border-strong)이고 Cancel 은 raised ghost 글자, 오류 줄은 raised 위 오류 색이다. 카드의 필드 행은 라벨 열 `settings-label-width` 와 컨트롤 열이며, 컨트롤 아래 한 줄은 오류 > 안내 > caption 순으로 하나만 보인다. Add 는 필수 값(detector: Id, handler: Short name · Detector · 동작별 필수 값)이 차면 눌리고 형식 오류는 누른 뒤에 보인다. detector Id 는 입력 그대로 저장되며(`[a-z0-9-]`, 64자 이하) 이미 등록된 id 를 적으면 오류가 아니라 사용자 기여를 더하거나 바꾸는 patch 다 — 입력하는 동안 안내 줄이 뜨고 버튼이 "Update detector" 가 된다. patch 는 사용자 기여를 통째로 바꾸므로, 행의 켜짐 상태가 다른 출처의 병합 결과와 다르면 그 상태를 새 기여에 적어 유지한다(plugin 이 끈 detector 를 사용자가 켜 둔 경우 포함). host·plugin 기여는 끄기만 적을 수 있어(켜기는 설치 때 버린다) 하나라도 끄면 기본은 꺼짐이고, 이를 다시 켜는 것은 사용자 기여뿐이다. 등록되지 않은 새 `$` id 는 예약 오류다. handler 는 `user/<short>` 로 저장되고(`[a-z0-9-]`, 32자 이하, "Saved as" caption 이 입력 중 보인다) 같은 short name 은 앞선 사용자 handler 를 교체한다. 카드가 열린 동안 고른 detector 가 목록에서 사라지면 선택을 비우고 Add 를 막는다. Add 는 카드를 닫고 행을 정렬 위치에 "added on save" · "your rules added on save" 등의 대기 Tag 와 함께 넣으며, Undo 가 그 한 변경을 되돌리고 Save 가 추가 → 켜기 → 삭제 순서로 적용한다. Hook Handlers 는 공유 훅 핸들러 레지스트리(host 기본 + plugin 기여 + user 매핑) 편집 — 행별 enabled 토글(전 출처), ShellCommand 행 인라인 명령 편집, **user 행만** 제거, 인라인 추가 폼(신규 행 origin=user·max+10 priority·enabled). **출처가 지울 수 있는지를 정한다** — 레지스트리가 시작마다 host/plugin 기본값을 다시 심으므로 그 행에는 휴지통 대신 자물쇠 글리프가 온다(disabled 버튼이 아니다 — 보류된 것이 없다). 출처 Tag 는 모든 행이 달고 `host` · `you` · 그 plugin 의 id 를 찍는다. `IpcSequence` 행은 mono 한 줄 요약(스텝을 `→` 로 이음)과 오른쪽 끝 **Edit** 이다. Edit 은 그 행 둘째 줄을 인라인 편집기(공용 `CodeArea` — 줄 번호 거터, 한 줄에 호출 하나: method 뒤에 선택적 한 줄 JSON params, `#` 줄은 건너뛰고 보존하지 않는다)로 바꾸며 한 번에 한 행만 연다. 입력할 때마다 해석해 첫 오류 하나를 입력 아래 한 줄(번역 문장 + serde 원문 이유)과 거터 표시로 보이고, 오류가 있으면 Apply 가 비활성이다. 빈 시퀀스는 허용하고 "No calls" 안내를 붙인다. Apply(또는 단축키 `code_area_apply`, 기본 Mod+Enter)는 탭 초안에 넣고 창의 Save 가 저장하며, Cancel(또는 `code_area_cancel`, 기본 Esc)은 편집기를 닫는다([입력칸 바인딩](../keybindings/index.md#입력칸-바인딩--text_field_binding_fields)). host/plugin 행을 고치면 같은 id 의 user override 로 저장하되 source·priority·enabled 는 건드리지 않는다. 그 override 는 기본값보다 우선하므로 이후 host·plugin 이 기본 시퀀스를 바꿔도 **따라가지 않는다**. 행에는 편집했다는 표시가 없고(origin Tag·자물쇠 그대로) 설정 창에 되돌리는 버튼도 없다 — 기본값으로 되돌리려면 `tasty hook-handler remove --id <id>` 로 user 부분을 지운다. 한 줄 형식으로 쓸 수 없는 method 가 든 시퀀스는 Edit 대신 "Edit with CLI"(툴팁 = 명령)와 `tasty hook-handler get --id <id>` 복사 버튼이고, 그 시퀀스는 [`tasty hook-handler get`/`upsert`](../hooks/index.md#핸들러-레지스트리-hook_handler) 로 고친다(`~/.tasty/hook-handlers.toml` 손편집 + `tasty hook-handler reload` 도 그대로 된다). Add handler 카드는 ShellCommand 만 만든다. 이 카드도 surface-raised 라 라벨은 text-secondary-raised, Input 은 raised 바탕, Cancel 은 raised ghost 글자다. 웹훅 리스너(bind/port/secret) 설정은 여기 노출하지 않는다(CLI 전용).
- **Misc** — L2: Scripts (전 플랫폼·최상단 — Lua 스크립트 관리, [lua-hooks](../lua-hooks/index.md)) + Tastyrc (Windows 전용).
- **Plugins** — 플러그인 기여 설정 페이지 (동적).

L2 섹션은 좌측에 목록으로 뜨고 **필터 텍스트로 검색** 가능 (L1 전환 시 클리어).

### 좁은 창의 행 쌓기

설정 행은 라벨 열(서브탭 최장 라벨을 150 … 240 으로 clamp) · `settings-label-gap` · 컨트롤로 나란히 놓인다. 지원 최소 창(1100 × 700)보다 좁아 컨트롤이 라벨 옆 남은 폭에 들어가지 않으면 그 행만 쌓는다 — 라벨은 행 전체 폭에서 줄을 바꾸고, 컨트롤은 그 아래 행 시작 x 에 `settings-row-stack-gap`(4) 띄워 둔다. 쌓인 행은 컨트롤이 `settings-row-stack-hysteresis`(16) 만큼 여유를 두고 들어갈 때만 다시 나란히 둔다. 한 서브탭 안에서 나란한 행과 쌓인 행이 섞일 수 있다.

- 판단은 행마다 지난 프레임에 잰 컨트롤 자연 폭으로 하고, 판단이 바뀐 프레임은 다시 그린다(`tasty_ui_widgets::settings_stack_row`).
- 컨트롤 안의 기존 줄바꿈(여러 단축키 버튼, 숫자 칸의 단위)은 그대로다. 줄을 바꾸는 컨트롤은 첫 조각이 남은 폭보다 넓을 때만 쌓인다.
- 적용 범위는 `SettingsRow::show` 로 그리는 모든 행과 단축키 서브탭의 엔트리 · 빠른 전환(수식키·슬롯) · 스크립트 행이다. 글꼴 override 격자처럼 컨트롤 줄을 직접 짜는 행(`SettingsRow::show_label`)은 쌓지 않는다.

### 고정 선택지 드롭다운

General 의 닫기 동작(`close_behavior`: ask · minimize · quit), Terminal 의 링크 수식키(`link_click_modifier`: ctrl · alt · none)와 Windows 셸 모드(`shell_mode`: default · tasty)는 공용 Select 로 그린다. 트리거 높이는 `select-height`이고 폭은 닫기 동작·셸 모드가 `field-width-lg`, 링크 수식키가 `field-width-md`다. 고른 옵션은 위 문자열 그대로 저장한다.

- 저장 값이 목록에 없으면(설정 파일 손편집 등) 목록 끝에 `<값> (unknown)` 행을 붙여 선택해 두고 값을 덮어쓰지 않는다. 이 행을 다시 골라도 값은 그대로이며, 목록의 옵션을 고르면 첫 옵션이라도 바로 그 값이 된다.
- 시작할 때 설정을 읽으면서 목록 밖 값을 기본값으로 정규화하므로 이 행은 정규화를 거치지 않은 값에서만 보인다.
- 시험: `src/view/settings/ui/tabs/choice.rs`, 탭별 `general.rs`·`terminal.rs` 의 시험이 트리거 상자(높이·폭), 저장 문자열, 목록 밖 값 보존과 첫 옵션 교정을 헤드리스 egui 클릭으로 확인한다. 셸 모드 시험은 Windows 빌드에서만 돈다.

### 숫자 칸

설정의 숫자는 `src/view/settings/ui/tabs/number.rs` 의 공용 칸으로 입력한다. 입력 중에는 보정하지 않고, 포커스를 잃거나 `↵` 를 누를 때 눈금과 범위를 적용한다.

- 설정 파일에서 읽은 값은 범위 밖이어도 보정하지 않고 그대로 보인다. 사용자가 칸의 글자를 바꾸기 전에는 경고하지 않고 확정하지도 않는다. 이 규칙은 정수 칸과 소수 칸(토스트 표시 시간·휠 거리 등) 모두에 적용된다.
- 소수 자리가 없는 칸은 값 전체를 담도록 폭이 자란다. 폭은 max(`number-field-min-width` 90, (글자 수 + 1) × 칸 mono 글꼴의 `0` 너비 + 상자 padding·테두리)이고 상한은 컨트롤 칸의 남은 폭이다. 편집 중에는 친 글자 수로 다시 재므로 안쪽 스크롤 없이 커서가 움직인다. 소수 칸은 `field-width-xs` 고정이다. File Handlers priority 칸은 이 공용 칸이 아니며 90 고정이다.
- 정수로 저장하는 값(i64·u64·usize)은 `integer_field` 가 `i128` 로 다룬다. 그래서 f64 로는 자릿수를 잃는 큰 값도 그대로 보인다(예: 디스크 보관 줄 수의 9223372036854775807).
- 디스크 보관 줄 수는 단위를 칸 뒤에 두고, 칸의 값이 -1 이면 단위 대신 "no limit" 을 적는다. 범위 밖 값을 치면 칸이 invalid 테두리가 되고 칸 아래에 `settings.performance.disk_lines_range` 문장이 나온다. 다른 칸의 범위 줄은 범위와 확정될 값을 적는다.
- 칸이 남은 폭 상한에 닿을 만큼 좁은 컨트롤 칸에서는 값 전체가 보인다고 보장하지 않는다. 지원하는 설정 창 폭에서 가장 긴 i64 값(20자)은 상한에 닿지 않는다.

### 플러그인 기여 설정 (generic 컨트롤)

플러그인이 `[[contributes.settings_pages]]` 로 기여한 페이지를 host 가 `draw_plugin_settings_page` 로 렌더한다. manifest item `kind` 별로 generic 컨트롤을 그린다 — `toggle` → Switch, `select` → Select(드롭다운), `number` → text Input(mono, + `suffix_key` 단위), `font_override` → surface 폰트 섹션. `toggle`/`select`/`number` 값은 `plugin_settings.<plugin_id>.<storage_key>` 슬롯(`PluginSettingValue` = Bool/Number/Text)에 저장·영속되며(`font_override` 의 전역 `plugin_font_overrides` 와 별개 네임스페이스), 변경 즉시 write + persist 된다. 첫 소비자는 `com.tasty.html` — Appearance 에 HTML viewer 설정(zoom / color scheme / allow remote content / sandbox scripts)을 이 방식으로 노출한다.

> **surface 폰트 override 저장소**: surface-kind 폰트 override 는 전부 `appearance.plugin_font_overrides.<kind>`(generic per-kind, host 는 live 경로에서 특정 kind 이름을 모른다)로 수렴한다. 단 레거시 top-level `[markdown_font]`/`[explorer_font]` 섹션은 **전환기 back-compat 로 유지**한다(`migrate_legacy_font_overrides` 가 load 시 `plugin_font_overrides` 로 일회성 승계 — 읽기 전용, write-back 없음). 이전 버전 사용자의 설정을 옮길 수 있도록 유지하는 호환 경로다.

**쓸 수 없는 글꼴**: 글꼴 값은 저장을 막지 않고 사용할 때 건너뛴다. 그 글꼴을 뺀 다음 글꼴로 동작하고(이름으로 찾은 글꼴이 유효하면 그것을 쓴다) 설정값은 고치지 않는다. 그래서 기본·플러그인 글꼴은 파일을 정상 글꼴로 바꾸면 설정을 건드리지 않아도 재시작 뒤 그 글꼴을 쓴다. 터미널 글꼴은 그렇지 않다. 터미널 렌더러가 custom 파일과 줄 높이 없이 시작하고 부팅 때 저장값으로 글꼴 signature를 기록하므로, 저장된 custom 파일과 줄 높이는 적용한 그 실행에서만 쓰이고 재시작 뒤에는 쓰이지 않는다. 판정은 값을 쓰는 경로마다 다르고 `judge_font`(`src/adapters/ui/font_judge.rs`) 한 곳에 있다.

| 행 | 판정 | 오류 |
|---|---|---|
| 기본·플러그인 custom 파일 | 읽기와 언어팩 글꼴과 같은 판정(`tasty_i18n::font::validate`) | 읽기 실패, 파싱 실패, 줄 높이 0 이하 |
| 터미널 custom 파일 | 읽기, fontdb, 그 파일만 든 cosmic-text에서 자기 cmap의 글자를 그려 보기(`tasty_font::judge_terminal_file`) | 읽기 실패, face 없음, 어느 face도 글자를 그리지 못함 |
| 기본·플러그인 family | 이름 조회와 위 egui 판정 | 거절, 이름 없음 |
| 터미널 family | 이름 조회와 위 그려 보기 | 이름 없음, 그리지 못함 |

두 경로가 갈리는 값이 있다. 줄 높이가 음수인 파일은 터미널이 그리므로 터미널 행에서 오류가 아니고, 앞부분만 남은 파일이나 cmap 없는 파일은 egui만 받으므로 터미널 행에서만 오류다. 일부 글리프만 손상된 파일은 그릴 수 있는 글자가 있어 어느 경로에서도 잡지 못한다. 값이 비었거나 family가 `monospace`면 판정하지 않고, Use default가 켜진 행은 오류를 표시하지 않는다. family 이름은 그 행의 custom 파일이 제공해도 찾은 것으로 친다. 파일이 정상이면 family가 비었거나 달라 터미널이 그 파일을 쓰지 않아도 오류가 아니다. 이 답과 저장된 family 이름이 없어진 경우의 답은 `is_error` 한 함수로 정한다.

오류가 있는 입력에는 빨간 테두리와 아래 한 줄을 표시한다. family 콤보에는 줄만 표시한다. 입력에 포커스가 있거나 목록이 열린 동안에는 글자를 지우고 보이던 줄의 칸을 유지하며 포커스가 빠지면 그 값의 판정을 그린다. Save는 항상 할 수 있고 값을 지우지 않는다. 미리보기는 거절된 글꼴을 뺀 적용 글꼴로 그린다. 입력 중에 UI 행의 판정은 미리보기 등록이 키마다 돌지만 터미널 판정은 포커스가 빠진 뒤 값이 바뀌었을 때만 돈다. 글리프가 글꼴 atlas보다 크면(atlas 폭 2048에서 폭 2048px 또는 높이 1024px 초과) 판정과 관계없이 그 글자만 빈 칸으로 그린다. 글꼴은 계속 쓰이고 입력란 오류나 토스트는 없다. 본체는 경고 로그를 남기고, `-p`로 빌드한 번들 플러그인은 경고도 남기지 않는다. 빈 칸의 폭은 1em 이하다. 큰 글자 크기와 배율에서는 정상 글꼴에도 해당할 수 있다.

**글꼴 묶음 안의 이름**: 이름으로 찾은 글꼴이 묶음 파일(`.ttc`) 안에 있으면 그 이름의 글꼴 번호를 읽는다. `custom_font_path` 는 파일만 지정하므로 묶음 파일이면 0번을 읽는다.

> **설정 적용**: host 가 `resolve_webview_settings` 로 `plugin_settings."com.tasty.html"` 을 읽어 네이티브 webview 에 직접 적용한다(별도 host→plugin IPC 없음 — `font_override` 호스트 적용과 같은 선례). 적용 현황:
> - **zoom**: 3 OS 모두 실효.
> - **sandbox scripts**: 켜 두면 main frame 문서마다 JS를 끄고, 사용자가 허용한 문서에서만 켠다. `resolve_webview_settings`는 전역 값만 넘기고 문서별 판정은 navigation 콜백이 한다([HTML Viewer (`com.tasty.html`)](../../surfaces/html/index.md)). 끄면 모든 문서에서 JS를 켠다. 문서별 적용 시점은 Linux에서만 측정했다.
> - **color_scheme**(`prefers-color-scheme` 강제): macOS 실효(NSAppearance). Windows/Linux 는 no-op(후속).
> - **allow remote content**(원격 http/https 서브리소스 차단): 3 OS 모두 실효 — macOS 는 `WKContentRuleList`, Windows 는 `WebResourceRequested` 403, Linux 는 같은 스키마의 content filter 를 `WebKitUserContentFilterStore` 로 컴파일해 user content manager 에 붙인다(네비게이션은 decide-policy 가 따로 막는다). 근거·대안은 [WebView 호스트 계약](../../design/systems/webview.md). 플랫폼별 백엔드는 해당 OS에서 검증해야 한다.

### draft / save 모델

편집은 **작업 사본(`draft`)** 에 쌓이고, Save 시 영속 `Settings` 로 커밋, Cancel 시 폐기.

**Save 는 필드 단위 3-way 병합으로 커밋한다**(`tasty_settings::merge::merge_settings_edit`, 호출은 `src/app/modal.rs` `close_active_modal`). 창은 열 때의 설정(`SettingsView::opened_settings`)을 들고 있다가, Save 때 그것·창의 결과·저장 시점의 현재 설정을 비교한다. 창이 열린 동안 agent IPC(입력 규칙·원격 전송·웹훅 외부 허용 등)나 스크립트 승인이 현재 설정을 바꿀 수 있기 때문이다.

- 창이 바꾸지 않은 필드는 현재 값, 바꾼 필드는 창의 값을 쓴다. 표·맵은 키마다 내려가 비교한다.
- 양쪽이 같은 필드를 다른 값으로 바꿨으면 창(사용자) 값을 쓰고, 그 경로들을 `settings save: fields also changed elsewhere …` warn 로그로 남긴다.
- 목록은 기본적으로 값 하나로 비교한다. 아래 목록은 항목 단위로 맞춘다. 순서는 창의 결과를 따르고, 창을 연 뒤 다른 경로가 더한 항목은 뒤에 붙는다. 창이 지운 항목을 다른 경로가 바꿨으면 창의 삭제를 따르고 충돌로 남긴다.

| 목록 | 항목 키 |
|---|---|
| `terminal_input.rules` | `app` |
| `scripts.scripts` | `id` (승인 해시 `sha256` 등 필드는 항목 안에서 다시 3-way) |
| `scripts.scripts[].triggers` | 항목 값 |
| `keybindings.script_bindings` | 항목 값 |

- 직렬화·역직렬화에 실패하면 창의 결과를 그대로 쓰고 사유를 같은 warn 로그로 남긴다. 직렬화하지 않는 필드(`serde` skip — 저장 정책 `origin`, 레거시 `appearance.markdown_font`·`explorer_font`)는 비교에 나타나지 않으므로 `carry_unserialized` 가 창의 결과에서 잇는다. `tasty-settings` 소스의 skip 사용이 이 목록과 다르면 `serde_skipped_fields_match_the_carried_list` 시험이 실패하므로, 새 skip 필드를 더할 때 목록과 잇는 함수를 함께 고친다.
- 사용자 입력으로 바꾸는 설정(테마 토글·줌·힌트 위치·마우스 캡처 메뉴 등)은 modal 이 열린 동안 메인 윈도우 입력이 막혀 이 병합에 들어오지 않는다(`src/view/main.rs` `handle_engine_event`). 일부 항목(FileHandler 의 파일 서브탭 → `~/.tasty/file-handlers.toml`, Hook Handlers → `~/.tasty/hook-handlers.toml`)은 Save 시 각 registry commit 후 user TOML 에 직접 atomic write.

**저장 실패는 오류 토스트로 알린다.** Tastyrc(Windows) 편집 저장이 실패하면 그 사유를 모달이 들고 있다가, 창이 닫힐 때 host 가 회수해 main window 에 Error 토스트(`toast.bashrc_save_failed`)로 띄운다 — Save 는 곧바로 설정 창을 닫으므로 설정 창 안에 띄우면 보이지 않는다. 사유에는 대상 경로와 OS 에러가 들어 있어 문구에 함께 싣되, 토스트 200자 캡에 맞춰 **가운데를 생략**한다(`tasty_i18n::t_fmt_fit`) — 호스트 기본 잘림은 꼬리(=OS 에러)를 버린다. 성공 토스트는 없다(저장은 기본 기대 동작이라 매번 알리면 소음). → [toast](../../design/systems/toast.md)

**설정 적용 결과도 기존 토스트로 알린다.** Save 한 설정이 검사·admission 에서 거절되면 Error 토스트(`toast.settings_not_applied`), 메모리에는 적용됐지만 `config.toml` 에 저장하지 못하면 Warning 토스트(`toast.settings_not_saved` — 이번 실행에만 적용되고 다시 시작하면 사라진다)를 포커스된 main window(없으면 첫 main window)에 띄운다. 두 경우를 다른 종류·문구로 구분한다. 사유는 `t_fmt_fit` 으로 가운데를 줄인다. category 초기화(`workspace_categories_enabled` 끄기)의 journal 실패는 공통 적용 오류 보고가 맡는다. 에이전트·plugin 요청의 같은 결과는 토스트 대신 응답으로 돌려준다 → [완료 의미](../../architecture/data-flows.md#에이전트plugin-설정-변경-요청의-완료-의미)

### 단축키 탭 (Keybindings)

키 조합을 직접 녹화해 바인딩 할당. 충돌 시 확인 팝업으로 수락/거부. (모든 단축키는 `KeybindingSettings` 경유 — 코드 하드코딩 금지.)

### 플러그인 기여 페이지

플러그인이 설정 페이지를 contribute 하면 Appearance 의 sub-tab + Plugins 탭에 `(plugin_id, page_id)` 복합키로 나타난다. 등록된 plugin page 가 없으면 Plugins 탭/해당 sub-tab 이 비거나 사라진다.

## 인터페이스

- **사용자**: 사이드바 설정 버튼 → 모달, L1/L2 탐색, 편집 → Save/Cancel.
- **각 설정 도메인은 해당 기능으로 연결** (설정 창은 편집을 담당하고 동작 규칙은 각 기능 문서가 설명한다):
  - Keybindings → [`features/keybindings/`](../keybindings/index.md) / 키 매핑 정책 [`design/policies/key-mapping`](../../design/policies/key-mapping.md)
  - Appearance/Theme → [`design/systems/theme`](../../design/systems/theme.md)
  - Clipboard → [`features/clipboard/`](../clipboard/index.md) · Notifications → [`features/notifications/`](../notifications/index.md) · FileHandler(파일 서브탭) → [`features/file-handler/`](../file-handler/index.md) · Hook Handlers → [`features/webhook/`](../webhook/index.md)·[`features/hooks/`](../hooks/index.md)
  - Plugins → [`features/plugin-system/`](../plugin-system/index.md)
  - General › Language → [`features/language-packs/`](../language-packs/index.md) · 로더 규칙 [`dev-guide/i18n`](../../dev-guide/i18n.md)

## 비-목표

- 각 설정 항목의 *도메인 동작* (테마가 무엇을 바꾸나, 단축키가 무엇을 하나 등) — 설정 창은 *편집 표면* 일 뿐. 도메인은 각 기능/시스템 문서.

## Acceptance Criteria

- 사이드바 설정 버튼 클릭 시 설정 모달이 열린다 (L1 7탭).
- L1 탭 전환 시 좌측 L2 섹션 목록이 그 탭의 것으로 바뀌고 필터가 클리어된다.
- 편집 후 Save 시 영속 Settings 에 반영되고, Cancel 시 폐기된다.
- Given 설정 창이 열려 있다 When agent IPC 가 입력 규칙·원격 전송 폴더·웹훅 외부 허용을 바꾼 뒤 창에서 다른 항목을 바꾸고 Save 한다 Then IPC 가 바꾼 값과 창의 변경이 모두 남는다(`merge.rs` 의 `fields_the_window_did_not_change_keep_their_current_values`, `input_rules_merge_per_app`).
- Given 창과 다른 경로가 같은 필드를 다른 값으로 바꿨다 When Save 한다 Then 창의 값이 남고 그 경로가 warn 로그에 남는다(`a_field_changed_on_both_sides_takes_the_windows_value_and_is_reported`).
- Keybindings 에서 키 조합 녹화 시 충돌이 있으면 확인 팝업이 뜬다.
- Given 설정 창이 좁아 한 행의 컨트롤이 라벨 옆에 들어가지 않는다 When 그 서브탭을 연다 Then 그 행만 라벨 아래 행 시작 x 로 컨트롤이 내려가고, 창을 넓혀 16 이상 여유가 생기면 다시 나란해진다(`crates/tasty-ui-widgets/tests/settings_row_stack.rs`, `entries_tests::a_narrow_subtab_stacks_rows_and_starts_buttons_at_the_row_start`).
- Given 설정 파일의 디스크 보관 줄 수가 9223372036854775807 이다 When 성능 탭을 연다 Then 칸에 19자리가 그대로 보이고 칸 안에 모두 들어가며 값은 바뀌지 않는다(`number.rs` 시험).
- Given 정수 칸을 편집한다 When 글자를 더 친다 Then 다음 프레임에 칸이 한 글자만큼 자라고 값 앞부분이 칸 밖으로 밀리지 않는다(`number.rs` 시험).
- 플러그인이 설정 페이지를 contribute 하면 Plugins 탭/Appearance sub-tab 에 나타난다.

> 모달 창이라 시각 검증은 스크린샷, draft/save·plugin page 등록은 시나리오로 검증.

## 구현

- `src/view/settings.rs` — `SettingsView`, `SettingsUiState`(draft/active_tab/sub-tab 상태).
- `src/view/settings/ui.rs` — `SettingsTab`(L1 7탭: General / Terminal / Appearance / Keybindings / FileHandler / Misc / Plugins), L2 enum 군 `GeneralSubTab` / `TerminalSubTab` / `AppearanceSubTab`(Theme / Colors / General / Display / Tasty / Terminal / Explorer / Plugin) / `MiscSubTab` / `PluginSubTab`, L2 필터. FileHandler 의 L2 는 `FileHandlerSubTab`(ExtensionMapping / Detectors / Handlers / HookHandlers).
- 언어 콤보: `src/view/settings/ui/tabs/general.rs` → `tasty_ui_widgets::language_select`(갤러리 Settings specimen 공유), 목록은 `SettingsUiState.languages`(창 오픈 시 `tasty_i18n::available_languages()` 1회).
- `src/app/settings_change.rs` — 설정 변경 요청(`SettingsChange`)과 결과 채널(`SettingsDone`·`SettingsWait`), 결과 전달(적용·저장 결과와 거절을 기다리는 요청에 돌려주고 사용자 출처면 토스트). 바로 적용하는 경로와 category 초기화 뒤 journal 완료 경로가 같은 결과 전달을 쓴다. 단축키가 바뀌면 plugin에 `command.host_shortcut_changed`를 발행한다.
- 탭별: `src/view/settings/ui/tabs/*` + `keybindings_tab.rs` + `file_handler_tab.rs`(+ `file_handler_tab/hook_handlers.rs` — 훅 핸들러 레지스트리 편집).

## 화면

- [screens/settings.md](screens/settings.md) — 설정 창 레이아웃(L1 탭바 / L2 섹션 / 콘텐츠 / Save·Cancel)과 섹션별 연결.
