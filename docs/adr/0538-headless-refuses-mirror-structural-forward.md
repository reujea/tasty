# ADR-0538: 헤드리스는 mirror 구조 op 를 forward 큐에 넣지 않고 거절한다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: headless, attach, mirror, structural-op, forward-queue, ipc, agent-facing, build-combination, adr-0425, adr-0346

## Context

mirror(원격 attach client) 워크스페이스 안의 구조 변경(split · 탭 생성·닫기 · convert · 이동 · 복원)은
로컬에서 실행하지 않는다. `Core::apply` 가 그 op 를 `StructuralOp` 로 바꿔 `CoreState` 의 구조 op
forward 큐에 넣고 `MirrorStructuralBlocked { forwarded: true }` 를 돌려준다. IPC 구조 변경 핸들러는
그 값을 `structural_apply_error`(`src/adapters/ipc/handler.rs`)에서 `{forwarded: true}` **성공**으로
답한다 — "원격에 큐잉됐고 곧 실행된다" 는 뜻이다.

큐를 비워 attach 채널로 보내는 쪽은 `App::dispatch_pending_structural_forwards` 하나이고, 그것은
GUI 의 `about_to_wait` 에만 있다. 헤드리스(`--no-default-features`)에는 없다. 그런데 큐에 넣는
`Core::apply` 는 두 조합에 다 컴파일된다. 그러니 헤드리스에서 이 갈래에 닿으면 호출자는 성공을
받고 op 는 영영 나가지 않는다. 큐는 요청 수만큼 자란다.

같은 모양이던 두 forward 큐 — `git_viewer.query` 의 git 조회 · `markdown_mirror.content_request` 의
원문 조회 — 는 2026-09-21 에 dispatch arm 을 gui 로 게이트해 헤드리스에서 `-32017` 로 답하게
했다(ADR-0053 이 금지하는 "수락만 받고 결과가 안 오는" 형태, 처방은
[ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) 의 셋째 갈래). 구조 op 는
그 처방을 못 쓴다 — 메서드 이름(`tab.create` · `split` 등)이 헤드리스에서도 제 할 일을 하는 표면이라
arm 을 뺄 수 없다. 갈리는 것은 메서드가 아니라 **대상 워크스페이스가 mirror 인가**다.

오늘 헤드리스에서 이 갈래는 도달하지 않는다. mirror 워크스페이스를 만드는 자리
(`build_mirror_workspace`)가 `app::attach_client` 에 있고 그 모듈이 gui 전용이다. 그러나 그 사실은
"`Core::apply` 가 헤드리스에서 거짓 성공을 낼 수 있다" 를 막지 않는다 — 막는 것은 다른 파일의
cfg 한 줄이다. 두 사실이 떨어져 있어 한쪽이 바뀌면 다른 쪽이 조용히 결함이 된다.

## Decision

**헤드리스의 `Core::apply` 는 mirror 구조 op 를 forward 큐에 넣지 않는다.** 대상이 mirror
워크스페이스이면 op 종류와 무관하게 `MirrorStructuralBlocked { forwarded: false }` 를 돌려준다.
IPC 핸들러는 그것을 기존대로 `-32603` 오류로 답한다. 헤드리스의 문구는 끝에
`(this headless build has no attach client to forward it)` 를 붙여, 거절 사유가 op 가 아니라 빌드
조합임을 말한다.

큐에 넣는 자리(`queue_mirror_forward`)가 `cfg(feature = "gui")` 로 갈리고 op 를 만드는
`build_mirror_forward_op` 는 gui 전용이 된다. gui 의 동작은 그대로다.

같은 이유로 드러난 resize forward 큐(`pending_resize_forward`)는 채우는 자리
(`Core::resize_all_terminals`)도 비우는 자리도 gui 전용이었다. 그 필드를 gui 전용으로 옮긴다
([ADR-0346](0346-headless-compiles-only-what-it-reaches.md) 의 ① 갈래). 결정이 아니라 정의 정리라
여기 한 줄로만 적는다.

## Consequences

- **얻은 것**: 헤드리스에서 mirror 구조 op 가 성공으로 읽히는 길이 없어졌다. mirror 워크스페이스가
  헤드리스에 생기는 날에도(예: 헤드리스 attach client) 응답은 거짓 성공이 아니라 오류다. forward
  큐 셋(git 조회 · markdown 원문 · 구조 op)이 헤드리스에서 모두 즉시 거절로 답하고, 사유가 세 자리에서
  갈린다 — 앞의 둘은 `-32017` 문구가 메서드 이름을 싣고, 셋째는 `-32603` 의 mirror 문구다.
- **잃은 것**: 없다. 오늘 헤드리스에 mirror 워크스페이스가 없으므로 바뀐 외부 동작이 관측되지 않는다.
- **운영 비용 / 유지 부담**: `PendingStructuralForward` 는 헤드리스 라이브러리에도 남는다 — 두 조합이
  공유하는 `mark_last_forward_*` 가 큐의 마지막 원소를 표시하기 때문이다. 그 정의의
  `expect(dead_code)` 사유가 이 사실을 적는다.

## Alternatives Considered

- **A. 헤드리스에 forward drain 을 둔다.** 헤드리스가 attach client 가 되어야 의미가 있다 — 세션
  매핑(로컬 id → 원격 id)과 역반영 delta 적용이 전부 `app::attach_client` 에 있다. 결함 수정이 아니라
  새 실행 형태다.
- **B. 그대로 둔다(도달하지 않으니 괜찮다).** 도달하지 않는다는 근거가 다른 모듈의 cfg 한 줄이다.
  그 줄이 바뀌어도 이 자리에서는 컴파일도 시험도 아무것도 말하지 않는다 — 거짓 성공이 조용히
  살아난다.
- **C. IPC 핸들러에서 헤드리스일 때 `forwarded: true` 를 오류로 바꾼다.** 응답은 고쳐지지만 큐에는
  계속 쌓인다. 원인(보낼 쪽 없는 큐잉)이 남는다.

오류 코드 자체의 선택지 — Decision 은 기존 `forwarded: false` 갈래의 `-32603` 을 그대로 쓴다.
같은 값이 gui 에서 forward 할 수 없는 mirror op 에 이미 나가고 있어, 소비자가 이미 가르는 코드에
헤드리스가 합류하는 쪽이 호환을 가장 적게 바꾼다. 빌드 조합이 사유라는 것은 코드가 아니라 문구 꼬리가 말한다.

- **D. 전용 코드를 새로 둔다.** 오늘 헤드리스에 mirror 워크스페이스가 없어 그 코드를 받을 호출이 0 이다.
  관측되지 않는 갈래를 위해 공개 코드 표를 늘리고, gui 의 같은 사유(`forwarded: false`)와 코드가 갈라진다.
- **E. `invalid_params`(`-32602`) 로 답한다.** 요청은 잘못되지 않았다 — 같은 params 가 gui 에서는
  수락된다. 호출자가 params 를 고치라는 뜻으로 읽고 재시도할 자리를 찾게 된다.
- **F. `-32017` 을 재사용한다.** 그 코드는 "이 메서드의 arm 이 이 빌드에 없다" 는 뜻인데
  ([ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md)), 구조 op 메서드는 헤드리스에도
  arm 이 있고 제 할 일을 한다. 갈리는 것이 메서드가 아니라 대상이라는 것은 위 Context 셋째 문단.

## Reconsideration Triggers

**채널이 붙는 것**

- `src/core/attach_runtime.rs` 의 `dispatch_refuses_tab_create_in_mirror_workspace_in_headless` 가
  실패한다 — 헤드리스가 mirror 구조 op 를 다시 큐잉하거나 성공으로 답한다는 뜻이다. 헤드리스
  조합에서만 돈다(`check-headless`). 변이 확인: 헤드리스 `queue_mirror_forward` 가 `true` 를 돌려주게
  하면 이 시험이 죽는다(2026-09-23).
- `tests/e2e_tests.rs` 의 `mirror_forward_requests_are_refused_by_name_in_a_headless_daemon` 이
  실패한다 — 앞의 두 forward 요청 중 하나를 헤드리스가 다시 수락한다는 뜻이다. 변이 확인: 헤드리스에
  `git_viewer.query` arm 을 되살려 `request_id` 로 답하게 하면 죽는다(2026-09-23).

**원리적으로 안 붙는 것**

- 헤드리스가 attach client 가 된다(위 A). 그때 forward 셋을 헤드리스로 열고 이 거절을 거둔다. 재는 법:
  `app::attach_client` 의 모듈 선언에서 `cfg(feature = "gui")` 가 빠졌는지 본다.
- 헤드리스에 mirror 워크스페이스가 생긴다(위 A 이든 다른 경로이든). 그때 이 거절의 외부 관측이 0 이
  아니게 되므로 `-32603` 이 의미상 맞는지 — 위 D · E · F — 를 다시 본다. 재는 법: mirror 워크스페이스를
  만드는 `build_mirror_workspace` 가 헤드리스 조합에 컴파일되는지 본다 — 오늘은 그 함수가 사는
  `app::attach_client` 모듈 선언이 `cfg(feature = "gui")` 라 컴파일되지 않는다.

## References

- [headless-build-boundaries](../dev-guide/headless-build-boundaries.md) — ③ 갈래 표의 구조 op 큐 행
- [headless-ipc-surface](../dev-guide/headless-ipc-surface.md) — census 뒤에 게이트된 arm 표
- [attach-behavior](../dev-guide/attach-behavior.md) — "진입 경로별 응답 정합성"
- 선행 결정: [ADR-0425](0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md) (같은 원칙 — 헤드리스가 적용할 수 없는 요청을 수락하지 않는다 — 을 다른 요청에 적용) · [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) (`-32017` 의 뜻) · [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) (정의의 컴파일 경계)
- 코드 근거(결정이 실현된 현재 위치): `src/core/impl_mirror.rs` 의 `queue_mirror_forward` · `MirrorStructuralBlocked` 의 `Display` · `src/adapters/ipc/handler.rs` 의 `structural_apply_error`
