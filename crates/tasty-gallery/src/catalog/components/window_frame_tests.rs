//! 승인·포트·원격 표본 창이 정본처럼 크기 토큰으로 그려지는지 잰다.

use tasty_type_appearance::theme::Theme;

/// `draw`를 넓은 열에서 한 번 그리고 채움이 `fill`이고 테두리가 `edge`인 사각형 크기를 모은다.
fn frame_sizes(
    draw: fn(&mut egui::Ui, &Theme),
    fill: egui::Color32,
    edge: egui::Color32,
) -> Vec<egui::Vec2> {
    let theme = tasty_themes::mocha_fallback();
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(2400.0, 8000.0),
        )),
        ..Default::default()
    };
    let col_w = 2000.0;
    // 패널은 앞 프레임에서 잰 높이로 자리를 잡으므로 몇 프레임 그려 배치가 멈춘 뒤를 잰다.
    let mut out = egui::FullOutput::default();
    for _ in 0..3 {
        out = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(col_w);
                ui.data_mut(|d| d.insert_temp(crate::catalog::spec::body_column_width_id(), col_w));
                draw(ui, &theme);
            });
        });
    }
    let mut sizes = Vec::new();
    let mut stack: Vec<egui::Shape> = out.shapes.into_iter().map(|c| c.shape).collect();
    while let Some(shape) = stack.pop() {
        match shape {
            egui::Shape::Vec(v) => stack.extend(v),
            egui::Shape::Rect(r) if r.fill == fill && r.stroke.color == edge => {
                sizes.push(r.rect.size())
            }
            _ => {}
        }
    }
    sizes
}

/// 프레임 사각형은 내용 크기에 양쪽 테두리를 더한 크기다.
fn framed(theme: &Theme, w: f32, h: Option<f32>) -> (f32, Option<f32>) {
    let b = 2.0 * theme.border_width.value();
    (w + b, h.map(|h| h + b))
}

fn assert_has_frame(sizes: &[egui::Vec2], (w, h): (f32, Option<f32>), what: &str) {
    assert!(
        sizes.iter().any(|s| s.x == w && h.is_none_or(|h| s.y == h)),
        "{what}: {w}×{h:?} 프레임이 없다: {sizes:?}"
    );
}

/// 승인 표본 폭은 정본 ApprovalFrame처럼 approval-width다.
#[test]
fn the_approval_sample_is_approval_width_wide() {
    let th = tasty_themes::mocha_fallback();
    let sizes = frame_sizes(
        super::approval::draw,
        th.bg_panel().to_egui(),
        th.border_frame().to_egui(),
    );
    assert_has_frame(
        &sizes,
        framed(&th, th.approval_width().value(), None),
        "approval",
    );
}

/// 포트 표본 창은 정본 PortsFrame처럼 ports-window-width × ports-window-height다.
#[test]
fn the_ports_sample_window_is_the_ports_window_size() {
    let th = tasty_themes::mocha_fallback();
    let sizes = frame_sizes(
        super::port_scanner::draw,
        th.bg_panel().to_egui(),
        th.border_strong().to_egui(),
    );
    let want = framed(
        &th,
        th.ports_window_width().value(),
        Some(th.ports_window_height().value()),
    );
    assert_has_frame(&sizes, want, "ports");
}

/// 원격 표본 창은 정본 RemoteAttachFrame처럼 remote-picker-width × remote-picker-height다.
#[test]
fn the_remote_attach_sample_window_is_the_remote_picker_size() {
    let th = tasty_themes::mocha_fallback();
    let sizes = frame_sizes(
        super::remote_attach::draw,
        th.bg_panel().to_egui(),
        th.border_strong().to_egui(),
    );
    let want = framed(
        &th,
        th.remote_picker_width().value(),
        Some(th.remote_picker_height().value()),
    );
    assert_has_frame(&sizes, want, "remote attach");
}
