//! Surface 트리의 ID와 별도로 Terminal 인스턴스와 scrollback 저장 ID를 보관한다.
//! busy 상태·대기 scrollback·deferred 생성 정보는 각 수명에 맞는 다른 상태에 둔다.

use std::collections::HashMap;

use std::sync::{Arc, atomic::AtomicU32};
use tasty_terminal::{ColorPalette, Pty, ResourceGeneration, Terminal, TerminalRgb};
use tasty_type_appearance::color::HexColor;

use crate::model::SurfaceId;

fn rgb(c: HexColor) -> TerminalRgb {
    TerminalRgb::new(c.r, c.g, c.b)
}

/// OSC 색 조회용 팔레트. terminal의 focused 색과 ANSI 팔레트를 사용한다.
/// unfocused 화면의 효과까지 반영한 현재 픽셀값은 아니다.
pub(crate) fn current_terminal_palette() -> ColorPalette {
    let theme = crate::theme::theme();
    let surface = theme.surface("terminal");
    let fg = rgb(surface.focused_fg);
    let bg = rgb(surface.focused_bg);
    let ansi = [
        rgb(theme.ansi_black),
        rgb(theme.ansi_red),
        rgb(theme.ansi_green),
        rgb(theme.ansi_yellow),
        rgb(theme.ansi_blue),
        rgb(theme.ansi_magenta),
        rgb(theme.ansi_cyan),
        rgb(theme.ansi_white),
        rgb(theme.ansi_bright_black),
        rgb(theme.ansi_bright_red),
        rgb(theme.ansi_bright_green),
        rgb(theme.ansi_bright_yellow),
        rgb(theme.ansi_bright_blue),
        rgb(theme.ansi_bright_magenta),
        rgb(theme.ansi_bright_cyan),
        rgb(theme.ansi_bright_white),
    ];
    ColorPalette {
        foreground: fg,
        background: bg,
        cursor: fg,
        ansi,
    }
}

pub(crate) struct TerminalStore {
    terminals: HashMap<SurfaceId, (Terminal, Option<Pty>)>,
    next_pty_id: Arc<AtomicU32>,
    exited_retention: std::time::Duration,
    /// Tasty가 스스로 지운 PTY ID와 이유. 상한을 넘으면 오래된 것부터 버린다.
    reclaimed: std::collections::VecDeque<(u32, PtyReclaimReason)>,

    /// Terminal 등록과 별도로 설정하는 디스크 scrollback 저장 ID.
    scrollback_persist_ids: HashMap<SurfaceId, String>,
}

impl TerminalStore {
    pub(crate) fn new(next_pty_id: Arc<AtomicU32>) -> Self {
        Self {
            terminals: HashMap::new(),
            scrollback_persist_ids: HashMap::new(),
            next_pty_id,
            exited_retention: EXITED_RETENTION,
            reclaimed: std::collections::VecDeque::new(),
        }
    }

    /// Each map entry is the only owner of its content and optional physical connection.
    pub(crate) fn insert(&mut self, id: SurfaceId, mut terminal: Terminal, pty: Option<Pty>) {
        if let Some(pty) = &pty {
            assert_eq!(terminal.resource_generation(), pty.generation());
        }
        terminal.set_color_palette(current_terminal_palette());
        if let Some((mut old, _pty)) = self.terminals.insert(id, (terminal, pty)) {
            old.disconnect();
        }
    }

    /// Revoke callbacks before returning old content and its physical owner for disposal.
    pub(crate) fn remove(&mut self, id: SurfaceId) -> Option<(Terminal, Option<Pty>)> {
        self.scrollback_persist_ids.remove(&id);
        let (mut terminal, pty) = self.terminals.remove(&id)?;
        terminal.disconnect();
        Some((terminal, pty))
    }

    pub(crate) fn replace(
        &mut self,
        id: SurfaceId,
        mut terminal: Terminal,
        pty: Option<Pty>,
    ) -> Option<(Terminal, Option<Pty>)> {
        if let Some(pty) = &pty {
            assert_eq!(terminal.resource_generation(), pty.generation());
        }
        terminal.set_color_palette(current_terminal_palette());
        self.terminals
            .insert(id, (terminal, pty))
            .map(|(mut old, pty)| {
                old.disconnect();
                (old, pty)
            })
    }

    /// Transfer to a committed adoption operation without revoking its content/I/O lease.
    pub(crate) fn take_standalone_for_adoption(
        &mut self,
        id: u32,
        generation: u64,
    ) -> Option<(Terminal, Pty, Option<String>)> {
        let owner = self.standalone(id)?;
        if owner.generation().value() != generation || owner.state().exit().is_some() {
            return None;
        }
        let (terminal, pty) = self.terminals.remove(&id)?;
        Some((terminal, pty?, self.scrollback_persist_ids.remove(&id)))
    }
    /// Take back an adopted owner of a withdrawn installation without revoking its connection.
    pub(crate) fn take_adopted(
        &mut self,
        id: SurfaceId,
        generation: u64,
    ) -> Option<(Terminal, Pty, Option<String>)> {
        if self.generation(id)?.value() != generation || self.pty(id).is_none() {
            return None;
        }
        let (terminal, pty) = self.terminals.remove(&id)?;
        Some((terminal, pty?, self.scrollback_persist_ids.remove(&id)))
    }
    pub(crate) fn restore_standalone_adoption(
        &mut self,
        id: u32,
        terminal: Terminal,
        pty: Pty,
        persist_id: Option<String>,
    ) -> Result<(), (Terminal, Pty, Option<String>)> {
        if self.terminals.contains_key(&id) || pty.state().standalone().is_none() {
            return Err((terminal, pty, persist_id));
        }
        self.terminals.insert(id, (terminal, Some(pty)));
        if let Some(persist_id) = persist_id {
            self.scrollback_persist_ids.insert(id, persist_id);
        }
        Ok(())
    }

    pub(crate) fn pty(&self, id: u32) -> Option<&Pty> {
        self.terminals.get(&id)?.1.as_ref()
    }
    pub(crate) fn pty_mut(&mut self, id: u32) -> Option<&mut Pty> {
        self.terminals.get_mut(&id)?.1.as_mut()
    }
    pub(crate) fn generation(&self, id: u32) -> Option<ResourceGeneration> {
        self.get(id).map(Terminal::resource_generation)
    }
    pub(crate) fn matches_generation(&self, id: u32, generation: ResourceGeneration) -> bool {
        self.generation(id) == Some(generation)
    }
    pub(crate) fn cwd(&self, id: u32) -> Option<std::path::PathBuf> {
        self.get(id)?
            .cached_cwd()
            .or_else(|| tasty_terminal::cwd::get_cwd_of_pid(self.pty(id)?.process_id()?))
    }
    /// 로컬 레이아웃이 정한 크기를 적용한다. 헤드리스에는 로컬 레이아웃이 없다.
    #[cfg(feature = "gui")]
    pub(crate) fn resize(&mut self, id: u32, cols: usize, rows: usize) -> bool {
        let Some((terminal, pty)) = self.terminals.get_mut(&id) else {
            return false;
        };
        if !terminal.resize(cols, rows) {
            return false;
        }
        if let Some(pty) = pty {
            pty.schedule_resize(terminal.resource_generation(), cols, rows);
        }
        true
    }

    /// attach 요청용 resize. 같은 크기여도 attach tap에 현재 크기를 남긴다. terminal이 없으면 false다.
    pub(crate) fn resize_for_attach(&mut self, id: u32, cols: usize, rows: usize) -> bool {
        let Some((terminal, pty)) = self.terminals.get_mut(&id) else {
            return false;
        };
        if terminal.resize_or_confirm_attach(cols, rows)
            && let Some(pty) = pty
        {
            pty.schedule_resize(terminal.resource_generation(), cols, rows);
        }
        true
    }

    #[cfg(feature = "gui")]
    pub(crate) fn resync_palettes(&mut self) {
        let palette = current_terminal_palette();
        for (t, _) in self.terminals.values_mut() {
            t.set_color_palette(palette.clone());
        }
    }

    pub(crate) fn get(&self, id: SurfaceId) -> Option<&Terminal> {
        self.terminals.get(&id).map(|(terminal, _)| terminal)
    }

    pub(crate) fn get_mut(&mut self, id: SurfaceId) -> Option<&mut Terminal> {
        self.terminals.get_mut(&id).map(|(terminal, _)| terminal)
    }

    pub(crate) fn contains(&self, id: SurfaceId) -> bool {
        self.terminals.contains_key(&id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (SurfaceId, &Terminal)> {
        self.terminals.iter().map(|(&id, (t, _))| (id, t))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (SurfaceId, &mut Terminal)> {
        self.terminals.iter_mut().map(|(&id, (t, _))| (id, t))
    }

    pub(crate) fn scrollback_persist_id(&self, id: SurfaceId) -> Option<&str> {
        self.scrollback_persist_ids.get(&id).map(String::as_str)
    }

    pub(crate) fn set_scrollback_persist_id(&mut self, id: SurfaceId, persist_id: String) {
        self.scrollback_persist_ids.insert(id, persist_id);
    }

    /// 모든 터미널을 처리하고 읽은 출력이 있던 surface ID를 돌려준다.
    pub(crate) fn process_all(&mut self) -> Vec<SurfaceId> {
        self.terminals
            .iter_mut()
            .filter_map(|(&id, (terminal, pty))| {
                Self::process_pair(terminal, pty.as_mut()).then_some(id)
            })
            .collect()
    }
    pub(crate) fn process_surface(&mut self, id: SurfaceId) -> bool {
        self.terminals
            .get_mut(&id)
            .is_some_and(|(terminal, pty)| Self::process_pair(terminal, pty.as_mut()))
    }
    fn process_pair(terminal: &mut Terminal, pty: Option<&mut Pty>) -> bool {
        if let Some(pty) = pty {
            pty.force_flush_resize();
            if pty.observe_exit() && pty.state().standalone().is_none() {
                terminal.record_exit(pty.generation());
            }
        }
        terminal.process()
    }
    #[cfg(feature = "gui")]
    pub(crate) fn flush_pty_resizes(&mut self) -> bool {
        let mut pending = false;
        for (_, pty) in self.terminals.values_mut() {
            if let Some(pty) = pty {
                pty.flush_resize();
                pending |= pty.has_pending_resize();
            }
        }
        pending
    }
}

/// 터미널에서 닫은 항목 snapshot에 넣을 값을 읽는다. 줄마다 terminal mutex를 잠그지
/// 않도록 스크롤백은 한 번에 읽는다. 스크롤백은 디스크 저장 형식으로 인코딩해 넘기므로
/// 닫기 뒤 저장은 이 바이트를 그대로 쓴다.
#[cfg(test)]
pub(crate) fn closed_capture_of(
    terminal: &Terminal,
    cwd: Option<std::path::PathBuf>,
) -> crate::model::closed_item::TerminalCapture {
    let lines = terminal.scrollback_lines_all();
    crate::model::closed_item::TerminalCapture {
        cwd,
        scrollback: crate::model::closed_item::ScrollbackBlob {
            bytes: tasty_terminal::disk_scrollback::serialize_lines(&lines),
            lines: lines.len(),
        },
    }
}

mod standalone;
pub(crate) use standalone::{EXITED_RETENTION, PTY_ID_BASE, PtyReclaimReason, is_surface_id_space};

#[cfg(test)]
mod process_tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicU32};

    /// 전역 처리는 읽은 출력이 있던 surface만 돌려준다. redraw 유발원 분류가 이 목록을 쓴다.
    #[test]
    fn process_all_reports_only_surfaces_with_new_output() {
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        store.insert(1, Terminal::new_detached(20, 4), None);
        store.insert(2, Terminal::new_detached(20, 4), None);
        assert!(store.get_mut(2).expect("inserted").feed_bytes(b"x"));
        assert_eq!(store.process_all(), vec![2]);
        assert!(store.process_all().is_empty());
    }
}
