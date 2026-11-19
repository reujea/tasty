//! 정의마다 선언 값이 지금 그리기 결과와 같은지 표로 고정한다.
//!
//! `EXPECTED`는 선언 값을 도입하기 전 id 명부(scrim·scrim 포인터·탐색기 끌어 놓기 차단·그림자·
//! 콘텐츠 여백·배경·테두리·제목줄 아래 선)가 정의마다 낸 결과를 옮겨 적은 것이다. 명부를 지우기 전
//! 커밋에서 명부 함수의 결과와 이 표가 같은 것을 확인했다.
//! 정의를 더하면 이 표에도 행을 더한다. 표에 없는 정의가 있으면 실패한다.

use super::super::{
    PopupContentMargin, PopupDecl, PopupEscape, PopupKeyboard, PopupLook, PopupManager,
    PopupOutsideClick, PopupShadow, PopupShell, PopupState,
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
    /// 관리자가 셸을 칠하지 않는다(콘텐츠가 그린다).
    Transparent,
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
        Tok::Transparent => Color32::TRANSPARENT,
    }
}

/// 옛 명부가 정의 하나에 낸 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    id: &'static str,
    scrim: bool,
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
        popover_shadow,
        margin_zero,
        bg: shell.0,
        border: shell.1,
        title_edge: shell.2,
    }
}

const RAISED: (Tok, Tok, Tok) = (Tok::SurfaceRaised, Tok::BorderFrame, Tok::BorderFrame);
const PANEL: (Tok, Tok, Tok) = (Tok::BgPanel, Tok::BorderFrame, Tok::BorderFrame);
const BY_CONTENT: (Tok, Tok, Tok) = (Tok::Transparent, Tok::Transparent, Tok::Transparent);

/// 정의 34개와 설정 창의 단축키 충돌 팝업.
const EXPECTED: &[Row] = &[
    row("notifications", false, false, true, PANEL),
    row("convert_surface", true, false, false, PANEL),
    row("explorer_conflict", true, false, true, RAISED),
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
    row("tutorial_topics", false, false, false, PANEL),
    row(
        "rail_category",
        false,
        true,
        false,
        (Tok::SurfaceRaised, Tok::MenuBorder, Tok::BorderFrame),
    ),
    // 단축키 도움말은 옛 명부에 없던 정의다. 셸은 콘텐츠가 modhint 토큰으로 그린다.
    row("modifier_hint", false, true, true, BY_CONTENT),
    // 복원 안내도 옛 명부에 없던 정의다. 카드는 콘텐츠가 그린다.
    row("restore_notice", false, true, true, BY_CONTENT),
    row("keybinding_conflict", false, false, false, RAISED),
];

/// 표의 모든 행이 가리키는 팝업을 등록한 그대로 모은다. 정의 목록, 설정 창의 충돌 확인 순서다.
fn registered() -> Vec<PopupState> {
    let mut mgr = PopupManager::new();
    for def in super::all_defs() {
        mgr.register_def(def, 1.0);
    }
    mgr.register_def(&crate::view::settings::ui::keybinding_conflict_def(), 1.0);
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
        PopupShell::Content => BY_CONTENT,
    }
}

fn row_of(id: &'static str, look: &PopupLook) -> Row {
    let (bg, border, title_edge) = shell_tokens(look.shell);
    Row {
        id,
        scrim: look.covers_scope,
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
    assert_eq!(ids.len(), 35, "정의 34개와 설정 창의 단축키 충돌 확인");
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
            let mut sized = state.fresh_copy();
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

/// 아직 읽는 곳이 없는 입력 속성의 기대 값. 각 값은 지금 코드가 하는 일을 옮겨 적었다.
/// - keyboard: 글자를 편집하는 입력칸이 있으면 Text.
/// - escape: 포커스된 팝업에서 Esc를 누른 결과. 키 처리 경로는 포커스를 놓고, 바깥 클릭에 닫히는 팝업이면
///   닫는다. 같은 키를 받은 그리기 함수가 닫으면 Close다. 둘 다 닫지 않으면 Unfocus다.
/// - focus_on_open: 사용자가 여는 주 경로가 포커스를 주는지. 사용자 경로가 없는 팝업은 false다.
///   경로마다 다른 팝업은 notifications 하나다(사이드바는 포커스, 단축키는 포커스 없음).
const INPUT_EXPECTED: &[(&str, PopupKeyboard, PopupEscape, PopupOutsideClick, bool)] = {
    use PopupEscape::{Close as EscClose, Unfocus as EscUnfocus};
    use PopupKeyboard::{Keys, Text};
    use PopupOutsideClick::{Close as OutClose, Unfocus as OutUnfocus};
    &[
        ("notifications", Keys, EscClose, OutUnfocus, true),
        ("convert_surface", Keys, EscClose, OutClose, true),
        ("explorer_conflict", Keys, EscUnfocus, OutUnfocus, true),
        ("script_changed_confirm", Keys, EscClose, OutUnfocus, true),
        ("rename", Text, EscClose, OutUnfocus, true),
        ("explorer_properties", Keys, EscUnfocus, OutUnfocus, true),
        ("search_bar", Text, EscClose, OutUnfocus, true),
        ("info_modal", Keys, EscClose, OutUnfocus, true),
        ("approval", Text, EscUnfocus, OutUnfocus, false),
        ("file_handler_picker", Keys, EscClose, OutUnfocus, true),
        ("file_picker", Text, EscClose, OutUnfocus, true),
        ("port_scanner", Text, EscClose, OutClose, true),
        ("command_palette", Text, EscClose, OutClose, true),
        ("dag_list", Text, EscClose, OutUnfocus, true),
        ("remote_tool", Text, EscClose, OutUnfocus, true),
        ("remote_attach", Keys, EscClose, OutUnfocus, true),
        ("transfer_progress", Keys, EscUnfocus, OutUnfocus, false),
        ("transfer_error", Keys, EscClose, OutClose, true),
        ("download_run", Keys, EscClose, OutClose, true),
        ("download_run_large", Keys, EscClose, OutClose, true),
        ("download_run_progress", Keys, EscUnfocus, OutUnfocus, false),
        ("apply_workspace_preset", Keys, EscClose, OutClose, true),
        ("apply_tab_preset", Keys, EscClose, OutClose, true),
        ("apply_pane_preset", Keys, EscClose, OutClose, true),
        ("tools_menu", Keys, EscClose, OutClose, true),
        ("mouse_capture_banner_menu", Keys, EscClose, OutClose, true),
        ("confirm_delete_category", Keys, EscClose, OutClose, true),
        (
            "confirm_force_detach_workspace",
            Keys,
            EscClose,
            OutClose,
            true,
        ),
        ("confirm_close_remote", Keys, EscClose, OutUnfocus, true),
        ("remote_row", Keys, EscUnfocus, OutUnfocus, false),
        ("tutorial_topics", Keys, EscClose, OutClose, true),
        ("rail_category", Keys, EscClose, OutClose, true),
        // 키를 받지 않고 포커스도 받지 않는다. 닫기는 키를 떼거나 X다.
        (
            "modifier_hint",
            PopupKeyboard::None,
            PopupEscape::Ignore,
            PopupOutsideClick::Ignore,
            false,
        ),
        // 팝업 포커스를 받지 않는다. 안쪽 컨트롤의 포커스와 Esc는 콘텐츠가 다룬다.
        (
            "restore_notice",
            PopupKeyboard::None,
            PopupEscape::Ignore,
            PopupOutsideClick::Ignore,
            false,
        ),
        ("keybinding_conflict", Keys, EscClose, OutUnfocus, true),
    ]
};

/// 표의 모든 팝업의 선언. 창의 파일 선택은 file_picker 정의의 인스턴스라 따로 세지 않는다.
fn all_decls() -> Vec<(&'static str, PopupDecl)> {
    let mut out: Vec<_> = super::all_defs().iter().map(|d| (d.id, d.decl)).collect();
    let conflict = crate::view::settings::ui::keybinding_conflict_def();
    out.push((conflict.id, conflict.decl));
    out
}

/// 입력 속성 표가 선언 목록과 같은 팝업을 가리키고 값이 같다.
#[test]
fn input_attributes_match_current_behavior() {
    let decls = all_decls();
    assert_eq!(decls.len(), INPUT_EXPECTED.len(), "표와 선언 수");
    for (id, decl) in decls {
        let &(_, keyboard, escape, outside, focus) = INPUT_EXPECTED
            .iter()
            .find(|r| r.0 == id)
            .unwrap_or_else(|| panic!("`{id}` 행이 입력 속성 표에 없다"));
        assert_eq!(decl.keyboard, keyboard, "{id} keyboard");
        assert_eq!(decl.escape, escape, "{id} escape");
        assert_eq!(decl.outside_click, outside, "{id} outside_click");
        assert_eq!(decl.focus_on_open, focus, "{id} focus_on_open");
    }
}
