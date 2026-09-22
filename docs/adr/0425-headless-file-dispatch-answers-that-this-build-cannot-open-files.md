# ADR-0425: 헤드리스의 `file_handler.dispatch` 는 수락하지 않고 "이 빌드에 없다" 로 답한다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: ipc, headless, file-handler, agent-facing, build-combination, error-code
- **Group**: file-handler

## Context

`file_handler.dispatch` 는 임의 경로를 파일 열기 흐름에 넣는다. 핸들러는 `DomainIntent::DispatchFile` 을
큐에 넣고, 그 intent 결과와 무관하게 `{"accepted": true, "depth", "ignore_size_limit"}` 로 답한다. intent 는
`Core::apply` 에서 identify worker 에 넘어가고, 식별 결과가 `AppEvent::IdentifyDone` 으로 돌아와 handler 가
실행된다.

그 identify worker 와 결과를 여는 창은 **gui 빌드에만 있다.** 헤드리스(`--no-default-features`)에서는
`Core::apply` 의 `#[cfg(not(feature = "gui"))]` 갈래가 `warn!("DispatchFile dropped in headless build")` 한 줄을
남기고 `Ok(vec![])` 를 돌려줬다. 그런데 arm 은 헤드리스에도 있었다. 그래서 헤드리스 데몬은 요청을 버리면서
**수락했다고 답했다.** 실측(2026-09-21, base `290d89f56`, 격리 헤드리스 데몬):
`tasty file-handler dispatch <page.html>` → `{"accepted": true, "depth": "cheap", "ignore_size_limit": false}`,
CLI rc 0, 로그에 drop 경고 한 줄, 새 프로세스·surface 없음.

에이전트는 그 응답을 성공으로 읽는다. 에이전트가 자기 작업의 결과를 알 수 있어야 한다는
[identity](../identity.md) 의 원칙 2 에 어긋난다.

헤드리스에서 **적용할 수 없어서** arm 이 없는 메서드는 이미 있고, 그 답은 정해져 있다. 라우터 끝이
등재된 이름에 대해 `-32017`("registered but this binary has no dispatch arm for it: it is gated out of this
build combination (headless / release)")로 답한다([ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md)).
같은 모양의 선례가 `git_viewer.query` 다 — "헤드리스에 두면 accept 만 받고 결과가 안 온다" 는 이유로 arm 을
gui 에만 둔다(`src/adapters/ipc/handler.rs` 의 그 arm 주석).

## Decision

**`file_handler.dispatch` 의 dispatch arm 을 gui 빌드에만 둔다.** 헤드리스에서는 라우터 끝이 `-32017` 로
답한다. 핸들러(`handle_dispatch`)와 그 요청 타입, `DomainIntent::DispatchFile` variant, `Core::apply` 의 그
arm 도 gui 전용이 된다. 이 intent 를 만드는 자리가 헤드리스에 더는 없기 때문이다
([ADR-0346](0346-headless-compiles-only-what-it-reaches.md) — 헤드리스는 닿는 것만 컴파일한다).

gui 빌드의 동작은 바이트 단위로 같다. 응답 필드(`accepted` · `depth` · `ignore_size_limit`)도 그대로다.
`file_handler.reload` 는 헤드리스에서도 그대로 답한다 — 적용할 레지스트리가 두 빌드에 다 있다.

## Consequences

- **얻은 것**: 헤드리스에서 이 요청은 성공으로 읽히지 않는다. 에이전트와 CLI(rc ≠ 0)가 그 조합에서
  파일을 열 수 없다는 사실을 응답으로 받는다. 코드는 "이름을 의심하라" 가 아니라 "빌드 조합을 보라" 를
  뜻하는 기존 `-32017` 이다 — 새 계약이 없다.
- **얻은 것**: 헤드리스 빌드에서 `DispatchFile` 이 컴파일되지 않으므로, 그 intent 를 만드는 자리가 헤드리스에
  다시 생기면 컴파일이 먼저 알린다. 예전의 "버리고 경고" 갈래는 없어졌다.
- **잃은 것**: 헤드리스에서 이 메서드를 부르던 호출자는 이제 오류를 받는다. 그 호출자가 받던 성공은
  **거짓**이었다 — 아무 일도 일어나지 않았다 — 그래서 보존할 동작이 없다. 헤드리스에서 plugin 이
  host call 로 부르면(`com.tasty.markdown` 의 링크 열기 · 파일 열기 팝업) `-32017` 이 한 겹 감싸여
  plugin 에 돌아간다. 두 자리 모두 이미 실패를 warn 로 남긴다(`dispatch failed: …`).
- **운영 비용 / 유지 부담**: 헤드리스에서 파일 열기를 지원하려면 identify 결과를 적용할 자리(창 없이 열
  수 있는 handler — 예: OS 위임 `System`)를 먼저 헤드리스로 옮겨야 한다. 그때 이 arm 을 다시 연다.

## Alternatives Considered

- **A. 응답에 새 필드(예: `"handled": false` 와 사유)를 더하고 `accepted: true` 는 그대로 둔다.** 헤드리스
  wire 모양이 가장 적게 바뀐다. 안 고른 이유: `accepted` 를 읽는 기존 호출자는 여전히 성공으로 읽는다.
  결함이 새 필드를 아는 호출자에게서만 고쳐진다. 그리고 "이 빌드에는 없다" 를 메서드마다 다른 필드로
  말하는 새 계약이 생긴다 — 같은 사실에 이미 `-32017` 이 있다.
- **B. 헤드리스에서 `accepted: false` 로 답한다.** 필드는 그대로다. 안 고른 이유: 기존 필드의 뜻을 빌드
  조합마다 다르게 만든다. 응답이 여전히 JSON-RPC 성공이라 CLI 가 rc 0 으로 끝난다.
- **C. 헤드리스에서도 dispatch 를 실행한다**(식별을 동기로 하고 창 없이 실행할 수 있는 handler 만 돌린다).
  안 고른 이유: 결함 수정이 아니라 새 기능이다. `OpenSurface` handler 는 헤드리스에서 열 자리가 없고,
  handler 선택 규칙(picker 포함)을 헤드리스용으로 따로 정해야 한다.

## Reconsideration Triggers

**채널이 붙는 것**

- `tests/e2e_tests.rs` 의 `file_dispatch_is_refused_rather_than_accepted_in_a_headless_daemon` 이 실패한다 —
  헤드리스가 이 메서드에 다시 답하기 시작했다는 뜻이다. 변이 확인: 헤드리스 arm 을 되살려
  `{"accepted": true}` 로 답하게 하면 이 시험이 죽는다(2026-09-21).
- 헤드리스 빌드에서 `DomainIntent::DispatchFile` 을 만드는 코드가 생긴다 — 그 variant 가 gui 전용이라
  헤드리스 컴파일이 먼저 실패한다.

**원리적으로 안 붙는 것**

- 헤드리스에 창 없이 파일을 여는 handler 실행 자리가 생긴다(위 C). 그때 이 arm 을 다시 열고 응답이 실제
  결과를 싣게 한다. 재는 법: 헤드리스 데몬에 `file-handler dispatch` 를 보내 handler 가 실행되는지 본다.

## References

- [file-handler 기능 문서](../features/file-handler/index.md) — 인터페이스 절의 헤드리스 제약
- [headless-ipc-surface](../dev-guide/headless-ipc-surface.md) — gui 로 게이트된 dispatch arm 표면
- [ADR-0167](0167-a-registered-name-answers-whether-it-is-in-this-binary.md) — `-32017` 의 뜻
- [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) — 헤드리스가 닿지 않는 코드를 컴파일하지 않는 규칙
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/ipc/handler.rs` 의 `"file_handler.dispatch"` arm ·
  `src/adapters/ipc/handler/file_handler.rs` 의 `handle_dispatch` · `src/core/intent.rs` 의
  `DomainIntent::DispatchFile` · `src/core/impl_mirror.rs` 의 그 arm
