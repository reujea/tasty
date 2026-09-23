#!/usr/bin/env bash
# 착지 범위(<base>..<tip>)에서 추가·변경·이름이 바뀐 ADR 을 나란히 놓는 **보고 도구**다.
#
# 겹침(같은 조항을 두 ADR 이 결정하는가)은 텍스트로 안 갈린다 — 그래서 이 도구는 겹침을
# 판정하지 않는다. 각 ADR 의 제목과 Decision 절이 백틱으로 인용한 이름(심볼 · 파일 · 설정 키)을
# 뽑고, 같은 이름에 둘 이상 걸린 것을 "겹침 후보" 로 모아 보일 뿐이다. 판정은 착지하는 사람이
# 두 Decision 을 읽고 한다. 절차와 근거: docs/dev-guide/adr-index.md#착지-때-새-adr-끼리-대조 · ADR-0508.
#
# 종료 코드
#   0 — 보고 완료. 겹침 후보가 있든 없든 0 이다(후보는 판정이 아니다).
#   2 — 판정 불가. 없는 rev · 얕은 clone 으로 객체 부재 · git 오류. 목록을 못 뽑은 초록과
#       목록이 빈 초록이 같은 줄로 보이지 않게 이 둘은 갈린다.
#
# 사용: bash scripts/adr-landing-report.sh [--candidates-only] <base> <tip>
#   --candidates-only — ADR 별 목록을 빼고 끝의 요약과 겹침 후보만 찍는다(착지 범위가 클 때).
set -euo pipefail

quiet=0
if [ "${1:-}" = "--candidates-only" ]; then
    quiet=1
    shift
fi
if [ "$#" -ne 2 ]; then
    echo "사용: bash scripts/adr-landing-report.sh [--candidates-only] <base> <tip>" >&2
    exit 2
fi
base=$1
tip=$2

for rev in "$base" "$tip"; do
    if ! git rev-parse --verify --quiet "${rev}^{commit}" > /dev/null; then
        echo "[adr-landing] 판정 불가 — '${rev}' 를 커밋으로 풀지 못했다(없는 rev 이거나 얕은 clone 이면 git fetch)." >&2
        exit 2
    fi
done

if ! changes=$(git diff --name-status --diff-filter=AMR "$base" "$tip" -- 'docs/adr/[0-9]*.md'); then
    echo "[adr-landing] 판정 불가 — git diff 가 실패했다." >&2
    exit 2
fi

if [ -z "$changes" ]; then
    echo "[adr-landing] ${base}..${tip} 에서 추가·변경된 ADR 0 건 — 대조할 것이 없다."
    exit 0
fi

# ADR 번호 → 인용 이름 목록을 모은 뒤 이름 → ADR 로 뒤집는다.
pairs=""
count=0
while IFS=$'\t' read -r status first second; do
    path=$first
    if [ -n "${second:-}" ]; then
        path=$second # R 은 새 경로가 둘째 칸이다
    fi
    num=$(basename "$path")
    num=${num:0:4}
    if ! body=$(git show "${tip}:${path}"); then
        echo "[adr-landing] 판정 불가 — ${tip}:${path} 를 읽지 못했다." >&2
        exit 2
    fi
    title=$(sed -n '1p' <<< "$body")
    count=$((count + 1))
    [ "$quiet" = 1 ] || echo "── ${status:0:1} ${title}"
    decision=$(awk '/^## Decision/{f=1; next} /^## /{f=0} f' <<< "$body")
    # 백틱은 셸 확장이 아니라 grep 이 찾는 글자다 — 작은따옴표가 맞다.
    # shellcheck disable=SC2016
    names=$(grep -o '`[^` ]\{4,\}`' <<< "$decision" | tr -d '`' | sort -u || true)
    if [ -z "$names" ]; then
        [ "$quiet" = 1 ] || echo "   (Decision 절에 백틱 인용 없음 — 제목으로 읽어라)"
        continue
    fi
    while IFS= read -r name; do
        [ "$quiet" = 1 ] || echo "   · ${name}"
        # 겹침 후보는 뿌리 이름으로 모은다 — `PhysicalPx(` · `PhysicalPx::new(24.0)` 은 같은 대상이다.
        root=${name%%(*}
        root=${root%%::*}
        if [ "${#root}" -ge 4 ]; then
            pairs+="${root}"$'\t'"${num}"$'\n'
        fi
    done <<< "$names"
done <<< "$changes"

[ "$quiet" = 1 ] || echo
echo "[adr-landing] 대조 대상 ADR ${count} 건 (${base}..${tip}, 상태 A·M·R)"

candidates=$(printf '%s' "$pairs" | sort -u | awk -F'\t' '
    { list[$1] = (list[$1] == "" ? $2 : list[$1] " " $2); n[$1]++ }
    END { for (k in list) if (n[k] > 1) print k "\t" list[k] }' | sort)

if [ -z "$candidates" ]; then
    echo "[adr-landing] 같은 인용 이름에 둘 이상 걸린 ADR 없음. 제목은 위 목록으로 사람이 대조한다."
    exit 0
fi

echo "[adr-landing] 겹침 후보 — 같은 이름을 Decision 에서 인용한다. 두 Decision 을 읽고 같은 조항인지 판단하라:"
while IFS=$'\t' read -r name nums; do
    echo "   ${name}  ←  ${nums}"
done <<< "$candidates"
exit 0
