# ADR-0482: forward 가 사라진 surface 를 지목하면 IPC 와 같은 "no live surface" 사유로 거절한다

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: attach, mirror, remote, error-message, wire-format, parity, adr-0395
- **Group**: remote-attach

## Context

mirror client 가 forward 한 구조 op 는 anchor(원격 surface id)로 점유 워크스페이스를 찾는다. 서버의 두 호출측 — gui `App::apply_forwarded_structural_op`(`src/app/event_handler.rs`) · headless `apply_structural_ops`(`src/boot/headless_stream.rs`) — 은 anchor 가 점유 역매핑에 없으면 도메인 실행 전에 `"workspace not found"` 로 회신해 왔다.

그런데 anchor 가 역매핑에 없는 가장 흔한 이유는 워크스페이스가 아니라 **surface 가 사라진 것**이다. 서버에서 셸이 끝나 surface 가 먼저 닫히면(닫기 정리가 역매핑에서 그 surface 를 뺀다) 그 직후 client 가 같은 surface 를 지목한 forward 는 이 자리에서 거절된다. 실측(격리 GUI 두 인스턴스 loopback attach): mirror 탭에 `exit` 를 보내고 0.8 초 뒤 그 surface 를 forward close 하자 client 토스트가 "…(workspace not found)" 였다. 워크스페이스는 살아 있었다. 같은 상황의 IPC(`surface.close` 에 살아 있지 않은 id)는 `no live surface N (named by 'surface.close'); list the resource to get a live id — a named target is never resolved by focus` 로 거절한다.

## Decision

anchor 가 풀리지 않을 때, **요청 client 가 이 인스턴스에서 살아 있는 워크스페이스를 점유 중이면** IPC 와 같은 생성기(`request_target::unowned_target_message` → `tasty_utils::target::unowned_target_message`)로 사유를 만든다. 종류는 `surface`, id 는 anchor, 요청 이름 자리에는 anchor 를 지목한 것 — forward op 의 wire 이름 `structural_op.<kind>`(예: `structural_op.close_surface`)를 넣는다. 점유 워크스페이스가 없거나 그 인스턴스에 없으면 종전 문구 `workspace not found` 그대로다. **anchor 가 인스턴스 어딘가에 살아 있으면**(점유 워크스페이스 밖 — 같은 engine 의 다른 워크스페이스든 GUI 의 다른 창 engine 이든) 역시 종전 문구다: 그 surface 는 살아 있으므로 "no live surface" 는 거짓이 되고, 이 결정은 사유가 사실을 말하게 하려는 것이다. 이 검사는 engine 하나씩이 아니라 **모든 engine 을 먼저** 본다 — 점유한 engine 은 다른 engine 의 surface 를 모르므로 engine 단위로 물으면 거짓 사유가 나온다. 판정은 `attach_structure_sync::unresolved_anchor_reason` 한 함수이고 두 빌드의 호출측이 그것을 감싼 `unresolved_forward_reason` 을 부른다. `StructuralResult` 의 모양(`ok:false` + `reason`)과 다른 거절 사유(`not workspace holder` 등)는 바꾸지 않는다 — forward 회신에는 에러 **코드** 칸이 없고 새로 만들지 않는다.

## Consequences

- **얻은 것**: 사유가 사실을 말한다. 에이전트는 IPC 와 같은 판정기(`tasty_utils::target::says_no_live_target`)로 이 거절을 알아볼 수 있다. gui · headless 가 같은 함수로 같은 바이트를 낸다.
- **잃은 것**: 토스트 문구가 길어진다(IPC 문구의 꼬리 문장까지 싣는다). `workspace not found` 를 문자열로 가르던 소비자가 있었다면 이 상황에서 갈리지 않는다 — 레포 안에는 그 문자열을 가르는 소비자가 없다(client 는 사유를 토스트에 그대로 붙인다).
- **운영 비용 / 유지 부담**: 요청 이름 자리의 `structural_op.<kind>` 는 `StructuralOp::wire_kind` 가 serde 태그와 같은 철자를 낸다는 데 기댄다 — `crates/tasty-ipc/src/stream.rs` 의 `wire_kind_is_the_serde_tag` 가 그 사본을 고정한다.

## Alternatives Considered

- **요청 이름 자리에 대응 IPC 메서드(`surface.close` · `tab.close` · `split` …)를 넣는다** — IPC 문구와 완전히 같아지지만 거짓이 섞인다. `tab.close` 는 tab id 로 대상을 지목하는데 forward 는 surface 로 지목하고, convert · restore · move-surface 는 대응 IPC 가 없다. 그 자리의 정의("그 대상을 이름으로 지목한 요청")를 지키는 쪽을 골랐다.
- **"anchor surface not found" 같은 새 문구** — 사실은 맞지만 IPC 와 문구가 또 갈라진다. 같은 상황에 같은 문구를 쓰는 것이 이 결정의 목적이다.
- **살아 있는지를 engine 마다 `unresolved_anchor_reason` 안에서 본다** — 같은 engine 의 다른 워크스페이스는 잡지만, GUI 에서 anchor 가 다른 창 engine 에 살아 있으면 점유한 engine 이 먼저 "no live" 를 답한다. 모든 engine 을 먼저 보는 한 자리면 두 경우가 다 잡히고, engine 안 검사는 그 뒤에서 도달할 수 없는 중복이 된다(변이로 확인: engine 안 검사만 끄면 아무 시험도 안 빨개졌다) — 그래서 한 자리만 둔다.
- **anchor 가 서버에 살아 있기만 하면(다른 워크스페이스라도) 실행한다** — holder 가 점유하지 않은 워크스페이스를 바꾸게 되어 hard 점유 모델(ADR-0040)을 깬다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 두 호출측 중 하나가 `unresolved_forward_reason` 을 안 거치면 — `tests/attach_structure_sync_loopback.rs` 의 `a_forward_naming_a_gone_surface_is_answered_like_ipc` 가 그 조합에서 실패한다(변이로 확인: headless 호출을 끊으면 `--no-default-features` 조합에서, 헬퍼를 끊으면 gui 조합에서 빨개졌다).
- IPC 거절 문구의 생성기가 바뀌면 같은 시험의 대조군 단언이 먼저 실패한다.
- anchor 가 다른 곳에 살아 있을 때도 "no live surface" 를 싣게 되면 — `src/core/attach_runtime.rs` 의 `an_anchor_alive_in_another_workspace_is_not_called_gone`(같은 engine 의 다른 워크스페이스) · `an_anchor_alive_in_another_engine_is_not_called_gone`(다른 engine) 이 실패한다(변이로 확인: `unresolved_forward_reason` 의 전 engine 검사를 끄면 둘 다 빨개졌다).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- forward 회신에 에러 코드 칸이 필요해지는지(에이전트가 문자열이 아니라 코드로 가르고 싶어 하는지). 재는 법: `StructuralResult.reason` 을 파싱하는 소비자가 생기는지 리뷰에서 본다.

## References

- 코드 근거(이 결정이 실현된 현재 위치): `src/core/attach_structure_sync.rs`(`unresolved_anchor_reason` · `unresolved_forward_reason`) · `src/app/event_handler.rs`(`apply_forwarded_structural_op`) · `src/boot/headless_stream.rs`(`apply_structural_ops`) · `src/core/request_target.rs`(`unowned_target_message`) · `crates/tasty-utils/src/target.rs`.
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) "mirror 구조 변경 forward" — 회신 절.
- [ADR-0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md) — forward 와 IPC 가 같은 도메인 실행 · 같은 실패 문구를 쓰는 결정. 이 ADR 은 그 규칙을 도메인 실행 **이전**의 거절까지 넓힌다.
