# ADR-0024: Banner — Modal/Popup/Toast 에 이은 4번째 오버레이 개념(별도 매니저)

- **Status**: Accepted
- **Date**: 2026-06-26
- **Tags**: ui, overlay, banner, popup, toast, ubiquitous-language, user-agent-separation
- **Group**: ui-theme-gallery

## Context

tasty 의 기존 오버레이 UI 는 **Modal / Popup / Toast** 3종이다. 여기에 "parent 상단에 떠서 **안내(info) + 즉시·임시 조치(action)** 를 제공하는 지속·인터랙티브 알림" 이 필요해졌다. 첫 용도는 TUI 가 마우스를 캡쳐(DECSET 1000/1002/1003)해 드래그 선택이 막혔을 때 "왜 막혔는지 + 우회 방법" 을 안내하는 배너다(iTerm 의 동일 UX 참고). 이후 다양한 "지속·인터랙티브 알림" 의 공용 컴포넌트가 된다.

이 요구는 세 축에서 기존 3종 어디에도 들어맞지 않는다.

- **마우스 입력**: 소비(뒤로 전파 X) — Toast(통과)와 충돌.
- **키보드 포커스**: 없음(클릭해도 포커스 이동 X) — Popup(클릭→포커스)과 충돌.
- **내부 인터랙션(버튼)**: 있음 — Toast(본문만)와 충돌.

또한 위치(parent 상단 고정 floating)·수명(사용자 닫기 또는 TTL)·타이틀바/드래그/자유이동 없음 등도 Popup 의 7대 규칙([popup.md](../design/systems/popup.md))·Toast 의 휘발성([toast.md](../design/systems/toast.md))과 어긋난다.

## Decision

**Banner 를 Modal/Popup/Toast 에 이은 4번째 오버레이 개념으로 신설하고, 전용 매니저(`BannerManager`)로 관리한다.** Popup/Toast 의 변종으로 끼워 넣지 않는다. 발화 정책은 기존 오버레이와 동일하게 **사용자 직접 조작 전용**(IPC/에이전트 발화 금지, identity 원칙 1 정합)으로 못 박는다. 종류(kind) 는 범용 분류(Info/Success/…)를 두지 않고 **배너 고유 id 자체가 kind** 다. 이 ADR 시점에는 **개념·동작·정책만 확정** 하고, 시각 토큰(색·치수·그림자·전환)은 claude design 수령 후 보강한다. 동작 모델 정본은 [design/systems/banner.md](../design/systems/banner.md).

## Consequences

- **얻은 것**: 포커스 없음 + 마우스 소비 + 내부 인터랙션이라는 조합을 가진 알림을, Popup/Toast 의 불변식을 깨지 않고 깔끔히 표현. TTL·큐(스코프당 1+최대 5)·계층 z-index/60% 투명 같은 배너 고유 동작을 한 곳(`BannerManager`)에 집중. 발화 정책이 기존 3종과 일관돼 사용자/에이전트 분리가 유지된다.
- **잃은 것**: 오버레이 매니저가 3개 → 4개로 늘어 표면적이 증가한다. 스코프 정의·rect 계산은 `LayoutContext` 를 재사용해 중복을 줄인다.
- **운영 비용 / 유지 부담**: 새 개념이므로 ubiquitous-language·design/systems·index 갱신이 필요(본 작업에서 반영). 시각 토큰은 디자인 의존이라 banner-02/03 에서 별도 보강해야 한다.

## Alternatives Considered

- **A — Popup 에 "포커스 없는 상단 고정" 모드 추가**: 7대 규칙(타이틀바·X·드래그·z-order 승격·자유이동)이 배너와 정면 충돌. 모드 플래그로 절반을 끄면 PopupManager 의 불변식이 약해지고 분기가 폭증한다. 기각.
- **B — Toast 에 버튼·입력 소비·TTL 정지 추가**: Toast 의 핵심 정체성(입력 비소비·휘발·본문만)을 정면으로 뒤집는다. Toast 가 Popup 의 변종이 아니라 별도 매니저로 분리된 것과 같은 이유로, 배너도 별도로 둔다. 기각.
- **C — 범용 kind(Info/Success/Warning/Error) 부여**: 배너는 콘텐츠·액션이 배너마다 달라 범용 심각도 분류가 의미가 약하다. id 자체를 kind 로 두고 심각도 표현은 각 배너 디자인에 위임. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 배너·Toast·Popup 의 동작이 수렴해 매니저 하나로 합쳐도 불변식이 깨지지 않게 되는 경우.
- 배너를 IPC/에이전트로 띄울 정당한 요구가 생겨 발화 정책(사용자 전용)을 바꿔야 하는 경우 — 단 identity 원칙 1 재검토를 동반해야 한다.
- 범용 kind 분류가 실제로 필요해지는(여러 배너가 동일 심각도 시각을 공유해야 하는) 경우.

## References

- [design/systems/banner.md](../design/systems/banner.md) — 배너 동작 모델 정본
- [design/systems/popup.md](../design/systems/popup.md) · [design/systems/toast.md](../design/systems/toast.md) — 대조 오버레이 시스템
- [concepts/ubiquitous-language.md](../concepts/ubiquitous-language.md) — Modal/Popup/Toast/Banner 용어
- [identity.md](../identity.md) 원칙 1 — 사용자/에이전트 행동 분리(발화 정책 근거)
- `crates/tasty-terminal/src/modes.rs` — `mouse_tracking()`(배너 첫 용도의 신호원), [ADR-0022](0022-shift-rightclick-context-menu-bypass.md)·[ADR-0023](0023-shift-leftclick-selection-bypass.md)
