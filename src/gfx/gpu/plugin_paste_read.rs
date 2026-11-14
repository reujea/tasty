//! 플러그인 입력칸(회신을 약속한 팝업·egui-mesh surface)에 보낼 붙여넣기 글자의 네이티브 Wayland 늦은 읽기.
//! 읽기는 창의 [`crate::file::drag::TextSelection`]이 하고, 여기서는 요청마다 새 tag 를 붙여 기다리는 읽기의
//! 결과를 호스트 입력칸·`HeldPastes`보다 먼저 가려 App 에 넘긴다. App 은 그 글자로 편집 요청을 보내거나 원래 키를
//! 재생한다(`crate::app::plugin_edit_holds`). 계약: `docs/design/policies/key-mapping.md`의 "플러그인 입력칸" 절.

use std::collections::VecDeque;

/// 플러그인 대상 붙여넣기 키가 키 시점에 읽은 클립보드 글자.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PluginClip {
    /// 붙여넣을 글자.
    Text(String),
    /// 글자가 없다(빈 클립보드, 글자 아닌 형식만 있음, 읽기 실패).
    Empty,
    /// 네이티브 Wayland 에서 다른 앱의 선택을 읽는 중이다. 결과는 이 tag 로 나중에 온다.
    Pending(u64),
}

impl PluginClip {
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// 기다리던 읽기가 끝난 모양.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PluginPasteEnd {
    /// 붙여넣을 글자가 왔다.
    Text(String),
    /// 글자가 없거나 읽기가 실패했다.
    Nothing,
    /// 다른 읽기(다른 창, 터미널 붙여넣기)가 공유 읽기를 대신해 결과가 오지 않는다.
    Replaced,
}

/// 끝난 읽기를 더 기억하는 수. 그 tag 로 늦게 온 결과는 다른 입력칸에 들어가지 않고 버려진다.
const RETIRED_TAGS: usize = 8;

/// 창마다 하나인 플러그인 붙여넣기 읽기. 창의 입력은 기다리는 동안 줄을 서므로 한 번에 하나만 기다린다.
#[derive(Debug, Default)]
pub(crate) struct PluginPasteReads {
    seq: u64,
    waiting: Option<u64>,
    arrived: Vec<(u64, PluginPasteEnd)>,
    retired: VecDeque<u64>,
}

impl PluginPasteReads {
    /// 새 읽기의 tag. 요청마다 달라 끝난 읽기의 결과가 다음 요청이나 호스트 입력칸의 것으로 읽히지 않는다.
    pub(crate) fn next_tag(&mut self) -> u64 {
        self.seq += 1;
        plugin_read_tag(self.seq)
    }

    /// `tag`의 읽기를 기다리기 시작한다. 앞서 기다리던 읽기는 공유 읽기가 대신했으므로 끝낸다.
    pub(crate) fn started(&mut self, tag: u64) {
        if let Some(earlier) = self.waiting.replace(tag) {
            self.retire(earlier);
            self.arrived.push((earlier, PluginPasteEnd::Replaced));
        }
    }

    /// 실패한 읽기도 결과로 받아야 한다(`poll_with_failures`).
    pub(crate) fn wants_failures(&self) -> bool {
        self.waiting.is_some()
    }

    /// 끝난 읽기에서 기다리던 것을 가져가고 나머지를 돌려준다. 끝낸 읽기의 늦은 결과는 버린다. `still_reading`은
    /// 이 창의 읽기가 아직 진행 중인지다. 기다리는데 결과도 없고 진행 중도 아니면 다른 읽기가 대신한 것이다.
    pub(crate) fn sort(
        &mut self,
        finished: Vec<(u64, Option<String>)>,
        still_reading: bool,
    ) -> Vec<(u64, Option<String>)> {
        let mut rest = Vec::new();
        for (tag, text) in finished {
            if self.waiting == Some(tag) {
                self.waiting = None;
                self.retire(tag);
                let end = match text.filter(|t| !t.is_empty()) {
                    Some(text) => PluginPasteEnd::Text(text),
                    None => PluginPasteEnd::Nothing,
                };
                self.arrived.push((tag, end));
            } else if self.retired.contains(&tag) {
                tracing::debug!("plugin paste read result dropped: its paste already ended");
            } else {
                rest.push((tag, text));
            }
        }
        if !still_reading && let Some(tag) = self.waiting.take() {
            self.retire(tag);
            self.arrived.push((tag, PluginPasteEnd::Replaced));
        }
        rest
    }

    /// `tag`의 읽기를 더 기다리지 않는다. 그 읽기가 아직 진행 중이면 true 이고 호출자가 읽기를 취소한다.
    pub(crate) fn cancel(&mut self, tag: u64) -> bool {
        self.retire(tag);
        self.arrived.retain(|(arrived, _)| *arrived != tag);
        if self.waiting == Some(tag) {
            self.waiting = None;
            return true;
        }
        false
    }

    /// App 에 넘길 끝난 읽기.
    pub(crate) fn take_arrived(&mut self) -> Vec<(u64, PluginPasteEnd)> {
        std::mem::take(&mut self.arrived)
    }

    /// 기다리는 읽기나 넘기지 않은 결과가 있다. 폴링과 다시 그리기가 이어져야 한다.
    pub(crate) fn busy(&self) -> bool {
        self.waiting.is_some() || !self.arrived.is_empty()
    }

    fn retire(&mut self, tag: u64) {
        if self.retired.contains(&tag) {
            return;
        }
        self.retired.push_back(tag);
        while self.retired.len() > RETIRED_TAGS {
            self.retired.pop_front();
        }
    }
}

/// 플러그인 붙여넣기 읽기의 tag. 호스트 위젯 id·팝업 tag 와 겹치지 않는다.
pub(crate) fn plugin_read_tag(seq: u64) -> u64 {
    egui::Id::new(("plugin_paste_read", seq)).value()
}

#[cfg(test)]
#[path = "plugin_paste_read_tests.rs"]
mod tests;
