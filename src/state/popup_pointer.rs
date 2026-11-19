//! 창의 포인터 처리(winit)·놓기·배너·커서가 포인터 대상을 묻는다. 직전 프레임에 배치·clamp를 마친 팝업
//! 노드(상자, 자식 드롭다운, 범위로 자른 접힌 버튼, 덮는 범위)를 host·plugin 합친 z 순서로 본다. winit 입력은 그
//! 좌표로 egui 프레임을 돌리기 전에 오므로 egui의 hover 판정이 아니라 사각형을 쓴다. 그래서 판정은 직전 프레임의
//! 배치를 따른다(1프레임 지연).

use crate::adapters::ui::popup::occlusion::Occluder;

impl super::MainViewState {
    /// 논리 좌표 `pos`의 포인터 대상인 맨 위 팝업 노드. 팝업이 아니라 아래 내용이 대상이면 `None`이다.
    pub(crate) fn popup_pointer_target(&self, pos: egui::Pos2) -> Option<Occluder> {
        self.host_popup_hittest
            .iter()
            .chain(&self.plugin_popup_hittest)
            .chain(&self.host_popup_covers)
            .chain(&self.plugin_popup_covers)
            .filter(|o| o.rect.contains(pos))
            .max_by_key(|o| o.z_seq)
            .copied()
    }

    /// 논리 좌표 `pos`의 포인터 대상이 팝업 노드인지. 참이면 아래 surface·분할선·배너·탐색기는 그 포인터를
    /// 받지 않는다.
    pub(crate) fn popup_owns_point(&self, pos: egui::Pos2) -> bool {
        self.popup_pointer_target(pos).is_some()
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::ui::popup::occlusion::Occluder;

    fn r(x: f32, w: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(w, 100.0))
    }

    /// 겹친 노드 가운데 z가 가장 큰 것이 대상이다. host·plugin, 상자·덮는 범위를 가리지 않는다. 노드 밖은 아래
    /// 내용이 대상이다.
    #[test]
    fn the_highest_node_under_the_pointer_is_the_target() {
        let (mut state, _session) = crate::state::tests::test_state();
        state.host_popup_hittest = vec![Occluder {
            rect: r(0.0, 50.0),
            z_seq: 1,
        }];
        state.plugin_popup_covers = vec![Occluder {
            rect: r(20.0, 100.0),
            z_seq: 2,
        }];
        state.host_popup_covers = vec![Occluder {
            rect: r(40.0, 20.0),
            z_seq: 3,
        }];
        let at = |x: f32| {
            state
                .popup_pointer_target(egui::pos2(x, 50.0))
                .map(|o| o.z_seq)
        };
        assert_eq!(at(10.0), Some(1));
        assert_eq!(at(30.0), Some(2));
        assert_eq!(at(45.0), Some(3));
        assert_eq!(at(110.0), Some(2));
        assert_eq!(at(130.0), None);
        assert!(!state.popup_owns_point(egui::pos2(130.0, 50.0)));
    }
}
