# ADR-0130: 휠 1노치가 옮기는 거리는 창 안에서 하나이고, 그 값은 사용자가 정한다

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: input, scroll, egui, plugin-bridge, settings, accessibility, wire-contract, adr-0108
- **Group**: terminal-input

## Context

같은 창 안에서 휠 한 칸이 표면에 따라 다른 거리를 스크롤한다.

| 표면 | 변환 위치 | Line 1노치 |
|---|---|---|
| plugin egui-mesh surface | `src/view/main/mouse.rs` | 50pt |
| attach mesh mirror | `src/view/main/mouse.rs` | 50pt |
| plugin popup | `src/plugin_bridge/popup_render.rs` | 50pt |
| plugin banner | `src/plugin_bridge/banner_render.rs` | 50pt |
| modifier hint overlay | `src/adapters/ui/modifier_hint_overlay.rs` | 40pt |
| host egui 위젯 전반(설정 모달·사이드바 등 `ScrollArea`) | egui 내부 | 40pt |

앞의 넷은 `plugin_bridge::wire_scroll::LINE_SCROLL`(50)을 읽고, 뒤의 둘은 egui
`Options::line_scroll_speed` 를 읽는다. 차이는 25% 다.

두 값 중 어느 쪽도 측정이나 원리에서 나오지 않았다.

- **50** 은 egui-mesh surface 경로가 원래 winit `LineDelta` 에 곱하던 상수이고, 뒤늦게
  합류한 popup·banner 를 거기에 맞춘 것이다 — 기존 동작 보존이 근거다.
- **40** 은 egui 의 native 기본값이다. egui 는 같은 값을 web 에서 8 로 잡으며, 그 분기
  바로 위에 `TODO(emilk): figure out why these constants need to be different on web and
  on native` 이 붙어 있다. 즉 egui 자신도 이 값을 설명하지 못한 채 플랫폼별로 다르게
  두고 있고, 그래서 **"egui 기본값이니까 40 이 맞다" 는 근거가 되지 못한다.** egui 는
  이 값을 시각 축(`Style`)이 아니라 동작 축(`Options`)에 두어 **앱이 정할 값**으로
  분류했다.

갈래가 어디를 지나는지가 판단의 핵심이다. 이 분할선은 **표면의 성격을 따르지 않는다.**
chrome 에 해당하는 것이 양쪽에 다 있고(modifier hint overlay 40 · plugin banner 50),
콘텐츠 표면도 양쪽에 다 있다(plugin markdown 50 · host 위젯 목록 40). 실제로 선을
가르는 것은 **그 코드가 `wire_scroll` 을 지나느냐 egui 내부를 지나느냐** — 구현 출신이다.

접근성 축도 함께 걸린다. 스크롤 속도는 조정 요구가 흔한 값인데 현재는 어느 쪽도
사용자가 바꿀 수 없다.

## Decision

**한 창 안에서 휠 1노치가 옮기는 거리는 표면 종류와 무관하게 하나다. 그 값을 tasty 가
직접 정해 egui `Options::line_scroll_speed` 에 밀어 넣고, 사용자가 조정할 수 있도록
`GeneralSettings.wheel_line_scroll` 로 노출한다. 기본값은 50pt 다.**

세 가지가 함께 결정된다.

1. **통일 방향은 50 이다.** 40 은 egui 가 설명하지 못한 채 플랫폼별로 갈라 둔 값이라
   기준이 될 수 없고, 50 은 이미 6 표면 중 4 곳이 쓰는 값이며 그중 셋은 **plugin
   프로세스로 나가는 와이어 값**이다 — 와이어 `Scroll` 은 논리 포인트를 나른다
   (`src/plugin_bridge/wire_scroll.rs`).
   바꿔야 할 곳이 적은 쪽이자, 프로세스 경계를 넘지 않는 쪽이다.
2. **런타임 단일 출처는 egui `Options::line_scroll_speed` 다.** 설정값을 그 옵션에 밀어
   넣고, 휠을 포인트로 환산하는 모든 지점이 그 옵션을 읽는다. `wire_scroll` 은 배율
   상수를 갖지 않게 되고, 기본값의 정의처는 설정 쪽
   (`tasty_settings::DEFAULT_WHEEL_LINE_SCROLL`) 하나다. 이 구조 덕에
   `modifier_hint_overlay` 는 손대지 않아도 따라온다 — 그것은 이미 이 옵션을 읽는다.

   **이 성질을 보는 술어가 둘이다.** 값을 한 번 맞추는 것과 맞은 채로 있는 것은 다르고,
   환산 함수에 노치를 인자로 넘겨 산술만 보는 테스트는 호출 자리가 그 노치를 **어디서
   얻는지**를 말하지 않는다 — 표면이 하나 더 생기면서 상수를 다시 박아도 초록이다.
   그래서 동작 축(`src/plugin_bridge/wire_scroll.rs` 의 `one_notch_per_context`)은 기본값도
   egui 기본값도 아닌 값을 컨텍스트에 심고 egui 자신·와이어 환산·overlay 셋이 그것을
   집어 오는지 재고, 소스 축(`crates/tasty-doc-guards/tests/wheel_notch_has_one_runtime_source.rs`)은 휠 Line 을
   다루는 자리를 세어 **넷째가 생기는 것**을 본다. 환산하지 않고 단위를 그대로 넘기는
   자리는 그 성질을 사유로 적어 면제한다.
3. **설정의 자리는 `GeneralSettings` 다.** 이 절은 이미 마우스 동작 설정을 담고 있다
   (`link_click_modifier` · `click_to_move_cursor` · `mouse_capture_hint` ·
   `mouse_capture_blacklist`). 새 절을 만들지 않는다.

설정 하나가 여섯 표면 전부에 걸린다 — host 위젯은 egui 가 그 옵션으로 스크롤하고,
plugin 표면은 같은 옵션에서 뽑은 값이 와이어에 실린다.

## Consequences

- **얻은 것**: 같은 창에서 사이드바를 굴리든 plugin popup 을 굴리든 같은 거리를 움직인다.
  변환 지점이 넷에서 **하나의 값**으로 수렴해, `wire_scroll` 모듈 문서가 달고 있던
  "이것이 프로세스 전체의 단일 출처는 아니다" 단서가 사라진다. `modifier_hint_overlay`
  의 40 은 예외로 남지 않고 자동으로 해소된다. 접근성 요구(느리게/빠르게)가 설정
  하나로 충족되고, 그 설정이 두 경로 모두에 걸린다.
- **잃은 것**: host UI 스크롤이 기존보다 **25% 빨라진다.** 설정 모달·사이드바·갤러리
  등 사용자가 이미 익숙해진 표면의 체감이 바뀐다. 되돌릴 수단(설정)을 같은 변경에서
  함께 제공하지만, 기본값이 바뀌는 사실 자체는 남는다.
- **운영 비용 / 유지 부담**: 와이어 값의 성질이 바뀐다. 종전에는 "host 플랫폼 사정
  (native 40 / web 8)에 흔들리지 않는 고정값" 이었고, 이제는 **"사용자 설정에 따라
  인스턴스마다 다를 수 있는 값"** 이다. plugin 은 받은 포인트를 그대로 쓰므로 동작은
  같지만, plugin 로그나 재현 절차에서 "노치당 50pt" 를 상수로 가정하면 어긋난다 —
  값을 가정하지 말고 설정을 읽어야 한다. 그리고 이제 egui 옵션을 tasty 가 소유하므로,
  egui 가 이 기본값을 바꿔도 tasty 는 따라가지 않는다(추종하려면 명시적 결정이 필요하다).

## Alternatives Considered

- **A: `modifier_hint_overlay` 만 `LINE_SCROLL`(50)로 맞춘다** — 변경량이 가장 작다.
  그러나 host 위젯 전반은 여전히 옵션 값을 쓰므로, "overlay 는 plugin 과 같고 나머지
  host UI 는 다르다" 는 **새 갈래가 생긴다.** 갈래가 줄지 않고 옮겨갈 뿐이라 기각.
  채택안은 이 항목을 부수적으로 흡수한다 — overlay 는 옵션을 읽으므로 손댈 필요가 없다.
- **B: 40 으로 통일한다** — 방향만 반대인 안. 근거로 쓸 수 있는 것이 "egui native
  기본값" 뿐인데 그것이 위 Context 의 이유로 근거가 못 된다. 바꿔야 할 지점이 넷으로
  더 많고, 그중 셋은 프로세스 경계를 넘는 와이어 계약이다. 기각.
- **C: 통일하지 않는다 — 표면 성격이 다르므로** — 가장 진지하게 검토한 대안이다.
  "plugin 이 그리는 콘텐츠 표면과 host 의 UI 크롬은 다른 물건이니 스크롤 감도가 달라도
  된다" 는 주장 자체는 성립할 수 있다. 기각 사유는 그 주장이 **현재 코드가 그은 선과
  일치하지 않는다**는 것이다: chrome 인 modifier hint overlay 가 40 이고 chrome 인
  plugin banner 가 50 이며, 콘텐츠인 markdown 표면이 50 이고 콘텐츠인 host 목록이 40 이다.
  지금의 분할선은 성격이 아니라 구현 출신을 따른다. 성격에 따라 나누는 설계를 하려면
  그 경계를 새로 그어야 하고, 그것은 "지금 상태를 유지한다" 와 다른 일이다.
- **D: 값만 50 으로 통일하고 설정 노출은 나중으로 미룬다** — 25% 체감 변화를 되돌릴
  수단 없이 내보내게 된다. 또 노출 시점에 "그럼 기본값을 얼마로 두나" 를 다시 열게 되어
  같은 결정을 두 번 하게 된다. 결정은 함께 하고 구현만 나누는 편이 낫다고 판단.
- **E: `Theme` 에 둔다** — 색·타이포·간격·모션 duration 을 담는 곳에 입력 장치 튜닝
  값이 들어가면 "테마를 바꾸면 스크롤 속도가 바뀐다" 가 된다. egui 자신도 `Style`(시각)이
  아니라 `Options`(동작)에 두어 같은 선을 긋는다. 기각.
- **F: `AccessibilitySettings` 에 둔다** — 접근성 축에서 요구되는 값인 것은 맞다. 다만
  그 절은 토글 모음이고(현재 `reduced_motion` 하나), 스크롤 속도는 접근성 전용 관심사가
  아니라 **모든 사용자가 취향을 갖는 입력 선호**다. 접근성 요구가 이 설정으로 충족되는
  것과 이 설정이 접근성 절에 속하는 것은 다른 이야기이고, "스크롤 속도" 를 찾는 사용자가
  접근성 절을 뒤지게 만들 이유가 없다. 마우스 설정이 이미 모여 있는 `GeneralSettings` 로.
- **G: `KeybindingSettings` 에 둔다** — 단축키가 아니다. 그 절은 키 조합만 담는다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- OS 가 "휠 한 번에 몇 줄" 시스템 설정을 노출하고 tasty 가 그것을 읽을 수 있게 된다 —
  그러면 기본값을 고정 50 대신 **OS 값 추종**으로 두는 쪽이 더 옳을 수 있다.
- egui 가 `line_scroll_speed` 의 native/web 분기를 정리하거나 플랫폼 설정에서 읽어오도록
  바뀐다(그 `TODO` 가 해소된다) — tasty 가 옵션을 계속 소유할지 다시 판단할 시점.
- plugin 이 포인트가 아니라 줄 수를 받기를 요구하는 사용례가 나타난다 — 와이어가 나르는
  단위 자체를 다시 여는 일이라 이 ADR 도 함께 본다.

## References

- [ADR-0108](0108-egui-mesh-scroll-delivered-in-one-pass.md) — 같은 와이어 `Scroll` 의 전달 방식(한 pass 에 전량, 분할 조각)
- [input-layer](../architecture/input-layer.md) — 입력 계층 일관성
- [theme](../design/systems/theme.md) — 시각 토큰의 범위(입력 튜닝 값을 담지 않는 이유)
- `src/plugin_bridge/wire_scroll.rs` — 줄 → 포인트 환산의 단일 지점
- `crates/tasty-doc-guards/tests/wheel_notch_has_one_runtime_source.rs` — 환산 자리가 런타임 출처를 읽는지 세는 술어
