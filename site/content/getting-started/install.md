# 설치

내 컴퓨터에 맞는 Tasty를 설치하고 첫 터미널을 열어보세요. 아래에서 OS별 설치 방법과 업데이트, 제거 방법을 확인할 수 있습니다.

설치 파일은 [GitHub 릴리스 페이지](https://github.com/zilhak/tasty/releases)에서 받으세요. 현재는 설치 스크립트나 패키지 매니저(apt 저장소, Homebrew, winget 등)를 통한 설치는 제공하지 않습니다.

## 설치 파일 고르기

`{ver}` 자리에는 버전(예 `0.10.3`)이 들어갑니다.

| OS | 아키텍처 | 파일 | 설명 |
|----|---------|------|------|
| Linux | x86_64 | `tasty_{ver}-1_amd64.deb` | Debian / Ubuntu / Mint |
| Linux | x86_64 | `tasty-{ver}-1.x86_64.rpm` | Fedora / RHEL / openSUSE |
| Linux | x86_64 | `Tasty-{ver}-x86_64.AppImage` | 배포판 무관 단일 파일 |
| Linux | x86_64 | `tasty-{ver}-linux-x64.tar.gz` | 수동 설치용 압축 파일 |
| Linux | aarch64 | `_arm64.deb` / `.aarch64.rpm` / `-aarch64.AppImage` / `-linux-arm64.tar.gz` | ARM64 |
| macOS | Apple Silicon | `Tasty-{ver}-macos-arm64.dmg` | 드래그 설치. Intel Mac 용 빌드는 없습니다 |
| Windows | x86_64 | `tasty-{ver}-windows-x64.msi` | 설치 마법사 (권장) |
| Windows | x86_64 | `tasty-{ver}-windows-x64.zip` | 압축만 풀어 쓰는 무설치판 |

각 OS 의 `SHA256SUMS-*.txt` 로 받은 파일을 검증할 수 있습니다.

## Linux

```sh
# .deb (Debian / Ubuntu / Mint)
sudo apt install ./tasty_{ver}-1_amd64.deb

# .rpm (Fedora / RHEL / openSUSE)
sudo dnf install ./tasty-{ver}-1.x86_64.rpm

# .AppImage — 설치 없이 실행
chmod +x Tasty-{ver}-x86_64.AppImage && ./Tasty-{ver}-x86_64.AppImage
# FUSE 가 없는 환경이면
./Tasty-{ver}-x86_64.AppImage --appimage-extract-and-run

# .tar.gz — 압축 해제 후 바로 실행
tar -xzf tasty-{ver}-linux-x64.tar.gz && ./tasty-linux-x64/tasty
```

- `.deb` / `.rpm` 은 `tasty` 명령을 PATH 에 등록하고 앱 메뉴에 아이콘을 넣습니다. 필요한 라이브러리는 패키지가 자동으로 끌어옵니다.
- GPU 가속(Vulkan)은 `libvulkan1` / `vulkan-loader` 가 있을 때 씁니다. 없어도 설치·실행은 되고 OpenGL 로, 그것도 안 되면 소프트웨어 렌더링으로 동작합니다.
- `.AppImage` 는 라이브러리를 모두 포함합니다. 앱 메뉴 등록은 직접 합니다(`appimaged` 사용 또는 `.desktop` 파일을 `~/.local/share/applications/` 에 두기).
- `.tar.gz` 는 PATH 등록과 메뉴 등록을 직접 해야 합니다. 필요한 시스템 라이브러리가 없으면 `tasty` 실행 시 무엇이 빠졌는지 안내하고 종료합니다.
- 라이선스 고지(Tasty 의 MIT 본문 · 함께 번들하는 폰트와 markdown 렌더링 엔진의 라이선스 본문 · 제3자 고지 문서)는 산출물 안에 함께 들어 있습니다. `.deb` 은 `/usr/share/doc/tasty/`, `.rpm` 과 `.AppImage` 는 `usr/share/licenses/tasty/`, `.tar.gz` 는 압축을 푼 디렉토리 최상단입니다. 릴리스 페이지에서도 따로 내려받을 수 있도록 해 두었지만, 그렇게 올라간 릴리스는 아직 없습니다.
- 빌드 기준이 Ubuntu 24.04(glibc 2.39)라서 그보다 오래된 배포판(Ubuntu 20.04, Debian 11 등)에서는 `GLIBC_2.39 not found` 오류로 실행되지 않을 수 있습니다. 구배포판용 빌드는 따로 제공하지 않습니다.

## macOS

1. `.dmg` 를 열고 `Tasty.app` 을 `Applications` 폴더로 드래그합니다.
2. 처음 실행할 때 "확인되지 않은 개발자" 경고가 뜨면 **시스템 설정 > 개인정보 보호 및 보안** 에서 허용합니다.

터미널에서 바로 경고를 없애려면:

```sh
xattr -dr com.apple.quarantine /Applications/Tasty.app
```

첫 실행 직후에는 macOS 권한 프롬프트(다운로드 · 문서 · 데스크탑 폴더, 화면 기록)가 차례로 뜹니다. 왜 뜨는지와 어떻게 답할지는 [문제 해결](../help/troubleshooting.md#macos-권한-프롬프트) 에 있습니다.

## Windows

- **`.msi` (권장)** — 더블클릭해서 설치 마법사를 따릅니다. 시작 메뉴 바로가기와 "앱 및 기능" 제거 항목이 등록됩니다.
- **`.zip`** — 원하는 폴더에 풀고 `tasty.exe` 를 실행합니다.

```powershell
Expand-Archive tasty-{ver}-windows-x64.zip -DestinationPath tasty
.\tasty\tasty.exe
```

첫 실행 시 "Windows 의 PC 보호" 경고가 뜹니다(코드 서명이 없어서입니다). **추가 정보 → 실행** 을 누르면 됩니다.

Windows 에서 Tasty 는 **Git Bash** 를 셸로 씁니다. Git for Windows 가 없으면 설정 윈도우에서 "Git Bash를 찾을 수 없습니다" 안내가 뜨므로, 먼저 설치하거나 **설정** <!-- en: Settings --> > **터미널** <!-- en: Terminal --> > **셸** <!-- en: Shell --> 에서 bash 경로를 직접 지정합니다.

## 첫 실행

- 윈도우가 열리면 워크스페이스 하나와 터미널 하나가 준비돼 있습니다. 화면 구성은 [첫 화면 둘러보기](first-look.md).
- UI 언어의 기본값은 영어입니다. 한국어로 바꾸려면 **설정** > **일반** <!-- en: General --> > **언어** <!-- en: Language --> 에서 한국어를 고르고 저장한 뒤 Tasty 를 다시 시작합니다. `~/.tasty/config.toml` 에 직접 쓸 수도 있습니다:

```toml
[general]
language = "ko"
```

터미널에서 설치가 잘 됐는지 확인합니다. 두 번째 명령은 Tasty 가 실행 중이어야 응답합니다.

```sh
tasty --version      # 예: tasty 0.10.3
tasty list info      # 실행 중인 Tasty 의 버전 · 워크스페이스 수
```

Tasty 안에서 연 셸에는 `tasty` 명령이 자동으로 PATH 에 들어갑니다. macOS 에서 다른 터미널 앱이나 스크립트에서도 쓰려면 `/Applications/Tasty.app/Contents/MacOS/tasty` 를 PATH 에 넣거나 심볼릭 링크를 만듭니다. Windows `.zip` 판도 마찬가지로 압축 푼 폴더를 PATH 에 넣습니다.

## GPU 요구사항

Tasty 는 GPU(Vulkan / DirectX 12 / Metal)로 화면을 그립니다. GPU 가 없으면 소프트웨어 렌더러로 한 번 더 시도하고, 그것도 안 되면 "GPU 어댑터를 찾을 수 없음" 메시지를 내고 종료합니다. GPU 드라이버를 설치·업데이트하면 대부분 해결됩니다. 배포되는 설치 파일은 모두 GUI 빌드라 GPU 없는 서버에서는 실행되지 않습니다.

## headless 빌드

윈도우 없이 서버에서 Tasty 를 돌리는 **headless 빌드**는 설치 파일로 배포하지 않습니다. 배포된 Tasty 에 `--headless` 를 붙여 실행해도 윈도우 없이 뜨지 않고 평소처럼 GUI 로 실행됩니다.

headless 빌드가 필요하면 Rust 가 설치된 환경에서 소스로 직접 만듭니다.

```sh
git clone https://github.com/zilhak/tasty.git
cd tasty
cargo build --release --no-default-features
./target/release/tasty
```

이렇게 만든 `tasty` 는 옵션 없이 실행해도 윈도우 없이 뜨고, 터미널은 `tasty` CLI 로만 만들고 다룹니다. 다른 컴퓨터에서 [원격 attach](../remote/attach.md) 로 그 워크스페이스를 가져와 볼 수 있습니다.

## 업데이트

자동 업데이트나 새 버전 알림 기능은 없습니다. 릴리스 페이지에서 새 버전을 받아 같은 방식으로 다시 설치합니다.

- Linux `.deb` / `.rpm`: 새 파일로 같은 명령을 다시 실행하면 덮어 설치됩니다.
- macOS: 새 `Tasty.app` 을 `Applications` 에 덮어씁니다.
- Windows `.msi`: 새 `.msi` 를 실행하면 업그레이드됩니다. 사용자 데이터(`~/.tasty/`)는 그대로 남습니다.

설정 · 세션 · 테마는 모두 `~/.tasty/` 에 있으므로 업데이트해도 유지됩니다.

## 제거

| OS | 프로그램 제거 | 사용자 데이터 |
|----|--------------|--------------|
| Linux `.deb` | `sudo apt remove tasty` | `~/.tasty/` 는 남습니다. 직접 지웁니다 |
| Linux `.rpm` | `sudo dnf remove tasty` | 위와 같습니다 |
| Linux AppImage / tar.gz | 파일 · 폴더를 지웁니다 | 위와 같습니다 |
| macOS | `Tasty.app` 을 휴지통으로 | 위와 같습니다 |
| Windows `.msi` | **설정 > 앱** 에서 Tasty 제거 | **`~/.tasty/` 까지 함께 삭제됩니다** — 설정 · 세션 · 테마를 남기려면 제거 전에 백업합니다 |
| Windows `.zip` | 압축 푼 폴더를 지웁니다 | `~/.tasty/` 는 남습니다. 직접 지웁니다 |

Windows 에서 `~` 는 `%USERPROFILE%`(보통 `C:\Users\<이름>`)입니다.

## 설치 위치

| OS | 실행 파일 | 사용자 데이터 |
|----|----------|--------------|
| Linux | `/usr/bin/tasty` (`.deb` / `.rpm`) 또는 압축 푼 위치 | `~/.tasty/` |
| macOS | `/Applications/Tasty.app/Contents/MacOS/tasty` | `~/.tasty/` |
| Windows | `C:\Program Files\tasty\bin\tasty.exe` (`.msi`) | `%USERPROFILE%\.tasty\` |

`~/.tasty/` 에는 설정(`config.toml`), 실행 중인 Tasty 의 IPC 포트(`tasty.port`), 레이아웃, 테마, 로그가 들어갑니다. 주요 파일은 [설정](../customize/settings.md) 과 [문제 해결](../help/troubleshooting.md) 에서 다룹니다.
