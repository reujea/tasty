#!/usr/bin/env bash
# 잠금 파일(official-plugins.lock.toml)이 가리키는 공식 플러그인 릴리스 산출물을 받아
# SHA-256을 대조하고 <out>/<id>/에 푼다. 푼 매니페스트는 현재 호스트의 파서·검증기로 읽어 본다.
# 사용: scripts/fetch-official-plugins.sh --platform <p> --out <dir> [--all] [--lock <path>] [--cache <dir>]
#   <p>: macos-arm64 | windows-x64 | windows-arm64 | linux-x64 | linux-arm64
#   --all: bundle = false 플러그인까지 받는다(개발 경로용). 없으면 배포 대상만.
#   --out: 매번 비우거나 새로 만든다. 이번 계획에 없는 폴더가 남아 있으면 unpack이 실패한다.
# 받은 파일은 ${CARGO_TARGET_DIR:-target}/official-plugins/<tag>/<platform>/에 둔다. --cache를 주면 그 폴더에 둔다.
# 해시가 맞는 파일은 다시 받지 않고, 맞지 않는 파일은 지우고 다시 받는다.
# 해시 계산과 TOML 읽기는 official-plugins 도구(crates/tasty-doc-guards)가 한다.
# 필요한 것: bash(3.2 이상, macOS 기본 bash 포함), cargo, curl, tar. Windows는 Git Bash에서 실행한다.
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  fetch-official-plugins.sh --platform <platform> --out <dir> [--all] [--lock <path>] [--cache <dir>]
USAGE
    exit "${1:-2}"
}

root="$(cd "$(dirname "$0")/.." && pwd)"
platform=""
out=""
all=""
lock="$root/official-plugins.lock.toml"
cache_dir=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --platform)
            [[ $# -ge 2 ]] || usage 2
            platform="$2"
            shift 2
            ;;
        --out)
            [[ $# -ge 2 ]] || usage 2
            out="$2"
            shift 2
            ;;
        --lock)
            [[ $# -ge 2 ]] || usage 2
            lock="$2"
            shift 2
            ;;
        --cache)
            [[ $# -ge 2 ]] || usage 2
            cache_dir="$2"
            shift 2
            ;;
        --all)
            all="--all"
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

if [[ -z "$platform" || -z "$out" ]]; then
    echo "Error: --platform and --out are required" >&2
    usage 2
fi
for tool in cargo curl tar; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Error: $tool not found in PATH" >&2
        exit 1
    fi
done

tool() {
    cargo run -q --locked --manifest-path "$root/Cargo.toml" \
        -p tasty-doc-guards --bin official-plugins -- "$@"
}

tool check --lock "$lock"

plan_file="$(mktemp)"
verify_log="$(mktemp)"
trap 'rm -f "$plan_file" "$verify_log"' EXIT
tool plan --lock "$lock" --platform "$platform" ${all:+"$all"} > "$plan_file"
if [[ ! -s "$plan_file" ]]; then
    echo "Error: the lock file plans no plugin for $platform" >&2
    exit 1
fi

# 받은 파일 위치: URL의 태그 칸(.../download/<tag>/<file>)으로 나눈다.
first_url="$(cut -f4 "$plan_file" | sed -n '1p')"
tag="$(basename "$(dirname "$first_url")")"
cache="${cache_dir:-${CARGO_TARGET_DIR:-$root/target}/official-plugins/$tag/$platform}"
mkdir -p "$cache"

# verify가 이름을 대는 파일(없음·해시 불일치)만 받는다.
need=""
if ! tool verify --lock "$lock" --dir "$cache" --platform "$platform" ${all:+"$all"} 2> "$verify_log"; then
    while IFS= read -r line; do
        case "$line" in
            *"missing: "*|*"sha256 mismatch: "*)
                name="${line#*: }"
                name="${name%% *}"
                need="$need$name"$'\n'
                ;;
        esac
    done < "$verify_log"
    if [[ -z "$need" ]]; then
        cat "$verify_log" >&2
        echo "Error: verify failed without naming a file" >&2
        exit 1
    fi
fi

fetched=0
while IFS=$'\t' read -r id version file url sha; do
    case $'\n'"$need" in
        *$'\n'"$file"$'\n'*) ;;
        *) continue ;;
    esac
    echo "==> Fetching $id $version ($platform)"
    rm -f "$cache/$file" "$cache/$file.part"
    curl -fL --retry 3 --silent --show-error -o "$cache/$file.part" "$url"
    mv "$cache/$file.part" "$cache/$file"
    fetched=$((fetched + 1))
    : "$sha"
done < "$plan_file"
planned=$(wc -l < "$plan_file" | tr -d ' ')
echo "==> $platform: planned $planned, fetched $fetched, reused $((planned - fetched)) from $cache"

mkdir -p "$out"
tool unpack --lock "$lock" --dir "$cache" --out "$out" --platform "$platform" ${all:+"$all"}

# 현재 호스트의 매니페스트 파서·검증기로 읽는다(api_version·manifest_version·권한 이름 포함).
declare -a staged=()
while IFS=$'\t' read -r id _rest; do
    staged+=("$out/$id")
done < "$plan_file"
cargo run -q --locked --manifest-path "$root/Cargo.toml" \
    -p tasty-plugin-manifest --example validate-plugin-dirs -- "${staged[@]}"
echo "==> Staged ${#staged[@]} official plugin(s) into $out"
