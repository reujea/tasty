//! host 내장 surface의 생성·복원·snapshot 동작을 등록한다.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::model::{
    DagDirection, DagGraphSurface, EmptySurface, ExplorerPanel, ExplorerPreview, ExplorerTab,
    ExplorerViewMode, SortColumn, SortDir, Surface, resolve_root,
};

use super::{
    KindSource, PresetFieldInput, PresetFieldSpec, PresetFieldTarget, RegisteredRendering,
    SurfaceKindDef, SurfaceKindRegistry,
};

pub fn register_builtin_kinds(registry: &SurfaceKindRegistry) {
    register_terminal(registry);
    register_empty(registry);
    register_explorer(registry);
    register_dag_graph(registry);
}

/// plugin 등록 경로가 host 내장 종류를 덮어쓰지 않도록 확인하는 목록.
pub fn is_host_builtin_kind(kind: &str) -> bool {
    matches!(kind, "terminal" | "empty" | "explorer" | "dag_graph")
}

// Terminal의 PTY 생성·복원은 host가 별도로 처리하므로 이 등록의 create/restore는 오류를 반환한다.

fn register_terminal(registry: &SurfaceKindRegistry) {
    registry.register(SurfaceKindDef {
        kind: "terminal",
        rendering: RegisteredRendering::HostEgui,
        source: KindSource::HostBuiltin,
        display_name_i18n_key: "surface.kind.terminal",
        icon: Some("terminal".to_string()),
        create: Arc::new(|_sid, _cwd, _params| {
            anyhow::bail!("terminal surfaces require host-managed PTY spawn; use split_pane_targeted/add_terminal_tab")
        }),
        restore: Arc::new(|_sid, _cwd, _data| {
            anyhow::bail!("terminal surfaces are restored via SavedSurface::Terminal, not Generic")
        }),
        snapshot: Arc::new(|_| None),
        // terminal의 cwd·startup은 params가 아닌 PresetSurface 전용 필드에 저장한다.
        preset_fields: vec![
            PresetFieldSpec {
                id: "cwd".to_string(),
                label_key: "preset.edit.cwd".to_string(),
                target: PresetFieldTarget::Cwd,
                input: PresetFieldInput::Dir,
                required: false,
                placeholder_key: None,
                default: None,
                derive_cwd: false,
            },
            PresetFieldSpec {
                id: "startup".to_string(),
                label_key: "preset.edit.startup".to_string(),
                target: PresetFieldTarget::Startup,
                input: PresetFieldInput::Text,
                required: false,
                placeholder_key: Some("preset.edit.startup_hint".to_string()),
                default: None,
                derive_cwd: false,
            },
        ],
        param_aliases: std::collections::HashMap::new(),
        default_params: std::collections::HashMap::new(),
        // terminal 입력·줌·복사는 이 기능 플래그 대신 별도 PTY/GPU 경로를 쓴다.
        consumes_egui_input: false,
        zoomable: false,
        egui_copy: false,
        copy_path: false,
        egui_paste: false,
        name_from_param: None,
        tab_name_from_params: None,
        records_recent: false,
        convert_requires_input: false,
        convert_input_popup: None,
    });
}

// Explorer는 내부 탭과 보기·정렬을 저장한다. 앞뒤 이동 이력은 저장하지 않는다.

fn register_explorer(registry: &SurfaceKindRegistry) {
    registry.register(SurfaceKindDef {
        kind: "explorer",
        rendering: RegisteredRendering::HostEgui,
        source: KindSource::HostBuiltin,
        display_name_i18n_key: "surface.kind.explorer",
        icon: Some("folder".to_string()),
        create: Arc::new(|sid, cwd, params| {
            let explicit = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from)
                .or_else(|| cwd.map(std::path::PathBuf::from));
            // capture 한 preset 은 snapshot 을 params 로 싣는다. 명시 path·cwd 가 없으면 레이아웃 복원과
            // 같은 방식으로 내부 탭을 되살린다. 명시 값이 있으면 그 폴더 하나로 연다.
            if explicit.is_none() && explorer_snapshot_has_tabs(params) {
                return Ok(crate::runtime::surface_registry::PreparedKind::local(
                    Box::new(explorer_panel_from_snapshot(sid, params)) as Box<dyn Surface>,
                ));
            }
            // 명시 path가 있으면 cwd보다 우선한다. 상대 path는 resolve_root가 기본 경로로 바꾸며 cwd로 재시도하지 않는다.
            let root = resolve_root(explicit);
            let view_mode = params
                .get("view_mode")
                .and_then(|v| v.as_str())
                .map(ExplorerViewMode::from_str)
                .unwrap_or(ExplorerViewMode::Detail);
            let mut panel = ExplorerPanel::new_with_mode(sid, root, view_mode);
            // preset 이 snapshot 의 preview·show_hidden 을 params 로 실어 오면 그대로 쓴다.
            panel.preview = explorer_preview_from_json(params.get("preview"));
            panel.show_hidden = explorer_show_hidden_from_json(params);
            Ok(crate::runtime::surface_registry::PreparedKind::local(
                Box::new(panel) as Box<dyn Surface>,
            ))
        }),
        restore: Arc::new(|sid, _cwd, data| {
            Ok(crate::runtime::surface_registry::PreparedKind::local(
                Box::new(explorer_panel_from_snapshot(sid, data)) as Box<dyn Surface>,
            ))
        }),
        snapshot: Arc::new(|s: &dyn Surface| {
            let ex = s.as_any().downcast_ref::<ExplorerPanel>()?;
            let tabs: Vec<Value> = ex
                .tabs
                .iter()
                .map(|t| {
                    json!({
                        "cwd": t.cwd.to_string_lossy(),
                        "root": t.root.to_string_lossy(),
                        "view_mode": t.view_mode.as_str(),
                        "sort_column": t.sort_column.as_str(),
                        "sort_dir": t.sort_dir.as_str(),
                    })
                })
                .collect();
            let mut snap = json!({ "tabs": tabs, "active": ex.active });
            if let Some(preview) = explorer_preview_to_json(&ex.preview) {
                snap["preview"] = preview;
            }
            // 기본값(끔)이면 키를 싣지 않는다. preset 은 TOML 이라 false 도 굳이 남기지 않는다.
            if ex.show_hidden {
                snap["show_hidden"] = Value::Bool(true);
            }
            Some(snap)
        }),
        // params.path가 없으면 cwd를 쓰므로 편집기는 전용 cwd 필드에 저장한다.
        preset_fields: vec![PresetFieldSpec {
            id: "cwd".to_string(),
            label_key: "preset.edit.cwd".to_string(),
            target: PresetFieldTarget::Cwd,
            input: PresetFieldInput::Dir,
            required: false,
            placeholder_key: None,
            default: None,
            derive_cwd: false,
        }],
        param_aliases: std::collections::HashMap::new(),
        // @home은 호출자가 홈을 제공한 새 탭 경로에서 해석한다. 상속 cwd 경로에서는 해석하지 않는다.
        default_params: std::collections::HashMap::from([
            (
                "view_mode".to_string(),
                "@settings.explorer_view_mode".to_string(),
            ),
            ("path".to_string(), "@home".to_string()),
        ]),
        consumes_egui_input: true,
        zoomable: true,
        egui_copy: false,
        copy_path: true,
        egui_paste: false,
        name_from_param: Some("path".to_string()),
        // path 가 없으면 create 와 같은 순서(cwd → capture 한 탭)로 만들어질 폴더의 이름을 쓴다.
        tab_name_from_params: Some(explorer_tab_name),
        records_recent: false,
        convert_requires_input: false,
        convert_input_popup: None,
    });
}

/// path param 이 없을 때 create 가 열 폴더의 이름. cwd 가 있으면 그 폴더, 없고 capture 한 탭이 있으면
/// 활성 탭의 cwd, 둘 다 없으면 create 가 여는 기본 폴더(홈)다. CLI 는 빠진 값을 null 로 보내므로
/// path·cwd 가 null 인 경우도 여기로 온다. 폴더 이름이 없으면(파일시스템 루트) 종류 표시명을 쓴다.
fn explorer_tab_name(cwd: Option<&std::path::Path>, params: &Value) -> Option<String> {
    let folder = match cwd {
        Some(cwd) => resolve_root(Some(cwd.to_path_buf())),
        None if explorer_snapshot_has_tabs(params) => {
            explorer_panel_from_snapshot(0, params).cwd().to_path_buf()
        }
        None => resolve_root(None),
    };
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

fn explorer_snapshot_has_tabs(data: &Value) -> bool {
    data.get("tabs")
        .and_then(Value::as_array)
        .is_some_and(|tabs| !tabs.is_empty())
}

/// 레이아웃 복원과 capture 한 preset 적용이 함께 쓴다. 탭이 없으면 홈 하나로, 활성 번호가 범위를
/// 넘으면 마지막 탭으로 맞춘다. 탭마다 저장한 뷰 모드를 쓰므로 params 의 `view_mode` 는 보지 않는다.
fn explorer_panel_from_snapshot(sid: u32, data: &Value) -> ExplorerPanel {
    let active = data.get("active").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let tabs: Vec<ExplorerTab> = data
        .get("tabs")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(explorer_tab_from_json).collect())
        .unwrap_or_default();
    let mut panel = ExplorerPanel::from_tabs(sid, tabs, active);
    panel.preview = explorer_preview_from_json(data.get("preview"));
    panel.show_hidden = explorer_show_hidden_from_json(data);
    panel
}

/// 없거나 bool 이 아니면 끔이다.
fn explorer_show_hidden_from_json(data: &Value) -> bool {
    data.get("show_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// 기본값(닫힘·기본 폭)이면 키를 싣지 않는다. 값이 없는 칸도 null 대신 키를 뺀다.
/// preset 은 이 snapshot 을 TOML 로 저장하는데 TOML 에는 null 이 없다.
fn explorer_preview_to_json(preview: &ExplorerPreview) -> Option<Value> {
    if *preview == ExplorerPreview::default() {
        return None;
    }
    let mut out = serde_json::Map::new();
    if preview.open {
        out.insert("open".into(), Value::Bool(true));
    }
    if let Some(width) = preview.width {
        out.insert("width".into(), json!(width.value()));
    }
    Some(Value::Object(out))
}

/// 없거나 형식이 맞지 않는 칸은 기본값으로 둔다. 폭의 범위는 그릴 때 토큰 범위로 다시 제한한다.
fn explorer_preview_from_json(v: Option<&Value>) -> ExplorerPreview {
    let Some(v) = v else {
        return ExplorerPreview::default();
    };
    ExplorerPreview {
        open: v.get("open").and_then(Value::as_bool).unwrap_or(false),
        width: v
            .get("width")
            .and_then(Value::as_f64)
            .map(|w| w as f32)
            .filter(|w| w.is_finite() && *w > 0.0)
            .map(tasty_type_geometry::length::LogicalPx),
    }
}

fn explorer_tab_from_json(v: &Value) -> ExplorerTab {
    // root가 없거나 상대 경로이면 기본 경로로 보정한다.
    let current = resolve_root(
        v.get("root")
            .and_then(|x| x.as_str())
            .map(std::path::PathBuf::from),
    );
    // cwd가 없는 옛 snapshot은 current를 사용하며 상대 cwd도 current로 바꾼다.
    let cwd = v
        .get("cwd")
        .and_then(|x| x.as_str())
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| current.clone());
    let mut tab = ExplorerTab::with_cwd(cwd, current);
    if let Some(m) = v.get("view_mode").and_then(|x| x.as_str()) {
        tab.view_mode = ExplorerViewMode::from_str(m);
    }
    if let Some(c) = v.get("sort_column").and_then(|x| x.as_str()) {
        tab.sort_column = SortColumn::from_str(c);
    }
    if let Some(d) = v.get("sort_dir").and_then(|x| x.as_str()) {
        tab.sort_dir = SortDir::from_str(d);
    }
    tab
}

fn register_empty(registry: &SurfaceKindRegistry) {
    registry.register(SurfaceKindDef {
        kind: "empty",
        rendering: RegisteredRendering::HostEgui,
        source: KindSource::HostBuiltin,
        display_name_i18n_key: "surface.kind.empty",
        icon: None,
        create: Arc::new(|sid, cwd, _params| {
            Ok(crate::runtime::surface_registry::PreparedKind::local(
                Box::new(EmptySurface::new(sid).with_cwd(cwd.map(std::path::PathBuf::from)))
                    as Box<dyn Surface>,
            ))
        }),
        restore: Arc::new(|sid, cwd, _data| {
            Ok(crate::runtime::surface_registry::PreparedKind::local(
                Box::new(EmptySurface::new(sid).with_cwd(cwd.map(std::path::PathBuf::from)))
                    as Box<dyn Surface>,
            ))
        }),
        snapshot: Arc::new(|_| Some(Value::Object(Default::default()))),
        preset_fields: Vec::new(),
        param_aliases: std::collections::HashMap::new(),
        default_params: std::collections::HashMap::new(),
        consumes_egui_input: false,
        zoomable: false,
        egui_copy: false,
        copy_path: false,
        egui_paste: false,
        name_from_param: None,
        tab_name_from_params: None,
        records_recent: false,
        convert_requires_input: false,
        convert_input_popup: None,
    });
}

// DAG 데이터는 host가 보유하므로 내장 surface가 직접 조회한다.
// 대상·방향은 저장하지만 달라진 그래프에 낡은 화면 위치를 적용하지 않도록 줌·팬·선택은 저장하지 않는다.

fn register_dag_graph(registry: &SurfaceKindRegistry) {
    registry.register(SurfaceKindDef {
        kind: "dag_graph",
        rendering: RegisteredRendering::HostEgui,
        source: KindSource::HostBuiltin,
        display_name_i18n_key: "surface.kind.dag_graph",
        icon: Some("git_tree".to_string()),
        create: Arc::new(|sid, _cwd, params| {
            Ok(crate::runtime::surface_registry::PreparedKind::local(
                Box::new(DagGraphSurface::with_target(
                    sid,
                    dag_id_param(params),
                    params.get("workspace_id").and_then(parse_workspace_id),
                    params
                        .get("direction")
                        .and_then(|v| v.as_str())
                        .map(DagDirection::from_str)
                        .unwrap_or_default(),
                )) as Box<dyn Surface>,
            ))
        }),
        restore: Arc::new(|sid, _cwd, data| {
            Ok(crate::runtime::surface_registry::PreparedKind::local(
                Box::new(DagGraphSurface::with_target(
                    sid,
                    dag_id_param(data),
                    data.get("workspace_id").and_then(parse_workspace_id),
                    data.get("direction")
                        .and_then(|v| v.as_str())
                        .map(DagDirection::from_str)
                        .unwrap_or_default(),
                )) as Box<dyn Surface>,
            ))
        }),
        snapshot: Arc::new(|s: &dyn Surface| {
            let dag = s.as_any().downcast_ref::<DagGraphSurface>()?;
            let mut obj = json!({ "direction": dag.direction.as_str() });
            if let Some(id) = &dag.dag_id {
                obj["dag_id"] = json!(id);
            }
            if let Some(ws) = dag.workspace_id {
                obj["workspace_id"] = json!(ws);
            }
            Some(obj)
        }),
        // 프리셋은 관찰 대상만 받는다. 방향은 화면에서 바꾸는 설정이다.
        preset_fields: vec![PresetFieldSpec {
            id: "dag_id".to_string(),
            label_key: "preset.edit.dag_id".to_string(),
            target: PresetFieldTarget::Params("dag_id".to_string()),
            input: PresetFieldInput::Text,
            required: false,
            placeholder_key: Some("preset.edit.dag_id_hint".to_string()),
            default: None,
            derive_cwd: false,
        }],
        param_aliases: std::collections::HashMap::from([("dag".to_string(), "dag_id".to_string())]),
        default_params: std::collections::HashMap::new(),
        consumes_egui_input: true,
        // UI 폰트 줌과 그래프 줌은 다르므로 같은 입력으로 둘을 함께 바꾸지 않는다.
        zoomable: false,
        egui_copy: false,
        copy_path: false,
        egui_paste: false,
        name_from_param: None,
        tab_name_from_params: None,
        records_recent: false,
        convert_requires_input: false,
        convert_input_popup: None,
    });
}

fn dag_id_param(v: &Value) -> Option<String> {
    v.get("dag_id")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_workspace_id(v: &Value) -> Option<u32> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        .and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::abs_path;

    fn registry_with_builtins() -> SurfaceKindRegistry {
        let r = SurfaceKindRegistry::new();
        register_builtin_kinds(&r);
        r
    }

    #[test]
    fn builtin_kinds_no_longer_include_image() {
        let reg = registry_with_builtins();
        assert!(
            reg.get("image").is_none(),
            "image kind must be registered via com.tasty.image plugin, not register_builtin_kinds"
        );
    }

    #[test]
    fn builtin_kinds_no_longer_include_markdown() {
        let reg = registry_with_builtins();
        assert!(
            reg.get("markdown").is_none(),
            "markdown kind must be registered via com.tasty.markdown plugin, not register_builtin_kinds"
        );
    }

    #[test]
    fn empty_snapshot_returns_object() {
        let reg = registry_with_builtins();
        let def = reg.get("empty").unwrap();
        let s = (def.create)(1, None, &json!({}))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let snap = (def.snapshot)(s.as_ref()).unwrap();
        assert!(snap.is_object());
    }

    #[test]
    fn empty_restores_the_saved_cwd() {
        let reg = SurfaceKindRegistry::new();
        register_builtin_kinds(&reg);
        let def = reg.get("empty").unwrap();
        let cwd = std::path::PathBuf::from("/saved-folder");
        let restored = (def.restore)(4, Some(cwd.as_path()), &json!({}))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(restored.source_cwd().as_deref(), Some(cwd.as_path()));
    }

    #[test]
    fn empty_carries_cwd_from_create() {
        let reg = registry_with_builtins();
        let def = reg.get("empty").unwrap();
        let cwd = std::path::PathBuf::from("/tmp/carry-test");
        let s = (def.create)(1, Some(cwd.as_path()), &json!({}))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert_eq!(s.source_cwd().as_deref(), Some(cwd.as_path()));
    }

    #[test]
    fn explorer_is_builtin_and_round_trips() {
        let reg = registry_with_builtins();
        let def = reg
            .get("explorer")
            .expect("explorer is a host builtin kind");
        let root = abs_path("tmp/exp");
        let root_str = root.to_string_lossy().into_owned();
        let s = (def.create)(5, None, &json!({ "path": &root_str }))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert_eq!(s.kind(), "explorer");
        assert_eq!(s.surface_id(), Some(5));
        let snap = (def.snapshot)(s.as_ref()).unwrap();
        assert_eq!(snap["tabs"][0]["cwd"], root_str);
        assert_eq!(snap["tabs"][0]["root"], root_str);
        let restored = (def.restore)(5, None, &snap)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert_eq!(restored.kind(), "explorer");
        let ex = restored
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.current_root(), root.as_path());
        assert_eq!(ex.cwd(), root.as_path());
    }

    #[test]
    fn explorer_preview_rides_the_snapshot_and_a_preset() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").expect("explorer kind");
        let root = abs_path("tmp/exp");
        let mut s = (def.create)(5, None, &json!({ "path": root.to_string_lossy() }))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        // 기본값이면 키를 싣지 않는다.
        let snap = (def.snapshot)(s.as_ref()).unwrap();
        assert!(snap.get("preview").is_none(), "{snap}");

        let set = |s: &mut Box<dyn Surface>, preview: ExplorerPreview| {
            s.as_any_mut()
                .downcast_mut::<ExplorerPanel>()
                .unwrap()
                .preview = preview;
        };
        let get = |s: &dyn Surface| s.as_any().downcast_ref::<ExplorerPanel>().unwrap().preview;
        for preview in [
            ExplorerPreview {
                open: true,
                width: Some(tasty_type_geometry::length::LogicalPx(320.0)),
            },
            ExplorerPreview {
                open: true,
                width: None,
            },
            ExplorerPreview {
                open: false,
                width: Some(tasty_type_geometry::length::LogicalPx(240.0)),
            },
        ] {
            set(&mut s, preview);
            let snap = (def.snapshot)(s.as_ref()).unwrap();
            let restored = (def.restore)(5, None, &snap)
                .and_then(|prepared| prepared.publish())
                .unwrap();
            assert_eq!(get(restored.as_ref()), preview, "{snap}");
            // preset 은 snapshot 을 TOML 로 저장하고 params 로 create 에 넘긴다. null 이 있으면 TOML 이 거절한다.
            let toml_text = toml::to_string(&snap).expect("snapshot is TOML-safe");
            let params: Value = toml::from_str::<toml::Value>(&toml_text)
                .map(|v| serde_json::to_value(v).unwrap())
                .unwrap();
            let created = (def.create)(6, None, &params)
                .and_then(|prepared| prepared.publish())
                .unwrap();
            assert_eq!(get(created.as_ref()), preview, "{params}");
        }

        // 형식이 맞지 않는 값은 기본값으로 둔다.
        let bad = json!({"tabs": [], "active": 0, "preview": {"open": "yes", "width": -3}});
        let restored = (def.restore)(7, None, &bad)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert_eq!(get(restored.as_ref()), ExplorerPreview::default());
    }

    #[test]
    fn explorer_hidden_files_toggle_rides_the_snapshot_and_a_preset() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").expect("explorer kind");
        let root = abs_path("tmp/exp");
        let mut s = (def.create)(5, None, &json!({ "path": root.to_string_lossy() }))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let get = |s: &dyn Surface| {
            s.as_any()
                .downcast_ref::<ExplorerPanel>()
                .unwrap()
                .show_hidden
        };
        // 기본값(끔)이면 키를 싣지 않는다.
        assert!(!get(s.as_ref()));
        let snap = (def.snapshot)(s.as_ref()).unwrap();
        assert!(snap.get("show_hidden").is_none(), "{snap}");

        s.as_any_mut()
            .downcast_mut::<ExplorerPanel>()
            .unwrap()
            .show_hidden = true;
        let snap = (def.snapshot)(s.as_ref()).unwrap();
        let restored = (def.restore)(6, None, &snap)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert!(get(restored.as_ref()), "{snap}");
        let toml_text = toml::to_string(&snap).expect("snapshot is TOML-safe");
        let params: Value = toml::from_str::<toml::Value>(&toml_text)
            .map(|v| serde_json::to_value(v).unwrap())
            .unwrap();
        let created = (def.create)(7, None, &params)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert!(get(created.as_ref()), "{params}");

        // bool 이 아니면 끔이다.
        let bad = json!({"tabs": [], "active": 0, "show_hidden": "yes"});
        let restored = (def.restore)(8, None, &bad)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert!(!get(restored.as_ref()));
    }

    #[test]
    fn old_explorer_json_without_hidden_keeps_the_off_default() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        let root = abs_path("tmp/legacy-explorer");
        let legacy: Value =
            serde_json::from_str(&json!({"tabs": [{"root": root}], "active": 0}).to_string())
                .unwrap();
        assert!(legacy.get("show_hidden").is_none());
        let restored = (def.restore)(9, None, &legacy).unwrap().publish().unwrap();
        let panel = restored.as_any().downcast_ref::<ExplorerPanel>().unwrap();
        assert!(!panel.show_hidden);
        assert!(
            (def.snapshot)(restored.as_ref())
                .unwrap()
                .get("show_hidden")
                .is_none()
        );
    }

    #[test]
    fn a_captured_explorer_preset_reopens_its_inner_tabs() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").expect("explorer kind");
        let project = abs_path("w/project");
        let docs = abs_path("w/project/docs");
        let other = abs_path("w/other");
        let mut s = (def.create)(5, None, &json!({ "path": project.to_string_lossy() }))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        {
            let ex = s.as_any_mut().downcast_mut::<ExplorerPanel>().unwrap();
            ex.active_tab_mut().navigate_to(docs.clone());
            ex.active_tab_mut().view_mode = ExplorerViewMode::Grid;
            ex.add_tab();
            ex.active_tab_mut().set_cwd(other.clone());
            ex.active = 0;
        }
        let snap = (def.snapshot)(s.as_ref()).unwrap();
        // preset 은 snapshot 을 TOML 로 저장했다가 params 로 읽고, 빠진 기본 params 를 채워 create 를 부른다.
        let toml_text = toml::to_string(&snap).unwrap();
        let mut params =
            serde_json::to_value(toml::from_str::<toml::Value>(&toml_text).unwrap()).unwrap();
        params["view_mode"] = json!("detail");

        let created = (def.create)(6, None, &params)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = created.as_any().downcast_ref::<ExplorerPanel>().unwrap();
        assert_eq!(ex.tabs.len(), 2);
        assert_eq!(ex.active, 0);
        assert_eq!(ex.tabs[0].cwd, project);
        assert_eq!(ex.tabs[0].root, docs);
        assert_eq!(
            ex.tabs[0].view_mode,
            ExplorerViewMode::Grid,
            "the tab's own mode, not the injected default"
        );
        assert_eq!(ex.tabs[1].cwd, other);
        assert_eq!(ex.tabs[1].root, other);
        assert_eq!(created.source_cwd().as_deref(), Some(project.as_path()));

        // preset 편집기가 cwd 를 정했거나 path 를 명시하면 그 폴더 하나로 연다.
        for (cwd, path) in [(Some(other.as_path()), None), (None, Some(&other))] {
            let mut explicit = params.clone();
            if let Some(path) = path {
                explicit["path"] = json!(path.to_string_lossy());
            }
            let created = (def.create)(7, cwd, &explicit)
                .and_then(|prepared| prepared.publish())
                .unwrap();
            let ex = created.as_any().downcast_ref::<ExplorerPanel>().unwrap();
            assert_eq!(ex.tabs.len(), 1);
            assert_eq!(ex.cwd(), other.as_path());
            assert_eq!(ex.tabs[0].view_mode, ExplorerViewMode::Detail);
        }

        // 빈 tabs 는 capture 결과가 아니므로 예전처럼 path·cwd·홈 순서로 연다.
        let created = (def.create)(8, Some(other.as_path()), &json!({ "tabs": [] }))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        assert_eq!(created.source_cwd().as_deref(), Some(other.as_path()));
    }

    #[test]
    fn a_captured_explorer_preset_names_its_tab_after_the_folder_create_opens() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").expect("explorer kind");
        let project = abs_path("w/project");
        let other = abs_path("w/other");
        let mut s = (def.create)(5, None, &json!({ "path": project.to_string_lossy() }))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        s.as_any_mut()
            .downcast_mut::<ExplorerPanel>()
            .unwrap()
            .add_tab();
        {
            let ex = s.as_any_mut().downcast_mut::<ExplorerPanel>().unwrap();
            ex.active_tab_mut().set_cwd(other.clone());
        }
        let name = |cwd: Option<&std::path::Path>, params: &Value| {
            crate::runtime::surface_registry::default_tab_name_for_kind(
                "explorer",
                cwd,
                params,
                Some(&def),
            )
        };
        // 활성 탭(두 번째)의 cwd 이름이다.
        let mut params = (def.snapshot)(s.as_ref()).unwrap();
        assert_eq!(name(None, &params), "other");
        params["active"] = json!(0);
        assert_eq!(name(None, &params), "project");
        // create 처럼 cwd·path 가 저장한 탭보다 먼저다.
        assert_eq!(name(Some(other.as_path()), &params), "other");
        let mut with_path = params.clone();
        with_path["path"] = json!(abs_path("w/given").to_string_lossy());
        assert_eq!(name(None, &with_path), "given");
        // 둘 다 없으면 create 가 여는 홈의 이름이다. CLI 처럼 빠진 값을 null 로 보내도 같다.
        let home = crate::model::default_root()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| crate::i18n::t("surface.kind.explorer").to_string());
        assert_eq!(name(None, &json!({})), home);
        assert_eq!(name(None, &json!({"path": null, "cwd": null})), home);
    }

    #[test]
    fn explorer_restore_old_snapshot_without_cwd() {
        let root = abs_path("x");
        let old = json!({
            "tabs": [{"root": root.to_string_lossy(), "view_mode": "detail", "sort_column": "name", "sort_dir": "asc"}],
            "active": 0,
        });
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        let restored = (def.restore)(9, None, &old)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = restored
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.cwd(), root.as_path());
        assert_eq!(ex.current_root(), root.as_path());
    }

    #[test]
    fn explorer_create_defaults_to_cwd() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        let cwd = abs_path("tmp/cwd-default");
        let s = (def.create)(1, Some(cwd.as_path()), &json!({}))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = s
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.current_root(), cwd.as_path());
    }

    #[test]
    fn explorer_create_without_path_or_cwd_falls_back_to_absolute_root() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        let s = (def.create)(1, None, &json!({}))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = s
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        let root = ex.cwd();
        assert!(
            root.is_absolute(),
            "explorer root must never be relative: {root:?}"
        );
        assert_ne!(root, std::path::Path::new("."));
        assert_eq!(root, crate::model::default_root());
    }

    #[test]
    fn explorer_create_rejects_relative_path_and_cwd() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        for s in [
            (def.create)(1, None, &json!({"path": "."}))
                .and_then(|prepared| prepared.publish())
                .unwrap(),
            (def.create)(2, None, &json!({"path": "sub/dir"}))
                .and_then(|prepared| prepared.publish())
                .unwrap(),
            (def.create)(3, Some(std::path::Path::new(".")), &json!({}))
                .and_then(|prepared| prepared.publish())
                .unwrap(),
        ] {
            let root = s
                .as_any()
                .downcast_ref::<crate::model::ExplorerPanel>()
                .unwrap()
                .cwd()
                .to_path_buf();
            assert_eq!(
                root,
                crate::model::default_root(),
                "relative root must fall back"
            );
        }
    }

    #[test]
    fn explorer_create_preserves_explicit_absolute_priority() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        let carry = abs_path("tmp/carry");
        let explicit = abs_path("tmp/explicit");
        let s = (def.create)(
            1,
            Some(carry.as_path()),
            &json!({ "path": explicit.to_string_lossy() }),
        )
        .and_then(|prepared| prepared.publish())
        .unwrap();
        let ex = s
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.cwd(), explicit.as_path());

        let s = (def.create)(2, Some(carry.as_path()), &json!({}))
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = s
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.cwd(), carry.as_path());
    }

    #[test]
    fn explorer_restore_normalizes_relative_snapshot_root() {
        let reg = registry_with_builtins();
        let def = reg.get("explorer").unwrap();
        let home = crate::model::default_root();

        let dotted = json!({"tabs": [{"root": ".", "cwd": "."}], "active": 0});
        let ex = (def.restore)(1, None, &dotted)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = ex
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.cwd(), home.as_path());
        assert_eq!(ex.current_root(), home.as_path());

        let missing = json!({"tabs": [{"view_mode": "detail"}], "active": 0});
        let ex = (def.restore)(2, None, &missing)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = ex
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert!(ex.cwd().is_absolute());
        assert_eq!(ex.cwd(), home.as_path());

        let empty = json!({"tabs": [], "active": 0});
        let ex = (def.restore)(3, None, &empty)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = ex
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert!(ex.cwd().is_absolute());

        let xy = abs_path("x/y");
        let mixed = json!({"tabs": [{"root": xy.to_string_lossy(), "cwd": "."}], "active": 0});
        let ex = (def.restore)(4, None, &mixed)
            .and_then(|prepared| prepared.publish())
            .unwrap();
        let ex = ex
            .as_any()
            .downcast_ref::<crate::model::ExplorerPanel>()
            .unwrap();
        assert_eq!(ex.cwd(), xy.as_path());
        assert_eq!(ex.current_root(), xy.as_path());
    }

    #[test]
    fn terminal_create_errors() {
        let reg = registry_with_builtins();
        let def = reg.get("terminal").unwrap();
        assert!(
            (def.create)(1, None, &json!({}))
                .and_then(|prepared| prepared.publish())
                .is_err()
        );
    }

    #[test]
    fn explorer_capability_flags() {
        let reg = registry_with_builtins();
        let ex = reg.get("explorer").unwrap();
        assert!(ex.consumes_egui_input);
        assert!(ex.zoomable);
        assert!(ex.copy_path);
        assert!(!ex.egui_copy);
        assert!(!ex.egui_paste);
        let term = reg.get("terminal").unwrap();
        assert!(!term.consumes_egui_input);
        assert!(!term.zoomable);
        assert!(!term.copy_path);
    }
}
