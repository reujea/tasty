//! OSC 52 클립보드 읽기 요청을 처리한다.

use super::*;
use crate::runtime::engine_access::EngineMut;

/// 결과를 기다리는 OSC 52 읽기 질의.
#[cfg(feature = "gui")]
pub(crate) struct PendingClipboardRead {
    token: u64,
    surface_id: u32,
    generation: tasty_terminal::ResourceGeneration,
}

impl AppServices {
    /// 설정이 허용할 때만 로컬 클립보드를 읽고 답한다. 기본값은 거절이며 이때 응답 바이트도 보내지 않는다.
    /// 바로 읽을 수 없으면 질의를 보관하고 [`Self::take_finished_clipboard_reads`]가 결과를 넘긴다.
    pub(super) fn handle_clipboard_query(
        &mut self,
        engine: &mut EngineMut<'_>,
        sid: u32,
        generation: tasty_terminal::ResourceGeneration,
    ) {
        if !engine.runtime.settings.general.allow_clipboard_read {
            return;
        }
        match self.clipboard.begin_read_text() {
            Ok(result) => {
                Self::reply_clipboard_query(engine, sid, generation, result);
            }
            #[cfg(feature = "gui")]
            Err(token) => {
                self.pending_clipboard_reads.push(PendingClipboardRead {
                    token,
                    surface_id: sid,
                    generation,
                });
            }
            // 헤드리스 클립보드는 바로 답하거나 실패한다. 결과를 기다릴 루프도 없다.
            #[cfg(not(feature = "gui"))]
            Err(_) => tracing::warn!("OSC 52 clipboard read deferred without an event loop"),
        }
    }

    /// 끝난 읽기를 `(surface, 세대, 결과)`로 꺼낸다. 기다리는 읽기는 남긴다.
    #[cfg(feature = "gui")]
    pub(crate) fn take_finished_clipboard_reads(
        &mut self,
    ) -> Vec<(
        u32,
        tasty_terminal::ResourceGeneration,
        anyhow::Result<String>,
    )> {
        let clipboard = self.clipboard.clone();
        let mut finished = Vec::new();
        self.pending_clipboard_reads
            .retain(|read| match clipboard.finish_read_text(read.token) {
                Some(result) => {
                    finished.push((read.surface_id, read.generation, result));
                    false
                }
                None => true,
            });
        finished
    }

    /// 질의한 터미널이 같은 세대이고 설정이 아직 허용할 때만 답한다. 읽기 실패는 무응답이다.
    pub(crate) fn reply_clipboard_query(
        engine: &mut EngineMut<'_>,
        sid: u32,
        generation: tasty_terminal::ResourceGeneration,
        result: anyhow::Result<String>,
    ) {
        let allow = engine.runtime.settings.general.allow_clipboard_read;
        let clip = match result {
            Ok(text) => Some(text),
            Err(e) => {
                tracing::warn!("OSC 52 clipboard read failed: {e}");
                None
            }
        };
        if engine.runtime.terminals.matches_generation(sid, generation)
            && let Some(reply) = osc52_clipboard_read_reply(allow, clip.as_deref())
            && let Some(terminal) = engine.find_terminal_by_id_mut(sid)
        {
            terminal.send_response_for(generation, &reply);
        }
    }
}

/// 허용된 텍스트를 base64로 감싼 OSC 52 응답(BEL 종료)을 만든다. 거절 또는 텍스트 부재면 None이다.
fn osc52_clipboard_read_reply(allow: bool, clipboard_text: Option<&str>) -> Option<Vec<u8>> {
    if !allow {
        return None;
    }
    let text = clipboard_text?;
    use base64::Engine as _;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    Some(format!("\x1b]52;c;{encoded}\x07").into_bytes())
}

#[cfg(test)]
mod osc52_clipboard_read_tests {
    use super::osc52_clipboard_read_reply;

    #[test]
    fn off_emits_no_bytes() {
        assert_eq!(osc52_clipboard_read_reply(false, Some("secret")), None);
        assert_eq!(osc52_clipboard_read_reply(false, None), None);
    }

    #[test]
    fn on_encodes_clipboard_as_osc52() {
        let reply = osc52_clipboard_read_reply(true, Some("hi")).expect("reply when allowed");
        assert_eq!(reply, b"\x1b]52;c;aGk=\x07".to_vec());
    }

    #[test]
    fn on_with_empty_clipboard_still_replies() {
        assert_eq!(osc52_clipboard_read_reply(true, None), None);
    }
}
