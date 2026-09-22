# ADR-0536: 원격 끊김으로 창의 마지막 mirror 워크스페이스가 사라지면 기본 워크스페이스를 다시 만든다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: remote-attach, mirror, disconnect, workspace, window-lifecycle, identity-principle-1, adr-0120
- **Group**: remote-attach

## Context

앵커(워크스페이스 매핑)가 없는 mirror 세션은 원격이 끊기면(EOF · force-detach · heartbeat TTL
만료 — client 에게는 셋 다 "소켓이 닫혔다" 로 온다) 재연결하지 않고 정리된다.
`src/app/attach_client.rs` 의 `remove_mirror_workspace_from_engine` 이 그 워크스페이스를
`engine.workspaces` 에서 지우고 활성 인덱스를 보정한다.

사용자가 로컬 워크스페이스를 다 닫고 mirror 하나만 남긴 창에서 이 정리가 일어나면 창은
워크스페이스 0 개로 남는다. 다음 redraw 가 `forward_egui_mesh_context → surface_regions →
active_workspace` 에서 debug_assert 로 죽는다(release 는 바로 뒤 빈 Vec 인덱싱). 격리 debug
인스턴스 둘(A 원격 · B GUI, loopback `remote attach --into-gui`)로 세 끊김 경로를 각각 재현했다
(실측 2026-09-23 — 수정 전 바이너리에서 EOF 6 회 · force-detach 2 회(+ 판독 미완 1 회, 같은 panic) · heartbeat
TTL 1 회, 매번 같은 panic. 횟수는 판독까지 끝난 회차 수다. 수정 후 바이너리는 경로마다 3 회씩 9 회 전부 B 가 살아남았다 — 수정 후 쪽은 구현자가
아닌 리뷰어가 쟀다).

같은 빈 상태를 만드는 다른 자리들은 둘 중 하나로 막혀 있다.

- 사용자가 마지막 워크스페이스를 닫는 경로(단축키 · 메뉴)는 창을 닫는다(`request_close` →
  마지막 main 이면 park). 사용자가 원한 것이 그것이다.
- 사용자 행동이 아닌 제거 — 에이전트가 마지막 surface 를 닫거나 PTY 가 끝난 cascade — 는 기본
  워크스페이스를 즉시 다시 만든다(`close_surface_by_id_no_snapshot`,
  `structural_cascade::recreate_workspace_if_now_empty`). 에이전트의 명시적 `workspace.close` 는
  마지막 하나를 거절한다([ADR-0120](0120-agent-workspace-close-boundaries.md) ①).

원격 끊김은 사용자 행동이 아니고, 거절할 요청도 없다 — 이미 일어난 사건이다.

## Decision

**원격 끊김이 창의 마지막 워크스페이스를 지우면 기본 터미널 워크스페이스를 다시 만들어 활성으로
삼는다.** 사용자 행동이 아닌 제거에 쓰는 기존 복구와 같은 모양이다 — 복구 본문은
`AppState::recreate_workspace_if_empty` 하나이고 `close_surface_by_id_no_snapshot` 도 그것을 쓴다.
mirror 정리는 창 있는 engine 과 parked engine 이 같은 함수를 쓰므로 parked engine 에도 같이
적용된다(창이 복원될 때 빈 engine 이 실리지 않는다). host event 는 내지 않는다
(`Core::create_default_workspace` 와 같은 의미의 시스템 복구다).

## Consequences

- **얻은 것**: 원격이 끊겨도 사용자 창이 죽지도 닫히지도 않는다. 세 끊김 경로가 한 함수로
  모이므로 한 자리의 수정이 셋을 덮는다.
- **잃은 것**: 사용자가 요청하지 않은 터미널 워크스페이스(PTY 하나)가 생긴다. 끊김
  toast(`attach.toast.mirror_disconnected`)는 종전대로 뜬다 — 사용자는 mirror 가 왜 사라졌는지를
  그것으로 안다.
- **운영 비용 / 유지 부담**: 없다 — 기존 복구 본문을 함수로 뽑아 두 자리가 공유한다.

## Alternatives Considered

- **창을 닫는다(park)** — 사용자가 마지막 워크스페이스를 닫았을 때와 같은 결과다. 그러나 그것은
  사용자가 원해서 한 일이고, 원격 끊김은 사용자 행동이 아니다. 원격 사건이 사용자 창을 없애면
  원칙 1(다른 주체의 행동이 사용자 상태에 닿지 않는다)에 걸린다. ADR-0120 이 에이전트 요청에 대해
  같은 이유로 "창까지 닫는다" 를 기각했다.
- **mirror 워크스페이스를 끊긴 채 남긴다** — 앵커 있는 세션이 재연결 대기 중에 하는 일과 같은
  모양이다. 그러나 앵커 없는 세션에는 재연결 트리거가 없어 그 워크스페이스는 영영 죽은 채로
  남고, 창에 워크스페이스가 둘 이상일 때의 종전 동작(mirror 가 사라진다)과 갈린다. 기존 외부
  동작을 가장 많이 보존하는 쪽은 "mirror 는 사라진다" 를 그대로 두고 빈 창만 메우는 것이다.
- **렌더 경로가 빈 창을 건너뛴다** — 증상 은폐다. 창은 워크스페이스 없이 남고, redraw 밖에서도
  active workspace 를 묻는 IPC·입력 핸들러가 같은 전제로 죽는다.

## Reconsideration Triggers

**채널이 붙는 것**

- 앵커 없는 mirror 세션도 끊김 뒤 재연결하게 되면 — 워크스페이스를 지우지 않고 재연결 대기로
  남기는 쪽이 맞을 수 있다. 재는 법: `src/app/attach_client.rs` 의 `disconnect_disposition` 에서
  `has_anchor == false` 가 여전히 `Cleanup` 으로 가는지.
- `engine.workspaces` 를 줄이는 새 자리가 생기면 그 자리가 이 복구나 `request_close` 중 하나를
  타는지 본다. 이 판정에는 기계 채널이 없다 — 빈 창 레이스의 가드
  (`crates/tasty-doc-guards/tests/close_request_consumed_in_place.rs`)는 `request_close` 생산자만
  보고 직접 `workspaces.remove` 하는 자리는 못 본다. 재는 법:
  `git grep -nE 'workspaces\.(remove|clear|retain|drain|truncate|pop|swap_remove)\(' -- src`
  의 비시험 자리마다 복구 경로를 확인한다.

**원리적으로 안 붙는 것**

- 사용자가 "끊겼는데 왜 새 터미널이 열리나" 를 불편으로 보고하면 — 창을 닫는 쪽과 다시 견준다.
  재는 법: 사용자 보고.

## References

- 선행 결정: [ADR-0120](0120-agent-workspace-close-boundaries.md) (다른 조항 — 에이전트의 명시적 닫기 요청은 거절, 여기서는 거절할 요청이 없다)
- [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) "mirror 세션 종료" · [features/remote-attach](../features/remote-attach/index.md) "창 없는 상태(parked)에서의 세션 수명"
- [identity.md](../identity.md) 원칙 1
- 코드 근거(심볼 이름): `AppState::recreate_workspace_if_empty` · `remove_mirror_workspace_from_engine` ·
  `close_surface_by_id_no_snapshot`. 시험: `src/app/attach_client.rs` 의
  `removing_the_only_mirror_workspace_recreates_a_default_workspace` · `src/state/tests.rs` 의
  `close_surface_by_id_no_snapshot_recreates_when_emptied`.
