//! HTML source and received-entry state owned by one plugin surface.
use super::*;

impl RemoteSurface {
    /// 스크립트 허용 상태를 잠그고 `f`를 실행한다.
    #[cfg(feature = "gui")]
    pub fn with_html_script<R>(
        &self,
        f: impl FnOnce(&mut tasty_model::html_script::HtmlScriptState) -> R,
    ) -> R {
        f(&mut crate::poison::recover_mutex(
            self.html_script.lock(),
            HTML_SCRIPT_WHAT,
            &HTML_SCRIPT_POISON_REPORTED,
        ))
    }

    #[cfg(feature = "gui")]
    pub(crate) fn lock_html_mirror(
        &self,
    ) -> Option<std::sync::MutexGuard<'_, tasty_remote::html_mirror::HtmlMirror>> {
        self.html_mirror.as_ref().map(|mirror| {
            crate::poison::recover_mutex(mirror.lock(), "remote HTML mirror", &HTML_SOURCE_POISONED)
        })
    }

    pub fn set_html_source(&self, raw: Option<&str>) {
        if !self.capabilities.address_band {
            return;
        }
        let cwd = self.source_cwd();
        crate::poison::recover_mutex(
            self.html_source.lock(),
            "HTML source",
            &HTML_SOURCE_POISONED,
        )
        .set(raw, cwd.as_deref());
    }

    /// Snapshot fallback is only initialized before a requested URL exists.
    pub fn html_source_info(&self) -> (tasty_ipc::html_transfer::HtmlSource, u64) {
        let mut state = crate::poison::recover_mutex(
            self.html_source.lock(),
            "HTML source",
            &HTML_SOURCE_POISONED,
        );
        if state.source.is_none() {
            let snapshot = crate::poison::recover_mutex(
                self.snapshot_cache.lock(),
                SNAPSHOT_WHAT,
                &SNAPSHOT_POISON_REPORTED,
            );
            let cwd = crate::poison::recover_mutex(self.cwd.lock(), CWD_WHAT, &CWD_POISON_REPORTED);
            state.set(
                snapshot
                    .as_ref()
                    .and_then(|v| v.get("url"))
                    .and_then(Value::as_str),
                cwd.as_deref(),
            );
        }
        (
            state
                .source
                .clone()
                .unwrap_or(tasty_ipc::html_transfer::HtmlSource::Unsupported),
            state.generation,
        )
    }
}
