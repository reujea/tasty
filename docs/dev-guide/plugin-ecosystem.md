# Plugin 생태계 정책

plugin 시스템의 작성 형식·배포·신뢰·호환성·hot reload 정책과, 번들 plugin 자동 upgrade 동작. 3 카테고리(host-native/bundled/user) 정의는 [concepts/plugins](../concepts/plugins.md), 패키징/서명은 [plugin-packaging](plugin-packaging.md). 본 문서의 "built-in"/`BUILTINS` 는 *bundled plugin* 을 가리킨다.

## 정책 (현행)

각 결정엔 사건 기반 재검토 trigger 가 있다. 수량 지표는 보조 신호로만 쓴다. 깊은 결정 근거·대안은 [ADR-0009](../adr/0009-plugin-sandbox-deferred.md)(sandbox) · [ADR-0010](../adr/0010-plugin-marketplace-deferred.md)(marketplace).

| 영역 | 결정 | 재검토 trigger |
|------|------|---------------|
| **작성 형식** | Rust crate + Process entry. WASM·Lua 는 별도 layer | 비-Rust 작성 요청 2건+ / 권한 게이트 보안 이슈 1건 |
| **배포** | 로컬 path install(`tasty plugin install <path>`) + 동봉 builtin. marketplace 는 RFC 대기 | 첫 외부 plugin 출시 / 외부 plugin 5+ 자생 |
| **신뢰** | 매니페스트 `permissions[]` + 사용자 grant + IPC method_meta 게이트. 추가 sandbox(seccomp 등) 미지원 | 권한 오해 보안 이슈 1건 |
| **api_version** | `HOST_API_VERSION` 메이저 매치 강제. schema 추가만(optional+default) | — |
| **hot reload** | seamless 미지원. `disable`→`enable` 재시작 안내 | 재빌드 워크플로 비용 명백한 사례 |

- WASM 의 가치는 "가벼움"이 아니라 **강제 가능한 sandbox** 다. 현 권한 모델은 *호스트 API 호출* 만 게이트하고 plugin 이 자기 프로세스에서 직접 fs/network 접근하는 것은 OS process privilege 에 의존한다. 이 한계를 false security 보다 투명하게 명시한다 — 매니페스트 `permissions` 는 "호스트 API 호출 권한"이지 "OS 자원 권한"이 아니다([plugin-permissions](plugin-permissions.md)).
- Lua 는 plugin 과 책임이 다르다(plugin = 시스템 확장, Lua = 사용자 일상 커스터마이징) — 호스트 임베드 별 시스템([lua-hooks](lua-hooks.md)).

## 호환성 분류 (plugin-protocol)

| 변경 | 분류 |
|------|------|
| 새 메시지 타입 / optional+default 필드 추가 | minor |
| required 필드 추가 · 필드 의미/타입/nullability 변경·제거 · 에러 코드 의미 변경 · fallback 없는 enum variant 추가 | major |

새 필드는 **반드시 optional + default** 만 허용 → minor 내 호환 유지. plugin 은 별 OS 프로세스 + JSON 이라 ABI 무관, JSON schema 호환성이 본질. 이력은 `crates/tasty-plugin-protocol/CHANGELOG.md`. (IPC 표면 전반 정책은 [api-conventions](api-conventions.md).)

## 번들 plugin 자동 upgrade

호스트와 함께 배포되는 builtin 은 사용자 디렉토리에 1회 복사된 후에도 부팅 시 bundle 의 새 버전이 있으면 자동 갱신된다. 기준은 **매니페스트 `version`(semver)** — mtime 은 tarball 압축 해제 시 보존돼 1차 신호로 부적합.

### 동작 (`install_builtins_if_needed`)

BUILTINS 각 항목에 대해 bundle vs 설치본 매니페스트 version 비교:

- `bundle > installed` → mtime 무시 덮어쓰기 + 옛 잔존 파일 제거. 로그 `upgrading builtin '<id>' v<old> → v<new>`.
- `bundle == installed` → 내용 기반 sync 만(내용이 다른 파일만 옮긴다, mtime 은 안 본다).
- `bundle < installed` → skip(자동 다운그레이드 금지).
- 매니페스트 파싱 실패: bundle corrupt → skip / installed corrupt + bundle ok → 내용 sync 복구.

### bundle signature 검증

bundle 의 `tasty-plugin.toml` 은 ed25519 detached signature(`.sig` sidecar)로 보호. 검증은 plugin 로드 시점(`discovery.rs` 의 `trust_outcome`)과 `upgrade-builtins` 경로(`verify_builtin_bundle_trust`)에서 한다. release 빌드는 검증 실패 시 차단하고, debug 빌드는 로드 시점 검증을 건너뛰고 upgrade 경로에서는 `tracing::debug!` 로만 남긴다(`#[cfg(debug_assertions)]`). 키·회전은 [plugin-packaging](plugin-packaging.md).

### 수동 재설치 — `tasty plugin upgrade-builtins`

- `--force` — 동일/하위 버전도 강제 덮어쓰기(corruption 복구).
- `--restore-removed <ID>`(반복) / `--restore-removed-all` — `tasty plugin remove` 로 `removed_builtins` 에 박힌 항목을 unmark 해 재설치 대상화. 부팅 자동 install 경로는 절대 unmark 하지 않음 — 이 flag 만 진입점.
- `--restart-running` — graceful swap(실행 중 process 를 config 의 enabled 미변경으로 shutdown→respawn). POSIX inode 교체 + Windows sharing violation 양쪽 해소. default off — swap 중 해당 plugin surface 가 잠깐 missing.

응답은 항목별 `BuiltinUpgradeReport`(`Upgraded`/`Reinstalled`/`Skipped`/`NotInBundle`/`Failed`). `--restart-running` 없이 호출하면 in-place 교체만 — 실행 중 process 는 옛 binary 유지하므로 `disable`→`enable`(또는 다음 부팅) 필요. Windows in-place 교체는 sharing violation 으로 `Failed` → `--restart-running` 재호출로 성공.

### 사용자 수정 영역

builtin 디렉토리는 **host-owned** — 자동/수동 upgrade 가 `overwrite_builtin_dir` 로 사용자 추가 파일 제거 가능. 보존 상태(grants/disabled/removed_builtins/단축키 override)는 디렉토리 *밖* `~/.tasty/plugins.toml` 에 있어 영향 없음.

### 매니페스트 version bump

plugin 작성자가 의미적 변경 시 `tasty-plugin.toml::version` 을 수동 bump 해야 자동 upgrade 가 동작한다. **루트 앱 자동 패치 +1 정책과 분리** — plugin 단위 변경(매니페스트/permission 추가, behavior 변경)이 있을 때 그 plugin 매니페스트만 bump. version 그대로면 동일버전 분기(내용 resync)로 떨어져 파일은 옮겨지지만, 같은 버전 아래 두 산출물이 남는다.

### 개발용 자동 reload — `TASTY_PLUGIN_AUTO_RELOAD`

dev workspace 에서 `cargo build -p tasty-plugin-X --release` 반복 시 수동 disable/enable 없이 새 binary 즉시 적용. env 가 빈 문자열/`"0"` 아니면 부팅 시 활성(production 기본 off — flag off 면 pump tick 부담 0). 신호: 실행 중 plugin 의 entry binary mtime 또는 매니페스트 version 변화. polling `AUTO_RELOAD_POLL_INTERVAL`(2초). swap 은 `--restart-running` 과 동일 helper(`plugins.toml::disabled` 미수정). respawn 실패 시 warn + baseline 갱신(무한 swap 차단), 옛 동작으로 graceful degrade.

## i18n 키 충돌

plugin 의 `lang/` 키는 **plugin id prefix** 권장(`com.example.explorer.menu.refresh`). 충돌 시 마지막 로드가 이김. 1.0 시점에 prefix 강제 여부 결정.

## 관련

- [concepts/plugins](../concepts/plugins.md) — 3 카테고리·통합 축 · [plugin-packaging](plugin-packaging.md) — 서명/staging
- [plugin-development](plugin-development.md) · [plugin-permissions](plugin-permissions.md) · [api-conventions](api-conventions.md)
