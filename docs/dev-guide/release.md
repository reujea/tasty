# 릴리스 절차

릴리스는 **Git 태그 push** 로 트리거된다(`.github/workflows/release.yml`). 버전 형식은 `MAJOR.MINOR.PATCH`. 버전 정책의 권위는 [`../../CLAUDE.md`](../../CLAUDE.md) "버전 정책" — 이 문서는 절차다.

## 버전 자동 bump 규칙 (요약)

CLAUDE.md 정책의 운영 형태:

- **본체** (`Cargo.toml` 루트): 사용자가 *빌드를 요청* 했고, 마지막 빌드 이후 새 커밋이 있으며, 사용자가 막지 않았으면 **patch +1**. AI 자체 검증 빌드(`cargo build`/`test`)는 올리지 않는다.
- **Plugin** (`crates/tasty-plugin-*/Cargo.toml`): 그 plugin 의 빌드 산출물이 달라지면(판정 기준은 staged 파일이 아니라 의존 폐포의 내용 — 루트 CLAUDE.md 의 plugin 버전 정책) **patch +1 을 같은 커밋에** 포함. 여러 plugin 변경 시 각각 독립 적용. 본체 규칙과 독립.
- **Plugin 매니페스트 lockstep** (`crates/tasty-plugin-*/tasty-plugin.toml`): 위 patch +1 과 함께 매니페스트 `version` 을 **동일 값**으로 맞춰 같은 커밋에 포함(`.sig` 는 `.gitignore` 된 빌드 산출물이라 커밋하지 않는다). Cargo.toml 만 올리면 `plugin.list`·업그레이드 판정이 노출·비교하는 매니페스트 version 이 어긋난다(version drift). `crates/tasty-doc-guards/tests/plugin_manifest_version_parity.rs` 가 정합을 강제한다(채널은 [ci-gates](ci-gates.md)). 자동 잡은 push 된 커밋만 보므로 커밋 전에는 직접 돌린다.
- **minor / major**: 사용자가 직접 지정. AI 가 임의로 올리지 않는다.

본체 patch bump 절차: `Cargo.toml` patch +1 → `cargo build`(Cargo.lock 갱신) → `README.md`·`README.ko.md` 의 Version 배지(`badge/version-X.Y.Z-blue`)를 같은 값으로 갱신 → `Cargo.toml` + `Cargo.lock` + 두 README 를 **함께** 커밋(`chore: bump version to X.Y.Z`) → 아래 릴리스 절차로 이어감. 배지를 빠뜨리면 `crates/tasty-doc-guards/tests/readme_badge_parity.rs` 가 실패시킨다 — `doc-guards.yml` 이 main push · PR 마다 자동으로 돌리므로([ci-gates](ci-gates.md)) 빠뜨린 bump 는 CI 에서 잡힌다. 다만 자동 잡은 push 된 커밋만 보니 **커밋 전에 직접 돌리면 그 자리에서 잡힌다** — 릴리스뿐 아니라 이 자동 patch +1 커밋에도 적용된다.

## 릴리스 단계

### 1. 버전 + CHANGELOG

1. `Cargo.toml`(루트) `version` 갱신, `cargo build` 로 `Cargo.lock` 갱신.
2. `CHANGELOG.md` 의 `## [Unreleased]` 를 새 버전 헤더로 promote 하고, 빈 `[Unreleased]` 를 다시 추가한다 (`tests/changelog_unreleased.rs` 가 빈 절 존재를 강제).
   ```markdown
   ## [Unreleased]

   ## [X.Y.Z] - YYYY-MM-DD
   ### Fixed
   - `fix(...)`: ...
   ```
3. `README.md`·`README.ko.md` 의 Version 배지(`img.shields.io/badge/version-X.Y.Z-blue`, `CHANGELOG.md` 로 링크)를 `Cargo.toml` 과 같은 값으로 갱신한다. shields.io static badge 라 URL 에 값이 박혀 있어 어디서도 파생되지 않는다 — 이 단계가 빠지면 배지가 `Cargo.toml` 과 다른 버전을 가리킨 채 남는다. 배지 변경은 §3 의 bump 커밋에 함께 넣는다. **`crates/tasty-doc-guards/tests/readme_badge_parity.rs` 가 이 정합을 강제하고, `doc-guards.yml` 이 main push · PR 마다 그것을 돌린다**([ci-gates](ci-gates.md)). **다만 자동 잡은 push 된 커밋만 본다 — 배지를 빠뜨린 bump 커밋은 push 전까지 통과한 것처럼 보이므로 아래 로컬 확인을 거르지 마라.** 로컬 확인:
   ```bash
   cargo test -p tasty-doc-guards --test readme_badge_parity
   ```
4. schema 변경이 있었다면 `crates/tasty-plugin-protocol/CHANGELOG.md` 도 동일 처리. break/deprecation 분류는 [api-conventions](api-conventions.md) "안정성 정책".

### 2. Plugin 매니페스트 서명

`crates/tasty-plugin-*/tasty-plugin.toml` 가 이번 사이클에 하나라도 변경됐으면(또는 첫 release 빌드면) 서명을 갱신한다.

```bash
./scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --all-builtins
```

생성/갱신되는 `*.toml.sig`(번들 plugin 매니페스트 전부 — `--all-builtins` 가 `crates/tasty-plugin-*/tasty-plugin.toml` 을 자동 검색한다)는 **`.gitignore` 로 제외된 빌드 산출물**이라 커밋되지 않는다 — 로컬 release 빌드·dev 검증용이며, CI 정식 release 는 각 self-hosted 러너가 그 자리에서 로컬 자동생성한 키로 재서명한다(영구 보관 안 함, dev/debug 빌드는 서명을 검증하지 않음). 따라서 매니페스트 version bump 시 커밋되는 건 `tasty-plugin.toml` 자체뿐이고 `.sig` 재생성은 커밋 절차 밖이다. 알고리즘·키 보관은 [plugin-packaging](plugin-packaging.md).

### 3. 커밋 — body 가 곧 릴리스 노트

`Cargo.toml` + `Cargo.lock` 을 함께 커밋한다. **커밋 body 에 changelog 를 적는다** — 워크플로가 `git log -1 --format=%b` 로 추출해 GitHub Release 노트로 쓴다.

```
chore: bump version to X.Y.Z

## What's Changed
### Features
- feat(...): ...
### Bug Fixes
- fix(...): ...
```

이전 태그 이후 커밋: `git log v<이전>..HEAD --oneline`.

### 4. 태그 + push

```bash
git tag vX.Y.Z          # 'v' 접두사 + Cargo.toml 버전과 정확히 일치 (불일치 시 워크플로 fail)
git push origin main --tags
```

### 5. 워크플로 (release.yml)

1. **create-release** — 버전 검증 → draft release 생성(body = 릴리스 노트).
2. **build-macos / build-windows / build-linux-x64 / build-linux-arm64** — 각 빌드 스크립트가 로컬 키를 자동생성해(Linux·macOS 는 `ensure-sign-key.sh`, Windows 는 `build-windows.ps1` 이 직접) plugin 재서명 → 빌드(`--profile dist`) → 아티팩트 업로드. GitHub Secret 관여 없음(배경은 [plugin-packaging](plugin-packaging.md) "영구 release 키를 두지 않는 이유").
3. **publish-release** — draft 해제(공개).

### 6. 검증

GitHub Releases 에서 노트 + 플랫폼별 아티팩트 확인:

- macOS `*.dmg` / Windows `*.zip`·`*.msi` / Linux x64·arm64 각 `.tar.gz`·`.deb`·`.rpm`·`.AppImage`
- `SHA256SUMS-{macos,windows,linux-x64,linux-arm64}.txt` 4종 (다운로드 무결성 수동 검증용)

> **사용자 업그레이드**: publish 되면 사용자는 GitHub Releases 에서 새 아티팩트를 직접 내려받아 SHA256SUMS 로 검증한 뒤 수동 설치한다.

## 러너

릴리스 빌드는 GitHub Actions **self-hosted runner** 에서 동작한다. 워크플로는 `.github/workflows/release.yml`, 빌드 스크립트는 `scripts/build-{linux,macos-dmg,windows}.{sh,ps1}`.

### 러너 인벤토리

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

### 1회 도구 설치 (러너 추가 / 새 도구 의존성 시)

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

### CI PATH 매핑

`release.yml` Linux 잡:

```yaml
- name: Setup PATH
  run: |
    echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"   # cargo, cargo-deb, cargo-generate-rpm
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"   # linuxdeploy
```

cargo install 도구는 PATH 변경 불필요. 별도 다운로드 도구는 `~/.local/bin` + 동일 설정.

### 운영 명령

```bash
ssh server 'systemctl status actions.runner.zilhak-tasty.tasty-server-x64.service'
sudo systemctl {restart|stop|start} <service-name>
sudo journalctl -u <service-name> -f
gh api repos/zilhak/tasty/actions/runners        # GitHub 측 등록 상태

# 등록 해제 (머신 정리): stop+disable 후
cd ~/actions-runner && ./config.sh remove --token <REMOVAL_TOKEN>   # token: Settings→Runners→Remove
```

### 트러블슈팅

| 증상 | 해결 |
|------|------|
| `cargo-deb`/`cargo-generate-rpm`/`linuxdeploy` not found | 1회 설치 누락 + `release.yml` Setup PATH 점검 |
| `cmake`/`freetype`/`fontconfig` 빠짐 | `build-linux.sh` 안내대로 `apt install` |
| 러너 offline | `systemctl status` → `restart` |
| release upload 실패 | `GH_TOKEN` 권한/tag 이름 확인 (`--clobber` 재업로드) |
| Windows `candle could not be found` | WiX 3.x 미설치/`WIX` env 누락 → `winget install` 후 서비스 재시작 |
| AppImage `fuse: failed to exec fusermount` | 타깃 환경 FUSE 미지원 → `--appimage-extract-and-run` |

## API 안정성 가드

0.x 라인은 *추가만 가능, 제거 금지* 원칙으로 외부 표면 회귀를 막는다 (메서드 baseline 등 `cargo test --workspace` 강제). 분류는 [api-conventions](api-conventions.md).

## 관련

- [commit-convention.md](commit-convention.md) — bump 커밋 형식
- [build.md](build.md) — `--profile dist` 배포 빌드·패키징 스크립트
