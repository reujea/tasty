//! 편집 가능한 경로 필드와 탐색기 보기 전환 예제. 공용 PathField를 사용한다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{PathField, PathFieldPending};

use super::glyph;
use crate::catalog::icons::{LAYOUT_DETAIL, LAYOUT_GRID, LIST, MockGlyph};
use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, note, stage};
use crate::i18n::t;

/// 최근 디렉토리 후보(design ExpToolbar `PathField candidates`).
const EXP_RECENT: &[&str] = &[
    "~/Downloads",
    "~/work/tasty",
    "~/work/tasty/crates/tasty-ui-widgets",
    "~/.config/tasty",
];

/// 라이브 PathField 편집 상태(호출측 소유 계약).
struct AddrState {
    buf: String,
    editing: bool,
    active: Option<usize>,
}

thread_local! {
    static SEG_SEL: RefCell<usize> = const { RefCell::new(2) }; // detail 기본 활성
    static ADDR: RefCell<Option<AddrState>> = const { RefCell::new(None) };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let folder_icon = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        glyph::FOLDER_OPEN
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };
    let go_icon = |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
        glyph::ARROW_RIGHT
            .image(rect.height(), c)
            .paint_at(ui, rect);
    };

    stage(ui, theme, StageVariant::Tight, |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_panel()))
            .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
            .show(ui, |ui| {
                ui.set_width(theme.measure_md.value());
                ADDR.with(|s| {
                    let mut slot = s.borrow_mut();
                    let st = slot.get_or_insert_with(|| AddrState {
                        buf: "~/Downloads".to_string(),
                        editing: false,
                        active: None,
                    });
                    PathField::new("gallery_exp_addr")
                        .placeholder(t("explorer.address.placeholder"))
                        .empty_label(t("explorer.address.empty"))
                        .leading_icon(&folder_icon)
                        .row_icon(&folder_icon)
                        .go_icon(&go_icon)
                        .show(
                            ui,
                            theme,
                            &mut st.buf,
                            &mut st.editing,
                            &mut st.active,
                            EXP_RECENT,
                            "~/Downloads",
                        );
                });
            });
    });

    // 원격 확인 대기: 입력한 경로와 포커스 테두리를 두고, 지연 뒤에는 뒤 칸에 Spinner 를 보인다.
    for (label, spinner) in [
        ("remote check — first 200 ms (no spinner)", None),
        (
            "remote check — after 200 ms (spinner)",
            Some(theme.explorer_address_pending_size().value()),
        ),
    ] {
        cluster(ui, theme, label, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from(theme.bg_panel()))
                .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
                .show(ui, |ui| {
                    ui.set_width(theme.measure_md.value());
                    let typed = "/srv/app/logs/2026-10-10.log";
                    let mut buf = typed.to_string();
                    let (mut editing, mut active) = (false, None);
                    PathField::new(&format!("gallery_exp_addr_pending_{label}"))
                        .placeholder(t("explorer.address.placeholder"))
                        .leading_icon(&folder_icon)
                        .go_icon(&go_icon)
                        .pending(Some(PathFieldPending {
                            spinner,
                            tooltip: t("explorer.address.checking"),
                        }))
                        .show(
                            ui,
                            theme,
                            &mut buf,
                            &mut editing,
                            &mut active,
                            &[],
                            "~/Downloads",
                        );
                });
        });
    }

    cluster(ui, theme, "view-mode toggle (grid / list / detail)", |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_panel()))
            .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
            .show(ui, |ui| {
                SEG_SEL.with(|s| {
                    let mut sel = s.borrow_mut();
                    if let Some(i) = seg_toggle(ui, theme, *sel, None) {
                        *sel = i;
                    }
                });
            });
    });

    meta(
        ui,
        theme,
        &[
            ("field", "PathField — input-bg + input-border(-focus)"),
            ("leading", "folderOpen (input-icon-fg)"),
            ("trailing", "Go IconButton (sm) — arrow-right"),
            ("keys", "Enter/Go navigate · ↑/↓ active · Esc revert"),
            (
                "remote check",
                "typed path + focus border kept · spinner (explorer-address-pending-size 12, spinner-indicator) after explorer-address-pending-delay 200 ms · Esc cancels · 8 s → typed path",
            ),
            ("toggle", "3 icons · active segtoggle-on-bg"),
            ("height", "28 (control-height-interactive)"),
        ],
        &[
            TokenChip::new(
                "input-bg",
                "field fill",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::new(
                "text-secondary",
                "idle path",
                egui::Color32::from(theme.text_secondary_raised()),
            ),
            TokenChip::new(
                "segtoggle-on-bg",
                "selected segment",
                egui::Color32::from(theme.segtoggle_on_bg()),
            ),
            TokenChip::new(
                "segtoggle-on-fg",
                "selected segment glyph",
                egui::Color32::from(theme.segtoggle_on_fg()),
            ),
            TokenChip::new(
                "text-muted",
                "folder / inactive",
                egui::Color32::from(theme.text_muted_raised()),
            ),
        ],
    );

    note(
        ui,
        theme,
        "Click the shared PathField to edit the path. Enter or Go commits the path; recent directories appear in AutoComplete. The gallery updates its local path value without opening a directory. The grid, list and detail toggle shows the selected mode.",
    );
}

/// 토글의 총 폭. 본체 `seg_toggle_width` 와 같은 식(pad·세 칸·두 간격·양쪽 테두리)이다.
pub(super) fn seg_toggle_width(theme: &Theme) -> f32 {
    let pad = theme.spacing_xs.value();
    let gap = theme.spacing_xs.value();
    let seg_w = theme.icon_glyph_size_md.value() + theme.spacing_sm.value();
    pad * 2.0 + seg_w * 3.0 + gap * 2.0 + theme.border_width.value() * 2.0
}

/// grid/list/detail 아이콘 토글. 클릭된 세그먼트 index 반환(없으면 None).
/// `hover` 는 포인터와 관계없이 hover 배경을 칠할 칸이다(정적 예제용).
pub(super) fn seg_toggle(
    ui: &mut egui::Ui,
    theme: &Theme,
    selected: usize,
    hover: Option<usize>,
) -> Option<usize> {
    let pad = theme.spacing_xs.value();
    let gap = theme.spacing_xs.value();
    let h = theme.item_height_interactive.value();
    let seg_w = theme.icon_glyph_size_md.value() + theme.spacing_sm.value();
    let icon = theme.icon_glyph_size_md.value();
    let total_w = seg_toggle_width(theme);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, h), egui::Sense::hover());
    ui.painter().rect(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.surface_raised()),
        egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_default()),
        ),
        egui::StrokeKind::Inside,
    );
    let glyphs: [MockGlyph; 3] = [LAYOUT_GRID, LIST, LAYOUT_DETAIL];
    let seg_h = h - pad * 2.0;
    let mut sx = rect.min.x + theme.border_width.value() + pad;
    let mut clicked = None;
    for (i, g) in glyphs.into_iter().enumerate() {
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(sx, rect.center().y - seg_h / 2.0),
            egui::vec2(seg_w, seg_h),
        );
        let resp = ui.interact(seg_rect, ui.id().with(("gal_seg", i)), egui::Sense::click());
        let active = i == selected;
        if active {
            ui.painter().rect_filled(
                seg_rect,
                theme.corner_radius_sm.value(),
                egui::Color32::from(theme.segtoggle_on_bg()),
            );
        } else if resp.hovered() || hover == Some(i) {
            ui.painter().rect_filled(
                seg_rect,
                theme.corner_radius_sm.value(),
                theme.overlay_hover().to_egui_premultiplied(),
            );
        }
        let fg = if active {
            egui::Color32::from(theme.segtoggle_on_fg())
        } else {
            egui::Color32::from(theme.text_muted_raised())
        };
        let ir = egui::Rect::from_center_size(seg_rect.center(), egui::vec2(icon, icon));
        g.image(icon, fg).paint_at(ui, ir);
        if resp.clicked() {
            clicked = Some(i);
        }
        sx += seg_w + gap;
    }
    clicked
}
