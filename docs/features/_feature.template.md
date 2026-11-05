<!--
기능의 내부 동작을 설명하는 템플릿.
새 기능을 만들 때:  cp docs/features/_feature.template.md docs/features/<feature>/index.md
규칙: docs/documentation-model.md / docs/identity.md / docs/principles/index.md (작성 전 필독).

작성 시 지킬 것:
- 기능의 동작을 먼저 쓴다. 필요한 구현 경로는 "## 구현"에 핵심 크레이트·모듈·함수만 연결한다. 자주 바뀌는 호출부를 전부 나열하지 않는다.
- 시각 수치와 토큰은 디자인 문서로 연결하고 값을 반복해서 적지 않는다.
- 빌드/로드맵 상태(Phase·구현 예정·이관)는 적지 않는다 — 현재 상태만.
- 마크다운 체크박스(task list)를 쓰지 않는다 — Acceptance Criteria 는 평문 Given/When/Then 불릿 (documentation-model.md §6).
- 화면이 없으면(headless 전용) "## 화면" 섹션을 지운다. 구현 포인터가 불필요하면 "## 구현" 도 지운다.
- 화면이 하나이고 템플릿 절 밖의 자기 규칙이 없으면 "## 화면" 에 화면정의서를 직접 쓴다(_screen.template.md 의 절을 한 단계 내려서). 화면이 둘 이상이거나 자기 규칙이 있으면 screens/ 파일로 빼고 여기엔 목록만 둔다 — 기준은 documentation-model.md §3.
이 주석 블록은 실제 문서에서 삭제한다.
-->

# <기능 이름>

- **Status**: Implemented | Partial | Planned
- **주체**: 로컬 사용자 / AI Agent / 원격 접속 사용자 중 이 기능을 쓰는 주체 (복수 가능 — [주체](../../concepts/actors.md))
- **설계**: ADR-XXXX (있으면)
- **코드**: `src/...` / `crates/...`
- **화면**: [아래 절](#화면) 또는 `screens/<screen>.md` (없으면 "없음 — headless 전용")

## 목적

기능의 목적을 한 문단으로 쓴다.

## 내부 동작 (headless-valid)

기능이 *무엇을* 하나 — 상태·판정 규칙·흐름·예외. GUI 없이도 성립하는 동작.

## 인터페이스

- **AI Agent (IPC/CLI)**: `tasty ...` / IPC `...` — 파라미터·응답
- **사용자 트리거**: <단축키 / 클릭 / 자동> (화면이 있는 경우)
- **원격 / 점유**: (해당 시) 점유 필요 여부 등

## 비-목표 (Out of scope)

이 기능에서 지원하지 않는 범위.

## Acceptance Criteria

- Given <조건> When <행동> Then <결과>  <!-- headless(IPC/CLI)로 검증 가능한 형태 -->

## 화면

- **시각 소스**: `design-system/.../<x>.jsx` (Design System)

### 트리거

### UI 요소 인벤토리

### 상태별 시각

### 시각 소스

<!-- 파일로 뺐으면 위 대신: - `screens/<screen>.md` — <한 줄 설명> -->
