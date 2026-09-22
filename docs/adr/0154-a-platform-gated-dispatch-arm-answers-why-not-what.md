# ADR-0154: 플랫폼이 못 하는 메서드는 "없다" 가 아니라 "여기선 못 한다" 로 답한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: ipc, debug, cross-platform, error-codes, cli, guards, adr-0115
- **Group**: ipc-contract

## Context

`surface.raw_key` 와 `surface.switch_input_source` 는 OS 전역 입력 스트림에 이벤트를 넣는
debug 전용 메서드다(사용자 입력 재현이라 [ADR-0115](0115-input-reproduction-ipc-debug-isolation.md)
로 debug 격리). 구현은 macOS 의 `CGEventPost` / `TISSelectInputSource` 에 직접 얹혀 있어
다른 OS 에 대응물이 없다.

세 층이 이 메서드를 다루는데 조건이 서로 달랐다.

| 층 | 조건 | 결과 |
|----|------|------|
| 등재 (`DEBUG_METHODS`) | `debug_assertions` 만 | 모든 플랫폼에서 표에 있다 |
| CLI 서브커맨드 | 없음 | 모든 플랫폼에서 `--help` 에 뜬다 |
| dispatch arm (`handler.rs`) | `all(target_os = "macos", feature = "gui")` | macOS gui 에서만 있다 |

그래서 Linux/Windows debug 빌드에서 `tasty debug raw-key` 는 **도움말에 뜨는데** 실행하면
`match` 의 `_` 로 떨어져 `-32601 Method not found: surface.raw_key` 로 끝났다(실측,
2026-09-05 Linux debug census).

그 답은 거짓이다. `-32601` 은 "그런 메서드는 없다" 는 뜻이고, 호출자가 그것을 받으면
**이름을 의심한다** — 오타를 고치거나 표를 다시 읽는다. 실제 사실은 이름이 맞고 표에도
있으며 이 플랫폼이 못 한다는 것이다. 두 상황은 고칠 방법이 다르므로 같은 코드로 답하면
호출자를 틀린 방향으로 보낸다.

## Decision

**등재와 CLI 는 플랫폼 균일하게 두고, 플랫폼 차이는 dispatch 층에서 코드와 사유로 답한다.**
`#[cfg(target_os = …)]` 로 좁힌 dispatch arm 에는 그 여집합을 받는 상보 arm 을 반드시 짝으로
두고, 그 arm 은 새 코드 `-32015` 와 **왜 못 하는지**를 돌려준다.

    -32015  input reproduction over the OS event stream is macOS-only and needs the gui
            build (CGEventPost / TISSelectInputSource have no equivalent here)

이 선택의 근거는 취향이 아니라 이 저장소의 기존 관례다. 실측(2026-09-05): `crates/tasty-cli/src/`
와 `crates/tasty-ipc/src/method_meta.rs` 에 `target_os` 게이트가 **0 건**이다. 두 층은 이미
플랫폼 균일하고, 차이는 이미 dispatch 층에만 있었다 — 빠진 것은 그 층의 상보 arm 하나였다.

짝이 맞는지는 `src/source_guards/platform_gated_dispatch_complement.rs` 가 강제한다.

## Consequences

- **얻은 것**: 호출자가 "오타" 와 "여기선 안 됨" 을 응답만으로 가른다. `-32601` 을 받으면
  이름을 의심하고, `-32015` 를 받으면 플랫폼을 본다. 실측으로 답이 바뀌었다 —
  변이(상보 arm 제거)로 `-32601` 이 되돌아오는 것을 확인했다.
- **얻은 것**: 등재·CLI·dispatch 세 층의 조건이 어긋난 자리가 가드로 고정됐다. 새 플랫폼
  게이트를 dispatch 에 넣으면 상보 arm 없이는 통과하지 못한다.
- **잃은 것**: 에러 코드가 하나 늘었다(`-32015`). 문서에 코드표 항목이 하나 더 생긴다.
- **운영 비용**: 상보 arm 의 사유 문자열은 손으로 쓴다. 구현이 다른 플랫폼으로 확장되면
  arm 과 문자열을 함께 지워야 하고, 안 지우면 가드가 아니라 사람이 알아채야 한다.
- **범위**: 이 결정은 dispatch 층의 `target_os` 게이트에만 걸린다. `feature = "gui"` 게이트는
  다른 축이다 — 헤드리스에서 창이 필요한 메서드가 없는 것은 정의이지 플랫폼 결손이 아니다
  ([headless-ipc-surface](../dev-guide/headless-ipc-surface.md)).

### 적용 범위 확장 — 조합 게이트 (2026-09-05, 후속 트랙)

같은 판정을 `feature = "gui"` 게이트에도 적용한다. 위 결정이 "플랫폼이 못 하는 것" 을
다뤘다면 이쪽은 "조합에 자리가 없는 것" 인데, **판별식이 하나**다: 그 핸들러가 창(또는
렌더러·egui 입력 큐)을 읽는가.

- **안 읽는데 게이트된 것은 결함이다.** 모듈 위치가 게이트를 상속시켰을 뿐이므로, 게이트를
  느슨하게 하는 것이 아니라 **핸들러를 게이트 밖으로 옮긴다**(느슨하게 하면 같은 모듈의
  다른 것까지 딸려 나온다). 실측으로 셋을 옮겼다 — `theme.query`(release 표면),
  `debug.lua.eval` · `debug.event_bus.*` · `debug.extension.invoke_hook`(debug 표면).
- **읽는 것은 없는 것이 정답이고, 사유를 문서 표에 적는다.** 답이 `-32601` 인 것은 이때만
  참이다 — 창이 없으면 그 메서드가 하는 일 자체가 없다.

`-32015`("있는데 여기선 못 한다")는 이 확장에 쓰지 않는다. 플랫폼은 같은 빌드에서 메서드가
**존재하는데** 못 하는 것이고, 조합은 그 자리가 애초에 없는 것이라 두 답이 서로 다른 사실을
말한다.

debug 표면을 열 때 지킨 경계 하나: 여는 것은 **헤드리스 debug 빌드에서도 답한다**이지
release 노출이 아니다(identity 원칙 1). release 헤드리스 실행 대조로 확인한다 — 연 다섯이
release 에서 전부 `-32601` 이다.

## Alternatives Considered

- **A: `DEBUG_METHODS` 등재에 `target_os` 조건을 건다** — 표에서 빼면 `-32601` 이 참이 되므로
  일관은 맞는다. 안 고른 이유: 등재 표는 문서·권한 판정·`plugin_callable` 메타를 함께 나르는
  플랫폼 중립 카탈로그이고, 지금 `target_os` 게이트가 0 건이다. 여기에 플랫폼 조건을 처음
  들이면 "이 표는 무엇의 목록인가" 가 조합마다 달라진다. 그리고 CLI 를 함께 안 고치면
  도움말에는 여전히 뜨므로, 고칠 자리가 하나가 아니라 둘이 된다.
- **B: CLI 서브커맨드를 `#[cfg(target_os = "macos")]` 로 숨긴다** — 도움말에서 사라지니 혼동은
  준다. 안 고른 이유: 원격 IPC 호출자는 CLI 를 거치지 않는다. 숨겨도 그쪽은 여전히 `-32601`
  을 받으므로 문제의 절반만 덮는다. 게다가 CLI 표면이 플랫폼마다 달라지면
  [identity](../identity.md) 원칙 2 의 "IPC + CLI 양면" 이 조합 의존이 된다.
- **C: 그냥 `-32601` 로 둔다(현상 유지)** — 안 고른 이유가 Context 다. 그 답은 참이 아니고,
  참이 아닌 답은 호출자를 틀린 수리로 보낸다.
- **D: 상보 arm 없이 `_` 팔에서 이름을 보고 갈래를 친다** — arm 을 안 늘려도 된다. 안 고른
  이유: 이름 목록이 dispatch 위치에서 떨어져 나가 둘이 따로 자란다. 이 저장소는 같은 실패형
  (같은 로직이 두 곳에 복제돼 서로 다르게 자란 것)을 이미 겪었다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 입력 재현 구현이 macOS 밖으로 확장된다(예: Linux 의 `uinput`, Windows 의 `SendInput`).
  그때는 상보 arm 이 좁아지거나 사라진다.
- `DEBUG_METHODS` 나 CLI 에 `target_os` 게이트가 처음으로 들어간다 — 그 순간 "두 층은 플랫폼
  균일" 이라는 이 결정의 전제가 깨진다.
- 플랫폼 결손을 나르는 dispatch arm 이 여럿으로 늘어 사유 문자열이 반복되기 시작한다.
  그때는 사유를 메서드 메타로 옮기는 편이 나을 수 있다.

## References

- [ADR-0115](0115-input-reproduction-ipc-debug-isolation.md) — 입력 재현 IPC 의 debug 격리
- [debug-ipc](../dev-guide/debug-ipc.md) — debug 전용 IPC 와 에러 코드표
- `src/adapters/ipc/handler.rs` — 상보 arm 과 사유 상수
- `src/source_guards/platform_gated_dispatch_complement.rs` — 짝을 강제하는 가드
