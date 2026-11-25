//! 튜토리얼의 대상 표시·안내 말풍선·주제 목록을 보여주는 정적 예제.
//! 색과 공용 치수는 Theme를 사용하고 예제 고유의 구조 값은 이름 붙인 상수로 둔다.
//! design_token_adherence는 일부 인라인 여백을 검사하지만 이 상수들의 설계 적합성은 검사하지 않는다.
//! 따라서 상수의 역할과 중복 여부는 직접 검토해야 한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::{STRUCT_GAP_2, TUTORIAL_STEP_GAP_X};
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, margin_all, vspace};

use crate::catalog::spec::{self, StageVariant, TokenChip};
const CALLOUT_W: LogicalPx = LogicalPx(244.0);
const POPUP_W: LogicalPx = LogicalPx(360.0);
/// 중앙 topic 팝업 주위의 scrim을 보여 주는 데모 무대 높이. marker 링 크기가 아니다.
const TOPIC_STAGE_H: LogicalPx = LogicalPx(300.0);
const TAIL: LogicalPx = LogicalPx(12.0); // 12px diamond → 삼각 tail
// 네 방향 말풍선이 디자인의 가로·세로 꼬리 오프셋을 공유한다.
const TAIL_OFFSET_X: LogicalPx = LogicalPx(28.0);
const TAIL_OFFSET_Y: LogicalPx = LogicalPx(24.0);

// 링과 발광 예제의 크기를 같게 한다. 다른 역할의 값은 우연히 같아도 공유하지 않는다.
const MARKER_DEMO_W: LogicalPx = LogicalPx(260.0);
const MARKER_DEMO_H: LogicalPx = LogicalPx(150.0);
const MARKER_DEMO_INSET_L: LogicalPx = LogicalPx(34.0);
const MARKER_DEMO_INSET_T: LogicalPx = LogicalPx(38.0);
const MARKER_DEMO_INSET_R: LogicalPx = LogicalPx(16.0);
const MARKER_DEMO_INSET_B: LogicalPx = LogicalPx(22.0);
// 본체 셸의 비율을 보여주는 예제 전용 크기다. 공용 여백은 Theme 값을 사용한다.
const FAUX_SIDEBAR_W: LogicalPx = LogicalPx(116.0);
const FAUX_STATUSBAR_H: LogicalPx = LogicalPx(20.0);
const FAUX_ROW_H: LogicalPx = LogicalPx(22.0);
const FAUX_ROW_GAP: LogicalPx = LogicalPx(3.0);
const FAUX_GLYPH_INSET: LogicalPx = LogicalPx(6.0);
const FAUX_DOT_R: LogicalPx = LogicalPx(4.0);
const FAUX_LABEL_GAP: LogicalPx = LogicalPx(14.0);
const FAUX_TEXT_PAD: LogicalPx = LogicalPx(10.0);

/// 본체 basics 주제의 단계 하나. 제목·본문은 본체 번역 키로 읽어 문구 사본을 두지 않는다.
pub(crate) struct BasicsStep {
    pub(crate) id: &'static str,
    pub(crate) title_key: &'static str,
    pub(crate) body_key: &'static str,
}

macro_rules! basics_step {
    ($id:literal) => {
        BasicsStep {
            id: $id,
            title_key: concat!("tutorial.step_", $id, "_title"),
            body_key: concat!("tutorial.step_", $id, "_body"),
        }
    };
}

/// 본체 `src/adapters/ui/tutorial/catalog.rs`의 basics 주제와 같은 순서다. 갤러리는 본체 크레이트에
/// 의존하지 않으므로 목록을 여기 두고, 본체 정의와 같은지는 이 모듈의 시험이 본체 소스를 읽어 본다.
/// 진행 표시의 전체 수는 이 길이다. 마지막 summary 단계는 본체에서 마커·spotlight 없이 말풍선만 뜬다.
pub(crate) const BASICS_STEPS: [BasicsStep; 5] = [
    basics_step!("workspace"),
    basics_step!("pane"),
    basics_step!("tab"),
    basics_step!("surface"),
    basics_step!("summary"),
];

/// 합성 표본 항목의 한 줄 설명. 시험이 단계 수를 `BASICS_STEPS`와 대조한다.
pub(crate) const COMPOSITE_WHEN: &str =
    "Step 1/5 워크스페이스 — marker + spotlight dim + callout, popup closed";

/// 합성 표본의 설명. 시험이 단계 수·순서를 `BASICS_STEPS`와 대조한다.
pub(crate) const COMPOSITE_NOTE: &str = "Popup closed: marker (glow ring) + spotlight dim + \
     callout only. Step 1 wraps the whole content area (tabs + surface + status, sidebar \
     excluded); later steps move the marker pane → tab → surface: it shrinks to the pane, then \
     sits on the pane's tab header, then moves to the surface below the tab bar. The last step (summary) shows the callout alone, with no marker and no dim. Next → step 2 · \
     Back → prev · Skip/Esc → topic popup · last-step Next → reopen popup. Marker + dim are \
     pointer-events:none.";

/// 합성 표본 무대 왼쪽 위에서 첫 단계 말풍선까지의 거리. tasty-page 합성 표본의 말풍선 위치(left 150, top 120)다.
const COMPOSITE_CALLOUT_X: LogicalPx = LogicalPx(150.0);
const COMPOSITE_CALLOUT_Y: LogicalPx = LogicalPx(120.0);
/// 합성 표본 무대의 최대 폭과 높이. 폭의 바닥은 토큰 tutorial-step-specimen-min-width다.
const COMPOSITE_STAGE_MAX_W: LogicalPx = LogicalPx(520.0);
const COMPOSITE_STAGE_H: LogicalPx = LogicalPx(340.0);

/// 사이드바와 마커가 같은 경계를 쓰도록 예제의 폭 제한을 한 곳에서 계산한다.
fn faux_sidebar_w(r: egui::Rect) -> f32 {
    FAUX_SIDEBAR_W.value().min(r.width() * 0.5)
}

fn paint_faux_app(p: &egui::Painter, r: egui::Rect, theme: &Theme) {
    let sep = egui::Stroke::new(
        theme.border_width.value(),
        theme.separator.to_egui_premultiplied(),
    );
    let inset = theme.spacing_sm.value(); // 8 — 행 좌우 인셋
    let cap = theme.font_size_caption.value(); // 11 — faux 앱 라벨 폰트
    let tab_h = theme.tab_bar_height.value();
    let sidebar_w = faux_sidebar_w(r);

    let side = egui::Rect::from_min_max(r.min, egui::pos2(r.min.x + sidebar_w, r.max.y));
    p.rect_filled(side, 0.0, theme.bg_sidebar().to_egui());
    p.vline(side.max.x, side.y_range(), sep);
    let rows = [("web", true), ("api", false), ("db", false)];
    let mut ry = LogicalPx(side.min.y) + FAUX_TEXT_PAD;
    for (label, active) in rows {
        let row = egui::Rect::from_min_size(
            egui::pos2(side.min.x + inset, ry.value()),
            egui::vec2(sidebar_w - inset * 2.0, FAUX_ROW_H.value()),
        );
        if active {
            p.rect_filled(
                row,
                theme.corner_radius_sm.value(),
                theme.surface_active().to_egui(),
            );
        }
        p.circle_filled(
            egui::pos2(
                row.min.x + (FAUX_GLYPH_INSET + FAUX_DOT_R).value(),
                row.center().y,
            ),
            FAUX_DOT_R.value(),
            theme.accent_success().to_egui(),
        );
        p.text(
            egui::pos2(
                row.min.x + (FAUX_GLYPH_INSET + FAUX_LABEL_GAP).value(),
                row.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(cap),
            if active {
                theme.text_on_active().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );
        ry += FAUX_ROW_H + FAUX_ROW_GAP;
    }

    let main = egui::Rect::from_min_max(egui::pos2(side.max.x, r.min.y), r.max);
    let tabbar = egui::Rect::from_min_size(main.min, egui::vec2(main.width(), tab_h));
    p.rect_filled(tabbar, 0.0, theme.bg_panel().to_egui());
    p.hline(tabbar.x_range(), tabbar.max.y, sep);
    let term_bg = theme.surface("terminal").focused_bg.to_egui();
    let tab_pad = theme.spacing_md.value(); // 12 — 탭 좌우 패딩
    let mut tx = tabbar.min.x;
    for (i, t) in ["zsh", "logs", "vim"].iter().enumerate() {
        let tw = tab_pad
            + p.layout_no_wrap(
                t.to_string(),
                egui::FontId::proportional(cap),
                egui::Color32::WHITE,
            )
            .size()
            .x
            + tab_pad;
        let tab = egui::Rect::from_min_size(egui::pos2(tx, tabbar.min.y), egui::vec2(tw, tab_h));
        if i == 0 {
            p.rect_filled(tab, 0.0, term_bg);
        }
        p.vline(tab.max.x, tab.y_range(), sep);
        p.text(
            tab.center(),
            egui::Align2::CENTER_CENTER,
            t,
            egui::FontId::proportional(cap),
            if i == 0 {
                theme.text_primary().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );
        tx += tw;
    }

    let body = egui::Rect::from_min_max(
        egui::pos2(main.min.x, tabbar.max.y),
        egui::pos2(main.max.x, main.max.y - FAUX_STATUSBAR_H.value()),
    );
    p.rect_filled(body, 0.0, term_bg);
    p.text(
        egui::pos2(body.min.x + tab_pad, body.min.y + FAUX_TEXT_PAD.value()),
        egui::Align2::LEFT_TOP,
        "$ kubectl get pods -n prod",
        egui::FontId::monospace(cap),
        theme.text_muted().to_egui(),
    );

    let status = egui::Rect::from_min_max(egui::pos2(main.min.x, body.max.y), main.max);
    p.rect_filled(status, 0.0, theme.bg_sidebar().to_egui());
    p.hline(status.x_range(), status.min.y, sep);
}

/// 스포트라이트 scrim — 전체를 scrim-bg 로 덮는다(마커는 그 위에 밝게).
fn paint_scrim(p: &egui::Painter, r: egui::Rect, theme: &Theme) {
    p.rect_filled(r, 0.0, theme.scrim().to_egui());
}

/// 대상의 링과 선택적인 정적 발광 효과를 그린다.
fn paint_marker(p: &egui::Painter, rect: egui::Rect, theme: &Theme, glow: bool) {
    let accent = theme.accent_primary();
    let radius = theme.corner_radius.value();
    if glow {
        // 반투명 링 두 겹으로 정적인 발광 효과를 만든다.
        for (grow, alpha) in [(5.0_f32, 60u8), (2.5, 110)] {
            p.rect_stroke(
                rect.expand(grow),
                radius + grow,
                egui::Stroke::new(
                    theme.focus_ring_width.value() + grow,
                    accent.with_alpha(alpha).to_egui(),
                ),
                egui::StrokeKind::Outside,
            );
        }
    }
    p.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.focus_ring_width.value(), accent.to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// tail 방향 — 마커가 말풍선의 어느 쪽에 있는지.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tail {
    Up,
    Down,
    Left,
    Right,
}

/// 안내 말풍선 프레임을 그린 뒤 지정한 방향에 꼬리 삼각형을 덧붙인다. 꼬리까지 덮는 사각형을 돌려준다.
#[allow(clippy::too_many_arguments)]
fn callout(
    ui: &mut egui::Ui,
    theme: &Theme,
    tail: Tail,
    step: usize,
    total: usize,
    title: &str,
    body: &str,
    first: bool,
) -> egui::Rect {
    let resp = egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius_lg.value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin {
            left: theme.spacing_lg.value() as i8,
            right: theme.spacing_lg.value() as i8,
            top: theme.spacing_md.value() as i8,
            bottom: theme.spacing_md.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_width((CALLOUT_W - theme.spacing_lg.scaled(2.0)).value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.label(
                egui::RichText::new(format!("{step} / {total}"))
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .strong()
                    .color(theme.accent_primary_raised().to_egui()),
            );
            vspace(ui, theme.spacing_xs);
            ui.label(
                egui::RichText::new(title)
                    .size(theme.font_size_body.value())
                    .strong()
                    .color(theme.text_primary().to_egui()),
            );
            // 디자인의 6px 간격을 공용 토큰으로 계산한다.
            vspace(ui, theme.spacing_xs * 1.5);
            ui.label(
                egui::RichText::new(body)
                    .size(theme.font_size_caption.value())
                    .color(theme.text_secondary_raised().to_egui()),
            );
            ui.add_space(theme.spacing_md.value());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                for i in 0..total {
                    let c = if i == step - 1 {
                        theme.accent_primary().to_egui()
                    } else {
                        theme.surface_active().to_egui()
                    };
                    // 지름 5는 디자인 값이며 작은 진행 점의 비율을 유지한다.
                    let (r, _) = ui.allocate_exact_size(egui::vec2(5.0, 5.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 2.5, c);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("Next")
                        .variant(ButtonVariant::Primary)
                        .size(ControlSize::Sm)
                        .show(ui, theme);
                    if !first {
                        Button::new("Back")
                            .variant(ButtonVariant::Secondary)
                            .size(ControlSize::Sm)
                            .show(ui, theme);
                    }
                    ui.label(
                        egui::RichText::new("Skip")
                            .size(theme.font_size_caption.value())
                            .color(theme.text_muted_raised().to_egui()),
                    );
                });
            });
        });

    let tail_rect = paint_tail(ui.painter(), resp.response.rect, theme, tail);
    resp.response.rect.union(tail_rect)
}

/// 말풍선 꼬리. 본체와 맞닿는 변을 빼고 나머지 두 변에 테두리를 그린다. 꼬리 삼각형을 덮는 사각형을 돌려준다.
fn paint_tail(p: &egui::Painter, bubble: egui::Rect, theme: &Theme, tail: Tail) -> egui::Rect {
    let fill = theme.surface_raised().to_egui();
    let stroke = egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui());
    let h = TAIL / 2.0; // 삼각 높이 (돌출 길이).
    let (a, b, apex) = match tail {
        Tail::Up => {
            let cx = LogicalPx(bubble.min.x) + TAIL_OFFSET_X;
            (
                egui::pos2((cx - h).value(), bubble.min.y),
                egui::pos2((cx + h).value(), bubble.min.y),
                egui::pos2(cx.value(), bubble.min.y - h.value()),
            )
        }
        Tail::Down => {
            let cx = LogicalPx(bubble.min.x) + TAIL_OFFSET_X;
            (
                egui::pos2((cx - h).value(), bubble.max.y),
                egui::pos2((cx + h).value(), bubble.max.y),
                egui::pos2(cx.value(), bubble.max.y + h.value()),
            )
        }
        Tail::Left => {
            let cy = LogicalPx(bubble.min.y) + TAIL_OFFSET_Y;
            (
                egui::pos2(bubble.min.x, (cy - h).value()),
                egui::pos2(bubble.min.x, (cy + h).value()),
                egui::pos2(bubble.min.x - h.value(), cy.value()),
            )
        }
        Tail::Right => {
            let cy = LogicalPx(bubble.min.y) + TAIL_OFFSET_Y;
            (
                egui::pos2(bubble.max.x, (cy - h).value()),
                egui::pos2(bubble.max.x, (cy + h).value()),
                egui::pos2(bubble.max.x + h.value(), cy.value()),
            )
        }
    };
    p.add(egui::Shape::convex_polygon(
        vec![a, apex, b],
        fill,
        egui::Stroke::NONE,
    ));
    // 외곽 2변만 stroke (base 는 bubble 이 덮음).
    p.line_segment([a, apex], stroke);
    p.line_segment([apex, b], stroke);
    egui::Rect::from_points(&[a, apex, b])
}

fn topic_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    n: usize,
    title: &str,
    desc: &str,
    sel: bool,
    done: bool,
) {
    // 선택된 토픽 테두리 — accent 의 40% alpha. 대응 토큰 없음.
    const SELECTED_BORDER_ALPHA: u8 = 102;
    let border = if sel {
        theme
            .accent_primary()
            .with_alpha(SELECTED_BORDER_ALPHA)
            .to_egui()
    } else {
        egui::Color32::TRANSPARENT
    };
    let fill = if sel {
        theme.surface_active().to_egui()
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(theme.border_width.value(), border))
        .corner_radius(theme.corner_radius.value())
        // 디자인의 10px 간격을 공용 토큰으로 계산한다.
        .inner_margin(margin_all(theme.spacing_xs * 2.5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = TUTORIAL_STEP_GAP_X;
                let (cap, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                let (cap_bg, cap_fg) = if sel {
                    (
                        theme.accent_primary().to_egui(),
                        theme.text_on_accent().to_egui(),
                    )
                } else {
                    (
                        theme.surface_raised().to_egui(),
                        theme.text_muted_raised().to_egui(),
                    )
                };
                ui.painter()
                    .rect_filled(cap, theme.corner_radius_sm.value(), cap_bg);
                ui.painter().text(
                    cap.center(),
                    egui::Align2::CENTER_CENTER,
                    n.to_string(),
                    egui::FontId::monospace(theme.font_size_micro.value()),
                    cap_fg,
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
                    let (title_fg, desc_fg) = if sel {
                        (theme.text_on_active(), theme.text_on_active())
                    } else {
                        (theme.text_primary(), theme.text_muted_raised())
                    };
                    ui.label(
                        egui::RichText::new(title)
                            .size(theme.font_size_body.value())
                            .color(title_fg.to_egui()),
                    );
                    ui.label(
                        egui::RichText::new(desc)
                            .size(theme.font_size_caption.value())
                            .color(desc_fg.to_egui()),
                    );
                });
                if done {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.label(
                            egui::RichText::new("✓")
                                .size(theme.font_size_caption.value())
                                .color(
                                    if sel {
                                        theme.accent_success_on_active()
                                    } else {
                                        theme.accent_success()
                                    }
                                    .to_egui(),
                                ),
                        );
                    });
                }
            });
        });
}

/// 중앙 주제 목록 팝업은 modal 그림자를 사용한다. 트리거 옆의 callout과 구분한다.
fn topic_popup(ui: &mut egui::Ui, theme: &Theme, scaled: bool) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius_lg.value())
        .shadow(theme.shadow_modal().to_egui())
        .show(ui, |ui| {
            ui.set_width(POPUP_W.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: theme.spacing_lg.value() as i8,
                    right: theme.spacing_lg.value() as i8,
                    top: theme.spacing_md.value() as i8,
                    bottom: theme.spacing_md.value() as i8,
                })
                .show(ui, |ui| {
                    ui.set_width(POPUP_W.value());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("튜토리얼")
                                .size(theme.font_size_body.value())
                                .strong()
                                .color(theme.text_primary().to_egui()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("✕").color(theme.text_muted().to_egui()));
                        });
                    });
                });
            hsep(ui, theme);
            egui::Frame::new()
                .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
                .show(ui, |ui| {
                    ui.set_width(POPUP_W.value());
                    // 주제 목록만 스크롤한다. 본체와 같은 상한이며, 배율을 타는 행을 묶으므로 상한도 배율을 탄다.
                    // 화면 전용 치수라 DTCG 토큰이 없고 수기 Theme 접근자가 값을 가진다(디자인 결정).
                    egui::ScrollArea::vertical()
                        .max_height(theme.tutorial_topic_body_max_height().value())
                        .auto_shrink([false, true])
                        .drag_to_scroll(false)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                            topic_row(
                                ui,
                                theme,
                                1,
                                "워크스페이스 · 페인 · 탭 · 서피스",
                                "화면 구조와 두 레벨 분할. · 완료",
                                true,
                                scaled,
                            );
                            if scaled {
                                topic_row(
                                    ui,
                                    theme,
                                    2,
                                    "직접 나누고 탭 전환하기",
                                    "서피스·탭·페인 분할 실습. · 진행 중",
                                    false,
                                    false,
                                );
                                topic_row(
                                    ui,
                                    theme,
                                    3,
                                    "명령과 단축키 찾기",
                                    "팔레트와 나의 단축키. · 미시작",
                                    false,
                                    false,
                                );
                            }
                        });
                });
            hsep(ui, theme);
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: theme.spacing_lg.value() as i8,
                    right: theme.spacing_lg.value() as i8,
                    top: theme.spacing_md.value() as i8,
                    bottom: theme.spacing_md.value() as i8,
                })
                .show(ui, |ui| {
                    ui.set_width(POPUP_W.value());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Esc 닫기")
                                .family(tasty_ui_widgets::label_font_family())
                                .size(theme.font_size_micro.value())
                                .color(theme.text_muted().to_egui()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            Button::new("진행")
                                .variant(ButtonVariant::Primary)
                                .size(ControlSize::Sm)
                                .show(ui, theme);
                        });
                    });
                });
        });
}

/// 전체 폭 1px separator.
fn hsep(ui: &mut egui::Ui, theme: &Theme) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}
/// 예제 영역을 확보하고 그 안에 그린다.
fn demo_box(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: LogicalPx,
    h: LogicalPx,
    add: impl FnOnce(&egui::Painter, egui::Rect),
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w.value(), h.value()), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, theme.corner_radius.value(), theme.bg_app().to_egui());
    add(&p, rect);
    p.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );
    rect
}
pub fn draw_marker(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "ring — solid 2px", |ui| {
            demo_box(ui, theme, MARKER_DEMO_W, MARKER_DEMO_H, |p, r| {
                paint_faux_app(p, r, theme);
                let m = egui::Rect::from_min_max(
                    egui::pos2(
                        r.min.x + MARKER_DEMO_INSET_L.value(),
                        r.min.y + MARKER_DEMO_INSET_T.value(),
                    ),
                    egui::pos2(
                        r.max.x - MARKER_DEMO_INSET_R.value(),
                        r.max.y - MARKER_DEMO_INSET_B.value(),
                    ),
                );
                paint_marker(p, m, theme, false);
            });
        });
        spec::cluster(ui, theme, "glow + spotlight — default", |ui| {
            demo_box(ui, theme, MARKER_DEMO_W, MARKER_DEMO_H, |p, r| {
                paint_faux_app(p, r, theme);
                paint_scrim(p, r, theme);
                let m = egui::Rect::from_min_max(
                    egui::pos2(
                        r.min.x + MARKER_DEMO_INSET_L.value(),
                        r.min.y + MARKER_DEMO_INSET_T.value(),
                    ),
                    egui::pos2(
                        r.max.x - MARKER_DEMO_INSET_R.value(),
                        r.max.y - MARKER_DEMO_INSET_B.value(),
                    ),
                );
                paint_marker(p, m, theme, true);
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "family",
                "Modal · Popup · Toast · Banner · Modifier-hint · Marker",
            ),
            ("z-layer", "top — above all content"),
            ("pointer", "none (clicks pass through)"),
            ("ring width", "2px (focus-ring-width)"),
            ("radius", "4px (radius)"),
            ("highlight", "static glow default · pulse opt-in"),
            ("fires on", "Tools → Tutorial only"),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "ring + halo",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::without_color("focus-ring-width", "2px stroke"),
            TokenChip::new("scrim-bg", "spotlight dim", theme.scrim().to_egui()),
            TokenChip::without_color("radius", "corner"),
            TokenChip::new(
                "border-strong",
                "callout edge",
                theme.border_strong().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Marker is the 6th overlay concept — the only one with no message. A pure \
         geometric ring drawn at a rect on the top z-layer, never touching the target \
         widget's own border, pointer-events:none so clicks pass through. Spotlight scrim \
         is ON by default (user-disableable); pulse is opt-in and falls back to a static \
         ring under reduced-motion.",
    );
}
/// basics 주제의 `id` 단계를 본체 순서의 번호와 본체 문구로 그린다.
fn basics_callout(ui: &mut egui::Ui, theme: &Theme, tail: Tail, id: &str) -> egui::Rect {
    let (i, step) = BASICS_STEPS
        .iter()
        .enumerate()
        .find(|(_, s)| s.id == id)
        .expect("basics step id");
    callout(
        ui,
        theme,
        tail,
        i + 1,
        BASICS_STEPS.len(),
        crate::i18n::t(step.title_key),
        crate::i18n::t(step.body_key),
        i == 0,
    )
}
pub fn draw_callout(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, tail, id) in [
            ("tail up", Tail::Up, "pane"),
            ("tail down", Tail::Down, "tab"),
            ("tail left", Tail::Left, "surface"),
            ("tail right · first step", Tail::Right, "workspace"),
        ] {
            spec::cluster(ui, theme, label, |ui| {
                basics_callout(ui, theme, tail, id);
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("width", "244px fixed"),
            ("tail", "up · down · left · right (12px)"),
            ("progress", "2/5 label + dot rail"),
            ("buttons", "Skip (link) · Back (secondary) · Next (primary)"),
            ("first step", "Back hidden"),
            ("last step", "Next reopens topic popup"),
            (
                "placement",
                "below → above → right → left; flip + clamp 8px",
            ),
        ],
        &[
            TokenChip::new(
                "surface-raised",
                "bubble fill",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "border-strong",
                "edge + tail",
                theme.border_strong().to_egui(),
            ),
            TokenChip::without_color("shadow-modal", "floating lift"),
            TokenChip::new(
                "accent-primary",
                "step · dot · Next",
                theme.accent_primary().to_egui(),
            ),
        ],
    );

    spec::do_(
        ui,
        theme,
        "Reuse the DS Button (primary Next / secondary Back) and keep Skip a low-emphasis \
         link — the callout is guidance, not a decision surface.",
    );
}

fn topic_stage(ui: &mut egui::Ui, theme: &Theme, scaled: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(392.0, TOPIC_STAGE_H.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, theme.corner_radius.value(), theme.bg_app().to_egui());
    paint_scrim(&p, rect, theme);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
    ));
    child.set_clip_rect(rect);
    child.vertical_centered(|ui| {
        topic_popup(ui, theme, scaled);
    });
    ui.painter().rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );
}
pub fn draw_topics(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // 같은 ScrollArea의 여러 예제가 ID를 공유하지 않도록 인스턴스를 구분한다.
        spec::cluster(ui, theme, "default — one topic", |ui| {
            ui.push_id("topic_default", |ui| topic_stage(ui, theme, false));
        });
        spec::cluster(ui, theme, "scaled — scrollable + done ✓", |ui| {
            ui.push_id("topic_scaled", |ui| topic_stage(ui, theme, true));
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("type", "CenteredFocused popup + scrim"),
            ("width", "360px"),
            ("list", "max-height 200px → internal scroll"),
            (
                "row states",
                "hover · selected = surface-active + accent cap · done = ✓",
            ),
            ("footer", "Esc hint + 진행 (primary)"),
        ],
        &[
            TokenChip::new("bg-panel", "popup fill", theme.bg_panel().to_egui()),
            TokenChip::new("scrim-bg", "dim behind", theme.scrim().to_egui()),
            TokenChip::new(
                "surface-active",
                "selected row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new("accent-success", "done ✓", theme.accent_success().to_egui()),
        ],
    );
}
pub fn draw_composite(ui: &mut egui::Ui, theme: &Theme) {
    composite_specimen(ui, theme);
    spec::note(ui, theme, COMPOSITE_NOTE);
}

/// 합성 표본을 그린 결과의 사각형과 스크롤 상태. 그리기는 쓰지 않고 시험만 배치와 스크롤을 읽는다.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct CompositeLayout {
    /// 무대(앱 모형) 사각형.
    pub(super) stage: egui::Rect,
    /// 꼬리까지 덮는 말풍선 사각형.
    pub(super) bubble: egui::Rect,
    /// 가로 스크롤이 실제로 보이는 영역.
    pub(super) viewport: egui::Rect,
    /// 스크롤되는 내용 크기.
    pub(super) content: egui::Vec2,
    /// 가로 스크롤 영역의 상태 ID.
    pub(super) scroll_id: egui::Id,
}

pub(super) fn composite_specimen(ui: &mut egui::Ui, theme: &Theme) -> CompositeLayout {
    // 앞선 고정 폭 예제(주제 팝업 무대 등)가 열의 Ui를 넓혀 놓으면 가용 폭이 창에 보이는 열보다 커진다.
    // 그래서 보이는 폭은 갤러리가 정한 본문 열 폭에서 무대 틀의 안쪽 여백·테두리(바깥과 안쪽 가용 폭의
    // 차)를 뺀 값으로 정한다.
    let column = spec::body_column_width(ui);
    let outer = ui.available_width();
    let mut layout = None;
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        let frame = outer - ui.available_width();
        let viewport = LogicalPx(column - frame).max(LogicalPx(0.0));
        layout = Some(composite_scroll(ui, theme, viewport));
    });
    layout.expect("stage는 내용을 한 번 그린다")
}

/// 합성 표본 무대 폭. 열 폭을 따르되 최대 폭을 넘지 않고, 말풍선과 꼬리가 들어가는 토큰 바닥 아래로
/// 줄지 않는다.
fn composite_stage_w(available: LogicalPx, theme: &Theme) -> LogicalPx {
    available
        .min(COMPOSITE_STAGE_MAX_W)
        .max(theme.tutorial_step_specimen_min_width())
}

/// 보이는 폭 `viewport_w`가 무대 바닥보다 좁으면 무대를 가로로 스크롤한다. 스크롤바는 숨기고 더 스크롤할
/// 수 있는 가장자리에 페이드를 그린다(테마 가이드의 스크롤 표시 규칙).
fn composite_scroll(ui: &mut egui::Ui, theme: &Theme, viewport_w: LogicalPx) -> CompositeLayout {
    let w = composite_stage_w(viewport_w, theme);
    let out = egui::ScrollArea::horizontal()
        .id_salt("tutorial-composite")
        .max_width(viewport_w.value())
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .drag_to_scroll(false)
        .show(ui, |ui| composite_stage(ui, theme, w));
    // 가로로만 스크롤하는 영역의 inner_rect는 세로로 남은 가용 높이까지 잡히므로, 보이는 영역은 내용 높이로
    // 자른다. 자르지 않으면 페이드가 무대 아래 설명까지 덮는다.
    let view = egui::Rect::from_min_size(
        out.inner_rect.min,
        egui::vec2(
            out.inner_rect.width(),
            out.content_size.y.min(out.inner_rect.height()),
        ),
    );
    let (left, right) = fade_edges(
        LogicalPx(out.state.offset.x),
        LogicalPx(out.content_size.x),
        LogicalPx(view.width()),
    );
    if left {
        paint_edge_fade(ui, theme, view, FadeEdge::Left);
    }
    if right {
        paint_edge_fade(ui, theme, view, FadeEdge::Right);
    }
    let (stage, bubble) = out.inner;
    CompositeLayout {
        stage,
        bubble,
        viewport: view,
        content: out.content_size,
        scroll_id: out.id,
    }
}

/// `(왼쪽 페이드, 오른쪽 페이드)` 판정. 본체 원격 도구의 세로 목록 페이드(`scroll_list_with_fade`)와 같은
/// 규칙을 가로로 쓴다. 갤러리는 본체 크레이트에 의존하지 않아 같은 꼴을 여기 둔다.
fn fade_edges(offset: LogicalPx, content: LogicalPx, view: LogicalPx) -> (bool, bool) {
    // 끝부분의 반올림 오차로 페이드가 남지 않도록 여유를 둔다.
    const EPS: LogicalPx = LogicalPx(1.0);
    let max_offset = (content - view).max(LogicalPx(0.0));
    (offset > EPS, offset < max_offset - EPS)
}

/// 페이드를 그릴 가장자리.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FadeEdge {
    Left,
    Right,
}

/// 무대 틀 바탕색(bg-panel)에서 투명으로 이어지는 페이드. 길이는 본체 페이드와 같은 spacing-xl이고, 양쪽
/// 페이드가 겹치지 않도록 보이는 폭의 절반으로 제한한다. Color32는 premultiplied라 TRANSPARENT와 보간한다.
fn paint_edge_fade(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, edge: FadeEdge) {
    let len = theme.spacing_xl.min(LogicalPx(rect.width()) * 0.5).value();
    let (solid_x, clear_x) = match edge {
        FadeEdge::Left => (rect.left(), rect.left() + len),
        FadeEdge::Right => (rect.right(), rect.right() - len),
    };
    let solid = theme.bg_panel().to_egui();
    let clear = egui::Color32::TRANSPARENT;
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(egui::pos2(solid_x, rect.top()), solid);
    mesh.colored_vertex(egui::pos2(solid_x, rect.bottom()), solid);
    mesh.colored_vertex(egui::pos2(clear_x, rect.top()), clear);
    mesh.colored_vertex(egui::pos2(clear_x, rect.bottom()), clear);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn composite_stage(ui: &mut egui::Ui, theme: &Theme, w: LogicalPx) -> (egui::Rect, egui::Rect) {
    let stage = demo_box(ui, theme, w, COMPOSITE_STAGE_H, |p, r| {
        paint_faux_app(p, r, theme);
        paint_scrim(p, r, theme);
        // 사이드바 폭이 줄어들어도 마커가 콘텐츠 경계에 맞도록 같은 계산을 쓴다.
        let inset = theme.spacing_sm.value();
        let m = egui::Rect::from_min_max(
            egui::pos2(r.min.x + faux_sidebar_w(r) + inset, r.min.y + inset),
            // 오른쪽 10px은 그림자가 잘리지 않도록 남기는 여백이다.
            egui::pos2(r.max.x - 10.0, r.max.y - inset),
        );
        paint_marker(p, m, theme, true);
    });
    // 본체처럼 현재 단계(첫 단계) 번호와 basics 전체 단계 수를 말풍선에 넘긴다.
    let at = stage.min + egui::vec2(COMPOSITE_CALLOUT_X.value(), COMPOSITE_CALLOUT_Y.value());
    let mut child = ui
        .new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(at, stage.max - at)));
    let bubble = basics_callout(&mut child, theme, Tail::Right, BASICS_STEPS[0].id);
    (stage, bubble)
}

#[cfg(test)]
mod tests;
