# 권한 상승 & 감사 (Capability elevation & audit)

- **Status**: Implemented
- **주체**: AI Agent (요청) · 로컬 사용자 (승인) · 운영자 (audit 조회)
- **설계**: [호스트 호출 계약](../../dev-guide/plugin-permissions.md) (신뢰 모델)
- **코드**: dispatcher 권한 게이트(`handler.rs`), `src/adapters/ipc/audit.rs`, session/elevation 핸들러, 승인 응답의 권한 부여(`src/app/approval_grant.rs`)
- **화면**: capability_elevation approval popup
- **메서드 목록**: [reference/api](../../reference/api.md#plugin-관리-plugin-local-only)

## 목적

agent 가 IPC 호출 시 호스트가 권한을 강제하고, 부족하면 **popup으로 사용자에게 추가 권한을 요청**하고, **거부된 호출을 audit log 에 영속**한다. Agent 세션 권한([plugin-permissions](../../dev-guide/plugin-permissions.md)) 위에 추가한 런타임 권한 모델이다. 플러그인은 권한으로 막지 않으므로 이 흐름의 대상이 아니다.

## 내부 동작

<a id="세-축"></a>

### 세 가지 역할

1. **권한 평가** — `method_meta` 가 메서드별 필요 권한 선언. Agent caller 의 권한 셋에 모두 포함돼야 통과. Plugin caller 는 plugin 에 연 메서드면 통과한다.
2. **Capability elevation** — Agent 가 권한 부족으로 거부되면 자동 popup. 키보드 포커스는 가져가지 않는다([passive 팝업](../../design/systems/popup.md#passive-팝업)).
3. **Audit log** — IPC 의 deny 결정을 dispatcher 단일 진입점에서 영속(allow 는 기록 안 함 — 아래).

### Agent session 권한

claude.spawn 등으로 띄운 자식은 `session.issue` 로 토큰 발급(Agent 부모의 발급은 자기 권한의 부분집합으로 제한, escalation 금지 — Local·plugin 부모는 어떤 토큰이든 자식에게 부여할 수 있다). plugin namespace 메서드(표에 이름이 없는 것)는 agent 에게도 그 namespace 의 `ipc.invoke:<prefix>` 를 요구한다 — 없으면 아래 elevation 으로 연다([호스트 호출 계약](../../dev-guide/plugin-permissions.md)). 자식은 모든 호출에 `TASTY_SESSION_TOKEN` 을 envelope `session_token` 으로 첨부 → 호스트가 `CallerContext::Agent` 구성. invalid/expired/revoked 토큰은 `-32001`(Local fallback 안 함 — 신원을 밝히려 한 요청이 조용히 사용자 권한으로 실행되지 않게 한다). 런타임 추가 grant 는 `plugin.grant_agent_permission`(TTL 가능, base 와 분리 슬롯).

### Elevation flow

Agent 가 권한 부족으로 거부되면 호스트가 (같은 (agent, permission) Pending 없을 때) `approval.request{kind=capability_elevation, choices=[approve, approve_permanently, deny]}` 자동 발행. 거부 응답의 `error.data` 에 `{approval_id, permission, method}` 첨부 → agent 는 같은 호출을 다시 시도한다(사용자가 답하기 전에는 같은 `approval_id` 로 다시 거부된다). `approval.await` 는 local-only 라 세션 토큰을 쓰는 agent 는 부를 수 없고, `Approval` 권한이 있으면 `approval.get` 으로 상태를 볼 수 있다 → 사용자 선택에 따라 임시(TTL)/무기한 grant 적용 → agent 재호출 시 통과. 사용자는 GUI 승인 popup 이나 Local caller(사용자 CLI)의 `approval.respond` 로 답하고, 두 진입점 모두 App 의 `respond_approval` 이 응답 전이 직후·기록 제출 전에 같은 grant 를 적용한다. `approve` 는 요청의 `grant_ttl_secs`(기본 1시간) 동안, `approve_permanently` 는 무기한 부여하고 `deny`·취소·만료는 부여하지 않는다. 그 agent 의 활성 세션이 없으면 grant 는 경고 로그만 남기고 승인 응답은 그대로 유지된다.

grant 대상(agent·권한·유효 시간)은 host 가 권한 거부·`plugin.request_permission` 으로 요청을 만들 때 기록 구조의 host 전용 필드(`ApprovalRequest.host_elevation`)에 담는다. 이 필드는 직렬화하지 않으므로 IPC 요청이나 저장된 기록으로 채울 수 없다. grant 와 같은 (agent, permission) Pending 재사용은 이 필드로만 판정하고, metadata 의 `kind`·`agent_id`·`permission`·`grant_ttl_secs` 는 조회용 사본일 뿐 grant 근거가 아니다. 공개 `approval.request` 가 metadata `kind` 에 `capability_elevation` 을 주면 `-32602`(`invalid_request: …`)로 거절한다 — 승인 권한만 가진 caller 가 권한 상승 요청을 흉내 내 다른 agent 에게 권한을 붙이지 못하게 한다. agent 가 `plugin.request_permission` 으로 **미리** 발행할 수도 있다(거부 대기 없이).

**권한 상승 요청은 사용자만 종결한다.** `host_elevation` 이 있는 기록에 사용자(GUI popup·Local caller, `Responder::User`)가 아닌 응답자 — agent 세션·plugin — 가 `approval.respond` 를 보내면 선택지(`approve`·`approve_permanently`·`deny`)와 관계없이 `-32019`(`user_response_required: <id>`)로 거절하고 상태를 바꾸지 않는다. 같은 caller 의 `approval.cancel` 도 같은 오류로 거절한다. 검사는 승인 저장소의 응답 전이 안에서 하므로 grant 보다 앞선다. 거절·취소까지 막는 이유: agent 가 사용자의 판단보다 먼저 다른 agent 의 요청을 닫으면, 사용자는 popup 에서 그 요청을 볼 수 없고 대상 agent 는 이유 없이 거부된다. 만료는 elevation 요청에 `timeout_ms` 가 없고 `approval.await` 가 local-only 라 세션 토큰을 쓰는 agent 는 일으킬 수 없다. 일반 승인 요청(`host_elevation` 없음)은 `Approval` 권한을 가진 agent 와 plugin 도 응답·취소할 수 있다. 이 보장은 세션 토큰으로 접속한 agent 와 plugin 에 대한 것이고, 토큰 없이 접속한 프로세스는 로컬 TCP 접속 = 사용자라는 정의대로 사용자로 취급된다.

`plugin.request_permission` 의 Agent caller 는 자기 자신만 대상으로 지정한다. `agent_id` 를 생략하거나 자기 agent_id 를 주면 되고, 다른 값은 `-32602` 로 거절한다. Plugin·Local caller 는 대상 agent 를 지정할 수 있다. 이 두 제한이 없으면 `Approval` 권한을 가진 agent 가 다른 agent 를 대상으로 요청을 만들거나(또는 다른 agent 의 거부로 생긴 요청을 찾아) 스스로 승인해 사용자 개입 없이 권한을 붙일 수 있다. 메커니즘은 [human-handoff](../human-handoff/index.md) 위에 얹힘.

### Audit log

레코드: `ts_ms, seq, caller_kind(local/plugin/agent), caller_id, method, decision(allow/deny), reason?, workspace_id?`. `local` 의 `caller_id` 는 호출자가 요청 봉투로 밝힌 자기 신고 agent ID 이고 없으면 빈 값이다([호스트 호출 계약](../../dev-guide/plugin-permissions.md)). 영속 키 `tasty.audit.{ts}.{seq}`(global, query 시 lazy evict). `seq` 는 telemetry 와 공유 단조 증가.

**권한 거부(`deny`)만 기록한다.** 허용된 호출까지 저장하면 반복 조회가 대부분인 에이전트
작업에서 불필요한 데이터가 계속 쌓인다. 따라서 이 로그로 허용된 작업의 수행 이력을
추적할 수는 없다. 거부는 메서드와 관계없이 모두 기록한다.
근거와 재검토 조건은 [저장소 시스템 (Storage System)](../../design/systems/storage.md)를 따른다.

elevation 자체는 이 로그에 의존하지 않는다 — 승인 이력은 `tasty.approval.*` 로 별도 영속되고, elevation 발행 트리거도 deny 경로다.

보존: 관측 로그 3종 공통 정책(`store::log_retention`)을 따른다 — 50시간 + 5만 건 상한이며 부팅과 런타임 양쪽에서 집행된다.

### 무엇을 막는 장치인가

agent 의 세션 토큰은 요청이 어느 agent 의 것인지 구분하는 표지이고, agent 권한과 권한 상승 승인은 협조적인 agent 의 실수를 막고 의도를 드러내는 장치다. agent 는 사용자가 자기 컴퓨터에서 띄운 프로세스이므로 agent 와 사용자 사이에는 보안 경계를 두지 않는다. 토큰을 지운 agent 의 요청이 사용자 요청으로 처리되는 것은 결함이 아니라 정의된 동작이다. 플러그인에도 별도 실행 보안 경계를 두지 않으며 설치 뒤 권한으로 막지 않는다. 인증·미인증 설치 경고 정책은 [플러그인 원칙](../../principles/plugins.md)에 있다. Agent 권한 검사도 호스트 IPC 호출만 제한한다([권한 게이트의 한계](../../dev-guide/plugin-permissions.md#한계)). 근거와 기각한 대안은 [호스트 호출 계약](../../dev-guide/plugin-permissions.md)에 있다.

## 인터페이스

- **AI Agent**: `session.issue`(AgentManage), `plugin.request_permission`(Approval), `plugin.list_agent_permissions`(readonly).
- **운영자/CLI (local-only)**: `plugin.{grant,revoke}_agent_permission` · `plugin.audit_{query,summary,follow,clear}`(`tasty plugin audit-query/summary/follow/clear` — follow 는 CLI 측 폴링).

## 관련

- [dev-guide/plugin-permissions](../../dev-guide/plugin-permissions.md) — 호출자별 판정·Agent 권한 토큰 · [human-handoff](../human-handoff/index.md) — approval 메커니즘
- [사용자 통제와 데이터](../../principles/control-and-data.md#사용자에이전트원격-사용자) — 사용자/에이전트 구분
