//! 워크스페이스·탭·부제·카테고리 이름 변경 팝업 예제.
//! 콘텐츠는 본체 popup 과 같은 `tasty_ui_widgets::rename_popup` 이 그린다. 제목 줄만 예제가 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    RenamePopupLayout, RenamePopupView, margin_sym, rename_pad_x, rename_popup,
};

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use crate::i18n::t;

/// `dialog.rs::rename_popup_sizer` 의 배율 전 기본 폭(시안 `size-360`).
pub(crate) const POPUP_WIDTH: LogicalPx = LogicalPx(360.0);

/// 예제 카드 한 장의 입력.
pub(crate) struct Card<'a> {
    pub layout: RenamePopupLayout,
    pub title: &'a str,
    pub save: &'a str,
    pub name: &'a str,
    pub error: Option<&'a str>,
    pub waiting: Option<&'a str>,
}

/// 예제 rename 팝업 한 장. 입력은 예제끼리 포커스를 다투지 않도록 자동 포커스를 끄고,
/// 고친 값은 다음 프레임에 처음 값으로 돌아간다.
pub(crate) fn card(ui: &mut egui::Ui, theme: &Theme, card: Card<'_>) {
    let Card {
        layout,
        title,
        save,
        name,
        error,
        waiting,
    } = card;
    let width = LogicalPx((POPUP_WIDTH.value() * theme.ui_zoom).round());
    // 제목 띠의 위 여백은 그 시안의 콘텐츠 위 여백과 같다.
    let top = match layout {
        RenamePopupLayout::Dialog => rename_pad_x(theme),
        RenamePopupLayout::Explorer => theme.spacing_md,
    };
    kit::frame_card(ui, theme, width, kit::dialog_fill(theme), |ui| {
        kit::region(
            ui,
            egui::Margin {
                bottom: 0,
                ..margin_sym(rename_pad_x(theme), top)
            },
            |ui| kit::title(ui, theme, title),
        );
        let mut buffer = name.to_owned();
        rename_popup(
            ui,
            theme,
            &RenamePopupView {
                layout,
                save,
                cancel: t("button.cancel"),
                error,
                save_enabled: error.is_none(),
                waiting,
                autofocus: false,
                keys: true,
                // 갤러리에는 팝업 키 통로가 없다. 예제는 Enter 를 그대로 확정 판정값으로 넘긴다.
                confirm: ui.input(|i| i.key_pressed(egui::Key::Enter)),
            },
            &mut buffer,
        );
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "Workspace title", |ui| {
            card(
                ui,
                theme,
                Card {
                    layout: RenamePopupLayout::Dialog,
                    title: t("rename_dialog.title_heading"),
                    save: t("button.save"),
                    name: "Planning",
                    error: None,
                    waiting: None,
                },
            )
        });
        spec::cluster(ui, theme, "Long name (scrolls inside the padding)", |ui| {
            card(
                ui,
                theme,
                Card {
                    layout: RenamePopupLayout::Dialog,
                    title: t("rename_dialog.tab_heading"),
                    save: t("button.save"),
                    name: "a-very-long-tab-name-that-scrolls-inside-the-field",
                    error: None,
                    waiting: None,
                },
            )
        });
        let latte = Theme::with_colors_and_zoom(
            crate::host_shell::latte_theme().to_colors(),
            true,
            theme.ui_zoom,
        );
        spec::cluster(ui, theme, "Latte", |ui| {
            card(
                ui,
                &latte,
                Card {
                    layout: RenamePopupLayout::Dialog,
                    title: t("rename_dialog.subtitle_heading"),
                    save: t("button.save"),
                    name: "기획 문서",
                    error: None,
                    waiting: None,
                },
            )
        });
        spec::cluster(ui, theme, "Validation error (wraps)", |ui| {
            card(
                ui,
                theme,
                Card {
                    layout: RenamePopupLayout::Dialog,
                    title: t("rename_dialog.category_heading"),
                    save: t("button.save"),
                    name: "normal",
                    error: Some(t("workspace_category.error.reserved")),
                    waiting: None,
                },
            )
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "360px × ui scale · dialog-bg"),
            (
                "padding",
                "explorer-props-padding-x (14) all sides and above the footer",
            ),
            (
                "input",
                "Input block · input-height 28 · autofocused + selected",
            ),
            (
                "error",
                "caption accent-danger · space-xs above (design 6, no role token)",
            ),
            ("footer", "Cancel · Save · button md"),
        ],
        &[
            TokenChip::new("dialog-bg", "frame", theme.dialog_bg().to_egui()),
            TokenChip::new("input-bg", "input", theme.input_bg().to_egui()),
            TokenChip::new(
                "input-border-invalid",
                "invalid edge",
                theme.input_border_invalid().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "One view serves workspace, subtitle, tab, category and explorer renames — only the \
         title, buffer and validation differ. The popup height is the drawn content, so the \
         footer stays inside at every UI scale and with an error or waiting line.",
    );
}
