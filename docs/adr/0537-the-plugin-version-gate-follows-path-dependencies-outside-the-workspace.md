# ADR-0537: plugin 버전 게이트는 워크스페이스 밖 path 의존까지 판정한다 — ADR-0166 의 폐포 범위 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: plugin, versioning, guards, ci-gates, vendoring, dependency, adr-0166, adr-0516
- **Group**: build-release

## Context

[ADR-0166](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md) 은 plugin 버전 게이트
(`scripts/check-plugin-version-bump.sh`)의 판정 대상을 plugin 디렉토리에서 **워크스페이스 내부 의존
폐포**로 넓혔다. 그 폐포는 `crates/` 아래 크레이트만 좌변으로 삼았다 — 당시 plugin 이 링크하는 path
의존은 전부 워크스페이스 멤버였고, 멤버는 전부 `crates/` 아래에 있었다.

[ADR-0516](0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md) 이 그 전제를
깼다. 상류 `tiny_http` 사본을 `vendor/tiny_http/` 에 두고 워크스페이스 `exclude` 에 넣은 뒤
`[patch.crates-io]` 로 끼웠다. `cargo tree --workspace -i tiny_http -e normal` 로 보면 그 사본을 링크하는
것은 본체 `tasty` 와 번들 plugin `tasty-plugin-agent-stream` 둘이다. 즉 사본을 고치면 agent-stream 의
산출물이 달라지는데, 게이트는 `crates/` 밖을 안 봤다.

잰 값(2026-09-23, 이 결정 이전의 게이트 = 커밋 `424713258` 의 스크립트):

- ADR-0516 을 착지시킨 범위 `424713258..89bf95ffc`(사본 신설 + agent-stream 0.1.42→0.1.43)에서
  `판정 대상 0 건 (변경된 crates 파일 4 개 중)`, rc 0. 그 범위의 bump 는 손으로 한 것이고 게이트는
  그것을 보지 않았다.
- `vendor/tiny_http/src/common.rs` 의 출하 코드 한 줄(상태 문구 하나)만 staged 한 상태에서
  `--staged --base HEAD` 가 `판정 대상 0 건 (변경된 crates 파일 0 개 중)`, rc 0.

pre-commit `P.1` 은 `^crates/` 선필터를 두었고 `plugin-version-check.yml` 은 경로 필터가 `crates/**`
였다. 그래서 사본만 고친 커밋은 **세 채널(P.1 · B.9 · CI) 모두 초록**이었다 — 둘은 스크립트를 부르지도
않았고, 부르는 하나(B.9)는 그 경로를 좌변에 안 넣었다. plugin 버전 정책이 가장 피하려는 상태(같은 버전
문자열 아래 서로 다른 두 산출물)가 조용히 만들어지는 자리다.

## Decision

**좌변에 plugin 폐포에 실제로 든 워크스페이스 밖 path 의존을 더한다.** 이것은 ADR-0166 Decision 의
"넓히는 쪽" 한 조항만 개정한다.

- **어떤 디렉토리인가**: 번들 plugin 마다 `cargo tree -p <plugin> -e normal,build --prefix none` 을
  돌려(ADR-0166 이 이미 쓰는 호출이다) path 의존 줄의 `(/절대/경로)` 를 읽고, 그중 저장소 안이면서
  `crates/` 밖이고 워크스페이스 멤버가 **아닌** 디렉토리를 고른다. 멤버 명부는
  `cargo metadata --no-deps` 의 `manifest_path` 다. 오늘 그 집합은 `vendor/tiny_http` 하나다. 경로를
  목록으로 적지 않고 cargo 폐포에서 구하므로, 사본이 새로 생겨도 이 자리를 고칠 필요가 없다.
- **언제 구하는가**: `crates/` 밖에 산출물 경로 모양(`*/src/*` · `*/Cargo.toml` · `*/build.rs` 등,
  게이트의 `build_affecting`)의 변경이 **있을 때만**. 없으면 cargo 를 안 부른다. 멤버 명부는 폐포에
  `crates/` 밖 경로가 처음 나올 때 한 번 읽는다.
- **어떻게 판정하는가**: `crates/` 아래 공유 크레이트와 같다. 그 사본을 링크하는 plugin 이 판정 대상이
  되고, 비교는 출하 판정기(`strip-cfg-test --blank-test-only-files`)를 거친 뒤 rustfmt 정규화 내용으로
  한다. 판정기에는 그 사본의 뿌리를 스캔 뿌리로 함께 넘긴다.
- **판정 불가**: 멤버 명부를 못 읽으면(`cargo metadata` 실패) 통과가 아니라 rc 2 이고, cargo 의 stderr 를
  사유로 찍는다.
- **채널 둘의 경로 필터를 걷는다**: pre-commit `P.1` 은 선필터 없이 staged 가 있으면 부르고, 거르는 일은
  스크립트가 한다. `plugin-version-check.yml` 은 `paths: crates/**` 대신 문서만 빼는 필터를 쓴다. 경로를 목록으로
  다시 적으면 새 사본이 생길 때 그 자리만 옛 목록에 남기 때문이다. B.9 는 원래 필터가 없다.
- **CI 필터는 판정 대상의 상위집합으로 둔다**: 스크립트의 `build_affecting` 은 `src/`·`lang/`·`assets/`
  아래를 확장자와 무관하게 산출물로 보므로 `crates/tasty-plugin-markdown/assets/NOTICE.md` 가 판정 대상이다.
  `paths-ignore: '**/*.md'` 는 그 파일만 담은 push 에서 잡을 안 켠다. 그래서 `paths` 에 부정 패턴을
  `'**'` · `'!**/*.md'` · `'**/src/**'` · `'**/lang/**'` · `'**/assets/**'` · `'!docs/**'` · `'!site/**'`
  순서로 쓴다. GitHub Actions 는 뒤에 오는 패턴이 이기므로 순서가 의미를 정하고, 그 사실을 워크플로 주석이
  그 자리에 적는다. 세 디렉토리 모양은 `build_affecting` 의 복제이고 두 자리가 서로를 가리킨다.
- **보고 줄**: 통과 줄의 "변경된 crates 파일" 수는 `crates/` 아래만 세고, 워크스페이스 밖 path 의존의
  파일이 있으면 `· 워크스페이스 밖 path 의존 파일 N 개` 를 따로 붙인다. 빈 모수를 훑은 초록과 사본을
  **보고** 난 초록이 같은 줄로 안 보이게 하려는 것이다.

**개정하지 않는 것**: ADR-0166 의 좁히는 쪽(출하 내용만 센다 — 세 형태), dev-의존 제외, 공유 크레이트를
이름 정확 일치로 묻는 `crates/` 쪽 판정, 판정기 부재 시 넓게 보는 방향, 그리고 ADR-0137 의 판별식
(rustfmt 정규화 후 내용 · toml 의 version 줄 제외). 모두 그대로다.

### 잰 것 (2026-09-23)

- 같은 범위 `424713258..89bf95ffc` 에서 새 게이트: `판정 대상 1 건 (변경된 crates 파일 4 개 · 워크스페이스
  밖 path 의존 파일 19 개 중)`, rc 0 — agent-stream 이 판정 대상이 되고, 손으로 한 bump 가 그 판정을
  통과시킨다. 벽시계 9.7 s(변경 없는 범위는 0.1 s).
- 변이: `vendor/tiny_http/src/common.rs` 의 출하 코드 한 줄만 staged → `crates/tasty-plugin-agent-stream:
  산출물이 달라지는데 version 이 안 올랐다 (0.1.43 → 0.1.43)`, 바뀐 파일로 그 한 줄을 지목, rc 1. 같은
  staged 상태에서 이전 게이트는 rc 0 · 대상 0. 원복(cp -p 백업 + touch) 뒤 rc 0 · 대상 0.
- 변이: 같은 파일의 `#[cfg(test)] mod test` 안 한 줄만 staged → rc 0, `판정 대상 0 건 (… 워크스페이스
  밖 path 의존 파일 1 개 중)`. 출하 판정기가 사본에도 적용된다. 이 갈래는 합성 저장소 시험으로 못
  잰다 — 합성 저장소에는 판정기 바이너리가 없다.
- 시험 `tests/plugin_version_bump_channel.rs` 에 셋을 더했다: 링크된 워크스페이스 밖 사본의 변경은 두
  모드 모두 rc 1, 아무도 링크하지 않는 사본은 rc 0, 멤버 명부를 못 읽으면 rc 2 와 사유 문구. 게이트에서
  좌변을 채우는 한 줄을 지우는 변이는 첫 시험이 죽였다(27 ok / 1 FAILED).

## Consequences

- **얻은 것**: `vendor/tiny_http/src` 만 고친 커밋이 세 채널에서 agent-stream bump 를 요구한다. 다음 사본도
  폐포에 들기만 하면 같은 판정을 받는다 — 목록을 고칠 자리가 없다.
- **잃은 것**: `crates/` 밖 산출물 경로 모양의 변경(예: `site/src/**`)이 있는 커밋은 plugin 수만큼 `cargo tree`
  를 더 부른다(오늘 plugin 하나당 ~0.15 s). 그런 커밋의 P.1 과 CI 가 그만큼 느려진다. CI 잡은 문서만 담은
  push 가 아니면 매번 돈다 — 전에는 `crates/**` 가 안 바뀐 push 에서 안 돌았다.
- **운영 비용 / 유지 부담**: 사본의 출하 코드를 고치는 사람은 그것을 링크하는 번들 plugin 의 patch 를 올려야
  한다. 오늘은 agent-stream 하나다. 사본의 테스트 전용 변경·`PATCHES.md` 같은 문서 변경은 요구하지 않는다.
- **남는 경계**: 루트 `Cargo.toml` 의 `[patch]` 줄을 바꾸거나 `Cargo.lock` 으로 외부 의존 버전이 바뀌는 것은
  여전히 판정 밖이다. 좌변이 "저장소 안 path 의존의 파일" 이지 "의존 그래프" 가 아니기 때문이고, 그 경계는
  ADR-0166 이전부터 있었다. 이 결정은 그것을 안 바꾼다.

## Alternatives Considered

- **ADR-0516 에 "사본을 고치면 agent-stream 을 손으로 올려라" 만 적는다**: 규칙이 사람 기억에 남고, 세 채널은
  계속 초록이다. 이 레포의 plugin 버전 규율은 조용한 통과를 최악으로 둔다 — 기각.
- **`SCAN_ROOT` 를 여럿으로 넓힌다(`crates` + `vendor`)**: 게이트는 `SCAN_ROOT` 아래 디렉토리 이름을 크레이트
  이름으로 읽고 매니페스트 명부도 거기서 찾는다. 사본은 plugin 이 아니고 디렉토리 이름이 크레이트 이름과
  같다는 보장도 없다. 그리고 `vendor/` 를 이름으로 적으면 다른 자리의 사본은 다시 안 보인다 — 기각.
- **루트 `Cargo.toml` 의 `[patch]` path 를 sed 로 읽는다**: `[patch]` 가 아닌 일반 path 의존으로 끼운 사본을
  놓치고, toml 을 정규식으로 읽는 두 번째 해석기가 생긴다. cargo 폐포가 이미 답을 가지고 있다 — 기각.
- **`cargo metadata` 전체(의존 포함)로 좌변을 구한다**: 가능하지만 JSON 의 의존 그래프를 셸에서 걸어야 한다.
  게이트가 이미 부르는 `cargo tree` 의 출력에 경로가 있어 그것을 쓰고, `cargo metadata` 는 멤버 명부
  (`--no-deps`)에만 쓴다.
- **CI 필터를 `paths-ignore`(`docs/**` · `site/**` · `**/*.md`)로 두고, `assets/` 의 `.md` 는 문서에 한 줄로
  덮는다**: 실측으로는 참이다 — 추적 파일 중 `crates/*/(src|lang|assets)/` 와 `vendor/*/` 아래 `.md` 는
  `NOTICE.md` 하나이고, `.md` 를 `include_str!`/`include_bytes!` 로 싣는 자리는 0 이다. 그러나 "필터가 판정
  대상의 상위집합" 은 이 게이트 체계가 조용히 통과하지 않는다는 보증이다. 그 0 은 미래에 바뀔 수 있고, 바뀌면
  같은 파일을 P.1 · B.9 는 bump 대상으로 보는데 CI 는 잡을 안 켠다. 이것을 알릴 채널은 없다. 보증을 포기하고
  얻는 것이 문서 한 줄뿐이다 — 기각.
- **`build_affecting` 에서 `*.md` 를 뺀다**: 필터와 스크립트가 다시 같은 집합이 되지만, ADR-0137 의 판별식
  (무엇이 산출물 경로인가)을 바꾸는 일이다. 이 결정은 좌변의 폐포 범위만 개정하고 판별식은 안 건드린다
  (위 "개정하지 않는 것") — 기각.

## Reconsideration Triggers

**채널이 붙는 것**

- 게이트가 path 의존 경로를 읽는 모양(`cargo tree --prefix none` 의 `(/절대/경로)`)이 바뀌면 — 시험
  `a_change_in_a_linked_path_dependency_outside_the_workspace_demands_a_bump` 가 진짜 `cargo tree` 로 합성
  워크스페이스를 읽으므로 거기서 죽는다.
- `vendor/tiny_http` 가 워크스페이스 멤버가 되거나 사라지면 — 이 좌변에서 자연히 빠진다(멤버는 `crates/`
  쪽 판정의 일이다). 그때는 ADR-0516 이 먼저 바뀐다.

**원리적으로 안 붙는 것**

- 이 게이트를 Windows 셸에서 부르는 채널이 생기면 — `cargo tree` 가 `C:\…` 형태의 경로를 찍어 저장소 루트
  접두 비교가 안 맞고, 워크스페이스 밖 사본이 조용히 좌변에서 빠진다. 오늘 세 채널은 Linux 에서 돈다
  (CI 는 self-hosted Linux, 훅은 이 레포의 개발 머신). 재는 법: Windows 에서 사본 한 줄만 staged 하고
  `--staged` 가 rc 1 을 내는지 본다.
- `build_affecting` 에 디렉토리 모양이 더해지거나, 워크플로 `paths` 의 줄 순서가 바뀌면 — 필터가 판정
  대상의 상위집합에서 조용히 빠진다. 두 자리를 대조하는 판정기는 없다. 재는 법: 새 모양 아래 `.md`
  하나만 담은 push 에서 이 잡이 켜졌는지 `gh api "/repos/<owner>/<repo>/actions/runs?head_sha=<sha>"` 로 본다.
  이 결정의 필터 자체도 첫 push 전까지 발화는 **미측정**이다.
- `crates/` 밖 산출물 모양 변경이 잦아져 P.1 의 `cargo tree` 비용이 체감되면 — 재는 법: 그런 커밋에서
  `time bash scripts/check-plugin-version-bump.sh --staged` 를 잰다.

## References

- 개정 대상: [ADR-0166](0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md) (Decision 의 "넓히는 쪽" — 폐포의 범위)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 계기: [ADR-0516](0516-the-webhook-413-closes-the-connection-through-a-vendored-tiny-http-patch.md) — 워크스페이스 밖 사본
- [ADR-0137](0137-plugin-version-bump-is-judged-by-content-not-file-count.md) — 판별식(바뀌지 않는다)
- 현재 코드(결정이 실현된 위치): `scripts/check-plugin-version-bump.sh` 의 `EXTRA_ROOTS` · `links_crate` ·
  `read_members`, `.githooks/pre-commit` 의 `check_plugin_version_bump`, `.github/workflows/plugin-version-check.yml`
- 시험: `tests/plugin_version_bump_channel.rs`
- [docs/dev-guide/ci-gates.md](../dev-guide/ci-gates.md)
