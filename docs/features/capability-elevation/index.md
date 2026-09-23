# 권한 상승 & 감사 (Capability elevation & audit)

- **Status**: Implemented
- **주체**: AI Agent (요청) · 로컬 사용자 (승인) · 운영자 (audit 조회)
- **ADR**: 없음
- **코드**: dispatcher 권한 게이트(`handler.rs`), `src/adapters/ipc/audit.rs`, session/elevation 핸들러
- **화면**: capability_elevation approval popup
- **메서드 목록**: [reference/api](../../reference/api.md#plugin-관리-plugin-local-only)

## 목적

plugin/agent 가 IPC 호출 시 호스트가 권한을 강제하고, 부족하면 **사용자에게 popup 으로 즉시 elevation** 을 받고, **거부된 호출을 audit log 에 영속**한다. 매니페스트 권한([plugin-permissions](../../dev-guide/plugin-permissions.md)) 위에 얹은 런타임 권한 모델이다.

## 내부 동작

### 세 축

1. **권한 평가** — `method_meta` 가 메서드별 필요 권한 선언. caller 의 권한 셋에 모두 포함돼야 통과.
2. **Capability elevation** — Agent 가 권한 부족으로 거부되면 자동 popup.
3. **Audit log** — IPC 의 deny 결정을 dispatcher 단일 진입점에서 영속(allow 는 기록 안 함 — 아래).

### Agent session 권한

claude.spawn 등으로 띄운 자식은 `session.issue` 로 토큰 발급(base permissions = 부모 권한의 부분집합, escalation 금지 — 단 plugin 은 자기 namespace 의 `ipc.invoke:<prefix>` 를 쥐지 않고도 넘긴다). plugin namespace 메서드(표에 이름이 없는 것)는 agent 에게도 그 namespace 의 `ipc.invoke:<prefix>` 를 요구한다 — 없으면 아래 elevation 으로 연다([ADR-0271](../../adr/0271-a-plugin-namespace-is-invoked-with-its-token-from-every-gated-caller.md)). 자식은 모든 호출에 `TASTY_SESSION_TOKEN` 을 envelope `session_token` 으로 첨부 → 호스트가 `CallerContext::Agent` 구성. invalid/expired/revoked 토큰은 `-32001`(Local fallback 안 함 — 위조 방어). 런타임 추가 grant 는 `plugin.grant_agent_permission`(TTL 가능, base 와 분리 슬롯).

### Elevation flow

Agent 가 권한 부족으로 거부되면 호스트가 (같은 (agent, permission) Pending 없을 때) `approval.request{kind=capability_elevation, choices=[approve, approve_permanently, deny]}` 자동 발행. 거부 응답의 `error.data` 에 `{approval_id, permission, method}` 첨부 → agent 가 `approval.await` 폴링 → 사용자 선택에 따라 임시(TTL)/무기한 grant 적용 → agent 재호출 시 통과. agent 가 `plugin.request_permission` 으로 **미리** 발행할 수도 있다(거부 대기 없이). 메커니즘은 [human-handoff](../human-handoff/index.md) 위에 얹힘.

### Audit log

레코드: `ts_ms, seq, caller_kind(local/plugin/agent), caller_id, method, decision(allow/deny), reason?, workspace_id?`. 영속 키 `tasty.audit.{ts}.{seq}`(global, query 시 lazy evict). `seq` 는 telemetry 와 공유 단조 증가.

**기록되는 것은 deny 뿐이다.** allow 는 method 와 무관하게 저장하지 않는다 — 폴링형 에이전트 워크로드에서 allow 가 초당 14건씩 영구 레코드를 만들어 `memory.db` 의 최대 유입원이 됐기 때문이다(18시간 실행 371,936행, 그중 deny 0건). 그래서 이 로그는 **평시에 사실상 빈 테이블**이고, "agent 가 승인된 권한으로 실제 무엇을 했나" 를 사후에 되짚는 용도는 없다. 반대로 deny 는 어떤 method 든 전부 남으므로 권한 거부 사고 추적은 그대로다. 결정의 근거·대안·재검토 조건은 [ADR-0085](../../adr/0085-ipc-log-retention-bounded.md).

elevation 자체는 이 로그에 의존하지 않는다 — 승인 이력은 `tasty.approval.*` 로 별도 영속되고, elevation 발행 트리거도 deny 경로다.

보존: 관측 로그 3종 공통 정책(`store::log_retention`)을 따른다 — 50시간 + 5만 건 상한이며 부팅과 런타임 양쪽에서 집행된다.

## 인터페이스

- **AI Agent**: `session.issue`(AgentManage), `plugin.request_permission`(Approval), `plugin.list_agent_permissions`(readonly).
- **운영자/CLI (local-only)**: `plugin.{grant,revoke}_agent_permission` · `plugin.audit_{query,summary,follow,clear}`(`tasty plugin audit-query/summary/follow/clear` — follow 는 CLI 측 폴링).

## 관련

- [dev-guide/plugin-permissions](../../dev-guide/plugin-permissions.md) — 권한 토큰/매니페스트 · [human-handoff](../human-handoff/index.md) — approval 메커니즘
- [identity](../../identity.md) — 사용자/에이전트 분리
