# ADR-0534: 꺼진 plugin 의 surface kind 는 지우지 않고 철회한다 — 열린 surface 는 두고 새 생성만 막는다

- **Status**: Accepted
- **Date**: 2026-09-23
- **Tags**: plugin, host-plugin, lifecycle, surface-kind, surface-registry, disable, remove, compatibility, adr-0173

## Context

plugin 의 surface kind 는 hello 를 받을 때 `SurfaceKindRegistry` 에 등록된다
(`App::finalize_plugin_hello` → `register_plugin_surface_kinds`, 헤드리스는
`boot::headless_plugins::register_surface_kinds`). 해제하는 자리는 없었다. plugin 을 끄거나
지우는 경로(`PluginManager::disable` → `forget_plugin_runtime`)는 이벤트 권한 · pending · 설정
sub-page · 등록 게이트를 거두지만 registry 는 매니저 밖(본체 `CoreState`)에 있어 닿지 않았다.

그래서 `plugin disable` 이나 `plugin remove` 뒤에도 그 kind 로 새 surface 가 만들어졌다.
`plugin list` 에서 사라진 뒤에도 `ConvertSurface kind=markdown` 이 성공했고, 재부팅해야
`unknown surface kind` 가 됐다(2026-09-23 관측). 만들어진 surface 는 응답할 프로세스가 없는
plugin kind 라 빈 채로 남는다. `surface.kinds` · convert 팝업 · 프리셋 편집기의 kind 목록도 꺼진
kind 를 계속 보였다.

정하지 않은 물음은 **이미 열린 surface** 였다. registry 항목을 지우면 새 생성이 막히지만, 같은
정의를 열린 surface 가 매 프레임·매 저장마다 읽는다 — layout 저장의 `snapshot`
(`snapshot_fn_for` · `SavedSurface::capture_generic_surface`), 탭 아이콘, 표시명, 입력 플래그다.
정의가 사라지면 저장이 그 surface 를 `empty` 로 떨어뜨려 다시 켜도 문서가 돌아오지 않는다.

## Decision

**disable · remove 는 그 plugin 이 등록한 kind 를 registry 에서 지우지 않고 "철회" 로 표시한다.
철회된 kind 로는 새 surface 를 만들지 않고, 이미 열린 surface 는 그대로 둔다. 같은 kind 가 다시
등록되면(다시 켜서 hello 가 오면) 철회가 풀린다.**

- **조회를 둘로 가른다.** `get` 은 철회된 정의도 돌려준다 — 이미 있는 surface 에 대한 물음이
  쓴다. 새로 만들 수 있는가는 `get_live` · `contains` · `kinds_snapshot` 이 답하고 셋 다 철회된
  kind 를 뺀다.
- **생성 funnel 은 명확한 사유로 거절한다.** `CoreState::create_surface_via_registry` 는 철회된
  kind 에 `unknown surface kind` 가 아니라 `SurfaceKindWithdrawn` 을 낸다 — "그 kind 를 제공하던
  plugin 이 꺼졌다". IPC 응답은 그 문장(영어)을, 사용자 발화 intent 는 i18n toast
  (`surface.kind_toast.withdrawn`)를 받는다. 에이전트 발화는 ADR-0503 대로 로그뿐이다.
- **복원 경로는 "아직 없는 kind" 와 같이 다룬다.** 닫은 탭 복원 · 프리셋 적용 · attach mirror
  markdown 은 철회된 kind 를 kind 대기 placeholder 로 두고, 다시 켜면 표시 시점의 reify 가
  실제화한다. 거절하지 않는 이유는 그 경로들이 이미 미등록 kind 를 placeholder 로 흡수하는
  정책이라서다 — 거절하면 형제 탭까지 버린다(`preset_apply` 의 주석).
- **목록에서 빠진다.** `surface.kinds`(`tasty list surface-kinds`) · convert 팝업 · 프리셋 편집기
  kind 목록 · `plugin.show` 의 `registered` 가 철회된 kind 를 안 보인다(`registered: false`).
- **거두는 자리는 둘이다.** disable 은 `ipc::handler::plugin::disable` 이 한다 — gui IPC · 헤드리스
  IPC · 설정 모달이 모두 이 함수를 거친다. remove 는 매니저의 `disable` 을 직접 부르므로
  `App::plugin_remove` 가 같은 철회를 한다.
- **재시작 · swap · 연결 실패는 철회하지 않는다.** 그 경로는 곧바로 다시 띄우고 새 hello 가 같은
  kind 를 다시 등록한다 — 그 사이를 거절로 채우면 무응답 재시작 한 번이 사용자의 새 탭을 실패시킨다.

## Consequences

- **얻은 것**: 끈 plugin 의 kind 로 새 surface 가 생기지 않는다. 사유가 "없는 kind" 가 아니라
  "제공 plugin 이 꺼졌다" 라서 사용자·에이전트가 할 일(다시 켠다)이 보인다.
- **얻은 것**: 열린 surface 는 끄기 전과 똑같이 남는다 — 저장·아이콘·표시명이 그대로라 다시 켜면
  이어진다. 끄기 전의 외부 동작 중 바뀌는 것은 "꺼진 kind 로 새로 만들기" 하나다.
- **잃은 것**: 철회된 kind 로 새로 만드는 요청이 예전에는 성공(빈 surface)했고 이제 실패한다.
  그 성공에 기대던 호출자는 오류를 받는다 — 성공이 응답할 수 없는 surface 를 만들던 것이라
  기댈 만한 동작이 아니었다고 판단했다.
- **잃은 것**: 꺼진 plugin 의 열린 surface 는 여전히 응답할 프로세스가 없다 — 이 결정은 그
  surface 를 닫거나 바꾸지 않는다. 다시 켜면 hello 가 정의를 덮어쓰고 이어진다(종전과 같다).
- **운영 비용**: registry 조회가 둘이다. 새 호출자는 "이미 있는 surface 에 대한 물음인가, 새로
  만들 수 있는가" 를 골라야 하고, 틀리게 고르면 앞쪽은 저장 오염, 뒤쪽은 거절 누락이 된다.
  생성 요청이 적용(거절)보다 **앞에서** registry 를 읽는 자리가 셋이다. 그중 제자리 변환의 최근
  목록 기록(`intent::surface` 의 `records_recent` 판정)은 흔적을 남기므로 `get_live` 로 묻는다 —
  `get` 이면 거절된 변환이 최근 목록에 남는다. 나머지 둘 — 같은 함수의 param alias 정규화와
  `core::structural_exec` 의 새 탭 기본 params 주입 — 은 곧 거절될 params 를 고칠 뿐 아무것도
  남기지 않아 `get` 그대로 둔다.

## Alternatives Considered

- **A: registry 항목을 지운다** — 새 생성은 막히지만 열린 surface 의 저장이 `empty` 로 떨어진다
  (위 Context). 호환을 가장 많이 잃는다.
- **B: 지우고, 열린 surface 도 함께 닫거나 placeholder 로 바꾼다** — 저장 오염은 없지만 에이전트
  명령(`plugin disable`)이 사용자의 탭을 닫거나 바꾼다. 원칙 1(사용자 상태 불가침)에 닿고, 다시
  켜도 닫은 탭은 돌아오지 않는다.
- **C: 생성 funnel 에서 매니저의 활성 여부를 묻는다** — registry 가 아니라 `PluginManager` 를 보면
  되지만 `CoreState` 의 생성 funnel 은 매니저를 모른다(App 필드). 또 목록 · 복원 · attach mirror 의
  판정이 각자 같은 물음을 다시 해야 해 자리가 흩어진다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 철회가 정의를 지우거나 생성 funnel 이 철회를 안 보면 `core::surface_registry` 의
  `a_withdrawn_kind_keeps_its_definition_but_refuses_creation` 이, 다시 등록해도 철회가 안 풀리면
  `registering_a_withdrawn_kind_again_lifts_the_withdrawal` 이, 다른 plugin · host builtin 의 kind 까지
  철회하면 `withdrawal_touches_only_the_plugins_own_kinds` 가 잡는다.
- disable 이 철회를 안 부르면(또는 열린 surface 를 건드리면, 다시 켜도 철회가 안 풀리면) 헤드리스
  조합의 `tests/plugin_disable_withdraws_surface_kinds.rs` 가 실제 데몬 상대로 잡는다. gui 조합의
  같은 배선(`App::plugin_disable` · `app_methods` 의 토글 arm)과 `App::plugin_remove` 의 철회는 시험
  채널이 없다 — 단위 시험이 `PluginManager` 를 못 만들고 gui 전용 인스턴스는 창을 띄운다. 재는 법:
  격리 `TASTY_HOME` GUI 인스턴스에서 `plugin disable` · `plugin remove` 뒤 `tasty list surface-kinds`.
- 사용자 발화의 철회 거절이 toast 를 안 내거나 에이전트 발화가 toast 를 내면
  `intent::apply_error_tests` 의 `a_withdrawn_kind_refusal_toasts_only_for_the_user` 가 잡는다.
- 철회된 kind 로의 제자리 변환이 최근 목록에 남으면 같은 모듈의
  `a_convert_to_a_withdrawn_kind_leaves_no_recent_entry` 가 잡는다(`get_live` 를 `get` 으로 되돌리는
  변이에서 실패한다 — 2026-09-23 실측).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- 철회된 kind 의 열린 surface 를 사용자가 어떻게 보는가(빈 surface 를 그대로 두는 것이 혼란을
  주는가). 재는 법: 격리 `TASTY_HOME` 인스턴스에서 markdown 탭을 연 채 `plugin disable` 하고 화면과
  `tasty list tree` 를 본 뒤, `plugin enable` 로 이어지는지 본다.
- 재시작 · swap · 연결 실패 경로도 철회해야 하는 사례가 나오면(재기동이 끝내 실패한 plugin 의
  kind 가 계속 보인다는 보고) 다시 연다. 재는 법: 연결하지 않는 probe plugin 을 켠 뒤
  `tasty list surface-kinds` 에 그 kind 가 남는지 본다.

## References

- 선행 결정: [ADR-0173](0173-namespace-resolution-reads-the-manifest-not-the-process-table.md) (같은 형태의 선택 — disable 은 소유를 지우지 않고 "지금 안 뜬 plugin" 으로 답한다. 이 결정은 kind 에 대한 대응 조항이다)
- 관련 ADR: [ADR-0259](0259-a-kind-request-starts-the-owner-that-declares-it.md) (헤드리스의 kind 지목 생성은 소유자를 띄운다 — 철회된 kind 는 등록 대기의 "아직 없는 kind" 로 본다) · [ADR-0503](0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) (적용 실패 toast 는 사용자 origin 에서만) · [ADR-0505](0505-a-plugin-start-waits-for-its-connection-off-the-main-thread.md) (`forget_plugin_runtime` 을 거치는 네 경로)
- 탐색: `git grep -l 'SurfaceKindRegistry\|forget_plugin_runtime\|withdraw' -- docs/adr/`
- 관련 문서: [`plugin-development.md`](../dev-guide/plugin-development.md) §7 "생명주기" · [`features/plugin-system`](../features/plugin-system/index.md) · [`features/work-area`](../features/work-area/index.md) 의 `surface.kinds`
- **코드 근거 (결정이 실현된 현재 위치)**: `core::surface_registry` 의 `SurfaceKindRegistry::withdraw_plugin` ·
  `get_live` · `withdrawn_by` · `SurfaceKindWithdrawn`, `CoreState::create_surface_via_registry`,
  `ipc::handler::plugin::disable`, `App::plugin_remove`, `intent::report_apply_error`.
