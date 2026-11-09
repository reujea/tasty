//! 펼친 사이드바와 접힌 레일의 정적 예제. 워크스페이스·카테고리·상태 배지를 비교한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    ControlSize, IconButton, LoadingPlacement, TagVariant, brand, paint_dashed_outline,
    paint_loading_overlay, paint_num_keycap, paint_rail_category_line, paint_workspace_mirror_chip,
    tag_caps,
};

use crate::catalog::icons::{
    CHEVRON_DOWN, CHEVRON_RIGHT, CHEVRONS_LEFT, FOLDER, MockGlyph, PLUG, REMOTE, SETTINGS, TERMINAL,
};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// (이름, 배지, 활성 여부, 원격 미러 여부). 원격 미러는 이름 아래 별도 줄에 표시한다.
type WsRow = (&'static str, Option<&'static str>, bool, bool);
/// 카테고리 섹션 데모 데이터: (label, collapsed, rows).
type CategorySection = (&'static str, bool, &'static [WsRow]);

/// "infra" 는 mirror + notif 배지 공존 데모(채널 분리: glyph / badge 별도 축).
const WORKSPACES: &[WsRow] = &[
    ("main", None, true, false),
    ("infra", Some("2"), false, true),
    ("agent", None, false, false),
];

/// 카테고리 그룹 데모 데이터 (디자인 데모셋: normal / Services / Archived).
/// SERVICES 의 "agent" 는 mirror — full 은 이름 아래 별도 줄의 "REMOTE" pill, rail 은
/// 아바타 우하단 corner chip 으로 표시.
const CATEGORY_SECTIONS: &[CategorySection] = &[
    (
        "WORKSPACES",
        false,
        &[
            ("main", None, true, false),
            ("review", Some("3"), false, false),
        ],
    ),
    ("SERVICES", false, &[("agent", None, false, true)]),
    // 빈 + 접힌 카테고리 — 헤더(chevron ▶)만.
    ("ARCHIVED", true, &[]),
];
/// footer ghost rows.
const FOOTER: &[(MockGlyph, &str)] = &[
    (TERMINAL, "Tools"),
    (PLUG, "Plugins"),
    (SETTINGS, "Settings"),
];
/// collapsed rail slots.
const RAIL_SLOTS: &[MockGlyph] = &[TERMINAL, FOLDER, PLUG, SETTINGS];

fn paint_icon(
    ui: &mut egui::Ui,
    glyph: MockGlyph,
    center: egui::Pos2,
    size: f32,
    color: egui::Color32,
) {
    let r = egui::Rect::from_center_size(center, egui::vec2(size, size));
    glyph.image(size, color).paint_at(ui, r);
}

/// mirror 행의 pill 줄 높이(추가 행 높이 산정용) — 아이콘/pill 높이 중 큰 값.
fn mirror_pill_line_h(theme: &Theme) -> f32 {
    theme
        .tag_size()
        .value()
        .max(theme.workspace_mirror_icon_size().value())
}

/// 원격 미러는 이름 아래에 REMOTE 배지를 표시한다. 호출자가 추가 행 높이를 확보해야 한다.
fn mirror_pill_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    row: egui::Rect,
    name_x: f32,
    mirror: bool,
    active: bool,
) {
    if !mirror {
        return;
    }
    let y = row.bottom() + theme.spacing_xs.value();
    let sz = theme.workspace_mirror_icon_size().value();
    let cy = y + mirror_pill_line_h(theme) * 0.5;
    paint_icon(
        ui,
        REMOTE,
        egui::pos2(name_x + sz * 0.5, cy),
        sz,
        egui::Color32::from(if active {
            theme.workspace_mirror_fg_active()
        } else {
            theme.workspace_mirror_fg()
        }),
    );
    let mut tag_ui = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
        egui::pos2(name_x + sz + theme.workspace_mirror_gap().value(), y),
        egui::vec2(ui.available_width(), mirror_pill_line_h(theme)),
    )));
    tag_caps(&mut tag_ui, theme, "remote", TagVariant::Info, false);
}

/// 오른쪽 끝에서 offset만큼 떨어진 곳에 개수 배지를 그린다.
/// 반환한 폭은 다음 배지를 배치할 때 사용한다.
pub(crate) fn paint_ws_count_badge_at(
    p: &egui::Painter,
    theme: &Theme,
    row: egui::Rect,
    right_offset: f32,
    label: &str,
    fill: egui::Color32,
) -> f32 {
    let size = theme.badge_size().value();
    let pad_x = theme.badge_padding_x().value();
    let galley = p.layout_no_wrap(
        label.to_string(),
        egui::FontId::monospace(theme.badge_font_size().value()),
        egui::Color32::from(theme.text_on_accent()),
    );
    let w = (galley.size().x + pad_x * 2.0).max(size);
    let badge_rect = egui::Rect::from_min_size(
        egui::pos2(
            row.max.x - theme.spacing_sm.value() - right_offset - w,
            row.center().y - size * 0.5,
        ),
        egui::vec2(w, size),
    );
    p.rect_filled(badge_rect, size / 2.0, fill);
    let gp = egui::pos2(
        badge_rect.center().x - galley.size().x / 2.0,
        badge_rect.center().y - galley.size().y / 2.0,
    );
    p.galley(gp, galley, egui::Color32::from(theme.text_on_accent()));
    w
}

/// Completion(파랑) 단일 배지 — 기존 데모 행(`WORKSPACES`/`CATEGORY_SECTIONS`)이
/// 쓰는 단순 형태.
fn paint_ws_count_badge(p: &egui::Painter, theme: &Theme, row: egui::Rect, label: &str) {
    paint_ws_count_badge_at(
        p,
        theme,
        row,
        0.0,
        label,
        egui::Color32::from(theme.accent_primary()),
    );
}

/// 시안 사이드바 머리(`padding: md md xs`) — 공용 워드마크와 오른쪽 끝 접기 버튼.
/// 머리 아래 y 를 돌려준다.
fn sidebar_head(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, y: f32) -> f32 {
    let pad = theme.spacing_md.value();
    // 머리 높이는 로고(22)와 접기 버튼(IconButton sm 24) 중 큰 쪽이다.
    let head_h = theme
        .sidebar_logo_size
        .value()
        .max(ControlSize::Sm.height(theme));
    // 본체 사이드바 머리와 같은 공용 워드마크(로고 + mono `tasty.`, sidebar-wordmark-tracking).
    let mut head = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                egui::pos2(rect.min.x + pad, y),
                egui::vec2(rect.width() - pad * 2.0, head_h),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    brand::draw_wordmark(
        &mut head,
        theme,
        theme.sidebar_logo_size,
        theme.sidebar_wordmark_font_size,
        theme.sidebar_wordmark_tracking(theme.sidebar_wordmark_font_size),
    );
    // 시안 머리 오른쪽 끝의 접기 버튼(IconButton sm, chevrons) — 본체와 같은 공용 위젯.
    head.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        IconButton::new()
            .size(ControlSize::Sm)
            .show(ui, theme, &|ui, rect, c| {
                CHEVRONS_LEFT.image(rect.width(), c).paint_at(ui, rect);
            });
    });
    y + head_h + theme.spacing_xs.value()
}

fn full(ui: &mut egui::Ui, theme: &Theme) {
    let w = theme.field_width_lg.value() + theme.spacing_md.value(); // 212
    let h = theme.spacing_xl.value() * 15.0; // 360
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_sidebar()),
    );

    let pad = theme.spacing_md.value(); // 12
    let row_h = theme.item_height_interactive.value(); // 28
    let mut y = rect.min.y + pad;

    y = sidebar_head(ui, theme, rect, y) + theme.spacing_md.value();

    let heading = section_heading_galley(ui, theme, "Workspaces", theme.text_muted().into());
    p.galley(
        egui::pos2(rect.min.x + pad, y),
        heading,
        egui::Color32::PLACEHOLDER,
    );
    y += theme.spacing_lg.value();

    for (name, badge, active, mirror) in WORKSPACES {
        let row = WsRowSpec::plain(name, *badge, *active, *mirror);
        let card_h = row_h + ws_row_extra_h(theme, &row);
        let card = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + theme.spacing_xs.value(), y),
            egui::vec2(w - theme.spacing_xs.value() * 2.0, card_h),
        );
        paint_ws_row(ui, theme, card, &row);
        y += card_h + theme.spacing_xs.value();
    }

    let footer_h = row_h * FOOTER.len() as f32 + pad;
    let footer_top = rect.max.y - footer_h;
    p.hline(
        rect.x_range(),
        footer_top,
        egui::Stroke::new(
            theme.border_width.value(),
            egui::Color32::from(theme.border_default()),
        ),
    );
    let mut fy = footer_top + theme.spacing_sm.value();
    for (glyph, label) in FOOTER {
        let cy = fy + row_h * 0.5;
        paint_icon(
            ui,
            *glyph,
            egui::pos2(
                rect.min.x + pad + theme.icon_glyph_size_sm.value() * 0.5,
                cy,
            ),
            theme.icon_glyph_size_sm.value(),
            egui::Color32::from(theme.text_muted()),
        );
        p.text(
            egui::pos2(
                rect.min.x + pad + theme.icon_glyph_size_sm.value() + theme.spacing_sm.value(),
                cy,
            ),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(theme.sidebar_button_label_font_size.value()),
            egui::Color32::from(theme.text_secondary()),
        );
        fy += row_h;
    }
}

fn rail(ui: &mut egui::Ui, theme: &Theme) {
    // 52 = collapsed slot 32 + lg 16 + xs 4.
    let w = theme.sidebar_collapsed_slot_width.value()
        + theme.spacing_lg.value()
        + theme.spacing_xs.value();
    let h = theme.spacing_xl.value() * 15.0; // 360
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_sidebar()),
    );

    let cx = rect.center().x;
    let mut y = rect.min.y + theme.spacing_md.value();

    let logo = theme.sidebar_logo_collapsed_size.value(); // 24
    paint_icon(
        ui,
        TERMINAL,
        egui::pos2(cx, y + logo * 0.5),
        logo,
        egui::Color32::from(theme.accent_primary()),
    );
    y += logo + theme.spacing_md.value();

    let slot = theme.item_height_interactive.value(); // 28
    for (i, glyph) in RAIL_SLOTS.iter().enumerate() {
        let area =
            egui::Rect::from_center_size(egui::pos2(cx, y + slot * 0.5), egui::vec2(slot, slot));
        if i == 0 {
            p.rect_filled(
                area,
                theme.corner_radius_sm.value(),
                egui::Color32::from(theme.surface_active()),
            );
        }
        paint_icon(
            ui,
            *glyph,
            area.center(),
            theme.icon_glyph_size_md.value(),
            // 비활성 레일 아이콘은 본체와 같은 glyph-dim 색을 쓴다.
            egui::Color32::from(if i == 0 {
                theme.text_on_active()
            } else {
                theme.glyph_dim()
            }),
        );
        y += slot + theme.spacing_sm.value();
    }
}

/// 사이드바 섹션·카테고리 머리글 — mono 대문자, `sidebar_section_heading_tracking` 자간(본체와 같다).
fn section_heading_galley(
    ui: &egui::Ui,
    theme: &Theme,
    text: &str,
    color: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    let size = theme.sidebar_section_heading_font_size;
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: tasty_ui_widgets::label_font(size.value()),
            extra_letter_spacing: theme.sidebar_section_heading_tracking(size).value(),
            color,
            ..Default::default()
        },
    );
    ui.painter().layout_job(job)
}

/// 행 배경 상태. hover는 비활성 행 위에 겹치는 overlay다.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum RowState {
    Active,
    Inactive,
    Hover,
}

/// 워크스페이스 행 1개의 표시 내용. 본체 `draw_workspace_card`의 축을 그대로 옮긴다.
#[derive(Clone, Copy)]
pub(crate) struct WsRowSpec<'a> {
    pub(crate) name: &'a str,
    pub(crate) badge: Option<&'a str>,
    pub(crate) state: RowState,
    pub(crate) busy: bool,
    pub(crate) attached: bool,
    pub(crate) mirror: bool,
    pub(crate) subtitle: Option<&'a str>,
    /// 부제 아래 설명. 본체처럼 최대 두 줄로 줄인다.
    pub(crate) description: Option<&'a str>,
    /// attach 응답 대기 스피너를 점 칸에 겹친다.
    pub(crate) loading: bool,
    /// 전환 키캡 숫자. 점 대신 그리고 스피너보다 위에 둔다.
    pub(crate) keycap: Option<&'a str>,
    /// 오버레이 rect(점 칸)를 점선으로 보인다.
    pub(crate) boxes: bool,
}

impl<'a> WsRowSpec<'a> {
    /// 기존 데모 행. 활성 행만 실행 중으로 표시하고 attached·부제는 없다.
    fn plain(name: &'a str, badge: Option<&'a str>, active: bool, mirror: bool) -> Self {
        Self {
            name,
            badge,
            state: if active {
                RowState::Active
            } else {
                RowState::Inactive
            },
            busy: active,
            attached: false,
            mirror,
            subtitle: None,
            description: None,
            loading: false,
            keycap: None,
            boxes: false,
        }
    }
}

/// 점 슬롯 중심 x와 본문 x. 슬롯은 attached 여부와 무관하게 모든 행에서 예약해
/// ring 전체(점 8 + 2×(offset + 폭))가 inset·간격 안에 들어가게 한다.
/// 슬롯 폭은 점과 ring을 각각 반올림한 뒤 더한 값이다(0.85 → 15 · 1 → 16 · 1.2 → 18).
fn ws_row_columns(theme: &Theme, card: egui::Rect) -> (f32, f32) {
    let slot_x = card.min.x + theme.workspace_row_padding_x().value();
    let slot = theme.workspace_dot_slot().value();
    (
        slot_x + slot * 0.5,
        slot_x + slot + theme.workspace_dot_gap().value(),
    )
}

/// 부제 한 줄 높이. 본체 부제와 같은 글꼴에 UI 줄간격을 적용한다.
fn subtitle_line_h(theme: &Theme) -> f32 {
    theme.sidebar_button_label_font_size.value() * theme.line_height_ui
}

/// 제목 줄 아래에 붙는 REMOTE 줄·부제 줄의 높이 합.
pub(crate) fn ws_row_extra_h(theme: &Theme, row: &WsRowSpec) -> f32 {
    let mut h = 0.0;
    if row.mirror {
        h += theme.spacing_xs.value() + mirror_pill_line_h(theme);
    }
    if row.subtitle.is_some() {
        h += theme.spacing_xs.value() + subtitle_line_h(theme);
    }
    if row.description.is_some() {
        h += theme.spacing_xs.value() + description_line_h(theme) * 2.0;
    }
    h
}

/// 설명 한 줄 높이. 본체 설명과 같은 글꼴·줄간격이다.
fn description_line_h(theme: &Theme) -> f32 {
    theme.font_size_caption.value() * theme.line_height_ui
}

/// 워크스페이스 행 1개 — 슬롯 가운데 점(+ attached ring), 이름, 배지, REMOTE 줄, 부제.
/// `card`는 제목 줄과 추가 줄을 모두 포함하며 배경·hover·accent bar가 카드 전체를 덮는다.
pub(crate) fn paint_ws_row(ui: &mut egui::Ui, theme: &Theme, card: egui::Rect, row: &WsRowSpec) {
    let p = ui.painter_at(card);
    match row.state {
        RowState::Active => {
            p.rect_filled(
                card,
                theme.corner_radius_sm.value(),
                egui::Color32::from(theme.surface_active()),
            );
            let bar = egui::Rect::from_min_size(
                card.min,
                egui::vec2(
                    theme.workspace_row_active_bar_width().value(),
                    card.height(),
                ),
            );
            p.rect_filled(bar, 0.0, egui::Color32::from(theme.accent_primary()));
        }
        RowState::Hover => {
            p.rect_filled(
                card,
                theme.corner_radius_sm.value(),
                theme.hover_overlay.to_egui_premultiplied(),
            );
        }
        RowState::Inactive => {}
    }
    let title = egui::Rect::from_min_size(
        card.min,
        egui::vec2(card.width(), theme.item_height_interactive.value()),
    );
    let (dot_x, name_x) = ws_row_columns(theme, card);
    let dc = egui::pos2(dot_x, title.center().y);
    let dot_r = theme.status_dot_size.value() * 0.5;
    if let Some(digit) = row.keycap {
        paint_num_keycap(&p, theme, dc, digit, row.state == RowState::Active, 1.0);
    } else {
        paint_ws_dot(ui, theme, &p, row, dc, dot_r);
    }
    let active = row.state == RowState::Active;
    p.text(
        egui::pos2(name_x, title.center().y),
        egui::Align2::LEFT_CENTER,
        row.name,
        egui::FontId::proportional(theme.font_size_body.value()),
        egui::Color32::from(if active {
            theme.text_on_active()
        } else if row.state == RowState::Hover {
            theme.text_secondary_raised()
        } else {
            theme.text_secondary()
        }),
    );
    if let Some(b) = row.badge {
        paint_ws_count_badge(&p, theme, title, b);
    }
    mirror_pill_line(ui, theme, title, name_x, row.mirror, active);
    if let Some(sub) = row.subtitle {
        let pill = if row.mirror {
            theme.spacing_xs.value() + mirror_pill_line_h(theme)
        } else {
            0.0
        };
        let top = title.bottom() + pill + theme.spacing_xs.value();
        p.text(
            egui::pos2(name_x, top + subtitle_line_h(theme) * 0.5),
            egui::Align2::LEFT_CENTER,
            sub,
            egui::FontId::proportional(theme.sidebar_button_label_font_size.value()),
            egui::Color32::from(if active {
                theme.text_on_active()
            } else {
                theme.text_muted()
            }),
        );
    }
    if let Some(description) = row.description {
        paint_ws_description(ui, theme, card, name_x, description, active);
    }
}

/// 카드 맨 아래 두 줄 칸에 설명을 그린다. 본체처럼 placeholder 색, 최대 두 줄이다.
fn paint_ws_description(
    ui: &egui::Ui,
    theme: &Theme,
    card: egui::Rect,
    name_x: f32,
    description: &str,
    active: bool,
) {
    let size = theme.font_size_caption.value();
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: (card.right() - theme.spacing_sm.value() - name_x).max(0.0),
            max_rows: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    job.append(
        description,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: egui::Color32::from(if active {
                theme.text_on_active()
            } else {
                theme.text_placeholder()
            }),
            line_height: Some(description_line_h(theme)),
            ..Default::default()
        },
    );
    let galley = ui.fonts(|f| f.layout_job(job));
    let top = card.bottom() - description_line_h(theme) * 2.0;
    ui.painter_at(card)
        .galley(egui::pos2(name_x, top), galley, egui::Color32::PLACEHOLDER);
}

/// 점과 attached ring. 로딩이면 그 위 점 칸(16 × 제목 줄)에 행 바탕을 다시 깔고 스피너를 그린다.
fn paint_ws_dot(
    ui: &mut egui::Ui,
    theme: &Theme,
    p: &egui::Painter,
    row: &WsRowSpec,
    dc: egui::Pos2,
    dot_r: f32,
) {
    let active = row.state == RowState::Active;
    p.circle_filled(
        dc,
        dot_r,
        egui::Color32::from(match (row.busy, active) {
            (true, true) => theme.status_dot_success_active(),
            (true, false) => theme.status_dot_success(),
            (false, true) => theme.status_dot_idle_active(),
            (false, false) => theme.status_dot_idle(),
        }),
    );
    // 링의 offset은 점의 바깥쪽에서 링 안쪽까지의 거리다(본체와 같은 식).
    if row.attached {
        let ring_w = theme.status_dot_attached_ring_width().value();
        p.circle_stroke(
            dc,
            dot_r + theme.status_dot_attached_ring_offset().value() + ring_w * 0.5,
            egui::Stroke::new(
                ring_w,
                egui::Color32::from(if active {
                    theme.status_dot_attached_ring_active()
                } else {
                    theme.status_dot_attached_ring()
                }),
            ),
        );
    }
    if !row.loading {
        return;
    }
    let slot = egui::Rect::from_center_size(
        dc,
        egui::vec2(
            theme.workspace_dot_slot().value(),
            theme.font_size_body.value() * theme.line_height_ui,
        ),
    );
    // 시안 bed는 bg-sidebar 위에 행 채움을 겹친 색이다.
    p.rect_filled(slot, 0.0, egui::Color32::from(theme.bg_sidebar()));
    match row.state {
        RowState::Active => {
            p.rect_filled(slot, 0.0, egui::Color32::from(theme.surface_active()));
        }
        RowState::Hover => {
            p.rect_filled(slot, 0.0, theme.hover_overlay.to_egui_premultiplied());
        }
        RowState::Inactive => {}
    }
    paint_loading_overlay(
        ui,
        theme,
        slot,
        LoadingPlacement::Lnb,
        egui::Color32::TRANSPARENT,
    );
    if row.boxes {
        paint_dashed_outline(
            p,
            theme,
            slot,
            0.0,
            egui::Color32::from(theme.accent_move()),
        );
    }
}

/// 카테고리 그룹(확장) — chevron 헤더 + 소속 행. 접힌/빈 카테고리는 헤더만.
fn full_categories(ui: &mut egui::Ui, theme: &Theme) {
    let w = theme.field_width_lg.value() + theme.spacing_md.value(); // 212
    let h = theme.spacing_xl.value() * 15.0; // 360
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_sidebar()),
    );

    let pad = theme.spacing_md.value(); // 12
    let row_h = theme.item_height_interactive.value(); // 28
    let mut y = sidebar_head(ui, theme, rect, rect.min.y + pad);

    for (i, (label, collapsed, rows)) in CATEGORY_SECTIONS.iter().enumerate() {
        if i > 0 {
            y += theme.spacing_md.value();
        }
        let chevron = if *collapsed {
            CHEVRON_RIGHT
        } else {
            CHEVRON_DOWN
        };
        let pad_y = theme.sidebar_category_header_pad_y().value();
        let pad_x = theme.sidebar_category_header_pad_x().value();
        let ch_size = theme.icon_glyph_size_sm.value();
        let header_h = pad_y + ch_size + pad_y;
        let header_rect =
            egui::Rect::from_min_size(egui::pos2(rect.min.x, y), egui::vec2(w, header_h));
        p.rect_filled(
            header_rect,
            0.0,
            theme.sidebar_category_header_bg().to_egui(),
        );
        let border = theme
            .sidebar_category_header_border()
            .to_egui_premultiplied();
        let border_w = theme.border_width.value();
        p.hline(
            header_rect.x_range(),
            header_rect.min.y,
            egui::Stroke::new(border_w, border),
        );
        p.hline(
            header_rect.x_range(),
            header_rect.max.y,
            egui::Stroke::new(border_w, border),
        );
        y += pad_y;
        let fg = egui::Color32::from(theme.sidebar_category_header_fg());
        let ch_c = egui::pos2(rect.min.x + pad_x + ch_size * 0.5, y + ch_size * 0.5);
        paint_icon(ui, chevron, ch_c, ch_size, fg);
        let heading = section_heading_galley(ui, theme, label, fg);
        let heading_y = ch_c.y - heading.size().y * 0.5;
        p.galley(
            egui::pos2(ch_c.x + ch_size * 0.5 + theme.spacing_xs.value(), heading_y),
            heading,
            egui::Color32::PLACEHOLDER,
        );
        p.text(
            egui::pos2(rect.max.x - pad_x, ch_c.y),
            egui::Align2::RIGHT_CENTER,
            rows.len().to_string(),
            egui::FontId::monospace(theme.sidebar_category_header_count_font_size().value()),
            egui::Color32::from(theme.sidebar_category_header_count_fg()),
        );
        y += ch_size + pad_y;

        // 헤더가 이미 아래쪽 선을 그리므로 구분선을 덧그리지 않는다.
        if !*collapsed && !rows.is_empty() {
            for (name, badge, active, mirror) in *rows {
                let row = WsRowSpec::plain(name, *badge, *active, *mirror);
                let card_h = row_h + ws_row_extra_h(theme, &row);
                let card = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + theme.spacing_xs.value(), y),
                    egui::vec2(w - theme.spacing_xs.value() * 2.0, card_h),
                );
                paint_ws_row(ui, theme, card, &row);
                y += card_h + theme.spacing_xs.value();
            }
        }
    }
}

/// 카테고리 그룹(축소 레일) — `---` 경계 버튼 + 아바타. 접힌 카테고리는 `---` 만.
fn rail_categories(ui: &mut egui::Ui, theme: &Theme) {
    let w = theme.sidebar_collapsed_slot_width.value()
        + theme.spacing_lg.value()
        + theme.spacing_xs.value();
    let h = theme.spacing_xl.value() * 15.0; // 360
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_sidebar()),
    );

    let cx = rect.center().x;
    let slot = theme.item_height_interactive.value(); // 28
    let mut y = rect.min.y + theme.spacing_md.value();

    for (_label, collapsed, rows) in CATEGORY_SECTIONS {
        paint_rail_category_line(
            &p,
            theme,
            egui::pos2(cx, y + theme.spacing_lg.value() * 0.5),
            false,
        );
        y += theme.spacing_lg.value() + theme.spacing_xs.value();

        if !*collapsed {
            for (name, _badge, active, mirror) in *rows {
                let area = egui::Rect::from_center_size(
                    egui::pos2(cx, y + slot * 0.5),
                    egui::vec2(slot, slot),
                );
                if *active {
                    p.rect_filled(
                        area,
                        theme.corner_radius_sm.value(),
                        egui::Color32::from(theme.surface_active()),
                    );
                }
                let letter = name
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                p.text(
                    area.center(),
                    egui::Align2::CENTER_CENTER,
                    letter,
                    egui::FontId::monospace(theme.font_size_body.value()),
                    egui::Color32::from(if *active {
                        theme.accent_primary_raised()
                    } else {
                        theme.text_muted()
                    }),
                );
                if *mirror {
                    paint_workspace_mirror_chip(ui, theme, area);
                }
                y += slot + theme.spacing_sm.value();
            }
        }
    }
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "Full · 212", |ui| full(ui, theme));
        spec::cluster(ui, theme, "Collapsed rail · 52", |ui| rail(ui, theme));
        spec::cluster(ui, theme, "Categories · full", |ui| {
            full_categories(ui, theme)
        });
        spec::cluster(ui, theme, "Categories · rail", |ui| {
            rail_categories(ui, theme)
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("full width", "212"),
            ("rail width", "52"),
            ("logo", "22 full / 24 rail"),
            ("row", "dot + name + badge"),
            ("active row", "surface-active + 2px inset accent"),
            ("mirror", "REMOTE pill line / rail corner chip"),
            (
                "rail mirror chip",
                "12 circle · 1px workspace-mirror-chip-border inside · bed workspace-mirror-chip-bg · glyph 8 · 2px bg-sidebar halo",
            ),
            ("footer", "Tools·Plugins·Settings, border-top"),
        ],
        &[
            TokenChip::new("bg-sidebar", "sidebar fill", theme.bg_sidebar().into()),
            TokenChip::new(
                "surface-active",
                "active row",
                theme.surface_active().into(),
            ),
            TokenChip::new(
                "accent-primary",
                "inset bar + logo",
                theme.accent_primary().into(),
            ),
            TokenChip::new(
                "workspace-mirror-fg",
                "mirror glyph/chip",
                theme.workspace_mirror_fg().into(),
            ),
            TokenChip::new(
                "rail-category-line",
                "rail category --- line (2px) → text-muted",
                theme.rail_category_line().into(),
            ),
            TokenChip::new(
                "workspace-mirror-chip-border",
                "rail chip edge → workspace-mirror-fg",
                theme.workspace_mirror_chip_border().into(),
            ),
            TokenChip::new(
                "workspace-mirror-chip-bg",
                "rail chip bed → bg-sidebar",
                theme.workspace_mirror_chip_bg().into(),
            ),
            TokenChip::new(
                "border-default",
                "footer divider",
                theme.border_default().into(),
            ),
            TokenChip::new(
                "sidebar-category-header-bg",
                "header band face (one tier below the sidebar)",
                theme.sidebar_category_header_bg().into(),
            ),
            TokenChip::new(
                "sidebar-category-header-fg",
                "label + chevron",
                theme.sidebar_category_header_fg().into(),
            ),
            TokenChip::new(
                "sidebar-category-header-count-fg",
                "trailing count",
                theme.sidebar_category_header_count_fg().into(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "펼친 사이드바는 이름과 배지를 표시하고 접힌 레일은 아이콘 중심으로 보여준다. 원격 미러는 펼친 행의 REMOTE 배지 또는 아바타 오른쪽 아래 표시로 구분한다. 카테고리에는 접기 버튼이 있으며 접힌 레일에서는 경계선으로 표시한다. 응답 대기·완료 배지와 레일 점의 순위는 Layouts › Attention kinds 에서 보인다.",
    );
}

/// attached ring 예제 행: 시안 "Attached ring in the workspace row"와 같은 세 행.
/// (이름, attached, REMOTE 줄, 부제).
const ATTACHED_ROWS: &[(&str, bool, bool, Option<&str>)] = &[
    ("second", true, false, None),
    ("api-server", false, false, None),
    ("staging", true, true, Some("ssh deploy@10.0.4.12")),
];

/// 행 여러 개를 사이드바 폭의 카드 열로 그린다.
pub(crate) fn ws_column(ui: &mut egui::Ui, theme: &Theme, rows: &[WsRowSpec]) {
    let w = theme.field_width_lg.value() + theme.spacing_md.value(); // 212
    let row_h = theme.item_height_interactive.value();
    let heights: Vec<f32> = rows
        .iter()
        .map(|r| row_h + ws_row_extra_h(theme, r))
        .collect();
    let h = heights.iter().sum::<f32>() + theme.spacing_xs.value() * (rows.len() as f32 + 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter_at(rect).rect_filled(
        rect,
        theme.corner_radius.value(),
        egui::Color32::from(theme.bg_sidebar()),
    );
    let mut y = rect.min.y + theme.spacing_xs.value();
    for (row, card_h) in rows.iter().zip(heights) {
        let card = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + theme.spacing_xs.value(), y),
            egui::vec2(w - theme.spacing_xs.value() * 2.0, card_h),
        );
        paint_ws_row(ui, theme, card, row);
        y += card_h + theme.spacing_xs.value();
    }
}

fn attached_rows(state: RowState) -> Vec<WsRowSpec<'static>> {
    ATTACHED_ROWS
        .iter()
        .map(|(name, attached, mirror, subtitle)| WsRowSpec {
            name,
            badge: None,
            state,
            busy: true,
            attached: *attached,
            mirror: *mirror,
            subtitle: *subtitle,
            description: None,
            loading: false,
            keycap: None,
            boxes: false,
        })
        .collect()
}

/// 같은 팔레트를 다른 UI 배율로 다시 만든다. 토큰은 배율을 타므로 슬롯·inset·간격이 함께 변한다.
pub(crate) fn with_zoom(base: &Theme, zoom: f32) -> Theme {
    Theme::with_colors_and_zoom(base.to_colors(), base.is_light, zoom)
}

/// 워크스페이스 행의 attached ring — 모든 행이 16 슬롯을 예약한다.
pub fn draw_attached_ring(ui: &mut egui::Ui, theme: &Theme) {
    let palettes = [
        ("Mocha", tasty_themes::mocha_fallback()),
        ("Latte", crate::host_shell::latte_theme()),
    ];
    // 팔레트마다 그 팔레트의 무대 배경 위에 세 상태 열을 놓는다.
    for (palette, base) in &palettes {
        let th = with_zoom(base, theme.ui_zoom);
        spec::stage(ui, &th, StageVariant::Wrap, |ui| {
            for (state, label) in [
                (RowState::Active, "active"),
                (RowState::Inactive, "inactive"),
                (RowState::Hover, "hover"),
            ] {
                spec::cluster(ui, &th, &format!("{palette} · {label}"), |ui| {
                    ws_column(ui, &th, &attached_rows(state))
                });
            }
        });
    }
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for key in ["small", "large"] {
            let zoom = tasty_settings::AppearanceSettings::ui_scale_factor_for(key);
            let th = with_zoom(theme, theme.ui_zoom * zoom);
            let mut rows = attached_rows(RowState::Inactive);
            rows[0].state = RowState::Active;
            rows.remove(1);
            let caption = format!(
                "ui_scale {zoom} — slot {} (derived)",
                th.workspace_dot_slot().value()
            );
            spec::cluster(ui, theme, &caption, |ui| ws_column(ui, &th, &rows));
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("dot", "8 (status-dot-size)"),
            ("attached bbox", "8 + 2×(2+2) = 16"),
            ("row inset", "8 (workspace-row-padding-x)"),
            (
                "dot slot",
                "16, reserved on every row — derived: round(dot) + 2×(round(ring-width) + round(ring-offset))",
            ),
            (
                "slot height",
                "the title label's own line box — never grows the row",
            ),
            ("slot → body", "4 (workspace-dot-gap)"),
            ("body x", "28 (title · remote pill · subtitle)"),
            ("clearance", "card 8 · accent bar 6 · label 4"),
            (
                "active bar",
                "2, hairline, zoom-exempt (workspace-row-active-bar-width → selection-edge-width)",
            ),
            (
                "ui_scale 0.85",
                "inset 7 · slot 15 (derived) · gap 3 → label x 25 · clear card 7 · bar 5 · label 3",
            ),
            (
                "ui_scale 1.2",
                "inset 10 · slot 18 · gap 5 → label x 33 · row height unchanged",
            ),
        ],
        &[
            TokenChip::without_color("workspace-row-padding-x", "→ space-sm 8"),
            TokenChip::without_color("workspace-dot-slot", "→ size-16"),
            TokenChip::without_color("workspace-dot-gap", "→ space-xs 4"),
            TokenChip::without_color(
                "workspace-row-active-bar-width",
                "→ selection-edge-width 2 (hairline)",
            ),
            TokenChip::new(
                "status-dot-attached-ring",
                "attached ring",
                theme.status_dot_attached_ring().into(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "워크스페이스 행은 일반 점 8을 유지하고 attached ring까지 담는 16 슬롯을 모든 행에 예약한다. ring이 없는 행도 이름·REMOTE 줄·부제가 같은 x에서 시작한다. inset·간격은 토큰마다 반올림하고, 슬롯은 반올림한 점과 ring을 더해 만들어 ring이 항상 슬롯 안에 들어간다. 활성 행의 accent bar는 hairline이라 배율과 무관하게 2다.",
    );
}
