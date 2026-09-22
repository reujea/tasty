# ADR-0071: 네이티브 컨텍스트 메뉴는 "즉시 반환 + 프레임 폴링" 계약으로 바꾸고, 해소 타이밍은 플랫폼별로 다르게 둔다

- **Status**: Accepted
- **Date**: 2026-08-15
- **Tags**: native-menu, context-menu, linux, x11, gtk, winit, event-loop, async, no-hang, cross-platform
- **Group**: ui-theme-gallery

## Context

`show_context_menu` 는 세 OS 가 같은 동기 시그니처(`-> Option<u32>`)를 쓰도록 의도적으로
설계돼 있었다. 선택 결과를 그 자리에서 받아 처리하는 구조라 호출부(`src/view/main/redraw.rs`
의 컨텍스트 메뉴 핸들러 11 개)가 단순했고, macOS(NSMenu) / Windows(TrackPopupMenu) 는
**메인 윈도우가 원래 쓰는 런루프 / 메시지펌프 안에서** 메뉴를 트래킹하므로 동기 반환에
아무 대가가 없었다.

Linux(GTK 3) 만 사정이 달랐다. tasty 는 전용 GTK main loop 이 없으므로 이 백엔드는
winit 콜백 안에서 `while !done { gtk::main_iteration_do(true) }` 로 GTK 의 별도 main
context 를 직접 돌려 동기 계약을 흉내 냈다. 이 루프가 도는 동안 winit 이 소유한 X11
이벤트 큐는 전혀 처리되지 않는다.

정상적으로는 메뉴를 클릭/바깥클릭하는 순간 `selection-done` 이 떠서 루프가 즉시 빠지지만,
`popup_at_rect` 을 트리거 `GdkEvent` 없이(winit 이 이미 그 클릭을 소비했다) 호출하기
때문에 포인터/키보드 grab 이 실패하는 경우가 있다. grab 이 실패하면 바깥 클릭이 메뉴에
도달하지 않아 `selection-done` 이 영영 오지 않고, 30 초 워치독이 강제로 닫을 때까지 루프가
계속 돈다. X11/GNOME(Mutter) 환경에서 **실제 하드웨어 마우스 우클릭**으로 재현됐고
(합성 입력·xdotool 로는 재현되지 않는다), 그동안:

- winit 이벤트 루프가 막혀 `_NET_WM_PING` 에 응답하지 못해 WM 이 "응답 없음" 배너를 띄운다,
- 화면이 갱신되지 않고 키/마우스 입력도 처리되지 않는다(밀린 입력이 30 초 뒤 한꺼번에 흘러든다).

즉 워치독은 의도대로 동작했지만 그 30 초 동안의 UX 가 실질적 앱 프리즈였다. 30 초 상한을
줄이는 것은 근본 해결이 아니고(줄인 만큼 프리즈), grab 실패의 GDK/X11 레벨 근본원인은
이번 조사에서 규명되지 않았다.

## Decision

`show_context_menu` 의 반환형을 세 OS 모두 `MenuOutcome { Ready(Option<u32>),
Pending(MenuHandle) }` 으로 통일한다 (출처: 세 OS **컴파일** 채널 — `.github/workflows/crossplatform-check.yml` 의 `check-macos`·`check-windows`·`check-headless`. **런타임 동작은 그 채널 밖이다** — integration·e2e 는 Linux 뿐이다) — 타입이 소스에 하나뿐이라 모양은 소스로 참이고, 세 조합이 실제로 컴파일되는 것을 그 채널이 본다. **API 형태(=호출 규약과 호출부 코드 모양)는 통일하되,
언제 해소되는지는 플랫폼별로 다르다는 것을 타입으로 드러낸다.**

- macOS / Windows: 기존 구현을 그대로 두고 결과를 `Ready` 로 감싸기만 한다. 호출부에서 볼 때
  continuation 은 예전 동기 코드와 **같은 시점**(호출 직후, 같은 프레임)에 실행된다.
  이미 문제가 없는 두 백엔드의 내부를 억지로 비동기화하지 않는다 — 얻는 것 없이 검증되지 않은
  회귀 위험만 들여오기 때문이다.
- Linux: `popup_at_rect` 직후 즉시 `Pending(handle)` 을 반환한다. 블로킹 대기 루프는 삭제한다.
  핸들이 `gtk::Menu` 수명·공유 `Rc<Cell<_>>`·워치독 소스·X11 display 를 소유하고,
  `poll()` 이 `main_iteration_do(false)` 비블로킹 펌프 후 완료 여부만 보고한다.

호출부는 `MainView::open_native_menu(x, y, items, cont)` 단일 관문을 거친다. `Ready` 면
`cont` 를 즉시 실행하고, `Pending` 이면 `(handle, cont)` 를 `MainView::pending_menu` 에
넣어 `redraw` 마다 `poll_pending_native_menu` 가 소비한다. 11 개 핸들러의 선택 처리 로직은
그대로 두고 continuation 안으로 옮긴다.

30 초 워치독은 유지하되 의미가 바뀐다: 더 이상 "앱 프리즈 방지"가 아니라 **유령 메뉴 방지**
(아무도 닫을 수 없는 팝업이 화면에 남고 continuation 이 그 뒤에 묶이는 것)다. 여기에 더해
winit press 경로에서 `MenuHandle::dismiss()` 를 호출해, grab 이 실패해도 바깥 클릭으로
확실히 닫히게 한다 — 워치독은 사실상 발화하지 않는 최후 안전장치가 된다.

grab 실패의 GDK/X11 이벤트 스트림 레벨 근본원인 규명은 이번 범위 밖으로 유예한다. 목표는
"grab 이 실패해도 화면이 멈추지 않는다" 이고, 그 목표는 근본원인과 독립적으로 달성된다.

## Consequences

- **얻은 것**: winit 메인 스레드가 네이티브 메뉴 때문에 막히는 경로가 구조적으로 사라졌다
  (`_NET_WM_PING` 응답 유지, 메뉴가 떠 있는 동안에도 렌더/입력 계속). grab 실패는 이제
  "바깥 클릭 dismiss 가 winit 경로로 처리된다" 는 경미한 차이로 강등된다. 워치독 타임아웃
  경고 로그가 실제 `grabbed` 값을 찍어 추정과 사실을 구분한다.
- **잃은 것**: Linux 에서 continuation 이 메뉴 오픈보다 **여러 프레임 뒤**에 실행된다.
  그 사이 대상(탭·pane·surface·카테고리·워크스페이스 인덱스)이 사라질 수 있어, 각
  continuation 시작부에 대상 유효성 재확인이 필요해졌다(동기 코드엔 없던 신규 요구).
- **운영 비용 / 유지 부담**: 메뉴가 떠 있는 동안 `about_to_wait` 이 8ms `WaitUntil` 로
  폴링 프레임을 예약한다(메뉴가 없으면 기존 `Wait` 그대로 — 상시 wakeup 아님). 새 컨텍스트
  메뉴를 추가할 때 `show_context_menu` 를 직접 부르면 `Pending` 을 흘려 메뉴가 그대로 뜬 채
  남으므로, 반드시 `open_native_menu` 를 거쳐야 한다.

## Alternatives Considered

- **동기 계약 유지 + Linux 내부만 수정**: `gtk::main_iteration_do` 대기 중에도 winit 쪽 X11
  이벤트(최소한 `_NET_WM_PING`)를 함께 서비스하도록 만드는 안. 호출부 11 곳을 안 건드려도
  되지만, winit 이 소유한 이벤트 루프를 그 콜백 **안에서** 재진입시키는 안전한 방법이 없고
  (GTK 루프를 별도 스레드로 옮기는 변형은 GTK 의 단일 스레드 요구와 충돌), "메인 스레드가
  절대 안 막힌다" 는 보장을 타입으로 못 준다. 프리즈가 남을 가능성을 구조적으로 배제하지
  못하는 것이 결정적이었다.
- **macOS / Windows 도 콜백 계약으로 완전 통일**: 세 OS 가 문자 그대로 같은 코드 경로를 타지만,
  문제가 없는 두 백엔드를 재작성하는 대가로 얻는 것이 "대칭성" 뿐이다. `MenuOutcome` 으로
  호출부 코드 모양은 이미 하나로 모였고, 두 백엔드는 `Ready` 만 반환하므로 계약 위반 여지도 없다.
- **30 초 워치독을 짧게(예: 1 초) 조정**: 프리즈 시간만 줄고 원인은 그대로다. 게다가 짧은
  상한은 사용자가 메뉴를 천천히 고르는 정상 사용을 잘라먹는다.

## Reconsideration Triggers

- Wayland 백엔드를 지원 대상에 넣을 때 — 현재 Linux 구현은 X11 전용이라 팝업 앵커/grab 전략
  자체를 다시 설계해야 한다.
- grab 실패의 GDK/X11 근본원인이 규명되어 실패 자체를 없앨 수 있게 될 때 — 워치독과 winit
  press dismiss 경로의 필요성을 재평가한다.
- macOS / Windows 백엔드에서 동기 트래킹이 문제를 일으키는 사례가 나올 때 — 그 백엔드도
  `Pending` 을 반환하도록 전환한다(호출부는 이미 두 경로를 모두 다룬다).

## References

- [`docs/dev-guide/context-menu.md`](../dev-guide/context-menu.md) — 2단계 패턴, continuation
  형태 호출 예제, Linux 백엔드 동작
- `crates/tasty-platform/src/native_menu.rs`, `crates/tasty-platform/src/native_menu/linux.rs` — `MenuOutcome` /
  `MenuHandle` / `GtkMenuHandle`
- `src/view/main/redraw.rs` — `open_native_menu` / `poll_pending_native_menu`
- 과거 커밋 `f54b5202`, `a177f6dc`, `e56d3a37` — 동일 함수의 이전 수정(무한 행 방지 워치독
  도입, 바깥 클릭 dismiss, clippy exempt)
