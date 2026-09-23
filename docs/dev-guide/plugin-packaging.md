# Plugin 패키징 — 서명 + staging 동기화 + 생태계 정책

번들 plugin 을 release/dist 빌드에 포함시키는 절차: **Ed25519 매니페스트 서명** + **빌드 staging 7 위치 동기화**. 런타임 라이프사이클(자동 install/upgrade)과 작성 형식·신뢰·호환성 정책은 아래 [생태계 정책](#생태계-정책--자동-upgrade--호환성-분류) 절, 제작은 [plugin-development](plugin-development.md).

## 번들 plugin 목록 (SoT)

`crates/tasty-host-plugin/src/builtin.rs::BUILTINS` 가 단일 출처다. 아래 표는 그 복제본이라 낡을 수 있다 — **수를 세지 말고 그 상수를 봐라**(실제로 `agent-stream` 이 빠진 채 "8 종" 으로 남아 있었다):

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

plugin 당 산출물: `<bin>`(Windows `.exe`) · `tasty-plugin.toml`(매니페스트) · `tasty-plugin.toml.sig`(서명 sidecar, non-debug 필수) · `lang/{en,ja,ko}.toml`.

### 배포 제외 플래그 (`bundle = false`)

매니페스트 최상위 `bundle` 키(기본 `true`, 스키마: `crates/tasty-plugin-manifest/src/types.rs`)로 **개별 plugin 을 배포 패키징에서만 제외**할 수 있다. `false` 면 dist 스크립트(`build-macos-dmg.sh`/`build-linux.sh`/`build-windows.ps1`)의 plugin 탐색 glob 이 그 crate 를 건너뛰어 DMG/AppImage/MSI 산출물과 실제 바이너리 빌드에는 넣지 않는다. **dev 스테이징**(`just build-plugins`/`link-plugins`)은 이 플래그를 보지 않으므로 로컬 빌드에는 그대로 포함된다 — 데모/PoC plugin 을 개발 중엔 쓰되 출하판엔 빼는 용도.

런타임 `BUILTINS`(`builtin.rs`)에는 그대로 남겨둔다: `install_builtins_if_needed` 가 번들에 없는 builtin 을 debug 로그만 남기고 **graceful skip** 하므로, dev(스테이징됨)는 설치·dist(미스테이징)는 무시로 자연히 갈린다. 현재 `com.tasty.mesh-demo`(egui-mesh PoC)가 유일한 `bundle = false`. **주의**: `bundle = false` 는 glob 기반 위치(4/5/6)와 바이너리 빌드에만 자동 적용되고, 아래 "staging 7 위치 동기화" 표의 **명시(explicit) 위치(1/2/3)는 자동으로 걸러지지 않는다** — 새로 `bundle = false` 를 붙인 plugin 이 있으면 `[package.metadata.deb] assets`/`[package.metadata.generate-rpm] assets`/`wix/main.wxs` 에서도 그 plugin 항목을 수동으로 빼야 한다. mesh-demo 는 WiX 는 애초에 목록에 없었지만 deb/rpm 에는 남아있어 dist 빌드가 `Static file asset has not been built`(cargo-deb)로 fail 하는 실제 사고가 있었다 — deb/rpm assets 에서도 제거해 정정됨.

## 서명

| 항목 | 값 |
|------|-----|
| 알고리즘 | Ed25519 (`ed25519-dalek`) |
| 서명 대상 | `<plugin-dir>/tasty-plugin.toml` 의 SHA-256 digest |
| 서명 파일 | `tasty-plugin.toml.sig` (raw 64 byte) |
| Trust store | `crates/tasty-host-plugin/keys/` 의 `release-pubkey.bin` + `dev-pubkey.bin` (2-slot 배열, `bundle_sig.rs::TRUSTED_PUBKEYS`) — 둘 다 추적 안 함, 매 빌드 로컬 자동생성. `release-pubkey.bin` 은 항상 placeholder 로 남고 실질 검증은 dev 슬롯이 담당 |
| 검증 시점 | plugin 로드(`discovery.rs::trust_outcome`)와 `upgrade-builtins`(`builtin.rs::verify_builtin_bundle_trust`) — release/dist 는 실제 차단, debug 는 건너뛰거나 `debug!` 로그만(`#[cfg(debug_assertions)]`) |

보호 범위는 **매니페스트 한 파일만** — 권한/contributes/kind 가 매니페스트 안이라 변조 시 confused-deputy 가 최대 위험. binary 는 OS codesign(macOS notarization/Windows Authenticode)에 위임, lang/ 등 부속은 검증 밖.

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

`.github/workflows/release.yml` 이 tag push(`v*`)/manual dispatch 시, self-hosted 빌드 러너(macOS/Windows/Linux x64/Linux ARM64)가 각자 `scripts/build-*.sh` 안에서 `scripts/ensure-sign-key.sh`(Windows 는 `build-windows.ps1` 의 자체 키 탐색) → `scripts/sign-bundle.sh --all-builtins` 를 호출해 서명한다. GitHub Secret 은 관여하지 않는다 — 각 러너는 `~/.tasty-keys/release.pem` 이 없으면(4대 전부 없음, 아래 "영구 release 키를 두지 않는 이유" 참고) `gen-dev-key.sh` 로 그 자리에서 새 키를 만들어 서명하고, 그 키는 해당 머신에만 남는다.

repo 의 `.sig` 는 *로컬 release/dev 검증용* — CI 정식 release 는 그 빌드 시점에 생성된 키로 재서명하므로 repo 와 다른 키로 덮어쓰는 게 정상.

### 영구 release 키를 두지 않는 이유

원래는 운영자가 Ed25519 keypair 를 1회 발급해 `tasty-host-plugin` 의 `keys/release-pubkey.bin` 에 영구 커밋하고, 개인키를 GitHub Secret `TASTY_RELEASE_SIGN_KEY` 로 등록해 모든 release 빌드가 공유하는 설계였다. 하지만 이 절차가 실제로 완료된 적이 없어(`release-pubkey.bin` 이 계속 all-zero placeholder) CI 가 매 release 마다 secret 부재로 실패했다.

`install_builtins_if_needed()`(`crates/tasty-host-plugin/src/builtin.rs`) 확인 결과, builtin plugin 은 항상 그 앱 바이너리와 **같은 설치 번들 안에서 로컬로 복사**된다 — 앱 버전과 독립적으로 원격에서 개별 업데이트되는 경로가 없다. 즉 "구버전 바이너리가 신버전 키로 서명된 plugin 을 검증해야 하는" 상황 자체가 없어, 릴리스마다 자기 안에서 완결되는 신뢰 단위다. 그래서 영구 신뢰 루트 대신 **매 빌드 로컬 자동생성 dev 키**로 통일했다 — 수동 키 배포 절차가 없어지고, 유출 리스크도 그 빌드 1 회로 국한된다. 대신 "이 서명이 특정 발급자가 발급했다"는 장기 정체성 보증은 포기하는데, 애초에 이 서명이 막으려는 건 발급자 신원 위조가 아니라 매니페스트 변조에 의한 confused-deputy(위 표 참고)라 이 트레이드오프가 맞는다. 배경·대안은 [ADR-0051](../adr/0051-ephemeral-release-signing-key.md) 참고.

플러그인이 향후 앱 버전과 독립적으로 배포/업데이트되는 마켓플레이스 모델이 생기면 이 결정을 재검토해야 한다(그 경우 영구 신뢰 루트가 다시 필요).

## staging 7 위치 동기화

새 plugin 추가/제거 시 동시 갱신할 위치:

| # | 위치 | 명시/동적 |
|---|------|-----------|
| 1 | `Cargo.toml [package.metadata.deb] assets` | **명시** (per-plugin ×4) |
| 2 | `Cargo.toml [package.metadata.generate-rpm]` | **명시** (×4) |
| 3 | `wix/main.wxs` `<Component>` + `<ComponentRef>` | **명시** (×6: bin/manifest/sig/en/ja/ko) |
| 4 | `scripts/build-macos-dmg.sh` staging 루프 | 동적 (`crates/tasty-plugin-*` glob) |
| 5 | `scripts/build-linux.sh::stage_plugins()` | 동적 |
| 6 | `scripts/build-windows.ps1::Stage-Plugins` | 동적 |
| 7 | `builtin.rs::BUILTINS` | **명시** (windows + non-windows cfg) |

1/2/3/7 은 새 plugin 시 **반드시 수정**, 4/5/6 은 glob 자동 발견.

### drift 함정

- **lang 파일 — wix 만 enumerate**: deb/rpm/빌드스크립트는 `lang/*` 자동 포함, wix 는 `LangEn`/`LangJa`/`LangKo` Component 를 *나열* → 새 로케일(`de.toml` 등) 추가 시 wix 만 silent skip → .msi 사용자만 누락. wix 의 해당 plugin Directory 에 `Component`+`ComponentRef` 직접 추가 필요.
- **`.sig` 빌드 시점 의존**: git 에 commit 안 되는 빌드 산출물. 6 staging 위치 모두 비존재 시 non-debug 빌드 fail — CI 가 `sign-bundle.sh` 를 항상 실행하도록 보장.
- **`bundle = false` — 명시 위치(1/2/3)는 자동으로 안 걸러짐**: glob 위치(4/5/6)는 빌드 자체가 그 crate 를 건너뛰지만, deb/rpm assets·wix components 는 plugin 마다 하드코딩된 목록이라 `bundle = false` 여부와 무관하게 그대로 남아있다. mesh-demo 를 deb/rpm assets 에서 안 뺐다가 `cargo-deb`/`cargo-generate-rpm` 이 "빌드 안 된 바이너리를 packaging 하려 함" 으로 dist 빌드 전체가 fail 한 사고가 실제로 있었다(v0.9.5 릴리스). 새로 `bundle = false` 를 붙일 때 1/2/3 에서도 그 plugin 항목을 반드시 제거할 것.

비-staging(번들 산출물 아님): `~/.tasty/known-plugins.toml`(사용자 trust DB, 런타임 생성) · `.pub` sidecar(없음 — 공개키는 호스트 바이너리 embed).

## 생태계 정책 — 자동 upgrade · 호환성 분류

plugin 시스템의 작성 형식·배포·신뢰·호환성·hot reload 정책과, 번들 plugin 자동 upgrade 동작. 3 카테고리(host-native/bundled/user) 정의는 [concepts/plugins](../concepts/plugins.md), 패키징/서명은 위 [서명](#서명) · [staging 7 위치 동기화](#staging-7-위치-동기화) 절. 이 절의 "built-in"/`BUILTINS` 는 *bundled plugin* 을 가리킨다.

### 정책 (현행)

각 결정엔 사건 기반 재검토 trigger 가 있다. 수량 지표는 보조 신호로만 쓴다. 깊은 결정 근거·대안은 [ADR-0009](../adr/0009-plugin-sandbox-deferred.md)(sandbox) · [ADR-0010](../adr/0010-plugin-marketplace-deferred.md)(marketplace).

| 영역 | 결정 | 재검토 trigger |
|------|------|---------------|
| **작성 형식** | Rust crate + Process entry. WASM·Lua 는 별도 layer | 비-Rust 작성 요청 2건+ / 권한 게이트 보안 이슈 1건 |
| **배포** | 로컬 path install(`tasty plugin install <path>`) + 동봉 builtin. marketplace 는 RFC 대기 | 첫 외부 plugin 출시 / 외부 plugin 5+ 자생 |
| **신뢰** | 매니페스트 `permissions[]` + 사용자 grant + IPC method_meta 게이트. 추가 sandbox(seccomp 등) 미지원 | 권한 오해 보안 이슈 1건 |
| **api_version** | `HOST_API_VERSION` 메이저 매치 강제. schema 추가만(optional+default) | — |
| **hot reload** | seamless 미지원. `disable`→`enable` 재시작 안내 | 재빌드 워크플로 비용 명백한 사례 |

- WASM 의 가치는 "가벼움"이 아니라 **강제 가능한 sandbox** 다. 현 권한 모델은 *호스트 API 호출* 만 게이트하고 plugin 이 자기 프로세스에서 직접 fs/network 접근하는 것은 OS process privilege 에 의존한다. 이 한계를 false security 보다 투명하게 명시한다 — 매니페스트 `permissions` 는 "호스트 API 호출 권한"이지 "OS 자원 권한"이 아니다([plugin-permissions](plugin-permissions.md)).
- Lua 는 plugin 과 책임이 다르다(plugin = 시스템 확장, Lua = 사용자 일상 커스터마이징) — 호스트 임베드 별 시스템([lua-hooks](../features/lua-hooks/index.md)).

### 호환성 분류 (plugin-protocol)

| 변경 | 분류 |
|------|------|
| 새 메시지 타입 / optional+default 필드 추가 | minor |
| required 필드 추가 · 필드 의미/타입/nullability 변경·제거 · 에러 코드 의미 변경 · fallback 없는 enum variant 추가 | major |

새 필드는 **반드시 optional + default** 만 허용 → minor 내 호환 유지. plugin 은 별 OS 프로세스 + JSON 이라 ABI 무관, JSON schema 호환성이 본질. 이력은 `crates/tasty-plugin-protocol/CHANGELOG.md`. (IPC 표면 전반 정책은 [api-conventions](api-conventions.md).)

### 번들 plugin 자동 upgrade

호스트와 함께 배포되는 builtin 은 사용자 디렉토리에 1회 복사된 후에도 부팅 시 bundle 의 새 버전이 있으면 자동 갱신된다. 기준은 **매니페스트 `version`(semver)** — mtime 은 tarball 압축 해제 시 보존돼 1차 신호로 부적합.

#### 동작 (`install_builtins_if_needed`)

BUILTINS 각 항목에 대해 bundle vs 설치본 매니페스트 version 비교:

- `bundle > installed` → mtime 무시 덮어쓰기 + 옛 잔존 파일 제거. 로그 `upgrading builtin '<id>' v<old> → v<new>`.
- `bundle == installed` → 내용 기반 sync 만(내용이 다른 파일만 옮긴다, mtime 은 안 본다).
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

plugin 작성자가 의미적 변경 시 `tasty-plugin.toml::version` 을 수동 bump 해야 자동 upgrade 가 동작한다. **루트 앱 자동 패치 +1 정책과 분리** — plugin 단위 변경(매니페스트/permission 추가, behavior 변경)이 있을 때 그 plugin 매니페스트만 bump. version 그대로면 동일버전 분기(내용 resync)로 떨어져 파일은 옮겨지지만, 같은 버전 아래 두 산출물이 남는다.

#### 개발용 자동 reload — `TASTY_PLUGIN_AUTO_RELOAD`

dev workspace 에서 `cargo build -p tasty-plugin-X --release` 반복 시 수동 disable/enable 없이 새 binary 즉시 적용. env 가 빈 문자열/`"0"` 아니면 부팅 시 활성(production 기본 off — flag off 면 pump tick 부담 0). 신호: 실행 중 plugin 의 entry binary mtime 또는 매니페스트 version 변화. polling `AUTO_RELOAD_POLL_INTERVAL`(2초). swap 은 `--restart-running` 과 동일 helper(`plugins.toml::disabled` 미수정). respawn 실패 시 warn + baseline 갱신(무한 swap 차단), 옛 동작으로 graceful degrade.

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
