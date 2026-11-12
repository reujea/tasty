#!/usr/bin/env bash
# merge 커밋 허용 판정(scripts/lib/merge-policy.sh)을 표와 일회용 저장소의 실제 merge로 확인한다.
# 일회용 저장소에는 이 저장소의 pre-commit·pre-merge-commit과 판정 파일을 설치한다.
# pre-commit의 다른 검사가 부르는 cargo와 검사 스크립트는 항상 성공하는 대역으로 바꾼다.
# 사용: merge-policy.sh <저장소 루트> <비어 있는 임시 폴더>
set -eu -o pipefail
root="$(cd "$1" && pwd -P)"
fixture="$(cd "$2" && pwd -P)"
local_vars="$(git rev-parse --local-env-vars)"
while IFS= read -r name; do
    [ -z "$name" ] || unset "$name"
done <<< "$local_vars"
# 바깥 merge 가 넘긴 값이 남아 판정을 바꾸지 않게 한다.
while IFS= read -r name; do
    case "$name" in GITHEAD_*) unset "$name" ;; esac
done <<< "$(compgen -e)"

failures=0
expect() {
    local want="$1" got="$2" what="$3"
    if [ "$want" = "$got" ]; then
        printf 'ok   %s (rc=%s)\n' "$what" "$got"
    else
        printf 'FAIL %s: rc=%s, 기대 %s\n' "$what" "$got" "$want" >&2
        failures=$((failures + 1))
    fi
}

# 1. 판정 표: 허용 쌍 다섯 개 말고는 모두 거부한다.
# shellcheck source=../lib/merge-policy.sh
. "$root/scripts/lib/merge-policy.sh"
allowed=" release:main develop:main develop:release release:develop main:release "
for current in develop release main work/x conductor/wt-1; do
    for source in develop release main work/x conductor/wt-1; do
        want=1
        case "$allowed" in *" $current:$source "*) want=0 ;; esac
        got=0
        merge_pair_allowed "$current" "$source" || got=$?
        expect "$want" "$got" "표 $current ← $source"
    done
done

# 2. 일회용 저장소. 훅을 설치하고, pre-commit 의 다른 검사가 부르는 도구는 대역으로 둔다.
repo="$fixture/repo"
mkdir -p "$repo/.githooks" "$repo/scripts/lib" "$fixture/bin"
git -C "$repo" init -q -b main
cp "$root/.githooks/pre-commit" "$root/.githooks/pre-merge-commit" "$repo/.githooks/"
cp "$root/scripts/lib/merge-policy.sh" "$repo/scripts/lib/merge-policy.sh"
chmod +x "$repo/.githooks/pre-commit" "$repo/.githooks/pre-merge-commit"
for stub in check-shell-assets.sh check-plugin-version-bump.sh; do
    printf '#!/usr/bin/env bash\nexit 0\n' > "$repo/scripts/$stub"
done
printf '#!/usr/bin/env bash\nexit 0\n' > "$fixture/bin/cargo"
chmod +x "$fixture/bin/cargo"
export PATH="$fixture/bin:$PATH"
git -C "$repo" config core.hooksPath .githooks
git -C "$repo" config user.name fixture
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" config commit.gpgsign false
git -C "$repo" config merge.ff false
cd "$repo"

commit() {
    printf '%s\n' "$1" > "$1"
    git add "$1"
    git commit -qm "$1" >/dev/null 2>&1
}
merge() {
    local rc=0
    git merge -q --no-edit "$@" >/dev/null 2>&1 || rc=$?
    if [ "$rc" -ne 0 ] && git rev-parse -q --verify MERGE_HEAD >/dev/null; then
        git merge --abort
    fi
    printf '%s' "$rc"
}
judge() {
    local rc=0
    bash -c '. scripts/lib/merge-policy.sh; merge_commit_allowed' >/dev/null 2>&1 || rc=$?
    printf '%s' "$rc"
}

git add .githooks scripts
git commit -qm base >/dev/null 2>&1
git branch develop
git branch release

# (f) merge 가 아닌 커밋은 실제 pre-commit 을 통과한다. (e) merge 상태가 아니면 판정은 거부한다.
git switch -q develop
rc=0; commit d1 || rc=$?
expect 0 "$rc" "pre-commit: merge 가 아닌 커밋"
expect 1 "$(judge)" "MERGE_HEAD·GITHEAD 없음"

# (c) 작업 브랜치 tip 은 어느 장기 브랜치에도 없으므로 거부한다.
git switch -q -c work/x
commit w1
git switch -q develop
expect 1 "$(merge work/x)" "merge develop ← work/x"
expect 1 "$(merge "$(git rev-parse work/x)")" "merge develop ← work/x 해시"
expect 0 "$(merge --ff-only work/x)" "fast-forward develop ← work/x"

git switch -q main
commit h1
git switch -q develop
expect 0 "$(merge main)" "역병합 develop ← main"
git switch -q release
commit r1
expect 0 "$(merge main)" "역병합 release ← main"
git switch -q develop
expect 0 "$(merge release)" "역병합 develop ← release"
git switch -q release
commit r2
expect 0 "$(merge develop)" "승격 release ← develop"
git switch -q main
expect 0 "$(merge --ff-only release)" "fast-forward main ← release (hotfix 역병합 뒤)"
git switch -q release
commit r3
git switch -q main
commit h2
expect 0 "$(merge release)" "승격 main ← release"
git switch -q develop
commit d2
git switch -q main
expect 1 "$(merge develop)" "merge main ← develop"
git switch -q --detach release
expect 1 "$(merge develop)" "merge detached ← develop"
git switch -q -c work/z develop
commit z1
git switch -q develop
commit d3
expect 1 "$(merge main work/z)" "octopus develop ← main, work/z"

# (a) develop 의 과거 시점을 release 로 비-FF 승격한다.
git switch -q develop
commit d4
commit d5
git switch -q release
commit r4
expect 0 "$(merge develop~1)" "승격 release ← develop 과거 시점"
# (b) release 의 과거 시점을 main 으로 합친다.
commit r5
git switch -q main
commit h3
expect 0 "$(merge release~1)" "승격 main ← release 과거 시점"
# (d) develop 과 release 둘에 있는 커밋은 main 으로 허용, develop 에만 있는 커밋은 거부.
git switch -q develop
commit d6
git switch -q release
git merge -q --no-edit develop >/dev/null 2>&1
git switch -q develop
commit d7
git switch -q main
expect 0 "$(merge develop~1)" "main ← develop·release 둘에 있는 커밋"
expect 1 "$(merge develop)" "main ← develop 에만 있는 커밋"
# 로컬 장기 브랜치가 없으면 origin 의 원격 추적 브랜치를 쓴다.
git switch -q release
commit r6
git update-ref refs/remotes/origin/main "$(git rev-parse main)"
git switch -q main
commit h4
git switch -q release
git branch -q -D main
expect 0 "$(merge origin/main)" "역병합 release ← origin/main (로컬 main 없음)"
git branch -q main origin/main

# (g) 충돌한 merge 는 git commit 의 실제 pre-commit(M.1)이 판정한다.
conflict_commit() {
    local rc=0
    git merge -q --no-edit "$1" >/dev/null 2>&1 || true
    printf 'resolved\n' > clash
    git add clash
    git commit -q --no-edit >/dev/null 2>&1 || rc=$?
    if git rev-parse -q --verify MERGE_HEAD >/dev/null; then
        git merge --abort
    fi
    printf '%s' "$rc"
}
git switch -q develop
printf 'develop\n' > clash
git add clash
git commit -qm clash-develop >/dev/null 2>&1
git switch -q main
printf 'main\n' > clash
git add clash
git commit -qm clash-main >/dev/null 2>&1
git switch -q -c work/y develop~1
printf 'work\n' > clash
git add clash
git commit -qm clash-work >/dev/null 2>&1
git switch -q develop
expect 1 "$(conflict_commit work/y)" "pre-commit: 충돌 해결 merge develop ← work/y"
expect 0 "$(conflict_commit main)" "pre-commit: 충돌 해결 역병합 develop ← main"

if [ "$failures" -ne 0 ]; then
    printf '%s 건 실패\n' "$failures" >&2
    exit 1
fi
echo "merge-policy: 모두 통과"
