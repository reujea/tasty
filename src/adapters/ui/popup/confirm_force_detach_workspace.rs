//! 사이드바에서 워크스페이스 점유를 강제로 해제하기 전 확인한다.
//! 작용 대상인 워크스페이스 범위로 연다. 그 워크스페이스가 활성일 때 그린다.
//! 서버는 SSH 터널 밖의 사용자를 식별할 수 없어 워크스페이스 이름과 해제 결과만 안내한다.
//! 대상이 사라졌거나 이미 점유가 풀렸으면 닫는다.

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::{t, t_fmt};
use crate::state::MainViewState;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;

pub const CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID: &str = "confirm_force_detach_workspace";

/// 다이얼로그 폭 (destructive confirm 공통 380px).
const WIDTH: LogicalPx = LogicalPx(380.0);

/// 대상 워크스페이스를 보류 상태에 두고 그 워크스페이스 범위로 확인을 연다.
/// 확인을 기다리는 동안 인덱스가 바뀔 수 있으므로 ID를 보관한다.
pub(crate) fn open(state: &mut MainViewState, workspace_id: u32) {
    state.dialogs.pending_force_detach_workspace = Some(workspace_id);
    crate::adapters::ui::workspace_action_popup::open(
        state,
        workspace_id,
        CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
        |intent| intent.from_user_menu("workspace/force-detach"),
    );
}

/// 화면에 표시할 대상 이름. 실제 해제 대상 ID는 실행할 때 보류 상태에서 다시 읽는다.
struct Target {
    name: String,
}

/// 대상이 없거나 점유가 이미 풀렸으면 None을 반환한다.
fn resolve_target(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Option<Target> {
    let ws_id = state.dialogs.pending_force_detach_workspace?;
    let ws = engine.workspaces().into_iter().find(|w| w.id == ws_id)?;
    engine.live.occupancy.workspace_holder(ws_id)?;
    Some(Target {
        name: ws.name.clone(),
    })
}

/// PopupDef.title_fn — headless 라 실제 타이틀바는 없지만, 접근성/디버그용 라벨.
pub fn confirm_force_detach_workspace_title(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> String {
    t("attach.force_detach_confirm_title").to_string()
}

/// 대상 이름을 뺀 안내문 자체의 길이. 대상이 아직 안 잡힌 상태의 기준이기도 하다.
const BASE_BODY_LEN: usize = 90;

/// 등록 크기와 sizer가 같은 계산을 써 첫 프레임의 크기를 맞춘다.
fn size_for(body_len: usize) -> egui::Vec2 {
    let approx_lines = (body_len as f32 / 42.0).ceil().max(2.0);
    let body_h = approx_lines * theme::theme().font_size_body.value() * 1.5;
    let content_h = 24.0 + body_h + 40.0;
    egui::vec2(
        WIDTH.value(),
        (popup::content_margin().scaled(2.0) + LogicalPx(content_h)).value(),
    )
}

/// PopupDef.decl.size — 등록 시점의 placeholder. 대상이 아직 없을 때 sizer 가 내는
/// 값과 **같은 식에서** 나온다.
pub fn confirm_force_detach_workspace_default_size() -> egui::Vec2 {
    size_for(BASE_BODY_LEN)
}

/// PopupDef.sizer — 본문 길이에 따라 height 조정(소형 모달).
pub fn confirm_force_detach_workspace_sizer(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let body_len = resolve_target(state, engine)
        .map(|tgt| tgt.name.chars().count() + BASE_BODY_LEN)
        .unwrap_or(BASE_BODY_LEN);
    size_for(body_len)
}

/// PopupDef::on_close entry point — 어떤 경로로 닫히든(취소/외부/Escape) 대상을 비운다.
pub fn on_close_confirm_force_detach_workspace(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.dialogs.pending_force_detach_workspace = None;
}

pub fn draw_confirm_force_detach_workspace(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let ctx = ui.ctx().clone();
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.dialogs.pending_force_detach_workspace = None;
        return PopupAction::Close;
    }
    let Some(target) = resolve_target(state, engine) else {
        state.dialogs.pending_force_detach_workspace = None;
        return PopupAction::Close;
    };
    let th = theme::theme();

    let margin = th.spacing_sm.value();
    let available = ui.available_rect_before_wrap();
    let inner_rect = available.shrink2(egui::vec2(margin, th.spacing_xs.value()));
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect));
    let ui = &mut child_ui;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
        let icon_size = th.icon_glyph_size_md.value();
        let (icon_rect, _) =
            ui.allocate_exact_size(egui::vec2(icon_size, icon_size), egui::Sense::hover());
        icons::CLOSE
            .image(icon_size, th.accent_danger().into())
            .paint_at(ui, icon_rect);
        ui.label(
            egui::RichText::new(t("attach.force_detach_confirm_title"))
                .color(th.text_primary())
                .size(th.font_size_body.value())
                .strong(),
        );
    });

    ui.add_space(th.spacing_sm.value());

    ui.label(
        egui::RichText::new(t_fmt("attach.force_detach_confirm_body", &target.name))
            .color(th.text_secondary())
            .size(th.font_size_body.value()),
    );

    let mut confirm = false;
    let mut cancel = false;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
        ui.add_space(th.spacing_sm.value());
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let go = egui::Button::new(
                    egui::RichText::new(t("attach.force_detach")).color(th.text_on_accent()),
                )
                .fill(th.accent_danger());
                if ui.add(go).clicked() {
                    confirm = true;
                }
                if ui.button(t("button.cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    });

    if cancel {
        state.dialogs.pending_force_detach_workspace = None;
        return PopupAction::Close;
    }
    if confirm {
        apply_force_detach(state, engine);
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 보류 대상의 점유를 해제하고 대상을 비운다. 실제 해제된 holder를 반환하며
/// 이미 풀렸으면 None이다. 버튼 클릭 재현 없이도 해제 동작을 검사할 수 있다.
pub(crate) fn apply_force_detach(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    if let Some(workspace) = state.dialogs.pending_force_detach_workspace.take()
        && let Some((_, lock)) = engine
            .live
            .occupancy
            .workspaces_snapshot()
            .into_iter()
            .find(|(id, _)| *id == workspace)
    {
        state.dispatch_intent(
            crate::intent::Intent::Engine(
                crate::app::engine_action::EngineAction::DetachWorkspace {
                    workspace,
                    holder: lock.holder,
                    grant: lock.granted_seq,
                },
            )
            .from_user_context_menu(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_confirm_opens_in_the_workspace_it_detaches() {
        let (mut state, _session) = crate::state::tests::two_pane_fixture();
        open(&mut state, 7);
        assert_eq!(state.dialogs.pending_force_detach_workspace, Some(7));
        let intents = state.take_pending_intents();
        assert!(
            matches!(
                &intents[..],
                [crate::intent::DispatchedIntent {
                    body: crate::intent::Intent::Ui(crate::intent::UiIntent::OpenPopup {
                        id: CONFIRM_FORCE_DETACH_WORKSPACE_POPUP_ID,
                        mode: crate::intent::OpenPopupMode::CenteredFocused(
                            crate::model::popup_kind::PopupScope::Workspace(7)
                        ),
                    }),
                    ..
                }]
            ),
            "{intents:?}"
        );
    }
}
