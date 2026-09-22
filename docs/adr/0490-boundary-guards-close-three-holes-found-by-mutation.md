# ADR-0490: 경계 가드의 세 빈자리를 판정기를 넓혀 닫는다 — 변이로 찾은 것

- **Status**: Accepted
- **Date**: 2026-09-22
- **Tags**: guard, domain, layering, mutation, gui, focus, automation, webhook, hook-handler, adr-0440
- **Group**: architecture

## Context

[ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md) 은 도메인
경계를 크레이트가 아니라 모듈 경계와 가드로 세웠다. 리팩토링 마스터플랜의 검증 단계가 그 경계와
인접 경계를 **변이**로 쟀다 — 위반을 한 줄 넣고 무엇이 빨개지는지 본다. 대부분은 잡혔고, 셋은
**아무것도 안 빨개졌다.** 셋 다 지금 트리의 위반은 0 이다. 결함은 회귀를 막는 채널의 빈자리다.

1. **도메인의 기존 gui 게이트 안에 GUI 크레이트를 더 들인다.** `src/core/cascade_window.rs` 의
   `#[cfg(feature = "gui")]` import 에 `egui::Context` 를 끼워 넣었다. `domain_does_not_reach_up`
   은 `crate::`·`super::` 경로만 상위 모듈 표와 대조하고 외부 크레이트 이름은 안 본다. gui 게이트
   수 래칫은 **새 게이트**만 센다 — 게이트 수는 그대로다. gui 컴파일은 egui 가 있어 통과하고,
   headless 컴파일은 그 줄을 안 본다. 무조건 `use egui::…` 는 headless 컴파일이, 새 게이트는
   래칫이 잡으므로 빈자리는 이 한 형태다.
2. **포트 구현 파일이 에이전트 대면 활성 읽기 가드의 스캔 밖이다.** 구조 cascade 가 활성
   워크스페이스를 되만드는 대입은 `CascadeWindow::set_active_workspace` 로 역전됐고, 그 구현은
   `src/state/cascade_window.rs` 에 있다. `agent_facing_reads_of_active_state_are_classified` 의
   스캔 루트에 `src/state` 가 없다. 그 파일에 `self.active_workspace` 읽기를 넣어도 초록이었다.
   cascade 쪽 자리가 지금 세어지는 것은 메서드 이름 `set_active_workspace` 가 바늘을 부분문자열로
   담아서다 — 이름이 바늘을 안 담는 포트 메서드면 구현만 활성 포인터를 읽고 가드는 못 본다.
3. **자동화 실행부(webhook · hook handler)의 inbound adapter 역참조를 보는 가드가 없다.** 공용
   경계 작업은 두 실행부가 IPC 핸들러 트리의 파일 배치를 모르고 공용 계약
   (`tasty_ipc::host_call::HostIpcInjector`)으로 호출을 주입하게 했다. 도메인 가드의 좌변은
   `src/core` · `src/ports` 라 이 두 디렉토리를 안 본다. `src/webhook/mod.rs` 에
   `use crate::adapters::production::tcp_ipc_server` 를 넣어도 doc-guards 전량과 lib 소스 가드가
   초록이었고 컴파일도 됐다.

## Decision

**세 자리 모두 새 채널이 아니라 기존 판정기를 넓혀 닫는다.** 크레이트 루트 경로를 읽는
판정기(마스킹 · 중괄호 import 펴기 · 줄을 넘는 경로 · `super::` 사슬 · 앞마디 일치)는
`tasty_doc_guards::crate_paths` 로 올려 두 가드가 함께 쓴다.

1. **도메인 GUI 크레이트 — 목록 래칫.** `domain_does_not_reach_up` 이 도메인 출하 코드에서
   외부 크레이트 경로를 읽는다. 대상 크레이트는 워크스페이스 `Cargo.toml` 의 `gui` feature 가
   `dep:` 로 켜는 optional 의존 전부다(실측 29 개) — 손으로 적지 않고 매니페스트에서 읽는다.
   여기에 `windows` 의 창·그리기 하위 경로(`windows::Win32::UI` · `windows::Win32::Graphics`)와
   `Foundation` 의 창 핸들 항목(`HWND` · `HINSTANCE` · `LPARAM` · `WPARAM`)을 더한다. 경로가 그
   항목들의 **진접두사에서 끝나는** import(`use windows::Win32 as w32;` · `use windows::Win32::*;` ·
   `use windows::Win32::Foundation::{self, …}` · `use windows as w;`)도 잡는다 — 들인 모듈 이름으로
   `w32::UI::…` 를 부르면 경로가 창 갈래에 닿기 전에 끊긴다(처음에는 이 형태가 초록이었다 —
   Gate 4 변이 winalias · winglob). 진접두사에서 끝나는 경로는 모듈을 들이는 import 뿐이라 식
   안의 경로(`windows::Win32::System::…::X`)는 걸리지 않는다. gui 게이트 뒤의 줄도 **빼지 않고** 읽는다(`#[cfg(test)]` 만 뺀다). 기존 자리는
   **(파일, 경로) 목록**으로 양방향 고정한다 — 늘면 새 자리가, 줄면 남은 항목이 빨갛다. 오늘
   목록은 비어 있다. 수가 아니라 목록인 이유는 1 번 구멍 그 자체다: 수는 기존 자리 안의 추가를
   못 본다.
2. **포트 구현 파일을 파일 단위로 에이전트 대면에 올린다.** `src/state/cascade_window.rs`
   (`CascadeWindow`)와 `src/file/identify_worker.rs`(`IdentifySpawner`) — 도메인이 선언하고 창
   쪽이 구현하는 포트는 오늘 이 둘이다. 스캔 루트에 `src/state` · `src/file` 을 더하되 분류는
   파일 단위로 한다. 기존 두 출현(`set_active_workspace` 구현 이름과 그 대입)은 `Recovery` 로
   명부에 올린다. adapters 가 선언하고 창 쪽이 구현하는 포트의 구현도 같다 — `src/state/ipc_window.rs`
   (`IpcWindow`, [ADR-0471](0471-ipc-engine-handlers-reach-the-window-through-a-port.md))를 같은 방식으로 올리고,
   그 두 출현(`active_workspace_index` 구현 이름과 그 읽기)은 핸들러 호출 자리의 분류를 물려받는다.
   파일 단위 항목이 옮겨지면 아무것도 안 걸러 조용히 통과하므로, 모든 항목이
   트리에 실재하는지 보는 시험을 더한다.
3. **자동화 실행부 가드를 새 통합 타깃으로 둔다** —
   `automation_runners_do_not_reach_inbound_adapters`. 좌변은 `src/webhook/**` ·
   `src/hook_handler/**` 의 출하 코드, 금지 표는 `adapters::ipc` · `adapters::cli` ·
   `adapters::production::tcp_ipc_server` · `hub` · `app` 과 그 lib 루트 별칭(`ipc` · `cli` ·
   `App` · `AppEvent` · `debug_info`)이다. 별칭은 `src/lib.rs` 의 `pub(crate) use` 전부를 두 표
   (`UPPER` · `INBOUND`)와 대조해 빠짐이 없게 적었다 — 처음에 `debug_info` 가 빠져 초록이었다
   (Gate 4 변이로 확인). 기대값 0, 면제 명부 없음.

## Consequences

- **얻은 것**: 검증 단계의 변이 셋(도메인 gui 게이트 안 `egui::Context` · 포트 구현의 활성
  읽기 · webhook 의 IPC 서버 import)이 모두 빨갛다. 표기 변형(`::egui::` 절대 경로 · `use egui
  as e;` · 여러 줄 중괄호 · `windows::Win32::UI::…`)과 hook handler 쪽 변이, 별칭 경로도 빨갛고,
  허용 경로(공용 계약 `tasty_ipc::host_call` · `std` · `windows::Win32::Foundation` · test 모듈 안
  import · 포트 구현이 아닌 `src/state` 파일)는 초록이다.
- **얻은 것**: GUI 크레이트 목록이 매니페스트를 따라간다. `gui` 에 크레이트가 더해지면 판정도
  같이 넓어지고, 목록 읽기가 깨지면 앵커 일곱(egui · winit · wgpu · webkit2gtk · gtk ·
  objc2-app-kit · webview2-com)이 빨갛다.
- **잃은 것 — 전이 의존은 여전히 안 본다.** 도메인·실행부가 부르는 형제 모듈이 다시 GUI
  크레이트나 핸들러를 부르는 경로는 세 판정 모두 밖이다(ADR-0440 의 같은 한계).
- **잃은 것 — GUI 갈래를 가진 워크스페이스 크레이트.** `gui` 가 `tasty-platform/gui` ·
  `tasty-icons/egui` 처럼 **다른 크레이트의 feature** 를 켜는 것은 목록에 안 든다. 그 크레이트는
  headless 에도 링크되어 `tasty_platform::…` 이 GUI 갈래를 부르는지 이름으로 안 갈린다.
- **잃은 것 — GUI 타입이 아닌 크레이트도 목록에 있다.** `png` · `image` · `pulldown-cmark` ·
  `trash` 등도 `gui` feature 뒤라 목록에 든다. 뺄 이유가 없다고 판단했다 — headless 그래프에
  없으니 도메인이 부르면 gui 게이트 뒤에 숨겨야만 컴파일되고, 그것이 곧 "도메인이 GUI 구성을
  안다" 이다.
- **잃은 것 — (파일, 경로) 목록은 같은 파일의 같은 경로 재등장을 못 본다.** 목록에 오른 쌍은
  그 파일에서 몇 번 나와도 초록이라, 첫 항목이 오르는 날부터 그 쌍에 한해 "기존 자리 안 추가"
  구멍이 다시 생긴다. 오늘 목록이 비어 있어 해당이 없다 — 목록에 첫 항목이 오를 때
  (파일, 경로, 개수)로 바꾼다.
- **잃은 것 — `windows` 창 경로의 조상 모듈을 들이는 것 자체가 막힌다.** `use windows::Win32::Foundation;`
  로 `Foundation::HANDLE` 을 쓰는 것처럼 창 아닌 항목만 쓰려는 import 도 빨갛다 — 들인 모듈로
  `Foundation::HWND` 도 닿기 때문이다. 창 아닌 항목은 항목 경로로 들이면 된다(`use
  windows::Win32::Foundation::HANDLE;`). 오늘 도메인 출하 적중 0.
- **잃은 것 — 지역 모듈과 같은 이름.** `use` 없이 `image::x` 로 부르는 지역 모듈은 크레이트와
  텍스트로 안 갈린다. 오늘 도메인에 그 이름의 모듈은 없다(`git grep` 적중 0).
- **잃은 것 — 포트 구현 목록은 손으로 는다.** 도메인(또는 adapters)이 새 포트를 선언하고 창 쪽이
  새 파일에서 구현하면 `AGENT_FACING` 에 한 줄을 더해야 한다. 그것을 알려 주는 채널은 없다(아래 재검토 조건).
- **운영 비용**: 도메인 gui 게이트 고정값(`GUI_GATES_IN_DOMAIN`)과 GUI 크레이트 목록은 서로
  다른 물음이라 둘 다 유지한다. 앞은 "게이트가 늘었나", 뒤는 "게이트 뒤에 GUI 크레이트가
  들어왔나" 다.

## Alternatives Considered

- **GUI 크레이트를 수로 고정한다(게이트 수 래칫과 같은 모양)**: 1 번 구멍을 그대로 남긴다 —
  기존 자리 안에 한 항목을 더하면 수가 1 늘지만, 같은 커밋이 다른 자리를 하나 지우면 수는
  그대로다. 목록은 그 교환도 잡는다.
- **GUI 크레이트 목록을 가드에 손으로 적는다**: `gui` feature 에 크레이트가 더해질 때 목록이
  안 따라간다. 매니페스트가 이미 그 사실을 적고 있어 두 번 쓸 이유가 없다.
- **`windows` 크레이트 전체를 막는다**: `windows` 는 `gui` 뒤가 아니라 Windows 타깃에서 조건
  없이 링크되고, 프로세스·콘솔·파일 시스템도 이것으로 부른다. 도메인이 그 갈래를 쓰는 것은
  GUI 를 아는 것이 아니다. 창·그리기 하위 경로와 **`Foundation` 의 창 핸들**(`HWND` · `HINSTANCE` ·
  창 프로시저 인자 `LPARAM` · `WPARAM` — 항목 단위)만 막는다. `Foundation` 은 `HANDLE` 같은 창
  아닌 타입도 담아 통째로 막지 않는다. 처음에는 `UI` · `Graphics` 둘만 적어 `HWND` 가 초록이었다
  (Gate 4 변이로 확인).
- **`src/state` 전체를 에이전트 대면에 올린다**: 사용자 입력 경로(포커스 이동 · 선택 · 스크롤)가
  섞여 명부가 사람 판정 없이 부풀고, 이 가드의 물음("에이전트가 부르는 경로인가")이 흐려진다.
  파일 단위로 좁혔다.
- **자동화 실행부 금지 표에 `adapters` 전체를 올린다**: outbound adapter(`adapters::production`
  의 fs · clock · process 구현)까지 막게 된다. 그것은 "요청을 받는 쪽을 아는가" 가 아니라
  "포트 대신 구현을 직접 쓰는가" 라는 다른 물음이다. 오늘 적중이 0 이라 어느 쪽이든 초록이지만,
  물음을 섞지 않으려고 inbound 쪽만 올렸다.
- **자동화 실행부를 도메인 가드의 좌변에 더한다**: 두 좌변의 금지 표가 다르다 — 실행부는 형제
  모듈(`file` · `store`)이나 `state` 를 불러도 이 물음의 위반이 아니다. 좌변을 섞으면 표가
  "둘 중 더 좁은 쪽" 으로 맞춰진다. 판정기만 공유하고 타깃은 나눴다.

## Reconsideration Triggers

**채널이 붙는 것**

- 도메인 출하 코드가 GUI 크레이트 경로를 부른다 — `domain_does_not_reach_up` 의
  `the_domain_names_no_gui_crate_beyond_the_pinned_list` 가 파일:줄·경로로 가리킨다. 처방은
  상위 참조와 같다(GUI 쪽으로 옮기거나 포트로 역전). 옮길 수 없는 자리가 생기면 목록에 올리지
  말고 이 ADR 을 다시 연다.
- 자동화 실행부가 inbound adapter 를 부른다 — `automation_runners_do_not_reach_inbound_adapters`.
  공용 계약에 없는 것이 필요해서라면 계약을 넓힌다. 그래도 안 되면 이 ADR 을 다시 연다.
- `AGENT_FACING` 의 파일 단위 항목이 사라진다 — `every_agent_facing_entry_exists`.

**원리적으로 안 붙는 것**

- 도메인(또는 adapters)이 새 포트를 선언하고 창 쪽이 **새 파일**에서 구현한다. 그 파일이 `AGENT_FACING` 에
  없으면 활성 읽기가 스캔 밖이다. 재는 법:
  `git grep -nE '^impl [^{]*(crate::core::|CascadeWindow|IdentifySpawner|IpcWindow)[^{]* for ' -- src ':!src/core'`
  의 파일 목록(2026-09-22 실측: 위 두 파일과 `src/state/ipc_window.rs`)을 `AGENT_FACING` 과 견준다. 도메인이 선언한 trait 이
  늘었으면 그 이름을 패턴에 더한다(trait 을 `use` 로 들여와 짧은 이름으로 구현하면 첫 갈래가
  못 본다).
- 도메인이 `gui` feature 뒤의 워크스페이스 크레이트 갈래(`tasty_platform` 의 gui 모듈 등)를
  부르기 시작한다. 재는 법: `git grep -n 'tasty_platform::\|tasty_icons::' -- src/core src/ports`
  의 적중을 gui 게이트 뒤인지 본다.

## References

- [ADR-0440](0440-the-domain-boundary-is-a-module-boundary-with-a-guard-not-a-crate.md) — 도메인
  경계. 이 결정이 그 가드의 판정 범위를 넓혔고 "잃은 것" 을 갱신했다
- [ADR-0470](0470-an-ipc-handler-takes-window-state-only-when-it-reads-it.md) — 핸들러의 창 상태
  인자. 활성 읽기 가드의 명부와 같은 경로를 본다
- [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) — headless 컴파일 경계
- [포커스 정책](../design/policies/focus.md) · [아키텍처](../architecture/index.md) 의 "도메인 경계"
- 결정이 실현된 현재 위치: `crates/tasty-doc-guards/src/crate_paths.rs` ·
  `crates/tasty-doc-guards/src/cargo_manifest.rs` 의 `feature_enabled_deps` ·
  `crates/tasty-doc-guards/tests/domain_does_not_reach_up.rs` ·
  `crates/tasty-doc-guards/tests/agent_facing_reads_of_active_state_are_classified.rs` ·
  `crates/tasty-doc-guards/tests/automation_runners_do_not_reach_inbound_adapters.rs`
