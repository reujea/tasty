# ADR-0159: NULL GdkWindow 은 크래시가 아니라 값이다 — 그리고 연결이 둘이면 왕복해야 한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: linux, x11, gdk, webview, ffi, crash-safety, guards, adr-0117
- **Group**: plugin-render

## Context

Linux 에서 html surface(webview)는 winit 창 아래의 **X11 자식창**으로 만든다. 그 창을
GTK/WebKit 에 넘기려면 XID 를 `GdkWindow` 으로 감싸야 한다. 이 경로가 사용자에게 보이는
즉사를 냈다:

```
panic: gdk-0.18.2/src/auto/window.rs:20  assertion failed: !ptr.is_null()
  gdkx11::auto::x11_window::X11Window::foreign_new_for_display
  tasty::host_api::webview::linux::PlatformWebView::new
  tasty::view::main::redraw::…::create_missing_webviews
```

두 사실이 겹쳤다.

**하나 — 연결이 둘이다.** 자식창은 winit 의 Xlib 연결(`display_handle()` 에서 얻은
`Display*`)로 만들고, 그것을 `GdkWindow` 로 감싸는 조회는 **GDK 자기 연결**로 한다.
그 사이에 있던 `XFlush` 는 소켓에 쓰기만 하고 서버가 그 요청을 **처리했는지**는 기다리지
않는다. 두 연결 사이에는 순서 보장이 없으므로, 조회가 생성을 앞지르면 서버는 "그런 창
없다" 로 답한다. 그래서 이 크래시는 **간헐적**이었다 — 같은 명령을 8 회 돌려 0 회 죽고,
다른 회차에는 3 회 중 3 회 죽었다.

**둘 — NULL 이 오류가 아니라 정상 반환값이다.** `gdk_x11_window_foreign_new_for_display`
는 `XGetWindowAttributes` 로 창을 조회하고 없으면 NULL 을 준다. 그런데 gdkx11 의 안전
바인딩은 그 NULL 을 `gdk::Window::from_glib_full` 안의 `assert!(!ptr.is_null())` 로 받는다.
즉 **"창이 없다" 를 호출자에게 알리는 유일한 경로가 프로세스 즉사로 바뀐다.**

같은 바인딩을 네이티브 컨텍스트 메뉴(`popup_at_rect` 앵커)도 쓰고 있었다. 거기서는 winit
이 오래 전에 만든 창이라 생성 경합은 없지만, NULL 이 오면 **우클릭 한 번으로 죽는다.**

## Decision

**NULL 은 값으로 받고, 연결이 둘이면 왕복한다.**

1. XID → `GdkWindow` 변환은 `crates/tasty-platform/src/x11_gdk_window.rs` 의 `foreign_gdk_window` 한
   곳으로 모은다. 이 함수는 `gdkx11::ffi` 의 원 함수를 직접 불러 NULL 을 검사하고 `Err`
   를 돌려준다. **`X11Window::foreign_new_for_display`(패닉하는 바인딩)는 레포에서 쓰지
   않는다.**
2. 자식창 생성과 GDK 조회 사이의 `XFlush` 를 **`XSync`** 로 바꾼다. `XSync` 는 왕복이라
   반환 시점에 생성이 서버에서 **처리 완료**돼 있고, 그 뒤에는 어느 연결이 물어도 창이
   보인다. 이것은 확률을 낮추는 것이 아니라 **순서 불변식**이다.
3. NULL 로 실패한 경로는 **방금 만든 X 창을 파괴하고** 돌아간다. 호출부
   (`create_missing_webviews`)는 webview 가 없는 surface 를 매 프레임 다시 시도하므로,
   정리하지 않으면 실패가 이어지는 동안 창이 쌓인다.

3 번은 1 번의 직접적 결과다. **패닉은 자원을 안 쌓는다 — 프로세스가 죽으니까.** 패닉을
`Err` 로 바꾸는 순간 실패 경로가 반복 가능해지고, 그때 정리 책임이 새로 생긴다.

## Consequences

- **얻은 것**: html surface 생성이 실패해도 프로세스가 산다. 실패는 `tracing::warn!` 로
  남고 다음 프레임에 재시도되므로 **일시적 실패는 스스로 낫는다.** 우클릭 경로도 같은
  성질을 얻었다. 그리고 `XSync` 가 경합 자체를 없앤다.
- **잃은 것**: `XSync` 는 왕복이라 webview 생성마다 X 서버 왕복이 한 번 늘어난다.
  생성은 surface 당 1 회라 프레임 예산에 들어가지 않는다.
- **운영 비용 / 유지 부담**: 안전 바인딩 대신 `*-sys` 를 직접 부르므로 `unsafe` 블록이
  하나 늘었다. 그 대가로 NULL 처리가 호출자에게 보인다.
- **영구 실패가 반복되면 재시도가 로그를 폭주시킨다** — 합성 실패 실측에서 8 초에 warn
  28115 건이 나왔다. 이것은 이 ADR 이 만든 문제가 아니라 그 `Err` 분기 전체의 성질이고
  (`Not an X11 window` · `GTK init failed` 도 같은 길), 후속 트랙에서 해소했다(아래).

### 후속 확정 — 실패의 종류와 시도 상한

폭주의 기전은 "매 프레임 재시도" 가 아니었다. 실패 경로가 X 자식창을 만들었다 지우면
그 X 이벤트가 이벤트 루프를 깨워 **다음 시도를 스스로 부른다.** 같은 실패를 X 작업이
없는 자리(함수 첫 분기)로 옮기면 시도가 3 회에서 멈췄다 — **고리를 만드는 것은 로그가
아니라 X 왕복이다.**

그래서 `Result<Self, String>` 을 `WebViewCreateError { Transient, Permanent }` 로 바꿨다.
`String` 하나로는 두 처방을 나눌 수 없기 때문이다 — 영구 실패는 즉시 포기, 일시 실패는
`MAX_WEBVIEW_CREATE_ATTEMPTS`(8) 까지. 상한은 상수이고 `const _: () = assert!(> 1)` 이
1 로 줄어드는 것을 컴파일 타임에 막는다(1 이면 두 처방이 같아진다).

실측(10 초 창, 합성 영구 실패):

| | 시도 | tasty CPU(jiffies) | X 서버 CPU | 로그 |
|---|---|---|---|---|
| 상한 없음 | 27477 | 723 | 275 | 8.2 MB |
| 상한 있음(일시로 분류) | 8 | 5 | 0~1 | 2.1 KB |
| 상한 있음(영구로 분류) | 1 | 6 | 0 | 2.1 KB |

**Windows 백엔드의 분류는 정적 독해로만 했다** — 이 환경에서 실행할 수 없다. 틀렸을
때의 비용은 상한이 막는다(영구를 일시로 잘못 적어도 8 회에서 멈춘다). macOS 는 셋 다
`Permanent` 이고, 그 백엔드는 **컴파일 검증도 못 했다**(크로스 빌드가 `libsqlite3-sys`
에서 멈춘다). Windows 는 `x86_64-pc-windows-gnu` 로 컴파일을 확인했다.

## Alternatives Considered

- **A: 바인딩을 그대로 쓰고 생성 시점을 늦춘다(부모 창 확보 후 attach)** — 경합 창을
  좁힐 뿐 없애지 못한다. "언제 충분히 늦은가" 는 확률이지 불변식이 아니라, 옳다고
  말할 근거를 만들 수 없다. 무엇보다 NULL 이 여전히 즉사로 남는다.
- **B: `XSync` 만 하고 NULL 검사는 안 한다** — 경합은 사라지지만 다른 이유로 창이 없는
  경우(종료 중, 부모 파괴)가 남는다. 그때 무엇을 하는지가 코드에 없으면 "안 깨진다" 는
  확률 진술이 된다.
- **C: NULL 검사만 하고 `XFlush` 를 둔다** — 죽지는 않지만 **정상 동작이 간헐적으로
  실패한다.** 사용자에게는 "가끔 webview 가 안 뜬다" 로 보인다. 크래시를 조용한 오작동
  으로 바꾸는 것은 수정이 아니다.
- **D: gdkx11 에 상류 패치** — 맞는 방향이지만 이 레포의 크래시를 지금 못 막는다.
  바인딩이 고쳐지면 아래 재검토 조건 ②가 걸린다.

## 자동 채널 — 무엇을 지키고 무엇을 안 지키는가

**이 절이 이 ADR 의 핵심 중 하나다.** 가드가 있다는 것과 그 가드가 이 결정을 지킨다는
것은 다르다.

| 결정 | 지키는 것 | 어디 |
|---|---|---|
| 1(패닉 바인딩 금지) | **지킨다.** 레포 전체 소스 스캔으로 호출부 0 을 강제 | `panicking_binding_has_no_call_site` |
| 1(NULL → `Err`) | **지킨다.** null 포인터가 패닉이 아니라 `Err` 임을 합성 입력으로 | `null_pointer_becomes_an_error_not_a_panic` |
| 2(`XSync` 그 줄) | **지킨다.** 생성·왕복·조회의 존재와 **순서**를 검사 | `adr_0159_two_connection_premise_still_holds` |
| 2(경합이 없다는 **성질**) | **못 지킨다.** 아래 참조 | — |
| 3(실패 경로의 창 정리) | **못 지킨다.** 아래 참조 | — |

**"경합이 없다" 는 성질에는 자동 채널이 없다.** 위 가드는 *그 자리의 그 줄*이 `XSync`
인지를 볼 뿐이다. 다른 곳에 새 교차-연결 조회가 생기면 아무것도 안 잡는다. 타이밍
속성이라 소스 스캔으로도 유닛 테스트로도 판정할 수 없고, **만들 수 없다고 여기 적는다** —
다음 사람이 위 표의 초록을 "경합이 검사된다" 로 읽지 않게 하기 위해서다.

**실패 경로의 창 정리에도 자동 채널이 없다.** 실측으로는 확정했다(정리를 뺀 빌드에서
경고 88 건에 창 89 개 — 차 1 은 winit 자기 창). 그러나 그 실측은 X 서버를 띄우고 합성
실패를 먹여야 나오는 값이라 유닛 테스트가 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

1. **연결이 하나가 된다.** 자식창을 GDK 의 디스플레이로 직접 만들거나, winit 이 GDK 와
   연결을 공유하게 되면 `XSync` 의 근거가 사라진다. 그때 `XSync` 를 그냥 지우는 것이
   아니라 이 결정을 다시 연다 — **왜 하나가 됐는지가 새 전제이기 때문이다.**
   이 트리거는 가드가 있다: `adr_0159_two_connection_premise_still_holds` 가 두 연결의
   출처와 왕복의 순서를 검사하고, 깨지면 "고치지 말고 이 ADR 을 다시 열어라" 로 실패한다.
2. **gdkx11 이 NULL 에서 `assert!` 하지 않게 바뀐다.** 그러면 `*-sys` 직접 호출과
   `unsafe` 블록이 불필요해지고, 안전 바인딩으로 되돌리는 것이 맞다.
3. **Wayland 지원.** 이 결정 전체가 X11 전제 위에 있다. Wayland 백엔드에서는 XID 도
   `XSync` 도 없다.
4. **위 "자동 채널" 표의 못 지키는 두 칸에 채널이 생긴다.** 특히 경합 축에 관측 수단이
   생기면(예: 교차-연결 조회를 세는 검사) 이 ADR 의 위험 서술이 바뀐다.

## References

- `crates/tasty-platform/src/x11_gdk_window.rs` — 결정 1 의 구현과 세 가드
- `src/host_api/webview/linux.rs` — 결정 2·3 의 자리
- `crates/tasty-platform/src/native_menu/linux.rs` — 같은 바인딩을 쓰던 두 번째 자리
- [`docs/dev-guide/unsafe-checklist.md`](../dev-guide/unsafe-checklist.md) — 자가검토 6·7 문항
  (NULL 이 정상 반환값인가 · 연결이 하나인가)
- [ADR-0117](0117-window-and-modal-creation-failure-policy.md) — 창 생성 실패 정책
