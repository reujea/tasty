//! T-11-1·T-11-3: `route_key` 표 시험과 egui surface 내용 겹침 측정. 장면마다 시험 함수 하나다.

use super::*;
use crate::adapters::ui::input::key_level::KEYBINDING_LEVELS;
use crate::settings::KeybindingSettings;
use egui::{Key, Modifiers};

const TERM: u32 = 7;
const SID: u32 = 9;

fn key(key: Key) -> RouteKey {
    with(key, Modifiers::NONE)
}

fn with(key: Key, modifiers: Modifiers) -> RouteKey {
    RouteKey {
        key: Some(key),
        modifiers,
        super_held: false,
    }
}

fn host(n: u64) -> PopupNode {
    PopupNode::Host(PopupInstanceId(n))
}

fn popup(n: u64, keyboard: PopupKeyboard, escape: PopupEscape, covers: bool) -> KeyNode {
    KeyNode::Popup(PopupKeyNode {
        node: host(n),
        keyboard,
        escape,
        fold: PopupFold::Fits,
        keymap: false,
        covers,
    })
}

fn scope(level: KeyLevel, content: ScopeContent, attached: Vec<AttachedPopup>) -> KeyNode {
    KeyNode::Scope(ScopeKeyNode {
        level,
        content,
        attached,
    })
}

fn attached(n: u64, escape: PopupEscape) -> AttachedPopup {
    AttachedPopup {
        node: host(n),
        escape,
    }
}

/// Window에 알림 목록(Close, 3번)이 붙은 범위 노드들. `from`부터 Window까지.
fn scopes_from(from: KeyLevel, surface: ScopeContent) -> Vec<KeyNode> {
    let mut out = Vec::new();
    let chain = [
        KeyLevel::Surface,
        KeyLevel::Tab,
        KeyLevel::Pane,
        KeyLevel::Workspace,
        KeyLevel::Window,
    ];
    let start = chain.iter().position(|l| *l == from).expect("level");
    for level in &chain[start..] {
        let content = if *level == KeyLevel::Surface {
            surface
        } else {
            ScopeContent::None
        };
        let notices = if *level == KeyLevel::Window {
            vec![attached(3, PopupEscape::Close)]
        } else {
            Vec::new()
        };
        out.push(scope(*level, content, notices));
    }
    out
}

fn terminal_path() -> Vec<KeyNode> {
    scopes_from(KeyLevel::Surface, ScopeContent::Yielding(TERM))
}

fn on(start: KeyNode, rest: Vec<KeyNode>) -> Vec<KeyNode> {
    std::iter::once(start).chain(rest).collect()
}

fn route(path: &[KeyNode], k: RouteKey, matching: &[KeyLevel]) -> KeyReceiver {
    route_key(path, k, &EditBindings::default(), matching).receiver
}

fn esc() -> RouteKey {
    key(Key::Escape)
}

// --- 출발 노드 내용 ---

/// [출발 노드 내용] 터미널 포커스: 글자와 Esc는 PTY, 경로에 맞는 단축키가 있으면 단축키(끝 목적지 PTY).
#[test]
fn a_terminal_start_takes_what_no_ancestor_binding_takes() {
    let path = terminal_path();
    assert_eq!(route(&path, key(Key::A), &[]), KeyReceiver::Surface(TERM));
    assert_eq!(route(&path, esc(), &[]), KeyReceiver::Surface(TERM));
    assert_eq!(
        route(&path, with(Key::T, Modifiers::CTRL), &[KeyLevel::Pane]),
        KeyReceiver::Shortcut {
            end: KeyEnd::Surface(TERM)
        }
    );
}

/// 범위 노드에 내용이 없으면(빈 pane) 단축키 아니면 버림.
#[test]
fn an_empty_scope_start_drops_what_no_shortcut_takes() {
    let path = scopes_from(KeyLevel::Pane, ScopeContent::None);
    assert_eq!(route(&path, key(Key::A), &[]), KeyReceiver::Drop);
    assert_eq!(
        route(&path, key(Key::A), &[KeyLevel::Window]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

// --- 팝업 노드: 소비 집합·Text 명령 키·표시 단계·노드 키맵 ---

/// [부모 없는 팝업] 터미널에 붙은 `Keys` 팝업: 집합 안 키는 그 팝업이 받는다.
#[test]
fn a_keys_popup_takes_keys_in_its_set() {
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, false),
        scopes_from(KeyLevel::Surface, ScopeContent::Yielding(TERM))
            .into_iter()
            .map(|n| match n {
                KeyNode::Scope(mut s) if s.level == KeyLevel::Surface => {
                    s.attached = vec![attached(1, PopupEscape::Close)];
                    KeyNode::Scope(s)
                }
                n => n,
            })
            .collect(),
    );
    for k in [
        key(Key::ArrowDown),
        key(Key::A),
        with(Key::Tab, Modifiers::SHIFT),
    ] {
        assert_eq!(route(&path, k, &[]), KeyReceiver::Popup(PopupInstanceId(1)));
    }
}

/// [같은 범위 부모 팝업] 부모 팝업(2번)과 자식(1번)이 같은 범위에 붙어 있어도 수신자는 목록의 출발 노드(자식)다.
#[test]
fn a_child_popup_on_its_parent_scope_receives_as_the_start_node() {
    let mut rest = terminal_path();
    if let KeyNode::Scope(s) = &mut rest[0] {
        s.attached = vec![
            attached(2, PopupEscape::Close),
            attached(1, PopupEscape::Close),
        ];
    }
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, false),
        rest,
    );
    assert_eq!(
        route(&path, key(Key::ArrowDown), &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
}

/// [붙은 팝업 위로] `Keys` 팝업이 쓰지 않는 명령 키는 붙은 노드의 내용(PTY)을 건너뛰고 경로 단축키로 가고,
/// 단축키가 없으면 버린다(J-6).
#[test]
fn a_key_a_keys_popup_leaves_skips_the_attached_terminal() {
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, false),
        terminal_path(),
    );
    let ctrl_t = with(Key::T, Modifiers::CTRL);
    assert_eq!(route(&path, ctrl_t, &[]), KeyReceiver::Drop);
    assert_eq!(
        route(&path, ctrl_t, &[KeyLevel::Surface]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// `Text` 팝업의 집합 밖 명령 키: 경로에 맞는 후보가 있으면 단축키, 없으면 그 팝업(입력칸 내장 조합).
#[test]
fn a_text_popup_yields_command_keys_only_to_a_path_shortcut() {
    let path = on(
        popup(1, PopupKeyboard::Text, PopupEscape::Close, false),
        terminal_path(),
    );
    let ctrl_a = with(Key::A, Modifiers::CTRL);
    assert_eq!(
        route(&path, ctrl_a, &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
    assert_eq!(
        route(&path, ctrl_a, &[KeyLevel::Engine]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    assert_eq!(
        route(&path, with(Key::A, Modifiers::SHIFT), &[KeyLevel::Engine]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
}

/// 덮는 팝업은 덮인 범위의 level을 경로에서 뺀다. 그 level의 후보만 맞으면 `Text` 팝업이 받는다.
#[test]
fn a_covering_popup_filters_the_covered_level() {
    let path = on(
        popup(1, PopupKeyboard::Text, PopupEscape::Close, true),
        scopes_from(KeyLevel::Tab, ScopeContent::None),
    );
    let routed = route_key(
        &path,
        with(Key::A, Modifiers::CTRL),
        &EditBindings::default(),
        &[KeyLevel::Surface],
    );
    assert_eq!(routed.receiver, KeyReceiver::Popup(PopupInstanceId(1)));
    assert!(!routed.levels.contains(KeyLevel::Surface));
    assert!(routed.levels.contains(KeyLevel::Tab));
}

/// 같은 범위를 쓰는 덮는 팝업과 붙은 팝업은 노드 목록이 달라 수신자가 갈린다.
#[test]
fn covering_and_attached_popups_on_one_scope_route_by_their_node_lists() {
    let attached_path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, false),
        terminal_path(),
    );
    let covering_path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, true),
        scopes_from(KeyLevel::Tab, ScopeContent::None),
    );
    let ctrl_t = with(Key::T, Modifiers::CTRL);
    assert_eq!(
        route(&attached_path, ctrl_t, &[KeyLevel::Surface]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    assert_eq!(
        route(&covering_path, ctrl_t, &[KeyLevel::Surface]),
        KeyReceiver::Drop
    );
}

/// 펼친 팝업은 Window를 덮어 engine 후보만 남는다.
#[test]
fn an_expanded_popup_keeps_only_engine_candidates() {
    let path = vec![KeyNode::Popup(PopupKeyNode {
        fold: PopupFold::Expanded,
        ..match popup(1, PopupKeyboard::Keys, PopupEscape::Close, true) {
            KeyNode::Popup(p) => p,
            _ => unreachable!(),
        }
    })];
    let ctrl_t = with(Key::T, Modifiers::CTRL);
    assert_eq!(route(&path, ctrl_t, &[KeyLevel::Window]), KeyReceiver::Drop);
    assert_eq!(
        route(&path, ctrl_t, &[KeyLevel::Engine]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// 표시 단계: 접힌 대표 버튼은 펼치기 키를 받는다(소비 집합이 없는 노드여도).
#[test]
fn a_collapsed_popup_takes_the_expand_key() {
    let collapsed = |fold| {
        vec![KeyNode::Popup(PopupKeyNode {
            node: host(1),
            keyboard: PopupKeyboard::None,
            escape: PopupEscape::Close,
            fold,
            keymap: false,
            covers: false,
        })]
    };
    assert_eq!(
        route(&collapsed(PopupFold::Collapsed), key(Key::Enter), &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
    assert_eq!(
        route(&collapsed(PopupFold::Fits), key(Key::Enter), &[]),
        KeyReceiver::Drop
    );
}

/// 노드 키맵: 소비 집합 밖 키도 키맵에 맞으면 그 노드가 받는다(폴더 Alt+↑).
#[test]
fn a_node_keymap_hit_takes_a_key_outside_the_set() {
    let mut node = match popup(1, PopupKeyboard::Text, PopupEscape::Close, true) {
        KeyNode::Popup(p) => p,
        _ => unreachable!(),
    };
    let alt_up = with(Key::ArrowUp, Modifiers::ALT);
    let path = |node| {
        on(
            KeyNode::Popup(node),
            scopes_from(KeyLevel::Tab, ScopeContent::None),
        )
    };
    assert_eq!(
        route(&path(node), alt_up, &[KeyLevel::Window]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    node.keymap = true;
    assert_eq!(
        route(&path(node), alt_up, &[KeyLevel::Window]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
}

/// plugin 팝업 출발은 plugin 팝업 수신자다.
#[test]
fn a_plugin_popup_start_is_a_plugin_popup_receiver() {
    let path = on(
        KeyNode::Popup(PopupKeyNode {
            node: PopupNode::Plugin(42),
            keyboard: PopupKeyboard::Keys,
            escape: PopupEscape::Close,
            fold: PopupFold::Fits,
            keymap: false,
            covers: false,
        }),
        terminal_path(),
    );
    assert_eq!(route(&path, key(Key::A), &[]), KeyReceiver::PluginPopup(42));
    assert_eq!(route(&path, esc(), &[]), KeyReceiver::PluginPopup(42));
}

// --- Esc 선언 ---

/// [Close] Esc 선언이 Close면 그 팝업이 받는다.
#[test]
fn a_close_popup_takes_escape() {
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, true),
        scopes_from(KeyLevel::Tab, ScopeContent::None),
    );
    assert_eq!(
        route(&path, esc(), &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
}

/// Unfocus도 그 팝업에서 끝난다.
#[test]
fn an_unfocus_popup_takes_escape() {
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Unfocus, true),
        scopes_from(KeyLevel::Tab, ScopeContent::None),
    );
    assert_eq!(
        route(&path, esc(), &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
}

/// [Ignore] 덮는 Ignore 팝업의 Esc는 부모 경로의 다음 수신자(Window에 붙은 알림 목록)다.
#[test]
fn an_ignore_popup_passes_escape_to_the_parent_path() {
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Ignore, true),
        scopes_from(KeyLevel::Tab, ScopeContent::None),
    );
    assert_eq!(
        route(&path, esc(), &[]),
        KeyReceiver::Popup(PopupInstanceId(3))
    );
    assert_eq!(
        route(&path, esc(), &[KeyLevel::Pane]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// 펼친 Ignore 팝업은 다시 접는 Esc를 받는다.
#[test]
fn an_expanded_ignore_popup_takes_escape() {
    let path = vec![KeyNode::Popup(PopupKeyNode {
        node: host(1),
        keyboard: PopupKeyboard::Keys,
        escape: PopupEscape::Ignore,
        fold: PopupFold::Expanded,
        keymap: false,
        covers: true,
    })];
    assert_eq!(
        route(&path, esc(), &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
}

/// [붙은 팝업 J-6] 터미널에 붙은 Ignore 팝업의 Esc는 터미널 내용(PTY)과 형제 팝업을 건너뛰고 Window에 붙은
/// 알림 목록이 받는다.
#[test]
fn escape_from_an_attached_popup_skips_the_attached_node_content() {
    let mut rest = terminal_path();
    if let KeyNode::Scope(s) = &mut rest[0] {
        s.attached = vec![
            attached(5, PopupEscape::Close),
            attached(1, PopupEscape::Ignore),
        ];
    }
    let path = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Ignore, false),
        rest,
    );
    let receivers: Vec<_> = [esc()]
        .into_iter()
        .map(|k| route(&path, k, &[]))
        .filter(|r| r.is_egui())
        .collect();
    assert_eq!(receivers, [KeyReceiver::Popup(PopupInstanceId(3))]);
}

/// 붙은 비덮음 팝업은 하위(내용 없는 Tab)에서 올라온 Esc를 z 최상단부터 받는다. Ignore는 건너뛴다.
#[test]
fn attached_popups_take_escape_from_below_top_first() {
    let mut path = scopes_from(KeyLevel::Tab, ScopeContent::None);
    if let Some(KeyNode::Scope(window)) = path.last_mut() {
        window.attached = vec![
            attached(4, PopupEscape::Ignore),
            attached(6, PopupEscape::Unfocus),
            attached(3, PopupEscape::Close),
        ];
    }
    assert_eq!(
        route(&path, esc(), &[]),
        KeyReceiver::Popup(PopupInstanceId(6))
    );
}

/// 수식키 Esc는 Esc 선언의 대상이 아니다: `Keys` 팝업의 Shift+Esc는 위로, `Text` 팝업은 그 팝업.
#[test]
fn a_modified_escape_is_an_ordinary_key() {
    let shift_esc = with(Key::Escape, Modifiers::SHIFT);
    let keys = on(
        popup(1, PopupKeyboard::Keys, PopupEscape::Close, false),
        terminal_path(),
    );
    assert_eq!(route(&keys, shift_esc, &[]), KeyReceiver::Drop);
    let text = on(
        popup(1, PopupKeyboard::Text, PopupEscape::Ignore, false),
        terminal_path(),
    );
    assert_eq!(
        route(&text, shift_esc, &[]),
        KeyReceiver::Popup(PopupInstanceId(1))
    );
    assert_eq!(
        route(
            &text,
            with(Key::Escape, Modifiers::CTRL),
            &[KeyLevel::Window]
        ),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

// --- 비팝업 출발점 ---

/// 시험 안내 인스턴스.
fn notice_iid() -> PopupInstanceId {
    PopupInstanceId(7)
}

/// 복원 안내: 위젯 키는 안내, 나머지는 Window 키맵 → engine(PTY 아님).
#[test]
fn a_notice_start_takes_its_widget_keys() {
    let path = on(
        KeyNode::Notice(notice_iid()),
        scopes_from(KeyLevel::Window, ScopeContent::None),
    );
    assert_eq!(
        route(&path, key(Key::Enter), &[]),
        KeyReceiver::Notice(notice_iid())
    );
    assert_eq!(route(&path, esc(), &[]), KeyReceiver::Notice(notice_iid()));
    assert_eq!(
        route(&path, with(Key::Escape, Modifiers::SHIFT), &[]),
        KeyReceiver::Notice(notice_iid())
    );
    assert_eq!(
        route(&path, key(Key::A), &[KeyLevel::Surface]),
        KeyReceiver::Drop
    );
    assert_eq!(
        route(&path, key(Key::A), &[KeyLevel::Window]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// 튜토리얼 말풍선: Esc·Enter·Space는 말풍선, 글자는 PTY 아님.
#[test]
fn a_tutorial_start_takes_its_widget_keys() {
    let path = on(
        KeyNode::Tutorial,
        scopes_from(KeyLevel::Window, ScopeContent::None),
    );
    for k in [esc(), key(Key::Enter), key(Key::Space)] {
        assert_eq!(route(&path, k, &[]), KeyReceiver::Tutorial);
    }
    assert_eq!(route(&path, key(Key::A), &[]), KeyReceiver::Drop);
}

/// 주소 띠: 키맵에 맞거나 `Text` 집합이면 주소 띠, 쓰지 않는 명령 키는 surface 키맵 위로(WebView 내용 아님).
#[test]
fn an_address_band_start_is_a_text_node_over_its_surface() {
    let path = |keymap| {
        on(
            KeyNode::HtmlAddress { sid: SID, keymap },
            scopes_from(KeyLevel::Surface, ScopeContent::Yielding(SID)),
        )
    };
    assert_eq!(
        route(&path(false), key(Key::A), &[]),
        KeyReceiver::HtmlAddress(SID)
    );
    assert_eq!(
        route(&path(false), esc(), &[]),
        KeyReceiver::HtmlAddress(SID)
    );
    assert_eq!(
        route(&path(true), key(Key::F5), &[KeyLevel::Surface]),
        KeyReceiver::HtmlAddress(SID)
    );
    assert_eq!(
        route(
            &path(false),
            with(Key::T, Modifiers::CTRL),
            &[KeyLevel::Surface]
        ),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
}

/// 무대: 종료 키가 먼저, 내용 집합, 그 밖의 명령 키는 engine 후보만.
#[test]
fn a_stage_start_keeps_only_engine_candidates() {
    let path = |exit| vec![KeyNode::Stage { exit }];
    let ctrl_q = with(Key::Q, Modifiers::CTRL);
    assert_eq!(
        route(&path(true), ctrl_q, &[KeyLevel::Engine]),
        KeyReceiver::Stage
    );
    assert_eq!(
        route(&path(false), ctrl_q, &[KeyLevel::Engine]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    assert_eq!(
        route(&path(false), ctrl_q, &[KeyLevel::Window]),
        KeyReceiver::Stage
    );
    assert_eq!(route(&path(false), key(Key::A), &[]), KeyReceiver::Stage);
    assert_eq!(route(&path(false), esc(), &[]), KeyReceiver::Stage);
}

/// egui surface: 내용이 쓰는 키는 내용이 먼저(양보하지 않음), 쓰지 않는 키는 단축키, 아니면 내용.
#[test]
fn an_egui_surface_content_comes_before_shortcuts() {
    let path = |takes| scopes_from(KeyLevel::Surface, ScopeContent::Egui { sid: SID, takes });
    let up = key(Key::ArrowUp);
    assert_eq!(
        route(&path(true), up, &[KeyLevel::Surface]),
        KeyReceiver::EguiSurface(SID)
    );
    assert_eq!(
        route(&path(false), up, &[KeyLevel::Surface]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    assert_eq!(route(&path(false), up, &[]), KeyReceiver::EguiSurface(SID));
    assert_eq!(
        route(&path(true), esc(), &[]),
        KeyReceiver::EguiSurface(SID)
    );
    assert_eq!(
        route(&path(false), esc(), &[KeyLevel::Surface]),
        KeyReceiver::Shortcut { end: KeyEnd::Drop }
    );
    assert_eq!(
        route(&path(false), esc(), &[]),
        KeyReceiver::Popup(PopupInstanceId(3))
    );
}

/// 출발점 하나에서 egui 수신자는 많아야 하나다(키·수식키·후보를 바꿔도).
#[test]
fn one_start_node_has_at_most_one_egui_receiver() {
    let starts: Vec<Vec<KeyNode>> = vec![
        terminal_path(),
        on(
            popup(1, PopupKeyboard::Keys, PopupEscape::Ignore, false),
            terminal_path(),
        ),
        on(
            popup(1, PopupKeyboard::Text, PopupEscape::Close, true),
            scopes_from(KeyLevel::Tab, ScopeContent::None),
        ),
        on(
            KeyNode::Notice(notice_iid()),
            scopes_from(KeyLevel::Window, ScopeContent::None),
        ),
        on(
            KeyNode::Tutorial,
            scopes_from(KeyLevel::Window, ScopeContent::None),
        ),
        on(
            KeyNode::HtmlAddress {
                sid: SID,
                keymap: false,
            },
            scopes_from(KeyLevel::Surface, ScopeContent::Yielding(SID)),
        ),
        vec![KeyNode::Stage { exit: false }],
        scopes_from(
            KeyLevel::Surface,
            ScopeContent::Egui {
                sid: SID,
                takes: true,
            },
        ),
    ];
    let keys = [
        key(Key::A),
        key(Key::ArrowDown),
        key(Key::Enter),
        with(Key::A, Modifiers::CTRL),
        with(Key::A, Modifiers::SHIFT),
        with(Key::Escape, Modifiers::SHIFT),
        key(Key::F5),
    ];
    let matchings: [&[KeyLevel]; 3] = [&[], &[KeyLevel::Engine], &[KeyLevel::Surface]];
    for path in &starts {
        let mut egui: Vec<KeyReceiver> = Vec::new();
        for k in keys {
            for m in matchings {
                let r = route(path, k, m);
                if r.is_egui() && !egui.contains(&r) {
                    egui.push(r);
                }
            }
        }
        assert!(egui.len() <= 1, "{path:?}: {egui:?}");
    }
}

// --- T-11-3 겹침 측정 ---

/// egui 입력칸 내장 조합(egui 0.31.1 `TextEdit`·`CCursorRange`가 읽는 명령 키, Linux·Windows의 command = Ctrl).
/// 전체 선택, 실행 취소·다시 실행, 단어·줄 이동(Shift 포함), 단어 지우기, Ctrl+H·K·U·W 지우기.
fn text_edit_builtin_combos() -> Vec<(Key, Modifiers)> {
    let ctrl = Modifiers::CTRL;
    let ctrl_shift = Modifiers::CTRL | Modifiers::SHIFT;
    let mut out = vec![
        (Key::A, ctrl),
        (Key::Z, ctrl),
        (Key::Z, ctrl_shift),
        (Key::Y, ctrl),
        (Key::Backspace, ctrl),
        (Key::Delete, ctrl),
        (Key::H, ctrl),
        (Key::K, ctrl),
        (Key::U, ctrl),
        (Key::W, ctrl),
    ];
    for k in [
        Key::ArrowLeft,
        Key::ArrowRight,
        Key::ArrowUp,
        Key::ArrowDown,
        Key::Home,
        Key::End,
    ] {
        out.extend([(k, ctrl), (k, ctrl_shift)]);
    }
    for k in [Key::ArrowLeft, Key::ArrowRight] {
        out.extend([(k, Modifiers::ALT), (k, Modifiers::ALT | Modifiers::SHIFT)]);
    }
    out
}

/// 기본 바인딩마다 (필드, 바인딩, level, 맞는 egui 누름).
fn default_bindings() -> Vec<(&'static str, String, KeyLevel, Key, Modifiers)> {
    let kb = KeybindingSettings::default();
    let mods = [
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::CTRL,
        Modifiers::ALT,
        Modifiers::CTRL | Modifiers::SHIFT,
        Modifiers::ALT | Modifiers::SHIFT,
    ];
    let mut out = Vec::new();
    for (field, level) in KEYBINDING_LEVELS {
        let Some(level) = level.level() else { continue };
        let Some(bindings) = kb.get_bindings(field) else {
            continue;
        };
        for binding in bindings {
            for k in Key::ALL {
                for m in mods {
                    if tasty_key_match::binding_matches_egui(
                        std::slice::from_ref(binding),
                        *k,
                        m,
                        false,
                    ) {
                        out.push((*field, binding.clone(), level, *k, m));
                    }
                }
            }
        }
    }
    out
}

/// T-11-3 (가): explorer 내용(입력칸이 활성인 동안 `Text` 집합과 입력칸 내장 조합을 쓴다)이 소비하는 키와
/// Surface level 이상 기본 바인딩의 겹침. dag graph 내용은 egui 키를 읽지 않는다(내용 소스에 키 읽기가 없다).
/// 겹침이 있으면 egui surface 내용은 조상 바인딩에 양보하지 않는다(결정 J-7).
#[test]
fn egui_surface_content_keys_overlap_surface_bindings() {
    let builtin = text_edit_builtin_combos();
    let overlap: Vec<String> = default_bindings()
        .into_iter()
        .filter(|(_, _, _, k, m)| {
            consumes_key(PopupKeyboard::Text, *k, *m, false, &EditBindings::default())
                || builtin.contains(&(*k, *m))
        })
        .map(|(field, binding, level, _, _)| format!("{field}={binding}({level:?})"))
        .collect();
    assert!(!overlap.is_empty());
    #[cfg(not(target_os = "macos"))]
    assert_eq!(overlap, MEASURED_CONTENT_OVERLAP);
    assert_eq!(EGUI_SURFACE_YIELDS_TO_ANCESTOR_BINDINGS, overlap.is_empty());
}

/// T-11-3 (나): egui 입력칸 내장 조합과 모든 level 기본 바인딩의 겹침. 덮는 팝업에서 경로에 남는 level의 겹친
/// 키는 `Text` 노드에서 단축키가 이긴다(`Text` 명령 키 규칙).
#[test]
fn text_edit_builtin_combos_overlap_default_bindings() {
    let builtin = text_edit_builtin_combos();
    let overlap: Vec<String> = default_bindings()
        .into_iter()
        .filter(|(_, _, _, k, m)| builtin.contains(&(*k, *m)))
        .map(|(field, binding, level, _, _)| format!("{field}={binding}({level:?})"))
        .collect();
    #[cfg(not(target_os = "macos"))]
    assert_eq!(overlap, MEASURED_BUILTIN_OVERLAP);
    // 덮는 팝업에서도 경로에 남는 engine·Window level의 겹침은 없다.
    assert!(
        !overlap
            .iter()
            .any(|o| o.ends_with("(Engine)") || o.ends_with("(Window)")),
        "{overlap:?}"
    );
}

/// T-11-3 (가) 측정값(Linux·Windows 기본 바인딩).
#[cfg(not(target_os = "macos"))]
const MEASURED_CONTENT_OVERLAP: &[&str] = &[
    "fullscreen_stage_exit=escape(Window)",
    "code_area_cancel=escape(Surface)",
    "explorer_cursor_up=up(Surface)",
    "explorer_cursor_down=down(Surface)",
    "explorer_cursor_left=left(Surface)",
    "explorer_cursor_right=right(Surface)",
    "explorer_cursor_home=home(Surface)",
    "explorer_cursor_end=end(Surface)",
    "explorer_cursor_page_up=pageup(Surface)",
    "explorer_cursor_page_down=pagedown(Surface)",
    "explorer_extend_up=shift+up(Surface)",
    "explorer_extend_down=shift+down(Surface)",
    "explorer_extend_left=shift+left(Surface)",
    "explorer_extend_right=shift+right(Surface)",
    "explorer_extend_home=shift+home(Surface)",
    "explorer_extend_end=shift+end(Surface)",
    "explorer_extend_page_up=shift+pageup(Surface)",
    "explorer_extend_page_down=shift+pagedown(Surface)",
    "explorer_open=enter(Surface)",
    "explorer_clear_selection=escape(Surface)",
    "close_active=ctrl+w(Tab)",
    "select_all=ctrl+a(Surface)",
    "refresh=f5(Surface)",
    "explorer_refresh=f5(Surface)",
    "explorer_back=alt+left(Surface)",
    "explorer_forward=alt+right(Surface)",
    "explorer_new_folder=f7(Surface)",
    "explorer_new_file=shift+f4(Surface)",
    "explorer_rename=f2(Surface)",
    "explorer_trash=delete(Surface)",
    "explorer_undo=ctrl+z(Surface)",
    "explorer_redo=ctrl+shift+z(Surface)",
    "explorer_redo=ctrl+y(Surface)",
    "page_undo=ctrl+z(Surface)",
    "page_redo=ctrl+shift+z(Surface)",
    "page_redo=ctrl+y(Surface)",
    "rename_tab=f2(Tab)",
    "rename_workspace=f3(Workspace)",
    "rename_workspace_subtitle=f4(Workspace)",
];

/// T-11-3 (나) 측정값(Linux·Windows 기본 바인딩).
#[cfg(not(target_os = "macos"))]
const MEASURED_BUILTIN_OVERLAP: &[&str] = &[
    "close_active=ctrl+w(Tab)",
    "select_all=ctrl+a(Surface)",
    "explorer_back=alt+left(Surface)",
    "explorer_forward=alt+right(Surface)",
    "explorer_undo=ctrl+z(Surface)",
    "explorer_redo=ctrl+shift+z(Surface)",
    "explorer_redo=ctrl+y(Surface)",
    "page_undo=ctrl+z(Surface)",
    "page_redo=ctrl+shift+z(Surface)",
    "page_redo=ctrl+y(Surface)",
];
