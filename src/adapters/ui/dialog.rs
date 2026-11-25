//! 이름 변경 팝업. 콘텐츠는 갤러리 예제와 같은 `tasty_ui_widgets::rename_popup` 이 그리고,
//! 이 모듈은 대상 검사·원격 대기·실제 이름 변경을 맡는다.

use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::t;
use crate::state::{MainViewState, RenameTarget};
use crate::theme;
use crate::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    RenamePopupAction, RenamePopupLayout, RenamePopupView, rename_popup, rename_popup_base_height,
};

/// 배율 전 기본 폭. 시안 `RenameFrame`·`RenamePending` 의 `size-360` 이며 역할 토큰이 없는 화면 전용
/// 고정 치수라 UI 배율만 곱한다(docs/design/systems/theme.md).
const POPUP_WIDTH: LogicalPx = LogicalPx(360.0);

/// 폭은 기본 폭에 UI 배율을 곱한 값이다. 높이는 직전 프레임에 그린 콘텐츠 높이에 타이틀바를 더한다.
/// 콘텐츠 여백은 공용 view 가 넣어 잰 높이에 들어 있으므로 공통 내부 여백은 이 popup 에 적용하지
/// 않는다(`content_rect`). 아직 재지 않은 첫 프레임은 오류·대기 줄이 없는 콘텐츠 높이를 쓴다.
pub fn rename_popup_sizer(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &crate::adapters::ui::popup::PopupPayload,
) -> egui::Vec2 {
    let rename = crate::adapters::ui::rename_popup::payload_of(payload);
    let layout = rename.map_or(RenamePopupLayout::Dialog, |r| rename_layout(&r.target));
    rename_popup_size(
        &theme::theme(),
        rename.and_then(|r| r.content_height),
        layout,
    )
}

pub fn rename_popup_default_size() -> egui::Vec2 {
    rename_popup_size(&theme::theme(), None, RenamePopupLayout::Dialog)
}

/// 탐색기 항목·즐겨찾기 추가는 시안 `RenamePending`, 나머지는 `RenameFrame`·`CategoryEditFrame` 치수다.
fn rename_layout(target: &RenameTarget) -> RenamePopupLayout {
    match target {
        RenameTarget::ExplorerEntry { .. } | RenameTarget::ExplorerAddFavorite { .. } => {
            RenamePopupLayout::Explorer
        }
        _ => RenamePopupLayout::Dialog,
    }
}

fn rename_popup_size(
    th: &Theme,
    content: Option<LogicalPx>,
    layout: RenamePopupLayout,
) -> egui::Vec2 {
    let width = crate::adapters::ui::zoomed_px(th, POPUP_WIDTH);
    let content = content.unwrap_or_else(|| rename_popup_base_height(th, layout));
    egui::vec2(width.value(), (popup::title_bar_height() + content).value())
}

pub fn rename_popup_title(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &crate::adapters::ui::popup::PopupPayload,
) -> String {
    crate::adapters::ui::rename_popup::payload_of(payload)
        .map(|r| t(r.target.heading_key()).to_string())
        .unwrap_or_else(|| t("rename_dialog.tab_heading").to_string())
}

pub fn draw_rename_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let th = theme::theme();

    let crate::adapters::ui::popup::PopupPayload::Rename(rename) = cx.payload else {
        return PopupAction::Close;
    };
    let target = &rename.target;

    if !rename_target_exists(target, engine) || rename.waiting.as_ref().is_some_and(|w| w.done) {
        return PopupAction::Close;
    }
    let waiting = rename.waiting.as_ref().map(|w| renaming_line(&w.host));
    // 확인을 기다리는 동안 Esc 는 무시한다. 그 Esc 를 쓴 것으로 남겨 중앙 단계가 닫지 않게 한다.
    if waiting.is_some() {
        cx.keys
            .consume(crate::adapters::ui::popup::keys::BasicKey::Escape);
    }

    // 직전 프레임의 입력으로 카테고리 이름과 탐색기 항목 이름을 검사한다. 다른 대상에는 이 제한이 없다.
    let (validation_error, save_enabled) = match target {
        RenameTarget::ExplorerEntry {
            surface_id, path, ..
        } => explorer_validation(
            &rename.buffer,
            explorer_name_rules(engine, *surface_id, path),
        ),
        _ => category_validation(target, &rename.buffer, engine),
    };

    let save_label = match target {
        RenameTarget::ExplorerAddFavorite { .. } => t("explorer.popup.add_favorite.add"),
        RenameTarget::ExplorerEntry { .. } => t("explorer.popup.rename.rename"),
        _ => t("button.save"),
    };
    let view = RenamePopupView {
        layout: rename_layout(target),
        save: save_label,
        cancel: t("button.cancel"),
        error: validation_error.as_deref(),
        save_enabled,
        waiting: waiting.as_deref(),
        // 키 입력과 입력 포커스는 포커스를 가진 인스턴스만 가져간다. agent가 연 passive 인스턴스는 빼앗지 않는다.
        autofocus: cx.keys.receives(),
        keys: cx.keys.receives(),
        // 소비하지 않는다: 입력칸(TextEdit)도 지금처럼 같은 Enter 를 본다.
        confirm: cx
            .keys
            .pressed(crate::adapters::ui::popup::keys::BasicKey::Confirm),
    };
    let drawn = ui.scope(|ui| rename_popup(ui, &th, &view, &mut rename.buffer));
    let measured = Some(LogicalPx(drawn.response.rect.height()));
    // 오류·대기 줄이 생기거나 사라진 프레임은 직전 높이로 잘려 있으므로 새 높이로 바로 다시 그린다.
    if rename.content_height != measured {
        ui.ctx().request_repaint();
    }
    rename.content_height = measured;

    match drawn.inner {
        RenamePopupAction::None => PopupAction::None,
        RenamePopupAction::Cancel => PopupAction::Close,
        RenamePopupAction::Confirm(buffer) => {
            match request_remote_rename(state, engine, &rename.target, &buffer) {
                Some(Some(waiting)) => {
                    rename.waiting = Some(waiting);
                    return PopupAction::None;
                }
                // 받지 않은 이유는 그 칸이 알린다.
                Some(None) => return PopupAction::Close,
                None => {}
            }
            apply_rename(state, engine, rename.target.clone(), buffer);
            PopupAction::Close
        }
    }
}

/// 원격 탐색기 항목의 이름 변경이면 요청을 넣는다. 원격이 아니면 None 이고 호출자가 로컬과 같이
/// 처리한다. 원격이면 받은 요청의 기다림을, 받지 않았거나 보낼 수 없는 이름이면 `Some(None)` 을 돌려준다.
fn request_remote_rename(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &RenameTarget,
    buffer: &str,
) -> Option<Option<crate::state::RenameWaiting>> {
    let RenameTarget::ExplorerEntry {
        surface_id,
        path,
        binding,
    } = target
    else {
        return None;
    };
    let surface_id = *surface_id;
    engine.mirror_workspace_of(surface_id)?;
    let windows = explorer_name_rules(engine, surface_id, path);
    let name = buffer.trim();
    if !binding.current_in(engine)
        || name.is_empty()
        || explorer_name_error(buffer, windows).is_some()
    {
        return Some(None);
    }
    let host = state
        .explorer_views
        .get(surface_id)
        .and_then(|v| v.remote_host.as_ref())
        .map(|h| h.label.clone())
        .unwrap_or_default();
    let operation = crate::app::explorer_files::Operation::Rename {
        path: path.clone(),
        name: name.to_owned(),
    };
    let request = state.request_explorer_file_numbered(
        engine,
        surface_id,
        operation,
        crate::intent::IntentOrigin::User {
            source: crate::intent::UserSource::Menu("rename_popup"),
        },
    );
    Some(request.map(|request| crate::state::RenameWaiting {
        request,
        host,
        done: false,
    }))
}

/// 탐색기 항목 이름은 새 항목 입력과 같은 규칙으로 검사하고 이유를 보인다.
fn explorer_validation(buffer: &str, windows: bool) -> (Option<String>, bool) {
    match explorer_name_error(buffer, windows) {
        Some(error) => (Some(error), false),
        None => (None, true),
    }
}

/// 이름 변경에 Windows 이름 규칙을 쓰는가. 원격 항목은 그 경로의 형식을 따른다.
fn explorer_name_rules(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
    path: &std::path::Path,
) -> bool {
    crate::adapters::ui::surface::explorer::create::windows_rules(
        path,
        engine.mirror_workspace_of(surface_id).is_some(),
    )
}

/// 이름 바꾸기 대화상자의 이름 검사. 새 항목 입력과 같은 규칙(빈 이름·금지 글자·앞뒤 공백·예약 이름)이다.
/// 문제가 없으면 `None`, 있으면 보일 문구다. 이미 있는지는 실제 이름 바꾸기가 알린다.
fn explorer_name_error(name: &str, windows: bool) -> Option<String> {
    crate::adapters::ui::surface::explorer::create::check_name(name, windows)
        .err()
        .map(|e| e.message(name))
}

/// 빈 카테고리 이름은 확인만 막고 오류 문구는 숨긴다. rename에서 자기 이름은 중복으로 보지 않는다.
fn category_validation(
    target: &RenameTarget,
    buffer: &str,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> (Option<String>, bool) {
    let result = match target {
        RenameTarget::NewCategory => {
            let existing: Vec<&str> = engine
                .categories()
                .iter()
                .map(|c| c.name.as_str())
                .collect();
            crate::model::validate_new_category_name(buffer, existing)
        }
        RenameTarget::CategoryName { cat_id } => {
            let existing: Vec<&str> = engine
                .categories()
                .iter()
                .filter(|c| c.id != *cat_id)
                .map(|c| c.name.as_str())
                .collect();
            crate::model::validate_rename_category_name(buffer, existing)
        }
        _ => return (None, true),
    };
    match result {
        Ok(_) => (None, true),
        Err(e) => {
            let text = if buffer.trim().is_empty() {
                None
            } else {
                Some(t(e.i18n_key()).to_string())
            };
            (text, false)
        }
    }
}

/// 이름 바꾸기 팝업의 대상이 아직 있는지. 범위가 대상의 상위 자리라 범위만으로는 대상 소멸을 알 수 없어
/// 팝업 정리 단계가 그리기와 무관하게 인스턴스마다 매 프레임 부른다. 데이터가 없으면 거짓이다(그리기도 닫는다).
pub(crate) fn rename_popup_target_alive(
    payload: &crate::adapters::ui::popup::PopupPayload,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    crate::adapters::ui::rename_popup::payload_of(payload)
        .is_some_and(|r| rename_target_exists(&r.target, engine))
}

fn rename_target_exists(
    target: &RenameTarget,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> bool {
    match target {
        RenameTarget::WorkspaceName { workspace_id }
        | RenameTarget::WorkspaceSubtitle { workspace_id } => {
            engine.find_workspace_index_for_id(*workspace_id).is_some()
        }
        RenameTarget::TabName { tab_id } => engine.find_pane_for_tab(*tab_id).is_some(),
        // 원격 항목은 이 컴퓨터에서 확인할 수 없다. 없으면 서버의 이름 변경이 실패로 알린다.
        RenameTarget::ExplorerEntry {
            path,
            binding,
            surface_id,
        } => {
            binding.current_in(engine)
                && (engine.mirror_workspace_of(*surface_id).is_some() || path.exists())
        }
        RenameTarget::ExplorerAddFavorite { path } => path.exists(),
        RenameTarget::NewCategory => true,
        RenameTarget::CategoryName { cat_id } => engine.category_index(*cat_id).is_some(),
    }
}

fn apply_rename(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: RenameTarget,
    buffer: String,
) {
    match target {
        RenameTarget::WorkspaceName { workspace_id } => {
            apply_rename_workspace_name(state, engine, workspace_id, buffer)
        }
        RenameTarget::WorkspaceSubtitle { workspace_id } => {
            apply_rename_workspace_subtitle(state, engine, workspace_id, buffer)
        }
        RenameTarget::TabName { tab_id } => apply_rename_tab_name(state, engine, tab_id, buffer),
        RenameTarget::ExplorerEntry {
            surface_id,
            path,
            binding,
        } => {
            // 팝업이 떠 있는 동안 surface 가 바뀌었으면 새 surface 에 요청하지 않는다.
            // 확인 버튼은 직전 프레임 입력으로 막으므로, 같은 프레임의 Enter 도 여기서 다시 검사한다.
            let windows = explorer_name_rules(engine, surface_id, &path);
            if binding.current_in(engine) && explorer_name_error(&buffer, windows).is_none() {
                apply_rename_explorer_entry(state, engine, surface_id, path, buffer)
            }
        }
        RenameTarget::ExplorerAddFavorite { path } => {
            apply_rename_explorer_add_favorite(state, path, buffer)
        }
        RenameTarget::NewCategory => apply_rename_new_category(state, buffer),
        RenameTarget::CategoryName { cat_id } => apply_rename_category_name(state, cat_id, buffer),
    }
}

fn apply_rename_workspace_name(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    workspace_id: u32,
    buffer: String,
) {
    if buffer.is_empty() {
        return;
    }
    if engine.find_workspace_index_for_id(workspace_id).is_some() {
        state.dispatch_intent(
            crate::intent::Intent::DirectRename(
                crate::intent::rename::DirectRename::WorkspaceName {
                    workspace_id,
                    name: buffer,
                },
            )
            .from_user_menu("rename_popup"),
        );
    }
}

fn apply_rename_workspace_subtitle(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    workspace_id: u32,
    buffer: String,
) {
    if engine.find_workspace_index_for_id(workspace_id).is_some() {
        state.dispatch_intent(
            crate::intent::Intent::DirectRename(
                crate::intent::rename::DirectRename::WorkspaceSubtitle {
                    workspace_id,
                    subtitle: buffer,
                },
            )
            .from_user_menu("rename_popup"),
        );
    }
}

fn apply_rename_tab_name(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    tab_id: u32,
    buffer: String,
) {
    if engine.find_pane_for_tab(tab_id).is_none() {
        return;
    }
    // 빈 이름은 사용자 이름을 지우고 선택된 surface의 제목으로 돌아가게 한다.
    let name = buffer.trim();
    let name = (!name.is_empty()).then(|| name.to_string());
    state.dispatch_intent(
        crate::intent::Intent::DirectRename(crate::intent::rename::DirectRename::TabName {
            tab_id,
            name,
        })
        .from_user_menu("rename_popup"),
    );
}

fn apply_rename_explorer_entry(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
    path: std::path::PathBuf,
    buffer: String,
) {
    let name = buffer.trim();
    if !name.is_empty() {
        state.request_explorer_file(
            engine,
            surface_id,
            crate::app::explorer_files::Operation::Rename {
                path,
                name: name.to_owned(),
            },
            crate::intent::IntentOrigin::User {
                source: crate::intent::UserSource::Menu("rename_popup"),
            },
        );
    }
}

fn apply_rename_explorer_add_favorite(
    state: &mut MainViewState,
    path: std::path::PathBuf,
    label: String,
) {
    state.dispatch_intent(
        crate::intent::Intent::Engine(
            crate::app::engine_action::EngineAction::AddExplorerFavorite { path, label },
        )
        .from_user_context_menu(),
    );
}

fn apply_rename_new_category(state: &mut MainViewState, buffer: String) {
    state.dispatch_intent(
        crate::app::command::DomainIntent::CreateCategory { name: buffer }
            .from_user_menu("rename_popup"),
    );
}

fn apply_rename_category_name(
    state: &mut MainViewState,
    cat_id: crate::model::WorkspaceCategoryId,
    buffer: String,
) {
    state.dispatch_intent(
        crate::app::command::DomainIntent::RenameCategory {
            id: cat_id,
            name: buffer,
        }
        .from_user_menu("rename_popup"),
    );
}

/// 원격 이름 바꾸기를 기다리는 팝업의 대기 줄. 호스트 글자가 비면(프로필 이름이 없는 attach) 자리표시
/// 없는 문구다.
pub(crate) fn renaming_line(host: &str) -> String {
    if host.is_empty() {
        crate::i18n::t("explorer.remote.renaming_no_host").to_string()
    } else {
        crate::i18n::t_fmt("explorer.remote.renaming", host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> crate::runtime::engine_session::EngineSession {
        engine_with_state().1
    }

    fn engine_with_state() -> (
        crate::state::RequestContext,
        crate::runtime::engine_session::EngineSession,
    ) {
        use tasty_core::DomainEvent as E;
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::CategoryCreated {
                id: 1,
                name: "Services".into(),
                index: 1,
            },
            E::CategoryCreated {
                id: 2,
                name: "Infra".into(),
                index: 2,
            },
        ]))
    }

    #[test]
    fn category_validation_new_category_rules() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let (err, ok) = category_validation(&RenameTarget::NewCategory, "  ", &e.read());
        assert!(!ok && err.is_none());
        let (err, ok) = category_validation(&RenameTarget::NewCategory, "normal", &e.read());
        assert!(!ok && err.is_some());
        let (err, ok) = category_validation(&RenameTarget::NewCategory, "services", &e.read());
        assert!(!ok && err.is_some());
        let (err, ok) = category_validation(&RenameTarget::NewCategory, "Other", &e.read());
        assert!(ok && err.is_none());
    }

    #[test]
    fn category_validation_rename_allows_self_name() {
        let mut e_session = engine();
        let e = e_session.borrow_mut();
        let id = 1;
        let (err, ok) = category_validation(
            &RenameTarget::CategoryName { cat_id: id },
            "SERVICES",
            &e.read(),
        );
        assert!(ok && err.is_none());
        let (err, ok) = category_validation(
            &RenameTarget::CategoryName { cat_id: id },
            "Infra",
            &e.read(),
        );
        assert!(!ok && err.is_some());
    }

    /// 그린 콘텐츠 높이가 다음 프레임 popup 높이가 된다: 오류 줄이 생기면 늘고 사라지면 돌아온다.
    /// popup 콘텐츠 영역(공통 여백 없음)이 이 높이라 버튼 줄이 잘리지 않는다.
    #[test]
    fn the_rename_popup_height_follows_the_drawn_content() {
        crate::i18n::init("en");
        let (mut state, session) = engine_with_state();
        let th = theme::theme();
        use crate::adapters::ui::popup::PopupPayload;
        use crate::adapters::ui::rename_popup::{RenamePayload, payload_of};
        let draw = |state: &mut crate::state::RequestContext,
                    session: &crate::runtime::engine_session::EngineSession,
                    payload: &mut PopupPayload| {
            let ctx = egui::Context::default();
            for _ in 0..2 {
                drop(ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_width(rename_popup_default_size().x);
                        draw_rename_popup(
                            ui,
                            state,
                            &session.read(),
                            &mut crate::adapters::ui::popup::PopupCx {
                                id: "rename",
                                instance: crate::adapters::ui::popup::PopupInstanceId::NONE,
                                focused: false,
                                keys: crate::adapters::ui::popup::keys::PopupKeys::new(
                                    ctx,
                                    false,
                                    crate::adapters::ui::popup::PopupKeyboard::Text,
                                    &crate::adapters::ui::input::edit_key::EditBindings::default(),
                                ),
                                payload: &mut *payload,
                            },
                        );
                    });
                }));
            }
            payload_of(payload)
                .and_then(|r| r.content_height)
                .expect("measured")
        };
        let category = |name: &str| {
            PopupPayload::Rename(Box::new(RenamePayload::new(
                RenameTarget::NewCategory,
                name.into(),
            )))
        };
        assert_eq!(
            rename_popup_sizer(&state, &session.read(), &category("Ops")),
            rename_popup_default_size(),
            "unmeasured: base content"
        );
        assert_eq!(
            rename_popup_default_size().x,
            crate::adapters::ui::zoomed_px(&th, POPUP_WIDTH).value()
        );

        let plain = draw(&mut state, &session, &mut category("Ops"));
        assert_eq!(
            plain,
            rename_popup_base_height(&th, RenamePopupLayout::Dialog)
        );
        let mut with_error = category("normal");
        let error = draw(&mut state, &session, &mut with_error);
        assert!(error > plain, "{error:?} > {plain:?}");
        assert_eq!(
            rename_popup_sizer(&state, &session.read(), &with_error).y,
            (popup::title_bar_height() + error).value()
        );
        assert_eq!(
            draw(&mut state, &session, &mut category("Web")),
            plain,
            "no leftover room"
        );
    }

    /// rename 인스턴스들을 한 프레임씩 그린다. 인스턴스마다 마지막 프레임의 결과와, 그린 뒤 egui 입력 포커스가
    /// 있는지를 돌려준다.
    fn draw_renames(focused: &[bool], frames: Vec<Vec<egui::Event>>) -> (Vec<&'static str>, bool) {
        draw_renames_on(&egui::Context::default(), focused, frames)
    }

    /// [`draw_renames`] 를 `ctx` 로 그린다.
    fn draw_renames_on(
        ctx: &egui::Context,
        focused: &[bool],
        frames: Vec<Vec<egui::Event>>,
    ) -> (Vec<&'static str>, bool) {
        crate::i18n::init("en");
        let (mut state, session) = engine_with_state();
        use crate::adapters::ui::popup::PopupPayload;
        use crate::adapters::ui::rename_popup::RenamePayload;
        let mut payloads: Vec<PopupPayload> = focused
            .iter()
            .map(|_| {
                PopupPayload::Rename(Box::new(RenamePayload::new(
                    RenameTarget::NewCategory,
                    "Ops".into(),
                )))
            })
            .collect();
        let mut results = vec!["none"; focused.len()];
        for events in frames {
            let raw = egui::RawInput {
                events,
                ..Default::default()
            };
            drop(ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for (i, payload) in payloads.iter_mut().enumerate() {
                        ui.push_id(i, |ui| {
                            let action = draw_rename_popup(
                                ui,
                                &mut state,
                                &session.read(),
                                &mut crate::adapters::ui::popup::PopupCx {
                                    id: "rename",
                                    instance: crate::adapters::ui::popup::PopupInstanceId::NONE,
                                    focused: focused[i],
                                    keys: crate::adapters::ui::popup::keys::PopupKeys::new(ctx, focused[i], crate::adapters::ui::popup::PopupKeyboard::Text, &crate::adapters::ui::input::edit_key::EditBindings::default()),
                                    payload: &mut *payload,
                                },
                            );
                            results[i] = match action {
                                PopupAction::None => "none",
                                PopupAction::CloseBy(
                                    crate::adapters::ui::popup::PopupCloseReason::Escape,
                                ) => "escape",
                                _ => "closed",
                            };
                        });
                    }
                });
            }));
        }
        (results, ctx.memory(|m| m.focused()).is_some())
    }

    /// 포커스를 가진 rename 하나가 입력 포커스를 얻은 뒤 `held` 를 붙인 Enter 를 누른 프레임의 결과.
    fn rename_on_enter(
        held: Option<crate::adapters::ui::popup::window_input_test_support::Held>,
    ) -> &'static str {
        let ctx = egui::Context::default();
        let press = crate::adapters::ui::popup::window_input_test_support::held_press(
            &ctx,
            egui::Key::Enter,
            held,
        );
        draw_renames_on(&ctx, &[true], vec![vec![], vec![], vec![press]]).0[0]
    }

    /// T-9a: Given 입력 포커스를 가진 rename, When 수식키 없는 Enter 를 누르면, Then 저장하고 닫는다.
    #[test]
    fn bare_enter_saves_the_rename() {
        assert_eq!(rename_on_enter(None), "closed");
    }

    /// T-9a: Given 입력 포커스를 가진 rename, When Shift·Ctrl·Alt·Cmd(비-macOS Super)+Enter 를 누르면, Then 저장하지
    /// 않는다.
    #[test]
    fn modified_enter_is_ignored_by_the_rename() {
        for held in crate::adapters::ui::popup::window_input_test_support::Held::ALL {
            assert_eq!(rename_on_enter(Some(held)), "none", "{held:?}");
        }
    }

    fn escape() -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: Some(egui::Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }

    /// Given rename 둘이 보이고 한쪽만 포커스를 가졌을 때, When Escape를 누르면, Then 두 본문 모두 닫지 않는다.
    /// 본문은 Esc 를 읽지 않는다. 닫기는 팝업 층 중앙 단계가 선언대로 정한다(T-4a3-25).
    #[test]
    fn the_rename_bodies_leave_escape_to_the_popup_layer() {
        let (results, _) = draw_renames(&[false, true], vec![vec![], escape()]);
        assert_eq!(results, ["none", "none"]);
    }

    /// Given 포커스 없는(agent가 연 passive 같은) rename 하나만 보일 때, Then 입력 포커스를 가져가지 않는다.
    /// 포커스를 가진 rename은 입력 포커스를 가져간다.
    #[test]
    fn only_a_focused_rename_takes_the_input_focus() {
        assert!(!draw_renames(&[false], vec![vec![], vec![]]).1);
        assert!(draw_renames(&[true], vec![vec![], vec![]]).1);
    }

    fn text_center(out: &egui::FullOutput, text: &str) -> egui::Pos2 {
        out.shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::Text(g) if g.galley.text() == text => {
                    Some(g.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{text} drawn"))
    }

    /// 실제 popup 경로(`draw_popups` → `content_rect` clip)에서 여러 프레임을 그린다. 높이는 프레임마다
    /// 자라지 않고, 오류 줄을 더하고 빼도 저장/취소 글자와 버튼이 popup 의 실제 clip 안에 그대로 보인다.
    #[test]
    fn the_real_popup_clip_keeps_the_footer_through_error_changes() {
        use crate::model::{PhysicalPx, PhysicalRect};
        crate::i18n::init("en");
        let (mut state, mut session) = crate::state::tests::test_state();
        let engine = session.borrow_mut();
        let ctx = egui::Context::default();
        let term = PhysicalRect {
            x: PhysicalPx(0.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(1920.0),
            height: PhysicalPx(1080.0),
        };
        crate::adapters::ui::rename_popup::open_for_test(
            &mut state,
            crate::adapters::ui::rename_popup::RenamePayload::new(
                RenameTarget::NewCategory,
                "Ops".into(),
            ),
            crate::model::popup_kind::PopupScope::Window,
            Some(egui::pos2(100.0, 100.0)),
        );
        let frame = |state: &mut crate::state::RequestContext| {
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                crate::adapters::ui::draw_popups(ctx, state, &engine.read(), &[], term, 1.0);
            });
            let size = state.popups.get_mut("rename").expect("open").size;
            let repaint = out
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|v| v.repaint_delay.is_zero());
            (out, size, repaint)
        };
        // 각 단계: 그린 모든 버튼 글자가 clip 에 잘리지 않고, 몇 프레임 뒤 높이가 멈춘다.
        let settle = |state: &mut crate::state::RequestContext, label: &str| {
            let mut sizes = Vec::new();
            let mut last = None;
            for _ in 0..5 {
                let (out, size, _) = frame(state);
                sizes.push(size);
                last = Some(out);
            }
            assert!(
                sizes[2..].windows(2).all(|w| w[0] == w[1]),
                "{label}: height keeps changing {sizes:?}"
            );
            let out = last.unwrap();
            for text in [t("button.save"), t("button.cancel")] {
                let (r, clip) = out
                    .shapes
                    .iter()
                    .find_map(|c| match &c.shape {
                        egui::Shape::Text(g) if g.galley.text() == text => {
                            Some((g.visual_bounding_rect(), c.clip_rect))
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{label}: {text} drawn"));
                assert!(
                    clip.contains_rect(r),
                    "{label}: {text} {r:?} clipped by {clip:?}"
                );
            }
            // 저장 버튼 상자 전체와 그 아래 하단 여백(space-md)이 실제 clip 안에 남는다.
            let save = text_center(&out, t("button.save"));
            let (button, clip) = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Rect(r) if r.fill.a() > 0 && r.rect.contains(save) => {
                        Some((r.rect, c.clip_rect))
                    }
                    _ => None,
                })
                .min_by(|a, b| a.0.area().total_cmp(&b.0.area()))
                .expect("the primary button box is drawn");
            assert!(
                clip.bottom() - button.bottom() >= theme::theme().spacing_md.value(),
                "{label}: no room under the button {button:?} in {clip:?}"
            );
            *sizes.last().unwrap()
        };
        let plain = settle(&mut state, "plain");
        let title = popup::title_bar_height().value();
        assert_eq!(
            plain.y,
            title + rename_popup_base_height(&theme::theme(), RenamePopupLayout::Dialog).value()
        );

        crate::adapters::ui::rename_popup::set_buffer_for_test(&mut state, "normal");
        let (_, stale, repaint) = frame(&mut state);
        assert_eq!(
            stale, plain,
            "the first error frame still uses the old height"
        );
        assert!(repaint, "the grown content asks for an immediate repaint");
        let error = settle(&mut state, "error");
        assert!(error.y > plain.y);

        crate::adapters::ui::rename_popup::set_buffer_for_test(&mut state, "Web");
        let (_, _, repaint) = frame(&mut state);
        assert!(repaint, "the shrunk content asks for an immediate repaint");
        assert_eq!(
            settle(&mut state, "error removed"),
            plain,
            "no leftover room"
        );
    }

    #[test]
    fn an_explorer_rename_uses_the_new_item_name_rules() {
        crate::i18n::init("en");
        assert_eq!(explorer_validation("b.txt", false), (None, true));
        assert_eq!(
            explorer_validation("b.txt ", false),
            (
                Some("Names can't start or end with a space.".to_string()),
                false
            )
        );
        assert!(!explorer_validation("   ", false).1);

        let (mut state, session) = crate::state::tests::test_state();
        let sid = session.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let binding =
            crate::runtime::surface_binding::SurfaceBinding::capture(&session.read(), sid).unwrap();
        let target = RenameTarget::ExplorerEntry {
            surface_id: sid,
            path: crate::test_support::abs_path("proj/a.txt"),
            binding,
        };
        apply_rename(&mut state, &session.read(), target, " b.txt".into());
        assert_eq!(
            state.explorer_file_requests.len(),
            0,
            "a name the dialog refuses is never sent, even on the frame it was typed"
        );
    }

    #[test]
    fn an_explorer_rename_is_dropped_when_its_surface_changed_while_the_popup_was_open() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let sid = session.read().workspace_at(0).unwrap().all_surface_ids()[0];
        let path = crate::test_support::abs_path("proj/a.txt");
        let target = |binding| RenameTarget::ExplorerEntry {
            surface_id: sid,
            path: path.clone(),
            binding,
        };
        let opened =
            crate::runtime::surface_binding::SurfaceBinding::capture(&session.read(), sid).unwrap();

        session.borrow_mut().runtime.terminals.insert(
            sid,
            tasty_terminal::Terminal::new_detached(80, 24),
            None,
        );
        apply_rename(&mut state, &session.read(), target(opened), "b.txt".into());
        assert_eq!(state.explorer_file_requests.len(), 0);

        let current =
            crate::runtime::surface_binding::SurfaceBinding::capture(&session.read(), sid).unwrap();
        apply_rename(&mut state, &session.read(), target(current), "b.txt".into());
        assert_eq!(
            state.explorer_file_requests.len(),
            1,
            "an unchanged surface is renamed"
        );
    }
}
