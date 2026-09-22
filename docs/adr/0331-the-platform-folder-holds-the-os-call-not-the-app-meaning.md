# ADR-0331: platform 은 OS 호출을 들고, 그 신호가 App 에서 무엇이 되는지는 안 정한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, platform, layering, app-event, callback, windows, macos, cross-platform
- **Group**: architecture

## Context

`crates/tasty-platform/src/` 은 "플랫폼 특정 모듈" 이라는 이름만 있고 **무엇이 그 안에 있어야 하는지를
정한 문장이 없었다.** 그래서 두 종류가 한 폴더에 섞였다 — OS API 를 부르는 코드와, 그
결과가 App 에서 무슨 사건인지 정하는 코드다.

섞인 자리를 세어 보면 폴더 17 파일 중 셋이다.

- `power_windows.rs` 의 서브클래스 프로시저는 `WM_POWERBROADCAST` 를 가로채는 OS 일과
  `AppEvent::SystemResumed` 를 보내는 결정을 **한 함수 안에서** 했다. 그러려고
  `EventLoopProxy<AppEvent>` 를 `dwrefdata` 로 leak 해 OS 콜백 문맥까지 끌고 갔다.
- `macos_delegate.rs` 는 winit 의 NSApplicationDelegate 에 메서드를 주입하는 AppKit 일을
  하면서, dock 클릭이 `AppEvent::CreateWindow(WindowRequestOrigin::User, None)` 이고
  메뉴 Quit 이 `AppEvent::QuitRequested` 라는 것도 자기가 정했다.
- `debug_info.rs` 는 그 반대였다 — **OS 를 한 번도 안 부르는데** platform 에 있었다.
  읽는 것이 `AppState` · `CoreState` · `GpuState` 뿐이고 유일한 소비자가 App 의 IPC
  핸들러다.

이 섞임에는 값이 붙는 대가가 있다. 이 폴더에서 App 을 안 보는 파일들을 크레이트로 떼는
작업이 예고돼 있는데, 위 결합이 남아 있는 한 그 판정을 **파일마다 사람이 다시** 해야 한다.
그리고 판정을 기계로 하려고 `crate::app` 을 훑으면 틀린다 — `app_icon` 이라는 형제
플랫폼 모듈의 루트 별칭이 같은 접두사를 갖는다. 실측으로 그 오탐이 하나 있었고, 그 한
자리 때문에 "App 결합 파일" 이 넷으로 세어졌다.

## Decision

**`crates/tasty-platform/src/` 에는 OS 를 부르는 코드가 있고, 그 신호가 App 에서 무엇이 되는지는 그
폴더 밖에서 정한다.** 플랫폼 모듈이 App 에 알릴 것이 있으면 **호출부가 맡긴 콜백**을
부르고, 그 콜백이 무엇을 하는지는 모른다.

적용된 형태는 둘이다. Windows 절전 후크는 `EventLoopProxy<AppEvent>` 대신
`OnResume = Box<dyn Fn() + Send>` 를 leak 하고, 그것을 만드는 쪽은 proxy 를 이미 들고 있던
`src/app/window_lifecycle.rs` 다. macOS delegate 는 `OnceLock<EventLoopProxy<AppEvent>>`
대신 `DelegateActions { new_window, quit }` 를 들고, 그것을 만드는 쪽은 `src/boot/os.rs` 다.

같은 규칙이 반대 방향으로도 적용된다 — **OS 를 안 부르는 파일은 이 폴더에 있지 않는다.**
그래서 `debug_info.rs` 는 glue 를 떼는 대신 파일째 `src/app/` 으로 갔다.

**형제 플랫폼 모듈은 자기 경로로 부른다.** 루트 별칭은 이름이 App 모듈과 앞부분을 공유해
훑는 쪽에서 결합으로 읽히고, 그 오탐이 이미 한 번 판정을 바꿨다.

## Consequences

- **얻은 것**: 폴더의 계약이 문장이 됐고, 그 계약으로 판정한 결과가 파일 목록으로 남는다.
  `power_windows.rs` 의 `crate::` 참조가 2 에서 **0** 이 됐다 — 이 파일은 이제 본체의
  어떤 이름도 안 본다. 그래서 크레이트로 뗄 후보는 13 이 아니라 **14** 다. 그 수의
  좌변은 **조사가 "App 결합" 으로 지목한 넷을 뺀 명부**다 — `system_tray.rs` 는 결합이
  아니었고 `debug_info.rs` 는 폴더를 떠났으므로 넷이 둘이 됐다. 폴더를 다시 센 값이
  아니라는 뜻이고, 그 둘(`macos_delegate.rs` · `power_windows.rs`)이 지금 무엇을 보는지는
  아래 트리거의 바늘로 따로 잰다.
- **잃은 것**: 간접이 한 겹 늘었다. resume 신호가 어디서 `AppEvent` 가 되는지 보려면
  플랫폼 파일이 아니라 설치하는 쪽을 봐야 한다. 그리고 콜백은 `Send`(macOS 쪽은
  `Send + Sync`)를 요구하므로, 나중에 스레드 친화적이지 않은 값을 실으려면 그 자리에서
  막힌다.
- **운영 비용 / 유지 부담**: 새 플랫폼 후크를 더할 때 "여기서 `AppEvent` 를 보내면 되지"
  가 더 이상 맞는 답이 아니다. 콜백 형태를 한 겹 더 써야 한다.

## Alternatives Considered

- **A: 폴더 전체를 크레이트로 떼면서 결합 파일만 남긴다** — 경계가 크레이트 경계로 강제돼
  가장 강하다. 안 고른 이유는 새 크레이트 하나가 네 자리 lockstep(아키텍처 문서 · README
  둘의 배지와 본문 · 크레이트 목록 시험)을 같은 커밋에 끌고 오고, **그 앞에 이 결정이
  서야** 무엇을 떼는지가 정해지기 때문이다. 순서의 문제이지 배제가 아니다.
- **B: 결합을 그대로 두고 판정기를 만들어 `crate::app` 참조를 막는다** — 현상 유지 + 게이트.
  안 고른 이유는 그 좌변이 오탐을 낸다는 것이 실측으로 확인됐고(형제 모듈의 루트 별칭),
  오탐을 피하려면 결국 파일마다 사람이 판정해야 해서 게이트가 판정을 대신하지 못한다.
- **C: 콜백 대신 채널(`mpsc`)을 플랫폼 모듈에 쥐여 준다** — 타입 결합은 똑같이 끊긴다.
  안 고른 이유는 OS 콜백이 `unsafe extern` 문맥에서 불리고, 거기서 필요한 것은 "한 번
  부른다" 뿐이라 채널의 수명·버퍼·소비자 관리가 순수한 추가 비용이기 때문이다.
- **D: `debug_info.rs` 를 platform 에 두고 이름만 바꾼다** — 이동이 없어 가장 싸다. 안 고른
  이유는 그 파일이 OS 를 한 번도 안 불러 이 ADR 의 규칙에 **정면으로** 걸리고, 남겨 두면
  폴더 계약의 첫 반례가 되기 때문이다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `crates/tasty-platform/src/` 의 어떤 파일이 `AppEvent` 를 다시 이름으로 부른다. 이 결정이 없앤 것이
  정확히 그것이라, 다시 나타나면 규칙이 안 지켜졌거나 규칙이 틀린 것이다.
  **좌변은 주석을 뺀 코드 줄이다.** 이 폴더에는 규칙 자체를 설명하는 주석이 그 낱말을
  담고 있어(`power_windows.rs` 머리말), 원문을 그냥 훑으면 규칙을 적은 줄이 위반으로
  잡힌다. 재는 법:
  `grep -rnE '(^|[^A-Za-z0-9_])AppEvent' crates/tasty-platform/src/ | grep -vE ':[[:space:]]*//'`
  — 이 결정 전 **10**, 지금 **0** 이다(원문을 그냥 훑으면 각각 11 과 **2** 라 0 이 안
  나온다). 뒤쪽이 1 이 아니라 2 인 것은 [ADR-0343](0343-the-os-boundary-is-a-crate.md)
  이 좌변을 폴더에서 크레이트로 옮기면서 모수에 파일이 하나 들어왔기 때문이다 —
  새 `lib.rs` 머리말이 "여기에는 `AppEvent` 도 `AppState` 도 없다" 고 적는다. 즉 늘어난
  1 은 규칙을 어긴 줄이 아니라 **규칙을 적은 줄**이고, 필터판이 0 인 것이 그 증거다.
- `crates/tasty-platform/src/power_windows.rs` 가 `crate::` 를 다시 참조한다. 지금 0 이고, 0 이라는 것이
  이 결정이 도달한 지점이다.
  **★ 이 바늘의 뜻이 [ADR-0343](0343-the-os-boundary-is-a-crate.md) 이후 좁아졌다.** 폴더가
  크레이트가 되면서 `crate::` 는 이제 본체가 아니라 **그 크레이트 자신**을 가리킨다 —
  형제 플랫폼 모듈을 `crate::app_icon` 으로 부르는 것은 이 결정이 **요구하는** 형태이고
  위반이 아니다. 그래서 이 조건이 묻는 것은 이제 `crate::` 의 유무가 아니라 **크레이트가
  본체 타입에 닿는가**이고, 그 좌변은 매니페스트다(ADR-0343 의 첫 트리거). 이 줄은 그
  크레이트 안에서는 `crate::` 로 본체에 닿을 길이 없어졌다는 기록으로 남긴다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **콜백 한 겹이 실제로 디버깅을 어렵게 만든다는 보고.** 이 결정이 맞바꾼 것이 그것이라,
  값이 반대로 나오면 되돌릴 근거가 된다. 재는 법: resume 또는 dock 클릭이 App 에 안
  닿는 버그를 추적할 때, 플랫폼 파일에서 시작해 설치 지점까지 몇 번을 건너뛰어야 했는지
  세고, 이 결정 전의 직접 호출과 견준다.
- **macOS 에서 이 형태가 실제로 컴파일되고 동작하는가.** 이 결정을 내린 트리에는 macOS 를
  컴파일하는 채널이 없다 — 구문은 rustfmt 가 파싱하는 것으로 확인했고 타입은 **미측정**
  이다. 재는 법: clang 이 있는 머신에서 `cargo check --target aarch64-apple-darwin`,
  그리고 실제 macOS 에서 dock reopen 과 메뉴 Quit 을 눌러 창 생성·종료가 일어나는지 본다.

## References

- `docs/dev-guide/build.md` — 워크스페이스 구조와 크레이트 분리 가이드
- `docs/adr/0017-windows-suspend-resume-pty-recovery.md` — 이 결정이 형태를 바꾼 절전 후크
- `docs/adr/0091-render-stall-watchdog-observation-only.md` — 같은 폴더의 다른 파일을 고정한 결정
- [ADR-0343](0343-the-os-boundary-is-a-crate.md) — 이 ADR 의 Alternative A 를 실행해 폴더를
  `tasty-platform` 크레이트로 뺀 후속 결정. 이 ADR 의 결정 자체는 그대로 유효하다.
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-platform/src/power_windows.rs` 의 `OnResume` ·
  `crates/tasty-platform/src/macos_delegate.rs` 의 `DelegateActions` · `src/boot/os.rs` 의
  `install_macos_delegate` · `src/app/debug_info.rs`
