#!/bin/bash
# Build Tasty (host, dist profile) with the locked official plugins and install it to /Applications.
#
# Usage:
#   ./scripts/install-macos.sh            # dist build (full LTO, 배포용)
#   ./scripts/install-macos.sh --release  # release build (thin LTO, 빠른 빌드)
#   ./scripts/install-macos.sh --debug    # debug build
#
# The app contains bundled plugins. On startup, the host synchronizes changed
# files at the same version and keeps any newer installed plugin version.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Error: This script must be run on macOS." >&2
    exit 1
fi

cd "$(dirname "$0")/.."

APP_NAME="Tasty"
SRC_APP="dist/$APP_NAME.app"
DEST_APP="/Applications/$APP_NAME.app"

# Installing is not a development command, so the rustc lints that the cap
# applies to in the built crates must not stop or clutter the build.
# --cap-lints=allow lowers the workspace deny lints for this build only, and
# flags the caller set are kept in front. Lints the caller forces with
# --force-warn are not capped and still warn.
# Cargo ignores RUSTFLAGS when CARGO_ENCODED_RUSTFLAGS is set, so the cap goes
# into whichever of the two Cargo will read (a set but empty
# CARGO_ENCODED_RUSTFLAGS still wins over RUSTFLAGS). Either one replaces
# target.<triple>.rustflags and build.rustflags from config files; the
# repository sets none for macOS targets.
# Build scripts are host artifacts of a --target build and do not get the cap.
# Assemble the .app bundle without packaging a DMG (NO_DMG=1).
if [[ -n "${CARGO_ENCODED_RUSTFLAGS+set}" ]]; then
    flag_sep=$'\x1f'
    NO_DMG=1 CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS:+$CARGO_ENCODED_RUSTFLAGS$flag_sep}--cap-lints=allow" \
        ./scripts/build-macos-dmg.sh "$@"
else
    NO_DMG=1 RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--cap-lints=allow" \
        ./scripts/build-macos-dmg.sh "$@"
fi

if [[ ! -d "$SRC_APP" ]]; then
    echo "Error: expected bundle missing: $SRC_APP" >&2
    exit 1
fi

echo "==> Installing to $DEST_APP (overwrite)..."
rm -rf "$DEST_APP"
cp -R "$SRC_APP" "$DEST_APP"

echo ""
echo "Installed!"
echo "  App: $DEST_APP"
echo "  플러그인은 앱 시작 시 버전·내용을 비교해 사용자 플러그인 폴더에 반영합니다."
