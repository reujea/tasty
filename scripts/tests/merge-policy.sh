#!/usr/bin/env bash
# merge 커밋 허용 판정(scripts/lib/merge-policy.sh)을 표와 일회용 저장소의 실제 merge로 확인한다.
# 사용: merge-policy.sh <저장소 루트> <비어 있는 임시 폴더>
set -eu -o pipefail
root="$(cd "$1" && pwd -P)"
fixture="$(cd "$2" && pwd -P)"
local_vars="$(git rev-parse --local-env-vars)"
while IFS= read -r name; do
    [ -z "$name" ] || unset "$name"
done <<< "$local_vars"

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

# 2. 실제 merge: 저장소의 pre-merge-commit과 판정 파일로 훅을 실행한다.
repo="$fixture/repo"
mkdir -p "$repo/.githooks" "$repo/scripts/lib"
git -C "$repo" init -q -b main
cp "$root/.githooks/pre-merge-commit" "$repo/.githooks/pre-merge-commit"
cp "$root/scripts/lib/merge-policy.sh" "$repo/scripts/lib/merge-policy.sh"
chmod +x "$repo/.githooks/pre-merge-commit"
git -C "$repo" config core.hooksPath .githooks
git -C "$repo" config user.name fixture
git -C "$repo" config user.email fixture@example.invalid
git -C "$repo" config commit.gpgsign false
git -C "$repo" config merge.ff false
cd "$repo"

commit() {
    printf '%s\n' "$1" > "$1"
    git add "$1"
    git commit -qm "$1"
}
merge() {
    local rc=0
    git merge -q --no-edit "$@" >/dev/null 2>&1 || rc=$?
    if [ "$rc" -ne 0 ] && git rev-parse -q --verify MERGE_HEAD >/dev/null; then
        git merge --abort
    fi
    printf '%s' "$rc"
}

git add .githooks scripts
git commit -qm base
git branch develop
git branch release

git switch -q develop
commit d1
git switch -q -c work/x
commit w1
git switch -q develop
expect 1 "$(merge work/x)" "merge develop ← work/x"
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
git branch -q stale main
git switch -q main
commit h3
git switch -q release
expect 1 "$(merge "$(git rev-parse stale)")" "merge release ← 이름 없는 커밋"

# 3. 충돌한 merge: M.1이 git commit 전에 같은 함수로 판정한다.
judge() {
    local rc=0
    bash -c '. scripts/lib/merge-policy.sh; merge_commit_allowed' >/dev/null 2>&1 || rc=$?
    printf '%s' "$rc"
}
git switch -q develop
printf 'develop\n' > clash
git add clash
git commit -qm clash-develop
git switch -q main
printf 'main\n' > clash
git add clash
git commit -qm clash-main
git switch -q develop
git merge -q --no-edit main >/dev/null 2>&1 || true
expect 0 "$(judge)" "충돌 중 develop ← main"
git merge --abort
git switch -q -c work/y main~1
printf 'work\n' > clash
git add clash
git commit -qm clash-work
git switch -q develop
git merge -q --no-edit work/y >/dev/null 2>&1 || true
expect 1 "$(judge)" "충돌 중 develop ← work/y"
git merge --abort

if [ "$failures" -ne 0 ]; then
    printf '%s 건 실패\n' "$failures" >&2
    exit 1
fi
echo "merge-policy: 모두 통과"
