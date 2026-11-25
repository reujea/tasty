//! 탐색기 파일·폴더 이름 변경 팝업 예제. 본체는 rename 팝업을 해당 서피스 중앙에 연다.
//! 즐겨찾기 추가와 같은 공용 view 를 쓰고 제목·초기 파일명·확인 버튼만 다르다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t;

/// 이름 변경 팝업 카드 한 장. 콘텐츠는 본체와 같은 공용 view 다.
pub(super) fn card(ui: &mut egui::Ui, theme: &Theme) {
    super::rename_popup::card(
        ui,
        theme,
        super::rename_popup::Card {
            layout: tasty_ui_widgets::RenamePopupLayout::Explorer,
            title: t("explorer.popup.rename.title"),
            save: t("explorer.popup.rename.rename"),
            name: "photo.png",
            error: None,
            waiting: None,
        },
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| card(ui, theme));

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "360 × ui scale · dialog-bg · 1px border-strong"),
            ("title", "heading semibold"),
            ("input", "28 · input-bg · seeded current name"),
            ("footer", "right — Cancel · Rename (primary) · button sm"),
        ],
        &[
            TokenChip::new("dialog-bg", "frame", theme.dialog_bg().to_egui()),
            TokenChip::new("input-bg", "input", theme.input_bg().to_egui()),
            TokenChip::new(
                "accent-primary",
                "Save button",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::without_color("shadow-modal", "popup lift"),
        ],
    );

    spec::note(
        ui,
        theme,
        "The host opens this popup in the selected surface scope. It shares the Add-to-favorites \
         frame, with a different title, current file name including its extension, and button label.",
    );
}
