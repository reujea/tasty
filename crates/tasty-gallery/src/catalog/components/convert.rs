//! 서피스 타입 전환 팝업의 정적 예제.
//!
//! 본체 `popup/convert.rs` 와 같은 작은 목록 popup 이다 — 공용 타이틀바(× 만) 아래에 바꿀 수
//! 있는 kind 마다 `MenuItem` 하나. 행 끝에는 단축키 글자를 `Kbd` 키캡으로 그린다(본체와 같은
//! `menu_item_kbd`). 폭은 `convert-popup-width` 에 UI 배율을 곱한 값이다.

use tasty_egui_theme::overlay_edge::BoxEdge;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{MenuItemVariant, menu_item_kbd_with_hover};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::popup_frame::{self, TitleButtons};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 행 하나 — (아이콘, 라벨, 단축키 글자). 글자는 kind id 첫 영문 글자의 대문자라 locale 과 무관하다.
/// 글자가 없으면 앞 행과 첫 글자가 겹친 행이며 키캡도 그 자리도 없다.
type Kind = (MockGlyph, Label, Option<&'static str>);

/// 행 라벨. 본체 번역표에 있는 kind 표시 이름은 본체와 같은 키로 읽는다.
#[derive(Clone, Copy)]
enum Label {
    Lit(&'static str),
    Key(&'static str),
}

impl Label {
    fn text(self) -> &'static str {
        match self {
            Label::Lit(s) => s,
            Label::Key(k) => crate::i18n::t(k),
        }
    }
}

const DAG: Label = Label::Key("surface.kind.dag_graph");

/// 본체가 터미널 서피스에서 여는 목록. 현재 kind 는 빠진다.
const KINDS: &[Kind] = &[
    (icons::MARKDOWN, Label::Lit("Markdown"), Some("M")),
    (icons::IMAGE, Label::Lit("Image"), Some("I")),
    (icons::GIT_TREE, DAG, Some("D")),
    (icons::HTML, Label::Lit("HTML"), Some("H")),
    (icons::FOLDER, Label::Lit("Explorer"), Some("E")),
];

/// 상태 견본 — 첫 글자 충돌 행(키캡 없음, 라벨이 행 끝까지)과 말줄임되는 긴 라벨.
const KINDS_STATES: &[Kind] = &[
    (icons::MARKDOWN, Label::Lit("Markdown"), Some("M")),
    (icons::IMAGE, Label::Lit("Image"), Some("I")),
    (icons::HTML, Label::Lit("HTML"), Some("H")),
    (icons::FILE, Label::Lit("Mermaid (plugin)"), None),
    (
        icons::FILE,
        Label::Lit("Notebook viewer with outputs (plugin)"),
        Some("N"),
    ),
];

/// 마크다운 서피스에서 연 ko 목록 — 터미널 행이 `T` 를 받는다.
const KINDS_KO: &[Kind] = &[
    (icons::TERMINAL, Label::Lit("터미널"), Some("T")),
    (icons::IMAGE, Label::Lit("이미지"), Some("I")),
    (icons::GIT_TREE, DAG, Some("D")),
    (icons::HTML, Label::Lit("HTML"), Some("H")),
    (icons::FOLDER, Label::Lit("탐색기"), Some("E")),
];

const KINDS_JA: &[Kind] = &[
    (icons::TERMINAL, Label::Lit("ターミナル"), Some("T")),
    (icons::MARKDOWN, Label::Lit("マークダウン"), Some("M")),
    (icons::IMAGE, Label::Lit("画像"), Some("I")),
    (icons::HTML, Label::Lit("HTML"), Some("H")),
    (icons::FOLDER, Label::Lit("エクスプローラー"), Some("E")),
];

const KO_TITLE: &str = "서피스 타입 전환";

/// 배율을 따르지 않던 옛 convert popup 폭. 1.2 배 전후 비교 specimen 의 '전' 쪽에서만 쓴다.
const LITERAL_WIDTH_BEFORE: LogicalPx = LogicalPx(200.0);

const JA_TITLE: &str = "サーフェスタイプ切替";

/// 배율을 적용한 테마. 갤러리 기본 테마는 egui 전역 배율을 쓰므로 1.0 이다.
fn scaled(theme: &Theme, zoom: f32) -> Theme {
    Theme::with_colors_and_zoom(theme.to_colors(), theme.is_light, zoom)
}

/// 본체 convert popup 한 장. 높이 = 타이틀바 + 콘텐츠 여백 × 2 + 행 수 × MenuItem 높이.
/// `open_tip`이면 잘린 제목의 Tooltip을 호버 없이 띄운다(specimen).
fn popup(ui: &mut egui::Ui, th: &Theme, title: &str, width: LogicalPx, open_tip: bool) {
    popup_rows(ui, th, title, width, open_tip, KINDS, None, BoxEdge::NORMAL);
}

/// convert popup 한 장을 주어진 가장자리로 그린다. WebView 페이지 위 예제가 쓴다.
pub fn card(ui: &mut egui::Ui, th: &Theme, title: &str, edge: BoxEdge) {
    let width = th.convert_popup_width();
    popup_rows(ui, th, title, width, false, KINDS, None, edge);
}

/// [`popup`] 에 행 목록과 호버 지정 행을 받는다. 선택(키보드) 행은 언제나 첫 행이다.
#[allow(clippy::too_many_arguments)] // reason: specimen 이 행 목록·호버·가장자리를 모두 바꿔 그린다.
fn popup_rows(
    ui: &mut egui::Ui,
    th: &Theme,
    title: &str,
    width: LogicalPx,
    open_tip: bool,
    kinds: &[Kind],
    hover: Option<usize>,
    edge: BoxEdge,
) {
    let title_h = th.item_height_interactive;
    let margin = th.spacing_xs;
    let rows = th.menu_item_height().scaled(kinds.len() as f32);
    let (frame, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), (title_h + margin.scaled(2.0) + rows).value()),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(frame.expand(th.spacing_lg.value()));
    let radius = edge.radius(th, th.corner_radius).value();
    painter.add(
        edge.shadow(th, th.shadow_modal())
            .to_egui()
            .as_shape(frame, radius),
    );
    painter.rect_filled(frame, radius, th.bg_panel().to_egui());
    painter.rect_stroke(
        frame,
        radius,
        egui::Stroke::new(th.border_width.value(), th.border_frame().to_egui()),
        egui::StrokeKind::Outside,
    );
    let title_rect =
        egui::Rect::from_min_size(frame.min, egui::vec2(frame.width(), title_h.value()));
    let cr = radius as u8;
    painter.rect_filled(
        title_rect,
        egui::CornerRadius {
            nw: cr,
            ne: cr,
            sw: 0,
            se: 0,
        },
        th.bg_sidebar().to_egui(),
    );
    painter.hline(
        title_rect.x_range(),
        title_rect.max.y,
        egui::Stroke::new(th.border_width.value(), th.border_frame().to_egui()),
    );
    let buttons_left =
        popup_frame::draw_title_buttons(ui.ctx(), &painter, th, title_rect, TitleButtons::CLOSE);
    let cut_band = popup_frame::draw_title_text(&painter, th, title_rect, buttons_left, title);
    popup_frame::title_tooltip(ui, th, title, cut_band, open_tip);

    let content = egui::Rect::from_min_max(
        egui::pos2(
            frame.min.x + margin.value(),
            title_rect.max.y + margin.value(),
        ),
        frame.max - egui::vec2(margin.value(), margin.value()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 0.0;
    for (i, (glyph, label, key)) in kinds.iter().enumerate() {
        let glyph = *glyph;
        let keys: Vec<&str> = key.iter().copied().collect();
        menu_item_kbd_with_hover(
            &mut child,
            th,
            Some(&|ui: &mut egui::Ui, r: egui::Rect, c: egui::Color32| {
                glyph.image(r.height(), c).paint_at(ui, r);
            }),
            label.text(),
            &keys,
            MenuItemVariant::Normal,
            i == 0,
            true,
            hover == Some(i),
        );
    }
}

/// 목록 popup — 바꿀 kind 를 고른다.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, th) in [
            ("Mocha · en · ui_scale 1.0", theme),
            ("Latte · en · ui_scale 1.0", &latte),
        ] {
            spec::cluster(ui, th, label, |ui| {
                popup(ui, th, "Surface Type", th.convert_popup_width(), false);
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "width",
                "240 × ui_scale · convert-popup-width (was literal 200)",
            ),
            (
                "title",
                "shared popup title bar (× only, reserve 32 per side)",
            ),
            (
                "rows",
                "MenuItem 28 · icon + kind label + Kbd letter · one per kind the surface can become",
            ),
            (
                "actions",
                "click / Enter converts · the row's letter converts · × / Esc close",
            ),
        ],
        &[
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::new("border-frame", "edge", theme.border_frame().to_egui()),
            TokenChip::new(
                "surface-active",
                "keyboard row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::without_color("convert-popup-width", "→ size-240, × ui_scale"),
        ],
    );

    spec::note(
        ui,
        theme,
        "Only the renderer changes — the running process keeps its scrollback. The current kind is \
         not listed. The earlier 400-wide From / To dialog is retired.",
    );
}

/// 행 끝 단축키 키캡 — 행 상태와 무관하게 같은 키캡, 충돌 행은 키캡 없음, 긴 라벨만 말줄임.
pub fn draw_shortcut(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha_12 = scaled(theme, 1.2);
    let latte_12 = scaled(&latte, 1.2);
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, th) in [
            (
                "Mocha · en · 1.0 — active (Markdown) · hover (Image) · collision · long",
                theme,
            ),
            ("Latte · en · 1.0 — same states", &latte),
        ] {
            spec::cluster(ui, th, label, |ui| {
                popup_rows(
                    ui,
                    th,
                    "Surface Type",
                    th.convert_popup_width(),
                    false,
                    KINDS_STATES,
                    Some(1),
                    BoxEdge::NORMAL,
                );
            });
        }
        spec::cluster(ui, theme, "Mocha · ko · 1.0", |ui| {
            popup_rows(
                ui,
                theme,
                KO_TITLE,
                theme.convert_popup_width(),
                false,
                KINDS_KO,
                None,
                BoxEdge::NORMAL,
            );
        });
        for (label, frame_th, th) in [
            ("Mocha · ja · 1.2 (240 × 1.2)", theme, &mocha_12),
            ("Latte · ja · 1.2 (240 × 1.2)", &latte, &latte_12),
        ] {
            spec::cluster(ui, frame_th, label, |ui| {
                popup_rows(
                    ui,
                    th,
                    JA_TITLE,
                    th.convert_popup_width(),
                    false,
                    KINDS_JA,
                    None,
                    BoxEdge::NORMAL,
                );
            });
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "chip",
                "Kbd keycap in the MenuItem shortcut slot · one latin capital",
            ),
            (
                "letter",
                "kind id first letter, upper-case · never the label",
            ),
            ("states", "rest = hover = active · keycap unchanged"),
            (
                "no shortcut",
                "collision → no chip, no reserved slot, label to row end",
            ),
            ("overflow", "label ellipsis · chip never shrinks"),
            ("width", "240 × ui_scale — unchanged"),
            (
                "input",
                "display only · click / ↑↓ Enter / letter / Esc unchanged",
            ),
        ],
        &[
            TokenChip::without_color("kbd-size", "keycap height / min width"),
            TokenChip::without_color("kbd-font-size", "letter"),
            TokenChip::new(
                "surface-raised",
                "keycap fill",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "border-strong",
                "keycap edge",
                theme.border_strong().to_egui(),
            ),
            TokenChip::new(
                "text-secondary",
                "letter ink",
                theme.text_secondary_raised().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The letter follows the kind id, not the translated label, so the row shows it. Same keycap \
         as the command palette; no new tokens.",
    );
}

/// 좁은 popup 과 긴 제목 — 폭은 배율을 따르고, 그래도 넘치는 제목은 띠 안에서 말줄임한다.
pub fn draw_narrow(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha_12 = scaled(theme, 1.2);
    let latte_12 = scaled(&latte, 1.2);
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(
            ui,
            theme,
            "Mocha · ja · 1.2 — before: literal 200 (cuts)",
            |ui| {
                popup(ui, &mocha_12, JA_TITLE, LITERAL_WIDTH_BEFORE, false);
            },
        );
        spec::cluster(
            ui,
            theme,
            "Mocha · ja · 1.2 — convert-popup-width 240 × 1.2 (fits)",
            |ui| {
                popup(
                    ui,
                    &mocha_12,
                    JA_TITLE,
                    mocha_12.convert_popup_width(),
                    false,
                );
            },
        );
        spec::cluster(
            ui,
            &latte,
            "Latte · ja · 1.2 — convert-popup-width 240 × 1.2 (fits)",
            |ui| {
                popup(
                    ui,
                    &latte_12,
                    JA_TITLE,
                    latte_12.convert_popup_width(),
                    false,
                );
            },
        );
        spec::cluster(
            ui,
            theme,
            "Mocha · still too long — ellipsis + full title tooltip (forced open)",
            |ui| {
                popup(
                    ui,
                    theme,
                    "Convert this surface to another type",
                    theme.convert_popup_width(),
                    true,
                );
            },
        );
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "width",
                "token × ui_scale — never a literal · base sized for the longest locale title at 1.0",
            ),
            ("band", "width − 2 × reserve(N) · convert 240 → 176"),
            ("overflow", "ellipsis inside the band"),
            (
                "tooltip",
                "only when cut · full title on hover after tooltip-delay · top, then bottom",
            ),
            ("widen to fit", "no"),
        ],
        &[
            TokenChip::new("bg-sidebar", "title bar", theme.bg_sidebar().to_egui()),
            TokenChip::without_color("convert-popup-width", "240 × ui_scale"),
            TokenChip::without_color("popup-title-btn-size", "reserve part"),
        ],
    );

    spec::dont(
        ui,
        theme,
        "Don't keep a popup width as a literal outside the scale, and don't widen a popup to its \
         title — the band follows the token width.",
    );
}
