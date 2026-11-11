# 토스트 시스템

Toast는 “복사됨”, “저장됨”처럼 사용자 조작의 결과를 잠깐 표시한 뒤 사라지는 알림이다. `PopupManager` 가 아니라 별도 `ToastManager`(`src/adapters/ui/toast.rs`)로 관리된다. 용어 구분은 [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md).

## Popup 과의 차이

| 항목 | Popup | Toast |
|------|-------|-------|
| 수명 | 사용자가 닫을 때까지 | 일정 시간 후 자동 소멸(기본 2s, Settings › General › Overlay 의 Toast duration 으로 1~10s 조절). 포인터가 카드 위에 있는 동안 멈추고, 벗어나면 전역 수명이 처음부터 흐른다 |
| 포커스 | 클릭 시 보유 가능 | **절대 받지 않음** |
| 입력 | 클릭/드래그/X | **소비 안 함**(마우스 통과) |
| 타이틀바 | 있음 | 없음(본문만) |
| 위치 | 자유 이동 | 스코프별 고정 스택 |
| 트리거 | 사용자 또는(debug)에이전트 | 사용자 결과와 아래 허용·예외 상황 |

토스트는 팝업의 타이틀바·닫기·드래그·클릭 순서 규칙을 사용하지 않아 별도 매니저를 둔다. 영역 계산은 `LayoutContext`를 재사용한다.

## 트리거 정책 (CRITICAL)

토스트는 기본적으로 사용자 조작의 결과를 알린다. 에이전트의 CLI·IPC 요청 성공·실패는 사용자 토스트로 표시하지 않는다. [포커스 정책](../policies/focus.md)의 사용자 상태 분리를 따른다.

| 상황 | 처리 |
|---|---|
| 사용자 복사·저장 | 토스트 표시 |
| 사용자 조작의 OS 열기 실패(터미널 링크 클릭, 사용자 출처 System 파일 핸들러, Plugins 창의 폴더 열기·홈페이지) | 실패 토스트(Error) 1건. 성공에는 표시하지 않음. 에이전트·미증명 플러그인 출처의 System 핸들러 실패에는 토스트를 띄우지 않고, 요청의 `run`을 `failed`("the OS opener could not be started")로 끝낸다 |
| 사용자 intent·구조 변경·분할선 확정·카테고리 초기화 실패 | 실패 토스트(Error) 1건. preset·split 하한·철회된 kind의 기존 안내와 중복하지 않음. 화면 없는 엔진에는 표시하지 않음 |
| 에이전트 IPC 쓰기·intent 적용 실패 | 사용자 토스트 없음. 에이전트는 결과를 IPC 응답 문자열로 받는다. 접수 뒤 끝나는 요청(`file_handler.dispatch`·`markdown.navigate`·`hook_handler.dispatch`·`attach.into_gui`·기존 workspace `remote.attach`)은 응답의 `run`으로 최종 결과를 조회한다. 접수 응답 뒤의 적용 실패가 경고 로그로만 남는 그 밖의 경로는 [결과 피드백 원칙](../../principles/feedback-and-recovery.md#시작-경로에-따른-결과-전달)의 현행 미충족이다 |
| mirror 연결 끊김·손실 | 연결 상태 토스트 허용. parked engine에서는 표시하지 않음. 저장된 원격 행의 재연결 시작·성공·포기는 토스트 대신 행의 연결 팝업과 표지로 보임 |
| 원격 연결 팝업의 연결 버튼이 접수된 뒤 연결·핸드셰이크·mirror 설치 실패 | 시작한 창에 실패 토스트(Error, `attach.toast.mirror_connect_failed`) 1건, 사유는 가운데를 줄여 붙임. 그 창이 닫혔거나 View가 바뀌었으면 표시하지 않음. IPC·자동 attach·재연결 실패는 이 토스트를 내지 않음 |
| 사용자 구조 변경의 원격 적용 실패 | 실패 토스트 표시 |
| agent origin 또는 직접 IPC 구조 요청의 원격 실패 | op_id로 origin을 기억해 토스트를 띄우지 않는다. 원격 실패 사유는 run을 가진 intent(mirror 대상 `file_handler.dispatch`·`markdown.navigate`)에만 `failed`로 전달된다([file-handler](../../features/file-handler/index.md#접수-뒤-최종-결과)). 직접 IPC 구조 요청(`split`·`tab.create` 등)은 `forwarded` 접수 응답 뒤 원격 실패가 로그로만 남으며 이는 현행 미충족이다 |
| 원격 markdown 잘림 | 최초 열기·실패 후 변경 재조회·사용자 새로고침에는 알려진 예외로 표시. markdown.reload IPC 재조회에는 로그만 기록 |
| 사용자 단축키·변경 확인 승인으로 실행한 Lua 스크립트의 실패 | 그 단축키를 누른 창에 실패 토스트(Error, `Window`) 1건. 성공에는 표시하지 않음. 창이 닫힌 뒤 도착한 결과는 버림. 자동실행 실패는 토스트가 아니라 경고 알림([Lua 실행 결과 전달](../../features/lua-hooks/index.md#실행-결과-전달)) |
| 사용자 설정 저장에서 저장된 글꼴 값이 거절됨(`App::report_rejected_fonts`) | 값마다 Warning 카드 1장, 버튼 없음, 본문에 값. 설정 창은 Save로 닫히므로 메인 창에 띄운다. 같은 값을 다시 저장해도 늘지 않고, 통과한 뒤 다시 거절되면 또 띄운다. 입력 중에는 띄우지 않는다. 사용자 출처 적용에서 글꼴 값이 그대로면 파일을 다시 읽지 않지만 설정 창 Save는 값이 같아도 판정한다. 에이전트의 `settings.apply` 등 그 밖의 출처는 경고 로그만 남긴다 |
| 부팅 때 저장된 글꼴 값이 거절됨 | 언어팩 글꼴 경고와 같이 사용자 조작 없이 표시하는 알려진 예외. 값마다 Warning 카드 1장. 부팅 카드의 수명도 저장된 Toast duration을 따른다 |
| OSC 52 클립보드 쓰기 | 보이지 않는 클립보드 변경을 알리기 위해 표시. PTY 바이트만으로 사용자·에이전트를 구분할 수 없는 알려진 예외 |

연결 상태 알림은 사용자가 보는 원격 사본이 최신인지 알려 준다. 세션을 에이전트가 열었더라도 이후 네트워크 단절은 IPC 호출의 직접 결과와 구분한다. 반대로 에이전트가 요청한 구조 변경의 거절은 연결 상태 예외로 허용하지 않는다.

### origin이 적용되는 경로

`report_apply_error`와 preset 적용·저장 실패는 사용자 origin에서만 토스트를 낸다. 일반 오류는 작업 범주별 번역 문구를 쓰며 내부 경로·IPC 메서드·오류 원문은 로그에만 남긴다. 동기 dispatch 실패도 요청을 시작한 View가 남아 있을 때 같은 규칙을 따른다. 분할선 거절은 미리보기를 취소하고 이전 위치로 되돌렸다는 안내를 표시한다. 카테고리 초기화 실패는 기존 설정 결과 전달 규칙대로 열려 있는 메인 윈도우에 표시하며, 메인 윈도우가 없으면 로그만 남긴다. 에이전트의 forward 요청에는 `silent_failure`를 붙이고 attach client의 `AgentRequests`가 회신까지 op_id를 보관한다. 성공·실패 회신 뒤에는 항목을 지우고 재연결 때도 비운다. preset 저장 성공 알림은 이 실패 처리 규칙의 대상이 아니다.

구조 변경은 App journal의 원 reply·origin에 오류를 전달한다. IPC·plugin·원격 요청은 에이전트 origin을 유지하며, 사용자 View continuation만 현재 identity와 선택 세대를 확인해 토스트를 만든다. 원격 요청을 재전달하는 경우도 사용자 조작으로 승격하지 않는다.

`file_handler.dispatch`는 사용자가 조작한 플러그인 팝업을 검증할 수 있으면 사용자 origin이 된다. markdown 파일열기 팝업이 이 경로다. 사용자 origin을 보존한 mirror forward는 원 요청의 실패 표시 정책을 유지한다. accepted 응답 뒤의 실패 사유는 이미 보낸 응답에 넣지 않고, 접수 응답의 `run`을 `failed`(원격이 결과 전에 끊기면 `unknown`)로 끝내 `run.result {run}`으로 돌려준다([file-handler](../../features/file-handler/index.md#접수-뒤-최종-결과)).

관련 검사는 `src/intent/apply_error_tests.rs`, `src/app/attach_client/agent_origin.rs`, markdown 원문 요청 테스트에 있다. 이 테스트들이 모든 토스트 생성 경로를 검사하는 것은 아니다.

### 새 경로를 검토할 때

직접 호출은 다음 검색으로 찾을 수 있다.

```bash
rg -n 'toasts|report_apply_error|push_toast' src/adapters/ipc src/app/ipc src/app/ipc.rs src/boot/headless_dispatch.rs src/boot/headless_plugins.rs -g '!**/*_tests.rs'
```

검색 결과가 없더라도 IPC가 만든 winit 이벤트·도메인 이벤트·PTY 출력·intent를 따라 다른 파일에서 토스트가 발생할 수 있다. 직접 호출 검색을 정책 전체의 검증으로 보고하지 않는다. 새 실패 경로는 실제 origin을 확인하는 지점과 비동기 회신까지 함께 검사한다.

손실 경고와 재연결 성공은 서로 다른 메시지라 반복해서 뜰 수 있다. 느린 연결에서 방해가 된다면 같은 창에서 일정 시간 발생 횟수를 세어 별도 상태 표시로 옮길지 판단한다. markdown 잘림도 문서 내 표지로 옮기는 결정은 아직 하지 않았다.

## 스코프

`ToastScope`는 알림을 놓을 영역을 지정한다. `ToastScope`: `Window` / `Workspace(usize)` / `Pane(u32)` / `Surface(u32)`. 기본은 `Surface`(어디서 일어난 일인지 모르면 `Window`). 같은 스코프 내 여럿이면 오른쪽 아래 모서리에서 위로 쌓는다. 가장 새 토스트가 맨 아래에 오고 오래된 것이 위로 밀린다. 맨 아래 카드는 스코프 하단에서 띄워 놓는다. `Window`·`Workspace`처럼 창 전체를 덮는 스코프는 메인 창에서 창 하단으로부터 `toast-stack-offset-bottom`(→ `size-36` = 상태바 24 + 12)만큼 띄워 하단 상태바 위에 둔다. 이 값은 상태바와 카드 구조 치수처럼 UI 배율을 따르지 않는다. Settings 창에서는 `toast-stack-offset-bottom-settings`(→ `size-64` = 하단 버튼 줄 52 + 12)만큼 띄워 Cancel·Save 위에 둔다. 하단 버튼 줄이 UI 배율을 따르므로 이 값도 배율을 따른다. `Pane`·`Surface`는 가장자리 여백 `space-md`(12)를 둔다. 공용 위젯은 이 구분을 `ToastStackBottom`(`Window` / `SettingsWindow` / `ScopeMargin`)으로 받고, 맨 아래 카드의 오른쪽 아래 모서리는 `stack_anchor`가 계산한다. 스코프가 화면에서 사라지면 즉시 제거.

**같은 모서리의 스택**: 맨 아래 카드의 기준점이 같은 스코프들은 한 열로 합친다. 오른쪽 아래 pane 의 스택은 workspace 스택과 같은 점에 놓이므로, 따로 그리면 카드가 겹친다. 합친 열은 가장 새 카드가 맨 아래이고 상한 5개를 스코프 구분 없이 함께 센다. 영역은 가장 넓은 스코프의 것을 써서 좁은 pane 위쪽 경계에 잘리지 않는다. 판정은 기준점 좌표로 하므로 `Window`·`Surface` 스코프도 같은 점에 놓이면 합쳐진다.

메인 창 밖의 Settings·Preset·Plugins 창은 각자 토스트 스택을 갖고 `Window` 범위만 그린다. Settings 창의 관리자는 `ToastManager::for_settings_window`로 만들어 위 Settings 하단 거리를 쓴다. debug 빌드의 `debug.toast`는 창과 범위를 지정해 토스트를 띄운다. 사용자 조작 없이 위치를 실측하기 위한 진입점이며 release에는 없다([debug IPC](../../dev-guide/debug-ipc.md)).

## 시각 / 레이아웃

모든 색·치수는 Theme 토큰([theme.md](theme.md)). 배경 `surface-raised` + 1px `toast-border`(= `border-strong`) 보더 + `corner_radius`, 본문 `font_size_body`, 스코프 우측 하단 정렬·스택, 카드 사이 간격 `space-sm`(8). 종류 강조는 좌측 컬러 바다. 바 두께는 `toast_accent_width`(`toast-accent-width` = 2px)이며 본문 줄바꿈 폭과 카드 폭 계산에도 같은 값을 쓴다:

| 종류 | 바 색 | 용도 |
|------|-------|------|
| Info | `blue` | 일반(기본) |
| Success | `green` | 완료 |
| Warning | `yellow` | 주의 |
| Error | `red` | 실패 |

시안의 `agent` 변형(`accent-agent` 바)과 `icon` 슬롯은 카탈로그 전용이다. 본체는 에이전트가 낸 결과를 토스트로 알리지 않으므로(위 트리거 정책) `ToastKind`에 agent가 없고, 본체 카드는 강조 바와 본문만 그린다.

**hint**: 알림을 낸 동작의 단축키를 카드 오른쪽 끝에 키캡(`kbd_text_parts_painted`, Kbd와 같은 토큰)으로 그린다. 그 동작을 **메뉴나 마우스로** 실행했고 binding이 비어 있지 않을 때만 붙인다. 키로 실행했다면 사용자가 이미 키를 알고, binding이 빈 동작에는 붙이지 않는다. 키캡 표기는 단축키 설정의 표시 방식(`KeybindingSettings::format_display_parts`)을 따르며 binding의 첫 값만 쓴다. hint는 본문 첫 줄의 세로 중심에 맞추고 줄어들지 않는다. 본문 줄바꿈 폭에서 키캡 폭과 간격(`TOAST_HINT_GAP` = `space-sm`)을 먼저 빼므로 긴 본문이 먼저 줄바꿈된다. 키캡 색에도 카드와 같은 불투명도를 곱한다.

hint를 붙이는 호출부는 다음 셋이다. 호출부는 `ToastManager::push_with_hint`에 `toast::binding_hint(settings, <binding id>)`의 결과를 넘긴다.

| 알림 | 경로 | binding |
|---|---|---|
| `toast.copied` | 터미널 우클릭 메뉴의 Copy | `copy` |
| `toast.copied_path` | 탐색기 우클릭 메뉴의 Copy path(선택한 항목이 있을 때) | `copy_path` |
| `toast.copied` | 터미널 링크 우클릭 메뉴의 Copy | `copy_link` |

`copy_link`는 기본 키가 없으므로 사용자가 키를 묶기 전까지 링크 메뉴의 Copy에는 hint가 없다. 링크 복사는 선택 영역 복사와 다른 동작이므로 `copy` binding을 빌려 쓰지 않는다.

같은 알림이라도 키보드 단축키·vi 복사 모드·surface ID 복사·빈 곳을 눌러 현재 폴더를 복사하는 메뉴는 hint가 없다. 앞의 둘은 키로 실행한 동작이고, 뒤의 둘은 대응하는 binding이 없다. 마우스 드래그 선택만으로 복사하는 동작은 본체에 없다.

> 페이드(등장/소멸 알파만, 위치 이동 없음)는 적용된다 — theme.md 의 "터미널 콘텐츠 애니메이션 0ms" 규칙은 **터미널 콘텐츠** 한정이라 비-터미널 알림 UI 에는 적용되지 않는다.

**카드 폭**: 스택의 카드마다 자기 내용 폭을 쓰고 상한은 `toast_max_width`(`toast-max-width` = 320)다. 오른쪽 끝을 앵커에 맞추므로 짧은 알림은 긴 알림보다 좁다. 공유 스택 폭이 없어 카드가 들어오고 나가도 다른 카드가 다시 흐르지 않는다.

**경계 처리**: 토스트의 모든 변은 자기 스코프 `scope_rect` 안에 머문다. ① 카드 폭 상한은 surface 안쪽 폭(`width - 2*margin`)으로도 클램프해 좁은 surface 에서 좌측 누출을 막고, ② 스택이 스코프 상단을 넘으면 더 오래된 토스트는 그리지 않으며, ③ scope 경계로 painter 를 클립해 1px 단위 누출까지 차단한다. 우측 하단 정렬 자체는 유지된다.

## 입력 — 소비하지 않음

Toast 위에서 마우스 클릭/드래그해도 토스트는 무시하고 이벤트가 아래 레이어(터미널/popup/divider)로 통과한다. `popup_hovered` 도 토스트 영역에선 false. 키보드 포커스도 받지 않아 `has_focused()` 와 무관.

**hover 유지**: `ToastManager::draw`는 만료 판정 전에 포인터 위치(`hover_pos`)를 지난 프레임에 그린 카드 사각형과 비교한다. 포인터 아래 카드는 그 프레임의 생성 시각을 등장 페이드 끝(`now - FADE_IN`)으로 당겨, 완전 불투명으로 남고 만료되지 않는다. 페이드아웃 중이던 카드도 불투명으로 돌아온다. 포인터가 카드를 벗어나거나 창을 떠나면(`hover_pos`가 없음) 그 프레임부터 전역 수명이 처음부터 흐르고 소멸 페이드 뒤 사라진다. 모션 감소에서는 수명이 끝나면 페이드 없이 사라진다. 토스트별 수명 값은 없고 언제나 전역 `toast_duration_ms`를 쓴다.

- 판정은 카드 단위다. 같은 열에서 포인터 아래 카드만 멈추고 다른 카드는 제 수명대로 사라진다. 열은 앵커(아래)에서 위로 쌓으므로, 붙잡힌 카드보다 아래(새) 카드가 사라지면 붙잡힌 카드가 내려와 포인터에서 벗어날 수 있다.
- 판정은 좌표 비교뿐이다. Area·`Sense`·`interact`를 만들지 않으므로 위 입력 계약이 그대로다.
- 살아 있는 토스트가 모두 붙잡혀 있으면 매 프레임 다시 그리지 않고 `request_repaint_after(수명)`만 건다. 포인터 이동과 창 이탈은 입력으로 프레임을 깨운다.
- 포인터가 native WebView 위에 있으면 메인 창 egui가 포인터 위치를 받지 못한다. 카드와 겹치는 WebView는 카드가 떠 있는 동안 숨기므로(아래 절) 그 자리에서는 판정된다. 키를 받는 WebView 위의 카드는 WebView 아래에 가려 hover 대상이 아니다.

## native WebView 위의 토스트

토스트는 egui가 그리므로 OS 자식 창인 WebView(`html` · `markdown`)가 그 위를 덮는다. 카드 사각형과 겹치는 WebView만 그 카드가 그려지는 동안(페이드 포함) 숨기고, 그 자리에는 html surface의 boundary 빈 타일이 보인다. 겹치지 않는 WebView는 그대로 보인다.

**지금 키를 받는 WebView는 숨기지 않는다.** 숨긴 WebView에는 키가 가지 않고 호스트도 그 키를 쓰지 않아, 사용자가 치던 입력이 토스트가 떠 있는 동안 조용히 사라진다. 그래서 그 WebView는 그대로 두고 카드는 이전처럼 그 아래에 가린다. 판정은 백엔드의 `receives_keyboard_input`이다.

| 플랫폼 | 키를 받는 WebView |
|--------|-------------------|
| Linux(X11) | X 포커스가 WebView 안에 있거나, 포커스 창이 WebView의 조상(또는 PointerRoot)이고 포인터가 WebView 위에 있다. X 서버는 포커스 창의 자손 위에 포인터가 있으면 키를 그 자손에 보낸다. 오버레이(불투명·투명)가 입력 shape를 비운 동안에는 X가 키를 부모로 보내므로 포커스만 본다([WebView](webview.md#linux-불투명-구현)) |
| macOS | `holds_keyboard_focus`와 같다(first responder) |
| Windows | `holds_keyboard_focus`와 같다(GetFocus) |

포커스 조회는 OS 호출이므로 카드와 겹치는 WebView에만 한다. 토스트는 포커스를 받지 않으므로 숨긴 WebView의 키보드 포커스를 회수하지 않는다.

`draw_toast_scopes`가 그린 카드마다 토스트 id와 사각형(스코프 경계로 자른 값)을 돌려주고 `ToastManager::card_rects`가 마지막 프레임의 사각형을 보관한다. 토스트가 없으면 비어 있다. WebView 동기화는 이 영역을 물리 좌표로 바꿔 WebView 사각형과 면적을 공유하는지 비교한다. 변이 맞닿기만 하면 겹침이 아니다. 전체 WebView를 숨기는 방식은 토스트마다 모든 페이지가 깜박이고, 카드를 WebView 밖으로 옮기는 방식은 pane마다 WebView가 아닌 영역이 없을 수 있어 채택하지 않았다. 키를 받는 WebView 위의 카드를 보이게 할 방법은 디자인 요청으로 남아 있다.

## 합치기 / 제한

같은 스코프에서 같은 메시지가 짧은 시간(기본 500ms) 내 다시 발생하면 새로 만들지 않고 **기존 토스트 수명만 갱신**(연속 Ctrl+C 깜빡임 방지). 이미 등장 페이드를 지난 카드는 생성 시각을 등장 페이드 길이만큼 앞당겨 불투명을 유지한 채 수명을 처음부터 흐르게 하고, 등장 중인 카드는 생성 시각을 바꾸지 않는다. 마지막 draw에서 포인터 아래에 있던 카드는 나이와 관계없이 합친다. 그래서 포인터가 있는 동안 오는 같은 문구는 draw가 쉬고 있어도 언제든 그 카드에 합쳐진다. 스코프당(같은 모서리로 합친 열은 열당) 최대 동시 5개, 초과 시 가장 오래된(맨 위) 것 즉시 제거. 넘친 토스트를 `+N more` 같은 요약 행으로 접지 않는다. 쉬는 카드는 나이와 관계없이 모두 불투명(alpha 1)이고 불투명도는 등장·소멸 페이드에만 쓴다.

본문은 **200자(유니코드 문자 기준)** 로 제한한다. 초과 시 앞 200자만 남기고 줄바꿈 + 안내 접미(`toast.char_limit_notice`)를 붙여(`<앞 200자>\n(200자 제한)`) 긴 입력 때문에 토스트가 지나치게 길어지는 것을 막는다. 길이/자르기는 char 경계로 처리해 멀티바이트에서 안전하며, coalesce 비교 이전(`push` 진입부 `truncate_message`)에 적용된다.

문자 수 상한은 `tasty-i18n` 의 `TOAST_MAX_CHARS` **하나**다. 상한에 맞춰 문구를 만드는 쪽(`fit_fragment` · `t_fmt_fit`)과 상한을 적용하는 쪽(`truncate_message`)이 같은 상수를 읽고, 접미가 말하는 숫자도 번역문의 `{}` 에 그 값이 들어가 만들어진다. 셋 중 하나만 바뀌어 "맞췄는데 잘린다" 거나 "200자 제한이라고 적혀 있는데 실제는 아니다" 가 되는 상태를 만들 수 없다. 번역문에 숫자가 다시 박히는 것은 `tests/toast_message_fits_cap.rs` 가 막는다.

> **길이를 모르는 조각(경로·실패 사유)을 실을 때는 `push` 전에 미리 줄인다** — `tasty_i18n::t_fmt_fit` / `fit_fragment` 가 번역된 틀은 그대로 두고 **조각의 가운데만** 생략해 상한 안에 맞춘다. 호스트의 기본 잘림은 **꼬리를 버리므로**, 경로면 어느 파일인지가, 실패 사유면 OS 에러가, 문장이면 "어떻게 하라" 는 지시가 사라진다. 소비자: 언어팩 폴백 경고(`LoadReport::user_warning`), 설정 변경 결과(`toast.settings_not_applied`·`toast.settings_not_saved`), 스크린샷 단축키 결과(`toast.screenshot_*`, `attach.toast.mirror_capture_*` — [원격 스크린샷](../../features/remote-screenshot-clipboard/index.md#결과-안내-토스트)).

## 구조 — 그리기와 상태가 다른 크레이트에 있다

**그리기**는 `crates/tasty-ui-widgets/src/toast.rs` 가 소유한다. 본체와 갤러리 specimen 이
같은 레이아웃·색·그리기 함수를 사용한다. 입력 폭·테마·배율이 같아야 같은 화면이 된다.

- `ToastEntryView` / `ToastScopeView` / `ToastViewProps` — 그릴 준비가 끝난 입력. 시간도
  상태도 안 들어 있고 `alpha` 는 이미 계산돼 있다. `ToastEntryView::id`는 호출자가 정한 카드 식별자다.
- `draw_toast_scopes(painter, props)` — 스택 배치 + 카드 chrome. 그린 카드마다 `(id, 사각형)`을
  돌려주며 그리지 않은 카드(alpha 0, 위쪽 경계 밖)는 빠진다. `Context` 가 아니라
  `Painter` 를 받는 이유는 호출부마다 그릴 레이어가 다르기 때문이다: 본체는
  `Order::Tooltip` 레이어 painter 를, 갤러리는 무대 frame 의 painter 를 넘긴다.
- `toast_layout_card(ctx, theme, message, max_width)` — 본문 galley 와 카드 크기(폭 × 높이).
  폭 상한을 **어디서 얻는가**(스택은 `toast_max_width` 와 스코프 안쪽 폭 중 작은 값, 단일 카드는 `toast_max_width`)만 부르는 쪽이
  정하고, 줄바꿈 폭·패딩·accent 바·높이는 여기서 한 번만 정한다.
- `toast_card_colors(theme, kind, alpha)` — alpha 를 반영한 fill · border · accent · 글자 색.
  alpha 를 곱하는 순서가 여기 고정돼 있다 — 테마 색(straight)에 곱한 뒤 `Color32` 로 바꾼다.
  두 순서 모두 sRGB 바이트 공간의 곱이지만 `Color32` 로 바꾼 뒤 곱하면 반올림이 한 번 더 일어난다.
- `draw_toast_single_card(ui, theme, kind, message, alpha)` — 스택 없이 카드 한 장만
  그려야 하는 자리용(갤러리의 Toast · Toast stack specimen). 치수와 색은 위 두 함수에서 온다.
- `draw_toast_card` / `ToastCardColors` — 카드 chrome 만 그리는 하위 함수. 치수·색을
  부르는 쪽이 직접 채운다.
- `toast_accent_color(kind, theme)` — kind → 좌측 바 색.
- `toast_fade_alpha(age, lifetime, reduced_motion)` — 페이드 곡선. `Duration` 둘만 받아
  어떤 상태 타입도 안 본다.

**상태**는 `src/adapters/ui/toast.rs` 에 남는다 — host 를 봐야 하는 것들이다.

- `ToastKind` — Info / Success / Warning / Error. 정본은
  `crates/tasty-type-appearance/src/toast_kind.rs` 이고 여기는 재수출이다.
- `ToastScope` — 위 enum. 정본은 `crates/tasty-model/src/toast_kind.rs`.
- `ToastState` — id, message, kind, scope, spawned_at, lifetime.
- `ToastManager` — `push(message, kind, scope)` / `push_info(...)` / `draw(ctx, &LayoutContext, reduced_motion)`(hover 유지 + 만료 제거 + 렌더). 지난 프레임에 그린 `(id, 사각형)`을 보관해 hover 판정에 쓴다. `MainViewState::toasts` 로 통합, draw 는 popup draw 직후(= 위 레이어)에서.
- `compute_alpha` — `ToastState` 에서 곡선이 읽는 두 값을 꺼내는 어댑터. `ToastState` 가
  `ToastScope`를 포함해 `tasty-model`에 의존하므로 위젯 크레이트로 옮기지 않는다.
- `truncate_message` — 문자 수 제한. `push` 진입부라 그리기 경로가 아니다.

> **본문 글자는 페이드하지 않는다.** `Fonts::layout` 에 색을 명시하면 그 색이 galley 에
> 박히고 `Painter::galley` 의 fallback 은 `Color32::PLACEHOLDER` 구간에만 쓰이므로,
> 페이드는 카드 배경·보더·accent 바에만 적용한다. 갤러리도 같은 동작을 사용한다.

모든 토스트 문자열은 `t("toast.*")` 키 — `lang/{en,ko,ja,zh-Hans}.toml` 네 파일 동시 추가([i18n](../../dev-guide/i18n.md)).

## 관련

- [popup.md](popup.md) — 내부 팝업 시스템
- [banner.md](banner.md) — parent 상단 info+action 오버레이 (내용 적으면 Toast 권장)
- [포커스 정책](../policies/focus.md) · [사용자 통제와 데이터](../../principles/control-and-data.md#사용자에이전트원격-사용자) — 사용자/에이전트 행동 구분
- [테마 시스템 (운영 상세)](theme.md) — 카드 치수와 색 계산을 공용 구현으로 모은 이유

사용자 프리셋 적용의 journal 실패는 접수한 원 View에 `preset.toast.apply_failed` Error toast로 알린다. continuation의 View identity가 바뀌었거나 요청이 에이전트 origin 또는 replay이면 표시하지 않는다. 성공을 기다리는 동안 다른 View를 다시 골라 실패를 표시하지 않는다.
