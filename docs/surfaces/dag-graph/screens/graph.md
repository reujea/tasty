# DAG 그래프 surface 화면

- **부모 기획**: [dag-graph](../index.md)
- **시각 소스**: tasty-page `vendor/ui_kits/terminal/overlays/dag_view.jsx` · `vendor/ui_kits/terminal/overlays/dag_surfaces.jsx` · `vendor/gallery/dag/index.jsx`
- **kind**: `dag_graph` (host builtin)

workspace 의 task DAG 를 노드/엣지 그래프로 **관찰**하는 화면이다. 노드 편집기가 아니다 —
연결 포트나 소켓이 없으며 노드를 드래그하거나 연결할 수 없다. runner 가 진행하는 동안 스스로
갱신되며, 사용자 조작은 pan / zoom / 선택 / 대상 DAG 전환 / 방향 전환뿐이다.

## 트리거

```bash
tasty new tab --pane <PANE> --type dag_graph
tasty split --level surface --surface <SID> --type dag_graph
```

- 변환 팝업(`Alt+'`)에도 다른 kind 와 같이 나온다 — 파일 입력이 없어 즉시 변환된다
  ([surface 변환](../../../features/convert-surface/index.md)).
- 관찰 대상과 방향을 정하는 params 는 [생성·갱신·종료](../index.md#생성갱신종료)에 있다.

## UI 요소 인벤토리

| 영역 | 요소 |
|------|------|
| 헤더 | DAG 선택(목록이 2 개 이상일 때만 드롭다운) · `완료/전체` 진척(`font-label` caption 글꼴, `dag-header-count-fg` 색. 건너뛴 task 가 있으면 뒤에 ` · {n} skipped ({k} not selected)`, 목록 행과 같은 규칙) · 러너 배지 · 재개 힌트 · 새로고침 |
| 사이클 배너 | 의존성 사이클일 때만. 사이클을 이루는 task id 나열 |
| 캔버스 | 점 격자 배경(탭 surface는 포커스 바탕 쌍, 팝업은 `dag-canvas-bg`) · 노드 카드 · 직교 엣지 · LOD 칩(좌하단) · 미니맵 + 줌 클러스터(`− % + \| fit dir`, 우하단) |
| 상세 | 선택 노드의 이름 · 닫기 · 상태 + 종류 태그 · (입력 대기면) 알림 + 세션 열기 버튼 · (알 수 없음이면) 이유 · 실패정책/시작/소요/종료코드/작업 ID · 명령/의존(개수)/에러/출력(에러·출력은 복사 버튼) |

- **노드 상태**는 색·글리프·텍스트로 함께 표시한다. 축소하면 글리프와 텍스트가
  숨겨지므로 테두리와 배경에도 상태색을 쓴다. `waiting`·`cancelled`·`skipped`는
  중립 테두리를 유지해 실행 중인 노드와 구분한다.
- **건너뛴 이유**: 경로가 선택되지 않아 실행하지 않은 task(`skip.reason: branch_not_selected`)는
  skipped 카드 그대로 라벨만 `NOT SELECTED` 로 읽는다. 정상적인 미선택이라 새 상태색을 쓰지
  않는다. 선행 결과를 쓸 수 없어 건너뛴 task(`upstream_unavailable`)는 `SKIPPED` 라벨이다.
  어느 선행이 실패했는지는 상세의 의존성 행에서 읽는다.
- **호버 툴팁**은 `이름 — 라벨` 한 줄 뒤에 이유 줄을 붙인다. 첫 줄의 라벨은 카드와 같은 경우를
  고르지만 대문자가 아닌 문장형 문구(`dag.tooltip.*`, 예: `Needs input`, `Postprocess · run 1`)다. 미선택은 "Why: Not selected by the
  upstream result", 선행 실패는 "Why: An upstream task did not succeed", 알 수 없음은 기록된
  이유(`Why: {reason}`)와 "Retry or cancel it to let the graph continue.", 입력 대기는 "Waiting for
  a person in the {provider} session · {대기 시간}" 이다. 이유가 없는 노드는 첫 줄만 보인다.
- **실행 중 세부 단계**(v2 task 의 `phase`)는 카드에 이렇게 보인다. `executing` 은 일반 실행 중
  카드다. `awaiting_input`(agent 세션이 사람의 입력을 기다림)은 바·테두리·배경·글리프·라벨을 모두
  needs-input 노랑(`dag-phase-awaiting*`)으로 바꾸고, 글리프 `!` 와 라벨 `NEEDS INPUT` 을 쓰며,
  소요 시간 자리에 기다린 시간을 보인다. compact 티어에서도 카드 색으로 읽힌다.
  `postprocessing`·`retry_wait` 는 실행 중 색 그대로 라벨만 `POSTPROCESS · RUN n`·`RETRY WAIT · RUN n`
  으로 바꾼다. n 은 후처리 실행 번호다. v1 task 는 세부 단계가 없다.
- **카드 좌상단 아이콘**은 상태와 별개로 task 종류(`run` 터미널 / `custom` 플러그 / `reduce` 레이어 /
  `wait_barrier` 자물쇠 / `agent` 말풍선)를 나타낸다. Claude·Codex agent task 는 같은 `agent`
  아이콘을 쓰고, provider 는 상세 패널에서 읽는다. 상세의 종류 태그는 `Agent` 다.
- **엣지**는 관계 5종을 색과 파선으로 함께 구분한다: `depends_on` 실선, `fallback` 6·3 파선,
  `reduce` 2·3 점선, `binding`(v2 입력 연결, 상세 라벨 "binds input") 8·2·2·2 일점쇄선,
  `transition`(v2 전이) 10·4 긴 파선. 같은 원본에서 `depends_on` 과 `binding` 이 함께 오면
  binding 한 줄만 그린다(binding 이 순서를 이미 뜻한다). 이 축약은 그리기에서만 하고
  `agent.task_graph` JSON 엣지는 두 선언을 모두 낸다. `one_of` binding 은 원본마다 한 줄이다.
  전이 엣지는 선택 상태에 따라 굵기와 잉크만 바뀐다: `pending` 1px, `selected` 2px,
  `not_selected`·`unavailable` 은 죽은 경로와 같은 `dag-edge-dim` 실선 잉크로 둔다(숨기지 않는다). 관계는 파선 무늬가 알리고, 선택 노드의 강조색이 가장 앞선다.
- 파선 배열은 vendor 토큰 `dag-edge-dash-*`(strokeStyle)를 본체 `DagRelation::dash` 와 갤러리
  `Rel::dash` 두 곳에 옮겨 둔 것이다. 루트 패키지 시험 `edge_dashes_match_the_vendor_tokens` 와
  갤러리 `dash_tokens` 시험(`crates/tasty-gallery/src/catalog/components/dag.rs`)이 각각 토큰 JSON 원본과 양방향으로 대조한다(토큰마다 관계 하나,
  실선 관계에는 토큰 없음). 색은 두 곳 모두 같은 Theme 접근자를 쓴다.
  선택 노드에 연결된 엣지는 강조색으로 표시한다.
- **러너 배지**는 `idle` / `active` / `stopped` / `crashed`를 구분한다.
  `stopped`는 할 일이 남았는데 러너가 없는 상태다. `stopped`·`crashed`는 경고색을 쓰고,
  배지 옆에 재개 명령 `tasty agent task-run --workspace <N> --action start`를 표시한다.
  안내는 비례폭 글꼴, 복사할 명령은 고정폭 글꼴을 쓴다. 폭이 640px 미만이면 이 설명을
  숨긴다. 이 화면은 관찰 전용이므로 러너를 실행하는 버튼은 없다.
- **노드 상세**에서는 상태 옆에 종류 태그를 표시한다. 입력 대기 노드는 그 아래 노란 알림에
  "The {provider} session is waiting for a person · {대기 시간}. The graph continues after you answer
  it." 과 **세션 열기**(Open session) 버튼을 둔다. 알 수 없음 노드는 `Why unknown` 아래 기록된 이유와
  다시 실행하거나 취소해야 그래프가 이어진다는 안내를 둔다. 구역 캡션(명령 · 의존(Depends on · 개수) · Why unknown ·
  에러 · 출력)은 번역 문구의 대소문자를 그대로 쓰고 caption 크기(11)로 그린다. 실패정책·시작·소요·종료코드·작업 ID
  행과 명령·에러·출력 본문도 caption 크기다. 작업 ID 값은 mono 이며 선택해 복사할 수 있다. 의존성 줄의 관계 라벨은 micro 크기다. 같은 상세 내용을 넓은 화면에서는
  우측 패널에, 좁은 화면에서는 하단 시트에 배치한다. 경계선은 배치를 담당하는 쪽에서
  그린다(우측 패널의 왼쪽, 하단 시트의 위쪽).

## 상호작용

| 조작 | 결과 |
|------|------|
| 빈 캔버스 좌드래그 / 어디서든 중드래그 | pan. 노드 위 좌드래그는 pan 이 아니다(노드를 옮기는 화면으로 오독되지 않게) |
| 휠 | 세로 pan · `Shift`+휠 가로 pan · `Ctrl`(macOS `Cmd`)+휠 줌(포인터 아래 지점 고정) |
| 노드 클릭 | 선택 → 상세 표시. 빈 곳 클릭 또는 `Esc` 로 해제 |
| 상세의 의존성 행 클릭 | 그 노드로 선택 점프 |
| 상세의 세션 열기 | 입력을 기다리는 agent 세션의 surface 가 보이도록 그 workspace·pane·탭·surface 를 선택한다. 사용자 조작으로만 일어나며 IPC·CLI 경로는 없다. DAG 목록 popup 에서 누르면 popup 을 닫는다 |
| 줌 클러스터 `fit` | 그래프 전체가 들어오게 맞춤(100% 를 넘겨 확대하지는 않는다) |
| 줌 클러스터 방향 버튼 | 좌→우 ↔ 위→아래 전환. 선택은 유지된다. 글리프는 **지금** 방향을 보여준다 |
| 헤더 새로고침 | 폴링 주기를 기다리지 않고 즉시 다시 읽는다 |
| 상세 닫기(`×`) | 선택 해제 — 패널/시트가 접힌다 |

방향·fit·줌에는 키 단축키가 없고 캔버스 우하단 줌 클러스터가 담당한다. tasty 의 단축키는
전부 `KeybindingSettings` 를 거쳐야 하므로([key-mapping](../../../design/policies/key-mapping.md)),
캔버스가 자체 조합을 박으면 이미 배정된 전역 액션과 조용히 겹친다.

`Esc` 는 이 규칙에 맞지 않는다. 캔버스가 egui 의 `Key::Escape` 를 직접 읽어
(`src/adapters/ui/surface/dag_graph/canvas.rs`) 포인터가 캔버스 위(줌 클러스터 밖)에 있으면
선택을 해제하고, 키를 소비하지 않는다. `KeybindingSettings` 에 대응 필드가 없다. 정책의
예외는 Tasty 가 바꾸거나 가로챌 수 없는 OS 관리 단축키뿐이고 이 키는 Tasty 가 직접 처리하므로
예외에 해당하지 않는다. 바인딩으로 노출할지 다른 방식으로 정리할지는 정해지지 않았다.

줌 클러스터가 헤더가 아니라 캔버스 위에 있는 것은 **조작 대상 옆에 붙어야 손이 왕복하지
않기** 때문이다. 헤더 띠는 정체성(DAG 이름, 진행 상황, 러너 상태)만 싣는다.

### 좁은 폭

| 폭 | 접힘 |
|----|------|
| < 640px | 헤더가 **2 줄**(정체성 / 조작)로 접히고(재개 캡션 생략, 배지 알약만), 상세는 우측 패널 대신 하단 시트로 간다 |
| 캔버스 < 560px | 미니맵을 숨긴다 — 캔버스를 가리는 손해가 더 크다 |
| 캔버스 < 400px | 줌 퍼센트 숫자를 숨긴다 — ± 버튼만 있어도 조작은 된다. 기준은 surface 폭이 아니라 **캔버스 폭**이다(상세가 열려 캔버스만 좁아진 경우도 같이 접힌다) |

접는 순서는 밀도가 아니라 **우선순위**다. 러너 배지(그래프가 진행 중인가)와 사이클
배너(그래프가 애초에 돌 수 있는가)는 마지막까지 남는다.

### LOD

줌 배율에 따라 카드 **내용만** 바뀐다. 박스 크기는 3 단계 전부에서 같다 — 줌아웃에서 카드를
실제로 줄이면 레이아웃 좌표까지 다시 계산해야 하고, 그러면 줌 조작 중 그래프가 흔들린다.

| 배율 | 표시 |
|------|------|
| ≥ 70% | 종류 아이콘 + 이름 + 글리프 + 상태 라벨 + 소요시간 |
| ≥ 40% | 종류 아이콘 + 이름 |
| < 40% | 상태 채움만(상태색을 카드 배경에 진하게 섞고 테두리도 상태색) |

축약 중일 때는 좌하단 칩이 "왜 축약되어 보이는가" 를 알린다.

## 상태별 시각

| 상태 | 화면 |
|------|------|
| 첫 폴링 전 | 캔버스 배경만. 0.5 초짜리 스피너 깜빡임을 만들지 않는다 |
| workspace 에 task 없음 | "No tasks in this workspace" + 해당 workspace id 가 박힌 `task-create` 명령 |
| 지정한 DAG 가 사라짐 | "DAG not found: `<id>`" + 헤더에서 다른 DAG 를 고르라는 안내. **다른 DAG 로 자동 전환하지 않는다** |
| 사이클 | 배너 + 사이클 노드 테두리 경고색. 그래프는 그대로 그린다(레이아웃 엔진이 역엣지를 걷어내고 배치한다) |
| 죽은 경로 | 실패/취소/스킵의 하류 중 아직 terminal 이 아닌 노드는 흐리게, 그 엣지는 불투명도 없이 `dag-edge-dim` 잉크로. `fallback` 엣지는 상류 실패가 발동 조건이라 전파하지 않는다 |
| 폴링 실패 | 마지막으로 성공한 그래프를 그대로 둔다(일시적 실패로 그래프가 사라졌다 나타나면 더 혼란스럽다) |

task 없음과 사라진 DAG 두 빈 상태는 공용 `tasty_ui_widgets::paint_dag_empty`로 그린다. 갤러리 `dag-states` 예제도 같은 함수와 같은 문구 키를 쓴다. 글리프(24, text-disabled) · 제목(body, text-secondary) · 안내(caption, text-muted, measure-sm 폭 줄바꿈)를 `spacing_sm` 간격으로 가운데에 쌓고, 안내의 백틱 구간은 code run으로 그린다. 안내는 줄마다 가운데에 맞추므로 다음 줄로 넘어간 명령도 가운데에 온다.

## 갱신과 비용

- **폴링 0.5 초.** runner tick 과 같은 주기다. 캔버스가 보이는 동안에만
  `request_repaint_after` 를 걸므로 배경 탭은 프레임을 소모하지 않는다.
- **사용자 조작은 주기를 기다리지 않는다.** 헤더에서 다른 DAG 를 고르거나 새로고침을
  누르면 폴링 캐시를 무효화해 **다음 프레임에 즉시** 다시 읽는다. 기다리게 두면 고른
  DAG 대신 이전 그래프가 최대 0.5 초 더 남아, 선택이 반영되지 않은 것처럼 보인다.
- **레이아웃 캐시 키는 `(노드 id 나열, 엣지 나열, 방향, 치수)`.** `TaskState` 는 키에 들어가지
  않는다 — 상태만 바뀌었는데 좌표를 다시 계산하면 0.5 초마다 노드가 미세하게 튄다. 좌표
  계산은 [`dag-layout`](../../../dev-guide/dag-layout.md) 이 한다.
- **auto-fit 은 `(DAG, 방향, 뷰포트 버킷)` 조합마다 한 번.** 폴링은 이 키의 어느 성분도
  건드리지 않으므로 데이터 갱신이 사용자의 시야를 리셋하지 못한다.

## 시각 소스

`ui_kits/terminal/overlays/dag_view.jsx` · `dag_surfaces.jsx` · `gallery/dag.jsx` — 픽셀·토큰·
레이아웃 수치의 단일 출처. 토큰은 `component.dag-*`([theme](../../../design/systems/theme.md)).

디자인 시안은 위→아래를 기본 방향으로 그렸지만 구현 기본값은 **좌→우** 다 —
`agent.task_graph --format dot` 이 `rankdir=LR` 을 내보내 CLI 출력과 방향이 일치하고,
노드 카드가 가로로 긴 형태(168×48)라 LR 이 화면 폭을 아낀다. 방향은 토글로 바꿀 수 있다.

## 갤러리 specimen

`cargo run -p tasty-gallery` → **Layouts** 페이지의 `Task DAG · canvas & nodes` /
`Task DAG · chrome, detail & surface` 두 섹션. 캔버스 · 노드 8 상태 · 종류 5 · LOD 3 티어 ·
엣지 5 관계 · 전이 선택 4 상태와 미선택 노드 · 줌 클러스터 + 미니맵 · 러너 배지 5 상태 · 노드 상세 · 빈 상태와 사이클 배너 ·
풀탭 서피스(넓은/320px) 를 전시한다.

갤러리는 main 바이너리를 의존할 수 없어 이 화면의 specimen 은 **별도 구현**이다(좌표 계산만은
`tasty-dag-layout` 을 직접 불러 본체와 같은 코드를 쓴다). 미러가 본체와 어긋난 지점,
시안 대비 의도적 차이(글리프 치환 · 재개 힌트 문구 · 기본 방향)는
[design-gallery-mapping](../../../design/systems/design-gallery-mapping.md#task-dag--surface--canvas--node-layouts)
의 3자 매핑 표에 기록한다.
