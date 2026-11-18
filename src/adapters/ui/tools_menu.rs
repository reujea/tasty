//! 사이드바 도구 메뉴. 본체 항목과 활성 플러그인의 도구 항목을 표시한다.
//! 플러그인 이벤트·팝업 요청은 큐에 넣고 surface 열기는 입력 때 정한 pane을 대상으로 한다.

use crate::adapters::ui::popup::{self, PopupAction, PopupPayload};
use crate::i18n::t;
use crate::intent::UiIntent;
use crate::plugin::manifest::ToolAction;
use crate::plugin::tool_registry::ToolItem;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::state::attach_pending::PendingId;
use crate::state::attach_pending::deferred::{
    CommandSurfaceTarget, DeferredCommand, InvokedNotice,
};
use crate::state::command_target::CommandTarget;
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
    /// mirror 여부로 로컬/원격을 판별해 파일 피커 인스턴스 데이터를 채워야 하므로
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
    cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> PopupAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return PopupAction::Close;
    }

    let th = theme::theme();
    *cx.payload = PopupPayload::ToolsMenu {
        inner_width: measure_inner_width(ui.ctx(), state),
    };
    // 디자인처럼 행 사이 간격 없이 쌓는다.
    ui.spacing_mut().item_spacing.y = 0.0;

    let mut open_popup: Option<&'static str> = None;
    let mut open_workspace_popup: Option<&'static str> = None;
    let mut open_window: Option<WindowKind> = None;
    let mut open_file_picker = false;
    for entry in BUILTIN_TOOLS {
        if row(ui, &th, t(entry.label_key), true).clicked() {
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
            UiIntent::OpenPopup(
                crate::intent::OpenRequest::new(
                    popup_id,
                    popup::PopupScope::Workspace(state.active_workspace(engine).id),
                )
                .centered()
                .focused(),
            )
            .from_user_menu("tools_menu"),
        );
        return PopupAction::Close;
    }
    if let Some(popup_id) = open_popup {
        // 본체 팝업은 가운데 열고 포커스를 준다.
        state.dispatch_intent(
            UiIntent::OpenPopup(
                crate::intent::OpenRequest::new(
                    popup_id,
                    crate::model::popup_kind::PopupScope::Window,
                )
                .centered()
                .focused(),
            )
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

    let (clicked, _) = draw_plugin_rows(ui, &th, state, engine, &items);

    if let Some(item) = clicked {
        let target = state.command_target(engine);
        if let Err(blocked) = run_tool(state, engine, &item, &target, None, |_| {}) {
            blocked.log_not_run("tool", &item.key);
        }
        return PopupAction::Close;
    }
    PopupAction::None
}

/// 플러그인 항목 행을 그린다. 실행할 수 없는 항목도 그리되 비활성 행으로 그린다.
/// 누른 실행 가능한 항목과, 그린 행마다 라벨과 응답을 돌려준다.
fn draw_plugin_rows(
    ui: &mut egui::Ui,
    th: &theme::Theme,
    state: &MainViewState,
    engine: &EngineRead<'_>,
    items: &[ToolItem],
) -> (Option<ToolItem>, Vec<(String, egui::Response)>) {
    let mut clicked: Option<ToolItem> = None;
    let mut drawn = Vec::with_capacity(items.len());
    for item in items {
        let label = crate::adapters::ui::label_or_raw_key(&item.label_i18n_key);
        let enabled = !tool_action_blocked(state, engine, &item.action);
        let response = row(ui, th, &label, enabled);
        if response.clicked() && enabled {
            clicked = Some(item.clone());
        }
        drawn.push((label, response));
    }
    (clicked, drawn)
}

/// 아이콘·단축키 없는 공용 MenuItem 행. 넘친 라벨은 공용 행이 끝을 줄인다.
/// 실행할 수 없는 항목은 숨기지 않고 비활성 상태로 그린다.
fn row(ui: &mut egui::Ui, th: &theme::Theme, label: &str, enabled: bool) -> egui::Response {
    menu_item(
        ui,
        th,
        None,
        label,
        None,
        MenuItemVariant::Normal,
        false,
        enabled,
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

/// 열기 전에 현재 라벨로 폭을 잰 인스턴스 데이터. 첫 프레임부터 내용 폭으로 연다.
pub fn measure_on_open(ctx: &egui::Context, state: &MainViewState) -> PopupPayload {
    PopupPayload::ToolsMenu {
        inner_width: measure_inner_width(ctx, state),
    }
}

/// 도구·명령 동작을 지금 실행할 수 없는 이유.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolBlocked {
    /// surface 범위로 선언한 팝업인데 대상 surface 도 생성 대기 초안도 없다.
    NoSurfaceTarget,
    /// 대상이던 생성 대기 초안이 이미 끝났거나 사라졌다.
    DraftGone,
    /// surface 를 열 pane 이 입력 때 정해지지 않았거나 그 사이 사라졌다.
    PaneGone,
}

impl ToolBlocked {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NoSurfaceTarget => "the popup needs a surface target and no surface is focused",
            Self::DraftGone => "the pending draft it targets is no longer waiting",
            Self::PaneGone => "the pane it opens a surface in is gone",
        }
    }

    /// 입력 뒤 대상이 사라진 정상 경합인지. 초안이 끝났거나 열 pane 이 사라진 경우다.
    fn is_race(self) -> bool {
        matches!(self, Self::DraftGone | Self::PaneGone)
    }

    /// 실행하지 않은 명령 하나를 한 줄로 남긴다. 입력 뒤 대상이 사라진 경합은 info, 그 밖은 warn 이다.
    pub(crate) fn log_not_run(self, what: &str, key: &str) {
        if self.is_race() {
            tracing::info!("{what} '{key}' not run: {}", self.message());
        } else {
            tracing::warn!("{what} '{key}' not run: {}", self.message());
        }
    }
}

/// 도구·명령을 입력 시점 대상으로 실행했는지, 생성 대기 초안에 걸어 두었는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolRun {
    Ran,
    Deferred(PendingId),
}

/// 이 동작을 지금 실행할 수 없는지. 도구 메뉴 비활성 표시와 모든 실행 경로가 같은 판정을 쓴다.
pub(crate) fn tool_action_blocked(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    action: &ToolAction,
) -> bool {
    tool_action_block(state, action, state.command_surface_target(engine)).is_some()
}

fn tool_action_block(
    state: &MainViewState,
    action: &ToolAction,
    target: Option<CommandSurfaceTarget>,
) -> Option<ToolBlocked> {
    (state.tool_registry.needs_surface(action) && target.is_none())
        .then_some(ToolBlocked::NoSurfaceTarget)
}

/// 입력 시점에 정한 `target`으로 도구·명령 동작을 실행한다. 대상이 생성 대기 초안이면 팝업 동작은
/// 걸어 두고 생성된 surface에서 실행한다. surface 열기의 pane 과 cwd 원본은 `target` 에 입력 때 정해진
/// 값이고 여기서 다시 고르지 않는다. `command.invoked`(`invoked`)는 실제로 실행할 때만
/// `notify`로 알린다. 실행할 수 없으면 알리지도 실행하지도 않고 이유를 돌려준다.
pub(crate) fn run_tool(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    item: &ToolItem,
    target: &CommandTarget,
    invoked: Option<InvokedNotice>,
    notify: impl FnOnce(&InvokedNotice),
) -> Result<ToolRun, ToolBlocked> {
    if let Some(blocked) = tool_action_block(state, &item.action, target.surface) {
        return Err(blocked);
    }
    let surface = match target.surface {
        Some(CommandSurfaceTarget::Pending(pending)) => {
            match draft_run(state, engine, &item.action, pending, target)? {
                DraftRun::Hold => {
                    state.attach_pending.deferred.push(
                        pending,
                        DeferredCommand::Tool {
                            item: item.clone(),
                            invoked,
                        },
                    );
                    return Ok(ToolRun::Deferred(pending));
                }
                DraftRun::OpenAt(at) => {
                    if let Some(blocked) = open_pane_gone(engine, at) {
                        return Err(blocked);
                    }
                    if let Some(invoked) = &invoked {
                        notify(invoked);
                    }
                    if let ToolAction::OpenSurface { surface_kind } = &item.action {
                        open_surface_at(state, surface_kind, at);
                    }
                    return Ok(ToolRun::Ran);
                }
                DraftRun::NoSurface => None,
            }
        }
        Some(CommandSurfaceTarget::Surface(sid)) => Some(sid),
        None => None,
    };
    let at = target.structure_target();
    if matches!(item.action, ToolAction::OpenSurface { .. })
        && let Some(blocked) = open_pane_gone(engine, at)
    {
        return Err(blocked);
    }
    if let Some(invoked) = &invoked {
        notify(invoked);
    }
    invoke_tool_on(state, engine, item, surface, at);
    Ok(ToolRun::Ran)
}

/// surface 열기를 입력 때 정한 pane 에 할 수 없는지. pane 이 정해지지 않았거나 그 사이 사라졌으면 다른 pane
/// 으로 바꾸지 않고, 실행하지도 알리지도 않도록 막는다. 로그는 호출자가 `ToolBlocked::log_not_run` 으로 남긴다.
fn open_pane_gone(
    engine: &EngineRead<'_>,
    at: crate::intent::StructureTarget,
) -> Option<ToolBlocked> {
    match at.pane {
        Some(pane) if engine.find_pane_by_id(pane).is_some() => None,
        _ => Some(ToolBlocked::PaneGone),
    }
}

/// 생성 대기 초안이 대상일 때 동작을 어떻게 할지.
enum DraftRun {
    /// 걸어 두었다가 생성된 surface 에서 실행한다.
    Hold,
    /// 초안이 기댄 기존 pane 에 surface 를 지금 연다.
    OpenAt(crate::intent::StructureTarget),
    /// 대상 surface 를 쓰지 않는 동작이라 지금 실행한다.
    NoSurface,
}

/// 팝업은 선언 범위와 관계없이 대상 surface 의 문맥(cwd·mirror)을 받으므로 걸어 둔다. surface 열기는 pane
/// 명령이라 구조 명령과 같이 판정한다. 새 탭·surface 분할 초안이면 기댄 기존 pane 이 대상이라 지금 열고 pane
/// 과 cwd 원본은 입력 때 `target` 에 정한 값(초안 뒤 탭에서 그때 골라져 있던 surface)이다. 새 pane 초안이면
/// 만들 pane 이 대상이라 걸어 둔다. 지금 판정하는 것은 초안이 아직 기다리는지뿐이고, 끝났으면 실행하지 않는다.
/// 대상 surface 를 쓰지 않는 이벤트는 지금 실행한다.
fn draft_run(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    action: &ToolAction,
    pending: PendingId,
    target: &CommandTarget,
) -> Result<DraftRun, ToolBlocked> {
    match action {
        ToolAction::OpenPopup { .. } => Ok(DraftRun::Hold),
        ToolAction::Event { .. } => Ok(DraftRun::NoSurface),
        ToolAction::OpenSurface { .. } => {
            let Some(entry) = state.attach_pending.waiting_entry(engine, pending) else {
                return Err(ToolBlocked::DraftGone);
            };
            if matches!(
                entry.creation,
                Some(crate::state::attach_pending::Creation::Pane(_))
            ) {
                return Ok(DraftRun::Hold);
            }
            Ok(DraftRun::OpenAt(target.structure_target()))
        }
    }
}

/// 생성 대기 초안 `pending` 이 대상일 때 걸어 둘 동작인지.
pub(crate) fn defers(
    state: &MainViewState,
    engine: &EngineRead<'_>,
    action: &ToolAction,
    pending: PendingId,
    target: &CommandTarget,
) -> bool {
    matches!(
        draft_run(state, engine, action, pending, target),
        Ok(DraftRun::Hold)
    )
}

/// plugin 명령을 도구 항목으로 만든다. 명령 팔레트와 명령 단축키가 같은 모양을 쓴다.
pub(crate) fn command_item(plugin_id: &str, command_id: &str, action: ToolAction) -> ToolItem {
    ToolItem {
        source: crate::plugin::tool_registry::ToolSource::Plugin {
            plugin_id: plugin_id.to_string(),
            tool_id: command_id.to_string(),
        },
        key: format!("{plugin_id}/{command_id}"),
        label_i18n_key: String::new(),
        icon: None,
        action,
        order_hint: 0,
    }
}

/// 팔레트에서 고른 plugin 명령의 대상이 생성 대기 초안이고 걸어 둘 동작이면 고른 지금 초안에 건다.
/// 그래야 App 이 큐를 처리하기 전에 초안이 정산돼도 명령을 잃지 않고, 다른 진입점과 입력 순서가 맞는다.
/// 걸어 두었으면 true, 아니면 false 를 돌려주고 호출자가 App 큐로 넘긴다.
pub(crate) fn defer_palette_command(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    cmd: &crate::state::command_palette::PaletteCommand,
    target: &CommandTarget,
) -> bool {
    let (
        Some(CommandSurfaceTarget::Pending(pending)),
        crate::state::command_palette::PaletteCommand::Plugin {
            plugin_id,
            command_id,
            ..
        },
    ) = (target.surface, cmd)
    else {
        return false;
    };
    let Some(action) = state
        .palette_plugin_commands
        .iter()
        .find(|e| e.plugin_id == *plugin_id && e.command_id == *command_id)
        .and_then(|e| e.action.clone())
        .filter(|action| defers(state, engine, action, pending, target))
    else {
        return false;
    };
    let item = command_item(plugin_id, command_id, action);
    let invoked = InvokedNotice {
        plugin_id: plugin_id.clone(),
        command_id: command_id.clone(),
        surface_id: None,
    };
    matches!(
        run_tool(state, engine, &item, target, Some(invoked), |_| {}),
        Ok(ToolRun::Deferred(_))
    )
}

/// `surface_kind` 새 탭을 `at` 에 연다.
fn open_surface_at(
    state: &mut MainViewState,
    surface_kind: &str,
    at: crate::intent::StructureTarget,
) {
    state.dispatch_intent(
        crate::intent::Intent::NewTab {
            kind: Some(surface_kind.to_string()),
            params: serde_json::Value::Null,
            target: at,
        }
        .from_user_menu("tools_menu/open_surface"),
    );
}

/// 플러그인 도구·명령 동작을 `surface`를 대상으로 실행한다. surface 열기는 `at` 의 pane 에 열고 cwd 원본도
/// `at` 의 것이다. 대상은 호출자가 입력 시점에 정한 값이다.
pub(crate) fn invoke_tool_on(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    item: &ToolItem,
    surface: Option<u32>,
    at: crate::intent::StructureTarget,
) {
    match &item.action {
        ToolAction::Event { event_key } => {
            let payload = serde_json::json!({ "tool_id": item.key });
            state.pending_tool_events.push((event_key.clone(), payload));
        }
        ToolAction::OpenSurface { surface_kind } => open_surface_at(state, surface_kind, at),
        ToolAction::OpenPopup { popup_id } => {
            // plugin/id를 나누어 팝업 요청을 큐에 넣는다. 대상 surface의 cwd·mirror 정보를
            // 함께 전달하며 local_surface_id는 원격 조회 대상에 사용한다.
            if let Some((plugin_id, local_id)) = popup_id.split_once('/') {
                let context = state.popup_surface_context(&engine.as_ref(), surface);
                state
                    .pending_popup_opens
                    .push(crate::state::PendingPopupOpen {
                        plugin_id: plugin_id.to_string(),
                        popup_id: local_id.to_string(),
                        context,
                        target_surface: surface,
                    });
            } else {
                tracing::warn!(
                    "invoke_tool_on: open_popup '{}' is not in '<plugin_id>/<id>' form",
                    popup_id
                );
            }
        }
    }
}

/// 생성된 surface가 정해진 걸어 둔 명령(도구·변환 입력·내장 액션)을 그 surface에서 접수 순서대로 실행한다.
/// 실행 직전에 surface가 사라졌으면 버린다. 실행한 명령의 `command.invoked` 알림을 돌려준다.
pub(crate) fn run_deferred_tools(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
) -> Vec<InvokedNotice> {
    let mut notices = Vec::new();
    for (local, command) in state.attach_pending.deferred.take_ready() {
        if !engine.has_surface(local) {
            tracing::info!(
                "deferred command '{}' dropped: its surface {local} is gone",
                command.key()
            );
            continue;
        }
        // 앞의 창 액션이 아직 실행되지 않았으면 접수 순서를 지키려 그 뒤로 미룬다.
        if state.attach_pending.deferred.waits_for_view(local) {
            state.attach_pending.deferred.put_back(local, command);
            continue;
        }
        match command {
            DeferredCommand::Tool { item, invoked } => {
                notices.extend(invoked);
                // 걸어 둔 명령의 대상은 생성된 surface 이고 그 pane 은 생성 결과로 정해진다.
                let at = crate::intent::StructureTarget {
                    pane: engine.find_pane_for_surface(local),
                    cwd_source: Some(local),
                };
                invoke_tool_on(state, engine, &item, Some(local), at);
            }
            DeferredCommand::ConvertInput { kind } => {
                state.open_deferred_convert_input(engine, &kind, local);
            }
            DeferredCommand::Action { id, kind, trigger } => {
                crate::shortcuts::surface_actions::run_held_action(
                    state, engine, id, &kind, trigger, local,
                );
            }
            DeferredCommand::Zoom { change, kind } => {
                crate::shortcuts::run_held_zoom(state, engine, change, &kind, local);
            }
            DeferredCommand::Structure { command, source } => {
                crate::shortcuts::structure::run_held(state, engine, command, source, local);
            }
        }
    }
    notices
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
    payload: &PopupPayload,
) -> egui::Vec2 {
    let th = theme::theme();
    let plugin_count = state.tool_registry.visible_items().len();
    let inner_width = match payload {
        PopupPayload::ToolsMenu { inner_width } => *inner_width,
        _ => min_inner_width(),
    };
    tools_menu_size_for(
        inner_width,
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
    payload: &PopupPayload,
) -> egui::Vec2 {
    tools_menu_sizer(state, engine, payload)
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

#[cfg(test)]
mod block_tests {
    use super::*;
    use std::sync::Arc;

    const SURFACE_POPUP: &str = "com.example.viewer/confirm";
    const WINDOW_POPUP: &str = "com.example.viewer/list";

    fn open(popup_id: &str) -> ToolAction {
        ToolAction::OpenPopup {
            popup_id: popup_id.into(),
        }
    }

    fn item(popup_id: &str) -> ToolItem {
        ToolItem {
            source: crate::plugin::tool_registry::ToolSource::Plugin {
                plugin_id: "com.example.viewer".into(),
                tool_id: "t".into(),
            },
            key: popup_id.into(),
            label_i18n_key: String::new(),
            icon: None,
            action: open(popup_id),
            order_hint: 0,
        }
    }

    fn notice(command_id: &str) -> InvokedNotice {
        InvokedNotice {
            plugin_id: "com.example.viewer".into(),
            command_id: command_id.into(),
            surface_id: None,
        }
    }

    fn begin_tab_draft(
        state: &mut MainViewState,
        read: &EngineRead<'_>,
    ) -> (
        Arc<tasty_remote::pending_request::PendingRequest>,
        Arc<tasty_remote::connection::ConnectionSender>,
        Arc<()>,
    ) {
        let anchor = state.focused_surface_id(read).unwrap();
        let (sender, _queue) = tasty_remote::connection::channel();
        let view = Arc::new(());
        let receipt = state
            .attach_pending
            .begin(
                read,
                &state.navigation,
                &crate::ipc::stream::StructuralOp::NewTab {
                    anchor_surface_id: anchor,
                    surface_kind: "terminal".into(),
                    params: serde_json::json!({}),
                    inherit_cwd: None,
                },
                sender.epoch(),
                Arc::downgrade(&view),
                std::time::Instant::now(),
            )
            .unwrap();
        (receipt, sender, view)
    }

    /// 새 탭 생성 초안이 선택돼 포커스 surface 는 없고 초안이 명령 대상인 장면에서 판정한다.
    fn with_pending_draft(check: impl FnOnce(&mut MainViewState, &EngineRead<'_>, PendingId)) {
        let (mut state, mut session) = crate::state::tests::test_mirror_state();
        state
            .tool_registry
            .set_surface_popups([SURFACE_POPUP.to_string()].into_iter().collect());
        let engine = session.borrow_mut();
        let read = engine.read();
        let _draft = begin_tab_draft(&mut state, &read);
        assert_eq!(state.focused_surface_id(&read), None);
        let pending = state.attach_pending.entries[0].id;
        assert_eq!(
            state.command_surface_target(&read),
            Some(CommandSurfaceTarget::Pending(pending))
        );
        check(&mut state, &read, pending);
    }

    /// 생성 대기 초안도 명령 대상이므로 surface 팝업 도구를 막지 않는다. 대상이 아예 없을 때만 막는다.
    #[test]
    fn a_pending_draft_is_a_target_and_only_no_target_blocks_a_surface_tool() {
        with_pending_draft(|state, read, _| {
            assert!(!tool_action_blocked(state, read, &open(SURFACE_POPUP)));
            assert!(!tool_action_blocked(state, read, &open(WINDOW_POPUP)));
            assert_eq!(
                tool_action_block(state, &open(SURFACE_POPUP), None),
                Some(ToolBlocked::NoSurfaceTarget)
            );
            assert_eq!(tool_action_block(state, &open(WINDOW_POPUP), None), None);
        });
    }

    /// 초안이 대상이면 surface 팝업 도구 행도 누를 수 있다.
    #[test]
    fn a_surface_tool_row_is_enabled_while_a_draft_is_pending() {
        with_pending_draft(|state, read, _| {
            let mut a = item(SURFACE_POPUP);
            a.label_i18n_key = "tool.surface".into();
            let mut b = item(WINDOW_POPUP);
            b.label_i18n_key = "tool.window".into();
            let ctx = egui::Context::default();
            let th = theme::theme();
            let mut drawn = Vec::new();
            let _output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    drawn = draw_plugin_rows(ui, &th, state, read, &[a.clone(), b.clone()]).1;
                });
            });
            let senses: Vec<(String, bool)> = drawn
                .iter()
                .map(|(label, r)| (label.clone(), r.sense.senses_click()))
                .collect();
            assert_eq!(
                senses,
                vec![
                    ("tool.surface".to_string(), true),
                    ("tool.window".to_string(), true)
                ]
            );
        });
    }

    /// 초안 대상 팝업 도구는 걸어 두기만 하고 알림도 팝업 요청도 만들지 않는다.
    /// 대상이 필요 없는 이벤트 도구는 바로 실행한다.
    #[test]
    fn a_popup_tool_on_a_pending_draft_is_deferred_without_a_notice() {
        with_pending_draft(|state, read, pending| {
            let target = state.command_target(read);
            let mut notified = 0;
            assert_eq!(
                run_tool(
                    state,
                    read,
                    &item(SURFACE_POPUP),
                    &target,
                    Some(notice("confirm")),
                    |_| notified += 1
                ),
                Ok(ToolRun::Deferred(pending))
            );
            assert_eq!(notified, 0);
            assert!(state.pending_popup_opens.is_empty());
            assert_eq!(state.attach_pending.deferred.waiting_for(pending), 1);
            let mut event = item(SURFACE_POPUP);
            event.action = ToolAction::Event {
                event_key: "com.example.viewer.go".into(),
            };
            assert_eq!(
                run_tool(state, read, &event, &target, None, |_| {}),
                Ok(ToolRun::Ran)
            );
            assert_eq!(state.pending_tool_events.len(), 1);
        });
    }

    /// 걸어 둔 명령은 사용자가 대기 중 다른 surface 를 골라도 생성된 surface 에서 접수 순서대로 실행되고,
    /// 실행할 때 command.invoked 를 한 번 알린다.
    #[test]
    fn deferred_tools_run_on_the_created_surface_in_input_order_after_focus_moves() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        let engine = session.borrow_mut();
        let read = engine.read();
        let (receipt, sender, view) = begin_tab_draft(&mut state, &read);
        let target = state.command_target(&read);
        assert!(matches!(
            target.surface,
            Some(CommandSurfaceTarget::Pending(_))
        ));
        for (popup, invoked) in [
            (SURFACE_POPUP, Some(notice("confirm"))),
            (WINDOW_POPUP, None),
        ] {
            run_tool(&mut state, &read, &item(popup), &target, invoked, |_| {}).unwrap();
        }
        assert!(state.focus_surface_by_id(&read, 101));
        receipt.bind(0);
        state
            .attach_pending
            .result(&sender.epoch(), 0, Some(99), true);
        state.attach_pending.applied(
            &read,
            &state.navigation,
            &sender.epoch(),
            Some(&Arc::downgrade(&view)),
            &[(99, 102)].into_iter().collect(),
            &[99],
        );
        assert_eq!(state.focused_surface_id(&read), Some(101));
        let notices = run_deferred_tools(&mut state, &read);
        assert_eq!(notices, vec![notice("confirm")]);
        let opened: Vec<_> = state
            .pending_popup_opens
            .iter()
            .map(|o| {
                (
                    o.popup_id.as_str(),
                    o.target_surface,
                    o.context["origin_surface_id"].as_u64(),
                )
            })
            .collect();
        assert_eq!(
            opened,
            vec![
                ("confirm", Some(102), Some(102)),
                ("list", Some(102), Some(102))
            ]
        );
        assert!(run_deferred_tools(&mut state, &read).is_empty());
    }

    /// 실행하기 전에 생성된 surface 가 사라졌으면 실행하지 않고 버린다.
    #[test]
    fn a_deferred_tool_whose_surface_is_gone_is_dropped() {
        with_pending_draft(|state, read, pending| {
            state.attach_pending.deferred.push(
                pending,
                crate::state::attach_pending::deferred::test_popup_command(SURFACE_POPUP),
            );
            state.attach_pending.entries[0].receipt.bind(0);
            let epoch = state.attach_pending.entries[0].epoch.clone();
            state.attach_pending.result(&epoch, 0, Some(99), true);
            let view = state.attach_pending.entries[0].view.clone();
            state.attach_pending.applied(
                read,
                &state.navigation,
                &epoch,
                Some(&view),
                &[(99, 9_999)].into_iter().collect(),
                &[99],
            );
            assert!(run_deferred_tools(state, read).is_empty());
            assert!(state.pending_popup_opens.is_empty());
        });
    }

    /// 바로 실행하는 도구는 실행 시점의 포커스가 아니라 입력 시점에 정한 surface 를 대상으로 연다.
    #[test]
    fn an_immediate_tool_runs_on_the_input_time_surface_not_the_current_focus() {
        let (mut state, mut session) = crate::state::tests::two_pane_fixture();
        let engine = session.borrow_mut();
        let read = engine.read();
        let target = state.command_target(&read);
        assert_eq!(target.surface, Some(CommandSurfaceTarget::Surface(201)));
        assert!(state.focus_surface_by_id(&read, 101));
        assert_eq!(
            run_tool(
                &mut state,
                &read,
                &item(SURFACE_POPUP),
                &target,
                None,
                |_| {}
            ),
            Ok(ToolRun::Ran)
        );
        assert_eq!(state.pending_popup_opens[0].target_surface, Some(201));
        assert_eq!(
            state.pending_popup_opens[0].context["origin_surface_id"].as_u64(),
            Some(201)
        );
    }
}
