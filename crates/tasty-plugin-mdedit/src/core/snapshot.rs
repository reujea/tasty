//! 스냅샷(E5). 외부 쓰기를 감지한 순간의 **직전 내용**과 저장 직전의 **디스크 내용**을 문서별
//! 폴더에 남긴다. 위치는 plugin data dir(업그레이드에도 보존) 아래 `snapshots/<경로 지문>/`.
//!
//! 파일 이름 `<unix ms 13 자리>-<사유>.md` 가 곧 id 다 — 이름순이 시간순이다. 원문 바이트를
//! 그대로 담는다(BOM·CRLF 포함).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::textio::content_hash;

/// 스냅샷을 남긴 이유.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// 외부(에이전트 파일 도구·다른 편집기)가 쓰기 직전의 내용.
    External,
    /// `mdedit` 쓰기 연산 직전의 내용.
    Agent,
    /// 편집기 저장 직전의 디스크 내용.
    Save,
    /// 스냅샷 복원 직전의 내용.
    Restore,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::External => "external",
            Self::Agent => "agent",
            Self::Save => "save",
            Self::Restore => "restore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub reason: String,
    pub at_ms: u64,
    pub bytes: u64,
}

/// 보존 정책.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    pub keep_count: usize,
    pub keep_days: u64,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            keep_count: 50,
            keep_days: 7,
        }
    }
}

pub struct SnapshotStore {
    root: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn is_id(s: &str) -> bool {
    let mut parts = s.splitn(2, '-');
    let ms = parts.next().unwrap_or_default();
    let reason = parts.next().unwrap_or_default();
    ms.len() == 13
        && ms.bytes().all(|b| b.is_ascii_digit())
        && !reason.is_empty()
        && reason.bytes().all(|b| b.is_ascii_lowercase())
}

impl SnapshotStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn dir_for(&self, doc: &Path) -> PathBuf {
        self.root.join(content_hash(&doc.to_string_lossy()))
    }

    /// 새 스냅샷. 가장 최근 스냅샷과 내용이 같으면 남기지 않고 그 id 를 돌려준다.
    pub fn save(
        &self,
        doc: &Path,
        raw: &str,
        reason: Reason,
        keep: Retention,
    ) -> std::io::Result<String> {
        let dir = self.dir_for(doc);
        std::fs::create_dir_all(&dir)?;
        let marker = dir.join("path.txt");
        if !marker.exists() {
            std::fs::write(&marker, doc.to_string_lossy().as_bytes())?;
        }
        let last = self.list(doc)?.into_iter().next();
        if let Some(last) = &last
            && std::fs::read_to_string(dir.join(format!("{}.md", last.id)))
                .ok()
                .as_deref()
                == Some(raw)
        {
            return Ok(last.id.clone());
        }
        // 이름순 = 시간순이 되려면 같은 밀리초에 둘이 생겨도 뒤엣것의 시각이 커야 한다(사유
        // 문자열이 정렬을 뒤집지 않게).
        let ms = now_ms().max(last.map_or(0, |l| l.at_ms + 1));
        let id = format!("{ms:013}-{}", reason.as_str());
        std::fs::write(dir.join(format!("{id}.md")), raw)?;
        self.prune(doc, keep)?;
        Ok(id)
    }

    /// 최신순.
    pub fn list(&self, doc: &Path) -> std::io::Result<Vec<SnapshotInfo>> {
        let dir = self.dir_for(doc);
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut out = Vec::new();
        for entry in rd {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".md") else {
                continue;
            };
            if !is_id(id) {
                continue;
            }
            let (ms, reason) = id.split_once('-').unwrap_or_default();
            out.push(SnapshotInfo {
                id: id.to_string(),
                reason: reason.to_string(),
                at_ms: ms.parse().unwrap_or(0),
                bytes: entry.metadata().map_or(0, |m| m.len()),
            });
        }
        out.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(out)
    }

    pub fn read(&self, doc: &Path, id: &str) -> std::io::Result<String> {
        if !is_id(id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("not a snapshot id: '{id}'"),
            ));
        }
        std::fs::read_to_string(self.dir_for(doc).join(format!("{id}.md")))
    }

    /// 개수·기간 중 하나라도 넘는 오래된 것부터 지운다. 가장 최근 하나는 늘 남긴다.
    pub fn prune(&self, doc: &Path, keep: Retention) -> std::io::Result<()> {
        let dir = self.dir_for(doc);
        let cutoff = now_ms().saturating_sub(keep.keep_days.saturating_mul(86_400_000));
        for (i, s) in self.list(doc)?.iter().enumerate() {
            if i == 0 {
                continue;
            }
            if i >= keep.keep_count.max(1) || s.at_ms < cutoff {
                std::fs::remove_file(dir.join(format!("{}.md", s.id)))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_list_read_dedupe_prune() {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(dir.path().to_path_buf());
        let doc = Path::new("/x/일지.md");
        let keep = Retention {
            keep_count: 2,
            keep_days: 7,
        };
        let a = store.save(doc, "v1", Reason::External, keep).unwrap();
        assert_eq!(store.save(doc, "v1", Reason::Save, keep).unwrap(), a);
        let b = store.save(doc, "v2", Reason::Save, keep).unwrap();
        let c = store.save(doc, "v3", Reason::Agent, keep).unwrap();
        let ids: Vec<_> = store.list(doc).unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![c.clone(), b.clone()]);
        assert_eq!(store.read(doc, &b).unwrap(), "v2");
        assert!(c.ends_with("-agent"));
        assert!(store.read(doc, "../../etc/passwd").is_err());
        assert!(store.list(Path::new("/other.md")).unwrap().is_empty());
    }
}
