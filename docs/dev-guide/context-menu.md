# 컨텍스트 메뉴 구현

**모든 우클릭 컨텍스트 메뉴는 OS 네이티브 메뉴를 쓴다.** egui `Area`/`Window`/`menu` 로 자체 구현 금지.

이유: 네이티브 메뉴는 클릭 위치에 표시되고 WebView 같은 네이티브 자식 뷰 위에 놓인다. 폰트·애니메이션·접근성도 OS와 일관되게 제공한다.

## surface 컨텍스트 메뉴 생산자 (terminal vs 비-terminal)

surface 우클릭 컨텍스트 메뉴는 surface 종류에 따라 생산 경로가 갈린다:

- **terminal**: winit의 `src/view/main/mouse.rs::handle_right_button`이 메뉴를 만든다.
  이 함수가 mouse-tracking 위임 여부도 판단한다(docs/surfaces/terminal/index.md).
  `terminal_menu_open_state`는 **Linux에서는 release**, **macOS/Windows에서는 press**에 메뉴를 연다.
  Linux에서는 버튼을 누른 상태로 GTK `popup_at_rect()`를 호출하면 메뉴가 표시되지 않을 수 있다.
  macOS/Windows의 `show_context_menu`는 `MenuOutcome::Ready`를 동기적으로 반환하므로
  기존 press 시점을 사용한다. Linux도 입력을 받는 곳은 winit이며, 메뉴를 여는 시점만 다르다.
- **terminal 링크 위**: `handle_right_button`은 mouse-tracking 위임보다 링크를 먼저 확인한다.
  링크 위라면 `PendingNativeMenu::TerminalLink`를 설정하고(`src/view/main/link_menu.rs`),
  press와 release를 터미널에 전달하지 않는다. 메뉴를 여는 시점은 위 플랫폼별 규칙을 따르지만,
  링크 정보는 press 때 저장한다. Linux에서 release까지 기다리는 동안 hover 대상이 바뀌어도
  처음 누른 링크의 메뉴를 열기 위해서다. 명세는 [terminal-link](../features/terminal-link/index.md)를 참고한다.
- **비-terminal**(explorer/empty/markdown/image/webview/remote): winit에서는 메뉴를 만들지 않는다.
  egui의 `emit_surface_menu_fallback`(`src/adapters/ui/egui_panels.rs`)이 release 때
  `secondary_clicked()`를 확인해 `PendingNativeMenu::Surface`를 설정한다.
  explorer는 같은 프레임에서 `apply_explorer_action`이 위치별 메뉴를 먼저 선택하며,
  fallback은 `is_none()`일 때만 메뉴를 추가한다.
- **plugin webview 페이지**: 페이지가 OS 우클릭을 받으므로 egui fallback이 닿지 않는다.
  페이지가 `contextmenu`를 막고 위치를 소유 plugin에 넘기면, plugin이 `webview.context_menu`
  (`src/app/dispatch/plugin_webview_menu.rs`)로 `PendingNativeMenu::WebviewSurface`를 세운다. 좌표는 webview 왼쪽 위
  기준 논리 픽셀이고 host가 드러난 webview 영역(`native_content_rects`)으로 창 좌표를 만든다. 항목은 "터미널 ID 복사"
  하나다(`src/view/main/webview_menu.rs`). 고르면 host가 클립보드에 쓰되 토스트는 띄우지 않는다 — 토스트 카드가 겹친
  webview는 토스트가 닫힐 때까지 통째로 숨겨지기 때문이다(`hidden_under_toast`). 대신 소유 plugin에만
  `webview.context_menu_selected`(`surface_id` · `item` · `copied` · 요청한 페이지 좌표 `x` · `y`)를 보내 페이지 안에서
  알리게 한다. 클립보드 쓰기 실패는 host 토스트로 알린다.
- **explorer의 나머지 영역**: 파일 셀, content의 빈 영역, 트리 노드, 즐겨찾기는 각 위치에 맞는
  메뉴를 만든다. `draw_explorer` 마지막에서는 전체 rect를 확인해 툴바·주소창·내부 탭바·상태줄·
  빈 사이드바의 우클릭에도 Empty 메뉴를 제공한다. 파일 브라우저 위에 무관한 Surface 메뉴
  ("터미널 ID 복사")를 표시하지 않기 위해서다.
  `LoadState::NoPermission`에서는 붙여넣기를 할 수 없으므로 이 전체 rect 처리를 건너뛴다.
- **fallback 의 explorer-aware 최후 방어선**: catch-all 이 어떤 이유(런타임 이벤트 프레이밍·좌표 미스 등)로 explorer 슬롯을 못 세우고 `emit_surface_menu_fallback` 이 발화하더라도, fallback 은 **explorer surface 위에서는 generic `Surface` 대신 빈영역 `Explorer` 메뉴**(paths 빈 vec + cwd = current_root)를 세운다. 첫 패스에서 각 패널의 explorer 여부와 `current_root` 를 `EguiPanelInfo.explorer_cwd` 로 캡처해 판별한다. 그래서 explorer 위에는 어떤 경로로도 surface-op 메뉴가 노출되지 않고, 사용자는 항상 explorer 메뉴를 받는다. OS 무관 순수 로직(`#[cfg]` 불필요)이며, explorer 위 generic 메뉴는 원래 어느 OS 에서도 뜨면 안 되므로 플랫폼 공통으로 같은 선택 규칙을 적용한다.

비-terminal 메뉴를 winit press와 egui release 양쪽에서 만들면 generic 메뉴가
위치별 메뉴를 먼저 차지할 수 있다. 이를 피하기 위해 egui release 경로 한 곳에서 만든다.

## 2단계 패턴

egui render 루프 안에서는 OS 네이티브 메뉴를 직접 호출할 수 없다(`winit::Window` 참조 불가). 우클릭 시점에 정보+좌표만 저장하고, MainView 가 꺼내 표시한다.

### 1. egui 에서 우클릭 감지 → `PendingNativeMenu` 설정 (`src/state/dialogs.rs`)

```rust
pub enum PendingNativeMenu {
    Tab { pane_id: u32, tab_index: usize, x: f32, y: f32 },
    Pane { pane_id: u32, x: f32, y: f32 },
    ExplorerFavorite { surface_id: u32, path: PathBuf, x: f32, y: f32 },
    // … (전체 목록은 src/state/dialogs.rs)
    // 새 메뉴 유형 → variant 추가 (좌표 x/y 필수)
}
```

```rust
if resp.secondary_clicked() {
    let pos = resp.interact_pointer_pos().unwrap_or_default();
    state.dialogs.pending_native_menu = Some(PendingNativeMenu::MyMenu { data, x: pos.x, y: pos.y });
}
```

### 2. MainView 에서 표시 + 결과 처리 (`src/view/main/redraw.rs`)

`process_pending_native_menu()` 가 `PendingNativeMenu` 를 꺼내 메뉴를 띄우고, 선택 결과 처리는
**continuation 으로 예약**한다. 메뉴 표시는 반드시 `open_native_menu` 를 거친다 —
`show_context_menu` 를 직접 부르면 Linux 의 `Pending` 을 흘려 메뉴가 뜬 채 남는다.

```rust
PendingNativeMenu::MyMenu { data, x, y } => {
    let items = [MenuItem::new(1, "Action A"), MenuItem::new(2, "Action B")];
    self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
        // Linux 는 continuation 이 여러 프레임 뒤에 실행된다 — 그 사이 대상이
        // 사라졌을 수 있으므로 대상 id 유효성을 여기서 다시 확인하고 무효하면 반환.
        if !engine.has_surface(surface_id) {
            return;
        }
        match result {
            Some(1) => { /* A */ }
            Some(2) => { /* B */ }
            _ => {}
        }
    });
}
```

`mark_dirty()` 는 `open_native_menu` / 폴링 완료 경로가 대신 호출하므로 continuation 안에서
따로 부르지 않아도 된다.

## 네이티브 메뉴 API (`crates/tasty-platform/src/native_menu/`)

| 함수 | 설명 |
|------|------|
| `show_context_menu(window, x, y, items)` | 좌표에 메뉴 표시 → `MenuOutcome` |
| `MenuOutcome::Ready(Option<u32>)` | 메뉴가 이미 끝났다 — 선택 ID(없으면 `None`) |
| `MenuOutcome::Pending(MenuHandle)` | 메뉴가 떠 있다 — 매 프레임 `poll()` 로 결과 회수 |
| `MenuHandle::poll()` | 비블로킹 펌프 1 회. `None`=아직 열림 / `Some(result)`=닫힘 |
| `MenuHandle::dismiss()` | 바깥 클릭 등으로 강제 닫기(결과는 다음 `poll()` 로 나온다) |
| `MenuItem::new(id, label)` / `disabled(id, label)` / `separator()` | 항목 / 비활성 / 구분선 |

`macos.rs` / `windows.rs` / `linux.rs` 가 플랫폼 구현.


### 선택적 단색 아이콘

`MenuItem::with_icon(MenuIcon::from_svg(svg, size))`로 아이콘을 전달한다. 기본값은
`None`이며 기존 label·enabled·tooltip·shortcut·구분선과 선택 결과는 그대로다.
`svg`는 `tasty_icons::EXTERNAL.svg`나 `FOLDER_OPEN.svg` 같은 기존 정본 도형이고,
`size`는 `tasty_type_geometry::length::LogicalPx`다. 메뉴 아이콘의 디자인 크기는
`SIZING.icon_glyph_size_md`를 사용한다. OS 메뉴는 앱의 egui zoom 대상이 아니므로
zoom 전 토큰을 전달하고 어댑터가 표시 시점의 native backing scale을 적용한다.
GPU texture나 egui Image를 OS에 넘기지 않는다.

SVG는 공용 아이콘 로더와 같은 resvg로 원하는 물리 크기에 래스터한다. 도형의 알파만
사용하고 잉크·배경·선택·비활성 표현은 native 메뉴에 맞춘다. 잘못된 SVG/크기는
경고 로그를 남기며 아이콘만 생략한다. 아이콘 실패가 메뉴 동작을 막지 않는다.

- macOS: 현재 NSView 윈도우의 backingScaleFactor로 PNG를 만들고 NSImage의 논리 크기를
  지정한다. template 이미지이므로 AppKit이 밝기·선택·disabled 잉크를 정한다.
  NSMenuItem이 이미지를 retain하므로 임시 PNG/로컬 참조가 사라져도 메뉴 수명 동안 유효하다.
- Windows: 현재 HWND의 DPI/96으로 top-down 32-bit premultiplied BGRA DIB를 만든다.
  MIIM_BITMAP으로 native 메뉴에 연결하고 TrackPopupMenu 종료·DestroyMenu 뒤 DeleteObject로
  해제한다. 항목이 비활성이면 COLOR_GRAYTEXT, 아니면 COLOR_MENUTEXT를 메뉴 생성 시 읽는다.
  텍스트·선택 배경은 OS가 그리며 bitmap 잉크는 메뉴가 열린 동안 고정된다. owner-draw 메뉴나
  앱 popup으로 바꾸지 않는다. DestroyMenu 실패 때는 빌린 bitmap의 조기 해제를 피하고 경고한다.
- Linux: 기존 GTK MenuItem의 행 안에 DrawingArea를 붙인다. GTK의 현재 backing scale에서
  래스터하고 메뉴 항목의 상태별 style color를 알파 mask에 적용해 disabled/선택 잉크를 따른다.
  아이콘과 라벨 사이는 시안 MenuItem의 icon–label gap인 `SIZING.spacing_sm`(zoom 전 원본)을 GTK 여백으로
  둔다. 행이 메뉴에 붙기 전에는 GTK style padding이 0이라 테마 값으로 대신할 수 없고, 메뉴 창 배율은
  GTK가 적용한다. widget은 메뉴가 소유하고 draw closure는 메뉴 항목을
  weak 참조해 순환 소유를 만들지 않는다. Pending/poll/dismiss 계약은 변하지 않는다.

아이콘 유닛 검사는 알파·배율·잘못된 입력·기존 builder 계약을 검사한다. Windows의
bitmap 검사는 숨긴 소유 윈도우로 backing 크기와 해제를 확인한다. Linux의 ignored
`icon_menu_keeps_async_selection_and_disabled_rows`는 전용 X11에서 실제 포인터로
첫 행을 선택해야 끝나며 아이콘 행의 간격도 확인한다. 같은 조건의 ignored
`icon_keeps_the_design_gap_before_its_label`은 행을 메뉴에 붙인 뒤 간격이 `SIZING.spacing_sm`인지 본다.
이 검사들은 제품 입력 API를 추가하지 않는다. 테스트 존재나
컴파일 통과를 해당 OS의 화면 실측으로 보고하지 않는다.

<a id="해소-타이밍은-플랫폼별로-다르다-docs/dev-guide/agent-runner.md"></a>

### 메뉴 선택을 처리하는 시점은 플랫폼별로 다르다

API 형태는 세 OS 가 같지만 **언제 결과가 나오는지**는 다르다:

- **macOS / Windows**: 항상 `Ready`. NSMenu / `TrackPopupMenu` 가 메인 윈도우의 런루프·메시지
  펌프 안에서 메뉴를 트래킹하므로 호출이 돌아온 시점에 이미 끝나 있다 — continuation 이 같은
  프레임에 실행된다(예전 동기 계약과 타이밍 동일).
- **Linux**: 항상 `Pending`. GTK 는 자기 main context 를 따로 돌려야 하는데, 그 루프를 winit
  콜백 안에서 돌리면 winit 이벤트 루프가 그동안 통째로 막힌다(grab 실패 시 최대 30 초 — WM
  "응답 없음" 배너 + 입력/렌더 정지). 그래서 팝업만 띄우고 즉시 반환한다.

호출부(`MainView::pending_menu` + `poll_pending_native_menu`)가 두 경로를 모두 다루므로 새
메뉴를 추가할 때 플랫폼 분기를 쓸 일은 없다. 결정 근거·대안은
[docs/design/systems/popup.md](../design/systems/popup.md).

### Linux 구현 (GTK 3, X11 only)

winit 이 소유한 창은 GTK 소유가 아니라 기본적으로 대응하는 `GdkWindow` 가 없다. `linux.rs` 는 winit 창의 raw X11 XID 를 `gdkx11::X11Window::foreign_new_for_display` 로 감싼 foreign `GdkWindow` 를 만들어 `menu.popup_at_rect()` 의 `rect_window` 로 넘긴다(`host_api/webview/linux.rs` 와 동일 패턴). 이게 없으면 `gtk_menu_popup_at_rect: assertion 'GDK_IS_WINDOW (rect_window)' failed` 로 팝업 자체가 무효화된다.

**바깥 클릭 dismiss**: 진짜 트리거 `GdkEvent`(winit → egui 로 이미 소비된 뒤라 합성 불가)가 없어 `trigger_event` 는 항상 `None` 이고, `popup_at_rect` 내부의 자체 grab 은 `Gtk-WARNING: no trigger event for menu popup` 와 함께 사실상 no-op 이다. 이를 보완하려고 `linux.rs` 가 팝업이 실제로 mapped 된 직후(`glib::idle_add_local_once` 로 `popup_at_rect` 호출 다음 tick 에 지연 실행 — widget 의 `map` 시그널 핸들러 안에서 하면 아직 서버에 안 떴을 수 있어 `GrabNotViewable` 로 실패한다) `gdk::Seat::grab()` 으로 직접 포인터/키보드를 잡는다.

- **raw Xlib grab(`XGrabPointer`/`XGrabKeyboard`) 이 아니라 `gdk::Seat::grab()` 을 쓰는 이유**: GDK3 의 X11 백엔드는 XInput2(XI2) 로만 이벤트를 받는다. raw core-protocol grab 으로 리다이렉트된 클릭은 X11 프로토콜 레벨에서는 메뉴 창으로 도착하지만 GDK 의 이벤트 소스가 XI2 이벤트만 인식하므로 `GdkEventButton` 으로 변환되지 않아 GTK 위젯 로직(activate, 우리가 추가한 바깥 클릭 감지 핸들러 모두)에 전혀 도달하지 못한다(raw grab 으로 실제 테스트해 확인된 사실).
- **`owner_events=true` 로 잡는 이유**: `false` 로 잡으면 메뉴 "안" 클릭까지 전부 grab 창(메뉴 자신)으로 강제 리다이렉트되면서 GTK 내부 hit-test 가 깨져 항목 클릭(activate)이 아예 씹힌다(실측 회귀 — Rename 클릭 시 다이얼로그가 안 뜸). `true` 로 두면 자신이 소유한 서브윈도우 위의 클릭은 정상 라우팅되어 항목 클릭이 그대로 동작하고, 소유하지 않은 다른 창(tasty 메인 창 등) 위의 클릭만 grab 창으로 리다이렉트된다.
- 리다이렉트된 바깥 클릭은 `menu.connect_button_press_event` 핸들러가 좌표를 메뉴 자신의 allocation 과 비교해 밖이면 직접 `popdown()` 한다 — GTK 의 기본 deactivate 로직은 (트리거 이벤트 없이 잡은) grab 소유 여부를 스스로 신뢰하지 못해 기대할 수 없다.
- `Seat::grab` 이 실패(`GrabStatus::Success` 가 아님)해도 패닉하지 않고 경고만 남긴 뒤 grab 없이 진행한다 — 이 경우 바깥 클릭이 GTK 에 도달하지 않지만, **그 클릭은 winit 이 받으므로** `mouse.rs` 의 press 핸들러가 `MenuHandle::dismiss()` 를 호출해 메뉴를 닫는다(아래 "메뉴를 닫는 클릭 처리" 참고). 즉 grab 실패해도 바깥 클릭 dismiss 는 그대로 동작하고, 메뉴 자체나 항목 클릭도 영향받지 않는다.
- 완료(선택/취소/타임아웃) 시 `grabbed` 가 true 였으면 반드시 `seat.ungrab()` 으로 대칭 해제한다. 결과를 회수하지 않고 핸들이 버려지는 경로(창 종료 등)는 `Drop` 이 `popdown()` + 같은 해제를 수행한다.

**메뉴를 닫는 클릭 처리**: 바깥 클릭으로 메뉴를 닫을 때는 press와 짝이 되는 release를
모두 egui 입력 큐와 Tasty 입력 처리에 전달하지 않는다. `view/main.rs`의 `handle_event`가
각 이벤트를 egui에 넣기 전에 `mouse.rs::take_menu_dismiss_swallow`와
`menu_dismiss_swallow_step`으로 판단한다. egui 입력 처리와 `handle_mouse_input`은 이 결과를
함께 사용한다. 제외한 이벤트 대신 `GpuState::push_egui_pointer_gone`으로 `PointerGone`을
넣어 기존 hover 표시도 해제한다.

press만 제외하고 release를 전달하면 egui가 `clicked()`를 만들 수 있다.
우클릭이면 `secondary_clicked()`로 새 메뉴를 열 수도 있어, 메뉴를 닫으려던 클릭이
아래의 사이드바·탭·explorer 항목까지 실행하게 된다. 따라서 release도 제외해야 한다.

release를 제외할지는 메뉴의 현재 표시 여부가 아니라 앞선 press 기록으로 판단한다.
press로 메뉴를 닫은 뒤 release 전에 `poll_pending_native_menu`가 메뉴 슬롯을 비울 수 있기 때문이다.
`mouse.rs`의 `menu_dismiss_tests`가 이 상태 전이를 검사하며, `egui::Context`와 대조군을 사용해
제외한 클릭이 `clicked()`를 만들지 않는지도 확인한다.

**비블로킹 폴링**: `GtkMenuHandle::poll()` 은 `while gtk::events_pending() { main_iteration_do(false) }` 로 큐에 있는 것만 처리하고 즉시 반환한다. `MainView::poll_pending_native_menu` 가 두 곳에서 이를 호출한다 — `handle_redraw`(같은 프레임 안에서 `process_pending_native_menu` 보다 **먼저**, 방금 닫힌 메뉴의 뒤처리가 다음 메뉴 요청을 막지 않게)와 `about_to_wait`(메뉴가 떠 있는 동안 8ms `WaitUntil` 로 재예약되며 돌아, redraw 이벤트가 없는 순간에도 폴링이 끊기지 않게). 이 `WaitUntil` 재예약을 빠뜨리면 메뉴가 열린 채 폴링이 멈춘다. 트레이(AppIndicator) GTK 펌프도 메뉴가 떠 있으면 함께 돈다.

**30초 워치독**: `selection-done` 이 30초 안에 안 오면 강제로 `popdown()` 한다. 비동기 계약으로 바뀐 뒤 이 워치독의 의미는 "앱 프리즈 방지"가 아니라 **닫히지 않는 메뉴 방지**다 — 아무도 닫을 수 없는 팝업이 화면에 남고 continuation 이 그 뒤에 묶이는 것을 막는다. 위 winit press dismiss 경로가 있으므로 실제로는 거의 발생시키지 않는다. 타임아웃 경고 로그에는 실제 `grabbed` 값이 함께 찍힌다.

**debug 훅** (`#[cfg(debug_assertions)]`, release 미노출): grab 실패는 실제 하드웨어 클릭에서만 재현되고 합성 입력으로는 재현되지 않아 일반적인 회귀 테스트를 만들 수 없다. 대신 두 환경변수로 실패 경로를 결정적으로 만들 수 있다.

| 환경변수 | 효과 |
|----------|------|
| `TASTY_DEBUG_NATIVE_MENU_FORCE_GRAB_FAIL` | grab 을 시도하지 않아 "grab 실패" 상태를 강제 |
| `TASTY_DEBUG_NATIVE_MENU_TIMEOUT_MS` | 30초 워치독을 짧은 값으로 override |

`crates/tasty-platform/src/native_menu/linux.rs` 의 `#[ignore]` 테스트(`forced_grab_failure_resolves_via_watchdog_without_blocking`)가 이 둘을 써서 "즉시 반환 · 폴링 비블로킹 · 워치독 해소"를 실제 GTK 백엔드로 검증한다. 실행에는 X11 디스플레이가 필요하다:

```
cargo test -p tasty-platform --lib --features gui -- --ignored --test-threads=1 native_menu::linux
```

패키지와 `--features gui` 를 둘 다 줘야 한다. 이 모듈은 `tasty-platform` 의 기본
feature 에 없으므로, 둘 중 하나라도 빠지면 이름 필터에 **실행된 시험이 0개인데도 성공**으로 끝난다 —
`--ignored` 와 이름 필터를 함께 쓰는 조합이라 0 건이 정상처럼 보인다.

## 새 메뉴 체크리스트

1. `PendingNativeMenu` variant 추가(좌표 `x/y` 필수).
2. egui `secondary_clicked()` → `pending_native_menu` 설정.
3. `process_pending_native_menu()` 에 match 분기 → 핸들러에서 `open_native_menu(x, y, &items, cont)` 호출(`show_context_menu` 직접 호출 금지).
4. 대상은 인덱스가 아니라 ID로 캡처하고, continuation 시작부에서 현재 위치를 다시 찾는다. 메뉴가 열려 있는 동안 에이전트가 대상을 닫거나 순서를 바꿀 수 있다. 사라졌으면 반환한다. workspace·탭 메뉴는 `src/view/main/menu_target.rs`의 `WorkspaceMenuTarget`·`TabMenuTarget`을 쓴다.
5. 항목 텍스트는 `crate::i18n::t()` + `lang/{en,ko,ja,zh-Hans}.toml` 키 추가.
