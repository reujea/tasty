# 헤드리스 정의 경계

headless(`--no-default-features`)는 IPC/CLI 와 attach 서버를 실행하는 제품 형태다. GUI 가
없다는 이유로 공유 Core·registry를 통째로 숨기지 않는다. GUI는 MainViewState, headless는 별도 CommandContext를 소유한다. `dead_code` 진단이
가리키는 정의의 생산자와 소비자를 따라가며 아래 기준으로 컴파일 대상을 나눈다.

결정과 그 근거는 [헤드리스 IPC 지원 범위](headless-ipc-surface.md).

## 세 갈래

**① GUI 전용 정의 — `cfg(feature = "gui")`**

headless 에 생산자도 호출자도 없는 정의. 모듈 전체가 해당하면 모듈 선언에 붙이고,
일부 항목만이면 그 항목에 붙인다.

View의 비동기 파일 읽기 receipt와 대기 스크립트는 GUI 전용이다. 공유 생성자에서도
이 필드의 초기화에 같은 feature 조건을 적용한다.

**② GUI 와 순수 테스트가 함께 쓰는 정의 — `cfg(any(feature = "gui", test))`**

테스트가 **실제로 그 정의를 호출할 때만** 쓴다. 호출하지 않는데 `test` 를 포함시키면 그
정의는 라이브러리 대신 테스트 구성에서 사용되지 않는 정의로 남는다. 테스트의
존재는 headless 제품 코드가 그 정의를 쓴다는 증거가 아니다.

시험 helper의 OS 조건도 실제 호출자와 맞춘다. journal의 임시 저장소 전환 helper와 child registry의 동기 load·flush·초기 데이터 주입 helper는 Unix GUI 시험만 호출하므로 `all(test, unix, feature = "gui")` 범위에 둔다. 시험 전용 모듈 안에서는 `test`가 이미 적용되므로 OS와 feature 조건만 더한다. Windows에서는 이 helper를 만들지 않으며 production 복구·writer 동작의 조건은 바꾸지 않는다.

**③ headless 도 만들지만 읽는 자가 GUI 뿐인 정의 — `expect(dead_code, reason = ...)`**

빼면 동작이 바뀌므로 제외하지 않는다. 자리마다 `cfg_attr(not(feature = "gui"), expect(...))`
로 그 항목에 필요한 이유를 설명한다. 조건이 달라져 진단이 사라지면
`unfulfilled_lint_expectations` 가 해당 항목을 **경고한다**. 그 lint 는 warn 이고 `Cargo.toml` 의
deny 목록 밖이라 `cargo check`(pre-push B.6 포함)에서는 경고로만 보이지만, CI 의 headless clippy
스텝(`check-headless`·`check-windows` 의 `cargo clippy (headless)` 와 `check-headless` 의 `cargo clippy (headless, release)`)은
`-D warnings` 로 돌아 **실패한다**. 모듈이나 crate 전체를 덮는
dead_code 예외는 쓰지 않는다.

모듈·crate 단위 `dead_code` 억제는 이 경계 밖의 두 부류에만 남는다 — 통합 시험 공용 모듈
(`tests/*/mod.rs`: test binary 마다 쓰는 부분집합이 달라 binary 별로 dead 가 생긴다)과 생성
파일(`tasty-design-tokens` 의 `generated/primitive.rs` 와 그것을 쓰는 생성기: `pub(crate)` 스케일을
미참조 엔트리까지 보존한다). 둘 다 자리에 사유가 붙어 있다. 그 밖에는 0 이고, 결정과 재검토
조건은 [헤드리스 IPC 지원 범위](headless-ipc-surface.md).
세는 법: `git grep -nE '#!\[(cfg_attr\([^]]*)?allow\([^)]*dead_code' -- '*.rs'`.

③에 해당하는 정의는 아래처럼 분류한다.

| 정의 | 왜 남는가 |
|---|---|
| `SurfaceKindDef` 의 입력·줌·복사 플래그와 변환 입력 popup id | plugin 매니페스트의 `SurfaceKindDecl` 에서 복사되는 값이다. 복사는 headless 에서도 일어난다 |
| CoreEvent 의 페이로드(터미널 이벤트 중 제목 · 알림 · 벨 · 명령 완료 · 셸 통합 힌트 · 클립보드, `RestoredKind` 의 인덱스) | variant는 headless에서도 발생하지만 그 빌드의 drain 이 `other` 분기에서 버린다. OSC 7 cwd(`TerminalCwdChanged`)는 여기 없다 — headless PTY drain 이 읽는다(아래 "두 조합이 같게 하는 것") |
| 호스트 이벤트 큐 항목(`PendingHostEvent` · `PendingSurfaceClosed`, `core/host_event.rs`) | enqueue 메서드는 두 빌드에서 컴파일된다. 헤드리스도 host event 큐를 비우지만 `HookFired`만 적용하고, 이 항목의 GUI 전용 payload는 읽지 않는다. 그 메서드들이 headless 에서 어느 갈래인지는 [MainViewState 필드 소유권](app-state-ownership.md) 이 적는다 |
| 파일 열기 요청의 주체(`FileDispatchOrigin`, `core/origin.rs`) | 도메인의 `DispatchFile` intent 가 headless 에도 컴파일되지만 값을 만드는 자리(explorer·링크·드롭·picker·`file_handler.dispatch` arm)가 전부 GUI 다. `file::dispatch` 모듈 자체는 headless 라이브러리에 없다(링크 해석·대상 판정을 시험이 부르므로 ②) |
| `App` 의 필드(`src/app.rs`) | headless boot 도 `App` 을 세워 Core 를 쓰지만, 일부 필드를 읽는 자는 gui 이벤트 루프뿐이다 |
| `AppEvent::Shutdown` · `AppEvent::QuitRequested`(`src/app/event.rs`) | 만드는 자리가 gui 창 라이프사이클·종료 경로뿐이다. 열거와 그 match 는 headless 도 컴파일한다 |
| 사용자가 발생시킨 intent 의 variant(`Intent` 의 단축키·메뉴 variant · `UiIntent` · `OpenRequest` · `ConvertTarget` · 도메인의 `IntentOrigin::User` · `UserSource`) | 만드는 자리(단축키·메뉴·우클릭·popup)가 GUI 뿐이다. 열거와 그 match 는 headless 의 intent drain 도 컴파일한다. `IntentOrigin::User` 는 headless 시험이 만들므로 `not(test)` 도 조건이다 |

## 판정은 바깥에서 안으로

헤드리스에 생산자나 호출자가 없는 GUI 정의는 해당 feature에서 제외한다. 두 빌드에서 생산되지만 GUI만 읽는 값은 항목별 expect와 이유를 남긴다. 테스트가 실제 호출하는 경우에만 test 조건을 포함한다. 진단은 호출자부터 안쪽 정의 순서로 해결한다. GUI/헤드리스, lib/all-targets, debug/release 조합을 모두 검사해야 debug 전용 호출자 때문에 가려진 미사용 정의를 찾을 수 있다. crate 전체의 dead_code 허용으로 경계를 숨기지 않는다.

모듈 전체 dead_code 허용은 각 test binary가 일부만 사용하는 통합 테스트 공용 모듈과, 미사용 primitive 항목 보존이 계약인 생성 코드에만 둔다. 그 외 정의는 호출자가 없으면 삭제하고 조건부 호출자는 같은 cfg로 제한하며 생산은 되지만 일부 조합에서만 읽히면 이유 있는 expect를 쓴다. 공유 테스트가 하나의 binary로 합쳐지거나 생성 항목이 pub이 되면 넓은 억제도 제거할 수 있다.

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

각 조합은 다른 사용 경로를 검사한다. `cfg(any(feature = "gui", test))`인 정의는
헤드리스 라이브러리에서 제외돼도 테스트 구성에는 남는다. 테스트가 실제로 호출하지 않으면
그 구성에서 미사용 진단이 발생한다.

`debug_assertions`는 GUI feature와 별개다. debug 핸들러만 사용하는 정의는 debug 빌드에서
정상이더라도 release 헤드리스에서 미사용으로 판정될 수 있다. 해당 정의에는 호출자 조건을
합친 `cfg(any(feature = "gui", debug_assertions))`를 적용하고, 테스트가 호출하면 `test`도 더한다.

커밋 전에 여덟 조합을 모두 실행하는 자동 검사는 없다. headless debug 컴파일은 `check-headless` 가
workspace 전체를, pre-push B.6 이 루트 패키지와 그 의존을 본다(두 범위의 차이는 [ci-gates](ci-gates.md#로컬-훅이-앞당겨-주는-것)). release gui 라이브러리는 pre-push 와 `check-release` 가 본다.
release headless `--all-targets` 칸은 `check-headless` 의 `cargo clippy (headless, release)` 가 본다
([ci-gates](ci-gates.md)). 타깃 옵션 없는 release headless 칸은 lib·bin 만 선택하고 시험·예제에 필요한
dev-dependency 를 선택하지 않아(예: 루트의 `tokio/test-util`) feature 집합이 다르므로 별도 검사다. 이 칸은
자동 검사에 포함되지 않으므로 직접 실행한다. release gui `--all-targets` 칸은 Windows 에서만 `check-windows` 의
`cargo clippy (release)` 가 보고 Linux·macOS 에서는 자동 검사에 포함되지 않으므로 직접 실행한다.

## 두 조합이 같게 하는 것

헤드리스는 Intent 큐를 IPC 응답 전, 플러그인 응답 전, 메인 루프 대기 전에 처리한다. 상태 변경을 완료한 뒤 성공을 돌려주므로 `set_mark` 직후 조회에서도 설정한 mark를 읽을 수 있다. 한 번의 처리는 최대 8라운드이며 남은 작업은 다음 호출에서 이어간다. 반복 상한에 닿으면 상한부터 올리지 말고 작업이 다시 생성되는 원인을 살핀다. OSC 7의 cwd 변경은 PTY 이벤트 처리에서 탭 이름과 레이아웃 변경 상태를 직접 갱신한다. 화면 갱신·메뉴·알림음처럼 GUI에만 있는 효과는 실행하지 않는다.

host event 큐도 같은 세 처리 지점에서 비운다. `HookFired`는 대기 중인 agent task를 완료시키고 나머지는 제거한다. 헤드리스는 host event를 플러그인 event bus로 전달하지 않는다. event bus를 거치지 않는 소비자가 추가되거나 헤드리스 플러그인의 이벤트 구독을 지원하게 되면 종류별 적용 범위를 다시 정한다. 단위 테스트에서 훅 대기 해소와 큐 처리를 검증하더라도, 실제 플러그인이 등록한 hook ID와 전달한 ID가 일치하는지는 별도 종단간 검증 대상이다.

## 두 조합이 다르게 두는 것

헤드리스는 mirror 구조 변경을 forward 큐에 넣지 않는다. `forwarded: false`와 기존 -32603 오류로 응답하며 attach client가 없어 전달할 수 없다는 사유를 설명한다. 같은 구조 메서드는 일반 헤드리스 workspace에서 동작하므로 메서드 미지원 -32017로 바꾸지 않는다. 아직 mirror 생성 경로가 GUI 전용이어도 core 자체에서 거절해 향후 조용한 성공과 큐 누적을 막는다. 헤드리스 attach client를 지원하게 되면 전송과 결과 반영을 구현한 뒤 이 제한을 재검토한다.

헤드리스 workspace는 프로세스 수명 동안만 유지되고 레이아웃을 저장·복원하지 않는다. restore_layout이 켜진 경우 부팅에서 warn으로 알리며 system.info의 layout_slot:null이 슬롯 미점유를 나타낸다. 레거시 layout 이관·scrollback 정리 부팅 훅은 공유 홈의 GUI 데이터를 위해 계속 실행한다. 자동 복원을 도입하면 예상 밖 셸 재기동, ID 변화, 프로세스 간 슬롯 충돌, lazy plugin 복원을 함께 해결해야 한다. 같은 설정을 GUI와 공유하므로 복원 설정 때문에 헤드리스 부팅을 실패시키지 않는다.

관련 문서: [IPC 지원 범위](headless-ipc-surface.md) · [MainViewState 소유권](app-state-ownership.md) · [빌드](build.md)
