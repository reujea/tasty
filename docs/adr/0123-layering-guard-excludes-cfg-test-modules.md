# ADR-0123: 계층 가드는 `#[cfg(test)]` 전용 모듈을 범위 밖으로 둔다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: layering, guards, testing, tasty-cli, adr-0105
- **Group**: guard-design

## Context

`crates/tasty-doc-guards/tests/layering.rs` 는 본체(`src/`)가 `tasty-cli` 크레이트를 직접 참조하면 실패한다.
목적은 의존 방향의 역전을 막는 것이다 — GUI 런타임·IPC 핸들러·앱 상태가 CLI 진입
계층 내부를 들여다보면, CLI 타입 변경이 GUI 를 깨고 재사용 목적의 로직이 CLI 안에
눌러앉는다. 가드가 없으면 위반이 컴파일 에러로 잡히지 않는다 — `src/adapters/cli.rs`
가 `pub use tasty_cli::*;` 로 와일드카드 재수출을 하기 때문이다.

여기에 새 상황이 생겼다. IPC 핸들러가 읽는 **params 를 CLI 가 올바르게 조립하는가**
라는 축을 검사하는 테스트(`src/adapters/ipc/handler/cli_entry_tests.rs`)가 필요해졌다.
`tasty_cli::request::command_to_request` 의 출력을 프로덕션 핸들러에 그대로 먹여
성공/예상 에러를 단정하는 형태라, 정의상 **양쪽을 동시에 참조**해야 한다.

이 축은 기존 가드 어느 것도 보지 않는다. `tests/cli_naming_count_drift.rs` 는
`METHOD_TABLE` 의 네임스페이스별 **개수**만 세고, `tests/ipc_router_table_parity.rs` 는
**라우터 팔 ↔ 표** 방향만 본다. CLI 가 무엇을 어떤 파라미터로 보내는지는 둘 다 입력이
아니다.

그런데 그 테스트를 `tests/` 통합 테스트로 옮길 수 없다. `tasty` 패키지에는 `[lib]`
타깃도 크레이트 루트의 `lib.rs` 도 없다 — 바이너리 전용 크레이트다. 통합 테스트 크레이트가 링크할
대상 자체가 없으므로, 항목을 `pub` 으로 올려도 닿지 않는다. 가시성 문제가 아니라
링크 대상의 부재다.

### 2026-09-20 정정 — 이 근거의 앞쪽 절반이 사라졌다

[ADR-0325](0325-the-root-package-splits-into-a-lib-and-a-bin.md) 가 루트에 `[lib]` 를
세웠고, 그래서 아래 재검토 조건 셋째가 **실제로 발동했다**(채널:
`crates/tasty-doc-guards/tests/layering_guard_premises_still_hold.rs`). 조건이 지시한 순서대로
다시 봤고, **결론은 바뀌지 않았다 — 막는 것이 바뀌었을 뿐이다.**

`tests/` 가 이제 `tasty::` 를 링크할 수는 있다. 그러나 이 면제 항목이 쓰는 픽스처가
`crate::state::tests::test_state` 와 `crate::adapters::test::*` 이고, 그 둘은 부모에서
`#[cfg(test)]` 로만 선언돼 **lib 산출물에 애초에 들어가지 않는다.** 통합 테스트가 링크하는
rlib 에 그 심볼이 없으므로, 가시성을 `pub` 으로 올려도 닿지 않는다. 그래서 결정(아래
`TEST_ONLY_FILES`)은 그대로 두고, 근거를 "링크 대상의 부재" 에서 **"픽스처가 출하 범위
밖"** 으로 고쳐 적는다. 재검토 조건 셋째도 같은 좌변으로 옮겼다.

## Decision

계층 가드에 **세 번째 목록** `TEST_ONLY_FILES` 를 둔다 — `(경로, 사유)` 쌍이며,
`#[cfg(test)]` 로만 컴파일되는 모듈을 가드의 **범위 밖**으로 선언한다. 그 참조는
프로덕션 바이너리에 존재하지 않으므로 이 가드가 겨냥하는 의존 방향 역전이 애초에
일어나지 않는다.

면제는 주장이 아니라 **검사된 사실**이어야 하므로 두 방향을 함께 강제한다.

1. **전제 검사** — 면제된 파일이 정말로 부모 모듈에서 `#[cfg(test)]` 아래 선언돼
   있는지 확인한다(`declared_under_cfg_test`). 누가 그 attribute 를 떼면 면제가
   조용히 프로덕션 참조를 허용하게 되므로, 그 순간 가드가 실패한다.
2. **stale 검사** — 면제된 파일에 실제 참조가 없으면 실패한다. 참조를 걷어냈으면
   목록에서도 지워야 한다. `BASELINE_FILES` 가 이미 받는 대우와 같다.

세 목록은 서로 겹칠 수 없다(교차 검사).

## Consequences

- **얻은 것**: CLI↔핸들러 파라미터 축을 검사하는 테스트를 유지하면서 프로덕션
  레이어링 가드의 강도를 낮추지 않았다. 면제의 근거(`#[cfg(test)]`)가 코드에서
  사라지면 가드가 즉시 깨지므로, 면제가 시간이 지나며 넓어지지 않는다.
- **잃은 것**: 가드가 소스 텍스트를 한 겹 더 해석한다 — 부모 모듈 파일을 찾아
  `mod <name>;` 선언과 그 앞줄을 읽고, 그 앞줄이 **정확히 `#[cfg(test)]`** 일 것을
  요구한다. `#[cfg(all(test))]` 같은 동등한 변형이나 한 줄에 여러 선언을 적는 형태는
  통과하지 못한다. 텍스트 검사로 cfg 술어를 일반적으로 해석하려 들면 조용한 오인이
  생기므로, 인식 못 하는 형태는 **실패**시킨다 — 거짓 통과가 아니라 거짓 실패라
  방향이 안전하고, 면제 대상은 몇 개 안 되므로 정규 형태를 요구하는 비용이 작다.
  판별력 자체는 `the_cfg_test_precondition_check_discriminates` 가 게이트 없는 실제
  형제 모듈로 고정한다.
- **운영 비용 / 유지 부담**: 항목이 늘 때마다 사유를 함께 적어야 한다.
  `BASELINE_FILES` 와 달리 "비워야 하는 목록" 이 아니므로 0 을 목표로 삼지 않는다 —
  대신 각 항목이 왜 `tests/` 로 못 가는지가 사유에 남는다.

## Alternatives Considered

- **A: `BASELINE_FILES` 에 추가한다** — 한 줄로 끝나지만 그 목록은 "이행 중인 기존
  위반의 스냅샷, 줄어들기만 한다" 는 뜻을 갖는다. 테스트 모듈은 없앨 대상이 아니므로
  넣는 순간 목록의 의미가 거짓이 되고, "언젠가 비운다" 는 목표를 영원히 달성할 수
  없게 된다. 성격이 다른 것을 같은 목록에 담지 않는다.
- **B: 테스트를 `tests/` 로 옮기고 필요한 항목만 최소 노출한다** — 가장 깨끗한 형태였을
  것이다. **불가능하다**: `tasty` 는 `[lib]` 타깃이 없는 바이너리 전용 크레이트라
  통합 테스트가 링크할 대상이 없다. `pub(crate)` 시드나 테스트 전용 seam 을 아무리
  열어도 `tests/` 에서는 `tasty::` 경로 자체가 존재하지 않는다(레포의 어떤 통합
  테스트도 `use tasty::` 를 쓰지 않는 이유다). lib 타깃을 신설하는 것은 이 가드의
  범위를 훨씬 넘는 구조 변경이라 별개 결정이다.
  - **2026-09-20**: 그 별개 결정이 났다([ADR-0325](0325-the-root-package-splits-into-a-lib-and-a-bin.md)).
    `tasty::` 경로는 이제 존재한다. 그래도 B 는 여전히 불가능하다 — 이 항목이 쓰는
    픽스처가 `#[cfg(test)]` 전용이라 lib 산출물 밖이고, 통합 테스트가 링크하는 rlib 에
    그 심볼이 없다. B 를 열려면 픽스처를 `#[cfg(test)]` 밖으로 꺼내거나 별도 크레이트로
    빼야 하고, 그 둘은 이 ADR 이 아니라 그때의 결정이다.
- **C: 가드에서 `FORBIDDEN` 매칭을 `#[cfg(test)]` 블록 밖으로 한정한다** — 파일 단위
  목록 없이 자동으로 판정하는 형태. 그러려면 가드가 Rust 를 파싱해 attribute 의
  적용 범위를 알아야 한다. 텍스트 스캔으로 흉내 내면 조용한 미스캔(거짓 통과)이
  생기고, 그건 가드가 없는 것보다 나쁘다. 명시 목록 + 전제 검사가 같은 안전성을
  훨씬 싸게 준다.
- **D: 그 테스트를 포기한다** — CLI 가 조립한 params 를 핸들러가 실제로 읽는지 보는
  가드가 레포에서 사라진다. 그 축에서 실제로 결함이 있었고(IPC 전용 메서드에 CLI
  진입점을 열면서 테스트가 없었다), 그것이 어느 가드에도 안 걸린다는 것이 확인됐다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `tasty-cli` 가 루트 `Cargo.toml` 의 `[dependencies]` 에서 `[dev-dependencies]` 로
  내려간다 — 그러면 프로덕션 참조가 컴파일 단계에서 막히므로 이 가드의 텍스트 검사
  자체가 대부분 불필요해진다.
- 본체가 CLI 파서를 더 이상 소유하지 않게 된다(진입점 분리) — `ALLOWED_PATHS` 를
  포함해 가드 전체의 전제가 바뀐다.
- 면제 항목이 쓰는 픽스처(`src/state/tests.rs` · `src/adapters/test/`)가 `#[cfg(test)]`
  전용이 아니게 된다 — 그러면 lib 산출물에 들어가 대안 B 가 실현 가능해지므로 면제 항목을
  `tests/` 로 옮길 수 있는지 다시 본다. (원래 이 조건은 "`[lib]` 타깃이 생긴다" 였다.
  2026-09-20 에 실제로 발동했고, 위 Context 의 정정 절대로 좌변을 옮겼다.)
- `TEST_ONLY_FILES` 항목이 늘어 목록만으로 판정이 어려워진다 — 대안 C(범위 기반
  자동 판정)를 다시 검토한다.

## References

- `crates/tasty-doc-guards/tests/layering.rs` — 가드 본체(세 목록과 두 방향 검사).
- `crates/tasty-doc-guards/tests/layering_guard_premises_still_hold.rs` — 위 재검토 조건
  둘에 발화 자리를 주는 채널. 결정이 실현된 현재 위치다.
- [ADR-0325](0325-the-root-package-splits-into-a-lib-and-a-bin.md) — 셋째 조건을 발동시킨
  결정. 위 Context 의 2026-09-20 정정 절이 그 결과다.
- `crates/tasty-doc-guards/tests/no_todo_file_citation.rs` — `(경로, 사유)` 쌍 면제 목록의 선례.
- [ADR-0105](0105-no-nongit-path-refs-in-tracked-sources.md) — 추적 소스의 참조 규칙.
