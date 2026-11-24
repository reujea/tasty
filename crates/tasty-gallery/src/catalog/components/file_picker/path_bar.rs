//! 파일 선택 예제의 경로와 상위 이동·새로고침 버튼. 경로는 본체와 같은 `crumb_alloc::plan` 으로 실폭에 배분한다.

use tasty_ui_widgets::crumb_alloc::{Caps, CrumbSlot, Measure, Plan, Role, plan};
use tasty_ui_widgets::tokens::STRUCT_GAP_2;
use tasty_ui_widgets::{
    ControlSize, IconButton, IconButtonVariant, MenuItemVariant, menu_item_with_hover,
};

use super::{CRUMB_GLYPH, Variant, path_bar_height};
use crate::catalog::icons;
use crate::catalog::widgets::dialog as kit;
use tasty_egui_theme::overlay_edge::BoxEdge;
use tasty_type_appearance::theme::Theme;

/// 경로 데이터의 종류 — 디자인 `FilePickerFrame` 의 `remote` · `deep` · `pathKind`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PathKind {
    /// `/ › Users › maya › projects`.
    Local,
    /// 원격 호스트가 root 인 경로.
    Remote,
    /// 조상이 많은 경로 — `/Users/maya/tasty/config/keybindings/exports/2026-09`.
    Deep,
    /// 부모·현재 폴더가 모두 53자인 경로 — 디자인 `pathKind="longtwo"`.
    LongTwo,
    /// 긴 UNC root 와 짧은 현재 폴더 — 디자인 `pathKind="longroot"`.
    LongRoot,
}

/// 접힌 조상의 `…` 칸이 그려진 자리와 숨긴 조상들. `…` 메뉴 예제가 그 아래에 붙는다.
pub(super) struct HiddenCrumbs {
    pub anchor: egui::Rect,
    pub labels: Vec<&'static str>,
}

pub(super) fn path_bar(ui: &mut egui::Ui, theme: &Theme, v: Variant) -> Option<HiddenCrumbs> {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(v.w.value(), path_bar_height(theme).value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + theme.fp_inset_start().value(),
            rect.top() + theme.fp_path_pad_y().value(),
        ),
        egui::pos2(
            rect.right() - theme.fp_inset_end().value(),
            rect.bottom() - theme.fp_path_pad_y().value(),
        ),
    );
    // 시안 path bar 는 경로 · Up · Refresh 를 fp-section-gap 간격으로 놓는다.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.fp_section_gap().value();
    for glyph in [icons::REFRESH, icons::CHEVRON_UP] {
        IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .show(&mut child, theme, &|ui, rect, c| {
                glyph.image(rect.height(), c).paint_at(ui, rect)
            });
    }
    let crumbs_rect = egui::Rect::from_min_size(
        inner.min,
        egui::vec2(crumbs_width(theme, rect.width()), inner.height()),
    );
    let mut crumbs_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(crumbs_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    crumbs_ui.set_clip_rect(crumbs_rect.intersect(ui.clip_rect()));
    crumbs(&mut crumbs_ui, theme, v.path)
}

/// 경로 칸의 가용 폭 — 디자인 measure: 막대 − 좌우 inset − Up − Refresh − 간격 둘(팝업 폭이 아니다).
pub(super) fn crumbs_width(theme: &Theme, bar_w: f32) -> f32 {
    let button = ControlSize::Sm.height(theme) + theme.fp_section_gap().value();
    (bar_w - theme.fp_inset_start().value() - theme.fp_inset_end().value() - button * 2.0).max(0.0)
}

/// 이 카드의 경로가 받는 배분 — 성분 수와 `crumb_alloc::plan` 의 단계. 폭 사다리 예제가 라벨에 쓴다.
pub(super) fn path_step(ui: &egui::Ui, theme: &Theme, v: Variant) -> (usize, usize) {
    let plan = allocate(ui, theme, v.path, crumbs_width(theme, v.w.value()));
    (items(v.path).len(), plan.step)
}

struct Crumb {
    label: &'static str,
    root: bool,
}

const fn root(label: &'static str) -> Crumb {
    Crumb { label, root: true }
}

const fn crumb(label: &'static str) -> Crumb {
    Crumb { label, root: false }
}

// 디자인 `FilePickerFrame` 의 crumbs seed. 마지막 성분이 현재 폴더다.
const LOCAL: &[Crumb] = &[root("/"), crumb("Users"), crumb("maya"), crumb("projects")];
// 원격도 경로 뿌리는 경로의 뿌리다(호스트가 아니다) — 디자인 `FilePickerFrame` remote crumbs.
const REMOTE: &[Crumb] = &[
    root("/"),
    crumb("home"),
    crumb("deploy"),
    crumb("agents-prod"),
];
const DEEP: &[Crumb] = &[
    root("/"),
    crumb("Users"),
    crumb("maya"),
    crumb("tasty"),
    crumb("config"),
    crumb("keybindings"),
    crumb("exports"),
    crumb("2026-09"),
];
const LONG_TWO: &[Crumb] = &[
    root("/"),
    crumb("Users"),
    crumb("maya"),
    crumb("long-folder-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    crumb("current-folder-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
];
const LONG_ROOT: &[Crumb] = &[
    root("\\\\build-farm-eu-west\\releases$"),
    crumb("2026"),
    crumb("09"),
    crumb("nightly"),
];

fn items(kind: PathKind) -> &'static [Crumb] {
    match kind {
        PathKind::Local => LOCAL,
        PathKind::Remote => REMOTE,
        PathKind::Deep => DEEP,
        PathKind::LongTwo => LONG_TWO,
        PathKind::LongRoot => LONG_ROOT,
    }
}

/// root 는 mono, 나머지는 UI 글꼴 — 본체 `crumb_font` 와 같다.
fn crumb_font(theme: &Theme, root: bool) -> egui::FontId {
    let size = theme.font_size_caption.value();
    if root {
        egui::FontId::monospace(size)
    } else {
        egui::FontId::proportional(size)
    }
}

fn crumb_color(theme: &Theme, current: bool) -> egui::Color32 {
    if current {
        theme.text_primary().to_egui()
    } else {
        theme.accent_primary().to_egui()
    }
}

fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
        .size()
        .x
}

/// 앞에서 말줄임(`…-bbbb`) — 현재 폴더는 형제 폴더를 가르는 꼬리를 남긴다.
fn elide_front(ui: &egui::Ui, label: &str, font: &egui::FontId, width: f32) -> String {
    if text_width(ui, label, font) <= width {
        return label.to_owned();
    }
    for (at, _) in label.char_indices() {
        let candidate = format!("…{}", &label[at..]);
        if text_width(ui, &candidate, font) <= width {
            return candidate;
        }
    }
    "…".to_owned()
}

/// 배분 결과의 한 칸. 조상은 꼬리를, 현재 폴더는 앞을 줄인다.
fn crumb_galley(
    ui: &egui::Ui,
    theme: &Theme,
    it: &Crumb,
    current: bool,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let font = crumb_font(theme, it.root);
    let text = if current {
        elide_front(ui, it.label, &font, width)
    } else {
        it.label.to_owned()
    };
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, crumb_color(theme, current));
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.fonts(|f| f.layout_job(job))
}

/// 경로 막대의 치수 — 전부 `component.fp-*` 토큰이고 Theme 접근자가 배율을 건다.
fn caps(theme: &Theme) -> Caps {
    Caps {
        crumb_max: theme.fp_crumb_max_width().value(),
        ancestor_min: theme.fp_crumb_min_width().value(),
        current_min: theme.fp_crumb_current_min_width().value(),
        hysteresis: theme.fp_bar_hysteresis().value(),
    }
}

/// 가용 폭에 대한 배분. 갤러리 카드는 크기가 고정이라 직전 단계(히스테리시스)를 넘기지 않는다.
fn allocate(ui: &egui::Ui, theme: &Theme, kind: PathKind, available: f32) -> Plan {
    let natural: Vec<f32> = items(kind)
        .iter()
        .map(|c| text_width(ui, c.label, &crumb_font(theme, c.root)))
        .collect();
    let measure = Measure {
        natural: &natural,
        ellipsis: text_width(ui, "…", &crumb_font(theme, false)),
        separator: CRUMB_GLYPH.value() + STRUCT_GAP_2.value() * 2.0,
        available,
        caps: caps(theme),
    };
    plan(&measure, None)
}

/// 브레드크럼 — root(mono) → 조상(accent 링크) → 현재 폴더(text-primary, 비클릭).
fn crumbs(ui: &mut egui::Ui, theme: &Theme, kind: PathKind) -> Option<HiddenCrumbs> {
    let all = items(kind);
    let plan = allocate(ui, theme, kind, ui.available_width());
    ui.spacing_mut().item_spacing.x = STRUCT_GAP_2.value();
    let mut hidden = None;
    for (i, placed) in plan.placed.iter().enumerate() {
        if i > 0 {
            kit::icon(
                ui,
                icons::CHEVRON_RIGHT,
                CRUMB_GLYPH,
                theme.text_disabled().to_egui(),
            );
        }
        match &placed.slot {
            CrumbSlot::Crumb(at) => {
                let galley = crumb_galley(
                    ui,
                    theme,
                    &all[*at],
                    placed.role == Role::Current,
                    placed.width,
                );
                let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
                ui.painter()
                    .galley(rect.min, galley, theme.text_primary().to_egui());
            }
            CrumbSlot::Hidden(range) => {
                let font = crumb_font(theme, false);
                let galley = ui.painter().layout_no_wrap(
                    "…".to_owned(),
                    font,
                    theme.accent_primary().to_egui(),
                );
                let (rect, resp) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
                ui.painter()
                    .galley(rect.min, galley, theme.accent_primary().to_egui());
                resp.on_hover_text(hidden_tooltip(range.len()));
                hidden = Some(HiddenCrumbs {
                    anchor: rect,
                    labels: all[range.clone()].iter().map(|c| c.label).collect(),
                });
            }
        }
    }
    hidden
}

/// `…` 의 툴팁 — 디자인 `FpCrumbs` title. 하나면 단수형이다.
fn hidden_tooltip(count: usize) -> String {
    crate::i18n::t_count(
        "filepicker.hidden_folders",
        count as u64,
        &[&count.to_string()],
    )
}

/// `…` 메뉴 틀의 안쪽 여백과 테두리를 합친 좌우 폭 — 본체 `menu_chrome` 과 같다.
fn menu_chrome(theme: &Theme) -> f32 {
    (theme.popup_content_margin().value() + theme.border_width.value()) * 2.0
}

/// `…` 메뉴의 바깥 폭. 행(폴더 아이콘·간격·라벨·좌우 패딩)에 틀을 더한 border-box 폭을 밴드 안으로
/// 제한한다 — 본체 `menu_width` 와 같다.
fn menu_width(widest_label: f32, theme: &Theme) -> f32 {
    let row = widest_label
        + theme.menu_item_padding_x().value() * 2.0
        + theme.icon_glyph_size_md.value()
        + theme.spacing_sm.value();
    (row + menu_chrome(theme)).clamp(
        theme.fp_crumb_menu_min_width().value(),
        theme.fp_crumb_menu_max_width().value(),
    )
}

/// 펼친 `…` 메뉴 — 디자인 `FpCrumbMenu`. 숨긴 조상을 경로 순서로 한 줄씩 놓고 첫 줄은 hover 상태로 보인다.
pub(super) fn crumb_menu(
    ui: &mut egui::Ui,
    theme: &Theme,
    hidden: &HiddenCrumbs,
    bar_bottom: f32,
    edge: BoxEdge,
) -> egui::Rect {
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let widest = hidden
        .labels
        .iter()
        .map(|l| text_width(ui, l, &font))
        .fold(0.0_f32, f32::max);
    let width = menu_width(widest, theme);
    let origin = egui::pos2(hidden.anchor.left(), bar_bottom + theme.spacing_xs.value());
    let mut menu_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                origin,
                egui::vec2(width, f32::INFINITY),
            ))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    egui::Frame::new()
        .fill(theme.menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.menu_border().to_egui(),
        ))
        .corner_radius(edge.radius(theme, theme.menu_radius()).value())
        .shadow(edge.shadow(theme, theme.shadow_popover()).to_egui())
        .inner_margin(egui::Margin::same(
            theme.popup_content_margin().value() as i8
        ))
        .show(&mut menu_ui, |ui| {
            // 밴드는 테두리까지 포함한 바깥 폭이다. 안쪽 폭은 틀을 뺀 값이다.
            ui.set_width(width - menu_chrome(theme));
            ui.spacing_mut().item_spacing.y = 0.0;
            let folder = theme.accent_primary().to_egui();
            for (i, label) in hidden.labels.iter().enumerate() {
                let glyph = |ui: &mut egui::Ui, rect: egui::Rect, _c: egui::Color32| {
                    icons::FOLDER
                        .image(rect.height(), folder)
                        .paint_at(ui, rect)
                };
                // 첫 행은 hover 상태로 보여 준다.
                menu_item_with_hover(
                    ui,
                    theme,
                    Some(&glyph),
                    label,
                    None,
                    MenuItemVariant::Normal,
                    false,
                    true,
                    i == 0,
                );
            }
        })
        .response
        .rect
}
