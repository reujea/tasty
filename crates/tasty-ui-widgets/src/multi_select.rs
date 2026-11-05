//! 항목을 연속으로 선택할 수 있는 드롭다운. 단일 Select와 트리거의 배치·그리기를 공유한다.
//! 선택 상태는 호출자가 소유하며 요약 문구와 선택적인 전체 선택/해제 문구도 전달한다.
//! 항목을 눌러도 메뉴는 열린 채로 남고 바깥 클릭이나 Esc로 닫힌다.
//! 키보드 동작·비활성 행·일괄 선택 규칙은 multi_select 함수의 계약을 따른다.

use tasty_type_appearance::theme::Theme;

use crate::keyboard_cursor::{edge_enabled, row_enabled, step_active};
use crate::select::{alloc_trigger, paint_chevron, paint_trigger_box};

/// 선택 개수에 따라 표시할 요약 문구. 번역은 호출자가 제공한다.
#[derive(Clone, Copy, Debug)]
pub struct MultiSelectLabels<'a> {
    /// 아무것도 선택되지 않았을 때. 옵션이 0 개일 때도 이 문구다.
    pub none: &'a str,
    /// 일부 선택 상태. 첫 번째 {}만 선택 개수로 바꾸고 나머지는 유지한다.
    pub some: &'a str,
    /// 하나만 선택했을 때의 `some`. 단수형이 따로 없는 언어는 `some` 과 같은 문구다.
    pub some_one: &'a str,
    /// 전부 선택됐을 때. 개수를 쓰지 않는 별도 문구라 치환 자리가 없다.
    pub all: &'a str,
}

/// 선택적인 일괄 토글 행의 문구. Some일 때만 행을 표시한다.
#[derive(Clone, Copy, Debug)]
pub struct MultiSelectAllToggle<'a> {
    /// 아직 전부 켜지지 않았을 때. 누르면 (토글 가능한) 모든 옵션이 켜진다.
    pub select_all: &'a str,
    /// 이미 전부 켜져 있을 때. 누르면 (토글 가능한) 모든 옵션이 꺼진다.
    pub clear_all: &'a str,
}

/// 선택이 없거나 옵션이 비면 none, 전부 선택하면 all, 나머지는 개수를 넣은 some을 반환한다.
pub fn multi_select_summary(labels: &MultiSelectLabels<'_>, selected: &[bool]) -> String {
    let n = selected.iter().filter(|s| **s).count();
    if n == 0 {
        labels.none.to_owned()
    } else if n == selected.len() {
        labels.all.to_owned()
    } else {
        let template = if n == 1 { labels.some_one } else { labels.some };
        template.replacen("{}", &n.to_string(), 1)
    }
}

/// 외부에서 팝업 상태를 다룰 때 사용하는 ID. 호출자가 ID 생성 규칙을 복제하지 않게 공개한다.
pub fn multi_select_popup_id(ui: &egui::Ui, id_salt: &str) -> egui::Id {
    ui.make_persistent_id(("tasty_multi_select", id_salt))
}

/// 메뉴 전체 폭에서 본문 폭을 구할 때 빼야 하는 프레임의 가로 여유.
/// 메뉴 프레임은 [`menu_style`]로 그리므로 같은 토큰(안쪽 여백·테두리)에서 계산한다.
pub fn popup_chrome_width(theme: &Theme) -> f32 {
    (theme.multiselect_menu_padding().value() + theme.border_width.value()) * 2.0
}

/// 메뉴 프레임용 style. egui 팝업은 부모 style로 프레임을 만들므로 그림자만 바꾼 사본을 넘긴다.
/// 채움·테두리·반경은 `with_popover_frame`이 메뉴 토큰으로 정하며 multiselect-menu-bg·border·
/// radius 는 그 별칭이다. 안쪽 여백은 [`menu_margin`]을 래퍼에 넘겨 정한다. 메뉴 안의 행 style은
/// 바뀌지 않는다.
fn menu_style(base: &egui::Style, theme: &Theme) -> egui::Style {
    let mut style = base.clone();
    style.visuals.popup_shadow = theme.shadow_popover().to_egui();
    style
}

/// 메뉴 안쪽 여백 `multiselect-menu-padding`. 래퍼가 팝오버 틀에 넣는다.
fn menu_margin(theme: &Theme) -> egui::Margin {
    egui::Margin::same(theme.multiselect_menu_padding().value() as i8)
}

/// egui Memory에 저장하는 인스턴스별 키보드 커서의 키.
fn active_row_id(popup_id: egui::Id) -> egui::Id {
    popup_id.with("active_row")
}

/// 열린 팝업이 이번 프레임에 가져가는 키.
#[derive(Default, Clone, Copy)]
struct NavKeys {
    up: bool,
    down: bool,
    home: bool,
    end: bool,
    /// `Space` / `Enter` — active 행 토글. 팝업은 닫지 않는다.
    toggle: bool,
    /// `Esc` — 닫기.
    close: bool,
    /// `Tab` — 닫고 다음 위젯으로.
    tab: bool,
}

/// 트리거를 할당하기 전에 Space/Enter를 소비해 행 토글이 트리거 클릭으로도 처리되지 않게 한다.
/// Esc도 소비해 부모 팝업까지 닫히는 것을 막는다. Tab은 egui가 포커스를 옮기도록 남긴다.
fn take_nav_keys(ui: &egui::Ui) -> NavKeys {
    let none = egui::Modifiers::NONE;
    ui.input_mut(|i| NavKeys {
        up: i.consume_key(none, egui::Key::ArrowUp),
        down: i.consume_key(none, egui::Key::ArrowDown),
        home: i.consume_key(none, egui::Key::Home),
        end: i.consume_key(none, egui::Key::End),
        // `|` 는 단락 평가가 없다 — 둘 다 반드시 소비해야 한 쪽이 남아 클릭으로 새지 않는다.
        toggle: i.consume_key(none, egui::Key::Space) | i.consume_key(none, egui::Key::Enter),
        close: i.consume_key(none, egui::Key::Escape),
        tab: i.key_pressed(egui::Key::Tab),
    })
}

/// 팝업 안을 누르면 트리거 포커스를 되찾아 키보드 조작을 이어갈 수 있도록 감지한다.
fn pressed_inside_popup(ui: &egui::Ui, popup_id: egui::Id) -> bool {
    let Some(rect) = ui.memory(|m| m.area_rect(popup_id)) else {
        return false;
    };
    ui.input(|i| {
        i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
    })
}

/// 실제로 그려지는 행 개수 — `options` 와 `selected` 중 짧은 쪽이 목록을 끊는다.
fn row_count(selected: &[bool], options: &[&str]) -> usize {
    options.len().min(selected.len())
}

/// 활성 행이 모두 선택됐는지 확인한다. 비활성 행은 전체 선택·해제 판단에서 제외한다.
fn all_rows_on(selected: &[bool], options: &[&str], disabled: Option<&[bool]>) -> bool {
    let n = row_count(selected, options);
    let mut any = false;
    for (i, on) in selected.iter().enumerate().take(n) {
        if !row_enabled(disabled, i) {
            continue;
        }
        any = true;
        if !on {
            return false;
        }
    }
    any
}

/// 옵션 행과 같은 높이·좌우 여백의 일괄 토글 액션. 부분 선택 체크박스 대신 텍스트 액션으로 표시한다.
fn all_toggle_row(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    let body = theme.font_size_body.value();
    let pad = theme.multiselect_row_padding_x().value();
    let width = ui.available_width();
    // 옵션 행과 같은 규칙으로 말줄임 — 메뉴 폭이 max-width 에 걸려도 보더를 넘지 않는다.
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(body),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((width - pad * 2.0).max(0.0));
    let galley = ui.fonts(|f| f.layout_job(job));
    let height = theme.multiselect_row_height().value();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            theme.menu_item_radius().value(),
            theme.multiselect_row_bg_hover().to_egui_premultiplied(),
        );
    }
    let pos = egui::pos2(
        rect.left() + pad,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter()
        .galley(pos, galley, theme.multiselect_all_fg().to_egui());
    resp
}

/// 옵션 한 행. 높이·좌우 여백은 multiselect-row 토큰이고, 포인터가 올라간 행 전체에 hover 채움,
/// 키보드 활성 행에 active 채움을 깐다. 체크박스 밖의 행 여백을 눌러도 그 행만 토글한다.
/// 반환값은 (행 응답, 선택이 바뀌었는지)다.
fn option_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    flag: &mut bool,
    label: &str,
    enabled: bool,
    active: bool,
) -> (egui::Response, bool) {
    let pad = theme.multiselect_row_padding_x().value();
    let height = theme.multiselect_row_height().value();
    let width = ui.available_width();
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    // 체크박스가 행 위에 놓여 행 응답의 hovered가 꺼질 수 있으므로 포인터 위치로 판정한다.
    let hovered = enabled && ui.rect_contains_pointer(rect);
    let fill = if active {
        Some(theme.multiselect_row_bg_active().to_egui())
    } else if hovered {
        Some(theme.multiselect_row_bg_hover().to_egui_premultiplied())
    } else {
        None
    };
    if let Some(fill) = fill {
        ui.painter()
            .rect_filled(rect, theme.menu_item_radius().value(), fill);
    }
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad, rect.top()),
        egui::pos2(rect.right() - pad, rect.bottom()),
    );
    let mut slot = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let label_fg = if active {
        theme.multiselect_row_fg_active()
    } else if hovered {
        theme.text_on_active()
    } else {
        theme.multiselect_row_fg()
    };
    let ground = if active {
        crate::Ground::Active
    } else {
        crate::Ground::Raised
    };
    let check = crate::checkbox_with_label_color(
        &mut slot,
        theme,
        flag,
        label,
        enabled,
        ground,
        label_fg.to_egui(),
    );
    let mut changed = check.changed();
    if row.clicked() {
        *flag = !*flag;
        changed = true;
    }
    (row.union(check), changed)
}

/// 일괄 토글 라벨을 줄이지 않고 표시하는 데 필요한 폭.
fn all_toggle_width(ui: &egui::Ui, theme: &Theme, label: &str) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(theme.font_size_body.value()),
            egui::Color32::PLACEHOLDER,
        )
    })
    .rect
    .width()
}

/// 일괄 토글 아래의 구분선. 파생 separator 색은 premultiplied 표현 그대로 그린다.
pub(crate) fn all_toggle_separator(ui: &mut egui::Ui, theme: &Theme) {
    list_separator(ui, theme, theme.separator.to_egui_premultiplied());
}

/// 목록 안 1px 구분선. 행 사이 간격이 0이므로 위아래 `space-xs` 여백을 직접 둔다.
pub(crate) fn list_separator(ui: &mut egui::Ui, theme: &Theme, color: egui::Color32) {
    let bw = theme.border_width.value();
    let margin = theme.spacing_xs.value();
    ui.add_space(margin);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), bw), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
    ui.add_space(margin);
}

/// 다중 선택 상태가 바뀌면 true를 반환한다. selected와 options의 길이는 같아야 하며 짧은 쪽까지만 그린다.
/// disabled는 행별 마스크다. true인 행은 보이지만 변경할 수 없고, 없는 인덱스는 활성으로 본다.
/// enabled는 컨트롤 전체의 활성 여부다. 일괄 토글도 비활성 행을 변경하지 않는다.
/// all_toggle이 Some이면 스크롤 목록 위에 고정된 전체 선택/해제 행을 표시한다.
///
/// 포커스된 트리거에서 ↓/Enter/Space로 열고 ↑/↓/Home/End로 활성 행을 고른다.
/// Space/Enter는 메뉴를 닫지 않고 선택을 바꾼다. Esc는 트리거 포커스를 유지한 채 닫고,
/// Tab은 메뉴를 닫고 다음 위젯으로 이동한다. 열 때마다 키보드 커서는 초기화한다.
#[allow(clippy::too_many_arguments)] // reason: select 와 대칭인 시그니처 + labels 주입. 인위적 그룹핑 불필요
pub fn multi_select(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    selected: &mut [bool],
    options: &[&str],
    disabled: Option<&[bool]>,
    labels: &MultiSelectLabels<'_>,
    all_toggle: Option<MultiSelectAllToggle<'_>>,
    width: f32,
    enabled: bool,
) -> bool {
    let pad_x = theme.select_padding_x().value();
    let body = theme.select_font_size().value();
    let chevron_room = theme.select_chevron_room().value();

    let popup_id = multi_select_popup_id(ui, id_salt);
    let mut open = ui.memory(|m| m.is_popup_open(popup_id));

    // 트리거 할당 전에 열린 팝업의 키를 먼저 소비한다.
    let nav = if open {
        take_nav_keys(ui)
    } else {
        NavKeys::default()
    };

    let active_id = active_row_id(popup_id);
    let mut active: Option<usize> = ui
        .data(|d| d.get_temp::<Option<usize>>(active_id))
        .flatten();
    let mut changed = false;
    // 키로 이동할 때만 선택 행을 따라 스크롤한다. 마우스 스크롤 중에는 위치를 바꾸지 않는다.
    let mut scroll_to_active = false;
    if open {
        let n = row_count(selected, options);
        // 목록·마스크는 프레임마다 바뀔 수 있다 — 갈 곳을 잃은 커서는 버린다.
        active = active.filter(|i| *i < n && row_enabled(disabled, *i));
        for (pressed, forward) in [(nav.down, true), (nav.up, false)] {
            if pressed {
                active = step_active(active, n, disabled, forward);
                scroll_to_active = true;
            }
        }
        if nav.home || nav.end {
            active = edge_enabled(n, disabled, nav.home);
            scroll_to_active = true;
        }
        // 이번 프레임의 요약 문구에 반영되도록 트리거보다 먼저 선택을 바꾼다.
        if nav.toggle
            && let Some(flag) = active.and_then(|i| selected.get_mut(i))
        {
            *flag = !*flag;
            changed = true;
        }
        if nav.close || nav.tab {
            // Tab은 메뉴만 닫고 egui의 포커스 이동에 남긴다.
            ui.memory_mut(|m| m.close_popup());
            open = false;
        }
    }

    let (rect, resp) = alloc_trigger(ui, theme, width, enabled);
    // disabled 글자·글리프는 opacity 없이 disabled ink를 쓴다.
    let dim = |c: egui::Color32| {
        if enabled {
            c
        } else {
            theme.state_disabled_fg().to_egui()
        }
    };

    // 열린 상태뿐 아니라 키보드 포커스가 있을 때도 포커스 테두리를 표시한다.
    let border = if !enabled {
        theme.select_border()
    } else if open || resp.has_focus() {
        theme.select_border_focus()
    } else if resp.hovered() {
        theme.border_strong()
    } else {
        theme.select_border()
    };
    paint_trigger_box(
        ui.painter(),
        theme,
        crate::Ground::Panel,
        rect,
        border,
        enabled,
    );

    // 요약이 테두리나 화살표를 넘지 않게 줄이며 선택이 없으면 placeholder 색을 쓴다.
    let summary = multi_select_summary(labels, selected);
    let none_selected = !selected.iter().any(|s| *s);
    let fg = if none_selected {
        theme.text_placeholder()
    } else {
        theme.select_fg()
    };
    let text_max_width = (rect.right() - chevron_room - (rect.left() + pad_x)).max(0.0);
    let mut job = egui::text::LayoutJob::simple_singleline(
        summary,
        egui::FontId::proportional(body),
        egui::Color32::PLACEHOLDER,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_max_width);
    let galley = ui.fonts(|f| f.layout_job(job));
    let text_pos = egui::pos2(
        rect.left() + pad_x,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(text_pos, galley, dim(fg.to_egui()));

    // chevron — 열려 있으면 위를 향한다(단일 select 는 네이티브 미러라 항상 아래).
    let cx = rect.right() - chevron_room * 0.5;
    let ch = dim(theme.select_chevron_fg().to_egui());
    paint_chevron(ui.painter(), egui::pos2(cx, rect.center().y), ch, open);

    // 닫힌 트리거에서 `↓` 는 "열기" 다. `Enter`/`Space` 는 egui 가 클릭으로 승격시켜
    // (fake primary click) 아래 `resp.clicked()` 로 들어오므로 따로 읽지 않는다.
    let open_by_key = enabled
        && !open
        && resp.has_focus()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));

    if (enabled && resp.clicked()) || open_by_key {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
        if ui.memory(|m| m.is_popup_open(popup_id)) {
            open = true;
            active = None;
            // 클릭 후에도 키보드 조작을 이어갈 수 있도록 포커스를 가져온다.
            resp.request_focus();
        } else {
            open = false;
        }
    }

    // 포커스 필터는 이번 프레임의 최종 열림 상태를 사용한다.
    // Esc는 열렸을 때만 소비하고 닫힌 상태에서는 상위 화면에 전달한다.
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    // `Tab` 은 넘긴다 — 팝업만 닫고 포커스는 다음 위젯으로 가야 한다.
                    tab: false,
                    horizontal_arrows: false,
                    vertical_arrows: true,
                    escape: open,
                },
            );
        });
    }

    // 이전 프레임의 폭과 말줄임 결과가 서로 영향을 주지 않도록 필요한 폭을 먼저 측정한다.
    // 최소 폭은 트리거 폭이며 내용에 맞춰 상한까지 늘린다. 트리거가 상한보다 넓으면 트리거 폭을 따른다.
    let widest_option = options
        .iter()
        .take(selected.len())
        .map(|opt| crate::checkbox_width(ui, theme, opt))
        .fold(0.0_f32, f32::max);
    // 선택/해제 문구가 바뀌어도 폭이 흔들리지 않도록 둘 중 넓은 쪽을 사용한다.
    let widest_all_toggle = all_toggle.map_or(0.0, |t| {
        all_toggle_width(ui, theme, t.select_all).max(all_toggle_width(ui, theme, t.clear_all))
    });
    let widest_row =
        widest_option.max(widest_all_toggle) + theme.multiselect_row_padding_x().value() * 2.0;
    let menu_chrome = popup_chrome_width(theme);
    let menu_min = width;
    let menu_max = (theme.multiselect_menu_max_width().value() - menu_chrome).max(menu_min);
    let menu_width = widest_row.clamp(menu_min, menu_max);

    // 여백·그림자 style만 바꾼 자식 Ui로 팝업을 띄운다. 자식은 부모 레이아웃에 자리를 차지하지 않는다.
    let mut menu_parent = ui.new_child(egui::UiBuilder::new().max_rect(resp.rect));
    menu_parent.set_style(menu_style(ui.style(), theme));
    // 트리거와 메뉴 사이 `multiselect-menu-gap`. 팝업은 기준 rect 바로 아래에 붙으므로 기준만 내린다.
    let mut anchor = resp.clone();
    anchor.rect = resp
        .rect
        .translate(egui::vec2(0.0, theme.multiselect_menu_gap().value()));
    tasty_egui_theme::with_popover_frame_ring(&mut menu_parent, theme, menu_margin(theme), |ui| {
        egui::popup_below_widget(
            ui,
            popup_id,
            &anchor,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(menu_width);
                ui.set_max_width(menu_width);
                // 행은 간격 없이 쌓는다. 일괄 토글 행은 스크롤 밖에 고정한다.
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(t) = all_toggle {
                    let all_on = all_rows_on(selected, options, disabled);
                    let label = if all_on { t.clear_all } else { t.select_all };
                    if all_toggle_row(ui, theme, label).clicked() {
                        // 전부 켜져 있으면 끄고, 아니면 켠다. 비활성 행은 어느 쪽이든 그대로.
                        let n = row_count(selected, options);
                        for (i, flag) in selected.iter_mut().enumerate().take(n) {
                            if row_enabled(disabled, i) && *flag == all_on {
                                *flag = !all_on;
                                changed = true;
                            }
                        }
                    }
                    all_toggle_separator(ui, theme);
                }
                // 긴 목록만 내부에서 스크롤한다.
                egui::ScrollArea::vertical()
                    .id_salt(("tasty_multi_select_list", id_salt))
                    .max_height(theme.multiselect_menu_max_height().value())
                    .auto_shrink([true, true])
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        for (i, opt) in options.iter().enumerate() {
                            let Some(flag) = selected.get_mut(i) else {
                                break;
                            };
                            let (resp, row_changed) = option_row(
                                ui,
                                theme,
                                flag,
                                opt,
                                row_enabled(disabled, i),
                                active == Some(i),
                            );
                            if row_changed {
                                changed = true;
                            }
                            if active == Some(i) && scroll_to_active {
                                ui.scroll_to_rect(resp.rect, None);
                            }
                        }
                    });
            },
        )
    });

    // 팝업 내부 클릭으로 잃은 포커스를 되찾아 키보드 조작을 이어간다.
    if open && pressed_inside_popup(ui, popup_id) {
        resp.request_focus();
    }
    ui.data_mut(|d| d.insert_temp(active_id, active));
    changed
}
