#!/usr/bin/env bash
# 매니페스트의 SHA-256 digest를 Ed25519로 서명해 <manifest>.sig에 raw 64바이트를 쓴다.
# bundle_sig.rs의 검증과 같은 두 단계다. 원문 직접 서명이나 Ed25519ph로 임의 변경하지 않는다.
# 사용: ./scripts/sign-bundle.sh --key <private-key.pem> --manifest <manifest>
#       ./scripts/sign-bundle.sh --key <private-key.pem> --all-builtins
#       ./scripts/sign-bundle.sh --key <private-key.pem> --dir <staged-plugins-dir>
# --all-builtins는 crates 바로 아래 디렉터리들의 tasty-plugin.toml을 찾는다.
# --dir는 <dir>/<id>/tasty-plugin.toml을 모두 서명한다. 받은 산출물은 서명하지 않은 채로 와야 하므로
# 그 아래에 .sig가 하나라도 있으면 아무것도 서명하지 않고 실패한다. OpenSSL이 필요하다.

set -euo pipefail

KEY_PATH=""
MANIFEST=""
ALL_BUILTINS=0
STAGE_DIR=""

usage() {
    cat <<'USAGE'
Usage:
  sign-bundle.sh --key <private-key.pem> --manifest <path-to-tasty-plugin.toml>
  sign-bundle.sh --key <private-key.pem> --all-builtins
  sign-bundle.sh --key <private-key.pem> --dir <staged-plugins-dir>
USAGE
    exit "${1:-2}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --key)
            [[ $# -ge 2 ]] || usage 2
            KEY_PATH="$2"
            shift 2
            ;;
        --manifest)
            [[ $# -ge 2 ]] || usage 2
            MANIFEST="$2"
            shift 2
            ;;
        --all-builtins)
            ALL_BUILTINS=1
            shift
            ;;
        --dir)
            [[ $# -ge 2 ]] || usage 2
            STAGE_DIR="$2"
            shift 2
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

if [[ -z "$KEY_PATH" ]]; then
    echo "Error: --key is required" >&2
    usage 2
fi

if ! command -v openssl >/dev/null 2>&1; then
    echo "Error: openssl not found in PATH" >&2
    exit 1
fi

if [[ ! -f "$KEY_PATH" ]]; then
    echo "Error: private key not found: $KEY_PATH" >&2
    echo "  Run ./scripts/gen-dev-key.sh to generate a dev key." >&2
    exit 1
fi

MODES=0
[[ -n "$MANIFEST" ]] && MODES=$((MODES + 1))
[[ "$ALL_BUILTINS" -eq 1 ]] && MODES=$((MODES + 1))
[[ -n "$STAGE_DIR" ]] && MODES=$((MODES + 1))
if [[ "$MODES" -gt 1 ]]; then
    echo "Error: --manifest, --all-builtins and --dir are mutually exclusive" >&2
    usage 2
fi
if [[ "$MODES" -eq 0 ]]; then
    echo "Error: must specify --manifest, --all-builtins or --dir" >&2
    usage 2
fi

declare -a MANIFESTS=()
if [[ "$ALL_BUILTINS" -eq 1 ]]; then
    REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
    while IFS= read -r -d '' f; do
        MANIFESTS+=("$f")
    done < <(find "$REPO_ROOT/crates" -mindepth 2 -maxdepth 2 -name 'tasty-plugin.toml' -print0)
    if [[ ${#MANIFESTS[@]} -eq 0 ]]; then
        echo "Error: no crates/tasty-plugin-*/tasty-plugin.toml found" >&2
        exit 1
    fi
elif [[ -n "$STAGE_DIR" ]]; then
    if [[ ! -d "$STAGE_DIR" ]]; then
        echo "Error: directory not found: $STAGE_DIR" >&2
        exit 1
    fi
    while IFS= read -r -d '' f; do
        MANIFESTS+=("$f")
    done < <(find "$STAGE_DIR" -mindepth 2 -maxdepth 2 -name 'tasty-plugin.toml' -print0 | sort -z)
    if [[ ${#MANIFESTS[@]} -eq 0 ]]; then
        echo "Error: no $STAGE_DIR/*/tasty-plugin.toml found" >&2
        exit 1
    fi
    declare -a PRESIGNED=()
    while IFS= read -r -d '' f; do
        PRESIGNED+=("$f")
    done < <(find "$STAGE_DIR" -name '*.sig' -print0 | sort -z)
    if [[ ${#PRESIGNED[@]} -gt 0 ]]; then
        echo "Error: $STAGE_DIR already contains signatures; staged plugins must arrive unsigned:" >&2
        printf '  %s\n' "${PRESIGNED[@]}" >&2
        exit 1
    fi
else
    if [[ ! -f "$MANIFEST" ]]; then
        echo "Error: manifest not found: $MANIFEST" >&2
        exit 1
    fi
    MANIFESTS=("$MANIFEST")
fi

TMPDIR_SIGN="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_SIGN"' EXIT

SIGNED=0
for manifest in "${MANIFESTS[@]}"; do
    sig_path="${manifest}.sig"
    digest_path="$TMPDIR_SIGN/$(basename "$(dirname "$manifest")").sha256"

    openssl dgst -sha256 -binary "$manifest" > "$digest_path"

    # -rawin은 위 digest를 서명할 메시지로 사용한다.
    openssl pkeyutl -sign \
        -inkey "$KEY_PATH" \
        -rawin \
        -in "$digest_path" \
        -out "$sig_path"

    sig_len=$(wc -c < "$sig_path" | tr -d ' ')
    if [[ "$sig_len" != "64" ]]; then
        echo "Error: $sig_path is $sig_len bytes, expected 64" >&2
        echo "  (확인 사항: --key 가 Ed25519 키인가? openssl 버전이 1.1.1 이상인가?)" >&2
        rm -f "$sig_path"
        exit 1
    fi

    SIGNED=$((SIGNED + 1))
    echo "  signed: $manifest → $sig_path"
done

echo "==> Signed $SIGNED plugin manifest(s)."
