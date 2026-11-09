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
    use crate::model::Surface;
    if let Some((kind, plugin_id, _)) = surface.attach_content_info() {
        crate::remote::server::notify_markdown_changed(
            &engine.live.occupancy,
            engine.remote,
            kind,
            plugin_id,
            sid,
        );
    }
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

/// 이미 실린 문서에 JSON 메시지를 보낸다. 페이지는 `window` 의 `tasty:message` 이벤트로 받고
/// 값은 `event.detail` 에 있다. 문서를 갈아끼우는 set_url과 달리 입력 중인 값과 스크롤 위치가
/// 유지된다. plugin 은 코드가 아니라 데이터만 보내며, 이벤트를 내는 스크립트는 호스트가 만든다.
/// 결과는 돌려주지 않는다.
///
/// native WebView는 창이 들고 있으므로 RemoteSurface의 큐에 넣고 webview 동기화가 비운다.
/// 호출자는 surface 소유 플러그인이어야 하고, surface kind의 해석된 WebView 설정에서
/// 스크립트가 켜져 있어야 한다(받을 페이지 스크립트가 없으면 메시지는 아무도 안 받는다).
/// backend의 evaluate API는 페이지 JS 차단과 무관하게 실행되므로 이 검사가 유일한 게이트다.
#[cfg(feature = "gui")]
pub fn post_message(
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    id: Value,
    request: &crate::ipc::protocol::JsonRpcRequest,
) -> JsonRpcResponse {
    handle_post_message(&engine.as_ref(), caller, id, &request.params)
}

#[cfg(feature = "gui")]
fn handle_post_message(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let sid = match require_u32(params, "surface_id", &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let script = match params.get("data") {
        Some(data) => match message_script(data) {
            Ok(s) => s,
            Err(reason) => return JsonRpcResponse::invalid_params(id, reason),
        },
        None => return JsonRpcResponse::invalid_params(id, "missing 'data'"),
    };

    for ws in &engine.workspaces() {
        for &pid in &ws.pane_layout().all_pane_ids() {
            let Some(pane) = ws.pane_layout().find_pane(pid) else {
                continue;
            };
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
                let Some(rs) = surface
                    .as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
                else {
                    tracing::warn!(
                        "webview.post_message: surface {sid} is not a webview-enabled RemoteSurface"
                    );
                    return JsonRpcResponse::error(
                        id,
                        -32000,
                        "surface is not a webview-enabled RemoteSurface",
                    );
                };
                if let Err(reason) = post_message_allowed(engine, caller, rs) {
                    tracing::warn!("webview.post_message: surface {sid} refused: {reason}");
                    return JsonRpcResponse::error(id, -32000, reason);
                }
                rs.queue_script(script);
                return JsonRpcResponse::success(id, serde_json::json!({ "ok": true }));
            }
        }
    }
    tracing::warn!("webview.post_message: surface {sid} not found in any workspace layout");
    JsonRpcResponse::error(id, -32000, "surface_id not found")
}

/// 한 메시지의 직렬화 상한. 큐는 프레임마다 비지만 큰 값이 쌓이면 메모리와 프레임 시간을 잡아먹는다.
#[cfg(feature = "gui")]
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// 메시지를 `tasty:message` 이벤트로 내는 스크립트. JSON 은 그대로 JS 식이므로 값이 코드로
/// 바뀌지 않는다. 상한을 넘으면 거절한다.
#[cfg(feature = "gui")]
fn message_script(data: &Value) -> Result<String, &'static str> {
    let json = serde_json::to_string(data).map_err(|_| "'data' could not be serialized")?;
    if json.len() > MAX_MESSAGE_BYTES {
        return Err("'data' is larger than 1 MiB");
    }
    Ok(format!(
        "window.dispatchEvent(new CustomEvent('tasty:message',{{detail:{json}}}));"
    ))
}

/// post_message 허용 판정. 소유 플러그인만 자기 문서에 메시지를 보낼 수 있고, 스크립트 허용 여부는
/// 실제 WebView에 적용하는 것과 같은 해석(manifest 기본값과 사용자 설정)을 따른다.
#[cfg(feature = "gui")]
fn post_message_allowed(
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    caller: &tasty_ipc::caller::CallerContext,
    rs: &crate::plugin_bridge::remote_surface::RemoteSurface,
) -> Result<(), &'static str> {
    if !is_owner(caller, rs) {
        return Err("caller does not own this surface");
    }
    let Some(source) =
        crate::runtime::surface_registry::webview_kind::webview_settings_source(rs.kind_static)
    else {
        return Err("surface is not a webview-enabled RemoteSurface");
    };
    if !crate::webview::HtmlWebViewSettings::resolve(Some(&source), &engine.runtime.settings)
        .javascript_enabled
    {
        return Err("scripts are disabled for this webview surface");
    }
    Ok(())
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
                Box::new(crate::plugin_bridge::remote_surface::RemoteSurface::new(
                    sid,
                    "markdown",
                    "com.tasty.markdown".into(),
                    "markdown".into(),
                )),
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
            permissions: std::sync::Arc::new(std::collections::HashSet::new()),
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

    fn post_message_as(
        engine: &crate::runtime::engine_access::EngineRef<'_>,
        caller: &tasty_ipc::caller::CallerContext,
        sid: u32,
    ) -> JsonRpcResponse {
        handle_post_message(
            engine,
            caller,
            json!(1),
            &json!({ "surface_id": sid, "data": { "type": "update" } }),
        )
    }

    /// fixture의 markdown kind를 manifest 선언으로 등록한다. sandbox_scripts가 false면 스크립트가 켜진다.
    fn register_markdown_kind(sandbox_scripts: Option<bool>) {
        use crate::runtime::surface_registry::webview_kind as wk;
        let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(json!({
            "kind": "markdown",
            "display_name_i18n_key": "surface.kind.markdown",
            "rendering": "webview",
            "webview_defaults": { "sandbox_scripts": sandbox_scripts },
        }))
        .expect("surface kind decl");
        wk::reset_for_test();
        wk::register_webview_kind("com.tasty.markdown", &decl, &[]);
    }

    // 따옴표·괄호·태그가 든 값도 JSON 리터럴 안에 머문다 — 값이 코드로 이어지지 않는다.
    #[test]
    fn a_message_stays_a_json_literal_inside_the_event() {
        let data = json!({ "t": "a'b\"c);alert(1)//</script>" });
        let script = message_script(&data).expect("script");
        let detail = script
            .strip_prefix("window.dispatchEvent(new CustomEvent('tasty:message',{detail:")
            .and_then(|r| r.strip_suffix("}));"))
            .expect("고정된 앞뒤");
        assert_eq!(serde_json::from_str::<Value>(detail).expect("JSON"), data);
    }

    // 상한을 넘는 메시지는 큐에 넣기 전에 거절한다.
    #[test]
    fn a_message_over_the_cap_is_refused() {
        let big = json!("x".repeat(MAX_MESSAGE_BYTES));
        assert_eq!(message_script(&big), Err("'data' is larger than 1 MiB"));
    }

    // 소유 플러그인이고 manifest가 스크립트를 켠 kind일 때만 큐에 넣는다. 순서와 개수를 유지한다.
    #[test]
    fn post_message_queues_events_for_the_owner_when_the_manifest_enables_scripts() {
        let (state, mut engine_session) = fixture(0, true);
        // fixture 가 같은 락으로 kind 를 등록하므로 락은 그 뒤에 잡는다(std Mutex 는 재진입 불가).
        let _guard = crate::runtime::surface_registry::webview_kind::WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        register_markdown_kind(Some(false));
        let engine = engine_session.borrow_mut();
        let md_sid = focused_surface_id(&state, &engine.as_ref());
        let owner = plugin_caller(&remote_surface(&engine.as_ref(), md_sid).plugin_id.clone());

        for _ in 0..2 {
            let resp = post_message_as(&engine.as_ref(), &owner, md_sid);
            assert_eq!(resp.result, Some(json!({ "ok": true })), "{:?}", resp.error);
        }
        assert_eq!(
            remote_surface(&engine.as_ref(), md_sid).take_scripts(),
            vec![
                r#"window.dispatchEvent(new CustomEvent('tasty:message',{detail:{"type":"update"}}));"#;
                2
            ]
        );
        crate::runtime::surface_registry::webview_kind::reset_for_test();
    }

    // 소유자가 아니거나, kind가 등록되지 않았거나, 스크립트가 꺼진 kind면 거절하고 큐는 비어 있다.
    #[test]
    fn post_message_refuses_non_owners_and_surfaces_without_scripts() {
        let (state, mut engine_session) = fixture(0, true);
        // fixture 가 같은 락으로 kind 를 등록하므로 락은 그 뒤에 잡는다(std Mutex 는 재진입 불가).
        let _guard = crate::runtime::surface_registry::webview_kind::WEBVIEW_KIND_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let engine = engine_session.borrow_mut();
        let md_sid = focused_surface_id(&state, &engine.as_ref());
        let owner = plugin_caller(&remote_surface(&engine.as_ref(), md_sid).plugin_id.clone());
        let message = |caller: &tasty_ipc::caller::CallerContext| {
            post_message_as(&engine.as_ref(), caller, md_sid)
                .error
                .map(|e| e.message)
        };

        register_markdown_kind(Some(false));
        for other in [
            plugin_caller("com.example.other"),
            tasty_ipc::caller::CallerContext::local(),
        ] {
            assert_eq!(
                message(&other),
                Some("caller does not own this surface".to_string())
            );
        }

        crate::runtime::surface_registry::webview_kind::reset_for_test();
        assert_eq!(
            message(&owner),
            Some("surface is not a webview-enabled RemoteSurface".to_string())
        );

        for sandbox in [None, Some(true)] {
            register_markdown_kind(sandbox);
            assert_eq!(
                message(&owner),
                Some("scripts are disabled for this webview surface".to_string())
            );
        }

        assert!(
            remote_surface(&engine.as_ref(), md_sid)
                .take_scripts()
                .is_empty()
        );
        crate::runtime::surface_registry::webview_kind::reset_for_test();
    }
}
