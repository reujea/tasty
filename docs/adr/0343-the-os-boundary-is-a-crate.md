# ADR-0343: OS 경계는 폴더가 아니라 크레이트다 — 본체는 별칭으로 부른다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, platform, crate-split, layering, features, headless, cross-platform, adr-0331
- **Group**: architecture

## Context

[ADR-0331](0331-the-platform-folder-holds-the-os-call-not-the-app-meaning.md) 이
플랫폼 폴더의 계약을 문장으로 정했다 — **OS 를 부르는 코드만 담고, 그 신호가 App 에서
무엇이 되는지는 부르는 쪽이 정한다.** 그 ADR 의 Alternative A 가 "폴더 전체를 크레이트로
뗀다" 였고, 안 고른 이유로 적힌 것은 배제가 아니라 **순서**였다: 크레이트 하나가 lockstep
네 자리를 끌고 오므로 무엇을 떼는지부터 정해야 한다는 것.

그 결정이 착지한 뒤 폴더를 다시 재면 이렇다(좌변은 이 트리의 실측이다).

```
$ find <플랫폼 폴더> -name '*.rs' | wc -l                       → 16
$ grep -rn 'crate::' <플랫폼 폴더>                              → 25 줄 / 8 파일
$ grep -rn 'crate::app\|AppEvent\|AppState' <플랫폼 폴더>       → 2 (둘 다 주석)
$ 같은 좌변에 ADR-0331 의 주석 필터를 붙이면                     → 0
```

**뒤 두 줄이 왜 둘인지가 이 자리의 요점이다.** 이 폴더에는 규칙 자신을 설명하는 주석이
그 낱말을 담고 있어, 원문을 그냥 훑으면 규칙을 적은 줄이 참조로 잡힌다 — ADR-0331 이
그 함정을 문장으로 못박고 좌변에 필터(`grep -vE ':[[:space:]]*//'`)를 박아 둔 이유다.
잡히는 둘은 `power_windows.rs:10` 과 `window_chrome.rs:67` 이고 **둘 다 주석**이라
코드 참조는 0 이다. 결론(본체 App 을 안 본다)은 필터판이 지탱하고, 원문판의 2 는
**그 결론의 반례가 아니라 좌변이 무엇을 세는지의 값**이다.

`crate::` 25 줄이 무엇을 보는지 전수로 갈라 보면 **본체 모듈은 하나도 없다.**

| 보는 것 | 실체 | 줄 |
|---|---|---|
| `crate::i18n` | `tasty-i18n` 의 재수출(`src/i18n.rs` 는 `pub use tasty_i18n::…` 다) | 3 |
| `crate::settings` | `tasty-settings`(`lib.rs` 의 `pub use`) | 8 |
| `crate::paths` · `crate::poison` | `tasty-utils`(`lib.rs` 의 `pub use`) | 4 |
| `crate::test_support` | `tasty-test-support`(dev-dependency) | 3 |
| 형제 플랫폼 모듈 | 같은 폴더 안 | 6 |
| `crate::shortcuts` | 산문 안의 intra-doc 링크 한 줄 (ADR-0331 §3 이 "크레이트가 갈릴 때 함께 푼다" 로 미뤄 둔 자리) | 1 |

> 위 25/8 과 분해 세 행(4 · 3 · 6)은 **고쳐 적은 값**이다. 이관 커밋의 본문은 같은
> 좌변을 26 줄 / 9 파일로 적었고 그것이 틀렸다 — 커밋 메시지는 고칠 수 없으므로 이
> 문단이 정본이다. 결론(본체 모듈이 0)은 재측정으로도 그대로 선다: 틀린 것은 수뿐이고,
> 어느 분해 행도 "본체 모듈" 칸으로 넘어가지 않는다.


즉 **본체에 남을 이유가 있는 파일이 0 이다.** 폴더가 본체 크레이트 안에 있다는 사실만으로
`crate::` 한 줄이면 App 상태에 닿을 수 있고, 계약을 지키는지는 사람이 매번 다시 읽어야
한다. ADR-0331 이 Alternative B 에서 이미 적었듯 그 판정을 텍스트 게이트로 대신하려는
시도는 오탐을 낸다.

## Decision

**플랫폼 폴더 전체를 `tasty-platform` 크레이트로 뺀다. 16 파일 전부이고, 본체에 남기는
파일은 없다.** 계약은 이제 문장이 아니라 **크레이트 경계**가 강제한다 — `tasty-platform`
의 `Cargo.toml` 에 본체가 없으므로 `AppEvent`·`AppState` 를 부르는 것이 컴파일되지 않는다.

**호출부는 한 줄도 안 바꾼다.** 본체 `src/lib.rs` 가 `mod platform;` 대신
`pub(crate) use tasty_platform as platform;` 를 걸어, `crate::platform::native_menu::…`
같은 기존 경로가 그대로 풀린다. 루트 별칭(`crate::crash_report` · `crate::system_tray` 등)
여덟도 `platform::` 을 거치므로 함께 유지된다. 이 회차의 기본값이 **행동 보존**이라
경계를 옮기는 것과 호출 형태를 바꾸는 것을 같은 커밋에 넣지 않는다.

**`gui` feature 는 새 크레이트에도 같은 이름으로 둔다.** 본체의 `gui` 가
`tasty-platform/gui` 를 켜고, 그 뒤에 winit·png·tray-icon·GTK·AppKit 이 있다. headless
빌드가 이 크레이트에서 컴파일하는 것은 `crash_report` 하나다 — 그 모듈만 feature 밖이다.

**크레이트의 절 소속은 "OS 경계" 를 새로 만들어 UI primitive 뒤에 둔다.** 네이티브 메뉴·
트레이는 UI 지만 egui 를 한 줄도 안 쓰고 갤러리 소비자가 없어 그 절의 규칙에 안 맞는다.

## Consequences

- **얻은 것**: ADR-0331 의 계약에 채널이 생겼다. 그 결정이 "게이트로는 못 잰다" 고 적은
  것을 컴파일러가 잰다 — 본체 타입을 부르면 `unresolved import` 다.
  본체 `src/` 의 `.rs` 가 **623 → 606**(−17: 모듈 16 + 모듈 선언 파일 하나)이고,
  크레이트 수가 **59 → 60** 이다. 새 크레이트의 `.rs` 는 **17** 개다.
- **얻은 것 ②: 이 머신에 없던 크로스 타깃 채널 둘이 이 크레이트에는 생겼다.** 본체는
  `x86_64-pc-windows-msvc` 도 `aarch64-apple-darwin` 도 이 호스트에서 검사가 안 된다 —
  `libsqlite3-sys` · `mlua-sys` 의 build script 가 그 타깃용 C 툴체인을 요구하고 여기엔
  mingw 밖에 없다. **새 크레이트는 그 둘을 안 들어서 `-p tasty-platform` 으로는 셋 다
  통과한다**(gnu · msvc · arm64 macOS, 전부 `--features gui`). ADR-0331 이 "타입은
  미측정" 으로 남긴 `macos_delegate.rs` 의 `DelegateActions` 가 이 경로로 처음 타입
  검사를 받았다. 그리고 그 첫 실행이 결함 하나를 잡았다 — `macos_permissions` 의 홈
  디렉터리 해석이 `directories` 를 부르는데, 본체 안에 있던 동안은 본체의 의존을 빌려
  쓰고 있었다. 크레이트가 갈리자 macOS 갈래에서만 `E0433` 이 났다.
- **잃은 것**: lockstep 자리가 넷 늘었다(아키텍처 문서의 개요 수·절 제목 수, README 둘의
  배지와 본문, 루트 `CLAUDE.md` 의 복제 문장). 판정기 둘이 그 넷을 본다 —
  `architecture_crate_list_complete` 와 `readme_badge_parity`.
  그리고 `macos_permissions` 의 항목 **열다섯**이 `pub(crate)` 에서 `pub` 이 됐다.
  크레이트 밖이 이름으로 요구하는 자리가 그만큼이다 — 설정 탭 · 부팅 · 키 주입 판정
  셋이 부르고, cfg 짝까지 세어 열다섯이다. 재는 법:
  `grep -cE '^\s*pub (fn|struct|trait|enum|const)' crates/tasty-platform/src/macos_permissions.rs`.

  **처음에는 열여덟을 열었다.** 나머지 셋(`FsProbe` · `RealFs` · `prewarm_targets`)은
  크레이트 밖에 소비자가 없었다 — 그 셋을 부르는 유일한 자리는 화면 캡처이고 그것은
  크레이트 **안**이다(`screen_capture.rs` 가 `crate::macos_permissions::…` 로 부른다).
  그래서 먼저 `pub(crate)` 로 되돌렸고, 다시 재니 **크레이트 안의 다른 모듈도 그 셋을
  안 쓴다**(`macos_permissions.rs` 를 뺀 `crates/tasty-platform/src/` 에서 0 줄). 그래서
  지금은 셋 다 **모듈 private** 이다 — 이 폴더가 본체 안에 있던 시절의 `pub(crate)` 보다
  좁다. `RealFs` 는 macOS 갈래에만 사는 타입이라 Linux 체크만으로는 "안 쓰인다" 가 안 서고,
  `--target aarch64-apple-darwin` 까지 rc=0 인 것으로 선다.

  **이 좁힘은 위 열다섯을 안 움직인다** — 그 좌변이 세는 것은 `pub` 이고 셋은 이미 그
  밖이었다. `pub` 18 → 15 는 첫 좁힘이 만든 값이고, 두 번째 좁힘은 `pub(crate)` 3 → 0 이다.
- **운영 비용 / 유지 부담**: 플랫폼 코드가 본체 타입을 쓰려면 이제 **그 타입을 leaf 로
  내리거나 콜백으로 받아야** 한다. ADR-0331 이 콜백을 규칙으로 정했으므로 그 비용은 새로
  생긴 것이 아니라 강제된 것이다. 그리고 새 OS 의존을 더할 때 만질 매니페스트가 둘이다.

## Alternatives Considered

- **A: 폴더를 그대로 두고 텍스트 게이트로 `crate::app` 참조를 막는다** — ADR-0331 의
  Alternative B 와 같다. 안 고른 이유도 같다: 형제 모듈의 루트 별칭이 같은 접두사를 가져
  좌변이 오탐을 내고, 오탐을 피하려면 결국 사람이 판정한다.
- **B: 일부만 뗀다 — `crate::` 가 0 인 파일만** — 가장 싸다. 안 고른 이유는 그 기준으로
  남는 것이 `i18n`·`settings`·`utils` 재수출을 보는 파일들뿐인데 **셋 다 이미 leaf
  크레이트**라, 남기는 근거가 "이름이 `crate::` 로 시작한다" 밖에 없다. 경계가 실제
  의존이 아니라 표기를 따라 그어진다.
- **C: 호출부를 전부 `tasty_platform::…` 로 고쳐 별칭을 없앤다** — 별칭 한 줄이 안 남아
  더 깨끗하다. 안 고른 이유는 이 회차가 **행동 보존** 회차이고, 호출부 32 자리를 같은
  커밋에서 고치면 경계 이동과 이름 변경이 한 diff 에 섞여 회귀의 출처를 가를 수 없기
  때문이다. 별칭은 다른 leaf 재수출 여덟과 같은 형태라 새 관행도 아니다.
- **D: 크레이트를 UI primitive 절에 넣는다** — 절을 새로 안 만들어도 된다. 안 고른 이유는
  그 절의 정의가 "본체·갤러리가 공유하는 egui 위젯" 이고 이 크레이트는 egui 를 안 쓰며
  갤러리 소비자가 없기 때문이다. 소속이 틀리면 절 순서 판정의 뜻도 흐려진다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `crates/tasty-platform/Cargo.toml` 의 `[dependencies]` 에 leaf 가 아닌 워크스페이스
  크레이트가 들어온다. 이 결정이 산 것이 "본체 타입에 못 닿는다" 이고, 그 성질은 의존
  목록으로만 유지된다. 지금은 `tasty-i18n` · `tasty-settings` · `tasty-utils` 셋이다.
  재는 법: `grep -n '^tasty-' crates/tasty-platform/Cargo.toml`.
- `src/lib.rs` 에서 `pub(crate) use tasty_platform as platform;` 가 사라진다. 그 줄이
  사라지는 경우는 둘뿐이다 — 호출부를 전부 고쳐 Alternative C 로 갔거나, 크레이트를
  되돌렸거나. 어느 쪽이든 이 결정의 전제가 바뀐 것이다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **macOS·Windows 에서 이 크레이트가 실제로 링크되고 동작하는가.** **타입은 측정됐다** —
  위 Consequences 의 크로스 체크 셋이 전부 rc=0 이고, macOS 전용 의존의 feature 목록도
  그 경로로 확인됐다. 안 재진 것은 그 다음이다: 크로스 `check` 는 링크를 안 하고,
  NSMenu 항목을 눌렀을 때 콜백이 실제로 불리는지는 어느 타깃 검사도 답하지 않는다.
  재는 법: 실제 macOS 에서 dock reopen 과 메뉴 Quit 을 눌러 창 생성·종료가 일어나는지,
  Windows 에서 절전 복귀가 PTY 를 되살리는지 본다. 타입 쪽 회귀를 다시 잴 때는
  `cargo check -p tasty-platform --features gui --target <타깃>` 세 줄을 그대로 돌린다.
- **`pub` 으로 넓어진 표면이 실제로 오용되는가.** `macos_permissions` 의 항목이 크레이트
  밖에서 의도 밖으로 불리면 경계를 좁힐 근거가 된다. 재는 법: `git grep -n
  'macos_permissions::' -- src crates | grep -v crates/tasty-platform` 로 호출부를 세고,
  이 결정 시점의 자리(설정 탭 · 부팅 · 키 주입 판정 **셋**)와 견준다. 화면 캡처는 이
  목록에 없다 — 크레이트 안에서 부르므로 이 좌변에 안 잡힌다.
  **반대 방향도 이 바늘의 일이다**: 공개 항목이 줄어들 수도 있다. `pub` 항목 수는 지금
  **15**(`grep -cE '^\s*pub (fn|struct|trait|enum|const)' <그 파일>`)이고, 테스트 이음새
  셋은 그 밖에서 모듈 private 이다. 그 셋이 다시 `pub`/`pub(crate)` 이 되면 그때는
  크레이트 밖이나 형제 모듈이 요구했다는 뜻이므로, 요구한 자리를 함께 적어야 한다.

## References

- [ADR-0331](0331-the-platform-folder-holds-the-os-call-not-the-app-meaning.md) — 이
  결정이 실행하는 Alternative A 를 적어 둔 선행 결정. 폴더의 계약 자체는 그쪽이 정본이다.
- [ADR-0089](0089-crate-split-follows-dependency-direction.md) — 크레이트 분리의 기준이
  크기가 아니라 의존 방향이라는 결정
- `docs/architecture/index.md` — "OS 경계" 절과 크레이트 수 정본
- `docs/dev-guide/build.md` — 워크스페이스 구조와 크레이트 분리 가이드
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-platform/src/lib.rs` ·
  `src/lib.rs` 의 `pub(crate) use tasty_platform as platform;` · 루트 `Cargo.toml` 의
  `tasty-platform` 의존과 `gui` feature 줄
