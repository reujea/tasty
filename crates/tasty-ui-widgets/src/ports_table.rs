//! 포트 스캐너 표의 열 정의와 표 꾸밈. 본체 popup 과 갤러리 예제가 같은 정의로 그린다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::chip::TagVariant;
use crate::status_dot::{StatusKind, status_dot};
use crate::table::{Table, TableAlign, TableColumn, TableColumnWidth};

/// 포트 표의 일곱 열. 별 열은 항상 맨 앞에 있으며 이 목록에 넣지 않는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortsColumn {
    Port,
    Proto,
    Address,
    Process,
    Workspace,
    Tab,
    State,
}

impl PortsColumn {
    /// 왼쪽에서 오른쪽 순서.
    pub const ALL: [PortsColumn; 7] = [
        PortsColumn::Port,
        PortsColumn::Proto,
        PortsColumn::Address,
        PortsColumn::Process,
        PortsColumn::Workspace,
        PortsColumn::Tab,
        PortsColumn::State,
    ];

    /// 열이 줄어들 수 있는 하한. 고정 열은 이 폭을 그대로 쓴다. Address·Process만 해당 토큰이
    /// 있다. 나머지는 이 표 전용 값이며 같은 숫자의 다른 역할 토큰으로 대체하지 않는다.
    pub fn floor(self, theme: &Theme) -> LogicalPx {
        match self {
            PortsColumn::Port => LogicalPx(84.0),
            PortsColumn::Proto => LogicalPx(76.0),
            PortsColumn::Address => theme.port_addr_col_min_width(),
            PortsColumn::Process => theme.port_process_col_min_width(),
            PortsColumn::Workspace => LogicalPx(120.0),
            PortsColumn::Tab => LogicalPx(80.0),
            PortsColumn::State => LogicalPx(140.0),
        }
    }

    /// 남는 폭을 받는 열인지. Process만 하한에서 늘어나며 남는 폭을 모두 받는다. Address는
    /// 하한에 고정된다.
    pub fn flex(self) -> bool {
        matches!(self, PortsColumn::Process)
    }

    /// 공용 Table 열 폭. 가변 열은 하한을 `Flex` 최소 폭으로, 고정 열은 하한을 `Exact` 폭으로 둔다.
    pub fn width(self, theme: &Theme) -> TableColumnWidth {
        if self.flex() {
            TableColumnWidth::Flex {
                min_width: self.floor(theme),
            }
        } else {
            TableColumnWidth::Exact(self.floor(theme))
        }
    }

    /// 셀 가로 정렬. Port만 오른쪽 정렬이다.
    pub fn align(self) -> TableAlign {
        match self {
            PortsColumn::Port => TableAlign::Right,
            _ => TableAlign::Left,
        }
    }
}

/// 포트 popup 좌우 안쪽 여백. 디자인 `port_scanner.jsx`의 `--tasty-size-14`로 4px 그리드 밖이다
/// (가장 가까운 `spacing_md` 12와 2px 차). 헤더·필터·즐겨찾기 캡션·푸터가 같은 세로선에 서야 해서
/// 본체 popup과 갤러리 예제가 한 값을 쓴다. `egui::Margin` 필드가 `i8`이라 타입을 맞춘다.
pub const PORTS_PANEL_PAD_X: i8 = 14;

/// 즐겨찾기 행 오른쪽 상태 칸의 최소 폭. 디자인 `port_scanner.jsx`의 `--tasty-size-112`이며
/// 역할 토큰이 없다. 토큰처럼 UI 배율을 곱한다.
pub fn ports_favorite_state_min_width(theme: &Theme) -> LogicalPx {
    LogicalPx((112.0 * theme.ui_zoom).round())
}

/// 즐겨찾기 행에서 주소 뒤에 오는 상세와 상태 칸. 행의 남은 폭을 모두 차지한다.
/// 상세는 주소에서 `spacing_md` 떨어져 왼쪽에 붙고 넘치면 말줄임한다. 상태 점과 라벨은 행
/// 오른쪽 끝의 최소 폭 칸 안에서 오른쪽에 붙으므로 상세가 길어도 점에 닿지 않는다.
/// 상세 라벨과 상태 점의 응답을 차례로 돌려준다.
pub fn ports_favorite_detail_and_state(
    ui: &mut egui::Ui,
    theme: &Theme,
    detail: &str,
    kind: StatusKind,
    state: &str,
    pulse: bool,
    reduced_motion: bool,
) -> (egui::Response, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width(),
            theme.port_favorites_row_height().value(),
        ),
        egui::Sense::hover(),
    );
    let state_left =
        (rect.right() - ports_favorite_state_min_width(theme).value()).max(rect.left());
    let detail_left = (rect.left() + theme.spacing_md.value()).min(state_left);
    let detail_rect = egui::Rect::from_x_y_ranges(detail_left..=state_left, rect.y_range());
    let state_rect = egui::Rect::from_x_y_ranges(state_left..=rect.right(), rect.y_range());

    let mut detail_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(detail_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let detail = detail_ui.add(
        egui::Label::new(
            egui::RichText::new(detail)
                .color(theme.text_muted().to_egui())
                .size(theme.font_size_caption.value()),
        )
        .truncate(),
    );

    let mut state_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(state_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    let state = status_dot(
        &mut state_ui,
        theme,
        kind,
        state,
        pulse,
        reduced_motion,
        crate::Ground::Panel,
    );
    (detail, state)
}

/// 맨 앞 별 열의 폭.
pub fn ports_star_column_width(theme: &Theme) -> TableColumnWidth {
    TableColumnWidth::Exact(theme.port_star_col_width())
}

/// 포트 표의 공용 Table. 행 선택, 가로 스크롤, 헤더 배경과 헤더 왼쪽 여백을 정해 둔다.
/// 열의 하한 합이 표 폭보다 크면 열을 줄이거나 숨기지 않고 본문을 가로로 스크롤한다.
pub fn ports_table<'a, K>(columns: Vec<TableColumn<'a, K>>, theme: &Theme) -> Table<'a, K> {
    Table::new(columns)
        .selectable(true)
        .horizontal_scroll(true)
        .header_fill(theme.bg_sidebar().to_egui())
        .header_pad_x(theme.table_cell_padding_x())
}

/// Process 셀: 프로세스 이름과 PID Tag. 이름과 Tag 사이는 `spacing_sm` 이다. 셀이 좁으면 이름만
/// 말줄임하고 Tag 는 그대로 보인다. PID 가 있으면 Tag 의 응답을 돌려준다.
pub fn ports_process_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    pid: Option<&str>,
) -> Option<egui::Response> {
    ui.horizontal(|ui| {
        let gap = theme.spacing_sm.value();
        ui.spacing_mut().item_spacing.x = gap;
        let tag_reserve = pid.map_or(0.0, |p| crate::tag_mono_width(ui, theme, p) + gap);
        let name_max = (ui.available_width() - tag_reserve).max(0.0);
        let name_fg = crate::table_cell_ink(ui, theme, theme.text_primary());
        ui.scope(|ui| {
            ui.set_max_width(name_max);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(name)
                        .color(name_fg)
                        .size(theme.font_size_body.value()),
                )
                .truncate(),
            );
        });
        pid.map(|p| crate::tag_mono(ui, theme, p, TagVariant::Default, false))
    })
    .inner
}

#[cfg(test)]
mod favorite_row_tests {
    use super::*;

    fn theme(zoom: f32) -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom)
    }

    /// 행 폭 `width` 안에서 주소 다음 자리부터 상세와 상태를 그리고 (행, 상세, 상태) 사각형을 돌려준다.
    fn draw(detail: &str, width: f32) -> (egui::Rect, egui::Rect, egui::Rect) {
        let theme = theme(1.0);
        let ctx = egui::Context::default();
        let mut out = None;
        for _ in 0..2 {
            drop(ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, theme.port_favorites_row_height().value()),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let row_left = ui.cursor().left();
                            let (d, s) = ports_favorite_detail_and_state(
                                ui,
                                &theme,
                                detail,
                                StatusKind::Running,
                                "LISTEN",
                                false,
                                true,
                            );
                            let row = egui::Rect::from_x_y_ranges(
                                row_left..=row_left + width,
                                d.rect.y_range(),
                            );
                            out = Some((row, d.rect, s.rect));
                        },
                    );
                });
            }));
        }
        out.expect("drawn")
    }

    #[test]
    fn a_short_detail_starts_one_spacing_md_after_the_address() {
        let theme = theme(1.0);
        let (row, detail, state) = draw("node · 48213", 480.0);
        assert_eq!(detail.left(), row.left() + theme.spacing_md.value());
        assert_eq!(state.right(), row.right());
        let gap = state.left() - detail.right();
        assert!(
            gap >= theme.spacing_md.value(),
            "detail {detail:?} sits {gap}px from the state {state:?}"
        );
    }

    #[test]
    fn a_long_detail_ellipsizes_before_the_state_column() {
        let (row, detail, state) = draw(
            "node · 48213 · a workspace name long enough to run past the state column",
            320.0,
        );
        let state_left = row.right() - ports_favorite_state_min_width(&theme(1.0)).value();
        assert!(
            detail.right() <= state_left,
            "detail {detail:?} runs into the state column at {state_left}"
        );
        assert!(
            state.left() >= state_left,
            "state {state:?} left of {state_left}"
        );
        assert_eq!(state.right(), row.right());
    }

    #[test]
    fn the_state_column_floor_scales_with_ui_zoom() {
        assert_eq!(ports_favorite_state_min_width(&theme(1.0)).value(), 112.0);
        assert_eq!(ports_favorite_state_min_width(&theme(1.2)).value(), 134.0);
    }
}
