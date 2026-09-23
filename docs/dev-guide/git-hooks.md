# Git Hooks

`.githooks/` 에 pre-commit / pre-push / pre-merge-commit 훅이 있다. clone 직후 **1회 셋업**:

```bash
./scripts/dev-setup.sh   # core.hooksPath 를 .githooks/ 로 설정 (멱등)
```

안 하면 hook 이 안 돈다. 긴급 우회: `git commit --no-verify` / `git push --no-verify`. (merge 차단 우회는 아래 pre-merge-commit 절 참고)

**어떤 검사가 CI 에도 있고 어떤 검사가 훅에만 있는지는 [ci-gates](ci-gates.md) 의 표가 정본이다.** 훅을 설치하지 않거나 우회하면 훅 전용 검사(mod/use 선언 순서 · `egui::Window` 직접 사용 · `println!`/`dbg!`)는 아무 데서도 잡히지 않는다.

## pre-commit (2–5초 — 그중 T.1 이 실측 2.0 s)

A.1/A.2 는 파일 전체, C.* 는 **staged diff 의 추가 라인만** 검사(기존 코드 통과, 신규 위반만 차단). T.1 만 **레포 전체 작업 트리**를 본다 — 그 가드의 좌변이 순회라 staged 밖 위반도 잡힌다. 화이트리스트·정확한 검출 로직은 `.githooks/pre-commit` 가 SoT.

| ID | 검사 | 목적 |
|----|------|------|
| A.1 | top-level 선언 영역에서 `mod` 가 `use` 뒤에 나오는지 | 선언 순서 |
| A.2 | `cargo fmt --check` | rustfmt 강제 |
| A.3 | 셸 자산 정적 검사 (`shellcheck`, warning 이상) | `scripts/check-shell-assets.sh` 를 훅과 CI 가 **같은 것으로** 부른다 — 모수만 다르다(훅은 staged 자산, CI 는 추적되는 자산 전부). 검사기가 없으면 통과가 아니라 **판정 불가로 멈춘다**(`scripts/dev-setup.sh` 가 받아 둔다). 문턱의 근거는 [ADR-0295](../adr/0295-shell-assets-are-judged-at-warning-and-above.md) |
| C.6 | 주석 없는 `let _ =` | 왜 무시하는지 흔적 강제 (전수판은 `crates/tasty-doc-guards/tests/let_underscore_documented.rs` — 아래 참고) |
| C.8 | UI 색상 하드코딩 | **비활성** — `clippy.toml` 의 `disallowed-methods` 가 같은 검사를 `#[allow]` 인식·path resolution 까지 포함해 대신한다(grep 판은 의도된 외부 입력 자리의 `#[allow]` 도 잡아 오탐이 많았다). 훅에는 즉시 통과하는 자리만 남아 있고 **실행 수에는 들어간다** — 통과 줄의 `검사 N/N` 이 이 자리를 센다. 실제 색 검사는 pre-push 의 `B.4` 가 한다 |
| C.9 | `egui::Window::` 직접 사용 | PopupManager 강제 ([popup-implementation](popup-implementation.md)) |
| C.11 | `println!`/`eprintln!` | `tracing::*` 강제 (예외: CLI 출력 — `crates/tasty-cli/*`, `src/boot/cli_routing.rs`) |
| C.12 | `dbg!` | release leak 방지 |
| M.1 | 2-parent merge 커밋 (branch 가 갈라지는 merge) | 갈래 커밋 차단. `pre-merge-commit` 은 **clean non-ff merge 만** 잡는다 — 충돌 merge 는 git 이 커밋을 만들지 않고 멈춘 뒤 resolve → `git commit` 으로 마무리되므로 이 훅을 탄다 |
| P.1 | plugin 산출물이 바뀌었는데 매니페스트 `version` 이 그대로 | `scripts/check-plugin-version-bump.sh` 를 훅과 CI 가 **같은 것으로** 부른다 — 둘이 갈리지 않게. 비교 기준은 `HEAD` 가 아니라 **`main` 과의 merge-base** 라 `--amend`·rebase 에 안 흔들린다 ([ADR-0137](../adr/0137-plugin-version-bump-is-judged-by-content-not-file-count.md)). 이 모수는 **발행을 묻지 않는다** — 그 물음은 pre-push `B.9` 가 원격 tip 을 모수로 답한다 |
| T.1 | 커밋되지 않는 로컬 티켓·문서를 가리키는 인용 | `cargo test -q -p tasty-doc-guards --test no_todo_file_citation` 을 그대로 부른다. **이 검사만 staged diff 가 아니라 레포 전체 작업 트리를 본다** — 가드의 좌변이 순회라 그렇다. 내가 안 건드린 파일이 범인일 수 있는 대신, staged 밖에 남은 죽은 인용도 같이 막힌다. pre-push `B.7` 과 겹치는 것은 의도다(커밋 대 push). 규칙 전문은 [ADR-0105](../adr/0105-no-nongit-path-refs-in-tracked-sources.md) |
| W.1 | 사용자 표면 선언 파일(`crates/tasty-ipc/src/method_meta.rs` · `crates/tasty-cli/src/commands/` · `crates/tasty-plugin-*/tasty-plugin.toml`)이 staged 인데 `CHANGELOG.md` 는 아님 | CHANGELOG 누락 상기 — **경고만, 커밋은 통과** |
| W.2 | 새로 추가된 파일을 **처음 보는 타깃** 안내 | 판정자가 다른 패키지에 있어 놓치는 일을 줄인다 — `cargo test -p <크레이트>` 는 루트 패키지의 통합 타깃을 안 돌리고 그 반대도 마찬가지다. "이 파일을 무엇이 판정하는가" 의 정확한 매핑은 순회 범위를 소스에서 읽어야 해 근사밖에 안 되므로, **새것의 종류**(경로 모양)로만 안내한다 — **경고만, 커밋은 통과** |

> W.1 이 경고에 그치는 이유: 그 파일을 만졌다고 반드시 사용자 표면이 바뀌는 것은 아니다(내부 refactor, 도움말 오타, 매니페스트 버전 bump). 하드 실패로 만들면 무해한 커밋마다 `--no-verify` 를 쓰게 되고 훅 전체가 무력화된다 — 판단은 사람이 한다.
>
> 색상 하드코딩(옛 C.8)은 pre-commit 에서 빠지고 **clippy `disallowed-methods`** 로 이관됐다 — `#[allow]` 와 path 예외를 정확히 인식한다([theme › 색 생성 정책](../design/systems/theme.md#색-생성-정책), [clippy-policy](clippy-policy.md)).
>
> C.6 은 staged diff 만 보므로 **기존 코드의 위반은 못 잡는다**. 전수 검사는
> `crates/tasty-doc-guards/tests/let_underscore_documented.rs` 가 한다 — 훅이 인정하는 세 형태(같은 줄·윗줄·다음 줄)를
> 모두 포함하고 조금 더 넓어(빈 줄·속성 건너뛰기, 멀티라인 문장 내부), 훅이 통과시킨 코드를 전수
> 검사가 떨어뜨리는 방향은 생기지 않는다. 그 채널은 [ci-gates](ci-gates.md) 가 정본이다. 판정 규약은 [error-handling](error-handling.md) "주석 위치".

> i18n(번역 키 정합·자연어 하드코딩)은 pre-commit 검사가 아니다 — 소스 전체를 읽어야 해서 hook 예산(1–3초)을 넘는다. 집행하는 타깃은 둘이고 **채널이 서로 다르다.**

> `crates/tasty-doc-guards/tests/no_hardcoded_ui_strings.rs`(자연어 하드코딩)는 의존 0 크레이트에 살아서 `doc-guards.yml` 이 경로 필터 없이 돌린다 — main push 와 PR 마다 자동으로 돈다([ci-gates](ci-gates.md)).

> `tests/i18n_key_parity.rs`(번역 키 정합)는 루트 패키지의 통합 타깃이라 기본 조합의 `cargo test --workspace` 에는 **자동 채널이 없다** — 자동 실행은 `check-headless` 잡에서만 일어나고, 기본 조합의 전체 스위트는 병합 후 main 에서 사람이 돌린다([ci-gates](ci-gates.md)). 로컬 확인 명령은 [i18n](i18n.md) "강제 테스트" 절.

## pre-push (수십초)

| ID | 검사 |
|----|------|
| B.9 | plugin 의 **발행 판정** — 밀려는 ref 마다 `scripts/check-plugin-version-bump.sh --range <원격 tip> <로컬 tip>`. **pre-commit 의 P.1 과 같은 스크립트를 다른 모수로** 부른다: P.1 은 "내 커밋이 버전을 올렸나"(staged vs merge-base), B.9 는 **"발행된 값과 지금 내용이 짝이 맞나"**(원격 tip vs 로컬 tip). 그 모수는 git 이 훅의 stdin 으로 직접 준다 — push 순간이 곧 발행 순간이라 여기가 그것을 손으로 안 구해도 되는 유일한 자리다. 분할 착지(두 lane 이 서로 다른 base 에서 **같은 값**으로 올려 같은 버전 아래 두 산출물이 남는 것)를 여기서 잡는다 ([ADR-0192](../adr/0192-a-repo-wide-ratchet-is-judged-at-the-merge-tree-not-per-lane.md) · [ADR-0137](../adr/0137-plugin-version-bump-is-judged-by-content-not-file-count.md)) |
| B.10 | 공용 모수의 **트리 판정** — 밀려는 ref 마다 `scripts/check-population-freshness.sh --rev <로컬 tip>`. `crates/tasty-doc-guards/src/floored_walk.rs` 의 `populations::*` 네 선언이 **그 tip 의 트리**와 같은지 본다. B.9 와 같은 이유로 여기 있다 — 모자란 것이 술어가 아니라 **모수**다: 두 lane 이 각자 파일을 하나씩 더하면 둘 다 자기 base 기준으로 **같은 +1 값**을 적고, 값이 같아 git 이 충돌을 안 내며, 두 lane 의 트리에서는 각각 참이고 **병합된 트리에서만** 거짓이 된다. 그 병합된 트리가 처음 실재하는 자리가 push 다 ([ADR-0192](../adr/0192-a-repo-wide-ratchet-is-judged-at-the-merge-tree-not-per-lane.md)) |
| B.5 | `cargo check --workspace --all-targets` |
| B.6 | `cargo check --no-default-features` (headless 빌드 — `gui` feature 없이 컴파일) |
| B.4 | `cargo clippy --workspace --all-targets -- -D clippy::correctness` |
| B.7 | `cargo test -p tasty-doc-guards` — 문서·매니페스트·인덱스 정합. `adr_index_parity`·`readme_badge_parity`·`ci_channel_claims_match_workflows` 등은 pre-commit 이 안 보므로 여기가 **로컬 조기 채널**이다(자동 채널은 `doc-guards.yml`) |
| B.8 | `cargo check --workspace --release --locked` (B.5 와 `debug_assertions` 이 반대 — 소비자가 debug 쪽에만 있는 항목의 `dead_code`) |

B.9 와 B.10 이 표에서 먼저인 것은 훅 안에서의 실행 순서이기도 하다 — 나머지 다섯은 컴파일이라 분
단위인데 그 둘은 초 단위다. 발행 판정으로 빨개질 커밋을 컴파일 다섯 번 뒤에 알려 주는 것은 같은 답을
훨씬 비싸게 주는 것이다.

**B.10 은 lane 안에서도 부를 수 있지만 거기서는 답이 다르다.** `--rev HEAD` 로 언제든 돌아가고,
lane 이 자기가 움직인 모수를 갱신했는지는 그것으로 확인된다. 그러나 그 답은 **lane 의 트리**에 대한
것이라 분할 착지를 못 본다. 실측 2026-09-20: 같은 base 에서 lane 둘이 `docs/` 에 문서를 하나씩 더하고
둘 다 선언을 451 로 적으면, 두 lane 의 트리에서 각각 `통과` 가 나오고 병합한 트리에서만 `위반`(선언
451 · 트리 452)이 나온다. 그래서 lane 의 초록을 착지 통과로 읽지 않는다 — P.1 과 B.9 의 관계와 같다.

**B.9 는 모수를 못 정하면 통과시키지 않는다.** 원격 tip 이 로컬에 없는 객체면(얕은 clone · fetch
안 함) 견줄 대상이 없으므로 실패하고 `git fetch` 를 찍는다. **견줄 발행 값이 원래부터 없는** 두
경우 — 원격에 그 ref 가 없는 최초 생성 push, 그리고 삭제 push — 만 사유를 찍고 건너뛴다. 훅을 git
이 아니라 사람이 직접 부르면 ref 목록 자체가 없으므로 건너뛰되, 손으로 재는 명령을 찍는다. 매
실행이 **본 ref 수 · 판정한 수 · 건너뛴 수**를 찍는 것은 빈 모수를 훑은 초록과 실제로 판정한
초록이 같은 줄로 보이지 않게 하려는 것이다([ADR-0183](../adr/0183-a-green-check-is-not-evidence-without-a-control.md)).

**P.1 과 B.9 모두 사전 필터가 없다 — 경로로 미리 거르면 판정 대상의 일부가 사각이 된다.** 판정
대상은 plugin 디렉토리가 아니라 **의존 폐포**이고([ADR-0166](../adr/0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md)),
폐포는 `crates/` 밖의 워크스페이스 밖 path 의존(`vendor/tiny_http`)까지 닿는다([ADR-0537](../adr/0537-the-plugin-version-gate-follows-path-dependencies-outside-the-workspace.md)).
그래서 두 훅은 staged(P.1) · 밀려는 ref(B.9)가 있으면 조건 없이 게이트를 부르고, 무엇이 산출물인지
거르는 일은 스크립트 하나가 한다. 경로 선필터가 사각을 만든다는 근거는 실측이다(2026-09-20):
P.1 이 staged 목록이 `crates/tasty-plugin-<이름>/` 에 걸릴 때만 부르던 판에서,
`crates/tasty-utils/src/id.rs` 에 출하되는 한 줄을 더해 staged 하니 선필터에 걸리는 파일이 0 이라
pre-commit 은 plugin 버전 줄을 한 줄도 안 찍었고, 같은 커밋을 push 하니 B.9 가 번들 plugin **9 개
전부**를 위반으로 냈다. 선필터를 `^crates/` 로 넓혀도 같은 형태가 `vendor/` 에서 다시 났다(ADR-0537
Context). 두 훅이 모두 부르므로 P.1 은 커밋 시점에, B.9 는 발행 모수로 push 시점에 판정한다 — 모수의
차이는 위 절 그대로다.

clippy 의 `style`/`pedantic` 은 warning 으로만(error 승격 안 함 — false positive 노이즈 방지).

훅은 `set -e -o pipefail` 로 연다. 스텝들이 `cargo … 2>&1 | tail -N` 형태라 `pipefail` 이 없으면 종료코드가 `tail` 의 것이 되고 — `tail` 은 거의 항상 0 이다 — **cargo 가 죽어도 훅이 통과한다**. 새 스텝을 더할 때 파이프로 끝내려면 이 옵션이 살아 있는지 먼저 확인한다.

## pre-merge-commit (즉시)

non-fast-forward merge 를 차단한다. 이 훅은 merge 가 **merge 커밋을 만들 때만**, 즉 non-ff merge 일 때만 실행된다 — ff merge 는 커밋을 만들지 않으므로 훅이 돌지 않고 통과한다. 따라서 "ff 가 아닌 merge 시도" 만 정확히 잡는다.

> **충돌한 merge 는 이 훅이 못 본다.** 그때 git 은 커밋을 만들지 않고 멈추고, 사람이 resolve 한 뒤 `git commit` 으로 마무리한다 — 그 커밋은 `pre-merge-commit` 이 아니라 **pre-commit 의 M.1** 을 탄다. 두 훅이 갈래 커밋을 나눠 막는다.

| 정책 | 내용 |
|------|------|
| 권장 | merge 는 ff-merge(fast-forward) 를 권장 |
| 일반 merge 허용 조건 | rebase 또는 ff-merge 가 **정말로 불가능한** 경우에만 |

차단 시점엔 git 이 이미 merge 를 **진행 중 상태(MERGE_HEAD)** 로 남긴다. 따라서 우회는 두 갈래다:

- **되돌리기**: `git merge --abort` → rebase + `git merge --ff-only` 로 재시도.
- **그대로 마무리(선택)**: ff/rebase 가 정말 불가능함을 확인한 뒤에만 `git commit --no-verify` 로 진행 중인 merge 를 완료. (처음부터 선제 우회하려면 깨끗한 상태에서 `git merge --no-verify <브랜치>`)

rebase 충돌 해결 시 충돌 마커만 지우지 말고, 각 충돌이 어떤 변경끼리 부딪힌 것인지·양쪽 의도가 무엇인지 확인하고 '맞는' 합본을 판단한 뒤 진행한다(애매하면 `git rebase --abort` 후 원인 분석). conductor 의 merge 정책과 일치한다.

## 새 검사 추가

`.githooks/pre-commit` 의 `check_*` 함수 패턴: staged 파일 순회 → 위반 시 `fail`(메시지+FAIL=1) → 마지막 exit code 결정. 1초 이상 느려지면 pre-push 로 옮긴다.

경고만 내는 검사(W.*)는 `fail` 을 쓰지 않고 `yellow` 로 출력만 한다 — `FAIL` 을 건드리지 않으므로 exit code 에 영향이 없다.

## 훅 자신은 무엇이 고정하는가

훅은 다른 것을 판정하지만, **훅 자신을 판정하는 자리**도 따로 있어야 한다. 없으면 훅 파일이 통째로 사라져도 아무것도 안 빨개진다 — 실제로 그런 상태였다(훅 하나를 치우고 `cargo test -p tasty-doc-guards` 를 돌려도 한 건도 안 죽었다).

`crates/tasty-doc-guards/tests/githooks_are_pinned.rs` 가 그 자리다. 세 가지를 본다.

- **파일 명부** — `.githooks/` 에 있어야 하는 이름 셋을 값으로 들고 있다. 하나가 사라지면 그 이름을 찍고 빨개진다. 수가 아니라 이름인 이유는 자리바꿈(하나가 사라지고 다른 하나가 생김) 때문이다.
- **반대 방향** — 명부에 없는 훅 파일이 디렉토리에 있으면 빨개진다. 훅을 새로 놓고 명부와 이 문서에 안 적으면 거기서 막힌다.
- **내용** — 빈 파일은 "있다" 로 세어지지만 아무 검사도 안 하면서 초록을 낸다. 그래서 줄 수 하한과 bash shebang 을 함께 본다.

디렉토리가 통째로 없으면 순회가 빈손으로 돌아와 "셋 다 사라졌다" 와 "못 읽었다" 가 같은 실패문이 된다. 그 둘을 가르는 줄이 명부 대조보다 먼저 선다.

같은 파일이 **스텝 명부**도 본다. 각 훅의 구역 배너(박스 괘선 바로 아래 줄)에서 검사 ID 를 읽어, 이 파일의 명부와 위 두 표를 셋이 같은지 묻는다. 그래서 스텝 하나를 지우면 셋이 어긋나 빨개지고, 표에서 행을 지워도 같은 방향으로 빨개진다. 산문에 나오는 같은 ID 는 괘선이 없어 안 걸린다.

`pre-commit` 은 한 가지를 더 본다 — 검사 함수의 수와 `CHECKS` 실행 목록의 길이가 같은지다. 훅 자신도 실행할 때 그 둘을 견주지만, 훅을 안 설치했거나 `--no-verify` 로 넘긴 커밋에서는 그 비교가 아예 없다. 그리고 그 훅의 명부는 검사 함수 수보다 작을 수 없다 — 명부와 표를 **같이** 비우면 두 대조가 짝이 맞아 조용히 통과하므로, 그 방향을 이 줄이 막는다.

`M.1` 이 `pre-commit` 아래에 있는 것은 편집 실수가 아니다. 충돌한 merge 는 `pre-merge-commit` 을 안 타고 `git commit` 으로 마무리되므로, 그 갈래를 막는 코드가 `pre-commit` 에 있다. `pre-merge-commit` 자신은 검사가 하나뿐이라 안에서 가리킬 ID 가 없고, 명부에 빈 줄로 남겨 "아직 안 적었다" 와 "적을 것이 없다" 를 가른다.

자동 채널은 `doc-guards.yml` 이고, 커밋 전에 직접 돌리는 명령은 `cargo test --locked -p tasty-doc-guards --test githooks_are_pinned` 다.
