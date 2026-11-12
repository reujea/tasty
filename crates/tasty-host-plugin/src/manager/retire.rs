//! 플러그인 종료 후 회수는 전용 스레드가 맡고 호스트는 완료 여부를 폴링한다.
//! 이전 프로세스가 쓰던 포트·파일과 겹치지 않도록 회수 뒤에 재시작한다.
//! 명시적 enable이나 파일 교체는 회수 완료를 기다린다.
//! 스레드를 만들지 못하면 호출한 스레드에서 기다린다.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::host_cmd::RemoteRetirementCompletion;
use crate::process::{CHILD_EXIT_POLL_INTERVAL, PendingShutdown, PluginProcess, ShutdownOutcome};

use super::{PLUGIN_SHUTDOWN_TIMEOUT, PluginManager, PluginTick};

/// 회수 중인 plugin 한 건.
pub(crate) struct Retiring {
    /// 대기 스레드. 스레드를 못 띄워 그 자리에서 기다린 경우엔 `None` 이고 결과가
    /// [`Self::done`] 에 이미 있다.
    handle: Option<JoinHandle<ShutdownOutcome>>,
    done: Option<ShutdownOutcome>,
    started: Instant,
    /// 회수가 끝나면 다시 띄우는가. 무응답 재시작과, 회수 중에 기동 창구를 지난 전체 기동
    /// (`discover_and_start`)이 세우며, 회수 중에 온 `disable` 과 호스트 종료가 내린다.
    /// 명시적 `enable` 은 이것을 세우지 않고 회수를 기다려 그 자리에서 띄운다.
    respawn: bool,
    /// 회수 중인 프로세스의 응답 바인딩. 이 세대가 만든 surface 의 소멸을 판정한다.
    generation: Option<std::sync::Weak<()>>,
    /// 이 세대의 회수가 끝나면 성공으로 확정할 surface 회수 receipt.
    waiters: Vec<RemoteRetirementCompletion>,
    /// 관측이 멈춘 동안 미리 거둔 경우 그때까지 걸린 시간(ms). 기록 줄이 실제 회수 시각을 쓴다.
    settled_ms: Option<f64>,
}

impl Retiring {
    fn is_finished(&self) -> bool {
        self.done.is_some() || self.handle.as_ref().is_some_and(|h| h.is_finished())
    }

    /// 결과를 꺼낸다. 회수 스레드가 실행 중이면 끝날 때까지 기다린다.
    fn join(mut self) -> ShutdownOutcome {
        if let Some(outcome) = self.done.take() {
            return outcome;
        }
        match self.handle.take().map(JoinHandle::join) {
            Some(Ok(outcome)) => outcome,
            // 대기 스레드가 패닉했다. 남은 핸들의 Drop이 종료·회수를 시도한다.
            Some(Err(_)) => {
                tracing::warn!("plugin retire thread panicked");
                ShutdownOutcome::Killed
            }
            None => ShutdownOutcome::NoChild,
        }
    }
}

/// 대기 핸들을 스레드와 나눠 쥐는 자리. 스레드를 못 띄우면 호출자가 되찾는다.
type PendingSlot = Arc<Mutex<Option<PendingShutdown>>>;

static SLOT_POISONED: AtomicBool = AtomicBool::new(false);
const SLOT_WHAT: &str = "plugin retire slot";

/// 잠금에서 종료 핸들을 꺼내 회수를 기다린다. poison 상태여도 Option 값을 재사용한다.
fn wait_slot(slot: &PendingSlot) -> ShutdownOutcome {
    let taken = tasty_utils::poison::recover_mutex(slot.lock(), SLOT_WHAT, &SLOT_POISONED).take();
    match taken {
        Some(p) => p.wait(),
        None => ShutdownOutcome::NoChild,
    }
}

/// 회수를 별도 스레드에 맡긴다. 스레드를 만들지 못하면 여기서 기다린다.
fn spawn_waiter(plugin_id: &str, pending: PendingShutdown) -> Retiring {
    let started = Instant::now();
    let slot: PendingSlot = Arc::new(Mutex::new(Some(pending)));
    let for_thread = Arc::clone(&slot);
    let spawned = std::thread::Builder::new()
        .name(format!("plugin-retire-{plugin_id}"))
        .spawn(move || wait_slot(&for_thread));
    match spawned {
        Ok(handle) => Retiring {
            handle: Some(handle),
            done: None,
            started,
            respawn: false,
            generation: None,
            waiters: Vec::new(),
            settled_ms: None,
        },
        Err(e) => {
            tracing::warn!(
                "plugin '{plugin_id}' retire thread could not start ({e}) — waiting inline"
            );
            let outcome = wait_slot(&slot);
            Retiring {
                handle: None,
                done: Some(outcome),
                started,
                respawn: false,
                generation: None,
                waiters: Vec::new(),
                settled_ms: None,
            }
        }
    }
}

impl PluginManager {
    /// 종료를 요청하고 회수를 별도 스레드에 맡긴다. respawn이면 회수 뒤 다시 실행한다.
    /// 스레드 생성에 실패하면 호출한 스레드에서 기다린다.
    pub(super) fn retire_process(&mut self, plugin_id: &str, proc: PluginProcess, respawn: bool) {
        let generation = proc.reply_binding();
        let pending = proc.begin_shutdown(Instant::now() + PLUGIN_SHUTDOWN_TIMEOUT);
        self.retire_pending(plugin_id, pending, respawn, generation);
    }

    /// 이미 만든 종료 핸들의 회수를 스레드에 맡긴다 — [`Self::retire_process`] 와, 연결이
    /// 끝내 안 와 요청 없이 kill 하는 자리(`manager::connect`)가 함께 쓴다.
    pub(super) fn retire_pending(
        &mut self,
        plugin_id: &str,
        pending: PendingShutdown,
        respawn: bool,
        generation: std::sync::Weak<()>,
    ) {
        let mut retiring = spawn_waiter(plugin_id, pending);
        retiring.respawn = respawn;
        retiring.generation = Some(generation);
        // 같은 id 가 이미 회수 중일 수는 없다 — 회수 중인 id 는 기동이 미뤄지므로
        // 새 프로세스가 없다. 그래도 덮어쓰면 옛 핸들을 잃으니 먼저 거둔다.
        if let Some(prev) = self.retiring.remove(plugin_id) {
            let outcome = self.settle_retired(prev);
            tracing::warn!(
                "plugin '{plugin_id}' was already retiring ({}) — joined before retiring again",
                outcome.as_str()
            );
        }
        self.retiring.insert(plugin_id.to_string(), retiring);
        if !self.timers.is_registered(PluginTick::Retire) {
            self.timers.every(
                PluginTick::Retire,
                CHILD_EXIT_POLL_INTERVAL,
                tasty_timer::Precision::Strict,
                Instant::now(),
            );
        }
    }

    /// `plugin_id` 가 회수 중이면 회수 뒤에 다시 띄우라고 적고 `true` 를 돌려준다.
    /// 기동 창구([`Self::start_plugin_internal`])가 겹침을 막으려고 부른다.
    pub(super) fn defer_start_until_retired(&mut self, plugin_id: &str) -> bool {
        match self.retiring.get_mut(plugin_id) {
            Some(r) => {
                r.respawn = true;
                tracing::info!(
                    "plugin '{plugin_id}' start deferred — the previous process is still exiting"
                );
                true
            }
            None => false,
        }
    }

    /// 회수 중인 `plugin_id` 를 다시 띄우지 않게 한다. disable 이 부른다.
    pub(super) fn cancel_respawn_after_retire(&mut self, plugin_id: &str) {
        if let Some(r) = self.retiring.get_mut(plugin_id) {
            r.respawn = false;
        }
    }

    /// 회수가 끝난 것을 거둔다 — 기다리지 않는다. `(plugin_id, 다시 띄우는가)` 를 끝난
    /// 것만 돌려준다. 남은 것이 없으면 [`PluginTick::Retire`] 를 내린다.
    pub(super) fn reap_finished_retirements(&mut self) -> Vec<(String, bool)> {
        let finished: Vec<String> = self
            .retiring
            .iter()
            .filter(|(_, r)| r.is_finished())
            .map(|(id, _)| id.clone())
            .collect();
        let mut out = Vec::with_capacity(finished.len());
        for id in finished {
            let Some(r) = self.retiring.remove(&id) else {
                continue;
            };
            let respawn = r.respawn;
            let ms = r
                .settled_ms
                .unwrap_or_else(|| r.started.elapsed().as_secs_f64() * 1000.0);
            let outcome = self.settle_retired(r);
            tracing::info!(
                plugin_id = id,
                ms,
                reason = outcome.as_str(),
                "plugin process retired"
            );
            out.push((id, respawn));
        }
        if self.retiring.is_empty() {
            self.timers.cancel(PluginTick::Retire);
        }
        out
    }

    /// 관측이 멈춘 동안(닫기 정리 중)에는 pump 와 [`PluginTick::Retire`] 가 돌지 않는다.
    /// 그 사이 끝난 회수를 거둬 세대를 기록하고 기다리던 surface 회수를 확정한다. 다시 띄우기는
    /// 새 실행이라 여기서 하지 않고, 결과를 남긴 항목으로 Retire 틱에 넘긴다.
    pub(super) fn settle_finished_retirements_in_place(&mut self) {
        let finished: Vec<String> = self
            .retiring
            .iter()
            .filter(|(_, r)| r.generation.is_some() && r.is_finished())
            .map(|(id, _)| id.clone())
            .collect();
        for id in finished {
            let Some(r) = self.retiring.remove(&id) else {
                continue;
            };
            let started = r.started;
            let respawn = r.respawn;
            let settled_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
            let outcome = self.settle_retired(r);
            self.retiring.insert(
                id,
                Retiring {
                    handle: None,
                    done: Some(outcome),
                    started,
                    respawn,
                    generation: None,
                    waiters: Vec::new(),
                    settled_ms,
                },
            );
        }
    }

    /// [`PluginTick::Retire`] 한 번 — 끝난 회수를 거두고, 다시 띄울 것을 띄운다.
    pub(super) fn poll_retiring(&mut self) {
        for (id, respawn) in self.reap_finished_retirements() {
            if respawn {
                self.start_if_still_wanted(&id);
            }
        }
    }

    /// 회수 뒤로 미뤄 둔 기동을 한다 — 그 사이 disable 되었거나 이미 떠 있거나 설치 목록에서
    /// 빠졌으면 아무것도 안 한다.
    pub(crate) fn start_if_still_wanted(&mut self, plugin_id: &str) {
        if self.config.is_disabled(plugin_id) || self.processes.contains_key(plugin_id) {
            return;
        }
        if let Some(pkg) = self
            .packages
            .iter()
            .find(|p| p.manifest.id == plugin_id)
            .cloned()
        {
            self.ensure_listener();
            self.start_plugin_internal(&pkg);
        }
    }

    /// `plugin_id` 가 회수 중인가 — 쓰기 전에 기다릴지 묻는 호출자가 쓴다.
    pub(crate) fn is_retiring(&self, plugin_id: &str) -> bool {
        self.retiring.contains_key(plugin_id)
    }

    /// File continuations also run when the ordinary Retire tick is paused for shutdown.
    pub(super) fn poll_file_retirement_in_place(&mut self, id: &str) -> bool {
        if self.retiring.get(id).is_some_and(Retiring::is_finished)
            && let Some(retiring) = self.retiring.remove(id)
        {
            let ms = retiring
                .settled_ms
                .unwrap_or_else(|| retiring.started.elapsed().as_secs_f64() * 1000.0);
            let outcome = self.settle_retired(retiring);
            tracing::info!(
                plugin_id = id,
                ms,
                reason = outcome.as_str(),
                "plugin process retired"
            );
            if self.retiring.is_empty() {
                self.timers.cancel(PluginTick::Retire);
            }
        }
        !self.is_retiring(id)
    }

    /// Transfer a pending restart to an ordered file operation without waiting.
    pub fn take_file_restart(&mut self, plugin_id: &str) -> bool {
        let restart = self.retiring.get(plugin_id).is_some_and(|r| r.respawn);
        self.cancel_respawn_after_retire(plugin_id);
        restart
    }

    /// 회수가 끝날 때까지 기다린다. 파일 삭제·교체나 명시적 enable에 사용한다.
    /// true이면 재시작 예약도 가져온 것이므로, 쓰기 뒤 start_if_still_wanted로 이어야 한다.
    #[must_use = "true means a restart was pending on this retirement and is now yours to carry \
                  out (start_if_still_wanted after the write); dropping it leaves an enabled plugin \
                  down"]
    pub fn wait_retired(&mut self, plugin_id: &str) -> bool {
        let mut respawn = false;
        if let Some(r) = self.retiring.remove(plugin_id) {
            respawn = r.respawn;
            let outcome = self.settle_retired(r);
            tracing::info!(
                plugin_id,
                reason = outcome.as_str(),
                respawn,
                "plugin process retired (waited)"
            );
        }
        if self.retiring.is_empty() {
            self.timers.cancel(PluginTick::Retire);
        }
        respawn
    }

    /// 호스트 종료 — 회수 중인 것은 다시 띄우지 않고, 전부 끝났는지만 본다. 기다리지
    /// 않는다. 끝난 것마다 종료 계측(`S4a`)과 같은 줄을 남긴다.
    pub(super) fn poll_retiring_for_exit(&mut self) -> bool {
        for r in self.retiring.values_mut() {
            r.respawn = false;
        }
        let started: Vec<(String, f64)> = self
            .retiring
            .iter()
            .filter(|(_, r)| r.is_finished())
            .map(|(id, r)| (id.clone(), r.started.elapsed().as_secs_f64() * 1000.0))
            .collect();
        for (id, ms) in started {
            if let Some(r) = self.retiring.remove(&id) {
                let outcome = self.settle_retired(r);
                tracing::info!(
                    target: "tasty::shutdown",
                    ms,
                    plugin_id = id,
                    reason = outcome.as_str(),
                    "S4a plugin_shutdown_one (graceful deadline 2s, retiring before exit)"
                );
            }
        }
        if self.retiring.is_empty() {
            self.timers.cancel(PluginTick::Retire);
            true
        } else {
            false
        }
    }

    /// 회수가 끝난 세대를 기록하고 그 세대를 기다리던 surface 회수를 성공으로 확정한다.
    /// 프로세스가 끝났으므로 그 안의 surface 인스턴스도 남아 있지 않다.
    fn settle_retired(&mut self, mut r: Retiring) -> ShutdownOutcome {
        let generation = r.generation.take();
        let waiters = std::mem::take(&mut r.waiters);
        let outcome = r.join();
        if let Some(generation) = generation {
            self.note_reaped_generation(generation);
        }
        for waiter in waiters {
            waiter.finish(Ok(()));
        }
        outcome
    }

    /// 회수를 끝낸 프로세스의 세대를 기록한다. 기록하기 전에 어떤 surface 등록이나 mesh
    /// bootstrap 도 가리키지 않는 세대를 지운다. 가리키는 세대는 남겨 할당을 붙잡는다 —
    /// 주소가 재사용되면 `ptr_eq` 가 새 프로세스와 옛 세대를 혼동한다.
    pub(super) fn note_reaped_generation(&mut self, generation: std::sync::Weak<()>) {
        self.prune_unreferenced_generations();
        self.reaped_generations.push(generation);
    }

    fn prune_unreferenced_generations(&mut self) {
        if self.reaped_generations.is_empty() {
            return;
        }
        let mut referenced: Vec<std::sync::Weak<()>> = self
            .surfaces
            .values()
            .filter_map(|entry| match &entry.publication {
                super::RemotePublication::Sent(binding) => Some(binding.clone()),
                super::RemotePublication::NeverSent => None,
            })
            .collect();
        referenced.extend(self.mesh_publications.live_generations());
        self.reaped_generations
            .retain(|generation| referenced.iter().any(|r| r.ptr_eq(generation)));
    }

    /// 원 프로세스가 없을 때 surface 회수를 그 세대의 회수로 확정한다. 회수가 끝났으면 바로
    /// 성공, 회수 중이면 끝날 때 성공으로 확정한다. 어느 세대인지 모르면 receipt 를 돌려준다.
    pub(super) fn settle_on_retired_generation(
        &mut self,
        plugin_id: &str,
        generation: &std::sync::Weak<()>,
        completion: RemoteRetirementCompletion,
    ) -> Option<RemoteRetirementCompletion> {
        if self.reaped_generations.iter().any(|g| g.ptr_eq(generation)) {
            completion.finish(Ok(()));
            return None;
        }
        match self
            .retiring
            .get_mut(plugin_id)
            .filter(|r| r.generation.as_ref().is_some_and(|g| g.ptr_eq(generation)))
        {
            Some(r) => {
                r.waiters.push(completion);
                None
            }
            None => Some(completion),
        }
    }

    /// 이미 보낸 파괴 요청의 응답을 더 받을 수 없을 때 부른다(원 프로세스 회수, 채널 끊김,
    /// 플러그인 정리). 그 세대가 회수 중이거나 회수됐으면 회수를 소멸 증거로 확정하고, 아니면
    /// `reason` 으로 실패시킨다 — 끊긴 연결만으로는 플러그인이 surface 를 없앴다고 볼 수 없다.
    pub(super) fn settle_lost_retire_ack(
        &mut self,
        plugin_id: &str,
        process_binding: &std::sync::Weak<()>,
        completion: RemoteRetirementCompletion,
        reason: String,
    ) {
        if let Some(completion) =
            self.settle_on_retired_generation(plugin_id, process_binding, completion)
        {
            completion.finish(Err(reason));
        }
    }

    /// 응답을 더 받을 수 없는 파괴 대기 요청 `id` 를 지우고 [`Self::settle_lost_retire_ack`] 로 확정한다.
    pub(super) fn settle_removed_retire_ack(&mut self, id: u64) {
        if let Some(super::PendingRequest {
            to,
            kind:
                super::PendingRequestKind::SurfaceRetire {
                    completion,
                    process_binding,
                },
            ..
        }) = self.pending_requests.remove(&id)
        {
            self.settle_lost_retire_ack(
                &to,
                &process_binding,
                completion,
                "remote retirement acknowledgement was lost".into(),
            );
        }
    }

    /// 이 세대가 회수 중이거나 회수를 마쳤는가.
    pub(super) fn is_retired_generation(
        &self,
        plugin_id: &str,
        generation: &std::sync::Weak<()>,
    ) -> bool {
        self.reaped_generations.iter().any(|g| g.ptr_eq(generation))
            || self
                .retiring
                .get(plugin_id)
                .is_some_and(|r| r.generation.as_ref().is_some_and(|g| g.ptr_eq(generation)))
    }

    /// 헬스체크의 무응답 재시작 한 번 — 시험 전용(다른 모듈의 시험이 부른다).
    #[cfg(all(test, unix))]
    pub(crate) fn restart_unresponsive_for_test(&mut self) {
        self.restart_unresponsive_plugins(Instant::now());
    }

    /// 지금 회수 중인 plugin 수 — 시험 전용.
    #[cfg(test)]
    pub(crate) fn retiring_count(&self) -> usize {
        self.retiring.len()
    }

    /// 회수 중인 plugin 의 회수가 끝났는가 — 시험 전용.
    #[cfg(test)]
    pub(crate) fn retiring_finished(&self, plugin_id: &str) -> Option<bool> {
        self.retiring.get(plugin_id).map(Retiring::is_finished)
    }

    /// 회수 중인 plugin 이 다시 뜰 예정인가 — 시험 전용.
    #[cfg(test)]
    pub(crate) fn retiring_respawn(&self, plugin_id: &str) -> Option<bool> {
        self.retiring.get(plugin_id).map(|r| r.respawn)
    }
}
