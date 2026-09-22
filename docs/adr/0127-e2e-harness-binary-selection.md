# ADR-0127: e2e 하네스가 띄울 바이너리는 한 곳에서 정한다 — 기본 조합의 GPU 종속은 `App` 이분이 선행이다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: testing, e2e, headless, gpu, feature-flags, cargo, harness, adr-0090
- **Group**: testing

## Context

e2e 하네스(`tests/common`·`tests/webhook_common`)는 `CARGO_BIN_EXE_tasty` 를 띄운다 — **테스트 자신과 같은 feature 로 빌드된 자기 바이너리**다. 기본 조합에서 그것은 GUI 바이너리이고, GUI 부팅은 창 + wgpu 디바이스를 만든 뒤 `finish_boot` 안에서 **비로소** IPC 를 시작한다. 즉 GPU 초기화가 막히면 IPC 는 시작조차 못 하고 port file 이 안 써진다.

문제는 그 대가를 치르는 스위트가 GPU 를 검증하지 않는다는 것이다. 인스턴스를 띄우는 test binary 11 개(기본 조합에서 실제로 도는 것) 중 화면을 보는 것은 2 개뿐이고 나머지 9 개는 IPC(JSON-RPC)와 attach stream 프레임만 쓴다. 실측(2026-09-04): 워크트리 4 곳이 동시에 스위트를 돌린 날 기록된 **모든** 실패가 같은 지점(port file 미작성, 30 초 상한)에서 났고, run 마다 깨지는 스위트가 달랐다 — 코드 인과가 아니라 GPU 디바이스 경합의 무작위 희생자라는 직접 증거다.

`--headless` 플래그는 존재하지만 gui 빌드에서는 무시된다. `run_headless` 와 `App::new_headless` 가 `#[cfg(not(feature = "gui"))]` 라 headless 진입점이 그 빌드에 **존재하지 않기** 때문이다.

## Decision

**하네스가 띄울 바이너리는 `spawn_diag::instance_bin()` 한 곳에서 정한다.** 기본값은 `CARGO_BIN_EXE_tasty` 그대로이고, `TASTY_E2E_BIN` 이 있으면 그 경로를 띄운다. 두 하네스와 웹훅 CLI 러너가 모두 이 함수를 거치며, `tests/e2e_single_instance_guard.rs` 의 세 번째 축이 다른 자리에서의 직접 선택을 막는다.

그 위에서 **기본(gui) 조합의 GPU 종속은 당분간 유지한다.** 두 대안이 지금은 닫혀 있기 때문이다.

- **런타임 headless 분기는 `App` 이분이 선행이다.** gui 빌드에서 `--headless` 가 실제로 동작하려면 `run_headless`/`App::new_headless` 의 `cfg` 를 걷어내야 하는데, `src/app.rs` 한 파일에만 `#[cfg(feature = "gui")]` 가 약 80 곳이고 그중 다수가 `App` 구조체의 **필드**다. gui 필드를 전부 `Option` 으로 바꾸거나 타입을 이분하는 본체 구조 변경이고, 그 자체가 별도 결정을 요구한다.
- **별도 test-only 바이너리 타깃은 cargo 구조상 막혀 있다.** 루트 패키지에 lib 타깃이 없어 다른 워크스페이스 멤버가 `default-features = false` 로 의존할 대상이 없고, 만들더라도 resolver v2 의 feature 통합이 normal dependency 사이에서 그대로 일어나 같은 `cargo test --workspace` 안에서 누군가 `gui` 를 켜면 headless 쪽도 함께 켜진다.

**반면 `--no-default-features` 조합에서는 전환이 이미 일어나 있다.** 같은 `CARGO_BIN_EXE_tasty` 가 그 조합에서는 곧 headless 데몬이고, 실측(`DISPLAY`·`WAYLAND_DISPLAY` 둘 다 없는 상태)으로 port file 까지 **54 ms** 다 — 기본 조합이 GPU 경합에서 30~56 초 만에 실패하던 그 단계다. `TASTY_E2E_BIN` 은 이 사실을 기본 조합에서도 쓸 수 있게 하는 로컬 탈출구다.

**GUI 가 실제로 필요한 스위트는 두 개로 확정한다** — `tests/attach_attention_loopback.rs`(매 프레임 실-포커스 해제가 검증 대상 그 자체)와 `tests/e2e_tests.rs`(`window.create` + gui cfg 로 묶인 debug 메서드). (**이 목록은 그 뒤 하나로 줄었다 — 아래 「갱신」 절.** 아래 문단들은 작성 시점의 근거를 그대로 보존한 것이다.) 이 판정은 **세 독립적인 방법이 같은 경계선을 그린 것**에 근거한다: ① 파일별 코드 독해로 얻은 목록, ② 헤드리스 조합 전수 실행에서 실제로 실패한 목록, ③ 아래 `TASTY_E2E_BIN` 으로 headless 인스턴스를 띄워 돌린 IPC 전용 9 개가 통과한다는 반대편 확인. ①②는 "GUI 가 필요한 쪽" 을, ③은 "필요 없는 쪽" 을 봤다 — 본 방향이 서로 달랐는데 같은 선이 나왔다. 한 방법만 쓰면 "소스만 보고 실행을 오판" 하거나 "실행 결과만 보고 원인을 오판" 하는데, 이 세션의 오판이 모두 그 형태였다.

## 발화한 트리거 — `check-headless` 가 전체 스위트로 올라갔다 (병합됨)

본 ADR 이 첫 재검토 트리거로 걸어 둔 조건이 **작성과 병합 사이에 충족됐다.** `check-headless`
잡이 `--lib --bins` 에서 **전체 스위트**로 올라갔고(`--skip` 3 건 제외), 그 결과:

- **IPC 전용 통합 검증이 자동 채널에서 headless 로 돈다.** `TASTY_E2E_BIN` 은 여전히 로컬
  탈출구이지만, "IPC 전용 스위트가 GPU 를 통과한다" 가 CI 에서는 더 이상 참이 아니다.
- **기본(gui) 조합의 GPU 종속을 유지한다는 결정 자체는 그대로다.** 기본 조합에는 여전히
  전체 스위트의 자동 실행 채널이 없고(Windows 잡은 `--lib --bins`), 대안 A·B 를 막고 있던
  이유(`App` 이분 선행 / cargo 구조)도 그대로다. 바뀐 것은 *어느 조합이 그 검증을 자동으로
  보는가* 이지 *기본 조합을 어떻게 부팅하는가* 가 아니다.

**부수 확인 하나 — GUI 필수 스위트 2 개 판정이 독립적으로 재확인됐다.** (**이 네 번째 근거는 그 뒤 사라졌다 — 아래 「갱신」 절.**) 그 잡의 명명 skip
3 건 중 둘(`hard_occupied_attention_survives_the_servers_local_focus` ·
`all_e2e_tests`)이 본 ADR 이 "GUI 가 실제로 필요하다" 고 지목한 두 스위트
(`tests/attach_attention_loopback.rs` · `tests/e2e_tests.rs`)의 테스트다. 헤드리스 잡을
넓힌 쪽은 본 조사와 다른 경로로 같은 경계선에 도달했다 — Decision 이 근거로 든 "세 방법의
수렴" 에 네 번째가 붙은 셈이다. (세 번째 skip 은 GUI 요구가 아니라 headless 배선 결함이다.)

## 갱신 — GUI 를 요구하는 것은 스위트 하나가 아니라 테스트 하나다

위 Decision 이 "GUI 가 실제로 필요하다" 고 지목한 두 스위트 중 하나가 그 뒤 헤드리스에서
전수 통과하게 됐다. `check-headless` 의 명명 skip 세 건 중 둘이 닫혔기 때문이다 — 하나는
output-match 훅이 헤드리스에 배선되지 않은 결함이었고(위 괄호가 "GUI 요구가 아니라 배선
결함" 이라고 적어둔 그 세 번째 skip), 다른 하나가
`hard_occupied_attention_survives_the_servers_local_focus` 다. 후자는 그 테스트가 쓰는 debug
nav 핸들러가 `gui` feature 게이트 아래 있어서 헤드리스에 존재하지 않았던 것이고, 핸들러를
`debug_assertions` 게이트의 별도 모듈로 옮기자 풀렸다. **그 스위트가 GUI 를 요구한 것이
아니라 핸들러가 `gui` feature 에 묶여 있었을 뿐**이라는 것이 사후에 드러났다.

실측(헤드리스 조합, 명명 skip 은 `all_e2e_tests` 하나):

- `tests/attach_attention_loopback.rs` — 15 passed / 0 failed / 0 ignored / 0 filtered out
- `tests/e2e_tests.rs` — 8 passed / 0 failed / 0 ignored / 1 filtered out

따라서 이 절 시점의 상태는 이렇다. (**이 절의 "하나" 가 가리키는 테스트는 그 뒤
바뀌었다 — 아래 「발화한 트리거를 닫는다」 절.** 지금 그 자리는 `multi_window_owner_routing`
이고, `all_e2e_tests` 라는 이름은 소스에 없다.)

- **GUI 를 실제로 요구하는 것은 `tests/e2e_tests.rs` 의 `all_e2e_tests` 하나다.** 경계는
  스위트가 아니라 테스트 단위로 그어진다.
- 워크플로의 명명 skip 은 **1 건**이다(위 "발화한 트리거" 절이 3 건이라고 적은 그 목록).
- 위 "부수 확인" 의 네 번째 수렴은 **근거가 사라졌다** — 그 근거로 든 skip 두 건 중 하나가
  없어졌다. Decision 이 든 세 방법의 수렴은 그대로다.

**Decision 자체는 열지 않는다.** 띄울 바이너리를 한 곳에서 정한다는 것도, 기본(gui) 조합의
GPU 종속을 당분간 유지한다는 것도 그대로이며, 대안 A·B 를 막고 있던 이유도 변하지 않았다.
바뀐 것은 *그 결정의 영향 범위를 서술한 사실* 뿐이다.

**다만 재검토 트리거 하나가 이 갱신으로 강해진다.** 대안 A(`App` 이분 선행)가 열리면 기본
조합도 헤드리스로 띄울 수 있게 되는데, 그때 GUI 를 요구하는 표면이 스위트 두 개가 아니라
테스트 하나라면 옮겨야 할 범위도 그만큼 작다.

### 발화한 트리거를 닫는다 — 경계는 **테스트 단위**로 긋는다 (결정 개정)

위 갱신은 사실이고, 그 사실이 본 ADR 의 재검토 트리거 하나를 **실제로 발화시켰다**:
*"GUI 가 필요한 스위트가 2 개에서 늘거나 줄면 — 그 판정이 이 결정의 범위를 정한다."*
발화한 트리거는 재검토로 닫거나 다시 쓰지 않으면 남아서 **영원히 울린 상태**가 된다.
재검토한 결과를 여기 적는다.

**개정 내용은 수가 아니라 단위다.** "2 개" 를 "1 개" 로 고치는 것은 사실 갱신이고 위 절이
이미 했다. 결정 차원에서 바뀌는 것은 이것이다.

> **어떤 검증이 GUI 를 요구하는지는 스위트가 아니라 테스트 단위로 판정한다.**

근거는 갱신 절의 실측 그 자체다. `tests/attach_attention_loopback.rs` 는 **스위트 통째로**
GUI 를 요구한다고 판정됐지만, 실제로 GUI 에 묶여 있던 것은 그 스위트가 아니라 **한 테스트가
쓰는 핸들러가 `gui` feature 게이트 아래 있었다**는 사실이었다. 게이트를 옮기자 스위트 15 개가
전부 헤드리스에서 통과했다. **스위트 단위 판정은 참인 명제 하나를 거짓 명제 열넷과 함께
묶어 놓았던 것**이고, 그 형태는 다시 나온다 — 한 테스트가 GUI 를 요구하면 같은 파일의 다른
테스트도 그렇다고 읽게 만든다.

운영상 따라오는 것 둘.

- **워크플로의 명명 skip 은 테스트 이름으로 적고, 건마다 이유를 붙인다.** 파일 단위로
  뭉뚱그리지 않는다. 이유는 "GUI 를 요구한다" 와 "배선 결함이라 고치면 풀린다" 를 구분해
  적는다 — 후자는 skip 을 지우는 것이 목표라는 뜻이다.
- **"이 스위트는 GUI 가 필요하다" 는 형태의 판정을 더는 쓰지 않는다.** 스위트가 통째로
  못 도는 경우에도, 그것이 *파일 단위의 성질* 인지 *그 파일이 한 테스트로 묶여 있는 구조*
  때문인지를 구분해 적는다.

**이 개정이 지목한 구조는 그 뒤 실제로 풀렸다.** 위 두 번째 불릿이 예로 든
`all_e2e_tests`(33 개 시나리오가 한 `#[test]` 안에 있어 `--skip` 으로 가를 수 없던 것)를
시나리오 단위 `#[test]` 로 쪼갰다(Unix 기준 19 개 — `dim_sgr2_survives_to_the_renderer`
가 `cfg(not(windows))` 라 Windows 에서는 18 개다). 창을 요구하는 단언은 `multi_window_owner_routing`
하나에 모였고(`window.create` — gui 라우터의 `app_methods` step 에만 있다), 나머지 18 개는
헤드리스에서 실제로 돈다. 실측(헤드리스 조합, `--skip multi_window_owner_routing`):
`tests/e2e_tests.rs` — **26 passed / 0 failed / 1 filtered out**(하네스 자체 테스트 8 건
포함). 쪼개기 전 같은 조합의 같은 파일은 8 passed / 1 filtered out 이었다.

이로써 **skip 의 단위가 파일에서 테스트로 내려왔다** — 개정이 규정한 형태가 워크플로에
실물로 존재하게 된 것이고, 명명 skip 1 건은 이제 "GUI 요구" 라는 사유를 정확히 하나의
테스트에만 적용한다. 이름의 정확성(죽은 skip / 과대 매칭)은
`crates/tasty-doc-guards/tests/headless_skip_names_are_exact.rs` 가 강제한다.

**Decision 본문은 열지 않는다.** 띄울 바이너리를 한 곳에서 정한다는 것, 기본(gui) 조합의
GPU 종속을 유지한다는 것, 대안 A·B 를 막는 이유 — 셋 다 이 개정과 무관하게 그대로다.
바뀐 것은 **그 결정이 어느 단위로 범위를 재는가** 이다.

## Consequences

- **얻은 것**: 무엇을 띄우는지가 한 곳에 모였다 — 나중에 기본 조합을 전환할 때 바꿀 자리가 하나다. GPU 경합이 실재하는 환경(워크트리 병렬)에서 IPC 전용 스위트를 GPU 밖으로 뺄 수단이 생겼다. "IPC 전용 검증이 GPU 를 통과한다" 는 사실과 그 이유가 문서·ADR 에 남았다.
- **잃은 것**: 기본 조합의 GPU 종속은 그대로다 — `TASTY_E2E_BIN` 은 **선택적 탈출구**이지 문제의 제거가 아니다. 쓰려면 사람이 headless 를 따로 빌드해야 한다.
- **운영 비용 / 유지 부담**: override 는 반드시 별도 `CARGO_TARGET_DIR` 의 산출물을 가리켜야 한다 — 같은 target 디렉토리는 `target/debug/tasty` 를 다투므로 다음 `cargo test` 가 headless 를 gui 로 덮어쓴다. 이 함정을 `docs/dev-guide/e2e-tests.md` §0-1 과 함수 doc 에 적었고, 존재하지 않는 경로는 하네스가 그 자리에서 실패시킨다.
- headless 로 띄우면 IPC 표면이 다르다 — 어느 스위트가 왜 빠지는지는 워크플로의 headless 스텝 주석이 정본이며 다른 문서는 그것을 가리키기만 한다.

## Alternatives Considered

- **A. gui 빌드에서 `--headless` 를 런타임 분기로 만든다** — 방향으로는 이것이 맞다(검증 대상이 IPC 인데 GPU 를 통과할 이유가 없다). 지금 고르지 않은 이유는 `App` 이분이 선행이라는 것뿐이고, 그것이 끝나면 이 ADR 의 결정을 다시 연다.
- **B. `--no-default-features` 로 빌드한 별도 test-only bin 타깃** — cargo 구조상 막혀 있다(위 Decision). 워크스페이스 밖으로 빼면 `cargo test --workspace` 가 그 바이너리를 만들지 않아 하네스가 사전 빌드 산출물에 의존하게 되는데, 그 의존은 "낡은 바이너리를 재고도 초록으로 읽는" 형태를 새로 들인다.
- **C. 구조는 두고 세마포어·직렬화로만 막는다** — 현행 완화책의 고착이다. 실패까지의 시간만 늘리고 GPU 를 못 잡는 환경에서는 애초에 풀리지 않는다.
- **D. GUI 부팅 순서에서 IPC 시작을 GPU 초기화 앞으로 옮긴다** — GPU 가 **느린** 경우는 풀리고 **없는** 경우는 안 풀린다. 게다가 IPC 핸들러가 `AppState` 를 요구해 "port file 만 먼저 쓰고 핸들러는 나중" 이라는 반쪽 상태의 설계가 선행 쟁점이 된다. 부팅 순서 변경은 A 와 같은 표면을 건드리므로 A 와 함께 판단하는 것이 맞다.
- **E. spawn 상한을 올린다** — GPU 를 못 잡으면 60 초든 120 초든 실패하고 실패까지의 시간만 길어진다. 상한 통일과 실패 원인 판정은 이미 별도로 처리했다(`tests/spawn_diag`).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **기본 조합에도 전체 스위트의 자동 채널이 생기면** — 지금은 헤드리스 조합에만 있다(위 "발화한 트리거" 절). 기본 조합까지 자동으로 돌기 시작하면 GPU 경합이 게이트에 직접 얹히므로, 탈출구를 기본값으로 올릴지를 그때 판단한다.
- `App` 의 gui 필드 이분이 이루어져 gui 빌드에서 런타임 headless 분기가 가능해지면 — 대안 A 를 다시 연다.
- 루트 패키지에 lib 타깃이 생기거나 cargo 의 feature 통합 규칙이 바뀌어 대안 B 가 열리면.
- ~~GUI 가 필요한 스위트가 2 개에서 늘거나 줄면~~ — **발화했고 닫혔다**(위 「발화한 트리거를
  닫는다」 절). 단위를 테스트로 바꿔 다시 건다: **`check-headless` 의 명명 skip 이 1 건에서
  늘면** — 판정: `.github/workflows/crossplatform-check.yml` 의 `--skip` 인자 개수. 늘어난
  건이 "GUI 요구" 인지 "배선 결함" 인지에 따라 이 결정의 범위가 달라진다. **줄어드는 경우
  (0 건)는 별개다** — 그러면 헤드리스가 전 스위트를 돌리므로 대안 A 의 압력이 커진다.
  **이 트리거에는 자동 채널이 없다**(base `1837a307` 기준) — `--skip` 개수를 세는 것은
  사람이고, 그 개수가 이 문서의 서술과 어긋나도 아무것도 울리지 않는다.
- `TASTY_E2E_BIN` 이 탈출구가 아니라 상시 경로가 되면(예: CI 가 그것으로 돌기 시작하면) — 선택적 override 가 아니라 기본 동작이므로 결정을 다시 쓴다.

## References

- [ADR-0090](0090-test-isolation-by-workspace-not-process.md) — 격리 단위는 프로세스가 아니라 workspace
- [dev-guide/e2e-tests.md](../dev-guide/e2e-tests.md) §0-1 — 어느 바이너리를 띄우는가, 탈출구 절차와 함정
- [dev-guide/ci-gates.md](../dev-guide/ci-gates.md) — 어느 검사가 언제 도는지의 정본
- `tests/spawn_diag/mod.rs` — `instance_bin()`(선택) · spawn 상한 · 실패 원인 판정
- `tests/e2e_single_instance_guard.rs` — 세 번째 축(바이너리 선택의 초크포인트 강제)
