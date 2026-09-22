# ADR-0298: 훅 핸들러 시퀀스는 CLI 에서 제자리로 고친다 — 지우고 다시 만들지 않는다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: hook-handler, cli, ipc, registry, ipc-sequence, local-only, settings, adr-0046, adr-0047
- **Group**: webhook-hooks

## Context

공유 훅 핸들러 레지스트리([ADR-0047](0047-shared-hook-handler-registry-source-gate.md))의 user 출처 항목을 고치는 **모델 계층은 처음부터 있었다** — `HookHandlerRegistry::upsert_user_handler`(patch semantics) 와 `save_user_config`(atomic write). 없던 것은 그 표면이다.

- **GUI**: Settings › Handler › Hook Handlers 서브탭이 그 API 를 부른다. 다만 `IpcSequence` 행은 mono 한 줄 요약만 두고 편집 진입점이 없다 — 디자인의 `Edit` ghost 버튼을 두지 않은 것은 **결정**이었다("아무 데도 안 여는 버튼은 그 자체가 거짓 표시라"). 그 결정의 해제 조건은 "편집기 범위가 정해질 때까지" 였다.
- **CLI/IPC**: ADR-0047 의 Consequences 가 "user 편집 API 는 정의돼 있으나 부팅/Settings 에 배선되지 않았고, `hook_handler.*` IPC/CLI 도 아직 없다" 고 적었다. 그 뒤 `list`/`reload`/`dispatch` 가 생겼고 **편집은 안 생겼다.** 그래서 이쪽의 부재는 결정이 아니라 **덜 배선된 상태**다.

남은 경로는 `~/.tasty/hook-handlers.toml` 손편집 + `reload` 하나였다. 그것은 루트 `CLAUDE.md` 원칙 2(에이전트 기능은 IPC + CLI 양면)의 반대편이다 — 에이전트가 자기 자동화를 고치려면 tasty 밖으로 나가 파일을 써야 했다. `list` 는 `steps` 수만 내므로 **지금 무엇이 들었는지 읽을 길조차 없었다.**

"고친다" 에는 관측 가능하게 다른 두 모양이 있다. 기존에 그 비슷한 것을 하는 유일한 경로(`webhook.register --sequence` → 익명 핸들러 `user/wh-<slug>` 생성)는 뒤쪽이다.

| | 제자리 patch | 지우고 다시 만들기 |
|---|---|---|
| 식별자 | 유지 — 그 id 를 참조하는 훅 바인딩(`HookBinding::Handler(id)`)이 계속 같은 것을 가리킨다 | `user/wh-<slug>` 를 새로 채번한다(슬러그가 첫 method 에서 나오므로 첫 스텝을 바꾸면 id 가 바뀐다) |
| 안 적은 필드 | 그대로 둔다 | 기본값으로 되돌아간다(priority 0 · source webhook · display name 없음) |
| host/plugin 기본값 | 덮인 채 유지 | 지운 순간 드러났다가 다시 덮인다 |
| 그 사이 트리거 | 없다 — 한 번의 쓰기다 | 갈 곳이 없다(id 가 없는 창이 생긴다) |

## Decision

`hook_handler.{get,upsert,remove}` 셋을 **`local_only` IPC + `tasty hook-handler get|upsert|remove` CLI 양면**으로 낸다. `upsert` 는 **제자리 patch** 다 — 준 필드만 덮고 안 준 필드는 지우지 않는다. `get` 은 `list` 가 감추는 action 본문을 내고, 그 `action` 값은 `upsert` 의 `action` 파라미터와 **같은 모양**이라 읽은 것을 고쳐 그대로 되돌려 보낼 수 있다. 성공한 `upsert`/`remove` 는 user config 를 즉시 atomic write 하고, **쓰기에 실패하면 성공으로 보고하지 않는다.**

셋 다 `local_only` 다. `IpcSequence` 는 Local 권한으로 실행되므로 plugin 이 시퀀스를 읽거나 고칠 수 있으면 자기 권한 집합을 넘어선 IPC escalation 이 된다 — `webhook.register` 가 plugin 의 인라인 `sequence` 를 거부하는 것과 같은 근거이자 같은 자리다. 이것은 사용자 조작의 재현이 아니라 **에이전트가 자기 작업에 필요한 기능**이므로 release 표면이다(원칙 1 의 debug 격리 대상이 아니다).

**GUI 편집기는 이 결정에 포함되지 않는다.** 시퀀스 편집기의 레이아웃·간격·색은 확정 시안에 없고, 디자인 값은 로컬에서 정하지 않는다. 위 "거짓 표시" 결정은 그대로 유지된다 — 열 편집기가 생긴 뒤에 버튼을 둔다.

## Consequences

- **얻은 것**: 시퀀스를 tasty 안에서 읽고 고칠 수 있다. 에이전트가 자기 훅을 파일시스템 경유 없이 고친다. GUI 편집기가 언젠가 생길 때 그것이 열 대상(모델 + 표면)이 이미 값으로 정해져 있다.
- **잃은 것**: 없던 쓰기 표면이 하나 늘었다. 다만 `local_only` 라 plugin 표면은 그대로고, 셸 불변식(`ShellCommand` 는 `source = hook` 만)은 레지스트리가 그대로 강제한다.
- **안 얻은 것 — 명시**: **이미 등록된 웹훅은 안 따라온다.** 웹훅 엔트리는 등록 시점의 `calls` 스냅샷을 직접 소유하고 발화 시 그것을 실행한다(`--handler <id>` 로 바인딩한 것도 등록 시점에 복사된다). 바뀐 시퀀스를 외부 URL 에도 적용하려면 그 웹훅을 다시 등록한다. owner 가 등록 시 흐름을 고정한다는 [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) 의 모양이라 결함이 아니고, 그래서 **값으로 적는다.**
- **운영 비용 / 유지 부담**: 메서드 수 스냅샷(`tests/cli_naming_count_drift.rs`)·라우터·권한 표·CLI 도움말 세 언어가 같이 움직인다. 아무 필드도 안 준 `upsert` 와 스키마에 안 맞는 `action` 은 **거부한다** — 조용히 넘기면 아무것도 안 고친 요청이 성공으로 보고된다.

## Alternatives Considered

- **A: GUI 시퀀스 편집기를 먼저 만든다** — 디자인 값이 확정 시안에 없어 여기서 정할 수 없고, gallery-first 규율상 확정 시안 → specimen → 본체 순서가 선행이다. 원칙 2 도 GUI 전용 에이전트 기능을 금지하므로 어차피 CLI/IPC 가 필요하다.
- **B: `remove` 후 `webhook.register --sequence` 로 다시 만든다** — 위 표의 네 줄이 전부 달라진다. 특히 id 채번이 첫 method 에서 나오므로 **첫 스텝을 바꾸면 그 핸들러를 가리키던 훅들이 전부 끊긴다.** "고친다" 라고 부를 수 없다.
- **C: Settings 에 "TOML 열기" 버튼을 둔다** — GUI 전용이라 원칙 2 위반이고, 에이전트는 여전히 파일을 손으로 써야 한다.
- **D: 아무것도 안 한다** — 손편집 + `reload` 가 이미 되므로 기능상 불가능한 것은 없다. 다만 그것은 tasty 밖의 경로이고, `list` 가 내용을 안 보여주므로 **고치기 전에 읽을 방법이 없다.**
- **E: `upsert` 만 내고 `remove` 는 안 낸다** — 만들 수는 있고 지울 수는 없는 상태를 새 표면에 다시 만드는 것이라, 이 ADR 이 고치려는 결함과 같은 형태다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `hook_handler.handle:<id>` 토큰에 **강제 지점이 생기면** — `docs/dev-guide/plugin-permissions.md` 가 지금 "형식 검증만 있고 강제하는 지점이 아직 없다" 고 적은 자리다. 그 토큰이 실제로 동작을 가르게 되면 plugin 이 자기 소유 핸들러를 고치는 것을 `local_only` 로 계속 막을지가 다시 열린다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 웹훅이 발화 시점에 핸들러를 **다시 조회**하게 바뀌면, 위 "안 얻은 것" 이 거짓이 되고 문서 네 자리가 같이 낡는다. 재는 법: `src/webhook/registry.rs` 의 매칭 지점이 `entry.calls` 를 실행하는지, 아니면 `hook_handler::global().get(...)` 를 부르는지 읽는다.
- GUI 시퀀스 편집기의 확정 시안이 오면 — 그때 `Edit` 버튼 결정과 이 CLI 표면의 관계를 다시 정한다. 재는 법: 확정 시안 아카이브에 시퀀스 편집기 화면이 들어왔는지 본다.

## References

- [ADR-0047](0047-shared-hook-handler-registry-source-gate.md) — 공유 훅 핸들러 레지스트리 + source 게이트(이 결정이 그 Consequences 의 미배선 항을 닫는다)
- [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) — owner 가 등록 시 흐름을 고정한다(웹훅 스냅샷의 근거)
- [hooks](../features/hooks/index.md) "핸들러 레지스트리" · [webhook](../features/webhook/index.md) · [reference/api](../reference/api.md)
- 코드 근거(결정이 실현된 현재 위치): `src/adapters/ipc/handler/hook_handler.rs` 의 `handle_get` · `handle_upsert` · `handle_remove` · `parse_upsert`, `crates/tasty-cli/src/commands/hook_handler.rs` 의 `HookHandlerCommands`
