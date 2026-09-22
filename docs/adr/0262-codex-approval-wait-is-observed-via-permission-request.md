# ADR-0262: Codex 승인 대기는 `PermissionRequest` 훅으로 관측하고, `idle` 전이가 대기를 내린다

- **Status**: Accepted
- **Date**: 2026-09-11
- **Tags**: plugin, codex, agent-state, attention, hooks
- **Group**: agent-integration

## Context

Codex 가 `Would you like to run the following command?` 승인 화면을 띄우면 tasty 쪽에는
아무 신호도 없었다. 대상 surface 는 `active` 로 남고, 탭·워크스페이스에는 표시가 없으며,
`spawn`/`tell` 을 건 caller 에게도 알림이 가지 않았다. 저장소는 그 상태를 "codex 에는
`needs_input` 류 hook 이 없다" 로 설명해 왔고(`resolve_policy_args` 의 doc ·
`tasty-plugin.toml` 의 `contributes.hook_events` 주석 · `tasty-plugin-agent-common` 의
crate doc · 공개 사이트 가이드), 그 설명 위에 "승인 정책 기본값을 `never` 로 강제한다" 는
회피책이 서 있었다. 짝인 claude plugin 은 같은 물음에 `Notification`/`PreToolUse` →
`needs_input` + `needs-input` surface hook + `surface.completion`(kind=needs_input) 으로
이미 답하고 있었다.

codex-cli 0.154.0 을 실제로 띄워 훅 payload 와 발생 순서를 덤프한 결과 그 전제가 틀렸다.

- 지원 이벤트는 12 개이고 그중 `PermissionRequest` 가 **승인 화면이 뜨기 직전**에 발화한다.
- 승인이 **필요 없는** 실행에는 발화하지 않는다 — 자동 승인·장기 실행·무출력이 대기로
  오판되지 않는다.
- 해제 쪽 신호는 둘뿐이다. `PostToolUse`(승인된 도구가 **끝났을 때**)와
  `Interrupt`(거절·Esc·Ctrl-C). codex 는 "승인이 났다" 자체를 알리는 이벤트를 갖고 있지
  않다.
- 거절·Esc·Ctrl-C 는 `Interrupt` **하나만** 쏘고 `Stop` 도 `PostToolUse` 도 뒤따르지 않는다.
- `PermissionRequest` payload 에는 `tool_use_id` 가 없다(`PreToolUse`/`PostToolUse` 에는
  있다) — 대기와 해제를 tool 단위로 짝지을 수 없다.

여기에 호스트 쪽 제약이 하나 더 있었다. `ChildTerminalRegistry` 는 `idle` 과 `needs_input`
을 별도 플래그로 들고 `state_of` 가 `needs_input` 을 `idle` 보다 우선한다. 그런데
`set_idle(_, true)` 는 `needs_input` 을 내리지 않았다 — 한 번 세워진 대기는 그것을
명시적으로 내리는 경로가 없는 한 뒤따르는 모든 `idle` 을 영구히 가린다.

## Decision

Codex 승인 대기를 `PermissionRequest` 훅으로 관측한다. 훅은 대상 surface 의 실행 상태를
`needs_input` 으로 주입하고, 같은 자리에서 `needs-input` surface hook(완료 알림 경로)과
공용 attention(`surface.completion` kind=needs_input)까지 쏜다 — 표시·해제 정책은
[ADR-0039](0039-surface-highlight-shared-primitive.md) ·
[ADR-0062](0062-attention-store-kind-aware-primitive.md) 를 그대로 따른다. 해제는
`PostToolUse` → `active`, `Interrupt` → `idle` 두 경로로 받는다. 설치 훅은 셋에서 여섯으로
늘고 matcher 는 어느 항목에도 걸지 않는다.

그리고 **`ChildTerminalRegistry::set_idle` 은 방향과 무관하게 `needs_input` 을 함께
내린다.** "턴이 끝났다"(idle)와 "턴 안에서 사람을 기다린다"(needs_input)는 동시에 참일 수
없으므로, 어느 쪽 전이든 대기는 해소된 것으로 본다. 이 규칙이 없으면 승인을 거절한 자식이
`Interrupt`(→ idle) 하나만 받고 그 자리에 영구히 얼어붙는다.

승인 대기는 **완료가 아니다.** `codex.spawn`/`codex.tell` 의 completion strategy
`terminal_states` 에 `needs_input` 을 넣지 않는다(claude 와 갈리는 지점) — 사람이 답해야
풀리는 중간 상태를 성공 종결로 읽으면 답 안 한 자식의 산출물을 downstream 이 전제하게 된다.

## Consequences

- **얻은 것**: 승인 대기가 상태 조회(`codex.state`)·탭/워크스페이스 표시·caller 알림
  세 표면에 모두 나타난다. 덤으로 `Interrupt` 매핑이 기존 결함 하나를 닫는다 — 중단된
  자식이 `active` 로 남아 기다리는 부모가 영원히 풀리지 않던 경로다. 호스트 불변식
  (`idle ⇒ ¬needs_input`)이 서면서 claude 쪽의 같은 잔류 경로도 함께 닫힌다.
- **잃은 것**: 해제가 늦다. 승인 직후 장기 실행 중에는 `needs_input` 이 그 도구의 실행
  시간만큼 남는다(측정: 45 s sleep 명령에서 45.4 s). 잔류가 아니라 지연이고, codex 가
  "승인됨" 이벤트를 주지 않는 한 훅만으로는 좁힐 수 없다. 또한 `tool_use_id` 가 없어 한
  surface 안에서 승인이 연달아 나면 해제가 tool 단위가 아니라 surface 단위로 뭉뚱그려진다
  (surface 사이에는 `TASTY_SURFACE_ID` 가 대상을 고정하므로 섞이지 않는다).
- **운영 비용 / 유지 부담**: `PostToolUse` 를 matcher 없이 설치하므로 tool 호출마다 hook
  프로세스가 하나 뜬다(claude 는 `AskUserQuestion` 으로 좁혀 이 비용을 피한다 — codex 는
  승인이 어느 tool 에서도 요청될 수 있어 좁힐 근거가 없다). 기존 설치 사용자는 훅 목록이
  바뀌었으므로 `tasty codex install` 재실행이 필요하다. 지원 범위는 **도구 실행 승인**
  하나뿐이다 — 일반 질문 입력(`request_user_input` 등)은 이 훅의 coverage 가 아니다.

## Alternatives Considered

- **`Stop` 만으로 해제한다** — `PostToolUse` 를 설치하지 않으면 per-tool 훅 비용은 없지만
  해제가 턴 끝까지(분 단위) 밀린다. 도구 실행 시간만큼의 지연보다 훨씬 나쁘다.
- **`PermissionRequest` 시점의 `PreToolUse` `tool_use_id` 를 plugin 이 기억해 짝지은 뒤
  그 id 의 `PostToolUse` 로만 해제한다** — 해제 창이 좁아지지 않는다(여전히 도구가 끝나야
  한다). 얻는 것은 동시 도구 구분뿐인데, 그 대가로 plugin 이 surface 별 대기 상태를
  들게 된다(호스트 registry 가 단일 SoT 라는 구조를 깬다).
- **화면 문구 매칭 / 무출력 시간으로 대기를 감지한다** — 승인 화면 문구는 codex 버전마다
  바뀌고, 무출력은 장기 실행과 구별되지 않는다. 훅이라는 구조적 신호가 있는데 휴리스틱으로
  내려갈 이유가 없다.
- **`set_idle` 은 그대로 두고 codex plugin 이 `interrupt` 에서 `active` → `idle` 두 번
  쏜다** — 호스트의 결함을 plugin 이 우회하는 형태다. 같은 잔류가 claude 쪽에도 있는데
  한쪽만 고치게 되고, 전이 도중 관측되는 가짜 `active` 가 생긴다.
- **`needs_input` 을 `terminal_states` 에 넣어 claude 와 맞춘다** — `codex.spawn` 노드의
  의미가 달라진다(입력 대기가 작업 성공이 된다). 그 변경은 이 결정의 범위가 아니다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `HOOK_EVENTS` 와 `hook_event_to_state` 가 갈리면
  `every_installed_event_has_a_state_mapping` 이 빨개진다 — 설치만 늘고 해석이 안 따라오는
  표류를 그 자리에서 잡는다.
- `set_idle` 의 불변식이 풀리면 `set_state_idle_clears_a_pending_needs_input` 이 빨개진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- codex 가 **승인 결정 자체를 알리는 이벤트**(승인 직후 발화하는 hook, 또는
  `PermissionRequest` 의 결과를 싣는 후속 이벤트)를 추가하면 해제 지연이 사라진다.
  재는 법: 훅 전부를 거는 격리 `CODEX_HOME` 으로 codex 를 띄워 승인 프롬프트를 만들고,
  승인한 시각과 각 훅의 발화 시각을 함께 기록해 그 사이에 오는 이벤트가 있는지 본다.
- `PermissionRequest` payload 에 `tool_use_id` 가 생기면 대기↔해제를 tool 단위로 짝지을 수
  있다. 재는 법: 같은 절차로 `PermissionRequest` payload 의 키 목록을 덤프해
  `PreToolUse`/`PostToolUse` 의 키 목록과 대조한다.
- codex 가 **일반 질문 입력**(`request_user_input` 등)에 대응하는 훅을 노출하면 coverage 를
  넓힐 수 있다. 재는 법: codex 바이너리의 hook event 목록을 덤프해 지금의 12 개와 대조한다.

## References

- [ADR-0039](0039-surface-highlight-shared-primitive.md) — 공통 attention 과 실제 포커스 해제 원칙
- [ADR-0062](0062-attention-store-kind-aware-primitive.md) — kind 별 공통 attention 모델
- [ADR-0072](0072-child-state-hook-observation-fusion.md) — hook 보고와 호스트 관측의 융합 우선순위
- [features/surface-highlight](../features/surface-highlight/index.md) — 표시 우선순위·해제·mirror 정책
- [plugins/codex](../plugins/codex/index.md) — 훅 표와 실측값(현재 위치)
- 코드 근거(결정이 실현된 **현재** 위치): `tasty-plugin-codex` 의 `HOOK_EVENTS` ·
  `hook_event_to_state` · `hook_side_effects`, `ChildTerminalRegistry::set_idle`
