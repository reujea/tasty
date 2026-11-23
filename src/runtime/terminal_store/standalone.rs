//! Standalone policy on the existing resource collection; no second resource registry.
use super::TerminalStore;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tasty_terminal::Pty;

/// 끝난 항목의 종료 결과를 보존하는 기간. 종료를 확인한 시각과 마지막 API 접근 중 늦은 쪽부터 센다.
pub(crate) const EXITED_RETENTION: Duration = Duration::from_secs(300);
/// 회수 기록은 오래된 것부터 버린다. 넘친 ID는 처음부터 없던 ID와 같은 응답을 받는다.
pub(crate) const RECLAIM_RECORD_LIMIT: usize = 256;
pub(crate) const PTY_ID_BASE: u32 = 0x8000_0000;
pub(crate) const fn is_surface_id_space(id: u32) -> bool {
    id < PTY_ID_BASE
}

/// 동시 개수는 거절 사유가 아니다. ID 무결성을 지킬 수 없을 때만 생성을 거절한다.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PtySpawnError {
    IdExhausted,
}
impl std::fmt::Display for PtySpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdExhausted => f.write_str("headless PTY ID space exhausted"),
        }
    }
}
impl std::error::Error for PtySpawnError {}

/// Tasty가 스스로 항목을 지운 이유. 호출자가 지운 `pty.kill`은 여기에 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PtyReclaimReason {
    /// 호출자가 spawn에서 고른 유휴 제한이 지나 실행 중인 프로세스를 끝냈다.
    IdleTimeout { timeout: Duration },
    /// 이미 끝난 항목의 보존 기간이 지나 종료 결과를 지웠다.
    ExitedRetention {
        exit_code: Option<i32>,
        retention: Duration,
    },
}

impl PtyReclaimReason {
    /// 회수된 ID로 온 `pty.*` 호출에 돌려주는 문장. 처음부터 없던 ID의 응답과 구별된다.
    pub(crate) fn describe(&self, id: u32) -> String {
        match self {
            Self::IdleTimeout { timeout } => format!(
                "headless pty {id} was reclaimed by Tasty: the idle timeout of {}s set at spawn \
                 passed without pty.read/write/wait, so its process was terminated",
                timeout.as_secs()
            ),
            Self::ExitedRetention {
                exit_code,
                retention,
            } => {
                let code = exit_code.map_or_else(|| "unknown".to_string(), |c| c.to_string());
                format!(
                    "headless pty {id} was removed by Tasty: it had exited (exit code {code}) \
                     and its result was kept for {}s",
                    retention.as_secs()
                )
            }
        }
    }
}

impl TerminalStore {
    pub(crate) fn reserve_pty_id(&self) -> Result<u32, PtySpawnError> {
        self.next_pty_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| PtySpawnError::IdExhausted)
    }
    pub(crate) fn is_standalone(&self, id: u32) -> bool {
        self.standalone(id).is_some()
    }
    pub(crate) fn standalone(&self, id: u32) -> Option<&Pty> {
        self.pty(id)
            .filter(|pty| pty.state().standalone().is_some())
    }
    pub(crate) fn standalone_iter(&self) -> impl Iterator<Item = (u32, &Pty)> {
        self.terminals.iter().filter_map(|(&id, (_, pty))| {
            pty.as_ref()
                .filter(|pty| pty.state().standalone().is_some())
                .map(|pty| (id, pty))
        })
    }
    pub(crate) fn touch_standalone(&mut self, id: u32, now: Instant) {
        if let Some(pty) = self.pty_mut(id) {
            pty.touch(now);
        }
    }
    /// 회수할 항목과 이유를 고른다. 실행 중인 항목은 호출자가 유휴 제한을 골랐을 때만 고른다.
    /// 끝난 항목은 처음 확인한 시각을 여기서 기록하므로 주기 정리의 간격만큼 늦게 셀 수 있다.
    pub(crate) fn due_standalone_reclaims(&mut self, now: Instant) -> Vec<(u32, PtyReclaimReason)> {
        let retention = self.exited_retention;
        let mut due = Vec::new();
        for (&id, (_, pty)) in &mut self.terminals {
            let Some(pty) = pty.as_mut() else { continue };
            if pty.state().standalone().is_none() {
                continue;
            }
            pty.observe_exit();
            let exit = pty.state().exit();
            let meta = pty.standalone_mut().expect("standalone checked above");
            let reason = match exit {
                Some(exit) => {
                    let exited_at = *meta.exited_at.get_or_insert(now);
                    let since = exited_at.max(meta.last_activity);
                    (now.saturating_duration_since(since) >= retention).then_some(
                        PtyReclaimReason::ExitedRetention {
                            exit_code: exit.code,
                            retention,
                        },
                    )
                }
                None => meta.idle_timeout.and_then(|timeout| {
                    (now.saturating_duration_since(meta.last_activity) >= timeout)
                        .then_some(PtyReclaimReason::IdleTimeout { timeout })
                }),
            };
            if let Some(reason) = reason {
                due.push((id, reason));
            }
        }
        due.sort_unstable_by_key(|(id, _)| *id);
        due
    }
    pub(crate) fn record_reclaim(&mut self, id: u32, reason: PtyReclaimReason) {
        if self.reclaimed.len() >= RECLAIM_RECORD_LIMIT {
            self.reclaimed.pop_front();
        }
        self.reclaimed.push_back((id, reason));
    }
    /// 살아 있는 항목이나 회수 기록이 있어 이 저장소가 그 PTY ID에 답할 수 있는지.
    /// 요청 라우팅이 이 판정으로 소유 engine을 골라야 회수 이유가 핸들러 응답까지 간다.
    pub(crate) fn answers_for_standalone(&self, id: u32) -> bool {
        self.is_standalone(id) || self.reclaim_record(id).is_some()
    }
    pub(crate) fn reclaim_record(&self, id: u32) -> Option<PtyReclaimReason> {
        self.reclaimed
            .iter()
            .rev()
            .find(|(reclaimed, _)| *reclaimed == id)
            .map(|(_, reason)| *reason)
    }
    #[cfg(test)]
    pub(crate) fn set_exited_retention(&mut self, retention: Duration) {
        self.exited_retention = retention;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicU32};
    #[cfg(unix)]
    use tasty_utils::test_wait::holds_by_every;

    #[test]
    fn shared_ids_keep_the_standalone_range_and_refuse_wraparound() {
        let counter = Arc::new(AtomicU32::new(PTY_ID_BASE));
        let first = TerminalStore::new(Arc::clone(&counter));
        let second = TerminalStore::new(counter);
        assert_eq!(first.reserve_pty_id().unwrap(), PTY_ID_BASE);
        assert_eq!(second.reserve_pty_id().unwrap(), PTY_ID_BASE + 1);
        assert!(is_surface_id_space(PTY_ID_BASE - 1));
        assert!(!is_surface_id_space(PTY_ID_BASE));
        let exhausted = TerminalStore::new(Arc::new(AtomicU32::new(u32::MAX)));
        assert_eq!(exhausted.reserve_pty_id(), Err(PtySpawnError::IdExhausted));
        assert_eq!(exhausted.reserve_pty_id(), Err(PtySpawnError::IdExhausted));
    }

    #[cfg(unix)]
    fn add(store: &mut TerminalStore, now: Instant, command: &str) -> u32 {
        add_limited(store, now, command, None)
    }

    #[cfg(unix)]
    fn add_limited(
        store: &mut TerminalStore,
        now: Instant,
        command: &str,
        idle_timeout: Option<Duration>,
    ) -> u32 {
        let id = store.reserve_pty_id().unwrap();
        let (terminal, mut pty) = tasty_terminal::spawn_terminal(
            tasty_terminal::TerminalConfig {
                cols: 80,
                rows: 24,
                shell: Some("/bin/sh"),
                args: &["-c", command],
                surface_id: id,
                working_dir: None,
                initial_input: None,
                extra_env: &[],
            },
            Arc::new(|| {}),
        )
        .unwrap();
        pty.set_standalone(tasty_terminal::StandalonePty {
            owner_agent_id: "owner".into(),
            cwd: None,
            command: vec![command.into()],
            created_at: now,
            last_activity: now,
            idle_timeout,
            exited_at: None,
        });
        store.insert(id, terminal, Some(pty));
        id
    }

    #[cfg(unix)]
    #[test]
    fn exited_entries_do_not_block_new_entries_beyond_the_former_limit() {
        let _home = crate::test_support::IsolatedHome::new();
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        let now = Instant::now();
        let exited: Vec<u32> = (0..3).map(|_| add(&mut store, now, "exit 7")).collect();
        let deadline = Instant::now() + Duration::from_secs(10);
        for &id in &exited {
            assert!(
                exit_observed_by(&mut store, id, deadline),
                "actual child exit not observed"
            );
            let pty = store.standalone(id).unwrap();
            assert_eq!(pty.state().exit().unwrap().code, Some(7));
            assert_eq!(pty.state().standalone().unwrap().owner_agent_id, "owner");
        }
        let running: Vec<u32> = (0..9)
            .map(|_| add(&mut store, now, "exec sleep 60"))
            .collect();
        assert_eq!(store.standalone_iter().count(), 12);
        assert!(running.iter().all(|&id| store.is_standalone(id)));
        let next = store.reserve_pty_id().expect("no concurrency refusal");
        assert!(next > *running.last().unwrap());
        drop(store.remove(exited[0]));
        assert!(store.get(exited[0]).is_none() && store.pty(exited[0]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn adoption_transfer_and_rollback_keep_the_physical_owner() {
        let _home = crate::test_support::IsolatedHome::new();
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        let now = Instant::now();
        let limit = Some(Duration::from_secs(10));
        let first = add_limited(&mut store, now, "exec sleep 60", limit);
        let second = add_limited(&mut store, now, "exec sleep 60", limit);
        store.touch_standalone(second, now + Duration::from_secs(9));
        assert_eq!(
            ids(store.due_standalone_reclaims(now + Duration::from_secs(11))),
            vec![first]
        );
        let generation = store.generation(first).unwrap();
        let pid = store.pty(first).unwrap().process_id();
        assert!(
            store
                .take_standalone_for_adoption(first, generation.value() + 1)
                .is_none()
        );
        let (terminal, pty, persisted) = store
            .take_standalone_for_adoption(first, generation.value())
            .expect("original owner");
        assert!(store.get(first).is_none());
        store
            .restore_standalone_adoption(first, terminal, pty, persisted)
            .unwrap_or_else(|_| panic!("rollback must preserve the original owner"));
        assert_eq!(store.generation(first), Some(generation));
        assert_eq!(store.pty(first).unwrap().process_id(), pid);
        assert!(store.is_standalone(first));
        let (mut terminal, mut pty, _) = store
            .take_standalone_for_adoption(first, generation.value())
            .expect("retry adoption");
        pty.adopt();
        terminal.rewire_waker(Arc::new(|| {}));
        store.insert(5, terminal, Some(pty));
        assert!(!store.is_standalone(first) && !store.is_standalone(5));
        assert!(store.get(first).is_none() && store.pty(first).is_none());
        assert_eq!(store.generation(5), Some(generation));
        assert_eq!(store.pty(5).unwrap().process_id(), pid);
        assert_eq!(
            ids(store.due_standalone_reclaims(now + Duration::from_secs(30))),
            vec![second]
        );
    }

    #[cfg(unix)]
    fn ids(due: Vec<(u32, PtyReclaimReason)>) -> Vec<u32> {
        due.into_iter().map(|(id, _)| id).collect()
    }

    #[cfg(unix)]
    fn wait_exit(store: &mut TerminalStore, id: u32) {
        let deadline = Instant::now() + Duration::from_secs(10);
        assert!(
            exit_observed_by(store, id, deadline),
            "actual child exit not observed"
        );
    }

    /// 원래 순서(종료 읽기 → process_surface → 기한 검사 → 쉼)를 지킨다. process_surface 는 출력 처리와
    /// resize 반영까지 하는 진행이라 기한 뒤 관측 읽기에서는 부르지 않고, 끝난 자식 종료만 거두는
    /// observe_exit 뒤에 종료를 읽는다.
    #[cfg(unix)]
    fn exit_observed_by(store: &mut TerminalStore, id: u32, deadline: Instant) -> bool {
        holds_by_every(deadline, Duration::from_millis(5), |last| {
            if last && let Some(pty) = store.pty_mut(id) {
                pty.observe_exit();
            }
            if store.standalone(id).unwrap().state().exit().is_some() {
                return true;
            }
            if !last {
                store.process_surface(id);
            }
            false
        })
    }

    #[cfg(unix)]
    #[test]
    fn running_entries_without_an_opt_in_limit_are_never_due() {
        let _home = crate::test_support::IsolatedHome::new();
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        let now = Instant::now();
        let quiet = add(&mut store, now, "exec sleep 60");
        let chatty = add(&mut store, now, "while :; do echo tick; sleep 0.01; done");
        for secs in [299, 300, 301, 390, 86_400] {
            assert!(
                store
                    .due_standalone_reclaims(now + Duration::from_secs(secs))
                    .is_empty(),
                "running entry chosen for reclaim at {secs}s"
            );
        }
        assert!(store.is_standalone(quiet) && store.is_standalone(chatty));
        assert!(store.pty(quiet).unwrap().process_id().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn an_opt_in_idle_timeout_is_due_exactly_at_its_boundary() {
        let _home = crate::test_support::IsolatedHome::new();
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        let now = Instant::now();
        let timeout = Duration::from_secs(300);
        let limited = add_limited(&mut store, now, "exec sleep 60", Some(timeout));
        let unlimited = add(&mut store, now, "exec sleep 60");
        assert!(
            store
                .due_standalone_reclaims(now + Duration::from_secs(299))
                .is_empty()
        );
        let expected = vec![(limited, PtyReclaimReason::IdleTimeout { timeout })];
        assert_eq!(
            store.due_standalone_reclaims(now + Duration::from_secs(300)),
            expected
        );
        assert_eq!(
            store.due_standalone_reclaims(now + Duration::from_secs(301)),
            expected
        );
        store.touch_standalone(limited, now + Duration::from_secs(200));
        assert!(
            store
                .due_standalone_reclaims(now + Duration::from_secs(499))
                .is_empty(),
            "API access must restart the opted-in idle timer"
        );
        assert!(store.is_standalone(unlimited));
    }

    #[cfg(unix)]
    #[test]
    fn exited_entries_keep_their_result_for_the_retention_period() {
        let _home = crate::test_support::IsolatedHome::new();
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        let id = add_limited(
            &mut store,
            Instant::now(),
            "exit 7",
            Some(Duration::from_secs(1)),
        );
        wait_exit(&mut store, id);
        let seen = Instant::now();
        assert!(
            store.due_standalone_reclaims(seen).is_empty(),
            "an exited entry is never reclaimed by the idle timeout"
        );
        assert!(
            store
                .due_standalone_reclaims(seen + EXITED_RETENTION - Duration::from_secs(1))
                .is_empty()
        );
        assert_eq!(
            store.standalone(id).unwrap().state().exit().unwrap().code,
            Some(7)
        );
        assert_eq!(
            store.due_standalone_reclaims(seen + EXITED_RETENTION),
            vec![(
                id,
                PtyReclaimReason::ExitedRetention {
                    exit_code: Some(7),
                    retention: EXITED_RETENTION,
                }
            )]
        );
        store.touch_standalone(id, seen + Duration::from_secs(100));
        assert!(
            store
                .due_standalone_reclaims(seen + EXITED_RETENTION)
                .is_empty(),
            "API access after exit extends the retention"
        );
    }

    #[test]
    fn reclaim_records_are_bounded_and_keep_the_newest() {
        let mut store = TerminalStore::new(Arc::new(AtomicU32::new(PTY_ID_BASE)));
        let reason = PtyReclaimReason::IdleTimeout {
            timeout: Duration::from_secs(5),
        };
        let total = RECLAIM_RECORD_LIMIT as u32 + 10;
        for n in 0..total {
            store.record_reclaim(PTY_ID_BASE + n, reason);
        }
        assert_eq!(store.reclaimed.len(), RECLAIM_RECORD_LIMIT);
        assert_eq!(store.reclaim_record(PTY_ID_BASE), None);
        assert_eq!(store.reclaim_record(PTY_ID_BASE + 9), None);
        assert_eq!(store.reclaim_record(PTY_ID_BASE + 10), Some(reason));
        assert_eq!(store.reclaim_record(PTY_ID_BASE + total - 1), Some(reason));
    }

    #[test]
    fn reclaim_reasons_are_distinct_from_an_unknown_id() {
        let idle = PtyReclaimReason::IdleTimeout {
            timeout: Duration::from_secs(60),
        }
        .describe(7);
        let exited = PtyReclaimReason::ExitedRetention {
            exit_code: Some(7),
            retention: EXITED_RETENTION,
        }
        .describe(7);
        for text in [&idle, &exited] {
            assert!(!text.contains("not found"), "{text}");
        }
        assert!(idle.contains("idle timeout of 60s") && idle.contains("terminated"));
        assert!(exited.contains("exit code 7") && exited.contains("300s"));
    }
}
