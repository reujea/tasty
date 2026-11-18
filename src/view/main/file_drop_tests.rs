//! OS 파일 드롭 분기 시험. hover 가 정한 explorer 칸, 좌표 없음, 터미널 영역 밖, 좌표 아래 칸 순서와
//! explorer 칸에 놓을 때의 작업을 본다.

use super::*;
use crate::app::explorer_files::job::OpKind;

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn the_folder_chosen_while_hovering_wins_over_the_cursor() {
    for cursor in [None, Some(((5.0, 5.0), false)), Some(((5.0, 5.0), true))] {
        assert_eq!(
            route(Some((7, p("/dest"), Verdict::Go(OpKind::Copy))), cursor),
            Route::Hovered(7, p("/dest"), Verdict::Go(OpKind::Copy))
        );
    }
    let refused = Verdict::No("explorer.drag.no_write");
    assert_eq!(
        route(Some((7, p("/dest"), refused)), None),
        Route::Hovered(7, p("/dest"), refused)
    );
}

#[test]
fn without_a_hovered_folder_the_cursor_decides() {
    assert_eq!(route(None, None), Route::NoCursor);
    assert_eq!(route(None, Some(((5.0, 6.0), false))), Route::Outside);
    assert_eq!(
        route(None, Some(((5.0, 6.0), true))),
        Route::AtCursor(5.0, 6.0)
    );
}

/// OS 에서 온 파일은 판정이 이동이어도 복사한다. 원본을 지우지 않는다.
#[test]
fn files_from_the_os_are_always_copied() {
    let paths = vec![p("/os/a.txt"), p("/os/b")];
    for decided in [Verdict::Go(OpKind::Copy), Verdict::Go(OpKind::Move)] {
        let Ok(Operation::Paste {
            paths: sent,
            destination,
            cut,
        }) = explorer_drop(decided, p("/dest"), paths.clone())
        else {
            panic!("{decided:?} must paste");
        };
        assert_eq!((sent, destination, cut), (paths.clone(), p("/dest"), false));
    }
}

#[test]
fn a_refused_folder_explains_why() {
    let reason = "explorer.drag.no_write";
    let Err(text) = explorer_drop(Verdict::No(reason), p("/dest"), vec![p("/os/a.txt")]) else {
        panic!("a refused drop must not paste");
    };
    assert_eq!(
        text,
        format!(
            "{}{}",
            crate::i18n::t("explorer.drag.refused"),
            crate::i18n::t_fmt("explorer.drag.refused_reason", crate::i18n::t(reason))
        )
    );
    assert!(text.contains(crate::i18n::t(reason)));
}

/// 네이티브 Wayland는 끄는 동안 포인터 좌표가 없다. drop과 함께 온 좌표로 칸을 고른다.
#[test]
fn a_drop_position_decides_even_without_a_cursor() {
    use tasty_type_geometry::length::PhysicalPx;
    let dropped = (PhysicalPx(5.0), PhysicalPx(6.0));
    let at = drop_point(Some(dropped), None);
    assert_eq!(at, Some(dropped));
    assert_eq!(
        route(None, at.map(|(x, y)| ((x.value(), y.value()), true))),
        Route::AtCursor(5.0, 6.0)
    );
    let cursor = winit::dpi::PhysicalPosition::new(1.0, 2.0);
    assert_eq!(drop_point(Some(dropped), Some(cursor)), Some(dropped));
    assert_eq!(
        drop_point(None, Some(cursor)),
        Some((PhysicalPx(1.0), PhysicalPx(2.0)))
    );
    assert_eq!(drop_point(None, None), None);
    assert_eq!(route(None, None), Route::NoCursor);
}

/// 탭 바처럼 surface 가 없는 자리에 놓아도 그 좌표의 pane 을 결과 자리로 정한다. 처리 때 포커스로 미루지 않는다.
#[test]
fn a_drop_off_any_surface_binds_the_pane_under_the_cursor() {
    use crate::file::dispatch::FileResultPlace;
    use crate::model::PhysicalPx;
    let (mut state, engine) = crate::state::tests::two_pane_fixture();
    state.tab_bar_height = PhysicalPx(24.0);
    let terminal_rect = crate::model::PhysicalRect {
        x: PhysicalPx(0.0),
        y: PhysicalPx(0.0),
        width: PhysicalPx(800.0),
        height: PhysicalPx(400.0),
    };
    let read = engine.read();
    let ws = state.active_workspace(&read);
    let (_, rect) = state
        .pane_rects(&read, ws, terminal_rect, 1.0)
        .into_iter()
        .find(|(id, _)| *id == 20)
        .expect("pane 20 rect");
    // pane 20 탭 바 위 한 점.
    let (x, y) = (rect.x.value() + 4.0, rect.y.value() + 4.0);
    state.select_pane(&engine.core_state, 20);
    assert_eq!(
        state.surface_at_position(&read, x, y, terminal_rect, 1.0),
        None
    );
    let pane_at = state.pane_at_position(&read, x, y, terminal_rect, 1.0);
    assert_eq!(pane_at, Some(20));
    state.select_pane(&engine.core_state, 10);
    assert_eq!(drop_result_place(None, pane_at), FileResultPlace::Pane(20));
    assert_eq!(
        drop_result_place(Some(10), pane_at),
        FileResultPlace::Pane(10)
    );
    assert_eq!(drop_result_place(None, None), FileResultPlace::Unbound);
}
