# 설치

Tasty는 GitHub Releases 에서 OS·아키텍처·설치 형태별 산출물로 배포된다. 자동 설치 스크립트나 패키지 매니저 통합은 제공하지 않는다 — 모든 설치는 [릴리스 페이지](https://github.com/zilhak/tasty/releases)에서 직접 받는다.

## 산출물 매트릭스

| OS | 아키텍처 | 산출물 | 비고 |
|----|---------|--------|------|
| Linux | x86_64 | `tasty-{ver}-linux-x64.tar.gz` | 동적 링크 · glibc/freetype/fontconfig/GTK 등 시스템 라이브러리 필요 |
| Linux | x86_64 | `tasty_{ver}-1_amd64.deb` | Debian/Ubuntu/Mint |
| Linux | x86_64 | `tasty-{ver}-1.x86_64.rpm` | Fedora/RHEL/openSUSE |
| Linux | x86_64 | `Tasty-{ver}-x86_64.AppImage` | 의존 라이브러리를 묶은 단일 파일 |
| Linux | aarch64 | `tasty-{ver}-linux-arm64.tar.gz` / `_arm64.deb` / `.aarch64.rpm` / `-aarch64.AppImage` | ARM64 |
| macOS | arm64 | `Tasty-{ver}-macos-arm64.dmg` | App Bundle 드래그 설치 (Apple Silicon 전용) |
| Windows | x86_64 | `tasty-{ver}-windows-x64.zip` | 바이너리만 압축 |
| Windows | x86_64 | `tasty-{ver}-windows-x64.msi` | 시작 메뉴/제거 등록 인스톨러 |
| Linux | x86_64 · aarch64 | `tasty-headless-{ver}-linux-{x64,arm64}.tar.gz` | 헤드리스 빌드(`gui` feature 없음) · 번들 plugin 포함 |
| macOS | arm64 | `tasty-headless-{ver}-macos-arm64.tar.gz` | 헤드리스 빌드 |
| Windows | x86_64 | `tasty-headless-{ver}-windows-x64.zip` | 헤드리스 빌드 |
| Windows | ARM64 | `tasty-{ver}-windows-arm64.zip` / `.msi` / `tasty-headless-{ver}-windows-arm64.zip` | 지원 대상([플랫폼 원칙](principles/platform-and-compatibility.md)). 0.12.2 보다 새 릴리스부터 포함한다. Windows ARM 기기에서 설치·실행은 미측정([릴리스](dev-guide/release.md#windows-arm64)). 0.12.2 이하의 x64 산출물을 Windows ARM 기기에서 실행하는 것도 확인하지 않았다 |

## Linux

```bash
# .deb (Debian/Ubuntu/Mint)
sudo apt install ./tasty_{ver}-1_amd64.deb      # 또는 dpkg -i + apt-get install -f

# .rpm (Fedora/RHEL/openSUSE)
sudo dnf install ./tasty-{ver}-1.x86_64.rpm

# .AppImage (설치 없이 실행)
chmod +x Tasty-{ver}-x86_64.AppImage && ./Tasty-{ver}-x86_64.AppImage
# FUSE 미지원 환경: ./Tasty-*.AppImage --appimage-extract-and-run

# .tar.gz (수동)
tar -xzf tasty-{ver}-linux-x64.tar.gz && ./tasty-linux-x64/tasty
```

- `.deb`/`.rpm`: `tasty` 가 PATH 등록 + 데스크톱 메뉴 아이콘 등록. 의존성은 패키지 메타데이터(`libfreetype6`/`libfontconfig1`/`libgtk-3`/`libwebkit2gtk-4.1` 등)로 자동 분석. GPU 가속(Vulkan)은 `libvulkan1`/`vulkan-loader` 를 Recommends 로만 요구 — 없어도 설치·실행은 되고 소프트웨어 렌더러로 fallback 한다. `zenity` 도 deb·rpm 모두 Recommends 다 — 두 번째 실행이 실행 중 Tasty에 요청을 넘기지 못했을 때의 메시지 상자(rfd)가 쓰며, 없으면 데스크톱 알림으로 대신한다. 데스크톱 항목은 `StartupNotify=true` 라 실행기가 활성화 토큰을 넘긴다.
- `.AppImage`: 필요한 라이브러리를 번들. 데스크톱 메뉴 등록은 수동(`appimaged` 또는 `.desktop` 을 `~/.local/share/applications/`).
- `.tar.gz`: PATH·메뉴 등록 사용자 직접. 편한 등록을 원하면 `.deb`/`.rpm`/`.AppImage` 권장. 필요 `.so` 가
  없으면 실행 전 `tasty` wrapper 가 감지해 안내 후 종료한다(`tasty.bin` 이 실제 바이너리).
- **최소 glibc**: 빌드 환경(Ubuntu 24.04, glibc 2.39)보다 오래된 배포판(Ubuntu 20.04/Debian 11 등)은
  `tasty: /lib/x86_64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found` 로 실행이 안 될 수 있다.
  구배포판 지원을 위한 별도 빌드는 제공하지 않는다 — 필요하면 소스 빌드([dev-guide/build](dev-guide/build.md)).
- **최소 WebKitGTK**: HTML·Markdown 뷰어가 WebKitGTK 2.40 API를 사용한다(`Cargo.toml`의 `webkit2gtk` feature `v2_40`). 빌드 기준인 Ubuntu 24.04의 `libwebkit2gtk-4.1`은 이를 만족한다(2026-09-29 Ubuntu 24.04 arm64에서 2.50.4 확인).

## macOS

`.dmg` 더블클릭 → `Tasty.app` 을 `Applications` 로 드래그. 첫 실행 시 Gatekeeper 경고가 나오면 시스템 설정 > 개인정보 보호 및 보안에서 허용(코드 사인 미인증 빌드 한정).

터미널에서 바로 우회하려면:

```bash
xattr -dr com.apple.quarantine /Applications/Tasty.app
```

Tasty는 이 속성을 스스로 지우지 않는다. 기본 제공 플러그인은 부팅 때 앱 번들에서 `~/.tasty/plugins`로 복사되며 복사본이 번들 파일의 속성을 그대로 가질 수 있다. 기본 제공 플러그인이 실행되지 않으면 플러그인 로그(`tasty plugin logs <id>`)를 확인하고, 필요하면 사용자가 직접 앱과 복사본의 속성을 지운다(`xattr -dr com.apple.quarantine ~/.tasty/plugins`).

## Windows

```powershell
# .msi (권장) — 더블클릭 → 설치 마법사. 시작 메뉴 바로가기 + 프로그램 추가/제거 등록
# .zip (수동)
Expand-Archive tasty-{ver}-windows-x64.zip -DestinationPath tasty; .\tasty\tasty.exe
```

### SmartScreen 경고 우회

Authenticode 서명이 없어 첫 실행 시 "Windows의 PC 보호" 경고가 뜬다. **추가 정보 → 그래도 실행**을 선택하면 실행된다.

### 제거 (.msi)

"설정 > 앱 > 설치된 앱" 또는 제어판 > 프로그램 제거에서 tasty 를 제거한다. 설정 앱 목록의 **제거** 는 Windows 기본 동작대로 체크박스 화면 없이 진행 표시만 띄우고(기본 UI 제거, `msiexec /qb /x`) 사용자 데이터(`~/.tasty/`)를 남긴다. 제어판 **제거** 버튼이 어느 화면을 여는지는 확인하지 않았다. 어느 경로든 체크박스를 켜지 않으면 데이터는 남는다.

설정과 기록까지 지우려면 설정 앱 목록의 **수정** 을 누르거나 `.msi` 파일을 다시 실행한 뒤 **Remove** 를 고른다. 이 제거 화면에 설정과 기록을 함께 지우는 체크박스가 있고 기본은 꺼짐이다. 체크박스 아래 설명 문단이 지우는 범위와 시점, 남는 것을 영어로 알려 준다.

- `Program Files\tasty\` 의 바이너리·플러그인 일체, 시작 메뉴 바로가기, PATH 항목은 항상 정리된다.
- **체크박스를 끈 채 제거하면 사용자 데이터(`~/.tasty/`)가 남는다.** 나중에 다시 설치하면 설정과 기록을 그대로 쓴다.
- 데이터를 남기면 `~/.tasty/plugins/` 의 번들 플러그인 사본과 `plugins.toml` 의 `[removed_builtins]` 기록도 남는다. 다시 설치해 실행하면 번들 버전이 사본과 같거나 높은 플러그인은 번들 내용으로 다시 맞추고, 사본 버전이 더 높으면 그대로 둔다(`tasty plugin upgrade-builtins --force` 로 번들로 되돌린다). 사용자가 제거한 번들 플러그인은 다시 설치하지 않는다. 더 낮은 버전이나 다른 서명 키로 빌드한 패키지를 다시 설치했을 때 남은 사본이 정상으로 로드되는지는 확인하지 않았다.
- **체크박스를 켜고 제거할 때만 `~/.tasty/` 를 지운다.** 지우는 시점은 제거가 성공한 뒤다. 제거가 실패하거나 취소되면 데이터는 남는다.
- 파일이 사용 중이라 일부를 지우지 못해도 제거는 성공하고, 폴더가 남았다는 메시지 상자가 뜬다. 실행 중인 Tasty 가 있으면 데이터베이스와 플러그인 사본 등이 남을 수 있으니 Tasty 를 닫고 다시 지운다.
- **체크박스 화면이 없는 제거는 데이터를 지우지 않는다.** 설정 앱 목록의 **제거** 는 진행 표시만 띄우는 기본 UI 제거(`msiexec /qb /x {제품 코드}`)이고, `msiexec /x {제품 코드} /qn` 과 `winget uninstall --silent` 는 화면 없는 제거다. 어느 쪽도 체크박스를 거치지 않으며 프로퍼티로도 삭제를 켤 수 없다.
- **업그레이드와 복구는 `~/.tasty/` 를 건드리지 않는다.**

삭제 범위는 제거를 실행한 Windows 사용자의 `%USERPROFILE%\.tasty` 하나다. `TASTY_HOME` 으로 옮긴 데이터, 다른 Windows 사용자의 데이터, `~/.claude/settings.json` 과 `~/.codex/config.toml` 의 Tasty 훅은 지우지 않는다.

이 동작은 이 체크박스가 들어간 버전을 설치한 뒤부터 적용된다. 제거는 설치된 제품의 MSI 가 수행하므로, 그 전 버전을 업그레이드 없이 바로 제거하면 예전처럼 `~/.tasty/` 가 삭제된다. 먼저 새 버전으로 업그레이드하면 데이터가 보존된다. 그 뒤에도 체크박스는 설정 앱의 **수정** → **Remove**, `.msi` 재실행의 **Remove**, 전체 UI 제거(`msiexec /x {제품 코드}`)에서만 나온다. 설정 앱의 **제거**(`msiexec /qb /x`)와 `/qn` 에는 체크박스가 없고 데이터를 남긴다.

복구와 변경도 앱 목록의 **수정** 이나 `.msi` 재실행으로 한다. 앱 목록에는 Windows Installer 가 등록한 기본 항목이 그대로 보인다.

> `.zip` 은 설치 개념이 없으므로 압축 해제한 폴더를 지우면 끝. 단 `~/.tasty/` 사용자 데이터는 수동 삭제해야 한다.

빌드 측면은 [dev-guide/build](dev-guide/build.md) "Windows MSI".

## GPU 요구사항

Tasty는 GPU 가속 렌더링(wgpu, Vulkan/DX12/Metal)을 쓴다. 하드웨어 GPU 어댑터가 없으면(GPU 미탑재 서버·VM·컨테이너 등) 소프트웨어 렌더러로 한 번 더 시도하고, 그마저 없으면 안내 메시지를 낸 뒤 종료한다. 이름에 `headless` 가 없는 산출물은 모두 GUI 빌드다 — `--headless` 플래그는 받지만 GUI 빌드에서는 효과가 없어 경고 한 줄을 남기고 평소처럼 GUI 로 실행된다. 헤드리스 여부는 플래그가 아니라 빌드가 정하므로, GPU 없이 IPC/CLI 만 쓰려면 `tasty-headless-*` 압축 파일을 받는다. 압축 파일이 없는 환경(예: Intel Mac)은 소스에서 `cargo build --no-default-features` 로 빌드한다([dev-guide/build](dev-guide/build.md), 산출물 상세는 [release](dev-guide/release.md#헤드리스-산출물)).

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
