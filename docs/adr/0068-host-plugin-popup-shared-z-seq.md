# ADR-0068: Host popup ↔ plugin popup z-order — 공유 z_seq + 조건부 GPU pass·sublayer 순서

- **Status**: Accepted
- **Date**: 2026-08-12
- **Tags**: popup, plugin, z-order, gpu-rendering, egui
- **Group**: ui-theme-gallery

## Context

Host `PopupManager` 가 그리는 popup(`file_picker` 등)과 plugin 이 egui-mesh 로 그리는 popup(예: markdown 파일열기)은 서로 다른 두 개의 독립 렌더 경로를 탄다:

- Host popup shell/content: `run_egui_frame` 안에서 egui paint job 으로 tessellate 되어 `render_egui_pass`(wgpu, `LoadOp::Load`)로 그려진다.
- Plugin popup shell: 같은 `run_egui_frame` 안에서 `ctx.layer_painter()` 로 그려지지만(raw painter), 콘텐츠는 별도 `wgpu::Renderer` pass(`render_egui_mesh_popups`)로 host egui pass 와 **텍스처/리소스 독립적**으로 합성된다.

버그: `render_egui_pass` 가 항상 `render_egui_mesh_popups` 보다 먼저 실행돼, plugin popup 콘텐츠가 실제 open/click 순서와 무관하게 항상 host popup 위에 그려졌다 — [popup.md 규칙 7](../design/systems/popup.md#8대-규칙)("나중에 열리거나 클릭된 것이 앞")을 위반.

이 버그를 고치려는 첫 시도(egui-mesh popup ↔ host popup 의 `draw_popups`/`draw_plugin_popups` 호출 순서를 조건부로 스왑)는 효과가 없었다. egui 0.31.1 은 `ctx.layer_painter()` 로 그린 raw layer 를 `Memory::Areas::order`(같은 tier 안 실제 그리기 순서를 정하는 Vec)에 전혀 등록하지 않는다 — `egui::Area`/`Window` 기반 위젯만 `Areas::set_state`/`move_to_top` 을 통해 이 Vec 에 들어간다. `end_pass()` 는 `area_order` 에 있는 레이어를 먼저 drain 한 뒤, 거기 없는("미등록") 레이어를 **별도의 HashMap 순회**로 append 한다 — 즉 host popup 과 plugin popup 셸처럼 둘 다 raw painter 인 레이어끼리는, 같은 프레임 안에서 어느 걸 먼저 호출했는지가 최종 그리기 순서에 전혀 영향을 주지 않는다(비결정적). 실측(스크린샷 픽셀 비교)으로 스왑 후에도 plugin popup 의 반투명 scrim 이 계속 host popup 위에 남는 것을 확인해 이 사실을 확정했다.

## Decision

1. **단일 z 비교 축**: host popup(`PopupState`) 과 plugin popup(`PopupInstance`) 양쪽에 `z_seq: u64` 필드를 추가하고, `tasty-host-plugin` 크레이트(이미 본체가 단방향 의존)에 두는 전역 `AtomicU64` 카운터 `tasty_host_plugin::next_popup_z_seq()` 에서 open/click 시점마다 값을 뽑아 기록한다. 매 프레임 두 진영의 열린 popup 중 최댓값끼리 비교해(`host_popup_should_render_on_top`) 승자를 정한다.
2. **Shell 순서**: raw layer 는 `Areas::order` 미등록 상태라 호출 순서로 못 고친다는 사실 위에서, 기존 `enforce_foreground_z_order`(banner/modifier-hint 에 이미 쓰던 패턴)와 동일한 `ctx.set_sublayer(parent, child)` 를 진 쪽→이긴 쪽 자식으로 호출해(`enforce_host_plugin_popup_z_order`) 강제한다. `set_sublayer` 호출 자체가 parent/child 를 모두 `Areas::order` 에 강제 등록하므로, 이 호출이 일어나는 프레임에 한해 두 레이어 모두 "raw 미등록" 상태를 벗어난다.
3. **Content 순서**: `render_egui_pass`(host) 와 `render_egui_mesh_popups`(plugin) 는 텍스처/리소스가 서로 독립이라 순수 합성 순서 문제로 취급 가능(`wgpu::LoadOp::Load` 는 누적 블렌딩) — 같은 승패 결과로 두 pass 호출 순서를 `render_egui_pass_and_mesh_popups`(`src/gfx/gpu/render_pass.rs`) 안에서 뒤바꾼다.
4. **자기 자신을 가리지 않게**: plugin popup 콘텐츠가 shell 보다 먼저 그려지는 경우(host popup 이 이긴 경우), 기존처럼 shell 배경 전체를 단일 사각형으로 칠하면 방금 그린 자기 콘텐츠를 스스로 덮는다. shell 배경을 `content_rect` 를 제외한 4분할 사각형으로 그리는 `paint_shell_background_excluding_content` 로 대체해, 어느 순서로 합성되든 자기 콘텐츠를 가리지 않게 했다.
5. **클릭도 순서에 반영**: 규칙 7 은 "클릭된 것도 앞" 이므로, plugin popup 클릭 시에도 `touch_popup_instance_z`(host: 기존 `bring_to_front` 확장)로 같은 카운터에서 새 값을 받는다.

## Consequences

- **얻은 것**: host popup 과 plugin popup 이 open/click 시점 기준 하나의 z-order 축으로 비교되어 규칙 7 이 host↔plugin 경계를 넘어 성립한다. banner/modifier-hint 에 쓰던 `set_sublayer` 패턴을 재사용해 새 개념을 추가하지 않았다.
- **잃은 것**: plugin popup 셸이 "항상 Foreground tier 최상단"이던 기존 불변식([input-layer.md 의 `plugin_bridge/popup_render.rs` 절](../architecture/input-layer.md))이 host popup 이 더 나중에 열린 경우에 한해 깨진다 — 문서를 이 조건부 동작에 맞춰 갱신했다.
- **운영 비용 / 유지 부담**: `set_sublayer` 는 1단 들여쓰기만 지원해(egui 문서상 중첩 시 동작 unspecified), 이 설계는 host 묶음 vs plugin 묶음의 2-그룹 star 패턴만 지원한다 — plugin popup 이 여러 개 열렸을 때 그들끼리의 shell 상대 순서는 이 메커니즘 밖이다(GPU 콘텐츠는 z_seq 로 정렬되지만 shell 은 아님). 또한 host popup 레이어가 `modifier_hint_layer` 와 `plugin_popup_layer` 양쪽의 자식이 되는 극단적 동시-경합 케이스는 `set_sublayer` 의 nesting-unspecified 특성상 비결정적일 수 있음을 감수한다(기존 코드베이스에도 유사 전례 있음, `enforce_foreground_z_order` 의 banner/modifier-hint 두 그룹을 서로 엮지 않은 이유와 동일 제약).

## Alternatives Considered

- **`draw_popups`/`draw_plugin_popups` 호출 순서 스왑**: 최초 시도. egui 가 raw painter 레이어를 `Areas::order` 로 관리하지 않아 실측상 전혀 효과 없음 — 폐기.
- **Plugin popup 도 `egui::Area` 로 등록 전환**: `Areas::order` 의 안정 정렬에 자연히 편입시켜 `set_sublayer` 없이 해결하는 방안. Plugin popup 은 전체화면 scrim + 다른 모든 Foreground 레이어보다 항상 위여야 하는 기존 의도적 예외([input-layer.md](../architecture/input-layer.md#plugin_bridgepopup_renderrs--의도적-예외))가 있어, Area 등록 시 자연 등록 순서에 따라 그 예외가 깨질 위험이 있고 입력 라우팅(hover/scroll)도 plugin 은 raw event forward 방식이라 Area 등록의 원 동기가 적용되지 않음 — 채택하지 않음.
- **`PopupManager`/`PluginManager` 완전 단일화(popup 레지스트리 통합)**: z-order 뿐 아니라 포커스·스코프·닫힘 계약까지 한 곳으로 합치는 대규모 리팩터. 이번 버그는 z-order 축 하나만 필요해 범위 밖으로 판단 — 최소 설계(공유 카운터)로 충분.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- Plugin popup 을 동시에 2개 이상 열어 그들끼리의 shell 상대 순서(호스트와 무관하게)가 사용자에게 노출되는 요구가 생길 때 — 현재 설계는 host 대 plugin 2-그룹 비교만 지원.
- `enforce_foreground_z_order`(banner/modifier-hint) 와 `enforce_host_plugin_popup_z_order`(host popup/plugin popup) 의 그룹이 서로 겹쳐야 하는 시나리오가 생길 때 — `set_sublayer` 1단 제약을 넘어서므로 별도 메커니즘이 필요.
- egui 버전 업그레이드로 `Areas::order`/`set_sublayer` 의미가 바뀔 때.

## References

- [popup.md § Host ↔ Plugin popup z-order](../design/systems/popup.md#host--plugin-popup-z-order)
- [input-layer.md (c)/(d) 및 `plugin_bridge/popup_render.rs` 절](../architecture/input-layer.md)
- `src/gfx/gpu/egui_bridge.rs::enforce_host_plugin_popup_z_order`, `host_popup_should_render_on_top`
- `src/gfx/gpu/render_pass.rs::render_egui_pass_and_mesh_popups`
- `src/plugin_bridge/popup_render.rs::paint_shell_background_excluding_content`
- `crates/tasty-host-plugin/src/manager.rs::next_popup_z_seq`
