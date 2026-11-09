# Tasty 문서

크로스 플랫폼 GPU 가속 네이티브 터미널 에뮬레이터의 설계·명세·개발 문서. 이 인덱스가 진입점이다.

문서의 위치와 작성 방법은 [문서 작성 규칙](documentation-model.md)을 따른다.

## 작업 전 필독

| 문서 | 설명 |
|------|------|
| [identity.md](identity.md) | **Tasty 정체성** — 목적과 범위, 적용 대상, 판단 기준. 판단 기준인 제품 원칙은 [principles](principles/index.md)에 있다. 설계·구현 전에 둘 다 읽는다 |
| [concepts/actors.md](concepts/actors.md) | **주체(Actors)** — 로컬 사용자·에이전트·원격 사용자와 점유의 정의 |
| [concepts/hierarchy.md](concepts/hierarchy.md) | **구조 계층** — Window/View › Workspace › Pane › Tab › Surface + 두 레벨 레이아웃 |
| [concepts/plugins.md](concepts/plugins.md) | **플러그인** — 배포/통합 축, surface_kind 렌더 분기, 권한 |
| [concepts/ubiquitous-language.md](concepts/ubiquitous-language.md) | **통합 용어집** — 주요 용어와 상세 설명 링크, tmux/iTerm2 및 코드 심볼과의 대응 |
| [concepts/typed-length.md](concepts/typed-length.md) | **타입 있는 길이** — `PhysicalPx`/`LogicalPx` newtype (DPI 혼동 컴파일 차단) |
| [documentation-model.md](documentation-model.md) | **문서 모델** — 문서 분류와 기능·화면 설명의 분리 기준. 새 문서를 쓰기 전에 읽는다 |

## 문서

| 카테고리 | 진입 | 설명 |
|----------|------|------|
| 개념 (concepts) | [concepts/index.md](concepts/index.md) | 공통 용어와 객체 관계 |
| 기능 (기획·화면) | [features/index.md](features/index.md) | 제품 기능과 화면 사용법 |
| Surface 종류 | [surfaces/index.md](surfaces/index.md) | kind별 명세(terminal · empty · explorer · dag_graph · markdown · image · html). 제공 주체와 관계없이 여기서 찾는다 |
| 번들 플러그인 | [plugins/index.md](plugins/index.md) | 번들 플러그인 카탈로그. surface 를 제공하지 않는 플러그인의 기능 문서 |
| 설계 (design) | [design/index.md](design/index.md) | 정책·시스템·흐름 |
| 레퍼런스 (조회) | [reference/index.md](reference/index.md) | API·이벤트·출력 형식 |
| 개발 가이드 | [dev-guide/index.md](dev-guide/index.md) | 구현·빌드·검증·배포 절차 |
| 아키텍처 | [architecture/index.md](architecture/index.md) | 모듈 구조와 데이터 흐름 |
| AI 자체 검증 | [ai-verification/index.md](ai-verification/index.md) | IPC·화면·플랫폼 검증 방법 |
| 연구 자료 | [research/index.md](research/index.md) | 제품 결정의 근거로 남기는 측정 설계·원자료·결과 |
| 제품 원칙 | [principles/index.md](principles/index.md) | 설계·개발 판단 기준과 충돌 해소 |
| 원칙 예외 ADR | [adr/index.md](adr/index.md) | 원칙의 예외·미충족을 채택한 결정 |
| 설치 | [installation.md](installation.md) | 플랫폼별 설치 방법 |
