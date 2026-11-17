//! 점유된 원격 workspace의 파일·Git·문서 조회와 응답 바이트 예산.

#[cfg(feature = "gui")]
use crate::model::SurfaceId;
use crate::runtime::engine_access::{EngineMut, EngineRef};
use tasty_ipc::stream::{StreamFrame, StreamTag};
#[cfg(feature = "gui")]
use tasty_ipc::stream_hub::PushResult;
use tasty_ipc::stream_hub::StreamHub;

/// 이 engine의 workspace를 하나라도 점유한 client의 디렉터리 조회를 처리한다.
/// plugin IPC의 FsRead 검사는 적용하지 않으며 경로를 점유 workspace 내부로 제한하지 않는다.
pub(crate) fn handle_list_dir_request(
    engine: &mut EngineMut<'_>,
    hub: &StreamHub,
    client_id: u32,
    request_id: u64,
    dir: &str,
) {
    let is_holder = engine.live.occupancy.client_holds_workspace(client_id);
    let result = if !is_holder {
        Err("client does not hold a workspace attach".to_string())
    } else {
        list_dir_for_request(dir)
    };
    let payload = match result {
        Ok((resolved_dir, entries)) => {
            let (wire_entries, truncated) = list_dir_entries_wire_capped(&entries);
            serde_json::json!({
                "event": "list_dir_result",
                "request_id": request_id,
                "ok": true,
                "dir": resolved_dir,
                "entries": wire_entries,
                "truncated": truncated,
                "volume": listing_volume(&resolved_dir),
            })
        }
        Err(reason) => serde_json::json!({
            "event": "list_dir_result",
            "request_id": request_id,
            "ok": false,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // 손실·끊김 처리는 허브에 맡기고 여기서는 재전송하지 않는다.
}

/// 빈 경로면 서버 홈을, 아니면 요청한 경로를 읽는다. 디렉터리 우선·이름순으로 반환한다.
fn list_dir_for_request(
    dir: &str,
) -> Result<(String, Vec<crate::core::fs_list::DirEntryInfo>), String> {
    let path = if dir.trim().is_empty() {
        directories::BaseDirs::new()
            .map(|d| d.home_dir().to_path_buf())
            .ok_or_else(|| "no home directory".to_string())?
    } else {
        std::path::PathBuf::from(dir)
    };
    let mut entries = crate::core::fs_list::read_dir_entries(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "permission denied".to_string()
        } else {
            e.to_string()
        }
    })?;
    crate::core::fs_list::sort_entries(
        &mut entries,
        tasty_model::SortColumn::Name,
        tasty_model::SortDir::Asc,
    );
    Ok((path.to_string_lossy().to_string(), entries))
}

mod list_dir_wire;
use list_dir_wire::{list_dir_entries_wire_capped, listing_volume};

/// workspace를 하나라도 점유한 client의 Git 조회를 처리한다.
/// worktree_path가 있으면 그 경로를 쓰고, 없으면 서버 터미널에서 cwd를 조회한다.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_git_query_request(
    engine: &mut crate::runtime::engine_access::EngineMut<'_>,
    hub: &StreamHub,
    client_id: u32,
    request_id: u64,
    surface_id: u32,
    kind: tasty_ipc::stream_hub::GitQueryKind,
    worktree_path: Option<String>,
    diff_path: Option<String>,
) {
    use tasty_ipc::stream_hub::GitQueryKind;

    let is_holder = engine.live.occupancy.client_holds_workspace(client_id);
    let result: Result<serde_json::Value, String> = if !is_holder {
        Err("client does not hold a workspace attach".to_string())
    } else {
        match kind {
            GitQueryKind::Snapshot => {
                git_query_snapshot(&engine.as_ref(), surface_id, worktree_path.as_deref())
            }
            GitQueryKind::Diff => match diff_path.as_deref() {
                Some(p) if !p.trim().is_empty() => {
                    git_query_diff(&engine.as_ref(), surface_id, worktree_path.as_deref(), p)
                }
                _ => Err("missing diff_path for kind=diff".to_string()),
            },
        }
    };
    let kind_wire = kind.as_wire_str();
    let payload = match result {
        Ok(mut data) => {
            let obj = data
                .as_object_mut()
                .expect("git query helpers return objects");
            obj.insert("event".to_string(), serde_json::json!("git_query_result"));
            obj.insert("request_id".to_string(), serde_json::json!(request_id));
            obj.insert("ok".to_string(), serde_json::json!(true));
            obj.insert("kind".to_string(), serde_json::json!(kind_wire));
            data
        }
        Err(reason) => serde_json::json!({
            "event": "git_query_result",
            "request_id": request_id,
            "ok": false,
            "kind": kind_wire,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // 손실·끊김 처리는 허브에 맡기고 여기서는 재전송하지 않는다.
}

fn resolve_git_query_target(
    engine: &EngineRef<'_>,
    surface_id: u32,
    worktree_path: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    if let Some(p) = worktree_path {
        if p.trim().is_empty() {
            return Err("empty worktree_path".to_string());
        }
        return Ok(std::path::PathBuf::from(p));
    }
    engine
        .runtime
        .terminals
        .cwd(surface_id)
        .ok_or_else(|| "remote surface has no known cwd".to_string())
}

/// Git 상태·로그·worktree를 수집한다. worktree는 모두 싣고 남은 예산으로 상태·로그를 자른다.
/// worktree 목록 자체가 예산을 넘을 수 있으며 전체 프레임 크기를 다시 검사하지 않는다.
fn git_query_snapshot(
    engine: &EngineRef<'_>,
    surface_id: u32,
    worktree_path: Option<&str>,
) -> Result<serde_json::Value, String> {
    let target = resolve_git_query_target(engine, surface_id, worktree_path)?;
    let repo = tasty_git_core::discover_repo(&target)
        .ok_or_else(|| "no git repository found".to_string())?;
    let current_wd = repo
        .workdir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo.path().to_path_buf());
    let worktrees =
        tasty_git_core::collect_worktrees(&repo, &current_wd).map_err(|e| e.to_string())?;
    let active = worktrees.iter().find(|w| w.is_current);
    let branch = active.and_then(|w| w.branch.clone());
    let oid = active.and_then(|w| w.oid.clone());

    let status = tasty_git_core::collect_status(&repo).map_err(|e| e.to_string())?;
    let log = tasty_git_core::collect_log(&repo, GIT_QUERY_LOG_LIMIT).map_err(|e| e.to_string())?;

    let worktrees_wire: Vec<serde_json::Value> =
        worktrees.iter().map(worktree_entry_wire).collect();
    let mut budget = GIT_QUERY_BYTE_BUDGET.saturating_sub(wire_values_len(&worktrees_wire));
    let (status_wire, truncated_status) =
        cap_wire_values(budget, status.iter().map(status_entry_wire).collect());
    budget = budget.saturating_sub(wire_values_len(&status_wire));
    let (log_wire, truncated_log) =
        cap_wire_values(budget, log.iter().map(log_entry_wire).collect());

    Ok(serde_json::json!({
        "active_worktree_path": display_path_lossy(&current_wd),
        "branch": branch,
        "oid": oid,
        "worktrees": worktrees_wire,
        "status_entries": status_wire,
        "truncated_status": truncated_status,
        "log_entries": log_wire,
        "truncated_log": truncated_log,
    }))
}

/// 파일 diff를 hunk 단위로 예산에 맞춰 반환한다. 응답의 나머지 필드는 이 예산에 포함하지 않는다.
fn git_query_diff(
    engine: &EngineRef<'_>,
    surface_id: u32,
    worktree_path: Option<&str>,
    diff_path: &str,
) -> Result<serde_json::Value, String> {
    let target = resolve_git_query_target(engine, surface_id, worktree_path)?;
    let repo = tasty_git_core::discover_repo(&target)
        .ok_or_else(|| "no git repository found".to_string())?;
    let diff = tasty_git_core::collect_diff(&repo, diff_path).map_err(|e| e.to_string())?;
    let (hunks_wire, truncated) = cap_diff_hunks(GIT_QUERY_BYTE_BUDGET, &diff.hunks);
    Ok(serde_json::json!({
        "file_path": diff.file_path,
        "hunks": hunks_wire,
        "truncated_diff": truncated,
    }))
}

fn display_path_lossy(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn worktree_entry_wire(w: &tasty_git_core::WorktreeEntry) -> serde_json::Value {
    serde_json::json!({
        "name": w.name,
        "path": display_path_lossy(&w.path),
        "branch": w.branch,
        "oid": w.oid,
        "is_main": w.is_main,
        "is_current": w.is_current,
        "locked": w.locked,
        "lock_reason": w.lock_reason,
        "is_valid": w.is_valid,
    })
}

fn file_status_wire(s: tasty_git_core::FileStatus) -> &'static str {
    use tasty_git_core::FileStatus;
    match s {
        FileStatus::Modified => "modified",
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Renamed => "renamed",
        FileStatus::Untracked => "untracked",
        FileStatus::Conflicted => "conflicted",
    }
}

fn status_entry_wire(e: &tasty_git_core::StatusEntry) -> serde_json::Value {
    serde_json::json!({ "status": file_status_wire(e.status), "path": e.path })
}

fn log_entry_wire(e: &tasty_git_core::LogEntry) -> serde_json::Value {
    serde_json::json!({
        "oid_short": e.oid_short,
        "summary": e.summary,
        "author": e.author,
        "time": e.time,
        "refs": e.refs,
    })
}

fn diff_line_wire(l: &tasty_git_core::DiffLine) -> serde_json::Value {
    use tasty_git_core::DiffLineKind;
    let kind = match l.kind {
        DiffLineKind::Context => "context",
        DiffLineKind::Addition => "addition",
        DiffLineKind::Deletion => "deletion",
    };
    serde_json::json!({
        "kind": kind,
        "content": l.content,
        "old_lineno": l.old_lineno,
        "new_lineno": l.new_lineno,
    })
}

fn diff_hunk_wire(h: &tasty_git_core::DiffHunk) -> serde_json::Value {
    serde_json::json!({
        "header": h.header,
        "lines": h.lines.iter().map(diff_line_wire).collect::<Vec<_>>(),
    })
}

/// 상태·로그·diff의 직렬화 데이터 제한. 나머지 응답 필드를 위해 프레임 상한보다 작게 둔다.
const GIT_QUERY_BYTE_BUDGET: usize = 700 * 1024;

const GIT_QUERY_LOG_LIMIT: usize = 200;

fn wire_values_len(items: &[serde_json::Value]) -> usize {
    items
        .iter()
        .map(|v| serde_json::to_vec(v).map(|b| b.len()).unwrap_or(0))
        .sum()
}

/// 각 value의 직렬화 길이를 더해 예산 안에서 자른다. 배열 괄호·쉼표는 합계에 포함하지 않는다.
fn cap_wire_values(
    mut budget: usize,
    items: Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, bool) {
    let mut out = Vec::with_capacity(items.len());
    for v in items {
        let len = serde_json::to_vec(&v).map(|b| b.len()).unwrap_or(0);
        if len > budget {
            return (out, true);
        }
        budget -= len;
        out.push(v);
    }
    (out, false)
}

/// hunk를 통째로 포함하거나 제외하며 hunk 내부의 줄을 일부만 보내지 않는다.
fn cap_diff_hunks(
    mut budget: usize,
    hunks: &[tasty_git_core::DiffHunk],
) -> (Vec<serde_json::Value>, bool) {
    let mut out = Vec::with_capacity(hunks.len());
    for h in hunks {
        let wire = diff_hunk_wire(h);
        let len = serde_json::to_vec(&wire).map(|b| b.len()).unwrap_or(0);
        if len > budget {
            return (out, true);
        }
        budget -= len;
        out.push(wire);
    }
    (out, false)
}

/// workspace를 하나라도 점유한 client에게 markdown 원문을 보낸다.
/// 동기 요청이라 plugin에 되묻지 않고 host가 파일을 직접 읽는다. plugin의 대용량 표시 확인과는 별개다.
/// 파일을 모두 읽은 뒤 응답을 자르므로 이 예산이 읽기 메모리 사용량을 제한하지는 않는다.
pub(crate) fn handle_markdown_content_request(
    engine: &mut EngineMut<'_>,
    hub: &StreamHub,
    client_id: u32,
    request_id: u64,
    surface_id: u32,
) {
    let is_holder = engine.live.occupancy.client_holds_workspace(client_id);
    let result = if !is_holder {
        Err("client does not hold a workspace attach".to_string())
    } else {
        markdown_content_for_request(&engine.as_ref(), surface_id)
    };
    let payload = match result {
        Ok((file, source, truncated)) => serde_json::json!({
            "event": "markdown_content_result",
            "request_id": request_id,
            "surface_id": surface_id,
            "ok": true,
            "file": file,
            "source": source,
            "truncated": truncated,
        }),
        Err(reason) => serde_json::json!({
            "event": "markdown_content_result",
            "request_id": request_id,
            "surface_id": surface_id,
            "ok": false,
            "reason": reason,
        }),
    };
    let frame = StreamFrame::new(
        StreamTag::Control,
        serde_json::to_vec(&payload).unwrap_or_default(),
    );
    let _ = hub.push(client_id, frame); // 손실·끊김 처리는 허브에 맡기고 여기서는 재전송하지 않는다.
}

/// client에 다시 읽기 버튼을 표시하게 하는 변경 통지.
#[cfg(feature = "gui")]
const MARKDOWN_CHANGED_EVENT: &str = "markdown_changed";

/// webview.set_url로 문서가 바뀌었을 때 workspace holder에 통지를 시도한다.
/// Markdown은 사용자 요청으로 원문을 읽고 HTTP mirror는 새 메타데이터를 요청한다.
/// notifier·점유가 없으면 생략한다. 반환값은 큐에 실린 client 수다.
/// webview.set_url이 GUI 전용이므로 헤드리스에서는 이 통지를 보내지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn notify_markdown_changed(
    attach: &crate::core::attach::OccupancyRegistry,
    remote: &crate::remote::state::RemoteState,
    capabilities: crate::runtime::surface_registry::KindCapabilities,
    surface_id: SurfaceId,
) -> usize {
    if !(capabilities.mirror_content || capabilities.mirror_viewer) {
        return 0;
    }
    let Some(hub) = remote.notifier() else {
        return 0;
    };
    let holders = attach.workspace_holders();
    if holders.is_empty() {
        return 0;
    }
    let payload = serde_json::json!({
        "event": MARKDOWN_CHANGED_EVENT,
        "surface_id": surface_id,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let mut sent = 0;
    for client_id in holders {
        match hub.push(
            client_id,
            StreamFrame::new(StreamTag::Control, bytes.clone()),
        ) {
            PushResult::Sent => sent += 1,
            other => tracing::debug!(
                "attach: markdown_changed surface={surface_id} client={client_id} not queued: {other:?}"
            ),
        }
    }
    sent
}

/// 허용된 content surface의 파일을 읽어 (file, source, truncated)를 반환한다.
/// 파일 없이 열린 markdown은 빈 문서로 답한다. 파일 전체를 읽은 뒤 응답 크기를 제한한다.
fn markdown_content_for_request(
    engine: &EngineRef<'_>,
    surface_id: u32,
) -> Result<(String, String, bool), String> {
    let surface = engine
        .find_surface_by_id(surface_id)
        .ok_or_else(|| "unknown surface".to_string())?;
    let (kind, plugin_id, file) = surface
        .attach_content_info()
        .ok_or_else(|| "surface does not carry content".to_string())?;
    if !engine
        .runtime
        .surface_registry
        .owner_capabilities(kind, plugin_id)
        .mirror_content
    {
        return Err(format!("surface kind '{kind}' is not content-mirrored"));
    }
    let Some(path) = file else {
        return Ok((String::new(), String::new(), false));
    };
    let bytes = std::fs::read(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "permission denied".to_string()
        } else {
            e.to_string()
        }
    })?;
    let (source, truncated) = markdown_source_wire_capped(&bytes);
    Ok((path.to_string_lossy().to_string(), source, truncated))
}

/// source를 JSON 문자열로 직렬화한 크기 제한. 이스케이프와 따옴표를 포함한다.
/// 전체 프레임 상한보다 작게 두어 나머지 응답 필드의 여유를 남긴다.
/// plugin의 대용량 표시 확인과는 별개이며 파일 읽기·메모리 사용량의 상한이 아니다.
const MARKDOWN_CONTENT_BYTE_BUDGET: usize = 700 * 1024;

fn markdown_source_wire_capped(bytes: &[u8]) -> (String, bool) {
    markdown_source_wire_capped_with_budget(bytes, MARKDOWN_CONTENT_BYTE_BUDGET)
}

/// JSON 문자열 안에서 char가 차지할 바이트 수. BMP 문자는 아래 검사에서 serde_json과 비교한다.
fn json_escaped_char_len(ch: char) -> usize {
    match ch {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' => 2,
        c if (c as u32) < 0x20 => 6,
        c => c.len_utf8(),
    }
}

/// 따옴표 두 바이트와 이스케이프 비용을 세어 UTF-8 문자 경계에서 자른다.
/// bytes 전체를 먼저 lossy 변환한다. 빈 문자열도 따옴표 두 바이트가 필요하다.
fn markdown_source_wire_capped_with_budget(bytes: &[u8], budget: usize) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    let mut used: usize = 2;
    for (i, ch) in text.char_indices() {
        let cost = json_escaped_char_len(ch);
        if used + cost > budget {
            return (text[..i].to_string(), true);
        }
        used += cost;
    }
    (text.into_owned(), false)
}

#[cfg(test)]
mod markdown_content_tests {
    use super::{json_escaped_char_len, markdown_source_wire_capped_with_budget};

    fn serialized_len(s: &str) -> usize {
        serde_json::to_vec(&serde_json::Value::String(s.to_string()))
            .expect("string always serializes")
            .len()
    }

    #[test]
    fn a_document_within_budget_is_not_truncated() {
        let (source, truncated) = markdown_source_wire_capped_with_budget(b"# hi\n", 64);
        assert_eq!(source, "# hi\n");
        assert!(!truncated);
    }

    #[test]
    fn truncation_backs_up_to_a_char_boundary() {
        let bytes = "가나".as_bytes();
        assert_eq!(bytes.len(), 6);
        let (source, truncated) = markdown_source_wire_capped_with_budget(bytes, 7);
        assert_eq!(source, "가", "잘린 자리에 U+FFFD 가 생기면 안 된다");
        assert!(truncated);
    }

    #[test]
    fn truncation_at_an_exact_boundary_keeps_everything_before_it() {
        let bytes = "가나".as_bytes();
        let (source, truncated) = markdown_source_wire_capped_with_budget(bytes, 5);
        assert_eq!(source, "가");
        assert!(truncated);
        assert_eq!(serialized_len(&source), 5);
    }

    #[test]
    fn budget_counts_escape_expansion_not_raw_bytes() {
        let raw = "\"".repeat(400);
        assert_eq!(serialized_len(&raw), 802);

        let (source, truncated) = markdown_source_wire_capped_with_budget(raw.as_bytes(), 500);
        assert!(
            truncated,
            "원문(400) 은 예산(500) 안이지만 직렬화(802) 는 넘는다 — 잘려야 한다"
        );
        assert_eq!(source.len(), 249, "따옴표 249 개 = 2 + 249*2 = 500");
        assert_eq!(serialized_len(&source), 500);
    }

    #[test]
    fn control_characters_are_counted_at_their_six_byte_cost() {
        let raw = "\u{01}".repeat(100);
        let (source, truncated) = markdown_source_wire_capped_with_budget(raw.as_bytes(), 200);
        assert!(truncated);
        assert_eq!(source.chars().count(), 33, "2 + 33*6 = 200");
        assert_eq!(serialized_len(&source), 200);
    }

    /// 유효한 BMP 문자 전부를 serde_json의 직렬화 길이와 대조한다.
    #[test]
    fn escaped_char_len_matches_serde_json() {
        for cp in 0u32..=0xFFFF {
            let Some(ch) = char::from_u32(cp) else {
                continue; // surrogate
            };
            let s = ch.to_string();
            let expected = serialized_len(&s) - 2;
            assert_eq!(
                json_escaped_char_len(ch),
                expected,
                "U+{cp:04X} 의 이스케이프 길이가 serde_json 과 다르다"
            );
        }
    }

    #[test]
    fn invalid_utf8_is_carried_lossily_without_failing() {
        let (source, truncated) = markdown_source_wire_capped_with_budget(&[0xff, 0xfe], 64);
        assert!(!source.is_empty());
        assert!(!truncated);
    }
}

#[cfg(all(test, feature = "gui"))]
mod markdown_changed_tests {
    use super::notify_markdown_changed;
    use crate::core::attach::OccupancyRegistry;
    use crate::runtime::surface_registry::KindCapabilities;

    /// 아무것도 선언하지 않은 kind가 등록 때 받는 능력.
    fn caps(kind: &str, plugin: &str) -> KindCapabilities {
        KindCapabilities::resolve(&Default::default(), kind, plugin)
    }
    use tasty_ipc::stream::StreamTag;
    use tasty_ipc::stream_hub::StreamHub;

    fn changed_surface_id(frame: &tasty_ipc::stream::StreamFrame) -> Option<u64> {
        assert_eq!(frame.tag, StreamTag::Control);
        let v: serde_json::Value = serde_json::from_slice(&frame.payload).ok()?;
        (v.get("event")?.as_str()? == "markdown_changed").then_some(())?;
        v.get("surface_id")?.as_u64()
    }

    #[test]
    fn every_workspace_holder_receives_the_signal_once() {
        let hub = StreamHub::new();
        let a = hub.alloc_id();
        let b = hub.alloc_id();
        let bystander = hub.alloc_id();
        let rx_a = hub.register(a);
        let rx_b = hub.register(b);
        let rx_bystander = hub.register(bystander);
        let mut reg = OccupancyRegistry::new();
        let mut remote = crate::remote::state::RemoteState::new();
        remote.set_notifier(hub);
        reg.acquire_workspace(100, &[10], &[10, 11], a).unwrap();
        reg.acquire_workspace(200, &[20], &[20], b).unwrap();
        reg.acquire_workspace(300, &[30], &[30], a).unwrap();

        assert_eq!(
            notify_markdown_changed(&reg, &remote, caps("markdown", "com.tasty.markdown"), 11),
            2
        );
        assert_eq!(changed_surface_id(&rx_a.try_recv().unwrap()), Some(11));
        assert!(
            rx_a.try_recv().is_err(),
            "두 워크스페이스를 점유해도 신호는 한 번"
        );
        assert_eq!(changed_surface_id(&rx_b.try_recv().unwrap()), Some(11));
        assert!(
            rx_bystander.try_recv().is_err(),
            "점유하지 않은 client 에는 가지 않는다"
        );
        assert_eq!(
            notify_markdown_changed(&reg, &remote, caps("html", "com.tasty.html"), 11),
            2
        );
        assert_eq!(changed_surface_id(&rx_a.try_recv().unwrap()), Some(11));
        assert_eq!(changed_surface_id(&rx_b.try_recv().unwrap()), Some(11));
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_err());
        assert!(rx_bystander.try_recv().is_err());
    }

    #[test]
    fn no_holder_or_untrusted_document_owner_is_a_no_op() {
        let hub = StreamHub::new();
        let a = hub.alloc_id();
        let rx = hub.register(a);
        let mut reg = OccupancyRegistry::new();
        let mut remote = crate::remote::state::RemoteState::new();
        remote.set_notifier(hub);
        assert_eq!(
            notify_markdown_changed(&reg, &remote, caps("markdown", "com.tasty.markdown"), 11),
            0
        );
        assert!(rx.try_recv().is_err());

        reg.acquire_workspace(100, &[10], &[10, 11], a).unwrap();
        assert_eq!(
            notify_markdown_changed(&reg, &remote, caps("html", "com.thirdparty.html"), 11),
            0
        );
        assert_eq!(
            notify_markdown_changed(
                &reg,
                &remote,
                caps("markdown", "com.thirdparty.markdown"),
                11
            ),
            0
        );
        assert!(rx.try_recv().is_err());
    }
}

#[cfg(test)]
mod content_request_tests {
    use super::markdown_content_for_request;

    /// 원문 제공은 소유 plugin이 `mirror_content`를 선언한 kind에만 한다. id 접두사는 보지 않는다.
    #[test]
    fn a_declared_third_party_kind_serves_its_file_like_markdown() {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        let mut session =
            crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine");
        let engine = session.borrow_mut();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "body").unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        for (sid, kind, mirror_content) in [(11, "notes", true), (12, "plainweb", false)] {
            let decl = serde_json::from_value(serde_json::json!({
                "kind": kind, "display_name_i18n_key": "surface.kind.markdown",
                "rendering": "webview", "mirror_content": mirror_content,
            }))
            .unwrap();
            let registry = &engine.runtime.surface_registry;
            crate::plugin_bridge::remote_kind::register_remote_kind(
                registry,
                "com.example.docs",
                &decl,
                tx.clone(),
            );
            let def = registry.get_live(kind).unwrap();
            let surface = (def.create)(sid, None, &serde_json::json!({ "file": file }))
                .unwrap()
                .publish()
                .unwrap();
            engine.runtime.surfaces.insert(sid, surface);
        }
        let (served, source, truncated) =
            markdown_content_for_request(&engine.as_ref(), 11).expect("declared kind");
        assert_eq!(
            (served.as_str(), source.as_str(), truncated),
            (file.to_str().unwrap(), "body", false)
        );
        assert!(markdown_content_for_request(&engine.as_ref(), 12).is_err());
    }
}

#[cfg(test)]
mod git_query_tests {
    use super::*;

    #[test]
    fn cap_wire_values_stops_before_exceeding_budget() {
        let items: Vec<serde_json::Value> = (0..20)
            .map(|i| serde_json::json!({ "path": format!("file_{i:03}") }))
            .collect();
        let one_len = serde_json::to_vec(&items[0]).unwrap().len();
        let budget = one_len * 3;
        let (out, truncated) = cap_wire_values(budget, items);
        assert_eq!(out.len(), 3, "expected exactly 3 entries to fit: {out:?}");
        assert!(truncated);
    }

    #[test]
    fn cap_wire_values_empty_input_never_truncated() {
        let (out, truncated) = cap_wire_values(0, Vec::new());
        assert!(out.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn worktree_entry_wire_roundtrips_fields() {
        let w = tasty_git_core::WorktreeEntry {
            name: "main".to_string(),
            path: std::path::PathBuf::from("/repo"),
            branch: Some("main".to_string()),
            oid: Some("abc1234".to_string()),
            is_main: true,
            is_current: true,
            locked: false,
            lock_reason: None,
            is_valid: true,
        };
        let wire = worktree_entry_wire(&w);
        assert_eq!(wire["name"], "main");
        assert_eq!(wire["path"], "/repo");
        assert_eq!(wire["is_current"], true);
    }

    fn test_engine() -> crate::runtime::engine_session::EngineSession {
        let term_waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, term_waker).expect("engine")
    }

    #[test]
    fn resolve_git_query_target_prefers_worktree_path_over_surface_cwd() {
        let mut engine_session = test_engine();
        let engine = engine_session.borrow_mut();
        let resolved =
            resolve_git_query_target(&engine.as_ref(), 999, Some("/explicit/path")).unwrap();
        assert_eq!(resolved, std::path::PathBuf::from("/explicit/path"));
    }

    #[test]
    fn resolve_git_query_target_errors_without_cwd_or_worktree_path() {
        let mut engine_session = test_engine();
        let engine = engine_session.borrow_mut();
        let err = resolve_git_query_target(&engine.as_ref(), 999, None).unwrap_err();
        assert!(err.contains("no known cwd"));
    }
}

#[cfg(test)]
mod list_dir_entries_wire_capped_tests {
    use super::list_dir_wire::{list_dir_entries_wire_capped_with_budget, list_dir_entry_wire};
    use crate::core::fs_list::DirEntryInfo;

    fn entry(name: &str) -> DirEntryInfo {
        DirEntryInfo {
            #[cfg(feature = "gui")]
            path: name.into(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn fits_within_budget_untruncated() {
        let entries: Vec<_> = (0..10).map(|i| entry(&format!("f{i}"))).collect();
        let (wire, truncated) = list_dir_entries_wire_capped_with_budget(&entries, 10_000);
        assert_eq!(wire.len(), 10);
        assert!(!truncated);
    }

    #[test]
    fn stops_before_exceeding_budget() {
        // 포맷이 바뀌어도 항목 개수 경계를 검사하도록 현재 직렬화 길이로 예산을 정한다.
        let entries: Vec<_> = (0..20).map(|i| entry(&format!("file_{i:03}"))).collect();
        let one_entry_len = serde_json::to_vec(&list_dir_entry_wire(&entries[0]))
            .unwrap()
            .len();
        let budget = one_entry_len * 3;
        let (wire, truncated) = list_dir_entries_wire_capped_with_budget(&entries, budget);
        assert_eq!(wire.len(), 3, "expected exactly 3 entries to fit: {wire:?}");
        assert!(truncated);
    }

    #[test]
    fn empty_input_is_never_truncated() {
        let (wire, truncated) = list_dir_entries_wire_capped_with_budget(&[], 0);
        assert!(wire.is_empty());
        assert!(!truncated);
    }
}
