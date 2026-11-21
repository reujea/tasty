# Plugin 패키징 — 서명 + staging 동기화 + 생태계 정책

번들 plugin 을 release/dist 빌드에 포함시키는 절차: **공식 플러그인 잠금 파일의 산출물 받기** + **Ed25519 매니페스트 서명** + **패키지별 배치와 대조**. 런타임 라이프사이클(자동 install/upgrade)과 작성 형식·신뢰·호환성 정책은 아래 [생태계 정책](#생태계-정책--자동-upgrade--호환성-분류) 절, 제작은 [plugin-development](plugin-development.md).

## 번들 plugin 목록 (SoT)

`crates/tasty-host-plugin/src/builtin.rs::BUILTINS` 가 기준 목록이다. 아래 표는 탐색용이다. 추가·제거할 때는 실제 `BUILTINS` 목록과 패키징 설정을 함께 확인한다:

| crate | plugin ID |
|-------|-----------|
| `tasty-plugin-agent-stream` | `com.tasty.agent-stream` |
| `tasty-plugin-claude` | `com.tasty.claude` |
| `tasty-plugin-clipboard-viewer` | `com.tasty.clipboard-viewer` |
| `tasty-plugin-codex` | `com.tasty.codex` |
| `tasty-plugin-git-viewer` | `com.tasty.git-viewer` |
| `tasty-plugin-html` | `com.tasty.html` |
| `tasty-plugin-image` | `com.tasty.image` |
| `tasty-plugin-markdown` | `com.tasty.markdown` |
| `tasty-plugin-mesh-demo` | `com.tasty.mesh-demo` |

### 공식 id 목록

`crates/tasty-host-plugin/src/official.rs::OFFICIAL_PLUGIN_IDS`가 Tasty가 아는 공식 플러그인 id 목록이다. 번들 여부와 무관하며, 번들에서 빠져 따로 내려받아 설치하는 공식 플러그인도 이 목록으로 판정한다. 이 목록에 없는 `com.tasty.*` id는 설치·탐색·등록에서 거부된다([플러그인 관리](../features/plugin-system/index.md#예약-id-comtasty)).

- `BUILTINS`의 id는 모두 이 목록에 있어야 한다. `official::tests::bundled_ids_of_every_os_are_official`가 `builtin.rs` 원문에서 두 OS 벌(`#[cfg(windows)]`·`#[cfg(not(windows))]`)의 id 18개를 읽어 대조하고, `bundled_ids_of_this_build_are_official`가 이 빌드가 고른 벌을 대조한다.
- 번들 플러그인 매니페스트(레포 `crates/*/tasty-plugin.toml`과 공식 플러그인 픽스처, 같은 id면 픽스처)의 id도 모두 이 목록에 있어야 한다(`repository_plugin_manifests_are_official`, 매니페스트 9개).
- 새 공식 플러그인은 `BUILTINS`와 별도로 이 목록에 한 줄을 넣는다. 위 시험들과 목록 길이 단언(`every_official_id_is_admitted`)을 함께 고친다.
- 번들 설치·갱신은 번들 폴더 매니페스트의 `id`가 폴더 이름(`BuiltinSpec::id`)과 같은지 복사 전에 확인한다. 다르면 복사 없이 실패로 남긴다.

배포 패키지의 plugin 폴더(`plugins/<id>/`)에는 받은 산출물 폴더가 통째로 들어가고, non-debug 패키징이 `tasty-plugin.toml.sig`(서명 sidecar)를 더한다: `<bin>`(Windows `.exe`) · `tasty-plugin.toml`(매니페스트) · `lang/*.toml` · 고지 파일(`LICENSE` 등, 릴리스마다 다를 수 있다) · `tasty-plugin.toml.sig`.

### 배포 제외 플래그 (`bundle = false`)

매니페스트 최상위 `bundle` 키(기본 `true`, 스키마: `crates/tasty-plugin-manifest/src/types.rs`)로 **개별 plugin 을 배포 패키징에서만 제외**할 수 있다. 배포 패키징이 따르는 값은 잠금 파일의 `[[plugin]] bundle`이다. dist 스크립트(`build-macos-dmg.sh`/`build-linux.sh`/`build-windows.ps1`)는 `bundle = true`인 산출물만 받는다. **dev 스테이징**(`just build-plugins`)은 이 플래그와 관계없이 모든 공식 플러그인을 받으므로(`--all`) 로컬 빌드에는 그대로 포함된다 — 데모/PoC plugin 을 개발 중엔 쓰되 배포 패키지에서는 빼는 용도.

런타임 `BUILTINS`(`builtin.rs`)에는 그대로 남겨둔다: `install_builtins_if_needed` 가 번들에 없는 builtin 을 debug 로그만 남기고 건너뛰므로, 개발 빌드는 스테이징된 plugin을 설치하고, 배포 빌드는 번들에 없는 항목을 건너뛴다. 현재 `com.tasty.mesh-demo`와 `com.tasty.agent-stream`이 `bundle = false`다(mesh-demo는 잠금 파일에 없다). deb/rpm assets와 MSI 목록은 받은 폴더에서 나오므로 `bundle` 값을 바꿀 때 패키징 설정에서 따로 뺄 항목은 없다.

## 공식 플러그인 잠금 파일

공식 플러그인(mesh-demo를 뺀 8개)은 [tasty-official-plugins](https://github.com/zilhak/tasty-official-plugins) 레포가 릴리스한다. 루트의 `official-plugins.lock.toml`이 그 릴리스 하나를 고정한다. 배포 패키징 스크립트는 이 파일의 산출물을 받아 넣는다([배포 패키지의 번들 플러그인](#배포-패키지의-번들-플러그인)). 개발 스테이징(`just build-plugins`, 즉 `scripts/stage-dev-plugins.sh`)도 기본으로 이 파일의 산출물을 받는다(`fetch-official-plugins.sh --all`이라 `bundle = false`인 플러그인도 받는다). 공식 플러그인을 빌드하는 것은 `--from-checkout`으로 공식 플러그인 레포 체크아웃을 줄 때뿐이다. mesh-demo는 잠금 파일에 없어 어느 경로에서든 이 workspace에서 빌드한다([build.md](build.md#plugin-빌드--스테이징)).

### 형식

손으로 고치지 않는다. 아래 [갱신](#잠금-파일-갱신) 절차의 `generate`가 만든다.

| 위치 | 키 | 뜻 |
|---|---|---|
| 최상위 | `schema` | 파일 형식 버전. 지금 `1` |
| `[release]` | `repository` · `tag` · `commit` | 플러그인 레포 이름, 릴리스 태그, 그 태그의 커밋 |
| `[release]` | `tasty_rev` | 그 릴리스가 빌드에 쓴 tasty 커밋 |
| `[release]` | `host_api_version` · `protocol_version` · `manifest_version` · `epaint_version` · `epaint_patch_tree` | 그 릴리스가 빌드에 쓴 계약 값. 아래 [계약 호환 가드](#계약-호환-가드)가 현재 tasty와 대조한다 |
| `[[plugin]]` | `id` · `version` · `bundle` | 플러그인. `bundle = false`(agent-stream)는 배포 패키지에 넣지 않고 개발 경로만 받는다 |
| `[[plugin.artifact]]` | `platform` · `file` · `url` · `sha256` | 플랫폼별 산출물. 플랫폼은 `macos-arm64`, `windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64` 다섯이다(`release.yml`의 다섯 빌드 잡) |

산출물은 다섯 플랫폼 모두 `<id>-<version>-<platform>.tar.gz`이고 URL은 `https://github.com/<repository>/releases/download/<tag>/<file>`이다. 아카이브 최상위는 `<id>/` 하나이고 그 아래 실행 파일 `tasty-plugin-<id의 com.tasty. 뒤>`(Windows는 `.exe`), `tasty-plugin.toml`, `lang/`이 있다. 서명(`.sig`)은 없다. 패키징이 받은 뒤 `sign-bundle.sh --dir`로 서명한다. 그 밖의 파일(라이선스·고지)은 릴리스마다 달라질 수 있어 도구가 목록을 고정하지 않는다.

### 읽기 도구

셸·PowerShell이 TOML을 직접 읽지 않도록 `crates/tasty-doc-guards`의 bin `official-plugins`가 잠금 파일을 줄 단위로 바꾼다. 네트워크는 쓰지 않는다. 종료코드는 0 통과, 1 검사 실패, 2 인자·입출력 오류다.

| 하위 명령 | 하는 일 |
|---|---|
| `generate --meta <release-meta.toml> --tsv <plugins-<p>.tsv>... --out <파일>` | 릴리스의 메타데이터와 TSV 5개로 잠금 파일을 만든다. TSV마다 (id, version) 집합이 메타데이터와 같아야 하고 다섯 플랫폼이 한 번씩 있어야 한다 |
| `check [--lock <파일>]` | 형식과 완비(모든 플러그인에 5플랫폼, 64자리 sha256, 계산한 파일 이름·URL)를 확인한다. 받기 스크립트가 경로 조각으로 쓰는 값도 제한한다. `id`는 `com.tasty.` 뒤에 `[a-z0-9-]`만, `tag`는 `[A-Za-z0-9._-]`로만 된 `.`·`..`이 아닌 값, `file`은 `/`·`\`·`..`이 없는 단일 이름이어야 한다 |
| `plan --platform <p> [--all]` | 받을 산출물을 `id`, `version`, `file`, `url`, `sha256` 탭 구분 한 줄씩 출력한다. `--all`이 없으면 `bundle = true`만 |
| `verify --dir <폴더> --platform <p> [--all]` | 받은 파일의 SHA-256을 대조한다. 없는 파일은 `missing: <file>`, 다른 파일은 `sha256 mismatch: <file>`로 이름을 댄다 |
| `unpack --dir <폴더> --out <폴더> --platform <p> [--all]` | `verify` 뒤 시스템 `tar`로 풀어 `<out>/<id>/`에 둔다. 풀기 전에 아카이브 목록(`tar -t`·`tar -tv`)을 읽어 `<id>/` 밖의 이름, 절대 경로, `..` 구성요소, 일반 파일·디렉터리가 아닌 항목을 거부한다. 링크는 자세한 목록 줄의 종류 문자(`l`·`h`)로 거부하고, 종류 문자가 `-`여도 줄이 항목 이름 뒤에 링크 표기(` link to <대상>`, ` -> <대상>`)를 붙이면 거부한다. bsdtar는 mode 필드에 종류 비트가 든 하드 링크 헤더를 `-`로 보이기 때문이다. 목록은 `LC_ALL=C`로 읽으므로 bsdtar 환경(macOS, Windows의 기본 `tar`)에서는 pax 형식 아카이브의 항목 이름이 ASCII여야 한다. 비ASCII 이름이 있으면 bsdtar가 목록 단계에서 실패하고 `unpack`도 실패한다. 공식 플러그인 패키징은 ASCII 이름만 넣는다. 푼 뒤에는 링크를 따라가지 않고 최상위 `<id>/`, 매니페스트, 실행 파일을 확인하며 매니페스트 `id`·`version`이 잠금 파일과 같아야 한다. `<out>`에 이번 계획에 없는 항목이 있으면 그 이름을 대고 실패하며 지우지 않는다 |

doc-guards 크레이트는 의존성을 두지 않으므로 이 도구는 잠금 파일과 메타데이터가 쓰는 TOML 부분집합만 읽고, 모르는 형식은 줄 번호와 함께 거절한다.

### 받기 스크립트

```bash
scripts/fetch-official-plugins.sh --platform <p> --out <폴더> [--all] [--lock <파일>] [--cache <폴더>]
```

`target/official-plugins/<tag>/<platform>/`(`CARGO_TARGET_DIR`가 있으면 그 아래, `--cache`를 주면 그 폴더)에 `curl -fL`로 받는다. `verify`가 이름을 댄 파일만 받으므로 해시가 맞는 파일은 다시 받지 않는다. 그 뒤 `unpack`으로 `<out>/<id>/`에 풀고, 푼 매니페스트를 현재 호스트의 파서·검증기(`Manifest::load`, `tasty-plugin-manifest`의 `validate-plugin-dirs` 예제)로 읽는다. 스테이징 폴더(`--out`)는 매번 비우거나 새로 만든다. 이전에 푼 다른 플러그인이 남아 있으면 `unpack`이 실패한다. bash 3.2(macOS 기본)와 Windows Git Bash에서 실행한다. cargo·curl·tar가 필요하다.

### 잠금 파일 갱신

플러그인 레포가 새 릴리스를 올리거나 아래 계약 호환 가드가 실패하면 갱신한다.

1. 릴리스의 `release-meta.toml`과 `plugins-<platform>.tsv` 5개를 받는다.

   ```bash
   gh release download <tag> -R zilhak/tasty-official-plugins -p 'release-meta.toml' -p 'plugins-*.tsv' -D <임시 폴더>
   ```

2. 잠금 파일을 만든다.

   ```bash
   cargo run -p tasty-doc-guards --bin official-plugins -- generate \
     --meta <임시 폴더>/release-meta.toml \
     --tsv <임시 폴더>/plugins-macos-arm64.tsv --tsv <임시 폴더>/plugins-windows-x64.tsv \
     --tsv <임시 폴더>/plugins-windows-arm64.tsv --tsv <임시 폴더>/plugins-linux-x64.tsv \
     --tsv <임시 폴더>/plugins-linux-arm64.tsv --out official-plugins.lock.toml
   ```

3. `cargo test -p tasty-doc-guards --test official_plugins_lock`로 완비·계약·집합 가드를 확인한다.
4. 한 플랫폼 이상에서 `scripts/fetch-official-plugins.sh --platform <p> --out <임시 폴더> --all`로 실제로 받아 본다.
5. 호스트 시험 픽스처를 새 산출물로 다시 쓰고 확인한다. 잠금 파일과 바뀐 픽스처를 같은 커밋에 넣는다([호스트 시험 픽스처](#호스트-시험-픽스처)).

   ```bash
   bash scripts/sync-official-plugin-fixtures.sh --update
   cargo test -p tasty-doc-guards --test official_plugin_fixtures
   ```

### 계약 호환 가드

본체와 공식 플러그인은 다른 커밋에서 빌드된다. 잠금 파일이 가리키는 플러그인은 그 릴리스의 `tasty_rev`에서 빌드됐으므로, 지금 tasty와 계약이 맞는지 doc-guards `official_plugins_lock`이 확인한다. 다르면 "공식 플러그인 레포의 새 릴리스로 잠금 파일을 갱신해야 한다"로 실패한다.

| 축 | 잠금 파일 | 현재 tasty |
|---|---|---|
| (a) | `host_api_version` | `crates/tasty-plugin-manifest/src/types.rs` `HOST_API_VERSION` |
| (b) | `protocol_version` | `crates/tasty-plugin-protocol/Cargo.toml` version |
| (c) | `manifest_version` | `types.rs` `MANIFEST_VERSION` |
| (d) | `epaint_version` | `Cargo.lock`의 epaint |
| (e) | `epaint_patch_tree` | `git rev-parse HEAD:vendor/epaint` |

같은 시험이 집합 두 가지도 본다. `BUILTINS`(두 cfg 벌)와 `OFFICIAL_PLUGIN_IDS`는 잠금 파일 전체 id에 mesh-demo를 더한 집합이어야 하고, 배포 패키지 스크립트(`scripts/lib/bundled-plugins.sh`, `scripts/build-windows.ps1`)는 `fetch-official-plugins.sh`를 `--all` 없이 불러 잠금 파일의 `bundle = true`만 받고, 개발 번들(`scripts/stage-dev-plugins.sh` 기본 경로)은 `--all`로 전부 받는지 호출 옵션을 본다. 잠금 항목과 매니페스트의 `bundle`이 같은지는 픽스처 가드가 본다([호스트 시험 픽스처](#호스트-시험-픽스처)). 받은 매니페스트는 받기 스크립트가 현재 검증기로 읽는다.

한계: 이 가드는 버전 값만 비교한다. protocol schema를 바꾸고 버전을 그대로 두면 지나간다. protocol 버전과 `CHANGELOG.md`는 tasty 릴리스 때 올리므로([릴리스 절차](release.md#1-버전--changelog)) 릴리스 사이의 develop에서는 버전이 그대로인 채 schema가 바뀌는 것이 정상 흐름이고, 그 변경은 가드가 보지 못한다. 실제로 v0.1.0의 `tasty_rev` 뒤에 편집 결과 회신(`PluginEvent::Hello`의 `capabilities`, `EditOutcome`)이 protocol 0.2.16 그대로 들어왔다. 추가 변형이라 옛 플러그인과 호환되지만 호환되지 않는 변경도 같은 길로 지나갈 수 있다. 그래서 protocol 변경은 [protocol CHANGELOG 정책](../../crates/tasty-plugin-protocol/CHANGELOG.md#정책)대로 `tasty_rev` 시점의 플러그인과 호환되게 만든다.

`tasty_rev` 시점과 지금의 `crates/tasty-plugin-protocol`·`crates/tasty-plugin-manifest` 트리 id를 비교하는 엄격안은 쓰지 않는다. 주석 한 줄에도 실패해서 계약 크레이트를 고칠 때마다 플러그인 레포 릴리스를 강제하고, v0.1.0에서도 바로 실패한다. 버전 축 가드와 CHANGELOG 정책으로 시작하고, 호환되지 않는 변경이 버전 없이 들어온 사례가 생기면 다시 판단한다.

### 호스트 시험 픽스처

호스트가 공식 매니페스트를 읽고 쓰는 방식(파싱·기본값·CLI 선언·번역 키 등)을 보는 시험은 `crates/tasty-test-support/fixtures/official-plugins/<id>/`를 입력으로 쓴다. 폴더마다 잠금 파일이 가리키는 릴리스 산출물의 `tasty-plugin.toml`과 `lang/` 원문이 바이트 그대로 있다. 잠금 파일의 `[[plugin]]` 전부(`bundle = false` 포함)가 대상이다. `tasty_test_support::official_plugin_dir(id)`가 폴더 경로를, `bundled_plugin_manifest_dirs()`가 번들 매니페스트 전체(크레이트 바로 아래 매니페스트와 픽스처, 같은 id면 픽스처)를 주고, test-support에 의존하지 않는 크레이트는 `tasty_doc_guards::OFFICIAL_PLUGIN_FIXTURES_DIR` 또는 `CARGO_MANIFEST_DIR` 기준 상대 경로로 같은 폴더를 읽는다.

`scripts/sync-official-plugin-fixtures.sh`가 픽스처를 갱신하고 대조한다.

```bash
bash scripts/sync-official-plugin-fixtures.sh --update               # 잠금 산출물로 픽스처를 다시 쓴다
bash scripts/sync-official-plugin-fixtures.sh --check [--shared-cache] # 받은 산출물과 바이트로 대조한다
```

- 받기: 잠금 파일을 사본으로 떠서 `fetch-official-plugins.sh --platform <호스트> --lock <사본> --all --out <빈 폴더> --cache <전용 폴더>`로 받는다. 해시 확인은 fetch가 한다. 실행 폴더는 `target/official-plugin-fixtures-sync/` 아래이고, 같은 폴더의 `.lock` 디렉터리로 동시 실행을 막는다. `--shared-cache`는 fetch 기본 캐시(`target/official-plugins/<tag>/<platform>/`)의 파일을 전용 폴더로 복사한 뒤 받으므로, 같은 체크아웃에서 `stage-dev-plugins.sh` 바로 뒤에 돌면 다시 받지 않는다.
- `--update`: 후보를 따로 만들어 CR 바이트가 없는지, 받은 폴더와 바이트가 같은지, 잠금 항목마다 `lang/en.toml`이 있고 매니페스트 최상위의 `id`·`version`·`bundle`이 잠금 항목과 같은지 본다. 매니페스트는 오프라인 가드와 같은 `toml-scan.awk` 판독과 규칙으로 읽는다(읽지 못한 최상위 키가 하나라도 있으면 실패, `bundle` 키가 없으면 true). 잠금 파일의 `version`·`bundle`은 항목마다 있어야 한다. 하나라도 어기면 기존 픽스처를 그대로 두고 실패한다. 순서는 후보를 픽스처 폴더와 같은 부모 아래의 새 점 폴더(`.official-plugins-sync.*`)로 복사·대조 → 잠금 파일이 받기 시작할 때와 같은지 재대조 → 기존 폴더를 그 점 폴더 안으로, 후보를 픽스처 자리로 이름 바꾸기다. 마지막 두 이름 바꾸기는 같은 부모 아래 새 폴더끼리라 복사로 바뀌지 않는다. 재귀 품질 가드는 이 점 폴더도 픽스처로 본다. 잠금 파일에 없는 옛 픽스처는 남지 않는다.
- `--check`: 잠금 id마다 매니페스트와 `lang/` 파일 집합·바이트를 대조하고, 잠금 파일에 없는 픽스처 폴더와 CR 바이트도 실패로 본다. 받는 동안 잠금 파일이 바뀌면 실패한다.
- 대조 두 겹: doc-guards `official_plugin_fixtures`가 네트워크 없이 폴더 집합·`id`·`version`·`bundle`·`lang/en.toml`·CR 없음을 본다. 매니페스트 필드는 `--update`와 같은 `scripts/lib/toml-scan.awk` 판독으로 읽으므로 Unix에서만 돌고, 폴더 집합·`lang/en.toml`·CR 검사는 모든 플랫폼에서 돈다. 같은 version의 내용 변이(잠금 해시만 바뀐 재발행)와 번역 원문 차이는 `crossplatform-check.yml`의 `check official plugin fixtures` 단계(`--check --shared-cache`)가 바이트로 잡는다([CI 가이드](ci-gates.md#번들-플러그인-준비)).
- 줄끝: `.gitattributes`의 LF 정규화를 픽스처에도 그대로 둔다. CRLF 원문이 들어오면 갱신·오프라인 가드·CI 대조가 실패한다.
- 품질 가드: 픽스처는 공식 플러그인 레포의 원문이라 tasty에서 문구를 고칠 수 없다. 레포를 재귀로 도는 품질 가드(이모지·번역 키 사용처·경로 인용·로컬 문서 참조·게이트 수치·CI 주장·용어)는 이 폴더를 뺀다. 그 검사는 공식 플러그인 레포의 몫이다.

## 서명

| 항목 | 값 |
|------|-----|
| 알고리즘 | Ed25519 (`ed25519-dalek`) |
| 서명 대상 | `<plugin-dir>/tasty-plugin.toml` 의 SHA-256 digest |
| 서명 파일 | `tasty-plugin.toml.sig` (raw 64 byte) |
| Trust store | `crates/tasty-host-plugin/keys/`의 `release-pubkey.bin`과 `dev-pubkey.bin` 두 슬롯. 추적하지 않는 로컬 파일이며, 없는 슬롯은 build.rs가 placeholder로 채운다. 실제 서명 키에 대응하는 공개키가 빌드에 포함돼야 한다 |
| 검증 시점 | 설치·확인 필요 탭 등록(`trust_verdict::judge`, debug·release 공통 — 미인증은 사용자가 고를 때만 설치·등록), plugin 로드(`discovery.rs::trust_outcome`)와 `upgrade-builtins`(`builtin.rs::verify_builtin_bundle_trust`) — 뒤의 둘은 release/dist 에서 판정하고 debug 는 건너뛰거나 `debug!` 로그만(`#[cfg(debug_assertions)]`) |
| 공개 키 파일 | 커스텀 plugin 은 `tasty-plugin.toml.pub`(raw 32 byte)를 함께 배포해야 사용자가 그 키를 신뢰할 수 있다. 서명은 공개 키를 드러내지 않는다 |

보호 범위는 **매니페스트 한 파일만** — contributes/kind 가 매니페스트 안이라 변조되면 host가 잘못된 기여 정보를 신뢰할 수 있다. binary 는 OS codesign(macOS notarization/Windows Authenticode)에 위임, lang/ 등 부속은 검증 밖.

### dev key (개발자 1회)

```bash
./scripts/gen-dev-key.sh
# → ~/.tasty-keys/dev.pem  (600, gitignored)
# → crates/tasty-host-plugin/keys/dev-pubkey.bin  (public 32B, 로컬 전용 — 추적 안 함)
```

`dev-pubkey.bin` 은 개발자별 로컬 키라 **추적하지 않는다** (`keys/.gitignore`). 파일이 없어도 build.rs 가 OUT_DIR 슬롯을 all-zero placeholder 로 채워 컴파일은 안 깨지지만, dev key 서명 plugin 을 자동 trust 하려면 `dev-pubkey.bin` 이 서명에 쓰는 `dev.pem` 과 일치해야 한다.

`gen-dev-key.sh` 는 idempotent 하다 — `dev.pem` 이 이미 있으면 private key 는 유지하고 `dev-pubkey.bin` 만 그 키에서 재도출한다. `build-*.sh` / `build-windows.ps1` 도 dev 키 경로에서 cargo build 직전 항상 `gen-dev-key.sh` 를 호출하므로, `dev.pem` 만 있고 `dev-pubkey.bin` 이 없는 상태(새 클론·추적 해제 후)에서도 placeholder 가 아닌 실제 trust 키가 임베드된다.

### 재서명 (로컬)

서명은 번들에 들어가는 매니페스트 사본에 만든다. `tasty-plugin.toml` 을 한 줄이라도 고치면 그 사본의 기존 `.sig` 는 무효가 되므로, 매니페스트를 고쳤으면 그 사본을 만드는 빌드를 다시 실행해 서명을 새로 만든다.

**로컬 release/dist `just` 빌드**(`build-plugins`, 즉 `just build --release`·`just run --release`·`just build-all`)는 받거나 빌드한 공식 플러그인을 `--dir` 로, mesh-demo 는 스테이징한 매니페스트 사본을 `--manifest` 로 서명한다([build.md](build.md#plugin-빌드--스테이징)). dist 스크립트(`build-macos-dmg.sh`/`build-linux.sh`/`build-windows.ps1`)는 받은 폴더를 아래 `--dir`로 서명한다. debug 프로필은 게이트가 꺼져 있어 서명 단계를 건너뛴다.

`--all-builtins` 는 workspace 크레이트 매니페스트(`crates/*/tasty-plugin.toml`) 옆에 `.sig` 를 제자리로 쓰는 수동 도구다. 패키징과 스테이징은 이 서명을 쓰지 않는다. 크레이트 옆 `.sig` 를 읽는 곳은 debug 부팅 동기화(`crates/tasty-host-plugin/src/builtin.rs` 의 `sync_builtin_dev_sig`)뿐이다. 이 동기화는 workspace 원천 플러그인(mesh-demo)만 다루고 서명을 검증하지 않는다.

```bash
./scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --all-builtins
```

받아서 스테이징한 플러그인 폴더는 `--dir`로 서명한다. dist 스크립트는 각자 서명 함수 하나(`sign_bundled_plugins`, Windows `Sign-BundledPlugins`)에서만 이 명령을 부른다. `<dir>/<id>/tasty-plugin.toml`을 모두 서명하고, 그 폴더 아래에 `.sig`가 하나라도 있으면 아무것도 서명하지 않고 실패한다. 공식 플러그인 릴리스 산출물은 서명 없이 오고 tasty 패키징이 서명한다는 계약을 이 실패로 확인한다. dist 스크립트는 서명을 만들지 않는 debug에서도 이 계약을 지키도록 받기 직후 같은 검사를 따로 한다([배포 패키지의 번들 플러그인](#배포-패키지의-번들-플러그인)). 서명할 매니페스트가 없을 때와 키 파일이 없을 때도 실패한다. 계약은 `scripts/tests/sign-bundle-dir.sh`(doc-guards `sign_bundle_dir_contract`가 실행)가 확인한다.

```bash
./scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --dir <스테이징 폴더>
```

키 탐색 규칙(`SIGN_KEY_PATH` env → `release.pem` → `dev.pem`+`gen-dev-key.sh`)은 cargo build **전에** 수행돼야 임베드 `dev-pubkey.bin` 이 서명 키와 일치한다(순서 불변식). 이 규칙은 `scripts/ensure-sign-key.sh` 공용 헬퍼로 추출돼 Justfile·`build-linux.sh`·`build-macos-dmg.sh` 가 공유한다(키 경로를 stdout, 진단은 stderr). `build-windows.ps1` 은 PowerShell-native 로직을 유지한다.

### Release CI 서명

release workflow는 각 빌드 러너에서 서명 스크립트를 실행한다. 키 선택은 다음 순서다.

1. `SIGN_KEY_PATH`가 지정돼 있으면 그 경로를 사용한다.
2. 없으면 기존 `~/.tasty-keys/release.pem`을 사용한다.
3. 둘 다 없으면 `gen-dev-key.sh`로 `dev.pem`을 준비한다. 기존 키는 재사용하고 없을 때만 만든다.

셸 빌드는 `ensure-sign-key.sh`, Windows 빌드는 PowerShell의 대응 로직을 사용한다.
`gen-dev-key.sh`는 기존 개인키를 보존하면서 공개키를 다시 도출하고, 내용이 같으면 공개키 파일을 다시 쓰지 않는다.
매 빌드마다 새 개인키를 만들거나 release.pem을 자동 생성하지 않는다.
특정 러너에 키가 없다고 고정해서 가정하지 말고 빌드 로그에서 선택된 경로와 공개키 준비 단계를 확인한다.
개인키 내용은 로그에 출력하지 않는다.

### 영구 release 키를 두지 않는 이유

현재 번들 플러그인은 앱과 같은 설치 묶음에서 복사된다. 앱과 별도로 원격 업데이트하는 체계는 없다.
따라서 모든 빌드가 하나의 장기 발급자 키를 공유해야 한다는 요구를 두지 않고, 위 우선순위로 준비된 키를 사용한다.
이는 키를 매 빌드 교체한다는 뜻이 아니다. dev.pem도 재사용하므로 유출 영향을 한 빌드로 한정할 수 없다.

매니페스트 서명은 그 파일의 변조를 확인한다. 외부 생태계의 장기 발급자 신원이나 binary·lang 파일 전체를 보증하지 않는다.
앱과 독립적으로 플러그인을 배포하거나 업데이트할 때는 신뢰 루트·키 회전·서명 범위를 함께 다시 정한다.
구체적인 빌드·배포 절차는 [release](release.md), 공식 플러그인 판별은
[플러그인 원칙](../principles/plugins.md#배포)을 따른다.

## 배포 패키지의 번들 플러그인

dist 스크립트는 플러그인을 빌드하지 않는다. 서명 키를 준비한 뒤 본체 빌드보다 먼저 받기를 해서, 받기가 실패하면 본체를 빌드하지 않고 멈춘다. 순서는 아래와 같다. 셸 쪽 공용 함수는 `scripts/lib/bundled-plugins.sh`, Windows는 `build-windows.ps1`의 `Get-BundledPlugins`·`Stage-Plugins`·`Test-BundledPluginTree`다.

1. **받기**: `fetch-official-plugins.sh --platform <p> --out target/bundled-plugins`로 잠금 파일의 `bundle = true` 산출물을 받는다. 플랫폼은 Linux `linux-x64`·`linux-arm64`, macOS `macos-arm64`, Windows `windows-x64`·`windows-arm64`다. Windows는 Git Bash로 이 스크립트를 실행하고 `.tar.gz`도 그 셸에서 찾은 `tar`가 푼다. 산출물이 없거나 SHA-256이 다르면 받기에서 실패하고 패키지를 만들지 않는다. 받은 폴더에 `.sig`가 하나라도 있어도 debug를 포함한 모든 프로필에서 받기 직후 실패한다. 매니페스트 서명은 이 빌드의 키로만 만든다. 스테이징 폴더는 매번 지우고 새로 만든다.
2. **서명**: 본체를 빌드한 뒤 non-debug 프로필이면 `sign-bundle.sh --dir target/bundled-plugins`로 매니페스트를 서명한다. debug는 서명을 만들지 않는다. 키는 본체 빌드 전에 준비한 것을 쓴다([서명](#서명)).
3. **배치**: 받은 `<id>/` 폴더를 통째로 패키지의 plugin 폴더에 복사한다. 고지 파일(`LICENSE`, 릴리스에 따라 `THIRD_PARTY_LICENSES.md`·`LICENSES/`·`NOTICE.md`)도 플러그인과 함께 간다. 루트 고지 세트([배포물의 고지 파일](release.md#배포물의-고지-파일))는 본체 몫이고 플러그인 고지를 대신하지 않는다.
4. **대조**: 패키지를 만든 뒤 그 안의 plugin 폴더를 스테이징 폴더와 비교한다. 다르면 실패한다.

| 패키지 | plugin 폴더 | 목록의 원천 | 대조 |
|---|---|---|---|
| Linux `.tar.gz`·헤드리스 `.tar.gz` | `plugins/` | 스테이징 폴더 복사 | 목록의 파일 집합, 푼 트리의 바이트 |
| Linux `.deb`·`.rpm` | `/usr/lib/tasty/plugins/` | `Cargo.toml` assets의 `target/bundled-plugins/**/*` glob 하나 | 패키지 목록의 파일 집합(`rpm` 명령이 없으면 rpm은 건너뛴다) |
| Linux `.AppImage` | `usr/bin/plugins/` | 스테이징 폴더 복사 | linuxdeploy에 넘긴 AppDir의 바이트 |
| macOS `Tasty.app`·`.dmg`·헤드리스 `.tar.gz` | `Contents/Resources/plugins/`, 헤드리스는 `plugins/` | 스테이징 폴더 복사(앱 codesign 전) | 앱·마운트한 DMG·푼 헤드리스 트리의 바이트, 헤드리스 목록의 파일 집합 |
| Windows `.zip`·헤드리스 `.zip` | `plugins\` | 스테이징 폴더 복사 | 푼 트리의 파일 집합·SHA256 |
| Windows `.msi` | `bin\plugins\` | 빌드 때 만드는 `target\bundled-plugins.wxs` | 관리 설치(또는 `dark.exe -x`) 트리의 파일 집합·SHA256 |

- deb·rpm assets는 mode를 적지 않아 파일 권한을 그대로 싣는다. `build-linux.sh`가 서명 뒤 실행 권한이 있는 파일은 755, 나머지는 644로 맞춘다. 두 도구는 `--profile <p>`를 받으면 assets의 `target/release/` 접두를 `target/<p>/`로 바꿔 읽고, `target/bundled-plugins`는 그대로 읽는다.
- MSI fragment는 받은 폴더를 그대로 옮긴 `DirectoryRef Id='PluginsDir'`와 `ComponentGroup Id='BundledPlugins'`다. Directory·Component·File Id는 상대 경로의 SHA-256 앞 24자리라 폴더 구성이 같으면 같다. `wix/main.wxs`는 `Bin` 아래에 빈 `PluginsDir`만 두고 `Binaries` 기능이 그 그룹을 담는다. `cargo wix --include`로 fragment를 넘긴다. doc-guards `wix_takes_bundled_plugins_from_the_generated_group`가 `main.wxs`에 플러그인 파일이 남지 않았는지와 이 연결을 확인한다. 생성 결과는 MSI를 만든 뒤 위 대조가 확인한다.
- MSI의 루트 인벤토리 검사는 `bin\plugins\` 밖의 `THIRD_PARTY_LICENSES.md`만 센다. PE 검사의 실행 파일 수는 본체 하나와 스테이징 폴더의 `<id>` 폴더 수다.
- 플러그인 레포가 새 릴리스에서 파일을 더하거나 빼도 패키징 설정은 고치지 않는다. 잠금 파일만 갱신한다([잠금 파일 갱신](#잠금-파일-갱신)).

새 공식 플러그인을 번들에 넣을 때 tasty에서 고칠 곳은 잠금 파일(`generate`), `builtin.rs::BUILTINS`(windows·non-windows 두 벌), [공식 id 목록](#공식-id-목록)이다.

비-staging(번들 산출물 아님): `~/.tasty/known-plugins.toml`(사용자 trust DB, 런타임 생성) · `.pub` sidecar(번들 plugin 은 없음 — 공개키는 호스트 바이너리 embed. 커스텀 plugin 은 위 서명 표).

## 생태계 정책 — 자동 upgrade · 호환성 분류

plugin 시스템의 작성 형식·배포·신뢰·호환성·hot reload 정책과, 번들 plugin 자동 upgrade 동작. 3 카테고리(host-native/bundled/user) 정의는 [concepts/plugins](../concepts/plugins.md), 패키징/서명은 위 [서명](#서명) · [배포 패키지의 번들 플러그인](#배포-패키지의-번들-플러그인) 절. 이 절의 "built-in"/`BUILTINS` 는 *bundled plugin* 을 가리킨다.

### 정책 (현행)

| 영역 | 현재 지원 | 다시 결정할 조건 |
|------|-----------|------------------|
| 작성 형식 | 별도 OS 프로세스. Rust SDK 제공. WASM 실험은 보류 | 다른 언어·강제 격리가 필요한 실제 소비자 |
| 배포 | 로컬 경로 설치와 앱 동봉 번들. 공개 마켓플레이스 미지원 | 외부 플러그인의 배포·독립 업데이트 요구 |
| 신뢰 | 매니페스트 서명 판정·미인증 설치 선택. 설치 뒤에는 권한 제한 없음 | OS 접근까지 제한해야 하는 요구 |
| 호환 | HOST_API_VERSION 메이저 일치. 추가 필드는 optional+default | 기존 필드의 의미나 필수 조건 변경 |
| 재적재 | disable→enable 또는 아래 개발용 자동 reload | 상태를 잃지 않는 교체가 실제로 필요할 때 |

플러그인의 직접 파일·네트워크 접근은 제한하지 않는다.
OS 샌드박스는 아직 제공하지 않으며 WASM도 지원 기능으로 안내하지 않는다.
제품의 보안 기준은 [플러그인 원칙](../principles/plugins.md)의 인증과 미인증 설치 경고다. 마켓플레이스의 도입 조건으로 OS 샌드박스나 설치 후 권한 제약을 요구하지 않는다. 현재 release의 매니페스트 서명 거절·신뢰 DB 검사는 새 인증 제도와 다르며 원칙을 충족한 구현으로 설명하지 않는다.
Lua는 호스트에 등록한 사용자 스크립트 실행 기능이며 플러그인 실행 형식과 별개다.

### 프로토콜 호환성

현재 handshake는 HOST_API_VERSION의 메이저 일치를 검사한다. optional+default 필드 추가는 기존 reader와 양립하고, required 필드·타입·의미·enum 변경은 기존 소비자를 깨뜨릴 수 있다. 이 영향은 검토하되 패키지 버전·유예 여부를 자동으로 강제하지 않는다. [호환성 원칙](../principles/platform-and-compatibility.md)에 따라 개발자가 결정한다. 프로토콜의 현재 형식은 crates/tasty-plugin-protocol, 변경 안내는 그 CHANGELOG에 있다.

### 번들 plugin 자동 upgrade

호스트와 함께 배포되는 builtin 은 사용자 디렉토리에 1 회 복사된 후에도 부팅 시 bundle 의 새 버전이 있으면 자동 갱신된다. 기준은 **매니페스트 `version`(semver)** — mtime 은 tarball 압축 해제 시 보존돼 1차 신호로 부적합.

#### 동작 (`install_builtins_if_needed`)

BUILTINS 각 항목에 대해 bundle vs 설치본 매니페스트 version 비교:

- `bundle > installed` → mtime 무시 덮어쓰기 + 옛 잔존 파일 제거. 로그 `upgrading builtin '<id>' v<old> → v<new>`.
- `bundle == installed` → 내용을 비교해 다른 파일만 옮긴다. 대상이 없거나 크기가 다르면 복사하고, 같은 크기는 64KiB씩 바이트를 비교해 첫 차이에서 멈춘다. mtime은 사용하지 않으며 서명용 해시와 이 로컬 비교는 별개다.
- `bundle < installed` → skip(자동 다운그레이드 금지).
- 매니페스트 파싱 실패: bundle corrupt → skip / installed corrupt + bundle ok → 내용 sync 복구.

#### bundle signature 검증

bundle 의 `tasty-plugin.toml` 은 ed25519 detached signature(`.sig` sidecar)로 보호. 검증은 plugin 로드 시점(`discovery.rs` 의 `trust_outcome`)과 `upgrade-builtins` 경로(`verify_builtin_bundle_trust`)에서 한다. release 빌드는 검증 실패 시 차단하고, debug 빌드는 로드 시점 검증을 건너뛰고 upgrade 경로에서는 `tracing::debug!` 로만 남긴다(`#[cfg(debug_assertions)]`). 키·회전은 위 [서명](#서명) 절.

#### 수동 재설치 — `tasty plugin upgrade-builtins`

- `--force` — 동일/하위 버전도 강제 덮어쓰기(corruption 복구).
- `--restore-removed <ID>`(반복) / `--restore-removed-all` — `tasty plugin remove` 로 `removed_builtins` 에 박힌 항목을 unmark 해 재설치 대상화. 부팅 자동 install 경로는 절대 unmark 하지 않음 — 이 flag 만 진입점.
- `--restart-running` — graceful swap(실행 중 process 를 config 의 enabled 미변경으로 shutdown→respawn). POSIX inode 교체 + Windows sharing violation 양쪽 해소. default off — swap 중 해당 plugin surface 가 잠깐 missing.

응답은 항목별 `BuiltinUpgradeReport`(`Upgraded`/`Reinstalled`/`Skipped`/`NotInBundle`/`Failed`). `--restart-running` 없이 호출하면 in-place 교체만 — 실행 중 process 는 옛 binary 유지하므로 `disable`→`enable`(또는 다음 부팅) 필요. Windows in-place 교체는 sharing violation 으로 `Failed` → `--restart-running` 재호출로 성공.

#### 사용자 수정 영역

builtin 디렉토리는 **host-owned** — 자동/수동 upgrade 가 `overwrite_builtin_dir` 로 사용자 추가 파일 제거 가능. 보존 상태(disabled/removed_builtins/단축키 override)는 디렉토리 *밖* `~/.tasty/plugins.toml` 에 있어 영향 없음.

#### 매니페스트 version bump

plugin 작성자가 의미적 변경 시 `tasty-plugin.toml::version` 을 수동 bump 해야 자동 upgrade 가 동작한다. 루트 앱 버전과는 별개다. plugin을 바꾸면 해당 매니페스트·Cargo 버전과 lock 파일을 함께 갱신한다. version 그대로면 동일버전 분기(내용 resync)로 떨어져 파일은 옮겨지지만, 같은 버전 이름으로 서로 다른 산출물이 존재하게 된다.

#### 개발용 자동 reload — `TASTY_PLUGIN_AUTO_RELOAD`

dev workspace 에서 `cargo build -p tasty-plugin-X --release` 반복 시 수동 disable/enable 없이 새 binary 즉시 적용. env 가 빈 문자열/`"0"` 아니면 부팅 시 활성(production 기본 off — flag off 면 pump tick 부담 0). 신호: 실행 중 plugin 의 entry binary mtime 또는 매니페스트 version 변화. polling `AUTO_RELOAD_POLL_INTERVAL`(2 초). swap 은 `--restart-running` 과 동일 helper(`plugins.toml::disabled` 미수정). respawn 실패 시 warn + baseline 갱신(무한 swap 차단), 재시도 반복을 막는다.

### i18n 키 충돌

plugin 의 `lang/` 키는 **plugin id prefix** 권장(`com.example.explorer.menu.refresh`). 충돌 시 마지막 로드가 이김. 1.0 시점에 prefix 강제 여부 결정.

## 트러블슈팅

| 증상 | 조치 |
|------|------|
| `Skipped { signature-invalid }` 로 builtin 미설치 | non-debug 인데 `.sig` 부재. 패키징본(exe-relative `plugins/`)은 `sign-bundle.sh` 후 재빌드. workspace 산출물 직접 실행(`target/<profile>/tasty.exe`)은 non-debug 에서 dev bundle 동기화를 안 하므로, `PROFILE=<profile> just build-plugins`(서명 + `.sig` 스테이징)로 `target/<profile>/builtin-plugins/` 를 다시 채운다 |
| 로컬 release 에서 dev key 서명 검증 실패 | `dev-pubkey.bin` 이 사용 private key 와 불일치 → `gen-dev-key.sh` 로 두 파일 함께 갱신 |
| Windows `candle could not be found` | WiX 3.x 미설치/`WIX` env 누락 → `winget install WiXToolset.WiXToolset` |

## 관련

- [concepts/plugins](../concepts/plugins.md) — 3 카테고리·통합 축
- [plugin-development](plugin-development.md) · [plugin-permissions](plugin-permissions.md) · [api-conventions](api-conventions.md) · [release](release.md) · [debug-ipc](debug-ipc.md)
