//! Private secret replacement. The ordinary helper deliberately preserves wide old modes.
use anyhow::{Context, Result, bail};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tasty_utils::file_replace::{self, Fingerprint};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write(dir: &Path, name: &str, secret: &str) -> Result<PathBuf> {
    if !crate::is_valid_passkey_name(name) {
        bail!("invalid passkey name");
    }
    fs::create_dir_all(dir)?;
    reject_link(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join(name);
    reject_link(&path)?;
    let _guard = file_replace::lock(&path)?;
    reject_link(&path)?;
    let observed = Fingerprint::read(&path)?;
    if path.exists() {
        OpenOptions::new().write(true).open(&path)?;
    }
    let attempt = file_replace::observe_replacement(secret.len());
    let result = (|| -> Result<()> {
        let (temp, mut file) = create(dir, name)?;
        let result = (|| -> Result<()> {
            secure(&temp, &file)?;
            file.write_all(secret.as_bytes())
                .context("write private secret")?;
            file.sync_all()?;
            drop(file);
            reject_link(&path)?;
            if Fingerprint::read(&path)? != observed {
                bail!("secret file changed during mutation");
            }
            fs::rename(&temp, &path).context("replace private secret")?;
            Ok(())
        })();
        if result.is_err()
            && let Err(e) = fs::remove_file(&temp)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(error=%e, "could not clean private temporary file");
        }
        result
    })();
    attempt.finish(result.as_ref().copied().map_err(|error| {
        error
            .downcast_ref::<std::io::Error>()
            .map_or(std::io::ErrorKind::Other, std::io::Error::kind)
    }));
    result?;
    Ok(path)
}
fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            bail!("managed secret symlink refused; original preserved")
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn create(dir: &Path, name: &str) -> Result<(PathBuf, File)> {
    loop {
        let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = dir.join(format!(".{name}.tmp-{}-{n}", std::process::id()));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
#[cfg(unix)]
fn secure(_path: &Path, file: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // Also remove any inherited ACL's effective group permissions before payload write.
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}
#[cfg(windows)]
fn secure(path: &Path, _file: &File) -> Result<()> {
    // PowerShell invokes the OS ACL APIs on the still-empty file. The path is an environment
    // value, never interpolated into code, and no secret enters the child process.
    let script = r#"$ErrorActionPreference='Stop';
$p=$env:TASTY_PRIVATE_TEMP_PATH;
$sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User;
$acl=New-Object System.Security.AccessControl.FileSecurity;
$acl.SetOwner($sid); $acl.SetAccessRuleProtection($true,$false);
$rule=[System.Security.AccessControl.FileSystemAccessRule]::new($sid,[System.Security.AccessControl.FileSystemRights]::FullControl,[System.Security.AccessControl.AccessControlType]::Allow);
$acl.AddAccessRule($rule); Set-Acl -LiteralPath $p -AclObject $acl;
$actual=Get-Acl -LiteralPath $p;
$rules=@($actual.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier]));
if ($actual.GetOwner([System.Security.Principal.SecurityIdentifier]) -ne $sid -or !$actual.AreAccessRulesProtected -or $rules.Count -ne 1 -or $rules[0].IdentityReference -ne $sid -or $rules[0].AccessControlType -ne 'Allow' -or $rules[0].FileSystemRights -ne 'FullControl') { throw 'private ACL verification failed' }
"#;
    let result = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("TASTY_PRIVATE_TEMP_PATH", path)
        // A PSModulePath inherited from PowerShell 7 makes Windows PowerShell load modules it
        // cannot use, so Set-Acl and Get-Acl are missing. Without it the child uses its defaults.
        .env_remove("PSModulePath")
        .output()
        .context("configure private secret ACL")?;
    if !result.status.success() {
        bail!(
            "private secret ACL setup/verification failed; no payload written: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(())
}
#[cfg(not(any(unix, windows)))]
fn secure(_path: &Path, _file: &File) -> Result<()> {
    bail!("private secret files unsupported on this platform")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn wide_acl_is_replaced_with_verified_private_acl_and_no_temp_remains() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        fs::write(&path, "old fixture").unwrap();
        let wide = std::process::Command::new("icacls.exe")
            .arg(&path)
            .args(["/grant", "*S-1-1-0:(F)"])
            .output()
            .unwrap();
        assert!(wide.status.success());
        // write() verifies the actual protected ACL before the first payload byte.
        write(dir.path(), "key", "new fixture").unwrap();
        let verification = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                r#"$ErrorActionPreference='Stop';
$a=Get-Acl -LiteralPath $env:TASTY_PRIVATE_TEMP_PATH;
$s=[System.Security.Principal.WindowsIdentity]::GetCurrent().User;
$r=@($a.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier]));
if (!$a.AreAccessRulesProtected -or $r.Count -ne 1 -or $r[0].IdentityReference -ne $s) { exit 1 }
"#,
            ])
            .env("TASTY_PRIVATE_TEMP_PATH", &path)
            .output()
            .unwrap();
        assert!(verification.status.success());
        assert_eq!(fs::read_to_string(path).unwrap(), "new fixture");
        assert!(
            !fs::read_dir(dir.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-"))
        );
    }
}
