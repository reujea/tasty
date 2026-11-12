//! Application inputs for kind materialization. Surface IDs are targets, never focus aliases.
use crate::app::command::{ConvertSurfaceTarget, DomainIntent};
use serde::{Deserialize, Serialize};

/// CLI가 상대 `file`과 함께 보내는 호출자 폴더의 키.
const FILE_BASE_KEY: &str = "file_base";

/// 상대 `file`을 호출자 폴더 기준 절대 경로로 바꾸고 기준 키를 지운다. `local`이 거짓이면 file은
/// 원격 경로이므로 그대로 두고, 이쪽 폴더인 기준 키만 지워 플러그인과 원격에 넘기지 않는다.
pub(super) fn resolve_relative_file(params: &mut serde_json::Value, local: bool) {
    let Some(object) = params.as_object_mut() else {
        return;
    };
    let Some(base) = object.remove(FILE_BASE_KEY) else {
        return;
    };
    let (true, Some(base)) = (local, base.as_str()) else {
        return;
    };
    let Some(file) = object.get("file").and_then(|file| file.as_str()) else {
        return;
    };
    if file.is_empty() || std::path::Path::new(file).is_absolute() {
        return;
    }
    // components는 `./a.md`의 `.`을 없앤다. 그대로 두면 폴더와 최근 파일 경로에 `.`이 남는다.
    let path: std::path::PathBuf = std::path::Path::new(base).join(file).components().collect();
    object.insert(
        "file".into(),
        serde_json::Value::from(path.to_string_lossy().into_owned()),
    );
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum Destination {
    Adopt {
        pane: u32,
        pty: u32,
    },
    Workspace {
        name: Option<String>,
        subtitle: Option<String>,
        description: Option<String>,
        category: Option<u32>,
    },
    Tab {
        pane: u32,
        name: Option<String>,
        activate: bool,
    },
    Pane {
        target: u32,
        direction: crate::model::SplitDirection,
    },
    Surface {
        target: u32,
        direction: crate::model::SplitDirection,
    },
    Convert {
        surface: u32,
        respawn: bool,
    },
}

/// 생성 요청의 cwd 상속 원본. 원격 holder 요청만 `View` 외의 값을 가진다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CwdSource {
    /// 이 인스턴스 View의 선택을 이 인스턴스의 `inherit_cwd` 설정으로 상속한다. 로컬 요청의 규칙이며,
    /// 원격 요청이 기준을 싣지 않았을 때의 fallback이다.
    View,
    /// 요청이 지목한 surface의 실행 시점 cwd. 요청자가 이미 상속을 판단했으므로 이 인스턴스의 설정으로 막지 않는다.
    Surface(u32),
    /// 상속하지 않는다. View 선택도 쓰지 않는다.
    NoInherit,
}

impl CwdSource {
    /// 원격 요청이 실은 값. 모르는 값은 접수 단계에서 거절했으므로 여기 오지 않는다.
    pub fn of_remote(value: Option<&tasty_ipc::stream::InheritCwd>) -> Self {
        use tasty_ipc::stream::InheritCwd;
        match value {
            Some(InheritCwd::Surface(surface)) => CwdSource::Surface(*surface),
            Some(InheritCwd::NoInherit | InheritCwd::Unknown(_)) => CwdSource::NoInherit,
            None => CwdSource::View,
        }
    }
}

/// View가 pane에서 고른 탭의 선택 surface. 기록이 없거나 낡았으면 첫 탭·첫 surface다.
pub(super) fn selected_in_view(
    core: &crate::core::CoreState,
    view: &crate::runtime::journal_product::CompletionView,
    pane: u32,
) -> Option<u32> {
    let pane = core.find_pane_by_id(pane)?;
    let tab = view
        .selected_tabs
        .get(&pane.id)
        .and_then(|id| pane.tabs.iter().find(|tab| tab.id == *id))
        .or_else(|| pane.tabs.first())?;
    view.selected_surfaces
        .get(&tab.id)
        .copied()
        .filter(|id| tab.contains_surface(*id))
        .or_else(|| tab.first_surface_id())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Spec {
    pub destination: Destination,
    pub kind: String,
    pub cwd: Option<std::path::PathBuf>,
    pub params: serde_json::Value,
}

impl Spec {
    pub fn from_intent(intent: &DomainIntent) -> Option<Self> {
        let (destination, kind, cwd, params) = match intent {
            DomainIntent::CreateWorkspace {
                cwd,
                kind,
                surface_params,
                name,
                subtitle,
                description,
                category,
            } => (
                Destination::Workspace {
                    name: name.clone(),
                    subtitle: subtitle.clone(),
                    description: description.clone(),
                    category: *category,
                },
                kind.clone(),
                cwd.clone(),
                surface_params.clone(),
            ),
            DomainIntent::CreateTab {
                pane_id,
                cwd,
                kind,
                name,
                surface_params,
                activate,
            } => (
                Destination::Tab {
                    pane: *pane_id,
                    name: name.clone(),
                    activate: *activate,
                },
                kind.clone(),
                cwd.clone(),
                surface_params.clone(),
            ),
            DomainIntent::SplitPane {
                target_pane_id,
                direction,
                cwd,
                kind,
                surface_params,
            } => (
                Destination::Pane {
                    target: *target_pane_id,
                    direction: *direction,
                },
                kind.clone(),
                cwd.clone(),
                surface_params.clone(),
            ),
            DomainIntent::SplitSurface {
                target_surface_id,
                direction,
                cwd,
                kind,
                surface_params,
            } => (
                Destination::Surface {
                    target: *target_surface_id,
                    direction: *direction,
                },
                kind.clone(),
                cwd.clone(),
                surface_params.clone(),
            ),
            DomainIntent::ConvertSurface { surface_id, target } => {
                let (kind, cwd, params) = match target {
                    ConvertSurfaceTarget::Terminal { cwd } => {
                        ("terminal".into(), cwd.clone(), serde_json::json!({}))
                    }
                    ConvertSurfaceTarget::Kind { cwd, kind, params } => {
                        (kind.clone(), cwd.clone(), params.clone())
                    }
                };
                (
                    Destination::Convert {
                        surface: *surface_id,
                        respawn: false,
                    },
                    kind,
                    cwd,
                    params,
                )
            }
            _ => return None,
        };
        Some(Self {
            destination,
            kind,
            cwd,
            params,
        })
    }
}

impl Spec {
    pub fn from_public(
        request: &crate::ipc::protocol::JsonRpcRequest,
        engine: &crate::runtime::engine_access::EngineRef<'_>,
        view: &crate::runtime::journal_product::CompletionView,
        source: CwdSource,
    ) -> Result<Self, crate::ipc::protocol::JsonRpcResponse> {
        use crate::ipc::{handler::params, protocol::JsonRpcResponse};
        let id = serde_json::Value::Null;
        let bad = |message: String| JsonRpcResponse::invalid_params(id.clone(), message);
        let mut input = request.params.clone();
        let mut kind = input
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("terminal")
            .to_owned();
        let explicit_cwd = input
            .get("cwd")
            .and_then(|value| value.as_str())
            .map(std::path::PathBuf::from);
        let selected = |pane: u32| selected_in_view(engine.core, view, pane);
        // `selected`는 View 규칙의 상속 원본이다. 원격 요청이 원본을 정했으면 그 값만 쓴다.
        let inherit = |selected: Option<u32>| match source {
            CwdSource::View if engine.runtime.settings.general.inherit_cwd => {
                selected.and_then(|surface| engine.local_surface_cwd(surface))
            }
            CwdSource::View | CwdSource::NoInherit => None,
            CwdSource::Surface(surface) => engine.local_surface_cwd(surface),
        };
        let (destination, cwd) = match request.method.as_str() {
            "image.open" => {
                let surface = params::require_u32(&input, "surface_id", &id)?;
                let path = input
                    .get("path")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| bad("Missing required 'path' parameter".into()))?
                    .to_owned();
                kind = "image".into();
                input = serde_json::json!({"file":path});
                (
                    Destination::Convert {
                        surface,
                        respawn: false,
                    },
                    None,
                )
            }
            "pty.attach_surface" => {
                kind = "terminal".into();
                (
                    Destination::Adopt {
                        pane: params::require_u32(&input, "pane_id", &id)?,
                        pty: params::require_u32(&input, "id", &id)?,
                    },
                    None,
                )
            }
            "tab.create" => {
                let pane = params::require_u32(&input, "pane_id", &id)?;
                if engine.find_pane_by_id(pane).is_none() {
                    return Err(bad(format!("Pane {pane} not found")));
                }
                if let Some(definition) = engine.runtime.surface_registry.get(&kind) {
                    // 명시 cwd 가 있으면 @home 기본값이 그 cwd 를 가리지 않게 홈을 주지 않는다.
                    let home = explicit_cwd.is_none().then(|| {
                        directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
                    });
                    engine.apply_kind_default_params(
                        &definition,
                        &mut input,
                        home.flatten().as_deref(),
                    );
                }
                // 명시 cwd 는 kind 와 관계없이 넘긴다. 상속은 terminal 만 한다.
                let cwd = if kind == "terminal" {
                    explicit_cwd.or_else(|| inherit(selected(pane)))
                } else {
                    explicit_cwd
                };
                (
                    Destination::Tab {
                        pane,
                        name: input
                            .get("name")
                            .and_then(|value| value.as_str())
                            .map(str::to_owned),
                        activate: false,
                    },
                    cwd,
                )
            }
            "split" => {
                let surface = crate::ipc::handler::pane::resolve_surface_target(&input);
                let pane = params::optional_u32(&input, "target_pane", &id)?;
                if surface.is_none() && pane.is_none() {
                    return Err(bad("Missing target. Use 'target_surface' (surface ID or nickname) and/or 'target_pane' (pane ID)".into()));
                }
                if surface.is_some() && pane.is_some() {
                    return Err(bad(
                        "Cannot specify both 'target_surface' and 'target_pane'. Use one.".into(),
                    ));
                }
                if let Some(path) = &explicit_cwd
                    && !path.is_dir()
                {
                    return Err(bad(format!("cwd does not exist: {}", path.display())));
                }
                if let Some(definition) = engine.runtime.surface_registry.get_live(&kind)
                    && let Some(missing) = definition.first_missing_required_param(&input)
                {
                    return Err(bad(format!(
                        "Missing '{missing}' parameter for {kind} type"
                    )));
                }
                let direction = match input.get("direction").and_then(|value| value.as_str()) {
                    Some("horizontal" | "h") => crate::model::SplitDirection::Horizontal,
                    _ => crate::model::SplitDirection::Vertical,
                };
                match input.get("level").and_then(|value| value.as_str()) {
                    Some("pane" | "pane-group") => {
                        let target = pane
                            .or_else(|| {
                                surface.and_then(|surface| engine.find_pane_for_surface(surface))
                            })
                            .ok_or_else(|| {
                                bad(format!("Surface {} not found", surface.unwrap_or(0)))
                            })?;
                        (
                            Destination::Pane { target, direction },
                            if kind == "terminal" {
                                explicit_cwd.or_else(|| inherit(selected(target)))
                            } else {
                                explicit_cwd
                            },
                        )
                    }
                    Some("surface") => {
                        let target = surface.ok_or_else(|| {
                            bad(
                                "Surface-level split requires 'target_surface', not 'target_pane'"
                                    .into(),
                            )
                        })?;
                        (
                            Destination::Surface { target, direction },
                            if kind == "terminal" {
                                explicit_cwd.or_else(|| inherit(Some(target)))
                            } else {
                                explicit_cwd
                            },
                        )
                    }
                    Some(other) => {
                        return Err(bad(format!("Invalid level '{other}'. Use: pane, surface")));
                    }
                    None => return Err(bad("Missing 'level' parameter".into())),
                }
            }
            "surface.respawn_terminal" => {
                kind = "terminal".into();
                let surface = params::require_u32(&input, "surface_id", &id)?;
                if engine.runtime.terminals.get(surface).is_none() {
                    return Err(bad(format!("Surface {surface} is not a terminal")));
                }
                (
                    Destination::Convert {
                        surface,
                        respawn: true,
                    },
                    explicit_cwd,
                )
            }
            _ => return Err(bad("unsupported kind creation method".into())),
        };
        if let Some(path) = &cwd
            && !path.is_dir()
        {
            return Err(bad(format!("cwd does not exist: {}", path.display())));
        }
        let mirror = match &destination {
            Destination::Tab { pane, .. } | Destination::Pane { target: pane, .. } => {
                selected(*pane).is_some_and(|surface| engine.is_mirror_surface(surface))
            }
            Destination::Surface { target, .. } => engine.is_mirror_surface(*target),
            _ => false,
        };
        resolve_relative_file(&mut input, !mirror);
        Ok(Self {
            destination,
            kind,
            cwd,
            params: input,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Destination, Spec, resolve_relative_file};
    use serde_json::json;

    fn base() -> std::path::PathBuf {
        std::path::PathBuf::from(if cfg!(windows) { r"C:\work" } else { "/work" })
    }

    #[test]
    fn a_relative_file_resolves_against_the_caller_folder() {
        for (file, expected) in [
            ("a.md", base().join("a.md")),
            ("./a.md", base().join("a.md")),
            ("docs/a.md", base().join("docs").join("a.md")),
        ] {
            let mut params = json!({"file": file, "file_base": base()});
            resolve_relative_file(&mut params, true);
            assert_eq!(params, json!({"file": expected}), "{file}");
        }
    }

    #[test]
    fn an_absolute_file_or_a_missing_base_stays_unchanged() {
        let absolute = base().join("a.md");
        let mut params = json!({"file": absolute, "file_base": "/elsewhere"});
        resolve_relative_file(&mut params, true);
        assert_eq!(params, json!({"file": absolute}));
        let mut params = json!({"file": "a.md"});
        resolve_relative_file(&mut params, true);
        assert_eq!(params, json!({"file": "a.md"}));
    }

    /// mirror 대상의 file은 원격 경로이고 기준 키는 이쪽 폴더라 원격에 보내지 않는다.
    #[test]
    fn a_mirror_target_keeps_the_remote_file_and_drops_the_local_base() {
        let mut params = json!({"file": "a.md", "file_base": base()});
        resolve_relative_file(&mut params, false);
        assert_eq!(params, json!({"file": "a.md"}));
    }

    fn view() -> crate::runtime::journal_product::CompletionView {
        crate::runtime::journal_product::CompletionView {
            mirror_count: 0,
            focused_panes: Default::default(),
            selected_tabs: Default::default(),
            selected_surfaces: Default::default(),
        }
    }

    /// 대상이 어느 workspace에 있는지로 file을 바꿀지 정한다.
    #[test]
    fn the_target_workspace_decides_whether_the_file_is_resolved() {
        let (mut state, mut session) = crate::state::tests::test_state();
        let mut engine = session.borrow_mut();
        let mirror = crate::state::tests::add_mirror_test_workspace(&mut state, &mut engine);
        let local = engine
            .core
            .local_workspaces()
            .first()
            .expect("local workspace")
            .all_surface_ids()[0];
        for (surface, expected) in [(local, json!(base().join("a.md"))), (mirror, json!("a.md"))] {
            let request: crate::ipc::protocol::JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "split",
                "id": null,
                "params": {
                    "level": "surface", "target_surface": surface, "type": "empty",
                    "file": "a.md", "file_base": base(),
                },
            }))
            .expect("request");
            let spec =
                Spec::from_public(&request, &engine.as_ref(), &view(), super::CwdSource::View)
                    .expect("spec");
            assert!(matches!(spec.destination, Destination::Surface { .. }));
            assert_eq!(spec.params["file"], expected, "surface {surface}");
            assert!(spec.params.get("file_base").is_none());
        }
    }
}
