//! attach holder 가 원격 파일 하나를 client 로 가져오는 제어 메시지("다운로드 후 실행").
//!
//! client 가 경로를 지정해 먼저 크기와 식별 정보를 묻고(`stat`), 그 뒤 위치를 지정해 한 조각씩 읽는다
//! (`read`). 서버는 요청 사이에 상태를 두지 않는다. 조각마다 첫 응답의 식별 정보와 지금 파일을 대조해
//! 전송 중 교체·변경을 실패로 돌려준다. client 가 다음 조각을 요청하지 않으면 전송이 멈추므로 취소는
//! client 쪽에서 끝난다. 별도 포트·서비스 없이 기존 holder 연결의 Control 프레임으로 오간다.
//!
//! 구 서버는 이 event 를 모르고 버린다. client 는 attach descriptor 에 [`CAPABILITY`] 가 있는 연결에만 보낸다.

use serde::{Deserialize, Serialize};

/// 서버가 경로 지정 파일 읽기를 받는다는 capability 이름.
pub const CAPABILITY: &str = "ipc.stream.file-download";
/// [`CAPABILITY`] 의 버전.
pub const VERSION: u32 = 1;

/// 한 조각의 원본 바이트 상한. base64 로 4/3 배가 되어도 프레임 상한
/// ([`crate::stream::MAX_FRAME_LEN`])의 절반 아래에 머문다.
pub const CHUNK_BYTES: u64 = 256 * 1024;

/// 이 크기까지는 확인 없이 받는다. 1 바이트라도 넘으면 사용자에게 한 번 묻는다(1 MiB).
pub const CONFIRM_ABOVE_BYTES: u64 = 1024 * 1024;

/// client → 서버 요청과 서버 → client 결과.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum FileDownloadMsg {
    /// `path` 는 서버 호스트의 경로다. 서버는 이 연결이 workspace 를 점유했을 때만 답한다.
    FileDownloadRequest {
        request_id: u64,
        path: String,
        op: DownloadOp,
    },
    FileDownloadResult {
        request_id: u64,
        outcome: DownloadOutcome,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DownloadOp {
    /// 크기·이름·식별 정보. 파일을 열지 않는다.
    Stat,
    /// `offset` 부터 최대 [`CHUNK_BYTES`]. `identity` 는 `stat` 이 준 값이다.
    Read { offset: u64, identity: FileIdentity },
}

/// 전송 중 교체·변경을 알아보는 값. 서버 OS 가 주지 않는 항목은 비운다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub size: u64,
    /// UNIX 시각 이후 나노초. JSON 이 u128 을 싣지 못하므로 u64 로 줄인다(2554 년까지 담긴다).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_ns: Option<u64>,
    /// 생성 시각(UNIX 시각 이후 나노초). Unix (device, inode)가 없는 OS(Windows)에서도 같은 크기·수정 시각의
    /// 다른 파일로 바뀐 것을 알아본다. OS·파일시스템이 주지 않으면 비운다. 이 필드를 보내지 않는 서버와는
    /// client 가 받은 값(빈 값)을 그대로 돌려주므로 양쪽 모두 비어 같게 비교된다. 서버는 구조체 전체를
    /// 비교하므로 이 필드를 모르는 client 는 생성 시각을 얻는 파일마다 `changed` 로 거절된다. 그런 client 는
    /// 없다: 이 필드는 [`CAPABILITY`] 와 함께 들어왔다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ns: Option<u64>,
    /// Unix 의 (device, inode). 같은 경로에 다른 파일이 들어오면 바뀐다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DownloadOutcome {
    /// `name` 은 요청 경로의 마지막 구성 요소다. client 는 그대로 믿지 않고 경로 구성 요소를 지운다.
    Stat {
        name: String,
        identity: FileIdentity,
    },
    Chunk {
        offset: u64,
        data_b64: String,
        /// 이 조각으로 파일 끝에 닿았다.
        eof: bool,
    },
    Refused {
        reason: DownloadRefusal,
    },
}

/// 거절 이유. client 는 이유마다 다른 안내를 고른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadRefusal {
    /// 폴더이거나 폴더를 가리키는 링크다. 폴더는 지원하지 않는다.
    Directory,
    /// FIFO·장치·소켓처럼 일반 파일이 아니다.
    NotRegular,
    NotFound,
    PermissionDenied,
    /// 첫 응답 뒤 파일이 바뀌었거나 다른 파일로 교체됐다.
    Changed,
    /// 이 연결이 workspace 를 점유하지 않았다.
    NotAttached,
    /// 그 밖의 읽기 실패.
    Io,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_result_round_trip_with_their_event_tags() {
        let identity = FileIdentity {
            size: 3,
            modified_ns: Some(7),
            created_ns: Some(5),
            file_id: Some((1, 2)),
        };
        let req = FileDownloadMsg::FileDownloadRequest {
            request_id: 4,
            path: "/srv/a.bin".into(),
            op: DownloadOp::Read {
                offset: 0,
                identity: identity.clone(),
            },
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["event"], "file_download_request");
        assert_eq!(json["op"]["op"], "read");
        assert_eq!(
            serde_json::from_value::<FileDownloadMsg>(json).unwrap(),
            req
        );
        let res = FileDownloadMsg::FileDownloadResult {
            request_id: 4,
            outcome: DownloadOutcome::Refused {
                reason: DownloadRefusal::Directory,
            },
        };
        let json = serde_json::to_value(&res).unwrap();
        assert_eq!(json["event"], "file_download_result");
        assert_eq!(json["outcome"]["status"], "refused");
        assert_eq!(json["outcome"]["reason"], "directory");
        assert_eq!(
            serde_json::from_value::<FileDownloadMsg>(json).unwrap(),
            res
        );
    }

    #[test]
    fn a_full_chunk_fits_half_a_frame_after_base64() {
        let encoded = CHUNK_BYTES.div_ceil(3) * 4;
        assert!(encoded + 1024 < u64::from(crate::stream::MAX_FRAME_LEN) / 2);
    }
}
