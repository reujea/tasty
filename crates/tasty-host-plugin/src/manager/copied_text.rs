//! 플러그인 egui가 복사·잘라내기로 만든 텍스트를 호스트가 꺼낼 때까지 담는 일회성 큐.
//!
//! 프레임처럼 최신값으로 덮어쓰지 않는다. 한 번 꺼낸 결과는 다시 나오지 않는다.

use std::collections::VecDeque;

use tasty_plugin_protocol::CopyTargetWire;

use super::PluginManager;

/// 꺼내지 않은 복사 결과의 최대 수. 소비자가 없는 실행에서 큐가 계속 자라지 않게 한다.
/// 넘으면 가장 오래된 결과부터 버린다.
pub(super) const COPIED_TEXT_QUEUE_LIMIT: usize = 64;

/// 복사 결과의 대상. 알림을 보낸 플러그인이 이 대상을 소유하는지는 소비자가 확인한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCopyTarget {
    /// egui-mesh surface.
    Surface(u32),
    /// egui-mesh popup 인스턴스.
    Popup(u64),
}

impl PluginCopyTarget {
    /// 와이어 대상을 바꾼다. 모르는 대상이면 `None`이다.
    pub(super) fn from_wire(target: CopyTargetWire) -> Option<Self> {
        match target {
            CopyTargetWire::Surface { surface_id } => Some(Self::Surface(surface_id)),
            CopyTargetWire::Popup { instance_id } => Some(Self::Popup(instance_id)),
            CopyTargetWire::Unknown => None,
        }
    }
}

/// 플러그인이 보낸 복사 결과 한 건.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCopiedText {
    /// 알림이 실제로 들어온 채널의 플러그인 id. 플러그인이 주장한 값이 아니다.
    pub plugin_id: String,
    pub target: PluginCopyTarget,
    pub text: String,
}

/// 결과를 큐 끝에 넣고 상한을 넘은 만큼 앞에서 버린다.
pub(super) fn push_bounded(queue: &mut VecDeque<PluginCopiedText>, item: PluginCopiedText) {
    queue.push_back(item);
    while queue.len() > COPIED_TEXT_QUEUE_LIMIT {
        if let Some(dropped) = queue.pop_front() {
            tracing::debug!(
                "plugin '{}' copied text dropped: queue limit {} reached",
                dropped.plugin_id,
                COPIED_TEXT_QUEUE_LIMIT
            );
        }
    }
}

impl PluginManager {
    /// 이 플러그인 실행이 남긴 복사 결과를 지운다. 종료·비활성화·재시작·연결 끊김 때 불러
    /// 다음 실행이 같은 id·대상을 다시 쓰더라도 이전 실행의 결과가 나오지 않게 한다.
    pub(super) fn forget_copied_texts(&mut self, plugin_id: &str) {
        self.copied_texts.retain(|c| c.plugin_id != plugin_id);
    }

    /// 받은 순서대로 surface·popup 복사 결과를 함께 모두 꺼낸다. pump 뒤 같은 tick에서
    /// 한 번 부른다. 결과에는 창 정보가 없으므로 어느 창으로 보낼지는 호출자가 정한다.
    /// 수신 스레드가 알림 줄마다 이벤트 루프를 깨우므로, 프레임 없이 복사 결과만 온
    /// tick에도 pump와 이 호출이 실행된다.
    pub fn take_copied_texts(&mut self) -> Vec<PluginCopiedText> {
        self.copied_texts.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: usize) -> PluginCopiedText {
        PluginCopiedText {
            plugin_id: "com.example".into(),
            target: PluginCopyTarget::Surface(1),
            text: n.to_string(),
        }
    }

    /// 상한을 넘으면 가장 오래된 결과를 버리고 순서는 유지한다.
    #[test]
    fn push_bounded_drops_the_oldest_beyond_the_limit() {
        let mut q = VecDeque::new();
        for n in 0..COPIED_TEXT_QUEUE_LIMIT + 2 {
            push_bounded(&mut q, item(n));
        }
        assert_eq!(q.len(), COPIED_TEXT_QUEUE_LIMIT);
        assert_eq!(q.front().map(|i| i.text.as_str()), Some("2"));
        let last = (COPIED_TEXT_QUEUE_LIMIT + 1).to_string();
        assert_eq!(q.back().map(|i| i.text.as_str()), Some(last.as_str()));
    }

    /// 와이어 대상은 surface·popup 으로 바뀌고 모르는 대상은 버린다.
    #[test]
    fn wire_targets_convert_and_unknown_is_rejected() {
        assert_eq!(
            PluginCopyTarget::from_wire(CopyTargetWire::Surface { surface_id: 3 }),
            Some(PluginCopyTarget::Surface(3))
        );
        assert_eq!(
            PluginCopyTarget::from_wire(CopyTargetWire::Popup { instance_id: 8 }),
            Some(PluginCopyTarget::Popup(8))
        );
        assert_eq!(PluginCopyTarget::from_wire(CopyTargetWire::Unknown), None);
    }
}
