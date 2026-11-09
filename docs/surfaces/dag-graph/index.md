# DAG 그래프 (`dag_graph`)

- **Status**: Implemented
- **kind**: `dag_graph` — host 내장
- **렌더 경로**: host-egui
- **주체**: 로컬 사용자(관찰) · AI Agent(생성)
- **설계**: [다중 에이전트 협업 (Agent collaboration)](../../features/agent-collaboration/index.md)
- **코드**: 등록 `register_dag_graph`(`src/runtime/surface_registry/builtins.rs`), 모델 `DagGraphSurface`(`crates/tasty-model/src/dag_graph_surface.rs`), 화면 `src/adapters/ui/surface/dag_graph/`
- **화면**: [screens/graph.md](screens/graph.md)

## 목적·지원 범위

workspace 의 task DAG 를 노드/엣지 그래프로 관찰하는 surface 다. 노드를 편집하거나 task 를 실행하지 않는다. DAG 의 도메인·실행·협업 API(`agent.*`)와 DAG 목록 popup 은 [다중 에이전트 협업](../../features/agent-collaboration/index.md)에 있고, 이 문서는 그 데이터를 보여 주는 surface 의 계약만 다룬다.

## 상태 소유자

- 구조 트리의 leaf 는 다른 kind 와 같은 `SurfaceDescriptor` 다.
- 관찰 대상(`dag_id`·`workspace_id`)과 방향은 engine 의 surface 실행 객체 `DagGraphSurface` 가 가진다. 화면에서 다른 DAG 를 고르거나 방향을 바꾸면 View 가 `EngineAction::DagSelection` 을 보내고, App 이 요청에 담긴 `SurfaceBinding` 이 아직 그 surface 를 가리킬 때만 적용한 뒤 레이아웃을 dirty 로 표시한다(`src/app/engine_action.rs`).
- DAG 데이터는 host 의 task store 가 가진다. surface 는 아래 조회 서비스로 읽기만 한다. 노드를 바꾸거나 task 를 실행하는 경로는 없다.
- 줌·팬·선택·마지막 조회 결과는 창의 View 상태 `MainViewState::dag_graph_views`(surface id 별 `DagGraphView`)에 있고 저장하지 않는다(아래 저장·복원).

### 조회 소유와 취소

View는 `EngineRead::dag_source()`로 읽기 요청을 등록하고 완료된 스냅샷만 받는다.
공유 task store의 잠금 획득·조회·DAG 집계는 App 소유 워커가 수행한다.
동시에 실행하는 워커는 App 전체 4개, engine별 1개이며 대기 큐는 engine별 16개다.
큐가 가득 차면 View의 수신 핸들이 요청을 보존해 다음 폴링에 다시 등록한다.

명시한 `workspace_id`가 다른 창의 engine에 속해도 조회할 수 있다. App은 완료 전달 전에
대상 workspace의 실제 소유 engine과 journal incarnation/runtime epoch를 다시 대조하고,
러너 표시도 그 소유자의 상태를 사용한다. View는 요청을 낸 engine과 표시 대상 DAG를
대조한다. surface activation 변경·숨김·닫힘은 진행 중인 수신 핸들을 버린다.

같은 대상의 일시적인 조회 실패는 마지막 정상 화면을 유지한다. 소유 세대가 바뀌거나
workspace가 사라진 응답은 기존 데이터를 지운다. 저장소 잠금을 기다리는 워커도
수신 핸들 취소나 App 종료를 관측하면 잠금 해제를 기다리지 않고 반환한다.
이미 실행 중인 저장소 호출은 강제로 중단하지 않으며 App의 종료 대기 기한을 따른다.

## 생성·갱신·종료

- `tasty new tab --pane <PANE> --type dag_graph`, `tasty split --level surface --surface <SID> --type dag_graph` 로 만든다. [surface 변환](../../features/convert-surface/index.md) popup 에도 다른 kind 와 같이 나온다.
- params: `dag_id`(별칭 `dag`) · `workspace_id` · `direction`(`td` 이면 위→아래, 그 밖의 값과 생략은 좌→우). 모두 선택이다. `dag_id`·`workspace_id` 를 모두 생략하면 이 surface 가 속한 workspace 에서 진행 중인 DAG 를 고른다(없으면 가장 최근 갱신). 활성 workspace 가 아니라 소속 workspace 다([포커스 정책](../../design/policies/focus.md)). 생성 인자의 정본은 이 절이다.
- 생성은 모델만 만들고 조회를 시작하지 않는다. 조회는 surface 가 화면에 보일 때 View 가 시작한다.
- 화면 갱신 주기와 캐시는 [화면 문서의 갱신과 비용](screens/graph.md#갱신과-비용)에 있다.
- 숨김·닫힘은 진행 중인 조회 수신 핸들을 버린다(위 조회 소유와 취소). 숨긴 surface 는 다시 보일 때 새로 조회한다. 닫으면 engine 이 모델을 지우고 창은 `release_surface_views` 로 그 surface 의 View 와 조회 예약을 지운다.
- 지정한 DAG 가 사라지면 "DAG not found" 상태를 보이고 다른 DAG 로 자동 전환하지 않는다. workspace 가 사라지거나 소유 세대가 바뀌면 이전 데이터를 지운다(위 조회 소유와 취소, [화면 상태](screens/graph.md#상태별-시각)).

## 저장·복원

`direction` 과 (지정했다면) `dag_id` / `workspace_id` 만 레이아웃에 저장된다(`register_dag_graph` 의 `snapshot`). 헤더에서 고른 DAG 도 `dag_id` 로 저장된다. 복원(`restore`)은 생성과 같은 규칙으로 모델을 만들고, 저장한 DAG 가 그 사이 사라졌으면 복원은 성공하고 "DAG not found" 상태를 보인다. 줌/팬/선택은
저장하지 않는다 — 재시작 후 그래프 모양이 달라져 있을 수 있어 예전 뷰포트를 복원하면 엉뚱한
빈 곳을 보게 된다. 복원 직후에는 auto-fit 이 돈다.

## IPC·CLI

이 kind 전용 IPC 는 없다. 생성·분할·닫기는 공통 명령([작업 영역](../../features/work-area/index.md#인터페이스))을 쓰고, DAG 데이터의 조회와 조작은 `agent.*`([다중 에이전트 협업](../../features/agent-collaboration/index.md))을 쓴다.

## headless·원격 제약

- **headless**: host 내장 kind 라 헤드리스에도 등록된다. 생성·조회(`tasty list surfaces`)·닫기·저장·복원은 모델만으로 동작한다. DAG 조회는 View 가 보일 때만 시작하므로 헤드리스에서는 조회 워커가 이 surface 때문에 돌지 않는다. DAG 데이터는 `agent.*` 로 직접 읽는다.
- **attach mirror**: DAG 콘텐츠를 mirror 하지 않는다. mirror workspace 의 `dag_graph` leaf 는 client 에서 `empty` placeholder 로 만들어지고(`build_layout`, `src/app/attach_client/projection.rs`) client 는 DAG 를 조회하지 않는다([원격 attach](../../features/remote-attach/index.md)의 kind 표 "나머지").

## 화면

- [screens/graph.md](screens/graph.md) — 그래프 캔버스, 상호작용, 상태별 시각, 갱신 주기

탭 surface의 캔버스는 포커스 바탕 쌍으로 칠하고 값은 [포커스 표현](../../design/policies/focus.md#surface-포커스-표현)의 판정을 따른다. 헤더·상세·노드 색은 포커스로 바뀌지 않는다. 워크스페이스 팝업의 그래프는 surface가 아니므로 캔버스가 `dag-canvas-bg` 그대로다.

## 검증 기준

- Given 진행 중인 DAG 가 있는 workspace When `tasty new tab --pane <PANE> --type dag_graph` 로 params 없이 만든다 Then 이 surface 가 속한 workspace 의 진행 중인 DAG 를 관찰한다.
- Given `direction` 을 바꾼 DAG surface When 앱을 다시 시작한다 Then 방향과 지정한 대상은 복원되고 줌·팬·선택은 복원되지 않는다.
- Given 특정 `dag_id` 를 관찰하는 surface When 그 DAG 가 지워진다 Then "DAG not found" 상태가 되고 다른 DAG 로 자동 전환하지 않는다.
- Given 그래프를 보여 주는 surface When 조회가 일시적으로 실패한다 Then 마지막으로 성공한 그래프가 그대로 남는다(`src/adapters/ui/surface/dag_graph/view.rs` 의 `changing_dag_target_discards_the_old_read_and_same_target_failure_keeps_its_snapshot`).
- Given 조회가 진행 중인 surface When 다른 탭으로 가려지거나 닫힌다 Then 진행 중인 수신 핸들을 버리고, 가려진 surface 는 다시 보일 때 새로 조회한다(`retired_workspace_and_replaced_activation_clear_display_and_cancel_pending_reads`).
- Given 노드를 선택한 그래프 When 포인터를 캔버스 위에 두고 `Esc` 를 누른다 Then 선택이 해제된다. 이 키는 `KeybindingSettings` 를 거치지 않는다([화면 문서](screens/graph.md#상호작용)).
- Given mirror workspace 의 `dag_graph` When client 가 보여 준다 Then placeholder 로 보이고 DAG 를 조회하지 않는다.
