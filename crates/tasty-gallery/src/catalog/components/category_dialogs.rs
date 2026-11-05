//! 카테고리 생성·이름 변경·삭제 확인과 접힌 사이드바 팝업의 정적 예제.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, MenuItemVariant, menu_item, menu_separator, paint_rail_category_line,
};

use crate::catalog::icons::{CHEVRON_DOWN, EDIT, MockGlyph, PLUS, TRASH};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

const DELETE_WIDTH: LogicalPx = LogicalPx(380.0);
const POPUP_WIDTH: LogicalPx = LogicalPx(176.0);

/// 생성/이름변경 다이얼로그 — `error` 가 `Some` 이면 검증 에러 상태(확인 비활성).
/// 카테고리 만들기·이름 변경 팝업. 본체와 같은 공용 rename view 다.
fn edit_dialog(ui: &mut egui::Ui, theme: &Theme, value: &str, error: Option<&str>) {
    super::rename_popup::card(
        ui,
        theme,
        super::rename_popup::Card {
            layout: tasty_ui_widgets::RenamePopupLayout::Dialog,
            title: crate::i18n::t("rename_dialog.new_category_heading"),
            save: crate::i18n::t("button.save"),
            name: value,
            error,
            waiting: None,
        },
    );
}

fn delete_confirm(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, DELETE_WIDTH, kit::panel_fill(theme), |ui| {
        kit::region_sym(ui, theme.spacing_md, theme.spacing_md, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            ui.horizontal(|ui| {
                kit::icon(
                    ui,
                    TRASH,
                    theme.icon_glyph_size_md,
                    theme.accent_danger().to_egui(),
                );
                kit::title(ui, theme, "Delete category?");
            });
            kit::body(
                ui,
                theme,
                "Delete Services? Its 3 workspaces aren't deleted — they move back to Workspaces.",
            );
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("Delete category")
                        .variant(ButtonVariant::Danger)
                        .show(ui, theme);
                    Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                });
            });
        });
    });
}

/// 시안 `RailCategoryFrame` 치수. 레일 52 × 380, 위아래 안쪽 여백 10, 항목 간격 4.
const RAIL_W: LogicalPx = LogicalPx(52.0);
const RAIL_H: LogicalPx = LogicalPx(380.0);
const RAIL_PAD_Y: LogicalPx = LogicalPx(10.0);
const RAIL_GAP: LogicalPx = LogicalPx(4.0);
/// 브랜드 사각 24(모서리 6, 아래 여백 4).
const RAIL_BRAND: LogicalPx = LogicalPx(24.0);
const RAIL_BRAND_RADIUS: LogicalPx = LogicalPx(6.0);
/// `---` 선은 공용 `paint_rail_category_line`이 그린다. 구분선은 위아래 여백 3, `---` 버튼은 위아래 안쪽 여백 2.
const RAIL_SEP_MARGIN: LogicalPx = LogicalPx(3.0);
const RAIL_BUTTON_PAD: LogicalPx = LogicalPx(2.0);
/// 글자 아바타 28, 오른쪽 위 점 6(위치 -1, 둘레 2px bg-sidebar).
const RAIL_AVATAR: LogicalPx = LogicalPx(28.0);
const RAIL_DOT: LogicalPx = LogicalPx(6.0);
const RAIL_DOT_RING: LogicalPx = LogicalPx(2.0);
/// 팝업 위치(`left: 60, top: 118`). 안쪽 여백은 `popup-content-margin`.
const RAIL_POPUP_LEFT: LogicalPx = LogicalPx(60.0);
const RAIL_POPUP_TOP: LogicalPx = LogicalPx(118.0);

/// 레일 한 칸.
#[derive(Clone, Copy)]
enum RailItem {
    Brand,
    /// 접혔거나 빈 카테고리 경계 또는 이름 없는 구분선.
    Sep,
    /// 포인터가 올라간 `---` 버튼. 팝업이 이 버튼 오른쪽에 붙는다.
    HoveredButton,
    /// (글자, active, 오른쪽 위 점)
    Avatar(&'static str, bool, bool),
}

/// 시안 레일: Workspaces(A·S) · Services(I·A·D, 팝업 열림) · Archived(빈 카테고리, `---`만).
const RAIL: &[RailItem] = &[
    RailItem::Brand,
    RailItem::Sep,
    RailItem::Avatar("A", true, false),
    RailItem::Avatar("S", false, false),
    RailItem::HoveredButton,
    RailItem::Avatar("I", false, false),
    RailItem::Avatar("A", false, true),
    RailItem::Avatar("D", false, true),
    RailItem::Sep,
];

/// 52px 레일과 Services `---` 버튼 오른쪽에 붙은 팝업.
fn rail_frame(ui: &mut egui::Ui, theme: &Theme) {
    let bw = theme.border_width.value();
    let size = egui::vec2(
        RAIL_POPUP_LEFT.value() + POPUP_WIDTH.value(),
        RAIL_H.value(),
    );
    let (frame, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let rail = egui::Rect::from_min_size(frame.min, egui::vec2(RAIL_W.value(), RAIL_H.value()));
    let p = ui.painter();
    let radius = theme.corner_radius.value();
    p.rect_filled(rail, radius, theme.bg_sidebar().to_egui());
    p.rect_stroke(
        rail,
        radius,
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
        egui::StrokeKind::Inside,
    );
    let cx = rail.center().x;
    let line_h = theme.rail_category_line_width().value();
    let mut y = rail.top() + bw + RAIL_PAD_Y.value();
    for item in RAIL {
        match *item {
            RailItem::Brand => {
                let r = egui::Rect::from_min_size(
                    egui::pos2(cx - RAIL_BRAND.value() * 0.5, y),
                    egui::vec2(RAIL_BRAND.value(), RAIL_BRAND.value()),
                );
                p.rect_filled(
                    r,
                    RAIL_BRAND_RADIUS.value(),
                    theme.brand_melon_flesh().to_egui(),
                );
                y += RAIL_BRAND.value() + theme.spacing_xs.value();
            }
            RailItem::Sep => {
                y += RAIL_SEP_MARGIN.value();
                paint_rail_category_line(p, theme, egui::pos2(cx, y + line_h * 0.5), false);
                y += line_h + RAIL_SEP_MARGIN.value();
            }
            RailItem::HoveredButton => {
                y += RAIL_BUTTON_PAD.value();
                paint_rail_category_line(p, theme, egui::pos2(cx, y + line_h * 0.5), true);
                y += line_h + RAIL_BUTTON_PAD.value();
            }
            RailItem::Avatar(ch, active, dot) => {
                rail_avatar(p, theme, egui::pos2(cx, y), ch, active, dot);
                y += RAIL_AVATAR.value();
            }
        }
        y += RAIL_GAP.value();
    }

    let popup = egui::Rect::from_min_size(
        frame.min + egui::vec2(RAIL_POPUP_LEFT.value(), RAIL_POPUP_TOP.value()),
        egui::vec2(POPUP_WIDTH.value(), RAIL_H.value() - RAIL_POPUP_TOP.value()),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(popup));
    rail_category_menu(&mut child, theme);
}

/// 카테고리 `---` 선. 아바타 글자와 같은 무게의 표지라서 둥근 끝의 2px 선으로 그린다.
fn rail_avatar(
    p: &egui::Painter,
    theme: &Theme,
    top_center: egui::Pos2,
    ch: &str,
    active: bool,
    dot: bool,
) {
    let s = RAIL_AVATAR.value();
    let r = egui::Rect::from_min_size(
        egui::pos2(top_center.x - s * 0.5, top_center.y),
        egui::vec2(s, s),
    );
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    if active {
        p.rect_filled(r, radius, theme.surface_active().to_egui());
        p.rect_stroke(
            r,
            radius,
            egui::Stroke::new(bw, theme.accent_primary().to_egui()),
            egui::StrokeKind::Inside,
        );
    }
    let ink = if active {
        theme.text_on_active()
    } else {
        theme.text_secondary()
    };
    p.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        ch,
        egui::FontId::monospace(theme.font_size_body.value()),
        ink.to_egui(),
    );
    if dot {
        let d = RAIL_DOT.value();
        let c = egui::pos2(r.right() + bw - d * 0.5, r.top() - bw + d * 0.5);
        p.circle_filled(
            c,
            d * 0.5 + RAIL_DOT_RING.value(),
            theme.bg_sidebar().to_egui(),
        );
        p.circle_filled(c, d * 0.5, theme.accent_primary().to_egui());
    }
}

/// 시안 레일 팝업: 누를 수 없는 카테고리 이름 머리줄(+ 개수) 아래에 메뉴 항목.
fn rail_category_menu(ui: &mut egui::Ui, theme: &Theme) {
    let bw = theme.border_width.value();
    let ring = theme.popup_content_margin().value();
    egui::Frame::new()
        .fill(theme.menu_bg().to_egui())
        .stroke(egui::Stroke::new(bw, theme.menu_border().to_egui()))
        .corner_radius(theme.menu_radius().value())
        .shadow(theme.shadow_popover().to_egui())
        .inner_margin(egui::Margin::same(ring as i8))
        .show(ui, |ui| {
            ui.set_width(POPUP_WIDTH.value() - (ring + bw) * 2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            // 머리줄: 안쪽 여백 4 8 8, 아래 separator, 아래 바깥 여백 4.
            egui::Frame::new()
                .inner_margin(egui::Margin {
                    left: theme.spacing_sm.value() as i8,
                    right: theme.spacing_sm.value() as i8,
                    top: theme.spacing_xs.value() as i8,
                    bottom: theme.spacing_sm.value() as i8,
                })
                .show(ui, |ui| header_row(ui, theme, "Services", "3"));
            let (line, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), bw), egui::Sense::hover());
            ui.painter()
                .rect_filled(line, 0.0, theme.separator.to_egui_premultiplied());
            ui.add_space(theme.spacing_xs.value());
            let item = |ui: &mut egui::Ui, g: MockGlyph, label: &str, danger: bool| {
                let variant = if danger {
                    MenuItemVariant::Danger
                } else {
                    MenuItemVariant::Normal
                };
                menu_item(
                    ui,
                    theme,
                    Some(&|ui, rect, c| g.image(rect.height(), c).paint_at(ui, rect)),
                    label,
                    None,
                    variant,
                    false,
                    true,
                );
            };
            item(ui, PLUS, "Add workspace", false);
            item(ui, CHEVRON_DOWN, "Collapse", false);
            menu_separator(ui, theme);
            item(ui, EDIT, "Rename category", false);
            item(ui, TRASH, "Delete category", true);
        });
}

/// Overlays › Workspace categories — 접힌 레일의 `---` 카테고리 버튼과 오른쪽 팝업.
pub fn draw_rail(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Center, |ui| rail_frame(ui, theme));
    spec::meta(
        ui,
        theme,
        &[
            ("boundary", "--- full-width button"),
            ("anchor", "popup to the right of the button"),
            ("header", "category name — non-clickable"),
            ("items", "Add workspace · Collapse · Rename · Delete"),
            ("collapsed/empty", "--- only, no avatars"),
            ("reserved", "no Rename/Delete"),
        ],
        &[
            TokenChip::new(
                "rail-category-line",
                "--- line (idle) → text-muted",
                theme.rail_category_line().to_egui(),
            ),
            TokenChip::new(
                "rail-category-line-hover",
                "--- line (hover · pressed) → text-secondary",
                theme.rail_category_line_hover().to_egui(),
            ),
            TokenChip::without_color(
                "rail-category-line-width",
                "→ focus-ring-width 2 · round ends",
            ),
            TokenChip::new(
                "surface-raised",
                "popup fill",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new("menu-border", "edge", theme.menu_border().to_egui()),
            TokenChip::without_color("popup-content-margin", "inner ring 4 (all sides)"),
            TokenChip::new(
                "accent-danger",
                "delete row",
                theme.accent_danger().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Collapse state is per-category and shared with the full sidebar (both seed from the \
         persisted collapsed flag in layout.json). Toggling here or in the full header updates the \
         same state.",
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "Create / rename", |ui| {
            edit_dialog(ui, theme, "Services", None)
        });
        spec::cluster(ui, theme, "Validation error", |ui| {
            edit_dialog(
                ui,
                theme,
                "normal",
                Some(crate::i18n::t("workspace_category.error.reserved")),
            )
        });
        spec::cluster(ui, theme, "Delete confirm", |ui| delete_confirm(ui, theme));
        spec::cluster(ui, theme, "Rail popup", |ui| {
            // 줄바꿈 줄은 가로 배치라 팝업 폭이 행 남은 폭으로 늘어난다. 레일 예제처럼
            // 팝업 폭의 세로 칸 안에 그린다.
            ui.allocate_ui_with_layout(
                egui::vec2(POPUP_WIDTH.value(), 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| rail_category_menu(ui, theme),
            );
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "edit",
                "shared rename view · 360 × ui scale · inline validation",
            ),
            ("delete", "380px · destructive · trash glyph"),
            ("rail popup", "176px · name header + actions"),
            (
                "error",
                "reserved / duplicate / empty → danger line + disabled",
            ),
        ],
        &[
            TokenChip::new("dialog-bg", "edit frame", theme.dialog_bg().to_egui()),
            TokenChip::new("bg-panel", "delete frame", theme.bg_panel().to_egui()),
            TokenChip::new(
                "surface-raised",
                "rail popup frame",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "delete + error",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::without_color("shadow-popover", "lift"),
        ],
    );

    spec::note(
        ui,
        theme,
        "생성/이름변경은 360px 단일필드 다이얼로그를 재사용하고 확인 시 백엔드와 동일 규칙 \
         (빈/예약어 normal/중복) 으로 라이브 검증한다. 삭제는 destructive confirm 을 한 번 \
         거치며 본문이 안전한 결과(워크스페이스는 normal 로 이동)를 안내한다. 레일 팝업은 \
         `---` 버튼 우측에 앵커드로 뜬다.",
    );
}

/// 팝업 머리줄 — 이름(body, text-primary)과 오른쪽 개수(mono micro, text-muted).
/// 시안은 `alignItems: baseline` 이라 크기가 다른 두 글자의 기준선을 맞춘다.
fn header_row(ui: &mut egui::Ui, theme: &Theme, name: &str, count: &str) {
    let name = ui.painter().layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
    );
    let count = ui.painter().layout_no_wrap(
        count.to_owned(),
        egui::FontId::monospace(theme.font_size_micro.value()),
        theme.text_muted().to_egui(),
    );
    let baseline = |g: &egui::Galley| {
        g.rows
            .first()
            .and_then(|r| r.glyphs.first())
            .map_or(g.rect.height(), |gl| gl.pos.y)
    };
    let (name_base, count_base) = (baseline(&name), baseline(&count));
    let top = (count_base - name_base).max(0.0);
    let h = (top + name.rect.height()).max(name_base - count_base + count.rect.height());
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
    let name_pos = egui::pos2(rect.left(), rect.top() + top);
    let count_pos = egui::pos2(
        rect.right() - count.rect.width(),
        name_pos.y + name_base - count_base,
    );
    let (name_fg, count_fg) = (theme.text_primary().to_egui(), theme.text_muted().to_egui());
    ui.painter().galley(name_pos, name, name_fg);
    ui.painter().galley(count_pos, count, count_fg);
}
