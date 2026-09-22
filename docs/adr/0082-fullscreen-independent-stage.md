# ADR-0082: 전체화면은 기존 요소를 확대하지 않고 **독립 무대**로 만든다

- **Status**: Accepted
- **Date**: 2026-08-24
- **Tags**: fullscreen, stage, ui, render-pipeline, layout, webview, attach, screenshot
- **Group**: window-workspace-lifecycle

## Context

tasty 에는 "무언가를 전체화면으로 보여주는" 기능이 **없었다.** winit `set_fullscreen`
호출이 0 건이고(창 상태 조작은 `set_maximized` 뿐), View 안에서 특정 요소가 작업영역을
독점하는 상태(tmux zoom 계열)도 없었다. 가장 가까운 선례는 사이드바 숨김뿐이다.

전체화면을 만드는 방법은 크게 둘이다.

1. **기존 요소 확대** — 브라우저 Fullscreen API 의 `:fullscreen` 리레이아웃처럼, 지목된
   요소가 뷰포트 크기로 다시 배치된다.
2. **독립 무대** — 창 전체를 쓰는 별개의 표면을 띄우고, 뒤의 트리는 손대지 않는다.

1 번을 택하면 화면 rect 를 소비하는 **독립 경로 8 개**가 전부 fullscreen-aware 여야 한다.
그중 3 개는 공통 레이아웃 함수(`AppState::surface_regions`)를 아예 경유하지 않는다:

| # | 채널 | rect 출처 | `surface_regions` 경유 |
|---|------|-----------|------------------------|
| 1 | GPU 터미널 렌더 | `src/gfx/gpu.rs` | O |
| 2 | egui surface(explorer/empty/dag) + 탭바 | `src/adapters/ui/egui_panels.rs` | **X** — `pane_layout().compute_rects` 직접 |
| 3 | egui-mesh plugin(image) | `src/gfx/gpu/egui_mesh_prepare.rs` | O |
| 4 | WebView 네이티브 오버레이(html/markdown) | `src/view/main/redraw.rs` | **X** — pane_rect 자체 계산 |
| 5 | PTY grid resize | `src/core/impl_pty.rs` | **X** — 전 workspace 순회 |
| 6 | popup/toast/banner scope clamp | `src/adapters/ui/layout_context.rs` | O |
| 7 | 마우스 히트테스트·커서 | `src/state/mouse.rs` | O |
| 8 | 방향 focus 이동 | `src/state/focus.rs` | **X** — `compute_rects` |

## Decision

**전체화면은 기존 Workspace/Pane/Tab/Surface 트리와 병렬로 존재하는 독립 무대(stage)로
만든다.** 무대에는 뒤의 tasty 개체와 내부 로직상 연관이 없는 **별개의 데이터**가 들어간다.
"popup 을 전체화면으로" 는 그 popup 을 확대하는 것이 아니라, 무대에 같은 형상의 **별개
인스턴스**를 구성해 보여주는 것이다. 원본은 그대로 남되, 무대가 유지되는 동안 가려져 있으므로
**redraw 하지 않는다.**

이 결정이 위 표의 rect 계산 8 개를 **하나도 건드리지 않게** 한다 — 무대는 자기 rect(창 전체)만
알고 기존 레이아웃 계산은 그대로 있다. 그것이 이 모델을 택한 핵심 근거다.

무대에 올릴 콘텐츠는 popup 의 `PopupDef` / `all_defs()` 와 같은 **프로세스 수명 정적
테이블**(`StageDef` / `fullscreen::defs::all_defs()`)로 등록한다. 호출부가 draw 클로저를
넘기는 방식이 아니다 — 클로저 방식이면 debug IPC 가 가리킬 **id** 가 존재하지 않고,
"선언하지 않은 것은 무대에 올라갈 수 없다" 는 성질도 따라오지 않는다.

**"한 번에 하나" 는 창 단위다.** 무대 상태는 `AppState`(= `MainView` 당 하나)의 `Option`
필드다. 한 창 안에서는 무대가 최대 1 개, 창이 여럿이면 창마다 독립적으로 무대를 가질 수
있다. 프로세스 전역 조정은 필요 없고, 이것이 멀티 모니터(창 N 개로 모니터 N 개를 동시에
전체화면)를 성립시키는 전제다.

브라우저 Fullscreen API 와 대조하면 **top layer 승격 · 뒤 가림 · 뒤 페인트 스킵 · 종료 키를
UA 가 소비** 는 채택하고, **"요소를 뷰포트 크기로 리레이아웃"** 만 기각한 것이다.

## Consequences

- **얻은 것**: rect 소비 경로 8 개를 그대로 둔다. 무대는 자기 rect 만 알면 되므로 새 콘텐츠를
  올리는 비용이 레이아웃 지식과 무관하다. 정적 테이블 덕에 debug IPC 가 id 로 무대를 지정할 수
  있고, popup 의 gallery-first · `on_close` 수명 계약 같은 기존 관례를 그대로 재사용한다.
- **잃은 것**: 무대 콘텐츠는 원본과 **자동으로 동기화되지 않는다.** "이 popup 을 전체화면으로"
  는 같은 형상을 다시 구성하는 작업이고, 원본과 무대가 같은 상태를 봐야 하면 그 공유는 콘텐츠
  쪽이 명시적으로 만들어야 한다. 이는 사용자가 확정한 모델("무대에는 별개 데이터")의 직접적
  귀결이다.
- **"rect 를 안 건드린다" ≠ "아무것도 안 건드린다".** 다음은 rect 와 무관하게 반드시
  fullscreen-aware 여야 한다: WebView **표시 여부**(네이티브 자식 뷰라 안 그려도 화면에 남는다
  → `has_egui_overlay_open` 게이트), 무대 중 PTY grid 동결, 그리고 무대 분기가 **건너뛰면 안
  되는** 세 가지 — offscreen surface 스크린샷 · window 스크린샷 캡처+present · attach mesh
  relay.
- **운영 비용**: 무대 분기의 **위치**가 계약이다. 위아래 어느 쪽으로 밀어도 조용히 죽는 기능이
  있어(아래 표) 코드 주석 + `crates/tasty-doc-guards/tests/fullscreen_stage_render_gate.rs` 구조 가드로 고정했다.

| 분기 위치 | 결과 |
|-----------|------|
| `MainView::render_if_dirty` 조기 반환 | ❌ attach mesh relay 사망 — 로컬 사용자가 전체화면을 켰다고 원격 사용자 화면이 멈춘다(주체 간 비침범 위반) |
| `Gpu::render` 최상단 | ❌ offscreen surface 스크린샷 사망 — `ui.screenshot --surface <id>` 가 영구 대기한다. release 에이전트 기능이라 죽으면 안 된다 |
| 스크린샷 처리 뒤 + 레이아웃/렌더 패스 앞, capture+present 는 유지 | ✅ 유일하게 성립 |

## Alternatives Considered

- **기존 요소 확대(브라우저 `:fullscreen` 모델)**: rect 소비 경로 8 개(그중 3 개는 공통 레이아웃
  함수를 경유조차 안 함)를 전부 fullscreen-aware 로 고쳐야 한다. 한 곳만 빠져도 전체화면 중
  그 채널이 옛 rect 로 그린다. 이득(원본과의 자동 동기화)보다 침습 범위가 압도적으로 크다.
- **무대 상태를 프로세스 전역(엔진 레벨)에 두기**: 창이 여럿일 때 "한 번에 하나" 를 전역으로
  강제하면 모니터마다 창을 전체화면으로 띄우는 시나리오가 불가능해진다. 사용자가 창 단위를
  확정했고, `AppState` 필드 배치 자체가 그 계약이 된다.
- **콘텐츠를 draw 클로저로 받기**: 등록 테이블 없이 호출부가 그리기 함수를 넘기는 방식. debug
  IPC 가 지정할 이름이 없고, "특수 처리 없이 아무거나 무대에 붙는" 형상이 되어 사용자가 요구한
  성질과 반대다.
- **무대 중 `dirty` 억제**("어차피 뒤가 안 보이니 프레임을 아끼자"): relay 전체가 로컬 `dirty`
  프레임에 종속돼 있어 원격 mesh 구독자가 굶는다. 기각.
- **무대 상태 영속화**: 재시작이 전체화면 상태로 부팅되면 사용자가 창을 조작할 수 없는 상태가
  된다. popup 과 같이 휘발성으로 둔다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 무대 콘텐츠가 원본과 **실시간 동기화**되어야 한다는 요구가 생긴다(별개 데이터 모델의 한계가
  실제로 걸리는 시점).
- 한 창 안에서 무대가 둘 이상 겹쳐야 하는 요구가 생긴다 — 그때는 `Option` 이 아니라 popup 처럼
  z-order 를 가진 관리자가 필요하다.
- rect 소비 경로가 공통 레이아웃 함수 하나로 수렴해 "요소 확대" 의 침습 범위가 8 곳에서 1 곳으로
  줄어든다.
- 무대 중 DPI/모니터 전환의 grid 계약(현재: 기본 grid 갱신을 보류했다가 무대를 나온 첫 프레임에
  적용)이 실사용에서 어긋난다.

## References

- [`docs/design/systems/fullscreen-stage.md`](../design/systems/fullscreen-stage.md) — 무대 동작 모델(현재 상태)
- [`docs/identity.md`](../identity.md) — 동시성(주체 간 비침범) · 사용자/에이전트 분리 · headless
- [`docs/architecture/input-layer.md`](../architecture/input-layer.md) — `Order` tier / 미등록 레이어 함정. 무대 레이어 선택 근거
- [`docs/design/systems/popup.md`](../design/systems/popup.md) — 무대 정의 테이블이 대칭으로 따라간 모델
- [ADR-0063](0063-popup-close-hook-single-choke-point.md) — 닫힘 경로 단일 수렴점. 무대 종료도 같은 패턴
- [`docs/design/policies/focus.md`](../design/policies/focus.md) — offscreen 스크린샷이 무대 중에도 살아 있어야 하는 근거
