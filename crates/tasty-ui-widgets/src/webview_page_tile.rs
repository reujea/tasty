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
    /// WebView를 만들 수 없는 이유의 안내(정본 `HtmlPageState`의 표시 백엔드 kind). info 글리프, 원인을
    /// 담은 `title`, 해결책 `sub`를 그린다. 해결책은 끝까지 읽혀야 하므로 두 줄 모두 말줄임 없이
    /// 줄을 바꾸고, 블록이 영역보다 크면 위에서 시작해 세로로 스크롤한다. `sub`가 없어도 그 줄 자리는
    /// 남긴다. `at_end`는 갤러리의 스크롤 끝 프레임 표본만 쓴다. 본체는 `false`를 넘긴다.
    Notice {
        title: &'a str,
        sub: Option<&'a str>,
        at_end: bool,
    },
}

/// WebView를 만들 수 없는 표시 경로의 안내 kind(정본 `HtmlPageState` `DB_NOTICE`). html·markdown·html
/// mirror가 같은 kind와 같은 문구 키를 쓴다. 문구는 tasty-page `display_backend_copy.js`에서 복사한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayNotice {
    /// 원인 1: XWayland가 없어 네이티브 Wayland로 실행됐다.
    NativeWayland,
    /// 원인 1b: `DISPLAY`의 X 서버에 연결하지 못해 네이티브 Wayland로 내려갔다.
    NativeWaylandXFailed,
    /// 원인 2: 시작 환경의 `GDK_BACKEND`가 x11을 허용하지 않아 GTK 초기화가 실패했다.
    GdkBackend,
    /// 창은 X11인데 다른 이유로 GTK 초기화가 실패했다. 해결책 줄이 없다.
    GtkInit,
}

impl DisplayNotice {
    pub const ALL: [Self; 4] = [
        Self::NativeWayland,
        Self::NativeWaylandXFailed,
        Self::GdkBackend,
        Self::GtkInit,
    ];

    /// 정본 kind 이름.
    pub fn design_kind(self) -> &'static str {
        match self {
            Self::NativeWayland => "native-wayland",
            Self::NativeWaylandXFailed => "native-wayland-x",
            Self::GdkBackend => "gdk-backend",
            Self::GtkInit => "gtk-init",
        }
    }

    /// 원인을 담은 제목의 번역 키.
    pub fn title_key(self) -> &'static str {
        match self {
            Self::NativeWayland => "webview.native_wayland",
            Self::NativeWaylandXFailed => "webview.native_wayland_x_failed",
            Self::GdkBackend => "webview.gdk_backend",
            Self::GtkInit => "webview.gtk_init_failed",
        }
    }

    /// 해결책 줄의 번역 키. GTK 초기화 실패는 사용자가 할 수 있는 해결책이 없어 키가 없다.
    pub fn sub_key(self) -> Option<&'static str> {
        match self {
            Self::NativeWayland => Some("webview.native_wayland_sub"),
            Self::NativeWaylandXFailed => Some("webview.native_wayland_x_failed_sub"),
            Self::GdkBackend => Some("webview.gdk_backend_sub"),
            Self::GtkInit => None,
        }
    }
}

/// 그린 요소의 위치. 그리지 않은 요소는 `None`이다.
#[derive(Clone, Copy, Debug, Default)]
pub struct WebviewPageTileOutput {
    pub glyph: Option<egui::Rect>,
    pub title: Option<egui::Rect>,
    /// Notice의 해결책 줄 자리. 해결책이 없어도 남긴 자리를 돌려준다.
    pub sub: Option<egui::Rect>,
    /// Notice 블록이 영역보다 커서 스크롤이 생겼는지.
    pub overflows: bool,
    /// Notice 스크롤 영역. 여백은 이 안쪽에 있어 스크롤바가 영역 가장자리에 붙는다.
    pub scroll: Option<egui::Rect>,
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
    if let WebviewPageState::Notice { title, sub, at_end } = state {
        let area = panel_rect.intersect(content);
        return notice(ui, th, area, title, sub, at_end);
    }

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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
                    };
                }
                WebviewPageState::Boundary | WebviewPageState::Notice { .. } => {}
            }
        },
    );
    out
}

/// 정본 `HtmlPageState` notice 배치: 안쪽 여백 space-md, 글리프·제목·해결책 사이 space-sm.
/// 블록이 들어가면 세로 가운데, 넘치면 위 여백에서 시작하고 영역이 세로로 스크롤한다.
/// 정본처럼 스크롤 영역이 `area` 가장자리까지 차고 여백은 그 안쪽이라 여백도 함께 스크롤된다.
fn notice(
    ui: &mut egui::Ui,
    th: &Theme,
    area: egui::Rect,
    title: &str,
    sub: Option<&str>,
    at_end: bool,
) -> WebviewPageTileOutput {
    let mut out = WebviewPageTileOutput::default();
    let pad = th.spacing_md.value();
    if area.width() <= 2.0 * pad || area.height() <= 2.0 * pad {
        return out;
    }
    let gap = th.spacing_sm.value();
    let glyph = th.icon_glyph_size_md.value();
    let caption_line = (th.font_size_caption * th.line_height_ui).value();
    let ink = th.text_muted().to_egui();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("webview_page_notice")
                .drag_to_scroll(false)
                .max_height(area.height())
                .auto_shrink([false, false]);
            // 끝 위치는 앞 프레임의 내용 높이로 정해지므로 첫 프레임은 위에서 시작한다.
            // 오프셋을 직접 크게 주면 매 프레임 다시 적용돼 내용이 영역 밖에 그려진다.
            if at_end {
                scroll = scroll.stick_to_bottom(true);
            }
            let shown = scroll.show(ui, |ui| {
                let width = ui.available_width() - 2.0 * pad;
                let title_g = wrapped(ui, th, title, th.font_size_body, ink, width);
                let sub_g = sub.map(|sub| wrapped(ui, th, sub, th.font_size_caption, ink, width));
                let sub_h = sub_g
                    .as_ref()
                    .map_or(caption_line, |g| g.size().y.max(caption_line));
                let block = glyph + gap + title_g.size().y + gap + sub_h;
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(pad + ((area.height() - 2.0 * pad - block) / 2.0).max(0.0));
                let g = ui.add(tasty_icons::INFO.image(glyph, th.glyph_dim().to_egui()));
                ui.add_space(gap);
                let title_rect = paint_centered(ui, width, title_g.size().y, title_g, ink);
                ui.add_space(gap);
                let (sub_rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, sub_h), egui::Sense::hover());
                if let Some(sub_g) = sub_g {
                    let top = sub_rect.center().y - sub_g.size().y / 2.0;
                    ui.painter()
                        .galley(egui::pos2(sub_rect.center().x, top), sub_g, ink);
                }
                ui.add_space(pad);
                out.glyph = Some(g.rect);
                out.title = Some(title_rect);
                out.sub = Some(sub_rect);
                block + 2.0 * pad
            });
            out.overflows = shown.inner > shown.inner_rect.height();
            out.scroll = Some(shown.inner_rect);
        },
    );
    out
}

/// 가운데 정렬로 줄을 바꾼 글자. 줄 높이는 `line-height-ui`를 따른다.
fn wrapped(
    ui: &egui::Ui,
    th: &Theme,
    text: &str,
    size: LogicalPx,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size.value()),
            color,
            line_height: Some((size * th.line_height_ui).value()),
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    job.halign = egui::Align::Center;
    ui.fonts(|f| f.layout_job(job))
}

/// 가운데 정렬 galley의 행은 기준점 좌우로 펼쳐진다. 폭 전체를 잡고 가운데 x에 그린다.
fn paint_centered(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    galley: std::sync::Arc<egui::Galley>,
    ink: egui::Color32,
) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter()
        .galley(egui::pos2(rect.center().x, rect.top()), galley, ink);
    rect
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
