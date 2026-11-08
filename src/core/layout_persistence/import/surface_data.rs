//! surface 저장 자료의 payload 형식. journal 모델은 자료 참조만 가지며 내용 해석은 kind별로 여기서 한다.
//!
//! terminal의 cwd·복원 명령·scrollback은 관측해서 저장하는 값이라 이벤트가 아니라 이 자료에 담는다.
//! 형식: 버전 1바이트, 머리 길이(u32 little endian), 머리 JSON, 본문. terminal의 본문은 scrollback
//! 바이트이고 plugin surface는 본문이 없다.

use serde::{Deserialize, Serialize};

/// 이 빌드가 쓰고 읽는 자료 형식 버전.
const FORMAT_VERSION: u8 = 1;
const HEADER_OFFSET: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SurfaceData {
    Terminal {
        cwd: Option<String>,
        restore_command: Option<String>,
        /// 원래 scrollback 파일 ID. 파일이 없어 내용을 담지 못해도 참조는 남긴다.
        scrollback_ref: Option<String>,
        scrollback: Option<Vec<u8>>,
    },
    /// terminal이 아닌 surface가 저장한 자료. cwd는 호스트가 보관하던 값이다.
    Generic {
        data: serde_json::Value,
        cwd: Option<String>,
    },
}

#[derive(Serialize, Deserialize)]
enum Header {
    Terminal {
        cwd: Option<String>,
        restore_command: Option<String>,
        scrollback_ref: Option<String>,
        /// 본문이 비어 있어도 scrollback이 있었는지 구별한다.
        has_scrollback: bool,
    },
    Generic {
        data: serde_json::Value,
        /// 이 키가 없는 옛 저장본은 cwd 없이 읽는다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
}

#[derive(Debug)]
pub(crate) enum SurfaceDataError {
    UnsupportedVersion(u8),
    Truncated,
    Header(serde_json::Error),
}

impl std::fmt::Display for SurfaceDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "surface data format {v} is not supported"),
            Self::Truncated => f.write_str("surface data is truncated"),
            Self::Header(error) => write!(f, "surface data header: {error}"),
        }
    }
}

impl std::error::Error for SurfaceDataError {}

impl SurfaceData {
    /// 저장할 값이 하나도 없으면 자료를 만들지 않는다.
    pub(crate) fn is_empty(&self) -> bool {
        match self {
            Self::Terminal {
                cwd,
                restore_command,
                scrollback_ref,
                scrollback,
            } => {
                cwd.is_none()
                    && restore_command.is_none()
                    && scrollback_ref.is_none()
                    && scrollback.is_none()
            }
            // Even JSON null is a snapshot: restoring it must call restore, never create.
            Self::Generic { .. } => false,
        }
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        let (header, body) = match self {
            Self::Terminal {
                cwd,
                restore_command,
                scrollback_ref,
                scrollback,
            } => (
                Header::Terminal {
                    cwd: cwd.clone(),
                    restore_command: restore_command.clone(),
                    scrollback_ref: scrollback_ref.clone(),
                    has_scrollback: scrollback.is_some(),
                },
                scrollback.as_deref().unwrap_or_default(),
            ),
            Self::Generic { data, cwd } => (
                Header::Generic {
                    data: data.clone(),
                    cwd: cwd.clone(),
                },
                &[][..],
            ),
        };
        let header = serde_json::to_vec(&header)?;
        let len = u32::try_from(header.len()).map_err(serde::ser::Error::custom)?;
        let mut out = Vec::with_capacity(HEADER_OFFSET + header.len() + body.len());
        out.push(FORMAT_VERSION);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(body);
        Ok(out)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, SurfaceDataError> {
        let (&version, rest) = bytes.split_first().ok_or(SurfaceDataError::Truncated)?;
        if version != FORMAT_VERSION {
            return Err(SurfaceDataError::UnsupportedVersion(version));
        }
        let len_bytes: [u8; 4] = rest
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .ok_or(SurfaceDataError::Truncated)?;
        let len = u32::from_le_bytes(len_bytes) as usize;
        let header = rest.get(4..4 + len).ok_or(SurfaceDataError::Truncated)?;
        let body = &rest[4 + len..];
        Ok(
            match serde_json::from_slice(header).map_err(SurfaceDataError::Header)? {
                Header::Terminal {
                    cwd,
                    restore_command,
                    scrollback_ref,
                    has_scrollback,
                } => Self::Terminal {
                    cwd,
                    restore_command,
                    scrollback_ref,
                    scrollback: has_scrollback.then(|| body.to_vec()),
                },
                Header::Generic { data, cwd } => Self::Generic { data, cwd },
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{SurfaceData, SurfaceDataError};

    fn encoded_header(header: &str) -> Vec<u8> {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&u32::try_from(header.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(header.as_bytes());
        bytes
    }

    /// cwd 키가 없던 저장본의 바이트를 그대로 읽는다.
    #[test]
    fn a_generic_capture_saved_without_a_cwd_still_decodes() {
        let saved = encoded_header(r#"{"Generic":{"data":{"file":"/a.md"}}}"#);
        assert_eq!(
            SurfaceData::decode(&saved).expect("decode"),
            SurfaceData::Generic {
                data: serde_json::json!({"file": "/a.md"}),
                cwd: None,
            }
        );
    }

    /// cwd가 없으면 옛 형식과 같은 바이트를 쓰고, 있으면 왕복한다.
    #[test]
    fn a_generic_capture_keeps_its_cwd_and_omits_an_absent_one() {
        let without = SurfaceData::Generic {
            data: serde_json::json!({"file": "/a.md"}),
            cwd: None,
        };
        assert_eq!(
            without.encode().expect("encode"),
            encoded_header(r#"{"Generic":{"data":{"file":"/a.md"}}}"#)
        );
        let with = SurfaceData::Generic {
            data: serde_json::json!({"url": "http://127.0.0.1:9/"}),
            cwd: Some("/work".to_owned()),
        };
        let bytes = with.encode().expect("encode");
        assert_eq!(SurfaceData::decode(&bytes).expect("decode"), with);
    }

    #[test]
    fn empty_scrollback_differs_from_none_and_bad_input_is_rejected() {
        let with_empty = SurfaceData::Terminal {
            cwd: None,
            restore_command: None,
            scrollback_ref: Some("sb".to_owned()),
            scrollback: Some(Vec::new()),
        };
        let bytes = with_empty.encode().expect("encode");
        assert_eq!(SurfaceData::decode(&bytes).expect("decode"), with_empty);

        let mut newer = bytes.clone();
        newer[0] = 2;
        assert!(matches!(
            SurfaceData::decode(&newer),
            Err(SurfaceDataError::UnsupportedVersion(2))
        ));
        assert!(matches!(
            SurfaceData::decode(&bytes[..3]),
            Err(SurfaceDataError::Truncated)
        ));
        assert!(matches!(
            SurfaceData::decode(&[]),
            Err(SurfaceDataError::Truncated)
        ));
    }
}
