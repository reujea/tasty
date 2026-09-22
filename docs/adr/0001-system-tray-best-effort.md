# ADR-0001: 시스템 트레이 — 전 OS best-effort 지원 (graceful degradation)

- **Status**: Accepted
- **Date**: 2026-06-17
- **Tags**: system-tray, platform, background, cross-platform, windows, macos, linux
- **Group**: window-workspace-lifecycle

## Context

tasty 는 크로스 플랫폼 터미널이다. **GUI 가 붙은 환경에서 마지막 윈도우를 닫거나 백그라운드로 보낼 때, OS 의 트레이/상태 영역으로 들어가 빠르게 복귀**할 수 있어야 한다 — 사용자가 터미널 앱에 기대하는 표준 동선이다. 각 OS 의 상태 영역:

| OS | 상태 영역 | 등록 수단 |
|----|-----------|-----------|
| Windows | 알림 영역(notification area / system tray) | `Shell_NotifyIcon` (이미 구현 — `crates/tasty-platform/src/system_tray.rs`, Show/New/Quit 메뉴) |
| macOS | 메뉴 바(menu bar) 우측의 상태 항목(menu bar extras) | `NSStatusItem` (`NSStatusBar`) |
| Linux | 시스템 트레이/알림 영역 | StatusNotifierItem(SNI) / AppIndicator |

Linux 는 데스크톱 환경(DE)별로 트레이 가용성이 갈린다 — KDE Plasma 는 SNI 네이티브, Ubuntu(GNOME)는 AppIndicator 확장 전제, XFCE/Cinnamon/MATE 는 지원, 미니멀 WM 은 트레이 자체가 없다. **그러나 "모든 환경에서 동일 보장이 불가능" 하다는 것이 "아예 안 한다" 의 근거는 아니다** — 가용한 환경에서 최대한 활용하고, 없는 환경에선 조용히 생략하면 된다.

## Decision

**GUI 환경에서 백그라운드로 갈 때 가능한 모든 OS 에서 트레이로 들어간다 — best-effort.** "최대한 트레이를 활용한다" 가 방향이지 "모든 환경 보장" 이 아니다.

- **Windows**: 알림 영역(구현됨).
- **macOS**: 메뉴 바 상태 항목(`NSStatusItem`).
- **Linux**: SNI/AppIndicator 가 가용한 DE(KDE, AppIndicator 확장이 있는 GNOME/Ubuntu, XFCE 등)에 등록.
- **graceful degradation**: 트레이를 제공하지 않는, 또는 tasty 가 지원하지 않는 환경(미니멀 WM, 트레이 없는 배포판 등)에서는 **트레이 등록을 조용히 생략**한다 — 에러로 취급하지 않는다. 그 경우 백그라운드 동선은 태스크바/도크 최소화(`set_minimized(true)`)로 폴백한다.
- 세 OS 모두 동일 `tray-icon` 크레이트로 통합 가능하다(Windows=Shell_NotifyIcon, macOS=NSStatusItem, Linux=SNI/libappindicator).

## Consequences

- **얻은 것**: 백그라운드 복귀 동선이 OS 표준 트레이로 일관된다. "앱을 닫아도 트레이로 들어간다" 는 사용자 기대에 부합. graceful degradation 이라 트레이 없는 환경에서도 앱은 정상 동작(트레이만 빠짐). **세 OS 모두 구현됨** — `system_tray.rs` 는 `tray-icon` 0.22 로 Windows/macOS/Linux 를 단일 코드 경로로 다룬다.
- **OS 별 백그라운드/복귀 동선 차이**:
  - **Windows / Linux**: 트레이가 있으면 윈도우를 *숨김*(`set_visible(false)`) 해 트레이로 보내고, 트레이 "Show Window" 가 다시 보이게 한다. 트레이가 없으면 태스크바 최소화로 폴백.
  - **macOS**: 기존 백그라운드 모델(윈도우 파기 + state 파킹, dock reopen 시 복원)을 유지한다. 메뉴 바 상태 항목은 dock 과 동일한 *추가* 재진입 경로다. "Show Window" 는 **살아있는 main 윈도우가 있으면 그 창을 focus(앞으로 가져오기)하고, 살아있는 창이 하나도 없을 때(전부 파킹)만** `CreateWindow`(파킹 state 복원)로 새 창을 만든다. 이미 떠 있는 창이 있으면 중복 생성하지 않는다. (2026-06 트레이 재진입 동작 정밀화로 갱신 — 과거엔 "숨겨진 윈도우가 없다" 는 전제로 무조건 `CreateWindow` 로 라우팅했으나, 살아있는 창이 남아 있는 경우 중복 생성 문제가 있어 정정.)
- **잃은 것 / 비용**: Linux 트레이는 `tray-icon`(AppIndicator)이 GTK 초기화·실행 중인 GTK 이벤트 루프를 요구한다 — tasty 는 전용 GTK 메인 루프 대신 winit `about_to_wait` 에서 비차단 `gtk::main_iteration_do(false)` 로 펌프한다. 런타임 의존(`libgtk-3`, `libappindicator3` 또는 `libayatana-appindicator3`, `libxdo`)이 따라온다. Linux 는 DE 별 동작 차이가 있어 best-effort 범위를 정책 문서로 관리한다.
- **운영 비용**: best-effort 범위(지원 DE 매트릭스)와 폴백 동선을 정책 문서가 기술한다.

## Alternatives Considered

- **Linux 트레이 전면 미지원** (이 ADR 의 이전 결정): GNOME 기본 미지원·DE 분열을 이유로 Linux 트레이를 아예 두지 않고 태스크바 최소화로만 해결. — 가용한 DE(KDE·AppIndicator GNOME) 사용자까지 버리게 되어 "최대한 활용" 방향과 어긋난다. **본 ADR 이 이 입장을 대체한다.**
- **모든 환경에서 트레이 필수화**: 트레이가 없는 미니멀 WM 등에서 실패/차단. — graceful degradation 이 맞다. 기각.
- **트레이 없이 태스크바/도크 최소화만**: 백그라운드 표준 동선을 트레이로 통일하려는 방향과 어긋난다. 트레이 불가 환경의 **폴백** 으로만 유지.

## Reconsideration Triggers

- best-effort 범위(어떤 DE/환경까지 지원)가 유지보수 부담이 되면 지원 목록을 좁힌다.
- `tray-icon` 크레이트가 특정 OS 에서 한계를 보이면 OS 별 네이티브 경로(직접 NSStatusItem / SNI 배선)를 재검토한다.

## References

- 코드: `crates/tasty-platform/src/system_tray.rs` (Windows/macOS/Linux 단일 경로, `tray-icon` 0.22), 배선 `src/app/event_handler.rs`(생성·백그라운드·폴링), `src/app/event.rs`(`TrayShowWindow`)
- [`design/policies/system-tray`](../design/policies/system-tray.md) — OS 별 best-effort 동작·DE 매트릭스·폴백 동선 (운영 상세)
- 관련: ADR-0003 (CSD) — 같은 "OS 별 네이티브 표면을 어디까지 직접 다루나" 사고의 연장
</content>
