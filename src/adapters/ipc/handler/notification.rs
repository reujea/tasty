use serde_json::json;

use crate::i18n::t;
use tasty_ipc::protocol::JsonRpcResponse;

pub fn handle_notification_list(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    id: serde_json::Value,
) -> JsonRpcResponse {
    let notifications: Vec<_> = engine
        .live
        .notifications
        .all()
        .rev()
        .map(|n| {
            json!({
                "id": n.id,
                "level": n.level,
                "title": n.title,
                "body": n.body,
                "workspace_id": n.source_workspace,
                "surface_id": n.source_surface,
                "read": n.read,
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!(latest_notifications(notifications)))
}

/// Preserve the single-engine contract: newest creation first, at most 50.
/// Coalescing updates an entry in place without assigning it a new creation ID.
/// Apply the same cap after merging engines, never 50 times the window count.
pub(crate) fn latest_notifications(mut rows: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    rows.sort_by_key(|row| std::cmp::Reverse(row["id"].as_u64()));
    rows.truncate(50);
    rows
}

pub fn handle_notification_create(
    out: &mut crate::ipc::window_port::IntentOutbox,
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    id: serde_json::Value,
    params: &serde_json::Value,
) -> JsonRpcResponse {
    let level = match params.get("level") {
        None => crate::notification::NotificationLevel::Info,
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(level) => level,
            Err(_) => {
                return JsonRpcResponse::invalid_params(
                    id,
                    "level must be error, warning, or info",
                );
            }
        },
    };
    let title = params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(t("notification.default_title"))
        .to_string();
    let body = params
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let surface_id = match super::params::optional_u32(params, "surface_id", &id) {
        Ok(v) => v.unwrap_or(0),
        Err(e) => return e,
    };

    // 사용자 포커스 대신 명시 ID의 소속으로 대상을 정한다.
    let ws_param = match super::params::optional_u32(params, "workspace_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let ws_id = match ws_param {
        Some(v) => v,
        None => {
            if surface_id > 0 {
                if let Some((idx, _)) = engine.find_workspace_index_for_surface(surface_id) {
                    engine
                        .workspace_at(idx)
                        .expect("workspace index is valid")
                        .id
                } else {
                    return JsonRpcResponse::invalid_params(
                        id,
                        format!(
                            "surface_id {} not found; cannot determine workspace",
                            surface_id
                        ),
                    );
                }
            // workspace가 하나뿐이면 호환 동작으로 그 대상을 쓴다.
            } else if engine.workspaces().len() == 1 {
                tracing::warn!(
                    "notification.create called without 'workspace_id' or 'surface_id'; \
                     auto-routing to the only workspace."
                );
                engine.workspace_at(0).expect("workspace index is valid").id
            } else {
                return JsonRpcResponse::invalid_params(
                    id,
                    "Missing required 'workspace_id' parameter \
                     (focus-independent IPC requires explicit workspace_id, or surface_id \
                     to route by ownership)",
                );
            }
        }
    };
    if !engine.has_workspace(ws_id) {
        return JsonRpcResponse::invalid_params(id, format!("workspace_id {ws_id} not found"));
    }
    if surface_id > 0
        && engine
            .find_workspace_index_for_surface(surface_id)
            .is_none_or(|(idx, _)| engine.workspace_at(idx).is_none_or(|ws| ws.id != ws_id))
    {
        return JsonRpcResponse::invalid_params(id, "surface_id does not belong to workspace_id");
    }
    // 저장과 이벤트 통지는 후속 intent 처리에 맡긴다.
    out.push(
        crate::app::command::DomainIntent::PushNotification {
            level,
            ws_id,
            surface_id,
            title,
            body,
            source: "host".to_string(),
            merge: crate::app::command::NotificationMerge::Coalesce,
            terminal_event: false,
        }
        .from_agent_ipc(),
    );
    JsonRpcResponse::success(id, json!({ "created": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_validates_level_and_explicit_ownership_before_enqueueing() {
        let (_, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let ws = engine.workspace_at(0).unwrap().id;
        let sid = engine.workspace_at(0).unwrap().all_surface_ids()[0];
        for params in [
            json!({"workspace_id":ws,"level":"fatal"}),
            json!({"workspace_id":ws,"level":null}),
            json!({"workspace_id":u32::MAX}),
            json!({"workspace_id":ws,"surface_id":u32::MAX}),
        ] {
            let mut out = crate::ipc::window_port::IntentOutbox::default();
            assert!(
                handle_notification_create(&mut out, &mut engine, json!(1), &params)
                    .error
                    .is_some()
            );
            assert!(out.is_empty());
        }
        for (level, expected) in [
            (None, crate::notification::NotificationLevel::Info),
            (Some("error"), crate::notification::NotificationLevel::Error),
            (
                Some("warning"),
                crate::notification::NotificationLevel::Warning,
            ),
        ] {
            let mut params = json!({"surface_id":sid});
            if let Some(level) = level {
                params["level"] = json!(level);
            }
            let mut out = crate::ipc::window_port::IntentOutbox::default();
            assert!(
                handle_notification_create(&mut out, &mut engine, json!(2), &params)
                    .error
                    .is_none()
            );
            assert!(
                matches!(&out.into_vec()[0].body, crate::intent::Intent::Domain(crate::app::command::DomainIntent::PushNotification { level, ws_id, .. }) if *level==expected && *ws_id==ws)
            );
        }
    }

    #[test]
    fn notification_merge_has_one_global_limit_and_matches_creation_order() {
        let (_state_a, mut a_session) = crate::state::tests::test_state();
        let a = a_session.borrow_mut();
        let (_state_b, mut b_session) = crate::state::tests::test_state();
        let b = b_session.borrow_mut();
        let ids = crate::runtime::counters::RuntimeCounters::new();
        a.live.notifications =
            crate::notification::NotificationStore::with_counter(0, ids.notification_counter());
        b.live.notifications =
            crate::notification::NotificationStore::with_counter(0, ids.notification_counter());
        for i in 0..120 {
            let store = if i % 2 == 0 {
                &mut a.live.notifications
            } else {
                &mut b.live.notifications
            };
            store.add(1, 1, format!("entry-{i}"), String::new());
        }
        let mut rows = Vec::new();
        for engine in [&b, &a] {
            rows.extend(
                handle_notification_list(&engine.as_ref(), json!(1))
                    .result
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned(),
            );
        }
        let merged = latest_notifications(rows);
        assert_eq!(merged.len(), 50);
        let titles: Vec<_> = merged
            .iter()
            .map(|n| n["title"].as_str().unwrap())
            .collect();
        let expected: Vec<_> = (70..120).rev().map(|i| format!("entry-{i}")).collect();
        assert_eq!(titles, expected);
        let ids: std::collections::BTreeSet<_> =
            merged.iter().map(|n| n["id"].as_u64().unwrap()).collect();
        assert_eq!(ids.len(), 50);
    }
}
