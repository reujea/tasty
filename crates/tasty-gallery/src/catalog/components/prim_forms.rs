//! Select, Checkbox, Switch의 상태별 예제. MultiSelect 는 `prim_multiselect`,
//! SearchableSelect 의 상태 전체는 `prim_searchable_select` 에 있고 여기에는 같은 줄 비교용 live 칸만 둔다.

use std::cell::RefCell;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Ground, SearchableSelect, SearchableSelectLabels, SearchableSelectOption, SelectLook, checkbox,
    select, select_or_placeholder, select_pinned, switch,
};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, meta, stage, wrap_item};

const FONTS: [SearchableSelectOption<'static>; 4] = [
    SearchableSelectOption::new("D2Coding"),
    SearchableSelectOption::new("JetBrains Mono"),
    SearchableSelectOption::new("Fira Code"),
    SearchableSelectOption::new("Cascadia Code"),
];
const PIN: [SearchableSelectOption<'static>; 1] = [SearchableSelectOption::with_value(
    "",
    "Default (monospace)",
)];
/// 검색 안내 · 결과 없음 · 로딩은 본체 글꼴 행과 같은 번역 키(갤러리 손 사본을 두지 않는다).
fn font_labels() -> SearchableSelectLabels<'static> {
    SearchableSelectLabels {
        placeholder: "Choose a font",
        search_placeholder: crate::i18n::t("settings.appearance.font_search_placeholder"),
        empty: crate::i18n::t("settings.appearance.font_no_match"),
        loading: crate::i18n::t("settings.appearance.font_loading"),
    }
}

thread_local! {
    static STATE: RefCell<FormState> = const {
        RefCell::new(FormState {
            sel: 0,
            sel_long: 3,
            sel_placeholder: None,
            font: None,
            check_a: true,
            check_b: false,
            switch_a: true,
            switch_b: false,
        })
    };
}

struct FormState {
    sel: usize,
    sel_long: usize,
    /// "아직 안 고름" 상태를 가진 Select — 처음엔 비어 있어 placeholder 가 보인다.
    sel_placeholder: Option<usize>,
    /// SearchableSelect 의 값. 처음엔 비어 placeholder 가 보인다.
    font: Option<String>,
    check_a: bool,
    check_b: bool,
    switch_a: bool,
    switch_b: bool,
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let field_md = theme.field_width_md.value();
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        stage(ui, theme, StageVariant::Column, |ui| {
            cluster(ui, theme, "Select", |ui| {
                let opts = ["Default (full rc)", "Tasty rc", "Custom"];
                select(
                    ui,
                    theme,
                    "gallery_theme",
                    &mut st.sel,
                    &opts,
                    field_md,
                    true,
                );
            });
            // 회귀 방지: field_width_md(160px) 가용 폭(~108px)을 넘는 긴 옵션 라벨 —
            // Codex 플러그인 default_approval_policy select 재현 케이스.
            cluster(ui, theme, "Select (long text)", |ui| {
                let opts = [
                    "상속 (codex 기본값)",
                    "Untrusted (신뢰되지 않은 명령만 승인 요청)",
                    "On request (모델이 판단)",
                    "Never (승인 프롬프트 없음)",
                ];
                select(
                    ui,
                    theme,
                    "gallery_select_long",
                    &mut st.sel_long,
                    &opts,
                    field_md,
                    true,
                );
            });
            // 한 번 선택하면 placeholder는 목록에서 사라진다.
            cluster(ui, theme, "Select (placeholder)", |ui| {
                let opts = ["Ctrl", "Alt", "Ctrl+Alt"];
                select_or_placeholder(
                    ui,
                    theme,
                    "gallery_select_placeholder",
                    &mut st.sel_placeholder,
                    &opts,
                    "Select a modifier",
                    field_md,
                    true,
                );
            });
            cluster(ui, theme, "Searchable Select", |ui| {
                SearchableSelect::new("gallery_searchable_select", &FONTS, font_labels())
                    .pinned(&PIN)
                    .width(theme.field_width_md)
                    .show(ui, theme, &mut st.font);
            });
            cluster(ui, theme, "Checkbox on a ground", |ui| {
                for (ground, name) in crate::catalog::spec::GROUND_CELLS {
                    crate::catalog::spec::on_ground(ui, theme, ground, |ui| {
                        checkbox(ui, theme, &mut false, name, true, ground);
                        checkbox(ui, theme, &mut false, "disabled", false, ground);
                    });
                }
            });
            cluster(ui, theme, "Checkbox", |ui| {
                checkbox(
                    ui,
                    theme,
                    &mut st.check_a,
                    "Confirm on close",
                    true,
                    Ground::Panel,
                );
                checkbox(
                    ui,
                    theme,
                    &mut st.check_b,
                    "Restore layout",
                    true,
                    Ground::Panel,
                );
            });
            cluster(ui, theme, "Switch", |ui| {
                switch(ui, theme, &mut st.switch_a, Some("Ligatures"), true);
                switch(ui, theme, &mut st.switch_b, Some("Reduced motion"), true);
            });
        });
    });
    select_states(ui, theme, false);
    select_states(ui, theme, true);

    meta(
        ui,
        theme,
        &[
            ("height", "28 control-height"),
            ("checkbox", "16px square"),
            ("switch", "28×16 track"),
            ("accent", "primary"),
            (
                "Select ink",
                "value = text-primary · no value = select-placeholder · disabled (value or not) = state-disabled-fg, chevron state-disabled-fg (else text-muted) · open list options always text-primary · no hover/active overlay when disabled",
            ),
            (
                "searchable select",
                "same 28 trigger and chevron as Select; open list has a search field (see SearchableSelect)",
            ),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "checked fill",
                egui::Color32::from(theme.accent_primary()),
            ),
            TokenChip::new(
                "surface-raised",
                "control fill",
                egui::Color32::from(theme.surface_raised()),
            ),
            TokenChip::without_color("border-default", "outline"),
        ],
    );
}

/// 시안 "Select states": Mocha·Latte 칸마다 여섯 상태를 field-width-lg 칸에 고정해 그린다.
/// hover·open은 포인터로만 생기므로 본체 Select와 같은 칠을 쓰는 `select_pinned`로 고정한다.
/// `raised`면 surface-raised 위 ground="raised" 줄이다 — value · focus/open · no value · disabled와
/// 비교용 panel ground 칸.
fn select_states(ui: &mut egui::Ui, theme: &Theme, raised: bool) {
    const VALUE: &str = "Tasty rc";
    const PLACEHOLDER: &str = "Select a profile…";
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    let panel: [(&str, Option<&str>, SelectLook, bool, Ground); 6] = [
        (
            "value",
            Some(VALUE),
            SelectLook::Normal,
            true,
            Ground::Panel,
        ),
        ("hover", Some(VALUE), SelectLook::Hover, true, Ground::Panel),
        (
            "focus / open",
            Some(VALUE),
            SelectLook::Open,
            true,
            Ground::Panel,
        ),
        (
            "no value → select-placeholder",
            None,
            SelectLook::Normal,
            true,
            Ground::Panel,
        ),
        (
            "disabled + value → state-disabled-fg",
            Some(VALUE),
            SelectLook::Normal,
            false,
            Ground::Panel,
        ),
        (
            "disabled + no value → state-disabled-fg",
            None,
            SelectLook::Normal,
            false,
            Ground::Panel,
        ),
    ];
    let on_raised: [(&str, Option<&str>, SelectLook, bool, Ground); 5] = [
        (
            "value",
            Some(VALUE),
            SelectLook::Normal,
            true,
            Ground::Raised,
        ),
        (
            "focus / open",
            Some(VALUE),
            SelectLook::Open,
            true,
            Ground::Raised,
        ),
        ("no value", None, SelectLook::Normal, true, Ground::Raised),
        (
            "disabled",
            Some(VALUE),
            SelectLook::Normal,
            false,
            Ground::Raised,
        ),
        (
            "panel ground (vanishes)",
            Some(VALUE),
            SelectLook::Normal,
            true,
            Ground::Panel,
        ),
    ];
    let cells: &[(&str, Option<&str>, SelectLook, bool, Ground)] =
        if raised { &on_raised } else { &panel };
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
        for (label, th) in &themes {
            let (fill, ink, head) = if raised {
                (
                    th.surface_raised(),
                    th.text_secondary_raised(),
                    format!(
                        "{label} — Select ground=\"raised\": box bg-panel + select-raised-border, hover border-control-raised; ink and focus unchanged"
                    ),
                )
            } else {
                (
                    th.bg_panel(),
                    th.text_muted_raised(),
                    format!(
                        "{label} — Select states: hover / focus are pinned with the same border values the widget applies"
                    ),
                )
            };
            egui::Frame::new()
                .fill(fill.to_egui())
                .corner_radius(th.corner_radius.value())
                .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                        ui.label(
                            egui::RichText::new(head)
                                .size(th.font_size_caption.value())
                                .color(ink.to_egui()),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing =
                                egui::vec2(th.spacing_md.value(), th.spacing_md.value());
                            for &(cell, value, look, enabled, ground) in cells {
                                // 칸의 크기를 미리 알아야 줄을 바꾼다 — 지난 프레임 크기를 쓰는 wrap_item.
                                wrap_item(ui, |ui| {
                                    ui.set_width(th.field_width_lg.value());
                                    ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
                                    ui.label(
                                        egui::RichText::new(cell)
                                            .size(th.font_size_micro.value())
                                            .color(ink.to_egui()),
                                    );
                                    select_pinned(
                                        ui,
                                        th,
                                        ground,
                                        value,
                                        PLACEHOLDER,
                                        th.field_width_lg,
                                        look,
                                        enabled,
                                    );
                                });
                            }
                        });
                    });
                });
        }
    });
}
