# ADR-0044: 스크린샷을 focus-독립 + ID 지정으로 만들어 debug 격리에서 release 로 승격한다

- **Status**: Accepted
- **Date**: 2026-07-10
- **Tags**: screenshot, ipc, cli, focus-independence, offscreen-render, gpu, debug-ipc, local-only, adr-0032, adr-0040
- **Group**: window-workspace-lifecycle

## Context

`ui.screenshot` 은 tasty 가 실제 렌더한 프레임을 PNG 로 떨구는 캡처 IPC 다. 기존 구현은 두 가지 이유로 **debug 빌드 전용**(`DEBUG_METHODS`, `#[cfg(debug_assertions)]`)으로 격리돼 있었다.

- **focus 종속**: 핸들러가 `View.focused_view_id` 로 대상 창을 잡고 그 창의 swapchain 프레임만 캡처했다. 대상 창/surface 를 ID 로 지정할 수 없어, "지금 포커스된 창"이라는 **사용자 가시 상태에 묶인 동작**이었다.
- **가시성 종속**: 화면에 보이는(활성 workspace 의 활성 탭) surface 만 프레임에 그려지므로, 배경 탭·다른 workspace·비-focus 창의 특정 surface 를 지정해 찍을 수 없었다.

이는 불가침 원칙 위반이다 — 에이전트 행동(캡처)이 사용자 상태(focus/가시성)에 의존하면 원칙 #3(포커스 독립성: 모든 명령은 대상을 ID 로 직접 지정, 활성 상태 의존 동작 금지)과 원칙 #1(에이전트 행동 ↔ 사용자 상태 분리)에 어긋난다. 그래서 debug 격리가 정당했다.

그런데 스크린샷 자체는 **사용자 입력 재현이 아니라 에이전트가 자기 작업을 관찰하는 기능**이다(원칙 #2: 에이전트 기능은 IPC+CLI 양면으로 노출되는 정식 능력). 승격을 막던 것은 기능의 성격이 아니라 *구현의 focus 종속*이었다. 캡처 하부(`capture_frame_to_png`)와 터미널 렌더(`render_terminals`/`render_all`)가 모두 타깃 텍스처·크기를 인자로 받아 swapchain 이 아닌 오프스크린 텍스처로 재타깃 가능하고, surface 표면이 이미 `RENDER_ATTACHMENT | COPY_SRC` usage 라, focus 종속을 걷어낼 기술적 여지가 있었다.

즉 **"focus 독립 + 대상 ID 지정"이 release 승격의 전제조건**이며, 그 전제를 충족하도록 리팩토링하면 원칙 위반 없이 정식 기능이 된다.

## Decision

`ui.screenshot` 을 focus-독립 + ID 지정 형태로 리팩토링해 release `METHOD_TABLE` 로 승격한다. 세부 결정:

1. **단일 메서드 확장** — 새 메서드를 분리하지 않고 기존 `ui.screenshot` 을 `{ path, surface_id?, window_id? }` 로 확장한다(표면 최소화). `surface_id` 가 있으면 surface 오프스크린 캡처, 없으면 window 프레임 캡처.
2. **focus 기본값 금지 · ID 해소** — `focused_view_id` 의존을 완전히 제거한다. window 캡처는 `window_id` 로 지정하고, 생략 시 창이 정확히 1개면 그 창, 다중이면 에러(포커스 폴백 없음). surface 캡처는 `surface_id` 로 소유 창(창별 `CoreState`)을 순회 해소한다 — focus 무관.
3. **surface = grid 크기 오프스크린** — 지정 surface(터미널)를 그 자체 터미널 grid(cols×rows × cell px) 크기의 **오프스크린 텍스처**에 렌더해 캡처한다(`pending_surface_screenshot` → `GpuState::capture_surface_to_png`). swapchain/present/가시 프레임/focus 를 전혀 건드리지 않으며, 공유 렌더러의 projection uniform 을 오프스크린 크기로 잠시 retarget 한 뒤 `self.size` 로 복원한다. 배경 탭·다른 workspace·비-focus 창의 surface 도 데이터가 보존돼 있어 캡처된다.
4. **캡처 범위** — surface 캡처는 콘텐츠(터미널 그리드)만, window 캡처는 창 전체 프레임(chrome 포함)이다.
5. **v1 은 터미널만** — surface 오프스크린 캡처는 터미널 kind 만 지원한다. egui 패널(explorer/markdown/image/html)·plugin-mesh·native webview 는 범위 밖이며 명확한 에러를 반환한다(webview 는 OS 합성이라 GPU readback 원천 불가).
6. **local_only 유지** — 임의 경로 파일 쓰기 표면이므로 `plugin_callable=false`(plugin 미노출). 로컬 CLI/client 만 호출한다. 보안은 연결 경계(로컬 소켓)에 위임한다.
7. **debug `ui.screenshot` 흡수** — 기존 debug-only 등록·핸들러를 제거하고 release 핸들러로 흡수한다(별칭·중복 없음). CLI `tasty screenshot --path <png> [--surface <id>] [--window <id>]` 를 신설한다.

## Consequences

- **얻은 것**: 스크린샷이 release 에서 동작하는 정식 에이전트 기능이 된다. focus 를 바꾸지 않고 대상 창/surface 를 ID 로 직접 캡처할 수 있어(배경 탭·비-focus 창 포함) 원칙 #1·#3 을 지킨다. 오프스크린 surface 캡처는 자체 렌더라 창 캡처보다 견고한 프리미티브다(최소화·가려진 창의 redraw 미도달로 인한 stall 이 없다).
- **잃은 것**: v1 은 터미널 surface 만 오프스크린 캡처한다 — egui 패널/plugin/webview 는 아직 지정 캡처 불가(에러). window 전체 캡처로는 잡히지만 개별 surface 로는 아니다.
- **운영 비용 / 유지 부담**: 오프스크린 경로가 공유 렌더러의 projection uniform 을 임시 변경·복원하는 stateful 절차라, 렌더러 uniform/scissor 규약이 바뀌면 이 경로도 함께 손봐야 한다. 캡처는 `render()` 안에서 동기 readback(poll Wait)이라 드물게만 쓰는 전제다. surface 캡처는 unfocused 색으로 렌더한다(정적 캡처에 커서/선택/IME 오버레이 미포함).

## Alternatives Considered

- **focus 종속 유지 + debug 격리 존치**: 승격을 포기하는 안. 스크린샷이 자기-관찰 기능(원칙 #2)임에도 release 에서 못 쓰고, 대상 지정 불가라는 사용성 한계가 남는다. focus 의존이 원칙 #1·#3 위반이라 애초에 격리 사유였고, 이를 걷어낼 수 있는 이상 유지할 이유가 없어 기각.
- **surface.screenshot 를 별도 메서드로 분리**: window 캡처(`ui.screenshot`)와 surface 캡처를 분리하는 안. IPC/CLI 표면이 늘고, 두 경로가 사실상 같은 캡처 프리미티브(오프스크린이 상위호환)라 단일 메서드 옵션 확장이 표면을 최소화한다 — 기각.
- **즉시 전 kind(egui 패널·plugin·webview) 지원**: v1 에서 모든 surface 종류를 오프스크린 캡처. egui 패널은 surface-scope egui 프레임 pass 신설, plugin-mesh 는 mesh 타깃/플러그인 렌더가 필요해 난이도가 크게 다르고 webview 는 원천 불가다. 터미널만으로 명확하게 시작하고 나머지는 후속(Phase 3)으로 미룸 — 기각(범위 분리).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- **Phase 3 착수** — egui 패널(explorer/markdown/image/html) 또는 비-terminal surface 의 오프스크린 캡처를 지원하게 될 때(surface-scope egui/mesh 렌더 경로 신설). 결정 5 의 "터미널만" 이 바뀐다.
- **캡처 색 정합 요구** — surface 오프스크린 캡처를 unfocused 색이 아니라 실제 focus 상태에 정합하는 색/오버레이(커서·선택 등)로 렌더할 필요가 생길 때.
- **window multi-view 정책 변경** — 창 다중 선택/기본 대상 규칙(결정 2)이 바뀌거나, "ID 생략 시 전 창 각각 캡처" 같은 배치 캡처가 요구될 때.
- **경로/권한 정책 변경** — `local_only` + 임의 경로 쓰기 전제가 바뀌어(예: 경로 sandbox, plugin 노출) 권한 레이어가 필요해질 때.

## References

- [identity](../identity.md) — 불가침 원칙 #1(사용자↔에이전트 행동 분리)·#2(에이전트 기능 IPC+CLI)·#3(포커스 독립성)
- [focus 정책](../design/policies/focus.md) — release 표면엔 포커스 변경 API 없음, 대상은 ID 로 직접 지정
- [debug-ipc](../dev-guide/debug-ipc.md) — debug 격리 IPC 판단 기준(에이전트 자기 작업 vs 사용자 입력 재현)
- [screenshot-methods](../ai-verification/screenshot-methods.md) — 승격 후 CLI/IPC 사용법, 오프스크린 캡처 동작
- 캡처 재사용 지점: `src/gfx/gpu/screenshot.rs`(`capture_frame_to_png` / `capture_surface_to_png`), `src/gfx/gpu/render_pass.rs`(`render_terminals`), `src/gfx/renderer.rs`(`render_all` / `resize`)
- 관련: [ADR-0032](0032-remote-attach-two-layer-split.md)(remote attach 2계층), [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md)(점유 모델 — readonly 렌더 대상 surface 캡처)
