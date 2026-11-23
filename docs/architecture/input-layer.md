# 마우스 입력 계층 (Input Layer)

마우스 이벤트(클릭·이동·스크롤)는 화면에서 위에 그려진 요소부터 처리한다.
입력을 받은 요소가 커서 아이콘도 결정한다. 아래에는 비활성 surface를 활성화하는 첫 클릭 등
이 원칙의 구체적인 조건을 정리한다.

키보드 입력은 좌표가 아니라 포커스 노드에서 시작한다. 그 규칙은 아래 [키 입력 경로](#키-입력-경로)에 있다.

## 기본 동작: 소비 (Consume)

z-order 최상위부터 hit-test 한다. 좌표가 어떤 레이어 영역 안이면 그 레이어가 이벤트를 **소비**하고, 하위로 전달하지 않는다. 투과(pass-through)는 기본이 아니다.

## 입력 계층 순서

마우스 핸들러(`src/view/main/mouse.rs`)는 상위부터 차례로 가드한다:

| 순서 | 레이어 | 판정 | 동작 |
|------|--------|------|------|
| **0** | **전체화면 무대** | `state.fullscreen_stage_active()` | 활성이면 **1~7 전부를 좌표 판정 없이 무조건 차단**(아래) |
| 1 | 모달/오버레이 | `overlay_open` | 열려 있으면 터미널 입력 차단 |
| 2 | Popup | `MainView::pointer_on_popup()` = 직전 프레임에 놓인 팝업 노드 중 그 좌표의 맨 위 대상(`MainViewState::popup_pointer_target`·`popup_owns_point`: host 상자·범위로 자른 접힌 버튼·자식 드롭다운 오버레이, plugin 상자, 덮는 팝업의 덮는 사각형을 host·plugin 합친 z 순서로 본다) | 팝업 노드 위면 소비(터미널 무시). 누름도 이 판정으로 순서 2c·3 보다 먼저 가린다 |
| 2b | 포커스를 받지 않는 팝업(Modifier-hint·복원 안내) | 순서 2 와 같다 | 창 팝업 인스턴스라 Popup 과 같은 판정이다. 드래그/그립 크기 조절/X·안내 버튼 클릭이 하위 surface 포커스로 새지 않는다 |
| 2c | **Divider press** (좌클릭 press) | `!egui_consumed && !banner_hovered` 이고 `find_*_divider_at` | 포커스를 바꾸지 않고 드래그 시작 — 비활성 surface 쪽 hit 띠도 첫 press 부터(아래 참고) |
| 3 | **비활성 surface 전환** (좌클릭 press) | `surface_id_at_position != focused` | 첫 클릭은 surface 전환이 소비 — 그 위 배너/egui 위젯과 무관(아래 참고) |
| 4 | egui 위젯 (타이틀바·사이드바·탭바·오버레이) | `egui_consumed`, 좌클릭 press 는 `left_press_on_titlebar` 도 | egui 가 소비하면 종료. 직전 패스에 그린 타이틀바 패널 위 좌클릭 press 도 여기서 끝난다(아래 참고) |
| 5 | Banner | `state.banner_hovered` | 배너 위면 소비(터미널 무시) — **단 비활성 surface 전환에는 적용되지 않음** |
| 6 | Divider | `find_*_divider_at`(threshold) | 분할 경계 안이면 소비(드래그 시작) |
| 7 | Terminal/Surface | — | 최하위, 콘텐츠가 처리 |

`handle_cursor_moved` / `handle_mouse_input` / `handle_mouse_wheel` 이 모두 같은 가드(`egui_consumed || overlay_open || pointer_on_popup() || state.banner_hovered`)로 상위 레이어를 먼저 거른 뒤 divider → terminal 로 내려간다. `handle_mouse_input` 은 여기에 타이틀바 좌클릭 press 판정을 더한다. 배너는 **자기 영역의 마우스를 소비**(뒤로 전파 X)하는 focus-less 오버레이라 popup 과 같은 precedence 에서 차단한다(Toast 는 입력을 통과시키므로 이 가드에 없다 — [banner 시스템](../design/systems/banner.md)).

**순서 6 은 버튼 없는 hover motion 보고에도 적용된다.** DECSET 1003(AnyEventMouse)을 켠 앱에 커서 이동을 보고하기 전에 press 경로와 **같은 threshold**(`state::mouse::DIVIDER_HIT_THRESHOLD`)로 divider 밴드를 먼저 판정하고, 밴드 안이면 보고하지 않는다 — 밴드는 gap(1~2px)보다 넓어 양쪽 surface rect 안쪽까지 겹치므로, 이 가드가 없으면 "커서는 ↔ 인데 그 아래 TUI 는 hover 를 계속 받는" 불일치가 생긴다. OS 창 리사이즈 가장자리 밴드와 divider 드래그 진행 중에도 같은 이유로 보고하지 않는다. 대상은 focused surface 한정이다([터미널 (Terminal)](../surfaces/terminal/index.md)) — 순서 3(비활성 surface 전환)이 클릭에 대해 세운 "배경 캡쳐 TUI 로 마우스가 새지 않게" 원칙을 hover 에도 적용한 것이며, 다만 hover 는 포커스를 옮기지 않는다. **버튼을 누른 채 시작한 드래그 motion 은 이 가드들의 적용 대상이 아니다** — 대상 surface 가 press 시점에 고정되므로 밴드/이웃 surface 로 나가도 원래 surface 기준으로 계속 보고된다.

**분할선 hit 띠의 좌클릭 press 는 click-to-activate 보다 먼저 드래그를 시작한다(순서 2c).**
hit 띠는 분할선 양쪽 surface 안쪽까지 겹친다. 전환을 먼저 보면 비활성 surface 쪽 띠의 첫 press 가
전환에 쓰여 분할선을 잡으려면 두 번 눌러야 한다. native WebView 위 띠도 같은 규칙이다.
이 press 는 포커스·선택·마우스 리포트를 바꾸지 않는다. egui·배너가 받은 press(`egui_consumed`,
`banner_hovered`)와 modal·popup(modifier-hint 포함) 위 press 는 이 단계를 건너뛴다. 그래서 분할선 띠와
겹치는 이웃 pane 탭 바는 egui 가 그 press 를 받으면 탭 바가 우선한다.

**진행 중인 divider 드래그는 egui 의 이동 소비보다 우선한다.** `handle_cursor_moved` 는 드래그 중이면
통합 가드로 반환하지 않고 분할선 비율을 갱신한다. host 가 드래그를 시작한 press 를 egui 위젯도
클릭 후보로 잡으면(`is_using_pointer`) 이후 이동이 모두 `egui_consumed` 로 오기 때문이다.
배율 2 에서 html pane 아래 분할선을 잡을 때 이 경로로 분할선이 움직이지 않는 것을 관측했다.

**진행 중인 divider 드래그는 surface 콘텐츠보다 우선한다(순서 6 > 순서 7).**
egui-mesh surface는 포인터 이동과 버튼 이벤트를 `egui_mesh_target_at` 경로로 plugin에 전달한다.
이는 콘텐츠 처리이므로 `dragging_divider.is_some()`일 때는 건너뛴다.
`handle_cursor_moved`와 `handle_mouse_input`의 forward 조건에 모두
`dragging_divider.is_none()`이 필요하다. 이 조건이 없으면 커서가 mesh 영역에 들어갈 때
divider 갱신이 멈추고, release도 plugin이 소비해 드래그가 끝나지 않는다.
터미널과 explorer surface는 이 mesh 전달 경로를 사용하지 않는다.

**Modifier-hint**(modifier 홀드 시 뜨는 패널)는 키보드 포커스를 받지 않는 창(Window) 범위 팝업 인스턴스다(선언 `keyboard = None`). 그 위 마우스는 다른 팝업처럼 포인터 대상 판정(`popup_owns_point`)에 들어가 통합 가드(소비·휠·커서)를 거친다. 누름은 복원 안내와 같이 순서 2 의 팝업 노드 판정으로 click-to-activate·분할선 press 보다 먼저 가린다. 패널 위 좌클릭은 하위 surface 로 포커스를 옮기지 못하고(banner 와 차이), 드래그 이동·그립 크기 조절·X 클릭이 터미널 포커스·selection·마우스 리포트로 새지 않는다. 패널을 눌러도 키보드 포커스와 다른 팝업의 포커스는 바뀌지 않는다([포커스 정책](../design/policies/focus.md)).

### 순서 2 — 덮는 팝업은 그 범위의 포인터 입력을 모두 가진다

포인터 대상은 위에서부터 찾고, 덮는 팝업(선언 `covers_scope` 이거나 창 위로 펼친 팝업, [popup 시스템](../design/systems/popup.md#덮는-팝업과-포인터))을 만나면 그 배치 범위 아래로 내려가지 않는다. 그 범위 안에서는 팝업 상자 밖이어도 대상이 그 팝업이다. 그래서 덮인 범위 위의 누름·뗌·휠·hover·마우스 리포팅 이동 보고·커서 모양·링크 hover·링크 복사 키·분할선 잡기·click-to-activate·배너(host·plugin)는 모두 입력을 받지 않는다. 뗌은 누름이 간 곳을 따른다 — 팝업이 가져간 누름의 뗌은 그 뒤 팝업이 닫혀도 아래로 가지 않는다(`blocked_press_buttons`). 파일 놓기도 같은 판정이라 덮인 범위에만 놓을 수 없다.

winit 쪽은 직전 프레임에 그린 사각형으로 판정한다(`popup_owns_point`). egui 쪽은 덮는 팝업마다 덮는 사각형을 채우는 hover 감지 레이어(`popup::press::block_pointer`)를 그 팝업 레이어 바로 아래에 둔다. egui 의 hit test 는 포인터 둘레를 다 덮는 위젯이 있는 레이어 아래를 보지 않으므로 사이드바·탭 바·탐색기·상태바 위젯이 누름·hover·휠을 받지 않는다. 이 레이어는 끌기를 감지하지 않는다. 감지하면 팝업 상자 중 콘텐츠 밖(제목줄·테두리) 누름을 이 레이어가 가져가 덮는 팝업을 옮기거나 크기를 바꿀 수 없다. 두 쪽 모두 열린 다음 프레임부터 막는다.

### 순서 0 — 전체화면 무대는 좌표를 묻지 않는다

[전체화면 무대](../design/systems/fullscreen-stage.md)가 활성이면 그 위 어떤 레이어도 뒤에 있는 요소의 입력을 받지 못한다. 이 단이 표의 다른 단들과 다른 점은 **hit-test 가 없다**는 것이다 — popup(`popup_owns_point`)·배너(`banner_hovered`)는 "포인터가 그 위인가" 를 묻지만, 무대는 화면 전체를 덮으므로 물을 이유가 없고 뒤 위젯은 그려지지도 않은 상태라 그 좌표로 판정하는 것 자체가 그리지 않은 위젯에 입력을 전달하게 된다. "투과는 기본이 아니다" 원칙의 최상위 사례다.

판정은 `MainView::mouse_overlay_open()`(= `settings_open_requested || fullscreen_stage_active()`) 한 곳으로 모아, 통합 가드·click-to-activate press 가드·휠·커서 이동·OS 가장자리 리사이즈 양보·링크 hover 계산이 전부 같은 값을 보게 했다. 커서 아이콘도 무대 중에는 `winit_cursor_icon_at` 이 조기 반환한다(무대 프레임의 커서는 egui `platform_output` 이 정한다).

`settings_open_requested`는 설정 창이 열려 있는 동안 유지되는 값이 아니다.
egui 패스(`src/adapters/ui/draw.rs`)가 프레임 N에서 이 열기 요청을 설정하고,
다음 프레임의 `MainView::handle_redraw` 첫 단계인 `dispatch_pending_modal_opens`가 지운다.
따라서 버튼을 누른 뒤 실제 모달 창이 생기기 전까지의 입력을 막는다.
`MainViewState::keyboard_overlay_open`도 같은 값을 같은 용도로 사용한다.

설정 창은 별도 winit 창이며, 열린 뒤에는 메인 창이 입력을 받지 않는다.
모달의 지속적인 활성 상태가 필요한 호출자는 `view::View::is_modal_active()`를 쓴다.
`crates/tasty-doc-guards/tests/fullscreen_stage_render_gate.rs`는 위 호출 순서를 소스에서
검사한다. 순서가 뒤집히면 요청을 읽는 네 지점에서 값이 항상 false가 될 수 있다.
이 프레임 사이 동작을 런타임으로 측정하는 시험은 없다.

**키보드는 별도로 처리한다.** 마우스 계층과 달리 키보드에는 `handle_keyboard_input` 파이프라인의 **0단계 게이트**가 있다(double-tap 1~3단계보다 앞). 무대 중 ESC 는 그 자리에서 무대만 닫고 즉시 `return` 하므로 4단계(settings/notifications 닫기)에 도달하지 않는다 — "무대 종료 ESC 는 뒤로 전파되지 않는다" 는 입력 규칙이다. 입력 계약 전체(IME·진입 시 정리·OS 레벨 UI·모달과의 공존)는 [fullscreen-stage.md § 입력 계약](../design/systems/fullscreen-stage.md#입력-계약).

### 비활성 surface 클릭 = 전환 우선 (click-to-activate swallow)

**포커스되지 않은 surface 영역을 좌클릭하면, 그 위에 배너/egui 위젯이 있든 없든 그 첫 클릭은 "surface 전환" 이 통째로 소비한다** (macOS click-to-activate 모델). 이 단계는 통합 가드(`egui_consumed || … || banner_hovered`) **위**, modal/popup **아래**에 위치한다(`src/view/main/mouse.rs::handle_mouse_input`). 따라서:

- modal(`overlay_open`)·popup(`popup_owns_point`)은 surface 비소속 독립 상위 레이어라 전환보다 **먼저** 배제된다 — 팝업/모달을 클릭해도 뒤 surface 로 포커스가 넘어가지 않는다.
- Banner 소비(순서 5)는 *동작*(action 위젯·마우스 리포트 차단)에는 적용되나 *비활성 surface 포커스 전환*에는 적용되지 않는다 — 배너 카드를 클릭해도 소속 surface 로 포커스가 간다. 이로써 마우스-캡쳐 배너 같은 persistent 배너가 떠 있어도 surface 전환이 막히지 않는다.
- **트레이드오프**: 비활성 surface 첫 클릭은 전환에만 쓰이고 그 자리 selection/cursor/마우스 리포트로 흐르지 않는다(한 번 더 클릭). 배경 캡쳐 TUI 로 마우스가 새는 것을 막는다.
- 분할선 hit 띠 안의 press 는 이 단계 전에 드래그로 소비된다(순서 2c). 띠 밖만 전환 대상이다.
- gating 은 `surface_id_at_position(x,y) != focused_surface_id` — 이미 활성인 surface 안 클릭은 전환 단계를 건너뛰어 정상(selection/리포트/divider)으로 흐른다. egui 크롬(사이드바·탭바)은 surface rect 밖이라 `surface_id_at_position == None` → 전환 대상 아님 → `egui_consumed` 로 소비.

**타이틀바 좌클릭 press 는 사건 좌표로 가린다.** `egui_consumed` 는 egui 가 직전 패스에 받은 포인터 위치로 정한다(`wants_pointer_input`). 포인터 이동과 press 가 프레임 사이에 함께 오면 타이틀바 버튼 위 press 도 소비되지 않은 것으로 나와 터미널 클릭(선택, 클릭 커서 이동)이 돈다. egui 는 다음 패스에 같은 press 를 처리하므로 한 press 를 두 곳이 받는다. 그래서 타이틀바를 그릴 때 패널 배치 영역을 그 패스 배율로 물리 좌표에 옮겨 `MainViewState::titlebar_rect` 에 두고, 좌클릭 press 의 물리 사건 좌표가 그 안이며 지금 창 안이면 위에 남긴다(`left_press_on_titlebar`, 위·왼쪽 포함, 아래·오른쪽 제외). 짝 release 는 다른 위 press 와 같이 `blocked_press_buttons` 로 위에 남는다. 전체화면 무대처럼 타이틀바를 그리지 않는 패스는 그 영역을 지운다. 판정은 egui 판정을 다시 계산하지 않는 라우팅 규칙이다. 배율이 바뀌면(`Resized`·`ScaleFactorChanged` 처리에서 `sync_scale_factor` 가 바뀐 배율을 알리면) 옛 배율로 적은 영역을 지운다. 다음 패스가 새 배율로 다시 적을 때까지는 egui 판정만 쓴다. 이 판정보다 앞선 단계(메뉴 닫기, OS 테두리 리사이즈, 분할선 우선 press, click-to-activate)는 그대로 먼저 본다.

### 팝업 사각형 / `banner_hovered` 프레임 간 전달

`PopupManager::draw()`(`src/adapters/ui/popup/draw.rs`, modifier-hint 포함) / `BannerManager::draw()`(`src/adapters/ui/banner.rs`)는 렌더 시점에 호출되고 마우스 이벤트는 이벤트 시점에 처리돼 타이밍이 다르다. 팝업은 배치·clamp 를 마친 사각형(host 는 `PopupDrawResult.hit_rects`·`covers` → `state.host_popup_hittest`·`host_popup_covers`, `src/adapters/ui/popup/frame.rs`; plugin 은 `plugin_popup_hittest`·`plugin_popup_covers`, `src/plugin_bridge/popup_render.rs`)을 프레임 간 상태로 남기고, 마우스 핸들러는 이벤트 좌표로 `popup_owns_point` 를 물어 그 좌표의 맨 위 대상을 찾는다. 직전 위치의 hover 값을 쓰지 않으므로 덮이지 않은 곳으로 옮긴 직후의 누름이 이전 hover 로 막히지 않는다. 배너는 draw 결과(`BannerDrawResult.hovered`)를 `state.banner_hovered`(`src/adapters/ui/overlay.rs` 의 draw 패스에서 기록)에 저장해 참조한다. (상세 모델은 [popup 시스템](../design/systems/popup.md) · [banner 시스템](../design/systems/banner.md).)

## 커서 결정

이벤트를 소비한 레이어가 커서를 결정한다(`src/state/mouse.rs::winit_cursor_icon_at`).

| 레이어 | 커서 |
|--------|------|
| egui 위젯 | egui 결정(보통 Default/PointingHand) |
| Popup 타이틀바 | Grab(드래그 이동) |
| Popup 콘텐츠 | Default(또는 콘텐츠별) |
| Banner | Default(또는 내부 action 위젯별) |
| Divider (수직 분할) | ResizeHorizontal |
| Divider (수평 분할) | ResizeVertical |
| Terminal | Text |
| 그 외 surface | None(시스템 기본) |

## 구현 메커니즘 — 렌더링 z-order 가 위 정책과 어긋나지 않으려면

위 표는 **입력 소비** 순서다. 이 순서가 뜻대로 동작하려면 **렌더링** z-order 가 먼저 그 순서와 일치해야 한다(가려진 레이어가 입력을 가로채면 안 되므로). egui 는 CSS `z-index` 같은 임의 값이 없어, 이 절은 tasty 가 렌더링 z-order 를 그 값 없이 어떻게 강제하는지 기술한다.

### (a) `Order` — 5단 고정 tier

egui 는 `egui::Order` enum(`Background` / `Middle` / `Foreground` / `Tooltip` / `Debug`, egui 0.31.1 `layers.rs`)으로 그리기 순서를 5단으로 고정한다 — **enum 선언 순서 그대로**, 매 프레임 무조건 그 순서로 그린다. tasty의 Popup·Modifier-hint·Banner·탭바·상태바는 `Order::Foreground` 안에 있다. 이들의 상대 순서는 `Order`만으로 정할 수 없다. 사이드바의 SidePanel은 Background에 속한다. Divider/Terminal(6/7)은 다른 tier(`Middle` 이하)라 애초에 범위 밖이다.

### (b) 같은 tier 안의 순서 — `Areas::order` + `move_to_top`

같은 `Order` tier 안에서는 `Memory::Areas::order: Vec<LayerId>`(egui 내부, `memory/mod.rs`)가 실제 그리는 순서를 정한다 — 이 Vec 의 **뒤쪽일수록 나중에 그려져 위에 보인다**. 이 Vec 에 편입되려면 그 레이어가 **`egui::Area`로 등록**돼야 하고(`.show(ctx, ...)` 호출 시 내부적으로 `move_to_top` 또는 `set_state`가 `order.push`), 등록된 레이어끼리는 매 프레임 `end_pass()`가 `order.sort_by_key(|l| (l.order, wants_to_be_on_top.contains(l)))`(안정 정렬)로 재배치한다. `Context::move_to_top(layer_id)`(`ctx.move_to_top`)를 호출하면 그 프레임에 "맨 위로" 플래그(`wants_to_be_on_top`)가 서고, 이미 등록된 id 면 그 프레임의 정렬에서 플래그 안 선 다른 레이어들보다 뒤(위)로 간다.

**주의 — `move_to_top` 을 여러 레이어에 순서대로 반복 호출해도, 그 여러 레이어 "끼리의" 상대 순서는 바뀌지 않는다.** 안정 정렬은 플래그가 같은(모두 true) 레이어들을 **서로 tie** 로 묶어, 그 tie 안에서는 각 레이어가 세션 중 **최초로 등록된** 시점의 상대 순서를 그대로 유지한다. 즉 "4개 레이어에 A→B→C→D 순서로 `move_to_top` 을 매 프레임 호출하면 A<B<C<D 계층이 만들어진다"는 직관은 **egui 0.31.1 에서 성립하지 않는다** — 실제로 성립하려면 그 레이어들이 서로 다른 `wants_to_be_on_top` 상태(일부만 true)여야 한다.

### (c) 미등록 레이어 함정

`egui::Area`로 등록하지 않은 레이어는 `Areas::order`에 없다.
`ctx.layer_painter(layer_id)`로 직접 그리면 `GraphicLayers::drain()`이 같은 tier의
등록된 레이어를 모두 그린 뒤에 처리한다. 따라서 등록된 레이어보다 위에 보인다.
`Ui::new(...).layer_id(...)`의 호출 순서를 바꾸는 것만으로는 이 순서를 고칠 수 없다.

### (d) tasty 의 중앙 집중식 강제 — `enforce_foreground_z_order`

`src/gfx/gpu/egui_bridge.rs::enforce_foreground_z_order`(`run_egui_frame` 안 `ui::draw_popups` 직후, 매 프레임 1회 호출)가 Foreground tier 안 Banner/egui위젯(상태바·탭바)의 상대 순서를 강제한다. (b)의 한계 — `move_to_top` 반복 호출로는 계층을 못 만든다 — 때문에, 실제로는 `Context::set_sublayer(parent, child)`(`end_pass` 가 안정 정렬 *이후* child 를 parent 위치 바로 뒤로 splice — 등록 시점과 무관한 강제 인접 배치, egui 내부적으로 `Window` 안 하위 `Area` 배치에 쓰는 것과 같은 API)를 조합한다:

- `banner_layer` 를 부모로, `status_bar`/각 pane 의 `tab_bar` 를 자식으로 묶어 **Banner < {상태바, 탭바}** 를 등록 시점과 무관하게 고정한다.
- `set_sublayer` 는 **1단 들여쓰기만** 지원한다. parent 가 다른 sublayer 의 child 이면 동작이 egui 문서상 unspecified 다(실측: `Areas::end_pass` 가 `sublayers` HashMap 을 순회하는 순서에 좌우돼 비결정적으로 깨질 수 있음). 그래서 다른 그룹을 이 체인에 엮지 않는다.

**Modifier-hint < Popup** 은 팝업 관리자(`PopupManager::draw`)가 정한다. 포커스를 받지 않는 팝업(modifier-hint·복원 안내)을 그리기 순서 맨 앞에 두고 `restack_layers` 로 egui 순서를 맞춘다. egui 는 누른 영역을 맨 위로 올리므로, 포커스를 받지 않는 팝업을 누른 프레임에는 모든 팝업 레이어를 함께 올려 순서를 지킨다. modifier-hint 는 사용자가 modifier 를 처음 누르는 프레임에야 등록되므로 배너보다 늘 늦게 등록되고, 안정 정렬에서 배너 위에 남는다.

결과적으로 Banner(5) < {상태바·탭바}(4) < Modifier-hint(2b) < Popup(2) 이 재현된다. Modal(1)은 별도 OS 창이라 범위 밖, Divider/Terminal(6/7)은 다른 `Order` tier 라 범위 밖이다.

Popup 과 Modifier-hint 를 겹쳐 Popup 이 위에 보이는 것은 `the_hint_stays_below_other_popups` 가 egui 레이어 판정(`layer_id_at`)으로 확인한다.
Banner와 탭바를 겹친 비교에서도 `enforce_foreground_z_order`가 있을 때 탭 칩이 위에 보였다.
각 요소를 단독으로 표시하는 경우도 확인했다.

상태바는 탭바와 같은 `set_sublayer(banner_layer, ...)` 관계를 쓴다. 다만 Banner 카드가
zone 상단에 고정되고 `set_clip_rect(zone)`에 잘리므로 상태바와 직접 겹치는 배치는
만들지 못했다. 같은 구현을 쓴다는 근거와 실제 화면에서 확인한 범위를 구분한다.

### 레이어별 Order/Area 등록 현황 (조사 결과)

| # | 레이어 | `Order` | 등록과 처리 |
|---|--------|---------|------------|
| 1 | 모달/오버레이(`overlay_open`) | 해당 없음 | 별도 OS 창. rename 다이얼로그는 popup 경로(#2) |
| 2 | Popup | Foreground | `egui::Area` 등록 |
| 2 (예외) | `plugin_bridge/popup_render.rs` mesh popup | Foreground | 기본 미등록. host popup과 z-order를 비교할 때 조건부 등록(아래 절) |
| 2b | Modifier-hint·복원 안내 | Foreground | 팝업 관리자가 `egui::Area` 등록(포커스를 받지 않는 팝업) |
| 4 | 사이드바·탭바·상태바 | 혼재 | SidePanel은 Background 미등록. 탭바·상태바는 Foreground 등록 |
| 5 | Banner | Foreground | 등록됨. `enforce_foreground_z_order`가 탭바·상태바 아래로 배치 |
| 6 | Divider | Middle | raw painter. 표의 입력 순위는 좌표 판정으로 처리 |
| 7 | Terminal/Surface | 터미널은 egui Order 밖, 비터미널 egui 영역은 Background | egui 영역만 등록. native WebView는 별도 표시 경로 |

### Background tier(사이드바 SidePanel·Explorer·Markdown·Html)는?

Background 안에서는 현재 서로 겹치는 레이어를 검증할 배치가 없다. 사이드바는 고정
도킹되고 surface 콘텐츠는 각 pane/tab 영역에 그려진다. Foreground의 순서 제어가
Background에도 적용되는 것은 아니다. plugin이 자유 위치 오버레이를 추가하는 등
겹침이 생기면 해당 tier의 등록 순서와 실제 화면을 따로 확인한다.

### `plugin_bridge/popup_render.rs` — 의도적 예외

`draw_plugin_popups`의 mesh popup 셸(`Id::new("plugin_mesh_popup").with(instance_id)`)은
기본적으로 raw `layer_painter`를 쓴다. 전체 화면 scrim을 등록된 Foreground 위젯보다
위에 그리기 위해 Area에 등록하지 않는다. 위젯 트리 없이 `collect_mesh_popup_input`이
raw 이벤트를 plugin으로 전달하므로 host Area의 스크롤·hover 라우팅도 필요하지 않다.

이 popup을 Area에 등록하려면 상대 순서도 함께 정해야 한다. 등록만 하면 먼저 등록된
Banner나 상태바보다 아래에 그려질 수 있다.

더 나중에 열리거나 클릭된 host popup(`file_picker` 등)은 예외다.
`enforce_host_plugin_popup_z_order`(`src/gfx/gpu/egui_bridge.rs`)가 공유
`z_seq`(`tasty_host_plugin::next_popup_z_seq()`)를 비교하고 `ctx.set_sublayer()`로
host popup을 위에 배치한다. 이 호출은 parent와 child를 모두 `Areas::order`에
등록한다. host popup이 없거나 그 `z_seq`가 더 작으면 기본 미등록 경로를 쓴다.
자세한 규칙은 [popup.md의 Host ↔ Plugin popup z-order](../design/systems/popup.md#host--plugin-popup-z-order)를 따른다.

### `overlay_open` — 정의 3 개

"오버레이가 열려 있는가" 는 **하나의 판정이 아니다.** 이름과 모양이 비슷한 정의가 셋 있고,
용도가 달라 조건도 다르다. **새 상태를 추가할 때는 세 정의를 각각 확인한다.
키보드 판정은 호출하는 곳의 처리 순서도 확인해야 한다.**

| 정의 | 조합 | 묻는 질문 |
|------|------|-----------|
| `MainViewState::keyboard_overlay_open()` (`src/state.rs`, 순수 술어는 같은 파일 하단) | settings + 포커스 노드인 input dialog + focused host popup + 키 주인 plugin popup | 키/IME 를 host egui 로 들여보낼지(= 터미널 포워딩을 막을지). 경로 키는 이 값 대신 [키 경로의 수신자](#키-경로와-수신자)로 정한다 |
| `MainView::mouse_overlay_open()` (`src/view/main/mouse.rs`) | settings + **무대** | 메인 화면의 좌표로 마우스를 처리할지 |
| `MainViewState::has_egui_overlay_open()` (`src/state.rs`) | dialog + plugin popup + popup(visible) + **무대** + tutorial | WebView(OS 네이티브 자식 뷰) 위에 대상 오버레이가 있는지. 숨길지는 이 값과 별도의 오버레이 모드(`state::webview_overlay`)가 정한다. 모드가 숨김이면 모두 숨기고, 아니면 보인 채 대상 영역을 backend 에 넘긴다([WebView](../design/systems/webview.md#모드-결정)). 토스트는 여기에 들어가지 않고 카드와 겹치는 WebView 중 키를 받지 않는 것만 숨긴다([토스트](../design/systems/toast.md#native-webview-위의-토스트)) |

왜 조합이 다른가:

- **키보드/IME 판정**은 키 이벤트를 egui와 중앙 디스패처(터미널/단축키) 중 어디로 보낼지 정한다. 이 앱은 키를 기본적으로 egui 에 주지 않으므로, "지금 텍스트 입력을 받는 오버레이가 있는가" 라는 넓은 정의가 필요하다.
- **마우스** 는 `src/view/main.rs` 의 이벤트 분기에서 **항상 무조건** egui 로 먼저 전달되고 `egui_consumed` 로 결과를 받는다 — 라우팅 전제 자체가 없다. 그 결과는 직전 패스의 포인터 위치 기준이라, 타이틀바 좌클릭 press 는 위 '타이틀바 좌클릭 press' 단락의 사건 좌표 판정을 함께 쓴다. Popup 위 클릭은 이미 `egui_consumed`/`popup_owns_point`(위치 기반)로 정확히 처리되므로, 여기 남은 항은 **모달(별도 OS 창) 전용** 보강 게이트일 뿐이다. `has_input_dialog_open()`(rename, popup 시스템으로 구현됨)과 `popups.has_focused()` 는 정책상 Popup 이 비모달이라 위치 밖 클릭까지 막을 이유가 없어 안 들어간다.
- **WebView** 는 입력이 아니라 **표시** 질문이다. WebView 는 OS 네이티브 자식 뷰라 wgpu 표면 **위**에 있어 "안 그리는 것" 만으로는 사라지지 않는다 — 숨기려면 `set_visible(false)` 가 필요하고 그 게이트가 이 함수다. 그래서 popup 을 `has_focused()` 가 아니라 `has_visible_open()` 으로 넓게 본다. 페이지를 보인 채 두는 모드에서는 대상이 보이는 동안 WebView 영역의 포인터를 호스트가 받으므로, 페이지 위 클릭도 egui 의 바깥 클릭으로 처리된다.

**plugin egui-mesh popup 의 키보드 계층**: 이 popup 은 host 인스턴스가 아니라 `popups.has_focused()` 로 잡히지 않지만, 키보드 계층에서는 **focused host popup 과 동급**이다 — 창의 포커스 노드이면 키/IME 가 egui 로 들어가고 터미널로는 안 간다. 열려 있기만 한 plugin popup(다른 범위, passive)은 키를 가져가지 않는다. 그 **키와 IME 조합**은 `collect_mesh_popup_input` 이 `ctx.input` 에서 수집해 plugin 프로세스로 forward 하므로, 게이트가 닫혀 있으면 forward 소스가 비어 입력이 통째로 터미널로 전달된다. IME 는 조합 세션 네 단계를 모두 전달한다 — Commit 만 실으면 egui `TextEdit` 이 조합 결과를 조용히 버린다([egui-mesh-channel § 입력 게이트](../dev-guide/egui-mesh-channel.md)). 게이트 지점(winit 이벤트 핸들러)은 `PluginManager` 에 접근할 수 없어 포커스 칸과 렌더 프레임이 남긴 plugin 후보로 정하는 `MainViewState::plugin_popup_owns_keys()` 를 읽는다 — 마우스 쪽 팝업 사각형과 같은 프레임 간 전달 패턴이다(위 "프레임 간 전달" 절). 같은 술어를 IME 라우팅(`view::main::ime`)과 plugin surface 단축키 게이트(`app::plugin_glue::shortcut`)도 공유한다. 예외는 `set_ime_allowed` 판정(`gfx/gpu.rs`) 하나 — plugin popup 은 host egui 위젯이 없어 IME 를 끄면 popup 안에서 조합 입력을 못 하게 되므로 제외한다. 겹친 popup 중 **누가** 키를 갖는지는 z 순서가 아니라 창의 포커스 노드가 정한다([popup.md § 포커스](../design/systems/popup.md#포커스)).

#### 무대는 창 키 경로의 정면 함수가 함께 본다

`has_egui_overlay_open` 에 무대가 들어가 있어도 나머지 둘은 각자의 조건을 확인한다 — 그 함수의
프로덕션 소비처는 WebView 표시 하나뿐이다. `keyboard_overlay_open()` 에는 무대 항이 없다. 창 키 경로는
그 술어를 직접 부르지 않고 정면 함수 `MainViewState::window_keys_to_overlay()`(`src/state.rs`, 본문
`keyboard_overlay_open() || fullscreen_stage_active()`)를 부른다. 술어를 직접 부르는 곳은 아래 1·2 둘뿐이다.

| # | 지점 | 무대 | 근거 |
|---|------|--------|------|
| 1 | `window_keys_to_overlay` 정의(`src/state.rs`) | 정의에 포함 | 아래 3의 지점이 모두 이 함수를 부른다 |
| 2 | `src/state/popup_key_owner.rs` `window_holds_terminal_keys`(경로 키가 아닌 사건의 터미널 포워딩 게이트). 호출부는 같은 파일의 `window_key_holds_terminal` 이고, 그 함수를 `handle_keyboard_input`(`src/view/main/keyboard.rs`)이 부른다 | **없음** | 호출부 함수 **앞**의 0단계 게이트(`try_consume_fullscreen_stage_key`)가 무대 키를 전부 소비하고 return 하므로 여기까지 오지 않는다. 그 게이트가 사라지면 이 식도 무대 항이 필요해진다 |
| 3 | 정면 함수를 부르는 곳: `window_feeds_egui`(egui feed 게이트, `src/state/popup_key_owner.rs`), IME(`src/view/main/ime.rs`), 경로 키가 아닌 사건의 plugin 명령 게이트(winit·native webview 포워딩 키 모두, `src/adapters/ui/input/key_nodes.rs`), `ui.state` 의 `keyboard_shortcuts_gated`(`src/adapters/ipc/handler/debug_state.rs`), 외부 끌어내기(`src/view/main/file_drag.rs` `poll_outbound_drag`), X11 조합 종료 키 소유권(`src/adapters/ui/input/explorer_ime.rs`), native 편집 키 차단 상태(`src/app/webview_sync.rs`), 편집 키 판정(`src/view/main/edit_keys.rs`), 플러그인 surface 편집 키(`src/view/main/plugin_surface_keys.rs`) | 정면 함수 | egui feed 는 방향이 반대다(무대 콘텐츠는 egui 위젯이라 키가 egui 로 가야 한다). IME 는 무대만 떠 있으면 조합 Commit 이 터미널로 샌다. plugin 단축키·webview 키·native 편집 키는 `dispatch_window_event_to_view` 이전이나 밖에서 돌아 0단계 게이트를 거치지 않는다([WebView 호스트 계약](../design/systems/webview.md)). 편집 키·플러그인 surface 편집 키는 무대에 들어간 키가 뒤쪽 화면에서 다시 쓰이지 않게 한다. 끌어내기는 OS 드래그라 렌더한 오버레이로 덮을 수 없어 취소한다. 보고 값은 무대 중에 "막히지 않았다"고 보고하지 않게 한다 |
| 4 | `mouse_overlay_open()` 정의 | 정의에 포함 | 마우스 다섯 호출부 전부가 이 하나를 본다 |
| 5 | `has_egui_overlay_open()` 정의 | 정의에 포함 | WebView 가 무대 위로 뚫고 나오지 못하게 |

무대를 `keyboard_overlay_open()` 안에 넣지 않은 이유는 2 다 — 그 지점은 무대에 대해 다른
답(0단계 게이트가 이미 처리)을 쓴다. `crates/tasty-doc-guards/tests/fullscreen_stage_input_gate.rs` 의
`every_overlay_open_composite_is_stage_aware` 가 (a) 정면 함수 본문이 무대를 보는지, (b) 술어를 직접 부르는
곳이 1·2뿐인지, (c) 정면 함수 호출부 집합이 3과 같은지를 소스에서 확인한다. 시험 파일과 소스 가드는 호출부로
세지 않는다. 새 호출부가 생기면 그 테스트가 실패하고, 그때 이 표도 함께 갱신한다. 2 의 예외도 그 테스트가
"0단계 게이트가 유지되는가" 로 함께 검증한다.

탐색기 타입어헤드와 html 주소 입력칸처럼 그리기 중 키를 받을지 정하는 판정은 정면 함수를 부르지 않는다. 그리는
입력 몫의 키 노드(`MainViewState::pass_key_node`, 그 몫이 팝업에 묶이지 않았을 때의 몫 포커스 노드)가 자기 노드
(`Surface`·`HtmlAddress`)인지로 정한다. 그 몫의 포커스 노드가 무대면 받는 surface 가 없으므로 무대에 들어간 키가
뒤 탐색기로 가지 않는다. 그리기 경로에서 지금 포커스·키 게이트를 읽는 자리는 소스 가드
`source_guards::draw_pass_key_reads` 가 호출 자리(파일·감싼 함수·호출 줄) 명부로 고정한다.

알려진 미해소 사안: popup 바깥 클릭이 popup 을 닫으면서, 그 클릭이 겨냥한 하위 액션도 같은 클릭에서 함께 발생한다. 닫힘만 소비하고 액션을 막을지는 UX 판단이 필요해 별개 사안으로 분리돼 있다.

### 새 Foreground 레이어를 추가할 때 체크리스트

1. `egui::Area::new(...).order(Order::Foreground)...show(ctx, |ui| { ... })` 로 그린다 — bare `Ui::new(...).layer_id(...)` 금지((c) 참고).
2. 이 레이어가 위 정책 표의 어느 순서에 들어가는지 정하고, `enforce_foreground_z_order` 의 `set_sublayer` 체인에 끼워 넣는다. 기존 그룹(Banner↔{상태바,탭바})에 합류시키거나, 새 그룹을 만들 때는 **1단 들여쓰기 제약**((d) 참고)을 넘지 않게 그룹을 겹치지 않게 나눈다.
3. 정말 모달처럼 항상 최상단이어야 하는 예외라면(예: 위 plugin popup) Area 미등록을 의도적으로 유지하고 그 이유를 주석/문서로 남긴다.
4. 겹치는 배치를 만들어 `tasty screenshot` 으로 실측 확인한다(디자인 시나리오만으로 단정하지 않는다, `docs/dev-guide/self-verification.md`).

**전체화면 무대는 이 체크리스트의 2 번 대상이 아니다.** 무대(`src/adapters/ui/fullscreen.rs`)는
1 번대로 등록된 `Area`(`Order::Foreground`)로 그리지만 `enforce_foreground_z_order` 체인에는
들어가지 않는다 — 무대는 **별개 프레임**에서 그려지고(`Gpu::render` 의 무대 분기) 그 프레임에는
host chrome·popup·오버레이가 아예 그려지지 않아 같은 tier 안에서 순서를 다툴 상대가 없기
때문이다. 3 번(Area 미등록으로 최상단을 얻는 예외)에 기대지 않은 것도 같은 이유다. 상세
[`design/systems/fullscreen-stage.md`](../design/systems/fullscreen-stage.md).

## 키 입력 경로

키보드 입력은 좌표가 아니라 창의 포커스 노드에서 시작한다. 키 하나는 포커스 노드에서 engine 쪽으로 오르는
노드 목록(키 경로)을 따라가고, 그 키를 처음 쓰는 노드가 받는다. 팝업이 쓰지 않는 키는 그 팝업이 붙은 노드와
그 위 level의 단축키로 올라간다. 메인 창에 적용하고, 설정·plugins·preset·quit 같은 별도 창은 지금 그 창의
입구(`judged_window_event`)대로 처리한다.

### 키 사건 순서

메인 창 키 사건 하나는 App 입구(`App::route_window_input`)에서 plugin 명령 판정(`try_plugin_shortcut`)을 거친 뒤
MainView 입구(`MainView::handle_engine_event`)로 간다. view 안에서는 사건 앞 처리 뒤에 수신자를 한 번 정하고
(`decide_key_receiver`, 결과는 `MainView::key_decision`), egui 적재(`feed_egui_keyboard`)와 키 처리
(`handle_keyboard_input`)가 그 값을 함께 읽는다. 단계 순서의 정본은 `MainKeySteps`(`src/view/main/key_event_steps.rs`)이고
`source_guards::edit_key_judgment_order`가 고정한다. egui 적재는 `MainViewState::window_feeds_key`, 터미널로 보낼지는
`MainViewState::window_key_holds_terminal`이 정한다. vi 복사 모드는 지금 자리 그대로다.

### 경로 키

키 경로를 지나는 사건은 경로 키뿐이다. 경로 키는 egui 키 사건이 되는 누름 가운데 지금 경로에 남는 예외(bare Esc,
설정 열기 요청 래치 중 키, 클립보드 조합, 편집 바인딩 키)를 뺀 것이다. 판정의 정본은 `key_class::classify_key`이고
egui-winit 변환과 같은지는 `key_class_tests.rs`가 잠금 버전 소스와 대조한다. 경로 키가 아닌 사건은 키 경로 이전의
게이트(`window_feeds_egui`·`window_holds_terminal_keys`·`window_keys_to_overlay`)를 그대로 지난다. bare Esc의 처리는
[popup.md의 Esc 소유권](../design/systems/popup.md#plugin-popup--host-popup-부모-자식)을 따른다.

### 키 경로와 수신자

출발 노드(`key_path::key_start_node`)는 창의 포커스 노드이고, 이번 패스의 키를 팝업에 묶었으면 그 팝업이다. 경로
(`key_nodes::window_key_path`)는 출발 노드에서 engine 쪽으로 오른다. 덮는 팝업은 덮인 범위를 건너뛰고, 펼친 팝업과
전체화면 무대는 engine으로 바로 잇는다.

수신자(`key_route::route_key`)는 출발 노드가 그 키를 쓰면 그 노드이고, 쓰지 않으면 경로의 단축키 단계
(`KeyReceiver::Shortcut`)다. 맞는 단축키가 없으면 출발 surface(끝 목적지)가 받는다. 노드마다 쓰는 키는
`key_route::start_takes`에, 입력칸 팝업의 명령 키 규칙은 `key_route::text_or_up`에 있다. 장면별 기대는
`key_route_tests.rs`와 `key_nodes_tests.rs`가 정본이다.

### 단축키 level

단축키 필드마다 level이 있다(`key_level::KEYBINDING_LEVELS`). 키 경로에 없는 level의 후보는 매칭 전에 비운다
(`key_path::focus_path_levels`, `key_path::path_keybindings`). 창 단축키, 더블탭, plugin 명령이 같은 값으로
거른다. 그래서 덮는 팝업이 포커스면 덮인 범위의 단축키가 돌지 않는다.

### plugin 명령과 WebView 전달 키

plugin 명령 판정은 지금처럼 App 입구에서 view보다 먼저다. 게이트(`PluginKeyGate`)만 키 경로를 쓴다.

winit 키는 포커스 노드에서, native WebView가 넘긴 키는 그 키를 받은 surface에서 시작한 경로로 판정한다. 그래서
WebView 키는 받은 surface를 덮는 팝업에만 막힌다. 조건의 정본은 `key_nodes::plugin_key_gate_open`,
`key_nodes::received_key_blocked`, WebView 쪽 창 전체 차단은 `app::webview_keys`의 `forwarded_key_gate_open`이다.

## 관련

- [popup 시스템](../design/systems/popup.md) — 팝업 z-order·스코프·포커스
- [전체화면 무대](../design/systems/fullscreen-stage.md) — 별개 프레임으로 그려지는 창 독점 표면
- [banner 시스템](../design/systems/banner.md) — 배너 마우스 소비·스코프·발화 정책
- [dev-guide/popup-implementation](../dev-guide/popup-implementation.md) — 팝업 추가 절차
- [multi-window](multi-window.md) — 윈도우 단위 구조
