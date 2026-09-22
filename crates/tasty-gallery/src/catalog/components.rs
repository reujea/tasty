//! Tier 3 components — popup / sidebar / tab_bar 등의 props-분리된 view 데모.
//!
//! 각 컴포넌트는 본체 (`crate tasty`) 의 view 함수 시그니처
//! `fn draw_xxx_view(ui, theme, &XxxProps) -> XxxAction` 와 동일한 형태를
//! 로컬에 재현한다. 갤러리는 본체 binary 에 직접 의존할 수 없어 props 타입과
//! 시각 layout 을 *복제* 한다 — 본체 update 시 시각 동등성은 수동 검증.
//!
//! **예외 — view 가 공용 crate 로 올라간 컴포넌트는 복제하지 않는다.** 본체와
//! 갤러리가 `tasty-ui-widgets` 의 **같은 함수**를 호출하므로 시각이 자동 동기화되고,
//! specimen 은 표시 데이터(props)만 준다. 현재 이 경로: [`status_bar`]
//! (`tasty_ui_widgets::draw_status_bar_view`). 새 bar/패널을 추가할 때는 복제보다
//! 이 경로를 우선한다 — `docs/dev-guide/gallery-first.md`.

pub mod apply_preset;
pub mod approval;
pub mod category_dialogs;
pub mod clipboard_viewer;
pub mod command_palette;
pub mod convert;
pub mod dag;
pub mod drop_overlay;
pub mod empty_surface;
pub mod explorer_context_menu;
pub mod explorer_favorite_popup;
pub mod explorer_rename_popup;
pub mod explorer_sidebar;
pub mod explorer_tab_bar;
pub mod explorer_toolbar;
pub mod explorer_view_cells;
pub mod file_handler_picker;
pub mod file_picker;
pub mod fullscreen_stage;
pub mod git_viewer;
pub mod glyph;
pub mod html_chrome;
pub mod image_viewer;
pub mod info_modal;
pub mod kb_import_export;
pub mod markdown_open;
pub mod markdown_viewer;
pub mod md_large_file;
pub mod modifier_hint;
pub mod notification_panel;
pub mod occupancy_borders;
pub mod plugin_settings;
pub mod plugins_window;
pub mod port_scanner;
pub mod preset_editor;
pub mod preset_surface_settings;
pub mod prim_autocomplete;
pub mod prim_button;
pub mod prim_chips;
pub mod prim_drilldown;
pub mod prim_forms;
pub mod prim_help_hint;
pub mod prim_icon_button;
pub mod prim_input;
pub mod prim_layout_shell;
pub mod prim_listctrl;
pub mod prim_nav;
pub mod prim_path_field;
pub mod prim_spinner;
pub mod prim_status_dot;
pub mod prim_status_resolution;
pub mod prim_tab;
pub mod prim_table;
pub mod quit_modal;
pub mod remote;
pub mod remote_attach;
pub mod rename_popup;
pub mod scrim_scope;
pub mod script_confirm;
pub mod script_manager;
pub mod search_bar;
pub mod segmented;
pub mod settings;
pub mod settings_handler;
pub mod settings_number;
pub mod settings_remote_transfer;
pub mod sidebar;
pub mod status_bar;
pub mod surface_highlights;
pub mod switch_overlay;
pub mod tab_bar;
pub mod titlebar;
pub mod toast;
pub mod tools_menu;
pub mod transfer;
