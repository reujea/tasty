//! 서버는 attach 점유 중인 터미널의 표시용 사본을 주기적으로 갱신한다.
//! PTY를 가진 원본은 유지하고 GUI는 이 사본을 읽기 전용으로 그린다. 갱신 주기는 호출부 타이머가 정한다.

use crate::runtime::engine_access::EngineMut;
use tasty_terminal::Terminal;

impl EngineMut<'_> {
    /// 점유 터미널의 화면을 사본에 적용하고 점유가 끝난 사본은 지운다.
    /// snapshot의 clear·home으로 이전 화면을 덮으며 크기가 바뀌면 사본을 다시 만든다.
    /// 원본의 내용 기준점(ContentCut)이 사본을 만들 때와 같으면 직렬화와 재해석을 건너뛴다.
    /// 스트림 회차마다 불리므로 바뀐 것이 없을 때의 비용은 잠금 한 번과 비교 한 번이어야 한다.
    /// 터미널 사본을 하나라도 갱신했으면 true다.
    pub(crate) fn refresh_readonly_views(&mut self) -> bool {
        let attached: Vec<u32> = self
            .live
            .occupancy
            .locks_snapshot()
            .into_iter()
            .map(|(sid, _)| sid)
            .collect();

        self.runtime
            .readonly_views
            .retain(|sid, _| attached.contains(sid));
        self.runtime
            .readonly_sources
            .retain(|sid, _| attached.contains(sid));

        let mut any = false;
        for sid in attached {
            let Some(live) = self.runtime.terminals.get(sid) else {
                continue;
            };
            let current = live.with_content(|view| view.cut());
            if self.runtime.readonly_views.contains_key(&sid)
                && self.runtime.readonly_sources.get(&sid) == Some(&current)
            {
                continue;
            }
            let (cols, rows, snapshot, cut) = live
                .with_content(|view| (view.cols(), view.rows(), view.snapshot_as_vt(), view.cut()));
            let mirror = self
                .runtime
                .readonly_views
                .entry(sid)
                .or_insert_with(|| Terminal::new_detached(cols, rows));
            if mirror.cols() != cols || mirror.rows() != rows {
                *mirror = Terminal::new_detached(cols, rows);
            }
            mirror.feed_bytes(&snapshot);
            self.runtime.readonly_sources.insert(sid, cut);
            any = true;
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use tasty_terminal::Terminal;

    #[test]
    fn copy_is_rebuilt_only_when_the_source_changes() {
        let mut session = crate::state::tests::test_state().1;
        let sid = session
            .core_state
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        session
            .runtime
            .terminals
            .insert(sid, Terminal::new_detached(40, 5), None);
        let mut e = session.borrow_mut();
        e.live.occupancy.acquire(sid, 7).expect("lock");

        assert!(
            e.refresh_readonly_views(),
            "the first refresh builds the copy"
        );
        assert!(
            !e.refresh_readonly_views(),
            "an unchanged source must not be serialized again"
        );

        e.runtime
            .terminals
            .get_mut(sid)
            .expect("terminal")
            .feed_bytes(b"hello");
        assert!(e.refresh_readonly_views(), "new output rebuilds the copy");
        assert!(
            e.runtime.readonly_views[&sid]
                .screen_text(false)
                .contains("hello")
        );
        assert!(!e.refresh_readonly_views());
    }

    #[test]
    fn released_occupancy_drops_the_copy_and_its_source_mark() {
        let mut session = crate::state::tests::test_state().1;
        let sid = session
            .core_state
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        session
            .runtime
            .terminals
            .insert(sid, Terminal::new_detached(40, 5), None);
        let mut e = session.borrow_mut();
        e.live.occupancy.acquire(sid, 7).expect("lock");
        assert!(e.refresh_readonly_views());

        e.live.occupancy.release_all_for_client(7);
        assert!(!e.refresh_readonly_views());
        assert!(e.runtime.readonly_views.is_empty());
        assert!(e.runtime.readonly_sources.is_empty());

        e.live.occupancy.acquire(sid, 8).expect("lock");
        assert!(
            e.refresh_readonly_views(),
            "a new holder gets a fresh copy even though the source did not change"
        );
    }
}
