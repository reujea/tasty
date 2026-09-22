# ADR-0248: webview 정리는 GDK 를 먼저 끝낸 뒤 X 창을 지운다 — 남는 경합은 에러 트랩이 값으로 받는다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: linux, x11, gdk, gtk, webview, crash-safety, teardown, ordering, adr-0159
- **Group**: plugin-render

## Context

Linux 에서 webview surface 는 winit 창 아래의 **X11 자식창**으로 만들고, 그 XID 를 foreign
`GdkWindow` 으로 감싸 GTK toplevel 에 `set_window` 으로 묶는다. 즉 **같은 X 창 ID 에 소유자가
둘**이다 — 이 백엔드의 raw Xlib 경로와 GDK/GTK. [ADR-0159](0159-a-null-gdk-window-is-a-value-not-a-crash.md)
는 그 이중 소유가 **생성** 쪽에서 내던 크래시를 닫았다. 이 ADR 은 **정리** 쪽이다.

정리 코드는 셋을 이 순서로 불렀다: `webview.destroy()` → `XDestroyWindow` → `gtk_window.close()`.
그러면 webview 탭이 있는 창을 닫을 때 프로세스가 통째로 죽었다.

```
Gdk-WARNING **: The program 'tasty' received an X Window System error.
The error was 'BadWindow (invalid Window parameter)'.
  (Details: serial <n> error_code 3 request_code 10 (core protocol) minor_code 0)
```

측정(2026-09-08, 격리 홈, debug 빌드, 조건마다 갓 띄운 인스턴스). 판정 전에 그 창의 X
자식창 수를 세어 webview 가 실제로 만들어졌음을 확인했다 — 안 세면 webview 가 안 생긴
회차가 **거짓 음성**으로 초록이 된다.

| 조작 | 고치기 전 |
|---|---|
| 터미널 탭만 있는 창 닫기 (대조군) | 살아남음 |
| markdown webview 탭이 있는 창 닫기 | **죽음**, BadWindow 1 |
| html webview 탭이 있는 창 닫기 | **죽음**, BadWindow 1 |
| 한 창에 webview 탭 둘, 창 닫기 | **죽음**, BadWindow 1 |
| webview 탭 하나 — 탭만 닫기 (대조군) | 살아남음 |

원인을 **부모가 먼저 죽어서**로 짚기 쉽다(X 는 부모를 파괴할 때 자식을 함께 파괴하고,
탭만 닫는 대조군은 부모가 산다). 그 가설은 계측에서 틀렸다. `XDestroyWindow` 뒤에 `XSync`
를 걸어 보면 그 파괴는 **에러 없이** 끝난다 — 그 시점에 자식창은 살아 있었다. 죽이는 요청은
그 뒤 `gtk_window.close()` 가 예약해 둔 것이 **다음 메인 루프 반복에서** 나가는
`UnmapWindow`(request_code 10)였다. 우리가 GDK 몰래 창을 지우고, GDK 는 자기가 아는 살아
있는 ID 로 unmap 을 낸 것이다. GDK 는 Xlib 에러 핸들러를 **프로세스 전역**으로 걸어 두므로
(`XSetErrorHandler` 는 연결별이 아니다) 그 자리에서 abort 한다.

대조군이 살아남은 이유도 여기서 갈린다. 탭만 닫는 경로는 그 뒤로도 앱이 계속 GTK 를 돌려
GDK 가 `DestroyNotify` 를 먼저 보고 자기 상태를 맞출 시간이 있다. 창을 닫는 경로는 그 틈이
없다. **그래서 이것은 순서 문제이지 부모 수명 문제가 아니다.**

## Decision

**GDK 를 먼저 끝내고, 그 다음에 X 창을 지운다. 그리고 정리하는 동안만 GDK 에러 트랩을 건다.**

`PlatformWebView::drop` 의 순서를 이렇게 못 박는다.

1. `webview.destroy()`
2. `gtk_window.hide()` → GTK 펌프. GDK 의 unmap 을 **창이 아직 있을 때** 내보낸다.
3. `gtk_window.close()` → GTK 펌프.
4. `XDestroyWindow` + `XSync` → GTK 펌프. foreign 창은 GDK 가 파괴하지 않으므로 이 호출은
   그대로 필요하고, 마지막 펌프로 GDK 가 `DestroyNotify` 를 받게 한다.

펌프는 이 레포가 GTK 를 winit 루프 안에서 돌릴 때 쓰는 형태 그대로다
(`platform::native_menu::linux` · `platform::system_tray`). `hide()`·`close()` 는 요청을
**걸어 두고 돌아오기 때문에** 펌프가 없으면 위 순서가 실제 순서가 되지 못한다.

그 위에 `gdk_x11_display_error_trap_push` / `..._pop` 을 정리 구간에만 건다. 트랩은 abort 를
**값으로 바꾼다** — 트랩이 걸린 동안 그 디스플레이에서 난 X 에러는 전역 핸들러로 안 가고
`pop` 의 반환값이 된다. 0 이 아니면 `tracing::warn!` 로 남긴다. **이것은 순서 수정 뒤의
마지막 그물이지 순서의 대체가 아니다** — 트랩만 걸고 순서를 두면 근본 경합이 그대로 남고,
로그만 늘어난다.

## Consequences

- **얻은 것**: 위 표의 세 사망 조작이 모두 살아남는다. 고친 뒤 측정 —
  markdown 창 닫기 4/4, html 창 닫기 4/4, webview 둘인 창 닫기 6/6, 두 대조군 각각
  4/4·6/6 이 살아남았고 BadWindow 0 이었다.
- **잃은 것**: webview 하나를 정리할 때 GTK 메인 루프를 세 번 돌린다. 실측 이벤트 수는
  자리마다 한 자릿수였다.
- **운영 비용 / 유지 부담**: 이 순서는 주석이 지키는 불변식이다. 판정기를 세우지 않았다 —
  순서를 정적으로 읽어 판정하려면 "GDK 가 이 창을 다 놓았는가" 를 소스에서 물어야 하는데
  그 술어가 없다. 대신 이 자리를 만지는 사람이 읽도록 `Drop` 과 `pump_gtk` 에 근거를 붙였다.

## Alternatives Considered

- **A: 창이 닫히는 경로에서 webview 를 먼저 정리하고 그 뒤에 winit toplevel 을 파괴한다**
  (필드 선언 순서 또는 `impl Drop for MainView`) — 원인 가설이 "부모가 먼저 죽는다" 였을
  때의 본선이었다. **전제가 계측에서 틀렸다**(위 Context). 덧붙여 `MainView` 에 `Drop` 을
  달면 마지막 창을 파킹하며 `state`·`core_state` 를 꺼내는 경로가 `E0509`(Drop 타입에서
  부분 이동 불가)로 컴파일이 깨진다.
- **B: `PlatformWebView` 가 부모 winit 창의 `Arc` 지분을 쥔다** — toplevel 수명을 소유로
  고정하는 형태. 짓고 재 봤더니 **없어도 같은 결과**였다(같은 조작이 똑같이 살았다). 안 쓰는
  수명 결합과 세 백엔드의 시그니처 변경만 남으므로 뺐다.
- **C: 에러 트랩만 건다** — 은폐다. 순서를 그대로 두고 펌프만 넣은 갈래는 여전히 죽었다
  (펌프 도중 abort). 그래서 트랩은 순서 위에만 얹는다.
- **D: `gtk_widget_destroy` 로 toplevel 을 동기 파괴한다** — 프로세스가 즉사한다.
  이 toplevel 의 GdkWindow 는 `set_window` 으로 끼워 넣은 foreign 창이라
  `gtk_widget_unregister_window` 의 `user_data == widget` 단정이 깨지고 GTK 가 `Bail out!`
  으로 죽인다(실측 4/4).
- **E: `gdk_window.destroy_notify()` 로 GDK 에 "남이 파괴했다" 를 알린다** — 이름이 이
  상황에 정확히 맞지만 측정은 반대였다. webview 둘인 창 닫기가 4 회 중 1 회 사망에서
  **6 회 중 5 회 사망**으로 나빠졌다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `gtk` / `webkit2gtk` 의 메이저 의존 버전이 바뀐다(`Cargo.toml`). GTK4 에는 `GdkWindow`
  도 `set_window` 도 없어 이 결정의 전제가 통째로 사라진다.
- `PlatformWebView` 가 X11 자식창을 직접 만들지 않게 된다(`XCreateSimpleWindow` 호출이
  이 백엔드에서 사라진다). 이중 소유가 없어지면 순서 규약도 필요 없다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- webview 를 담은 창을 닫을 때 프로세스가 다시 죽는다. 재는 법: 격리 홈으로 인스턴스를
  띄우고(사용자 인스턴스에서 재현하지 말 것 — 진짜로 죽는다) 위 표의 다섯 조작을 각각
  갓 띄운 인스턴스에서 돌린다. 각 조작 전에 대상 창의 X 자식창 수를 세어 webview 가
  실제로 생겼는지 확인한다(안 세면 거짓 음성이 초록으로 지나간다).
- `webview 정리 중 X 에러` warn 이 로그에 남는다. 트랩이 abort 를 막았다는 뜻이고, 그
  자리는 위 순서가 아직 못 막은 경합이다.

## References

- [ADR-0159](0159-a-null-gdk-window-is-a-value-not-a-crash.md) — 같은 이중 소유가 **생성**
  쪽에서 내던 크래시. 거기서 `XFlush` → `XSync` 로 바꾼 이유(연결이 둘이면 왕복해야 한다)가
  이 ADR 의 4 단계에서 그대로 쓰인다.
- 코드 근거(결정이 실현된 현재 위치): `host_api::webview::linux` 의 `PlatformWebView::drop`
  과 `pump_gtk`.
