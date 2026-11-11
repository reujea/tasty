//! native WebView 키를 공통 정책으로 판정해 host 요청 큐에 넣는다. 실제 실행은 큐를 읽는 호출자가 맡는다.
//! 수식키 유무와 관계없이 바인딩된 조합을 가져간다(Tasty 단축키 우선). plugin·새로고침 조합은 페이지 예약 키와도 비교한다.
//! modifier 조합은 물리 US 배열 문자를 우선하고 물리 위치를 모르면 레이아웃 문자를 사용한다.
//! repeat는 [`WebViewKeySink::consumes_repeat`]로 다시 실행하지 않고 막을지만 정한다. Linux의 키 press는
//! repeat 표시가 없어 반복 이벤트도 일반 press로 전달한다.

use std::cell::RefCell;

use winit::keyboard::{Key, ModifiersState, PhysicalKey};

/// 사용자 편집 바인딩이 native WebView에 실행할 엔진 편집 명령.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebViewEditAction {
    Copy,
    Cut,
    Paste,
    SelectAll,
    /// 페이지 텍스트 편집의 실행 취소. Explorer 파일 이력과 별개다.
    Undo,
    Redo,
}

impl WebViewEditAction {
    pub const ALL: [WebViewEditAction; 6] = [
        Self::Copy,
        Self::Cut,
        Self::Paste,
        Self::SelectAll,
        Self::Undo,
        Self::Redo,
    ];

    /// 같은 동작의 `KeybindingSettings` 필드 ID.
    pub fn field_id(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::SelectAll => "select_all",
            Self::Undo => "page_undo",
            Self::Redo => "page_redo",
        }
    }

    pub fn from_field_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.field_id() == id)
    }

    /// 바인딩에 있어도 Tasty가 실행하지 않고 엔진과 페이지 keydown에 그대로 두는 키. Tasty 단축키가 먼저 받는다는
    /// 원칙의 예외이며 Windows의 실행 취소·다시 실행 엔진 키(Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y)만 해당한다. 웹 앱이
    /// 실제 키 keydown으로 자기 실행 취소를 처리하게 둔다. 복사·잘라내기·붙여넣기·전체 선택은 엔진 키여도
    /// Tasty가 실행하고 키를 넘기지 않는다. Linux WebKitGTK는 이 임베딩에서 Ctrl+Z 계열을 스스로 실행하지
    /// 않으므로 Tasty가 받는다. macOS WKWebView는 Edit 메뉴 없이 Cmd 편집 키를 실행하지 않으므로 대상이 없다.
    fn is_engine_default(self, key: &Key, mods: ModifiersState) -> bool {
        cfg!(windows) && matches!(self, Self::Undo | Self::Redo) && self.is_engine_key(key, mods)
    }

    /// 엔진이 바인딩과 관계없이 스스로 편집으로 처리할 수 있는 키. 입력을 막은 동안에는 바인딩에
    /// 없어도 페이지로 보내지 않는다. macOS는 Edit 메뉴가 없어 대상이 없다.
    fn is_engine_key(self, key: &Key, mods: ModifiersState) -> bool {
        if cfg!(target_os = "macos") {
            return false;
        }
        let ctrl_only = mods == ModifiersState::CONTROL;
        let shift_only = mods == ModifiersState::SHIFT;
        let ch = |c: &str| matches!(key, Key::Character(k) if k.eq_ignore_ascii_case(c));
        let named = |n: winit::keyboard::NamedKey| matches!(key, Key::Named(k) if *k == n);
        use winit::keyboard::NamedKey::{Delete, Insert};
        match self {
            Self::Copy => ctrl_only && (ch("c") || named(Insert)),
            Self::Cut => (ctrl_only && ch("x")) || (shift_only && named(Delete)),
            Self::Paste => (ctrl_only && ch("v")) || (shift_only && named(Insert)),
            Self::SelectAll => ctrl_only && ch("a"),
            Self::Undo => ctrl_only && ch("z"),
            Self::Redo => {
                (mods == ModifiersState::CONTROL | ModifiersState::SHIFT && ch("z"))
                    || (ctrl_only && ch("y"))
            }
        }
    }
}

/// 편집 액션 바인딩과 별개로 엔진 키 표가 붙인 편집 키. 입력을 막은 동안에만 쓰며, 그 밖의 처리는
/// 엔진에 그대로 둔다. Chromium(WebView2)은 Ctrl+Shift+V 서식 없이 붙여넣기와 Windows의 Alt+Backspace
/// 실행 취소·Alt+Shift+Backspace 다시 실행을, WebKitGTK는 Ctrl+Shift+V 일반 텍스트 붙여넣기와 Ctrl+/ 전체 선택을 둔다.
fn is_engine_extra_edit_key(key: &Key, mods: ModifiersState) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let ch = |c: &str| matches!(key, Key::Character(k) if k.eq_ignore_ascii_case(c));
    let backspace = matches!(key, Key::Named(winit::keyboard::NamedKey::Backspace));
    let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;
    let alt_shift = ModifiersState::ALT | ModifiersState::SHIFT;
    (mods == ctrl_shift && ch("v"))
        || (cfg!(windows) && backspace && (mods == ModifiersState::ALT || mods == alt_shift))
        || (cfg!(target_os = "linux") && mods == ModifiersState::CONTROL && ch("/"))
}

#[derive(Debug, Clone)]
pub struct WebViewKeyEvent {
    /// 창 bridge 안에서 가져간 순서대로 늘어나는 일련번호. 처리 결과를 키를 가져간 WebView의 사본에 짝짓는다.
    pub id: u64,
    /// 입력을 받은 surface. 큐를 처리할 때 대상이 남아 있는지 확인하는 데 쓴다.
    pub surface_id: u32,
    /// 물리 위치 폴백을 적용한 조회 키.
    pub key: Key,
    pub mods: ModifiersState,
}

fn shortcut_lookup_key(key: Key, physical: &PhysicalKey, mods: ModifiersState) -> Key {
    tasty_key_match::shortcut_lookup_key(&key, physical, mods)
}

#[derive(Debug, Clone, Default)]
pub struct ShortcutSources {
    /// 페이지 예약 액션을 호출자가 미리 제외한 host 조합.
    pub host: Vec<String>,
    /// 같은 의미의 plugin 조합을 제외할 페이지 예약 목록.
    pub page_reserved: Vec<String>,
    /// 매니페스트와 사용자 설정을 합친 plugin Global 명령 조합. plugin surface가 아닌 WebView에서 실행되는 범위다.
    pub plugin: Vec<String>,
    /// plugin별 모든 명령(Global·Surface) 조합. 그 plugin의 surface가 키를 받으면 이 조합만 실행 범위다
    /// (다른 plugin의 명령은 실행하지 않는다, `dispatch_plugin_shortcut_key`).
    pub plugin_owned: Vec<(String, Vec<String>)>,
    /// 주소 띠가 있는 html 페이지에서만 host가 가져갈 조합(주소창 포커스).
    pub address: Vec<String>,
    /// 공통 새로고침 조합. 페이지 예약 조합과 같으면 가져가지 않는다.
    pub refresh: Vec<String>,
    /// 편집 액션별 사용자 조합. 엔진 기본 편집 키가 아닌 조합만 native 명령으로 실행한다.
    pub edit: Vec<(WebViewEditAction, Vec<String>)>,
}

/// 설정에서 만든 host 키 조합의 사본. 호출자가 변경 시 교체해야 한다.
#[derive(Debug, Clone, Default)]
pub struct HostShortcutPolicy {
    /// host·새로고침·plugin Global 조합. plugin surface가 아닌 WebView의 판정이다.
    combos: Vec<String>,
    /// host·새로고침 조합. plugin surface의 판정은 이것과 그 plugin의 조합을 쓴다.
    host_only: Vec<String>,
    plugin_owned: std::collections::HashMap<String, Vec<String>>,
    address: Vec<String>,
    edit: Vec<(WebViewEditAction, Vec<String>)>,
}

impl HostShortcutPolicy {
    /// 같은 문자열은 중복 제거한다. 새로고침·plugin만 예약 조합과 의미를 비교한다.
    pub fn new(sources: ShortcutSources) -> Self {
        let ShortcutSources {
            host,
            page_reserved,
            plugin,
            plugin_owned,
            address,
            refresh,
            edit,
        } = sources;
        // 수식키 없는 조합도 담는다. 웹 화면에서 동작할 수 없는 동작은 호출자가 미리 뺀다.
        fn push(combos: &mut Vec<String>, combo: String) {
            if !combo.is_empty() && !combos.contains(&combo) {
                combos.push(combo);
            }
        }
        // 표기 대소문자·modifier 순서가 달라도 같은 단축키이면 페이지 예약 조합으로 본다.
        let reserved = |combo: &str| {
            page_reserved
                .iter()
                .any(|r| tasty_key_match::bindings_equivalent(r, combo))
        };

        let mut host_only: Vec<String> = Vec::new();
        for combo in host {
            push(&mut host_only, combo);
        }
        // 페이지 예약 편집 조합이 먼저다. 같은 조합의 새로고침은 페이지에 남긴다.
        for combo in refresh {
            if !reserved(&combo) {
                push(&mut host_only, combo);
            }
        }

        let mut combos = host_only.clone();
        for combo in plugin {
            if !reserved(&combo) {
                push(&mut combos, combo);
            }
        }
        let plugin_owned = plugin_owned
            .into_iter()
            .map(|(plugin_id, owned)| {
                let mut kept = Vec::new();
                for combo in owned {
                    if !reserved(&combo) {
                        push(&mut kept, combo);
                    }
                }
                (plugin_id, kept)
            })
            .collect();

        Self {
            combos,
            host_only,
            plugin_owned,
            address,
            edit,
        }
    }

    #[cfg(test)]
    pub(crate) fn combos(&self) -> &[String] {
        &self.combos
    }

    /// plugin surface가 아닌 WebView에서 host가 가져갈 조합인지.
    #[cfg(test)]
    pub fn claims(&self, key: &Key, mods: ModifiersState) -> bool {
        self.claims_on(key, mods, None)
    }

    /// `owner`는 키를 받은 surface를 소유한 plugin이다. 그 plugin의 surface에서는 host 조합과 그 plugin의
    /// 명령만 실행되므로 다른 plugin의 Global 조합은 가져가지 않는다.
    pub fn claims_on(&self, key: &Key, mods: ModifiersState, owner: Option<&str>) -> bool {
        match owner {
            None => tasty_key_match::matches_any_binding(&self.combos, key, mods),
            Some(plugin_id) => {
                tasty_key_match::matches_any_binding(&self.host_only, key, mods)
                    || self
                        .plugin_owned
                        .get(plugin_id)
                        .is_some_and(|owned| tasty_key_match::matches_any_binding(owned, key, mods))
            }
        }
    }

    /// 엔진 기본 키까지 포함해 편집 바인딩에 걸리는지. 입력을 막은 동안 페이지로 보내지 않을 키다.
    pub fn matches_edit_binding(&self, key: &Key, mods: ModifiersState) -> bool {
        self.edit
            .iter()
            .any(|(_, combos)| tasty_key_match::matches_any_binding(combos, key, mods))
    }

    /// native 명령으로 실행할 편집 액션. 엔진이 같은 편집으로 처리하는 키는 None이다.
    pub fn edit_action(&self, key: &Key, mods: ModifiersState) -> Option<WebViewEditAction> {
        self.edit.iter().find_map(|(action, combos)| {
            (tasty_key_match::matches_any_binding(combos, key, mods)
                && !action.is_engine_default(key, mods))
            .then_some(*action)
        })
    }

    /// 주소 띠가 있는 html 페이지에서만 가져갈 조합인지.
    pub fn claims_for_address(&self, key: &Key, mods: ModifiersState) -> bool {
        tasty_key_match::matches_any_binding(&self.address, key, mods)
    }
}

/// native 콜백에서 동기 판정·포커스 관측을 전달하는 계약.
/// 구현과 callback은 같은 GUI 스레드에서 사용하며 Send/Sync를 요구하지 않는다.
pub trait WebViewKeySink {
    /// true이면 backend가 페이지 전달을 막는다. 큐 등록과 실제 액션 성공은 별개다.
    /// key는 레이아웃 문자, physical은 위치이며 알 수 없으면 Unidentified다. backend는 결과를 키에 짝짓기 위해
    /// [`Self::forward_key`]를 쓰고, 이 함수는 판정만 보는 시험에 쓴다.
    #[cfg(test)]
    fn capture_key(
        &self,
        surface_id: u32,
        key: Key,
        physical: PhysicalKey,
        mods: ModifiersState,
    ) -> bool {
        self.forward_key(surface_id, key, physical, mods).is_some()
    }

    /// [`Self::capture_key`]와 같고, 가져갔으면 host 큐 이벤트의 일련번호([`WebViewKeyEvent::id`])를 돌려준다.
    /// 처리 결과를 그 키에 짝지을 때 쓴다.
    fn forward_key(
        &self,
        surface_id: u32,
        key: Key,
        physical: PhysicalKey,
        mods: ModifiersState,
    ) -> Option<u64>;

    /// 사용자 편집 바인딩이면 그 액션을 돌려준다. backend는 키를 받은 WebView에 즉시 실행하고
    /// 페이지 전달을 막는다. 큐를 거치지 않으므로 닫히거나 교체된 WebView에는 실행되지 않는다.
    /// Windows 명령은 같은 WebView에 비동기로 전달된다. Linux는 키를 WebKit 입력 큐에 넘기고 web process 확장이
    /// 그 키를 입력 순서대로 받았다고 알린 뒤 실행한다. 그 사이 포커스·문서가 바뀌었거나 host 모달·overlay가
    /// 열렸거나 WebView를 버렸으면 실행하지 않는다. Copy·Cut은 그 시점에 범위 선택이 없으면 실행하지 않고 키를
    /// 막지 않으므로, 그 키는 엔진 기본 동작과 페이지 keydown으로 간다.
    fn edit_action(
        &self,
        key: &Key,
        physical: &PhysicalKey,
        mods: ModifiersState,
    ) -> Option<WebViewEditAction>;

    /// host 모달이나 키보드를 가져가는 overlay가 열린 동안 페이지에 보내지도 실행하지도 않을 편집 키인지.
    /// true이면 backend는 아무것도 실행하지 않고 페이지 전달을 막는다. [`Self::edit_action`]보다 먼저 묻는다.
    fn blocks_edit_key(&self, key: &Key, physical: &PhysicalKey, mods: ModifiersState) -> bool;

    /// 자동 반복 key-down을 페이지·엔진에 보내지 않을지. 첫 누름이 막혔거나(입력 차단), 편집 명령으로
    /// 실행됐거나, host가 가져갔을 조합이면 true다. 반복에서는 아무것도 실행하거나 큐에 넣지 않는다.
    /// false이면 반복을 엔진·페이지에 그대로 둔다. Linux GDK press에는 repeat 표시가 없어 쓰지 않는다.
    /// Windows·macOS backend는 그 물리 키의 첫 누름을 host 큐로 가져간 기록이 있으면 그 결과를 먼저 따르고
    /// (`key_hold`), 기록이 없을 때만 이 판정을 쓴다.
    #[cfg(any(windows, target_os = "macos", test))]
    fn consumes_repeat(
        &self,
        surface_id: u32,
        key: &Key,
        physical: &PhysicalKey,
        mods: ModifiersState,
    ) -> bool;

    /// native 클릭 또는 포커스 관측을 알린다. 실제 모델 선택 변경은 host가 판단한다.
    fn note_focus(&self, surface_id: u32);
}

/// 창의 WebView들이 Rc로 공유한다. RefCell 상태는 GUI 스레드에서만 접근해야 한다.
#[derive(Debug, Default)]
pub struct WebViewKeyBridge {
    policy: RefCell<HostShortcutPolicy>,
    /// 주소 띠가 있는 html surface. 주소창 포커스 조합은 이 페이지에서만 가져간다.
    address_surfaces: RefCell<std::collections::HashSet<u32>>,
    /// plugin이 소유한 WebView surface와 그 plugin ID. 없는 surface는 plugin surface가 아니다.
    surface_owners: RefCell<std::collections::HashMap<u32, String>>,
    pending: RefCell<Vec<WebViewKeyEvent>>,
    /// 다음에 가져갈 키의 일련번호.
    next_key_id: std::cell::Cell<u64>,
    focus_requests: RefCell<Vec<u32>>,
    /// host 모달이 열려 있다. 모달을 열고 닫을 때 바로 바꾼다.
    modal_open: std::cell::Cell<bool>,
    /// 키보드를 가져가는 host overlay(popup·dialog)가 열려 있다. 프레임 동기화에서 바꾼다.
    overlay_open: std::cell::Cell<bool>,
}

impl WebViewKeyBridge {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_policy(&self, policy: HostShortcutPolicy) {
        *self.policy.borrow_mut() = policy;
    }

    /// 주소 띠가 있는 html surface 목록을 바꾼다.
    pub fn set_address_surfaces(&self, surfaces: std::collections::HashSet<u32>) {
        *self.address_surfaces.borrow_mut() = surfaces;
    }

    /// WebView surface를 소유한 plugin 목록을 바꾼다.
    pub fn set_surface_owners(&self, owners: std::collections::HashMap<u32, String>) {
        *self.surface_owners.borrow_mut() = owners;
    }

    pub fn take_pending(&self) -> Vec<WebViewKeyEvent> {
        std::mem::take(&mut *self.pending.borrow_mut())
    }

    pub fn take_focus_requests(&self) -> Vec<u32> {
        std::mem::take(&mut *self.focus_requests.borrow_mut())
    }

    pub fn set_modal_open(&self, open: bool) {
        self.modal_open.set(open);
    }

    pub fn set_overlay_open(&self, open: bool) {
        self.overlay_open.set(open);
    }

    fn input_blocked(&self) -> bool {
        self.modal_open.get() || self.overlay_open.get()
    }

    /// host가 가져갈 조합인지만 판정한다. 큐는 바꾸지 않는다.
    fn claims(&self, surface_id: u32, key: &Key, mods: ModifiersState) -> bool {
        let policy = self.policy.borrow();
        let owners = self.surface_owners.borrow();
        policy.claims_on(key, mods, owners.get(&surface_id).map(String::as_str))
            || (self.address_surfaces.borrow().contains(&surface_id)
                && policy.claims_for_address(key, mods))
    }
}

/// [`route_key_down`]의 판정.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyDownRoute {
    /// 페이지·엔진에 그대로 보낸다.
    Pass,
    /// 입력 차단·편집 명령·반복 판정으로 소비했다. 결과를 기다리지 않는다.
    Consumed,
    /// host 큐에 일련번호로 넣었다. backend는 키를 소비하고 원래 이벤트를 그 번호로 붙잡는다.
    Forwarded(u64),
}

#[cfg(any(windows, test))]
impl KeyDownRoute {
    /// backend가 페이지 전달을 막을지.
    pub(crate) fn consumed(self) -> bool {
        self != Self::Pass
    }
}

/// key-down 하나를 사용자 바인딩으로 판정한다. 소비했으면 backend는 페이지·엔진에 보내지 않는다.
/// 자동 반복은 다시 실행하지 않는다. 첫 누름을 가져간 기록이 있는 반복은 backend가 그 기록을 먼저 따르고, 이 함수는
/// 기록이 없을 때의 판정이다. 편집 명령은 `run_edit`로 그 WebView에 실행한다.
/// 소비하지 않을 때는 sink 상태를 바꾸지 않으므로, 같은 키를 두 경로가 차례로 판정해도 한 번만 실행된다.
/// Windows의 메시지 훅과 `AcceleratorKeyPressed`가 이 순서를 함께 쓴다.
#[cfg(any(windows, test))]
pub(crate) fn route_key_down(
    sink: &dyn WebViewKeySink,
    surface_id: u32,
    key: Key,
    physical: PhysicalKey,
    mods: ModifiersState,
    repeat: bool,
    run_edit: impl FnOnce(WebViewEditAction),
) -> KeyDownRoute {
    if repeat {
        return if sink.consumes_repeat(surface_id, &key, &physical, mods) {
            KeyDownRoute::Consumed
        } else {
            KeyDownRoute::Pass
        };
    }
    if sink.blocks_edit_key(&key, &physical, mods) {
        return KeyDownRoute::Consumed;
    }
    if let Some(action) = sink.edit_action(&key, &physical, mods) {
        run_edit(action);
        return KeyDownRoute::Consumed;
    }
    match sink.forward_key(surface_id, key, physical, mods) {
        Some(id) => KeyDownRoute::Forwarded(id),
        None => KeyDownRoute::Pass,
    }
}

impl WebViewKeySink for WebViewKeyBridge {
    /// 정책이 선택한 키만 큐에 넣는다. 실행 시점이나 최종 동작 성공을 여기서 보장하지 않는다.
    fn forward_key(
        &self,
        surface_id: u32,
        key: Key,
        physical: PhysicalKey,
        mods: ModifiersState,
    ) -> Option<u64> {
        let key = shortcut_lookup_key(key, &physical, mods);
        if !self.claims(surface_id, &key, mods) {
            return None;
        }
        let id = self.next_key_id.get();
        self.next_key_id.set(id.wrapping_add(1));
        self.pending.borrow_mut().push(WebViewKeyEvent {
            id,
            surface_id,
            key,
            mods,
        });
        Some(id)
    }

    fn edit_action(
        &self,
        key: &Key,
        physical: &PhysicalKey,
        mods: ModifiersState,
    ) -> Option<WebViewEditAction> {
        if self.input_blocked() {
            return None;
        }
        let key = shortcut_lookup_key(key.clone(), physical, mods);
        self.policy.borrow().edit_action(&key, mods)
    }

    fn blocks_edit_key(&self, key: &Key, physical: &PhysicalKey, mods: ModifiersState) -> bool {
        if !self.input_blocked() {
            return false;
        }
        let key = shortcut_lookup_key(key.clone(), physical, mods);
        self.policy.borrow().matches_edit_binding(&key, mods)
            || WebViewEditAction::ALL
                .into_iter()
                .any(|a| a.is_engine_key(&key, mods))
            || is_engine_extra_edit_key(&key, mods)
    }

    #[cfg(any(windows, target_os = "macos", test))]
    fn consumes_repeat(
        &self,
        surface_id: u32,
        key: &Key,
        physical: &PhysicalKey,
        mods: ModifiersState,
    ) -> bool {
        if self.blocks_edit_key(key, physical, mods)
            || self.edit_action(key, physical, mods).is_some()
        {
            return true;
        }
        let key = shortcut_lookup_key(key.clone(), physical, mods);
        self.claims(surface_id, &key, mods)
    }

    fn note_focus(&self, surface_id: u32) {
        let mut q = self.focus_requests.borrow_mut();
        if q.last() != Some(&surface_id) {
            q.push(surface_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{KeyCode, NamedKey, NativeKeyCode};

    fn ch(c: &str) -> Key {
        Key::Character(c.into())
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn policy(host: &[&str], page_reserved: &[&str], plugin: &[&str]) -> HostShortcutPolicy {
        HostShortcutPolicy::new(ShortcutSources {
            host: strings(host),
            page_reserved: strings(page_reserved),
            plugin: strings(plugin),
            plugin_owned: Vec::new(),
            address: Vec::new(),
            refresh: Vec::new(),
            edit: Vec::new(),
        })
    }

    fn unknown_physical() -> PhysicalKey {
        PhysicalKey::Unidentified(NativeKeyCode::Unidentified)
    }

    fn reorder_modifiers(combo: &str) -> String {
        let parts: Vec<&str> = combo.split('+').collect();
        if parts.len() < 3 {
            return combo.to_string();
        }
        let key = parts[parts.len() - 1];
        let mut mods: Vec<&str> = parts[..parts.len() - 1].to_vec();
        mods.reverse();
        format!("{}+{}", mods.join("+"), key)
    }

    fn alt_mod() -> ModifiersState {
        if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ModifiersState::ALT
        }
    }

    /// 공통 정책·큐 동작을 확인한다. OS의 실제 포커스 전달까지 검사하지는 않는다.
    #[test]
    fn capture_key_consumes_host_shortcut_and_passes_the_rest() {
        let bridge = WebViewKeyBridge::new();
        bridge.set_policy(policy(&["alt+d"], &[], &[]));

        assert!(bridge.capture_key(7, ch("d"), unknown_physical(), alt_mod()));

        assert!(!bridge.capture_key(7, ch("d"), unknown_physical(), ModifiersState::empty()));
        assert!(!bridge.capture_key(7, ch("D"), unknown_physical(), ModifiersState::SHIFT));
        assert!(!bridge.capture_key(7, ch("e"), unknown_physical(), alt_mod()));
        assert!(!bridge.capture_key(
            7,
            Key::Named(NamedKey::Escape),
            unknown_physical(),
            ModifiersState::empty()
        ));

        let pending = bridge.take_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].surface_id, 7);
        assert!(bridge.take_pending().is_empty());
    }

    #[test]
    fn a_bridge_without_a_policy_claims_nothing() {
        let bridge = WebViewKeyBridge::new();
        assert!(!bridge.capture_key(1, ch("d"), unknown_physical(), alt_mod()));
        assert!(bridge.take_pending().is_empty());
    }

    #[test]
    fn set_policy_replaces_the_previous_policy() {
        let bridge = WebViewKeyBridge::new();
        bridge.set_policy(policy(&["alt+d"], &[], &[]));
        bridge.set_policy(policy(&["alt+e"], &[], &[]));
        assert!(!bridge.capture_key(1, ch("d"), unknown_physical(), alt_mod()));
        assert!(bridge.capture_key(1, ch("e"), unknown_physical(), alt_mod()));
    }

    /// 수식키 유무와 관계없이 바인딩을 가져가고(Tasty 단축키 우선) 같은 문자열은 하나만 둔다.
    #[test]
    fn host_combos_keep_modifierless_bindings_and_are_deduplicated() {
        let p = policy(
            &["alt+d", "f2", "shift+g", "alt+d", "ctrl+shift+k"],
            &[],
            &[],
        );
        assert_eq!(p.combos(), ["alt+d", "f2", "shift+g", "ctrl+shift+k"]);
    }

    #[test]
    fn plugin_command_bindings_join_the_policy() {
        let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(
            !policy(&[], &[], &[]).claims(&ch("h"), mods),
            "sanity: 빈 정책은 ctrl+shift+h 를 안 가져간다"
        );

        let p = policy(&[], &[], &["ctrl+shift+h", "f"]);
        assert!(
            p.claims(&ch("h"), mods),
            "plugin 콤보가 정책에 없다: {:?}",
            p.combos()
        );
        assert!(p.claims(&ch("f"), ModifiersState::empty()));
    }

    #[test]
    fn plugin_binding_cannot_take_a_page_reserved_combo() {
        let reserved = ["ctrl+f", "ctrl+alt+f"];
        let mut variants: Vec<String> = Vec::new();
        for combo in reserved {
            variants.push(combo.to_string());
            variants.push(combo.to_ascii_uppercase());
            variants.push(reorder_modifiers(combo));
        }
        // 표기 변형이 원문과 달라야 문자열 비교와 의미 비교를 구분할 수 있다.
        assert!(
            variants.iter().any(|v| !reserved.contains(&v.as_str())),
            "sanity: 표기 변형이 원본과 다른 문자열이어야 한다: {variants:?}"
        );

        let p = HostShortcutPolicy::new(ShortcutSources {
            host: Vec::new(),
            page_reserved: strings(&reserved),
            plugin: variants.clone(),
            plugin_owned: Vec::new(),
            address: Vec::new(),
            refresh: Vec::new(),
            edit: Vec::new(),
        });
        assert!(
            p.combos().is_empty(),
            "page-reserved combo (표기 변형 포함) must stay with the page: {:?}",
            p.combos()
        );

        let p = HostShortcutPolicy::new(ShortcutSources {
            host: Vec::new(),
            page_reserved: strings(&reserved),
            plugin: strings(&["ctrl+shift+h"]),
            plugin_owned: Vec::new(),
            address: Vec::new(),
            refresh: Vec::new(),
            edit: Vec::new(),
        });
        assert_eq!(p.combos(), ["ctrl+shift+h"]);
    }

    /// host 목록은 호출자가 예약 액션을 제외하므로 여기서 예약 필터를 다시 적용하지 않는다.
    #[test]
    fn the_reserved_filter_applies_to_plugin_combos_only() {
        let p = policy(&["ctrl+f"], &["ctrl+f"], &[]);
        assert_eq!(p.combos(), ["ctrl+f"]);
    }

    #[test]
    fn non_latin_layout_matches_through_the_physical_fallback() {
        let bridge = WebViewKeyBridge::new();
        bridge.set_policy(policy(&[], &[], &["ctrl+shift+h"]));

        let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(!bridge.capture_key(3, ch("Р"), unknown_physical(), mods));
        assert!(bridge.capture_key(3, ch("Р"), PhysicalKey::Code(KeyCode::KeyH), mods));

        let pending = bridge.take_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].key, ch("h"));
    }

    #[test]
    fn physical_fallback_only_applies_with_modifiers() {
        let key = shortcut_lookup_key(
            ch("Р"),
            &PhysicalKey::Code(KeyCode::KeyH),
            ModifiersState::SHIFT,
        );
        assert_eq!(key, ch("Р"));
        let key = shortcut_lookup_key(
            ch("Р"),
            &PhysicalKey::Code(KeyCode::KeyH),
            ModifiersState::CONTROL,
        );
        assert_eq!(key, ch("h"));
    }

    fn edit_policy(edit: &[(WebViewEditAction, &[&str])]) -> HostShortcutPolicy {
        HostShortcutPolicy::new(ShortcutSources {
            edit: edit.iter().map(|(a, c)| (*a, strings(c))).collect(),
            ..Default::default()
        })
    }

    /// 복사·잘라내기·붙여넣기·전체 선택 바인딩은 엔진 기본 키여도 Tasty 명령이 된다. Windows의 실행 취소·다시 실행
    /// 엔진 키만 엔진에 남긴다.
    #[test]
    fn bound_edit_keys_become_native_commands_except_windows_undo_keys() {
        let p = edit_policy(&[
            (
                WebViewEditAction::Copy,
                &["ctrl+c", "alt+c", "ctrl+shift+c", "ctrl+insert"],
            ),
            (WebViewEditAction::Cut, &["ctrl+x", "shift+delete"]),
            (WebViewEditAction::Paste, &["ctrl+v", "shift+insert"]),
            (WebViewEditAction::SelectAll, &["ctrl+a", "ctrl+alt+a"]),
            (WebViewEditAction::Undo, &["ctrl+z", "alt+u"]),
            (WebViewEditAction::Redo, &["ctrl+shift+z", "ctrl+y"]),
        ]);
        let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let ctrl = ModifiersState::CONTROL;
        let shift = ModifiersState::SHIFT;
        let insert = Key::Named(NamedKey::Insert);
        let delete = Key::Named(NamedKey::Delete);
        let engine = |key: &str, mods| p.edit_action(&ch(key), mods);
        assert_eq!(engine("c", ctrl_shift), Some(WebViewEditAction::Copy));
        assert_eq!(engine("c", alt_mod()), Some(WebViewEditAction::Copy));
        assert_eq!(
            engine("a", ModifiersState::CONTROL | alt_mod()),
            Some(WebViewEditAction::SelectAll)
        );
        // 엔진도 처리하는 키지만 Tasty 동작이 있으므로 Tasty가 실행한다.
        assert_eq!(engine("c", ctrl), Some(WebViewEditAction::Copy));
        assert_eq!(p.edit_action(&insert, ctrl), Some(WebViewEditAction::Copy));
        assert_eq!(engine("x", ctrl), Some(WebViewEditAction::Cut));
        assert_eq!(p.edit_action(&delete, shift), Some(WebViewEditAction::Cut));
        assert_eq!(engine("v", ctrl), Some(WebViewEditAction::Paste));
        assert_eq!(
            p.edit_action(&insert, shift),
            Some(WebViewEditAction::Paste)
        );
        assert_eq!(engine("a", ctrl), Some(WebViewEditAction::SelectAll));
        if cfg!(windows) {
            assert_eq!(
                engine("z", ctrl),
                None,
                "WebView2 는 Ctrl+Z 를 실행 취소하고 웹 앱이 그 keydown 을 받는다"
            );
            assert_eq!(engine("z", ctrl_shift), None);
            assert_eq!(
                engine("y", ctrl),
                None,
                "WebView2 는 Ctrl+Y 를 다시 실행한다"
            );
        } else {
            // WebKitGTK 임베딩과 WKWebView 는 Ctrl/Cmd+Z 를 스스로 실행하지 않는다.
            assert_eq!(engine("z", ctrl), Some(WebViewEditAction::Undo));
            assert_eq!(engine("z", ctrl_shift), Some(WebViewEditAction::Redo));
            assert_eq!(engine("y", ctrl), Some(WebViewEditAction::Redo));
        }
        assert_eq!(engine("u", alt_mod()), Some(WebViewEditAction::Undo));
        assert_eq!(engine("v", alt_mod()), None);
    }

    /// 빈 목록은 편집 명령을 만들지 않고 키는 페이지·host 판정으로 간다.
    #[test]
    fn an_empty_edit_binding_produces_no_command() {
        let p = edit_policy(&[(WebViewEditAction::Paste, &[])]);
        assert_eq!(p.edit_action(&ch("v"), alt_mod()), None);
        let bridge = WebViewKeyBridge::new();
        bridge.set_policy(edit_policy(&[(WebViewEditAction::Cut, &["alt+x"])]));
        assert_eq!(
            bridge.edit_action(&ch("x"), &unknown_physical(), alt_mod()),
            Some(WebViewEditAction::Cut)
        );
        assert!(
            bridge.take_pending().is_empty(),
            "편집 키는 host 큐에 넣지 않는다"
        );
        bridge.set_policy(edit_policy(&[(WebViewEditAction::Cut, &[])]));
        assert_eq!(
            bridge.edit_action(&ch("x"), &unknown_physical(), alt_mod()),
            None,
            "바인딩 변경이 즉시 반영된다"
        );
    }

    /// 모달이나 overlay가 열린 동안 편집 키는 엔진 기본 키까지 실행·전달 없이 소비되고, 닫히면 돌아온다.
    #[test]
    fn edit_keys_are_swallowed_while_host_input_is_blocked() {
        let bridge = WebViewKeyBridge::new();
        bridge.set_policy(edit_policy(&[(
            WebViewEditAction::Paste,
            &["ctrl+v", "alt+v"],
        )]));
        let phys = unknown_physical();
        let ctrl = ModifiersState::CONTROL;
        assert!(!bridge.blocks_edit_key(&ch("v"), &phys, alt_mod()));
        assert_eq!(
            bridge.edit_action(&ch("v"), &phys, alt_mod()),
            Some(WebViewEditAction::Paste)
        );
        for (modal, overlay) in [(true, false), (false, true)] {
            bridge.set_modal_open(modal);
            bridge.set_overlay_open(overlay);
            assert!(bridge.blocks_edit_key(&ch("v"), &phys, alt_mod()));
            assert!(
                bridge.blocks_edit_key(&ch("v"), &phys, ctrl),
                "엔진 기본 키도 막는다"
            );
            assert_eq!(bridge.edit_action(&ch("v"), &phys, alt_mod()), None);
            assert!(
                !bridge.blocks_edit_key(&ch("q"), &phys, alt_mod()),
                "편집이 아닌 키는 기존 판정으로 간다"
            );
            let extra: &[(Key, ModifiersState, bool)] = &[
                (
                    ch("v"),
                    ModifiersState::CONTROL | ModifiersState::SHIFT,
                    !cfg!(target_os = "macos"),
                ),
                (
                    Key::Named(winit::keyboard::NamedKey::Backspace),
                    ModifiersState::ALT,
                    cfg!(windows),
                ),
                (
                    Key::Named(winit::keyboard::NamedKey::Backspace),
                    ModifiersState::ALT | ModifiersState::SHIFT,
                    cfg!(windows),
                ),
                (ch("/"), ModifiersState::CONTROL, cfg!(target_os = "linux")),
            ];
            for (key, mods, held) in extra {
                assert_eq!(
                    bridge.blocks_edit_key(key, &phys, *mods),
                    *held,
                    "엔진 키 표의 편집 키 {key:?} {mods:?}"
                );
            }
            let shift_insert = Key::Named(winit::keyboard::NamedKey::Insert);
            assert_eq!(
                bridge.blocks_edit_key(&shift_insert, &phys, ModifiersState::SHIFT),
                !cfg!(target_os = "macos"),
                "바인딩에 없는 엔진 편집 키도 막는다"
            );
        }
        bridge.set_modal_open(false);
        bridge.set_overlay_open(false);
        assert!(!bridge.blocks_edit_key(&ch("v"), &phys, ctrl));
        let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(
            !bridge.blocks_edit_key(&ch("v"), &phys, ctrl_shift),
            "막지 않는 동안에는 바인딩에 없는 엔진 키를 엔진에 그대로 둔다"
        );
        assert_eq!(bridge.edit_action(&ch("v"), &phys, ctrl_shift), None);
        assert_eq!(
            bridge.edit_action(&ch("v"), &phys, alt_mod()),
            Some(WebViewEditAction::Paste)
        );
    }

    /// 반복은 첫 누름과 같은 판정으로 막되 아무것도 실행하거나 큐에 넣지 않는다.
    /// 엔진에 그대로 두는 키와 가져가지 않는 키의 반복은 엔진·페이지로 간다.
    #[test]
    fn repeats_follow_the_first_press_decision_without_running_anything() {
        let bridge = WebViewKeyBridge::new();
        let mut policy_sources = ShortcutSources {
            host: vec!["ctrl+r".into()],
            ..Default::default()
        };
        policy_sources.edit = vec![
            (
                WebViewEditAction::Copy,
                vec!["alt+c".into(), "ctrl+c".into()],
            ),
            (WebViewEditAction::Paste, vec!["ctrl+v".into()]),
        ];
        bridge.set_policy(HostShortcutPolicy::new(policy_sources));
        let phys = unknown_physical();
        let ctrl = ModifiersState::CONTROL;
        assert!(
            bridge.consumes_repeat(1, &ch("r"), &phys, ctrl),
            "host 조합 반복"
        );
        assert!(
            bridge.consumes_repeat(1, &ch("c"), &phys, alt_mod()),
            "편집 바인딩 반복"
        );
        assert!(
            bridge.take_pending().is_empty(),
            "반복은 host 큐에 넣지 않는다"
        );
        assert!(!bridge.consumes_repeat(1, &ch("q"), &phys, alt_mod()));
        // 바인딩된 Ctrl+V·Ctrl+C는 Tasty가 실행하므로 반복도 막는다.
        assert!(bridge.consumes_repeat(1, &ch("v"), &phys, ctrl));
        assert!(bridge.consumes_repeat(1, &ch("c"), &phys, ctrl));
        // 바인딩에 없는 엔진 키의 반복은 엔진으로 간다.
        assert!(!bridge.consumes_repeat(1, &ch("z"), &phys, ctrl));
        bridge.set_modal_open(true);
        assert_eq!(
            bridge.consumes_repeat(1, &ch("z"), &phys, ctrl),
            !cfg!(target_os = "macos"),
            "입력을 막은 동안에는 바인딩에 없는 엔진 키 반복도 막는다"
        );
        assert!(!bridge.consumes_repeat(1, &ch("q"), &phys, alt_mod()));
    }

    #[test]
    fn focus_requests_dedupe_consecutive() {
        let bridge = WebViewKeyBridge::new();
        bridge.note_focus(3);
        bridge.note_focus(3);
        bridge.note_focus(5);
        assert_eq!(bridge.take_focus_requests(), vec![3, 5]);
        assert!(bridge.take_focus_requests().is_empty());
    }

    /// 새로고침은 수식키와 관계없이 가져가고, 페이지 예약 조합과 같으면 페이지에 남긴다.
    #[test]
    fn refresh_claims_function_keys_but_yields_to_page_reserved_combos() {
        let p = HostShortcutPolicy::new(ShortcutSources {
            host: Vec::new(),
            page_reserved: strings(&["ctrl+c", "f3"]),
            plugin: Vec::new(),
            plugin_owned: Vec::new(),
            address: Vec::new(),
            refresh: strings(&["f5", "r", "CTRL+C", "F3", "ctrl+alt+r"]),
            edit: Vec::new(),
        });
        assert_eq!(p.combos(), ["f5", "r", "ctrl+alt+r"]);
    }

    /// Windows 메시지 훅이 소비한 key-down은 Dispatch되지 않아 accelerator가 다시 오지 않는다.
    /// 훅이 통과시킨 키는 accelerator가 한 번 더 판정해도 실행·큐 등록이 생기지 않는다.
    /// 훅을 지나지 않은 키(중첩 메시지 루프)는 accelerator 판정 하나로 같은 결과를 낸다.
    #[test]
    fn judging_a_key_down_twice_never_runs_a_binding_twice() {
        let bridge = WebViewKeyBridge::new();
        bridge.set_policy(HostShortcutPolicy::new(ShortcutSources {
            host: strings(&["ctrl+j"]),
            edit: vec![(WebViewEditAction::SelectAll, strings(&["q"]))],
            ..Default::default()
        }));
        let phys = unknown_physical();
        let ctrl = ModifiersState::CONTROL;
        let mut edits = Vec::new();
        let judge = |key: &str, mods: ModifiersState, repeat: bool, edits: &mut Vec<_>| {
            route_key_down(&bridge, 7, ch(key), phys, mods, repeat, |a| edits.push(a)).consumed()
        };

        // 훅 경로: 소비하면 끝이다(accelerator가 오지 않는다).
        assert!(judge("j", ctrl, false, &mut edits));
        assert!(judge("q", ModifiersState::empty(), false, &mut edits));
        assert_eq!(edits, vec![WebViewEditAction::SelectAll]);
        assert_eq!(bridge.take_pending().len(), 1);

        // 훅이 통과시킨 키를 accelerator가 다시 판정해도 아무것도 생기지 않는다.
        for _ in 0..2 {
            assert!(!judge("k", ctrl, false, &mut edits));
            assert!(!judge("z", ModifiersState::empty(), false, &mut edits));
        }
        assert_eq!(edits.len(), 1);
        assert!(bridge.take_pending().is_empty());

        // 중첩 루프: 훅 없이 accelerator 판정만 와도 한 번 실행된다. 반복은 다시 실행하지 않는다.
        assert!(judge("j", ctrl, false, &mut edits));
        assert!(judge("j", ctrl, true, &mut edits));
        assert_eq!(bridge.take_pending().len(), 1);
    }
}
