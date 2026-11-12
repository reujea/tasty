//! 승인 레코드의 읽기 사본을 표시하고 응답 의도를 App으로 보낸다.
//! 큐가 비면 닫으며, 표시 함수는 갤러리에서도 상태 저장소 없이 사용할 수 있다.

use tasty_approval::{ApprovalRecord, Severity};
use tasty_type_geometry::length::LogicalPx;

use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;
use tasty_ui_widgets::vspace;

pub const APPROVAL_POPUP_ID: &str = "approval";

const DEFAULT_WIDTH: LogicalPx = LogicalPx(480.0);
const MIN_HEIGHT: LogicalPx = LogicalPx(180.0);
const MAX_HEIGHT: LogicalPx = LogicalPx(480.0);

/// PopupDef.title_fn — 큐 head 의 title 을 popup 타이틀로 사용.
pub fn approval_popup_title(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> String {
    let Some(id) = state.dialogs.pending_approval_ids.front() else {
        return t("approval.popup.title").to_string();
    };
    state
        .dialogs
        .approval_records
        .get(id)
        .map(|r| r.request.title.clone())
        .unwrap_or_else(|| t("approval.popup.title").to_string())
}

/// PopupDef.sizer — body 길이 + 선택지 수에 따라 height 추정.
pub fn approval_popup_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let Some(id) = state.dialogs.pending_approval_ids.front() else {
        return egui::vec2(DEFAULT_WIDTH.value(), MIN_HEIGHT.value());
    };
    let record = state.dialogs.approval_records.get(id);
    let body_len = record
        .as_ref()
        .and_then(|r| r.request.body.as_deref())
        .map(|s| s.chars().count())
        .unwrap_or(0);
    let choice_count = record
        .as_ref()
        .map(|r| r.request.choices.len())
        .unwrap_or(2);
    let approx_lines = (body_len as f32 / 60.0).ceil().max(1.0);
    let body_h = approx_lines * theme::theme().font_size_body.value() * 1.5;
    let buttons_h = (choice_count as f32 / 3.0).ceil() * 32.0;
    let total_h = (popup::title_bar_height()
        + popup::content_margin().scaled(2.0)
        + LogicalPx(body_h)
        + LogicalPx(32.0)
        + LogicalPx(buttons_h)
        + LogicalPx(56.0))
    .min(MAX_HEIGHT)
    .max(MIN_HEIGHT);
    egui::vec2(DEFAULT_WIDTH.value(), total_h.value())
}

/// View 입력 — 한 선택지의 시각/의미 데이터.
#[derive(Debug, Clone)]
pub struct ApprovalChoiceView {
    /// 응답 시 store 로 전달되는 식별자.
    pub key: String,
    /// 버튼에 표시되는 라벨.
    pub label: String,
    /// 위험 강조 표시 여부 (빨간 배경).
    pub destructive: bool,
}

/// 승인 화면의 입력. 코멘트 버퍼는 호출부에서 빌려 쓴다.
pub struct ApprovalProps<'a> {
    /// 요청 id 를 헤더에 함께 표시.
    pub id: String,
    pub severity: Severity,
    /// 사전 번역된 severity 라벨 (예: "INFO" / "WARN" / "DANGER").
    pub severity_label: String,
    /// 사전 번역된 comment 입력 위 라벨.
    pub comment_label: String,
    /// 사전 번역된 comment 입력 placeholder.
    pub comment_hint: String,
    /// 본문. None 이면 body block 자체를 그리지 않는다.
    pub body: Option<String>,
    /// 선택지 (1 개 이상). 빈 vec 이면 응답 불가능 상태로 그려진다.
    pub choices: Vec<ApprovalChoiceView>,
    /// 코멘트 입력 버퍼. View 가 `TextEdit` 으로 직접 mutate.
    pub comment_buffer: &'a mut String,
    /// 팝업이 키보드 포커스를 가졌는지. 거짓이면 숫자 키는 다른 곳의 입력이므로 선택으로 읽지 않는다.
    pub keyboard_focused: bool,
}

/// 코멘트 입력칸의 egui ID. 승인 팝업은 하나만 열린다.
const COMMENT_FIELD_ID: &str = "approval.comment";

/// View 의 출력 — 사용자 의도. wrapper 가 store/queue mutation 으로 변환.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalViewAction {
    None,
    /// 사용자가 한 선택지를 마우스 클릭 또는 1..=9 숫자 키로 선택. 숫자 키는 팝업이 키보드
    /// 포커스를 갖고 코멘트 입력칸에는 포커스가 없을 때만 선택이다.
    Chosen {
        key: String,
    },
}

/// 클릭과 1..=9 키 입력을 선택 결과로 반환한다. Escape는 응답으로 처리하지 않는다.
/// 코멘트 입력 중의 숫자는 글자다. egui의 `TextEdit`은 Key 이벤트를 소비하지 않으므로
/// 입력칸 포커스를 직접 확인한다.
pub fn draw_approval_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    props: &mut ApprovalProps<'_>,
) -> ApprovalViewAction {
    let sev_color = match props.severity {
        Severity::Info => theme.text_muted().to_egui(),
        Severity::Warn => theme.accent_warning_raised().to_egui(),
        Severity::Danger => theme.accent_danger_raised().to_egui(),
    };
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&props.severity_label)
                .size(theme.font_size_caption.value())
                .color(sev_color),
        );
        ui.label(
            egui::RichText::new(format!("· {}", props.id))
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    });
    vspace(ui, theme.spacing_sm);

    if let Some(body) = &props.body {
        ui.label(
            egui::RichText::new(body)
                .color(theme.text_primary().to_egui())
                .size(theme.font_size_body.value()),
        );
        vspace(ui, theme.spacing_sm);
    }

    ui.label(
        egui::RichText::new(&props.comment_label)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
    let comment = ui.add(
        egui::TextEdit::singleline(props.comment_buffer)
            .id(egui::Id::new(COMMENT_FIELD_ID))
            .desired_width(ui.available_width())
            .hint_text(tasty_egui_theme::hint_text(
                theme,
                props.comment_hint.clone(),
            )),
    );
    let digits_choose = props.keyboard_focused && !comment.has_focus();
    vspace(ui, theme.spacing_sm);

    let mut action = ApprovalViewAction::None;
    ui.horizontal_wrapped(|ui| {
        for (idx, choice) in props.choices.iter().enumerate() {
            let label_text = if idx < 9 {
                format!("{} ({})", choice.label, idx + 1)
            } else {
                choice.label.clone()
            };
            if tasty_ui_widgets::approval_choice(ui, theme, &label_text, choice.destructive)
                .clicked()
            {
                action = ApprovalViewAction::Chosen {
                    key: choice.key.clone(),
                };
            }
        }
    });

    let pressed_num = digits_choose
        .then(|| {
            ui.ctx().input(|i| {
                for n in 1..=9u8 {
                    let key = match n {
                        1 => egui::Key::Num1,
                        2 => egui::Key::Num2,
                        3 => egui::Key::Num3,
                        4 => egui::Key::Num4,
                        5 => egui::Key::Num5,
                        6 => egui::Key::Num6,
                        7 => egui::Key::Num7,
                        8 => egui::Key::Num8,
                        9 => egui::Key::Num9,
                        _ => continue,
                    };
                    if i.key_pressed(key) {
                        return Some(n as usize);
                    }
                }
                None
            })
        })
        .flatten();
    if let Some(idx) = pressed_num
        && let Some(choice) = props.choices.get(idx - 1)
    {
        action = ApprovalViewAction::Chosen {
            key: choice.key.clone(),
        };
    }

    action
}

/// 승인 레코드를 번역한 화면 입력으로 바꾼다.
fn props_from_record<'a>(
    record: &ApprovalRecord,
    comment_buffer: &'a mut String,
    keyboard_focused: bool,
) -> ApprovalProps<'a> {
    let severity_label_key = match record.request.severity {
        Severity::Info => "approval.severity.info",
        Severity::Warn => "approval.severity.warn",
        Severity::Danger => "approval.severity.danger",
    };
    ApprovalProps {
        id: record.request.id.to_string(),
        severity: record.request.severity,
        severity_label: t(severity_label_key).to_string(),
        comment_label: t("approval.popup.comment_label").to_string(),
        comment_hint: t("approval.popup.comment_hint").to_string(),
        body: record.request.body.clone(),
        choices: record
            .request
            .choices
            .iter()
            .map(|c| ApprovalChoiceView {
                key: c.key.clone(),
                label: c.label.clone(),
                destructive: c.destructive,
            })
            .collect(),
        comment_buffer,
        keyboard_focused,
    }
}

/// 팝업이 닫히면 코멘트 버퍼를 비운다. 승인 큐는 유지하며, 요청이 남아 있으면
/// OpenPopup을 보낸다. 실제로 다시 여는 시점은 다음 intent 처리 때다. 에이전트 출처로
/// 다시 열므로 키보드 포커스를 가져가지 않는다.
pub fn on_close_approval_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.dialogs.approval_comment_buffer.clear();
    if !state.dialogs.pending_approval_ids.is_empty() {
        state.dispatch_intent(
            crate::intent::UiIntent::OpenPopup {
                id: APPROVAL_POPUP_ID,
                mode: crate::intent::OpenPopupMode::WithScope(
                    crate::adapters::ui::popup::PopupScope::Window,
                ),
            }
            .from_agent_ipc(),
        );
    }
}

/// 큐의 첫 요청을 그리고 사용자 응답을 저장한다.
pub fn draw_approval_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let Some(current_id) = state.dialogs.pending_approval_ids.front().cloned() else {
        return PopupAction::Close;
    };

    let Some(record) = state.dialogs.approval_records.get(&current_id).cloned() else {
        state.dialogs.pending_approval_ids.pop_front();
        state.dialogs.approval_records.remove(&current_id);
        state.dialogs.approval_submitting = None;
        state.dialogs.approval_comment_buffer.clear();
        return PopupAction::Close;
    };

    // 다음 요청은 닫은 뒤 다시 연다. 앞 요청에 답하느라 준 포커스를 에이전트의 다음 요청이
    // 이어받지 않는다. 다시 여는 일은 on_close 훅이 맡는다.
    if record.state.is_terminal() {
        state.dialogs.pending_approval_ids.pop_front();
        state.dialogs.approval_records.remove(&current_id);
        state.dialogs.approval_submitting = None;
        state.dialogs.approval_comment_buffer.clear();
        return PopupAction::Close;
    }

    let th = theme::theme();
    let available = ui.available_rect_before_wrap();
    let inner_rect = available.shrink2(egui::vec2(th.spacing_sm.value(), th.spacing_xs.value()));
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect));
    let ui = &mut child_ui;

    let keyboard_focused = state.popup_keyboard_focus.contains(&APPROVAL_POPUP_ID);
    let mut props = props_from_record(
        &record,
        &mut state.dialogs.approval_comment_buffer,
        keyboard_focused,
    );
    let view_action = draw_approval_view(ui, &theme::theme(), &mut props);

    if let ApprovalViewAction::Chosen { key: choice_key } = view_action {
        let comment = if state.dialogs.approval_comment_buffer.trim().is_empty() {
            None
        } else {
            Some(state.dialogs.approval_comment_buffer.trim().to_string())
        };
        if state.dialogs.approval_submitting.is_none() {
            state.dialogs.approval_submitting = Some(current_id.clone());
            state.dispatch_intent(
                crate::intent::Intent::RespondApproval {
                    request_id: current_id,
                    choice: choice_key,
                    comment,
                }
                .from_user_menu("approval.respond"),
            );
        }
    }

    PopupAction::None
}

/// 중복되지 않은 승인 요청을 큐에 넣고 팝업 열기와 알림을 요청한다.
pub fn enqueue_approval(
    state: &mut MainViewState,
    engine: &crate::core::CoreState,
    record: &ApprovalRecord,
) {
    state
        .dialogs
        .approval_records
        .insert(record.request.id.clone(), record.clone());
    if state
        .dialogs
        .pending_approval_ids
        .iter()
        .any(|id| id == &record.request.id)
    {
        return;
    }
    state
        .dialogs
        .pending_approval_ids
        .push_back(record.request.id.clone());
    // 에이전트·플러그인 요청이므로 사용자 조작으로 처리하지 않는다. 팝업은 포커스 없이 열린다.
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup {
            id: APPROVAL_POPUP_ID,
            mode: crate::intent::OpenPopupMode::WithScope(
                crate::adapters::ui::popup::PopupScope::Window,
            ),
        }
        .from_agent_ipc(),
    );

    let severity_prefix = match record.request.severity {
        Severity::Info => "",
        Severity::Warn => "[WARN] ",
        Severity::Danger => "[DANGER] ",
    };
    let body = record
        .request
        .body
        .clone()
        .unwrap_or_else(|| t("approval.notification.body_fallback").to_string());
    let workspace = record.request.workspace_id.unwrap_or(0);
    let surface = record.request.surface_id.unwrap_or(0);
    let _ = engine; // 옛 직접 add 경로 제거 — cascade 가 라우팅 + add + host event 일괄 처리.
    state.dispatch_intent(
        crate::app::command::DomainIntent::PushNotification {
            level: crate::notification::NotificationLevel::Info,
            ws_id: workspace,
            surface_id: surface,
            title: format!("{severity_prefix}{}", record.request.title),
            body,
            source: "host".to_string(),
            merge: crate::app::command::NotificationMerge::Coalesce,
        }
        .from_agent_ipc(),
    );
}

#[cfg(test)]
mod focus_tests;

#[cfg(test)]
mod props_tests {
    use super::*;
    use tasty_approval::{ApprovalChoice, ApprovalId, ApprovalRequest, ApprovalState, Requester};

    fn mk_record(
        severity: Severity,
        body: Option<&str>,
        choices: Vec<ApprovalChoice>,
    ) -> ApprovalRecord {
        let request = ApprovalRequest {
            id: ApprovalId("test-1".to_string()),
            requester: Requester::Plugin {
                id: "test".to_string(),
            },
            workspace_id: None,
            surface_id: None,
            title: "Test approval".to_string(),
            body: body.map(|s| s.to_string()),
            choices,
            default_choice: None,
            timeout_ms: None,
            severity,
            created_at: 0,
            metadata: serde_json::Value::Null,
            host_elevation: None,
        };
        ApprovalRecord {
            request,
            state: ApprovalState::Pending,
            history: vec![],
        }
    }

    #[test]
    fn props_carry_id_and_severity() {
        let mut buf = String::new();
        let rec = mk_record(Severity::Warn, None, vec![ApprovalChoice::approve()]);
        let props = props_from_record(&rec, &mut buf, true);
        assert_eq!(props.id, "test-1");
        assert_eq!(props.severity, Severity::Warn);
        assert_eq!(props.choices.len(), 1);
        assert_eq!(props.choices[0].key, "approve");
    }

    #[test]
    fn props_preserve_destructive_flag() {
        let mut buf = String::new();
        let rec = mk_record(
            Severity::Danger,
            Some("body text"),
            vec![ApprovalChoice::approve(), ApprovalChoice::deny()],
        );
        let props = props_from_record(&rec, &mut buf, true);
        assert_eq!(props.body.as_deref(), Some("body text"));
        assert!(!props.choices[0].destructive);
        assert!(props.choices[1].destructive);
    }

    #[test]
    fn props_empty_body_becomes_none() {
        let mut buf = String::new();
        let rec = mk_record(Severity::Info, None, vec![ApprovalChoice::approve()]);
        let props = props_from_record(&rec, &mut buf, true);
        assert!(props.body.is_none());
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use tasty_themes::mocha_fallback;

    /// 1 frame egui Context 안에서 view 함수를 호출하고 결과 action 을 받는다.
    fn run_view<F>(props_builder: F) -> ApprovalViewAction
    where
        F: FnOnce() -> (Vec<ApprovalChoiceView>, Option<egui::Key>),
    {
        let ctx = egui::Context::default();
        let mut action = ApprovalViewAction::None;
        let (choices, pressed_key) = props_builder();
        let mut buf = String::new();
        let theme = mocha_fallback();

        let mut raw_input = egui::RawInput::default();
        if let Some(key) = pressed_key {
            raw_input.events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }

        let _full_output = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut props = ApprovalProps {
                    id: "abc".to_string(),
                    severity: Severity::Info,
                    severity_label: "INFO".to_string(),
                    comment_label: "Comment".to_string(),
                    comment_hint: "Hint".to_string(),
                    body: Some("body".to_string()),
                    choices: choices.clone(),
                    comment_buffer: &mut buf,
                    keyboard_focused: true,
                };
                action = draw_approval_view(ui, &theme, &mut props);
            });
        });
        action
    }

    #[test]
    fn no_input_returns_none() {
        let action = run_view(|| {
            (
                vec![ApprovalChoiceView {
                    key: "approve".to_string(),
                    label: "Approve".to_string(),
                    destructive: false,
                }],
                None,
            )
        });
        assert_eq!(action, ApprovalViewAction::None);
    }

    #[test]
    fn num1_key_chooses_first_choice() {
        let action = run_view(|| {
            (
                vec![
                    ApprovalChoiceView {
                        key: "approve".to_string(),
                        label: "Approve".to_string(),
                        destructive: false,
                    },
                    ApprovalChoiceView {
                        key: "deny".to_string(),
                        label: "Deny".to_string(),
                        destructive: true,
                    },
                ],
                Some(egui::Key::Num1),
            )
        });
        assert_eq!(
            action,
            ApprovalViewAction::Chosen {
                key: "approve".to_string()
            }
        );
    }

    #[test]
    fn num2_key_chooses_second_choice() {
        let action = run_view(|| {
            (
                vec![
                    ApprovalChoiceView {
                        key: "approve".to_string(),
                        label: "Approve".to_string(),
                        destructive: false,
                    },
                    ApprovalChoiceView {
                        key: "deny".to_string(),
                        label: "Deny".to_string(),
                        destructive: true,
                    },
                ],
                Some(egui::Key::Num2),
            )
        });
        assert_eq!(
            action,
            ApprovalViewAction::Chosen {
                key: "deny".to_string()
            }
        );
    }

    fn two_choices() -> Vec<ApprovalChoiceView> {
        vec![
            ApprovalChoiceView {
                key: "approve".to_string(),
                label: "Approve".to_string(),
                destructive: false,
            },
            ApprovalChoiceView {
                key: "deny".to_string(),
                label: "Deny".to_string(),
                destructive: true,
            },
        ]
    }

    fn num2_events(raw: &mut egui::RawInput) {
        raw.events.push(egui::Event::Key {
            key: egui::Key::Num2,
            physical_key: Some(egui::Key::Num2),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        raw.events.push(egui::Event::Text("2".to_string()));
    }

    /// 같은 Context로 두 프레임을 그린다. 첫 프레임 뒤 `focus_comment`면 코멘트 칸에 포커스를
    /// 주고, 둘째 프레임에 Num2 Key와 Text "2"를 함께 넣는다. 둘째 프레임의 결과와 버퍼를 돌려준다.
    fn run_digit_frames(
        keyboard_focused: bool,
        focus_comment: bool,
    ) -> (ApprovalViewAction, String) {
        let ctx = egui::Context::default();
        let theme = mocha_fallback();
        let choices = two_choices();
        let mut buf = String::new();
        let mut action = ApprovalViewAction::None;
        let frame = |raw: egui::RawInput, buf: &mut String, action: &mut ApprovalViewAction| {
            drop(ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut props = ApprovalProps {
                        id: "abc".to_string(),
                        severity: Severity::Info,
                        severity_label: "INFO".to_string(),
                        comment_label: "Comment".to_string(),
                        comment_hint: "Hint".to_string(),
                        body: None,
                        choices: choices.clone(),
                        comment_buffer: buf,
                        keyboard_focused,
                    };
                    *action = draw_approval_view(ui, &theme, &mut props);
                });
            }));
        };
        frame(egui::RawInput::default(), &mut buf, &mut action);
        if focus_comment {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new(COMMENT_FIELD_ID)));
            frame(egui::RawInput::default(), &mut buf, &mut action);
            assert!(ctx.memory(|m| m.has_focus(egui::Id::new(COMMENT_FIELD_ID))));
        }
        let mut raw = egui::RawInput::default();
        num2_events(&mut raw);
        frame(raw, &mut buf, &mut action);
        (action, buf)
    }

    #[test]
    fn digit_typed_into_focused_comment_is_text_not_a_choice() {
        let (action, buf) = run_digit_frames(true, true);
        assert_eq!(action, ApprovalViewAction::None);
        assert_eq!(buf, "2");
    }

    #[test]
    fn digit_chooses_when_popup_focused_and_comment_is_not() {
        let (action, buf) = run_digit_frames(true, false);
        assert_eq!(
            action,
            ApprovalViewAction::Chosen {
                key: "deny".to_string()
            }
        );
        assert!(buf.is_empty());
    }

    #[test]
    fn digit_is_ignored_while_popup_lacks_keyboard_focus() {
        let (action, _) = run_digit_frames(false, false);
        assert_eq!(action, ApprovalViewAction::None);
    }

    #[test]
    fn out_of_range_num_key_ignored() {
        let action = run_view(|| {
            (
                vec![ApprovalChoiceView {
                    key: "approve".to_string(),
                    label: "Approve".to_string(),
                    destructive: false,
                }],
                Some(egui::Key::Num5),
            )
        });
        assert_eq!(action, ApprovalViewAction::None);
    }
}
