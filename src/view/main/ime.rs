//! OS별 IME 조합 처리. macOS는 조합 종료 때 위치 보정을 초기화한다.
//! Windows·Linux는 빈 Preedit이나 Disabled가 글자마다 올 수 있어 화면만 지우고
//! PTY 에코 위치 보정은 유지한다. 공통 처리는 Commit 문자 폭을 더하고 에코가 따라온 만큼 뺀다.

use crate::runtime::engine_read::EngineRead;
use crate::state::input_destination::InputDestination;
use tasty_plugin_protocol::ImeWire;
use winit::event::Ime;

use super::MainView;
use crate::app::command::{DomainIntent, SendPayload};
use crate::gpu::ImePreeditState;
use crate::view::ui::View as _;

#[derive(Clone)]
pub(crate) struct CompositionTarget {
    selection: std::sync::Weak<()>,
    pending: u64,
    surface: Option<u32>,
    // Keep the prior target for an ensuing Commit; the next nonempty Preedit starts anew.
    preedit_ended: bool,
}
impl CompositionTarget {
    fn capture(w: &MainView, engine: &EngineRead<'_>) -> Self {
        Self {
            selection: w.state.navigation.generation(),
            pending: w.state.attach_pending.selection_serial,
            surface: w.state.focused_surface_id(engine),
            preedit_ended: false,
        }
    }
    fn matches(&self, current: &Self) -> bool {
        self.selection.ptr_eq(&current.selection)
            && self.pending == current.pending
            && self.surface == current.surface
    }
}
fn accepts_commit(w: &mut MainView, engine: &EngineRead<'_>) -> bool {
    let current = CompositionTarget::capture(w, engine);
    w.ime_request_target
        .take()
        .is_none_or(|target| target.matches(&current))
}

#[cfg(test)]
#[test]
fn composition_cannot_cross_pending_selection_even_back_to_the_same_surface() {
    let selection = std::sync::Arc::new(());
    let old = CompositionTarget {
        selection: std::sync::Arc::downgrade(&selection),
        pending: 0,
        surface: Some(7),
        preedit_ended: false,
    };
    let pending = CompositionTarget {
        pending: 1,
        surface: None,
        ..old.clone()
    };
    let returned = CompositionTarget {
        pending: 2,
        ..old.clone()
    };
    assert!(!old.matches(&pending));
    assert!(!old.matches(&returned));
    assert!(!pending.matches(&returned));
    assert!(returned.matches(&returned.clone()));
}

/// IME 입력의 preedit/commit text 를 Intent 큐로 보낸다. surface_id 가
/// None 이거나 text 가 비어있으면 no-op.
fn dispatch_send_text(
    w: &mut MainView,
    engine: &EngineRead<'_>,
    surface_id: Option<u32>,
    text: &str,
) {
    let Some(sid) = surface_id else { return };
    if text.is_empty() {
        return;
    }
    w.state
        .terminal_views
        .update(engine, sid, |viewport, _| viewport.scroll_to_bottom());
    w.state.dispatch_intent(
        DomainIntent::SendToSurface {
            surface_id: sid,
            payload: SendPayload::Text(text.to_string()),
        }
        .from_user_shortcut("ime_commit"),
    );
}

pub(crate) fn handle_event(
    w: &mut MainView,
    engine: &EngineRead<'_>,
    event: Ime,
    egui_consumed: bool,
) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    if egui_consumed {
        w.mark_dirty();
        return;
    }

    let target = CompositionTarget::capture(w, engine);
    match &event {
        // Enabled activates an input method; the first nonempty Preedit begins a composition.
        Ime::Preedit(text, _) if text.is_empty() => {
            if let Some(target) = &mut w.ime_request_target {
                target.preedit_ended = true;
            }
        }
        Ime::Preedit(_, _)
            if w.ime_request_target
                .as_ref()
                .is_none_or(|target| target.preedit_ended) =>
        {
            w.ime_request_target = Some(target.clone())
        }
        Ime::Commit(_) => {
            if !accepts_commit(w, engine) {
                w.ime_preedit = None;
                w.mark_dirty();
                return;
            }
        }
        Ime::Disabled => w.ime_request_target = None,
        _ => {}
    }

    // 팝업·무대가 있으면 조합 문자를 배경 터미널에 보내지 않는다.
    // Enabled/Disabled는 상태 추적에 사용한다. plugin 팝업 입력은 egui 수집기가 이미 전달하므로 여기서 반복하지 않는다.
    // mesh surface는 터미널 오버레이 대신 플러그인에 조합·확정을 전달한다.
    match w
        .state
        .input_destination(engine, w.state.window_keys_to_overlay())
    {
        InputDestination::Overlay => {
            match event {
                Ime::Enabled => {
                    w.ime_active = true;
                }
                Ime::Disabled => {
                    w.ime_active = false;
                }
                Ime::Preedit(..) | Ime::Commit(..) => {}
            }
            w.mark_dirty();
            return;
        }
        InputDestination::EguiMesh(sid) => {
            forward_ime_to_egui_mesh(w, sid, event);
            w.mark_dirty();
            return;
        }
        InputDestination::AttachMesh(sid) => {
            forward_ime_to_attach_mesh(w, sid, event);
            w.mark_dirty();
            return;
        }
        InputDestination::Terminal(_) | InputDestination::Other(_) => {}
    }

    match event {
        Ime::Enabled => w.ime_active = true,
        Ime::Disabled => on_disabled(w, engine),
        Ime::Preedit(text, cursor) => on_preedit(w, engine, text, cursor),
        Ime::Commit(text) => on_commit(w, engine, text),
    }
}

/// mesh로 전달할 때 터미널 조합 상태는 바꾸지 않고 키보드 중복 입력을 막는 ime_active만 갱신한다.
fn forward_ime_to_egui_mesh(w: &mut MainView, surface_id: u32, event: Ime) {
    let wire = match event {
        Ime::Enabled => {
            w.ime_active = true;
            ImeWire::Enabled
        }
        Ime::Disabled => {
            w.ime_active = false;
            ImeWire::Disabled
        }
        Ime::Preedit(text, _cursor) => ImeWire::Preedit { text },
        Ime::Commit(text) => ImeWire::Commit { text },
    };
    w.egui_mesh_push_ime(surface_id, wire);
}

/// [`forward_ime_to_egui_mesh`]의 attach mesh mirror 대응 — 목적지가 원격
/// plugin 이라는 점만 다르다.
fn forward_ime_to_attach_mesh(w: &mut MainView, surface_id: u32, event: Ime) {
    let wire = match event {
        Ime::Enabled => {
            w.ime_active = true;
            ImeWire::Enabled
        }
        Ime::Disabled => {
            w.ime_active = false;
            ImeWire::Disabled
        }
        Ime::Preedit(text, _cursor) => ImeWire::Preedit { text },
        Ime::Commit(text) => ImeWire::Commit { text },
    };
    w.attach_mesh_push_ime(surface_id, wire);
}

/// PTY 출력이 도착해 terminal cursor(또는 TUI의 fake cursor)가 움직였을 수 있을
/// 때 호출. advance가 차감되어 0이 되거나, fake cursor가 최신 위치로 갱신된 순간을
/// 포착해 preedit anchor를 재계산한다.
pub(super) fn recalc_anchor(w: &mut MainView, engine: &EngineRead<'_>) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    let Some(preedit) = &w.ime_preedit else {
        return;
    };
    let surface_id = preedit.surface_id;
    let Some(terminal) = engine.find_terminal_by_id(surface_id) else {
        return;
    };

    let (col, row, cut) = terminal.with_content(|view| {
        let (col, row) = reference_cursor(&view);
        (col, row, view.cut())
    });
    if invalidate_stale_composition(
        &mut w.ime_preedit,
        &mut w.ime_cursor_advance,
        &mut w.ime_advance_base,
        cut.epoch,
    ) {
        w.mark_dirty();
        return;
    }
    if w.ime_cursor_advance == 0 {
        return;
    }
    let cols = cut.cols;
    let (base_col, base_row) = w.ime_advance_base;
    let raw_advance = compute_raw_advance(col, row, base_col, base_row, cols);

    if raw_advance >= w.ime_cursor_advance {
        w.ime_cursor_advance = 0;
    } else {
        w.ime_cursor_advance -= raw_advance;
    }
    w.ime_advance_base = (col, row);

    let (anchor_col, anchor_row) = advanced_anchor(col, row, cols, w.ime_cursor_advance);
    if let Some(p) = &mut w.ime_preedit {
        p.anchor = tasty_selection::SelectionPoint {
            epoch: cut.epoch,
            col: anchor_col,
            absolute_row: cut.screen_start + anchor_row,
        };
    }
}

/// 현재 preedit이 있으면 확정해서 PTY로 보낸다 (단축키 소비 전 호출).
pub(super) fn flush_preedit(w: &mut MainView, engine: &EngineRead<'_>) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    let preedit = match w.ime_preedit.take() {
        Some(p) if !p.text.is_empty() => p,
        _ => {
            w.ime_cursor_advance = 0;
            w.ime_advance_base = (0, 0);
            return;
        }
    };
    let current = CompositionTarget::capture(w, engine);
    if w.ime_request_target
        .as_ref()
        .is_some_and(|target| !target.matches(&current))
    {
        w.ime_cursor_advance = 0;
        w.ime_advance_base = (0, 0);
        w.mark_dirty();
        return;
    }
    if let Some(target) = &mut w.ime_request_target {
        target.preedit_ended = true;
    }
    dispatch_send_text(w, &engine.as_ref(), Some(preedit.surface_id), &preedit.text);
    w.record_typing_intent(engine, preedit.surface_id);
    w.ime_cursor_advance = 0;
    w.ime_advance_base = (0, 0);
    w.mark_dirty();
}

/// 현재 preedit을 PTY로 보내지 않고 버린다.
/// 팝업/오버레이가 열릴 때 조합 중 문자가 터미널로 전달되지 않도록 사용.
pub(super) fn clear_preedit(
    w: &mut MainView,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    w.ime_preedit = None;
    if let Some(target) = &mut w.ime_request_target {
        target.preedit_ended = true;
    }
    w.ime_cursor_advance = 0;
    w.ime_advance_base = (0, 0);
    w.mark_dirty();
}

/// 완전 리셋 — composition 세션 종료(`Disabled`/`Preedit("")`)에서 advance까지 0으로 미는 경로.
/// macOS만 호출한다 (Windows/Linux는 매 글자마다 빈 시그널이 들어와 advance를 보존해야 함).
#[cfg(target_os = "macos")]
fn clear_all(w: &mut MainView, engine: &crate::runtime::engine_read::EngineRead<'_>) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    w.ime_preedit = None;
    w.ime_cursor_advance = 0;
    w.ime_advance_base = (0, 0);
}

// 강제 조합 상태 설정은 debug 전용이며 전체 세션 모델로 처리한다.

#[cfg(debug_assertions)]
pub(crate) fn ipc_set_preedit(
    w: &mut MainView,
    engine: &EngineRead<'_>,
    text: String,
    cursor: Option<(usize, usize)>,
) -> Option<(usize, usize, u32)> {
    // This debug entry remains terminal-only, including a selected pending terminal area.
    if w.state.focused_surface_id(engine).is_some() && w.state.focused_terminal(engine).is_none() {
        return None;
    }
    handle_event(w, engine, Ime::Preedit(text, cursor), false);
    let preedit = w.ime_preedit.as_ref()?;
    let terminal = engine.find_terminal_by_id(preedit.surface_id)?;
    Some((
        preedit.anchor.col,
        preedit
            .anchor
            .absolute_row
            .saturating_sub(terminal.content_cut().screen_start),
        preedit.surface_id,
    ))
}

#[cfg(debug_assertions)]
pub(crate) fn ipc_commit(w: &mut MainView, engine: &EngineRead<'_>, text: &str) {
    if w.state.focused_surface_id(engine).is_some() && w.state.focused_terminal(engine).is_none() {
        return;
    }
    handle_event(w, engine, Ime::Commit(text.to_owned()), false);
}

/// 종료 시 macOS는 위치 보정도 초기화하고 Windows·Linux는 다음 글자의 에코 보정을 위해 유지한다.
fn on_composition_end(w: &mut MainView, engine: &crate::runtime::engine_read::EngineRead<'_>) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    #[cfg(windows)]
    {
        w.ime_preedit = None;
    }
    #[cfg(target_os = "linux")]
    {
        w.ime_preedit = None;
    }
    #[cfg(target_os = "macos")]
    {
        clear_all(w, engine);
    }
}

fn on_disabled(w: &mut MainView, engine: &crate::runtime::engine_read::EngineRead<'_>) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    w.ime_active = false;
    on_composition_end(w, engine);
}

fn on_preedit(
    w: &mut MainView,
    engine: &EngineRead<'_>,
    text: String,
    cursor: Option<(usize, usize)>,
) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    if text.is_empty() {
        on_composition_end(w, engine);
        w.mark_dirty();
        return;
    }

    let surface_id = w.state.focused_surface_id(engine);
    let anchor = reconcile_and_compute_anchor(w, engine);

    w.ime_preedit = match (surface_id, anchor) {
        (Some(sid), Some(anchor)) => Some(ImePreeditState {
            text,
            cursor,
            anchor,
            surface_id: sid,
        }),
        _ => None,
    };
    w.update_ime_cursor_area(&engine.as_ref());
    w.mark_dirty();
}

fn on_commit(w: &mut MainView, engine: &EngineRead<'_>, text: String) {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    if w.ime_cursor_advance == 0
        && let Some(terminal) = w.state.focused_terminal(&engine.as_ref())
    {
        w.ime_advance_base = terminal.with_content(|view| reference_cursor(&view));
    }
    for ch in text.chars() {
        w.ime_cursor_advance += tasty_cell_width::unicode_width(ch);
    }
    w.ime_preedit = None;

    let sid = w.state.focused_surface_id(engine);
    dispatch_send_text(w, &engine.as_ref(), sid, &text);
    if let Some(sid) = sid {
        w.record_typing_intent(engine, sid);
    }
    w.mark_dirty();
}

fn compute_raw_advance(
    col: usize,
    row: usize,
    base_col: usize,
    base_row: usize,
    cols: usize,
) -> usize {
    if row > base_row {
        (row - base_row) * cols + col.saturating_sub(base_col)
    } else {
        col.saturating_sub(base_col)
    }
}

fn advanced_anchor(col: usize, row: usize, cols: usize, advance: usize) -> (usize, usize) {
    let adjusted_col = col + advance;
    if cols > 0 && adjusted_col >= cols {
        (adjusted_col % cols, row + adjusted_col / cols)
    } else {
        (adjusted_col, row)
    }
}

fn reconcile_and_compute_anchor(
    w: &mut MainView,
    engine: &EngineRead<'_>,
) -> Option<tasty_selection::SelectionPoint> {
    let _ = engine; // engine alias: 일부 분기/cfg 에서 미사용 — reborrow 로 unused 경고 억제(값 drop, Result 아님).
    let terminal = w.state.focused_terminal(&engine.as_ref())?;
    let (ref_col, ref_row, cut) = terminal.with_content(|view| {
        let (col, row) = reference_cursor(&view);
        (col, row, view.cut())
    });
    let cols = cut.cols;

    if w.ime_cursor_advance > 0 {
        let (base_col, base_row) = w.ime_advance_base;
        let raw_advance = compute_raw_advance(ref_col, ref_row, base_col, base_row, cols);
        if raw_advance >= w.ime_cursor_advance {
            w.ime_cursor_advance = 0;
        } else {
            w.ime_cursor_advance -= raw_advance;
        }
        w.ime_advance_base = (ref_col, ref_row);
    }

    let (col, row) = advanced_anchor(ref_col, ref_row, cols, w.ime_cursor_advance);
    Some(tasty_selection::SelectionPoint {
        epoch: cut.epoch,
        col,
        absolute_row: cut.screen_start + row,
    })
}

fn invalidate_stale_composition(
    preedit: &mut Option<ImePreeditState>,
    advance: &mut usize,
    base: &mut (usize, usize),
    epoch: tasty_terminal::ContentEpoch,
) -> bool {
    if preedit.as_ref().is_some_and(|p| p.anchor.epoch != epoch) {
        *preedit = None;
        *advance = 0;
        *base = (0, 0);
        true
    } else {
        false
    }
}

/// Preedit/commit 모두가 사용하는 "입력 위치" 좌표.
/// Ink 기반 TUI가 `\e[?25l`로 real cursor를 숨기고 `\e[7m`으로 그린 fake cursor가
/// 있으면 그걸 우선 사용. 없으면 real cursor.
fn reference_cursor(terminal: &tasty_terminal::TerminalReadView<'_>) -> (usize, usize) {
    if !terminal.cursor_visible()
        && let Some(fake) = terminal.find_fake_cursor_cell()
    {
        return fake;
    }
    terminal.cursor_position()
}

#[cfg(test)]
mod viewport_tests {
    use super::*;
    #[test]
    fn stale_preedit_is_cleared_instead_of_rebound_by_echo_reconciliation() {
        let mut terminal = tasty_terminal::Terminal::new_detached(20, 3);
        let mut preedit = Some(ImePreeditState {
            text: "ime".into(),
            cursor: None,
            anchor: tasty_selection::SelectionPoint {
                epoch: terminal.content_cut().epoch,
                col: 1,
                absolute_row: 0,
            },
            surface_id: 1,
        });
        let mut advance = 2;
        let mut base = (1, 0);
        terminal.feed_bytes(b"one\r\ntwo\r\nthree\r\nfour");
        assert!(!invalidate_stale_composition(
            &mut preedit,
            &mut advance,
            &mut base,
            terminal.content_cut().epoch
        ));
        terminal.feed_bytes(b"\x1b[3JNEW");
        assert!(invalidate_stale_composition(
            &mut preedit,
            &mut advance,
            &mut base,
            terminal.content_cut().epoch
        ));
        assert!(preedit.is_none());
        assert_eq!(advance, 0);
        assert_eq!(base, (0, 0));
    }
}
