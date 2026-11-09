//! Theme를 통해 component 치수·색·시간을 읽는 접근자를 생성한다.
//! 치수에는 UI 배율을 적용하고 시간에는 적용하지 않는다.
//! 배율을 적용하지 않은 상수는 `super`에서 생성한다.

use super::{
    ThemeMode, Tier, Token, TokenSet, alias_target,
    duration_accessor::{emit_duration_accessor, resolve_duration_accessor},
};

/// 토큰 경로와 Theme/SIZING 필드의 대응표. 치수 검사도 이 표를 사용한다.
/// 같은 토큰에 필드가 여럿이면 첫 항목을 선택하므로 배율이 적용되는 위젯 필드를
/// 배율에서 제외되는 창 UI 필드보다 앞에 둔다.
pub const SEMANTIC_DIM_TO_THEME_FIELD: &[(&str, &str)] = &[
    // spacing (4px 그리드 5단)
    ("semantic.space-xs", "spacing_xs"),
    ("semantic.space-sm", "spacing_sm"),
    ("semantic.space-md", "spacing_md"),
    ("semantic.space-lg", "spacing_lg"),
    ("semantic.space-xl", "spacing_xl"),
    // 보더/라운드
    ("semantic.border-width", "border_width"),
    ("semantic.focus-ring-width", "focus_ring_width"),
    ("semantic.radius", "corner_radius"),
    ("semantic.radius-sm", "corner_radius_sm"),
    // 컨트롤 높이 / 탭
    ("semantic.control-height-tree", "item_height_tree"),
    ("semantic.control-height", "item_height_interactive"),
    ("semantic.control-height-tab", "item_height_tab"),
    ("semantic.tab-width", "tab_width"),
    // 타이포
    ("semantic.font-size-micro", "font_size_micro"),
    ("semantic.font-size-caption", "font_size_caption"),
    ("semantic.font-size-body", "font_size_body"),
    ("semantic.font-size-heading", "font_size_heading"),
    ("semantic.font-size-max", "font_size_max"),
    (
        "semantic.font-size-brand-display",
        "font_size_brand_display",
    ),
    ("semantic.font-size-prose-h1", "font_size_prose_h1"),
    ("semantic.font-size-term-sm", "font_size_term_sm"),
    ("semantic.font-size-term", "font_size_term"),
    ("semantic.font-size-term-lg", "font_size_term_lg"),
    // 아이콘 글리프
    ("semantic.icon-size-xs", "icon_glyph_size_xs"),
    ("semantic.icon-size-sm", "icon_glyph_size_sm"),
    ("semantic.icon-size-md", "icon_glyph_size_md"),
    ("semantic.icon-size-lg", "icon_glyph_size_lg"),
    // 가독 폭 / form-control 폭
    ("semantic.measure-sm", "measure_sm"),
    ("semantic.measure-md", "measure_md"),
    ("semantic.measure-lg", "measure_lg"),
    ("semantic.measure-xl", "measure_xl"),
    ("semantic.field-width-xs", "field_width_xs"),
    ("semantic.field-width-color", "field_width_color"),
    ("semantic.field-width-md", "field_width_md"),
    ("semantic.field-width-lg", "field_width_lg"),
    // 세부 치수 (semantic)
    ("semantic.status-bar-height", "status_bar_height"),
    ("semantic.toolbar-height", "toolbar_height"),
    ("semantic.titlebar-height", "titlebar_height"),
    ("semantic.overlay-top-offset", "overlay_top_offset"),
    ("semantic.label-detail-gap", "label_detail_gap"),
    ("semantic.selection-edge-width", "selection_edge_width"),
    ("semantic.tint-edge-width", "tint_edge_width"),
    ("semantic.border-dash", "border_dash"),
    ("semantic.border-dash-gap", "border_dash_gap"),
    // component 전용 필드 — 사이드바 (host UI zoom 영향 받음)
    ("component.sidebar-logo-size", "sidebar_logo_size"),
    (
        "component.sidebar-logo-collapsed-size",
        "sidebar_logo_collapsed_size",
    ),
    (
        "component.sidebar-wordmark-font-size",
        "sidebar_wordmark_font_size",
    ),
    (
        "component.sidebar-section-heading-font-size",
        "sidebar_section_heading_font_size",
    ),
    (
        "component.sidebar-button-label-font-size",
        "sidebar_button_label_font_size",
    ),
    (
        "component.sidebar-collapsed-slot-width",
        "sidebar_collapsed_slot_width",
    ),
    (
        "component.sidebar-collapsed-icon-height",
        "sidebar_collapsed_icon_height",
    ),
    (
        "component.sidebar-collapsed-workspace-height",
        "sidebar_collapsed_workspace_height",
    ),
    // component 전용 필드 — titlebar / toast / status-dot / spinner / tab (zoom 제외 또는 고정)
    ("component.titlebar-traffic-size", "traffic_size"),
    ("component.titlebar-caption-width", "caption_width"),
    (
        "component.titlebar-window-button-size",
        "window_button_size",
    ),
    ("component.toast-max-width", "toast_max_width"),
    ("component.toast-accent-width", "toast_accent_width"),
    ("component.status-dot-size", "status_dot_size"),
    ("component.status-dot-ring-width", "status_dot_ring_width"),
    ("component.spinner-size", "spinner_size"),
    // 위의 배율 적용 필드가 먼저 선택된다. 아래 필드는 치수 검사에 필요하다.
    ("semantic.control-height-tab", "tab_bar_height"),
    ("semantic.font-size-body", "tab_bar_label_font_size"),
];

/// semantic 색 토큰과 Theme 필드·메서드의 대응표. `self.<expr>`로 생성한다.
/// 대응하는 접근자가 없는 토큰은 로그를 남기고 제외한다.
pub const SEMANTIC_COLOR_TO_THEME_ACCESSOR: &[(&str, &str)] = &[
    ("semantic.accent-agent", "accent_agent()"),
    ("semantic.accent-attached", "border_attached()"),
    ("semantic.accent-attention", "accent_attention()"),
    ("semantic.accent-danger", "accent_danger()"),
    ("semantic.accent-danger-raised", "accent_danger_raised()"),
    ("semantic.accent-data", "accent_data()"),
    ("semantic.accent-data-strong", "accent_data_strong()"),
    ("semantic.accent-decorative", "accent_decorative()"),
    ("semantic.accent-info", "accent_info()"),
    ("semantic.accent-macos-close", "accent_macos_close()"),
    ("semantic.accent-macos-min", "accent_macos_min()"),
    ("semantic.accent-macos-zoom", "accent_macos_zoom()"),
    ("semantic.accent-move", "accent_move()"),
    ("semantic.accent-occupied-hard", "accent_occupied_hard()"),
    ("semantic.accent-occupied-soft", "accent_occupied_soft()"),
    ("semantic.accent-primary", "accent_primary()"),
    ("semantic.accent-remote", "accent_remote()"),
    ("semantic.accent-route", "accent_route()"),
    ("semantic.accent-success", "accent_success()"),
    ("semantic.accent-success-ink", "accent_success_ink()"),
    ("semantic.accent-warning", "accent_warning()"),
    ("semantic.accent-warning-glyph", "accent_warning_glyph()"),
    ("semantic.accent-window-close", "accent_window_close()"),
    ("semantic.notify-error", "accent_danger()"),
    ("semantic.notify-warning", "accent_warning()"),
    ("semantic.notify-warning-fill", "notify_warning_fill()"),
    ("semantic.notify-info", "attention_completion()"),
    ("semantic.notify-error-fg", "text_on_accent()"),
    ("semantic.notify-warning-fg", "notify_warning_fg()"),
    ("semantic.notify-info-fg", "text_on_accent()"),
    ("semantic.notify-unread", "accent_primary()"),
    ("semantic.attention-completion", "attention_completion()"),
    (
        "semantic.attention-completion-fg",
        "attention_completion_fg()",
    ),
    ("semantic.attention-needs-input", "attention_needs_input()"),
    (
        "semantic.attention-needs-input-fg",
        "attention_needs_input_fg()",
    ),
    ("semantic.bg-app", "bg_app()"),
    ("semantic.bg-panel", "bg_panel()"),
    ("semantic.bg-sidebar", "bg_sidebar()"),
    ("semantic.border-default", "border_default()"),
    ("semantic.border-focus", "border_focus()"),
    ("semantic.border-frame", "border_frame()"),
    ("semantic.border-strong", "border_strong()"),
    ("semantic.glyph-dim", "glyph_dim()"),
    ("semantic.overlay-active", "overlay_active()"),
    ("semantic.overlay-hover", "overlay_hover()"),
    ("semantic.separator", "separator"),
    ("semantic.state-disabled-border", "state_disabled_border()"),
    ("semantic.state-disabled-fg", "state_disabled_fg()"),
    ("semantic.state-disabled-fill", "state_disabled_fill()"),
    (
        "semantic.state-disabled-fill-raised",
        "state_disabled_fill_raised()",
    ),
    (
        "semantic.state-disabled-border-raised",
        "state_disabled_border_raised()",
    ),
    ("semantic.status-idle", "status_idle()"),
    ("semantic.surface-active", "surface_active()"),
    ("semantic.surface-hover", "surface_hover()"),
    ("semantic.surface-raised", "surface_raised()"),
    ("semantic.text-disabled", "text_disabled()"),
    ("semantic.text-muted", "text_muted()"),
    ("semantic.text-on-accent", "text_on_accent()"),
    ("semantic.text-on-window-close", "text_on_window_close()"),
    ("semantic.text-placeholder", "text_placeholder()"),
    ("semantic.text-primary", "text_primary()"),
    ("semantic.text-secondary", "text_secondary()"),
    ("semantic.text-secondary-raised", "text_secondary_raised()"),
    ("semantic.text-muted-raised", "text_muted_raised()"),
    ("semantic.surface-on-raised", "surface_on_raised()"),
    ("semantic.text-on-active", "text_on_active()"),
    (
        "semantic.accent-primary-on-active",
        "accent_primary_on_active()",
    ),
    ("semantic.accent-warning-raised", "accent_warning_raised()"),
    (
        "semantic.search-match-active-unfocused-fg",
        "search_match_active_unfocused_fg()",
    ),
    (
        "semantic.search-match-selected-fg",
        "search_match_selected_fg()",
    ),
    (
        "semantic.search-match-active-selected-bg",
        "search_match_active_selected_bg()",
    ),
    ("semantic.surface-bed-focused", "surface_bed_focused()"),
    ("semantic.surface-bed-unfocused", "surface_bed_unfocused()"),
    (
        "semantic.accent-attention-raised",
        "accent_attention_raised()",
    ),
    ("semantic.accent-info-raised", "accent_info_raised()"),
    (
        "semantic.accent-warning-on-active",
        "accent_warning_on_active()",
    ),
    (
        "semantic.accent-success-on-active",
        "accent_success_on_active()",
    ),
    (
        "semantic.accent-danger-on-active",
        "accent_danger_on_active()",
    ),
    (
        "semantic.accent-agent-on-active",
        "accent_agent_on_active()",
    ),
    (
        "semantic.accent-remote-on-active",
        "accent_remote_on_active()",
    ),
    (
        "semantic.accent-attached-raised",
        "accent_attached_raised()",
    ),
    (
        "semantic.accent-attached-on-active",
        "accent_attached_on_active()",
    ),
    ("semantic.surface-field", "surface_field()"),
    ("semantic.border-control", "border_control()"),
    ("semantic.border-control-raised", "border_control_raised()"),
    ("semantic.control-thumb", "control_thumb()"),
    ("semantic.control-thumb-raised", "control_thumb_raised()"),
    ("semantic.scrollbar-thumb", "scrollbar_thumb()"),
    (
        "semantic.scrollbar-thumb-raised",
        "scrollbar_thumb_raised()",
    ),
    ("semantic.scrollbar-thumb-hover", "scrollbar_thumb_hover()"),
    ("semantic.text-on-active-hover", "text_on_active_hover()"),
    ("semantic.text-quiet", "text_quiet()"),
    ("semantic.glyph-dim-raised", "glyph_dim_raised()"),
    ("semantic.accent-primary-raised", "accent_primary_raised()"),
    ("semantic.accent-agent-raised", "accent_agent_raised()"),
    ("semantic.accent-success-raised", "accent_success_raised()"),
    ("semantic.accent-remote-raised", "accent_remote_raised()"),
    ("semantic.accent-move-raised", "accent_move_raised()"),
    ("semantic.accent-route-raised", "accent_route_raised()"),
    ("semantic.accent-primary-strong", "accent_primary_strong()"),
    ("semantic.accent-info-strong", "accent_info_strong()"),
    ("semantic.accent-success-strong", "accent_success_strong()"),
    ("semantic.accent-warning-strong", "accent_warning_strong()"),
    (
        "semantic.accent-attention-strong",
        "accent_attention_strong()",
    ),
    ("semantic.accent-danger-strong", "accent_danger_strong()"),
    ("semantic.accent-agent-strong", "accent_agent_strong()"),
    (
        "semantic.accent-attached-strong",
        "accent_attached_strong()",
    ),
    ("semantic.accent-remote-strong", "accent_remote_strong()"),
    ("semantic.accent-info-on-active", "accent_info_on_active()"),
    (
        "semantic.accent-attention-on-active",
        "accent_attention_on_active()",
    ),
    ("semantic.accent-move-on-active", "accent_move_on_active()"),
    (
        "semantic.accent-route-on-active",
        "accent_route_on_active()",
    ),
    ("semantic.accent-success-fill", "accent_success_fill()"),
    (
        "semantic.attention-needs-input-fill",
        "attention_needs_input_fill()",
    ),
    ("semantic.tint-primary-bg", "tint_primary_bg()"),
    ("semantic.tint-info-bg", "tint_info_bg()"),
    ("semantic.tint-success-bg", "tint_success_bg()"),
    ("semantic.tint-warning-bg", "tint_warning_bg()"),
    ("semantic.tint-danger-bg", "tint_danger_bg()"),
    ("semantic.tint-agent-bg", "tint_agent_bg()"),
    ("semantic.vi-cursor-fg", "vi_cursor_fg()"),
];

/// 생성할 semantic 색 접근자: 토큰 경로, 메서드명, 반환할 Theme 필드.
/// 토큰과 필드가 항상 일대일로 대응하지 않으므로 명시적으로 연결한다.
/// 예를 들어 text-placeholder는 테마 파일의 placeholder 필드를 그대로 쓴다.
/// color_drift 시험은 값, semantic_accessors_map_to_primitives 시험은 필드 연결을 확인한다.
pub const SEMANTIC_COLOR_ACCESSOR_GEN: &[(&str, &str, &str)] = &[
    // 배경 (bg-*)
    ("semantic.bg-app", "bg_app", "crust"),
    ("semantic.bg-sidebar", "bg_sidebar", "mantle"),
    ("semantic.bg-panel", "bg_panel", "base"),
    // 표면 (surface-*)
    ("semantic.surface-raised", "surface_raised", "surface0"),
    ("semantic.surface-hover", "surface_hover", "surface1"),
    ("semantic.surface-active", "surface_active", "surface2"),
    // 텍스트 (text-*)
    ("semantic.text-primary", "text_primary", "text"),
    ("semantic.text-secondary", "text_secondary", "subtext1"),
    (
        "semantic.text-placeholder",
        "text_placeholder",
        "placeholder",
    ),
    // accent (의미색)
    ("semantic.accent-primary", "accent_primary", "blue"),
    ("semantic.accent-info", "accent_info", "sky"),
    // 원격 연결과 일반 안내는 기본 색이 같아도 용도를 구분한다.
    ("semantic.accent-remote", "accent_remote", "sky"),
    ("semantic.accent-success", "accent_success", "green"),
    ("semantic.accent-warning", "accent_warning", "yellow"),
    // 주의 환기(peach)와 경고(yellow)를 구분한다.
    ("semantic.accent-attention", "accent_attention", "peach"),
    // 점유 표시는 soft=green, hard=peach. 성공·주의 환기와 색이 같아도 별도 역할이다.
    (
        "semantic.accent-occupied-soft",
        "accent_occupied_soft",
        "green",
    ),
    (
        "semantic.accent-occupied-hard",
        "accent_occupied_hard",
        "peach",
    ),
    ("semantic.accent-danger", "accent_danger", "red"),
    ("semantic.accent-agent", "accent_agent", "mauve"),
    ("semantic.accent-attached", "border_attached", "lavender"),
    // 이동 대기 표시는 상태나 알림 종류가 아닌 별도 역할이다.
    ("semantic.accent-move", "accent_move", "pink"),
    // DAG 입력 binding(값 전달)과 결과로 고른 전이 경로. 원격 연결·연결 표시와 색이 같아도 역할을 구분한다.
    ("semantic.accent-data", "accent_data", "teal"),
    ("semantic.accent-route", "accent_route", "lavender"),
    // 장식과 주의 환기는 색이 같아도 역할을 구분한다.
    ("semantic.accent-decorative", "accent_decorative", "peach"),
    // 알림 종류(attention kind)는 경고·기본 강조와 색이 같아도 별도 역할이다.
    (
        "semantic.attention-needs-input",
        "attention_needs_input",
        "yellow",
    ),
    (
        "semantic.attention-completion",
        "attention_completion",
        "blue",
    ),
    // 보더 (border-*)
    ("semantic.border-default", "border_default", "surface0"),
    ("semantic.border-strong", "border_strong", "surface1"),
    ("semantic.border-focus", "border_focus", "blue"),
    // 프레임 선과 선택된 배경은 색이 같아도 역할을 구분한다.
    ("semantic.border-frame", "border_frame", "surface2"),
];

/// 단순 필드 반환이 아닌 색 접근자는 theme.rs에 직접 구현한다.
/// 터미널 전용 색이나 사용하지 않는 토큰처럼 접근자 자체가 없는 색은 이 목록에서 제외한다.
const SEMANTIC_COLOR_HAND_WRITTEN: &[(&str, &str)] = &[
    (
        "semantic.accent-success-ink",
        "Latte green 60% + primitive black 합성색",
    ),
    (
        "semantic.accent-warning-glyph",
        "Latte yellow 50% + neutral-1100 합성색",
    ),
    (
        "semantic.accent-danger-raised",
        "Latte red 60% + neutral-1100 합성색",
    ),
    (
        "semantic.notify-warning-fill",
        "Latte yellow 50% + white 합성색",
    ),
    (
        "semantic.notify-warning-fg",
        "Mocha=text-on-accent / Latte=text 역할 분기",
    ),
    (
        "semantic.text-on-accent",
        "is_light role-remap (Mocha=crust / Latte=white) — 단순 alias 아님",
    ),
    (
        "semantic.text-disabled",
        "is_light role-remap (Mocha=overlay1 / Latte=overlay2) — 단순 alias 아님",
    ),
    (
        "semantic.text-secondary-raised",
        "is_light role-remap (Mocha=subtext1 / Latte=text) — 단순 alias 아님",
    ),
    (
        "semantic.text-muted-raised",
        "is_light role-remap (Mocha=subtext0 / Latte=text) — 단순 alias 아님",
    ),
    (
        "semantic.surface-on-raised",
        "is_light role-remap (Mocha=surface-hover / Latte=base) — 단순 alias 아님",
    ),
    (
        "semantic.text-on-active",
        "Latte neutral-1100 70% + primitive black 합성색",
    ),
    (
        "semantic.accent-primary-on-active",
        "Mocha blue 65% + white / Latte blue 65% + black 합성색",
    ),
    (
        "semantic.accent-warning-raised",
        "Latte yellow 50% + primitive black 합성색",
    ),
    (
        "semantic.accent-attention-raised",
        "Latte peach 55% + primitive black 합성색",
    ),
    (
        "semantic.accent-info-raised",
        "Latte sky 55% + primitive black 합성색",
    ),
    (
        "semantic.text-muted",
        "is_light role-remap (Mocha=subtext0 / Latte=subtext1) — 단순 alias 아님",
    ),
    (
        "semantic.accent-warning-on-active",
        "Latte yellow 45% + black 합성색",
    ),
    (
        "semantic.accent-success-on-active",
        "Latte green 55% + black 합성색",
    ),
    (
        "semantic.accent-danger-on-active",
        "Mocha red 60% + white / Latte red 70% + black 합성색",
    ),
    (
        "semantic.accent-agent-on-active",
        "Mocha mauve 70% + white / Latte mauve 70% + black 합성색",
    ),
    (
        "semantic.accent-remote-on-active",
        "accent-info-on-active alias, 대응 Theme 필드 없음",
    ),
    (
        "semantic.accent-attached-raised",
        "Latte lavender 55% + primitive black 합성색",
    ),
    (
        "semantic.accent-attached-on-active",
        "Mocha lavender 85% + white / Latte lavender 50% + black 합성색",
    ),
    (
        "semantic.overlay-hover",
        "derive_overlays 도출값 (hover_overlay, primitive 아님)",
    ),
    (
        "semantic.overlay-active",
        "derive_overlays 도출값 (active_overlay, primitive 아님)",
    ),
    (
        "semantic.scrim-bg",
        "합성색 from_rgba(0,0,0,SCRIM_ALPHA) — primitive 필드 아님",
    ),
    (
        "semantic.accent-window-close",
        "OS 리터럴 const (Windows close hover, 테마 불변)",
    ),
    ("semantic.text-on-window-close", "리터럴 const (white 고정)"),
    (
        "semantic.attention-needs-input-fg",
        "notify-warning-fg alias — Mocha=text-on-accent / Latte=text",
    ),
    (
        "semantic.attention-completion-fg",
        "text-on-accent alias — is_light role-remap을 따른다",
    ),
    ("semantic.surface-field", "state_ink.rs 수기 역할"),
    (
        "semantic.accent-data-strong",
        "Latte teal 85% + primitive black 합성색",
    ),
    (
        "semantic.search-match-active-unfocused-fg",
        "Mocha=search-match-active-fg / Latte=primitive black 역할 분기",
    ),
    (
        "semantic.search-match-selected-fg",
        "Mocha=neutral-0 / Latte=primitive black 역할 분기",
    ),
    (
        "semantic.search-match-active-selected-bg",
        "Mocha=search-match-active-bg / Latte=yellow 80% 역할 분기",
    ),
    (
        "semantic.surface-bed-focused",
        "surface-terminal-focused-bg 체인, 테마의 surfaces.terminal 값",
    ),
    (
        "semantic.surface-bed-unfocused",
        "is_light role-remap (Mocha=bg-panel / Latte=bg-app) — 단순 alias 아님",
    ),
    ("semantic.border-control", "state_ink.rs 수기 역할"),
    ("semantic.border-control-raised", "state_ink.rs 수기 역할"),
    ("semantic.control-thumb", "state_ink.rs 수기 역할"),
    ("semantic.control-thumb-raised", "state_ink.rs 수기 역할"),
    ("semantic.scrollbar-thumb", "state_ink.rs 수기 역할"),
    ("semantic.scrollbar-thumb-raised", "state_ink.rs 수기 역할"),
    ("semantic.scrollbar-thumb-hover", "state_ink.rs 수기 역할"),
    ("semantic.text-on-active-hover", "state_ink.rs 수기 역할"),
    ("semantic.text-quiet", "state_ink.rs 수기 역할"),
    ("semantic.glyph-dim", "state_ink.rs 수기 역할"),
    ("semantic.glyph-dim-raised", "state_ink.rs 수기 역할"),
    ("semantic.status-idle", "state_ink.rs 수기 역할"),
    ("semantic.accent-primary-raised", "state_ink.rs 수기 역할"),
    ("semantic.accent-agent-raised", "state_ink.rs 수기 역할"),
    ("semantic.accent-success-raised", "state_ink.rs 수기 역할"),
    ("semantic.accent-remote-raised", "state_ink.rs 수기 역할"),
    ("semantic.accent-move-raised", "state_ink.rs 수기 역할"),
    ("semantic.accent-route-raised", "state_ink.rs 수기 역할"),
    ("semantic.accent-primary-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-info-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-success-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-warning-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-attention-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-danger-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-agent-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-attached-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-remote-strong", "state_ink.rs 수기 역할"),
    ("semantic.accent-info-on-active", "state_ink.rs 수기 역할"),
    (
        "semantic.accent-attention-on-active",
        "state_ink.rs 수기 역할",
    ),
    ("semantic.accent-move-on-active", "state_ink.rs 수기 역할"),
    ("semantic.accent-route-on-active", "state_ink.rs 수기 역할"),
    ("semantic.accent-success-fill", "state_ink.rs 수기 역할"),
    (
        "semantic.attention-needs-input-fill",
        "state_ink.rs 수기 역할",
    ),
    ("semantic.tint-primary-bg", "state_ink.rs 수기 역할"),
    ("semantic.tint-info-bg", "state_ink.rs 수기 역할"),
    ("semantic.tint-success-bg", "state_ink.rs 수기 역할"),
    ("semantic.tint-warning-bg", "state_ink.rs 수기 역할"),
    ("semantic.tint-danger-bg", "state_ink.rs 수기 역할"),
    ("semantic.tint-agent-bg", "state_ink.rs 수기 역할"),
    ("semantic.vi-cursor-fg", "state_ink.rs 수기 역할"),
    ("semantic.accent-macos-close", "OS 리터럴 const"),
    ("semantic.accent-macos-min", "OS 리터럴 const"),
    ("semantic.accent-macos-zoom", "OS 리터럴 const"),
    (
        "semantic.brand-melon-flesh",
        "브랜드 리터럴 const (테마 불변)",
    ),
];

/// theme.rs에 직접 구현한 색 접근자. 같은 이름으로 생성하면 중복 정의가 된다.
const EXISTING_THEME_ACCESSOR_NAMES: &[&str] = &[
    "banner_bg",
    "banner_border",
    "banner_fg",
    "banner_icon_fg",
    "banner_countdown_fg",
    "titlebar_bg",
    "titlebar_bg_inactive",
    "titlebar_border",
    "titlebar_fg",
    "titlebar_fg_inactive",
    "preset_leaf_label_fg",
    "preset_leaf_value_fg",
    "modhint_bg",
    "modhint_border",
    "modhint_role_bg",
    "modhint_role_fg",
    "modhint_empty_fg",
];

/// theme.rs에 직접 구현한 치수 접근자. 같은 이름으로 생성하지 않는다.
const EXISTING_THEME_DIM_ACCESSOR_NAMES: &[&str] = &[
    "modhint_width",
    "modhint_height",
    "modhint_min_width",
    "modhint_min_height",
    "modhint_section_gap",
    // 점·ring을 각각 반올림한 뒤 더하는 파생 치수다. 토큰 값 하나에 배율을 곱하면 틀린다.
    "workspace_dot_slot",
];

/// component 토큰 kebab 이름 → snake_case 접근자 함수명 (전체 이름 그대로 —
/// `component_split` 의 모듈 분해와 달리 flat `impl Theme` 메서드라 그룹 접두어를
/// 남겨 둔다. 예: `switch-overlay-bg` → `switch_overlay_bg`).
pub(super) fn accessor_fn_name(name: &str) -> String {
    name.replace('-', "_")
}

/// 치수 component 접근자의 본문 형태.
enum DimAccessor<'a> {
    /// alias 체인이 표에 있는 `Theme` 필드에 닿음 — 필드를 그대로 반환.
    Field(&'a str),
    /// alias 체인이 다른 component 접근자에 닿음(component→component) — 그 접근자를 호출.
    Chain(String),
    /// alias 체인이 primitive 로 직접 닿거나 표에 없는 semantic 을 거침 — `ui_zoom` 을
    /// 곱해 직접 계산 (raw const 를 그대로 소비하는 zoom 우회를 막는다).
    RawZoom(f32),
    /// `calc()` 식 — 각 항을 그 토큰의 접근자로 읽어 계수를 곱해 더한다. 항마다 반올림된 값을
    /// 더하므로 실제로 그려지는 부분의 합과 같다. 계수는 개수라 접근자 값 뒤에 곱한다
    /// (`LogicalPx(` 바로 뒤의 숫자는 배율 없는 길이로 읽힌다). 두 번째 값은 배율 1의 px(문서 주석용)이다.
    Calc(Vec<(f32, String)>, f32),
}

/// 치수 component 토큰 하나의 접근자 형태를 결정. 실패하면 스킵 사유 문자열.
fn resolve_dim_accessor<'a>(set: &TokenSet, token: &'a Token) -> Result<DimAccessor<'a>, String> {
    let own_path = token.path();
    if let Some((_, field)) = SEMANTIC_DIM_TO_THEME_FIELD
        .iter()
        .find(|(p, _)| *p == own_path)
    {
        return Ok(DimAccessor::Field(field));
    }
    if let Some(terms) = super::calc::parse(set, &token.value) {
        let skip = |e: String| format!("{own_path}: {e} — 생성 스킵");
        let terms = terms.map_err(skip)?;
        let px = super::calc::px_at_unit_zoom(set, &terms).map_err(skip)?;
        let mut parts = Vec::with_capacity(terms.len());
        for term in &terms {
            parts.push((term.coef, term_expr(set, term.token).map_err(skip)?));
        }
        return Ok(DimAccessor::Calc(parts, px));
    }
    let target_path = match alias_target(&token.value) {
        Some(t) => t,
        None => return Err(format!("{own_path}: 리터럴(alias 아님) — 생성 스킵")),
    };
    if let Some((_, field)) = SEMANTIC_DIM_TO_THEME_FIELD
        .iter()
        .find(|(p, _)| *p == target_path)
    {
        return Ok(DimAccessor::Field(field));
    }
    match set.get(target_path) {
        Some(target) if target.tier == Tier::Component => {
            Ok(DimAccessor::Chain(accessor_fn_name(&target.name)))
        }
        Some(_) => {
            let terminal = set
                .resolve(&own_path, ThemeMode::Mocha)
                .map_err(|e| format!("{own_path}: {e} — 생성 스킵"))?;
            let stripped = terminal.strip_suffix("px").unwrap_or(&terminal);
            stripped
                .trim()
                .parse::<f32>()
                .map(DimAccessor::RawZoom)
                .map_err(|_| format!("{own_path}: 최종 값 파싱 실패 ({terminal}) — 생성 스킵"))
        }
        None => Err(format!(
            "{own_path}: alias 대상 없음 ({target_path}) — 생성 스킵"
        )),
    }
}

/// `calc()` 항 토큰을 읽는 `LogicalPx` 식. 그 토큰의 접근자가 읽는 방식을 그대로 쓴다.
fn term_expr(set: &TokenSet, token: &Token) -> Result<String, String> {
    if token.tier == Tier::Component {
        return match resolve_dim_accessor(set, token)? {
            DimAccessor::Calc(..) => Err(format!("calc 항이 다시 calc 토큰 ({})", token.path())),
            _ => Ok(format!("self.{}()", accessor_fn_name(&token.name))),
        };
    }
    let path = token.path();
    if let Some((_, field)) = SEMANTIC_DIM_TO_THEME_FIELD.iter().find(|(p, _)| *p == path) {
        return Ok(format!("self.{field}"));
    }
    let terminal = set
        .resolve(&path, ThemeMode::Mocha)
        .map_err(|e| e.to_string())?;
    let v = terminal
        .strip_suffix("px")
        .unwrap_or(&terminal)
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("calc 항의 최종 값 파싱 실패 ({terminal})"))?;
    Ok(format!("LogicalPx(({v:?} * self.ui_zoom).round())"))
}

/// premultiplied 바이트로 저장되는 semantic 색. 이 색에 닿는 접근자는 `PremulColor`를 반환해
/// 알파를 한 번 더 곱하는 `to_egui()`를 컴파일 단계에서 막는다.
pub const PREMULTIPLIED_SEMANTIC_COLORS: &[&str] = &[
    "semantic.overlay-hover",
    "semantic.overlay-active",
    "semantic.separator",
];

/// alias 체인이 premultiplied semantic 색에 닿는지.
fn chain_reaches_premultiplied(set: &TokenSet, token: &Token) -> bool {
    let mut cur = token;
    // alias 체인은 짧다. 순환이 있어도 멈추도록 단계 수를 제한한다.
    for _ in 0..16 {
        if PREMULTIPLIED_SEMANTIC_COLORS.contains(&cur.path().as_str()) {
            return true;
        }
        let Some(next) = alias_target(&cur.value).and_then(|p| set.get(p)) else {
            return false;
        };
        cur = next;
    }
    false
}

/// 색 component 접근자의 본문 형태.
enum ColorAccessor {
    /// alias 체인이 semantic 색에 닿고, 표에 대응 `theme.rs` 접근자가 있음.
    SemanticExpr(&'static str),
    /// alias 체인이 다른 component 색 접근자에 닿음(component→component).
    Chain(String),
}

/// 색 component 토큰 하나의 접근자 형태를 결정. 실패하면 스킵 사유 문자열.
fn resolve_color_accessor(set: &TokenSet, token: &Token) -> Result<ColorAccessor, String> {
    let own_path = token.path();
    let target_path = match alias_target(&token.value) {
        Some(t) => t,
        None => return Err(format!("{own_path}: 리터럴/합성 색상값 — 생성 스킵")),
    };
    match set.get(target_path) {
        Some(target) if target.tier == Tier::Semantic => SEMANTIC_COLOR_TO_THEME_ACCESSOR
            .iter()
            .find(|(p, _)| *p == target_path)
            .map(|(_, expr)| ColorAccessor::SemanticExpr(expr))
            .ok_or_else(|| {
                format!("{own_path}: `{target_path}` 대응 theme.rs 접근자 없음 — 생성 스킵")
            }),
        Some(target) if target.tier == Tier::Component => {
            let fn_name = accessor_fn_name(&target.name);
            // 대상 접근자도 생성 가능한지 확인해야 존재하지 않는 메서드를 호출하지 않는다.
            if EXISTING_THEME_ACCESSOR_NAMES.contains(&fn_name.as_str()) {
                Ok(ColorAccessor::Chain(fn_name))
            } else {
                resolve_color_accessor(set, target)
                    .map(|_| ColorAccessor::Chain(fn_name))
                    .map_err(|e| format!("{own_path}: chain 대상이 스킵됨 → {e}"))
            }
        }
        Some(_) => Err(format!(
            "{own_path}: primitive 직접 alias — 색 접근자 규칙 밖, 생성 스킵"
        )),
        None => Err(format!(
            "{own_path}: alias 대상 없음 ({target_path}) — 생성 스킵"
        )),
    }
}

/// 치수 접근자 하나의 `impl Theme` 메서드 텍스트.
fn emit_dim_accessor(set: &TokenSet, token: &Token, acc: &DimAccessor) -> String {
    let terminal = match acc {
        DimAccessor::Calc(_, px) => format!("{px}px"),
        _ => set
            .resolve(&token.path(), ThemeMode::Mocha)
            .expect("resolve_dim_accessor 통과 토큰은 resolve 가능"),
    };
    let sentinel = if terminal == "9999px" {
        " (sentinel — 완전 원형용 상한값)"
    } else {
        ""
    };
    let fn_name = accessor_fn_name(&token.name);
    let body = match acc {
        DimAccessor::Field(field) => format!("self.{field}"),
        DimAccessor::Chain(target_fn) => format!("self.{target_fn}()"),
        // 음수는 부호를 밖에 둔다. `-1.0 * x` 는 clippy neg_multiply 에 걸리고, round() 는
        // 0 기준 대칭이라 값은 같다.
        DimAccessor::RawZoom(v) if *v < 0.0 => {
            format!("LogicalPx((-({:?} * self.ui_zoom)).round())", -v)
        }
        DimAccessor::RawZoom(v) => format!("LogicalPx(({v:?} * self.ui_zoom).round())"),
        DimAccessor::Calc(parts, _) => {
            let sum: Vec<String> = parts
                .iter()
                .map(|(coef, expr)| format!("{expr}.value() * {coef:?}"))
                .collect();
            // 생성 파일도 rustfmt 대상이라 freshness 비교가 깨지지 않게 rustfmt 의 줄바꿈을 따른다.
            let one_line = format!("LogicalPx({})", sum.join(" + "));
            if 8 + one_line.len() <= 100 {
                one_line
            } else {
                format!(
                    "LogicalPx(\n            {},\n        )",
                    sum.join("\n                + ")
                )
            }
        }
    };
    format!(
        "\n    /// `{}` → `{}` = {terminal}{sentinel}\n    #[inline]\n    pub fn {fn_name}(&self) -> LogicalPx {{\n        {body}\n    }}\n",
        token.path(),
        token.value,
    )
}

/// 색 접근자 하나의 `impl Theme` 메서드 텍스트.
fn emit_color_accessor(set: &TokenSet, token: &Token, acc: &ColorAccessor) -> String {
    let fn_name = accessor_fn_name(&token.name);
    let body = match acc {
        ColorAccessor::SemanticExpr(expr) => format!("self.{expr}"),
        ColorAccessor::Chain(target_fn) => format!("self.{target_fn}()"),
    };
    let ret = if chain_reaches_premultiplied(set, token) {
        "PremulColor"
    } else {
        "HexColor"
    };
    format!(
        "\n    /// `{}` → `{}`\n    #[inline]\n    pub fn {fn_name}(&self) -> {ret} {{\n        {body}\n    }}\n",
        token.path(),
        token.value,
    )
}

/// component 접근자는 Theme가 있는 type-appearance에 쓴다.
/// design-tokens가 type-appearance에 런타임 의존성을 갖지 않도록 한다.
pub(super) fn generate_component_accessors(set: &TokenSet) -> (String, Vec<String>) {
    let mut skips = Vec::new();
    let mut body = String::new();

    for token in set.iter().filter(|t| t.tier == Tier::Component) {
        match token.ty.as_str() {
            "dimension" => {
                let fn_name = accessor_fn_name(&token.name);
                if EXISTING_THEME_DIM_ACCESSOR_NAMES.contains(&fn_name.as_str()) {
                    skips.push(format!(
                        "{}: theme.rs 기존 수기 접근자 `{fn_name}` 과 이름 충돌 — 생성 스킵",
                        token.path()
                    ));
                    continue;
                }
                // 자간 토큰(em, 또는 자간 계열 이름의 단위 없는 0)은 글자 크기를 받는 접근자로
                // `generated_tracking.rs`에 같은 이름으로 생성된다.
                if super::tracking_accessor::terminal_em(set, token).is_some() {
                    skips.push(format!(
                        "{}: 자간 토큰 — 접근자는 generated_tracking.rs 에 생성",
                        token.path()
                    ));
                    continue;
                }
                match resolve_dim_accessor(set, token) {
                    Ok(acc) => body.push_str(&emit_dim_accessor(set, token, &acc)),
                    Err(reason) => skips.push(reason),
                }
            }
            "color" => {
                let fn_name = accessor_fn_name(&token.name);
                if EXISTING_THEME_ACCESSOR_NAMES.contains(&fn_name.as_str()) {
                    skips.push(format!(
                        "{}: theme.rs 기존 수기 접근자 `{fn_name}` 과 이름 충돌 — 생성 스킵",
                        token.path()
                    ));
                    continue;
                }
                match resolve_color_accessor(set, token) {
                    Ok(acc) => body.push_str(&emit_color_accessor(set, token, &acc)),
                    Err(reason) => skips.push(reason),
                }
            }
            "duration" => {
                let fn_name = accessor_fn_name(&token.name);
                // 반환 타입이 달라도 같은 이름의 메서드를 중복 정의할 수 없다.
                if EXISTING_THEME_DIM_ACCESSOR_NAMES.contains(&fn_name.as_str())
                    || EXISTING_THEME_ACCESSOR_NAMES.contains(&fn_name.as_str())
                {
                    skips.push(format!(
                        "{}: theme.rs 기존 수기 접근자 `{fn_name}` 과 이름 충돌 — 생성 스킵",
                        token.path()
                    ));
                    continue;
                }
                match resolve_duration_accessor(set, token) {
                    Ok(acc) => body.push_str(&emit_duration_accessor(set, token, &acc)),
                    Err(reason) => skips.push(reason),
                }
            }
            // 테마와 배율에 무관한 number/fontWeight는 상수로만 생성한다.
            _ => {}
        }
    }

    let header = "//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.\n\
                  //! 재생성: `cargo run -p tasty-design-tokens --bin generate`.\n\
                  //!\n\
                  //! Component 치수·색·시간을 Theme를 통해 읽는다.\n\
                  //! 치수는 배율을 적용한 필드를 쓰거나 ui_zoom을 곱한다.\n\
                  //! 색은 연결된 접근자로 읽고, 시간은 배율 없이 Millis로 반환한다.\n\n\
                  use crate::color::{HexColor, PremulColor};\n\
                  use crate::motion::Millis;\n\
                  use tasty_type_geometry::length::LogicalPx;\n\n\
                  impl crate::theme::Theme {";
    let mut file = header.to_string();
    file.push_str(&body);
    file.push_str("}\n");
    (file, skips)
}

/// semantic 색 접근자 하나의 `impl Theme` 메서드 텍스트.
fn emit_semantic_color_accessor(token: &Token, fn_name: &str, field: &str) -> String {
    format!(
        "\n    /// `{}` → `{}`\n    #[inline]\n    pub fn {fn_name}(&self) -> HexColor {{\n        self.{field}\n    }}\n",
        token.path(),
        token.value,
    )
}

/// semantic 색 접근자를 Theme가 있는 type-appearance에 생성한다.
pub(super) fn generate_semantic_color_accessors(set: &TokenSet) -> (String, Vec<String>) {
    let mut skips = Vec::new();
    let mut body = String::new();

    for (path, fn_name, field) in SEMANTIC_COLOR_ACCESSOR_GEN {
        match set.get(path) {
            // 대응표에만 남은 토큰은 로그로 알린다.
            None => skips.push(format!(
                "{path}: SEMANTIC_COLOR_ACCESSOR_GEN 표에 있으나 vendor json 에 없음 — 생성 스킵"
            )),
            Some(token) if token.ty != "color" => skips.push(format!(
                "{path}: $type {} (color 아님) — 생성 스킵",
                token.ty
            )),
            Some(token) => body.push_str(&emit_semantic_color_accessor(token, fn_name, field)),
        }
    }

    for (path, reason) in SEMANTIC_COLOR_HAND_WRITTEN {
        skips.push(format!("{path}: {reason} — theme.rs 수기 유지"));
    }

    let header = "//! Generated from `dtcg/tasty.tokens.json` — DO NOT EDIT.\n\
                  //! 재생성: `cargo run -p tasty-design-tokens --bin generate`.\n\
                  //!\n\
                  //! Semantic 색 접근자는 대응하는 Theme 필드를 반환한다.\n\
                  //! 테마 밝기에 따른 분기, 합성색, OS·브랜드 고정 색처럼 단순 필드 반환이\n\
                  //! 아닌 접근자는 theme.rs에 직접 구현한다.\n\n\
                  use crate::color::HexColor;\n\n\
                  impl crate::theme::Theme {";
    let mut file = header.to_string();
    file.push_str(&body);
    file.push_str("}\n");
    (file, skips)
}
