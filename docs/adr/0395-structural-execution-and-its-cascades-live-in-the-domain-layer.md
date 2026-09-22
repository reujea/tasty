# ADR-0395: 구조 변경 실행과 그 cascade 는 도메인 계층에 산다 — ADR-0337 의 결정 2 와 핸들러 재사용 조항 개정

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: architecture, boundary, hexagonal, attach, cascade, close, headless, adr-0337
- **Group**: architecture

## Context

구조 변경(split / tab.create / tab.close / tab.move / pane.close / surface.close)은 진입점이
둘이다. 하나는 에이전트 IPC 이고, 다른 하나는 원격 mirror 가 forward 한 구조 op 의 실행
(`core::attach_runtime::execute_forwarded_structural_op`)이다.
[ADR-0337](0337-structural-execution-answers-with-domain-values.md) 은 forward 실행이 도메인
값(`Result<(), String>`)으로 답하게 했다. 그러나 두 방향 역전은 범위 밖으로 남겼다.

- **core → inbound adapter**: forward 실행이 여섯 IPC 핸들러를 JSON params 로 불렀다.
  그리고 그 응답의 에러 메시지를 되풀어 사유로 썼다.
- **core → app**: 자원 회수(`reclaim_closed_surfaces`)와 close cascade
  (`cascade_surface_closed`, `SurfaceCloseCascade`)가 `app::dispatch_domain` 에 있었다.
  headless 사본은 `app::dispatch_domain_stubs` 에 따로 있었다. 두 파일은
  `cfg(feature = "gui")` 가 배타라 기본 빌드가 headless 사본을 아예 안 봤다. 매핑(일곱 필드)도
  빌드마다 한 벌씩이었다.

핸들러 재사용을 없애려면 핸들러가 소유한 검증을 옮겨야 한다. 옮길 검증은 대상 둘 지정 거절,
cwd 확인, kind 필수 파라미터, cwd 상속, 이벤트 확인이다. ADR-0337 이 이 작업을 미룬 이유다.
옮길 때 지켜야 할 것은 셋이다.

- **실패 문구가 byte 단위로 같아야 한다.** forward 회신의 사유는 client toast 에 그대로 나간다.
  기준은 ADR-0337 이 착지한 `331baf491` 의 문구다.
- **forward 된 파라미터 묶음이 IPC 요청과 같은 자리로 읽혀야 한다.** mirror 는 자기가 받은 IPC
  params 를 통째로 `StructuralOp` 의 `params` 에 실어 보낸다. 서버는 거기에 제어 키
  (level / direction / target_surface / type)만 덮어쓴다. 그래서 묶음 안의 `target_pane` ·
  `cwd` · `meta` 를 서버가 IPC 와 같은 규칙으로 읽는다. `target_pane` 을 읽는 규칙
  (`read_u32` — 자르지 않고, 없는 것과 잘못된 것을 가른다)은 어댑터
  (`adapters::ipc::handler::params`)에 있었다.
- **요청 게이트는 진입점에 남아야 한다.** 다음 넷은 IPC 진입의 일이다.
  - IPC 요청당 한 번의 권한·cap·rate-limit 판정([ADR-0277](0277-ipc-admission-and-observation-run-once.md))
  - 원격 하드 점유 거절
  - 호출자 자기 대상 거절
  - 비-holder 구조 변경 차단

  forward 실행은 holder 검증을 지나 들어온다. 그 면제는 "어느 함수를 부르느냐" 로 표현돼 있다.

## Decision

**1. 구조 변경 실행은 `core::structural_exec` 가 소유한다.** 연산마다 함수가 하나다
(`split` · `create_tab` · `close_tab` · `move_tab` · `close_pane` · `close_surface`). 각 함수가
검증·`Core::apply`·cascade 를 한다. IPC 핸들러와 forward 실행이 **같은 함수**를 부른다.
실패는 `StructuralFailure` 로 돌려준다. 갈래는 셋이다.

- `Rejected`: IPC 에서 `invalid_params`
- `MissingEvent`: IPC 에서 `internal_error`
- `Apply`: IPC 에서 `structural_apply_error`

문구는 두 진입점이 같은 값을 받는다.

- 핸들러는 wire 변환만 한다. 파라미터 파싱과 응답 JSON 조립이다.
- IPC 진입 게이트는 핸들러와 IPC 디스패치에 남는다. 호출자 자기 대상 거절, 하드 점유 거절,
  요청당 권한 판정이 여기에 든다.
- forward 전용 단계는 forward 실행에 남는다. anchor resolve, 복원 스택 캡처, 즉시-tap 억제
  구간, delta 계산이 여기에 든다.
- forward 된 close 가 복원 스택에 남는 축(`save_snapshot=true`,
  [ADR-0264](0264-mirror-restore-closed-item-runs-on-the-remote.md) 결정 4)은 이제 도메인
  함수의 **인자**다. 전에는 holder 전용 핸들러 함수로 표현했다. params 로는 여전히 표현하지
  않는다.

**2. 파라미터 스칼라 판정(`read_int` · `read_u32` …)은 `core::param_bag` 이 소유한다.**
어댑터의 `params` 모듈은 그것을 재수출하고, `invalid_params` 로 감싸는 얼굴만 남는다.

**3. 구조 변경 cascade 는 `core::structural_cascade` 가 소유한다.** 여기에 드는 것은 다음과 같다.

- split / tab 생성 / tab·pane·surface close cascade
- `reclaim_closed_surfaces`
- `SurfaceCloseCascade` 와 그 두 생성자

두 빌드가 같은 파일을 컴파일하므로 함수 하나에 본문 하나다. gui 와 headless 가 갈리는 지점은
본문 안의 `#[cfg(feature = "gui")]` 블록이다. 기준은 ADR-0337 과 같다 — 그 효과에 headless 에서
소비자가 있는가. headless 에서 안 읽히는 필드·인자는 `cfg_attr(not(feature = "gui"),
expect(...))` 로 적는다([ADR-0346](0346-headless-compiles-only-what-it-reaches.md) 의 "근거" 형태).
`app::dispatch_domain_stubs` 에는 옮기지 않은 워크스페이스 단위 cascade 만 남는다.

**4. headless 동작은 옛 stub 과 같게 둔다. 예외는 관측 불가능한 하나다.** split cascade 의
사용자 origin 포커스 이동은 공통 본문에 둔다. 옛 headless stub 은 split cascade 전체가
no-op 이었다. 그러나 그 분기의 조건은 `User` origin 이고, 그 origin 을 세우는 발화점
(단축키·메뉴·마우스)은 gui 에만 있다. 그래서 headless 결과가 같다. close cascade 의
`debug_assert`(workspace level ↔ 제거 위치)도 공통이다. `Core` 가 내는 이벤트의 불변식이라
빌드 형태와 무관하다.

**5. `ForwardedDelta.converted_surface` 는 유지한다.** forward 된 convert 가 kind 를 실제로
바꾸면 `PluginManager::drop_egui_mesh_frame` 을 불러야 한다. plugin manager 는 forward 실행이
받는 상태(`Core` / `AppState` / `CoreState`) 어디에도 없다. 두 빌드 모두 그것을 소유하는 쪽이
forward 실행의 호출자다(`app/event_handler.rs` · `boot/headless_stream.rs`). 그래서 실행 결과를
값으로 돌려주고 호출자가 투영하는 지금 모양이 의존 방향에 맞다. 재검토 전의 필드 문서는 이것을
"App(GUI) 레벨 상태" 라 적었는데, headless 호출자도 같은 일을 한다. 그래서 문서만 고쳤다.

**개정하지 않는 것.** ADR-0337 의 다음 결정은 유효하다.

- 결정 1 의 "구조 op 실행은 도메인 값으로 답하고, 핸들러를 안 거치는 op 는 wire 타입을
  만들지 않는다"
- 결정 3 의 "`MoveSurfaceApplied` 매핑은 한 이름이 소유한다"

개정하는 것은 둘이다.

- 결정 2 의 "빌드 형태마다 한 함수"가 "한 함수"가 됐다.
- "핸들러 재사용 자체는 범위 밖" 조항은 이 ADR 이 실행했다. 그 결과 결정 1 의 "변환은
  `handler_result` 한 함수가 한다" 는 전제가 사라졌다 — forward 실행은 wire 타입을 전혀 안
  만든다.

외부 동작(IPC 응답 형태·코드·문구, CLI 출력, attach wire 의 `StructuralResult` 사유)은 바뀌지
않는다.

## Consequences

- **얻은 것**: `core` 의 production 코드가 `adapters` 와 `app` 을 부르지 않는다.
  `execute_forwarded_structural_op` 가 핸들러를 부르던 import 와, `app::dispatch_domain` 을 부르던
  두 자리가 사라졌다.
- **얻은 것**: 실패 문구를 두 시험이 나눠 잰다. 둘은 서로 다른 물음에 답한다.
  - **진입점 사이의 갈림** —
    `forward_exec_tests::forward_and_ipc_fail_with_the_same_reason_for_the_same_input`. 같은
    입력에 IPC 에러 메시지와 forward 회신 사유가 같은지 본다. 기준 문구는 모른다 — 도메인
    함수의 문구를 바꾸면 두 진입점이 함께 바뀌어 이 시험은 초록이다.
  - **기준 문구의 고정** — `forward_exec_tests::failure_reasons_keep_the_base_literals`. 같은
    입력들의 두 진입점 문구를 `331baf491` 의 리터럴과 byte 단위로 단정한다. 문구를 바꾸는
    변경은 여기서 빨개진다. 실측: `structural_exec.rs` 의 `"cwd does not exist: {}"` 두 자리와
    `"Cannot specify both …"` 한 자리를 바꾸면 lib 전량에서 이 시험 하나만 죽고, 세 자리를
    한 자리씩 바꿔도 각각 해당 입력에서 죽는다.

  두 시험의 입력 표는 같다(다섯 개): 서버에 없는 kind, 서버에 없는 cwd(탭·split), 묶음에 실린
  `target_pane`, 잘못된 `target_pane`. 표 밖의 문구는 이동 시점에 `331baf491` 과 문자열 리터럴
  집합으로 대조했다. 대조 대상은 forward 실행과 여섯 재사용 핸들러(지금은 도메인 함수)의
  리터럴이고, 두 집합이 같았다. 옮겨진 `read_int` · `read_u32` · `malformed` 와
  `structural_apply_error` 의 본문도 같았다. 이 집합 대조는 일회성 측정이지 채널이 아니다.
- **얻은 것**: close 매핑(`SurfaceCloseCascade` 생성자)과 cascade 공통 단계는 기본 빌드 하나로
  모든 자리가 재진다. ADR-0337 이 "재는 채널이 없다" 고 적은 짝이 없어졌다.
- **잃은 것**: `StructuralFailure` 라는 타입이 하나 늘었다. 그리고 IPC 핸들러는 도메인 함수 앞에
  파싱 층을 하나 더 거친다.
- **드러난 것 — 옛 동작 그대로 보존한 결함 후보 둘.** 둘 다 이 결정이 바꾸지 않는다.
  - forward 된 pane-level split 은 묶음에 `target_pane` 이 있으면 "Cannot specify both …" 로
    실패한다. mirror 에서 `--target-pane` 으로 split 하면 그 키가 묶음에 실려 오고, 서버가
    `target_surface` 를 덮어쓰기 때문이다.
  - headless close cascade 는 워크스페이스가 통째로 사라질 때 그 워크스페이스의 memory scope
    를 purge 하지 않는다(C4). `workspace.closed` 통지와 한 함수(`after_workspace_removed`)에
    묶여 함께 빠져 있다.
- **운영 비용**: `core::structural_cascade` 에 단계를 더할 때는 그것이 gui 블록 안인지 밖인지를
  정해야 한다. 기준(소비자가 있는가)은 그 모듈 문서와 `docs/architecture/close-sequence.md` 의
  표에 있다. gui 블록 안의 코드는 headless 빌드가 컴파일하지 않는다. 그래서 두 조합 check 는
  여전히 필요하다.

## Alternatives Considered

- **A. forward 실행을 `app` 으로 옮긴다**(ADR-0337 의 대안 A). 방향 역전이 사라지는 것은 같다.
  안 고른 이유는 셋이다.
  - 핸들러 재사용이 그대로 남는다. `app` 이 inbound 어댑터를 부르는 것도 같은 역전이다.
  - headless 경로(`boot/headless_stream.rs`)도 그 실행을 부른다.
  - 즉시-tap 억제 판정기(`src/source_guards/auto_tap_suppression_window.rs`)의 좌변까지 함께
    옮겨야 한다.
- **B. 도메인 함수가 파라미터 묶음 전체를 받아 level·대상까지 파싱한다.** forward 실행이 IPC 와
  완전히 같은 파서를 타게 된다. 안 고른 이유는 IPC 전용 해석이 도메인으로 들어오기 때문이다.
  nickname 을 memory 에서 찾아 대상으로 푸는 해석과 level 문자열 해석이 그것이다. forward 는 그
  값을 타입으로 가지고 있다. forward 에서 파싱 결과가 달라질 수 있는 키는 `target_pane` 하나다.
  그 하나를 같은 판정(`param_bag::read_u32`)으로 읽는다.
- **C. 파라미터 판정을 옮기지 않고 core 가 어댑터의 `params` 를 부른다.** 옮기는 변경이 없다.
  안 고른 이유는 역전이 한 자리 남기 때문이다. 대상이 순수 함수라도 core → inbound adapter
  방향은 그대로다.
- **D. `converted_surface` 대신 `AppState` 에 pending 큐를 두고 App 루프가 비운다.** 반환값을
  없앨 수 있다. 안 고른 이유는 호출자가 이미 결과를 동기적으로 받는 자리라는 것이다. 큐는 같은
  정보를 한 프레임 늦게, 두 빌드의 drain 배선을 더해 전달할 뿐이다.
- **E. headless 에서 split 포커스 이동도 gui 블록에 가둔다.** 옛 stub 과 줄 단위로 같다. 안 고른
  이유는 그 분기가 사용자 상태(포커스)를 고치는 일이라는 것이다. 통지가 아니므로 "소비자가
  있는가" 기준으로는 공통이다. 또 headless 에서 관측할 수 없다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `src/core/` 의 production 코드에 `crate::adapters::` 나 `crate::app::` 참조가 다시 생긴다.
  재는 법: `grep -rn "crate::adapters::\|crate::app::" src/core` 에서 `#[cfg(test)]` 모듈 밖이고
  주석이 아닌 줄을 센다. 이 결정 직후 0 이다(주석 안의 언급 둘은 문서 링크다). 튜토리얼 관찰은
  그래서 `AppState` 의 gui 전용 메서드(`observe_tutorial_surface_split` ·
  `observe_tutorial_pane_split`)를 거친다 — cascade 가 UI 어댑터 타입을 직접 이름 부르지 않게.
- `forward_exec_tests::forward_and_ipc_fail_with_the_same_reason_for_the_same_input` 가 실패한다.
  두 진입점이 다른 실행을 타기 시작했다는 뜻이다.
- `forward_exec_tests::failure_reasons_keep_the_base_literals` 가 실패한다. 외부에 보이는 실패
  문구가 바뀌었다는 뜻이다 — 의도한 변경이면 그 시험의 표와 이 ADR 의 호환 판단을 함께 고친다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다.

- headless 에 `User` origin 을 세우는 발화점이 생긴다. 그러면 결정 4 의 "관측 불가능" 이 깨진다.
  재는 법: headless 빌드에서 `IntentOrigin::User` 를 만드는 자리를 찾는다
  (`cargo check --no-default-features` 로 컴파일되는 파일 중 그 생성자를 부르는 곳).
- plugin manager 가 `Core` 나 `CoreState` 안으로 들어온다. 그러면 결정 5 의 근거가 사라지고,
  forward 실행이 직접 투영할 수 있다. 재는 법: `drop_egui_mesh_frame` 의 소유 타입을 확인한다.

## References

- 개정 대상: [ADR-0337](0337-structural-execution-answers-with-domain-values.md) (결정 2 · 핸들러 재사용 조항)
- 개정 패턴 선례: [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
- [close-sequence](../architecture/close-sequence.md) — 자원 회수 소유와 gui/headless 차이표
- [attach 동작](../dev-guide/attach-behavior.md) — forward 실행과 비-holder 차단의 메커니즘
- [ADR-0277](0277-ipc-admission-and-observation-run-once.md) — 진입 요청당 한 번의 판정. 이 결정은 그 자리를 옮기지 않는다
- [ADR-0264](0264-mirror-restore-closed-item-runs-on-the-remote.md) — forward 된 close 의 복원 스택 축
- [ADR-0480](0480-a-forwarded-close-carries-who-asked-for-it.md) — forward 된 close 의 복원 스택 여부는 이제 op 의 origin 이 정한다
- 결정이 실현된 현재 위치: `core::structural_exec`(`split` · `create_tab` · `close_tab` · `move_tab` ·
  `close_pane` · `close_surface` · `StructuralFailure`), `core::structural_cascade`,
  `core::param_bag`, `core::attach_runtime` 의 `forward_result` 와 `ForwardedDelta::converted_surface`,
  `adapters::ipc::handler` 의 `structural_failure_response`
