//! 파일 피커 path bar — breadcrumb 과 상위·새로고침 버튼. 넘치는 경로를 흡수하는 자리다
//! (`docs/features/native-file-picker/index.md` "긴 경로 — 넘침은 path bar 가 흡수한다").

use tasty_ui_widgets::tokens::STRUCT_GAP_2;
use tasty_ui_widgets::{IconButton, IconButtonVariant, MenuItemVariant, menu_item};

use tasty_ui_widgets::crumb_alloc::{Caps, CrumbSlot, Measure, Role, plan};

use super::{CRUMB_GLYPH, FilePickerAction, FilePickerProps};
use crate::adapters::ui::icons;
use crate::theme::Theme;

/// path bar — 오른쪽 버튼(flex:none)이 먼저 자리 잡고, breadcrumb 이 남은 폭(flex:1;
/// min-width:0; overflow:hidden) 안에서만 그린다.
pub(super) fn path_bar(
    ui: &mut egui::Ui,
    props: &FilePickerProps<'_>,
    action: &mut FilePickerAction,
) {
    let th = props.theme;
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), th.item_height_interactive.value()),
        egui::Sense::hover(),
    );
    let mut buttons = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("file_picker_path_buttons")
            .max_rect(row)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    buttons.spacing_mut().item_spacing.x = STRUCT_GAP_2.value();
    if IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .show(&mut buttons, th, &|ui, rect, c| {
            icons::REFRESH.image(rect.height(), c).paint_at(ui, rect)
        })
        .clicked()
    {
        *action = FilePickerAction::Refresh;
    }
    if IconButton::new()
        .variant(IconButtonVariant::Ghost)
        .show(&mut buttons, th, &|ui, rect, c| {
            icons::CHEVRON_UP.image(rect.height(), c).paint_at(ui, rect)
        })
        .clicked()
    {
        *action = FilePickerAction::NavigateUp;
    }
    let crumbs_right = (buttons.min_rect().left() - th.spacing_sm.value()).max(row.left());
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

/// 크럼 한 칸의 글꼴 — root 만 mono 다(경로 뿌리는 값이지 이름이 아니다).
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

/// 칸 하나를 주어진 폭에서 말줄임한 galley.
///
/// 방향이 역할마다 다르다. 조상은 **꼬리**에서 자른다(앞이 상위 경로다). 현재 폴더는
/// **앞**에서 자른다 — 형제 폴더를 가르는 것은 꼬리이고, 꼬리를 자르면 두 폴더가 같은
/// 문자열로 보인다. 어느 쪽이든 잘렸으면 `…` 가 보인다.
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

/// 가장 긴 라벨 폭으로 정하는 메뉴 폭. 밴드 안에서 **내용이 정한다**.
///
/// 라벨만으로 재면 안 된다 — 행은 폴더 글리프 + gap + 라벨 + 좌우 패딩이라, 그 몫을
/// 빼놓으면 긴 이름이 천장에 닿기 전에 잘린다.
fn menu_width(widest_label: f32, th: &Theme) -> f32 {
    let (floor, ceiling) = menu_band(th);
    let row = widest_label
        + th.menu_item_padding_x().value() * 2.0
        + th.icon_glyph_size_md.value()
        + th.spacing_sm.value();
    row.clamp(floor, ceiling)
}

#[cfg(test)]
mod menu_width_tests {
    use super::*;

    /// 짧은 경로는 바닥을, 긴 경로는 천장을 받고, 그 사이는 **행 전체**로 잰다.
    /// 라벨만 세면 가운데 갈래가 글리프와 패딩만큼 좁아져 이름이 일찍 잘린다.
    #[test]
    fn the_menu_width_is_measured_inside_the_band_and_counts_the_whole_row() {
        let th = crate::theme::theme();
        let (floor, ceiling) = menu_band(&th);
        assert!(floor < ceiling, "밴드가 뒤집혔다: {floor} .. {ceiling}");

        assert_eq!(menu_width(0.0, &th), floor, "짧은 경로가 바닥을 안 받았다");
        assert_eq!(
            menu_width(ceiling * 2.0, &th),
            ceiling,
            "긴 경로가 천장을 안 받았다"
        );

        // 바닥과 천장 사이로 떨어지는 라벨 폭 하나.
        let chrome = menu_width(0.0, &th) - floor; // 0 — 바닥에 걸려 안 보인다
        assert_eq!(chrome, 0.0);
        let mid_label = (floor + ceiling) * 0.5;
        let mid = menu_width(mid_label, &th);
        assert!(
            mid > mid_label,
            "행 chrome(글리프 · gap · 좌우 패딩)을 안 셌다 — {mid} <= {mid_label}"
        );
        assert!(
            mid < ceiling,
            "가운데 갈래가 천장에 붙었다 — 이 케이스가 아무것도 안 잰다"
        );
    }
}

/// 직전 프레임의 사다리 단을 기억하는 자리. 히스테리시스가 이 값을 읽는다 — 이것이 없으면
/// 경계 폭에서 두 단이 매 프레임 뒤바뀐다.
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
        .galley(rect.min, galley, th.accent_primary().into());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // 툴팁은 **클릭하면 무엇이 되는지**를 말한다("Show 3 hidden folders") — 상태 서술이
    // 아니다(디자인 2026-09-14 §6 이 직전 판 "N folders hidden" 을 그 이유로 물렸다).
    let resp = resp.on_hover_text(if range.len() == 1 {
        props.hidden_folders_one.to_owned()
    } else {
        props
            .hidden_folders_many
            .replace("{}", &range.len().to_string())
    });
    let popup_id = ui.make_persistent_id("file_picker_hidden_crumbs");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    egui::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            // 메뉴 폭은 **밴드 안에서 내용으로 잰다** — 짧은 경로가 넓은 메뉴를 받지
            // 않고 긴 경로는 천장에서 말줄임한다(디자인 `FpCrumbMenu`).
            let font = egui::FontId::proportional(th.font_size_body.value());
            let widest = props.crumbs[range.clone()]
                .iter()
                .map(|c| {
                    ui.painter()
                        .layout_no_wrap(c.label.clone(), font.clone(), egui::Color32::PLACEHOLDER)
                        .size()
                        .x
                })
                .fold(0.0_f32, f32::max);
            let band = menu_band(th);
            ui.set_min_width(menu_width(widest, th));
            ui.set_max_width(band.1);
            let folder = th.accent_primary().to_egui();
            for i in range.clone() {
                let glyph = |ui: &mut egui::Ui, rect: egui::Rect, _c: egui::Color32| {
                    icons::FOLDER
                        .image(rect.height(), folder)
                        .paint_at(ui, rect)
                };
                if menu_item(
                    ui,
                    th,
                    Some(&glyph),
                    &props.crumbs[i].label,
                    None,
                    MenuItemVariant::Normal,
                    false,
                    true,
                )
                .clicked()
                {
                    *action = FilePickerAction::NavigateTo(i);
                }
            }
        },
    );
}
