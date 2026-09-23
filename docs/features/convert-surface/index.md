# Surface 타입 전환 (Convert surface)

- **Status**: Implemented
- **주체**: 로컬 사용자 (`Alt+'` 팝업) · AI Agent (kind 별 IPC — 범용 convert 없음)
- **ADR**: [ADR-0043](../../adr/0043-convert-input-popup-capability.md) (파일 입력이 필요한 kind 의 convert 라우팅 capability)
- **코드**: `ConvertSurface` intent (`src/intent/surface.rs`), 팝업 `src/adapters/ui/popup/convert.rs`
- **화면**: convert 팝업 (`PopupScope::Surface`)

## 목적

열려 있는 surface 의 *종류*(Terminal/Markdown/Explorer/Image/…)를 그 자리에서 교체한다. 새 탭을 만들지 않고 현재 위치·레이아웃을 유지한 채 콘텐츠 유형만 바꾼다.

## 내부 동작

### 사용자 트리거

- `convert_surface` 단축키(기본 `Alt+'`) → Surface 스코프 팝업. Empty surface 중앙의 타입 전환 버튼도 동일 팝업. `convert_to_markdown` 직접 전환 단축키(기본 없음).
- 팝업 항목은 `SurfaceKindRegistry` 에서 **동적 enumerate**(빌트인 + plugin 제공 kind). `empty` 등 시스템 kind 는 `HIDDEN_KINDS` 로 제외. 현재 타입과 같은 항목은 체크 + 비활성. `dag_graph`([화면](../agent-collaboration/screens/dag-graph-surface.md))는 파일 입력이 없어 빈 params 즉시 변환 경로를 탄다 — 별도 분기가 없다.
- 키보드 탐색(Up/Down+Enter), kind 첫 글자 즉시 선택. **convert 라우팅은 registry capability 로 판정**(host 는 kind 이름을 하드코딩하지 않는다, [ADR-0043](../../adr/0043-convert-input-popup-capability.md)): `terminal` 은 host PTY spawn 전용 경로, `convert_requires_input` capability 를 선언한 kind(예: markdown)는 그 kind 소유 plugin 의 파일 입력 팝업(`convert_input_popup`)을 먼저 열고(제자리 변환은 context 에 `surface_id` 실림), 그 외 kind 는 빈 params 로 즉시 변환.
- **개별 surface 교체 원칙**: 대상 surface 구현체만 교체, 탭 레이아웃·다른 surface 무영향.
- **cwd carry**: 터미널에서 `cd /foo/bar` 후 Explorer 로 전환하면 Explorer 루트는 `/foo/bar`(호스트 시작 cwd 로 fallback 안 함) — [surface-cwd invariant](../../design/policies/cwd.md#surface-cwd-invariant). mirror(원격 attach) 워크스페이스에서도 동일하다 — convert 가 원격으로 forward 될 때 cwd 가 함께 전달되고, 전달값이 없으면 원격이 대상 surface 의 실제 PTY 에서 직접 resolve 한다([§3-1](../../design/policies/cwd.md#3-1-mirror원격-attach-forward-경로도-같은-불변식-대상)).

### 에이전트

- **범용 `surface.convert` IPC/CLI 는 없다.** 전환 팝업은 사용자 단축키 전용 UI — release 에는 팝업을 여는 경로가 없고, debug 는 `debug.host_popup.open` 으로만 연다(사용자 입력 재현은 debug 격리).
- kind 별 IPC 가 내부적으로 같은 메커니즘(`DomainIntent::ConvertSurface`)을 쓴다 — 예: `image.open{surface_id, path}` 가 대상을 image kind 로 전환.
- 새 타입 surface 가 필요하면 에이전트는 `tab.create`/`split` 의 `type` 으로 **새로 생성**하는 게 기본 경로.

## 비-목표

- 전환 전 surface 상태의 보존/병합 — 기존 구현체는 메모리에서 해제(터미널→다른 kind→터미널 되돌려도 이전 셸 세션 복귀 안 됨).
- 전환 시 포커스 이동(포커스 독립) · release 팝업의 에이전트 트리거.

## 관련

- [work-area](../work-area/index.md)(Surface kind) · [surface-cwd invariant](../../design/policies/cwd.md#surface-cwd-invariant) · [popup-implementation](../../dev-guide/popup-implementation.md)
