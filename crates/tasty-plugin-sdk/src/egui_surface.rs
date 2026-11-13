//! 플러그인의 egui Context로 그린 결과를 공유 버퍼로 전달한다.
//! 호스트의 set_context 입력을 받아 egui 실행, tessellation과 mesh 인코딩을 한다.
//! 출력 해시와 IME 위치가 직전과 같으면 송신을 생략하되 전체 텍스처 복구 요청은 처리한다.
//!
//! Context와 폰트 atlas는 플러그인 소유다. 호스트와 글꼴을 맞추려면
//! context().set_fonts로 같은 폰트를 설치한다.

mod input;
use input::build_raw_input;
#[cfg(test)]
use input::{
    EGUI_SMOOTH_WHEEL_LIMIT, SCROLL_SPLIT_MAX_PARTS, SCROLL_SPLIT_STEP, map_event,
    push_scroll_events, wheel_event,
};

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use egui::epaint::textures::{TextureOptions, TexturesDelta};
use egui::epaint::{ImageData, ImageDelta, TextureId};
use egui::{
    Context, Event, ImeEvent, Key, Modifiers, MouseWheelUnit, OutputCommand, PointerButton, Pos2,
    RawInput, Rect, Vec2, vec2,
};
use tasty_plugin_protocol::mesh_wire::encode_paint;
use tasty_plugin_protocol::{
    BannerSetContextParams, ImeCursorWire, ImeWire, ModifiersWire, PointerButtonWire,
    PopupSetContextParams, RawInputEventWire, RawInputWire, RectWire, SurfaceSetContextParams,
    ThemeWire,
};

#[cfg(any(unix, windows))]
use tasty_plugin_protocol::{CopyTargetWire, PluginEvent, SharedBufferId};

#[cfg(any(unix, windows))]
use crate::error::PluginError;
#[cfg(any(unix, windows))]
use crate::host::HostHandle;
#[cfg(any(unix, windows))]
use crate::shared_buffer::SharedBuffer;

/// egui 사각형을 좌상단 좌표와 너비·높이로 옮긴다.
fn rect_wire(r: Rect) -> RectWire {
    RectWire {
        x: r.min.x,
        y: r.min.y,
        width: r.width(),
        height: r.height(),
    }
}

/// Surface, Popup, Banner가 공유하는 egui 렌더 상태와 버퍼 전송 기능.
struct EguiMeshCore {
    ctx: Context,
    /// 직전 frame 에 인코드한 mesh 바이트의 해시. 이 값과 IME 위치가 모두 같으면 송신 생략.
    last_hash: Option<u64>,
    /// 직전에 보낸 frame 의 IME 위치. 그림이 같아도 입력칸 포커스가 바뀌면 frame 을 보내
    /// 호스트의 `ime_cursor` 캐시를 갱신한다.
    last_sent_ime: Option<ImeCursorWire>,
    /// 직전 set_context 의 렌더 컨텍스트(geom/ppp/theme). out-of-band 상태 변경 뒤
    /// [`repaint_last`](Self::repaint_last)가 **빈 입력**으로 재-run 할 때 재사용한다.
    /// 첫 set_context 도착 전엔 `None` — 그 때 repaint 는 no-op.
    last_ctx: Option<CachedContext>,
    /// 이 코어가 host 로 보낸 텍스처들의 누적 full 상태 (id → full image + options).
    /// wire 는 delta 기반 + latest-wins buffer 라 host 가 중간 frame 을 놓칠 수 있다 —
    /// host 가 `need_full_textures` 로 복구를 요청하면 이 상태를 full image delta 로
    /// 재구성해 동봉한다(font atlas 포함, image plugin 비트맵 등 임의 Managed 텍스처 전부).
    /// `BTreeMap` — full 재구성 인코딩의 결정적 순서(해시 dedup 안정성).
    tex_state: BTreeMap<TextureId, (ImageData, TextureOptions)>,
    /// 지금까지 **송신한** frame 수(= 마지막 송신 frame 의 seq, 1부터). footer generation
    /// 과 달리 shared buffer 재생성(성장)과 무관하게 이어지는 단조 시퀀스로, host 가
    /// textures_delta 체인의 연속성(`frame_seq == last + 1`)을 검증하는 데 쓰인다.
    frame_seq: u64,
    /// mesh POD 블록을 쓰는 shared buffer. 필요 크기보다 작아지면 재생성한다.
    #[cfg(any(unix, windows))]
    buffer: Option<SharedBuffer>,
    /// egui가 요청한 다음 렌더링 지연. 호스트에 갱신 요청을 보내는 데 쓴다.
    pending_self_repaint: Option<Duration>,
    /// 직전 렌더에서 나온 첫 번째 비어 있지 않은 CopyText 값.
    /// 호출자가 가져가 OS 클립보드에 기록할 수 있다.
    last_copied_text: Option<String>,
    /// 직전 렌더의 IME 위젯 위치. 콘텐츠 영역의 논리 좌표로 호스트에 전달한다.
    last_ime_cursor: Option<ImeCursorWire>,
}

/// 한 번의 렌더가 만든 송신 후보 frame — 인코드된 mesh 바이트 + full 마킹.
struct MeshFrame {
    bytes: Vec<u8>,
    /// 이 프레임에 전체 텍스처 상태를 담았는가.
    full_textures: bool,
}

/// 입력 없이 다시 그릴 때 사용할 크기·배율·포커스·테마.
/// 사용자 이벤트는 저장하지 않는다. focused는 상태값이므로 유지한다.
struct CachedContext {
    width_px: u32,
    height_px: u32,
    ppp: f32,
    focused: bool,
    theme: Option<ThemeWire>,
}

impl EguiMeshCore {
    fn new() -> Self {
        let ctx = Context::default();
        // 배율은 호스트 설정을 따르므로 egui의 별도 키보드 줌을 끈다.
        ctx.options_mut(|opts| {
            opts.zoom_with_keyboard = false;
        });
        // 프로그램으로 요청한 스크롤의 애니메이션을 끈다.
        // 추가 프레임마다 호스트와 메시지를 주고받는 비용을 줄인다.
        // 입력란 커서는 호스트처럼 깜박이지 않는다. 깜박이면 포커스된 입력란이 있는 동안
        // 0.5초마다 호스트와 프레임을 주고받는다.
        ctx.all_styles_mut(|s| {
            s.scroll_animation = egui::style::ScrollAnimation::none();
            s.visuals.text_cursor.blink = false;
        });
        Self {
            ctx,
            last_hash: None,
            last_sent_ime: None,
            last_ctx: None,
            tex_state: BTreeMap::new(),
            frame_seq: 0,
            #[cfg(any(unix, windows))]
            buffer: None,
            pending_self_repaint: None,
            last_copied_text: None,
            last_ime_cursor: None,
        }
    }

    /// 한 프레임을 인코딩한다. 직전 출력과 해시가 같으면 None을 반환한다.
    /// need_full이면 생략하지 않고 누적 텍스처 전체를 보낸다.
    #[allow(clippy::too_many_arguments)] // reason: set_context 렌더 컨텍스트 전체
    fn run_frame(
        &mut self,
        width_px: u32,
        height_px: u32,
        ppp: f32,
        theme: Option<&ThemeWire>,
        raw_input: &RawInputWire,
        need_full: bool,
        run_ui: impl FnMut(&Context),
    ) -> Option<MeshFrame> {
        // out-of-band 재-paint 가 재현할 수 있도록 이번 컨텍스트를 캐시한다
        // (입력 이벤트 제외 — focused 는 지속 상태라 포함).
        self.last_ctx = Some(CachedContext {
            width_px,
            height_px,
            ppp,
            focused: raw_input.focused,
            theme: theme.cloned(),
        });
        let raw = build_raw_input(width_px, height_px, ppp, raw_input);
        self.render(raw, need_full, run_ui)
    }

    /// 캐시한 크기·배율·포커스로 다시 그리되 사용자 이벤트는 넣지 않는다.
    /// 캐시가 없거나 출력 해시가 같으면 None을 반환한다.
    fn repaint_last(&mut self, run_ui: impl FnMut(&Context)) -> Option<MeshFrame> {
        let (width_px, height_px, ppp, focused) = {
            let c = self.last_ctx.as_ref()?;
            (c.width_px, c.height_px, c.ppp, c.focused)
        };
        let wire = RawInputWire {
            focused,
            ..Default::default()
        };
        let raw = build_raw_input(width_px, height_px, ppp, &wire);
        self.render(raw, false, run_ui)
    }

    /// 직전 set_context 의 theme 스냅샷. plugin 의 재-paint closure 가 캐시된 theme 으로
    /// 다시 그릴 수 있도록 노출한다. 첫 set_context 전이거나 theme 미동봉이면 `None`.
    fn last_theme(&self) -> Option<&ThemeWire> {
        self.last_ctx.as_ref().and_then(|c| c.theme.as_ref())
    }

    /// 직전 렌더의 repaint_delay. 요청이 없으면 None, 즉시 요청이면 0이다.
    fn pending_self_repaint(&self) -> Option<Duration> {
        self.pending_self_repaint
    }

    /// 직전 렌더에서 얻은 CopyText를 한 번 꺼낸다. 값이 없으면 None이다.
    fn take_copied_text(&mut self) -> Option<String> {
        self.last_copied_text.take()
    }

    /// 직전 렌더의 복사 결과가 있으면 호스트에 [`PluginEvent::CopyText`] 로 알리고 소비한다.
    /// 화면이 바뀌지 않는 복사는 프레임 송신이 생략되므로 생략 판정 전에 부른다.
    #[cfg(any(unix, windows))]
    fn notify_copied_text(
        &mut self,
        host: &HostHandle,
        target: CopyTargetWire,
    ) -> Result<(), PluginError> {
        let Some(text) = self.take_copied_text() else {
            return Ok(());
        };
        host.notify(&PluginEvent::CopyText { target, text })
    }

    /// 직전 렌더의 IME 커서 위치를 복사한다. 송신을 생략해도 최신 렌더 값을 유지한다.
    fn ime_cursor(&self) -> Option<ImeCursorWire> {
        self.last_ime_cursor
    }

    /// 렌더링과 인코딩 후 출력 해시와 IME 위치를 비교한다. 둘 다 같으면 None을 반환한다.
    /// need_full이면 해시 비교로 생략하지 않고 누적 텍스처 전체를 넣는다.
    fn render(
        &mut self,
        raw: RawInput,
        need_full: bool,
        run_ui: impl FnMut(&Context),
    ) -> Option<MeshFrame> {
        let full = self.ctx.run(raw, run_ui);
        // 한 패스에 여러 번 복사하면 호스트 선택 복사처럼 마지막 CopyText 값을 쓴다.
        // 마지막 값이 빈 문자열이면 빈 문자열이다.
        self.last_copied_text = full
            .platform_output
            .commands
            .iter()
            .rev()
            .find_map(|c| match c {
                OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            });
        self.last_ime_cursor = full.platform_output.ime.map(|ime| ImeCursorWire {
            rect: rect_wire(ime.rect),
            cursor_rect: rect_wire(ime.cursor_rect),
        });
        // 송신을 생략해도 다음 렌더 요청은 보관한다. Duration::MAX는 요청 없음이다.
        self.pending_self_repaint = full
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
            .filter(|d| *d < Duration::MAX);
        let primitives = self.ctx.tessellate(full.shapes, full.pixels_per_point);
        // 이 frame 의 delta 를 누적 상태에 먼저 반영한다 — full 재구성은 항상 최신 상태 기준.
        let naturally_full = self.accumulate_textures(&full.textures_delta);
        let (bytes, full_textures) = if need_full {
            (
                encode_paint(
                    &primitives,
                    &self.full_texture_delta(),
                    full.pixels_per_point,
                ),
                true,
            )
        } else {
            (
                encode_paint(&primitives, &full.textures_delta, full.pixels_per_point),
                naturally_full,
            )
        };

        let hash = hash_bytes(&bytes);
        if !need_full && self.last_hash == Some(hash) && self.last_sent_ime == self.last_ime_cursor
        {
            return None;
        }
        self.last_hash = Some(hash);
        self.last_sent_ime = self.last_ime_cursor;
        Some(MeshFrame {
            bytes,
            full_textures,
        })
    }

    /// 이 frame 의 textures_delta 를 누적 상태에 적용하고, delta 가 누적 상태 **전체**를
    /// full image(`pos == None`)로 담고 있는지(자연-full frame — 예: Context 생성 후 첫
    /// frame) 반환한다.
    fn accumulate_textures(&mut self, delta: &TexturesDelta) -> bool {
        for (id, d) in &delta.set {
            match d.pos {
                None => {
                    self.tex_state.insert(*id, (d.image.clone(), d.options));
                }
                Some(pos) => match self.tex_state.get_mut(id) {
                    Some((base, options)) => {
                        *options = d.options;
                        patch_image(base, &d.image, pos);
                    }
                    // egui 는 full image 를 먼저 보낸 텍스처에만 patch 를 낸다 —
                    // base 없는 patch 는 계약 위반이므로 기록만 하고 버린다.
                    None => tracing::warn!(
                        "egui-mesh: texture patch for unknown texture {id:?}; dropping"
                    ),
                },
            }
        }
        for id in &delta.free {
            self.tex_state.remove(id);
        }
        self.tex_state.keys().all(|id| {
            delta
                .set
                .iter()
                .any(|(sid, sd)| sid == id && sd.pos.is_none())
        })
    }

    /// 누적 텍스처 상태 전체를 full image delta 로 재구성한다 (`need_full_textures` 응답).
    fn full_texture_delta(&self) -> TexturesDelta {
        TexturesDelta {
            set: self
                .tex_state
                .iter()
                .map(|(id, (image, options))| {
                    (
                        *id,
                        ImageDelta {
                            image: image.clone(),
                            options: *options,
                            pos: None,
                        },
                    )
                })
                .collect(),
            free: Vec::new(),
        }
    }

    /// 송신 확정된 frame 의 시퀀스를 발급한다(1부터 단조 증가). commit 성공 후에만
    /// 호출해 "송신된 frame 수" 와 어긋나지 않게 한다.
    #[cfg(any(unix, windows))]
    fn next_frame_seq(&mut self) -> u64 {
        self.frame_seq += 1;
        self.frame_seq
    }

    /// 인코드된 바이트를 shared buffer 에 commit 하고 (buffer_id, generation) 을 돌려준다.
    /// 회신 알림(PaintFrame/PopupPaintFrame)은 호출자가 보낸다.
    #[cfg(any(unix, windows))]
    fn commit(
        &mut self,
        host: &HostHandle,
        bytes: &[u8],
    ) -> Result<(SharedBufferId, u64), PluginError> {
        self.ensure_buffer(host, bytes.len())?;
        let buffer = self
            .buffer
            .as_ref()
            .expect("ensure_buffer guarantees a buffer");

        // SAFETY: ensure_buffer가 쓰기 길이를 확보하고 이 코어는 버퍼에 대한
        // 자체 쓰기를 직렬로 처리한다. 그러나 호스트의 동시 읽기를 배제하는 절차는
        // 이 경로에 없으며, 뒤의 generation 갱신만으로 그 안전 조건이 충족되지는 않는다.
        unsafe {
            buffer.as_mut_slice()[..bytes.len()].copy_from_slice(bytes);
        }
        buffer.commit(None)?;
        Ok((buffer.id(), buffer.generation()))
    }

    /// shared buffer 가 `needed` 바이트를 담을 수 있게 보장한다. 부족하면 헤드룸을 둔
    /// 크기로 새로 만든다(폰트 atlas 가 큰 첫 frame spike 를 흡수, 매 frame 재생성 방지).
    #[cfg(any(unix, windows))]
    fn ensure_buffer(&mut self, host: &HostHandle, needed: usize) -> Result<(), PluginError> {
        let big_enough = self.buffer.as_ref().is_some_and(|b| b.len() >= needed);
        if !big_enough {
            let cap = needed.max(4096).next_power_of_two();
            let new_buf = host.create_shared_buffer(cap)?;
            // 이전 버퍼의 해제를 호스트에 알린다. 전송 실패는 경고로 남기고 계속 그린다.
            if let Some(old) = self.buffer.replace(new_buf)
                && let Err(e) = host.notify(&PluginEvent::SharedBufferReleased { id: old.id() })
            {
                tracing::warn!("shared buffer release notify failed: {e}");
            }
        }
        Ok(())
    }
}

/// 예약한 렌더 알림의 기한, 중복 방지 플래그와 실행 함수.
#[cfg(any(unix, windows))]
struct SelfRepaintRequest {
    deadline: std::time::Instant,
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    fire: Box<dyn FnOnce() + Send>,
}

/// 프로세스에서 공유하는 렌더 알림 타이머. 가장 이른 기한까지 기다리고
/// 새 요청이 들어오면 기한을 다시 계산한다. 인스턴스별 대기 요청은 하나다.
///
/// 알림 함수의 패닉은 요청별로 처리한다. 루프가 끝나면 대기 요청과 플래그를 정리해
/// 다음 요청에서 스레드를 다시 만들 수 있게 한다. 스레드 생성에 실패하면
/// 해당 요청만 별도 타이머 스레드로 시도한다.
#[cfg(any(unix, windows))]
struct SelfRepaintTimer {
    pending: std::sync::Mutex<Vec<SelfRepaintRequest>>,
    wake: std::sync::Condvar,
    /// 상주 스레드가 살아 있는지. spawn 에 **성공했을 때만** true 로 남으므로,
    /// 실패나 비정상 종료는 다음 [`Self::arm`] 이 자연히 재시도한다.
    running: std::sync::atomic::AtomicBool,
}

#[cfg(any(unix, windows))]
impl SelfRepaintTimer {
    fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(Vec::new()),
            wake: std::sync::Condvar::new(),
            running: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 프로세스 공용 인스턴스. 요청이 한 번도 없는 plugin 은 스레드를 갖지 않는다
    /// (상주 스레드 기동은 [`Self::arm`] 이 필요할 때만 한다). surface/popup 여러 개가
    /// 같은 스레드를 공유한다.
    fn global() -> &'static SelfRepaintTimer {
        static TIMER: std::sync::OnceLock<SelfRepaintTimer> = std::sync::OnceLock::new();
        // static 이라 `get_or_init` 이 `&'static` 을 돌려준다 — 그대로 스레드로 넘긴다.
        TIMER.get_or_init(SelfRepaintTimer::new)
    }

    /// 상주 타이머를 사용할 수 있는지 확인하고 없으면 생성한다.
    /// 생성 실패나 스레드 종료 뒤에도 다시 시도할 수 있도록 running을 관리한다.
    fn ensure_running(&'static self) -> bool {
        if self
            .running
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return true; // 이미 살아 있다.
        }
        let spawned = std::thread::Builder::new()
            .name("plugin-self-repaint-timer".into())
            .spawn(move || {
                // 스케줄러가 종료되면 대기 플래그를 해제해 다음 요청이 다시 시도할 수 있게 한다.
                let panicked =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run())).is_err();
                self.release_pending_on_exit(panicked);
                self.running
                    .store(false, std::sync::atomic::Ordering::Release);
            });
        if let Err(e) = spawned {
            self.running
                .store(false, std::sync::atomic::Ordering::Release);
            tracing::warn!(
                "egui-mesh self-repaint timer thread spawn failed: {e} — falling back to a one-shot timer for this request"
            );
            return false;
        }
        true
    }

    /// 중복 요청은 생략한다. 시간 범위를 넘는 delay는 플래그를 설정하기 전에 거절한다.
    fn arm(
        &'static self,
        armed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        delay: Duration,
        fire: impl FnOnce() + Send + 'static,
    ) {
        let Some(deadline) = std::time::Instant::now().checked_add(delay) else {
            tracing::warn!(
                "egui-mesh self-repaint delay {delay:?} overflows the monotonic clock — request dropped"
            );
            return;
        };
        if armed
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return;
        }
        if !self.ensure_running() {
            // 상주 스레드를 만들지 못하면 이 요청만 별도 스레드에서 시도한다.
            self.fire_on_one_shot_thread(armed, delay, fire);
            return;
        }
        let request = SelfRepaintRequest {
            deadline,
            armed: std::sync::Arc::clone(armed),
            fire: Box::new(fire),
        };
        // 알림은 잠금 밖에서 실행한다. 큐 잠금이 poison되면 복구해 대기 요청 처리를 계속한다.
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.push(request);
        drop(pending);
        self.wake.notify_one();
    }

    /// 상주 타이머 생성 실패 시 요청별 스레드를 시도한다. 이것도 실패하면
    /// 플래그를 풀어 나중 요청에서 다시 시도할 수 있게 한다.
    fn fire_on_one_shot_thread(
        &self,
        armed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        delay: Duration,
        fire: impl FnOnce() + Send + 'static,
    ) {
        let armed = std::sync::Arc::clone(armed);
        let spawned = std::thread::Builder::new()
            .name("plugin-self-repaint-once".into())
            .spawn({
                let armed = std::sync::Arc::clone(&armed);
                move || {
                    if !delay.is_zero() {
                        std::thread::sleep(delay);
                    }
                    armed.store(false, std::sync::atomic::Ordering::Release);
                    fire();
                }
            });
        if let Err(e) = spawned {
            armed.store(false, std::sync::atomic::Ordering::Release);
            tracing::error!(
                "egui-mesh self-repaint fallback thread spawn failed: {e} — this repaint request is dropped"
            );
        }
    }

    /// 상주 루프 — 마감이 지난 요청을 모아 락 밖에서 발사하고, 다음 마감까지 잔다.
    fn run(&self) {
        loop {
            let due = self.take_due();
            for request in due {
                // 가드를 먼저 풀어야 이 알림에 이어지는 다음 `render()` 가 재-arm 할 수
                // 있다(발사 순서는 가드 해제 → 알림 — 기존 동작과 동일).
                request
                    .armed
                    .store(false, std::sync::atomic::Ordering::Release);
                // 알림 하나의 panic 이 상주 스레드를 죽이면 프로세스 전체의 self-repaint
                // 가 영구 정지한다 — 그 요청만 버리고 루프는 유지한다.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(request.fire)).is_err() {
                    tracing::error!(
                        "egui-mesh self-repaint notify panicked — request dropped, timer thread kept alive"
                    );
                }
            }
        }
    }

    /// 상주 루프가 풀렸을 때의 뒷정리 — 대기 요청을 비우고 가드를 전부 푼다.
    /// 가드가 armed 로 남으면 그 인스턴스는 스레드가 재기동돼도 재-arm 하지 못한다.
    fn release_pending_on_exit(&self, panicked: bool) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let stranded: Vec<SelfRepaintRequest> = pending.drain(..).collect();
        drop(pending);
        for request in &stranded {
            request
                .armed
                .store(false, std::sync::atomic::Ordering::Release);
        }
        tracing::error!(
            "egui-mesh self-repaint timer thread exited (panicked={panicked}) — {} pending request(s) released; the next repaint request restarts it",
            stranded.len()
        );
    }

    /// 기한이 지난 요청을 큐에서 꺼낸다. 잠금이 poison되면 복구해 계속 사용한다.
    fn take_due(&self) -> Vec<SelfRepaintRequest> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            let now = std::time::Instant::now();
            let mut due = Vec::new();
            let mut i = 0;
            while i < pending.len() {
                if pending[i].deadline <= now {
                    due.push(pending.swap_remove(i));
                } else {
                    i += 1;
                }
            }
            if !due.is_empty() {
                return due;
            }
            let next = pending.iter().map(|r| r.deadline).min();
            pending = match next {
                // 가장 이른 마감까지만 잔다 — 더 이른 요청이 들어오면 `notify_one` 이 깨운다.
                Some(deadline) => {
                    let wait = deadline.saturating_duration_since(std::time::Instant::now());
                    self.wake
                        .wait_timeout(pending, wait)
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .0
                }
                None => self
                    .wake
                    .wait(pending)
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            };
        }
    }
}

/// egui의 다음 렌더 요청을 공용 타이머에 예약한다. 대기 중이면 중복 예약하지
/// 않으며 알림 직전에 플래그를 풀어 다음 렌더에서 다시 예약할 수 있게 한다.
#[cfg(any(unix, windows))]
fn arm_self_repaint_timer(
    delay: Duration,
    armed: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    host: &HostHandle,
    notify: impl FnOnce(&HostHandle) -> Result<(), PluginError> + Send + 'static,
) {
    let host = host.clone();
    SelfRepaintTimer::global().arm(armed, delay, move || {
        if let Err(e) = notify(&host) {
            tracing::warn!("egui-mesh self-repaint notify failed: {e}");
        }
    });
}

/// 한 egui-mesh surface 의 plugin 측 렌더 상태. surface 하나당 인스턴스 하나를 둔다
/// (여러 surface 면 `surface_id` 별로 분리). drop 시 shared buffer 매핑이 해제된다.
pub struct EguiMeshSurface {
    surface_id: u32,
    core: EguiMeshCore,
    /// 이 인스턴스의 타이머 중복 예약을 막는 플래그. 알림 실행 전에 해제한다.
    #[cfg(any(unix, windows))]
    self_repaint_armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EguiMeshSurface {
    /// `surface_id` 에 대응하는 새 egui-mesh surface 를 만든다. egui `default_fonts` 가
    /// 설치된 독립 [`Context`] 를 소유한다.
    pub fn new(surface_id: u32) -> Self {
        Self {
            surface_id,
            core: EguiMeshCore::new(),
            #[cfg(any(unix, windows))]
            self_repaint_armed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 이 surface 의 host 측 식별자.
    pub fn surface_id(&self) -> u32 {
        self.surface_id
    }

    /// 폰트/스타일을 커스터마이즈할 수 있도록 내부 egui [`Context`] 를 노출한다.
    /// (예: host 와 동일 폰트 설치 → `surface.context().set_fonts(...)`.)
    pub fn context(&self) -> &Context {
        &self.core.ctx
    }

    /// set_context 입력으로 한 프레임을 인코딩한다. 출력 해시가 같으면 생략하며
    /// need_full_textures 요청이 있으면 전체 텍스처를 포함한다.
    pub fn run_frame(
        &mut self,
        params: &SurfaceSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Option<Vec<u8>> {
        self.run_frame_inner(params, run_ui).map(|f| f.bytes)
    }

    fn run_frame_inner(
        &mut self,
        params: &SurfaceSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Option<MeshFrame> {
        self.core.run_frame(
            params.width_px,
            params.height_px,
            params.pixels_per_point,
            params.theme.as_ref(),
            &params.raw_input,
            params.need_full_textures,
            run_ui,
        )
    }

    /// 직전 `set_context` 의 theme 스냅샷(재-paint closure 재구성용). 첫 컨텍스트 전 `None`.
    pub fn last_theme(&self) -> Option<&ThemeWire> {
        self.core.last_theme()
    }

    /// 직전 렌더에서 나온 CopyText를 한 번 꺼낸다. [`Self::paint`]·[`Self::repaint_last`] 는
    /// 이 값을 호스트에 알리며 소비하므로, 그 뒤에는 `None` 이다. [`Self::run_frame`] 만 쓰는
    /// 호출자가 꺼낼 때 쓴다.
    pub fn take_copied_text(&mut self) -> Option<String> {
        self.core.take_copied_text()
    }

    /// `set_context` 한 frame 을 그려 shared buffer 에 commit 하고 host 에
    /// [`PluginEvent::PaintFrame`] 알림을 보낸다. 출력이 직전과 같으면 `Ok(None)`,
    /// 변경됐으면 commit 후의 footer generation 을 `Ok(Some(gen))` 으로 반환한다.
    #[cfg(any(unix, windows))]
    pub fn paint(
        &mut self,
        host: &HostHandle,
        params: &SurfaceSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Result<Option<u64>, PluginError> {
        let frame = self.run_frame_inner(params, run_ui);
        self.schedule_self_repaint(host);
        self.core.notify_copied_text(
            host,
            CopyTargetWire::Surface {
                surface_id: self.surface_id,
            },
        )?;
        let Some(frame) = frame else {
            return Ok(None);
        };
        let byte_len = frame.bytes.len() as u32;
        let (buffer_id, generation) = self.core.commit(host, &frame.bytes)?;
        host.notify(&PluginEvent::PaintFrame {
            surface_id: self.surface_id,
            buffer_id,
            generation,
            frame_seq: self.core.next_frame_seq(),
            full_textures: frame.full_textures,
            byte_len,
            ime_cursor: self.core.ime_cursor(),
        })?;
        Ok(Some(generation))
    }

    /// egui가 다음 프레임을 요청하면 호스트에 SurfaceInvalidated를 보내도록 예약한다.
    /// 이 알림이 있어야 사용자 입력이 없어도 다음 렌더링을 요청할 수 있다.
    #[cfg(any(unix, windows))]
    fn schedule_self_repaint(&self, host: &HostHandle) {
        let Some(delay) = self.core.pending_self_repaint() else {
            return;
        };
        let surface_id = self.surface_id;
        arm_self_repaint_timer(delay, &self.self_repaint_armed, host, move |host| {
            host.notify(&PluginEvent::SurfaceInvalidated { surface_id })
        });
    }

    /// 사용자 이벤트 없이 캐시한 컨텍스트로 다시 그리고 필요한 프레임을 보낸다.
    #[cfg(any(unix, windows))]
    pub fn repaint_last(
        &mut self,
        host: &HostHandle,
        run_ui: impl FnMut(&Context),
    ) -> Result<Option<u64>, PluginError> {
        let frame = self.core.repaint_last(run_ui);
        self.schedule_self_repaint(host);
        self.core.notify_copied_text(
            host,
            CopyTargetWire::Surface {
                surface_id: self.surface_id,
            },
        )?;
        let Some(frame) = frame else {
            return Ok(None);
        };
        let byte_len = frame.bytes.len() as u32;
        let (buffer_id, generation) = self.core.commit(host, &frame.bytes)?;
        host.notify(&PluginEvent::PaintFrame {
            surface_id: self.surface_id,
            buffer_id,
            generation,
            frame_seq: self.core.next_frame_seq(),
            full_textures: frame.full_textures,
            byte_len,
            ime_cursor: self.core.ime_cursor(),
        })?;
        Ok(Some(generation))
    }
}

/// 한 egui-mesh popup 인스턴스의 plugin 측 렌더 상태(A2). [`EguiMeshSurface`] 의 popup
/// 대응 — 회신 알림이 [`PluginEvent::PopupPaintFrame`] 이고 `instance_id` 로 키잉되는
/// 점만 다르다. popup 인스턴스 하나당 하나를 두고, `popup.closed` 수신 시 drop 한다.
pub struct EguiMeshPopup {
    instance_id: u64,
    core: EguiMeshCore,
    /// [`EguiMeshSurface::self_repaint_armed`] 와 동형 — `schedule_self_repaint` 의
    /// 중복 타이머 요청 방지 가드.
    #[cfg(any(unix, windows))]
    self_repaint_armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EguiMeshPopup {
    /// `instance_id` 에 대응하는 새 egui-mesh popup 을 만든다. egui `default_fonts` 가
    /// 설치된 독립 [`Context`] 를 소유한다.
    pub fn new(instance_id: u64) -> Self {
        Self {
            instance_id,
            core: EguiMeshCore::new(),
            #[cfg(any(unix, windows))]
            self_repaint_armed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 이 popup 의 host 측 인스턴스 식별자.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// 폰트/스타일 커스터마이즈용 내부 egui [`Context`] 노출.
    pub fn context(&self) -> &Context {
        &self.core.ctx
    }

    /// `popup.set_context` 입력으로 한 frame 을 그려 POD mesh 바이트를 만든다.
    /// 정적 화면이면 `None`(송신 생략). `need_full_textures` 면 dedup 우회 + 전체 텍스처 동봉.
    pub fn run_frame(
        &mut self,
        params: &PopupSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Option<Vec<u8>> {
        self.run_frame_inner(params, run_ui).map(|f| f.bytes)
    }

    fn run_frame_inner(
        &mut self,
        params: &PopupSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Option<MeshFrame> {
        self.core.run_frame(
            params.width_px,
            params.height_px,
            params.pixels_per_point,
            params.theme.as_ref(),
            &params.raw_input,
            params.need_full_textures,
            run_ui,
        )
    }

    /// 직전 `popup.set_context` 의 theme 스냅샷(재-paint closure 재구성용). 첫 컨텍스트 전 `None`.
    pub fn last_theme(&self) -> Option<&ThemeWire> {
        self.core.last_theme()
    }

    /// [`EguiMeshSurface::take_copied_text`] 의 popup 대응. [`Self::paint`]·[`Self::repaint_last`]
    /// 는 이 값을 호스트에 알리며 소비한다.
    pub fn take_copied_text(&mut self) -> Option<String> {
        self.core.take_copied_text()
    }

    /// `popup.set_context` 한 frame 을 그려 shared buffer 에 commit 하고 host 에
    /// [`PluginEvent::PopupPaintFrame`] 알림을 보낸다. 정적 화면이면 `Ok(None)`.
    #[cfg(any(unix, windows))]
    pub fn paint(
        &mut self,
        host: &HostHandle,
        params: &PopupSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Result<Option<u64>, PluginError> {
        let frame = self.run_frame_inner(params, run_ui);
        self.schedule_self_repaint(host);
        self.core.notify_copied_text(
            host,
            CopyTargetWire::Popup {
                instance_id: self.instance_id,
            },
        )?;
        let Some(frame) = frame else {
            return Ok(None);
        };
        let (buffer_id, generation) = self.core.commit(host, &frame.bytes)?;
        host.notify(&PluginEvent::PopupPaintFrame {
            instance_id: self.instance_id,
            buffer_id,
            generation,
            frame_seq: self.core.next_frame_seq(),
            full_textures: frame.full_textures,
            ime_cursor: self.core.ime_cursor(),
        })?;
        Ok(Some(generation))
    }

    /// 사용자 이벤트 없이 캐시한 컨텍스트로 다시 그리고 필요한 팝업 프레임을 보낸다.
    #[cfg(any(unix, windows))]
    pub fn repaint_last(
        &mut self,
        host: &HostHandle,
        run_ui: impl FnMut(&Context),
    ) -> Result<Option<u64>, PluginError> {
        let frame = self.core.repaint_last(run_ui);
        self.schedule_self_repaint(host);
        self.core.notify_copied_text(
            host,
            CopyTargetWire::Popup {
                instance_id: self.instance_id,
            },
        )?;
        let Some(frame) = frame else {
            return Ok(None);
        };
        let (buffer_id, generation) = self.core.commit(host, &frame.bytes)?;
        host.notify(&PluginEvent::PopupPaintFrame {
            instance_id: self.instance_id,
            buffer_id,
            generation,
            frame_seq: self.core.next_frame_seq(),
            full_textures: frame.full_textures,
            ime_cursor: self.core.ime_cursor(),
        })?;
        Ok(Some(generation))
    }

    /// [`EguiMeshSurface::schedule_self_repaint`] 의 popup 대응 —
    /// [`PluginEvent::PopupInvalidated`] 로 host 의 popup pending-repaint 경로
    /// (docs/dev-guide/attach-behavior.md#커스텀-이벤트-확장-streamcontrol-밖-raw-json-event-태그 `plugin_mesh_popup_pending_repaint`)에 편승한다.
    #[cfg(any(unix, windows))]
    fn schedule_self_repaint(&self, host: &HostHandle) {
        let Some(delay) = self.core.pending_self_repaint() else {
            return;
        };
        let instance_id = self.instance_id;
        arm_self_repaint_timer(delay, &self.self_repaint_armed, host, move |host| {
            host.notify(&PluginEvent::PopupInvalidated { instance_id })
        });
    }
}

/// 한 egui-mesh banner 인스턴스의 plugin 측 렌더 상태(A3). [`EguiMeshPopup`] 의 banner
/// 대응 — 회신 알림이 [`PluginEvent::BannerPaintFrame`] 이고 `instance_id` 로 키잉되는
/// 점만 다르다. banner 인스턴스 하나당 하나를 두고, `banner.closed` 수신 시 drop 한다.
pub struct EguiMeshBanner {
    instance_id: u64,
    core: EguiMeshCore,
    /// [`EguiMeshPopup::self_repaint_armed`] 와 동형 — `schedule_self_repaint` 의
    /// 중복 타이머 요청 방지 가드.
    #[cfg(any(unix, windows))]
    self_repaint_armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EguiMeshBanner {
    /// `instance_id` 에 대응하는 새 egui-mesh banner 를 만든다. egui `default_fonts` 가
    /// 설치된 독립 [`Context`] 를 소유한다.
    pub fn new(instance_id: u64) -> Self {
        Self {
            instance_id,
            core: EguiMeshCore::new(),
            #[cfg(any(unix, windows))]
            self_repaint_armed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 이 banner 의 host 측 인스턴스 식별자.
    pub fn instance_id(&self) -> u64 {
        self.instance_id
    }

    /// 폰트/스타일 커스터마이즈용 내부 egui [`Context`] 노출.
    pub fn context(&self) -> &Context {
        &self.core.ctx
    }

    /// `banner.set_context` 입력으로 한 frame 을 그려 POD mesh 바이트를 만든다.
    /// 정적 화면이면 `None`(송신 생략). `need_full_textures` 면 dedup 우회 + 전체 텍스처 동봉.
    /// frame 전체를 배너 셸 문맥([`tasty_ui_widgets::banner_surface_ctx`]) 안에서 그리므로
    /// 공용 Secondary 버튼이 배너 박스를 쓴다.
    pub fn run_frame(
        &mut self,
        params: &BannerSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Option<Vec<u8>> {
        self.run_frame_inner(params, run_ui).map(|f| f.bytes)
    }

    fn run_frame_inner(
        &mut self,
        params: &BannerSetContextParams,
        mut run_ui: impl FnMut(&Context),
    ) -> Option<MeshFrame> {
        self.core.run_frame(
            params.width_px,
            params.height_px,
            params.pixels_per_point,
            params.theme.as_ref(),
            &params.raw_input,
            params.need_full_textures,
            |ctx| {
                tasty_ui_widgets::plugin_banner_body_host_tooltip(ctx, || {
                    tasty_ui_widgets::banner_surface_ctx(ctx, || run_ui(ctx));
                });
            },
        )
    }

    /// 캐시한 컨텍스트로 다시 그린다. 말줄임한 본문은 [`run_frame`](Self::run_frame)처럼 호스트 채널로 기록한다.
    fn repaint_last_frame(&mut self, mut run_ui: impl FnMut(&Context)) -> Option<MeshFrame> {
        self.core.repaint_last(|ctx| {
            tasty_ui_widgets::plugin_banner_body_host_tooltip(ctx, || run_ui(ctx));
        })
    }

    /// 직전 frame에서 말줄임한 plugin 본문을 wire 값으로 꺼낸다. 호스트가 배너 카드 아래 툴팁으로 보인다.
    fn take_body_cut(&self) -> Option<tasty_plugin_protocol::BannerBodyCutWire> {
        tasty_ui_widgets::take_plugin_banner_body_cut(&self.core.ctx).map(|cut| {
            tasty_plugin_protocol::BannerBodyCutWire {
                text: cut.text,
                body_rect: tasty_plugin_protocol::RectWire {
                    x: cut.body_rect.min.x,
                    y: cut.body_rect.min.y,
                    width: cut.body_rect.width(),
                    height: cut.body_rect.height(),
                },
            }
        })
    }

    /// 직전 `banner.set_context` 의 theme 스냅샷(재-paint closure 재구성용). 첫 컨텍스트 전 `None`.
    pub fn last_theme(&self) -> Option<&ThemeWire> {
        self.core.last_theme()
    }

    /// `banner.set_context` 한 frame 을 그려 shared buffer 에 commit 하고 host 에
    /// [`PluginEvent::BannerPaintFrame`] 알림을 보낸다. 정적 화면이면 `Ok(None)`.
    #[cfg(any(unix, windows))]
    pub fn paint(
        &mut self,
        host: &HostHandle,
        params: &BannerSetContextParams,
        run_ui: impl FnMut(&Context),
    ) -> Result<Option<u64>, PluginError> {
        let frame = self.run_frame_inner(params, run_ui);
        let body_cut = self.take_body_cut();
        // 정적 화면이어서 보낼 mesh 가 없어도 self-repaint 요청은 남을 수 있다
        // (hover fade 처럼 출력이 아직 안 바뀐 첫 frame). 그래서 popup 과 같이
        // **early return 앞에서** 예약한다 — 뒤에 두면 그 요청을 버린다.
        self.schedule_self_repaint(host);
        let Some(frame) = frame else {
            return Ok(None);
        };
        let (buffer_id, generation) = self.core.commit(host, &frame.bytes)?;
        host.notify(&PluginEvent::BannerPaintFrame {
            instance_id: self.instance_id,
            buffer_id,
            generation,
            frame_seq: self.core.next_frame_seq(),
            full_textures: frame.full_textures,
            body_cut,
        })?;
        Ok(Some(generation))
    }

    /// 사용자 이벤트 없이 캐시한 컨텍스트로 다시 그리고 필요한 배너 프레임을 보낸다.
    #[cfg(any(unix, windows))]
    pub fn repaint_last(
        &mut self,
        host: &HostHandle,
        run_ui: impl FnMut(&Context),
    ) -> Result<Option<u64>, PluginError> {
        let frame = self.repaint_last_frame(run_ui);
        let body_cut = self.take_body_cut();
        self.schedule_self_repaint(host);
        let Some(frame) = frame else {
            return Ok(None);
        };
        let (buffer_id, generation) = self.core.commit(host, &frame.bytes)?;
        host.notify(&PluginEvent::BannerPaintFrame {
            instance_id: self.instance_id,
            buffer_id,
            generation,
            frame_seq: self.core.next_frame_seq(),
            full_textures: frame.full_textures,
            body_cut,
        })?;
        Ok(Some(generation))
    }

    /// [`EguiMeshPopup::schedule_self_repaint`] 의 banner 대응 —
    /// [`PluginEvent::BannerInvalidated`] 로 host 의 banner pending-repaint 경로
    /// (`AppState::plugin_mesh_banner_pending_repaint`)에 편승한다. 보내는 variant
    /// 말고는 popup 판과 같다.
    #[cfg(any(unix, windows))]
    fn schedule_self_repaint(&self, host: &HostHandle) {
        let Some(delay) = self.core.pending_self_repaint() else {
            return;
        };
        let instance_id = self.instance_id;
        arm_self_repaint_timer(delay, &self.self_repaint_armed, host, move |host| {
            host.notify(&PluginEvent::BannerInvalidated { instance_id })
        });
    }
}

/// 전체 이미지에 부분 변경을 적용한다. 종류가 다르거나 범위를 벗어나면 경고 후 버린다.
#[allow(clippy::cognitive_complexity)] // complexity-exempt: ImageData 종류(Color/Font)별 bounds-check + copy 나열 — egui delta 계약상 두 kind 처리가 구조적으로 대칭이라 분해해도 절반짜리 로직 두 함수로만 흩어짐.
fn patch_image(base: &mut ImageData, patch: &ImageData, [x, y]: [usize; 2]) {
    match (base, patch) {
        (ImageData::Color(base), ImageData::Color(patch)) => {
            let [pw, ph] = patch.size;
            let [bw, bh] = base.size;
            if x + pw > bw || y + ph > bh {
                tracing::warn!("egui-mesh: color patch out of bounds; dropping");
                return;
            }
            let base = std::sync::Arc::make_mut(base);
            for row in 0..ph {
                let dst = (y + row) * bw + x;
                let src = row * pw;
                base.pixels[dst..dst + pw].copy_from_slice(&patch.pixels[src..src + pw]);
            }
        }
        (ImageData::Font(base), ImageData::Font(patch)) => {
            let [pw, ph] = patch.size;
            let [bw, bh] = base.size;
            if x + pw > bw || y + ph > bh {
                tracing::warn!("egui-mesh: font patch out of bounds; dropping");
                return;
            }
            for row in 0..ph {
                let dst = (y + row) * bw + x;
                let src = row * pw;
                base.pixels[dst..dst + pw].copy_from_slice(&patch.pixels[src..src + pw]);
            }
        }
        _ => tracing::warn!("egui-mesh: texture patch kind mismatch; dropping"),
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::Primitive;
    use tasty_plugin_protocol::RawInputWire;
    use tasty_plugin_protocol::mesh_wire::decode_paint;

    /// 상주 타이머가 등록된 요청을 **각각 정확히 한 번** 발사하고(유실 0 · 중복 0),
    /// 발사 시 arm 가드를 풀어 다음 프레임이 재-arm 할 수 있게 하는지.
    /// 지연 0(스크롤 스무딩의 상시 경로)과 지연 있는 요청을 함께 건다.
    #[cfg(any(unix, windows))]
    #[test]
    fn self_repaint_timer_fires_each_request_exactly_once() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let timer = SelfRepaintTimer::global();
        let delays = [
            Duration::ZERO,
            Duration::from_millis(5),
            Duration::from_millis(15),
        ];
        let guards: Vec<Arc<AtomicBool>> = delays
            .iter()
            .map(|_| Arc::new(AtomicBool::new(false)))
            .collect();
        let counters: Vec<Arc<AtomicUsize>> = delays
            .iter()
            .map(|_| Arc::new(AtomicUsize::new(0)))
            .collect();
        for ((delay, guard), counter) in delays.iter().zip(&guards).zip(&counters) {
            let counter = Arc::clone(counter);
            timer.arm(guard, *delay, move || {
                counter.fetch_add(1, Ordering::Release);
            });
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline
            && counters.iter().any(|c| c.load(Ordering::Acquire) == 0)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        for (i, counter) in counters.iter().enumerate() {
            assert_eq!(counter.load(Ordering::Acquire), 1, "request {i} fired once");
        }
        // 발사 뒤에는 가드가 풀려 있어야 다음 render() 가 재-arm 할 수 있다.
        for (i, guard) in guards.iter().enumerate() {
            assert!(!guard.load(Ordering::Acquire), "guard {i} released on fire");
        }
    }

    /// 단조 시계 표현 범위를 넘기는 지연은 **panic 없이** 버려지고, arm 가드도
    /// armed 로 고착되지 않는다 — `Instant + Duration` 은 넘칠 때 panic 하므로
    /// `checked_add` 로 막는다(`render` 의 필터는 `Duration::MAX` 자신만 걸러낸다).
    #[cfg(any(unix, windows))]
    #[test]
    fn self_repaint_overflowing_delay_is_dropped_without_panic() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let timer = SelfRepaintTimer::global();
        let guard = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicUsize::new(0));
        let fired = Arc::clone(&counter);
        timer.arm(&guard, Duration::MAX, move || {
            fired.fetch_add(1, Ordering::Release);
        });
        assert_eq!(counter.load(Ordering::Acquire), 0, "request dropped");
        assert!(!guard.load(Ordering::Acquire), "guard not left armed");

        // 가드가 깨끗하므로 정상 지연 요청은 그대로 받아들여진다.
        let fired = Arc::clone(&counter);
        timer.arm(&guard, Duration::ZERO, move || {
            fired.fetch_add(1, Ordering::Release);
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline && counter.load(Ordering::Acquire) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            counter.load(Ordering::Acquire),
            1,
            "normal request still fires"
        );
    }

    /// arm 가드는 대기 중인 요청이 있는 동안의 재-arm 을 삼킨다 — 매 프레임 불려도
    /// 알림은 한 번만 나간다(중복 set_context 왕복 방지). 발사 뒤에는 다시 arm 된다.
    #[cfg(any(unix, windows))]
    #[test]
    fn self_repaint_arm_guard_collapses_requests_while_pending() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let timer = SelfRepaintTimer::global();
        let guard = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicUsize::new(0));
        // 첫 요청이 대기하는 동안 같은 가드로 여러 번 재-arm 해도 큐에는 1건만 남는다.
        for _ in 0..8 {
            let counter = Arc::clone(&counter);
            timer.arm(&guard, Duration::from_millis(20), move || {
                counter.fetch_add(1, Ordering::Release);
            });
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline && counter.load(Ordering::Acquire) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            counter.load(Ordering::Acquire),
            1,
            "collapsed to one notify"
        );
        assert!(!guard.load(Ordering::Acquire), "guard released on fire");

        // 가드가 풀렸으므로 다음 요청은 다시 받아들여진다(유실 없음).
        let counter2 = Arc::clone(&counter);
        timer.arm(&guard, Duration::ZERO, move || {
            counter2.fetch_add(1, Ordering::Release);
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline && counter.load(Ordering::Acquire) < 2 {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            counter.load(Ordering::Acquire),
            2,
            "re-arm after fire works"
        );
    }

    fn ctx_params(width_px: u32, height_px: u32, ppp: f32) -> SurfaceSetContextParams {
        SurfaceSetContextParams {
            surface_id: 1,
            width_px,
            height_px,
            pixels_per_point: ppp,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        }
    }

    /// set_context → run → tessellate → encode 라운드. 인코드 결과가 decode_paint 로
    /// 복원되고, 라벨을 그리면 mesh + 폰트 atlas 가 실린다.
    #[test]
    fn run_frame_round_trips_through_decode() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(400, 300, 2.0);
        let bytes = surface
            .run_frame(&params, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Tasty egui-mesh");
                    ui.label("A1-S4 plugin SDK helper");
                });
            })
            .expect("first frame is always a change");

        let decoded = decode_paint(&bytes).expect("decode encoded mesh");
        // ppp 가 native_pixels_per_point 로 전달돼 출력 ppp 와 정합해야 한다.
        assert_eq!(decoded.pixels_per_point, 2.0);
        let mesh_count = decoded
            .primitives
            .iter()
            .filter(|p| matches!(p.primitive, Primitive::Mesh(_)))
            .count();
        assert!(mesh_count > 0, "label/heading should tessellate to meshes");
        assert!(
            !decoded.textures_delta.set.is_empty(),
            "first frame should carry the font atlas"
        );
    }

    /// 정적 화면: 같은 입력+UI 를 반복하면 egui 가 안정화(폰트 atlas 업로드·레이아웃
    /// 수렴)된 뒤 출력 무변화 → None(송신 생략)으로 떨어지고, 이후로도 None 을 유지한다.
    #[test]
    fn static_frame_stabilizes_to_no_change() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(320, 200, 1.0);
        let draw = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("static");
            });
        };
        // 첫 frame 은 항상 변화로 송신된다.
        assert!(
            surface.run_frame(&params, draw).is_some(),
            "first frame paints"
        );
        // 몇 frame 안에 무변화(None)로 수렴해야 한다.
        let stabilized = (0..5).any(|_| surface.run_frame(&params, draw).is_none());
        assert!(
            stabilized,
            "static UI should converge to no-change within a few frames"
        );
        // 수렴 후 동일 입력은 계속 송신 생략.
        assert!(
            surface.run_frame(&params, draw).is_none(),
            "stable static UI stays skipped"
        );
    }

    /// 출력이 바뀌면(다른 텍스트) 다시 Some 을 돌려준다.
    #[test]
    fn changed_output_paints_again() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(320, 200, 1.0);
        assert!(
            surface
                .run_frame(&params, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("before");
                    });
                })
                .is_some()
        );
        assert!(
            surface
                .run_frame(&params, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("after");
                    });
                })
                .is_some(),
            "different content must repaint"
        );
    }

    /// 첫 set_context 전(캐시 없음)엔 재-paint 가 no-op 이어야 한다(옵션 A 계약).
    #[test]
    fn repaint_last_is_noop_before_first_context() {
        let mut surface = EguiMeshSurface::new(1);
        let out = surface.core.repaint_last(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("no context yet");
            });
        });
        assert!(out.is_none(), "no cached context → repaint is a no-op");
    }

    /// out-of-band 상태 변경(입력 없이) 뒤 재-paint: 캐시된 geom 으로 재-run 하되
    /// 출력이 같으면 dedup(None), 바뀌면 프레임 생성(Some).
    #[test]
    fn repaint_last_repaints_only_on_output_change() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(320, 200, 1.0);
        let draw_a = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("a");
            });
        };
        // 정적 화면으로 수렴시켜 폰트 atlas 업로드까지 안정화한다.
        surface.run_frame(&params, draw_a);
        for _ in 0..5 {
            surface.run_frame(&params, draw_a);
        }
        // 같은 내용의 재-paint 는 dedup 으로 송신 생략.
        assert!(
            surface.core.repaint_last(draw_a).is_none(),
            "unchanged output dedups to no-send"
        );
        // out-of-band 로 내용이 바뀌면 입력 없이도 재-paint 가 프레임을 만든다.
        let changed = surface.core.repaint_last(|ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("b");
            });
        });
        assert!(
            changed.is_some(),
            "changed output must repaint without any user input"
        );
    }

    /// identity 불변식: 재-paint 가 재현하는 입력에는 사용자 이벤트가 하나도 없다.
    /// (focused 는 이벤트가 아닌 지속 상태라 캐시 재현 대상 — 아래
    /// `repaint_last_preserves_last_focused` 가 검증.)
    #[test]
    fn empty_raw_input_carries_no_events() {
        let raw = build_raw_input(320, 200, 1.0, &RawInputWire::default());
        assert!(
            raw.events.is_empty(),
            "repaint replays with empty events — no fake events injected"
        );
        assert!(!raw.focused, "default wire maps to focused=false");
    }

    /// 입력 없이 다시 그릴 때에도 직전 focused 값을 유지하는지 확인한다.
    #[test]
    fn repaint_last_preserves_last_focused() {
        let mut surface = EguiMeshSurface::new(1);
        let focused_params = SurfaceSetContextParams {
            raw_input: RawInputWire {
                focused: true,
                ..Default::default()
            },
            ..ctx_params(320, 200, 1.0)
        };
        surface.run_frame(&focused_params, |ctx| {
            assert!(ctx.input(|i| i.focused), "set_context frame is focused");
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("a");
            });
        });
        // 재-paint 재-run 프레임에서도 i.focused 가 true 로 유지돼야 한다.
        let mut checked = false;
        let _ = surface.core.repaint_last(|ctx| {
            checked = true;
            assert!(
                ctx.input(|i| i.focused),
                "repaint must replay the last focused state, not default(false)"
            );
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("b");
            });
        });
        assert!(checked, "repaint ran the ui closure");

        // 직전이 unfocused 였으면 캐시값도 false — banner(항상 focused=false forward)
        // 등의 기존 동작 불변.
        surface.run_frame(&ctx_params(320, 200, 1.0), |ctx| {
            assert!(!ctx.input(|i| i.focused));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("c");
            });
        });
        let _ = surface.core.repaint_last(|ctx| {
            assert!(
                !ctx.input(|i| i.focused),
                "unfocused last context replays unfocused"
            );
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("d");
            });
        });
    }

    /// 캐시된 theme 스냅샷이 set_context 뒤 노출되고, theme 미동봉이면 None.
    #[test]
    fn last_theme_reflects_last_set_context() {
        let mut surface = EguiMeshSurface::new(1);
        assert!(surface.last_theme().is_none(), "no context yet → no theme");
        // theme 미동봉 params.
        let params = ctx_params(320, 200, 1.0);
        surface.run_frame(&params, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("x");
            });
        });
        assert!(
            surface.last_theme().is_none(),
            "set_context without theme → last_theme stays None"
        );
    }

    /// Context 생성 후 첫 frame 은 자연-full 로 마킹된다 — bootstrap race(gen1 이
    /// 디코드 전에 덮여도 host 가 full 재요청으로 회복)의 전제.
    #[test]
    fn first_frame_is_naturally_full() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(320, 200, 1.0);
        let frame = surface
            .run_frame_inner(&params, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("first");
                });
            })
            .expect("first frame paints");
        assert!(
            frame.full_textures,
            "first frame carries the whole texture state (font atlas full image)"
        );
        assert!(
            !surface.core.tex_state.is_empty(),
            "font atlas accumulated into tex_state"
        );
    }

    /// need_full_textures: 정적 화면(dedup 수렴)이어도 강제 송신되고, 누적 텍스처
    /// 전체가 full image(pos == None)로 동봉되며 full 로 마킹된다.
    #[test]
    fn need_full_bypasses_dedup_and_carries_all_textures() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(320, 200, 1.0);
        let draw = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("static");
            });
        };
        // 정적 수렴시켜 일반 재-run 은 None 이 되게 한다.
        for _ in 0..6 {
            surface.run_frame(&params, draw);
        }
        assert!(surface.run_frame(&params, draw).is_none(), "converged");

        let full_params = SurfaceSetContextParams {
            need_full_textures: true,
            ..ctx_params(320, 200, 1.0)
        };
        let frame = surface
            .run_frame_inner(&full_params, draw)
            .expect("need_full forces a send even when output is unchanged");
        assert!(frame.full_textures);

        let decoded = decode_paint(&frame.bytes).expect("decode");
        assert_eq!(
            decoded.textures_delta.set.len(),
            surface.core.tex_state.len(),
            "full frame carries every accumulated texture"
        );
        assert!(
            decoded
                .textures_delta
                .set
                .iter()
                .all(|(_, d)| d.pos.is_none()),
            "full frame textures are full images"
        );
        assert!(decoded.textures_delta.free.is_empty());
    }

    /// 누적층: full → patch 합성 → free 제거, 자연-full 판정.
    #[test]
    fn accumulate_textures_composites_patches_and_frees() {
        use egui::epaint::ColorImage;
        use egui::{Color32, epaint::textures::TexturesDelta};

        let mut core = EguiMeshCore::new();
        let id = TextureId::User(7);
        let full = ImageDelta {
            image: ImageData::Color(std::sync::Arc::new(ColorImage {
                size: [2, 2],
                pixels: vec![Color32::BLACK; 4],
            })),
            options: Default::default(),
            pos: None,
        };
        let delta = TexturesDelta {
            set: vec![(id, full)],
            free: vec![],
        };
        assert!(
            core.accumulate_textures(&delta),
            "all-full delta is naturally full"
        );

        // (1,1) 에 1x1 patch — 해당 픽셀만 바뀐다.
        let patch = ImageDelta {
            image: ImageData::Color(std::sync::Arc::new(ColorImage {
                size: [1, 1],
                pixels: vec![Color32::WHITE],
            })),
            options: Default::default(),
            pos: Some([1, 1]),
        };
        let delta = TexturesDelta {
            set: vec![(id, patch)],
            free: vec![],
        };
        assert!(
            !core.accumulate_textures(&delta),
            "patch-only delta is not full"
        );
        let (img, _) = core.tex_state.get(&id).expect("texture retained");
        let ImageData::Color(img) = img else {
            panic!("expected color image");
        };
        assert_eq!(img.pixels[3], Color32::WHITE, "patched pixel at (1,1)");
        assert_eq!(img.pixels[0], Color32::BLACK, "other pixels intact");

        // free → 누적 상태에서 제거, full 재구성에서 빠진다.
        let delta = TexturesDelta {
            set: vec![],
            free: vec![id],
        };
        core.accumulate_textures(&delta);
        assert!(core.tex_state.is_empty());
        assert!(core.full_texture_delta().set.is_empty());
    }

    /// RawInputWire → egui RawInput 매핑: 좌표 보존, 키 이름 파싱, 매핑 불가 키 드롭.
    #[test]
    fn raw_input_mapping_covers_pointer_scroll_key_text() {
        let wire = RawInputWire {
            time: Some(1.5),
            focused: true,
            modifiers: ModifiersWire {
                ctrl: true,
                ..Default::default()
            },
            events: vec![
                RawInputEventWire::PointerMoved { x: 12.0, y: 34.0 },
                RawInputEventWire::PointerButton {
                    x: 12.0,
                    y: 34.0,
                    button: PointerButtonWire::Secondary,
                    pressed: true,
                    modifiers: ModifiersWire::default(),
                },
                // 분할 임계(`SCROLL_SPLIT_STEP`) 아래라 1:1 로 남는다 — 1:N 분할은
                // `scroll_delta_is_split_below_egui_smooth_limit` 가 따로 고정한다.
                RawInputEventWire::Scroll { x: 0.0, y: -4.0 },
                RawInputEventWire::Key {
                    key: "Enter".into(),
                    pressed: true,
                    repeat: false,
                    modifiers: ModifiersWire::default(),
                },
                // 매핑 불가한 키 이름 → 드롭.
                RawInputEventWire::Key {
                    key: "NotARealKey".into(),
                    pressed: true,
                    repeat: false,
                    modifiers: ModifiersWire::default(),
                },
                RawInputEventWire::Text { text: "hi".into() },
            ],
        };
        let params = SurfaceSetContextParams {
            surface_id: 7,
            width_px: 200,
            height_px: 100,
            pixels_per_point: 2.0,
            raw_input: wire,
            theme: None,
            need_full_textures: false,
        };
        let raw = build_raw_input(
            params.width_px,
            params.height_px,
            params.pixels_per_point,
            &params.raw_input,
        );

        // screen_rect = 물리 px / ppp.
        let rect = raw.screen_rect.expect("screen_rect");
        assert_eq!(rect.width(), 100.0);
        assert_eq!(rect.height(), 50.0);
        assert_eq!(
            raw.viewports
                .get(&raw.viewport_id)
                .and_then(|v| v.native_pixels_per_point),
            Some(2.0)
        );
        assert_eq!(raw.time, Some(1.5));
        assert!(raw.focused);
        assert!(raw.modifiers.ctrl);

        // 매핑 불가 키 1개가 드롭돼 6개 중 5개만 남는다.
        assert_eq!(raw.events.len(), 5);
        assert!(matches!(raw.events[0], Event::PointerMoved(p) if p == Pos2::new(12.0, 34.0)));
        assert!(matches!(
            raw.events[1],
            Event::PointerButton {
                button: PointerButton::Secondary,
                pressed: true,
                ..
            }
        ));
        assert!(matches!(
            raw.events[2],
            Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                ..
            }
        ));
        assert!(matches!(
            raw.events[3],
            Event::Key {
                key: Key::Enter,
                pressed: true,
                ..
            }
        ));
        assert!(matches!(&raw.events[4], Event::Text(t) if t == "hi"));
    }

    /// IME wire 4단계가 각각 대응하는 `egui::Event::Ime(ImeEvent::…)` 로 매핑된다.
    /// markdown 주소창의 라이브 preedit 표시가 이 매핑에 의존한다.
    #[test]
    fn ime_wire_maps_to_egui_ime_events() {
        let cases = [
            (ImeWire::Enabled, ImeEvent::Enabled),
            (
                ImeWire::Preedit { text: "ㅎ".into() },
                ImeEvent::Preedit("ㅎ".into()),
            ),
            (
                ImeWire::Commit { text: "한".into() },
                ImeEvent::Commit("한".into()),
            ),
            (ImeWire::Disabled, ImeEvent::Disabled),
        ];
        for (wire, expected) in cases {
            let mapped = map_event(&RawInputEventWire::Ime { event: wire });
            assert_eq!(mapped, Some(Event::Ime(expected)));
        }
    }

    /// 창과 입력란이 모두 포커스를 가져도 커서가 깜박이지 않아 다시 그릴 요청이 남지 않는다.
    #[test]
    fn a_focused_text_field_does_not_ask_for_blink_frames() {
        let mut surface = EguiMeshSurface::new(1);
        let mut params = ctx_params(320, 100, 1.0);
        params.raw_input.focused = true;
        let id = egui::Id::new("caret_test");
        let mut buf = "text".to_string();
        let mut draw = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                ui.add(egui::TextEdit::singleline(&mut buf).id(id));
            });
        };
        // 첫 몇 프레임은 egui 자체 안정화 요청이 섞일 수 있어 수렴 여부만 본다.
        let settled = (0..6).any(|_| {
            surface.run_frame(&params, &mut draw);
            surface.core.pending_self_repaint().is_none()
        });
        assert!(settled, "a focused caret kept asking for frames");
    }

    /// egui의 다음 렌더 요청이 pending_self_repaint에 남는지 확인한다.
    #[test]
    fn render_captures_egui_repaint_request_instead_of_dropping_it() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(320, 200, 1.0);
        let draw_static = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("static");
            });
        };

        // bootstrap frame(폰트 atlas 업로드 등 첫 frame 특유의 repaint 요청이 섞일 수
        // 있어 baseline 값 자체는 단언하지 않는다 — 안정화 목적으로만 1회 그린다).
        surface.run_frame(&params, draw_static);

        // egui 내부 애니메이션(스크롤 스무딩 등)에 의존하지 않고, `request_repaint_after`
        // 를 직접 호출해 "다음 pass 필요" 신호를 결정적으로 재현한다.
        surface.run_frame(&params, |ctx| {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            draw_static(ctx);
        });
        let delay = surface
            .core
            .pending_self_repaint()
            .expect("egui's repaint request must not be dropped by render()");
        assert!(
            delay <= std::time::Duration::from_millis(50),
            "captured delay must reflect (or be tighter than) the requested 50ms, got {delay:?}"
        );

        // 다음 pass 부터는 더 이상 요청하지 않으면 정보가 stale 하게 남지 않고
        // 갱신된다 — 몇 프레임 안에 None 으로 수렴해야 한다(첫 몇 frame 은 egui 의
        // 자체 안정화 repaint 요청이 섞일 수 있어 `static_frame_stabilizes_to_no_change`
        // 와 동일하게 수렴 여부만 확인한다).
        let converged = (0..5).any(|_| {
            surface.run_frame(&params, draw_static);
            surface.core.pending_self_repaint().is_none()
        });
        assert!(
            converged,
            "pending_self_repaint must not stay stuck once egui stops requesting repaints"
        );
    }

    /// 한 스크롤 pass 뒤 화면이 멎을 때까지 필요한 **추가 pass 수**. egui-mesh 에서는
    /// 추가 pass 하나가 곧 self-repaint 알림 → `set_context` → 전체 egui pass 라는
    /// 프로세스 간 왕복 한 번이다.
    fn passes_until_settled(events: Vec<Event>) -> usize {
        let mut core = EguiMeshCore::new();
        // bootstrap — 폰트 atlas 업로드 등 첫 frame 특유의 요청을 먼저 소진시킨다.
        for _ in 0..16 {
            core.render(raw_with(Vec::new()), false, draw_scroll_area);
            if core.pending_self_repaint().is_none() {
                break;
            }
        }
        assert!(
            core.pending_self_repaint().is_none(),
            "bootstrap must settle before measuring"
        );
        core.render(raw_with(events), false, draw_scroll_area);
        let mut extra = 0;
        while core.pending_self_repaint().is_some() && extra < 64 {
            core.render(raw_with(Vec::new()), false, draw_scroll_area);
            extra += 1;
        }
        extra
    }

    fn raw_with(events: Vec<Event>) -> RawInput {
        let mut raw = build_raw_input(320, 200, 1.0, &RawInputWire::default());
        raw.events = events;
        raw
    }

    /// 스크롤 가능한 데모 UI — 내용이 넘쳐야 `ScrollArea` 가 델타를 소비한다.
    fn draw_scroll_area(ctx: &Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for i in 0..80 {
                    ui.label(format!("scrollable line {i}"));
                }
            });
        });
    }

    /// 휠 델타를 분할하면 원래 한 건을 그대로 보낼 때보다 추가 렌더 횟수가 줄어드는지 확인한다.
    #[test]
    fn splitting_a_wheel_delta_cuts_the_follow_up_repaint_passes() {
        let delta = vec2(0.0, -50.0);
        let unsplit = passes_until_settled(vec![wheel_event(delta)]);
        let mut split_events = Vec::new();
        push_scroll_events(&mut split_events, delta);
        let split = passes_until_settled(split_events);

        assert!(
            unsplit >= 4,
            "baseline sanity: an unsplit {delta:?} must ride egui's multi-pass smoothing, got {unsplit}"
        );
        assert!(
            split < unsplit,
            "splitting must cut follow-up passes: split={split} unsplit={unsplit}"
        );
        assert!(
            split <= 2,
            "a split delta must settle within a pass or two, got {split}"
        );
    }

    /// 분할 계약: 조각 하나하나가 egui 의 smooth 판정선 **미만**이고, 조각 합이 원본
    /// 델타와 (부동소수 오차 범위에서) 같다 — 이동량 보존 = 델타 유실 없음.
    #[test]
    fn scroll_delta_is_split_below_egui_smooth_limit() {
        for delta in [
            vec2(0.0, -50.0),
            vec2(0.0, 120.0),
            vec2(-30.0, 40.0),
            vec2(0.0, -7.9),
        ] {
            let mut out = Vec::new();
            push_scroll_events(&mut out, delta);
            let mut sum = Vec2::ZERO;
            for e in &out {
                let Event::MouseWheel { unit, delta: d, .. } = e else {
                    panic!("push_scroll_events must only emit MouseWheel, got {e:?}");
                };
                assert_eq!(*unit, MouseWheelUnit::Point);
                assert!(
                    d.length() < EGUI_SMOOTH_WHEEL_LIMIT,
                    "part {d:?} (len {}) must stay under egui's smooth limit",
                    d.length()
                );
                sum += *d;
            }
            assert!(
                (sum - delta).length() < 1e-3,
                "split parts must sum back to {delta:?}, got {sum:?}"
            );
        }
    }

    /// 임계 아래 델타는 쪼개지 않는다(이벤트 1건 그대로), 0 델타도 그대로 통과한다.
    #[test]
    fn small_scroll_delta_is_not_split() {
        for delta in [vec2(0.0, -4.0), vec2(0.0, 0.0), vec2(1.0, 1.0)] {
            let mut out = Vec::new();
            push_scroll_events(&mut out, delta);
            assert_eq!(out.len(), 1, "{delta:?} must stay a single event");
        }
    }

    /// 스크롤 가능한 데모 UI 의 계측판 — `ScrollArea` 의 세로 offset 을 기록한다.
    /// hover 판정이 레이아웃에 좌우되지 않도록 `auto_shrink` 를 꺼 영역을 패널 전체로
    /// 편다(이 테스트가 보려는 것은 레이아웃이 아니라 **와이어 델타의 도착**이다).
    fn draw_scroll_area_measured(ctx: &Context, offset: &std::cell::Cell<f32>) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let out = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for i in 0..80 {
                        ui.label(format!("scrollable line {i}"));
                    }
                });
            offset.set(out.state.offset.y);
        });
    }

    /// 포인터가 놓인 스크롤 영역에서 분할한 휠 델타만큼 콘텐츠가 이동하는지 확인한다.
    #[test]
    fn a_wheel_delta_actually_moves_the_scroll_offset() {
        let (w, h, ppp) = (320u32, 200u32, 1.0f32);
        let hover = egui::pos2(w as f32 / 2.0, h as f32 / 2.0);
        let mut core = EguiMeshCore::new();
        let offset = std::cell::Cell::new(f32::NAN);

        let frame = |events: Vec<Event>, core: &mut EguiMeshCore| {
            let mut raw = build_raw_input(w, h, ppp, &RawInputWire::default());
            raw.events = events;
            core.render(raw, false, |ctx| draw_scroll_area_measured(ctx, &offset));
        };

        // bootstrap — 폰트 atlas 등 첫 frame 특유의 요청을 소진시키고 hover 를 세운다.
        for _ in 0..16 {
            frame(vec![Event::PointerMoved(hover)], &mut core);
            if core.pending_self_repaint().is_none() {
                break;
            }
        }
        assert_eq!(offset.get(), 0.0, "시작 offset 은 0 이어야 한다");

        let delta = vec2(0.0, -50.0);
        let mut events = vec![Event::PointerMoved(hover)];
        push_scroll_events(&mut events, delta);
        frame(events, &mut core);
        // 분할 델타는 입력 pass 에서 전량 소비되지만, 소비된 값이 다음 프레임의 레이아웃에
        // 반영되는 것을 한 프레임 더 돌려서 읽는다.
        frame(vec![Event::PointerMoved(hover)], &mut core);

        assert!(
            (offset.get() - 50.0).abs() < 1.0,
            "휠 50pt 가 offset 을 그만큼 움직여야 한다 — 실제 {}",
            offset.get()
        );

        // 포인터가 없는 경우에는 같은 델타로 스크롤되지 않는지 함께 확인한다.
        let mut never_hovered = EguiMeshCore::new();
        let idle = std::cell::Cell::new(f32::NAN);
        let idle_frame = |events: Vec<Event>, core: &mut EguiMeshCore| {
            let mut raw = build_raw_input(w, h, ppp, &RawInputWire::default());
            raw.events = events;
            core.render(raw, false, |ctx| draw_scroll_area_measured(ctx, &idle));
        };
        for _ in 0..16 {
            idle_frame(Vec::new(), &mut never_hovered);
            if never_hovered.pending_self_repaint().is_none() {
                break;
            }
        }
        let mut wheel_only = Vec::new();
        push_scroll_events(&mut wheel_only, delta);
        idle_frame(wheel_only, &mut never_hovered);
        idle_frame(Vec::new(), &mut never_hovered);
        assert_eq!(
            idle.get(),
            0.0,
            "포인터가 없는 surface는 휠 입력으로 움직이지 않아야 한다"
        );
    }

    /// 상한을 넘는 극단적 델타는 쪼개지 않고 그대로 넘긴다(이벤트 폭증 방지).
    /// 이 경우에만 egui 기본 스무딩으로 되돌아간다.
    #[test]
    fn oversized_scroll_delta_falls_back_to_a_single_event() {
        let mut out = Vec::new();
        let huge = vec2(
            0.0,
            -(SCROLL_SPLIT_STEP * SCROLL_SPLIT_MAX_PARTS as f32) - 1.0,
        );
        push_scroll_events(&mut out, huge);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], Event::MouseWheel { delta, .. } if delta == huge));
    }

    /// `EguiMeshCore` 는 생성 시점에 프로그램적 스크롤 애니메이션을 꺼 둔다 — dark/light
    /// 양쪽 style 모두(테마 전환에도 유지). egui-mesh 는 애니메이션 프레임 하나가 곧
    /// 프로세스 간 왕복 한 번이다(docs/dev-guide/egui-mesh-channel.md#입력-forward--사용자-입력-경계).
    #[test]
    fn mesh_context_disables_scroll_animation() {
        let core = EguiMeshCore::new();
        let none = egui::style::ScrollAnimation::none();
        let (dark, light) = core.ctx.options(|o| {
            (
                o.dark_style.scroll_animation,
                o.light_style.scroll_animation,
            )
        });
        assert_eq!(dark, none, "dark style must disable scroll animation");
        assert_eq!(light, none, "light style must disable scroll animation");
    }

    /// [`EguiMeshPopup`] 도 [`EguiMeshCore::pending_self_repaint`] 를 공유한다 — 위
    /// surface 테스트와 동형으로, popup 채널(git-viewer/clipboard-viewer)도 같은
    /// 정보 유실 없이 self-repaint 요청을 캡처해야 한다(`docs/dev-guide/egui-mesh-channel.md` "popup·banner 대응",
    /// `PopupInvalidated`).
    #[test]
    fn popup_also_captures_egui_repaint_request() {
        let mut popup = EguiMeshPopup::new(1);
        let params = PopupSetContextParams {
            instance_id: 1,
            width_px: 320,
            height_px: 200,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        popup.run_frame(&params, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("popup content");
            });
        });
        popup.run_frame(&params, |ctx| {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("popup content");
            });
        });
        assert!(
            popup.core.pending_self_repaint().is_some(),
            "popup render() must not drop egui's repaint request either"
        );
    }

    /// Banner도 egui의 다음 렌더 요청을 보관하는지 확인한다.
    /// 이 시험은 run_frame만 호출하므로 실제 알림 송신은 아래 소켓 시험에서 확인한다.
    #[test]
    fn banner_also_captures_egui_repaint_request() {
        let mut banner = EguiMeshBanner::new(1);
        let params = BannerSetContextParams {
            instance_id: 1,
            width_px: 320,
            height_px: 64,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        banner.run_frame(&params, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("banner content");
            });
        });
        banner.run_frame(&params, |ctx| {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("banner content");
            });
        });
        assert!(
            banner.core.pending_self_repaint().is_some(),
            "banner render() must not drop egui's repaint request either"
        );
    }

    /// Banner frame은 배너 셸 문맥 안에서 그리고, 끝나면 문맥을 닫는다.
    #[test]
    fn banner_frame_runs_inside_the_banner_context() {
        let mut banner = EguiMeshBanner::new(1);
        let params = BannerSetContextParams {
            instance_id: 1,
            width_px: 320,
            height_px: 64,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        let mut inside = false;
        banner.run_frame(&params, |ctx| {
            inside = tasty_ui_widgets::in_banner_surface(ctx);
        });
        assert!(inside, "banner content must see the banner context");
        assert!(!tasty_ui_widgets::in_banner_surface(banner.context()));
    }

    const CUT_BODY: &str = "A plugin body long enough to run past three rows at this banner width, \
                            so the widget cuts it and the host has to show the full text in a \
                            tooltip below the banner card instead of inside the plugin mesh.";

    fn cut_body_params() -> BannerSetContextParams {
        BannerSetContextParams {
            instance_id: 3,
            width_px: 160,
            height_px: 64,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        }
    }

    fn cut_body_ui(ctx: &Context) {
        let theme = tasty_type_appearance::theme::Theme::with_colors_and_zoom(
            tasty_themes::mocha_fallback_colors(),
            false,
            1.0,
        );
        egui::CentralPanel::default().show(ctx, |ui| {
            tasty_ui_widgets::plugin_banner_body(ui, &theme, CUT_BODY);
        });
    }

    /// banner frame에서 말줄임한 plugin 본문은 호스트에 보낼 잘림으로 남고, 콘텐츠 영역 좌표를 쓴다.
    #[test]
    fn a_cut_plugin_body_is_reported_for_the_host_tooltip() {
        let mut banner = EguiMeshBanner::new(3);
        banner.run_frame(&cut_body_params(), cut_body_ui);
        let cut = banner.take_body_cut().expect("cut");
        assert_eq!(cut.text, CUT_BODY);
        assert!(cut.body_rect.x >= 0.0 && cut.body_rect.y >= 0.0);
        assert!(cut.body_rect.x + cut.body_rect.width <= 160.0);
        assert!(banner.take_body_cut().is_none(), "taken once");
    }

    /// 입력 없는 다시 그리기도 같은 채널로 잘림을 남긴다.
    #[test]
    fn a_repaint_also_reports_the_cut_plugin_body() {
        let mut banner = EguiMeshBanner::new(3);
        banner.run_frame(&cut_body_params(), cut_body_ui);
        assert!(banner.take_body_cut().is_some());
        banner.repaint_last_frame(cut_body_ui);
        assert_eq!(
            banner.take_body_cut().map(|c| c.text),
            Some(CUT_BODY.to_owned())
        );
    }

    /// 잘리지 않은 본문은 아무것도 보고하지 않는다.
    #[test]
    fn an_uncut_plugin_body_reports_nothing() {
        let mut banner = EguiMeshBanner::new(3);
        let mut params = cut_body_params();
        params.width_px = 2000;
        banner.run_frame(&params, cut_body_ui);
        assert!(banner.take_body_cut().is_none());
    }

    /// paint에서 타이머를 거쳐 BannerInvalidated가 소켓으로 나가는지 확인한다.
    /// 출력을 먼저 안정시켜 mesh 송신은 생략하고 알림 예약만 수행하게 한다.
    /// 이 시험은 플러그인의 송신까지 확인하며 호스트의 실제 렌더링은 실행하지 않는다.
    #[cfg(any(unix, windows))]
    #[test]
    fn banner_paint_notifies_the_host_of_a_self_repaint_request() {
        use std::io::{BufRead, BufReader};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind localhost");
        let port = listener.local_addr().expect("local addr").port();
        let accept = std::thread::spawn(move || listener.accept().expect("accept").0);
        let plugin_side = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let host_side = accept.join().expect("accept thread");
        let mut host = HostHandle::new(
            Arc::new(Mutex::new(plugin_side)),
            Arc::new(Mutex::new(std::collections::HashMap::new())),
        );
        // 아래 단정이 틀렸을 때(=출력이 dedup 되지 않아 commit 으로 갈 때) 60 초를
        // 기다리지 않고 그 자리에서 실패하게 한다.
        host.timeout = Duration::from_millis(200);

        let mut banner = EguiMeshBanner::new(7);
        let params = BannerSetContextParams {
            instance_id: 7,
            width_px: 320,
            height_px: 64,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        let ui = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("banner content");
            });
        };
        // 출력이 안 바뀔 때까지(=정적 화면) 먼저 그린다 — egui 는 첫 frame 에 레이아웃
        // 정보가 없어 한 번으로는 안 잠긴다. host 는 여기서 안 쓴다(`run_frame` 은
        // commit 을 안 한다).
        let mut settled = false;
        for _ in 0..8 {
            if banner.run_frame(&params, ui).is_none() {
                settled = true;
                break;
            }
        }
        assert!(settled, "시험 전에 출력이 안정되어야 한다");
        let sent = banner.paint(&host, &params, |ctx| {
            ctx.request_repaint_after(Duration::from_millis(5));
            ui(ctx);
        });
        assert!(
            matches!(sent, Ok(None)),
            "같은 출력은 mesh 송신을 생략해야 한다: {sent:?}"
        );

        host_side
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        let mut line = String::new();
        BufReader::new(host_side)
            .read_line(&mut line)
            .expect("호스트가 렌더 요청 알림 한 줄을 받아야 한다");
        let payload: serde_json::Value = serde_json::from_str(&line).expect("알림이 json 이다");
        let event: PluginEvent =
            serde_json::from_value(payload["event"].clone()).expect("event 필드가 PluginEvent 다");
        assert!(
            matches!(event, PluginEvent::BannerInvalidated { instance_id: 7 }),
            "BannerInvalidated 여야 한다 — 실제 {event:?}"
        );
    }

    /// Copy 입력이 egui::Event::Copy로 변환되는지 확인한다.
    #[test]
    fn copy_wire_event_maps_to_egui_copy() {
        assert_eq!(map_event(&RawInputEventWire::Copy), Some(Event::Copy));
    }

    /// 모르는 입력 이벤트는 egui 이벤트로 바꾸지 않고 버린다.
    #[test]
    fn unknown_wire_event_is_dropped() {
        assert_eq!(map_event(&RawInputEventWire::Unknown), None);
    }

    /// Paste 입력이 내용 없는 egui::Event::Paste로 변환되고 와이어 왕복이 유지되는지 확인한다.
    #[test]
    fn paste_wire_event_maps_to_an_empty_egui_paste() {
        assert_eq!(
            map_event(&RawInputEventWire::Paste),
            Some(Event::Paste(String::new()))
        );
        let wire = serde_json::to_value(RawInputEventWire::Paste).expect("직렬화");
        let back: RawInputEventWire = serde_json::from_value(wire).expect("역직렬화");
        assert_eq!(back, RawInputEventWire::Paste);
    }

    /// 엔드투엔드 회귀: 텍스트가 선택된 상태에서 `Copy` wire 이벤트를 보내면, egui 의
    /// 내장 `TextEdit` 선택-복사 로직이 `platform_output.commands` 에 `CopyText` 를
    /// 채우고 `render()` 가 그 값을 `take_copied_text()` 로 노출해야 한다. 선택이
    /// 없으면(다음 frame) 값이 남아있지 않아야 한다(1회 소비 + 자연 소거).
    #[test]
    fn copy_event_exposes_selected_text_edit_range() {
        let mut surface = EguiMeshSurface::new(1);
        let id = egui::Id::new("copy_test");
        let mut buf = "hello world".to_string();

        // frame 1: TextEdit 에 포커스를 주고, 전체 텍스트를 선택한 상태를 저장한다
        // (실제 마우스 드래그 대신 `TextEditState` 를 직접 seed — egui 공식 문서의
        // "새 selection 만들기" 레시피와 동일한 방식).
        surface.run_frame(&ctx_params(400, 100, 1.0), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                let mut output = egui::TextEdit::singleline(&mut buf).id(id).show(ui);
                let range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(buf.chars().count()),
                );
                output.state.cursor.set_char_range(Some(range));
                output.state.store(ui.ctx(), output.response.id);
            });
        });
        assert_eq!(
            surface.take_copied_text(),
            None,
            "selecting text alone must not copy anything"
        );

        // frame 2: 선택은 유지된 채(입력 없는 재-run 이 아니라 같은 UI 를 다시 그려
        // 포커스+선택을 재현) Copy wire 이벤트만 보낸다.
        let mut copy_params = ctx_params(400, 100, 1.0);
        copy_params.raw_input.events = vec![RawInputEventWire::Copy];
        surface.run_frame(&copy_params, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                ui.add(egui::TextEdit::singleline(&mut buf).id(id));
            });
        });

        assert_eq!(surface.take_copied_text().as_deref(), Some("hello world"));
        // 1회 소비 — 바로 다시 물으면 비어 있다.
        assert_eq!(surface.take_copied_text(), None);
    }

    /// 포커스된 singleline `TextEdit` 의 `[start, end)` 문자 범위를 선택한 상태로 한 프레임을 그린다.
    fn seed_text_edit_selection(
        surface: &mut EguiMeshSurface,
        id: egui::Id,
        buf: &mut String,
        start: usize,
        end: usize,
    ) {
        surface.run_frame(&ctx_params(400, 100, 1.0), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                let mut output = egui::TextEdit::singleline(buf).id(id).show(ui);
                let range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(start),
                    egui::text::CCursor::new(end),
                );
                output.state.cursor.set_char_range(Some(range));
                output.state.store(ui.ctx(), output.response.id);
            });
        });
    }

    /// 같은 포커스된 `TextEdit` 을 다시 그리며 와이어 입력 이벤트를 넣는다.
    fn feed_text_edit(
        surface: &mut EguiMeshSurface,
        id: egui::Id,
        buf: &mut String,
        events: Vec<RawInputEventWire>,
    ) {
        let mut params = ctx_params(400, 100, 1.0);
        params.raw_input.events = events;
        surface.run_frame(&params, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                ui.add(egui::TextEdit::singleline(buf).id(id));
            });
        });
    }

    /// PasteText 와이어 이벤트가 텍스트를 담은 egui::Event::Paste 로, Cut 이 egui::Event::Cut 으로 바뀐다.
    #[test]
    fn paste_text_and_cut_wire_events_map_to_egui_events() {
        assert_eq!(
            map_event(&RawInputEventWire::PasteText {
                text: "/tmp/a.md".into()
            }),
            Some(Event::Paste("/tmp/a.md".into()))
        );
        assert_eq!(map_event(&RawInputEventWire::Cut), Some(Event::Cut));
    }

    /// 포커스된 `TextEdit` 에 PasteText 가 오면 그 텍스트가 값에 들어간다.
    #[test]
    fn paste_text_event_inserts_into_the_focused_text_edit() {
        let mut surface = EguiMeshSurface::new(1);
        let id = egui::Id::new("paste_text_test");
        let mut buf = String::new();
        seed_text_edit_selection(&mut surface, id, &mut buf, 0, 0);
        feed_text_edit(
            &mut surface,
            id,
            &mut buf,
            vec![RawInputEventWire::PasteText {
                text: "/tmp/a.md".into(),
            }],
        );
        assert_eq!(buf, "/tmp/a.md");
    }

    /// 선택이 있을 때 Cut 이 오면 선택이 지워지고 그 문자열이 복사 결과가 된다.
    #[test]
    fn cut_event_removes_the_selection_and_copies_it() {
        let mut surface = EguiMeshSurface::new(1);
        let id = egui::Id::new("cut_test");
        let mut buf = "hello world".to_string();
        seed_text_edit_selection(&mut surface, id, &mut buf, 0, 6);
        feed_text_edit(&mut surface, id, &mut buf, vec![RawInputEventWire::Cut]);
        assert_eq!(buf, "world");
        assert_eq!(surface.take_copied_text().as_deref(), Some("hello "));
        assert_eq!(surface.take_copied_text(), None);
    }

    /// 루프백 소켓으로 이은 시험용 [`HostHandle`] 과 호스트 쪽 소켓.
    /// 출력이 dedup 되지 않아 commit 으로 가면 공유 버퍼 요청이 200ms 안에 실패한다.
    #[cfg(any(unix, windows))]
    fn loopback_host() -> (HostHandle, std::net::TcpStream) {
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind localhost");
        let port = listener.local_addr().expect("local addr").port();
        let accept = std::thread::spawn(move || listener.accept().expect("accept").0);
        let plugin_side = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        let host_side = accept.join().expect("accept thread");
        let mut host = HostHandle::new(
            Arc::new(Mutex::new(plugin_side)),
            Arc::new(Mutex::new(std::collections::HashMap::new())),
        );
        host.timeout = Duration::from_millis(200);
        host_side
            .set_read_timeout(Some(Duration::from_millis(300)))
            .expect("read timeout");
        (host, host_side)
    }

    /// 호스트 쪽 소켓에서 읽기 시간이 끝날 때까지 받은 알림을 모두 읽는다.
    /// `event` 가 아닌 줄(공유 버퍼 요청 등)은 건너뛴다.
    #[cfg(any(unix, windows))]
    fn read_events(host_side: &std::net::TcpStream) -> Vec<PluginEvent> {
        use std::io::{BufRead, BufReader};
        let mut reader = BufReader::new(host_side.try_clone().expect("clone"));
        let mut events = Vec::new();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let payload: serde_json::Value =
                        serde_json::from_str(&line).expect("한 줄은 json 이다");
                    if let Ok(event) = serde_json::from_value(payload["event"].clone()) {
                        events.push(event);
                    }
                }
            }
        }
        events
    }

    /// 받은 알림 중 복사 결과만 (대상, 텍스트) 로 모은다.
    #[cfg(any(unix, windows))]
    fn copied(events: &[PluginEvent]) -> Vec<(CopyTargetWire, String)> {
        events
            .iter()
            .filter_map(|e| match e {
                PluginEvent::CopyText { target, text } => Some((*target, text.clone())),
                _ => None,
            })
            .collect()
    }

    /// 전체가 선택된 포커스 `TextEdit` 을 그리는 UI. 매 프레임 같은 선택을 다시 넣어 출력이 안정된다.
    fn selected_text_edit_ui(id: egui::Id, buf: &mut String) -> impl FnMut(&Context) + '_ {
        move |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                let mut output = egui::TextEdit::singleline(&mut *buf).id(id).show(ui);
                let range = egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(5),
                );
                output.state.cursor.set_char_range(Some(range));
                output.state.store(ui.ctx(), output.response.id);
            });
        }
    }

    /// 화면 출력이 같아 PaintFrame 이 생략되는 Copy 도 surface 복사 결과 알림은 한 번 나가고,
    /// 알린 값은 소비되어 take_copied_text 와 다음 paint 에서 다시 나오지 않는다.
    #[cfg(any(unix, windows))]
    #[test]
    fn surface_paint_reports_a_copy_even_when_the_frame_is_skipped() {
        let (host, host_side) = loopback_host();
        let mut surface = EguiMeshSurface::new(4);
        let id = egui::Id::new("surface_copy_notify");
        let mut buf = "hello world".to_string();
        let mut params = ctx_params(400, 100, 1.0);
        params.surface_id = 4;
        let mut settled = false;
        for _ in 0..8 {
            if surface
                .run_frame(&params, selected_text_edit_ui(id, &mut buf))
                .is_none()
            {
                settled = true;
                break;
            }
        }
        assert!(settled, "시험 전에 출력이 안정되어야 한다");

        let mut copy_params = params.clone();
        copy_params.raw_input.events = vec![RawInputEventWire::Copy];
        let sent = surface.paint(&host, &copy_params, selected_text_edit_ui(id, &mut buf));
        assert!(
            matches!(sent, Ok(None)),
            "같은 출력은 프레임을 생략한다: {sent:?}"
        );
        assert_eq!(surface.take_copied_text(), None, "알린 값은 소비된다");
        let sent = surface.paint(&host, &params, selected_text_edit_ui(id, &mut buf));
        assert!(matches!(sent, Ok(None)), "{sent:?}");

        assert_eq!(
            copied(&read_events(&host_side)),
            vec![(
                CopyTargetWire::Surface { surface_id: 4 },
                "hello".to_string()
            )]
        );
    }

    /// popup 의 Cut 은 선택을 지우고 popup 대상 복사 결과를 한 번 알린다.
    #[cfg(any(unix, windows))]
    #[test]
    fn popup_paint_reports_a_cut_once() {
        let (host, host_side) = loopback_host();
        let mut popup = EguiMeshPopup::new(9);
        let id = egui::Id::new("popup_cut_notify");
        let mut buf = "hello world".to_string();
        let params = PopupSetContextParams {
            instance_id: 9,
            width_px: 400,
            height_px: 100,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        popup.run_frame(&params, selected_text_edit_ui(id, &mut buf));
        let mut cut_params = params.clone();
        cut_params.raw_input.events = vec![RawInputEventWire::Cut];
        // Cut 은 화면을 바꾸므로 프레임 commit 을 시도한다. 시험 대역은 공유 버퍼 요청에
        // 답하지 않아 commit 이 실패하므로, 복사 결과 알림은 그 전에 나가야 한다.
        let sent = popup.paint(&host, &cut_params, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.memory_mut(|m| m.request_focus(id));
                ui.add(egui::TextEdit::singleline(&mut buf).id(id));
            });
        });
        assert!(
            !matches!(sent, Ok(None)),
            "Cut 은 화면을 바꾸므로 프레임을 생략하지 않는다: {sent:?}"
        );
        assert_eq!(buf, " world");
        assert_eq!(popup.take_copied_text(), None, "알린 값은 소비된다");

        assert_eq!(
            copied(&read_events(&host_side)),
            vec![(
                CopyTargetWire::Popup { instance_id: 9 },
                "hello".to_string()
            )]
        );
    }

    /// 입력 없는 다시 그리기에서 UI 코드가 복사한 텍스트도 surface·popup 모두 알린다.
    #[cfg(any(unix, windows))]
    #[test]
    fn repaint_last_reports_text_copied_by_ui_code() {
        let (host, host_side) = loopback_host();
        let label = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("static");
            });
        };
        let copy_and_label = |ctx: &Context| {
            ctx.copy_text("from ui".into());
            label(ctx);
        };
        let mut surface = EguiMeshSurface::new(2);
        let mut popup = EguiMeshPopup::new(3);
        let popup_params = PopupSetContextParams {
            instance_id: 3,
            width_px: 200,
            height_px: 50,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        for _ in 0..8 {
            surface.run_frame(&ctx_params(200, 50, 1.0), label);
            popup.run_frame(&popup_params, label);
        }
        let s = surface.repaint_last(&host, copy_and_label);
        let p = popup.repaint_last(&host, copy_and_label);
        assert!(
            matches!(s, Ok(None)) && matches!(p, Ok(None)),
            "{s:?} {p:?}"
        );
        assert_eq!(
            copied(&read_events(&host_side)),
            vec![
                (
                    CopyTargetWire::Surface { surface_id: 2 },
                    "from ui".to_string()
                ),
                (
                    CopyTargetWire::Popup { instance_id: 3 },
                    "from ui".to_string()
                ),
            ]
        );
    }

    /// 한 패스에 여러 번 복사하면 호스트 선택 복사와 같게 마지막 값을 쓴다.
    #[test]
    fn several_copies_in_one_pass_keep_the_last() {
        let mut surface = EguiMeshSurface::new(1);
        surface.run_frame(&ctx_params(200, 50, 1.0), |ctx| {
            ctx.copy_text("first".into());
            ctx.copy_text("last".into());
        });
        assert_eq!(surface.take_copied_text().as_deref(), Some("last"));
    }

    /// 그림이 같아도 IME 위치가 Some↔None 으로 바뀐 패스는 프레임을 보낸다. 호스트는
    /// PaintFrame 의 `ime_cursor` 로 입력칸 포커스를 판정하기 때문이다. IME 도 같으면 생략한다.
    #[test]
    fn an_ime_change_alone_is_not_skipped() {
        let mut surface = EguiMeshSurface::new(1);
        let params = ctx_params(200, 50, 1.0);
        let label = |ctx: &Context| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("static");
            });
        };
        let with_ime = |ctx: &Context| {
            label(ctx);
            ctx.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    rect: Rect::from_min_size(Pos2::new(1.0, 2.0), vec2(30.0, 10.0)),
                    cursor_rect: Rect::from_min_size(Pos2::new(3.0, 2.0), vec2(1.0, 10.0)),
                });
            });
        };
        let mut settled = false;
        for _ in 0..8 {
            if surface.run_frame(&params, label).is_none() {
                settled = true;
                break;
            }
        }
        assert!(settled, "시험 전에 출력이 안정되어야 한다");
        assert!(
            surface.run_frame(&params, with_ime).is_some(),
            "None→Some IME 변화는 프레임을 보낸다"
        );
        assert!(
            surface.run_frame(&params, with_ime).is_none(),
            "같은 IME 는 생략한다"
        );
        assert!(
            surface.run_frame(&params, label).is_some(),
            "Some→None IME 변화는 프레임을 보낸다"
        );
        assert!(surface.run_frame(&params, label).is_none());
    }

    /// 마지막 CopyText 가 빈 문자열이면 앞의 값 대신 빈 문자열을 쓴다(호스트와 같은 규칙).
    #[test]
    fn an_empty_last_copy_wins_over_an_earlier_one() {
        let mut surface = EguiMeshSurface::new(1);
        surface.run_frame(&ctx_params(200, 50, 1.0), |ctx| {
            ctx.copy_text("first".into());
            ctx.copy_text(String::new());
        });
        assert_eq!(surface.take_copied_text().as_deref(), Some(""));
    }

    /// 화면 출력이 같아 PopupPaintFrame 이 생략되는 Copy 도 popup 복사 결과 알림은 한 번 나가고,
    /// 다음 무복사 paint 에서는 추가 알림이 없다.
    #[cfg(any(unix, windows))]
    #[test]
    fn popup_paint_reports_a_copy_even_when_the_frame_is_skipped() {
        let (host, host_side) = loopback_host();
        let mut popup = EguiMeshPopup::new(6);
        let id = egui::Id::new("popup_copy_notify");
        let mut buf = "hello world".to_string();
        let params = PopupSetContextParams {
            instance_id: 6,
            width_px: 400,
            height_px: 100,
            pixels_per_point: 1.0,
            raw_input: RawInputWire::default(),
            theme: None,
            need_full_textures: false,
        };
        let mut settled = false;
        for _ in 0..8 {
            if popup
                .run_frame(&params, selected_text_edit_ui(id, &mut buf))
                .is_none()
            {
                settled = true;
                break;
            }
        }
        assert!(settled, "시험 전에 출력이 안정되어야 한다");

        let mut copy_params = params.clone();
        copy_params.raw_input.events = vec![RawInputEventWire::Copy];
        let sent = popup.paint(&host, &copy_params, selected_text_edit_ui(id, &mut buf));
        assert!(
            matches!(sent, Ok(None)),
            "같은 출력은 프레임을 생략한다: {sent:?}"
        );
        let sent = popup.paint(&host, &params, selected_text_edit_ui(id, &mut buf));
        assert!(matches!(sent, Ok(None)), "{sent:?}");

        assert_eq!(
            copied(&read_events(&host_side)),
            vec![(
                CopyTargetWire::Popup { instance_id: 6 },
                "hello".to_string()
            )]
        );
    }
}
