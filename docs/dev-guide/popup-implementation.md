# Popup 구현 가이드

View 내부 가상 창은 모두 **`PopupManager` + `PopupDef` 시스템**으로 만든다. `egui::Window` 를 직접 쓰지 않는다. 용어(Window/Modal/Popup/Toast 구분)는 [concepts/ubiquitous-language](../concepts/ubiquitous-language.md), 시스템 설계는 [`design/systems/popup.md`](../design/systems/popup.md).

> **0단계 — gallery-first**: 새 팝업은 본체에 넣기 **전에** 갤러리에 먼저 만든다(디자인 수령 → 갤러리 specimen → 본체). 아래 3단계는 그 "본체 반영" 단계다. 절차·근거는 [gallery-first](gallery-first.md) · [테마 시스템 (운영 상세)](../design/systems/theme.md).

## 두 팝업 시스템 — host `PopupDef` vs plugin `[[contributes.popup]]`

tasty 에는 팝업을 만드는 경로가 **둘** 있다. 아래 문서 나머지(3단계·필드표 등)는 **host `PopupDef`** 경로 전용이다.

| | **host `PopupDef`** | **plugin `[[contributes.popup]]`** |
|---|---|---|
| 정의 위치 | `src/adapters/ui/popup/defs.rs::all_defs()` 정적 목록(`OnceLock`) | plugin 매니페스트 `tasty-plugin.toml` |
| 콘텐츠 렌더 | host 프로세스 egui (`draw_fn`) | **plugin 프로세스** egui → egui-mesh 로 tessellate, host 가 합성 ([egui-mesh 렌더 채널](egui-mesh-channel.md)) |
| 셸(scrim·border·이동·리사이즈·outside-click·Esc) | `PopupManager` | **host `PopupManager`** (동일 — 셸은 언제나 host 소유) |
| 여는 주체 | host — `UiIntent::OpenPopup(OpenRequest)` | host 가 `PluginManager::open_popup_instance(plugin_id, popup_id, context)` 로 인스턴스화. 트리거는 (a) 매니페스트 `trigger = { kind = "event", event_key }` 를 host 이벤트로 실행, 또는 (b) surface-kind capability(`convert_input_popup`) 로 host 가 직접 open ([파일 핸들러 (File handler)](../features/file-handler/index.md)) |
| 상태·입력 버퍼 | host `MainViewState.dialogs` | **plugin 프로세스** 내 인스턴스 상태(`instance_id` 키) |
| 스코프(가시성·경계) | 등록 때 `Window` + 여는 쪽이 `OpenRequest`의 범위로 정한다 | 매니페스트 `scope`(`window` 기본 / `surface`) + 여는 host 진입점이 대상 surface 바인딩 — 판정 함수는 host 와 같다([design/systems/popup.md](../design/systems/popup.md) §plugin popup 의 스코프) |

**선택 기준 — 콘텐츠의 소유자가 누구인가:**

- **host `PopupDef`**: 콘텐츠가 host 데이터/위젯(설정, 파일 핸들러 picker, convert 목록, 이름변경 등)이고 host 가 렌더에 필요한 모든 상태를 가진 경우. host 가 kind 이름을 몰라야 하는 정보(특정 plugin 의 도메인 데이터)는 담지 않는다.
- **plugin `[[contributes.popup]]`**: 콘텐츠가 **특정 plugin 의 도메인**(그 plugin 만 아는 데이터·검증·동작)인 경우. host 는 그 내용을 몰라야 한다 — host 는 plugin 이름/도메인으로 조건분기하지 않는다. 셸만 host 가 그려 준다.

**현재 plugin 팝업 예 (markdown, egui-mesh):**

- `large-file-confirm` — 대용량 파일 열기 확인. 크기 감지·확인 로직이 plugin in-process 소유(host 는 파일 크기를 stat 하지 않는다). plugin 이 `com.tasty.markdown.large_file_confirm` 이벤트를 발행하면 열린다.
- `file-open` — markdown 파일 경로 입력 폼(경로 필드 + 찾아보기 + 열기/취소). `scope = "surface"` 라 대상 surface 가 보일 때만 그 영역 안에 뜬다. host 가 surface-kind capability `convert_input_popup="file-open"` 를 보고 convert/open 진입점에서 직접 열거나 event trigger 로도 열린다. 찾아보기는 host 소유 file_picker popup(`file_picker.trigger`, [내부 팝업 시스템](../design/systems/popup.md))으로 위임하고 팝업을 띄운 surface 의 폴더에서 출발시킨다, 열기 확정 시 context 의 `surface_id` 유무로 제자리 변환(`markdown.navigate`)/새 탭(`file_handler.dispatch`) 분기. 새 탭 쪽은 `owner_popup_instance` 에 자기 popup 을 실어 사용자 조작임을 알린다 — 빠지면 host 가 제스처 미증명 plugin 요청(PluginUnverified)으로 받아 새 탭을 선택하지 않고, 매칭 핸들러가 없을 때만 fallback picker 를 연다([파일 핸들러 (File handler)](../features/file-handler/index.md)). 상세: [markdown surface](../surfaces/markdown/index.md).

plugin 팝업 제작 절차는 [plugin-development](plugin-development.md) · [egui-mesh-channel](egui-mesh-channel.md) 참조. 갤러리 specimen 은 host-side 미러로 유지한다(gallery-completeness — plugin egui-mesh 를 갤러리가 직접 렌더하지 않으므로 폼/토큰/구조만 정합).

## 왜 `egui::Window` 직접 사용 금지

- `PopupManager` 의 입력 계층(팝업 사각형으로 찾는 포인터 대상 `popup_owns_point`)을 우회 → 팝업 위를 클릭해도 뒤 surface 가 클릭을 받는다.
- z-order·드래그·스코프 경계 클램핑 같은 공통 동작이 빠진다.

(예외: `src/gfx/gpu/shell_setup.rs` 의 부팅 전 셸 셋업, `src/gfx/gpu/boot_error.rs` 의 부팅 실패 화면처럼 popup 시스템이 살아있기 전 단계만. 앱 내부 다이얼로그는 전부 PopupDef.)

## 팝업 추가 — 3단계

### 1. draw 함수 (`src/adapters/ui/...`)

```rust
use crate::state::MainViewState;
use crate::adapters::ui::popup::{PopupAction, PopupCx};

pub fn draw_my_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    cx: &mut PopupCx<'_>,              // 인스턴스 id·포커스·키 통로·인스턴스 데이터
) -> PopupAction {
    // Esc 는 관리자가 선언 `escape` 로 처리한다(아래 `escape` 행). 본문은 기본 조작을 키 통로로 읽는다.
    if cx.keys.consume(crate::adapters::ui::popup::keys::BasicKey::Confirm) {
        // None | Close | CloseBy(사유) | ReleaseFocus
        return PopupAction::Close;
    }
    // 콘텐츠 렌더...
    PopupAction::None
}
```

### 2. `PopupDef` 등록 (`src/adapters/ui/popup/defs.rs::all_defs()`)

`all_defs()` 의 `vec![...]` 에 한 항목 추가:

```rust
PopupDef {
    id: "my_popup",
    decl: PopupDecl {
        outside_click: PopupOutsideClick::Unfocus, // Close = 바깥 클릭에 닫힘
        sticky_focus: false,                       // 포커스 판정이 읽지 않는 선언 값
        chrome: PopupChrome::TitleBar,             // Headless = 타이틀바·닫기버튼 없이 콘텐츠만
        drag_handle: PopupDragHandle::TitleBar,
        resizable: false,
        min_size: None,
        covers_scope: false,                       // 기본 true(범위를 덮음). 범위에 붙은 메뉴·안내만 false
        shell: PopupShell::Raised,                 // 배경·테두리·제목선 토큰 묶음
        ..host_decl(popup_size(egui::vec2(280.0, 120.0)))
    },
    title_key: "my_popup.title",     // i18n 키 (t() 로 번역)
    title_fn: None,                  // 동적 제목이면 Some(fn(&MainViewState, &EngineRead, &PopupPayload) -> String)
    sizer: None,                     // 동적 크기면 Some(fn(&MainViewState, &EngineRead, &PopupPayload) -> Vec2)
    drag_region: None,               // drag_handle 이 Region 이면 Some(영역 fn)
    fullscreen_stage: None,          // Some(stage_id) = 타이틀바에 전체화면 버튼 노출 (아래 참고)
    draw_fn: super::my_popup::draw_my_popup,
    on_open: None,                   // 새 인스턴스를 열기 직전에 할 일이 있으면 Some(fn)
    on_close: None,                  // 상태를 가지면 Some(정리 fn) (아래 "닫힘 정리")
}
```

<a id="3-팝업-열기--intent-큐로-발화"></a>

### 3. 팝업 열기 — Intent 큐로 요청

`state.popups.open*` 직접 호출 금지 — **Intent를 생성**한다 (origin 정책·디스패치 이유는 [`design/flows/action-dispatch.md`](../design/flows/action-dispatch.md)).

메인 창 안의 host 팝업은 모두 `OpenRequest` 하나로 연다. 사용자·에이전트 요청은 `UiIntent`로 보내고, 닫기 훅과 시스템 이벤트가 큐의 다음 항목이나 진행 팝업을 바로 열 때도 `PopupManager::open_request`에 같은 요청을 넘긴다. 예외는 별도 winit 창이 가진 팝업 관리자다. 설정 창의 키 충돌 확인(`src/view/settings/ui.rs`)과 파일 선택 창(`src/view/file_chooser/window.rs`)은 자기 관리자의 `open_centered_focused`로 연다. 이 경로는 `OpenRequest`·`UiIntent`의 요청 출처 처리를 거치지 않는다. 인스턴스 정책은 메인 창과 같이 `open_slot`이 선언의 `instances`로 판정한다. 별도 창의 관리자를 메인 창 관리자와 합치기 전까지 이 예외가 남는다.

```rust
use crate::adapters::ui::popup::PopupScope;
use crate::intent::{OpenRequest, UiIntent};
state.dispatch_intent(
    UiIntent::OpenPopup(OpenRequest::new("my_popup", PopupScope::Window).centered().focused())
        .from_user_menu("my_button"),
);
```

`OpenRequest::new(id, scope)`가 열기 요청 하나를 만든다. 위치(`placement`)는 기본이 `Keep`(직전 위치 또는 기본 배치)이고 `.centered()`(범위 가운데)·`.at_top()`(범위 위쪽)·`.at(pos)`(그 점)로 바꾼다. `.focused()`는 열 때 키보드 포커스를 준다. 범위 없이는 요청을 만들 수 없으므로 범위를 빠뜨리면 컴파일되지 않고, 닫았다 다시 열 때 직전 범위가 남지 않는다. 범위는 팝업이 작용하는 대상이 표시·조작되는 자리에서 고른다. 요소 자체를 다루는 팝업(이름 바꾸기, 닫기·분리 확인, 프리셋 적용)은 그 요소가 보이는 상위 컨테이너, 요소 내용 안의 팝업은 그 요소 범위다. 대상이 없으면 `PopupScope::Window`를 넣지 말고 열지 않는다([design/systems/popup.md](../design/systems/popup.md)의 작용 대상 범위 규칙). 검사 한계: 컴파일러는 범위 인자가 있는지만 확인한다. 상위 컨테이너가 창이 아닌 팝업에 `Window`를 넣는 실수는 잡지 못하며, 범위가 맞는지는 각 열기 지점의 단위 시험이 확인한다. 요청의 origin(`from_user_*` / `from_agent_*`)이 사용자가 아니면 `focused()`여도 포커스를 주지 않는다([popup 설계](../design/systems/popup.md)의 passive 팝업). 선언의 인스턴스 정책이 이 요청에 쓸 인스턴스가 이미 열려 있으면 OpenPopup 은 그 인스턴스의 위치·포커스·데이터를 바꾸지 않는다(`Single`은 같은 id 의 아무 인스턴스, `PerScopeTarget`·`Queue`는 같은 범위의 인스턴스, `Multi`는 늘 새 인스턴스). `Queue`면 요청 데이터의 항목을 그 인스턴스 큐 뒤에 붙인다. 인스턴스 데이터는 `.with_payload(...)`로 싣는다. 새 요청이 열린 팝업의 내용과 대상을 바꾸는 열기 지점(다운로드 후 실행 묻기, 원격 닫기·강제 분리 확인, 이름 바꾸기, 탭·pane 프리셋 적용, surface 변환, 파일 처리기 선택)은 `UiIntent::ReopenPopup`을 보낸다. 쓸 인스턴스의 범위가 요청 범위와 다르면 닫고 새 범위로 다시 연다. 이때 on_close 훅은 부르지 않는다(새 요청이 이미 채운 상태를 지우지 않게). 범위가 같으면 위치·포커스는 그대로 두고 인스턴스 데이터만 요청 값으로 바꾼다. 범위를 열린 팝업에 덮어쓰는 경로는 두지 않는다.

## `PopupDef` 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `id` | `PopupId`(`&'static str`) | 고유 식별자 |
| `decl` | `PopupDecl` | 공통 선언. 팝업 사이 동작 차이는 이 값으로만 정한다(아래 "`PopupDecl` 속성") |
| `title_key` | `&'static str` | i18n 키 → 타이틀바 |
| `title_fn` | `Option<fn(&MainViewState, &EngineRead, &PopupPayload) -> String>` | 동적 제목. 설정 시 `title_key` 대신 매 프레임 인스턴스마다 호출하며 그 인스턴스의 데이터를 받는다. 길이 걱정 없이 원본 문자열 반환 — 폭 초과 시 elide 는 `draw.rs`가 공통 처리(아래 "타이틀 길이 처리" 참고) |
| `sizer` | `Option<fn(&MainViewState, &EngineRead, &PopupPayload) -> Vec2>` | 동적 크기. **매 프레임 인스턴스마다 호출된다**(그 인스턴스의 데이터를 받는다) — 아래 "sizer 는 매 프레임 돈다" 참조. **`ui_scale_factor()` 곱 금지** — sizing 토큰에 host UI zoom 이 이미 baked. 추가 곱은 이중 곱셈으로 medium/large 에서 layout 붕괴. **사용자가 직접 리사이즈한 팝업(`resizable`)에서는 리사이즈 이후 sizer 가 크기를 덮어쓰지 않는다**(`size_user_overridden` 가드 — popup close 시 리셋되어 다음 open 에 복원) |
| `drag_region` | `Option<fn(&PopupState) -> Rect>` | `decl.drag_handle` 이 `Region` 일 때 팝업이 pos/size 로부터 계산하는 전용 핸들 띠. 타이틀바 없는 팝업도 이동할 수 있다 |
| `fullscreen_stage` | `Option<StageId>` | `Some(id)` 면 타이틀바 X 왼쪽에 전체화면 버튼이 붙고, 누르면 그 [무대](../design/systems/fullscreen-stage.md)가 뜬다. 노출 여부와 대상이 한 필드라 "버튼은 있는데 갈 곳이 없는" 상태가 생기지 않는다. 아래 "전체화면 버튼" 참고 |
| `draw_fn` | `fn(&mut Ui, &mut MainViewState, &EngineRead, &mut PopupCx) -> PopupAction` | 매 프레임 인스턴스마다 렌더. `PopupCx`는 선언 id, 인스턴스 id, 그리기 시작 때의 포커스, 관리자가 넘긴 키 통로(`keys`, 본문 키 게이트는 `keys.receives()`), 인스턴스 데이터를 준다. 본문은 키를 이 통로로만 읽는다(아래 "본문의 키") |
| `on_open` | `Option<fn(&mut MainViewState)>` | 새 인스턴스를 열기 직전에 한 번 부른다. 이미 열린 인스턴스에 온 요청(dedup)에서는 부르지 않는다. 튜토리얼 목록과 명령 팔레트가 쓴다 |
| `on_close` | `Option<fn(&egui::Context, &mut MainViewState, &EngineRead, &ClosedPopup)>` | 닫힘 뒷정리 훅. 모든 닫기 경로가 거치는 관리자의 인스턴스 닫기를 통해 어떤 경로로 닫히든 인스턴스마다 정확히 한 번 실행한다. `ClosedPopup`은 선언 id, 인스턴스 id, 닫기 사유, 닫히기 직전 범위, 인스턴스 데이터를 담는다(아래 "닫힘 정리" 참고). 큐의 다음 항목을 직접 여는 훅은 그 항목을 넣은 요청의 출처(`Queued.origin`)로 연다 |
| `debug_payload` | `Option<fn(&MainViewState, &CoreState, Option<u32>) -> PopupPayload>`(debug 빌드 전용) | `debug.host_popup.open` 이 사용자 클릭 없이 열 때 실을 인스턴스 데이터. 세 번째 인자는 `surface_scope` 로 열 때의 대상 surface 다. 변환 팝업만 채운다(그 surface 를 바꿀 대상으로). 핸들러는 팝업 id 로 갈라지지 않는다 |

### `PopupDecl` 속성

`PopupDecl` 은 `tasty-plugin-protocol` 의 `popup_decl` 모듈에 있는 직렬화 가능한 타입이다. host 정의는 `defs::host_decl(size)` 에서 시작해 팝업마다 다른 값을 적는다. `host_decl` 은 `PopupDecl::new` 의 기본값에서 `instances = Single` 만 다르다. 그래서 host 팝업은 기본으로 범위를 덮고, 범위에 붙은 메뉴·안내만 `covers_scope: false` 를 적는다. `PopupState::new`·`declared`는 범위를 인자로 받으며 기본 범위가 없다. 등록(`register_def`)은 `Window`를 넘기고, 열 때는 열기 함수가 범위를 다시 정한다.

`MainViewState`가 없는 창(설정 창, 플러그인·프리셋 창의 파일 선택)도 자기 `PopupManager`에 `PopupDef`를 등록한다. 설정 창의 단축키 충돌 확인은 `keybinding_conflict_def()`이고, 창의 파일 선택은 메인 창의 `file_picker` 정의를 그대로 등록해 그 인스턴스로 연다. 이런 정의의 `draw_fn`은 아무것도 그리지 않는 `drawn_by_view`이고, 창은 `PopupManager::draw_view_local`로 콘텐츠를 그린다. 이 함수는 그리기 함수에 그 인스턴스가 이번 패스 키 주인(창이 그 패스에 넣은 키 주인, 넣지 않았으면 포커스 인스턴스)인지 알리고, 콘텐츠는 `cx.keys` 로 키를 읽는다. 콘텐츠가 끝났다고 하면 그 인스턴스를 닫으며, 이 프레임에 닫힌 팝업 id를 돌려준다. 정의의 `on_close` 훅은 `MainViewState`를 받으므로 이 경로에서는 부르지 않는다.

| 속성 | 값 | `PopupDecl::new` 기본값 | 지금 읽는 곳 |
|------|----|------|------|
| `scope_kind` | Window / Workspace / Pane / Tab / Surface | 인자 | 없음. 등록 범위는 `Window` 이고 실제 범위는 여는 쪽이 준다 |
| `scope_target` | Origin / Parent / Explicit | Origin | 없음 |
| `instances` | Single / PerScopeTarget / Multi / Queue | PerScopeTarget | host `PopupManager::instance_for`가 열기·다시 열기 요청이 쓸 인스턴스를 고를 때 읽는다 |
| `covers_scope` | bool | true | 덮는 노드인지(`PopupState::covers`, 창 위로 펼친 팝업도 덮는다). 덮으면 scrim 을 깔고(`PopupManager::draw`) 배치 범위의 포인터 입력을 모두 가진다(`popup::press`, [덮는 팝업과 포인터](../design/systems/popup.md#덮는-팝업과-포인터)) |
| `fit` | Collapse / Shrink | Collapse | 범위보다 클 때. `Collapse` 는 보기 버튼으로 접고 누르면 창 위로 펼친다(`popup::display`, [좁은 범위](../design/systems/popup.md#좁은-범위-접기와-펼침)). `Shrink` 는 범위에 맞춰 줄인다. `search_bar` 만 `Shrink` 다. 창 범위 팝업은 접지 않는다 |
| `anchor` | ScopeCenter / ScopeTop / AtPoint / Cursor / Column | ScopeCenter | `Column` 과 `ScopeTop` 만 읽는다. `Column` 인스턴스는 호출자가 `set_notice_column` 으로 정한 안내 열에 연 순서로 위에서부터 쌓이고(폭은 열의 폭, 높이는 직전 프레임의 크기), 그리기 결과의 `column_rect` 가 차지한 사각형의 합이다. `ScopeTop` 인 보이는 팝업의 사각형은 `reserved_top_bands` 가 다른 배치가 피할 띠로 돌려준다. 나머지 값의 위치는 여는 쪽 `OpenRequest` 의 `placement` 가 정한다. 범위 가운데가 아닌 곳에 붙는 6개(`tools_menu`·`search_bar`·`rail_category`·배너 더보기 메뉴·`modifier_hint`·`restore_notice`)만 다른 값을 적었다. 메인 창은 프레임마다 팝업을 그리기 전에 창 범위 배너와 `reserved_top_bands` 아래로 안내 열을 정하고, `column_rect` 를 attach 대기 칩이 이어 붙을 자리로 쓴다 |
| `scope_inset` | bool | true | 접힘 판정 경계에만 쓴다(`popup::display`). 배치 경계의 Surface 범위 안쪽 여백은 범위 종류로 정하고 원격 행 팝업만 범위 사각형을 그대로 쓴다. 원격 행 팝업은 선언 하나(`remote_row`)이고, 열 때 정하는 덮음 값(`OpenRequest::covers_scope`)으로 범위를 덮는 모양(마지막 화면 위)과 덮지 않는 모양을 고른다 |
| `focus_on_open` | bool | true | 없음. 포커스는 여는 쪽 `OpenRequest` 의 `focus` 와 요청 출처가 정한다. 값은 사용자가 여는 주 경로가 포커스를 주는지를 적었다. 포커스 없이 여는 `transfer_progress`·`download_run_progress`, 에이전트 요청으로만 열리는 `approval`, 원격 행 팝업 `remote_row` 가 false 다. `notifications` 는 true 지만 단축키로 열 때는 지금 포커스를 가져가지 않는다(사이드바 클릭과 열기 경로가 다르다) |
| `keyboard` | None / Keys / Text | Keys | 관리자는 `None` 인지만 읽는다. `None` 인 팝업은 열거나 눌러도 키보드 포커스를 받지 않고 다른 팝업의 포커스도 바꾸지 않으며, 맨 앞으로 올라오지 않고 다른 팝업 아래에 그린다. 키 주인(포커스 노드)도 되지 않는다. 본문 키 통로(`PopupCx.keys`)는 `None` 이면 키를 주지 않는다. `Keys`·`Text` 의 차이는 그 통로의 소비 집합(`popup::keys::consumes_key`)이고, 창의 키 경로도 같은 집합으로 그 팝업이 키를 받을지 정한다(`key_route::route_key`). 집합 밖 키는 팝업이 붙은 범위의 단축키로 오른다. `Text` 팝업의 명령 키 규칙은 `key_route::text_or_up`에 있다. 글자를 편집하는 입력칸이 있는 팝업이 `Text` 다: `rename`·`search_bar`·`command_palette`·`port_scanner`·`dag_list`·`approval`·`remote_tool`·`file_picker`(저장 모드 이름 칸, 창의 파일 선택도 이 정의의 인스턴스). 키 주인이 아닌 `Keys`·`Text` 인스턴스(영역과 그 자식 레이어)의 텍스트 입력칸이 egui 포커스를 가지면 관리자가 모든 인스턴스를 그린 뒤 그 포커스를 놓는다(`popup::draw::text_focus`). 이번 패스 누름으로 얻은 포커스, 텍스트 입력칸이 아닌 위젯(버튼), `None` 인스턴스(복원 안내의 안쪽 포커스)는 남긴다 |
| `escape` | Close / Unfocus / Ignore | Close | 창의 Esc 판정(`state::escape_gate`)과 팝업 층 중앙 단계(`popup::escape_step`)가 읽는다. 창은 수식키 없는 Esc 만 받는다(`tasty_key_match::is_window_escape`). `Close`·`Unfocus` 키 주인이면 창이 그 인스턴스 포커스를 풀고 그 Esc 를 본문에 넘긴다. 본문이 그 Esc 를 쓰지 않았으면 `Close` 는 그 패스 중앙 단계가 사유 `Escape` 로 닫고, `Unfocus` 는 포커스를 푼 것으로 끝난다. 썼다는 것은 본문이 키 통로로 Esc 를 소비했거나(`cx.keys.consume(BasicKey::Escape)`), 본문 결과가 `PopupAction::None` 이 아니거나, 그리기 전에 그 인스턴스의 자식 팝업(드롭다운 등)이 열려 있던 경우다(그 Esc 는 egui 가 자식을 닫는 데 쓴다). `Ignore` 는 창이 Esc 를 받아 끝내고 본문에 넘기지 않는다(포커스도 그대로). 창 위로 펼친 팝업은 선언과 무관하게 포커스를 풀지 않고 넘기며, 본문이 쓰지 않았으면 닫지 않고 다시 접는다. `Unfocus` 는 `approval` 과 원격 행 팝업 `remote_row`, `Ignore` 는 explorer 충돌 질문 `explorer_conflict` 다(Esc 로 답하지도 닫히지도 않는다). 하위 화면이 있는 팝업은 `Close` 이고 하위 화면에서는 본문이 Esc 를 소비해 하위 화면만 닫는다(`dag_list` 상세 보기는 목록으로, `remote_tool` 양식은 앞 화면으로). 별도 창(설정 창 등)에는 창의 Esc 판정이 없다. 그 창이 egui 에 넣은 Esc 누름마다 사건 시점 수식키를 표지로 남기고, 창이 패스마다 키 주인과 함께 관리자에 넣어 같은 중앙 단계를 쓴다(`escape_step::WindowPassKeys`) |
| `outside_click` | Close / Unfocus / Ignore | Unfocus | 바깥 클릭에 닫을지(`Close`) 포커스만 놓을지 |
| `sticky_focus` | bool | false | 없음. 포커스 노드 판정이 읽지 않는다. 바깥 누름은 늘 포커스를 옮긴다([popup 설계](../design/systems/popup.md#sticky_focus)) |
| `chrome` | TitleBar / Headless | TitleBar | 공통 타이틀바·닫기 버튼을 그릴지 |
| `drag_handle` | None / TitleBar / Region | TitleBar | 이동 영역. `Region` 이면 정의의 `drag_region` 함수를 쓴다 |
| `resizable` | bool | false | 끌어서 크기 조절(min_size·scope 경계 클램프, 엣지별 리사이즈 커서). 잡은 변은 포인터를 따라간다 |
| `resize_handle` | Edges / Grip { size } | Edges | 크기를 바꾸려고 잡는 곳. `Edges` 는 테두리 8방향, `Grip` 은 오른쪽 아래 `size` 정사각형만이고 그립은 콘텐츠가 그린다 |
| `min_size` | `Option<PopupSize>` | None | 리사이즈 최소 크기. 없으면 `size` |
| `size` | `PopupSize`(논리 px) | 인자 | 처음 크기(unzoomed baseline). `sizer` 가 없으면 등록 때 UI 배율을 곱한다 |
| `shell` | Raised / Panel / InfoPanel / Dialog / Menu / BannerMenu / SearchBar / Content | Raised | 배경·테두리·제목줄 아래 선 토큰 묶음(`draw::shell_colors`). `Content` 는 관리자가 셸(배경·테두리·그림자)을 그리지 않고 콘텐츠가 직접 그린다(`modifier_hint`) |
| `shadow` | Modal / Popover | Modal | 셸 그림자 토큰(`draw::shadow_token`) |
| `content_margin` | Default / None | Default | 콘텐츠 둘레 공통 여백(`space-xs`). 구역별 여백을 직접 두는 팝업은 `None` |

"없음" 인 속성은 선언 값과 실제 동작이 다를 수 있다. 그때 동작은 오른쪽 칸이 적은 코드가 정한다. `decl_table_tests.rs` 가 등록된 팝업 35개(정의 34개와 설정 창의 단축키 충돌 확인)의 scrim·그림자·여백·셸 색과 `keyboard`·`escape`·`outside_click`·`focus_on_open` 값을 표로 고정한다. 팝업 공용 코드(`popup.rs`, `popup/` 바로 아래의 공용 파일)에서 팝업 id 를 비교해 그리기 값을 정하면 `popup_attributes_are_declared` 가 실패한다.

### sizer 는 매 프레임 돈다 (열 때 한 번이 아니다)

`popup::frame::draw_popup_layer` 의 첫 루프가 등록된 모든 def 의 `sizer` 를 **프레임마다**
부르고 그 결과를 `PopupState.size` 에 넣는다. 그래서 "검색으로 목록이 줄면 카드도 줄어든다"
같은 **프레임마다 달라지는 크기**에 이 필드를 그대로 쓸 수 있다 — 별도 경로를 만들지 않는다
(`command_palette`·`tools_menu`·`rail_category` 가 그 형태다).

이 사실을 쓸 때 함께 지킬 것 둘:

- **draw 와 sizer 가 같은 식을 본다.** 높이를 구역별로 쪼갠 함수/상수를 양쪽이 **같이**
  읽게 하고, 한쪽에만 리터럴을 두지 않는다. 두 식이 갈리면 목록 마지막 행이 footer 밑으로
  밀리는데, 빌드도 테스트도 그대로 통과한다.
- **배율은 sizer 의 몫이다.** `sizer` 가 있는 popup 은 등록 시 `decl.size` 에 host UI zoom 이
  곱해지지 않는다(`PopupManager::register`). 그래서 폭이 콘텐츠를 안 따르더라도 sizer 가
  `zoomed_px` 로 직접 곱해 돌려줘야 한다. 높이 식도 같다 — `Theme` 값은 생성 때 배율을
  이미 탔고 파일 안 const 는 안 탔으므로, 한 식에서 섞으면 팝업 크기는 고정된 채 글자만
  커진다(docs/design/systems/theme.md). 고정 크기였을 때는 등록이 곱해 주던 몫이라 **sizer 로 바꾸는 순간
  조용히 사라진다.**

sizer 연결은 실제 팝업 크기로 확인한다. `defs.rs`에 함수를 등록한 뒤 한 프레임
(`draw_popups`)을 돌리고 `state.popups.get(id).size` 를 읽는 테스트를 둔다. sizer 가 돌려준
반환값 자체만 비교하지 말고 sizer 반환값을 바꿨을 때 실제 크기를 검사하는 테스트가 실패하는지로
확인한다(`command_palette.rs::sizer_wiring_tests`).

### scrim 의 범위

scrim 이 덮는 rect 는 그 팝업의 `PopupScope` rect 다 — `Surface` 범위면 그 칸 하나, `Pane`·`Tab`
범위면 그 pane, `Window`·`Workspace` 범위면 창 전체다. 범위 rect 를 찾지 못하면 창 전체로
떨어진다(`PopupManager::scope_rect` 의 `None` → `screen_rect`)
([내부 팝업 시스템](../design/systems/popup.md)).
규칙 본문은 [design/systems/popup.md](../design/systems/popup.md) §scrim 의 범위. 새 팝업을
만들 때 손댈 자리는 둘이다.

- **scrim 을 깔 것인가** — 정의의 `decl.covers_scope` 로 선언한다. 범위로
  판정하지 않는다: `search_bar` 처럼 `Surface` 범위를 쓰면서도 anchored + scrim-less 인
  갈래가 있고([`tasty-ui-widgets` — 본체·갤러리 공유 UI primitive](../architecture/ui-widgets-crate.md)), 범위로
  판정하면 그 갈래가 조용히 뒤집힌다. `anchor` 를 범위 가운데가 아닌 값으로 선언한 팝업이
  scrim 을 깔거나 modal 그림자를 쓰면 `no_anchored_popup_takes_a_scrim` 이 잡는다. 이 선언은 host 팝업의 것이다 —
  plugin 팝업은 예외 없이 scrim 을 깐다(`src/plugin_bridge/popup_render.rs`).
- **부모-자식이면 자식은 안 깐다** — 한 경로(host 또는 plugin) 안에서는 같은 범위에 팝업이
  여럿이어도 scrim 은 한 번이다. `PopupManager::pick_scrim_layers` 가 두 경로에서 같은 판정을
  하지만 입력은 경로별로 갈린다 — host 는 host 팝업만, plugin 은 plugin 팝업만 모아 따로
  판정한다. 그래서 plugin 부모 + host 자식처럼 경로가 갈리면 자식이 `covers_scope = false` 인 것만이 이중
  scrim 을 막는다. 자식 팝업은 `covers_scope = true` 로 선언하지 마라.

경계를 지키는 것은 layer clip(`ctx.layer_painter(..).with_clip_rect(..)` 에 그 범위 rect 를
넘긴다)이고, **그 clip 은 매니저가 그리는 것만 덮는다** — scrim · 배경 · 프레임 · 그림자.
콘텐츠는 자기 `Area` 안에서 그려지고, 그 `Ui` 의 clip 은 egui 의 패널 컨테이너가 자기 rect 로
덮어쓴다(`TopBottomPanel::show_inside` · `CentralPanel::show_inside` 가 `set_clip_rect(panel_rect)`
를 부른다 — 부모 clip 과 교집합하지 않는다). 그래서 자기 자연폭보다 훨씬 좁은 칸에 묶이는 팝업을
새로 만들면 콘텐츠가 칸 밖으로 샐 수 있다. 재는 법은 픽셀 비교다 — 그 팝업을 좁은 칸에 띄우고
칸 바깥을 팝업 없는 같은 화면과 견줘 차이가 0 인지 본다.

매니저가 그리는 쪽은 `nothing_a_surface_scoped_popup_paints_lands_outside_its_surface`
(`src/adapters/ui/popup/scrim_scope_tests.rs`)가 확인한다. 프레임이 만든 도형마다 실제로 칠해지는
자리(clip ∩ 도형 경계)를 구해 칸 안인지 본다. **scrim 색 사각형만 세는 시험으로는 이것이 안
잡힌다**: clip 을 지우는 변이에서 scrim 색만 보던 시험은 차이를 놓쳤고 이 시험은 실패했다. 그리고
그 시험의 픽스처는 좁은 칸이어야 한다 — 넉넉한 칸에서는 그림자가 경계에 닿지 않아 clip 이
없어도 아무것도 안 샌다.

### 이동 / 리사이즈

- **이동**: `drag_handle` 으로 선언한 영역을 클릭+드래그 → 스코프 경계 안에서 위치 이동. 타이틀바 팝업은 `DragHandle::TitleBar`(기본). 타이틀바 없는 팝업은 `DragHandle::Region(fn)` 으로 pos/size 로부터 핸들 띠를 직접 계산해 선언한다.
  - **실측 헤더 rect 보고(헤더 전체 드래그)**: 헤더 높이가 host zoom·팝업별로 달라 정적 리터럴로 못 잡는 headless 패널(`port_scanner`·`remote_tool`)은, 뷰가 렌더 시점의 실제 헤더 rect(전체폭 × 실측 높이)를 `popup::report_header_drag_rect(ctx, popup_id, rect)` 로 매니저에 보고한다. hit-test(`effective_drag_handle_rect`)는 보고된 rect 를 `Region` 정적 띠보다 **우선** 사용해 헤더 전체를 이동 영역으로 만든다. 보고는 hit-test 보다 뒤(콘텐츠 렌더 시점)라 **직전 프레임** 값을 쓰며(1프레임 지연), open 첫 프레임엔 보고가 없어 정적 띠로 폴백한다. 또한 헤더 텍스트 라벨은 각 헤더 함수 최상단에서 `selectable_labels = false` 로 비선택 처리해 글자 위 드래그가 텍스트 선택으로 가로채지지 않게 한다.
  - **위젯 우선 중재(`is_using_pointer`)**: 이동/리사이즈의 *START 판정* 은 콘텐츠 렌더 **뒤** 에서 `ctx.is_using_pointer()` 게이트로 한다. 이번 프레임에 egui 위젯(버튼·입력)이 프레스를 가져갔으면 이동/리사이즈는 발동하지 않는다 → 핸들 띠가 위젯과 겹쳐도 **위젯이 우선**(입력 우선순위: 위젯 > 리사이즈 > 이동). 예외는 `resize_handle: Grip` 의 그립이다. 그립은 매니저가 그리는 위젯이라 그 위젯이 누름을 가진 경우에도 크기 조절이 시작되고, 그립 아래에 겹친 콘텐츠 위젯(스크롤바 등)보다 우선한다. 따라서 `Region` 은 헤더 띠 전체처럼 넓은 영역을 가리켜도 안전하다(예: `port_scanner` 가 좁은 폭에서 좌측 띠와 검색 입력이 겹쳐도 입력 클릭이 우선). 단 **close 버튼은 매니저가 직접 페인팅** 한 영역이라 egui 위젯이 아니므로 `is_using_pointer` 에 안 잡힌다 → close 는 콘텐츠 렌더 *전* 에 따로 hit-test 해 우선 처리한다.
- **리사이즈**: `resizable: true` 팝업은 테두리 밴드(약 6px)를 잡아 8방향으로 크기 조절(`resize_handle: Grip` 이면 오른쪽 아래 그립만). 사용자가 옮기거나 크기를 바꾼 뒤 놓으면 그 프레임의 그리기 결과 `released` 에 인스턴스와 사각형이 한 번 들어 있다. 우선순위는 **close 버튼 > 리사이즈 엣지 > 드래그 핸들 > 콘텐츠**.

### Outside-click / hover 히트테스트와 자식 오버레이(드롭다운)

`PopupManager::draw`의 outside-click/hover 판정(`hovered_popup`)은 기본적으로 각 popup 의 `popup_rect()`만 히트테스트한다. 하지만 `draw_fn` 내부에서 `egui::popup_below_widget`/`popup_above_or_below_widget` 같은 **egui 네이티브 API로 별도 생성되는 드롭다운 Area**는 `PopupManager`가 전혀 모르는 독립 레이어라, 팝업이 좁거나 화면 가장자리에 있어 드롭다운이 `popup_rect` 밖으로 삐져나가면 그 위 클릭이 "바깥 클릭"으로 오판되어 `close_on_outside_click` 팝업 전체가 드롭다운째 닫혀버린다.

- **해결**: 드롭다운을 그리는 뷰가 `popup::report_child_overlay_rect(ctx, popup_id, overlay_key, rect)`로 실측 rect를 매 프레임 보고한다(닫혀 있으면 반드시 `None` — stale rect 방지). egui popup 드롭다운은 열림 확인과 영역 조회를 함께 하는 `popup::report_dropdown_overlay(ui, popup_id, overlay_key, dropdown_id)`를 쓴다. `overlay_key`는 오버레이별 고유 문자열(보통 그 드롭다운의 egui popup id 문자열)로, 한 popup 에 오버레이가 여러 개(`port_scanner`의 state_filter + column_chooser)여도 report 호출 순서와 무관하게 서로 덮어쓰지 않는다.
- **hit-test 반영**: `PopupManager::draw`의 pre-content 판정은 `popup_rect().contains(pos)`가 실패하면 그 popup 소유의 등록된 오버레이 rect 들도 추가로 확인한다(`child_overlay_hit`). 여기 걸리면 `hovered_popup`이 그 popup으로 설정되어 outside-click 으로도 "바깥"으로 취급되지 않는다. 오버레이 rect 는 그리기 뒤 포인터 사각형(`PopupManager::pointer_boxes` → `PopupDrawResult.hit_rects` → `state.host_popup_hittest`)에도 들어가 창의 포인터 대상 판정(`popup_owns_point`)도 그 popup 으로 본다 — 드롭다운이 시각적으로 떠 있는 동안은 그 위 터미널 입력도 계속 차단되는 게 맞는 동작이다. close 버튼/리사이즈 엣지/드래그 핸들 판정은 `popup_rect` 자체에만 유효하므로 오버레이 hit 은 여기 관여하지 않는다.
- **1프레임 지연**: 보고는 hit-test보다 뒤(콘텐츠 렌더 시점)라 직전 프레임 값을 쓴다 — `report_header_drag_rect`와 동일한 트레이드오프.
- **적용 예**: `port_scanner.rs`의 `draw_state_filter`/`draw_column_chooser`, `remote_tool.rs`의 `draw_protocol_filter`. `remote_tool`은 부모 popup 이 `close_on_outside_click: false`라 증상 자체는 안 드러나지만 구조는 동일하게 맞춰져 있다.

## 타이틀 길이 처리 (elide)

타이틀바 텍스트가 길면 우측 상단 버튼군과 겹칠 수 있다. 이 겹침 방지는 **`popup/draw.rs`의 타이틀 렌더링이 모든 popup 공통으로 전담**한다 — 버튼군 왼쪽 경계(`title_buttons_left_x()`: 전체화면 버튼이 있으면 그 왼쪽 경계, 없으면 `close_btn_rect` 왼쪽 경계)을 제외한 실제 가용 폭(px)을 계산해 `egui::Fonts::layout_no_wrap`로 폭을 측정하고, 넘치면 `tasty_ui_widgets::elide_popup_title()`가 뒤를 `…`로 잘라 맞춘다(안전망으로 `painter.with_clip_rect`도 함께 적용). 갤러리도 같은 함수로 말줄임한다.

- **제목 글꼴은 `font-size-max`다** — 디자인 PopupTitleBar 의 크기를 모든 popup 에 쓴다. 본체 `draw.rs`와 갤러리 `popup_frame::draw_title_text`가 `tasty_ui_widgets::popup_title_font`를 함께 읽는다. 디자인의 semibold 는 egui UI에 굵은 글꼴을 등록하지 않아 재현하지 않는다.
- **제목은 버튼 수와 관계없이 스트립 가운데다** — `tasty_ui_widgets::popup_title_text_rect`가 계산하고 갤러리도 같은 함수를 쓴다. 본체는 `PopupState::title_text_rect()`로 띠를 구하며 `draw.rs`와 단위 테스트가 이 메서드를 함께 쓴다.
  - 오른쪽 예약 폭(가장자리 여백 + 버튼 칸 + 버튼 사이 간격 + 제목 간격)을 왼쪽에도 똑같이 비운다. 토큰으로는 `popup-title-edge-inset + N × popup-title-btn-size + (N − 1) × popup-title-btn-gap + popup-title-text-gap`이고, 배율 1에서 닫기 버튼만 있으면 32, 전체화면 버튼까지 있으면 60이다. 나란히 놓인 두 popup 의 제목이 같은 선에 맞는다.
  - 말줄임 폭은 스트립 폭 − 2 × 예약 폭이라, 폭이 좁은 popup 에서 긴 번역 제목이 더 일찍 잘린다. 제목이 이 띠를 넘어도 왼쪽 빈 공간으로 밀지 않는다.
  - 스트립이 좁아 양쪽 예약이 겹치면 버튼 앞 영역으로 대신해 제목이 버튼과 겹치지 않게 한다. 폭은 0 아래로 내려가지 않는다.
- **잘린 제목만 호버 Tooltip 으로 전체 문구를 보여 준다** — 말줄임이 일어난 제목의 띠 위에 포인터가 `tooltip-delay` 동안 머물면 `tasty_ui_widgets::show_popup_title_tooltip`이 공용 `Tooltip`을 띄운다. 위에 들어가면 위, 아니면 아래에 뜨고 둘 다 안 되면 위(창 안으로 당김)다. 잘리지 않은 제목, 버튼 위, 끌어 옮기는 중에는 뜨지 않는다. 제목을 보이려고 popup 을 넓히지 않는다. 갤러리 `popup_frame::title_tooltip`도 같은 함수를 쓴다.
- **버튼 칸은 IconButton sm 크기다** — `popup-title-btn-size`(24), 버튼 사이 `popup-title-btn-gap`(4), 오른쪽 끝에서 `popup-title-edge-inset`(4). 본체 `close_btn_rect`·`fullscreen_btn_rect`와 갤러리 `popup_frame::draw_title_buttons`가 같은 Theme 접근자를 읽는다.
- **버튼은 IconButton sm 규칙으로 그린다** — 닫기는 `icons::CLOSE`, 전체화면은 `icons::FIT` 글리프를 `icon_glyph_size_sm` 크기로 그린다. 타이틀바는 Ui 없이 painter로 그리고 hit-test를 매니저가 하므로, 판정 결과(호버, 호버 중 누름)를 `tasty_ui_widgets::paint_icon_button_state`에 넘겨 `IconButton` 위젯과 같은 배경(`icon-button-overlay-hover`, 누름 `icon-button-bg-active`)과 글리프 색(`icon-button-fg`, 호버 `icon-button-fg-hover`)을 얻는다. 글리프는 `tasty_ui_widgets::paint_popup_title_glyph`가 SVG 텍스처를 painter에 올린다. 이 버튼들은 키보드 포커스를 받지 않는다.
- **개별 popup 은 타이틀 문자열을 미리 축약하지 않는다.** `title_key`/`title_fn`은 원본 텍스트(전체 경로, 원본 문구 등)를 그대로 반환하면 된다 — 문자 수 기준 임의 축약(예: N자 초과 시 `.../parent/name`)을 타이틀 겹침 방지 목적으로 넣지 않는다. 폭 기준 elide 가 아닌 문자 수 기준 축약은 폰트/문자 폭이 다르면 여전히 겹치거나 불필요하게 짧아질 수 있다.
- **본문이 경로를 스스로 그리는 자리는 다르다 — 거기서는 앞에서 자른다.** 타이틀 겹침 방지가 아니라 **어느 쪽 끝이 정보인가**의 문제다. 경로와 reverse-DNS id 는 꼬리가 대상을 가르고 앞쪽이 반복되는 부분이라, 잘라야 하면 앞에서 자르고(`…/federation/screens.tsx`) 그 결과를 좌→우로 그린다. `direction: rtl` 류의 뒤집기는 런을 재배열해 정보가 있는 끝을 자른다. `file_handler_picker.rs` 의 `elide_target_front()`(헤더 경로) · `elide_id_front()`(행의 handler id)가 그 형태다.
- **본문(body) 텍스트는 별개**: 타이틀 밖의 본문 라벨(예: "대상: /긴/경로")은 이 elide 로직의 대상이 아니다. 본문이 popup 폭을 넘지 않게 하려면 각 popup 이 자체적으로 축약하거나 `ui.available_width()` 기준 elide를 적용한다(`transfer.rs`의 `elide_mono()` 참고).
- **새 동적 타이틀(`title_fn`) 추가 시**: 타이틀 길이를 걱정할 필요 없이 원본 문자열을 그대로 반환하면 된다. 다만 극단적으로 긴 문자열이 항상 몇 글자만 보이는 게 UX 상 문제라면(예: 뒷부분이 더 중요한 경로), `title_fn` 쪽에서 표시 우선순위를 조정한 축약 문자열을 넘기는 것은 여전히 가능하다 — 이때도 최종 겹침 방지는 `draw.rs`가 다시 한번 보장한다.

## 전체화면 버튼

`fullscreen_stage: Some(<stage id>)` 하나로 끝난다 — rect 계산·렌더·hit-test·커서·tooltip 은 `PopupManager` 가 공통 처리하고, 클릭은 `popup::frame::draw_popup_layer` 가 `MainViewState::open_fullscreen_stage` 로 넘긴다.

- **대상 무대는 먼저 존재해야 한다** — `fullscreen::defs::all_defs()` 에 같은 id 의 `StageDef` 를 등록한다(방법: [fullscreen-stage.md](../design/systems/fullscreen-stage.md)). 두 테이블의 정합은 단위 테스트가 강제한다(`popup_declared_stages_exist_and_are_not_headless`).
- **headless popup 에는 달 수 없다** — 타이틀바가 없어 버튼을 놓을 자리가 없다. 값이 `Some` 이어도 그려지지 않고, 같은 테스트가 그 조합을 금지한다.
- **원본 popup 은 닫히지 않는다** — 무대가 덮을 뿐이고 나오면 그대로 다시 보인다. 무대 콘텐츠는 이 popup 의 인스턴스가 아니라 **같은 형태로 그린 별도 콘텐츠**이므로, 무대에서 무엇을 하든 popup 상태에 반영되지 않는다.
- **글리프는 `fit` 아이콘이다** — 닫기 버튼 왼쪽 칸에 닫기 버튼과 같은 IconButton sm 규칙(위 "타이틀 길이 처리"의 버튼 항목)으로 `icons::FIT`을 그린다.
- **버튼을 달지 않은 popup 은 타이틀바가 변하지 않는다** — close 버튼 rect 는 전체화면 버튼 유무와 무관하게 타이틀바 우측 끝 고정이다. 제목은 버튼 수에 맞는 대칭 예약(위 "타이틀 길이 처리")을 따른다.

## 본문의 키

- 본문은 egui 입력을 직접 읽지 않고 키 통로 `cx.keys`(`popup::keys::PopupKeys`)로만 읽는다. 소스 가드 `source_guards::popup_body_key_gate::popup_layer_reads_no_keys_directly` 가 팝업 층 파일 출하 줄의 직접 읽기(`key_pressed(`·`consume_key(`·`Event::Key` 등)를 `파일:줄:형태` 로 막는다. 예외는 키 통로 파일 넷(`keys.rs`·`body_keys.rs`·`frame_keys.rs`·`window_input_test_support.rs`)과 plugin mesh 입력을 직렬화하는 `popup_render.rs::collect_mesh_popup_input` 하나다.
- 기본 조작은 `BasicKey`(Escape·Confirm·Expand·화살표·Tab·BackTab)로 읽는다. 바인딩은 설정에 노출하지 않는 고정 값(`enter`, `shift+tab` 등)이고 수식키까지 정확히 같아야 맞는다. 그래서 Ctrl+Enter 는 `Confirm` 이 아니다. `consume` 은 누름을 입력에서 지우고, `pressed` 는 지우지 않고 본다(포커스된 버튼·입력칸이 같은 Enter 를 써야 하는 충돌 카드, 전송 오류, 이름 바꾸기).
- 수식키가 붙은 조합은 `consume_key(수식키, 키)`(정확 일치, 소비 집합 안), 글자 단축키는 `consume_physical(후보)`(물리 키), egui 논리 일치를 쓰는 노드 키맵(파일 피커 폴더 이동, 검색 find)은 `consume_keymap_key`·`consume_keymap_binding` 으로 읽는다.
- 키 주인이 아닌 인스턴스와 선언 `keyboard = None` 은 모든 조회에서 빈 값을 받는다. 소비 집합 밖 누름도 빈 값이고 입력에 남는다. 창은 bare Esc가 아닌 경로 키 가운데 그 팝업이 쓰는 키만 egui에 싣고, 쓰지 않는 키는 붙은 범위와 그 위 level의 단축키로 보낸다([입력 계층의 키 입력 경로](../architecture/input-layer.md#키-입력-경로)). 덮는 팝업이 포커스면 덮인 범위 level의 단축키는 돌지 않는다.
- 공용 위젯은 키를 읽지 않고 호출자가 통로로 읽은 판정값을 받는다(충돌 카드의 Enter, 승인 선택지의 숫자, 접기 버튼의 `focused`). 팝업 층이 부르는, 키를 읽는 위젯은 예외 짝 셋뿐이고 같은 가드 모듈의 `a_popup_layer_calls_no_key_reading_widget` 이 그 목록을 고정한다.

## 텍스트 입력이 있는 팝업

```rust
let resp = ui.add_sized([width, 22.0], egui::TextEdit::singleline(buffer));
if cx.keys.receives() && !resp.has_focus() { resp.request_focus(); } // 키 주인일 때만 포커스 유지
if resp.gained_focus() { /* 첫 프레임 전체 선택 (TextEdit::load_state) */ }
if cx.keys.consume(BasicKey::Confirm) { /* apply */ }
```

**주의**: `lost_focus()` 만으로 닫지 않는다 — 팝업 내 다른 영역 클릭에도 TextEdit 는 포커스를 잃는다. **Enter/Escape 또는 명시적 버튼**으로만 닫는다.

**IME**: 텍스트 입력이 있는 팝업이라고 별도로 등록할 것은 없다 — IME 활성 여부는 `src/gfx/gpu.rs`의 `GpuState::apply_platform_output()`이 `egui::PlatformOutput::ime`(그 프레임에 IME가 필요한 위젯이 실제로 focus 중일 때만 `Some`)로 자동 판정한다. popup이 focus를 가졌더라도 그 안의 `TextEdit`이 실제로 focus되어 있으면 IME는 켜진 채로 유지되고, 텍스트 입력이 없는 화면(목록/네비게이션 등)에서는 꺼져 Escape/화살표 등 단축키가 physical_key로 매칭된다. 즉 popup 종류를 열거하는 예외 목록이 없으므로, 새 popup에 텍스트 입력을 추가해도 이 문서 밖에서 추가로 손댈 곳이 없다.

## 콘텐츠 레이어 — `egui::Area` 등록 (스크롤·클립)

팝업 콘텐츠(`draw_fn`)는 `PopupManager::draw`(`popup/draw.rs`)에서 **`egui::Area` 로 등록**되어 렌더된다. Area id = bg/title painter 와 동일한 layer_id(`Id("popup")+popup_id+z_idx`) → 한 레이어로 통합(z-order 자동 정합).

- **왜 Area 여야 하나**: egui 의 `Memory::layer_id_at` 은 **등록된 Area 만** 인식한다. 콘텐츠를 bare `Ui::new(layer_id)` 로 그리면 layer_id_at 이 팝업 레이어를 못 찾아 `ScrollArea::ui_contains_pointer()`=false → **휠/드래그 스크롤 입력이 무시**된다(위젯 클릭은 별도 widget hit-test 경로라 정상 → "클릭은 되는데 스크롤만 안 되는" 증상). 그래서 스크롤 가능한 콘텐츠(`ScrollArea`, `egui_extras::Table`)를 담는 팝업은 Area 등록이 필수다.
- **`movable(false)` + `sense(hover)`**: 드래그/클램핑/outside-click 은 `PopupManager` 가 **수동 좌표 hit-test**로 처리하므로 Area 가 클릭/드래그를 소비하지 않게 한다. egui Area 등록은 egui 내부 스크롤/호버 라우팅 전용이고, 터미널 입력 차단(`popup_owns_point`, geometry 기반)과는 독립이다.
- **`set_min_size`/`set_max_size`(content_rect) + `set_clip_rect`(content_rect) 필수**: Area 는 콘텐츠에 맞춰 auto-shrink 하므로, footer 처럼 `allocate_new_ui` 로 별도 배치되는 요소가 빠지면 Area hit-rect 가 줄어 layer_id_at 이 팝업 하단을 못 잡는다 → `set_min_size` 로 hit-rect 를 content_rect 전체로 강제. 또 `egui::Ui::new(max_rect(r))` 는 clip_rect=r 였지만 Area 는 기본 clip 이 더 넓어 콘텐츠 넘침(긴 라벨·선택 하이라이트·스크롤바)이 팝업 밖으로 샌다 → `set_clip_rect(content_rect)` 로 경계 클립 복원.

> 즉 popup `draw_fn` 안에서는 일반 egui 위젯/`ScrollArea`/`Table` 을 그냥 쓰면 된다 — 스크롤·클립·레이어 등록은 `PopupManager::draw` 가 콘텐츠를 감싼 Area 가 처리한다.

### `ScrollArea` 안에서 clip 을 좁힐 때 — `shrink_clip_rect`

`ScrollArea` 내부의 행이 자기 rect 로 clip 을 좁힐 때는 `Ui::shrink_clip_rect(rect)` 를
쓴다. **`set_clip_rect` 는 부모 clip 과의 교집합이 아니라 덮어쓰기**라, 행 rect(=스크롤
가상 콘텐츠 좌표)로 호출하면 `ScrollArea` 가 걸어둔 뷰포트 clip 이 사라져 뷰포트 밖으로
밀려난 행의 라벨·pill·상태 dot 이 리스트 경계 밖(팝업 바깥까지)에 그려진다. 목록이 짧아
스크롤이 생기지 않는 동안은 증상이 없어 늦게 발견된다.

- 적용 예: `src/adapters/ui/popup/remote_attach.rs` 의 프로필/워크스페이스 행 — 가로
  truncate 가드 목적으로 좁히되 뷰포트 clip 은 그대로 남는다.
- `ScrollArea` **를 감싸는** 컨테이너 `Ui` 에 컨테이너 rect 를 그대로 거는 것은 이 함정이
  아니다 — 그 rect 는 부모 clip 안이고, `ScrollArea` 가 그 안에서 자기 뷰포트로 다시
  좁힌다.

## 닫힘 정리

**새 팝업이 draft 버퍼/대상 id 같은 상태를 가지면 반드시 `PopupDef.on_close` 를 선언한다.** draw_fn 내부에서 Escape/버튼 클릭 시에만 정리하면 X 버튼·바깥 클릭·`UiIntent::ClosePopup`(디버그 IPC 포함)처럼 draw_fn 을 거치지 않는 닫힘 경로에서 정리가 새고, 재오픈 시 이전 상태가 그대로 보이거나(가벼운 경우) 진행 중 워커/네트워크 연결이 살아남는다(무거운 경우 — 예: `remote_attach`/`remote_tool` 의 ssh 터널). `on_close` 는 어떤 닫힘 경로로도 정확히 한 번 호출되는 유일한 지점이므로, 상태 정리는 draw_fn 안에 흩어놓지 말고 여기 모은다. 상태가 전혀 없거나(예: `notifications`) 남아도 무해하다고 **판단**했다면 `on_close: None` 옆에 근거를 한 줄 남긴다 — `src/adapters/ui/popup/defs.rs` 의 기존 항목들이 그 예시다. 닫힌 경로에 따라 정리를 달리해야 하면 draw_fn에서 경로를 나누지 말고 훅이 `ClosedPopup.reason`을 본다(예: `port_scanner`). 사유 표는 [design/systems/popup.md](../design/systems/popup.md#수명-계약-open--close--뒷정리)에 있다.

닫힘 큐에는 실제로 열려 있던 인스턴스만 추가한다. 훅이 다른 팝업을 닫으면 같은 처리 과정에서 그 훅도 실행한다. `ON_CLOSE_DRAIN_MAX_ROUNDS = 8`을 넘으면 경고를 남기고 해당 라운드의 훅은 실행하지 않는다. 정상 사용이 상한에 걸리는 경우에는 상한을 무조건 늘리지 말고 닫힘 관계를 확인한다.

## 관련

- [concepts/ubiquitous-language](../concepts/ubiquitous-language.md) — Window/Modal/Popup/Toast 구분
- [`design/systems/popup.md`](../design/systems/popup.md) — 팝업 시스템 전체 설계 (스코프·z-order·입력 계층)
- [architecture/input-layer](../architecture/input-layer.md) — 마우스 입력 계층/소비

## 플러그인이 호스트 팝업 결과를 기다릴 때

`<popup_id>.trigger`는 사용자 선택을 기다리지 않고 `{request_id}`를 돌려준다. 결과는 `<popup_id>.result` 이벤트로 요청 플러그인에게만 전송한다. 플러그인은 pending map에서 ID를 대조한다. 반대 방향인 호스트→플러그인 요청의 `PendingRequestKind`는 이 흐름에 쓰지 않는다.

파일 피커는 `file_picker.trigger`와 `file_picker.result`를 사용한다. 결과에는 `request_id`, `paths`, 취소 여부가 들어가며 취소 시 `paths`는 빈 배열이다. 피커 내부의 원격 디렉터리 조회 ID는 외부 trigger ID와 별개다. 요청자는 플러그인으로 확인되어야 하며, 기능은 GUI 빌드에 한정된다. CLI·에이전트 호출은 피커를 열 수 없다. 피커 범위는 `owner_popup_instance`의 결속 범위, 없으면 `origin_surface_id`의 surface 범위다. 둘 다 없거나 대상을 찾지 못하면 `-32602`로 거절한다([파일 피커](../features/native-file-picker/index.md)). owner의 결속 범위는 요청을 처리하기 직전에 plugin manager의 열린 인스턴스로 다시 계산하므로, owner를 연 뒤 첫 그리기 전에 온 요청도 거절하지 않는다.

호스트는 플러그인의 이름이나 요청 목적을 하드코딩하지 않는다. 로컬·원격 파일 조회는 기존 피커가 처리한다. 새 팝업에 같은 패턴을 적용할 때에는 중복 요청의 거부·대기 정책과 확정/취소 양쪽의 결과 전송을 함께 확인한다.

부모 팝업에서 열었다면 `owner_popup_instance`를 전달한다. 플러그인 팝업 닫힘은 `MainViewState.plugin_popup_closes` 큐를 거쳐 한 곳에서 처리해야 한다. 부모 종료로 자식을 취소한 뒤에는 자식의 부모 링크를 끊는다. 이미 확정된 결과는 취소로 덮지 않고 링크도 남겨, 결과 전송 시 부모가 없으면 경고를 기록한다. 경고 여부와 관계없이 결과 이벤트는 전송한다. 플러그인이 팝업 밖에서 결과를 기다릴 수도 있기 때문이다.

부모를 신고하지 않은 요청은 단독 팝업이다. Esc 는 키 주인 하나가 받고 그 인스턴스의 선언 `escape` 와 중앙 단계로 처리한다(위 `escape` 행). 부모·자식 사이에서 Esc 를 조상으로 올리는 중재는 없다.

팝업의 context와 host 범위 바인딩은 같은 원본 surface에서 만든다. 단일 인스턴스 제한으로 기존 팝업을 재사용하면 최초 context가 남으므로 범위도 덮어쓰지 않는다. 다른 surface에서 열기를 요청해도 기존 대상이 숨겨져 있으면 팝업이 보이지 않을 수 있다. host 팝업은 대상을 인스턴스 데이터로 싣고 선언의 `instances`를 `PerScopeTarget`으로 두면 범위마다 따로 열린다. 같은 범위에서 대상을 바꾸는 요청은 `UiIntent::ReopenPopup`으로 보낸다. 탐색기 Properties(`explorer_properties.rs`)·surface 변환(`convert.rs`)·파일 처리기 선택(`file_handler_picker.rs`)·파일 피커(`file_picker.rs`)가 이 방식이다. 검색 바(`search_bar.rs`)는 창에 하나(`Single`)지만 검색할 surface를 인스턴스 데이터(`PopupPayload::Search`)로 싣고, 다른 surface에서 연 요청은 `UiIntent::ReopenPopup`으로 범위와 대상을 함께 옮긴다. 닫힐 때 결과를 넘겨야 하는 팝업은 닫기 훅이 `ClosedPopup.payload`를 읽어 결과를 옮긴다. 파일 처리기 선택은 데이터 사본을 결과와 함께 `DialogState.finished_handler_pickers`에 넣고 App이 프레임 끝에 실행한다. 파일 피커는 범위·요청 ID·결과·요청자를 `DialogState.finished_file_pickers`에 넣고, App이 프레임 끝에 각 결과를 그 요청의 호출자에게 넘긴다.
