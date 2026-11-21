#!/usr/bin/env bash
# 호스트 시험 픽스처(crates/tasty-test-support/fixtures/official-plugins/<id>/)를 잠금 파일의 릴리스 산출물과 맞춘다.
# 픽스처는 산출물의 tasty-plugin.toml과 lang/ 원문이다. 받기와 SHA-256 확인은 fetch-official-plugins.sh가 한다.
# 사용: scripts/sync-official-plugin-fixtures.sh --update|--check [--shared-cache]
#   --update        받은 산출물로 픽스처를 다시 쓴다. 잠금 파일을 갱신한 직후 실행하고 둘을 같은 커밋에 넣는다.
#   --check         받은 산출물과 픽스처를 바이트로 대조한다.
#   --shared-cache  fetch 기본 캐시의 파일을 이 실행 전용 캐시로 복사한 뒤 받는다. 같은 체크아웃에서
#                   stage-dev-plugins.sh 바로 뒤에 돌 때 다시 받지 않는다. fetch는 늘 전용 캐시만 검증하고 풀므로
#                   그 사이 기본 캐시가 바뀌어도 검증한 바이트와 푼 바이트가 같다. 복사 중 바뀐 파일은 해시가 달라 다시 받는다.
# 종료 코드: 0 일치·갱신 완료, 1 불일치·갱신 실패, 2 사용법·환경 오류(받기 실패 포함).
# 필요한 것: bash, awk, cmp, cp, find, grep, mktemp와 fetch-official-plugins.sh가 요구하는 도구. Windows는 Git Bash에서 실행한다.
set -euo pipefail

usage() {
    sed -n '4,8p' "$0" | sed 's/^# \{0,1\}//' >&2
    exit "${1:-2}"
}

root="$(cd "$(dirname "$0")/.." && pwd)"
fixtures_rel="crates/tasty-test-support/fixtures/official-plugins"
fixtures="$root/$fixtures_rel"
lock_file="$root/official-plugins.lock.toml"
scan_awk="$root/scripts/lib/toml-scan.awk"
# 후보는 target 아래에서 만들고 대조한다(재귀 품질 가드가 target을 빌드 캐시로 건너뛴다). 들일 때는 픽스처 폴더와 같은 부모 아래의
# 새 점 폴더로 복사해 이름만 바꾼다. 점 폴더는 재귀 가드가 건너뛰고 끝나면 지운다.
work_root="$root/target/official-plugin-fixtures-sync"
lock_dir="$work_root.lock"

mode=""
shared_cache=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --update|--check)
            [[ -z "$mode" ]] || usage 2
            mode="${1#--}"
            shift
            ;;
        --shared-cache)
            shared_cache=1
            shift
            ;;
        -h|--help)
            usage 0
            ;;
        *)
            echo "Error: unknown argument: $1" >&2
            usage 2
            ;;
    esac
done
[[ -n "$mode" ]] || usage 2
if [[ ! -f "$lock_file" ]]; then
    echo "Error: $lock_file not found" >&2
    exit 2
fi

# stage-dev-plugins.sh의 같은 함수와 같은 판정이다.
host_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin)
            case "$arch" in
                arm64|aarch64) echo macos-arm64 ;;
                *) return 1 ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) echo linux-x64 ;;
                aarch64|arm64) echo linux-arm64 ;;
                *) return 1 ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            case "${PROCESSOR_ARCHITEW6432:-${PROCESSOR_ARCHITECTURE:-}}" in
                ARM64) echo windows-arm64 ;;
                AMD64|x86_64|"") echo windows-x64 ;;
                *) return 1 ;;
            esac
            ;;
        *) return 1 ;;
    esac
}
if ! platform="$(host_platform)"; then
    echo "Error: no official plugin platform for $(uname -s) $(uname -m)" >&2
    exit 2
fi

work=""
lock_held=0
backup=""
side=""
installed=0

# 들이기 전에 끝나면 옮겨 둔 기존 픽스처를 되돌린다. 들였으면 백업을 버린다.
cleanup() {
    local rc=$? keep_side=0
    trap - EXIT INT TERM
    if [[ -n "$backup" && -e "$backup" && "$installed" -eq 0 ]]; then
        if [[ ! -e "$fixtures" ]] && mv "$backup" "$fixtures"; then
            echo "Restored the previous fixtures in $fixtures_rel" >&2
        else
            echo "Error: could not restore $fixtures_rel. The previous fixtures are at $backup." >&2
            echo "  Move them back by hand before removing $side." >&2
            keep_side=1
            rc=1
        fi
    fi
    if [[ -n "$side" && "$keep_side" -eq 0 ]] && ! rm -rf "$side"; then
        echo "Warning: could not remove $side" >&2
    fi
    if [[ -n "$work" ]] && ! rm -rf "$work"; then
        echo "Warning: could not remove $work" >&2
    fi
    if [[ "$lock_held" -eq 1 ]] && ! rmdir "$lock_dir"; then
        echo "Warning: could not remove $lock_dir" >&2
    fi
    exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$work_root"
if ! mkdir "$lock_dir" 2>/dev/null; then
    if [[ -d "$lock_dir" ]]; then
        echo "Error: another run is syncing the fixtures ($lock_dir exists)." >&2
        echo "  Wait for it to finish and run this again. If no run is in progress, remove $lock_dir." >&2
    else
        echo "Error: could not create $lock_dir" >&2
    fi
    exit 2
fi
lock_held=1
work="$(mktemp -d "$work_root/run.XXXXXX")"

# 받기와 대조가 같은 잠금 파일을 보도록 사본을 쓴다.
cp "$lock_file" "$work/lock.toml"
if ! awk -f "$scan_awk" "$work/lock.toml" > "$work/lock.scan"; then
    echo "Error: could not read official-plugins.lock.toml: $(sed -n '$p' "$work/lock.scan")" >&2
    exit 2
fi
# 잠금 항목의 id·version·bundle을 탭으로 나열한다. 잠금 파일은 generate가 쓰는 형식만 받는다:
# 표는 [release]·[[plugin]]·[[plugin.artifact]]의 맨 이름이고, [[plugin]]마다 맨 키 id(기본 문자열)와
# version(기본 문자열)·bundle(불 값)이 하나씩 있다. id는 checker와 같은 공식 id 꼴이고 겹치지 않는다. 잠금 파일에는 bundle 기본값이 없다(official-plugins check가 거부한다).
if ! awk -F'\t' '
    function fail(m) { if (err == "") err = m }
    $1 == "H" {
        if ($3 != "bare" || ($2 != "[release]" && $2 != "[[plugin]]" && $2 != "[[plugin.artifact]]")) fail("unexpected table " $2)
        if ($2 == "[release]" && releases++) fail("[release] table appears twice")
        inp = ($2 == "[[plugin]]"); if (inp) n++
        if ($2 == "[[plugin.artifact]]" && n == 0) fail("[[plugin.artifact]] before any [[plugin]] table")
        next
    }
    $1 == "K" && inp && $2 == "unknown" { fail("unreadable key in a [[plugin]] table") }
    $1 == "K" && inp && $3 == "id" {
        if ($2 != "bare" || $4 != "basic" || (n in id)) fail("unexpected id in plugin " n)
        if ($5 !~ /^com[.]tasty[.][a-z0-9-]+$/) fail("id is not an official plugin id: " $5)
        if (ids[$5]++) fail("duplicate id " $5)
        id[n] = $5
    }
    $1 == "K" && inp && $3 == "version" { if ($2 != "bare" || $4 != "basic" || (n in v)) fail("unexpected version in plugin " n); v[n] = $5 }
    $1 == "K" && inp && $3 == "bundle" { if ($2 != "bare" || $4 != "bool" || (n in b)) fail("unexpected bundle in plugin " n); b[n] = $5 }
    END {
        if (n == 0) fail("no [[plugin]] table")
        for (i = 1; i <= n; i++) if (!(i in id) || !(i in v) || !(i in b)) fail("plugin " i " lacks id, version or bundle")
        if (err != "") { print err > "/dev/stderr"; exit 3 }
        for (i = 1; i <= n; i++) print id[i] "\t" v[i] "\t" b[i]
    }
' "$work/lock.scan" > "$work/lock.entries"; then
    echo "Error: could not read the plugin entries of official-plugins.lock.toml" >&2
    exit 2
fi
mkdir "$work/out" "$work/cache"
if [[ "$shared_cache" -eq 1 ]]; then
    tag="$(awk -F'\t' '$1 == "H" { rel = ($2 == "[release]"); next } $1 == "K" && rel && $3 == "tag" { print $5 }' "$work/lock.scan")"
    shared="${CARGO_TARGET_DIR:-$root/target}/official-plugins/$tag/$platform"
    if [[ -n "$tag" && -d "$shared" ]]; then
        find "$shared" -maxdepth 1 -type f ! -name '*.part' -exec cp {} "$work/cache/" \;
    fi
fi
fetch_args=(--platform "$platform" --lock "$work/lock.toml" --all --out "$work/out" --cache "$work/cache")
if ! bash "$root/scripts/fetch-official-plugins.sh" "${fetch_args[@]}"; then
    echo "Error: fetching the locked official plugin artifacts failed" >&2
    exit 2
fi

ids=()
for dir in "$work/out"/*/; do
    [[ -d "$dir" ]] || continue
    ids+=("$(basename "$dir")")
done
if [[ ${#ids[@]} -eq 0 ]]; then
    echo "Error: the fetch staged no official plugin" >&2
    exit 2
fi

is_locked_id() {
    local id
    for id in "${ids[@]}"; do
        [[ "$id" == "$1" ]] && return 0
    done
    return 1
}

# 한 플러그인 폴더에서 픽스처가 담는 파일(매니페스트와 lang/ 아래 전부)을 상대 경로로 나열한다.
fixture_files() {
    (
        cd "$1"
        if [[ -f tasty-plugin.toml ]]; then
            echo tasty-plugin.toml
        fi
        if [[ -d lang ]]; then
            find lang -type f
        fi
    ) | LC_ALL=C sort
}

# 받은 폴더(<out root>)와 픽스처 루트를 대조한다. 다른 점을 모두 적고 하나라도 있으면 1.
compare() {
    local out="$1" fix="$2" bad=0 id want have name dir
    # 링크는 읽기 전에 거부한다. 루트나 항목이 링크면 링크 너머의 파일로 대조하게 된다.
    if [[ -L "$fix" ]]; then
        echo "  the fixture root is a symbolic link: $fix" >&2
        return 1
    fi
    for id in "${ids[@]}"; do
        if [[ -L "$fix/$id" ]]; then
            echo "  symbolic link in the fixture root: $id" >&2
            bad=1
            continue
        fi
        if [[ ! -d "$fix/$id" ]]; then
            echo "  missing fixture: $id" >&2
            bad=1
            continue
        fi
        want="$(fixture_files "$out/$id")"
        have="$(cd "$fix/$id" && find . -type f | sed 's|^\./||' | LC_ALL=C sort)"
        if [[ "$want" != "$have" ]]; then
            echo "  file set differs: $id" >&2
            bad=1
        fi
        while IFS= read -r name; do
            [[ -n "$name" ]] || continue
            if ! cmp -s "$out/$id/$name" "$fix/$id/$name"; then
                echo "  differs: $id/$name" >&2
                bad=1
            fi
        done <<EOF
$want
EOF
    done
    # 글롭 */ 는 점으로 시작하는 폴더를 빼므로 find 로 바로 아래 항목을 모두 본다(오프라인 가드와 같은 집합).
    # 목록은 파일로 받아 find 의 종료 상태를 먼저 확인한다. 폴더를 가리키는 링크는 링크 너머를 읽지 않고 오류다.
    if ! find "$fix" -mindepth 1 -maxdepth 1 -print0 > "$work/fixture-entries"; then
        echo "  could not list $fix" >&2
        return 1
    fi
    while IFS= read -r -d '' dir; do
        id="$(basename "$dir")"
        if [[ -L "$dir" ]]; then
            echo "  symbolic link in the fixture root: $id" >&2
            bad=1
        elif [[ -d "$dir" ]] && ! is_locked_id "$id"; then
            echo "  not in the lock file: $id" >&2
            bad=1
        fi
    done < "$work/fixture-entries"
    return "$bad"
}

# CR 바이트가 든 파일을 나열한다. Git이 줄끝을 바꾸면 산출물과 바이트가 달라진다.
cr_files() {
    LC_ALL=C grep -rl $'\r' "$1" 2>/dev/null || true
}

lock_unchanged() {
    if cmp -s "$work/lock.toml" "$lock_file"; then
        return 0
    fi
    echo "Error: official-plugins.lock.toml changed while this run was fetching. Run it again." >&2
    return 1
}

if [[ "$mode" == check ]]; then
    if [[ ! -d "$fixtures" ]]; then
        echo "Error: no fixtures in $fixtures_rel. Run --update." >&2
        exit 1
    fi
    bad=0
    crs="$(cr_files "$fixtures")"
    if [[ -n "$crs" ]]; then
        echo "  CR bytes in:" >&2
        printf '%s\n' "$crs" | sed 's|^|    |' >&2
        bad=1
    fi
    compare "$work/out" "$fixtures" || bad=1
    lock_unchanged || exit 1
    if [[ "$bad" -ne 0 ]]; then
        echo "Error: the fixtures in $fixtures_rel differ from the locked artifacts. Run --update." >&2
        exit 1
    fi
    echo "==> ${#ids[@]} fixture(s) in $fixtures_rel match the locked artifacts"
    exit 0
fi

# --update: 후보를 만들어 검사한 뒤 이름 바꾸기로 들인다.
cand="$work/candidate"
mkdir "$cand"
for id in "${ids[@]}"; do
    if [[ ! -f "$work/out/$id/tasty-plugin.toml" ]]; then
        echo "Error: $id has no tasty-plugin.toml in the artifact" >&2
        exit 1
    fi
    mkdir "$cand/$id"
    cp "$work/out/$id/tasty-plugin.toml" "$cand/$id/"
    if [[ -d "$work/out/$id/lang" ]]; then
        cp -R "$work/out/$id/lang" "$cand/$id/lang"
    fi
done
crs="$(cr_files "$cand")"
if [[ -n "$crs" ]]; then
    echo "Error: the locked artifacts carry CR bytes; the fixtures were left unchanged:" >&2
    printf '%s\n' "$crs" | sed 's|^|    |' >&2
    exit 1
fi
if ! compare "$work/out" "$cand"; then
    echo "Error: the candidate does not match the fetched artifacts; the fixtures were left unchanged" >&2
    exit 1
fi
# 들이기 전에 오프라인 가드(official_plugin_fixtures)와 같은 조건을 후보에 적용한다.
bad=0
while IFS=$'\t' read -r id want_version want_bundle; do
    [[ -n "$id" ]] || continue
    if [[ ! -f "$cand/$id/lang/en.toml" ]]; then
        echo "  $id: lang/en.toml is missing" >&2
        bad=1
    fi
    if [[ ! -f "$cand/$id/tasty-plugin.toml" ]]; then
        echo "  $id: tasty-plugin.toml is missing" >&2
        bad=1
        continue
    fi
    if ! awk -f "$scan_awk" "$cand/$id/tasty-plugin.toml" > "$work/manifest.scan"; then
        echo "  $id: could not read tasty-plugin.toml" >&2
        bad=1
        continue
    fi
    # 매니페스트 최상위의 id·version·bundle. 오프라인 가드(official_plugin_fixtures.rs top_level)와 같은 규칙이다:
    # 읽지 못한 키, 불 값이 아닌 bundle(점 구분 키·표 포함), 두 번 나온 id·version·bundle이 하나라도 있으면
    # 위치와 관계없이 실패다.
    # bundle 키가 없으면 호스트 기본값 true다.
    if ! top="$(awk -F'\t' '
        # 어느 위치든 첫 조각이 bundle 인 표([bundle], [bundle."x.y"])는 최상위 bundle 을 불 값이 아닌 표로 만든다.
        # 첫 조각을 디코딩하지 못한 표(unknown)도 bundle 일 수 있어 실패다. 빈 인용 키 [""] 는 bundle 이 아니다.
        $1 == "H" { top_done = 1; if ($4 != "known" || $5 == "bundle") bad = 1; next }
        top_done { next }
        $1 == "K" && $2 == "unknown" { bad = 1 }
        $1 == "K" && $2 == "dotted" && $7 == "bundle" { bad = 1 }
        $1 == "K" && ($3 == "id" || $3 == "version" || $3 == "bundle") && seen[$3]++ { bad = 1 }
        $1 == "K" && $3 == "bundle" { if ($4 == "bool") b = $5; else bad = 1 }
        $1 == "K" && $3 == "id" && ($4 == "basic" || $4 == "literal") { id = $5 }
        $1 == "K" && $3 == "version" && ($4 == "basic" || $4 == "literal") { v = $5 }
        END { if (bad) exit 1; print id "\t" v "\t" (b == "" ? "true" : b) }' "$work/manifest.scan")"; then
        echo "  $id: tasty-plugin.toml has a top-level key it cannot read" >&2
        bad=1
        continue
    fi
    if [[ "$top" != "$id"$'\t'"$want_version"$'\t'"$want_bundle" ]]; then
        echo "  $id: id, version, bundle = ${top//$'\t'/, } in the manifest, $id, $want_version, $want_bundle in the lock file" >&2
        bad=1
    fi
done < "$work/lock.entries"
if [[ "$bad" -ne 0 ]]; then
    echo "Error: the candidate fails the fixture checks; the fixtures were left unchanged" >&2
    exit 1
fi
# 교체는 픽스처 폴더와 같은 부모 아래의 새 폴더끼리 이름만 바꾼다. 같은 폴더 안이라 mv가 복사로 바뀌지 않는다.
parent="$(dirname "$fixtures")"
mkdir -p "$parent"
side="$(mktemp -d "$parent/.official-plugins-sync.XXXXXX")"
if ! cp -R "$cand" "$side/new" || ! compare "$cand" "$side/new"; then
    echo "Error: could not place the candidate next to $fixtures_rel; the fixtures were left unchanged" >&2
    exit 1
fi
# 들이기 직전에만 잠금 파일을 다시 대조한다. 들인 뒤의 변경은 official_plugin_fixtures 검사와 CI --check가 잡는다.
lock_unchanged || exit 1
if [[ -e "$fixtures" ]]; then
    backup="$side/previous"
    if ! mv "$fixtures" "$backup"; then
        echo "Error: could not move $fixtures_rel aside; it was left unchanged" >&2
        exit 1
    fi
fi
if ! mv "$side/new" "$fixtures"; then
    echo "Error: could not move the new fixtures into $fixtures_rel" >&2
    exit 1
fi
installed=1
echo "==> Updated ${#ids[@]} fixture(s) in $fixtures_rel"
