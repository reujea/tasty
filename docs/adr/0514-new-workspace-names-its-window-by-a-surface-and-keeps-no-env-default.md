# ADR-0514: `tasty new workspace` 는 창을 서피스로 지목하고, 생략값을 `TASTY_SURFACE_ID` 로 채우지 않는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: cli, ipc, workspace, window, multi-window, focus, parity, routing

## Context

IPC `workspace.create` 는 params 에 `surface_id` 를 실으면 라우터(`src/app/ipc/routing.rs` ·
`src/app/request_owner.rs`)가 그 surface 를 가진 main 창으로 요청을 보낸다. 지목한 surface 를
아무 창도 안 가졌으면 포커스로 새지 않고 거절한다([포커스 정책](../design/policies/focus.md)).
핸들러(`handle_workspace_create`)는 그 키를 읽지 않는다 — 창을 고르는 것은 라우팅뿐이다.

CLI `tasty new workspace` 에는 그 인자가 없었다. 여러 창이 열려 있으면 워크스페이스는 늘 대상 없는
요청의 폴백인 사용자가 보는 창에 생겼다. IPC 로 되는 지목을 CLI 로 못 하는 상태 —
[정체성 원칙](../identity.md) 2(IPC + CLI 양면)와 3(대상을 ID 로 지정)의 결손이다.
[ADR-0497](0497-an-agent-created-window-does-not-take-the-users-focus.md) 뒤로는 `tasty new window`
가 포커스를 안 가져가므로, 에이전트가 자기가 연 창에 워크스페이스를 만들 CLI 경로가 아예 없었다.

인자 이름을 정할 때 본 선례는 둘이다. `tasty split --target-surface` 는 `this`·nickname 을 받지만
nickname 해석은 라우터가 `split` 에만 한다. `--surface <ID>` 는 CLI 전반이 surface 를 지목하는 이름이고
다수 명령이 생략 시 `TASTY_SURFACE_ID` 로 채운다.

## Decision

**`tasty new workspace` 에 `--surface <ID>`(숫자)를 더하고, 주면 IPC `surface_id` 로 싣는다.
생략하면 키를 싣지 않는다 — `TASTY_SURFACE_ID` 로 채우지 않는다.**

- 이름은 IPC 키(`surface_id`)와 CLI 의 surface 지목 관례(`--surface`)를 따른다. `--window <ID>` 는
  두지 않는다 — `workspace.create` 의 라우팅 키에 창 ID 가 없어서, 그 이름을 두려면 호스트 라우팅을
  새로 넓혀야 한다.
- 생략 시 동작(사용자가 보는 창)은 이 결정 전과 같다. tasty 안의 에이전트가 `--surface` 없이
  부른 기존 스크립트의 결과가 바뀌지 않는다.
- 호스트는 바꾸지 않는다. 지목·거절은 기존 라우팅 그대로다.

## Consequences

- **얻은 것**: 에이전트가 여러 창 중 원하는 창에 워크스페이스를 만든다. 없는 surface 를 적으면
  다른 창에 조용히 생기는 대신 오류로 끝난다.
- **잃은 것**: 창을 고르려면 그 창의 surface ID 가 필요하다(`tasty list windows` 의 `workspace_ids`
  와 `tasty list surfaces` 의 `workspace_id` 로 찾는다). 창 ID 로 바로 지목하는 길은 없다.
- **운영 비용 / 유지 부담**: 없음 — CLI 매핑 한 줄과 도움말 세 언어.

## Alternatives Considered

- **생략 시 `TASTY_SURFACE_ID` 로 채운다**(자기 창에 만든다) — tasty 안에서 돌던 기존 호출이 전부
  사용자가 보는 창 대신 호출자 창으로 옮겨 간다. 외부 동작이 조용히 바뀐다.
- **`--window <ID>` 를 둔다** — 호스트 라우팅(`request_target` 의 키 목록)에 창 종류를 더해야 한다.
  이 요구의 범위(IPC 에 이미 있는 지목을 CLI 로)를 넘는다.
- **`--target-surface` 로 `this`·nickname 을 받는다** — nickname 은 라우터가 `split` 에서만 풀어,
  이 메서드에서는 조용히 폴백으로 떨어진다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `request_target` 의 라우팅 키에 창 ID 가 생긴다. 그때 `--window` 를 더할지 다시 본다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- tasty 안의 에이전트가 `--surface` 를 빠뜨려 사용자 창에 워크스페이스가 생기는 일이 잦다는 보고. 재는 법: 이슈·사용자 보고.

## References

- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-cli/src/commands/new_close.rs` 의 `NewCommands::Workspace` · `crates/tasty-cli/src/request.rs` 의 `new_command_to_method_params`
- [ADR-0497](0497-an-agent-created-window-does-not-take-the-users-focus.md) — 에이전트 창은 포커스를 안 가져간다
- [`docs/features/work-area/index.md`](../features/work-area/index.md) "인터페이스"
- 부분 개정: [0532](0532-workspace-create-inherits-cwd-from-the-surface-that-names-its-window.md) ("호스트는 바꾸지 않는다" 개정 — 핸들러가 `surface_id` 를 cwd 상속 원본으로 읽는다)
