//! 클립보드 플러그인 화면의 정적 예제. 헤더 → 타입 선택 → 본문 → 푸터를 재현한다.
//! 플러그인 프로세스를 실행하지 않으므로 실제 데이터·픽셀 일치는 검증하지 않는다.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{TagVariant, TypeSegment, checkbox, draw_type_segments, tag};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// popup 본문 치수(디자인 480×360 고정 — size_hint). Theme 에 대응 토큰이 없는
/// 화면 전용 고정값.
const POPUP_W: LogicalPx = LogicalPx(480.0);
const POPUP_H: LogicalPx = LogicalPx(360.0);

use tasty_ui_widgets::tokens::CLIPBOARD_CENTER_ICON_SIZE as CENTER_ICON_SIZE;

/// compact type-bar 의 다섯 세그먼트 — (아이콘, 라벨). `ClipboardType` 의 다섯 arm 과 같은
/// 순서이고 아이콘도 plugin `type_icon` 과 같은 짝이다. 선택은 Files 다.
const COMPACT_TYPES: &[(MockGlyph, &str)] = &[
    (icons::TEXT_LEFT, "Text"),
    (icons::FILE, "Files"),
    (icons::IMAGE, "Image"),
    (icons::HTML, "Html"),
    (icons::LAYERS, "Other"),
];

/// 두 타입(Text/Files) 세그먼트, 선택은 Files.
const SEGMENTED_TYPES: &[(MockGlyph, &str)] = &[(icons::TEXT_LEFT, "Text"), (icons::FILE, "Files")];

/// body well 안 mono 미리보기 샘플 — 현재 클립보드 text 표현.
const PREVIEW: &[&str] = &[
    "cargo build -p tasty-gallery",
    "git switch wt-5/T8-code",
    "tasty read screen --surface 3",
];

/// files 상태 body 미리보기 샘플 — 파일 탐색기에서 복사한 경로 목록.
const FILE_PREVIEW: &[&str] = &[
    "/home/user/Documents/report.pdf",
    "/home/user/Pictures/screenshot-2026-07-30.png",
    "/home/user/workspace/tasty/Cargo.toml",
];

/// HTML 원본과 태그 깊이에 맞춰 들여쓴 예제. 플러그인 포매터를 호출하지 않는다.
const HTML_RAW: &str = "<div class=\"card\"><p>Hello <b>world</b></p></div>";
const HTML_PRETTY: &str =
    "<div class=\"card\">\n  <p>\n    Hello\n    <b>\n      world\n    </b>\n  </p>\n</div>";

/// 기타 포맷의 이름·크기·미리보기·생략 줄 수를 담는 예제 데이터.
struct OtherSample {
    name: &'static str,
    size: &'static str,
    preview: &'static str,
    /// 생략된 줄 수. `Some(n)`이면 `+n more lines`를 표시한다.
    more_lines: Option<usize>,
}

/// text/files/image/html 어디에도 안 걸린 raw 포맷 예시 2종 — 하나는 짧은 순수 텍스트
/// (Windows 드래그앤드롭 힌트류), 하나는 절삭이 필요한 긴 JSON(커스텀 앱 전용 포맷).
const OTHER_SAMPLES: &[OtherSample] = &[
    OtherSample {
        name: "Files Drop Effect",
        size: "4 B",
        preview: "DROPEFFECT_COPY",
        more_lines: None,
    },
    OtherSample {
        name: "Custom App Format",
        size: "1.2 KB",
        preview: "{\"id\":\"note-8842\",\"kind\":\"reference\"}\n{\"id\":\"note-8843\",\"kind\":\"reference\"}\n{\"id\":\"note-8844\",\"kind\":\"reference\"}",
        more_lines: Some(6),
    },
];

thread_local! {
    static HTML_RAW_PRETTY_ON: RefCell<bool> = const { RefCell::new(false) };
    static HTML_PRETTY_PRETTY_ON: RefCell<bool> = const { RefCell::new(true) };
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    // 고정 폭 카드가 가로 스크롤 밖으로 잘리지 않도록 세로로 나열한다.
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        spec::cluster(
            ui,
            theme,
            "text only — header / type-bar(badge) / body / footer",
            |ui| data_popup(ui, theme),
        );
        spec::cluster(
            ui,
            theme,
            "files — type-bar(segmented Text/Files) / body(icon+path rows)",
            |ui| files_popup(ui, theme),
        );
        spec::cluster(
            ui,
            theme,
            "compact — five types, inactive segments are icon-only",
            |ui| compact_popup(ui, theme),
        );
        spec::cluster(
            ui,
            theme,
            "image — icon + meta + \"no inline preview\"",
            |ui| image_popup(ui, theme),
        );
        spec::cluster(
            ui,
            theme,
            "html — raw source (Pretty print unchecked)",
            |ui| data_popup_html(ui, theme, &HTML_RAW_PRETTY_ON, HTML_RAW),
        );
        spec::cluster(
            ui,
            theme,
            "html — pretty print (Pretty print checked)",
            |ui| data_popup_html(ui, theme, &HTML_PRETTY_PRETTY_ON, HTML_PRETTY),
        );
        spec::cluster(
            ui,
            theme,
            "other — raw format bucket (unrecognized formats)",
            |ui| other_popup(ui, theme),
        );
        spec::cluster(ui, theme, "empty clipboard", |ui| {
            center_popup(
                ui,
                theme,
                icons::CLIPBOARD,
                "Clipboard is empty",
                "Copy some text, an image, or files and reopen to see a snapshot here.",
                false,
            );
        });
        spec::cluster(ui, theme, "read failed", |ui| {
            center_popup(
                ui,
                theme,
                icons::ALERT_TRIANGLE,
                "Couldn't read the clipboard",
                "The system clipboard handle could not be opened. Close another app that may be holding it and reopen.",
                true,
            );
        });
        spec::cluster(ui, theme, "already open", |ui| {
            center_popup(
                ui,
                theme,
                icons::LOCK,
                "Clipboard viewer is already open",
                "Only one snapshot window runs at a time — the existing one was brought to the front.",
                false,
            );
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "480×360 popup · bg-panel"),
            ("header", "icon + title(14/600) + snapshot tag + close"),
            (
                "type-bar",
                "≤1: icon+tag(accent) · ≥2: segmented(border-default)",
            ),
            ("body", "well(border+radius+bg-app) · mono scroll"),
            (
                "footer",
                "mime(mono caption)[+meta for html] + Close(secondary)",
            ),
            (
                "html type-bar right slot",
                "Pretty print checkbox swaps in for meta text",
            ),
            (
                "states",
                "data(text) · data(files) · image · html(raw/pretty) · other · empty · read-failed · already-open",
            ),
        ],
        &[
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::new("bg-app", "body well fill", theme.bg_app().to_egui()),
            TokenChip::new(
                "bg-sidebar",
                "type-bar row fill",
                theme.bg_sidebar().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "active segment / badge",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "separator",
                "row divider",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "accent-danger",
                "read-failed tone",
                theme.accent_danger().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "타입이 하나면 배지, 둘 이상이면 가로 선택 목록으로 표시한다. 다섯 타입이 모두 있으면 \
         선택되지 않은 타입은 아이콘만 남긴다. 파일 경로는 한 줄로 줄여 표시하고, 이미지는 \
         실제 미리보기 없이 아이콘과 치수·크기만 보여준다.\n\n\
         HTML은 렌더링하지 않고 소스를 표시한다. Pretty print는 미리 준비한 들여쓰기 예제를 \
         선택하며 플러그인 포매터를 실행하지 않는다. HTML의 문자·줄 수는 푸터에 \
         text/html · N chars · N line(s)로 표시한다.\n\n\
         Other는 앞의 네 타입에 속하지 않는 포맷이다. 이름·크기·미리보기를 포맷별로 나열하며 \
         포맷 목록은 접지 않는다. 긴 미리보기는 +N more lines로 생략량을 표시하고 푸터에는 \
         {n} unrecognized formats를 표시한다. 플랫폼별 클립보드 열거는 이 예제에서 실행하지 않는다.",
    );
}

/// 디자인 예제의 이미지 치수·바이트 크기.
const IMAGE_META: &str = "1920×1080 · 7.9 MB";

fn data_popup(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        type_bar_row(ui, theme);
        body_row(ui, theme);
        footer_row(ui, theme, "text/plain");
    });
}

fn files_popup(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        type_bar_segmented_row(ui, theme);
        files_body_row(ui, theme);
        footer_row(ui, theme, "text/uri-list");
    });
}

fn compact_popup(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        type_bar_compact_row(ui, theme);
        files_body_row(ui, theme);
        footer_row(ui, theme, "text/uri-list");
    });
}

fn image_popup(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        image_type_bar_row(ui, theme);
        image_body_row(ui, theme);
        footer_row(ui, theme, "image/rgba8");
    });
}

/// type-bar — 타입 2개(Text/Files) 가로 세그먼트, active=Files(accent 채움).
fn type_bar_segmented_row(ui: &mut egui::Ui, theme: &Theme) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    type_segments(&mut lui, theme, SEGMENTED_TYPES, 1);

    hline(ui, theme, rect.bottom());
}

fn image_type_bar_row(ui: &mut egui::Ui, theme: &Theme) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    lui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(
        &mut lui,
        icons::IMAGE,
        theme.icon_glyph_size_sm,
        theme.text_muted().to_egui(),
    );
    tag(&mut lui, theme, "Image", TagVariant::Accent, false);

    let mut rui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    rui.label(
        egui::RichText::new(IMAGE_META)
            .monospace()
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );

    hline(ui, theme, rect.bottom());
}

/// 다섯 타입을 함께 표시한다. 선택되지 않은 타입은 아이콘만 남긴다.
fn type_bar_compact_row(ui: &mut egui::Ui, theme: &Theme) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    type_segments(&mut lui, theme, COMPACT_TYPES, 1);

    hline(ui, theme, rect.bottom());
}

/// plugin 과 같은 공용 세그먼트 view 를 부른다. 아이콘만 갤러리 SVG 글리프로 그린다.
fn type_segments(ui: &mut egui::Ui, theme: &Theme, types: &[(MockGlyph, &str)], active: usize) {
    let segments: Vec<TypeSegment<'_>> = types
        .iter()
        .map(|(_, label)| TypeSegment {
            label,
            tooltip: label,
        })
        .collect();
    draw_type_segments(ui, theme, &segments, active, &|ui, i, center, size, fg| {
        let rect = egui::Rect::from_center_size(center, egui::vec2(size, size));
        types[i].0.image(size, fg).paint_at(ui, rect);
    });
}

fn files_body_row(ui: &mut egui::Ui, theme: &Theme) {
    let footer_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let header_h = theme.spacing_md.scaled(2.0) + theme.item_height_tab;
    let type_bar_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let h = POPUP_H - header_h - type_bar_h - footer_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());

    let margin = theme.spacing_md.value();
    let well = rect.shrink(margin);
    let p = ui.painter();
    p.rect(
        well,
        theme.corner_radius.value(),
        theme.bg_app().to_egui(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );

    let icon_sz = theme.icon_glyph_size_sm.value();
    let gap = theme.spacing_sm.value();
    let tx = well.left() + theme.spacing_sm.value();
    let mut ty = well.top() + theme.spacing_sm.value();
    let line_h = icon_sz.max(theme.font_size_term_sm.value()) + theme.spacing_xs.value();
    for path in FILE_PREVIEW {
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(tx + icon_sz * 0.5, ty + line_h * 0.5),
            egui::vec2(icon_sz, icon_sz),
        );
        icons::FILE
            .image(icon_sz, theme.text_muted().to_egui())
            .paint_at(ui, icon_rect);
        p.text(
            egui::pos2(tx + icon_sz + gap, ty + line_h * 0.5),
            egui::Align2::LEFT_CENTER,
            path,
            egui::FontId::monospace(theme.font_size_term_sm.value()),
            theme.text_primary().to_egui(),
        );
        ty += line_h;
    }
}

fn image_body_row(ui: &mut egui::Ui, theme: &Theme) {
    let footer_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let header_h = theme.spacing_md.scaled(2.0) + theme.item_height_tab;
    let type_bar_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let h = POPUP_H - header_h - type_bar_h - footer_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());

    let margin = theme.spacing_md.value();
    let well = rect.shrink(margin);
    let p = ui.painter();
    p.rect(
        well,
        theme.corner_radius.value(),
        theme.bg_app().to_egui(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );

    // 빈 상태와 같은 아이콘 크기를 쓴다.
    let gap = theme.spacing_sm.value();
    let icon_h = CENTER_ICON_SIZE;
    let meta_h = theme.font_size_caption.value();
    let sub_h = theme.font_size_caption.value();
    let block_h = icon_h + gap + meta_h + theme.spacing_xs.value() + sub_h;
    let mut y = well.center().y - block_h / 2.0;

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(well.center().x, y + icon_h / 2.0),
        egui::vec2(icon_h, icon_h),
    );
    icons::IMAGE
        .image(icon_h, theme.text_muted().to_egui())
        .paint_at(ui, icon_rect);
    y += icon_h + gap;

    ui.painter().text(
        egui::pos2(well.center().x, y),
        egui::Align2::CENTER_TOP,
        IMAGE_META,
        egui::FontId::monospace(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
    y += meta_h + theme.spacing_xs.value();

    ui.painter().text(
        egui::pos2(well.center().x, y),
        egui::Align2::CENTER_TOP,
        "No inline image preview",
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_disabled().to_egui(),
    );
}

fn other_popup(ui: &mut egui::Ui, theme: &Theme) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        other_type_bar_row(ui, theme);
        other_body_row(ui, theme);
        footer_row(
            ui,
            theme,
            format!("{} unrecognized formats", OTHER_SAMPLES.len()),
        );
    });
}

fn other_type_bar_row(ui: &mut egui::Ui, theme: &Theme) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    lui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(
        &mut lui,
        icons::LAYERS,
        theme.icon_glyph_size_sm,
        theme.text_muted().to_egui(),
    );
    tag(&mut lui, theme, "Other", TagVariant::Accent, false);

    hline(ui, theme, rect.bottom());
}

/// 포맷별 미리보기를 나열한다. 본문이 스크롤되므로 포맷 목록은 접지 않는다.
fn other_body_row(ui: &mut egui::Ui, theme: &Theme) {
    let footer_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let header_h = theme.spacing_md.scaled(2.0) + theme.item_height_tab;
    let type_bar_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let h = POPUP_H - header_h - type_bar_h - footer_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());

    let margin = theme.spacing_md.value();
    let well = rect.shrink(margin);
    let p = ui.painter();
    p.rect(
        well,
        theme.corner_radius.value(),
        theme.bg_app().to_egui(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );

    let tx = well.left() + theme.spacing_sm.value();
    let mut ty = well.top() + theme.spacing_sm.value();
    let name_font = egui::FontId::monospace(theme.font_size_caption.value());
    let name_h = theme.font_size_caption.value();
    let line_h = theme.font_size_term_sm.value() + theme.spacing_xs.value();

    for (i, sample) in OTHER_SAMPLES.iter().enumerate() {
        if i > 0 {
            ty += theme.spacing_sm.value();
            p.hline(
                egui::Rangef::new(well.left(), well.right()),
                ty,
                egui::Stroke::new(
                    theme.border_width.value(),
                    theme.separator.to_egui_premultiplied(),
                ),
            );
            ty += theme.spacing_sm.value();
        }

        let name_w = ui
            .fonts(|f| {
                f.layout_no_wrap(
                    sample.name.to_owned(),
                    name_font.clone(),
                    egui::Color32::PLACEHOLDER,
                )
            })
            .size()
            .x;
        p.text(
            egui::pos2(tx, ty),
            egui::Align2::LEFT_TOP,
            sample.name,
            name_font.clone(),
            theme.text_secondary().to_egui(),
        );
        p.text(
            egui::pos2(tx + name_w + theme.spacing_sm.value(), ty),
            egui::Align2::LEFT_TOP,
            sample.size,
            name_font.clone(),
            theme.text_muted().to_egui(),
        );
        ty += name_h + theme.spacing_xs.value();

        for line in sample.preview.lines() {
            p.text(
                egui::pos2(tx, ty),
                egui::Align2::LEFT_TOP,
                line,
                egui::FontId::monospace(theme.font_size_term_sm.value()),
                theme.text_primary().to_egui(),
            );
            ty += line_h;
        }
        if let Some(n) = sample.more_lines {
            p.text(
                egui::pos2(tx, ty),
                egui::Align2::LEFT_TOP,
                format!("+{n} more lines"),
                name_font.clone(),
                theme.text_muted().to_egui(),
            );
            ty += line_h;
        }
    }
}

fn data_popup_html(
    ui: &mut egui::Ui,
    theme: &Theme,
    pretty_on: &'static std::thread::LocalKey<RefCell<bool>>,
    content: &str,
) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        type_bar_row_html(ui, theme, pretty_on);
        body_row_text(ui, theme, content);
        footer_row_html(ui, theme, content);
    });
}

fn header_row(ui: &mut egui::Ui, theme: &Theme) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_md.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    lui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(
        &mut lui,
        icons::CLIPBOARD,
        theme.icon_glyph_size_md,
        theme.text_muted().to_egui(),
    );
    lui.label(
        egui::RichText::new("Clipboard")
            .size(theme.font_size_max.value())
            .strong()
            .color(theme.text_primary().to_egui()),
    );
    tag(&mut lui, theme, "snapshot", TagVariant::Default, false);

    let close_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + pad_y),
        egui::pos2(rect.right() - pad_x, rect.top() + pad_y + ctrl_h),
    );
    let mut rui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(close_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    kit::icon(
        &mut rui,
        icons::CLOSE,
        theme.icon_glyph_size_sm,
        theme.text_secondary().to_egui(),
    );

    hline(ui, theme, rect.bottom());
}

/// type-bar — 타입이 1개(Text)뿐인 상태는 세그먼트가 아니라 아이콘+accent 뱃지.
fn type_bar_row(ui: &mut egui::Ui, theme: &Theme) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    lui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(
        &mut lui,
        icons::TEXT_LEFT,
        theme.icon_glyph_size_sm,
        theme.text_muted().to_egui(),
    );
    tag(&mut lui, theme, "Text", TagVariant::Accent, false);

    hline(ui, theme, rect.bottom());
}

/// HTML의 Pretty print 선택은 갤러리 내부 상태만 바꾼다.
fn type_bar_row_html(
    ui: &mut egui::Ui,
    theme: &Theme,
    pretty_on: &'static std::thread::LocalKey<RefCell<bool>>,
) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());

    let content = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    let mut lui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    lui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    kit::icon(
        &mut lui,
        icons::HTML,
        theme.icon_glyph_size_sm,
        theme.text_muted().to_egui(),
    );
    tag(&mut lui, theme, "HTML", TagVariant::Accent, false);

    let mut rui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    pretty_on.with_borrow_mut(|checked| {
        checkbox(
            &mut rui,
            theme,
            checked,
            "Pretty print",
            true,
            tasty_ui_widgets::Ground::Panel,
        );
    });

    hline(ui, theme, rect.bottom());
}

fn body_row(ui: &mut egui::Ui, theme: &Theme) {
    let footer_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let header_h = theme.spacing_md.scaled(2.0) + theme.item_height_tab;
    let type_bar_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let h = POPUP_H - header_h - type_bar_h - footer_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());

    let margin = theme.spacing_md.value();
    let well = rect.shrink(margin);
    let p = ui.painter();
    p.rect(
        well,
        theme.corner_radius.value(),
        theme.bg_app().to_egui(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );

    let tx = well.left() + theme.spacing_sm.value();
    let mut ty = well.top() + theme.spacing_sm.value();
    let line_h = theme.font_size_term_sm.value() + theme.spacing_xs.value();
    for line in PREVIEW {
        p.text(
            egui::pos2(tx, ty),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(theme.font_size_term_sm.value()),
            theme.text_primary().to_egui(),
        );
        ty += line_h;
    }
}

fn body_row_text(ui: &mut egui::Ui, theme: &Theme, content: &str) {
    let footer_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let header_h = theme.spacing_md.scaled(2.0) + theme.item_height_tab;
    let type_bar_h = theme.spacing_sm.scaled(2.0) + theme.item_height_tab;
    let h = POPUP_H - header_h - type_bar_h - footer_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());

    let margin = theme.spacing_md.value();
    let well = rect.shrink(margin);
    let p = ui.painter();
    p.rect(
        well,
        theme.corner_radius.value(),
        theme.bg_app().to_egui(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );

    let tx = well.left() + theme.spacing_sm.value();
    let mut ty = well.top() + theme.spacing_sm.value();
    let line_h = theme.font_size_term_sm.value() + theme.spacing_xs.value();
    for line in content.lines() {
        p.text(
            egui::pos2(tx, ty),
            egui::Align2::LEFT_TOP,
            line,
            egui::FontId::monospace(theme.font_size_term_sm.value()),
            theme.text_primary().to_egui(),
        );
        ty += line_h;
    }
}

/// HTML MIME 뒤에 문자 수와 줄 수를 붙인다.
fn footer_row_html(ui: &mut egui::Ui, theme: &Theme, content: &str) {
    let chars = content.chars().count();
    let lines = content.lines().count().max(1);
    let word = if lines == 1 { "line" } else { "lines" };
    footer_row(
        ui,
        theme,
        format!("text/html · {chars} chars · {lines} {word}"),
    );
}

/// 상태별 MIME·요약 라벨과 Close 버튼에 공통 레이아웃을 쓴다.
fn footer_row(ui: &mut egui::Ui, theme: &Theme, mime: impl ToString) {
    let pad_x = theme.spacing_md.value();
    let pad_y = theme.spacing_sm.value();
    let ctrl_h = theme.item_height_tab.value();
    let h = pad_y * 2.0 + ctrl_h;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.top() + theme.border_width.value() * 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );

    // MIME은 값이라 고정폭이고 뒤의 메타데이터는 번역 문구라 run을 나눈다.
    let text = mime.to_string();
    let (mime, words) = text.split_once(" · ").unwrap_or((&text, ""));
    let size = theme.font_size_caption.value();
    let color = theme.text_muted_raised().to_egui();
    let mut job = egui::text::LayoutJob::default();
    job.append(
        mime,
        0.0,
        egui::TextFormat::simple(egui::FontId::monospace(size), color),
    );
    if !words.is_empty() {
        job.append(
            &format!(" · {words}"),
            0.0,
            egui::TextFormat::simple(tasty_ui_widgets::label_font(size), color),
        );
    }
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(rect.left() + pad_x, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );

    let btn_w = 64.0;
    let btn_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - pad_x - btn_w, rect.top() + pad_y),
        egui::pos2(rect.right() - pad_x, rect.top() + pad_y + ctrl_h),
    );
    ui.painter().rect(
        btn_rect,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        "Close",
        egui::FontId::proportional(theme.font_size_term_sm.value()),
        theme.text_secondary_raised().to_egui(),
    );
}

/// CenterState — 아이콘(28px) + 굵은 타이틀 + 옅은 부제 2줄, popup 높이 절반 정도.
fn center_popup(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: MockGlyph,
    title: &str,
    sub: &str,
    danger: bool,
) {
    kit::frame_card(ui, theme, POPUP_W, kit::panel_fill(theme), |ui| {
        header_row(ui, theme);
        let h = POPUP_H / 2.0;
        let w = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(w, h.value().round()),
            egui::Layout::centered_and_justified(egui::Direction::TopDown),
            |ui| {
                ui.vertical_centered(|ui| {
                    // 빈 상태 아이콘 톤 — 디자인 opacity 0.9(danger)/0.5(muted). 대응 토큰 없음.
                    const EMPTY_ICON_DANGER_OPACITY: f32 = 0.9;
                    const EMPTY_ICON_MUTED_OPACITY: f32 = 0.5;
                    let tint = if danger {
                        theme
                            .accent_danger()
                            .to_egui()
                            .gamma_multiply(EMPTY_ICON_DANGER_OPACITY)
                    } else {
                        theme
                            .text_muted()
                            .to_egui()
                            .gamma_multiply(EMPTY_ICON_MUTED_OPACITY)
                    };
                    kit::icon(ui, glyph, LogicalPx(CENTER_ICON_SIZE), tint);
                    ui.add_space(theme.spacing_sm.value());
                    let title_color = if danger {
                        theme.accent_danger().to_egui()
                    } else {
                        theme.text_secondary().to_egui()
                    };
                    ui.label(
                        egui::RichText::new(title)
                            .size(theme.font_size_body.value())
                            .strong()
                            .color(title_color),
                    );
                    ui.add_space(theme.spacing_xs.value());
                    ui.label(
                        egui::RichText::new(sub)
                            .size(theme.font_size_term_sm.value())
                            .color(theme.text_muted().to_egui()),
                    );
                });
            },
        );
    });
}

fn hline(ui: &mut egui::Ui, theme: &Theme, y: f32) {
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        y - theme.border_width.value() * 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}
