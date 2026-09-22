# ADR-0055: 마우스 캡처 안내 배너 per-app 억제를 캡처 억제와 독립된 축으로 둔다

- **Status**: Accepted
- **Date**: 2026-07-28
- **Tags**: terminal, mouse, mouse-reporting, banner, settings, ux, adr-0022
- **Group**: terminal-input

## Context

[ADR-0022](0022-shift-rightclick-context-menu-bypass.md) 는 마우스 캡처 트래킹 세션마다 1회
"마우스 캡처 중 — Shift 로 우회 가능" 안내 배너를 띄우기로 결정했고, 그 Reconsideration
Triggers 에 *"Shift+우클릭에 의존하는 트래킹 앱 사용 사례가 실제로 보고되면 ... per-app 예외를
검토한다"* 를 명시해 두었다.

이후 `general.mouse_capture_blacklist`(캡처 자체를 끄는 블랙리스트, foreground 프로세스
이름 패턴 매칭)가 추가됐지만, 이 필드는 **클릭/드래그 캡처를 완전히 끄는** 용도다 — 부수효과로
`effective_click_tracking() != None` 조건이 성립하지 않게 되어 배너도 함께 안 뜨는 것뿐,
"캡처는 유지하되 배너만 끈다"는 별도 축을 표현하지 못한다.

자주 쓰는 트래킹 앱(예: vim)에서는 배너가 트래킹 세션마다 1회 뜨는 게 거슬리지만, 캡처 자체는
계속 앱에 위임하고 싶다는 요구가 있다. 다른 트래킹 앱(예: htop)에서는 안내가 여전히 유용할 수
있으므로 전역 `mouse_capture_hint` OFF 로는 이 요구를 만족할 수 없다(전부 켜짐 또는 전부 꺼짐
뿐).

## Decision

캡처 억제 블랙리스트와 **완전히 독립된** 새 필드 `general.mouse_capture_banner_blacklist`
(기본 빈 벡터)를 추가한다.

- 매칭 규칙은 `mouse_capture_blacklist`(`.exe` 제거·소문자화 후 substring 또는 `*` glob)와
  동일하지만, 매칭 결과로 하는 일이 다르다: 캡처(클릭/드래그 앱 위임)는 그대로 유지하고
  "마우스 캡처 중..." 안내 배너만 `report_left_press_capture`(`src/view/main/mouse.rs`, 좌·우
  클릭 보고 경로가 공유)의 최상단에서 걸러 표시하지 않는다.
- 억제 대상 surface 에서는 `take_mouse_capture_hint()`(armed 플래그 소모)를 아예 호출하지
  않는다 — 캡처 판정(`effective_click_tracking`) 단계에서 걸러지지 않으므로 캡처는 정상 위임된
  채, 배너 판정만 별도로 빠진다. armed 플래그를 소모하지 않아 두는 이유: 같은 트래킹 세션
  도중(예: foreground 가 억제 대상 앱에서 비-억제 앱으로 바뀜) 다음 상호작용에서 배너를 정상적으로
  띄울 수 있어야 하기 때문이다 — 억제 판정 이전에 플래그를 먼저 소모해버리면, 나중에 블랙리스트
  조건이 안 맞게 돼도 같은 세션 안에서는 이미 소모된 플래그 때문에 배너가 영영 못 뜬다.
- 캐시 계산은 기존 `mouse_capture_disabled_surfaces` 와 동일하게 1Hz `refresh_busy_surfaces`
  의 foreground resolve 결과에 편승한다(별도 프로세스 스냅샷 없음) — 새 캐시
  `mouse_capture_banner_suppressed_surfaces`.

## Consequences

- **얻은 것**: 사용자가 특정 트래킹 앱에서만 안내 배너를 끌 수 있다 — 캡처 위임은 그대로 유지한
  채. 전역 ON/OFF 보다 세밀한 제어.
- **잃은 것**: 없음 — 새 필드가 빈 목록(기본값)이면 기존 동작과 완전히 동일(회귀 없음).
- **운영 비용 / 유지 부담**: 마우스 캡처 관련 설정 필드가 3개(`mouse_capture_hint` 전역 토글 /
  `mouse_capture_blacklist` 캡처 억제 / `mouse_capture_banner_blacklist` 배너만 억제)로
  늘어난다. 세 축이 서로 독립적이라는 점을 문서(`docs/features/terminal/index.md`)에 명시해
  혼동을 막는다.
- **불변**: mirror(원격 attach) surface 는 로컬 foreground 프로세스를 resolve 할 수 없어
  (`terminal.process_id()` 가 `None`) 새 억제 리스트가 적용되지 않는다 — 이는 새 기능이 만드는
  제약이 아니라 기존 `mouse_capture_disabled_surfaces` 와 대칭인 기존 제약이다.

## Alternatives Considered

- **`mouse_capture_blacklist` 에 enum 모드 추가**(끄기 vs 배너만 억제) — 필드 하나로 통합하면
  설정 스키마·UI·마이그레이션이 더 복잡해진다(기존 `Vec<String>` 단순 목록이 `Vec<(String,
  Mode)>` 류로 바뀌어야 함). 두 축이 독립적으로 켜고 끌 수 있어야 하는데, 한 프로세스가 두
  블랙리스트 모두에 걸릴 수도 있으므로 별도 필드가 더 단순하다.
- **ADR-0022 본문 개정** — 과거 결정(트래킹 안내 배너 도입 근거)은 그대로 유효하므로 개정 대상이
  아니다. 이번 결정은 그 Reconsideration Trigger 의 발동이자 새로운 결정이라 신규 ADR 이 맞다.
  ADR-0022 의 References 에 본 ADR 을 상호 참조로만 추가한다.

## Reconsideration Triggers

- 배너 억제 리스트와 캡처 억제 리스트를 매번 따로 관리하는 것이 번거롭다는 피드백이 쌓이면,
  통합 UI(예: 앱별 정책 테이블)를 검토한다.
- mirror(원격 attach) surface 에도 배너 억제가 필요하다는 요구가 생기면, 원격 foreground 이름을
  attach 스트림으로 전달하는 별도 메커니즘을 검토한다(현재는 로컬 전용).

## References

- 영향 파일: `crates/tasty-settings/src/general.rs`(`mouse_capture_banner_blacklist` /
  `mouse_capture_banner_disabled_for`), `src/core/state.rs`
  (`mouse_capture_banner_suppressed_surfaces`), `src/core/state/busy.rs`
  (`refresh_busy_surfaces` 계산 + `is_surface_mouse_capture_banner_suppressed`),
  `src/view/main/mouse.rs`(`report_left_press_capture` 억제 체크),
  `src/view/settings/ui/tabs/terminal.rs`(Mouse Capture 탭 두 번째 블랙리스트 블록).
- 관련: [ADR-0022](0022-shift-rightclick-context-menu-bypass.md) — 본 ADR 이 그
  Reconsideration Trigger("per-app 예외 검토")를 해소한다.
- 관련: [ADR-0023](0023-shift-leftclick-selection-bypass.md) — armed 플래그 공유 설계의 배경.
