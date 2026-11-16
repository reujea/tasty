//! 사이드바 도구 메뉴. 본체 항목과 활성·권한 허용된 플러그인의 도구 항목을 표시한다.
//! 플러그인 이벤트·팝업 요청은 큐에 넣고 surface 열기는 사용자 포커스 pane을 대상으로 한다.

use crate::adapters::ui::popup::{self, PopupAction};
use crate::i18n::t;
use crate::intent::{OpenPopupMode, UiIntent};
use crate::plugin::manifest::ToolAction;
use crate::plugin::tool_registry::ToolItem;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{MenuItemVariant, fit_menu_width, menu_item, menu_separator};

/// Built-in tool entries that are not contributed by any plugin.
/// `action` 으로 popup / 별도 winit 윈도우 오픈을 구분한다.
struct BuiltinTool {
    label_key: &'static str,
    action: BuiltinAction,
}

enum BuiltinAction {
    /// 일반 popup 열기.
    Popup(&'static str),
    /// 별도 winit 윈도우 열기. 현재 사용처는 PresetView 하나.
    Window(WindowKind),
    /// workspace 스코프 popup 열기. 스코프는 정의가 아니라 **여는 시점**의 활성
    /// workspace 로 정해지므로 `Popup` 과 분기가 다르다 — 이 창은 그 workspace
    /// 를 벗어나면 숨고 돌아오면 다시 뜬다.
    WorkspacePopup(&'static str),
    /// 파일 피커(docs/features/native-file-picker/index.md) — 단순 `Popup` 과 달리 여는 *전* 활성 workspace 의
    /// mirror 여부로 로컬/원격을 판별해 `state.dialogs.file_picker` 를 채워야 하므로
    /// 별도 분기.
    FilePicker,
}

#[derive(Debug, Clone, Copy)]
enum WindowKind {
    Preset,
}

const BUILTIN_TOOLS: &[BuiltinTool] = &[
    BuiltinTool {
        label_key: "command_palette.tools_menu_item",
        action: BuiltinAction::Popup(super::popup::command_palette::COMMAND_PALETTE_POPUP_ID),
    },
    BuiltinTool {
        label_key: "port_scanner.tools_menu_item",
        action: BuiltinAction::Popup(super::popup::port_scanner::PORT_SCANNER_POPUP_ID),
    },
    BuiltinTool {
        label_key: "remote_tool.tools_menu_item",
        action: BuiltinAction::Popup(super::popup::remote_tool::REMOTE_TOOL_POPUP_ID),
    },
    BuiltinTool {
        label_key: "preset.tools.menu_item",
        action: BuiltinAction::Window(WindowKind::Preset),
    },
    BuiltinTool {
        label_key: "tutorial.tools_menu_item",
        action: BuiltinAction::Popup(
            crate::adapters::ui::tutorial::topic_popup::TUTORIAL_TOPICS_POPUP_ID,
        ),
    },
    BuiltinTool {
        label_key: "dag_list.tools_menu_item",
        action: BuiltinAction::WorkspacePopup(super::popup::dag_list::DAG_LIST_POPUP_ID),
    },
    BuiltinTool {
        label_key: "filepicker.tools_menu_item",
        action: BuiltinAction::FilePicker,
    },
];

pub fn draw_tools_menu(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
) -> PopupAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return PopupAction::Close;
    }

    let th = theme::theme();
    state.dialogs.tools_menu_width = Some(measure_inner_width(ui.ctx(), state));
    // 디자인처럼 행 사이 간격 없이 쌓는다.
    ui.spacing_mut().item_spacing.y = 0.0;

    let mut open_popup: Option<&'static str> = None;
    let mut open_workspace_popup: Option<&'static str> = None;
    let mut open_window: Option<WindowKind> = None;
    let mut open_file_picker = false;
    for entry in BUILTIN_TOOLS {
        if row(ui, &th, t(entry.label_key)).clicked() {
            match entry.action {
                BuiltinAction::Popup(id) => open_popup = Some(id),
                BuiltinAction::WorkspacePopup(id) => open_workspace_popup = Some(id),
                BuiltinAction::Window(k) => open_window = Some(k),
                BuiltinAction::FilePicker => open_file_picker = true,
            }
        }
    }
    if open_file_picker {
        crate::adapters::ui::file_open_tool::open(state, engine);
        return PopupAction::Close;
    }
    if let Some(popup_id) = open_workspace_popup {
        state.dispatch_intent(
            UiIntent::OpenPopup {
                id: popup_id,
                mode: OpenPopupMode::CenteredFocused(popup::PopupScope::Workspace(
                    state.active_workspace(engine).id,
                )),
            }
            .from_user_menu("tools_menu"),
        );
        return PopupAction::Close;
    }
    if let Some(popup_id) = open_popup {
        // 본체 팝업은 가운데 열고 포커스를 준다.
        state.dispatch_intent(
            UiIntent::OpenPopup {
                id: popup_id,
                mode: OpenPopupMode::CenteredFocused(crate::model::popup_kind::PopupScope::Window),
            }
            .from_user_menu("tools_menu"),
        );
        return PopupAction::Close;
    }
    if let Some(kind) = open_window {
        match kind {
            WindowKind::Preset => state.dialogs.pending_open_preset_window = true,
        }
        return PopupAction::Close;
    }

    let items = state.tool_registry.visible_items();
    if !items.is_empty() && !BUILTIN_TOOLS.is_empty() {
        menu_separator(ui, &th);
    }

    let mut clicked: Option<ToolItem> = None;
    for item in &items {
        let label = crate::adapters::ui::label_or_raw_key(&item.label_i18n_key);
        if row(ui, &th, &label).clicked() {
            clicked = Some(item.clone());
        }
    }

    if let Some(item) = clicked {
        invoke_tool(state, engine, &item);
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 아이콘·단축키 없는 공용 MenuItem 행. 넘친 라벨은 공용 행이 끝을 줄인다.
fn row(ui: &mut egui::Ui, th: &theme::Theme, label: &str) -> egui::Response {
    menu_item(
        ui,
        th,
        None,
        label,
        None,
        MenuItemVariant::Normal,
        false,
        true,
    )
}

/// 메뉴 셸 안쪽 폭. 내장·플러그인 라벨 중 가장 넓은 것에 맞추고
/// `tools-menu-min-width`..`tools-menu-max-width`(테두리 포함 폭)로 제한한 뒤 테두리를 뺀다.
fn measure_inner_width(ctx: &egui::Context, state: &MainViewState) -> LogicalPx {
    let th = theme::theme();
    let plugin_labels: Vec<String> = state
        .tool_registry
        .visible_items()
        .iter()
        .map(|i| crate::adapters::ui::label_or_raw_key(&i.label_i18n_key))
        .collect();
    let labels = BUILTIN_TOOLS
        .iter()
        .map(|e| t(e.label_key))
        .chain(plugin_labels.iter().map(String::as_str));
    let outer = fit_menu_width(
        ctx,
        &th,
        labels,
        th.tools_menu_min_width().value(),
        th.tools_menu_max_width().value(),
    );
    LogicalPx(outer - th.border_width.value() * 2.0)
}

/// 하한 폭의 셸 안쪽 폭. 아직 재지 않았을 때 쓴다.
fn min_inner_width() -> LogicalPx {
    let th = theme::theme();
    LogicalPx(th.tools_menu_min_width().value() - th.border_width.value() * 2.0)
}

/// 열기 전에 현재 라벨로 폭을 잰다. 첫 프레임부터 내용 폭으로 연다.
pub fn measure_on_open(ctx: &egui::Context, state: &mut MainViewState) {
    state.dialogs.tools_menu_width = Some(measure_inner_width(ctx, state));
}

/// 사용자 클릭의 플러그인 도구를 실행한다. debug.tool.invoke는 대상 ID를 받는 별도 경로다.
pub fn invoke_tool(state: &mut MainViewState, engine: &EngineRead<'_>, item: &ToolItem) {
    match &item.action {
        ToolAction::Event { event_key } => {
            let payload = serde_json::json!({ "tool_id": item.key });
            state.pending_tool_events.push((event_key.clone(), payload));
        }
        ToolAction::OpenSurface { surface_kind } => {
            state.dispatch_intent(
                crate::intent::Intent::NewTab {
                    kind: Some(surface_kind.clone()),
                    params: serde_json::Value::Null,
                }
                .from_user_menu("tools_menu/open_surface"),
            );
        }
        ToolAction::OpenPopup { popup_id } => {
            // plugin/id를 나누어 팝업 요청을 큐에 넣는다. 사용자 포커스 surface의
            // cwd·mirror 정보를 함께 전달하며 local_surface_id는 원격 조회 대상에 사용한다.
            if let Some((plugin_id, local_id)) = popup_id.split_once('/') {
                let origin = state.focused_surface_id(engine);
                let context = state.popup_surface_context(&engine.as_ref(), origin);
                state
                    .pending_popup_opens
                    .push(crate::state::PendingPopupOpen {
                        plugin_id: plugin_id.to_string(),
                        popup_id: local_id.to_string(),
                        context,
                        target_surface: origin,
                    });
            } else {
                tracing::warn!(
                    "invoke_tool: open_popup '{}' is not in '<plugin_id>/<id>' form",
                    popup_id
                );
            }
        }
    }
}

/// 본체·플러그인 항목과 구분선 여백을 포함한 메뉴 크기. 행은 간격 없이 쌓고
/// 구분선은 위아래 `space-xs` 를 차지한다. 타이틀바는 없는 팝업이다.
fn tools_menu_size_for(
    width: LogicalPx,
    builtin_count: usize,
    plugin_count: usize,
    row_height: LogicalPx,
    separator_gap: LogicalPx,
) -> egui::Vec2 {
    let total = (builtin_count + plugin_count).max(1);
    let mut content_h = row_height.scaled(total as f32);
    if builtin_count > 0 && plugin_count > 0 {
        content_h += separator_gap.scaled(2.0);
    }
    // 행은 간격 없이 붙으므로 위아래 안쪽 여백이 같은 값으로 남도록 여분을 더하지 않는다.
    egui::vec2(
        width.value(),
        (popup::content_margin().scaled(2.0) + content_h).value(),
    )
}

/// PopupDef.sizer — 매 프레임 plugin tool registry 의 실제 항목 수로 height 재계산.
pub fn tools_menu_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let th = theme::theme();
    let plugin_count = state.tool_registry.visible_items().len();
    tools_menu_size_for(
        state
            .dialogs
            .tools_menu_width
            .unwrap_or_else(min_inner_width),
        BUILTIN_TOOLS.len(),
        plugin_count,
        th.menu_item_height(),
        th.spacing_xs,
    )
}

/// 등록 시에는 본체 항목으로 계산하고 렌더링 때 sizer로 갱신한다.
pub fn tools_menu_default_size() -> egui::Vec2 {
    let th = theme::theme();
    tools_menu_size_for(
        min_inner_width(),
        BUILTIN_TOOLS.len(),
        0,
        th.menu_item_height(),
        th.spacing_xs,
    )
}

/// 메뉴 위치 계산에 사용할 현재 본체·플러그인 항목의 크기.
pub fn tools_menu_current_size(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    tools_menu_sizer(state, engine)
}

#[cfg(test)]
mod size_tests {
    use super::*;

    const W: LogicalPx = LogicalPx(158.0);
    const ROW: LogicalPx = LogicalPx(28.0);
    const GAP: LogicalPx = LogicalPx(4.0);

    #[test]
    fn fits_builtin_only_rows_flush() {
        let size = tools_menu_size_for(W, 4, 0, ROW, GAP);
        let needed = popup::content_margin().scaled(2.0) + ROW.scaled(4.0);
        assert_eq!(
            LogicalPx(size.y),
            needed,
            "rows flush with the same inner margin above and below"
        );
        assert_eq!(size.x, W.value());
    }

    #[test]
    fn fits_builtin_plus_plugin_with_separator() {
        let size = tools_menu_size_for(W, 4, 3, ROW, GAP);
        let needed = popup::content_margin().scaled(2.0) + ROW.scaled(7.0) + GAP.scaled(2.0); // menu_separator = 2·spacing_xs
        assert_eq!(LogicalPx(size.y), needed);
    }

    #[test]
    fn fits_plugin_only_no_separator() {
        let size = tools_menu_size_for(W, 0, 5, ROW, GAP);
        let needed = popup::content_margin().scaled(2.0) + ROW.scaled(5.0);
        assert_eq!(LogicalPx(size.y), needed);
    }

    #[test]
    fn empty_does_not_underflow() {
        let size = tools_menu_size_for(W, 0, 0, ROW, GAP);
        assert!(LogicalPx(size.y) >= popup::content_margin().scaled(2.0) + ROW);
    }

    #[test]
    fn scales_with_ui_scale_1_2() {
        let row = LogicalPx(28.0 * 1.2);
        let size = tools_menu_size_for(W, 4, 0, row, LogicalPx(4.0 * 1.2));
        let needed = popup::content_margin().scaled(2.0) + row.scaled(4.0);
        assert!(LogicalPx(size.y) >= needed);
    }
}
