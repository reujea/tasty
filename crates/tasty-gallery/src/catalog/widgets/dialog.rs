//! 모달·팝오버·그림자 없는 콘텐츠에 맞는 프레임을 제공한다.
//! 호출자가 화면의 용도에 맞는 함수를 선택한다. 배경 어둡게 하기는 별도 헬퍼로 그린다.
//! 두 배치 예제를 비교할 수 있도록 예제 영역과 카드의 크기는 공유 상수로 둔다.

use tasty_egui_theme::overlay_edge::BoxEdge;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::icons::MockGlyph;
use crate::catalog::spec::{self, StageVariant, TokenChip};
/// scrim Spec 무대의 높이. center anchor 와 top anchor 변형이 공유한다.
const SCRIM_STAGE_H: LogicalPx = LogicalPx(200.0);
/// 무대 안에 놓는 모달 카드의 폭. 두 변형이 같아야 anchor 차이만 눈에 남는다.
const FRAME_CARD_W: LogicalPx = LogicalPx(240.0);
/// top anchor 카드를 무대 위쪽에서 띄우는 데모 inset. center anchor와 위치를 비교한다.
const TOP_ANCHOR_DEMO_INSET: LogicalPx = LogicalPx(28.0);

/// 창 중앙의 모달 프레임. 콘텐츠 영역이 각자 여백을 가지므로 item_spacing은 0이다.
/// 트리거 옆 팝오버는 frame_card_popover, 일반 콘텐츠는 frame_card_flat을 사용한다.
pub fn frame_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    fill: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    frame_card_edge(ui, theme, width, fill, BoxEdge::NORMAL, add);
}

/// [`frame_card`] 에 상자 가장자리를 받는다. 불투명 모드 WebView 페이지 위에서는 각진 모서리·
/// 그림자 없음이다.
pub fn frame_card_edge(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    fill: egui::Color32,
    edge: BoxEdge,
    add: impl FnOnce(&mut egui::Ui),
) {
    let mut frame = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            strong_edge(theme),
        ))
        .corner_radius(edge.radius(theme, theme.corner_radius).value());
    let shadow = edge.shadow(theme, theme.shadow_modal());
    frame = frame.shadow(shadow.to_egui());
    show_card(ui, frame, width, add);
}

/// 트리거 옆에 뜨는 팝오버 프레임. 배경을 어둡게 하지 않는다.
pub fn frame_card_popover(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    fill: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    frame_card_with_shadow(
        ui,
        theme,
        width,
        fill,
        strong_edge(theme),
        Some(theme.shadow_popover()),
        add,
    );
}

/// 별도 창·페인·설정 섹션처럼 떠 있는 팝업이 아닌 콘텐츠에는 그림자를 넣지 않는다.
pub fn frame_card_flat(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    fill: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    frame_card_with_shadow(ui, theme, width, fill, strong_edge(theme), None, add);
}

/// 버튼에 붙는 팝오버(드롭다운) 프레임. 본체 egui popup 틀과 같은 메뉴 컨테이너 토큰
/// (`menu-bg` · `menu-border` · `menu-radius` · `shadow-popover`)을 쓴다.
pub fn frame_card_menu(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    add: impl FnOnce(&mut egui::Ui),
) {
    frame_card_menu_edge(ui, theme, width, BoxEdge::NORMAL, add);
}

/// [`frame_card_menu`] 에 상자 가장자리를 받는다.
pub fn frame_card_menu_edge(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    edge: BoxEdge,
    add: impl FnOnce(&mut egui::Ui),
) {
    let frame = egui::Frame::new()
        .fill(theme.menu_bg().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.menu_border().to_egui(),
        ))
        .corner_radius(edge.radius(theme, theme.menu_radius()).value())
        .shadow(edge.shadow(theme, theme.shadow_popover()).to_egui());
    show_card(ui, frame, width, add);
}

/// 떠 있는 입력 띠처럼 메뉴가 아닌 팝오버에 그 표면의 테두리 토큰(`edge`)을 쓴다.
pub fn frame_card_popover_edge(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    fill: egui::Color32,
    edge: egui::Color32,
    add: impl FnOnce(&mut egui::Ui),
) {
    frame_card_with_shadow(
        ui,
        theme,
        width,
        fill,
        edge,
        Some(theme.shadow_popover()),
        add,
    );
}

fn strong_edge(theme: &Theme) -> egui::Color32 {
    theme.border_strong().to_egui()
}

fn frame_card_with_shadow(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    fill: egui::Color32,
    edge: egui::Color32,
    shadow: Option<tasty_type_appearance::theme::ShadowToken>,
    add: impl FnOnce(&mut egui::Ui),
) {
    let mut frame = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(theme.border_width.value(), edge))
        .corner_radius(theme.corner_radius.value());
    if let Some(shadow) = shadow {
        frame = frame.shadow(shadow.to_egui());
    }
    show_card(ui, frame, width, add);
}

/// 카드 프레임 안에 `width` 폭의 세로 본문을 그린다. 콘텐츠 영역이 각자 여백을 가지므로
/// item_spacing 은 0이다.
fn show_card(
    ui: &mut egui::Ui,
    frame: egui::Frame,
    width: LogicalPx,
    add: impl FnOnce(&mut egui::Ui),
) {
    frame.show(ui, |ui| {
        // 부모의 가로 레이아웃을 상속하면 본문 폭이 좁아지므로 세로 child를 만든다.
        ui.set_width(width.value());
        ui.vertical(|ui| {
            ui.set_width(width.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            add(ui);
        });
    });
}

/// 정본은 border-box라 창 크기 토큰이 테두리를 포함한 바깥 길이다. 프레임 카드는 받은 폭을 안쪽에
/// 쓰므로 바깥 길이 `outer`에서 양쪽 테두리를 뺀 안쪽 길이를 넘긴다.
pub fn border_box_inner(theme: &Theme, outer: LogicalPx) -> LogicalPx {
    outer - theme.border_width * 2.0
}

/// 인라인 글리프 — `size` 정사각 영역을 할당해 `color` tint 로 그린다.
pub fn icon(ui: &mut egui::Ui, glyph: MockGlyph, size: LogicalPx, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(size.value(), size.value()), egui::Sense::hover());
    glyph.image(size.value(), color).paint_at(ui, rect);
}

/// 모달 기본 배경 (bg-panel).
pub fn panel_fill(theme: &Theme) -> egui::Color32 {
    theme.bg_panel().to_egui()
}

/// 이름 변경 팝업 배경 (dialog-bg). workspace·탭·카테고리·탐색기 항목·즐겨찾기 추가가 함께 쓰는
/// 본체 rename 팝업의 바탕이다.
pub fn dialog_fill(theme: &Theme) -> egui::Color32 {
    theme.dialog_bg().to_egui()
}

/// 팝오버/팔레트 배경 (surface-raised).
pub fn raised_fill(theme: &Theme) -> egui::Color32 {
    theme.surface_raised().to_egui()
}

/// 패딩 영역 — 전체 폭을 차지하는 child Ui 를 margin 안에 그린다.
pub fn region(ui: &mut egui::Ui, margin: egui::Margin, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        add(ui);
    });
}

/// 대칭 패딩 region (좌우 `x`, 상하 `y`).
pub fn region_sym(ui: &mut egui::Ui, x: LogicalPx, y: LogicalPx, add: impl FnOnce(&mut egui::Ui)) {
    region(
        ui,
        egui::Margin::symmetric(x.value() as i8, y.value() as i8),
        add,
    );
}

/// 전체 폭 1px separator (모달 region 구분선 — border-bottom).
pub fn hsep(ui: &mut egui::Ui, theme: &Theme) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
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

/// 모달 제목 — 14px(font-size-max) semibold, text-primary.
pub fn title(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_max.value())
            .strong()
            .color(theme.text_primary().to_egui()),
    );
}

/// 본문 산문 — 13px(body), text-secondary.
pub fn body(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(theme.font_size_body.value())
            .color(theme.text_secondary().to_egui()),
    );
}

/// 보조 caption — 11px(caption), text-muted (mono 옵션).
pub fn caption(ui: &mut egui::Ui, theme: &Theme, text: &str, mono: bool) {
    let mut rt = egui::RichText::new(text)
        .size(theme.font_size_caption.value())
        .color(theme.text_muted().to_egui());
    if mono {
        rt = rt.monospace();
    }
    ui.label(rt);
}

/// 입력값과 placeholder를 보여주는 정적 필드. 여러 예제의 포커스 경합을 피한다.
pub fn field(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: Option<LogicalPx>,
    text: &str,
    placeholder: bool,
    mono: bool,
) {
    let h = theme.item_height_interactive;
    let w = width.unwrap_or_else(|| LogicalPx(ui.available_width()));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w.value(), h.value()), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        rect,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
    );
    p.rect_stroke(
        rect,
        theme.corner_radius.value(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let color = if placeholder {
        theme.text_placeholder()
    } else {
        theme.text_primary()
    };
    let font = if mono {
        egui::FontId::monospace(theme.font_size_body.value())
    } else {
        egui::FontId::proportional(theme.font_size_body.value())
    };
    p.text(
        egui::pos2(rect.left() + theme.spacing_md.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        font,
        color.to_egui(),
    );
}

/// faux 앱 배경 + scrim — 모달이 그 위에 뜨는 무대. `add` 가 scrim 위 모달을
/// 그린다. `top_space` 만큼 위에서 띄워 anchor(center/top) 를 표현한다.
pub fn scrim_backdrop(
    ui: &mut egui::Ui,
    theme: &Theme,
    width: LogicalPx,
    height: LogicalPx,
    top_space: LogicalPx,
    add: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width.value(), height.value()),
        egui::Sense::hover(),
    );
    let p = ui.painter_at(rect);
    p.rect_filled(rect, theme.corner_radius.value(), theme.bg_app().to_egui());
    p.rect_filled(rect, theme.corner_radius.value(), theme.scrim().to_egui());

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    child.add_space(top_space.value());
    add(&mut child);
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "center anchor", |ui| {
            scrim_backdrop(
                ui,
                theme,
                theme.measure_sm,
                SCRIM_STAGE_H,
                LogicalPx(64.0),
                |ui| {
                    frame_card(ui, theme, FRAME_CARD_W, panel_fill(theme), |ui| {
                        region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
                            title(ui, theme, "Frame");
                            ui.add_space(theme.spacing_sm.value());
                            body(ui, theme, "bg-panel · 1px border-strong · modal shadow");
                        });
                    });
                },
            );
        });
        spec::cluster(ui, theme, "top anchor (~88px)", |ui| {
            scrim_backdrop(
                ui,
                theme,
                theme.measure_sm,
                SCRIM_STAGE_H,
                TOP_ANCHOR_DEMO_INSET,
                |ui| {
                    frame_card(ui, theme, FRAME_CARD_W, raised_fill(theme), |ui| {
                        region_sym(ui, theme.spacing_lg, theme.spacing_md, |ui| {
                            title(ui, theme, "Palette-style");
                            ui.add_space(theme.spacing_sm.value());
                            body(ui, theme, "surface-raised · spawns under the title bar");
                        });
                    });
                },
            );
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("scrim", "black 50%; no blur in this example"),
            ("frame bg", "bg-panel / surface-raised"),
            ("frame border", "1px border-strong"),
            ("shadow", "modal — 0 20px 60px /.55"),
            ("dismiss", "scrim click / Esc"),
            ("anchors", "center · top (~88px)"),
        ],
        &[
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::new(
                "border-strong",
                "frame edge",
                theme.border_strong().to_egui(),
            ),
            TokenChip::without_color("radius", "4px corners"),
            TokenChip::new(
                "surface-raised",
                "popover frame",
                theme.surface_raised().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Choose the modal, popover, or flat frame for the surface being shown. The caller decides whether to dim the background and where to place the frame.",
    );
}
