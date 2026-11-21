# 빌드·플러그인 준비·개발 실행·배포 명령. 전체 목록은 just --list로 확인한다.
# build/run은 기본 debug, build-plugins/build-all은 기본 release를 사용한다.
# build/run에 --release를 주거나 PROFILE 환경변수로 프로필을 선택할 수 있다.
# install은 현재 macOS만 지원한다.
#
# Windows의 Bash 레시피에는 Git for Windows와 cygpath가 필요하다.
# C:\Program Files\Git\usr\bin을 PATH에 포함한다.
# shebang은 /bin/bash를 사용한다. /usr/bin/env bash는 WSL의 bash를 찾을 수 있다.

# 사용 가능한 명령을 표시한다.
default:
    @just --list

# 호스트 OS에 맞는 배포 패키지를 빌드한다.
dist:
    #!/bin/bash
    set -euo pipefail
    case "$(uname -s)" in
        Darwin)
            exec ./scripts/build-macos-dmg.sh ;;
        Linux)
            exec ./scripts/build-linux.sh ;;
        MINGW*|MSYS*|CYGWIN*)
            exec pwsh -File ./scripts/build-windows.ps1 ;;
        *)
            echo "Unsupported OS: $(uname -s)" >&2
            exit 1 ;;
    esac

# 플러그인은 호스트 시작 시 번들 설치 정책에 따라 동기화한다.

# macOS 앱 번들을 빌드해 /Applications에 설치한다.
# 개발용 명령이 아니라서 빌드 대상 크레이트에서 cap이 적용되는 rustc lint를 무시한다(--cap-lints=allow).
# build.rs의 lint와 cargo 자체 경고는 그대로다. 자세한 범위는 docs/dev-guide/build.md.
install:
    #!/bin/bash
    set -euo pipefail
    case "$(uname -s)" in
        Darwin)
            exec ./scripts/install-macos.sh ;;
        Linux)
            echo "Linux 자동 설치는 아직 미구현입니다." >&2
            echo "  just dist-linux 로 산출물(.deb/.rpm/AppImage)을 빌드한 뒤 수동 설치하세요." >&2
            exit 1 ;;
        MINGW*|MSYS*|CYGWIN*)
            echo "Windows 자동 설치는 아직 미구현입니다." >&2
            echo "  just dist-windows 로 .msi 를 빌드한 뒤 설치하세요." >&2
            exit 1 ;;
        *)
            echo "Unsupported OS: $(uname -s)" >&2
            exit 1 ;;
    esac

# macOS 배포 패키지를 빌드한다.
dist-macos:
    ./scripts/build-macos-dmg.sh

# Linux 배포 패키지를 빌드한다.
dist-linux:
    ./scripts/build-linux.sh

# Windows x64 배포 패키지를 빌드한다.
dist-windows:
    pwsh -File ./scripts/build-windows.ps1 -Arch x64

# Windows ARM64 배포 패키지를 빌드한다. x64 호스트에서는 교차 빌드한다.
dist-windows-arm64:
    pwsh -File ./scripts/build-windows.ps1 -Arch arm64

# 배포 결과와 패키징 중간 파일을 삭제한다.
dist-clean:
    rm -rf dist target/debian target/generate-rpm target/generate-rpm-assets target/AppDir

# Linux 패키징 도구를 설치한다. apt 단계에는 sudo 권한이 필요하다.
dist-setup-linux:
    #!/bin/bash
    set -euo pipefail
    sudo apt install -y cmake pkg-config libfreetype6-dev libfontconfig1-dev rpm cpio binutils python3
    cargo install cargo-deb cargo-generate-rpm
    if ! command -v linuxdeploy &>/dev/null; then
        mkdir -p "$HOME/.local/bin"
        curl -fsSL -o "$HOME/.local/bin/linuxdeploy" \
            "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$(uname -m).AppImage"
        chmod +x "$HOME/.local/bin/linuxdeploy"
        echo "linuxdeploy installed to ~/.local/bin (ensure on PATH)"
    fi

# 플러그인·build-all 명령의 기본 프로필. build/run은 별도로 debug를 기본값으로 쓴다.
PROFILE := env_var_or_default('PROFILE', 'release')

# TASTY_E2E_BIN을 허용하는 테스트에서만 사용할 수 있다.
# 사용법: BIN=$(just e2e-headless-bin) && export TASTY_E2E_BIN=$BIN

# e2e용 headless 바이너리를 빌드하고 경로를 출력한다.
e2e-headless-bin:
    @scripts/build-e2e-headless.sh

# --from-checkout [DIR]은 공식 플러그인 레포 체크아웃(기본 ../tasty-official-plugins)에서 빌드하고,
# --with-local-tasty를 더하면 그 빌드가 이 체크아웃의 tasty 패키지를 쓴다. 자세한 것은 scripts/stage-dev-plugins.sh.

# 공식 플러그인(잠금 파일의 릴리스 산출물)과 mesh-demo를 실행 파일 옆 builtin-plugins에 준비한다.
build-plugins *ARGS:
    #!/bin/bash
    set -euo pipefail
    exec /bin/bash ./scripts/stage-dev-plugins.sh --profile "{{PROFILE}}" {{ARGS}}

# 플러그인을 준비한 뒤 본체를 빌드한다.
build-all: build-plugins
    #!/bin/bash
    set -euo pipefail
    profile="{{PROFILE}}"
    case "$profile" in
        release) profile_flag="--release" ;;
        debug)   profile_flag="" ;;
        *)       profile_flag="--profile $profile" ;;
    esac
    cargo build $profile_flag --bin tasty

# 사용자 설치 폴더에는 다음 호스트 시작 때 번들 정책에 따라 반영한다.

# 본체·플러그인을 빌드하고 번들을 준비한다. 호스트는 실행하지 않는다.
build *ARGS:
    #!/bin/bash
    set -euo pipefail
    profile="${PROFILE:-debug}"
    for arg in {{ARGS}}; do
        case "$arg" in
            --release) profile="release" ;;
            --debug)   profile="debug" ;;
            *) echo "build: 알 수 없는 인자 '$arg'" >&2; exit 1 ;;
        esac
    done
    case "$profile" in
        release) profile_flag="--release" ;;
        debug)   profile_flag="" ;;
        *)       profile_flag="--profile $profile" ;;
    esac
    PROFILE="$profile" just build-plugins
    cargo build $profile_flag --bin tasty

# --release/--debug는 프로필로 해석하고 나머지 인자는 호스트에 전달한다.
# 호스트는 같은 버전의 변경 파일을 동기화하며 더 높은 설치 버전은 유지한다.

# 본체·플러그인을 빌드하고 호스트를 실행한다.
run *ARGS:
    #!/bin/bash
    set -euo pipefail
    profile="${PROFILE:-debug}"
    passthrough=()
    for arg in {{ARGS}}; do
        case "$arg" in
            --release) profile="release" ;;
            --debug)   profile="debug" ;;
            *)         passthrough+=("$arg") ;;
        esac
    done
    case "$profile" in
        release) profile_flag="--release" ;;
        debug)   profile_flag="" ;;
        *)       profile_flag="--profile $profile" ;;
    esac
    PROFILE="$profile" just build-plugins
    # Bash 3.2의 set -u에서 빈 배열을 펼치면 오류가 나므로 분기한다.
    if [ "${#passthrough[@]}" -gt 0 ]; then
        cargo run $profile_flag --bin tasty -- "${passthrough[@]}"
    else
        cargo run $profile_flag --bin tasty
    fi

# 배포 파일의 SHA256SUMS를 다시 확인한다.
dist-verify:
    #!/bin/bash
    set -euo pipefail
    cd dist
    case "$(uname -s)" in
        Darwin)
            shasum -a 256 --check SHA256SUMS-macos.txt ;;
        Linux)
            ARCH_RAW=$(uname -m)
            case "$ARCH_RAW" in
                x86_64)  ARCH=x64 ;;
                aarch64) ARCH=arm64 ;;
                *) echo "Unsupported architecture: $ARCH_RAW" >&2; exit 1 ;;
            esac
            sha256sum --check "SHA256SUMS-linux-${ARCH}.txt" ;;
        *)
            echo "On Windows, run: Get-FileHash dist\\*.zip,dist\\*.msi -Algorithm SHA256" >&2
            exit 1 ;;
    esac
