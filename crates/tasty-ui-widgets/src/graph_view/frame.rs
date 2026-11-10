//! 그래프 뷰를 담는 틀과 상세(Drawer) 자리.
//! 상세는 넓은 화면에서 우측 고정폭 패널, 좁은 화면에서 하단 시트다. 내용은 호출자가 채운다.

use tasty_type_appearance::theme::Theme;

use crate::{ControlSize, IconButton, margin_all};

use super::chrome::GRAPH_NARROW_DETAIL_SHEET;

/// 그래프 뷰를 담는 틀. surface는 자체 헤더·줌 버튼을 그리고 팝업은 back bar의 버튼 동작을 받아 처리한다.
pub enum GraphFrame<A> {
    /// 탭 surface — 헤더 띠 + 캔버스 위 줌 클러스터를 그래프 뷰가 그린다.
    /// `bed_focused`는 캔버스가 칠할 포커스 값이다.
    Surface { bed_focused: bool },
    /// 본문보다 먼저 처리된 팝업 back bar 동작.
    Popup(Option<A>),
}

impl<A> GraphFrame<A> {
    /// 캔버스 바탕. 탭 surface는 포커스를 보이고, surface가 아닌 팝업은 그대로 `dag-canvas-bg`다.
    pub fn canvas_bg(&self, theme: &Theme) -> egui::Color32 {
        match self {
            Self::Surface { bed_focused } => crate::surface_bed(theme, *bed_focused),
            Self::Popup(_) => theme.dag_canvas_bg().to_egui(),
        }
    }

    /// 캔버스 위에 줌 클러스터를 직접 그리는 틀인지.
    pub fn owns_zoom_cluster(&self) -> bool {
        matches!(self, Self::Surface { .. })
    }
}

/// 상세 패널의 배치 위치.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GraphDrawerDock {
    /// 캔버스 오른쪽 고정폭 패널 — 캔버스와의 경계는 **왼쪽 세로선**.
    Side,
    /// 캔버스 아래 시트 — 경계는 **위쪽 가로선**. 팝오버로 띄울 때도 같다.
    Sheet,
}

/// 도킹 자리와 캔버스 사이의 경계선. 도킹을 정한 쪽이 그 자리 rect 로 호출한다.
pub fn paint_drawer_divider(
    painter: &egui::Painter,
    theme: &Theme,
    dock: egui::Rect,
    side: GraphDrawerDock,
) {
    let ends = match side {
        GraphDrawerDock::Side => [dock.left_top(), dock.left_bottom()],
        GraphDrawerDock::Sheet => [dock.left_top(), dock.right_top()],
    };
    painter.line_segment(
        ends,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.dag_detail_border().to_egui(),
        ),
    );
}

/// 캔버스와 상세 자리를 나눈 결과.
pub struct GraphDrawerSplit<C, D> {
    pub canvas: C,
    /// 상세를 그렸을 때만 있다.
    pub drawer: Option<D>,
    /// 캔버스가 받은 크기. 맞춤·줌 앵커 계산에 쓴다.
    pub viewport: egui::Vec2,
}

/// 상세가 있으면 화면 폭에 따라 오른쪽 또는 아래에 배치하고 나머지를 캔버스에 준다.
/// `surface_width`는 틀 전체 폭이다(헤더를 그리기 전에 잰 값).
pub fn split_graph_drawer<C, D>(
    ui: &mut egui::Ui,
    theme: &Theme,
    surface_width: f32,
    drawer: Option<impl FnOnce(&mut egui::Ui) -> D>,
    canvas: impl FnOnce(&mut egui::Ui) -> C,
) -> GraphDrawerSplit<C, D> {
    let wide = surface_width >= GRAPH_NARROW_DETAIL_SHEET.value();
    let Some(drawer) = drawer else {
        let viewport = ui.available_size();
        return GraphDrawerSplit {
            canvas: canvas(ui),
            drawer: None,
            viewport,
        };
    };
    let mut canvas_out = None;
    let mut drawer_out = None;
    let viewport;
    if !wide {
        // 좁은 화면: 캔버스(위) + 하단 시트. 경계선은 시트 **위쪽 가로선**이다.
        let sheet_h = theme.dag_detail_sheet_height().value();
        let canvas_h = (ui.available_height() - sheet_h).max(0.0);
        viewport = egui::vec2(ui.available_width(), canvas_h);
        ui.allocate_ui(viewport, |ui| canvas_out = Some(canvas(ui)));
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), sheet_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let dock = ui.available_rect_before_wrap();
                drawer_out = Some(drawer(ui));
                paint_drawer_divider(ui.painter(), theme, dock, GraphDrawerDock::Sheet);
            },
        );
    } else {
        // 넓은 화면: 캔버스 | 우측 패널. 경계선은 패널 **왼쪽 세로선**이다.
        let panel_w = theme.dag_detail_width().value();
        let canvas_w = (ui.available_width() - panel_w).max(0.0);
        viewport = egui::vec2(canvas_w, ui.available_height());
        ui.horizontal_top(|ui| {
            ui.allocate_ui(viewport, |ui| canvas_out = Some(canvas(ui)));
            ui.allocate_ui_with_layout(
                egui::vec2(panel_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let dock = ui.available_rect_before_wrap();
                    drawer_out = Some(drawer(ui));
                    paint_drawer_divider(ui.painter(), theme, dock, GraphDrawerDock::Side);
                },
            );
        });
    }
    GraphDrawerSplit {
        canvas: canvas_out.expect("canvas closure runs inside allocate_ui"),
        drawer: drawer_out,
        viewport,
    }
}

/// 상세 본문 틀 — 바탕을 채우고 여백 안에서 세로로 스크롤한다. 구분선은 배치를 정한 쪽이 그린다.
pub fn drawer_body<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let dock = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(dock, 0.0, theme.dag_detail_bg().to_egui());

    let mut out = None;
    egui::Frame::NONE
        .inner_margin(margin_all(theme.dag_detail_padding()))
        .show(ui, |ui| {
            ui.set_min_width(dock.width() - theme.dag_detail_padding().value() * 2.0);
            ui.spacing_mut().item_spacing =
                egui::vec2(theme.spacing_xs.value(), theme.spacing_xs.value());
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| out = Some(add_contents(ui)));
        });
    out
}

/// 상세 캡션. 문구의 대소문자를 그대로 쓰고 caption 크기로 그린다.
pub fn drawer_section(ui: &mut egui::Ui, theme: &Theme, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(theme.font_size_caption.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 라벨과 값 한 줄.
pub fn drawer_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        ui.label(
            egui::RichText::new(value)
                .size(theme.font_size_caption.value())
                .color(theme.text_primary().to_egui()),
        );
    });
}

/// CLI에 그대로 쓰는 식별자는 mono 값으로 두고 선택·복사할 수 있게 한다.
pub fn drawer_mono_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(value)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(theme.text_primary().to_egui()),
            )
            .selectable(true),
        );
    });
}

/// 높이를 넘으면 스크롤하는 텍스트 블록. `copy_tooltip`이 있으면 복사 버튼도 표시한다.
pub fn drawer_text_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    body: &str,
    max_h: f32,
    copy_tooltip: Option<&str>,
) {
    if let Some(tooltip) = copy_tooltip {
        ui.horizontal(|ui| {
            drawer_section(ui, theme, label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if IconButton::new()
                    .size(ControlSize::Sm)
                    .show(ui, theme, &|ui, rect, c| {
                        tasty_icons::COPY.image(rect.height(), c).paint_at(ui, rect)
                    })
                    .on_hover_text(tooltip)
                    .clicked()
                {
                    ui.ctx().copy_text(body.to_owned());
                }
            });
        });
    } else {
        drawer_section(ui, theme, label);
    }
    egui::Frame::NONE
        .fill(theme.dag_detail_log_bg().to_egui())
        .inner_margin(margin_all(theme.spacing_xs))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(max_h)
                .auto_shrink([false, true])
                .id_salt(label)
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(body)
                                .monospace()
                                .size(theme.font_size_caption.value())
                                .color(theme.text_primary().to_egui()),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latte() -> Theme {
        let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
        let (partial, is_light) = file.to_partial();
        let mut colors = tasty_themes::mocha_fallback_colors();
        colors.apply_partial(&partial);
        Theme::with_colors(colors, is_light.unwrap_or(true))
    }

    /// 팝업의 그래프는 surface가 아니므로 포커스를 보이지 않는다. 탭 surface만 바탕 쌍을 읽는다.
    #[test]
    fn only_the_tab_surface_canvas_shows_focus() {
        for theme in [tasty_themes::mocha_fallback(), latte()] {
            assert_eq!(
                GraphFrame::<()>::Popup(None).canvas_bg(&theme),
                theme.dag_canvas_bg().to_egui()
            );
            assert_eq!(
                GraphFrame::<()>::Surface { bed_focused: true }.canvas_bg(&theme),
                theme.surface_bed_focused().to_egui()
            );
            assert_eq!(
                GraphFrame::<()>::Surface { bed_focused: false }.canvas_bg(&theme),
                theme.surface_bed_unfocused().to_egui()
            );
        }
    }
}
