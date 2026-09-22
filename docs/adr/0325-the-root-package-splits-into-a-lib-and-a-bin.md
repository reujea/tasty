# ADR-0325: 루트 패키지를 lib 와 bin 으로 가른다 — 경계를 만드는 것이 아니라 잴 좌변을 만드는 것이다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: architecture, cargo, targets, testing, public-api, headless
- **Group**: architecture

## Context

루트 패키지 `tasty` 에는 `[lib]` 절도 `src/lib.rs` 도 없었다. 바이너리 전용 패키지라
`src/main.rs` 가 모듈 트리(`mod` 선언 약 30 줄)와 재수출(`pub use` 약 25 줄)을 함께 들고
있었고, 밖에서 이 크레이트의 항목을 import 할 방법이 원리적으로 없었다.

그래서 두 가지가 막혀 있었다.

- **"GUI 없는 소비자가 도메인을 구성한다" 를 잴 수가 없다.** 막은 것은 `cfg` 얽힘이
  아니라 **소비할 타깃이 없다는 것**이다. 루트 `tests/*.rs` 가 전부 바이너리를 띄워
  밖에서 재는 형태인 것도 같은 이유다.
- **"무엇이 밖으로 나가는가" 라는 좌변 자체가 없다.** 바이너리 전용 패키지에서는 `pub`
  이 아무 데도 안 나가므로, 공개 표면을 좁히는 작업도 넓히는 작업도 잴 대상이 없다.

실측(2026-09-20). **모수를 이름으로 적는다** — `cargo build -p tasty --no-default-features`
뒤 `target/debug/libtasty.d` 가 드는 `src/*.rs` 의 중복 제거 수다(루트 `src/lib.rs` 를
포함하고, bin 쪽 `target/debug/tasty.d` 는 거기에 `src/main.rs` 하나가 더 붙어 264 다).
그 값은 **263** 이고, 그중 GUI 크레이트를 코드에서 참조하는 것은 `tasty-type-appearance` 4 ·
`tasty-font` 2 뿐이다 (`tasty-ui-widgets` 0 · `tasty-egui-theme` 0 · `tasty-icons` 0).
즉 이 갈림을 막고 있던 것은 코드가 아니었다.

## Decision

루트 `Cargo.toml` 에 `[lib] name = "tasty" path = "src/lib.rs"` 와
`[[bin]] name = "tasty" path = "src/main.rs"` 를 선언하고, 모듈 선언과 재수출을 `src/lib.rs`
로 옮긴다. `src/main.rs` 에는 `fn main()` 과 `dhat` 전역 할당자 선언, windows subsystem
attribute 만 남는다. `boot::run` 만 `pub` 으로 올린다.

**이것으로 경계가 생기는 것이 아니다.** 모듈은 전부 비공개이거나 `pub(crate)` 이고, 밖으로
나가는 것은 `boot::run` 과 이전부터 있던 재수출뿐이다. 생기는 것은 **좌변**이다 — 이제
"무엇이 공개되어 있는가" 를 `cargo doc -p tasty --no-deps` 로 그릴 수 있고, 무엇을 공개할지는
소비자가 실제로 생길 때 그 자리에서 정한다.

★ **그 좌변을 처음 재면 1 이 아니다.** `pub(crate)` → `pub` 으로 **승급한 항목**은
`boot::run` 하나지만, **실제로 나가는 것**은 그것 하나가 아니다. 바이너리 전용이던 시절의
`pub use tasty_font as font;` 류 재수출 넷은 글자 그대로 `pub` 이었는데 아무 데도 안 나갔고,
lib 타깃이 생긴 순간 **처음으로 표면이 된다.** 두 수를 같은 칸에 적지 마라 — "승급 1" 과
"표면 5" 는 다른 물음의 답이다. 실측(2026-09-20): `cargo doc -p tasty --no-deps` 의 루트
페이지는 모듈 둘(`boot` · `paths`)과 함수 열을 들고, 크레이트 통째 재수출 셋
(`font` · `settings` · `theme`)은 `--no-deps` 라 페이지가 안 생겨 그 목록에 안 뜬다 —
**그래서 그 명령의 출력을 표면 전부로 읽으면 적게 센다.**

**기존 시험은 한 줄도 바꾸지 않는다.** 루트 `tests/*.rs` 가 `use tasty::…` 를 *할 수 있게*
되지만, 바이너리를 띄워 재던 것과 라이브러리로 재는 것은 **다른 좌변**이라 이 결정이 그
이동을 요구하지 않는다.

## Consequences

- **얻은 것**: 공개 표면을 잴 자리. 그리고 라이브러리 소비자를 붙일 수 있는 타깃 — "GUI 없는
  라이브러리 소비자가 도메인을 구성한다" 를 이제 **원리적으로** 재볼 수 있다(아직 재지
  않았다).
- **잃은 것 — 단위시험을 좁히는 플래그의 이름이 바뀐다.** 루트 패키지의 단위시험은 전부 lib
  타깃으로 옮겨간다. 실측: 이전 `cargo test -p tasty --bin tasty` 가 1466 건, 이후 같은 조합이
  **0 건**이고 `--lib` 가 **1466 건**이다. 두 목록은 정렬 후 **완전 일치**(diff 0 줄)라 잃은
  시험은 없다. 그러나 **`--bin tasty` 로 좁힌 호출은 이제 0 건을 돌고 초록으로 끝난다** —
  조용한 미측정이다. 그래서 레포 안의 그 조합 인용을 함께 갈아끼운다(아래 References).
  자동 잡 쪽은 `--lib --bins` 를 함께 주므로 담는 것이 그대로다.
- **운영 비용 / 유지 부담**: 타깃이 하나 늘어 rustc 호출과 링크가 한 번씩 더 든다. **이
  머신에서는 그 차가 부하에 묻혀 분리되지 않았다** — 아래 재는 법 참조.

## Alternatives Considered

- **`[lib]` 없이 그대로 두고 통합 테스트가 바이너리를 계속 띄운다** — 지금 상태다. 이러면
  "무엇이 밖으로 나가는가" 를 묻는 모든 후속 작업이 좌변 없이 시작한다. 공개 표면을 좁히는
  일과 넓히는 일이 둘 다 관측 불가다.
- **`[lib]` 와 함께 `doctest = false` 를 건다** — 오늘 동작 보존을 위해 검토했으나 **필요가
  없었다.** 실측: `cargo test -p tasty --doc` 가 **0 건**을 수집한다(`src/` 의 코드펜스는
  비공개 항목의 주석 안에 있어 rustdoc 이 안 본다). 끄지 않은 쪽을 골랐다 — 끄면 나중에
  `pub` 이 늘어 doctest 가 의미를 갖게 될 때 그 사실이 조용히 사라진다.
- **모듈 트리를 `main.rs` 에 남기고 `lib.rs` 가 `#[path]` 로 같은 파일을 두 번 읽는다** —
  같은 코드를 두 크레이트로 컴파일하게 되어 빌드 비용이 진짜로 두 배가 되고, 두 사본이
  갈릴 자리가 생긴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 루트 크레이트의 `pub` 항목이 `boot::run` 과 기존 재수출 밖으로 늘어난다. 그 순간 이것은
  "잴 좌변" 이 아니라 **실제 공개 API** 이고, 무엇을 공개하고 무엇을 `pub(crate)` 로 남길지는
  이 ADR 이 정하지 않은 별도 결정이다. **지금 이 좌변을 보는 가드는 없다**(이 회차는 새 가드를
  만들지 않는 회차였다).
- 루트 `tests/*.rs` 중 하나가 `use tasty::` 로 라이브러리를 직접 재기 시작한다. 그것은 이
  결정이 연 것이지만 **다른 좌변으로 옮겨 재는 일**이라, 옮긴 쪽과 바이너리로 재던 쪽이
  같은 것을 보는지가 그때 물어야 할 물음이다. 채널 없음.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 두 타깃 빌드의 벽시계 비용이 체감될 만큼 벌어진다. 재는 법: `touch src/state.rs` 후
  `/usr/bin/time -f 'wall=%e user=%U sys=%S' cargo build --no-default-features` 를 이 결정의
  전후 상태에서 번갈아 여러 번 재고 **궤적**을 본다. 실측 2026-09-20(여섯 lane 이 같은 머신에서
  빌드 중): after `user=18.00 / 16.35 / 22.53`, before `user=23.53 / 23.26` — 표본이 부하와 함께
  단조 증가해 **A/B 차가 부하와 분리되지 않았다.** 한산한 머신에서 다시 재야 답이 나온다.

## References

- 이 결정이 실현된 현재 위치: 루트 `Cargo.toml` 의 `[lib]`·`[[bin]]`, `src/lib.rs`,
  `src/main.rs`, `src/boot.rs` 의 `run`.
- **쓸어야 할 인용의 좌변은 하나가 아니라 둘이다 — `--bin tasty` 와 `--bins`.** 루트
  패키지를 어느 쪽으로 좁혀도 단위시험이 0 건 돌고 `ok` 로 끝나므로, 한 형태만 세면 다른
  형태가 그대로 남는다. 세는 명령은 둘이다(`target/`·`.git/` 제외):
  `grep -rn -- '--bin tasty' .` 와 `grep -rn -- '--bins' . | grep -v -- '--lib'`.
  뒤엣것이 `--lib` 를 거르는 이유는 `--lib --bins` 를 **함께** 준 자리는 참이기 때문이다.
- 갈아끼운 조합 인용(`--bin tasty`/`--bins` → `--lib`): `.githooks/pre-commit` 의
  `suggest_target`, `scripts/survey-guard-move.sh`,
  [`../dev-guide/ci-gates.md`](../dev-guide/ci-gates.md),
  [`../dev-guide/self-verification.md`](../dev-guide/self-verification.md),
  [`../dev-guide/unit-test-isolation.md`](../dev-guide/unit-test-isolation.md),
  [`../dev-guide/guard-relocation.md`](../dev-guide/guard-relocation.md),
  [`../dev-guide/context-menu.md`](../dev-guide/context-menu.md),
  [`../design/systems/theme.md`](../design/systems/theme.md),
  [`0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md`](0305-request-pressure-is-a-process-gauge-not-a-per-caller-observation.md),
  `crates/tasty-platform/src/native_menu/linux.rs`, `src/source_guards/gallery_copied_dimensions.rs`,
  `src/source_guards/gallery_specimen_parity.rs`.
- **과거 실측이 든 모수 이름은 그대로 뒀다** — 그 수를 낸 조합의 이름이라 갈아끼우면
  측정과 모수가 갈린다([ADR-0139](0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md)).
  기준은 "날짜가 붙었는가" 가 아니라 **"그 자리가 과거 실측인가"** 다 — 대부분 날짜를 달고
  있지만 하나는 안 달고 있고, 날짜의 유무가 계보를 바꾸지는 않는다.
  해당 자리: `docs/dev-guide/ci-gates.md` 의 2026-09-06 실측,
  `docs/dev-guide/guard-population.md` 의 2026-09-08 실측 둘, `src/state/tests.rs`(2026-09-06),
  `src/app/window_access.rs`(2026-09-08), `src/source_guards/headless_app_layer_coverage.rs`
  의 2026-09-10 실측 넷(`--bins` 축), 그리고 **날짜가 없는**
  `src/source_guards/modifier_hint_paint_order.rs` 의 변이 실측.
- 빌드 가이드의 해당 절: [`../dev-guide/build.md`](../dev-guide/build.md) "워크스페이스 구조".
- **이 결정이 발동시킨 재검토 조건**: [ADR-0123](0123-layering-guard-excludes-cfg-test-modules.md)
  의 셋째 조건("`tasty` 에 `[lib]` 타깃이 생긴다")이 실제로 발화했다(채널:
  `crates/tasty-doc-guards/tests/layering_guard_premises_still_hold.rs`). 그 ADR 의 지시대로
  대안 B 가 실현 가능해졌는지 다시 봤고, **아니었다** — 면제 항목이 쓰는 픽스처가
  `#[cfg(test)]` 전용이라 lib 산출물 밖이다. 그 ADR 의 Context 에 정정 절을 더하고 조건의
  좌변을 그 사실로 옮겼다. 이 lane 이 `TEST_ONLY_FILES` 를 줄이지는 않았다.
