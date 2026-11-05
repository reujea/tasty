//! 파일 경로와 상위 이동·새로고침 버튼. 긴 경로는 breadcrumb 안에서 줄인다.

use tasty_ui_widgets::tokens::STRUCT_GAP_2;
use tasty_ui_widgets::{ControlSize, IconButton, IconButtonVariant, MenuItemVariant, menu_item};

use tasty_ui_widgets::crumb_alloc::{Caps, CrumbSlot, Measure, Role, plan};

use super::{CRUMB_GLYPH, FilePickerAction, FilePickerProps};
use crate::adapters::ui::icons;
use crate::theme::Theme;

/// 오른쪽 버튼 폭을 먼저 확보하고 남은 폭에 breadcrumb을 그린다. 경로·Up·Refresh 사이는 모두
/// `fp-section-gap` 이다.
pub(super) fn path_bar(
    ui: &mut egui::Ui,
    props: &FilePickerProps<'_>,
    action: &mut FilePickerAction,
) {
    let th = props.theme;
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ControlSize::Sm.height(th)),
        egui::Sense::hover(),
    );
    let mut buttons = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_path_buttons")
            .max_rect(row)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    buttons.spacing_mut().item_spacing.x = th.fp_section_gap().value();
    if IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(&mut buttons, th, &|ui, rect, c| {
            icons::REFRESH.image(rect.height(), c).paint_at(ui, rect)
        })
        .clicked()
    {
        *action = FilePickerAction::Refresh;
    }
    if IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .show(&mut buttons, th, &|ui, rect, c| {
            icons::CHEVRON_UP.image(rect.height(), c).paint_at(ui, rect)
        })
        .clicked()
    {
        *action = FilePickerAction::NavigateUp;
    }
    let crumbs_right = (buttons.min_rect().left() - th.fp_section_gap().value()).max(row.left());
    let crumbs_rect = egui::Rect::from_min_max(row.min, egui::pos2(crumbs_right, row.bottom()));
    let mut crumbs_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_crumbs")
            .max_rect(crumbs_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    crumbs_ui.set_clip_rect(crumbs_rect.intersect(ui.clip_rect()));
    draw_crumbs(&mut crumbs_ui, props, action);
}

/// root는 고정폭 글꼴을 사용한다.
fn crumb_font(th: &Theme, root: bool) -> egui::FontId {
    let size = th.font_size_caption.value();
    if root {
        egui::FontId::monospace(size)
    } else {
        egui::FontId::proportional(size)
    }
}

fn crumb_color(th: &Theme, current: bool) -> egui::Color32 {
    if current {
        th.text_primary().into()
    } else {
        th.accent_primary().into()
    }
}

/// 말줄임 없는 폭.
fn natural_width(ui: &egui::Ui, th: &Theme, label: &str, root: bool) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_owned(),
            crumb_font(th, root),
            egui::Color32::PLACEHOLDER,
        )
        .size()
        .x
}

/// 조상 이름은 끝을, 현재 폴더 이름은 앞을 줄이고 …를 붙인다.
/// 현재 폴더는 이름 끝의 차이를 구분할 수 있도록 남긴다.
fn crumb_galley(
    ui: &egui::Ui,
    th: &Theme,
    label: &str,
    root: bool,
    current: bool,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let font = crumb_font(th, root);
    let color = crumb_color(th, current);
    let text = if current {
        elide_front(ui, label, &font, width)
    } else {
        label.to_owned()
    };
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.fonts(|f| f.layout_job(job))
}

/// 앞에서 말줄임 — `…-bbbb`. 들어가는 가장 긴 **접미사**를 찾아 앞에 `…` 를 붙인다.
///
/// egui 의 `truncate_at_width` 는 꼬리에서만 자른다. 그래서 문자열을 먼저 줄여 넘긴다.
fn elide_front(ui: &egui::Ui, label: &str, font: &egui::FontId, width: f32) -> String {
    let measure = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    if measure(label) <= width {
        return label.to_owned();
    }
    // 문자 경계에서만 자른다 — 바이트로 자르면 다국어 이름에서 패닉한다.
    let mut best = String::from("…");
    for (at, _) in label.char_indices() {
        let candidate = format!("…{}", &label[at..]);
        if measure(&candidate) <= width {
            best = candidate;
            break;
        }
    }
    best
}

/// path bar 프레임의 치수 — 전부 `component.fp-*` 토큰이고 `&Theme` 접근자가 배율을 건다.
fn caps(th: &Theme) -> Caps {
    Caps {
        crumb_max: th.fp_crumb_max_width().value(),
        ancestor_min: th.fp_crumb_min_width().value(),
        current_min: th.fp_crumb_current_min_width().value(),
        hysteresis: th.fp_bar_hysteresis().value(),
    }
}

/// `…` 메뉴 폭의 밴드 — `component.fp-crumb-menu-{min,max}-width`.
fn menu_band(th: &Theme) -> (f32, f32) {
    (
        th.fp_crumb_menu_min_width().value(),
        th.fp_crumb_menu_max_width().value(),
    )
}

/// 메뉴 틀의 안쪽 여백(`popup-content-margin`)과 테두리를 합친 좌우 폭.
fn menu_chrome(th: &Theme) -> f32 {
    (th.popup_content_margin().value() + th.border_width.value()) * 2.0
}

/// 메뉴의 바깥 폭. 행(폴더 아이콘·간격·라벨·패딩)에 틀을 더한 border-box 폭을 최소·최대 범위 안으로
/// 제한한다.
fn menu_width(widest_label: f32, th: &Theme) -> f32 {
    let (floor, ceiling) = menu_band(th);
    let row = widest_label
        + th.menu_item_padding_x().value() * 2.0
        + th.icon_glyph_size_md.value()
        + th.spacing_sm.value();
    (row + menu_chrome(th)).clamp(floor, ceiling)
}

/// 경계 폭에서 표시 방식이 매 프레임 바뀌지 않도록 직전 단계를 기억한다.
fn step_memory_id(ui: &egui::Ui) -> egui::Id {
    ui.make_persistent_id("file_picker_path_bar_step")
}

fn draw_crumbs(ui: &mut egui::Ui, props: &FilePickerProps<'_>, action: &mut FilePickerAction) {
    let th = props.theme;
    let gap = STRUCT_GAP_2.value();
    ui.spacing_mut().item_spacing.x = gap;
    let n = props.crumbs.len();
    if n == 0 {
        return;
    }
    let natural: Vec<f32> = props
        .crumbs
        .iter()
        .enumerate()
        .map(|(i, c)| natural_width(ui, th, &c.label, i == 0))
        .collect();
    let measure = Measure {
        natural: &natural,
        ellipsis: natural_width(ui, th, "…", false),
        separator: CRUMB_GLYPH.value() + gap * 2.0,
        available: ui.available_width(),
        caps: caps(th),
    };
    let memory_id = step_memory_id(ui);
    let previous = ui.memory(|m| m.data.get_temp::<usize>(memory_id));
    let plan = plan(&measure, previous);
    ui.memory_mut(|m| m.data.insert_temp(memory_id, plan.step));

    for (slot_idx, placed) in plan.placed.iter().enumerate() {
        if slot_idx > 0 {
            ui.add(icons::CHEVRON_RIGHT.image(CRUMB_GLYPH.value(), th.text_disabled().into()));
        }
        match &placed.slot {
            CrumbSlot::Crumb(i) => {
                let i = *i;
                let is_current = placed.role == Role::Current;
                let galley = crumb_galley(
                    ui,
                    th,
                    &props.crumbs[i].label,
                    i == 0,
                    is_current,
                    placed.width,
                );
                let sense = if is_current {
                    egui::Sense::hover()
                } else {
                    egui::Sense::click()
                };
                let (rect, resp) = ui.allocate_exact_size(galley.size(), sense);
                ui.painter()
                    .galley(rect.min, galley, crumb_color(th, is_current));
                if !is_current {
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        *action = FilePickerAction::NavigateTo(i);
                    }
                }
            }
            CrumbSlot::Hidden(range) => hidden_crumbs(ui, props, range.clone(), action),
        }
    }
}

/// `…` 크럼 — 클릭하면 숨긴 조상들을 메뉴로 나열한다.
fn hidden_crumbs(
    ui: &mut egui::Ui,
    props: &FilePickerProps<'_>,
    range: std::ops::Range<usize>,
    action: &mut FilePickerAction,
) {
    let th = props.theme;
    let galley = crumb_galley(ui, th, "…", false, false, natural_width(ui, th, "…", false));
    let (rect, resp) = ui.allocate_exact_size(galley.size(), egui::Sense::click());
    ui.painter()
        .galley(rect.min, galley, th.accent_primary_raised().into());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let template = if range.len() == 1 {
        props.hidden_folders_one
    } else {
        props.hidden_folders_many
    };
    let resp = resp.on_hover_text(template.replace("{}", &range.len().to_string()));
    let popup_id = ui.make_persistent_id("file_picker_hidden_crumbs");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    // 앵커 메뉴의 안쪽 여백은 네 변 모두 popup-content-margin 이다. 팝업 틀이 부모 스타일을 읽으므로
    // 여는 동안만 바꾸고 되돌린다(scope 는 크럼 줄에 빈 칸과 간격을 더해 쓰지 않는다).
    let prev_margin = ui.spacing().menu_margin;
    ui.spacing_mut().menu_margin = egui::Margin::same(th.popup_content_margin().value() as i8);
    tasty_egui_theme::with_popover_frame(ui, th, |ui| {
        egui::popup_below_widget(
            ui,
            popup_id,
            &resp,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                let font = egui::FontId::proportional(th.font_size_body.value());
                let widest = props.crumbs[range.clone()]
                    .iter()
                    .map(|c| {
                        ui.painter()
                            .layout_no_wrap(
                                c.label.clone(),
                                font.clone(),
                                egui::Color32::PLACEHOLDER,
                            )
                            .size()
                            .x
                    })
                    .fold(0.0_f32, f32::max);
                // 밴드는 테두리까지 포함한 바깥 폭이라 안쪽 폭은 틀을 뺀 값이다. 행은 남은 폭을
                // 모두 차지하므로 상한만 두면 메뉴가 늘 상한까지 늘어난다 — 폭을 고정한다.
                ui.set_width(menu_width(widest, th) - menu_chrome(th));
                let rows = range.clone().map(|i| (i, props.crumbs[i].label.as_str()));
                if let Some(i) = hidden_menu_rows(ui, th, rows) {
                    *action = FilePickerAction::NavigateTo(i);
                }
            },
        )
    });
    ui.spacing_mut().menu_margin = prev_margin;
}

/// 숨은 조상 폴더 행을 그리고 누른 행의 크럼 번호를 돌려준다. 행은 다른 메뉴처럼
/// menu-item-height 간격으로 붙인다.
fn hidden_menu_rows<'a>(
    ui: &mut egui::Ui,
    th: &Theme,
    rows: impl Iterator<Item = (usize, &'a str)>,
) -> Option<usize> {
    ui.spacing_mut().item_spacing.y = 0.0;
    let folder = th.accent_primary().to_egui();
    let mut clicked = None;
    for (i, label) in rows {
        let glyph = |ui: &mut egui::Ui, rect: egui::Rect, _c: egui::Color32| {
            icons::FOLDER
                .image(rect.height(), folder)
                .paint_at(ui, rect)
        };
        if menu_item(
            ui,
            th,
            Some(&glyph),
            label,
            None,
            MenuItemVariant::Normal,
            false,
            true,
        )
        .clicked()
        {
            clicked = Some(i);
        }
    }
    clicked
}

#[cfg(test)]
mod hidden_menu_row_tests {
    use super::*;

    /// 기본 item_spacing 이 0 이 아닌 Ui 에서도 세 행의 높이 합이 menu-item-height 의 세 배다.
    #[test]
    fn hidden_folder_rows_sit_flush_at_the_menu_item_height() {
        let th = crate::theme::theme();
        let ctx = egui::Context::default();
        let mut height = 0.0;
        let _frame = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(ui.spacing().item_spacing.y > 0.0);
                let top = ui.cursor().top();
                let rows = ["a", "b", "c"].into_iter().enumerate();
                hidden_menu_rows(ui, &th, rows);
                height = ui.cursor().top() - top;
            });
        });
        assert_eq!(height, th.menu_item_height().value() * 3.0);
    }
}

#[cfg(test)]
mod menu_width_tests {
    use super::*;

    #[test]
    fn the_menu_width_is_measured_inside_the_band_and_counts_the_whole_row() {
        let th = crate::theme::theme();
        let (floor, ceiling) = menu_band(&th);
        assert!(
            floor < ceiling,
            "최소 너비가 최대 너비 이상이다: {floor} .. {ceiling}"
        );

        assert_eq!(
            menu_width(0.0, &th),
            floor,
            "짧은 경로에 최소 너비가 적용되지 않았다"
        );
        assert_eq!(
            menu_width(ceiling * 2.0, &th),
            ceiling,
            "긴 경로에 최대 너비가 적용되지 않았다"
        );

        let at_floor = menu_width(0.0, &th) - floor; // 0 — 바닥에 걸려 안 보인다
        assert_eq!(at_floor, 0.0);
        let mid_label = (floor + ceiling) * 0.5;
        let mid = menu_width(mid_label, &th);
        assert!(
            mid > mid_label + menu_chrome(&th),
            "행 chrome(글리프 · gap · 좌우 패딩)과 틀(여백·테두리)을 안 셌다 — {mid} <= {mid_label}"
        );
        assert!(
            mid < ceiling,
            "중간 너비를 검사할 입력이 최대 너비에 도달했다"
        );
    }
}
