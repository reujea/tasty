#!/usr/bin/env bash
# 개발용 Ed25519 개인키와 raw 공개키를 준비한다. 기존 개인키는 유지하고 공개키를 다시 추출한다.
# --force는 개인키를 덮어쓰므로 기존 키로 만든 서명과 호환되지 않을 수 있다.
# 사용: bash scripts/gen-dev-key.sh [--force]. 키 경로는 아래 설정을 따른다.

set -euo pipefail

FORCE=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --force)
            FORCE=1
            shift
            ;;
        -h|--help)
            cat <<'USAGE'
Usage: gen-dev-key.sh [--force]

  --force   기존 dev key 가 있어도 덮어쓰기. 평소엔 사용 금지.
USAGE
            exit 0
            ;;
        *)
            echo "Error: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

if ! command -v openssl >/dev/null 2>&1; then
    echo "Error: openssl not found in PATH" >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KEY_DIR="${HOME}/.tasty-keys"
PRIV_PATH="${KEY_DIR}/dev.pem"
PUB_PATH="${REPO_ROOT}/crates/tasty-host-plugin/keys/dev-pubkey.bin"

mkdir -p "$KEY_DIR"
chmod 700 "$KEY_DIR"

# DER 공개키의 마지막 32바이트를 사용한다. 추출 길이는 확인하지만 DER 구조 전체를 파싱하지 않는다.
derive_pubkey() {
    local priv="$1" out="$2"
    mkdir -p "$(dirname "$out")"
    local tmp
    tmp="$(mktemp)"
    openssl pkey -in "$priv" -pubout -outform DER | tail -c 32 > "$tmp"
    local len
    len=$(wc -c < "$tmp" | tr -d ' ')
    if [[ "$len" != "32" ]]; then
        echo "Error: extracted pubkey is $len bytes, expected 32" >&2
        echo "  (openssl 버전 또는 OS 차이로 인한 DER 헤더 변경 가능성)" >&2
        rm -f "$tmp"
        exit 1
    fi
    # build.rs가 공개키 변경을 감지하므로 내용이 같으면 mtime을 유지한다.
    if [[ -f "$out" ]] && cmp -s "$tmp" "$out"; then
        rm -f "$tmp"
    else
        mv "$tmp" "$out"
    fi
}

if [[ -f "$PRIV_PATH" && "$FORCE" -ne 1 ]]; then
    derive_pubkey "$PRIV_PATH" "$PUB_PATH"
    echo "Private key exists: $PRIV_PATH (kept)"
    echo "Re-derived public key: $PUB_PATH"
    echo "  Use --force to regenerate the private key (NOTE: 기존 .sig 는 모두 무효화됨)."
    exit 0
fi

openssl genpkey -algorithm Ed25519 -out "$PRIV_PATH"
chmod 600 "$PRIV_PATH"

derive_pubkey "$PRIV_PATH" "$PUB_PATH"

cat <<EOF
==> Generated Ed25519 dev keypair.

  Private (gitignored, chmod 600): $PRIV_PATH
  Public  (local, untracked):      $PUB_PATH

다음 단계: 따로 실행할 명령은 없다.
  다음 release·dist 빌드(just build --release, just build-all, dist 스크립트)가
  번들에 넣을 플러그인 매니페스트 사본을 서명하고, 본체를 빌드하며 새 공개키를 임베드한다.
  SIGN_KEY_PATH와 ~/.tasty-keys/release.pem이 없으면 이 키로 서명한다.
  just build-plugins만 실행했다면 본체도 다시 빌드해야 새 공개키가 들어간다.
  debug 빌드는 서명하지 않는다. sign-bundle.sh --all-builtins 는 크레이트 원본 옆에만
  .sig 를 쓰는 선택적 수동 도구라 번들에는 반영되지 않는다.
EOF
