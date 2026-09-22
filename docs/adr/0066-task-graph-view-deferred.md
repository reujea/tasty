# ADR-0066: task-graph 실시간 화면은 보류한다 (task runner 안정화 전까지)

- **Status**: Superseded by [ADR-0073](0073-task-graph-view-unblock.md)
- **Date**: 2026-08-10
- **Tags**: agent-collaboration, task-graph, ui, scope, deferred, superseded
- **Group**: agent-integration

> **이 ADR 은 더 이상 현행 결정이 아니다.** 여기 적힌 유보는 [ADR-0073](0073-task-graph-view-unblock.md) 로 해제됐다 — 본 ADR 의 재검토 트리거 두 개가 모두 충족되어 task-graph 화면(host builtin surface + workspace 스코프 popup)에 착수하기로 결정했다.

## Context

[agent-collaboration](../features/agent-collaboration/index.md) 은 여러 AI 에이전트가 한
tasty 인스턴스를 공유할 때 쓰는 협업 primitive 6종(Task DAG·Barrier·Semaphore·Lease·Reducer·
Rate-limit)을 IPC/CLI 로만 제공하고, 화면(뷰)이 없다.

- Task DAG 상태(`task_list`/`task_graph`)는 여러 에이전트가 주고받는 조율 상태다. ADR-0040 이
  다루는 "에이전트 점유는 로컬 사용자에게 시각적으로 구분돼야 한다"는 상시-노출 요구나,
  [human-handoff](../features/human-handoff/index.md) 의 승인 popup 처럼 *로컬 사용자의 즉각
  개입이 필요한 이벤트* 와는 성격이 다르다 — 관측이 필요한 시점은 그 조율을 요청한 에이전트(또는
  그 뒤의 사람)가 능동적으로 CLI 로 확인하는 순간뿐이다.
- 다만 이 판단이 features 문서 본문에 "화면을 영구적으로 만들지 않기로 확정한 결정"처럼 서술돼
  있었는데, 그 근거·대안·재검토 조건을 담는 ADR 은 실제로 존재하지 않았다(문서 메타데이터에
  "ADR: 없음"으로 남아 있었다). 실제 의도는 DAG 실행 엔진(task runner) 자체가 아직 정비 중이라
  화면을 만들 시점이 아니라는 **순서상의 유보**였는데, 그 구분을 담을 자리가 없어 기록되지
  못했다.
- task runner 는 이미 동작하지만(상태 8종, command 4종, OnFailure 3종, 완료 판정 전략
  poll/push, 재시작 시 정화+GC 등), 여전히 진행 중인 정비가 있다 — 완료 판정 전략의 검증 보강,
  조회(`task_list`/`task_graph`) 깊이 확장, workspace_id 중복 처리 정리, reducer 출력 추출
  방식 정리, 동시성 제한(rate-limit·semaphore) 관련 문서화 등. 이런 항목이 안정화되기 전에
  화면부터 만들면, 화면이 노출하는 모델(상태 전이·참조 무결성·동시성 한계)이 엔진 쪽 변경을
  따라 계속 다시 그려져야 한다.

## Decision

task-graph 를 실시간으로 보여주는 전용 화면은 지금 만들지 않는다. **이는 순서상의 유보이지
영구 배제가 아니다** — DAG 실행 엔진(task runner)이 실사용으로 안정화되기 전까지 화면 작업에
착수하지 않는다는 뜻이며, 아래 재검토 트리거가 충족되면 화면 설계에 착수한다.

그때까지 관측 수단은 `agent.task_list`/`agent.task_graph`/`agent.task_get`(CLI:
`tasty agent task-list`/`task-graph`/`task-get`) 뿐이다.

## Consequences

- **얻은 것**: task runner 내부 모델이 바뀔 때마다 화면을 다시 맞추는 비용을 피하고, 코어 엔진
  정비에 자원을 집중한다. "화면이 왜 없는가"가 더 이상 기능 문서 본문의 확정처럼 읽히는 서술이
  아니라, 근거·재검토 조건이 명시된 ADR 로 위임된다.
- **잃은 것**: 지금은 실행 중인 DAG 의 상태 변화를 사람이 실시간으로 지켜볼 수 없다 — 매번 CLI
  로 poll 해야 한다. conductor 류의 오케스트레이터가 여러 task 를 동시에 굴릴 때, 진행 상황을
  한눈에 보려면 CLI 출력을 반복 조회하거나 별도 스크립트가 필요하다.
- **운영 비용 / 유지 부담**: 없음(화면 자체가 없으므로 유지할 대상도 없다).
  [agent-collaboration](../features/agent-collaboration/index.md) 은 본 ADR 을 인용해 "화면이
  없는 이유"를 설명한다.

## Alternatives Considered

- **지금 바로 최소 read-only 상태 트리 화면을 만든다**: 엔진 모델(상태 8종·command 4종·
  OnFailure 3종 등)이 아직 넓어지고 있는 중이라, 화면이 그 변화를 계속 따라가야 해 자주 깨진다.
  지금 들일 비용 대비 얻는 값이 낮아 기각.
- **영구 비지원(rejected)으로 못박는다**: 실제로는 필요성 자체를 부정하는 게 아니라 순서 문제일
  뿐이라 과한 결정이다. 못박으면 나중에 필요해졌을 때 새 ADR 로 뒤집어야 하는 번거로움만 남아
  기각.
- **갤러리 specimen 만 먼저 만들어 둔다(본체 배선 없이)**: 화면이 없다는 사실 자체가 "지금은
  CLI 전용" 이라는 운영 방침이라, 실제로 쓰이지 않는 미리보기만 유지보수 부담으로 남는다.
  본체 배선과 함께 갈 때 만들기로 하고 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- agent/task 서브시스템 정비 — task 완료 판정 전략의 검증 보강, `task_list`/`task_graph` 조회
  깊이 확장, workspace_id 중복 처리 정리, reducer 출력 추출 방식 정리, 동시성 제한(rate-limit·
  semaphore) 문서화 등 — 가 완료되어 화면이 노출할 모델이 안정될 때.
- conductor 같은 오케스트레이터가 실사용 중 이 상태를 사람이 반복적으로 눈으로 봐야 하는 사례가
  실제로 확인될 때 — CLI polling 으로는 불충분하다는 근거가 쌓이면.

## References

- 기능 문서: [`features/agent-collaboration`](../features/agent-collaboration/index.md)
- dev-guide: [`dev-guide/agent-runner`](../dev-guide/agent-runner.md) — task runner 내부 동작·
  재시작 계약
- 관련 ADR: [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md)(에이전트 점유의 시각
  구분 필요성 — 본 ADR 이 대비시키는 사례), [human-handoff](../features/human-handoff/index.md)
  승인 popup(마찬가지로 대비되는 즉각-개입 사례)
