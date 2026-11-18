//! 등록된 Lua 스크립트와 현재 해시가 다르면 실행 전 확인한다(docs/features/hooks/index.md).
//! 사용자가 실행을 확정해야 dispatch_pending_script_confirm이 해시를 저장하고 워커를 시작한다.

use crate::adapters::ui::popup::{self, PopupAction, PopupPayload};
use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{ScriptConfirmView, script_confirm};

/// Pure view 의 입력. MainViewState/CoreState 를 알지 못한다.
pub struct ScriptConfirmProps<'a> {
    pub theme: &'a Theme,
    /// 변경된 스크립트 표시 이름.
    pub name: &'a str,
}

/// view 가 보고하는 사용자 의도.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptConfirmAction {
    None,
    /// Escape — 취소로 간주(실행 안 함).
    Close,
    /// Cancel 버튼 — 실행 안 함.
    Cancel,
    /// Run 버튼 — 변경본을 실행하고 해시를 갱신.
    Run,
}

/// 순수 view. MainViewState/CoreState 접근 금지.
pub fn draw_script_confirm_view(
    ui: &mut egui::Ui,
    props: &ScriptConfirmProps<'_>,
) -> ScriptConfirmAction {
    let th = props.theme;

    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return ScriptConfirmAction::Close;
    }

    let out = script_confirm(
        ui,
        th,
        &ScriptConfirmView {
            title: t("script.confirm.title"),
            name: props.name,
            changed_tag: t("script.confirm.changed_tag"),
            body: t("script.confirm.body"),
            run: t("script.confirm.run"),
            cancel: t("button.cancel"),
        },
    );
    if out.run {
        ScriptConfirmAction::Run
    } else if out.cancel {
        ScriptConfirmAction::Cancel
    } else {
        ScriptConfirmAction::None
    }
}

pub const SCRIPT_CONFIRM_POPUP_ID: &str = "script_changed_confirm";

/// 확인할 스크립트를 인스턴스 데이터로 실어 창 가운데에 연다.
pub(crate) fn open(state: &mut MainViewState, pending: crate::state::PendingScriptConfirm) {
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup(
            crate::intent::OpenRequest::new(
                SCRIPT_CONFIRM_POPUP_ID,
                crate::model::popup_kind::PopupScope::Window,
            )
            .centered()
            .focused()
            .with_payload(PopupPayload::ScriptConfirm(Box::new(pending))),
        )
        .from_user_menu("script_tofu_gate"),
    );
}

/// 확인이 열려 있거나 열릴 차례이거나, 실행 결정이 아직 처리되지 않았으면 다음 확인을 열지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn busy(state: &MainViewState) -> bool {
    state.dialogs.approved_script.is_some()
        || state.popups.is_open(SCRIPT_CONFIRM_POPUP_ID)
        || state.popup_open_queued(SCRIPT_CONFIRM_POPUP_ID)
}

/// 인스턴스가 확인하는 스크립트.
fn pending_of(payload: &PopupPayload) -> Option<&crate::state::PendingScriptConfirm> {
    match payload {
        PopupPayload::ScriptConfirm(p) => Some(p),
        _ => None,
    }
}

/// 실행으로 결정하고 닫혔으면 그 요청을 출력함에 옮긴다. 다음 프레임 시작에 App이 해시를 저장하고 실행한다.
/// 결정 없이 닫혔으면 요청은 인스턴스와 함께 버려진다.
pub fn on_close_script_confirm_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    if let Some(pending) = pending_of(&closed.payload)
        && pending.result == Some(true)
    {
        state.dialogs.approved_script = Some(pending.clone());
    }
}

/// 등록 크기. 본문 높이를 아직 재지 않은 첫 프레임에도 쓴다(배율 전 값).
const DEFAULT_SIZE: (LogicalPx, LogicalPx) = (LogicalPx(360.0), LogicalPx(152.0));

/// PopupDef.decl.size.
pub fn script_confirm_default_size() -> egui::Vec2 {
    egui::vec2(DEFAULT_SIZE.0.value(), DEFAULT_SIZE.1.value())
}

/// PopupDef.sizer — 폭은 기본 폭에 UI 배율을 곱한 값이다. 높이는 직전 프레임에 잰 콘텐츠 높이에
/// 타이틀바를 더한 값이다(최소값 없음). 콘텐츠 여백은 공용 위젯이 넣어 잰 높이에 들어 있으므로
/// 공통 내부 여백은 이 popup 에 적용하지 않는다(`content_rect`). 아직 재지 않은 첫 프레임만 기본 높이를 쓴다.
pub fn script_confirm_sizer(
    _state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &PopupPayload,
) -> egui::Vec2 {
    let th = theme::theme();
    let width = crate::adapters::ui::zoomed_px(&th, DEFAULT_SIZE.0);
    let measured = pending_of(payload).and_then(|p| p.content_height);
    let height = match measured {
        Some(content) => popup::title_bar_height() + content,
        None => crate::adapters::ui::zoomed_px(&th, DEFAULT_SIZE.1),
    };
    egui::vec2(width.value(), height.value())
}

/// PopupDef::draw_fn entry.
pub fn draw_script_confirm_popup(
    ui: &mut egui::Ui,
    _state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let PopupPayload::ScriptConfirm(pending) = cx.payload else {
        return PopupAction::Close;
    };
    let name = pending.name.clone();

    let action = {
        let theme_guard = theme::theme();
        let props = ScriptConfirmProps {
            theme: &theme_guard,
            name: &name,
        };
        let drawn = ui.scope(|ui| draw_script_confirm_view(ui, &props));
        pending.content_height = Some(LogicalPx(drawn.response.rect.height()));
        drawn.inner
    };

    match action {
        ScriptConfirmAction::None => PopupAction::None,
        ScriptConfirmAction::Close | ScriptConfirmAction::Cancel => PopupAction::Close,
        ScriptConfirmAction::Run => {
            // 결정 기록 — 닫기 훅이 출력함에 옮기고 frame begin 의 `dispatch_pending_script_confirm` 이 갱신·실행.
            pending.result = Some(true);
            PopupAction::Close
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_view_once(events: Vec<egui::Event>) -> ScriptConfirmAction {
        let ctx = egui::Context::default();
        let theme = tasty_themes::mocha_fallback();
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut captured = ScriptConfirmAction::None;
        let _full_output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let props = ScriptConfirmProps {
                    theme: &theme,
                    name: "deploy.lua",
                };
                captured = draw_script_confirm_view(ui, &props);
            });
        });
        captured
    }

    #[test]
    fn escape_returns_close() {
        let action = run_view_once(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_eq!(action, ScriptConfirmAction::Close);
    }

    #[test]
    fn no_input_returns_none() {
        assert_eq!(run_view_once(Vec::new()), ScriptConfirmAction::None);
    }

    /// 콘텐츠 폭(popup 폭 − 양쪽 콘텐츠 여백)보다 넓게 그리면 오른쪽이 잘린다. 본문이 한 줄로
    /// 이어지던 때는 버튼 행까지 그 폭으로 밀려 "Run anyway" 가 잘렸다.
    #[test]
    fn the_view_stays_within_the_content_width() {
        let ctx = egui::Context::default();
        let theme = tasty_themes::mocha_fallback();
        let width = 240.0;
        let mut drawn = egui::Rect::NOTHING;
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let props = ScriptConfirmProps {
                    theme: &theme,
                    name: "deploy.lua",
                };
                drawn = ui
                    .allocate_ui(egui::vec2(width, 600.0), |ui| {
                        draw_script_confirm_view(ui, &props)
                    })
                    .response
                    .rect;
            });
        }));
        assert!(
            drawn.width() <= width,
            "the view is {} wide in a {width} wide area",
            drawn.width()
        );
    }
}

#[cfg(test)]
mod sizer_wiring_tests {
    use super::*;
    use crate::adapters::ui::draw_popups;
    use crate::model::{PhysicalPx, PhysicalRect};
    use crate::state::tests::test_state;

    const ID: &str = SCRIPT_CONFIRM_POPUP_ID;

    fn run_one_frame(
        state: &mut MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        let ctx = egui::Context::default();
        let term = PhysicalRect {
            x: PhysicalPx(0.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(1920.0),
            height: PhysicalPx(1080.0),
        };
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            draw_popups(ctx, state, engine, &[], term, 1.0);
        }));
    }

    /// 확인을 고정 위치에 그 데이터와 함께 연다.
    fn open_at(state: &mut MainViewState, pending: crate::state::PendingScriptConfirm) {
        state.popups.open_request(
            &crate::intent::OpenRequest::new(ID, crate::model::popup_kind::PopupScope::Window)
                .at(egui::pos2(100.0, 100.0))
                .focused()
                .with_payload(PopupPayload::ScriptConfirm(Box::new(pending))),
            true,
        );
    }

    fn pending(content_height: Option<LogicalPx>) -> crate::state::PendingScriptConfirm {
        crate::state::PendingScriptConfirm {
            script_id: "script-0".to_string(),
            name: "deploy.lua".to_string(),
            source: String::new(),
            new_hash: String::new(),
            result: None,
            content_height,
        }
    }

    /// 실제 popup 높이는 잰 콘텐츠(위젯 여백 포함)에 타이틀바를 더한 값이다. 기본 높이보다 작아도 줄어든다.
    #[test]
    fn a_frame_sizes_the_popup_from_the_measured_content() {
        let th = theme::theme();
        let chrome = popup::title_bar_height();
        for (content, expected) in [
            (LogicalPx(400.0), (chrome + LogicalPx(400.0)).value()),
            (LogicalPx(10.0), (chrome + LogicalPx(10.0)).value()),
        ] {
            let (mut state, mut engine_session) = test_state();
            let engine = engine_session.borrow_mut();
            // 그리는 프레임이 잰 값으로 덮어쓰므로, sizer 가 먼저 읽는 값만 본다.
            open_at(&mut state, pending(Some(content)));
            run_one_frame(&mut state, &engine.read());
            let size = state.popups.get_mut(ID).expect("registered").size;
            assert_eq!(size.y, expected, "content {content:?}");
            assert_eq!(
                size.x,
                crate::adapters::ui::zoomed_px(&th, DEFAULT_SIZE.0).value()
            );
        }
    }

    /// 그린 프레임이 콘텐츠 높이를 재어 다음 프레임의 sizer 에 넘긴다.
    #[test]
    fn drawing_records_the_content_height() {
        let (mut state, mut engine_session) = test_state();
        let engine = engine_session.borrow_mut();
        open_at(&mut state, pending(None));
        run_one_frame(&mut state, &engine.read());
        let measured = state
            .popups
            .topmost_payload(ID)
            .and_then(pending_of)
            .and_then(|p| p.content_height)
            .expect("measured");
        assert!(measured.value() > 0.0);
    }

    /// Given 확인 요청이 처리 전 대기열에 있을 때, Then 다음 스크립트 확인을 열지 않는다. 열린 동안과 실행 결정이
    /// 처리되기 전에도 같다. 닫히고 결정이 처리되면 다시 열 수 있다.
    #[test]
    fn a_second_changed_script_waits_for_the_first_confirm() {
        let (mut state, mut session) = test_state();
        assert!(!busy(&state));
        open(&mut state, pending(None));
        assert!(busy(&state), "queued request");
        crate::intent::popup::apply_pending(&mut state, &mut session);
        assert!(busy(&state), "open instance");
        state.popups.close(ID);
        assert!(!busy(&state));
        state.dialogs.approved_script = Some(pending(None));
        assert!(busy(&state), "undispatched decision");
    }
}
