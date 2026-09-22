# ADR-0003: 네이티브 윈도우 데코레이션 대신 CSD(Client-Side Decorations) 채택

- **Status**: Accepted
- **Date**: 2026-06-15
- **Tags**: window, csd, titlebar, cross-platform, winit, macos, windows, linux
- **Group**: window-workspace-lifecycle

## Context

tasty 는 전환 전 3 OS 모두 네이티브 데코레이션을 썼다 — `WindowAttributes::default()`
(`with_decorations(false)` 호출 0건). 윈도우 상단 타이틀바는 OS 가 소유했고, tasty 의 탭은
pane 단위 탭바로 콘텐츠 영역 안에 있었다.

CSD 의 동기:

- 타이틀바 영역을 tasty 가 소유해야 향후 탭/상태를 윈도우 크롬에 통합해 **세로 공간을
  절약**할 수 있다(네이티브 타이틀바 + 별도 탭바의 이중 높이 제거 여지).
- 3 OS 에서 **일관된 시각·테마**(active/inactive 디밍, 색 토큰, 보더)를 타이틀바까지 적용.
- AI 코딩 에이전트용 터미널로서 크롬을 제품이 제어할 수 있어야 한다.

제약: winit 0.30(+ winit-ime-fix 포크) 의 OS별 API 범위 안에서 데코를 끄고 자체 크롬을
그려야 하며, 원칙 1(사용자/에이전트 분리)에 따라 타이틀바 조작은 사용자 입력으로만 노출하고
release IPC/CLI 에 노출하지 않는다.

## Decision

**네이티브 데코레이션을 끄고 tasty 가 CSD 타이틀바를 직접 그린다.** OS별 전략은 다르다:

- **macOS**: 데코를 완전히 끄지 않는다. `titlebar_transparent` + `fullsize_content_view` +
  `title_hidden` 으로 콘텐츠를 y=0 까지 확장하되 **네이티브 신호등(standardWindowButton)을
  유지**한다. 신호등의 클릭·hover·풀스크린·접근성·디밍은 OS 가 처리하고, tasty 는 드래그/
  더블클릭과 신호등 폭 carve-out 만 담당한다.
- **Windows**: `with_decorations(false)` + `with_undecorated_shadow(true)`. tasty 가 우측
  캡션 버튼(min/max/restore/close)을 직접 그린다.
- **Linux**: `with_decorations(false)`. tasty 가 DE 가변 버튼(데이터 드리븐)을 직접 그린다.

데코 없는 Windows/Linux 창의 **가장자리 리사이즈는 단일 MainView 경로**로 통일한다 —
`handle_mouse_input` 이 좌클릭 press 에서 `egui_consumed == false`(콘텐츠가 포인터를
원하지 않음) AND 가장자리 margin(`resize_direction_at`) AND 비최대화 AND 오버레이/팝업/
배너 없음일 때만 `drag_resize_window` 를 호출한다. 이전의 Windows egui 오버레이(`Order::Foreground`
흡수 레이어 + carve-out)와 Linux pre-interception 은 제거됐다. 모든 egui 인터랙티브
콘텐츠가 자동으로 입력 우선권을 가지므로(콘텐츠 우선 입력모델) 가장자리 콘텐츠마다
carve-out 을 뚫어야 하던 O(n) 부채가 종결된다. macOS 는 데코 있는 창이라 이 경로를 타지
않고(cfg 가드) OS 네이티브 보더가 리사이즈를 처리한다.

공통 타이틀바(36px, full-width, 드래그/더블클릭→maximize)는 `src/adapters/ui/titlebar/`
의 단일 어댑터가 그리고, `top_inset` 으로 사이드바·터미널 영역을 밀어낸다.

## Consequences

- **얻은 것**: 타이틀바 영역 소유 → 향후 탭-in-크롬 통합 여지, 3 OS 일관 테마, 제품이 크롬
  제어. 신호등은 OS 에 위임(macOS)해 접근성·표준 동작 무료.
- **잃은 것**: OS 가 주던 리사이즈 보더/캡션/Snap 을 tasty 가 직접 재현해야 한다(Windows/
  Linux). 플랫폼별 코드 증가(`#[cfg(target_os=...)]` 분기).
- **운영 비용 / 유지 부담**: 새 OS API(winit 0.30) 의존. macOS 외 OS 는 실기 검증이 별도
  필요(현 개발 OS = macOS). 디자인 시스템에 Windows/Linux 타이틀바 정식 jsx 가 없어
  현재 구현은 P1 토큰 + OS 관습 기반 → 디자인 보강 후 미세 조정 여지.

## Alternatives Considered

- **네이티브 데코 유지**: OS 타이틀바를 그대로 두고 탭바만 콘텐츠 안에 유지. — 타이틀바를
  제품이 못 만져 탭-in-크롬 공간 절약·일관 테마가 불가. CSD 동기 자체를 포기하게 됨.
- **macOS 도 신호등 직접 그리기**: 디자인 jsx 처럼 tasty 가 신호등을 커스텀 렌더(group-hover
  글리프, 색 토큰). — 시각 완전 제어를 얻지만 클릭 동작·접근성·풀스크린·디밍을 직접
  구현해야 하고 macOS 버전/노치 영향에 취약. CSD 목적은 "신호등 재구현"이 아니라 "크롬
  공간 소유"라 네이티브 유지를 택함.
- **탭을 타이틀바에 즉시 통합(변형 B)**: tasty 탭은 pane 하위인데 디자인 탭스트립은 window
  단위 가정 → multi-pane split 에서 모델 불일치. 고비용·아키텍처 영향이라 별도 후속 트랙으로
  분리(현재는 타이틀바=제목/컨트롤만, 탭은 기존 pane 탭바 유지 = 변형 A).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- winit 이 Windows `WM_NCHITTEST` 후킹(NCHITTEST LRESULT 변경)을 표준 API 로 노출 →
  **Snap Layouts(`HTMAXBUTTON`)** 를 raw HWND 서브클래싱 없이 구현 가능해질 때.
- winit Wayland CSD(SCTK) 가 둥근 모서리/그림자/리사이즈 엣지를 자동 처리하게 되어 Linux
  프레이밍을 직접 그릴 필요가 사라질 때.
- 탭-in-크롬 통합(변형 B)을 제품 결정으로 채택해 타이틀바 모델을 재정의할 때.
- 네이티브 데코 유지가 더 낫다고 판단될 만큼 CSD 유지 비용(플랫폼 분기·검증)이 커질 때.

## References

- [features/window-chrome](../features/window-chrome/index.md) — CSD 타이틀바 현재 동작
- [`design/systems/theme.md`](../design/systems/theme.md) — CSD 타이틀바 토큰
- [`design/policies/key-mapping.md`](../design/policies/key-mapping.md) · [`design/policies/focus.md`](../design/policies/focus.md)
- 구현: `crates/tasty-platform/src/window_chrome.rs`, `src/adapters/ui/titlebar/`
