# ADR-0337: 구조 op 실행은 도메인 값으로 답하고, 자원 회수는 cascade 한 자리가 소유한다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, boundary, close, attach, cascade, resource-reclamation, hexagonal
- **Group**: architecture

## Context

구조 변경(split / new-tab / close / move / convert / restore)은 진입점이 셋이다 — 사용자
GUI, 에이전트 IPC, 그리고 원격 mirror 가 forward 한 op. 셋 중 forward 경로만 `core` 안에
있고(`core::attach_runtime` 의 forward 실행), 그것이 두 방향으로 바깥을 불렀다.

- **inbound adapter 재사용**: forward 실행이 `serde_json` params 를 조립해
  `adapters::ipc::handler` 의 split / tab.create / tab.close / tab.move / pane.close /
  surface.close 를 부른다. 핸들러는 그 params 를 다시 파싱한다 — 같은 프로세스 안에서
  wire 형식을 한 바퀴 돈다.
- **wire 타입 조립**: 재사용할 핸들러가 없는 셋(convert / restore / move-surface)은
  `Core::apply` 를 직접 부르고도 **결과를 `JsonRpcResponse` 로 조립**했다. 그 응답은
  아무 데도 보내지지 않는다 — 함수가 곧바로 `resp.error` 로 되풀어 `Err(String)` 을
  만들고, 호출자가 회신하는 것은 `StructuralResult` 라는 전혀 다른 프레임이다. 실측
  (2026-09-20, main `63a777ecc`): 비-test 코드에서 그 조립이 **9 자리**였고 전부 이 형태였다.

자원 회수 쪽은 다른 형태로 흩어져 있었다. `Core::apply` 의 close 계열은 구조만 바꾸고
`cleanup_targets` 를 **반환**해 회수를 호출자에게 넘긴다. 그 회수 루프(=닫힌 surface 마다
`cleanup_surface` + `surface.closed` lifecycle enqueue)가 cascade 셋
(surface / pane / tab)에 각각 한 벌씩, gui 와 headless 양쪽에 있어 **여섯 벌**이었다.
여섯 벌은 이미 갈라져 있었다 — surface 경로만 `cleanup_surface_traced` 로 C5 계측을 모았고
tab/pane 경로는 안 모았다. 같은 형태의 복제가 하나 더 있었다:
`CoreEvent::MoveSurfaceApplied` 의 일곱 필드를 `SurfaceCloseCascade` 로 옮기는 매핑이
로컬 dispatcher 와 forward 실행 **두 자리**에 풀려 있었다.

## Decision

세 가지를 정한다.

**1. 구조 op 실행은 도메인 값으로 답한다.** forward 실행의 결과 타입은
`Result<(), String>` 이고, 핸들러를 안 거치는 op 는 wire 타입을 **만들지 않는다**.
재사용 핸들러가 `JsonRpcResponse` 로만 답하기 때문에 생기는 변환은 `handler_result`
**한 함수**가 어댑터 경계에서 수행한다.

**2. 닫힌 surface 의 자원 회수는 빌드 형태마다 한 함수가 소유한다.**
`reclaim_closed_surfaces` — gui 는 `app::dispatch_domain`, headless 는
`app::dispatch_domain_stubs` 에 같은 이름으로 둔다. 세 close cascade 가 그것을 부른다.
gui 와 headless 가 갈리는 지점은 **lifecycle 통지 한 줄**이고(headless 에는
`pending_lifecycle_events` 를 비우는 주체가 없어 enqueue 하면 큐가 무한 적재된다),
그 차이는 이제 두 함수의 본문 차이로만 존재한다 — 여섯 사본에 흩어져 있지 않다.
계측(C5) 발화 여부는 인자 하나로 드러낸다.

**3. `MoveSurfaceApplied` → close cascade 매핑은 한 이름
(`SurfaceCloseCascade::from_move_surface_applied`)이 소유한다.** 두 발행 경로(로컬
dispatcher · 원격 forward 실행)가 그것을 부른다. 다만 **이름이 하나인 것이 자리가 하나라는 뜻은 아니다** — 빌드 형태마다 사본이
하나씩 있고, 기본 빌드는 그중 하나만 본다. 아래 Consequences 의 해당 항이 그 모수를 센다.

**핸들러 재사용 자체는 이 결정의 범위 밖이다.** forward 실행이 여섯 IPC 핸들러를 부르는
것은 그대로 둔다 — 그 배선을 도메인 실행으로 바꾸려면 핸들러가 소유한 검증(nickname
해석 · required param · cwd inherit · caller 자기-pane 거부)을 함께 옮겨야 하고, 그것은
핸들러 층의 계약을 바꾸는 별도 작업이다. 여기서 정하는 것은 **그 재사용이 남아 있는 동안에도
경계 변환이 한 자리뿐이라는 것**이다.

## Consequences

- **얻은 것**: 아무 데도 안 보내지는 wire 응답 조립이 9 자리에서 0 이 됐다(남은 한 자리는
  `handler_result` 의 인자 타입 — 핸들러가 그 모양으로 답하기 때문이다). 자원 회수 루프가
  6 벌에서 2 벌로 줄었다 — 그리고 이것은 **두 조합 각각에서** 준 것이다(gui 가 보는 자리
  3 → 1 · headless 가 보는 자리 3 → 1). cascade 삼형제 중 어느 것이 계측을 모으고 어느
  것이 안 모으는지는 **인자 하나**로 보인다.
- **잃은 것**: `reclaim_closed_surfaces` 에 `trace: Option<&'static str>` 인자가 생겼다 —
  호출부 셋 중 하나만 `Some` 이다. 계측을 켤지를 호출자가 정한다는 사실이 시그니처에
  드러나는 대신, 인자 하나가 늘었다.
- **운영 비용 / 유지 부담**: gui 와 headless 의 `reclaim_closed_surfaces` 는 이름이 같고
  본문이 다르다. 한쪽에 단계를 더하면 다른 쪽에도 더할지를 판단해야 한다 — 그 판단 기준
  (통지에 소비자가 있는가)은 두 함수의 doc 에 적혀 있다. 이 저장소에는 그 짝을 재는 채널이
  없다(`dispatch_domain_stubs.rs` 는 `cfg(not(feature = "gui"))` 라 기본 빌드가 아예 안 본다).
- **★ `MoveSurfaceApplied` 매핑은 자리가 안 줄었고, 기본 빌드가 보는 자리는 오히려 하나
  줄었다 — 그것이 위험이다.** 모수를 밝혀 센다. 일곱 필드를 손으로 나열하는 자리는 이 결정
  전후로 **둘 그대로**다(전: `app::dispatch_domain` 의 arm · `core::attach_runtime` 의 arm /
  후: 두 `SurfaceCloseCascade::from_move_surface_applied` — gui 와 headless). 바뀐 것은
  **어느 조합이 그것을 보는가**다. 전에는 `core::attach_runtime` 이 두 조합 모두에서
  컴파일돼 **gui 조합이 두 자리를 다 봤다**. 지금은 gui 가 하나(`app::dispatch_domain`),
  headless 가 하나(`app::dispatch_domain_stubs`)다. 그래서 variant 에 필드를 더하면
  `cargo check --workspace` 는 gui 쪽만 지적하고 headless 쪽은 **한 마디도 안 한다** — 그
  상태로 착지하면 `--no-default-features` 빌드가 `E0027` 로 깨진다. 결정 3 은 *한 이름이
  매핑을 소유한다* 까지만 성립하고, *자리가 하나다* 로 읽으면 안 된다. 두 생성자의 doc 에
  이 사실을 박아 두었고, 재는 법은 조합 둘을 다 돌리는 것뿐이다. 자리를 실제로 하나로
  만들려면 `SurfaceCloseCascade` 를 `cfg` 밖 공용 모듈로 올려야 하는데, 그것은 모듈 선언
  (`src/app.rs`)을 바꾸는 별도 작업이다.

## Alternatives Considered

- **A. forward 실행을 `app` 으로 옮긴다** — `core` 가 `adapters` 와 `app` 을 부르는
  방향 역전이 통째로 사라진다. 안 고른 이유는 호출부와 판정기가 이 작업의 소유 경로 밖에
  있어서다: 호출자 둘(`app::event_handler` · `boot::headless_stream`)이 경로를 바꿔야 하고,
  `src/source_guards/auto_tap_suppression_window.rs` 는 즉시-tap 억제 구간을
  **`src/core/attach_runtime.rs` 안의 그 함수 본문에서** 잘라 읽는다 — 파일이 바뀌면 그
  판정기가 본문을 못 잘라 빨개진다(그 가드는 좌변이 비면 초록이 아니라 실패하도록 만들어져
  있다). 옮기는 작업과 그 판정기의 좌변을 옮기는 작업은 한 회차에 같이 가야 한다.
- **B. 자원 회수를 `Core::apply` 안으로 넣는다** — 호출자가 잊을 수 없게 된다. 안 고른
  이유는 `cleanup_surface` 가 `AppState` 메서드이고 `Core::apply` 는 `AppState` 를 안 받기
  때문이다. 받게 하면 도메인 진입점의 시그니처가 앱 상태를 요구하게 되어, 경계를 닫으려던
  변경이 경계를 더 흐린다.
- **C. `cleanup_targets` 를 `#[must_use]` 새 타입으로 감싼다** — 회수를 잊으면 경고가
  난다. 안 고른 이유는 `#[must_use]` 가 **표현식 결과**에만 붙고, 이 값은 이벤트 variant 를
  분해해 얻는 **바인딩**이라 분해 즉시 그 경고를 벗어나기 때문이다. 판정이 안 되는 채널을
  붙이면 붙었다고 믿게 된다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `core::attach_runtime` 의 forward 실행이 `adapters::ipc::handler` 를 더 이상 안 부르게
  되면(= 대안 A 가 실행되면) 결정 1 의 "재사용이 남아 있는 동안" 이라는 전제가 사라진다.
  `src/source_guards/auto_tap_suppression_window.rs` 가 그 자리를 이미 본다 — 그 파일의
  `the_guard_cuts_the_two_functions_and_not_some_others` 는 잘라 온 본문에
  `handle_tab_create(` 가 있는지를 단정하므로, 재사용이 사라지는 순간 그 시험이 좌표를 찍고
  죽는다. 이 조건은 ADR-0395 로 발화해 소진됐다 — 그 시험은 지금 `exec::create_tab(` 를 단정한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- gui 와 headless 의 `reclaim_closed_surfaces` 가 "통지에 소비자가 있는가" 말고 다른
  축으로도 갈리게 되면. 재는 법: 두 함수 본문을 나란히 놓고 줄 단위로 대조한다 — 두 파일은
  `cfg` 가 배타라 한 빌드가 둘을 같이 컴파일하지 않으므로, 이 대조를 컴파일러가 대신해 주지
  않는다. `cargo check --workspace --all-targets` 와
  `cargo check --workspace --no-default-features --all-targets` 를 둘 다 돌려야 양쪽이
  적어도 **컴파일은** 됐다는 것까지만 확인된다.

## References

- [close-sequence](../architecture/close-sequence.md) — 세 close 경로와 C1~C5 계측. 자원
  회수 소유 절이 이 결정의 현재 운영 상태를 기술한다.
- [ADR-0113](0113-close-preserves-the-focused-target.md) — 제거 후 활성 포인터 보정. gui 와
  headless cascade 가 같은 헬퍼를 지나야 하는 근거.
- [ADR-0264](0264-mirror-restore-closed-item-runs-on-the-remote.md) — forward 된 close 가 원격 사용자의 손
  조작이라 복원 스택에 들어간다는 결정. 이 ADR 의 결정 1 은 그 판정을 안 건드린다.
- [ADR-0480](0480-a-forwarded-close-carries-who-asked-for-it.md) — forward 된 close 의 복원 스택 여부는 이제 op 의 origin 이 정한다
- 코드 근거(결정이 실현된 현재 위치): `core::attach_runtime` 의 `forward_result` ·
  `core::structural_cascade` 의 `reclaim_closed_surfaces` ·
  `SurfaceCloseCascade::from_move_surface_applied`. `handler_result` 와 빌드 형태별 두 사본은
  ADR-0395 가 없앴다.
- 부분 개정: [0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md) (결정 2 의
  "빌드 형태마다 한 함수" 와 "핸들러 재사용 자체는 범위 밖" 조항 개정 — 구조 변경 실행과 cascade 가
  도메인 계층의 한 함수가 됐다)
