//! `Core` — mirror(원격 attach client) 워크스페이스 구조 변경 차단/forward + 단일 mutate
//! 진입점(`Core::apply`). `src/core/mod.rs` 의 `impl Core` 분할.

use super::*;

/// mirror(원격 attach client) 워크스페이스에서 구조 변경(split·new-tab·close·이동)이
/// 시도됐음을 나타내는 마커 에러. `Core::apply` 가 구조 `DomainIntent` 의 대상이
/// mirror 워크스페이스일 때 로컬 실행을 **거부**하며 반환한다 — 로컬 PTY spawn /
/// 로컬 트리 변경은 "workspace 전체가 remote" 불변식을 깨기 때문.
///
/// 호출자는 [`anyhow::Error::downcast_ref`] 로 이 타입을 식별해 (사용자 경로에서)
/// 차단 toast 를 띄운다. 구조 변경을 원격으로 forward 하는 2단계에서 이 지점이
/// forward 요청/응답으로 대체된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MirrorStructuralBlocked {
    /// 대상 mirror 워크스페이스 인덱스.
    pub workspace_index: usize,
    /// `true` 면 이 구조 op 를 원격으로 **forward** 하도록 큐에 넣었다(2단계). 이 경우
    /// 로컬 실행만 막고 차단 toast 는 띄우지 않는다(원격 실행 결과가 UX 를 결정 —
    /// 성공 시 무음, 실패 시 forward 실패 toast). `false` 면 forward 할 수 없는 op
    /// (워크스페이스 경계를 넘는 move-surface, 또는 anchor 를 못 찾은 op)라 기존 차단
    /// toast 를 띄운다. convert 는 항상 forward 된다(`build_mirror_forward_op`).
    pub forwarded: bool,
}

impl std::fmt::Display for MirrorStructuralBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "structural change rejected: target belongs to a mirror (remote attach) workspace; \
             the operation must be performed on the remote instance"
        )?;
        // headless 의 거절은 op 종류가 아니라 빌드가 사유다 — 같은 문구로 두면 호출자가
        // "이 op 만 안 된다" 로 읽는다(docs/adr/0538-headless-refuses-mirror-structural-forward.md).
        #[cfg(not(feature = "gui"))]
        write!(
            f,
            " (this headless build has no attach client to forward it)"
        )?;
        Ok(())
    }
}

impl std::error::Error for MirrorStructuralBlocked {}

/// mirror 구조 변경 forward 큐(`CoreState::pending_structural_forward`)의 원소.
/// `Core::apply` 는 origin 을 모르므로 항상 `user_triggered: false`(+ 빈 candidates)로
/// push 한다 — 이는 IPC/에이전트 호출과 동일하게 취급되는 안전한 기본값이다. origin 을
/// 아는 GUI 호출부(`intent::pane`/`intent::surface`/`intent::tab`, 그리고 origin 개념이
/// 아예 없이 항상 GUI 직접 호출인 `state::AppState::forward_mirror_structural`)가
/// 사후에 `user_triggered`를 뒤집거나(전자) 처음부터 `true`로 push한다(후자).
///
/// 두 client-only focus 보정이 이 태그를 근거로 한다:
/// - **새 리소스로 focus 이동**: `user_triggered`가 true 인 new-tab/split 이
///   성공하면, 그 결과 delta 에서 새로 생긴 surface 로 focus 를 옮긴다.
/// - **close 시 인접 대상 fallback**: `close_focus_candidates`(로컬 surface id,
///   우선순위 순)를 담아두면, 닫힌 surface 가 focus 였던 경우(=기존 `restore_focus_
///   after_delta`가 복원할 대상을 잃는 경우) 첫 번째로 살아남은 후보로 focus 를
///   옮긴다. new-tab/split 등 close 가 아닌 op 은 항상 빈 벡터.
#[derive(Debug, Clone)]
#[cfg_attr(
    all(not(feature = "gui"), not(test)),
    expect(
        dead_code,
        reason = "headless 의 `Core::apply` 는 이 큐에 넣지 않고 거절한다(ADR-0538). \
                  정의가 남는 것은 두 조합이 공유하는 `mark_last_forward_*` 가 큐의 \
                  마지막 원소를 표시하기 때문이고, op 를 읽어 보내는 쪽은 GUI 의 \
                  about_to_wait 뿐이다. 시험은 칸을 읽으므로 test 구성은 뺀다"
    )
)]
pub(crate) struct PendingStructuralForward {
    pub(crate) op: tasty_ipc::stream::StructuralOp,
    pub(crate) user_triggered: bool,
    pub(crate) close_focus_candidates: Vec<u32>,
    /// 원격이 이 op 를 적용하지 못했을 때 사용자 toast 를 내지 않고 로그로만 남긴다.
    /// origin 을 아는 호출부가 에이전트 발화라고 표시한 op 만 `true` 다
    /// ([`mark_last_forward_agent_origin`]) — 에이전트 행동의 실패가 사용자 시각 상태에
    /// 닿지 않게 한다(identity 원칙 1). 기본 `false` 는 origin 을 모르는 `Core::apply`
    /// 호출의 종전 동작(실패 toast)을 그대로 둔다.
    pub(crate) silent_failure: bool,
}

impl PendingStructuralForward {
    #[cfg(feature = "gui")]
    fn agent(op: tasty_ipc::stream::StructuralOp) -> Self {
        Self {
            op,
            user_triggered: false,
            close_focus_candidates: Vec::new(),
            silent_failure: false,
        }
    }
}

/// `core.apply(...)` 가 mirror-block+forward 로 방금 push 한 **마지막** op 를 "에이전트
/// 발화" 로 표시한다 — 원격 실패 회신이 사용자 toast 가 아니라 로그로 가게 한다.
/// [`mark_last_forward_user_triggered`] 와 같은 가드를 쓴다: `err` 가 `forwarded=true` 인
/// `MirrorStructuralBlocked` 가 아니거나 `origin` 이 에이전트가 아니면 no-op 이다.
pub(crate) fn mark_last_forward_agent_origin(
    engine: &mut CoreState,
    err: &anyhow::Error,
    origin: &crate::core::origin::IntentOrigin,
) {
    let Some(blocked) = err.downcast_ref::<MirrorStructuralBlocked>() else {
        return;
    };
    if !blocked.forwarded || !origin.is_agent() {
        return;
    }
    if let Some(last) = engine.pending_structural_forward.last_mut() {
        last.silent_failure = true;
    }
}

/// `core.apply(...)`가 mirror-block+forward 로 방금 push 한 **마지막** op 를 "사용자
/// GUI 조작 유래"로 표시한다(new-tab/split focus 이동의 근거). `err` 가 `forwarded=true`인 `MirrorStructuralBlocked`
/// 가 아니거나 `origin` 이 사용자가 아니면 no-op(기본 `false` 유지) — 다른 이유의
/// 실패로 큐에 아무것도 안 쌓였는데 엉뚱한 이전 op 를 잘못 표시하는 것을 막는다.
pub(crate) fn mark_last_forward_user_triggered(
    engine: &mut CoreState,
    err: &anyhow::Error,
    origin: &crate::core::origin::IntentOrigin,
) {
    let Some(blocked) = err.downcast_ref::<MirrorStructuralBlocked>() else {
        return;
    };
    if !blocked.forwarded || !origin.is_user() {
        return;
    }
    if let Some(last) = engine.pending_structural_forward.last_mut() {
        last.user_triggered = true;
    }
}

/// mirror 구조 `DomainIntent` → 원격 forward 할 [`StructuralOp`](tasty_ipc::stream::StructuralOp).
/// anchor 는 **로컬** mirror surface id(App drain 이 세션 매핑으로 원격 id 로 치환).
/// pane/tab 대상 op 는 그 pane/tab 의 대표 surface(활성 탭의 focused surface)를 anchor 로
/// 삼아 원격이 자기 트리에서 pane/tab 을 resolve 하게 한다. `MoveSurface` 는 source/target
/// 이 서로 다른 workspace 에 걸치면(mirror↔local 경계 포함) forward 하지 않는다 — 로컬
/// 전용 surface_id 를 원격에 그대로 보내면 그 id 가 원격 트리의 무관한 surface 와 우연히
/// 겹칠 때(둘 다 단순 u32, 네임스페이스 분리 없음) 엉뚱한 surface 가 대상이 될 위험이
/// 있다. anchor 를 못 찾거나 위 조건에 안 맞으면 `None`(→ 기존 차단 유지).
#[cfg(feature = "gui")]
fn build_mirror_forward_op(
    engine: &crate::core::CoreState,
    intent: &DomainIntent,
) -> Option<tasty_ipc::stream::StructuralOp> {
    use crate::core::intent::DomainIntent as D;
    use tasty_ipc::stream::{SplitAxis, StructuralOp};

    fn axis(d: &crate::model::SplitDirection) -> SplitAxis {
        match d {
            crate::model::SplitDirection::Horizontal => SplitAxis::Horizontal,
            crate::model::SplitDirection::Vertical => SplitAxis::Vertical,
        }
    }
    // pane 안 대표 surface(활성 탭의 focused surface) — pane/tab op 의 anchor.
    let pane_anchor = |pane_id: u32| -> Option<u32> {
        engine
            .find_pane_by_id(pane_id)
            .and_then(|p| p.tabs.get(p.active_tab))
            .and_then(|t| t.focused_surface_id())
    };
    let tab_anchor = |tab_id: u32| -> Option<u32> {
        for ws in &engine.workspaces {
            for pid in ws.pane_layout().all_pane_ids() {
                if let Some(pane) = ws.pane_layout().find_pane(pid) {
                    for tab in &pane.tabs {
                        if tab.id == tab_id {
                            return tab.focused_surface_id();
                        }
                    }
                }
            }
        }
        None
    };

    match intent {
        D::SplitSurface {
            target_surface_id,
            direction,
            kind,
            surface_params,
            ..
        } => Some(StructuralOp::SplitSurface {
            surface_id: *target_surface_id,
            direction: axis(direction),
            surface_kind: kind.clone(),
            params: surface_params.clone(),
        }),
        D::SplitPane {
            target_pane_id,
            direction,
            kind,
            surface_params,
            ..
        } => Some(StructuralOp::SplitPane {
            anchor_surface_id: pane_anchor(*target_pane_id)?,
            direction: axis(direction),
            surface_kind: kind.clone(),
            params: surface_params.clone(),
        }),
        D::CreateTab {
            pane_id,
            kind,
            surface_params,
            ..
        } => Some(StructuralOp::NewTab {
            anchor_surface_id: pane_anchor(*pane_id)?,
            surface_kind: kind.clone(),
            params: surface_params.clone(),
        }),
        D::CloseSurface { surface_id, .. } => Some(StructuralOp::CloseSurface {
            surface_id: *surface_id,
        }),
        D::CloseTab { tab_id } => Some(StructuralOp::CloseTab {
            anchor_surface_id: tab_anchor(*tab_id)?,
        }),
        D::ClosePane { pane_id } => Some(StructuralOp::ClosePane {
            anchor_surface_id: pane_anchor(*pane_id)?,
        }),
        // 복원은 "무엇을 만들지" 를 클라이언트가 정하지 않는다 — 무엇이 복원될지는
        // 원격 스택이 정하므로 op 에는 anchor 밖에 없다(ADR-0264 결정 2).
        D::RestoreClosedItem { target_pane_id, .. } => Some(StructuralOp::RestoreClosedItem {
            anchor_surface_id: pane_anchor((*target_pane_id)?)?,
        }),
        D::MoveTab {
            pane_id,
            from_index,
            to_index,
        } => Some(StructuralOp::MoveTab {
            anchor_surface_id: pane_anchor(*pane_id)?,
            from_index: *from_index,
            to_index: *to_index,
        }),
        D::ConvertSurface { surface_id, target } => {
            use crate::core::intent::ConvertSurfaceTarget;
            // cwd 는 intent handler 가 결정한 값이다(`docs/design/policies/cwd.md#surface-cwd-invariant`
            // §3) — forward 경로에서 버리지 않고 그대로 실어보낸다. mirror surface 에서 carry
            // 한 cwd 는 원격 출처라 로컬 carry 헬퍼가 `None` 을 돌려주므로, 여기 값이 있는
            // 것은 호출자가 명시한 경우뿐이다. `None` 이면 서버가 자기 PTY 에서 직접
            // resolve 한다 — 그 값이 진실 원천이고 client 가 가진 값은 그 사본이다(§3-2).
            let (surface_kind, params, cwd) = match target {
                ConvertSurfaceTarget::Terminal { cwd } => {
                    ("terminal".to_string(), serde_json::json!({}), cwd.clone())
                }
                ConvertSurfaceTarget::Kind { kind, params, cwd } => {
                    (kind.clone(), params.clone(), cwd.clone())
                }
            };
            Some(StructuralOp::ConvertSurface {
                surface_id: *surface_id,
                surface_kind,
                params,
                cwd: cwd.map(|p| p.to_string_lossy().into_owned()),
            })
        }
        D::MoveSurface {
            source_surface_id,
            target_surface_id,
        } => {
            let src_ws = engine
                .find_workspace_index_for_surface(*source_surface_id)
                .map(|(i, _)| i);
            let tgt_ws = engine
                .find_workspace_index_for_surface(*target_surface_id)
                .map(|(i, _)| i);
            if src_ws.is_some() && src_ws == tgt_ws {
                Some(StructuralOp::MoveSurface {
                    source_surface_id: *source_surface_id,
                    target_surface_id: *target_surface_id,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// mirror 구조 op 를 원격 forward 큐에 넣는다. 넣었으면 `true`(→ 호출자는 로컬 실행만
/// 막고 `{forwarded: true}` 로 답한다), forward 할 수 없는 op 면 `false`(→ 차단).
#[cfg(feature = "gui")]
fn queue_mirror_forward(engine: &mut crate::core::CoreState, intent: &DomainIntent) -> bool {
    match build_mirror_forward_op(engine, intent) {
        Some(op) => {
            engine
                .pending_structural_forward
                .push(PendingStructuralForward::agent(op));
            true
        }
        None => false,
    }
}

/// headless 는 mirror 구조 op 를 forward 하지 않는다 — 큐를 비워 attach 채널로 보내는
/// 주체(`App::dispatch_pending_structural_forwards`)가 GUI 의 `about_to_wait` 에만 있다.
/// 넣으면 IPC 핸들러가 `{forwarded: true}` 로 성공을 답하고 op 는 영영 안 나간다.
/// 그래서 넣지 않고 차단으로 답한다(docs/adr/0538-headless-refuses-mirror-structural-forward.md).
/// headless 에는 mirror workspace 를 만드는 자리(`app::attach_client`)도 없어 오늘은
/// 도달하지 않는 갈래지만, 도달하는 날에도 거짓 성공이 아니라 거절이 나가게 한다.
#[cfg(not(feature = "gui"))]
fn queue_mirror_forward(_engine: &mut crate::core::CoreState, _intent: &DomainIntent) -> bool {
    false
}

impl Core {
    /// 도메인 변경의 단일 진입점. handler 가 발행한 `DomainIntent` 를 받아
    /// 결과 이벤트 목록을 반환. variant 를 더하면 본 match 도 채운다.
    ///
    /// `engine` 인자: 발화 대상 engine. 현재 *이벤트만 발행* 패턴인 variant
    /// 들은 인자를 사용하지 않으나, workspace.create
    /// 처럼 *결과 정보가 필요한* variant 는 본 메서드 안에서 직접 mutate 후
    /// event 에 결과를 담아 반환한다. CreateWorkspace 분기만 engine 을
    /// 사용하므로 rustc 는 unused 경고를 내지 않는다.
    pub(crate) fn apply(
        &mut self,
        engine: &mut crate::core::CoreState,
        intent: DomainIntent,
    ) -> anyhow::Result<Vec<CoreEvent>> {
        // mirror(원격 attach client) 워크스페이스 누출 차단 — 그 안의 구조 변경은
        // 로컬에서 실행하지 않는다(로컬 PTY spawn / 트리 변경 금지). 사용자 단축키·
        // 에이전트 IPC 어느 진입 경로든 여기(단일 mutate 진입점)로 수렴하므로 한 곳에서
        // 막는다. 구조와 무관한 intent 는 통과. (2단계에서 이 지점이 원격 forward 로 대체.)
        if let Some(workspace_index) = engine.mirror_workspace_index_for_structural(&intent) {
            // 2단계: 로컬 실행은 여전히 막되(불변식 유지), forward 가능한 op 는 원격에
            // 넘기도록 큐에 넣는다. anchor 는 아직 로컬 surface id — App drain 이 세션
            // 매핑으로 원격 id 로 치환해 전송한다. forward 불가 op(워크스페이스 경계를 넘는
            // move-surface, anchor 를 못 찾은 op)는 None → 기존 차단 toast. convert 는 항상
            // forward 된다. headless 는 forward 하지 않고 모두 차단한다(`queue_mirror_forward`).
            let forwarded = queue_mirror_forward(engine, &intent);
            return Err(anyhow::Error::new(MirrorStructuralBlocked {
                workspace_index,
                forwarded,
            }));
        }
        match intent {
            // 본 분기들은 *이벤트만 발행*한다. cascade(Theme apply / Scrollback
            // limit / clipboard max / notification coalesce 등)는 창·App 에 닿으므로
            // 이벤트를 받는 App 쪽 dispatcher(`app::dispatch_domain`)가 한다.
            DomainIntent::UpdateSettings(new_settings) => {
                Ok(vec![CoreEvent::SettingsUpdated(new_settings)])
            }
            DomainIntent::PushNotification {
                ws_id,
                surface_id,
                title,
                body,
                source,
            } => Ok(vec![CoreEvent::NotificationPushRequested {
                ws_id,
                surface_id,
                title,
                body,
                source,
            }]),
            #[cfg(feature = "gui")]
            DomainIntent::MarkNotificationRead { id } => {
                Ok(vec![CoreEvent::NotificationReadRequested { id }])
            }
            #[cfg(feature = "gui")]
            DomainIntent::MarkAllNotificationsRead => {
                Ok(vec![CoreEvent::AllNotificationsReadRequested])
            }
            #[cfg(feature = "gui")]
            DomainIntent::SurfaceCwdChanged { surface_id } => {
                Ok(vec![CoreEvent::SurfaceCwdChanged { surface_id }])
            }
            DomainIntent::SetTerminalMark { surface_id } => {
                Ok(vec![CoreEvent::TerminalMarkSet { surface_id }])
            }
            DomainIntent::SurfaceCompletion { surface_id, kind } => {
                Ok(vec![CoreEvent::SurfaceCompletionRequested {
                    surface_id,
                    kind,
                }])
            }
            DomainIntent::SurfaceAttentionClear { surface_id, kind } => {
                Ok(vec![CoreEvent::SurfaceAttentionClearRequested {
                    surface_id,
                    kind,
                }])
            }
            DomainIntent::CreateWorkspace {
                cwd,
                kind,
                surface_params,
                name,
                subtitle,
                description,
                category,
            } => self.apply_create_workspace(
                engine,
                WorkspaceCreationParams {
                    cwd,
                    kind,
                    surface_params,
                    name,
                    subtitle,
                    description,
                    category,
                },
            ),
            DomainIntent::UpdateWorkspaceMeta {
                workspace_id,
                name,
                subtitle,
                description,
            } => {
                self.apply_update_workspace_meta(engine, workspace_id, name, subtitle, description)
            }
            DomainIntent::MoveWorkspace {
                from_index,
                to_index,
            } => Ok(vec![
                self.apply_move_workspace(engine, from_index, to_index),
            ]),
            DomainIntent::CreateTab {
                pane_id,
                cwd,
                kind,
                name,
                surface_params,
                activate,
            } => Self::apply_create_tab(engine, pane_id, cwd, kind, name, surface_params, activate),
            DomainIntent::CloseTab { tab_id } => Ok(vec![Self::apply_close_tab(engine, tab_id)]),
            DomainIntent::MoveTab {
                pane_id,
                from_index,
                to_index,
            } => Ok(vec![Self::apply_move_tab(
                engine, pane_id, from_index, to_index,
            )]),
            DomainIntent::AdoptTerminal { pane_id, pty_id } => {
                Self::apply_adopt_terminal(engine, pane_id, pty_id)
            }
            DomainIntent::SplitPane {
                target_pane_id,
                direction,
                cwd,
                kind,
                surface_params,
            } => {
                Self::apply_split_pane(engine, target_pane_id, direction, cwd, kind, surface_params)
            }
            DomainIntent::SplitSurface {
                target_surface_id,
                direction,
                cwd,
                kind,
                surface_params,
            } => Self::apply_split_surface(
                engine,
                target_surface_id,
                direction,
                cwd,
                kind,
                surface_params,
            ),
            DomainIntent::ClosePane { pane_id } => {
                Ok(vec![Self::apply_close_pane(engine, pane_id)])
            }
            DomainIntent::CloseSurface {
                surface_id,
                save_snapshot,
            } => Ok(vec![Self::apply_close_surface(
                engine,
                surface_id,
                save_snapshot,
            )]),
            DomainIntent::ConvertSurface { surface_id, target } => {
                Ok(vec![Self::apply_convert_surface(
                    engine, surface_id, target,
                )])
            }
            DomainIntent::MoveSurface {
                source_surface_id,
                target_surface_id,
            } => Ok(vec![Self::apply_move_surface(
                engine,
                source_surface_id,
                target_surface_id,
            )]),
            DomainIntent::SendToSurface {
                surface_id,
                payload,
            } => Ok(vec![Self::apply_send_to_surface(
                engine, surface_id, payload,
            )]),
            DomainIntent::RespawnTerminal { surface_id, cwd } => {
                Ok(vec![Self::apply_respawn_terminal(engine, surface_id, cwd)])
            }
            DomainIntent::RestoreClosedItem {
                target_pane_id,
                scope,
            } => Ok(vec![Self::apply_restore_closed_item(
                engine,
                target_pane_id,
                scope,
            )]),
            #[cfg(feature = "gui")]
            DomainIntent::UpdateTabName { surface_id, name } => {
                Ok(vec![Self::apply_update_tab_name(engine, surface_id, name)])
            }
            #[cfg(feature = "gui")]
            DomainIntent::SaveLayoutNow {
                active_workspace,
                force,
            } => Ok(vec![Self::apply_save_layout_now(
                engine,
                active_workspace,
                force,
            )]),
            #[cfg(feature = "gui")]
            DomainIntent::ApplyPendingLayoutRestore => {
                Ok(vec![Self::apply_apply_pending_layout_restore(engine)])
            }
            // 이 intent 를 적용할 identify worker 가 gui 에만 있고, 만드는 자리도 전부 gui 에만
            // 있다 — 에이전트 경로 `file_handler.dispatch` 는 headless 에서 arm 이 없어 `-32017`
            // 로 거절된다(docs/adr/0425-headless-file-dispatch-answers-that-this-build-cannot-open-files.md).
            #[cfg(feature = "gui")]
            DomainIntent::DispatchFile {
                target,
                depth,
                origin_surface_id,
                dispatch_origin,
                ignore_size_limit,
            } => {
                if let Some(sid) = origin_surface_id {
                    crate::core::origin::require_origin_pane(engine, sid)
                        .map_err(anyhow::Error::msg)?;
                }
                match engine.identify_worker.as_ref() {
                    Some(worker) => {
                        // request id not tracked.
                        worker.spawn_identify(
                            target,
                            depth,
                            origin_surface_id,
                            dispatch_origin,
                            ignore_size_limit,
                        );
                    }
                    None => {
                        tracing::warn!(
                            target = %target.display(),
                            "DispatchFile: identify_worker not injected — drop",
                        );
                    }
                }
                Ok(vec![])
            }
        }
    }
}

#[cfg(test)]
mod mirror_structural_guard_tests {
    //! mirror(원격 attach client) 워크스페이스 누출 차단 (1단계). mirror 워크스페이스의
    //! surface/pane 을 target 으로 한 구조 `DomainIntent` 를 `Core::apply` 로 디스패치하면
    //! 로컬 실행이 거부되고([`MirrorStructuralBlocked`]) **새 로컬 터미널이 insert 되지
    //! 않아야** 한다. 비-mirror 워크스페이스는 그대로 통과(회귀 방지).
    use super::*;
    use crate::core::intent::DomainIntent;
    use crate::model::SplitDirection;
    use tasty_terminal::Terminal;

    /// 테스트용 `Core` — 모든 port 를 mock/in-memory 로 주입. `apply` 의 mirror 가드는
    /// 어떤 port 도 건드리기 전에 반환하므로 실제 PTY/디스크 접근이 없다.
    fn build_test_core() -> (Core, CoreState) {
        use std::sync::{Arc, Mutex};

        use crate::adapters::test::{
            fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
            mock_process::MockProcessSpawner, tmp_home::TmpHome,
        };
        use crate::core::builder::CoreBuilder;
        use crate::ports::notification_sound::NoopPlayer;

        let waker: tasty_terminal::Waker = Arc::new(|| {});
        let engine = CoreState::new(80, 24, waker).expect("engine");

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn tasty_memory::MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn tasty_themes::ThemeStorage> = Arc::new(tasty_themes::ThemeStore::new());

        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(TmpHome::new(
                tempfile::tempdir().expect("tmp").keep(),
            )))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core");
        (core, engine)
    }

    /// 기본 워크스페이스 0 의 단일 surface `a` 에 detached 터미널을 붙이고 `(surface, pane)`
    /// 를 반환. `mirror` 는 호출자가 세팅.
    fn seed(engine: &mut CoreState) -> (u32, u32) {
        let a = engine.workspaces[0].all_surface_ids()[0];
        engine.terminals.insert(a, Terminal::new_detached(80, 24));
        let (_ws, pane) = engine.find_workspace_index_for_surface(a).unwrap();
        (a, pane)
    }

    fn is_blocked(err: &anyhow::Error) -> bool {
        err.downcast_ref::<MirrorStructuralBlocked>().is_some()
    }

    /// mirror 워크스페이스에서 SplitSurface/SplitPane/CreateTab 디스패치 시 거부 +
    /// 새 로컬 터미널 insert 없음. (수정 전이라면 로컬 PTY 가 spawn 돼 count 가 늘어난다.)
    #[test]
    fn mirror_split_and_newtab_are_blocked_without_spawning() {
        let (mut core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        let before = engine.terminals.iter().count();

        for intent in [
            DomainIntent::SplitSurface {
                target_surface_id: a,
                direction: SplitDirection::Horizontal,
                cwd: None,
                kind: "terminal".to_string(),
                surface_params: serde_json::json!({}),
            },
            DomainIntent::SplitPane {
                target_pane_id: pane,
                direction: SplitDirection::Horizontal,
                cwd: None,
                kind: "terminal".to_string(),
                surface_params: serde_json::json!({}),
            },
            DomainIntent::CreateTab {
                pane_id: pane,
                cwd: None,
                kind: "terminal".to_string(),
                name: None,
                surface_params: serde_json::json!({}),
                activate: false,
            },
        ] {
            let err = core
                .apply(&mut engine, intent)
                .expect_err("must be blocked");
            assert!(
                is_blocked(&err),
                "expected MirrorStructuralBlocked, got: {err}"
            );
            assert_eq!(
                engine.terminals.iter().count(),
                before,
                "mirror 워크스페이스에서 새 로컬 터미널이 insert 되면 안 된다"
            );
        }
    }

    /// 비-mirror 워크스페이스는 가드에 걸리지 않는다(회귀 방지). SplitSurface 가
    /// 통과해 새 터미널이 실제로 insert 된다.
    #[test]
    fn non_mirror_split_passes_and_spawns() {
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        assert!(!engine.workspaces[0].mirror);
        let before = engine.terminals.iter().count();

        core.apply(
            &mut engine,
            DomainIntent::SplitSurface {
                target_surface_id: a,
                direction: SplitDirection::Horizontal,
                cwd: None,
                kind: "terminal".to_string(),
                surface_params: serde_json::json!({}),
            },
        )
        .expect("non-mirror split must succeed");
        assert_eq!(
            engine.terminals.iter().count(),
            before + 1,
            "비-mirror split 은 로컬 터미널을 1개 늘려야 한다(회귀)"
        );
    }

    /// attach 로 이미 점유된 workspace 에서 로컬 생성 경로
    /// (create-tab/split-pane/split-surface/adopt-terminal)로 새 터미널 surface 가
    /// 생기면, forward-op 경로(`forward_split_inherits_workspace_occupancy`)와 동형으로
    /// 그 hard 점유를 상속해야 한다 — 등록이 빠지면 attach 클라이언트에 그 새 surface 가
    /// 검정 화면으로만 보인다(스트림 tap 이 시작되지 않으므로).
    #[test]
    fn create_tab_in_occupied_workspace_inherits_occupancy() {
        let (mut core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let client_id = 42;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], client_id)
            .expect("workspace 점유 획득");

        let events = core
            .apply(
                &mut engine,
                DomainIntent::CreateTab {
                    pane_id: pane,
                    cwd: None,
                    kind: "terminal".to_string(),
                    name: None,
                    surface_params: serde_json::json!({}),
                    activate: false,
                },
            )
            .expect("create tab must succeed");
        let Some(CoreEvent::TabCreated { surface_id, .. }) = events.into_iter().next() else {
            panic!("expected TabCreated event");
        };

        assert!(
            engine.attach.is_hard_occupied(surface_id),
            "새 tab 의 터미널은 hard 점유를 상속해야 한다"
        );
        assert_eq!(
            engine.attach.workspace_of_surface(surface_id),
            Some(ws_id),
            "새 surface 는 점유 workspace 멤버로 등록돼야 한다"
        );
        assert_eq!(
            engine.attach.workspace_holder_of(surface_id),
            Some(client_id),
            "새 surface 의 holder 는 workspace holder 와 동일해야 한다"
        );
    }

    /// create-tab 과 동형 — `SplitPane` 경로도 같은 gap 후보였다.
    #[test]
    fn split_pane_in_occupied_workspace_inherits_occupancy() {
        let (mut core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let client_id = 7;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], client_id)
            .expect("workspace 점유 획득");

        let events = core
            .apply(
                &mut engine,
                DomainIntent::SplitPane {
                    target_pane_id: pane,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
            )
            .expect("split pane must succeed");
        let Some(CoreEvent::PaneSplit { new_surface_id, .. }) = events.into_iter().next() else {
            panic!("expected PaneSplit event");
        };

        assert!(engine.attach.is_hard_occupied(new_surface_id));
        assert_eq!(
            engine.attach.workspace_of_surface(new_surface_id),
            Some(ws_id)
        );
        assert_eq!(
            engine.attach.workspace_holder_of(new_surface_id),
            Some(client_id)
        );
    }

    /// create-tab 과 동형 — `SplitSurface` 경로도 같은 gap 후보였다.
    #[test]
    fn split_surface_in_occupied_workspace_inherits_occupancy() {
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let client_id = 9;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], client_id)
            .expect("workspace 점유 획득");

        let events = core
            .apply(
                &mut engine,
                DomainIntent::SplitSurface {
                    target_surface_id: a,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
            )
            .expect("split surface must succeed");
        let Some(CoreEvent::SurfaceSplit { new_surface_id, .. }) = events.into_iter().next() else {
            panic!("expected SurfaceSplit event");
        };

        assert!(engine.attach.is_hard_occupied(new_surface_id));
        assert_eq!(
            engine.attach.workspace_of_surface(new_surface_id),
            Some(ws_id)
        );
        assert_eq!(
            engine.attach.workspace_holder_of(new_surface_id),
            Some(client_id)
        );
    }

    /// adopt-terminal(headless PTY 승격)도 새 surface_id 를 발급하는 생성 경로라 같은
    /// gap 후보였다(문서 "범위" 절 참고 — 실측 재현은 안 됐으나 구조적으로 동일).
    #[test]
    fn adopt_terminal_in_occupied_workspace_inherits_occupancy() {
        use crate::core::pty_registry::PtySpawnSpec;

        let (mut core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        let ws_id = engine.workspaces[0].id;
        let client_id = 13;
        engine
            .attach
            .acquire_workspace(ws_id, &[a], &[a], client_id)
            .expect("workspace 점유 획득");

        let pty_id = engine
            .pty_registry
            .register(
                PtySpawnSpec {
                    owner_agent_id: "agent-x".into(),
                    cwd: None,
                    command: vec![],
                },
                std::time::Instant::now(),
            )
            .expect("register headless pty");
        let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
        let waker = engine.make_waker(pty_id);
        let terminal = tasty_terminal::Terminal::new(
            tasty_terminal::TerminalConfig {
                cols: 80,
                rows: 24,
                shell: sh.shell_ref(),
                args: &sh.args_ref(),
                extra_env: &sh.envs_ref(),
                surface_id: pty_id,
                working_dir: None,
                initial_input: None,
            },
            waker,
        )
        .expect("spawn headless terminal");
        engine.terminals.insert(pty_id, terminal);

        let events = core
            .apply(
                &mut engine,
                DomainIntent::AdoptTerminal {
                    pane_id: pane,
                    pty_id,
                },
            )
            .expect("adopt must succeed");
        let Some(CoreEvent::TabCreated { surface_id, .. }) = events.into_iter().next() else {
            panic!("expected TabCreated event");
        };

        assert!(engine.attach.is_hard_occupied(surface_id));
        assert_eq!(engine.attach.workspace_of_surface(surface_id), Some(ws_id));
        assert_eq!(
            engine.attach.workspace_holder_of(surface_id),
            Some(client_id)
        );

        engine.terminals.remove(surface_id);
    }

    /// 18-c e2e: headless PTY spawn 흉내 → `AdoptTerminal` 승격 → (1) 같은 Terminal
    /// 인스턴스가 pty_id→surface_id 로 re-key 되어 상태 보존, (2) registry 에서 제거,
    /// (3) pane tab 목록에 등장, (4) `TabCreated` cascade 이벤트 발행.
    #[test]
    fn adopt_terminal_promotes_headless_pty_preserving_state() {
        use crate::core::pty_registry::PtySpawnSpec;

        let (mut core, mut engine) = build_test_core();
        let (_a, pane) = seed(&mut engine);

        // pty.spawn 흉내: registry 등록 + 같은 pty_id 로 real Terminal 삽입.
        let pty_id = engine
            .pty_registry
            .register(
                PtySpawnSpec {
                    owner_agent_id: "agent-x".into(),
                    cwd: None,
                    command: vec![],
                },
                std::time::Instant::now(),
            )
            .expect("register headless pty");
        let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
        let waker = engine.make_waker(pty_id);
        let terminal = tasty_terminal::Terminal::new(
            tasty_terminal::TerminalConfig {
                cols: 80,
                rows: 24,
                shell: sh.shell_ref(),
                args: &sh.args_ref(),
                extra_env: &sh.envs_ref(),
                surface_id: pty_id,
                working_dir: None,
                initial_input: None,
            },
            waker,
        )
        .expect("spawn headless terminal");
        engine.terminals.insert(pty_id, terminal);

        // 승격 전에 상태를 만들어 둔다 — 같은 프로세스라면 승격 후에도 화면에 남는다.
        engine
            .find_terminal_by_id_mut(pty_id)
            .expect("headless terminal")
            .send_bytes(b"echo ADOPT_MARKER_123\n");
        let mut seen = false;
        for _ in 0..500 {
            engine.process_surface(pty_id);
            if engine
                .find_terminal_by_id(pty_id)
                .map(|t| t.screen_text(true))
                .unwrap_or_default()
                .contains("ADOPT_MARKER_123")
            {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(seen, "marker should appear before adoption");

        // 승격.
        let events = core
            .apply(
                &mut engine,
                DomainIntent::AdoptTerminal {
                    pane_id: pane,
                    pty_id,
                },
            )
            .expect("adopt must succeed");
        let surface_id = match events.into_iter().next() {
            Some(CoreEvent::TabCreated {
                surface_id,
                pane_id: p,
                ..
            }) => {
                assert_eq!(p, pane, "TabCreated pane_id");
                surface_id
            }
            other => panic!("expected TabCreated cascade event, got {other:?}"),
        };

        // (1) re-key: pty_id 는 사라지고 surface_id 로 옮겨졌으며 상태가 보존됐다.
        assert!(
            engine.find_terminal_by_id(pty_id).is_none(),
            "old pty_id key removed from store"
        );
        let screen = engine
            .find_terminal_by_id(surface_id)
            .expect("terminal now at surface_id")
            .screen_text(true);
        assert!(
            screen.contains("ADOPT_MARKER_123"),
            "state preserved across promotion (same process): {screen:?}"
        );

        // (2) registry 에서 제거 — pty.list 에서 빠지고 이중 등록 방지.
        assert!(
            !engine.pty_registry.contains(pty_id),
            "promoted pty must leave the headless registry"
        );

        // (3) pane tab 목록에 새 surface 등장.
        let pane_ref = engine.find_pane_by_id(pane).expect("pane");
        assert!(
            pane_ref
                .tabs
                .iter()
                .any(|t| t.all_surface_ids().contains(&surface_id)),
            "promoted surface must appear in the pane's tabs"
        );

        // 정리: 승격된 surface 의 Terminal 제거(프로세스 종료).
        engine.terminals.remove(surface_id);
    }

    /// 회귀(waker dedup 게이트 누수): `AdoptTerminal` 승격은 Terminal 을
    /// pty_id→surface_id 로 re-key 하며 새 surface_id 게이트를 배선하므로, 옛 pty_id
    /// 게이트를 `forget_surface` 로 정리해야 한다(미정리 시 승격마다 누적).
    #[test]
    fn adopt_terminal_forgets_old_pty_waker_gate() {
        use crate::adapters::test::mock_waker_factory::RecordingWakerFactory;
        use crate::core::pty_registry::PtySpawnSpec;

        let (mut core, mut engine) = build_test_core();
        let factory = RecordingWakerFactory::new();
        let shared: crate::waker::SharedWakerFactory = factory.clone();
        engine.waker_factory = Some(shared);
        let (_a, pane) = seed(&mut engine);

        // pty.spawn 흉내: registry 등록 + make_waker(pty_id) 로 pty_id 게이트 생성.
        let pty_id = engine
            .pty_registry
            .register(
                PtySpawnSpec {
                    owner_agent_id: "agent-x".into(),
                    cwd: None,
                    command: vec![],
                },
                std::time::Instant::now(),
            )
            .expect("register headless pty");
        let sh = crate::core::state::ShellConfig::from_settings(&engine.settings);
        let waker = engine.make_waker(pty_id);
        let terminal = tasty_terminal::Terminal::new(
            tasty_terminal::TerminalConfig {
                cols: 80,
                rows: 24,
                shell: sh.shell_ref(),
                args: &sh.args_ref(),
                extra_env: &sh.envs_ref(),
                surface_id: pty_id,
                working_dir: None,
                initial_input: None,
            },
            waker,
        )
        .expect("spawn headless terminal");
        engine.terminals.insert(pty_id, terminal);
        assert!(
            factory.made().contains(&pty_id),
            "spawn 흉내는 pty_id 게이트를 만든다"
        );

        // 승격.
        let events = core
            .apply(
                &mut engine,
                DomainIntent::AdoptTerminal {
                    pane_id: pane,
                    pty_id,
                },
            )
            .expect("adopt must succeed");
        let surface_id = match events.into_iter().next() {
            Some(CoreEvent::TabCreated { surface_id, .. }) => surface_id,
            other => panic!("expected TabCreated, got {other:?}"),
        };

        // 옛 pty_id 게이트는 정리, 새 surface_id 게이트(재배선된 활성 게이트)는 보존.
        assert!(
            factory.forgotten().contains(&pty_id),
            "adopt 는 옛 pty_id 의 waker 게이트를 정리해야 한다"
        );
        assert!(
            !factory.forgotten().contains(&surface_id),
            "재배선된 새 surface_id 게이트는 정리 대상이 아니다"
        );

        // 정리: 승격된 surface 의 Terminal 제거.
        engine.terminals.remove(surface_id);
    }

    /// 18-c: 존재하지 않는 pty_id 로 승격 시도는 에러 — store/트리 무변경.
    #[test]
    fn adopt_unknown_pty_errors() {
        let (mut core, mut engine) = build_test_core();
        let (_a, pane) = seed(&mut engine);
        let bogus = crate::core::pty_registry::PTY_ID_BASE + 4242;
        let before = engine.terminals.iter().count();
        let err = core
            .apply(
                &mut engine,
                DomainIntent::AdoptTerminal {
                    pane_id: pane,
                    pty_id: bogus,
                },
            )
            .expect_err("unknown pty must error");
        assert!(err.to_string().contains("not found"), "err: {err}");
        assert_eq!(
            engine.terminals.iter().count(),
            before,
            "실패한 승격은 store 를 건드리지 않아야 한다"
        );
    }

    /// 순수 판별 헬퍼: 모든 구조 variant 가 mirror 워크스페이스 대상일 때 Some,
    /// mirror 플래그가 없으면 None. (구조와 무관한 intent 는 항상 None.)
    #[test]
    fn helper_flags_structural_targets_only_when_mirror() {
        let (_core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        // 두 번째 탭을 추가해 CloseTab/tab 대상 확보.
        let tab_id = engine.next_ids.next_tab();
        let sid1 = engine.next_ids.next_surface();
        engine
            .terminals
            .insert(sid1, Terminal::new_detached(80, 24));
        engine.workspaces[0]
            .pane_layout_mut()
            .find_pane_mut(pane)
            .unwrap()
            .add_terminal_marker_tab(tab_id, sid1);

        let structural = |a: u32, pane: u32, tab_id: u32| {
            vec![
                DomainIntent::SplitSurface {
                    target_surface_id: a,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
                DomainIntent::SplitPane {
                    target_pane_id: pane,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
                DomainIntent::CreateTab {
                    pane_id: pane,
                    cwd: None,
                    kind: "terminal".to_string(),
                    name: None,
                    surface_params: serde_json::json!({}),
                    activate: false,
                },
                DomainIntent::CloseSurface {
                    surface_id: a,
                    save_snapshot: false,
                },
                DomainIntent::ClosePane { pane_id: pane },
                DomainIntent::CloseTab { tab_id },
                DomainIntent::MoveTab {
                    pane_id: pane,
                    from_index: 0,
                    to_index: 1,
                },
                DomainIntent::RestoreClosedItem {
                    target_pane_id: Some(pane),
                    scope: crate::core::intent::RestoreScope::Local,
                },
            ]
        };

        // 비-mirror: 전부 None.
        for intent in structural(a, pane, tab_id) {
            assert_eq!(
                engine.mirror_workspace_index_for_structural(&intent),
                None,
                "비-mirror 는 통과해야 한다: {intent:?}"
            );
        }
        // mirror: 전부 Some(0).
        engine.workspaces[0].mirror = true;
        for intent in structural(a, pane, tab_id) {
            assert_eq!(
                engine.mirror_workspace_index_for_structural(&intent),
                Some(0),
                "mirror 는 차단 대상이어야 한다: {intent:?}"
            );
        }
        // 구조와 무관한 intent 는 mirror 여도 None.
        assert_eq!(
            engine.mirror_workspace_index_for_structural(&DomainIntent::SetTerminalMark {
                surface_id: a
            }),
            None,
        );
    }

    /// 2단계 client 측: mirror split 은 로컬 실행이 차단되면서 forward 큐에 op 를 쌓는다.
    /// op 의 anchor 는 아직 **로컬** surface id(App drain 이 원격으로 치환), forwarded=true.
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_split_enqueues_forward_with_local_anchor() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        assert!(engine.pending_structural_forward.is_empty());

        let err = core
            .apply(
                &mut engine,
                DomainIntent::SplitSurface {
                    target_surface_id: a,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
            )
            .expect_err("mirror split must be blocked locally");
        let blocked = err
            .downcast_ref::<MirrorStructuralBlocked>()
            .expect("MirrorStructuralBlocked");
        assert!(blocked.forwarded, "forward 가능 op 는 forwarded=true");
        assert_eq!(engine.pending_structural_forward.len(), 1);
        let queued = &engine.pending_structural_forward[0];
        assert!(
            !queued.user_triggered,
            "Core::apply 는 origin 을 모르므로 기본 user_triggered=false"
        );
        match &queued.op {
            StructuralOp::SplitSurface { surface_id, .. } => {
                assert_eq!(*surface_id, a, "anchor 는 로컬 surface a");
            }
            other => panic!("expected SplitSurface, got {other:?}"),
        }
    }

    /// SplitPane/NewTab 는 pane 의 대표 surface(활성 탭 focused)를 anchor 로 큐잉한다.
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_split_pane_anchors_on_pane_surface() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        core.apply(
            &mut engine,
            DomainIntent::SplitPane {
                target_pane_id: pane,
                direction: SplitDirection::Vertical,
                cwd: None,
                kind: "terminal".to_string(),
                surface_params: serde_json::json!({}),
            },
        )
        .expect_err("blocked");
        match &engine.pending_structural_forward[0].op {
            StructuralOp::SplitPane {
                anchor_surface_id, ..
            } => assert_eq!(*anchor_surface_id, a, "pane anchor = 활성 탭 surface a"),
            other => panic!("expected SplitPane, got {other:?}"),
        }
    }

    /// mirror 에서 누른 복원은 로컬 실행이 막히고 forward 큐에 op 하나가 쌓인다.
    /// **로컬 복원 스택은 손대지 않는다** — 로컬 pop 은 게이트가 `apply_restore_closed_item`
    /// 호출 전에 돌려주므로 자동으로 막힌다(ADR-0264 결정 2). 그 사실을 고정한다.
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_restore_enqueues_forward_and_leaves_the_local_stack_alone() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, pane) = seed(&mut engine);
        engine.push_closed_item(crate::model::ClosedItem::Surface {
            surface: crate::model::closed_item::ClosedSurface::from_surface_id(9999, None),
            tab_name: "gone".to_string(),
        });
        let before_len = engine.closed_items.len();
        engine.workspaces[0].mirror = true;

        let err = core
            .apply(
                &mut engine,
                DomainIntent::RestoreClosedItem {
                    target_pane_id: Some(pane),
                    scope: crate::core::intent::RestoreScope::Local,
                },
            )
            .expect_err("mirror restore must be blocked locally");
        let blocked = err
            .downcast_ref::<MirrorStructuralBlocked>()
            .expect("MirrorStructuralBlocked");
        assert!(blocked.forwarded, "복원은 forward 대상이다");
        assert_eq!(engine.pending_structural_forward.len(), 1);
        match &engine.pending_structural_forward[0].op {
            StructuralOp::RestoreClosedItem { anchor_surface_id } => {
                assert_eq!(*anchor_surface_id, a, "anchor 는 그 pane 의 대표 surface");
            }
            other => panic!("expected RestoreClosedItem, got {other:?}"),
        }
        assert_eq!(
            engine.closed_items.len(),
            before_len,
            "로컬 스택은 그대로여야 한다 — 로컬 pop 이 일어나면 안 된다"
        );
    }

    /// 대상 pane 을 못 잡으면(워크스페이스가 없어 `target_pane_id` 가 `None`) mirror
    /// 판정 자체가 성립하지 않는다 — 비-mirror 취급으로 떨어져 기존 경로를 탄다.
    #[test]
    fn a_restore_without_a_target_pane_is_not_a_mirror_op() {
        let (mut _core, mut engine) = build_test_core();
        let (_a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        assert_eq!(
            engine.mirror_workspace_index_for_structural(&DomainIntent::RestoreClosedItem {
                target_pane_id: None,
                scope: crate::core::intent::RestoreScope::Local,
            }),
            None,
        );
    }

    /// convert 는 이제 forward 대상이다 — `StructuralOp::ConvertSurface` 로 큐잉되고
    /// (surface_kind/params 전달), forwarded=true(로컬 차단 유지, 원격에 위임).
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_convert_enqueues_forward_with_local_anchor() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        let err = core
            .apply(
                &mut engine,
                DomainIntent::ConvertSurface {
                    surface_id: a,
                    target: crate::core::intent::ConvertSurfaceTarget::Kind {
                        cwd: None,
                        kind: "markdown".to_string(),
                        params: serde_json::json!({ "file": "/tmp/a.md" }),
                    },
                },
            )
            .expect_err("blocked locally");
        let blocked = err
            .downcast_ref::<MirrorStructuralBlocked>()
            .expect("MirrorStructuralBlocked");
        assert!(blocked.forwarded, "convert 는 이제 forward 대상이다");
        assert_eq!(engine.pending_structural_forward.len(), 1);
        match &engine.pending_structural_forward[0].op {
            StructuralOp::ConvertSurface {
                surface_id,
                surface_kind,
                params,
                cwd,
            } => {
                assert_eq!(*surface_id, a, "anchor 는 로컬 surface a");
                assert_eq!(surface_kind, "markdown");
                assert_eq!(params, &serde_json::json!({ "file": "/tmp/a.md" }));
                assert!(
                    cwd.is_none(),
                    "cwd 를 지정하지 않은 convert 는 None 으로 나간다"
                );
            }
            other => panic!("expected ConvertSurface, got {other:?}"),
        }
    }

    /// intent handler 가 source surface 에서 carry 한 cwd 는 forward op 에 그대로
    /// 실린다 — mirror 경로에서 cwd 가 유실되면 explorer root 가 상대경로가 된다.
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_convert_forwards_cwd() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        let err = core
            .apply(
                &mut engine,
                DomainIntent::ConvertSurface {
                    surface_id: a,
                    target: crate::core::intent::ConvertSurfaceTarget::Kind {
                        cwd: Some(std::path::PathBuf::from("/tmp/proj")),
                        kind: "explorer".to_string(),
                        params: serde_json::json!({}),
                    },
                },
            )
            .expect_err("blocked locally");
        assert!(
            err.downcast_ref::<MirrorStructuralBlocked>()
                .expect("MirrorStructuralBlocked")
                .forwarded
        );
        match &engine.pending_structural_forward[0].op {
            StructuralOp::ConvertSurface { cwd, .. } => {
                assert_eq!(cwd.as_deref(), Some("/tmp/proj"));
            }
            other => panic!("expected ConvertSurface, got {other:?}"),
        }
    }

    /// 터미널로 되돌리는 변환(`ConvertSurfaceTarget::Terminal`)도 같은 불변식 대상 —
    /// 원격 PTY 가 홈이 아니라 source cwd 에서 뜨도록 cwd 를 실어보낸다.
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_convert_to_terminal_forwards_cwd() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        core.apply(
            &mut engine,
            DomainIntent::ConvertSurface {
                surface_id: a,
                target: crate::core::intent::ConvertSurfaceTarget::Terminal {
                    cwd: Some(std::path::PathBuf::from("/tmp/proj")),
                },
            },
        )
        .expect_err("blocked locally");
        match &engine.pending_structural_forward[0].op {
            StructuralOp::ConvertSurface {
                cwd, surface_kind, ..
            } => {
                assert_eq!(surface_kind, "terminal");
                assert_eq!(cwd.as_deref(), Some("/tmp/proj"));
            }
            other => panic!("expected ConvertSurface, got {other:?}"),
        }
    }

    /// MoveSurface 는 source/target 이 같은 mirror workspace 안에 있을 때만
    /// forward 된다(결정됨 — cross-workspace 는 로컬 전용 id 유출 위험이라 계속 차단).
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mirror_move_surface_enqueues_forward_when_same_workspace() {
        use tasty_ipc::stream::StructuralOp;
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        // mirror=true 로 세팅하기 전(=로컬 실행 허용될 때) 실제 split 으로 같은
        // workspace 안에 형제 surface b 를 만든다.
        let events = core
            .apply(
                &mut engine,
                DomainIntent::SplitSurface {
                    target_surface_id: a,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
            )
            .expect("split ok");
        let b = match events.into_iter().next() {
            Some(CoreEvent::SurfaceSplit { new_surface_id, .. }) => new_surface_id,
            other => panic!("expected SurfaceSplit, got {other:?}"),
        };
        engine.workspaces[0].mirror = true;

        let err = core
            .apply(
                &mut engine,
                DomainIntent::MoveSurface {
                    source_surface_id: a,
                    target_surface_id: b,
                },
            )
            .expect_err("blocked locally");
        let blocked = err
            .downcast_ref::<MirrorStructuralBlocked>()
            .expect("MirrorStructuralBlocked");
        assert!(
            blocked.forwarded,
            "같은 mirror workspace 안의 move 는 forward 돼야 한다"
        );
        assert_eq!(engine.pending_structural_forward.len(), 1);
        match &engine.pending_structural_forward[0].op {
            StructuralOp::MoveSurface {
                source_surface_id,
                target_surface_id,
            } => {
                assert_eq!(*source_surface_id, a);
                assert_eq!(*target_surface_id, b);
            }
            other => panic!("expected MoveSurface, got {other:?}"),
        }
    }

    /// MoveSurface 가 mirror workspace 와 (다른) 로컬 workspace 경계를 넘으면 forward
    /// 하지 않고 기존 로컬 차단을 유지한다 — target 이 로컬 전용 id 라 그대로 원격에
    /// 보내면 원격 트리의 무관한 surface 와 우연히 겹칠 위험이 있다(결정됨 절 참조).
    #[test]
    fn mirror_move_surface_blocked_when_crossing_workspace_boundary() {
        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;

        // 두 번째(비-mirror, 로컬) workspace 를 만들고 그 안의 surface 를 target 으로 쓴다.
        let ws1_id = engine.next_ids.next_workspace();
        let pane1_id = engine.next_ids.next_pane();
        let tab1_id = engine.next_ids.next_tab();
        let sid1 = engine.next_ids.next_surface();
        engine
            .terminals
            .insert(sid1, Terminal::new_detached(80, 24));
        let ws1 = crate::model::Workspace::new_with_terminal_marker(
            ws1_id,
            "ws1".to_string(),
            pane1_id,
            tab1_id,
            sid1,
        );
        engine.workspaces.push(ws1);

        let err = core
            .apply(
                &mut engine,
                DomainIntent::MoveSurface {
                    source_surface_id: a,
                    target_surface_id: sid1,
                },
            )
            .expect_err("blocked locally");
        let blocked = err
            .downcast_ref::<MirrorStructuralBlocked>()
            .expect("MirrorStructuralBlocked");
        assert!(
            !blocked.forwarded,
            "workspace 경계를 넘는 move 는 forward 하면 안 된다"
        );
        assert!(engine.pending_structural_forward.is_empty());
    }

    /// `mark_last_forward_user_triggered` 는 `forwarded=true` + user origin 일
    /// 때만 마지막 pending forward 를 `user_triggered=true` 로 뒤집는다.
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mark_last_forward_user_triggered_flips_on_user_origin() {
        use crate::intent::{IntentOrigin, UserSource};

        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        let err = core
            .apply(
                &mut engine,
                DomainIntent::SplitSurface {
                    target_surface_id: a,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
            )
            .expect_err("blocked");
        assert!(!engine.pending_structural_forward[0].user_triggered);

        mark_last_forward_user_triggered(
            &mut engine,
            &err,
            &IntentOrigin::User {
                source: UserSource::Shortcut("split_surface_horizontal"),
            },
        );
        assert!(
            engine.pending_structural_forward[0].user_triggered,
            "user origin + forwarded=true 는 뒤집혀야 한다"
        );
    }

    /// agent/IPC origin 이면 forwarded=true 여도 그대로 false 로 남는다(기존 동작
    /// 유지, IPC 경로는 focus 를 옮기지 않아야 하므로).
    // forward 큐에 넣는 것은 gui 뿐이다 — headless 는 거절한다(ADR-0538).
    #[cfg(feature = "gui")]
    #[test]
    fn mark_last_forward_user_triggered_stays_false_on_agent_origin() {
        use crate::intent::IntentOrigin;

        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;
        let err = core
            .apply(
                &mut engine,
                DomainIntent::SplitSurface {
                    target_surface_id: a,
                    direction: SplitDirection::Horizontal,
                    cwd: None,
                    kind: "terminal".to_string(),
                    surface_params: serde_json::json!({}),
                },
            )
            .expect_err("blocked");

        mark_last_forward_user_triggered(
            &mut engine,
            &err,
            &IntentOrigin::Agent {
                source: crate::intent::AgentSource::Ipc,
            },
        );
        assert!(
            !engine.pending_structural_forward[0].user_triggered,
            "agent origin 은 뒤집히면 안 된다"
        );
    }

    /// `forwarded=false`(workspace 경계를 넘는 MoveSurface 등 forward 불가
    /// op)면 origin 이 user 여도 아무것도 건드리지 않는다(애초에 큐가 비어 있으므로
    /// no-op).
    #[test]
    fn mark_last_forward_user_triggered_noop_when_not_forwarded() {
        use crate::intent::{IntentOrigin, UserSource};

        let (mut core, mut engine) = build_test_core();
        let (a, _pane) = seed(&mut engine);
        engine.workspaces[0].mirror = true;

        let ws1_id = engine.next_ids.next_workspace();
        let pane1_id = engine.next_ids.next_pane();
        let tab1_id = engine.next_ids.next_tab();
        let sid1 = engine.next_ids.next_surface();
        engine
            .terminals
            .insert(sid1, Terminal::new_detached(80, 24));
        let ws1 = crate::model::Workspace::new_with_terminal_marker(
            ws1_id,
            "ws1".to_string(),
            pane1_id,
            tab1_id,
            sid1,
        );
        engine.workspaces.push(ws1);

        let err = core
            .apply(
                &mut engine,
                DomainIntent::MoveSurface {
                    source_surface_id: a,
                    target_surface_id: sid1,
                },
            )
            .expect_err("blocked");

        mark_last_forward_user_triggered(
            &mut engine,
            &err,
            &IntentOrigin::User {
                source: UserSource::Shortcut("x"),
            },
        );
        assert!(engine.pending_structural_forward.is_empty());
    }
}
