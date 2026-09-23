# 전체화면 무대 (Fullscreen stage)

- **Status**: Partial — 무대 코어(상태 · 정의 테이블 · 렌더 파이프라인 게이트) + OS 창 전체화면 전환 + 첫 콘텐츠(알림 무대) + 사용자 진입 경로(popup 타이틀바 전체화면 버튼) + 셸 종료 버튼 + 입력 라우팅 게이트 + 설정 가능한 종료 단축키(`fullscreen_stage_exit`, 기본 ESC) + 에이전트 진입/조회(debug 전용 IPC/CLI)까지 있다. release 표면은 없다.
- **주체**: 로컬 사용자
- **ADR**: [ADR-0082](../../adr/0082-fullscreen-independent-stage.md)
- **코드**: `src/adapters/ui/fullscreen.rs` · `src/adapters/ui/fullscreen/defs.rs` · `src/adapters/ui/fullscreen/notifications.rs` · `src/adapters/ui/popup/draw.rs`(진입 버튼) · `src/state.rs` · `src/gfx/gpu.rs` · `src/view/main/redraw.rs` · `src/view/main/keyboard.rs` · `src/view/main/mouse.rs` · `src/view/main/fullscreen_window.rs`(OS 창 전환 + 상태 덤프) · `src/app/ipc/debug_methods.rs`(debug IPC) · `crates/tasty-settings/src/keybindings.rs`(종료 바인딩)
- **화면**: 없음 — 무대 자체가 화면이다. 동작 모델은 [`design/systems/fullscreen-stage.md`](../../design/systems/fullscreen-stage.md)

## 목적

무언가를 창 전체로 크게 보여주기 위한 기반. tasty 에는 이 개념이 아예 없었다(winit
`set_fullscreen` 호출 0 건, View 안에서 요소가 작업영역을 독점하는 상태도 없음). 무대는 기존
레이아웃을 확대하는 대신 **창 전체를 쓰는 독립 표면**을 띄우고 뒤는 손대지 않는다 — 그래서
화면 rect 를 계산하는 기존 경로를 하나도 고치지 않는다(근거는 ADR-0082).

## 내부 동작

- **상태**: `AppState.fullscreen_stage: Option<StageState>` — 창(= `MainView`)당 최대 하나.
  창이 여럿이면 창마다 독립적으로 가질 수 있다. 영속화하지 않는다.
- **등록**: 무대에 올릴 수 있는 것은 `fullscreen::defs::all_defs()` 의 `StageDef` 뿐이다
  (`id` · `title_key` · `draw_fn` · `on_close`). 선언되지 않은 id 로 여는 시도는 거부된다.
  현재 등록된 무대: `blank`(셸 확인용 기준 무대) · `notifications`(알림 무대).
- **셸 chrome**: scrim + 상단 제목 + **종료 버튼**. 종료 버튼은 콘텐츠가 아니라 셸이 그린다 —
  무대 프레임에는 창 닫기 버튼(CSD 타이틀바)조차 없어, 콘텐츠 구현자가 빠뜨리면 창을 빠져나갈
  수 없는 상태가 되기 때문이다. 콘텐츠는 제목 띠 아래로 잘린 child `Ui` 만 받는다.
- **알림 무대**: popup 의 형상 함수(`notification::draw_notification_content_inner`)를 그대로
  호출한다 — popup 인스턴스가 아니라 같은 형상의 **별개 콘텐츠**이고, 목록 스크롤 위치는 무대
  자신의 것이라 popup 쪽과 섞이지 않는다(무대 종료 시 `on_close` 가 지운다).
- **진입/종료**: `AppState::open_fullscreen_stage(id)` / `close_fullscreen_stage()`. 다른 무대가
  올라와 있으면 그 무대를 닫고 교체하며, 같은 id 재진입은 no-op 이다. 종료는 **모든 닫힘
  경로가 지나는 유일한 지점**이고, 닫힌 id 가 훅 대기열을 거쳐 `on_close` 를 정확히 1 회
  발화시킨다.
- **렌더**: 무대가 켜져 있으면 프레임이 통째로 갈린다 — clear + 무대 콘텐츠만 그리고, 터미널
  글리프 · egui-mesh · attach mesh 합성 · host chrome(사이드바/탭바/상태바/popup/오버레이)은
  그리지 않는다.
- **무대 중에도 계속 도는 것**: PTY 출력 처리(스크롤백이 계속 쌓인다) · attach mesh relay ·
  offscreen surface 스크린샷 · window 스크린샷.
- **무대 중 동결되는 것**: 터미널 grid(cols/rows). 창 크기가 바뀌어도 진입 시점 값을 유지한다
  — 그래야 나올 때 원본이 리플로우되지 않는다. DPI 변경에 따른 신규 터미널 기본 grid 갱신은
  보류했다가 무대를 나온 첫 프레임에 1 회 적용한다.
- **WebView**: 네이티브 자식 뷰라 "안 그린다" 로는 사라지지 않는다. 무대는
  `has_egui_overlay_open()` 에 참여해 popup 과 같은 게이트로 `set_visible(false)` 를 받는다.
- **입력**: 무대는 입력 계층의 새 최상위 단이다 — 활성이면 뒤 세계의 키보드/마우스가 전부
  막힌다(좌표 판정 없이). ESC 는 무대만 닫고 **뒤로 전파되지 않는다**(settings 모달·
  notifications 팝업이 함께 열려 있어도 그것들은 닫히지 않는다). 무대 콘텐츠는 egui 위젯이라
  키/IME/클릭을 정상적으로 받는다. 진입 시 진행 중이던 IME 조합·드래그·네이티브 메뉴·파일
  드래그는 확정하지 않고 폐기한다. 계약 전체는
  [`design/systems/fullscreen-stage.md` § 입력 계약](../../design/systems/fullscreen-stage.md#입력-계약).
- **종료 키**: `KeybindingSettings.fullscreen_stage_exit`(4 프리셋 공통 기본값 `escape`). 조회는
  0단계 무대 게이트 안에서만 일어나므로 무대가 없으면 이 바인딩은 아예 매칭되지 않는다 —
  평상시 ESC 동작을 가져가지 않는 것이 이 위치의 목적이다. 사용자가 다른 키로 바꾸면 그 키로
  닫히고 ESC 로는 닫히지 않는다. 빈 값이면 키보드 종료 수단만 사라지고 셸의 종료 버튼이
  남는다(셸이 항상 그리므로 탈출 불가 상태가 되지 않는다). 프리셋 표는
  [`features/keybindings/index.md`](../keybindings/index.md#프리셋-바인딩--네-프리셋의-기본값).

## 인터페이스

- **AI Agent (IPC/CLI)**: **debug 빌드 전용**. 무대는 화면 투영이고 진입은 사용자 조작이라
  release 표면을 두지 않는다(불가침 원칙 1) — 대신 자기검증용으로
  `debug.fullscreen.{list,open,close,state}` 4 종이 `#[cfg(debug_assertions)]` 로 격리되어 있다
  (CLI: `tasty debug fullscreen {list,open,close,state}`). release 바이너리에서는 아예 없어
  `method_not_found` 로 떨어진다. 전부 `local_only` 라 plugin caller 는 호출할 수 없다.
  대상 창은 `window_id` 로 지목하고, 미지정 시 창이 하나면 폴백·여럿이면 에러다(포커스된 창으로
  조용히 폴백하지 않는다). 파라미터·응답 상세는
  [`dev-guide/debug-ipc.md`](../../dev-guide/debug-ipc.md).
- **사용자 트리거**: 무대를 선언한 popup(`PopupDef.fullscreen_stage`)의 타이틀바 전체화면
  버튼 — 현재 알림 popup. 누르면 무대가 뜨고 **원본 popup 은 열린 채 남는다**. 종료는 무대
  우측 상단의 종료 버튼 또는 `fullscreen_stage_exit` 단축키(기본 ESC). 진입 단축키는 아직
  없다 — 무대를 여는 것은 popup 타이틀바 버튼뿐이다.
- **설정 UI**: 설정 > 단축키 > 일반에 "전체화면 무대 종료" 엔트리로 노출된다. 발견성 때문에
  노출한다 — 무대에는 메뉴도 CSD 타이틀바도 없어 종료 버튼 외에는 이 엔트리가 종료 수단을
  알 수 있는 유일한 자리다. 다만 녹화 버튼에서 ESC 는 "슬롯 비우기" 로 예약돼 있어 ESC 를
  다시 지정할 수는 없다(엔트리 툴팁에 명시).

## 아직 없는 것

이 기능 자체에 남은 미구현 항목은 없다 — 종료 키의 설정화와 에이전트(debug IPC) 진입이
모두 붙었다. release 에이전트 표면을 두지 않는 것은 미구현이 아니라 확정된 경계다(무대는
화면 투영이라 대응 도메인이 없다).

다만 무대 **위에 무언가를 얹을 때** 전제해야 하는 경계(무대 프레임에는 CSD 타이틀바조차
없다 등)는 그대로다 —
[`design/systems/fullscreen-stage.md`](../../design/systems/fullscreen-stage.md) 의
"아직 없는 것" 절.

## OS 창 전환

무대가 서면 **창 자체가 모니터를 덮는다** — 무대의 경계는 작업영역이 아니라 OS 창까지다
(브라우저 Fullscreen API 와 같은 모델: 새 창을 만들지 않고 같은 창을 전환한다). 창이 있는
그 모니터를 덮으며 primary 로 튀지 않는다. 종료하면 진입 직전 창 상태로 되돌아간다 —
maximize 였으면 maximize 로, **사용자가 직접 만든 전체화면이었으면 그대로 유지한다**(무대는
자기가 만든 전환만 되돌린다). 뒤 터미널의 grid 는 두 전환 모두에서 불변이다.

동작 모델·플랫폼별 확인 결과(Wayland/Windows/macOS/멀티 모니터는 **미확인**)는
[`design/systems/fullscreen-stage.md` §OS 창 전환](../../design/systems/fullscreen-stage.md#os-창-전환).

## 비-목표 (Out of scope)

- 기존 요소(pane/tab/surface)를 뷰포트 크기로 리레이아웃하는 것 — 무대는 확대가 아니다.
- 무대 콘텐츠와 원본의 자동 동기화 — 무대에는 별개 데이터가 들어간다.
- 영속화 — 재시작이 전체화면 상태로 부팅되지 않는다.
- headless 대응 — 화면 투영이라 대응 도메인이 없다.

## Acceptance Criteria

단위 테스트로 상시 검증되는 것:

- Given 정의 테이블에 없는 id When 무대 진입 Then 거부되고 무대가 서지 않는다
- Given 무대 활성 When `has_egui_overlay_open()` 조회 Then true (WebView 숨김 게이트)
- Given 무대 종료 When 다음 프레임 Then 닫힘 훅이 정확히 1 회 발화한다(무대/일반 프레임 양쪽)
- Given 무대 A 활성 When 무대 B 진입 Then A 의 닫힘 훅이 1 회 발화하고 B 만 남는다
- Given 진입 시점 창 상태 When 종료 시 복원 동작 결정 Then 일반/maximize 는 되돌리고 사용자 fullscreen 은 유지한다
- Given fullscreen 단독 When 리사이즈 엣지 판정 Then 잠긴 것으로 본다(maximize 만 보지 않는다)
- Given 무대를 선언한 popup When 타이틀바 전체화면 버튼 클릭 Then 그 무대가 뜨고 popup 은
      열린 채 남는다
- Given 무대를 선언하지 않은 popup When 같은 좌표 클릭 Then 아무 무대도 뜨지 않는다
- Given 알림 무대 활성 When 무대 종료 Then 무대 자체 상태(목록 스크롤)가 지워진다
- Given popup 이 선언한 무대 id When 무대 테이블 조회 Then 반드시 존재하고, 그 popup 은
      headless 가 아니다

**미검증 — 살아 있는 GUI 가 필요해 자동 회귀가 없다.** 아래는 실제 창·PTY·GPU 가 있어야
재현되므로 headless 테스트로 고정할 수 없다. 다만 `debug.fullscreen.*` 로 진입/조회가
상시화되어 실측 재현 자체는 언제든 가능하며, 표시된 항목은 실측으로 1 회 확인했다(회귀를
잡지는 못한다).

- Given 무대 활성 When PTY 로 출력 발생 Then 무대를 나온 뒤 그 출력이 스크롤백에 있다
- Given 무대 진입 전후 When 터미널 grid 조회 Then cols/rows 가 동일하다(무대 중 창
      크기가 바뀌어도)
- Given 무대 활성 When `ui.screenshot`(window) Then 응답하고 결과에 무대만 찍힌다
      <!-- 실측 확인: debug.fullscreen.open 후 window 캡처에 무대 셸(제목 띠 + 종료 버튼)과
           콘텐츠만 있고 사이드바/탭바/상태바가 없음 -->
- Given 무대 활성 When `ui.screenshot --surface <id>` Then 응답하고 그 surface 의 터미널
      내용이 찍힌다
- Given 일반 창에서 무대 진입 When 무대 종료 Then 진입 전 크기·위치의 일반 창으로 복귀
      <!-- 실측 확인: debug.fullscreen.state 의 inner_size 가 1280x720 → 1920x1080 →
           1280x720, os_fullscreen 이 false → true → false -->
- Given maximize 창에서 무대 진입 When 무대 종료 Then maximize 로 복귀
- Given 이미 OS fullscreen 인 창에서 무대 진입 When 무대 종료 Then fullscreen 유지
- Given 창 2 개가 각각 무대 활성 When 한쪽만 종료 Then 다른 쪽 전체화면 유지
      <!-- 실측 확인: 창 A=blank / B=notifications 로 각각 진입 후 A 만 close →
           A 는 stage_id null·os_fullscreen false, B 는 notifications·true 유지 -->
- Given 무대 활성 + notifications 팝업 열림 When ESC Then 무대만 닫히고 팝업은 남는다
      <!-- 실측 확인: X11 + xdotool 로 winit 키 경로 재현 — ESC 후 창이 무대 크기에서
           원래 크기로 돌아오고 `ui.state` 의 notification_panel_open 은 true 유지 -->
- Given `fullscreen_stage_exit` 를 다른 키로 변경 When 무대 활성 + ESC Then 닫히지 않고,
      그 새 키를 주입하면 닫힌다
      <!-- 실측 확인: debug.settings.apply 로 ctrl+alt+q 재바인딩 후 ESC 는 무대 유지,
           ctrl+alt+q 로 종료. 빈 vec 이면 둘 다 무효고 셸 종료 버튼으로만 닫힘 -->
- Given 무대 활성 When 문자 키 주입 Then 뒤 터미널에 그 문자가 들어가지 않는다
- Given 무대 활성 When 등록 단축키(사이드바 토글 등) 주입 Then 발화하지 않는다
- Given 무대 활성 When 뒤 surface 좌표 클릭 주입 Then 포커스가 이동하지 않는다
- Given 무대 활성 When `debug.pending_menu` 조회 Then 대기 중 네이티브 메뉴가 없다

**수동 확인 대상** — IPC 로 재현할 수 없다:

- IME 조합 중 무대 진입 시 조합 문자가 PTY 로 새지 않음(실제 IME 입력 필요)
- OS 인터랙티브 스크린샷 선택 UI 가 무대 중 뜨지 않음(OS 레벨 UI)
- 네이티브 파일 드래그가 무대 중 시작되지 않음(OS 레벨 드래그 세션)
