//! 명령을 검색해 위·아래 키로 선택하고 Enter로 실행한다.
//! 선택한 명령은 pending_run에 넣고 팝업을 닫으며 실제 실행은 MainView에서 처리한다.
//! 표시 함수는 갤러리에서도 앱 상태 없이 사용할 수 있다.

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::PopupAction;
use crate::i18n::t;
use crate::state::MainViewState;
use crate::state::command_palette::{self, PaletteCommand};
use crate::theme;
use crate::theme::Theme;
use tasty_settings::KeybindingSettings;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{KbdKey, margin_all};

/// footer 힌트 사이 가로 간격. 디자인 전사값 14 로 4px 그리드 밖이다
/// (`spacing_md`=12 와 2px 차).
const PALETTE_HINT_GAP_X: LogicalPx = LogicalPx(14.0);

// 파일 상수에는 Theme의 배율이 자동 적용되지 않으므로 zoomed()를 거쳐 사용한다.

/// footer 한 줄 높이에 더해지는 상하 패딩 + 보더 몫(디자인 padding 8 12 + borderTop).
const PALETTE_FOOTER_CHROME: LogicalPx = LogicalPx(20.0);

pub const COMMAND_PALETTE_POPUP_ID: &str = "command_palette";

/// 파일 상수에 현재 UI 배율을 적용한다.
fn zoomed(theme: &Theme, px: LogicalPx) -> f32 {
    crate::adapters::ui::zoomed_px(theme, px).value()
}

/// Theme의 공용 control-height를 사용하는 명령 행 높이.
fn palette_row_height(theme: &Theme) -> f32 {
    theme.item_height_interactive.value()
}

/// 카드 크기 계산과 렌더링에서 공유하는 푸터 높이.
fn palette_footer_height(theme: &Theme) -> f32 {
    theme.font_size_caption.value() + zoomed(theme, PALETTE_FOOTER_CHROME)
}

/// 목록 구역이 차지하는 높이 — 표시 항목 수로 정해지고 `palette-list-max-height`에서 멈춘다.
/// 상한과 행 높이가 함께 배율을 따르므로 보이는 행 수는 배율과 관계없이 같다.
///
/// 항목 0 건은 목록 대신 "결과 없음" 한 줄이 그려지므로 행 하나 높이로 둔다.
fn palette_list_height(theme: &Theme, item_count: usize) -> f32 {
    let row = palette_row_height(theme);
    if item_count == 0 {
        return row;
    }
    (item_count as f32 * row).min(theme.palette_list_max_height().value())
}

/// 목록을 뺀 나머지가 늘 차지하는 높이 — 검색 구역(사방 space-md) + 목록의 위아래
/// 여백(space-sm) + footer. 목록과 footer 사이에 따로 두는 간격은 없다.
fn palette_chrome_height(theme: &Theme) -> f32 {
    let search_h = 2.0 * theme.spacing_md.value() + theme.input_height().value();
    search_h + 2.0 * theme.spacing_sm.value() + palette_footer_height(theme)
}

/// 콘텐츠 맞춤 카드 높이.
fn palette_height(theme: &Theme, item_count: usize) -> f32 {
    palette_chrome_height(theme) + palette_list_height(theme, item_count)
}

/// View 입력 — 한 명령 행의 시각/의미 데이터.
#[derive(Debug, Clone)]
pub struct CommandItemView {
    /// 사용자에게 보이는 라벨 (i18n 해결 + 끝 `:` 제거).
    pub label: String,
    /// 우측에 표시할 단축키 — 키캡 토큰 단위(`["Ctrl","Shift","N"]`). 빈 vec 이면
    /// 표시하지 않는다. `+` 구분자 모호성 회피를 위해 단일 문자열이 아닌 토큰 벡터.
    pub shortcut_keys: Vec<String>,
    /// 행 좌측 leading 아이콘. 6개 디자인 명시 명령은 전용 아이콘, 나머지 동적
    /// 명령은 `COMMAND` fallback 글리프. None 이면 빈 슬롯 (라벨 정렬은 유지).
    pub icon: Option<icons::Icon>,
}

/// 명령 팔레트 입력. 검색 버퍼는 호출부에서 빌려 쓴다.
pub struct CommandPaletteProps<'a> {
    /// TextEdit 의 placeholder.
    pub placeholder: String,
    /// 매칭 결과 0 건일 때 표시할 메시지.
    pub no_results_text: String,
    /// 필터된 명령 목록. 순서는 wrapper 의 score 정렬 결과.
    pub items: Vec<CommandItemView>,
    /// 현재 선택 인덱스 — items 가 비어 있으면 무시.
    pub selected_index: usize,
    /// 쿼리 입력 버퍼. View 가 `TextEdit` 으로 직접 mutate.
    pub query_buffer: &'a mut String,
    /// 푸터 힌트 — 네비게이션 동작 라벨 (`↑↓ {navigate}`).
    pub hint_navigate: String,
    /// 푸터 힌트 — 실행 동작 라벨 (`↵ {run}`).
    pub hint_run: String,
    /// 푸터 힌트 — 닫기 동작 라벨 (`esc {close}`).
    pub hint_close: String,
}

/// View 의 출력 — 사용자 의도. wrapper 가 state mutation 으로 변환.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPaletteAction {
    None,
    /// 사용자가 한 항목을 Enter 또는 마우스 클릭으로 실행. items 의 인덱스.
    Execute {
        index: usize,
    },
    /// TextEdit 의 내용이 변경됨. wrapper 는 selected_index 를 0 으로 리셋.
    QueryChanged,
    /// 키보드 ↑/↓ 또는 마우스 hover 로 선택 인덱스 변경. items 범위 내에서만 발생.
    SelectionChanged(usize),
    /// Escape 키.
    Close,
}

/// 키 입력과 검색 변경을 화면 동작으로 반환한다.
pub fn draw_command_palette_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    props: &mut CommandPaletteProps<'_>,
) -> CommandPaletteAction {
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        return CommandPaletteAction::Close;
    }

    let (up, down, enter) = ui.ctx().input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
        )
    });

    let mut action = CommandPaletteAction::None;

    // 디자인(command_palette.jsx:51 `active = n===0 && q!==""`): 쿼리가 비면 어떤
    // 행도 강조하지 않는다. 한 글자라도 입력하면 첫 매칭 행이 강조된다.
    let query_empty = props.query_buffer.is_empty();

    if !props.items.is_empty() {
        if down {
            let next = (props.selected_index + 1).min(props.items.len() - 1);
            if next != props.selected_index {
                action = CommandPaletteAction::SelectionChanged(next);
            }
        }
        if up {
            let next = props.selected_index.saturating_sub(1);
            if next != props.selected_index {
                action = CommandPaletteAction::SelectionChanged(next);
            }
        }
    }

    // 공통 content_margin 없이 검색·목록·푸터에 각각 여백을 준다.
    let full = ui.max_rect();
    let sep = egui::Stroke::new(theme.border_width.value(), theme.border_strong());
    ui.spacing_mut().item_spacing.y = 0.0;

    let mut query_changed = false;
    let search_ir = egui::Frame::NONE
        .inner_margin(margin_all(theme.spacing_md))
        .show(ui, |ui| {
            let resp = tasty_ui_widgets::Input::new()
                .ground(tasty_ui_widgets::Ground::Raised)
                .placeholder(&props.placeholder)
                .icon(&|ui, rect, color| {
                    icons::SEARCH.image(rect.height(), color).paint_at(ui, rect);
                })
                .show(ui, theme, props.query_buffer);
            if !resp.has_focus() {
                resp.request_focus();
            }
            if resp.changed() {
                query_changed = true;
            }
        });
    if query_changed {
        action = CommandPaletteAction::QueryChanged;
    }
    ui.painter()
        .hline(full.x_range(), search_ir.response.rect.bottom(), sep);

    // 카드 높이 계산과 같은 푸터 높이를 예약한다.
    let footer_h = palette_footer_height(theme);
    let footer_top = full.bottom() - footer_h;

    // 목록 여백은 사방 space-sm이다.
    egui::Frame::NONE
        .inner_margin(margin_all(theme.spacing_sm))
        .show(ui, |ui| {
            if props.items.is_empty() {
                ui.label(
                    egui::RichText::new(&props.no_results_text)
                        .color(theme.text_muted_raised().to_egui())
                        .italics(),
                );
                return;
            }
            let row_height = palette_row_height(theme);
            // 행 치수는 디자인 MenuItem과 같은 토큰이다(갤러리 `menu_item_kbd`와 공용).
            let pad_x = theme.menu_item_padding_x().value();
            let icon_size = theme.icon_glyph_size_md.value();
            let icon_gap = theme.spacing_sm.value();
            let radius = theme.menu_item_radius().value();
            let selected_idx = props.selected_index;
            let list_h =
                (footer_top - ui.cursor().top() - theme.spacing_sm.value()).max(row_height);
            egui::ScrollArea::vertical()
                .max_height(list_h)
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    for (i, item) in props.items.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row_height),
                            egui::Sense::click(),
                        );
                        let is_selected = row_highlighted(query_empty, i, selected_idx);
                        if is_selected {
                            ui.painter().rect_filled(
                                rect,
                                radius,
                                theme.active_overlay.to_egui_premultiplied(),
                            );
                        } else if resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                radius,
                                theme.hover_overlay.to_egui_premultiplied(),
                            );
                        }
                        let color: egui::Color32 = if is_selected || resp.hovered() {
                            theme.text_on_active().into()
                        } else {
                            theme.text_muted_raised().into()
                        };

                        if let Some(icon) = item.icon {
                            let icon_rect = egui::Rect::from_min_size(
                                egui::pos2(rect.min.x + pad_x, rect.center().y - icon_size / 2.0),
                                egui::vec2(icon_size, icon_size),
                            );
                            icon.image(icon_size, color).paint_at(ui, icon_rect);
                        }
                        let label_x = rect.min.x + pad_x + icon_size + icon_gap;
                        ui.painter().text(
                            egui::pos2(label_x, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &item.label,
                            egui::FontId::proportional(theme.font_size_body.value()),
                            color,
                        );
                        draw_keycaps(
                            ui,
                            theme,
                            rect.max.x - pad_x,
                            rect.center().y,
                            &item.shortcut_keys,
                            is_selected,
                        );
                        if resp.clicked() {
                            action = CommandPaletteAction::Execute { index: i };
                        }
                    }
                });
        });

    let cur = ui.cursor().top();
    if cur < footer_top {
        ui.add_space(footer_top - cur);
    }
    ui.painter().hline(full.x_range(), footer_top, sep);
    egui::Frame::NONE
        .inner_margin(egui::Margin {
            left: theme.spacing_md.value() as i8,
            right: theme.spacing_md.value() as i8,
            top: theme.spacing_sm.value() as i8,
            bottom: theme.spacing_sm.value() as i8,
        })
        .show(ui, |ui| {
            let hint_color = theme.text_muted_raised().to_egui();
            let hint_font = tasty_ui_widgets::label_font(theme.font_size_caption.value());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = PALETTE_HINT_GAP_X.value();
                for hint in [
                    format!("↑↓ {}", props.hint_navigate),
                    format!("↵ {}", props.hint_run),
                    format!("esc {}", props.hint_close),
                ] {
                    ui.label(
                        egui::RichText::new(hint)
                            .font(hint_font.clone())
                            .color(hint_color),
                    );
                }
            });
        });

    if enter && !props.items.is_empty() {
        action = CommandPaletteAction::Execute {
            index: props.selected_index,
        };
    }

    action
}

/// 행 강조 여부 — 쿼리가 비어있지 않고(`!query_empty`) 선택 인덱스와 일치할 때만.
/// 디자인 `command_palette.jsx:51 active = n===0 && q!==""` 의 일반화(선택 인덱스 n).
fn row_highlighted(query_empty: bool, row: usize, selected: usize) -> bool {
    !query_empty && row == selected
}

/// 공용 키캡 위젯으로 키캡을 오른쪽 정렬한다. 강조 행이면 `+`를 선택 행 색으로 그린다.
fn draw_keycaps(
    ui: &egui::Ui,
    theme: &Theme,
    right_x: f32,
    center_y: f32,
    keys: &[String],
    on_active: bool,
) {
    if keys.is_empty() {
        return;
    }
    let parts: Vec<KbdKey<'_>> = keys.iter().map(|k| KbdKey::Text(k.as_str())).collect();
    let ground = if on_active {
        tasty_ui_widgets::Ground::Active
    } else {
        tasty_ui_widgets::Ground::Raised
    };
    tasty_ui_widgets::kbd_parts_at(ui, theme, &parts, right_x, center_y, ground);
}

/// 화면 항목과 실행할 명령을 같은 순서로 반환한다.
fn items_from_state(
    commands: &[PaletteCommand],
    labels: &[String],
    query: &str,
    keys_for: impl Fn(&PaletteCommand) -> Vec<String>,
) -> (Vec<CommandItemView>, Vec<PaletteCommand>) {
    let matches = command_palette::search(query, commands, labels);
    let mut items = Vec::with_capacity(matches.len());
    let mut ids = Vec::with_capacity(matches.len());
    for (_score, cmd) in matches {
        // 검색 결과는 commands 원소의 참조이므로 같은 원소의 라벨을 찾는다.
        let raw_label = commands
            .iter()
            .position(|c| std::ptr::eq(c, cmd))
            .and_then(|i| labels.get(i))
            .map(|s| s.trim_end_matches(':').to_string())
            .unwrap_or_default();
        items.push(CommandItemView {
            label: raw_label,
            shortcut_keys: keys_for(cmd),
            icon: icon_for(cmd),
        });
        ids.push(cmd.clone());
    }
    (items, ids)
}

/// 바깥 클릭 등 그리기를 거치지 않는 닫기에서도 검색어·선택을 초기화한다.
pub fn on_close_command_palette_popup(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.command_palette.reset();
}

/// 현재 검색 결과 수에 맞춰 카드 높이를 계산한다. 사용자 리사이즈 전까지 매 프레임 적용된다.
/// 표시 항목 전체를 만들지는 않고 검색 결과 수로 높이를 정한다. 폭에는 UI 배율을 적용한다.
pub fn command_palette_sizer(
    state: &MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> egui::Vec2 {
    let commands = command_palette::all_commands(&state.palette_plugin_commands);
    let labels: Vec<String> = commands.iter().map(label_for).collect();
    let matched = command_palette::search(&state.command_palette.query, &commands, &labels).len();
    let th = theme::theme();
    egui::vec2(th.palette_width().value(), palette_height(&th, matched))
}

/// 앱 상태를 화면 입력으로 바꾸고 반환된 동작을 처리한다.
pub fn draw_command_palette_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> PopupAction {
    let commands = command_palette::all_commands(&state.palette_plugin_commands);
    let labels: Vec<String> = commands.iter().map(label_for).collect();
    let (items, matched_commands) =
        items_from_state(&commands, &labels, &state.command_palette.query, |cmd| {
            // 여기서는 PluginManager의 사용자 단축키 설정을 읽을 수 없어 플러그인 키캡을 생략한다.
            let PaletteCommand::Host { id, .. } = cmd else {
                return Vec::new();
            };
            // + 키가 구분자와 섞이지 않도록 표시 문자열을 split하지 않고 토큰으로 받는다.
            engine
                .settings
                .keybindings
                .get_bindings(id)
                .and_then(|b| b.first())
                .map(|s| KeybindingSettings::format_display_parts(s, &engine.settings.general))
                .unwrap_or_default()
        });

    if items.is_empty() {
        state.command_palette.selected = 0;
    } else if state.command_palette.selected >= items.len() {
        state.command_palette.selected = items.len() - 1;
    }

    let mut props = CommandPaletteProps {
        placeholder: t("command_palette.placeholder").to_string(),
        no_results_text: t("command_palette.no_results").to_string(),
        items,
        selected_index: state.command_palette.selected,
        query_buffer: &mut state.command_palette.query,
        hint_navigate: t("command_palette.hint_navigate").to_string(),
        hint_run: t("command_palette.hint_run").to_string(),
        hint_close: t("command_palette.hint_close").to_string(),
    };

    let view_action = draw_command_palette_view(ui, &theme::theme(), &mut props);

    match view_action {
        CommandPaletteAction::None => PopupAction::None,
        CommandPaletteAction::Close => {
            state.command_palette.reset();
            PopupAction::Close
        }
        CommandPaletteAction::QueryChanged => {
            state.command_palette.selected = 0;
            PopupAction::None
        }
        CommandPaletteAction::SelectionChanged(idx) => {
            state.command_palette.selected = idx;
            PopupAction::None
        }
        CommandPaletteAction::Execute { index } => {
            if let Some(cmd) = matched_commands.get(index) {
                state.command_palette.pending_run = Some(cmd.clone());
            }
            state.command_palette.reset();
            PopupAction::Close
        }
    }
}

/// leading 아이콘. 디자인(`command_palette.jsx`)이 명시한 6개 호스트 명령은 전용
/// 아이콘, 나머지(동적 호스트 명령 + plugin 명령)는 모두 `COMMAND` fallback 글리프.
fn icon_for(cmd: &PaletteCommand) -> Option<icons::Icon> {
    let PaletteCommand::Host { id, .. } = cmd else {
        return Some(icons::COMMAND);
    };
    Some(match *id {
        "new_workspace" => icons::PLUS,
        "new_tab" => icons::TERM,
        "open_markdown" => icons::MD,
        "toggle_settings" => icons::SETTINGS,
        "split_pane_vertical" => icons::SPLIT,
        _ => icons::COMMAND,
    })
}

/// 플러그인을 포함한 공용 번역에서 라벨을 읽고 끝의 :를 제거한다. 키가 없으면 원문을 쓴다.
fn label_for(cmd: &PaletteCommand) -> String {
    let raw = match cmd {
        PaletteCommand::Host { label_key, .. } => t(label_key).to_string(),
        PaletteCommand::Plugin { title_i18n_key, .. } => {
            crate::adapters::ui::label_or_raw_key(title_i18n_key)
        }
    };
    raw.trim_end_matches(':').to_string()
}

#[cfg(test)]
mod props_tests {
    use super::*;

    fn host(id: &'static str, label_key: &'static str) -> PaletteCommand {
        PaletteCommand::Host { id, label_key }
    }

    fn plugin_cmd(plugin_id: &str, command_id: &str) -> PaletteCommand {
        PaletteCommand::Plugin {
            plugin_id: plugin_id.to_string(),
            command_id: command_id.to_string(),
            title_i18n_key: format!("{plugin_id}.{command_id}.title"),
        }
    }

    fn host_id(cmd: &PaletteCommand) -> &str {
        match cmd {
            PaletteCommand::Host { id, .. } => id,
            PaletteCommand::Plugin { .. } => panic!("expected Host"),
        }
    }

    #[test]
    fn items_from_state_empty_query_returns_all_in_order() {
        let cmds = vec![host("a", "k.a"), host("b", "k.b")];
        let labels = vec!["Alpha".to_string(), "Beta".to_string()];
        let (items, ids) = items_from_state(&cmds, &labels, "", |cmd| match host_id(cmd) {
            "a" => vec!["Ctrl".to_string(), "A".to_string()],
            _ => Vec::new(),
        });
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "Alpha");
        assert_eq!(items[0].shortcut_keys, vec!["Ctrl", "A"]);
        assert_eq!(items[1].label, "Beta");
        assert!(items[1].shortcut_keys.is_empty());
        assert_eq!(ids, vec![host("a", "k.a"), host("b", "k.b")]);
    }

    #[test]
    fn items_from_state_filters_by_query() {
        let cmds = vec![host("new_workspace", "k.new"), host("close_tab", "k.close")];
        let labels = vec!["New workspace".to_string(), "Close tab".to_string()];
        let (items, ids) = items_from_state(&cmds, &labels, "close", |_| Vec::new());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Close tab");
        assert_eq!(ids, vec![host("close_tab", "k.close")]);
    }

    #[test]
    fn items_from_state_strips_trailing_colon_from_labels() {
        let cmds = vec![host("a", "k.a")];
        let labels = vec!["Settings: New window:".to_string()];
        let (items, _ids) = items_from_state(&cmds, &labels, "", |_| Vec::new());
        assert_eq!(items[0].label, "Settings: New window");
    }

    #[test]
    fn items_from_state_includes_plugin_commands() {
        let cmds = vec![host("a", "k.a"), plugin_cmd("com.example.x", "x.open")];
        let labels = vec!["Alpha".to_string(), "Open X".to_string()];
        let (items, ids) = items_from_state(&cmds, &labels, "open", |_| Vec::new());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Open X");
        assert_eq!(ids, vec![plugin_cmd("com.example.x", "x.open")]);
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use tasty_themes::mocha_fallback;

    fn make_items(count: usize) -> Vec<CommandItemView> {
        (0..count)
            .map(|i| CommandItemView {
                label: format!("Item {i}"),
                shortcut_keys: Vec::new(),
                icon: None,
            })
            .collect()
    }

    /// 1 frame egui Context 안에서 view 함수를 호출하고 결과 action 을 받는다.
    fn run_view(
        items: Vec<CommandItemView>,
        selected_index: usize,
        pressed_key: Option<egui::Key>,
    ) -> CommandPaletteAction {
        let ctx = egui::Context::default();
        let mut action = CommandPaletteAction::None;
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

        let mut items_opt = Some(items);
        let _full = ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut props = CommandPaletteProps {
                    placeholder: "Search…".to_string(),
                    no_results_text: "No matches".to_string(),
                    items: items_opt.take().unwrap_or_default(),
                    selected_index,
                    query_buffer: &mut buf,
                    hint_navigate: "navigate".to_string(),
                    hint_run: "run".to_string(),
                    hint_close: "close".to_string(),
                };
                action = draw_command_palette_view(ui, &theme, &mut props);
            });
        });
        action
    }

    /// 한 프레임을 그려 텍스트 중심과 푸터 위 구분선의 y좌표를 반환한다.
    fn painted(items: Vec<CommandItemView>, card_h: f32) -> (Vec<(String, f32)>, f32) {
        fn walk(shape: &egui::epaint::Shape, texts: &mut Vec<(String, f32)>, last_line: &mut f32) {
            match shape {
                egui::epaint::Shape::Text(t) => texts.push((
                    t.galley.text().to_owned(),
                    t.pos.y + t.galley.rect.height() * 0.5,
                )),
                egui::epaint::Shape::LineSegment { points, .. } => {
                    *last_line = last_line.max(points[0].y)
                }
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts, last_line)),
                _ => {}
            }
        }

        let ctx = egui::Context::default();
        let theme = mocha_fallback();
        let mut buf = String::new();
        let mut items_opt = Some(items);
        let out = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(theme.palette_width().value(), card_h),
                );
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                let mut props = CommandPaletteProps {
                    placeholder: "Search…".to_string(),
                    no_results_text: "No matches".to_string(),
                    items: items_opt.take().unwrap_or_default(),
                    selected_index: 0,
                    query_buffer: &mut buf,
                    hint_navigate: "navigate".to_string(),
                    hint_run: "run".to_string(),
                    hint_close: "close".to_string(),
                };
                draw_command_palette_view(&mut child, &theme, &mut props);
            });
        });
        let mut texts = Vec::new();
        let mut last_line = f32::MIN;
        for cs in &out.shapes {
            walk(&cs.shape, &mut texts, &mut last_line);
        }
        (texts, last_line)
    }

    fn label_y(texts: &[(String, f32)], label: &str) -> f32 {
        texts
            .iter()
            .find(|(t, _)| t == label)
            .unwrap_or_else(|| panic!("{label} was not painted; painted = {texts:?}"))
            .1
    }

    #[test]
    fn sized_height_puts_the_footer_right_under_the_last_row() {
        let th = mocha_fallback();
        let (texts, footer_line) = painted(make_items(1), palette_height(&th, 1));
        let gap = footer_line - label_y(&texts, "Item 0");
        assert!(
            gap < palette_row_height(&th),
            "footer should sit within a row of the last item, gap = {gap}"
        );
    }

    #[test]
    fn the_old_fixed_height_left_the_gap_this_sizing_removes() {
        // 높이를 크게 고정한 대조군에서는 목록 아래 여백이 남아야 한다.
        let (texts, footer_line) = painted(make_items(1), 412.0);
        let gap = footer_line - label_y(&texts, "Item 0");
        assert!(
            gap > 200.0,
            "fixed height should leave a big gap, got {gap}"
        );
    }

    #[test]
    fn every_item_is_painted_above_the_footer_at_the_sized_height() {
        let th = mocha_fallback();
        let n = 5;
        let (texts, footer_line) = painted(make_items(n), palette_height(&th, n));
        for i in 0..n {
            let y = label_y(&texts, &format!("Item {i}"));
            assert!(
                y < footer_line,
                "Item {i} at {y} is not above {footer_line}"
            );
        }
    }

    #[test]
    fn height_grows_with_the_item_count_and_stops_at_the_list_cap() {
        let th = mocha_fallback();
        let row = palette_row_height(&th);
        assert_eq!(palette_height(&th, 2) - palette_height(&th, 1), row);
        assert_eq!(palette_height(&th, 12), palette_height(&th, 1000));
        assert!(palette_height(&th, 11) < palette_height(&th, 12));
    }

    /// 목록 상한과 행 높이가 함께 배율을 따라 어느 배율에서도 11 행까지 보이고 12 행부터 스크롤한다.
    #[test]
    fn the_list_cap_shows_eleven_rows_at_every_ui_scale() {
        for zoom in [0.85_f32, 1.0, 1.2] {
            let th = Theme::with_colors_and_zoom(mocha_fallback().to_colors(), false, zoom);
            assert!(
                palette_height(&th, 11) < palette_height(&th, 12),
                "zoom {zoom}: 11 rows should still grow the card"
            );
            assert_eq!(
                palette_height(&th, 12),
                palette_height(&th, 1000),
                "zoom {zoom}: the cap should stop the card at 12 rows"
            );
        }
    }

    /// 카드 폭은 palette-width 토큰이며 UI 배율을 곱한다. 디자인이 적은 배율별 폭과 같다.
    #[test]
    fn card_width_follows_the_palette_width_token_at_every_ui_scale() {
        for (zoom, width) in [(0.85_f32, 459.0), (1.0, 540.0), (1.2, 648.0)] {
            let th = Theme::with_colors_and_zoom(mocha_fallback().to_colors(), false, zoom);
            assert_eq!(th.palette_width().value(), width, "zoom {zoom}");
        }
    }

    /// 행 라벨의 시작 x는 목록 여백 + MenuItem 토큰(padding-x · icon-size-md · space-sm)이고
    /// 배율과 함께 커진다.
    #[test]
    fn row_label_starts_after_the_menu_item_tokens_at_every_ui_scale() {
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(String, f32)>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push((t.galley.text().to_owned(), t.pos.x)),
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        for zoom in [0.85_f32, 1.0, 1.2] {
            let th = Theme::with_colors_and_zoom(mocha_fallback().to_colors(), false, zoom);
            let ctx = egui::Context::default();
            let mut buf = String::new();
            let mut items = Some(make_items(1));
            let out = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        egui::vec2(th.palette_width().value(), palette_height(&th, 1)),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    let mut props = CommandPaletteProps {
                        placeholder: "Search…".to_string(),
                        no_results_text: "No matches".to_string(),
                        items: items.take().unwrap_or_default(),
                        selected_index: 0,
                        query_buffer: &mut buf,
                        hint_navigate: "navigate".to_string(),
                        hint_run: "run".to_string(),
                        hint_close: "close".to_string(),
                    };
                    draw_command_palette_view(&mut child, &th, &mut props);
                });
            });
            let mut texts = Vec::new();
            for clipped in &out.shapes {
                walk(&clipped.shape, &mut texts);
            }
            let x = texts
                .iter()
                .find(|(t, _)| t == "Item 0")
                .map(|(_, x)| *x)
                .expect("row label painted");
            let want = th.spacing_sm.value()
                + th.menu_item_padding_x().value()
                + th.icon_glyph_size_md.value()
                + th.spacing_sm.value();
            assert!(
                (x - want).abs() < 0.5,
                "zoom {zoom}: label x {x}, want {want}"
            );
        }
    }

    /// 검색 구역은 사방 space-md, 목록은 위아래 space-sm이고 목록과 footer 사이 간격은 따로 없다.
    #[test]
    fn chrome_is_search_space_md_list_space_sm_and_the_footer() {
        let th = mocha_fallback();
        let want = 2.0 * th.spacing_md.value()
            + th.input_height().value()
            + 2.0 * th.spacing_sm.value()
            + palette_footer_height(&th);
        assert_eq!(palette_chrome_height(&th), want);
    }

    #[test]
    fn no_results_reserves_one_row_not_a_full_list() {
        let th = mocha_fallback();
        assert_eq!(palette_height(&th, 0), palette_height(&th, 1));
        let (texts, footer_line) = painted(Vec::new(), palette_height(&th, 0));
        let y = label_y(&texts, "No matches");
        assert!(
            y < footer_line,
            "empty-state line at {y} is under {footer_line}"
        );
    }

    #[test]
    fn escape_key_returns_close() {
        let action = run_view(make_items(3), 0, Some(egui::Key::Escape));
        assert_eq!(action, CommandPaletteAction::Close);
    }

    #[test]
    fn enter_key_executes_selected_item() {
        let action = run_view(make_items(3), 1, Some(egui::Key::Enter));
        assert_eq!(action, CommandPaletteAction::Execute { index: 1 });
    }

    #[test]
    fn enter_with_empty_items_does_not_execute() {
        let action = run_view(vec![], 0, Some(egui::Key::Enter));
        assert_eq!(action, CommandPaletteAction::None);
    }

    #[test]
    fn arrow_down_moves_selection() {
        let action = run_view(make_items(3), 0, Some(egui::Key::ArrowDown));
        assert_eq!(action, CommandPaletteAction::SelectionChanged(1));
    }

    #[test]
    fn arrow_up_at_top_does_not_change_selection() {
        let action = run_view(make_items(3), 0, Some(egui::Key::ArrowUp));
        assert_eq!(action, CommandPaletteAction::None);
    }

    #[test]
    fn arrow_down_at_bottom_is_clamped() {
        let action = run_view(make_items(3), 2, Some(egui::Key::ArrowDown));
        assert_eq!(action, CommandPaletteAction::None);
    }

    #[test]
    fn no_input_returns_none() {
        let action = run_view(make_items(3), 0, None);
        assert_eq!(action, CommandPaletteAction::None);
    }

    #[test]
    fn empty_query_never_highlights() {
        assert!(!row_highlighted(true, 0, 0));
        assert!(!row_highlighted(true, 2, 2));
    }

    #[test]
    fn non_empty_query_highlights_selected_row_only() {
        assert!(row_highlighted(false, 0, 0));
        assert!(row_highlighted(false, 3, 3));
        assert!(!row_highlighted(false, 1, 0));
    }
}

#[cfg(test)]
mod sizer_wiring_tests {
    use super::*;
    use crate::adapters::ui::draw_popups;
    use crate::model::{PhysicalPx, PhysicalRect};
    use crate::state::tests::test_state;

    fn run_one_frame(
        state: &mut MainViewState,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        let ctx = egui::Context::default();
        let term = PhysicalRect {
            x: PhysicalPx(0.0),
            y: PhysicalPx(0.0),
            width: PhysicalPx(1920.0),
            height: PhysicalPx(1080.0),
        };
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            draw_popups(ctx, state, engine, &[], term, 1.0);
        }));
    }

    fn card_height_after_a_frame(query: &str) -> f32 {
        let (mut state, mut engine_session) = test_state();
        let engine = engine_session.borrow_mut();
        state
            .popups
            .open_at_focused(COMMAND_PALETTE_POPUP_ID, egui::pos2(100.0, 100.0));
        state.command_palette.query = query.to_string();
        run_one_frame(&mut state, &engine.read());
        state
            .popups
            .get_mut(COMMAND_PALETTE_POPUP_ID)
            .expect("palette registered")
            .size
            .y
    }

    /// 실제 프레임 처리 후 sizer 결과가 팝업 크기에 적용됐는지 확인한다.
    #[test]
    fn a_frame_sizes_the_card_from_the_match_count() {
        let th = theme::theme();
        // 어떤 명령과도 안 맞는 쿼리 → 목록 대신 "결과 없음" 한 줄.
        let empty = card_height_after_a_frame("zzzz-no-such-command-zzzz");
        assert_eq!(empty, palette_height(&th, 0));
        // 빈 쿼리 → 전 명령이 매칭돼 목록이 상한에 닿는다.
        let full = card_height_after_a_frame("");
        assert_eq!(full, palette_height(&th, 1000));
        assert!(
            full > empty,
            "full list should be taller than the empty state"
        );
    }
}
