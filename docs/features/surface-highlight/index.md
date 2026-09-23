# Surface Highlight (주의 환기)

- **Status**: Implemented
- **주체**: AI Agent · 로컬 사용자
- **ADR**: [ADR-0039](../../adr/0039-surface-highlight-shared-primitive.md) ·
  [ADR-0062](../../adr/0062-attention-store-kind-aware-primitive.md)(kind 확장) ·
  [ADR-0098](../../adr/0098-mirror-local-attention-raise-suppressed.md)(mirror 로컬 발동 억제) ·
  [ADR-0104](../../adr/0104-mirror-attention-clear-forwarded-to-owner.md)(mirror 해제 edge 전달) ·
  [ADR-0107](../../adr/0107-attention-clear-ipc-symmetry.md)(IPC/CLI 해제·조회) ·
  [ADR-0109](../../adr/0109-hard-occupancy-attention-clear-holder-only.md)(하드 점유 중 해제는 홀더만)
- **코드**: `src/core/state/attention.rs` (상태·헬퍼·mirror 로컬 발동 억제 게이트·하드 점유 해제 게이트·attach forward diff) · `src/gfx/gpu.rs` (focus 해제) ·
  `src/adapters/ui/{divider,tab_bar,sidebar}` (소비처) ·
  `src/adapters/ipc/handler/surface/completion.rs` (completion producer) ·
  `src/adapters/ipc/handler/surface/attention.rs` (IPC 조회·해제) ·
  `crates/tasty-plugin-claude/src/hook.rs` (Claude Stop/session-end/notification hook producer) ·
  `crates/tasty-plugin-codex/src/handlers.rs::hook_side_effects` (Codex PermissionRequest hook producer) ·
  `src/app/dispatch_domain.rs::cascade_terminal_command_completed` (OSC 133 명령 완료 producer) ·
  `src/core/attach_runtime.rs::forward_attention`/`apply_attached_attention_clear` +
  `src/app/attach_client.rs` (원격 attach mirror 전파 — server→client push, client→server 해제 edge)
- **화면**: 없음 — 세 소비처(테두리·탭·사이드바)에 투영되는 상태 (전용 화면 없음)

## 목적

surface 가 "확인 대기(주의 환기)" 상태임을 사용자에게 알린다. 여러 경로(producer)가
같은 상태를 발동시킬 수 있는 **producer 중립 공유 primitive** 로, 특정 producer(toast·
completion 등)의 소유물이 아니다. 이렇게 두면 후속 producer(hook·명령완료 자동감지·plugin)가
같은 API 를 호출해 동일한 시각 효과를 재사용할 수 있다.

## 내부 동작 (headless-valid)

- **상태**: `AttentionStore`(`CoreState.attention`) — surface id → `{ kind, raised_at }`
  레코드. `busy_surfaces` 형태(폴링 아닌 직접 CRUD 헬퍼)를 미러하되, bool 집합이 아니라
  kind 를 들고 있어 "왜 발동했는지" 를 표현한다. kind 는 `Completion`(작업 완료) ·
  `NeedsInput`(응답 대기, 우선순위가 더 높다 — 아래 "kind → 효과") 2종.
  `NotificationStore`(알림 패널)와는 **별개 저장소** — attention 레코드가 곧 패널 아이템은
  아니다(아래 "kind → 효과" 참고).
- **발동(raise)**: `raise_attention(surface_id, kind)` — `surface_id == 0`(미지정)은 무시.
  같은 surface 에 다시 raise 하면 최신 kind 로 완전히 대체된다(레코드는 surface 당 1개).
  **mirror(원격 attach) surface 는 이 로컬 축의 대상이 아니다** — `raise_attention` 이
  `is_mirror_surface` 로 걸러 no-op 으로 끝낸다(아래 "Producer" 의 mirror 항목). 미러의
  값은 서버 push 전용 진입점(`set_mirror_surface_attention`)으로만 들어온다.
- **해제(clear)**: `clear_attention(surface_id) -> bool` — 반환값은 **실제로 레코드를
  제거했는지**(= 해제 edge)다. 레코드가 없는 상태의 호출은 no-op(`false`)이라, 매 프레임
  도는 호출부에서도 상태가 바뀐 순간에만 신호가 나간다. 세 경로에서 호출된다:
  1. 매 렌더 프레임 **실제 렌더 시점 포커스** surface 에 대해 자동 호출(`gpu.rs`) —
     에이전트가 주입한 포커스가 아니라 실 사용자 포커스라 불가침 원칙 1 에 안전. kind 와
     무관하게 단일 규칙(`Completion`/`NeedsInput` 모두 이 경로로 해제).
  2. 그 surface 를 source 로 하는 알림을 읽음 처리(개별/전체)했을 때, `CoreState::
     mark_notification_read`/`mark_all_notifications_read`(`state/attention.rs`)가
     그 surface 에 남은 안읽음 알림이 없는 경우에만 호출 — 같은 surface 의 다른 알림이
     아직 안읽음이면 지우지 않는다(무조건 clear 시 다른 안읽음 알림의 주의 환기를
     오해제하는 엣지 케이스가 있어 조건부로만 지운다).
  3. `surface.attention.clear` IPC / `tasty surface attention clear` CLI(아래 "인터페이스").
     앞의 두 경로가 전부 GUI 로컬 사건이라, **headless 인스턴스에서는 이것이 유일한 해제
     수단**이다. `kind` 를 주면 현재 기록된 kind 가 일치할 때만 지운다. 대상이 **mirror
     surface** 이거나 **하드 점유(원격 attach) 중**이면 거절한다 — 두 경우 모두 그 상태의
     소유자가 다른 인스턴스라, 여기서 지우면 소유자와 갈라진다(아래 "인터페이스").

  **해제 권한 — 하드 점유 중에는 홀더만 해제한다**([ADR-0109](../../adr/0109-hard-occupancy-attention-clear-holder-only.md)).
  위 1·2 경로는 그 인스턴스의 **로컬 사용자 사건**이라 로컬 축 진입점
  `clear_attention_local(surface_id) -> bool` 을 지나고, 그 안에서 게이트 술어
  `local_attention_clear_allowed(surface_id)`(= `!is_hard_occupied`)가 한 번 평가된다.
  하드 점유(attach)된 surface 의 주체는 홀더이고 로컬 사용자는 readonly 이므로
  (ADR-0040) "확인했다" 는 판정도 홀더의 것이다 — 점유 중 서버 GUI 에서 그 surface 를
  포커스하거나 알림 패널에서 읽음 처리해도 attention 은 남는다. 세부:

  - **게이트는 `clear_attention` 이 아니라 `clear_attention_local` 에 있다.** 홀더의 해제를
    적용하는 서버측 경로(`apply_attached_attention_clear`)는 primitive 를 직접 부르므로
    게이트를 지나지 않는다 — 게이트가 primitive 안에 있으면 그 경로까지 막혀 점유 중
    해제 주체가 0 이 된다.
  - **경로 3(IPC 해제)은 `clear_attention_local` 을 지나지 않는다.** 같은 술어
    (`is_hard_occupied`)를 API 경계에서 먼저 평가해 **명시적 에러로 거절**하고, 통과한
    경우에만 primitive 를 부른다. 게이트를 우회하는 것이 아니라 같은 정책을 더 앞에서
    집행하는 것이다 — 래퍼를 태우면 점유 중 호출이 조용한 no-op 으로 끝나 호출자가
    "지웠다" 와 구분할 수 없다(ADR-0107).
  - **알림 읽음 자체는 점유와 무관하게 처리된다.** 게이트가 걸리는 것은 attention 해제
    한 가지뿐 — 읽음은 이 인스턴스 사용자의 알림 패널 상태이고 attention 은 홀더와
    공유하는 상태다. "모두 읽음" 도 알림 전부를 읽음 처리하되 점유 중 surface 의
    attention 만 남긴다.
  - **soft 점유(child-terminal)에는 걸리지 않는다** — ADR-0040 의 약한 점유는 로컬
    사용자를 배제하지 않는다. 같은 실-포커스 블록의 soft 점유 지연 청소
    (`reconcile_soft_occupancy_on_focus`)도 이 게이트와 무관하게 그대로 돈다.
  - **미러 인스턴스에서는 걸리지 않는다** — 점유는 surface 를 소유한 인스턴스가 기록하므로
    미러의 `OccupancyRegistry` 에는 그 lock 이 없다. 미러 사용자의 확인은 그대로 제거
    edge 가 되어 서버로 forward 된다(아래 해제 항목).
  - **데드락 없음**: 게이트는 상태를 저장하지 않고 매 호출 점유를 다시 묻는다. 점유가
    풀리면(detach / force-detach / 연결 끊김) 서버 로컬 포커스가 자동으로 해제 주체로
    복귀해 stale 레코드를 회수한다.

  **해제 판정은 surface 를 소유한 인스턴스 밖에서도 일어난다.** 원격 attach 로 mirror
  중이면 위 1·2 경로가 **미러 인스턴스**에서 발동한다(미러 사용자의 실-포커스, 미러 로컬
  알림의 읽음 처리). 미러 사용자의 어떤 행동도 서버의 이 두 경로를 발동시킬 수 없고,
  서버 사용자가 하드 점유된 surface 를 포커스하더라도 **아래 해제 권한 게이트에 막힌다.**
  그래서 그 판정을 옮기지 않으면 서버 쪽 해제 주체가 아예 없다 — mirror surface 에서 발생한
  **제거 edge 만** `StreamControl::ClientAttentionClear` 로 소유 인스턴스에 1 회 전달한다.
  규칙 자체는 그대로이고 판정 결과만 옮긴다(아래 "원격 attach mirror 로의 전파" 의 해제 항목).
- **조회**: 프로세스 내부는 `attention_kind(id) -> Option<AttentionKind>`,
  `attention_count_of_kind(kind, &[id]) -> usize`,
  `attention_dominant_kind(&[id]) -> Option<AttentionKind>`(목록 중 최고 우선순위 kind —
  탭 제목·collapsed rail dot 처럼 여러 surface 를 하나의 색으로 압축해야 하는 소비처 전용).
  프로세스 밖에서는 `surface.attention.get` IPC / `tasty surface attention get` 이 단일
  surface 의 kind 를 돌려준다(`"completion"` / `"needs_input"` / `null`). mirror surface 도
  조회는 허용된다 — 서버가 push 해 준 로컬 레코드를 읽는 것이라 소유권 문제가 없다.
- **kind → 효과**: `effects_of(kind) -> AttentionEffects { level, panel_item, os_notify, sound }`
  가 host/cascade 비의존 순수 함수로 정책을 표현한다(`crates/tasty-plugin-claude/src/hook.rs`
  의 `apply_hook` 과 동형 패턴). `level` 은 디자인 rank 토큰(`--tasty-attention-rank-*`)을
  미러링한 우선순위 — `NeedsInput`(30) > `Completion`(10). 두 kind 모두 `panel_item = false`
  — attention 레코드 자체는 알림 패널 아이템을 만들지 않는다. 패널 노출이 필요한
  producer(toast, windows resume)는 지금처럼 `NotificationStore::add()` 를 별도로 직접
  호출한다. OSC 133 명령 완료는 이 `add()` 를 호출하지 않으므로, 테두리/탭 제목은
  발동하되 알림 패널에는 아이템이 쌓이지 않는 조합이 성립한다.
- **효과 3채널** (kind 별로 색이 갈린다 — `NeedsInput`=노랑(`accent_warning`),
  `Completion`=파랑(`accent_primary`)):
  1. **테두리** — attention surface 둘레 강조 보더(2px, `divider.rs`). 우선순위:
     `NeedsInput` > 점유(soft/hard, ADR-0040) > `Completion` — 점유 중 surface 는
     `Completion` 테두리를 억제하지만 `NeedsInput` 은 억제하지 않는다("지금 답하지 않으면
     멈춘다"는 신호를 점유가 가리면 안 되기 때문).
  2. **탭 제목** — 그 탭에 속한 surface 들의 `attention_dominant_kind` 로 제목색 결정.
     순서: `NeedsInput`(노랑) → `Completion`(파랑) → active(`text_primary`) → 평상시
     (`text_muted`). busy 녹색 dot 과 별개 채널(`tab_bar/tab.rs`).
  3. **워크스페이스 배지/dot** — 사이드바 워크스페이스 행 우측: full 은 kind 별 개수
     숫자 배지 2종(트레일링 슬롯은 kind 무관 유지 — 1개면 단독 그 자리, 2개면 `NeedsInput`
     이 좌측·`Completion`이 우측, 간격 `badge-group-gap`=`spacing_xs`), collapsed 은 dot
     1개(kind 우선순위로 대표색 선택: `NeedsInput` > `Completion` > running) (`sidebar/view.rs`).

### 원격 attach mirror 로의 전파 (server→client)

`AttentionStore` 는 **인스턴스 로컬**이다 — 인스턴스 간 자동 동기화가 없다. 그래서 원격
attach 로 워크스페이스를 mirror 하는 쪽은 서버(surface 를 소유한 인스턴스)에서 발동한
attention 을 자기 store 로 받아야 한다. 특히 `NeedsInput` 은 Claude·Codex 플러그인 훅이
PTY 가 있는 인스턴스에서만 발화하고 그 훅 신호는 미러에 도달하지 않으므로, push 가 없으면
미러 사용자에게 "응답 필요" 가 도달할 경로가 **원천적으로 없다**.

- **진실 원천은 surface 를 소유한 인스턴스다.** 미러는 서버가 push 한 값을 **반영만** 하고
  자기 판단으로 레코드를 만들지 않는다(아래 Producer 의 mirror 항목).
  busy(`StreamControl::Activity`)와 같은 방향·같은 이유의 단방향 채널이다.
  단, "미러가 그 사건을 볼 수 없다" 는 **`NeedsInput` 에만 해당한다** — Claude·Codex 훅은
  PTY 가 있는 쪽에서만 발화한다. `Completion` 쪽 producer 중 OSC 133 명령 완료·Bell·OSC 9/777 은
  서버 바이트를 파싱하는 미러에서도 **실제로 발화하며**, 그래서 정책적으로 억제한다.
- **서버측**: 1Hz `Tick::Busy` 에 편승해 `CoreState::forward_attention`
  (`core/attach_runtime.rs`)이 `attention_forwards`(`core/state/attention.rs`)가 계산한
  **점유 중 surface 의 변화분**만 `StreamControl::Attention{surface_id, kind}` 로 holder
  client 에 push 한다. `kind: null` 이 해제 — 별도 변형이 아니라 kind 의 부재다.
  `last_forwarded_attention` 캐시로 값이 실제로 바뀐 tick 에만 프레임이 나가고(스팸 없음),
  점유가 풀렸다 재점유되면 캐시를 버려 새 holder 가 baseline 을 다시 받는다. 캐시가 holder 도
  함께 기억하므로 해제와 다른 client 의 재점유가 한 tick 창 안에 끝나도 새 holder 가 baseline 을
  받는다.
- **client 측**: reader 스레드가 `MirrorEvent::Attention` 으로 버퍼링하고, 세션의
  `remote_to_local` 매핑으로 로컬 mirror surface id 를 찾아
  `CoreState::set_mirror_surface_attention` 으로 적용한다. 값은 **기존 `AttentionStore` 에
  그대로** 들어가므로 세 소비처(테두리·탭 제목·워크스페이스 배지)가 코드 변경 없이 미러
  워크스페이스에서도 동작한다(busy 가 `mirror_busy_surfaces` 별도 집합을 쓰는 것과 다른
  점 — attention 에는 `refresh_busy_surfaces` 같은 wholesale 교체가 없어 분리할 이유가 없다).
- **적용 API 는 로컬 producer 와 분리한다.** 미러가 push 를 반영할 때
  `raise_attention`/`clear_attention` 을 타지 않고 `set_mirror_surface_attention` 전용
  진입점을 쓴다 — 그 두 API 는 로컬 producer 축이다. `raise_attention` 에는 mirror
  surface 로컬 발동 억제 게이트가 있고, `clear_attention` 에는 mirror 해제 edge 의
  forward 큐 push 가 있다. 같은 함수를 타면 서버 push 가 그 억제 게이트에 막히고, 서버가
  내려준 해제가 곧바로 서버로 되돌아가는 에코가 된다.
- **해제 edge 를 소유 인스턴스로 되돌린다 (client→server).** 미러에서 그 surface 를
  확인해 레코드가 **실제로 제거되면**, 그 사실 1 회를 `StreamControl::ClientAttentionClear`
  로 서버에 보내 서버 레코드도 지운다(`CoreState::apply_attached_attention_clear`, holder
  검증 포함). 큐 push 는 호출부가 아니라 `clear_attention` 안에서 하므로 포커스 해제와
  미러 로컬 알림 읽음이 **균일하게** 덮인다. 전송은 **제거 edge 에서만** 일어나 포커스를
  유지해도 프레임이 반복되지 않는다 — 주기 전송도, 별도 last-sent 추적도 없다.
  에코 루프는 없다: 서버가 지우면 다음 diff 가 `kind: null` 을 미러로 push 하는데 미러엔
  이미 레코드가 없어 edge 가 생기지 않는다(idempotent 수렴).
- **수렴 보장 범위.** push 가 보장하는 수렴은 **wire 축의 유실**이다 — 프레임이 드롭·지연
  돼도 서버 값이 그대로면 다음 tick 에 같은 값이 다시 나간다. 미러가 자기 store 를 로컬로
  바꾸는 축(발동·해제)은 자동 수렴이 아니라 위 두 장치로 맞춘다: 발동은 애초에 미러에서
  일어나지 않게 막고(Producer 의 mirror 항목), 해제는 edge 를 서버로 되돌린다.
- **알려진 엣지(의도된 동작)**: 미러가 그 surface 를 **이미 포커스한 상태**에서 서버가 새
  raise 를 push 하면, 미러는 레코드를 심자마자 다음 렌더 프레임에서 지우고 해제 edge 를
  보내 배지가 사실상 뜨지 않는다. 단일 인스턴스 로컬 동작과 같은 규칙이다(포커스된
  surface 에 raise 하면 지금도 다음 프레임에 지워진다) — 규칙이 일관되므로 그대로 둔다.
- **teardown**: mirror surface 가 사라지면(`cleanup_mirror_workspace` 의 세션 정리,
  `apply_mirror_structural_delta` 의 removed 처리) `forget_mirror_surface_attention` 으로
  레코드도 함께 버린다 — 로컬 id 가 재사용될 때 stale attention 이 새 surface 에 잘못
  붙는 것을 막는다. **kind 변환**(terminal → 다른 kind) 경로는 대상이 아니다: 거기서는
  surface 가 계속 존재하고 서버도 여전히 레코드를 들고 있어, 미러만 지우면 diff 기반
  push 가 다시 보내주지 않아 세션이 끝날 때까지 divergence 가 남는다.

## Producer

attention 을 발동시키는 경로. 여러 종류가 공존한다 — attention 자체는 어느 producer 에도
종속되지 않는다. Toast/completion IPC·CLI/OSC 133 은 `kind = Completion` 으로,
Claude·Codex hook 은 이벤트별로 `Completion` 또는 `NeedsInput` 으로 발동한다.

- **Toast 알림** — toast 알림이 신규 발화(coalesce 아님)되면 그 source surface 를 attention
  (`dispatch_domain.rs` 주경로 + `event_handler.rs` windows resume 경로). 이 두 경로는
  `NotificationStore::add()` 를 직접 호출해 패널 아이템도 함께 만든다. `kind = Completion`.
- **Completion (IPC/CLI)** — surface 가 작업을 완료했다는(또는 응답이 필요하다는) 신호.
  `surface.completion` IPC / `tasty surface completion --surface <id>` CLI. `kind` 는
  선택 파라미터(`"needs_input"` 이면 `NeedsInput`, 생략 포함 그 외는 `Completion` — 하위
  호환: CLI/OSC 133/toast/windows-resume producer 는 kind 를 모른 채 이 IPC 를 호출한다).
  핸들러가 대상 engine 에 `raise_attention(surface_id, kind)` 를 적용하고 cascade 가 redraw 를
  얹는다(headless 에는 cascade 가 없어 핸들러가 적용 주체다 — 아래 "구현").
  패널 아이템은 만들지 않는다.
- **Claude hook (plugin)** — `crates/tasty-plugin-claude/src/hook.rs` 의 `apply_hook`이
  이벤트별로 `HostCall::SurfaceCompletion { surface_id, kind }`를 산출하고, `deliver()`가
  이를 위 `surface.completion` IPC(`{ "surface_id": surface_id, "kind": kind }`)로 매핑해
  그대로 재사용한다 — 새 프로토콜을 만들지 않고 기존 completion producer 를 plugin 이
  호출하는 형태.
  - `"stop"`/`"subagent-stop"`(턴 완료)·`"session-end"`(세션 종료) → `kind = "completion"`.
  - `"notification"`(비-`idle_prompt` — 승인/plan 허락 등 확인 대기)·`"pre-tool-use"`
    (`AskUserQuestion` matcher — 선택지 UI) → `kind = "needs_input"`.
  - `"prompt-submit"`/`"session-start"`/`"active"`(작업 시작 신호)·`"post-tool-use"`(답변
    직후 active 복귀)는 대상이 아니다.
  - `tasty claude install`이 등록한 훅(`crates/tasty-plugin-claude/src/install.rs`)을 통해
    Claude 세션이 이벤트를 발화할 때마다 자동 — 별도 사용자 조작 불필요.
- **Codex hook (plugin)** — `crates/tasty-plugin-codex/src/handlers.rs` 의 `hook_side_effects`
  가 같은 `surface.completion` IPC 를 호출한다. 발동 이벤트는 `PermissionRequest`
  (도구 실행 승인 프롬프트가 뜨기 직전) 하나이고 `kind = "needs_input"` 이다. 해제 쪽
  (`PostToolUse` → active, `Interrupt` → idle)은 attention 을 다시 발동시키지 않는다 —
  attention 해제는 이 축이 아니라 실제 포커스가 한다(위 "해제"). Codex 의 턴 완료
  (`Stop`)는 `Completion` attention 을 발동시키지 않는다(claude 와 갈리는 지점 — 지금은
  완료 알림 경로만 쓴다). 상세·실측은 [plugins/codex](../../plugins/codex/index.md).
- **OSC 133 명령 완료 (셸 통합)** — `cascade_terminal_command_completed`
  (`src/app/dispatch_domain.rs`)가 OSC 133 D phase(개별 셸 명령 종료)
  를 받을 때마다 exit code(성공/실패) 무관하게 항상 `raise_attention(surface_id,
  AttentionKind::Completion)` 을 호출한다 — Claude plugin 과 달리 IPC 왕복 없이 host
  프로세스 내부에서 engine 을 직접 호출(같은 cascade 함수가 이미 `engine`을 들고 있음).
  exit code 정보는 이 호출로 소비되지 않고 같은 이벤트가 이미 채우는
  `command_index::on_boundary` 의 memory 기록(`tasty.commands.*` 의 `exit_code`
  필드)과 `HookEvent::CommandCompleted(exit_code)` 훅 payload 양쪽에
  그대로 보존된다. 셸 프로세스 자체의 시작/끝(`terminal.spawn`/`ProcessExit`)이
  아니라 그 **안에서 실행되는 개별 명령**(`docker build` 등) 단위로 발동한다는 점이
  Claude producer 와의 차이 — 셸이 OSC 133 통합을 로드해야만 동작(미설치 시
  "셸 통합 미설치" 안내 배너만 뜨고 attention 은 발동하지 않음). `NotificationStore::add()`
  를 호출하지 않으므로 알림 패널에는 아이템이 쌓이지 않는다.
- **원격 attach 서버 push (mirror 한정) — mirror surface 는 로컬 producer 대상에서
  제외되고, 서버 push 가 유일한 소스다.** mirror(원격 attach client) surface 의 attention
  은 `StreamControl::Attention` → `CoreState::set_mirror_surface_attention` 으로만 들어온다
  (위 "원격 attach mirror 로의 전파" 절).
  - **로컬 producer 가 미러에서도 발화한다는 점에 주의.** 미러 터미널은 로컬 PTY 가 없지만
    서버가 흘려준 바이트를 그대로 파싱하므로 OSC 133 D·Bell·OSC 9/777 이 미러에서도
    나오고, `surface.completion` IPC/CLI 는 미러 인스턴스에서 도는 에이전트·플러그인이
    **로컬 mirror surface id** 로 직접 부를 수 있다. 그대로 두면 같은 사건에 서버·미러가
    각각 별개 레코드를 갖는 이중 상태가 되고, 미러에서 지운 값은 서버에 닿지 않는다.
  - **억제는 `raise_attention` 단일 진입점에서 집행한다** (`src/core/state/attention.rs`) —
    producer cascade 마다 게이트를 흩뿌리지 않으므로 위 producer 전부와 앞으로 추가될
    producer 까지 함께 덮인다. 서버 push 적용 경로는 이 함수를 타지 않아 막히지 않는다.
  - **억제 대상은 attention 레코드 하나뿐**이다. 같은 cascade 가 만드는 알림 패널 아이템·
    토스트·훅(`HookEvent`) 발화는 게이트 밖이라 미러에서도 그대로 동작한다 — 원격 작업
    중의 벨/알림은 여전히 유효한 로컬 UX 다.
  - `surface.completion` 이 미러를 대상으로 불린 경우도 **서버로 forward 하지 않고 억제**
    한다. 근거·대안은 [ADR-0098](../../adr/0098-mirror-local-attention-raise-suppressed.md).
- **후속(미구현)** — Claude·Codex 외 plugin 이 자체 완료/확인대기
  신호를 이 attention API 에 연결하는 것. attention API 는 이들이 호출 가능하게 계속
  열려 있다.

## 인터페이스

- **AI Agent (IPC/CLI)**: `tasty surface completion --surface <id> [--kind needs_input]` /
  IPC `surface.completion { surface_id, kind? }` — 대상 surface 를 attention 발동.
  `surface_id` **필수**(포커스 독립, 불가침 원칙 1). `kind` 생략(또는 `needs_input` 외 값)은
  `Completion`. 권한 `Notification`.
- **AI Agent (IPC/CLI) — 해제**: `tasty surface attention clear --surface <id> [--kind <k>]` /
  IPC `surface.attention.clear { surface_id, kind? }`. `surface_id` **필수**. `kind` 를 주면
  현재 기록된 kind 가 그 값일 때만 지운다(생략 = kind 무관 해제, 알 수 없는 값은 거절).
  attention 이 없던 surface 도 성공한다(idempotent) — 응답 `{ ok, surface_id, cleared,
  previous_kind }` 의 `cleared` 가 실제로 지웠는지를 알린다. 권한 `Notification`.
  **거절되는 대상 3 종**(전부 명시적 `invalid_params`, 조용한 no-op 아님):
  존재하지 않는 surface · **mirror surface** · **하드 점유 중인 surface**. 뒤의 둘은 그
  attention 의 소유자가 다른 인스턴스라 여기서 지우면 소유자와 갈라진다 —
  mirror 거절은 발동 축의 억제(ADR-0098)와 대칭이고 ADR-0104 가 이 IPC 를 도입하는 트랙에
  맡긴 집행이다. 미러 사용자가 그 surface 를 **실제로 보고** 확인한 경우는 위 해제 경로
  1·2 로 잡혀 `ClientAttentionClear` 로 소유 인스턴스에 전달된다 — 즉 미러에서 지우는
  정당한 길은 막히지 않고, 에이전트가 보지 않은 채 대신 지우는 길만 막힌다.
- **AI Agent (IPC/CLI) — 조회**: `tasty surface attention get --surface <id>` /
  IPC `surface.attention.get { surface_id }` → `{ surface_id, kind }`. `kind` 는
  `"completion"` / `"needs_input"` / `null`. mirror·점유 중에도 허용(읽기 전용).
  권한 `Notification`.
- **원격 attach 스트림**: `StreamControl::Attention { surface_id, kind }`(server→client,
  `crates/tasty-ipc/src/stream.rs`) — 점유 중 surface 의 attention 변화분을 holder client 로
  push. `surface_id` 는 **원격 id**(client 가 자기 매핑으로 로컬 mirror id 를 찾는다),
  `kind` 는 `"completion"`/`"needs_input"`(`AttentionKindWire`)이고 `null` 이 해제.
  직렬화 문자열은 위 `surface.completion` IPC 의 `kind` 파라미터와 같은 어휘다.
- **원격 attach 스트림(해제)**: `StreamControl::ClientAttentionClear { surface_id }`
  (client→server, `crates/tasty-ipc/src/stream.rs`) — 미러에서 그 surface 를 확인해
  레코드가 제거된 **edge 1 회**. `surface_id` 는 **원격 id**. 인가는 기존 attach 하드
  점유 모델 그대로(요청 client 가 그 워크스페이스의 holder 여야 적용).
- **사용자 트리거**: 직접 트리거 없음. 효과는 세 소비처로 표시되고, surface 에 포커스하거나
  그 surface 발 알림을 알림 패널에서 읽음 처리하면 해제(잔여 안읽음이 없을 때, kind 무관).

> **검증 한계(문서화 — 원격 attach 전파 한정)**: GUI 두 인스턴스를 실제로 attach 해 미러
> 사이드바의 배지·테두리·탭 제목 색을 눈으로 확인하는 것은 이 headless 작업 환경(GPU
> 디스플레이 없음)에서 실행할 수 없다. 대신 `tests/attach_attention_loopback.rs` 가 실제로
> 기동한 `tasty` 서버 인스턴스에 raw `TcpStream` 으로 attach 점유를 획득한 뒤,
> `surface.completion` IPC 로 서버에서 attention 을 raise 시켜 `attention` Control 프레임이
> 원격 surface id·kind 와 함께 도착하는지(그리고 값이 그대로면 다시 나가지 않는지)를
> 프로토콜 레벨에서 검증한다. 해제(`kind: null`) 전파와 dedup·재점유 baseline 은
> `src/core/state/attention.rs` 의 단위 테스트가 덮는다 — 서버가 자기 값을 내리는 시점을
> 프로토콜 e2e 로 구동할 수단이 제한적이기 때문이다. 알림 읽음은 release IPC 표면에 없고,
> 실 렌더 포커스는 서버가 GUI 인스턴스일 때 **debug IPC 로 구동할 수 있다** —
> `debug.switch_workspace` 로 그 워크스페이스를 활성화하면 그 surface 가 실 렌더 포커스를
> 얻어 `gpu.rs` 의 매 프레임 해제 경로가 실제로 실행된다. 하드 점유 중 해제 게이트
> (ADR-0109)의 e2e 가 이 수단을 쓴다. `surface.attention.clear` 도 이 자리를 대신하지
> 못한다: 그 시나리오의 서버 surface 는 하드 점유 중이라 아래 거절 3 종에 걸린다. 해제 축
> 자체(로컬·headless)는 `tests/e2e_tests.rs` 의 attention 왕복(raise → kind 필터 불일치 →
> 해제 → idempotent → 재발동)과 `tests/attach_attention_loopback.rs` 의 하드 점유 거절,
> `src/adapters/ipc/handler/surface/attention.rs` 의 mirror 거절 단위 테스트가 실행한다.
> 미러가 받은 값을 실제 픽셀로 그리는 부분은
> 세 소비처가 로컬 attention 과 **같은 `AttentionStore`** 를 읽는다는 사실로 대체한다
> (`docs/features/native-file-picker/index.md` 의 "검증 한계" 와 동종의 한계).
>
> **미러 측 로컬 발동 억제**와 **미러 측 해제 edge 큐 적재**도 같은 이유로 loopback e2e 가
> 아니라 단위 테스트가 고정한다(`src/state/tests.rs` 5 종 · `src/core/state/attention.rs` 6 종)
> — loopback client 는 raw `TcpStream` 이라 미러 `AttentionStore` 자체가 없어 그 경로를
> 실행조차 하지 않는다. loopback e2e 가 실제로 실행하는 것은 그 프레임을 받은 **서버 측**
> 절반(역직렬화 → holder 검증 → 레코드 제거)이다.

## 비-목표 (Out of scope)

- 에이전트가 포커스를 주입해 attention 을 해제하는 경로(사용자 입력 재현 → debug 격리 대상).
  `surface.attention.clear` 는 포커스를 건드리지 않고 상태만 지우므로 이 금지에 걸리지 않는다.
- 워크스페이스/탭 일괄 해제. 대상은 항상 surface id 로 명시한다(포커스 독립) — 일괄 해제는
  요구가 생길 때 별도로 판단한다.
- 해제와 알림 읽음 처리의 합성. `AttentionStore` 와 `NotificationStore` 는 별개 저장소라,
  `surface.attention.clear` 는 attention 만 지우고 알림 읽음 상태는 건드리지 않는다.
- `Completion`/`NeedsInput` 외 다른 kind(예: `error` rank 40, `approval` rank 20 — 디자인
  토큰은 예약돼 있으나 이번 구현 범위 밖).
- toast 알림 시스템 자체 리팩터링(발행/coalesce/store 는 불변, attention insert 위치만 이전됨).

## Acceptance Criteria

- Given surface S When `tasty surface completion --surface S` Then S 가 attention 발동되어
      `attention_kind(S) == Some(Completion)`.
- Given surface S When `tasty surface completion --surface S --kind needs_input` Then
      `attention_kind(S) == Some(NeedsInput)`.
- Given attention 발동된 S When S 가 실제 포커스를 얻음 Then attention 자동 해제(kind 무관).
- Given 워크스페이스 W 에 `Completion` surface M개·`NeedsInput` surface N개 Then full
      사이드바에 파란 배지 M·노란 배지 N 이 공존(각각 >99 시 "99+"), N==0 이면 파란 배지만
      단독으로 우측(기존 자리)에 표시.
- Given 대상(탭/워크스페이스)에 `Completion`·`NeedsInput` surface 가 섞여 있음 Then
      탭 제목 색과 collapsed rail dot 색은 `NeedsInput` 이 이긴다(우선순위 30 > 10).
- Given surface S 가 점유(soft/hard, ADR-0040) 중 When `NeedsInput` attention 이 발동
      Then 테두리가 억제되지 않고 노랑으로 그려진다(점유는 `Completion` 만 억제).
- Given toast 알림 신규 발화 Then 그 source surface 가 attention 발동(producer 공존 회귀 없음).
- Given attention 발동된 S When `tasty surface attention clear --surface S` Then
  `attention_kind(S) == None` 이고 응답 `cleared == true`, `previous_kind` 가 직전 kind.
- Given `NeedsInput` 이 발동된 S When `--kind completion` 으로 해제 Then 지워지지 않고
  응답 `cleared == false`(필터 불일치) — 같은 kind 로 호출하면 지워진다.
- Given attention 이 없는 S When 해제 Then 성공하고 `cleared == false`(idempotent, panic 없음).
- Given 존재하지 않는 surface id 또는 알 수 없는 `kind` When 해제 Then 명시적 에러.
- Given S 가 하드 점유(원격 attach) 중 When 해제 Then 점유를 사유로 명시한 에러로 거절되고
  attention 은 그대로 남는다 — 조회(`surface.attention.get`)는 점유 중에도 성공한다.
- Given S 가 mirror(원격 attach client) 워크스페이스의 surface When 해제 Then mirror 를
  사유로 명시한 에러로 거절되고 로컬 레코드도 그대로 남는다(해제 forward 도 타지 않는다)
  — 조회는 mirror 에서도 성공한다.
- Given GUI 없이 기동한 headless 인스턴스 When `tasty surface completion` 으로 발동하고
  `tasty surface attention get`/`clear` 로 조회·해제 Then 전부 동작한다(해제 producer 0 개
  상태 해소).
- Given surface S 발 안읽음 알림 1건 When 그 알림을 읽음 처리 Then S 의 attention 해제.
- Given surface S 발 안읽음 알림 2건 When 그중 1건만 읽음 처리 Then S 의 attention 유지(엣지
      케이스) — 나머지도 읽음 처리하면 그제서야 해제.
- Given surface S 에서 Claude 세션 실행 중 When Claude 가 한 턴 응답을 마쳐 Stop hook
      (`claude.hook stop`)이 발동 Then S 가 `Completion` kind 로 attention 발동.
- Given surface S 에서 Claude 가 승인/plan 허락 등 확인이 필요해 `notification`(비-
      `idle_prompt`) 또는 `pre-tool-use`(`AskUserQuestion`) hook 이 발동 Then S 가
      `NeedsInput` kind 로 attention 발동 — `Completion` 과 다른 색(노랑)으로 구분된다.
- Given surface S 에서 Claude 세션이 끝나(`session-end` hook) 자식 상태가 정리될 때 Then
      S 가 동일하게 attention 발동.
- Given surface S 에서 Claude 가 `prompt-submit`/`session-start`/`active`(작업 시작) 신호를
      보낼 때 Then attention 은 발동하지 않는다(완료/확인대기와 구분, 회귀 방지).
- Given surface S 에서 OSC 133 셸 통합이 설치된 셸이 명령을 성공으로 종료(exit code 0)
      When D phase 가 도착 Then S 가 `Completion` kind 로 attention 발동.
- Given 위와 동일 상황이지만 명령이 실패로 종료(exit code != 0) Then 동일하게 S 가
      attention 발동(성공/실패 무관, exit code 로 필터링하지 않음).
- Given 위 두 케이스 모두 Then exit code 정보가 `command_index` memory 기록과
      `HookEvent::CommandCompleted` 훅 payload 양쪽에서 유실되지 않는다(하나의 이벤트가
      attention/memory/hook 세 소비처로 fan-out).
- Given surface S 에서 OSC 133 명령이 완료 Then attention 은 발동하지만 알림 패널에는
      아이템이 생기지 않는다(`AttentionStore` 와 `NotificationStore` 가 별개 저장소).
- Given 원격 워크스페이스를 attach 로 mirror 중 When 서버에서 그 워크스페이스의 surface 에
      `NeedsInput` 이 raise Then 1 tick(≤1s) 안에 미러의 `AttentionStore` 에 반영되어 사이드바
      needs-input 배지가 1 증가.
- Given 위와 같이 반영된 상태 When 서버에서 해제 Then 미러에서도 사라진다(`kind: null` push).
- Given mirror 워크스페이스의 surface M When 서버가 흘려준 출력에 OSC 133 D 가 섞여
      도착 Then M 에는 attention 이 발동하지 않는다(로컬 발동 억제) — 같은 사건이 mirror
      아닌 surface 에서는 그대로 발동한다. (단위 테스트로 고정됨:
      `osc133_command_completed_raises_attention_only_off_mirror`)
- Given mirror 워크스페이스의 surface M When M 에서 Bell / OSC 9·777 알림이 도착 Then
      알림 패널 아이템·토스트는 그대로 생기되 attention 레코드만 생기지 않는다. (단위
      테스트로 고정됨: `mirror_surface_notification_item_survives_the_attention_gate`)
- Given mirror 워크스페이스의 surface M When 미러 인스턴스에서 `tasty surface completion
      --surface M` 을 실행 Then attention 이 발동하지 않고 서버로도 forward 되지 않는다
      (ADR-0098). 현재 IPC 응답은 `ok: true` 이며 억제는 `tracing::trace!` 로만 관측된다.
- Given mirror 워크스페이스의 surface M When 서버가 `Attention` 프레임을 push Then 위
      억제 게이트에 막히지 않고 그대로 반영된다(적용은 별도 진입점). (단위 테스트로 고정됨:
      `server_push_apply_is_not_blocked_by_the_mirror_gate`)
- Given 서버 push 로 M 에 attention 이 표시된 상태 When 미러 사용자가 M 을 포커스
      Then 해제 edge 가 forward 큐에 1 건 쌓인다. (단위 테스트로 고정됨:
      `mirror_clear_queues_exactly_one_forward_edge` — 큐까지만. 큐 → 소켓 전송 구간은
      고정되지 않는다)
- Given holder 연결이 해제 프레임을 보냄 Then 서버가 holder 검증 후 자기 레코드를
      지운다. (loopback e2e 로 고정됨: `mirror_clear_frame_drops_the_server_attention_record`
      — raw `TcpStream` 이 프레임을 직접 보내는 **서버 절반**의 검증이다)
- Given 위 해제가 끝난 상태 When 미러가 포커스를 유지한 채 여러 프레임이 렌더된다
      Then 추가 해제 프레임이 나가지 않는다. (단위 테스트 `mirror_clear_queues_exactly_one_forward_edge`
      + loopback e2e `repeated_clear_frames_do_not_respam_the_stream` 로 고정됨)
- Given mirror 워크스페이스의 surface M 발 미러 로컬 알림 When 미러 알림 패널에서
      읽음 처리 Then 포커스 경로와 동일하게 해제 프레임이 서버로 간다. (단위 테스트로
      고정됨: `mirror_notification_read_queues_the_clear_forward`)
- Given 하드 점유(attach)된 surface S 에 attention 이 발동된 상태 When 서버 GUI 에서
      S 가 실 렌더 포커스를 얻음 Then attention 이 유지된다 — 홀더만 해제할 수 있다.
      (단위 테스트 `hard_occupied_surface_survives_the_local_focus_clear`
      + loopback e2e `hard_occupied_attention_survives_the_servers_local_focus` 로 고정됨 —
      후자는 GUI 서버의 활성 워크스페이스를 점유 워크스페이스로 전환해 `gpu.rs` 경로를
      실제로 태우고 `kind: null` push 의 부재로 관측한다)
- Given 위 상태 When 서버 알림 패널에서 그 surface 발 알림을 읽음 처리(개별/모두)
      Then attention 은 유지되고 알림의 `read` 플래그는 그대로 세워진다. (단위 테스트로
      고정됨: `marking_a_notification_read_keeps_attention_while_hard_occupied` ·
      `mark_all_read_skips_hard_occupied_surfaces_only`)
- Given 위 상태 When 점유가 풀림(detach / force-detach / 연결 끊김) Then 서버 로컬
      포커스가 다시 해제 주체가 되어 stale 레코드를 지운다. (단위 테스트로 고정됨:
      `local_clear_is_disallowed_exactly_while_hard_occupied`)
- Given soft 점유(child-terminal)된 surface Then 이 게이트는 걸리지 않는다 — 로컬
      해제가 그대로 동작한다. (단위 테스트로 고정됨: `soft_occupancy_does_not_gate_the_local_clear`)
- Given 하드 점유된 surface 에 대해 holder 가 해제 프레임을 보냄 Then 게이트와 무관하게
      적용된다 — 게이트가 primitive 안에 있으면 해제 주체가 0 이 된다. (단위 테스트로
      고정됨: `the_holders_clear_is_not_blocked_by_the_gate`)
- Given 서버 attention 값이 바뀌지 않는 tick Then 프레임이 나가지 않는다(스팸 없음).
- Given mirror surface 가 사라짐(세션 정리 / 구조 delta 제거) Then 그 로컬 id 의 attention
      레코드도 함께 정리된다 — 단, kind 변환(terminal → 다른 kind) 경로는 정리하지 않는다
      (surface 가 계속 존재하므로 지우면 서버와 divergence).

## 구현

- 상태·헬퍼: `src/core/state/attention.rs` (`AttentionStore`, `AttentionKind`, `AttentionLevel`,
  `effects_of`, `raise_attention`/`clear_attention`/`attention_kind`/
  `attention_count_of_kind`/`attention_dominant_kind`).
- 하드 점유 해제 게이트: `local_attention_clear_allowed`(술어) + `clear_attention_local`
  (로컬 축 진입점, `src/core/state/attention.rs`). 로컬 해제 호출부 셋이 이 진입점을
  지난다 — 실-포커스(`src/gfx/gpu.rs`) · `mark_notification_read` ·
  `mark_all_notifications_read`. primitive `clear_attention` 은 게이트 없이 남아
  홀더 경로(`apply_attached_attention_clear`)가 직접 부른다.
- completion/needs-input producer: intent `SurfaceCompletion { surface_id, kind }` → event
  `SurfaceCompletionRequested { surface_id, kind }` (`src/core/{intent.rs,mod.rs}`) →
  `cascade_surface_completion(surface_id, kind)` (`src/app/dispatch_domain.rs`).
  `src/adapters/ipc/handler/surface/completion.rs::handle_completion` 가 IPC 파라미터의
  `kind` 문자열(`"needs_input"` 외 전부 `Completion`)을 파싱하고, 대상이 이 engine 소속이면
  `raise_attention` 을 직접 호출한다(아래 IPC 해제 항목과 같은 이유 — headless 에는 cascade
  가 없다). 응답 계약은 그대로다: 존재하지 않는 surface 도 `ok` 로 응답하되 레코드는 만들지
  않는다.
- IPC 조회·해제: `src/adapters/ipc/handler/surface/attention.rs` 의 `handle_attention_get` /
  `handle_attention_clear`. 해제는 intent `SurfaceAttentionClear { surface_id, kind }` → event
  `SurfaceAttentionClearRequested` → `cascade_surface_attention_clear`
  (`src/app/dispatch_domain.rs`)로 이어지지만, **상태 변경 자체는 핸들러가 라우팅된 owner
  engine 에 직접 적용**한다 — Intent 큐 drain(`App::dispatch_pending_intents`)과
  `dispatch_domain` 모듈이 gui 빌드 전용이라 headless 에서는 cascade 가 돌지 않기 때문이다.
  cascade 는 gui 소비처 redraw 를 얹고, IPC 를 타지 않는 도메인 내부 호출자를 위해 자기
  완결적으로 남는다. mirror 판정은 `CoreState::is_mirror_surface`, 점유 판정은
  `OccupancyRegistry::is_hard_occupied`. 근거는 ADR-0107.
  CLI 는 `crates/tasty-cli/src/commands/surface.rs` 의 `SurfaceAttentionCommands`,
  매핑은 `crates/tasty-cli/src/request.rs`.
- Claude hook producer: `apply_hook` (`crates/tasty-plugin-claude/src/hook.rs`) → `HostCall::
  SurfaceCompletion { surface_id, kind }` → `deliver()` 가 `surface.completion` IPC 호출로
  매핑(`{ "surface_id", "kind" }`) → 위 producer 경로 그대로 재사용.
- OSC 133 명령 완료 producer: `Core::handle_prompt_boundary` (`src/core/impl_pty.rs`, D phase)
  → `CoreEvent::TerminalCommandCompleted { surface_id, exit_code }` (`src/core/intent.rs`)
  → `App::cascade_terminal_command_completed` (`src/app/dispatch_domain.rs`)가
  `engine.raise_attention(surface_id, AttentionKind::Completion)` 직접 호출(자동 경로) +
  `HookEvent::CommandCompleted` 훅 발화(커스터마이즈 경로) 동시 처리.
- 원격 attach 전파(server→client): wire 는 `StreamControl::Attention` +
  `AttentionKindWire`(`crates/tasty-ipc/src/stream.rs`), host↔wire 변환은
  `AttentionKind::to_wire`/`from_wire`(`src/core/state/attention.rs`). 서버측 diff 는
  `CoreState::attention_forwards`(캐시 `CoreState.last_forwarded_attention`) → push 는
  `CoreState::forward_attention`(`src/core/attach_runtime.rs`), 호출 캐던스는 1Hz
  `Tick::Busy` 3 지점(`src/app/busy.rs` 의 main window · parked engine, `src/boot.rs` 의
  headless). client 측은 `MirrorEvent::Attention` 파싱 → `apply_one_mirror_event` 가
  `CoreState::set_mirror_surface_attention` 호출, teardown 은
  `CoreState::forget_mirror_surface_attention`(`src/app/attach_client.rs`).
- 알림 읽음 clear: `CoreState::mark_notification_read`/`mark_all_notifications_read`
  (`src/core/state/attention.rs`) — `NotificationStore::has_unread_for_surface`
  (`src/store/notification.rs`)로 잔여 안읽음을 확인 후 clear. 호출부는
  `cascade_notification_read`/`cascade_all_notifications_read`(`src/app/dispatch_domain.rs`).
