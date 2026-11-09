//! 마크다운의 제목·본문·목록·이미지·코드·표·인용·알림 예제.
//! WebView나 플러그인 렌더러를 실행하지 않고 egui로 근사한다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{checkbox, surface_bed};

use super::callout_kinds::{Callout, callout_box};
use super::{DOC_W, rich};
use crate::catalog::icons;

pub(super) fn document(ui: &mut egui::Ui, theme: &Theme, focused: bool) {
    egui::Frame::new()
        .fill(surface_bed(theme, focused))
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_lg.value() as i8,
            theme.spacing_md.value() as i8,
        ))
        .show(ui, |ui| {
            ui.set_width((DOC_W - theme.spacing_lg.scaled(2.0)).value());
            ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();

            heading(ui, theme, 1, "Markdown surface");
            body_inline(
                ui,
                theme,
                &[
                    Run::text("A read-only viewer that reloads on file change. Inline runs: "),
                    Run::strong("bold"),
                    Run::text(", "),
                    Run::italic("italic"),
                    Run::text(", "),
                    Run::strike("strikethrough"),
                    Run::text(", a "),
                    Run::link("link"),
                    Run::text(", and "),
                    Run::code("inline code"),
                    Run::text("."),
                ],
            );

            heading(ui, theme, 2, "Headings & emphasis");
            body_inline(
                ui,
                theme,
                &[Run::text(
                    "Body uses font-size-body at the 14px cap, line-height 1.6.",
                )],
            );
            heading(ui, theme, 3, "Lists");
            bullet_row(
                ui,
                theme,
                0,
                "•",
                "Bullet item with wrapped text for rhythm.",
            );
            bullet_row(ui, theme, 1, "◦", "Nested bullet");
            bullet_row(ui, theme, 0, "1.", "Ordered item");
            task_row(ui, theme, true, "Task done");
            task_row(ui, theme, false, "Task to do");

            heading(ui, theme, 3, "Code block");
            code_block(ui, theme, &rust_snippet_tokens());

            heading(ui, theme, 3, "Image");
            image_block(ui, theme, "Referenced with a relative path — resolved against the md file's own directory, e.g. ![alt](./screenshot.png).");

            heading(ui, theme, 3, "Table");
            table(
                ui,
                theme,
                focused,
                &[["surface", "viewer", "12"], ["popup", "overlay", "8"]],
            );

            heading(ui, theme, 3, "Blockquote");
            blockquote(ui, theme);

            heading(ui, theme, 3, "Callouts");
            callouts(ui, theme, focused);

            heading(ui, theme, 4, "Subsection (h4)");
            heading(ui, theme, 5, "Minor note (h5)");
            heading(ui, theme, 6, "Label (h6)");

            ui.add_space(theme.spacing_sm.value());
            hr(ui, theme);
            ui.add_space(theme.spacing_xs.value());
            ui.label(rich(
                theme,
                "Horizontal rule above · trailing space below.",
                theme.font_size_caption.value(),
                theme.text_muted().to_egui(),
            ));
        });
}

/// heading 한 줄 — 디자인 MD_H 전사. 크기는 렌더러와 같은 6단계 보간이고 모든 단계가 text-primary 다.
fn heading(ui: &mut egui::Ui, theme: &Theme, level: u8, text: &str) {
    let idx = usize::from(level.clamp(1, 6)) - 1;
    let size = super::css_path::heading_sizes(theme)[idx];
    let top = match level {
        1 => 0.0,
        2 => theme.spacing_md.value(),
        3 | 4 => theme.spacing_sm.value(),
        _ => theme.spacing_xs.value(),
    };
    ui.add_space(top);
    ui.label(
        egui::RichText::new(text)
            .size(size)
            .color(theme.text_primary().to_egui()),
    );
}

/// 한 inline run.
struct Run {
    text: String,
    kind: RunKind,
}
enum RunKind {
    Text,
    Strong,
    Italic,
    Strike,
    Code,
    Link,
}
impl Run {
    fn text(s: &str) -> Self {
        Self {
            text: s.into(),
            kind: RunKind::Text,
        }
    }
    fn strong(s: &str) -> Self {
        Self {
            text: s.into(),
            kind: RunKind::Strong,
        }
    }
    fn italic(s: &str) -> Self {
        Self {
            text: s.into(),
            kind: RunKind::Italic,
        }
    }
    fn strike(s: &str) -> Self {
        Self {
            text: s.into(),
            kind: RunKind::Strike,
        }
    }
    fn code(s: &str) -> Self {
        Self {
            text: s.into(),
            kind: RunKind::Code,
        }
    }
    fn link(s: &str) -> Self {
        Self {
            text: s.into(),
            kind: RunKind::Link,
        }
    }
}

/// 본문 문단 — inline run 들을 wrap 행에 배치(공백은 run 텍스트에 포함).
fn body_inline(ui: &mut egui::Ui, theme: &Theme, runs: &[Run]) {
    let body = theme.font_size_body.value();
    let secondary = theme.text_secondary_raised().to_egui();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, body * 0.6);
        for r in runs {
            let rt = match r.kind {
                RunKind::Text => egui::RichText::new(&r.text).size(body).color(secondary),
                RunKind::Strong => egui::RichText::new(&r.text)
                    .size(body)
                    .color(theme.text_primary().to_egui()),
                RunKind::Italic => egui::RichText::new(&r.text)
                    .size(body)
                    .color(secondary)
                    .italics(),
                RunKind::Strike => egui::RichText::new(&r.text)
                    .size(body)
                    .color(theme.text_muted_raised().to_egui())
                    .strikethrough(),
                RunKind::Code => egui::RichText::new(&r.text)
                    .monospace()
                    .size(body)
                    .color(theme.text_primary().to_egui())
                    .background_color(theme.surface_raised().to_egui()),
                RunKind::Link => egui::RichText::new(&r.text)
                    .size(body)
                    .color(theme.accent_primary_raised().to_egui())
                    .underline(),
            };
            ui.label(rt);
        }
    });
}

/// bullet/ordered 리스트 한 행 — marker(muted) + 본문(secondary).
fn bullet_row(ui: &mut egui::Ui, theme: &Theme, depth: usize, marker: &str, text: &str) {
    let body = theme.font_size_body.value();
    ui.horizontal(|ui| {
        ui.add_space(theme.spacing_lg.value() + depth as f32 * theme.spacing_lg.value());
        ui.label(rich(theme, marker, body, theme.text_muted().to_egui()));
        ui.add_space(theme.spacing_xs.value());
        ui.label(rich(theme, text, body, theme.text_secondary().to_egui()));
    });
}

/// task 리스트 한 행 — 16px checkbox + 본문.
fn task_row(ui: &mut egui::Ui, theme: &Theme, mut done: bool, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(theme.spacing_lg.value());
        checkbox(
            ui,
            theme,
            &mut done,
            text,
            false,
            tasty_ui_widgets::Ground::Panel,
        );
    });
}

/// 이미지 파일을 읽지 않고 대체 영역과 alt 설명을 그린다.
fn image_block(ui: &mut egui::Ui, theme: &Theme, alt: &str) {
    let (w, h) = (200.0, 120.0);
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "image",
                egui::FontId::monospace(theme.font_size_caption.value()),
                theme.text_muted_raised().to_egui(),
            );
        });
    ui.add_space(theme.spacing_xs.value());
    ui.label(rich(
        theme,
        alt,
        theme.font_size_caption.value(),
        theme.text_muted_raised().to_egui(),
    ));
}

/// 플러그인 hljs_css와 같은 색을 쓰는 구문 종류. 실제 highlight.js는 실행하지 않는다.
#[derive(Clone, Copy)]
enum TokenKind {
    Plain,
    Keyword,
    Title,
    Builtin,
    String,
}

/// One token's text + [`TokenKind`] — a code-block line is a `Vec<CodeToken>`.
struct CodeToken(&'static str, TokenKind);

/// 미리 나눈 코드 토큰 예제. 실제 구문 분석은 하지 않는다.
fn rust_snippet_tokens() -> [Vec<CodeToken>; 3] {
    [
        vec![
            CodeToken("fn ", TokenKind::Keyword),
            CodeToken("main", TokenKind::Title),
            CodeToken("() {", TokenKind::Plain),
        ],
        vec![
            CodeToken("    ", TokenKind::Plain),
            CodeToken("format!", TokenKind::Builtin),
            CodeToken("(", TokenKind::Plain),
            CodeToken("\"hi from tasty\"", TokenKind::String),
            CodeToken(");", TokenKind::Plain),
        ],
        vec![CodeToken("}", TokenKind::Plain)],
    ]
}

/// 토큰별 색을 적용해 구문 강조 결과를 근사한다.
fn code_block(ui: &mut egui::Ui, theme: &Theme, lines: &[Vec<CodeToken>]) {
    let body = theme.font_size_body.value();
    let token_color = |kind: TokenKind| match kind {
        TokenKind::Plain => theme.text_secondary_raised().to_egui(),
        TokenKind::Keyword => theme.mauve.to_egui(),
        TokenKind::Title => theme.blue.to_egui(),
        TokenKind::Builtin => theme.red.to_egui(),
        TokenKind::String => theme.green.to_egui(),
    };
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_sm.value() as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width() - theme.spacing_sm.value() * 2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in lines {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for CodeToken(text, kind) in line {
                        ui.label(
                            egui::RichText::new(*text)
                                .monospace()
                                .size(body)
                                .color(token_color(*kind)),
                        );
                    }
                });
            }
        });
}

/// 표 row·zebra 색. 비포커스에서 맞바꾼다. 바탕과 이 둘 말고는 포커스로 바뀌지 않는다.
pub(crate) fn table_fills(theme: &Theme, focused: bool) -> (egui::Color32, egui::Color32) {
    if focused {
        (
            theme.md_table_row_bg().to_egui(),
            theme.md_table_row_bg_zebra().to_egui(),
        )
    } else {
        (
            theme.md_table_row_bg_unfocused().to_egui(),
            theme.md_table_row_bg_zebra_unfocused().to_egui(),
        )
    }
}

/// grid + zebra 테이블 (md-table 토큰) — header(surface-raised) 밴드 + base/zebra 본문
/// 외곽·가로·세로 격자선은 border-strong을 사용한다.
pub(crate) fn table(ui: &mut egui::Ui, theme: &Theme, focused: bool, rows: &[[&str; 3]]) {
    let (row_bg, zebra_bg) = table_fills(theme, focused);
    let body = theme.font_size_body.value();
    let pad_x = theme.md_table_cell_padding_x().value();
    let pad_y = theme.md_table_cell_padding_y().value();
    // 세로 격자선 양쪽에 셀 패딩(pad_x)이 대칭으로 오도록 컬럼 간격을 2*pad_x 로.
    let col_gap = pad_x * 2.0;
    let border_w = theme.border_width.value();
    let border = theme.md_table_border().to_egui();
    let r = theme.corner_radius.value() as u8;
    let margin = egui::Margin::symmetric(pad_x as i8, pad_y as i8);
    let cols = 3usize;

    let cell = |ui: &mut egui::Ui, i: usize, text: &str, color: egui::Color32| {
        let lay = if i == 2 {
            egui::Layout::right_to_left(egui::Align::Min)
        } else {
            egui::Layout::left_to_right(egui::Align::Min)
        };
        ui.with_layout(lay, |ui| {
            let rt = if i == 2 {
                egui::RichText::new(text)
                    .monospace()
                    .size(body)
                    .color(color)
            } else {
                egui::RichText::new(text).size(body).color(color)
            };
            ui.label(rt);
        });
    };

    let row = |ui: &mut egui::Ui, cells: [&str; 3], header: bool| {
        let color = if header {
            theme.md_table_header_fg().to_egui()
        } else {
            theme.md_table_cell_fg().to_egui()
        };
        ui.spacing_mut().item_spacing.x = col_gap;
        // 동적폭 방어: 잔여폭이 컬럼 간격 합 미만이면 columns 내부 폭이 음수가 되어 panic.
        if ui.available_width() > col_gap * (cols as f32 - 1.0) + 1.0 {
            ui.columns(cols, |c| {
                for (i, t) in cells.iter().enumerate() {
                    cell(&mut c[i], i, t, color);
                }
            });
        } else {
            ui.vertical(|ui| {
                for (i, t) in cells.iter().enumerate() {
                    cell(ui, i, t, color);
                }
            });
        }
    };

    let out = egui::Frame::new()
        .fill(row_bg) // 불투명 채움이라 표가 문서 바탕에서 떨어져 읽힌다.
        .stroke(egui::Stroke::new(border_w, border))
        .corner_radius(theme.corner_radius.value())
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .fill(theme.md_table_header_bg().to_egui())
                .corner_radius(egui::CornerRadius {
                    nw: r,
                    ne: r,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(margin)
                .show(ui, |ui| row(ui, ["Resource", "Kind", "Count"], true));
            for (i, cells) in rows.iter().enumerate() {
                table_divider(ui, theme);
                let last = i + 1 == rows.len();
                let bottom = if last { r } else { 0 };
                egui::Frame::new()
                    .fill(if i % 2 == 1 {
                        zebra_bg
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: bottom,
                        se: bottom,
                    })
                    .inner_margin(margin)
                    .show(ui, |ui| row(ui, *cells, false));
            }
        });

    // egui columns가 세로선을 그리지 않으므로 열 경계만 직접 그린다.
    let content = out.response.rect;
    let inner_w = content.width() - col_gap;
    if inner_w > col_gap * (cols as f32 - 1.0) {
        let col_w = (inner_w - col_gap * (cols as f32 - 1.0)) / cols as f32;
        let inner_left = content.left() + pad_x;
        let painter = ui.painter();
        for i in 1..cols {
            let x = inner_left + i as f32 * col_w + (2 * i - 1) as f32 * pad_x;
            painter.vline(x, content.y_range(), egui::Stroke::new(border_w, border));
        }
    }
}

fn table_divider(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.md_table_border().to_egui(),
        ),
    );
}

/// nested blockquote — left bar(border-strong) + muted 본문, 1단계 중첩.
fn blockquote(ui: &mut egui::Ui, theme: &Theme) {
    let body = theme.font_size_body.value();
    let bar_w = theme.md_quote_bar_width().value();
    let gap = theme.spacing_md.value();
    quote_block(ui, theme, bar_w, gap, |ui| {
        ui.label(rich(
            theme,
            "Quoted text reads one tone down (muted) with a left bar.",
            body,
            theme.text_muted().to_egui(),
        ));
        ui.add_space(theme.spacing_xs.value());
        quote_block(ui, theme, bar_w, gap, |ui| {
            ui.label(rich(
                theme,
                "Nested quote, one level deeper.",
                body,
                theme.text_muted().to_egui(),
            ));
        });
    });
}

/// left bar(border-strong) + 들여쓴 content.
pub(super) fn quote_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    bar_w: f32,
    gap: f32,
    add: impl FnOnce(&mut egui::Ui),
) {
    callout_block(ui, bar_w, gap, theme.border_strong().to_egui(), add);
}

/// 콜아웃 표본의 아이콘·강조색·제목색·라벨·본문·접힘 여부.
type CalloutSpec = (
    icons::MockGlyph,
    fn(&Theme) -> tasty_type_appearance::color::HexColor,
    fn(&Theme) -> tasty_type_appearance::color::HexColor,
    &'static str,
    &'static str,
    bool,
);

/// GitHub alert·Obsidian callout 두 형태(blockquote · 접히는 details) — 시안 `Callouts`.
/// 본체 렌더러와 같은 상자(종류 색 채움·아이콘·radius)이며, 접힌 형태는 제목 앞에 chevron 을 두고
/// 본문 대신 caption 설명을 단다.
fn callouts(ui: &mut egui::Ui, theme: &Theme, focused: bool) {
    let items: [CalloutSpec; 2] = [
        (
            icons::ALERT_CIRCLE,
            Theme::accent_primary,
            Theme::accent_primary_raised,
            "Note",
            "Non-collapsible: > [!note]",
            false,
        ),
        (
            icons::ALERT_TRIANGLE,
            Theme::accent_warning,
            Theme::accent_warning_raised,
            "Warning",
            "Collapsible: > [!warning]- (closed until clicked)",
            true,
        ),
    ];
    for (icon, accent, raised, label, body, folded) in items {
        callout_box(
            ui,
            theme,
            &Callout {
                icon,
                accent: accent(theme),
                raised: raised(theme),
                label,
                body,
                folded,
                bed: surface_bed(theme, focused),
            },
        );
        ui.add_space(theme.spacing_md.value());
    }
}

/// 색이 있는 왼쪽 막대 + 들여쓴 content. content 를 자식 ui 로 측정한 뒤 막대를 그 높이만큼 칠한다.
fn callout_block(
    ui: &mut egui::Ui,
    bar_w: f32,
    gap: f32,
    color: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    let top = ui.cursor().min.y;
    let left = ui.min_rect().left();
    let avail = ui.available_width();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                egui::pos2(left + bar_w + gap, top),
                egui::vec2((avail - bar_w - gap).max(1.0), f32::INFINITY),
            ))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    add(&mut child);
    let bottom = child.min_rect().bottom();
    ui.allocate_rect(
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + avail, bottom)),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + bar_w, bottom)),
        0.0,
        color,
    );
}

fn hr(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}
