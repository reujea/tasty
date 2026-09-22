# ADR-0273: plugin popup 은 소속 범위의 종류만 선언하고 대상은 host 가 바인딩한다

- **Status**: Accepted
- **Date**: 2026-09-14
- **Tags**: popup, plugin, scope, manifest
- **Group**: ui-theme-gallery

## Context

host popup(`PopupDef`)은 `PopupScope` 로 소속 범위를 갖고, `PopupManager::draw` 가
`LayoutContext` 로 그 범위의 **가시성 필터**와 **경계 clamp** 를 정한다
([design/systems/popup.md](../design/systems/popup.md) §스코프). 대상을 런타임에야 아는
범위는 여는 쪽이 `OpenPopupMode::WithScope` 로 주입한다(`convert_surface` · `search_bar`).

plugin popup(`[[contributes.popup]]`)에는 그 축이 없었다. 위치 관련 필드는 `anchor` 하나였고,
`anchor` 는 위치만 정할 뿐 가시성도 경계도 정하지 않는다. `draw_plugin_popups` 는
`LayoutContext` 를 받지 않고 열린 인스턴스를 전부 그리며 화면 전체에 clamp 했다. 그래서
markdown `file-open` 처럼 대상 surface 가 분명한 popup 도 워크스페이스·탭을 옮겨도 계속
떠 있었고, 위치도 그 surface 가 아니라 화면 가운데였다. `PopupAnchor::ActiveSurfaceCenter` 는
화면 가운데로 폴백하는 미구현 갈래였다.

"plugin popup 은 창 범위로 둔다" 는 결정 기록은 없었다 — 의도된 제한이 아니라 빠진 축이었다.
같은 계열인 banner(`BannerContribute.scope`)는 이미 범위를 선언한다.

남은 물음은 넷이었다: 매니페스트에 무엇을 노출하나, 대상 surface 를 누가 정하나, 대상이
없는 진입점은 어떻게 되나, 기존 매니페스트는 어떻게 읽히나.

## Decision

`PopupContribute` 에 `scope: PopupScopeDecl` 을 더한다. 값은 `window`(기본)와 `surface`
둘이다. 선언은 **종류**만 정하고, 대상 surface 는 **popup 을 여는 host 진입점**이 인스턴스에
바인딩한다(`PendingPopupOpen.target_surface` → `PluginManager::bind_popup_instance_surface` →
`PopupInstance.scope_surface`). 렌더는 선언과 바인딩으로 `PopupScope` 를 만들고
(`surface` + 바인딩 있음 → `Surface(id)`, 그 밖 → `Window`), host popup 과 **같은 판정 함수**
(`PopupManager::is_scope_visible` · `scope_rect`)와 같은 frame 의 `LayoutContext` 로 가시성·
경계를 정한다. 범위가 안 보이는 인스턴스는 그 frame 의 셸·합성 영역·히트테스트 rect·Esc·
outside-click·키 게이트 어디에도 들어가지 않는다. 인스턴스와 forward 추적은 남아 범위가
다시 보이면 상태째 복원된다. 앵커의 가운데 정렬 기준은 화면이 아니라 범위 경계다.

진입점별 대상: 변환 입력 popup(`AppState::enqueue_convert_input_popup`)은 제자리 변환이면
그 surface, 새 탭이면 여는 시점의 focus surface 다 — plugin 에 넘기는 cwd 의 기준(origin)과
같은 값이다. 도구 메뉴 popup 은 focus surface 를 싣는다. plugin 이 IPC·이벤트로 스스로 연
popup 은 바인딩이 없어 `window` 로 뜬다. 하위 호환은 serde 기본값이다 — `scope` 가 없는
매니페스트는 `window` 로 읽혀 이전 동작과 같다.

markdown `file-open` 은 `scope = "surface"` 를 선언한다.

## Consequences

- **얻은 것**: plugin popup 이 host popup 과 같은 범위 계약을 따른다. surface 범위 popup 은
  그 surface 가 안 보이면 그려지지 않고, 보이지 않는 rect 가 클릭을 삼키지 않으며, 다른 창의
  layout 에는 그 surface 가 없으므로 다른 창에도 그려지지 않는다.
- **잃은 것**: plugin 이 대상 surface 를 스스로 지목할 수 없다. plugin 이 연 popup 은 선언이
  `surface` 여도 창 범위다.
- **운영 비용 / 유지 부담**: 대상이 plugin context(`surface_id` 등)와 host 바인딩 두 자리에
  실린다. 둘을 같은 origin 에서 만들어야 갈리지 않는다. 단일 인스턴스 가드로 기존 인스턴스가
  재사용되면 plugin 은 첫 context 를 계속 쓰므로, 바인딩도 첫 값을 유지한다(덮지 않는다).
  그 결과 surface A 에 묶여 숨은 popup 을 surface B 에서 다시 열면 아무것도 안 보인다 — 가드의
  기존 정책이 범위와 만나 드러나는 형태다.

## Alternatives Considered

- **`anchor` 를 확장한다** — anchor 는 위치다. 가시성과 경계를 anchor 값에 싣으면 한 필드가 두
  물음에 답해 둘을 따로 고를 수 없다.
- **host 가 plugin context 의 `surface_id` 를 읽어 범위로 승격한다** — host 가 plugin context 의
  키 이름을 해석하게 된다(host 는 plugin 도메인을 모른다). 새 탭 경로는 `surface_id` 가 없어
  어차피 별도 규칙이 필요하다.
- **plugin 이 open 시 대상 surface 를 명시한다** — 남의 surface 를 지목하는 길이 열린다. banner 는
  소유 검증으로 막지만, popup 을 여는 주된 경로가 이미 host 진입점이라 그 검증을 새로 세울
  이유가 없다.
- **pane · tab · workspace 도 노출한다** — 소비자가 없다. 필요해지면 enum 에 갈래를 더한다.

## Reconsideration Triggers

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `PopupScopeDecl` 에 `surface` 외의 대상 있는 갈래가 필요한 번들 plugin popup 이 생긴다.
- plugin 이 연 popup(`trigger = { kind = "ipc" }` · event)이 `scope = "surface"` 를 선언한다 —
  그 선언은 지금 효과가 없으므로, 그때 plugin 지정 대상과 소유 검증을 다시 연다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- surface 범위 popup 이 native WebView overlay(markdown · html surface)와 겹쳐 가려진다. 재는 법:
  markdown surface 에서 파일 열기 popup 을 띄우고 OS 화면 캡처로 본다(`ui.screenshot` 에는
  WebView 가 담기지 않는다 — [ai-verification/screenshot-methods](../ai-verification/screenshot-methods.md)).
- ~~plugin popup 이 창 전체에 깔던 scrim 의 범위를 surface 범위 popup 에서 어떻게 할지 — 이 결정은
  scrim 을 바꾸지 않았다. 디자인 값이라 Claude Design 이 정한다.~~ — **발화했고 닫혔다.** 디자인이
  답을 정했고 [ADR-0300](0300-the-scrim-covers-the-popups-scope-not-always-the-window.md) 이 그것을
  결정으로 박았다(scrim 은 popup 이 소속된 범위의 rect 를 덮는다). 이 ADR 의 본체는 바뀌지 않는다.

## References

- [design/systems/popup.md](../design/systems/popup.md) §스코프
- [dev-guide/popup-implementation.md](../dev-guide/popup-implementation.md) — 두 팝업 시스템 비교
- [ADR-0300](0300-the-scrim-covers-the-popups-scope-not-always-the-window.md) — 위 재검토 조건의 미결(scrim 범위)을 닫은 결정
- [ADR-0043](0043-convert-input-popup-capability.md) — 변환 입력 popup 을 host 가 여는 경로
- 코드 근거(결정이 실현된 현재 위치): `PopupScopeDecl`(`crates/tasty-plugin-manifest/src/types.rs`),
  `popup_scope`(`src/plugin_bridge/popup_scope.rs`) · `place_popup`(`src/plugin_bridge/popup_render.rs`),
  `PluginManager::bind_popup_instance_surface`(`crates/tasty-host-plugin/src/manager/popup.rs`)
