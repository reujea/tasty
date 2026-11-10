# 공통 그래프 뷰

노드와 간선을 레이어로 배치해 관찰하는 화면은 `crates/tasty-ui-widgets/src/graph_view/`의 공통 그래프 뷰로 그린다. DAG surface, DAG 목록 popup의 상세 그래프, 갤러리 DAG 예제가 같은 함수를 호출한다. 데이터의 의미(작업 상태, 관계)와 문구는 호출자가 넣는다.

## 나눈 책임

| 층 | 위치 | 맡는 일 |
|---|---|---|
| 데이터 계약 | `graph_view/model.rs` | `GraphData`·`GraphNodeData`·`GraphEdgeData`. 식별자, 간선 끝(노드 인덱스), 노드 종류, 간선 연결 상태만 읽는다 |
| 뷰 상태 | `graph_view/state.rs` | `GraphView`: 배율, 이동, 선택한 노드 식별자, 자동 맞춤 키, 레이아웃 캐시. 시간과 조회 주기는 모른다 |
| 캔버스 | `graph_view/canvas.rs` | `draw_graph_canvas`: 점 격자, 직교 간선, 화살촉, 카드 자리, 선택 링, 이동·배율·선택 입력, 호버 툴팁 |
| 조작 도구 | `graph_view/chrome.rs` | 줌 클러스터, 미니맵, LOD 칩. 버튼은 `GraphChromeAction`을 돌려주고 적용은 호출자가 한다 |
| 틀과 Drawer | `graph_view/frame.rs` | `GraphFrame`(탭 surface 또는 popup), `split_graph_drawer`(넓으면 우측 패널, 좁으면 하단 시트), `drawer_body`와 상세 행 함수 |
| DAG 카드 | `graph_view/dag_card.rs` | `DagStatus`, 상태 색, `paint_dag_card`. 카드 문구는 `DagCardView`로 받는다 |

카드 내용, 간선 잉크, 흐림 판정은 `GraphPainter`가 정한다. 본체 DAG는 `src/adapters/ui/surface/dag_graph/canvas.rs`의 `DagPainter`, 갤러리는 `catalog/components/dag/canvas.rs`의 `SamplePainter`를 쓴다.

## 데이터 소스

본체는 `src/adapters/ui/surface/graph_source.rs`의 `GraphSource`로 조회를 감싼다. 조회 주기, 진행 중 수신 핸들, 마지막 성공 스냅샷, 오류를 소스가 가진다. DAG는 `DagGraphSource`가 구현한다. 화면 상태 `DagGraphView`는 `source: DagGraphSource`와 `canvas: GraphView` 두 필드로 나뉜다.

데이터 계약에는 아직 동작하지 않는 자리가 있다. 지금 DAG 소스는 이 자리를 쓰지 않고 기본값을 넘긴다.

- `machine_scoped_id(machine, local_id)`: 머신을 넘는 그래프에서 같은 지역 식별자가 겹치지 않게 머신을 식별자 앞에 붙인다.
- `GraphNodeKind::{External, Gap}`: surface가 아닌 바깥 항목과 끊긴 구간을 대신하는 노드 종류.
- `EdgeLink::Broken`: 연결이 끊긴 간선.

## 위젯 crate 경계

공통 그래프 뷰는 `tasty-i18n`, 전역 `theme()`, 본체 타입을 쓰지 않는다. 줌 버튼 툴팁과 LOD 칩 문구는 `GraphChromeLabels`로, 카드 문구는 `DagCardView`로 받는다. 레이아웃은 `tasty-dag-layout`을 직접 부른다. 길이는 Theme 토큰의 `LogicalPx`에서 꺼내 egui 좌표로 계산한다.

## 갤러리

갤러리의 `dag-graph-view` 예제가 공통 뷰의 상태를 보여 준다: 빈 그래프, 작은 그래프, 큰 그래프, 선택과 상세, 좁은 폭의 하단 시트, LOD 세 단계. 다른 DAG 예제도 캔버스·카드·미니맵·줌 클러스터·Drawer 틀은 같은 함수를 쓴다. 본체 바이너리에만 있는 DAG 헤더, 러너 배지, 사이클 배너, 상세 내용, 간선 잉크 규칙은 갤러리가 같은 모양으로 다시 그린다.
