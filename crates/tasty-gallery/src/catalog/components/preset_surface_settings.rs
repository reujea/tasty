//! `preseteditor-settings` specimen — 프리셋 편집기의 **surface 설정 화면**. 디자인
//! `gallery/preset_editor.jsx` 의 `SurfaceSettings` + `SettingsDemo` 상태 프레임 5종을
//! 전사한다(본체 `src/adapters/ui/preset/surface_settings.rs`).
//!
//! 세 상자: 헤더(kind 아이콘 · 표시명 · mono breadcrumb · dirty 일 때 unsaved 점) /
//! 유일하게 스크롤되는 본문(한 열 폼, Kind 다음 kind 선언 필드, dir/file 은 입력 +
//! Browse 한 줄) / 높이 고정 footer(`[Cancel ghost] [OK primary]`, 변경 없으면 OK 비활성).
//!
//! 정적 specimen 이라 입력은 프레임마다 샘플 값으로 돌아간다 — draft·확인·취소는 본체에서만
//! 동작한다. 필드 타입도 본체가 그리는 것(text · dir · file)만 쓴다 — 디자인 데모의
//! `number`/`select` 필드는 본체에 없는 타입이라 text 로 그린다(parity-notes).

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::tokens::STRUCT_GAP_3;
use tasty_ui_widgets::{Button, ButtonVariant, Input, select};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};

// 갤러리 stage 안의 데모 프레임 크기(디자인 `SettingsDemo` width/height) — 본체 치수가
// 아니라 specimen 배치값이다.
const FRAME_W: f32 = 280.0;
const FRAME_NARROW_W: f32 = 220.0;
const FRAME_H: f32 = 300.0;
/// ③ 의 본문 스크롤 위치(디자인 `scrollTo={120}`) — 헤더·footer 가 고정인 것을 보이려는
/// 데모 값.
const SCROLLED_Y: f32 = 120.0;
/// 한 줄에 놓는 상태 프레임 수 — specimen 배치값.
const ROW_LEN: usize = 3;

/// kind 드롭다운 후보(표시명). 본체는 registry 에서 받는다.
const KINDS: &[&str] = &[
    "Terminal",
    "Markdown",
    "Image",
    "Explorer",
    "HTML",
    "Port scanner",
];

struct DemoField {
    label: &'static str,
    value: &'static str,
    placeholder: &'static str,
    browse: bool,
}

const fn field(label: &'static str, value: &'static str, placeholder: &'static str) -> DemoField {
    DemoField {
        label,
        value,
        placeholder,
        browse: false,
    }
}

const fn path_field(
    label: &'static str,
    value: &'static str,
    placeholder: &'static str,
) -> DemoField {
    DemoField {
        label,
        value,
        placeholder,
        browse: true,
    }
}

struct Demo {
    caption: &'static str,
    kind: usize,
    icon: MockGlyph,
    accent: fn(&Theme) -> egui::Color32,
    path: &'static [&'static str],
    fields: Vec<DemoField>,
    dirty: bool,
    scroll: Option<f32>,
    width: f32,
}

fn demos() -> Vec<Demo> {
    vec![
        Demo {
            caption: "1 · terminal — 2 fields",
            kind: 0,
            icon: icons::TERMINAL,
            accent: |t| t.accent_success().to_egui(),
            path: &["six", "pane 1", "code", "surface 2"],
            fields: vec![
                path_field("Working directory", "~/tasty", "~"),
                field("Startup command", "cargo watch -x run", "(none)"),
            ],
            dirty: false,
            scroll: None,
            width: FRAME_W,
        },
        Demo {
            caption: "2 · markdown — 1 field + Browse",
            kind: 1,
            icon: icons::MARKDOWN,
            accent: |t| t.accent_primary().to_egui(),
            path: &["six", "pane 2", "ops", "surface 3"],
            fields: vec![path_field("File", "docs/runbook.md", "README.md")],
            dirty: false,
            scroll: None,
            width: FRAME_W,
        },
        Demo {
            caption: "3 · plugin kind — 7 fields, body scrolls (header + footer stay)",
            kind: 5,
            icon: icons::PORT,
            accent: |t| t.text_secondary().to_egui(),
            path: &["six", "pane 2", "ops", "surface 2"],
            fields: vec![
                field("Host", "127.0.0.1", "127.0.0.1"),
                field("Port range", "3000-3999", "1-65535"),
                field("Protocol", "tcp", "tcp"),
                field("Scan interval", "1000", "1000"),
                field("Connect timeout", "200", "200"),
                path_field("Output file", "", "ports.log"),
                field("Notify on change", "new port", "never"),
            ],
            dirty: false,
            scroll: Some(SCROLLED_Y),
            width: FRAME_W,
        },
        Demo {
            caption: "4 · kind changed → unsaved dot, OK enabled",
            kind: 0,
            icon: icons::TERMINAL,
            accent: |t| t.accent_success().to_egui(),
            path: &["claude", "pane 1", "edit", "surface 1"],
            fields: vec![
                path_field("Working directory", "~/tasty/src", "~"),
                field("Startup command", "", "(none)"),
            ],
            dirty: true,
            scroll: None,
            width: FRAME_W,
        },
        Demo {
            caption: "5 · narrow detail (220px) — breadcrumb ellipsizes, Browse stays",
            kind: 1,
            icon: icons::MARKDOWN,
            accent: |t| t.accent_primary().to_egui(),
            path: &["claude", "pane 2", "preview", "surface 1"],
            fields: vec![path_field("File", "docs/architecture.md", "README.md")],
            dirty: false,
            scroll: None,
            width: FRAME_NARROW_W,
        },
    ]
}

/// 설정 화면 한 벌 — 본체 `draw_surface_settings` 의 세 상자 전사.
fn draw_screen(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, d: &Demo, salt: usize) {
    let bw = theme.border_width.value();
    let header_h = theme.preset_cfg_header_height().value();
    let footer_h = theme.preset_cfg_footer_height().value();
    let header = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), header_h));
    let footer = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - footer_h), rect.max);
    let body = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, header.max.y),
        egui::pos2(rect.max.x, footer.min.y),
    );

    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let sep = egui::Stroke::new(bw, theme.separator.to_egui());
    p.hline(header.x_range(), header.max.y, sep);
    p.hline(footer.x_range(), footer.min.y, sep);

    draw_header(ui, theme, header, d);
    draw_body(ui, theme, body, d, salt);
    draw_footer(ui, theme, footer, d.dirty);
}

fn draw_header(ui: &mut egui::Ui, theme: &Theme, header: egui::Rect, d: &Demo) {
    let inner = header.shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
    let mut hui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    hui.set_clip_rect(header);
    hui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    let muted = theme.text_muted().to_egui();
    let caption = theme.font_size_caption.value();
    if d.dirty {
        hui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
            ui.label(egui::RichText::new("unsaved").size(caption).color(muted));
            let s = theme.status_dot_size_compact().value();
            let (dot, _) = ui.allocate_exact_size(egui::vec2(s, s), egui::Sense::hover());
            ui.painter().circle_filled(
                dot.center(),
                s * 0.5,
                theme.preset_cfg_draft_fg().to_egui(),
            );
        });
    }
    hui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let size = theme.icon_glyph_size_md.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        d.icon.image(size, (d.accent)(theme)).paint_at(ui, r);
        ui.label(
            egui::RichText::new(KINDS[d.kind])
                .size(theme.font_size_max.value())
                .strong()
                .color(theme.text_primary().to_egui()),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(d.path.join(" › "))
                    .monospace()
                    .size(caption)
                    .color(muted),
            )
            .truncate(),
        );
    });
}

fn draw_body(ui: &mut egui::Ui, theme: &Theme, body: egui::Rect, d: &Demo, salt: usize) {
    let mut bui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    bui.set_clip_rect(body);
    let pad = theme.preset_cfg_form_padding().value();
    let mut area = egui::ScrollArea::vertical()
        .id_salt(("preset_cfg_demo_body", salt))
        .auto_shrink([false; 2])
        .drag_to_scroll(false);
    if let Some(y) = d.scroll {
        area = area.vertical_scroll_offset(y);
    }
    area.show(&mut bui, |ui| {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(pad as i8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let w = ui
                    .available_width()
                    .min(theme.preset_cfg_form_max_width().value());
                let gap = theme.preset_cfg_field_gap().value();
                field_label(ui, theme, "Kind");
                let mut kind = d.kind;
                select(
                    ui,
                    theme,
                    &format!("preset_cfg_demo_kind_{salt}"),
                    &mut kind,
                    KINDS,
                    w,
                    true,
                );
                for (i, f) in d.fields.iter().enumerate() {
                    ui.add_space(gap);
                    field_label(ui, theme, f.label);
                    ui.push_id(("preset_cfg_demo_field", salt, i), |ui| {
                        let mut buf = f.value.to_string();
                        if f.browse {
                            ui.allocate_ui_with_layout(
                                egui::vec2(w, theme.input_height().value()),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                                    Button::new("Browse…")
                                        .variant(ButtonVariant::Secondary)
                                        .leading_icon(&|ui, rect, c| {
                                            icons::FOLDER_OPEN
                                                .image(rect.width(), c)
                                                .paint_at(ui, rect);
                                        })
                                        .show(ui, theme);
                                    Input::new()
                                        .mono(true)
                                        .width(ui.available_width())
                                        .placeholder(f.placeholder)
                                        .show(ui, theme, &mut buf);
                                },
                            );
                        } else {
                            Input::new()
                                .mono(true)
                                .width(w)
                                .placeholder(f.placeholder)
                                .show(ui, theme, &mut buf);
                        }
                    });
                }
            });
    });
}

fn draw_footer(ui: &mut egui::Ui, theme: &Theme, footer: egui::Rect, dirty: bool) {
    let inner = footer.shrink2(egui::vec2(theme.preset_cfg_footer_padding_x().value(), 0.0));
    let mut fui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    fui.set_clip_rect(footer);
    fui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    Button::new("OK")
        .variant(ButtonVariant::Primary)
        .enabled(dirty)
        .show(&mut fui, theme);
    Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .show(&mut fui, theme);
}

/// 필드 라벨 — mono micro uppercase muted, 입력과 3px(본체 `field_label` 전사).
fn field_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .monospace()
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
    ui.add_space(STRUCT_GAP_3.value());
}

/// 상태 프레임 한 칸 — 캡션 + 설정 화면 + 외곽선.
fn draw_demo(ui: &mut egui::Ui, theme: &Theme, d: &Demo, salt: usize) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(d.caption)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        let (rect, _) = ui.allocate_exact_size(egui::vec2(d.width, FRAME_H), egui::Sense::hover());
        draw_screen(ui, theme, rect, d, salt);
        ui.painter_at(rect).rect_stroke(
            rect,
            theme.corner_radius.value(),
            egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
            egui::StrokeKind::Inside,
        );
    });
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    // 무대가 본문 컬럼보다 넓어져 wrap 이 안 꺾이므로(다섯 프레임이 한 줄로 흘러 페이지 클립에
    // 잘린다) 줄을 명시한다 — 한 줄 세 프레임이 컬럼 안에 든다.
    let demos = demos();
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for (row, chunk) in demos.chunks(ROW_LEN).enumerate() {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                for (i, d) in chunk.iter().enumerate() {
                    draw_demo(ui, theme, d, row * ROW_LEN + i);
                }
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("boxes", "header / scroll body / fixed footer"),
            ("header", "44px — same as the toolbar it replaces"),
            ("body", "16px padding · 12px field gap · max 460"),
            ("footer", "52px fixed · 0 14px · Cancel ghost + OK primary"),
            ("OK", "disabled until draft ≠ saved"),
            ("keys", "Esc = Cancel · Enter in an input = OK"),
            ("structure keys", "inert while open (preview hidden)"),
            ("after OK/Cancel", "back to preview, leaf stays selected"),
        ],
        &[
            TokenChip::new(
                "preset-cfg-draft-fg",
                "unsaved dot (accent-warning)",
                theme.preset_cfg_draft_fg().to_egui(),
            ),
            TokenChip::new("bg-panel", "screen fill", theme.bg_panel().to_egui()),
            TokenChip::new(
                "separator",
                "header / footer rule",
                theme.separator.to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Editing a leaf's parameters (kind · cwd · startup · whatever the kind declares) takes \
         over the whole detail column — toolbar and preview — as three stacked boxes: a header \
         (kind icon in its accent + kind name · mono breadcrumb preset › pane N › tab › surface k, \
         pane N only in workspace scope · an unsaved dot once the draft differs), a scrolling body \
         (one-column form, label over full-width input, capped at 460; Kind first, then the kind's \
         fields — dir / file get Browse on the same row) and a fixed footer (right-aligned Cancel \
         ghost · OK primary). The screen is a draft: OK applies the whole surface at once and \
         saves, Cancel discards. OK is disabled until something changed. While it is open the \
         preset list and the scope tabs dim to the disabled opacity and ignore input. Open it with \
         the selected leaf's gear handle or a double-click.",
    );

    spec::do_(
        ui,
        theme,
        "Keep the three boxes independent: only the body scrolls; header and footer never move, \
         so Cancel / OK sit in the same place for a two-field terminal and a many-field plugin.",
    );

    spec::dont(
        ui,
        theme,
        "Don't show \"saved automatically\" while the screen is open — it is not true for the \
         draft. The toolbar is replaced wholesale, so the two save models never appear side by \
         side; the header's unsaved dot is the only persistence cue on this screen.",
    );
}
