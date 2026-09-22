# ADR-0480: forward 된 구조 op 는 누가 요청했는지를 싣고, 에이전트의 close 는 서버 복원 스택에 안 남는다 — ADR-0264 의 결정 4 개정

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: attach, mirror, remote, restore, closed-item, wire-format, user-agent-separation, identity, adr-0264, adr-0395
- **Group**: remote-attach

## Context

[ADR-0264](0264-mirror-restore-closed-item-runs-on-the-remote.md) 결정 4 는 "forward 된 close 를 일으킨 것은 **원격 사용자의 손 조작**이다" 를 전제로, forward 된 close 를 서버 복원 스택에 남기기로 했다. 그 전제가 틀렸다. mirror 워크스페이스의 구조 op 는 사용자 GUI 에서만 오지 않는다 — 에이전트 CLI/IPC 도 `Core::apply` 로 수렴해 **같은 forward 로** 나간다(`docs/dev-guide/attach-behavior.md` "진입 경로별 응답 정합성": 에이전트의 원격 mirror 조작은 정식 기능이다).

실측(2026-09-22, 격리 GUI 두 인스턴스 loopback attach): 클라이언트의 에이전트가 `tasty close surface --surface <mirror id>` 로 mirror surface 를 닫은 뒤, 서버 창에서 `Ctrl+Shift+T` 를 누르면 에이전트가 닫은 pane 이 **서버 로컬 워크스페이스에 되살아났다.** 에이전트 행동의 부수효과가 사용자의 닫은 항목 히스토리에 닿은 것이고, [`docs/identity.md`](../identity.md) 불가침 원칙 1 의 정면 위반이다.

서버는 이것을 가를 수 없었다. wire 의 `StreamControl::StructuralOp { op_id, op }` 에 요청 주체가 없기 때문이다. 클라이언트는 이미 알고 있었다 — forward 큐의 원소 `PendingStructuralForward.user_triggered` 가 GUI 손 조작(`true`)과 `Core::apply` 경유의 에이전트 경로(`false`, 안전한 기본값)를 가른다. 그 값이 client-only focus 보정에만 쓰이고 wire 에 실리지 않았을 뿐이다.

## Decision

`StreamControl::StructuralOp` 에 **선택(optional) 칸 `origin`**(`"user"` | `"agent"`)을 더하고, 서버는 `origin` 이 `agent` 인 close(`CloseSurface`/`CloseTab`/`ClosePane`)를 **복원 스택에 넣지 않는다** — `structural_exec::close_surface` 를 `save_snapshot=false` 로 부르고, `CloseTab`/`ClosePane` 의 사전 캡처(`capture_closed_*` → `push_closed_item`)를 건너뛴다.

- **칸이 없으면 사용자 조작으로 읽는다**(`ForwardOrigin::of_wire`). 이 칸 이전의 클라이언트가 forward 한 close 는 전부 복원 가능했다 — 부재를 `user` 로 읽으면 옛 클라이언트의 동작이 한 바이트도 안 바뀐다.
- **새 클라이언트는 언제나 명시해서 보낸다.** `user_triggered` 가 `true` 면 `user`, 아니면 `agent` 다. 그래서 클라이언트 쪽의 판정 기준은 기존의 `user_triggered` 하나이고 새로 생기지 않는다.
- **옛 서버는 칸을 무시한다.** `StreamControl` 에 `deny_unknown_fields` 가 없어 serde 는 알려진 variant 안의 모르는 키를 버린다 — 프레임이 아니라 칸만 떨어지고, 옛 서버는 종전대로 모든 forward close 를 남긴다. 새 클라이언트 × 옛 서버 조합에서는 이 결함이 그대로 남는다(아래 잃은 것).
- **서버는 모르는 값을 거절하지 않고 `agent` 로 읽는다.** 칸을 이해하는 서버는 `"user"` 만 `User` 로, 그 밖의 값은 문자열이든 숫자든 객체든 `Agent` 로 읽는다(`ForwardOrigin` 의 손으로 쓴 `Deserialize`). 파생 역직렬화는 모르는 값에서 **프레임을 통째로** 실패시키고, 그 op 는 `StructuralResult` 없이 사라진다(ADR-0264 결정 5 와 같은 무음 실패). 모르는 값을 `Agent` 로 보는 것은 origin 이 정하는 것이 사용자 상태(복원 스택)를 건드리느냐뿐이고, 건드리지 않는 쪽이 원칙 1 이 지키는 쪽이기 때문이다. 부재(와 `null`)는 여전히 `User` 다 — 옛 클라이언트 호환은 부재에 걸려 있고, 모르는 값을 보내는 것은 그보다 **새** 클라이언트뿐이다. 이 관용은 `origin` 칸과 같은 커밋 계열에 들어가므로, 칸을 이해하는 서버는 모두 모르는 값을 견딘다.
- 서버는 `origin` 을 **close 계열에서만** 읽는다. 다른 op 의 실행은 origin 과 무관하다.

**개정하지 않는 것**: ADR-0264 의 결정 1(복원 스택의 소유자는 서버) · 결정 2(forward 복원, 로컬 폴백 없음) · 결정 3(서버 복원 스택의 워크스페이스 스코프, 서버 로컬 복원은 전역 LIFO) · 결정 5(새 variant 의 무음 실패). 결정 4 중 "사용자의 손 조작으로 일어난 forward close 는 복원 가능하다" 와 "`save_snapshot` 과 `is_user_close` 는 독립 축이다" 도 그대로다 — 개정하는 것은 "forward 된 close 는 **전부** 원격 사용자의 손 조작이다" 라는 전제 하나다.

## Consequences

- **얻은 것**: 원격 에이전트가 닫은 것이 서버 사용자(와 원격 사용자)의 `Ctrl+Shift+T` 로 되살아나지 않는다. 원칙 1 이 attach forward 경로에서도 선다. 로컬 IPC close(`save_snapshot=false`)와 forward 에이전트 close 가 같은 규칙을 갖는다.
- **잃은 것**: 새 클라이언트가 **옛 서버**에 붙으면 칸이 무시돼 에이전트 close 가 여전히 서버 스택에 남는다. 서버를 올려야 닫힌다. 반대 조합(옛 클라이언트 × 새 서버)은 종전 동작 그대로라 잃는 것이 없다.
- **잃은 것**: 원격 에이전트가 닫은 탭을 원격 사용자가 mirror 안에서 복원할 수도 없게 된다. 로컬에서 에이전트가 닫은 것을 로컬 사용자가 복원할 수 없는 것과 같은 규칙이다.
- **운영 비용 / 유지 부담**: forward 큐에 원소를 넣는 새 GUI 경로는 `user_triggered` 를 올바로 세워야 한다 — 빠뜨리면 그 경로의 사용자 close 가 `agent` 로 나가 서버 복원 스택에 안 남는다(원칙 위반 방향이 아니라 기능 누락 방향으로 틀린다). 지금 그 값을 세우는 자리는 `mark_last_forward_user_triggered`(origin 을 아는 intent 호출부)와 `AppState::forward_mirror_structural`(항상 GUI 직접 조작) 둘이다.

## Alternatives Considered

- **칸이 없으면 에이전트로 읽는다** — 원칙 1 쪽으로 더 안전하지만, 옛 클라이언트 사용자의 손 조작 close 가 전부 되돌릴 수 없게 된다. 이 결정의 선택 기준 — 기존 외부 동작(wire)의 호환을 가장 많이 보존하는 쪽 — 에 어긋난다.
- **forward close 를 전부 복원 불가로 되돌린다(ADR-0264 이전)** — wire 를 안 바꾸지만 ADR-0264 가 고친 결함(mirror 안에서 사용자가 닫은 것을 되돌릴 방법이 없다)을 되살린다.
- **`StructuralOp` 의 close variant 마다 칸을 둔다** — 판정을 읽는 쪽은 close 뿐이지만 "누가 요청했는가" 는 op 의 성질이 아니라 요청의 성질이다. 봉투(`StreamControl::StructuralOp`)에 한 번 두면 variant 가 늘어도 칸을 복제하지 않는다.
- **새 wire variant(`StructuralOpV2` 등)** — 옛 서버가 그 프레임을 통째로 무시해(ADR-0264 결정 5 의 무음 실패) 새 클라이언트 × 옛 서버에서 모든 forward 가 멈춘다. 칸 추가는 칸만 떨어진다.
- **bool 칸(`agent: bool`)** — 키를 더하는 호환은 같지만, 둘을 넘는 주체(예: plugin 이 대신 요청한 op)가 생기면 그 값을 담을 자리가 없어 새 키를 또 만들어야 한다. 열거는 새 값을 같은 칸에 담는다. 단 **값 추가가 호환이 되는 것은 파생 역직렬화 덕이 아니다** — 파생 열거는 모르는 값에서 프레임을 버린다. 위 결정의 "모르는 값은 `agent`" 관용이 있어서 새 값이 옛 서버(이 칸을 아는 서버)에서 프레임 drop 대신 `Agent` 로 읽히고, 그래서 열거가 bool 보다 여지를 남긴다는 말이 참이 된다.
- **모르는 값을 `User` 로 읽는다** — 옛 클라이언트 호환과 무관하고(옛 클라이언트는 칸을 안 보낸다) 새 주체의 close 를 사용자 스택에 넣게 된다. 원칙 1 의 반대 방향이다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `StreamControl::StructuralOp` 에서 `origin` 칸이 사라지거나 `ForwardOrigin::of_wire` 가 부재를 `User` 가 아닌 값으로 풀면 이 결정의 호환 조항이 바뀐 것이다 — `crates/tasty-ipc/src/stream.rs` 의 `a_structural_op_without_origin_is_a_users_op` 가 그 자리에서 실패한다.
- 모르는 origin 값이 프레임을 버리게 되거나 `Agent` 가 아닌 값으로 풀리면 — `crates/tasty-ipc/src/stream.rs` 의 `an_unknown_origin_keeps_the_frame_and_reads_as_agent` 가 실패한다(변이로 확인: 모르는 값을 `User` 로 바꿔도, 역직렬화 에러로 바꿔도 그 시험이 빨개졌다).
- 새 클라이언트가 에이전트 경로의 op 를 `"agent"` 로 싣지 않으면 서버 쪽 판정이 아무리 옳아도 결함이 되살아난다 — `src/app/attach_client.rs` 의 `an_agent_close_is_forwarded_with_the_agent_origin` 이 실패한다(변이로 확인: `forward_origin_of` 의 `ForwardOrigin::Agent` 를 `User` 로 바꾸면 전량 lib 2608 중 그 시험 하나만 빨개졌다).
- forward 실행이 origin 과 무관하게 close 를 스냅샷하면 결정이 뒤집힌 것이다 — `src/core/attach_runtime.rs` 의 `a_forwarded_agent_close_leaves_no_snapshot` 이 실패한다(변이로 확인: `restorable` 을 상수 `true` 로 바꾸면 그 시험만 빨개졌다).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 옛 서버가 운영에서 사라졌는지 — 사라지면 위 "잃은 것" 첫째가 없어진다. 재는 법: attach 상대 서버의 버전 분포(수동 확인).
- 요청 주체가 둘을 넘는 경우(plugin 이 사용자를 대신해 mirror 구조를 바꾸는 흐름)가 생기는지. 재는 법: forward 큐에 원소를 넣는 새 경로가 `user_triggered` 로 표현 안 되는 주체를 갖는지 코드 리뷰에서 본다.

## References

- 개정 대상: [ADR-0264](0264-mirror-restore-closed-item-runs-on-the-remote.md) (결정 4 의 전제)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 코드 근거(이 결정이 실현된 현재 위치): `crates/tasty-ipc/src/stream.rs`(`StreamControl::StructuralOp` 의 `origin` · `ForwardOrigin`) · `crates/tasty-ipc/src/stream_hub.rs`(`PumpOutcome::structural_ops`) · `src/core/attach_runtime.rs`(`execute_forwarded_structural_op`) · `src/app/attach_client.rs`(`forward_one_structural_op` · `forward_origin_of` · `structural_op_payload`) · `src/core/impl_mirror.rs`(`PendingStructuralForward`).
- [`docs/identity.md`](../identity.md) 원칙 1 · [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) "mirror 구조 변경 forward".
