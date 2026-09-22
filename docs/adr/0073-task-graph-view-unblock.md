# ADR-0073: task-graph 화면 보류를 해제하고 host builtin surface + workspace popup 두 표면으로 만든다

- **Status**: Accepted
- **Date**: 2026-08-19
- **Tags**: agent-collaboration, task-graph, dag, ui, surface, popup, host-builtin, egui-mesh, adr-0066
- **Group**: agent-integration

## Context

[ADR-0066](0066-task-graph-view-deferred.md) 은 "task-graph 를 실시간으로 보여주는 전용 화면은
지금 만들지 않는다" 를 **순서상의 유보(영구 배제 아님)** 로 기록하고, 두 개의 재검토 트리거를
명시했다. 두 트리거가 모두 충족됐다.

### 트리거 1 — agent/task 서브시스템 정비 완료

ADR-0066 이 열거한 정비 항목의 현 상태:

- **task 완료 판정 전략의 검증 보강** — 해소. `src/completion_strategy/`(레지스트리 +
  `registry_tests.rs`)로 전략이 1급 등록 대상이 됐고, plugin 매니페스트
  `[[contributes.completion_strategy]]` 검증까지 붙었다. 동작·결정은
  [dev-guide/agent-runner](../dev-guide/agent-runner.md#완료-판정-전략-레지스트리-srccompletion_strategy)
  에 문서화됐다.
- **`task_list`/`task_graph` 조회 깊이 확장** — 해소. `task_graph` 의 `nodes` 는
  `command_kind`/`on_failure_kind` 를 싣고, `edges` 는 `depends_on`/`fallback`/`reduce` 3종
  `kind` 로 태그되며, 응답에 `runner{running, crashed, ready_count, running_count}` 가 동반된다.
  즉 **화면이 그려야 할 노드/엣지/러너 상태가 이미 조회 표면에 전부 나와 있다** — 화면을 위해
  엔진 모델을 새로 넓힐 필요가 없다.
- **reducer 출력 추출 방식 정리** — 해소. JSON Pointer 기반 `--extract-path` 로 확정되고
  [dev-guide/agent-runner](../dev-guide/agent-runner.md) 에 문서화됐다.
- **동시성 제한(rate-limit·semaphore) 문서화** — 해소.
  [dev-guide/agent-runner](../dev-guide/agent-runner.md#동시성-제한-concurrency-limit) 의 "동시성
  제한" / "자원 풀 배정" 절로 반영됐다.
- **workspace_id 중복 처리 정리** — **미해소이나 화면과 독립.** `Task` 레코드가 `workspace_id`
  필드를 갖는 동시에 영속 스코프 키(`Scope::Workspace(task.workspace_id)`)로도 같은 값을 쓰는
  이중 보유가 그대로 남아 있다. 다만 스코프 키는 항상 레코드 필드에서 파생되므로 둘이 어긋날
  경로가 없고, 화면은 조회 응답의 `workspace_id` 하나만 읽는다. 이 항목의 정리는 영속 포맷
  리팩터링의 문제이지 화면 착수의 선행조건이 아니다.

### 트리거 2 — 사람이 반복적으로 눈으로 봐야 하는 실사용 사례

conductor 류 오케스트레이터가 한 인스턴스에서 여러 task 를 동시에 굴리는 실사용이 자리잡았고,
그 진행 상황을 CLI(`tasty agent task-graph`) 반복 조회로 따라가는 비용이 실제로 드러났다.
더불어 DAG 뷰(그래프 surface + 목록 popup)의 **UI 시안이 확정 수령**됐다 — 화면의 필요성이
"있으면 좋겠다" 수준이 아니라 구체적 화면 설계가 완결될 만큼 확정됐다는 신호다.

## Decision

task-graph 실시간 화면 보류를 해제하고 착수한다. 화면은 **두 표면**으로 만든다.

1. **host builtin surface kind** — 탭/분할로 열어 상시 관찰하는 그래프 뷰
   (`tasty new tab --type <kind>` / `tasty split --type <kind>`). 하나의 DAG 를 노드-엣지로
   그리고, 대상 DAG 는 IPC/CLI 로 명시 지정 가능해야 한다(원칙 3: 포커스 독립성).
2. **workspace 스코프 popup** — DAG 목록을 띄우고 하나를 고르면 그 DAG 로 drilldown 하는,
   "잠깐 열고 닫는" 관측 창.

두 표면 모두 **host 소유**다 — plugin 이 아니다. 선행 데이터 계층(DAG 를 1급 목록으로 여는 조회
표면)과 좌표 계산(레이아웃)도 host 쪽에 둔다. `agent.*` 는 host 도메인이고,
`Core::task_list`/`task_graph` 가 in-process 직접 접근자이기 때문이다.

## Consequences

- **얻은 것**: 실행 중인 DAG 의 상태 변화를 사람이 화면으로 지켜볼 수 있다. ADR-0066 이 "잃은
  것" 으로 적었던 CLI 반복 polling 비용이 해소된다. 화면이 노출할 모델(노드 kind·엣지 kind·
  runner 상태)이 이미 조회 응답에 안정적으로 존재하므로, 착수 시점의 재작업 위험이 ADR-0066
  작성 시점보다 현저히 낮다.
- **잃은 것**: 화면 없음(=유지 대상 없음)이라는 무비용 상태를 포기한다. 라이브 갱신을 위해
  화면이 스스로 재조회를 예약해야 한다 — runner 는 별도 thread(500ms tick)라 egui 렌더 루프를
  깨우지 않는다. 보이지 않는 동안에는 예약하지 않아 유휴 CPU 를 낭비하지 않는 것이 요구사항이
  된다.
- **운영 비용 / 유지 부담**: 화면이 노출하는 모델은 앞으로 엔진 변경을 따라가야 한다 — task 상태
  8종·command 4종·OnFailure 3종·엣지 3종이 늘거나 바뀌면 화면도 함께 갱신한다. 이 비용을 이제
  **지불하기로 한다**(ADR-0066 은 정확히 이 비용을 피하려 유보했었다). 더불어 host builtin
  surface kind 신설은 레이아웃 영속(snapshot/restore)·i18n 키·갤러리 specimen 이라는 상시
  유지 항목을 함께 만든다.
- **미해소 항목의 처리**: 위 "workspace_id 중복 처리 정리" 는 본 ADR 로 해소되지 않는다.
  화면과 독립이므로 화면 작업의 차단 요인으로 취급하지 않되, ADR-0066 의 트리거 목록에서 유일하게
  남은 항목이라는 사실을 여기 기록해 둔다.

## Alternatives Considered

- **plugin + egui-mesh 렌더 채널로 만든다**: 기각. (1) `is_egui_mesh_allowed(kind, plugin_id)`
  가 특정 (kind, plugin_id) 조합만 하드코딩으로 허용하는 화이트리스트라, plugin 으로 만들어도
  **host 소스를 어차피 고쳐야 한다** — plugin 화의 이점(호스트 무수정)이 성립하지 않는다.
  (2) epaint 와이어가 host·plugin 동일 컴파일을 강제해 `api_version` lockstep 부담이 얹힌다
  ([dev-guide/egui-mesh-channel](../dev-guide/egui-mesh-channel.md)). (3) `agent.*` 는 host
  소유 도메인이라, host 가 이미 in-process 로 들고 있는 데이터를 plugin 이 IPC 로 되받아오는
  우회가 된다 — [dev-guide/popup-implementation](../dev-guide/popup-implementation.md) 의
  host/plugin 선택 기준("콘텐츠가 host 데이터이고 host 가 렌더에 필요한 상태를 다 가진 경우 →
  host")에 정면으로 어긋난다.
- **webview(HTML+CSS) 채널로 만든다**: 기각. [ADR-0065](0065-markdown-webview-render-channel.md)
  가 markdown 을 webview 로 옮긴 근거는 타이포그래피·문서 레이아웃이었다. 노드 드래그·pan/zoom
  이 핵심인 그래프 드로잉에는 과하고, host 상태(러너/선택)와의 왕복이 오히려 늘어난다.
- **표면 하나만 만든다(surface 만 또는 popup 만)**: 기각. 두 관측 방식은 대체 관계가 아니다 —
  surface 는 탭을 점유하는 상시 관찰용, popup 은 "잠깐 확인하고 닫는" 용도다. 하나만 만들면
  나머지 용도가 그대로 CLI polling 으로 남아 본 ADR 의 목적이 절반만 달성된다.
- **ADR-0066 파일 자체를 고쳐 Status 만 전이시킨다**: 기각. ADR 은 "그때의 결정" 기록이라
  사후 덮어쓰기보다 supersede 가 이력 보존에 맞고, [`template.md`](template.md) 의 작성 규칙도
  결정이 바뀌면 새 ADR 로 supersede 하도록 요구한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 화면이 실제로 만들어졌는데 실사용에서 CLI 조회 대비 이점이 확인되지 않을 때 — 유지 비용만
  남으면 표면 축소(둘 중 하나 제거)를 검토한다.
- egui-mesh 화이트리스트가 일반화되거나 plugin 이 host 도메인 데이터를 IPC 왕복 없이 읽는 경로가
  생겨, plugin 화의 기각 근거 3개가 모두 무너질 때.
- task 엔진 모델(상태/command/OnFailure/엣지 종류)이 화면이 따라가기 어려운 속도로 다시
  넓어질 때 — 그때는 화면이 노출하는 범위를 좁히는 결정을 새로 기록한다.

## References

- 대체 대상: [ADR-0066](0066-task-graph-view-deferred.md) — 본 ADR 이 supersede 하는 보류 결정
- 기능 문서: [`features/agent-collaboration`](../features/agent-collaboration/index.md)
- dev-guide: [`dev-guide/agent-runner`](../dev-guide/agent-runner.md) — task runner 내부 동작·
  조회 표면·완료 판정 전략 레지스트리
- dev-guide: [`dev-guide/popup-implementation`](../dev-guide/popup-implementation.md) —
  host/plugin 선택 기준, `PopupDef` 등록 절차
- dev-guide: [`dev-guide/egui-mesh-channel`](../dev-guide/egui-mesh-channel.md) — egui 버전
  lockstep 제약
- 관련 ADR: [ADR-0065](0065-markdown-webview-render-channel.md)(렌더 채널 선택 선례),
  [ADR-0028](0028-plugin-egui-mesh-render-channel.md)(plugin egui-mesh 렌더 채널 정의)
