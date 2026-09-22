# ADR-0533: 대상을 생략한 요청의 포커스 기본값은 아무것도 대상을 대지 않은 자리에만 남는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: focus, identity-principle-3, ipc, cli, approval, telemetry, audit, notification, default-value, compatibility, adr-0470, adr-0471
- **Group**: window-workspace-lifecycle

## Context

[ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md) 은 IPC 엔진 핸들러가 창의
활성 워크스페이스를 `IpcWindow::active_workspace_index` 로만 읽게 했고, 그 값으로 무엇을 할지는
"정하지 않았다" 고 적었다(Consequences "잃은 것" 첫 항). 이 ADR 이 그 재결정이다.

실측(2026-09-23, 착수 트리 `6bd3167ee`). 출하 코드만 남긴 사본(`strip-cfg-test`)에서
`src/adapters/ipc` 의 `active_workspace_index` 호출은 **11 자리**다(debug 파일의 직접 읽기 1 과
포트 구현 `state/ipc_window.rs` 는 따로). 이 작업을 연 메모가 적은 "15 자리" 는 ADR-0470 착수
트리(`81fd484b8`)의 `state.active_workspace` 계수였고 ADR-0471 은 같은 것을 13(debug 2 포함)으로
셌다 — 그 뒤 판정기가 달라 지금 수와 직접 비교되지 않는다. 11 자리를 **그 값이 무엇을 정하는가**로
가르면 넷이다.

| # | 자리 | 무엇을 정하나 | 요청 인자로 대상을 댈 수 있나 |
|---|---|---|---|
| 1 | `system.info` 응답(`handler.rs`) | 응답의 활성 index·id | 보고라 대상이 없다 |
| 2 | `tree` 의 활성 플래그(`handler.rs`) | 응답 표시 | 보고 |
| 3 | `workspace.list` 의 활성 플래그(`workspace.rs`) | 응답 표시 | 보고 |
| 4 | `check_request` 의 게이트 태그(`checked.rs`) | 거부·허용 audit 행과 telemetry 의 workspace 열 | 없다 — 게이트는 라우팅 전에 모든 메서드에 한 번 돈다 |
| 5 | `telemetry.record`(`record.rs`) | 이벤트 귀속 | `workspace_id`(IPC) · `--workspace-id`(CLI) |
| 6 | `telemetry.record_batch`(`record.rs`) | 이벤트 귀속 | 이벤트마다 `workspace_id` |
| 7 | `approval.request`(`approval/request.rs`) | 요청 귀속(영속 스코프 · 함께 뜨는 알림의 워크스페이스) | `workspace_id` · `surface_id` 둘 다 IPC·CLI 에 있다 |
| 8 | 권한 부족 격상 발행(`approval.rs` `publish_capability_elevation`) | 격상 요청의 귀속 | 없다 — 호스트가 거부 응답 안에서 스스로 발행한다 |
| 9 | 상한 `require_approval`(`cap.rs`) | 승인 요청의 귀속(영속 스코프 `workspace:<id>` · 함께 뜨는 알림) | 요청 인자로는 없다 — 발화 경로 둘(⒤ · ⒥) 중 ⒤ 명시 `telemetry.record` 에서만 계기 이벤트의 `workspace_id` 가 뜻을 갖는다(아래 ⒞) |
| 10 | 상한 `notify`(`cap.rs`) | 알림을 붙일 워크스페이스 | 9 와 같다 |
| 11 | 이상 탐지 알림(`anomaly.rs`) | 알림을 붙일 워크스페이스 | 요청 인자로는 없다 — 발화 경로 셋(⒤ · ⒥ · ⒦) 중 ⒤ 에서만 계기 이벤트의 `workspace_id` 가 뜻을 갖고, ⒦ 에는 계기 요청이 아예 없다(아래 ⒞) |

어느 자리도 요청이 **작용할 대상**을 포커스로 고르지 않는다 — 대상을 고르는 일은 라우터가
요청이 실은 id 로 하고, 위 값은 전부 보고 · 귀속 · 알림 배치다. 흔들리는 것은 **같은 요청을 다시
불렀을 때 기록이 어디에 묶이는가**다. 그중 7 은 형태가 다르다: 호출자가 `surface_id` 를 대서
대상이 어디 있는지 이미 말했는데, 귀속은 그 surface 가 아니라 사용자가 보고 있는 워크스페이스를
따랐다. surface 는 워크스페이스 B 에 있고 기록은 A 에 묶여, B 를 닫으면 사라져야 할 영속 기록이
A 의 스코프에 남는다. 같은 판단의 선례가 `notification.create` 에 있다 — `workspace_id` →
`surface_id` 의 워크스페이스 → 워크스페이스가 하나뿐일 때만 그것 → 거절.

## Decision

**요청이 대상을 댔으면 그 대상에서 귀속을 끌어내고, 아무것도 대지 않은 자리에만 활성 워크스페이스
기본값을 남긴다.** 자리마다 갈래는 셋 중 하나다.

- **⒜ 명시 수단이 이미 있다 → 기본값을 남기고 명시를 권한다.** 5 · 6 이다. `workspace_id` 가 IPC
  와 CLI 에 모두 있으므로 새 수단을 만들지 않는다. 생략한 호출만 재현되지 않고, 그것은
  [포커스 정책](../design/policies/focus.md) 의 "기본값 채우기" 부류가 적는 성질이다.
- **⒝ 요청이 이미 대상을 댔다 → 그 대상에서 끌어낸다.** 7 이다. `approval.request` 의 귀속 순서를
  **명시 `workspace_id` → `surface_id` 가 사는 워크스페이스 → 활성 워크스페이스**로 바꿨다. 둘 다
  안 준 요청은 종전과 같다. 살아 있지 않은 `surface_id` 는 핸들러까지 오지 않는다 — 라우터가
  지목 대상을 먼저 확인해 `-32602 no live surface` 로 거절한다(헤드리스는 실측, GUI 는 [포커스 정책](../design/policies/focus.md)
  "지목한 대상을 아무도 안 가졌으면 거절한다" · 헤드리스 판정 근거 [ADR-0143](0143-a-named-target-is-checked-before-the-engine-in-headless.md)). 그 확인을 안 거치는
  직접 호출에서 surface 를 못 찾으면 핸들러는 활성으로 떨어진다 — 거절 판정을 라우터와 둘로
  만들지 않으려는 것이다.
- **⒞ 기본값이 사용자 상태를 읽는 것 자체를 못 없앤다 → 이유를 적고 남긴다.**
  - 1 · 2 · 3 은 보고다. "활성 상태 *조회* 는 허용" 그대로이고, 없애면 응답이 답해야 할 사실이
    사라진다.
  - 4 는 게이트가 라우팅 **전에** 모든 메서드에 한 번 돌아 요청의 대상을 아직 모른다. 대상 키를
    갖지 않은 메서드도 많아 "대상의 워크스페이스" 로 바꿀 좌변이 없다. 값은 판정에 안 쓰이는
    관측 태그다.
  - 8 은 호스트가 권한 거부 응답 안에서 스스로 내는 격상 요청이고 요청 인자가 없다. 그것을 일으킨
    에이전트는 agent id 로만 식별되고 워크스페이스에 매이지 않는다.
  - 9 · 10 · 11 은 좌변이 **없는** 것이 아니라 **있어도 안 쓰기로 한** 자리다. 셋은 텔레메트리
    기록 또는 RSS 샘플 직후 동기로 발화하고, 발화 경로는 셋이다(상한 9 · 10 은 ⒤ · ⒥ 둘, 이상 탐지 11 은 셋 다).
    - ⒤ 명시 `telemetry.record`(`_batch`)(`telemetry/record.rs`) — 상한 평가(9 · 10)와 RSS
      self-report 이상 탐지(11)를 부른다. 그 순간 계기 이벤트를 들고 있고 그 `workspace_id` 는
      호출자가 댔으면 그 값이다. 안 댔으면 그것도 활성 워크스페이스에서 왔다(5 · 6 의 기본값 —
      번들 claude plugin 경로가 이것이다).
    - ⒥ 게이트 미들웨어(`handler/telemetry.rs` `record_ipc_call`) — `telemetry.*` 와 host caller 를 뺀 모든 IPC 호출마다
      `ipc_calls` 이벤트를 쌓고 상한 평가(9 · 10)와 호출 폭주·느린 루프 이상 탐지(11)를 부른다. 이 이벤트의
      워크스페이스 태그는 `engine.workspaces.first()` 라 요청과 무관한 좌변이다.
    - ⒦ plugin RSS 샘플링(`handler.rs` `record_plugin_rss_samples`) — host 가 sysinfo 로 잰 값을
      이상 탐지(11)에만 공급한다. 이벤트 자체가 없다.

    그러니 "계기 이벤트의 워크스페이스" 는 셋 중 ⒤ 하나에서만 뜻이 있다. 그 위에 셋을 활성에
    남기는 이유는 이렇다. 상한은 agent + metric 단위 **누적**이라 여러 워크스페이스의 이벤트를
    합치고, 임계를 넘긴 마지막 이벤트 하나의 워크스페이스가 그 누적의 귀속으로 옳다는 보장이 없다.
    10 · 11 은 알림 배치라 **사용자가 지금 보는 곳에 뜨는 것**이 뜻 자체다. 11 은 ⒦ 갈래에 정말
    계기 요청이 없다.
  - **9 만은 성질이 다르다 — 이 긴장을 남긴 채 둔다.** 9 는 알림이 아니라 **영속되는 승인 기록**이다
    (`approval::persist_record` → `workspace:<활성 id>` 스코프). 이 ADR 이 7 을 고친 바로 그 이유 —
    다른 워크스페이스에 속할 영속 기록이 사용자가 보는 워크스페이스의 스코프에 남고, 그쪽을 닫아도
    안 사라진다 — 가 9 에도 같은 형태로 선다. 그런데도 ⒞ 로 두는 것은 위의 누적 논거 때문이다:
    7 은 요청이 댄 surface 라는 옳은 좌변이 있었고, 9 는 가장 뜻있는 ⒤ 에서도 누적의 한 조각을
    가리킬 뿐이다. 재검토 조건에 적었다.

## Consequences

- **얻은 것**: `surface_id` 를 준 `approval.request` 는 사용자가 어느 워크스페이스를 보고 있든 같은
  워크스페이스에 묶인다. 영속 스코프가 그 surface 의 워크스페이스라 워크스페이스를 닫을 때 함께
  정리되고, 함께 뜨는 알림도 그 워크스페이스의 것이 된다. 시험
  `a_named_surface_decides_the_workspace_whatever_the_user_is_viewing` 이 활성 워크스페이스를 바꿔
  가며 같은 답을 고정한다.
- **얻은 것**: 11 자리가 전부 갈래와 이유를 갖는다. 조용히 넘어간 자리가 없다.
- **잃은 것 — 외부 동작이 한 경우에서 바뀐다.** `surface_id` 만 주고 `workspace_id` 를 안 준
  `approval.request` 의 기록 `workspace_id` 가 활성 워크스페이스에서 surface 의 워크스페이스로 바뀐다.
  그 surface 가 활성 워크스페이스에 있으면 값은 같다. popup 은 창 단위라 안 움직인다.
- **잃은 것**: 5 · 6 은 여전히 생략하면 포커스를 읽는다. 번들 claude plugin 의 `telemetry.record` 는
  surface 를 태그로만 싣고 `workspace_id` 를 안 줘 그 기본값을 탄다 — 이 결정은 그것을 고치지
  않았다.
- **운영 비용**: 에이전트 대면 경로가 활성 포인터를 새로 읽으면
  `agent_facing_reads_of_active_state_are_classified` 명부가 빨개진다(ADR-0471 이 붙인 채널). 그때
  위 세 갈래 중 하나를 고른다.

## Alternatives Considered

- **대상 생략을 거절한다(`notification.create` 처럼 에러 + 사용법)**: 원칙 3 의 "target 미지정은 에러"
  에 가장 맞다. 안 고른 이유: 5 · 6 · 7 은 모두 기록의 귀속이지 작용 대상이 아니고, 거절하면 지금
  `workspace_id` 없이 부르는 모든 호출(번들 plugin 포함)이 깨진다. 호환이 가장 많이 보존되는 쪽을
  골랐다.
- **호출자 surface(`TASTY_SURFACE_ID`)의 워크스페이스를 기본값으로 쓴다**: 요청 인자가 아니라 호출
  환경이라 IPC 에는 좌변이 없다 — CLI 가 채워야 한다. [ADR-0514](0514-new-workspace-names-its-window-by-a-surface-and-keeps-no-env-default.md)
  가 같은 형태(생략값을 환경변수로 채우기)를 기각했다. 호출자 surface 는 대상이 아니다.
- **`telemetry.record` 에도 `surface_id` 인자를 새로 둔다(⒝ 로 옮긴다)**: 번들 claude plugin 의 귀속이
  고쳐진다. 안 고른 이유: 새 인자는 라우팅 키가 되어 닫힌 surface 를 실은 기록이 거절로 바뀌는 등
  이 결정보다 넓은 외부 변경이고 plugin 버전도 움직인다. 명시 수단(`workspace_id`)이 이미 있어 ⒜
  에 둔다.
- **핸들러도 없는 `surface_id` 를 거절한다(`notification.create` 선례)**: 안 고른 이유: 외부에서
  들어오는 요청은 라우터가 이미 같은 판정으로 거절한다(실측 — 아래 References). 핸들러에 둘째
  거절을 두면 같은 물음에 판정기가 둘이 되고, 갈리면 조합마다 다른 오류가 나간다.
- **게이트 태그(4)를 라우팅 뒤로 미뤄 대상의 워크스페이스를 쓴다**: 거부 행은 라우팅 전에 나가므로
  행마다 태그의 뜻이 갈린다. 게이트는 모든 진입점이 공유하는 입장 판정 자리이고([ADR-0420](0420-the-idempotency-key-envelope-is-judged-at-the-admission-gate.md)
  이 봉투 판정까지 거기 모았다), 그것을 라우팅 뒤로 옮기는 것은 이 결정의 범위가 아니다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 에이전트 대면 경로가 활성 포인터를 새로 읽는다 — `agent_facing_reads_of_active_state_are_classified`
  가 명부 불일치로 잡는다.
- `approval.request` 가 surface 의 워크스페이스를 안 따르게 된다 — 위 시험이 잡는다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 번들 plugin 이 `telemetry.record` 에 `workspace_id` 를 싣게 되거나, 그 귀속을 워크스페이스로 조회하는
  기능이 생긴다 — 그때 5 · 6 의 기본값을 거절이나 surface 유도로 옮길지 다시 본다. 재는 법:
  `git grep -n '"telemetry.record"' -- crates/tasty-plugin-*`.
- 상한 평가가 워크스페이스 단위로 갈라지거나(상한이 워크스페이스를 키로 갖게 된다), 상한 평가의
  발화 경로가 명시 `telemetry.record`(⒤) 하나로 좁아진다 — 그러면 9 의 영속 승인 기록을 계기의
  워크스페이스로 귀속할 옳은 좌변이 생기므로 9 를 ⒝ 로 옮길지 다시 본다. 재는 법: `CostCap` 이
  워크스페이스 필드를 갖는가(`crates/tasty-telemetry`), `git grep -n evaluate_caps_after_record -- src`
  의 호출 자리가 `telemetry/record.rs` 뿐인가.
- 호스트가 스스로 내는 승인·알림(8~11)에 그것을 일으킨 에이전트의 surface 가 실리게 된다 — 그러면
  활성 대신 그 surface 의 워크스페이스로 귀속할 좌변이 생긴다. 재는 법: `CallerContext::Agent` 가
  surface 를 들고 있는가(`crates/tasty-ipc/src/caller.rs`).

## References

- 선행 결정: [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md) (그것이 남긴 "대상 생략 기본값 재결정" 을 이 ADR 이 정한다) ·
  [ADR-0470](0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md) (남은 걸음 2 의 출발점)
- 탐색: `git grep -l 'active_workspace_index\|대상 생략' -- docs/adr/`
- [포커스 정책](../design/policies/focus.md) "라우팅 아래에도 층이 하나 더 있다" · [human-handoff](../features/human-handoff/index.md) "귀속 워크스페이스"
- 실측(2026-09-23, 격리 `TASTY_HOME` 헤드리스 데몬, 워크스페이스 1(활성) · 2, surface 2 는 워크스페이스 2):
  `tasty approval request --surface-id 2` → `workspace_id: 2` · 인자 없음 → `1` ·
  `--surface-id 2 --workspace-id 1` → `1` · `--surface-id 99` → `-32602 no live surface 99`.
- 결정이 실현된 현재 위치: `approval::request::handle_request` · `IpcWindow::active_workspace_index`
