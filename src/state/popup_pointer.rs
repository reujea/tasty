//! 창의 포인터 처리(winit)가 포인터 대상이 팝업 노드인지 묻는다. 직전 프레임에 그린 팝업 상자와 덮는 범위로
//! 판정한다. winit 입력은 그 좌표로 egui 프레임을 돌리기 전에 오므로 egui의 hover 판정이 아니라 사각형을 쓴다.
//! 그래서 판정은 직전 프레임의 배치를 따른다(1프레임 지연).

impl super::MainViewState {
    /// 논리 좌표 `pos`의 포인터 대상이 팝업 노드인지. 팝업 상자(포커스를 받지 않는 팝업 포함) 위이거나 덮는
    /// 팝업이 덮은 범위 안이면 참이다. 참이면 아래 surface·분할선·배너는 그 포인터를 받지 않는다.
    pub(crate) fn popup_owns_point(&self, pos: egui::Pos2) -> bool {
        self.host_popup_hittest
            .iter()
            .chain(&self.plugin_popup_hittest)
            .chain(&self.host_popup_covers)
            .chain(&self.plugin_popup_covers)
            .any(|o| o.rect.contains(pos))
    }
}
