# 작업 규칙

이 저장소에서 코드나 문서를 고치는 사람과 에이전트가 따르는 규칙이다. 루트 `CLAUDE.md`가 이 문서를 가리킨다. 본문의 경로는 checkout 루트 기준이다.

## Tasty

cmux(macOS 전용)에서 영감을 받은 크로스 플랫폼 GPU 가속 터미널 에뮬레이터다.
Rust 기반 네이티브 GUI 앱으로 Windows, macOS, Linux를 모두 지원한다.
제작자의 입맛에 맞춘 터미널이며 AI 에이전트 협업은 편의를 위해 더하는 기능이다.

레포 주소와 라이선스는 `README.md`, 기술 스택 표는 `docs/architecture/index.md` "기술 스택"에 있다.

## 핵심 원칙

아래 세 항목을 지킨다. 문서·ADR을 어디에 쓰는지는 `docs/documentation-model.md` "작업을 끝낼 때"를 따른다.

- 에이전트는 터미널 작업을 수행할 수 있다. release에서는 사용자 포커스를 빼앗는 동작을 제공하지 않고 debug에서는 사용자 조작 재현이 가능해야 한다. debug 핸들러는 모듈 선언에 cfg(debug_assertions)를 둔 별도 파일로 분리한다([debug IPC](debug-ipc.md)).
- 에이전트가 자기 작업에 사용하는 기능은 IPC·CLI 양쪽에서 제공한다. 대상은 ID로 지정하며 현재 선택·스크롤·히스토리 보존 계약은 [포커스 정책](../design/policies/focus.md)에 있다.
- Windows·macOS(Apple Silicon)·Linux를 1급 지원한다. Windows ARM도 릴리스 대상이다. 검증 여건과 지원 등급을 구분하며 OS별 코드는 cfg로 나누어 다른 OS의 컴파일을 깨뜨리지 않는다. 원인이 특정 OS의 라이브러리·API 제약에만 있으면 수정도 `#[cfg(target_os = "…")]`로 그 OS에만 적용하고, 다른 OS의 동작을 검증 없이 바꾸지 않는다.

## 시작할 때

1. `docs/identity.md`와 `docs/principles/index.md`(제품 원칙)를 먼저 읽는다.
2. 용어집 `docs/concepts/ubiquitous-language.md`의 "용어 한 줄 정의" 아래 "구조"와 "사용자 화면 표기" 절을 읽는다. 용어를 잘못 쓰면 코드와 문서의 일관성이 깨진다. 특히 Window / Pane / Tab / Surface 계층, 상위/하위 레이아웃 구분, Modal / Popup / Toast 구분을 확인한다.
3. 작업 영역의 가이드를 읽는다. `docs/index.md`의 "작업 전 필독" 표와 카테고리 표에서 영역 인덱스를 찾고 그 영역 가이드로 간다. 개발 가이드 전체 목록은 `docs/dev-guide/index.md`다. 용어집은 작업 영역에 해당하는 절(원격 연결, View 내부, Surface 종류, Claude plugin, attach, CLI 명령 갈래, 용어와 코드 심볼의 대응)을 더 읽는다.

## 언제 무엇을 읽나

이번 변경에 해당하는 행만 읽는다. 작업 범위가 바뀌면 표를 다시 본다.

| 언제 | 읽을 곳 |
|---|---|
| 작업을 시작할 때 (공통) | 이 문서 "시작할 때" 1·2: `docs/identity.md`, `docs/principles/index.md`, 용어집 `docs/concepts/ubiquitous-language.md`의 "구조"·"사용자 화면 표기" 절 |
| 작업을 시작할 때 (영역) | 이 문서 "시작할 때" 3: `docs/index.md` "작업 전 필독" 표와 카테고리 표에서 영역 가이드, 개발 가이드 전체 목록 `docs/dev-guide/index.md`, 용어집의 영역 절 |
| 길이·좌표 값을 다루는 코드를 고치기 전 | `docs/concepts/typed-length.md` |
| UI(색·크기·간격·레이아웃)를 고치기 전 | `docs/design/systems/theme.md` "UI 디자인 규칙", `docs/dev-guide/gallery-first.md`, `docs/dev-guide/design-change-workflow.md` |
| UI를 디자인에 맞추기 전 | `docs/dev-guide/gallery-first.md` "디자인에 맞출 때" |
| 갤러리 항목을 더하거나 지우기 전, 디자인 카탈로그에 없는 컴포넌트를 다룰 때 | `docs/design/policies/gallery-completeness.md` |
| UI 문자열이나 CLI 도움말을 더하거나 고치기 전 | `docs/dev-guide/i18n.md` |
| 단축키·메뉴를 고치기 전 | `docs/design/policies/key-mapping.md` |
| Popup을 만들거나 기존 Popup을 고치기 전 | `docs/dev-guide/popup-implementation.md`(`PopupDef` 시스템, `egui::Window` 직접 사용 금지) |
| debug 전용 IPC를 더하기 전 | `docs/dev-guide/debug-ipc.md`(debug 빌드 전용 IPC와 격리 정책) |
| Model·View를 나누는 코드를 고치기 전 | `docs/dev-guide/model-view-split.md` |
| GPU 렌더링을 고치기 전 | `docs/dev-guide/gpu-rendering.md` |
| agent 협업 기능을 고치기 전 | `docs/dev-guide/agent-runner.md`(작업 DAG 실행과 `agent.*` 협업 기능 6종: task DAG · barrier · semaphore · lease · reducer · rate-limit). 기능과 인터페이스는 `docs/features/agent-collaboration/index.md` |
| Result를 다루거나 로그를 남기기 전 | `docs/dev-guide/error-handling.md` |
| 기능을 없애거나 저장 형식을 바꾸기 전 | 이 문서 "1.0 전 저장 형식" |
| 플러그인을 만들거나 권한·민감 데이터를 다루기 전 | `docs/dev-guide/plugin-development.md`(민감 데이터는 "민감 데이터 — regular · secret · keyring 선택" 절), `docs/dev-guide/plugin-permissions.md` |
| 번들 plugin 코드를 고친 뒤 실행 중 인스턴스에 반영하기 전(새 plugin을 만들지 않아도) | `docs/dev-guide/plugin-development.md` §9.1. 실행 중 프로필 확인 → `disable` → 그 프로필로 `just build-plugins` → `upgrade-builtins` → `enable` 순서다. 공식 플러그인 소스를 고쳤다면 `--from-checkout`으로 공식 플러그인 레포 체크아웃에서 빌드하고, mesh-demo만 고쳤다면 옵션 없이 실행한다. 서명은 release/dist 프로필에서만 붙는다 |
| 빌드 프로필을 고르거나 빌드·검사를 돌리기 전 | `docs/dev-guide/build.md` "빌드 프로필 (3종)"(자체 검증에 `dist`를 쓰지 않는다)·"검증 명령"·"개발 번들"·"도구", `docs/dev-guide/ci-gates.md` |
| 변경을 검증하기 시작할 때(첫 실행 검증·시나리오 준비 전) | `docs/dev-guide/self-verification.md` "원칙"(검증 가능 여부 판단, 격리 실행). 빌드·테스트 통과와 실제 실행 확인은 구분하고, 격리된 debug 인스턴스로 시나리오를 재현한다 |
| UI·렌더링·OS·환경별 동작을 검증하기 전 | `docs/ai-verification/index.md`의 해당 항목 문서. UI 변경이면 `docs/ai-verification/screenshot-methods.md` "시각 판정 체크리스트"(체크리스트와 스크린샷 판단 휴리스틱) |
| 커밋하기 전 | `docs/dev-guide/self-verification.md`(모든 변경을 커밋 전에 직접 확인하고 사용자에게 검증을 떠넘기지 않는다), `docs/dev-guide/commit-convention.md` "커밋 정책" |
| 사용자가 빌드를 요청했을 때, 버전 줄·번들 플러그인·그 normal/build·path 의존성을 바꾼 커밋 전, 나눠 push하기 전(배포 여부와 관계없이 develop push 포함) | `docs/dev-guide/release.md` "버전 정책" |
| 같은 사실(이름·수치·목록)을 코드·설정·문서에서 고치기 전 | `docs/documentation-model.md` "작성 규칙 요약", `docs/dev-guide/duplicated-sets.md` |
| ADR을 쓰기 전 | `docs/adr/template.md`, `docs/documentation-model.md` "작업을 끝낼 때"의 가이드·ADR 구분 |
| 주석·문서를 쓰기 전, 작업을 끝낼 때 | `docs/documentation-model.md` "작업을 끝낼 때", "경로와 코드 인용" |

## 임시 파일과 계획

- 임시 파일(스크린샷·디버그 스크립트·테스트 출력)과 구현 계획은 로컬 지침이 지정한 Git 제외 폴더에 둔다. 프로젝트 루트나 소스 디렉터리에 만들지 않는다. 로컬 전용 경로는 추적 문서에 적지 않는다.
- 임시 파일은 작업 후, 계획은 구현 완료 후 정리한다.
- `docs/`에는 현재 설계와 동작을 기록한다. 진행 중인 계획과 변경 일기는 넣지 않는다.

## 1.0 전 저장 형식

1.0 전에는 기능을 없애거나 저장 형식을 바꿀 때 옛 저장값을 변환하거나 안내하는 코드를 새로 만들지 않는다. 그런 코드를 둘지 결정할 거리로도 올리지 않는다.
