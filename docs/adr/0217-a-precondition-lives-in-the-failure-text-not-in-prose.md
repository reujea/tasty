# ADR-0217: 검증의 전제는 산문이 아니라 실패 문구에 산다

- **Status**: Accepted
- **Date**: 2026-09-08
- **Tags**: testing, e2e, diagnostics, failure-messages, preconditions, guards, measurement, adr-0139, adr-0142, adr-0206, adr-0211
- **Group**: testing

## Context

`tests/e2e_tests.rs` 의 두 건(`markdown_recent_is_read_only`,
`a_request_naming_an_unowned_target_is_rejected`)이 **두 회차 연속** 빨갰고 귀속이
미확정으로 남았다. 원인은 코드 회귀가 아니라 전제 위반이었다 — 루트 패키지만 도는
조합(`cargo test --test e2e_tests`)은 번들 plugin 바이너리를 짓지 않고, 그 상태에서
plugin namespace 호출은 `-32601 Method not found` 로 떨어진다. 그 문구는 **메서드가
사라진 것**과 글자 그대로 같다.

실측 2026-09-08: `target/debug` 의 plugin 바이너리가 0 개인 트리에서 44 통과 · 2 실패,
`cargo build -p tasty-plugin-markdown` **하나**로 둘 다 초록.
`cargo test --locked --test e2e_tests --no-run` 을 돌린 뒤에도 그 바이너리는 여전히
없다 — 조합이 색을 정하고, CI 의 `--workspace` 조합은 초록이라 두 회차 동안
자동 채널에서도 안 보였다.

**핵심은 아무도 몰랐던 것이 아니라는 점이다.** 이 전제는 세 곳에 **글로 이미 적혀
있었다**: `docs/dev-guide/e2e-tests.md` 의 "전제: plugin 바이너리 최신화 (필수)",
루트 `CLAUDE.md` 의 "`cargo build` 는 plugin 바이너리를 다시 만들지 않는다",
그리고 `tests/spawn_diag/mod.rs` 의 `instance_bin` doc 이 "함정 3" 으로 적은
문단 — 그 문단은 **증상이 `Method not found` 로 같다는 것까지** 적어 두었다.
글이 셋이고 판정이 0 이었다. 실패하는 순간에 읽히지 않는 글은 아무것도 안 막았고,
네 번째 사본을 더 쓰는 것은 같은 값을 하나 더 만드는 일일 뿐이었다.

## Decision

**검증 절차의 전제는 그 전제가 깨졌을 때 나오는 실패 문구가 이름으로 말해야 한다.**
문서·주석에 적는 것은 그 위에 얹는 것이지 대신하는 것이 아니다. 전제가 깨진 상태와
실제 결함이 **같은 문구**로 나오는 자리를 발견하면, 문서를 고치는 것이 아니라 그
자리에서 두 원인을 가르는 판정을 짓는다. 판정문은 무엇이 깨졌는지와 **무엇을 하면
되는지**를 함께 낸다.

판정은 **전제를 확인하는 자리가 아니라 실패하는 자리**에 붙인다. 부팅·초기화 단계에서
전제를 강제하면 그 전제와 무관한 시험들의 참인 초록까지 함께 사라져, 신호가 늘지 않고
줄어든다. 실측: 이 건에서 spawn 단계 강제는 44 개의 참인 초록을 빨강으로 바꿨을 것이고,
실패 자리에 붙인 진단은 빨간 2 건만 설명한다.

이 결정의 실현이 `tests/spawn_diag/mod.rs` 의 `staged_bundle_note` 다 — opt-in 한
스위트에서 실행 파일 옆의 plugin 바이너리를 세고, 0 이면 실패 문구 끝에 무엇을 지을지
붙인다.

## Consequences

- **얻은 것**: 같은 상태가 다시 나면 실패문 자체가 원인과 처방을 말한다. 두 회차를
  태운 오독 경로가 닫힌다. 문서는 판정을 **가리키는** 자리가 되어, 같은 사실의 사본이
  더 늘지 않는다.
- **잃은 것**: 판정은 좌변을 하나 더 가진다. 여기서는 실행 파일 이름의 접두사이고,
  그것이 바뀌면 판정이 조용히 아무것도 안 세게 된다 — 그래서 접두사가 실제로 가르는지를
  음성 대조로 함께 고정한다.
- **운영 비용 / 유지 부담**: 전제마다 판정을 하나씩 지어야 한다. 값이 있는 것은
  **두 원인이 같은 문구로 나오는 전제**뿐이고, 증상이 이미 구별되는 전제까지 이 형태로
  옮길 이유는 없다.

## Alternatives Considered

- **A: 문서를 더 눈에 띄게 고친다** — 이미 세 곳에 있었고 두 회차를 못 막았다. 네 번째
  사본은 값이 아니라 갈릴 자리를 하나 더 만드는 것이다.
- **B: 부팅 단계에서 전제를 강제한다** — 전제와 무관한 시험까지 같이 죽는다. 판정
  불가를 실패로 보내는 극성 자체는 옳지만(rc=2 는 통과가 아니다), 그 극성을 **모수
  전체**에 적용하면 신호가 준다.
- **C: 그 시험들을 `#[ignore]` 로 뺀다** — 전제가 깨진 상태와 진짜 회귀를 **둘 다**
  안 보게 된다. 조용한 통과를 만드는 방향이라 채택하지 않는다.
- **D: 판정이 전제를 스스로 충족시킨다(하네스가 바이너리를 짓는다)** — 시험 하네스가
  cargo 를 부르면 바깥 cargo 와 락을 다툰다. 같은 이유로 이 저장소의 게이트들은
  판정기를 **짓지 않고 없으면 말한다**.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- **`staged_bundle_note` 가 사라지면** 그것을 부르는 세 시험이 컴파일되지 않는다
  (`an_empty_target_dir_tells_an_opted_in_suite_what_to_build` ·
  `a_staged_plugin_binary_silences_the_note_but_a_lookalike_does_not` ·
  `a_suite_outside_the_roster_gets_no_build_prescription`). 이 결정의 실현이 지워지는
  것은 그 자리에서 빨개진다.
- **루트 패키지만 도는 조합이 번들 plugin 바이너리를 짓게 바뀌면** 이 건의 전제 자체가
  사라진다. 그때는 판정이 아니라 판정이 겨눈 상태가 없어진 것이므로, 지울지 남길지를 다시
  정한다. 이 조건의 주어는 사람 판단이 아니라 **빌드 선언**이다 — 루트 패키지의
  `Cargo.toml` 이 그 바이너리를 의존·멤버로 들이는가, 그리고
  `docs/dev-guide/ci-gates.md` 가 그 조합에 배정한 명령이 `--workspace` 를 갖는가.
  아직 안 지었다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- **이 형태의 진단이 오히려 실패문을 읽기 어렵게 만든다는 보고가 반복될 때.** 재는 법:
  진단이 붙은 실패가 처음 읽은 사람에게 원인까지 전달되는지를, 그 실패를 처음 만난 회차의
  보고에서 귀속이 한 번에 확정됐는가로 센다.

## References

- **이 원리가 적용된 두 축**(둘은 서로 형제이고 이 결정에 포함된다):
  [ADR-0206](0206-a-refusal-is-told-apart-by-its-own-words.md) 은 **종료 코드**를 부정한다 —
  계약이 셋뿐이라 거절 자리가 열여섯이어도 코드는 하나다.
  [ADR-0211](0211-the-pty-exit-budget-is-a-safety-net-not-a-race-budget.md) 은 **상한(예산)** 을
  부정한다 — 상한을 다 쓴 것은 경주에서 진 것과 죽은 것을 못 가른다.
  둘은 이름 붙은 레버를 하나씩 부정하므로 이 결정보다 좁고, 서로를 포함하지 않는다.
- 결정이 실현된 현재 위치: `tests/spawn_diag/mod.rs` 의 `staged_bundle_note` ·
  `bundle_staging_note` · `suite_calls_bundled_plugins`, 그것을 실패 문구에 잇는
  `tests/common/mod.rs` 와 `tests/e2e_tests.rs`.
- 전제의 산문 서술(이 ADR 이 대체하지 않고 가리키게 만든 것):
  [`docs/dev-guide/e2e-tests.md`](../dev-guide/e2e-tests.md) §0.
- 조합에 따라 자동 채널이 무엇을 보는지: [`docs/dev-guide/ci-gates.md`](../dev-guide/ci-gates.md).
- 값이 적히는 순간 낡는다는 같은 축의 결정:
  [ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md) ·
  [ADR-0142](0142-channel-claims-are-written-against-the-working-tree.md).
