//! 플러그인 surface kind를 등록하고 RemoteSurface 생성·복원 요청을 매니저에 전달한다.
//! 이름과 snapshot은 플러그인 응답을 받은 뒤 공유 핸들로 갱신한다.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use crate::model::Surface;

use crate::plugin::manifest::SurfaceKindDecl;
use crate::plugin_bridge::host_cmd::HostCmd;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::surface_registry::{
    KindSource, RegisteredRendering, SurfaceKindDef, SurfaceKindRegistry,
};

// Surface::kind가 static 문자열을 요구한다. 같은 kind를 다시 등록해도 추가 할당하며 해제하지 않는다.
fn leak_kind(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// 같은 등록 함수를 쓰는 remote와 webview를 실제 렌더링 정보에서는 구분한다.
fn registered_rendering(decl: &SurfaceKindDecl) -> RegisteredRendering {
    match decl.rendering {
        crate::plugin::manifest::SurfaceKindRendering::Webview => RegisteredRendering::Webview,
        _ => RegisteredRendering::Remote,
    }
}

/// kind를 등록한다. host_cmd_tx는 surface 생성·복원 요청을 매니저에 전달한다.
pub fn register_remote_kind(
    registry: &SurfaceKindRegistry,
    plugin_id: &str,
    decl: &SurfaceKindDecl,
    host_cmd_tx: Sender<HostCmd>,
) {
    if crate::runtime::surface_registry::builtins::is_host_builtin_kind(&decl.kind) {
        tracing::warn!(
            "plugin '{}' declared remote kind '{}' which is a host builtin; ignoring \
             (host-rendered surface takes precedence)",
            plugin_id,
            decl.kind
        );
        return;
    }

    let kind_static: &'static str = leak_kind(&decl.kind);
    let i18n_key_static: &'static str = leak_str(&decl.display_name_i18n_key);
    let plugin_id_owned = plugin_id.to_string();

    let plugin_id_for_create = plugin_id_owned.clone();
    let plugin_id_for_restore = plugin_id_owned;
    let tx_create = host_cmd_tx.clone();
    let tx_restore = host_cmd_tx;
    let preset_fields =
        crate::runtime::surface_registry::PresetFieldSpec::from_decls(&decl.preset_fields);
    let preset_fields_for_create = preset_fields.clone();
    let preset_fields_for_restore = preset_fields.clone();

    registry.register(SurfaceKindDef {
        kind: kind_static,
        rendering: registered_rendering(decl),
        source: KindSource::Plugin(plugin_id.to_string()),
        display_name_i18n_key: i18n_key_static,
        icon: decl.icon.clone(),
        create: Arc::new(move |sid, cwd, params| {
            let initial_name = params
                .get("display_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| kind_static.to_string());
            let surface =
                RemoteSurface::new(sid, kind_static, plugin_id_for_create.clone(), initial_name)
                    .with_initial_file(params);
            // 선언한 파일 경로에서 얻은 cwd를 상속 cwd보다 우선한다.
            // 이 값은 호스트 source_cwd용이며 플러그인에 보내는 cwd는 원래 인자를 유지한다.
            let surface_cwd = crate::runtime::surface_registry::PresetFieldSpec::derive_cwd(
                &preset_fields_for_create,
                params,
            )
            .or_else(|| cwd.map(std::path::PathBuf::from));
            surface.set_cwd(surface_cwd);
            if kind_static == "html" {
                surface.set_html_source(params.get("url").and_then(serde_json::Value::as_str));
            }
            let handles = surface.handles();
            let command = HostCmd::RemoteSurfaceCreated {
                surface_id: sid,
                plugin_id: plugin_id_for_create.clone(),
                kind: kind_static.to_string(),
                cwd: cwd.map(std::path::PathBuf::from),
                params: params.clone(),
                handles,
            };
            let sender = tx_create.clone();
            Ok(crate::runtime::surface_registry::PreparedKind::deferred(
                Box::new(surface),
                Box::new(move || {
                    sender.send(command).map_err(|error| {
                        anyhow::anyhow!("RemoteSurfaceCreated host command failed: {error}")
                    })
                }),
            ))
        }),
        restore: Arc::new(move |sid, cwd, data| {
            let surface = RemoteSurface::new(
                sid,
                kind_static,
                plugin_id_for_restore.clone(),
                kind_static.to_string(),
            );
            // 생성과 같은 규칙으로 저장 자료의 파일에서 cwd를 얻고, 없으면 저장한 cwd를 쓴다.
            surface.set_cwd(
                crate::runtime::surface_registry::PresetFieldSpec::derive_cwd(
                    &preset_fields_for_restore,
                    data,
                )
                .or_else(|| cwd.map(std::path::PathBuf::from)),
            );
            // 복원 응답을 받기 전에 다시 저장해도 기존 snapshot을 유지한다.
            surface.cache_snapshot(data.clone());
            let handles = surface.handles();
            let command = HostCmd::RemoteSurfaceRestored {
                surface_id: sid,
                plugin_id: plugin_id_for_restore.clone(),
                kind: kind_static.to_string(),
                data: data.clone(),
                handles,
            };
            let sender = tx_restore.clone();
            Ok(crate::runtime::surface_registry::PreparedKind::deferred(
                Box::new(surface),
                Box::new(move || {
                    sender.send(command).map_err(|error| {
                        anyhow::anyhow!("RemoteSurfaceRestored host command failed: {error}")
                    })
                }),
            ))
        }),
        snapshot: Arc::new(|s: &dyn Surface| {
            let any = s.as_any();
            let rs = any.downcast_ref::<RemoteSurface>()?;
            // poison 복구로 snapshot을 유지하고 최초 오류를 기록한다.
            crate::poison::recover_mutex(
                rs.snapshot_cache.lock(),
                super::remote_surface::SNAPSHOT_WHAT,
                &super::remote_surface::SNAPSHOT_POISON_REPORTED,
            )
            .clone()
        }),
        preset_fields,
        param_aliases: decl.param_aliases.clone(),
        default_params: decl.default_params.clone(),
        consumes_egui_input: decl.consumes_egui_input,
        zoomable: decl.zoomable,
        egui_copy: decl.egui_copy,
        copy_path: decl.copy_path,
        egui_paste: decl.egui_paste,
        name_from_param: decl.name_from_param.clone(),
        tab_name_from_params: None,
        records_recent: decl.records_recent,
        convert_requires_input: decl.convert_requires_input,
        convert_input_popup: decl
            .convert_input_popup
            .as_ref()
            .map(|p| format!("{plugin_id}/{p}")),
    });

    tracing::info!(
        "registered remote surface kind '{}' from plugin '{}'",
        kind_static,
        plugin_id
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_same_id_candidate_does_not_publish_or_mutate_the_original_before_install() {
        let registry = SurfaceKindRegistry::new();
        let (sender, receiver) = std::sync::mpsc::channel();
        let decl = serde_json::from_value(serde_json::json!({
            "kind":"test-document", "display_name_i18n_key":"surface.kind.markdown", "rendering":"remote"
        })).unwrap();
        register_remote_kind(&registry, "com.test.document", &decl, sender);
        let definition = registry.get_live("test-document").unwrap();
        let old = (definition.create)(41, None, &serde_json::json!({"display_name":"original"}))
            .unwrap()
            .publish()
            .unwrap();
        let HostCmd::RemoteSurfaceCreated {
            handles: old_handles,
            ..
        } = receiver.try_recv().unwrap()
        else {
            panic!("create")
        };
        let candidate =
            (definition.create)(41, None, &serde_json::json!({"display_name":"candidate"}))
                .unwrap();
        assert!(receiver.try_recv().is_err());
        drop(candidate);
        assert!(receiver.try_recv().is_err());
        assert_eq!(old.display_name(), "original");
        assert_eq!(*old_handles.display_name.lock().unwrap(), "original");
        let candidate =
            (definition.create)(41, None, &serde_json::json!({"display_name":"replacement"}))
                .unwrap();
        candidate.publish().unwrap();
        let HostCmd::RemoteSurfaceCreated {
            handles: new_handles,
            ..
        } = receiver.try_recv().unwrap()
        else {
            panic!("create")
        };
        assert!(!old_handles.binding().matches(&new_handles));
        assert_eq!(*old_handles.display_name.lock().unwrap(), "original");
        assert_eq!(*new_handles.display_name.lock().unwrap(), "replacement");
    }

    fn document_kind(registry: &SurfaceKindRegistry, sender: Sender<HostCmd>) {
        let decl = serde_json::from_value(serde_json::json!({
            "kind": "test-doc", "display_name_i18n_key": "surface.kind.markdown",
            "rendering": "webview",
            "preset_fields": [{ "id": "file", "label_key": "x", "param_key": "file",
                                "input_type": "file_path", "derive_cwd": true }]
        }))
        .unwrap();
        register_remote_kind(registry, "com.test.doc", &decl, sender);
    }

    fn absolute(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(if cfg!(windows) { r"C:\docs" } else { "/docs" }).join(name)
    }

    #[test]
    fn restore_derives_cwd_from_the_snapshot_file_like_create() {
        let registry = SurfaceKindRegistry::new();
        let (sender, _receiver) = std::sync::mpsc::channel();
        document_kind(&registry, sender);
        let def = registry.get_live("test-doc").unwrap();
        let file = absolute("a.md");
        let docs = file.parent().map(std::path::Path::to_path_buf);
        let created = (def.create)(1, None, &serde_json::json!({ "file": file }))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(created.source_cwd(), docs);
        let restored = (def.restore)(2, None, &serde_json::json!({ "file": file }))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(restored.source_cwd(), docs);
    }

    /// 파일이 없는 자료(html HTTP·inline)는 저장한 cwd를 되살리고, 파일이 있으면 파일이 우선한다.
    #[test]
    fn restore_uses_the_saved_cwd_only_without_a_file_to_derive_from() {
        let registry = SurfaceKindRegistry::new();
        let (sender, _receiver) = std::sync::mpsc::channel();
        document_kind(&registry, sender);
        let def = registry.get_live("test-doc").unwrap();
        let file = absolute("a.md");
        let saved = absolute("saved-folder");
        let without_file = (def.restore)(5, Some(saved.as_path()), &serde_json::json!({}))
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(without_file.source_cwd(), Some(saved.clone()));
        let with_file = (def.restore)(
            6,
            Some(saved.as_path()),
            &serde_json::json!({ "file": file }),
        )
        .unwrap()
        .publish()
        .unwrap();
        assert_eq!(
            with_file.source_cwd(),
            file.parent().map(std::path::Path::to_path_buf)
        );
    }

    /// mirror 복원 자료의 파일은 원격 경로라 로컬 cwd가 되면 안 된다.
    #[test]
    fn a_mirror_snapshot_does_not_derive_a_local_cwd() {
        let registry = SurfaceKindRegistry::new();
        let (sender, _receiver) = std::sync::mpsc::channel();
        document_kind(&registry, sender);
        let def = registry.get_live("test-doc").unwrap();
        let mirror = (def.restore)(
            3,
            None,
            &serde_json::json!({ "display_name": "a.md", "remote": { "file": "/srv/a.md" } }),
        )
        .unwrap()
        .publish()
        .unwrap();
        assert_eq!(mirror.source_cwd(), None);
    }

    /// 옛 저장본에 남은 상대 경로의 부모를 호스트 시작 폴더 기준 cwd로 쓰지 않는다.
    #[test]
    fn a_relative_snapshot_file_does_not_derive_a_cwd() {
        let registry = SurfaceKindRegistry::new();
        let (sender, _receiver) = std::sync::mpsc::channel();
        document_kind(&registry, sender);
        let def = registry.get_live("test-doc").unwrap();
        for file in ["a.md", "docs/a.md"] {
            let restored = (def.restore)(4, None, &serde_json::json!({ "file": file }))
                .unwrap()
                .publish()
                .unwrap();
            assert_eq!(restored.source_cwd(), None, "{file}");
        }
    }
}
