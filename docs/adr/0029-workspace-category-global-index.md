# ADR-0029: 워크스페이스 카테고리 — active 는 전역 인덱스 단일 진실 소스 유지

- **Status**: Accepted
- **Date**: 2026-06-29
- **Tags**: workspace, workspace-category, sidebar, indexing, focus
- **Group**: window-workspace-lifecycle

## Context

워크스페이스 카테고리(사이드바 폴더, S-WSCAT) 도입으로 사이드바가 "카테고리 → 그 안의 워크스페이스" 2단 구조가 된다. 사용자 active 워크스페이스와 `Alt+숫자` 전환을 어떤 인덱스 공간으로 표현할지 결정해야 했다.

기존 코드의 active/이동/닫기 보정은 전부 **전역 평면 인덱스**(`AppState.active_workspace: usize` = `CoreState.workspaces` Vec 의 0-based 위치)를 가정한다. 보정 로직은 두 곳에 중복 존재한다 — 직접 경로(`src/state/workspace.rs`)와 intent cascade(`src/app/dispatch_domain.rs`). `move_workspace` / `close_workspace_at` 의 active 인덱스 보정도 모두 전역 인덱스 산술이다.

선택지는 ① 전역 인덱스를 단일 진실 소스로 유지하고 카테고리-로컬은 표시·단축키 매핑 계층에서만 변환, ② active 를 `(category_id, local_index)` 튜플로 전면 교체.

## Decision

**active 워크스페이스는 전역 인덱스(`usize`)를 단일 진실 소스로 유지한다.** 카테고리-로컬 인덱스는 단축키/사이드바 매핑 계층에서만 쓰고, 진입 시 즉시 전역 인덱스로 변환해 기존 `switch_workspace`/move/close/cascade 경로를 그대로 재사용한다(`switch_workspace_in_active_category`, `workspaces_in_category` 가 전역 인덱스를 동반 반환). 카테고리 CRUD·삭제·소속 변경은 워크스페이스의 물리 순서(전역 인덱스)를 보존하므로 active 보정이 전혀 필요 없다.

## Consequences

- **얻은 것**: move/close/cascade 의 active 보정 로직(두 곳 중복)을 한 줄도 재작성하지 않고 재사용. off(토글 비활성) 경로가 바이트 단위로 현행과 동일 → 무회귀가 자연 충족. 카테고리 IPC 의 부수효과가 사용자 active 에 닿지 않음(원칙 1·3) 이 인덱스 모델 차원에서 보장됨.
- **잃은 것**: "카테고리별 마지막 active 워크스페이스 기억" 같은 카테고리-로컬 상태는 전역 인덱스만으로 표현되지 않아 별도 보조 상태가 필요하다(Phase 7 과제).
- **운영 비용 / 유지 부담**: 로컬↔전역 변환이 매핑 계층에 분산되므로, 카테고리 필터/정렬을 바꿀 때 변환 헬퍼(`workspaces_in_category`)의 순서 계약을 유지해야 한다.

## Alternatives Considered

- **(category_id, local_index) 튜플로 active 전면 교체** — 영속·move·close·cascade·드래그앤드롭의 인덱스 산술을 전부 재작성해야 하고, 두 곳의 보정 로직이 발산할 위험이 크다. 회귀 표면이 넓어 비용 대비 이득이 낮다.
- **카테고리별 평면 서브-Vec 보유(중첩 저장)** — `workspaces` 를 카테고리별로 쪼개면 전역 순회(finders, list 전 워크스페이스 순회 원칙)가 복잡해지고 surface→workspace 탐색이 느려진다.

## Reconsideration Triggers

- 카테고리-로컬 상태(예: per-category last-active, 카테고리별 독립 스크롤/정렬)가 늘어 전역 인덱스 변환 계층이 오히려 더 복잡해질 때.
- 다중 카테고리 동시 표시/필터처럼 "전역 평면 순서" 가정이 깨지는 요구가 생길 때.

## References

- [features/workspace-category](../features/workspace-category/index.md)
- [focus 정책](../design/policies/focus.md) (포커스 독립성, 원칙 1·3)
