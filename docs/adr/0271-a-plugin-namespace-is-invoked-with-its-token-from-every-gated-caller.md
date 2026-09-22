# ADR-0271: plugin namespace 는 권한 셋을 가진 모든 caller 에게 그 namespace 의 토큰으로 열린다

- **Status**: Accepted
- **Date**: 2026-09-14
- **Tags**: permissions, plugin, ipc, agent, session-token
- **Group**: permission-security

## Context

권한 셋을 가진 caller 는 둘이다 — plugin 프로세스(매니페스트 ∩ grant)와 agent(세션 토큰의
base ∪ temp grant). 둘 다 모든 호출이 라우팅 전에 `CallerContext::ensure_allowed` 를 지난다
([ADR-0152](0152-gates-run-before-routing-not-inside-it.md)). 그 게이트는 `method_meta()` 가 돌려준
`required` 를 권한 셋과 대조한다.

`method_meta()` 는 `METHOD_TABLE` → `DEBUG_METHODS` → 정적 `PREFIX_RULES` → **런타임 등록 plugin
prefix** 순으로 이름을 해소한다. 마지막 갈래는 plugin 이 `[[contributes.ipc_namespace]]` 로 점유한
prefix 아래의 **표에 없는 모든 이름**을 받는데, 그 갈래가 `plugin_callable: true, required: []` 로만
답했다. 필요한 권한(`ipc.invoke:<prefix>`)은 prefix 가 이름에서만 나오므로 `&'static` 칸에 적을 수
없었고, 그 칸이 비어 있다는 사실이 게이트에게는 "권한이 필요 없다" 로 읽혔다.

그래서 경계가 caller 종류에 따라 갈렸다:

| caller | 다른 plugin 의 namespace 호출 |
|---|---|
| plugin 프로세스 | host-plugin 의 `validate_namespace_call` 이 `ipc.invoke:<prefix>` 를 요구 |
| agent (세션 토큰) | **검사 없음** — IPC 진입부의 forward 갈래가 caller 를 받고도 `None`(CLI 호출)으로 넘겼다 |

실측(2026-09-14, 격리 홈 헤드리스 데몬, `tasty session issue --agent-id zero` 로 권한 0 토큰 발급):
그 토큰으로 `tasty markdown recent` 는 `{"recent": []}`, `tasty agent-stream list` 는
`{"watches": []}` 를 받았다. 권한 셋이 있다는 것은 "이 caller 는 이만큼만" 이라는 뜻인데, 그 경계가
이 한 갈래에서만 서지 않았다. 게다가 forward 된 호출은 plugin 쪽에서 `caller_plugin_id: null` —
사용자 호출과 구분되지 않는다 — 로 도착하므로, agent 는 자기 권한 셋에 없는 일(예: 토큰에
`terminal.spawn` 이 없는데 `claude.spawn` 으로 터미널 생성)을 plugin 의 권한으로 시킬 수 있었다.

막는 쪽의 제약도 있었다. 이 경로의 가장 큰 사용자는 번들 claude plugin 이 띄운 **자식 Claude** 다.
그 자식은 claude plugin 이 발급한 토큰(`surface.read`·`surface.write`·`terminal.read`·
`terminal.write`·`notification`·`telemetry`·`agent`)을 들고, 완료 알림(`tasty claude hook stop` 등
설치된 Claude Code 훅 전부), 손자 spawn·tell, Codex 교차 검증(`tasty codex spawn`)을 전부 이 경로로
부른다. 훅 명령은 `|| true` 로 감싸여 있어 거부가 **조용히** 완료 알림 유실이 된다. 그리고 발급
규칙(`session.issue`)은 "caller 가 가진 권한의 부분집합만" 이라, claude plugin 은 자기가 쥐지 않은
`ipc.invoke:claude` 를 자식에게 넘길 수 없었다 — 자기 namespace 토큰은 plugin 에게 무용이라 매니페스트에
두지 말라는 것이 기존 규칙이다.

## Decision

**plugin namespace 의 표에 없는 이름은, 권한 셋을 가진 모든 caller 에게 그 namespace 의
`ipc.invoke:<prefix>` 를 요구한다.** plugin→plugin forward 가 이미 요구하던 같은 토큰을 agent 에게도
요구하는 것이다 — 새 토큰은 만들지 않는다. 세부는 넷이다.

1. **게이트 자리.** `MethodMeta` 에 `namespace_forward` 표시를 두고 런타임 prefix 갈래만 참으로
   답한다. `ensure_allowed` 가 그 표시를 보고 이름의 prefix 로 `ipc.invoke:<prefix>` 를 요구한다.
   게이트가 하나이므로 gui 진입부(`caller_gate`)·헤드리스 진입부(`check_permission_gate`)·plugin
   host-call 진입부(`gates_before_routing`)가 같은 답을 낸다. 거부는 `MissingPermission` 이라 agent 에게는
   capability elevation 이 그대로 발행된다 — 사용자가 승인하면 열린다.
2. **소유자 면제는 plugin 프로세스에만.** 소유 plugin 이 자기 namespace 를 부르는 것은 forward 가
   아니라 trampoline(host 가 답한다)이라 토큰을 요구하지 않는다. agent 의 `agent_id` 는 발급자가 고른
   문자열이라 plugin id 와 같게 지을 수 있으므로, 그 면제를 agent 에게는 주지 않는다.
3. **소유자는 자기 namespace 토큰을 쥐지 않고도 넘긴다.** `session.issue` 의 부분집합 규칙에 예외
   하나를 둔다: plugin 프로세스 caller 는 자기가 점유한 prefix 의 `ipc.invoke:<prefix>` 를 자식 토큰에
   넣을 수 있다. 자기 namespace 의 주인이 그 입장권을 나눠 주는 것이고, 주인 자신에게는 (2) 때문에
   쓸모가 없는 토큰이다. agent 는 여전히 받은 토큰 안에서만 넘긴다.
4. **claude plugin 의 자식 토큰.** `ipc.invoke:claude`(발급자로 돌아오는 호출)와
   `ipc.invoke:codex`(교차 검증)를 넣는다. 뒤엣것은 남의 namespace 라 넘기려면 쥐어야 하므로 매니페스트에
   선언한다(번들 plugin 은 신규 토큰이 증분 자동 grant 된다).

표에 **이름 그대로** 적힌 plugin prefix 아래 이름(`image.open`·`markdown.navigate` 등)은 이 결정
밖이다 — host 가 답하는 메서드라 표가 적은 권한만 요구한다.

## Consequences

- **얻은 것**: 권한 0 토큰이 설치된 plugin 의 namespace 를 부르지 못한다. 실측(2026-09-14, 같은 절차):
  `markdown recent` · `agent-stream list` 둘 다
  `-32001 permission_denied: 'zero' missing permission 'ipc.invoke:<prefix>'`. `ipc.invoke:markdown` 을
  준 토큰은 `markdown recent` 가 통과하고, Local 은 그대로 무검사다. 거부가 forward 앞에서 나므로
  **거부된 호출은 plugin 을 띄우지도 않는다** — 갓 만든 홈에서 권한 0 `markdown recent` 뒤
  `plugin list` 의 running 이 0 이다(고치기 전에는 같은 호출이 설치된 9 개를 전부 띄웠다).
- **얻은 것**: 한 토큰이 plugin 과 agent 에게 같은 뜻이 됐다. "그 namespace 를 부를 수 있다" 를 묻는
  자리가 두 곳(`validate_namespace_call` · `ensure_allowed`)에서 같은 토큰을 본다.
- **유지한 것**: claude plugin 이 띄운 자식의 흐름. 실측(2026-09-14, 헤드리스 데몬에서 실제
  `claude spawn` 으로 claude plugin 이 발급한 토큰을 받은 자식): `claude parent` · `claude hook stop` ·
  `codex children` 통과, `markdown recent` 는 `ipc.invoke:markdown` 없음으로 거부.
- **잃은 것**: claude plugin 이 띄우지 않은 agent 토큰(`tasty session issue` 로 사람이 발급한 것 등)이
  plugin namespace 를 쓰던 흐름은 이제 토큰에 `ipc.invoke:<prefix>` 를 적어야 한다. 안 적었으면
  elevation 승인으로 연다.
- **잃은 것**: claude plugin 이 이제 `codex.*` 를 스스로 부를 자격도 가진다. 자식에게 넘기려면 쥐어야
  한다는 부분집합 규칙의 대가다. 그 plugin 은 지금 `codex.*` 를 부르지 않는다.
- **운영 비용**: 이 결정이 고치지 않은 것 — `session.issue` 가 실패하면 claude plugin 은 토큰 **없이**
  자식을 띄우고, 토큰 없는 호출은 `Local` 이라 무검사다. 그래서 claude plugin 의 grant 에서
  `ipc.invoke:codex` 를 사용자가 거두면 발급 자체가 실패하고 자식은 오히려 무제한이 된다. 발급 실패가
  열린 쪽으로 떨어지는 것은 이 ADR 이전부터의 성질이고 별건이다.

## Alternatives Considered

- **발급자(parent)가 소유한 namespace 를 토큰 해소 시점에 암묵적으로 더한다** — 세션 레코드에
  `parent` 가 있으니 이미 떠 있는 자식도 코드 변경 없이 계속 동작한다. 그런데 `parent` 는 caller 의
  `agent_id` 문자열이라, `agent` 권한을 가진 agent 가 `agent_id` 를 plugin id 로 지어 토큰을 만들고 그
  토큰으로 다시 발급하면 손자의 `parent` 가 그 plugin id 가 되어 남의 namespace 가 열린다. 막으려면
  레코드에 발급자 종류를 새로 적어야 하고, 그러면 옛 레코드는 어차피 암묵 권한을 못 받는다 —
  "기존 토큰 호환" 이라는 장점이 사라진다. 게다가 host 교체는 재시작이고 재시작은 PTY 자식을 전부
  끝내므로, 옛 토큰을 든 자식이 새 게이트를 만나는 경우가 드물다.
- **forward 되는 이름을 `METHOD_TABLE` 에 올린다** — plugin 이 자기 메서드를 host 표에 등재해야 한다.
  서드파티 plugin 은 host 소스를 못 고치고, 표는 host 가 답하는 이름을 가려내는 데도 쓰인다
  (`is_registered_name`) — plugin 이름을 섞으면 그 판정이 무너진다.
- **plugin 이 알아서 판정한다(지금이 의도)** — forward 는 caller 를 `None` 으로 넘겨서 plugin 은
  사용자와 agent 를 구분할 재료조차 없다. 재료를 넘기더라도 모든 plugin 이 각자 권한 모델을 다시
  구현해야 하고, 빠뜨린 plugin 이 곧 구멍이다. 게이트가 한 곳이라는 ADR-0152 의 결정과도 어긋난다.
- **agent 에게 plugin namespace 를 통째로 닫는다** — 자식 Claude 의 완료 알림이 전부 조용히 끊긴다.
  원칙 2(에이전트 기능은 IPC + CLI 양면)와 정면으로 충돌한다.
- **claude plugin 매니페스트에 `ipc.invoke:claude` 를 선언해 부분집합 규칙을 그대로 통과한다** —
  발급 규칙을 안 건드려도 되지만, "자기 namespace 토큰은 무용이라 두지 않는다" 는 규칙을 뒤집고 모든
  발급 plugin 이 자기 이름을 자기에게 grant 받아야 한다. 사용자가 그 grant 를 거두면 발급 자체가 실패해
  위 운영 비용의 열린 갈래로 떨어진다. 주인이 자기 입장권을 나눠 주는 데 허락이 필요하지 않다고 본다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `session.issue` 를 부르는 plugin 이 claude 말고도 생긴다 — 그 plugin 의 자식이 어느 namespace 를
  부르는지 세고 토큰 목록을 같은 방식으로 정해야 한다. 재는 법: `"session.issue"` 를 host.call 로 부르는
  자리를 `crates/tasty-plugin-*/src` 에서 센다.
- claude plugin 자식이 `claude`·`codex` 밖의 namespace 를 기본 흐름에서 부르게 된다(예: agent-stream
  훅) — 자식 토큰 목록과 매니페스트 선언을 함께 늘려야 한다.
- 세션 레코드가 발급자 종류(plugin 프로세스인가)를 적게 된다 — 첫 대안의 위조 반론이 사라지므로
  암묵 부여를 다시 따진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 자식 Claude 의 완료 알림이 거부로 끊기는 일이 보고된다. 재는 법: 자식 셸에서
  `tasty claude hook stop </dev/null` 을 직접 실행해 `-32001 … ipc.invoke:` 가 나오는지 본다(훅은
  `|| true` 라 평소엔 안 보인다).

## References

- 운영 서술: [`docs/dev-guide/plugin-permissions.md`](../dev-guide/plugin-permissions.md) — 토큰 표의
  `ipc.invoke:<prefix>` 행, "Agent caller", "비-Local caller 가 유발할 수 있는 plugin 수명주기"
- 게이트 순서: [ADR-0152](0152-gates-run-before-routing-not-inside-it.md)
- 소유 표 해소: [ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) ·
  [ADR-0179](0179-the-resolver-is-handed-the-table-not-a-callback.md)
- 이 경로를 별건으로 남긴 자리: [ADR-0259](0259-a-kind-request-starts-the-owner-that-declares-it.md)
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-ipc/src/method_meta.rs` 의 `MethodMeta::namespace_forward`
  · `plugin_owns_prefix`, `crates/tasty-ipc/src/caller.rs` 의 `check_permissions`,
  `src/adapters/ipc/handler/session.rs` 의 `caller_may_grant`, `crates/tasty-plugin-claude/src/handlers.rs`
  의 `issue_session_token`
