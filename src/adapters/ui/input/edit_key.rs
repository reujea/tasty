//! 복사·잘라내기·붙여넣기 키의 공통 판정. 모든 입력 대상이 이 판정을 다음 처리자(egui-winit, 터미널,
//! 페이지)보다 먼저 한 번 한다. 바인딩한 키는 실제로 동작했을 때만 소비하고, 동작하지 못했으면 원래 키를
//! 그대로 다음 처리자에 넘긴다. 키 시점에 알 수 없으면 원래 키를 붙잡아 두고 결과를 본 뒤 정한다.
//! 계약: `docs/design/policies/key-mapping.md`의 "편집 키 판정" 절.

use std::time::Instant;

use tasty_key_match::matches_any_binding;
use winit::keyboard::{Key, ModifiersState};

/// 공통 판정이 다루는 편집 동작.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKey {
    Copy,
    Cut,
    Paste,
}

impl EditKey {
    /// 한 키가 여러 편집 바인딩에 걸리면 이 순서로 하나만 고른다.
    pub(crate) const ORDER: [EditKey; 3] = [Self::Copy, Self::Cut, Self::Paste];

    /// 같은 동작의 `KeybindingSettings` 필드 ID.
    pub(crate) fn field_id(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
        }
    }
}

/// 판정에 쓰는 편집 바인딩 사본. 창마다 적용된 설정에서 만든다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EditBindings {
    copy: Vec<String>,
    cut: Vec<String>,
    paste: Vec<String>,
    /// 판정할 키가 이미 편집으로 판정된 키인지(편집 보류 뒤 재생하는 원래 키). 그 키는 지금 바인딩과 무관하게 편집
    /// 키로 본다.
    judged: bool,
}

impl EditBindings {
    pub(crate) fn from_settings(kb: &crate::settings::KeybindingSettings) -> Self {
        Self {
            copy: kb.copy.clone(),
            cut: kb.cut.clone(),
            paste: kb.paste.clone(),
            judged: false,
        }
    }

    /// 재생하는 원래 키의 판정. `judged` 면 이 키는 보류 때 판정한 편집 키다(재생 사이 설정이 바뀌어도 그 판정을 쓴다).
    pub(crate) fn with_judged(mut self, judged: bool) -> Self {
        self.judged = judged;
        self
    }

    /// 판정할 키가 이미 편집으로 판정된 키인지.
    pub(crate) fn judged(&self) -> bool {
        self.judged
    }

    /// 한 편집 동작의 바인딩 목록.
    pub(crate) fn combos(&self, edit: EditKey) -> &[String] {
        match edit {
            EditKey::Copy => &self.copy,
            EditKey::Cut => &self.cut,
            EditKey::Paste => &self.paste,
        }
    }

    /// 키가 가리키는 편집 동작 하나. `key`는 `shortcut_lookup_key`로 고른 단축키 판정 키다.
    pub(crate) fn binding(&self, key: &Key, mods: ModifiersState) -> Option<EditKey> {
        first_bound(|edit| self.combos(edit), key, mods)
    }
}

/// `KeybindingSettings`에서 키가 가리키는 편집 동작 하나. [`EditBindings::binding`]과 같은 순서다.
pub(crate) fn edit_binding(
    kb: &crate::settings::KeybindingSettings,
    key: &Key,
    mods: ModifiersState,
) -> Option<EditKey> {
    first_bound(
        |edit| kb.get_bindings(edit.field_id()).unwrap_or(&[]),
        key,
        mods,
    )
}

fn first_bound<'a>(
    combos: impl Fn(EditKey) -> &'a [String],
    key: &Key,
    mods: ModifiersState,
) -> Option<EditKey> {
    EditKey::ORDER
        .into_iter()
        .find(|edit| matches_any_binding(combos(*edit), key, mods))
}

/// 키 시점에 대상이 그 편집을 할 수 있는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    /// 할 수 있음을 안다.
    Yes,
    /// 할 수 없음을 안다(선택 없음, 빈 클립보드, 입력칸 없음).
    No,
    /// 결과를 봐야 안다.
    Unknown,
}

/// 편집 키 하나의 판정.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// 편집 바인딩이 아니다. 키는 원래 경로로 간다.
    NoBinding,
    /// 지금 동작할 수 있다. 실행하고 키를 소비한다.
    Act(EditKey),
    /// 결과를 봐야 안다. 원래 키를 [`EditKeyHold`]에 붙잡고 편집을 시도한다.
    Hold(EditKey),
    /// 지금 동작할 수 없다. 원래 키를 다음 처리자에 넘기고, 뒤 단계는 같은 키를 편집으로 다시 고르지 않는다.
    Pass(EditKey),
}

/// 바인딩과 대상 능력으로 판정한다. 능력은 바인딩이 있을 때만 묻는다.
pub(crate) fn verdict(
    binding: Option<EditKey>,
    capability: impl FnOnce(EditKey) -> Capability,
) -> Verdict {
    let Some(edit) = binding else {
        return Verdict::NoBinding;
    };
    match capability(edit) {
        Capability::Yes => Verdict::Act(edit),
        Capability::No => Verdict::Pass(edit),
        Capability::Unknown => Verdict::Hold(edit),
    }
}

/// 붙잡은 키의 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 편집이 실제로 일어났다. 원래 키는 소비한다.
    Acted,
    /// 편집이 일어나지 않았다. 원래 키를 한 번 재생한다.
    NotActed,
    /// 결과가 오지 않았다. 보류를 만들 때 정한 규칙(`replays_on_timeout`)이 재생 여부를 정한다.
    TimedOut,
    /// 결과가 오기 전에 대상이 사라지거나 바뀌었다. 실행 여부를 모르므로 재생하지 않는다.
    TargetGone,
}

/// `Hold`인 원래 키와 그 뒤에 온 같은 창의 입력. 결과가 나면 [`Self::resolve`]가 풀어 준다.
#[derive(Debug)]
pub(crate) struct EditKeyHold<E> {
    original: E,
    edit: EditKey,
    queued: Vec<LineItem<E>>,
    replays_on_timeout: bool,
}

/// 보류 줄에 선 입력 하나. 창 사건과 debug 주입이 한 줄에 도착한 순서대로 서고, 재생 때 각자 창 입구를 다시 지난다.
#[derive(Debug)]
pub(crate) enum LineItem<E> {
    /// 창 사건과 그것이 창에 처음 도착한 시각. 재생한 사건의 더블탭·힌트 홀드는 처리 시각이 아니라 이 시각으로
    /// 판정한다.
    Event(E, Instant),
    /// debug 입력 주입 하나(`debug_input::DebugInput`). 재생 때 그 창의 debug 주입 입구를 다시 지난다.
    #[cfg(debug_assertions)]
    Debug(crate::view::main::debug_input::DebugInput),
}

impl<E> LineItem<E> {
    /// 창 사건이면 그 사건.
    #[cfg(test)]
    pub(crate) fn event(&self) -> Option<&E> {
        match self {
            Self::Event(event, _) => Some(event),
            #[cfg(debug_assertions)]
            Self::Debug(_) => None,
        }
    }

    /// 창 사건이면 그 도착 시각. debug 주입은 시각이 없다.
    pub(crate) fn arrived_at(&self) -> Option<Instant> {
        match self {
            Self::Event(_, at) => Some(*at),
            #[cfg(debug_assertions)]
            Self::Debug(_) => None,
        }
    }
}

/// 보류를 푼 결과. `replay`는 다음 처리자에 한 번 줄 원래 키이고 편집 판정을 다시 하지 않는다. 원래 키는 편집 키
/// 누름이라 더블탭·힌트 홀드 판정에 시각을 쓰지 않으므로 시각을 두지 않는다. `queued`는 보류 중에 온 입력으로,
/// 받은 순서대로 평소 입구(판정 포함)로 다시 처리한다.
#[derive(Debug)]
pub(crate) struct Release<E> {
    pub(crate) replay: Option<(E, EditKey)>,
    pub(crate) queued: Vec<LineItem<E>>,
}

impl<E> Release<E> {
    /// 줄 선 창 사건 가운데 가장 먼저 창에 도착한 시각(X11 관찰 정리, `explorer_ime`).
    pub(crate) fn first_queued_at(&self) -> Option<Instant> {
        self.queued.iter().find_map(LineItem::arrived_at)
    }

    /// 줄 선 창 사건 하나를 그 도착 시각과 함께 끝에 더한다.
    pub(crate) fn push(&mut self, event: E, at: Instant) {
        self.queued.push(LineItem::Event(event, at));
    }

    /// 줄 선 입력 하나를 끝에 더한다.
    pub(crate) fn push_item(&mut self, item: LineItem<E>) {
        self.queued.push(item);
    }

    /// 원래 키와 줄 선 입력.
    pub(crate) fn split(self) -> (Option<(E, EditKey)>, Vec<LineItem<E>>) {
        (self.replay, self.queued)
    }

    /// 줄 선 창 사건만 순서대로.
    #[cfg(test)]
    pub(crate) fn queued_events(&self) -> Vec<E>
    where
        E: Clone,
    {
        self.queued
            .iter()
            .filter_map(LineItem::event)
            .cloned()
            .collect()
    }

    /// 줄 선 창 사건과 그 도착 시각만 순서대로.
    #[cfg(test)]
    pub(crate) fn queued_arrivals(&self) -> Vec<(E, Instant)>
    where
        E: Clone,
    {
        self.queued
            .iter()
            .filter_map(|item| Some((item.event()?.clone(), item.arrived_at()?)))
            .collect()
    }

    /// 시험이 만드는 값: 줄 선 창 사건과 그 도착 시각. 시각이 모자라면 지금이다.
    #[cfg(test)]
    pub(crate) fn of_events(
        replay: Option<(E, EditKey)>,
        queued: Vec<E>,
        at: Vec<Instant>,
    ) -> Self {
        let at = at.into_iter().map(Some).chain(std::iter::repeat(None));
        let queued = queued
            .into_iter()
            .zip(at)
            .map(|(event, at)| LineItem::Event(event, at.unwrap_or_else(Instant::now)))
            .collect();
        Self { replay, queued }
    }
}

impl<E> EditKeyHold<E> {
    /// `replays_on_timeout`는 시간 초과면 대상이 그 편집을 실행하지 않았음이 보장되는지다(편집을
    /// 거둬들였거나 읽기가 끝나지 않았다). 실행됐을 수 있는 대상(결과를 회신하는 플러그인)은 false로
    /// 만들어, 시간 초과에 두 번 실행하지 않도록 키를 소비하고 경고를 남긴다.
    pub(crate) fn new(original: E, edit: EditKey, replays_on_timeout: bool) -> Self {
        Self {
            original,
            edit,
            queued: Vec::new(),
            replays_on_timeout,
        }
    }

    pub(crate) fn edit(&self) -> EditKey {
        self.edit
    }

    /// 보류 중에 온 입력 하나를 받은 순서대로 붙잡는다.
    pub(crate) fn queue_item(&mut self, item: LineItem<E>) {
        self.queued.push(item);
    }

    /// 줄 선 창 사건 가운데 가장 먼저 창에 도착한 시각(X11 관찰 정리, `explorer_ime`).
    pub(crate) fn first_queued_at(&self) -> Option<Instant> {
        self.queued.iter().find_map(LineItem::arrived_at)
    }

    /// 붙잡은 이벤트가 있다. 뒤에 온 이벤트가 이들을 앞지르지 않게 줄을 세울 때 본다.
    pub(crate) fn has_queued(&self) -> bool {
        !self.queued.is_empty()
    }

    pub(crate) fn resolve(self, outcome: Outcome) -> Release<E> {
        let replay = match outcome {
            Outcome::Acted | Outcome::TargetGone => false,
            Outcome::NotActed => true,
            Outcome::TimedOut if self.replays_on_timeout => true,
            Outcome::TimedOut => {
                tracing::warn!(
                    edit = ?self.edit,
                    "edit key result timed out; the key is consumed because the edit may have run"
                );
                false
            }
        };
        Release {
            replay: replay.then_some((self.original, self.edit)),
            queued: self.queued,
        }
    }
}

impl From<EditKey> for crate::webview::WebViewEditAction {
    fn from(edit: EditKey) -> Self {
        match edit {
            EditKey::Copy => Self::Copy,
            EditKey::Cut => Self::Cut,
            EditKey::Paste => Self::Paste,
        }
    }
}

#[cfg(test)]
#[path = "edit_key_tests.rs"]
mod tests;
