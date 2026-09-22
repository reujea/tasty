# ADR-0543: forward 된 convert 의 실패 사유는 도메인 이벤트가 나르고, 사유가 없으면 원인을 짐작하지 않는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: attach, mirror, remote, error-message, wire-format, convert, compatibility, adr-0395, adr-0482
- **Group**: remote-attach

## Context

mirror client 가 forward 한 구조 op 는 서버에서 실행되고, 실패하면 `StructuralResult{ok:false, reason}` 로 회신된다. client 는 그 사유를 고치지 않고 사용자 발화 op 의 실패 toast 끝 괄호 안에 싣는다(에이전트 발화 op 의 실패는 [ADR-0503](0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) 에 따라 toast 없이 warn 로그로 간다). split · 새 탭 · close 류는 도메인 실행 함수(`core/structural_exec.rs`)가 실패 문구를 돌려주므로 그 문구가 그대로 간다([ADR-0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md)).

convert 는 대응하는 도메인 실행 함수가 없어 서버가 `Core::apply(DomainIntent::ConvertSurface)` 를 직접 부른다. 그런데 `Core::apply` 는 convert 실패를 `Err` 로 돌려주지 않고 `CoreEvent::SurfaceConverted { replaced: false }` 이벤트로 답했고, 그 이벤트에는 사유 칸이 없었다(registry 오류는 warn 로그로만 남았다). 그래서 `execute_forwarded_structural_op` 가 사유를 스스로 만들었다 — `surface {id} not found`. 실측: 서버에서 kind 가 미등록이라 실패한 convert(`unknown surface kind: <kind>`)가 client 에는 `surface 5 not found` 로 왔다. 대상 surface 는 멀쩡히 살아 있었다.

결함의 형태는 둘이다. 사유가 실패를 낸 자리에서 회신 자리까지 **운반되지 않았고**, 운반되지 않은 자리에서 **원인을 짐작한 문구**가 채워졌다. 앞엣것만 고치면 다음에 사유를 빠뜨리는 실패 자리가 생겼을 때 같은 짐작 문구가 다시 사실을 가린다.

## Decision

1. **사유는 도메인 이벤트가 나른다.** `CoreEvent::SurfaceConverted` 에 `failure: Option<String>` 칸을 더하고, `impl_convert.rs` 의 실패 자리는 모두 그 자리의 문구를 싣는다(registry 오류 `unknown surface kind: <kind>` · Terminal 생성 오류 · 위치 탐색 실패의 `surface N not found` · `pane N not found` · leaf 교체 실패). 성공이면 `None` 이다. `Core::apply` 는 convert 실패를 여전히 `Err` 가 아니라 `replaced:false` 이벤트로 답한다 — 반환 계약을 바꾸지 않는다. `execute_forwarded_structural_op` 는 `failure` 를 그대로 회신 사유로 옮긴다.
2. **사유가 없을 때의 폴백은 원인을 짐작하지 않는다.** 이벤트의 `failure` 가 비어 있으면 회신 문구는 `surface <id> was not converted`(`convert_failure_fallback`)다 — 무엇이 일어났는지(변환되지 않았다)만 말하고 왜인지는 말하지 않는다. "not found" 같은 원인 문구는 그것이 원인일 때만 참이고, 원인을 모르는 자리에서 쓰면 원인이 다를 때 실제 사유를 가린다. 이 규칙은 이 함수 한 자리의 문구 선택이 아니라, forward 회신 사유를 **지어내는** 자리가 새로 생길 때 같은 기준으로 적용한다.

client 쪽 폴백(wire 에 `reason` 이 아예 없으면 괄호 없는 기본 toast 문구)은 바꾸지 않는다.

## Consequences

- **얻은 것**: 원격에서 난 convert 실패 사유가 client toast 에 그대로 온다. 서버 도메인이 모든 실패 자리에 사유를 실으므로 폴백은 새 실패 자리가 사유를 빠뜨렸을 때만 나가고, 그때도 사실과 어긋나는 문구는 안 나간다.
- **잃은 것**: 없음 — 로컬 convert 와 `image.open` 의 외부 동작은 그대로다. 이벤트 칸을 더한 것 외에 로컬 소비 자리는 바뀌지 않았다: `src/app/dispatch_domain.rs` 의 `SurfaceConverted` 매치는 `..` 한 줄만 더했고(diff 로 확인, 분기 동작 무변경), `src/adapters/ipc/handler/image.rs` 는 이미 `..` 패턴이라 소스가 바뀌지 않았다.
- **운영 비용 / 유지 부담**: `impl_convert.rs` 에 실패 자리를 새로 만드는 사람은 `failure` 를 채워야 한다. 빠뜨리면 컴파일러는 잡지 않고(`Option`) 폴백 문구가 나간다 — 틀린 사유는 아니지만 정보가 빠진다. `image.open` 핸들러는 `replaced:false` 에서 아직 `Surface {sid} not found` 를 스스로 만든다 — 결정 2 의 기준으로는 같은 형태이고, 이 ADR 이 그 자리를 고치지는 않는다.

## Alternatives Considered

- **`Core::apply` 가 convert 실패를 `Err` 로 돌려준다** — 사유 운반 자체는 더 자연스럽지만 로컬 경로의 외부 동작이 바뀐다. 로컬 convert 는 지금 `replaced:false` 이벤트를 받고 조용히 넘어가는데(`dispatch_domain` 은 `replaced` 가 참일 때만 후처리한다), `Err` 가 되면 `src/intent/surface.rs` 의 `report_apply_error` 를 타 사용자 toast 가 새로 뜨고, `image.open` 도 지금의 `invalid_params` 대신 `structural_apply_error` 경로의 다른 응답을 내게 된다. 이 결함은 forward 회신의 사유가 틀린 것이지 로컬 동작이 틀린 것이 아니므로, 이벤트에 칸을 더해 로컬 경로를 그대로 두는 쪽을 골랐다.
- **폴백 문구를 종전 `surface <id> not found` 로 둔다** — 지금 도메인이 모든 자리에 사유를 실으므로 당장은 안 나간다. 그러나 나가는 날에는 이번 결함과 같은 거짓을 말한다. 원인 없는 문구로 바꾸는 비용이 0 이라 남길 이유가 없다.
- **폴백 대신 사유가 없으면 panic · debug_assert** — 서버 한 op 의 사유 누락으로 서버 동작을 멈추는 것은 결함의 크기와 맞지 않는다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 도메인 실패 자리가 사유를 싣지 않게 되거나 회신이 그것을 버리면 — `src/core/attach_runtime.rs` 의 `forward_convert_unknown_kind_reports_the_remote_reason` 이 실패한다(변이로 확인: registry 실패 자리의 `failure` 를 `None` 으로, 또는 회신 자리에서 `failure` 를 버리고 폴백으로 보내면 left `surface 1 was not converted` 로 빨개졌다).
- client 가 wire 사유를 원문 그대로 toast 에 싣지 않게 되면 — `src/app/attach_client.rs` 의 `a_structural_failure_reason_reaches_the_toast_verbatim` 이 실패한다.
- `Core::apply` 가 convert 실패를 `Err` 로 돌려주게 바뀌면 결정 1 의 전제(반환 계약 유지)가 사라진다 — 그때 `failure` 칸은 중복이 되어 거둘지 다시 판단한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- forward 회신 사유를 파싱해 가르는 소비자가 생기는지(그러면 폴백 문구가 계약이 되어 코드 칸이 필요해진다 — [ADR-0482](0482-a-forward-naming-a-gone-surface-is-answered-like-ipc.md) 의 같은 조건). 재는 법: `StructuralResult.reason` 을 문자열 비교하는 코드가 리뷰에 나오는지 본다.

## References

- 코드 근거(이 결정이 실현된 현재 위치): `src/core/intent.rs`(`CoreEvent::SurfaceConverted`) · `src/core/impl_convert.rs` · `src/core/attach_runtime.rs`(`execute_forwarded_structural_op` · `convert_failure_fallback`) · `src/app/dispatch_domain.rs`.
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) "mirror 구조 변경 forward" 의 회신 절 · [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md) 의 실패 회신 항목.
- 선행 결정: [ADR-0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md) (forward 와 IPC 가 같은 도메인 실행 · 같은 실패 문구를 쓴다 — 이 ADR 은 대응 실행 함수가 없는 convert 에 그 "실패를 낸 자리의 문구" 를 잇는다) · [ADR-0482](0482-a-forward-naming-a-gone-surface-is-answered-like-ipc.md) (도메인 실행 이전의 거절 사유가 사실을 말하게 한다 — 같은 계열의 다른 조항).
- 탐색: `git grep -l 'SurfaceConverted\|convert_failure\|StructuralResult' -- docs/adr/` → 0337 · 0395 · 0480 · 0481 · 0482. 0337 · 0395 는 convert 가 `Core::apply` 를 직접 부르는 배치를, 0480 · 0481 은 회신·역반영의 다른 조항을 정한다 — 사유 운반과 폴백 문구를 정한 ADR 은 없다.
