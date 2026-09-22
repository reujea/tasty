# ADR-0028: Plugin 이 자기 프로세스에서 egui 를 tessellate 한 mesh 를 host 가 합성하는 out-of-process 렌더 채널 도입

- **Status**: Accepted
- **Accepted**: 2026-06-30
- **Date**: 2026-06-29
- **Tags**: plugin, render-channel, egui, epaint, mesh, ipc, shared-memory, surface-kind, popup, banner, host-rendered-removal, bundled-only, adr-0008, adr-0009
- **Group**: plugin-render

## Context

현재 plugin 렌더 채널은 `SurfaceKindRendering` enum 의 세 variant — `Remote` / `Host` / `Webview` — 가 단일 출처다 (`crates/tasty-plugin-manifest/src/types.rs:356-369`). 이 중 host-rendered 채널(`rendering = "host"`)로 동작하는 image / markdown surface 는 다음 한계를 안고 있다.

- **host-rendered surface 는 껍데기다.** manifest 등록·IPC namespace 만 점유할 뿐, 모델 타입·렌더 로직·편집 히스토리가 전부 본체에 산다. `ImagePanel` / `MarkdownPanel` 모델은 식별·네비게이션 메타데이터만 갖고(당시 `tasty-model` 의 `image_panel.rs` · `markdown_panel.rs` — 두 모델은 이후 제거됐다), 실제 픽셀·텍스처·스크롤·편집 상태는 host 의 `ImageView` / `MarkdownView` 가 소유한다. plugin 프로세스(`crates/tasty-plugin-image/src/main.rs:32-34`, `crates/tasty-plugin-markdown/src/main.rs:28-30`)는 `SurfaceResult::default()` 만 돌려주는 trampoline 이고, 렌더는 본체 egui 가 `downcast_mut` 으로 직접 수행한다(`src/adapters/ui/egui_panels.rs:154,204`). "plugin 으로 분리했다"는 의미가 사실상 없다.
- **`UiNode` DSL 은 고정 위젯 어휘다.** `Vbox`/`Hbox`/`Label`/`Button`/`Tree`/`Canvas` 등 고정 위젯만 있어(당시 `tasty-plugin-protocol` 의 `ui_tree.rs` — 이 DSL 은 이후 제거됐다), 흐르는 rich-text·인라인 혼합 서식(bold+code+link)·인라인 클릭 링크·테이블을 표현할 어휘가 없다. markdown 렌더(헤딩/리스트/인라인 코드/링크/이미지/테이블)는 이 DSL 로 옮길 수 없다. 그래서 현재 host 가 직접 그린다.
- **Canvas(SharedBuffer 픽셀)는 고정 해상도 래스터다.** 픽셀 프레임버퍼를 plugin 이 직접 채우는 경로(`CanvasTextureCache`)는 완비돼 있으나, DPI/줌 변화에서 텍스트가 깨지고 plugin 이 폰트·레이아웃을 스스로 래스터화해야 한다.
- **같은 한계가 surface 를 넘어 popup·banner 에도 적용된다.** plugin 팝업(git-viewer·clipboard-viewer)은 `UiNode` tree 를 host 가 `render_popup_tree` 로 그리므로 위 DSL 어휘 한계를 그대로 안고(git-viewer 의 섹션 strip·diff well·2줄 행 등 디자인을 표현 불가), banner 는 아예 plugin 이 기여할 채널이 없다. 즉 host 가 plugin-content 를 대신 그리는 구조(host-rendered `Host` + UiNode `Remote` 렌더) 전반이 의도된 아키텍처가 아니다.

반면, "plugin 이 만든 GPU 데이터를 host 가 egui 로 합성"하는 픽셀 파이프라인(Canvas + SharedBuffer + `CanvasTextureCache` + `register_native_texture`)은 **이미 픽셀 수준으로 완비·테스트**돼 있다. 같은 경로를 픽셀에서 벡터(mesh)로 끌어올릴 토대가 존재한다. host 자신의 프레임을 그리는 호출 경로(`ctx.tessellate` → `egui_renderer.update_buffers/render`, `src/gfx/gpu/egui_bridge.rs:148,200,208,231`)도 production 이다.

## Decision

**plugin 이 자기 프로세스에서 egui 를 돌려 tessellate 한 paint 출력 `(Vec<ClippedPrimitive>, TexturesDelta, pixels_per_point)` 을 IPC 로 host 에 보내고, host 의 `egui_wgpu::Renderer` 가 surface·popup·banner 영역에 래스터화하는 out-of-process egui mesh 렌더 채널(이하 채널 B)을 도입한다. 이 채널은 plugin-content 를 그리는 _유일 경로_ 로 삼으며, 기존 host-rendered(`Host`)·UiNode(`Remote`) 렌더 경로는 전환 완료 후 전면 제거한다(html 의 `Webview`, host 자체 builtin 팝업은 plugin-content 가 아니므로 유지).** 이 방향(surface 한정이 아니라 popup·banner 포함 + host-rendered 제거)은 사용자가 확정했다 — 본 ADR 의 Status 가 Proposed 인 것은 ADR Accept 절차(사용자 승인) 때문이며, 채택 방향 자체에 미정 여지를 두는 것이 아니다.

세부 결정:

- **tessellate 는 plugin 이 한다.** 텍스트 tessellation 은 plugin 이 소유한 `Fonts`/galley 에 의존하므로, host 는 plugin 의 폰트 atlas 없이 text shape 를 tessellate 할 수 없다. plugin 이 `ctx.run(...)` → `FullOutput` → `ctx.tessellate(...)` 까지 마친 mesh 를 보낸다. un-tessellated `Shape` 를 보내 폰트 동기화를 host 로 떠넘기는 변형은 기각한다.
- **taxonomy 확장.** `SurfaceKindRendering` 에 4번째 variant `EguiMesh`(와이어 키 `"egui-mesh"`)를 추가하고, 같은 mesh 메커니즘을 **popup·banner 렌더 채널에도** 적용한다(팝업은 현재 UiNode tree, banner 는 **신규 plugin 기여 채널** — 기존엔 plugin banner 경로가 없다). 전환 기간엔 기존 `Host`/`Remote`(UiNode) 와 공존하지만 이는 **과도기일 뿐**이며, 전환 완료 시 `Host`·`Remote`(UiNode) 는 제거되어 plugin-content 채널은 `EguiMesh`(+image 비트맵의 Canvas) 와 html 의 `Webview` 만 남는다.
- **개방 정책 = bundled 전용.** host-rendered 와 동일 패턴의 `(kind, plugin_id)` 화이트리스트(당시 `engine/surface_registry/host_rendered.rs` 미러 — 그 파일은 이후 제거됐다) + epaint 버전 pin + `api_version` gate 로 제한한다. user plugin 개방은 epaint 와이어 포맷이 host·plugin 동일 컴파일을 강제하는 동안 보류한다(1.0 이후 재검토).
- **scope = 단계적 전환 후 host-rendered 전면 제거.** 순서: (1) **surface** — markdown(순수 위젯, 비트맵 불필요)을 mesh 만으로 먼저 검증 → image 는 비트맵=SharedBuffer/Canvas + 툴바·핸들 chrome=mesh 하이브리드. (2) **popup** — git-viewer(+새 디자인 자가 렌더)·clipboard-viewer 를 mesh 팝업으로. (3) **banner** — plugin 이 banner 를 띄우는 신규 mesh 채널 신설. (4) **제거** — 위가 전부 전환되면 host-rendered(`Host`) + UiNode(`Remote`) 렌더 경로(`ui_tree_render.rs`/`render_popup_tree`/`egui_panels.rs` 직접 렌더)와 `UiNode` DSL 을 제거한다. 현 host-rendered 구현은 의도된 아키텍처가 아니므로 잔존시키지 않는다.
- **인프라 재사용.** 텍스처(폰트 atlas·이미지)는 기존 SharedBuffer / `CanvasTextureCache` 채널을 재사용하고(작은 image delta 는 inline 허용), mesh 는 binary stream frame(`crates/tasty-ipc/src/stream.rs`, `StreamTag::Data`)으로 보낸다. plugin→host `surface.paint_frame`, host→plugin `surface.set_context`(size/ppp/raw input) 신규 메시지를 둔다.

## Consequences

- **얻은 것**:
  - 풀 egui 표현력(rich-text·테이블·인라인 링크) — `UiNode` DSL 의 어휘 한계가 사라진다.
  - DPI 선명도 — 벡터 mesh 라 줌·고DPI 에서 텍스트가 깨지지 않는다(Canvas 픽셀 래스터 대비).
  - 격리·권한·크래시 격리 유지 — plugin 은 여전히 별도 프로세스이며 권한 게이트(`[memory]` 등)와 크래시 경계가 그대로 산다.
  - 인프라 대부분 재사용 — host 소비 경로, SharedBuffer/`CanvasTextureCache`, binary stream 이 이미 production.
  - 채널 일원화 — 전환 완료 시 host 가 plugin-content 를 대신 그리던 두 경로(`Host` 직접 렌더 + UiNode `Remote` 렌더)와 `UiNode` DSL 이 사라져, plugin-content 렌더 표면이 `EguiMesh` 하나로 수렴(유지부담·이중 경로 혼란 제거).
- **잃은 것**:
  - 프레임당 mesh IPC 비용 — 상호작용 중(타이핑·드래그)에는 프레임마다 mesh 를 직렬화·전송한다(정적 화면은 invalidate 시에만).
  - epaint 버전 결합 — host 와 모든 bundled plugin 이 동일 epaint feature set 으로 컴파일돼야 와이어가 일치한다. 이것이 user plugin 미개방·버전 pin 의 근거다.
  - epaint serde 와이어 유지부담 — 현 빌드는 epaint serde 가 비활성이라(아래 References) paint 타입 직렬화가 공짜가 아니다. serde feature 가 일부 타입을 못 덮으면 그 타입에 한해 POD 매핑 레이어를 유지해야 한다.
- **운영 비용 / 유지 부담**:
  - epaint major 변경 시 와이어 포맷 재검증.
  - serde 미커버 타입(`ImageData::Font`/`Color` 등)에 대한 POD 매핑 레이어 유지.
  - 입력 forward 프로토콜·텍스처 atlas lifecycle·TextureId remap 구현·유지.
  - **identity 점검**: 본 채널은 *사용자 surface 상호작용 경로*(paint/입력)이지 에이전트 제어 API 가 아니다. 입력 forward 가 "에이전트 IPC 가 사용자 입력을 재현"하는 경로를 새로 열지 않도록 구현 단계에서 경계를 지킨다(`docs/identity.md` 원칙 1·3).

## Alternatives Considered

- **A. in-process dylib egui**: plugin 이 host 프로세스 안에서 `ui.label()` 을 직접 호출. 격리·권한 모델을 위배하고(권한 게이트는 plugin 이 별도 프로세스라는 전제 위에 선다, `docs/concepts/plugins.md:48`), 크래시가 host 로 전파되며, Rust ABI/egui 버전을 락한다. 진입로 자체가 `Entry::Process` 단일 variant 라(`types.rs:73-82`) 구조와도 충돌 → **기각**.
- **Canvas-only(고정 해상도 픽셀 프레임버퍼)**: plugin 이 픽셀을 직접 채워 SharedBuffer 로 전송. DPI/텍스트 품질 한계로 markdown 같은 벡터 콘텐츠에 부적합 → image 비트맵에만 부분 채택(하이브리드).
- **`UiNode` 어휘 확장(rich-text 노드 추가)**: DSL 에 인라인 서식·테이블 노드를 계속 더하는 길. 사실상 egui 재구현으로 DSL 이 비대해지고, 표현력을 영원히 따라가야 함 → **기각**.
- **host builtin 강등(plugin 폐기)**: image/markdown 을 explorer 선례(`src/core/surface_registry/builtins.rs`)처럼 host builtin 으로 굳히는 길. 기술적으로 가능하나 "plugin 이 내용을 소유한다"는 사용자 의도와 정반대 → **기각**(기록만).
- **C. wgpu 텍스처/서피스 직접 공유(외부 GPU 메모리)**: plugin 이 자기 wgpu device 로 그린 텍스처를 DMA-BUF / D3D11 shared handle / IOSurface 로 host 와 GPU 레벨 공유. IPC 직렬화는 0 이지만 플랫폼별 GPU 공유 API 가 제각각이고 wgpu 24 의 cross-API external texture 지원이 미성숙하며 크로스플랫폼 1급(CLAUDE.md 원칙4) 부담이 크다 → **기각**(image 거대 비트맵 특수 최적화로 재검토 여지).

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- spike 실측 프레임 비용이 체감 임계를 초과한다(상호작용이 거슬릴 정도의 latency).
- user plugin 에 egui-mesh 채널을 개방하라는 요구가 발생한다(→ epaint ABI 안정화 / 버전 협상 설계 필요).
- epaint major 버전 변경으로 와이어 포맷이 파손된다.
- 외부 primitive 의 TextureId 충돌·수명 관리가 spike 에서 깨끗하게 풀리지 않는다(→ 텍스처 remap 설계 재고).
- wgpu external texture 공유가 충분히 성숙한다(→ Alternative C 재평가).

## References

- 코드 근거(**결정 당시의 좌표다** — 아래 파일 여럿은 그 뒤 옮겨지거나 제거됐다): `crates/tasty-plugin-manifest/src/types.rs:356-369,348-352,73-82`; `crates/tasty-plugin-protocol/src/{protocol.rs:51-65,331-338,227,145-146, ui_tree.rs:14-151,279-348}`; `src/engine/surface_registry/{host_rendered.rs:14-19, builtins.rs:67-122,155-193}`; `src/gfx/gpu/{canvas_prepare.rs:46,80-107, canvas_texture.rs:73,117,228,276, egui_bridge.rs:148,200,208,231, render_pass.rs:198}`; `src/gfx/gpu.rs:50,314`; `crates/tasty-shm/{lib.rs, footer.rs}`; `crates/tasty-ipc/{protocol.rs, stream.rs:27}`; `crates/tasty-model/src/{image_panel.rs:17, markdown_panel.rs:6}`; `src/adapters/ui/egui_panels.rs:154,204`.
- 관련 ADR: [0008](0008-inline-graphics-protocols-deferred.md)(인라인 그래픽 보류), [0009](0009-plugin-sandbox-deferred.md)(plugin sandbox 보류), [0026](0026-clipboard-history-removal-plugin-direct-read.md)(plugin 직접-read).
- 부분 개정: [0030](0030-image-egui-mesh-bitmap-texture.md) (image 의 Canvas-하이브리드 조항 개정), [0065](0065-markdown-webview-render-channel.md) (markdown 을 EguiMesh 첫 선례(B1)로 명시한 조항·최종 채널 구성 조항 중 markdown 관련 부분 개정 — Webview 로 전환).
- 버전: egui/egui-wgpu/egui-winit/egui_extras 0.31(`Cargo.toml:196-200`), epaint 0.31.1(serde 비활성, `Cargo.lock` 직접 확인), wgpu 24, winit 0.30.
