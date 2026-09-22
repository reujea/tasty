//! 구조 변경 실행 — split / tab.create / tab.close / tab.move / pane.close / surface.close 의
//! 검증 · `Core::apply` · cascade 를 한 함수씩 소유한다.
//!
//! 이 실행을 부르는 진입점은 둘이다 — IPC 핸들러(`adapters::ipc::handler` 의 `pane` · `tab` ·
//! `surface::close`)와 원격 mirror 가 forward 한 구조 op 의 실행
//! (`core::attach_runtime::execute_forwarded_structural_op`). 둘이 같은 함수를 부르므로 같은
//! 입력에 같은 결과와 같은 실패 문구가 나온다. 예전에는 forward 실행이 IPC 핸들러를
//! 직접 불러 JSON-RPC 응답을 만들게 한 뒤 그 에러 메시지를 되풀었다 — 도메인이 inbound
//! 어댑터를 부르는 방향이었다.
//!
//! **여기 없는 것** — 진입점마다 다른 것은 진입점에 남는다.
//!
//! - wire 변환: 응답 JSON 조립과 실패의 JSON-RPC 코드 선택은 핸들러가 한다
//!   ([`StructuralFailure`] 의 갈래가 코드를 정한다).
//! - 요청 진입 게이트: 호출자 자기 surface/tab/pane 거절, 원격 하드 점유 거절, IPC 요청당
//!   한 번의 권한·cap·rate-limit 판정(ADR-0277)은 IPC 진입점의 일이다. forward 실행은 holder
//!   검증을 이미 지나 들어오고, 그 면제를 params 플래그가 아니라 **어느 함수를 부르느냐**로
//!   표현한다(`handler/surface/close.rs` 의 `refuse_if_hard_occupied` 문서).
//! - forward 전용 단계: anchor resolve, 복원 스택 캡처, 즉시-tap 억제 구간, delta 계산은
//!   `core::attach_runtime` 에 있다.
//!
//! 실행 결과의 plugin 통지와 자원 회수는 `core::structural_cascade` 가 한다.

use std::path::PathBuf;

use serde_json::Value;

use crate::core::cascade_window::CascadeWindow;
use crate::core::intent::{CoreEvent, DomainIntent};
use crate::core::origin::{AgentSource, IntentOrigin};
use crate::core::structural_cascade::{
    PaneSplitCascade, SurfaceCloseCascade, cascade_pane_closed_full, cascade_pane_split,
    cascade_surface_closed, cascade_surface_split, cascade_tab_closed_full, cascade_tab_created,
};
use crate::core::{Core, CoreState};
use crate::model::SplitDirection;

/// 구조 변경 실행의 실패. 갈래가 IPC 응답 코드를 정하고, 문구는 두 진입점이 byte 단위로
/// 같게 받는다.
#[derive(Debug)]
pub(crate) enum StructuralFailure {
    /// 요청이 가리키는 대상·파라미터가 틀렸다 — IPC 는 `invalid_params` 로 답한다.
    Rejected(String),
    /// `Core::apply` 가 성공했는데 약속한 이벤트를 안 냈다 — IPC 는 `internal_error`.
    MissingEvent(&'static str),
    /// `Core::apply` 가 실패했다 — IPC 는 `structural_apply_error` 로 답한다(mirror 로
    /// forward 된 차단은 그 함수가 성공 응답으로 바꾼다).
    Apply(anyhow::Error),
}

impl From<anyhow::Error> for StructuralFailure {
    fn from(e: anyhow::Error) -> Self {
        Self::Apply(e)
    }
}

/// 에이전트 경로의 origin — 포커스를 움직이지 않는다. forward 실행도 이 origin 으로 돈다:
/// 원격 사용자의 조작이 이 기계 앞 사용자의 포커스를 움직이면 안 된다(원칙 1·3). client
/// 쪽 포커스는 client 가 delta 적용 시점에 스스로 보정한다.
fn agent_origin() -> IntentOrigin {
    IntentOrigin::Agent {
        source: AgentSource::Ipc,
    }
}

/// split 의 단위.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitLevel {
    Pane,
    Surface,
}

/// split 요청. `level` · `direction` · 대상은 진입점이 해석해 넘긴다(IPC 는 문자열·nickname
/// 을, forward 는 op 의 타입 값을). 나머지는 `params` 에서 읽는다.
pub(crate) struct SplitRequest<'a> {
    pub(crate) level: SplitLevel,
    pub(crate) direction: SplitDirection,
    pub(crate) target_surface: Option<u32>,
    pub(crate) target_pane: Option<u32>,
    /// 요청의 파라미터 묶음. `cwd` · `meta` · `type` · kind 의 필수 파라미터를 여기서 읽고,
    /// 통째로 새 surface 의 `surface_params` 가 된다.
    pub(crate) params: &'a Value,
}

/// split 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitOutcome {
    Pane {
        new_pane_id: u32,
        new_surface_id: u32,
    },
    Surface {
        new_surface_id: u32,
    },
}

/// split 을 검증하고 실행한다. 대상 해석 · cwd 확인 · kind 필수 파라미터 선검증 · terminal
/// cwd 상속 · `Core::apply` · cascade · `meta` 적용 순서다.
pub(crate) fn split(
    core: &mut Core,
    state: &mut dyn CascadeWindow,
    engine: &mut CoreState,
    req: SplitRequest<'_>,
) -> Result<SplitOutcome, StructuralFailure> {
    let SplitRequest {
        level,
        direction,
        target_surface,
        target_pane,
        params,
    } = req;

    // Validate: at least one target must be specified
    if target_surface.is_none() && target_pane.is_none() {
        return Err(StructuralFailure::Rejected(
            "Missing target. Use 'target_surface' (surface ID or nickname) and/or 'target_pane' (pane ID)".to_string(),
        ));
    }
    // Validate: can't specify both
    if target_surface.is_some() && target_pane.is_some() {
        return Err(StructuralFailure::Rejected(
            "Cannot specify both 'target_surface' and 'target_pane'. Use one.".to_string(),
        ));
    }

    let meta = params.get("meta").and_then(|v| v.as_object());
    let cwd = params
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);
    // 2 차 방어: 호스트가 absolute + valid 만 받는다는 contract 검증.
    if let Some(p) = &cwd
        && !p.is_dir()
    {
        return Err(StructuralFailure::Rejected(format!(
            "cwd does not exist: {}",
            p.display()
        )));
    }
    let kind = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("terminal");

    // 필수 파라미터 선검증 — registry 의 required_params(preset_fields.required)로
    // generic 하게 검증한다(kind 하드코딩 없음).
    if let Some(def) = engine.surface_registry.get_live(kind)
        && let Some(missing) = def.first_missing_required_param(params)
    {
        return Err(StructuralFailure::Rejected(format!(
            "Missing '{missing}' parameter for {kind} type"
        )));
    }

    let outcome = match level {
        SplitLevel::Pane => {
            // pane-level split: target_pane 또는 target_surface 로 pane_id 결정.
            let resolved_pane_id = if let Some(pid) = target_pane {
                pid
            } else if let Some(sid) = target_surface {
                match engine.find_pane_for_surface(sid) {
                    Some(pid) => pid,
                    None => {
                        return Err(StructuralFailure::Rejected(format!(
                            "Surface {sid} not found"
                        )));
                    }
                }
            } else {
                return Err(StructuralFailure::Rejected(
                    "pane-level split requires 'target_pane' or 'target_surface'".to_string(),
                ));
            };

            // terminal 의 cwd inherit — 호출자가 미리 결정 (Core 는 focus state 모름).
            let resolved_cwd = if kind == "terminal" {
                cwd.or_else(|| {
                    let sid = engine
                        .find_pane_by_id(resolved_pane_id)
                        .and_then(|p| p.tabs.get(p.active_tab))
                        .and_then(|t| t.focused_surface_id())?;
                    state.resolve_inherit_cwd_from_surface(engine, sid)
                })
            } else {
                None
            };

            let intent = DomainIntent::SplitPane {
                target_pane_id: resolved_pane_id,
                direction,
                cwd: resolved_cwd,
                kind: kind.to_string(),
                surface_params: params.clone(),
            };
            let events = core.apply(engine, intent)?;
            let Some(CoreEvent::PaneSplit {
                workspace_index,
                original_pane_id,
                new_pane_id,
                new_surface_id,
                direction,
            }) = events.into_iter().next()
            else {
                return Err(StructuralFailure::MissingEvent(
                    "Core::apply returned no PaneSplit event",
                ));
            };

            cascade_pane_split(
                state,
                engine,
                &agent_origin(),
                PaneSplitCascade {
                    workspace_index,
                    original_pane_id,
                    new_pane_id,
                    new_surface_id,
                    direction,
                },
            );
            SplitOutcome::Pane {
                new_pane_id,
                new_surface_id,
            }
        }
        SplitLevel::Surface => {
            let Some(sid) = target_surface else {
                return Err(StructuralFailure::Rejected(
                    "Surface-level split requires 'target_surface', not 'target_pane'".to_string(),
                ));
            };

            // terminal cwd inherit — 호출자가 미리 결정.
            let resolved_cwd = if kind == "terminal" {
                cwd.or_else(|| state.resolve_inherit_cwd_from_surface(engine, sid))
            } else {
                None
            };

            let intent = DomainIntent::SplitSurface {
                target_surface_id: sid,
                direction,
                cwd: resolved_cwd,
                kind: kind.to_string(),
                surface_params: params.clone(),
            };
            let events = core.apply(engine, intent)?;
            let Some(CoreEvent::SurfaceSplit {
                workspace_index,
                pane_id,
                new_surface_id,
            }) = events.into_iter().next()
            else {
                return Err(StructuralFailure::MissingEvent(
                    "Core::apply returned no SurfaceSplit event",
                ));
            };

            cascade_surface_split(
                state,
                engine,
                &agent_origin(),
                workspace_index,
                pane_id,
                new_surface_id,
            );
            SplitOutcome::Surface { new_surface_id }
        }
    };

    let new_surface_id = match outcome {
        SplitOutcome::Pane { new_surface_id, .. } | SplitOutcome::Surface { new_surface_id } => {
            new_surface_id
        }
    };
    apply_meta(state, new_surface_id, meta);
    Ok(outcome)
}

/// Apply metadata key-value pairs to a surface.
fn apply_meta(
    state: &dyn CascadeWindow,
    surface_id: u32,
    meta: Option<&serde_json::Map<String, Value>>,
) {
    if let Some(map) = meta {
        for (key, value) in map {
            if let Some(v) = value.as_str() {
                let result = state.set_surface_meta(surface_id, key, v);
                if let Err(e) = result {
                    tracing::warn!(
                        "surface_meta set failed for surface {surface_id} key '{key}': {e}"
                    );
                }
            }
        }
    }
}

/// 새 탭 결과 — `CoreEvent::TabCreated` 가 실어 온 값 그대로다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabCreated {
    pub(crate) pane_id: u32,
    pub(crate) surface_id: u32,
    pub(crate) tab_count: usize,
    pub(crate) active_tab: usize,
}

/// `pane_id` 에 새 탭을 만든다. `params` 의 `type`(기본 terminal) · `cwd` · `name` 을 읽고,
/// kind 의 fresh-context 기본 파라미터를 채운 사본이 새 surface 의 `surface_params` 가 된다.
/// `activate` 는 새 탭을 활성 탭으로 세우는가다 — 에이전트 진입점은 `false` 를 준다
/// ([ADR-0502](../../docs/adr/0502-an-agent-created-tab-does-not-take-the-users-tab.md)).
pub(crate) fn create_tab(
    core: &mut Core,
    state: &mut dyn CascadeWindow,
    engine: &mut CoreState,
    pane_id: u32,
    params: &Value,
    activate: bool,
) -> Result<TabCreated, StructuralFailure> {
    if engine.find_pane_by_id(pane_id).is_none() {
        return Err(StructuralFailure::Rejected(format!(
            "Pane {} not found",
            pane_id
        )));
    }

    let surface_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("terminal");

    // 새 탭은 상속·carry cwd 컨텍스트가 없다(fresh-context). kind 가 `@home` 같은
    // fresh-context 기본값(예: explorer path)을 선언하면 여기서 주입한다(generic —
    // kind 하드코딩 없음). split/preset/workspace 는 이 경로를 거치지 않아 회귀 없음.
    let mut params = params.clone();
    if let Some(def) = engine.surface_registry.get(surface_type) {
        let home = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf());
        engine.apply_kind_default_params(&def, &mut params, home.as_deref());
    }

    // cwd resolve — terminal 만. explicit > pane active surface 의 inherit.
    let cwd = if surface_type == "terminal" {
        let explicit = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        // 2 차 방어: 호스트가 absolute + valid 만 받는다는 contract 검증.
        if let Some(p) = &explicit
            && !p.is_dir()
        {
            return Err(StructuralFailure::Rejected(format!(
                "cwd does not exist: {}",
                p.display()
            )));
        }
        explicit.or_else(|| {
            let sid = engine
                .find_pane_by_id(pane_id)
                .and_then(|p| p.tabs.get(p.active_tab))
                .and_then(|t| t.focused_surface_id())?;
            state.resolve_inherit_cwd_from_surface(engine, sid)
        })
    } else {
        None
    };

    let tab_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let intent = DomainIntent::CreateTab {
        pane_id,
        cwd,
        kind: surface_type.to_string(),
        name: tab_name,
        surface_params: params,
        activate,
    };
    let events = core.apply(engine, intent)?;

    let Some(CoreEvent::TabCreated {
        pane_id,
        tab_id,
        surface_id,
        tab_count,
        active_tab,
    }) = events.into_iter().next()
    else {
        return Err(StructuralFailure::MissingEvent(
            "Core::apply returned no TabCreated event",
        ));
    };

    // dispatcher 와 같은 cascade 공유 (close_tab ↔ cascade_tab_closed_full 동형) —
    // tab.created/surface.created host event enqueue + baseline 동기화.
    cascade_tab_created(state, engine, pane_id, tab_id, surface_id);

    Ok(TabCreated {
        pane_id,
        surface_id,
        tab_count,
        active_tab,
    })
}

/// close 결과. `closed=false` 는 실패가 아니다 — 대상을 못 찾았거나 마지막 탭·페인이라 닫을
/// 수 없었다는 **답**이고, 두 진입점 모두 성공으로 돌려준다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Closed {
    /// 이벤트가 실어 온 대상 id(요청의 id 와 같다).
    pub(crate) id: u32,
    pub(crate) closed: bool,
}

/// 탭을 닫는다. 에이전트 경로라 `is_user_close=false` 로 cascade 한다.
pub(crate) fn close_tab(
    core: &mut Core,
    state: &mut dyn CascadeWindow,
    engine: &mut CoreState,
    tab_id: u32,
) -> Result<Closed, StructuralFailure> {
    let events = core.apply(engine, DomainIntent::CloseTab { tab_id })?;

    let Some(CoreEvent::TabClosed {
        tab_id,
        pane_id,
        closed,
        cleanup_targets,
    }) = events.into_iter().next()
    else {
        return Err(StructuralFailure::MissingEvent(
            "Core::apply returned no TabClosed event",
        ));
    };

    if closed {
        // 에이전트 origin → is_user_close=false.
        // helper 가 cleanup_surface + surface.closed lifecycle enqueue +
        // tab.closed host event enqueue + baseline 갱신을 일괄 처리한다.
        cascade_tab_closed_full(state, engine, tab_id, pane_id, cleanup_targets, false);
    }
    Ok(Closed { id: tab_id, closed })
}

/// 페인을 닫는다. 없는 페인은 `Rejected` 다(탭과 달리 `Core::apply` 전에 거른다).
pub(crate) fn close_pane(
    core: &mut Core,
    state: &mut dyn CascadeWindow,
    engine: &mut CoreState,
    pane_id: u32,
) -> Result<Closed, StructuralFailure> {
    if engine.find_pane_by_id(pane_id).is_none() {
        return Err(StructuralFailure::Rejected(format!(
            "Pane {} not found",
            pane_id
        )));
    }

    let events = core.apply(engine, DomainIntent::ClosePane { pane_id })?;
    let Some(CoreEvent::PaneClosed {
        pane_id,
        closed,
        cleanup_targets,
    }) = events.into_iter().next()
    else {
        return Err(StructuralFailure::MissingEvent(
            "Core::apply returned no PaneClosed event",
        ));
    };

    if closed {
        // 에이전트 origin → is_user_close=false.
        // helper 가 cleanup_surface + surface.closed lifecycle enqueue +
        // pane.closed host event enqueue 를 일괄 처리한다.
        cascade_pane_closed_full(state, engine, pane_id, cleanup_targets, false);
    }
    Ok(Closed {
        id: pane_id,
        closed,
    })
}

/// 탭을 한 페인 안에서 옮긴다. 결과는 실제로 옮겼는가다.
pub(crate) fn move_tab(
    core: &mut Core,
    engine: &mut CoreState,
    pane_id: u32,
    from_index: usize,
    to_index: usize,
) -> Result<bool, StructuralFailure> {
    let events = core.apply(
        engine,
        DomainIntent::MoveTab {
            pane_id,
            from_index,
            to_index,
        },
    )?;
    Ok(matches!(
        events.into_iter().next(),
        Some(CoreEvent::TabMoved { moved: true, .. })
    ))
}

/// surface 를 닫는다. 워크스페이스가 비면 cascade 가 기본 워크스페이스를 다시 만든다.
///
/// `save_snapshot` 은 **진입점이 정한다.** IPC 요청은 에이전트 경로라 `false` 이고(되돌리기
/// 스택은 사용자 행동의 것이다), forward 된 holder 의 close 는 op 의 origin 이 정한다 —
/// `User` 면 `true`, `Agent` 면 `false`
/// (`docs/adr/0480-a-forwarded-close-carries-who-asked-for-it.md`). 그 축을 params 가
/// 아니라 인자로 받는 이유는 IPC 진입점의 `refuse_if_hard_occupied` 문서에 있다 — params 는
/// 호출자가 만들므로 데이터로 두면 아무 에이전트나 같은 키를 실어 사용자 스택을 채운다.
///
/// `is_user_close` 는 **독립 축**이라 여기서 함께 뒤집지 않는다(plugin lifecycle 이벤트에
/// 나가는 값 — `src/state/tests.rs` 의
/// `pty_exit_close_skips_the_snapshot_but_still_reports_a_user_close` 가 두 축이 별개임을
/// 고정한다). forward 된 close 도 `is_user_close=false` 로 나간다.
pub(crate) fn close_surface(
    core: &mut Core,
    state: &mut dyn CascadeWindow,
    engine: &mut CoreState,
    surface_id: u32,
    save_snapshot: bool,
) -> Result<Closed, StructuralFailure> {
    let intent = DomainIntent::CloseSurface {
        surface_id,
        save_snapshot,
    };
    let events = core.apply(engine, intent)?;
    let Some(event @ CoreEvent::SurfaceClosed { surface_id, .. }) = events.into_iter().next()
    else {
        return Err(StructuralFailure::MissingEvent(
            "Core::apply returned no SurfaceClosed event",
        ));
    };

    // `None` = closed=false — 회수할 것이 없다. is_user_close=false — 에이전트 경로.
    // cleanup_targets 의 모든 surface 에 대한 lifecycle enqueue 는 cascade 가 처리한다.
    let Some(c) = SurfaceCloseCascade::from_surface_closed(event, false) else {
        return Ok(Closed {
            id: surface_id,
            closed: false,
        });
    };
    cascade_surface_closed(core, state, engine, c);
    Ok(Closed {
        id: surface_id,
        closed: true,
    })
}
