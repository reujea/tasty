use winit::window::Window;

use crate::adapters::ui;
use crate::model::PhysicalRect;
use crate::settings::EffectiveFont;
use crate::state::AppState;

use super::GpuState;

/// Build a single-line signature capturing every `EffectiveFont` field that
/// the GPU cell renderer cares about, plus the scale-resolved size. Used to
/// detect when `update_font` must be re-run.
///
/// Comparing the raw user-facing settings (not the resolved family name) is
/// intentional — `cosmic_text` normalizes `""` / `"monospace"` into
/// `FamilyOwned::Name("D2Coding ligature")`, so reading back the post-resolve
/// family from the renderer and comparing it against the raw settings would
/// always mismatch, causing a wasted atlas reset every frame.
pub(super) fn term_font_signature(font: &EffectiveFont, effective_size: f32) -> String {
    format!(
        "{}|{}|{}|{}",
        effective_size, font.font_family, font.custom_font_path, font.line_height
    )
}

/// Foreground 티어 chrome 레이어의 상대 z-order를 매 프레임 강제한다.
///
/// egui 의 `Order` 는 5단 고정 tier 이고, 같은 tier 안에서는
/// `Memory::Areas::order`(Vec, 안정 정렬)가 실제 그리기 순서를 정한다.
/// `Context::move_to_top` 은 "이번 프레임엔 맨 위 그룹" 플래그를 세우거나 신규
/// LayerId 를 등록할 뿐이라, **이미 등록된** 레이어 여러 개에 매 프레임
/// 반복 호출해도 서로 tie 로 묶여 안정 정렬을 타므로 상대 위치가 바뀌지
/// 않는다 — 결과적으로 상대 순서는 각 레이어가 세션 중 *최초로* 등록된
/// 시점(자연 호출 순서)에 영구히 고정된다. 배너가 tab_bar/status_bar 보다
/// 나중에(`ui::draw_popups` 내부에서) 처음 등록되기 때문에, 단순히 4개
/// 레이어에 순서대로 `move_to_top` 을 호출하는 것만으로는 배너를 tab_bar/
/// status_bar 아래로 내릴 수 없다.
///
/// 대신 `Context::set_sublayer(parent, child)` — `end_pass` 가 안정 정렬 *이후*
/// child 를 parent 위치 바로 뒤(=parent 바로 위)로 splice 하는, 등록 시점과
/// 무관한 강제 인접 배치 — 를 조합해 계층을 만든다. 단, 이 API 는 1단
/// 들여쓰기만 지원한다(parent 가 다른 sublayer 의 child 이면 동작 unspecified,
/// egui 소스 주석) — 그래서 아래 두 그룹은 서로 겹치지 않게 나눈다:
///
/// - `banner_layer` 를 부모로, `status_bar`/각 pane 의 tab_bar 를 자식으로 묶어
///   Banner < {status_bar, tab_bar} 를 등록 시점과 무관하게 고정한다.
/// - `modifier_hint` 가 떠 있으면 그것을 부모로, 열려 있는 모든 popup 레이어를
///   자식으로 묶어 Modifier-hint < Popup 을 고정한다.
///
/// 두 그룹의 부모끼리(`banner_layer` ↔ `modifier_hint`)는 서로를 sublayer로
/// 엮지 않는다(엮으면 2단 들여쓰기가 되어 위 unspecified 제약에 걸린다). 대신
/// 자연 등록 순서에 안전하게 의존한다: `banner_layer` 는 배너가 0개여도 매
/// 프레임 무조건 그려지므로 앱 시작 첫 프레임에 반드시 등록되는 반면,
/// `modifier_hint` 는 사용자가 modifier 를 처음 누르는 프레임에야 처음
/// 등록된다 — 항상 banner 보다 늦게 등록되므로 안정 정렬에서 영구히 더 위(늦은
/// 위치)에 남는다.
///
/// 결과: `docs/architecture/input-layer.md` 정책의
/// Banner(5) < egui위젯(4) < Modifier-hint(2b) < Popup(2) 관계가 그대로
/// 재현된다. Modal(1) 은 별도 OS 창이라 범위 밖, Divider/Terminal(6/7) 은 다른
/// `Order` tier 라 범위 밖이다.
///
/// `AppState` 전체가 아니라 관련된 3 개 레이어 값만 받는다 — 헤드리스
/// `egui::Context` + 단정된 레이어 3~4 개만으로 [`tests`] 가 `AppState`/
/// `CoreState` 구성 없이 정책을 직접 assert 할 수 있게 하기 위한 의도적 축소다.
fn enforce_foreground_z_order(
    ctx: &egui::Context,
    banner_layer: Option<egui::LayerId>,
    modifier_hint_layer: Option<egui::LayerId>,
    popup_layers: &[egui::LayerId],
    pane_rects: &[(u32, PhysicalRect)],
) {
    if let Some(banner_layer) = banner_layer {
        // Area Id 는 본체 소유 — `adapters::ui::status_bar` 의 상수를 통해서만 읽는다
        // (문자열을 여기 하드코딩하면 한쪽만 바뀌어도 컴파일은 통과하고 z-order 만
        // 조용히 깨진다).
        ctx.set_sublayer(banner_layer, ui::status_bar::status_bar_layer_id());
        for (pane_id, _) in pane_rects {
            let tab_bar_layer = egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new(format!("pane_tabs_{pane_id}")),
            );
            ctx.set_sublayer(banner_layer, tab_bar_layer);
        }
    }

    if let Some(modifier_hint_layer) = modifier_hint_layer {
        for popup_layer in popup_layers {
            ctx.set_sublayer(modifier_hint_layer, *popup_layer);
        }
    }
}

/// host popup(`PopupManager`, `host_popup_layers`)과 plugin egui-mesh popup(`draw_plugin_popups`,
/// `plugin_popup_layers`) 셸 사이의 상대 순서를 `host_popup_on_top`(z_seq 비교 결과,
/// `host_popup_should_render_on_top`)에 따라 강제한다.
///
/// 두 popup 종류 모두 `ctx.layer_painter(layer_id)` 로 직접 그리는 **raw layer** 다(위
/// `enforce_foreground_z_order` 의 `egui::Area` 기반 layer 와 다름). raw layer 는
/// `egui::Area`/`Window` 를 거치지 않으므로 `Memory::Areas::order`(egui 가 "최근에
/// 상호작용한 Area" 를 추적하는 리스트)에 자동으로 편입되지 않는다 — `end_pass` 의
/// `GraphicLayers::drain` 은 `order` 에 없는 레이어를 별도의 내부 맵 순회로 덧붙이는데
/// (egui 소스: "Also draw areas that are missing in `area_order`"), 그 순회 순서는 이
/// 프레임에서 어떤 함수를 먼저 호출했는지와 무관하다 — 즉 **draw 호출 순서로는 이 둘의
/// 상대 순서를 제어할 수 없다.** `ctx.set_sublayer(parent, child)` 는 호출 즉시 두
/// layer 를 `order` 에 강제 등록하고 `end_pass` 에서 child 를 parent 바로 위로 splice 하므로,
/// 이 관계를 결정론적으로 강제하는 유일한 방법이다.
///
/// host popup 이 여러 개, plugin popup 이 여러 개 있으면 모든 조합에 대해 `set_sublayer`
/// 를 건다(star 패턴: 한쪽 그룹 전체가 다른 쪽 그룹 전체 위/아래) — 두 그룹이 서로
/// 엇갈리는 세밀한 개별 순서까지는 재현하지 못한다(egui 가 1단 중첩만 지원해 체인 불가,
/// `host_popup_should_render_on_top` 문서의 "최소 설계" 항목과 동일한 제약).
/// `modifier_hint_layer` 가 동시에 켜져 있으면 `host_popup_layers` 원소가
/// `enforce_foreground_z_order` 에서도 자식으로 쓰이므로(한 layer 가 두 부모의 자식이
/// 되는 셈) 그 조합의 최종 순서는 `sublayers` HashMap 순회 순서에 좌우되는 비결정적
/// 예외로 남는다 — 실사용에서 드문 조합이라 허용한다.
fn enforce_host_plugin_popup_z_order(
    ctx: &egui::Context,
    host_popup_layers: &[egui::LayerId],
    plugin_popup_layers: &[egui::LayerId],
    host_popup_on_top: bool,
) {
    if host_popup_on_top {
        for plugin_layer in plugin_popup_layers {
            for host_layer in host_popup_layers {
                ctx.set_sublayer(*plugin_layer, *host_layer);
            }
        }
    } else {
        for host_layer in host_popup_layers {
            for plugin_layer in plugin_popup_layers {
                ctx.set_sublayer(*host_layer, *plugin_layer);
            }
        }
    }
}

/// host popup(`PopupManager`)과 plugin popup(egui-mesh) 사이의 z-order 를 판정하는 유일한
/// 기준: 열려 있는 각 쪽의 최대 z_seq(`tasty_host_plugin::next_popup_z_seq` 공유 카운터)를
/// 비교해 host popup 이 위에 그려져야 하는지 결정한다(규칙 7 "나중에 열리거나 클릭된 것이
/// 앞", `docs/design/systems/popup.md`). 한쪽이 아예 없으면(`None`) 애초에 겹칠 popup 쌍이
/// 없으므로 항상 `false`(=현재/기본 순서인 "plugin 이 host 위" 유지) — 이 경우 결과가 어느
/// 값이든 시각적으로 무관하지만, `false` 고정이 기존 동작(host popup 만 있거나 plugin popup
/// 만 있는 절대다수 프레임)과 완전히 동일한 코드 경로를 타게 해 회귀 위험을 없앤다.
///
/// 셸 등록 순서(`run_egui_frame` 아래)와 GPU 콘텐츠 합성 순서(`gfx/gpu.rs` 의
/// `render_egui_pass`/`render_egui_mesh_popups`) 양쪽에서 같은 값을 재사용해야 한 프레임
/// 안에서 셸과 콘텐츠가 서로 다른 결정을 내리지 않는다.
///
/// host popup 이 여러 개, plugin popup 이 여러 개 동시에 열려 있는 경우 이 함수는 "그룹
/// 전체" 단위로만 판정한다(각 그룹의 최댓값끼리 비교) — host popup A(z_seq=1) < plugin
/// popup(z_seq=2) < host popup B(z_seq=3) 처럼 두 그룹이 서로 엇갈리는 경우까지 완전히
/// 재현하지는 못한다(egui 의 `set_sublayer` 가 1단 중첩만 지원해 N-way interleave 가
/// 불가능 — egui 소스 주석 "This currently only supports one level of nesting"). 이는
/// 의도된 최소 설계다: 실사용 사례가 host popup 1개 vs plugin popup 1개(예: markdown
/// "Open Markdown File" → Browse → `file_picker`)인 한 정확하고, 3개 이상 동시 존재하는
/// 드문 케이스는 "그룹 전체가 한 덩어리로 위/아래" 근사로 성능 저하 없이 저하(graceful
/// degradation)한다.
pub(super) fn host_popup_should_render_on_top(
    host_top_z_seq: Option<u64>,
    plugin_top_z_seq: Option<u64>,
) -> bool {
    match (host_top_z_seq, plugin_top_z_seq) {
        (Some(host), Some(plugin)) => host > plugin,
        _ => false,
    }
}

impl GpuState {
    #[allow(clippy::too_many_arguments)] // reason: frame context 전체 전달
    pub(super) fn run_egui_frame(
        &mut self,
        state: &mut AppState,
        engine: &mut crate::core::CoreState,
        window: &Window,
        pane_rects: &[(u32, PhysicalRect)],
        dividers: &[PhysicalRect],
        terminal_rect: PhysicalRect,
        plugin_manager: Option<&crate::plugin::PluginManager>,
        host_popup_on_top: bool,
    ) -> egui::FullOutput {
        let raw_input = self.egui_state.take_egui_input(window);
        // 설정이 바뀌면 다음 프레임부터 따라오게 한다 — 생성 시점 값만 쓰면 설정 모달에서
        // 슬라이더를 옮겨도 이미 열려 있는 창은 옛 거리로 스크롤한다(ADR-0130).
        self.egui_ctx.options_mut(|o| {
            o.line_scroll_speed = engine.settings.general.wheel_line_scroll;
        });
        let scale_factor = self.scale_factor;
        let proxy = &self.proxy;

        self.egui_ctx.run(raw_input, |ctx| {
            // CSD 공통 titlebar — TopBottomPanel::top 이 먼저 등록되어야 사이드바
            // SidePanel 이 그 아래에서 시작한다. 드래그/더블클릭을 winit window 로 브리지.
            ui::titlebar::draw_titlebar(ctx, state, window, proxy);
            let plugin_alert = plugin_manager.map_or(0, |m| m.attention_count());
            ui::draw_ui(ctx, state, engine, scale_factor, plugin_alert);
            ui::draw_pane_dividers(ctx, dividers, scale_factor);
            ui::draw_surface_highlights(ctx, state, engine, terminal_rect, scale_factor);
            ui::draw_pane_tab_bars(ctx, state, engine, pane_rects, scale_factor);
            ui::draw_egui_panels(ctx, state, engine, pane_rects, scale_factor);
            ui::draw_status_bar(ctx, state, engine, terminal_rect, scale_factor);
            // Context menus are now handled via native OS menus (see process_pending_native_menu)
            //
            // ★ **이 두 호출의 순서가 계약이다.** host popup 이 plugin popup 보다 **먼저**
            // 그려져야 한다. 페인트 때문이 아니라(아래 z-order 주석 참고 — 그쪽은 이 순서에
            // 안 기댄다) **한 프레임 안에서 누가 누구의 rect 를 읽는가** 때문이다:
            //
            // - `draw_popups` 가 이번 프레임의 host popup 히트테스트 rect 를 적재하고,
            //   `draw_plugin_popups` 가 **같은 프레임에** 그것을 읽는다. 순서가 뒤집히면
            //   plugin 쪽 판정이 **지난 프레임** rect 를 읽는다
            //   (`adapters/ui/popup/draw.rs` 의 `hit_rects` 주석).
            // - 반대 방향은 이미 1 프레임 stale 인 것이 **정상**이다 —
            //   `plugin_popup_hittest` 는 plugin draw 가 뒤라서 host 가 다음 프레임에
            //   읽는다(`state.rs` 의 그 필드 선언). 순서를 뒤집으면 그 "1 프레임" 이
            //   조용히 0 이 되고, 두 판정이 같은 프레임 안에서 서로를 물게 된다.
            //
            // 어기면 화면은 멀쩡하다. 어긋난 rect 로 히트테스트가 한 프레임 틀릴 뿐이라
            // 컴파일도 되고 시험도 통과한다 — 그래서 `source_guards::frame_draw_order` 가
            // 이 두 줄의 순서를 값으로 문다.
            crate::plugin_bridge::popup_scope::inherit_file_picker_scope(
                state,
                plugin_manager
                    .into_iter()
                    .flat_map(|mgr| mgr.popup_instances()),
            );
            let popup_layout =
                ui::draw_popups(ctx, state, engine, pane_rects, terminal_rect, scale_factor);
            // Plugin popup 인스턴스(동적 instance_id) — host PopupManager와 별도 경로.
            // 소속 범위 판정은 host popup 이 방금 쓴 것과 같은 layout 으로 한다.
            crate::plugin_bridge::popup_render::draw_plugin_popups(
                ctx,
                state,
                engine,
                plugin_manager,
                Some(&popup_layout),
            );
            enforce_foreground_z_order(
                ctx,
                state.banner_layer,
                state.modifier_hint_layer,
                &state.popup_layers,
                pane_rects,
            );
            // host popup ↔ plugin popup 셸 순서 — 둘 다 `ctx.layer_painter`로 직접 그리는
            // raw layer 라 위 두 draw 호출의 순서는 **최종 페인트 순서에는** 영향이 없다.
            // 그 문장은 여기 z-order 판정에만 걸린다 — 위 두 줄의 순서 자체는 히트테스트
            // rect 의 프레임 계약이라 바꾸면 안 된다(위 ★ 주석). 페인트에 안 걸린다는 것을
            // "순서가 아무래도 좋다" 로 읽으면 그 계약이 조용히 깨진다.(egui
            // `Areas::order`는 `egui::Area` 기반 위젯만 자동 추적 — 문서는
            // `enforce_host_plugin_popup_z_order` 참고). `set_sublayer` 로 명시적으로
            // 관계를 걸어야 한다.
            enforce_host_plugin_popup_z_order(
                ctx,
                &state.popup_layers,
                &state.plugin_popup_layers,
                host_popup_on_top,
            );
            // Plugin egui-mesh banner(A3) — banner manager 가 `draw_ui` 중 셸을 그리고
            // content_rect 슬롯을 기록한 뒤, 여기서 content mesh forward + 합성 영역 적재.
            crate::plugin_bridge::banner_render::draw_plugin_banners(
                ctx,
                state,
                engine,
                plugin_manager,
            );
            // 외부 drag&drop hover 시각 피드백 — 모든 레이어 위에 그린다.
            ui::drop_overlay::draw_drop_overlay(ctx, state, engine, terminal_rect, scale_factor);

            // 통합 리사이즈 커서 — `handle_cursor_moved` 가 창 가장자리 hover 시
            // 저장한 8방향을 egui 프레임 내에서 적용한다. egui 가 매 프레임 winit
            // 커서를 덮으므로 여기서만 적용할 수 있다. macOS 는 네이티브 데코라
            // `pending_resize_cursor` 가 항상 None (write 경로가 cfg 로 제외됨).
            if let Some(dir) = state.pending_resize_cursor {
                ctx.set_cursor_icon(ui::titlebar::resize_cursor(dir));
            }

            // Settings UI is now rendered in the modal window (ModalView)
        })
    }

    pub(super) fn post_egui_update(&mut self, engine: &crate::core::CoreState, _prev_theme: &str) {
        // Theme / ui_zoom 변경 감지는 더 이상 polling 으로 하지 않는다 —
        // settings cascade 와 단축키 (Z-7) 가 `UiIntent::AppearanceChanged` 를
        // 발화하면 `App::cascade_appearance_changed` 가 broadcast 로 갱신한다.
        // 본 함수는 매 프레임 egui style reapply 만 담당 (cheap; visuals/style
        // state 가 egui ctx 안에 있어 다른 set_style 호출에 덮어쓰일 가능성을
        // 대비한 idempotent re-application).
        tasty_egui_theme::apply_theme_to_egui(&crate::theme::theme(), &self.egui_ctx);

        // Surface font refresh is done in render() before run_egui_frame().

        let term_font = engine.settings.appearance.effective_terminal_font();
        let effective_font_size = term_font.effective_font_size(self.scale_factor);
        let new_sig = term_font_signature(&term_font, effective_font_size);
        if new_sig != self.last_term_font_sig {
            self.renderer.update_font(
                &self.device,
                &self.queue,
                effective_font_size,
                &term_font.font_family,
                &term_font.custom_font_path,
                term_font.line_height,
            );
            self.renderer
                .resize(&self.queue, self.size.width, self.size.height);
            self.last_term_font_sig = new_sig;
        }
    }

    /// Apply the theme to the egui context.
    pub(super) fn apply_theme(ctx: &egui::Context, _theme: &str) {
        tasty_egui_theme::apply_theme_to_egui(&crate::theme::theme(), ctx);
    }

    /// Re-apply the current global `Theme` to this window's egui context.
    /// `cascade_appearance_changed` broadcast 의 진입점 — main + modal 모두 같은
    /// 시그니처로 호출한다.
    pub fn refresh_theme(&self) {
        tasty_egui_theme::apply_theme_to_egui(&crate::theme::theme(), &self.egui_ctx);
    }

    // ── Generic egui helpers (for modal windows) ──

    /// Take egui input from a window.
    pub fn take_egui_input(&mut self, window: &Window) -> egui::RawInput {
        self.egui_state.take_egui_input(window)
    }

    /// Run an egui frame with a custom UI closure.
    pub fn run_egui(
        &self,
        raw_input: egui::RawInput,
        ui_fn: impl FnMut(&egui::Context),
    ) -> egui::FullOutput {
        self.egui_ctx.run(raw_input, ui_fn)
    }

    /// Finish an egui frame: tessellate, render, present.
    pub fn finish_egui_frame(&mut self, window: &Window, full_output: egui::FullOutput) {
        // egui-winit disables IME when no egui text field is focused
        // (calls set_ime_allowed(false) when self.allow_ime differs from
        // ime.is_some()). The terminal always needs IME active.
        // Pre-set allow_ime=false so that when egui computes allow_ime=false
        // (no text field), the check false!=false is false and it skips
        // the set_ime_allowed(false) call entirely.
        self.egui_state.set_allow_ime(false);
        self.egui_state
            .handle_platform_output(window, full_output.platform_output);

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        let output = match self.surface.get_current_texture() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("modal surface error: {e}");
                return;
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Clear
        let th = crate::theme::theme();
        let bg = th.bg_panel().to_float();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("modal_clear"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("modal_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(std::iter::once(encoder.finish()));

        // Egui render
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }
        let mut egui_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("modal_egui"),
                });
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut egui_encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        {
            let render_pass = egui_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("modal_egui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            self.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }
        self.queue.submit(std::iter::once(egui_encoder.finish()));

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // `ui.screenshot`(window) 이 모달·preset 창을 대상으로 삼을 수 있으므로 이
        // 경로에도 present 직전 readback 이 있어야 한다 — 없으면 요청이 큐에 남아
        // 영구 대기한다. main 창 경로(`Gpu::render` / `render_fullscreen_stage`)의
        // 같은 지점과 동형이다.
        if let Some(path) = self.pending_screenshot.take() {
            self.capture_frame_to_png(&output.texture, self.size.width, self.size.height, &path);
        }

        output.present();
    }
}

#[cfg(test)]
// 테스트 본문은 `let _ =` 사유 주석 정책의 범위 밖이다(전수 가드가 제외한다) —
// 여기 경고는 조치 대상이 될 수 없어 프로덕션 신호만 가린다. error-handling.md.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;
    use tasty_type_geometry::length::PhysicalPx;

    /// 헤드리스 `egui::Context` 에 지정한 rect 로 interactable Foreground Area 를
    /// 하나 등록한다. `ctx.run(...)` 클로저 안에서 호출해야 한다.
    fn register_area(ctx: &egui::Context, name: &str, rect: egui::Rect) {
        egui::Area::new(egui::Id::new(name))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .interactable(true)
            .sense(egui::Sense::hover())
            .show(ctx, |ui| {
                ui.allocate_exact_size(rect.size(), egui::Sense::hover());
            });
    }

    fn layer_of(name: &str) -> egui::LayerId {
        egui::LayerId::new(egui::Order::Foreground, egui::Id::new(name))
    }

    fn dummy_pane_rect(pane_id: u32) -> (u32, PhysicalRect) {
        (
            pane_id,
            PhysicalRect {
                x: PhysicalPx(0.0),
                y: PhysicalPx(0.0),
                width: PhysicalPx(10.0),
                height: PhysicalPx(10.0),
            },
        )
    }

    /// egui `Areas::order` 의 안정 정렬은 "최초 등록 시점" 순서를 tie-break 로 쓴다
    /// (본 파일 `enforce_foreground_z_order` 의 doc 참고) — 그래서 실제 앱의 자연
    /// 호출 순서(status_bar/tab_bar 가 banner 보다 먼저 등록됨, `run_egui_frame`)를
    /// 그대로 재현해야 이 테스트가 "고치기 전엔 실패했을" 시나리오를 검증한다.
    /// 순서를 바꿔 등록하면(banner 를 먼저 등록) 우연히 통과해 회귀를 못 잡는다.
    #[test]
    fn banner_is_pinned_below_status_bar_and_tab_bar() {
        let ctx = egui::Context::default();
        let status_point = egui::pos2(5.0, 5.0);
        let tab_point = egui::pos2(50.0, 5.0);
        let status_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));
        let tab_rect = egui::Rect::from_min_size(egui::pos2(45.0, 0.0), egui::vec2(10.0, 10.0));
        // banner 는 화면 전체를 덮는 하나의 zone 이므로 두 지점 모두와 겹친다.
        let banner_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 10.0));

        // FullOutput 불필요 — 이 테스트는 order/z-stack 결과만 확인한다.
        let _ = ctx.run(Default::default(), |ctx| {
            // 실제 draw 순서(run_egui_frame): tab_bar/status_bar 먼저, banner 는
            // draw_popups 내부에서 나중에 등록된다.
            register_area(ctx, "workspace_status_bar", status_rect);
            register_area(ctx, "pane_tabs_1", tab_rect);
            register_area(ctx, "banner_layer", banner_rect);

            enforce_foreground_z_order(
                ctx,
                Some(layer_of("banner_layer")),
                None,
                &[],
                &[dummy_pane_rect(1)],
            );
        });

        assert_eq!(
            ctx.layer_id_at(status_point),
            Some(layer_of("workspace_status_bar")),
            "status_bar 가 banner 위에 그려져야 한다"
        );
        assert_eq!(
            ctx.layer_id_at(tab_point),
            Some(layer_of("pane_tabs_1")),
            "tab_bar 가 banner 위에 그려져야 한다"
        );
    }

    /// `enforce_foreground_z_order` 를 호출하지 않으면(수정 전 상태와 동일) banner 가
    /// 자연 등록 순서상 나중이라 위에 그려진다 — 위 테스트가 실제로 이 문제를 잡아내는
    /// 테스트임을 보여주는 characterization test.
    #[test]
    fn without_enforcement_banner_naturally_ends_up_on_top() {
        let ctx = egui::Context::default();
        let point = egui::pos2(5.0, 5.0);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));

        // FullOutput 불필요 — 이 테스트는 order/z-stack 결과만 확인한다.
        let _ = ctx.run(Default::default(), |ctx| {
            register_area(ctx, "workspace_status_bar", rect);
            register_area(ctx, "banner_layer", rect);
            // enforce_foreground_z_order 호출 없음.
        });

        assert_eq!(
            ctx.layer_id_at(point),
            Some(layer_of("banner_layer")),
            "강제 없이는 나중에 등록된 banner 가 위에 그려진다(수정 전 버그 재현)"
        );
    }

    /// popup 을 먼저 열어 두고 그 뒤에 modifier 를 처음 홀드하는 순서 — popup 의
    /// `LayerId` 가 modifier-hint 보다 먼저 등록되므로, 자연 등록 순서만 믿으면
    /// modifier-hint 가 popup 위로 올라간다(정책 위반). `set_sublayer` 는 등록
    /// 시점과 무관하게 강제하므로 이 순서에서도 popup 이 위여야 한다.
    #[test]
    fn modifier_hint_is_pinned_below_popup_even_when_popup_registers_first() {
        let ctx = egui::Context::default();
        let point = egui::pos2(5.0, 5.0);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));

        // FullOutput 불필요 — 이 테스트는 order/z-stack 결과만 확인한다.
        let _ = ctx.run(Default::default(), |ctx| {
            register_area(ctx, "popup_instance", rect);
            register_area(ctx, "modhint_layer", rect);

            enforce_foreground_z_order(
                ctx,
                None,
                Some(layer_of("modhint_layer")),
                &[layer_of("popup_instance")],
                &[],
            );
        });

        assert_eq!(
            ctx.layer_id_at(point),
            Some(layer_of("popup_instance")),
            "popup 이 modifier-hint 위에 그려져야 한다(등록 순서가 반대여도)"
        );
    }

    /// `enforce_foreground_z_order` 없이는 위 순서(popup 먼저, modifier-hint 나중)에서
    /// modifier-hint 가 자연스럽게 위로 올라간다 — 위 테스트가 실제로 이 케이스를
    /// 잡아내는 테스트임을 보여주는 characterization test.
    #[test]
    fn without_enforcement_later_popup_registration_order_flips_modifier_hint_on_top() {
        let ctx = egui::Context::default();
        let point = egui::pos2(5.0, 5.0);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));

        // FullOutput 불필요 — 이 테스트는 order/z-stack 결과만 확인한다.
        let _ = ctx.run(Default::default(), |ctx| {
            register_area(ctx, "popup_instance", rect);
            register_area(ctx, "modhint_layer", rect);
            // enforce_foreground_z_order 호출 없음.
        });

        assert_eq!(
            ctx.layer_id_at(point),
            Some(layer_of("modhint_layer")),
            "강제 없이는 나중에 등록된 modifier-hint 가 popup 위로 올라간다"
        );
    }

    fn base_font() -> EffectiveFont {
        EffectiveFont {
            font_family: String::new(),
            font_size: 14.0,
            custom_font_path: String::new(),
            line_height: 1.0,
            font_scale_mode: "auto".to_string(),
        }
    }

    #[test]
    fn signature_stable_for_identical_input() {
        let f = base_font();
        assert_eq!(term_font_signature(&f, 14.0), term_font_signature(&f, 14.0));
    }

    #[test]
    fn signature_differs_when_family_changes() {
        let a = base_font();
        let mut b = base_font();
        b.font_family = "Hack".into();
        assert_ne!(term_font_signature(&a, 14.0), term_font_signature(&b, 14.0));
    }

    #[test]
    fn signature_differs_when_custom_font_path_changes() {
        let a = base_font();
        let mut b = base_font();
        b.custom_font_path = "/tmp/x.ttf".into();
        assert_ne!(term_font_signature(&a, 14.0), term_font_signature(&b, 14.0));
    }

    #[test]
    fn signature_differs_when_line_height_changes() {
        let a = base_font();
        let mut b = base_font();
        b.line_height = 1.25;
        assert_ne!(term_font_signature(&a, 14.0), term_font_signature(&b, 14.0));
    }

    #[test]
    fn signature_differs_when_effective_size_changes() {
        let f = base_font();
        // scale_factor 변화(HiDPI 모니터 전환) 또는 font_scale_mode 토글의 결과.
        assert_ne!(term_font_signature(&f, 14.0), term_font_signature(&f, 28.0));
    }

    #[test]
    fn signature_collapses_empty_and_normalized_family() {
        // 정규화 mismatch(`""` ↔ `"D2Coding ligature"`) 가 신호 안 되도록,
        // signature 는 사용자가 입력한 raw family 만 본다.
        let f = base_font();
        let sig_empty = term_font_signature(&f, 14.0);
        // 같은 `""` 가 두 번 들어왔을 때 같은 signature 이어야 한다 — 매 frame
        // update_font 호출되던 G2 회귀의 회귀 방지.
        assert_eq!(sig_empty, term_font_signature(&f, 14.0));
    }

    /// host popup ↔ plugin popup z-order 판정 — `docs/design/systems/popup.md` 규칙 7
    /// ("나중에 열리거나 클릭된 것이 앞") 재현 대상 시나리오: markdown "Open Markdown
    /// File"(plugin, 먼저 열림) → Browse → `file_picker`(host, 나중에 열림) 이면
    /// host 가 위여야 한다.
    #[test]
    fn host_popup_on_top_when_host_z_seq_is_higher() {
        assert!(host_popup_should_render_on_top(Some(2), Some(1)));
    }

    /// 기본/현재 동작(plugin popup 이 host popup 보다 나중에 열린 경우) — plugin 이 위에
    /// 남아 있어야 한다(변경 없음).
    #[test]
    fn plugin_popup_stays_on_top_when_plugin_z_seq_is_higher() {
        assert!(!host_popup_should_render_on_top(Some(1), Some(2)));
    }

    /// 한쪽 종류의 popup 만 열려 있으면(겹칠 대상이 없음) 항상 false — 기존 단일-매니저
    /// 경로(순서 변경 없음)를 그대로 탄다.
    #[test]
    fn no_contention_when_only_one_kind_open() {
        assert!(!host_popup_should_render_on_top(Some(5), None));
        assert!(!host_popup_should_render_on_top(None, Some(5)));
        assert!(!host_popup_should_render_on_top(None, None));
    }
}
