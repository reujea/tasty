//! Git 플러그인의 워크트리·변경 파일·커밋·차이 보기 예제.
//! 실제 플러그인을 실행하지 않으며 Theme 토큰으로 화면 구조를 재현한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{TagVariant, tag, tag_mono};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

// ── popup 치수 (디자인 960×640, rail 232 고정). gallery 전시용 축소 ──
const POPUP_W: LogicalPx = LogicalPx(720.0);
const POPUP_H: LogicalPx = LogicalPx(440.0);
const RAIL_W: LogicalPx = LogicalPx(232.0);
const SECTION_H: LogicalPx = LogicalPx(28.0);
const CTX_H: LogicalPx = LogicalPx(30.0);
const HEADER_H: LogicalPx = LogicalPx(44.0);
/// diff 거터 왼쪽에서 old 줄번호의 우측 정렬선까지 거리. 행 높이와 무관한 열 기하다.
const DIFF_OLD_LINE_NUMBER_RIGHT_X: LogicalPx = LogicalPx(28.0);

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        spec::cluster(
            ui,
            theme,
            "normal — rail(2-line rows) | Changes / Commits",
            |ui| {
                shell(ui, theme, false);
            },
        );
        spec::cluster(
            ui,
            theme,
            "diff — selected file swaps the bottom pane",
            |ui| {
                shell(ui, theme, true);
            },
        );
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "960×640 · bg-panel · read-only"),
            ("header", "Git + Refresh · context strip"),
            ("rail", "232px · 2-line worktree rows"),
            ("sections", "uppercase strip + count"),
            ("commits", "sky oid + refs · author · time"),
            ("diff", "bg-app well · gutter · ± tint"),
        ],
        &[
            TokenChip::new(
                "accent-info",
                "oid · refs · main · hunk",
                theme.accent_info().to_egui(),
            ),
            TokenChip::new(
                "accent-success",
                "current · added · +",
                theme.accent_success().to_egui(),
            ),
            TokenChip::new(
                "accent-warning",
                "locked · modified",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "invalid · deleted · -",
                theme.accent_danger().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "A read-only worktree overview. Header + a bg-sidebar context strip (worktree · \
         branch · HEAD oid · repo path); each pane carries an uppercase section-header strip \
         with a count. The 232px rail lists worktrees as two-line rows (name + type pill / \
         short oid + state pill, selected = surface-active + inset accent bar). The right \
         column splits Changes over Commits↔Diff; the diff is a recessed bg-app well with an \
         old/new gutter and ±-line tints. Pills are real Tags (with the sky info tone). The \
         plugin egui-mesh renders this; the mock mirrors its structure + tokens.",
    );
}

/// popup shell — header + context strip + rail | (changes / commits-or-diff).
fn shell(ui: &mut egui::Ui, theme: &Theme, show_diff: bool) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        let w = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(w, POPUP_H.value()), egui::Sense::hover());
        header(ui, theme, rect);

        let ctx_top = LogicalPx(rect.top()) + HEADER_H;
        let ctx_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), ctx_top.value()),
            egui::vec2(w, CTX_H.value()),
        );
        context_strip(ui, theme, ctx_rect);

        let body_top = ctx_top + CTX_H;
        let body = egui::Rect::from_min_max(egui::pos2(rect.left(), body_top.value()), rect.max);
        let rail = egui::Rect::from_min_max(
            body.min,
            egui::pos2(body.left() + RAIL_W.value(), body.bottom()),
        );
        let right = egui::Rect::from_min_max(
            egui::pos2(body.left() + RAIL_W.value(), body.top()),
            body.max,
        );
        vline(
            ui,
            theme,
            LogicalPx(body.left()) + RAIL_W,
            LogicalPx(body.top()),
            LogicalPx(body.bottom()),
        );

        rail_pane(ui, theme, rail);

        let half = (right.height() * 0.5).round();
        let top = egui::Rect::from_min_size(right.min, egui::vec2(right.width(), half));
        let bottom =
            egui::Rect::from_min_max(egui::pos2(right.left(), right.top() + half), right.max);
        hline(
            ui,
            theme,
            LogicalPx(right.left()),
            LogicalPx(right.right()),
            LogicalPx(right.top() + half),
        );
        changes_pane(ui, theme, top);
        if show_diff {
            diff_pane(ui, theme, bottom);
        } else {
            commits_pane(ui, theme, bottom);
        }
    });
}

fn header(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let pad = theme.spacing_md.value();
    let p = ui.painter();
    p.text(
        egui::pos2(rect.left() + pad, rect.top() + HEADER_H.scaled(0.5).value()),
        egui::Align2::LEFT_CENTER,
        "Git",
        egui::FontId::proportional(theme.font_size_max.value()),
        theme.text_primary().to_egui(),
    );
    let bw = theme.field_width_xs.value() * 0.7;
    let bh = theme.item_height_tab.value();
    let btn = egui::Rect::from_min_size(
        egui::pos2(
            rect.right() - pad - bw,
            rect.top() + (HEADER_H - LogicalPx(bh)).scaled(0.5).value(),
        ),
        egui::vec2(bw, bh),
    );
    p.rect_filled(
        btn,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
    );
    p.rect_stroke(
        btn,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    p.text(
        btn.center(),
        egui::Align2::CENTER_CENTER,
        "Refresh",
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_primary().to_egui(),
    );
    hline(
        ui,
        theme,
        LogicalPx(rect.left()),
        LogicalPx(rect.right()),
        LogicalPx(rect.top()) + HEADER_H,
    );
}

fn context_strip(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    hline(
        ui,
        theme,
        LogicalPx(rect.left()),
        LogicalPx(rect.right()),
        LogicalPx(rect.bottom()),
    );
    let pad = theme.spacing_md.value();
    let mut cui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(pad, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    mono_label(
        &mut cui,
        "tasty",
        theme.font_size_term_sm.value(),
        theme.text_primary(),
    );
    cui.label(egui::RichText::new("·").color(theme.text_disabled().to_egui()));
    mono_label(
        &mut cui,
        "conductor/mesh-b3",
        theme.font_size_term_sm.value(),
        theme.text_secondary(),
    );
    tag_mono(&mut cui, theme, "0b0b9a9d", TagVariant::Info, false);
    ui.painter().text(
        egui::pos2(rect.right() - pad, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "~/work/tasty/.worktree/wt-3",
        egui::FontId::monospace(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
}

fn rail_pane(ui: &mut egui::Ui, theme: &Theme, area: egui::Rect) {
    section_head(ui, theme, area, "WORKTREES (3)");
    let mut y = LogicalPx(area.top()) + SECTION_H;
    y = wt_row(
        ui,
        theme,
        area,
        y,
        "tasty",
        "0b0b9a9d",
        true,
        false,
        "main",
        ("current", TagVariant::Success),
    );
    y = wt_row(
        ui,
        theme,
        area,
        y,
        "feature-ui",
        "9f8e7d6c",
        false,
        false,
        "linked",
        ("locked", TagVariant::Warning),
    );
    wt_row(
        ui,
        theme,
        area,
        y,
        "stale-wt",
        "",
        false,
        true,
        "linked",
        ("invalid", TagVariant::Danger),
    );
}

/// 2줄 worktree 행 mock. 다음 y 반환.
#[allow(clippy::too_many_arguments)] // reason: 갤러리의 행을 그리는 내부 함수로 UI 문맥과 각 열의 표시값을 함께 받는다.
fn wt_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    area: egui::Rect,
    y: LogicalPx,
    name: &str,
    oid: &str,
    selected: bool,
    invalid: bool,
    kind: &str,
    state: (&str, TagVariant),
) -> LogicalPx {
    let pad_x = theme.spacing_md;
    let pad_y = theme.spacing_sm;
    let gap = theme.spacing_xs;
    let l1_h = theme.font_size_term_sm + gap;
    let l2_h = theme.font_size_caption + gap;
    let h = pad_y.scaled(2.0) + l1_h + gap + l2_h;
    let rect = egui::Rect::from_min_size(
        egui::pos2(area.left(), y.value()),
        egui::vec2(area.width(), h.value()),
    );
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                rect.min,
                egui::vec2(theme.listctrl_selected_bar_width().value(), h.value()),
            ),
            0.0,
            theme.accent_primary().to_egui(),
        );
    }
    let name_color = if invalid && !selected {
        theme.text_muted()
    } else if selected {
        theme.text_on_active()
    } else {
        theme.text_secondary()
    };
    let l1 = egui::Rect::from_min_size(
        egui::pos2(
            (LogicalPx(rect.left()) + pad_x).value(),
            (LogicalPx(rect.top()) + pad_y).value(),
        ),
        egui::vec2(
            (LogicalPx(rect.width()) - pad_x.scaled(2.0)).value(),
            l1_h.value(),
        ),
    );
    let type_variant = if kind == "main" {
        TagVariant::Info
    } else {
        TagVariant::Default
    };
    let mut t1 = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(l1)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    tag(&mut t1, theme, kind, type_variant, false);
    ui.painter().text(
        egui::pos2(l1.left(), l1.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::monospace(theme.font_size_term_sm.value()),
        name_color.to_egui(),
    );
    let l2 = egui::Rect::from_min_size(
        egui::pos2(
            (LogicalPx(rect.left()) + pad_x).value(),
            (LogicalPx(l1.max.y) + gap).value(),
        ),
        egui::vec2(
            (LogicalPx(rect.width()) - pad_x.scaled(2.0)).value(),
            l2_h.value(),
        ),
    );
    let mut t2 = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(l2)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    tag(&mut t2, theme, state.0, state.1, true);
    if !oid.is_empty() {
        ui.painter().text(
            egui::pos2(l2.left(), l2.center().y),
            egui::Align2::LEFT_CENTER,
            oid,
            egui::FontId::monospace(theme.font_size_caption.value()),
            if selected {
                theme.text_on_active()
            } else {
                theme.accent_info()
            }
            .to_egui(),
        );
    }
    hline(
        ui,
        theme,
        LogicalPx(rect.left()),
        LogicalPx(rect.right()),
        LogicalPx(rect.bottom()),
    );
    y + h
}

fn changes_pane(ui: &mut egui::Ui, theme: &Theme, area: egui::Rect) {
    section_head(ui, theme, area, "CHANGES (3)");
    let mut y = LogicalPx(area.top()) + SECTION_H;
    y = ch_row(
        ui,
        theme,
        area,
        y,
        "M",
        TagVariant::Warning,
        "crates/tasty-plugin-git-viewer/src/",
        "render.rs",
        true,
    );
    y = ch_row(
        ui,
        theme,
        area,
        y,
        "A",
        TagVariant::Success,
        "docs/design/systems/",
        "git-viewer.md",
        false,
    );
    ch_row(
        ui,
        theme,
        area,
        y,
        "D",
        TagVariant::Danger,
        "crates/tasty-plugin-git-viewer/src/",
        "view.rs",
        false,
    );
}

#[allow(clippy::too_many_arguments)] // reason: 갤러리의 행을 그리는 내부 함수로 UI 문맥과 각 열의 표시값을 함께 받는다.
fn ch_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    area: egui::Rect,
    y: LogicalPx,
    glyph: &str,
    variant: TagVariant,
    dir: &str,
    file: &str,
    selected: bool,
) -> LogicalPx {
    let pad_x = theme.spacing_md;
    let h = LogicalPx(26.0);
    let rect = egui::Rect::from_min_size(
        egui::pos2(area.left(), y.value()),
        egui::vec2(area.width(), h.value()),
    );
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                rect.min,
                egui::vec2(theme.listctrl_selected_bar_width().value(), h.value()),
            ),
            0.0,
            theme.accent_primary().to_egui(),
        );
    }
    let mut cui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(pad_x.value(), 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    tag_mono(&mut cui, theme, glyph, variant, false);
    let x = cui.cursor().left() + theme.spacing_sm.value();
    let p = ui.painter();
    let cy = rect.center().y;
    let (dir_fg, file_fg) = if selected {
        let ink = theme.text_on_active().to_egui();
        (ink, ink)
    } else {
        (theme.text_muted().to_egui(), theme.text_primary().to_egui())
    };
    let g = p.layout_no_wrap(
        dir.to_owned(),
        egui::FontId::monospace(theme.font_size_term_sm.value()),
        dir_fg,
    );
    let dw = g.rect.width();
    p.galley(egui::pos2(x, cy - g.rect.height() * 0.5), g, dir_fg);
    p.text(
        egui::pos2(x + dw, cy),
        egui::Align2::LEFT_CENTER,
        file,
        egui::FontId::monospace(theme.font_size_term_sm.value()),
        file_fg,
    );
    y + h
}

fn commits_pane(ui: &mut egui::Ui, theme: &Theme, area: egui::Rect) {
    section_head(ui, theme, area, "COMMITS (2)");
    let mut y = LogicalPx(area.top()) + SECTION_H;
    y = cm_row(
        ui,
        theme,
        area,
        y,
        "0b0b9a9d",
        &["HEAD", "main"],
        "feat(egui-mesh): git-viewer new design",
        "zilhak",
        "2026-06-30 17:23",
    );
    cm_row(
        ui,
        theme,
        area,
        y,
        "25b2908c",
        &[],
        "fix(build): mark script executable",
        "zilhak",
        "2026-06-30 16:10",
    );
}

#[allow(clippy::too_many_arguments)] // reason: 갤러리의 행을 그리는 내부 함수로 UI 문맥과 각 열의 표시값을 함께 받는다.
fn cm_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    area: egui::Rect,
    y: LogicalPx,
    oid: &str,
    refs: &[&str],
    summary: &str,
    author: &str,
    time: &str,
) -> LogicalPx {
    let pad_x = theme.spacing_md;
    let h = theme.item_height_interactive;
    let rect = egui::Rect::from_min_size(
        egui::pos2(area.left(), y.value()),
        egui::vec2(area.width(), h.value()),
    );
    let gap = theme.spacing_sm.value();
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(pad_x.value(), 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    left.spacing_mut().item_spacing.x = gap;
    mono_label(
        &mut left,
        oid,
        theme.font_size_caption.value(),
        theme.accent_info(),
    );
    for r in refs {
        tag_mono(&mut left, theme, r, TagVariant::Info, false);
    }
    let left_end = left.min_rect().right();
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(pad_x.value(), 0.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    right.spacing_mut().item_spacing.x = gap;
    mono_label(
        &mut right,
        time,
        theme.font_size_caption.value(),
        theme.text_muted(),
    );
    right.label(
        egui::RichText::new(author)
            .size(theme.font_size_term_sm.value())
            .color(theme.text_muted().to_egui()),
    );
    let right_start = right.min_rect().left();
    let sx = left_end + gap;
    let clip = egui::Rect::from_min_max(
        egui::pos2(sx, rect.top()),
        egui::pos2((right_start - gap).max(sx), rect.bottom()),
    );
    ui.painter().with_clip_rect(clip).text(
        egui::pos2(sx, rect.center().y),
        egui::Align2::LEFT_CENTER,
        summary,
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
    );
    y + h
}

fn diff_pane(ui: &mut egui::Ui, theme: &Theme, area: egui::Rect) {
    let toolbar = egui::Rect::from_min_size(
        area.min,
        egui::vec2(area.width(), theme.git_toolbar_height().value()),
    );
    ui.painter()
        .rect_filled(toolbar, 0.0, theme.bg_sidebar().to_egui());
    hline(
        ui,
        theme,
        LogicalPx(toolbar.left()),
        LogicalPx(toolbar.right()),
        LogicalPx(toolbar.bottom()),
    );
    let pad = theme.spacing_sm.value();
    let p = ui.painter();
    p.text(
        egui::pos2(toolbar.left() + pad, toolbar.center().y),
        egui::Align2::LEFT_CENTER,
        "‹ Back",
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_secondary().to_egui(),
    );
    p.text(
        egui::pos2(
            toolbar.left() + pad + theme.field_width_xs.value(),
            toolbar.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        "src/render.rs",
        egui::FontId::monospace(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
    let well = egui::Rect::from_min_max(egui::pos2(area.left(), toolbar.bottom()), area.max);
    ui.painter()
        .rect_filled(well, 0.0, theme.bg_app().to_egui());
    let mut ly = well.top() + theme.spacing_xs.value();
    diff_line(
        ui,
        theme,
        well,
        &mut ly,
        DiffKind::Hunk,
        "",
        "",
        "@@ -1,4 +1,6 @@ fn draw",
    );
    diff_line(
        ui,
        theme,
        well,
        &mut ly,
        DiffKind::Ctx,
        "1",
        "1",
        "fn draw(ui) {",
    );
    diff_line(
        ui,
        theme,
        well,
        &mut ly,
        DiffKind::Add,
        "",
        "2",
        "let strip = section();",
    );
    diff_line(
        ui,
        theme,
        well,
        &mut ly,
        DiffKind::Del,
        "2",
        "",
        "let h = old_head();",
    );
    diff_line(
        ui,
        theme,
        well,
        &mut ly,
        DiffKind::Ctx,
        "3",
        "3",
        "vbox(children)",
    );
}

enum DiffKind {
    Hunk,
    Ctx,
    Add,
    Del,
}

#[allow(clippy::too_many_arguments)] // reason: 갤러리 차이 보기의 행을 그리며 UI 문맥·좌표·표시값을 함께 받는다.
fn diff_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    well: egui::Rect,
    ly: &mut f32,
    kind: DiffKind,
    old: &str,
    new: &str,
    text: &str,
) {
    let sz = theme.font_size_caption.value();
    let h = (sz * 1.65).round();
    let rect = egui::Rect::from_min_size(egui::pos2(well.left(), *ly), egui::vec2(well.width(), h));
    // diff 줄 배경 톤. hunk 머리만 한 단계 더 옅다. 대응 토큰 없음.
    const DIFF_HUNK_BG_OPACITY: f32 = 0.09;
    const DIFF_LINE_BG_OPACITY: f32 = 0.10;
    let (fg, bg, sign) = match kind {
        DiffKind::Hunk => (
            theme.accent_info().to_egui(),
            theme
                .accent_info()
                .to_egui()
                .gamma_multiply(DIFF_HUNK_BG_OPACITY),
            "",
        ),
        DiffKind::Add => (
            theme.accent_success().to_egui(),
            theme
                .accent_success()
                .to_egui()
                .gamma_multiply(DIFF_LINE_BG_OPACITY),
            "+",
        ),
        DiffKind::Del => (
            theme.accent_danger().to_egui(),
            theme
                .accent_danger()
                .to_egui()
                .gamma_multiply(DIFF_LINE_BG_OPACITY),
            "-",
        ),
        DiffKind::Ctx => (
            theme.text_primary().to_egui(),
            egui::Color32::TRANSPARENT,
            "",
        ),
    };
    if bg != egui::Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 0.0, bg);
    }
    let p = ui.painter();
    let cy = rect.center().y;
    let gutter = theme.text_muted_raised().to_egui();
    let mono = |s| egui::FontId::monospace(s);
    p.text(
        egui::pos2(rect.left() + DIFF_OLD_LINE_NUMBER_RIGHT_X.value(), cy),
        egui::Align2::RIGHT_CENTER,
        old,
        mono(sz),
        gutter,
    );
    p.text(
        egui::pos2(rect.left() + 60.0, cy),
        egui::Align2::RIGHT_CENTER,
        new,
        mono(sz),
        gutter,
    );
    p.text(
        egui::pos2(rect.left() + 75.0, cy),
        egui::Align2::CENTER_CENTER,
        sign,
        mono(sz),
        fg,
    );
    p.text(
        egui::pos2(rect.left() + 84.0, cy),
        egui::Align2::LEFT_CENTER,
        text,
        mono(sz),
        fg,
    );
    *ly += h;
}

fn section_head(ui: &mut egui::Ui, theme: &Theme, area: egui::Rect, text: &str) {
    let rect = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), SECTION_H.value()));
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    hline(
        ui,
        theme,
        LogicalPx(rect.left()),
        LogicalPx(rect.right()),
        LogicalPx(rect.bottom()),
    );
    ui.painter().text(
        egui::pos2(rect.left() + theme.spacing_md.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::monospace(theme.font_size_micro.value()),
        theme.text_muted().to_egui(),
    );
}

fn mono_label(
    ui: &mut egui::Ui,
    text: &str,
    size: f32,
    color: tasty_type_appearance::color::HexColor,
) {
    ui.label(
        egui::RichText::new(text)
            .font(egui::FontId::monospace(size))
            .color(color.to_egui()),
    );
}

fn hline(ui: &mut egui::Ui, theme: &Theme, x0: LogicalPx, x1: LogicalPx, y: LogicalPx) {
    ui.painter().hline(
        x0.value()..=x1.value(),
        y.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

fn vline(ui: &mut egui::Ui, theme: &Theme, x: LogicalPx, y0: LogicalPx, y1: LogicalPx) {
    ui.painter().vline(
        x.value(),
        y0.value()..=y1.value(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}
