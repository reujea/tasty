# Git Hooks

`.githooks/`에는 커밋, push, merge 전에 실행하는 검사 세 가지가 있다. clone 후 `./scripts/dev-setup.sh`를 실행하면 설치된다. `git config core.hooksPath .githooks`로 직접 설정할 수도 있다. 셸 스크립트를 커밋하려면 PATH에 `shellcheck`가 필요하다.

CI에서도 실행하는 검사와 로컬 훅에서만 실행하는 검사는 [ci-gates](ci-gates.md)에서 확인한다.

## pre-commit

코드 형식과 추가된 코드, 플러그인 버전, 문서 참조를 확인한다. 검사 시간은 변경 범위와 빌드 캐시에 따라 달라진다.

| ID | 검사 | 범위와 수정 방법 |
|----|------|------------------|
| M.1 | merge 커밋 차단 | 충돌을 해결한 merge 커밋을 [merge 커밋 판정](#merge-커밋-판정)으로 확인한다. 역병합과 승격이 아니면 진행 중인 merge를 취소하고 rebase 후 fast-forward merge한다. |
| A.1 | `mod`와 `use` 선언 순서 | 커밋 대상 Rust 파일의 선언 영역에서 `mod`를 `use`보다 앞에 둔다. |
| A.2 | `cargo fmt --check` | Rust 파일이 변경되면 워크스페이스를 검사한다. 제외된 크레이트는 해당 경로가 변경됐을 때 별도로 검사한다. |
| A.3 | 셸 스크립트 정적 검사 | `scripts/check-shell-assets.sh`에 커밋 대상 파일을 전달한다. `shellcheck`가 없거나 warning 이상의 문제가 있으면 중단한다. |
| C.6 | `let _ =`의 사유 주석 | 추가된 코드에서 값을 무시하는 이유를 같은 줄, 윗줄 또는 다음 줄의 주석으로 설명한다. 윗줄을 찾을 때 빈 줄과 속성은 건너뛴다. |
| C.8 | 색상 검사 이전 안내 | 함수는 호출되지만 별도 검사는 하지 않는다. 실제 검사는 Clippy로 이전됐다. [clippy-policy](clippy-policy.md) 참고. |
| C.9 | `egui::Window` 직접 사용 | 추가된 코드에서는 PopupManager/PopupDef를 사용한다. 예외는 [popup-implementation](popup-implementation.md)에 있다. |
| C.11 | `println!`와 `eprintln!` 사용 | 추가된 로그는 `tracing`으로 출력한다. CLI 등 표준 출력이 필요한 파일과 `vendor/` 사본은 제외한다. |
| C.12 | `dbg!` 사용 | 추가된 코드에서 디버그 매크로를 제거한다. |
| P.1 | 플러그인 버전 변경 | `scripts/check-plugin-version-bump.sh`로 staged 내용과 `main`의 공통 조상 커밋을 비교한다. 공통 조상을 찾지 못하면 HEAD를 사용한다. 연결된 작업 트리에 `tasty.pluginBump=deferred`가 worktree 범위로 있으면 검사하지 않고 안내만 출력한다([lane 작업 트리의 P.1 보류](#lane-작업-트리의-p1-보류)). |
| T.1 | 로컬 전용 문서 참조 | `cargo test -q -p tasty-doc-guards --test no_todo_file_citation`으로 저장소 전체를 검사한다. 커밋 대상이 아닌 파일도 포함된다. |
| W.1 | CHANGELOG 누락 안내 | 사용자 기능 관련 선언이 바뀌었지만 CHANGELOG가 변경되지 않았으면 안내한다. 커밋은 막지 않는다. |
| W.2 | 새 파일에 필요한 검사 안내 | 새 파일의 종류에 따라 테스트 명령을 안내한다. 커밋은 막지 않는다. |

C.6, C.9, C.11, C.12는 staged diff에 추가된 코드만 확인한다. 전체 문서·소스 검사는 `cargo test -p tasty-doc-guards`로 실행한다. 번역 키 정합 검사는 별도이며 [i18n](i18n.md)에 실행 방법이 있다.

플러그인 버전 검사는 파일 경로로 대상을 미리 제한하지 않는다. 공용 라이브러리나 vendor 의존성 변경도 플러그인에 영향을 줄 수 있기 때문이다. 검사 스크립트가 실제 의존 관계를 확인한다.

### lane 작업 트리의 P.1 보류

`main`에 반영된 커밋마다 플러그인 내용 변경과 그 patch 증가를 같은 커밋에 담는 정책은 그대로다. 여러 lane이 병렬로 작업해 병합 담당이 차례로 합치는 경우에는 증가를 병합 단계가 붙일 수 있다. 여러 lane이 같은 플러그인의 버전 줄을 각자 올리면 rebase 때마다 `Cargo.toml`·`tasty-plugin.toml`·`Cargo.lock`의 같은 줄에서 충돌하기 때문이다.

lane 작업 트리에서 다음과 같이 설정한다. `extensions.worktreeConfig`는 저장소 공용 설정이고, 두 번째 값은 그 작업 트리의 `config.worktree`에만 들어간다.

```sh
git config extensions.worktreeConfig true
git config --worktree tasty.pluginBump deferred
```

P.1은 다음 세 조건이 모두 맞을 때만 검사를 건너뛴다. 하나라도 어긋나면 지금처럼 검사하고, 받아들이지 않은 이유를 출력한다. 판정은 `scripts/lib/plugin-bump-mode.sh`에 있다.

- 값이 `deferred`다.
- 값이 worktree 범위에 있다. 공유 설정(`.git/config`)이나 사용자 설정에 둔 값은 모든 작업 트리의 검사를 끄게 되므로 따르지 않는다.
- 연결된 작업 트리다. 주 작업 트리(메인 저장소)는 `config.worktree`에 값이 있어도 검사한다.

이 모드는 환경 변수나 `--no-verify`와 달리 작업 트리에 남는 설정이다. `git -c`나 `GIT_CONFIG_*`로 준 값은 worktree 범위가 아니므로 따르지 않는다. 다른 pre-commit 검사는 그대로 실행하고, pre-push의 B.9도 바뀌지 않는다.

B.9는 커밋마다 검사하지 않는다. 원격의 기존 ref와 push할 끝 커밋을 `--range <원격 커밋> <로컬 커밋>`으로 한 번 비교해, push 범위 전체에서 증가가 빠진 플러그인을 막는다. 같은 범위 안의 다른 커밋이 이미 올렸다면 중간 커밋의 누락은 드러나지 않고, 새 ref는 비교할 원격 버전이 없어 건너뛴다. 따라서 `main`에 반영된 커밋마다 증가가 들어 있다는 보장은 아래 병합 단계의 커밋별 검사가 맡는다.

훅 파일은 `core.hooksPath`가 가리키는 작업 트리에서 읽으므로, 그 작업 트리가 이 판정을 담은 훅으로 갱신된 뒤부터 모드가 동작한다. 판정 파일이 없는 트리를 커밋할 때는 지금처럼 검사한다.

병합 담당은 lane 작업 트리에서 lane 브랜치를 기준 위로 rebase하면서 커밋마다 버전을 붙이고, 그 뒤 `main`에 반영할 커밋을 하나씩 검사한다.

```sh
git rebase --exec 'bash scripts/plugin-bump-fixup.sh' <기준>
for c in $(git rev-list --reverse <기준>..HEAD); do
    bash scripts/check-plugin-version-bump.sh --range "$c^" "$c" || exit 1
done
```

exec 에는 fixup 스크립트만 넣는다. 커밋별 시험은 exec 밖에서 실행한다. 이유와 방법은 [자체 검증](self-verification.md#커밋별로-검사할-때)에 있다.

lane 작업 트리에 `deferred`가 있어도 된다. amend 때 P.1은 건너뛰지만 fixup이 합친 뒤 같은 범위를 다시 검사하고, 이어지는 커밋별 검사가 반영 범위를 확인한다. 설정이 없는 작업 트리에서 rebase해도 결과는 같다.

`scripts/plugin-bump-fixup.sh`는 HEAD 커밋을 부모와 비교한다. 판정은 `check-plugin-version-bump.sh --range HEAD^ HEAD --violations-out <파일>`을 그대로 쓴다.

- 올려야 할 플러그인마다 `Cargo.toml`과 `tasty-plugin.toml`의 첫 `version` 줄을 같은 값으로 patch +1 한다. 목록 전체를 먼저 확인해, 어느 플러그인이든 두 값이 다르거나 `MAJOR.MINOR.PATCH` 숫자 형식이 아니면 파일을 고치기 전에 멈춘다.
- `cargo metadata --offline`으로 `Cargo.lock`을 갱신한다. 올린 패키지의 version 줄 말고 다른 줄이 바뀌면 멈춘다.
- `git commit --amend --no-edit`로 HEAD 커밋에 합친다. amend도 pre-commit을 거친다. 합친 뒤 같은 범위를 다시 검사한다.
- 파일을 고친 뒤 lock 갱신이나 amend에서 실패하면, 고친 파일을 HEAD 내용으로 되돌리고 끝낸다. 추적 파일은 시작할 때처럼 깨끗하다. 재검사가 실패한 경우만 증가가 이미 HEAD에 합쳐져 있다.
- 이미 올라가 있거나 플러그인 내용 변경이 없으면 아무것도 하지 않는다.
- 커밋하지 않은 추적 파일 변경이 있거나, merge 커밋이거나, 검사기가 판정 불가(2)를 내면 커밋을 바꾸지 않고 멈춘다.

종료 코드는 완료나 할 일 없음 0, 실패 1, 판정 불가 2다. 0이 아니면 rebase가 그 커밋에서 멈춘다. 그 자리에서 다음 중 하나로 처리한다.

- 이어 가기: 출력한 원인을 고친다. 커밋 내용이 원인이면(예: 두 version이 어긋남) 파일을 고쳐 `git commit -a --amend --no-edit`로 그 커밋에 넣는다. 훅 실패처럼 커밋 밖이 원인이면 그것을 고친다. 그다음 `bash scripts/plugin-bump-fixup.sh`를 다시 실행해 0을 확인하고 `git rebase --continue`를 실행한다.
- 그만두기: `git rebase --abort`로 rebase 전 커밋과 트리로 돌아간다.
- `git status --porcelain --untracked-files=no`에 수정이 남아 있다면(되돌리기 실패를 알린 경우) 다시 실행하기 전에 `git restore --source=HEAD --staged --worktree -- .`로 버린다. 남은 수정이 있으면 fixup은 판정 불가(2)로 거절한다.

`tests/plugin_bump_fixup.rs`가 amend 훅 실패와 뒤쪽 플러그인의 version 불일치에서 트리가 깨끗하게 남는지, 이어 가기와 그만두기가 실제로 되는지 rebase 중에 확인한다.

lane의 커밋 하나하나가 각자 patch +1을 받는다. 같은 플러그인을 바꾼 커밋이 여럿이면 그 수만큼 올라간다. lane이 이미 올린 커밋은 그대로 둔다. 그러나 lane이 버전 줄을 직접 바꾸면 rebase 충돌이 다시 생기므로, `deferred` 작업 트리에서는 버전 줄을 건드리지 않는다.

## pre-push

먼저 push할 커밋의 플러그인 버전을 검사한다. 실패하면 컴파일을 시작하지 않고 중단한다. 통과하면 아래 순서대로 빌드, 문서, 소스 가드를 검사한다. 빌드 검사 중 하나가 실패해도 나머지는 실행하여 오류를 한 번에 확인할 수 있다.

Git은 훅에 `GIT_DIR`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE` 같은 저장소 환경을 전달할 수 있다.
pre-push는 먼저 원 저장소와 로그 위치를 확정하고, 각 검사를 실행하는 서브셸에서
`git rev-parse --local-env-vars`가 열거한 변수만 해제한 뒤 원 저장소를 작업 디렉터리로 고정한다.
B.9의 명시 ref 조회는 같은 저장소를 읽되, Cargo·도구·시험이 만드는 다른 Git 저장소에는
push 저장소의 경로를 전달하지 않는다. 이는 [Git 훅의 저장소 환경 규칙](https://git-scm.com/docs/githooks#_description)을 따른다.
부모 Git 프로세스의 환경은 바뀌지 않으며 사용자 HOME/PATH와 검사 argv·실패 처리도 유지한다.
`githooks_are_pinned`의 일회용 저장소 회귀는 이 경계를 검사한다. fixture의 Cargo 대역 성공은
실제 빌드·문서 검사 통과를 뜻하지 않는다.

| ID | 검사 |
|----|------|
| B.9 | `scripts/check-plugin-version-bump.sh --range <원격 커밋> <로컬 커밋>`으로 원격에 게시된 플러그인 버전과 비교한다. |
| B.4 | `cargo clippy --workspace --all-targets -- -D clippy::correctness`로 워크스페이스의 개발 빌드·테스트 코드 컴파일과 lint를 함께 검사한다. |
| B.8 | `cargo check --workspace --release --locked` |
| B.6 | `cargo check --no-default-features` |
| B.7 | `cargo test -p tasty-doc-guards`. `TASTY_PAGE_DIR`가 없으면 `scripts/fetch-design-source.sh`로 `crates/tasty-design-tokens/dtcg/SOURCE` 커밋의 tasty-page를 공용 `.git/design-source/<SOURCE 커밋>`에 받아 넘긴다(네트워크 필요). |
| B.10 | `cargo test --lib --locked --no-fail-fast -- source_guards`로 루트 lib의 소스 가드를 실행한다. 종료 코드가 0이어도 로그에 `test source_guards::… ... ok` 줄이 없으면 실패한다. |

기본 feature의 개발 빌드·테스트 코드 컴파일 검사는 Clippy 한 번으로 수행한다.
같은 타깃에 대한 `cargo check --workspace --all-targets`는 중복 실행하지 않는다.
release와 headless는 컴파일 조건이 달라 각각 검사한다. 문서 가드는
[빌드 프로필](build.md#빌드-프로필-3종)의 패키지별 최적화를 적용하며 전체 항목을 실행한다.
소스 가드는 루트 lib의 `#[cfg(test)]` 모듈이라 B.4는 컴파일만 하고 B.7은 다른 크레이트를 실행한다. B.10은 이 모듈만 이름 필터로 골라 실행하며 루트 lib의 다른 시험은 실행하지 않는다.
필터는 부분 일치이고 맞는 시험이 없어도 Cargo는 0으로 끝나므로, B.10은 모듈 경로로 통과한 시험이 있는지 확인한다. 모듈 안의 일부 시험만 사라진 경우는 이 확인으로 찾지 못한다.

B.9는 Git이 전달한 ref별 커밋을 검사한다. 원격 커밋을 로컬에서 찾지 못하면 `git fetch`가 필요하다. 새 ref는 비교할 원격 버전이 없고 삭제할 ref는 올릴 내용이 없으므로 B.9를 생략한다. ref별 검사·생략 수는 로그에 남는다.

훅을 직접 실행하면 Git의 ref 정보가 없어 B.9를 생략했다고 표시한다. B.9는 표의 명령으로 따로 실행할 수 있다. push할 ref가 없으면 빌드도 생략한다.

빌드·문서 검사는 현재 작업 트리에서 실행한다. 따라서 다른 커밋을 지정해 push하거나 미커밋 변경이 있는 경우에는 push할 커밋의 빌드를 검증한 것으로 볼 수 없다.

### 실행 시간과 로그

각 검사는 `시작: 플러그인 버전`, `통과: 플러그인 버전 (1초)`처럼 이름과 상태·소요 시간을 표시하고 마지막에 전체 시간을 요약한다. 검사 ID는 진행·요약 문구에 붙이지 않으며 문서와 로그 파일을 연결하는 용도로 사용한다. 최초 빌드나 소스 변경 후에는 분 단위로 걸릴 수 있으며, 캐시가 있으면 짧아진다. 개발 빌드, release 빌드, headless 빌드는 설정이 달라 각각 검사한다.

B.10은 루트 크레이트를 시험 프로필로 따로 컴파일하므로 루트 소스를 바꾼 push에서 다른 검사보다 오래 걸린다. 2026-11-08 20코어 aarch64 Linux에서 `CARGO_BUILD_JOBS=4`로, 다른 빌드가 함께 돌아 load가 44.85~62.82인 상태에서 잰 값이다. `src/lib.rs` 내용을 바꾼 뒤 B.10은 91~187초(3회), 변경이 없으면 13초(1회)였다. 시험 프로필 산출물이 없으면 처음 실행에 빌드만 13분 39초(1회, load 30대)가 더 걸렸다.

로그는 `git rev-parse --git-path hook-logs`가 가리키는 디렉토리 아래 실행별 폴더에 저장한다. `B.4.log`처럼 검사 ID별 전체 출력과 `summary.log`를 확인할 수 있다. 실패하면 마지막 60줄과 전체 로그 경로를 표시한다. 이전 로그는 자동으로 삭제하지 않으며 필요 없으면 해당 실행 폴더를 지워도 된다.

검사 결과는 출력의 마지막 명령이 아니라 검사 명령 자체의 종료 코드로 결정한다. 로그 저장이나 요약 출력이 성공해도 실패한 검사가 통과로 바뀌지 않는다.

## pre-merge-commit

fast-forward가 아닌 merge 커밋 중 역병합과 승격이 아닌 것을 막는다([merge 커밋 판정](#merge-커밋-판정)). 차단되면 `git merge --abort`로 현재 merge를 취소한다. 작업 브랜치에서 `git rebase <대상-브랜치>`를 실행한 뒤 대상 브랜치로 전환해 `git merge --ff-only <작업-브랜치>`로 반영한다.

충돌한 merge는 이 훅 대신 충돌 해결 후 실행하는 `git commit`의 pre-commit M.1에서 같은 판정으로 확인한다. 충돌을 해결할 때는 양쪽 변경이 필요한 이유를 확인하고 내용을 합친다.

### merge 커밋 판정

pre-merge-commit과 M.1은 `scripts/lib/merge-policy.sh`의 `merge_commit_allowed`로 판정한다. 아래 다섯 경우만 허용한다.

| 현재 브랜치 | 합치는 브랜치 |
|---|---|
| `release` | `main` |
| `develop` | `main` |
| `develop` | `release` |
| `release` | `develop` |
| `main` | `release` |

- 현재 브랜치는 `HEAD`가 가리키는 브랜치 이름이다. detached HEAD에서는 거부한다.
- 합치는 커밋은 M.1에서는 `MERGE_HEAD` 파일에서 읽는다. pre-merge-commit이 실행될 때는 Git이 `MERGE_HEAD`를 아직 쓰지 않으므로, `git merge`가 합치는 커밋마다 넘기는 `GITHEAD_<커밋>` 환경 변수에서 읽는다(Git 2.43.0에서 확인).
- 합치는 브랜치는 그 커밋과 같은 커밋을 가리키는 `develop`·`release`·`main`이나 `origin/develop`·`origin/release`·`origin/main`이다. 해시로 합쳤거나 그 사이 브랜치가 움직였다면 이름을 찾지 못해 거부한다.
- 커밋 두 개 이상을 한 번에 합치는 merge는 거부한다.
- 판정 파일이 없는 트리에서는 모든 merge 커밋을 막는다.

`scripts/tests/merge-policy.sh`가 판정 표 전체와 일회용 저장소의 실제 merge(허용 다섯 경우, 작업 브랜치 merge, detached HEAD, 여러 커밋 merge, 이름 없는 커밋, 충돌 상태)를 확인한다. `githooks_are_pinned` 시험이 이 스크립트를 실행한다.

## 검사 추가와 검증

pre-commit에 검사를 추가할 때는 `check_*` 함수를 만들고 `CHECKS` 배열에 등록한다. 실패는 `fail` 또는 `FAIL=1`로 기록한다. 안내만 필요한 경우에는 `yellow`로 출력하고 실패 상태를 바꾸지 않는다.

`crates/tasty-doc-guards/tests/githooks_are_pinned.rs`는 훅 파일, 검사 ID, 이 문서의 표, pre-commit 실행 목록이 일치하는지 확인한다. 검사 ID는 훅의 구분선 바로 아래 주석에서 읽는다. 수정 후에는 `cargo test --locked -p tasty-doc-guards --test githooks_are_pinned`를 실행한다.
