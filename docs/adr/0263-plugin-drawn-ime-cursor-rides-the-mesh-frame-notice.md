# ADR-0263: plugin 이 그린 IME 커서 영역은 mesh frame 알림에 실려 host 로 돌아온다

- **Status**: Accepted
- **Date**: 2026-09-11
- **Tags**: ime, egui-mesh, plugin-protocol, typed-length, candidate-window, popup, surface
- **Group**: plugin-render

## Context

OS IME 후보창(조합 후보 목록)의 화면 위치는 winit `set_ime_cursor_area` 로 정하고, 그
API 를 부를 수 있는 것은 **창을 쥔 host** 뿐이다. 그런데 위치를 아는 것은 host 가 아니다.

host 에는 그 위치를 알아내는 경로가 둘 있었고 egui-mesh 에서는 **둘 다 안 탔다**.

- tasty 자체 경로(`update_ime_cursor_area`)는 `ime_preedit` 가 `Some` 일 때만 동작하고
  위치를 **터미널 셀 좌표**로 계산한다. egui-mesh forward 경로는 그 상태를 일부러 안
  세운다 — egui-mesh 콘텐츠에는 셀 좌표가 없다.
- egui-winit 자동 경로는 `PlatformOutput::ime` 를 읽는다. 그 값은 IME 를 원하는 egui
  위젯이 실제로 focus 중일 때만 `Some` 인데, egui-mesh 콘텐츠는 **plugin 프로세스의
  egui** 가 그리므로 host egui 에는 대응 위젯이 없다 — host 의 `platform_output.ime` 는
  늘 `None` 이다(`gfx/gpu.rs` 의 `ime_widget_focused` 주석이 이미 그 사실을 적고 있었다).

결과: egui-mesh surface·popup 에서 조합은 되는데 후보창이 마지막에 설정된 위치나 창
원점 근처에 떠 입력 지점에서 떨어진다.

값을 아는 쪽은 정해져 있다 — **plugin 프로세스의 egui** 가 매 pass `PlatformOutput::ime`
(`IMEOutput { rect, cursor_rect }`)를 계산한다. 문제는 그 값이 host 로 돌아오는 칸이
없던 것이다. 그때 **mesh 채널**이 plugin → host 로 나르던 것은 mesh 바이트(+ 그 메타)와
self-repaint 요청뿐이었다.

## Decision

**plugin egui 가 계산한 IME 커서 영역을 `PaintFrame` · `PopupPaintFrame` 알림의 새 칸
`ime_cursor: Option<ImeCursorWire>` 에 실어 host 로 되돌린다.** 좌표는 그 mesh 콘텐츠
영역 로컬 논리 포인트 — 입력 와이어의 포인터 좌표와 같은 계다. host 는 콘텐츠 영역의
물리 origin 을 더해 창 좌표로 올린 뒤(`plugin_bridge::mesh_ime_cursor_area`)
`set_ime_cursor_area` 를 부른다. 후보창 위치는 `cursor_rect`(주 캐럿)를 쓴다 — 터미널
갈래가 anchor **셀** 사각형을 넘기는 것과 같은 의미다.

`update_ime_cursor_area` 의 입력원은 이제 셋이고 **IME 라우팅의 선점 순서와 같은 순서로**
고른다: 키 포커스를 가진 plugin popup → 포커스된 egui-mesh surface → 터미널 preedit.
조합을 받는 쪽이 후보창 위치도 정한다.

**banner 는 대상이 아니다** — banner 는 키/텍스트/IME 를 forward 받지 않으므로(포커스를
주지 않는 non-modal 공지) 그 칸이 늘 `None` 이다. `BannerPaintFrame` 에는 칸을 두지 않았다.

**attach mesh mirror 는 이 결정의 범위 밖이다** — 그 경로의 frame 은 고정 길이 바이너리
chunk 헤더(`tasty-ipc` `mesh_stream`)로 나르고, 거기에 칸을 더하는 것은 헤더 길이를 바꾸는
일이라 버전이 다른 두 peer 가 조용히 오파싱한다. JSON 알림의 `#[serde(default)]` 확장과
성질이 다른 결정이므로 별건으로 남긴다.

## Consequences

- **얻은 것**: egui-mesh surface·popup 에서 후보창이 캐럿 아래에 뜬다. 값의 출처가 그
  값을 아는 유일한 프로세스라 근사 오차가 없고, 한 popup 에 입력란이 여럿이어도 맞는다.
  `#[serde(default)]` 라 그 칸이 없는 옛 plugin 의 알림도 그대로 파싱된다.
- **잃은 것**: mesh frame **dedup 에 종속된다.** 출력 바이트가 직전과 같으면 SDK 는
  알림을 아예 안 보내므로(`EguiMeshCore::render` 의 해시 dedup) 그 tick 에는 값이
  갱신되지 않는다. 실제로는 캐럿이 mesh 의 일부라 캐럿이 움직이면 바이트가 달라지고,
  안 움직이면 직전 값이 여전히 맞다 — 그래서 관측되는 결함이 없다. 다만 이 정합은
  "캐럿이 그려진다" 는 사실에 의존하므로 독립적인 불변식이 아니다.
- **운영 비용 / 유지 부담**: `tasty-plugin-protocol` · `tasty-plugin-sdk` 가 워크스페이스
  의존 폐포 안이라 번들 plugin **전부**의 patch bump + 매니페스트 lockstep + `Cargo.lock`
  이 같은 커밋에 필요하다(CLAUDE.md "버전 정책"). 판정은
  `scripts/check-plugin-version-bump.sh` 가 한다.

## Alternatives Considered

- **host 가 콘텐츠 영역만으로 근사한다**(예: 콘텐츠 영역 좌하단) — 프로토콜을 안
  건드리므로 번들 plugin bump 가 없다. 그런데 이 결정이 고치려는 결함이 바로 "후보창이
  입력 지점에서 떨어진다" 는 것이고, 근사는 입력란이 하나뿐인 폼에서만 맞는다. 고치려는
  증상과 같은 형태의 오차를 남기는 처방이라 기각했다. 값을 아는 프로세스가 이미 매 pass
  계산하고 있는데 host 가 추측하는 구조이기도 하다.
- **plugin → host 전용 이벤트를 새로 만든다**(`ImeCursorArea { … }`) — mesh frame dedup 과
  독립해진다. 그런데 그 값이 생기는 자리는 egui pass 하나뿐이라 "값이 바뀌었다" 를 아는
  시점도 pass 뿐이고, 결국 pass 마다 알림 하나가 더 늘 뿐이다. 정적 화면에서 프로세스 간
  왕복을 늘리지 않는 것이 이 채널의 설계 전제라(ADR-0108) 칸 하나를 얹는 쪽을 골랐다.
- **`rect`(편집 위젯 전체)를 후보창 위치로 쓴다** — egui-winit 이 host 위젯에 대해
  그렇게 한다. 긴 입력란에서 후보창이 줄 왼쪽 끝에 붙어, 터미널 갈래(anchor 셀)와 같은
  화면에서 동작이 갈린다. 두 rect 를 다 나르되 host 가 `cursor_rect` 를 쓴다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

**채널이 붙는 것** — 판정 시점에 레포가 읽을 수 있는 사실이다.

- `BannerPaintFrame` 이 키·텍스트·IME 중 하나라도 forward 받게 되면(=
  `banner_render.rs` 의 수집기에 `Event::Key`/`Event::Text`/`Event::Ime` 갈래가 생기면)
  banner 를 범위 밖으로 둔 근거가 사라진다.
- `mesh_stream::MESH_CHUNK_HEADER_LEN` 에 버전 협상이나 가변 확장 칸이 생기면 attach
  mesh mirror 를 범위 밖으로 둔 근거가 사라진다.

**원리적으로 안 붙는 것** — 사람이 관측해야 한다. 재는 법을 함께 적는다.

- dedup 종속이 실제 결함으로 관측되면(캐럿이 그려지지 않는 편집 위젯에서 후보창이 낡은
  자리에 뜨는 형태) 전용 이벤트 갈래를 다시 본다. 재는 법: egui-mesh 입력란에서 조합 중
  캐럿을 좌우로 옮기며 `set_ime_cursor_area` 에 넘긴 값을 로그로 관측해, 캐럿 이동마다
  값이 따라오는지 본다.

## References

- [egui-mesh 렌더 채널](../dev-guide/egui-mesh-channel.md) — 입력 forward · 입력 게이트 ·
  알려진 한계
- [ADR-0108](0108-egui-mesh-scroll-delivered-in-one-pass.md) — 정적 화면에서 프로세스 간
  왕복을 늘리지 않는다는 이 채널의 설계 전제
- [typed-length](../concepts/typed-length.md) — 논리/물리 경계를 타입으로 고정하는 규칙
- 코드 근거(결정이 실현된 현재 위치): `tasty_plugin_protocol::ImeCursorWire` ·
  `plugin_bridge::mesh_ime_cursor_area` · `MainView::update_ime_cursor_area`
