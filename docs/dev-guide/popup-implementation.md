# Popup 구현 가이드

View 내부 가상 창은 모두 **`PopupManager` + `PopupDef` 시스템**으로 만든다. `egui::Window` 를 직접 쓰지 않는다. 용어(Window/Modal/Popup/Toast 구분)는 [concepts/ubiquitous-language](../concepts/ubiquitous-language.md), 시스템 설계는 [`design/systems/popup.md`](../design/systems/popup.md).

> **0단계 — gallery-first**: 새 팝업은 본체에 넣기 **전에** 갤러리에 먼저 만든다(디자인 수령 → 갤러리 specimen → 본체). 아래 3단계는 그 "본체 반영" 단계다. 절차·근거는 [gallery-first](gallery-first.md) · [ADR-0510](../adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md).

## 두 팝업 시스템 — host `PopupDef` vs plugin `[[contributes.popup]]`

tasty 에는 팝업을 만드는 경로가 **둘** 있다. 아래 문서 나머지(3단계·필드표 등)는 **host `PopupDef`** 경로 전용이다.

| | **host `PopupDef`** | **plugin `[[contributes.popup]]`** |
|---|---|---|
| 정의 위치 | `src/adapters/ui/popup/defs.rs::all_defs()` 정적 목록(`OnceLock`) | plugin 매니페스트 `tasty-plugin.toml` |
| 콘텐츠 렌더 | host 프로세스 egui (`draw_fn`) | **plugin 프로세스** egui → egui-mesh 로 tessellate, host 가 합성 ([ADR-0028](../adr/0028-plugin-egui-mesh-render-channel.md)) |
| 셸(scrim·border·이동·리사이즈·outside-click·Esc) | `PopupManager` | **host `PopupManager`** (동일 — 셸은 언제나 host 소유) |
| 여는 주체 | host — `UiIntent::OpenPopup { id }` | host 가 `PluginManager::open_popup_instance(plugin_id, popup_id, context)` 로 인스턴스화. 트리거는 (a) 매니페스트 `trigger = { kind = "event", event_key }` 를 host event 발행이 발화, 또는 (b) surface-kind capability(`convert_input_popup`) 로 host 가 직접 open ([ADR-0043](../adr/0043-convert-input-popup-capability.md)) |
| 상태·입력 버퍼 | host `AppState.dialogs` | **plugin 프로세스** 내 인스턴스 상태(`instance_id` 키) |
| 스코프(가시성·경계) | `PopupDef.default_scope` + 여는 쪽 `OpenPopupMode::WithScope` | 매니페스트 `scope`(`window` 기본 / `surface`) + 여는 host 진입점이 대상 surface 바인딩 — 판정 함수는 host 와 같다([design/systems/popup.md](../design/systems/popup.md) §plugin popup 의 스코프) |

**선택 기준 — 콘텐츠의 소유자가 누구인가:**

- **host `PopupDef`**: 콘텐츠가 host 데이터/위젯(설정, 파일 핸들러 picker, convert 목록, 이름변경 등)이고 host 가 렌더에 필요한 모든 상태를 가진 경우. host 가 kind 이름을 몰라야 하는 정보(특정 plugin 의 도메인 데이터)는 담지 않는다.
- **plugin `[[contributes.popup]]`**: 콘텐츠가 **특정 plugin 의 도메인**(그 plugin 만 아는 데이터·검증·동작)인 경우. host 는 그 내용을 몰라야 한다(불가침 원칙 — host 는 plugin 이름/도메인으로 조건분기하지 않는다). 셸만 host 가 그려 준다.

**현재 plugin 팝업 (markdown, egui-mesh):**

- `large-file-confirm` — 대용량 파일 열기 확인. 크기 감지·확인 로직이 plugin in-process 소유(host 는 파일 크기를 stat 하지 않는다). plugin 이 `com.tasty.markdown.large_file_confirm` 이벤트를 발행하면 열린다.
- `file-open` — markdown 파일 경로 입력 폼(경로 필드 + 찾아보기 + 열기/취소). `scope = "surface"` 라 대상 surface 가 보일 때만 그 영역 안에 뜬다. host 가 surface-kind capability `convert_input_popup="file-open"` 를 보고 convert/open 진입점에서 직접 열거나 event trigger 로도 열린다. 찾아보기는 host 소유 file_picker popup(`file_picker.trigger`, [ADR-0058](../adr/0058-plugin-triggered-host-popup-async-ack-push.md))으로 위임하고 팝업을 띄운 surface 의 폴더에서 출발시킨다, 열기 확정 시 context 의 `surface_id` 유무로 제자리 변환(`markdown.navigate`)/새 탭(`file_handler.dispatch`) 분기. 새 탭 쪽은 `owner_popup_instance` 에 자기 popup 을 실어 사용자 조작임을 알린다 — 빠지면 host 가 에이전트 요청으로 받아 새 탭을 선택하지 않는다([ADR-0526](../adr/0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md)). 상세: [plugins/markdown](../plugins/markdown/index.md).

plugin 팝업 제작 절차는 [plugin-development](plugin-development.md) · [egui-mesh-channel](egui-mesh-channel.md) 참조. 갤러리 specimen 은 host-side 미러로 유지한다(gallery-completeness — plugin egui-mesh 를 갤러리가 직접 렌더하지 않으므로 폼/토큰/구조만 정합).

## 왜 `egui::Window` 직접 사용 금지

- `PopupManager` 의 입력 계층(`popup_hovered`)을 우회 → 팝업 위를 클릭해도 뒤 surface 가 클릭을 받는다.
- z-order·드래그·스코프 경계 클램핑 같은 공통 동작이 빠진다.

(예외: `src/gfx/gpu/shell_setup.rs` 의 부팅 전 셸 셋업, `src/gfx/gpu/boot_error.rs` 의 부팅 실패 화면처럼 popup 시스템이 살아있기 전 단계만. 앱 내부 다이얼로그는 전부 PopupDef.)

## 팝업 추가 — 3단계

### 1. draw 함수 (`src/adapters/ui/...`)

```rust
use crate::state::AppState;
use crate::adapters::ui::popup::PopupAction;

pub fn draw_my_popup(
    ui: &mut egui::Ui,
    state: &mut AppState,
    core: &mut crate::core::CoreState,
) -> PopupAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return PopupAction::Close;   // None | Close 둘뿐
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
    title_key: "my_popup.title",     // i18n 키 (t() 로 번역)
    title_fn: None,                  // 동적 제목이면 Some(fn(&AppState, &CoreState) -> String)
    default_size: egui::vec2(280.0, 120.0),
    sizer: None,                     // 동적 크기면 Some(fn(&AppState, &CoreState) -> Vec2)
    default_scope: PopupScope::Window,  // Window/Workspace/Pane/Tab/Surface
    close_on_outside_click: false,
    headless: false,                 // true = 타이틀바·닫기버튼 없이 콘텐츠만 (컨텍스트 메뉴 스타일)
    sticky_focus: false,             // true = 바깥 클릭해도 키보드 포커스 유지 (검색바 등)
    drag_handle: DragHandle::TitleBar,
    resizable: false,
    min_size: None,
    fullscreen_stage: None,          // Some(stage_id) = 타이틀바에 전체화면 버튼 노출 (아래 참고)
    draw_fn: super::my_popup::draw_my_popup,
    on_close: None,                  // 상태를 가지면 Some(정리 fn) (아래 "닫힘 정리")
}
```

### 3. 팝업 열기 — Intent 큐로 발화

`state.popups.open*` 직접 호출 금지 — **Intent 로 발화**한다 (origin 정책·디스패치 이유는 [`design/flows/action-dispatch.md`](../design/flows/action-dispatch.md)).

```rust
use crate::intent::{UiIntent, OpenPopupMode};
state.dispatch_intent(UiIntent::OpenPopup { id: "my_popup", mode: OpenPopupMode::CenteredFocused }.from_user_menu("my_button"));
```

`OpenPopupMode`: `Default` · `CenteredFocused` · `WithScope(scope)` · `AtTopOfScope(scope)` · `AtFocused(pos)`. 발화 origin(`from_user_*` / `from_agent_*`)에 맞는 mode 를 고른다. 같은 id 가 이미 열려 있으면 두 번째 OpenPopup 은 dedup 으로 무시된다.

## `PopupDef` 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `id` | `PopupId`(`&'static str`) | 고유 식별자 |
| `title_key` | `&'static str` | i18n 키 → 타이틀바 |
| `title_fn` | `Option<fn(&AppState, &CoreState) -> String>` | 동적 제목. 설정 시 `title_key` 대신 매 프레임 호출. 길이 걱정 없이 원본 문자열 반환 — 폭 초과 시 elide 는 `draw.rs`가 공통 처리(아래 "타이틀 길이 처리" 참고) |
| `default_size` | `egui::Vec2` | 기본 크기 (unzoomed baseline) |
| `sizer` | `Option<fn(&AppState, &CoreState) -> Vec2>` | 동적 크기. **매 프레임 호출된다** — 아래 "sizer 는 매 프레임 돈다" 참조. **`ui_scale_factor()` 곱 금지** — sizing 토큰에 host UI zoom 이 이미 baked. 추가 곱은 이중 곱셈으로 medium/large 에서 layout 붕괴. **사용자가 직접 리사이즈한 팝업(`resizable`)에서는 리사이즈 이후 sizer 가 크기를 덮어쓰지 않는다**(`size_user_overridden` 가드 — popup close 시 리셋되어 다음 open 에 복원) |
| `default_scope` | `PopupScope` | 가시성/경계 범위. `Surface` 범위면 경계가 그 칸 **안쪽 8pt** 이고 scrim 도 그 칸만 덮는다(아래 "scrim 의 범위") |
| `close_on_outside_click` | `bool` | 바깥 클릭 시 닫힘 |
| `headless` | `bool` | 타이틀바 없이 콘텐츠만 |
| `sticky_focus` | `bool` | 바깥 클릭해도 키보드 포커스 유지 |
| `drag_handle` | `DragHandle` | 이동(드래그) 핸들 선언. `None`(이동 불가) / `TitleBar`(타이틀바=핸들, 기존 동작; headless 면 핸들 없음) / `Region(fn(&PopupState)->Rect)`(팝업이 pos/size 로부터 **전용 핸들 띠** 계산 — 타이틀바 없는 팝업도 이동 가능). `movable` 여부는 별도 bool 없이 이 값으로 표현 |
| `resizable` | `bool` | true 면 테두리 8방향 드래그로 크기 조절(min_size·scope 경계 클램프, 엣지별 리사이즈 커서) |
| `min_size` | `Option<egui::Vec2>` | 리사이즈 최소 크기. `None`이면 `default_size`를 최소로 사용 |
| `fullscreen_stage` | `Option<StageId>` | `Some(id)` 면 타이틀바 X 왼쪽에 전체화면 버튼이 붙고, 누르면 그 [무대](../design/systems/fullscreen-stage.md)가 뜬다. 노출 여부와 대상이 한 필드라 "버튼은 있는데 갈 곳이 없는" 상태가 생기지 않는다. 아래 "전체화면 버튼" 참고 |
| `draw_fn` | `fn(&mut Ui, &mut AppState, &mut CoreState) -> PopupAction` | 매 프레임 렌더 |
| `on_close` | `Option<fn(&egui::Context, &mut AppState, &mut CoreState)>` | 닫힘 뒷정리 훅. `PopupManager::close()`(6개 close 경로 전부가 거치는 유일한 지점)를 통해 어떤 경로로 닫히든 정확히 한 번 발화(아래 "닫힘 정리" 참고) |

### sizer 는 매 프레임 돈다 (열 때 한 번이 아니다)

`popup::frame::draw_popup_layer` 의 첫 루프가 등록된 모든 def 의 `sizer` 를 **프레임마다**
부르고 그 결과를 `PopupState.size` 에 넣는다. 그래서 "검색으로 목록이 줄면 카드도 줄어든다"
같은 **프레임마다 달라지는 크기**에 이 필드를 그대로 쓸 수 있다 — 별도 경로를 만들지 않는다
(`command_palette`·`tools_menu`·`rail_category` 가 그 형태다).

이 사실을 쓸 때 함께 지킬 것 둘:

- **draw 와 sizer 가 같은 식을 본다.** 높이를 구역별로 쪼갠 함수/상수를 양쪽이 **같이**
  읽게 하고, 한쪽에만 리터럴을 두지 않는다. 두 식이 갈리면 목록 마지막 행이 footer 밑으로
  밀리는데, 빌드도 테스트도 그대로 통과한다.
- **배율은 sizer 의 몫이다.** `sizer` 가 있는 popup 은 등록 시 `default_size` 에 host UI zoom 이
  곱해지지 않는다(`PopupManager::register`). 그래서 폭이 콘텐츠를 안 따르더라도 sizer 가
  `zoomed_px` 로 직접 곱해 돌려줘야 한다. 높이 식도 같다 — `Theme` 값은 생성 때 배율을
  이미 탔고 파일 안 const 는 안 탔으므로, 한 식에서 섞으면 그릇만 고정되고 안의 글자가
  커진다(ADR-0126). 고정 크기였을 때는 등록이 곱해 주던 몫이라 **sizer 로 바꾸는 순간
  조용히 사라진다.**

**배선은 값으로 확인한다.** `defs.rs` 에 함수를 꽂아 두는 것은 배선이 아니다 — 한 프레임
(`draw_popups`)을 돌리고 `state.popups.get(id).size` 를 읽는 테스트를 둔다. sizer 가 돌려준
값을 그대로 비교하면 항등식이 되므로, 변이는 **sizer 쪽 반환값**을 흔들어 그 테스트가 죽는지로
확인한다(`command_palette.rs::sizer_wiring_tests`).

### scrim 의 범위

scrim 이 덮는 rect 는 그 팝업의 `PopupScope` rect 다 — `Surface` 범위면 그 칸 하나, `Pane`·`Tab`
범위면 그 pane, `Window`·`Workspace` 범위면 창 전체다. 범위 rect 를 찾지 못하면 창 전체로
떨어진다(`PopupManager::scope_rect` 의 `None` → `screen_rect`)
([ADR-0300](../adr/0300-the-scrim-covers-the-popups-scope-not-always-the-window.md)).
규칙 본문은 [design/systems/popup.md](../design/systems/popup.md) §scrim 의 범위. 새 팝업을
만들 때 손댈 자리는 둘이다.

- **scrim 을 깔 것인가** — `PopupManager::popup_has_scrim` 의 id 명부에 더한다. 범위로
  판정하지 않는다: `search_bar` 처럼 `Surface` 범위를 쓰면서도 anchored + scrim-less 인
  갈래가 있고([ADR-0254](../adr/0254-floating-surface-shadow-scope-rule.md)), 범위로
  판정하면 그 갈래가 조용히 뒤집힌다. anchored 명부(`ANCHORED_POPUPS`)와 이 명부가
  어긋나면 `no_anchored_popup_takes_a_scrim` 이 잡는다. 이 명부는 host 팝업의 것이다 —
  plugin 팝업은 명부 없이 예외 없이 scrim 을 깐다(`src/plugin_bridge/popup_render.rs`).
- **부모-자식이면 자식은 안 깐다** — 한 경로(host 또는 plugin) 안에서는 같은 범위에 팝업이
  여럿이어도 scrim 은 한 번이다. `PopupManager::pick_scrim_layers` 가 두 경로에서 같은 판정을
  하지만 입력은 경로별로 갈린다 — host 는 host 팝업만, plugin 은 plugin 팝업만 모아 따로
  판정한다. 그래서 plugin 부모 + host 자식처럼 경로가 갈리면 자식이 명부에 없는 것만이 이중
  scrim 을 막는다. 자식 팝업을 scrim 명부에 넣지 마라.

경계를 지키는 것은 layer clip(`ctx.layer_painter(..).with_clip_rect(..)` 에 그 범위 rect 를
넘긴다)이고, **그 clip 은 매니저가 그리는 것만 덮는다** — scrim · 배경 · 프레임 · 그림자.
콘텐츠는 자기 `Area` 안에서 그려지고, 그 `Ui` 의 clip 은 egui 의 패널 컨테이너가 자기 rect 로
덮어쓴다(`TopBottomPanel::show_inside` · `CentralPanel::show_inside` 가 `set_clip_rect(panel_rect)`
를 부른다 — 부모 clip 과 교집합하지 않는다). 그래서 자기 자연폭보다 훨씬 좁은 칸에 묶이는 팝업을
새로 만들면 콘텐츠가 칸 밖으로 샐 수 있다. 재는 법은 픽셀 비교다 — 그 팝업을 좁은 칸에 띄우고
칸 바깥을 팝업 없는 같은 화면과 견줘 차이가 0 인지 본다.

매니저가 그리는 쪽은 `nothing_a_surface_scoped_popup_paints_lands_outside_its_surface`
(`src/adapters/ui/popup/scrim_scope_tests.rs`)가 든다 — 프레임이 낸 도형마다 실제로 칠해지는
자리(clip ∩ 도형 경계)를 구해 칸 안인지 본다. **scrim 색 사각형만 세는 시험으로는 이것이 안
잡힌다**: clip 을 지우는 변이에서 그쪽 다섯 시험은 전부 살아남았고, 이 시험만 죽는다. 그리고
그 시험의 픽스처는 좁은 칸이어야 한다 — 넉넉한 칸에서는 그림자가 경계에 닿지 않아 clip 이
없어도 아무것도 안 샌다.

### 이동 / 리사이즈

- **이동**: `drag_handle` 으로 선언한 영역을 클릭+드래그 → 스코프 경계 안에서 위치 이동. 타이틀바 팝업은 `DragHandle::TitleBar`(기본). 타이틀바 없는 팝업은 `DragHandle::Region(fn)` 으로 pos/size 로부터 핸들 띠를 직접 계산해 선언한다.
  - **실측 헤더 rect 보고(헤더 전체 드래그)**: 헤더 높이가 host zoom·팝업별로 달라 정적 리터럴로 못 잡는 headless 패널(`port_scanner`·`remote_tool`)은, 뷰가 렌더 시점의 실제 헤더 rect(전체폭 × 실측 높이)를 `popup::report_header_drag_rect(ctx, popup_id, rect)` 로 매니저에 보고한다. hit-test(`effective_drag_handle_rect`)는 보고된 rect 를 `Region` 정적 띠보다 **우선** 사용해 헤더 전체를 이동 영역으로 만든다. 보고는 hit-test 보다 뒤(콘텐츠 렌더 시점)라 **직전 프레임** 값을 쓰며(1프레임 지연), open 첫 프레임엔 보고가 없어 정적 띠로 폴백한다. 또한 헤더 텍스트 라벨은 각 헤더 함수 최상단에서 `selectable_labels = false` 로 비선택 처리해 글자 위 드래그가 텍스트 선택으로 가로채지지 않게 한다.
  - **위젯 우선 중재(`is_using_pointer`)**: 이동/리사이즈의 *START 판정* 은 콘텐츠 렌더 **뒤** 에서 `ctx.is_using_pointer()` 게이트로 한다. 이번 프레임에 egui 위젯(버튼·입력)이 프레스를 가져갔으면 이동/리사이즈는 발동하지 않는다 → 핸들 띠가 위젯과 겹쳐도 **위젯이 항상 우선**(입력 우선순위: 위젯 > 리사이즈 > 이동). 따라서 `Region` 은 헤더 띠 전체처럼 넓은 영역을 가리켜도 안전하다(예: `port_scanner` 가 좁은 폭에서 좌측 띠와 검색 입력이 겹쳐도 입력 클릭이 우선). 단 **close 버튼은 매니저가 직접 페인팅** 한 영역이라 egui 위젯이 아니므로 `is_using_pointer` 에 안 잡힌다 → close 는 콘텐츠 렌더 *전* 에 따로 hit-test 해 우선 처리한다.
- **리사이즈**: `resizable: true` 팝업은 테두리 밴드(약 6px)를 잡아 8방향으로 크기 조절. 우선순위는 **close 버튼 > 리사이즈 엣지 > 드래그 핸들 > 콘텐츠**.

### Outside-click / hover 히트테스트와 자식 오버레이(드롭다운)

`PopupManager::draw`의 outside-click/hover 판정(`hovered_popup`)은 기본적으로 각 popup 의 `popup_rect()`만 히트테스트한다. 하지만 `draw_fn` 내부에서 `egui::popup_below_widget`/`popup_above_or_below_widget` 같은 **egui 네이티브 API로 별도 생성되는 드롭다운 Area**는 `PopupManager`가 전혀 모르는 독립 레이어라, 팝업이 좁거나 화면 가장자리에 있어 드롭다운이 `popup_rect` 밖으로 삐져나가면 그 위 클릭이 "바깥 클릭"으로 오판되어 `close_on_outside_click` 팝업 전체가 드롭다운째 닫혀버린다.

- **해결**: 드롭다운을 그리는 뷰가 `popup::report_child_overlay_rect(ctx, popup_id, overlay_key, rect)`로 실측 rect를 매 프레임 보고한다(닫혀 있으면 반드시 `None` — stale rect 방지). `overlay_key`는 오버레이별 고유 문자열(보통 그 드롭다운의 egui popup id 문자열)로, 한 popup 에 오버레이가 여러 개(`port_scanner`의 state_filter + column_chooser)여도 report 호출 순서와 무관하게 서로 덮어쓰지 않는다.
- **hit-test 반영**: `PopupManager::draw`의 pre-content 판정은 `popup_rect().contains(pos)`가 실패하면 그 popup 소유의 등록된 오버레이 rect 들도 추가로 확인한다(`child_overlay_hit`). 여기 걸리면 `hovered_popup`이 그 popup으로 설정되어 outside-click 으로도, hover 기반 입력 게이팅(`PopupDrawResult.hovered` → `state.popup_hovered`)에도 "바깥"으로 취급되지 않는다 — 드롭다운이 시각적으로 떠 있는 동안은 그 위 터미널 입력도 계속 차단되는 게 맞는 동작이다. close 버튼/리사이즈 엣지/드래그 핸들 판정은 `popup_rect` 자체에만 유효하므로 오버레이 hit 은 여기 관여하지 않는다.
- **1프레임 지연**: 보고는 hit-test보다 뒤(콘텐츠 렌더 시점)라 직전 프레임 값을 쓴다 — `report_header_drag_rect`와 동일한 트레이드오프(사실상 인지 불가).
- **적용 예**: `port_scanner.rs`의 `draw_state_filter`/`draw_column_chooser`, `remote_tool.rs`의 `draw_protocol_filter`. `remote_tool`은 부모 popup 이 `close_on_outside_click: false`라 증상 자체는 안 드러나지만 구조는 동일하게 맞춰져 있다.

## 타이틀 길이 처리 (elide)

타이틀바 텍스트가 길면 우측 상단 버튼군과 겹칠 수 있다. 이 겹침 방지는 **`popup/draw.rs`의 타이틀 렌더링이 모든 popup 공통으로 전담**한다 — 버튼군 좌변(`title_buttons_left_x()`: 전체화면 버튼이 있으면 그 좌변, 없으면 `close_btn_rect` 좌변)을 제외한 실제 가용 폭(px)을 계산해 `egui::Fonts::layout_no_wrap`로 폭을 측정하고, 넘치면 `elide_for_width()`가 뒤를 `…`로 잘라 맞춘다(안전망으로 `painter.with_clip_rect`도 함께 적용).

- **개별 popup 은 타이틀 문자열을 미리 축약하지 않는다.** `title_key`/`title_fn`은 원본 텍스트(전체 경로, 원본 문구 등)를 그대로 반환하면 된다 — 문자 수 기준 임의 축약(예: N자 초과 시 `.../parent/name`)을 타이틀 겹침 방지 목적으로 넣지 않는다. 폭 기준 elide 가 아닌 문자 수 기준 축약은 폰트/문자 폭이 다르면 여전히 겹치거나 불필요하게 짧아질 수 있다.
- **본문이 경로를 스스로 그리는 자리는 다르다 — 거기서는 앞에서 자른다.** 타이틀 겹침 방지가 아니라 **어느 쪽 끝이 정보인가**의 문제다. 경로와 reverse-DNS id 는 꼬리가 대상을 가르고 앞쪽이 반복되는 부분이라, 잘라야 하면 앞에서 자르고(`…/federation/screens.tsx`) 그 결과를 좌→우로 그린다. `direction: rtl` 류의 뒤집기는 런을 재배열해 정보가 있는 끝을 자른다. `file_handler_picker.rs` 의 `elide_target_front()`(헤더 경로) · `elide_id_front()`(행의 handler id)가 그 형태다.
- **본문(body) 텍스트는 별개**: 타이틀 밖의 본문 라벨(예: "대상: /긴/경로")은 이 elide 로직의 대상이 아니다. 본문이 popup 폭을 넘지 않게 하려면 각 popup 이 자체적으로 축약하거나 `ui.available_width()` 기준 elide를 적용한다(`transfer.rs`의 `elide_mono()` 참고).
- **새 동적 타이틀(`title_fn`) 추가 시**: 타이틀 길이를 걱정할 필요 없이 원본 문자열을 그대로 반환하면 된다. 다만 극단적으로 긴 문자열이 항상 몇 글자만 보이는 게 UX 상 문제라면(예: 뒷부분이 더 중요한 경로), `title_fn` 쪽에서 표시 우선순위를 조정한 축약 문자열을 넘기는 것은 여전히 가능하다 — 이때도 최종 겹침 방지는 `draw.rs`가 다시 한번 보장한다.

## 전체화면 버튼

`fullscreen_stage: Some(<stage id>)` 하나로 끝난다 — rect 계산·렌더·hit-test·커서·tooltip 은 `PopupManager` 가 공통 처리하고, 클릭은 `popup::frame::draw_popup_layer` 가 `AppState::open_fullscreen_stage` 로 넘긴다.

- **대상 무대는 먼저 존재해야 한다** — `fullscreen::defs::all_defs()` 에 같은 id 의 `StageDef` 를 등록한다(방법: [fullscreen-stage.md](../design/systems/fullscreen-stage.md)). 두 테이블의 정합은 단위 테스트가 강제한다(`popup_declared_stages_exist_and_are_not_headless`).
- **headless popup 에는 달 수 없다** — 타이틀바가 없어 버튼을 놓을 자리가 없다. 값이 `Some` 이어도 그려지지 않고, 같은 테스트가 그 조합을 금지한다.
- **원본 popup 은 닫히지 않는다** — 무대가 덮을 뿐이고 나오면 그대로 다시 보인다. 무대 콘텐츠는 이 popup 의 인스턴스가 아니라 **같은 형상의 별개 콘텐츠**이므로, 무대에서 무엇을 하든 popup 상태에 반영되지 않는다.
- **버튼을 달지 않은 popup 은 타이틀바가 변하지 않는다** — close 버튼 rect 는 전체화면 버튼 유무와 무관하게 타이틀바 우측 끝 고정이고, 제목 elide 기준도 버튼이 없으면 예전과 같은 `close_btn_rect` 좌변이다.

## 텍스트 입력이 있는 팝업

```rust
let resp = ui.add_sized([width, 22.0], egui::TextEdit::singleline(buffer));
if !resp.has_focus() { resp.request_focus(); }            // 포커스 자동 유지
if resp.gained_focus() { /* 첫 프레임 전체 선택 (TextEdit::load_state) */ }
if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { /* apply */ }
```

**주의**: `lost_focus()` 만으로 닫지 않는다 — 팝업 내 다른 영역 클릭에도 TextEdit 는 포커스를 잃는다. **Enter/Escape 또는 명시적 버튼**으로만 닫는다.

**IME**: 텍스트 입력이 있는 팝업이라고 별도로 등록할 것은 없다 — IME 활성 여부는 `src/gfx/gpu.rs`의 `GpuState::apply_platform_output()`이 `egui::PlatformOutput::ime`(그 프레임에 IME가 필요한 위젯이 실제로 focus 중일 때만 `Some`)로 자동 판정한다. popup이 focus를 가졌더라도 그 안의 `TextEdit`이 실제로 focus되어 있으면 IME는 켜진 채로 유지되고, 텍스트 입력이 없는 화면(목록/네비게이션 등)에서는 꺼져 Escape/화살표 등 단축키가 physical_key로 매칭된다. 즉 popup 종류를 열거하는 예외 목록이 없으므로, 새 popup에 텍스트 입력을 추가해도 이 문서 밖에서 추가로 손댈 곳이 없다.

## 콘텐츠 레이어 — `egui::Area` 등록 (스크롤·클립)

팝업 콘텐츠(`draw_fn`)는 `PopupManager::draw`(`popup/draw.rs`)에서 **`egui::Area` 로 등록**되어 렌더된다. Area id = bg/title painter 와 동일한 layer_id(`Id("popup")+popup_id+z_idx`) → 한 레이어로 통합(z-order 자동 정합).

- **왜 Area 여야 하나**: egui 의 `Memory::layer_id_at` 은 **등록된 Area 만** 인식한다. 콘텐츠를 bare `Ui::new(layer_id)` 로 그리면 layer_id_at 이 팝업 레이어를 못 찾아 `ScrollArea::ui_contains_pointer()`=false → **휠/드래그 스크롤 입력이 무시**된다(위젯 클릭은 별도 widget hit-test 경로라 정상 → "클릭은 되는데 스크롤만 안 되는" 증상). 그래서 스크롤 가능한 콘텐츠(`ScrollArea`, `egui_extras::Table`)를 담는 팝업은 Area 등록이 필수다.
- **`movable(false)` + `sense(hover)`**: 드래그/클램핑/outside-click 은 `PopupManager` 가 **수동 좌표 hit-test**(`popup_hovered`)로 처리하므로 Area 가 클릭/드래그를 소비하지 않게 한다. egui Area 등록은 egui 내부 스크롤/호버 라우팅 전용이고, 터미널 입력 차단(`popup_hovered`, geometry 기반)과는 독립이다.
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

**새 팝업이 draft 버퍼/대상 id 같은 상태를 가지면 반드시 `PopupDef.on_close` 를 선언한다.** draw_fn 내부에서 Escape/버튼 클릭 시에만 정리하면 X 버튼·바깥 클릭·`UiIntent::ClosePopup`(디버그 IPC 포함)처럼 draw_fn 을 거치지 않는 닫힘 경로에서 정리가 새고, 재오픈 시 이전 상태가 그대로 보이거나(가벼운 경우) 진행 중 워커/네트워크 연결이 살아남는다(무거운 경우 — 예: `remote_attach`/`remote_tool` 의 ssh 터널). `on_close` 는 어떤 닫힘 경로로도 정확히 한 번 호출되는 유일한 지점이므로, 상태 정리는 draw_fn 안에 흩어놓지 말고 여기 모은다. 상태가 전혀 없거나(예: `notifications`) 남아도 무해하다고 **판단**했다면(예: `mouse_capture_menu` 의 대상 id) `on_close: None` 옆에 근거를 한 줄 남긴다 — `src/adapters/ui/popup/defs.rs` 의 기존 항목들이 그 예시다.

## 관련

- [concepts/ubiquitous-language](../concepts/ubiquitous-language.md) — Window/Modal/Popup/Toast 구분
- [`design/systems/popup.md`](../design/systems/popup.md) — 팝업 시스템 전체 설계 (스코프·z-order·입력 계층)
- [architecture/input-layer](../architecture/input-layer.md) — 마우스 입력 계층/소비
