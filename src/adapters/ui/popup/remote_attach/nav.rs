//! 원격 워크스페이스 목록의 방향키 이동과 Enter 확정.

use tasty_remote::browse::RemoteWorkspace;

use super::{Conn, NewWsPhase, UiState, WsSel};

/// 목록이 있을 때 방향키로 행을 옮기고 Enter 를 footer 확정으로 넘긴다. 생성 중에는 목록이 멈춘다.
/// 반환값은 (옮긴 행으로 스크롤할지, Enter 로 확정할지)다.
pub(super) fn handle_keys(ctx: &egui::Context, st: &mut UiState) -> (bool, bool) {
    let Conn::Loaded(ws) = &st.conn else {
        return (false, false);
    };
    if st.creating() {
        return (false, false);
    }
    let (up, down, enter) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
        )
    });
    let mut moved = false;
    if up != down {
        let next = step_sel(&nav_order(ws), st.ws_sel, down);
        if next == Some(WsSel::New) && matches!(st.phase, NewWsPhase::Failed(_)) {
            st.phase = NewWsPhase::Rest;
        }
        st.ws_sel = next;
        moved = true;
    }
    (moved, enter)
}

/// 방향키로 옮긴 행이 보이도록, 방금 그린 행(`top` 부터 커서까지)으로 스크롤한다.
pub(super) fn scroll_to_row(ui: &mut egui::Ui, top: f32, scroll: bool) {
    if scroll {
        let r = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
        ui.scroll_to_rect(r, None);
    }
}

/// 방향키 이동 순서 — 새 행이 1번이고, 이미 attach 됐거나 원격에서도 mirror인(고를 수 없는) 행은 건너뛴다.
fn nav_order(ws: &[RemoteWorkspace]) -> Vec<WsSel> {
    std::iter::once(WsSel::New)
        .chain(
            ws.iter()
                .filter(|w| !w.attached && !w.mirror)
                .map(|w| WsSel::Existing(w.id)),
        )
        .collect()
}

/// 한 칸 이동한 선택. 다른 목록 popup(convert·preset_apply)처럼 양 끝에서 반대쪽으로 돈다.
/// 선택이 없으면 아래는 첫 행, 위는 마지막 행이다.
fn step_sel(order: &[WsSel], cur: Option<WsSel>, down: bool) -> Option<WsSel> {
    let n = order.len();
    if n == 0 {
        return cur;
    }
    let i = match cur.and_then(|c| order.iter().position(|o| *o == c)) {
        Some(i) if down => (i + 1) % n,
        Some(i) => (i + n - 1) % n,
        None if down => 0,
        None => n - 1,
    };
    Some(order[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: u32, attached: bool) -> RemoteWorkspace {
        RemoteWorkspace {
            id,
            name: format!("ws{id}"),
            subtitle: None,
            description: None,
            pane_count: 1,
            busy_count: 0,
            attached,
            holder: None,
            mirror: false,
        }
    }

    #[test]
    fn the_new_row_is_first_and_attached_rows_are_skipped() {
        let order = nav_order(&[ws(1, false), ws(2, true), ws(3, false)]);
        assert_eq!(
            order,
            vec![WsSel::New, WsSel::Existing(1), WsSel::Existing(3)]
        );
        assert_eq!(nav_order(&[]), vec![WsSel::New]);
        let mut chain = ws(4, false);
        chain.mirror = true;
        assert_eq!(
            nav_order(&[chain, ws(5, false)]),
            vec![WsSel::New, WsSel::Existing(5)],
            "a remote mirror cannot be mirrored again"
        );
    }

    #[test]
    fn arrows_start_at_the_ends_and_wrap() {
        let order = nav_order(&[ws(1, false), ws(3, false)]);
        assert_eq!(step_sel(&order, None, true), Some(WsSel::New));
        assert_eq!(step_sel(&order, None, false), Some(WsSel::Existing(3)));
        assert_eq!(
            step_sel(&order, Some(WsSel::New), true),
            Some(WsSel::Existing(1))
        );
        assert_eq!(
            step_sel(&order, Some(WsSel::Existing(3)), true),
            Some(WsSel::New)
        );
        assert_eq!(
            step_sel(&order, Some(WsSel::New), false),
            Some(WsSel::Existing(3))
        );
    }
}
