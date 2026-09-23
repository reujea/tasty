//! `image.*` IPC 핸들러 — host 가 소유한 표면만 담당하는 "얇은 어댑터".
//!
//! `image.open`(ConvertSurface) / `image.list`(surface 순회) 만 host 가 처리한다.
//! 픽셀 편집 계열(`image.save`/`export_png`/`paste`/`next`/`prev`)은 com.tasty.image
//! plugin 이 자기 `image` namespace 에서 직접 처리한다 (namespace forward 가 host
//! 라우터보다 먼저 매칭) — 옛 host `ImagePanel`/`ImageView` 위임 핸들러는 제거됐다.
//!
//! 모든 메서드는 `surface_id`를 명시적으로 받는다 (포커스 독립성 원칙).

use serde_json::{Value, json};

use tasty_ipc::protocol::JsonRpcResponse;

use super::require_surface_id;

/// `image.open { surface_id, path }` — surface를 image kind로 (재)설정 + 파일 로드.
pub fn handle_open(
    core: &mut crate::core::Core,
    engine: &mut crate::core::CoreState,
    id: Value,
    params: &Value,
) -> JsonRpcResponse {
    let sid = match require_surface_id(params, &id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path = match params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p.to_string(),
        None => return JsonRpcResponse::invalid_params(id, "Missing required 'path' parameter"),
    };
    let intent = crate::core::intent::DomainIntent::ConvertSurface {
        surface_id: sid,
        target: crate::core::intent::ConvertSurfaceTarget::Kind {
            cwd: None,
            kind: "image".to_string(),
            params: json!({ "file": path.clone() }),
        },
    };
    let events = match core.apply(engine, intent) {
        Ok(e) => e,
        // mirror surface 대상이면 convert 도 forward 되어 `MirrorStructuralBlocked
        // {forwarded:true}` 로 돌아온다 — 이걸 그냥 `internal_error` 로 뭉개면 실제로는
        // 원격에 정상 큐잉된 요청을 호출자가 실패로 오인한다. 다른 재사용 핸들러(split
        // 등)와 동일하게 `structural_apply_error` 로 `forwarded:true` 를 성공 응답으로
        // 변환한다. 에이전트 요청이므로 그 forward op 의 원격 실패는 사용자 toast 가 아니라
        // 로그로 간다(`docs/adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md`).
        Err(e) => {
            crate::core::mark_last_forward_agent_origin(
                engine,
                &e,
                &crate::core::origin::IntentOrigin::Agent {
                    source: crate::core::origin::AgentSource::Ipc,
                },
            );
            return super::structural_apply_error(id, &e);
        }
    };
    let replaced = matches!(
        events.into_iter().next(),
        Some(crate::core::intent::CoreEvent::SurfaceConverted { replaced: true, .. })
    );
    if !replaced {
        return JsonRpcResponse::invalid_params(id, format!("Surface {sid} not found"));
    }
    JsonRpcResponse::success(id, json!({ "ok": true, "surface_id": sid, "path": path }))
}

/// `image.list` — 열린 모든 image surface 목록.
pub fn handle_list(engine: &crate::core::CoreState, id: Value) -> JsonRpcResponse {
    let mut entries: Vec<Value> = Vec::new();
    for workspace in &engine.workspaces {
        for pid in workspace.pane_layout().all_pane_ids() {
            if let Some(pane) = workspace.pane_layout().find_pane(pid) {
                for tab in &pane.tabs {
                    collect_image_panels(tab.layout(), &mut entries);
                }
            }
        }
    }
    JsonRpcResponse::success(id, json!({ "entries": entries }))
}

fn collect_image_panels(layout: &crate::model::SurfaceLayout, out: &mut Vec<Value>) {
    match layout {
        crate::model::SurfaceLayout::Leaf(surface) => {
            // image 는 B2(ADR-0028)에서 egui-mesh 로 전환돼 host 측 stand-in 은
            // `EguiMeshSurface`(kind=="image")다. dir_count/current_index 는 plugin 이
            // 소유하므로 host list 는 surface_id/path 만 노출한다.
            if let Some(ms) = surface
                .as_any()
                .downcast_ref::<crate::core::egui_mesh_surface::EguiMeshSurface>()
                && ms.kind_static == "image"
            {
                out.push(json!({
                    "surface_id": ms.id,
                    "path": ms.file,
                }));
            }
        }
        crate::model::SurfaceLayout::Split { first, second, .. } => {
            collect_image_panels(first, out);
            collect_image_panels(second, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    /// `handle_open` 처럼 `Core::apply` 를 호출하는 test 용 4-tuple fixture.
    /// `TempDir` 은 호출자가 명명된 binding 으로 받아 즉시 drop 되지 않게 한다.
    /// image kind 는 본래 com.tasty.image plugin 이 hello 시 등록 — 단위 test 는
    /// plugin 을 띄우지 않으므로 host whitelist 등록을 직접 호출한다.
    fn make_test_core_state() -> (
        crate::core::Core,
        AppState,
        crate::core::CoreState,
        tempfile::TempDir,
    ) {
        use std::sync::{Arc, Mutex};
        use tasty_memory::MemoryStorage;
        use tasty_themes::{ThemeStorage, ThemeStore};

        use crate::adapters::test::{
            fake_clock::FakeClock, mem_fs::MemFileSystem, mock_clipboard::MockClipboard,
            mock_process::MockProcessSpawner, tmp_home::TmpHome,
        };
        use crate::core::builder::CoreBuilder;
        use crate::ports::notification_sound::NoopPlayer;

        let term_waker: crate::terminal::Waker = Arc::new(|| {});

        let mut engine = crate::core::CoreState::new(80, 24, term_waker).unwrap();

        let preset_store: Arc<Mutex<tasty_presets::PresetStore>> =
            Arc::new(Mutex::new(tasty_presets::PresetStore::load_default()));
        let memory: Arc<Mutex<dyn MemoryStorage>> =
            Arc::new(Mutex::new(tasty_memory::testing::InMemoryStorage::new()));
        let themes: Arc<dyn ThemeStorage> = Arc::new(ThemeStore::new());

        let state = AppState::new(&mut engine, preset_store.clone(), memory.clone());
        // 런타임과 동형으로 "image" kind 를 egui-mesh stand-in(EguiMeshSurface) 으로
        // 등록한다 (com.tasty.image plugin 이 hello 시 하는 등록의 test 재현).
        let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(json!({
            "kind": "image",
            "display_name_i18n_key": "surface.kind.image",
            "rendering": "egui-mesh",
        }))
        .expect("test SurfaceKindDecl");
        assert!(
            crate::core::surface_registry::egui_mesh::register_egui_mesh_kind(
                &engine.surface_registry,
                "com.tasty.image",
                &decl,
                crate::plugin::manifest::HOST_API_VERSION,
            )
        );

        let home_tmp = tempfile::tempdir().expect("test tempdir");
        let home = TmpHome::new(home_tmp.path().to_path_buf());

        let core = CoreBuilder::new()
            .with_fs(Arc::new(MemFileSystem::new()))
            .with_clock(Arc::new(FakeClock::default()))
            .with_clipboard(Arc::new(MockClipboard::default()))
            .with_process(Arc::new(MockProcessSpawner))
            .with_home(Arc::new(home))
            .with_sound_player(Arc::new(NoopPlayer))
            .with_memory(memory)
            .with_themes(themes)
            .with_preset_store(preset_store)
            .with_settings_storage(Arc::new(tasty_settings::FileSettingsStorage))
            .build()
            .expect("test Core build");

        (core, state, engine, home_tmp)
    }

    fn first_surface_id(state: &mut AppState, engine: &mut crate::core::CoreState) -> u32 {
        let ws_ids: std::collections::HashSet<u32> = state
            .active_workspace_mut(engine)
            .all_surface_ids()
            .into_iter()
            .collect();
        engine
            .terminals
            .iter()
            .find_map(|(sid, _)| ws_ids.contains(&sid).then_some(sid))
            .expect("no terminal surface in active workspace")
    }

    fn write_blank_png(dir: &std::path::Path, name: &str) -> String {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).expect("create png");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer
            .write_image_data(&[0u8; 2 * 2 * 4])
            .expect("png write");
        writer.finish().expect("png finish");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn open_converts_surface_to_image_with_path() {
        let (mut core, mut state, mut engine, _home_tmp) = make_test_core_state();
        let sid = first_surface_id(&mut state, &mut engine);
        let tmp = tempfile::tempdir().unwrap();
        let path = write_blank_png(tmp.path(), "a.png");

        let resp = handle_open(
            &mut core,
            &mut engine,
            Value::Null,
            &json!({ "surface_id": sid, "path": path.clone() }),
        );
        assert!(resp.result.is_some(), "open failed: {resp:?}");
        // 변환 결과는 egui-mesh stand-in — list 로 kind/path 반영을 확인한다.
        let resp = handle_list(&engine, Value::Null);
        let v = resp.result.expect("list ok");
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["surface_id"], sid);
        assert_eq!(entries[0]["path"], path.as_str());
    }

    #[test]
    fn open_rejects_missing_path() {
        let (mut core, mut state, mut engine, _home_tmp) = make_test_core_state();
        let sid = first_surface_id(&mut state, &mut engine);
        let resp = handle_open(
            &mut core,
            &mut engine,
            Value::Null,
            &json!({ "surface_id": sid }),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn open_rejects_unknown_surface() {
        let (mut core, _state, mut engine, _home_tmp) = make_test_core_state();
        let resp = handle_open(
            &mut core,
            &mut engine,
            Value::Null,
            &json!({ "surface_id": 999_999, "path": "/tmp/x.png" }),
        );
        assert!(resp.error.is_some());
    }

    #[test]
    fn list_finds_image_surfaces() {
        let (_core, mut state, mut engine, _home_tmp) = make_test_core_state();
        let sid = first_surface_id(&mut state, &mut engine);
        // Convert to image (blank canvas — no file).
        assert!(state.test_convert_surface_to_kind(&mut engine, sid, "image", &json!({})));

        let resp = handle_list(&engine, Value::Null);
        let v = resp.result.expect("list ok");
        let entries = v["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["surface_id"], sid);
    }
}
