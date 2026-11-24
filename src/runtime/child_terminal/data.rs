//! Serialized child relationships and detached worker transaction data.
//! Engine ownership and field deltas preserve other engines and external parents. Runtime
//! access uses the Hub facade; synchronous convenience methods below exist only for backend
//! fixtures. No runtime registry guard owns this detached data during filesystem operations.

use super::persistence;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub(crate) use crate::core::child_parent::ParentKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildEntry {
    pub child_surface_id: u32,
    pub index: u32,
    pub cwd: Option<String>,
    pub role: Option<String>,
    pub nickname: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileSummary {
    pub removed_children: u32,
    pub removed_parents: u32,
}

impl ReconcileSummary {
    pub fn changed(&self) -> bool {
        self.removed_children > 0 || self.removed_parents > 0
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegistryData {
    pub(super) children: HashMap<u32, Vec<ChildEntry>>,
    /// Explicit reports carry intent even when their value equals the loaded snapshot.
    #[serde(skip)]
    pub(super) idle_intents: HashMap<u32, (u64, bool)>,
    #[serde(skip)]
    pub(super) needs_input_intents: HashMap<u32, (u64, bool)>,
    #[serde(skip)]
    pub(super) intent_serial: u64,
    pub(super) parent_of: HashMap<u32, u32>,
    pub(super) next_index: HashMap<u32, u32>,
    pub(super) idle: HashMap<u32, bool>,
    pub(super) needs_input: HashMap<u32, bool>,
    /// 마지막 상태 보고 시각(Unix epoch ms). 등록 시각으로 시작하고 상태 보고마다 갱신한다.
    /// 재시작 뒤에도 보고가 끊긴 기간을 잴 수 있다. 유효성 판정은 child_liveness에서 맡는다.
    #[serde(default)]
    pub(super) last_state_report_at: HashMap<u32, u64>,
    /// load로 정하며 default는 저장 경로가 없다.
    #[serde(skip)]
    pub(super) path: Option<PathBuf>,
    /// A failed initial read must never authorize replacing the original.
    #[serde(skip)]
    pub(super) load_error: Option<String>,
    /// Immutable loaded/committed view; never recursively retains older baselines.
    #[serde(skip)]
    pub(super) baseline: Option<Arc<Self>>,
    /// 항목 surface 가 속한 레이아웃 슬롯. 슬롯 engine 의 소유 판정과 시작 시 정리에 쓴다.
    #[serde(default)]
    pub(super) slot_of: HashMap<u32, u32>,
    /// 이 engine 이 소유한 surface. 저장 병합과 정리의 범위를 정한다.
    #[serde(skip)]
    pub(super) owned: HashSet<u32>,
    /// 이 engine 의 레이아웃 슬롯. [`Self::bind_slot`] 로 정한다.
    #[serde(skip)]
    pub(super) slot: Option<u32>,
    /// External 부모의 자식 목록. 항목은 자식 surface 의 소유를 따른다.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(super) external_children: HashMap<String, Vec<ChildEntry>>,
    /// 자식 surface → External 부모. 한 자식은 `parent_of` 와 이 표 중 한 곳에만 있다.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(super) external_parent_of: HashMap<u32, String>,
    /// External 부모의 다음 번호. 여러 engine 이 같은 부모에 번호를 줄 수 있어 병합은 큰 값을 따른다.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub(super) external_next_index: HashMap<String, u32>,
    /// 이 engine 이 뺀 External 자식의 index + 1 최댓값. 저장 병합은 파일에 남은 자식이 없고 파일의
    /// 다음 번호가 이 값을 넘지 않을 때만 파일의 다음 번호도 지운다. 메모리의 다음 번호는 병합으로
    /// 다른 engine 의 예약까지 받으므로 상한으로 쓰지 않는다.
    #[serde(skip)]
    pub(super) removed_external_top: HashMap<String, u32>,
}

impl RegistryData {
    /// Load from TASTY_HOME. Failed reads expose an empty view with writes disabled.
    #[cfg(test)]
    pub fn load() -> Self {
        let path = tasty_utils::path::tasty_home().map(|d| d.join("child-terminals.json"));
        let mut s = path.as_deref().map(read_file).unwrap_or_default();
        s.path = path;
        s
    }

    pub(super) fn checkpoint(&mut self) {
        self.baseline = None;
        self.baseline = Some(Arc::new(self.clone()));
    }

    /// Apply changed owned fields to the latest file under the participant lock.
    #[cfg(test)]
    pub fn save(&mut self) {
        if let Err(error) = self.persist(persistence::Operation::Merge) {
            tracing::warn!(%error, "child-terminal registry was not saved");
        }
    }

    #[cfg(test)]
    pub(super) fn persist(
        &mut self,
        operation: persistence::Operation,
    ) -> Result<Option<u32>, String> {
        let Some(path) = self.path.clone() else {
            return self.apply_operation(operation);
        };
        let outcome = persistence::Command::new(path, self.clone(), operation).execute()?;
        *self = outcome.registry;
        Ok(outcome.index)
    }

    pub(super) fn apply_operation(
        &mut self,
        operation: persistence::Operation,
    ) -> Result<Option<u32>, String> {
        match operation {
            persistence::Operation::Merge => Ok(None),
            persistence::Operation::Reserve(parent) => self.allocate_index(&parent).map(Some),
            persistence::Operation::Adopt(parent, mut child) => {
                if self.parent_of_child(child.child_surface_id).is_some() {
                    return Err("surface already has a child relationship".into());
                }
                child.index = self.allocate_index(&parent)?;
                let index = child.index;
                self.register_child(parent, child);
                Ok(Some(index))
            }
            persistence::Operation::Forget(child) => {
                self.unregister_child_by_surface(child);
                Ok(None)
            }
            persistence::Operation::SetNickname {
                parent,
                index,
                surface,
                nickname,
            } => self
                .set_child_nickname(&parent, index, surface, nickname)
                .map(|_| None)
                .ok_or_else(|| {
                    format!("child {index} is no longer surface {surface}; nickname not saved")
                }),
            #[cfg(feature = "gui")]
            persistence::Operation::ReleaseOwned(gone) => {
                self.drop_surfaces(&gone);
                Ok(None)
            }
            #[cfg(feature = "gui")]
            persistence::Operation::RetirePending { parent, child } => {
                let mut gone = HashSet::new();
                gone.insert(child);
                gone.extend(parent);
                self.drop_surfaces(&gone);
                Ok(None)
            }
            #[cfg(feature = "gui")]
            persistence::Operation::Prune(keep) => {
                self.prune_slots(&keep);
                Ok(None)
            }
            #[cfg(feature = "gui")]
            persistence::Operation::Clear => {
                *self = Self {
                    path: self.path.take(),
                    ..Self::default()
                };
                Ok(None)
            }
        }
    }

    /// 이 engine 의 슬롯을 정하고 그 슬롯의 항목을 소유한다. 슬롯이 없는 engine(headless)은 한
    /// 프로세스에 하나뿐이므로 읽은 항목을 모두 소유한다. 시작 복원이 슬롯을 바꾸면 다시 부른다.
    pub(crate) fn bind_slot(&mut self, slot: Option<u32>) {
        self.slot = slot;
        self.owned.clear();
        let mine: Vec<u32> = self
            .surfaces()
            .into_iter()
            .filter(|s| slot.is_none() || self.slot_of.get(s) == slot.as_ref())
            .collect();
        self.owned.extend(mine);
    }

    /// 항목 어디에든 나오는 surface.
    pub(super) fn surfaces(&self) -> HashSet<u32> {
        let mut all: HashSet<u32> = self.children.keys().copied().collect();
        all.extend(self.parent_of.keys().copied());
        all.extend(self.parent_of.values().copied());
        all.extend(self.next_index.keys().copied());
        all.extend(self.idle.keys().copied());
        all.extend(self.needs_input.keys().copied());
        all.extend(self.last_state_report_at.keys().copied());
        all.extend(self.external_parent_of.keys().copied());
        all
    }

    /// 자식이 남지 않은 External 부모의 목록과 다음 번호를 지운다. 지운 수를 반환한다.
    fn drop_empty_external(&mut self) -> u32 {
        let empty: Vec<String> = self
            .external_children
            .iter()
            .filter(|(_, list)| list.is_empty())
            .map(|(parent, _)| parent.clone())
            .collect();
        for parent in &empty {
            self.external_children.remove(parent);
            self.external_next_index.remove(parent);
        }
        empty.len() as u32
    }

    /// 시작 복원이 끝난 뒤 한 번 부른다. 파일을 다시 읽고, 슬롯 표시가 없는 이전 형식의 항목을 이
    /// engine 의 슬롯에 넣어 소유한 뒤, `keep`(다시 열 수 있거나 열린 슬롯) 밖의 슬롯에 속한 항목을
    /// 지우고 쓴다.
    #[cfg(feature = "gui")]
    #[cfg(test)]
    pub(crate) fn forget_slots_except(&mut self, keep: &HashSet<u32>) {
        if let Err(error) = self.persist(persistence::Operation::Prune(keep.clone())) {
            tracing::warn!(%error, "child-terminal registry slots were not pruned");
        }
    }

    #[cfg(feature = "gui")]
    fn prune_slots(&mut self, keep: &HashSet<u32>) {
        if let Some(slot) = self.slot {
            for surface in self.surfaces() {
                if let std::collections::hash_map::Entry::Vacant(tag) = self.slot_of.entry(surface)
                {
                    tag.insert(slot);
                    self.owned.insert(surface);
                }
            }
        }
        let gone: HashSet<u32> = self
            .slot_of
            .iter()
            .filter(|(_, slot)| !keep.contains(slot))
            .map(|(surface, _)| *surface)
            .collect();
        self.drop_surfaces(&gone);
    }

    /// engine 이 사라질 때 그 engine 이 소유한 항목(빈 부모 목록·다음 번호 포함)을 지우고 저장한다.
    #[cfg(feature = "gui")]
    #[cfg(test)]
    pub(crate) fn release_owned(&mut self) {
        let gone = self.owned.clone();
        self.drop_surfaces(&gone);
        self.save();
    }

    /// `gone` surface 의 자식 항목·상태와, 그래서 빈 `gone` 부모의 목록·다음 번호를 지운다.
    #[cfg(feature = "gui")]
    pub(super) fn drop_surfaces(&mut self, gone: &HashSet<u32>) {
        for list in self.children.values_mut() {
            list.retain(|c| !gone.contains(&c.child_surface_id));
        }
        self.children
            .retain(|parent, list| !(gone.contains(parent) && list.is_empty()));
        let children = &self.children;
        self.next_index
            .retain(|parent, _| !gone.contains(parent) || children.contains_key(parent));
        for map in [&mut self.idle, &mut self.needs_input] {
            map.retain(|surface, _| !gone.contains(surface));
        }
        self.idle_intents.retain(|child, _| !gone.contains(child));
        self.needs_input_intents
            .retain(|child, _| !gone.contains(child));
        self.parent_of.retain(|child, _| !gone.contains(child));
        self.last_state_report_at
            .retain(|surface, _| !gone.contains(surface));
        self.slot_of.retain(|surface, _| !gone.contains(surface));
        for (parent, list) in self.external_children.iter_mut() {
            note_removed(&mut self.removed_external_top, parent, list, |c| {
                gone.contains(&c.child_surface_id)
            });
        }
        self.external_parent_of
            .retain(|child, _| !gone.contains(child));
        self.drop_empty_external();
    }

    fn own(&mut self, surface: u32) {
        self.owned.insert(surface);
    }

    #[cfg(test)]
    pub(crate) fn reserve_index(&mut self, parent: &ParentKey) -> Result<u32, String> {
        self.persist(persistence::Operation::Reserve(parent.clone()))?
            .ok_or_else(|| "child index reservation returned no index".into())
    }

    pub(super) fn allocate_index(&mut self, parent: &ParentKey) -> Result<u32, String> {
        let entry = match parent {
            ParentKey::Surface(parent) => {
                self.own(*parent);
                self.next_index.entry(*parent).or_insert(0)
            }
            ParentKey::External(parent) => {
                self.removed_external_top.remove(parent);
                self.external_next_index.entry(parent.clone()).or_insert(0)
            }
        };
        let index = *entry;
        *entry = entry.checked_add(1).ok_or("child index space exhausted")?;
        Ok(index)
    }

    #[cfg(test)]
    pub fn next_index_for(&mut self, parent: u32) -> u32 {
        self.own(parent);
        let entry = self.next_index.entry(parent).or_insert(0);
        let idx = *entry;
        *entry += 1;
        idx
    }

    pub fn register_child(&mut self, parent: impl Into<ParentKey>, child: ChildEntry) {
        self.own(child.child_surface_id);
        self.last_state_report_at
            .insert(child.child_surface_id, now_epoch_ms());
        match parent.into() {
            ParentKey::Surface(parent) => {
                self.own(parent);
                self.parent_of.insert(child.child_surface_id, parent);
                self.children.entry(parent).or_default().push(child);
            }
            ParentKey::External(parent) => {
                self.external_parent_of
                    .insert(child.child_surface_id, parent.clone());
                self.external_children
                    .entry(parent)
                    .or_default()
                    .push(child);
            }
        }
    }

    fn list_of(&self, parent: &ParentKey) -> Option<&Vec<ChildEntry>> {
        match parent {
            ParentKey::Surface(parent) => self.children.get(parent),
            ParentKey::External(parent) => self.external_children.get(parent),
        }
    }

    fn list_of_mut(&mut self, parent: &ParentKey) -> Option<&mut Vec<ChildEntry>> {
        list_in(&mut self.children, &mut self.external_children, parent)
    }

    pub fn list_children(&self, parent: &ParentKey) -> &[ChildEntry] {
        self.list_of(parent).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn find_child(&self, parent: &ParentKey, index: u32) -> Option<&ChildEntry> {
        self.list_of(parent)?.iter().find(|c| c.index == index)
    }

    /// 해당 자식이 없으면 false다. 콜백이 바꾼 필드의 별도 인덱스는 여기서 다시 만들지 않는다.
    pub fn update_child<F>(&mut self, parent: &ParentKey, index: u32, f: F) -> bool
    where
        F: FnOnce(&mut ChildEntry),
    {
        let Some(list) = list_in(&mut self.children, &mut self.external_children, parent) else {
            return false;
        };
        let Some(entry) = list.iter_mut().find(|c| c.index == index) else {
            return false;
        };
        self.owned.insert(entry.child_surface_id);
        f(entry);
        true
    }

    /// 부모·번호의 자식이 아직 `surface`일 때만 nickname을 바꾸고 이전 값을 돌려준다. 그 사이 번호가 다른
    /// 자식에게 넘어갔거나 관계가 사라졌으면 None이고 아무것도 바꾸지 않는다.
    pub(super) fn set_child_nickname(
        &mut self,
        parent: &ParentKey,
        index: u32,
        surface: u32,
        nickname: Option<String>,
    ) -> Option<Option<String>> {
        let list = list_in(&mut self.children, &mut self.external_children, parent)?;
        let entry = list
            .iter_mut()
            .find(|c| c.index == index && c.child_surface_id == surface)?;
        Some(std::mem::replace(&mut entry.nickname, nickname))
    }

    pub fn parent_of_child(&self, child_surface: u32) -> Option<ParentKey> {
        if let Some(parent) = self.parent_of.get(&child_surface) {
            return Some(ParentKey::Surface(*parent));
        }
        self.external_parent_of
            .get(&child_surface)
            .map(|parent| ParentKey::External(parent.clone()))
    }

    /// External 부모의 마지막 자식이면 목록과 다음 번호도 지운다.
    pub fn remove_child(&mut self, parent: &ParentKey, index: u32) -> Option<ChildEntry> {
        let list = self.list_of_mut(parent)?;
        let pos = list.iter().position(|c| c.index == index)?;
        let removed = list.remove(pos);
        if let ParentKey::External(parent) = parent {
            raise_bound(&mut self.removed_external_top, parent, removed.index);
        }
        self.forget_child(removed.child_surface_id);
        Some(removed)
    }

    /// surface ID로 자식을 제거한다. 항목이 없으면 false다.
    pub fn unregister_child_by_surface(&mut self, surface_id: u32) -> bool {
        let Some(parent) = self.parent_of_child(surface_id) else {
            return false;
        };
        let Some(list) = self.list_of_mut(&parent) else {
            return false;
        };
        let Some(pos) = list.iter().position(|c| c.child_surface_id == surface_id) else {
            return false;
        };
        let removed = list.remove(pos);
        if let ParentKey::External(parent) = &parent {
            raise_bound(&mut self.removed_external_top, parent, removed.index);
        }
        self.forget_child(surface_id);
        true
    }

    /// 목록에서 뺀 자식의 역색인과 상태를 지운다.
    fn forget_child(&mut self, surface_id: u32) {
        self.owned.insert(surface_id);
        self.idle_intents.remove(&surface_id);
        self.needs_input_intents.remove(&surface_id);
        self.parent_of.remove(&surface_id);
        self.external_parent_of.remove(&surface_id);
        self.idle.remove(&surface_id);
        self.needs_input.remove(&surface_id);
        self.last_state_report_at.remove(&surface_id);
        self.drop_empty_external();
    }

    /// idle 값과 무관하게 needs_input도 해제한다. 대기 플래그가 남아 이후 idle·active를 가리지 않게 한다.
    pub fn set_idle(&mut self, child_surface: u32, idle: bool) {
        self.own(child_surface);
        self.record_state_intent(child_surface, Some(idle), Some(false));
        self.idle.insert(child_surface, idle);
        self.needs_input.insert(child_surface, false);
        self.stamp_state_report(child_surface);
    }

    /// 입력을 보낸 등록 자식의 idle/needs_input을 해제한다. 이전 턴의 idle을 새 작업 완료로
    /// 읽지 않도록 입력을 보낸 쪽에서 바로 갱신한다. 상태가 바뀌면 호출자가 저장한다.
    /// 미등록 surface에는 잘못된 상태 보고 기록을 새로 만들지 않는다.
    pub(crate) fn clear_idle_for_new_prompt(&mut self, child_surface: u32) -> bool {
        if self.parent_of_child(child_surface).is_none() {
            return false;
        }
        self.set_idle(child_surface, false);
        true
    }

    pub fn set_needs_input(&mut self, child_surface: u32, val: bool) {
        self.own(child_surface);
        self.record_state_intent(child_surface, None, Some(val));
        self.needs_input.insert(child_surface, val);
        self.stamp_state_report(child_surface);
    }

    fn record_state_intent(&mut self, child: u32, idle: Option<bool>, needs: Option<bool>) {
        self.intent_serial = self.intent_serial.wrapping_add(1);
        if let Some(value) = idle {
            self.idle_intents.insert(child, (self.intent_serial, value));
        }
        if let Some(value) = needs {
            self.needs_input_intents
                .insert(child, (self.intent_serial, value));
        }
    }

    fn stamp_state_report(&mut self, child_surface: u32) {
        self.last_state_report_at
            .insert(child_surface, now_epoch_ms());
    }

    /// 등록 또는 마지막 상태 보고 시각. 해당 필드가 없는 오래된 저장 항목은 None일 수 있다.
    pub fn last_state_report_at(&self, child_surface: u32) -> Option<u64> {
        self.last_state_report_at.get(&child_surface).copied()
    }

    /// 마지막 보고 이후 시간. 시각 정보가 없으면 None, 시계가 뒤로 가면 0이다.
    /// 0으로 처리하는 동안에는 실제 보고 중단 시간을 짧게 판단할 수 있다.
    pub fn hook_silence(&self, child_surface: u32, now_ms: u64) -> Option<std::time::Duration> {
        let at = self.last_state_report_at(child_surface)?;
        Some(std::time::Duration::from_millis(now_ms.saturating_sub(at)))
    }

    pub fn state_of(&self, child_surface: u32) -> &'static str {
        if self
            .needs_input
            .get(&child_surface)
            .copied()
            .unwrap_or(false)
        {
            "needs_input"
        } else if self.idle.get(&child_surface).copied().unwrap_or(false) {
            "idle"
        } else {
            "active"
        }
    }

    /// 이 창의 부모 가운데 자식이 있는 것이 정확히 하나일 때만 ID를 반환한다.
    /// 이 창의 부모는 이 engine 이 소유했거나 `live`(이 engine 의 살아 있는 surface)에 있는 부모다.
    /// 저장 병합으로 받은 다른 창의 항목은 세지 않는다.
    pub fn single_parent(&self, live: &HashSet<u32>) -> Option<u32> {
        let parents: Vec<u32> = self
            .children
            .iter()
            .filter(|(parent, list)| {
                !list.is_empty() && (self.owned.contains(parent) || live.contains(parent))
            })
            .map(|(k, _)| *k)
            .collect();
        if parents.len() == 1 {
            Some(parents[0])
        } else {
            None
        }
    }

    /// `live` 는 이 engine 의 살아 있는 surface 다. 이 engine 이 소유했는데 `live` 에 없는 자식과 상태를
    /// 지우고, 그래서 비거나 원래 빈 소유 부모의 목록과 다음 번호도 지운다. 다른 engine 이나 아직 열지
    /// 않은 윈도우의 surface 는 `live` 에 없어도 지우지 않는다.
    /// 부모 자체의 생존 여부만으로 살아 있는 자식을 지우지는 않는다.
    pub fn reconcile_with_live_surfaces(&mut self, live: &HashSet<u32>) -> ReconcileSummary {
        self.owned.extend(live.iter().copied());
        let owned = &self.owned;
        let dead = |sid: &u32| owned.contains(sid) && !live.contains(sid);
        let mut summary = ReconcileSummary::default();

        let mut dead_parents: Vec<u32> = Vec::new();
        for (parent, list) in self.children.iter_mut() {
            let before = list.len();
            list.retain(|c| !dead(&c.child_surface_id));
            let removed = before - list.len();
            summary.removed_children += removed as u32;
            if list.is_empty() && owned.contains(parent) {
                dead_parents.push(*parent);
            }
        }

        for parent in &dead_parents {
            self.children.remove(parent);
            self.next_index.remove(parent);
            if !live.contains(parent) {
                summary.removed_parents += 1;
            }
        }

        for (parent, list) in self.external_children.iter_mut() {
            summary.removed_children +=
                note_removed(&mut self.removed_external_top, parent, list, |c| {
                    dead(&c.child_surface_id)
                });
        }

        self.idle_intents.retain(|sid, _| !dead(sid));
        self.needs_input_intents.retain(|sid, _| !dead(sid));
        self.parent_of.retain(|sid, _| !dead(sid));
        self.external_parent_of.retain(|sid, _| !dead(sid));
        self.idle.retain(|sid, _| !dead(sid));
        self.needs_input.retain(|sid, _| !dead(sid));
        self.last_state_report_at.retain(|sid, _| !dead(sid));
        self.slot_of.retain(|sid, _| !dead(sid));
        summary.removed_parents += self.drop_empty_external();

        summary
    }
}

/// 재시작 뒤에도 비교할 수 있도록 상태 보고 시각을 Unix epoch ms로 기록한다.
pub fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 첫 engine 을 만들기 전에 한 번 부른다. 레이아웃을 복원하지 않으면(`restore_layout = false`)
/// 이전 실행의 surface 는 어느 창에도 돌아오지 않으므로 관계를 모두 버린다. 복원하면 슬롯을 다 알게 된
/// 뒤 [`RegistryData::forget_slots_except`] 가 정리한다.
#[cfg(feature = "gui")]
#[cfg(test)]
pub(crate) fn clear_on_boot(settings: &crate::settings::Settings) {
    let Some(path) = tasty_utils::path::tasty_home().map(|d| d.join("child-terminals.json")) else {
        return;
    };
    if settings.general.restore_layout || !path.exists() {
        return;
    }
    let registry = read_file(&path);
    if let Err(error) =
        persistence::Command::new(path, registry, persistence::Operation::Clear).execute()
    {
        tracing::warn!(%error, "child-terminal registry was not cleared on boot");
    }
}

/// Keep the load usable for observation while refusing all writes after an invalid initial read.
pub(super) fn read_file(path: &std::path::Path) -> RegistryData {
    persistence::read(path)
        .map(|mut value| {
            value.checkpoint();
            value
        })
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "child-terminal registry load failed; protecting original");
            RegistryData {
                load_error: Some(error),
                ..Default::default()
            }
        })
}

/// 뺀 External 자식의 index 로 그 부모의 상한(index + 1 최댓값)을 올린다.
fn raise_bound(bounds: &mut HashMap<String, u32>, parent: &str, index: u32) {
    let top = index.saturating_add(1);
    let bound = bounds.entry(parent.to_owned()).or_insert(top);
    *bound = (*bound).max(top);
}

/// `remove` 가 참인 External 자식을 빼고 상한을 올린다. 뺀 수를 반환한다.
fn note_removed(
    bounds: &mut HashMap<String, u32>,
    parent: &str,
    list: &mut Vec<ChildEntry>,
    remove: impl Fn(&ChildEntry) -> bool,
) -> u32 {
    let before = list.len();
    list.retain(|c| {
        if remove(c) {
            raise_bound(bounds, parent, c.index);
            false
        } else {
            true
        }
    });
    (before - list.len()) as u32
}

/// 부모 키의 목록을 고른다. 다른 필드와 동시에 빌릴 수 있게 두 표만 받는다.
fn list_in<'a>(
    children: &'a mut HashMap<u32, Vec<ChildEntry>>,
    external: &'a mut HashMap<String, Vec<ChildEntry>>,
    parent: &ParentKey,
) -> Option<&'a mut Vec<ChildEntry>> {
    match parent {
        ParentKey::Surface(parent) => children.get_mut(parent),
        ParentKey::External(parent) => external.get_mut(parent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(child_surface_id: u32, index: u32) -> ChildEntry {
        ChildEntry {
            child_surface_id,
            index,
            cwd: None,
            role: None,
            nickname: None,
        }
    }

    #[test]
    fn a_new_prompt_clears_idle_on_a_registered_child() {
        let mut reg = RegistryData::default();
        reg.register_child(10, entry(50, 0));
        reg.set_idle(50, true);
        assert_eq!(reg.state_of(50), "idle");

        assert!(reg.clear_idle_for_new_prompt(50));
        assert_eq!(reg.state_of(50), "active");
    }

    #[test]
    fn a_new_prompt_clears_needs_input_too() {
        let mut reg = RegistryData::default();
        reg.register_child(10, entry(50, 0));
        reg.set_needs_input(50, true);
        assert_eq!(reg.state_of(50), "needs_input");

        assert!(reg.clear_idle_for_new_prompt(50));
        assert_eq!(reg.state_of(50), "active");
    }

    #[test]
    fn a_new_prompt_leaves_an_unregistered_surface_unrecorded() {
        let mut reg = RegistryData::default();
        assert!(!reg.clear_idle_for_new_prompt(777));
        assert_eq!(reg.state_of(777), "active");
        assert_eq!(reg.last_state_report_at(777), None);
    }

    #[test]
    fn register_and_list() {
        let mut s = RegistryData::default();
        let idx = s.next_index_for(10);
        s.register_child(
            10,
            ChildEntry {
                child_surface_id: 100,
                index: idx,
                cwd: Some("/tmp".into()),
                role: None,
                nickname: None,
            },
        );
        assert_eq!(s.list_children(&10.into()).len(), 1);
        assert_eq!(s.find_child(&10.into(), 0).unwrap().child_surface_id, 100);
    }

    #[test]
    fn next_index_monotonic_per_parent() {
        let mut s = RegistryData::default();
        assert_eq!(s.next_index_for(10), 0);
        assert_eq!(s.next_index_for(10), 1);
        assert_eq!(s.next_index_for(20), 0);
        assert_eq!(s.next_index_for(10), 2);
    }

    #[test]
    fn state_priority_needs_input_over_idle() {
        let mut s = RegistryData::default();
        s.set_idle(50, true);
        assert_eq!(s.state_of(50), "idle");
        s.set_needs_input(50, true);
        assert_eq!(s.state_of(50), "needs_input");
    }

    #[test]
    fn set_idle_false_clears_needs_input() {
        let mut s = RegistryData::default();
        s.set_idle(50, true);
        s.set_needs_input(50, true);
        s.set_idle(50, false);
        assert_eq!(s.state_of(50), "active");
    }

    #[test]
    fn remove_child_clears_indexes() {
        let mut s = RegistryData::default();
        let idx = s.next_index_for(10);
        s.register_child(10, entry(100, idx));
        s.set_idle(100, true);
        let removed = s.remove_child(&10.into(), idx).unwrap();
        assert_eq!(removed.child_surface_id, 100);
        assert!(s.find_child(&10.into(), idx).is_none());
        assert_eq!(s.state_of(100), "active");
    }

    #[test]
    fn unregister_child_by_surface_removes_entry_and_indexes() {
        let mut s = RegistryData::default();
        let idx = s.next_index_for(10);
        s.register_child(10, entry(100, idx));
        s.set_idle(100, true);
        assert!(s.unregister_child_by_surface(100));
        assert!(s.find_child(&10.into(), idx).is_none());
        assert_eq!(s.parent_of_child(100), None);
        assert_eq!(s.state_of(100), "active");
        assert!(!s.unregister_child_by_surface(100));
    }

    #[test]
    fn register_seeds_state_report_baseline() {
        let mut s = RegistryData::default();
        let before = now_epoch_ms();
        let idx = s.next_index_for(10);
        s.register_child(10, entry(100, idx));
        let at = s
            .last_state_report_at(100)
            .expect("등록 시각이 기록돼야 한다");
        assert!(at >= before, "{at} >= {before}");
        assert_eq!(
            s.last_state_report_at(999),
            None,
            "미등록 surface 는 기준점 없음"
        );
    }

    #[test]
    fn hook_push_refreshes_state_report() {
        let mut s = RegistryData::default();
        s.set_idle(50, true);
        let first = s.last_state_report_at(50).unwrap();
        s.set_needs_input(50, true);
        let second = s.last_state_report_at(50).unwrap();
        assert!(second >= first);
        s.set_idle(50, false);
        assert!(s.last_state_report_at(50).unwrap() >= second);
    }

    #[test]
    fn hook_silence_measures_from_last_report() {
        let mut s = RegistryData::default();
        s.set_idle(50, true);
        let at = s.last_state_report_at(50).unwrap();
        assert_eq!(
            s.hook_silence(50, at + 7_000),
            Some(std::time::Duration::from_secs(7))
        );
        assert_eq!(s.hook_silence(999, at), None, "기준점 없으면 판정 불가");
    }

    #[test]
    fn hook_silence_clamps_on_clock_rewind() {
        let mut s = RegistryData::default();
        s.set_idle(50, true);
        let at = s.last_state_report_at(50).unwrap();
        assert_eq!(
            s.hook_silence(50, at.saturating_sub(60_000)),
            Some(std::time::Duration::ZERO),
            "시계가 뒤로 가면 경과 시간을 0으로 처리해야 한다"
        );
    }

    #[test]
    fn state_report_axis_is_cleared_with_the_child() {
        let mut s = RegistryData::default();
        let idx = s.next_index_for(10);
        s.register_child(10, entry(100, idx));
        s.remove_child(&10.into(), idx);
        assert_eq!(s.last_state_report_at(100), None);

        let idx = s.next_index_for(20);
        s.register_child(20, entry(200, idx));
        s.reconcile_with_live_surfaces(&HashSet::new());
        assert_eq!(s.last_state_report_at(200), None);
    }

    #[test]
    fn legacy_persisted_registry_loads_without_state_report_field() {
        // last_state_report_at 필드가 없는 저장 파일도 읽을 수 있어야 한다.
        let legacy = r#"{
            "children": { "10": [{ "child_surface_id": 100, "index": 0,
                                   "cwd": null, "role": null, "nickname": null }] },
            "parent_of": { "100": 10 },
            "next_index": { "10": 1 },
            "idle": {},
            "needs_input": {}
        }"#;
        let s: RegistryData = serde_json::from_str(legacy).expect("하위호환 로드");
        assert_eq!(s.list_children(&10.into()).len(), 1);
        assert_eq!(s.state_of(100), "active");
        assert_eq!(
            s.last_state_report_at(100),
            None,
            "보고 시각이 없으면 경과 시간도 알 수 없다"
        );
    }

    #[test]
    fn single_parent_returns_some_when_one() {
        let mut s = RegistryData::default();
        assert_eq!(s.single_parent(&HashSet::new()), None);
        s.register_child(10, entry(100, 0));
        assert_eq!(s.single_parent(&HashSet::new()), Some(10));
        s.register_child(20, entry(200, 0));
        assert_eq!(s.single_parent(&HashSet::new()), None);
    }

    #[test]
    fn reconcile_removes_dead_child_surface() {
        let mut s = RegistryData::default();
        let idx0 = s.next_index_for(10);
        s.register_child(10, entry(100, idx0));
        let idx1 = s.next_index_for(10);
        s.register_child(10, entry(101, idx1));
        s.set_idle(101, true);
        s.set_needs_input(101, true);

        let live: HashSet<u32> = [10u32, 100].into_iter().collect();
        let summary = s.reconcile_with_live_surfaces(&live);

        assert_eq!(summary.removed_children, 1);
        assert_eq!(summary.removed_parents, 0);
        assert!(s.find_child(&10.into(), idx0).is_some());
        assert!(s.find_child(&10.into(), idx1).is_none());
        assert_eq!(s.parent_of_child(101), None);
        assert_eq!(s.state_of(101), "active");
        assert_eq!(s.next_index_for(10), 2);
    }

    #[test]
    fn reconcile_removes_orphan_parent_key() {
        let mut s = RegistryData::default();
        let idx = s.next_index_for(10);
        s.register_child(10, entry(100, idx));

        let live: HashSet<u32> = HashSet::new();
        let summary = s.reconcile_with_live_surfaces(&live);

        assert_eq!(summary.removed_children, 1);
        assert_eq!(summary.removed_parents, 1);
        assert_eq!(s.list_children(&10.into()).len(), 0);
        assert_eq!(s.parent_of_child(100), None);
        assert_eq!(s.next_index_for(10), 0);
    }

    #[test]
    fn reconcile_preserves_live_entries() {
        let mut s = RegistryData::default();
        let idx0 = s.next_index_for(10);
        s.register_child(10, entry(100, idx0));
        let idx1 = s.next_index_for(20);
        s.register_child(20, entry(200, idx1));
        s.set_idle(100, true);

        let live: HashSet<u32> = [10u32, 20, 100, 200].into_iter().collect();
        let summary = s.reconcile_with_live_surfaces(&live);

        assert_eq!(summary.removed_children, 0);
        assert_eq!(summary.removed_parents, 0);
        assert!(s.find_child(&10.into(), idx0).is_some());
        assert!(s.find_child(&20.into(), idx1).is_some());
        assert_eq!(s.parent_of_child(100), Some(10.into()));
        assert_eq!(s.state_of(100), "idle");
    }

    #[test]
    fn reconcile_summary_counts_multiple_parents() {
        let mut s = RegistryData::default();
        let i0 = s.next_index_for(10);
        s.register_child(10, entry(100, i0));
        let i1 = s.next_index_for(10);
        s.register_child(10, entry(101, i1));
        let j0 = s.next_index_for(20);
        s.register_child(20, entry(200, j0));
        let j1 = s.next_index_for(20);
        s.register_child(20, entry(201, j1));
        let k0 = s.next_index_for(30);
        s.register_child(30, entry(300, k0));

        let live: HashSet<u32> = [20u32, 30, 200].into_iter().collect();
        let summary = s.reconcile_with_live_surfaces(&live);

        assert_eq!(summary.removed_children, 4);
        assert_eq!(summary.removed_parents, 1);
        assert_eq!(s.list_children(&20.into()).len(), 1);
        assert_eq!(s.list_children(&10.into()).len(), 0);
        assert_eq!(s.list_children(&30.into()).len(), 0);
    }

    fn live(ids: &[u32]) -> HashSet<u32> {
        ids.iter().copied().collect()
    }

    fn children_on_disk() -> Vec<(u32, u32)> {
        let disk = RegistryData::load();
        let mut pairs: Vec<(u32, u32)> = disk.parent_of.iter().map(|(c, p)| (*p, *c)).collect();
        pairs.sort();
        pairs
    }

    /// 윈도우 두 개가 각자 읽은 레지스트리로 자식을 등록해도 저장이 서로를 지우지 않는다.
    #[test]
    fn children_registered_in_two_windows_both_survive_a_restart() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut first = RegistryData::load();
        let mut second = RegistryData::load();
        let index = first.next_index_for(513);
        first.register_child(513, entry(1027, index));
        first.save();
        let index = second.next_index_for(1026);
        second.register_child(1026, entry(1028, index));
        second.save();
        assert_eq!(children_on_disk(), [(513, 1027), (1026, 1028)]);
        assert_eq!(
            second.list_children(&513.into()).len(),
            1,
            "save also refreshes the copy"
        );

        // 지운 것도 자기 항목만 파일에 반영한다.
        assert!(first.unregister_child_by_surface(1027));
        first.save();
        assert_eq!(children_on_disk(), [(1026, 1028)]);

        // 재시작 뒤 아직 정리를 거치지 않은 윈도우가 surface 를 닫아도 그 해제가 파일에 남는다.
        let mut restarted = RegistryData::load();
        assert!(restarted.unregister_child_by_surface(1028));
        restarted.save();
        assert_eq!(children_on_disk(), []);
    }

    /// 재시작 뒤 먼저 연 윈도우의 정리는 아직 열지 않은 윈도우의 관계를 지우지 않는다.
    #[test]
    fn reconcile_keeps_children_of_windows_it_does_not_own() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut before = RegistryData::load();
        before.register_child(513, entry(1027, 0));
        before.register_child(1026, entry(1028, 0));
        before.save();

        let mut first = RegistryData::load();
        let summary = first.reconcile_with_live_surfaces(&live(&[513, 1027]));
        assert!(!summary.changed());
        first.save();
        assert_eq!(children_on_disk(), [(513, 1027), (1026, 1028)]);

        // 소유한 surface 가 사라지면 그것만 지운다.
        let summary = first.reconcile_with_live_surfaces(&live(&[513]));
        assert_eq!(summary.removed_children, 1);
        first.save();
        assert_eq!(children_on_disk(), [(1026, 1028)]);
    }

    /// 슬롯 `slot` 의 창이 읽은 레지스트리.
    #[cfg(feature = "gui")]
    fn window(slot: u32) -> RegistryData {
        let mut registry = RegistryData::load();
        registry.bind_slot(Some(slot));
        registry
    }

    /// 설정의 `restore_layout` 으로 시작 시 정리를 부른다.
    #[cfg(feature = "gui")]
    fn boot(restore_layout: bool) {
        let mut settings = crate::settings::Settings::default();
        settings.general.restore_layout = restore_layout;
        clear_on_boot(&settings);
    }

    /// 시작 시 정리는 레이아웃을 복원하면 관계를 그대로 두고, 복원하지 않으면 모두 버린다.
    #[cfg(feature = "gui")]
    #[test]
    fn boot_clear_follows_the_restore_layout_setting() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut first = window(1);
        first.register_child(513, entry(1027, 0));
        first.save();
        boot(true);
        assert_eq!(children_on_disk(), [(513, 1027)]);
        boot(false);
        assert_eq!(children_on_disk(), []);
    }

    /// 다른 창의 항목을 저장 병합으로 받아도 부모 생략 폴백은 이 창의 부모만 센다. 결과가 마지막 저장
    /// 시점에 따라 달라지지 않는다.
    #[cfg(feature = "gui")]
    #[test]
    fn single_parent_counts_only_this_windows_parents_after_a_merge() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut first = window(1);
        let mut second = window(2);
        first.register_child(513, entry(1027, 0));
        first.save();
        second.register_child(1026, entry(1028, 0));
        second.save();
        assert_eq!(first.single_parent(&live(&[513, 1027])), Some(513));

        // 첫 창이 다시 저장해 둘째 창의 항목을 받은 뒤에도 같다.
        first.set_idle(1027, true);
        first.save();
        assert_eq!(
            first.list_children(&1026.into()).len(),
            1,
            "merged the other window"
        );
        assert_eq!(first.single_parent(&live(&[513, 1027])), Some(513));
        assert_eq!(second.single_parent(&live(&[1026, 1028])), Some(1026));

        // 재시작 뒤 아직 정리하지 않은 창도 자기 슬롯의 부모만 센다.
        assert_eq!(window(1).single_parent(&live(&[])), Some(513));
        assert_eq!(window(2).single_parent(&live(&[])), Some(1026));
    }

    /// 레이아웃을 복원하지 않으면 재시작마다 이전 실행의 관계를 버린다. 다시 열리지 않는 둘째 창의
    /// 관계도 남지 않으므로 유령이 쌓여 부모 생략 폴백을 막지 않는다.
    #[cfg(feature = "gui")]
    #[test]
    fn relations_do_not_pile_up_across_restarts_without_layout_restore() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut second = window(2);
        second.register_child(1026, entry(1029, 0));
        second.save();
        for (parent, child) in [(1027, 1028), (1541, 1542), (2055, 2056)] {
            boot(false);
            let mut registry = window(1);
            registry.reconcile_with_live_surfaces(&live(&[parent, child]));
            registry.register_child(parent, entry(child, 0));
            registry.save();
            assert_eq!(
                registry.single_parent(&live(&[parent, child])),
                Some(parent)
            );
            assert_eq!(children_on_disk(), [(parent, child)]);
        }
    }

    /// 레이아웃을 복원하면 시작 복원 뒤 다시 열 수 있는 슬롯의 관계만 남긴다. 슬롯 표시가 없는 이전
    /// 형식의 항목은 첫 창의 슬롯에 속한다.
    #[cfg(feature = "gui")]
    #[test]
    fn boot_prune_keeps_relations_of_slots_that_can_reopen() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut legacy = RegistryData::load();
        legacy.register_child(7, entry(8, 0));
        legacy.save();
        let mut second = window(2);
        second.register_child(1026, entry(1028, 0));
        second.save();
        let mut third = window(3);
        third.register_child(1540, entry(1542, 0));
        third.save();

        boot(true);
        assert_eq!(children_on_disk(), [(7, 8), (1026, 1028), (1540, 1542)]);
        window(1).forget_slots_except(&[1, 2].into_iter().collect());
        assert_eq!(children_on_disk(), [(7, 8), (1026, 1028)]);
        assert_eq!(window(1).single_parent(&live(&[])), Some(7));
    }

    /// 다시 열지 않는 창이 닫히면 그 창의 관계와 빈 부모 목록·다음 번호를 지우고 다른 창 것은 둔다.
    #[cfg(feature = "gui")]
    #[test]
    fn a_released_window_removes_its_parent_keys() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut first = window(1);
        let index = first.next_index_for(513);
        first.register_child(513, entry(1027, index));
        first.save();
        let mut second = window(2);
        let index = second.next_index_for(1026);
        second.register_child(1026, entry(1028, index));
        assert!(second.unregister_child_by_surface(1028));
        second.save();
        let on_disk = RegistryData::load();
        assert!(
            on_disk.next_index.contains_key(&1026),
            "keys stay while open"
        );

        second.release_owned();
        let on_disk = RegistryData::load();
        assert_eq!(children_on_disk(), [(513, 1027)]);
        assert!(!on_disk.children.contains_key(&1026));
        assert!(!on_disk.next_index.contains_key(&1026));
        assert!(on_disk.next_index.contains_key(&513));
    }

    fn ext(key: &str) -> ParentKey {
        ParentKey::External(key.into())
    }

    fn external_on_disk() -> Vec<(String, u32, u32)> {
        let disk = RegistryData::load();
        let mut rows: Vec<(String, u32, u32)> = disk
            .external_children
            .iter()
            .flat_map(|(parent, list)| {
                list.iter()
                    .map(move |c| (parent.clone(), c.index, c.child_surface_id))
            })
            .collect();
        rows.sort();
        rows
    }

    /// Surface 부모만 쓰는 저장 파일은 External 필드가 없어 이전 형식과 같고, 이전 파일도 그대로 읽힌다.
    #[test]
    fn a_surface_only_registry_keeps_the_previous_file_shape() {
        let mut s = RegistryData::default();
        s.register_child(10, entry(100, 0));
        let json = serde_json::to_value(&s).unwrap();
        for key in [
            "external_children",
            "external_parent_of",
            "external_next_index",
        ] {
            assert!(json.get(key).is_none(), "{key} must be omitted");
        }
        let legacy = r#"{"children":{"10":[{"child_surface_id":100,"index":0,"cwd":null,"role":null,"nickname":null}]},
            "parent_of":{"100":10},"next_index":{"10":1},"idle":{},"needs_input":{}}"#;
        let loaded: RegistryData = serde_json::from_str(legacy).unwrap();
        assert_eq!(loaded.parent_of_child(100), Some(ParentKey::Surface(10)));
        assert_eq!(loaded.list_children(&10.into()).len(), 1);
    }

    /// External 부모 둘은 번호·목록·해제가 서로 섞이지 않는다. 마지막 자식이 빠지면 그 부모만 지우고
    /// 번호를 다시 0부터 준다. Surface 부모는 기존처럼 빈 목록을 정리 단계까지 둔다.
    #[test]
    fn two_external_parents_are_isolated_and_restart_after_the_last_child() {
        let mut s = RegistryData::default();
        let (a, b) = (ext("codex:thread-a"), ext("codex:thread-b"));
        assert_eq!(s.reserve_index(&a), Ok(0));
        assert_eq!(s.reserve_index(&a), Ok(1));
        assert_eq!(s.reserve_index(&b), Ok(0));
        s.register_child(a.clone(), entry(100, 0));
        s.register_child(a.clone(), entry(101, 1));
        s.register_child(b.clone(), entry(200, 0));
        assert_eq!(s.parent_of_child(101), Some(a.clone()));
        assert_eq!(s.parent_of_child(200), Some(b.clone()));
        assert_eq!(s.find_child(&a, 1).unwrap().child_surface_id, 101);
        assert!(s.find_child(&b, 1).is_none());
        assert!(s.list_children(&ParentKey::Surface(100)).is_empty());

        assert_eq!(s.remove_child(&a, 0).unwrap().child_surface_id, 100);
        assert_eq!(s.reserve_index(&a), Ok(2), "a still has a child");
        assert!(s.unregister_child_by_surface(101));
        assert!(s.list_children(&a).is_empty());
        assert_eq!(s.reserve_index(&a), Ok(0), "emptied parent restarts");
        assert_eq!(s.list_children(&b).len(), 1, "b untouched");
        assert_eq!(s.reserve_index(&b), Ok(1));

        s.register_child(10, entry(300, 0));
        assert!(s.remove_child(&10.into(), 0).is_some());
        assert!(
            s.children.contains_key(&10),
            "surface list waits for reconcile"
        );
    }

    /// External 부모는 surface 가 아니므로 부모 생략 폴백 후보가 아니다.
    #[test]
    fn single_parent_ignores_external_parents() {
        let mut s = RegistryData::default();
        s.register_child(ext("codex:t"), entry(100, 0));
        assert_eq!(s.single_parent(&live(&[100])), None);
        s.register_child(10, entry(101, 0));
        assert_eq!(s.single_parent(&live(&[10, 100, 101])), Some(10));
    }

    /// 정리는 이 engine 이 소유한 죽은 External 자식만 지우고, 비면 부모와 번호를 함께 지운다.
    #[test]
    fn reconcile_drops_dead_external_children_and_the_emptied_parent() {
        let mut s = RegistryData::default();
        let a = ext("codex:a");
        let index = s.reserve_index(&a).unwrap();
        s.register_child(a.clone(), entry(100, index));
        let summary = s.reconcile_with_live_surfaces(&live(&[100]));
        assert!(!summary.changed());
        let summary = s.reconcile_with_live_surfaces(&live(&[]));
        assert_eq!(summary.removed_children, 1);
        assert_eq!(summary.removed_parents, 1);
        assert!(s.parent_of_child(100).is_none());
        assert!(!s.external_next_index.contains_key("codex:a"));
    }

    /// 두 engine 이 같은 External 부모에 번호를 줘도 겹치지 않고, 저장이 서로의 자식을 지우지 않는다.
    /// 마지막 자식을 뺀 engine 의 저장이 파일의 번호까지 지워 다음 번호가 0부터 시작한다.
    #[test]
    fn two_engines_share_an_external_parent_without_duplicate_indices() {
        let _home = tasty_test_support::IsolatedHome::new();
        let mut first = RegistryData::load();
        let mut second = RegistryData::load();
        let parent = ext("codex:shared");
        let a = first.reserve_index(&parent).unwrap();
        let b = second.reserve_index(&parent).unwrap();
        assert_eq!((a, b), (0, 1));
        first.register_child(parent.clone(), entry(1027, a));
        first.save();
        second.register_child(parent.clone(), entry(1540, b));
        second.save();
        let shared = || "codex:shared".to_string();
        assert_eq!(
            external_on_disk(),
            [(shared(), 0, 1027), (shared(), 1, 1540)]
        );

        assert!(first.unregister_child_by_surface(1027));
        first.save();
        assert_eq!(external_on_disk(), [(shared(), 1, 1540)]);
        assert_eq!(
            RegistryData::load().external_next_index.get("codex:shared"),
            Some(&2),
            "the other engine still holds a child"
        );

        assert!(second.remove_child(&parent, 1).is_some());
        second.save();
        assert_eq!(external_on_disk(), []);
        let disk = RegistryData::load();
        assert!(!disk.external_next_index.contains_key("codex:shared"));
        assert!(disk.external_parent_of.is_empty());
        assert_eq!(first.reserve_index(&parent), Ok(0));
    }

    /// 다른 engine 이 번호를 예약만 하고 아직 등록하지 않았을 때 마지막 자식을 빼도 그 예약 번호를
    /// 다시 주지 않는다.
    #[test]
    fn a_pending_reservation_in_another_engine_survives_the_last_release() {
        let _home = tasty_test_support::IsolatedHome::new();
        let parent = ext("codex:race");
        let mut first = RegistryData::load();
        let mut second = RegistryData::load();
        let index = first.reserve_index(&parent).unwrap();
        first.register_child(parent.clone(), entry(1027, index));
        first.save();
        let pending = second.reserve_index(&parent).unwrap();
        assert_eq!(pending, 1);

        assert!(first.unregister_child_by_surface(1027));
        first.save();
        assert_eq!(
            RegistryData::load().external_next_index.get("codex:race"),
            Some(&2)
        );
        assert_eq!(first.reserve_index(&parent), Ok(2));
    }

    /// 자식이 남은 동안 저장을 거쳐도 앞서 뺀 번호의 상한이 남아, 마지막 자식이 빠지면 번호를 지운다.
    #[test]
    fn the_removed_index_bound_survives_saves_while_children_remain() {
        let _home = tasty_test_support::IsolatedHome::new();
        let parent = ext("codex:keep");
        let mut engine = RegistryData::load();
        for surface in [1027, 1028] {
            let index = engine.reserve_index(&parent).unwrap();
            engine.register_child(parent.clone(), entry(surface, index));
        }
        engine.save();
        assert!(engine.remove_child(&parent, 1).is_some());
        engine.save();
        engine.set_idle(1027, true);
        engine.save();
        assert!(engine.remove_child(&parent, 0).is_some());
        engine.save();
        assert!(
            !RegistryData::load()
                .external_next_index
                .contains_key("codex:keep")
        );
        assert_eq!(engine.reserve_index(&parent), Ok(0));
    }

    /// 다시 열지 않는 창의 External 자식은 창 정리와 함께 지워지고 다른 창의 자식은 남는다.
    #[cfg(feature = "gui")]
    #[test]
    fn released_and_pruned_windows_drop_only_their_external_children() {
        let _home = tasty_test_support::IsolatedHome::new();
        let parent = ext("codex:p");
        let mut first = window(1);
        let index = first.reserve_index(&parent).unwrap();
        first.register_child(parent.clone(), entry(1027, index));
        first.save();
        let mut second = window(2);
        let index = second.reserve_index(&parent).unwrap();
        second.register_child(parent.clone(), entry(1540, index));
        second.save();
        let mut third = window(3);
        let index = third.reserve_index(&parent).unwrap();
        third.register_child(parent.clone(), entry(2055, index));
        third.save();
        let p = || "codex:p".to_string();
        assert_eq!(
            external_on_disk(),
            [(p(), 0, 1027), (p(), 1, 1540), (p(), 2, 2055)]
        );

        second.release_owned();
        assert_eq!(external_on_disk(), [(p(), 0, 1027), (p(), 2, 2055)]);

        first.forget_slots_except(&[1].into_iter().collect());
        assert_eq!(external_on_disk(), [(p(), 0, 1027)]);
        let disk = RegistryData::load();
        assert_eq!(disk.external_next_index.get("codex:p"), Some(&3));
        assert_eq!(disk.parent_of_child(1027), Some(parent.clone()));

        // 정리한 창이 마지막 자식을 빼면 앞서 정리로 뺀 번호까지 알고 있어 번호를 지운다.
        first.release_owned();
        assert_eq!(external_on_disk(), []);
        assert!(RegistryData::load().external_next_index.is_empty());
    }
}

#[cfg(test)]
mod regression;
