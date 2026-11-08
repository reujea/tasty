//! Freeze runtime content without activating lazy leaves or performing journal I/O.
use crate::core::layout_persistence::import::surface_data::SurfaceData;
use crate::runtime::engine_session::EngineSession;

#[derive(Debug)]
pub(crate) struct CapturedSurface {
    pub surface: u32,
    pub kind: String,
    pub activation: Option<u64>,
    pub bytes: Vec<u8>,
    /// terminal 자료의 `restore_command`를 비워 두었다. journal worker가 [`RestoreLookup::fill`]로 채운다.
    pub restore_pending: bool,
}

/// capture가 비워 둔 terminal의 `restore.command`를 journal worker가 memory writer 순서로 읽어 채운다.
/// 메인 루프는 저장소를 기다리지 않고, capture 전에 넣은 metadata 쓰기는 이 읽기보다 먼저 반영된다.
pub(crate) struct RestoreLookup(tasty_memory::writer::MemoryAccess);

impl std::fmt::Debug for RestoreLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RestoreLookup").finish()
    }
}

impl RestoreLookup {
    pub(crate) fn of(session: &EngineSession) -> Self {
        Self(session.runtime.memory_access.clone())
    }

    /// 비워 둔 `restore_command`를 채운다. worker 스레드에서 writer 결과를 기다린다. 읽지 못하면 오류이고
    /// 자료는 비운 채 남는다.
    pub(crate) fn fill(&self, surfaces: &mut [CapturedSurface]) -> Result<(), String> {
        let ids: Vec<u32> = surfaces
            .iter()
            .filter(|surface| surface.restore_pending)
            .map(|surface| surface.surface)
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let bytes = ids.len() * 64;
        let request = tasty_memory::writer::MemoryRequest::unit(bytes, move |memory| {
            Ok(ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        crate::surface_meta::SurfaceMetaStore::get(
                            memory,
                            *id,
                            RESTORE_COMMAND_KEY,
                        ),
                    )
                })
                .collect::<Vec<_>>())
        });
        let found = self
            .0
            .run_and_wait(request, std::time::Duration::from_secs(30))
            .map_err(|why| format!("restore command lookup failed: {why:?}"))?
            .into_value::<Vec<(u32, Option<String>)>>()
            .ok_or("restore command lookup returned another value")?;
        for (id, command) in found {
            let Some(command) = command else { continue };
            let Some(surface) = surfaces.iter_mut().find(|s| s.surface == id) else {
                continue;
            };
            let mut data = SurfaceData::decode(&surface.bytes).map_err(|e| e.to_string())?;
            if let SurfaceData::Terminal {
                restore_command, ..
            } = &mut data
            {
                *restore_command = Some(command);
                surface.bytes = data.encode().map_err(|e| e.to_string())?;
            }
        }
        for surface in surfaces.iter_mut() {
            surface.restore_pending = false;
        }
        Ok(())
    }
}

const RESTORE_COMMAND_KEY: &str = "restore.command";
impl CapturedSurface {
    pub(crate) fn weight(&self) -> usize {
        self.kind
            .len()
            .saturating_add(self.bytes.len())
            .saturating_add(96)
    }
}

/// The caller serializes captures for an engine. The worker allocates persistent content sequence
/// numbers in accepted submission order, independently of activation and physical Pty generations.
/// A placeholder contributes no new capture: its canonical immutable data/seed stays pinned.
pub(crate) fn capture(session: &EngineSession) -> Result<Vec<CapturedSurface>, String> {
    let include_content = session
        .as_ref()
        .runtime
        .settings
        .general
        .restore_surface_content;
    capture_selected(session, None, include_content)
}
/// Serialized terminal history the next [`capture`] would copy, summed without reading disk rows.
/// Each surface's encoded bytes contain its whole history, so the request is at least this large.
pub(crate) fn content_lower_bound(session: &EngineSession) -> u64 {
    let engine = session.as_ref();
    if !engine.runtime.settings.general.restore_surface_content {
        return 0;
    }
    let mut total = 0u64;
    for workspace in engine.core.local_workspaces() {
        for id in workspace.all_surface_ids() {
            if engine.runtime.surfaces.get(&id).is_some_and(|owner| {
                owner
                    .as_any()
                    .is::<crate::runtime::surface_restorer::JournalPlaceholder>()
            }) {
                continue;
            }
            if let Some(terminal) = engine.runtime.terminals.get(id) {
                total = total.saturating_add(terminal.scrollback_serialized_len());
            }
        }
    }
    total
}
/// snapshot에 담기지 않는 호스트 cwd를 자료에 함께 저장한다. 복원은 이 값으로 상속 cwd를 되살린다.
fn saved_cwd(owner: &dyn crate::model::Surface) -> Option<String> {
    owner
        .source_cwd()
        .map(|cwd| cwd.to_string_lossy().into_owned())
}

pub(crate) fn capture_selected(
    session: &EngineSession,
    selected: Option<&std::collections::HashSet<u32>>,
    include_content: bool,
) -> Result<Vec<CapturedSurface>, String> {
    let engine = session.as_ref();
    let mut result = Vec::new();
    for workspace in engine.core.local_workspaces() {
        for id in workspace.all_surface_ids() {
            if selected.is_some_and(|selected| !selected.contains(&id)) {
                continue;
            }
            let descriptor = engine
                .core
                .find_surface_by_id(id)
                .ok_or("capture descriptor missing")?;
            let owner = engine
                .runtime
                .surfaces
                .get(&id)
                .ok_or("capture instance missing")?;
            if owner
                .as_any()
                .is::<crate::runtime::surface_restorer::JournalPlaceholder>()
            {
                continue;
            }
            let data = if descriptor.kind == "terminal" {
                if let Some(terminal) = engine.runtime.terminals.get(id) {
                    // 복원 명령은 journal worker가 writer 순서로 읽어 채운다(RestoreLookup).
                    let restore_command = None;
                    let lines = include_content.then(|| terminal.capture_history_and_screen());
                    SurfaceData::Terminal {
                        cwd: engine
                            .runtime
                            .terminals
                            .cwd(id)
                            .map(|cwd| cwd.to_string_lossy().into_owned()),
                        restore_command,
                        scrollback_ref: None,
                        scrollback: lines
                            .map(|lines| tasty_terminal::disk_scrollback::serialize_lines(&lines)),
                    }
                } else {
                    // Legacy lazy terminals are consumed by the import adapter; capture must not
                    // manufacture a replacement seed from current focus or default shell settings.
                    continue;
                }
            } else if let Some(empty) = owner.as_any().downcast_ref::<crate::model::EmptySurface>()
            {
                match &empty.deferred {
                    Some(saved) => SurfaceData::Generic {
                        data: saved.snapshot.clone(),
                        cwd: saved_cwd(owner.as_ref()),
                    },
                    None => continue,
                }
            } else {
                let definition = engine
                    .runtime
                    .surface_registry
                    .get(&descriptor.kind)
                    .ok_or_else(|| {
                        format!("cannot capture unregistered kind {}", descriptor.kind)
                    })?;
                SurfaceData::Generic {
                    data: (definition.snapshot)(owner.as_ref())
                        .unwrap_or_else(|| serde_json::json!({})),
                    cwd: saved_cwd(owner.as_ref()),
                }
            };
            let restore_pending = matches!(data, SurfaceData::Terminal { .. });
            result.push(CapturedSurface {
                surface: id,
                kind: descriptor.kind.clone(),
                activation: descriptor.activation_generation,
                bytes: data.encode().map_err(|error| error.to_string())?,
                restore_pending,
            });
        }
    }
    Ok(result)
}

/// 내용 없이 capture한 terminal에 history와 화면 줄을 `budget` bytes 안에서만 담는다. 직렬화
/// 하한이 남은 예산보다 큰 terminal은 디스크 줄을 읽지 않고 건너뛴다. 하한이 작은 terminal부터
/// 담아 내용을 지키는 terminal 수를 늘린다. 담지 못한 terminal은 내용 없이 남기고 그 ID를
/// 오름차순으로 돌려준다. 내용을 일부만 잘라 담지는 않는다.
pub(crate) fn add_history_within(
    session: &EngineSession,
    surfaces: &mut [CapturedSurface],
    budget: usize,
) -> Result<Vec<u32>, String> {
    let terminals = &session.as_ref().runtime.terminals;
    let mut candidates: Vec<(u64, usize)> = surfaces
        .iter()
        .enumerate()
        .filter(|(_, surface)| surface.kind == "terminal")
        .filter_map(|(index, surface)| {
            terminals
                .get(surface.surface)
                .map(|terminal| (terminal.scrollback_serialized_len(), index))
        })
        .collect();
    candidates.sort_unstable();
    let mut remaining = budget;
    let mut omitted = Vec::new();
    for (lower_bound, index) in candidates {
        let surface = &mut surfaces[index];
        let Some(terminal) = terminals.get(surface.surface) else {
            continue;
        };
        if lower_bound > remaining as u64 {
            omitted.push(surface.surface);
            continue;
        }
        let mut data = SurfaceData::decode(&surface.bytes).map_err(|e| e.to_string())?;
        if let SurfaceData::Terminal { scrollback, .. } = &mut data {
            *scrollback = Some(tasty_terminal::disk_scrollback::serialize_lines(
                &terminal.capture_history_and_screen(),
            ));
        }
        let bytes = data.encode().map_err(|e| e.to_string())?;
        let grown = bytes.len().saturating_sub(surface.bytes.len());
        if grown > remaining {
            omitted.push(surface.surface);
            continue;
        }
        remaining -= grown;
        surface.bytes = bytes;
    }
    omitted.sort_unstable();
    Ok(omitted)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{CapturedSurface, RestoreLookup, SurfaceData};
    use tasty_memory::MemoryStorage;

    fn terminal(surface: u32) -> CapturedSurface {
        let data = SurfaceData::Terminal {
            cwd: Some("/tmp".into()),
            restore_command: None,
            scrollback_ref: None,
            scrollback: None,
        };
        CapturedSurface {
            surface,
            kind: "terminal".into(),
            activation: None,
            bytes: data.encode().expect("encode"),
            restore_pending: true,
        }
    }

    /// 비-terminal 자료에 호스트 cwd를 함께 담아 복원이 되살릴 수 있게 한다.
    #[test]
    fn a_generic_capture_carries_the_host_cwd() {
        use tasty_core::DomainEvent as E;
        let markdown = |id| tasty_core::SurfaceSpec {
            id,
            kind: "markdown".into(),
            data: None,
        };
        let model = crate::state::tests::test_model(vec![
            E::CategoryCreated {
                id: 0,
                name: "normal".into(),
                index: 0,
            },
            E::WorkspaceCreated {
                id: 1,
                name: "w".into(),
                category: 0,
                index: 0,
                pane: 10,
            },
            E::TabCreated {
                id: 100,
                pane: 10,
                index: 0,
                name: "doc".into(),
                surface: markdown(11),
            },
        ]);
        let (_state, mut session) = crate::state::tests::test_state_from_model(model);
        let cwd = std::path::PathBuf::from("/saved-folder");
        session.runtime.surfaces.insert(
            11,
            Box::new(
                crate::model::EmptySurface::new_deferred_plugin(
                    11,
                    crate::model::DeferredPlugin {
                        kind: "markdown".into(),
                        snapshot: serde_json::json!({ "url": "http://127.0.0.1:9/" }),
                    },
                )
                .with_cwd(Some(cwd.clone())),
            ),
        );
        let captured = super::capture_selected(&session, None, false).expect("capture");
        let [surface] = captured.as_slice() else {
            panic!("one capture expected: {captured:?}");
        };
        assert_eq!(
            SurfaceData::decode(&surface.bytes).expect("decode"),
            SurfaceData::Generic {
                data: serde_json::json!({ "url": "http://127.0.0.1:9/" }),
                cwd: Some(cwd.to_string_lossy().into_owned()),
            }
        );
    }

    // writer 경로에서 먼저 접수한 metadata 쓰기를 capture의 복원 명령 읽기가 본다.
    #[test]
    fn the_worker_fills_restore_commands_after_earlier_metadata_writes() {
        let memory: Arc<Mutex<dyn MemoryStorage>> = Arc::new(Mutex::new(
            tasty_memory::MemoryStore::open_in_memory().expect("store"),
        ));
        let changes = Arc::new(tasty_memory::writer::ChangeFeed::default());
        let writer = tasty_memory::writer::spawn(
            tasty_memory::writer::WriterStore::shared(memory),
            changes.clone(),
            tasty_utils::batch_writer::Limits::default(),
            Arc::new(tasty_utils::batch_writer::SystemClock),
            Arc::new(|| {}),
        )
        .expect("writer");
        let access = tasty_memory::writer::MemoryAccess::writer(writer.handle(), changes);
        access.submit_detached(
            tasty_memory::writer::MemoryRequest::Write(tasty_memory::WriteOp::Put {
                owner: tasty_memory::HOST_OWNER.into(),
                scope: tasty_memory::Scope::Surface(3),
                key: "restore.command".into(),
                value: tasty_memory::MemoryValue::Text("claude -r x".into()),
                opts: tasty_memory::PutOpts::default(),
            }),
            "test metadata",
            None,
        );
        let mut surfaces = vec![terminal(3), terminal(4)];
        RestoreLookup(access).fill(&mut surfaces).expect("fill");
        let restored: Vec<Option<String>> = surfaces
            .iter()
            .map(|s| match SurfaceData::decode(&s.bytes).expect("decode") {
                SurfaceData::Terminal {
                    restore_command, ..
                } => restore_command,
                SurfaceData::Generic { .. } => panic!("terminal data changed kind"),
            })
            .collect();
        assert_eq!(restored, vec![Some("claude -r x".into()), None]);
        assert!(surfaces.iter().all(|s| !s.restore_pending));
    }
}
