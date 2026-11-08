//! 플러그인의 mesh·텍스처 출력을 host 화면에 합성한다. TextureId 충돌을 피하려 대상별 Renderer를 사용한다.
//! 도형·클립을 화면 좌표로 옮기고 대상 영역 안으로 제한한다.
//!
//! surface 자원은 보이는 동안이 아니라 레이아웃에 존재하는 동안 유지한다. 숨겨진 탭도
//! 도착 프레임을 디코드해 텍스처 변경을 반영하고, 실제 합성만 생략한다.
//!
//! 공유 버퍼는 최신 프레임만 보관하므로 중간 텍스처 변경을 놓칠 수 있다. 전체 텍스처를
//! 받았거나 순서가 이어질 때만 delta를 적용하며, 부분 변경의 범위도 기존 크기와 대조한다.
//! 순서가 끊겼더라도 기존 콘텐츠가 있고 참조 텍스처가 모두 상주하며 변경 범위가 맞으면
//! 도형만 채택한다. 이때 텍스처 내용은 최신임을 보장할 수 없어 last_seq를 갱신하지 않고
//! 전체 재전송을 계속 요청한다. 범위를 벗어나거나 참조가 없으면 도형도 보류한다.
//! 디코드 실패·ppp 불일치 등은 기존 캐시를 유지하고 다음 프레임에 다시 시도한다.

mod full_request;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use egui::epaint::textures::TexturesDelta;
use egui::epaint::{ClippedPrimitive, Primitive, TextureId};

use super::GpuState;
use crate::app::plugin_display::PluginDisplay;
use crate::model::PhysicalRect;
use crate::state::MainViewState;
use full_request::FullRequestPacer;

/// 디코드 ppp 와 host ppp 의 허용 오차. float 비교라 작은 epsilon.
const PPP_EPS: f32 = 1.0e-3;

/// 레이아웃에 존재하는 surface의 Renderer·디코드 캐시. 숨겨진 동안에도 텍스처를 유지한다.
pub(super) struct EguiMeshRenderTarget {
    /// 이 surface 전용 egui_wgpu Renderer (TextureId 네임스페이스 격리).
    renderer: egui_wgpu::Renderer,
    /// 디코드된 surface-local primitives (평행이동 전 원본).
    primitives: Vec<ClippedPrimitive>,
    /// 디코드된 pixels_per_point.
    ppp: f32,
    /// 마지막으로 디코드한 footer generation.
    generation: u64,
    /// 텍스처 delta까지 적용한 마지막 frame_seq. 도형만 채택한 프레임은 반영하지 않는다.
    last_seq: u64,
    /// 상주 텍스처의 크기. 전체 이미지로 갱신하며 부분 변경의 경계 검사·오래된 텍스처 정리에 사용한다.
    live_textures: HashMap<TextureId, [usize; 2]>,
    /// 전체 텍스처를 기다리는 상태. 이 값은 반복 로그만 막는다. 로컬 plugin 경로는 요청을 매 tick
    /// 다시 보내고, attach mirror 경로는 `full_request`가 정한 간격으로 보낸다.
    awaiting_full: bool,
    full_request: FullRequestPacer,
    /// 한 번이라도 유효 mesh 를 디코드했는가. false 면 합성 skip.
    has_content: bool,
    /// window-global 좌표로 평행이동/클립한 합성용 primitives 캐시.
    translated: Vec<ClippedPrimitive>,
    /// `translated` 가 유효한 (generation, rect bits) 키. 둘 중 하나라도 바뀌면 재계산.
    translated_key: Option<(u64, [u32; 4])>,
}

impl EguiMeshRenderTarget {
    fn new(renderer: egui_wgpu::Renderer) -> Self {
        Self {
            renderer,
            primitives: Vec::new(),
            ppp: 1.0,
            generation: 0,
            last_seq: 0,
            live_textures: HashMap::new(),
            awaiting_full: false,
            full_request: FullRequestPacer::default(),
            has_content: false,
            translated: Vec::new(),
            translated_key: None,
        }
    }

    /// 메타데이터상 텍스처 변경을 적용할 수 있는지 확인한다.
    fn chain_accepts(&self, frame_seq: u64, full_textures: bool) -> bool {
        chain_accepts(self.has_content, self.last_seq, frame_seq, full_textures)
    }
}

/// 전체 텍스처 프레임 또는 기존 콘텐츠의 연속 시퀀스이면 true다.
/// 실제 적용 전에는 디코드·경계 검사도 필요하며 도형만의 채택은 별도 판정한다.
fn chain_accepts(has_content: bool, last_seq: u64, frame_seq: u64, full_textures: bool) -> bool {
    full_textures || (has_content && frame_seq == last_seq.wrapping_add(1))
}

/// 도형이 참조하는 텍스처가 상주하는지 확인한다.
/// ID 존재만 보는 검사이므로 최신 내용이나 올바른 크기까지 보장하지 않는다.
fn all_textures_live(prims: &[ClippedPrimitive], live: &HashMap<TextureId, [usize; 2]>) -> bool {
    prims.iter().all(|p| match &p.primitive {
        Primitive::Mesh(m) => live.contains_key(&m.texture_id),
        Primitive::Callback(_) => true,
    })
}

/// 부분 변경이 상주 텍스처의 경계 안인지 확인한다. 부분 변경은 크기를 늘리지 않으므로
/// 전체 이미지 유실 후 큰 좌표가 오면 적용하지 않고 전체 재전송을 요청해야 한다.
/// 전체 이미지 변경은 크기를 새로 정하므로 기존 크기에 제한받지 않는다.
fn deltas_fit_live(delta: &TexturesDelta, live: &HashMap<TextureId, [usize; 2]>) -> bool {
    delta.set.iter().all(|(id, d)| match d.pos {
        None => true,
        Some([px, py]) => {
            let Some(&[tw, th]) = live.get(id) else {
                return false;
            };
            let [dw, dh] = d.image.size();
            px + dw <= tw && py + dh <= th
        }
    })
}

/// 디코드 결과 채택 판정. 연속 순서·경계가 맞으면 텍스처와 도형을 적용한다.
/// 순서가 끊겼으면 기존 콘텐츠·참조 상주·경계 일치를 모두 만족할 때 도형만 채택한다.
/// 나머지는 전체 재전송이 필요하다.
fn classify_decode(
    chain_ok: bool,
    has_content: bool,
    refs_live: bool,
    deltas_fit: bool,
) -> DecodeOutcome {
    if chain_ok {
        if deltas_fit {
            DecodeOutcome::Accepted
        } else {
            DecodeOutcome::NeedsFull
        }
    } else if has_content && refs_live && deltas_fit {
        DecodeOutcome::AcceptedStale
    } else {
        DecodeOutcome::NeedsFull
    }
}

/// [`decode_mesh_into_target`] 의 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeOutcome {
    /// mesh 채택 + 텍스처 delta 적용 완료(체인 연속 또는 full). `last_seq` 전진.
    Accepted,
    /// 도형만 채택했다. 텍스처 내용이 최신임을 보장할 수 없어 last_seq를 유지하고 전체 재전송을 요청한다.
    AcceptedStale,
    /// mesh 가 미상주 텍스처를 참조하거나 delta 가 경계를 벗어나 보류했다 — full 재전송이 필요.
    NeedsFull,
    /// 이번 frame 은 갱신 없이 넘어갔다(footer tear / ppp 불일치 등 일시적). 재요청
    /// 불필요 — 기존 캐시로 재합성하고 다음 tick 에 다시 시도한다.
    Deferred,
}

/// 활성 workspace 에서 egui-mesh surface 의 (surface_id, plugin_id, 물리 rect) 일람.
///
/// `surface_regions` 가 layout 에 실제 존재하는 surface 만 반환하고, 그중
/// 표시 query에서 mesh 모델로 분류된 것만 골라낸다. 화이트리스트 gate 에서 거부된
/// kind 는 애초에 `EguiMeshSurface` 로 생성되지 않으므로(registry 미등록) 여기 잡히지
/// 않는다 — registry 미등록 kind 의 합성을 시도하지 않는다(A1-S1 인계 점검).
pub(super) fn collect_egui_mesh_targets(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    terminal_rect: PhysicalRect,
    scale_factor: f32,
) -> Vec<(u32, String, PhysicalRect)> {
    let mut out: Vec<(u32, String, PhysicalRect)> = Vec::new();
    for (_pane_id, _pane_rect, regions) in
        state.surface_regions(engine, terminal_rect, scale_factor)
    {
        for r in regions {
            if let Some(ms) = engine
                .find_surface_by_id(r.id)
                .and_then(|surface| surface.mesh())
            {
                out.push((r.id, ms.plugin_id.clone(), r.rect));
            }
        }
    }
    out
}

/// 활성 workspace 에서 attach mesh mirror surface(`AttachMeshSurface`)의 (surface_id, 물리
/// rect) 일람. [`collect_egui_mesh_targets`]의 attach 대응 — plugin_id 는 로컬에 plugin
/// 프로세스가 없어 무의미하므로 반환하지 않는다.
pub(super) fn collect_attach_mesh_targets(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    terminal_rect: PhysicalRect,
    scale_factor: f32,
) -> Vec<(u32, PhysicalRect)> {
    let mut out: Vec<(u32, PhysicalRect)> = Vec::new();
    for (_pane_id, _pane_rect, regions) in
        state.surface_regions(engine, terminal_rect, scale_factor)
    {
        for r in regions {
            if engine
                .find_surface_by_id(r.id)
                .is_some_and(|surface| surface.attach_mesh().is_some())
            {
                out.push((r.id, r.rect));
            }
        }
    }
    out
}

/// primitives 를 surface origin 만큼 평행이동하고 clip_rect 를 surface 경계로 클립한다.
///
/// - `offset` (points): surface origin = (rect.x / ppp, rect.y / ppp).
/// - `bounds` (points, window-global): surface 의 화면 영역. clip 을 여기로 intersect 해
///   plugin 이 영역 밖을 그리려 해도 scissor 가 surface 안에 갇힌다.
fn offset_and_clip(
    prims: &[ClippedPrimitive],
    offset: egui::Vec2,
    bounds: egui::Rect,
) -> Vec<ClippedPrimitive> {
    prims
        .iter()
        .map(|p| {
            let clip_rect = p.clip_rect.translate(offset).intersect(bounds);
            let primitive = match &p.primitive {
                Primitive::Mesh(m) => {
                    let mut m = m.clone();
                    for v in &mut m.vertices {
                        v.pos += offset;
                    }
                    Primitive::Mesh(m)
                }
                // decode_paint 는 Callback 을 제거하므로 정상 경로엔 오지 않는다.
                // 방어적으로 그대로 통과(합성 시 무시됨).
                Primitive::Callback(c) => Primitive::Callback(c.clone()),
            };
            ClippedPrimitive {
                clip_rect,
                primitive,
            }
        })
        .collect()
}

/// WebView 오버레이 pass 에서 mesh 를 드러난 페이지 합집합으로 자른다. 페이지 여러 개와 겹치는
/// primitive 는 페이지마다 한 번씩 남고 겹치지 않으면 버린다. 좌표·메인 pass 캐시는 그대로다.
pub(super) fn clip_primitives_to_pages(
    prims: &[ClippedPrimitive],
    pages: &[egui::Rect],
) -> Vec<ClippedPrimitive> {
    prims
        .iter()
        .flat_map(|p| {
            pages.iter().filter_map(move |page| {
                let clip_rect = p.clip_rect.intersect(*page);
                clip_rect.is_positive().then(|| ClippedPrimitive {
                    clip_rect,
                    primitive: p.primitive.clone(),
                })
            })
        })
        .collect()
}

/// 공유 버퍼를 디코드한다. footer·버퍼 길이·ppp 검사에 실패하면 캐시를 유지한다.
/// 도형과 텍스처 채택 규칙은 classify_decode를 따른다. 도형만 채택하면 last_seq는 유지한다.
#[allow(clippy::too_many_arguments)] // reason: frame 디코드 컨텍스트 전체
fn decode_mesh_into_target(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &mut EguiMeshRenderTarget,
    raw: &[u8],
    generation: u64,
    frame_seq: u64,
    full_textures: bool,
    chain_ok: bool,
    host_ppp: f32,
    log_id: &str,
) -> DecodeOutcome {
    // footer::load의 길이 전제를 먼저 확인한다.
    if raw.len() < tasty_shm::footer::SIZE {
        tracing::warn!(
            target = log_id,
            actual = raw.len(),
            "egui-mesh prepare: buffer too small"
        );
        return DecodeOutcome::Deferred;
    }
    // SAFETY: raw는 페이지 정렬된 매핑 시작이며 길이는 위에서 확인했다.
    // 첫 8바이트는 양쪽 프로세스가 atomic footer로 사용한다.
    // 세대 불일치는 오래된 메타데이터 때문일 수도 있어 처리를 미룬다.
    // 세대가 같아도 이후 payload 쓰기를 배제하지는 못한다.
    let gen_now = unsafe { tasty_shm::footer::load(raw, Ordering::Acquire) };
    if gen_now != generation {
        return DecodeOutcome::Deferred;
    }
    let user = tasty_shm::footer::user_slice(raw);
    decode_mesh_bytes_into_target(
        device,
        queue,
        target,
        user,
        generation,
        frame_seq,
        full_textures,
        chain_ok,
        host_ppp,
        log_id,
    )
}

/// footer가 없는 payload의 공용 디코더. 로컬 공유 버퍼와 원격 TCP 수신 경로가 함께 쓴다.
#[allow(clippy::too_many_arguments)]
fn decode_mesh_bytes_into_target(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &mut EguiMeshRenderTarget,
    user: &[u8],
    generation: u64,
    frame_seq: u64,
    full_textures: bool,
    chain_ok: bool,
    host_ppp: f32,
    log_id: &str,
) -> DecodeOutcome {
    let decoded = match tasty_plugin_protocol::mesh_wire::decode_paint(user) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(target = log_id, "egui-mesh prepare: decode failed: {e}");
            return DecodeOutcome::Deferred;
        }
    };

    // DPI 변경 뒤 옛 ppp이면 캐시를 갱신하지 않고 새 프레임을 기다린다.
    if (decoded.pixels_per_point - host_ppp).abs() > PPP_EPS {
        tracing::debug!(
            target = log_id,
            decoded_ppp = decoded.pixels_per_point,
            host_ppp,
            "egui-mesh prepare: ppp mismatch, deferring composite"
        );
        return DecodeOutcome::Deferred;
    }

    let refs_live = all_textures_live(&decoded.primitives, &target.live_textures);
    let deltas_fit = deltas_fit_live(&decoded.textures_delta, &target.live_textures);
    let outcome = classify_decode(chain_ok, target.has_content, refs_live, deltas_fit);
    if outcome == DecodeOutcome::NeedsFull {
        return DecodeOutcome::NeedsFull;
    }

    // AcceptedStale은 도형만 갱신하며 GPU 텍스처는 바꾸지 않는다.
    if chain_ok {
        if full_textures {
            free_stale_textures(target, &decoded.textures_delta);
        }
        apply_texture_deltas(target, device, queue, &decoded.textures_delta);
    }

    target.primitives = decoded.primitives;
    target.ppp = decoded.pixels_per_point;
    target.generation = generation;
    target.has_content = true;
    target.translated_key = None; // 평행이동 캐시 무효화.
    if chain_ok {
        target.last_seq = frame_seq;
        target.awaiting_full = false;
        target.full_request.reset();
    }
    // 도형만 채택했으면 last_seq를 유지해 다음 tick에서도 전체 텍스처를 요청한다.
    outcome
}

/// 전체 텍스처 프레임에 없는 기존 텍스처를 해제한다.
fn free_stale_textures(target: &mut EguiMeshRenderTarget, textures_delta: &TexturesDelta) {
    let full_ids: HashSet<TextureId> = textures_delta.set.iter().map(|(id, _)| *id).collect();
    let stale: Vec<TextureId> = target
        .live_textures
        .keys()
        .filter(|id| !full_ids.contains(id))
        .copied()
        .collect();
    for id in stale {
        target.renderer.free_texture(&id);
        target.live_textures.remove(&id);
    }
}

/// 전용 Renderer에 set을 적용한 뒤 free를 처리한다.
/// 송신 프레임은 free 대상 텍스처를 도형에서 참조하지 않아야 한다.
fn apply_texture_deltas(
    target: &mut EguiMeshRenderTarget,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    textures_delta: &TexturesDelta,
) {
    for (id, delta) in &textures_delta.set {
        target.renderer.update_texture(device, queue, *id, delta);
        // 전체 이미지가 크기를 정하며 부분 변경은 기존 크기를 유지한다.
        if delta.pos.is_none() {
            target.live_textures.insert(*id, delta.image.size());
        } else {
            target
                .live_textures
                .entry(*id)
                .or_insert_with(|| delta.image.size());
        }
    }
    for id in &textures_delta.free {
        target.renderer.free_texture(id);
        target.live_textures.remove(id);
    }
}

/// 전용 pass에서 캐시 도형을 대상 영역에 합성한다. 호출 순서는 surface·팝업 경로에서 정한다.
fn composite_mesh_target(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &mut EguiMeshRenderTarget,
    view: &wgpu::TextureView,
    rect: PhysicalRect,
    size_in_pixels: [u32; 2],
    page_clip: Option<&[egui::Rect]>,
) {
    if !target.has_content {
        return;
    }
    let ppp = target.ppp;
    if ppp <= 0.0 {
        return;
    }

    let rect_bits = [
        rect.x.value().to_bits(),
        rect.y.value().to_bits(),
        rect.width.value().to_bits(),
        rect.height.value().to_bits(),
    ];
    let key = (target.generation, rect_bits);
    if target.translated_key != Some(key) {
        let logical = rect.to_logical(ppp);
        let offset = egui::vec2(logical.x.value(), logical.y.value());
        let bounds = egui::Rect::from_min_size(
            egui::pos2(logical.x.value(), logical.y.value()),
            egui::vec2(logical.width.value(), logical.height.value()),
        );
        target.translated = offset_and_clip(&target.primitives, offset, bounds);
        target.translated_key = Some(key);
    }
    let clipped;
    let prims: &[ClippedPrimitive] = match page_clip {
        Some(pages) => {
            clipped = clip_primitives_to_pages(&target.translated, pages);
            &clipped
        }
        None => &target.translated,
    };
    if prims.is_empty() {
        return;
    }

    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels,
        pixels_per_point: ppp,
    };

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("egui_mesh_encoder"),
    });
    target
        .renderer
        .update_buffers(device, queue, &mut encoder, prims, &screen_descriptor);
    {
        let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui_mesh_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let mut render_pass = render_pass.forget_lifetime();
        target
            .renderer
            .render(&mut render_pass, prims, &screen_descriptor);
    }
    queue.submit(std::iter::once(encoder.finish()));
}

/// 대상 Renderer가 없으면 생성한다.
fn ensure_mesh_target<K: std::hash::Hash + Eq + Copy>(
    targets: &mut HashMap<K, EguiMeshRenderTarget>,
    key: K,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) {
    targets.entry(key).or_insert_with(|| {
        let renderer = egui_wgpu::Renderer::new(device, format, None, 1, false);
        EguiMeshRenderTarget::new(renderer)
    });
}

/// attach mirror 의 전체 재전송 요청을 간격이 찼을 때만 등록하고, 아니면 그 시각에 다시 그리도록 예약한다.
/// 요청 목록은 매 프레임 비워지므로 기다리는 동안 이 함수가 다시 불린다. 로컬 plugin 경로는
/// 응답이 latest-wins 버퍼에서 덮일 수 있어 매 tick 다시 요청하므로 이 함수를 쓰지 않는다.
fn request_remote_full(
    targets: &mut HashMap<u32, EguiMeshRenderTarget>,
    requests: &mut HashSet<u32>,
    key: u32,
    egui_ctx: &egui::Context,
) {
    let Some(target) = targets.get_mut(&key) else {
        return;
    };
    match target.full_request.admit(std::time::Instant::now()) {
        Ok(()) => {
            requests.insert(key);
        }
        Err(wait) => egui_ctx.request_repaint_after(wait),
    }
}

/// 공유 메모리를 읽고 디코드 결과에 따라 전체 재전송 요청을 다시 등록한다.
#[allow(clippy::too_many_arguments)]
fn decode_and_track<K: std::hash::Hash + Eq + Copy>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    plugin_manager: PluginDisplay<'_>,
    targets: &mut HashMap<K, EguiMeshRenderTarget>,
    full_requests: &mut HashSet<K>,
    key: K,
    plugin_id: &str,
    buffer_id: tasty_plugin_protocol::SharedBufferId,
    generation: u64,
    frame_seq: u64,
    full_textures: bool,
    chain_ok: bool,
    awaiting: bool,
    host_ppp: f32,
    log_not_registered: impl FnOnce(),
    log_chain_broken: impl FnOnce(),
) {
    let Some(raw) = plugin_manager.mesh_bytes(plugin_id, buffer_id) else {
        log_not_registered();
        return;
    };
    let outcome = decode_mesh_into_target(
        device,
        queue,
        targets.get_mut(&key).expect("ensured above"),
        raw,
        generation,
        frame_seq,
        full_textures,
        chain_ok,
        host_ppp,
        plugin_id,
    );
    if !matches!(
        outcome,
        DecodeOutcome::NeedsFull | DecodeOutcome::AcceptedStale
    ) {
        return;
    }
    if !awaiting {
        log_chain_broken();
        targets.get_mut(&key).expect("ensured above").awaiting_full = true;
    }
    full_requests.insert(key);
}

/// 닫힌 팝업·배너 자원을 정리하고 남은 작업이 있는지 반환한다.
/// regions가 비어도 호출해야 이전 Renderer가 정리된다. 테스트에서는 GPU 없이 다른 값 타입으로 검사한다.
fn prune_mesh_targets<T>(
    targets: &mut std::collections::HashMap<u64, T>,
    regions: &[(u64, PhysicalRect)],
) -> bool {
    if regions.is_empty() && targets.is_empty() {
        return false;
    }
    let live: HashSet<u64> = regions.iter().map(|r| r.0).collect();
    targets.retain(|iid, _| live.contains(iid));
    true
}

impl GpuState {
    /// 터미널 다음, host UI 전에 surface를 합성한다.
    /// targets는 보이는 대상, existing은 숨겨진 탭까지 포함한 자원 유지·디코드 대상이다.
    pub(super) fn render_egui_mesh_surfaces(
        &mut self,
        view: &wgpu::TextureView,
        targets: &[(u32, String, PhysicalRect)],
        existing: &[(u32, String)],
        plugin_manager: PluginDisplay<'_>,
    ) {
        if targets.is_empty() && existing.is_empty() && self.egui_mesh_targets.is_empty() {
            return;
        }
        let live: HashSet<u32> = existing.iter().map(|t| t.0).collect();
        self.egui_mesh_targets.retain(|sid, _| live.contains(sid));

        // 숨겨진 surface도 디코드한다. 순서가 끊겼으면 조건에 따라 도형만 쓰고 전체 텍스처를 요청한다.
        let host_ppp = self.scale_factor;
        for (sid, plugin_id) in existing {
            // 플러그인 프레임이 없으면 캐시가 있어도 합성하지 않는다.
            let Some(frame) = plugin_manager.egui_mesh_frame(*sid) else {
                continue;
            };
            let frame_buffer_id = frame.buffer_id;
            let frame_generation = frame.generation;
            let frame_seq = frame.frame_seq;
            let frame_full = frame.full_textures;

            ensure_mesh_target(
                &mut self.egui_mesh_targets,
                *sid,
                &self.device,
                self.config.format,
            );

            let (needs_decode, chain_ok, awaiting) = {
                let t = &self.egui_mesh_targets[sid];
                (
                    !t.has_content || t.generation != frame_generation,
                    t.chain_accepts(frame_seq, frame_full),
                    t.awaiting_full,
                )
            };
            if !needs_decode {
                // 요청 목록은 매 프레임 비워지므로 전체 텍스처가 올 때까지 다시 등록한다.
                if awaiting {
                    self.egui_mesh_full_requests.insert(*sid);
                }
                continue;
            }
            decode_and_track(
                &self.device,
                &self.queue,
                plugin_manager,
                &mut self.egui_mesh_targets,
                &mut self.egui_mesh_full_requests,
                *sid,
                plugin_id,
                frame_buffer_id,
                frame_generation,
                frame_seq,
                frame_full,
                chain_ok,
                awaiting,
                host_ppp,
                || {
                    tracing::warn!(
                        plugin = %plugin_id,
                        buffer = frame_buffer_id.0,
                        surface = sid,
                        "egui-mesh prepare: SharedMemory not registered"
                    );
                },
                || {
                    tracing::debug!(
                        surface = sid,
                        frame_seq,
                        "egui-mesh prepare: texture delta chain broken; requesting full resend"
                    );
                },
            );
        }

        let size_in_pixels = [self.size.width, self.size.height];
        for (sid, _plugin_id, rect) in targets {
            if let Some(t) = self.egui_mesh_targets.get_mut(sid) {
                composite_mesh_target(
                    &self.device,
                    &self.queue,
                    t,
                    view,
                    *rect,
                    size_in_pixels,
                    None,
                );
            }
        }
    }

    /// attach mesh도 같은 디코더로 합성하되 TCP로 받은 payload를 사용한다.
    /// 유지할 existing 집합이 로컬 플러그인과 달라 자원 맵도 분리한다.
    /// targets는 보이는 대상이고 existing은 숨겨진 대상까지 포함한다.
    pub(super) fn render_attach_mesh_surfaces(
        &mut self,
        view: &wgpu::TextureView,
        targets: &[(u32, PhysicalRect)],
        existing: &[u32],
        frame_store: &crate::remote::mesh_frames::AttachMeshFrameStore,
    ) {
        let live: HashSet<u32> = existing.iter().copied().collect();
        self.attach_mesh_targets.retain(|sid, _| live.contains(sid));

        let host_ppp = self.scale_factor;
        for sid in existing {
            let Some(frame) = frame_store.get(*sid) else {
                continue;
            };

            if !self.attach_mesh_targets.contains_key(sid) {
                let renderer =
                    egui_wgpu::Renderer::new(&self.device, self.config.format, None, 1, false);
                self.attach_mesh_targets
                    .insert(*sid, EguiMeshRenderTarget::new(renderer));
            }

            let (needs_decode, chain_ok, awaiting) = {
                let t = &self.attach_mesh_targets[sid];
                (
                    !t.has_content || t.generation != frame.generation,
                    t.chain_accepts(frame.frame_seq, frame.full_textures),
                    t.awaiting_full,
                )
            };
            if !needs_decode {
                if awaiting {
                    request_remote_full(
                        &mut self.attach_mesh_targets,
                        &mut self.attach_mesh_full_requests,
                        *sid,
                        &self.egui_ctx,
                    );
                }
                continue;
            }

            let outcome = decode_mesh_bytes_into_target(
                &self.device,
                &self.queue,
                self.attach_mesh_targets
                    .get_mut(sid)
                    .expect("ensured above"),
                &frame.bytes,
                frame.generation,
                frame.frame_seq,
                frame.full_textures,
                chain_ok,
                host_ppp,
                "attach-mesh",
            );
            if matches!(
                outcome,
                DecodeOutcome::NeedsFull | DecodeOutcome::AcceptedStale
            ) {
                if !awaiting {
                    tracing::debug!(
                        surface = sid,
                        frame_seq = frame.frame_seq,
                        "attach-mesh prepare: texture delta chain broken; requesting full resend"
                    );
                    self.attach_mesh_targets
                        .get_mut(sid)
                        .expect("ensured above")
                        .awaiting_full = true;
                }
                request_remote_full(
                    &mut self.attach_mesh_targets,
                    &mut self.attach_mesh_full_requests,
                    *sid,
                    &self.egui_ctx,
                );
            }
        }

        let size_in_pixels = [self.size.width, self.size.height];
        for (sid, rect) in targets {
            if let Some(t) = self.attach_mesh_targets.get_mut(sid) {
                composite_mesh_target(
                    &self.device,
                    &self.queue,
                    t,
                    view,
                    *rect,
                    size_in_pixels,
                    None,
                );
            }
        }
    }

    /// 팝업의 최대 z_seq를 비교한 결과에 따라 host pass 앞이나 뒤에 합성한다.
    /// 콘텐츠는 content_rect 안으로 자르고 host 셸은 그 영역을 비워 어느 순서든 콘텐츠를 가리지 않는다.
    /// `size_in_pixels` 는 그리는 surface 의 크기다. `page_clip` 이 있으면(WebView 오버레이 pass)
    /// 콘텐츠를 그 페이지 합집합 안으로 더 자른다.
    pub(super) fn render_egui_mesh_popups(
        &mut self,
        view: &wgpu::TextureView,
        regions: &[(u64, PhysicalRect)],
        plugin_manager: PluginDisplay<'_>,
        size_in_pixels: [u32; 2],
        page_clip: Option<&[egui::Rect]>,
    ) {
        if !prune_mesh_targets(&mut self.egui_mesh_popup_targets, regions) {
            return;
        }

        for (iid, rect) in regions {
            let Some(frame) = plugin_manager.popup_mesh_frame(*iid) else {
                continue;
            };
            let frame_plugin_id = frame.plugin_id.clone();
            let frame_buffer_id = frame.buffer_id;
            let frame_generation = frame.generation;
            let frame_seq = frame.frame_seq;
            let frame_full = frame.full_textures;
            let host_ppp = self.scale_factor;

            ensure_mesh_target(
                &mut self.egui_mesh_popup_targets,
                *iid,
                &self.device,
                self.config.format,
            );

            let (needs_decode, chain_ok, awaiting) = {
                let t = &self.egui_mesh_popup_targets[iid];
                (
                    !t.has_content || t.generation != frame_generation,
                    t.chain_accepts(frame_seq, frame_full),
                    t.awaiting_full,
                )
            };
            if needs_decode {
                decode_and_track(
                    &self.device,
                    &self.queue,
                    plugin_manager,
                    &mut self.egui_mesh_popup_targets,
                    &mut self.egui_mesh_popup_full_requests,
                    *iid,
                    &frame_plugin_id,
                    frame_buffer_id,
                    frame_generation,
                    frame_seq,
                    frame_full,
                    chain_ok,
                    awaiting,
                    host_ppp,
                    || {
                        tracing::warn!(
                            plugin = %frame_plugin_id,
                            buffer = frame_buffer_id.0,
                            popup = iid,
                            "egui-mesh popup prepare: SharedMemory not registered"
                        );
                    },
                    || {
                        tracing::debug!(
                            popup = iid,
                            frame_seq,
                            "egui-mesh popup prepare: texture delta chain broken; requesting full resend"
                        );
                    },
                );
            } else if awaiting {
                self.egui_mesh_popup_full_requests.insert(*iid);
            }

            if let Some(t) = self.egui_mesh_popup_targets.get_mut(iid) {
                composite_mesh_target(
                    &self.device,
                    &self.queue,
                    t,
                    view,
                    *rect,
                    size_in_pixels,
                    page_clip,
                );
            }
        }
    }

    /// host가 그린 배너 셸 위 content_rect에 플러그인 mesh를 합성한다.
    pub(super) fn render_egui_mesh_banners(
        &mut self,
        view: &wgpu::TextureView,
        regions: &[(u64, PhysicalRect)],
        plugin_manager: PluginDisplay<'_>,
    ) {
        if !prune_mesh_targets(&mut self.egui_mesh_banner_targets, regions) {
            return;
        }

        let size_in_pixels = [self.size.width, self.size.height];
        for (iid, rect) in regions {
            let Some(frame) = plugin_manager.banner_mesh_frame(*iid) else {
                continue;
            };
            let frame_plugin_id = frame.plugin_id.clone();
            let frame_buffer_id = frame.buffer_id;
            let frame_generation = frame.generation;
            let frame_seq = frame.frame_seq;
            let frame_full = frame.full_textures;
            let host_ppp = self.scale_factor;

            ensure_mesh_target(
                &mut self.egui_mesh_banner_targets,
                *iid,
                &self.device,
                self.config.format,
            );

            let (needs_decode, chain_ok, awaiting) = {
                let t = &self.egui_mesh_banner_targets[iid];
                (
                    !t.has_content || t.generation != frame_generation,
                    t.chain_accepts(frame_seq, frame_full),
                    t.awaiting_full,
                )
            };
            if needs_decode {
                decode_and_track(
                    &self.device,
                    &self.queue,
                    plugin_manager,
                    &mut self.egui_mesh_banner_targets,
                    &mut self.egui_mesh_banner_full_requests,
                    *iid,
                    &frame_plugin_id,
                    frame_buffer_id,
                    frame_generation,
                    frame_seq,
                    frame_full,
                    chain_ok,
                    awaiting,
                    host_ppp,
                    || {
                        tracing::warn!(
                            plugin = %frame_plugin_id,
                            buffer = frame_buffer_id.0,
                            banner = iid,
                            "egui-mesh banner prepare: SharedMemory not registered"
                        );
                    },
                    || {
                        tracing::debug!(
                            banner = iid,
                            frame_seq,
                            "egui-mesh banner prepare: texture delta chain broken; requesting full resend"
                        );
                    },
                );
            } else if awaiting {
                self.egui_mesh_banner_full_requests.insert(*iid);
            }

            if let Some(t) = self.egui_mesh_banner_targets.get_mut(iid) {
                composite_mesh_target(
                    &self.device,
                    &self.queue,
                    t,
                    view,
                    *rect,
                    size_in_pixels,
                    None,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // 테스트 fixture 는 mesh 좌표 변환을 검증하려고 원시 정점값을 직접 만든다
    // (UI 색/좌표 "디자인" 이 아니라 합성 기하). clippy 의 정상 예외 경로.
    #![allow(clippy::disallowed_methods)]

    use super::*;
    use egui::Color32;
    use egui::emath::{Pos2, Rect};
    use egui::epaint::{Mesh, TextureId, Vertex};

    fn any_rect() -> PhysicalRect {
        PhysicalRect {
            x: tasty_type_geometry::length::PhysicalPx(0.0),
            y: tasty_type_geometry::length::PhysicalPx(0.0),
            width: tasty_type_geometry::length::PhysicalPx(1.0),
            height: tasty_type_geometry::length::PhysicalPx(1.0),
        }
    }

    /// 표시 영역이 없어도 닫힌 대상 자원을 정리한다.
    #[test]
    fn prune_runs_even_when_no_region_is_visible() {
        let mut targets: std::collections::HashMap<u64, u32> =
            std::collections::HashMap::from([(1, 10), (2, 20)]);
        assert!(
            prune_mesh_targets(&mut targets, &[]),
            "상주 자원이 남아 있으면 할 일이 있다고 답해야 한다"
        );
        assert!(
            targets.is_empty(),
            "보이는 region 이 없으면 상주 target 은 전부 닫힌 것이다 — 여기서 풀려야 한다"
        );
    }

    /// 열려 있는 대상은 유지해 전체 삭제와 구분한다.
    #[test]
    fn prune_keeps_the_instances_that_still_have_a_region() {
        let mut targets: std::collections::HashMap<u64, u32> =
            std::collections::HashMap::from([(1, 10), (2, 20)]);
        assert!(prune_mesh_targets(&mut targets, &[(1, any_rect())]));
        let mut left: Vec<u64> = targets.keys().copied().collect();
        left.sort_unstable();
        assert_eq!(left, vec![1], "region 이 있는 1 은 남고 2 만 풀린다");
    }

    /// 보이는 region 도 상주 자원도 없으면 할 일이 없다고 답한다(호출부는 곧장 반환).
    #[test]
    fn nothing_visible_and_nothing_resident_reports_no_work() {
        let mut targets: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
        assert!(!prune_mesh_targets(&mut targets, &[]));
    }

    fn vtx(x: f32, y: f32) -> Vertex {
        Vertex {
            pos: Pos2::new(x, y),
            uv: Pos2::ZERO,
            color: Color32::WHITE,
        }
    }

    fn mesh_prim(clip: Rect, verts: Vec<Vertex>) -> ClippedPrimitive {
        ClippedPrimitive {
            clip_rect: clip,
            primitive: Primitive::Mesh(Mesh {
                indices: vec![0, 1, 2],
                vertices: verts,
                texture_id: TextureId::Managed(0),
            }),
        }
    }

    /// 평행이동: 모든 vertex.pos 와 clip_rect 가 surface origin 만큼 더해진다.
    #[test]
    fn offset_translates_vertices_and_clip() {
        let prims = vec![mesh_prim(
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 80.0)),
            vec![vtx(0.0, 0.0), vtx(10.0, 0.0), vtx(0.0, 10.0)],
        )];
        let offset = egui::vec2(50.0, 40.0);
        let bounds = Rect::from_min_size(egui::pos2(50.0, 40.0), egui::vec2(100.0, 80.0));

        let out = offset_and_clip(&prims, offset, bounds);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].clip_rect.min, Pos2::new(50.0, 40.0));
        assert_eq!(out[0].clip_rect.max, Pos2::new(150.0, 120.0));
        let Primitive::Mesh(m) = &out[0].primitive else {
            panic!("expected mesh");
        };
        assert_eq!(m.vertices[0].pos, Pos2::new(50.0, 40.0));
        assert_eq!(m.vertices[1].pos, Pos2::new(60.0, 40.0));
        assert_eq!(m.vertices[2].pos, Pos2::new(50.0, 50.0));
    }

    /// 오버레이 pass 의 mesh 는 페이지 밖을 칠하지 않는다. 페이지가 x<300 이고 콘텐츠가 x=250..350 이면
    /// x=250..300 만 남는다. 좌표는 옮기지 않고, 겹치지 않는 primitive 는 버린다.
    #[test]
    fn overlay_mesh_is_clipped_to_the_page_union() {
        let content = Rect::from_min_max(Pos2::new(250.0, 100.0), Pos2::new(350.0, 200.0));
        let away = Rect::from_min_max(Pos2::new(400.0, 0.0), Pos2::new(450.0, 50.0));
        let prims = vec![
            mesh_prim(
                content,
                vec![vtx(250.0, 100.0), vtx(350.0, 100.0), vtx(250.0, 200.0)],
            ),
            mesh_prim(away, vec![vtx(400.0, 0.0)]),
        ];
        let left = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(300.0, 600.0));
        let out = clip_primitives_to_pages(&prims, &[left]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].clip_rect,
            Rect::from_min_max(Pos2::new(250.0, 100.0), Pos2::new(300.0, 200.0))
        );
        let Primitive::Mesh(m) = &out[0].primitive else {
            panic!("expected mesh");
        };
        assert_eq!(m.vertices[1].pos, Pos2::new(350.0, 100.0));
        // 두 페이지에 걸치면 페이지마다 한 번씩 남는다.
        let right = Rect::from_min_max(Pos2::new(320.0, 0.0), Pos2::new(600.0, 600.0));
        let both: Vec<Rect> = clip_primitives_to_pages(&prims, &[left, right])
            .iter()
            .map(|p| p.clip_rect)
            .collect();
        assert_eq!(
            both,
            vec![
                Rect::from_min_max(Pos2::new(250.0, 100.0), Pos2::new(300.0, 200.0)),
                Rect::from_min_max(Pos2::new(320.0, 100.0), Pos2::new(350.0, 200.0)),
                away,
            ]
        );
    }

    /// clip 이 surface 경계를 넘어가면 bounds 로 잘린다(영역 한정).
    #[test]
    fn clip_is_clamped_to_surface_bounds() {
        let prims = vec![mesh_prim(
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 200.0)),
            vec![vtx(0.0, 0.0)],
        )];
        let offset = egui::vec2(50.0, 40.0);
        let bounds = Rect::from_min_size(egui::pos2(50.0, 40.0), egui::vec2(100.0, 80.0));

        let out = offset_and_clip(&prims, offset, bounds);
        assert_eq!(out[0].clip_rect.min, Pos2::new(50.0, 40.0));
        assert_eq!(out[0].clip_rect.max, Pos2::new(150.0, 120.0));
    }

    /// 텍스처 적용 후보는 전체 프레임 또는 연속 시퀀스다.
    #[test]
    fn chain_accepts_full_or_contiguous_seq_only() {
        assert!(!chain_accepts(false, 0, 1, false));
        assert!(chain_accepts(false, 0, 1, true));
        assert!(chain_accepts(false, 0, 7, true));

        assert!(chain_accepts(true, 5, 6, false));
        assert!(!chain_accepts(true, 5, 7, false));
        assert!(!chain_accepts(true, 5, 5, false));
        assert!(chain_accepts(true, 5, 9, true));

        // 구버전 plugin (frame_seq 항상 0, full 마킹 없음) → 항상 체인 단절.
        assert!(!chain_accepts(true, 0, 0, false));
    }

    fn mesh_prim_tex(tex: TextureId) -> ClippedPrimitive {
        ClippedPrimitive {
            clip_rect: Rect::from_min_max(Pos2::ZERO, Pos2::new(10.0, 10.0)),
            primitive: Primitive::Mesh(Mesh {
                indices: vec![0, 1, 2],
                vertices: vec![vtx(0.0, 0.0), vtx(1.0, 0.0), vtx(0.0, 1.0)],
                texture_id: tex,
            }),
        }
    }

    /// 도형 참조 텍스처의 상주 여부를 검사한다.
    #[test]
    fn all_textures_live_requires_every_ref_present() {
        let font = TextureId::Managed(0);
        let image = TextureId::Managed(7);
        let mut live: HashMap<TextureId, [usize; 2]> = HashMap::new();
        live.insert(font, [1024, 64]);

        let prims = vec![mesh_prim_tex(font), mesh_prim_tex(font)];
        assert!(all_textures_live(&prims, &live));

        let prims = vec![mesh_prim_tex(font), mesh_prim_tex(image)];
        assert!(!all_textures_live(&prims, &live));

        assert!(all_textures_live(&[], &live));

        live.insert(image, [16, 16]);
        let prims = vec![mesh_prim_tex(font), mesh_prim_tex(image)];
        assert!(all_textures_live(&prims, &live));
    }

    /// 크기 [w,h] 의 이미지를 담은 delta 를 만든다. pos=None → full, pos=Some → partial.
    fn img_delta(pos: Option<[usize; 2]>, w: usize, h: usize) -> egui::epaint::ImageDelta {
        let image = egui::epaint::ColorImage::new([w, h], Color32::WHITE);
        egui::epaint::ImageDelta {
            image: egui::epaint::ImageData::Color(std::sync::Arc::new(image)),
            options: egui::epaint::textures::TextureOptions::LINEAR,
            pos,
        }
    }

    fn tex_delta(set: Vec<(TextureId, egui::epaint::ImageDelta)>) -> TexturesDelta {
        TexturesDelta {
            set,
            free: Vec::new(),
        }
    }

    /// 부분 변경의 끝 좌표가 텍스처 크기를 넘으면 거절한다.
    #[test]
    fn deltas_fit_live_rejects_partial_overrun() {
        let font = TextureId::Managed(0);
        let mut live: HashMap<TextureId, [usize; 2]> = HashMap::new();
        live.insert(font, [1024, 64]);

        assert!(deltas_fit_live(
            &tex_delta(vec![(font, img_delta(None, 1024, 128))]),
            &live
        ));

        assert!(deltas_fit_live(
            &tex_delta(vec![(font, img_delta(Some([0, 40]), 1024, 15))]),
            &live
        ));

        assert!(deltas_fit_live(
            &tex_delta(vec![(font, img_delta(Some([0, 49]), 1024, 15))]),
            &live
        ));

        assert!(!deltas_fit_live(
            &tex_delta(vec![(font, img_delta(Some([0, 57]), 1024, 15))]),
            &live
        ));

        assert!(!deltas_fit_live(
            &tex_delta(vec![(font, img_delta(Some([1020, 0]), 8, 8))]),
            &live
        ));

        assert!(!deltas_fit_live(
            &tex_delta(vec![(TextureId::Managed(9), img_delta(Some([0, 0]), 8, 8))]),
            &live
        ));

        assert!(deltas_fit_live(&tex_delta(vec![]), &live));
    }

    /// 연속 여부와 무관하게 경계를 넘는 부분 변경은 거절한다.
    /// 전체 이미지로 크기를 갱신한 뒤에는 같은 범위의 부분 변경을 수락한다.
    #[test]
    fn resize_loss_then_overrun_partial_is_gated() {
        let font = TextureId::Managed(0);

        let mut live: HashMap<TextureId, [usize; 2]> = HashMap::new();
        live.insert(font, [1024, 64]);

        let overrun = tex_delta(vec![(font, img_delta(Some([0, 57]), 1024, 15))]);
        let prims = vec![mesh_prim_tex(font)];

        let refs_live = all_textures_live(&prims, &live);
        let fits = deltas_fit_live(&overrun, &live);
        assert!(refs_live, "폰트 atlas 는 id 로 상주");
        assert!(!fits, "오버런 부분 delta 는 경계 검증에서 걸린다");
        assert!(
            !(refs_live && fits),
            "seq 점프 오버런 frame 은 채택되지 않는다"
        );

        assert!(
            !deltas_fit_live(&overrun, &live),
            "chain_ok 여도 오버런 delta 는 적용 전 차단"
        );

        live.insert(font, [1024, 128]);
        assert!(deltas_fit_live(&overrun, &live), "128 성장 후엔 경계 통과");
    }

    /// 텍스처 적용·도형만 채택·전체 재전송의 세 결과를 구분한다.
    #[test]
    fn classify_decode_gates_stale_and_overrun() {
        use DecodeOutcome::*;

        assert_eq!(classify_decode(true, true, true, true), Accepted);
        assert_eq!(classify_decode(true, true, false, true), Accepted);
        assert_eq!(classify_decode(true, false, true, true), Accepted);

        assert_eq!(classify_decode(false, true, true, true), AcceptedStale);

        assert_eq!(classify_decode(true, true, true, false), NeedsFull);
        assert_eq!(classify_decode(false, true, true, false), NeedsFull);

        assert_eq!(classify_decode(false, false, true, true), NeedsFull);
        assert_eq!(classify_decode(false, true, false, true), NeedsFull);
    }

    /// 디코드 출력(mesh_wire) → offset_and_clip 통합: 실제 tessellate 한 mesh 가
    /// 평행이동 후에도 mesh 로 유지되고 좌표가 origin 만큼 이동한다.
    #[test]
    fn decoded_paint_offsets_cleanly() {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 150.0))),
            ..Default::default()
        };
        let full_output = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("egui-mesh composite");
            });
        });
        let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let bytes = tasty_plugin_protocol::mesh_wire::encode_paint(
            &primitives,
            &full_output.textures_delta,
            full_output.pixels_per_point,
        );
        let decoded = tasty_plugin_protocol::mesh_wire::decode_paint(&bytes).expect("decode");

        let offset = egui::vec2(300.0, 200.0);
        let bounds = Rect::from_min_size(egui::pos2(300.0, 200.0), egui::vec2(200.0, 150.0));
        let out = offset_and_clip(&decoded.primitives, offset, bounds);

        assert_eq!(out.len(), decoded.primitives.len());
        // 실제 정점은 origin만큼 이동하며 화면 밖 부분은 clip으로 제한한다.
        for (orig, cp) in decoded.primitives.iter().zip(&out) {
            let (Primitive::Mesh(om), Primitive::Mesh(m)) = (&orig.primitive, &cp.primitive) else {
                panic!("expected mesh after offset");
            };
            for (ov, v) in om.vertices.iter().zip(&m.vertices) {
                assert_eq!(v.pos, ov.pos + offset);
            }
            assert!(bounds.contains_rect(cp.clip_rect));
        }
    }
}
