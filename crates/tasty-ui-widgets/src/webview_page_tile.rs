//! native WebView 아래에 깔리는 페이지 자리 타일. 페이지 자체는 OS WebView가 그린다.
//! 바탕·테두리는 받은 영역(`ui.max_rect()`) 전체에 칠하고, 글리프·글자는 native WebView가 덮는
//! `content` 안으로 자른다. native 창이 비워 둔 분할선 여백으로 글자가 비치지 않게 하기 위해서다.
//! 문구 번역, URL 줄 선택, 바탕 결정(surface 포커스 표현)은 호출자가 끝내고 넘긴다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

/// 타일이 보일 상태. 문구는 호출자가 번역해 넘긴다.
#[derive(Clone, Copy, Debug)]
pub enum WebviewPageState<'a> {
    /// 탐색 중. Spinner와 `title`을 가운데에 그린다.
    Loading { title: &'a str },
    /// 탐색 실패. 경고 글리프·`title`과, 있으면 mono URL 줄을 그린다.
    Failed {
        title: &'a str,
        caption: Option<&'a str>,
    },
    /// 열린 페이지가 없다. 페이지 글리프(`glyph` 색)·`title`과, 있으면 caption 안내를 그린다.
    Empty {
        title: &'a str,
        glyph: egui::Color32,
    },
    /// 페이지가 잠시 가려진 동안의 빈 타일. 바탕·테두리만 그린다.
    Boundary,
}

/// 그린 요소의 위치. 그리지 않은 요소는 `None`이다.
#[derive(Clone, Copy, Debug, Default)]
pub struct WebviewPageTileOutput {
    pub glyph: Option<egui::Rect>,
    pub title: Option<egui::Rect>,
}

/// `ui.max_rect()` 전체에 `bg`와 1px 테두리를 칠하고 `state`의 안내를 가운데에 그린다.
/// `empty_hint`는 빈 안내 아래의 caption이다. 글자는 Empty에서만 그리지만, 값이 있으면
/// 상태와 관계없이 그 줄 높이를 블록에 넣어 세로 위치를 정한다.
pub fn webview_page_tile(
    ui: &mut egui::Ui,
    theme: &Theme,
    bg: egui::Color32,
    content: egui::Rect,
    state: WebviewPageState<'_>,
    empty_hint: Option<&str>,
) -> WebviewPageTileOutput {
    let th = theme;
    let panel_rect = ui.max_rect();
    ui.painter().rect_filled(panel_rect, 0.0, bg);
    ui.painter().rect_stroke(
        panel_rect,
        0.0,
        egui::Stroke::new(th.border_width.value(), th.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );

    let glyph = th.icon_glyph_size_md;
    let hint_h = empty_hint.map_or(LogicalPx(0.0), |_| th.spacing_sm + th.font_size_caption);
    let block_h = glyph + th.spacing_sm + th.font_size_body * 2.0 + hint_h;
    let top_pad = ((LogicalPx(panel_rect.height()) - block_h) / 2.0).max(th.spacing_xl);

    let mut out = WebviewPageTileOutput::default();
    // 시안 `HtmlTile`의 내용 padding 16(space-lg)을 좌우에 둬 긴 URL 줄이 가장자리에 닿지 않게 한다.
    let content_rect = panel_rect.shrink2(egui::vec2(th.spacing_lg.value(), 0.0));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(content));
            ui.add_space(top_pad.value());
            match state {
                WebviewPageState::Failed { title, caption } => {
                    let g = ui.add(
                        tasty_icons::ALERT_CIRCLE
                            .image(glyph.value(), th.accent_danger_raised().to_egui()),
                    );
                    ui.add_space(th.spacing_sm.value());
                    let l = label(ui, th, title, th.accent_danger_raised().to_egui());
                    if let Some(caption) = caption {
                        url_line(ui, th, caption);
                    }
                    out = WebviewPageTileOutput {
                        glyph: Some(g.rect),
                        title: Some(l.rect),
                    };
                }
                WebviewPageState::Loading { title } => {
                    let g = crate::Spinner::new()
                        .size(th.spinner_size.value())
                        .show(ui, th);
                    ui.add_space(th.spacing_sm.value());
                    let l = label(ui, th, title, th.text_muted().to_egui());
                    out = WebviewPageTileOutput {
                        glyph: Some(g.rect),
                        title: Some(l.rect),
                    };
                }
                WebviewPageState::Empty { title, glyph: ink } => {
                    let g = ui.add(tasty_icons::HTML.image(glyph.value(), ink));
                    ui.add_space(th.spacing_sm.value());
                    let l = label(ui, th, title, th.text_muted().to_egui());
                    if let Some(hint) = empty_hint {
                        ui.add_space(th.spacing_sm.value());
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(hint)
                                    .size(th.font_size_caption.value())
                                    .color(th.text_muted().to_egui()),
                            )
                            .truncate(),
                        );
                    }
                    out = WebviewPageTileOutput {
                        glyph: Some(g.rect),
                        title: Some(l.rect),
                    };
                }
                WebviewPageState::Boundary => {}
            }
        },
    );
    out
}

/// URL 줄(mono caption · text-muted · 한 줄 말줄임).
fn url_line(ui: &mut egui::Ui, th: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(egui::FontId::monospace(th.font_size_caption.value()))
                .color(th.text_muted().to_egui()),
        )
        .truncate(),
    );
}

/// 캡션 한 줄(body · 지정색).
fn label(ui: &mut egui::Ui, th: &Theme, text: &str, color: egui::Color32) -> egui::Response {
    ui.label(
        egui::RichText::new(text)
            .size(th.font_size_body.value())
            .color(color),
    )
}
