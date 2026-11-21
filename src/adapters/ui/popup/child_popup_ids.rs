//! 포인터 영역 없이 egui popup id만 남기는 자식 팝업 목록. rect 등록부(`child_overlay_registry_id`)와 따로 두어
//! 바깥 누름·포인터 대상 판정이 읽지 않는다. 열림은 읽을 때 egui `is_popup_open`으로 본다. 상자와 배치 범위 밖으로
//! 나올 수 있는 자식(파일 선택기 숨은 경로 메뉴)이 쓴다.

use super::PopupInstanceId;

fn registry_id() -> egui::Id {
    egui::Id::new("popup.child_popup_id_registry")
}

type ChildPopupIdMap = std::collections::HashMap<(PopupInstanceId, &'static str), egui::Id>;

/// `instance`의 자식 팝업 id를 보고한다. None이면 지운다.
pub(crate) fn report_child_popup_id(
    ctx: &egui::Context,
    instance: PopupInstanceId,
    key: &'static str,
    popup: Option<egui::Id>,
) {
    let id = registry_id();
    ctx.memory_mut(|m| {
        let mut map: ChildPopupIdMap = m.data.get_temp(id).unwrap_or_default();
        match popup {
            Some(p) => {
                map.insert((instance, key), p);
            }
            None => {
                map.remove(&(instance, key));
            }
        }
        m.data.insert_temp(id, map);
    });
}

/// `open`이 거짓인 인스턴스의 항목을 지운다.
pub(super) fn retain(ctx: &egui::Context, open: impl Fn(PopupInstanceId) -> bool) {
    let id = registry_id();
    ctx.memory_mut(|m| {
        let Some(mut map) = m.data.get_temp::<ChildPopupIdMap>(id) else {
            return;
        };
        let before = map.len();
        map.retain(|(owner, _), _| open(*owner));
        if map.len() != before {
            m.data.insert_temp(id, map);
        }
    });
}

#[cfg(test)]
/// 시험이 자식 팝업 id 목록을 본다.
pub(crate) fn child_popup_ids(
    ctx: &egui::Context,
) -> Vec<((PopupInstanceId, &'static str), egui::Id)> {
    let id = registry_id();
    ctx.memory(|m| {
        m.data
            .get_temp::<ChildPopupIdMap>(id)
            .unwrap_or_default()
            .into_iter()
            .collect()
    })
}

#[cfg(test)]
/// 시험이 rect 등록부 항목 하나(egui popup id, 영역)를 본다.
pub(crate) fn child_overlay_entry(
    ctx: &egui::Context,
    instance: PopupInstanceId,
    overlay_key: &'static str,
) -> Option<(egui::Id, egui::Rect)> {
    let id = super::child_overlay_registry_id();
    ctx.memory(|m| {
        m.data
            .get_temp::<super::ChildOverlayMap>(id)
            .unwrap_or_default()
            .get(&(instance, overlay_key))
            .copied()
    })
}
