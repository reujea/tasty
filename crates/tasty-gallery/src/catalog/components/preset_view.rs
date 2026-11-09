//! Tools › Presets 의 모드리스 창 PresetView — L1 범위 탭 · 프리셋 목록 · 도구줄 + 미리보기.
//! 시안 `preset_editor.jsx` 의 `PresetWindow`. 범위 탭·목록 선택·Edit 토글은 동작하고,
//! 미리보기 안의 구조 편집은 아래 "Demo-layout preview" 예제가 맡는다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, IconButton};

use super::preset_editor::{
    Kind, Scope, Surf, cell, draw_scope_body, leaf_with, pleaf, psplit, ssplit, tab,
};
use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{StageVariant, TokenChip, meta, note, stage};

// 시안 창의 전시 치수. 대응 토큰이 없다.
/// 무대 안쪽 여백 — `padding: 20`.
const STAGE_PAD: LogicalPx = LogicalPx(20.0);
/// 목록 본문 여백 — `padding: "0 6px 6px"`.
const LIST_PAD: LogicalPx = LogicalPx(6.0);
/// 창 최대 폭 — `maxWidth: 680`. 무대가 좁으면 무대 폭을 따른다.
const WIN_MAX_W: LogicalPx = LogicalPx(680.0);
/// 창 높이 — `height: 452`.
const WIN_H: LogicalPx = LogicalPx(452.0);
/// 제목줄 높이 — `height: 36`.
const TITLE_H: LogicalPx = LogicalPx(36.0);
/// L1 범위 탭 줄 높이 — `height: 40`. 탭 버튼은 아래 테두리 1 을 뺀 39.
const L1_H: LogicalPx = LogicalPx(40.0);
/// L1 탭 좌우 여백 — `padding: "0 13px"`.
const L1_TAB_PAD_X: LogicalPx = LogicalPx(13.0);
/// L1 탭 사이 — `gap: 2`.
const L1_TAB_GAP: LogicalPx = LogicalPx(2.0);
/// L1 활성 밑줄 — `borderBottom: 2px`.
const L1_UNDERLINE: LogicalPx = LogicalPx(2.0);
/// 프리셋 목록 폭 — `width: 196`.
const LIST_W: LogicalPx = LogicalPx(196.0);
/// 목록 머리줄 여백 — `padding: "8px 10px 4px"` 의 좌우 10.
const LIST_HEAD_PAD_X: LogicalPx = LogicalPx(10.0);
/// 목록 행 여백 — `padding: "7px 9px"`.
const ROW_PAD: (f32, f32) = (9.0, 7.0);
/// 목록 행 위 간격 — `marginTop: 1`.
const ROW_GAP: LogicalPx = LogicalPx(1.0);
/// 목록 행 부제 글자 — `fontSize: 10.5`.
const ROW_SUB_FONT: LogicalPx = LogicalPx(10.5);
/// 목록 머리줄 자간 — `letterSpacing: .06em`. 같은 값의 토큰이 없다.
const HEAD_TRACKING_EM: f32 = 0.06;
/// 도구줄 높이 — `height: 44`.
const TOOLBAR_H: LogicalPx = LogicalPx(44.0);
/// 편집 중 이름 입력 폭 — `width: 150`.
const NAME_INPUT_W: LogicalPx = LogicalPx(150.0);
/// 편집 중 이름 입력 높이 — `height: 26`. 공용 Input 은 input-height(28) 고정이라 직접 그린다.
const NAME_INPUT_H: LogicalPx = LogicalPx(26.0);
/// 도구줄 세로 구분선 높이 — `height: 18`.
const TOOLBAR_SEP_H: LogicalPx = LogicalPx(18.0);
/// 저장 방식 안내 앞 체크 글리프 배율 — `transform: scale(.8)`. 자리는 원래 크기를 차지한다.
const SAVED_CHECK_SCALE: f32 = 0.8;

struct PresetEntry {
    name: &'static str,
    subtitle: &'static str,
    build: fn() -> Scope,
}

/// 시안 `SCOPES` + `PRESETS`.
const SCOPES: [(&str, &[PresetEntry]); 3] = [
    (
        "Workspace",
        &[
            PresetEntry {
                name: "claude",
                subtitle: "editor · agent · logs",
                build: build_claude,
            },
            PresetEntry {
                name: "six",
                subtitle: "6 surfaces · code + ops",
                build: build_six,
            },
            PresetEntry {
                name: "dev",
                subtitle: "2 panes · shell + watch",
                build: build_dev,
            },
            PresetEntry {
                name: "review",
                subtitle: "diff + terminal",
                build: build_review,
            },
        ],
    ),
    (
        "Tab",
        &[
            PresetEntry {
                name: "split-shell",
                subtitle: "editor + run + log",
                build: build_split_shell,
            },
            PresetEntry {
                name: "single",
                subtitle: "1 surface · terminal",
                build: build_single,
            },
        ],
    ),
    (
        "Pane",
        &[
            PresetEntry {
                name: "triple",
                subtitle: "3 tabs · server/dev/notes",
                build: build_triple,
            },
            PresetEntry {
                name: "watch",
                subtitle: "2 tabs · build + test",
                build: build_watch,
            },
        ],
    ),
];

// leaf 값 요약 — 시안 `surf(kind, x)` 는 모든 leaf 에 cwd "~/tasty" · startup "" 을 기본으로
// 넣고, `FIELDS` 가 kind 마다 보일 필드(terminal: cwd·startup, editor·log: cwd, markdown: file,
// plugin:portscan: host·range)를 정한다. 빈 값의 행은 숨긴다.
const DEFAULT_CWD: &str = "~/tasty";

fn terminal(startup: Option<&'static str>) -> Surf {
    let mut rows = vec![cell("cwd", DEFAULT_CWD, true)];
    if let Some(cmd) = startup {
        rows.push(cell("startup", cmd, false));
    }
    leaf_with(Kind::Terminal, rows)
}
fn editor(cwd: &'static str) -> Surf {
    leaf_with(Kind::Editor, vec![cell("cwd", cwd, true)])
}
fn log() -> Surf {
    leaf_with(Kind::Log, vec![cell("cwd", DEFAULT_CWD, true)])
}
fn markdown(file: &'static str) -> Surf {
    leaf_with(Kind::Markdown, vec![cell("file", file, true)])
}

/// 시안 `buildWorkspace`.
fn build_claude() -> Scope {
    Scope::PaneTree(psplit(
        true,
        0.6,
        pleaf(
            vec![
                tab(
                    "edit",
                    ssplit(
                        false,
                        0.64,
                        editor("~/tasty/src"),
                        terminal(Some("cargo watch")),
                    ),
                ),
                tab("agent", editor(DEFAULT_CWD)),
            ],
            0,
        ),
        pleaf(
            vec![
                tab("preview", markdown("docs/architecture.md")),
                tab("logs", ssplit(true, 0.5, log(), terminal(Some("tail -f")))),
            ],
            0,
        ),
    ))
}
/// 시안 `buildSix`.
fn build_six() -> Scope {
    Scope::PaneTree(psplit(
        true,
        0.5,
        pleaf(
            vec![tab(
                "code",
                ssplit(
                    false,
                    0.5,
                    editor("~/tasty/src"),
                    ssplit(
                        true,
                        0.5,
                        terminal(Some("cargo watch")),
                        terminal(Some("cargo test")),
                    ),
                ),
            )],
            0,
        ),
        pleaf(
            vec![tab(
                "ops",
                ssplit(
                    false,
                    0.5,
                    ssplit(
                        true,
                        0.5,
                        log(),
                        leaf_with(
                            Kind::PortScan,
                            vec![
                                cell("host", "127.0.0.1", false),
                                cell("range", "3000-3999", false),
                            ],
                        ),
                    ),
                    markdown("docs/runbook.md"),
                ),
            )],
            0,
        ),
    ))
}
fn build_dev() -> Scope {
    Scope::PaneTree(psplit(
        true,
        0.5,
        pleaf(vec![tab("shell", terminal(None))], 0),
        pleaf(vec![tab("logs", log())], 0),
    ))
}
fn build_review() -> Scope {
    Scope::PaneTree(psplit(
        false,
        0.55,
        pleaf(vec![tab("diff", editor(DEFAULT_CWD))], 0),
        pleaf(vec![tab("run", terminal(None))], 0),
    ))
}
/// 시안 `buildTab`.
fn build_split_shell() -> Scope {
    Scope::TabFrame(ssplit(
        true,
        0.5,
        editor("~/tasty/src"),
        ssplit(false, 0.5, terminal(Some("cargo build")), log()),
    ))
}
fn build_single() -> Scope {
    Scope::TabFrame(terminal(None))
}
/// 시안 `buildPane`.
fn build_triple() -> Scope {
    Scope::PaneTree(pleaf(
        vec![
            tab("server", terminal(Some("npm start"))),
            tab("dev", ssplit(false, 0.5, terminal(Some("vite")), log())),
            tab("notes", markdown("NOTES.md")),
        ],
        0,
    ))
}
fn build_watch() -> Scope {
    Scope::PaneTree(pleaf(
        vec![tab("build", terminal(None)), tab("test", terminal(None))],
        0,
    ))
}

struct WinState {
    scope: usize,
    pick: usize,
    edit: bool,
    name_buf: String,
}

thread_local! {
    static STATE: RefCell<WinState> = const {
        RefCell::new(WinState {
            scope: 0,
            pick: 0,
            edit: false,
            name_buf: String::new(),
        })
    };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(theme.bg_app().to_egui())
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin::same(STAGE_PAD.value() as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    STATE.with(|s| window(ui, theme, &mut s.borrow_mut()));
                });
            });
    });

    meta(
        ui,
        theme,
        &[
            ("frame", "up to 680 × 452 (adaptive)"),
            ("L1 tabs", "40px, accent underline"),
            ("list", "196px, fill + 2px bar"),
            ("toolbar", "44px, Edit on the right"),
            ("preview", "flex, on bg-app"),
        ],
        &[
            TokenChip::new("bg-sidebar", "L1 + list", theme.bg_sidebar().to_egui()),
            TokenChip::new("bg-panel", "detail", theme.bg_panel().to_egui()),
            TokenChip::new(
                "surface-active",
                "selected preset",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "active tab / select bar",
                theme.accent_primary().to_egui(),
            ),
        ],
    );
    note(
        ui,
        theme,
        "This is the 2-depth idiom (L1 fixed scope tabs → growable L2 list → detail), reused from \
         Settings — not a new shell. The detail column has two fillings: the demo-layout preview \
         (below) and, while a surface is being configured, the surface settings screen (last \
         spec). Try six › Edit › select a small cell › gear.",
    );
}

fn window(ui: &mut egui::Ui, theme: &Theme, st: &mut WinState) {
    let bw = theme.border_width.value();
    let win_w = WIN_MAX_W.value().min(ui.available_width());
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(bw, theme.border_strong().to_egui()))
        .corner_radius(theme.corner_radius.value())
        .shadow(theme.shadow_modal().to_egui())
        .show(ui, |ui| {
            let w = win_w - bw * 2.0;
            let h = WIN_H.value() - bw * 2.0;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
            let title = egui::Rect::from_min_size(rect.min, egui::vec2(w, TITLE_H.value()));
            title_bar(ui, theme, title);
            let l1 = egui::Rect::from_min_size(title.left_bottom(), egui::vec2(w, L1_H.value()));
            l1_tabs(ui, theme, l1, st);
            let body = egui::Rect::from_min_max(l1.left_bottom(), rect.max);
            let list =
                egui::Rect::from_min_size(body.min, egui::vec2(LIST_W.value(), body.height()));
            preset_list(ui, theme, list, st);
            let detail = egui::Rect::from_min_max(list.right_top(), body.max);
            detail_column(ui, theme, detail, st);
        });
}

/// 아래 테두리 hairline 을 그린다.
fn bottom_rule(ui: &egui::Ui, theme: &Theme, rect: egui::Rect) {
    let bw = theme.border_width.value();
    let y = rect.bottom() - bw * 0.5;
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );
}

fn child(ui: &mut egui::Ui, rect: egui::Rect, layout: egui::Layout) -> egui::Ui {
    ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout))
}

fn glyph_painter(g: MockGlyph) -> impl Fn(&mut egui::Ui, egui::Rect, egui::Color32) {
    move |ui, rect, c| g.image(rect.height(), c).paint_at(ui, rect)
}

/// 제목줄 — layers 글리프 + "Layout presets" + 닫기.
fn title_bar(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    bottom_rule(ui, theme, rect);
    let inner = rect.shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
    let mut c = child(ui, inner, egui::Layout::left_to_right(egui::Align::Center));
    c.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let g = theme.icon_glyph_size_md.value();
    let (r, _) = c.allocate_exact_size(egui::vec2(g, g), egui::Sense::hover());
    icons::LAYERS
        .image(g, theme.text_muted().to_egui())
        .paint_at(&c, r);
    c.label(
        egui::RichText::new("Layout presets")
            .size(theme.font_size_max.value())
            .color(theme.text_primary().to_egui()),
    );
    let mut right = child(ui, inner, egui::Layout::right_to_left(egui::Align::Center));
    IconButton::new()
        .size(ControlSize::Sm)
        .show(&mut right, theme, &glyph_painter(icons::CLOSE));
}

/// L1 범위 탭 — 활성 탭은 text-primary + 2px accent 밑줄.
fn l1_tabs(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, st: &mut WinState) {
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    bottom_rule(ui, theme, rect);
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let mut x = rect.left() + theme.spacing_md.value();
    let tab_h = rect.height() - theme.border_width.value();
    for (i, (label, _)) in SCOPES.iter().enumerate() {
        let on = i == st.scope;
        let ink = if on {
            theme.text_primary().to_egui()
        } else {
            theme.text_muted().to_egui()
        };
        let g = ui.fonts(|f| f.layout_no_wrap((*label).to_owned(), font.clone(), ink));
        let tw = g.size().x + L1_TAB_PAD_X.value() * 2.0;
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(tw, tab_h));
        let resp = ui.interact(r, ui.id().with(("g_preset_l1", i)), egui::Sense::click());
        if resp.clicked() && !on {
            st.scope = i;
            st.pick = 0;
            st.edit = false;
        }
        let pos = egui::pos2(
            r.left() + L1_TAB_PAD_X.value(),
            r.center().y - g.size().y * 0.5,
        );
        ui.painter().galley(pos, g, ink);
        if on {
            let bar = egui::Rect::from_min_max(
                egui::pos2(r.left(), r.bottom() - L1_UNDERLINE.value()),
                r.right_bottom(),
            );
            ui.painter()
                .rect_filled(bar, 0.0, theme.accent_primary().to_egui());
        }
        x += tw + L1_TAB_GAP.value();
    }
}

/// 왼쪽 목록 — "N presets" 머리줄 + 새 프리셋 버튼, 행은 선택 시 surface-active + 2px 막대.
fn preset_list(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, st: &mut WinState) {
    let bw = theme.border_width.value();
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    let x = rect.right() - bw * 0.5;
    ui.painter().line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );
    let list = SCOPES[st.scope].1;
    let head = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + LIST_HEAD_PAD_X.value(),
            rect.top() + theme.spacing_sm.value(),
        ),
        egui::pos2(
            rect.right() - LIST_HEAD_PAD_X.value(),
            rect.top() + theme.spacing_sm.value() + theme.button_height_sm().value(),
        ),
    );
    let mut h = child(ui, head, egui::Layout::left_to_right(egui::Align::Center));
    let micro = theme.font_size_micro;
    let mut job = egui::text::LayoutJob::default();
    let count = if list.len() == 1 {
        "1 preset".to_owned()
    } else {
        format!("{} presets", list.len())
    };
    job.append(
        &count.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: tasty_ui_widgets::label_font(micro.value()),
            color: theme.text_muted().to_egui(),
            extra_letter_spacing: micro.value() * HEAD_TRACKING_EM,
            ..Default::default()
        },
    );
    h.label(job);
    let mut hr = child(ui, head, egui::Layout::right_to_left(egui::Align::Center));
    IconButton::new()
        .size(ControlSize::Sm)
        .show(&mut hr, theme, &glyph_painter(icons::PLUS));

    let pad6 = LIST_PAD.value();
    let rows_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad6, head.bottom() + theme.spacing_xs.value()),
        egui::pos2(rect.right() - pad6 - bw, rect.bottom() - pad6),
    );
    let mut y = rows_rect.top();
    let name_font = egui::FontId::proportional(theme.font_size_body.value());
    let sub_font = egui::FontId::monospace(ROW_SUB_FONT.value());
    for (i, p) in list.iter().enumerate() {
        let on = i == st.pick;
        let name_ink = if on {
            theme.text_on_active().to_egui()
        } else {
            theme.text_secondary().to_egui()
        };
        let text_w = rows_rect.width() - ROW_PAD.0 * 2.0;
        let name = ui.fonts(|f| f.layout_no_wrap(p.name.to_owned(), name_font.clone(), name_ink));
        let mut sub_job = egui::text::LayoutJob::simple_singleline(
            p.subtitle.to_owned(),
            sub_font.clone(),
            if on {
                theme.text_on_active()
            } else {
                theme.text_muted()
            }
            .to_egui(),
        );
        sub_job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
        let sub = ui.fonts(|f| f.layout_job(sub_job));
        y += ROW_GAP.value();
        let row_h = ROW_PAD.1 * 2.0 + name.size().y + ROW_GAP.value() + sub.size().y;
        let r = egui::Rect::from_min_size(
            egui::pos2(rows_rect.left(), y),
            egui::vec2(rows_rect.width(), row_h),
        );
        let resp = ui.interact(r, ui.id().with(("g_preset_row", i)), egui::Sense::click());
        if resp.clicked() && !on {
            st.pick = i;
            st.edit = false;
        }
        if on {
            let radius = theme.corner_radius_sm.value();
            ui.painter()
                .rect_filled(r, radius, theme.surface_active().to_egui());
            // 시안 막대는 inset box-shadow 라 행의 둥근 모서리를 따른다. 행 모양을 막대 폭으로 자른다.
            let bar = egui::Rect::from_min_size(
                r.min,
                egui::vec2(theme.listctrl_selected_bar_width().value(), r.height()),
            );
            ui.painter().with_clip_rect(bar).rect_filled(
                r,
                radius,
                theme.listctrl_selected_bar().to_egui(),
            );
        }
        let nx = r.left() + ROW_PAD.0;
        let name_h = name.size().y;
        ui.painter()
            .galley(egui::pos2(nx, r.top() + ROW_PAD.1), name, name_ink);
        ui.painter().galley(
            egui::pos2(nx, r.top() + ROW_PAD.1 + name_h + ROW_GAP.value()),
            sub,
            if on {
                theme.text_on_active()
            } else {
                theme.text_muted()
            }
            .to_egui(),
        );
        y += row_h;
    }
}

/// 오른쪽 상세 — 44 도구줄 + bg-app 위 미리보기.
fn detail_column(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, st: &mut WinState) {
    let entry = &SCOPES[st.scope].1[st.pick];
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), TOOLBAR_H.value()));
    bottom_rule(ui, theme, bar);
    let inner = bar.shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
    let mut left = child(ui, inner, egui::Layout::left_to_right(egui::Align::Center));
    left.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    if st.edit {
        name_input(&mut left, theme, &mut st.name_buf);
    } else {
        left.label(
            egui::RichText::new(entry.name)
                .size(theme.font_size_max.value())
                .color(theme.text_primary().to_egui()),
        );
        left.label(
            egui::RichText::new(entry.subtitle)
                .monospace()
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    }
    let mut right = child(ui, inner, egui::Layout::right_to_left(egui::Align::Center));
    right.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let pencil = glyph_painter(icons::EDIT);
    let check = glyph_painter(icons::CHECK);
    if st.edit {
        let done = Button::new("Done")
            .variant(ButtonVariant::Primary)
            .size(ControlSize::Sm)
            .leading_icon(&check)
            .show(&mut right, theme);
        if done.clicked() {
            st.edit = false;
        }
        // 시안: 체크 + 문구 묶음(gap space-xs) · marginRight 4 · 도구줄 gap 8 · Done.
        right.add_space(theme.spacing_xs.value());
        right.spacing_mut().item_spacing.x = theme.spacing_xs.value();
        right.label(
            egui::RichText::new(crate::i18n::t("preset.toolbar.saved"))
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        let box_ = theme.icon_glyph_size_md.value();
        let (r, _) = right.allocate_exact_size(egui::vec2(box_, box_), egui::Sense::hover());
        let g = box_ * SAVED_CHECK_SCALE;
        icons::CHECK
            .image(g, theme.accent_success_raised().to_egui())
            .paint_at(
                &right,
                egui::Rect::from_center_size(r.center(), egui::vec2(g, g)),
            );
    } else {
        let edit = Button::new("Edit")
            .variant(ButtonVariant::Secondary)
            .size(ControlSize::Sm)
            .leading_icon(&pencil)
            .show(&mut right, theme);
        if edit.clicked() {
            st.edit = true;
            st.name_buf = entry.name.to_owned();
        }
        // 시안 구분선 `margin: 0 4px` — 도구줄 gap 8 에 더해 양쪽 12.
        right.add_space(theme.spacing_xs.value());
        let (sep, _) = right.allocate_exact_size(
            egui::vec2(theme.border_width.value(), TOOLBAR_SEP_H.value()),
            egui::Sense::hover(),
        );
        right
            .painter()
            .rect_filled(sep, 0.0, theme.separator.to_egui_premultiplied());
        right.add_space(theme.spacing_xs.value());
        for g in [icons::TRASH, icons::COPY, icons::EDIT] {
            IconButton::new()
                .size(ControlSize::Sm)
                .show(&mut right, theme, &glyph_painter(g));
        }
    }

    let preview = egui::Rect::from_min_max(bar.left_bottom(), rect.max);
    ui.painter()
        .rect_filled(preview, 0.0, theme.bg_app().to_egui());
    let scope = (entry.build)();
    draw_scope_body(ui, theme, preview.shrink(theme.spacing_md.value()), &scope);
}

/// 편집 중 이름 입력 — 시안 `<input>`: 150 × 26, border-default, radius, surface-raised,
/// 13px, 좌우 8.
fn name_input(ui: &mut egui::Ui, theme: &Theme, buf: &mut String) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(NAME_INPUT_W.value(), NAME_INPUT_H.value()),
        egui::Sense::hover(),
    );
    let radius = theme.corner_radius.value();
    ui.painter()
        .rect_filled(rect, radius, theme.surface_raised().to_egui());
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let inner = rect.shrink2(egui::vec2(theme.spacing_sm.value(), 0.0));
    let mut field = child(ui, inner, egui::Layout::left_to_right(egui::Align::Center));
    field.add(
        egui::TextEdit::singleline(buf)
            .frame(false)
            .margin(egui::Margin::ZERO)
            .desired_width(inner.width())
            .font(egui::FontId::proportional(theme.font_size_body.value()))
            .text_color(theme.text_primary().to_egui()),
    );
}
