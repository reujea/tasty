//! 저장된 원격 행의 연결 팝업 콘텐츠와 그 문구 선택.
//! 본체 popup 과 갤러리 예제가 이 함수를 함께 부른다. 문자열은 호출자가 주입하고 셸(배경·테두리·
//! 그림자·scrim)은 호출자가 맡는다. 콘텐츠 여백(space-md)은 이 함수가 넣는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::icon_button::IconPainter;
use crate::remote_row_menu::{REMOTE_ROW_CONNECT_KEY, REMOTE_ROW_RECONNECT_KEY, RemoteRowState};

/// 팝업 폭. 디자인 `RemoteConnectionPopup`의 `--tasty-size-360`이며 역할 토큰이 없다. 배율 전 값이다.
pub const REMOTE_ROW_POPUP_WIDTH: LogicalPx = crate::tokens::SIZE_360;

/// 팝업이 보이는 상황. 형상이 남은 행(scrim 위)과 빈 workspace 행이 같은 종류를 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRowPopupKind {
    /// 처음 연결하는 중이다. 제목 없이 비활성 버튼과 Spinner 만 있다.
    Connecting,
    /// 이번 실행에서 아직 연결하지 않았고 연결을 시작하지도 않았다.
    Idle,
    /// 연결에 실패했다. 이유를 본문에 둔다.
    Failed,
    /// 끊겨서 다시 연결하는 중이다.
    Reconnecting,
    /// 재연결 시간이 지나 멈췄다.
    GaveUp,
    /// 사용자가 연결을 끊었다.
    User,
    /// 이 행이 연결된 동안 원격 workspace 가 다른 곳에서 닫혔다. 같은 workspace 로 다시 붙을 수 없어
    /// 버튼은 숨기지 않고 끈다.
    Closed,
}

/// 연결하지 못한 이유. 문구 키와 그 인자(프로필 이름 또는 원격 workspace)를 고른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRowReason {
    ProfileMissing,
    WorkspaceMissing,
    Occupied,
    Chain,
    Endpoint,
    SelfInstance,
}

impl RemoteRowReason {
    pub fn key(self) -> &'static str {
        match self {
            Self::ProfileMissing => "remote.row.reason.profile_missing",
            Self::WorkspaceMissing => "remote.row.reason.workspace_missing",
            Self::Occupied => "remote.row.reason.occupied",
            Self::Chain => "remote.row.reason.chain",
            Self::Endpoint => "remote.row.reason.endpoint",
            Self::SelfInstance => "remote.row.reason.self",
        }
    }

    /// 문구의 인자가 프로필 이름인지. 아니면 원격 workspace 다.
    pub fn names_profile(self) -> bool {
        matches!(
            self,
            Self::ProfileMissing | Self::Endpoint | Self::SelfInstance
        )
    }
}

/// 본문 종류. 인자는 호출자가 채운다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteRowPopupBody {
    None,
    /// `{}` 자리에 재연결 시간(초)을 넣는다.
    Seconds(&'static str),
    /// 인자 없는 문장.
    Plain(&'static str),
    /// 이유 문장(`RemoteRowReason`). closed 는 workspace_missing 이다.
    Reason,
}

/// 한 종류의 문구 키와 모양.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteRowPopupCopy {
    pub title: Option<&'static str>,
    pub body: RemoteRowPopupBody,
    pub button: &'static str,
    /// 제목 앞 경고 글리프(failed·gave_up·closed).
    pub warn: bool,
    /// 시도 중이라 버튼 자리에 Spinner 를 둔다.
    pub trying: bool,
    pub enabled: bool,
}

/// 사용자가 끊은 행의 본문. 결정 L1 a(다시 열어도 끊긴 채로 둔다)는 `user_body_stay` 다.
/// L1 b 로 정해지면 화면에 남아 있는 동안 `user_body_next` 를 쓴다.
pub const REMOTE_ROW_USER_BODY_STAY: &str = "remote.row.popup.user_body_stay";
pub const REMOTE_ROW_USER_BODY_NEXT: &str = "remote.row.popup.user_body_next";

/// 종류와 형상 유무로 문구를 고른다. 형상이 있었던 행은 Reconnect·Reconnecting… 을 쓴다.
pub fn remote_row_popup_copy(
    kind: RemoteRowPopupKind,
    shape: bool,
    user_body: &'static str,
) -> RemoteRowPopupCopy {
    use RemoteRowPopupBody as B;
    use RemoteRowPopupKind as K;
    let (title, body) = match kind {
        K::Connecting => (None, B::None),
        K::Idle => (Some("remote.row.popup.idle_title"), B::None),
        K::Failed => (Some("remote.row.popup.failed_title"), B::Reason),
        K::Reconnecting => (
            Some("remote.row.popup.lost_title"),
            B::Seconds("remote.row.popup.lost_body"),
        ),
        K::GaveUp => (
            Some("remote.row.popup.gave_up_title"),
            B::Seconds("remote.row.popup.gave_up_body"),
        ),
        K::User => (Some("remote.row.popup.user_title"), B::Plain(user_body)),
        K::Closed => (Some("remote.row.popup.closed_title"), B::Reason),
    };
    let trying = matches!(kind, K::Connecting | K::Reconnecting);
    let button = match (trying, shape) {
        (true, true) => "child_graph.reconnecting",
        (true, false) => "remote.row.popup.connecting",
        (false, true) => REMOTE_ROW_RECONNECT_KEY,
        (false, false) => REMOTE_ROW_CONNECT_KEY,
    };
    RemoteRowPopupCopy {
        title,
        body,
        button,
        warn: matches!(kind, K::Failed | K::GaveUp | K::Closed),
        trying,
        enabled: !trying && kind != K::Closed,
    }
}

/// 행 이름 줄 아래 REMOTE Tag 뒤에 붙는 상태어. 연결된 행은 없다.
pub fn remote_row_state_word_key(state: RemoteRowState) -> Option<&'static str> {
    match state {
        RemoteRowState::Connected => None,
        RemoteRowState::Connecting => Some("remote.row.state.connecting"),
        RemoteRowState::Reconnecting => Some("status_bar.remote_reconnecting"),
        RemoteRowState::Disconnected => Some("status_bar.remote_disconnected"),
        RemoteRowState::NotConnected => Some("remote.row.state.not_connected"),
    }
}

/// 사이드바 행의 REMOTE 줄: 글리프 → REMOTE Tag → 상태어. 연결된 행은 지금처럼 info Tag 이고,
/// 그 밖의 상태는 중립 Tag 뒤에 상태어(font-label caption)를 붙인다. 줄 간격은 호출자가 정한다.
pub fn remote_row_tag_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: IconPainter<'_>,
    glyph_size: LogicalPx,
    glyph_color: egui::Color32,
    label: &str,
    word: Option<(&str, egui::Color32)>,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(glyph_size.value(), glyph_size.value()),
        egui::Sense::hover(),
    );
    glyph(ui, rect, glyph_color);
    let variant = if word.is_none() {
        crate::TagVariant::Info
    } else {
        crate::TagVariant::Default
    };
    let tag = crate::tag_caps(ui, theme, label, variant, false);
    if let Some((word, color)) = word {
        ui.add(
            egui::Label::new(
                egui::RichText::new(word)
                    .size(theme.font_size_caption.value())
                    .color(color),
            )
            .truncate(),
        );
    }
    tag
}

/// 팝업 콘텐츠의 입력값.
pub struct RemoteRowPopupView<'a> {
    pub name: &'a str,
    pub profile_label: &'a str,
    pub profile: &'a str,
    pub remote_label: &'a str,
    pub remote: &'a str,
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub button: &'a str,
    pub copy: RemoteRowPopupCopy,
    /// 머리의 remote 글리프. 색은 이 함수가 accent-remote-raised 로 준다.
    pub remote_glyph: IconPainter<'a>,
    /// 제목 앞 경고 글리프. 색은 이 함수가 accent-warning-glyph 로 준다.
    pub warn_glyph: IconPainter<'a>,
}

/// 머리(remote 글리프 + 행 이름) → 필드 두 줄 → 상태 블록 → 오른쪽 primary sm 버튼.
/// 폭은 호출자가 준 영역을 채운다. 버튼을 눌렀으면 true 다.
pub fn remote_row_popup(ui: &mut egui::Ui, theme: &Theme, view: &RemoteRowPopupView<'_>) -> bool {
    let md = theme.spacing_md.value();
    let sm = theme.spacing_sm.value();
    let xs = theme.spacing_xs.value();
    let body_size = theme.font_size_body.value();
    let caption = theme.font_size_caption.value();
    let line = |size: f32| Some(size * theme.line_height_ui);
    let mut clicked = false;
    egui::Frame::NONE
        .inner_margin(egui::Margin::same(md as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(sm, md);
            ui.horizontal(|ui| {
                let glyph = theme.icon_glyph_size_md.value();
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(glyph, glyph), egui::Sense::hover());
                (view.remote_glyph)(ui, rect, theme.accent_remote_raised().to_egui());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(view.name)
                            .size(body_size)
                            .strong()
                            .color(theme.text_primary().to_egui()),
                    )
                    .truncate(),
                );
            });
            // 한 화면에 팝업이 여럿이어도(갤러리) Grid id 가 겹치지 않게 호출 자리마다 다른 id 를 쓴다.
            egui::Grid::new(ui.next_auto_id().with("remote_row_popup_fields"))
                .num_columns(2)
                .spacing(egui::vec2(md, xs))
                .show(ui, |ui| {
                    for (label, value) in [
                        (view.profile_label, view.profile),
                        (view.remote_label, view.remote),
                    ] {
                        ui.label(
                            egui::RichText::new(label)
                                .size(caption)
                                .line_height(line(caption))
                                .color(theme.text_muted().to_egui()),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(value)
                                    .monospace()
                                    .size(caption)
                                    .line_height(line(caption))
                                    .color(theme.text_secondary().to_egui()),
                            )
                            .truncate(),
                        );
                        ui.end_row();
                    }
                });
            if let Some(title) = view.title {
                ui.horizontal_top(|ui| {
                    if view.copy.warn {
                        let glyph = theme.icon_glyph_size_sm.value();
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(glyph, body_size * theme.line_height_ui),
                            egui::Sense::hover(),
                        );
                        let icon =
                            egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
                        (view.warn_glyph)(ui, icon, theme.accent_warning_glyph().to_egui());
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = xs;
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(title)
                                    .size(body_size)
                                    .line_height(line(body_size))
                                    .strong()
                                    .color(theme.text_primary().to_egui()),
                            )
                            .wrap(),
                        );
                        if let Some(body) = view.body {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(body)
                                        .size(body_size)
                                        .line_height(line(body_size))
                                        .color(theme.text_secondary().to_egui()),
                                )
                                .wrap(),
                            );
                        }
                    });
                });
            }
            // with_layout 은 남은 높이를 모두 차지한다. 버튼 높이만 쓰도록 높이 0 으로 잡는다.
            let row = egui::vec2(ui.available_width(), 0.0);
            let layout = egui::Layout::right_to_left(egui::Align::Center);
            ui.allocate_ui_with_layout(row, layout, |ui| {
                let spinner_size = theme.icon_glyph_size_xs;
                let spinner = |ui: &mut egui::Ui, rect: egui::Rect, _: egui::Color32| {
                    crate::Spinner::new()
                        .size(spinner_size.value())
                        .paint_in(ui, theme, rect);
                };
                let mut button = Button::new(view.button)
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Sm)
                    .enabled(view.copy.enabled);
                if view.copy.trying {
                    button = button
                        .leading_icon(&spinner)
                        .leading_icon_size(spinner_size);
                }
                clicked = button.show(ui, theme).clicked() && view.copy.enabled;
            });
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use RemoteRowPopupKind as K;

    /// 시도 중에는 버튼을 끄고 Spinner 를 둔다. 다른 곳에서 닫힌 행도 버튼을 끈다.
    #[test]
    fn the_button_is_off_while_trying_and_for_a_row_closed_elsewhere() {
        for (kind, enabled) in [
            (K::Connecting, false),
            (K::Reconnecting, false),
            (K::Closed, false),
            (K::Idle, true),
            (K::Failed, true),
            (K::GaveUp, true),
            (K::User, true),
        ] {
            let copy = remote_row_popup_copy(kind, true, REMOTE_ROW_USER_BODY_STAY);
            assert_eq!(copy.enabled, enabled, "{kind:?}");
            assert_eq!(copy.trying, matches!(kind, K::Connecting | K::Reconnecting));
        }
    }

    /// 형상이 있었던 행은 Reconnect 계열, 처음 붙는 행은 Connect 계열 문구다.
    #[test]
    fn the_button_word_follows_whether_the_row_had_a_screen() {
        let button =
            |kind, shape| remote_row_popup_copy(kind, shape, REMOTE_ROW_USER_BODY_STAY).button;
        assert_eq!(button(K::Connecting, false), "remote.row.popup.connecting");
        assert_eq!(button(K::Reconnecting, true), "child_graph.reconnecting");
        assert_eq!(button(K::Failed, false), REMOTE_ROW_CONNECT_KEY);
        assert_eq!(button(K::GaveUp, true), REMOTE_ROW_RECONNECT_KEY);
    }

    #[test]
    fn warning_glyph_marks_failed_timed_out_and_closed() {
        let warn = |kind| remote_row_popup_copy(kind, true, REMOTE_ROW_USER_BODY_STAY).warn;
        assert!(warn(K::Failed) && warn(K::GaveUp) && warn(K::Closed));
        assert!(!warn(K::User) && !warn(K::Reconnecting) && !warn(K::Idle));
    }
}
