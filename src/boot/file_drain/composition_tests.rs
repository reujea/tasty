//! Actual shipping entry points with shared C1, W and L1 consumers.
use crate::app::{
    App, plugin_file_lifecycle::shutdown_tests as cases, preset_shutdown_tests::make_headless,
};
use crate::runtime::engine_session::EngineSession;

pub(in crate::boot) fn engine(app: &App) -> EngineSession {
    cases::shared_engine(app)
}

#[test]
fn shipping_pipeline_collects_child_webhook_and_plugin_original_replies() {
    cases::mixed_child_webhook_and_plugin_pipeline(make_headless, |app, mut session| {
        let drain = super::begin(app, &mut session);
        super::drain_engine(app, &mut session, drain);
    });
}

#[test]
fn shipping_pipeline_collects_ready_receipts_before_l1_continuation() {
    cases::ready_receipts_precede_l1_continuation(make_headless, |app, mut session| {
        let drain = super::begin(app, &mut session);
        super::drain_engine(app, &mut session, drain);
    });
}

#[test]
fn shipping_expired_file_drain_retains_unknown_and_replies_once() {
    cases::active_unknown_and_queued_refusal(make_headless, |app, drain| {
        let mut session = engine(app);
        super::drain_engine(app, &mut session, drain);
    });
}
