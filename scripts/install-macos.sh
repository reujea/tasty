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

# Installing is not a development command, so lints and warnings must not stop
# or clutter the build. --cap-lints=allow lowers the workspace deny lints for
# this build only. Any RUSTFLAGS the caller set are kept in front of it.
# Note that RUSTFLAGS replaces target.<triple>.rustflags from .cargo/config.toml;
# the repository sets none for macOS targets.
INSTALL_RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--cap-lints=allow"

# Assemble the .app bundle without packaging a DMG (NO_DMG=1).
NO_DMG=1 RUSTFLAGS="$INSTALL_RUSTFLAGS" ./scripts/build-macos-dmg.sh "$@"

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
