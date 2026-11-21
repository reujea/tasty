# 빌드 가이드

tasty 의 워크스페이스 구조, 빌드 프로필, 빌드 시간 최적화. 강제 정책(어떤 프로필을 언제 쓰는지)은 [`../../CLAUDE.md`](../../CLAUDE.md) "빌드".

## 워크스페이스 구조

workspace는 본 바이너리(`src/`)와 `crates/*`로 구성한다. 전체 목록과 수는
[아키텍처](../architecture/index.md)에서 관리한다. `architecture_crate_list_complete`와
`readme_badge_parity`는 그 목록, 루트 지침, README의 수를 같은
`crate_dir_names` 함수로 대조한다. 여기에는 역할별 예시만 적는다.

| 레이어 | 예 | 성격 |
|--------|-----|------|
| **type-\*** primitive | `tasty-type-geometry`(길이), `tasty-type-appearance`(색·theme schema) | 최하위 schema/primitive |
| 도메인 leaf (GUI-free) | `tasty-model`, `tasty-i18n`, `tasty-settings`, `tasty-themes`, `tasty-terminal`, `tasty-memory`, `tasty-hooks`, `tasty-ipc`, `tasty-ssh`, `tasty-remote`, `tasty-portscan` 등 | 공용 도메인·IO |
| plugin 인프라 | `tasty-plugin-protocol`, `tasty-plugin-sdk`, `tasty-plugin-manifest`, `tasty-host-plugin` | 호스트↔plugin 와이어·SDK·매니페스트 |
| 번들 plugin | `tasty-plugin-mesh-demo` | 공식 플러그인 8개는 [공식 플러그인 레포](https://github.com/zilhak/tasty-official-plugins)에서 빌드하고 잠금 파일로 받는다. → [`../plugins/`](../plugins/index.md) |
| CLI / 테스트 | `tasty-cli`, `tasty-tui-simulator` | |

본 바이너리는 각 크레이트를 재수출해 `crate::model::X` / `crate::theme::theme()` 같은 경로가 그대로 동작한다.

**루트 패키지는 `lib` 와 `bin` 두 타깃이다.** `src/lib.rs` 가 모듈 트리와 위 재수출을 들고, `src/main.rs` 는 `tasty::boot::run()` 만 부른다.

- **루트 패키지의 단위시험은 전부 lib 타깃에 산다.** `cargo test -p tasty --bin tasty` 로 좁히면 **실행된 시험이 0개인데도 성공으로 끝난다** — 0 건은 통과가 아니라 미측정이다. 좁히려면 `cargo test -p tasty --lib` 다. (`--lib --bins` 를 함께 주는 조합은 예전과 같은 것을 담는다.)
- 모듈은 비공개 또는 `pub(crate)`로 유지한다. `cargo doc -p tasty --no-deps`로
  공개 항목을 확인하되, 크레이트 전체 재수출(`font`·`settings`·`theme`)은 의존 문서를
  생성하지 않으면 목록에 나타나지 않을 수 있다. 재수출된 API도 공개 범위에 포함한다.
- 비공개 항목의 코드펜스는 rustdoc이 수집하지 않는다. `cargo test -p tasty --doc`가
  성공해도 실행된 예제가 있는지 확인한다. `doctest = false`로 전체를 끄지는 않는다.

### Windows 파일 identity 경계

`tasty-ipc`는 `forbid(unsafe_code)`를 유지한다. 포트 공개 세대의 Windows identity는
`tasty-utils::windows_file_identity::FileIdentity::of(&File)`가 안전한 API로 조회한다.
이 leaf 유틸리티는 기존 파일 보안 메타데이터 래퍼와 같이 OS 호출 경계를 소유한다.
`GetFileInformationByHandleEx(FileIdInfo)`의 출력 버퍼는 호출 동안 살아 있고 크기·정렬이
`FILE_ID_INFO`와 일치한다. 성공한 호출만 초기화된 결과로 읽으며 실패와 0 파일 ID는
오류로 돌려준다. 비교에는 volume serial과 128비트 파일 ID 전체를 사용한다.

IPC는 그 identity와 원래 열린 `File`을 함께 보관하여 파일 ID 재사용을 막는다. 기본
Windows delete-share를 유지하므로 핀을 가진 채 교체·삭제할 수 있다. identity를 확인하지
못하면 cleanup이 파일을 남긴다([포트 발견 계약](../design/systems/storage.md#ipc-포트-발견의-완료-경계)).
Windows 유틸리티 시험은 전체 ID 비교·0 ID 거절·native 오류 전달·열린 핸들 유지 중
교체와 삭제를 검사하며, IPC의 `port_file` 시험은 같은 포트 재공개 후 이전 세대가 새 파일을
삭제하지 않는지 검사한다. Unix identity 조회는 IPC의 안전한 표준 메타데이터 API를 쓴다.

### type-\* layer 의존 규약 (필수)

`tasty-type-*` 는 **primitive/schema 레이어**다.

- 그룹 내부 의존은 자유 (예: `tasty-type-appearance → tasty-type-geometry`).
- **도메인/IO crate 의존 금지** — `tasty-model`/`tasty-themes`/본 바이너리 등을 의존하지 않는다.
- **그룹 내 순환 금지.**

새 type-\* crate 도 이 3원칙을 따른다 — 의존 그래프가 한 방향으로 유지되어 순환을 방지한다.

## 빌드 프로필 (3종)

플랫폼 공통 설정 필드의 기본값은 모든 OS에서 초기화한다. macOS GUI에서만 쓰는 FDA 판정 헬퍼는 해당 빌드와 단위 테스트에서만 컴파일하며, 다른 OS의 일반 빌드에는 포함하지 않는다.

| 프로필 | 정의 | LTO | 용도 |
|--------|------|-----|------|
| `dev` (기본) | `opt-level = 0` | off | 일상 개발 `cargo build` |
| `release` | `opt-level = 3`, `strip = "debuginfo"` | **thin** | 최적화 검증 `cargo build --release` |
| `dist` | `inherits = "release"` | **full** (`lto = true`) | 배포 산출물 `cargo build --profile dist` |

- **`dev` 는 본체와 등재되지 않은 워크스페이스 크레이트가 opt 0 이다**: 외부 의존성은 `[profile.dev.package."*"]` 로 opt 3 이고, 워크스페이스 크레이트는 glob 에 안 걸려 루트 `Cargo.toml` 에 `[profile.dev.package.<이름>]` 으로 **하나씩 등재**한다. `tasty-doc-guards`는 opt 2, 나머지 등재된 크레이트는 opt 3 이다. 새 크레이트를 만들면 등재 여부를 정한다 — dev 최적화 선택과 재검토 기준은 [아키텍처 개요](../architecture/index.md).
- **문서 가드는 개발·테스트 프로필에서 opt 2로 실행한다**: `test`가 `dev` 설정을 상속하므로 `cargo test -p tasty-doc-guards`에도 적용된다. 저장소를 반복 분석하는 실행 비용을 줄이며, debug assertions와 overflow checks는 개발 프로필의 기본값을 유지한다. 검사 대상과 실행 항목은 줄이지 않는다.
- **`release` = thin LTO**: 크레이트 IR 요약을 공유해 cross-crate inlining 을 **병렬** 적용. 일상 "릴리즈 검증" 은 모두 이걸 쓴다.
- **`dist` = full LTO**: 모든 IR 을 단일 LLVM 모듈로 합쳐 재최적화. 최적화 비용이 커 빌드가 오래 걸림. **배포 바이너리(DMG/MSI/AppImage) 빌드 시에만** 쓴다. (AI 자체 검증 빌드에는 절대 사용 금지.)

```bash
cargo build                 # debug
cargo check                 # 타입검사만 (가장 빠름), -p <crate> 로 단일 크레이트
cargo build --release       # thin LTO 검증
cargo build --profile dist  # 배포용
```

### headless(`--no-default-features`) 빌드

`gui` 는 default feature 라 `cargo build` 는 winit/wgpu/egui 를 켠다. gui 없는 headless/CLI 인스턴스(로컬 GUI 사용자 없이 IPC/CLI + 원격 attach 만 쓰는 인스턴스, [헤드리스 정의 경계](headless-build-boundaries.md))는 `--no-default-features` 로 빌드한다.

```bash
cargo check --workspace --no-default-features   # headless 컴파일 검증
cargo build --workspace --no-default-features   # headless 빌드
```

릴리스 빌드 스크립트(`scripts/build-linux.sh` · `scripts/build-macos-dmg.sh` · `scripts/build-windows.ps1`)는 GUI 산출물을 만든 뒤
`CARGO_TARGET_DIR=target/headless` 로 같은 프로필의 루트 패키지를 `--no-default-features` 로 한 번 더 빌드해 헤드리스 압축 파일을 만든다.
GUI 패키지가 읽는 `target/<프로필>/tasty` 를 덮지 않으려고 target 디렉터리를 나눈다. 산출물 이름과 검증은 [릴리스 절차](release.md#헤드리스-산출물).

Windows 에서 GUI 서브시스템(`windows_subsystem = "windows"`)은 release 이면서 `gui` feature 인 조합에서만 링크한다(`src/main.rs`).
그래서 헤드리스판과 debug 빌드는 콘솔 서브시스템이며, 콘솔에서 실행하면 셸이 종료를 기다리고 출력과 종료 코드를 받는다.
부모 콘솔에 붙는 `src/boot/os.rs` 의 `attach_windows_console_if_needed` 도 같은 조합에서만 동작한다. 콘솔 서브시스템을 고른 이유는 [릴리스 절차](release.md).

#### Linux gui 빌드의 시스템 라이브러리

Linux 에서 기본 feature(`gui`)는 WebView 오버레이를 위해 webkit2gtk·gtk 를 링크한다(루트 `Cargo.toml` 의 Linux 대상 의존성).
그래서 gui 조합을 빌드하려면 GTK3·WebKitGTK 개발 패키지가 필요하다. 이 의존성은 optional 이라
`--no-default-features` 조합은 이 라이브러리를 빌드하지 않는다.

```bash
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev   # Debian·Ubuntu 계열
```

<a id="컴파일된다-와-그래프에-안-들어온다-는-다른-좌변이다"></a>

#### 컴파일 성공과 GUI 의존성 제외는 따로 확인한다

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
그 둘을 구분한다.

**전체 의존 그래프를 확인하는 자동 시험은 없다.** 위 두 줄이 그것을 재는 유일한
방법이다. 있는 것은 **크레이트 하나의 선언**을 보는 시험 하나뿐이다
(`crates/tasty-font/src/lib.rs` 의 `wgpu_stays_an_optional_dependency_of_this_crate` —
`wgpu` 가 비-optional 로 돌아가거나 `gpu` feature 가 사라지거나 `default` 가 안 비면 죽는다,
변이로 확인). **그것이 그래프를 보는 것은 아니다** — 다른 크레이트가 wgpu 를 새로 들이면
그 시험이 통과해도 GUI 의존성이 다시 포함된다.

과거 측정(2026-09-20, 위 ①처럼 이름을 중복 제거): 헤드리스 그래프는 **306** 노드다.
없는 것: `egui` 계열(`egui`·`ecolor`·`emath`·`epaint`·`egui_extras`) · `winit` ·
**`wgpu` 계열(`wgpu`·`wgpu-core`·`wgpu-hal`·`wgpu-types`·`naga`·`glow`·`ash`)**.
`wgpu` 가 빠진 것은 `tasty-font` 이 그것을 `gpu` feature 뒤 optional 로 들고 루트의 `gui` 만
켜기 때문이다([아키텍처 개요](../architecture/index.md)).

**아직 있는 것: `cosmic-text`·`fontdb`·`swash`·`skrifa`·`read-fonts`·`ttf-parser`.**
이것은 미완이 아니라 결정이다 — `FontConfig` 의 필드 셋이 cosmic-text 타입이고 헤드리스
코드(`src/boot/locale_font.rs` 의 `family_path`)가 그 타입을 실제로 쓴다. 빼려면 폰트 DB 를
여는 수단 자체를 바꿔야 하고 그것은 **동작 변경**이다.

gui 전용 심볼(`MainViewState.toasts` 등)을 `#[cfg(feature = "gui")]` 게이팅 없이 쓰면 gui 빌드는 통과하지만 headless 빌드만 깨진다. 이 회귀는 `.github/workflows/crossplatform-check.yml` 의 `check-headless` 잡(`cargo check --workspace --no-default-features --locked`)이 `develop`·`release`·`main` push 마다 자동 검출한다(문서만 바뀐 push 는 제외).

#### 공용 모듈 안의 GUI 전용 정의

경계를 가르는 규칙의 정본은 [headless-build-boundaries](headless-build-boundaries.md) 다 —
세 갈래(`cfg(feature = "gui")` · `cfg(any(feature = "gui", test))` · `expect(dead_code,
reason)`)와 그것을 재는 여덟 칸이 거기 있다. 아래 표는 이 빌드 문서에서 자주 걸리는 구체
사례다.

| 정의 | 빌드 경계 | 공용으로 남는 부분 |
|---|---|---|
| `db::init`, `default_db_path`, `db::publish` | GUI 부팅만 호출 | `db::shared_recent_files`는 headless에서도 부르며 늘 `None`이라 최근 목록은 공유하지 않는 빈 캐시에서 시작한다 |
| `Db`·`Db::open`·스키마·초기화 오류 분류·state writer(`db::writer`) | GUI 또는 단위 테스트 | headless는 state.db를 열지 않는다. 최근 목록의 저장 접수·revision 코드는 같은 경계로 headless에서 빠진다 |
| `crash_report::write_hang_report` | winit watchdog을 가진 GUI | panic 리포터 설치와 host 파일 로그는 GUI/headless/CLI 역할별 공용 부팅에서 사용 |
| `crash_report::error_loop`, `record_error` | GUI; detector는 debug, release는 no-op | headless panic/파일 로깅 경로를 함께 제거하지 않음 |

`state.db` 초기화와 `memory.db` 초기화는 다른 경로다. 이 빌드 경계가 headless의
메모리 저장소나 최근 파일 IPC를 끄는 것은 아니다. 오류 관측의 범위는
[crash-diagnostics](crash-diagnostics.md), 저장소 접근은
[storage](../design/systems/storage.md)를 따른다.

본체+플러그인을 한 번에 다루는 래퍼가 있다. `just build` 는 빌드·스테이징만(실행 X), `just run` 은 빌드 후 호스트까지 실행한다. 둘 다 먼저 `just build-plugins` 로 개발 번들을 준비한다(아래 "Plugin 빌드 / 스테이징"). 호스트는 부팅 시 builtin 을 동기화하므로(버전으로 갈래를 고르고 같은 버전이면 내용이 다른 파일만 옮긴다 — 설치본이 더 높으면 건너뛴다) 새로 스테이징한 플러그인이 (`just build` 면 다음 실행 시, `just run` 이면 그 실행에서) 반영된다.

```bash
just build                  # 본체 debug 빌드 + 개발 번들 준비 (실행 X)
just build --release        # 동일, release 프로필
just run                    # 본체 debug 빌드 + 개발 번들 준비 + 실행
just run --release          # 동일, release 프로필 (나머지 인자는 호스트로 passthrough)
```

**release/dist 로컬 빌드는 번들을 자동 서명한다.** `--release`(및 `--profile dist`)는
trust 게이트가 켜지므로(debug 는 `#[cfg(debug_assertions)]` 로 우회), `build-plugins` 가
본체 빌드보다 먼저 서명 키를 보장(`scripts/ensure-sign-key.sh`: `SIGN_KEY_PATH` env →
`release.pem` → `dev.pem`+`gen-dev-key.sh`)하고 임베드 `dev-pubkey.bin` 을 재도출한다.
받거나 빌드한 공식 플러그인은 서명 없이 오므로 `sign-bundle.sh --dir` 로 그 매니페스트들을,
mesh-demo 는 스테이징한 매니페스트를 `--manifest` 로 그 키로 서명한다. 순서는 키 준비 → 받기(또는 빌드) → 서명이다.
`just build`/`just run`(debug)은 게이트가 꺼져 있어 서명 단계를 건너뛴다(기본
dev 워크플로에 openssl 의존 미부과). dist 스크립트(`build-*.{sh,ps1}`)는 플러그인을 빌드하지 않고
잠금 파일의 공식 플러그인 산출물을 받아 같은 키로 서명한다(debug 는 서명 생략) — 상세는
[plugin-packaging](plugin-packaging.md#배포-패키지의-번들-플러그인).

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
사실을 경고하지 않는다.** 배포 산출물은 이 갈래를 타지 않는다 — Windows 산출물은 네이티브 Windows
러너가 만든다(`.github/workflows/release.yml` 의 `runs-on: [self-hosted, Windows]` 잡과, 같은 Windows 호스트의 arm64
전용 러너를 쓰는 `runs-on: [win-arm64]` 잡). 교차 빌드한
exe 는 컴파일·시험 확인용으로만 쓰고 배포물로 쓰지 않는다.

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

**`cargo:warning` 출력만으로 재실행을 판단하지 않는다** — cargo 는 스크립트가 fresh 여서 안 돌았을
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

워크스페이스의 빌드 스크립트는 루트까지 합쳐 넷이다. 각자 선언하는 것:

| 스크립트 | 선언 | 한 번 돌 때 나가는 줄 수 |
|---|---|---|
| `build.rs` | 아이콘 · `Cargo.toml` | 2 |
| `crates/tasty-host-plugin/build.rs` | `keys` 디렉토리 · `PROFILE` env | 2 |
| `crates/tasty-i18n/build.rs` | `lang/*.toml` 을 하나씩 | 디스크의 `lang/*.toml` 수만큼 |
| `crates/tasty-doc-guards/build.rs` | `src` 디렉토리 통째 | 1 |

`crates/tasty-cli/src/dynamic/build.rs` 는 **빌드 스크립트가 아니다** — 그 크레이트의 평범한
`mod build` 이고, 어느 `Cargo.toml` 에도 `build = ` 지정이 없다(빌드 스크립트는 패키지 루트의
`build.rs` 만 자동 인식된다).

`tasty-i18n` 은 루프에서 파일마다 한 줄씩 내보내므로 **소스의 `println!` 수(1)와 실제로 나가는
줄 수가 다르다.** 두 값을 섞어 세지 마라.

## Plugin 빌드 / 스테이징

공식 플러그인 8개(`bundle = false` 인 agent-stream 포함)는 공식 플러그인 레포에서 얻고, mesh-demo 만 이 workspace 에서 빌드한다. `just build-plugins`(`scripts/stage-dev-plugins.sh`)가 둘을 `${CARGO_TARGET_DIR:-target}/<profile>/builtin-plugins/<id>/` 에 스테이징한다. 번들 plugin 은 부팅 시 `install_builtins_if_needed` 가 `~/.tasty/plugins/<id>/` 로 자동 sync 한다. `bundle_root()` fallback 이 `<exe_dir>/builtin-plugins/`(= `target/<profile>/builtin-plugins/`)라, **그 경로에 스테이징만 해두면** 부팅 시 user dir 까지 흐른다.

```bash
PROFILE=debug just build-plugins                       # 잠금 파일의 릴리스 산출물 받기 (기본)
PROFILE=debug just build-plugins --from-checkout       # ../tasty-official-plugins 에서 빌드
PROFILE=debug just build-plugins --from-checkout ../tasty-official-plugins --with-local-tasty
just build-all                                          # 번들 준비 + 본체 (기본 release)
```

- **기본(받기)**: `official-plugins.lock.toml` 이 가리키는 릴리스에서 현재 플랫폼 산출물을 받아 SHA-256 을 대조하고 푼다(`scripts/fetch-official-plugins.sh --all`, 형식과 캐시는 [plugin-packaging](plugin-packaging.md)). 패키징과 같은 바이너리이고, 플러그인 빌드가 없으며, 플러그인 레포 체크아웃이 없는 CI·원격 머신에서도 같은 명령이다. 플랫폼은 호스트에서 정하고 `--platform` 으로 바꿀 수 있다.
- **옆 체크아웃(`--from-checkout [DIR]`)**: 공식 플러그인 레포 체크아웃(기본 `../tasty-official-plugins`)의 `tasty-official-plugins/scripts/dev-bundle.sh --profile <profile> --out <임시 폴더>` 로 빌드한다. 플러그인 레포는 자기 `target` 에 빌드한다(이 workspace 의 `CARGO_TARGET_DIR` 를 넘기지 않는다). `--profile` 은 debug·release·dist 만 받는다.
- **`--with-local-tasty`**: `--from-checkout` 빌드에 `--tasty <이 체크아웃>` 을 넘겨, 플러그인이 쓰는 tasty 패키지(SDK·protocol 등)를 이 체크아웃으로 바꿔 빌드한다. 호스트 계약과 플러그인을 함께 바꾸는 작업은 이 경로를 쓴다. 받은 산출물과 `--with-local-tasty` 없는 빌드는 둘 다 플러그인 레포가 기준으로 삼은 tasty 커밋의 SDK·protocol 로 만들어진다. 그 뒤 이 체크아웃이 계약을 바꿨다면 새 계약에 기대는 시험은 받은 번들로 통과하지 않는다.
- **교체 단위**: 공식 플러그인은 `<id>/` 폴더를 통째로 바꾸고, mesh-demo 도 같은 방식으로 다시 놓는다. 그 밖의 폴더는 건드리지 않는다. 교체는 기존 폴더를 옆 이름(`.old-<id>.<pid>`)으로 옮긴 뒤 새 폴더를 놓는다. 새 폴더를 놓지 못하거나 그 사이에 중단되면 옆 이름의 폴더를 되돌리고, 되돌리지 못하면 두 경로를 알리고 실패한다. 실행 중인 Tasty 는 설치 폴더(`~/.tasty/plugins/<id>/`)의 실행 파일을 쓰므로 번들 폴더를 잠그지 않는다. 다른 프로세스(탐색기·백신 등)가 번들 폴더의 파일을 열고 있으면 옮기기가 실패하고 그 폴더는 그대로 남는다. 그 프로그램을 닫고 다시 실행한다. 실행 중인 Tasty 에 반영하는 순서는 [§9.1](plugin-development.md#91-실행-중인-tasty-에-번들-플러그인만-반복-갱신-호스트-재빌드재시작-불필요)에 있다.
- **임시 폴더와 동시 실행**: 받거나 빌드한 결과는 실행마다 새로 만든 `<profile>/.official-plugins-stage.*`·`.mesh-demo-stage.*` 에 모은 뒤 옮기고, 실패나 중단(INT·TERM)으로 끝나도 지운다. 같은 target·프로필에 동시에 실행하면 `<profile>/builtin-plugins.lock` 폴더를 먼저 만든 실행만 진행하고 나머지는 안내와 함께 실패한다. 강제 종료(`kill -9`)로 잠금 폴더가 남았다면 진행 중인 실행이 없는 것을 확인하고 지운다.
- **debug 부팅 동기화**: debug 빌드만 `ensure_dev_bundle` 이 매 부팅 mtime(동률이면 내용) 비교로 workspace 의 mesh-demo(`target/<profile>/tasty-plugin-mesh-demo`, `crates/tasty-plugin-mesh-demo/` 의 매니페스트·lang)를 bundle 로 sync 한다. 공식 플러그인은 sync 하지 않는다 — workspace 에 같은 크레이트가 남아 있어도 스테이징한 산출물을 그대로 쓴다(`BUILTINS` 의 `source`). release/dist 는 mesh-demo 도 자동 동기화하지 않는다.
- **최적화 수준**: 이 workspace 는 `[profile.dev.package."*"]` 와 플러그인 크레이트별로 `opt-level = 3` 을 둔다(`Cargo.toml`). 플러그인 레포의 dev 프로필 설정은 그 레포가 정하므로 `--from-checkout` debug 번들은 여기서 빌드하던 debug 플러그인과 속도가 다를 수 있다. 받은 산출물은 릴리스 빌드라 해당 없다.

산출물: `${CARGO_TARGET_DIR:-target}/<profile>/builtin-plugins/<id>/{tasty-plugin.toml, <bin>, tasty-plugin.toml.sig(non-debug), lang/}`(받은 공식 플러그인은 고지 파일도 담을 수 있다). 스크립트는 `CARGO_TARGET_DIR` 를 cargo 와 같은 자리(호출 위치 기준)에서 읽는다 — 레포 밖 target 을 쓰는 작업 트리에서도 cargo 가 실제로 쓴 자리에 스테이징한다. `--target <triple>` 을 주면 `target/<triple>/<profile>/` 아래에 놓는다.

> **위 스테이징은 "부팅 시 user dir 로 흐른다" 전제(=호스트 재시작)다.** **이미 실행 중인 tasty 에** 플러그인 변경만(호스트 재빌드·재시작 없이) 반영·검증하려면 — 실행 프로필 확인 → `plugin disable` → 그 프로필로 번들 준비(서명 포함) → `upgrade-builtins[ --force]` → `enable` 절차를 쓴다: [plugin-development §9.1](plugin-development.md#91-실행-중인-tasty-에-번들-플러그인만-반복-갱신-호스트-재빌드재시작-불필요).

## 배포 패키징

```bash
./scripts/build-macos-dmg.sh    # .app + .dmg + 헤드리스 tar.gz (자동 --profile dist)
./scripts/build-linux.sh        # tar.gz + .deb + .rpm + .AppImage + 헤드리스 tar.gz (uname -m 자동 감지)
./scripts/build-windows.ps1     # zip + .msi + 헤드리스 zip (cargo-wix + WiX 3.x, -Arch x64|arm64)
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

  문구는 "터미널이 사용자가 친 명령을 대신 실행한다" 는 tasty 의 실제 접근 이유를 담는다. `t()` i18n 을 타지 않는다 — plist 문자열은 OS 가 표시하며, 다국어화는 `Contents/Resources/*.lproj/InfoPlist.strings` 라는 별개 메커니즘이 필요하다(현재 미도입, 영어 단일). `NSAppleEventsUsageDescription` 은 넣지 않는다 — tasty 자신은 Apple Events 를 보내지 않고(소스 전체에 `osascript`/`NSAppleScript` 호출 0건), 셸 자식 프로세스가 `osascript` 를 실행할 때 Automation 승인이 tasty 로 귀속되는지는 실 기기 확인이 필요한 별건이다. 키 누락 회귀는 `tests/macos_bundle_codesign.rs` 의 정적 검사가 막는다(스크립트 heredoc 을 직접 파싱하므로 Linux CI 에서도 돈다).
- **macOS 개발 중 권한 프롬프트가 매번 다시 뜬다면** — ad-hoc 서명은 designated requirement 가 cdhash 뿐이라 재빌드마다 macOS 가 다른 앱으로 보고 TCC 승인을 버린다. `./scripts/macos-codesign-identity.sh --create` 로 self-signed 인증서(`Tasty Dev`)를 한 번 발급해두면, 이후 `./scripts/install-macos.sh` 가 키체인에서 그것을 자동으로 집어 서명하므로 재빌드해도 승인이 유지된다. DR 이 cdhash 대신 `identifier "com.zilhak.tasty" and certificate leaf` 가 되기 때문이다. 자동 선택은 로컬 설치 빌드(`NO_DMG=1`)에서만 동작한다 — DMG 배포본은 그 인증서를 신뢰하지 않는 머신으로 가므로 ad-hoc 을 유지한다. 다른 identity 를 쓰려면 `TASTY_CODESIGN_IDENTITY` 로 지정한다.
- **macOS Full Disk Access는 서명 identity와 묶여 있다.** 권한을 부여해도 ad-hoc 서명 빌드는 재빌드할 때마다 권한이 초기화된다. designated requirement가 cdhash뿐이라 macOS가 매번 다른 앱으로 보기 때문이다. 직접 빌드해 쓰면서 이 권한을 유지하려면 위의 `Tasty Dev` 인증서 발급이 먼저다. 권한을 줬는데 다시 초기화됐다는 문제는 대부분 이것이 원인이라, 앱의 권한 안내 문구도 이 조건을 함께 알린다([macOS 권한](../features/macos-permissions/index.md)).
- **Linux** `.deb`/`.rpm` 은 `cargo-deb` / `cargo-generate-rpm`, `.AppImage` 는 `linuxdeploy`(ELF 의존 라이브러리를 전부 번들 + rpath `$ORIGIN` → distro 무관 동작). 패키지 메타데이터는 `Cargo.toml` 의 `[package.metadata.deb]` / `[package.metadata.generate-rpm]`. rpm 의 자동 Requires 는 `scripts/build-linux.sh` 가 `cargo generate-rpm --auto-req find-requires` 로 정한다. 빌드 머신의 `/usr/lib/rpm/find-requires`(rpm 의 rpmdeps)가 payload ELF 의 직접 의존만 soname capability(`libgtk-3.so.0()(64bit)` 같은 형식)와 심볼 버전 요구로 낸다. soname capability 는 배포판마다 패키지 이름이 달라도 맞는다. 인자를 빼면 cargo-generate-rpm 은 find-requires 가 없는 머신에서 ldd 로 전이 의존까지 넣는 방식으로 바뀌어, 빌드 머신 배포판의 라이브러리 버전(ICU 등)에 묶인다. 이 방식에는 대체 경로가 없으므로 find-requires 가 없는 머신에서는 빌드가 멈춘다.
- **Windows** MSI 는 `cargo-wix` + `wix/main.wxs`. **UpgradeCode GUID 는 절대 변경 금지** — 바뀌면 새 제품으로 인식되어 구버전과 공존.
- CI: `.github/workflows/release.yml` (self-hosted runner, Linux x64/arm64 라벨 분기, Windows). `workflow_dispatch` 로 태그 없는 수동 검증 빌드 가능.

현재 머신에 바로 설치하려면 `just install` — 본체를 dist 빌드하고 잠금 파일의 공식 플러그인을 받아 현재 OS 에 설치한다 (자동 감지). macOS 는 `scripts/install-macos.sh` 가 `build-macos-dmg.sh` 를 `NO_DMG=1` 로 재사용해 `dist/Tasty.app` 만 조립한 뒤 `/Applications/Tasty.app` 을 덮어쓴다. 번들 플러그인은 앱 실행 시 `~/.tasty/plugins` 로 동기화된다(설치본 버전이 더 높으면 건너뛴다). (Linux/Windows 자동 설치는 미구현 — `just dist-linux`/`dist-windows` 산출물로 수동 설치.)

`just install` 은 개발용 명령이 아니라 설치용이므로 빌드 대상 크레이트에서 cap 이 적용되는 rustc lint 를 무시한다. `install-macos.sh` 가 `--cap-lints=allow` 를 `RUSTFLAGS` 끝에 붙여(호출자가 준 값은 앞에 보존) workspace 의 deny lint 가 대상 크레이트 빌드를 멈추지 않는다. cargo 는 `CARGO_ENCODED_RUSTFLAGS` 가 설정돼 있으면(빈 값이어도) `RUSTFLAGS` 를 무시하므로, 그때는 `CARGO_ENCODED_RUSTFLAGS` 에 붙인다. 기존 값이 있으면 그 끝에 구분자 `\x1f` 와 함께 붙이고, 빈 값이면 구분자 없이 `--cap-lints=allow` 만 넘긴다. 어느 쪽이든 config 파일(저장소 `.cargo/config.toml` 과 사용자 `~/.cargo/config.toml`)의 `target.<triple>.rustflags` 와 `build.rustflags` 를 가린다. 저장소 config 에는 macOS target 용 값이 없다. 범위 밖인 것도 있다. `build-macos-dmg.sh` 는 `--target` 을 주고 빌드하므로 host 산출물인 build script(`build.rs`)에는 cap 이 적용되지 않는다. workspace lint 를 상속하는 `build.rs` 에 deny lint 위반이 있으면 `just install` 도 멈추고, 그 경고도 출력된다. 호출자 플래그에 `--force-warn` 으로 지정한 lint 는 cap 을 받지 않아 그대로 경고한다. cargo 자체 경고(future-incompat 보고 등)도 그대로 출력된다. `just dist`·`build-macos-dmg.sh` 직접 호출·CI 는 lint 를 그대로 적용한다.

### dist 빌드 명령 카탈로그

[release](release.md) 는 *태그 push → GitHub Actions* 워크플로, 이 절은 **로컬에서 dist 산출물을 빌드** 하는 명령 카탈로그. 빌드 프로필 정의(LTO/strip)는 위 [빌드 프로필 (3종)](#빌드-프로필-3종).

#### 빠른 시작 (Justfile)

```bash
just dist            # 호스트 OS dist 빌드 (자동 sanity check + SHA256SUMS)
just dist-clean      # dist/ 정리
just dist-verify     # SHA256SUMS 재검증
just dist-macos | dist-linux | dist-windows | dist-windows-arm64   # 플랫폼 명시
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

산출물: `dist/Tasty.app/...`(`CFBundleVersion` = Cargo version) · `dist/Tasty-{version}-macos-arm64.dmg`. `build-macos-dmg.sh` 마지막에 자동 sanity check(`tasty --version` / Mach-O / `CFBundleVersion` 일치 / DMG 존재) — 실패 시 빌드 fail. 고지 세트는 `Contents/Resources/` 에 codesign **전에** 스테이징되고(`scripts/lib/notice-set.sh`), `.app` · DMG 스테이징 트리 · 만든 DMG 를 읽기 전용으로 붙인 트리 세 곳에서 저장소 사본과 바이트 대조한다. 이 확인 코드는 구현돼 있지만 macOS 빌더에서 돈 적은 아직 없다(미측정). DMG 다음에 헤드리스 `tasty-headless-{version}-macos-arm64.tar.gz` 를 만든다(`NO_DMG=1` 이면 생략, [헤드리스 산출물](release.md#헤드리스-산출물)). 이 단계도 macOS 빌더에서 돈 적이 없다(미측정). `dist` 는 `release` 를 상속해 `strip = "debuginfo"` 이므로 디버그 정보는 없고 함수 이름 표는 남는다. `nm` 에 tasty 함수 심볼이 보여야 한다.

**서명은 ad-hoc, 공증은 범위 밖** — `build-macos-dmg.sh` 가 `codesign --sign -` 로 ad-hoc 서명한다(Apple Silicon 의 "손상됨" 하드 블록 완화). 인증서 서명이 아니라 Gatekeeper 는 여전히 rejected 이므로(`spctl -a` 로 확인) 사용자는 Finder 우클릭→열기로 우회. 번들 plugin 은 `Contents/Resources/plugins/` 에 staging 해야 서명이 통과한다 — `Contents/MacOS/` 하위면 codesign 이 그 디렉터리를 nested bundle 로 파싱하려다 실패한다([build.md](#배포-패키징)). 산출물은 **Apple Silicon(arm64) 전용**이다 — dist 는 full LTO 라 타깃을 하나 더 얹으면 빌드 시간이 배로 늘고, Intel Mac 은 macOS 26 이 마지막 지원 릴리스라 배포 대상에서 뺐다. Intel 에서 쓰려면 `--target x86_64-apple-darwin` 으로 직접 빌드한다.

#### Windows

> Windows 산출물은 Windows 머신에서 빌드하고 직접 확인한다.

Windows ARM 은 지원 대상([플랫폼 원칙](../principles/platform-and-compatibility.md))이다. `-Arch x64`(기본)와 `-Arch arm64` 로 아키텍처를 고르며, x64 호스트에서는 ARM64 를 교차 빌드한다. 사전 도구·검사 방법·미측정 범위는 [release 의 Windows ARM64](release.md#windows-arm64). 0.12.2 이하 릴리스에는 ARM64 산출물이 없고, x64 산출물을 Windows ARM 기기에서 실행하는 것은 확인하지 않았다.

```powershell
cargo install cargo-wix; winget install WiXToolset.WiXToolset   # 1회
.\scripts\build-windows.ps1              # x64: dist + ZIP + MSI
.\scripts\build-windows.ps1 -Arch arm64  # ARM64 (x64 호스트에서는 교차 빌드)
.\scripts\build-windows.ps1 -SkipMsi     # ZIP 만
```

산출물: `tasty-{v}-windows-{x64|arm64}.{zip,msi}` · 헤드리스 `tasty-headless-{v}-windows-{x64|arm64}.zip` + `SHA256SUMS-windows.txt`(x64) / `SHA256SUMS-windows-arm64.txt`(ARM64). Windows x64 개발 환경과 태그 없는 CI에서 두 아키텍처의 생성·패키지 추출·PE·고지·체크섬 검사를 확인했다.

패키지 안의 본체·번들 plugin `.exe`의 PE 머신 타입을 검사한다. 호스트와 같은 아키텍처는 `tasty.exe --version`을 실행하고 MSI를 `msiexec /a`로 풀며, 다른 아키텍처는 실행 검사 대신 PE 검사와 `dark.exe` 추출을 사용한다. GUI ZIP·MSI·헤드리스 ZIP의 고지 세트는 저장소 사본과 바이트 대조한다. `Test-PdbPair`는 스테이징 폴더, 풀어 낸 ZIP·헤드리스 ZIP, MSI(같은 아키텍처는 관리 설치, 다른 아키텍처는 `dark.exe` 추출 트리)에서 `tasty.exe` 옆에 `tasty.pdb`가 있고 exe가 기록한 GUID·Age와 짝이 맞는지 확인한다. PDB가 없거나 짝이 다르면 빌드가 실패한다([배포본의 심볼](crash-diagnostics.md#배포본의-심볼)). 관리 추출은 실제 설치가 아니며 Windows ARM 기기의 설치·실행·제거는 미측정이다. 태그 없는 패키징 검증을 실제 Release 게시로 보지 않는다.

`build-windows.ps1`은 MSI 단계에서 `$env:WIX\bin`을 PATH 앞에 붙인다. `wix/main.wxs`는 파일마다 이름을 적어야 하므로, MSI 빌드 전에 `LICENSES/`의 각 파일에 대응하는 `Source`가 있는지 검사한다. 검증 포인트: MSI UpgradeCode 유지(`wix/main.wxs`), 설치→시작 메뉴→제거.

MSI 제거의 사용자 데이터 삭제는 `wix/main.wxs`의 `RemoveTastyUserData`가 맡는다. 이 동작을 실행 시퀀스로 옮기지 않는다.

- 삭제는 `InstallUISequence`의 `ExecuteAction` 뒤에서 실행한다. 화면 없는 제거(`/qn`)와 기본 UI 제거(`/qb`)는 이 시퀀스를 돌지 않고, 실패하거나 취소한 제거는 이 동작까지 오지 않으며, 제거를 시작한 사용자의 권한으로 돈다.
- 체크박스 프로퍼티 `TastyRemoveUserData`는 비공개라 명령줄로 켤 수 없다. 조건에 `REMOVE`를 쓰지 않는다. 마법사의 `Remove` 이벤트는 클라이언트 쪽 `REMOVE`를 채우지 않아 조건이 거짓이 된다. 서버 쪽 `UILevel`도 전체 UI에서 2라 쓸 수 없다.
- 설정 앱은 Windows Installer 항목의 제거 버튼에 `msiexec /qb /x {제품 코드}`를 실행한다. 기본 UI는 UI 시퀀스를 돌지 않으므로 이 버튼은 데이터를 남긴다. 체크박스는 수정(`ModifyPath`, `MsiExec.exe /I{제품 코드}`)의 Remove와 `.msi` 재실행에서 나온다. Windows 기본 제거와 수정을 그대로 두기 위해 기본 항목을 `ARPSYSTEMCOMPONENT`로 숨기거나 `NoModify`, `NoRepair`, 자체 `UninstallString`으로 바꾸지 않는다. `msiexec /x {제품 코드} /qf`로 직접 실행하면 WixUI가 `ResumeDlg`를 내므로 그 조건을 좁혀 체크박스 화면을 대신 띄운다.
- 화면 없는 작업(세션 0)에서는 `msiexec /i {제품 코드}`도 UI 수준이 3으로 낮아진다. 전체 UI는 콘솔 세션에서 확인한다.
- `crates/tasty-doc-guards/tests/wix_user_data_removal_needs_checkbox.rs`가 이 구성을 문자열로 검사한다. MSI를 만들어 실행하지는 않는다.

#### Linux

```bash
just dist-setup-linux              # 또는 수동 (아래)
./scripts/build-linux.sh           # uname -m 으로 x64/arm64 자동 감지
```

수동 사전 도구: `sudo apt install cmake pkg-config libfreetype6-dev libfontconfig1-dev rpm cpio binutils` + `cargo install cargo-deb cargo-generate-rpm` + `linuxdeploy`(GitHub continuous, `~/.local/bin`). 도구 역할은 위 [배포 패키징](#배포-패키징) 의 Linux 항목.

산출물: `tar.gz` · `.deb` · `.rpm` · `.AppImage` · 헤드리스 `tasty-headless-{version}-linux-{x64|arm64}.tar.gz` + `SHA256SUMS-linux-{x64|arm64}.txt`. 헤드리스 tar.gz 는 `--debug` 에서도 만든다. 자동 sanity check(tar.gz `tasty --version`, `dpkg-deb -I`, `rpm -qpi`, AppImage ELF 확인 — 실행은 안 함, GUI hang 회피). release·dist 빌드에서는 `verify_symbol_table`이 GUI·헤드리스 tar.gz, deb(`dpkg-deb -x`로 푼다), rpm, AppImage의 입력 AppDir에서 실행 파일에 `.symtab`이 있고 `.debug_info`가 없는지 확인하고, 아니면 빌드가 실패한다. 이 검사에는 `readelf`(binutils)가 필요하다([배포본의 심볼](crash-diagnostics.md#배포본의-심볼)). release·dist 빌드는 시작할 때 `/usr/lib/rpm/find-requires`·`rpm`·`rpm2cpio`·`cpio`·`readelf`가 있는지와 find-requires가 실제 ELF 하나에서 `libc.so.6()(64bit)`를 내는지 확인하고, 아니면 본체를 빌드하기 전에 멈춘다(`scripts/lib/rpm-requires.sh`의 `check_rpm_tools`). rpm은 payload를 풀어 실행 파일 기호표, 번들 플러그인 파일, Requires를 확인한다. Requires 대조는 `rpm -qpR` 결과에서 rpm 형식 항목(`rpmlib(...)`)과 수동 항목(`/bin/sh`, `[package.metadata.generate-rpm.requires]`의 이름)을 뺀 나머지가 payload ELF들의 직접 의존(RPM 4.18.2 elfdeps 규칙을 readelf로 다시 계산한 NEEDED·심볼 버전 요구·`rtld(GNU_HASH)`)과 정확히 같은지 본다. 전이 의존이 들어오거나 자동 Requires가 빠지면 실패한다. payload에서 실행 비트가 있는 파일은 모두 ELF여야 한다(실행 스크립트는 다른 생성기가 Requires를 더하므로 싣지 않는다). 수동 Requires는 `이름 = "*"` 형식만 읽고, 버전 조건을 적으면 대조가 실패한다. 원본 asset과 payload의 권한 차이는 Requires 집합을 바꿀 때만 드러난다. 판정 함수는 `crates/tasty-doc-guards/tests/rpm_requires_script.rs`가 합성 입력으로 시험한다. 실제 rpm 4.18.2 find-requires와 기대 집합의 일치는 러너의 build-linux.sh 실행이 판정한다.

넷 다 고지 세트(`LICENSE` · `THIRD_PARTY_LICENSES.md` · `LICENSES/` 의 모든 파일)를 함께 나른다 — 생성 단계는 없고 저장소의 파일을 그대로 스테이징한다. `LICENSES/` 는 파일 이름이 아니라 디렉토리로 읽는다(`scripts/lib/notice-set.sh` 의 `notice_set_files`, deb/rpm 은 `Cargo.toml` asset 의 glob). 이 폴더는 일반 파일만 담고, 하위 폴더·링크·점으로 시작하는 항목이 있으면 `notice_set_files` 가 실패해 deb/rpm 을 만들기 전에 빌드가 멈춘다(숨김 항목도 열거한다). 리스팅 확인은 줄마다 경로 하나인 목록(`tar -t`, `rpm -qpl`, deb 은 `dpkg-deb --fsys-tarfile` 뒤 `tar -t`)에서 앞의 `./` 만 떼고 줄 전체를 `<자리>/<파일>` 과 비교한다. 자리는 산출물마다 다르고(`tar.gz` 는 최상단, deb 은 `/usr/share/doc/tasty/`, rpm 과 AppImage 는 `usr/share/licenses/tasty/`) 정본 표는 [`THIRD_PARTY_LICENSES.md`](../../THIRD_PARTY_LICENSES.md) 에 있다. sanity check 가 tar.gz · deb · rpm 리스팅과 AppImage 의 AppDir 에서 세트의 파일 **전부**를 함께 본다. rpm 도구는 release·dist 빌드의 필수 도구라 rpm 쪽 확인을 건너뛰지 않는다.

#### 산출물 요약

| 플랫폼 | 명령 | 산출물 |
|--------|------|--------|
| macOS (arm64) | `./scripts/build-macos-dmg.sh` | `Tasty-{v}-macos-arm64.dmg` · 헤드리스 `tar.gz` |
| Windows (x64) | `.\scripts\build-windows.ps1` | `{zip,msi}` · 헤드리스 `zip` |
| Windows (ARM64) | `.\scripts\build-windows.ps1 -Arch arm64` | `{zip,msi}` · 헤드리스 `zip` (실행 확인 미측정) |
| Linux | `./scripts/build-linux.sh` | `{tar.gz,deb,rpm,AppImage}` · 헤드리스 `tar.gz` |

#### 관련

- 위 [빌드 프로필 (3종)](#빌드-프로필-3종) — 프로필 정의 · [release](release.md) — 릴리스 워크플로 · [release 러너](release.md#러너) — self-hosted runner

## macOS 키 입력 도우미

`scripts/macos-input-helper/`는 OS IME를 거치는 키 입력을 자동으로 보내는 개발용 앱(`TastyInputHelper.app`, 번들 ID `com.zilhak.tasty.input-helper`)이다. 셸에서 실행한 CGEvent 도구의 권한은 그 셸을 띄운 터미널 앱으로 판정된다([macOS 키 주입 권한](../ai-verification/macos-key-injection.md)). 이 앱을 `open`으로 띄우면 도우미가 책임 프로세스가 되므로, 터미널 앱이 아니라 도우미 하나에만 손쉬운 사용을 허용하면 된다. 배포 패키지와 CI에는 들어가지 않는다.

### 설치와 허용

```bash
./scripts/macos-input-helper/build-install.sh
```

- `swiftc`로 빌드하고 `TASTY_CODESIGN_IDENTITY`(기본 `Tasty Dev`)로 서명한 뒤 `~/Applications/TastyInputHelper.app`에 설치한다. 설치 경로는 `TASTY_INPUT_HELPER_APP`으로 바꿀 수 있다. 설치는 기존 경로를 지우고 다시 만든다. 그래서 경로가 `.app`으로 끝나지 않거나, 심볼릭 링크이거나, 이미 있는 것이 번들 ID `com.zilhak.tasty.input-helper`의 앱이 아니면 지우지 않고 빌드 전에 실패한다.
- identity가 키체인에 없으면 ad-hoc으로 서명하지 않고 실패한다. 이때는 `./scripts/macos-codesign-identity.sh`로 조회하고, 없으면 `--create`로 발급한다.
- 서명 뒤 Identifier, linker 서명이나 ad-hoc 서명이 아닌지, `codesign --verify --deep --strict` 통과 여부를 확인한다.
- 허용 대상: 시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용에서 `~/Applications/TastyInputHelper.app` 하나만 허용한다.
- 목록에 도우미를 올리려면 `open -W -n -a ~/Applications/TastyInputHelper.app --args request-access [--result <결과 JSON 경로>]`로 시스템 요청 창을 띄운다. 도우미는 `AXIsProcessTrustedWithOptions`(`kAXTrustedCheckOptionPrompt=true`)와 `CGRequestPostEventAccess()`를 부르고, 두 반환값과 호출 뒤 조회 값을 결과 파일에 적는다. 키는 보내지 않는다. 이미 허용돼 있으면 0, 아니면 3으로 끝난다. 사용자는 창에서 시스템 설정을 열어 스위치를 켠다.
- 요청 창은 앱마다 한 번만 뜬다. 다시 띄우려면 `tccutil reset Accessibility com.zilhak.tasty.input-helper`로 도우미 항목만 초기화한 뒤 `request-access`를 다시 실행한다. 이 명령은 그 번들 ID의 허용도 지운다.
- 재빌드 뒤에도 허용이 유지되려면 번들 ID, 서명한 leaf 인증서, 설치 경로가 모두 같아야 한다. 인증서를 지우고 `--create`로 다시 만들면 leaf가 바뀐다. `Tasty Dev`로 서명하면 지정 요구(`codesign -d -r-`)가 `identifier "com.zilhak.tasty.input-helper" and certificate leaf = H"<인증서 해시>"`이다. 같은 소스로 다시 빌드하면 CDHash도 같다. Info.plist 버전 문자열이나 소스를 바꿔 다시 빌드하면 CDHash는 바뀌고, 지정 요구와 `Authority=Tasty Dev`는 같다. 허용한 뒤 소스를 바꿔 다시 설치해도 다시 허용하지 않은 상태에서 권한 조회 값이 true였다. 소스를 바꿔 다시 설치한 뒤 다시 허용하지 않은 상태에서 키 게시(문자·수식키·Cmd 단축키)와 왼쪽 버튼 `click`도 격리 인스턴스에서 동작했다(측정함). 그 밖의 마우스 명령은 위 범위에서 미측정이다.

### 호출

```bash
open -g -W -n -a ~/Applications/TastyInputHelper.app --args \
    --result <결과 JSON 경로> [--command-file <명령 파일 경로>]
```

- `-g`는 도우미를 앞으로 가져오지 않는다. 도우미는 `LSUIElement` 앱이라 Dock과 메뉴 막대에도 나타나지 않는다. `-W`는 도우미가 끝날 때까지 기다린다. `-n`은 이미 떠 있는 도우미가 있어도 새로 실행한다.
- `open`의 종료 코드는 도우미의 종료 코드가 아니다. 도우미가 3으로 끝나도 `open -W`는 0을 돌려준다(측정함). 성패는 결과 파일의 `exit_code`로 판정한다. 판정 절차는 다음과 같다.
  1. 실행 전에 호출자가 결과 경로를 직접 지우고, 지워졌는지 확인한다. 도우미도 시작할 때 옛 파일을 지우지만, 지우지 못하면 종료 코드 1로 끝나며 옛 파일이 남을 수 있다.
  2. 실행 시작 시각을 기록하고 도우미를 실행한다.
  3. 결과 파일이 없으면 결과를 쓰지 못한 실패로 본다.
  4. 결과 파일이 있으면 `started_at`이 2의 시작 시각 뒤인지 확인한 뒤 `exit_code`를 읽는다. 앞이면 이번 실행의 결과가 아니다.
- `--command-file`을 빼면 권한만 조회한다. 이 조회와 `key`·마우스 단계가 없는 명령 파일은 요청 창을 띄우지 않으므로, 허용을 기다리며 반복해 실행해도 된다.
- 같은 실행 파일(`Contents/MacOS/TastyInputHelper`)을 셸에서 직접 실행하면 권한 조회 전용이다. 이때 책임 프로세스는 셸을 띄운 터미널 앱이라, 요청 창과 손쉬운 사용 등록이 그 앱에 붙는다. 그래서 직접 실행에서는 `request-access`와, 모든 명령 파일(`key`뿐 아니라 `switch_source`·`activate`만 든 파일, 단계가 없는 빈 파일이나 주석만 있는 파일도)을 권한 여부와 관계없이 거부한다. 어떤 단계도 실행하지 않고 요청도 하지 않은 채 결과 파일에 `refused_reason: "not_launched_via_open"`을 적고 종료 코드 2로 끝낸다. 명령 파일 없는 조회만 된다. 요청과 단계 실행은 `open`으로 띄운 경우에만 한다. 도우미는 부모 프로세스가 launchd(PID 1)이면 `open`(LaunchServices)으로 띄운 것으로 본다. 그 값을 결과 파일의 `parent_pid`와 `launched_via_launch_services`에 적는다. 이 판정에는 한계가 있다. 부모 셸이 먼저 끝나 launchd(PID 1)로 옮겨진 직접 실행(`nohup … &` 뒤 셸 종료, 이중 fork, `setsid`·`disown`)도 `open` 실행으로 판정된다. 그러면 요청이 셸을 띄운 앱에 붙을 수 있고, 결과의 `parent_pid`로도 사후에 구분되지 않는다. 그래서 직접 실행은 포그라운드로 한다.

명령 파일은 한 줄에 한 단계다. 빈 줄과 `#`로 시작하는 줄은 건너뛴다. 키는 macOS 가상 키코드 숫자로, 방향은 `surface.raw_key`와 같은 어휘로, 입력 소스는 TIS ID 그대로 적는다. 문자열(유니코드) 주입 명령은 없다. 문자열 주입은 IME를 거치지 않기 때문이다.

| 명령 | 동작 |
|---|---|
| `switch_source <ID>` | `TISSelectInputSource`로 입력 소스를 바꾸고, 다시 조회한 ID가 같은지 확인한다(예: `com.apple.inputmethod.Korean.2SetKorean`, `com.apple.keylayout.ABC`) |
| `key <키코드> <press\|release\|click>` | 키 이벤트를 게시한다 |
| `sleep <ms>` | 기다린다 |
| `delay <ms>` | 이후 키 이벤트 사이의 지연을 바꾼다(기본 30ms) |
| `activate <pid>` | 그 PID의 앱을 앞으로 가져온다 |
| `require_front <pid>` | 앞 앱 PID가 다르면 이후 모든 단계(키·마우스)를 실행하지 않고 종료 코드 5로 끝낸다. 키·마우스 단계 앞에 넣는다. 마우스 이벤트는 HID 탭에 게시돼 그 좌표의 맨 위 창으로 가므로, 이 검사는 클릭 대상 창을 보장하지 않는다. 클릭 좌표가 격리 인스턴스 창 안이고 다른 창(사용자 Tasty 포함)이 그 자리를 덮지 않는지 따로 확인한다 |
| `mouse_move <x> <y>` | 포인터를 옮긴다. 버튼이 눌려 있으면 그 버튼의 드래그 이벤트가 된다 |
| `mouse_down [left\|right]`, `mouse_up [left\|right]` | 현재 포인터 위치에서 버튼을 누르거나 뗀다(기본 `left`) |
| `click <x> <y> [left\|right] [count]` | 포인터를 옮긴 뒤 `count`번(1~3, 기본 1) 누르고 뗀다. 각 누름의 click state는 1, 2, 3이다 |
| `drag <x1> <y1> <x2> <y2>` | 왼쪽 버튼으로 (x1, y1)에서 누르고 열 단계로 끌어 (x2, y2)에서 뗀다 |

- 수식키는 수식키 키코드(Command 55, Shift 56, Option 58, Control 59)를 `press`/`release`로 보낸다. 실제 키보드와 같은 순서다. 수식키 이벤트는 `flagsChanged` 종류로 게시한다. 도우미는 누른 채로 있는 키코드를 기억하고, 그중 수식키의 플래그를 그 뒤 모든 키 이벤트에 싣는다. 왼쪽·오른쪽 Command(55, 54)처럼 같은 플래그를 내는 키는 둘 다 떼어야 플래그가 꺼진다. 예: Cmd+D는 `key 55 press`, `key 2 click`, `key 55 release`다.
- 키 이벤트의 소스와 탭은 `surface.raw_key`와 같다(`combinedSessionState`, `cgAnnotatedSessionEventTap`). 마우스 이벤트는 창 서버가 포인터를 옮기고 그 위치의 창으로 보내도록 `cghidEventTap`에 게시한다. 마우스 이벤트에도 눌린 수식키의 플래그를 싣는다.
- 마우스 좌표는 전역 화면 좌표(포인트, 주 화면 왼쪽 위가 원점)이고 -100000~100000의 실수다. Tasty surface를 누르려면 `tasty debug surface-rect --surface <id>`(창 안 물리 픽셀)를 배율로 나눠 포인트로 바꾸고, 창의 화면 위치(`CGWindowListCopyWindowInfo`의 창 bounds)를 더한다. 포인터의 처음 위치는 도우미 시작 때의 커서 위치다.
- 실패한 단계에서 멈추며, 그때까지 누른 채로 있는 키와 마우스 버튼을 뗀다(일반 키, 버튼, 수식키 순). 뗀 것은 그 단계의 `released_on_failure`에 키코드나 `mouse_left`·`mouse_right`로 적는다. 모든 단계가 성공했는데 명령 파일이 키를 누른 채로 끝나도, 끝내기 전에 같은 방법으로 떼고 `released_at_end`에 적는다. 하나씩 떼기를 시도하며, 떼지 못한 것은 키코드(`keycode`)나 버튼(`button`)과 오류를 `release_failed`에 따로 적는다(실패 단계에서는 그 단계의 `release_failed`). 정상 끝에서 떼지 못한 키나 버튼이 하나라도 있으면 종료 코드 1이다. 측정 범위: 키를 누른 채 끝나는 명령 파일(Shift 56 press, a 0 press)은 종료 코드 0, `released_at_end: [0, 56]`이었다. 대상 터미널에는 `A`가 들어갔고, 다음 a는 소문자로 들어가 Shift가 남지 않았다(측정함). 실패 단계에서는 Shift 56 press 뒤 `require_front`가 다른 앱 PID를 받아 실패하자, 종료 코드 5와 그 단계의 `released_on_failure: [56]`이 나왔다. 실패 뒤 단계는 실행되지 않았고, 다음 a는 소문자로 들어갔다(측정함). 떼기 이벤트 생성 자체가 실패하는 `release_failed` 경로는 인위적으로 만들 수 없어 미측정이다.
- 명령 파일 형식은 권한 판정·요청보다 먼저 전부 검사한다. 키코드는 0~65535, `activate`·`require_front`의 PID는 1~2147483647, `sleep`·`delay`의 ms는 0 이상의 정수, 마우스 좌표는 -100000~100000의 유한 실수, `click`의 횟수는 1~3이다. 범위 밖이면 종료 코드 2다.

### 결과 파일과 종료 코드

결과 파일은 JSON이다. 임시 이름으로 쓴 뒤 rename하므로, 파일이 보이면 다 쓴 것이다. 들어 있는 값은 다음과 같다.

- `helper_pid`, `parent_pid`, `launched_via_launch_services`, `started_at`, `finished_at`(UTC), `bundle_id`, `bundle_version`, `executable`
- `AXIsProcessTrusted`, `CGPreflightPostEventAccess`(실행 시작 때 값)
- `command_file`(받은 명령 파일 경로, 없으면 null), `refused_reason`(직접 실행을 거부했을 때 `not_launched_via_open`)
- `released_at_end`(정상 끝에서 뗀 키코드와 `mouse_left`·`mouse_right`), `release_failed`(정상 끝에서 떼지 못한 키나 버튼과 오류)
- `requested_access`. 요청했으면 `AXIsProcessTrustedWithOptions_prompt`, `CGRequestPostEventAccess`(두 함수의 반환값)와 `AXIsProcessTrusted_after_request`, `CGPreflightPostEventAccess_after_request`(호출 뒤 다시 조회한 값)
- `source_before`, `source_after`(입력 소스 ID)
- `modifier_method`, `event_source`, `event_tap`, `mouse_event_tap`
- `steps`(단계별 `command`, `ok`, `error`와 단계 정보), `exit_code`, `message`

| 코드 | 뜻 |
|---|---|
| 0 | 모든 단계 성공. `request-access`는 이미 허용돼 있음 |
| 1 | 그 밖의 실패. 모든 단계가 성공했어도 끝에서 눌린 키나 마우스 버튼을 떼지 못하면(`release_failed`가 비어 있지 않음) 1이다. 결과 파일을 쓰거나 rename하지 못했거나 옛 결과 파일을 지우지 못하면 이유를 stderr에 쓰고 1로 끝난다. 이때 이번 결과 파일은 없고, 옛 결과 파일을 지우지 못한 경우에는 옛 파일이 남아 있을 수 있다 |
| 2 | 인자·명령 파일 오류. 직접 실행에서 `request-access`나 명령 파일(빈 파일 포함)을 받은 경우도 여기에 든다(`refused_reason: "not_launched_via_open"`, 권한과 무관). `--result`가 없으면 결과 파일 없이 끝난다 |
| 3 | 이벤트 게시 권한 없음(`CGPreflightPostEventAccess=false`). 어떤 단계도 실행하지 않는다. 명령 파일에 `key`·마우스 단계가 있었으면 끝내기 전에 권한을 한 번 요청하고 `requested_access=true`를 적는다. 직접 실행은 이 판정 전에 2로 끝나므로 3은 `open` 실행이나 명령 파일 없는 조회에서만 나온다 |
| 4 | 입력 소스가 없거나 전환 실패 |
| 5 | 앞 앱 PID 불일치. 키를 보내지 않는다 |

도우미가 권한 요청 함수를 부르는 경우는 `open`으로 띄웠을 때 `request-access`와, 권한 없이 `key`·마우스 단계가 든 명령 파일을 받은 경우 둘뿐이다. 직접 실행에서는 부르지 않는다(명령 파일과 `request-access`는 2로 거부된다). 명령 파일 없는 조회, `key`·마우스 단계가 없는 명령 파일(`sleep`·`switch_source`·`activate`·`require_front`만 있는 파일), 형식 오류(종료 코드 2)에서는 부르지 않는다.

### 격리 인스턴스에서 측정할 때

- 도우미로 키를 보내는 중에도 화면보호기가 띄운 잠금 화면 UI가 뜬 적이 있다. 잠금 화면이 뜨면 앞 앱이 loginwindow로 바뀌어 `require_front`가 실패하고 키는 게시되지 않는다.
- `caffeinate -u`는 사용자 활동 assertion을 잠깐 만들어 잠금 화면 UI를 내리는 데 쓴다. HID 유휴 시간(`ioreg -c IOHIDSystem`의 `HIDIdleTime`)은 초기화하지 않는다. `caffeinate -u -t 5` 전후로 유휴 시간이 계속 늘었다(측정함).
- 키를 게시하기 전과 장시간 측정 중에는 주기적으로 `caffeinate -u -t 1`을 실행한다. 잠금 화면이 보이면(`ioreg`의 `CGSSessionScreenIsLocked`가 켜져 있거나 앞 앱이 loginwindow) 먼저 `caffeinate -u -t 2`로 풀어 본다. 그래도 풀리지 않으면 측정을 멈추고 사용자를 부른다.
- 새 `TASTY_HOME`으로 처음 띄우면 호스트 팝업 `info_modal`이 포커스를 잡아 키를 가로챈다. 도우미의 키는 터미널에 닿지 않고, 도우미 결과는 성공으로 나온다. 측정 전에 `tasty debug host-popup close --popup-id info_modal`로 닫고, `tasty debug ui-state`의 `gate_host_popup_focused`와 `keyboard_shortcuts_gated`가 false인지 확인한다.
- 앞에 있는 앱의 입력 소스를 `switch_source`(TIS)로 바꾸면 그 앱에는 다시 활성화된 뒤에 적용될 수 있다. ABC로 띄운 Tasty와 대조용 AppKit 앱(NSTextView)에서 모두, 앞에 둔 채 2벌식으로 바꾸면 영문 키가 그대로 들어갔다. `activate`로 다른 앱에 갔다가 돌아온 뒤부터 조합했다(측정함). 그래서 `switch_source` 뒤에는 포커스 왕복 단계를 둔다.
- 2벌식이 선택된 채로 Tasty 창이 처음 활성화되면, 첫 조합의 첫 자모가 조합 없이 확정된다(r, k, 스페이스 → `ㄱㅏ `). 포커스 왕복 유무와 첫 조합 전 대기 시간(0.3초, 5초)과 관계없이 났고, 두 번째 조합부터는 정상이었다. 같은 조건의 대조용 NSTextView 앱에서는 나지 않았다(측정함). 이미 한글을 쓴 창에서 입력 소스를 바꾼 뒤 재활성화한 첫 조합에서도 한 번 났다. 그래서 "처음 활성화"만이 조건은 아니며, 발생 조건과 원인은 확정하지 않았다. 측정에서는 띄운 직후와 `switch_source`·포커스 왕복 뒤마다 한 조합을 흘려보낸 뒤 IME 통과 판정(2벌식에서 r, k, 스페이스를 보내 PTY에 `가 `가 들어가는지)을 하고 측정한다.
- 조합 중이던 글자가 OS IME에 남으면 다음 한글 입력에 이어 붙는다. 회차를 반복할 때는 각 회차를 2벌식 스페이스로 시작해 남은 조합을 비우고, 그때 PTY에 들어간 것도 기록한다.

### 합성 입력의 한계

- 하드웨어 단계 키는 합성으로 같은 반응이 나오지 않을 수 있다: Caps Lock 한/영 전환, Globe/fn, JIS 영숫자·가나 키. 이 키들은 키보드 펌웨어·HID 단계에서 처리되는 부분이 있어, CGEvent 키코드만으로는 OS가 같은 동작을 하지 않을 수 있다. 그래서 한영 전환은 `switch_source`나, 사용자 설정의 입력 소스 전환 단축키(예: Ctrl+Space) 키코드로 하고, 어느 방식으로 했는지 보고서에 적는다. Caps Lock 전환과 같은 결과인지는 실 키보드 확인 범위로 남긴다.
- 수식키를 단독으로 누르고 떼는 것(`flagsChanged`)은 일반 keyDown/keyUp과 다른 이벤트다. 수식키 단독 동작을 재려면 수식키 키코드를 따로 보내야 한다. 키 이벤트에 플래그만 싣는 방식으로는 `flagsChanged`가 생기지 않는다.
- 자동 반복(키를 누르고 있을 때의 반복 입력)은 시스템이 합성 이벤트에 대해 만들어 주지 않을 수 있다. 반복이 필요하면 명시적으로 여러 번 보내고, 그것이 실제 자동 반복(autorepeat 플래그)과 다르다는 것을 적는다.
- 합성 Cmd+Tab(Command 55를 누른 채 Tab 48)은 앱 전환을 일으키지 않았다(측정함). 키 이벤트를 세션 탭에 게시하므로 시스템의 앱 전환기에 닿지 않는 것으로 보인다. 앱 전환 경로는 `activate`로 대신 재거나 실 키보드로 확인한다.
- 시스템 단축키로 설정한 입력 소스 전환 키(예: "이전 입력 소스 선택"에 지정한 키)는 그 키코드와 수식키로 보내면 동작해 TIS 입력 소스가 바뀌었다(측정함). 다만 Karabiner 같은 HID 단계의 키 변환(예: 오른쪽 Option을 그 키로 바꾸는 설정)은 거치지 않는다. 이것은 측정한 것이 아니라 원리다. 합성 이벤트는 HID 변환 단계 뒤에 게시되기 때문이다. 그래서 실제 키가 그 변환까지 거치는지는 실 키보드 범위다.
- 마우스는 `click`(왼쪽 버튼 1회)으로 Tasty surface를 눌러 포커스를 옮기는 것을 측정했다. `drag`, `mouse_move`, `mouse_down`·`mouse_up` 단독, 오른쪽 버튼, `click`의 count 2~3은 형식 검사만 했고 실제 동작은 미측정이다. 트랙패드 제스처(관성 스크롤, 핀치, 포스터치)는 합성할 수 없다.
- Secure Input이 켜진 칸(암호 입력 등)에는 합성 키가 들어가지 않는다.

### 보고 규칙

- 이 도우미로 측정한 보고서에는 "합성 입력으로 확인한 범위"와 "사용자 실 키보드 확인이 남은 범위"를 나눠 적는다.
- 위 한계에 걸리는 항목(Caps Lock·Globe·JIS 키, 자동 반복, 제스처, Secure Input)은 합성 결과와 관계없이 실 키보드 확인 범위에 둔다.

## 빌드 시간 진단

```bash
cargo build --release --timings   # target/cargo-timings/*.html — 크레이트별 frontend/codegen
cargo machete                     # 미사용 의존성 (컴파일 시간 낭비)
cargo modules / cargo depgraph    # 모듈/크레이트 의존 그래프 (크레이트 분리 검토용)
```

한 크레이트가 전체 빌드의 50%+ 를 잡으면 너무 크다는 신호 — 모듈 분리를 고려한다.

## 크레이트 분리 가이드

본 바이너리의 큰 leaf 모듈을 떼어낼 때 후보 조건: **out-degree 작음**(다른 src/ 모듈 거의 미참조) · **사이클 없음** · **충분히 큼**(1000줄+). 절차: `crates/tasty-<name>/` 생성 → `git mv` → 내부 경로 갱신 → 본 `Cargo.toml` 의존 추가 → `pub use tasty_<name> as <name>` 재수출(backward path 유지) → `cargo check`/`build` 검증. 기존 `crate::model::Foo` 경로가 그대로 동작하는 게 핵심이라 reverse import 갈아끼우기가 불필요하다.

**세 조건 중 크기는 보조 지표다** — 소비자가 둘 이상이고, 합칠 후보 크레이트에 *그 크레이트가 원래 몰라도 되는 의존* 을 들이게 되는 코드는 1000줄에 못 미쳐도 분리한다(판정은 의존 방향이 우선, [아키텍처 개요](../architecture/index.md)).

**재수출 형태는 둘 중 하나를 고른다.**

| 형태 | 쓰는 곳 | 이유 |
|------|---------|------|
| `pub use tasty_<name>::*;`(glob) | 본체에서 해당 크레이트의 공개 심볼을 모두 사용해도 되는 shim (`src/model.rs`, `src/adapters/ui/icons.rs`) | 재수출 목록을 제한할 필요가 없다 |
| `pub use tasty_<name>::{A, B, C};`(명시 목록) | 본체가 **일부만 써야 하는** 크레이트 (`src/adapters/cli.rs`, `src/adapters/ipc.rs`) | glob 이면 계층 위반이 컴파일 에러로 안 잡힌다 — 본체 어디서든 재수출 경로로 크레이트 전체에 닿는다 |

판단 기준은 "본체가 이 크레이트의 아무 심볼이나 써도 되는가" 하나다. 아니라면 명시 목록을 쓰고, 목록 밖 심볼을 쓰려는 시도가 컴파일 에러가 되게 둔다. 소스 스캔 가드(`crates/tasty-doc-guards/tests/layering.rs`)는 재수출을 우회한 직접 참조를 막는 **2차 방어**이지, 재수출 형태를 좁히는 것의 대체재가 아니다.

### 의존 방향 규칙 — 본체는 `tasty-cli` 를 참조하지 않는다

`tasty-cli` 는 **바이너리의 진입 계층**(인자 파싱 + 그 파싱 결과로 실행되는 커맨드 구현)이다. GUI 런타임·IPC 핸들러·앱 상태(`src/`)가 그 크레이트 내부를 직접 들여다보면 의존 방향이 뒤집혀, CLI 쪽 타입 변경이 GUI 를 깨고 GUI 재사용 목적의 로직이 CLI 안에 눌러앉는다. 양쪽이 함께 쓰는 코어(ssh · remote browse · stream 등)는 CLI 가 아니라 **별도 크레이트**에 두고 본체는 그쪽을 참조한다.

`src/adapters/cli.rs` 는 boot 진입점 10개(`Cli` · `Commands` · `Envelope` · `run_client_with` · `try_run_plugin_cli` · `print_augmented_help` · `print_command_tree` · `format_parse_error` · `localized_command` · `help_route`)만 이름으로 재수출한다. 그 밖의 CLI 심볼은 본체에서 경로 자체가 존재하지 않는다. 재수출 밖의 경로(`tasty_cli::` 직접 참조)는 **`crates/tasty-doc-guards/tests/layering.rs` 가 막는다**(그 가드는 의존 0 크레이트에 살아서 `doc-guards.yml` 이 경로 필터 없이 돌린다 — `develop`·`release`·`main` push 와 PR 마다 자동으로 돈다, [ci-gates](ci-gates.md)). 목록 두 개의 성격이 다르다:

| 상수 | 성격 | 내용 |
|------|------|------|
| `ALLOWED_PATHS` | 영구 허용 | `src/main.rs` · `src/boot.rs` · `src/boot/` · `src/adapters/cli.rs` — 바이너리가 CLI 파서를 소유하는 정당한 의존 |
| `BASELINE_FILES` | 한시 허용 | 이행 중인 위반의 스냅샷. **현재 비어 있다** — 본체는 `tasty-ssh` / `tasty-remote` / `tasty_ipc::client` 를 직접 참조한다. **줄어들기만 한다**(새 항목 추가 금지, 참조를 걷어냈으면 목록에서도 지워야 통과 — 역방향 검사) |

주석 안의 언급도 위반으로 잡는다 — 주석이 옛 경로를 가리키면 그것도 실제 오정보이므로 코드와 함께 갱신한다.

### 의존 방향 규칙 — 도메인은 조립부를 부르지 않는다

순수 명령 판단·이벤트 적용과 구조 projection은 `tasty-core`, 구조 값은 `tasty-model`에 있다.
root의 `src/core/`·`src/ports/`는 실행 adapter와 호환 조회 경계를 포함하므로 순수 domain crate와 동일하게 취급하지 않는다.
배치 근거는 [App·Engine·View 상태 소유권](app-state-ownership.md)와
[아키텍처 개요](../architecture/index.md)의 도메인 경계 절을 따른다.

`domain_does_not_reach_up.rs`는 두 crate의 제품 source에서 지정 상위 모듈·host crate 및 GUI 경로를 검사한다.
package별 anchor와 Git 추적 Rust 파일 명부를 순회 결과와 대조하고, 제품 GUI 조건은 0으로 고정한다.
순수 crate의 `crate::state`는 CoreState 모듈이므로 root의 View state로 오인하지 않는다.
테스트 전용 코드·주석·문자열은 제외한다. 직접 경로를 읽는 텍스트 검사이며 타입 별칭·전이 의존을 증명하지 않는다.
root 실행 객체 대여는 `engine_resource_ownership`, 지정 canonical/projection writer 호출은 별도 shell guard의 범위다.
자동화 실행부(`src/webhook/` · `src/hook_handler/` · `src/hook_runtime/`)의 inbound adapter 역참조는
`automation_runners_do_not_reach_inbound_adapters.rs`가 별도로 검사한다.

crate 분리로 바뀐 실제 재빌드 범위와 시간은 같은 profile·feature·cache 조건에서 측정한다.
이 소스 검사의 통과만으로 편집 빌드가 빨라졌다고 판단하지 않는다.
