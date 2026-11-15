#[derive(Debug, Clone)]
pub struct SurfaceMessage {
    pub(crate) id: u32,
    pub(crate) from_surface_id: u32,
    pub(crate) content: String,
}

impl crate::runtime::engine_access::EngineMut<'_> {
    /// Send a message from one surface to another. Returns the assigned message ID.
    pub fn send_message(&mut self, from: u32, to: u32, content: String) -> u32 {
        self.live.surface_next_message_id += 1;
        let id = self.live.surface_next_message_id;
        let msg = SurfaceMessage {
            id,
            from_surface_id: from,
            content,
        };
        self.live.surface_messages.entry(to).or_default().push(msg);
        id
    }

    /// Read (and optionally consume) messages queued for a surface.
    /// If `from` is Some, only return messages from that sender.
    /// If `peek` is false, the returned messages are removed from the queue.
    pub fn read_messages(
        &mut self,
        surface_id: u32,
        from: Option<u32>,
        peek: bool,
    ) -> Vec<SurfaceMessage> {
        let queue = match self.live.surface_messages.get_mut(&surface_id) {
            Some(q) => q,
            None => return vec![],
        };

        if peek {
            queue
                .iter()
                .filter(|m| from.is_none_or(|f| m.from_surface_id == f))
                .cloned()
                .collect()
        } else {
            let mut retained = Vec::new();
            let mut taken = Vec::new();
            for msg in queue.drain(..) {
                if from.is_none_or(|f| msg.from_surface_id == f) {
                    taken.push(msg);
                } else {
                    retained.push(msg);
                }
            }
            *queue = retained;
            if queue.is_empty() {
                self.live.queue_notice_times.forget(surface_id);
            }
            taken
        }
    }

    /// 메시지를 넣은 뒤 받는 surface의 notify 로그에 알릴지 정한다. 없는 surface면 알리지 않는다.
    /// 알려야 하면 현재 대기 수를 돌려준다. 파일 I/O는 호출자가 백그라운드로 넘긴다.
    pub fn queue_notice_due(&mut self, to: u32, now: std::time::Instant) -> Option<usize> {
        if !self.has_surface(to) {
            return None;
        }
        let pending = self.message_count(to);
        let pending_before = pending.saturating_sub(1);
        self.live
            .queue_notice_times
            .should_notify(to, pending_before, now)
            .then_some(pending)
    }

    /// Count messages queued for a surface.
    pub fn message_count(&self, surface_id: u32) -> usize {
        self.live
            .surface_messages
            .get(&surface_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Clear all messages queued for a surface.
    pub fn clear_messages(&mut self, surface_id: u32) {
        self.live.surface_messages.remove(&surface_id);
        self.live.queue_notice_times.forget(surface_id);
    }
}
