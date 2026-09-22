//! `FileFormatRegistry` — cleanup 도메인.

use super::FileFormatRegistry;
use crate::types::RuleOrigin;

impl FileFormatRegistry {
    pub fn uninstall_plugin(&self, plugin_id: &str) {
        let mut inner = self.lock_write();
        let mut empty_ids = Vec::new();
        for (id, contribs) in inner.contributions.iter_mut() {
            contribs.retain(|c| !matches!(&c.origin, RuleOrigin::Plugin(p) if p == plugin_id));
            if contribs.is_empty() {
                empty_ids.push(id.clone());
            }
        }
        for id in empty_ids {
            inner.contributions.remove(&id);
            inner.install_order.remove(&id);
        }
        inner.dirty = true;
    }
}
