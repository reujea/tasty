//! 확장 사이드바와 축소 레일이 같은 카테고리 편집 팝업을 열도록 공유하는 진입점.

use crate::adapters::ui::popup::confirm_delete_category::CONFIRM_DELETE_CATEGORY_POPUP_ID;
use crate::intent::UiIntent;
use crate::model::WorkspaceCategoryId;
use crate::state::{MainViewState, RenameTarget};

pub(crate) fn open_new_category_dialog(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    let target = RenameTarget::NewCategory;
    crate::adapters::ui::rename_popup::open(state, engine, target, String::new(), |intent| {
        intent.from_user_menu("category/new")
    });
}

pub(crate) fn open_rename_category_dialog(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    cat_id: WorkspaceCategoryId,
) {
    let name = engine.category_name(cat_id).unwrap_or_default().to_string();
    let target = RenameTarget::CategoryName { cat_id };
    crate::adapters::ui::rename_popup::open(state, engine, target, name, |intent| {
        intent.from_user_menu("category/rename")
    });
}

/// 삭제 확인을 연다. 이미 열려 있으면 대상만 새 카테고리로 바꾼다.
pub(crate) fn open_delete_category_confirm(state: &mut MainViewState, cat_id: WorkspaceCategoryId) {
    state.dispatch_intent(
        UiIntent::ReopenPopup(
            crate::intent::OpenRequest::new(
                CONFIRM_DELETE_CATEGORY_POPUP_ID,
                crate::model::popup_kind::PopupScope::Window,
            )
            .centered()
            .focused()
            .with_payload(crate::adapters::ui::popup::PopupPayload::ConfirmDeleteCategory(cat_id)),
        )
        .from_user_menu("category/delete"),
    );
}
