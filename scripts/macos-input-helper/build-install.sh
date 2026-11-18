#!/bin/bash
# 개발용 macOS 키 입력 도우미(TastyInputHelper.app)를 빌드·서명해 설치한다. 배포 패키지에는 들어가지 않는다.
# 사용: ./scripts/macos-input-helper/build-install.sh
# TASTY_CODESIGN_IDENTITY(기본 'Tasty Dev')로 서명한다. identity가 없으면 ad-hoc으로 내려가지 않고 실패한다.
# TASTY_INPUT_HELPER_APP으로 설치 경로를 바꿀 수 있다(기본 ~/Applications/TastyInputHelper.app).
#
# 재빌드 뒤에도 손쉬운 사용 허용이 유지되려면 세 조건이 모두 같아야 한다.
# 번들 ID(Info.plist), 서명한 leaf 인증서(인증서를 지우고 다시 만들면 바뀐다), 설치 경로.
# 서명 요구(designated requirement)는 cdhash가 아니라 번들 ID와 leaf 인증서로 정해진다.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Error: This script must be run on macOS." >&2
    exit 1
fi

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="TastyInputHelper"
BUNDLE_ID="com.zilhak.tasty.input-helper"
IDENTITY="${TASTY_CODESIGN_IDENTITY:-Tasty Dev}"
INSTALL_APP="${TASTY_INPUT_HELPER_APP:-$HOME/Applications/$APP_NAME.app}"

if [[ "$IDENTITY" == "-" ]]; then
    echo "Error: ad-hoc 서명은 재빌드마다 손쉬운 사용 허용이 풀리므로 쓰지 않는다." >&2
    exit 1
fi
# security가 실패하면 그 결과를 identity 없음으로 판정하지 않고 오류로 끝낸다.
if ! identities=$(security find-identity -v -p codesigning); then
    echo "Error: security find-identity failed." >&2
    exit 1
fi
if ! grep -Fq "\"$IDENTITY\"" <<<"$identities"; then
    echo "Error: 코드 서명 identity '$IDENTITY' 이(가) 키체인에 없다." >&2
    echo "  ./scripts/macos-codesign-identity.sh 로 조회하고 --create 로 발급한다." >&2
    exit 1
fi

command -v swiftc &>/dev/null || {
    echo "Error: swiftc not found. Xcode Command Line Tools 가 필요하다." >&2
    exit 1
}

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT
APP_DIR="$WORK_DIR/$APP_NAME.app"
mkdir -p "$APP_DIR/Contents/MacOS"
cp "$SRC_DIR/Info.plist" "$APP_DIR/Contents/Info.plist"

echo "==> Building $APP_NAME..."
swiftc -O "$SRC_DIR/main.swift" -o "$APP_DIR/Contents/MacOS/$APP_NAME"

echo "==> Signing with identity '$IDENTITY'..."
codesign --force --sign "$IDENTITY" "$APP_DIR"

# linker의 자동 서명과 구별하려고 bundle 봉인·Identifier·linker-signed 여부를 함께 확인한다.
echo "==> Verifying signature..."
if [[ ! -d "$APP_DIR/Contents/_CodeSignature" ]]; then
    echo "Error: $APP_NAME.app has no _CodeSignature (codesign did not run)" >&2
    exit 1
fi
CODESIGN_INFO=$(codesign -dv "$APP_DIR" 2>&1 || true)
if ! grep -Fxq "Identifier=$BUNDLE_ID" <<<"$CODESIGN_INFO"; then
    echo "Error: signing identifier is not $BUNDLE_ID:" >&2
    grep "^Identifier=" <<<"$CODESIGN_INFO" >&2 || true
    exit 1
fi
if grep -q "linker-signed" <<<"$CODESIGN_INFO" || grep -q "Signature=adhoc" <<<"$CODESIGN_INFO"; then
    echo "Error: $APP_NAME.app is not signed with a certificate" >&2
    exit 1
fi
if ! codesign --verify --deep --strict "$APP_DIR"; then
    echo "Error: $APP_NAME.app failed codesign --verify" >&2
    exit 1
fi

# 같은 경로를 유지한 채 내용만 바꾼다.
echo "==> Installing to $INSTALL_APP..."
mkdir -p "$(dirname "$INSTALL_APP")"
rm -rf "$INSTALL_APP"
ditto "$APP_DIR" "$INSTALL_APP"
codesign --verify --deep --strict "$INSTALL_APP"

echo ""
codesign -dvvv "$INSTALL_APP" 2>&1 | grep -E '^(Identifier|Authority|CDHash)='
echo "designated requirement:"
codesign -d -r- "$INSTALL_APP" 2>&1 | grep -v '^Executable='
echo ""
echo "완료. 시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용에서 이 앱을 허용한다: $INSTALL_APP"
