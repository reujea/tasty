#!/usr/bin/env bash
# 변경 파일 경로를 직접 언급하는 검사·스크립트를 찾아 실행 후보로 보여 준다.
# 사용: scripts/what-sees-this-change.sh [base] (기본 develop, 로컬 develop이 없으면 origin/develop). 공통 조상부터 작업 트리 변경과 미추적 파일을 포함한다.
# 파일명을 적지 않고 디렉터리를 순회하는 검사, 간접 의존성과 오래된 줄 번호는 찾지 못한다.
# 주석 여부는 줄 앞 표지만 보는 근사이며, 목록에 나왔다고 수정이 필요한 것은 아니다.
# 결과가 없어도 정상 종료 0이며 기준 revision을 찾지 못하면 2다. 검색 오류 전체를 판정하는 도구는 아니다.
set -u

if [ $# -ge 1 ]; then
    BASE=$1
elif git rev-parse --verify --quiet develop >/dev/null; then
    BASE=develop
else
    BASE=origin/develop
fi

if ! git rev-parse --verify --quiet "$BASE" >/dev/null; then
    echo "판정 불가: base '$BASE' 를 못 찾는다." >&2
    exit 2
fi

# 다른 브랜치에만 추가된 변경을 내 변경으로 세지 않도록 공통 조상을 사용한다.
MERGE_BASE=$(git merge-base HEAD "$BASE" 2>/dev/null || true)
if [ -z "$MERGE_BASE" ]; then
    echo "판정 불가: HEAD 와 '$BASE' 의 공통 조상을 못 찾는다." >&2
    exit 2
fi
BASE_SHOWN=$BASE
if [ "$(git rev-parse "$BASE")" != "$MERGE_BASE" ]; then
    ahead=$(git rev-list --count "${MERGE_BASE}..${BASE}")
    BASE_SHOWN="$(git rev-parse --short=9 "$MERGE_BASE")  ('$BASE'가 ${ahead} 커밋 앞서
            있어 다른 변경을 제외하도록 공통 조상을 사용한다)"
fi
BASE=$MERGE_BASE

# git diff에 빠지는 미추적 파일도 조사 대상에 넣는다.
mapfile -t FILES < <(
    {
        git diff --name-only "$BASE"
        git ls-files --others --exclude-standard
    } | sort -u
)

# 검사·스크립트 목록에도 미추적 파일을 포함한다. 이 목록 밖의 검사는 찾지 못한다.
mapfile -t CORPUS < <(
    {
        git ls-files 'tests/*.rs' 'tests/**/*.rs' \
            'crates/*/tests/*.rs' 'crates/*/tests/**/*.rs' \
            'scripts/*.sh' '.githooks/*' 2>/dev/null
        find tests crates/*/tests scripts .githooks \
            \( -name '*.rs' -o -name '*.sh' -o -path '.githooks/*' \) \
            -type f 2>/dev/null
    } | sort -u
)

echo "발견 입력   git diff --name-only ${BASE_SHOWN} + 미추적분   (작업 트리 포함)"
echo "조사 범위   변경 파일 ${#FILES[@]} · 검사·스크립트 파일 ${#CORPUS[@]}"
echo

if [ "${#FILES[@]}" -eq 0 ]; then
    echo "변경 파일이 없다. 비교하려던 base가 맞는지 확인해라."
    exit 0
fi

mentioned=0
declare -A TARGETS=()

for f in "${FILES[@]}"; do
    hits=$(grep -l -F -- "$f" "${CORPUS[@]}" 2>/dev/null)
    if [ -z "$hits" ]; then
        continue
    fi
    mentioned=$((mentioned + 1))
    echo "  $f"
    while IFS= read -r g; do
        [ -n "$g" ] || continue
        TARGETS["$g"]=1
        lines=$(grep -n -F -- "$f" "$g" 2>/dev/null)
        total=$(printf '%s\n' "$lines" | grep -c .)
        # 여러 줄 문자열과 실제 주석을 구별하지 못하는 근사다.
        cmt=$(printf '%s\n' "$lines" | grep -cE '^[0-9]+:[[:space:]]*(//|#)')
        printf '      %-58s 줄 %-3s (주석 근사 %s / 그 밖 %s)\n' \
            "$g" "$total" "$cmt" "$((total - cmt))"
    done <<<"$hits"
done

echo
echo "변경 파일 ${#FILES[@]} · 리터럴로 언급된 것 ${mentioned} · 대응 타깃 ${#TARGETS[@]}"
echo "이 목록은 전체 검사 목록이 아니다. 파일명을 직접 언급하지 않는 순회 검사는 빠질 수 있다."
exit 0
