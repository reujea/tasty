//! 경로는 tasty_home(TASTY_HOME 또는 debug/release 기본 홈)을 따른다.
//! 자격증명 목록은 passkeys.toml에 경로로 저장한다. path는 사용자 파일을 참조하고
//! inline은 Tasty가 passkeys/<name> 파일을 만들어 관리한다. 비밀 파일은 암호화하지 않는다.
//! inline 파일은 payload 쓰기 전 Unix 0600 또는 Windows 보호 ACL을 적용한다.
//! 명령형 저장은 RemoteFileStore를 사용한다. load/save는 기존 소비자용 호환 API다.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tasty_utils::path::tasty_home;

/// 알려진 passkey kind. 그 외(미등록)도 저장은 되되 UI/CLI 가 경고 표시(상위).
pub const KNOWN_PASSKEY_KINDS: &[&str] = &["path", "inline"];

/// 파일명으로 사용할 이름은 ASCII 영숫자·하이픈·밑줄만 허용한다.
pub fn is_valid_passkey_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// 자동 생성용 이름. 허용하지 않는 문자는 밑줄로 바꾸고 빈 결과는 key로 대체한다.
pub fn sanitize_passkey_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "key".to_string() } else { s }
}

/// 한 자격증명. `path` 는 항상 파일 경로(inline 이면 tasty 관리 파일 경로).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passkey {
    /// 고유 식별자(프로필이 `passkey_ref` 로 참조).
    pub name: String,
    /// `path` | `inline`(+ 미등록). 라이프사이클 소유권을 가른다.
    pub kind: String,
    /// at-rest 경로. inline 이면 `~/.tasty/passkeys/<name>`.
    pub path: String,
}

impl Passkey {
    /// tasty 가 소유·관리하는 inline 파일인지(삭제 시 파일도 지운다).
    pub fn is_managed(&self) -> bool {
        self.kind == "inline"
    }
}

/// `~/.tasty/passkeys.toml` 전체 — 자격증명 목록 + 스키마 버전.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Passkeys {
    #[serde(default)]
    pub version: u32,
    #[serde(default, rename = "passkey")]
    pub passkeys: Vec<Passkey>,
}

/// `~/.tasty/passkeys.toml` 경로.
pub fn passkeys_file() -> Option<PathBuf> {
    tasty_home().map(|dir| dir.join("passkeys.toml"))
}

/// inline 비밀 파일 디렉토리 `~/.tasty/passkeys/`.
pub fn passkeys_dir() -> Option<PathBuf> {
    tasty_home().map(|dir| dir.join("passkeys"))
}

/// 파일 권한 0600(unix). 다른 OS 는 no-op(유저 프로필 ACL 에 위임).
fn set_private_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    let _ = path; // non-unix: 권한 설정 no-op — 인자 미사용(값 drop, Result 아님).
    Ok(())
}

/// 주어진 디렉토리에 inline 비밀을 0600 파일로 쓰고 그 경로를 반환한다(경로 주입형 —
/// 테스트가 temp dir 로 호출). 이름은 [`is_valid_passkey_name`] 통과 필수.
pub fn materialize_inline_in(dir: &Path, name: &str, secret: &str) -> Result<PathBuf> {
    crate::secret_file::write(dir, name, secret)
}

impl Passkeys {
    /// 저장 파일 경로.
    pub fn path() -> Option<PathBuf> {
        passkeys_file()
    }

    fn ensure_dir() -> Result<()> {
        if let Some(path) = Self::path()
            && let Some(parent) = path.parent()
        {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    /// 로드한다. 없거나 파싱 실패면 빈 목록(default)으로 폴백한다.
    #[allow(clippy::cognitive_complexity)] // complexity-exempt: 파일 없음/읽기 실패/파싱 실패 3갈래를 각각 로그 후 기본값 폴백 — RemoteProfiles::load 와 동형 패턴(설정 파일이 없어도 부팅을 막지 않는다는 정책의 반복 구현).
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            tracing::info!("no passkeys path available, using empty list");
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<Passkeys>(&contents) {
                Ok(p) => {
                    tracing::info!(
                        "loaded {} passkey(s) from {}",
                        p.passkeys.len(),
                        path.display()
                    );
                    p
                }
                Err(_) => {
                    tracing::warn!(
                        file = "passkeys.toml",
                        cause = "invalid TOML",
                        "could not load passkey list; original preserved, using empty list"
                    );
                    Self::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(file = "passkeys.toml", "no passkey file, using empty list");
                Self::default()
            }
            Err(error) => {
                tracing::warn!(file = "passkeys.toml", cause = ?error.kind(), "could not read passkey list; original preserved, using empty list");
                Self::default()
            }
        }
    }

    /// TOML 전체를 덮어쓴 뒤 Unix 권한 0600을 시도한다. 권한 설정 실패는 무시한다.
    pub fn save(&self) -> Result<()> {
        Self::ensure_dir()?;
        let Some(path) = Self::path() else {
            anyhow::bail!("could not determine passkeys path");
        };
        let mut to_write = self.clone();
        if to_write.version == 0 {
            to_write.version = 1;
        }
        let contents = toml::to_string_pretty(&to_write)?;
        fs::write(&path, contents)?;
        set_private_file(&path).ok(); // best-effort
        tracing::info!(
            "saved {} passkey(s) to {}",
            to_write.passkeys.len(),
            path.display()
        );
        Ok(())
    }

    /// 이름으로 조회.
    pub fn get(&self, name: &str) -> Option<&Passkey> {
        self.passkeys.iter().find(|p| p.name == name)
    }

    /// name 이 같으면 교체, 없으면 추가(메모리만 — 디스크 반영은 [`Self::save`]).
    pub fn upsert(&mut self, passkey: Passkey) {
        if let Some(existing) = self.passkeys.iter_mut().find(|p| p.name == passkey.name) {
            *existing = passkey;
        } else {
            self.passkeys.push(passkey);
        }
    }

    /// path kind passkey 등록(이름 검증). 메모리만 변경.
    pub fn upsert_path(
        &mut self,
        name: impl Into<String>,
        file_path: impl Into<String>,
    ) -> Result<()> {
        let name = name.into();
        if !is_valid_passkey_name(&name) {
            bail!("invalid passkey name '{name}' (allowed: letters, digits, - and _)");
        }
        self.upsert(Passkey {
            name,
            kind: "path".into(),
            path: file_path.into(),
        });
        Ok(())
    }

    /// inline 비밀 파일을 실제로 쓰고 메모리 목록에 경로를 등록한다.
    /// TOML 목록 저장은 별도의 save 호출이 필요하다.
    pub fn upsert_inline(&mut self, name: impl Into<String>, secret: &str) -> Result<()> {
        let name = name.into();
        let dir = passkeys_dir().context("could not determine passkeys dir")?;
        let path = materialize_inline_in(&dir, &name, secret)?;
        self.upsert(Passkey {
            name,
            kind: "inline".into(),
            path: path.to_string_lossy().into_owned(),
        });
        Ok(())
    }

    /// 목록에서 제거하고 inline 관리 파일도 삭제를 시도한다. 파일 삭제 실패는 경고하며 목록 제거는 유지한다.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(idx) = self.passkeys.iter().position(|p| p.name == name) else {
            return false;
        };
        let pk = self.passkeys.remove(idx);
        if pk.is_managed()
            && let Err(e) = tasty_utils::file_replace::remove(std::path::Path::new(&pk.path))
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("failed to remove managed passkey file {}: {e}", pk.path);
        }
        true
    }
}

#[cfg(test)]
// 이유: 테스트의 let _ =를 제품 코드의 오류 처리 명부에서 제외한다.
#[allow(clippy::let_underscore_must_use)]
mod tests {
    use super::*;

    #[test]
    fn valid_name_whitelist() {
        assert!(is_valid_passkey_name("host-key"));
        assert!(is_valid_passkey_name("my_key_1"));
        assert!(!is_valid_passkey_name("")); // 빈 값
        assert!(!is_valid_passkey_name("my key")); // 공백
        assert!(!is_valid_passkey_name("../etc/passwd")); // traversal
        assert!(!is_valid_passkey_name("a/b")); // 슬래시
        assert!(!is_valid_passkey_name("키")); // 비-ASCII
    }

    #[test]
    fn sanitize_replaces_and_never_empty() {
        assert_eq!(sanitize_passkey_name("host-key"), "host-key");
        assert_eq!(sanitize_passkey_name("my key"), "my_key");
        assert_eq!(sanitize_passkey_name("../etc"), "___etc");
        assert_eq!(sanitize_passkey_name("키"), "_"); // 전부 치환되어도 비지 않음
        for raw in ["내 서버", "a b/c", "x", ""] {
            assert!(
                is_valid_passkey_name(&sanitize_passkey_name(raw))
                    || sanitize_passkey_name(raw) == "_"
            );
        }
    }

    #[test]
    fn passkeys_toml_roundtrip() {
        let mut pk = Passkeys::default();
        pk.upsert(Passkey {
            name: "k1".into(),
            kind: "path".into(),
            path: "~/.ssh/id".into(),
        });
        let s = toml::to_string_pretty(&pk).unwrap();
        let back: Passkeys = toml::from_str(&s).unwrap();
        assert_eq!(back.get("k1").unwrap().path, "~/.ssh/id");
        assert_eq!(back.get("k1").unwrap().kind, "path");
    }

    #[test]
    fn upsert_path_rejects_bad_name() {
        let mut pk = Passkeys::default();
        assert!(pk.upsert_path("../x", "~/p").is_err());
        assert!(pk.upsert_path("ok-1", "~/p").is_ok());
        assert_eq!(pk.get("ok-1").unwrap().path, "~/p");
    }

    #[test]
    fn materialize_inline_writes_file_and_rejects_bad_name() {
        let dir = std::env::temp_dir().join(format!("tasty-pk-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir); // 테스트 정리 — 디렉토리 부재 에러는 정상, 무시.
        let path = materialize_inline_in(&dir, "tok", "s3cr3t").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "s3cr3t");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        assert!(materialize_inline_in(&dir, "../escape", "x").is_err());
        let _ = fs::remove_dir_all(&dir); // 테스트 정리 — 디렉토리 부재 에러는 정상, 무시.
    }

    #[cfg(unix)]
    fn file_mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    fn leftover_temp_files(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn materialize_inline_creates_private_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("passkeys");
        let path = materialize_inline_in(&dir, "fresh", "s3cr3t").unwrap();
        assert_eq!(file_mode(&path), 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "s3cr3t");
        assert!(leftover_temp_files(&dir).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn materialize_inline_overwrite_does_not_keep_wide_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("passkeys");
        fs::create_dir_all(&dir).unwrap();
        let existing = dir.join("tok");
        fs::write(&existing, "old").unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();
        // 덮어쓰기는 기존 inode를 고치지 않고 새 파일로 바꿔 넣어야 한다.
        let old_ino = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&existing).unwrap().ino()
        };
        let path = materialize_inline_in(&dir, "tok", "new-secret").unwrap();
        assert_eq!(path, existing);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new-secret");
        assert_eq!(file_mode(&path), 0o600);
        let new_ino = {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(&path).unwrap().ino()
        };
        assert_ne!(old_ino, new_ino);
        assert!(leftover_temp_files(&dir).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn materialize_inline_failure_leaves_no_temp_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("passkeys");
        // 대상 이름이 비어 있지 않은 디렉터리면 rename이 실패한다.
        fs::create_dir_all(dir.join("tok").join("child")).unwrap();
        assert!(materialize_inline_in(&dir, "tok", "s3cr3t").is_err());
        assert!(leftover_temp_files(&dir).is_empty());
        assert!(dir.join("tok").is_dir());
    }

    #[test]
    fn remove_managed_deletes_file() {
        let dir = std::env::temp_dir().join(format!("tasty-pk-rm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir); // 테스트 정리 — 디렉토리 부재 에러는 정상, 무시.
        let path = materialize_inline_in(&dir, "tok", "x").unwrap();
        let mut pk = Passkeys::default();
        pk.upsert(Passkey {
            name: "tok".into(),
            kind: "inline".into(),
            path: path.to_string_lossy().into_owned(),
        });
        assert!(path.exists());
        assert!(pk.remove("tok"));
        assert!(!path.exists()); // 관리 파일 삭제됨
        assert!(!pk.remove("tok")); // 이미 없음
        let _ = fs::remove_dir_all(&dir); // 테스트 정리 — 디렉토리 부재 에러는 정상, 무시.
    }

    #[test]
    fn remove_path_kind_keeps_file() {
        // path kind 는 사용자 소유라 파일을 지우지 않는다.
        let dir = std::env::temp_dir().join(format!("tasty-pk-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir); // 테스트 정리 — 디렉토리 부재 에러는 정상, 무시.
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("user-key");
        fs::write(&file, "owned").unwrap();
        let mut pk = Passkeys::default();
        pk.upsert(Passkey {
            name: "ref".into(),
            kind: "path".into(),
            path: file.to_string_lossy().into_owned(),
        });
        assert!(pk.remove("ref"));
        assert!(file.exists()); // 참조 해제했을 뿐 파일 보존
        let _ = fs::remove_dir_all(&dir); // 테스트 정리 — 디렉토리 부재 에러는 정상, 무시.
    }
}
