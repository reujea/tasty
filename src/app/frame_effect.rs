//! 그리는 창의 프레임 단계가 내는 효과. 단계는 타입이 정한다.
//!
//! 프레임 drain은 도달한 단계까지의 효과만 적용하고 일반 명령([`super::engine_action::EngineAction`])은
//! 출처 봉투 그대로 일반 dispatch에 넘긴다. 파일 I/O·탐색기 변경 같은 명령은 여기에 두지 않는다.
//! 일반 dispatch는 프레임 밖에서 만든 효과(창 resize 이벤트의 grid 등)도 적용한다. 프레임 사이에는
//! 모든 단계가 이미 지났기 때문이다. 단계 순서는 docs/design/flows/action-dispatch.md를 따른다.
use crate::core::origin::IntentOrigin;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::surface_binding::SurfaceBinding;

/// 프레임 단계. 선언 순서가 실행 순서다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FramePhase {
    /// prepare_redraw 뒤. grid와 tap이 입력 준비 전에 새 크기를 받는다.
    Geometry,
    /// prepare_render_inputs 뒤. 입력·mesh context가 mesh 중계와 draw 전에 전달된다.
    Inputs,
    /// render_if_dirty 뒤. HTML gate와 plugin 표시 context가 finish_redraw·native 동기화 전에 반영된다.
    Draw,
}

#[derive(Clone, Debug)]
pub(crate) enum FrameEffect {
    Geometry(GeometryEffect),
    Inputs(InputEffect),
    Draw(DrawEffect),
}

#[derive(Clone, Debug)]
pub(crate) enum GeometryEffect {
    DefaultGrid {
        cols: usize,
        rows: usize,
    },
    Resize {
        targets: Vec<(SurfaceBinding, LayoutTarget)>,
    },
}

/// 레이아웃 스윕이 surface 하나에 낸 목표 grid. `drawn`은 이 창이 지금 그 surface를 그리는지다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayoutTarget {
    pub(crate) surface_id: u32,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) drawn: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum InputEffect {
    FocusObserved {
        target: SurfaceBinding,
        /// 창이 OS 포커스를 가졌는지. attention은 사용자가 실제로 볼 때만 지운다.
        window_focused: bool,
    },
    LocalMesh {
        target: SurfaceBinding,
        plugin: String,
        registration: crate::runtime::kind_catalog::Registration,
        bootstrap: Option<(String, Option<String>, String)>,
        params: tasty_plugin_protocol::protocol::SurfaceSetContextParams,
    },
    RemoteMesh {
        admitted: std::time::Instant,
        target: SurfaceBinding,
        context: Option<crate::core::state::AttachMeshContextForward>,
        input: Option<tasty_plugin_protocol::protocol::RawInputWire>,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum DrawEffect {
    Html(crate::runtime::html_script::HtmlAction),
    HtmlReceive(crate::runtime::html_receive::ReceiveAction),
    PluginDisplay(super::plugin_display::PluginDisplayRequest),
    RemoteMeshFull { targets: Vec<SurfaceBinding> },
}

impl From<GeometryEffect> for FrameEffect {
    fn from(effect: GeometryEffect) -> Self {
        Self::Geometry(effect)
    }
}
impl From<InputEffect> for FrameEffect {
    fn from(effect: InputEffect) -> Self {
        Self::Inputs(effect)
    }
}
impl From<DrawEffect> for FrameEffect {
    fn from(effect: DrawEffect) -> Self {
        Self::Draw(effect)
    }
}

impl FrameEffect {
    pub(crate) fn phase(&self) -> FramePhase {
        match self {
            Self::Geometry(_) => FramePhase::Geometry,
            Self::Inputs(_) => FramePhase::Inputs,
            Self::Draw(_) => FramePhase::Draw,
        }
    }

    /// 대상 binding이 오래됐으면 조용히 버린다. 관측이 낡은 것이지 사용자 오류가 아니다.
    pub(crate) fn apply(
        &self,
        engine: &mut EngineMut<'_>,
        plugins: Option<&crate::plugin::PluginManager>,
        origin: &IntentOrigin,
    ) {
        match self {
            Self::Geometry(effect) => effect.apply(engine),
            Self::Inputs(effect) => effect.apply(engine, plugins),
            Self::Draw(effect) => effect.apply(engine, plugins, origin),
        }
    }
}

impl GeometryEffect {
    fn apply(&self, engine: &mut EngineMut<'_>) {
        match self {
            Self::DefaultGrid { cols, rows } => {
                engine.runtime.default_cols = *cols;
                engine.runtime.default_rows = *rows;
            }
            Self::Resize { targets } => {
                let targets = targets
                    .iter()
                    .filter(|(binding, _)| binding.current(&engine.as_ref()))
                    .map(|(_, target)| *target)
                    .collect();
                crate::app::services::AppServices::resize_terminals(engine, targets);
            }
        }
    }
}

impl InputEffect {
    fn apply(&self, engine: &mut EngineMut<'_>, plugins: Option<&crate::plugin::PluginManager>) {
        match self {
            Self::FocusObserved {
                target,
                window_focused,
            } => {
                if target.current(&engine.as_ref()) {
                    if *window_focused {
                        engine.clear_attention_local(target.surface_id());
                    }
                    engine.reconcile_soft_occupancy_on_focus(target.surface_id());
                }
            }
            Self::LocalMesh { .. } => self.apply_local_mesh(engine, plugins),
            Self::RemoteMesh { .. } => self.apply_remote_mesh(engine),
        }
    }

    fn apply_local_mesh(
        &self,
        engine: &mut EngineMut<'_>,
        plugins: Option<&crate::plugin::PluginManager>,
    ) {
        let Self::LocalMesh {
            target,
            plugin,
            registration,
            bootstrap,
            params,
        } = self
        else {
            unreachable!("variant-specific effect dispatch")
        };
        if !target.current(&engine.as_ref()) {
            return;
        }
        let Some(kind) = engine
            .core
            .find_surface_by_id(target.surface_id())
            .map(|surface| surface.kind.as_str())
        else {
            return;
        };
        let Some(current) = engine.runtime.surface_registry.get_live(kind) else {
            return;
        };
        if !registration.matches(&current) {
            return;
        }
        let Some(manager) = plugins else {
            return;
        };
        if let Some((kind, file, name)) = bootstrap {
            let Some(binding) = engine
                .runtime
                .surfaces
                .get(&target.surface_id())
                .and_then(|surface| {
                    surface
                        .as_any()
                        .downcast_ref::<crate::runtime::egui_mesh_surface::EguiMeshSurface>()
                })
                .map(|surface| surface.retirement_binding.clone())
            else {
                return;
            };
            manager.send_egui_mesh_surface_create(
                plugin,
                target.surface_id(),
                kind,
                file.as_deref(),
                name,
                &binding,
            );
        }
        manager.send_surface_set_context(plugin, params);
    }

    fn apply_remote_mesh(&self, engine: &mut EngineMut<'_>) {
        let Self::RemoteMesh {
            admitted,
            target,
            context,
            input,
        } = self
        else {
            unreachable!("variant-specific effect dispatch")
        };
        if !target.current(&engine.as_ref()) {
            return;
        }
        if let Some(context) = context {
            engine
                .remote
                .pending_mesh_context_forward
                .insert(target.surface_id(), context.clone());
        }
        if let Some(input) = input {
            engine
                .remote
                .pending_mesh_input_forward
                .entry(target.surface_id())
                .and_modify(|pending| {
                    pending.1.events.extend(input.events.clone());
                    pending.1.focused = input.focused;
                    pending.1.modifiers = input.modifiers;
                    pending.1.time = input.time;
                })
                .or_insert_with(|| (*admitted, input.clone()));
        }
    }
}

impl DrawEffect {
    fn apply(
        &self,
        engine: &mut EngineMut<'_>,
        plugins: Option<&crate::plugin::PluginManager>,
        origin: &IntentOrigin,
    ) {
        match self {
            Self::Html(action) => action.apply(engine, origin),
            Self::HtmlReceive(action) => action.apply(engine, origin),
            Self::PluginDisplay(request) => {
                if let Some(manager) = plugins {
                    request.apply(manager);
                }
            }
            Self::RemoteMeshFull { targets } => {
                for target in targets {
                    if target.current(&engine.as_ref()) {
                        engine
                            .remote
                            .pending_mesh_full_resend_forward
                            .insert(target.surface_id());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameEffect, InputEffect};
    use crate::core::state::attention::AttentionKind;
    use crate::runtime::surface_binding::SurfaceBinding;

    /// 창이 OS 포커스를 잃은 동안 그린 포커스 surface는 attention을 지우지 않는다.
    #[test]
    fn focus_observation_clears_attention_only_while_the_window_is_focused() {
        let mut session = crate::state::tests::test_state().1;
        let sid = session
            .borrow_mut()
            .workspace_at(0)
            .expect("workspace index is valid")
            .all_surface_ids()[0];
        let target = SurfaceBinding::capture(&session.read(), sid).expect("binding");
        session
            .borrow_mut()
            .raise_attention(sid, AttentionKind::NeedsInput);
        let origin = crate::intent::IntentOrigin::User {
            source: crate::intent::UserSource::Menu("render-focus"),
        };

        FrameEffect::from(InputEffect::FocusObserved {
            target: target.clone(),
            window_focused: false,
        })
        .apply(&mut session.borrow_mut(), None, &origin);
        assert_eq!(
            session.borrow_mut().attention_kind(sid),
            Some(AttentionKind::NeedsInput)
        );

        FrameEffect::from(InputEffect::FocusObserved {
            target,
            window_focused: true,
        })
        .apply(&mut session.borrow_mut(), None, &origin);
        assert_eq!(session.borrow_mut().attention_kind(sid), None);
    }
}
