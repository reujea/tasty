//! 팝업 레이어의 egui 그리기 순서 맞추기.

/// egui는 새로 보이거나 눌린 영역만 맨 위로 올리고, 올린 영역끼리는 기존 순서를 지킨다. 매니저가 순서를 바꾼
/// 프레임에는 직전 순서에서 제자리를 벗어난 레이어만 올려 그린 순서(아래에서 위)에 맞춘다. 순서가 그대로인
/// 프레임에는 건드리지 않아 팝업 안에서 연 드롭다운이 아래로 내려가지 않는다. 한 번에 맞출 수 없는 순서는
/// 다음 프레임에 이어서 맞춘다.
pub(super) fn restack_layers(ctx: &egui::Context, layers: &[egui::LayerId]) {
    let key = egui::Id::new("popup.layer_order");
    let last: Vec<egui::LayerId> = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    // 직전 순서에서 사라진 레이어를 빼고, 처음 보이는 레이어는 egui가 맨 위에 둔다.
    let fresh: Vec<egui::LayerId> = layers
        .iter()
        .filter(|l| !last.contains(l))
        .copied()
        .collect();
    let mut now: Vec<egui::LayerId> = last.into_iter().filter(|l| layers.contains(l)).collect();
    now.extend(&fresh);
    let mut lifted = Vec::new();
    let mut highest_kept: Option<usize> = None;
    for layer in layers {
        let at = now.iter().position(|l| l == layer).unwrap_or(now.len());
        if highest_kept.is_none_or(|top| at > top) {
            highest_kept = Some(at);
        } else {
            lifted.push(*layer);
        }
    }
    for layer in &lifted {
        ctx.move_to_top(*layer);
    }
    let raised = |l: &egui::LayerId| lifted.contains(l) || fresh.contains(l);
    let mut next: Vec<egui::LayerId> = now.iter().filter(|l| !raised(l)).copied().collect();
    next.extend(now.iter().filter(|l| raised(l)));
    ctx.data_mut(|d| d.insert_temp(key, next));
}
