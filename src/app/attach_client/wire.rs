//! Decode control payloads into typed mirror events.

mod list_dir;
#[cfg(test)]
mod tests;

use crate::ipc::stream::{StreamControl, StreamTag};
use list_dir::parse_list_dir_result;
use serde_json::Value;
use tasty_remote::client_session::{MirrorEvent, OutFrame, SharedFrameSender};

/// 제어 프레임을 mirror 이벤트로 변환한다. 어느 파서에서도 인식하지 못하면 무시한다.
pub(super) fn mirror_event_from_control(payload: &[u8]) -> Option<MirrorEvent> {
    match serde_json::from_slice::<StreamControl>(payload) {
        Ok(StreamControl::Resize {
            surface_id,
            cols,
            rows,
        }) => Some(MirrorEvent::Resize(surface_id, cols, rows)),
        Ok(StreamControl::ResizeRejected {
            surface_id,
            cols,
            rows,
        }) => Some(MirrorEvent::ResizeRejected(surface_id, cols, rows)),
        Ok(StreamControl::Activity { surface_id, busy }) => {
            Some(MirrorEvent::Activity(surface_id, busy))
        }
        Ok(StreamControl::Attention { surface_id, kind }) => {
            Some(MirrorEvent::Attention(surface_id, kind))
        }
        Ok(StreamControl::Cwd { surface_id, cwd }) => Some(MirrorEvent::Cwd(surface_id, cwd)),
        Ok(StreamControl::Foreground { surface_id, name }) => {
            Some(MirrorEvent::Foreground(surface_id, name))
        }
        // 무엇을 잃었는지 알 수 없는 연결 단위 통지라 세션 전체를 재동기화한다.
        Ok(StreamControl::Loss { frames }) => Some(MirrorEvent::Desynced { frames }),
        Ok(StreamControl::StructuralResult {
            ok: false,
            op_id,
            reason,
            ..
        }) => Some(MirrorEvent::StructuralFailed(op_id, reason)),
        Ok(StreamControl::StructuralResult {
            ok: true,
            op_id,
            created_surface_id,
            history_omitted,
            ..
        }) => Some(MirrorEvent::StructuralSucceeded(
            op_id,
            created_surface_id,
            history_omitted.unwrap_or(0),
        )),
        Ok(StreamControl::StructuralDelta {
            workspace_id,
            tree,
            surfaces,
        }) => Some(MirrorEvent::StructuralDelta {
            workspace_id,
            tree,
            surfaces,
        }),
        Ok(_) | Err(_) => parse_capture_result(payload)
            .or_else(|| parse_list_dir_result(payload))
            .or_else(|| parse_git_query_result(payload))
            .or_else(|| parse_markdown_content_result(payload))
            .or_else(|| parse_markdown_changed(payload))
            .or_else(|| super::html::parse(payload))
            .or_else(|| crate::app::explorer_files::remote::mirror_event(payload))
            .or_else(|| {
                serde_json::from_slice::<tasty_ipc::agent_child::AgentServerMsg>(payload)
                    .ok()
                    .map(MirrorEvent::AgentChild)
            })
            .or_else(|| parse_terminal_read_result(payload))
            .or_else(|| parse_file_download_result(payload)),
    }
}

fn parse_file_download_result(payload: &[u8]) -> Option<MirrorEvent> {
    use tasty_ipc::file_download::FileDownloadMsg;
    match serde_json::from_slice::<FileDownloadMsg>(payload).ok()? {
        FileDownloadMsg::FileDownloadResult {
            request_id,
            outcome,
        } => Some(MirrorEvent::FileDownload {
            request_id,
            outcome,
        }),
        FileDownloadMsg::FileDownloadRequest { .. } => None,
    }
}

fn parse_terminal_read_result(payload: &[u8]) -> Option<MirrorEvent> {
    use tasty_ipc::terminal_read::TerminalReadMsg;
    match serde_json::from_slice::<TerminalReadMsg>(payload).ok()? {
        TerminalReadMsg::TerminalReadResult {
            request_id,
            outcome,
        } => Some(MirrorEvent::TerminalRead {
            request_id,
            outcome,
        }),
        TerminalReadMsg::TerminalReadRequest { .. } => None,
    }
}

/// 옛 서버는 `image`·`image_reason` 을 싣지 않는다. 그 응답의 `ok` 는 경로만 쓴 성공이다.
#[derive(serde::Deserialize)]
struct CaptureResultWire {
    #[serde(default)]
    upload_id: Option<u64>,
    ok: bool,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    image: Option<bool>,
    #[serde(default)]
    image_reason: Option<String>,
}

fn control_event(payload: &[u8], event: &str) -> Option<Value> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    (value.get("event")?.as_str()? == event).then_some(value)
}

fn parse_capture_result(payload: &[u8]) -> Option<MirrorEvent> {
    let value = control_event(payload, "capture_result")?;
    let wire: CaptureResultWire = serde_json::from_value(value).ok()?;
    Some(MirrorEvent::CaptureResult {
        upload_id: wire.upload_id,
        ok: wire.ok,
        path: wire.path,
        reason: wire.reason,
        image: wire.image,
        image_reason: wire.image_reason,
    })
}

/// kind별 데이터는 flatten으로 받고 호스트가 해석하지 않은 채 git-viewer로 전달한다.
#[derive(serde::Deserialize)]
struct GitQueryResultWire {
    request_id: u64,
    ok: bool,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    truncated_status: bool,
    #[serde(default)]
    truncated_log: bool,
    #[serde(default)]
    truncated_diff: bool,
    #[serde(default)]
    reason: Option<String>,
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
}

fn parse_git_query_result(payload: &[u8]) -> Option<MirrorEvent> {
    let value = control_event(payload, "git_query_result")?;
    let wire: GitQueryResultWire = serde_json::from_value(value).ok()?;
    let truncated = wire.truncated_status || wire.truncated_log || wire.truncated_diff;
    let data = wire.ok.then_some(Value::Object(wire.rest));
    Some(MirrorEvent::GitQueryResult {
        request_id: wire.request_id,
        ok: wire.ok,
        kind: wire.kind,
        data,
        truncated,
        reason: wire.reason,
    })
}

#[derive(serde::Deserialize)]
struct MarkdownContentResultWire {
    request_id: u64,
    surface_id: u32,
    ok: bool,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    reason: Option<String>,
}

fn parse_markdown_content_result(payload: &[u8]) -> Option<MirrorEvent> {
    let value = control_event(payload, "markdown_content_result")?;
    let wire: MarkdownContentResultWire = serde_json::from_value(value).ok()?;
    Some(MirrorEvent::MarkdownContentResult {
        request_id: wire.request_id,
        surface_id: wire.surface_id,
        ok: wire.ok,
        file: wire.file,
        source: wire.source,
        truncated: wire.truncated,
        reason: wire.reason,
    })
}

fn parse_markdown_changed(payload: &[u8]) -> Option<MirrorEvent> {
    let value = control_event(payload, "markdown_changed")?;
    let surface_id = u32::try_from(value.get("surface_id")?.as_u64()?).ok()?;
    Some(MirrorEvent::MarkdownChanged { surface_id })
}

pub(super) fn send_control_frame(
    frame_tx: &SharedFrameSender,
    msg: &serde_json::Value,
) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(msg)?;
    frame_tx
        .send(OutFrame {
            tag: StreamTag::Control,
            payload,
        })
        .map_err(|_| anyhow::anyhow!("attach write queue closed (write thread gone)"))?;
    Ok(())
}
