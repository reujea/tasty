# Plugin 패키징 — 서명 + staging 동기화 + 생태계 정책

번들 plugin 을 release/dist 빌드에 포함시키는 절차: **Ed25519 매니페스트 서명** + **빌드 staging 7 위치 동기화**. 런타임 라이프사이클(자동 install/upgrade)과 작성 형식·신뢰·호환성 정책은 아래 [생태계 정책](#생태계-정책--자동-upgrade--호환성-분류) 절, 제작은 [plugin-development](plugin-development.md).

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
- 레포 `crates/*/tasty-plugin.toml`의 id도 모두 이 목록에 있어야 한다(`repository_plugin_manifests_are_official`, 매니페스트 9개).
- 새 공식 플러그인은 `BUILTINS`와 별도로 이 목록에 한 줄을 넣는다. 위 시험들과 목록 길이 단언(`every_official_id_is_admitted`)을 함께 고친다.
- 번들 설치·갱신은 번들 폴더 매니페스트의 `id`가 폴더 이름(`BuiltinSpec::id`)과 같은지 복사 전에 확인한다. 다르면 복사·권한 기록 없이 실패로 남긴다.

plugin 당 산출물: `<bin>`(Windows `.exe`) · `tasty-plugin.toml`(매니페스트) · `tasty-plugin.toml.sig`(서명 sidecar, non-debug 필수) · `lang/{en,ja,ko}.toml`.

### 배포 제외 플래그 (`bundle = false`)

매니페스트 최상위 `bundle` 키(기본 `true`, 스키마: `crates/tasty-plugin-manifest/src/types.rs`)로 **개별 plugin 을 배포 패키징에서만 제외**할 수 있다. `false` 면 dist 스크립트(`build-macos-dmg.sh`/`build-linux.sh`/`build-windows.ps1`)의 plugin 탐색 glob 이 그 crate 를 건너뛰어 DMG/AppImage/MSI 산출물과 실제 바이너리 빌드에는 넣지 않는다. **dev 스테이징**(`just build-plugins`/`link-plugins`)은 이 플래그를 보지 않으므로 로컬 빌드에는 그대로 포함된다 — 데모/PoC plugin 을 개발 중엔 쓰되 배포 패키지에서는 빼는 용도.

런타임 `BUILTINS`(`builtin.rs`)에는 그대로 남겨둔다: `install_builtins_if_needed` 가 번들에 없는 builtin 을 debug 로그만 남기고 건너뛰므로, 개발 빌드는 스테이징된 plugin을 설치하고, 배포 빌드는 번들에 없는 항목을 건너뛴다. 현재 `com.tasty.mesh-demo`와 `com.tasty.agent-stream`이 `bundle = false`다. **주의**: `bundle = false` 는 glob 기반 위치(4/5/6)와 바이너리 빌드에만 자동 적용되고, 아래 "staging 7 위치 동기화" 표의 **명시(explicit) 위치(1/2/3)는 자동으로 걸러지지 않는다** — 새로 `bundle = false` 를 붙인 plugin 이 있으면 `[package.metadata.deb] assets`/`[package.metadata.generate-rpm] assets`/`wix/main.wxs` 에서도 그 plugin 항목을 수동으로 빼야 한다. 누락하면 패키저가 만들지 않은 바이너리를 찾다가 실패한다.

## 서명

| 항목 | 값 |
|------|-----|
| 알고리즘 | Ed25519 (`ed25519-dalek`) |
| 서명 대상 | `<plugin-dir>/tasty-plugin.toml` 의 SHA-256 digest |
| 서명 파일 | `tasty-plugin.toml.sig` (raw 64 byte) |
| Trust store | `crates/tasty-host-plugin/keys/`의 `release-pubkey.bin`과 `dev-pubkey.bin` 두 슬롯. 추적하지 않는 로컬 파일이며, 없는 슬롯은 build.rs가 placeholder로 채운다. 실제 서명 키에 대응하는 공개키가 빌드에 포함돼야 한다 |
| 검증 시점 | 설치·확인 필요 탭 등록(`trust_verdict::judge`, debug·release 공통 — 미인증은 사용자가 고를 때만 설치·등록), plugin 로드(`discovery.rs::trust_outcome`)와 `upgrade-builtins`(`builtin.rs::verify_builtin_bundle_trust`) — 뒤의 둘은 release/dist 에서 판정하고 debug 는 건너뛰거나 `debug!` 로그만(`#[cfg(debug_assertions)]`) |
| 공개 키 파일 | 커스텀 plugin 은 `tasty-plugin.toml.pub`(raw 32 byte)를 함께 배포해야 사용자가 그 키를 신뢰할 수 있다. 서명은 공개 키를 드러내지 않는다 |

보호 범위는 **매니페스트 한 파일만** — 권한/contributes/kind 가 매니페스트 안이라 변조되면 host가 잘못된 권한·기여 정보를 신뢰할 수 있다. binary 는 OS codesign(macOS notarization/Windows Authenticode)에 위임, lang/ 등 부속은 검증 밖.

### dev key (개발자 1회)

```bash
./scripts/gen-dev-key.sh
# → ~/.tasty-keys/dev.pem  (600, gitignored)
# → crates/tasty-host-plugin/keys/dev-pubkey.bin  (public 32B, 로컬 전용 — 추적 안 함)
```

`dev-pubkey.bin` 은 개발자별 로컬 키라 **추적하지 않는다** (`keys/.gitignore`). 파일이 없어도 build.rs 가 OUT_DIR 슬롯을 all-zero placeholder 로 채워 컴파일은 안 깨지지만, dev key 서명 plugin 을 자동 trust 하려면 `dev-pubkey.bin` 이 서명에 쓰는 `dev.pem` 과 일치해야 한다.

`gen-dev-key.sh` 는 idempotent 하다 — `dev.pem` 이 이미 있으면 private key 는 유지하고 `dev-pubkey.bin` 만 그 키에서 재도출한다. `build-*.sh` / `build-windows.ps1` 도 dev 키 경로에서 cargo build 직전 항상 `gen-dev-key.sh` 를 호출하므로, `dev.pem` 만 있고 `dev-pubkey.bin` 이 없는 상태(새 클론·추적 해제 후)에서도 placeholder 가 아닌 실제 trust 키가 임베드된다.

### 재서명 (로컬)

`tasty-plugin.toml` 을 한 줄이라도 고치면 기존 `.sig` 무효화. 빌드 직전 재서명:

```bash
./scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --all-builtins
```

dist 스크립트(`build-macos-dmg.sh`/`build-linux.sh`/`build-windows.ps1`)와 **로컬 release/dist `just` 빌드**(`build-plugins`/`link-plugins`, 즉 `just build --release`·`just run --release`·`just build-all`)가 모두 패키징/스테이징 직전 자동으로 `sign-bundle.sh --all-builtins` 를 호출한다 — 매니페스트만 고치고 빌드 없이 확인할 때만 수동 호출. debug 프로필은 게이트가 꺼져 있어 서명 단계를 건너뛴다.

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

## staging 7 위치 동기화

새 plugin 추가/제거 시 동시 갱신할 위치:

| # | 위치 | 명시/동적 |
|---|------|-----------|
| 1 | `Cargo.toml [package.metadata.deb] assets` | **명시** (per-plugin ×4) |
| 2 | `Cargo.toml [package.metadata.generate-rpm]` | **명시** (×4) |
| 3 | `wix/main.wxs` `<Component>` + `<ComponentRef>` | **명시** (×7: bin/manifest/sig/en/ja/ko/zh-Hans) |
| 4 | `scripts/build-macos-dmg.sh` staging 루프 | 동적 (`crates/tasty-plugin-*` glob) |
| 5 | `scripts/build-linux.sh::stage_plugins()` | 동적 |
| 6 | `scripts/build-windows.ps1::Stage-Plugins` | 동적 |
| 7 | `builtin.rs::BUILTINS` | **명시** (windows + non-windows cfg) |

1/2/3/7 은 새 plugin 시 **반드시 수정**, 4/5/6 은 glob 자동 발견.

### drift 함정

- **번역 파일**: deb/rpm과 빌드 스크립트는 `lang/*`를 자동 포함한다. WiX는 `LangEn`·`LangJa`·`LangKo`·`LangZhHans`를 각각 나열하므로, 새 언어를 추가하면 해당 plugin Directory의 `Component`와 `ComponentRef`도 직접 추가해야 MSI에 포함된다. `crates/tasty-doc-guards/tests/wix_plugin_lang_matches_bundled_plugins.rs`가 `Source`와 번들 plugin(`bundle = false` 제외)의 `lang/*.toml`을 양방향으로 대조하고, 번역 `Component`와 `ComponentRef`의 짝, 번역 파일과 그것을 감싼 `Plugin<이름>Dir`·`Plugin<이름>LangDir`의 짝을 확인한다. 빠진 파일 이름을 대며 실패한다. MSI를 만들어 설치 결과를 보지는 않는다.
- **`.sig` 빌드 시점 의존**: git 에 commit 안 되는 빌드 산출물. 6 staging 위치 모두 비존재 시 non-debug 빌드 fail — CI 가 `sign-bundle.sh` 를 항상 실행하도록 보장.
- **`bundle = false` — 명시 위치(1/2/3)는 자동으로 안 걸러짐**: glob 위치(4/5/6)는 빌드 자체가 그 crate 를 건너뛰지만, deb/rpm assets·wix components 는 plugin 마다 하드코딩된 목록이라 `bundle = false` 여부와 무관하게 그대로 남아있다. 새로 `bundle = false` 를 붙일 때 1/2/3 에서도 그 plugin 항목을 반드시 제거할 것.

비-staging(번들 산출물 아님): `~/.tasty/known-plugins.toml`(사용자 trust DB, 런타임 생성) · `.pub` sidecar(번들 plugin 은 없음 — 공개키는 호스트 바이너리 embed. 커스텀 plugin 은 위 서명 표).

## 생태계 정책 — 자동 upgrade · 호환성 분류

plugin 시스템의 작성 형식·배포·신뢰·호환성·hot reload 정책과, 번들 plugin 자동 upgrade 동작. 3 카테고리(host-native/bundled/user) 정의는 [concepts/plugins](../concepts/plugins.md), 패키징/서명은 위 [서명](#서명) · [staging 7 위치 동기화](#staging-7-위치-동기화) 절. 이 절의 "built-in"/`BUILTINS` 는 *bundled plugin* 을 가리킨다.

### 정책 (현행)

| 영역 | 현재 지원 | 다시 결정할 조건 |
|------|-----------|------------------|
| 작성 형식 | 별도 OS 프로세스. Rust SDK 제공. WASM 실험은 보류 | 다른 언어·강제 격리가 필요한 실제 소비자 |
| 배포 | 로컬 경로 설치와 앱 동봉 번들. 공개 마켓플레이스 미지원 | 외부 플러그인의 배포·독립 업데이트 요구 |
| 신뢰 | 매니페스트 권한·사용자 승인·호스트 IPC 검사 | OS 접근까지 제한해야 하는 요구 |
| 호환 | HOST_API_VERSION 메이저 일치. 추가 필드는 optional+default | 기존 필드의 의미나 필수 조건 변경 |
| 재적재 | disable→enable 또는 아래 개발용 자동 reload | 상태를 잃지 않는 교체가 실제로 필요할 때 |

호스트 API 권한은 플러그인의 직접 파일·네트워크 접근을 제한하지 않는다.
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

builtin 디렉토리는 **host-owned** — 자동/수동 upgrade 가 `overwrite_builtin_dir` 로 사용자 추가 파일 제거 가능. 보존 상태(grants/disabled/removed_builtins/단축키 override)는 디렉토리 *밖* `~/.tasty/plugins.toml` 에 있어 영향 없음.

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
