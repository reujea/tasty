#!/usr/bin/env bash
# self-hosted 러너의 작업 트리를 잡 시작 때 정리한다. 빌드 캐시(루트 target)는 남기고 나머지 미추적·무시 파일은 지운다.
# checkout을 clean: false로 부른 바로 뒤에 실행한다. 러너 하나는 한 번에 잡 하나만 돌리므로 이 작업 트리를 쓰는 다른 실행은 없다.
# 사용: scripts/ci-prepare-workspace.sh
#       scripts/ci-prepare-workspace.sh --decide <오늘> <표지|-> <target 있음 0|1> <여유 KiB> <임계 KiB>
#   --decide  판정만 하고 "<action> <reason>"을 출력한다. 시험이 df 없이 경계값을 고정할 때 쓴다.
# 환경: TASTY_CI_CACHE_MIN_FREE_GB  빌드 캐시 볼륨의 여유가 이 값(GB = 10^9 바이트, 10진 정수) 이하면 target을 지운다. 기본 150.
# 종료 코드: 0 완료, 1 삭제 실패(target 삭제 실패면 표지는 지워진 채 남는다), 2 사용법·환경 오류(target이 심볼릭 링크인 경우 포함).
#   표지는 git 정리 전에 기록한다. git reset·clean이 실패하면 git의 종료 코드로 끝나고 표지는 기록된 채로 남는다.
# 기준: docs/dev-guide/ci-gates.md#러너의-빌드-캐시. Windows는 Git Bash에서 실행한다.
set -euo pipefail

marker_name=".ci-cache-day"
default_min_free_gb=150

# 출력: "<wipe|keep|none> <reason>"
decide() {
    local today="$1" marker="$2" target_exists="$3" free_kib="$4" threshold_kib="$5"
    if [[ "$target_exists" != 1 ]]; then
        echo "none target-absent"
    elif [[ -z "$marker" ]]; then
        echo "wipe no-marker"
    elif [[ "$marker" != "$today" ]]; then
        echo "wipe first-of-day"
    elif (( 10#$free_kib <= 10#$threshold_kib )); then
        echo "wipe low-disk"
    else
        echo "keep cached"
    fi
}

# 10진 정수만 받는다. 자릿수 상한으로 64비트 산술의 넘침을 막고, 쓰는 쪽은 10#로 8진수 해석을 막는다.
is_uint() {
    [[ "$1" =~ ^[0-9]{1,${2:-18}}$ ]]
}

if [[ "${1:-}" == "--decide" ]]; then
    if [[ $# -ne 6 ]] || [[ "$4" != 0 && "$4" != 1 ]] || ! is_uint "$5" || ! is_uint "$6"; then
        echo "Error: --decide <today> <marker|-> <target_exists 0|1> <free_kib> <threshold_kib>" >&2
        exit 2
    fi
    m="$3"
    [[ "$m" != "-" ]] || m=""
    decide "$2" "$m" "$4" "$5" "$6"
    exit 0
fi
if [[ $# -ne 0 ]]; then
    sed -n '4,9p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit 2
fi

min_free_gb="${TASTY_CI_CACHE_MIN_FREE_GB:-$default_min_free_gb}"
if ! is_uint "$min_free_gb" 9; then
    echo "Error: TASTY_CI_CACHE_MIN_FREE_GB must be an integer from 0 to 999999999 (GB): $min_free_gb" >&2
    exit 2
fi
# GB를 KiB로 바꾼다(올림). df -Pk의 Available과 같은 단위로 비교한다.
min_free_gb=$(( 10#$min_free_gb ))
threshold_kib=$(( (min_free_gb * 1000000000 + 1023) / 1024 ))

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# target이 링크면 표지·삭제가 작업 트리 밖을 가리킨다. 이 스크립트는 링크를 따라가지 않는다.
if [[ -L target ]]; then
    echo "Error: $root/target is a symbolic link; the build cache must be a real directory" >&2
    exit 2
fi

# 빌드 캐시(target)가 놓인 볼륨의 Available(KiB). target이 없으면 작업 트리 볼륨을 본다.
# target은 지울 수 있는 일반 디렉터리여야 한다(마운트 지점이면 rm -rf가 EBUSY로 실패한다).
# df -P의 데이터 행은 "<파일시스템> <전체> <사용> <여유> <비율%> <마운트>"이고 파일시스템·마운트 이름에 공백이 있을 수
# 있다. 그래서 비율 칸 바로 앞의 세 숫자를 찾아 여유를 읽는다. 여유는 음수일 수 있고 그때는 0(여유 없음)으로 본다.
free_kib() {
    local out line dir="$root"
    if [[ -d "$root/target" ]]; then dir="$root/target"; fi
    out="$(df -Pk "$dir")" || return 1
    line="${out##*$'\n'}"
    [[ "$line" =~ [[:space:]](-?[0-9]+)[[:space:]]+(-?[0-9]+)[[:space:]]+(-?[0-9]+)[[:space:]]+([0-9]+%|-)[[:space:]] ]] || return 1
    local avail="${BASH_REMATCH[3]}"
    if [[ "$avail" == -* ]]; then
        avail=0
    fi
    is_uint "$avail" || return 1
    echo "$(( 10#$avail ))"
}

if ! free_before="$(free_kib)"; then
    echo "Error: could not read free space of the build cache volume under $root with df -Pk" >&2
    exit 2
fi

today="$(date +%F)"
marker=""
target_exists=0
if [[ -e target ]]; then
    target_exists=1
    # 링크인 표지는 작업 트리 밖의 내용일 수 있으므로 표지로 인정하지 않는다(no-marker로 지운다).
    if [[ -f "target/$marker_name" && ! -L "target/$marker_name" ]]; then
        IFS= read -r marker < "target/$marker_name" || true
    fi
fi

read -r action reason <<<"$(decide "$today" "$marker" "$target_exists" "$free_before" "$threshold_kib")"

if [[ "$action" == wipe ]]; then
    # 표지부터 지운다. 재귀 삭제가 중간에 실패해도 오늘 표지가 남아 다음 잡이 덜 지운 캐시를 쓰는 일이 없다.
    if ! rm -f "target/$marker_name"; then
        echo "Error: could not remove the day marker target/$marker_name (reason $reason)" >&2
        exit 1
    fi
    if ! rm -rf target; then
        echo "Error: could not remove target (reason $reason). The day marker was removed, so the next job wipes again." >&2
        exit 1
    fi
fi
if [[ "$action" != keep ]]; then
    mkdir -p target
    rm -f "target/$marker_name"
    printf '%s\n' "$today" > "target/$marker_name"
fi

# clean: false는 checkout의 reset --hard와 git clean을 끈다. 루트 target만 빼고 둘 다 되살린다.
# crates/ 아래에 무시 파일만 남은 폴더도 여기서 지워져 cargo workspace가 깨지지 않는다.
git reset --hard -q HEAD
# Windows Git Bash는 /로 시작하는 인자를 Windows 경로로 바꿔 git에 넘긴다. 이 호출만 변환을 끈다.
MSYS2_ARG_CONV_EXCL='*' git clean -ffdxq -e /target/

# cargo가 관리하지 않는 target 안의 상태는 남은 캐시에서도 처음부터 빌드한 실행과 같게 비운다.
# 만드는 곳: scripts/stage-dev-plugins.sh(잠금·스테이징·번들), scripts/sync-official-plugin-fixtures.sh(대조 폴더·잠금),
# tests/spawn_diag/mod.rs의 BUNDLE_LINK_CACHE_DIR. 받기 캐시 target/official-plugins는 SHA-256 대조 뒤에만 쓰므로 남긴다.
# workspace 원천 번들 플러그인(crates/tasty-host-plugin/src/builtin.rs의 BuiltinSource::Workspace, 지금은 mesh-demo)의
# 실행 파일도 지운다. debug 호스트가 실행 파일 옆의 이 파일로 번들을 다시 만들기 때문이다. Cargo는 지워진 실행 파일을 다음 빌드에서 다시 놓는다.
if [[ "$action" == keep ]]; then
    shopt -s nullglob
    leftovers=(target/official-plugin-fixtures-sync target/official-plugin-fixtures-sync.lock*)
    for dir in target/*/ target/*/*/; do
        # 링크인 프로필·타깃 폴더는 작업 트리 밖일 수 있으므로 들어가지 않는다.
        d="${dir%/}"
        if [[ -L "$d" || -L "${d%/*}" ]]; then
            continue
        fi
        leftovers+=(
            "${dir}builtin-plugins"
            "${dir}builtin-plugins.lock"*
            "${dir}.official-plugins-stage."*
            "${dir}.mesh-demo-stage."*
            "${dir}test-bundle-links"
            "${dir}tasty-plugin-mesh-demo"
            "${dir}tasty-plugin-mesh-demo.exe"
        )
    done
    shopt -u nullglob
    for p in "${leftovers[@]}"; do
        if [[ -e "$p" || -L "$p" ]] && ! rm -rf "$p"; then
            echo "Error: could not remove $p" >&2
            exit 1
        fi
    done
fi

# KiB를 GB(10^9 바이트, 버림)로 바꾼다. 1024/10^9 = 128/125000000이고, 몫과 나머지로 나눠 곱셈이 넘치지 않게 한다.
gb() {
    local k=$(( 10#$1 ))
    echo $(( k / 125000000 * 128 + k % 125000000 * 128 / 125000000 ))
}
if free_after="$(free_kib)"; then
    free_after_gb="$(gb "$free_after")"
else
    free_after_gb=unknown
fi
echo "[ci-cache] action=$action reason=$reason day=$today marker=${marker:-none}" \
    "free_before_gb=$(gb "$free_before") free_after_gb=$free_after_gb" \
    "threshold_gb=$min_free_gb os=$(uname -s)"
