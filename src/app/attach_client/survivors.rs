//! Preserve mirror IDs and runtime instances while applying server descriptor kinds.

#[cfg(test)]
mod tests;

use super::projection::{MirrorMarkdownLeaves, MirrorMeshInfo};
use super::resources::{
    MARKDOWN_MIRROR_KIND, create_mirror_markdown_surface, deferred_mirror_markdown_surface,
    make_mirror_surface, markdown_mirror_available,
};
use crate::model::{EmptySurface, TerminalSurface};
use crate::runtime::engine_access::EngineMut;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tasty_remote::client_session::SharedFrameSender;

fn mirror_descriptor_kind(s: &Value, markdown_available: bool, html_available: bool) -> &str {
    match s.get("role").and_then(Value::as_str) {
        Some(role @ ("terminal" | "explorer")) => role,
        Some("mesh") => s.get("kind").and_then(Value::as_str).unwrap_or("mesh"),
        Some("html") if html_available => "html",
        Some("markdown") if markdown_available => MARKDOWN_MIRROR_KIND,
        _ => "empty",
    }
}

/// 재연결에서 살아남은 terminal mirror의 grid를 descriptor가 알린 서버 크기로 맞춘다.
///
/// mirror grid는 서버 echo로만 바뀌므로, 끊긴 연결과 함께 사라진 echo가 있으면 재연결 뒤에도
/// 서버 grid와 다르게 남는다. 재연결 descriptor는 새 snapshot과 같은 순간의 크기이고 새 연결의
/// 프레임보다 먼저 적용되므로, 여기서 맞추면 snapshot이 맞는 grid에 그려진다.
/// 구조 delta의 descriptor는 출력과 같은 순서의 값이 아니어서 이 함수를 부르지 않는다.
/// 크기가 없는 descriptor는 건너뛴다.
fn apply_reconnect_terminal_sizes(
    remote_to_local: &HashMap<u32, u32>,
    surfaces: &[Value],
    terminals: &mut crate::runtime::terminal_store::TerminalStore,
) {
    for s in surfaces {
        if s.get("role").and_then(|v| v.as_str()) != Some("terminal") {
            continue;
        }
        let Some(remote_id) = s
            .get("remote_id")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
        else {
            continue;
        };
        let dims = (
            s.get("cols").and_then(|v| v.as_u64()),
            s.get("rows").and_then(|v| v.as_u64()),
        );
        let (Some(cols), Some(rows)) = dims else {
            continue;
        };
        if let Some(&local) = remote_to_local.get(&remote_id)
            && let Some(t) = terminals.get_mut(local)
        {
            t.resize(cols as usize, rows as usize);
        }
    }
}

/// 기존 원격 surface의 로컬 ID·자원을 재사용하고 추가·삭제·kind 변경을 반영한다.
/// markdown은 기존 핸들을 공유해 구조 변경 때마다 문서를 다시 만들지 않는다.
/// 재연결 설치의 핵심부. 살아남은 mirror를 새 descriptor에 병합하고, terminal mirror를 서버 크기로
/// 맞추고, 옛 연결의 크기 요청 상태와 전경 이름을 비운다. 순서가 중요하다. 크기를 맞춘 뒤 상태를 비워야 다음
/// 리사이즈 스윕이 새 mirror grid와 목표를 비교해 요청을 다시 보내고, 그 응답을 새 연결에서 기다린다.
pub(super) fn install_reconnected_survivors(
    remote_to_local: &mut HashMap<u32, u32>,
    resize_sync: &mut tasty_remote::resize_sync::ResizeSync,
    resize_acks: bool,
    surfaces: &[Value],
    ids: &crate::runtime::id_reservations::ReservedIds,
    frame_tx: &SharedFrameSender,
    engine: &mut EngineMut<'_>,
) -> anyhow::Result<SurvivorMapping> {
    let mut mapping = merge_survivor_mapping(remote_to_local, surfaces, ids, frame_tx, engine)?;
    *remote_to_local = std::mem::take(&mut mapping.remote_to_local);
    apply_reconnect_terminal_sizes(remote_to_local, surfaces, &mut engine.runtime.terminals);
    resize_sync.reset_for_connection(resize_acks);
    // 옛 연결이 보낸 전경 이름은 새 연결의 첫 값(구버전 서버면 영영 없음) 전까지 쓰지 않는다.
    for &local in remote_to_local.values() {
        engine.reset_mirror_surface_foreground_for_connection(local);
    }
    Ok(mapping)
}

/// 구조 변경 뒤 terminal mirror로 남은 surface의 크기 요청 상태만 유지한다.
/// 닫히거나 다른 kind로 바뀐 surface의 대기와 실패 표시는 버린다.
pub(super) fn keep_terminal_resize_state(
    resize_sync: &mut tasty_remote::resize_sync::ResizeSync,
    remote_to_local: &HashMap<u32, u32>,
    mapping: &SurvivorMapping,
) {
    resize_sync.retain_surfaces(|remote| {
        remote_to_local
            .get(&remote)
            .is_some_and(|local| mapping.terminals.contains(local))
    });
}

pub(super) fn merge_survivor_mapping(
    old_map: &HashMap<u32, u32>,
    surfaces: &[Value],
    ids: &crate::runtime::id_reservations::ReservedIds,
    frame_tx: &SharedFrameSender,
    engine: &mut EngineMut<'_>,
) -> anyhow::Result<SurvivorMapping> {
    let mut new_map: HashMap<u32, u32> = HashMap::new();
    let mut terminal_locals: HashSet<u32> = HashSet::new();
    let mut mesh_locals: HashMap<u32, MirrorMeshInfo> = HashMap::new();
    let mut explorer_locals: HashMap<u32, std::path::PathBuf> = HashMap::new();
    let mut markdown_locals: MirrorMarkdownLeaves = HashMap::new();
    let mut html_locals = HashSet::new();
    let mut newly_created_remote_ids: Vec<u32> = Vec::new();
    let markdown_available = markdown_mirror_available(&engine.runtime.surface_registry);
    let html_available = super::html::resources::available(&engine.runtime.surface_registry);
    for s in surfaces {
        let remote_id = s.get("remote_id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let role = s.get("role").and_then(|v| v.as_str());
        let is_terminal = role == Some("terminal");
        // 재구성할 실제 kind로 비교한다. 미등록 markdown이나 지원하지 않는 role은 empty다.
        let new_kind = mirror_descriptor_kind(s, markdown_available, html_available);
        let survivor_local = old_map.get(&remote_id).copied();
        let old_kind: Option<&'static str> =
            survivor_local.and_then(|l| engine.find_surface_by_id(l).map(|s| s.kind()));
        let local_id = match survivor_local {
            Some(l) => {
                // ID가 같아도 kind가 바뀌면 옛 자원은 정리한다. markdown destroy는 호출자가 맡는다.
                if old_kind != Some(new_kind) {
                    engine.runtime.surfaces.remove(&l);
                    if old_kind == Some("terminal") {
                        engine.runtime.terminals.remove(l);
                        engine.forget_mirror_surface_busy(l);
                        engine.forget_mirror_surface_foreground(l);
                    }
                    // cwd는 terminal뿐 아니라 explorer·markdown에도 있어 이전 kind와 함께 지운다.
                    engine.forget_mirror_surface_cwd(l);
                    engine.remote.explorer_sources.remove(l);
                    // 새 frame이 올 때까지 이전 kind의 화면을 그리지 않게 한다.
                    engine.remote.attach_mesh_frames.remove(l);
                }
                l
            }
            None => {
                let l = ids.next_surface()?;
                newly_created_remote_ids.push(remote_id);
                l
            }
        };
        if is_terminal && old_kind != Some("terminal") {
            let cols = s.get("cols").and_then(Value::as_u64).unwrap_or(80) as usize;
            let rows = s.get("rows").and_then(Value::as_u64).unwrap_or(24) as usize;
            make_mirror_surface(
                remote_id,
                local_id,
                cols,
                rows,
                frame_tx,
                &mut engine.runtime.terminals,
                engine.observer_router.wants(local_id),
            );
        }
        if is_terminal {
            terminal_locals.insert(local_id);
        } else if role == Some("mesh") {
            let kind = s
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("mesh")
                .to_string();
            let plugin_id = s
                .get("plugin_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let display_name = s
                .get("display_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| kind.clone());
            mesh_locals.insert(
                local_id,
                MirrorMeshInfo {
                    display_name,
                    kind,
                    plugin_id,
                },
            );
        } else if role == Some("explorer") {
            let root = s
                .get("root")
                .and_then(|v| v.as_str())
                .map(std::path::PathBuf::from)
                .unwrap_or_default();
            explorer_locals.insert(local_id, root);
        } else if matches!(role, Some("html" | "markdown")) {
            let html = role == Some("html");
            if html {
                html_locals.insert(local_id);
            }
            let reuse = old_kind == Some(new_kind) && new_kind != "empty";
            if let Some(kind) = materialize_document(s, local_id, html, reuse, engine) {
                markdown_locals.insert(local_id, kind);
            }
        }
        // The descriptor tree never owns or clones a kind instance. Keep survivors in
        // the existing runtime collection, replacing only instances changed by the server.
        if is_terminal {
            engine
                .runtime
                .surfaces
                .entry(local_id)
                .or_insert_with(|| Box::new(TerminalSurface { id: local_id }));
        } else if let Some(info) = mesh_locals.get(&local_id) {
            engine.runtime.surfaces.insert(
                local_id,
                Box::new(crate::model::AttachMeshSurface::new(
                    local_id,
                    &info.kind,
                    info.plugin_id.clone(),
                    info.display_name.clone(),
                )),
            );
        } else if let Some(root) = explorer_locals.get(&local_id) {
            engine.remote.explorer_sources.sync(
                local_id,
                remote_id,
                root,
                frame_tx.epoch(),
                &mut engine.runtime.surfaces,
            );
        } else if !markdown_locals.contains_key(&local_id) {
            engine
                .runtime
                .surfaces
                .insert(local_id, Box::new(EmptySurface::new(local_id)));
        }
        new_map.insert(remote_id, local_id);
    }

    for (&remote_id, &local_id) in old_map.iter() {
        if !new_map.contains_key(&remote_id) {
            engine.runtime.surfaces.remove(&local_id);
            engine.runtime.terminals.remove(local_id);
            engine.forget_mirror_surface_busy(local_id);
            engine.forget_mirror_surface_attention(local_id);
            engine.forget_mirror_surface_cwd(local_id);
            engine.forget_mirror_surface_foreground(local_id);
            engine.remote.explorer_sources.remove(local_id);
            engine.remote.attach_mesh_frames.remove(local_id);
            // 원격이 닫은 surface도 로컬 닫기처럼 soft 점유 등 점유 기록을 남기지 않는다.
            engine.forget_closed_surface(local_id);
            super::agent_children::forget_mirror_child(engine, local_id);
        }
    }

    Ok(SurvivorMapping {
        remote_to_local: new_map,
        terminals: terminal_locals,
        mesh: mesh_locals,
        explorer: explorer_locals,
        markdown: markdown_locals,
        html: html_locals,
        newly_created_remote_ids,
    })
}

/// Reuse an owned document or publish its registered/deferred plugin instance.
fn materialize_document(
    descriptor: &Value,
    local: u32,
    html: bool,
    reuse: bool,
    engine: &mut EngineMut<'_>,
) -> Option<String> {
    let kind = if html { "html" } else { MARKDOWN_MIRROR_KIND };
    if reuse {
        return Some(kind.to_owned());
    }
    let registry = &engine.runtime.surface_registry;
    let surface = if html {
        super::html::resources::create(descriptor, local, registry)
    } else {
        create_mirror_markdown_surface(descriptor, local, registry)
    }
    .or_else(|| {
        (!registry.contains(kind)).then(|| {
            if html {
                super::html::resources::deferred(descriptor, local)
            } else {
                deferred_mirror_markdown_surface(descriptor, local)
            }
        })
    })?;
    let kind = surface.kind().to_owned();
    engine.runtime.surfaces.insert(local, surface);
    Some(kind)
}

pub(super) struct SurvivorMapping {
    pub(super) remote_to_local: HashMap<u32, u32>,
    pub(super) terminals: HashSet<u32>,
    pub(super) mesh: HashMap<u32, MirrorMeshInfo>,
    pub(super) explorer: HashMap<u32, std::path::PathBuf>,
    pub(super) markdown: MirrorMarkdownLeaves,
    pub(super) html: HashSet<u32>,
    /// 새로 매핑한 원격 surface는 사용자 new-tab/split의 포커스 후보가 된다.
    pub(super) newly_created_remote_ids: Vec<u32>,
}

impl SurvivorMapping {
    pub(super) fn markdown_ids(&self) -> HashSet<u32> {
        self.markdown
            .keys()
            .filter(|id| !self.html.contains(id))
            .copied()
            .collect()
    }
}
