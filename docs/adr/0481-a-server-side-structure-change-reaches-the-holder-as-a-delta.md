# ADR-0481: 점유 워크스페이스의 forward 아닌 구조 변경도 기존 StructuralDelta 로 holder 에게 보낸다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: attach, mirror, remote, wire-format, structural-delta, pty-exit, occupancy, adr-0040, adr-0264
- **Group**: remote-attach

## Context

attach client 는 mirror 워크스페이스의 구조를 `StreamControl::StructuralDelta`(실행 후 전체 트리 + surface 디스크립터)로만 안다. 그 메시지를 만드는 자리는 forward 실행(`execute_forwarded_structural_op`) 하나뿐이었다 — 문서도 "성공한 forward" 만 역반영을 약속했다(`docs/dev-guide/attach-behavior.md` "역반영").

그러나 점유 워크스페이스의 구조는 forward 말고도 바뀐다.

- **서버에서 PTY 가 끝난다.** mirror 탭에 `exit` 를 보내면 서버의 셸이 끝나고, 서버는 PTY 종료 cascade 로 그 surface(와 비게 된 탭·pane)를 닫는다. 실측(2026-09-22 이전 검증, 격리 GUI 두 인스턴스 loopback attach): 서버 `list tree` 에서 surface 가 사라진 뒤 5 초가 지나도 클라이언트 `list tree` 에는 그 탭이 남았고, 다음 forward 가 트리를 다시 보낼 때에야 사라졌다.
- **로컬 생성 경로로 멤버가 편입된다**(`tap_new_workspace_member` — `pty.attach_surface` 등 하드 점유 가드 밖의 경로). 이 자리는 새 surface 를 즉시 tap 했지만 트리는 보내지 않아, 클라이언트는 매핑에 없는 surface 의 스냅샷을 받고 버렸다.
- **마지막 멤버의 PTY 가 끝난다.** cascade 가 워크스페이스를 통째로 purge 하는데 holder 에게는 아무것도 안 갔고, `workspace_locks` 에 사라진 워크스페이스를 가리키는 lock 이 남았다. forward 경로의 같은 상황은 이미 `force_detach_workspace` 로 닫혀 있었다.

`OccupancyRegistry::forget_closed_surface` 의 주석은 "holder 는 구조 delta 로 그 사실을 받는다" 고 적고 있었는데, forward 가 아닌 close 에서는 그것이 사실이 아니었다.

## Decision

**새 wire 메시지를 만들지 않고, 기존 `StructuralDelta` 를 기존 의미(실행 후 전체 트리)로 더 많은 자리에서 보낸다.**

- `OccupancyRegistry` 가 "forward 아닌 원인으로 구조가 바뀐 점유 워크스페이스" 를 워크스페이스 단위 집합으로 쌓는다. 쌓는 자리는 둘이다 — `forget_closed_surface` 가 점유 워크스페이스의 멤버를 잊을 때(닫기 정리는 전부 이 함수를 지난다), 그리고 `tap_new_workspace_member` 가 forward 가 아닌 멤버를 편입할 때.
- `CoreState::push_structure_changes` 가 집합을 비우며 워크스페이스마다 holder 에게 `StructuralDelta` 를 push 한다. 워크스페이스가 사라졌으면 forward 경로와 같이 `force_detach_workspace` 한다.
- 비우는 자리는 셋이다: PTY 종료 처리 끝(`app::process_exit::handle` — 두 빌드 공통), 로컬 멤버 편입(**tap 전** — forward 의 "delta → tap" 순서와 같다), 두 빌드의 `StreamReady` 처리 끝(그 밖의 원인이 남긴 표시를 다음 스트림 활동에서 비우는 안전망).
- forward 실행은 자기 delta 를 호출측이 보내므로 성공 시 anchor 워크스페이스의 표시를 지운다 — 같은 트리가 두 번 나가지 않는다. forward 실행 중에는 비우는 자리를 지나지 않으므로 `StructuralResult` 와 그 delta 사이에 다른 delta 가 끼지 않는다(client 의 focus 보정이 "result 직후의 delta" 에 1 회 소비되는 계약이 유지된다).

## Consequences

- **얻은 것**: 서버에서 셸이 끝나 닫힌 탭이 mirror 에서도 곧바로 사라진다. 점유 워크스페이스의 마지막 셸이 끝나면 holder 가 강제 detach 되고 stale lock 이 남지 않는다. 로컬 편입 멤버가 mirror 에 나타난다.
- **잃은 것**: 옛 클라이언트도 이 메시지를 이미 알아 잃는 호환은 없다. 다만 옛 클라이언트는 **요청하지 않은** delta 를 처음 받게 된다 — 그 적용 경로는 forward delta 와 같은 함수이고(`apply_mirror_structural_delta`), 요청 op 와의 상관은 `StructuralResult` 쪽에서만 하므로 새 동작이 필요하지 않다.
- **운영 비용 / 유지 부담**: 구조를 바꾸는 새 경로가 `forget_closed_surface` 나 `tap_new_workspace_member` 를 지나지 않으면 이 역반영이 빠진다. 지금 닫기 정리는 `AppState::cleanup_surface_traced` 한 곳에서 `forget_closed_surface` 를 부른다. `StreamReady` 안전망은 **스트림 활동이 있어야** 돈다 — PTY 종료 · 로컬 편입 말고 다른 원인이 표시만 남기면, client 가 아무것도 안 보내는 동안 전달이 미뤄진다.

## Alternatives Considered

- **새 wire 메시지(예: 서버 발 `StructuralChanged`)** — 클라이언트가 새로 배워야 하고 옛 클라이언트는 무시한다. 기존 `StructuralDelta` 가 이미 "전체 트리로 재동기화" 라 이 용도에 그대로 맞는다.
- **매 루프마다 점유 워크스페이스의 트리 지문을 재서 바뀌면 보낸다** — 원인 목록에 의존하지 않는 대신 모든 루프에서 트리를 훑는다. 원인이 닫기 정리 한 점과 편입 한 점으로 모여 있어 표시 방식이 충분하다.
- **닫는 자리에서 즉시 보낸다(표시 없이)** — cascade 가 끝나기 전(탭·pane·워크스페이스 정리 도중)의 트리가 나갈 수 있고, forward 실행 중이면 `StructuralResult` 앞에 delta 가 끼어 client 의 focus 보정 계약이 깨진다.
- **headless 이벤트 루프(`boot.rs`)의 매 이벤트 끝에서 비운다** — 가장 넓은 안전망이지만 두 빌드의 루프가 다르고, PTY 종료가 두 빌드 공통 함수(`process_exit::handle`)로 이미 모여 있어 거기서 비우는 편이 대칭이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `app::process_exit::handle` 이 `push_structure_changes` 를 안 부르면 PTY 종료의 역반영이 끊긴 것이다 — `src/core/attach_runtime.rs` 의 `a_pty_exit_in_an_attached_workspace_reaches_the_holder_as_a_delta` 가 실패한다(변이로 확인: 그 호출을 지우면 그 시험과 `the_last_member_exiting_force_detaches_the_holder` 가 빨개졌다).
- forward 실행이 표시를 안 지우면 같은 트리가 두 번 나간다 — `a_forwarded_close_leaves_no_structure_change_behind` 가 실패한다(변이로 확인).
- 로컬 편입이 tap 전에 트리를 안 보내면 — `a_local_member_reaches_the_holder_as_a_delta_before_its_snapshot` 가 실패한다(변이로 확인).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- `StreamReady` 안전망에만 기대는 원인이 실제로 생겨 client 가 조용한 동안 mirror 가 낡는 일이 보고되는지. 재는 법: 서버 쪽에서 점유 워크스페이스의 구조를 바꾸는 새 경로가 추가될 때 그 경로가 위 두 표시 자리를 지나는지 리뷰에서 본다.

## References

- 코드 근거(이 결정이 실현된 현재 위치): `src/core/attach.rs`(`OccupancyRegistry::forget_closed_surface` · `mark_structure_changed` · `take_structure_changed`) · `src/core/attach_structure_sync.rs`(`CoreState::push_structure_changes`) · `src/core/attach_runtime.rs`(`tap_new_workspace_member` · `execute_forwarded_structural_op`) · `src/app/process_exit.rs`(`handle`) · `src/app/event_handler.rs`(`apply_stream_outcome`) · `src/boot/headless_stream.rs`(`apply`).
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) "mirror 구조 변경 forward" — 역반영 절.
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — hard 점유 모델.
- [ADR-0264](0264-mirror-restore-closed-item-runs-on-the-remote.md) — 같은 delta 채널로 복원 결과를 보내는 선례.
