//! 별도 창(설정·플러그인·프리셋)의 키 주인(W1). 키는 관리자 포커스 인스턴스가 받는다. 단, 직전 `run_egui`가 끝날 때의
//! egui 포커스 위젯이 그 인스턴스 레이어(등록 자식 포함) 안이거나 포커스 기록이 없을 때만이다. 기록한 포커스가
//! 인스턴스 밖이면 키 주인이 없고 창 위젯이 키를 받는다.

use super::{PopupInstanceId, PopupManager};

/// 별도 창 `run_egui`가 끝난 뒤의 egui 포커스 기록: 포커스 위젯이 마지막 패스에 그려진 레이어. 포커스가 없거나 그
/// 위젯이 마지막 패스에 그려지지 않았으면 기록 없음이다. `end_pass`가 위젯 기록을 바꾼 뒤라 `prev_pass`가 그 패스다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowFocusRecord(Option<egui::LayerId>);

impl WindowFocusRecord {
    /// `run_egui`가 돌아온 뒤 한 번 읽는다.
    pub(crate) fn read(ctx: &egui::Context) -> Self {
        let focused = ctx.memory(|m| m.focused());
        Self(
            focused
                .and_then(|id| ctx.viewport(|v| v.prev_pass.widgets.get(id).map(|w| w.layer_id))),
        )
    }

    /// 포커스 위젯이 그려진 레이어. 기록이 없으면 None.
    #[cfg(test)]
    pub(crate) fn layer(self) -> Option<egui::LayerId> {
        self.0
    }
}

impl PopupManager {
    /// 이 별도 창 관리자의 키 주인(W1). `record`는 직전 `run_egui` 뒤의 포커스 기록이다.
    pub(crate) fn window_key_owner(
        &self,
        ctx: &egui::Context,
        record: WindowFocusRecord,
    ) -> Option<PopupInstanceId> {
        let owner = self.host_key_owner()?;
        let Some(layer) = record.0 else {
            return Some(owner.instance);
        };
        let inside = std::iter::once(super::draw::area_id_of(owner.id, owner.instance))
            .chain(super::child_popup_ids::child_layer_ids(ctx, owner.instance))
            .any(|id| egui::LayerId::new(egui::Order::Foreground, id) == layer);
        inside.then_some(owner.instance)
    }
}

/// 시험이 `text`를 칠한 자리에 마지막 패스에 그린 포커스 받는 위젯(`order` 레이어)을 찾는다.
#[cfg(test)]
pub(crate) fn focusable_at_text(
    ctx: &egui::Context,
    out: &egui::FullOutput,
    text: &str,
    order: egui::Order,
) -> egui::Id {
    let points: Vec<egui::Pos2> = out
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                Some(egui::Rect::from_min_size(t.pos, t.galley.size()).center())
            }
            _ => None,
        })
        .collect();
    assert!(!points.is_empty(), "{text:?} was not painted");
    ctx.viewport(|v| {
        let widgets = &v.prev_pass.widgets;
        widgets
            .layer_ids()
            .filter(|l| l.order == order)
            .flat_map(|l| widgets.get_layer(l))
            .find(|w| w.sense.is_focusable() && points.iter().any(|p| w.rect.contains(*p)))
            .map(|w| w.id)
    })
    .unwrap_or_else(|| panic!("no focusable widget under {text:?} on {order:?}"))
}
