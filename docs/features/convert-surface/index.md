# Surface 타입 전환 (Convert surface)

- **Status**: Implemented
- **주체**: 로컬 사용자 (`Alt+'` 팝업) · AI Agent (kind 별 IPC — 범용 convert 없음)
- **설계**: [파일 핸들러 (File handler)](../file-handler/index.md) (파일 입력이 필요한 kind 의 convert 라우팅 capability)
- **코드**: `ConvertSurface` intent (`src/app/creation_intent.rs`), 팝업 `src/adapters/ui/popup/convert.rs`
- **화면**: convert 팝업 (`PopupScope::Surface`)

## 목적

열려 있는 surface 의 *종류*(Terminal/Markdown/Explorer/Image/…)를 그 자리에서 교체한다. 새 탭을 만들지 않고 현재 위치·레이아웃을 유지한 채 콘텐츠 유형만 바꾼다.

## 내부 동작

### 사용자 트리거

- `convert_surface` 단축키(기본 `Alt+'`) → Surface 스코프 팝업. Empty surface 중앙의 타입 전환 버튼도 동일 팝업. `convert_to_markdown` 직접 전환 단축키(기본 없음).
- 팝업 항목은 `SurfaceKindRegistry` 에서 **동적으로 조회**(빌트인 + plugin 제공 kind). `empty` 등 시스템 kind 는 `HIDDEN_KINDS` 로 제외. 대상 surface 의 현재 타입은 바꿀 대상이 아니라 목록에서 뺀다.
- 팝업은 작은 목록 popup 이다 — 공용 타이틀바(× 만) 아래에 kind 마다 공용 `MenuItem` 한 행(registry 아이콘 · 라벨 · 단축키 키캡). 단축키 글자는 kind id 첫 영문 글자의 대문자라 번역된 라벨과 무관하므로, 행 끝에 공용 `Kbd` 키캡(`menu_item_kbd`, 명령 팔레트와 같은 키캡)으로 보여 준다. 키캡은 표시 전용이고 기본·호버·선택 행에서 모양이 같다. 단축키가 없는 행은 키캡도 비워 둔 자리도 없이 라벨이 행 끝까지 가며, 라벨이 길면 라벨만 말줄임한다. 단축키는 현재 kind 를 뺀 목록에서 앞 항목부터 배정한다. 첫 글자가 같은 kind 중 앞선 것이 현재 kind 이면 뒤의 kind 가 그 글자를 받고, 목록 안에서 첫 글자가 겹치면 뒤 항목은 단축키가 없다. 폭은 `convert-popup-width`(240)에 UI 배율을 곱한 값이고, 높이는 타이틀바 + 콘텐츠 여백 + 행 수 × MenuItem 높이다. 키보드로 고른 행은 `surface-active` 배경이다. `dag_graph`([화면](../../surfaces/dag-graph/screens/graph.md))는 파일 입력이 없어 빈 params로 즉시 변환한다 — 별도 분기가 없다.
- surface 가 팝업보다 좁거나 낮으면 surface 안의 "팝업 보기" 버튼으로 접힌다. 버튼(키 포커스가 있으면 Enter·Space)을 누르면 창 위로 펼치고, Esc 나 제목줄의 접기 버튼으로 다시 접는다. 접거나 펼쳐도 팝업은 닫히지 않는다([좁은 범위](../../design/systems/popup.md#좁은-범위-접기와-펼침)).
- 팝업은 surface 마다 따로 열린다. 대상 surface 와 키보드 선택은 그 팝업 인스턴스가 가지므로 두 surface 에 연 팝업은 서로의 대상·선택을 바꾸지 않는다. 같은 surface 에서 다시 열면 열린 팝업의 선택만 처음으로 돌아간다.
- 키보드 탐색(Up/Down+Enter), kind 첫 글자 즉시 선택. **convert 라우팅은 registry capability 로 판정**(host 는 kind 이름을 하드코딩하지 않는다, [파일 핸들러 (File handler)](../file-handler/index.md)): `terminal` 은 host PTY spawn 전용 경로, `convert_requires_input` capability 를 선언한 kind(예: markdown)는 그 kind 소유 plugin 의 파일 입력 팝업(`convert_input_popup`)을 먼저 열고(제자리 변환은 context 에 `surface_id` 실림), 그 외 kind 는 빈 params 로 즉시 변환.
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
