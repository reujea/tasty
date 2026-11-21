use crate::adapters::ui::icons;
use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::t;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;
use crate::theme::Theme;
use serde_json::json;
use tasty_type_geometry::length::LogicalPx;

/// 변환 팝업 id.
pub const CONVERT_POPUP_ID: &str = "convert_surface";

/// 변환 팝업 인스턴스의 데이터.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertPayload {
    /// 바꿀 surface. 팝업 범위도 이 surface다.
    pub surface_id: u32,
    /// 키보드로 고른 항목. 처음에는 없다.
    pub selected: Option<usize>,
}

/// `surface_id`를 바꾸는 변환 팝업 요청. 범위 가운데에 포커스를 주며 연다.
///
/// surface마다 인스턴스가 따로 열린다. 같은 surface에 이미 열려 있으면 다시 열기 요청으로 선택만 처음으로 돌린다.
pub fn open_request(surface_id: u32) -> crate::intent::OpenRequest {
    crate::intent::OpenRequest::new(
        CONVERT_POPUP_ID,
        crate::model::popup_kind::PopupScope::Surface(surface_id),
    )
    .centered()
    .focused()
    .with_payload(payload(surface_id))
}

fn payload(surface_id: u32) -> popup::PopupPayload {
    popup::PopupPayload::Convert(ConvertPayload {
        surface_id,
        selected: None,
    })
}

/// debug IPC로 열 때의 데이터. surface 범위가 아니면 바꿀 surface가 없어 데이터 없이 연다.
#[cfg(debug_assertions)]
pub(crate) fn debug_payload(
    _state: &MainViewState,
    _engine: &crate::core::CoreState,
    surface: Option<u32>,
) -> popup::PopupPayload {
    surface.map_or(popup::PopupPayload::None, payload)
}

fn payload_of(payload: &popup::PopupPayload) -> Option<&ConvertPayload> {
    match payload {
        popup::PopupPayload::Convert(p) => Some(p),
        _ => None,
    }
}

/// 빌트인 비표시 kind (변환 메뉴에 등장하면 안 됨).
const HIDDEN_KINDS: &[&str] = &["empty"];
/// 메뉴의 우선 표시 순서. 나머지는 이름순으로 배치한다. kind의 실행 동작과는 별개다.
const PREFERRED_ORDER: &[&str] = &["terminal", "markdown", "image"];
/// registry가 비어 있을 수 있는 등록 시점의 임시 항목 수. 실제 크기는 sizer에서 다시 계산한다.
const DEFAULT_KIND_COUNT: usize = 5;

/// Sizer: 현재 kind 를 뺀 변환 가능 kind 수에 맞춰 popup 크기를 계산.
/// `popup::frame::draw_popup_layer`가 프레임마다 호출하므로 plugin이 새 kind를
/// 등록한 직후나 UI 배율이 바뀐 직후 자동으로 popup 크기가 맞춰진다.
pub fn convert_popup_sizer(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    payload: &popup::PopupPayload,
) -> egui::Vec2 {
    let target = payload_of(payload).map(|p| p.surface_id);
    convert_popup_size_for(&theme::theme(), listed_kinds(state, engine, target).len())
}

/// Default size used when the popup is first registered (registry가 비어 있을 수 있는 시점).
pub fn convert_popup_default_size() -> egui::Vec2 {
    // sizer 가 매 프레임 재계산하므로 실제 렌더링에는 영향 없음 — register 시점 placeholder.
    convert_popup_size_for(&theme::theme(), DEFAULT_KIND_COUNT)
}

/// 폭은 `convert-popup-width` 토큰(UI 배율 적용), 높이는 타이틀바 + 콘텐츠 여백 × 2 +
/// 행 수 × MenuItem 높이다. 행 사이 간격은 없다.
fn convert_popup_size_for(th: &Theme, count: usize) -> egui::Vec2 {
    let count = count.max(1);
    let content_h = th.menu_item_height().scaled(count as f32);
    // 누적 반올림 오차와 anti-alias 경계 잘림을 줄이기 위한 1px 여유.
    let safety_margin = 1.0;
    egui::vec2(
        th.convert_popup_width().value(),
        (popup::title_bar_height()
            + popup::content_margin().scaled(2.0)
            + content_h
            + LogicalPx(safety_margin))
        .value(),
    )
}

#[cfg(test)]
mod size_tests {
    use super::*;

    /// 마지막 항목이 잘리지 않으려면 sizer popup_h 가 *실제 필요 height* 이상이어야
    /// 한다. 실제 필요 = title_bar_height() + 2·content_margin() + N·menu_item_height.
    fn assert_fits(count: usize) {
        let th = theme::theme();
        let popup_h = convert_popup_size_for(&th, count).y;
        let needed = popup::title_bar_height()
            + popup::content_margin().scaled(2.0)
            + th.menu_item_height().scaled(count as f32);
        assert!(
            LogicalPx(popup_h) >= needed,
            "popup_h ({popup_h}) < needed ({needed}) for count={count}"
        );
    }

    #[test]
    fn fits_single_item() {
        assert_fits(1);
    }

    #[test]
    fn fits_five_items() {
        assert_fits(5);
    }

    #[test]
    fn fits_many_items() {
        assert_fits(10);
    }

    /// 폭은 옛 리터럴 200 이 아니라 토큰 240 에 UI 배율을 곱한 값이다.
    #[test]
    fn width_is_the_token_times_ui_scale() {
        for zoom in [0.85, 1.0, 1.2] {
            let base = theme::theme();
            let th = Theme::with_colors_and_zoom(base.to_colors(), base.is_light, zoom);
            assert_eq!(
                convert_popup_size_for(&th, 3).x,
                (240.0 * zoom).round(),
                "zoom {zoom}"
            );
        }
    }
}

/// PopupDef::draw_fn entry point for the convert surface popup.
pub fn draw_convert_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    let popup::PopupPayload::Convert(target) = cx.payload else {
        return PopupAction::None;
    };
    match draw_convert_content(ui, state, engine, target, cx.keys.receives()) {
        Some(ConvertResult::Close) => PopupAction::CloseBy(popup::PopupCloseReason::Escape),
        Some(ConvertResult::Action(action)) => {
            apply_convert_action(state, engine, target.surface_id, action);
            PopupAction::Close
        }
        None => PopupAction::None,
    }
}

/// 변환 가능한 surface kind 한 항목.
struct ConvertItem {
    kind: &'static str,
    label: String,
    shortcut: Option<char>,
    icon: icons::Icon,
}

/// 팝업에 나열할 항목 — 대상 surface 의 현재 kind 는 바꿀 대상이 아니라 뺀다.
fn listed_kinds(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: Option<u32>,
) -> Vec<ConvertItem> {
    let current = target.and_then(|id| current_surface_kind(state, engine, id));
    listed_items(enumerate_convertible_kinds(state, engine), current)
}

/// 현재 kind 를 뺀 뒤 단축키를 배정한다. 빠진 현재 kind 가 보이는 항목의 첫 글자를 차지하지 않는다.
fn listed_items(items: Vec<ConvertItem>, current: Option<&str>) -> Vec<ConvertItem> {
    assign_shortcuts(without_current(items, current))
}

/// kind 첫 글자(영문)를 대문자 단축키로 배정한다. 첫 글자가 겹치면 뒤 항목은 단축키가 없다.
fn assign_shortcuts(mut items: Vec<ConvertItem>) -> Vec<ConvertItem> {
    let mut used: Vec<char> = Vec::new();
    for item in &mut items {
        item.shortcut = item
            .kind
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .map(|c| c.to_ascii_uppercase())
            .filter(|c| !used.contains(c));
        if let Some(c) = item.shortcut {
            used.push(c);
        }
    }
    items
}

/// 현재 kind 를 목록에서 뺀다. 나머지 순서는 그대로다. 현재 kind 가 없으면 전부 남긴다.
fn without_current(items: Vec<ConvertItem>, current: Option<&str>) -> Vec<ConvertItem> {
    items
        .into_iter()
        .filter(|it| Some(it.kind) != current)
        .collect()
}

/// SurfaceKindRegistry로부터 변환 가능한 kind 목록을 생성.
/// - `empty` 같은 시스템 kind는 제외.
/// - PREFERRED_ORDER를 먼저, 나머지는 이름순으로 표시한다.
/// - label: `convert_popup.<kind>`가 번역되어 있으면 그 값, 아니면 registry의
///   `display_name_i18n_key`, 그것도 미번역이면 kind 자체를 대문자로.
/// - shortcut: 비워 둔다. 현재 kind 를 뺀 뒤 [`assign_shortcuts`]가 배정한다.
/// - icon: registry 의 아이콘 이름. 없으면 FILE.
fn enumerate_convertible_kinds(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> Vec<ConvertItem> {
    let snapshot = engine.surface_registry.kinds_snapshot();
    let mut kinds: Vec<&'static str> = snapshot
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| !HIDDEN_KINDS.contains(k))
        .collect();
    kinds.sort_by(|a, b| {
        let ia = PREFERRED_ORDER.iter().position(|p| *p == *a);
        let ib = PREFERRED_ORDER.iter().position(|p| *p == *b);
        match (ia, ib) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        }
    });

    let mut items = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let label = resolve_label(state, engine, kind);
        let icon = engine
            .surface_registry
            .get(kind)
            .and_then(|d| d.icon.clone())
            .map(|n| icons::from_name(&n))
            .unwrap_or(icons::FILE);
        items.push(ConvertItem {
            kind,
            label,
            shortcut: None,
            icon,
        });
    }
    items
}

fn resolve_label(
    _state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    kind: &str,
) -> String {
    let popup_key = format!("convert_popup.{kind}");
    let tr = t(&popup_key);
    if tr != popup_key.as_str() {
        return tr.to_string();
    }
    if let Some(def) = engine.surface_registry.get(kind) {
        let key = def.display_name_i18n_key;
        let tr = t(key);
        if tr != key {
            return tr.to_string();
        }
    }
    capitalize_ascii(kind)
}

fn capitalize_ascii(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
    }
}

/// Result of drawing the convert popup content.
pub enum ConvertResult {
    /// User selected an action.
    Action(ConvertAction),
    /// User pressed Escape.
    Close,
}

/// 앱 상태 없이 그릴 수 있는 메뉴 항목. 현재 kind 는 이미 빠져 있다.
#[derive(Debug, Clone)]
pub struct ConvertItemView {
    pub kind: String,
    pub label: String,
    pub icon: icons::Icon,
    /// 행 끝 키캡으로 보이는 단축키 글자. 라벨은 번역되므로 화면에 글자를 보여야 고를 수 있다.
    /// 첫 글자가 겹쳐 단축키가 없으면 None 이고 키캡과 그 자리를 모두 그리지 않는다.
    pub shortcut: Option<char>,
}

/// Props 일체. 호출처가 MainViewState/CoreState 에서 추출해서 전달.
#[derive(Debug, Clone, Default)]
pub struct ConvertProps {
    pub items: Vec<ConvertItemView>,
    /// 키보드 선택 위치. None 이면 마우스 호버만 강조.
    pub selected_index: Option<usize>,
}

/// View 의 출력 — 사용자 입력의 의미. wrapper 가 MainViewState/CoreState 에 반영.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertViewAction {
    None,
    /// 사용자가 항목을 클릭. wrapper 는 `kind` 를 `action_for_kind` 로 변환해 적용한다.
    Clicked {
        idx: usize,
        kind: String,
    },
}

/// 클릭과 강조 표시를 처리한다. Escape·방향키·Enter·문자 단축키는 호출부에서 처리한다.
pub fn draw_convert_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    props: &ConvertProps,
) -> ConvertViewAction {
    let mut action = ConvertViewAction::None;
    // MenuItem 행은 간격 없이 쌓는다.
    ui.spacing_mut().item_spacing.y = 0.0;

    for (idx, item) in props.items.iter().enumerate() {
        let icon = item.icon;
        // 키캡은 표시 전용이다. 글자 입력은 draw_convert_content 가 처리한다.
        let letter = item.shortcut.map(String::from);
        let keys: Vec<&str> = letter.as_deref().into_iter().collect();
        let resp = tasty_ui_widgets::menu_item_kbd(
            ui,
            theme,
            Some(&|ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                icon.image(rect.height(), c).paint_at(ui, rect);
            }),
            &item.label,
            &keys,
            tasty_ui_widgets::MenuItemVariant::Normal,
            props.selected_index == Some(idx),
            true,
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            action = ConvertViewAction::Clicked {
                idx,
                kind: item.kind.clone(),
            };
        }
    }

    action
}

/// 앱 상태에서 메뉴 입력을 만들고 키보드·선택 결과를 처리한다.
/// 키 입력은 포커스를 가진 인스턴스(`keys`)만 읽는다. 다른 surface의 변환 팝업이 같은 키에 반응하지 않는다.
pub fn draw_convert_content(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: &mut ConvertPayload,
    keys: bool,
) -> Option<ConvertResult> {
    let internal_items = listed_kinds(state, engine, Some(target.surface_id));
    let count = internal_items.len();

    let ctx = ui.ctx().clone();

    if keys && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(ConvertResult::Close);
    }

    let selected = target.selected;

    if keys && ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) && count > 0 {
        let new_sel = match selected {
            Some(cur) if cur < count => (cur + 1) % count,
            _ => 0,
        };
        target.selected = Some(new_sel);
    }

    if keys && ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) && count > 0 {
        let new_sel = match selected {
            Some(cur) if cur < count => (cur + count - 1) % count,
            _ => count - 1,
        };
        target.selected = Some(new_sel);
    }

    let mut action: Option<ConvertAction> = None;

    if keys
        && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        && let Some(sel) = target.selected
        && sel < count
    {
        action = Some(action_for_kind(engine, internal_items[sel].kind));
    }

    // 단축키: physical_key 사용 (한글 IME 활성 시에도 영문 매칭 보장).
    // 팝업 open 시 set_ime_allowed(false)로 IME가 비활성화되어 있다.
    ctx.input(|i| {
        for event in i.events.iter().filter(|_| keys) {
            if let egui::Event::Key {
                physical_key,
                pressed: true,
                modifiers,
                ..
            } = event
                && modifiers.is_none()
                && let Some(key) = physical_key
                && let Some(ch) = letter_key_to_char(key)
                && let Some(item) = internal_items.iter().find(|it| it.shortcut == Some(ch))
            {
                action = Some(action_for_kind(engine, item.kind));
            }
        }
    });

    let view_props = props_from_items(&internal_items, target.selected);
    let view_action = draw_convert_view(ui, &theme::theme(), &view_props);
    if let ConvertViewAction::Clicked { kind, .. } = view_action {
        action = Some(action_for_kind(engine, &kind));
    }

    action.map(ConvertResult::Action)
}

/// 항목 목록과 현재 kind로 화면 입력을 만든다.
fn props_from_items(items: &[ConvertItem], selected_index: Option<usize>) -> ConvertProps {
    let items = items
        .iter()
        .map(|it| ConvertItemView {
            kind: it.kind.to_string(),
            label: it.label.clone(),
            icon: it.icon,
            shortcut: it.shortcut,
        })
        .collect();
    ConvertProps {
        items,
        selected_index,
    }
}

/// Apply the convert action to the state.
pub fn apply_convert_action(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    surface_id: u32,
    action: ConvertAction,
) {
    match action {
        ConvertAction::Terminal => {
            state.dispatch_intent(
                crate::intent::Intent::ConvertSurface {
                    surface_id,
                    target: crate::intent::ConvertTarget::Terminal,
                }
                .from_user_menu("convert/terminal"),
            );
        }
        ConvertAction::RequiresInput(kind) => {
            // 파일 입력이 필요한 kind는 surface_id를 전달해 플러그인의 열기 팝업을 사용한다.
            state.enqueue_convert_input_popup(&engine.as_ref(), &kind, Some(surface_id));
        }
        ConvertAction::Kind(kind) => {
            state.dispatch_intent(
                crate::intent::Intent::ConvertSurface {
                    surface_id,
                    target: crate::intent::ConvertTarget::Kind {
                        cwd: None,
                        kind,
                        params: json!({}),
                    },
                }
                .from_user_menu("convert/kind"),
            );
        }
    }
}

#[derive(Clone)]
pub enum ConvertAction {
    /// host가 PTY를 생성해야 하는 터미널 전환.
    Terminal,
    /// convert_requires_input이 지정돼 파일 선택을 먼저 거치는 전환.
    RequiresInput(String),
    /// 추가 입력 없이 빈 params로 바로 전환할 kind.
    Kind(String),
}

fn action_for_kind(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    kind: &str,
) -> ConvertAction {
    if kind == "terminal" {
        return ConvertAction::Terminal;
    }
    if engine
        .surface_registry
        .get(kind)
        .is_some_and(|d| d.convert_requires_input)
    {
        return ConvertAction::RequiresInput(kind.to_string());
    }
    ConvertAction::Kind(kind.to_string())
}

fn letter_key_to_char(key: &egui::Key) -> Option<char> {
    use egui::Key;
    Some(match key {
        Key::A => 'A',
        Key::B => 'B',
        Key::C => 'C',
        Key::D => 'D',
        Key::E => 'E',
        Key::F => 'F',
        Key::G => 'G',
        Key::H => 'H',
        Key::I => 'I',
        Key::J => 'J',
        Key::K => 'K',
        Key::L => 'L',
        Key::M => 'M',
        Key::N => 'N',
        Key::O => 'O',
        Key::P => 'P',
        Key::Q => 'Q',
        Key::R => 'R',
        Key::S => 'S',
        Key::T => 'T',
        Key::U => 'U',
        Key::V => 'V',
        Key::W => 'W',
        Key::X => 'X',
        Key::Y => 'Y',
        Key::Z => 'Z',
        _ => return None,
    })
}

/// Get the current surface kind for a specific surface ID.
/// Split tab의 leaf surface도 정확히 식별한다.
fn current_surface_kind(
    _state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
) -> Option<&'static str> {
    for ws in &engine.workspaces() {
        for &pid in &ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pid) {
                for tab in &pane.tabs {
                    if !tab.contains_surface(surface_id) {
                        continue;
                    }
                    if let Some(leaf) = engine.find_surface_by_id(surface_id) {
                        return Some(leaf.kind());
                    }
                    return None;
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod props_tests {
    use super::*;

    fn mk(kind: &'static str, shortcut: Option<char>) -> ConvertItem {
        ConvertItem {
            kind,
            label: kind.to_string(),
            shortcut,
            icon: icons::FILE,
        }
    }

    #[test]
    fn keeps_every_listed_item_in_order() {
        let items = vec![mk("markdown", Some('M')), mk("image", Some('I'))];
        let props = props_from_items(&items, None);
        let kinds: Vec<&str> = props.items.iter().map(|it| it.kind.as_str()).collect();
        assert_eq!(kinds, ["markdown", "image"]);
        assert_eq!(props.items[1].label, "image");
    }

    #[test]
    fn shortcuts_reach_the_view_props() {
        let items = vec![mk("markdown", Some('M')), mk("mermaid", None)];
        let props = props_from_items(&items, None);
        assert_eq!(props.items[0].shortcut, Some('M'));
        assert_eq!(props.items[1].shortcut, None);
    }

    #[test]
    fn no_current_kind_keeps_every_item() {
        let items = vec![mk("terminal", Some('T')), mk("markdown", Some('M'))];
        let kinds: Vec<&str> = without_current(items, None)
            .iter()
            .map(|it| it.kind)
            .collect();
        assert_eq!(kinds, ["terminal", "markdown"]);
    }

    #[test]
    fn the_current_kind_is_left_out_and_the_rest_keep_their_order() {
        let items = vec![
            mk("markdown", Some('M')),
            mk("terminal", Some('T')),
            mk("image", Some('I')),
            mk("explorer", Some('E')),
        ];
        let kinds: Vec<&str> = without_current(items, Some("terminal"))
            .iter()
            .map(|it| it.kind)
            .collect();
        assert_eq!(kinds, ["markdown", "image", "explorer"]);
    }

    /// 첫 글자가 같은 두 kind 중 앞선 것이 현재 kind 이면 목록에 보이는 뒤의 kind 가 그 글자를 받는다.
    #[test]
    fn shortcuts_are_assigned_after_the_current_kind_is_left_out() {
        let items = vec![mk("markdown", None), mk("mermaid", None), mk("image", None)];
        let listed = listed_items(items, Some("markdown"));
        let got: Vec<(&str, Option<char>)> =
            listed.iter().map(|it| (it.kind, it.shortcut)).collect();
        assert_eq!(got, [("mermaid", Some('M')), ("image", Some('I'))]);
    }

    #[test]
    fn a_later_kind_with_a_taken_first_letter_has_no_shortcut() {
        let items = vec![
            mk("markdown", None),
            mk("mermaid", None),
            mk("9patch", None),
        ];
        let got: Vec<Option<char>> = listed_items(items, None)
            .iter()
            .map(|it| it.shortcut)
            .collect();
        assert_eq!(got, [Some('M'), None, None]);
    }

    #[test]
    fn empty_props_default() {
        let props = ConvertProps::default();
        assert!(props.items.is_empty());
        assert_eq!(props.selected_index, None);
    }

    #[test]
    fn preserves_selected_index() {
        let items = vec![mk("terminal", Some('T')), mk("markdown", Some('M'))];
        let props = props_from_items(&items, Some(1));
        assert_eq!(props.selected_index, Some(1));
    }
}

#[cfg(test)]
#[path = "convert_instance_tests.rs"]
mod instance_tests;
