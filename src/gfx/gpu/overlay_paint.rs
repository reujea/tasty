//! native WebView 위 오버레이 창에 그릴 egui 그림(오버레이 paint job)을 만든다.
//! 계약은 docs/design/systems/webview.md 의 "egui 오버레이와 native WebView" 절과
//! docs/dev-guide/gpu-rendering.md 의 "WebView 오버레이 pass" 절이다.
//!
//! - 대상 레이어(`OverlayFrame::targets`)의 shape 를 `end_pass` 전에 복제한다. 메인 surface 에는 원래대로
//!   모두 그리므로 화면 캡처 결과는 바뀌지 않는다.
//! - host·plugin 팝업 그룹 순서는 메인 pass 와 같은 `host_popup_should_render_on_top` 결정을 쓴다.
//! - 각 shape 와 plugin mesh 의 clip 을 드러낼 WebView 영역과 교차해 오버레이 창은 페이지 위만 칠한다.
//!   페이지 밖의 scrim·그림자·mesh 는 메인 surface 에만 있어 두 번 합성되지 않는다.
//! - present 한 그림의 모드·페이지를 남긴다. WebView 동기화는 그것이 맞을 때만 페이지를 드러낸다.

use egui::epaint::ClippedShape;

use crate::state::webview_overlay::{OverlayTarget, TargetKind};

/// 대상 레이어를 그릴 순서로 정렬한다. egui 레이어 순서(뒤→앞)를 따르되 host 팝업과 plugin 팝업은
/// 그룹 단위로 `host_on_top` 에 맞춘다. 드롭다운은 언제나 맨 위다(egui 팝업은 부모보다 앞).
pub(super) fn ordered_layers(
    targets: &[OverlayTarget],
    egui_order: &[egui::LayerId],
    host_on_top: bool,
) -> Vec<egui::LayerId> {
    let rank = |kind: TargetKind| match (kind, host_on_top) {
        (TargetKind::PluginPopup, false) | (TargetKind::HostPopup, true) => 1,
        (TargetKind::PluginPopup, true) | (TargetKind::HostPopup, false) => 0,
        (TargetKind::Dropdown, _) => 2,
    };
    let pos = |layer: egui::LayerId| {
        egui_order
            .iter()
            .position(|l| *l == layer)
            .unwrap_or(usize::MAX)
    };
    let mut layers: Vec<(usize, usize, egui::LayerId)> = Vec::new();
    for t in targets {
        if !layers.iter().any(|(_, _, l)| *l == t.layer) {
            layers.push((rank(t.kind), pos(t.layer), t.layer));
        }
    }
    layers.sort_by_key(|(rank, pos, _)| (*rank, *pos));
    layers.into_iter().map(|(_, _, l)| l).collect()
}

/// 대상 레이어의 shape 를 순서대로 복제한다. `end_pass` 가 그래픽 레이어를 비우기 전에 불러야 한다.
pub(super) fn clone_layers(ctx: &egui::Context, layers: &[egui::LayerId]) -> Vec<ClippedShape> {
    ctx.graphics(|g| {
        layers
            .iter()
            .filter_map(|l| g.get(*l))
            .flat_map(|list| list.all_entries().cloned())
            .collect()
    })
}

/// shape 마다 clip 을 WebView 영역과 교차한다. 영역 여러 개와 겹치는 shape 는 영역마다 한 번씩 남는다.
/// 겹치지 않는 shape 는 버린다.
pub(super) fn restrict_to_pages(
    shapes: Vec<ClippedShape>,
    pages: &[egui::Rect],
) -> Vec<ClippedShape> {
    let mut out = Vec::new();
    for shape in shapes {
        for page in pages {
            let clip = shape.clip_rect.intersect(*page);
            if clip.is_positive() {
                out.push(ClippedShape {
                    clip_rect: clip,
                    shape: shape.shape.clone(),
                });
            }
        }
    }
    out
}

/// 이번 프레임 오버레이 paint job 의 shape. 드러난 페이지가 없으면 비어 있다.
pub(super) fn overlay_shapes(
    ctx: &egui::Context,
    targets: &[OverlayTarget],
    host_on_top: bool,
    pages: &[egui::Rect],
) -> Vec<ClippedShape> {
    if pages.is_empty() || targets.is_empty() {
        return Vec::new();
    }
    let egui_order: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let layers = ordered_layers(targets, &egui_order, host_on_top);
    restrict_to_pages(clone_layers(ctx, &layers), pages)
}

/// 오버레이 surface 가 메인 surface 와 같은 물리 크기인지. 다르면 같은 좌표로 그릴 수 없으므로 그 프레임은
/// 실패로 돌려 `Hide` 로 돌아간다(크기를 맞춰 다시 만드는 일은 backend 몫이다).
pub(super) fn overlay_matches_main(main: [u32; 2], overlay: [u32; 2]) -> bool {
    main == overlay
}

impl super::GpuState {
    /// 이번 프레임 오버레이 창 그림이 준비됐으면 오버레이 창에 그린다(주 표면 그림은 렌더가 먼저 꺼낸다).
    /// 메인 surface 는 이미 모두 그렸으므로 캡처는 바뀌지 않는다. present 까지 끝나면 그 기준을 `painted` 로 남기고, 같은 프레임의
    /// WebView 동기화가 그것을 보고 페이지를 드러낸다. 실패는 동기화가 읽어 모든 페이지를 숨긴다.
    pub(super) fn paint_webview_overlay(
        &mut self,
        state: &mut crate::state::MainViewState,
        pixels_per_point: f32,
        plugin_manager: Option<crate::app::plugin_display::PluginDisplay<'_>>,
        host_on_top: bool,
    ) {
        let Some(mut job) = state.webview_overlay.paint_job.take() else {
            return;
        };
        let shapes = std::mem::take(&mut job.shapes);
        // 숨긴 host 에 그린 그림은 화면에 없으므로 페이지가 아니라 host 를 보일 근거로만 남긴다.
        let shown = self.overlay_shown();
        match self.render_webview_overlay(
            shapes,
            &job.pages,
            pixels_per_point,
            state.plugin_mesh_popup_regions.as_slice(),
            plugin_manager,
            host_on_top,
        ) {
            Ok(true) => {
                let size = [self.size.width, self.size.height];
                if shown {
                    state.webview_overlay.record_present(job, size);
                } else {
                    state.webview_overlay.record_prepared(job, size);
                }
            }
            Ok(false) => {}
            Err(e) => {
                if state.webview_overlay.health.record_failure() {
                    tracing::warn!(
                        "WebView overlay surface failed, hiding pages under popups: {e}"
                    );
                }
            }
        }
    }

    /// 오버레이 창을 투명으로 지우고 오버레이 paint job 과 plugin mesh 팝업을 그린다. 메인 pass 와 같은
    /// 그룹 순서를 쓰고 mesh 도 `pages` 안만 칠한다. shape 가 없어도 지운 프레임을 올린다(유효한 빈 그림).
    /// 창이 없거나 host 를 숨겼고 준비 중도 아니면 `Ok(false)` 라 기록이 남지 않는다. surface 를 잃었고 다시 설정해도 얻지 못하면 창을 버리고 `Err` 를
    /// 돌려준다. 호출자는 그 프레임부터 `Hide` 로 돌아간다.
    #[allow(clippy::too_many_arguments)] // reason: 오버레이 pass 한 번에 필요한 프레임 입력 그대로
    pub(super) fn render_webview_overlay(
        &mut self,
        shapes: Vec<ClippedShape>,
        pages: &[egui::Rect],
        pixels_per_point: f32,
        mesh_popup_regions: &[(u64, crate::model::PhysicalRect)],
        plugin_manager: Option<crate::app::plugin_display::PluginDisplay<'_>>,
        host_on_top: bool,
    ) -> Result<bool, String> {
        let Some(overlay) = self.overlay_surface.as_ref().filter(|o| o.paintable()) else {
            return Ok(false);
        };
        let size = [overlay.config.width, overlay.config.height];
        if !overlay_matches_main([self.size.width, self.size.height], size) {
            return Err(format!(
                "overlay surface {size:?} differs from the window {:?}",
                [self.size.width, self.size.height]
            ));
        }
        let acquire_start = std::time::Instant::now();
        let acquired = match overlay.surface.get_current_texture() {
            Ok(o) => Ok(o),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                overlay.surface.configure(&self.device, &overlay.config);
                overlay
                    .surface
                    .get_current_texture()
                    .map_err(|e| format!("overlay surface lost: {e}"))
            }
            Err(e) => Err(format!("overlay surface failed: {e}")),
        };
        self.overlay_acquire_ms += acquire_start.elapsed().as_secs_f64() * 1000.0;
        let output = match acquired {
            Ok(o) => o,
            Err(e) => {
                self.overlay_surface = None;
                return Err(e);
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("webview_overlay_clear"),
            });
        {
            // 페이지 위 영역 밖은 투명이어야 페이지가 보인다.
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("webview_overlay_clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.submit_commands(std::iter::once(encoder.finish()));
        let paint_jobs = self.egui_ctx.tessellate(shapes, pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point,
        };
        // 텍스처 갱신은 메인 pass 가 이미 했고 해제는 두 pass 뒤에 한다.
        self.render_egui_pass_and_mesh_popups(
            &view,
            &egui::TexturesDelta::default(),
            &paint_jobs,
            &screen_descriptor,
            mesh_popup_regions,
            plugin_manager,
            host_on_top,
            Some(pages),
        );
        output.present();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn er(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
    }

    fn target(kind: TargetKind, layer: egui::LayerId) -> OverlayTarget {
        OverlayTarget {
            kind,
            layer,
            rect: er(0.0, 0.0, 10.0, 10.0),
            opaque_rect: er(0.0, 0.0, 10.0, 10.0),
            paint_rect: er(0.0, 0.0, 10.0, 10.0),
            scrim: None,
        }
    }

    fn fg(name: &str) -> egui::LayerId {
        egui::LayerId::new(egui::Order::Foreground, egui::Id::new(name))
    }

    #[test]
    fn the_group_rule_orders_host_and_plugin_popups() {
        let host = fg("host");
        let plugin = fg("plugin");
        let drop = fg("drop");
        let order = [drop, plugin, host];
        let targets = [
            target(TargetKind::Dropdown, drop),
            target(TargetKind::HostPopup, host),
            target(TargetKind::PluginPopup, plugin),
        ];
        assert_eq!(
            ordered_layers(&targets, &order, true),
            vec![plugin, host, drop]
        );
        assert_eq!(
            ordered_layers(&targets, &order, false),
            vec![host, plugin, drop]
        );
    }

    /// 오버레이 창은 메인 창과 같은 물리 크기일 때만 같은 좌표로 그린다.
    #[test]
    fn the_overlay_surface_must_match_the_window_size() {
        assert!(overlay_matches_main([1280, 800], [1280, 800]));
        assert!(!overlay_matches_main([1280, 800], [1280, 799]));
        assert!(!overlay_matches_main([1280, 800], [640, 400]));
    }

    #[test]
    fn clips_are_limited_to_the_pages() {
        let shape = ClippedShape {
            clip_rect: er(0.0, 0.0, 1000.0, 800.0),
            shape: egui::Shape::rect_filled(er(0.0, 0.0, 1000.0, 800.0), 0.0, egui::Color32::BLACK),
        };
        let pages = [er(100.0, 100.0, 200.0, 200.0), er(500.0, 0.0, 100.0, 100.0)];
        let out = restrict_to_pages(vec![shape], &pages);
        let clips: Vec<egui::Rect> = out.iter().map(|s| s.clip_rect).collect();
        assert_eq!(clips, pages.to_vec());
        let away = ClippedShape {
            clip_rect: er(0.0, 0.0, 50.0, 50.0),
            shape: egui::Shape::Noop,
        };
        assert!(restrict_to_pages(vec![away], &pages).is_empty());
    }

    /// 실제 egui 프레임에서 팝업 레이어만 복제되고 탭 바·상태 바 같은 바탕 레이어는 빠진다.
    #[test]
    fn the_overlay_job_holds_popups_and_not_the_tab_or_status_bar() {
        let ctx = egui::Context::default();
        let popup = fg("popup");
        let mut cloned = Vec::new();
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            let tab_bar = egui::LayerId::new(egui::Order::Background, egui::Id::new("tab_bar"));
            let status = egui::LayerId::new(egui::Order::Middle, egui::Id::new("status_bar"));
            ctx.layer_painter(tab_bar).rect_filled(
                er(0.0, 0.0, 400.0, 30.0),
                0.0,
                egui::Color32::RED,
            );
            ctx.layer_painter(status).rect_filled(
                er(0.0, 570.0, 400.0, 30.0),
                0.0,
                egui::Color32::GREEN,
            );
            ctx.layer_painter(popup).rect_filled(
                er(100.0, 100.0, 200.0, 100.0),
                0.0,
                egui::Color32::BLUE,
            );
            cloned = overlay_shapes(
                ctx,
                &[target(TargetKind::HostPopup, popup)],
                false,
                &[er(0.0, 0.0, 400.0, 600.0)],
            );
        });
        let colors: Vec<egui::Color32> = cloned
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) => Some(r.fill),
                _ => None,
            })
            .collect();
        assert_eq!(colors, vec![egui::Color32::BLUE]);
    }
}
