#![forbid(unsafe_code)]

//! `tasty-ui-widgets` — 본체와 갤러리가 공유하는 egui layout / 위젯 primitive.
//!
//! `tasty-egui-theme` (색·폰트·spacing 토큰) 위에 *layout idiom* (frame + sub-tab 패널 등)
//! 을 얹는다. 본체 settings/plugins 와 갤러리가 동일 함수를 호출 → 시각 100% 동기화.
//!
//! 글로벌 `theme()` 호출 금지 — 모든 함수는 `&Theme` 을 명시적으로 받는다.
//! 본체 (`tasty`) 미의존 — 이 crate 는 본체 state 를 모른다.
//!
//! ## 본체 호출자가 없는 위젯이 여기 있는 것은 정상이다
//!
//! 이 crate 에는 갤러리 specimen 만 부르고 본체 호출자가 아직 없는 함수가 있다. 그것은
//! 치우다 만 것이 아니라 두 정책이 함께 그린 상태다:
//!
//! - `docs/design/policies/shared-widgets.md` — 보편 이름이 붙는 컴포넌트는 **사용처가
//!   한 곳뿐이어도** 인라인이 아니라 여기 산다. 소속 판정 기준이 사용처 개수가 아니라
//!   이름이므로, 호출자 수는 애초에 그 판정에 들어가지 않는다.
//! - `docs/dev-guide/gallery-first.md` · `docs/adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md`
//!   — 새 컴포넌트는 갤러리 specimen 을 먼저 만들고 그다음 본체에 반영한다. 그래서 **본체
//!   호출자 0 은 절차의 정상 중간 지점**이다. ADR-0510 이 요구하는 포함 방향도 한쪽이다
//!   (본체 ⊆ 갤러리) — 갤러리에만 있는 것은 그 요구를 어기지 않는다.
//!
//! 결함인 것은 호출자 0 자체가 아니라 **본체가 같은 것을 손으로 다시 그리는 것**이다.
//! 형상이 두 벌이면 갈린다 — 가장 먼저 갈리는 축은 배율이다(토큰은 `ui_zoom` 을 타고
//! 손으로 박은 상수는 안 탄다). 그런 자리를 찾으면 위젯 호출로 바꾼다.

mod autocomplete;
pub mod brand;
mod button;
mod chip;
mod chrome_slot;
mod control;
pub mod crumb_alloc;
mod drilldown;
pub mod file_handler;
mod help_hint;
mod horizontal_tab_bar;
mod icon_button;
mod input;
mod keyboard_cursor;
mod language_select;
mod listctrl;
mod menu_item;
mod multi_select;
mod path_field;
mod plugin_avatar;
mod remote_tool;
mod segmented;
mod select;
mod spacing;
mod spinner;
mod status_bar;
mod status_dot;
mod tab_content_frame;
mod table;
mod toast;
mod toggle;
pub mod tokens;
mod tooltip;
mod tree_row;
mod two_depth;
mod warning_callout;
pub use autocomplete::{
    AutoComplete, AutoCompleteAction, AutoCompleteResponse, MatchMode, autocomplete_dropdown,
};
pub use button::{Button, ButtonVariant};
pub use chip::{
    BadgeVariant, KbdKey, TagVariant, badge, badge_dot, kbd, kbd_parts, kbd_parts_at,
    kbd_parts_width, kbd_width, num_keycap, paint_badge_dot, paint_num_keycap, tag, tag_width,
};
pub use chrome_slot::top_right_inset_square;
pub use control::ControlSize;
pub use drilldown::{DrillDown, DrillDownActions, DrillDownOutput, DrillDownView};
pub use help_hint::HelpHint;
pub use horizontal_tab_bar::horizontal_tab_bar_with_arrows;
pub use icon_button::{IconButton, IconButtonVariant, IconPainter};
pub use input::Input;
pub use language_select::{LanguageOption, LanguageSelectLabels, language_select};
pub use listctrl::{ListCtrl, ListCtrlItem, ListCtrlOutput, ListCtrlTrailing};
pub use menu_item::{MenuItemVariant, menu_item, menu_item_kbd, menu_separator};
pub use multi_select::{
    MultiSelectAllToggle, MultiSelectLabels, multi_select, multi_select_popup_id,
    multi_select_summary, popup_chrome_width,
};
pub use path_field::{PathField, PathFieldOutcome};
pub use plugin_avatar::{PluginAvatarSize, paint_plugin_avatar, plugin_avatar};
pub use remote_tool::{
    FILTER_DROPDOWN_MAX_HEIGHT, FILTER_DROPDOWN_MIN_WIDTH, LocalSshHost, LocalSshSectionData,
    ProtocolFilterItem, ProtocolFilterLabels, TabStripData, TextWrap, draw_local_ssh_section,
    draw_protocol_filter_body, draw_protocol_filter_button, draw_tab_strip, ghost_button, hsep,
    primary_button, secondary_button, selectable_label, selectable_text, warn_badge,
};
pub use segmented::segmented;
pub use select::{select, select_or_placeholder};
pub use spacing::{hspace, margin_all, margin_sym, vspace};
pub use spinner::Spinner;
pub use status_bar::{StatusBarAction, StatusBarData, StatusBarDrawResult, draw_status_bar_view};
pub use status_dot::{StatusKind, status_dot};
pub use tab_content_frame::{settings_content_column, tab_content_frame};
pub use table::{Table, TableAlign, TableColumn, TableColumnWidth, TableOutput, TableSortDir};
pub use toast::{
    CardColors as ToastCardColors, FADE_IN_MS as TOAST_FADE_IN_MS,
    FADE_OUT_MS as TOAST_FADE_OUT_MS, ToastEntryView, ToastScopeView, ToastViewProps,
    accent_color as toast_accent_color, card_colors as toast_card_colors,
    draw_card as draw_toast_card, draw_single_card as draw_toast_single_card, draw_toast_scopes,
    fade_alpha as toast_fade_alpha, layout_card as toast_layout_card,
};
pub use toggle::{checkbox, checkbox_width, switch};
pub use tooltip::{Tooltip, TooltipPlacement};
pub use tree_row::tree_row;
pub use two_depth::{two_depth_layout, two_depth_layout_filtered};
pub use warning_callout::warning_callout;
