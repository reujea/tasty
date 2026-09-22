# ADR-0141: 호스트 키 namespace `tasty.` 는 raw `memory.*` kv 표면에서 예약한다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: security, permissions, memory, ipc, plugin, audit
- **Group**: permission-security

## Context

regular memory 는 **설계상 공유 네임스페이스**다 — `tasty_memory` 모듈 doc 이 "모든 caller 가
읽지만, 갱신·삭제는 owner 본인 또는 `_host` 만 가능" 이라고 적고 있고, `MemoryStorage` 의
`get`/`list`/`query`/`export` 에는 owner 인자가 **아예 없다**. plugin 별로 비공개가 필요한
데이터를 위해서는 secret 영역(`memory.secret.*`, owner 가 PK 일부)이 따로 있다.

그런데 호스트가 자기 상태를 그 공유 네임스페이스에 두면서, 그 데이터로 가는 **전용 문만**
잠갔다. 감사 로그가 그 예다.

- `plugin.audit_query` · `audit_summary` · `audit_follow` · `audit_clear` 는 넷 다
  `local_only()` 다 — plugin 은 어떤 권한을 들고도 못 부른다.
- 같은 행이 `Scope::Global` + 키 prefix `tasty.audit.` 로 memory store 에 앉아 있다.
- `memory.get/list/query/export` 는 `memory.read`, `memory.put/delete/import` 는
  `memory.write` 만 요구한다.

측정한 결과는 두 방향이었다(store 수준 재현).

- **읽기 — 열려 있다.** 읽기 API 에 owner 차원이 없으므로 `memory.list(scope=global,
  prefix="tasty.audit.")` 이 다른 caller 의 권한 거부 기록 전부를 돌려준다.
- **위조 — 열려 있다.** owner 검사는 *이미 있는 행* 만 지킨다. 아직 없는 키
  (`tasty.audit.9999`)는 누구든 만들 수 있고, 심은 행은 호스트 자신의 prefix 조회에
  그대로 섞인다. 즉 감사 기록을 **날조**하고 다른 caller 에게 뒤집어씌울 수 있다.
- **삭제 — 막혀 있다.** 기존 행은 owner 가 `_host` 라 plugin 의 `memory.delete` 는
  `OwnedByOther` 로 거부된다. (최초 제보는 이 방향도 열린 것으로 보았으나 재현 결과
  닫혀 있었다. 대신 제보에 없던 위조 방향이 열려 있었다.)

사용자가 승인 화면에서 본 것은 "메모리 읽기/쓰기" 다. **승인의 의미와 실제 권한이 어긋난다.**

이 형태는 메서드 단위 대조로는 안 잡힌다. `plugin.audit_*` 와 `memory.*` 는 각자 일관되고,
**조합에서만** 샌다.

호스트가 쓰는 키 namespace 는 감사 로그 하나가 아니다 — `tasty.audit.` ·
`tasty.telemetry.` · `tasty.agent.` · `tasty.approval.` · `tasty.commands.` ·
`tasty.session.` · `tasty.startup.` · `tasty.bb.` · `tasty.plan.` · `tasty.cache.` ·
`tasty.observer.` 로 11 개이며 5 개 크레이트에 흩어져 있다. 그리고 이들은 `Global`
하나에 모여 있지 않다 — `tasty.bb.`/`plan.`/`cache.` 는 `Workspace`,
`tasty.commands.` 는 `Surface`, `tasty.approval.` 은 둘 다 쓴다.

## Decision

**접두 `tasty.` 로 시작하는 키는 호스트 소유로 예약하고, 권한 게이트를 받는 caller
(plugin / agent)의 raw `memory.*` kv 표면에서 존재하지 않는 것으로 다룬다.**

경로의 모양에 따라 진입점이 셋이다.

- **키를 지목하는 경로**(`put` · `get` · `delete` · `exists` · `import`) — 거부한다.
- **열거하는 경로**(`list` · `query` · `export`) — 결과에서 제거한다. prefix 없이도 불릴 수
  있어 "지목했는가" 로 가를 수 없으므로 거부가 아니라 필터여야 한다.
- **세는 경로**(`count`) — 센 수에서 뺀다. 내용을 못 봐도 개수는 감사 기록의 존재와 규모를
  드러낸다.

`Local`(CLI·사용자)은 제외한다 — `ensure_allowed` 가 무조건 통과시키는 신뢰 caller 이고,
`tasty memory list --prefix tasty.audit.` 은 그대로 동작해야 한다. 판정은 `is_plugin()` 이
아니라 **권한 셋의 유무**(`CallerContext::permissions().is_some()`)로 한다 — agent caller 도
같은 권한 모델을 받아야 하고, 권한을 받는 caller 종류가 새로 생겨도 자동으로 덮인다.

정책은 **IPC 핸들러 층**에 둔다. 호스트 내부 코드는 store 를 직접 호출하므로 영향받지
않고, 정책이 필요한 caller 신원은 그 층에만 있다.

## Consequences

- **얻은 것**: 감사 로그가 읽기·위조 양방향으로 닫힌다. 같은 조치가 호스트 namespace
  11 개를 한꺼번에 덮는다 — 새 호스트 namespace 가 생겨도 자동으로 포함된다.
- **잃은 것**: plugin 이 raw kv 로 `tasty.bb.` · `tasty.plan.` · `tasty.cache.` 내부 키를
  직접 훑던 경로가 닫힌다. 전용 메서드(`memory.bb_*` · `plan_*` · `cache_*`)가 같은
  데이터의 지원되는 표면이고, 그쪽은 자기 prefix 로만 키를 조립하므로 영향이 없다.
  레포 안에서 그렇게 쓰는 곳은 실측 0 건이었다.
- **남는 구멍**: `memory.scopes` 는 scope 토큰만, `memory.stats` 는 집계 개수·바이트만
  돌려준다 — 키 이름도 값도 내보내지 않으므로 그대로 둔다. 이 둘은 테스트의 면제 목록에
  이름으로 적혀 있고, 그 목록이 이 정책이 답하지 않는 자리의 전부다.
- **운영 비용**: 접두가 하나라 유지할 목록이 없다. 새 raw kv 핸들러가 정책을 안 거치면
  소스 가드(`every_raw_kv_handler_consults_the_reserved_namespace`)가 잡는다.

## Alternatives Considered

- **A: plugin caller 에게 `Scope::Global` 을 닫는다** — 원리적 부분해다. 호스트 상태가
  Global·Workspace·Surface 세 scope 에 다 걸쳐 있어(`tasty.bb.` 등은 Workspace,
  `tasty.commands.` 는 Surface) 감사 로그만 덮고 나머지는 그대로 샌다. 동시에 범위는 더
  넓다 — Global 을 정당하게 쓰는 기존 plugin 상태까지 깨진다. **덜 막고 더 깨는** 선택이다.
- **B: 감사 로그를 memory store 밖으로 뺀다** — 감사 로그 하나는 확실히 닫히지만 나머지
  호스트 namespace 10 개는 그대로 열려 있다. 보존·GC(`log_retention`)가 memory store 위에
  올라가 있어 파장도 가장 크다. 이 ADR 의 조치와 배타적이지 않으므로, 저장 위치를 옮길
  이유가 따로 생기면 그때 별도로 판단한다.
- **C: 예약할 하위 namespace 를 목록으로 든다** — 목록이 또 하나의 손목록이 된다. 새
  호스트 namespace 가 생길 때마다 조용히 새는 자리가 늘고, 그 목록을 지키는 가드가 다시
  필요해진다. 접두 하나면 목록 자체가 없다.

## Reconsideration Triggers

- regular memory 의 읽기 경로가 owner 를 인자로 받게 되어 "모든 caller 가 읽는다" 는
  전제가 바뀔 때. 그 경우 `tasty-memory` 의
  `ownership_does_not_reserve_a_key_namespace` 가 먼저 깨진다.
- plugin 이 `tasty.` 로 시작하는 키를 정당하게 써야 하는 요구가 생길 때 — 그때는 접두
  예약 대신 owner 기반 가시성으로 옮기는 것이 맞다.
- `memory.stats` 의 집계값이 키 단위 분해를 노출하도록 바뀔 때. 면제의 근거가 사라진다.

## References

- [plugin-permissions](../dev-guide/plugin-permissions.md) — 권한 토큰과 그 토큰이 여는 메서드
- [ADR-0094](0094-surface-id-space-bounded-below-pty-base.md) — 같은 핸들러가 scope 파라미터에
  거는 다른 경계
- [ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md) — 같은 형태의
  결정을 **IPC 메서드 prefix** 에 내린 것. 그쪽은 예약 목록을 파생하지 않고 가드로 고정했고
  (집행 지점이 매니페스트 검증이라 목록이 필요했다), 이쪽은 집행 지점이 핸들러라 접두 하나로
  줄일 수 있었다 — 두 결정이 갈린 이유가 집행 지점이다
- `src/adapters/ipc/handler/memory.rs` (`HOST_KEY_NAMESPACE`) · `src/store/audit.rs`
  (`AUDIT_KEY_PREFIX`) · `crates/tasty-memory/src/lib.rs` (owner 모델)
