//! 한 프레임의 키 주인. 창이 키를 받은 입력 시점에 키 주인을 묶고(`bind_input_key_owner`), 그 키를 실은 그리기가
//! 이번 프레임 값을 한 번 정한다(`decide_frame_keys`). host 본문·접힌 보기 버튼·접기 버튼과 plugin 렌더가 이
//! 값 하나를 쓴다.
//!
//! 창의 키 처리는 그리기보다 먼저 돈다. 그래서 창이 고른 키 주인이 있으면 그 값이 그 프레임의 키 주인이다. 그사이
//! 그 팝업이 닫히거나 범위가 숨어도 다른 팝업으로 옮기지 않는다. 그 키는 처리한 노드에서 멈춘 것으로 본다.
//!
//! 키·IME·텍스트 사건 하나의 주인은 그 사건이 들어온 시점의 포커스 노드다. egui 패스 하나는 입력 몫 하나이고 몫
//! 하나는 포커스 노드 하나 몫이다. 사건 시점의 포커스 노드가 지금 몫의 노드와 다르면 그 사건 자리에서 egui 입력을
//! 끊는다. 같은 노드라도 지금 몫에 키 사건이 실렸거나 키 주인이 묶인 뒤 누름이 오면 그 누름 자리에서, 누름 뒤에 키가
//! 오면 그 키 자리에서 끊는다(`cut_key_frame`). 몫은 egui 패스마다 앞에서 하나씩
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
    /// 이번 그리기 몫의 결속이 `Bound::Popup`(이 주인 팝업)인지. 창이 그 몫의 키를 팝업에 묶을 때
    /// (`bind_input_key_owner`)와 끊어 닫힌 몫의 포커스 노드가 팝업일 때(`cut_key_frame`, 같은 노드의 누름 끊기 포함)
    /// 생긴다. `Bound::NoPopup` 은 결속이 아니라 이 값은 거짓이고 키 주인도 없다. 결속이 없는 몫도 거짓이고 그때는
    /// 그리기가 정한 주인이다.
    pub(crate) bound: bool,
}

impl FrameKeys {
    /// 키 주인이 있는 프레임인지. 묶인 주인이 사라진 프레임도 참이다.
    pub(crate) fn held(&self) -> bool {
        self.owner.is_some()
    }
}

/// 이 노드가 이번 프레임 키를 받는지. `target`은 그 쪽이 이미 고른 이번 프레임 키 대상이다(host 본문은
/// [`FrameKeys::host`], plugin 렌더는 `frame_plugin_key_target`). 비교만 하고 결정이 없을 때 대상을 고르지 않는다.
pub(crate) fn receives_frame_keys<T: PartialEq>(target: Option<T>, node: T) -> bool {
    target == Some(node)
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

/// 사건 시점의 노드가 지금 몫의 노드와 다른지(`cut_key_frame`). 메인 창 몫의 노드는 포커스 노드와 그 몫 키 시점의
/// 복원 안내 인스턴스다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeChange {
    Same,
    /// 포커스 노드는 같고 키 시점의 안내 인스턴스만 다르다. 끊지만 노드 전환이 아니라 빈 패스를 먼저 두지 않는다.
    Notice,
    /// 포커스 노드가 다르다.
    Focus,
}

/// egui 입력 큐 끝에 들어오려는 사건의 종류(`cut_key_frame`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Incoming {
    /// 포인터 누름. 포커스 노드를 옮길 수 있다.
    Press,
    /// 키·IME 사건.
    Key,
    /// 수식키 없는 Esc 누름(`RouteKey::bare_escape`). 자기 몫 하나를 갖는다(조건 8).
    Escape,
}

impl Incoming {
    /// 키·IME·Esc 사건인지(누름이 아닌지).
    pub(crate) fn is_key(self) -> bool {
        !matches!(self, Self::Press)
    }
}

/// 끊은 자리 뒤의 입력 시점 결속(뒤 그리기 몫).
#[derive(Debug, Default)]
struct AfterCut {
    /// 끊은 자리(egui 입력 큐 앞에서 센 사건 수). 이 앞의 사건은 앞 몫이다.
    at: usize,
    bound: Option<Bound>,
    escape: HandedEscape,
    /// egui 로 보내지 않은 Esc 뗌이 이 몫에 왔다(`note_unfed_escape_release`).
    escape_released: bool,
    /// 사건 시점의 포커스 노드가 바뀌어 연 몫이다. 이 몫이 비어 있는 동안 키가 오면 한 번 더 끊어, 이 몫은 새 노드의
    /// 빈 패스가 된다.
    node_transition: bool,
    /// egui 수신자로 간 bare Esc 누름이 든 몫이다(`note_escape_loaded`). 다음 사건은 새 몫에서 시작한다.
    escape_loaded: bool,
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
    /// 지금 패스가 자기 몫을 꺼냈는지. 패스를 닫을 때(`close_key_pass`) 꺼내지 않은 몫을 꺼내 버린다.
    pass_taken: bool,
    /// 지금 받는 첫 몫에 egui 로 보내지 않은 Esc 뗌이 왔다.
    escape_released: bool,
    /// 이번 패스 몫에 egui 로 보내지 않은 Esc 뗌이 왔다. 패스를 닫을 때 egui 의 Escape 눌림을 푼다.
    pass_escape_released: bool,
    /// 지금 받는 첫 몫에 egui 수신자로 간 bare Esc 누름이 들었다.
    escape_loaded: bool,
    /// 별도 창이 이번 `run` 에 정한 키 주인과 그 `run` 번호(`decide_window_frame_keys`).
    window_frame: Option<(u64, FrameKeys)>,
    /// 이번 그리기 몫 키들의 사건 시점 복원 안내 인스턴스(`MainViewState::pass_notice`). 있으면 포커스를 받지 않는
    /// 팝업 가운데 그 인스턴스만 키를 받는다.
    pass_notice: Option<PopupInstanceId>,
}

impl PopupManager {
    /// 이번 그리기 몫의 안내 인스턴스를 넣는다. 메인 창 팝업 층이 그리기 전에 패스마다 부른다.
    pub(crate) fn set_pass_notice(&mut self, notice: Option<PopupInstanceId>) {
        self.keys.pass_notice = notice;
    }

    /// 이번 그리기 몫의 안내 인스턴스(`set_pass_notice`).
    pub(super) fn pass_notice(&self) -> Option<PopupInstanceId> {
        self.keys.pass_notice
    }

    /// 창이 키를 처리할 때 그 시점의 키 주인을 묶는다. 이번 프레임에 먼저 묶인 값이 있으면 그대로 둔다(한 프레임의
    /// 키는 첫 키를 받은 주인 하나가 받는다).
    pub(crate) fn bind_input_key_owner(&mut self, owner: PopupNode) {
        let owner = Bound::Popup(owner);
        match self.keys.after_cut.back_mut() {
            Some(after) => after.bound.get_or_insert(owner),
            None => self.keys.bound.get_or_insert(owner),
        };
    }

    /// `incoming` 사건이 egui 입력 큐 `events` 끝에 들어오려 한다. 지금 몫(마지막으로 끊은 자리부터)과 포커스 노드가
    /// 다르거나 다를 수 있으면 그 자리에서 끊는다. 누름은 포커스 노드를 옮길 수 있으므로, 누름이 오는데 지금 몫에 키·IME·텍스트
    /// 사건이 실렸거나 키 주인이 묶였거나 넘길 Esc 가 있으면 끊는다. 키가 오는데 지금 몫의 마지막 입력이 누름이면
    /// 끊는다(그 누름이 포커스를 준 위젯은 누름을 처리한 패스 뒤에야 egui 포커스를 가진다). bare Esc 누름은 자기 몫
    /// 하나를 갖는다(조건 8). 앞 경계: Esc 가 오는데 지금 몫에 키·누름 사건이 실렸거나 키 주인이 묶였거나 넘길 Esc 가
    /// 있으면 끊는다(수신자를 읽지 않는다). 뒤 경계: egui 수신자로 간 Esc 가 든 몫(`note_escape_loaded`)에는 다음 사건이
    /// 들지 않는다. 끊는 조건은 이 함수에만 있다.
    ///
    /// `node` 는 그 사건 시점의 노드가 지금 몫의 노드와 다른지다(`MainViewState::cut_key_frame`). 다르면 사건 종류와
    /// 관계없이 끊는다. 포커스 노드가 달라(`NodeChange::Focus`) 연 몫에 아무 사건도 들기 전에 키가 오면 한 번 더 끊는다.
    /// 앞 몫의 마지막 사건(검색 단축키, 무대 종료 키, Esc 해제, 누름 등)이 포커스 노드를 바꿨으면 새 노드는 빈 패스 하나를
    /// 먼저 그린다. 새 노드의 입력칸은 그 패스에서 egui 키 포커스를 얻고 다음 패스에서 그 키를 받는다(누름 뒤 키와 같은
    /// 이유).
    ///
    /// 끊으면 닫히는 몫에 결속이 없을 때 `focus`(닫히는 몫의 포커스 노드가 팝업이면 그 노드)로 묶는다. 참이면 호출자는
    /// egui 입력도 그 자리에서 끊는다(`InputCuts::cut_at`).
    pub(crate) fn cut_key_frame(
        &mut self,
        events: &[egui::Event],
        incoming: Incoming,
        focus: Option<PopupNode>,
        node: NodeChange,
    ) -> bool {
        let (start, bound, escape, transition, escape_loaded) = match self.keys.after_cut.back_mut()
        {
            Some(after) => (
                after.at,
                &mut after.bound,
                !after.escape.is_empty(),
                after.node_transition,
                after.escape_loaded,
            ),
            None => (
                0,
                &mut self.keys.bound,
                !self.keys.escape.is_empty(),
                false,
                self.keys.escape_loaded,
            ),
        };
        let segment = events.get(start..).unwrap_or_default();
        let key_cut = || {
            (transition && segment.is_empty())
                || segment
                    .iter()
                    .rev()
                    .find(|e| is_key_input(e) || is_press(e))
                    .is_some_and(is_press)
        };
        let cut = node != NodeChange::Same
            || escape_loaded
            || match incoming {
                Incoming::Press => segment.iter().any(is_key_input) || bound.is_some() || escape,
                Incoming::Key => key_cut(),
                Incoming::Escape => {
                    key_cut()
                        || segment.iter().any(|e| is_key_input(e) || is_press(e))
                        || bound.is_some()
                        || escape
                }
            };
        if !cut {
            return false;
        }
        bound.get_or_insert(focus.map_or(Bound::NoPopup, Bound::Popup));
        self.keys.after_cut.push_back(AfterCut {
            at: events.len(),
            node_transition: node == NodeChange::Focus,
            ..AfterCut::default()
        });
        true
    }

    /// 지금 사건을 받는 몫(뒤 몫의 꼬리, 없으면 이번 몫)에 원시 사건도 결속·넘길 Esc·미전달 Esc 뗌 표시도 없는지.
    /// `raw_left` 는 남은 egui 입력 큐 길이다(`MainViewState::seal_after_drawing_transition`).
    pub(crate) fn open_segment_is_bare(&self, raw_left: usize) -> bool {
        match self.keys.after_cut.back() {
            Some(after) => {
                raw_left == after.at
                    && after.bound.is_none()
                    && after.escape.is_empty()
                    && !after.escape_released
                    && !after.escape_loaded
            }
            None => {
                raw_left == 0
                    && self.keys.bound.is_none()
                    && self.keys.escape.is_empty()
                    && !self.keys.escape_released
                    && !self.keys.escape_loaded
            }
        }
    }

    /// 지금 사건을 받는 몫을 남은 egui 입력 큐 끝(`at`)에서 닫고 결속 없는 새 몫을 연다(그리기 중 전환의 봉인,
    /// `MainViewState::seal_after_drawing_transition`). 끊는 조건을 거치지 않고, 닫는 몫의 결속·넘길 Esc 는 그대로 둔다.
    pub(crate) fn seal_open_segment(&mut self, at: usize) {
        self.keys.after_cut.push_back(AfterCut {
            at,
            ..AfterCut::default()
        });
    }

    /// egui 수신자로 간(egui 입력에 실린) bare Esc 누름이 지금 받는 몫에 들었다(단계 8 적재 결과). 그 몫은 이 Esc 로
    /// 끝나고 다음 사건은 새 몫에서 시작한다(`cut_key_frame`). egui 가 아닌 수신자로 간 Esc 는 표시를 남기지 않는다.
    pub(crate) fn note_escape_loaded(&mut self) {
        match self.keys.after_cut.back_mut() {
            Some(after) => after.escape_loaded = true,
            None => self.keys.escape_loaded = true,
        }
    }

    /// 키 오버레이가 없어 egui 로 보내지 않은 Esc 뗌이 왔다. 그 뗌은 지금 받는 몫 차례에 egui 의 Escape 눌림을 풀어야
    /// 한다. 앞 몫(끊은 자리 앞의 Esc 누름 등)을 그리기 전에 풀면 그 누름이 남긴 눌림이 뗌 뒤에도 남는다. 그래서 그
    /// 몫의 패스를 닫을 때 푼다(`close_key_pass`).
    pub(crate) fn note_unfed_escape_release(&mut self) {
        match self.keys.after_cut.back_mut() {
            Some(after) => after.escape_released = true,
            None => self.keys.escape_released = true,
        }
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
        let (bound, escape) = self.take_pass_segment();
        let keys = match bound {
            Some(Bound::Popup(PopupNode::Host(iid))) => {
                self.keys.frame_escape = escape;
                FrameKeys {
                    owner: Some(PopupKeyOwner::Host),
                    host: Some(iid),
                    bound: true,
                }
            }
            Some(Bound::Popup(PopupNode::Plugin(id))) => {
                self.keys.frame_escape = escape;
                FrameKeys {
                    owner: Some(PopupKeyOwner::Plugin(id)),
                    host: None,
                    bound: true,
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
                    bound: false,
                }
            }
        };
        self.keys.frame = Some(keys);
        keys
    }

    /// 별도 창 키 사건이 실린 몫의 결속을 그 사건 시점 노드로 적는다. `node` 가 없으면 팝업이 아닌 노드(창 위젯)다.
    /// 그 몫에 먼저 묶인 값이 있으면 그대로 둔다.
    pub(crate) fn bind_window_key_node(&mut self, node: Option<PopupNode>) {
        let bound = node.map_or(Bound::NoPopup, Bound::Popup);
        match self.keys.after_cut.back_mut() {
            Some(after) => after.bound.get_or_insert(bound),
            None => self.keys.bound.get_or_insert(bound),
        };
    }

    /// 별도 창의 이번 패스 키 주인을 정한다. 그 몫의 결속이 있으면 그것이고, 없으면 `owner`(W1 노드의 인스턴스)다
    /// (`decide_frame_keys`). egui 가 같은 `run` 안에서 패스를 다시 돌리거나 그리기가 두 번 물어도 몫은 한 번만
    /// 꺼내고 같은 값을 쓴다.
    pub(crate) fn decide_window_frame_keys(
        &mut self,
        ctx: &egui::Context,
        owner: Option<PopupInstanceId>,
    ) -> FrameKeys {
        let run = ctx
            .cumulative_pass_nr()
            .saturating_sub(ctx.output(|o| o.num_completed_passes) as u64);
        if let Some((at, keys)) = self.keys.window_frame
            && at == run
        {
            self.keys.frame = Some(keys);
            return keys;
        }
        let keys = self.decide_frame_keys(owner.map(|_| PopupKeyOwner::Host));
        self.keys.window_frame = Some((run, keys));
        keys
    }

    /// 이번 패스 그리기가 키를 줄 host 인스턴스. 넣은 값이 없으면 `take_frame_keys`의 대신 정하기와 같은 값이다.
    pub(super) fn pass_key_host(&self) -> Option<PopupInstanceId> {
        match self.keys.frame {
            Some(keys) => keys.host,
            None => self.host_key_owner().map(|p| p.instance),
        }
    }

    /// 이번 패스의 몫(결속·넘길 Esc)을 꺼낸다. 끊은 뒤 첫 몫이 다음 패스의 몫이 된다. 이번 몫이 egui 입력 큐에서
    /// 빠졌으므로 남은 몫의 자리와 넘길 Esc 의 자리를 그만큼 앞당긴다.
    fn take_pass_segment(&mut self) -> (Option<Bound>, HandedEscape) {
        self.keys.pass_taken = true;
        let bound = self.keys.bound.take();
        let escape = std::mem::take(&mut self.keys.escape);
        self.keys.pass_escape_released = std::mem::take(&mut self.keys.escape_released);
        self.keys.escape_loaded = false;
        if let Some(after) = self.keys.after_cut.pop_front() {
            let by = after.at;
            self.keys.bound = after.bound;
            self.keys.escape_released = after.escape_released;
            self.keys.escape_loaded = after.escape_loaded;
            self.keys.escape = after.escape.shifted_back(by);
            for rest in &mut self.keys.after_cut {
                rest.at = rest.at.saturating_sub(by);
                rest.escape = std::mem::take(&mut rest.escape).shifted_back(by);
            }
        }
        (bound, escape)
    }

    /// 아직 그리지 않은 끊은 뒤 몫의 수.
    #[cfg(test)]
    pub(crate) fn cut_segments_left(&self) -> usize {
        self.keys.after_cut.len()
    }

    /// 아직 그리지 않은 끊은 뒤 몫들이 시작하는 egui 입력 큐 좌표(`AfterCut::at`).
    #[cfg(test)]
    pub(crate) fn cut_ats(&self) -> Vec<usize> {
        self.keys.after_cut.iter().map(|a| a.at).collect()
    }

    /// egui 패스 하나를 닫는다. 그리기가 키 주인을 정하지 않은 패스(전체화면 무대)도 자기 몫 하나를 꺼내 버려,
    /// 패스와 몫이 하나씩 짝을 이룬다. 이번 패스의 키 주인과 남은 넘길 Esc 도 비운다. 이 몫에 egui 로 보내지 않은 Esc
    /// 뗌이 왔으면 참이다(호출자가 egui 의 Escape 눌림을 푼다).
    pub(crate) fn close_key_pass(&mut self) -> bool {
        if !self.keys.pass_taken {
            drop(self.take_pass_segment());
        }
        self.keys.pass_taken = false;
        self.keys.frame = None;
        self.keys.frame_escape = HandedEscape::default();
        std::mem::take(&mut self.keys.pass_escape_released)
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

/// 포인터 버튼 누름.
fn is_press(event: &egui::Event) -> bool {
    matches!(event, egui::Event::PointerButton { pressed: true, .. })
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

    /// 남은 끊은 자리의 수.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// 남은 끊은 자리가 없는지.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
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
