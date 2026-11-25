//! 승인 레코드의 읽기 사본을 표시하고 응답 의도를 App으로 보낸다.
//! 대기 목록은 창의 [`ApprovalInbox`]에 있고, 지금 보이는 요청과 코멘트는 인스턴스 데이터다.
//! 목록이 비면 닫으며, 표시 함수는 갤러리에서도 상태 저장소 없이 사용할 수 있다.

use std::collections::{HashMap, VecDeque};

use tasty_approval::{ApprovalId, ApprovalRecord, Severity};
use tasty_type_geometry::length::LogicalPx;

use crate::adapters::ui::popup::{self, PopupAction, PopupPayload};
use crate::i18n::t;
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;
use tasty_ui_widgets::vspace;

pub const APPROVAL_POPUP_ID: &str = "approval";

const MIN_HEIGHT: LogicalPx = LogicalPx(180.0);
const MAX_HEIGHT: LogicalPx = LogicalPx(480.0);

/// 창이 응답을 기다리는 승인 요청과 보여 줄 순서. 레코드는 `AppServices.approval_store`의 사본이다.
/// App이 저장소가 바뀔 때마다 모든 창의 목록을 맞추므로 팝업이 닫혀 있어도 갱신을 받는다.
/// 끝난 요청과 저장소에서 사라진 요청은 바로 뺀다.
#[derive(Debug, Default)]
pub(crate) struct ApprovalInbox {
    order: VecDeque<ApprovalId>,
    records: HashMap<ApprovalId, ApprovalRecord>,
}

impl ApprovalInbox {
    /// 기다릴 요청을 넣는다. 이미 있으면 레코드만 바꾸고, 끝난 요청이면 뺀다. 새로 넣었으면 참이다.
    pub(crate) fn push(&mut self, record: ApprovalRecord) -> bool {
        let id = record.request.id.clone();
        if record.state.is_terminal() {
            self.remove(&id);
            return false;
        }
        let new = self.records.insert(id.clone(), record).is_none();
        if new {
            self.order.push_back(id);
        }
        new
    }

    /// 기다리는 요청의 바뀐 레코드를 반영한다. 기다리지 않던 요청은 넣지 않는다. 상태가 바뀌었으면 참이다.
    pub(crate) fn update(&mut self, record: ApprovalRecord) -> bool {
        let id = record.request.id.clone();
        let Some(old) = self.records.get(&id) else {
            return false;
        };
        let changed = old.state != record.state;
        if record.state.is_terminal() {
            self.remove(&id);
        } else {
            self.records.insert(id, record);
        }
        changed
    }

    /// 기다리는 요청마다 저장소의 레코드를 읽어 맞춘다. 무엇이 바뀌었으면 참이다.
    pub(crate) fn sync(
        &mut self,
        mut lookup: impl FnMut(&ApprovalId) -> Option<ApprovalRecord>,
    ) -> bool {
        let ids: Vec<ApprovalId> = self.order.iter().cloned().collect();
        let mut changed = false;
        for id in ids {
            match lookup(&id) {
                Some(record) => changed |= self.update(record),
                None => {
                    self.remove(&id);
                    changed = true;
                }
            }
        }
        changed
    }

    fn remove(&mut self, id: &ApprovalId) {
        self.order.retain(|x| x != id);
        self.records.remove(id);
    }

    /// 맨 앞 요청.
    pub(crate) fn front(&self) -> Option<&ApprovalRecord> {
        self.order.front().and_then(|id| self.records.get(id))
    }

    pub(crate) fn get(&self, id: &ApprovalId) -> Option<&ApprovalRecord> {
        self.records.get(id)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// 인스턴스가 보이는 요청과 그 요청에 쓰는 코멘트. 닫으면 함께 사라지므로 다음 요청은 빈 코멘트로 시작한다.
#[derive(Debug, Clone)]
pub(crate) struct ShownApproval {
    id: ApprovalId,
    comment: String,
    /// 응답을 보냈고 App이 아직 처리하지 않았다. 같은 요청에 두 번 보내지 않는다.
    submitting: bool,
}

impl ShownApproval {
    #[cfg(test)]
    pub(crate) fn drafted(id: &str, comment: &str) -> PopupPayload {
        PopupPayload::Approval(Self {
            id: ApprovalId(id.to_string()),
            comment: comment.to_string(),
            submitting: false,
        })
    }

    fn of(payload: &PopupPayload) -> Option<&Self> {
        match payload {
            PopupPayload::Approval(shown) => Some(shown),
            _ => None,
        }
    }
}

/// 인스턴스가 보이는 요청. 아직 그리지 않은 인스턴스는 목록의 맨 앞 요청을 보인다.
fn shown_record<'a>(
    state: &'a MainViewState,
    payload: &PopupPayload,
) -> Option<&'a ApprovalRecord> {
    match ShownApproval::of(payload) {
        Some(shown) => state.approval_inbox.get(&shown.id),
        None => state.approval_inbox.front(),
    }
}

/// PopupDef.title_fn — 보이는 요청의 title 을 popup 타이틀로 사용.
pub fn approval_popup_title(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &PopupPayload,
) -> String {
    shown_record(state, payload)
        .map(|r| r.request.title.clone())
        .unwrap_or_else(|| t("approval.popup.title").to_string())
}

/// PopupDef.sizer — body 길이 + 선택지 수에 따라 height 추정.
pub fn approval_popup_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &PopupPayload,
) -> egui::Vec2 {
    let width = popup_width(&theme::theme());
    let Some(record) = shown_record(state, payload) else {
        return egui::vec2(width.value(), MIN_HEIGHT.value());
    };
    let record = Some(record);
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
    egui::vec2(width.value(), total_h.value())
}

/// 승인 팝업 폭은 approval-width 토큰이다. sizer가 배율을 직접 적용하므로 Theme 접근자로 읽는다.
fn popup_width(th: &Theme) -> LogicalPx {
    th.approval_width()
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

/// 보기 한 번의 결과. 숫자 키는 보기가 읽지 않는다. 호출자가 `digits_choose` 일 때 팝업 키 통로로 숫자를 읽어
/// [`with_digit_choice`]로 버튼 결과를 덮는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalView {
    /// 클릭으로 고른 결과.
    pub action: ApprovalViewAction,
    /// 이번 패스 숫자 키를 선택으로 읽는지. 팝업이 키보드 포커스를 가졌고 코멘트 입력칸에는 포커스가 없을 때다.
    /// 코멘트를 그린 뒤에 정하므로 이번 패스 클릭으로 코멘트가 포커스를 얻으면 거짓이다.
    pub digits_choose: bool,
}

/// 클릭을 선택 결과로 반환하고 숫자 키를 읽을지 알린다. Escape는 응답으로 처리하지 않는다.
/// 코멘트 입력 중의 숫자는 글자다. egui의 `TextEdit`은 Key 이벤트를 소비하지 않으므로
/// 입력칸 포커스를 직접 확인한다.
pub fn draw_approval_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    props: &mut ApprovalProps<'_>,
) -> ApprovalView {
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

    ApprovalView {
        action,
        digits_choose,
    }
}

/// 선택지 번호 키. 앞에서부터 1번이다.
const DIGIT_KEYS: [egui::Key; 9] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

/// 보기 결과에 숫자 키 선택을 덮는다. `digits_choose` 면 1번부터 `min(9, 항목 수)` 번까지 수식키 없는 누름과 Shift
/// 누름을 `keys` 에서 차례로 소비해 보고 첫 적중 번호를 고른다(작은 번호 우선, 숫자가 클릭을 덮는다). 항목 수보다
/// 큰 번호와 Ctrl·Alt·Cmd가 붙은 숫자는 소비하지 않는다.
pub(crate) fn with_digit_choice(
    view: ApprovalView,
    keys: &mut popup::keys::PopupKeys,
    choices: &[ApprovalChoiceView],
) -> ApprovalViewAction {
    if !view.digits_choose {
        return view.action;
    }
    let hit = DIGIT_KEYS.iter().take(choices.len()).position(|&key| {
        keys.consume_key(egui::Modifiers::NONE, key)
            || keys.consume_key(egui::Modifiers::SHIFT, key)
    });
    match hit.and_then(|idx| choices.get(idx)) {
        Some(choice) => ApprovalViewAction::Chosen {
            key: choice.key.clone(),
        },
        None => view.action,
    }
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

/// 팝업이 닫히면 보이던 요청과 코멘트는 인스턴스와 함께 사라진다. 기다리는 요청이 남아 있으면
/// OpenPopup을 보낸다. 실제로 다시 여는 시점은 다음 intent 처리 때다. 에이전트 출처로
/// 다시 열므로 키보드 포커스를 가져가지 않는다.
pub fn on_close_approval_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    _closed: &crate::adapters::ui::popup::ClosedPopup,
) {
    if !state.approval_inbox.is_empty() {
        dispatch_open(state);
    }
}

/// 에이전트·플러그인 요청이므로 사용자 조작으로 처리하지 않는다. 팝업은 포커스 없이 열린다.
/// 보일 요청은 열 때 정한다. 열기 전에 그 요청이 끝나면 처음 그릴 때 닫고 다음 요청으로 다시 연다.
fn dispatch_open(state: &mut MainViewState) {
    let Some(id) = state.approval_inbox.front().map(|r| r.request.id.clone()) else {
        return;
    };
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup(
            crate::intent::OpenRequest::new(
                APPROVAL_POPUP_ID,
                crate::adapters::ui::popup::PopupScope::Window,
            )
            .centered()
            .focused()
            .with_payload(PopupPayload::Approval(ShownApproval {
                id,
                comment: String::new(),
                submitting: false,
            })),
        )
        .from_agent_ipc(),
    );
}

/// App이 응답 의도를 처리했다. 응답이 실패해 요청이 남았으면 다시 고를 수 있다.
pub(crate) fn submission_handled(state: &mut MainViewState) {
    let iids: Vec<_> = state
        .popups
        .instances_of(APPROVAL_POPUP_ID)
        .map(|p| p.instance)
        .collect();
    for iid in iids {
        if let Some(p) = state.popups.instance_mut(iid)
            && let PopupPayload::Approval(shown) = &mut p.payload
        {
            shown.submitting = false;
        }
    }
}

/// 목록의 맨 앞 요청을 그리고 사용자 응답을 보낸다.
pub fn draw_approval_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
    cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let Some(record) = state.approval_inbox.front().cloned() else {
        return PopupAction::Close;
    };
    if ShownApproval::of(cx.payload).is_none() {
        *cx.payload = PopupPayload::Approval(ShownApproval {
            id: record.request.id.clone(),
            comment: String::new(),
            submitting: false,
        });
    }
    let PopupPayload::Approval(shown) = &mut *cx.payload else {
        return PopupAction::Close;
    };

    // 보이던 요청이 끝났으면 닫고 다음 요청은 다시 연다. 앞 요청에 답하느라 준 포커스와 코멘트를
    // 에이전트의 다음 요청이 이어받지 않는다. 다시 여는 일은 on_close 훅이 맡는다.
    if shown.id != record.request.id {
        return PopupAction::Close;
    }

    let th = theme::theme();
    let available = ui.available_rect_before_wrap();
    let inner_rect = available.shrink2(egui::vec2(th.spacing_sm.value(), th.spacing_xs.value()));
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect));
    let ui = &mut child_ui;

    let mut props = props_from_record(&record, &mut shown.comment, cx.keys.receives());
    let view = draw_approval_view(ui, &theme::theme(), &mut props);
    let view_action = with_digit_choice(view, &mut cx.keys, &props.choices);

    if let ApprovalViewAction::Chosen { key: choice_key } = view_action
        && !shown.submitting
    {
        let comment = Some(shown.comment.trim())
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        shown.submitting = true;
        state.dispatch_intent(
            crate::intent::Intent::RespondApproval {
                request_id: record.request.id.clone(),
                choice: choice_key,
                comment,
            }
            .from_user_menu("approval.respond"),
        );
    }

    PopupAction::None
}

/// 중복되지 않은 승인 요청을 큐에 넣고 팝업 열기와 알림을 요청한다.
pub fn enqueue_approval(
    state: &mut MainViewState,
    engine: &crate::core::CoreState,
    record: &ApprovalRecord,
) {
    if !state.approval_inbox.push(record.clone()) {
        return;
    }
    dispatch_open(state);

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
            terminal_event: false,
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

    /// 승인 팝업의 폭은 approval-width 토큰을 읽고 배율 1에서 값은 480 그대로다.
    #[test]
    fn default_width_is_the_token_value() {
        let th = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        assert_eq!(popup_width(&th), LogicalPx(480.0));
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

    /// 보기를 그리고 팝업 호출자처럼 키 통로로 숫자 선택을 덮는다. 통로는 `keyboard_focused` 면 키를 받는 `Text`
    /// 통로다(승인 팝업 선언 `keyboard`).
    fn view_and_digits(
        ui: &mut egui::Ui,
        theme: &Theme,
        props: &mut ApprovalProps<'_>,
    ) -> ApprovalViewAction {
        let view = draw_approval_view(ui, theme, props);
        let mut keys = popup::keys::PopupKeys::new(
            ui.ctx(),
            props.keyboard_focused,
            popup::PopupKeyboard::Text,
            &crate::adapters::ui::input::edit_key::EditBindings::default(),
        );
        with_digit_choice(view, &mut keys, &props.choices)
    }

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
                action = view_and_digits(ui, &theme, &mut props);
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
                    *action = view_and_digits(ui, &theme, &mut props);
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

    fn choices(n: usize) -> Vec<ApprovalChoiceView> {
        (1..=n)
            .map(|i| ApprovalChoiceView {
                key: format!("c{i}"),
                label: format!("Choice {i}"),
                destructive: false,
            })
            .collect()
    }

    fn digit(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn chosen(n: usize) -> ApprovalViewAction {
        ApprovalViewAction::Chosen {
            key: format!("c{n}"),
        }
    }

    /// 키보드 포커스를 가진 승인 보기를 같은 Context로 여러 프레임 그린다.
    struct View {
        ctx: egui::Context,
        choices: Vec<ApprovalChoiceView>,
        buf: String,
    }

    /// 한 프레임 결과: 고른 결과, 숫자 선택 뒤 입력에 남은 눌린 키, 칠한 글자와 영역.
    struct Frame {
        action: ApprovalViewAction,
        left: Vec<egui::Key>,
        texts: Vec<(String, egui::Rect)>,
    }

    impl View {
        fn new(n: usize) -> Self {
            let mut view = Self {
                ctx: egui::Context::default(),
                choices: choices(n),
                buf: String::new(),
            };
            view.frame(Vec::new());
            view
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> Frame {
            fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::Rect)>) {
                match shape {
                    egui::epaint::Shape::Text(t) => out.push((
                        t.galley.text().to_owned(),
                        egui::Rect::from_min_size(t.pos, t.galley.size()),
                    )),
                    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                    _ => {}
                }
            }
            let theme = mocha_fallback();
            let mut action = ApprovalViewAction::None;
            let mut left = Vec::new();
            let raw = egui::RawInput {
                events,
                ..Default::default()
            };
            let out = self.ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut props = ApprovalProps {
                        id: "abc".to_string(),
                        severity: Severity::Info,
                        severity_label: "INFO".to_string(),
                        comment_label: "Comment".to_string(),
                        comment_hint: "Hint".to_string(),
                        body: None,
                        choices: self.choices.clone(),
                        comment_buffer: &mut self.buf,
                        keyboard_focused: true,
                    };
                    action = view_and_digits(ui, &theme, &mut props);
                    left = ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|e| match e {
                                egui::Event::Key {
                                    key, pressed: true, ..
                                } => Some(*key),
                                _ => None,
                            })
                            .collect()
                    });
                });
            });
            let mut texts = Vec::new();
            for clipped in &out.shapes {
                walk(&clipped.shape, &mut texts);
            }
            Frame {
                action,
                left,
                texts,
            }
        }

        /// 칠한 글자 `wanted` 의 가운데.
        fn center_of(&mut self, wanted: &str) -> egui::Pos2 {
            let texts = self.frame(Vec::new()).texts;
            texts
                .iter()
                .find(|(s, _)| s == wanted)
                .map(|(_, r)| r.center())
                .unwrap_or_else(|| panic!("{wanted} 를 칠하지 않았다: {texts:?}"))
        }
    }

    fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![button(true), button(false)]
    }

    /// T-8-1: Given 키 주인 승인 보기, When 수식키 없는 `Num1` 을 누르면, Then 1번을 고른다.
    #[test]
    fn a_bare_digit_picks_the_choice() {
        let mut view = View::new(2);
        let f = view.frame(vec![digit(egui::Key::Num1, egui::Modifiers::NONE)]);
        assert_eq!(f.action, chosen(1));
    }

    /// T-8-2: Given 키 주인 승인 보기, When Shift+`Num1` 사건이 오면, Then 1번을 고른다.
    #[test]
    fn a_shifted_digit_event_picks_the_choice() {
        let mut view = View::new(2);
        let f = view.frame(vec![digit(egui::Key::Num1, egui::Modifiers::SHIFT)]);
        assert_eq!(f.action, chosen(1));
    }

    /// T-8-3: Given 키 주인 승인 보기, When Ctrl+`Num1` 을 누르면, Then 고르지 않는다.
    #[test]
    fn a_ctrl_digit_does_not_pick() {
        let mut view = View::new(2);
        let f = view.frame(vec![digit(egui::Key::Num1, egui::Modifiers::CTRL)]);
        assert_eq!(f.action, ApprovalViewAction::None);
    }

    /// T-8-3a: Given 키 주인 승인 보기, When Alt+`Num1` 을 누르면, Then 고르지 않는다.
    #[test]
    fn an_alt_digit_does_not_pick() {
        let mut view = View::new(2);
        let f = view.frame(vec![digit(egui::Key::Num1, egui::Modifiers::ALT)]);
        assert_eq!(f.action, ApprovalViewAction::None);
    }

    /// Cmd+`Num1` 누름.
    #[cfg(target_os = "macos")]
    fn command_digit(_ctx: &egui::Context) -> egui::Event {
        digit(
            egui::Key::Num1,
            egui::Modifiers::MAC_CMD | egui::Modifiers::COMMAND,
        )
    }

    /// Super+`Num1` 누름. 비-macOS 는 Super 를 egui 수식키에 싣지 않으므로 사건 수식키는 없고 `super_held` 로 남긴다.
    #[cfg(not(target_os = "macos"))]
    fn command_digit(ctx: &egui::Context) -> egui::Event {
        tasty_key_match::note_modifiers(ctx, winit::keyboard::ModifiersState::SUPER);
        digit(egui::Key::Num1, egui::Modifiers::NONE)
    }

    /// T-8-3m: Given 키 주인 승인 보기, When Cmd(비-macOS Super)+`Num1` 을 누르면, Then 고르지 않는다.
    #[test]
    fn a_command_digit_does_not_pick() {
        let mut view = View::new(2);
        let press = command_digit(&view.ctx);
        let f = view.frame(vec![press]);
        assert_eq!(f.action, ApprovalViewAction::None);
    }

    /// T-8-4: Given 코멘트 입력칸에 포커스가 있는 승인 보기, When `Num1` 을 누르면, Then 고르지 않는다.
    #[test]
    fn a_digit_with_the_comment_focused_is_ignored() {
        let mut view = View::new(2);
        view.ctx
            .memory_mut(|m| m.request_focus(egui::Id::new(COMMENT_FIELD_ID)));
        view.frame(Vec::new());
        assert!(
            view.ctx
                .memory(|m| m.has_focus(egui::Id::new(COMMENT_FIELD_ID)))
        );
        let f = view.frame(vec![digit(egui::Key::Num1, egui::Modifiers::NONE)]);
        assert_eq!(f.action, ApprovalViewAction::None);
    }

    /// T-8-4c: Given 승인 보기, When 한 패스에 코멘트 입력칸 클릭과 `Num1` 이 오면, Then 그 패스에 코멘트가 포커스를
    /// 얻어 고르지 않는다.
    #[test]
    fn a_click_into_the_comment_this_pass_stops_the_digit() {
        let mut view = View::new(2);
        let at = view.center_of("Hint");
        view.frame(vec![egui::Event::PointerMoved(at)]);
        let mut events = click_at(at);
        events.push(digit(egui::Key::Num1, egui::Modifiers::NONE));
        let f = view.frame(events);
        assert!(
            view.ctx
                .memory(|m| m.has_focus(egui::Id::new(COMMENT_FIELD_ID))),
            "코멘트가 포커스를 얻었다"
        );
        assert_eq!(f.action, ApprovalViewAction::None);
    }

    /// T-8-5: Given 승인 보기, When 한 패스에 2번 버튼 클릭과 `Num1` 이 오면, Then 숫자가 덮어 1번을 고른다.
    #[test]
    fn a_digit_overrides_a_button_click_in_the_same_pass() {
        let mut view = View::new(2);
        let at = view.center_of("Choice 2 (2)");
        view.frame(vec![egui::Event::PointerMoved(at)]);
        let mut clicked = view.frame(click_at(at));
        assert_eq!(clicked.action, chosen(2), "클릭만으로는 2번이다");
        let mut events = click_at(at);
        events.push(digit(egui::Key::Num1, egui::Modifiers::NONE));
        clicked = view.frame(events);
        assert_eq!(clicked.action, chosen(1));
    }

    /// T-8-6: Given 항목 셋, When 한 패스에 [`Num3`, `Num1`] 이 오면, Then 작은 번호 1번을 고른다.
    #[test]
    fn the_smaller_of_two_digits_wins() {
        let mut view = View::new(3);
        let f = view.frame(vec![
            digit(egui::Key::Num3, egui::Modifiers::NONE),
            digit(egui::Key::Num1, egui::Modifiers::NONE),
        ]);
        assert_eq!(f.action, chosen(1));
    }

    /// T-8-6r: Given 항목 셋, When 한 패스에 [`Num1`, `Num3`] 이 오면, Then 1번을 고른다.
    #[test]
    fn the_smaller_of_two_digits_wins_in_event_order_too() {
        let mut view = View::new(3);
        let f = view.frame(vec![
            digit(egui::Key::Num1, egui::Modifiers::NONE),
            digit(egui::Key::Num3, egui::Modifiers::NONE),
        ]);
        assert_eq!(f.action, chosen(1));
    }

    /// T-8-7: Given 항목 둘, When `Num5` 를 누르면, Then 고르지 않고 그 누름이 입력에 남는다.
    #[test]
    fn a_digit_past_the_choices_is_left_unused() {
        let mut view = View::new(2);
        let f = view.frame(vec![digit(egui::Key::Num5, egui::Modifiers::NONE)]);
        assert_eq!(f.action, ApprovalViewAction::None);
        assert!(f.left.contains(&egui::Key::Num5), "{:?}", f.left);
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
