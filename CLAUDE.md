# Tasty - 크로스 플랫폼 GPU 가속 터미널 에뮬레이터

cmux(macOS 전용)에서 영감을 받은 크로스 플랫폼 GPU 가속 터미널 에뮬레이터.
Rust 기반 네이티브 GUI 앱으로 Windows, macOS, Linux 를 모두 지원한다.
WezTerm/Alacritty 와 유사한 접근이지만 AI 코딩 에이전트에 특화된 기능을 제공한다.

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

Tasty 의 정체성과 거기서 나오는 **불가침 원칙** 전문은 [`docs/identity.md`](docs/identity.md) — **작업 전 필독.** 아래는 코드 작업 시 즉시 적용하는 집행 요지다 (배경·근거는 identity.md).

1. **사용자 행동 ↔ 에이전트 행동 분리** — 에이전트 행동(IPC/CLI)의 부수효과가 사용자 상태(포커스 / 닫은 항목 히스토리 / 선택·스크롤·커서)에 닿지 않는다. 사용자 입력 재현(키/마우스 주입, popup 강제 open/close, 메뉴 강제 invoke, 포커스 전환)은 release 에 없고 `#[cfg(debug_assertions)]` debug 격리로만 존재한다. debug 핸들러는 **모듈 선언에 cfg 가 붙은 별도 파일**로 모은다(디렉토리 이름이 아니라 그 성질이 기준이다 — `src/adapters/ipc/handler/` 의 `debug.rs`·`popup.rs` 등). 판단 기준: *에이전트가 자기 작업에 필요한가(→ release) vs 사용자 조작을 재현하는가(→ debug)*. 상세 [`docs/dev-guide/debug-ipc.md`](docs/dev-guide/debug-ipc.md).
2. **AI 에이전트 조작 가능성** — 에이전트 기능(surface/tab/workspace 생성·닫기·조회, 클립보드, 알림, 파일 열기, 메타데이터 등)은 **IPC + CLI 양면** 으로 동작해야 한다. GUI 전용 에이전트 기능 금지. 부족하면 추가한다.
3. **포커스 독립성** — 모든 명령은 대상을 ID 로 직접 지정, list 는 전 워크스페이스 순회, 활성 상태 의존 동작 금지, release 엔 포커스 변경 API 없음. 상세 [`docs/design/policies/focus.md`](docs/design/policies/focus.md).
4. **크로스 플랫폼** — Windows/macOS/Linux 모두 1급. 플랫폼 분기는 `#[cfg(...)]`, 한 OS 전용 기능도 다른 OS 컴파일이 깨지지 않게.

# 작업 규칙

## 시작 전 (필수)

1. [`docs/identity.md`](docs/identity.md) 먼저 읽기 — Tasty 정체성과 불가침 원칙. 모든 설계의 축.
2. [`docs/concepts/ubiquitous-language.md`](docs/concepts/ubiquitous-language.md) — 용어를 잘못 쓰면 코드/문서 일관성이 깨진다. 특히 Window / Pane / Tab / Surface 계층, 상위/하위 레이아웃 구분, Modal / Popup / Toast 구분.
3. 해당 작업 영역의 가이드 문서 확인 — [`docs/index.md`](docs/index.md) 에서 전체 인덱스 확인.

## 임시 파일·계획 위치

- 작업 중 생성하는 임시 파일(스크린샷, 디버그 스크립트, 테스트 출력 등)과 구현 작업 계획 md 는 **프로젝트 루트나 소스 디렉토리에 만들지 않는다.** 구체적인 위치는 `.gitignore` 대상인 로컬 작업 폴더이며, 그 경로는 커밋되지 않는 **로컬 전용 지침**(Claude Code 가 세션 시작 시 이 파일과 함께 로드하는 프로젝트 로컬 CLAUDE.md)이 정한다 — git 에 존재하지 않는 경로는 이 파일에 적지 않는다(아래 "소스 주석의 TODO 파일 및 디자인 changelog 인용 금지" 와 같은 원칙).
- 임시 파일은 작업 후 정리하고, 계획 md 는 구현 완료 후 삭제한다.
- `docs/` 에는 현재 상태의 설계/구조만 기록하고, 진행 중인 작업 계획이나 히스토리는 넣지 않는다.

## 문서 갱신 (필수)

모든 작업 완료 시 docs 를 갱신한다.

- 새 기능이 구현되면 [`docs/features/`](docs/features/index.md) 의 해당 카테고리 문서에 추가 (인덱스는 `docs/features/index.md`).
- 기존 기능이 변경되면 해당 문서 업데이트.
- **사용자에게 보이는 동작**(메뉴·단축키·설정 키·CLI 명령·설치 절차)이 바뀌면 공개 사이트의 사용자 가이드 [`site/content/`](site/content/index.md) 도 같은 커밋에서 갱신한다. 가이드는 `docs/` 명세와 독자가 다르다(설치해서 쓰는 사람) — 소스 경로·ADR·IPC 메서드명을 넣지 않는다. 영어 번역(`site/content/en/`)을 손봤으면 `--stamp` 한다 ([`docs/dev-guide/site.md`](docs/dev-guide/site.md)).
- 해당 카테고리의 인덱스(예: [`docs/features/index.md`](docs/features/index.md), [`docs/dev-guide/index.md`](docs/dev-guide/index.md))를 갱신. [`docs/index.md`](docs/index.md) 는 카테고리 진입점 표라, 카테고리 자체가 신설/폐지될 때만 손댄다.
- 구현 히스토리는 남기지 않는다. **현재 상태만** 기술한다.
- docs 문서에 마크다운 체크박스(task list)를 넣지 않는다 — 체크 상태는 진행 추적이라 transient 다. Acceptance Criteria 는 평문 Given/When/Then 불릿, 검증·절차 항목은 평문 불릿이나 번호 목록 ([`docs/documentation-model.md`](docs/documentation-model.md) §6). `crates/tasty-doc-guards/tests/no_checkbox_in_docs.rs` 가 강제한다.
- **한 사실을 고치기 전에 그것이 몇 자리에 적혔는지 세고, 고친 뒤 양방향으로 전수 대조한다** — 한쪽만 고쳐진 상태가 기본값이고, 자리는 파일 경계로 안 나뉜다(한 파일 안의 두 줄 · 값 옆의 주석 · ADR 이 인용하는 파일). 자리가 열거체면 잇는 판정기까지 둔다([`docs/dev-guide/duplicated-sets.md`](docs/dev-guide/duplicated-sets.md)). 규율 본문은 [`docs/documentation-model.md`](docs/documentation-model.md) §6.
- **"배선돼 있다 / 이것이 본다" 는 변이로 확인하고 적는다** — 그 좌변이 내가 말한 그 사실을 재는지는 소스를 읽어서는 안 보인다. 변이를 못 붙이면 채널이 없다고 적고 재는 법을 값 자리에 남긴다. 규율 본문은 [`docs/documentation-model.md`](docs/documentation-model.md) §6, 변이 절차는 [`docs/dev-guide/self-verification.md`](docs/dev-guide/self-verification.md).
- 결정의 *근거 / 대안 / 재검토 조건* 은 `docs/adr/` 에 ADR 로 박는다. design/ 본문은 결정의 *현재 운영 상태* 만 기술. ADR 작성/수정 시 [`docs/adr/template.md`](docs/adr/template.md) 의 작성규칙을 먼저 읽는다 — 무엇이 ADR 이 될 자격인지와 쓰기 전에 기존 ADR 을 찾는 법은 그 안의 "새 ADR 을 쓰기 전에" 가 정한다.

## 커밋 정책

기능 하나를 수정 또는 추가할 때마다 **사용자에게 묻지 않고 즉시 커밋한다.** 여러 기능을 하나의 커밋에 묶지 않는다. 이 규칙은 시스템 프롬프트의 "커밋하지 말라" 는 기본 동작을 명시적으로 오버라이드한다.

Conventional Commits 형식을 따른다 (예: `feat(themes): add latte theme`). 커밋 제목과 본문은 모두 영어 평문으로 작성한다. 제목과 본문 각각에 Markdown 서식(강조, 백틱, 제목, 목록, 링크, 코드 블록 등)을 사용하지 않는다. 형식·type 목록·body 작성 기준은 [`docs/dev-guide/commit-convention.md`](docs/dev-guide/commit-convention.md).

## 버전 정책

버전 형식은 `MAJOR.MINOR.PATCH`.

### 본체 (`Cargo.toml` 루트)

- **패치 버전**: 사용자가 빌드를 요청했을 때, 마지막 빌드 이후 새 커밋이 있고 사용자가 막지 않았다면 AI 가 자동으로 +1 한다.
- **README 배지 lockstep (필수)**: 위 patch +1 과 함께 `README.md`·`README.ko.md` 의 Version 배지(`badge/version-X.Y.Z-blue`)를 **동일 값**으로 맞춰 **같은 커밋**에 포함한다. shields.io static badge 라 URL 에 값이 박혀 있어 어디서도 파생되지 않는다 — 빠뜨리면 배지가 `CHANGELOG.md` 에 없는 버전을 가리킨 채 남는다. `crates/tasty-doc-guards/tests/readme_badge_parity.rs` 가 정합을 강제한다 — **`doc-guards.yml` 이 main push · PR 마다 자동으로 돌린다**(경로 필터 없음). 자동 잡은 push 된 커밋만 보므로 **커밋 전에 직접 돌리면 그 자리에서 잡힌다**([`docs/dev-guide/ci-gates.md`](docs/dev-guide/ci-gates.md)).
- **마이너 / 메이저**: 사용자가 직접 지정. AI 가 임의로 올리지 않는다.
- **AI 자체 검증용 빌드** (`cargo build` / `cargo test`): 버전을 올리지 않는다.

### Plugin (`crates/tasty-plugin-*/Cargo.toml`)

- **패치 버전 자동 +1**: 특정 plugin 디렉토리(`crates/tasty-plugin-<name>/` 중 **`tasty-plugin.toml` 을 가진 것** — 이름만 같고 매니페스트가 없는 라이브러리 크레이트는 대상이 아니다) 의 **빌드 산출물이 달라지면** 그 plugin 의 `Cargo.toml.version` 의 패치를 +1 하고 **같은 커밋**에 포함한다. 사용자가 명시적으로 막지 않는 한 적용.
  - **판정 대상은 그 디렉토리만이 아니다.** 번들 plugin 은 워크스페이스 크레이트를 링크하고(`tasty-plugin-agent-common`·`tasty-plugin-sdk`·`tasty-utils` 등), 그것이 바뀌면 plugin 산출물이 달라진다. 그래서 판정은 **워크스페이스 내부 의존 폐포**를 함께 본다 — 그 대신 폐포 안에서는 **출하되는 내용**만 센다(인라인 `#[cfg(test)]` 와 `#[cfg(test)] mod x;` 로만 선언된 파일 전체는 산출물 밖이라 차이로 안 센다). 근거·측정·대안은 [`docs/adr/0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md`](docs/adr/0166-the-plugin-version-gate-judges-the-artifact-not-the-directory.md). **`tasty-utils` 한 줄이 번들 plugin 9 개 전부의 bump 를 요구할 수 있다** — 그것이 정상 동작이다.
  - **판정 기준은 파일이 staged 되었는가가 아니라 내용이 달라졌는가다.** 대상 경로는 `src/`·`lang/`·`assets/`·`Cargo.toml`·`tasty-plugin.toml`·`build.rs` 이고, 그중 `.rs` 는 **rustfmt 로 정규화한 뒤** 비교한다. **두 toml 에서는 `version` 줄 한 줄을 증거에서 뺀다** — 판정하는 값 자신을 증거로 쓰면 값을 되돌리는 커밋이 또 한 번의 bump 를 요구하는 순환이 된다(분할 착지에서 병합하는 쪽이 값을 다시 정하는 것은 규칙이 정상으로 규정한 흐름이다). 나머지 줄(feature·의존·bin 선언)은 그대로 본다. 그래서 워크스페이스 전역 `cargo fmt` 정리는 plugin bump 를 요구하지 않는다. 문서(`*.md`)·러너 스크립트 등 산출물 밖 파일도 마찬가지다. 근거·측정·대안·재검토 조건은 [`docs/adr/0137-plugin-version-bump-is-judged-by-content-not-file-count.md`](docs/adr/0137-plugin-version-bump-is-judged-by-content-not-file-count.md). **파일 수 문턱("큰 커밋은 sweep 이니 봐준다")은 쓰지 않는다** — 그 수는 문턱값과 세는 대상에 따라 2 배 흔들려 재현되지 않는다.
  - **주석만 바뀐 변경은 이 판정에 걸린다**(rustfmt 는 주석을 지우지 않는다). 알려진 오탐이고, 그때는 patch 를 올리거나 사유를 밝히고 넘어간다 — 자동으로 봐주지 않는 이유는 정규식 주석 제거가 raw string 안의 `//` 를 잘못 지워 **거짓 음성**을 만들기 때문이다.
  - **한 lane 에서 한 번 올리면 된다 — 단 그 lane 이 한 번에 착지할 때만.** 판정은 커밋마다가 아니라 `main` 대비 두 끝점으로 한다 — 목적이 라이브 반영이라 재sync 는 한 번 올라가면 동작하고, 이 기준은 `--amend`·rebase 에도 흔들리지 않는다.
  - **★ lane 이 여러 번에 나뉘어 착지하면 위 기준이 깨진다.** 두 lane 이 서로 다른 base 에서 같은 값으로 올릴 수 있고, 앞쪽이 먼저 push 되면 **그 값은 앞쪽 내용으로 이미 발행된다.** 뒤쪽은 버전 줄이 이미 그 값이라 아무 변화도 안 만들고, 같은 버전 아래 **두 개의 다른 산출물**이 남는다. 그 상태가 조용한 이유는 **버전 줄이 발행된 값을 정하기 때문**이다 — 재sync 가 파일을 옮겨 주더라도, 같은 버전 문자열이 서로 다른 두 산출물을 가리키는 상태는 남는다(`plugin.list`·업그레이드 판정·배포 아카이브가 그 문자열을 믿는다). 빌드도 테스트도 초록인데 **무엇이 발행됐는지가 값으로 안 남는다.**
    - 그래서 **lane 의 `--staged` 검사는 이 물음에 원리적으로 답하지 못한다.** 그 검사는 "내 커밋이 버전을 올렸나" 를 보는데, 물어야 할 것은 **"발행된 값과 지금 내용이 짝이 맞나"** 다. lane 의 base 가 낡으면 통과도 낡는다.
    - 판정의 올바른 범위는 **직전 push 지점 → 현재**다. 그 모수를 사람이 손으로 구하지 않는다 — **push 순간이 곧 발행 순간이고, git 이 pre-push 훅에 직전 발행 tip 을 stdin 으로 건넨다.** 그래서 `.githooks/pre-push` 의 `B.9` 가 밀려는 ref 마다 `check-plugin-version-bump.sh --range <원격 tip> <로컬 tip>` 을 돌린다. lane 은 자기 base 기준 값을 **보고만** 하고, 최종 값은 병합하는 쪽이 정하되, 그 최종 값이 발행 직전에 한 번 더 발행 모수로 재어진다.
    - 그 훅이 **모수를 못 정하는 경우는 통과가 아니다.** 원격 tip 이 로컬에 없는 객체면(얕은 clone · fetch 안 함) 견줄 대상이 없으므로 실패시키고 `git fetch` 를 찍는다. 견줄 발행 값이 원래부터 없는 경우 — 원격에 그 ref 가 없는 최초 생성 push, 삭제 push — 만 사유를 찍고 건너뛰며, 건너뛴 수를 함께 찍어 **빈 모수를 훑은 초록과 실제로 판정한 초록이 같은 줄로 안 보이게** 한다.
    - 실측 2026-09-05: 이 형태가 하루에 두 번 났다(`tasty-plugin-claude` 0.1.59 · `tasty-plugin-markdown` 0.1.63). 두 번 다 lane 은 규칙대로 했고 규칙이 분할 병합을 안 다룬 것이다.
  - **자동 채널이 셋이고, 셋이 같은 스크립트를 서로 다른 모수로 부른다**: pre-commit(P.1)은 **staged 를 `main` 과의 merge-base 와**, pre-push(B.9)는 **원격 tip 을 로컬 tip 과**, `plugin-version-check.yml`(main push · PR, `crates/**` 변경 시)은 **밀려 들어온 범위의 두 끝점**을 본다. 앞의 하나는 "내 커밋이 버전을 올렸나", 뒤의 둘은 "발행된 값과 지금 내용이 짝이 맞나" 에 답한다 — 물음이 다르므로 P.1 의 초록을 발행 통과로 읽지 않는다. 판정 불가(비-git · 없는 rev · rustfmt 부재 · 원격 tip 을 로컬에서 못 찾음)는 어디서든 통과가 아니라 실패다.
  - **`Cargo.lock` 을 같은 커밋에 담는다 (필수).** 워크스페이스 멤버의 `version` 이 바뀌면 `Cargo.lock` 이 stale 이 되고, `--locked` 를 쓰는 모든 게이트가 **테스트를 한 건도 돌리기 전에** 실패한다. 함정은 조용하다 — `sed` 로 `Cargo.toml` 을 고치면 `Cargo.lock` 은 **안 바뀌고**, cargo 를 한 번 돌려야 갱신된다. 그래서 sed 직후의 `git add -A` 는 lock 갱신이 없는 상태를 스테이징한다. 손으로 버전을 고쳤으면 **커밋 전에 `cargo metadata` 등 아무 cargo 명령을 한 번 돌려라.** (본체 bump 에 대한 같은 규칙은 [`docs/dev-guide/release.md`](docs/dev-guide/release.md) 에 있었고 plugin 쪽에는 빠져 있었다.)
- **매니페스트 lockstep (필수)**: 그 plugin 의 매니페스트(`crates/tasty-plugin-<name>/tasty-plugin.toml`) 의 `version` 을 **Cargo.toml 과 동일 값**으로 맞춰 **같은 커밋**에 포함한다. Cargo.toml 만 올리고 매니페스트를 방치하면 `plugin.list`·업그레이드 판정이 노출·비교하는 값이 어긋난다(version drift). 정합은 `crates/tasty-doc-guards/tests/plugin_manifest_version_parity.rs` 가 강제한다 — 통합 테스트라 **자동 실행은 push 후 `check-headless` 잡에서만** 일어난다(컴파일은 두 조합 모두 자동). 자동 잡은 push 된 커밋만 보므로 **커밋 전에 직접 돌려야 그 자리에서 잡힌다**([`docs/dev-guide/ci-gates.md`](docs/dev-guide/ci-gates.md)). **`.sig` 는 커밋 대상이 아니다** — `.gitignore` 로 제외된 빌드 산출물이며, dev/debug 빌드는 서명을 검증하지 않고 release/dist 빌드가 `scripts/sign-bundle.sh` 로 자동 재생성한다. 따라서 매니페스트 version bump 시 커밋되는 건 매니페스트 `version` 한 줄뿐이고, 재서명은 커밋 절차가 아니다(로컬 release 빌드 확인이 필요할 때만 `scripts/sign-bundle.sh --key ~/.tasty-keys/dev.pem --manifest <경로>`).
- 여러 plugin 이 함께 변경된 커밋은 각 plugin 에 독립 적용 (각각의 Cargo.toml + 매니페스트 모두 갱신).
- **마이너 / 메이저**: 사용자가 직접 지정. AI 가 임의로 올리지 않는다.
- 본체 정책과 독립적으로 적용된다 (같은 커밋에 본체와 plugin 이 함께 변경돼도 본체는 본체 규칙, plugin 은 plugin 규칙).

자동 +1 절차와 릴리스 절차 전체: [`docs/dev-guide/release.md`](docs/dev-guide/release.md).

> **패치 버전 bump 는 발행된 값을 정하는 일이다.** 실행 중 tasty 에 번들 plugin 변경을 **재시작 없이** 반영하는 `upgrade-builtins` 재sync 는 **버전이 갈래를 고르고 그 갈래 안에서 내용이 판정한다**(2026-09-07 부터: 판정이 mtime 이 아니라 내용이다). 갈래는 셋이다 — 같은 버전이면 **내용이 다른 파일만 옮기고**, 번들이 높으면 전량 덮어쓰고, **설치본이 번들보다 높으면 내용을 아예 안 보고 건너뛴다**(그 경우만 `--force` 로 내린다). 즉 "버전을 올려야 반영된다" 는 거짓이지만 "버전과 무관하다" 도 과장이다. bump 가 정하는 것은 반영 여부가 아니라 **그 산출물이 어느 버전으로 발행됐는가**이고, 그래서 같은 버전에 두 산출물이 생기는 상태는 여전히 금지다. 반영 절차 전체는 [`docs/dev-guide/plugin-development.md`](docs/dev-guide/plugin-development.md) §9.1.

## 빌드

Tasty 는 cargo workspace 다 (본 바이너리 + `crates/*` 60 개 — 그중 `tasty-plugin-sdk-wasm` 은 workspace `exclude`). 빌드 프로필 3 종 (`dev` / `release` / `dist`).

> 위 크레이트 수는 [`docs/architecture/index.md`](docs/architecture/index.md) 가 정본이고 이 문장은 그 복제본이다 — `crates/tasty-doc-guards/tests/architecture_crate_list_complete.rs` 가 `crates/` 실측과 대조하므로 고칠 때 함께 움직인다. 두 README 의 Workspace 배지·본문도 같은 좌변을 쓰고 `readme_badge_parity` 가 본다. 좌변은 **`crates/` 바로 아래에서 `Cargo.toml` 을 가진 디렉토리 수**다 — `exclude` 된 것도 세고, 레포 루트의 본 바이너리 크레이트는 안 센다.

- **일상 개발**: `cargo build` 또는 `cargo build --release`.
- **배포 산출물 빌드 (DMG / MSIX / AppImage 등)**: `cargo build --profile dist`. 일상 빌드에는 사용하지 않는다 (3.5 배 느림).

워크스페이스 구조, 프로필 상세, LTO 설명, 빌드 시간 측정, 크레이트 분리 가이드 전체: [`docs/dev-guide/build.md`](docs/dev-guide/build.md).

## Conductor/에이전트 병렬 작업 시 빌드·검증 명령

`role:conductor` 스킬(스택 중립적 공통 문서)이 프로젝트별 빌드/lint/test 명령을 이 CLAUDE.md에서 찾도록 되어 있다. 이 프로젝트(cargo workspace)의 명령은 다음과 같다.

**"어디서 도는가" 열을 반드시 함께 읽는다** — 이 표의 명령이 CI·훅과 1:1로 같지 않다. 자동 채널이 없는 칸은 **네가 안 돌리면 아무도 안 돈다.** 전체 매트릭스(트리거·러너 포함)는 [`docs/dev-guide/ci-gates.md`](docs/dev-guide/ci-gates.md) 가 정본이다.

| 목적 | 명령 | 어디서 도는가 |
|------|------|---------------|
| 빌드 (dev) | `cargo build` — **plugin 을 고쳤으면 `cargo build --workspace`** (아래) | 자동 채널 없음. macOS·Windows 컴파일은 `crossplatform-check` 의 잡이 배선돼 있고(작업 트리 기준), Linux **dev(debug) gui** 컴파일은 아무 자동 잡도 안 본다(release-gui 컴파일은 `check-release` 가 본다 — 두 조합은 `debug_assertions` 이 반대라 상보적이다). **배선과 초록은 다르다** — 표 아래 "배선돼 있다는 것과 초록이라는 것" 참조 |
| 빌드 (release 검증) | `cargo build --release` | not-debug(release) gui **컴파일 정합성**은 `crossplatform-check` 의 `check-release` 잡이 본다(`cargo check --workspace --release`, main push · PR). **컴파일까지만** — 실행·dist 산출물은 아니다(dist 는 `build-check.yml` 수동) |
| lint | `cargo clippy --workspace --all-targets --locked` | 이 조합을 배선한 자동 잡은 `crossplatform-check` 의 Windows 잡 하나뿐이다 — **그 하나가 빨간 동안 이 조합에는 실행 채널이 없다.** pre-push 훅은 비슷하지만 다르다(`--locked` 없음 + `-- -D clippy::correctness`) |
| 포맷 검사 | `cargo fmt --check` | ✅ 자동 — `format-check.yml`(main push · PR) + pre-commit A.2 |
| 셸 게이트 (Intent 규율 · 사유 없는 `#[allow]` · 공용 순회 래칫 · 셸 자산 정적 검사) | `bash scripts/check-intent-discipline.sh` · `bash scripts/check-allow-reason.sh` · `bash scripts/check-shared-walk-ratchet.sh` · `bash scripts/check-shell-assets.sh` | ✅ 자동 — `script-gates.yml`(main push — **문서·site 만 담은 push 는 제외** · PR). 초 단위로 끝난다. **둘 다 판정기 하나를 먼저 짓는다** — 억제나 호출을 문자열 리터럴·주석이 아니라 **코드**에서 세려면 마스킹이 필요하고, 그 판정은 셸이 아니라 `tasty-doc-guards` 의 `mask-source` 가 한다(같은 물음에 답을 둘로 만들지 않으려는 것이다 — 한때 awk 판·정규식 판·러스트 판 셋이 있었다). 없으면 **세지 않는다** — 그 사실을 말하고 판정 불가(exit 2)로 끝낸다. 세고 나서 그 값을 안 쓰는 것이 아니라 **애초에 안 센다**: 쓰이지 않는 수를 상한 옆에 찍으면 누군가 그것을 상한과 견주고, 좌변 전체를 세는 비용도 그대로 버려진다(실측 2026-09-08: 판정기 없는 갈래의 벽시계가 `check-allow-reason.sh` 3.24 s → 0.01 s · `check-shared-walk-ratchet.sh` 0.30 s → 0.01 s, 판정기 있는 갈래는 그대로). 원문에서 셌다면 그 계수는 문자열·주석 안의 언급까지 세는 **더 많이 잡는** 방향이라 조용한 통과는 안 만든다. 그런데 **더 많이 잡는 것이 안전하다는 논거가 이 두 게이트에서는 안 선다**(레포의 모든 게이트에서 안 선다는 말이 아니다 — `check-plugin-version-bump.sh` 는 그 오탐을 **일부러** 감수한다 — 그것은 이 잡이 아니라 `plugin-version-check.yml` 과 두 로컬 훅(pre-commit · pre-push)이 부른다 — 셋을 모수와 함께 세는 정본은 위 plugin 버전 정책 항이다: 그 셋 중 pre-commit 이 부르는 셸 게이트는 그것 하나뿐이고, 갓 클론한 트리에는 판정기가 정상적으로 없으며, 그 처방인 patch +1 은 아무것도 헐겁게 만들지 않는다), 이유가 둘로 갈릴 뿐이다: 래칫(뒤쪽)은 양방향이라 그 값이 상한을 넘고 실패문이 **"상한을 올려서 통과시키지 마라"** 로 나가 **실재하지 않는 회귀에 대한 처방**이 되며(따르면 래칫이 영구히 헐거워진다), 잔여 0 hard-fail(앞쪽)은 더 잡은 것이 그대로 **위반**으로 나가고 실패문이 **"`// intent-exempt: <사유>` 를 추가하세요"** 로 나가 **실재하지 않는 위반에 대한 처방**이 된다(따르면 그 자리가 영구히 면제된다). 실측 2026-09-07: rebase 직후 판정기가 낡아 두 게이트가 그 문구로 빨개졌고, 마스킹 유무의 차는 정확히 +2 였다 — 회귀가 아니라 **세는 사본이 바뀐 폭**이다. 그래서 두 게이트는 값이 아니라 **판정 가능 여부로 먼저 갈리고**, 낡았으면 **그 자리를 헐겁게 만드는 레버를 만지지 말라**는 말과 함께 재빌드 명령을 그대로 찍는다(`cargo build -p tasty-doc-guards --bin mask-source`) — 레버는 게이트마다 다르다: 뒤쪽은 **상한**, 앞쪽은 **`intent-exempt` 주석**이다. 실측 2026-09-07(앞쪽): 판정기를 안 보이게 하고 앞쪽을 돌리면 위반 1 건이 나왔고 그것이 `src/adapters/ui/popup/frame.rs` 의 **doc 주석 한 줄**이었다 — 마스킹이 지우려고 존재하는 바로 그것이다. 그 자리는 오래 rc=2 가 아니라 **rc=1** 이었고(뒤쪽만 이 교훈을 반영했다), `tests/intent_discipline_gate.rs` 가 이제 그 갈래를 종료 코드로 고정한다. 뒤쪽은 물음이 둘이라 사본도 둘이다: 억제가 **있는가**는 주석까지 덮은 사본에서, 사유 **주석**이 붙었는가는 주석이 남은 사본에서 묻는다. 뒤쪽은 잔여가 0 이 아니라 **상한 래칫**이다 — 늘어도 실패하고, **줄어도 실패한다**(상한을 같이 내리라는 뜻: 남는 여유가 곧 안 보는 구간이다). 세는 형태는 `#[allow]` 과 `#[cfg_attr(<조건>, allow(...))]` 둘 다이고, 근거 마커는 `reason:`·`이유:`·`complexity-exempt:`·`SAFETY:` 를 **붙어 있는 주석 블록 전체**에서 찾고, **마커 뒤에 내용이 있어야 근거로 인정한다**(같은 줄이거나 바로 다음 주석 줄) — `// 이유:` 한 줄은 마커지 근거가 아니다. 셋째도 같은 잡에서 같은 판정기를 먼저 짓는 **상한 래칫**이다 — 공용 순회를 안 거치는 직접 `read_dir` 를 센다. **그 잡이 부르는 것은 넷 전부다** — 여기에 셋만 적혀 있으면 빠진 하나는 아무도 커밋 전에 안 돌린다. 넷째(`check-shell-assets.sh`)는 성질이 앞 셋과 다르다 — 판정기를 짓는 것이 아니라 **외부 검사기**(shellcheck)를 요구하고, 없으면 통과가 아니라 rc 2 로 멈춘다(받는 법은 실패문이 찍고, `scripts/dev-setup.sh` 가 받아 둔다). 래칫도 잔여 상한도 아닌 **잔여 0 hard-fail** 이고, 문턱(warning 이상)과 그 아래를 안 세기로 한 근거는 [ADR-0295](docs/adr/0295-shell-assets-are-judged-at-warning-and-above.md). 이 게이트는 pre-commit `A.3` 이 **staged 경로**로도 부른다 |
| 파일 SLOC (복잡도 게이트 파트 B) | `bash scripts/check-file-size.sh` · `bash scripts/check-frozen-sum-ratchet.sh` | ✅ 자동 — `complexity-check.yml`(main push — **문서·site 만 담은 push 는 제외** · PR · 수동). **4 초면 끝나는데 커밋 전 채널이 없다** — pre-commit 도 pre-push 도 이것을 안 부르므로 위반은 push 하고 나서야 드러난다(실측 2026-09-13: 한 회차에서 새로 임계를 넘긴 파일 3 개가 전부 이 방식으로 드러났다). 처방은 **모듈 분할**이다 — `.complexity-file-allowlist` 등재는 그 파일 자신이 "부채 대장" 이라 부르는 자리이고, 등재하면 `frozen-sum-budget` 을 같은 커밋에 올려야 하며(`scripts/check-frozen-sum-ratchet.sh` 가 양방향으로 고정한다) 그 뒤로는 그 파일이 얼마나 자라든 **아무 신호도 안 난다**. **그 잡은 스크립트를 둘 부른다** — 뒤쪽은 동결 목록에 오른 파일들의 출하 SLOC **합**을 양방향으로 고정하고, 계측기(tokei)의 doc 주석 편향까지 같이 고정한다. 편향이 움직이면 합이 성장 없이 반대로 움직이므로 `.complexity-file-allowlist` 의 예산 줄과 편향 줄을 **같은 폭으로 함께** 옮긴다(실패문이 쓸 값을 그대로 찍어 준다 — 예산을 합에 맞추면 같은 커밋의 진짜 성장까지 사면된다). 임계·동결 래칫의 근거는 [ADR-0168](docs/adr/0168-the-file-sloc-threshold-is-not-derived-and-the-freeze-ratchets-one-way.md), 트리거 이력은 [ADR-0131](docs/adr/0131-file-sloc-gate-needs-a-firing-trigger.md) |
| 테스트 | `cargo test --workspace --locked` | **이 조합(기본 feature) 그대로는 자동 채널 없음** — `test.yml` 의 전체 스위트는 `workflow_dispatch` 전용이다. 다만 `check-headless` 가 main push 마다 **헤드리스 조합의 전체 스위트**를 돌아 통합 테스트 대부분이 자동으로 실행된다(실측 `d7dc4079`: 통합 항목 474 중 438 — **그 시점의 `--skip` 은 3 건이었다**). 자동으로 안 도는 것은 `tests/gui_tests.rs` 와 명명 `--skip` 이고, **지금 `--skip` 은 1 건**(`multi_window_owner_routing`)이다. 위 438 은 그 시점 값이라 지금 수와 다르다 — 세는 명령은 [`ci-gates.md`](docs/dev-guide/ci-gates.md) 에 있다 |

- **배선돼 있다는 것과 초록이라는 것은 다르다 (필수).** 위 표는 워크플로 **파일**이 무엇을 배선했는지를 작업 트리 기준으로 적는다 — 그 잡이 지금 통과하는지는 적지 않는다([ADR-0139](docs/adr/0139-numbers-in-docs-are-classified-by-lineage-not-by-name.md): 커밋마다 바뀌는 값은 적는 순간 낡는다). 그러니 **"CI 가 본다" 를 근거로 자기 검증을 면제하려면 그 자리에서 직접 세라.** 규칙의 정본은 [`docs/dev-guide/ci-gates.md`](docs/dev-guide/ci-gates.md) 의 "채널이 있다는 것은 그 잡이 초록이라는 뜻이 아니다" 이고, 층의 정의는 [ADR-0142](docs/adr/0142-channel-claims-are-written-against-the-working-tree.md) 에 있다. 재는 명령:
  ```bash
  gh run list --limit 10                          # 워크플로 결론까지만 나온다
  gh run view <run-id> --json jobs \
    --jq '.jobs[] | "\(.conclusion) \(.name)"'   # 잡 단위 — 면제 판정은 이 줄로 한다
  ```
  **첫 줄만 보면 살아 있는 채널을 죽은 것으로 센다.** 워크플로 결론은 잡 하나만 빨개도 빨강이라, 잡이 여럿인 워크플로에서는 나머지가 초록인 것이 안 보인다 — `crossplatform-check` 가 그 형태다(잡 셋). 반대 방향의 함정도 있다: **앞 스텝이 죽으면 뒤 스텝은 아예 안 돈다.** 그때 뒤 스텝의 결과는 `0 failed` 로도 안 나온다 — 줄 자체가 없다. 그래서 **잡이 빨간 동안 그 잡이 배선한 커버리지는 실패가 아니라 미측정으로 센다.**

- **`cargo build` 는 plugin 바이너리를 다시 만들지 않는다 (필수)**: `crates/tasty-plugin-*/src/` 를 고치고 루트에서 `cargo build` 를 돌려도 `target/debug/tasty-plugin-<name>` 이 갱신되지 않는다(실측). `cargo build --workspace` 나 `cargo build -p tasty-plugin-<name>` 은 갱신한다. host 는 **debug 빌드에서 부팅할 때만** `copy_if_newer`(`crates/tasty-host-plugin/src/builtin.rs`) 로 `target/debug/builtin-plugins/` 를 채운다(release/dist 는 빌드 단계에서 스테이징). 모든 프로필은 번들에서 `<TASTY_HOME>/plugins/` 로 sync 하므로, 안 만들어진 바이너리는 **낡은 채로 조용히 실행된다.** 그래서 plugin 을 고친 뒤 GUI·주입으로 확인하면 **직전 plugin 코드를 재게 되고, 그 오진은 양방향이다** — 고친 것이 안 고쳐진 것처럼도, 되돌린 것이 여전히 고쳐진 것처럼도 보인다. 뒤쪽은 뮤테이션 "죽었다/살아남았다" 판정을 통째로 뒤집으므로 그 위의 모든 판정이 무효가 된다. 정식 절차는 `PROFILE=debug just build-plugins`(빌드 + 스테이징). **측정 전에 한 줄로 확인한다:**
  ```bash
  ls -la target/debug/tasty-plugin-<name> \
         target/debug/builtin-plugins/<manifest-id>/tasty-plugin-<name>
  ```
  **이 확인이 무엇을 답하고 무엇을 못 답하는지 갈라 읽어라.** 스테이징 판정은 2026-09-07 부터 **내용**이다 — 내용이 다르면 옮기고 같으면 안 옮기며, **시각이 같아도 내용을 본다.** 그래서 위 세 열의 mtime 으로 "반영됐나" 를 묻던 절차는 더 이상 필요하지 않다: 빌드 산출물이 갱신돼 있으면 다음 부팅이 옮긴다.
  ★ 그러나 **오진의 양방향성이 다 닫힌 것은 아니다.** 닫힌 것은 **스테이징 단계**의 한 갈래(시각이 같아 조용히 건너뛰던 것)뿐이고, 이 항목의 본래 함정 — `cargo build` 가 plugin 바이너리를 **애초에 다시 만들지 않는 것** — 은 그대로다. 안 만들어진 바이너리는 내용도 옛것이므로 스테이징이 옳게 동작해도 옛 코드가 간다. 그러니 위 `ls` 는 "스테이징이 반영했나" 가 아니라 **"빌드가 산출물을 다시 만들었나"** 를 보는 데 쓴다(산출물의 mtime 이 소스보다 뒤인가). 검증 절차 쪽 서술은 [`docs/ai-verification/screenshot-methods.md`](docs/ai-verification/screenshot-methods.md).
- **workspace exclude 크레이트는 위 명령이 보지 않는다**: `crates/tasty-plugin-sdk-wasm/` 은 `--manifest-path` 를 명시해 따로 검사한다. pre-commit A.2 가 그 디렉토리의 `.rs` 가 staged 됐을 때 fmt 검사를 자동 실행한다. `site/`(공개 사이트)는 Rust 가 아니라 Astro 앱이라 cargo 명령의 대상이 아니다 — `site/**` 를 담은 main push 는 `pages.yml` 이 `npm run build` 로 빌드·배포한다(조건과 범위는 [`docs/dev-guide/ci-gates.md`](docs/dev-guide/ci-gates.md), 구조는 [`docs/dev-guide/site.md`](docs/dev-guide/site.md)).
- **의존성 설치 스텝 없음**: pnpm/npm과 달리 cargo는 별도 `install` 명령이 없다. `cargo build`/`cargo test` 등이 최초 실행 시 자동으로 fetch·컴파일한다. worktree를 새로 만든 직후 미리 받아두고 싶으면 `cargo fetch`.
- **turbo류 캐시 재생 이슈 해당 없음**: `role:conductor` 스킬의 "빌드 검증 시 캐시 무효화" 규칙은 콘텐츠 해시 기반으로 컴파일을 통째로 건너뛰는 빌드 시스템(turbo 등)을 겨냥한다. cargo의 기본 incremental build는 변경분을 실제로 재컴파일하므로 이 프로젝트에서는 `--force` 류의 캐시 무효화 플래그가 불필요하다.
- **실행 시나리오 검증(Gate 5)**: 빌드/테스트 통과만으로 "동작 확인"으로 보고하지 않는다. `cargo run` 기반 debug 인스턴스로 실제 시나리오를 재현하는 방법은 [`docs/dev-guide/self-verification.md`](docs/dev-guide/self-verification.md) 참조 — child에게 검증을 맡길 때 이 문서의 절차를 prompt에 포함한다.

# 코드 정책

## 언어·툴

- 언어: Rust (edition 2024)
- 빌드: cargo, 포맷: rustfmt, 린트: clippy

## 길이 타입 (필수)

내부 소스에서 길이 값을 단순 `f32` 로 다루는 것은 **금지**. 반드시 `PhysicalPx` 또는 `LogicalPx` 타입을 사용한다.

- `PhysicalPx`: GPU, wgpu, winit 마우스 좌표, `Rect` 필드
- `LogicalPx`: egui UI, Theme 상수, 사이드바 너비
- 두 타입 간 직접 대입 불가. `to_physical(sf)` / `to_logical(sf)` 변환 필수. 사각형은 `PhysicalRect` / `LogicalRect` 짝으로 네 변을 한 번에 변환한다.
- **강제 수단이 둘이다**: 두 좌표계를 *섞는* 것은 컴파일러가 막고, 변환을 *빠뜨리는* 것(`.value()` 로 벗겨서 `× ppp` / `÷ scale_factor`)은 `src/dpi_conversion_guard.rs` 가 막는다. 타입만으로는 후자가 안 잡힌다.

상세: [`docs/concepts/typed-length.md`](docs/concepts/typed-length.md).

## UI 디자인 (필수)

모든 색상·폰트 크기·선 굵기·간격은 `Theme` 에서 가져온다. `from_rgb(...)` 등으로 하드코딩하지 않는다.

핵심 정책 (4px 그리드, 14px 폰트 상한, 1px 보더, 호버/액티브 오버레이 자동 도출, 4.5:1 대비, 터미널 콘텐츠 애니메이션 0ms): [`docs/design/systems/theme.md`](docs/design/systems/theme.md) 의 "UI 디자인 규칙" 섹션.

## 갤러리 완전성 · gallery-first (필수)

**갤러리(`crates/tasty-gallery`)는 본체의 모든 UI 컴포넌트를 노출한다 — cut 금지.** 디자인 산출물이 일부 컴포넌트를 카탈로그에서 생략해도 갤러리에서 빼지 않는다(생략은 디자인 측 결함 → 디자인 request 로 보강). **새 modal/popup/공용 위젯은 gallery-first** — 디자인 수령 → 갤러리 specimen → 본체 반영 순서로 만든다.

상세·근거: [`docs/adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md`](docs/adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) · [`docs/dev-guide/gallery-first.md`](docs/dev-guide/gallery-first.md) · [`docs/design/policies/gallery-completeness.md`](docs/design/policies/gallery-completeness.md).

**UI 를 디자인에 정합시킬 때는 두 축을 함께 충족한다 — 둘 다 필수다:**

1. **구조 축** — [`docs/design/systems/design-parity-notes.md`](docs/design/systems/design-parity-notes.md) 의 **구조 전사(structural transcription)** 원칙과 [`docs/design/systems/design-gallery-mapping.md`](docs/design/systems/design-gallery-mapping.md)(jsx↔함수 매핑)을 읽는다 — egui flow 로 눈대중 흉내 내지 말고 디자인의 레이아웃 구조(grid·컬럼·패딩·정렬)를 **컴포넌트 단위·소스코드 단위로** 1:1 전사한다.
2. **토큰 축** — 위 "UI 디자인 (필수)" 의 [`theme.md` "UI 디자인 규칙"](docs/design/systems/theme.md) 을 **반드시 함께** 적용한다: 색·폰트크기·선굵기·간격은 전부 디자인 토큰(=`Theme`)에서 가져오고 raw px·`from_rgb` 하드코딩 금지, 4px 그리드·14px 폰트 상한·1px 보더.

구조만 맞추고 토큰을 빠뜨리거나 그 반대면 정합이 절반만 된다. 두 축은 독립적으로 어긋날 수 있으므로 매 작업에서 둘 다 점검한다.

## 국제화 (필수)

모든 UI 문자열은 `t()` 함수를 통한 번역 키로 노출한다. 자연어 하드코딩 금지. 새 문자열 추가 시 `lang/{en,ko,ja}.toml` 세 파일에 모두 키 추가.

상세 (API, lang 파일 위치, plugin 네임스페이스, 하드코딩 허용 예외): [`docs/dev-guide/i18n.md`](docs/dev-guide/i18n.md).

## 단축키 (필수)

**tasty 의 모든 단축키는 `KeybindingSettings` 로 노출되며, 코드에 하드코딩되어서는 안 된다.** macOS NSMenu / Windows AcceleratorTable / Linux Wayland 같은 OS 메뉴 측 key equivalent 도 `KeybindingSettings` 의 대응 binding 값을 따라가야 한다.

예외 (수정 불가능한 단축키) — **OS 자체가 박아두어 tasty 가 무력화 / 덮어쓰기 / 가로채기 모두 불가능한 단축키**는 그대로 둔다 (예: macOS Spotlight `Cmd+Space`, OS 전역 윈도우 전환 등). 이 케이스는 애초에 tasty 가 등록할 수도 끌 수도 없는 것이므로 정책의 범위 밖이다.

반대로, **tasty 가 직접 NSMenu / AcceleratorTable 등에 등록하는 모든 메뉴 항목의 key equivalent 는 — `KeybindingSettings` 의 binding 에서 가져올 수 있으면 가져오고, 가져올 수 없으면 비운다.** selector 가 OS 표준 (`cut:` / `performClose:` / `miniaturize:` / `hide:` 등) 이라는 사실은 단축키 하드코딩의 정당화가 되지 않는다. tasty 가 winit 의 `with_default_menu(false)` 로 NSMenu 를 직접 소유한 시점부터 모든 NSMenu 항목의 key equivalent 는 tasty 의 선택이며, 정책은 "Settings 연동 또는 빈 값" 둘 중 하나만 허용한다. selector 와 단축키는 독립적으로 결정한다 — selector 는 OS 표준을 그대로 써도 되지만, 같은 항목의 key equivalent 까지 OS 컨벤션 단축키로 박는 것은 금지.

tasty 특화 액션 (예: `tastyQuit:` / `tastyNewWindow:` / split / convert 등) 은 **반드시** `KeybindingSettings` 의 대응 필드를 읽어 key equivalent 를 설정해야 한다. binding 이 빈 vec 이면 key equivalent 도 비워두어 단축키 없는 메뉴 항목으로 표시한다.

상세 (modifier 매핑 규칙, 위치 기반 추상화): [`docs/design/policies/key-mapping.md`](docs/design/policies/key-mapping.md).

## 에러 처리 (필수)

`Result` 를 `let _ =` 로 무시하지 않는다. 에러는 처리하거나 `tracing::warn!` / `tracing::error!` 로 로그를 남긴다.

상세 (로그 레벨 선택, 의도적 무시 시 주석 규칙): [`docs/dev-guide/error-handling.md`](docs/dev-guide/error-handling.md).

## 소스 주석의 TODO 파일 및 디자인 changelog 인용 금지 (필수)

**상위 규칙**: git 이 추적하는 파일에는 git 에 존재하지 않는 경로를 적지 않는다 — `.gitignore` 로 제외된 레포 로컬 작업 폴더는 경로도, 폴더 이름 단독 언급도 대상이다. 그 위치는 커밋되지 않는 로컬 전용 지침이 정하고, 추적 문서는 규칙의 내용만 쓴다. 범위 밖(적어도 되는 것)은 빌드가 만들어내는 산출물 경로, 사용자 홈의 런타임 경로, 경로가 아닌 식별자, `.gitignore` 자신이다. 근거·대안·재검토 조건은 [`docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md`](docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md). 아래는 그 규칙이 가장 자주 깨지는 두 형태를 구체화한 것이다.

`.gitignore` 대상인 로컬 작업 폴더의 TODO 티켓 파일(conductor 티켓 포함)은 git에 커밋되지 않고, 완료된 항목은 관례상 파일 자체가 삭제된다 — **로컬 세션에서만 유효한 휘발성 식별자**다. 소스 코드 주석·문자열(UI에 노출되는 텍스트 포함)에서 그 파일 번호를 "TODO 40", "(TODO18)" 같은 형태로 인용하지 않는다. **표기의 대소문자는 판정에 영향을 주지 않는다** — 소문자 "todo40" 도 같은 죽은 참조다. 저장소를 새로 clone한 사람에게는 그 번호가 가리키는 문서가 존재한 적이 없으므로, 인용 자체가 추적 불가능한 죽은 참조가 된다.

Claude Design(claude.ai/design) 프로젝트의 **changelog**도 동일하게 금지 대상이다 — changelog는 `.gitignore` 대상 로컬 디렉토리보다 한층 더 휘발적이다: 원격 Claude Design 프로젝트 **내부에만** 존재하며 로컬 파일시스템에는 애초에 흔적조차 남지 않는다. "2026-07-03-spacing-offgrid" 같은 changelog 판정 slug를 소스 주석·문자열에서 인용하지 않는다 — TODO 파일 번호와 완전히 같은 이유(추적 불가능한 죽은 참조)다.

근거가 필요하면 다음 중 하나를 쓴다(TODO/changelog 어느 쪽 인용을 대체하든 동일하게 적용):
- **이유가 자명하면**: 번호/slug 대신 이유를 주석에 직접 서술한다.
- **설계 결정이 크면**: 커밋되는 [`docs/adr/`](docs/adr/) ADR을 작성하고 그 경로를 인용한다.
- **기능 동작을 설명해야 하면**: 커밋되는 [`docs/`](docs/) 문서(예: `docs/dev-guide/`, `docs/features/`, `docs/plugins/`)를 참조하거나 신설해 그 경로를 인용한다.

상위 규칙과 위 두 형태를 함께 `crates/tasty-doc-guards/tests/no_todo_file_citation.rs` 가 강제한다 — 그 타깃은 `doc-guards.yml` 이 main push · PR 마다 **자동으로 실행한다**(경로 필터가 없어 문서만 바뀐 push 에서도 돈다). 다만 자동 잡은 push 된 커밋만 본다([`docs/dev-guide/ci-gates.md`](docs/dev-guide/ci-gates.md)). 즉 커밋 전에 직접 돌려야 잡힌다. 번호 인용(P1 — 대소문자 무시, 구분자는 공백 런·`/`·짝 맞는 괄호)·conductor 번호(P2)·경로 인용(P3)·changelog slug(P4)·앵커 슬러그 번호(P5)·로컬 작업 폴더 언급(P6)·산문 언급(P7)·작업 분할 번호(P8)·작업 계획 좌표(P9)·회차 번호(P10) 열 형태를 모두 잡는다. P7 은 번호가 안 붙은 `TODO` 다 — 번호가 없어도 가리키는 대상이 커밋되지 않는 로컬 문서면 똑같이 죽은 참조다. **할 일 표시는 허용한다**(대문자 `TODO` 바로 뒤에 `:` 또는 `(<범위>):` 가 오는 형태 — `// TODO: 나중에` · `// TODO(권한모델): 도입 후 대체`). 소문자 `todo:` 표시도 그대로 통과한다 — P7 은 소문자를 보지 않고, P1 은 숫자가 뒤따를 때만 잡는다. P8 은 괄호로 감싼 두 자리 일련번호다 — 기능 하나를 여러 단계로 쪼갠 작업 계획의 "몇 번째 항목" 이고, 그 계획은 커밋되지 않는다. 실측 2026-09-14 에 180 자리가 있었고 그중 한 갈래는 대응 항목이 문서에 아예 없었다. 번호만 홀로 서서 `TODO` 라는 낱말이 없으므로 P1 로는 안 잡힌다. 앞자리가 0 인 두 자리만 보므로 네 자리 ADR 좌표나 한 자리 수는 걸리지 않고, 괄호 없이 문장에 녹은 맨번호도 안 본다 — 날짜·버전·측정값과 가를 표지가 없어 세면 오탐이 본문을 덮는다. 그쪽은 사람이 문장을 다시 써야 하고 이 가드는 다시 스며드는 입구만 막는다. P9 는 점으로 이은 작업 계획 좌표이고(`<대문자>.<숫자>.<대문자>.<대문자>` 네 마디로 집는다), P10 은 `R` 에 두 자리 이상 숫자가 붙은 회차 번호다. 둘 다 같은 뿌리다 — 2026-05 의 커밋 제목이 그 좌표를 달고 있었고, 그것을 정의한 문서는 레포에 한 곳도 없다. 실측 2026-09-14 에 P9 가 53 자리·P10 이 414 자리였다. 절반 가까이는 괄호 하나로 문장 끝에 달려 근거를 대신하고 있었으므로, 처방은 번호를 빼는 것이 아니라 **번호가 대신하던 명제를 그 자리에 적는 것**이다. P10 이 한 자리 `R1` 을 안 보는 이유는 P8 이 한 자리 수를 안 보는 것과 같다 — 좌표축·순번 쪽이 압도적이다. **문서(`.md`)는 P7 범위 밖이다** — 규칙 본문을 인용하고 상류 라이브러리가 자기 소스에 남긴 마커를 서술하는 것이 문서의 일이라, 산문 언급이 거기서는 정상이다. 스캔 대상은 레포 전체 파일이다(스크립트·CI 설정·루트 문서 포함) — 바이너리 확장자, 빌드 산출물, gitignored 로컬 폴더, vendored `assets/` 만 뺀다. 금지 형태를 담는 것이 본질인 파일(규칙 본문 등)은 그 테스트의 `ALLOWLIST` 에 **(경로, 허용 패턴)** 으로 등록한다 — 파일 통째가 아니라 패턴 단위로 면제해, 그 파일이 다른 형태의 위반을 새로 들이면 그건 잡히게 한다.

로컬 작업을 추적할 목적 자체는 유효하다 — 로컬 작업 폴더에 번호 붙은 TODO 파일을 쓰는 관례는 그대로 유지한다(폴더 위치는 위 "임시 파일·계획 위치" 와 같이 로컬 전용 지침이 정한다). 다만 그 번호는 **작업 티켓**일 뿐 **영구 코드 근거 좌표**가 아니므로, 소스에 스며들게 하지 않는다.

# 작업 시 참고 문서

## 개발 가이드

전체 목록은 [`docs/dev-guide/index.md`](docs/dev-guide/index.md). 자주 참조하는 항목:

- [`docs/dev-guide/self-verification.md`](docs/dev-guide/self-verification.md) — **커밋 전에 직접 검증.** 사용자에게 검증을 떠넘기지 않는다.
- [`docs/dev-guide/build.md`](docs/dev-guide/build.md) — 워크스페이스 구조, 빌드 프로필
- [`docs/dev-guide/popup-implementation.md`](docs/dev-guide/popup-implementation.md) — Popup 구현 (`PopupDef` 시스템, `egui::Window` 직접 사용 금지)
- [`docs/dev-guide/debug-ipc.md`](docs/dev-guide/debug-ipc.md) — debug 빌드 전용 IPC + 격리 정책
- [`docs/dev-guide/model-view-split.md`](docs/dev-guide/model-view-split.md) — Model + Host View 분리 패턴
- [`docs/dev-guide/gpu-rendering.md`](docs/dev-guide/gpu-rendering.md) — GPU 렌더링 구조
- [`docs/dev-guide/agent-runner.md`](docs/dev-guide/agent-runner.md) — **Task DAG executor.** 여러 AI 에이전트가 같은 tasty 인스턴스를 공유할 때 쓰는 협업 primitive 6종(`agent.*`: task DAG · barrier · semaphore · lease · reducer · rate-limit). 기획/인터페이스는 [`docs/features/agent-collaboration/index.md`](docs/features/agent-collaboration/index.md)
- [`docs/dev-guide/plugin-development.md`](docs/dev-guide/plugin-development.md), [`plugin-permissions.md`](docs/dev-guide/plugin-permissions.md), [`plugin-sensitive-data.md`](docs/dev-guide/plugin-sensitive-data.md) — Plugin 제작. **번들 plugin 코드를 고친 뒤 실행 중 tasty 인스턴스에 재빌드·재시작 없이 반영할 때도 이 문서 §9.1** (빌드 → 재서명 → `disable` → `upgrade-builtins` → `enable` 순서) — "새 plugin 만들기"가 아니라도 반드시 먼저 확인한다.

## 자체 검증

UI / 렌더링 / 환경별 검증 시에는 [`docs/ai-verification/`](docs/ai-verification/) 의 항목별 문서를 반드시 확인. 전체 목록은 [`docs/ai-verification/index.md`](docs/ai-verification/index.md).

UI 변경 시 [`docs/ai-verification/visual-verification.md`](docs/ai-verification/visual-verification.md) 의 체크리스트 + 스크린샷 판단 휴리스틱을 따른다.

## 디자인 / 아키텍처

[`docs/design/`](docs/design/), [`docs/architecture/`](docs/architecture/). 전체 목록은 `docs/index.md`.
