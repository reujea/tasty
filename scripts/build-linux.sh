#!/usr/bin/env bash
# Build Linux distribution artifacts for Tasty.
#
# Usage:
#   ./scripts/build-linux.sh           # dist build (full LTO, 배포용)
#   ./scripts/build-linux.sh --release # release build (thin LTO, 빠른 빌드)
#   ./scripts/build-linux.sh --debug   # debug build (tar.gz만)
#
# Output:
#   dist/tasty-{version}-linux-{arch}.tar.gz       # arch: x64 | arm64
#   dist/tasty_{version}-1_{deb-arch}.deb          # debug 제외, deb-arch: amd64 | arm64
#   dist/tasty-{version}-1.{rpm-arch}.rpm          # debug 제외, rpm-arch: x86_64 | aarch64
#   dist/Tasty-{version}-{rpm-arch}.AppImage       # debug 제외, distro-무관 단일 파일
#   dist/tasty-headless-{version}-linux-{arch}.tar.gz  # gui feature 없는 헤드리스 빌드
#
# Requires:
#   curl, tar                          # 번들 플러그인: official-plugins.lock.toml의 산출물을 받는다(빌드하지 않는다)
#   cargo install cargo-deb            # .deb 패키지 생성 (debug 빌드에선 생략)
#   cargo install cargo-generate-rpm   # .rpm 패키지 생성 (debug 빌드에선 생략)
#   linuxdeploy (PATH에 위치)          # .AppImage 생성. 다음 한 번만 실행:
#     curl -fsSL -o ~/.local/bin/linuxdeploy \
#       https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$(uname -m).AppImage
#     chmod +x ~/.local/bin/linuxdeploy

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "Error: This script must be run on Linux." >&2
    exit 1
fi

cd "$(dirname "$0")/.."

PROFILE="dist"
CARGO_FLAGS="--profile dist"
if [[ "${1:-}" == "--debug" ]]; then
    PROFILE="debug"
    CARGO_FLAGS=""
elif [[ "${1:-}" == "--release" ]]; then
    PROFILE="release"
    CARGO_FLAGS="--release"
fi

ARCH_RAW=$(uname -m)
case "$ARCH_RAW" in
    x86_64)  ARCH=x64 ;;
    aarch64) ARCH=arm64 ;;
    *) echo "Error: Unsupported architecture: $ARCH_RAW" >&2; exit 1 ;;
esac

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
DIST_DIR="dist"
PKG_DIR="tasty-linux-${ARCH}"
ARCHIVE_NAME="tasty-${VERSION}-linux-${ARCH}.tar.gz"

echo "==> Checking build dependencies..."
MISSING_DEPS=()
for dep in cmake pkg-config; do
    if ! command -v "$dep" &>/dev/null; then
        MISSING_DEPS+=("$dep")
    fi
done
for lib in freetype2 fontconfig; do
    if ! pkg-config --exists "$lib" 2>/dev/null; then
        MISSING_DEPS+=("$lib")
    fi
done
# libxdo 개발 패키지의 링크 가능 여부는 .pc 파일 대신 실제 링커로 확인한다.
if ! echo 'int main(void){return 0;}' | cc -xc - -lxdo -o /dev/null 2>/dev/null; then
    MISSING_DEPS+=("libxdo")
fi

if [[ ${#MISSING_DEPS[@]} -gt 0 ]]; then
    PKG_MGR=""
    for m in apt-get dnf pacman zypper; do
        if command -v "$m" &>/dev/null; then PKG_MGR="$m"; break; fi
    done
    pkg_name_for() {
        case "$PKG_MGR:$1" in
            *:cmake) echo cmake ;;
            apt-get:pkg-config|zypper:pkg-config) echo pkg-config ;;
            dnf:pkg-config) echo pkgconf-pkg-config ;;
            pacman:pkg-config) echo pkgconf ;;
            apt-get:freetype2) echo libfreetype6-dev ;;
            dnf:freetype2)     echo freetype-devel ;;
            pacman:freetype2)  echo freetype2 ;;
            zypper:freetype2)  echo freetype2-devel ;;
            apt-get:fontconfig) echo libfontconfig1-dev ;;
            dnf:fontconfig)     echo fontconfig-devel ;;
            pacman:fontconfig)  echo fontconfig ;;
            zypper:fontconfig)  echo fontconfig-devel ;;
            apt-get:libxdo)        echo libxdo-dev ;;
            dnf:libxdo|zypper:libxdo) echo libxdo-devel ;;
            pacman:libxdo)         echo xdotool ;;
            *) echo "" ;;
        esac
    }

    PKGS=()
    UNMAPPED=()
    for key in "${MISSING_DEPS[@]}"; do
        name="$(pkg_name_for "$key")"
        if [[ -n "$name" ]]; then PKGS+=("$name"); else UNMAPPED+=("$key"); fi
    done

    echo "Error: Missing build dependencies: ${MISSING_DEPS[*]}" >&2

    SUDO=""
    if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
        command -v sudo &>/dev/null && SUDO="sudo"
    fi

    INSTALL_ARGS=()
    case "$PKG_MGR" in
        apt-get) INSTALL_ARGS=(apt-get install -y "${PKGS[@]}") ;;
        dnf)     INSTALL_ARGS=(dnf install -y "${PKGS[@]}") ;;
        pacman)  INSTALL_ARGS=(pacman -S --noconfirm "${PKGS[@]}") ;;
        zypper)  INSTALL_ARGS=(zypper install -y "${PKGS[@]}") ;;
    esac
    SHOW="${SUDO:+sudo }${INSTALL_ARGS[*]}"

    if [[ -n "$PKG_MGR" && ${#UNMAPPED[@]} -eq 0 && -t 0 \
          && ( -n "$SUDO" || "${EUID:-$(id -u)}" -eq 0 ) ]]; then
        printf "  Install now? [%s] [y/N] " "$SHOW" >&2
        read -r reply || reply=""
        if [[ "$reply" =~ ^[Yy]([Ee][Ss])?$ ]]; then
            echo "==> $SHOW"
            if [[ -n "$SUDO" ]]; then sudo "${INSTALL_ARGS[@]}"; else "${INSTALL_ARGS[@]}"; fi \
                || { echo "Error: dependency install failed." >&2; exit 1; }
        else
            echo "  Aborted. Run: $SHOW" >&2
            exit 1
        fi
    else
        echo "  Install with your package manager, e.g.: sudo apt install ${PKGS[*]:-${MISSING_DEPS[*]}}" >&2
        [[ ${#UNMAPPED[@]} -gt 0 ]] && echo "  (no package mapping for: ${UNMAPPED[*]})" >&2
        exit 1
    fi
fi

# 빌드 전에 서명 키를 준비해야 대응 공개키가 바이너리에 포함된다.
if [[ "$PROFILE" != "debug" ]]; then
    SIGN_KEY_PATH="$(./scripts/ensure-sign-key.sh)"
    export SIGN_KEY_PATH
fi

echo "==> Building tasty ($PROFILE)..."
cargo build $CARGO_FLAGS

# 번들 플러그인은 여기서 빌드하지 않는다. 잠금 파일의 bundle = true 산출물을 받아 넣는다.
# shellcheck source=scripts/lib/bundled-plugins.sh
. "scripts/lib/bundled-plugins.sh"

echo "==> Fetching locked official plugins (linux-$ARCH)..."
fetch_bundled_plugins "linux-$ARCH"

# 매니페스트 서명은 이 함수 하나가 맡는다. 서명 주체가 바뀌면 이 함수만 바꾼다.
# 받은 산출물에 .sig가 있으면 sign-bundle.sh가 실패한다(공식 플러그인은 서명 없이 온다).
sign_bundled_plugins() {
    echo "==> Signing plugin manifests with $SIGN_KEY_PATH..."
    ./scripts/sign-bundle.sh --key "$SIGN_KEY_PATH" --dir "$BUNDLED_PLUGINS_DIR"
}

if [[ "$PROFILE" != "debug" ]]; then
    sign_bundled_plugins
fi
normalize_bundled_plugin_modes

# 실행 파일 옆의 plugins/<id>에 배치한다. 번들 탐색 경로와 맞아야 한다.
stage_plugins() {
    stage_bundled_plugins "$1"
}

# 공용 notice 목록을 읽어 Linux와 macOS가 같은 파일을 배포한다.
# shellcheck source=scripts/lib/notice-set.sh
. "scripts/lib/notice-set.sh"

echo "==> Assembling archive..."
rm -rf "${DIST_DIR:?}/${PKG_DIR:?}"
mkdir -p "$DIST_DIR/$PKG_DIR"

# main 진입 전 공유 라이브러리가 없을 때 안내할 launcher를 함께 배포한다.
# $1 = 실제 바이너리 tasty.bin이 있는 디렉터리.
write_launcher() {
cat > "$1/tasty" <<'WRAPPER_EOF'
#!/usr/bin/env bash
# 실행 전에 ldd에서 누락된 공유 라이브러리를 찾는다. ldd가 없거나 검사 실패 시에도 실행을 시도한다.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HERE/tasty.bin"

if command -v ldd &>/dev/null; then
    MISSING=$(ldd "$BIN" 2>/dev/null | awk '/not found/ {print $1}' || true)
    if [[ -n "$MISSING" ]]; then
        echo "Error: tasty is missing required shared libraries:" >&2
        echo "$MISSING" | sed 's/^/  /' >&2
        echo "" >&2
        echo "Find the package that provides each library with your distro's tools:" >&2
        if command -v apt-get &>/dev/null; then
            echo "  apt-file search <library-name>   (may need: sudo apt install apt-file && sudo apt-file update)" >&2
        elif command -v dnf &>/dev/null; then
            echo "  dnf provides '*/<library-name>'" >&2
        elif command -v zypper &>/dev/null; then
            echo "  zypper what-provides <library-name>" >&2
        elif command -v pacman &>/dev/null; then
            echo "  pacman -F <library-name>" >&2
        fi
        echo "See docs/installation.md for the known dependency list." >&2
        exit 1
    fi
fi

exec "$BIN" "$@"
WRAPPER_EOF
chmod +x "$1/tasty"
}

cp "target/$PROFILE/tasty" "$DIST_DIR/$PKG_DIR/tasty.bin"
write_launcher "$DIST_DIR/$PKG_DIR"

stage_plugins "$DIST_DIR/$PKG_DIR/plugins"
stage_notice "$DIST_DIR/$PKG_DIR"

echo "==> Creating $ARCHIVE_NAME..."
rm -f "$DIST_DIR/$ARCHIVE_NAME"
tar -czf "$DIST_DIR/$ARCHIVE_NAME" -C "$DIST_DIR" "$PKG_DIR"

rm -rf "${DIST_DIR:?}/${PKG_DIR:?}"

DEB_FILE=""
RPM_FILE=""
APPIMAGE_FILE=""
if [[ "$PROFILE" != "debug" ]]; then
    if ! command -v cargo-deb &>/dev/null; then
        echo "Error: cargo-deb not found. Install with: cargo install cargo-deb" >&2
        exit 1
    fi
    if ! command -v cargo-generate-rpm &>/dev/null; then
        echo "Error: cargo-generate-rpm not found. Install with: cargo install cargo-generate-rpm" >&2
        exit 1
    fi
    if ! command -v linuxdeploy &>/dev/null; then
        echo "Error: linuxdeploy not found in PATH. See header of this script for install instructions." >&2
        exit 1
    fi

    # deb/rpm assets(Cargo.toml)는 본체를 target/release에서, 번들 플러그인을 $BUNDLED_PLUGINS_DIR 에서 읽는다.
    # 두 도구 모두 --profile을 주면 target/release/ 를 target/<profile>/ 로 바꿔 읽는다.

    echo "==> Building .deb package..."
    cargo deb --no-build --profile "$PROFILE"
    deb_candidates=$(ls -t target/debian/tasty_*.deb)
    DEB_SRC=$(head -1 <<<"$deb_candidates")
    cp "$DEB_SRC" "$DIST_DIR/"
    DEB_FILE="$DIST_DIR/$(basename "$DEB_SRC")"

    echo "==> Building .rpm package..."
    cargo generate-rpm --profile "$PROFILE"
    rpm_candidates=$(ls -t target/generate-rpm/tasty-*.rpm)
    RPM_SRC=$(head -1 <<<"$rpm_candidates")
    cp "$RPM_SRC" "$DIST_DIR/"
    RPM_FILE="$DIST_DIR/$(basename "$RPM_SRC")"

    echo "==> Building .AppImage..."
    APPIMAGE_NAME="Tasty-${VERSION}-${ARCH_RAW}.AppImage"
    APPDIR="target/AppDir"
    rm -rf "$APPDIR"
    mkdir -p "$APPDIR/usr/bin"
    stage_plugins "$APPDIR/usr/bin/plugins"
    stage_notice "$APPDIR/usr/share/licenses/tasty"
    VERSION="$VERSION" OUTPUT="$APPIMAGE_NAME" linuxdeploy \
        --appdir "$APPDIR" \
        --executable "target/$PROFILE/tasty" \
        --desktop-file assets/linux/tasty.desktop \
        --icon-file assets/icons/icon_256.png \
        --icon-filename tasty \
        --output appimage
    mv "$APPIMAGE_NAME" "$DIST_DIR/"
    APPIMAGE_FILE="$DIST_DIR/$APPIMAGE_NAME"
fi

# 헤드리스는 gui feature를 끈 빌드라 별도 target 디렉터리를 쓴다. 같은 target을 쓰면
# 위 패키지가 읽은 target/$PROFILE/tasty를 헤드리스 바이너리로 덮는다.
# plugin은 헤드리스 호스트도 실행하므로 위에서 받아 서명한 번들을 그대로 넣는다.
HEADLESS_TARGET_DIR="target/headless"
HEADLESS_PKG_DIR="tasty-headless-linux-${ARCH}"
HEADLESS_ARCHIVE_NAME="tasty-headless-${VERSION}-linux-${ARCH}.tar.gz"
echo "==> Building headless tasty ($PROFILE, --no-default-features)..."
CARGO_TARGET_DIR="$HEADLESS_TARGET_DIR" cargo build $CARGO_FLAGS --no-default-features

echo "==> Assembling headless archive..."
rm -rf "${DIST_DIR:?}/${HEADLESS_PKG_DIR:?}"
mkdir -p "$DIST_DIR/$HEADLESS_PKG_DIR"
cp "$HEADLESS_TARGET_DIR/$PROFILE/tasty" "$DIST_DIR/$HEADLESS_PKG_DIR/tasty.bin"
write_launcher "$DIST_DIR/$HEADLESS_PKG_DIR"
stage_plugins "$DIST_DIR/$HEADLESS_PKG_DIR/plugins"
stage_notice "$DIST_DIR/$HEADLESS_PKG_DIR"

echo "==> Creating $HEADLESS_ARCHIVE_NAME..."
rm -f "$DIST_DIR/$HEADLESS_ARCHIVE_NAME"
tar -czf "$DIST_DIR/$HEADLESS_ARCHIVE_NAME" -C "$DIST_DIR" "$HEADLESS_PKG_DIR"
rm -rf "${DIST_DIR:?}/${HEADLESS_PKG_DIR:?}"

echo "==> Verifying artifacts..."
# 조기 종료하는 grep/head에 producer를 직접 연결하면 pipefail에서 SIGPIPE로 실패할 수 있어 출력을 먼저 받는다.
TAR_LISTING=$(tar -tzf "$DIST_DIR/$ARCHIVE_NAME")
grep -q "$PKG_DIR/tasty" <<<"$TAR_LISTING" || {
    echo "Error: tasty not in $ARCHIVE_NAME" >&2
    exit 1
}
verify_notice_listing "$TAR_LISTING" "$PKG_DIR/" "$ARCHIVE_NAME" || exit 1
verify_bundled_plugin_listing "$TAR_LISTING" "$PKG_DIR/plugins/" "$ARCHIVE_NAME" || exit 1
VERIFY_TMP=$(mktemp -d)
tar -xzf "$DIST_DIR/$ARCHIVE_NAME" -C "$VERIFY_TMP"
"$VERIFY_TMP/$PKG_DIR/tasty" --version >/dev/null || {
    rm -rf "$VERIFY_TMP"
    echo "Error: tasty --version failed (from tar.gz)" >&2
    exit 1
}
verify_bundled_plugin_tree "$VERIFY_TMP/$PKG_DIR/plugins" "$ARCHIVE_NAME" || {
    rm -rf "$VERIFY_TMP"
    exit 1
}
rm -rf "$VERIFY_TMP"
HEADLESS_LISTING=$(tar -tzf "$DIST_DIR/$HEADLESS_ARCHIVE_NAME")
grep -q "$HEADLESS_PKG_DIR/tasty.bin" <<<"$HEADLESS_LISTING" || {
    echo "Error: tasty.bin not in $HEADLESS_ARCHIVE_NAME" >&2
    exit 1
}
grep -q "$HEADLESS_PKG_DIR/plugins/" <<<"$HEADLESS_LISTING" || {
    echo "Error: plugins not in $HEADLESS_ARCHIVE_NAME" >&2
    exit 1
}
verify_notice_listing "$HEADLESS_LISTING" "$HEADLESS_PKG_DIR/" "$HEADLESS_ARCHIVE_NAME" || exit 1
verify_bundled_plugin_listing "$HEADLESS_LISTING" "$HEADLESS_PKG_DIR/plugins/" "$HEADLESS_ARCHIVE_NAME" || exit 1
VERIFY_TMP=$(mktemp -d)
tar -xzf "$DIST_DIR/$HEADLESS_ARCHIVE_NAME" -C "$VERIFY_TMP"
"$VERIFY_TMP/$HEADLESS_PKG_DIR/tasty" --version >/dev/null || {
    rm -rf "$VERIFY_TMP"
    echo "Error: tasty --version failed (from $HEADLESS_ARCHIVE_NAME)" >&2
    exit 1
}
verify_bundled_plugin_tree "$VERIFY_TMP/$HEADLESS_PKG_DIR/plugins" "$HEADLESS_ARCHIVE_NAME" || {
    rm -rf "$VERIFY_TMP"
    exit 1
}
# 두 빌드의 feature가 다르므로 GUI 바이너리와 같은 파일이면 헤드리스 빌드가 적용되지 않은 것이다.
if cmp -s "$VERIFY_TMP/$HEADLESS_PKG_DIR/tasty.bin" "target/$PROFILE/tasty"; then
    rm -rf "$VERIFY_TMP"
    echo "Error: $HEADLESS_ARCHIVE_NAME holds the GUI binary" >&2
    exit 1
fi
rm -rf "$VERIFY_TMP"
if [[ -n "$DEB_FILE" ]]; then
    dpkg-deb -I "$DEB_FILE" >/dev/null || {
        echo "Error: dpkg-deb -I failed on $DEB_FILE" >&2
        exit 1
    }
    # dpkg-deb -c는 권한·크기가 앞에 붙으므로 경로만 나오는 payload 목록을 읽는다.
    DEB_LISTING=$(dpkg-deb --fsys-tarfile "$DEB_FILE" | tar -t) || {
        echo "Error: cannot list the payload of $DEB_FILE" >&2
        exit 1
    }
    verify_notice_listing "$DEB_LISTING" "usr/share/doc/tasty/" "$DEB_FILE" || exit 1
    verify_bundled_plugin_listing "$DEB_LISTING" "usr/lib/tasty/plugins/" "$DEB_FILE" || exit 1
fi
if [[ -n "$RPM_FILE" ]] && command -v rpm &>/dev/null; then
    rpm -qpi "$RPM_FILE" >/dev/null 2>&1 || {
        echo "Error: rpm -qpi failed on $RPM_FILE" >&2
        exit 1
    }
    RPM_LISTING=$(rpm -qpl "$RPM_FILE" 2>/dev/null)
    verify_notice_listing "$RPM_LISTING" "/usr/share/licenses/tasty/" "$RPM_FILE" flat || exit 1
    verify_bundled_plugin_listing "$RPM_LISTING" "/usr/lib/tasty/plugins/" "$RPM_FILE" || exit 1
fi
# AppImage는 실행하지 않고 파일 종류와 패키징 입력 AppDir만 확인한다.
if [[ -n "$APPIMAGE_FILE" ]]; then
    [[ -f "$APPIMAGE_FILE" ]] || {
        echo "Error: AppImage missing: $APPIMAGE_FILE" >&2
        exit 1
    }
    appimage_type=$(file "$APPIMAGE_FILE")
    grep -q "ELF" <<<"$appimage_type" || {
        echo "Error: AppImage is not an ELF binary: $APPIMAGE_FILE" >&2
        exit 1
    }
    # 완성 AppImage 내부가 아니라 linuxdeploy에 넘긴 AppDir의 notice 파일을 검사한다.
    verify_notice_tree "$APPDIR/usr/share/licenses/tasty" "$APPDIR" || exit 1
    verify_bundled_plugin_tree "$APPDIR/usr/bin/plugins" "$APPDIR" || exit 1
fi

SHASUMS_FILE="SHA256SUMS-linux-${ARCH}.txt"
(
    cd "$DIST_DIR"
    {
        sha256sum "$ARCHIVE_NAME"
        sha256sum "$HEADLESS_ARCHIVE_NAME"
        # --debug 에는 deb·rpm·AppImage 가 없다. `[[ ]] &&` 가 블록 마지막에서 거짓이면 set -e 로 스크립트가 끝난다.
        if [[ -n "$DEB_FILE" ]]; then sha256sum "$(basename "$DEB_FILE")"; fi
        if [[ -n "$RPM_FILE" ]]; then sha256sum "$(basename "$RPM_FILE")"; fi
        if [[ -n "$APPIMAGE_FILE" ]]; then sha256sum "$(basename "$APPIMAGE_FILE")"; fi
    } > "$SHASUMS_FILE"
)

echo ""
echo "Done!"
echo "  Archive:  $DIST_DIR/$ARCHIVE_NAME"
echo "  Headless: $DIST_DIR/$HEADLESS_ARCHIVE_NAME"
if [[ -n "$DEB_FILE" ]]; then echo "  Deb:      $DEB_FILE"; fi
if [[ -n "$RPM_FILE" ]]; then echo "  Rpm:      $RPM_FILE"; fi
if [[ -n "$APPIMAGE_FILE" ]]; then echo "  AppImage: $APPIMAGE_FILE"; fi
echo "  SHA:      $DIST_DIR/$SHASUMS_FILE"
