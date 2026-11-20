//! 한 프레임의 키 주인. 창이 키를 받은 입력 시점에 키 주인을 묶고(`bind_input_key_owner`), 그 키를 실은 그리기가
//! 이번 프레임 값을 한 번 정한다(`decide_frame_keys`). host 본문·접힌 보기 버튼·접기 버튼과 plugin 렌더가 이
//! 값 하나를 쓴다.
//!
//! 창의 키 처리는 그리기보다 먼저 돈다. 그래서 창이 고른 키 주인이 있으면 그 값이 그 프레임의 키 주인이다. 그사이
//! 그 팝업이 닫히거나 범위가 숨어도 다른 팝업으로 옮기지 않는다. 그 키는 처리한 노드에서 멈춘 것으로 본다.
//!
//! 키·IME·텍스트 사건 하나의 주인은 그 사건이 들어온 시점의 포커스 노드다. 포커스 노드는 누름에서만 바뀐다. 그래서
//! 한 그리기 입력에는 한 포커스 노드 몫의 키만 싣는다. 지금 몫에 키 사건이 실렸거나 키 주인이 묶인 뒤 누름이 오면 창은
//! 그 누름 자리에서 egui 입력을 끊고(`cut_key_frame`) 누름부터를 새 몫으로 둔다. 몫은 그리기마다 앞에서 하나씩
//! 꺼낸다. 몫마다 결속과 넘길 Esc 가 따로 있어 창 게이트는 지금 받는 몫(`bound_input_key_owner`)을, 그리기는 그
//! 그리기 몫(`frame_bound_key_owner`)을 읽는다. 한 패스에 끊는 자리 수에는 제한이 없다.

use super::body_keys::HandedEscape;
use super::focus::PopupNode;
use super::occlusion::PopupKeyOwner;
use super::{PopupInstanceId, PopupManager};

/// 이번 프레임의 키 주인 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FrameKeys {
    /// 키 주인. 없으면 이 프레임에는 키 주인이 없다.
    pub(crate) owner: Option<PopupKeyOwner>,
    /// 키 주인이 host 팝업이면 그 인스턴스. 창이 묶은 인스턴스는 닫혔거나 숨었어도 그대로다.
    pub(crate) host: Option<PopupInstanceId>,
}

impl FrameKeys {
    /// 키 주인이 있는 프레임인지. 묶인 주인이 사라진 프레임도 참이다.
    pub(crate) fn held(&self) -> bool {
        self.owner.is_some()
    }
}

/// 한 그리기 몫의 입력 시점 결속.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bound {
    /// 이 팝업이 그 몫의 키를 받았다.
    Popup(PopupNode),
    /// 그 몫의 키는 팝업이 아닌 포커스 노드(egui 로 입력을 받는 surface 등)가 받았다. 누름에서 끊을 때 결속이 없던
    /// 몫에 남긴다. 그리기 때의 포커스(이미 누름이 옮긴 값)로 키 주인을 다시 고르지 않는다.
    NoPopup,
}

impl Bound {
    fn popup(this: Option<Self>) -> Option<PopupNode> {
        match this {
            Some(Self::Popup(node)) => Some(node),
            _ => None,
        }
    }
}

/// 끊은 자리 뒤의 입력 시점 결속(뒤 그리기 몫).
#[derive(Debug, Default)]
struct AfterCut {
    /// 끊은 자리(egui 입력 큐 앞에서 센 사건 수). 이 앞의 사건은 앞 몫이다.
    at: usize,
    bound: Option<Bound>,
    escape: HandedEscape,
}

/// 입력 시점 결속과 이번 프레임 값.
#[derive(Debug, Default)]
pub(crate) struct KeySlots {
    bound: Option<Bound>,
    /// 끊은 자리 뒤의 몫들. 앞에서부터 그리기 순서다.
    after_cut: std::collections::VecDeque<AfterCut>,
    /// 창이 처리해 egui 입력에서 뺀 원래 Esc 사건(수식키 포함)과 그 자리. 묶인 키 주인(host 본문·plugin 렌더)에
    /// 넘긴다.
    escape: HandedEscape,
    frame: Option<FrameKeys>,
    /// 이번 프레임 키 주인에게 넘길 Esc 사건. 키 주인을 정할 때 옮긴다.
    frame_escape: HandedEscape,
}

impl PopupManager {
    /// 창이 키를 처리할 때 그 시점의 키 주인을 묶는다. 이번 프레임에 먼저 묶인 값이 있으면 그대로 둔다(한 프레임의
    /// 키는 첫 키를 받은 주인 하나가 받는다).
    pub(crate) fn bind_input_key_owner(&mut self, owner: PopupNode) {
        let owner = Bound::Popup(owner);
        match self.keys.after_cut.back_mut() {
            Some(after) => after.bound.get_or_insert(owner),
            None => self.keys.bound.get_or_insert(owner),
        };
    }

    /// 누름이 egui 입력 큐 `events` 끝에 들어오려 한다. 지금 몫(마지막으로 끊은 자리부터)에 키·IME·텍스트 사건이
    /// 실렸거나 키 주인이 묶였거나 넘길 Esc 가 있으면 그 자리에서 끊는다. 누름은 포커스 노드를 옮길 수 있으므로 그
    /// 앞의 키와 같은 그리기에 싣지 않는다. 참이면 호출자는 egui 입력도 그 자리에서 끊는다(`InputCuts::cut_at`).
    pub(crate) fn cut_key_frame(&mut self, events: &[egui::Event]) -> bool {
        let (start, bound, escape) = match self.keys.after_cut.back_mut() {
            Some(after) => (after.at, &mut after.bound, !after.escape.is_empty()),
            None => (0, &mut self.keys.bound, !self.keys.escape.is_empty()),
        };
        let carries_keys = events
            .get(start..)
            .is_some_and(|segment| segment.iter().any(is_key_input));
        if !(carries_keys || bound.is_some() || escape) {
            return false;
        }
        // 팝업에 묶이지 않은 앞 몫의 키는 그때의 포커스 노드(팝업 아님) 것이다.
        bound.get_or_insert(Bound::NoPopup);
        self.keys.after_cut.push_back(AfterCut {
            at: events.len(),
            ..AfterCut::default()
        });
        true
    }

    /// egui 패스를 시작한다. `escape_held` 는 직전 패스 끝의 Escape 눌림 상태다. 이 패스 몫으로 넘길 Esc 의 반복
    /// 판정은 이 값에서 시작한다(앞 몫의 누름·뗌을 반영한 값이라 끊은 뒤 몫도 egui 와 같은 출발점이다).
    pub(crate) fn begin_key_pass(&mut self, escape_held: bool) {
        self.keys.escape.start_from(escape_held);
    }

    /// 창이 처리해 egui 입력에서 뺀 원래 Esc 사건을 묶인 키 주인이 읽게 한다(host 본문의 하위 화면 되돌아가기,
    /// plugin 렌더의 닫기·다시 접기). 수식키를 그대로 두고, 빼기 전 자리(`taken` 의 앞선 빼지 않은 사건 수)와 직전 패스
    /// 끝의 Escape 눌림 상태(`held_before`)를 함께 둔다.
    pub(crate) fn hand_escape_to_bound_owner(
        &mut self,
        held_before: bool,
        taken: Vec<(usize, egui::Event)>,
    ) {
        match self.keys.after_cut.back_mut() {
            Some(after) => after.escape.push(held_before, taken),
            None => self.keys.escape.push(held_before, taken),
        }
    }

    /// 이번 프레임 키 주인에게 넘길 Esc 사건을 꺼낸다. host 그리기가 host 키 주인일 때, 남은 것은 plugin 렌더가
    /// 꺼낸다.
    pub(crate) fn take_handed_escape(&mut self) -> HandedEscape {
        std::mem::take(&mut self.keys.frame_escape)
    }

    /// 창이 지금 받는 키의 결속(끊었으면 끊은 뒤 몫). 창의 키 게이트가 읽는다.
    pub(crate) fn bound_input_key_owner(&self) -> Option<PopupNode> {
        Bound::popup(match self.keys.after_cut.back() {
            Some(after) => after.bound,
            None => self.keys.bound,
        })
    }

    /// 이번 그리기 몫의 결속. 그리기 중 읽는다(끊은 뒤 몫은 다음 그리기 것이다).
    pub(crate) fn frame_bound_key_owner(&self) -> Option<PopupNode> {
        Bound::popup(self.keys.bound)
    }

    /// 이번 그리기 몫이 결속됐는지와 그 팝업. `Some(None)`이면 그 몫의 키는 팝업이 아닌 포커스 노드가 받았다.
    pub(crate) fn frame_binding(&self) -> Option<Option<PopupNode>> {
        self.keys.bound.map(|bound| Bound::popup(Some(bound)))
    }

    /// 이번 프레임 키 주인을 정해 이번 그리기에 남긴다. 창이 묶은 값이 있으면 그것이고, 없으면 `live`(이번
    /// 프레임 범위·표시 판정 뒤 host·plugin 후보로 정한 값)다.
    pub(crate) fn decide_frame_keys(&mut self, live: Option<PopupKeyOwner>) -> FrameKeys {
        // 넘길 Esc 는 창이 묶은 주인 것이다. 묶인 값이 없는 프레임에는 넘길 Esc 도 없다.
        let escape = std::mem::take(&mut self.keys.escape);
        let keys = match self.keys.bound.take() {
            Some(Bound::Popup(PopupNode::Host(iid))) => {
                self.keys.frame_escape = escape;
                FrameKeys {
                    owner: Some(PopupKeyOwner::Host),
                    host: Some(iid),
                }
            }
            Some(Bound::Popup(PopupNode::Plugin(id))) => {
                self.keys.frame_escape = escape;
                FrameKeys {
                    owner: Some(PopupKeyOwner::Plugin(id)),
                    host: None,
                }
            }
            Some(Bound::NoPopup) => {
                self.keys.frame_escape = HandedEscape::default();
                FrameKeys::default()
            }
            None => {
                self.keys.frame_escape = HandedEscape::default();
                FrameKeys {
                    owner: live,
                    host: match live {
                        Some(PopupKeyOwner::Host) => self.host_key_owner().map(|p| p.instance),
                        _ => None,
                    },
                }
            }
        };
        self.keys.frame = Some(keys);
        // 끊은 뒤 첫 몫이 다음 그리기의 결속이 된다. 이번 그리기 몫이 큐에서 빠졌으므로 남은 몫의 자리와 넘길 Esc
        // 의 자리를 그만큼 앞당긴다.
        if let Some(after) = self.keys.after_cut.pop_front() {
            let by = after.at;
            self.keys.bound = after.bound;
            self.keys.escape = after.escape.shifted_back(by);
            for rest in &mut self.keys.after_cut {
                rest.at = rest.at.saturating_sub(by);
                rest.escape = std::mem::take(&mut rest.escape).shifted_back(by);
            }
        }
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

/// 키 주인 몫으로 세는 egui 입력 사건(키·글자·IME·붙여넣기·복사·잘라내기).
fn is_key_input(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key { .. }
            | egui::Event::Text(_)
            | egui::Event::Ime(_)
            | egui::Event::Paste(_)
            | egui::Event::Copy
            | egui::Event::Cut
    )
}

/// egui 입력 큐를 끊은 자리들. 창이 누름에서 끊고(`cut_at`) 패스가 입력을 꺼낼 때 앞 자리 하나에서 나눈다
/// (`split_pass`). 자리는 `KeySlots` 의 몫과 같은 순서로 하나씩 꺼낸다.
#[cfg(feature = "gui")]
#[derive(Debug, Default)]
pub(crate) struct InputCuts(std::collections::VecDeque<usize>);

#[cfg(feature = "gui")]
impl InputCuts {
    /// 큐 길이 `len` 자리에서 끊는다.
    pub(crate) fn cut_at(&mut self, len: usize) {
        self.0.push_back(len);
    }

    /// 이번 패스 입력 `events` 를 앞 자리에서 나눈다. 그 뒤 사건은 다음 입력 큐 `next` 앞에 순서대로 되돌리고 남은
    /// 자리는 그만큼 앞당긴다. 나눴으면 참이다(호출자는 다음 패스를 요청한다).
    pub(crate) fn split_pass(
        &mut self,
        events: &mut Vec<egui::Event>,
        next: &mut Vec<egui::Event>,
    ) -> bool {
        let Some(at) = self.0.pop_front() else {
            return false;
        };
        let at = at.min(events.len());
        let rest = events.split_off(at);
        next.splice(0..0, rest);
        for cut in &mut self.0 {
            *cut = cut.saturating_sub(at);
        }
        true
    }
}
