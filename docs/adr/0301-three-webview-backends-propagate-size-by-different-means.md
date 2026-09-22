# ADR-0301: 세 webview backend 는 크기를 서로 다른 수단으로 전파한다 — Linux 는 allocation 을 직접 준다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: linux, x11, gtk, webkitgtk, webview, layout, cross-platform, adr-0159
- **Group**: plugin-render

## Context

`PlatformWebView::set_bounds` 는 "이 webview 를 이 사각형에 맞춰라" 는 계약이다. 세
backend 가 같은 서명을 갖지만, 그 계약을 지키는 데 필요한 호출 수가 서로 다르다.

- **macOS** — `WKWebView` 자신이 렌더 타깃이자 배치 단위라 `setFrame` 한 번으로 끝난다.
  담는 것과 그리는 것이 같은 객체다.
- **Windows** — 둘이다. `SetWindowPos` 가 호스트 `HWND`(담는 것)를, `controller.SetBounds`
  가 WebView2 렌더 타깃을 움직인다. 앞의 것만 부르면 창은 커지고 내용은 옛 크기로 남는다.
- **Linux** — 담는 것은 우리가 `XCreateSimpleWindow` 로 만든 X11 자식창이고, 그리는 것은
  GTK/WebKit 이 realize 때 만드는 `GdkWindow` 의 allocation 이다. 그런데 `set_bounds` 는
  `XMoveResizeWindow` 로 앞의 것만 움직였다. 뒤의 것에 해당하던 `gtk_window.resize()` 는
  **이 구성에서 아무 일도 하지 않는다.**

**확인된 것은 결과다**: GTK 의 allocation 이 realize 시점 값에서 움직이지 않는다. 그 구성은
[ADR-0159](0159-a-null-gdk-window-is-a-value-not-a-crash.md) 가 기록한 것이다 —
`connect_realize` 에서 GTK 자신의 GdkWindow 를 우리가 만든 X11 창의 foreign wrapper 로
갈아끼우고, 그 창은 winit 의 Xlib 연결로 만들어져 WM 이 관리하지 않는다. `resize()` 가
WM 에게 보내는 요청이라는 것과 합치면 도달할 상대가 없다는 것까지는 말할 수 있다.

**왜 allocation 이 안 움직이는지는 이 결정으로 확정되지 않았다.** 가장 흔한 설명은
"`ConfigureNotify` 가 GTK 의 연결로 안 가서" 인데, 그 설명대로라면 `register_window` 와
`STRUCTURE_MASK` 로 이벤트 경로를 열면 움직여야 한다. **안 움직였다**(아래 표). 그러므로
이 ADR 은 그 기제를 주장하지 않는다 — 주장하는 것은 "GTK 가 스스로 알아내지 못한다" 는
관측과, 그래서 값을 직접 줘야 한다는 처방뿐이다.

증상은 두 갈래로 나왔다. 활성 탭에서 태어난 webview 는 realize 시점 크기가 마침 옳아
처음엔 맞아 보이다가 **창을 리사이즈하면 내용만 옛 크기로 남고**, 비활성 탭에서 태어난
webview 는 realize 시점에 실제 기하가 없어 **realize 때의 크기로 굳는다** — 이 레포의 격리
측정에서 그 값은 GTK 기본 toplevel 크기인 200x200 이었다(다른 환경에서는 우리가 만든 X11
창의 1x1 이 그대로 남은 것도 관측됐다 — 어느 값이 남는지는 GTK 가 기본 크기를 언제
적용하느냐에 달렸고, 결함은 같다).

## Decision

**Linux 도 Windows 와 같이 둘을 부른다 — 담는 것과 그리는 것에 각각.** `set_bounds` 는
`XMoveResizeWindow` 로 X11 자식창을 움직인 뒤, `gtk_window.size_allocate()` 로 GTK
allocation 을 **직접** 준다. GTK 가 스스로 알아낼 채널이 없는 것이 이 구성의 전제이므로,
allocation 은 계산의 결과가 아니라 우리가 넘기는 값이다.

수단은 측정으로 골랐다. foreign bind 상태에서 무엇이 allocation 을 실제로 바꾸는지는
소스만 읽어서는 안 보이므로, 한 빌드 안에 네 수단을 환경변수로 가르는 비계를 넣고 같은
시퀀스를 각각 돌려 `xwininfo -tree` 로 부모/자식 크기를 비교했다. 결과는 `size_allocate`
하나만 전 시점에서 크기를 맞췄다 — 수치는 아래 Alternatives 에 있다.

## Consequences

- **얻은 것**: 세 backend 가 같은 계약을 실제로 지킨다. 창 리사이즈·pane 분할·워크스페이스
  전환 모두에서 렌더 타깃이 부모를 따라간다. 비활성 탭에서 태어난 webview 도 드러나는
  프레임에 제 크기를 얻는다.
- **잃은 것**: GTK 의 layout 협상을 우리가 건너뛴다. `size_allocate` 는 본래 부모 컨테이너가
  자식에게 부르는 것이고, toplevel 에 직접 부르는 것은 GTK 가 상정한 흐름이 아니다. GTK 의
  size request 계산이 이 창에 대해서는 의미를 잃는다 — 어차피 foreign bind 로 이미 끊긴
  흐름이라 새로 잃는 것은 없지만, 그 사실이 이제 한 자리 더 늘었다.
- **운영 비용 / 유지 부담**: foreign bind 와 이 호출은 한 쌍이다. bind 를 걷어내면(GTK 가
  자기 GdkWindow 를 도로 쥐면) 이 호출은 불필요할 뿐 아니라 GTK 자신의 allocation 과
  싸운다. 그래서 둘이 함께 움직이도록 판정기를 뒀다(아래 재검토 조건).

## Alternatives Considered

후보 넷을 같은 빌드·같은 시퀀스로 쟀다(격리 Xvfb, markdown surface). 판정은
`xwininfo -id <부모> -tree` 의 부모 창 크기와 그 GDK 자식창 크기가 같은가다.

| 수단 | 활성 생성 | 탭 왕복 | 창 축소 | 창 확대 | 비활성 생성 | 전환 | 리사이즈 | 분할 |
|------|-----------|---------|---------|---------|-------------|------|----------|------|
| (없음, 대조군) | 같음 | 같음 | 다름 | 다름 | 같음(200) | 다름 | 다름 | 다름 |
| `GdkWindow::resize` | 같음 | 같음 | 다름 | 다름 | 같음(200) | 다름 | 다름 | 다름 |
| `set_size_request` | 같음 | 같음 | 다름 | 다름 | 같음(200) | 다름 | 다름 | 다름 |
| `register_window` + `STRUCTURE_MASK` | 같음 | 같음 | 다름 | 다름 | 같음(200) | 다름 | 다름 | 다름 |
| **`size_allocate`** | 같음 | 같음 | **같음** | **같음** | 같음(200) | **같음** | **같음** | **같음** |

- **`GdkWindow::resize`**: 창 자체는 커지지만 GTK 의 allocation 은 그대로라 WebKit 이 옛
  크기로 계속 그린다 — 대조군과 한 칸도 다르지 않았다.
- **`set_size_request`**: 위젯의 **요청** 크기만 바꾼다. 요청은 부모 컨테이너가 allocation 을
  다시 계산할 때 읽히는데, 이 창에는 그 계산을 촉발할 것이 없다 — 늘리는 쪽에서도 줄이는
  쪽에서도 대조군과 같았다.
- **`register_window` + `STRUCTURE_MASK`**: GTK 에게 이 GdkWindow 의 이벤트를 받게 해
  스스로 재협상하게 하려는 수단. 크기는 한 칸도 안 바뀌었다 — 대조군과 같다. 게다가 이
  수단은 Drop 경로의 전제를 건드린다: `PlatformWebView::drop` 이 `destroy()` 를 못 쓰는
  이유로 적혀 있는 것이 바로 `register_window` 를 부르지 않았다는 사실이라, 이것을 켜면
  그 경로를 다시 재야 한다. 얻는 것이 0 인 수단에 그 값을 치르지 않는다.
- **비활성 탭도 실제 기하로 생성한다**(webview 를 만들 때 1x1 대신 layout 이 이미 계산해 둔
  사각형을 넘긴다): 첫 프레임의 잘못된 크기를 아예 안 만드는 방향이라 이 결함과 독립적으로
  값이 있다. 그러나 그 자리는 `src/view/main/redraw.rs` 의 플랫폼 공용 경로라 세 backend
  전부의 생성 시점을 바꾸며, 이 결정만으로 필요하지 않다 — `size_allocate` 가 드러나는
  프레임에 크기를 맞추므로 결함 자체는 닫힌다. 별건으로 남긴다.
- **foreign bind 를 버린다**(GTK 가 자기 창을 쥐게 한다): 이 결함은 사라지지만
  [ADR-0159](0159-a-null-gdk-window-is-a-value-not-a-crash.md) 가 그 구성 위에서 내린
  결정이라 그 ADR 을 다시 여는 일이다. 한 호출로 닫히는 결함에 그 값을 치르지 않는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `set_bounds` 의 `size_allocate` 호출과 `connect_realize` 의 `set_window` 호출 중 **한쪽만**
  사라지는 것. 둘은 한 쌍이며, bind 가 없으면 이 호출은 GTK 와 싸우고 bind 가 있으면 이
  호출 없이는 크기가 안 간다. `crates/tasty-platform/src/x11_gdk_window.rs` 의
  `adr_0301_foreign_bind_and_explicit_allocation_move_together` 가 양방향으로 고정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- WebKitGTK 나 gtk-rs 가 foreign window 의 크기 변경을 GTK 에 알리는 공식 수단을 제공하는
  것. 그러면 allocation 을 손으로 주는 대신 그 수단을 쓴다. 재는 법: 위 표의 네 수단을 같은
  시퀀스로 다시 돌려 대조군이 `같음` 으로 바뀌는지 본다 — 절차는
  [`dev-guide/crash-diagnostics.md`](../dev-guide/crash-diagnostics.md) 의 "webview surface
  가 비어 보일 때" 항에 있다.
- `size_allocate` 를 toplevel 에 직접 부르는 것이 GTK 경고를 내거나 상위 버전에서 동작을
  바꾸는 것. 재는 법: 격리 인스턴스의 stderr 에서 `Gtk-WARNING` / `Gtk-CRITICAL` 을 세고,
  같은 시퀀스의 부모/자식 크기 판정이 유지되는지 본다.

## References

- 구현 위치(결정이 실현된 현재 위치): `PlatformWebView::set_bounds` — `src/host_api/webview/linux.rs`
- 비교 대상(같은 계약의 다른 backend): `src/host_api/webview/macos.rs` · `src/host_api/webview/windows.rs`
- 판정기: `adr_0301_foreign_bind_and_explicit_allocation_move_together` — `crates/tasty-platform/src/x11_gdk_window.rs`
- 전제가 되는 구성: [ADR-0159](0159-a-null-gdk-window-is-a-value-not-a-crash.md)
- 재는 절차: [`dev-guide/crash-diagnostics.md`](../dev-guide/crash-diagnostics.md)
