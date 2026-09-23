#![forbid(unsafe_code)]

//! Tasty egui-mesh PoC plugin (A1).
//!
//! egui-mesh 채널(ADR-0028)이 동작함을 증명하는 최소 소비자다. plugin 이 자기
//! 프로세스에서 egui 를 구동·tessellate 해 mesh 를 host 에 commit 하면, host 가
//! 전용 `egui_wgpu::Renderer` 로 surface 영역에 벡터 합성한다.
//!
//! 코덱/SDK 는 재구현하지 않는다 — [`EguiMeshSurface`] 헬퍼만 호출한다.
//! `surface.set_context`(host→plugin) → `run_frame`/`tessellate`/`encode` →
//! `paint`(shared buffer commit + `PaintFrame` 알림) 전 과정을 SDK 가 은닉한다.
//!
//! 데모는 label 1개에 더해 입력 forward 를 눈으로 확인할 수 있도록 클릭 카운터
//! 버튼과 스크롤 영역을 둔다 — host 가 forward 한 실제 사용자 입력이 plugin 의
//! mesh 를 바꾸는지 검증한다.

use std::collections::HashMap;

use tasty_plugin_sdk::{
    BannerClosedCtx, BannerOpenCtx, BannerSetContextCtx, EguiMeshBanner, EguiMeshPopup,
    EguiMeshSurface, Plugin, PopupClosedCtx, PopupOpenCtx, PopupOpenResult, PopupSetContextCtx,
    SurfaceCreateCtx, SurfaceResult, SurfaceSetContextCtx,
};

const PLUGIN_ID: &str = "com.tasty.mesh-demo";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Default)]
struct MeshDemoPlugin {
    /// surface_id → plugin 측 egui 렌더 상태(폰트 atlas·shared buffer 소유).
    surfaces: HashMap<u32, EguiMeshSurface>,
    /// surface_id → 클릭 횟수. 입력 forward 가 mesh 를 바꾸는 것을 보이는 데모 상태.
    clicks: HashMap<u32, u32>,
    /// popup instance_id → egui-mesh popup 렌더 상태(A2). open 시 생성, closed 시 해제.
    popups: HashMap<u64, EguiMeshPopup>,
    /// popup instance_id → 클릭 횟수. 입력 forward 가 popup mesh 를 바꾸는 데모 상태.
    popup_clicks: HashMap<u64, u32>,
    /// banner instance_id → egui-mesh banner 렌더 상태(A3). open 시 생성, closed 시 해제.
    banners: HashMap<u64, EguiMeshBanner>,
    /// banner instance_id → 클릭 횟수. 입력 forward 가 banner mesh 를 바꾸는 데모 상태.
    banner_clicks: HashMap<u64, u32>,
}

impl Plugin for MeshDemoPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        // egui-mesh surface 는 tree(UiNode) 가 아니라 mesh 채널로 그린다 — 빈 결과.
        SurfaceResult::default()
    }

    fn destroy_surface(&mut self, surface_id: u32) {
        // surface 가 닫히면 egui Context·shared buffer 매핑·데모 상태를 함께 해제.
        self.surfaces.remove(&surface_id);
        self.clicks.remove(&surface_id);
    }

    fn paint_surface(&mut self, ctx: SurfaceSetContextCtx) {
        self.paint(ctx);
    }

    fn open_popup(&mut self, ctx: PopupOpenCtx) -> PopupOpenResult {
        // egui-mesh popup 은 tree 가 아니라 mesh 채널(paint_popup)로 그린다 — 빈 트리.
        // 인스턴스별 데모 상태만 초기화한다.
        self.popup_clicks.entry(ctx.instance_id).or_insert(0);
        PopupOpenResult::default()
    }

    fn paint_popup(&mut self, ctx: PopupSetContextCtx) {
        self.paint_popup_impl(ctx);
    }

    fn on_popup_closed(&mut self, ctx: PopupClosedCtx) {
        // popup 이 닫히면 egui Context·shared buffer 매핑·데모 상태를 함께 해제.
        self.popups.remove(&ctx.instance_id);
        self.popup_clicks.remove(&ctx.instance_id);
    }

    fn open_banner(&mut self, ctx: BannerOpenCtx) {
        // egui-mesh banner 는 tree 가 아니라 mesh 채널(paint_banner)로 그린다 — 인스턴스별
        // 데모 상태만 초기화한다.
        self.banner_clicks.entry(ctx.instance_id).or_insert(0);
    }

    fn paint_banner(&mut self, ctx: BannerSetContextCtx) {
        self.paint_banner_impl(ctx);
    }

    fn on_banner_closed(&mut self, ctx: BannerClosedCtx) {
        // banner 가 닫히면(TTL/close X/plugin 요청) egui Context·매핑·데모 상태를 해제.
        self.banners.remove(&ctx.instance_id);
        self.banner_clicks.remove(&ctx.instance_id);
    }
}

impl MeshDemoPlugin {
    /// `set_context` 한 frame 을 그려 host 에 mesh 를 회신한다.
    #[cfg(unix)]
    fn paint(&mut self, ctx: SurfaceSetContextCtx) {
        let sid = ctx.params.surface_id;
        // surfaces / clicks 는 서로소 필드라 동시 mutable 차용이 안전하다.
        let surface = self
            .surfaces
            .entry(sid)
            .or_insert_with(|| EguiMeshSurface::new(sid));
        let clicks = self.clicks.entry(sid).or_insert(0);
        // tessellate+encode+commit 지연 측정 (ADR-0028 reconsideration trigger 점검용).
        let t0 = std::time::Instant::now();
        let result = surface.paint(&ctx.host, &ctx.params, |egui_ctx| {
            draw_demo(egui_ctx, clicks);
        });
        match result {
            Ok(Some(_gen)) => {
                let us = t0.elapsed().as_micros();
                tracing::info!("mesh-demo surface {sid} paint sent in {us}us");
            }
            Ok(None) => {} // 정적 화면 — 송신 생략.
            Err(e) => tracing::warn!("mesh-demo surface {sid} paint failed: {e}"),
        }
    }

    /// egui-mesh shared-buffer 송신은 현재 unix 전용(host buffer.rs 가 windows 미구현).
    /// 다른 OS 에선 채널이 비활성이라 no-op — 크로스플랫폼 컴파일만 보장한다.
    #[cfg(not(unix))]
    fn paint(&mut self, _ctx: SurfaceSetContextCtx) {}

    /// `popup.set_context` 한 frame 을 그려 host 에 popup mesh 를 회신한다(A2).
    #[cfg(unix)]
    fn paint_popup_impl(&mut self, ctx: PopupSetContextCtx) {
        let iid = ctx.params.instance_id;
        // popups / popup_clicks 는 서로소 필드라 동시 mutable 차용이 안전하다.
        let popup = self
            .popups
            .entry(iid)
            .or_insert_with(|| EguiMeshPopup::new(iid));
        let clicks = self.popup_clicks.entry(iid).or_insert(0);
        let result = popup.paint(&ctx.host, &ctx.params, |egui_ctx| {
            draw_popup(egui_ctx, clicks);
        });
        match result {
            Ok(Some(_gen)) => tracing::info!("mesh-demo popup {iid} paint sent"),
            Ok(None) => {} // 정적 화면 — 송신 생략.
            Err(e) => tracing::warn!("mesh-demo popup {iid} paint failed: {e}"),
        }
    }

    #[cfg(not(unix))]
    fn paint_popup_impl(&mut self, _ctx: PopupSetContextCtx) {}

    /// `banner.set_context` 한 frame 을 그려 host 에 banner mesh 를 회신한다(A3).
    #[cfg(unix)]
    fn paint_banner_impl(&mut self, ctx: BannerSetContextCtx) {
        let iid = ctx.params.instance_id;
        // banners / banner_clicks 는 서로소 필드라 동시 mutable 차용이 안전하다.
        let banner = self
            .banners
            .entry(iid)
            .or_insert_with(|| EguiMeshBanner::new(iid));
        let clicks = self.banner_clicks.entry(iid).or_insert(0);
        let result = banner.paint(&ctx.host, &ctx.params, |egui_ctx| {
            draw_banner(egui_ctx, clicks);
        });
        match result {
            Ok(Some(_gen)) => tracing::info!("mesh-demo banner {iid} paint sent"),
            Ok(None) => {} // 정적 화면 — 송신 생략.
            Err(e) => tracing::warn!("mesh-demo banner {iid} paint failed: {e}"),
        }
    }

    #[cfg(not(unix))]
    fn paint_banner_impl(&mut self, _ctx: BannerSetContextCtx) {}
}

/// 데모 UI: label + 클릭 카운터 버튼 + 스크롤 영역. 색/폰트는 egui 기본값을 쓴다
/// (PoC — Theme 토큰 연동은 B1 markdown 전환에서 다룬다).
#[cfg(unix)]
fn draw_demo(ctx: &egui::Context, clicks: &mut u32) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // 카운터를 heading 에 둬 입력 forward 효과(클릭)가 한눈에 보이게 한다.
        ui.heading(format!("Hello egui-mesh — clicks: {clicks}"));
        ui.label("Rendered in the plugin process, composited by the host.");
        // 풀폭 큰 버튼 — 헤드리스 주입이 빗나가지 않게 넉넉한 hit 영역.
        let btn = ui.add_sized([ui.available_width(), 56.0], egui::Button::new("CLICK ME"));
        if btn.clicked() {
            *clicks += 1;
        }
        ui.separator();
        // 스크롤 효과를 보이는 영역 — 줄 번호가 스크롤 시 위로 사라진다.
        //
        // `auto_shrink` 를 끄는 것이 이 표면의 스크롤 가능 여부를 가른다. 기본값(둘 다
        // `true`)이면 egui 가 스크롤하지 않는 축(여기서는 가로)의 크기를 **콘텐츠 폭**으로
        // 줄이고, 휠 hover 판정도 그 좁아진 사각형으로 한다. 라벨이 짧아 그 폭은 100pt
        // 남짓이라, 표면 대부분(오른쪽 빈 영역)에서 굴린 휠은 아무 데도 닿지 않는다 —
        // 화면에는 목록이 전폭으로 보이는데 거기서는 스크롤이 안 되는 상태가 된다.
        // 끄면 판정 사각형이 보이는 영역과 같아진다.
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            // 데모의 목적은 휠 스크롤이 도는지 보이는 것이라 드래그 패닝은 그 목적 밖이다.
            .drag_to_scroll(false)
            .show(ui, |ui| {
                for i in 1..=80 {
                    ui.label(format!("scrollable line {i}"));
                }
            });
    });
}

/// 데모 popup UI: label + 클릭 카운터 버튼. host 가 셸(scrim/border)을 그리고 이 콘텐츠만
/// plugin mesh 로 합성된다 — 입력 forward(클릭)가 popup mesh 를 바꾸는지 검증한다.
#[cfg(unix)]
fn draw_popup(ctx: &egui::Context, clicks: &mut u32) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading(format!("egui-mesh popup — clicks: {clicks}"));
        ui.label("Drawn in the plugin process. Host owns the shell (scrim/border).");
        let btn = ui.add_sized([ui.available_width(), 48.0], egui::Button::new("CLICK ME"));
        if btn.clicked() {
            *clicks += 1;
        }
        ui.separator();
        ui.label("Press Esc or click outside to close.");
    });
}

/// 데모 banner UI: 가로 레이아웃 label + 클릭 카운터 버튼. host 가 셸(컨테이너/border/
/// close X/카운트다운)과 스택/위치/dismiss 를 소유하고, 이 content 만 plugin mesh 로
/// content_rect 에 합성된다 — 입력 forward(클릭)가 banner mesh 를 바꾸는지 검증한다.
#[cfg(unix)]
fn draw_banner(ctx: &egui::Context, clicks: &mut u32) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("egui-mesh banner — clicks: {clicks}"));
            if ui.button("BUMP").clicked() {
                *clicks += 1;
            }
        });
        ui.label("Drawn in the plugin process. Host owns the shell + TTL countdown.");
    });
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    tasty_plugin_sdk::run(MeshDemoPlugin::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_identity() {
        let p = MeshDemoPlugin::default();
        assert_eq!(p.id(), "com.tasty.mesh-demo");
        // Cargo.toml 이 SoT — 하드코딩 기대값은 버전 bump 마다 드리프트한다
        // (0.1.2 vs 0.1.6 실재, Windows 단위테스트 CI 부재로 잠복했던 것).
        assert_eq!(p.version(), env!("CARGO_PKG_VERSION"));
    }

    /// 데모 표면의 **오른쪽 빈 영역**에서 굴린 휠이 목록을 움직이는가.
    ///
    /// host 는 휠 이벤트에 좌표를 싣지 않고 커서 이동 때 `PointerMoved` 로 hover 를
    /// 세운다. 그래서 스크롤 여부는 전적으로 "그 좌표가 `ScrollArea` 의 판정 사각형
    /// 안인가" 로 갈린다. `auto_shrink` 가 켜져 있으면 그 사각형이 콘텐츠 폭(라벨 몇
    /// 십 pt)으로 줄어, 목록이 전폭으로 보이는데도 오른쪽에서는 휠이 먹지 않는다.
    /// 표면 오른쪽 끝 근처를 고른 이유가 그것이다 — 가운데였다면 좁은 사각형 안에
    /// 들어가 버려 회귀를 놓친다.
    ///
    /// 단정 대상은 내부 상태가 아니라 **그려진 결과**다. 이 버그의 증상이 "델타는
    /// 도착하는데 화면이 안 바뀐다" 이므로, 첫 줄이 실제로 위로 올라갔는지를 본다.
    #[cfg(unix)]
    #[test]
    fn the_wheel_scrolls_the_list_from_anywhere_on_the_surface() {
        let (w, h) = (1200.0_f32, 800.0_f32);
        let ctx = egui::Context::default();
        let mut clicks = 0_u32;
        let mut draw = |events: Vec<egui::Event>| -> Option<f32> {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(w, h),
                )),
                events,
                ..Default::default()
            };
            let out = ctx.run(raw, |ctx| draw_demo(ctx, &mut clicks));
            probe_line_y(&out)
        };

        // 표면 오른쪽 끝 근처 — 라벨 콘텐츠 폭 밖이다.
        let far_right = egui::pos2(w - 40.0, h / 2.0);
        let mut before = None;
        for _ in 0..8 {
            before = draw(vec![egui::Event::PointerMoved(far_right)]);
        }
        let before = before.expect("기준 줄이 그려져야 한다");

        draw(vec![
            egui::Event::PointerMoved(far_right),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -50.0),
                modifiers: egui::Modifiers::default(),
            },
        ]);
        let mut after = None;
        for _ in 0..8 {
            after = draw(vec![egui::Event::PointerMoved(far_right)]);
        }
        let after = after.expect("기준 줄이 그려져야 한다");

        let moved = before - after;
        assert!(
            (moved - 50.0).abs() < 5.0,
            "표면 오른쪽에서 굴린 휠 50pt 가 목록을 그만큼 올려야 한다 — \
             {PROBE_LINE} y {before} -> {after} (이동 {moved})",
        );
    }

    /// 이번 frame 에 그려진 기준 줄의 세로 위치. 스크롤되면 그만큼 작아진다.
    ///
    /// 첫 줄이 아니라 20번째 줄을 쓴다 — 첫 줄은 조금만 스크롤해도 뷰포트 밖으로 나가
    /// egui 가 아예 그리지 않으므로(`Ui::is_rect_visible` 컬링), "안 그려졌다" 와
    /// "이동량" 을 구별할 수 없다.
    #[cfg(unix)]
    const PROBE_LINE: &str = "scrollable line 20";

    #[cfg(unix)]
    fn probe_line_y(out: &egui::FullOutput) -> Option<f32> {
        fn walk(shape: &egui::epaint::Shape, found: &mut Option<f32>) {
            match shape {
                egui::epaint::Shape::Text(t) => {
                    if t.galley.job.text.trim() == PROBE_LINE {
                        *found = Some(t.pos.y);
                    }
                }
                egui::epaint::Shape::Vec(v) => {
                    for s in v {
                        walk(s, found);
                    }
                }
                _ => {}
            }
        }
        let mut found = None;
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }

    /// surface **둘**을 넣는다. 하나만 넣으면 `destroy_surface` 가 맵을 통째로 비워도
    /// 이 시험은 초록이라, "7 을 지웠다" 와 "전부 지웠다" 가 안 갈린다.
    #[test]
    fn destroy_clears_state() {
        let mut p = MeshDemoPlugin::default();
        p.clicks.insert(7, 3);
        p.clicks.insert(8, 1);
        p.destroy_surface(7);
        assert!(!p.clicks.contains_key(&7));
        assert_eq!(p.clicks.get(&8), Some(&1));
    }
}
