# 릴리스 러너 설정

릴리스 빌드는 GitHub Actions **self-hosted runner** 에서 동작한다. 워크플로는 `.github/workflows/release.yml`, 빌드 스크립트는 `scripts/build-{linux,macos-dmg,windows}.{sh,ps1}`.

## 러너 인벤토리

| 항목 | x86_64 | aarch64 |
|------|--------|---------|
| 호스트 | `server` (192.168.0.16) | `gx10` (192.168.0.13) |
| Runner 이름 | `tasty-server-x64` | `tasty-gx10-arm64` |
| 라벨 | `self-hosted, Linux, X64` | `self-hosted, Linux, ARM64` |
| 설치 경로 | `/home/zilhak/actions-runner/` | 〃 |
| systemd 서비스 | `actions.runner.zilhak-tasty.tasty-server-x64.service` | `…tasty-gx10-arm64.service` |
| 자동 시작 | enabled | enabled |

macOS/Windows 러너도 동일 패턴(라벨만 `[self-hosted, macOS]` / `[self-hosted, Windows]`).

같은 mac/win 러너를 `.github/workflows/crossplatform-check.yml` 이 재사용한다 — dist 빌드 없이 컴파일 정합성만 확인하는 가벼운 가드다. **언제 도는가**: `main` 에 push 될 때(문서·사이트·마크다운만 바뀐 push 는 제외) · `main` 대상 PR · 수동 dispatch. 이 저장소는 PR 없이 main 에 직접 push 하는 흐름이라 push 가 실효 트리거이고, PR 트리거는 PR 흐름을 쓰게 될 때를 위해 남아 있다(선택 근거는 워크플로 파일 상단 주석). **무엇을 도는가**: 잡은 넷(`check-macos` · `check-windows` · `check-headless` · `check-release`)이고 잡별 명령은 [ci-gates](ci-gates.md) 의 표가 정본이다. 네이티브 host 타깃이 곧 `x86_64-pc-windows-msvc` / `aarch64-apple-darwin` 이라 `--target` 지정은 불필요. 잡들이 병렬로 돌고 같은 ref 의 앞선 실행은 취소되므로 러너 점유는 하루 수 분 규모다. (무거운 dist 빌드 검증은 여전히 수동 `build-check.yml`.)

## 1회 도구 설치 (러너 추가 / 새 도구 의존성 시)

각 OS 빌드 스크립트가 시작 시 도구를 검사하고 미설치면 명시적 에러로 안내한다.

```bash
# Linux (x64/arm64 공통)
sudo apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev
cargo install cargo-deb cargo-generate-rpm
mkdir -p ~/.local/bin
curl -fsSL -o ~/.local/bin/linuxdeploy \
  https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$(uname -m).AppImage
chmod +x ~/.local/bin/linuxdeploy
which cargo-deb cargo-generate-rpm linuxdeploy   # 검증
```

```powershell
# Windows
cargo install cargo-wix; winget install WiXToolset.WiXToolset
```

```bash
# macOS
xcode-select --install; brew install create-dmg
```

WiX winget 은 `WIX` env 만 등록(`%WIX%\bin` PATH 추가 안 함) — `build-windows.ps1` 이 자동 prepend. 단 러너 서비스가 새 env 를 못 보면 1회 재시작.

## CI PATH 매핑

`release.yml` Linux 잡:

```yaml
- name: Setup PATH
  run: |
    echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"   # cargo, cargo-deb, cargo-generate-rpm
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"   # linuxdeploy
```

cargo install 도구는 PATH 변경 불필요. 별도 다운로드 도구는 `~/.local/bin` + 동일 설정.

## 운영 명령

```bash
ssh server 'systemctl status actions.runner.zilhak-tasty.tasty-server-x64.service'
sudo systemctl {restart|stop|start} <service-name>
sudo journalctl -u <service-name> -f
gh api repos/zilhak/tasty/actions/runners        # GitHub 측 등록 상태

# 등록 해제 (머신 정리): stop+disable 후
cd ~/actions-runner && ./config.sh remove --token <REMOVAL_TOKEN>   # token: Settings→Runners→Remove
```

## 트러블슈팅

| 증상 | 해결 |
|------|------|
| `cargo-deb`/`cargo-generate-rpm`/`linuxdeploy` not found | 1회 설치 누락 + `release.yml` Setup PATH 점검 |
| `cmake`/`freetype`/`fontconfig` 빠짐 | `build-linux.sh` 안내대로 `apt install` |
| 러너 offline | `systemctl status` → `restart` |
| release upload 실패 | `GH_TOKEN` 권한/tag 이름 확인 (`--clobber` 재업로드) |
| Windows `candle could not be found` | WiX 3.x 미설치/`WIX` env 누락 → `winget install` 후 서비스 재시작 |
| AppImage `fuse: failed to exec fusermount` | 타깃 환경 FUSE 미지원 → `--appimage-extract-and-run` |
