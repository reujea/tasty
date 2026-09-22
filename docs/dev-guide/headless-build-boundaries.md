# 헤드리스 정의 경계

headless(`--no-default-features`)는 IPC/CLI 와 attach 서버를 실행하는 제품 형태다. GUI 가
없다는 이유로 공유 Core·AppState·registry 를 통째로 숨기지 않는다. `dead_code` 진단이
가리키는 정의의 생산자와 소비자를 따라가며 아래 기준으로 컴파일 대상을 나눈다.

결정과 그 근거는 [ADR-0346](../adr/0346-headless-compiles-only-what-it-reaches.md).

## 세 갈래

**① GUI 전용 정의 — `cfg(feature = "gui")`**

headless 에 생산자도 호출자도 없는 정의. 모듈 통째가 그 안쪽이면 모듈 선언에 붙이고,
일부 항목만이면 그 항목에 붙인다.

**② GUI 와 순수 테스트가 함께 쓰는 정의 — `cfg(any(feature = "gui", test))`**

테스트가 **실제로 그 정의를 호출할 때만** 쓴다. 호출하지 않는데 `test` 를 포함시키면 그
정의는 라이브러리 대신 테스트 구성에서 dead 로 남는다 — 자리만 옮긴 것이다. 테스트의
존재는 headless 출하 코드가 그 정의를 쓴다는 증거가 아니다.

**③ headless 도 만들지만 읽는 자가 GUI 뿐인 정의 — `expect(dead_code, reason = ...)`**

빼면 동작이 바뀌므로 제외하지 않는다. 자리마다 `cfg_attr(not(feature = "gui"), expect(...))`
로 계약 하나만 설명한다. 조건이 달라져 진단이 사라지면
`unfulfilled_lint_expectations` 가 그 자리를 이름으로 **경고한다** — 빌드·CI 를 막지는 않는다
(그 lint 는 warn 이고 `Cargo.toml` 의 deny 목록 밖이며, CI 는 `-D warnings` 를 안 쓴다). 모듈이나 crate 전체를 덮는
dead_code 예외는 쓰지 않는다.

모듈·crate 단위 `dead_code` 억제는 이 경계 밖의 두 부류에만 남는다 — 통합 시험 공용 모듈
(`tests/*/mod.rs`: test binary 마다 쓰는 부분집합이 달라 binary 별로 dead 가 생긴다)과 생성
파일(`tasty-design-tokens` 의 `generated/primitive.rs` 와 그것을 쓰는 생성기: `pub(crate)` 스케일을
미참조 엔트리까지 보존한다). 둘 다 자리에 사유가 붙어 있다. 그 밖에는 0 이고, 결정과 재검토
조건은 [ADR-0530](../adr/0530-module-wide-dead-code-allows-stay-only-where-the-judge-cannot-see-the-use.md).
세는 법: `git grep -nE '#!\[(cfg_attr\([^]]*)?allow\([^)]*dead_code' -- '*.rs'`.

지금 ③ 에 해당하는 것은 열하나다.

| 정의 | 왜 남는가 |
|---|---|
| 구조 op forward 큐의 원소(`PendingStructuralForward`, `core/impl_mirror.rs`) | headless 의 `Core::apply` 는 이 큐에 넣지 않고 거절한다([ADR-0538](../adr/0538-headless-refuses-mirror-structural-forward.md)). 정의가 남는 것은 두 조합이 공유하는 `mark_last_forward_*` 가 큐의 마지막 원소를 표시하기 때문이고, op 를 읽어 보내는 쪽은 GUI 의 `about_to_wait` 뿐이다. 같은 줄에 있던 git query · markdown content · resize 의 forward 큐는 채우는 자리도 gui 전용이라 필드째 ① 이다 |
| `SurfaceKindDef` 의 입력·줌·복사 플래그와 변환 입력 popup id | plugin 매니페스트의 `SurfaceKindDecl` 에서 복사되는 값이다. 복사는 headless 에서도 일어난다 |
| CoreEvent 의 페이로드(터미널 이벤트 중 제목 · 알림 · 벨 · 명령 완료 · 셸 통합 힌트 · 클립보드, `RestoredKind` 의 인덱스) | variant 는 headless 에서도 발화하지만 그 빌드의 drain 이 `other` 갈래로 흘린다. OSC 7 cwd(`TerminalCwdChanged`)는 여기 없다 — headless PTY drain 이 읽는다(아래 "두 조합이 같게 하는 것") |
| 호스트 이벤트 큐 항목(`PendingHostEvent` · `PendingSurfaceClosed`, `core/host_event.rs`) | 세우는 코드(`AppState` 의 enqueue 메서드)가 headless 빌드에도 컴파일되지만 비우는 자는 GUI 메인 루프뿐이다. 그 메서드들이 headless 에서 어느 갈래인지는 [AppState 필드 소유권](app-state-ownership.md) 이 적는다 |
| 파일 열기 발화 주체(`FileDispatchOrigin`, `core/origin.rs`) | 도메인의 `DispatchFile` intent 가 headless 에도 컴파일되지만 값을 만드는 자리(explorer·링크·드롭·picker·`file_handler.dispatch` arm)가 전부 GUI 다. `file::dispatch` 모듈 자체는 headless 라이브러리에 없다(링크 해석·대상 판정을 시험이 부르므로 ②) |
| workspace 생성 cascade 의 필드(`WorkspaceCreatedCascade`, headless 판 `app/dispatch_domain_stubs.rs`) | 만드는 자리(`workspace.create` IPC · workspace intent)는 두 빌드가 공유하지만 headless 의 cascade 는 no-op 이라 필드를 읽는 자가 gui 뿐이다. 그 파일 전체가 `not(feature = "gui")` 라 조건 없는 `expect` 로 적는다 |
| `App` 의 필드(`src/app.rs`) | headless boot 도 `App` 을 세워 Core 를 쓰지만, 일부 필드를 읽는 자는 gui 이벤트 루프뿐이다 |
| `AppEvent::Shutdown` · `AppEvent::QuitRequested`(`src/app/event.rs`) | 만드는 자리가 gui 창 라이프사이클·종료 경로뿐이다. 열거와 그 match 는 headless 도 컴파일한다 |
| `AppState::preset_store` | headless 도 `AppState::new` 로 Core 의 사본을 받지만 읽는 자(preset popup)가 GUI 뿐이다. 에이전트의 preset IPC 는 `Core.preset_store` 를 잠근다 |
| `ModalKind` 의 variant | 모달을 여는 자리(`App::open_modal`)가 GUI 뿐이다. 열거와 `active_modal_kind` 는 `ui.state` 덤프가 debug 빌드의 두 조합에서 같은 키로 읽는다(release 헤드리스에는 `active_modal_kind` 필드가 없다) |
| 사용자 발화 intent 의 variant(`Intent` 의 단축키·메뉴 variant · `UiIntent` · `OpenPopupMode` · `ConvertTarget` · 도메인의 `IntentOrigin::User` · `UserSource`) | 만드는 자리(단축키·메뉴·우클릭·popup)가 GUI 뿐이다. 열거와 그 match 는 headless 의 intent drain 도 컴파일한다. `IntentOrigin::User` 는 headless 시험이 만들므로 `not(test)` 도 조건이다 |

## 판정은 바깥에서 안으로

진단 목록은 평면이지만 사실은 그래프다. 어떤 정의가 dead 로 보이는 이유가 **그 호출자가
같은 실행에서 함께 dead 로 잡혔기 때문**일 수 있다. 안쪽을 먼저 자르면 컴파일이 깨진다.

실측 2026-09-21: `NotificationStore` 의 읽음 처리 셋을 GUI 로 게이팅했더니 headless 라이브러리가
`E0599` 로 깨졌다. 그 셋을 부르는 `CoreState::mark_notification_read` 가 그 빌드에서
컴파일되는데, 그쪽도 같은 실행에서 dead 로 보고되고 있었다.

## 재는 법 — 여덟 칸

`gui` × `headless` 의 두 feature 조합을, 각각 라이브러리와 `--all-targets` 로, 각각 debug 와
`--release` 프로필에서 잰다.

```bash
cargo check --workspace --no-default-features
cargo check --workspace --no-default-features --all-targets
cargo check --workspace
cargo check --workspace --all-targets
cargo check --workspace --release --no-default-features
cargo check --workspace --release --no-default-features --all-targets
cargo check --workspace --release
cargo check --workspace --release --all-targets
```

한 칸만 보면 나머지에서 회귀가 조용히 나간다. 라이브러리와 테스트 구성은 서로 다른
물음의 답이다 — `cfg(any(feature = "gui", test))` 는 앞을 풀고 뒤를 그대로 남긴다.

프로필도 같은 식으로 다른 물음이다. `debug_assertions` 는 feature 와 독립인 두 번째 경계라,
headless 쪽 호출자가 debug 전용 핸들러(`#[cfg(debug_assertions)]` 모듈)뿐인 정의는 debug 네
칸에서 살아 있고 release headless 에서만 dead 가 된다. 그런 정의의 경계는 호출자 조건의
합집합 — `cfg(any(feature = "gui", debug_assertions))`, 테스트가 부르면 `test` 를 더한다.
실측 2026-09-21: debug 네 칸이 0 이던 트리에서 release headless 라이브러리가 dead 16 건을 냈다.

커밋 전에 이 여덟을 보는 자동 채널은 없다. headless debug 컴파일은 pre-push 와
`check-headless` 가, release gui 라이브러리는 pre-push 와 `check-release` 가 본다
([ci-gates](ci-gates.md)). release headless 두 칸과 release gui `--all-targets` 칸은 어떤 자동
채널도 안 본다 — 직접 돌리지 않으면 아무도 안 돈다.

## 이 경계가 드러낸 것

경계를 그으면서 보인 사실 둘이다. 둘 다 기능을 뺀 것이 아니라 이미 그랬던 것이 보이게 된
것이었고, 각각 한쪽으로 정했다 — 아래 두 절.

- headless 는 레이아웃을 저장하지도 복원하지도 않는다. capture·restore·scrollback 경로
  전체에 그 빌드의 호출자가 없다.
- headless 의 OSC 7 cwd 변경은 탭 이름을 갱신하지 않았다. 터미널 이벤트에서 그 intent 로
  가는 배선이 그 빌드에 없어, drain 의 처리 갈래가 도달 불가능했다.

## 두 조합이 같게 하는 것

- **OSC 7 → 탭 이름.** 셸이 OSC 7 로 알린 cwd 가 그 탭의 이름이 되고(명시 이름·OSC 제목이
  없을 때) 레이아웃 dirty 가 선다. gui 는 `TerminalCwdChanged` → `SurfaceCwdChanged` 두 단
  cascade 가 하고, headless 는 PTY drain(`src/boot.rs` 의 `handle_terminal_output`)이
  `intent::headless::apply_terminal_cwd_changed` 로 같은 engine 갱신을 직접 한다 — gui 의
  둘째 단 intent 는 view redraw 를 함께 싣는 gui 전용이라 헤드리스에는 그 두 줄만 필요하다
  ([ADR-0111](../adr/0111-headless-drains-the-intent-queue.md) 의 "engine 에 완결되는 부분만").
  시험은 `tests/e2e_tests.rs` 의 `an_osc7_cwd_becomes_the_tab_name` 이 두 조합에서 같은 단언으로
  재고, 배선이 빠지면 headless 라이브러리가 dead_code 로 먼저 깨진다.

## 두 조합이 다르게 두는 것

다르게 두는 것은 조용히 두지 않는다 — 요청이 오면 거절로, 요청이 없으면 알림으로 말한다.

- **레이아웃 영속.** headless 는 레이아웃을 저장도 복원도 하지 않는다. 워크스페이스는 프로세스 수명
  동안만 산다. 헤드리스에 닿는 입력은 설정 `general.restore_layout` 하나뿐이라(저장·복원 IPC 는 없고
  두 intent 는 gui 전용이다), 그 설정이 켜져 있으면 부팅이 warn 한 줄로 무시된다고 말하고
  `system.info` 의 `layout_slot: null` 이 in-band 답이다. 근거·대안
  [ADR-0539](../adr/0539-headless-does-not-persist-layouts.md). 시험은 `src/boot.rs` 의
  `a_restore_layout_setting_is_announced_as_ignored`(문구)와 `tests/e2e_tests.rs` 의
  `a_headless_daemon_warns_at_boot_that_restore_layout_is_ignored`(부팅이 warn 으로 내는가) ·
  `a_headless_daemon_answers_that_it_holds_no_layout_slot`.
- **attach mirror 로 나가는 forward 셋.** git 조회(`git_viewer.query`) · markdown 원문
  (`markdown_mirror.content_request`) · mirror 구조 op. 큐를 비워 attach 채널로 보내는 쪽이 gui 에만
  있어 headless 는 셋 다 즉시 거절한다. 앞의 둘은 `-32017` 문구가 메서드 이름을 싣고, 셋째는
  `Core::apply` 가 `-32603` mirror 문구에 빌드 조합이 사유라고 덧붙인다
  ([ADR-0538](../adr/0538-headless-refuses-mirror-structural-forward.md)).

관련 문서: [app-state-ownership](app-state-ownership.md)(이 규칙을 `AppState` 필드에 적용한 표) ·
[build](build.md) · [model-view-split](model-view-split.md) ·
[unit-test-isolation](unit-test-isolation.md) · [action-dispatch](../design/flows/action-dispatch.md)
