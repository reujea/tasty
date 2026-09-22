# 휴먼 핸드오프 (Approval)

- **Status**: Implemented
- **주체**: AI Agent (요청) · 로컬 사용자 (응답)
- **ADR**: 없음
- **코드**: `approval.*` 핸들러(`src/adapters/ipc/handler/approval/`), 영속 `tasty-memory`
- **화면**: approval popup (pending 큐 head)
- **메서드 목록**: [reference/api](../../reference/api.md#휴먼-핸드오프-approval)

## 목적

에이전트가 위험한 동작 전에 사용자 결정을 **동기적으로** 받는 결정 게이트. 단방향 [알림](../notifications/index.md)과 달리 **요청-응답 워크플로우**다.

## 내부 동작

### 흐름

`approval.request` → 큐 push + popup 노출 + `notification.create` 자동 발화 → 에이전트가 `approval.await(id)` 로 blocking 대기 → 사용자 응답 → await 반환. 응답 경로 3가지가 모두 같은 `approval.respond` 로 수렴: popup 버튼 클릭 · popup 단축키 `1..=9`(선택지 순서) · CLI `tasty approval respond`.

### severity 별 표시

| severity | 채널 |
|----------|------|
| `danger` | popup + notification, **사용자 직접 응답만**(self-response 거부) |
| `warn` | popup + notification |
| `info` | notification only |

popup 은 `pending_approval_ids` 큐의 head 를 그리고, 응답 시 pop 하고 다음으로 자동 이동. **Esc 는 의도적으로 차단**(우회 응답 방지).

### 응답 규칙

self-response(같은 plugin 이 자기 요청에 응답)는 `-32011`, 이미 종료된 요청은 `-32010`. `approval.await` 는 local-only(plugin 호출 deadlock 방지) — `outcome ∈ {responded, timed_out, cancelled}`.

store 락이 poison 된 뒤(다른 스레드가 락을 든 채 패닉)의 `request`/`respond`/`cancel` 은 `-32014`(`store_poisoned`) 로 **거절**한다. 그 임계구역은 `state` → `history` → `waiters` 를 순서대로 갱신하므로 중간 상태가 남을 수 있고, 승인은 에이전트 행동의 관문이라 신뢰할 수 없는 기록 위에서 전이를 이어가지 않는다. 반면 `get`/`list` 는 표시용 읽기라 복구해서 계속 답한다. 어느 쪽도 패닉하지 않는다 — 이 store 는 승인 popup(메인 스레드)이 함께 쓰므로 패닉이 모든 창의 터미널 세션을 죽인다([error-handling](../../dev-guide/error-handling.md) "락 poison").

### 귀속 워크스페이스

요청이 묶일 워크스페이스는 **명시 `workspace_id` → `surface_id` 가 사는 워크스페이스 → 그 창의 활성 워크스페이스** 순으로 정한다. surface 를 댄 요청은 사용자가 어느 워크스페이스를 보고 있든 같은 곳에 묶인다. 둘 다 안 준 요청만 활성 워크스페이스로 떨어지고 그 값은 재현되지 않는다 — 재현이 필요하면 둘 중 하나를 준다([ADR-0533](../../adr/0533-an-omitted-target-keeps-its-focus-default-only-where-nothing-names-one.md)). 이 값은 아래 영속 스코프와 함께 뜨는 알림의 워크스페이스를 정한다. popup 자체는 창 단위라 이 값에 안 걸린다.

### 영속

매 상태 전이마다 `tasty.approval.<id>` 키로 직렬화(workspace_id 있으면 `workspace:<id>`, 없으면 `global`). 응답/타임아웃/취소 후에도 보존 → `approval.history` 로 재시작 후 조회. 세션 요약은 별도 키(`approval.summary.set/get`, markdown).

## 인터페이스

- **AI Agent**: `approval.request`(`Approval` 권한) → `approval.await`. capability elevation·cap require_approval 도 같은 메커니즘 위에 얹힌다([capability-elevation](../capability-elevation/index.md), [telemetry](../telemetry/index.md)).
- **사용자**: popup 응답 / `tasty approval {request,respond,await,get,list,history,summary}`.

## 관련

- [notifications](../notifications/index.md) · [capability-elevation](../capability-elevation/index.md) · [telemetry](../telemetry/index.md)
- [reference/api](../../reference/api.md#휴먼-핸드오프-approval)
