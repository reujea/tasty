# ADR-0115: OS 전역 입력 조작(`surface.raw_key` · `surface.switch_input_source` · `surface.ime_*`)은 debug 표면으로 격리한다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: ipc, security, input-injection, debug-isolation, macos, ime, focus-independence, method-table, guard-test, adr-0044
- **Group**: permission-security

## Context

[identity.md](../identity.md) 원칙 1 ② 는 "사용자 입력 재현은 release 에 없다" 고 못박는다 — 키/마우스 주입, popup 강제 open/close, 메뉴 강제 invoke, 프로그래밍적 포커스 전환은 release IPC/CLI 표면에 존재하지 않고 `#[cfg(debug_assertions)]` 격리로만 제공한다. 원칙 3(포커스 독립성)은 "모든 명령은 대상을 ID 로 직접 지정" 을 요구한다.

세 계열이 그 원칙 밖에 남아 있었다.

- **`surface.raw_key`** — macOS `CGEventPost` 로 **OS 이벤트 스트림에** 키를 주입한다. `METHOD_TABLE`(release 표)에 `plugin(&[TerminalWrite])` 로 등재돼, `TerminalWrite` 권한을 가진 plugin 과 모든 로컬 IPC 호출자가 부를 수 있었다. `TerminalWrite` 는 `pty.write`/`pty.kill` 과 같은 토큰이라, 터미널에 쓰기만 하려던 plugin 이 시스템 전역 키 주입 자격을 함께 얻는다. 라우팅에도 플랫폼 게이트만 걸려 있고 debug 게이트가 없었다.
- **`surface.switch_input_source`** — `TISSelectInputSource` 로 **시스템 입력 소스**(키보드 레이아웃·입력기)를 바꾼다. 같은 표에 같은 권한으로 등재돼 있었다.
- **`surface.ime_*`**(enable/disable/preedit/commit/status) — 창의 IME 조합 상태(`ime_active`/`ime_preedit`)를 강제로 세팅한다. `PREFIX_RULES` 의 `local_only()` 로 해소돼 plugin 에는 닫혀 있었지만 **release 빌드에 존재**했고, 대상을 ID 로 받지 못한 채 **포커스된 창**에 작용한다(원칙 3 도 위반).

세 계열 모두 **CLI 진입점은 이미 debug 전용**이었다 — `DebugCommands`(`crates/tasty-cli/src/commands/debug.rs`)가 모듈째 `#![cfg(debug_assertions)]` 이다. 같은 기능의 두 표면이 서로 다른 빌드에 노출된 상태였고, CLI 만 보면 debug 전용처럼 보여 IPC 표면이 열려 있다는 사실이 가려졌다.

가장 무거운 문제는 `raw_key` 의 **대상 부재**다. 파라미터가 `keycode`/`direction` 뿐이고 주입은 OS 이벤트 스트림으로 나가므로, 키를 받는 것은 **그 순간 OS 포커스를 가진 무엇이든** 이다 — tasty 창이 아닐 수도 있다. 메서드 이름이 `surface.*` 인데 surface 를 지정할 수단이 없다.

"release 에 둔다" 는 판단은 기능 문서에 서술만 남아 있었고([macos-permissions](../features/macos-permissions/index.md) "이 기능이 release IPC 표면에 있으므로 권한도 release 에서 필요하다") 그 판단을 기록한 ADR 이 없었다. 원칙과 기능 문서가 정면으로 어긋난 채, 어느 쪽이 맞는지 판정할 근거 문서가 없는 상태였다.

## Decision

세 계열을 모두 **debug 표면으로 격리한다.**

1. **메서드 표** — `surface.raw_key`·`surface.switch_input_source` 를 `METHOD_TABLE` 에서 빼 `DEBUG_METHODS` 에 `local_only()` 로 등재한다(plugin 호출 불가). `PREFIX_RULES` 의 `surface.ime_` 규칙은 `#[cfg(debug_assertions)]` 로 가르고, release 에서는 빈 슬라이스로 컴파일한다.
2. **라우팅** — 두 macOS 메서드의 dispatch 팔을 `route_engine_handler`(release+debug 공용)에서 `route_debug_handler`(`#[cfg(debug_assertions)]`)로 옮긴다. `surface.ime_*` 의 App-level 분기(`ipc_step_window_required`)도 debug 게이트 안으로 넣는다. release 빌드는 이 메서드들을 `method_not_found` 로 떨어뜨린다.
3. **모듈 게이트** — `handler/input_source.rs` 는 `#[cfg(all(debug_assertions, target_os = "macos", feature = "gui"))]`, `handler/ime.rs` 는 `#[cfg(all(debug_assertions, feature = "gui"))]` 로 선언한다. 두 파일 모두 이미 해당 핸들러만 담은 전용 파일이라 [debug-ipc.md](../dev-guide/debug-ipc.md) 의 "debug 핸들러는 별도 파일에 모은다" 를 그대로 만족한다 — 파일을 옮기지 않는다.
4. **런타임 게이트** — `raw_key`/`switch_input_source` 에 `debug.inject_key`/`debug.inject_mouse` 와 같은 `--enable-input-simulation` 게이트(`require_input_simulation`)를 건다. 두 메서드는 tasty 프로세스 **밖으로** 나가는 유일한 입력 조작이라, debug 빌드 안에서도 명시 opt-in 을 요구한다. `surface.ime_*` 에는 걸지 않는다 — 창 내부 상태만 바꾸는 in-process 시뮬레이션이라 `debug.selection` 계열과 같은 급이고, cfg 격리로 충분하다.
5. **회귀 가드** — `tests/ipc_release_table_excludes_input_reproduction.rs` 를 추가한다. 판정을 자동화하는 대신 **이미 내려진 판정과의 어긋남**을 잡는다: (가드 1) debug CLI 가 부르는 메서드가 release 표에 있으면 실패, (가드 2) 입력 재현임이 이름에 드러나는 형태(`inject`/`raw_key`/`switch_input_source`/`ime_`/`simulate`, 마지막 세그먼트 `focus`)가 release 표에 있으면 실패, (가드 3) `surface.ime_` prefix 규칙이 `#[cfg(debug_assertions)]` 밖에 있으면 실패.

[ADR-0044](0044-screenshot-release-promotion-surface-target.md) 가 debug→release 승격에 요구했던 조건(포커스 독립 + 대상 ID 지정)을 반대 방향의 판정 기준으로 그대로 적용했다. `raw_key` 는 CGEvent 가 OS 전역으로 나가는 특성상 그 조건을 **원리적으로** 충족할 수 없다 — 대상 창을 지목하는 API 가 아니다. `switch_input_source` 는 애초에 시스템 단일 상태를 바꾸는 것이라 대상 개념이 없다. `surface.ime_*` 는 포커스된 창에만 작용한다.

대조군으로 남는 것은 **`surface.send_key`(release 유지)** 다 — 대상 surface ID 를 필수로 받아 그 surface 의 PTY 에 바이트를 쓴다. 에이전트가 자기 작업을 하는 정상 동작이고, OS 로 나가지 않는다. "에이전트가 자기 작업에 필요한가 vs 사용자 조작을 재현하는가" 판단 기준의 양쪽 극단이다.

## Consequences

- **얻은 것**: macOS release 빌드에서 OS 전역 키 주입·입력 소스 전환 표면이 **사라진다**(코드째 컴파일되지 않는다). `TerminalWrite` 토큰이 "터미널 쓰기" 보다 넓은 능력을 함의하던 어긋남이 해소돼, 권한 토큰의 이름과 실제 능력이 일치한다. IPC 와 CLI 의 노출 빌드가 세 계열 모두에서 일치한다. 손쉬운 사용(Accessibility) 권한 요구가 debug 빌드 한정이 되어, release 사용자가 그 권한을 켜야 할 이유가 하나 줄어든다.
- **잃은 것**: release 빌드로는 macOS IME 파이프라인(`interpretKeyEvents` → `setMarkedText`/`insertText`) 자동 검증을 구동할 수 없다. 이 검증은 원래 자기검증 용도이므로 debug 빌드에서 수행하는 것이 정상 경로다. 외부 자동화가 release 인스턴스에 `surface.raw_key`/`surface.ime_*` 를 쏘고 있었다면 `method_not_found` 로 깨진다 — 이 표면은 문서상 debug 검증용이었으므로 의도된 파급이다.
- **운영 비용 / 유지 부담**: 가드 2 의 이름 패턴은 정상 에이전트 메서드를 오탐할 수 있다. 오탐이 나면 패턴을 좁히고 근거를 남기는 것이 절차다(가드가 막는 것이 아니라 판정을 강제하는 장치다). 가드 3(`src/source_guards/reserved_ipc_prefixes.rs`)은 `PREFIX_RULES` 정의를 소스 텍스트로 읽으므로 그 정의의 형태가 바뀌면 함께 손봐야 한다.

## Alternatives Considered

- **A: release 에 남기고 대상 surface ID 를 필수화** — ADR-0044 가 스크린샷 승격에 요구했던 조건을 채우는 방향. `CGEventPost` 는 이벤트를 OS 세션 이벤트 탭에 넣을 뿐 수신 창을 지목하지 못하므로, ID 를 받아도 그 ID 가 실제 수신자를 결정하지 못한다. 조건을 형식적으로만 만족시키고 실제 위험(포커스를 가진 임의의 앱이 키를 받는다)은 그대로 남아 기각.
- **B: `raw_key` 만 옮기고 `switch_input_source`·`ime_*` 는 별건으로 남긴다** — 티켓 범위를 좁게 잡는 선택. 세 계열은 같은 원칙의 같은 위반이고, 하나만 고치면 남은 둘이 "판정된 것" 처럼 보여 다음 감사에서 다시 발견돼야 한다. 같은 결함의 나머지라 함께 처리.
- **C: 런타임 플래그(`--enable-input-simulation`)만 걸고 release 표에 유지** — cfg 격리 없이 런타임 게이트만 두는 안. 코드와 메서드 등재가 release 바이너리에 남아 표면이 존재하고, 플래그를 켠 인스턴스에서는 원칙 1 ② 가 그대로 깨진다. 런타임 게이트는 cfg 격리의 **보강**이지 대체가 아니라고 판단.
- **D: 가드를 명시 denylist(메서드 이름 목록)로만 구성** — 지금 옮긴 세 계열을 목록에 박아 재등장만 막는 안. 새로 추가되는 같은 성격의 메서드를 전혀 잡지 못해, 이 결함이 처음 생긴 경로를 그대로 열어 둔다. 이름 규칙 + CLI 정합 대조를 함께 쓰는 쪽을 선택.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- macOS 가 **대상 창을 지목해** 키 이벤트를 보내는 공개 API 를 제공한다(그러면 ADR-0044 의 승격 조건을 실제로 충족할 수 있다).
- 에이전트가 자기 작업을 위해 OS 전역 입력 소스를 바꿔야 하는 정당한 사용례가 나타난다.
- 가드 2 의 이름 패턴이 정상 에이전트 메서드를 반복적으로 오탐해 예외가 누적된다(판정 기준의 기계화 방식을 다시 고를 시점).

## References

- [identity.md](../identity.md) — 원칙 1 ②(사용자 입력 재현 격리), 원칙 3(포커스 독립성)
- [debug-ipc.md](../dev-guide/debug-ipc.md) — 판단 기준, `DEBUG_METHODS` 등재 절차, 격리 정책, `†` 런타임 게이트
- [focus.md](../design/policies/focus.md) — "모든 명령은 대상을 ID 로 직접 지정"
- [macos-permissions](../features/macos-permissions/index.md) — 손쉬운 사용 권한 요구가 debug 한정이 된 결과
- [plugin-permissions.md](../dev-guide/plugin-permissions.md) — `TerminalWrite` 토큰 범위
- [ADR-0044](0044-screenshot-release-promotion-surface-target.md) — 반대 방향(debug → release) 승격 선례. 승격 조건(포커스 독립 + 대상 ID)이 이 건의 판정 기준
