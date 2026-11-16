//! 창 전체를 쓰는 별도 화면. 기존 workspace/pane/tab/surface 레이아웃을 바꾸지 않는다(docs/ai-verification/screenshot-methods.md).
//! 창당 하나의 StageState를 두며 정적 StageDef에 등록된 ID만 연다.
//! 콘텐츠별 임시 상태는 egui 메모리에 두고 on_close에서 정리한다.

pub mod defs;
pub(crate) mod notifications;

use crate::state::MainViewState;

pub use crate::fullscreen_stages::{StageId, StageMeta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageAction {
    None,
    Close,
}

pub struct StageDef {
    /// 공용 메타데이터를 복제하지 않고 참조한다.
    pub meta: &'static StageMeta,
    /// 공용 배경·제목 안쪽 콘텐츠를 그린다.
    pub draw_fn: fn(
        &mut egui::Ui,
        &mut MainViewState,
        &crate::runtime::engine_read::EngineRead<'_>,
    ) -> StageAction,
    /// 닫기 큐에서 호출할 정리 훅. 임시 egui 상태를 지울 수 있도록 Context를 받는다.
    pub on_close: Option<
        fn(&egui::Context, &mut MainViewState, &crate::runtime::engine_read::EngineRead<'_>),
    >,
}

impl StageDef {
    pub fn id(&self) -> StageId {
        self.meta.id
    }

    pub fn title_key(&self) -> &'static str {
        self.meta.title_key
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageState {
    pub id: StageId,
}

/// 훅이 다른 화면을 열고 닫는 재진입으로 무한 반복하지 않게 회차를 제한한다.
const ON_CLOSE_DRAIN_MAX_ROUNDS: u32 = 8;

/// 닫힌 다음에는 일반 프레임이 되므로 전체화면·일반 그리기 경로 모두에서 처리한다.
pub fn drain_on_close_hooks(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    let mut round = 0u32;
    loop {
        let queue = std::mem::take(&mut state.stage_closed_queue);
        if queue.is_empty() {
            return;
        }
        round += 1;
        if round > ON_CLOSE_DRAIN_MAX_ROUNDS {
            tracing::warn!(
                "fullscreen stage on_close drain exceeded {ON_CLOSE_DRAIN_MAX_ROUNDS} rounds; \
                 dropping {} pending id(s)",
                queue.len()
            );
            return;
        }
        for id in queue {
            if let Some(hook) = defs::find(id).and_then(|def| def.on_close) {
                hook(ctx, state, engine);
            }
        }
    }
}

/// 활성 전체화면을 Foreground Area에 등록해 정상 레이어·입력 순서를 사용한다.
/// 이 프레임에서는 일반 UI와 팝업을 그리지 않는다.
pub fn draw_fullscreen_stage(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    drain_on_close_hooks(ctx, state, engine);
    let Some(id) = state.fullscreen_stage_id() else {
        return;
    };
    let Some(def) = defs::find(id) else {
        tracing::warn!("fullscreen stage '{id}' has no definition; closing");
        state.close_fullscreen_stage();
        return;
    };

    let th = crate::theme::theme();
    let screen = ctx.screen_rect();
    let mut action = StageAction::None;
    egui::Area::new(egui::Id::new("fullscreen_stage"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            ui.painter().rect_filled(screen, 0.0, th.scrim().to_egui());
            let title = crate::i18n::t(def.title_key());
            ui.painter().text(
                egui::pos2(screen.center().x, screen.top() + th.spacing_xl.value()),
                egui::Align2::CENTER_TOP,
                title,
                egui::FontId::proportional(th.font_size_heading.value()),
                th.text_primary().to_egui(),
            );
            if draw_exit_button(ui, screen) {
                action = StageAction::Close;
            }
            let mut content = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect(screen))
                    .id_salt(def.id()),
            );
            let content_action = (def.draw_fn)(&mut content, state, engine);
            if content_action == StageAction::Close {
                action = StageAction::Close;
            }
        });

    if action == StageAction::Close {
        state.close_fullscreen_stage();
        // 닫은 뒤 일반 화면을 그리도록 다음 프레임을 요청한다.
        ctx.request_repaint();
    }
}

/// 제목·종료 버튼과 여백을 제외한 콘텐츠 영역.
fn content_rect(screen: egui::Rect) -> egui::Rect {
    let th = crate::theme::theme();
    let pad = th.spacing_xl.value();
    let chrome_h = pad + th.font_size_heading.value() + th.spacing_lg.value();
    egui::Rect::from_min_max(
        egui::pos2(screen.left() + pad, screen.top() + chrome_h),
        egui::pos2(screen.right() - pad, screen.bottom() - pad),
    )
}

/// 일반 창 닫기 버튼도 없는 화면이므로 종료 버튼은 공용으로 제공한다.
fn draw_exit_button(ui: &mut egui::Ui, screen: egui::Rect) -> bool {
    use tasty_ui_widgets::{ControlSize, IconButton, IconButtonVariant, top_right_inset_square};

    let th = crate::theme::theme();
    let pad = th.spacing_xl.value();
    let side = ControlSize::Md.height(&th);
    let rect = top_right_inset_square(screen, pad, side);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .size(ControlSize::Md)
            .show(ui, &th, &|ui, rect, c| {
                crate::adapters::ui::icons::CLOSE
                    .image(rect.height(), c)
                    .paint_at(ui, rect);
            })
            .on_hover_text(crate::i18n::t("fullscreen.stage.exit_tooltip"))
            .clicked()
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_def_has_a_unique_id() {
        let mut ids: Vec<StageId> = defs::all_defs().iter().map(|d| d.id()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(
            before,
            ids.len(),
            "무대 id 는 정적 테이블 안에서 고유해야 한다"
        );
    }

    // 팝업이 가리키는 전체화면 정의가 있어야 한다. 타이틀바 없는 팝업에는 전환 버튼을 둘 수 없다.
    #[test]
    fn popup_declared_stages_exist_and_are_not_headless() {
        for def in crate::adapters::ui::popup::defs::all_defs() {
            let Some(stage) = def.fullscreen_stage else {
                continue;
            };
            assert!(
                defs::find(stage).is_some(),
                "popup '{}' 이 선언한 무대 '{stage}' 가 무대 테이블에 없다",
                def.id
            );
            assert!(
                !def.decl.is_headless(),
                "headless popup '{}' 은 타이틀바가 없어 전체화면 버튼을 그릴 자리가 없다",
                def.id
            );
        }
    }

    #[test]
    fn find_rejects_unknown_ids() {
        assert!(defs::find("no-such-stage").is_none());
        for def in defs::all_defs() {
            assert!(defs::find(def.id()).is_some());
        }
    }
}
