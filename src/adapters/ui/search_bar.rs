use egui::emath::GuiRounding as _;

use crate::adapters::ui::icons;
use crate::adapters::ui::popup::{PopupAction, PopupScope};
use crate::i18n::t;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme::Theme;
use tasty_terminal::search::{SearchError, SearchOptions};
use tasty_type_geometry::length::LogicalPx;

/// 매치 카운터(`3/17`) 폰트. DTCG primitive `font-size-12` 는 있으나 semantic role 이
/// 없어 `Theme` 필드가 없다 — docs/design/systems/theme.md 대로 **이름에 primitive 임을 남긴다**.
const COUNTER_FONT_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

pub(crate) const SEARCH_BAR_POPUP_ID: &str = "search_bar";

/// Baseline search bar width; the sizer applies host zoom once.
const SEARCH_BAR_WIDTH: LogicalPx = LogicalPx(360.0);

/// The manager paints the border outside this rectangle. Its content keeps the
/// canonical Input height; two padding bands plus the outside border form the shell.
pub fn search_bar_default_size() -> egui::Vec2 {
    let theme = crate::theme::theme();
    let padding = LogicalPx(theme.popup_content_margin().value().round_ui());
    let height = theme.input_height() + padding.scaled(2.0);
    egui::vec2(
        crate::adapters::ui::zoomed_px(&theme, SEARCH_BAR_WIDTH).value(),
        height.value(),
    )
}

/// Recompute from current Theme tokens on every frame, including UI zoom changes.
pub fn search_bar_sizer(_state: &MainViewState, _engine: &EngineRead<'_>) -> egui::Vec2 {
    search_bar_default_size()
}

/// PopupDef::on_close entry point — 어떤 경로로 닫히든(대상 surface 소멸 포함) 검색 결과를 비운다.
/// 남겨 두면 다른 surface에서 다시 열 때 이전 surface의 좌표가 강조된다.
pub fn on_close_search_bar(
    _ctx: &egui::Context,
    state: &mut MainViewState,
    _engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    state.search.clear();
}

/// 포커스된 surface에 검색창을 연다. 그 surface에 이미 열려 있으면 입력 포커스만 준다.
/// 다른 surface에 열려 있으면(다른 탭이라 숨은 경우 포함) 그 검색을 비우고 창을 현재 surface로
/// 옮긴다. 닫았다 다시 열면 on_close 훅이 다음 프레임에 늦게 돌아 새 대상의 검색 상태를 지우고,
/// 열린 팝업에 보내는 OpenPopup intent는 무시되므로 창의 범위를 직접 옮긴다.
pub(crate) fn open_or_focus_for(
    state: &mut MainViewState,
    focused_surface: Option<u32>,
    source: &'static str,
) {
    use crate::intent::{OpenPopupMode, UiIntent};
    let Some(sid) = focused_surface else {
        return;
    };
    let scope = PopupScope::Surface(sid);
    if state.popups.open_scope(SEARCH_BAR_POPUP_ID) == Some(&scope) {
        state.popups.set_focused(SEARCH_BAR_POPUP_ID, true);
        return;
    }
    let open_elsewhere = state.popups.is_open(SEARCH_BAR_POPUP_ID);
    if open_elsewhere {
        state.search.clear();
    }
    state.search.surface_id = sid;
    if open_elsewhere {
        state
            .popups
            // intent-exempt: 열린 팝업의 OpenPopup은 무시되고 close→reopen은 늦은 on_close가 새 검색을 지운다. intent 로그 없음.
            .open_at_top_of_scope(SEARCH_BAR_POPUP_ID, scope);
    } else {
        state.dispatch_intent(
            UiIntent::OpenPopup {
                id: SEARCH_BAR_POPUP_ID,
                mode: OpenPopupMode::AtTopOfScope(scope),
            }
            .from_user_shortcut(source),
        );
    }
}

/// Draw the search bar popup content.
pub fn draw_search_bar(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
) -> PopupAction {
    let theme = crate::theme::theme();

    // 팝업의 focused 상태에 텍스트 필드 포커스를 맞춘다.
    let want_focus = state.popup_keyboard_focus.contains(&SEARCH_BAR_POPUP_ID);

    // Layout stays inside the body. Allow the Input's outside ring into the shell padding.
    ui.set_clip_rect(
        ui.clip_rect()
            .expand(theme.popup_content_margin().value().round_ui()),
    );

    let mut action = PopupAction::None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();

        // 고정 카운터·버튼·간격을 빼고 남은 폭을 입력 칸에 준다.
        let spacing = ui.spacing().item_spacing.x;
        let btn_size = theme.item_height_tab.value();
        let divider_width = theme.border_width.value();
        let reserved = 40.0 + 6.0 * btn_size + 8.0 * spacing + divider_width;
        let input_width = (ui.available_width() - reserved).max(60.0);

        // find 단축키는 입력칸보다 먼저 소비한다. 입력칸이 먼저 받으면 alt·option 조합의 글자가
        // 검색어에 들어간다.
        let query_id = ui.make_persistent_id("search_bar_query");
        let super_held = crate::adapters::ui::input::shortcuts::super_held(ui.ctx());
        let find_pressed = ui.memory(|m| m.has_focus(query_id))
            && ui.input_mut(|i| {
                crate::adapters::ui::input::shortcuts::consume_binding_egui(
                    &engine.settings.keybindings.find,
                    i,
                    super_held,
                )
            });
        let response = tasty_ui_widgets::Input::new()
            .ground(tasty_ui_widgets::Ground::Raised)
            .id(query_id)
            .placeholder(t("search.placeholder"))
            .width(input_width)
            .invalid(!state.search.query.is_empty() && state.search.last_error.is_some())
            .show(ui, &theme, &mut state.search.query);

        // 팝업이 포커스 상태인데 필드는 아니라면 포커스를 요청한다.
        if want_focus && !response.has_focus() {
            response.request_focus();
        }

        // 실제 필드 포커스가 있을 때만 검색 키를 처리한다.
        if response.has_focus() {
            // find 단축키 → 검색창은 그대로 두고 포커스만 터미널로 되돌린다.
            if find_pressed {
                response.surrender_focus();
                action = PopupAction::ReleaseFocus;
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                // Escape → 닫기 + 검색 상태 clear (하이라이트 제거) + 터미널 포커스 복귀.
                state.search.clear();
                action = PopupAction::Close;
            } else {
                if response.changed() {
                    let surface_id = focused_terminal_surface_id(state, engine);
                    state.search.surface_id = surface_id;
                    run_search(state, &engine.as_ref());
                }

                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let shift_held = ui.input(|i| i.modifiers.shift);

                if enter_pressed {
                    if shift_held {
                        state.search.prev_match();
                    } else {
                        state.search.next_match();
                    }
                    scroll_to_current_match(state, engine);
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    state.search.prev_match();
                    scroll_to_current_match(state, engine);
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    state.search.next_match();
                    scroll_to_current_match(state, engine);
                }
            }
        }

        // 검색어가 있는데 결과가 없거나 정규식 오류이면 카운터를 오류색으로 표시한다.
        let has_query = !state.search.query.is_empty();
        let (counter_text, counter_color) = if state.search.matches.is_empty() {
            let color = if has_query {
                theme.accent_danger_raised()
            } else {
                theme.text_muted()
            };
            ("0/0".to_string(), color)
        } else {
            let text = t("search.match_count")
                .replace("{current}", &(state.search.current_index + 1).to_string())
                .replace("{total}", &state.search.matches.len().to_string());
            (text, theme.text_muted())
        };
        draw_counter(ui, &counter_text, counter_color.into());

        let nav_enabled = !state.search.matches.is_empty();
        if nav_button(
            ui,
            &theme,
            icons::CHEVRON_UP,
            nav_enabled,
            t("search.prev_match_tooltip"),
        ) {
            state.search.prev_match();
            scroll_to_current_match(state, engine);
        }
        if nav_button(
            ui,
            &theme,
            icons::CHEVRON_DOWN,
            nav_enabled,
            t("search.next_match_tooltip"),
        ) {
            state.search.next_match();
            scroll_to_current_match(state, engine);
        }

        if toggle_button(
            ui,
            &theme,
            "Aa",
            !state.search.case_insensitive,
            t("search.case_tooltip"),
        ) {
            state.search.case_insensitive = !state.search.case_insensitive;
            run_search(state, &engine.as_ref());
        }
        if toggle_button(
            ui,
            &theme,
            ".*",
            state.search.regex,
            t("search.regex_tooltip"),
        ) {
            state.search.regex = !state.search.regex;
            run_search(state, &engine.as_ref());
        }
        if toggle_button(
            ui,
            &theme,
            "ab",
            state.search.whole_word,
            t("search.whole_word_tooltip"),
        ) {
            state.search.whole_word = !state.search.whole_word;
            run_search(state, &engine.as_ref());
        }

        draw_divider(ui, &theme);
        if nav_button(ui, &theme, icons::CLOSE, true, t("search.close_tooltip")) {
            state.search.clear();
            action = PopupAction::Close;
        }
    });

    action
}

/// 카운터가 바뀌어도 옆 버튼을 밀지 않도록 고정 폭에 가운데 정렬한다.
fn draw_counter(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(40.0, ui.available_height()),
        egui::Sense::hover(),
    );
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(COUNTER_FONT_PRIMITIVE_12.value()),
        color,
    );
    let pos = rect.center() - galley.size() * 0.5;
    ui.painter().galley(pos, galley, color);
}

/// 토글과 닫기 버튼 사이 구분선.
fn draw_divider(ui: &mut egui::Ui, theme: &Theme) {
    let height = theme.item_height_tab.value() * 0.6;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(theme.border_width.value(), height),
        egui::Sense::hover(),
    );
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// IconButton sm 규격의 정사각 슬롯을 할당하고 hover/active 배경 오버레이를
/// 그린다. `active` 면 토글-on 배경, `enabled=false` 면 상호작용 비활성(배경 없음).
/// rect 와 response 를 돌려준다.
fn icon_button_frame(
    ui: &mut egui::Ui,
    theme: &Theme,
    enabled: bool,
    active: bool,
) -> (egui::Rect, egui::Response) {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let btn_size = theme.item_height_tab.value();
    let radius = theme.corner_radius.value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(btn_size, btn_size), sense);
    if active || (enabled && resp.is_pointer_button_down_on()) {
        ui.painter()
            .rect_filled(rect, radius, theme.overlay_active().to_egui_premultiplied());
    } else if enabled && resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius, theme.overlay_hover().to_egui_premultiplied());
    }
    (rect, resp)
}

/// chevron SVG 를 담은 IconButton sm — prev/next 네비게이션. `enabled=false` 면
/// disabled ink로 그리고 클릭을 받지 않는다. Returns true when clicked.
fn nav_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: icons::Icon,
    enabled: bool,
    tooltip: impl Into<egui::WidgetText>,
) -> bool {
    let (rect, resp) = icon_button_frame(ui, theme, enabled, false);
    // disabled 아이콘 버튼은 공용 IconButton과 같이 opacity 없이 disabled ink를 쓴다.
    let color: egui::Color32 = if !enabled {
        theme.state_disabled_fg().into()
    } else if resp.hovered() {
        theme.text_primary().into()
    } else {
        theme.text_secondary().into()
    };
    let glyph = theme.icon_glyph_size_sm.value();
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    icon.image(glyph, color).paint_at(ui, icon_rect);
    if enabled {
        resp.clone().on_hover_text(tooltip);
        resp.clicked()
    } else {
        false
    }
}

/// mono 라벨을 담은 IconButton sm 토글 — active 면 active_overlay 배경 + 라벨 색
/// text_primary, 비활성이면 배경 없음 + text_muted. Returns true when clicked.
fn toggle_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    active: bool,
    tooltip: impl Into<egui::WidgetText>,
) -> bool {
    let (rect, resp) = icon_button_frame(ui, theme, true, active);
    let color: egui::Color32 = if active {
        theme.text_primary().into()
    } else {
        theme.text_muted().into()
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::monospace(theme.font_size_caption.value()),
        color,
    );
    let pos = rect.center() - galley.size() * 0.5;
    ui.painter().galley(pos, galley, color);
    resp.clone().on_hover_text(tooltip);
    resp.clicked()
}

/// 검색 상태 필드를 빌려 검색을 실행한다.
fn run_search(state: &mut MainViewState, engine: &EngineRead<'_>) {
    let surface_id = state.search.surface_id;
    let query = state.search.query.clone();
    let options = SearchOptions {
        case_insensitive: state.search.case_insensitive,
        regex: state.search.regex,
        whole_word: state.search.whole_word,
    };
    let result = engine.find_terminal_by_id(surface_id).map(|terminal| {
        terminal.with_content(|view| {
            state.search.epoch = view.cut().epoch;
            view.search(&query, &options)
        })
    });
    match result {
        Some(Ok(matches)) => {
            state.search.matches = matches;
            state.search.last_error = None;
        }
        Some(Err(SearchError::InvalidRegex(msg))) => {
            state.search.matches.clear();
            state.search.last_error = Some(msg);
        }
        None => {}
    }
    if state.search.matches.is_empty() {
        state.search.current_index = 0;
    } else if state.search.current_index >= state.search.matches.len() {
        state.search.current_index = state.search.matches.len() - 1;
    }
}

fn focused_terminal_surface_id(
    state: &MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) -> u32 {
    let ws = state.active_workspace(engine);
    let pane_id = state.navigation.pane_id(ws).unwrap_or(0);
    ws.pane_layout()
        .find_pane(pane_id)
        .and_then(|pane| pane.tabs.get(state.navigation.tab_index(pane)))
        .and_then(|tab| state.navigation.surface_id(tab))
        .unwrap_or(0)
}

fn scroll_to_current_match(state: &mut MainViewState, engine: &EngineRead<'_>) {
    let surface_id = state.search.surface_id;
    let search = &state.search;
    state
        .terminal_views
        .update(&engine.as_ref(), surface_id, |viewport, cut| {
            if let Some(offset) = search.scroll_to_current(cut) {
                viewport.set_scroll_offset(cut, offset);
            }
        });
}

#[cfg(test)]
#[path = "search_bar_tests.rs"]
mod tests;
