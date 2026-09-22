# ADR-0060: hard-occupied workspace 로의 `terminal.spawn` 을 구조 변경 차단 대상에 포함한다

- **Status**: Accepted
- **Date**: 2026-08-05
- **Tags**: occupation, hard-occupy, attach, terminal-spawn, agent-collaboration, adr-0040
- **Group**: remote-attach

## Context

`docs/features/remote-attach/index.md`(구 판)과 `docs/dev-guide/attach-behavior.md`(구 판)는
`hard_occupied_structural_guard`(`src/adapters/ipc/handler.rs`)의 차단 대상을 `split`/
`tab.create`/`pane.close`/`tab.close`/`tab.move`/`surface.close` 6종으로 명시하고, "새 리소스를
추가만 하는 생성 경로(`pty.attach_surface`, `tasty claude/codex spawn` 등)는 홀더의 화면을 안
흔드니 차단 대상이 아니다"라고 못박아 두었다. 이 결정의 논거는 오직 **점유 holder(원격 attach
client) 관점** 만 다뤘다 — 생성만 하니 holder 가 보는 화면엔 어차피 안 보인다는 것.

실제로는 `tasty claude spawn`/`tasty codex spawn` 으로 hard-occupied 상태인 workspace 에 자식
터미널을 spawn하면 다음이 일어난다:

1. `terminal.spawn`(`handle_spawn`, `src/adapters/ipc/handler/terminal.rs:227`)이
   `tab::handle_tab_create` 를 직접 함수 호출해 새 surface 를 만들고, **성공 응답**
   (`child_surface_id` 포함)을 반환한다.
2. `apply_create_tab`(`src/core/impl_tab.rs`)의 후처리 `tap_new_workspace_member`
   (`src/core/attach_runtime.rs`) → `OccupancyRegistry::add_workspace_member`
   (`src/core/attach.rs`)가 새 surface 를 이미 걸려 있는 workspace lock 에 **즉시 편입**한다 —
   ADR-0040 이 정의한 "점유는 surface 생성 방식과 무관하게 걸린다" 원칙의 정직한 결과다.
3. 편입 순간부터 그 surface 는 `is_hard_occupied() == true` 다. `apply_send_to_surface`
   (`src/core/impl_attach.rs`)가 무조건 `sent:false` 를 반환하므로, **spawn 을 호출한 쪽을 포함한 모든
   로컬 입력이 거부**된다 — `surface.send`/`terminal.tell` 모두 실패한다.
4. 그 실패는 "존재하지 않음"과 "존재하지만 점유로 차단됨"을 구분하지 않는
   `"Surface {id} not found"` 로 뭉뚱그려진다(`terminal.rs` `send_body_then_submit`) — 방금
   spawn 성공 응답으로 받은 surface_id 가 갑자기 "없다"는 모순된 진단이 나온다.

즉 spawn 은 "성공"하지만 그 결과물은 **애초에 아무도 조작할 수 없는 채로 태어난다.** 구 문서의
"생성 경로는 예외" 논거는 이 결과를 검토하지 않았다 — spawn 을 호출한 로컬 agent 자신이 피해자가
되는 경우는 holder 관점 논거로는 아예 보이지 않는 사각지대였다.

`docs/dev-guide/attach-behavior.md`(구 판)는 이 생성 경로가 `pty.attach_surface`(`AdoptTerminal`
intent)라고 서술했으나, 코드를 직접 추적한 결과 `tasty claude/codex spawn` 이 실제로 타는 IPC
method 는 **`terminal.spawn`** 이었다(`handle_spawn` → `tab::handle_tab_create` 직접 호출, 즉
`apply_create_tab` 경로) — 문서 서술은 오기였다.

## Decision

`hard_occupied_structural_guard`(`src/adapters/ipc/handler.rs`)의 차단 대상에 `terminal.spawn`
을 추가한다. hard-occupied workspace 로의 `terminal.spawn` 호출(`workspace` 파라미터로 지정된
workspace 가 hard-occupied 상태)은 **tab/surface 를 전혀 생성하지 않고** `invalid_params` 에러로
즉시 거부되며, 에러 메시지에 점유 중임을 명시한다.

- `workspace` 파라미터(문자열, ID 또는 이름)는 `terminal::resolve_workspace_id`
  (`src/adapters/ipc/handler/terminal.rs`, 기존 `handle_spawn` 이 쓰던 것과 동일 로직, 재사용을
  위해 `pub(super)` 로 가시성 조정)로 ws_id 를 resolve 한 뒤, 기존 6종과 동일하게
  `OccupancyRegistry::workspace_holder` 로 점유 여부를 확인한다.
- `terminal.spawn` 은 forward 실행 경로(`execute_forwarded_structural_op`, holder 본인이 mirror
  안에서 만든 구조 변경이 서버에 도달하는 경로)가 재사용하는 6개 IPC 핸들러
  (split/tab.create/tab.close/tab.move/pane.close/surface.close) 목록에 **포함되지 않는다** —
  가드를 추가해도 holder 본인의 정당한 forward 요청을 막는 회귀는 없다. (현재 형태: forward
  실행은 핸들러를 거치지 않고 도메인 실행 함수 `core::structural_exec` 를 직접 부른다 —
  [ADR-0395](0395-structural-execution-and-its-cascades-live-in-the-domain-layer.md). 결론은 같다.)
- 점유되지 않은 workspace 로의 `terminal.spawn` 은 기존과 동일하게 동작한다(회귀 없음).

## Consequences

- **얻은 것**: spawn 이 실패할 걸 알면서도 성공 응답을 주던 기만적 동작이 사라진다. hard-occupied
  workspace 로의 spawn 시도는 그 자리에서 명확한 이유와 함께 거부되고, 원인 불명의
  `"Surface not found"` 로 헤매는 후속 디버깅 비용이 사라진다.
- **잃은 것**: hard-occupied workspace 에 자식 터미널을 spawn 하는 정당한 유스케이스(있다면)가
  막힌다 — 다만 spawn 직후 그 결과물을 조작할 수 없다는 점에서 애초에 유효한 유스케이스가
  아니었다(이 ADR 이 다루는 사각지대 자체).
- **운영 비용 / 유지 부담**: `hard_occupied_structural_guard` 의 match 대상이 6종→7종으로 늘어난다.
  `terminal.spawn` 은 다른 6종과 달리 pane/tab/surface id 가 아니라 `workspace` 문자열 파라미터를
  직접 받으므로 resolve 로직이 별도 분기다 — 두 그룹(대상 식별 방식 다름)이 한 함수에 공존한다는
  사실을 다음 케이스 추가 시 주의해야 한다.
- **범위 밖으로 남는 알려진 갭**: `pty.attach_surface`(`AdoptTerminal` intent)도 같은
  `tap_new_workspace_member` 후처리를 타므로 이론상 동일한 부작용(호출자 자신의 입력 차단)을 가질
  수 있다. 이번 결정은 확인된 `terminal.spawn`(`tasty claude/codex spawn` 이 호출) 경로로 스코프를
  한정했다 — `pty.attach_surface` 의 소비자(`tasty terminal` 커맨드의 child-terminal, soft 점유
  기반)에 미치는 영향을 아직 검토하지 않았기 때문이다.

## Alternatives Considered

- **에러 메시지 개선만 하는 안** — spawn 은 그대로 성공시키되, `terminal.tell`/`surface.send` 가
  hard-occupied 로 인한 실패임을 명확히 알리는 별도 에러 코드/문구를 추가하는 방안. 원인 진단은
  개선되지만, 근본적으로 "쓸 수 없는 것을 만들어주는" 기만적 성공 응답 자체는 남는다. 사용자가
  명시적으로 "점유된 워크스페이스엔 spawn 이 막혀야 한다"고 요구했으므로 채택하지 않는다.
- **spawn 성공은 유지하되 자동으로 점유를 우회(soft 점유로 대체 등)** — hard 점유의 배타성
  (ADR-0040)을 spawn 경로에서만 예외 처리하는 것이라 점유 모델의 일관성이 깨진다. 또한 holder 가
  전혀 모르는 사이 자기 mirror 안에 안 보이는 surface 가 생기는 것도 별도 문제라 기각.
- **`pty.attach_surface` 까지 동시에 가드 대상에 포함** — 코드상 같은 취약점을 가질 가능성이
  높지만, 이 결정이 확인한 필수 스코프 밖이라 별도 검토 없이 포함시키지 않는다(재검토 조건
  참고).

## Reconsideration Triggers

- `pty.attach_surface`(`tasty terminal` 커맨드 등 soft 점유 기반 child-terminal 소비자)에서
  같은 "spawn 성공 후 자기 결과물에 입력 불가" 증상이 재현되면, 이 ADR 의 범위를
  `pty.attach_surface` 까지 확장할지 재검토한다.
- hard-occupied workspace 에 자식 터미널을 spawn 해야만 하는 정당한 유스케이스(예: holder 에게
  보여줄 목적이 아니라 순수 백그라운드 실행이며 결과는 나중에 다른 채널로 회수)가 보고되면, 차단
  대신 "spawn 은 허용하되 soft 점유로 격리해 hard lock 상속을 막는" 대안을 재검토한다.

## References

- 영향 파일: `src/adapters/ipc/handler.rs`(`hard_occupied_structural_guard`),
  `src/adapters/ipc/handler/terminal.rs`(`resolve_workspace_id` 가시성),
  `src/core/attach_runtime.rs`(`forward_exec_tests` 모듈의 신규 테스트).
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — "점유는 surface 생성 방식과
  무관하게 걸린다" 원칙(유지). 본 ADR 은 그 원칙의 자연스러운 귀결로,
  `hard_occupied_structural_guard`의 대상 범위(그 원칙 위에서 파생된 별개 결정)만 뒤집는다.
- [`docs/features/remote-attach/index.md`](../features/remote-attach/index.md#서버피점유측-비-holder-구조-변경-차단)
  — 차단 대상 목록(갱신됨).
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md#서버-로컬비-holder-구조-변경-차단)
  — 가드 메커니즘 상세(갱신됨).
- 선례: [ADR-0055](0055-mouse-capture-banner-suppress-list.md) — 기존 ADR 의 사각지대를 새 ADR
  로 채워 정책을 좁힌 유사 사례.
