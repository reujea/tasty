use winit::window::Window;

use crate::adapters::ui;
use crate::model::{PhysicalPx, PhysicalRect};
use crate::runtime::engine_read::EngineRead;
use crate::settings::EffectiveFont;
use crate::state::MainViewState;
use tasty_font::FontRequest;

use super::GpuState;

/// 터미널 렌더러에 넘길 글꼴 값. 창을 만들 때와 설정이 바뀌었을 때 모두 이 함수를 거친다.
/// family는 해석하지 않은 원본이다. 해석된 family를 원본과 비교하면 빈 값·monospace의 정규화
/// 때문에 매 프레임 다른 값으로 판단할 수 있다.
pub(super) fn terminal_font_request(font: &EffectiveFont, scale_factor: f32) -> FontRequest {
    FontRequest {
        size: PhysicalPx(font.effective_font_size(scale_factor)),
        family: font.font_family.clone(),
        custom_font_path: font.custom_font_path.clone(),
        line_height: font.line_height,
    }
}

/// 창을 만들 때 저장 설정으로 터미널 글꼴을 구성하고, 구성에 실제로 적용한 값의 서명을 함께
/// 돌려준다. GPU 없이 구성되므로 첫 프레임 전에 저장값이 적용되는지 시험할 수 있다.
pub(super) fn boot_terminal_font(
    appearance: &crate::settings::AppearanceSettings,
    scale_factor: f32,
) -> (tasty_font::FontConfig, String) {
    let request = terminal_font_request(&appearance.effective_terminal_font(), scale_factor);
    let config = tasty_font::FontConfig::from_request(&request);
    let signature = term_font_signature(config.request());
    (config, signature)
}

/// 이 프레임에 터미널 글꼴을 다시 적용해야 하면 그 값을 돌려준다.
pub(super) fn terminal_font_update(
    last_signature: &str,
    appearance: &crate::settings::AppearanceSettings,
    scale_factor: f32,
) -> Option<FontRequest> {
    let request = terminal_font_request(&appearance.effective_terminal_font(), scale_factor);
    (term_font_signature(&request) != last_signature).then_some(request)
}

/// 변경 감지용 서명. 문자열로 비교해 NaN 행간도 같은 값으로 본다.
pub(super) fn term_font_signature(font: &FontRequest) -> String {
    format!(
        "{}|{}|{}|{}",
        font.size.value(),
        font.family,
        font.custom_font_path,
        font.line_height
    )
}

/// 최소화된 창의 크기는 egui 화면으로 쓰지 않고 직전 화면을 유지한다. egui-winit 는 크기 0 만
/// 최소화로 보는데, 장식 없는 창(CSD)은 Windows 에서 최소화 중에도 작업 표시줄 아이콘 크기의
/// client 영역(측정 160x28)을 보고한다. 그 크기로 한 프레임을 그리면 팝업이 그 안으로 줄고 옮겨지며, 제목줄과
/// 여백보다 낮은 콘텐츠 영역이 만들어진다. 복원하면 직전 화면 그대로 이어진다.
pub(crate) fn keep_screen_while_minimized(raw_input: &mut egui::RawInput, minimized: bool) {
    if minimized {
        raw_input.screen_rect = None;
    }
}

/// 같은 Foreground 안에서 배너 < 상태바·탭바, modifier-hint < 팝업 순서를 정한다.
/// move_to_top만 반복하면 등록 순서가 남으므로 set_sublayer로 바로 위에 배치한다.
/// 중첩은 한 단계만 지원해 두 그룹을 서로 연결하지 않는다. 배너와 modifier-hint 사이는
/// 항상 먼저 등록되는 배너의 순서를 사용한다. 별도 OS 모달과 다른 Order 영역은 대상이 아니다.
fn enforce_foreground_z_order(
    ctx: &egui::Context,
    banner_layer: Option<egui::LayerId>,
    modifier_hint_layer: Option<egui::LayerId>,
    popup_layers: &[egui::LayerId],
    pane_rects: &[(u32, PhysicalRect)],
) {
    if let Some(banner_layer) = banner_layer {
        ctx.set_sublayer(banner_layer, ui::status_bar::status_bar_layer_id());
        for (pane_id, _) in pane_rects {
            ctx.set_sublayer(banner_layer, ui::tab_bar::pane_tab_bar_layer(*pane_id));
        }
    }

    if let Some(modifier_hint_layer) = modifier_hint_layer {
        for popup_layer in popup_layers {
            ctx.set_sublayer(modifier_hint_layer, *popup_layer);
        }
    }
}

/// host·plugin 팝업의 그룹 순서를 set_sublayer로 지정한다. 단순 그리기 호출 순서에 의존하지 않는다.
/// 두 종류가 번갈아 겹치는 개별 순서는 지원하지 않고 그룹 전체를 위·아래로 배치한다.
/// modifier-hint와 함께 있으면 같은 host 레이어가 두 부모에 속할 수 있어 최종 순서가 일정하지 않을 수 있다.
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

/// 양쪽 팝업 그룹의 최대 z_seq를 비교한다. 한쪽이 없으면 false다.
/// 셸 정렬과 GPU 콘텐츠 합성은 같은 값을 사용한다. 그룹 전체를 정렬하므로
/// host A < plugin B < host C처럼 종류가 섞인 개별 순서를 모두 재현하지는 못한다.
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
        state: &mut MainViewState,
        engine: &EngineRead<'_>,
        window: &Window,
        pane_rects: &[(u32, PhysicalRect)],
        dividers: &[PhysicalRect],
        terminal_rect: PhysicalRect,
        plugin_manager: Option<crate::app::plugin_display::PluginDisplay<'_>>,
        host_popup_on_top: bool,
    ) -> egui::FullOutput {
        let mut raw_input = self.egui_state.take_egui_input(window);
        // 최소화 중 크기를 잘못 보고하는 것은 Windows 의 장식 없는 창이다. 다른 OS 는 그대로 둔다.
        keep_screen_while_minimized(
            &mut raw_input,
            cfg!(windows) && window.is_minimized() == Some(true),
        );
        self.egui_ctx.options_mut(|o| {
            o.line_scroll_speed = engine.settings.general.wheel_line_scroll;
        });
        let scale_factor = self.scale_factor;
        let proxy = &self.proxy;
        // 직전 동기화가 지원 모드를 골랐으면 그 모드·그림 위치·페이지로 그림을 준비한다. 동기화는 이 그림이
        // present 된 프레임에만 페이지를 드러낸다.
        let has_overlay_surface = self.overlay_surface.is_some();
        let surface_size = [self.size.width, self.size.height];

        self.egui_ctx.run(raw_input, |ctx| {
            crate::state::webview_overlay::begin_frame(ctx, &mut state.webview_overlay);
            // 타이틀바를 먼저 등록해 사이드바가 그 아래에서 시작하게 한다.
            ui::titlebar::draw_titlebar(ctx, state, window, proxy);
            let plugin_alert = plugin_manager.map_or(0, |m| m.attention_count());
            // 닫힌 대상의 이동 대기 표시와 "이곳으로 이동" 메뉴가 남지 않게 한다.
            ui::move_source::clear_if_target_closed(engine, &mut state.pending_move);
            ui::draw_ui(ctx, state, engine, scale_factor, plugin_alert);
            ui::draw_pane_dividers(ctx, dividers, scale_factor);
            ui::draw_surface_highlights(ctx, state, engine, terminal_rect, scale_factor);
            ui::draw_pane_tab_bars(ctx, state, engine, pane_rects, terminal_rect, scale_factor);
            ui::draw_egui_panels(ctx, state, engine, pane_rects, scale_factor);
            // 탭 바와 점유 테두리 뒤에 그려 대상 rect에서 가장 마지막에 둔다.
            let move_mark = ui::move_source::resolve(
                state.pending_move,
                &state.navigation,
                engine,
                state.active_workspace_index(engine),
                pane_rects,
                state.tab_bar_height,
                Some(&state.layout_previews),
                scale_factor,
            );
            ui::move_source::draw_move_source_ring(ctx, move_mark, scale_factor);
            ui::draw_status_bar(ctx, state, engine, terminal_rect, scale_factor);
            ui::link_remote_tip::draw(ctx, state, scale_factor);
            // host 팝업을 먼저 그려 같은 프레임의 영역을 plugin hit-test에 전달한다.
            // 반대로 host가 보는 plugin 영역은 직전 프레임 값이다.
            // source_guards::frame_draw_order가 호출 순서를 검사한다.
            crate::plugin_bridge::popup_scope::inherit_file_picker_scope(
                state,
                plugin_manager
                    .into_iter()
                    .flat_map(|mgr| mgr.popup_instances()),
            );
            let popup_layout =
                ui::draw_popups(ctx, state, engine, pane_rects, terminal_rect, scale_factor);
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
            // 입력 영역 갱신 순서와 별개로 최종 레이어 순서를 명시한다.
            enforce_host_plugin_popup_z_order(
                ctx,
                &state.popup_layers,
                &state.plugin_popup_layers,
                host_popup_on_top,
            );
            crate::plugin_bridge::banner_render::draw_plugin_banners(
                ctx,
                state,
                engine,
                plugin_manager,
            );
            ui::drop_overlay::draw_drop_overlay(ctx, state, engine, terminal_rect, scale_factor);
            // 안내 열은 같은 프레임의 배너·검색 막대 아래에 놓고, attach 대기는 그 아래에 잇는다.
            let mut obstacles = state.banners.window_scope_rects();
            obstacles.extend(
                state
                    .popups
                    .visible_rect(ui::search_bar::SEARCH_BAR_POPUP_ID),
            );
            let notice_column_occupied = ui::restore_notice::draw_notice_column(
                ctx,
                &mut state.restore_notices,
                terminal_rect,
                state.tab_bar_height,
                scale_factor,
                &obstacles,
            );
            ui::attach_loading::draw(
                ctx,
                state,
                engine,
                terminal_rect,
                scale_factor,
                notice_column_occupied,
            );

            crate::state::webview_overlay::end_frame(ctx, &mut state.webview_overlay);
            crate::state::webview_overlay::prepare_paint_job(
                &mut state.webview_overlay,
                has_overlay_surface,
                surface_size,
                |targets, pages| {
                    super::overlay_paint::overlay_shapes(ctx, targets, host_popup_on_top, pages)
                },
            );
            // egui가 프레임마다 커서를 갱신하므로 저장된 창 리사이즈 방향도 여기서 반영한다.
            if let Some(dir) = state.pending_resize_cursor {
                ctx.set_cursor_icon(ui::titlebar::resize_cursor(dir));
            }
        })
    }

    pub(super) fn post_egui_update(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        _prev_theme: &str,
    ) {
        // 설정 변경은 App의 SettingsUpdated에서 전달받고, 여기서는 현재 Theme의 스타일을 다시 적용한다.
        tasty_egui_theme::apply_theme_to_egui(&crate::theme::theme(), &self.egui_ctx);

        if let Some(term_font) = terminal_font_update(
            &self.last_term_font_sig,
            &engine.settings.appearance,
            self.scale_factor,
        ) {
            self.renderer
                .update_font(&self.device, &self.queue, &term_font);
            self.renderer
                .resize(&self.queue, self.size.width, self.size.height);
            self.last_term_font_sig = term_font_signature(self.renderer.font_request());
        }
    }

    /// Apply the theme to the egui context.
    pub(super) fn apply_theme(ctx: &egui::Context, _theme: &str) {
        tasty_egui_theme::apply_theme_to_egui(&crate::theme::theme(), ctx);
    }

    /// Re-apply the current global `Theme` to this window's egui context.
    /// App settings 적용에서 main과 modal에 같은 스타일을 전달한다.
    pub fn refresh_theme(&self) {
        tasty_egui_theme::apply_theme_to_egui(&crate::theme::theme(), &self.egui_ctx);
    }

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
        // egui의 비입력 프레임이 창 IME를 끄지 않게 한다.
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

        let output = match self.acquire_surface() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("modal surface error: {e}");
                return;
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

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
        self.submit_commands(std::iter::once(encoder.finish()));

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
        self.submit_commands(std::iter::once(egui_encoder.finish()));

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // 모달·프리셋 창의 캡처도 present 전에 처리한다.
        if let Some(path) = self.pending_screenshot.take() {
            self.capture_frame_to_png(&output.texture, self.size.width, self.size.height, &path);
        }

        self.present_surface(output);
    }
}

#[cfg(test)]
// 테스트는 의도적으로 무시하는 결과가 많아 let _ 사유 검사에서 제외한다.
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

    /// 실제 등록 순서인 상태바·탭바→배너로 시작해 정렬 보정이 적용되는지 검사한다.
    #[test]
    fn banner_is_pinned_below_status_bar_and_tab_bar() {
        let ctx = egui::Context::default();
        let status_point = egui::pos2(5.0, 5.0);
        let tab_point = egui::pos2(50.0, 5.0);
        let status_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));
        let tab_rect = egui::Rect::from_min_size(egui::pos2(45.0, 0.0), egui::vec2(10.0, 10.0));
        let banner_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 10.0));

        let _ = ctx.run(Default::default(), |ctx| {
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

    /// 정렬 보정을 생략한 대조군에서는 뒤에 등록한 배너가 위에 남는다.
    #[test]
    fn without_enforcement_banner_naturally_ends_up_on_top() {
        let ctx = egui::Context::default();
        let point = egui::pos2(5.0, 5.0);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));

        let _ = ctx.run(Default::default(), |ctx| {
            register_area(ctx, "workspace_status_bar", rect);
            register_area(ctx, "banner_layer", rect);
        });

        assert_eq!(
            ctx.layer_id_at(point),
            Some(layer_of("banner_layer")),
            "강제 없이는 나중에 등록된 banner 가 위에 그려진다(수정 전 버그 재현)"
        );
    }

    /// 먼저 연 팝업도 나중에 등록한 modifier-hint보다 위에 있어야 한다.
    #[test]
    fn modifier_hint_is_pinned_below_popup_even_when_popup_registers_first() {
        let ctx = egui::Context::default();
        let point = egui::pos2(5.0, 5.0);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));

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

    /// 정렬 보정이 없으면 뒤에 등록한 modifier-hint가 위에 남는 대조군.
    #[test]
    fn without_enforcement_later_popup_registration_order_flips_modifier_hint_on_top() {
        let ctx = egui::Context::default();
        let point = egui::pos2(5.0, 5.0);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0));

        let _ = ctx.run(Default::default(), |ctx| {
            register_area(ctx, "popup_instance", rect);
            register_area(ctx, "modhint_layer", rect);
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

    fn sig(font: &EffectiveFont) -> String {
        term_font_signature(&terminal_font_request(font, 1.0))
    }

    /// 창을 만들 때 쓰는 값 묶음이 저장한 파일과 행간을 담는다. 빠지면 재시작과 새 창에서
    /// 저장값이 적용되지 않는다.
    #[test]
    fn request_carries_every_saved_value() {
        let mut f = base_font();
        f.font_family = "Hack".into();
        f.custom_font_path = "/tmp/x.ttf".into();
        f.line_height = 1.5;
        let request = terminal_font_request(&f, 2.0);
        assert_eq!(request.size, PhysicalPx(28.0));
        assert_eq!(request.family, "Hack");
        assert_eq!(request.custom_font_path, "/tmp/x.ttf");
        assert_eq!(request.line_height, 1.5);
    }

    /// family는 원본을 유지한다. 해석된 이름을 넣으면 빈 값이 매 프레임 재적용을 일으킨다.
    #[test]
    fn request_keeps_the_family_unresolved() {
        assert_eq!(terminal_font_request(&base_font(), 1.0).family, "");
    }

    /// 묶음 D2Coding 의 unitsPerEm 을 절반으로 줄여 칸 폭이 14가 되는 파일. 시스템 글꼴과 겹치지
    /// 않도록 family 이름도 바꾼다.
    fn wide_renamed_font() -> Vec<u8> {
        let mut font = tasty_font::D2CODING_REGULAR_TTF.to_vec();
        let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
        let table = |font: &[u8], tag: &[u8; 4]| {
            let r = (0..tables)
                .map(|i| 12 + 16 * i)
                .find(|&r| &font[r..r + 4] == tag)
                .unwrap();
            let word =
                |at: usize| u32::from_be_bytes(font[at..at + 4].try_into().unwrap()) as usize;
            (word(r + 8), word(r + 12))
        };
        let (head, _) = table(&font, b"head");
        let units = u16::from_be_bytes([font[head + 18], font[head + 19]]);
        font[head + 18..head + 20].copy_from_slice(&(units / 2).to_be_bytes());
        let (name, len) = table(&font, b"name");
        let from: Vec<u8> = "D2Coding"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let to: Vec<u8> = "D2Codinh"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        for start in name..name + len - from.len() {
            if font[start..start + from.len()] == from[..] {
                font[start..start + to.len()].copy_from_slice(&to);
            }
        }
        font
    }

    /// 저장 설정으로 창을 만들면 custom 파일과 줄 높이가 첫 프레임 전에 적용되고, 첫 프레임은
    /// 글꼴을 다시 적용하지 않는다(`terminal_font_updates == 0`과 같은 판정).
    #[test]
    fn a_new_window_applies_the_saved_font_before_the_first_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.ttf");
        std::fs::write(&path, wide_renamed_font()).unwrap();
        let saved = format!(
            "[default_font]\nfont_family = \"D2Codinh ligature\"\ncustom_font_path = {:?}\nline_height = 1.5\n",
            path.display().to_string()
        );
        let appearance: crate::settings::AppearanceSettings = toml::from_str(&saved).unwrap();
        let (config, signature) = boot_terminal_font(&appearance, 1.0);
        assert_eq!(config.metrics.cell_height, 21.0);
        assert_eq!(config.metrics.cell_width, 14.0);
        assert_eq!(signature, term_font_signature(config.request()));
        assert!(terminal_font_update(&signature, &appearance, 1.0).is_none());
    }

    #[test]
    fn signature_stable_for_identical_input() {
        let f = base_font();
        assert_eq!(sig(&f), sig(&f));
    }

    #[test]
    fn signature_differs_when_family_changes() {
        let a = base_font();
        let mut b = base_font();
        b.font_family = "Hack".into();
        assert_ne!(sig(&a), sig(&b));
    }

    #[test]
    fn signature_differs_when_custom_font_path_changes() {
        let a = base_font();
        let mut b = base_font();
        b.custom_font_path = "/tmp/x.ttf".into();
        assert_ne!(sig(&a), sig(&b));
    }

    #[test]
    fn signature_differs_when_line_height_changes() {
        let a = base_font();
        let mut b = base_font();
        b.line_height = 1.25;
        assert_ne!(sig(&a), sig(&b));
    }

    #[test]
    fn signature_differs_when_effective_size_changes() {
        let f = base_font();
        assert_ne!(
            term_font_signature(&terminal_font_request(&f, 1.0)),
            term_font_signature(&terminal_font_request(&f, 2.0))
        );
    }

    /// 나중에 연 host 팝업이 plugin 팝업보다 위에 온다.
    #[test]
    fn host_popup_on_top_when_host_z_seq_is_higher() {
        assert!(host_popup_should_render_on_top(Some(2), Some(1)));
    }

    #[test]
    fn plugin_popup_stays_on_top_when_plugin_z_seq_is_higher() {
        assert!(!host_popup_should_render_on_top(Some(1), Some(2)));
    }

    #[test]
    fn no_contention_when_only_one_kind_open() {
        assert!(!host_popup_should_render_on_top(Some(5), None));
        assert!(!host_popup_should_render_on_top(None, Some(5)));
        assert!(!host_popup_should_render_on_top(None, None));
    }
}
