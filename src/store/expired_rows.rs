//! memory.db의 정리되지 않은 만료 행 청소. 조회와 쓰기 판정은 만료 행을 이미 없는 것으로 보므로
//! 이 청소는 디스크와 quota 카운터에 남은 행을 지우고 지운 regular 키마다 `expired` 변경을 낸다.
//! 로그 정리 틱마다 기다리지 않는 writer 요청으로 실행한다.

use tasty_memory::MemoryStorage;

/// 만료 행을 지운다. 실패는 경고만 남기고 다음 틱에 다시 시도한다.
pub fn purge(mem: &mut dyn MemoryStorage) {
    let started = std::time::Instant::now();
    match mem.purge_expired() {
        Ok(stats) if stats.regular + stats.secret > 0 => tracing::debug!(
            regular = stats.regular,
            secret = stats.secret,
            elapsed_us = started.elapsed().as_micros() as u64,
            "purged expired memory rows"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!("expired memory purge failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use tasty_memory::{MemoryChangeKind, MemoryStore, MemoryValue, PutOpts, Scope};

    fn text(s: &str) -> MemoryValue {
        MemoryValue::Text(s.into())
    }

    /// 로그 정리 틱이 실제 writer 순서에서 만료 행을 지우고 `expired` 변경을 낸다. 로그 보존 정리의
    /// 주기와 관계없이 실행되는지 보려고 주기가 돌아오지 않는 시각(0)을 넘긴다.
    #[test]
    fn the_log_prune_tick_purges_expired_rows_on_the_writer() {
        let store = Arc::new(Mutex::new(MemoryStore::open_in_memory().expect("store")));
        {
            let mut s = store.lock().unwrap();
            let expired = PutOpts {
                expires_at: Some(1),
                cas: None,
            };
            s.put("com.tasty.a", &Scope::Global, "gone", &text("x"), &expired)
                .unwrap();
            s.put_secret("com.tasty.a", &Scope::Global, "gone", &text("y"), &expired)
                .unwrap();
            s.put(
                "com.tasty.a",
                &Scope::Global,
                "kept",
                &text("z"),
                &PutOpts::default(),
            )
            .unwrap();
            s.take_pending_changes();
        }
        let core = crate::adapters::ipc::handler::deferred::tests::writer_core_with(
            store.clone(),
            usize::MAX,
        );
        core.submit_periodic_prune(0);
        let deadline = Instant::now() + Duration::from_secs(10);
        // reap_detached 는 끝난 요청을 거둘 뿐이라 기한 뒤 마지막 읽기 앞에서도 부른다.
        assert!(
            tasty_utils::test_wait::holds_by_every(deadline, Duration::ZERO, |last| {
                if last {
                    core.memory_access().reap_detached();
                }
                core.memory_access().detached_len() == 0 || {
                    if !last {
                        std::thread::sleep(Duration::from_millis(10));
                        core.memory_access().reap_detached();
                    }
                    false
                }
            }),
            "정리 요청이 끝나지 않았다"
        );

        let mut s = store.lock().unwrap();
        let left = s.purge_expired().unwrap();
        assert_eq!((left.regular, left.secret), (0, 0), "만료 행이 남았다");
        assert!(s.get(&Scope::Global, "kept").unwrap().is_some());
        let changes: Vec<_> = core
            .memory_access()
            .changes()
            .take()
            .into_iter()
            .map(|c| (c.key, c.kind))
            .collect();
        assert_eq!(
            changes,
            vec![("gone".to_string(), MemoryChangeKind::Expired)]
        );
    }
}
