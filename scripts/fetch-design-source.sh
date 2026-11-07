#!/usr/bin/env bash
# 앱이 따르는 디자인 정본(tasty-page)을 crates/tasty-design-tokens/dtcg/SOURCE의 커밋으로 받아
# 그 경로를 출력한다. design_source_* 문서 검사에 TASTY_PAGE_DIR로 넘긴다.
#
#   export TASTY_PAGE_DIR="$(bash scripts/fetch-design-source.sh)"
#   bash scripts/fetch-design-source.sh <디렉터리>   # 받을 위치를 정할 때
#
# 기본 위치는 공용 .git 아래 design-source/<SOURCE 커밋>이다. 저장소 작업 트리 밖이라 순회 검사가
# 읽지 않고, 커밋마다 디렉터리가 달라 SOURCE가 다른 worktree가 동시에 돌아도 서로 체크아웃을 바꾸지
# 않는다. 이미 그 커밋이면 다시 받지 않는다.
# 받은 저장소에는 core.autocrlf=false, core.eol=lf를 둔다. tasty-page에는 .gitattributes가 없어서
# 시스템 설정이 autocrlf=true인 Windows에서는 CRLF로 체크아웃되고, 앱 JSON과의 바이트 비교가 깨진다.
# 성공하면 0, 실패하면 0이 아닌 값으로 끝나고 경로를 출력하지 않는다.

set -e -o pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
COMMON_DIR="$(git rev-parse --path-format=absolute --git-common-dir)"
# 훅이 넘긴 저장소 지정 변수가 남으면 아래 git -C 가 이 저장소를 가리킨다.
while IFS= read -r name; do
    [ -z "$name" ] || unset "$name"
done <<< "$(git rev-parse --local-env-vars)"

commit="$(tr -d '[:space:]' < "$REPO_ROOT/crates/tasty-design-tokens/dtcg/SOURCE")"
case "$commit" in
    *[!0-9a-f]* | "") echo "[design-source] SOURCE 값이 커밋 해시가 아니다: $commit" >&2; exit 1 ;;
esac
[ "${#commit}" -eq 40 ] || { echo "[design-source] SOURCE 해시가 40자가 아니다: $commit" >&2; exit 1; }

dir="${1:-$COMMON_DIR/design-source/$commit}"
head="$(git -C "$dir" rev-parse HEAD 2>/dev/null || true)"
autocrlf="$(git -C "$dir" config --local --get core.autocrlf 2>/dev/null || true)"
if [ "$head" != "$commit" ] || [ "$autocrlf" != false ]; then
    git init -q "$dir"
    git -C "$dir" config core.autocrlf false
    git -C "$dir" config core.eol lf
    if [ "$head" != "$commit" ]; then
        git -C "$dir" fetch -q --depth 1 https://github.com/zilhak/tasty-page.git "$commit"
        git -C "$dir" checkout -q -f --detach FETCH_HEAD
    else
        # 옛 설정으로 줄 끝을 바꿔 쓴 파일을 지금 설정으로 다시 쓴다.
        git -C "$dir" rm -q -r --cached .
        git -C "$dir" reset -q --hard
    fi
fi
printf '%s\n' "$dir"
