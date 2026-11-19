//! 한 프레임의 키 주인. 창이 키를 받은 입력 시점에 키 주인을 묶고(`bind_input_key_owner`), 그 키를 실은 그리기가
//! 이번 프레임 값을 한 번 정한다(`decide_frame_keys`). host 본문·접힌 보기 버튼·접기 버튼과 plugin 렌더가 이
//! 값 하나를 쓴다.
//!
//! 창의 키 처리는 그리기보다 먼저 돈다. 그래서 창이 고른 키 주인이 있으면 그 값이 그 프레임의 키 주인이다. 그사이
//! 그 팝업이 닫히거나 범위가 숨어도 다른 팝업으로 옮기지 않는다. 그 키는 처리한 노드에서 멈춘 것으로 본다.

use super::occlusion::PopupKeyOwner;
use super::{PopupInstanceId, PopupManager};

/// 창이 입력 시점에 고른 키 주인.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundKeyOwner {
    Host(PopupInstanceId),
    /// plugin 팝업 instance id.
    Plugin(u64),
}

/// 이번 프레임의 키 주인 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FrameKeys {
    /// 키 주인. 없으면 이 프레임에는 키 주인이 없다.
    pub(crate) owner: Option<PopupKeyOwner>,
    /// 키 주인이 host 팝업이면 그 인스턴스. 창이 묶은 인스턴스는 닫혔거나 숨었어도 그대로다.
    pub(crate) host: Option<PopupInstanceId>,
    /// 창이 처리해 egui 입력에서 뺀 Esc 를 묶인 host 본문에 넘긴다(`body_keys::hand_escape`).
    pub(crate) escape: bool,
}

impl FrameKeys {
    /// 키 주인이 있는 프레임인지. 묶인 주인이 사라진 프레임도 참이다.
    pub(crate) fn held(&self) -> bool {
        self.owner.is_some()
    }
}

/// 입력 시점 결속과 이번 프레임 값.
#[derive(Debug, Default)]
pub(crate) struct KeySlots {
    bound: Option<BoundKeyOwner>,
    /// 창이 처리한 Esc 를 묶인 host 본문에 넘길지.
    escape: bool,
    frame: Option<FrameKeys>,
}

impl PopupManager {
    /// 창이 키를 처리할 때 그 시점의 키 주인을 묶는다. 이번 프레임에 먼저 묶인 값이 있으면 그대로 둔다(한 프레임의
    /// 키는 첫 키를 받은 주인 하나가 받는다).
    pub(crate) fn bind_input_key_owner(&mut self, owner: BoundKeyOwner) {
        self.keys.bound.get_or_insert(owner);
    }

    /// 창이 처리해 egui 입력에서 뺀 Esc 를 묶인 host 키 주인 본문이 읽게 한다(하위 화면 되돌아가기 등).
    pub(crate) fn hand_escape_to_bound_owner(&mut self) {
        self.keys.escape = true;
    }

    /// 창이 묶은 키 주인. 아직 그리지 않았으면 `Some`이다.
    pub(crate) fn bound_input_key_owner(&self) -> Option<BoundKeyOwner> {
        self.keys.bound
    }

    /// 이번 프레임 키 주인을 정해 이번 그리기에 남긴다. 창이 묶은 값이 있으면 그것이고, 없으면 `live`(이번
    /// 프레임 범위·표시 판정 뒤 host·plugin 후보로 정한 값)다.
    pub(crate) fn decide_frame_keys(&mut self, live: Option<PopupKeyOwner>) -> FrameKeys {
        let escape = std::mem::take(&mut self.keys.escape);
        let keys = match self.keys.bound.take() {
            Some(BoundKeyOwner::Host(iid)) => FrameKeys {
                owner: Some(PopupKeyOwner::Host),
                host: Some(iid),
                escape,
            },
            Some(BoundKeyOwner::Plugin(id)) => FrameKeys {
                owner: Some(PopupKeyOwner::Plugin(id)),
                host: None,
                escape: false,
            },
            None => FrameKeys {
                owner: live,
                host: match live {
                    Some(PopupKeyOwner::Host) => self.host_key_owner().map(|p| p.instance),
                    _ => None,
                },
                escape: false,
            },
        };
        self.keys.frame = Some(keys);
        keys
    }

    /// 그리기가 이번 프레임 키 주인을 꺼낸다. 호출자가 정하지 않았으면(host 그리기만 부르는 갤러리·시험) host
    /// 후보만으로 같은 함수가 정한다.
    pub(super) fn take_frame_keys(&mut self) -> FrameKeys {
        match self.keys.frame.take() {
            Some(keys) => keys,
            None => {
                let live = self.host_key_owner().map(|_| PopupKeyOwner::Host);
                self.decide_frame_keys(live);
                self.keys.frame.take().unwrap_or_default()
            }
        }
    }
}
