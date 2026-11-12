# 브랜치 전략

이 레포는 `develop`, `release`, `main` 세 브랜치로 개발 중인 커밋, 릴리스 후보, 배포된 버전을 나눈다. 이 문서가 브랜치 규칙의 정본이다. 다른 레포가 이 규칙을 따르는지는 그 레포의 문서에 적는다.

릴리스 단계별 명령은 [릴리스 절차](release.md), 브랜치별로 도는 검사는 [CI 가이드](ci-gates.md), 커밋과 merge를 막는 훅은 [Git 훅](git-hooks.md)에 있다.

## 브랜치

| 브랜치 | 뜻 | 들어오는 것 |
|---|---|---|
| `develop` | 개발 중인 버전. 통합 브랜치이며 GitHub 기본 브랜치다 | 작업 브랜치가 CI를 통과하면 fast-forward로 반영한다. `release`·`main`의 역병합 |
| `release` | 릴리스 후보 | `develop`에서 고른 시점. 실제 실행 검증, 본체 버전 올리기, 릴리스 버그 수정, `main`의 역병합 |
| `main` | 배포된 버전. 릴리스 태그(`v*`)는 `main`의 커밋에만 붙인다 | 릴리스 때 `release`. hotfix |

`develop`이 GitHub 기본 브랜치이므로 새 clone의 기본 checkout, PR의 기본 base, `schedule`·`workflow_dispatch`로 도는 워크플로의 기본 ref는 `develop`이다.

## 작업 브랜치와 develop 반영

- 작업 브랜치는 `develop`에서 갈라 `develop` 위로 rebase한다. 반영은 `git merge --ff-only <작업-브랜치>`로 한다. 작업 브랜치를 merge 커밋으로 합치지 않는다.
- 외부 기여 PR의 base는 `develop`이다.
- 플러그인 patch 증가는 `develop`에 반영하는 커밋에 들어간다. 병렬 작업 트리에서 병합 단계가 붙이는 방법은 [lane 작업 트리의 P.1 보류](git-hooks.md#lane-작업-트리의-p1-보류)에 있다.

## release 승격

1. `release`를 `develop`에서 고른 커밋으로 올린다. `release`가 그 커밋의 조상이면 `git merge --ff-only <커밋>`으로, 아니면(릴리스 버그 수정이 아직 `develop`에 없을 때) merge 커밋으로 합친다.
2. `release`에서 실제 실행 검증을 하고 본체 patch 버전, README 배지, CHANGELOG, 릴리스 노트를 담은 버전 커밋을 만든다([릴리스 절차](release.md)).
3. `release`에서 고친 버그와 버전 커밋은 `develop`에 역병합한다. `develop`이 앞서 있으면 merge 커밋이 된다.

`release`에는 새 기능을 넣지 않는다. 검증, 버전, 릴리스 버그 수정만 넣는다.

## main과 태그

1. `main`을 `release`로 fast-forward한다(`git merge --ff-only release`). 릴리스 노트는 태그 커밋의 본문이므로 태그는 merge 커밋이 아니라 버전 커밋에 붙인다. `main`에 `release`에 없는 hotfix가 있어 fast-forward가 되지 않으면 먼저 `main`을 `release`에 역병합하고 그 위에서 버전 커밋을 만든다.
2. 버전 커밋에 태그를 붙이고 `release`·`main`과 태그를 한 번에 push한다. `--atomic`이라 하나라도 거부되면 셋 다 갱신되지 않는다.

   ```bash
   git tag vX.Y.Z
   git push --atomic origin release main vX.Y.Z
   ```

3. 태그 push가 릴리스 워크플로(`release.yml`)를 시작한다. `create-release` 잡은 태그 커밋이 `origin/main`의 조상인지 먼저 확인하고, 아니면 실패한다. `main`에 반영하지 않은 커밋에 태그를 붙여도 릴리스되지 않는다.

## hotfix

1. `main`에서 갈라 수정하고 `main`에 반영한다.
2. `main`에서 본체 patch 버전을 올리고 그 버전 커밋에 태그를 붙여 `main`과 태그를 한 번에 push한다(`git push --atomic origin main vX.Y.Z`). `release`는 이 push에 넣지 않는다. 태그 커밋 확인은 [main과 태그](#main과-태그)의 3번과 같다.
3. `main`을 `release`와 `develop`에 역병합한다. 대상이 앞서 있으면 merge 커밋이 된다.

역병합을 cherry-pick으로 하지 않는다. cherry-pick한 커밋은 해시가 달라 다음 `release` → `main` 승격이 fast-forward로 되지 않는다.

## merge 커밋을 허용하는 경우

merge 커밋은 아래 다섯 경우에만 만든다. 그 밖의 merge(작업 브랜치를 `develop`에 합치는 경우 포함)는 훅이 막는다.

| 현재 브랜치 | 출처 브랜치 | 종류 |
|---|---|---|
| `release` | `main` | 역병합 |
| `develop` | `main` | 역병합 |
| `develop` | `release` | 역병합 |
| `release` | `develop` | 승격 |
| `main` | `release` | 승격 |

`pre-merge-commit`과 pre-commit의 M.1이 같은 판정 함수(`scripts/lib/merge-policy.sh`)로 허용 여부를 정한다. 출처는 합치는 커밋을 포함하는 `develop`·`release`·`main`(로컬 브랜치가 없으면 `origin/<이름>`)이고, 그 가운데 하나라도 위 표의 방향이면 허용한다. 그래서 `develop`의 과거 시점도 `release`로 승격할 수 있고, 어느 장기 브랜치에도 없는 작업 브랜치 커밋은 거부된다. 판정 세부와 시험은 [merge 커밋 판정](git-hooks.md#merge-커밋-판정)에 있다.

## 버전

- 본체 patch 버전은 `release`에서 올린다. `develop`의 버전은 역병합으로 따라간다. 예외로 hotfix의 patch는 `main`에서 올리고, `release`와 `develop`은 역병합으로 따라간다. 두 README 배지와 `Cargo.toml`은 같은 커밋에서 바뀌므로 어느 브랜치에서든 `readme_badge_parity`가 통과한다.
- 플러그인 patch 버전은 `develop`에 반영할 때 올린다.

## CI

`develop`·`release`·`main` push와 이 세 브랜치를 base로 한 PR에서 같은 검사가 돈다. 같은 커밋을 `develop`에서 `release`, `main`으로 fast-forward하면 그 커밋의 검사가 브랜치마다 한 번씩 다시 돈다. 실행 그룹은 ref마다 나뉘므로 한 브랜치의 push가 다른 브랜치의 실행을 취소하지 않는다. 트리거별 검사 목록은 [CI 가이드](ci-gates.md)에 있다.

## tasty-page와의 관계

- 디자인 정본은 tasty-page `develop`의 `vendor/`다. 앱이 따르는 판은 `crates/tasty-design-tokens/dtcg/SOURCE`에 적힌 tasty-page 커밋이다([디자인 변경 워크플로](design-change-workflow.md)).
- tasty-page의 최신(개발) 페이지는 `develop`, 사이트 게시본은 `main`이다.
