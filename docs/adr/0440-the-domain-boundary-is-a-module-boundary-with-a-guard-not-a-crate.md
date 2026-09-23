# ADR-0440: 도메인 경계는 크레이트가 아니라 모듈 경계와 가드로 세운다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: headless, domain, core, layering, crate-split, ports, app-state, guard, adr-0355, adr-0346
- **Group**: architecture

## Context

headless 빌드(`--no-default-features`)는 GUI 없이 IPC/CLI 와 attach 서버를 실행하는 제품
형태다. 그 형태가 기대는 도메인 — `Core`·`CoreState` 와 그 실행(`src/core/`), 그리고
`Core` 가 주입받는 포트(`src/ports/`) — 이 GUI 조립부와 **어느 방향으로 의존하는가**가
물음이다. 도메인이 창 상태·IPC 핸들러·GUI 부품을 이름으로 부르면, headless 가 컴파일은
되더라도 "GUI 없이 도메인을 구성한다" 는 말이 구조로는 성립하지 않는다.

루트 패키지는 이미 lib·bin 타깃으로 갈려 있다(ADR-0325). 그 lib 이 밖으로 내는 것은
`boot` 하나이고 나머지는 전부 `pub(crate)` 이다 — 경계를 **잴 자리**만 있고 경계는 없다.

실측(2026-09-21, 착수 트리 `17e2a7f56`, 이 ADR 의 가드와 같은 판정기):

- 도메인 출하 코드가 상위 모듈을 부르는 자리 **24**. `AppState`(구조 실행·cascade·forward
  runner·gui 전용 resize·picker 적용) 15 · GUI intent 큐의 발화 주체 타입 3 · IPC 핸들러
  (승인·태스크 대기) 2 · `plugin_bridge` 의 surface 타입 3 · IPC 어댑터 재수출 1.
  `CoreState` 필드 하나는 gui 전용 `IdentifyWorker` 를 직접 들고 있었다.
- 도메인 출하 코드의 `feature = "gui"` **239**.
- `src/core/` 38,848 줄. 도메인이 기대는 형제 모듈(`file`·`store`·`hook_handler`·`host_api`·
  `completion_strategy`) 12,204 줄, 그중 `file::dispatch` 는 `AppState` 를 받는다(gui 전용 picker).
- core 밖에서 `crate::core::` 를 부르는 파일 212 개. 그 항목들은 `pub(crate)` 다.
- 증분 빌드 비용의 45~53% 는 link 가 차지한다(본체 크레이트 분해 분석, 2026-09-03 실측).
  크레이트를 떼어 줄어드는 몫은 typeck·codegen 이고, 그 이득은 "도메인 편집이 GUI 편집보다
  드물다" 가 성립해야 순이득이다.

## Decision

**도메인은 `src/core/` 와 `src/ports/` 다. 도메인을 별도 크레이트로 떼지 않고, 같은 크레이트
안에서 모듈 경계와 가드로 방향을 세운다.**

1. **방향**: 도메인의 출하 코드는 본체의 조립·어댑터·GUI 모듈(`app`·`adapters`·`ipc`·`cli`·
   `plugin_bridge`·`state`·`intent`·`view`·`gfx`·`boot`·`hub` 와 그 lib 루트 별칭, 별칭이 가리키는
   형제 모듈의 하위 항목 `file::dispatch`·`file::identify_worker`·`host_api::webview`·`clipboard`)을
   이름으로 부르지 않는다. 판정은 경로의 앞마디 일치이고, 중괄호 import 는 항목마다, 줄을 넘는
   경로는 이어서 읽는다. `crates/tasty-doc-guards/tests/domain_does_not_reach_up.rs` 가 강제하고,
   기대값은 0 이다 — 한시 허용 명부를 두지 않는다. 테스트(파일 단위 test-only · 인라인
   `#[cfg(test)]`)는 좌변이 아니다(ADR-0123 과 같은 이유).
2. **역전 수단**은 넷 중 하나다.
   - 양쪽이 쓰는 타입은 **정의를 도메인에** 두고 상위 모듈이 재수출한다 — 발화 주체
     (`core::origin` — 요청 발화 주체와 파일 열기 발화 주체 `FileDispatchOrigin`), 호스트 이벤트
     큐 항목(`core::host_event`), egui-mesh surface 자리표(`core::egui_mesh_surface`).
   - 도메인이 창 쪽 연산을 필요로 하면 **도메인이 trait 을 선언하고** 창 쪽이 구현한다 —
     구조 실행의 창 연산(`core::cascade_window::CascadeWindow`, `AppState` 가 한 줄 위임으로
     구현), 파일 식별 시작(`core::identify_port::IdentifySpawner`, `IdentifyWorker` 가 구현).
     인자는 `&mut dyn` 이다.
   - GUI 동작이 도메인 안에 cfg 로 숨어 있으면 **GUI 쪽으로 옮긴다** — identify·picker 결과
     적용(`file::dispatch::picker_apply`).
   - IPC 배관(워커 스레드로 응답 보내기)은 **어댑터 쪽**에 둔다 — 승인·태스크 대기의 spawn
     (`ipc::handler::approval` · `ipc::handler::agent::task`).
3. **gui 게이트 수를 고정한다.** 같은 가드가 도메인 출하 코드의 `feature = "gui"` 개수를
   양방향으로 고정한다(값의 정본은 `crates/tasty-doc-guards/tests/domain_does_not_reach_up.rs` 의 `GUI_GATES_IN_DOMAIN` — 아래 "잃은 것" 둘째 항). 도메인이 GUI 전용 항목을 cfg 로 들여오면 상위 참조가 없어도
   도메인이 GUI 를 안다 — 그 판단이 들어오는 커밋에 드러나게 한다.
4. **공개 표면은 넓히지 않는다.** 도메인 항목은 `pub(crate)` 그대로다. lib 이 밖에 내는 것은
   여전히 `boot` 하나다. GUI 없는 소비자는 같은 lib 의 headless 구성(`--no-default-features`)
   이고, 그 구성의 lib 테스트가 `CoreBuilder` 로 GUI 없이 도메인을 세운다.

## Consequences

- **얻은 것**: 도메인→상위 참조가 24 → 0 이 됐고, 다시 생기면 가드가 파일:줄로 가리킨다.
  같은 크레이트 안에서는 컴파일러가 이 방향을 못 막는다(`crate::app::…` 은 언제나 해석된다)
  — 가드가 그 몫을 한다.
- **얻은 것**: 구조 실행(`structural_exec`)과 cascade 가 `AppState` 라는 **타입 이름**을
  모른다. 창 쪽이 무엇을 해 주는지가 `CascadeWindow` 의 메서드 목록으로 적혀 있다 — 그 목록이
  ADR-0355 가 "핸들러가 `AppState` 에서 읽는 필드를 모아 대조한다" 고 적은 재는 법의 도메인
  쪽 답이다.
- **얻은 것**: IPC 응답 형태·CLI 출력·plugin wire·권한·포커스 정책이 하나도 안 바뀐다. 모든
  걸음이 이동 또는 위임이다.
- **새로 더한 성질은 방향 하나다.** GUI 없이 도메인을 세우고 시험하는 것은 착수 트리
  (`17e2a7f56`)에서도 이미 참이었다 — headless 구성(`--no-default-features`)의 lib 테스트가
  `CoreBuilder` 로 도메인을 세웠고, 그 수(1494)는 이 결정 뒤에도 같다. 이 결정이 더한 것은
  도메인→상위 참조가 0 이고 가드가 그것을 고정한다는 사실이다.
- **잃은 것**: 크레이트 경계가 주는 두 가지를 못 얻는다. (a) 편집 빌드 범위 — 도메인을 고쳐도
  GUI 를 고쳐도 같은 컴파일 단위가 다시 돈다. (b) 공개 표면의 강제 — `pub(crate)` 는 크레이트
  안에서 모두에게 열려 있어 GUI 쪽이 도메인 내부 필드를 직접 만지는 것은 여전히 막히지 않는다.
  이 결정의 가드는 **도메인→상위** 방향만 본다.
- **잃은 것**: gui 게이트 수가 경계 작업으로 239 → 246 으로 **올랐다**. `core/file.rs` −6(picker
  적용을 뺐다) · `core/cascade_window.rs` +9(포트 메서드 중 호출 자리가 이미 gui 로 가려진
  통지·튜토리얼 관찰 여덟 개와 import 하나) · `core/host_event.rs` +2(`state.rs` 의 모듈 단위
  `allow(dead_code)` 가 가리던 headless `expect` 가 드러났다) · `core/mod.rs` +1 ·
  `core/origin.rs` +1(`FileDispatchOrigin` 을 도메인으로 옮기며 `file::dispatch` 모듈 단위
  `allow` 가 가리던 headless `expect` 가 드러났다). 늘어난
  여덟은 새 GUI 의존이 아니라 이미 있던 게이트가 도메인 쪽 선언에 옮겨 적힌 것이다 — 그
  메서드들은 GUI 타입을 하나도 안 부른다. 그래도 수는 수이고, 크레이트로 떼는 날 이 수가
  그대로 일감이다. 그 뒤 `src/state.rs` 의 모듈 단위 `allow` 를 지운 것(ADR-0355 잔여 ②·③)이
  5 를 더 올려 251 이 됐다 — 그 `allow` 가 가리던 `core` 의 headless 소비자 없는 정의 다섯이
  드러났다(ADR-0355 의 "잔여의 현재 상태"). 이어 도메인 안의 모듈 단위 headless `allow(dead_code)`
  를 지우고 드러난 정의를 항목마다 갈랐다(모듈 속성 줄 자체가 게이트 하나라 지울 때마다 −1):
  `core/attach.rs` +1 · `core/attach_readonly.rs` 0(모듈 전체가 GUI 전용이라 선언에 cfg) ·
  `core/state/soft_occupancy.rs` 0 — 도메인 안에 모듈 단위 headless `allow` 는 더 없다. 도메인
  밖의 것도 뿌리였다: `file/dispatch.rs` 의 모듈 `allow` 를 지우자 `core/origin.rs` 의 판정 둘이
  headless 소비자 없는 정의로 드러나 +2, `intent.rs` 의 것을 지우자 `core/origin.rs` 의 사용자 발화
  주체 variant 둘이 headless 에서 안 만들어지는 것으로 드러나 +2(`expect`), `adapters/ipc.rs` 의 것을
  지우자 `core/session.rs` 의 agent 권한 임시 grant·revoke 가 드러나 +2. 결정 시점에 258 이었다. 조건이
  `not(feature = "gui")` 하나뿐인 모듈 단위 `allow(dead_code)` 는 0 이다. 모듈 단위 `dead_code` 억제
  자체가 없어진 것은 아니다 — 조건 없는 것과 다른 cfg 조합의 것이 남아 있다. 그 자리는 여기 적지
  않고 재는 법만 둔다: `git grep -nE '#!\[(cfg_attr\([^]]*)?allow\([^)]*dead_code' -- '*.rs'`.
- **잃은 것 — 전이 의존은 안 본다.** 도메인이 부르는 형제 모듈이 다시 상위를 부르는 경로는
  가드 밖이다. 형제 모듈이 상위 항목을 재수출하면 그 이름으로 우회된다. 창 상태를 받는
  `file::dispatch` 는 그래서 형제 모듈 전체가 아니라 그 하위 항목 자체를 가드의 표에 올렸다.
- **잃은 것이었다가 닫은 것 — 기존 gui 게이트 뒤의 GUI 크레이트** (2026-09-22 보강,
  [ADR-0490](0490-boundary-guards-close-three-holes-found-by-mutation.md)). 이 결정의 가드는 처음에
  `crate::`·`super::` 경로와 gui 게이트 **수**만 봤다. 이미 있는 게이트 뒤 import 에 `egui::Context`
  를 끼워 넣으면 수가 그대로라 아무것도 안 빨개졌다(변이로 확인). 지금은 같은 가드가 `gui` feature
  의 optional 의존(매니페스트에서 읽는다)과 `windows` 의 창·그리기 하위 경로·창 핸들을 게이트 뒤까지 읽고
  (파일, 경로) 목록으로 고정한다(오늘 0). 남은 한계 — 다른 워크스페이스 크레이트의 GUI 갈래
  (`tasty-platform/gui` 등)는 이름으로 안 갈려 여전히 밖이다.
- **운영 비용**: 도메인에 창 쪽 연산이 새로 필요하면 `CascadeWindow` 에 메서드를 더하고
  `state/cascade_window.rs` 에 위임을 쓴다. 도메인에 gui 게이트를 더하면 가드의 상수를 올리고
  커밋 본문에 판정(ADR-0346 ① 인가)을 적는다.

## Alternatives Considered

- **도메인을 `tasty-core` 크레이트로 뗀다**: 티켓과 분석이 원래 그린 모양이다. 이 트리에서
  안 고른 이유는 셋이다. (1) 도메인 출하 코드의 gui 게이트 239 개 — 크레이트에 `gui` feature
  를 두면 "도메인 경계에 GUI feature 를 넣어 역의존을 숨긴다" 는 것 자체이고, 없애려면 게이트
  자리마다 GUI 쪽 free fn·extension trait 로 옮겨야 한다. (2) 형제 모듈 폐포 — `file`·`store`·
  `hook_handler`·`host_api`·`completion_strategy` 가 함께 가야 하고 그중 `file::dispatch` 가 창
  상태를 받는다. (3) core 밖 212 파일이 부르는 `pub(crate)` 항목을 `pub` 으로 열어야 한다.
  이득(편집 빌드 범위)은 link 가 절반인 증분 비용에서 typeck·codegen 몫뿐이다. 크레이트 수를
  늘리는 것은 완료 조건이 아니다.
- **`AppState` 를 도메인 struct 와 GUI struct 로 가른다(ADR-0355 의 기각안)**: 핸들러 인자
  287 자리를 바꾸는 변경이다. 도메인 쪽 방향은 포트 trait 하나(구현 한 파일, 메서드마다 한 줄 위임)로
  같은 결과를 얻는다 — 도메인이 창 타입을 모른다.
- **포트를 제네릭(`S: CascadeWindow`)으로 받는다**: 구현이 하나(`AppState`)라 단형화 비용은
  없지만 구조 실행 함수 20 여 개와 그 호출자에 타입 인자가 번진다. 구조 op 은 사용자 동작
  단위라 vtable 한 번의 비용이 보이지 않는다.
- **남은 참조를 한시 허용 명부로 두고 가드를 먼저 세운다**: 기존 layering 가드의 형태다.
  이번에는 24 자리를 없애는 비용이 작아(이동·역전 9 걸음) 명부 없이 0 에서 세웠다. 명부가 없으면
  "줄어들기만 한다" 를 지킬 필요도 없다.
- **gui 게이트 수를 파일마다 고정한다**: 파일 사이 이동(이번 작업의 file.rs → picker_apply
  같은)마다 상수가 두 개씩 흔들린다. 물음은 "도메인이 GUI 를 더 아는가" 이고 그것은 합계가
  답한다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- 도메인 출하 코드가 상위 모듈을 이름으로 부른다 — `domain_does_not_reach_up.rs` 의
  `the_domain_does_not_name_an_upper_layer` 가 파일:줄로 가리킨다. 그 자리에서 역전 수단 넷
  중 하나를 고른다. 넷 다 안 맞으면 이 ADR 을 다시 연다.
- 도메인 gui 게이트 수가 움직인다 — 같은 파일의 `gui_gates_in_the_domain_are_pinned`.
- 도메인 출하 코드가 GUI 크레이트를 부른다 — 같은 파일의
  `the_domain_names_no_gui_crate_beyond_the_pinned_list`(ADR-0490 이 더했다).

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- 도메인을 크레이트 밖에서 써야 하는 소비자가 생긴다(별도 데몬 바이너리, plugin host 가
  도메인 타입을 직접 링크 등). 그때는 크레이트 분리가 필요하고, 위 기각 사유 (1)~(3) 이 그대로
  일감 목록이다. 재는 법: 이 가드의 gui 게이트 고정값(0 이 목표) · `git grep -n 'AppState'
  src/file/dispatch.rs` · `git grep -l 'crate::core::' -- src ':!src/core'` 의 파일 수.
- 편집 분포가 "도메인은 드물게, GUI 는 자주" 로 확인되고 증분 빌드가 병목으로 보고된다.
  재는 법: `git log --since=<기간> --name-only --format= -- src/core src/ports | sort -u | wc -l`
  과 같은 기간 `src/view src/adapters/ui src/app` 의 수를 견주고, 도메인 파일 하나 touch 뒤
  `cargo build -p tasty --lib` 벽시계와 GUI 파일 하나 touch 뒤의 벽시계를 같은 부하에서 잰다.

## References

- [ADR-0355](0355-app-state-ownership-is-split-by-the-gui-boundary-not-by-a-second-struct.md) —
  `AppState` 소유권. 그 ADR 의 잔여 ① 중 도메인 쪽을 이 결정의 `CascadeWindow` 가 맡았다
- [ADR-0346](0346-headless-compiles-only-what-it-reaches.md) — headless 컴파일 경계의 세 갈래
- [ADR-0325](0325-the-root-package-splits-into-a-lib-and-a-bin.md) — lib·bin 분리
- [ADR-0123](0123-layering-guard-excludes-cfg-test-modules.md) — 계층 가드가 test 모듈을 빼는 이유
- [ADR-0490](0490-boundary-guards-close-three-holes-found-by-mutation.md) — 이 가드의 판정 범위를
  넓혔다(GUI 크레이트 목록 래칫 · 판정기를 `tasty_doc_guards::crate_paths` 로 공유)
- [아키텍처](../architecture/index.md) · [헤드리스 정의 경계](../dev-guide/headless-build-boundaries.md) ·
  [AppState 필드 소유권](../dev-guide/app-state-ownership.md)
- 결정이 실현된 현재 위치: `crates/tasty-doc-guards/tests/domain_does_not_reach_up.rs` ·
  `core::cascade_window::CascadeWindow` · `state::cascade_window` · `core::identify_port::IdentifySpawner` ·
  `core::origin` · `core::host_event` · `core::egui_mesh_surface` · `file::dispatch::picker_apply`
