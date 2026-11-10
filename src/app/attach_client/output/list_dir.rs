//! 원격 폴더 목록 응답(`list_dir_result`)을 기다리던 explorer·주소창·파일 선택기에 반영한다.

use super::{MirrorEvent, MirrorHost, MirrorResultEnvelope, remote_entry};
use tasty_remote::client_session::AttachClientSession;

pub(super) fn apply(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_, '_>,
    ev: MirrorEvent,
) {
    let MirrorEvent::ListDirResult {
        request_id,
        ok,
        dir,
        entries,
        truncated,
        reason,
        volume,
    } = ev
    else {
        return;
    };
    let entries = entries.map(|entries| {
        entries
            .into_iter()
            .map(|entry| remote_entry::from_remote(dir.as_deref(), entry))
            .collect()
    });
    apply_result(
        sess,
        host,
        MirrorResultEnvelope {
            request_id,
            ok,
            truncated,
            reason,
        },
        dir,
        entries,
        volume,
    );
}

fn apply_result(
    sess: &mut AttachClientSession,
    host: &mut MirrorHost<'_, '_>,
    envelope: MirrorResultEnvelope,
    dir: Option<String>,
    entries: Option<Vec<crate::core::fs_list::DirEntryInfo>>,
    volume: Option<u64>,
) {
    let MirrorResultEnvelope {
        request_id,
        ok,
        truncated,
        reason,
    } = envelope;
    let consumer = sess
        .state
        .pending_list_dir_consumers
        .remove(&request_id)
        .flatten();
    if let Some(surface_id) = consumer {
        let result = if ok {
            Ok((entries.unwrap_or_default(), volume))
        } else {
            Err(reason.unwrap_or_default())
        };
        if let Some(panel) = host
            .engine
            .find_surface_by_id(surface_id)
            .and_then(|s| s.as_any().downcast_ref::<crate::model::ExplorerPanel>())
        {
            let is_err = result.is_err();
            host.state
                .explorer_views
                .apply_remote_list_dir_result(surface_id, request_id, panel, result);
            if !is_err && truncated {
                host.toast(
                    crate::i18n::t("explorer.state.remote_listing_truncated").to_string(),
                    crate::adapters::ui::ToastKind::Warning,
                );
            }
        }
        return;
    }
    let home = if ok { dir.as_deref() } else { None };
    if host
        .state
        .explorer_views
        .finish_home_probe(request_id, home)
    {
        return;
    }
    // 아직 열린 picker가 같은 요청을 기다릴 때만 반영한다.
    let Some(picker) = host.state.dialogs.file_picker.as_mut() else {
        return;
    };
    let is_pending = matches!(
        &picker.load,
        crate::state::FpLoadState::Loading { request_id: rid, .. } if *rid == request_id
    );
    if !is_pending {
        return;
    }
    if ok {
        let es = entries.unwrap_or_default();
        if let Some(d) = dir {
            picker.current_dir = d;
        }
        picker.load = if es.is_empty() {
            crate::state::FpLoadState::Empty
        } else {
            crate::state::FpLoadState::Loaded
        };
        picker.entries = es;
        if picker.remote_host.is_none() {
            picker.remote_host = Some(sess.state.host_text());
        }
        if truncated {
            host.toast(
                crate::i18n::t("filepicker.remote_listing_truncated").to_string(),
                crate::adapters::ui::ToastKind::Warning,
            );
        }
    } else {
        let reason_str = reason.unwrap_or_default();
        picker.load = if reason_str == "permission denied" {
            crate::state::FpLoadState::ErrorPerm(reason_str)
        } else {
            crate::state::FpLoadState::ErrorConn(reason_str)
        };
    }
}
