# 컨텍스트 메뉴 구현

**모든 우클릭 컨텍스트 메뉴는 OS 네이티브 메뉴를 쓴다.** egui `Area`/`Window`/`menu` 로 자체 구현 금지.

이유: 네이티브 메뉴는 클릭 위치 고정(egui Area 는 마우스 추종) · WebView 등 네이티브 자식 뷰 위에 올바르게 렌더 · OS 일관성(폰트/애니메이션/접근성).

## surface 컨텍스트 메뉴 생산자 (terminal vs 비-terminal)

surface 우클릭 컨텍스트 메뉴는 surface 종류에 따라 생산 경로가 갈린다:

- **terminal**: winit 경로(`src/view/main/mouse.rs` `handle_right_button`)가 생산한다. mouse-tracking 위임(ADR-0019/0022) 판정이 여기 있어 winit-level 이어야 한다. 큐잉 트리거 시점은 플랫폼별로 다르다(`terminal_menu_open_state` 의 `cfg!(target_os = "linux")` 분기): **Linux 는 release**, **macOS/Windows 는 press** 그대로. Linux 전용인 이유 — 버튼이 아직 눌려 있는 상태에서 GTK `popup_at_rect()` 를 부르면 팝업을 realize/map 하지 않고 조용히 no-op 한다(실측 확인: `has_window`/`realized`/`mapped` 가 영원히 false 로 고정, X11 이벤트 큐에 CreateWindow/MapWindow 요청 자체가 안 나감). macOS/Windows 는 `show_context_menu` 가 항상 `MenuOutcome::Ready` 로 동기 처리돼 이 제약 자체가 없어 기존 press 시점 동작을 그대로 둔다(불필요한 플랫폼 공통 동작 변경 방지). 비-terminal 이 egui `secondary_clicked()`(release 시점)로 이 GTK 문제를 애초에 안 겪는 것과 같은 이유로 Linux 만 맞춘 것 — layer 는 winit 그대로 유지하되 트리거 시점만 플랫폼별로 갈린다.
- **terminal 링크 위**: 같은 `handle_right_button` 이 mouse-tracking 위임 판정 **앞**에서 hover 링크를 먼저 본다. 링크면 `PendingNativeMenu::TerminalLink` 를 세우고(`src/view/main/link_menu.rs`) press·release 둘 다 로컬 소비한다 — 좌클릭 링크가 위임보다 먼저인 것과 대칭이다. 메뉴를 세우는 버튼 상태는 위와 같지만 링크 스냅샷은 press 에서 찍는다(Linux 는 release 까지 hover 가 바뀔 수 있다). 명세는 [terminal-link](../features/terminal-link/index.md).
- **비-terminal**(explorer/empty/markdown/image/webview/remote): winit 은 메뉴를 만들지 않고(`return`) **egui 프레임이 단일 생산자**다. `emit_surface_menu_fallback`(`src/adapters/ui/egui_panels.rs`)이 release 시점 `secondary_clicked()` 로 발화해 `PendingNativeMenu::Surface` 를 세팅한다. explorer 는 같은 프레임 안에서 `apply_explorer_action` 이 위치별 메뉴를 먼저 선점하고, fallback 은 `is_none()` 가드로 이를 존중한다.
- **explorer 표면 전체 커버리지**: explorer 는 위치별 메뉴(그리드 파일 셀 → 파일 메뉴, content 빈 영역 → Empty 메뉴, 트리 노드/즐겨찾기 → 각 메뉴)에 더해, `draw_explorer` 끝에서 **표면 전체 rect catch-all** 로 나머지 chrome(툴바/주소창/내부 탭바/상태줄/빈 사이드바)의 우클릭도 Empty 메뉴로 선점한다. 이로써 generic Surface fallback("터미널 ID 복사")이 explorer 위 어디에서도 뜨지 않는다(불가침 원칙 §1·§2 — 파일 브라우저에 무관한 surface-op 메뉴 노출 금지). 권한 거부 루트(`LoadState::NoPermission`)만 예외로, 붙여넣기가 무의미하므로 catch-all 을 건너뛴다.
- **fallback 의 explorer-aware 최후 방어선**: catch-all 이 어떤 이유(런타임 이벤트 프레이밍·좌표 미스 등)로 explorer 슬롯을 못 세우고 `emit_surface_menu_fallback` 이 발화하더라도, fallback 은 **explorer surface 위에서는 generic `Surface` 대신 빈영역 `Explorer` 메뉴**(paths 빈 vec + cwd = current_root)를 세운다. 첫 패스에서 각 패널의 explorer 여부와 `current_root` 를 `EguiPanelInfo.explorer_cwd` 로 캡처해 판별한다. 그래서 explorer 위에는 어떤 경로로도 surface-op 메뉴가 노출되지 않고, 사용자는 항상 explorer 메뉴를 받는다. OS 무관 순수 로직(`#[cfg]` 불필요)이며, explorer 위 generic 메뉴는 원래 어느 OS 에서도 뜨면 안 되므로 macOS 정상 경로 회귀도 불가능하다.

이렇게 나눈 이유: 과거 winit(press 시점)과 egui(release 시점) 두 생산자가 같은 슬롯을 두고 경합했는데, 중앙 surface 위에서는 `egui_consumed` 가 구조적으로 항상 false 라 winit press 가 늘 먼저 generic Surface 메뉴를 선점 → explorer 위치별 메뉴가 실행되지 못했다. 비-terminal 생산을 egui release 단일 경로로 일원화해 이 경합을 제거했다.

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
    self.open_native_menu(x, y, &items, move |this, result| {
        // Linux 는 continuation 이 여러 프레임 뒤에 실행된다 — 그 사이 대상이
        // 사라졌을 수 있으므로 대상 id 유효성을 여기서 다시 확인하고 무효하면 반환.
        if !this.core_state.has_surface(surface_id) {
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

### 해소 타이밍은 플랫폼별로 다르다 (ADR-0071)

API 형태는 세 OS 가 같지만 **언제 결과가 나오는지**는 다르다:

- **macOS / Windows**: 항상 `Ready`. NSMenu / `TrackPopupMenu` 가 메인 윈도우의 런루프·메시지
  펌프 안에서 메뉴를 트래킹하므로 호출이 돌아온 시점에 이미 끝나 있다 — continuation 이 같은
  프레임에 실행된다(예전 동기 계약과 타이밍 동일).
- **Linux**: 항상 `Pending`. GTK 는 자기 main context 를 따로 돌려야 하는데, 그 루프를 winit
  콜백 안에서 돌리면 winit 이벤트 루프가 그동안 통째로 막힌다(grab 실패 시 최대 30 초 — WM
  "응답 없음" 배너 + 입력/렌더 정지). 그래서 팝업만 띄우고 즉시 반환한다.

호출부(`MainView::pending_menu` + `poll_pending_native_menu`)가 두 경로를 모두 다루므로 새
메뉴를 추가할 때 플랫폼 분기를 쓸 일은 없다. 결정 근거·대안은
[`docs/adr/0071-native-context-menu-async-contract.md`](../adr/0071-native-context-menu-async-contract.md).

### Linux 구현 (GTK 3, X11 only)

winit 이 소유한 창은 GTK 소유가 아니라 기본적으로 대응하는 `GdkWindow` 가 없다. `linux.rs` 는 winit 창의 raw X11 XID 를 `gdkx11::X11Window::foreign_new_for_display` 로 감싼 foreign `GdkWindow` 를 만들어 `menu.popup_at_rect()` 의 `rect_window` 로 넘긴다(`host_api/webview/linux.rs` 와 동일 패턴). 이게 없으면 `gtk_menu_popup_at_rect: assertion 'GDK_IS_WINDOW (rect_window)' failed` 로 팝업 자체가 무효화된다.

**바깥 클릭 dismiss**: 진짜 트리거 `GdkEvent`(winit → egui 로 이미 소비된 뒤라 합성 불가)가 없어 `trigger_event` 는 항상 `None` 이고, `popup_at_rect` 내부의 자체 grab 은 `Gtk-WARNING: no trigger event for menu popup` 와 함께 사실상 no-op 이다. 이를 보완하려고 `linux.rs` 가 팝업이 실제로 mapped 된 직후(`glib::idle_add_local_once` 로 `popup_at_rect` 호출 다음 tick 에 지연 실행 — widget 의 `map` 시그널 핸들러 안에서 하면 아직 서버에 안 떴을 수 있어 `GrabNotViewable` 로 실패한다) `gdk::Seat::grab()` 으로 직접 포인터/키보드를 잡는다.

- **raw Xlib grab(`XGrabPointer`/`XGrabKeyboard`) 이 아니라 `gdk::Seat::grab()` 을 쓰는 이유**: GDK3 의 X11 백엔드는 XInput2(XI2) 로만 이벤트를 받는다. raw core-protocol grab 으로 리다이렉트된 클릭은 X11 프로토콜 레벨에서는 메뉴 창으로 도착하지만 GDK 의 이벤트 소스가 XI2 이벤트만 인식하므로 `GdkEventButton` 으로 변환되지 않아 GTK 위젯 로직(activate, 우리가 추가한 바깥 클릭 감지 핸들러 모두)에 전혀 도달하지 못한다(raw grab 으로 실제 테스트해 확인된 사실).
- **`owner_events=true` 로 잡는 이유**: `false` 로 잡으면 메뉴 "안" 클릭까지 전부 grab 창(메뉴 자신)으로 강제 리다이렉트되면서 GTK 내부 hit-test 가 깨져 항목 클릭(activate)이 아예 씹힌다(실측 회귀 — Rename 클릭 시 다이얼로그가 안 뜸). `true` 로 두면 자신이 소유한 서브윈도우 위의 클릭은 정상 라우팅되어 항목 클릭이 그대로 동작하고, 소유하지 않은 다른 창(tasty 메인 창 등) 위의 클릭만 grab 창으로 리다이렉트된다.
- 리다이렉트된 바깥 클릭은 `menu.connect_button_press_event` 핸들러가 좌표를 메뉴 자신의 allocation 과 비교해 밖이면 직접 `popdown()` 한다 — GTK 의 기본 deactivate 로직은 (트리거 이벤트 없이 잡은) grab 소유 여부를 스스로 신뢰하지 못해 기대할 수 없다.
- `Seat::grab` 이 실패(`GrabStatus::Success` 가 아님)해도 패닉하지 않고 경고만 남긴 뒤 grab 없이 진행한다 — 이 경우 바깥 클릭이 GTK 에 도달하지 않지만, **그 클릭은 winit 이 받으므로** `mouse.rs` 의 press 핸들러가 `MenuHandle::dismiss()` 를 호출해 메뉴를 닫는다(아래 "dismiss 클릭은 사이클 전체가 삼켜진다" 참고). 즉 grab 실패해도 바깥 클릭 dismiss 는 그대로 동작하고, 메뉴 자체나 항목 클릭도 영향받지 않는다.
- 완료(선택/취소/타임아웃) 시 `grabbed` 가 true 였으면 반드시 `seat.ungrab()` 으로 대칭 해제한다. 결과를 회수하지 않고 핸들이 버려지는 경로(창 종료 등)는 `Drop` 이 `popdown()` + 같은 해제를 수행한다.

**dismiss 클릭은 사이클 전체가 삼켜진다**: 메뉴를 닫는 바깥 클릭은 **press 부터 짝이 되는 release 까지** 삼킨다 — egui 입력 큐에도, tasty 자체 라우팅에도 넣지 않는다. 판정과 상태 전이는 `view/main.rs` `handle_event` 가 **egui feed 보다 먼저** 이벤트당 한 번 수행하고(`mouse.rs::take_menu_dismiss_swallow` → 순수 함수 `menu_dismiss_swallow_step`), 그 결과를 egui feed 게이트와 `handle_mouse_input` 이 함께 쓴다. 삼키는 이벤트에는 대신 `GpuState::push_egui_pointer_gone` 이 egui 에 `PointerGone` 을 넣어 hover 잔류를 끊는다.

press 만 삼키면 부족하다: winit 이벤트는 egui 에 **먼저** 먹여진 뒤 `handle_mouse_input` 이 불리므로, release 를 흘리면 egui 가 다음 프레임에 press+release 쌍을 완성해 커서 밑 위젯의 `clicked()`(우클릭이면 `secondary_clicked()` → 새 메뉴 큐잉)를 발화시킨다. 즉 메뉴를 닫으려던 클릭이 그 밑의 사이드바 행/탭/explorer 항목까지 실행한다. release 판정을 "메뉴가 아직 떠 있는가" 와 무관하게 press 기록으로만 하는 것도 이 때문이다 — press 로 dismiss 를 건 뒤 release 전에 `poll_pending_native_menu` 가 슬롯을 비우는 것이 정상 경로다. 회귀망은 `mouse.rs` 의 `menu_dismiss_tests`(사이클 전이 + 맨 `egui::Context` 로 "삼킨 사이클은 `clicked()` 를 발화하지 않는다" 검증, 대조군 포함).

**비블로킹 폴링**: `GtkMenuHandle::poll()` 은 `while gtk::events_pending() { main_iteration_do(false) }` 로 큐에 있는 것만 처리하고 즉시 반환한다. `MainView::poll_pending_native_menu` 가 두 곳에서 이를 호출한다 — `handle_redraw`(같은 프레임 안에서 `process_pending_native_menu` 보다 **먼저**, 방금 닫힌 메뉴의 뒤처리가 다음 메뉴 요청을 막지 않게)와 `about_to_wait`(메뉴가 떠 있는 동안 8ms `WaitUntil` 로 재예약되며 돌아, redraw 이벤트가 없는 순간에도 폴링이 끊기지 않게). 이 `WaitUntil` 재예약을 빠뜨리면 메뉴가 열린 채 폴링이 멈춘다. 트레이(AppIndicator) GTK 펌프도 메뉴가 떠 있으면 함께 돈다.

**30초 워치독**: `selection-done` 이 30초 안에 안 오면 강제로 `popdown()` 한다. 비동기 계약으로 바뀐 뒤 이 워치독의 의미는 "앱 프리즈 방지"가 아니라 **유령 메뉴 방지**다 — 아무도 닫을 수 없는 팝업이 화면에 남고 continuation 이 그 뒤에 묶이는 것을 막는다. 위 winit press dismiss 경로가 있으므로 실제로는 거의 발화하지 않는다. 타임아웃 경고 로그에는 실제 `grabbed` 값이 함께 찍힌다.

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
feature 에 없으므로, 둘 중 하나라도 빠지면 이름 필터에 **0 건이 걸려 초록**이 난다 —
`--ignored` 와 이름 필터를 함께 쓰는 조합이라 0 건이 정상처럼 보인다.

## 새 메뉴 체크리스트

1. `PendingNativeMenu` variant 추가(좌표 `x/y` 필수).
2. egui `secondary_clicked()` → `pending_native_menu` 설정.
3. `process_pending_native_menu()` 에 match 분기 → 핸들러에서 `open_native_menu(x, y, &items, cont)` 호출(`show_context_menu` 직접 호출 금지).
4. continuation 시작부에서 대상 id 유효성 재확인(메뉴가 열려 있는 동안 사라졌을 수 있다).
5. 항목 텍스트는 `crate::i18n::t()` + `lang/{en,ko,ja}.toml` 키 추가.
