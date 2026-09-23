# 마우스 입력 계층 (Input Layer)

윈도우 내부에서 마우스 이벤트(클릭·이동·스크롤)와 커서 아이콘이 **렌더링 z-order 와 일치하는 입력 z-order** 를 따른다 — 화면에서 위에 그려진 요소가 입력을 먼저 받고, 가려진 요소는 받지 않는다.

## 기본 동작: 소비 (Consume)

z-order 최상위부터 hit-test 한다. 좌표가 어떤 레이어 영역 안이면 그 레이어가 이벤트를 **소비**하고, 하위로 전달하지 않는다. 투과(pass-through)는 기본이 아니다.

## 입력 계층 순서

마우스 핸들러(`src/view/main/mouse.rs`)는 상위부터 차례로 가드한다:

| 순서 | 레이어 | 판정 | 동작 |
|------|--------|------|------|
| **0** | **전체화면 무대** | `state.fullscreen_stage_active()` | 활성이면 **1~7 전부를 좌표 판정 없이 무조건 차단**(아래) |
| 1 | 모달/오버레이 | `overlay_open` | 열려 있으면 터미널 입력 차단 |
| 2 | Popup | `state.popup_hovered` | 팝업 위면 소비(터미널 무시) |
| 2b | Modifier-hint 오버레이 | `state.modifier_hint_hovered` | 오버레이 위면 소비 **+ 비활성 surface 전환도 차단**(popup 과 동급) — 드래그/리사이즈/X 클릭이 하위 surface 포커스로 안 샘 |
| 3 | **비활성 surface 전환** (좌클릭 press) | `surface_id_at_position != focused` | 첫 클릭은 surface 전환이 소비 — 그 위 배너/egui 위젯과 무관(아래 참고) |
| 4 | egui 위젯 (사이드바·탭바·오버레이) | `egui_consumed` | egui 가 소비하면 종료 |
| 5 | Banner | `state.banner_hovered` | 배너 위면 소비(터미널 무시) — **단 비활성 surface 전환에는 적용되지 않음** |
| 6 | Divider | `find_*_divider_at`(threshold) | 분할 경계 안이면 소비(드래그 시작) |
| 7 | Terminal/Surface | — | 최하위, 콘텐츠가 처리 |

`handle_cursor_moved` / `handle_mouse_input` / `handle_mouse_wheel` 이 모두 같은 가드(`egui_consumed || overlay_open || state.popup_hovered || state.banner_hovered || state.modifier_hint_hovered`)로 상위 레이어를 먼저 거른 뒤 divider → terminal 로 내려간다. 배너는 **자기 영역의 마우스를 소비**(뒤로 전파 X)하는 focus-less 오버레이라 popup 과 같은 precedence 에서 차단한다(Toast 는 입력을 통과시키므로 이 가드에 없다 — [banner 시스템](../design/systems/banner.md)).

**순서 6 은 버튼 없는 hover motion 보고에도 적용된다.** DECSET 1003(AnyEventMouse)을 켠 앱에 커서 이동을 보고하기 전에 press 경로와 **같은 threshold**(`state::mouse::DIVIDER_HIT_THRESHOLD`)로 divider 밴드를 먼저 판정하고, 밴드 안이면 보고하지 않는다 — 밴드는 gap(1~2px)보다 넓어 양쪽 surface rect 안쪽까지 겹치므로, 이 가드가 없으면 "커서는 ↔ 인데 그 아래 TUI 는 hover 를 계속 받는" 불일치가 생긴다. OS 창 리사이즈 가장자리 밴드와 divider 드래그 진행 중에도 같은 이유로 보고하지 않는다. 대상은 focused surface 한정이다([ADR-0081](../adr/0081-hover-motion-focused-surface-only.md)) — 순서 3(비활성 surface 전환)이 클릭에 대해 세운 "배경 캡쳐 TUI 로 마우스가 새지 않게" 원칙을 hover 에도 적용한 것이며, 다만 hover 는 포커스를 옮기지 않는다. **버튼을 누른 채 시작한 드래그 motion 은 이 가드들의 적용 대상이 아니다** — 대상 surface 가 press 시점에 고정되므로 밴드/이웃 surface 로 나가도 원래 surface 기준으로 계속 보고된다.

**진행 중인 divider 드래그는 surface 콘텐츠보다 우선한다(순서 6 > 순서 7).** egui-mesh surface(마크다운·이미지)는 포인터 이동/버튼을 surface-local 로 forward 하고 소비하는 별도 경로(`egui_mesh_target_at`)를 갖는데, 이 forward 는 표의 순서 7(콘텐츠) 성격이므로 **`dragging_divider.is_some()` 일 때는 건너뛴다** — `handle_cursor_moved`(포인터 이동)와 `handle_mouse_input`(버튼)의 egui-mesh forward 가드에 `dragging_divider.is_none()` 조건을 둔다. 없으면 드래그 중 커서가 egui-mesh surface 영역으로 들어갈 때 forward 가 early-return 하여 divider 갱신이 멈추고(멈춤), release 도 forward 로 소비돼 드래그가 확정/해제되지 않는다(sticky). 터미널/explorer surface 는 egui-mesh 가 아니라 이 경로를 타지 않아 원래도 정상.

**Modifier-hint 오버레이**(modifier 홀드 시 뜨는 focus-less 패널)도 마우스를 소비하지만 배너보다 한 단계 강하다: 위 통합 가드(소비·휠·커서)에 더해 **click-to-activate 전환 가드**(`!popup_hovered && !modifier_hint_hovered`)에도 들어가, 오버레이 위 좌클릭이 하위 surface 로 **포커스를 옮기지 못하게** 막는다(popup 과 동급, banner 와 차이). 오버레이 드래그 이동·테두리/코너 리사이즈·X 클릭이 터미널 포커스·selection·마우스 리포트로 새지 않는다 — 키보드 포커스는 애초에 취득하지 않는다(원칙3, [banner 와 동일한 focus-less 성질]). 4지점 배선: `handle_mouse_input` 의 click-to-activate press 가드 + 통합 소비 가드, `handle_mouse_wheel`, `handle_cursor_moved`.

### 순서 0 — 전체화면 무대는 좌표를 묻지 않는다

[전체화면 무대](../design/systems/fullscreen-stage.md)가 활성이면 그 위 어떤 레이어도 뒤 세계의 입력을 받지 못한다. 이 단이 표의 다른 단들과 다른 점은 **hit-test 가 없다**는 것이다 — popup(`popup_hovered`)·배너(`banner_hovered`)는 "포인터가 그 위인가" 를 묻지만, 무대는 화면 전체를 덮으므로 물을 이유가 없고 뒤 위젯은 그려지지도 않은 상태라 그 좌표로 판정하는 것 자체가 유령 입력이다. "투과는 기본이 아니다" 원칙의 최상위 사례다.

배선은 `MainView::mouse_overlay_open()`(= `settings_open_requested || fullscreen_stage_active()`) 한 곳으로 모아, 통합 가드·click-to-activate press 가드·휠·커서 이동·OS 가장자리 리사이즈 양보·링크 hover 계산이 전부 같은 값을 보게 했다. 커서 아이콘도 무대 중에는 `winit_cursor_icon_at` 이 조기 반환한다(무대 프레임의 커서는 egui `platform_output` 이 정한다).

**두 항의 생애가 다르다 — `settings_open_requested` 가 덮는 것은 "설정 창이 떠 있는 동안" 이 아니라 프레임 경계 하나다.** 그 값은 사이드바 설정 버튼이 눌렸다는 **열기 요청 래치**다. 세우는 자리는 egui 패스(`src/adapters/ui/draw.rs`)이고 그것은 `render_if_dirty` 안에서 돌며, 지우는 자리는 `MainView::handle_redraw` 의 첫 줄 `dispatch_pending_modal_opens` 이라 그보다 **앞**이다. 그래서 값은 프레임 N 의 egui 패스에서 서고 프레임 N+1 시작에서 지워진다. 설정 창 자체는 `event_loop.create_window` 로 뜨는 **별도 winit 창**이라, 그 창이 떠 있는 동안 메인 창에는 입력 이벤트가 애초에 오지 않는다. 즉 이 항이 막는 구간은 **버튼을 누른 프레임과 모달이 실제로 생기기 전 사이의 한 틱**이다. 같은 항이 키보드 쪽 `AppState::keyboard_overlay_open` 에도 들어가며 생애는 동일하다. "모달이 떠 있는가" 를 물어야 하는 소비자가 볼 값은 그것이 아니라 `view::View::is_modal_active()` 다 — 그 값은 지속한다. 위 호출 순서는 `crates/tasty-doc-guards/tests/fullscreen_stage_render_gate.rs` 가 소스 구조로 고정한다(순서를 뒤집으면 이 항을 읽는 넷이 영영 참이 되지 않는데, 그것을 런타임으로 재는 시험은 없다).

**키보드는 별도 배선이다.** 마우스 계층과 달리 키보드에는 `handle_keyboard_input` 파이프라인의 **0단계 게이트**를 새로 세웠다(double-tap 1~3단계보다 앞). 무대 중 ESC 는 그 자리에서 무대만 닫고 즉시 `return` 하므로 4단계(settings/notifications 닫기)에 도달하지 않는다 — "무대 종료 ESC 는 뒤로 전파되지 않는다" 는 사용자 확정 계약이다. 입력 계약 전체(IME·진입 시 정리·OS 레벨 UI·모달과의 공존)는 [fullscreen-stage.md § 입력 계약](../design/systems/fullscreen-stage.md#입력-계약).

### 비활성 surface 클릭 = 전환 우선 (click-to-activate swallow)

**포커스되지 않은 surface 영역을 좌클릭하면, 그 위에 배너/egui 위젯이 있든 없든 그 첫 클릭은 "surface 전환" 이 통째로 소비한다** (macOS click-to-activate 모델). 이 단계는 통합 가드(`egui_consumed || … || banner_hovered`) **위**, modal/popup **아래**에 위치한다(`src/view/main/mouse.rs::handle_mouse_input`). 따라서:

- modal(`overlay_open`)·popup(`popup_hovered`)은 surface 비소속 독립 상위 레이어라 전환보다 **먼저** 배제된다 — 팝업/모달을 클릭해도 뒤 surface 로 포커스가 넘어가지 않는다.
- Banner 소비(순서 5)는 *동작*(action 위젯·마우스 리포트 차단)에는 적용되나 *비활성 surface 포커스 전환*에는 적용되지 않는다 — 배너 카드를 클릭해도 소속 surface 로 포커스가 간다. 이로써 마우스-캡쳐 배너 같은 persistent 배너가 떠 있어도 surface 전환이 막히지 않는다.
- **트레이드오프**: 비활성 surface 첫 클릭은 전환에만 쓰이고 그 자리 selection/cursor/마우스 리포트로 흐르지 않는다(한 번 더 클릭). 배경 캡쳐 TUI 로 마우스가 새는 것을 막는다.
- gating 은 `surface_id_at_position(x,y) != focused_surface_id` — 이미 활성인 surface 안 클릭은 전환 단계를 건너뛰어 정상(selection/리포트/divider)으로 흐른다. egui 크롬(사이드바·탭바)은 surface rect 밖이라 `surface_id_at_position == None` → 전환 대상 아님 → `egui_consumed` 로 소비.

### `popup_hovered` / `banner_hovered` / `modifier_hint_hovered` 프레임 간 전달

`PopupManager::draw()`(`src/adapters/ui/popup/draw.rs`) / `BannerManager::draw()`(`src/adapters/ui/banner.rs`) / `draw_modifier_hint()`(`src/adapters/ui/modifier_hint_overlay.rs`)는 렌더 시점에 호출되고 마우스 이벤트는 이벤트 시점에 처리돼 타이밍이 다르다. 그래서 draw 결과(`PopupDrawResult.hovered` / `BannerDrawResult.hovered` / `HintDrawResult.hovered`)를 `state.popup_hovered` / `state.banner_hovered` / `state.modifier_hint_hovered`(`src/state.rs`)에 프레임 간 상태로 저장해 두고(popup 은 `src/adapters/ui/popup/frame.rs`, 배너·modifier-hint 는 `src/adapters/ui/overlay.rs` 의 draw 패스에서 기록), 마우스 핸들러가 그 값을 참조한다. (상세 모델은 [popup 시스템](../design/systems/popup.md) · [banner 시스템](../design/systems/banner.md).)

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

egui 는 `egui::Order` enum(`Background` / `Middle` / `Foreground` / `Tooltip` / `Debug`, egui 0.31.1 `layers.rs`)으로 그리기 순서를 5단으로 고정한다 — **enum 선언 순서 그대로**, 매 프레임 무조건 그 순서로 그린다. tasty 의 Popup/Modifier-hint/Banner/egui 위젯(사이드바·탭바·상태바)은 전부 `Order::Foreground` 한 tier 안에 있다 — 즉 위 표의 1(모달 제외)·2·2b·4·5 는 **같은 tier 안에서** 상대 순서를 가려야 하는 문제고, `Order` 만으로는 해결되지 않는다. Divider/Terminal(6/7)은 다른 tier(`Middle` 이하)라 애초에 범위 밖이다.

### (b) 같은 tier 안의 순서 — `Areas::order` + `move_to_top`

같은 `Order` tier 안에서는 `Memory::Areas::order: Vec<LayerId>`(egui 내부, `memory/mod.rs`)가 실제 그리는 순서를 정한다 — 이 Vec 의 **뒤쪽일수록 나중에 그려져 위에 보인다**. 이 Vec 에 편입되려면 그 레이어가 **`egui::Area`로 등록**돼야 하고(`.show(ctx, ...)` 호출 시 내부적으로 `move_to_top` 또는 `set_state`가 `order.push`), 등록된 레이어끼리는 매 프레임 `end_pass()`가 `order.sort_by_key(|l| (l.order, wants_to_be_on_top.contains(l)))`(안정 정렬)로 재배치한다. `Context::move_to_top(layer_id)`(`ctx.move_to_top`)를 호출하면 그 프레임에 "맨 위로" 플래그(`wants_to_be_on_top`)가 서고, 이미 등록된 id 면 그 프레임의 정렬에서 플래그 안 선 다른 레이어들보다 뒤(위)로 간다.

**주의 — `move_to_top` 을 여러 레이어에 순서대로 반복 호출해도, 그 여러 레이어 "끼리의" 상대 순서는 바뀌지 않는다.** 안정 정렬은 플래그가 같은(모두 true) 레이어들을 **서로 tie** 로 묶어, 그 tie 안에서는 각 레이어가 세션 중 **최초로 등록된** 시점의 상대 순서를 그대로 유지한다. 즉 "4개 레이어에 A→B→C→D 순서로 `move_to_top` 을 매 프레임 호출하면 A<B<C<D 계층이 만들어진다"는 직관은 **egui 0.31.1 에서 성립하지 않는다** — 실제로 성립하려면 그 레이어들이 서로 다른 `wants_to_be_on_top` 상태(일부만 true)여야 한다.

### (c) 미등록 레이어 함정

`egui::Area` 로 등록되지 **않은** 레이어(`ctx.layer_painter(layer_id)` 로 얻은 raw painter — `Areas::order` 에 전혀 없음)는 `GraphicLayers::drain()` 이 같은 tier 안에서 **등록된 레이어를 전부 그린 다음** 그린다 — 즉 등록 여부와 무관하게, 미등록 레이어는 그 tier 안에서 **항상 최상단**에 고정된다. 원래 배너(`banner.rs`, 커밋 `03c583efd`)와 이번에 고친 Modifier-hint(`modifier_hint_overlay.rs`)가 각각 이 함정에 걸려 있었다 — 둘 다 `egui::Ui::new(...).layer_id(layer_id)` 로 bare `Ui` 를 직접 만들어 그렸을 뿐 `egui::Area::new(...).show()` 를 거치지 않았다. 두 경우 모두 **호출 순서를 바꿔도 고쳐지지 않는다** — 미등록인 한 항상 위다.

### (d) tasty 의 중앙 집중식 강제 — `enforce_foreground_z_order`

`src/gfx/gpu/egui_bridge.rs::enforce_foreground_z_order`(`run_egui_frame` 안 `ui::draw_popups` 직후, 매 프레임 1회 호출)가 Foreground tier 안 Banner/egui위젯(상태바·탭바)/Modifier-hint/Popup 4 레이어의 상대 순서를 강제한다. (b)의 한계 — `move_to_top` 반복 호출로는 4단 계층을 못 만든다 — 때문에, 실제로는 `Context::set_sublayer(parent, child)`(`end_pass` 가 안정 정렬 *이후* child 를 parent 위치 바로 뒤로 splice — 등록 시점과 무관한 강제 인접 배치, egui 내부적으로 `Window` 안 하위 `Area` 배치에 쓰는 것과 같은 API)를 조합한다:

- `banner_layer` 를 부모로, `status_bar`/각 pane 의 `tab_bar` 를 자식으로 묶어 **Banner < {상태바, 탭바}** 를 등록 시점과 무관하게 고정한다.
- `modifier_hint` 가 떠 있으면 그것을 부모로, 열려 있는 모든 popup 레이어를 자식으로 묶어 **Modifier-hint < Popup** 을 고정한다.
- 두 그룹의 부모끼리(`banner_layer` ↔ `modifier_hint`)는 서로를 엮지 않는다 — `set_sublayer` 는 **1단 들여쓰기만** 지원해 parent 가 다른 sublayer 의 child 이면 동작이 egui 문서상 unspecified(실측: `Areas::end_pass` 가 `sublayers` HashMap 을 순회하는 순서에 좌우돼 비결정적으로 깨질 수 있음)이기 때문이다. 대신 자연 등록 순서에 안전하게 의존한다 — `banner_layer` 는 배너가 0개여도 매 프레임 무조건 그려져 앱 시작 첫 프레임에 반드시 등록되는 반면, `modifier_hint` 는 사용자가 modifier 를 처음 누르는 프레임에야 등록된다. 항상 banner 보다 늦게 등록되므로, 안정 정렬에서 영구히 더 위(늦은 위치)에 남는다.

결과적으로 Banner(5) < {상태바·탭바}(4) < Modifier-hint(2b) < Popup(2) 이 재현된다. Modal(1)은 별도 OS 창이라 범위 밖, Divider/Terminal(6/7)은 다른 `Order` tier 라 범위 밖이다.

**실측 확인**(임시 debug 인스턴스 + `tasty screenshot` CLI):
- Popup(`debug.host_popup.open`)과 Modifier-hint(`debug.modifier_hint.hold`)를 창 크기를 줄여 강제로 겹치게 배치 → Popup 이 Modifier-hint 위에 그려짐(가려진 keycap 만 가장자리에 남음).
- Banner(`debug.banner.show --scope view`)는 View 스코프 플레이스홀더가 탭 행과 겹쳐 뜬다 — 탭 칩(`zilhak@...`)이 배너 카드 위에 온전히 그려짐(A/B 비교: `enforce_foreground_z_order` 호출을 임시로 빼면 탭 칩이 배너에 완전히 가려짐 — 재삽입하면 복구). 상태바도 탭바와 동일한 `set_sublayer(banner_layer, ...)` 관계라 같은 메커니즘이 적용된다. Banner 와 상태바가 **기하적으로 직접 겹치는** 배치는 만들 수 없었다 — Banner 의 zone 은 항상 탭바 하단에서 시작해 화면 끝까지 뻗지만 카드 자체는 zone 상단에 고정 높이로만 그려지고 `set_clip_rect(zone)` 로 그 밖으로 넘치지 않아, 카드가 화면 하단(상태바 행)까지 물리적으로 닿을 방법이 없다(정상 동작 — 버그 아님).
- Popup 단독 / Banner 단독 / Modifier-hint 단독 회귀 확인 — 셋 다 기존과 동일하게 정상 렌더.

### 레이어별 Order/Area 등록 현황 (조사 결과)

| # | 레이어 | `Order` | `egui::Area` 등록 | 상태 |
|---|--------|---------|---------------------|------|
| 1 | 모달/오버레이(`overlay_open`) | 해당없음 | 별도 OS 윈도우 — Area 개념 자체가 적용 안 됨 | **해소됨** — mouse.rs 의 `overlay_open` 정의 차이는 조사 결과 의도된 설계(위 "`overlay_open`" 절)로 결론. rename 다이얼로그는 popup 시스템 경로라 #2 와 동일 |
| 2 | Popup | Foreground | 등록됨 | 이상 없음(기존부터 정상) |
| 2 (예외) | `plugin_bridge/popup_render.rs` egui-mesh popup 셸 | Foreground | 기본 미등록(raw `layer_painter`), host popup 과 z-order 경합 시 `set_sublayer` 로 조건부 등록 | **해소됨** — 의도적 예외를 유지하되 host popup 과의 z_seq 경합 시 조건부로 깨진다(아래 "`plugin_bridge/popup_render.rs`" 절 갱신 참고) |
| 2b | Modifier-hint 오버레이 | Foreground | 미등록 → **등록함** | **해소됨** — `modifier_hint_overlay.rs` 를 `egui::Area` 로 등록하도록 수정 |
| 4 | egui 위젯(사이드바·탭바·상태바) | 혼재(SidePanel=Background 미등록 / tab_bar·status_bar=Foreground 등록) | — | tab_bar/status_bar 는 이상 없음(기존부터 정상). SidePanel 은 Background tier 라 이번 범위 밖(아래 "Background tier" 절) |
| 5 | Banner | Foreground | 등록됨(`03c583efd`, 이번 작업 이전 완료) | `enforce_foreground_z_order` 로 상태바/탭바보다 아래 고정 — **해소됨** |
| 6 | Divider | Middle | 미등록, raw painter | 범위 밖(다른 tier) — 조사 결과 표의 "6번"은 렌더 레이어 경쟁이 아니라 `mouse.rs` 의 좌표 기반 입력 우선순위로 확인, 변경 불필요 |
| 7 | Terminal/Surface | 터미널=Order 밖 / 비터미널=Background | 비터미널만 등록 | 범위 밖(다른 tier) |

**순서 불일치 가능성이 있던 3쌍의 결론**:

1. **Popup vs Modifier-hint** — 실측 결과 **불일치 확정 → 이번 작업에서 수정**(Modifier-hint Area 등록 + `enforce_foreground_z_order` 의 `set_sublayer(modifier_hint, popup)`). 수정 후 재실측으로 Popup 이 위에 그려짐을 확인(위 "실측 확인" 절).
2. **Banner vs status_bar/tab_bar** — 실측 결과 **불일치 확정(A/B 비교로 재현) → 이번 작업에서 수정**(`enforce_foreground_z_order` 의 `set_sublayer(banner_layer, status_bar/tab_bar)`). status_bar 와의 직접 겹침 배치는 못 만들었으나(위 "실측 확인" 절 — zone/clip_rect 구조상 불가능), tab_bar 와는 동일 메커니즘·동일 부모 관계로 실측 완료.
3. **Background tier(SidePanel bare 배경 vs Explorer/Markdown/Html 등록)** — 조사 결과 **현재는 겹치는 배치가 존재하지 않아 실측 불가/불필요**로 결론(위 "Background tier" 절). 잠재 위험은 남아 있으므로 겹치는 시나리오가 생기면 이 문서의 (b)~(d) 메커니즘을 적용한다.

### Background tier(사이드바 SidePanel·Explorer·Markdown·Html)는?

이번 구현 범위 밖이다. 이 레이어들은 `Order::Background` 라 위 (a)~(d)와 같은 tier 충돌이 없다 — `Background` tier 안에 서로 겹치는 여러 등록 레이어가 동시에 뜨는 시나리오 자체가 현재 없다(SidePanel 은 고정 도킹, Explorer/Markdown/Html 은 각자 자기 pane/tab 영역에만 그려져 서로 겹치지 않는다). 겹치는 시나리오가 생기면(예: plugin 이 Background tier 에 자유 위치 오버레이를 추가) 이 절의 (b)~(d) 메커니즘을 그대로 적용할 수 있다.

### `plugin_bridge/popup_render.rs` — 의도적 예외

`draw_plugin_popups`(egui-mesh popup 셸, `Id::new("plugin_mesh_popup").with(instance_id)`)도 `ctx.layer_painter(layer_id)` 로 그리는 미등록 레이어라 (c)의 대상처럼 보이지만, **의도적으로 Area 등록하지 않는다**:

- 이 popup 은 스크린 전체를 덮는 scrim(`painter.rect_filled(screen_rect, ...)`)을 그려 모달처럼 동작한다 — 열려 있는 동안 Foreground tier 의 다른 무엇보다도 위에 있는 것이 올바른 동작이고, 미등록 상태(= 항상 tier 최상단)가 정확히 그 성질을 공짜로 준다.
- Area 등록하면서 `enforce_foreground_z_order` 의 4개 대상에 넣지 않으면, 오히려 자연 등록 시점에 따라 이 popup 이 Banner/상태바보다 **아래**로 밀릴 위험이 생긴다(회귀).
- 인터랙션도 다르다 — egui 위젯 트리가 없고 입력을 raw event 로 모아(`collect_mesh_popup_input`) plugin 프로세스로 forward 할 뿐이라, Area 등록의 원 동기(스크롤/hover 라우팅, `docs/dev-guide/popup-implementation.md`)가 애초에 적용되지 않는다.

**갱신 — host popup 과의 z-order 도입 이후, "always top" 은 무조건 성립하지 않는다.** host popup(`file_picker` 등)이 이 plugin popup **보다 나중에** 열리거나 클릭되면, `enforce_host_plugin_popup_z_order`(`src/gfx/gpu/egui_bridge.rs`)가 공유 z_seq(`tasty_host_plugin::next_popup_z_seq()`) 비교로 그 host popup 을 `ctx.set_sublayer()` 를 통해 이 레이어 위로 강제한다 — 이 호출이 parent/child 를 모두 `Areas::order` 에 강제 등록하므로, 그 프레임에 한해 이 레이어도 미등록 상태를 벗어난다. host popup 이 열려 있지 않거나 이 plugin popup 보다 먼저 열렸다면(=z_seq 가 더 작으면) 기존과 동일하게 미등록 상태로 tier 최상단에 남는다. 상세 메커니즘은 [popup.md § Host ↔ Plugin popup z-order](../design/systems/popup.md#host--plugin-popup-z-order).

### `overlay_open` — 정의 3 개, 소비 지점 6 개

"오버레이가 열려 있는가" 는 **하나의 판정이 아니다.** 이름과 모양이 비슷한 정의가 셋 있고,
각자 묻는 질문이 달라 항의 조합도 다르다. 의도된 분화지만, **새 항(특히 무대 같은 전역
상태)을 더할 때 셋을 각각 봐야 하고, 그중 키보드 계열은 소비 지점까지 따로 봐야 한다.**

| 정의 | 조합 | 묻는 질문 |
|------|------|-----------|
| `AppState::keyboard_overlay_open()` (`src/state.rs`, 순수 술어는 같은 파일 하단) | settings + input dialog + focused host popup + plugin popup | 키/IME 를 host egui 로 들여보낼지(= 터미널 포워딩을 막을지) |
| `MainView::mouse_overlay_open()` (`src/view/main/mouse.rs`) | settings + **무대** | 이 마우스 이벤트를 뒤 세계 좌표로 처리할지 |
| `AppState::has_egui_overlay_open()` (`src/state.rs`) | dialog + plugin popup + popup(visible) + **무대** + tutorial | WebView(OS 네이티브 자식 뷰)를 숨길지 |

왜 조합이 다른가:

- **키보드/IME 경로** 는 "이 키 이벤트를 egui 로 줄지, 중앙 디스패처(터미널/단축키)로 줄지" 를 결정하는 라우팅 전제 질문이다. 이 앱은 키를 기본적으로 egui 에 주지 않으므로, "지금 텍스트 입력을 받는 오버레이가 있는가" 라는 넓은 정의가 필요하다.
- **마우스** 는 `src/view/main.rs` 의 이벤트 분기에서 **항상 무조건** egui 로 먼저 전달되고 `egui_consumed` 로 결과를 받는다 — 라우팅 전제 자체가 없다. Popup 위 클릭은 이미 `egui_consumed`/`popup_hovered`(위치 기반)로 정확히 처리되므로, 여기 남은 항은 **모달(별도 OS 창) 전용** 보강 게이트일 뿐이다. `has_input_dialog_open()`(rename, popup 시스템으로 구현됨)과 `popups.has_focused()` 는 정책상 Popup 이 비모달이라 위치 밖 클릭까지 막을 이유가 없어 안 들어간다.
- **WebView** 는 입력이 아니라 **표시** 질문이다. WebView 는 OS 네이티브 자식 뷰라 wgpu 표면 **위**에 있어 "안 그리는 것" 만으로는 사라지지 않는다 — `set_visible(false)` 가 필요하고 그 게이트가 이 함수다. 그래서 popup 을 `has_focused()` 가 아니라 `has_visible_open()` 으로 넓게 본다.

**plugin egui-mesh popup 의 키보드 계층**: 이 popup 은 host `PopupManager` 소속이 아니라 `popups.has_focused()` 로 잡히지 않지만, 키보드 계층에서는 **focused host popup 과 동급**이다 — 열려 있으면 키/IME 가 egui 로 들어가고 터미널로는 안 간다. 그 **키와 IME 조합**은 `collect_mesh_popup_input` 이 `ctx.input` 에서 긁어 plugin 프로세스로 forward 하므로, 게이트가 닫혀 있으면 forward 소스가 비어 입력이 통째로 터미널로 샌다. IME 는 조합 세션 네 단계를 전부 나른다 — Commit 만 실으면 egui `TextEdit` 이 조합 결과를 조용히 버린다([egui-mesh-channel § 입력 게이트](../dev-guide/egui-mesh-channel.md)). 게이트 지점(winit 이벤트 핸들러)은 `PluginManager` 에 접근할 수 없어 `AppState.plugin_popup_open` 캐시를 읽는다 — 마우스 쪽 `popup_hovered` 와 같은 프레임 간 전달 패턴이다(위 "프레임 간 전달" 절). 같은 술어를 IME 라우팅(`view::main::ime`)과 plugin surface 단축키 게이트(`app::plugin_glue::shortcut`)도 공유한다. 예외는 `set_ime_allowed` 판정(`gfx/gpu.rs`) 하나 — plugin popup 은 host egui 위젯이 없어 IME 를 끄면 popup 안에서 조합 입력을 못 하게 되므로 제외한다. 겹친 popup 중 **누가** 키를 갖는지는 [popup.md § Host ↔ Plugin popup z-order](../design/systems/popup.md#host--plugin-popup-z-order) 의 Esc 소유권 규칙을 따른다.

#### 무대는 어느 정의에도 자동으로 얹히지 않는다

`has_egui_overlay_open` 에 무대가 들어가 있어도 나머지 둘은 각자의 식을 본다 — 그 함수의
프로덕션 소비처는 WebView 표시 하나뿐이다. 게다가 `keyboard_overlay_open()` 은 **정의가
하나인데 소비 지점이 여섯**이고, 그 여섯이 무대에 대해 같은 답을 필요로 하지 않는다. 그래서
무대는 지점마다 명시적으로 배선한다 — 아래 여덟 곳이 전부다.

| # | 지점 | 무대 항 | 근거 |
|---|------|--------|------|
| 1 | `src/view/main.rs` egui feed 게이트 | `\|\| fullscreen_stage_active()` | **방향이 반대**다. 무대 콘텐츠는 egui 위젯이라 키/IME 가 egui 입력 시스템에 들어가야 클릭·텍스트 입력이 산다. 여기는 "무대**로** 준다" |
| 2 | `src/view/main/keyboard.rs` 터미널 포워딩 게이트 | **없음** | 같은 함수 **앞**의 0단계 게이트(`try_consume_fullscreen_stage_key`)가 무대 키를 전부 소비하고 return 하므로 여기까지 오지 않는다. 그 게이트가 사라지면 이 식도 무대 항이 필요해진다 |
| 3 | `src/view/main/ime.rs` | `\|\| fullscreen_stage_active()` | 필수. 무대만 떠 있으면 `keyboard_overlay_open()` 의 네 항이 전부 false 라 아무도 안 막고, 조합 중이던 IME 의 Commit 이 뒤 터미널 PTY 로 샌다 |
| 4 | `src/app/plugin_glue/shortcut.rs` plugin 단축키 | `\|\| fullscreen_stage_active()` | 필수. 이 경로는 `dispatch_window_event_to_view` **이전에** 호출되므로(`src/app/event_handler.rs`) 2 의 0단계 게이트가 아예 도달하지 못한다 |
| 5 | `src/app/webview_keys.rs` native webview 포워딩 키 | `\|\| fullscreen_stage_active()` | 필수. webview 자식 창에서 올라온 키는 winit `KeyboardInput` 경로를 타지 않아 2 의 0단계 게이트를 거치지 않는다([ADR-0102](../adr/0102-webview-key-forwarding.md)) |
| 6 | `mouse_overlay_open()` 정의 | 정의에 포함 | 마우스 다섯 호출부 전부가 이 하나를 본다 |
| 7 | `has_egui_overlay_open()` 정의 | 정의에 포함 | WebView 가 무대 위로 뚫고 나오지 못하게 |
| 8 | `src/adapters/ipc/handler/debug_state.rs` `ui.state` 의 `keyboard_shortcuts_gated` | `fullscreen_stage_active() \|\|` | 게이트가 아니라 **게이트의 보고**다. 그래도 무대 항이 필요하다 — 이 필드가 답하는 물음이 "단축키가 매처에 닿았는가" 인데, 무대는 2 의 0단계에서 먼저 소비한다. 무대 항을 빼면 무대 중에 거짓으로 "안 막혔다" 를 말하고, 그러면 시험이 왜 단축키가 안 먹었는지를 다시 못 보게 된다 |

1·3·4·5 가 같은 항을 각자 OR 하는 모양이라 "`keyboard_overlay_open()` 정의 안으로 넣으면
되지 않나" 가 자연스러운 질문이다. 넣지 않은 이유는 2 다 — 그 지점은 무대에 대해 다른
답(0단계 게이트가 이미 처리)을 쓰고 있고, 정의를 바꾸면 이 술어의 의미가 "키보드 오버레이"
에서 "키보드 오버레이 또는 무대" 로 넓어져 앞으로의 호출자에게도 그 결정이 따라붙는다.
대신 **완전성은 테스트가 강제한다** — `crates/tasty-doc-guards/tests/fullscreen_stage_input_gate.rs` 의
`every_overlay_open_composite_is_stage_aware` 가 `keyboard_overlay_open()` 호출부를 소스에서
기계적으로 전부 찾아 (a) 각각이 무대를 아는지, (b) 지점 집합이 위 표와 같은지를 확인한다.
새 호출부가 생기면 그 테스트가 먼저 깨지고, 그때 이 표도 함께 갱신한다. 2 의 예외도 그
테스트가 "0단계 게이트가 살아 있는가" 로 함께 검증한다.

알려진 미해소 사안: popup 바깥 클릭이 popup 을 닫으면서, 그 클릭이 겨냥한 하위 액션도 같은 클릭에서 함께 발생한다. 닫힘만 소비하고 액션을 막을지는 UX 판단이 필요해 별개 사안으로 분리돼 있다.

### 새 Foreground 레이어를 추가할 때 체크리스트

1. `egui::Area::new(...).order(Order::Foreground)...show(ctx, |ui| { ... })` 로 그린다 — bare `Ui::new(...).layer_id(...)` 금지((c) 참고).
2. 이 레이어가 위 정책 표의 어느 순서에 들어가는지 정하고, `enforce_foreground_z_order` 의 `set_sublayer` 체인에 끼워 넣는다. 기존 두 그룹(Banner↔{상태바,탭바}, Modifier-hint↔Popup) 중 하나에 합류시키거나, 새 그룹을 만들 때는 **1단 들여쓰기 제약**((d) 참고)을 넘지 않게 그룹을 겹치지 않게 나눈다.
3. 정말 모달처럼 항상 최상단이어야 하는 예외라면(예: 위 plugin popup) Area 미등록을 의도적으로 유지하고 그 이유를 주석/문서로 남긴다.
4. 겹치는 배치를 만들어 `tasty screenshot` 으로 실측 확인한다(디자인 시나리오만으로 단정하지 않는다, `docs/dev-guide/self-verification.md`).

**전체화면 무대는 이 체크리스트의 2 번 대상이 아니다.** 무대(`src/adapters/ui/fullscreen.rs`)는
1 번대로 등록된 `Area`(`Order::Foreground`)로 그리지만 `enforce_foreground_z_order` 체인에는
들어가지 않는다 — 무대는 **별개 프레임**에서 그려지고(`Gpu::render` 의 무대 분기) 그 프레임에는
host chrome·popup·오버레이가 아예 그려지지 않아 같은 tier 안에서 순서를 다툴 상대가 없기
때문이다. 3 번(Area 미등록으로 최상단을 얻는 예외)에 기대지 않은 것도 같은 이유다. 상세
[`design/systems/fullscreen-stage.md`](../design/systems/fullscreen-stage.md).

## 관련

- [popup 시스템](../design/systems/popup.md) — 팝업 z-order·스코프·포커스
- [전체화면 무대](../design/systems/fullscreen-stage.md) — 별개 프레임으로 그려지는 창 독점 표면
- [banner 시스템](../design/systems/banner.md) — 배너 마우스 소비·스코프·발화 정책
- [dev-guide/popup-implementation](../dev-guide/popup-implementation.md) — 팝업 추가 절차
- [multi-window](multi-window.md) — 윈도우 단위 구조
