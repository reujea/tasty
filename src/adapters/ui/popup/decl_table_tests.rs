//! 정의마다 선언 값이 지금 그리기 결과와 같은지 표로 고정한다.
//!
//! `EXPECTED`는 선언 값을 도입하기 전 id 명부(scrim·scrim 포인터·탐색기 끌어 놓기 차단·그림자·
//! 콘텐츠 여백·배경·테두리·제목줄 아래 선)가 정의마다 낸 결과를 옮겨 적은 것이다. 명부를 지우기 전
//! 커밋에서 명부 함수의 결과와 이 표가 같은 것을 확인했다.
//! 정의를 더하면 이 표에도 행을 더한다. 표에 없는 정의가 있으면 실패한다.

use super::super::{
    PopupContentMargin, PopupLook, PopupManager, PopupShadow, PopupShell, PopupState, ScrimInput,
};
use egui::Color32;
use tasty_type_appearance::theme::Theme;

/// 셸 색 토큰.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    SurfaceRaised,
    BgPanel,
    DialogBg,
    BorderFrame,
    MenuBorder,
    BannerMoreMenuBorder,
    SearchBarBorder,
    InfoModalTitleEdge,
}

fn color(tok: Tok, th: &Theme) -> Color32 {
    match tok {
        Tok::SurfaceRaised => th.surface_raised().into(),
        Tok::BgPanel => th.bg_panel().into(),
        Tok::DialogBg => th.dialog_bg().into(),
        Tok::BorderFrame => th.border_frame().into(),
        Tok::MenuBorder => th.menu_border().into(),
        Tok::BannerMoreMenuBorder => th.banner_more_menu_border().into(),
        Tok::SearchBarBorder => th.search_bar_border().into(),
        Tok::InfoModalTitleEdge => th.info_modal_title_edge().into(),
    }
}

/// 옛 명부가 정의 하나에 낸 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    id: &'static str,
    scrim: bool,
    scrim_pointer: bool,
    blocks_drop: bool,
    popover_shadow: bool,
    margin_zero: bool,
    bg: Tok,
    border: Tok,
    title_edge: Tok,
}

const fn row(
    id: &'static str,
    scrim: bool,
    popover_shadow: bool,
    margin_zero: bool,
    shell: (Tok, Tok, Tok),
) -> Row {
    Row {
        id,
        scrim,
        scrim_pointer: false,
        blocks_drop: false,
        popover_shadow,
        margin_zero,
        bg: shell.0,
        border: shell.1,
        title_edge: shell.2,
    }
}

const RAISED: (Tok, Tok, Tok) = (Tok::SurfaceRaised, Tok::BorderFrame, Tok::BorderFrame);
const PANEL: (Tok, Tok, Tok) = (Tok::BgPanel, Tok::BorderFrame, Tok::BorderFrame);

/// 정의 33개와 설정 창의 단축키 충돌 팝업, 설정 창 파일 선택기.
const EXPECTED: &[Row] = &[
    row("notifications", false, false, true, PANEL),
    row("convert_surface", true, false, false, PANEL),
    Row {
        blocks_drop: true,
        ..row("explorer_conflict", true, false, true, RAISED)
    },
    row("script_changed_confirm", false, false, true, RAISED),
    row(
        "rename",
        false,
        false,
        true,
        (Tok::DialogBg, Tok::BorderFrame, Tok::BorderFrame),
    ),
    row("explorer_properties", false, false, false, PANEL),
    row(
        "search_bar",
        false,
        true,
        false,
        (Tok::SurfaceRaised, Tok::SearchBarBorder, Tok::BorderFrame),
    ),
    row(
        "info_modal",
        false,
        false,
        true,
        (Tok::BgPanel, Tok::BorderFrame, Tok::InfoModalTitleEdge),
    ),
    row("approval", false, false, false, RAISED),
    row("file_handler_picker", false, false, false, PANEL),
    row("file_picker", false, false, true, PANEL),
    row("port_scanner", true, false, true, PANEL),
    row("command_palette", true, false, true, RAISED),
    row("dag_list", false, false, false, PANEL),
    row("remote_tool", true, false, true, PANEL),
    row("remote_attach", true, false, true, PANEL),
    row("transfer_progress", true, false, true, PANEL),
    row("transfer_error", true, false, true, PANEL),
    row("download_run", false, false, false, RAISED),
    row("download_run_large", false, false, false, RAISED),
    row("download_run_progress", false, false, false, RAISED),
    row("apply_workspace_preset", false, false, false, RAISED),
    row("apply_tab_preset", false, false, false, RAISED),
    row("apply_pane_preset", false, false, false, RAISED),
    row(
        "tools_menu",
        false,
        true,
        false,
        (Tok::SurfaceRaised, Tok::MenuBorder, Tok::BorderFrame),
    ),
    row(
        "mouse_capture_banner_menu",
        false,
        true,
        false,
        (
            Tok::SurfaceRaised,
            Tok::BannerMoreMenuBorder,
            Tok::BorderFrame,
        ),
    ),
    row("confirm_delete_category", false, false, false, RAISED),
    row(
        "confirm_force_detach_workspace",
        false,
        false,
        false,
        RAISED,
    ),
    row("confirm_close_remote", true, false, false, RAISED),
    row("remote_row", false, true, false, RAISED),
    Row {
        scrim_pointer: true,
        ..row("remote_row_over_screen", true, false, false, RAISED)
    },
    row("tutorial_topics", false, false, false, PANEL),
    row(
        "rail_category",
        false,
        true,
        false,
        (Tok::SurfaceRaised, Tok::MenuBorder, Tok::BorderFrame),
    ),
    row("keybinding_conflict", false, false, false, RAISED),
    row("local_file_chooser", false, false, true, PANEL),
];

/// 표의 모든 행이 가리키는 팝업을 등록한 그대로 모은다. 정의 목록, 설정 창의 두 팝업 순서다.
fn registered() -> Vec<PopupState> {
    let mut mgr = PopupManager::new();
    for def in super::all_defs() {
        mgr.register_def(def, 1.0);
    }
    mgr.register(PopupState::declared(
        "keybinding_conflict",
        "t",
        &crate::view::settings::ui::KEYBINDING_CONFLICT_DECL,
        ScrimInput::VisualOnly,
        None,
    ));
    crate::view::file_chooser::register(&mut mgr);
    mgr.popups
}

fn declared() -> Vec<(&'static str, PopupLook)> {
    registered().iter().map(|p| (p.id, p.look)).collect()
}

/// 선언의 셸 값이 가리키는 토큰. 그리기 코드가 같은 묶음을 쓰는지는 아래 시험이 색으로 확인한다.
fn shell_tokens(shell: PopupShell) -> (Tok, Tok, Tok) {
    match shell {
        PopupShell::Raised => RAISED,
        PopupShell::Panel => PANEL,
        PopupShell::InfoPanel => (Tok::BgPanel, Tok::BorderFrame, Tok::InfoModalTitleEdge),
        PopupShell::Dialog => (Tok::DialogBg, Tok::BorderFrame, Tok::BorderFrame),
        PopupShell::Menu => (Tok::SurfaceRaised, Tok::MenuBorder, Tok::BorderFrame),
        PopupShell::BannerMenu => (
            Tok::SurfaceRaised,
            Tok::BannerMoreMenuBorder,
            Tok::BorderFrame,
        ),
        PopupShell::SearchBar => (Tok::SurfaceRaised, Tok::SearchBarBorder, Tok::BorderFrame),
    }
}

fn row_of(id: &'static str, look: &PopupLook) -> Row {
    let (bg, border, title_edge) = shell_tokens(look.shell);
    Row {
        id,
        scrim: look.covers_scope,
        scrim_pointer: look.scrim_input == ScrimInput::TakesPointer,
        blocks_drop: look.scrim_input == ScrimInput::BlocksDrop,
        popover_shadow: look.shadow == PopupShadow::Popover,
        margin_zero: look.content_margin == PopupContentMargin::None,
        bg,
        border,
        title_edge,
    }
}

fn expected(id: &str) -> Row {
    *EXPECTED
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("`{id}` 행이 표에 없다. 정의를 더했으면 표에도 더한다"))
}

/// 표와 선언 목록이 같은 팝업을 빠짐없이 가리킨다.
#[test]
fn the_table_covers_every_declared_popup_once() {
    let ids: Vec<&str> = declared().iter().map(|(id, _)| *id).collect();
    assert_eq!(ids.len(), 35, "정의 33개와 설정 창 팝업 2개");
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "같은 id가 두 번 있다");
    assert_eq!(EXPECTED.len(), ids.len());
    for id in ids {
        expected(id);
    }
}

/// 선언 값으로 만든 행이 옛 명부의 결과와 같다.
#[test]
fn every_declaration_reproduces_the_old_lists() {
    for (id, look) in declared() {
        assert_eq!(row_of(id, &look), expected(id), "{id}");
    }
}

/// 두 내장 테마에서 그리기 코드가 선언으로 고른 색·그림자·여백이 표와 같다.
#[test]
fn drawing_reads_the_declarations_as_the_table_says() {
    let global = crate::theme::theme();
    for text in [tasty_themes::MOCHA_TOML_TEXT, tasty_themes::LATTE_TOML_TEXT] {
        let file = tasty_themes::ThemeFile::parse(text).expect("built-in theme");
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        let th = Theme::with_colors(colors, is_light.unwrap_or(false));
        for state in registered() {
            let id = state.id;
            let r = expected(id);
            let shell = super::super::draw::shell_colors(state.look.shell, &th);
            assert_eq!(shell.bg, color(r.bg, &th), "{id} bg");
            assert_eq!(shell.border, color(r.border, &th), "{id} border");
            assert_eq!(
                shell.title_edge,
                color(r.title_edge, &th),
                "{id} title edge"
            );
            let shadow = super::super::draw::shadow_token(state.look.shadow, &global);
            let want = if r.popover_shadow {
                global.shadow_popover()
            } else {
                global.shadow_modal()
            };
            assert_eq!(shadow, want, "{id} shadow");
            let mut sized = state.clone();
            sized.size = egui::vec2(300.0, 200.0);
            sized.headless = true;
            let margin = sized.content_rect().min.x - sized.popup_rect().min.x;
            assert_eq!(margin == 0.0, r.margin_zero, "{id} margin");
        }
    }
}

/// 등록한 범위의 종류가 선언의 범위 종류와 같다. 지금 정의는 모두 Window다.
#[test]
fn registered_scopes_match_the_declared_scope_kind() {
    let mut mgr = PopupManager::new();
    for def in super::all_defs() {
        mgr.register_def(def, 1.0);
        let state = mgr.get_mut(def.id).expect("registered");
        assert_eq!(
            super::super::scope_kind(&state.scope),
            def.decl.scope_kind,
            "{}",
            def.id
        );
    }
}
