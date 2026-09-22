# ADR-0108: 스크롤은 한 pass 에 전량 전달한다 — egui-mesh 는 휠 델타를 쪼개 넣고, 스크롤 애니메이션은 끈다

- **Status**: Accepted
- **Date**: 2026-09-03
- **Tags**: egui-mesh, plugin-sdk, scroll, self-repaint, performance, theme, animation
- **Group**: plugin-render

## Context

egui-mesh popup(git-viewer / clipboard-viewer 등)의 스크롤이 버벅인다는 보고가 있었다. 원인은 휠 한 번이 **여러 프레임에 걸친 프로세스 간 왕복**으로 증폭되는 것이다.

egui 0.31 은 마우스 휠 델타를 두 갈래로 나눈다. `Point` 단위 이벤트의 델타 길이가 **8pt 미만**이면 "이미 부드러운 입력"으로 보고 그 프레임에서 전량 반영하지만, 8pt 이상(또는 `Line`/`Page` 단위)이면 `unprocessed_scroll_delta` 에 적립해 여러 프레임에 걸쳐 지수완화로 소진한다. 소진이 끝날 때까지 egui 는 매 pass `wants_repaint_after() == Duration::ZERO` 를 돌려준다.

host 가 와이어 `Scroll` 로 보내는 값은 경로마다 다르다. **egui-mesh surface**(markdown/image 등)는 `src/view/main/mouse.rs` 가 winit 델타를 논리 포인트로 환산해 보낸다(`LineDelta` × 50, `PixelDelta` ÷ ppp). **popup / banner** 는 `src/plugin_bridge/popup_render.rs` · `banner_render.rs` 가 host egui 의 `Event::MouseWheel { delta, .. }` 를 **`unit` 을 버리고 그대로** `Scroll { x, y }` 로 전달한다 — 즉 트랙패드(`PixelDelta` → `Point`, 수십 pt)는 포인트 값이 그대로 오지만, 물리 마우스 휠은 winit 이 모든 플랫폼에서 `LineDelta(0, ±1)` 로 주므로 notch 당 1pt 만 도착한다(별개의 기존 결함 — 아래 Consequences). 어느 경로든 plugin SDK 는 받은 값을 `Point` 단위 `MouseWheel` **한 건**으로 egui 에 넣었으므로, 8pt 판정선을 넘는 값(surface 의 50pt, popup 의 트랙패드 델타)은 항상 다중 프레임 소진 경로를 탔다.

일반 egui 앱에서 이 스무딩은 같은 프로세스 안의 추가 프레임일 뿐이다. egui-mesh 는 다르다. plugin 이 별도 프로세스라, 추가 프레임 하나가 곧 `*Invalidated` 알림 → host 의 `set_context` 재송신 → plugin 의 **전체 egui pass + tessellate + 인코딩 + 공유버퍼 복사** → host 의 디코드 + GPU 업로드다. 같은 애니메이션이라도 지불하는 비용의 자릿수가 다르다.

한편 egui 의 `Style::scroll_animation`(기본 `points_per_second: 1000`, `duration: 0.1..=0.3`)은 코드베이스 어디에서도 설정된 적이 없었다. 다만 이 값은 **프로그램적 스크롤**(`Ui::scroll_to_cursor` / `scroll_to_rect` / `scroll_with_delta`, `Response::scroll_to_me`)에만 쓰이고 휠 스무딩과는 무관하다 — 위 증상의 원인이 아니다. 그럼에도 상한 300ms 는 [`design/systems/theme.md`](../design/systems/theme.md) "UI 디자인 규칙" 의 애니메이션 상한(150ms)을 넘고, egui-mesh 에서는 그 애니메이션 프레임도 똑같이 왕복 비용을 문다.

## Decision

**egui-mesh 의 스크롤은 입력이 도착한 pass 에서 전량 반영한다.** 두 축으로 집행하며 범위가 서로 다르다 — 1번(휠 델타 분할)은 egui-mesh 경로 전용이고, 2번(프로그램적 스크롤 애니메이션 비활성)은 host 까지 포함한 전역이다. host egui 의 **휠** 은 이 ADR 이 손대지 않는다(아래 Consequences).

1. **휠 델타 분할 (egui-mesh 전용, 원인 해소)** — plugin SDK 의 와이어→egui 이벤트 매핑(`crates/tasty-plugin-sdk/src/egui_surface.rs`)이 `Scroll` 하나를 egui 의 smooth 판정선(8pt) 아래 조각들로 쪼개 **같은 프레임의 이벤트 목록**에 넣는다. egui 는 각 조각을 "부드러운 입력" 으로 보고 그 프레임에서 모두 `smooth_scroll_delta` 에 더하므로, 합계는 원본 델타와 같고 `unprocessed_scroll_delta` 에는 아무것도 남지 않는다. 조각 수가 상한(64)을 넘는 극단적 델타는 쪼개지 않고 그대로 넘겨 이벤트 폭증을 막는다 — 그 경우에만 egui 기본 스무딩으로 되돌아간다.

2. **스크롤 애니메이션 비활성 (전역 정책)** — 프로그램적 스크롤의 `Style::scroll_animation` 을 `ScrollAnimation::none()` 으로 둔다. host egui(`crates/tasty-egui-theme` 의 `apply_theme_to_egui`)와 모든 egui-mesh Context(`EguiMeshCore::new`, dark/light 두 style 모두)에 동일하게 적용한다. 스크롤은 "입력 직후 피드백" 이 아니라 콘텐츠 이송이므로, 위 표에서 UI 위젯 애니메이션(100–150ms)이 아니라 "스크롤엔 transition 금지" 쪽에 선다.

**`Theme` 필드로 올리지 않는다.** `Theme` 은 팔레트·sizing 스키마이고 plugin 에는 `ThemeWire`(colors + is_light + ui_zoom)로만 건너간다. 이 결정에는 조절 가능한 수치가 없다(값이 아니라 "끈다" 는 정책이고, egui 가 `ScrollAnimation::none()` 이라는 이름 있는 생성자를 제공한다). 필드로 만들면 테마마다 달라지지 않는 상수를 위해 와이어 프로토콜을 넓히게 된다. 정책의 단일 출처는 이 ADR 과 theme.md 표이고, 두 호출 지점이 그것을 인용한다. theme.md 의 "새 시각 규칙은 `Theme` 에 필드 신설" 규칙에 **on/off 정책은 제외**라는 예외를 같은 취지로 명시해 두었다 — 기존 애니메이션 2행(터미널 0ms / 위젯 100–150ms)도 대응 `Theme` 필드가 없는 선례다.

## Consequences

- **얻은 것**: 휠 한 칸이 유발하는 후속 왕복이 크게 줄었다. `ScrollArea` 를 그리는 표준 UI 에서 후속 pass 가 **12 → 2** 로 줄어드는 것을 단위 테스트가 고정한다(`splitting_a_wheel_delta_cuts_the_follow_up_repaint_passes`). 남은 2 는 스크롤 스무딩이 아니라 egui 가 모든 입력 뒤에 요청하는 pass 1 회와 스크롤바 표시 전환 1 회다. 스크롤 이동량은 보존되고(조각 합 = 원본 델타), 입력이 멎은 뒤 잔여 델타가 남지 않으므로 "뒤늦게 몰려 반영" 회귀 경로 자체가 사라진다.
- **잃은 것**: egui-mesh 의 휠 스크롤이 관성 없이 즉시 이동한다 — 큰 델타에서 체감이 달라진다. host egui 의 프로그램적 스크롤도 애니메이션 없이 점프한다 — 소비처는 3곳이다: `src/adapters/ui/sidebar/view.rs`(활성 workspace 카드로 스크롤) · `crates/tasty-ui-widgets/src/multi_select.rs`(선택 행으로 스크롤) · `src/adapters/ui/modifier_hint_overlay.rs`(modifier 힌트 목록 휠 위임). 두 변화 모두 theme.md 의 스크롤 정책과는 정합하며 의도된 결과다.
- **범위 밖으로 남긴 사실**: popup·banner 경로는 host egui 델타를 **단위 없이** 전달하므로 `Line`/`Page` 단위가 포인트로 환산되지 않는다 — 물리 마우스 휠은 popup 에서 notch 당 1pt 만 움직이고, 그 값은 8pt 판정선 아래라 이 분할이 관여할 여지도 없다. 이 변경 이전부터 있던 별개 결함이고 재현 경로(물리 휠)가 이 트랙에서 검증되지 않았으므로 여기서 고치지 않는다. 환산이 들어오면(host 가 `line_scroll_speed` 로 `Line` → `Point` 변환) 분할이 popup + 마우스 휠 조합에도 비로소 작동한다.
- **host egui 휠은 그대로다**: host 는 `ScrollAnimation::none()` 만 받았고 분할은 SDK 에만 있다. egui-winit 이 휠을 `MouseWheelUnit::Line` 으로 넣고 egui 는 `Line` 을 절대 smooth 로 보지 않으므로, 설정창·팔레트·host popup·갤러리의 `ScrollArea` 는 egui 기본 스무딩을 유지한다. host 쪽 왕복은 프로세스 간이 아니라 같은 프로세스의 추가 프레임이라 이 ADR 이 겨냥한 비용이 아니다.
- **운영 비용 / 유지 부담**: 분할은 egui 내부 판정선(8pt)에 의존한다. egui 가 그 값을 바꾸거나 스무딩을 파라미터화하면(`input_state` 에 `TODO(emilk): parameterize` 가 남아 있다) 분할이 무의미해지거나 조각 수가 어긋난다. 다만 실패 양상은 **조용한 성능 회귀**(옛 동작으로 복귀)이지 기능 파손이 아니다. 판정선 상수와 조각 계약(각 조각 < 판정선, 합 = 원본)은 단위 테스트가 지킨다.

## Alternatives Considered

- **A: `scroll_animation` 만 조정한다** — 원 진단이 지목한 방법이지만 효과가 없다. `Style::scroll_animation` 은 `Ui::scroll_to_*` / `Response::scroll_to_me` 계열만 읽고, 휠 스무딩은 `InputState` 에 하드코딩된 지수완화라 이 필드를 보지 않는다. 그래서 원인 해소는 1번(분할)이 맡고, `scroll_animation` 은 정책 정합 목적으로 함께 적용했다.
- **B: host 가 보내는 델타를 8pt 미만으로 줄인다** — 스무딩은 사라지지만 휠 한 칸의 이동 거리가 함께 줄어든다. 스크롤 감각(이동량)을 바꾸지 않는 것이 이 작업의 전제라 기각.
- **C: plugin 쪽에서 `MouseWheel` 대신 `ScrollArea` 오프셋을 직접 조작한다** — 모든 plugin 이 자기 스크롤 상태를 손수 관리해야 하고, egui 위젯(중첩 스크롤·수평 스크롤·키보드 스크롤)과 이중 관리가 된다. SDK 한 곳에서 끝나는 1번보다 나쁘다.
- **D: 적용 범위를 git-viewer plugin 하나로 좁힌다** — 같은 증상은 clipboard-viewer·markdown·image 등 모든 egui-mesh 표면에 있고, 외부 plugin 도 같은 SDK 경로를 탄다. plugin 마다 반복 설정하게 하는 대신 SDK 공통 경로에 둔다.
- **E: 정책 값을 `Theme` 필드로 노출한다** — 위 Decision 마지막 문단 참조. 테마별로 달라지지 않는 상수를 위해 `ThemeWire` 를 넓히는 비용이 이득보다 크다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- egui 가 휠 스무딩을 파라미터화하거나(현재 `input_state` 의 `TODO(emilk): parameterize`) smooth 판정선(8pt)을 바꾼다 — 그 경우 분할 대신 공식 설정을 쓴다.
- egui-mesh 채널이 부분 갱신(전체 egui pass 없이 스크롤 오프셋만 반영)을 지원하게 되어 추가 pass 의 비용이 더 이상 자릿수 차이가 아니게 된다.
- popup·banner 경로에 휠 단위 환산(`Line`/`Page` → `Point`)이 들어온다 — 그때 물리 마우스 휠에서도 분할이 효과를 갖게 되므로 그 조합의 왕복 수치를 다시 잰다.
- 스크롤 관성이 없는 것이 실사용에서 문제로 보고된다 — 그 경우 "왕복 없이 관성" 을 만드는 별도 설계(plugin 쪽 자체 애니메이션 + 오프셋 전용 갱신)를 검토한다.

## References

- [`docs/design/systems/theme.md`](../design/systems/theme.md) — "UI 디자인 규칙" 표의 스크롤/애니메이션 정책
- [`docs/dev-guide/egui-mesh-channel.md`](../dev-guide/egui-mesh-channel.md) — egui-mesh 채널 규약, self-repaint 와 `*Invalidated`
- [ADR-0097](0097-plugin-self-repaint-resident-timer.md) — self-repaint 알림을 보내는 상주 타이머(이 ADR 은 알림 **횟수** 를 줄이는 축)
- `crates/tasty-plugin-sdk/src/egui_surface.rs` — 분할(`push_scroll_events`)과 Context 초기화
- `crates/tasty-egui-theme/src/lib.rs` — host egui 의 `apply_theme_to_egui`
- egui 0.31.1 `egui/src/input_state/mod.rs`(휠 smooth 판정과 지수완화 drain) · `egui/src/style.rs`(`ScrollAnimation`)
