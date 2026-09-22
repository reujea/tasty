//! 정보 알림용 modal-like popup. 부팅 시점 fallback/에러 알림에 사용한다.
//!
//! 구조: 큐(`DialogState.info_modal_queue`)에 `InfoModal`을 push하고 popup을
//! 띄우면, 큐 head를 표시 + [확인]/Enter/Escape로 pop. 큐가 비면 popup 닫힘.
//!
//! 호출 패턴:
//! ```ignore
//! show_info_modal(state, InfoModal {
//!     title: "...".into(),
//!     body: "...".into(),
//!     on_close: InfoModalAction::Continue,
//! });
//! ```

use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::t;
use crate::state::AppState;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::vspace;

/// 모달 [확인] 시 동작.
#[derive(Debug, Clone)]
pub enum InfoModalAction {
    /// 큐 처리 후 부팅/동작을 계속한다.
    Continue,
    /// 큐 처리 후 정상 종료. exit code는 best-effort (winit shutdown 후 process::exit).
    Exit(i32),
}

/// [확인] 옆에 추가로 붙는 버튼의 동작. 안내 자체로 끝나지 않고 사용자를 어딘가로
/// 보내야 하는 모달(예: OS 설정 패널로 유도)을 위한 것이다.
#[derive(Debug, Clone)]
pub enum InfoModalButtonAction {
    /// URL/스킴을 OS 기본 핸들러로 연다. 모달은 **열린 채 유지**된다 — 설정 패널을
    /// 열어둔 채 안내 문구를 다시 읽을 수 있어야 하기 때문.
    ///
    /// 현재 유일한 생산자가 macOS 전용 안내(Full Disk Access)라 다른 OS 에서는
    /// 아무도 만들지 않는다. `GeneralSubTab::Display` 와 동일하게 variant 자체는
    /// 플랫폼 공통으로 두고 경고만 억제한다 — 그리는 쪽은 어느 OS 에서든 컴파일된다.
    // 이유: 유일한 생산자가 macOS 전용 안내라 다른 OS 빌드엔 생성처가 없다(위 문단).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OpenExternal(String),
}

/// [확인] 외에 모달 하단에 함께 그리는 버튼.
#[derive(Debug, Clone)]
pub struct InfoModalButton {
    pub label: String,
    pub action: InfoModalButtonAction,
}

/// 큐에 들어가는 단일 메시지.
#[derive(Debug, Clone)]
pub struct InfoModal {
    pub title: String,
    pub body: String,
    pub on_close: InfoModalAction,
    /// [확인] 왼쪽에 그려지는 추가 버튼들. 비어 있으면 [확인] 하나만 나온다.
    pub extra_buttons: Vec<InfoModalButton>,
}

pub const INFO_MODAL_ID: &str = "info_modal";
const DEFAULT_WIDTH: LogicalPx = LogicalPx(440.0);
const MIN_HEIGHT: LogicalPx = LogicalPx(140.0);
const MAX_HEIGHT: LogicalPx = LogicalPx(360.0);
/// 본문 아래 버튼행이 차지하는 높이. **고정값**이다 — 갤러리 specimen 은 같은 자리를
/// 테마 파생(`item_height_interactive + spacing_lg + spacing_xs`)으로 낸다. 오늘 두 값이
/// 같을 뿐이고, 갈라지면 `gallery_copied_dimensions` 가 그 자리에서 결정을 요구한다.
const FOOTER_ROOM: LogicalPx = LogicalPx(48.0);

/// 큐에 modal 한 건을 추가하고 popup을 연다. 이미 열려 있으면 큐만 추가.
///
/// 호출처는 시스템 부트스트랩 (DB 초기화 실패, theme fallback 등) — 사용자 입력에
/// 의해 발화되지는 않지만, modal 인 만큼 focus 가 필요하다. agent IPC 가 아니므로
/// `from_user_menu` 와 동일한 user-ish origin 으로 발화 (PR 리뷰에서 정책 분기 결정).
pub fn show_info_modal(state: &mut AppState, modal: InfoModal) {
    state.dialogs.info_modal_queue.push_back(modal);
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup {
            id: INFO_MODAL_ID,
            mode: crate::intent::OpenPopupMode::CenteredFocused,
        }
        .from_user_menu("info_modal"),
    );
}

/// PopupDef.title_fn — 큐 head의 title을 popup 타이틀로 사용.
pub fn info_modal_title(state: &AppState, _engine: &crate::core::CoreState) -> String {
    state
        .dialogs
        .info_modal_queue
        .front()
        .map(|m| m.title.clone())
        .unwrap_or_default()
}

/// PopupDef.sizer — body 길이에 따라 height를 조정.
pub fn info_modal_sizer(state: &AppState, _engine: &crate::core::CoreState) -> egui::Vec2 {
    let body_len = state
        .dialogs
        .info_modal_queue
        .front()
        .map(|m| m.body.chars().count())
        .unwrap_or(0);
    // 대략 char당 1.6 line으로 가정 (한글/영문 혼합). 70 chars per line 기준.
    let approx_lines = (body_len as f32 / 60.0).ceil().max(2.0);
    let line_h = theme::theme().font_size_body.value() * 1.5;
    let body_h = approx_lines * line_h;
    let total_h = (popup::title_bar_height()
        + popup::content_margin().scaled(2.0)
        + LogicalPx(body_h)
        + FOOTER_ROOM)
        .min(MAX_HEIGHT)
        .max(MIN_HEIGHT);
    egui::vec2(DEFAULT_WIDTH.value(), total_h.value())
}

/// PopupDef::on_close 진입점 — X 버튼(또는 그 외 draw_fn 을 우회하는 닫힘 경로)로
/// 닫혔을 때 draw_fn 의 확인 로직을 그대로 미러한다. draw_fn 자신의 pop 경로로
/// 닫힌 경우엔 이 시점에 큐가 이미 비어 있어(그 경로만 `PopupAction::Close`를
/// 반환) `pop_front`가 `None`을 돌려주고 즉시 반환 — 이중 pop 은 없다.
///
/// X 로 닫으면 head 가 pop 되지 않던 시절엔 남은 큐가 다시 뜨지 않아 부팅 에러
/// 안내가 조용히 유실됐다 — 그 버그의 수정.
pub fn on_close_info_modal(
    _ctx: &egui::Context,
    state: &mut AppState,
    _engine: &mut crate::core::CoreState,
) {
    let Some(modal) = state.dialogs.info_modal_queue.pop_front() else {
        return;
    };
    if let InfoModalAction::Exit(code) = modal.on_close {
        tracing::info!("info modal exit requested (code={code})");
        std::process::exit(code);
    }
    if !state.dialogs.info_modal_queue.is_empty() {
        // intent-exempt: popup 자기-close cleanup — 이 함수가 on_close 훅이라 여기서 큐의 다음 항목을 잇는다
        state.popups.open_centered_focused(INFO_MODAL_ID);
    }
}

/// PopupDef.draw_fn — 큐 head를 보여주고 [확인]/Enter/Escape로 pop.
pub fn draw_info_modal(
    ui: &mut egui::Ui,
    state: &mut AppState,
    _engine: &mut crate::core::CoreState,
) -> PopupAction {
    let th = theme::theme();
    let ctx = ui.ctx().clone();

    let Some(current) = state.dialogs.info_modal_queue.front().cloned() else {
        return PopupAction::Close;
    };

    let margin = th.spacing_sm.value();
    let available = ui.available_rect_before_wrap();
    let inner_rect = available.shrink2(egui::vec2(margin, th.spacing_xs.value()));
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner_rect));
    let ui = &mut child_ui;

    ui.label(
        egui::RichText::new(&current.body)
            .color(th.text_primary())
            .size(th.font_size_body.value()),
    );

    let mut confirm =
        ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));

    ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
        vspace(ui, th.spacing_xs);
        // 오른쪽부터 쌓는다 — 먼저 그린 [확인] 이 가장 오른쪽에 오고 추가 버튼이
        // 그 왼쪽에 붙는다(dialog/preset 의 버튼 행과 같은 배치).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t("button.ok")).clicked() {
                confirm = true;
            }
            for button in &current.extra_buttons {
                if ui.button(&button.label).clicked() {
                    match &button.action {
                        InfoModalButtonAction::OpenExternal(url) => open_external(url),
                    }
                }
            }
        });
    });

    if !confirm {
        return PopupAction::None;
    }

    // Pop the current head and act on its on_close.
    let popped = state.dialogs.info_modal_queue.pop_front();
    if let Some(modal) = popped
        && let InfoModalAction::Exit(code) = modal.on_close
    {
        // 부팅 시점 fatal 알림에만 사용한다. winit destructor는 돌지 못하지만
        // 아직 PTY/plugin 등이 떠 있지 않은 시점이라 손실이 없다.
        tracing::info!("info modal exit requested (code={code})");
        std::process::exit(code);
    }

    if state.dialogs.info_modal_queue.is_empty() {
        PopupAction::Close
    } else {
        // 다음 메시지로 이어진다. popup은 그대로 유지하되 title/size는 다음 프레임에
        // popup::frame::draw_popup_layer의 refresh 루프가 자동 갱신.
        PopupAction::None
    }
}

/// URL/스킴을 OS 기본 핸들러로 넘긴다. `reveal::open_path` 와 같은 방식이되 경로가
/// 아니라 스킴을 넘기는 자리 — `x-apple.systempreferences:` 처럼 브라우저가 아닌
/// 핸들러가 받는 스킴도 그대로 통과해야 하기 때문이다. 프로세스를 기다리지 않는다
/// (렌더 경로에서 호출된다).
fn open_external(url: &str) {
    #[cfg(debug_assertions)]
    if crate::platform::debug_os_open::intercepted("open_external", url) {
        return;
    }
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "start", ""]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");

    if let Err(err) = cmd.arg(url).spawn() {
        tracing::warn!(%err, url, "info modal: 외부 링크 열기 실패");
    }
}
