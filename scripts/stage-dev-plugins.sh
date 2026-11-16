#!/usr/bin/env bash
# 개발 번들 ${CARGO_TARGET_DIR:-target}[/<triple>]/<profile>/builtin-plugins/<id>/를 준비한다.
# 공식 플러그인(bundle = false 포함)은 공식 플러그인 레포에서 얻고, mesh-demo는 이 workspace에서 빌드한다.
#   기본: 잠금 파일(official-plugins.lock.toml)의 릴리스 산출물을 받는다(fetch-official-plugins.sh --all).
#   --from-checkout [DIR]: 공식 플러그인 레포 체크아웃(기본 ../tasty-official-plugins)의
#     tasty-official-plugins/scripts/dev-bundle.sh로 빌드한다. 그 레포의 tasty 기준 커밋으로 빌드한다.
#   --with-local-tasty: --from-checkout과 함께 쓴다. 플러그인이 쓰는 tasty 패키지를 이 체크아웃으로
#     바꿔 빌드한다(dev-bundle.sh --tasty). 호스트 계약과 플러그인을 함께 바꿀 때 쓴다.
# 사용: scripts/stage-dev-plugins.sh [--profile <p>] [--platform <p>] [--target <triple>]
#                                     [--from-checkout [DIR]] [--with-local-tasty]
#   --profile: debug(기본), release, dist 등 target 아래 폴더 이름. debug가 아니면 서명 키를
#     먼저 준비하고(ensure-sign-key.sh) 스테이징한 매니페스트를 그 키로 서명한다.
#   --platform: 받을 산출물의 플랫폼 표지. 기본은 --target 또는 현재 호스트에서 정한다.
# 공식 플러그인은 <id>/ 폴더 단위로 통째 교체한다. 그 밖의 폴더는 건드리지 않는다.
# 교체는 기존 폴더를 옆 이름(.old-<id>.<pid>)으로 옮긴 뒤 새 폴더를 놓는다. 새 폴더를 놓지 못하거나
# 그 사이에 INT·TERM으로 끝나면 옆 이름의 폴더를 <id>/로 되돌린다. 되돌리지 못하면 두 경로를 알리고 실패한다.
# 실행 중인 Tasty는 설치 폴더(TASTY_HOME/plugins/<id>)의 실행 파일을 쓰므로 이 폴더를 잠그지 않는다.
# 다른 프로세스(탐색기·백신 등)가 이 폴더의 파일을 열고 있으면 옮기기가 실패하고 폴더는 그대로 남는다.
# 스테이징 폴더는 실행마다 새로 만들고 끝날 때 지운다. 같은 target·프로필의 동시 실행은
# <profile>/builtin-plugins.lock 폴더로 막는다. 이 실행이 끝나면 잠금을 푼다.
# 필요한 것: bash(3.2 이상), cargo, curl, tar. debug가 아니면 openssl. Windows는 Git Bash에서 실행한다.
set -euo pipefail

usage() {
    cat <<'USAGE'
Usage:
  stage-dev-plugins.sh [--profile <profile>] [--platform <platform>] [--target <triple>]
                       [--from-checkout [DIR]] [--with-local-tasty]
USAGE
    exit "${1:-2}"
}

root="$(cd "$(dirname "$0")/.." && pwd)"
caller="$PWD"
profile="debug"
platform=""
target=""
checkout=""
from_checkout=0
with_local_tasty=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --profile)
            [[ $# -ge 2 ]] || usage 2
            profile="$2"
            shift 2
            ;;
        --platform)
            [[ $# -ge 2 ]] || usage 2
            platform="$2"
            shift 2
            ;;
        --target)
            [[ $# -ge 2 ]] || usage 2
            target="$2"
            shift 2
            ;;
        --from-checkout)
            from_checkout=1
            if [[ $# -ge 2 && "$2" != --* ]]; then
                checkout="$2"
                shift 2
            else
                shift
            fi
            ;;
        --with-local-tasty)
            with_local_tasty=1
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

if [[ "$with_local_tasty" -eq 1 && "$from_checkout" -eq 0 ]]; then
    echo "Error: --with-local-tasty needs --from-checkout" >&2
    usage 2
fi

abs_path() {
    case "$1" in
        /* | [A-Za-z]:[\\/]*) printf '%s\n' "$1" ;;
        *) printf '%s/%s\n' "$caller" "$1" ;;
    esac
}

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
            # x64 에뮬레이션으로 도는 셸도 실제 CPU를 고르도록 Windows 환경 변수를 본다.
            case "${PROCESSOR_ARCHITEW6432:-${PROCESSOR_ARCHITECTURE:-}}" in
                ARM64) echo windows-arm64 ;;
                AMD64|x86_64|"") echo windows-x64 ;;
                *) return 1 ;;
            esac
            ;;
        *) return 1 ;;
    esac
}

triple_platform() {
    case "$1" in
        aarch64-apple-darwin) echo macos-arm64 ;;
        x86_64-unknown-linux-*) echo linux-x64 ;;
        aarch64-unknown-linux-*) echo linux-arm64 ;;
        x86_64-pc-windows-*) echo windows-x64 ;;
        aarch64-pc-windows-*) echo windows-arm64 ;;
        *) return 1 ;;
    esac
}

if [[ -z "$platform" ]]; then
    if [[ -n "$target" ]]; then
        platform="$(triple_platform "$target")" || {
            echo "Error: no official plugin platform for target $target; pass --platform" >&2
            exit 1
        }
    else
        platform="$(host_platform)" || {
            echo "Error: no official plugin platform for $(uname -s) $(uname -m); pass --platform" >&2
            exit 1
        }
    fi
fi
case "$platform" in
    windows-*) exe_ext=".exe" ;;
    *) exe_ext="" ;;
esac

case "$profile" in
    release) profile_flag=(--release) ;;
    debug)   profile_flag=() ;;
    *)       profile_flag=(--profile "$profile") ;;
esac
target_flag=()
[[ -n "$target" ]] && target_flag=(--target "$target")

# cargo와 같은 자리를 쓰도록 CARGO_TARGET_DIR를 호출 위치 기준 절대 경로로 고정한다.
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    CARGO_TARGET_DIR="$(abs_path "$CARGO_TARGET_DIR")"
    export CARGO_TARGET_DIR
fi
target_dir="${CARGO_TARGET_DIR:-$root/target}"
profile_dir="$target_dir${target:+/$target}/$profile"
bundle="$profile_dir/builtin-plugins"
lock="$profile_dir/builtin-plugins.lock"
cd "$root"

# 끝날 때 되돌리거나 지울 것. 교체 중인 폴더(pending_*)는 스테이징 폴더와 따로 다룬다.
stage=""
mesh_stage=""
lock_held=0
pending_old=""
pending_dest=""
pending_published=0

# 옆 이름으로 옮긴 기존 폴더를 정리한다. 새 폴더를 놓았으면 지우고, 놓지 못했으면 되돌린다.
settle_pending() {
    [[ -n "$pending_old" ]] || return 0
    local old="$pending_old" dest="$pending_dest" published="$pending_published"
    pending_old=""
    pending_dest=""
    pending_published=0
    [[ -e "$old" ]] || return 0
    if [[ "$published" -eq 1 ]]; then
        rm -rf "$old" || echo "Warning: could not remove $old" >&2
        return 0
    fi
    if [[ ! -e "$dest" ]] && mv "$old" "$dest"; then
        echo "Restored the previous $dest" >&2
        return 0
    fi
    echo "Error: could not restore $dest." >&2
    echo "  The previous folder is at $old. Move it back to $dest by hand." >&2
    return 1
}

cleanup() {
    local rc=$?
    trap - EXIT INT TERM
    settle_pending || rc=1
    local dir
    for dir in "$stage" "$mesh_stage"; do
        if [[ -n "$dir" ]] && ! rm -rf "$dir"; then
            echo "Warning: could not remove $dir" >&2
        fi
    done
    if [[ "$lock_held" -eq 1 ]]; then
        rmdir "$lock" || echo "Warning: could not remove $lock" >&2
    fi
    exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$profile_dir"
if ! mkdir "$lock" 2>/dev/null; then
    if [[ -d "$lock" ]]; then
        echo "Error: another run is staging into $bundle ($lock exists)." >&2
        echo "  Wait for it to finish and run this again. If no run is in progress, remove $lock." >&2
    else
        echo "Error: could not create $lock" >&2
    fi
    exit 1
fi
lock_held=1

# 본체에 공개키를 포함할 수 있도록 본체 빌드보다 먼저 서명 키를 준비한다.
if [[ "$profile" != debug ]]; then
    SIGN_KEY_PATH="$(bash "$root/scripts/ensure-sign-key.sh")"
    export SIGN_KEY_PATH
fi

stage="$(mktemp -d "$profile_dir/.official-plugins-stage.XXXXXX")"
if [[ "$from_checkout" -eq 1 ]]; then
    checkout="$(abs_path "${checkout:-../tasty-official-plugins}")"
    if [[ ! -f "$checkout/scripts/dev-bundle.sh" ]]; then
        echo "Error: $checkout/scripts/dev-bundle.sh not found; pass the official plugins checkout to --from-checkout" >&2
        exit 1
    fi
    case "$profile" in
        debug|release|dist) ;;
        *)
            echo "Error: dev-bundle.sh builds only debug, release, or dist (got $profile)" >&2
            exit 1
            ;;
    esac
    bundle_args=(--profile "$profile" --out "$stage")
    [[ "$with_local_tasty" -eq 1 ]] && bundle_args+=(--tasty "$root")
    [[ -n "$target" ]] && bundle_args+=(--target "$target")
    echo "==> Building official plugins in $checkout"
    # 플러그인 레포는 자기 target에 빌드한다. 같은 이름의 실행 파일이 이 workspace의 target에 섞이지 않게 한다.
    (cd "$checkout" && env -u CARGO_TARGET_DIR bash scripts/dev-bundle.sh "${bundle_args[@]}")
else
    bash "$root/scripts/fetch-official-plugins.sh" --platform "$platform" --out "$stage" --all
fi

if [[ "$profile" != debug ]]; then
    bash "$root/scripts/sign-bundle.sh" --key "$SIGN_KEY_PATH" --dir "$stage"
fi

# 기존 폴더를 옆 이름으로 옮긴 뒤 새 폴더를 놓는다. 새 폴더를 놓지 못하면 기존 폴더를 되돌린다.
# pending_*를 옮기기 전에 적어 두어, 두 이동 사이에 신호로 끝나도 cleanup이 되돌린다.
replace_dir() {
    local src="$1" dest="$2" id="$3"
    if [[ -e "$dest" ]]; then
        pending_old="$bundle/.old-$id.$$"
        pending_dest="$dest"
        pending_published=0
        if ! mv "$dest" "$pending_old"; then
            pending_old=""
            pending_dest=""
            echo "Error: could not move $dest aside; it was left unchanged." >&2
            echo "  Another process (Explorer, antivirus, an open file) may hold a file in it." >&2
            echo "  Close it and run this again." >&2
            return 1
        fi
    fi
    if ! mv "$src" "$dest"; then
        echo "Error: could not move the new $id folder into $dest" >&2
        settle_pending || true
        return 1
    fi
    pending_published=1
    settle_pending
}

mkdir -p "$bundle"
staged=0
for dir in "$stage"/*/; do
    [[ -d "$dir" ]] || continue
    id="$(basename "$dir")"
    replace_dir "$stage/$id" "$bundle/$id" "$id"
    echo "✓ staged $id → $bundle/$id"
    staged=$((staged + 1))
done
rmdir "$stage"
stage=""
if [[ "$staged" -eq 0 ]]; then
    echo "Error: no official plugin was staged" >&2
    exit 1
fi

# mesh-demo는 이 workspace의 크레이트에서 빌드한다.
mesh_crate="tasty-plugin-mesh-demo"
mesh_src="$root/crates/$mesh_crate"
mesh_id="com.tasty.mesh-demo"
# Bash 3.2의 set -u는 빈 배열 전개를 오류로 보므로 비었을 때는 아무것도 펼치지 않는다.
cargo build ${profile_flag[@]+"${profile_flag[@]}"} ${target_flag[@]+"${target_flag[@]}"} -p "$mesh_crate"
mesh_bin="$profile_dir/$mesh_crate$exe_ext"
if [[ ! -f "$mesh_bin" ]]; then
    echo "Error: built binary missing at $mesh_bin" >&2
    exit 1
fi
mesh_stage="$(mktemp -d "$profile_dir/.mesh-demo-stage.XXXXXX")"
# mktemp -d는 소유자만 읽는 폴더를 만든다. 이 폴더가 번들의 플러그인 폴더가 되므로 보통 권한으로 맞춘다.
chmod 755 "$mesh_stage"
cp "$mesh_bin" "$mesh_stage/"
cp "$mesh_src/tasty-plugin.toml" "$mesh_stage/"
if [[ -d "$mesh_src/lang" ]]; then
    cp -R "$mesh_src/lang" "$mesh_stage/lang"
fi
if [[ "$profile" != debug ]]; then
    bash "$root/scripts/sign-bundle.sh" --key "$SIGN_KEY_PATH" --manifest "$mesh_stage/tasty-plugin.toml"
fi
replace_dir "$mesh_stage" "$bundle/$mesh_id" "$mesh_id"
mesh_stage=""
echo "✓ staged $mesh_id → $bundle/$mesh_id"
echo "==> Development bundle ready: $bundle ($((staged + 1)) plugins)"
