# ADR-0030: image surface 는 mesh-only(비트맵=egui 텍스처) 로 전환 — ADR-0028 의 image Canvas-하이브리드 조항 개정

- **Status**: Accepted
- **Date**: 2026-07-01
- **Tags**: plugin, render-channel, egui, epaint, mesh, image, surface-kind, bitmap, texture, host-rendered-removal, adr-0028
- **Group**: plugin-render

## Context

[ADR-0028](0028-plugin-egui-mesh-render-channel.md) 은 plugin-content 렌더를 out-of-process egui-mesh 채널로 일원화하기로 결정하면서, image surface 만은 예외적으로 **하이브리드**로 두었다 — "거대 비트맵 = 픽셀 경로(SharedBuffer/Canvas, `CanvasTextureCache` 재사용) 유지, 툴바·핸들 chrome = mesh". 그 전제는 (a) 재사용 가능한 Canvas substrate 가 존재하고, (b) 거대 비트맵을 mesh 로 보내면 per-frame 직렬화 비용이 과하다는 우려였다.

B2 구현 착수 시 두 전제를 실측 검증한 결과 둘 다 성립하지 않았다.

- **재사용할 Canvas substrate 가 실재하지 않는다.** `Canvas` 는 `UiNode` DSL 의 위젯일 뿐이고(`CanvasTextureCache` 는 그 위젯의 GPU 경로), surface 단위로 "비트맵 Canvas 레이어를 mesh chrome *아래* 합성"하는 호스트 경로는 없다. 전환 이전의 image 조차 `CanvasTextureCache` 를 쓰지 않고 host egui 의 `load_texture` 로 직접 그렸다(당시 host 경로는 `adapters/ui/surface/image/view.rs` — image 기능은 이후 `crates/tasty-plugin-image`로 전부 이전되어 이 host 경로 자체가 사라졌다, 현재는 `crates/tasty-plugin-image/src/render.rs`). 하이브리드를 문구대로 구현하려면 surface 단위 Canvas 레이어 + z-order + 입력 라우팅 분리를 **새로** 신설해야 하며, 이는 ADR-0028 이 없애려던 "이중 렌더 경로"를 오히려 하나 더 만드는 것이다.
- **mesh 채널이 이미 임의 텍스처를 native 로 처리한다.** host 측 egui-mesh 합성기는 surface 마다 전용 `egui_wgpu::Renderer` 를 두고 `TexturesDelta.set` 의 **모든 텍스처**(폰트 atlas + 임의 이미지)를 업로드한다(`src/gfx/gpu/egui_mesh_prepare.rs`). 와이어(`mesh_wire`)는 `ImageData::Color`(RGBA 이미지)와 `ImageDelta.pos`(부분 sub-rect 업로드)를 완전 지원한다. 즉 비트맵을 plugin egui `Context` 에 텍스처로 올리면 폰트 atlas 와 **동일 경로**로 1회 업로드되어 전용 Renderer 에 캐시되고, 이후 프레임은 텍스처를 참조하는 quad(정점 몇 개)만 나른다. egui 의 `TexturesDelta` 는 변경 시에만 픽셀을 재전송하므로 정적 화면·pan/zoom 에는 픽셀 재전송이 없다(SDK 의 출력 해시 가드로 프레임 자체도 생략). "거대 비트맵 per-frame 직렬화" 우려의 전제가 성립하지 않는다.

## Decision

**image surface 도 markdown(B1)과 동일하게 mesh-only 로 렌더한다.** plugin 이 비트맵을 자기 egui `Context` 의 텍스처로 올리고(폰트 atlas 와 같은 `TexturesDelta` 채널), viewer/paint chrome(control bar·paint bar·8 handles·zoom group)과 함께 tessellate 해 host 가 합성한다. **별도의 Canvas/SharedBuffer 비트맵 레이어는 두지 않는다** — ADR-0028 의 image "Canvas-하이브리드" 조항을 이 결정으로 개정한다. ADR-0028 의 나머지(egui-mesh 채널 일원화, markdown/popup/banner, host-rendered 전면 제거 방향)는 그대로 유효하다.

세부:

- **비트맵 = egui 텍스처.** 원본 이미지 + 편집 오버레이(draw layer) + floating selection 을 plugin egui `Context` 의 텍스처로 올린다. `≤100% fit / >100% pan` zoom 과 입력(brush 드래그·pan·wheel·Esc·핸들 드래그)은 plugin closure 안 egui 상호작용으로 처리한다.
- **편집 상태는 plugin 소유.** brush/undo·redo/paste→floating→commit·Esc, 히스토리, 저장(PNG 합성)은 plugin(`ImageDoc`)이 소유한다. `image.save`/`export_png`/`paste`/`next`/`prev` 는 plugin 이 직접 처리하고, `image.open`(surface 변환)·`image.list`(host surface 열거)만 host 로 trampoline 한다.
- **거대 비트맵 편집 최적화.** brush stroke 로 draw layer 가 매번 통째로 재업로드되는 비용은 `ImageDelta.pos` 부분(sub-rect) 업로드로 줄일 수 있다(현재는 전체 재업로드, 필요 시 후속 최적화 — defer).
- **호스트 stand-in.** image surface 는 다른 egui-mesh kind 와 동일하게 host `EguiMeshSurface` stand-in(파일·display_name·영속화)으로 표현된다. host 의 `ImageView`/`ImagePanel` 직접 렌더 경로는 C1(host-rendered 채널 제거)까지 dead code 로 컴파일만 유지된다.

## Consequences

- **얻은 것**: 채널 일원화 완성에 근접(image 도 markdown 과 같은 mesh-only) — 새 Canvas-under-mesh 합성 인프라를 만들지 않는다. 호스트/프로토콜 신규 코드 0(기존 mesh 텍스처 채널 재사용). DPI 선명 chrome. 격리·권한·크래시 경계 유지(plugin 별도 프로세스).
- **잃은 것**: 거대 비트맵의 **최초 업로드**는 여전히 큰 1회 전송(폰트 atlas spike 와 동일 성격, 이후 캐시). paint 중 draw layer 재업로드는 부분 업데이트 최적화 전까지 이미지 크기에 비례.
- **운영 비용 / 유지 부담**: 텍스처 lifecycle(set/free)은 전용 Renderer 가 frame 경계에서 처리(기존 인프라). 부분 업데이트 최적화는 필요 시 후속.

## Alternatives Considered

- **A. ADR-0028 문구 그대로 — true Canvas 하이브리드**: surface 단위 Canvas 비트맵 레이어를 신설해 mesh chrome 아래 합성. 재사용할 substrate 가 없어 z-order·좌표 매핑·입력 라우팅 분리를 새로 만들어야 하고, ADR-0028 이 없애려는 이중 렌더 경로를 하나 더 만든다 → **기각**.
- **B. 외부 GPU 텍스처 공유(Alternative C of ADR-0028)**: 플랫폼별 GPU 공유 API 미성숙·크로스플랫폼 부담 → **기각**(ADR-0028 과 동일 사유, 거대 비트맵 특수 최적화로만 재검토 여지).

## Reconsideration Triggers

- paint 중 거대 비트맵 재업로드 비용이 체감 임계를 넘는다(→ `ImageDelta.pos` 부분 업데이트 도입, 그래도 부족하면 Alternative B 재평가).
- 최초 업로드 spike 가 큰 이미지에서 문제된다(→ 다운스케일/타일링 검토).
- ADR-0028 이 재검토/Supersede 되어 egui-mesh 채널 자체가 바뀐다.

## References

- 개정 대상: [ADR-0028](0028-plugin-egui-mesh-render-channel.md) (image Canvas-하이브리드 조항).
- 코드 근거: `src/gfx/gpu/egui_mesh_prepare.rs`(텍스처 업로드·전용 Renderer), `crates/tasty-plugin-protocol/src/mesh_wire.rs`(`ImageData::Color`·`ImageDelta.pos`), `crates/tasty-plugin-image/src/{main.rs,doc.rs,render.rs}`, `crates/tasty-plugin-markdown/src/main.rs`(B1 선례), `src/core/surface_registry/egui_mesh.rs`(화이트리스트).
