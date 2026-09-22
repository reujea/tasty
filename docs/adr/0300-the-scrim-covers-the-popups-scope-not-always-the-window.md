# ADR-0300: scrim 은 popup 이 소속된 범위를 덮는다 — 늘 창 전체가 아니다

- **Status**: Accepted
- **Date**: 2026-09-20
- **Tags**: popup, scope, scrim, plugin, design-tokens, gallery, adr-0273, adr-0254
- **Group**: ui-theme-gallery

## Context

[ADR-0273](0273-plugin-popup-declares-a-scope-kind-and-the-host-binds-the-target.md) 이
plugin popup 에 범위 축을 더하면서 가시성·경계·앵커 기준을 전부 그 범위로 옮겼다. 하나가
남았다 — **scrim**. 그 ADR 은 재검토 조건에 그것을 미결로 적었다: *"plugin popup 이 창
전체에 깔던 scrim 의 범위를 surface 범위 popup 에서 어떻게 할지 — 이 결정은 scrim 을
바꾸지 않았다. 디자인 값이라 Claude Design 이 정한다."*

그래서 지금의 상태는 축이 반만 옮겨진 것이다. surface 범위 popup 은 제 칸 안에 놓이고 그
칸이 안 보이면 사라지는데, 어둡게 하는 자리만 여전히 창 전체다. 옆 칸에서 도는 빌드도,
사이드바도, 상태바도 함께 어두워진다 — 그 popup 이 무엇에 묶였는지를 화면이 부정한다.

host 쪽에도 같은 결이 있었다. `PopupManager` 는 scrim 을 **id 세트**로만 깔았고 그 세트는
전부 창 범위 popup 이었다. surface 범위를 쓰는 host popup(`convert_surface`)에는 scrim 이
아예 없어서, 같은 자리에서 열리는 plugin popup 과 생김새가 갈렸다.

중복도 있었다. host 는 `scrim_painted` 불 하나로 프레임당 한 번을 보장했지만 plugin 경로는
**인스턴스마다** 깔았고, scrim 은 알파 한 벌이라 두 번 깔면 그 자리가 두 배로 어두워진다.

## Decision

**scrim 이 덮는 rect 는 그 popup 이 소속된 범위의 rect 다.** 창 범위면 화면 전체, surface
범위면 그 칸 하나다. 경계는 그 surface 의 **보더를 포함**하고 인접 surface · 사이드바 ·
pane 탭바 · 상태바는 **제외**한다. radius 는 새 값이 아니라 범위 대상 자신의 radius 이고,
오늘의 셸에서 surface radius 는 0 이라 직각으로 떨어진다. **알파는 한 벌이다** —
`--tasty-scrim-bg` 그대로이고, 범위가 둘이라고 값을 두 벌로 나누지 않는다.

**바인딩이 없으면 창 전체다.** 창·워크스페이스 범위, 그리고 선언이 `surface` 여도 대상이
바인딩되지 않은 호환 경로는 좁힐 대상이 없으므로 종전 동작을 그대로 유지한다.

**scrim 은 범위당 한 번이다.** 같은 범위에 popup 이 여럿 떠도(부모 popup 과 그것이 연 자식
file picker) 한 번만 깔린다. 창 scrim 이 있으면 그 안의 surface scrim 은 깔지 않는다 —
넓은 쪽이 이긴다. 두 규칙 모두 같은 이유에서 나온다: 알파가 한 벌이라 두 번 칠하면 그
자리만 두 배로 어두워진다. 판정은 host 와 plugin 두 경로가 **같은 함수**
(`PopupManager::pick_scrim_layers`)를 쓴다.

**어느 popup 이 scrim 을 까는가는 범위가 아니라 id 로 정한다.** surface 범위를 쓰면서도
scrim 을 안 까는 표면이 있다 — `search_bar` 는 트리거 옆에 붙는 anchored + scrim-less
갈래다([ADR-0254](0254-floating-surface-shadow-scope-rule.md)). 범위로 판정하면 그 갈래가
조용히 뒤집힌다. 그 대신 `convert_surface` 를 scrim 명부에 더해, surface 범위 host 모달도
plugin 쪽과 같은 모양을 갖는다.

**surface 범위 popup 은 그 칸에서 `spacing-sm`(8pt) 안쪽에 놓인다.** 칸 보더에 딱 붙으면
셸의 일부처럼 보여 어느 칸에 묶였는지가 안 읽힌다. 창·워크스페이스 범위는 경계가 화면이라
들일 자리가 없고, pane·tab 범위는 이 결정이 다루지 않아 그대로 둔다. 경계보다 큰 셸은
경계로 줄인다 — plugin 셸은 GPU 합성으로 콘텐츠가 올라가 egui 레이어 클립이 그것까지
잘라 주지 않으므로, 크기를 줄여야 이웃 칸으로 새지 않는다.

**이 결정이 안 바꾸는 것**: 입력 차단(어둡게 하는 것과 막는 것은 다른 일이다) · Esc 와
바깥 클릭의 순서 · draft 수명과 부모/자식 숨김-복원 정책 · 그림자 두 값과 그 3-갈래 규칙.

## Consequences

- **얻은 것**: surface 에 묶인 popup 이 그 칸만 어둡게 한다. 옆 칸에서 도는 작업이 계속
  읽히고, 그 popup 이 무엇에 묶였는지를 화면이 말한다. ADR-0273 이 옮긴 축이 마지막 한
  자리까지 같은 기준을 쓴다. plugin 경로의 인스턴스별 중복 scrim 도 함께 닫힌다.
- **잃은 것**: `convert_surface` 에 없던 scrim 이 생긴다 — 사용자에게 보이는 변화다.
  그리고 surface 범위 popup 은 이제 그 칸을 넘어설 수 없어, 칸이 좁으면 셸이 줄어든다.
- **운영 비용 / 유지 부담**: scrim 명부(`popup_has_scrim`)와 그림자 명부
  (`ANCHORED_POPUPS`)가 서로 맞물려 있다 — anchored 갈래가 scrim 을 받으면 그 갈래의
  이름이 거짓이 된다. 그 어긋남은 `no_anchored_popup_takes_a_scrim` 이 든다. host 와
  plugin 두 경로는 판정기는 공유하지만 그리기는 따로 하므로, 두 경로의 popup 이 같은
  범위에 동시에 뜨면 scrim 이 둘이 된다. 지금은 그 배치가 안 난다 — 같은 범위를 공유하는
  유일한 조합인 plugin 부모 + host 자식 file picker 에서 자식은 scrim 명부에 없다.
- **경계를 지키는 것은 layer clip 이고, 그 clip 은 매니저가 그리는 것만 덮는다**: scrim ·
  배경 · 프레임 · 그림자는 범위 rect 로 잘린다. popup 의 *내용* 은 자기 `Area` 안에서
  그려지고 그 `Ui` 의 clip 은 egui 의 패널 컨테이너가 자기 rect 로 **덮어쓴다**
  (`egui::TopBottomPanel::show_inside` · `CentralPanel::show_inside` 가
  `panel_ui.set_clip_rect(panel_rect)` 를 부른다). 그래서 자기 자연폭보다 훨씬 좁게
  눌린 popup 은 내용이 범위 밖으로 샐 수 있다. 실측: 39 칸(277pt) surface 에 `port_scanner`
  를 억지로 묶으면 footer 카운터와 빈 상태 문구가 사이드바 자리(x 124..179)에 430 px 그려졌다.
  제품 경로로는 안 난다 — surface 에 묶이는 host popup 은 `convert_surface`(200×229)와 그것이
  낳는 `file_picker` 뿐이고, 같은 칸에서 둘 다 바깥 유출 0 px 이다.

## Alternatives Considered

- **범위로 scrim 여부를 판정한다** (surface 범위면 무조건 깐다) — `search_bar` 가 scrim 을
  받는다. ADR-0254 가 그것을 anchored + scrim-less 로 부르는데 그 이름이 거짓이 된다.
  명부를 유지하는 비용보다 갈래가 조용히 뒤집히는 비용이 크다.
- **자식 picker 도 제 scrim 을 깐다** (겹침을 알파로 허용) — 부모만 있을 때와 자식이 열렸을
  때의 배경 밝기가 달라진다. 같은 토큰을 쓰는데 상태에 따라 값이 달라 보이는 것이라,
  "알파 한 벌" 이라는 결정과 정면으로 어긋난다.
- **inset 없이 칸 경계까지 붙인다** — 셸의 1px 보더와 popup 의 1px 보더가 맞닿아 두 줄이
  한 줄로 읽힌다. 좁은 칸에서는 popup 이 칸을 통째로 덮어 어느 칸인지 자체가 안 보인다.
- **창 scrim 과 surface scrim 을 둘 다 깐다** (겹치는 자리는 더 어둡게) — 겹침이 값을
  만든다. scrim 은 상태를 나르는 채널이 아니라 바닥을 한 단 내리는 값이므로, 단이 둘이면
  읽는 쪽이 그 차이에 뜻을 찾는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- surface 가 0 이 아닌 radius 를 갖는다. 지금은 `SURFACE_RADIUS` 가 0 이라 범위 대상의
  radius 를 따른다는 규칙과 "직각" 이 같은 그림이지만, 그 값이 움직이면 둘이 갈린다.
- pane 또는 tab 범위를 쓰는 popup 이 scrim 명부에 들어온다. 이 결정은 그 두 범위의 inset 을
  정하지 않았으므로, 그때 그 자리를 정해야 한다.
- host 와 plugin 두 경로의 popup 이 같은 범위에 동시에 뜨는 배치가 생긴다. 그러면 경로별
  중복 제거로는 부족해져 두 경로가 계획을 나눠 가져야 한다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- surface 범위 scrim 이 native WebView overlay(markdown · html surface)와 겹쳐 그 칸만
  어두워지지 않는다. 재는 법: markdown surface 에서 파일 열기 popup 을 띄우고 OS 화면
  캡처로 본다(`ui.screenshot` 에는 WebView 가 담기지 않는다 —
  [ai-verification/screenshot-methods](../ai-verification/screenshot-methods.md)).
- 좁은 칸에서 셸이 줄어든 결과가 쓸 수 없을 만큼 작다. 재는 법: 좌우 분할을 한쪽이 창의
  1/4 이하가 되게 끌고 그 칸에서 변환 popup 을 연다.
- 자기 자연폭보다 좁은 칸에 묶이는 popup 이 새로 생긴다(위 Consequences 의 내용 유출).
  그러면 경계를 내용까지 지키는 수단이 필요해진다 — 매니저의 layer clip 으로는 안 닿고,
  egui 패널이 clip 을 덮어쓰지 않게 하거나 패널을 안 쓰는 쪽으로 바꿔야 한다. 재는 법:
  그 popup 을 좁은 칸에 띄우고 칸 바깥 영역을 popup 없는 같은 화면과 픽셀 비교한다
  (차이 0 이어야 한다).

## References

- [design/systems/popup.md](../design/systems/popup.md) §스코프
- [dev-guide/popup-implementation.md](../dev-guide/popup-implementation.md)
- [ADR-0273](0273-plugin-popup-declares-a-scope-kind-and-the-host-binds-the-target.md) — 이 결정이
  그 ADR 의 재검토 조건에 적힌 미결(scrim 범위)을 닫는다. 그 ADR 의 본체는 그대로 유효하다.
- [ADR-0254](0254-floating-surface-shadow-scope-rule.md) — 그림자 3-갈래. 이 결정은 그것을
  바꾸지 않는다: 갈래를 가르는 술어는 여전히 "뷰포트를 점유하는가" 이고 scrim 유무가 아니다.
- 코드 근거(결정이 실현된 현재 위치): `PopupManager::pick_scrim_layers` · `scope_bounds` ·
  `popup_has_scrim`(`src/adapters/ui/popup/draw.rs`), `scrim_plan` · `place_popup`
  (`src/plugin_bridge/popup_render.rs`), specimen(`crates/tasty-gallery/src/catalog/components/scrim_scope.rs`)
