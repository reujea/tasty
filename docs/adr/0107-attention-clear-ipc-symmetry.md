# ADR-0107: attention 해제를 IPC/CLI 로 노출하고, 상태 변경은 IPC 핸들러가 직접 적용한다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: attention, surface-highlight, ipc, cli, headless, cascade, intent, attach, mirror, api-symmetry
- **Group**: attention-notification

## Context

attention(주의 환기, [ADR-0039](0039-surface-highlight-shared-primitive.md))은 **발동만 IPC/CLI 로
가능하고 해제는 불가능**했다. 발동은 `surface.completion`(IPC) / `tasty surface completion`(CLI)
이지만, 해제 producer 두 개는 전부 GUI 로컬 사건이다 — 실 렌더 시점 포커스(`src/gfx/gpu.rs`)와
알림 패널 읽음 처리. 불가침 원칙 2([identity](../identity.md))는 에이전트 기능이 IPC + CLI 양면으로
동작할 것을 요구하는데, 켤 수만 있고 끌 수 없는 표면은 그 원칙의 구멍이다.

headless 인스턴스(`--headless`, `src/boot.rs`)는 렌더도 알림 패널도 없어 해제 producer 가 0 개다.
그런데 조사 과정에서 **headless 에는 발동 producer 도 0 개**라는 사실이 함께 드러났다: Intent 큐를
drain 하는 `App::dispatch_pending_intents` 가 `#[cfg(feature = "gui")]` 이고
(`src/app.rs` 의 `mod dispatch`), headless IPC 펌프(`src/boot/headless_dispatch.rs`)는 그 큐를 읽지
않는다. headless 빌드가 `dispatch_domain` 자리에 대신 컴파일하는 것은 전부 no-op 인
`dispatch_domain_stubs.rs` 다. 즉 핸들러가 intent 를 enqueue 해도 그것을 꺼내 적용할 주체가 없어
`surface.completion` 은 headless 에서 아무 일도 하지 않았다 — headless 는 "해제만 없는" 상태가
아니라 attention 축 전체가 죽어 있었다.

또 두 가지 소유권 제약이 있다. ① 원격 attach 로 **하드 점유**된 surface 는 로컬 사용자·에이전트가
readonly 이고 주체는 홀더다([ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md)) —
"확인했다" 는 판정도 홀더의 것이다. 서버 값이 바뀌면 그 변화분은 홀더 미러로 push 되므로
([attach-behavior](../dev-guide/attach-behavior.md)), 점유 중에 로컬 IPC 가 서버 값을 지우면
그 해제가 미러까지 전파돼 **홀더가 배지를 보기도 전에 신호가 사라진다.**
[ADR-0109](0109-hard-occupancy-attention-clear-holder-only.md) 가 같은 판단을 로컬 GUI 축
(실-포커스 · 알림 읽음)에 대해 이미 내렸고, 에이전트 요청 축은 그때 열려 있었다.
② 반대편인 **mirror surface** 는 로컬 발동이 이미 억제돼 있고
([ADR-0098](0098-mirror-local-attention-raise-suppressed.md)), 해제 edge 를 소유 인스턴스로
전달할 자격은 "그 화면을 실제로 본 주체"(미러 사용자의 실-포커스 · 미러 로컬 알림 읽음)에게만
있다([ADR-0104](0104-mirror-attention-clear-forwarded-to-owner.md)). ADR-0104 는 그 결정의
집행("에이전트가 IPC 로 요청하는 해제는 mirror surface 를 대상으로 하면 거절")을 **이 IPC 를
도입하는 트랙**, 즉 본 ADR 에 맡겨 두었다.

## Decision

`surface.attention.clear`(해제)와 `surface.attention.get`(조회)를 IPC + CLI 양면으로 추가하고,
**상태 변경은 IPC 핸들러가 라우팅된 owner engine 에 직접 적용**한다. cascade(`dispatch_domain.rs`)는
gui 에서 소비처 redraw 를 얹는 자기 완결적 도메인 경로로 함께 두되, 상태의 유일한 적용 주체로
삼지 않는다 — 그러면 headless 에서 다시 아무 일도 일어나지 않기 때문이다. 같은 이유로
`surface.completion` 의 발동도 핸들러에서 직접 적용하도록 맞춘다(응답 계약은 불변 — 존재하지 않는
surface 는 지금처럼 `ok` 로 응답하되 유령 레코드를 만들지 않는다).

세부 결정 넷:

1. **이름은 3 단 보조 도메인** `surface.attention.{get,clear}` — `surface.meta.*` 와 같은 형태다
   ([api-conventions](../dev-guide/api-conventions.md) "보조 도메인은 3단"). namespace 는 단수
   `surface` 를 유지하고, attention 은 앞으로 항목이 더 붙는 축(미러 해제 forward, 점유 게이트)이라
   `<verb>_<modifier>` 평면 이름보다 한 단계 접는 쪽이 확장 여지가 크다.
2. **`kind` 는 선택적 필터** — 주면 현재 기록된 kind 가 그 값일 때만 지운다. 해제를 만든 시점과
   적용 시점 사이에 다른 producer 가 더 급한 kind(`NeedsInput`)로 재발동했을 수 있고, 무조건
   해제는 "지금 답하지 않으면 멈춘다" 는 신호를 조용히 지운다. 알 수 없는 kind 문자열은 거절한다 —
   `surface.completion` 은 하위 호환 때문에 미상 값을 `completion` 으로 떨어뜨리지만, 필터에서
   같은 관용은 "지정한 kind 만 지운다" 는 계약 자체를 깨뜨린다.
3. **하드 점유 중에는 해제를 거절**한다(명시적 `invalid_params`, 조용한 no-op 아님).
   [ADR-0109](0109-hard-occupancy-attention-clear-holder-only.md) 가 같은 정책을 로컬 GUI 축
   (실-포커스 · 알림 읽음)에 대해 `CoreState::clear_attention_local` 게이트로 집행한다 — 본 결정은
   그 정책을 **에이전트 요청 축**에서 집행하는 짝이다. 다만 그 래퍼를 지나지 않고 API 경계에서
   먼저 거절한 뒤 primitive `clear_attention` 을 부른다: 래퍼를 태우면 점유 중 호출이 조용한
   no-op 으로 끝나 "지웠는지" 를 응답으로 구분할 수 없게 되는데, 에이전트 표면에서는 그 침묵이
   곧 오인이다. ADR-0109 의 운영 조건("새 해제 producer 는 `clear_attention_local` 을 지나야
   한다")에 대한 **명시적 예외**이며, 게이트를 우회하는 것이 아니라 같은 술어
   (`is_hard_occupied`)를 더 앞에서 더 크게 집행한다.
4. **mirror surface 에 대한 해제도 거절**한다(같은 형식 — 선례는
   [ADR-0086](0086-reject-terminal-spawn-into-mirror-workspace.md)). ADR-0098 이 발동 축에서 내린
   판단의 대칭이고, ADR-0104 가 이 트랙에 맡긴 집행이다. 미러 사용자가 그 surface 를 **실제로 보고**
   확인한 경우는 여전히 해제 경로 1·2 로 잡혀 `ClientAttentionClear` 로 소유 인스턴스에 전달되므로,
   막히는 것은 "보지 않은 에이전트가 대신 지우는 길" 하나뿐이다.

3·4 어느 쪽도 **조회**(`surface.attention.get`)는 막지 않는다 — 읽기 전용이라 소유권 문제가 없고,
막으면 미러/점유 상황에서 배지 상태를 확인할 수단이 사라진다.

## Consequences

- **얻은 것**: attention 축이 IPC/CLI 에서 대칭이 됐다(발동·조회·해제). headless 인스턴스에서
  attention 이 실제로 동작한다 — 발동·조회·해제 전부. 조회 표면이 생겨 attach 미러 전파처럼
  "해제가 됐는지" 를 간접 관측하던 검증이 직접 assert 로 바뀐다.
- **잃은 것**: 상태 적용 지점이 핸들러와 cascade 두 곳에 존재한다(핸들러가 먼저 적용하므로
  cascade 의 재적용은 no-op). cascade 의 redraw 요청을 "지울 게 남아 있는지" 로 게이트할 수 없게
  돼, 필터 불일치로 아무것도 안 지운 호출도 프레임 한 번을 요청한다. 에이전트는 mirror 인스턴스
  쪽에서 원격 surface 의 attention 을 지울 수 없다 — 소유 인스턴스에 원격 id 로 요청해야 한다.
- **운영 비용 / 유지 부담**: `METHOD_TABLE` 등재 2 건, `tests/cli_naming_count_drift.rs` 의
  `surface` 카운트 스냅샷 갱신. 점유·mirror 게이트는 `attach.*` 정책(ADR-0040 · ADR-0098 ·
  ADR-0104 · ADR-0109)과 함께 움직여야 한다 — 특히 `is_hard_occupied` 술어가 바뀌면 로컬 축
  (`clear_attention_local`)과 이 핸들러 두 곳을 함께 봐야 한다. 게이트를 `CoreState::clear_attention` 안이 아니라 **IPC 핸들러
  안**에 둔 것은 ADR-0104 가 후속 작업에 건 제약("점유 게이트를 진입점에 두면 서버측 적용이 막혀
  해제 주체가 다시 0 이 된다")을 지키기 위해서다 — ADR-0109 는 같은 제약을 로컬 축 전용 래퍼
  `clear_attention_local` 로 풀었고, 본 ADR 은 에이전트 요청 축을 API 경계에서 막는다.

## Alternatives Considered

- **A. cascade 만으로 처리(핸들러는 enqueue 만, `surface.completion` 형태 그대로)** — 형제 구현과
  형태는 가장 잘 맞지만 headless 에서는 그 intent 를 꺼내 적용할 주체가 없어(cascade 자리에 no-op
  stub 이 들어간다) 이 ADR 이 풀려는 문제를 그대로 남긴다. 이 선택지가 곧 현재의 결함이다.
- **B. headless dispatch(`src/boot/headless_dispatch.rs`)가 Intent 큐를 drain 하게 만든다** — 원인을
  한 곳에서 없애는 가장 근본적인 수정이지만, 그 순간 모든 도메인 cascade 가 headless 에서 처음으로
  돌기 시작한다. attention 하나를 고치려고 검증되지 않은 전 도메인 경로를 headless 에 켜는 것은
  변경 폭이 위험 대비 과하다. 별도 트랙에서 도메인별로 검증하며 여는 편이 맞다.
- **C. 평면 이름 `surface.clear_attention`** — verb 화이트리스트의 `clear` + modifier 로 규약에는
  맞지만, 조회 짝(`surface.attention.get`)을 verb-first 로 만들면 `get_attention` 같은 어색한 이름이
  되고 attention 축이 나중에 더 붙을 때마다 평면 namespace 가 넓어진다.
- **D. 하드 점유 중에도 해제 허용** — 에이전트 입장에선 단순하지만, 그 해제가 diff push 를 타고
  홀더 미러까지 전파돼 홀더가 배지를 보기 전에 신호가 사라진다. 로컬 GUI 축에서 같은 이유로 이미
  막은 결정(ADR-0109)을 에이전트 요청 축에서만 여는 셈이라, 해제 주체를 홀더 하나로 확정한
  ADR-0104+0109 의 결론이 무너진다.
- **E. `kind` 필터 없이 무조건 해제** — API 는 단순해지지만 `Completion` 해제 요청이 그 사이 올라온
  `NeedsInput` 을 지우는 오해제를 막을 방법이 호출자에게 없다.
- **F. mirror 에서의 해제를 허용하고 `clear_attention` 의 forward 큐에 실어 서버로 보낸다** — 경로는
  이미 있으므로(ADR-0104) 기술적으로 가능하다. 채택하지 않은 이유는 그 forward 의 자격 조건이
  "미러 사용자가 그 화면을 실제로 봤다" 이기 때문이다. 미러 인스턴스의 에이전트는 원격 surface 를
  소유하지도 보고 있지도 않으므로, 그것을 forward 시키면 ADR-0104 가 규칙 자체는 바꾸지 않겠다고
  한 전제를 IPC 로 우회하게 된다.

## Reconsideration Triggers

- headless dispatch 가 Intent 큐를 drain 하게 되면(대안 B) 핸들러의 직접 적용은 중복이 되므로
  cascade 단일 적용으로 되돌린다.
- 미러 인스턴스의 에이전트에게도 원격 surface 를 확인할 자격을 주는 모델이 생긴다(예: 미러 쪽
  에이전트가 그 워크스페이스의 정당한 주체로 승격) — 그때는 4 번 거절 대신 ADR-0104 의 forward
  경로에 얹는 대안 F 를 다시 본다.
- 하드 점유 중 해제 권한을 홀더에게 여는 별도 경로가 IPC 로 들어오면, 그 경로가 이 메서드를
  재사용할지 별도 채널을 쓸지에 따라 위 3 번 게이트의 우회 지점을 다시 정한다.
- **핸들러 적용과 cascade 재적용 사이의 1 프레임 창**이 문제가 되면 이중 적용을 접는다. 핸들러는
  `process_ipc()` 시점에 지우고 cascade 는 `about_to_wait` 끝에서 한 번 더 도는데, 그 사이에 서버
  push(`set_mirror_surface_attention`)가 새 값을 넣으면 kind 필터가 없는 호출이 그것을 지울 수 있다.
  지금은 mirror 거절(4) 로 그 조합 자체가 IPC 경로에서 닫혀 있지만, 4 를 열면 이 창이 되살아난다.
- headless 에서 `AppState.pending_intents` 는 아무도 drain 하지 않아 프로세스 수명 동안 쌓인다
  (이 메서드만의 문제가 아니라 같은 패턴을 쓰는 IPC 핸들러 전부에 해당하는 기존 성질이다).
  누적이 실측으로 문제가 되거나 대안 B 가 도입되면 함께 정리한다.
- attention kind 가 2 종을 넘어서면(`error`/`approval` rank 예약) 필터 의미를 "정확히 일치" 가
  아니라 "이 kind 이하" 같은 순위 기반으로 다시 볼 여지가 생긴다.
- attention 해제와 알림 읽음 처리를 한 번에 하려는 소비처가 생기면, 지금의 저장소 분리
  (`AttentionStore` ↔ `NotificationStore`)를 유지한 채 합성 메서드를 둘지 판단한다.

## References

- [ADR-0039](0039-surface-highlight-shared-primitive.md) — attention 이 producer 중립 공유 상태라는 근거
- [ADR-0062](0062-attention-store-kind-aware-primitive.md) — kind 확장
- [ADR-0098](0098-mirror-local-attention-raise-suppressed.md) — 발동 축의 mirror 억제(본 ADR 4 번의 대칭)
- [ADR-0104](0104-mirror-attention-clear-forwarded-to-owner.md) — 미러 해제 edge 전달. 본 ADR 4 번의 거절을 이 트랙에 위임한 결정
- [ADR-0086](0086-reject-terminal-spawn-into-mirror-workspace.md) — mirror 대상 IPC 거절의 형식 선례
- [ADR-0109](0109-hard-occupancy-attention-clear-holder-only.md) — 하드 점유 중 해제는 홀더만.
  본 ADR 결정 3 은 그 정책을 에이전트 요청 축에서 집행하는 짝이다
- [features/surface-highlight](../features/surface-highlight/index.md) — 인터페이스·동작 현재 상태
- [dev-guide/api-conventions](../dev-guide/api-conventions.md) — 명명 규칙·권한 표 등재
- [dev-guide/attach-behavior](../dev-guide/attach-behavior.md) — attention 전파와 점유 모델
