# 설치

Tasty 는 GitHub Releases 에서 OS·아키텍처·설치 형태별 산출물로 배포된다. 자동 설치 스크립트나 패키지 매니저 통합은 제공하지 않는다 — 모든 설치는 [릴리스 페이지](https://github.com/zilhak/tasty/releases)에서 직접 받는다.

## 산출물 매트릭스

| OS | 아키텍처 | 산출물 | 비고 |
|----|---------|--------|------|
| Linux | x86_64 | `tasty-{ver}-linux-x64.tar.gz` | 동적 링크 · glibc/freetype/fontconfig/GTK 등 시스템 라이브러리 필요 |
| Linux | x86_64 | `tasty_{ver}-1_amd64.deb` | Debian/Ubuntu/Mint |
| Linux | x86_64 | `tasty-{ver}-1.x86_64.rpm` | Fedora/RHEL/openSUSE |
| Linux | x86_64 | `Tasty-{ver}-x86_64.AppImage` | distro 무관 단일 파일 |
| Linux | aarch64 | `tasty-{ver}-linux-arm64.tar.gz` / `_arm64.deb` / `.aarch64.rpm` / `-aarch64.AppImage` | ARM64 |
| macOS | arm64 | `Tasty-{ver}-macos-arm64.dmg` | App Bundle 드래그 설치 (Apple Silicon 전용) |
| Windows | x86_64 | `tasty-{ver}-windows-x64.zip` | 바이너리만 압축 |
| Windows | x86_64 | `tasty-{ver}-windows-x64.msi` | 시작 메뉴/제거 등록 인스톨러 |

## Linux

```bash
# .deb (Debian/Ubuntu/Mint)
sudo apt install ./tasty_{ver}-1_amd64.deb      # 또는 dpkg -i + apt-get install -f

# .rpm (Fedora/RHEL/openSUSE)
sudo dnf install ./tasty-{ver}-1.x86_64.rpm

# .AppImage (distro 무관, 설치 불요)
chmod +x Tasty-{ver}-x86_64.AppImage && ./Tasty-{ver}-x86_64.AppImage
# FUSE 미지원 환경: ./Tasty-*.AppImage --appimage-extract-and-run

# .tar.gz (수동)
tar -xzf tasty-{ver}-linux-x64.tar.gz && ./tasty-linux-x64/tasty
```

- `.deb`/`.rpm`: `tasty` 가 PATH 등록 + 데스크톱 메뉴 아이콘 등록. 의존성은 패키지 메타데이터(`libfreetype6`/`libfontconfig1`/`libgtk-3`/`libwebkit2gtk-4.1` 등)로 자동 분석. GPU 가속(Vulkan)은 `libvulkan1`/`vulkan-loader` 를 Recommends 로만 요구 — 없어도 설치·실행은 되고 소프트웨어 렌더러로 fallback 한다.
- `.AppImage`: 의존 라이브러리를 모두 번들. 데스크톱 메뉴 등록은 수동(`appimaged` 또는 `.desktop` 을 `~/.local/share/applications/`).
- `.tar.gz`: PATH·메뉴 등록 사용자 직접. 편한 등록을 원하면 `.deb`/`.rpm`/`.AppImage` 권장. 필요 `.so` 가
  없으면 실행 전 `tasty` wrapper 가 감지해 안내 후 종료한다(`tasty.bin` 이 실제 바이너리).
- **최소 glibc**: 빌드 환경(Ubuntu 24.04, glibc 2.39)보다 오래된 배포판(Ubuntu 20.04/Debian 11 등)은
  `tasty: /lib/x86_64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found` 로 실행이 안 될 수 있다.
  구배포판 지원을 위한 별도 빌드는 제공하지 않는다 — 필요하면 소스 빌드([dev-guide/build](dev-guide/build.md)).

## macOS

`.dmg` 더블클릭 → `Tasty.app` 을 `Applications` 로 드래그. 첫 실행 시 Gatekeeper 경고가 나오면 시스템 설정 > 개인정보 보호 및 보안에서 허용(코드 사인 미인증 빌드 한정).

터미널에서 바로 우회하려면:

```bash
xattr -dr com.apple.quarantine /Applications/Tasty.app
```

## Windows

```powershell
# .msi (권장) — 더블클릭 → 설치 마법사. 시작 메뉴 바로가기 + 프로그램 추가/제거 등록
# .zip (수동)
Expand-Archive tasty-{ver}-windows-x64.zip -DestinationPath tasty; .\tasty\tasty.exe
```

### SmartScreen 경고 우회

Authenticode 서명이 없어 첫 실행 시 "Windows의 PC 보호" 경고가 뜬다. **추가 정보 → 그래도 실행**을 선택하면 실행된다.

### 제거 (.msi)

"설정 > 앱" 또는 제어판 > 프로그램 제거에서 Tasty 를 제거한다(`msiexec /x` 등록됨). 제거 시:

- `Program Files\tasty\` 의 바이너리·플러그인 일체, 시작 메뉴 바로가기, PATH 항목이 정리된다.
- **사용자 데이터(`~/.tasty/`)도 함께 전부 삭제된다** — config·세션·테마·런타임이 복사한 플러그인 사본까지 모두. 정책상 완전 제거를 기본으로 한다(잔존 플러그인 사본이 재설치 시 trust 를 깨뜨리는 문제 방지). 보존하려면 제거 전에 `~/.tasty/` 를 백업한다.
- **업그레이드(같은 제품 재설치)는 `~/.tasty/` 를 보존한다** — 데이터 삭제는 진짜 제거(`REMOVE="ALL"`)일 때만 일어난다.

> `.zip` 은 설치 개념이 없으므로 압축 해제한 폴더를 지우면 끝. 단 `~/.tasty/` 사용자 데이터는 수동 삭제해야 한다.

빌드 측면은 [dev-guide/build](dev-guide/build.md) "Windows MSI".

## GPU 요구사항

Tasty 는 GPU 가속 렌더링(wgpu, Vulkan/DX12/Metal)을 쓴다. 하드웨어 GPU 어댑터가 없으면(GPU 미탑재 서버·VM·컨테이너 등) 소프트웨어 렌더러로 한 번 더 시도하고, 그마저 없으면 안내 메시지를 낸 뒤 종료한다. 위 배포 산출물은 모두 GUI 빌드다 — `--headless` 플래그는 받지만 GUI 빌드에서는 효과가 없어 경고 한 줄을 남기고 평소처럼 GUI 로 실행된다. 헤드리스 여부는 플래그가 아니라 빌드가 정하므로, GPU 없이 IPC/CLI 만 쓰려면 소스에서 `cargo build --no-default-features` 로 headless 빌드해야 한다([dev-guide/build](dev-guide/build.md)).

## 검증

```bash
tasty --version       # 버전
tasty list info       # GUI 인스턴스가 떠 있을 때 시스템 정보 IPC
```

## 설치 위치

| OS | 바이너리 | 사용자 데이터 |
|----|---------|--------------|
| Linux | `/usr/bin/tasty`(.deb/.rpm) 또는 추출 위치 | `~/.tasty/` |
| macOS | `/Applications/Tasty.app/Contents/MacOS/tasty` | `~/.tasty/` |
| Windows | `C:\Program Files\tasty\bin\tasty.exe`(.msi, perMachine) | `~/.tasty/` |

`~/.tasty/` 에 `tasty.port`(IPC 포트)·`config.toml`·세션 등 사용자 데이터가 들어간다(전체 지도: [design/systems/storage](design/systems/storage.md), OS별 경로: [reference/environments](reference/environments.md)).

## 패키지 빌드 (메인테이너)

[dev-guide/build](dev-guide/build.md) · [dev-guide/build › dist 빌드 명령](dev-guide/build.md#dist-빌드-명령-카탈로그) · [dev-guide/release › 러너](dev-guide/release.md#러너).
