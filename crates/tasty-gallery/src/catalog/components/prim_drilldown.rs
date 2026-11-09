//! 목록에서 항목을 고르면 전체 영역을 상세로 바꾸는 DrillDown 예제.
//! 뒤로 가기로 목록에 돌아오며 전환 애니메이션은 사용하지 않는다.
//! 시안 `layouts.jsx` 의 `DrillDownDemo` — 프리셋 목록 → 키 비교 표, 바깥 모달 푸터.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, DrillDown, DrillDownView, ListCtrl, ListCtrlItem,
    TagVariant, tag,
};

use crate::catalog::spec::{StageVariant, TokenChip, do_, dont, meta, stage};

// 시안 데모 상자의 전시 치수. 대응 토큰이 없다.
/// 데모 상자 폭 — `width: 560`.
const DEMO_W: LogicalPx = LogicalPx(560.0);
/// 데모 상자 높이 — `height: 320`. 푸터를 뺀 나머지가 DrillDown 영역이다.
const DEMO_H: LogicalPx = LogicalPx(320.0);
/// 비교 표 첫 열 비율 — `minmax(0,1.6fr) 1fr 1fr`.
const ACTION_COL_WEIGHT: f32 = 1.6;
/// 비교 표 칸의 세로 여백 — 머리줄 `padding: 0 12px 6px`, 칸 `padding: 6px 12px`.
const CELL_PAD_Y: LogicalPx = LogicalPx(6.0);

struct DemoState {
    view: DrillDownView,
    /// 상세로 들어간 프리셋.
    sel: usize,
    /// 현재 적용된 프리셋. 시안 기본값은 "default".
    active: usize,
}

thread_local! {
    static STATE: RefCell<DemoState> = const {
        RefCell::new(DemoState {
            view: DrillDownView::List,
            sel: 0,
            active: 0,
        })
    };
}

/// 데모 프리셋 — 이름 + 설명 + (동작, 현재 키, 프리셋 키).
struct Preset {
    name: &'static str,
    title: &'static str,
    desc: &'static str,
    rows: [(&'static str, &'static str, &'static str); 4],
}

/// 시안 `DD_PRESETS`.
const PRESETS: [Preset; 3] = [
    Preset {
        name: "Default",
        title: "Default preset",
        desc: "Tasty stock bindings",
        rows: [
            ("Copy", "Ctrl+Shift+C", "Ctrl+Shift+C"),
            ("Paste", "Ctrl+Shift+V", "Ctrl+Shift+V"),
            ("New tab", "Ctrl+T", "Ctrl+T"),
            ("Find", "Ctrl+F", "Ctrl+F"),
        ],
    },
    Preset {
        name: "Mac",
        title: "Mac preset",
        desc: "⌘-based, native-app muscle memory",
        rows: [
            ("Copy", "Ctrl+Shift+C", "⌘C"),
            ("Paste", "Ctrl+Shift+V", "⌘V"),
            ("New tab", "Ctrl+T", "⌘T"),
            ("Find", "Ctrl+F", "⌘F"),
        ],
    },
    Preset {
        name: "Vim",
        title: "Vim preset",
        desc: "modal, hjkl pane motions",
        rows: [
            ("Copy", "Ctrl+Shift+C", "y"),
            ("Paste", "Ctrl+Shift+V", "p"),
            ("New tab", "Ctrl+T", ":tabnew"),
            ("Find", "Ctrl+F", "/"),
        ],
    },
];

/// DrillDown — list ⇄ detail 교체 · back bar(← + 제목 + Apply) · 내부 스크롤 · 모달 푸터.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| demo(ui, theme));
            });
    });

    meta(
        ui,
        theme,
        &[
            ("views", "list (full width) ⇄ detail (full width)"),
            (
                "back bar",
                "36px, drilldown-backbar-height · bottom hairline",
            ),
            ("back button", "ghost IconButton · chevronLeft"),
            ("title", "detail subject, next to ←"),
            ("actions slot", "right of the back bar — holds Apply"),
            ("transition", "instant (reduced-motion aware opt-in fade)"),
            ("scroll", "detail body scrolls; back bar pinned"),
        ],
        &[
            TokenChip::new(
                "drilldown-backbar-border",
                "back-bar bottom rule",
                theme.drilldown_backbar_border().to_egui_premultiplied(),
            ),
            TokenChip::new(
                "drilldown-title-fg",
                "detail title",
                egui::Color32::from(theme.drilldown_title_fg()),
            ),
            TokenChip::new(
                "surface-active",
                "selected list row",
                egui::Color32::from(theme.surface_active()),
            ),
            TokenChip::new(
                "accent-primary",
                "Apply / active bar",
                egui::Color32::from(theme.accent_primary()),
            ),
        ],
    );
    do_(
        ui,
        theme,
        "Do put the detail's own action (Apply) in the back-bar actions slot — it stays clear of \
         a surrounding modal footer's Cancel / Save, so the two button rows never collide. Apply \
         is disabled for the already-active item (nothing to change).",
    );
    dont(
        ui,
        theme,
        "Don't reach for drill-down when items are few and detail is small — a plain 1-depth \
         list→detail split shows both at once. Swap only when the detail (or the list) genuinely \
         wants the whole width.",
    );
}

/// 560×320 상자 — DrillDown 영역 + 바깥 모달 푸터(Cancel / Save).
fn demo(ui: &mut egui::Ui, theme: &Theme) {
    let bw = theme.border_width.value();
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(bw, theme.border_strong().to_egui()))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            let inner_w = DEMO_W.value() - bw * 2.0;
            ui.set_width(inner_w);
            ui.spacing_mut().item_spacing.y = 0.0;
            // 푸터 = 위 hairline + 상하 space-sm + sm 버튼.
            let footer_h = bw + theme.spacing_sm.value() * 2.0 + theme.button_height_sm().value();
            let body_h = DEMO_H.value() - bw * 2.0 - footer_h;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // 스크롤 페이지 안에서는 남은 가용 높이가 작을 수 있어 영역 높이를 직접 준다.
                ui.allocate_ui_with_layout(
                    egui::vec2(inner_w, body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_height(body_h);
                        STATE.with(|s| drill(ui, theme, &mut s.borrow_mut(), inner_w, body_h));
                    },
                );
                footer(ui, theme, inner_w);
            });
        });
}

fn drill(ui: &mut egui::Ui, theme: &Theme, st: &mut DemoState, w: f32, h: f32) {
    let preset = &PRESETS[st.sel];
    let is_active_sel = st.sel == st.active;
    let mut clicked_row = None;
    let active = st.active;
    let apply = |ui: &mut egui::Ui, th: &Theme| {
        let label = if is_active_sel { "Applied" } else { "Apply" };
        let resp = Button::new(label)
            .variant(ButtonVariant::Primary)
            .size(ControlSize::Sm)
            .enabled(!is_active_sel)
            .show(ui, th);
        if resp.clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("g_drilldown_apply"), true));
        }
    };
    let out = DrillDown::new("prim_drilldown")
        .view(st.view)
        .title(preset.title)
        .back_label("Back")
        .width(w)
        .height(h)
        .show(
            ui,
            theme,
            |ui, th| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(
                        th.spacing_lg.value() as i8,
                        th.spacing_md.value() as i8,
                    ))
                    .show(ui, |ui| {
                        let active_tag = |ui: &mut egui::Ui, th: &Theme| {
                            tag(ui, th, "Active", TagVariant::Success, true);
                        };
                        let items: Vec<ListCtrlItem<'_>> = PRESETS
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                let item = ListCtrlItem::new(p.name).description(p.desc);
                                if i == active {
                                    item.trailing(&active_tag)
                                } else {
                                    item
                                }
                            })
                            .collect();
                        let out = ListCtrl::new().show(ui, th, &items, Some(active));
                        clicked_row = out.clicked;
                    });
            },
            |ui, th| diff_table(ui, th, preset),
            Some(&apply),
        );
    // actions 슬롯은 `&dyn Fn` 이라 클릭을 temp data 로 꺼낸다.
    let apply_clicked = ui.ctx().data_mut(|d| {
        d.remove_temp::<bool>(egui::Id::new("g_drilldown_apply"))
            .unwrap_or(false)
    });
    if let Some(i) = clicked_row {
        st.sel = i;
        st.view = DrillDownView::Detail;
    }
    if apply_clicked {
        st.active = st.sel;
    }
    if out.back_clicked {
        st.view = DrillDownView::List;
    }
}

/// 상세 — 동작 · 현재 키 · 프리셋 키 비교 표. 달라진 프리셋 값은 text-primary.
/// 시안의 semibold(600)는 갤러리 폰트에 굵은 꼴이 없어 색으로만 구분한다.
fn diff_table(ui: &mut egui::Ui, th: &Theme, preset: &Preset) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(th.spacing_lg.value() as i8))
        .show(ui, |ui| {
            let w = ui.available_width();
            let unit = w / (ACTION_COL_WEIGHT + 2.0);
            let cols = [unit * ACTION_COL_WEIGHT, unit, unit];
            let pad_x = th.spacing_md.value();
            let head = |text: &str| -> egui::WidgetText {
                let mut job = egui::text::LayoutJob::default();
                let font = tasty_ui_widgets::label_font(th.font_size_micro.value());
                job.append(
                    &text.to_uppercase(),
                    0.0,
                    egui::TextFormat {
                        font_id: font,
                        color: th.text_muted().to_egui(),
                        extra_letter_spacing: th
                            .sidebar_section_heading_tracking(th.font_size_micro)
                            .value(),
                        ..Default::default()
                    },
                );
                job.into()
            };
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            // 머리줄은 위 여백 없이 아래 6 + hairline.
            row(
                ui,
                th,
                &cols,
                pad_x,
                0.0,
                CELL_PAD_Y.value(),
                [head("Action"), head("Current"), head(preset.name)],
            );
            for (action, cur, next) in preset.rows {
                let changed = cur != next;
                let mono = egui::FontId::monospace(th.font_size_term_sm.value());
                // 현재 조합 열은 muted caption, 바뀔 조합 열은 12 다.
                let cur_font = egui::FontId::monospace(th.font_size_caption.value());
                let cells: [egui::WidgetText; 3] = [
                    egui::RichText::new(action)
                        .size(th.font_size_body.value())
                        .color(th.text_secondary().to_egui())
                        .into(),
                    egui::RichText::new(cur)
                        .font(cur_font)
                        .color(th.text_muted().to_egui())
                        .into(),
                    egui::RichText::new(next)
                        .font(mono)
                        .color(if changed {
                            th.text_primary().to_egui()
                        } else {
                            th.text_muted().to_egui()
                        })
                        .into(),
                ];
                let pad_y = CELL_PAD_Y.value();
                row(ui, th, &cols, pad_x, pad_y, pad_y, cells);
            }
        });
}

/// 표 한 줄 — 칸마다 좌우 12 여백, 아래 separator hairline.
fn row(
    ui: &mut egui::Ui,
    th: &Theme,
    cols: &[f32; 3],
    pad_x: f32,
    pad_top: f32,
    pad_bottom: f32,
    cells: [egui::WidgetText; 3],
) {
    let galleys: Vec<_> = cells
        .into_iter()
        .zip(cols.iter())
        .map(|(c, w)| {
            c.into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                (w - pad_x * 2.0).max(0.0),
                egui::TextStyle::Body,
            )
        })
        .collect();
    let text_h = galleys.iter().map(|g| g.size().y).fold(0.0_f32, f32::max);
    let bw = th.border_width.value();
    let total_w: f32 = cols.iter().sum();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(total_w, pad_top + text_h + pad_bottom + bw),
        egui::Sense::hover(),
    );
    let mut x = rect.left();
    for (g, w) in galleys.into_iter().zip(cols.iter()) {
        let pos = egui::pos2(
            x + pad_x,
            rect.top() + pad_top + (text_h - g.size().y) * 0.5,
        );
        ui.painter().galley(pos, g, th.text_primary().to_egui());
        x += w;
    }
    let y = rect.bottom() - bw * 0.5;
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(bw, th.separator.to_egui_premultiplied()),
    );
}

/// 바깥 모달 푸터 — Apply 는 back bar 에 있어 이 줄과 겹치지 않는다.
fn footer(ui: &mut egui::Ui, theme: &Theme, w: f32) {
    let bw = theme.border_width.value();
    let (line, _) = ui.allocate_exact_size(egui::vec2(w, bw), egui::Sense::hover());
    ui.painter()
        .rect_filled(line, 0.0, theme.separator.to_egui_premultiplied());
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_md.value() as i8,
            theme.spacing_sm.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(w - theme.spacing_md.value() * 2.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                Button::new("Save")
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
                Button::new("Cancel")
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
            });
        });
}
