# merge 커밋을 만들어도 되는지 정한다. pre-merge-commit 과 pre-commit 의 M.1 이 같이 쓴다.
# 허용 범위: docs/dev-guide/git-hooks.md#merge-커밋-판정
# shellcheck shell=bash

# 장기 브랜치. 합치는 브랜치의 이름은 이 셋 중에서만 찾는다.
MERGE_POLICY_BRANCHES="develop release main"

# 현재 브랜치 $1 에 브랜치 $2 를 merge 커밋으로 합쳐도 되면 0, 아니면 1.
# 역병합(main→release, main→develop, release→develop)과 승격(develop→release, release→main)만 허용한다.
merge_pair_allowed() {
    case "$1:$2" in
        release:main | develop:main | develop:release) return 0 ;;
        release:develop | main:release) return 0 ;;
    esac
    return 1
}

# 합치는 커밋을 한 줄에 하나씩 쓴다.
# 충돌을 해결하고 git commit 할 때(M.1)는 MERGE_HEAD 파일에 있다. pre-merge-commit 이 실행될 때는
# git 이 MERGE_HEAD 를 아직 쓰지 않았으므로, git merge 가 합치는 커밋마다 넘기는
# GITHEAD_<커밋> 환경 변수에서 읽는다.
merge_heads() {
    local file name
    file=$(git rev-parse --git-path MERGE_HEAD)
    if [ -s "$file" ]; then
        grep -E '^[0-9a-f]{40,64}$' "$file" || true
        return 0
    fi
    while IFS= read -r name; do
        case "$name" in
            GITHEAD_*) printf '%s\n' "${name#GITHEAD_}" ;;
        esac
    done <<< "$(compgen -e)"
    return 0
}

# 커밋 $1 을 가리키는 장기 브랜치 이름을 한 줄에 하나씩 쓴다.
# 로컬 브랜치와 origin 의 원격 추적 브랜치를 함께 본다.
branches_at() {
    local head b ref sha
    head=$(git rev-parse -q --verify "$1^{commit}" 2>/dev/null) || return 0
    for b in $MERGE_POLICY_BRANCHES; do
        for ref in "refs/heads/$b" "refs/remotes/origin/$b"; do
            sha=$(git rev-parse -q --verify "$ref^{commit}" 2>/dev/null) || continue
            if [ "$sha" = "$head" ]; then
                printf '%s\n' "$b"
                break
            fi
        done
    done
    return 0
}

# 진행 중인 merge 를 커밋해도 되면 0, 아니면 1. 판정 근거는 stderr 로 알린다.
merge_commit_allowed() {
    local current heads count src found=""
    if ! current=$(git symbolic-ref -q --short HEAD 2>/dev/null); then
        printf '[merge] 현재 브랜치가 없다(detached HEAD). merge 커밋은 장기 브랜치 위에서만 만든다.\n' >&2
        return 1
    fi
    heads=$(merge_heads)
    count=$(grep -c . <<< "$heads" || true)
    if [ "$count" -eq 0 ]; then
        printf '[merge] 합치는 커밋을 찾지 못했다.\n' >&2
        return 1
    fi
    if [ "$count" -gt 1 ]; then
        printf '[merge] 커밋 %s 개를 한 번에 합치는 merge 는 허용하지 않는다.\n' "$count" >&2
        return 1
    fi
    while IFS= read -r src; do
        [ -n "$src" ] || continue
        found="$found $src"
        if merge_pair_allowed "$current" "$src"; then
            printf '[merge] %s ← %s merge 커밋을 허용한다.\n' "$current" "$src" >&2
            return 0
        fi
    done <<< "$(branches_at "$heads")"
    if [ -n "$found" ]; then
        printf '[merge] %s ←%s 는 허용한 역병합·승격이 아니다.\n' "$current" "$found" >&2
    else
        printf '[merge] 합치는 커밋을 가리키는 develop·release·main 브랜치가 없다(현재 브랜치 %s).\n' "$current" >&2
    fi
    return 1
}
