# ADR-0532: `workspace.create` 는 창을 지목한 surface 에서 cwd 를 상속한다 — ADR-0514 의 "호스트는 바꾸지 않는다" 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: ipc, cli, workspace, cwd, inherit-cwd, focus, multi-window, routing, adr-0514
- **Group**: cli-logging

## Context

`workspace.create` 가 `cwd` 없이 terminal 워크스페이스를 만들면 핸들러(`handle_workspace_create`)가
`IpcWindow::resolve_inherit_cwd` 로 cwd 를 채운다. 그 값은 **그 창의 포커스 surface** 의 cwd 다.

[ADR-0514](0514-new-workspace-names-its-window-by-a-surface-and-keeps-no-env-default.md) 로
`tasty new workspace --surface <ID>` 가 IPC `surface_id` 를 싣게 됐고, 라우터가 그 surface 를 가진
창으로 요청을 보낸다. 그 ADR 은 "호스트는 바꾸지 않는다" 고 정했으므로 핸들러는 `surface_id` 를
읽지 않았다. 그래서 에이전트가 surface 를 지목해도 cwd 는 **그 창에서 사용자가 지금 보고 있는
surface** 에서 왔다 — 같은 인자로 두 번 불러도 사용자가 그 사이 탭을 옮겼으면 결과가 달라진다.
[정체성 원칙](../identity.md) 3(대상을 ID 로 지정하고 활성 상태에 의존하지 않는다)의 위반이다.

cwd 원본을 적을 인자를 정해야 했다. 사실 하나가 폭을 좁힌다: **engine(`CoreState`)은 창마다
따로다.** 핸들러는 라우터가 고른 창의 engine 만 쥐고, surface 의 cwd(`local_surface_cwd`)는 그
engine 에서만 읽힌다(`src/core/request_target.rs` 의 `Kind` 문서).

## Decision

**`workspace.create` 의 `surface_id` 는 주인 창과 cwd 상속 원본을 함께 정한다.** `cwd` 를 생략한
terminal 워크스페이스는 `surface_id` 를 실었으면 **그 surface** 의 cwd 를, 안 실었으면 종전처럼
그 창의 포커스 surface 의 cwd 를 상속한다. CLI 는 바뀌지 않는다 — `--surface` 가 이미 그 키를
채운다.

- 개정하는 것: ADR-0514 Decision 의 "호스트는 바꾸지 않는다" 조항. 핸들러가 이제 `surface_id` 를
  읽는다(cwd 원본으로만 — 창 선택·거절은 여전히 라우터의 일이다).
- 개정하지 않는 것: `--surface` 라는 이름과 숫자 ID 만 받는 것 · 생략 시 `TASTY_SURFACE_ID` 로
  채우지 않는 것 · 생략 시 사용자가 보는 창에 만드는 것 · `--window` 를 두지 않는 것 · 지목한
  surface 를 아무 창도 안 가졌을 때의 거절.
- 그대로 두는 것: 명시 `cwd` 가 언제나 이긴다. `inherit_cwd` 설정을 끄면 지목해도 상속하지 않는다.
  원본이 mirror surface 면 로컬 출처가 없어 `None`(= 홈)이다
  ([`surface-cwd.md`](../architecture/invariants/surface-cwd.md) §3-2).
- 숫자가 아닌 `surface_id` 는 `invalid_params` 로 거절한다. 핸들러가 그 키를 안 읽던 때에는 라우터도
  못 짚은 채 포커스 창의 포커스 surface 로 조용히 떨어졌다 — 읽기 시작한 이상 잘못된 값을 없는
  값처럼 다루지 않는다(`handler/params.rs` 의 관문 규칙). 그래서 이 메서드는 이제 미라우팅 명부
  (`src/source_guards/unrouted_dispatch_reasons.rs`)에 없다 — 핸들러가 대상 키를 읽는다.

## Consequences

- **얻은 것**: 지목한 호출은 사용자의 포커스와 무관하게 재현된다. `--surface` 만 주면 "그 surface
  옆에서 일을 이어간다" 가 된다 — 별도 `--cwd` 계산이 필요 없다.
- **잃은 것**: `surface_id` 를 싣던 기존 호출 중, 그 창의 포커스 surface 와 지목 surface 의 cwd 가
  달랐던 것은 결과 cwd 가 바뀐다. 원칙 3 이 호환보다 앞선다. 창만 고르고 cwd 는 다른 곳에서 가져
  오고 싶으면 `cwd` 를 명시한다.
- **운영 비용 / 유지 부담**: 핸들러의 작은 함수 둘(params 를 읽어 cwd 를 정하는 `resolve_create_cwd`,
  상속 원본을 읽는 `inherit_cwd_for_create`)과 시험 일곱(`create_cwd_tests`). 그중 하나는 핸들러를
  끝까지 불러 새 터미널의 셸이 계산된 cwd 에서 뜨는지를 실제 PTY 로 잰다 — 나머지는 cwd 를 무엇으로
  계산하는가만 재서 그 값을 생성 intent 에 싣지 않아도 초록이다. 셸 cwd 조회 수단이 linux·macos 에만
  있어 그 하나는 Windows 에서 컴파일되지 않는다.
  명시 `cwd` 갈래에는 실행 중 서버를 상대로 하는 두 번째 채널이 있다 — `tests/attach_attention_loopback.rs`
  의 `server_pushes_the_occupied_terminal_cwd_and_follows_cd` 가 명시 `cwd` 로 만든 워크스페이스의 셸
  시작 cwd 를 단언하므로, 생성 intent 에 `cwd: None` 을 실으면 그 시험도 실패한다(헤드리스 조합에서
  변이로 확인, 2026-09-23). 지목 surface 상속 갈래는 그 시험이 안 본다.

## Alternatives Considered

- **cwd 원본용 키를 따로 둔다**(예: `inherit_cwd_from`) — 창을 고르는 키와 다른 창의 surface 를
  적으면 핸들러가 가진 engine 에 그 surface 가 없어 읽을 수 없다. 읽게 하려면 라우팅을 넓히거나
  창을 건너는 조회를 새로 만들어야 하고, 같은 창의 surface 만 허용하면 결국 `surface_id` 와 같은
  정보를 두 번 적게 한다. 지목 없는 기존 호출이 포커스를 계속 읽는 것도 그대로다.
- **지목 여부와 무관하게 포커스 상속을 없앤다**(생략 시 홈) — 원칙 3 을 가장 넓게 지키지만,
  `surface_id` 없이 부르는 모든 기존 호출(사용자가 보는 창에 만드는 흐름)의 cwd 가 바뀐다.
  이 범위(지목했는데 포커스를 읽는 결함)를 넘는다.
- **ADR-0514 를 Supersede 한다** — 그 ADR 의 나머지 조항은 전부 유효하다. 조항 하나만 갈아끼운다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- engine 이 창마다 따로가 아니게 된다(`CoreState` 가 하나로 합쳐진다). 그때 cwd 원본을 창과
  떼어 다른 창의 surface 도 적게 할지 다시 본다.
- `workspace.create` 의 라우팅 키에 창 ID 가 생긴다(ADR-0514 의 재검토 조건과 같다). 그때 창을 창
  ID 로 고르는 호출의 cwd 원본을 무엇으로 할지 정해야 한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 창만 고르고 cwd 는 포커스를 따르길 원한다는 보고가 반복된다. 재는 법: 이슈·사용자 보고.

## References

- 개정 대상: [ADR-0514](0514-new-workspace-names-its-window-by-a-surface-and-keeps-no-env-default.md) (Decision 의 "호스트는 바꾸지 않는다" 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- 선행 결정: [ADR-0267](0267-mirror-surface-cwd-is-pushed-by-the-server.md) (상속은 로컬 출처만 — 그대로 따른다)
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/ipc/handler/workspace.rs` 의 `resolve_create_cwd`(params 를 읽는 자리) · `inherit_cwd_for_create`(상속 원본을 읽는 자리)와 그 `create_cwd_tests`
- [`docs/design/policies/focus.md`](../design/policies/focus.md) "기본값 채우기" · "에이전트가 만든 창과 포커스"
