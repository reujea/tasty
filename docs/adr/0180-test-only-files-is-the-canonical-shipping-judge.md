# ADR-0180: 소스 스캔 가드의 세 판정 물음에 이름과 집을 준다 — "출하되는가" 의 정본은 `shipping_scope` 다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: guards, shipping-scope, cfg-predicate, test-gate, canonical-judge, layering, cargo-layout, scan-target, adr-0129, adr-0165, adr-0166
- **Group**: guard-design

## Context

소스를 텍스트로 읽는 가드들은 몇 가지 **판정 물음**을 공유한다. 그 물음에 이름이 없어서,
필요한 가드마다 자기 자리에서 사본을 만들었다 — 하루에 **네 레인**이 같은 개념 언저리에
모였다: `test_gate.rs`(706, main), 이 lane 의 `shipping_scope` 확장 + 본 ADR,
폐기된 통합테스트 판정(728), temp_path uniquifier 중복(735/728). **넷이 모였다는 것은 그
개념이 코드에 정의돼 있지 않다는 뜻이다** — 그래서 본 ADR 의 값은 "합쳤다" 가 아니라 **이
물음들에 이름과 집을 준 것**이다.

혼동의 핵심은 **서로 다른 세 물음이 하나로 뭉쳐 보인 것**이었다. 실제로 이들은 다르고,
그 사실은 **호출부를 세는 것만으로** 드러난다: 706 의 한 커밋(`0f7ef11c5`)이
`cfg_predicate::implies` · `shipping_scope::test_only_files` · `test_gate::blank_test_modules`
를 **셋 다** 부른다. **한 소비자가 둘을 다 부르면 그 둘은 다른 물음이다** — 같은 물음이면
아무도 둘 다 필요하지 않다. 그래서 706 은 `shipping_scope` 의 **경쟁자가 아니라 첫 소비자**다.

## Decision

### 세 물음을 이름과 정의로 박는다

- **`cfg_predicate`** — *"이 줄/항목이 어떤 cfg 극성 아래 있는가"* (줄 단위). `implies(pred,
  needle)` 는 술어가 needle 을 함의하는지, `cfg_gated_lines(lines, needle)` 는 그 항목이
  덮는 줄들을 판정한다. 중첩 `all`/`any`/`not`·다줄 속성을 정확히 다룬다.
- **`test_gate`**(main 크레이트 `src/source_guards/`) — *"인라인 테스트 모듈의 형태"*:
  `#[cfg(test)]` 로 게이트된 블록을 줄 구조 보존한 채 지우고(`blank_test_modules`),
  게이트된 자식 `mod NAME;` 의 이름/파일을 센다. `length_constant_frontier` 가 소비한다.
- **`shipping_scope`**(`crates/tasty-doc-guards/`) — *"이 파일이 출하 산출물에 들어가는가"*
  (파일 단위): `test_only_files`(선언 전이 폐쇄) + `is_cargo_test_target`(cargo 통합테스트
  타깃). env·poison·SLOC 게이트·CLI 대조·본체 SLOC 대리인이 소비한다.

"정본" 이라는 낱말은 **`shipping_scope` 에 대해서만** 선언한다 — 세 물음 전체에 걸치면 그
선언이 거짓이 된다. `cfg_predicate`·`test_gate` 는 각자의 물음으로 선다.

### "출하되는가" 의 정본은 `shipping_scope::test_only_files` 다

이 물음을 묻는 자리는 이 판정기를 부르고 스캔 범위(모수)만 각자 넘긴다 — 답을 자기
자리에서 다시 구현하지 않는다. cargo 통합테스트 타깃(패키지 루트 `tests/`,
[`is_cargo_test_target`])은 레이아웃이 강제로 출하 밖이라 선언 없이도 test-only 이고,
그것을 이 판정기에 편입한다.

**정본의 집은 의존 방향이 강제한다.** env·poison·temp_path 는 `tasty-doc-guards` 에 살고,
그 크레이트는 main 에 의존할 수 없다. "출하되는가" 를 모든 소비자가 부르려면 정본은
**반드시 `tasty-doc-guards`** 에 있어야 한다. 이것은 "내 것을 고른다" 가 아니라 의존
방향이 정하는 답이다 — main 의 `test_gate`·`length_constant_frontier` 는 이 정본에 닿을 수
있으므로(main → doc-guards), 그쪽이 위임한다.

### 사본이 정당한 자리와 그 자리가 적어야 할 것

같은 물음이어도 **두 자리가 의존을 만들 수 없으면** 사본이 정당하다 — 층 경계다. 실측
사례는 **셸 게이트 ↔ 러스트 가드**뿐이다(`scripts/check-*.sh` 는 러스트 크레이트를
링크하지 못해 `mask-source`·`strip-cfg-test` **바이너리를 호출**한다). 반대로
`src/source_guards`(main) ↔ `crates/tasty-doc-guards` 는 **의존 가능**하므로(main →
doc-guards) 그 사이의 사본은 정당하지 않다 — 위임한다.

정당한 사본이 있는 자리는 셋을 적는다: **① 사본이라는 사실 ② 동조 조건(무엇이 바뀌면
같이 바뀌어야 하는가) ③ 정본으로 가는 상호 참조.** 이 셋이 없는 사본이 결함이다 —
사본 자체가 아니라.

### 사본인지 가르는 판별자 둘 — 그리고 싼 쪽은 반쪽이다

- **데이터 판별자 (완전하되 비쌈)**: 두 구현에 **같은 파일 집합**을 먹여 판정을 diff
  한다. 차이 0 이면 같은 답, 차이가 있으면 그 차이가 두 물음의 정의(또는 한쪽의 구멍)다.
  **같음도 다름도** 판정한다.
- **사용 판별자 (싸되 반쪽)**: **한 소비자가 둘을 다 부르면 그 둘은 다른 물음이다.**
  호출부만 세면 되므로 데이터 판별자보다 싸다 — 다섯 번째 사본을 만들기 전에 이것부터 센다.
  그러나 **"다름" 한 방향으로만 성립한다.** "아무도 둘 다 안 부른다" 는 "같은 물음이다"
  의 증거가 **되지 못한다** — 만날 일이 없어서일 수 있다. (반례: `rust_sources` 두 벌은
  다른 크레이트라 한 소비자가 둘 다 부를 수가 없지만 그래도 같은 물음이고 중복이다.)
- **싼 판별자를 같음의 증거로 쓰면 틀린다.** 사용 판별자가 침묵하면(아무도 둘 다 안 부름)
  판정은 아직 안 났다 — 같음/중복은 **데이터 판별자** 또는 **직접 증거**(정본이 이미 그
  답을 갖고 있음)로만 선다. 이 순서를 안 적으면 다음 사람이 사용 판별자의 침묵을 "같지 않다"
  나 "중복 아니다" 로 오독한다.

### 이 형태는 소스 스캔 가드 일반에 적용된다

위 규칙(물음에 이름 → 같은 물음은 위임·다른 물음은 이름 분리 → 정본은 공통 핵심만,
정당한 차이는 모수)은 이 세 물음에 국한되지 않는다. **레포 자신의 파일을 읽고 분류를
돌려주는 함수** 전반에 적용된다. 실측(728, 2026-09-06): 그런 함수 332 개 중 이름이 겹치는
것이 28 벌, 그중 `is_scan_target` 만 다섯 벌인데 넷은 서로 **다른 물음**을 같은 이름으로
숨기고 있었다. 이 형태로 전수 집행한다.

실증으로 둘을 실제로 고쳤다(읽지 않고 돌려서 잰다):

- **같은 물음 → 위임**: `cited_coordinates_exist` 와 `no_todo_file_citation` 의 바이너리
  denylist 를 lib 정본 [`is_binary_artifact_ext`] 로 올렸다. 위임 인프라가 이미 있어
  (두 가드가 `is_build_cache_dir`·`repo_root` 를 이미 위임받음) 배선 비용 0, 데이터 판별자 그린
  (모집단 불변). 자인된 사본(전자가 후자를 베낌)이 사라졌다.
- **다른 물음 → 이름 분리**: `no_checkbox_in_docs` 의 `is_scan_target`(=`docs/*.md`)을
  `is_checkbox_doc` 으로 바꿨다. 위임할 정본이 없다(물음이 다르다) — 이름이 물음을 말하게
  해 grep 에서 갈리게 한다.

두 경계를 함께 박는다 — 없으면 다음 사람이 안 맞는 데까지 끌고 간다:

- **경계 1 — "같은 이름" 은 판별자가 아니다.** `is_scan_target` 다섯은 이름이 같아도
  물음이 다르다(`docs/*.md` · `.rs`+매니페스트 · 바이너리 아닌 텍스트 · 출하 `.rs`).
  이름이 물음을 숨기므로 통합 전에 데이터 판별자나 본문으로 물음을 먼저 가른다. 같은 이름
  다른 물음은 위 세 물음(`cfg_predicate`·`test_gate`·`shipping_scope`)의 재현이다 —
  실측 4·5·6·7 번째 사례.
- **경계 2 — 같은 물음이어도 정본은 공통 핵심만 담는다.** `cited_coordinates_exist` 와
  `no_todo_file_citation` 은 "바이너리 아닌 텍스트 파일인가" 로 같은 물음이되 정당하게
  다른 경계를 갖는다(전자는 `.svg`·`.lock` 을 더 빼고, 후자는 vendored 를 뺀다). 그
  차이를 정본에 억지로 합치면 한쪽 행동이 바뀐다(데이터 판별자의 diff 가 그 신호다). 정본은 공통
  판정만 갖고 가드별 차이는 **모수**로 소비자에 남긴다 — "출하되는가" 정본이 "판정은
  하나, 스캔 범위는 각자" 로 한 것과 동형이다.

## Consequences

- **얻은 것**: 세 물음에 이름이 생겨, 다음 사람이 사본을 만들기 전에 어느 물음인지 묻고
  호출부를 센다. "출하되는가" 는 한 자리(`shipping_scope`)에서만 답이 나오고, 그 답이
  통합테스트까지 포함해 완전하다.
- **잃은 것**: `shipping_scope` 가 파일마다 `Cargo.toml` 조상을 찾는 파일시스템 왕복을
  한다(기존 `declaration_edges` 와 같은 층).
- **운영 비용**: 새 소비자는 정본을 부르고 모수만 넘긴다. 층 경계로 사본이 불가피하면 위
  세 줄(사실·동조·상호참조)을 단다.

### 경쟁 구현 전수

"정본" 을 선언하는 문서가 경쟁자를 안 세면 그 선언은 검증되지 않은 전제다. "출하되는가"
물음의 실측 구현들:

- **`shipping_scope::test_only_files`**(`tasty-doc-guards`) — 정본. 전이 폐쇄 +
  통합테스트 타깃.
- **`test_gate::test_gated_files`**(main, 706) — 파일 단위 테스트 게이트 판정을 담지만
  `length_constant_frontier` 는 이 모듈의 **줄 단위**(`blank_test_modules`)만 쓴다. 파일
  단위 물음은 이미 `shipping_scope` 에 위임한다(706 이 첫 소비자).
- **`length_constant_frontier` 옛 사본** — `blank_test_modules`/`test_gated_modules` 를
  자체 보유했다. 706 이 `test_gate` 로, 이 lane 이 `cfg_predicate` 로 각각 위임화했다.
- **728 폐기본** — 통합테스트 판정을 따로 만들었다가 이 정본과 중복이라 폐기.
- **`sloc_gate_skip_proxy`** — 이미 `shipping_scope` 에 위임(사본 아님).

### 706 대조 (데이터 판별자로 확인)

같은 파일 집합(`rust_sources(src, crates)`)에 두 파일-단위 판정기를 먹인 실측:
`test_gate::test_gated_files`=29 · `shipping_scope::test_only_files`=120 ·
한쪽만=91 · **반대쪽만=0**. 차이가 있고(두 답이 같지 않다), 한 방향이다 —
`shipping_scope` 가 통합테스트 타깃까지 더 잡는다. 사용 판별자(706 이 둘 다 호출)와 합쳐,
`test_gate` 의 파일 단위 물음은 `shipping_scope` 로 위임하는 것이 맞다.

### 판별자가 두 방향으로 답을 낸 두 사건 — 층이 사본을 정당화한다

이 물음군에서 사용 판별자가 실제로 **두 방향으로** 쓰였고, 그 대비가 "정당한 사본" 과 "진짜
중복" 을 가른다:

- **두 물음(사용 판별자 유효 방향)** — `test_gate` ↔ `shipping_scope`. 706 의 한 커밋이 둘 다
  부른다 → 다른 물음이다(위 706 대조). 둘은 경쟁이 아니라 층이다.
- **같은 층의 진짜 중복(사용 판별자 침묵 → 직접 증거)** — `length_constant_frontier` 의
  `blank_test_modules` 지역 사본(735, 폐기) vs `test_gate::blank_test_modules`(706, 공용).
  **아무도 둘 다 안 부른다 — 사용 판별자는 여기서 아무 말도 못 한다.** 판정 근거는 호출부가
  아니라 **직접 증거**다: 706 이 같은 개선(복합 cfg 를 함의로 판정)을 하면서 판정을
  `test_gate` 로 `pub(super)` 공용화했고 복합 teeth 까지 갖췄다 — 넣으려던 것이 이미 거기
  있다. 데이터 판별자로도 확인(diff=0: 모수에서 두 판이 갈릴 자리 넷 — `not(test)` 2·
  `any(debug_assertions, test)` 2 — 이 모두 길이 상수를 안 담는다). → 735 사본 폐기
  (`9994b8a57` revert), 잃는 것 0. **두 자리가 같은 크레이트라 의존을 만들 수 있으므로
  위임이 답이고, 사본은 정당하지 않다.**
- **다른 층의 정당한 사본** — temp_path uniquifier(728). 두 자리가 다른 크레이트라 의존을
  만들 수 없다 → 층 경계 사본. 위 "사본이 정당한 자리" 세 줄(사실·동조·상호참조)을 단다.

**한 문장으로**: 사용 판별자는 "둘 다 부른다" 로만 답하고 침묵은 답이 아니다. 침묵한 자리에서
같음/중복은 데이터 판별자나 직접 증거로 가리고, **그다음 층(의존 가능성)이 사본의 정당성을
가른다** — 같은 층이면 위임, 다른 층이면 정당한 사본.

### 한 판정기의 한 오답이 소비자마다 반대 방향으로 틀린다 (728)

정본이 하나여야 하는 가장 강한 논증이다: "출하되는가" 의 한 오답이 **소비자마다 반대
방향으로** 나타난다 — env 축은 "테스트가 아니다" 라며 건너뛰고(거짓 음성, 조용함),
poison 축은 "출하 코드다" 라며 잡는다(거짓 양성, 시끄러움). 사본이 갈리면 두 축이 서로
다른 진실을 믿는다.

### 모수가 아니라 성질 판정이 구멍이었다 (728 — 이 처방의 한계)

통합테스트 45 파일은 **이미 모수 안에 있었다**(env 는 `crates/*/tests/` 를 스캔하고
있었다). 구멍은 모수가 아니라 **성질 판정**(`test_only_files` 가 그 파일을 test-only 로
분류 못 함)이었다. 그래서 "스캔 루트만 넓히는" 처방이었으면 못 봤다 — 성질 판정을
고쳐야 했다. 루트 `tests/` 편입은 그 위에 별도로 필요한 확장이다.

## Alternatives Considered

- **A — 사본 유지(각 가드가 자기 판정)**: Context 의 네 레인이 모여든 그 상태다. 사각이 실제로 났고
  (통합테스트 조용히 놓침), 사본은 갈려서 반대 방향으로 조용히 틀린다.
- **B — 형태별 하드코딩 목록**(테스트·벤치 디렉토리를 이름으로 나열): 형태를 세므로 새
  형태가 생기면 샌다. 성질("cargo 가 별도 타깃으로 빌드하는가")로 판정하면 자동으로 들어온다.
- **C — 세 물음을 한 "정본" 으로 묶기**: `cfg_predicate`·`test_gate`·`shipping_scope` 를
  한 낱말로 덮으면 706 처럼 한 소비자가 둘을 다 부르는 순간 그 선언이 거짓이 된다.
  세 물음은 세 이름으로 둔다.

## Reconsideration Triggers

- cargo 타깃 레이아웃이 바뀌어 "패키지 루트 `tests/` 는 출하 안 됨" 전제가 깨질 때.
- 어떤 소비자가 "출하되는가" 를 다른 뜻으로 쓰기 시작할 때(예: example 바이너리를 배포
  산출물로 셈) — 그러면 물음이 갈리므로 정본을 나누거나 각자 물음을 적는다.
- `src/source_guards` 와 `tasty-doc-guards` 사이에 의존을 만들 수 없게 되는 변경이 생겨,
  지금 "위임" 인 자리가 "정당한 사본" 으로 바뀔 때.

## References

- 정본: `crates/tasty-doc-guards/src/shipping_scope.rs`(`test_only_files`·
  `is_cargo_test_target`) · `crates/tasty-doc-guards/src/cfg_predicate.rs` ·
  `crates/tasty-doc-guards/src/lib.rs`(`is_binary_artifact_ext` — 스캔 대상 파일 판정)
- 위임/소비: main 크레이트 `src/source_guards/` 의 `test_gate` 모듈(706, `7919c9a89` —
  이 lane 에는 아직 없고 train70 병합으로 들어온다) ·
  `src/source_guards/length_constant_frontier.rs` · `src/source_guards/sloc_gate_skip_proxy.rs`
  · `src/source_guards/headless_app_layer_coverage.rs`(명부 밖 잔여의 좌변을
  `cfg_predicate::blank_gated_lines` 로 좁힌다 — `strip-cfg-test` 와 같은 판정)
  · `crates/tasty-doc-guards/tests/agent_facing_reads_of_active_state_are_classified.rs`
  (`shipped_code` — 마스킹한 사본을 `blank_gated_lines` 에 넣는다. 한때 그 함수를 **글자
  그대로 베낀 루프**였고, 그 사본을 잡는 채널이 `git grep` 밖에 없었다. 지금은
  `cfg_predicate` 의 `one_span_judge` 가 두 스팬 판정기를 함께 부르는 자리를 잡는다)
  · `crates/tasty-doc-guards/src/env_isolation.rs` ·
  `crates/tasty-doc-guards/src/poison_recovery.rs` · `tests/cli_method_table_parity.rs`
- 층 경계 사본 선례(바이너리 위임): `scripts/check-*.sh` ↔ `crates/tasty-doc-guards/src/bin/`
  (`mask-source`·`strip-cfg-test`)
- 관련: [ADR-0165](0165-the-file-sloc-gate-measures-shipped-lines.md) ·
  [ADR-0166](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md)(shipping-scope
  형태) · [ADR-0129](0129-flaky-test-classes-and-standard-fixes.md)
