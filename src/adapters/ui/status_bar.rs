//! 현재 포커스된 surface의 정보를 읽어 공용 상태바에 전달하고 클릭 동작을 처리한다.
//! 상태바 레이어·위치는 본체에서 정하며 갤러리는 같은 공용 화면 함수를 사용한다.
//! 표시 항목: docs/features/workspace-status-bar/index.md.

use crate::runtime::engine_read::EngineRead;
use egui::emath::GuiRounding as _;
use tasty_type_geometry::length::{LogicalPx, PhysicalPx};
use tasty_type_geometry::rect::PhysicalRect;
use tasty_ui_widgets::{StatusBarAction, StatusBarData, StatusBarRemote, draw_status_bar_view};

use crate::state::MainViewState;
use crate::state::branch::HeadState;
use crate::theme;

/// Area 생성과 egui_bridge의 레이어 정렬에서 공유하는 상태바 ID.
pub(crate) const STATUS_BAR_AREA_ID: &str = "workspace_status_bar";

/// 배너 위에 놓을 상태바 Foreground 레이어.
pub(crate) fn status_bar_layer_id() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, egui::Id::new(STATUS_BAR_AREA_ID))
}

/// 터미널 영역에서 제외할 상태바 높이. Theme에서 읽고 물리 픽셀로 반환한다.
pub fn status_bar_bottom_inset(scale_factor: f32) -> PhysicalPx {
    theme::theme().status_bar_height.to_physical(scale_factor)
}

/// 터미널 영역 바로 아래에 상태바를 그리고 반환된 클릭 동작을 처리한다.
pub fn draw_status_bar(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    terminal_rect: PhysicalRect,
    scale_factor: f32,
) {
    let th = theme::theme();
    let bar_h_logical = th.status_bar_height.value();

    let logical = terminal_rect.to_logical(scale_factor);
    let x = logical.x.value().round_ui();
    let y = (logical.y + logical.height).value().round_ui();
    let w = logical.width.value().round_ui();
    let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, bar_h_logical));

    let surface_id = state.focused_surface_id(engine);
    // 그리드·프로세스·Git 정보는 캐시에서 읽어 프레임마다 시스템·파일 조회를 반복하지 않는다.
    let grid = surface_id
        .and_then(|sid| engine.terminals.get(sid))
        .map(|term| (term.cols(), term.rows()));
    let shell = surface_id.and_then(|sid| engine.foreground_name(sid).map(str::to_owned));
    let branch = surface_id
        .and_then(|sid| state.status_bar_branch(sid))
        .map(head_display);
    let pane_id = surface_id.and_then(|sid| engine.find_pane_for_surface(sid));
    let palette_keys = engine
        .settings
        .keybindings
        .toggle_command_palette
        .first()
        .map(|b| tasty_settings::KeybindingSettings::format_display(b, &engine.settings.general))
        .unwrap_or_default();

    let remote = remote_of(engine, surface_id);

    let data = StatusBarData {
        remote,
        branch,
        surface_id,
        pane_id,
        shell,
        grid,
        theme_is_light: th.is_light,
        palette_keys,
        palette_tooltip: crate::i18n::t("status_bar.palette_tooltip").to_owned(),
        theme_tooltip: crate::i18n::t("status_bar.theme_tooltip").to_owned(),
    };

    let result = egui::Area::new(egui::Id::new(STATUS_BAR_AREA_ID))
        .fixed_pos(rect.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            draw_status_bar_view(ui, &th, LogicalPx(rect.width()), &data)
        })
        .inner;
    state.resize_edge_widget_hovered |= result.resize_priority_hovered;

    for action in result.actions {
        match action {
            StatusBarAction::OpenPalette => {
                use crate::intent::{OpenPopupMode, UiIntent};
                state.dispatch_intent(
                    UiIntent::TogglePopup {
                        id: crate::adapters::ui::popup::command_palette::COMMAND_PALETTE_POPUP_ID,
                        mode: OpenPopupMode::CenteredFocused,
                    }
                    .from_user_menu("status_bar.palette"),
                );
            }
            StatusBarAction::ToggleTheme => {
                // 디자인 onTheme: latte ↔ mocha. 그 외 테마에서 누르면 latte 로.
                let target = if engine.settings.appearance.theme == tasty_themes::BUILTIN_LATTE_ID {
                    tasty_themes::BUILTIN_MOCHA_ID
                } else {
                    tasty_themes::BUILTIN_LATTE_ID
                };
                let mut new_settings = engine.settings.clone();
                tasty_themes::apply_theme(&mut new_settings.appearance, target);
                state.dispatch_intent(
                    crate::app::command::DomainIntent::UpdateSettings(new_settings)
                        .from_user_menu("status_bar.theme_toggle"),
                );
            }
        }
    }
}

/// 포커스 surface 가 mirror workspace 에 있으면 원격 항목, 로컬이면 None.
fn remote_of(engine: &EngineRead<'_>, surface_id: Option<u32>) -> Option<StatusBarRemote> {
    let sid = surface_id?;
    let mut item = remote_item(engine.mirror_location_of(sid)?);
    // 재연결을 멈춘 저장 행은 형상을 남긴 채 끊겨 있다. 같은 읽기 전용 모양에 끊김 상태어를 쓰고
    // 툴팁은 연결된 항목과 같다.
    if engine.mirror_workspace_of(sid).is_some_and(|ws| {
        engine.remote_row_state(ws) == tasty_ui_widgets::RemoteRowState::Disconnected
    }) {
        item.reconnecting = true;
        item.reconnecting_label = crate::i18n::t("status_bar.remote_disconnected").to_owned();
        item.tooltip = crate::i18n::t("attach.mirror_workspace").to_owned();
    }
    Some(item)
}

/// mirror 위치 값을 상태바 원격 항목으로 바꾼다. 재연결 중이면 툴팁 첫 줄도 끊김 안내로 바꾼다.
fn remote_item(location: &crate::remote::state::MirrorLocation) -> StatusBarRemote {
    let tooltip = if location.reconnecting {
        "attach.toast.mirror_reconnecting"
    } else {
        "attach.mirror_workspace"
    };
    StatusBarRemote {
        host: location.host.clone(),
        reconnecting: location.reconnecting,
        empty_label: crate::i18n::t("attach.mirror_pill_label").to_owned(),
        reconnecting_label: crate::i18n::t("status_bar.remote_reconnecting").to_owned(),
        tooltip: crate::i18n::t(tooltip).to_owned(),
    }
}

/// detached HEAD는 같은 이름의 브랜치와 구분하도록 @를 붙인다.
/// HeadState는 창 상태 타입이라 공용 위젯에 넘기기 전에 표시 문자열로 바꾼다.
fn head_display(head: &HeadState) -> String {
    match head {
        HeadState::Branch(name) => name.clone(),
        HeadState::Detached(short_sha) => format!("@ {short_sha}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{HeadState, head_display, remote_item, remote_of};
    use crate::remote::state::MirrorLocation;

    /// 같은 surface 라도 mirror workspace 에 있을 때만 원격 항목이 생긴다.
    #[test]
    fn only_a_mirror_surface_gets_the_remote_item() {
        crate::i18n::init("en");
        let location = MirrorLocation {
            host: "mac-company-attach".into(),
            reconnecting: false,
        };
        let (_state, mut local, sid, _root) = crate::state::tests::explorer_fixture(false);
        // 로컬 workspace 는 위치 값이 있어도 표시하지 않는다.
        local.remote.mirror_locations.insert(sid, location.clone());
        assert!(remote_of(&local.read(), Some(sid)).is_none());

        let (_state, mut mirror, sid, _root) = crate::state::tests::explorer_fixture(true);
        // 연결 정보가 아직 없으면 항목도 없다.
        assert!(remote_of(&mirror.read(), Some(sid)).is_none());
        mirror.remote.mirror_locations.insert(sid, location);
        let item = remote_of(&mirror.read(), Some(sid)).expect("mirror surface");
        assert_eq!(item.host, "mac-company-attach");
        assert!(remote_of(&mirror.read(), None).is_none());
    }

    /// mirror 는 호스트 글자와 연결 상태를 그대로 넘기고, 재연결 중이면 상태어와 끊김 툴팁을 쓴다.
    #[test]
    fn a_mirror_location_becomes_the_remote_item() {
        crate::i18n::init("en");
        let live = remote_item(&MirrorLocation {
            host: "gx10".into(),
            reconnecting: false,
        });
        assert_eq!(live.host, "gx10");
        assert!(!live.reconnecting);
        assert_eq!(live.tooltip, crate::i18n::t("attach.mirror_workspace"));
        assert_eq!(live.empty_label, crate::i18n::t("attach.mirror_pill_label"));
        let lost = remote_item(&MirrorLocation {
            host: "gx10".into(),
            reconnecting: true,
        });
        assert!(lost.reconnecting);
        assert_eq!(lost.reconnecting_label, "reconnecting");
        assert_eq!(
            lost.tooltip,
            crate::i18n::t("attach.toast.mirror_reconnecting")
        );
    }

    #[test]
    fn detached_gets_the_marker_and_a_branch_does_not() {
        assert_eq!(head_display(&HeadState::Branch("main".into())), "main");
        assert_eq!(
            head_display(&HeadState::Detached("4af6ac9".into())),
            "@ 4af6ac9"
        );
    }

    /// 표지처럼 생긴 **이름**은 이름 그대로 나간다. 표지가 덧붙지 않는다.
    #[test]
    fn a_branch_that_looks_like_the_marker_is_not_decorated() {
        assert_eq!(
            head_display(&HeadState::Branch("@4af6ac9".into())),
            "@4af6ac9"
        );
    }
}
