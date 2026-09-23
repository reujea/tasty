# 빌드 가이드

tasty 의 워크스페이스 구조, 빌드 프로필, 빌드 시간 최적화. 강제 정책(어떤 프로필을 언제 쓰는지)은 [`../../CLAUDE.md`](../../CLAUDE.md) "빌드".

## 워크스페이스 구조

cargo workspace — **본 바이너리(`src/`) + `crates/*`**. 크레이트 수와 전수 목록은 [architecture/index.md](../architecture/index.md) 가 정본이고 `architecture_crate_list_complete` 가 그것을 강제한다 — 여기서 수를 복제하지 않는다(복제본은 **가드 밖이면** 낡는다. 실제로 48 로 낡아 있었고, 그 값이 마지막까지 남아 있던 곳이 두 README 의 Workspace 배지와 본문이다). 복제를 남기기로 한 자리는 대조를 함께 붙였다 — 두 README 는 `readme_badge_parity`, 루트 `CLAUDE.md` "빌드" 절은 `architecture_crate_list_complete` 가 본다. 셋 다 좌변이 같은 함수(`tasty_doc_guards::crate_layers::crate_dir_names`)라 답이 갈리지 않는다. 크레이트는 레이어로 나뉜다(전체 목록·각 역할은 `crates/` 와 각 `Cargo.toml`):

| 레이어 | 예 | 성격 |
|--------|-----|------|
| **type-\*** primitive | `tasty-type-geometry`(길이), `tasty-type-appearance`(색·theme schema) | 최하위 schema/primitive |
| 도메인 leaf (GUI-free) | `tasty-model`, `tasty-i18n`, `tasty-settings`, `tasty-themes`, `tasty-terminal`, `tasty-memory`, `tasty-hooks`, `tasty-ipc`, `tasty-ssh`, `tasty-remote`, `tasty-portscan` 등 | 공용 도메인·IO |
| plugin 인프라 | `tasty-plugin-protocol`, `tasty-plugin-sdk`, `tasty-plugin-manifest`, `tasty-host-plugin`, `tasty-plugin-agent-common` | 호스트↔plugin 와이어·SDK·번들 plugin 공용 헬퍼 |
| 번들 plugin | `tasty-plugin-{claude,codex,image,html,markdown,git-viewer,clipboard-viewer,mesh-demo,agent-stream}` | → [`../plugins/`](../plugins/index.md) |
| CLI / 테스트 | `tasty-cli`, `tasty-tui-simulator` | |

본 바이너리는 `pub use tasty_core as ...` 식으로 재수출해 `crate::model::X` / `crate::theme::theme()` 같은 경로가 그대로 동작한다.

**루트 패키지는 `lib` 와 `bin` 두 타깃이다.** `src/lib.rs` 가 모듈 트리와 위 재수출을 들고, `src/main.rs` 는 `tasty::boot::run()` 만 부른다.

- **루트 패키지의 단위시험은 전부 lib 타깃에 산다.** `cargo test -p tasty --bin tasty` 로 좁히면 **0 건이 돌고 초록이 난다** — 0 건은 통과가 아니라 미측정이다. 좁히려면 `cargo test -p tasty --lib` 다. (`--lib --bins` 를 함께 주는 조합은 예전과 같은 것을 담는다.)
- **가른 것이 경계는 아니다.** 모듈은 여전히 전부 비공개이거나 `pub(crate)` 고, 밖으로 나가는 것은 `boot::run` 과 기존 재수출뿐이다. 생긴 것은 **공개 표면을 잴 좌변**이다 — 재는 법: `cargo doc -p tasty --no-deps`. 첫 값(실측 2026-09-20): 루트 페이지가 모듈 **둘**(`boot` · `paths`)과 함수 **열**(`boot::run` + `paths::*` 아홉)을 든다(`target/doc/tasty/sidebar-items.js` · `all.html`). **그 목록이 표면 전부는 아니다** — `pub use tasty_font as font;` 류의 **크레이트 통째 재수출** 셋(`font` · `settings` · `theme`)은 `--no-deps` 에서 페이지가 안 생겨 목록에 안 뜨지만 소비자에게는 그 크레이트의 공개 API 전부가 나간다. 좁히려면 그 셋부터 본다.
- 오늘 이 크레이트의 doctest 는 **0 건**이다(`cargo test -p tasty --doc`). `src/` 의 코드펜스는 비공개 항목의 주석 안에 있어 rustdoc 이 수집하지 않는다. `doctest = false` 를 걸지 않은 이유가 이것이고, 이 수가 움직이면 그때 자리를 정한다.

### type-\* layer 의존 규약 (필수)

`tasty-type-*` 는 **primitive/schema 레이어**다.

- 그룹 내부 의존은 자유 (예: `tasty-type-appearance → tasty-type-geometry`).
- **도메인/IO crate 의존 금지** — `tasty-model`/`tasty-themes`/본 바이너리 등을 의존하지 않는다.
- **그룹 내 순환 금지.**

새 type-\* crate 도 이 3원칙을 따른다 — 의존 그래프가 한 방향으로 유지되어 순환 위험 0.

## 빌드 프로필 (3종)

| 프로필 | 정의 | LTO | 용도 |
|--------|------|-----|------|
| `dev` (기본) | `opt-level = 0` | off | 일상 개발 `cargo build` |
| `release` | `opt-level = 3`, `strip = true` | **thin** | 최적화 검증 `cargo build --release` |
| `dist` | `inherits = "release"` | **full** (`lto = true`) | 배포 산출물 `cargo build --profile dist` |

- **`dev` 는 본체와 등재되지 않은 워크스페이스 크레이트가 opt 0 이다**: 의존성 전체는 `[profile.dev.package."*"]` 로 opt 3 이고, 워크스페이스 크레이트는 glob 에 안 걸려 루트 `Cargo.toml` 에 `[profile.dev.package.<이름>]` 으로 **하나씩 등재**된 것만 opt 3 이다. 새 크레이트를 만들면 등재 여부를 정한다 — 셀 렌더러의 잎 크레이트 셋을 실행·편집 비용으로 잰 예와 재는 절차는 [ADR-0381](../adr/0381-the-cell-renderer-leaf-crates-are-optimized-in-dev.md).
- **`release` = thin LTO**: 크레이트 IR 요약을 공유해 cross-crate inlining 을 **병렬** 적용. full 의 95–99% 효과를 1/3 시간에 — 일상 "릴리즈 검증" 은 모두 이걸 쓴다.
- **`dist` = full LTO**: 모든 IR 을 단일 LLVM 모듈로 합쳐 재최적화. 단일 스레드 단계가 길어 약 3.5배 느림. **배포 바이너리(DMG/MSI/AppImage) 빌드 시에만** 쓴다. (AI 자체 검증 빌드에는 절대 사용 금지.)

```bash
cargo build                 # debug
cargo check                 # 타입검사만 (가장 빠름), -p <crate> 로 단일 크레이트
cargo build --release       # thin LTO 검증
cargo build --profile dist  # 배포용
```

### headless(`--no-default-features`) 빌드

`gui` 는 default feature 라 `cargo build` 는 winit/wgpu/egui 를 켠다. gui 없는 headless/CLI 인스턴스(로컬 GUI 사용자 없이 IPC/CLI + 원격 attach 만 쓰는 인스턴스, `docs/identity.md` "headless 환경을 의식한다")는 `--no-default-features` 로 빌드한다.

```bash
cargo check --workspace --no-default-features   # headless 컴파일 검증
cargo build --workspace --no-default-features   # headless 빌드
```

#### "컴파일된다" 와 "그래프에 안 들어온다" 는 다른 좌변이다

`--no-default-features` 가 **컴파일되는 것**과 GUI 스택이 **의존 그래프에서 빠지는 것**은
따로 움직인다. 비-optional path 의존 하나가 남아 있으면 컴파일은 그대로 통과하면서 egui·wgpu
가 전부 링크된다. 그래서 그 둘을 각각 잰다.

```bash
# ① 그래프 크기 — name 만 센다(같은 크레이트의 여러 버전을 한 번만 세려면 이 형태다)
cargo tree --no-default-features --edges normal --prefix none | awk '{print $1}' | sort -u | wc -l

# ② 특정 스택이 들어왔는지 — 들어왔으면 누가 끌고 오는지까지 한 줄로 나온다
cargo tree --no-default-features --edges normal -i egui
cargo tree --no-default-features --edges normal -i wgpu
```

**②의 신호는 출력이 아니라 종료 코드다.** 그 이름이 그래프에 없으면 빈 출력이 아니라
`error: package ID specification ... did not match any packages` 로 **죽는다**(rc≠0).
그래서 "안 들어왔다" 와 "명령이 고장났다" 가 같은 모양으로 보인다 — 스크립트에 넣을 때
그 둘을 갈라라.

**그래프 자체를 보는 시험은 없다 — 그 좌변에는 채널이 없다.** 위 두 줄이 그것을 재는 유일한
방법이다. 있는 것은 **크레이트 하나의 선언**을 보는 시험 하나뿐이다
(`crates/tasty-font/src/lib.rs` 의 `wgpu_stays_an_optional_dependency_of_this_crate` —
`wgpu` 가 비-optional 로 돌아가거나 `gpu` feature 가 사라지거나 `default` 가 안 비면 죽는다,
변이로 확인). **그것이 그래프를 보는 것은 아니다** — 다른 크레이트가 wgpu 를 새로 들이면
그 시험은 초록인 채로 스택이 돌아온다.

현재 상태(실측 2026-09-20, 위 ① 규약 = 이름만 유일): 헤드리스 그래프는 **306** 노드다.
없는 것: `egui` 계열(`egui`·`ecolor`·`emath`·`epaint`·`egui_extras`) · `winit` ·
**`wgpu` 계열(`wgpu`·`wgpu-core`·`wgpu-hal`·`wgpu-types`·`naga`·`glow`·`ash`)**.
`wgpu` 가 빠진 것은 `tasty-font` 이 그것을 `gpu` feature 뒤 optional 로 들고 루트의 `gui` 만
켜기 때문이다([ADR-0336](../adr/0336-the-font-crate-splits-at-the-device-boundary.md)).

**아직 있는 것: `cosmic-text`·`fontdb`·`swash`·`skrifa`·`read-fonts`·`ttf-parser`.**
이것은 미완이 아니라 결정이다 — `FontConfig` 의 필드 셋이 cosmic-text 타입이고 헤드리스
코드(`src/boot/locale_font.rs` 의 `family_path`)가 그 타입을 실제로 쓴다. 빼려면 폰트 DB 를
여는 수단 자체를 바꿔야 하고 그것은 **동작 변경**이다.

gui 전용 심볼(`AppState.toasts` 등)을 `#[cfg(feature = "gui")]` 게이팅 없이 쓰면 gui 빌드는 통과하지만 headless 빌드만 깨진다. 이 회귀는 `.github/workflows/crossplatform-check.yml` 의 `check-headless` 잡(`cargo check --workspace --no-default-features --locked`)이 `main` push 마다 자동 검출한다(문서만 바뀐 push 는 제외).

#### 공용 모듈 안의 GUI 전용 정의

경계를 가르는 규칙의 정본은 [headless-build-boundaries](headless-build-boundaries.md) 다 —
세 갈래(`cfg(feature = "gui")` · `cfg(any(feature = "gui", test))` · `expect(dead_code,
reason)`)와 그것을 재는 여덟 칸이 거기 있다. 아래 표는 이 빌드 문서에서 자주 걸리는 구체
사례다.

| 정의 | 빌드 경계 | 공용으로 남는 부분 |
|---|---|---|
| `db::init`, `default_db_path` | GUI 부팅만 호출 | `Db`, `with_state_db`는 headless 최근 파일 조회에서도 사용 |
| `Db::open`·스키마·초기화 오류 분류 | GUI 또는 단위 테스트 | headless의 미초기화 `with_state_db`는 기존대로 `None` 반환 |
| `crash_report::write_hang_report` | winit watchdog을 가진 GUI | panic 리포터 설치와 host 파일 로그는 GUI/headless/CLI 역할별 공용 부팅에서 사용 |
| `crash_report::error_loop`, `record_error` | GUI; detector는 debug, release는 no-op | headless panic/파일 로깅 경로를 함께 제거하지 않음 |

`state.db` 초기화와 `memory.db` 초기화는 다른 경로다. 이 빌드 경계가 headless의
메모리 저장소나 최근 파일 IPC를 끄는 것은 아니다. 오류 관측의 범위는
[crash-diagnostics](crash-diagnostics.md), 저장소 접근은
[storage](../design/systems/storage.md)를 따른다.

본체+플러그인을 한 번에 다루는 래퍼가 있다. `just build` 는 빌드·스테이징만(실행 X), `just run` 은 빌드 후 호스트까지 실행한다. 둘 다 플러그인을 빌드·스테이징하며, 호스트는 부팅 시 builtin 을 동기화하므로(버전으로 갈래를 고르고 같은 버전이면 내용이 다른 파일만 옮긴다 — 설치본이 더 높으면 건너뛴다) 플러그인 소스 변경이 (`just build` 면 다음 실행 시, `just run` 이면 그 실행에서) 반영된다.

```bash
just build                  # 본체+플러그인 debug 빌드·스테이징 (실행 X)
just build --release        # 동일, release 프로필
just run                    # 본체+플러그인 debug 빌드 + 실행
just run --release          # 동일, release 프로필 (나머지 인자는 호스트로 passthrough)
```

**release/dist 로컬 빌드는 builtin 을 자동 재서명한다.** `--release`(및 `--profile dist`)는
trust 게이트가 켜지므로(debug 는 `#[cfg(debug_assertions)]` 로 우회), `build-plugins` 가
cargo build 전에 서명 키를 보장(`scripts/ensure-sign-key.sh`: `SIGN_KEY_PATH` env →
`release.pem` → `dev.pem`+`gen-dev-key.sh`)하고 임베드 `dev-pubkey.bin` 을 재도출한 뒤,
plugin 빌드 후 `sign-bundle.sh --all-builtins` 로 전체 매니페스트를 재서명한다. 따라서
매니페스트를 언제 고치든(plugin 버전 자동 bump 포함) 로컬 release 산출물은 항상 게이트를
통과한다. `just build`/`just run`(debug)은 게이트가 꺼져 있어 서명 단계를 건너뛴다(기본
dev 워크플로에 openssl 의존 미부과). dist 스크립트(`build-*.{sh,ps1}`)도 같은 규칙을
쓴다 — 상세는 [plugin-packaging](plugin-packaging.md).

## 빌드 스크립트 (`build.rs`)

루트 크레이트의 `build.rs` 가 하는 일은 하나다 — **Windows 실행 파일에 아이콘을 넣는 것.**
본문은 통째로 `#[cfg(windows)]` 안에 있고, `winresource` 는 `[target.'cfg(windows)'.build-dependencies]`
에만 선언돼 있어 **Linux · macOS 호스트의 빌드는 그 크레이트를 받지도 않고 스크립트 본문이 빈 `main`** 이다.

**여기서 `windows` 는 빌드 대상이 아니라 빌드 호스트다.** 빌드 스크립트는 호스트용으로 컴파일되므로
그 안의 `#[cfg(windows)]` 는 호스트 OS 로 갈린다. 대상 OS 는 스크립트가 실행 중에
`CARGO_CFG_TARGET_OS` 로 읽어야만 보이는데, 이 스크립트는 그것을 안 읽는다. `winresource`
build-dependency 도 호스트가 Windows 일 때만 해석된다. 실측(2026-09-21, Linux aarch64 호스트에서
`cargo check -p tasty --target x86_64-pc-windows-gnu -vv`): 스크립트는 `CARGO_CFG_TARGET_OS=windows`
로 **돌았지만**, 내보낸 것은 `rerun-if-changed` 두 줄뿐이었다. `OUT_DIR` 은 비었고 `winresource` 는 한 번도
컴파일되지 않았으며, `cargo tree -i winresource --target x86_64-pc-windows-gnu` 도 비었다. 그 머신에
`x86_64-w64-mingw32-windres` 가 있었는데도 그렇다.

그래서 **Windows 가 아닌 호스트(실측은 Linux)에서 교차 빌드한 Windows exe 에는 아이콘도 VERSIONINFO 도 없고, 빌드는 그
사실을 경고하지 않는다.** 출하 산출물은 이 갈래를 타지 않는다 — Windows 산출물은 네이티브 Windows
러너가 만든다(`.github/workflows/release.yml` 의 `runs-on: [self-hosted, Windows]` 잡). 교차 빌드한
exe 는 컴파일 · 시험 확인용으로만 쓰고 배포물로 쓰지 않는다.

### 입력 선언이 곧 계약이다

스크립트는 자기 입력을 `cargo:rerun-if-changed` 로 선언한다. **하나라도 선언하면 cargo 는
"패키지 안의 아무 파일이나 바뀌면 다시 돈다" 는 기본값을 버리고 선언한 것만 본다.** 그래서
목록에서 빠진 입력은 바뀌어도 재실행을 일으키지 못한다. 지금 선언된 입력은 둘이다.

| 입력 | 누가 읽나 |
|---|---|
| `assets/icons/icon.ico` | 스크립트 자신(`set_icon`) |
| `Cargo.toml` | `winresource` — `WindowsResource::new()` 가 `parse_cargo_toml` 을 부른다 |

**`Cargo.toml` 이 입력인 것은 `[package.metadata.winresource]` 섹션 때문이 아니다.** 그 섹션은
지금 없다. 그래도 같은 함수가 exe VERSIONINFO 의 기본값(패키지 이름·버전)을 그 파일에서 읽으므로
파일 자체가 입력이다. `parse_cargo_toml` 은 `toml` feature 로 켜지는데 그것이 그 크레이트의 기본
feature 이고 이 저장소는 `default-features` 를 끄지 않는다(`Cargo.lock` 의 `winresource` 항목에
`toml` 이 의존으로 들어 있는 것이 그 증거다).

### 재실행 동작을 재는 법

**`cargo:warning` 출력 유무를 좌변으로 쓰지 마라** — cargo 는 스크립트가 fresh 여서 안 돌았을
때도 캐시된 warning 을 다시 찍기 때문에 양쪽이 똑같아 보인다. 스크립트가 파일에 한 줄씩
append 하게 해서 **실행 횟수**를 세야 갈린다. 저장소 밖 임시 디렉토리에 최소 크레이트 둘(선언
있음 / 없음)을 만들어 재면 이렇게 나온다.

| | 최초 | 무관한 `src` 편집 후 | `Cargo.toml` 편집 후 | 아이콘 편집 후 |
|---|---|---|---|---|
| 선언 없음 | 1 | 2 | 3 | 4 |
| 선언 있음 | 1 | **1** | 2 | 3 |

즉 선언이 있으면 무관한 편집에는 안 돌고 선언한 입력 둘에는 각각 돈다. `build.rs` 자신의 변경은
선언과 무관하게 언제나 재실행을 일으킨다.

### 이 문서가 못 말하는 것

아이콘이나 `Cargo.toml` 이 바뀌었을 때 **Windows 산출물이 실제로 달라지는지**는 Windows **호스트**에서만
관측된다 — 리소스 컴파일은 빌드 호스트가 Windows 일 때만 돈다. Linux 에서 `--target` 으로 Windows 를
걸어도 리소스 단계는 빈 채로 지나가므로(위 빌드 스크립트 절), 교차 빌드로는 이
물음에 답할 수 없다. 위 표는 재실행 여부까지만 재고 산출물 차이는 재지 않는다.

### 나머지 빌드 스크립트

워크스페이스의 빌드 스크립트는 루트까지 합쳐 일곱이다. 각자 선언하는 것:

| 스크립트 | 선언 | 한 번 돌 때 나가는 줄 수 |
|---|---|---|
| `build.rs` | 아이콘 · `Cargo.toml` | 2 |
| `crates/tasty-host-plugin/build.rs` | `keys` 디렉토리 · `PROFILE` env | 2 |
| `crates/tasty-i18n/build.rs` | `lang/*.toml` 을 하나씩 | 디스크의 `lang/*.toml` 수만큼 |
| `crates/tasty-doc-guards/build.rs` | `src` 디렉토리 통째 | 1 |
| `crates/tasty-plugin-{clipboard-viewer,image,markdown}/build.rs` | `build.rs` 자기 자신만 | 각 1 |

`crates/tasty-cli/src/dynamic/build.rs` 는 **빌드 스크립트가 아니다** — 그 크레이트의 평범한
`mod build` 이고, 어느 `Cargo.toml` 에도 `build = ` 지정이 없다(빌드 스크립트는 패키지 루트의
`build.rs` 만 자동 인식된다).

`tasty-i18n` 은 루프에서 파일마다 한 줄씩 내보내므로 **소스의 `println!` 수(1)와 실제로 나가는
줄 수가 다르다.** 두 값을 섞어 세지 마라.

### plugin 세 개가 자기 자신만 거는 이유

세 plugin 의 빌드 스크립트는 `tasty-icons` 의 `Icon.svg` 문자열을 읽어 `OUT_DIR` 에 점배열을
베이크한다([ADR-0036](../adr/0036-plugin-icon-buildtime-bake-tasty-icons-single-source.md)).
그런데 거는 것은 `build.rs` 자기 자신뿐이다. **형태만 보면 입력을 빠뜨린 것 같지만 아니다** —
아이콘은 파일이 아니라 `crates/tasty-icons/src/lib.rs` 안의 문자열 리터럴이고, 그것이 바뀌면
`tasty-icons` 가 다시 컴파일되며, `tasty-icons` 는 세 스크립트의 **build-dependency** 라 그
재컴파일이 스크립트 재컴파일을, 스크립트 재컴파일이 재실행을 끌고 온다. 즉 입력 선언이 아니라
**의존 그래프**가 트리거를 공급한다.

그 사슬은 관측된 것이다. `FOLDER` 아이콘의 `d` 속성 한 글자를 바꾸고 세 패키지를 다시 빌드하면
`CARGO_LOG=cargo::core::compiler::fingerprint=info` 가 사슬을 그대로 찍는다.

```
tasty-icons  lib   dirty: FsStatusOutdated
  → …/markdown  build-script-build  Build           dirty: FsStatusOutdated(StaleDepFingerprint …)
    → …/markdown  build-script-build  RunCustomBuild  dirty: FsStatusOutdated(StaleDepFingerprint …)
```

그리고 셋 다 다시 돌았다(`target/<profile>/build/<pkg>-<hash>/invoked.timestamp` 가 갱신된다).
`markdown` 만 생성 결과가 달라지는데, `FOLDER` 를 베이크하는 것이 그 하나이기 때문이다 —
**나머지 둘은 자기가 안 쓰는 아이콘이 바뀌어도 다시 돈다.** 트리거는 크레이트 단위라
필요보다 거칠고, 대신 놓치지 않는다.

반대 방향도 쟀다. plugin 자신의 `src/` 를 고치고 같은 빌드를 돌리면 크레이트는 다시
컴파일되지만 **빌드 스크립트는 다시 돌지 않는다**(`invoked.timestamp` 불변). 선언이 하나라도
있으면 "패키지 안 아무 파일" 기본값이 꺼지기 때문이고, 그것이 여기서 노린 바다.

**A/B 로 잴 때 `-p` 목록을 바꾸지 마라.** cargo 의 feature 통일은 선택된 패키지 집합에 따라
달라지고, 달라지면 `-C metadata` 해시가 달라져 `build/<pkg>-<hash>/` 가 **새로 생긴다.** 그
새 디렉토리는 방금 실행된 것처럼 보여서, 재실행이 없는데 있는 것으로 읽힌다. 앞뒤 빌드의
패키지 선택을 고정해라.

## Plugin 빌드 / 스테이징

번들 plugin(`crates/tasty-plugin-*` 중 `tasty-plugin.toml` 보유)은 부팅 시 `install_builtins_if_needed` 가 `~/.tasty/plugins/<id>/` 로 자동 sync 한다. `bundle_root()` fallback 이 `<exe_dir>/builtin-plugins/`(= `target/<profile>/builtin-plugins/`)라, **그 경로에 스테이징만 해두면** 부팅 시 user dir 까지 흐른다. **debug 빌드만** `ensure_dev_bundle` 이 매 부팅 mtime(동률이면 내용) 비교로 workspace→bundle 을 sync 한다 — 소스가 bundle 보다 옛 시각이면 내용이 달라도 안 옮긴다(`cp -p` 복원 등). 플러그인까지 빌드한 `cargo build --workspace` 후 실행하면 반영된다. **release/dist 는 소스가 옆에 있어도 workspace→bundle 자동 동기화를 하지 않는다.** `just build --release` 또는 해당 프로필의 `just build-plugins` 로 서명과 함께 스테이징한 번들을 사용한다. 빌드 후 소스 매니페스트를 수정해도 다음 실행이 서명된 번들을 덮어쓰지 않는다. `cargo build --release --workspace` 만으로는 번들 스테이징이 되지 않는다.

```bash
just build-plugins                # 모든 bin plugin → release 스테이징
just build-plugin claude          # 단일 plugin
just build-all                    # plugins + 본 바이너리
just link-plugins                 # cp 대신 symlink (rebuild 즉시 반영)
```

산출물: `${CARGO_TARGET_DIR:-target}/<profile>/builtin-plugins/<id>/{tasty-plugin.toml, <bin>, tasty-plugin.toml.sig(non-debug), lang/}`. **세 레시피(`build-plugins`·`build-plugin`·`link-plugins`)는 `CARGO_TARGET_DIR` 를 cargo 와 같은 자리에서 읽는다** — 레포 밖 target 을 쓰는 작업 트리에서도 cargo 가 실제로 쓴 자리에 스테이징한다. 안 읽으면 cargo 는 새 자리에 짓고 레시피는 `target/` 에서 옛 산출물을 찾으므로, 스테이징이 조용히 비거나 낡은 채로 남는다. lib-only crate(protocol/sdk/manifest)는 manifest 부재로 자동 skip. release/dist 프로필은 `build-plugins`·`link-plugins` 가 위 자동 서명 단계를 수행해 `.sig` 까지 스테이징한다(`link-plugins` 는 crate-dir `.sig` 를 symlink 해 재서명이 자동 반영). debug 는 서명 없이 매니페스트/bin/lang 만.

> **위 스테이징은 "부팅 시 user dir 로 흐른다" 전제(=호스트 재시작)다.** **이미 실행 중인 tasty 에** 플러그인 변경만(호스트 재빌드·재시작 없이) 반영·검증하려면 — 실행 프로필 확인 → 그 프로필로 플러그인 빌드 → 재서명 → `plugin disable → upgrade-builtins[ --force] → enable` 절차를 쓴다: [plugin-development §9.1](plugin-development.md#91-실행-중인-tasty-에-번들-플러그인만-반복-갱신-호스트-재빌드재시작-불필요).

## 배포 패키징

```bash
./scripts/build-macos-dmg.sh    # .app + .dmg (자동 --profile dist)
./scripts/build-linux.sh        # tar.gz + .deb + .rpm + .AppImage (uname -m 자동 감지)
./scripts/build-windows.ps1     # zip + .msi (cargo-wix + WiX 3.x)
```

- **macOS** `.app` 은 ad-hoc 코드 서명(`codesign --sign -`). 번들 plugin 은 **`Contents/Resources/plugins/<id>/`** 에 staging 한다 — `Contents/MacOS/` 하위에 두면 codesign 이 그 디렉터리를 nested code 로 간주해 번들로 파싱하려다 `bundle format unrecognized` 로 **서명 자체가 실패**한다. `tests/macos_bundle_codesign.rs` 가 이 레이아웃을 강제하고, 런타임 탐색은 `bundle_root()`(`crates/tasty-host-plugin/src/builtin.rs`)가 담당한다.
- **macOS 번들 `Info.plist` 의 정본은 `scripts/build-macos-dmg.sh` 의 heredoc 하나뿐**이다 (`$VERSION` 이 `Cargo.toml` 에서 치환되고, 스크립트 말미의 `PLIST_VER` 검증이 그 정합을 강제한다). 여기엔 `CFBundle*` 계열 외에 TCC usage description 키가 들어간다 — macOS 는 보호 리소스 접근 프롬프트 **본문에 이 문자열을 그대로 표시**하며, 키가 없으면 이유 없는 프롬프트가 뜨거나 일부 서비스는 접근 시도 자체가 즉시 실패한다.

  | 키 | 언제 뜨는가 |
  |----|-------------|
  | `NSDownloadsFolderUsageDescription` | 셸 명령이 `~/Downloads` 에 접근할 때 |
  | `NSDocumentsFolderUsageDescription` | 셸 명령이 `~/Documents` 에 접근할 때 |
  | `NSDesktopFolderUsageDescription` | 셸 명령이 `~/Desktop` 에 접근할 때 |
  | `NSRemovableVolumesUsageDescription` | 마운트된 이동식 볼륨(USB·SD) 접근 시 |
  | `NSNetworkVolumesUsageDescription` | 마운트된 네트워크 볼륨 접근 시 |

  문구는 "터미널이 사용자가 친 명령을 대신 실행한다" 는 tasty 의 실제 접근 이유를 담는다. `t()` i18n 을 타지 않는다 — plist 문자열은 OS 가 표시하며, 다국어화는 `Contents/Resources/*.lproj/InfoPlist.strings` 라는 별개 메커니즘이 필요하다(현재 미도입, 영어 단일). `NSAppleEventsUsageDescription` 은 넣지 않는다 — tasty 자신은 Apple Events 를 보내지 않고(소스 전체에 `osascript`/`NSAppleScript` 호출 0건), 셸 자식 프로세스가 `osascript` 를 실행할 때 Automation 승인이 tasty 로 귀속되는지는 실기 확인이 필요한 별건이다. 키 누락 회귀는 `tests/macos_bundle_codesign.rs` 의 정적 검사가 막는다(스크립트 heredoc 을 직접 파싱하므로 Linux CI 에서도 돈다).
- **macOS 개발 중 권한 프롬프트가 매번 다시 뜬다면** — ad-hoc 서명은 designated requirement 가 cdhash 뿐이라 재빌드마다 macOS 가 다른 앱으로 보고 TCC 승인을 버린다. `./scripts/macos-codesign-identity.sh --create` 로 self-signed 인증서(`Tasty Dev`)를 한 번 발급해두면, 이후 `./scripts/install-macos.sh` 가 키체인에서 그것을 자동으로 집어 서명하므로 재빌드해도 승인이 유지된다. DR 이 cdhash 대신 `identifier "com.zilhak.tasty" and certificate leaf` 가 되기 때문이다. 자동 선택은 로컬 설치 빌드(`NO_DMG=1`)에서만 동작한다 — DMG 배포본은 그 인증서를 신뢰하지 않는 머신으로 가므로 ad-hoc 을 유지한다. 다른 identity 를 쓰려면 `TASTY_CODESIGN_IDENTITY` 로 지정한다.
- **macOS Full Disk Access 는 서명 identity 와 묶여 있다** — FDA 를 부여해도 **ad-hoc 서명 빌드는 재빌드할 때마다 권한이 초기화된다**(designated requirement 가 cdhash 뿐이라 macOS 가 매번 다른 앱으로 본다). 직접 빌드해 쓰면서 FDA 를 유지하려면 위 `Tasty Dev` 인증서 발급이 **선행 조건**이다. "FDA 를 줬는데 또 초기화됐다" 의 원인이 대부분 이것이라, 앱의 FDA 안내 문구도 이 조건을 함께 알린다([macOS 권한](../features/macos-permissions/index.md)).
- **Linux** `.deb`/`.rpm` 은 `cargo-deb` / `cargo-generate-rpm`, `.AppImage` 는 `linuxdeploy`(ELF 의존 라이브러리를 전부 번들 + rpath `$ORIGIN` → distro 무관 동작). 패키지 메타데이터는 `Cargo.toml` 의 `[package.metadata.deb]` / `[package.metadata.generate-rpm]`.
- **Windows** MSI 는 `cargo-wix` + `wix/main.wxs`. **UpgradeCode GUID 는 절대 변경 금지** — 바뀌면 새 제품으로 인식되어 구버전과 공존.
- CI: `.github/workflows/release.yml` (self-hosted runner, Linux x64/arm64 라벨 분기, Windows). `workflow_dispatch` 로 태그 없는 수동 검증 빌드 가능.

현재 머신에 바로 설치하려면 `just install` — 본체+플러그인을 dist 빌드해 현재 OS 에 설치한다 (자동 감지). macOS 는 `scripts/install-macos.sh` 가 `build-macos-dmg.sh` 를 `NO_DMG=1` 로 재사용해 `dist/Tasty.app` 만 조립한 뒤 `/Applications/Tasty.app` 을 덮어쓴다. 번들 플러그인은 앱 실행 시 `~/.tasty/plugins` 로 동기화된다(설치본 버전이 더 높으면 건너뛴다). (Linux/Windows 자동 설치는 미구현 — `just dist-linux`/`dist-windows` 산출물로 수동 설치.)

### dist 빌드 명령 카탈로그

[release](release.md) 는 *태그 push → GitHub Actions* 워크플로, 이 절은 **로컬에서 dist 산출물을 빌드** 하는 명령 카탈로그. 빌드 프로필 정의(LTO/strip)는 위 [빌드 프로필 (3종)](#빌드-프로필-3종).

#### 빠른 시작 (Justfile)

```bash
just dist            # 호스트 OS dist 빌드 (자동 sanity check + SHA256SUMS)
just dist-clean      # dist/ 정리
just dist-verify     # SHA256SUMS 재검증
just dist-macos | dist-linux | dist-windows   # 플랫폼 명시
just dist-setup-linux                          # Linux 사전 도구 (1회)
```

`just` 미설치: `cargo install just`. (Windows 일반 PowerShell 은 자동 감지가 안 될 수 있어 `just dist-windows` 명시 권장.)

#### 공통 사전 조건

- `Cargo.toml::version` 이 의도한 릴리스 버전인지 확인(검증 빌드엔 버전 안 올림 — [release](release.md) bump 절차).
- 모든 빌드 스크립트는 인자 없으면 `--profile dist` 기본.
- 산출물은 `dist/` 에 누적(동일 버전 재빌드는 silent overwrite).

#### macOS

도구(`hdiutil`/`codesign`/`xcrun`)는 Xcode CLT 포함.

```bash
cargo build --profile dist        # 워크스페이스 컴파일
./scripts/build-macos-dmg.sh      # .app 번들 + .dmg
```

산출물: `dist/Tasty.app/...`(`CFBundleVersion` = Cargo version) · `dist/Tasty-{version}-macos.dmg`. `build-macos-dmg.sh` 마지막에 자동 sanity check(`tasty --version` / Mach-O / `CFBundleVersion` 일치 / DMG 존재) — 실패 시 빌드 fail. 고지 세트는 `Contents/Resources/` 에 codesign **전에** 스테이징되고(`scripts/lib/notice-set.sh`), `.app` · DMG 스테이징 트리 · 만든 DMG 를 읽기 전용으로 붙인 트리 세 곳에서 저장소 사본과 바이트 대조한다. 이 확인은 배선이며 macOS 빌더에서 돈 적은 아직 없다(미측정). `dist` 는 `release` 상속(`strip=true`)이라 `nm` 이 거의 빈 건 정상.

**서명은 ad-hoc, 공증은 범위 밖** — `build-macos-dmg.sh` 가 `codesign --sign -` 로 ad-hoc 서명한다(Apple Silicon 의 "손상됨" 하드 블록 완화). 인증서 서명이 아니라 Gatekeeper 는 여전히 rejected 이므로(`spctl -a` 로 확인) 사용자는 Finder 우클릭→열기로 우회. 번들 plugin 은 `Contents/Resources/plugins/` 에 staging 해야 서명이 통과한다 — `Contents/MacOS/` 하위면 codesign 이 그 디렉터리를 nested bundle 로 파싱하려다 실패한다([build.md](#배포-패키징)). 산출물은 **Apple Silicon(arm64) 전용**이다 — dist 는 full LTO 라 타깃을 하나 더 얹으면 빌드 시간이 배로 늘고, Intel Mac 은 macOS 26 이 마지막 지원 릴리스라 배포 대상에서 뺐다. Intel 에서 쓰려면 `--target x86_64-apple-darwin` 으로 직접 빌드한다.

#### Windows

> Darwin 작성 환경에서 직접 실행 불가 — Windows 머신에서 빌드 후 결과 반영.

```powershell
cargo install cargo-wix; winget install WiXToolset.WiXToolset   # 1회
.\scripts\build-windows.ps1            # dist + ZIP + MSI
.\scripts\build-windows.ps1 -SkipMsi   # ZIP 만
```

산출물: `tasty-{v}-windows-x64.{zip,msi}` + `SHA256SUMS-windows.txt`. `build-windows.ps1` 이 MSI 단계에서 `$env:WIX\bin` 을 자동 PATH prepend. 자동 sanity check(ZIP 풀어 `tasty.exe --version`, MSI 존재). 고지 세트는 ZIP 최상단과 MSI 설치 디렉토리에 들어가고, 스크립트가 ZIP 을 푼 트리와 MSI 를 관리 설치(`msiexec /a`)로 푼 트리에서 저장소 사본과 바이트 대조한다. `wix/main.wxs` 는 파일마다 이름을 적어야 해서, MSI 빌드 전에 `LICENSES/` 의 파일마다 대응 `Source` 가 있는지 먼저 본다. 이 확인들은 배선이며 Windows 빌더에서 돈 적은 아직 없다(미측정). 검증 포인트: MSI UpgradeCode 유지(`wix/main.wxs`), 설치→시작메뉴→제거.

#### Linux

```bash
just dist-setup-linux              # 또는 수동 (아래)
./scripts/build-linux.sh           # uname -m 으로 x64/arm64 자동 감지
```

수동 사전 도구: `sudo apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev` + `cargo install cargo-deb cargo-generate-rpm` + `linuxdeploy`(GitHub continuous, `~/.local/bin`). 도구 역할은 위 [배포 패키징](#배포-패키징) 의 Linux 항목.

산출물: `tar.gz` · `.deb` · `.rpm` · `.AppImage` + `SHA256SUMS-linux-{x64|arm64}.txt`. 자동 sanity check(tar.gz `tasty --version`, `dpkg-deb -I`, `rpm -qpi`, AppImage ELF 확인 — 실행은 안 함, GUI hang 회피).

넷 다 고지 세트(`LICENSE` · `THIRD_PARTY_LICENSES.md` · `LICENSES/` 의 모든 파일)를 함께 나른다 — 생성 단계는 없고 저장소의 파일을 그대로 스테이징한다. `LICENSES/` 는 파일 이름이 아니라 디렉토리로 읽는다(`scripts/lib/notice-set.sh` 의 `notice_set_files`, deb/rpm 은 `Cargo.toml` asset 의 glob). 자리는 산출물마다 다르고(`tar.gz` 는 최상단, deb 은 `/usr/share/doc/tasty/`, rpm 과 AppImage 는 `usr/share/licenses/tasty/`) 정본 표는 [`THIRD_PARTY_LICENSES.md`](../../THIRD_PARTY_LICENSES.md) 에 있다. sanity check 가 tar.gz · deb · rpm 리스팅과 AppImage 의 AppDir 에서 세트의 파일 **전부**를 함께 보는데, **rpm 쪽 확인은 `rpm` 명령이 있는 빌더에서만 돈다** — 없으면 그 갈래는 통과가 아니라 미측정이다.

#### 산출물 요약

| 플랫폼 | 명령 | 산출물 |
|--------|------|--------|
| macOS (arm64) | `./scripts/build-macos-dmg.sh` | `Tasty-{v}-macos.dmg` (~18MB) |
| Windows (x64) | `.\scripts\build-windows.ps1` | `{zip,msi}` |
| Linux | `./scripts/build-linux.sh` | `{tar.gz,deb,rpm,AppImage}` (~83MB AppImage) |

#### 관련

- 위 [빌드 프로필 (3종)](#빌드-프로필-3종) — 프로필 정의 · [release](release.md) — 릴리스 워크플로 · [release 러너](release.md#러너) — self-hosted runner

## 빌드 시간 진단

```bash
cargo build --release --timings   # target/cargo-timings/*.html — 크레이트별 frontend/codegen
cargo machete                     # 미사용 의존성 (컴파일 시간 낭비)
cargo modules / cargo depgraph    # 모듈/크레이트 의존 그래프 (크레이트 분리 검토용)
```

한 크레이트가 전체 빌드의 50%+ 를 잡으면 너무 크다는 신호 — 모듈 분리를 고려한다.

## 크레이트 분리 가이드

본 바이너리의 큰 leaf 모듈을 떼어낼 때 후보 조건: **out-degree 작음**(다른 src/ 모듈 거의 미참조) · **사이클 없음** · **충분히 큼**(1000줄+). 절차: `crates/tasty-<name>/` 생성 → `git mv` → 내부 경로 갱신 → 본 `Cargo.toml` 의존 추가 → `pub use tasty_<name> as <name>` 재수출(backward path 유지) → `cargo check`/`build` 검증. 기존 `crate::model::Foo` 경로가 그대로 동작하는 게 핵심이라 reverse import 갈아끼우기가 불필요하다.

**세 조건 중 크기는 보조 지표다** — 소비자가 둘 이상이고, 합칠 후보 크레이트에 *그 크레이트가 원래 몰라도 되는 의존* 을 들이게 되는 코드는 1000줄에 못 미쳐도 분리한다(판정은 의존 방향이 우선, [ADR-0089](../adr/0089-crate-split-follows-dependency-direction.md)).

**재수출 형태는 둘 중 하나를 고른다.**

| 형태 | 쓰는 곳 | 이유 |
|------|---------|------|
| `pub use tasty_<name>::*;`(glob) | 본체가 그 크레이트를 **통째로 소유**하고 경계를 그을 이유가 없는 shim (`src/model.rs`, `src/adapters/ui/icons.rs`) | 표면 전체가 본체 것이라 좁힐 대상이 없다 |
| `pub use tasty_<name>::{A, B, C};`(명시 목록) | 본체가 **일부만 써야 하는** 크레이트 (`src/adapters/cli.rs`, `src/adapters/ipc.rs`) | glob 이면 계층 위반이 컴파일 에러로 안 잡힌다 — 본체 어디서든 재수출 경로로 크레이트 전체에 닿는다 |

판단 기준은 "본체가 이 크레이트의 아무 심볼이나 써도 되는가" 하나다. 아니라면 명시 목록을 쓰고, 목록 밖 심볼을 쓰려는 시도가 컴파일 에러가 되게 둔다. 소스 스캔 가드(`crates/tasty-doc-guards/tests/layering.rs`)는 재수출을 우회한 직접 참조를 막는 **2차 방어**이지, 재수출 형태를 좁히는 것의 대체재가 아니다.

### 의존 방향 규칙 — 본체는 `tasty-cli` 를 참조하지 않는다

`tasty-cli` 는 **바이너리의 진입 계층**(인자 파싱 + 그 파싱 결과로 실행되는 커맨드 구현)이다. GUI 런타임·IPC 핸들러·앱 상태(`src/`)가 그 크레이트 내부를 직접 들여다보면 의존 방향이 뒤집혀, CLI 쪽 타입 변경이 GUI 를 깨고 GUI 재사용 목적의 로직이 CLI 안에 눌러앉는다. 양쪽이 함께 쓰는 코어(ssh · remote browse · stream 등)는 CLI 가 아니라 **별도 크레이트**에 두고 본체는 그쪽을 참조한다.

`src/adapters/cli.rs` 는 boot 진입점 9개(`Cli` · `Commands` · `Envelope` · `run_client_with` · `try_run_plugin_cli` · `print_augmented_help` · `print_command_tree` · `format_parse_error` · `localized_command`)만 이름으로 재수출한다. 그 밖의 CLI 심볼은 본체에서 경로 자체가 존재하지 않는다. 재수출 밖의 경로(`tasty_cli::` 직접 참조)는 **`crates/tasty-doc-guards/tests/layering.rs` 가 막는다**(그 가드는 의존 0 크레이트에 살아서 `doc-guards.yml` 이 경로 필터 없이 돌린다 — main push 와 PR 마다 자동으로 돈다, [ci-gates](ci-gates.md)). 목록 두 개의 성격이 다르다:

| 상수 | 성격 | 내용 |
|------|------|------|
| `ALLOWED_PATHS` | 영구 허용 | `src/main.rs` · `src/boot.rs` · `src/boot/` · `src/adapters/cli.rs` — 바이너리가 CLI 파서를 소유하는 정당한 의존 |
| `BASELINE_FILES` | 한시 허용 | 이행 중인 위반의 스냅샷. **현재 비어 있다** — 본체는 `tasty-ssh` / `tasty-remote` / `tasty_ipc::client` 를 직접 참조한다. **줄어들기만 한다**(새 항목 추가 금지, 참조를 걷어냈으면 목록에서도 지워야 통과 — 역방향 검사) |

주석 안의 언급도 위반으로 잡는다 — 주석이 옛 경로를 가리키면 그것도 실제 오정보이므로 코드와 함께 갱신한다.

### 의존 방향 규칙 — 도메인은 조립부를 부르지 않는다

도메인(`src/core/` · `src/ports/`)은 **크레이트로 떼지 않았다** — 본체와 같은 크레이트에 있고,
방향은 모듈 경계와 가드로 선다. 떼지 않은 이유(도메인 안의 gui 게이트 수 · 형제 모듈 폐포 ·
`pub(crate)` 표면)와 다시 볼 조건은 [ADR-0440](../adr/0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md),
경계의 내용은 [아키텍처](../architecture/index.md) 의 "도메인 경계" 절이다.

같은 크레이트 안에서는 `crate::app::…` 이 언제나 해석되므로 컴파일러가 이 방향을 못 막는다.
`crates/tasty-doc-guards/tests/domain_does_not_reach_up.rs` 가 막는다(`doc-guards.yml` 이 경로
필터 없이 돌린다). 위 `tasty-cli` 가드와 달리 **면제 명부가 없다** — 기대값이 0 이다. 테스트
(파일 단위 test-only · 인라인 `#[cfg(test)]`)와 주석·문자열은 좌변이 아니다. 같은 파일이 도메인
출하 코드의 `feature = "gui"` 개수를 양방향으로 고정하고, gui feature 의 optional 의존(GUI 크레이트)을
부르는 자리를 gui 게이트 뒤까지 읽어 목록으로 고정한다 — 게이트 수만 세면 이미 있는 게이트 뒤에
`egui::…` 를 더 들여도 안 보이기 때문이다([ADR-0490](../adr/0490-boundary-guards-close-three-holes-found-by-mutation.md)).
자동화 실행부(`src/webhook/` · `src/hook_handler/`)가 inbound adapter 를 부르는 방향은
`automation_runners_do_not_reach_inbound_adapters.rs` 가 같은 판정기로 막는다.

그래서 이 경계를 세운다고 편집 빌드 범위가 줄지는 않는다 — 도메인을 고쳐도 GUI 를 고쳐도 같은
컴파일 단위(`tasty` lib)가 다시 돈다. 그 범위가 필요해지는 날이 ADR-0440 의 재검토 조건이다.
