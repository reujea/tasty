//! 외부 파일 드롭. hover 경로는 안내에, 완료 경로는 프레임 끝의 DispatchFile 처리에 사용한다.
//! X11 DroppedFile에는 좌표가 없어 보관 중인 cursor_position으로 대상을 고른다.
//! 네이티브 Wayland는 끄는 동안 포인터가 창을 떠난 것으로 보이므로 data device 좌표를 함께 넘긴다.

use std::path::PathBuf;

use crate::adapters::ui::ToastScope;
use crate::app::explorer_files::Operation;
use crate::explorer_ui::view::drag::Verdict;
use crate::state::DropHoverState;
use tasty_type_geometry::length::PhysicalPx;

use super::MainView;

/// 놓은 OS 파일을 보낼 곳.
#[derive(Debug, PartialEq)]
enum Route {
    /// hover 중에 explorer 칸이 정해 둔 폴더.
    Hovered(u32, PathBuf, Verdict),
    /// 놓은 좌표를 모른다. 무시한다.
    NoCursor,
    /// 터미널 영역 밖. 안내하고 무시한다.
    Outside,
    /// 놓은 좌표(물리 px) 아래 칸이 받는다. explorer 칸이면 그 폴더로, 아니면 파일을 연다.
    AtCursor(f32, f32),
}

/// 놓은 자리. drop과 함께 온 좌표가 있으면 그것을, 없으면 마지막 포인터 좌표를 쓴다.
fn drop_point(
    dropped_at: Option<(PhysicalPx, PhysicalPx)>,
    cursor: Option<winit::dpi::PhysicalPosition<f64>>,
) -> Option<(PhysicalPx, PhysicalPx)> {
    dropped_at.or_else(|| cursor.map(|pos| (PhysicalPx(pos.x as f32), PhysicalPx(pos.y as f32))))
}

/// hover 가 정한 explorer 칸이 먼저다. 없으면 놓은 좌표와 그 좌표가 터미널 영역 안인지로 정한다.
fn route(hovered: Option<(u32, PathBuf, Verdict)>, cursor: Option<((f32, f32), bool)>) -> Route {
    match (hovered, cursor) {
        (Some((sid, dest, decided)), _) => Route::Hovered(sid, dest, decided),
        (None, None) => Route::NoCursor,
        (None, Some((_, false))) => Route::Outside,
        (None, Some(((x, y), true))) => Route::AtCursor(x, y),
    }
}

/// explorer 칸에 놓은 OS 파일의 작업. 판정이 이동이어도 OS 에서 온 파일은 복사한다. 거절이면 안내 문구다.
fn explorer_drop(
    decided: Verdict,
    dest: PathBuf,
    paths: Vec<PathBuf>,
) -> Result<Operation, String> {
    match decided {
        Verdict::Go(_) => Ok(Operation::Paste {
            paths,
            destination: dest,
            cut: false,
        }),
        Verdict::No(reason) => Err(format!(
            "{}{}",
            crate::i18n::t("explorer.drag.refused"),
            crate::i18n::t_fmt("explorer.drag.refused_reason", crate::i18n::t(reason)),
        )),
    }
}

impl MainView {
    pub(crate) fn handle_hovered_file(&mut self, path: PathBuf) {
        let cursor = self.cursor_position.map(|p| (p.x as f32, p.y as f32));
        self.hover_file_at(path, cursor);
    }

    fn hover_file_at(&mut self, path: PathBuf, cursor: Option<(f32, f32)>) {
        match self.state.drop_hover.as_mut() {
            Some(s) => s.paths.push(path),
            None => {
                self.state.drop_hover = Some(DropHoverState {
                    paths: vec![path],
                    cursor,
                });
            }
        }
        self.base.state.dirty = true;
    }

    pub(crate) fn handle_hovered_file_cancelled(&mut self) {
        crate::explorer_ui::view::drag::take_os_drop(&mut self.state.explorer_views);
        if self.state.drop_hover.take().is_some() {
            self.base.state.dirty = true;
        }
    }

    /// 네이티브 Wayland data device가 넘긴 끌어오기. X11 경로와 같은 함수로 들어간다.
    /// 좌표는 surface 기준 논리 px이므로 이 창의 배율로 물리 px로 바꾼다.
    #[cfg(target_os = "linux")]
    pub(crate) fn apply_inbound_drag(&mut self, events: Vec<crate::file::drag::InboundEvent>) {
        use crate::file::drag::InboundEvent;
        use tasty_type_geometry::length::LogicalPx;
        if events.is_empty() {
            return;
        }
        let sf = self.base.gpu.scale_factor();
        let physical = |(x, y): (f64, f64)| {
            (
                LogicalPx(x as f32).to_physical(sf),
                LogicalPx(y as f32).to_physical(sf),
            )
        };
        // The hover state and its callers still take bare physical px pairs.
        let bare = |(x, y): (PhysicalPx, PhysicalPx)| (x.value(), y.value());
        for event in events {
            match event {
                InboundEvent::Hover { paths, at } => {
                    for path in paths {
                        self.hover_file_at(path, Some(bare(physical(at))));
                    }
                }
                InboundEvent::Motion { at } => {
                    if let Some(hover) = self.state.drop_hover.as_mut() {
                        hover.cursor = Some(bare(physical(at)));
                    }
                }
                InboundEvent::Leave => self.handle_hovered_file_cancelled(),
                InboundEvent::Drop { paths, at } => {
                    self.state.pending_drop_position = Some(physical(at));
                    for path in paths {
                        self.handle_dropped_file(path);
                    }
                }
            }
        }
        self.base.mark_dirty();
    }

    pub(crate) fn handle_dropped_file(&mut self, path: PathBuf) {
        self.state.pending_file_drops.push(path);
        // OS가 취소 이벤트를 보내지 않아도 hover 표시를 지운다.
        self.state.drop_hover = None;
        self.base.state.dirty = true;
    }

    /// explorer 칸 위에 놓은 OS 파일을 그 폴더로 복사한다. 거절된 칸이면 이유를 알린다.
    fn drop_into_explorer(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        surface: u32,
        dest: PathBuf,
        decided: Verdict,
        paths: Vec<PathBuf>,
    ) {
        match explorer_drop(decided, dest, paths) {
            Ok(operation) => self.state.request_explorer_file_direct(
                engine,
                surface,
                operation,
                crate::intent::IntentOrigin::User {
                    source: crate::intent::UserSource::Menu("explorer_os_drop"),
                },
            ),
            Err(text) => self.state.toasts.push_info(text, ToastScope::Window),
        }
        self.base.state.dirty = true;
    }

    /// 쌓인 파일을 DispatchFile로 보낸다. 터미널 영역 밖이면 안내하고 무시한다.
    pub(crate) fn process_pending_file_drops(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
    ) {
        let dropped_at = self.state.pending_drop_position.take();
        let drops = std::mem::take(&mut self.state.pending_file_drops);
        if drops.is_empty() {
            return;
        }
        let hovered = crate::explorer_ui::view::drag::take_os_drop(&mut self.state.explorer_views);
        let terminal_rect = self.compute_terminal_rect();
        let cursor = drop_point(dropped_at, self.cursor_position)
            .map(|(x, y)| ((x.value(), y.value()), terminal_rect.contains(x, y)));
        let (x, y) = match route(hovered, cursor) {
            Route::Hovered(sid, dest, decided) => {
                self.drop_into_explorer(engine, sid, dest, decided, drops);
                return;
            }
            Route::NoCursor => {
                tracing::warn!(
                    count = drops.len(),
                    "file drop with no cursor position — ignored",
                );
                return;
            }
            Route::Outside => {
                self.state
                    .toasts
                    .push_info(crate::i18n::t("file_drop.outside"), ToastScope::Window);
                return;
            }
            Route::AtCursor(x, y) => (x, y),
        };
        {
            let scale_factor = self.base.gpu.scale_factor();
            // best-effort focus 이동. drop 좌표에 pane/surface 가 없으면 현재 focus 유지.
            let _pane_focus =
                self.state
                    .focus_pane_at_position(engine, x, y, terminal_rect, scale_factor);
            let _surface_focus =
                self.state
                    .focus_surface_at_position(engine, x, y, terminal_rect, scale_factor);
            // 드래그 중 위치를 주지 않는 플랫폼에서는 칸이 hover 대상을 정하지 못한다.
            // 놓은 좌표 아래가 explorer 면 보고 있는 폴더로 복사한다.
            let hit = self
                .state
                .surface_at_position(engine, x, y, terminal_rect, scale_factor);
            if let Some(sid) = hit
                && let Some(target) = crate::explorer_ui::view::drag::shown_drop(
                    &self.state.explorer_views,
                    sid,
                    &drops,
                )
            {
                let (dest, decided) = target;
                self.drop_into_explorer(engine, sid, dest, decided, drops);
                return;
            }
        }
        for path in drops {
            self.state.dispatch_intent(
                crate::app::command::DomainIntent::DispatchFile {
                    target: crate::file::format::FileTarget::new(path),
                    depth: crate::file::format::DetectDepth::Deep,
                    origin_surface_id: None,
                    dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
                    ignore_size_limit: false,
                    run: None,
                }
                .from_user_menu("file_drop"),
            );
        }
    }
}

#[cfg(test)]
#[path = "file_drop_tests.rs"]
mod tests;
