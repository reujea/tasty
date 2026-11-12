# 릴리스 절차

## 원칙과 현재 동작

[플랫폼 원칙](../principles/platform-and-compatibility.md)은 Windows ARM을 1급 지원·릴리스 대상으로 포함한다. 릴리스 워크플로는 Windows x64와 ARM64 산출물을 함께 만들어 올리도록 구성돼 있으며, 빌드 방법과 확인 범위는 아래 [Windows ARM64](#windows-arm64)에 있다. Windows x64 개발 환경과 태그 없는 CI에서 두 아키텍처의 배포물 생성·검사를 확인했다. Windows ARM 기기의 설치·실행은 미측정이며, 태그 없는 빌드 검증은 실제 Release 게시와 구분한다. 0.12.2 이하 릴리스에는 Windows ARM64 자산이 없다.

릴리스는 `main`에 붙인 **Git 태그 push** 로 트리거된다(`.github/workflows/release.yml`). `develop`·`release`·`main`의 뜻과 승격·hotfix 순서는 [브랜치 전략](branching.md)이 정본이다. 버전 형식은 `MAJOR.MINOR.PATCH`. 버전 정책은 [`../../CLAUDE.md`](../../CLAUDE.md) "버전 정책" — 이 문서는 절차다.

## 버전 자동 bump 규칙 (요약)

CLAUDE.md 정책의 운영 형태:

- **본체** (`Cargo.toml` 루트): `release` 브랜치에서 올린다. `develop`의 버전은 역병합으로 따라간다. 예외로 hotfix의 patch는 `main`에서 올리고, `release`와 `develop`은 역병합으로 따라간다.([브랜치 전략](branching.md#버전)) 사용자가 *빌드를 요청* 했고, 마지막 빌드 이후 새 커밋이 있으며, 사용자가 막지 않았으면 **patch +1**. AI 자체 검증 빌드(`cargo build`/`test`)는 올리지 않는다.
- **Plugin** (`crates/tasty-plugin-*/Cargo.toml`): 그 plugin 의 빌드 산출물이 달라지면(판정 기준은 staged 파일이 아니라 의존 폐포의 내용 — 루트 CLAUDE.md 의 plugin 버전 정책) **patch +1 을 같은 커밋에** 포함. 여러 plugin 변경 시 각각 독립 적용. 본체 규칙과 독립. `tasty.pluginBump=deferred`인 lane 작업 트리에서는 lane이 버전 줄을 건드리지 않고, 병합 단계가 `scripts/plugin-bump-fixup.sh`로 커밋마다 붙인다. `develop`에 반영된 커밋이 증가를 함께 담는다는 점은 같다([git-hooks](git-hooks.md#lane-작업-트리의-p1-보류)).
- **Plugin 매니페스트 lockstep** (`crates/tasty-plugin-*/tasty-plugin.toml`): 위 patch +1 과 함께 매니페스트 `version` 을 **동일 값**으로 맞춰 같은 커밋에 포함(`.sig` 는 `.gitignore` 된 빌드 산출물이라 커밋하지 않는다). Cargo.toml 만 올리면 `plugin.list`·업그레이드 판정이 노출·비교하는 매니페스트 version 이 어긋난다(version drift). `crates/tasty-doc-guards/tests/plugin_manifest_version_parity.rs` 가 정합을 강제한다(채널은 [ci-gates](ci-gates.md)). 자동 잡은 push 된 커밋만 보므로 커밋 전에는 직접 돌린다.
- **minor / major**: 사용자가 직접 지정. AI 가 임의로 올리지 않는다.

본체 patch bump 절차: `Cargo.toml` patch +1 → `cargo build`(Cargo.lock 갱신) → `README.md`·`README.ko.md` 의 Version 배지(`badge/version-X.Y.Z-blue`)를 같은 값으로 갱신 → `Cargo.toml` + `Cargo.lock` + 두 README 를 **함께** 커밋(`chore: bump version to X.Y.Z`) → 아래 릴리스 절차로 이어감. 배지를 빠뜨리면 `crates/tasty-doc-guards/tests/readme_badge_parity.rs` 가 실패시킨다 — `doc-guards.yml` 이 develop·release·main push · PR 마다 자동으로 돌리므로([ci-gates](ci-gates.md)) 빠뜨린 bump 는 CI 에서 잡힌다. 다만 자동 잡은 push 된 커밋만 보니 **커밋 전에 직접 돌리면 그 자리에서 잡힌다** — 릴리스뿐 아니라 이 자동 patch +1 커밋에도 적용된다.

버전만 올리는 요청에서도 위 네 파일의 정합을 확인하고 함께 커밋한다. 버전 변경 자체는 태그 생성이나 릴리스 배포를 뜻하지 않는다. 태그·push는 배포 요청 범위에서 진행한다.

## 릴리스 단계

§1~§4는 `release` 브랜치에서 한다. 시작 전에 `release`를 `develop`에서 고른 커밋으로 올리고 push해 릴리스 후보의 CI를 돌린다. `release`가 그 커밋의 조상이면 fast-forward로, 아니면 merge 커밋으로 합친다([release 승격](branching.md#release-승격)).

```bash
git fetch origin
git switch release
git merge --ff-only origin/release
git merge <develop에서 고른 커밋>
git push origin release
```

릴리스 후보의 실제 실행 검증도 이 `release`에서 한다. 검증 중 고친 버그는 `release`에 커밋한다.

### 1. 버전 + CHANGELOG

1. `Cargo.toml`(루트) `version` 갱신, `cargo build` 로 `Cargo.lock` 갱신.
2. `CHANGELOG.md` 의 `## [Unreleased]` 를 새 버전 헤더로 promote 하고, 빈 `[Unreleased]` 를 다시 추가한다 (`tests/changelog_unreleased.rs` 가 빈 절 존재를 강제).
   ```markdown
   ## [Unreleased]

   ## [X.Y.Z] - YYYY-MM-DD
   ### Fixed
   - `fix(...)`: ...
   ```
3. `README.md`·`README.ko.md` 의 Version 배지(`img.shields.io/badge/version-X.Y.Z-blue`, `CHANGELOG.md` 로 링크)를 `Cargo.toml` 과 같은 값으로 갱신한다. shields.io static badge 라 URL 에 값이 박혀 있어 어디서도 파생되지 않는다 — 이 단계가 빠지면 배지와 `Cargo.toml`의 버전이 달라진다. 배지 변경은 §3 의 bump 커밋에 함께 넣는다. **`crates/tasty-doc-guards/tests/readme_badge_parity.rs` 가 이 정합을 강제하고, `doc-guards.yml` 이 develop·release·main push · PR 마다 그것을 돌린다**([ci-gates](ci-gates.md)). **다만 자동 잡은 push 된 커밋만 본다 — 배지를 빠뜨린 bump 커밋은 push 전까지 통과한 것처럼 보이므로 아래 로컬 확인을 거르지 마라.** 로컬 확인:
   ```bash
   cargo test -p tasty-doc-guards --test readme_badge_parity
   ```
4. schema 변경이 있었다면 `crates/tasty-plugin-protocol/CHANGELOG.md` 도 동일 처리. break/deprecation 분류는 [api-conventions](api-conventions.md) "안정성 정책".

### 2. Plugin 매니페스트 서명

`crates/tasty-plugin-*/tasty-plugin.toml` 가 이번 사이클에 하나라도 변경됐으면(또는 첫 release 빌드면) 서명을 갱신한다.

```bash
./scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --all-builtins
```

생성/갱신되는 `*.toml.sig`(번들 plugin 매니페스트 전부 — `--all-builtins` 가 `crates/tasty-plugin-*/tasty-plugin.toml` 을 자동 검색한다)는 **`.gitignore` 로 제외된 빌드 산출물**이라 커밋되지 않는다 — 로컬 release 빌드·dev 검증용이며, CI 정식 release는 각 러너가 선택한 로컬 키로 재서명한다. 키가 없으면 생성하고 기존 개인키는 재사용한다. dev/debug 빌드는 서명을 검증하지 않는다. 따라서 매니페스트 version bump 시 커밋되는 건 `tasty-plugin.toml` 자체뿐이고 `.sig` 재생성은 커밋 절차 밖이다. 알고리즘·키 보관은 [plugin-packaging](plugin-packaging.md).

### 3. 사이트 갱신

공개 사이트(`https://tasty.zilhak.org/`)와 사용자 가이드는 [tasty-page](https://github.com/zilhak/tasty-page) 레포에 있다.
그 레포의 `develop`은 개발 중 갱신하는 최신 페이지이고 `main`이 사이트 게시본이다. 사이트는 사용자가 받을 수 있는
버전을 설명하므로 기능 설명이나 버전이 바뀌는 변경은 Tasty 릴리스와 함께 게시본으로 승격한다. 공개된 Tasty 버전의
기능 설명이나 버전이 바뀌지 않는 변경(문구, 메타데이터 등)은 릴리스와 무관하게 바로 게시한다([8. 사이트 게시](#8-사이트-게시)).
태그를 만들기 전에 tasty-page `develop`을 준비한다.

1. **기획서 반영:** 로컬 지침이 정한 홈페이지 기획서가 tasty-page `develop`의 랜딩, 디자인 소개, 사용자 가이드에 반영됐는지 확인하고 명령어·설정 키·메뉴 이름을 이번 버전과 대조한다.
2. **검사:** tasty-page `develop`의 최신 커밋에서 그 레포의 CI가 통과했는지 확인한다. 이 검사는 사용자 가이드를 Tasty `develop`과 대조한다. Tasty `release`에만 있는 변경(버전 커밋, 릴리스 버그 수정)과의 대조는 태그 전에 여기서 하지 않는다. 그 대조는 [8. 사이트 게시](#8-사이트-게시)의 1번에서 tasty-page `develop`을 그 레포의 `release`로 승격할 때 그 레포의 `release` 검사가 Tasty `release`와 비교해 잡는다. 사용자 가이드와 본체의 대조는 그 레포의 `checks.yml`이 하며, 워크플로 이름과 브랜치마다 본체를 받는 ref는 [tasty-page 사이트 개발 문서](https://github.com/zilhak/tasty-page/blob/develop/docs/dev/site.md)를 따른다.

기획서를 읽을 수 없거나 tasty-page `develop`의 CI가 실패하면 태그를 만들지 않고 사용자에게 알린다. 태그 뒤 승격에서 tasty-page `release` 검사가 실패하면 게시를 멈추고, 원인이 Tasty `release`의 변경이면 tasty-page를 고친 뒤 다시 승격한다.

### 4. 커밋 — body 가 곧 릴리스 노트

`release` 브랜치에서 `Cargo.toml` + `Cargo.lock` 을 함께 커밋한다. **커밋 body에 변경 내용을 영어 평문으로 적는다** — 워크플로가 `git log -1 --format=%b` 로 추출해 GitHub Release 노트로 쓴다.

```
chore: bump version to X.Y.Z

Add the new feature and describe its user-visible behavior.

Fix the reported issue and describe the corrected behavior.
```

이전 태그 이후 커밋: `git log v<이전>..HEAD --oneline`.

### 5. 태그 + push

`main`을 `release`로 fast-forward하고 버전 커밋에 태그를 붙여 `release`·`main`과 태그를 한 번에 push한다. `--atomic`이라 셋 중 하나라도 거부되면 아무것도 갱신되지 않으므로, 원격에 태그만 있거나 `main`만 앞선 상태가 생기지 않는다.

```bash
git switch main
git merge --ff-only origin/main
git merge --ff-only release
git tag vX.Y.Z          # 'v' 접두사 + Cargo.toml 버전과 정확히 일치 (불일치 시 워크플로 fail)
git push --atomic origin release main vX.Y.Z
```

릴리스 노트는 태그 커밋의 본문이므로 태그는 merge 커밋이 아니라 버전 커밋에 붙인다. `main`에 `release`에 없는 hotfix가 있어 fast-forward가 되지 않으면, 먼저 `main`을 `release`에 역병합하고 그 위에서 §1~§4의 버전 커밋을 다시 만든다. `release.yml`은 태그 커밋이 `origin/main`의 조상이 아니면 실패한다.

이 push 뒤에는 태그로 시작한 release.yml 실행이 끝날 때까지 `develop`·`release`·`main` 어느 브랜치에도 push 하지 않는다. 브랜치 push 의 CI 가 같은 self-hosted 러너(특히 Windows arm64 전용 러너 `tasty-win-arm64`)를 먼저 잡으면 릴리스 빌드가 그만큼 늦게 시작한다. 이 push 에 포함된 `release`·`main` push 도 브랜치 CI 를 시작하므로 태그 실행과 같은 러너를 나눠 쓴다. 두 실행이 겹칠 때의 지연은 미측정이다. 실행이 끝났는지는 `gh run list --workflow=release.yml --limit 1` 의 상태나 `gh run watch <run-id>` 로 확인한다.

실행이 끝나면 `release`의 버전 커밋과 릴리스 버그 수정을 `develop`에 역병합한다. 이때 `release`와 `main`은 같은 커밋이다(§5의 fast-forward). `develop`이 앞서 있으면 merge 커밋이 된다.

```bash
git switch develop
git merge --ff-only origin/develop
git merge release
git push origin develop
```

hotfix는 `main`에서 갈라 `main`에 반영하고, `main`에서 patch를 올린 버전 커밋에 태그를 붙여 `git push --atomic origin main vX.Y.Z`로 push한 뒤 `main`을 `release`와 `develop`에 역병합한다([hotfix](branching.md#hotfix)). 사이트 승격([8. 사이트 게시](#8-사이트-게시))은 `main`을 `release`에 역병합한 뒤에 한다. 그 전에 승격하면 tasty-page `release` 검사가 hotfix가 빠진 Tasty `release`와 대조된다.

### 6. 워크플로 (release.yml)

1. **create-release** — 태그 커밋이 `origin/main`의 조상인지 확인 → 버전 검증 → draft release 생성(body = 릴리스 노트). `main`에 반영하지 않은 커밋의 태그면 여기서 실패한다.
2. **build-macos / build-windows / build-windows-arm64 / build-linux-x64 / build-linux-arm64** — 각 빌드 스크립트가 로컬 키를 준비해(Linux·macOS 는 `ensure-sign-key.sh`, Windows 는 `build-windows.ps1` 이 직접) plugin 재서명 → 빌드(`--profile dist`) → GUI 패키징 → 헤드리스 압축 파일(아래 "헤드리스 산출물") → 아티팩트 업로드. GitHub Secret 관여 없음(배경은 [plugin-packaging](plugin-packaging.md) "영구 release 키를 두지 않는 이유").
3. **publish-release** — draft를 해제해 공개한다. 파일을 올리지 않는다. 사이트 게시는 [8. 사이트 게시](#8-사이트-게시)에서 한다.

### 7. 검증

GitHub Releases 에서 노트를 확인하고, 자산이 아래 [릴리스 자산](#릴리스-자산) 표의 23개와 정확히 같은지 확인한다. 빠진 파일과 표에 없는 파일 모두 결함이다.

> **사용자 업그레이드**: publish 되면 사용자는 GitHub Releases 에서 새 아티팩트를 직접 내려받아 SHA256SUMS 로 검증한 뒤 수동 설치한다.

### 8. 사이트 게시

publish-release가 끝나 GitHub Release가 공개된 뒤 tasty-page의 최신 페이지(`develop`)를 게시본(`main`)으로
승격한다. tasty-page도 `develop`·`release`·`main`을 쓰며 `main` push가 사이트 배포를 시작한다.

1. tasty-page `develop`에서 고른 시점을 그 레포의 `release`에 merge하고 그 레포의 검사를 통과시킨다.
2. `release`를 `main`에 merge 커밋으로 합치고 태그를 붙인다.
3. `git push --atomic origin main <태그>`로 `main`과 태그를 함께 push한다. 이 `main` push가 배포다.

1번의 tasty-page `release` 검사는 Tasty `release`와 대조한다. 그래서 hotfix 릴리스는 Tasty가 `main`을 `release`에 역병합한 뒤에 승격한다([hotfix](branching.md#hotfix)의 3번).

태그 이름, 검사와 배포 워크플로의 이름, 브랜치마다 대조하는 Tasty ref는 [tasty-page 사이트 개발 문서](https://github.com/zilhak/tasty-page/blob/develop/docs/dev/site.md)를 따른다.

공개된 Tasty 버전의 기능 설명이나 버전이 바뀌지 않는 사이트 변경(문구, 메타데이터 등)은 릴리스와 무관하게 바로
게시한다. 이런 게시용 수정은 tasty-page `main`에 커밋해 배포한 뒤 그 레포의 `release`와 `develop`에 역병합한다.
기능 설명이나 버전이 바뀌는 변경은 `develop`에 넣고 릴리스와 함께 승격한다.

## 릴리스 자산

GitHub Release 에 올리는 파일은 아래 표가 전부다. 사용자가 받아서 설치·실행하는 배포물과 그 체크섬만 올린다. `{ver}` 는 `Cargo.toml` 버전이다.

| 자산 | 업로드 잡 | 종류 |
|---|---|---|
| `Tasty-{ver}-macos-arm64.dmg` | `build-macos` | GUI 설치 |
| `tasty-headless-{ver}-macos-arm64.tar.gz` | `build-macos` | 헤드리스 |
| `SHA256SUMS-macos.txt` | `build-macos` | 체크섬 |
| `tasty-{ver}-windows-x64.zip` | `build-windows` | GUI 압축 |
| `tasty-{ver}-windows-x64.msi` | `build-windows` | GUI 설치 |
| `tasty-headless-{ver}-windows-x64.zip` | `build-windows` | 헤드리스 |
| `SHA256SUMS-windows.txt` | `build-windows` | 체크섬 |
| `tasty-{ver}-windows-arm64.zip` | `build-windows-arm64` | GUI 압축 |
| `tasty-{ver}-windows-arm64.msi` | `build-windows-arm64` | GUI 설치 |
| `tasty-headless-{ver}-windows-arm64.zip` | `build-windows-arm64` | 헤드리스 |
| `SHA256SUMS-windows-arm64.txt` | `build-windows-arm64` | 체크섬 |
| `tasty-{ver}-linux-x64.tar.gz` | `build-linux-x64` | GUI 압축 |
| `tasty-headless-{ver}-linux-x64.tar.gz` | `build-linux-x64` | 헤드리스 |
| `tasty_{ver}-1_amd64.deb` | `build-linux-x64` | GUI 설치 |
| `tasty-{ver}-1.x86_64.rpm` | `build-linux-x64` | GUI 설치 |
| `Tasty-{ver}-x86_64.AppImage` | `build-linux-x64` | GUI 실행 |
| `SHA256SUMS-linux-x64.txt` | `build-linux-x64` | 체크섬 |
| `tasty-{ver}-linux-arm64.tar.gz` | `build-linux-arm64` | GUI 압축 |
| `tasty-headless-{ver}-linux-arm64.tar.gz` | `build-linux-arm64` | 헤드리스 |
| `tasty_{ver}-1_arm64.deb` | `build-linux-arm64` | GUI 설치 |
| `tasty-{ver}-1.aarch64.rpm` | `build-linux-arm64` | GUI 설치 |
| `Tasty-{ver}-aarch64.AppImage` | `build-linux-arm64` | GUI 실행 |
| `SHA256SUMS-linux-arm64.txt` | `build-linux-arm64` | 체크섬 |

- 체크섬 파일은 같은 잡이 올리는 배포물의 SHA-256 을 담는다. Windows 는 x64 의 기존 이름 `SHA256SUMS-windows.txt` 를 유지하고 ARM64 는 `SHA256SUMS-windows-arm64.txt` 를 따로 올린다. 두 파일을 합쳐 모든 Windows 배포물을 검증한다.
- 고지 파일(`LICENSE`, `THIRD_PARTY_LICENSES.md`, `LICENSES/`)은 자산으로 따로 올리지 않는다. 라이선스가 요구하는 고지 전달은 모든 배포물 안에 넣는 것으로 충족하고([배포물의 고지 파일](#배포물의-고지-파일)), 저장소에서도 공개돼 있다.
- 문서·설정 예시 같은 부속 파일도 올리지 않는다. 사용자에게 필요한 내용은 사용자 가이드나 배포물 안에 둔다.
- 자산을 추가·삭제하려면 이 표, `.github/workflows/release.yml` 의 업로드 줄, 해당 빌드 스크립트, 사용자 가이드의 설치 파일 표를 같은 커밋에서 고친다.
- `crates/tasty-doc-guards/tests/release_assets_match_the_guide.rs` 가 이 표와 `release.yml` 의 모든 `gh release upload` 대상을 이름과 잡 단위로 양방향 대조한다. glob·셸 변수처럼 이름을 펼칠 수 없는 업로드 대상도 표에 없는 자산으로 실패시킨다. 빌드 스크립트가 실제로 그 이름의 파일을 만드는지는 이 검사가 보지 않는다.

Windows 빌드 스크립트는 태그 없는 `workflow_dispatch`에서도 cleanup 전에 PE·고지·체크섬·산출물 명부를 확인한다. 실제 Release 자산 업로드는 태그를 발행하는 릴리스 단계에서 별도로 확인한다.

## 러너

릴리스 빌드는 GitHub Actions **self-hosted runner** 에서 동작한다. 워크플로는 `.github/workflows/release.yml`, 빌드 스크립트는 `scripts/build-{linux,macos-dmg,windows}.{sh,ps1}`.

### 러너 인벤토리

| 항목 | x86_64 | aarch64 |
|------|--------|---------|
| 호스트 | `8945hs` (192.168.0.5) | `gx10` (192.168.0.4) |
| Runner 이름 | `tasty-server-x64` | `tasty-gx10-arm64` |
| 라벨 | `self-hosted, Linux, X64` | `self-hosted, Linux, ARM64` |
| 설치 경로 | `/home/zilhak/actions-runner/` | 〃 |
| systemd 서비스 | `actions.runner.zilhak-tasty.tasty-server-x64.service` | `…tasty-gx10-arm64.service` |
| 자동 시작 | enabled | enabled |

macOS/Windows 러너도 동일 패턴(라벨만 `[self-hosted, macOS]` / `[self-hosted, Windows]`).

Windows 호스트(x64)에는 러너가 둘 있다.

| 항목 | 일반 Windows 러너 | Windows arm64 전용 러너 |
|------|--------|---------|
| Runner 이름 | `tasty-win-runner` | `tasty-win-arm64` |
| 라벨 | `self-hosted, X64, Windows` | `win-arm64` 하나(기본 라벨 없이 등록) |
| 맡는 잡 | `runs-on: [self-hosted, Windows]` 잡(x64 빌드, `check-windows`) | `runs-on: [win-arm64]` 잡 세 개: release·build-check 의 `build-windows-arm64`, crossplatform-check 의 `check-windows-arm64` |
| 실행 방식 | 서비스 | 서비스가 아니라 로그온 시 실행되는 예약 작업 `tasty-runner-arm64` |

전용 러너는 같은 호스트에서 같은 계정·Rust·MSVC·WiX 환경을 쓰고 ARM64 산출물을 교차 빌드한다([Windows ARM64](#windows-arm64)). 라벨이 `win-arm64` 하나라 일반 `[self-hosted, Windows]` 잡은 이 러너로 오지 않고, arm64 잡도 일반 러너로 가지 않는다. 그래서 두 Windows 릴리스 빌드는 서로 다른 러너에서 병렬로 돌 수 있다. 실제로 두 빌드가 겹쳐 돈 실행과 그때 각 잡의 시간은 미측정이다.

예약 작업은 호스트에 로그온한 동안만 러너를 띄운다. 로그온하지 않았거나 예약 작업이 멈추면 이 러너는 offline 이고, arm64 잡 세 개는 다른 러너로 넘어가지 않고 러너가 돌아올 때까지 Queued 상태로 머문다. `timeout-minutes` 는 이 대기 시간을 세지 않고, 24시간 넘게 러너를 기다린 잡은 GitHub 가 취소한다. release 실행에서 `build-windows-arm64` 가 취소되면 `publish-release` 는 `needs` 에 이 잡을 두고 `if` 에 `always()` 가 없어 건너뛰므로, 릴리스는 일부 자산만 올라간 draft 로 남고 공개되지 않는다. 복구는 [트러블슈팅](#트러블슈팅)에 있다.

같은 mac/win 러너를 `.github/workflows/crossplatform-check.yml` 이 재사용한다 — dist 빌드 없이 컴파일 정합성만 확인하는 가벼운 가드다. **언제 도는가**: `develop`·`release`·`main` 에 push 될 때(문서·사이트·마크다운만 바뀐 push 는 제외) · 세 브랜치 대상 PR · 수동 dispatch. 작업 브랜치는 PR 없이 `develop` 에 직접 반영하는 흐름이라 push 가 실효 트리거이고, PR 트리거는 외부 기여 PR 과 hotfix·릴리스 PR 을 검사한다(선택 근거는 워크플로 파일 상단 주석). **무엇을 도는가**: 잡은 다섯(`check-windows` · `check-windows-arm64` · `check-headless` · `check-release` · `check-android`)이고, macOS 잡 `check-macos` 는 같은 트리거의 `.github/workflows/crossplatform-check-macos.yml` 에 따로 있다. 잡별 명령은 [ci-gates](ci-gates.md) 의 표가 정본이다. `check-windows` 와 `check-macos` 는 러너의 네이티브 host 타깃(`x86_64-pc-windows-msvc` / `aarch64-apple-darwin`)으로 돌아 `--target` 을 지정하지 않는다. `check-windows-arm64` 는 같은 x64 Windows 호스트의 전용 러너 `tasty-win-arm64` 에서 `--target aarch64-pc-windows-msvc` 로 교차 컴파일만 한다([Windows ARM64](#windows-arm64)). `check-android` 는 Linux x64 러너에서 `--target aarch64-linux-android` 로 터미널 코어(`tasty-terminal` · `tasty-terminal-link`)를 교차 check 만 한다. 러너가 다른 잡들은 병렬로 돌 수 있어 `check-windows`(일반 Windows 러너)와 `check-windows-arm64`(전용 러너)도 서로 다른 러너에서 병렬로 돌 수 있다. 실제로 겹친 실행은 미측정이다. 같은 ref 의 앞선 실행을 취소하는 조건은 두 워크플로가 다르다([연속 push와 실행 취소](ci-gates.md#연속-push와-실행-취소)). (무거운 dist 빌드 검증은 여전히 수동 `build-check.yml`.)

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
# Windows ARM64 교차 빌드: Visual Studio 구성 요소 "MSVC ARM64 빌드 도구"
# (Microsoft.VisualStudio.Component.VC.Tools.ARM64)와 "C++ Clang Compiler for Windows"
# (Microsoft.VisualStudio.Component.VC.Llvm.Clang)를 Visual Studio Installer에서 추가한다.
# Rust 타깃 aarch64-pc-windows-msvc는 build-windows.ps1이 없으면 rustup으로 추가한다.
```

```bash
# macOS
xcode-select --install; brew install create-dmg
```

WiX winget 은 `WIX` env 만 등록(`%WIX%\bin` PATH 추가 안 함) — `build-windows.ps1` 이 자동 prepend. 단 러너 서비스가 새 env 를 못 보면 1회 재시작.

### Windows ARM64

Windows ARM64 산출물은 x64 Windows 호스트에 두 번째로 등록한 전용 러너 `tasty-win-arm64`(라벨 `win-arm64`)에서 `build-windows.ps1 -Arch arm64`로 교차 빌드한다. ARM64 Windows 기기를 러너로 두지 않는 이유는 저장소 밖 인프라를 늘리지 않고 같은 MSVC·WiX 환경에서 두 아키텍처를 만들 수 있기 때문이다. 러너를 따로 등록한 이유는 x64 빌드와 동시에 돌리기 위해서다([러너 인벤토리](#러너-인벤토리)). 이 러너도 x64 호스트에 있어 ARM64 실행 파일을 실행할 수 없다. 그래서 아래처럼 실행 검사를 대신한다.

- 두 Windows 잡 모두 `--target`(`x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`)을 명시한다. 러너 호스트 아키텍처와 관계없이 파일 이름의 아키텍처와 실제 바이너리가 일치한다. 산출물은 `target\<triple>\<프로필>\`에 생기고 cargo-wix에도 같은 `--target`을 넘긴다.
- 사전 조건: Rust 타깃(없으면 스크립트가 `rustup target add`로 추가), MSVC ARM64 빌드 도구와 clang-cl(`aws-lc-sys`의 AArch64 어셈블리에 필요하다. 스크립트가 `vswhere`로 둘 다 확인하고 없으면 중단), ARM64 MSI를 만드는 WiX 3.14 이상(스크립트가 `candle.exe` 버전을 확인). `wix/main.wxs`는 ARM64에서 64비트 Program Files와 Windows Installer 5.0(`InstallerVersion` 500)을 쓴다. x64 MSI의 `InstallerVersion` 450과 UpgradeCode는 그대로다.
- 산출물 검사는 빌드 스크립트 안에서 dist를 지우기 전에 실행하고 결과를 잡 로그에 남긴다. 태그 없는 `workflow_dispatch` 실행도 같은 로그로 확인한다.
  - ZIP·헤드리스 ZIP·MSI를 풀어 모든 `.exe`(본체와 번들 plugin)의 PE 헤더 Machine 값이 x64는 0x8664, ARM64는 0xAA64인지 확인한다. 실행 파일 수가 본체와 번들 plugin 수보다 적어도 실패한다.
  - `tasty.exe --version` 실행과 MSI 관리 설치(`msiexec /a`)는 호스트와 같은 아키텍처일 때만 한다. x64 러너의 ARM64 빌드는 실행 검사를 건너뛴다는 줄을 남기고 PE 검사로 아키텍처를 확인한다. MSI 안의 파일은 WiX `dark.exe -x`로 꺼내 `main.wxs`의 File Id·Source 짝으로 원래 경로에 놓은 뒤 고지 세트 대조와 PE 검사를 한다.
  - 체크섬 파일을 다시 읽어 그 아키텍처의 배포물 전부를 담는지와 각 해시가 맞는지 확인하고, 산출물 명부(이름과 크기)를 출력한다. staging 디렉터리는 `dist\tasty-windows-<arch>`처럼 아키텍처별이라 같은 dist에서 두 아키텍처를 만들어도 서로 덮지 않는다.
- CI는 `crossplatform-check.yml`의 `check-windows-arm64`가 develop·release·main push마다 ARM64 타깃 `cargo check`를 실행한다([ci-gates](ci-gates.md)). 테스트 실행과 링크는 포함하지 않는다.
- 확인 범위: Windows x64 개발 환경에서 두 아키텍처의 GUI ZIP·MSI·헤드리스 ZIP 생성, 본체·번들 plugin의 PE, 고지 세트와 체크섬을 확인했다. 같은 dist에 양쪽 배포물과 두 체크섬 명부가 함께 유지되는 것도 확인했다. 두 아키텍처의 패키징·산출물 검사는 CI의 태그 없는 실행에서도 확인했으며, ARM64 교차 컴파일·링크와 cargo-wix의 MSI 생성·`dark.exe` 추출을 포함한다.
- 미측정: Windows ARM 기기에서 설치·실행·제거는 확인하지 않았다. MSI 파일 추출·내용 검사는 실기기의 설치와 custom action 실행 검증을 대신하지 않는다. 제거 체크박스를 켠 삭제는 x64 Windows 11에서 시험 계정으로 확인했고, 제거 절차는 [build](build.md)의 Windows MSI 절에 있다. 실제 Release 자산 업로드도 별도 릴리스 단계의 확인 대상이다. 검증 환경이 없다는 이유로 ARM64 산출물을 제외하지 않는다.
- 실제 릴리스 뒤에는 받은 산출물의 PE 머신 타입을 `dumpbin /headers` 등으로 다시 확인한다. ARM64 Windows 기기를 쓸 수 있으면 설치·실행·제거를 확인하고, 없으면 미측정으로 기록한다.

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
ssh 8945hs 'systemctl status actions.runner.zilhak-tasty.tasty-server-x64.service'
sudo systemctl {restart|stop|start} <service-name>
sudo journalctl -u <service-name> -f
gh api repos/zilhak/tasty/actions/runners        # GitHub 측 등록 상태
gh api repos/zilhak/tasty/actions/runners --jq '.runners[] | select(.name=="tasty-win-arm64") | {status, busy}'   # Windows arm64 전용 러너
```

Windows arm64 전용 러너는 서비스가 아니다. offline 이면 Windows 호스트에 로그온한 세션에서 예약 작업을 실행한다.

```powershell
schtasks /Run /TN tasty-runner-arm64
```

```bash
# 등록 해제 (머신 정리): stop+disable 후
cd ~/actions-runner && ./config.sh remove --token <REMOVAL_TOKEN>   # token: Settings→Runners→Remove
```

### 트러블슈팅

| 증상 | 해결 |
|------|------|
| `cargo-deb`/`cargo-generate-rpm`/`linuxdeploy` not found | 1회 설치 누락 + `release.yml` Setup PATH 점검 |
| `cmake`/`freetype`/`fontconfig` 빠짐 | `build-linux.sh` 안내대로 `apt install` |
| 러너 offline | `systemctl status` → `restart` |
| Windows arm64 잡(release·build-check 의 `build-windows-arm64`, `check-windows-arm64`)이 Queued 에 머묾, 또는 24시간 뒤 취소됨 | 전용 러너 `tasty-win-arm64` 가 offline 이다. `timeout-minutes` 는 러너를 기다린 시간을 세지 않아 러너가 돌아올 때까지 Queued 로 머물고, 24시간이 지나면 GitHub 가 잡을 취소한다. 호스트가 로그온 상태인지 확인하고 예약 작업 `tasty-runner-arm64` 를 실행한다([운영 명령](#운영-명령)). 잡이 이미 취소됐으면 `gh run rerun <run-id> --failed` 로 다시 실행한다. release 실행이면 `publish-release` 도 건너뛴 상태라 함께 확인한다 |
| release upload 실패 | `GH_TOKEN` 권한/tag 이름 확인 (`--clobber` 재업로드) |
| Windows `candle could not be found` | WiX 3.x 미설치/`WIX` env 누락 → `winget install` 후 서비스 재시작 |
| Windows ARM64 `MSVC ARM64 build tools not found` | Visual Studio Installer 에서 `Microsoft.VisualStudio.Component.VC.Tools.ARM64` 추가 |
| Windows ARM64 `clang-cl not found` 또는 `aws-lc-sys` 링크 `LNK1181` | Visual Studio Installer 에서 `Microsoft.VisualStudio.Component.VC.Llvm.Clang` 추가 |
| Windows ARM64 `cannot build ARM64 MSI packages` | WiX 3.14 이상으로 갱신 |
| AppImage `fuse: failed to exec fusermount` | 타깃 환경 FUSE 미지원 → `--appimage-extract-and-run` |

## API 안정성 가드

외부 API의 추가·제거는 명시한 호환성 분류와 `(BREAK)` 표기 규칙을 따른다 (메서드 baseline 등 `cargo test --workspace` 강제). 분류는 [api-conventions](api-conventions.md).

## 관련

- [commit-convention.md](commit-convention.md) — bump 커밋 형식
- [build.md](build.md) — `--profile dist` 배포 빌드·패키징 스크립트

## 헤드리스 산출물

릴리스는 GUI 산출물과 함께 `gui` feature를 끈 헤드리스 빌드를 OS별 압축 파일로 올린다.
선택 근거와 대안은 이 문서에 있다.

| OS | 파일 | 만드는 스크립트 |
|---|---|---|
| Linux x64·arm64 | `tasty-headless-<버전>-linux-{x64,arm64}.tar.gz` | `scripts/build-linux.sh` |
| macOS Apple Silicon | `tasty-headless-<버전>-macos-arm64.tar.gz` | `scripts/build-macos-dmg.sh` (`NO_DMG=1`이면 만들지 않는다) |
| Windows x64·ARM64 | `tasty-headless-<버전>-windows-{x64,arm64}.zip` | `scripts/build-windows.ps1 -Arch {x64,arm64}` |

- 각 스크립트가 GUI 패키징을 마친 뒤 `CARGO_TARGET_DIR=target/headless`로 같은 프로필의 `cargo build --no-default-features`를 실행한다. GUI 패키지가 읽는 `target/<프로필>/tasty`를 덮지 않도록 target 디렉터리를 나눈다.
- 압축 파일 안에는 실행 파일 `tasty`(Windows `tasty.exe`), GUI 산출물과 같은 서명된 번들 plugin(`plugins/`), 고지 파일이 들어간다. Linux는 GUI `.tar.gz`와 같은 launcher 스크립트가 `tasty.bin`을 실행한다. macOS 헤드리스 바이너리는 앱 번들과 같은 identity로 단독 서명하고 공증하지 않는다.
- 검증: 압축 파일에서 `tasty --version` 실행, `plugins/`와 고지 파일 포함, GUI 바이너리와 같은 파일이 아님을 확인한다. macOS는 `codesign --verify`와 arm64 Mach-O도 확인한다. Windows는 PE 머신 타입도 확인하며, 호스트에서 실행할 수 없는 아키텍처는 `--version` 대신 이 검사로 확인한다([Windows ARM64](#windows-arm64)).
- 체크섬은 OS별 `SHA256SUMS-*.txt`에 GUI 산출물과 함께 들어간다.
- 설치 형식(`.deb`·`.rpm`·`.AppImage`·`.dmg`·`.msi`)에는 헤드리스를 넣지 않는다.

## 배포 범위와 번들 서명

Tasty 본체는 새 버전을 확인·다운로드·설치하지 않는다.
배포 패키지와 릴리스 페이지를 통해 갱신한다.

서명 키는 `SIGN_KEY_PATH`, 기존 `~/.tasty-keys/release.pem`, 로컬 dev 키 순서로 선택한다.
dev 키는 없을 때 생성하며 기존 개인키는 재사용한다. 공개키는 그 키에서 다시 도출해
빌드에 포함한다. 따라서 '매 빌드 새 키'나 '유출 영향이 빌드 한 번뿐'이라고 설명하지 않는다.
서명 전에 공개키를 준비하고 그 뒤 앱을 빌드해야 번들과 앱의 신뢰 키가 맞는다.
GitHub Secret을 필수로 요구하지 않으며 `.sig`와 로컬 키 산출물은 커밋하지 않는다.

## 플러그인 버전 비교

`check-plugin-version-bump.sh`는 매니페스트를 가진 번들 플러그인의 실제 내용 변경을 검사한다.
대상은 해당 디렉터리와 normal/build 의존성이다. dev 의존성은 제외한다.
Cargo가 보고하는 저장소 안 path 의존성은 workspace 밖에 있어도 포함한다.
`vendor` 같은 디렉터리 이름을 따로 목록으로 관리하지 않는다.

- 인라인 `cfg(test)`와 테스트 선언으로만 도달하는 파일은 제품 비교에서 제외한다.
- test를 요구하는 `cfg_attr`은 조건부 속성 줄만 제외한다. 항목 본문까지 지우지 않는다.
- `not(test)`나 test 밖에서도 참인 `any(test, ...)`는 테스트 전용으로 보지 않는다.
- Rust 내용은 rustfmt로 정규화하고, 실패하면 원문을 비교한다.
- Cargo.toml과 매니페스트의 비교 대상 버전 선언은 증거에서 제외한다.
- 주석·일부 선언 재배치는 산출물이 같아도 버전 증가를 요구할 수 있다.

보조 도구가 없거나 오래되면 테스트 전용 범위를 제외하지 못해 더 넓게 비교하고 안내를 남긴다.
다시 빌드한 뒤 재검사한다. 신선도는 파일 시각이 아닌 소스 내용 지문으로 확인한다.
보조 도구를 수정했다면 파일 SLOC·예외 총합·플러그인 버전 검사를 소비자로 함께 확인한다.

종료 코드는 위반 1, 판정 불가 2, 그 밖에는 0이다. 0일 때 마지막 줄로 두 경우를 구별한다.
`--violations-out <파일>`을 주면 버전을 올려야 하는 플러그인 디렉터리를 그 파일에 한 줄에 하나씩 쓴다. 병합 단계의 `scripts/plugin-bump-fixup.sh`가 이 목록을 읽는다. 매니페스트 version을 읽지 못한 위반은 목록에 넣지 않는다.
판정 대상이 1건 이상이면 `통과 — 판정 대상 N 건`이고, 버전을 확인할 플러그인 내용 변경이 없으면 `판정 대상 없음 — 판정 대상 0 건`이다.
대상이 없는 커밋은 정상이라 0으로 끝나지만 아무것도 검사하지 않았으므로 통과로 표기하지 않는다. 훅과 CI는 종료 코드만 읽는다.

여러 작업을 나누어 push하면 이전 작업이 이미 발행한 버전보다 새 버전을 사용해야 한다.
pre-commit의 비교 기준과 실제 push 직전 원격 끝점은 다를 수 있다.
pre-push는 Git이 전달한 원격·로컬 SHA를 비교한다. 원격 객체가 없으면 fetch 후 다시 검사한다.
새 ref 생성과 삭제처럼 비교할 이전 버전이 없는 경우는 건너뛴 이유와 개수를 따로 보고한다.

루트 `lang/*.toml`은 비교 대상이 아니다. `tasty-i18n`이 이 파일을 `include_str!`로 품고
번들 플러그인 9개가 모두 `tasty-i18n`에 normal 의존한다. 경로는 모두 SDK를 거치며,
clipboard-viewer·git-viewer·image·markdown은 `tasty-egui-theme` 경로도 있다
(clipboard-viewer·git-viewer·markdown은 `tasty-ui-widgets`를 거치기도 한다).
claude·codex는 `tasty-plugin-agent-common`을 거쳐서도 SDK에 닿는다.
SDK는 `plugin_catalog::load`만, `tasty-egui-theme`는 `font` 모듈만 부른다.
두 경로 모두 내장 번역표(`builtin_toml`)에 닿지 않아 번역표가 링크 단계에서 빠진다.
`lang/en.toml` 한 줄만 바꾼 트리와 원래 트리를 각각
`cargo build --release --locked -p <번들 플러그인 9개>`와 `--profile dist`로 빌드해 sha256을 비교했다.
두 프로필 모두 9개 바이너리가 같았고, 같은 원래 트리를 다른 경로에 두 번 빌드한 잡음 바닥도 9/9 같았다.
바이너리에는 번역 키나 값 문자열이 들어 있지 않다.
번들 플러그인이 링크하는 크레이트(플러그인 자신, SDK, `tasty-egui-theme`, `tasty-ui-widgets`,
`tasty-plugin-agent-common` 등) 중 하나라도 `init`·`available_languages`·`scan_languages`처럼
`builtin_toml`에 닿는 함수를 쓰게 되면 이 전제가 깨진다.
의존 경로는 `cargo tree -e normal,build -i tasty-i18n -p <플러그인>`으로 확인한다.
그때는 다시 측정하고 루트 `lang/`을 검사 대상에 넣는다.

Cargo.toml과 매니페스트 버전은 함께 맞추고 Cargo.lock도 갱신해 같은 커밋에 넣는다.
현재 설치본 반영 방법은 [플러그인 개발](plugin-development.md)에 있다.
같은 버전에서 내용이 달라 재동기화될 수 있다는 사실이 배포 버전 관리 의무를 없애지는 않는다.

남는 제한은 루트의 patch 선언이나 Cargo.lock만으로 바뀐 외부 의존 그래프,
그리고 Windows 셸의 경로 표기다. 현재 검사가 모든 의존 변화와 모든 셸을 보장한다고 쓰지 않는다.
CI 경로 필터도 검사 대상보다 넓어야 하며 assets 아래 Markdown을 일반 문서처럼 제외하지 않는다.

## 배포물의 고지 파일

고지 세트는 `LICENSE`, `THIRD_PARTY_LICENSES.md`, `LICENSES/`의 모든 파일이다.
릴리스 때 생성하지 않고 저장소 파일을 복사한다. Rust 의존 목록 생성기만으로는 별도로
벤더링한 폰트·JS·CSS 자산을 찾을 수 없기 때문이다.
파일의 출처와 산출물별 위치는 [제3자 고지](../../THIRD_PARTY_LICENSES.md)가 관리한다.

macOS는 앱의 Resources에 codesign 전에 넣는다. DMG에서 앱을 설치한 뒤에도 남아야 한다.
ZIP은 실행 파일 옆, MSI는 설치 디렉터리에 두며 LICENSES 하위 경로를 유지한다.
MSI 동의 화면의 RTF는 전체 고지 세트를 대신하지 않는다.
셸과 PowerShell 스테이징은 디렉터리를 읽고, 정적 WiX 목록은 빌드 전에 실제 파일과 비교한다.

확인은 패킹 전 트리와 최종 산출물을 구분한다. DMG는 읽기 전용 마운트,
ZIP은 압축 해제, MSI는 관리 설치 등으로 파일을 꺼내 저장소 사본과 바이트를 비교한다.
스크립트에 확인 코드가 있다는 것과 해당 플랫폼에서 실행해 통과했다는 것은 따로 보고한다.
다른 플랫폼에서도 스테이징은 구현하되 실제로 열어 보지 않았다면 미확인이라고 쓴다.

이 고지 세트가 모든 정적 링크 Rust 의존성의 고지를 완성하는 것은 아니다.
라이선스 허용 정책 검사와 배포물 고지 생성은 서로 다른 일이다.
새 자산에는 고지 파일·인벤토리·필요한 WiX 항목을 함께 갱신한다.
