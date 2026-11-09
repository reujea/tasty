//! 설정 행 한 줄 — 라벨 열 · gap · 컨트롤. 컨트롤이 옆에 들어가지 않으면 행마다 쌓는다.
//!
//! 지원 최소 창보다 좁을 때(1100 × 700 을 무시하는 타일링 WM 등) 한 덩어리 컨트롤이 라벨 옆 남은 폭보다
//! 넓어질 수 있다. 그 행만 라벨을 행 전체 폭에 두고, 컨트롤을 그 아래 **행 시작 x** 에
//! `settings-row-stack-gap` 만큼 띄워 둔다. 쌓인 행은 컨트롤이 `settings-row-stack-hysteresis` 만큼 여유를
//! 두고 들어갈 때만 다시 나란히 둔다. 창 폭을 조절하는 동안 경계에서 깜박이지 않게 하기 위해서다.
//! 컨트롤 안의 기존 줄바꿈(여러 단축키 버튼, 단위)은 그대로 쓰므로, 줄을 바꾸는 컨트롤은 첫 조각이 남은
//! 폭보다 넓을 때만 쌓인다.
//!
//! egui 는 즉시 모드라 이번 프레임의 컨트롤 폭은 그리고 나서야 안다. 그래서 지난 프레임에 잰 자연 폭을
//! 행 id 로 기억해 판단하고, 판단이 바뀐 프레임은 다시 그리도록 요청한다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::settings_row::{label_cell_height, settings_label_cell};

/// 행 하나의 틀.
#[derive(Clone, Copy)]
pub struct StackRow<'a> {
    /// 쌓임 상태를 기억할 id. 같은 화면 안에서 행마다 달라야 한다.
    pub id: egui::Id,
    /// 나란히 놓일 때의 라벨 열 폭.
    pub label_col: LogicalPx,
    /// 한 줄의 최소 높이. 쌓이면 라벨 줄과 컨트롤 줄에 각각 적용한다.
    pub row_h: LogicalPx,
    pub label: &'a str,
    pub hint: Option<&'a str>,
    /// 나란히 놓일 때 라벨과 컨트롤을 위에 맞춘다(여러 줄로 바뀌는 단축키 행). 아니면 세로 가운데.
    pub align_top: bool,
}

/// 지난 프레임까지의 상태.
#[derive(Clone, Copy, Default)]
struct StackState {
    stacked: bool,
    /// 지난 프레임에 잰 컨트롤 자연 폭.
    natural: Option<f32>,
}

/// 이번 프레임에 쌓을지. 나란한 행은 자연 폭이 남은 폭보다 넓으면 쌓고, 쌓인 행은 자연 폭에
/// `hysteresis` 를 더해도 남은 폭 안에 들어갈 때만 되돌린다. 아직 재지 않았으면 지난 상태를 따른다.
pub fn row_should_stack(
    was_stacked: bool,
    natural: Option<f32>,
    side_width: f32,
    hysteresis: f32,
) -> bool {
    match natural {
        None => was_stacked,
        Some(n) if was_stacked => n + hysteresis > side_width,
        Some(n) => n > side_width,
    }
}

/// 이 프레임에 쌓아 그린 행인지. 행을 그린 뒤에 읽는다.
pub(crate) fn row_is_stacked(ui: &egui::Ui, id: egui::Id) -> bool {
    ui.data(|d| d.get_temp::<StackState>(id))
        .is_some_and(|state| state.stacked)
}

/// 행을 그린다. 반환값은 행 전체(라벨과 컨트롤)의 응답이다.
pub fn settings_stack_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    row: StackRow<'_>,
    control: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let gap = theme.settings_label_gap().value();
    let side_width = ui.available_width() - row.label_col.value() - gap;
    let prev: StackState = ui.data(|d| d.get_temp(row.id)).unwrap_or_default();
    let stacked = row_should_stack(
        prev.stacked,
        prev.natural,
        side_width,
        theme.settings_row_stack_hysteresis().value(),
    );

    let mut natural = 0.0;
    let response = if stacked {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.settings_row_stack_gap().value();
            let full = LogicalPx(ui.available_width());
            settings_label_cell(ui, theme, full, row.row_h, row.label, row.hint);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.set_min_height(row.row_h.value());
                natural = ui.scope(control).response.rect.width();
            });
        })
        .response
    } else {
        let body = |ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // 가운데 정렬 행은 줄바꿈한 라벨 높이까지 미리 잡아야 컨트롤이 그 가운데에 선다.
            let label_h = label_cell_height(ui, theme, row.label_col, row.label, row.hint);
            ui.set_min_height(row.row_h.max(label_h).value());
            settings_label_cell(ui, theme, row.label_col, row.row_h, row.label, row.hint);
            ui.add_space(gap);
            natural = ui.scope(control).response.rect.width();
        };
        if row.align_top {
            ui.horizontal_top(body).response
        } else {
            ui.horizontal(body).response
        }
    };

    ui.data_mut(|d| {
        d.insert_temp(
            row.id,
            StackState {
                stacked,
                natural: Some(natural),
            },
        )
    });
    let next = row_should_stack(
        stacked,
        Some(natural),
        side_width,
        theme.settings_row_stack_hysteresis().value(),
    );
    if next != stacked {
        ui.ctx().request_discard("settings row stack changed");
    }
    response
}

#[cfg(test)]
mod tests {
    use super::row_should_stack;

    #[test]
    fn a_side_row_stacks_once_the_control_is_wider_than_the_space() {
        assert!(!row_should_stack(false, Some(200.0), 200.0, 16.0));
        assert!(row_should_stack(false, Some(200.5), 200.0, 16.0));
    }

    #[test]
    fn a_stacked_row_goes_back_only_with_the_hysteresis_to_spare() {
        assert!(row_should_stack(true, Some(190.0), 200.0, 16.0));
        assert!(!row_should_stack(true, Some(184.0), 200.0, 16.0));
    }

    #[test]
    fn an_unmeasured_row_keeps_its_state() {
        assert!(!row_should_stack(false, None, 0.0, 16.0));
        assert!(row_should_stack(true, None, 1000.0, 16.0));
    }
}
