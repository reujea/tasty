//! 플러그인이 RemoteSurface에 URL을 설정하면 sync_webviews가 native WebView에 반영한다.

use super::params::require_u32;
use serde_json::Value;

use tasty_ipc::protocol::JsonRpcResponse;

/// 문서 변경을 attach 클라이언트에 알린다. 대상과 허용 종류는 attach_runtime이 검사한다.
#[cfg(feature = "gui")]
fn notify_content_changed(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    surface: &crate::plugin_bridge::remote_surface::RemoteSurface,
    sid: u32,
) {
    crate::remote::server::notify_markdown_changed(
        &engine.live.occupancy,
        engine.remote,
        surface.capabilities,
        sid,
    );
}

/// RemoteSurface에 URL과 페이지 작성자 정보를 기록한다.
/// 외부 호출도 URL을 설정할 수 있지만 소유 플러그인이 쓴 페이지의 클릭만
/// 파일 열기의 사용자 행동으로 인정한다(docs/features/file-handler/index.md).
pub fn handle_set_url(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let sid = match require_u32(params, "surface_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let url = match params.get("url").and_then(|v| v.as_str()) {
        Some(u) => u.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "missing 'url'"),
    };
    // 선택 인자. host chrome은 url 대신 이 이름을 보인다(raw HTML url의 원문 노출 방지).
    let label = params
        .get("label")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // 분할 탭의 비포커스 surface도 찾도록 레이아웃 전체를 순회한다.
    for ws in &engine.workspaces() {
        for &pid in &ws.pane_layout().all_pane_ids() {
            if let Some(pane) = ws.pane_layout().find_pane(pid) {
                for tab in &pane.tabs {
                    let Some(layout) = tab.layout_if_initialized() else {
                        continue;
                    };
                    let Some(surface) = layout
                        .find_surface(sid)
                        .and_then(|_| engine.find_surface_by_id(sid))
                    else {
                        continue;
                    };
                    if let Some(rs) = surface
                        .as_any()
                        .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>(
                    ) {
                        rs.set_webview_url(Some(url), label, is_owner(caller, rs));
                        #[cfg(feature = "gui")]
                        notify_content_changed(engine, rs, sid);
                        return JsonRpcResponse::success(id, serde_json::json!({ "ok": true }));
                    }
                    tracing::warn!(
                        "webview.set_url: surface {sid} is not a webview-enabled RemoteSurface"
                    );
                    return JsonRpcResponse::error(
                        id,
                        -32000,
                        "surface is not a webview-enabled RemoteSurface",
                    );
                }
            }
        }
    }
    tracing::warn!("webview.set_url: surface {sid} not found in any workspace layout");
    JsonRpcResponse::error(id, -32000, "surface_id not found")
}

fn is_owner(
    caller: &tasty_ipc::caller::CallerContext,
    rs: &crate::plugin_bridge::remote_surface::RemoteSurface,
) -> bool {
    matches!(
        caller,
        tasty_ipc::caller::CallerContext::Plugin { plugin_id, .. } if *plugin_id == rs.plugin_id
    )
}

#[cfg(all(test, feature = "gui"))]
mod tests {
    use super::*;
    use crate::model::SplitDirection;
    use serde_json::json;

    fn fixture(
        markdown_count: u32,
        sole_markdown: bool,
    ) -> (
        crate::state::RequestContext,
        crate::runtime::engine_session::EngineSession,
    ) {
        use tasty_core::{DomainEvent as E, Placement, Ratio, SplitSpec, SurfaceSpec};
        let mut events = vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::WorkspaceCreated {
                id: 1,
                name: "workspace".into(),
                category: 0,
                index: 0,
                pane: 1,
            },
            E::TabCreated {
                id: 1,
                pane: 1,
                index: 0,
                name: "tab".into(),
                surface: SurfaceSpec {
                    id: 1,
                    kind: if sole_markdown {
                        "markdown"
                    } else {
                        "terminal"
                    }
                    .into(),
                    data: None,
                },
            },
        ];
        for index in 0..markdown_count {
            events.push(E::SurfaceSplit {
                target: 1,
                surface: SurfaceSpec {
                    id: index + 2,
                    kind: "markdown".into(),
                    data: None,
                },
                split: SplitSpec {
                    direction: SplitDirection::Vertical,
                    ratio: Ratio::from_f32(0.5),
                    placement: Placement::After,
                },
            });
        }
        let (state, mut owner) =
            crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
        let ids: Vec<u32> = if sole_markdown {
            vec![1]
        } else {
            (2..markdown_count + 2).collect()
        };
        for sid in ids {
            owner.runtime.surfaces.insert(
                sid,
                Box::new(
                    crate::plugin_bridge::remote_surface::RemoteSurface::new(
                        sid,
                        "markdown",
                        "com.tasty.markdown".into(),
                        "markdown".into(),
                    )
                    .with_capabilities(
                        crate::runtime::surface_registry::KindCapabilities::undeclared(
                            "markdown",
                            "com.tasty.markdown",
                        ),
                    ),
                ),
            );
        }
        (state, owner)
    }

    fn focused_surface_id(
        state: &crate::state::RequestContext,
        engine: &crate::runtime::engine_access::EngineRef<'_>,
    ) -> u32 {
        let ws = engine
            .workspace_at(state.active_workspace_index(engine))
            .expect("workspace index is valid");
        let pane = ws
            .pane_layout()
            .find_pane(state.navigation.pane_id(ws).unwrap())
            .expect("focused pane");
        state
            .navigation
            .surface_id(&pane.tabs[state.navigation.tab_index(pane)])
            .unwrap()
    }

    fn set_url(engine: &crate::runtime::engine_access::EngineRef<'_>, sid: u32) -> JsonRpcResponse {
        set_url_as(engine, &tasty_ipc::caller::CallerContext::local(), sid)
    }

    fn set_url_as(
        engine: &crate::runtime::engine_access::EngineRef<'_>,
        caller: &tasty_ipc::caller::CallerContext,
        sid: u32,
    ) -> JsonRpcResponse {
        handle_set_url(
            engine,
            caller,
            json!(1),
            &json!({ "surface_id": sid, "url": "file:///tmp/doc.html" }),
        )
    }

    fn plugin_caller(plugin_id: &str) -> tasty_ipc::caller::CallerContext {
        tasty_ipc::caller::CallerContext::Plugin {
            plugin_id: plugin_id.to_string(),
        }
    }

    fn remote_surface<'a>(
        engine: &'a crate::runtime::engine_access::EngineRef<'_>,
        sid: u32,
    ) -> &'a crate::plugin_bridge::remote_surface::RemoteSurface {
        engine
            .runtime
            .surfaces
            .get(&sid)
            .and_then(|s| {
                s.as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            })
            .expect("remote surface")
    }

    #[test]
    fn set_url_remembers_whether_the_owning_plugin_wrote_the_page() {
        let (state, mut engine_session) = fixture(0, true);
        let engine = engine_session.borrow_mut();
        let md_sid = focused_surface_id(&state, &engine.as_ref());
        let owner = remote_surface(&engine.as_ref(), md_sid).plugin_id.clone();
        assert!(!remote_surface(&engine.as_ref(), md_sid).webview_page_by_owner());

        let written_by = |caller: &tasty_ipc::caller::CallerContext| {
            assert!(set_url_as(&engine.as_ref(), caller, md_sid).error.is_none());
            remote_surface(&engine.as_ref(), md_sid).webview_page_by_owner()
        };
        assert!(written_by(&plugin_caller(&owner)));
        assert!(!written_by(&tasty_ipc::caller::CallerContext::local()));
        assert!(written_by(&plugin_caller(&owner)));
        assert!(!written_by(&plugin_caller("com.example.other")));
    }

    // 외부 작성자에서 소유 플러그인으로 바뀐 기록은 host가 읽을 때까지 유지한다.
    #[test]
    fn set_url_marks_when_the_owning_plugin_takes_the_page_back() {
        let (state, mut engine_session) = fixture(0, true);
        let engine = engine_session.borrow_mut();
        let md_sid = focused_surface_id(&state, &engine.as_ref());
        let owner = plugin_caller(&remote_surface(&engine.as_ref(), md_sid).plugin_id.clone());
        let agent = tasty_ipc::caller::CallerContext::local();

        let took_over_after = |caller: &tasty_ipc::caller::CallerContext| {
            assert!(set_url_as(&engine.as_ref(), caller, md_sid).error.is_none());
            remote_surface(&engine.as_ref(), md_sid).take_webview_owner_takeover()
        };
        assert!(took_over_after(&owner), "처음 쓴 페이지도 전이다");
        assert!(!took_over_after(&owner));
        assert!(!took_over_after(&agent));
        assert!(took_over_after(&owner));
        assert!(
            !remote_surface(&engine.as_ref(), md_sid).take_webview_owner_takeover(),
            "전이 표시를 가져오면 초기화되어야 한다"
        );
        assert!(!took_over_after(&agent));
        assert!(set_url_as(&engine.as_ref(), &owner, md_sid).error.is_none());
        assert!(set_url_as(&engine.as_ref(), &owner, md_sid).error.is_none());
        assert!(
            remote_surface(&engine.as_ref(), md_sid).take_webview_owner_takeover(),
            "가져가기 전의 전이는 뒤에 쓴 것이 지우지 않는다"
        );
    }

    // label은 URL과 함께 바뀐다. 빼고 보내면 앞 문서의 이름이 남지 않는다.
    #[test]
    fn set_url_stores_the_label_and_clears_it_when_omitted() {
        let (state, mut engine_session) = fixture(0, true);
        let engine = engine_session.borrow_mut();
        let md_sid = focused_surface_id(&state, &engine.as_ref());
        let caller = tasty_ipc::caller::CallerContext::local();
        let send = |params: serde_json::Value| {
            let resp = handle_set_url(&engine.as_ref(), &caller, json!(1), &params);
            assert!(resp.error.is_none(), "{:?}", resp.error);
            remote_surface(&engine.as_ref(), md_sid).webview_label()
        };
        assert_eq!(
            send(json!({ "surface_id": md_sid, "url": "<html></html>", "label": "/docs/a.md" })),
            Some("/docs/a.md".to_string())
        );
        assert_eq!(
            send(json!({ "surface_id": md_sid, "url": "<html></html>" })),
            None
        );
        assert_eq!(
            send(json!({ "surface_id": md_sid, "url": "<html></html>", "label": "" })),
            None
        );
    }

    #[test]
    fn set_url_reaches_non_focused_split_leaf() {
        let (state, mut engine_session) = fixture(1, false);
        let engine = engine_session.borrow_mut();
        let terminal_sid = focused_surface_id(&state, &engine.as_ref());
        let md_sid = 2;

        assert_eq!(focused_surface_id(&state, &engine.as_ref()), terminal_sid);

        let resp = set_url(&engine.as_ref(), md_sid);
        assert!(
            resp.error.is_none(),
            "non-focused split leaf should be reachable: {:?}",
            resp.error
        );
        assert_eq!(resp.result, Some(json!({ "ok": true })));
    }

    #[test]
    fn set_url_reaches_all_leaves_of_nested_split() {
        let (_, mut engine_session) = fixture(2, false);
        let engine = engine_session.borrow_mut();
        // 1차: terminal | markdown_a → 2차: (terminal | markdown_b) | markdown_a
        let md_a = 2;
        let md_b = 3;

        for sid in [md_a, md_b] {
            let resp = set_url(&engine.as_ref(), sid);
            assert!(
                resp.error.is_none(),
                "leaf {sid} should be reachable: {:?}",
                resp.error
            );
        }
    }

    #[test]
    fn set_url_sole_leaf_ok_and_unknown_id_errors() {
        let (state, mut engine_session) = fixture(0, true);
        let engine = engine_session.borrow_mut();
        let md_sid = focused_surface_id(&state, &engine.as_ref());
        let resp = set_url(&engine.as_ref(), md_sid);
        assert!(
            resp.error.is_none(),
            "sole leaf regression: {:?}",
            resp.error
        );

        let resp = set_url(&engine.as_ref(), 999_999);
        assert_eq!(
            resp.error.map(|e| e.message),
            Some("surface_id not found".to_string())
        );
    }

    // 수신자 판정 단독 시험과 별도로 set_url이 실제 변경 통지를 호출하는지 확인한다.
    #[test]
    fn set_url_on_markdown_surface_signals_attached_clients() {
        use tasty_ipc::stream_hub::StreamHub;

        let (state, mut engine_session) = fixture(1, false);
        let engine = engine_session.borrow_mut();
        let terminal_sid = focused_surface_id(&state, &engine.as_ref());
        let md_sid = 2;
        let hub = StreamHub::new();
        let client = hub.alloc_id();
        let rx = hub.register(client);
        engine.remote.set_notifier(hub);
        let ws_id = engine
            .workspace_at(state.active_workspace_index(&engine))
            .expect("workspace index is valid")
            .id;
        engine
            .live
            .occupancy
            .acquire_workspace(ws_id, &[terminal_sid], &[terminal_sid, md_sid], client)
            .expect("acquire workspace");

        assert!(set_url(&engine.as_ref(), md_sid).error.is_none());
        let frame = rx.try_recv().expect("markdown_changed frame");
        let payload: Value = serde_json::from_slice(&frame.payload).expect("json payload");
        assert_eq!(payload["event"], "markdown_changed");
        assert_eq!(payload["surface_id"], md_sid);
        assert!(rx.try_recv().is_err(), "한 번의 set_url 에 신호는 한 번");

        assert!(set_url(&engine.as_ref(), terminal_sid).error.is_some());
        assert!(
            rx.try_recv().is_err(),
            "webview surface 가 아니면 신호가 없다"
        );
    }

    #[test]
    fn set_url_on_terminal_leaf_reports_not_webview() {
        let (state, mut engine_session) = fixture(1, false);
        let engine = engine_session.borrow_mut();
        let terminal_sid = focused_surface_id(&state, &engine.as_ref());
        // 분할만 준비한다. 검사 대상은 기존 터미널이며 새 surface ID는 사용하지 않는다.
        let resp = set_url(&engine.as_ref(), terminal_sid);
        assert_eq!(
            resp.error.map(|e| e.message),
            Some("surface is not a webview-enabled RemoteSurface".to_string())
        );
    }
}
