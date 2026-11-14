#![forbid(unsafe_code)]

//! egui-mesh 채널을 확인하는 데모 플러그인.
//! 클릭 횟수와 스크롤 영역으로 사용자 입력 전달 및 호스트 합성 결과를 확인한다.
//! Mesh 인코딩과 공유 버퍼 전송은 SDK의 EguiMeshSurface를 사용한다.
//! docs/dev-guide/egui-mesh-channel.md#데이터-흐름.

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
    /// surface_id → 입력칸 글자. 호스트 편집 요청(복사·잘라내기·붙여넣기)과 결과 회신을 확인한다.
    texts: HashMap<u32, String>,
    /// surface_id → 형식 붙여넣기 상태. 입력칸 밖에서 받은 형식 붙여넣기(`paste`) 요청의 회신을 확인한다.
    format_pastes: HashMap<u32, FormatPaste>,
    /// Popup 인스턴스별 렌더 상태. 열 때 만들고 닫을 때 해제한다.
    popups: HashMap<u64, EguiMeshPopup>,
    /// popup instance_id → 클릭 횟수. 입력 forward 가 popup mesh 를 바꾸는 데모 상태.
    popup_clicks: HashMap<u64, u32>,
    /// popup instance_id → 입력칸 글자. 호스트 편집 요청(복사·잘라내기·붙여넣기)과 결과 회신을 확인한다.
    popup_texts: HashMap<u64, String>,
    /// Banner 인스턴스별 렌더 상태. 열 때 만들고 닫을 때 해제한다.
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

    /// surface·popup 입력칸을 `EditField`로 그리므로 편집 결과를 회신한다.
    fn reports_edit_outcomes(&self) -> bool {
        true
    }

    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        // egui-mesh surface 는 tree(UiNode) 가 아니라 mesh 채널로 그린다 — 빈 결과.
        SurfaceResult::default()
    }

    fn destroy_surface(&mut self, surface_id: u32) {
        // surface 가 닫히면 egui Context·shared buffer 매핑·데모 상태를 함께 해제.
        self.surfaces.remove(&surface_id);
        self.clicks.remove(&surface_id);
        self.texts.remove(&surface_id);
        self.format_pastes.remove(&surface_id);
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
        self.popup_texts.remove(&ctx.instance_id);
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
        // surfaces / clicks / texts 는 서로소 필드라 동시 mutable 차용이 안전하다.
        let surface = self
            .surfaces
            .entry(sid)
            .or_insert_with(|| EguiMeshSurface::new(sid));
        let clicks = self.clicks.entry(sid).or_insert(0);
        let text = self.texts.entry(sid).or_default();
        let format_paste = self.format_pastes.entry(sid).or_default();
        // tessellate+encode+commit 지연 측정 (docs/dev-guide/egui-mesh-channel.md#데이터-흐름 성능 확인).
        let t0 = std::time::Instant::now();
        let result = surface.paint(&ctx.host, &ctx.params, |egui_ctx| {
            draw_demo(egui_ctx, clicks, text, format_paste);
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

    /// 이 데모의 렌더 경로는 Unix에서만 활성화된다.
    #[cfg(not(unix))]
    fn paint(&mut self, _ctx: SurfaceSetContextCtx) {}

    /// Popup의 입력과 크기를 받아 한 프레임을 그린다.
    #[cfg(unix)]
    fn paint_popup_impl(&mut self, ctx: PopupSetContextCtx) {
        let iid = ctx.params.instance_id;
        // popups / popup_clicks 는 서로소 필드라 동시 mutable 차용이 안전하다.
        let popup = self
            .popups
            .entry(iid)
            .or_insert_with(|| EguiMeshPopup::new(iid));
        let clicks = self.popup_clicks.entry(iid).or_insert(0);
        let text = self.popup_texts.entry(iid).or_default();
        let result = popup.paint(&ctx.host, &ctx.params, |egui_ctx| {
            draw_popup(egui_ctx, clicks, text);
        });
        match result {
            Ok(Some(_gen)) => tracing::info!("mesh-demo popup {iid} paint sent"),
            Ok(None) => {} // 정적 화면 — 송신 생략.
            Err(e) => tracing::warn!("mesh-demo popup {iid} paint failed: {e}"),
        }
    }

    #[cfg(not(unix))]
    fn paint_popup_impl(&mut self, _ctx: PopupSetContextCtx) {}

    /// Banner의 입력과 크기를 받아 한 프레임을 그린다.
    #[cfg(unix)]
    fn paint_banner_impl(&mut self, ctx: BannerSetContextCtx) {
        let iid = ctx.params.instance_id;
        // banners / banner_clicks 는 서로소 필드라 동시 mutable 차용이 안전하다.
        let banner = self
            .banners
            .entry(iid)
            .or_insert_with(|| EguiMeshBanner::new(iid));
        let clicks = self.banner_clicks.entry(iid).or_insert(0);
        let theme = ctx.params.theme.as_ref().map(|w| {
            tasty_type_appearance::theme::Theme::with_colors_and_zoom(
                w.colors.clone(),
                w.is_light,
                w.ui_zoom,
            )
        });
        let result = banner.paint(&ctx.host, &ctx.params, |egui_ctx| {
            draw_banner(egui_ctx, theme.as_ref(), clicks);
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

/// 기본 egui 색과 폰트를 사용하는 클릭·스크롤 데모. 편집 결과를 회신하는 입력칸(`EditField`)으로 호스트
/// 편집 요청이 surface 입력칸에 닿고 결과가 회신되는지 검증한다.
#[cfg(unix)]
fn draw_demo(
    ctx: &egui::Context,
    clicks: &mut u32,
    text: &mut String,
    format_paste: &mut FormatPaste,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // 카운터를 heading 에 둬 입력 forward 효과(클릭)가 한눈에 보이게 한다.
        ui.heading(format!("Hello egui-mesh — clicks: {clicks}"));
        ui.label("Rendered in the plugin process, composited by the host.");
        // 풀폭 큰 버튼 — 헤드리스 주입이 빗나가지 않게 넉넉한 hit 영역.
        let btn = ui.add_sized([ui.available_width(), 56.0], egui::Button::new("CLICK ME"));
        if btn.clicked() {
            *clicks += 1;
        }
        let field = tasty_plugin_sdk::egui_surface::edit::EditField::singleline(text)
            .id_salt("mesh_demo_surface_text")
            .show(ui, |t| t.desired_width(f32::INFINITY));
        take_format_paste(ui, format_paste, field.response.has_focus());
        ui.separator();
        // 가로 자동 축소를 끄면 짧은 라벨의 오른쪽 빈 영역에서도 휠 입력을 받는다.
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

/// 입력칸 밖에서 받은 형식 붙여넣기 요청의 상태. 실제 클립보드 형식은 읽지 않는다. 받으면 횟수를 올려
/// `acted`로 알리고, 받지 않게 하면 `not_acted`로 알려 호스트가 원래 키를 넘기는지 확인한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FormatPaste {
    count: u32,
    accept: bool,
}

impl Default for FormatPaste {
    fn default() -> Self {
        Self {
            count: 0,
            accept: true,
        }
    }
}

/// 형식 붙여넣기의 횟수와 받기 여부를 그리고, 이 패스의 형식 붙여넣기 요청에 결과를 알린다. 입력칸 뒤에 불러야
/// 한다. 입력칸이 포커스를 가지면 입력칸 어댑터가 먼저 `not_acted`로 정하고 그 결과가 남는다.
#[cfg(unix)]
fn take_format_paste(ui: &mut egui::Ui, state: &mut FormatPaste, field_focused: bool) {
    use tasty_plugin_protocol::EditRequestWire;
    use tasty_plugin_sdk::egui_surface::edit::{edit_request, report_edit};
    let ctx = ui.ctx().clone();
    if edit_request(&ctx).is_some_and(|request| request.edit == EditRequestWire::Paste) {
        if state.accept && !field_focused {
            state.count += 1;
        }
        report_edit(&ctx, state.accept);
    }
    ui.horizontal(|ui| {
        ui.label(format!("format pastes: {}", state.count));
        ui.checkbox(&mut state.accept, "accept format paste");
    });
}

/// 데모 popup UI: label + 클릭 카운터 버튼 + 편집 결과를 회신하는 입력칸(`EditField`). host 가
/// 셸(scrim/border)을 그리고 이 콘텐츠만 plugin mesh 로 합성된다 — 입력 forward(클릭)가 popup mesh 를
/// 바꾸는지, 호스트 편집 요청이 입력칸에 닿고 결과가 회신되는지 검증한다.
#[cfg(unix)]
fn draw_popup(ctx: &egui::Context, clicks: &mut u32, text: &mut String) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading(format!("egui-mesh popup — clicks: {clicks}"));
        ui.label("Drawn in the plugin process. Host owns the shell (scrim/border).");
        let btn = ui.add_sized([ui.available_width(), 48.0], egui::Button::new("CLICK ME"));
        if btn.clicked() {
            *clicks += 1;
        }
        ui.separator();
        tasty_plugin_sdk::egui_surface::edit::EditField::singleline(text)
            .id_salt("mesh_demo_popup_text")
            .show(ui, |t| t.desired_width(f32::INFINITY));
        ui.label("Press Esc or click outside to close.");
    });
}

/// 데모 banner UI: 가로 레이아웃 label + 클릭 카운터 버튼. host 가 셸(컨테이너/border/
/// close X/카운트다운)과 스택/위치/dismiss 를 소유하고, 이 content 만 plugin mesh 로
/// content_rect 에 합성된다 — 입력 forward(클릭)가 banner mesh 를 바꾸는지 검증한다.
/// 버튼은 감싸지 않은 공용 Secondary 다. SDK 가 배너 frame 전체에 배너 셸 문맥을 열어
/// host 배너와 같은 배너 박스로 그려지는지 확인하는 예제다. host 가 테마를 보내지 않으면
/// 공용 위젯을 그릴 수 없어 egui 기본 버튼으로 대신한다.
#[cfg(unix)]
fn draw_banner(
    ctx: &egui::Context,
    theme: Option<&tasty_type_appearance::theme::Theme>,
    clicks: &mut u32,
) {
    use tasty_ui_widgets::{Button, ButtonVariant};
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("egui-mesh banner — clicks: {clicks}"));
            let bumped = match theme {
                Some(theme) => Button::new("BUMP")
                    .variant(ButtonVariant::Secondary)
                    .show(ui, theme)
                    .clicked(),
                None => ui.button("BUMP").clicked(),
            };
            if bumped {
                *clicks += 1;
            }
        });
        ui.label("Drawn in the plugin process. Host owns the shell and the close button.");
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
        // 버전은 Cargo.toml에서 읽는다.
        assert_eq!(p.version(), env!("CARGO_PKG_VERSION"));
    }

    /// 라벨 너비 밖인 오른쪽 빈 영역에서 휠을 굴려도 목록이 이동하는지 확인한다.
    /// auto_shrink가 켜지면 이 영역이 스크롤 입력 범위에서 빠질 수 있다.
    #[cfg(unix)]
    #[test]
    fn the_wheel_scrolls_the_list_from_anywhere_on_the_surface() {
        let (w, h) = (1200.0_f32, 800.0_f32);
        let ctx = egui::Context::default();
        let mut clicks = 0_u32;
        let mut text = String::new();
        let mut format_paste = FormatPaste::default();
        let mut draw = |events: Vec<egui::Event>| -> Option<f32> {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(w, h),
                )),
                events,
                ..Default::default()
            };
            let out = ctx.run(raw, |ctx| {
                draw_demo(ctx, &mut clicks, &mut text, &mut format_paste)
            });
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

    /// 다른 surface를 함께 넣어 지정한 항목만 삭제하는지 확인한다.
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
