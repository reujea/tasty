//! bool 상태를 바꾸는 체크박스와 스위치. 변경하면 response.changed로 알린다.
//! 상태는 즉시 표시하며 키보드 포커스 테두리는 구현하지 않는다.

use tasty_type_appearance::theme::Theme;

use crate::control::Ground;

/// 체크마크 선 굵기. 창 버튼용 icon_stroke_width와 역할이 달라 별도로 둔다.
const CHECK_STROKE: f32 = 2.0;

/// 체크박스·간격·라벨의 전체 폭. 메뉴 등에서 필요한 폭을 먼저 계산할 때 사용한다.
pub fn checkbox_width(ui: &egui::Ui, theme: &Theme, label: &str) -> f32 {
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(theme.font_size_body.value()),
            egui::Color32::PLACEHOLDER,
        )
    });
    theme.checkbox_size().value() + theme.spacing_sm.value() + galley.rect.width()
}

/// Checkbox — 16px 박스 + 라벨. 클릭 시 토글. `ground`는 박스가 놓인 바탕이다.
pub fn checkbox(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: &str,
    enabled: bool,
    ground: Ground,
) -> egui::Response {
    let label_fg = theme.text_primary().to_egui();
    draw_checkbox(ui, theme, checked, label, enabled, None, label_fg, ground)
}

/// [`checkbox`] 와 같지만 활성 상태의 라벨 색을 호출자가 정한다.
/// 채움이 정해진 행 위에 놓일 때 쓴다. disabled 라벨은 disabled ink 다.
pub fn checkbox_with_label_color(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: &str,
    enabled: bool,
    ground: Ground,
    label_fg: egui::Color32,
) -> egui::Response {
    draw_checkbox(ui, theme, checked, label, enabled, None, label_fg, ground)
}

/// 라벨이 남은 폭을 넘으면 말줄임 대신 어디서나 줄바꿈하는 [`checkbox`]. 위젯 id 는 호출자가 정한다.
/// 같은 체크박스를 다른 배치에 다시 그려도 id 가 같으므로 키보드 포커스가 남는다.
#[allow(clippy::too_many_arguments)] // reason: checkbox의 인자에 위젯 id 하나를 더한 형태를 유지한다.
pub fn checkbox_wrapping(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: egui::Id,
    checked: &mut bool,
    label: &str,
    enabled: bool,
    ground: Ground,
) -> egui::Response {
    let label_fg = theme.text_primary().to_egui();
    draw_checkbox(
        ui,
        theme,
        checked,
        label,
        enabled,
        Some(id),
        label_fg,
        ground,
    )
}

/// `id` 가 있으면 그 id 로 반응하고 라벨을 줄바꿈한다. 없으면 자동 id 와 한 줄 말줄임이다.
#[allow(clippy::too_many_arguments)] // reason: 공개 변형 셋이 같은 그리기를 공유하는 내부 함수다.
fn draw_checkbox(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: &str,
    enabled: bool,
    id: Option<egui::Id>,
    label_fg: egui::Color32,
    ground: Ground,
) -> egui::Response {
    // gap(라벨)·body 는 대응 checkbox component 토큰 없음 → semantic.
    let gap = theme.spacing_sm.value();
    let body = theme.font_size_body.value();
    let radius = theme.checkbox_radius().value();
    let bw = theme.border_width.value();
    let box_sz = theme.checkbox_size().value();

    // 상자가 차지한 폭을 제외한 공간에 맞춰 라벨을 줄이거나 줄바꿈한다.
    let label_max = (ui.available_width() - box_sz - gap).max(0.0);
    let font = egui::FontId::proportional(body);
    let job = if id.is_some() {
        let mut job = egui::text::LayoutJob::simple(
            label.to_owned(),
            font,
            egui::Color32::PLACEHOLDER,
            label_max,
        );
        job.wrap.break_anywhere = true;
        job
    } else {
        let mut job = egui::text::LayoutJob::simple_singleline(
            label.to_owned(),
            font,
            egui::Color32::PLACEHOLDER,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(label_max);
        job
    };
    let galley = ui.fonts(|f| f.layout_job(job));
    let h = box_sz.max(galley.rect.height());
    // 빈 라벨은 상자만 차지한다(간격을 두지 않는다).
    let w = if label.is_empty() && id.is_some() {
        box_sz
    } else {
        box_sz + gap + galley.rect.width()
    };
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, mut resp) = match id {
        Some(id) => {
            let (_, rect) = ui.allocate_space(egui::vec2(w, h));
            (rect, ui.interact(rect, id, sense))
        }
        None => ui.allocate_exact_size(egui::vec2(w, h), sense),
    };
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }

    // disabled는 opacity 없이 disabled ink를 쓴다. 상자·트랙은 아래에서 disabled 상자 role로 바꾼다.
    let ink = |c: egui::Color32| {
        if enabled {
            c
        } else {
            theme.state_disabled_fg().to_egui()
        }
    };
    let box_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - box_sz * 0.5),
        egui::vec2(box_sz, box_sz),
    );
    // checked 는 accent 채움(checkbox-bg-checked)이 fill=border 를 겸한다(별도
    // checkbox-border-checked 토큰 없음). unchecked 는 checkbox-bg/-border.
    let (fill, border) = if !enabled {
        let edge = match ground {
            Ground::Panel => theme.state_disabled_border(),
            Ground::Raised => theme.border_strong(),
            Ground::Active => theme.border_frame(),
        };
        (theme.state_disabled_fill().to_egui(), edge.to_egui())
    } else if *checked {
        (
            theme.checkbox_bg_checked().to_egui(),
            theme.checkbox_bg_checked().to_egui(),
        )
    } else {
        let edge = match ground {
            Ground::Panel => theme.checkbox_border(),
            Ground::Raised => theme.border_control_raised(),
            Ground::Active => theme.text_on_active(),
        };
        (theme.checkbox_bg().to_egui(), edge.to_egui())
    };
    ui.painter().rect(
        box_rect,
        radius,
        fill,
        egui::Stroke::new(bw, border),
        egui::StrokeKind::Inside,
    );
    if *checked {
        // 체크마크 — box 중앙 아이콘 가족 xs(12) 영역에 꺾은선 2 segment.
        let glyph = theme.icon_glyph_size_xs.value();
        let o = box_rect.center() - egui::vec2(glyph, glyph) * 0.5;
        let p = |fx: f32, fy: f32| o + egui::vec2(glyph * fx, glyph * fy);
        let stroke = egui::Stroke::new(CHECK_STROKE, ink(theme.checkbox_check_fg().to_egui()));
        ui.painter()
            .line_segment([p(0.22, 0.55), p(0.42, 0.74)], stroke);
        ui.painter()
            .line_segment([p(0.42, 0.74), p(0.80, 0.30)], stroke);
    }
    let label_pos = egui::pos2(
        rect.left() + box_sz + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(label_pos, galley, ink(label_fg));
    resp
}

/// Switch — 28×16 토글 트랙 + 라벨(옵션). 클릭 시 토글.
pub fn switch(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: Option<&str>,
    enabled: bool,
) -> egui::Response {
    let label_color = theme.text_primary().to_egui();
    switch_with_label_color(ui, theme, checked, label, enabled, label_color)
}

/// [`switch`] 와 같고 raised 바탕의 꺼진 트랙과 손잡이를 쓴다.
pub fn switch_on_raised(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: Option<&str>,
    enabled: bool,
) -> egui::Response {
    let label_color = theme.text_primary().to_egui();
    draw_switch(ui, theme, checked, label, enabled, label_color, true)
}

/// [`switch`] 와 같지만 활성 상태의 라벨 색을 호출자가 정한다. 트랙과 라벨이 한 클릭 영역이라
/// 키보드 초점도 한 칸이다. disabled 라벨은 다른 컨트롤과 같은 disabled ink 를 쓴다.
pub fn switch_with_label_color(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: Option<&str>,
    enabled: bool,
    label_color: egui::Color32,
) -> egui::Response {
    draw_switch(ui, theme, checked, label, enabled, label_color, false)
}

fn draw_switch(
    ui: &mut egui::Ui,
    theme: &Theme,
    checked: &mut bool,
    label: Option<&str>,
    enabled: bool,
    label_color: egui::Color32,
    on_raised: bool,
) -> egui::Response {
    // gap(라벨)·body 는 대응 switch component 토큰 없음 → semantic.
    let gap = theme.spacing_sm.value();
    let body = theme.font_size_body.value();
    let bw = theme.border_width.value();
    let track_w = theme.switch_track_width().value();
    let track_h = theme.switch_track_height().value();
    let thumb_sz = theme.switch_thumb_size().value();
    let thumb_inset = theme.switch_thumb_inset().value();

    let galley = label.map(|l| {
        ui.painter().layout_no_wrap(
            l.to_owned(),
            egui::FontId::proportional(body),
            egui::Color32::PLACEHOLDER,
        )
    });
    let label_w = galley.as_ref().map(|g| gap + g.rect.width()).unwrap_or(0.0);
    let h = track_h.max(galley.as_ref().map(|g| g.rect.height()).unwrap_or(0.0));
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(track_w + label_w, h), sense);
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }

    // disabled는 opacity 없이 disabled ink를 쓴다. 상자·트랙은 아래에서 disabled 상자 role로 바꾼다.
    let ink = |c: egui::Color32| {
        if enabled {
            c
        } else {
            theme.state_disabled_fg().to_egui()
        }
    };
    let track = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - track_h * 0.5),
        egui::vec2(track_w, track_h),
    );
    // 트랙은 채움이 곧 테두리다. 별도의 테두리 색이 없다.
    let (track_fill, track_border) = if !enabled {
        (
            theme.state_disabled_fill().to_egui(),
            theme.state_disabled_border().to_egui(),
        )
    } else if *checked {
        (
            theme.switch_track_bg_on().to_egui(),
            theme.switch_track_bg_on().to_egui(),
        )
    } else {
        let off = if on_raised {
            theme.switch_track_raised_bg()
        } else {
            theme.switch_track_bg()
        };
        (off.to_egui(), off.to_egui())
    };
    // 완전히 둥근 트랙이 되도록 높이의 절반을 반경으로 사용한다.
    ui.painter().rect(
        track,
        track_h * 0.5,
        track_fill,
        egui::Stroke::new(bw, track_border),
        egui::StrokeKind::Inside,
    );
    let thumb_x = if *checked {
        track.right() - thumb_inset - thumb_sz * 0.5
    } else {
        track.left() + thumb_inset + thumb_sz * 0.5
    };
    let thumb_color = if *checked {
        theme.switch_thumb_bg_on().to_egui()
    } else if on_raised {
        theme.switch_thumb_raised_bg().to_egui()
    } else {
        theme.switch_thumb_bg().to_egui()
    };
    ui.painter().circle_filled(
        egui::pos2(thumb_x, track.center().y),
        thumb_sz * 0.5,
        ink(thumb_color),
    );

    if let Some(g) = galley {
        let pos = egui::pos2(track.right() + gap, rect.center().y - g.rect.height() * 0.5);
        ui.painter().galley(pos, g, ink(label_color));
    }
    resp
}

#[cfg(test)]
mod ground_tests {
    use super::*;

    /// 해제 상자의 테두리 색을 그려서 읽는다.
    fn edge(theme: &Theme, enabled: bool, ground: Ground) -> egui::Color32 {
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                checkbox(ui, theme, &mut false, "", enabled, ground);
            });
        });
        out.shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.stroke.width > 0.0 => Some(r.stroke.color),
                _ => None,
            })
            .next_back()
            .expect("box stroke")
    }

    #[test]
    fn the_box_edge_follows_the_host_ground() {
        for light in [false, true] {
            let th = Theme::with_colors(tasty_themes::mocha_fallback_colors(), light);
            for (ground, off, disabled) in [
                (
                    Ground::Panel,
                    th.checkbox_border(),
                    th.state_disabled_border(),
                ),
                (
                    Ground::Raised,
                    th.border_control_raised(),
                    th.border_strong(),
                ),
                (Ground::Active, th.text_on_active(), th.border_frame()),
            ] {
                assert_eq!(edge(&th, true, ground), off.to_egui());
                assert_eq!(edge(&th, false, ground), disabled.to_egui());
            }
        }
    }
}
