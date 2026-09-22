# ADR-0138: 문서를 읽는 가드는 의존 0 크레이트에 산다 — 잡이 싸야 경로 필터를 뗄 수 있다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: ci, guards, docs, workspace, paths-ignore, build-cost, adr-0133
- **Group**: guard-design

## Context

`docs/` · `site/` · `*.md` 를 읽어 소스·워크플로 텍스트와 대조하는 통합 가드가 여럿 있다.
체크박스 금지, 비-git 경로 인용 금지, README 배지 정합, 크레이트 목록 완전성, 권한 토큰
표 정합 같은 것들이다. **이 가드들을 위반하는 방법은 문서를 고치는 것뿐이다.**

그런데 그 가드들의 유일한 자동 실행 채널은 `crossplatform-check.yml` 의 `check-headless`
잡이었고, 그 워크플로는 이렇게 시작한다.

    on:
      push:
        branches: [main]
        paths-ignore:
          - 'docs/**'
          - 'site/**'
          - '**/*.md'

GitHub 은 바뀐 파일이 **전부** 이 목록에 걸리면 워크플로를 통째로 건너뛴다. 즉 **문서만
바뀐 push 에서 정확히 꺼진다** — 그 가드들을 위반할 수 있는 유일한 형태의 push 에서.
최근 main 300 커밋 중 20% 가 그 형태였다.

이 구멍은 게으름이 아니라 **비용 논리의 산물**이다. 필터를 그냥 떼면 문서 한 줄 고칠
때마다 본체 컴파일이 붙는다. 헤드리스 조합조차 385 크레이트다(`cargo tree -e normal,build`
고유 크레이트 수, base `98d9b948`). "문서 push 마다 수 분짜리 빌드를 붙일 거냐" 는 반론이
성립하는 한, 필터를 떼자는 제안은 계속 기각된다. **그 반론을 무력화하지 않으면 같은
구멍이 다시 뚫린다.**

## Decision

**크레이트 코드를 한 줄도 안 쓰는 문서 가드를 의존 0 크레이트 `crates/tasty-doc-guards`
로 옮기고, 그 크레이트만 도는 워크플로를 경로 필터 없이 `ubuntu-latest` 에 붙인다.**

관례를 깬다 — 통합 테스트는 루트 `tests/` 에 두는 것이 이 레포의 기본형이다. 깨는 이유는
하나뿐이고 그것이 이 결정의 전부다: **`tests/` 에 있는 한 그 가드의 실행 비용은 본체
패키지의 컴파일 비용이고, 그 비용이 곧 경로 필터의 존재 이유다.** 위치를 바꾸면 비용이
바뀌고, 비용이 바뀌면 필터가 필요 없어진다.

측정으로 닫는다 — `cargo tree -e normal,build --prefix none | sort -u | wc -l`:

    tasty-doc-guards             =   1     <- 자기 자신뿐
    tasty --no-default-features  = 385     <- 같은 명령, 비영 대조

빌드 비용은 두 수로 갈린다 — **어느 쪽이 러너의 값인지 섞으면 안 된다.**

    빈 CARGO_TARGET_DIR, 레지스트리는 더운 상태            0.33s   (산출물 6.8M)
    빈 CARGO_TARGET_DIR + 빈 CARGO_HOME (콜드 러너)        7.53s   (CARGO_HOME 54M)

**러너의 값은 뒤쪽 7.53s 다.** 앞쪽은 로컬 반복 실행의 값이고, 그것을 러너 비용으로 적으면
과장이다. 의존이 0 인데도 콜드에서 7.5 초가 드는 이유는 이 크레이트가 **워크스페이스
멤버**라 cargo 가 빌드 전에 워크스페이스 전체 그래프를 해석하고, 그러려면 레지스트리
인덱스가 필요하기 때문이다 — 빈 `CARGO_HOME` + `CARGO_NET_OFFLINE=1` 에서
`failed to load source for dependency winit` 로 확인했다. 인덱스만 받고 크레이트는 하나도
안 받는다(컴파일된 것은 `tasty-doc-guards` 자신뿐).

그래도 "문서 push 마다 수 분짜리 빌드?" 라는 반론은 **7.5 초** 앞에서 성립하지 않는다.
**싸게 만든 것이 곧 필터를 뺄 명분이다.**

### 옮기는 것과 안 옮기는 것

옮긴 일곱은 `std` 외에 아무것도 안 쓴다.

    architecture_crate_list_complete · ci_channel_claims_match_workflows ·
    complexity_allowlist_docs_parity · no_checkbox_in_docs · no_todo_file_citation ·
    permission_token_docs_parity · readme_badge_parity

`changelog_unreleased` 는 문서를 읽지만 **안 옮긴다** — `test.yml` 의 push 트리거에는
경로 필터가 없고 그 잡(`semver-guards`)이 이 타깃을 이름으로 부른다. 이미 덮여 있다.

**이 판단은 이제 가드가 지킨다.** `filtered_guards_are_not_totally_blind.rs` 가 경로 필터
없는 워크플로의 `--test <이름>` 목록을 읽어 면제를 판정한다 — 사람이 기억할 필요가 없고,
`semver-guards` 가 그 이름을 떨어뜨리거나 `test.yml` 에 경로 필터가 생기거나 그 잡이
`workflow_dispatch` 전용이 되면 즉시 실패한다(세 방향 다 변이로 확인). 반대 방향의 함정도
같이 닫혔다: **이 가드를 옮기면 오히려 깨진다** — 타깃이 본체 패키지를 떠나면
`--test changelog_unreleased` 가 `no test target` 으로 실패한다. 덮는 채널을 손 명부로
들었다면 그 명부가 낡는 순간 "옮겨라" 라는 거짓 요구가 나왔을 자리다.

**남은 사각을 명시한다.** 다음 셋은 문서를 읽지만 크레이트 상수와 대조한다 —
`cli_method_table_parity` · `permission_free_methods_docs_parity`(둘 다 `tasty_ipc`,
86 크레이트) · `contributes_gate_docs_parity`(`tasty_plugin_manifest`, 56). 안 적으면
"문서 가드는 이제 다 싸다" 로 읽힌다.

**그 셋 중 둘이 하필 노출이 가장 심한 쪽이었다.** 가드 28 개를 경로 리터럴로 갈랐을 때
*입력이 전부 무시 대상* 인 것이 셋이었는데(`architecture_crate_list_complete` ·
`contributes_gate_docs_parity` · `permission_free_methods_docs_parity`), 이 ADR 시점에
옮긴 것은 첫째뿐이었다.

**정정 — "의존 0 이 안 된다" 는 전제가 틀렸다.** 여기서 셋을 링크가 필요한 것으로 묶고
다음 수단을 소유 크레이트 이동 + `-p` 잡(385 대신 86 / 56)으로 적었는데, **세 번째 길이
있었다**: 상수를 **텍스트로 읽고**, 판독이 진짜 표와 갈리는 위험은 본체 패키지의 교차 대조
가드가 받는다. 그 크레이트를 링크할 수 있는 자리는 그쪽이고, 판독기가 바뀌는 것은 소스
변경이라 `check-headless` 가 본다 — 채널이 갈리는 것이 오히려 맞다. 그리고 그 형태의
선례는 이 ADR 안에 이미 있었다: `permission_token_docs_parity` 가 `Permission::as_token`
을 그렇게 읽고 있었다.

그 길로 둘을 옮겼다 — `permission_free_methods_docs_parity`(`METHOD_TABLE` 판독,
교차 대조는 `tests/method_table_readings_agree.rs`) · `contributes_gate_docs_parity`
(`contributes_gates!` + `Permission::as_token` 판독, 교차 대조는
`tests/contributes_gate_readings_agree.rs`). 옮기기 전에 두 판독이 링크 열거와 같은지
먼저 쟀다(각각 276:276 · 게이트 12 항목 순서까지 일치). 판독기는 모르는 형태나 빈 결과를
만나면 **panic 한다** — 조용히 건너뛰면 소비자가 빈 쪽을 대조하며 통과한다.

**남은 하나는 `cli_method_table_parity` 다.** 같은 판독기가 닿지만(같은 파일의
`method_meta()` · `DEBUG_METHODS` 까지 필요하다) 804 줄이라 이동 비용이 다르고, 입력이
`docs/dev-guide/api-conventions.md` 라 *일부만 무시 대상* 쪽이어서 노출이 위 둘보다 덜하다.
소유 크레이트 이동 + `-p` 잡이라는 대안은 그 하나에 대해 여전히 유효하다.

### 루트 해석을 한 곳에 모으고, 틀리면 panic 한다

옮기면 `CARGO_MANIFEST_DIR` 이 레포 루트가 아니라 `crates/tasty-doc-guards` 가 된다.
일곱이 전부 그것을 루트로 쓰고 있었다. 각자 `../..` 를 붙이는 대신 `repo_root()` 하나로
모으고, 그 안에서 표지 파일 넷(`Cargo.toml` · `CHANGELOG.md` · `docs/adr/index.md` ·
`.github/workflows`)의 존재를 확인해 아니면 panic 한다.

이건 [ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) 이 다루는 실패의
정확한 형태다 — **스캔 가드에서 경로가 틀어지면 예외가 아니라 조용한 0 이 나오고, 0 인
모수는 언제나 초록이다.** 이동은 그 실패를 일곱 곳에 동시에 심을 수 있는 연산이므로,
확인 지점을 하나로 만들어 결정적으로 막는다. 표지를 `Cargo.toml` 하나로 줄이면 이 크레이트
자신의 디렉토리도 통과하므로, 그것을 대조로 보는 테스트를 함께 둔다.

## Consequences

- **얻은 것**: 문서만 바뀐 push 에서 일곱이 돈다. 지금까지 그 push 는 이 가드들이 유일하게
  위반될 수 있는 형태였는데도 검사 없이 통과했다.
- **얻은 것**: 경로 필터를 뺄 근거가 산문이 아니라 값이다(0.33s vs 385 크레이트). 다음에
  누가 "문서 push 에도 CI 를 돌리나" 를 물으면 답이 수로 있다.
- **잃은 것**: 발견성. 통합 가드를 찾는 사람은 루트 `tests/` 부터 본다. 일곱이 거기 없다.
  갚는 수단은 doc 한 문단이다 — `crates/tasty-doc-guards/src/lib.rs` 의 crate doc 과
  [architecture/index.md](../architecture/index.md) 의 크레이트 목록에 이유를 적었다.
- **잃은 것**: 가드가 두 곳에 산다. "문서를 읽나" 와 "크레이트 코드를 쓰나" 라는 두 축으로
  갈리는데, 후자가 바뀌면(가드가 상수를 쓰기 시작하면) 크레이트를 옮겨야 한다.
- **운영 비용**: 크레이트가 하나 늘어 `docs/architecture/index.md` 의 수와 목록,
  `CLAUDE.md` 의 같은 수를 갱신했다. `source_guards` 의 단위·파일 집합 판정은 `git ls-files`
  에서 유도하므로 갱신이 필요 없었다.
- **되돌리면 무엇이 깨지나**: 일곱을 `tests/` 로 되돌리면 잡이 다시 385 크레이트 컴파일이
  되고, 그러면 경로 필터를 유지할 이유가 되살아난다 — **필터가 부활하면 이 ADR 이 막은
  구멍이 그대로 돌아온다.** "관례에 맞춘다" 는 이유로 되돌리는 것은 그 구멍을 다시 여는
  것과 같다. 되돌리려면 먼저 문서 push 에 대한 다른 자동 채널을 확보해야 한다.

**구현 확정 보강 (2026-09-05).** 위 "되돌리면 무엇이 깨지나" 는 오래 **확인되지 않은
전제**였다. 변이 셋으로 재 봤다 — ① `doc-guards.yml` 의 `push:` 에 `paths:` 달기 ② 그 잡의
호출을 `--test <이름>` 하나로 좁히기 ③ 그 잡에 이벤트 조건을 달아 자동 회차에서 빼기.
셋 다 그때 있던 판정기 전부에서 살아남았다. 이 결정이 옮겨 놓은 가드들을 그 자리에서
면제하는 관측자조차 그 디렉토리를 상수로 건너뛰기만 했다 — 채널이 있다고 **가정**한
것이지 잰 것이 아니다.

`crates/tasty-doc-guards/tests/filter_free_channel_still_exists.rs` 가 그 셋을 닫는다.
그 타깃은 `doc-guards.yml` 과 `check-headless` 두 잡이 모두 돌린다(조합 정본은
[dev-guide/ci-gates.md](../dev-guide/ci-gates.md)). 판정은 이름이 아니라 성질이다:
"경로 필터 없이 매 push 도는 잡 중 이 패키지를 좁히지 않고 돌리는 것이 있는가". 그래서
워크플로를 옮기거나 이름을 바꾸는 것은 막지 않고, **채널을 잃는 것만** 막는다.

## Alternatives Considered

- **(H1) 아무것도 안 한다** — 구멍을 문서로만 적는다. 기각: 이 가드들이 잡으려는 위반이
  정확히 그 사각에서만 생긴다. 사각이 곧 전부다.
- **(H2a) 루트 패키지 그대로 두고 필터 없는 잡을 `ubuntu-latest` 에 추가** — **가능했다.**
  전제를 실측했다: 헤드리스 조합 385 크레이트에 GUI sys 크레이트 0(대조: 기본 조합에는
  `gtk` · `gtk-sys` · `gdkx11` · `gdkx11-sys` · `calloop-wayland-source` 가 있다),
  `cmake` · `bindgen` · `openssl` · `libssh2` 전부 0, `pkg-config` 1(`libgit2-sys` ·
  `libsqlite3-sys` 의 build-dep 이고 둘 다 번들 고정 — `rusqlite` bundled · `git2`
  default-features off · `mlua` vendored). 즉 필요한 시스템 의존은 C 컴파일러뿐이고
  `ubuntu-latest` 에 있다. **불가능해서 기각한 게 아니라 비싸서 기각했다** — 캐시가 맞으면
  수십 초, 캐시가 어긋나면 수 분이고, 그 비용이 살아 있는 한 "문서 push 마다?" 라는 반론도
  살아 있다. 반론을 남겨 두면 필터는 언젠가 돌아온다.
- **(H3) 경로 필터에서 `docs/**` 만 뺀다** — 기각: `check-headless` 전체가 문서 push 마다
  돌게 되고, 그건 (H2a)보다 비싸다.
- **이 크레이트를 워크스페이스에서 `exclude` 한다**(`site/` · `tasty-plugin-sdk-wasm` 의
  선례) — 자체 `Cargo.lock` 을 갖게 되어 콜드 비용이 7.53s 에서 인덱스 fetch 없는 값으로
  더 떨어진다. 기각: `cargo test --workspace` 가 그 일곱을 **안 보게 된다.** 작업 lane 의
  로컬 검증과 `check-headless` 가 둘 다 워크스페이스 단위라, 이중 채널을 잃고 새 잡 하나만
  남는다. 7.5 초를 더 줄이려고 채널을 하나로 줄이는 것은 이 ADR 의 목적(채널을 늘리는 것)과
  반대다. `site/` 가 exclude 된 이유는 비용이 아니라 **의존 누수**였고 여기엔 누수가 없다.
- **가드를 `--lib` 유닛으로 본체에 넣는다** — 기본 조합 잡(`--lib --bins`)이 자동으로 보게
  된다. 기각: 그 잡도 같은 `paths-ignore` 뒤에 있어 문서 push 에서 똑같이 꺼진다. 채널의
  종류가 아니라 **트리거**가 문제다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `crates/tasty-doc-guards/Cargo.toml` 의 `[dependencies]` 에 무엇이든 들어간다 — 의존 0 이
  이 결정의 유일한 근거이므로, 그 순간 전제가 사라진다. 필요한 의존이 생겼다면 그 가드는
  이 크레이트가 아니라 `tests/` 소속이다.
- 이 크레이트의 콜드 러너 시간이 분 단위로 올라간다 — 값이 7.5 초라서 필터가 필요 없는
  것이지 위치가 필터를 없애는 게 아니다. 재는 방법은 위 표와 같다(빈 `CARGO_HOME` + 빈
  `CARGO_TARGET_DIR`). 더운 레지스트리로 잰 값을 그 자리에 넣지 않는다.
- `crossplatform-check.yml` 의 `paths-ignore` 가 사라진다 — 그러면 문서 push 에도
  `check-headless` 가 돌아 이 크레이트의 존재 이유 절반이 없어진다(나머지 절반인 "싸다" 는
  남는다).
- 위 "남은 사각" 셋 중 하나가 크레이트 의존을 잃는다 — 그러면 옮길 수 있게 된다.

## References

- [ADR-0133](0133-guard-scan-population-is-pinned-not-enumerated.md) — 경로가 틀어지면 조용한
  0 이 되고 0 인 모수는 언제나 초록이다. `repo_root()` 의 표지 검사가 그 규칙의 적용이다.
- [dev-guide/ci-gates.md](../dev-guide/ci-gates.md) — 어느 가드가 어느 잡에서 도는지의 정본.
- [architecture/index.md](../architecture/index.md) — 워크스페이스 크레이트 목록.
- [ADR-0175](0175-window-owned-list-membership-is-judged-by-shape-not-by-name.md) — 대상 집합을
  이름 모양으로 뽑으면 성질이 같고 이름이 다른 것이 통째로 빠진다. 위 보강의 가드가 채널
  제공자를 파일 이름이 아니라 성질로 뽑는 것이 같은 규칙의 적용이다.
