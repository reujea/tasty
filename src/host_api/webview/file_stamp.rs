//! 로컬 파일 한 판(version)의 식별값. 스캔한 내용과 나중에 그 경로를 여는 로드가 같은 파일 판을
//! 가리키는지 메타데이터만으로 확인한다(docs/surfaces/html/index.md). 내용을 다시 읽지 않는다.

use std::fs::{File, Metadata};
use std::path::Path;
use std::time::SystemTime;

/// 크기·수정 시각·생성 시각과 파일 식별자(unix는 장치·inode, Windows는 볼륨 일련번호·파일 ID).
/// 제자리 쓰기는 크기나 수정 시각을, 이름 바꾸기로 한 교체는 파일 식별자를 바꾼다. Windows에서는
/// 이름 터널링이 교체한 파일에 옛 생성 시각을 물려주고 수정 시각 해상도가 거칠어, 같은 길이의
/// 교체를 시각만으로는 구별하지 못하므로 파일 ID가 필요하다. 파일 시스템이 이 값들을 모두 그대로
/// 두는 쓰기는 구별하지 못한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    /// 파일 시스템이 ID를 주지 않으면 없다. 그때는 크기·시각만으로 비교한다.
    #[cfg(windows)]
    identity: Option<(u32, u32, u32)>,
}

impl FileStamp {
    /// 열린 파일의 식별값. 메타데이터와 식별자를 같은 핸들에서 읽는다.
    pub fn of(file: &File) -> std::io::Result<Self> {
        let meta = file.metadata()?;
        Ok(Self::from_parts(
            &meta,
            #[cfg(windows)]
            identity_of(file),
        ))
    }

    fn from_parts(meta: &Metadata, #[cfg(windows)] identity: Option<(u32, u32, u32)>) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            #[cfg(unix)]
            dev: meta.dev(),
            #[cfg(unix)]
            ino: meta.ino(),
            #[cfg(windows)]
            identity,
        }
    }

    /// 경로가 지금 가리키는 정규 파일의 식별값. 내용은 읽지 않는다.
    /// unix에서는 FIFO를 열다 막히지 않도록 열지 않고 메타데이터만 조회한다.
    // 이유: Windows backend와 시험만 쓴다.
    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    pub fn of_path(path: &Path) -> Option<Self> {
        #[cfg(windows)]
        {
            let file = File::open(path).ok()?;
            let meta = file.metadata().ok()?;
            meta.is_file()
                .then(|| Self::from_parts(&meta, identity_of(&file)))
        }
        #[cfg(not(windows))]
        {
            let meta = std::fs::metadata(path).ok()?;
            meta.is_file().then(|| Self::from_parts(&meta))
        }
    }
}

/// 볼륨 일련번호와 64비트 파일 인덱스. std의 `MetadataExt::file_index`는 stable이 아니라 Win32를
/// 직접 부른다. 조회에 실패하면 없다.
#[cfg(windows)]
fn identity_of(file: &File) -> Option<(u32, u32, u32)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: file owns the valid handle for the whole call and info is writable storage for it.
    match unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) } {
        Ok(()) => Some((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        )),
        Err(e) => {
            tracing::debug!("file identity unavailable, comparing size and times only: {e}");
            None
        }
    }
}
