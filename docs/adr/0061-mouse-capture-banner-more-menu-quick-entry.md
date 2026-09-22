# ADR-0061: 마우스 캡처 배너에 "더보기"(⋯) 퀵 엔트리를 추가해 per-app 블랙리스트 진입 경로를 배너 자신으로 확장한다

- **Status**: Accepted
- **Date**: 2026-08-06
- **Tags**: terminal, mouse, mouse-reporting, banner, popup, settings, ux, i18n, gallery, adr-0024, adr-0055
- **Group**: terminal-input

## Context

[ADR-0055](0055-mouse-capture-banner-suppress-list.md) 는 마우스 캡처 안내 배너를 per-app 으로
억제하는 두 축(`mouse_capture_blacklist` / `mouse_capture_banner_blacklist`) 을 결정하며,
그 조작 경로를 **Settings › Terminal › Mouse Capture 탭에서 프로그램 이름을 직접 타이핑해
추가하는 것 하나**로 한정했다("per-app 제어는 Settings 목록으로만").

이 경로는 발견성이 낮다 — 사용자가 특정 TUI(예: vim)에서 배너가 거슬리거나 캡처를 끄고 싶을 때,
그 배너를 보고 있는 그 순간에는 Settings 화면 어디에 그 옵션이 있는지 알 방법이 없고, 별도로
Settings 를 열어 프로그램 이름을 정확히 타이핑해야 한다. 배너 자체는 이미 action 버튼을 가질 수
있는 개념([ADR-0024](0024-banner-fourth-overlay-concept.md))이었지만, 실제로는 안내 텍스트만
있었을 뿐 이 액션을 실을 진입점이 없었다.

이 진입점은 애초에 계획에 있었으나, 우선 기본 기능(안내 배너 + Settings 블랙리스트)부터 구현하고
후속으로 미뤄둔 것이었다(구두 결정 — 당시 별도 ADR 로 남기지 않음). 본 ADR 이 그 후속 작업의
근거를 명시적으로 기록한다.

## Decision

마우스 캡처 배너 우상단에 "더보기"(⋯) 트리거를 추가하고, 클릭 시 headless 컨텍스트 메뉴로
두 액션(배너만 끄기 / 캡처 비활성화)을 바로 실행할 수 있게 한다 — **단, 이것은 ADR-0055 가 정한
"블랙리스트는 두 필드, 매칭은 substring/glob" 축을 바꾸는 게 아니라, 그 축에 진입하는 경로를
Settings 하나에서 "Settings + 배너 퀵 엔트리" 둘로 넓히는 것**이다. 저장/매칭 로직은 그대로
`settings.general.mouse_capture_banner_blacklist` / `mouse_capture_blacklist` 를 push 하고
`matches_blacklist()` 가 읽는다 — 새 저장 축은 만들지 않는다.

구현 요지:

- ⋯ 트리거는 hover 시(× 와 동일 조건) 노출되고, ⋯ 왼쪽에 ×, 4px gap 으로 나란히 놓인다. 메뉴가
  열려 있는 동안은 hover 여부와 무관하게 계속 표시 + active 강조를 유지한다(재사용은 ⋯ 재클릭).
- 메뉴는 host `PopupDef` 의 `headless: true` 컨텍스트 메뉴 스타일로 만든다
  (`docs/dev-guide/popup-implementation.md`) — `egui::Window` 를 직접 쓰지 않는다. 위치는
  `OpenPopupMode::AtFocused` 로 트리거 버튼 아래 4px, 우측 정렬(뷰포트 하단 공간이 없으면 위로
  flip).
- 팝업 대상(어느 surface 의 배너인지)은 기존 rename 팝업의 `RenameTarget` 패턴과 동일하게
  `AppState.dialogs` 에 타깃 필드(`mouse_capture_banner_menu_target: Option<u32>`)를 두고
  전달한다 — `PopupDef.draw_fn` 시그니처엔 대상 정보가 없기 때문이다.
- 메뉴 항목 라벨은 고정 텍스트 + 프로그램 이름(mono, 별도 세그먼트) 두 조각으로 렌더한다 — 한
  문자열로 합쳐 ellipsis 하면 로케일에 따라(특히 en) 프로그램 이름부터 잘리는 문제를 막기 위해서다.
- "이 알림 끄기"는 배너도 즉시 함께 닫는다. "마우스 캡처 비활성화"는 배너를 남긴다(캡처가 이미
  풀렸다는 걸 사용자가 읽고 직접 닫도록). 두 액션 모두 danger 톤이 아니다 — 파괴/유실이 없고
  Settings 에서 되돌릴 수 있다.

## Consequences

- **얻은 것**: 배너를 보고 있는 바로 그 자리에서 per-app 억제를 1클릭 조작으로 실행할 수 있다.
  Settings 경로는 그대로 남아 있어(두 경로가 같은 데이터를 공유) 일괄 관리·조회 용도로 계속
  쓸 수 있다.
- **잃은 것**: 없음 — 배너에 새 UI 표면(⋯ + 메뉴)이 추가되지만 저장 스키마·매칭 로직·Settings
  UI 는 변경되지 않는다(회귀 없음).
- **운영 비용 / 유지 부담**: `BannerManager::draw()`(상태 관리, popup 시스템을 모름)와
  `notification::draw_popups()`(popup 시스템 접근 가능) 사이에 "더보기 클릭 요청"을 실어 나르는
  작은 프로토콜(`BannerDrawResult::more_clicked` + `more_menu_open_for` 파라미터)이 생겼다 —
  향후 다른 배너 kind 에 유사한 action 트리거를 추가하면 이 프로토콜을 재사용할 수 있다.

## Alternatives Considered

- **content_fn 내부에 버튼을 넣는다** — `BannerContentFn`(`fn(&mut egui::Ui, &Theme)`) 시그니처엔
  surface_id 가 없어 "어느 surface 의 배너인지" content_fn 내부에서 알 수 없다. 우상단 affordance
  는 `BannerManager::draw()` 콜사이트가 이미 `slot.scope` 를 알고 있으므로 그 자리에 추가하는 게
  구조적으로 맞다.
- **Settings 목록 UI 만 개선(검색/하이라이트)** — 발견성 문제의 근본(그 순간 배너를 보고 있다는
  맥락)을 해결하지 못한다. 진입점 자체를 배너로 옮기는 것이 문제를 직접 해소한다.
- **danger 톤 적용** — 두 액션 모두 파괴적이지 않고(Settings 블랙리스트에서 제거하면 되돌릴 수
  있음) "마우스를 tasty 로 되찾는" 긍정적 동작에 가까워 danger 로 표시하면 오히려 오해를 준다.

## Reconsideration Triggers

- 배너 외 다른 곳(예: 탭바, StatusBar)에도 같은 per-app 퀵 엔트리가 필요하다는 요구가 쌓이면,
  이 ADR 의 "더보기 트리거 + headless 메뉴" 패턴을 공용 컴포넌트로 승격하는 것을 검토한다.
- mouse-capture 이외의 배너 kind 에도 "더보기" 액션이 필요해지면, 현재 `slot.id ==
  BANNER_MOUSE_CAPTURE` 로 하드코딩된 게이팅을 `BannerDef` 필드(예:
  `more_menu: Option<PopupId>`)로 일반화하는 것을 검토한다.

## References

- 영향 파일: `src/adapters/ui/banner.rs`(`BannerManager::draw()` 우상단 affordance,
  `BannerDrawResult::more_clicked`), `src/adapters/ui/mouse_capture_menu.rs`(headless 메뉴
  draw_fn + 액션 함수), `src/adapters/ui/popup/defs.rs`(`mouse_capture_banner_menu` PopupDef),
  `src/adapters/ui/overlay.rs`(`draw_overlays` 의 배너↔popup 조립 지점), `src/state/dialogs.rs`
  (`DialogState::mouse_capture_banner_menu_target`), `crates/tasty-icons/src/lib.rs`(`MORE`
  아이콘), `lang/{ko,en,ja}.toml`(`banner.mouse_capture.more_button` /
  `popup.mouse_capture_banner_menu.*`), `crates/tasty-gallery/src/catalog/widgets/banner.rs`
  (`draw_more_menu` specimen).
- [ADR-0024](0024-banner-fourth-overlay-concept.md) — 배너가 action 버튼을 가질 수 있다는
  개념적 근거.
- [ADR-0055](0055-mouse-capture-banner-suppress-list.md) — per-app 억제 두 축의 저장·매칭
  로직 정본(본 ADR 은 이 축을 바꾸지 않고 진입 경로만 넓힌다).
- `docs/dev-guide/popup-implementation.md` — `PopupDef` 시스템, headless 컨텍스트 메뉴 패턴.
