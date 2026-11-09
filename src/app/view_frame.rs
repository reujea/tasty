//! Application frame sequencing: View geometry first, engine effects second, readonly drawing last.
use super::App;
use super::frame_effect::FramePhase;
impl App {
    pub(crate) fn redraw_main_window(&mut self, id: winit::window::WindowId) {
        self.dispatch_pending_intents();
        if self.journal.is_halted() || self.journal.pauses_observation() {
            return;
        }
        self.poll_port_scans();
        self.refresh_approval_presentations();
        let Some(engine) = self.engines.of_window(id) else {
            return;
        };
        let Some(session) = self.engines.session_mut(engine) else {
            return;
        };
        // 마지막 mirror 정리처럼 창을 유지한 채 workspace가 0개가 되면 기본 workspace가 journal을 거쳐
        // 생길 때까지 그리지 않는다. 화면 계산은 활성 workspace가 있다고 가정한다.
        if session.core_state.workspaces().is_empty() {
            tracing::debug!(window = ?id, "redraw deferred: the window has no workspace yet");
            return;
        }
        let Some(view) = self
            .view
            .views
            .get_mut(&id)
            .and_then(|view| view.as_main_mut())
        else {
            return;
        };
        // 다른 윈도우가 바꾼 공용 목록(Explorer·포트 즐겨찾기)을 그리기 전에 받는다.
        if session.runtime.sync_shared_lists() {
            tracing::debug!(window = ?id, "shared lists: copy refreshed before redraw");
            crate::view::ui::View::mark_dirty(view);
        }
        view.prepare_redraw(&session.read());
        apply_frame_effects(
            &mut view.state,
            session,
            self.plugin_manager.as_ref(),
            FramePhase::Geometry,
        );
        // TerminalStore owns grid → tap → throttled OS resize. Input contexts see that cut.
        view.prepare_render_inputs(
            &session.read(),
            self.plugin_manager
                .as_ref()
                .map(super::plugin_display::PluginDisplay::new),
        );
        apply_frame_effects(
            &mut view.state,
            session,
            self.plugin_manager.as_ref(),
            FramePhase::Inputs,
        );
        if let Some(manager) = self.plugin_manager.as_ref() {
            for update in super::view_mesh::relay_subscribed_mesh(
                &mut session.borrow_mut(),
                manager,
                &self.stream_hub,
            ) {
                match update {
                    super::view_mesh::MeshViewUpdate::Bootstrap(boot) => {
                        view.note_mesh_bootstrap(*boot)
                    }
                    super::view_mesh::MeshViewUpdate::Full { surface } => {
                        view.request_mesh_full(surface)
                    }
                }
            }
        }
        view.render_if_dirty(
            &session.read(),
            self.plugin_manager
                .as_ref()
                .map(super::plugin_display::PluginDisplay::new),
        );
        // A banner decision from this frame must update its gate before native reload/sync.
        apply_frame_effects(
            &mut view.state,
            session,
            self.plugin_manager.as_ref(),
            FramePhase::Draw,
        );
        view.finish_redraw(
            &session.read(),
            self.plugin_manager
                .as_ref()
                .map(super::plugin_display::PluginDisplay::new),
        );
        super::webview_sync::synchronize(
            view,
            &mut session.borrow_mut(),
            self.plugin_manager.as_ref(),
            &self.services.navigation_proofs,
        );
        super::webview_sync::announce_lazy_webviews(view, self.plugin_manager.as_mut());
        // 같은 프레임 동기화가 정한 모드·그림 위치로 창 단위 오버레이 host 를 맞춘다.
        super::webview_sync::settle_overlay_host(
            view,
            &self.gpu_instance,
            self.gpu_adapter.as_deref(),
        );
        self.process_remote_tool_requests(id);
        self.poll_port_scans();
        self.redraw_windows_for_shared_lists();
    }

    /// 공용 목록(Explorer·포트 즐겨찾기) 원본이 마지막으로 알린 리비전 뒤에 바뀌었으면 모든 메인 윈도우를
    /// 다시 그리게 한다. 즐겨찾기 변경은 일반 명령이라 프레임 시작·`about_to_wait` 의 일반 처리
    /// (`dispatch_pending_intents`)에서 적용되고 그쪽이 이것을 불러 알린다. 프레임 끝의 호출은 그 경로를
    /// 거치지 않은 변경을 위한 안전망이다. 각 윈도우는 그리기 전에 사본을 맞춘다.
    pub(crate) fn redraw_windows_for_shared_lists(&mut self) {
        let registries = &self.services.registries;
        redraw_other_windows(
            [
                registries.explorer_favorites.revision(),
                registries.port_favorites.revision(),
            ],
            &mut self.shared_lists_announced,
            self.view
                .views
                .values_mut()
                .filter_map(|view| view.as_main_mut()),
            crate::view::ui::View::mark_dirty,
        );
    }
}

/// 프레임 단계 사이에 View가 낸 요청 중 `reached` 단계까지의 프레임 효과만 지금 출처와 함께 적용한다.
/// 일반 명령과 아직 오지 않은 단계의 효과는 출처 봉투를 그대로 둔 채 같은 순서로 큐에 되돌린다. 뒤 단계
/// 효과는 이 프레임의 해당 drain이, 일반 명령은 다음 일반 dispatch가 처리한다.
fn apply_frame_effects(
    state: &mut crate::state::MainViewState,
    session: &mut crate::runtime::engine_session::EngineSession,
    plugins: Option<&crate::plugin::PluginManager>,
    reached: FramePhase,
) {
    for intent in state.take_pending_intents() {
        match &intent.body {
            crate::intent::Intent::Frame(effect) if effect.phase() <= reached => {
                effect.apply(&mut session.borrow_mut(), plugins, &intent.origin)
            }
            _ => state.dispatch_intent(intent),
        }
    }
}

/// `announced` 뒤에 원본 리비전이 바뀌었으면 기준을 올리고 모든 윈도우에 `redraw` 를 부른다.
fn redraw_other_windows<K: PartialEq + Copy, W>(
    current: K,
    announced: &mut K,
    windows: impl IntoIterator<Item = W>,
    mut redraw: impl FnMut(W),
) {
    if current == *announced {
        return;
    }
    *announced = current;
    for window in windows {
        redraw(window);
    }
}

#[cfg(test)]
mod tests {
    fn describe(intent: &crate::intent::DispatchedIntent) -> String {
        format!("{:?} {:?}", intent.body, intent.origin)
    }

    /// 도달한 단계까지의 프레임 효과만 그 자리에서 적용된다. 일반 명령과 뒤 단계 효과는 출처와 함께
    /// 원래 순서로 큐에 남고, 뒤 단계 효과는 그 단계의 drain에서 적용된다.
    #[test]
    fn a_frame_drain_applies_due_effects_and_requeues_the_rest_in_order() {
        use super::FramePhase;
        use crate::app::engine_action::{EngineAction, SettingsPatch};
        use crate::app::frame_effect::{DrawEffect, InputEffect};
        use crate::core::state::attention::AttentionKind;
        use crate::intent::Intent;

        let (mut state, mut session) = crate::state::tests::test_state();
        let sid = session
            .borrow_mut()
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = crate::runtime::surface_binding::SurfaceBinding::capture(&session.read(), sid)
            .expect("binding");
        session
            .borrow_mut()
            .raise_attention(sid, AttentionKind::NeedsInput);
        let patch = |mode: &str| Intent::PatchSettings(SettingsPatch::ExplorerMode(mode.into()));
        let typing = Intent::Engine(EngineAction::RecordTyping {
            target: target.clone(),
            at: std::time::Instant::now(),
        })
        .from_user_shortcut("typing");
        let full = Intent::Frame(
            DrawEffect::RemoteMeshFull {
                targets: vec![target.clone()],
            }
            .into(),
        )
        .from_user_shortcut("mesh-full-recovery");
        state.dispatch_intent(patch("first").from_user_menu("first"));
        state.dispatch_intent(
            Intent::Frame(
                InputEffect::FocusObserved {
                    target,
                    window_focused: true,
                }
                .into(),
            )
            .from_user_context_menu(),
        );
        state.dispatch_intent(typing.clone());
        state.dispatch_intent(full.clone());
        state.dispatch_intent(patch("second").from_agent_ipc());

        super::apply_frame_effects(&mut state, &mut session, None, FramePhase::Inputs);

        assert_eq!(
            session.borrow_mut().attention_kind(sid),
            None,
            "the input effect ran"
        );
        assert!(
            session.borrow_mut().live.last_key_input.is_empty(),
            "a general command is not run by a frame drain"
        );
        assert!(
            session
                .borrow_mut()
                .remote
                .pending_mesh_full_resend_forward
                .is_empty(),
            "a draw effect waits for the draw drain"
        );
        let first = patch("first").from_user_menu("first");
        let second = patch("second").from_agent_ipc();
        let rest: Vec<String> = state.take_pending_intents().iter().map(describe).collect();
        assert_eq!(
            rest,
            [
                describe(&first),
                describe(&typing),
                describe(&full),
                describe(&second)
            ]
        );

        for intent in [first.clone(), typing.clone(), full, second.clone()] {
            state.dispatch_intent(intent);
        }
        super::apply_frame_effects(&mut state, &mut session, None, FramePhase::Draw);
        assert!(
            session
                .borrow_mut()
                .remote
                .pending_mesh_full_resend_forward
                .contains(&sid)
        );
        let rest: Vec<String> = state.take_pending_intents().iter().map(describe).collect();
        assert_eq!(
            rest,
            [describe(&first), describe(&typing), describe(&second)]
        );
    }

    /// 화면 프레임 밖의 IPC가 낸 원격 목록 조회는 일반 명령이다. 프레임 drain은 받지 않고 일반 dispatch가
    /// 적용한다.
    #[test]
    fn an_ipc_list_directory_is_left_for_the_general_dispatch() {
        use super::FramePhase;
        use crate::app::engine_action::EngineAction;
        use crate::intent::Intent;

        let (mut state, mut session) = crate::state::tests::test_state();
        let ws_id = 9_000u32;
        let mut ws = crate::model::Workspace::new_with_terminal_marker(
            ws_id,
            "mirror".to_string(),
            9_001,
            9_002,
            9_003,
        );
        ws.mirror = true;
        session.borrow_mut().push_mirror_workspace(ws);
        let projection = session
            .read()
            .mirror_projection_token(ws_id)
            .expect("mirror projection");
        let list = Intent::Engine(EngineAction::ListDirectory {
            request: crate::core::PendingListDirForward {
                local_ws_id: ws_id,
                request_id: 7,
                dir: "/srv".into(),
                consumer: None,
            },
            projection,
            target: None,
        })
        .from_agent_ipc();
        state.dispatch_intent(list.clone());

        for phase in [FramePhase::Geometry, FramePhase::Inputs, FramePhase::Draw] {
            super::apply_frame_effects(&mut state, &mut session, None, phase);
        }

        let rest = state.take_pending_intents();
        assert_eq!(
            rest.iter().map(describe).collect::<Vec<_>>(),
            [describe(&list)]
        );
        assert!(
            session
                .borrow_mut()
                .remote
                .pending_list_dir_forward
                .is_empty()
        );

        let Intent::Engine(action) = &rest[0].body else {
            unreachable!("checked above")
        };
        action.apply(&mut session.borrow_mut());
        assert_eq!(
            session.borrow_mut().remote.pending_list_dir_forward.len(),
            1
        );
    }

    /// prepare_redraw가 낸 기본 grid는 첫 drain에서 바로 적용돼 입력 준비가 새 값을 본다.
    #[test]
    fn the_geometry_drain_applies_the_default_grid_before_inputs() {
        use crate::app::frame_effect::GeometryEffect;
        use crate::intent::Intent;

        let (mut state, mut session) = crate::state::tests::test_state();
        state.dispatch_intent(
            Intent::Frame(
                GeometryEffect::DefaultGrid {
                    cols: 133,
                    rows: 41,
                }
                .into(),
            )
            .from_user_shortcut("grid-resize"),
        );

        super::apply_frame_effects(&mut state, &mut session, None, super::FramePhase::Geometry);

        let engine = session.borrow_mut();
        assert_eq!(
            (engine.runtime.default_cols, engine.runtime.default_rows),
            (133, 41)
        );
        assert!(state.take_pending_intents().is_empty());
    }

    /// html 탭 하나와 스크립트가 있는 확정 문서를 만든다. 배너가 허용을 기다린다.
    fn state_with_html_document() -> (
        crate::state::RequestContext,
        crate::runtime::engine_session::EngineSession,
        u32,
    ) {
        use tasty_core::{DomainEvent as E, SurfaceSpec};
        use tasty_model::html_script::{Fingerprint, ScriptDetection, ScriptScan};
        let model = crate::state::tests::test_model(vec![
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
                name: "html".into(),
                surface: SurfaceSpec {
                    id: 1,
                    kind: "html".into(),
                    data: None,
                },
            },
        ]);
        let (state, mut session) = crate::state::tests::test_state_from_model(model);
        let remote = crate::plugin_bridge::remote_surface::RemoteSurface::new(
            1,
            "html",
            "com.tasty.html".into(),
            "html".into(),
        );
        remote.with_html_script(|st| {
            st.on_host_load_requested();
            st.on_load_started();
            st.on_main_response(
                "file:///docs/a.html",
                Some(ScriptScan {
                    fingerprint: Fingerprint([0xab; 32]),
                    detection: ScriptDetection::Scripts,
                    remote_refs: false,
                    remote_scripts: false,
                }),
            );
            st.on_committed(Some("file:///docs/a.html"));
            st.on_load_finished();
            st.update_banner();
        });
        session.runtime.surfaces.insert(1, Box::new(remote));
        (state, session, 1)
    }

    fn take_reload_request(
        session: &mut crate::runtime::engine_session::EngineSession,
        sid: u32,
    ) -> bool {
        session
            .runtime
            .surfaces
            .get(&sid)
            .and_then(|s| {
                s.as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
            })
            .expect("html remote surface")
            .with_html_script(|st| st.take_reload_request())
    }

    /// draw에서 누른 허용은 그 프레임의 마지막 drain에서 gate에 반영돼 native 동기화가 reload를 본다.
    #[test]
    fn an_allow_drawn_this_frame_reaches_the_gate_before_native_sync() {
        use crate::app::frame_effect::DrawEffect;
        use crate::intent::Intent;
        use crate::runtime::html_script::HtmlActionKind;

        let (mut state, mut session, sid) = state_with_html_document();
        let allow = session
            .read()
            .html_script(sid)
            .expect("html snapshot")
            .request(HtmlActionKind::Allow);
        state.dispatch_intent(
            Intent::Frame(DrawEffect::Html(allow).into()).from_user_menu("html-script-banner"),
        );
        assert!(!take_reload_request(&mut session, sid));

        super::apply_frame_effects(&mut state, &mut session, None, super::FramePhase::Draw);

        assert!(
            take_reload_request(&mut session, sid),
            "the allowance must request the reload before webview sync"
        );
    }

    /// 허용은 사용자 결정이다. 사용자 출처가 아닌 허용은 gate를 바꾸지 않는다.
    #[test]
    fn an_allow_without_a_user_origin_is_refused() {
        use crate::app::frame_effect::DrawEffect;
        use crate::intent::Intent;
        use crate::runtime::html_script::HtmlActionKind;

        let (mut state, mut session, sid) = state_with_html_document();
        let allow = session
            .read()
            .html_script(sid)
            .expect("html snapshot")
            .request(HtmlActionKind::Allow);
        state.dispatch_intent(Intent::Frame(DrawEffect::Html(allow).into()).from_agent_ipc());

        super::apply_frame_effects(&mut state, &mut session, None, super::FramePhase::Draw);

        assert!(!take_reload_request(&mut session, sid));
        assert!(state.take_pending_intents().is_empty());
    }

    use crate::core::explorer_favorites::ExplorerFavorites;
    use crate::core::port_favorites::PortFavorites;
    use crate::core::shared_list::SharedList;

    /// 프레임 밖에서 적용된 제거도, 다른 목록의 변경도 다음 확인 때 모든 윈도우를 한 번 깨운다.
    #[test]
    fn a_shared_list_change_redraws_every_window_once() {
        let _home = crate::test_support::IsolatedHome::new();
        let explorer = SharedList::<ExplorerFavorites>::default();
        let port = SharedList::<PortFavorites>::default();
        let current = || [explorer.revision(), port.revision()];
        let mut announced = current();
        let mut redrawn = [0, 0];
        let check = |announced: &mut [u64; 2], redrawn: &mut [i32; 2]| {
            super::redraw_other_windows(current(), announced, redrawn.iter_mut(), |n| *n += 1)
        };
        check(&mut announced, &mut redrawn);
        assert_eq!(redrawn, [0, 0], "nothing changed");

        let path = crate::test_support::abs_path("w/alpha");
        explorer.update(|f| f.add(path.clone(), String::new()));
        explorer.update(|f| f.remove(&path));
        check(&mut announced, &mut redrawn);
        assert_eq!(redrawn, [1, 1]);
        check(&mut announced, &mut redrawn);
        assert_eq!(redrawn, [1, 1], "already announced");

        port.update(|f| f.add("127.0.0.1".parse().unwrap(), 3000, String::new()));
        check(&mut announced, &mut redrawn);
        assert_eq!(redrawn, [2, 2], "a port favorite change also redraws");
    }
}
