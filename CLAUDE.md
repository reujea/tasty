# Tasty - 크로스 플랫폼 GPU 가속 터미널 에뮬레이터

cmux(macOS 전용)에서 영감을 받은 크로스 플랫폼 GPU 가속 터미널 에뮬레이터.
Rust 기반 네이티브 GUI 앱으로 Windows, macOS, Linux 를 모두 지원한다.
제작자의 입맛에 맞춘 터미널이며 AI 에이전트 협업은 편의를 위해 더하는 기능이다.

- 레포: git@github.com:zilhak/tasty.git
- 라이선스: MIT

## 기술 스택

| 역할 | 라이브러리 |
|------|-----------|
| 윈도우/입력 | winit |
| GPU 렌더링 | wgpu |
| UI 위젯 | egui (UI) + 커스텀 셰이더 (터미널) |
| VTE 파싱 | termwiz |
| PTY | portable-pty (Windows: ConPTY) |
| IPC | TCP (127.0.0.1, 동적 포트, `~/.tasty/tasty.port`) |
| CLI | clap |

# 핵심 원칙

작업 전에 [정체성](docs/identity.md)과 [제품 원칙](docs/principles/index.md)을 읽는다. 일반 구현의 계약과 이유는 해당 설계·기능·개발 문서에 기록한다. 원칙 간 충돌은 원칙 문서에서 우선순위를 해소하고, 원칙의 예외·미충족을 채택한 결정만 ADR에 남긴다.

- 에이전트는 터미널 작업을 수행할 수 있다. release에서는 사용자 포커스를 빼앗는 동작을 제공하지 않고 debug에서는 사용자 조작 재현이 가능해야 한다. debug 핸들러는 모듈 선언에 cfg(debug_assertions)를 둔 별도 파일로 분리한다([debug IPC](docs/dev-guide/debug-ipc.md)).
- 에이전트가 자기 작업에 사용하는 기능은 IPC·CLI 양쪽에서 제공한다. 대상은 ID로 지정하며 현재 선택·스크롤·히스토리 보존 계약은 [포커스 정책](docs/design/policies/focus.md)에 있다.
- Windows·macOS(Apple Silicon)·Linux를 1급 지원한다. Windows ARM도 릴리스 대상이다. 검증 여건과 지원 등급을 구분하며 OS별 코드는 cfg로 나누어 다른 OS의 컴파일을 깨뜨리지 않는다.

# 작업 규칙

## 시작 전 (필수)

1. [`docs/identity.md`](docs/identity.md)와 [제품 원칙](docs/principles/index.md)을 먼저 읽는다.
2. [`docs/concepts/ubiquitous-language.md`](docs/concepts/ubiquitous-language.md) — 용어를 잘못 쓰면 코드/문서 일관성이 깨진다. 특히 Window / Pane / Tab / Surface 계층, 상위/하위 레이아웃 구분, Modal / Popup / Toast 구분.
3. 해당 작업 영역의 가이드 문서 확인 — [`docs/index.md`](docs/index.md) 에서 전체 인덱스 확인.

## 임시 파일·계획 위치

- 임시 파일(스크린샷·디버그 스크립트·테스트 출력)과 구현 계획은 로컬 지침이 지정한 Git 제외 폴더에 둔다. 프로젝트 루트나 소스 디렉터리에 만들지 않는다. 로컬 전용 경로는 추적 문서에 적지 않는다.
- 임시 파일은 작업 후, 계획은 구현 완료 후 정리한다.
- `docs/`에는 현재 설계와 동작을 기록한다. 진행 중인 계획과 변경 일기는 넣지 않는다.

## 문서 갱신 (필수)

모든 작업 완료 시 docs 를 갱신한다.

- 새 기능이 구현되면 [`docs/features/`](docs/features/index.md) 의 해당 카테고리 문서에 추가 (인덱스는 `docs/features/index.md`).
- Surface 종류(kind)의 동작은 제공 주체(host·plugin)와 관계없이 [`docs/surfaces/`](docs/surfaces/index.md) 의 kind 문서에 둔다.
- 기존 기능이 변경되면 해당 문서 업데이트.
- **사용자에게 보이는 동작**(메뉴·단축키·설정 키·CLI 명령·설치 절차)이 바뀌면 로컬 지침이 정한 홈페이지 기획서를 바로 고친다. 공개 사이트와 사용자 가이드는 [tasty-page](https://github.com/zilhak/tasty-page) 레포에 있다. 그 레포의 `main`이 최신 페이지이고, 기능 설명이나 버전이 바뀌는 변경은 게시용 `release` 브랜치를 **릴리스할 때** `main`으로 승격해 공개한다. 사이트는 사용자가 받을 수 있는 버전을 설명하기 때문이다. 공개된 Tasty 버전의 기능 설명이나 버전이 바뀌지 않는 변경(문구, 메타데이터 등)은 릴리스와 무관하게 바로 게시한다([릴리스 절차](docs/dev-guide/release.md#3-사이트-갱신)). 가이드는 `docs/` 명세와 독자가 다르다(설치해서 쓰는 사람). 집필 규칙·번역·사이트 구조는 그 레포의 문서를 따른다.
- 해당 카테고리의 인덱스(예: [`docs/features/index.md`](docs/features/index.md), [`docs/dev-guide/index.md`](docs/dev-guide/index.md))를 갱신. [`docs/index.md`](docs/index.md) 는 카테고리 진입점 표라, 카테고리 자체가 신설/폐지될 때만 손댄다.
- 구현 히스토리는 남기지 않는다. **현재 상태만** 기술한다.
- docs 문서에는 마크다운 체크박스(task list)를 넣지 않는다. 진행 상태는 계속 바뀌므로 로컬 작업 문서에서 관리한다. 완료 조건은 Given/When/Then 형식의 평문 불릿으로, 검증·절차 항목은 평문 불릿이나 번호 목록으로 적는다([`docs/documentation-model.md`](docs/documentation-model.md) §6). `crates/tasty-doc-guards/tests/no_checkbox_in_docs.rs`가 이를 검사한다.
- 같은 사실을 고치기 전에 인용을 검색하고 변경 뒤 코드·문서를 함께 대조한다. 같은 목록을 여러 곳에서 관리하면 서로 일치하는지 검사한다([`docs/dev-guide/duplicated-sets.md`](docs/dev-guide/duplicated-sets.md)). 자세한 규칙은 [`docs/documentation-model.md`](docs/documentation-model.md) §6에 있다.
- 검사가 특정 사실을 보장한다고 쓸 때는 해당 결함을 실제로 검출하는지 확인한다. 확인한 범위와 한계는 [문서 작성 규칙](docs/documentation-model.md)과 [자체 검증](docs/dev-guide/self-verification.md)에 따라 적는다.
- 일반 구현의 계약·이유는 해당 가이드에, 원칙의 예외·미충족을 채택한 결정만 ADR에 쓴다. 원칙 간 충돌은 원칙에서 해소하고 원칙 변경 이력을 누적하지 않는다. ADR 작성 전 [양식](docs/adr/template.md)을 읽는다.

## 커밋 정책

기능 하나의 수정이나 추가가 끝나면 사용자에게 다시 묻지 않고 커밋한다. 서로 다른 기능을 한 커밋에 묶지 않는다.

Conventional Commits 형식을 따른다 (예: `feat(themes): add latte theme`). 커밋 제목과 본문은 모두 영어 평문으로 작성한다. 제목과 본문 각각에 Markdown 서식(강조, 백틱, 제목, 목록, 링크, 코드 블록 등)을 사용하지 않는다. 형식·type 목록·body 작성 기준은 [`docs/dev-guide/commit-convention.md`](docs/dev-guide/commit-convention.md).

## 버전 정책

버전 형식은 `MAJOR.MINOR.PATCH`.

### 본체 (`Cargo.toml` 루트)

- **패치 버전**: 사용자가 빌드를 요청했을 때, 마지막 빌드 이후 새 커밋이 있고 사용자가 막지 않았다면 AI 가 자동으로 +1 한다.
- **README 버전 배지 동기화 (필수)**: 본체 patch를 올리면 `README.md`와 `README.ko.md`의 `badge/version-X.Y.Z-blue`도 같은 값으로 바꿔 같은 커밋에 넣는다. `readme_badge_parity` 검사가 Cargo.toml과 배지의 일치를 확인한다. `doc-guards.yml`은 경로 필터 없이 main push와 PR에서 이 검사를 실행하므로, 커밋 전에도 직접 검사한다([CI 가이드](docs/dev-guide/ci-gates.md)).
- **마이너 / 메이저**: 사용자가 직접 지정한다. 임의로 올리지 않는다.
- **자체 검증용 빌드** (`cargo build` / `cargo test`): 버전을 올리지 않는다.

### Plugin (`crates/tasty-plugin-*/Cargo.toml`)

매니페스트가 있는 번들 플러그인의 제품 내용이 달라지면 패치 버전을 올린다.
해당 디렉터리뿐 아니라 normal/build 의존성의 변경도 포함한다.
저장소 안 path 의존성은 workspace 밖에 있어도 검사한다.

- Rust는 rustfmt 정규화 후 비교하고 테스트 전용 내용은 제외한다.
- 버전 선언 자체는 내용 변경 증거에서 제외한다. 파일 수나 커밋 type으로 면제하지 않는다.
- 주석만 바뀐 경우에도 보수적으로 버전 증가를 요구할 수 있다. 사유를 확인하고 처리한다.
- Cargo.toml, 매니페스트, Cargo.lock을 함께 갱신해 같은 커밋에 넣는다.
- 여러 플러그인이 영향을 받으면 각각 적용한다. minor/major는 사용자가 정한다.
- `tasty.pluginBump=deferred`(worktree 범위)인 lane 작업 트리에서는 lane이 버전 줄을 건드리지 않는다. 병합 단계가 `scripts/plugin-bump-fixup.sh`로 커밋마다 붙인다. 절차는 [Git 훅](docs/dev-guide/git-hooks.md#lane-작업-트리의-p1-보류)에 있다.
- 서명 파일은 빌드 산출물이며 커밋하지 않는다.
- 나누어 push할 때는 이미 원격에 발행된 버전과 현재 내용을 다시 비교한다.
  pre-commit 통과가 최종 배포 버전 검사를 대신하지 않는다.

내용 비교, 보조 도구 준비, 비교할 원격 커밋을 읽지 못했을 때의 처리는
[릴리스 절차](docs/dev-guide/release.md#플러그인-버전-비교)를 따른다.
설치된 플러그인을 실행 중인 앱에 반영하는 절차는
[플러그인 개발](docs/dev-guide/plugin-development.md)의 §9.1을 따른다.

## 빌드

Tasty 는 cargo workspace 다 (본 바이너리 + `crates/*` 64 개 — 그중 `tasty-plugin-sdk-wasm` 은 workspace `exclude`). 빌드 프로필 3 종 (`dev` / `release` / `dist`).

> 크레이트 수는 `crates/` 바로 아래에서 Cargo.toml을 가진 디렉터리 수다. workspace에서 제외된 크레이트도 세며 루트 패키지는 제외한다. [아키텍처 문서](docs/architecture/index.md), 이 문서, 두 README의 수치를 함께 갱신한다. `architecture_crate_list_complete`와 `readme_badge_parity`가 실제 디렉터리 목록과 비교한다.

- **일상 개발**: `cargo build` 또는 `cargo build --release`.
- **배포 산출물 빌드 (DMG / MSI / AppImage 등)**: `cargo build --profile dist`. 일상 빌드에는 사용하지 않는다 (3.5 배 느림).

워크스페이스 구조, 프로필 상세, LTO 설명, 빌드 시간 측정, 크레이트 분리 가이드 전체: [`docs/dev-guide/build.md`](docs/dev-guide/build.md).

## Conductor/에이전트 병렬 작업 시 빌드·검증 명령

변경에 맞는 검사를 직접 실행한다. 자동 실행 범위와 빌드 조합은
[CI 가이드](docs/dev-guide/ci-gates.md)에서 확인한다.
워크플로 파일에 명령이 있다는 사실만으로 해당 커밋에서 실행됐다고 보고하지 않는다.

| 목적 | 명령 |
|---|---|
| 개발 빌드 | `cargo build` |
| 플러그인을 포함한 개발 빌드 | `cargo build --workspace` |
| release 컴파일·빌드 확인 | `cargo build --release` |
| lint | `cargo clippy --workspace --all-targets --locked` |
| 포맷 | `cargo fmt --check` |
| 문서 검사 | `export TASTY_PAGE_DIR="$(bash scripts/fetch-design-source.sh)"` 뒤 `cargo test -p tasty-doc-guards --locked`. 디자인 정본 대조 시험이 `dtcg/SOURCE` 커밋의 tasty-page를 읽는다 |
| 전체 테스트 | `cargo test --workspace --locked` |
| 파일 크기·예외 총합 | `bash scripts/check-file-size.sh`와 `bash scripts/check-frozen-sum-ratchet.sh` |

셸 검사는 `check-intent-discipline.sh`, `check-allow-reason.sh`,
`check-shared-walk-ratchet.sh`, `check-core-writer-ratchet.sh`, `check-shell-assets.sh`를 사용한다.
필요한 보조 도구가 없거나 오래됐으면 먼저 준비한다. 실제 위반인지 확인하기 전에
상한이나 예외를 바꿔 검사를 통과시키지 않는다.

루트의 `cargo build`만으로 플러그인 실행 파일은 갱신되지 않는다.
`PROFILE=debug just build-plugins`로 빌드와 스테이징을 하고,
검증할 실행 파일과 번들 사본이 새 코드인지 확인한다.
공개키와 번들 준비는 빌드 전에 수행한다.
`cp -p` 등으로 오래된 시각을 복원했다면 내용이 바뀌었어도 갱신이 생략될 수 있다.

workspace에서 제외된 `tasty-plugin-sdk-wasm`은 `--manifest-path`로 따로 검사한다.
홈페이지와 디자인 정본은 이 레포에 없다. 그 검사는 [tasty-page](https://github.com/zilhak/tasty-page) 레포의 절차를 따른다.
Cargo에는 별도 의존성 설치 단계가 없고 build/test 때 필요한 것을 가져온다.
일반적인 소스 변경 검증에 무조건 전체 캐시를 지우지는 않는다.

빌드·테스트 통과와 실제 실행 확인은 구분한다.
[자체 검증](docs/dev-guide/self-verification.md)에 따라 격리된 debug 인스턴스로 시나리오를 재현한다.

# 코드 정책

## 언어·툴

- 언어: Rust (edition 2024)
- 빌드: cargo, 포맷: rustfmt, 린트: clippy

## 길이 타입 (필수)

내부 소스에서 길이 값을 단순 `f32` 로 다루는 것은 **금지**. 반드시 `PhysicalPx` 또는 `LogicalPx` 타입을 사용한다.

- `PhysicalPx`: GPU, wgpu, winit 마우스 좌표, `Rect` 필드
- `LogicalPx`: egui UI, Theme 상수, 사이드바 너비
- 두 타입 간 직접 대입 불가. `to_physical(sf)` / `to_logical(sf)` 변환 필수. 사각형은 `PhysicalRect` / `LogicalRect` 짝으로 네 변을 한 번에 변환한다.
- 두 좌표계의 타입을 섞어 쓰면 컴파일 오류가 난다. `.value()`로 숫자를 꺼낸 뒤 필요한 `× ppp` 또는 `÷ scale_factor` 변환을 빠뜨리는 경우는 `src/dpi_conversion_guard.rs`에서 검사한다. 타입 검사만으로는 이 누락을 찾을 수 없다.

상세: [`docs/concepts/typed-length.md`](docs/concepts/typed-length.md).

## UI 디자인 (필수)

모든 색상·폰트 크기·선 굵기·간격은 `Theme` 에서 가져온다. `from_rgb(...)` 등으로 하드코딩하지 않는다.

핵심 정책 (4px 그리드, 14px 폰트 상한, 1px 보더, 호버/액티브 오버레이 자동 도출, 4.5:1 대비, 터미널 콘텐츠 애니메이션 0ms): [`docs/design/systems/theme.md`](docs/design/systems/theme.md) 의 "UI 디자인 규칙" 섹션.

## 갤러리 완전성 · gallery-first (필수)

갤러리(`crates/tasty-gallery`)에는 본체의 모든 UI 컴포넌트를 포함한다. 디자인 카탈로그에서 빠진 컴포넌트도 갤러리에서 삭제하지 않고 디자인 요청으로 보완한다.

디자인 정본은 [tasty-page](https://github.com/zilhak/tasty-page) 레포 `main`의 `vendor/`다. 디자인은 구현 세션이 정하지 않는다. 디자인 변경은 디자이너 세션이 tasty-page 갤러리에 먼저 반영하고, 이 레포의 갤러리에 같은 형상을 구현한 뒤 본체에 적용한다. 갤러리와 본체는 같은 컴포넌트 소스를 쓰므로 모양만 바꾸는 변경은 갤러리 단계에서 본체에도 반영된다. 앱이 따르는 디자인 판은 `crates/tasty-design-tokens/dtcg/SOURCE`의 tasty-page 커밋이다. 절차는 [디자인 변경 워크플로](docs/dev-guide/design-change-workflow.md)에 있다.

상세·근거: [테마 시스템 (운영 상세)](docs/design/systems/theme.md) · [`docs/dev-guide/gallery-first.md`](docs/dev-guide/gallery-first.md) · [`docs/design/policies/gallery-completeness.md`](docs/design/policies/gallery-completeness.md).

**UI를 디자인에 맞출 때는 구조와 토큰을 모두 확인한다.**

1. **레이아웃 구조**: [디자인 정합 지침](docs/design/systems/design-parity-notes.md)과 [디자인·갤러리 매핑](docs/design/systems/design-gallery-mapping.md)을 읽는다. 디자인의 grid·컬럼·패딩·정렬 구조를 컴포넌트와 코드에 그대로 옮긴다.
2. **토큰**: [테마 규칙](docs/design/systems/theme.md)에 따라 색·폰트 크기·선 굵기·간격을 토큰에서 가져온다. raw px와 `from_rgb`를 직접 쓰지 않는다. 4px 그리드, 14px UI 폰트 상한, 1px 보더를 지킨다.

레이아웃 구조가 맞아도 토큰이 다를 수 있으므로 두 항목을 따로 확인한다.

## 국제화 (필수)

모든 UI 문자열은 `t()` 함수를 통한 번역 키로 노출한다. 자연어 하드코딩 금지. 새 문자열 추가 시 `lang/{en,ko,ja,zh-Hans}.toml` 네 파일에 모두 키 추가.

상세 (API, lang 파일 위치, plugin 네임스페이스, 하드코딩 허용 예외): [`docs/dev-guide/i18n.md`](docs/dev-guide/i18n.md).

## 단축키 (필수)

**ESC·Enter 같은 기본 조작을 제외한 Tasty 자체 단축키는 설정의 단축키 탭에서 변경 가능해야 한다.** 설정 대상은 `KeybindingSettings`로 노출하고 하드코딩하지 않는다. macOS NSMenu / Windows AcceleratorTable / Linux Wayland 같은 OS 메뉴 측 key equivalent 도 `KeybindingSettings` 의 대응 binding 값을 따라가야 한다.

예외는 **OS가 관리하며 Tasty가 변경·해제·가로채기할 수 없는 단축키**다. macOS Spotlight의 `Cmd+Space`나 OS 전역 윈도우 전환 등이 해당한다. Tasty가 제어할 수 없는 단축키는 설정에 노출하는 대상에서 제외한다.

Tasty가 직접 등록하는 NSMenu·AcceleratorTable 메뉴의 key equivalent는 `KeybindingSettings`의 binding을 사용한다. 대응 binding이 없으면 비워 둔다. `cut:`·`performClose:`·`miniaturize:`·`hide:`처럼 OS 표준 selector를 사용해도 단축키를 상수로 지정할 수는 없다. `with_default_menu(false)`로 만든 메뉴의 단축키는 Tasty가 관리한다.

tasty 특화 액션 (예: `tastyQuit:` / `tastyNewWindow:` / split / convert 등) 은 **반드시** `KeybindingSettings` 의 대응 필드를 읽어 key equivalent 를 설정해야 한다. binding 이 빈 vec 이면 key equivalent 도 비워두어 단축키 없는 메뉴 항목으로 표시한다.

상세 (modifier 매핑 규칙, 위치 기반 추상화): [`docs/design/policies/key-mapping.md`](docs/design/policies/key-mapping.md).

## 에러 처리 (필수)

`Result` 를 `let _ =` 로 무시하지 않는다. 에러는 처리하거나 `tracing::warn!` / `tracing::error!` 로 로그를 남긴다.

상세 (로그 레벨 선택, 의도적 무시 시 주석 규칙): [`docs/dev-guide/error-handling.md`](docs/dev-guide/error-handling.md).

## 소스 주석의 TODO 파일 및 디자인 changelog 인용 금지 (필수)

주석은 코드만으로 알기 어려운 이유·제약·안전 조건을 짧게 설명한다. 과거 변경 일기, 코드의 단순 반복, 중복 설명, 사라진 문서를 가리키는 주석은 삭제한다. 공개 API 계약·필요한 예제·라이선스·예외 사유는 보존한다.

추적 파일에 로컬 작업 폴더 경로·티켓 번호·일회성 디자인 changelog를 근거로 남기지 않는다.
새 clone에서 읽을 수 없는 자료이기 때문이다. 이유를 직접 쓰거나 현재 가이드·ADR을 연결한다.
할 일 표시인 `TODO:`와 `TODO(범위):`는 사용할 수 있다.

빌드 산출물, 사용자 홈의 런타임 경로, 설정 키 같은 식별자는 필요한 문서에서 설명한다.
금지 형태를 입력으로 쓰는 검사는 해당 패턴만 예외로 둔다.
세부 범위와 인용 방법은 [문서 작성 규칙](docs/documentation-model.md#경로와-코드-인용)을 따른다.
`no_todo_file_citation`은 규칙을 검사하지만 모든 산문의 의미를 판단하지는 않는다.

# 작업 시 참고 문서

## 개발 가이드

전체 목록은 [`docs/dev-guide/index.md`](docs/dev-guide/index.md). 자주 참조하는 항목:

- [`docs/dev-guide/self-verification.md`](docs/dev-guide/self-verification.md) — **커밋 전에 직접 검증.** 사용자에게 검증을 떠넘기지 않는다.
- [`docs/dev-guide/build.md`](docs/dev-guide/build.md) — 워크스페이스 구조, 빌드 프로필
- [`docs/dev-guide/popup-implementation.md`](docs/dev-guide/popup-implementation.md) — Popup 구현 (`PopupDef` 시스템, `egui::Window` 직접 사용 금지)
- [`docs/dev-guide/debug-ipc.md`](docs/dev-guide/debug-ipc.md) — debug 빌드 전용 IPC + 격리 정책
- [`docs/dev-guide/model-view-split.md`](docs/dev-guide/model-view-split.md) — Model + Host View 분리 패턴
- [`docs/dev-guide/gpu-rendering.md`](docs/dev-guide/gpu-rendering.md) — GPU 렌더링 구조
- [`docs/dev-guide/agent-runner.md`](docs/dev-guide/agent-runner.md) — 작업 의존 관계(DAG)에 따라 작업을 실행하는 방법. 여러 AI 에이전트가 같은 Tasty를 사용할 때 필요한 협업 기능 6종(`agent.*`: task DAG · barrier · semaphore · lease · reducer · rate-limit)을 설명한다. 기능과 인터페이스는 [`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md)에 있다.
- [`docs/dev-guide/plugin-development.md`](docs/dev-guide/plugin-development.md)(민감 데이터는 그 문서 §6 의 [민감 데이터 절](docs/dev-guide/plugin-development.md#민감-데이터--regular--secret--keyring-선택)), [`plugin-permissions.md`](docs/dev-guide/plugin-permissions.md) — Plugin 제작. **번들 plugin 코드를 고친 뒤 실행 중 tasty 인스턴스에 재빌드·재시작 없이 반영할 때도 이 문서 §9.1** (빌드 → 재서명 → `disable` → `upgrade-builtins` → `enable` 순서) — "새 plugin 만들기"가 아니라도 반드시 먼저 확인한다.

## 자체 검증

UI / 렌더링 / 환경별 검증 시에는 [`docs/ai-verification/`](docs/ai-verification/) 의 항목별 문서를 반드시 확인. 전체 목록은 [`docs/ai-verification/index.md`](docs/ai-verification/index.md).

UI 변경 시 [`docs/ai-verification/screenshot-methods.md`](docs/ai-verification/screenshot-methods.md#시각-판정-체크리스트) 의 "시각 판정 체크리스트" 절(체크리스트 + 스크린샷 판단 휴리스틱)을 따른다.

## 디자인 / 아키텍처

[`docs/design/`](docs/design/), [`docs/architecture/`](docs/architecture/). 전체 목록은 `docs/index.md`.
