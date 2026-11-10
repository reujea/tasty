//! Preserve access controls before putting content into a replacement file.
use std::{
    fs::{File, Metadata},
    io,
};

#[cfg(unix)]
pub(super) fn preserve(source: &File, target: &File, metadata: &Metadata) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let next = target.metadata()?;
    if (next.uid(), next.gid()) != (metadata.uid(), metadata.gid()) {
        rustix::fs::fchown(
            target,
            Some(rustix::fs::Uid::from_raw(metadata.uid())),
            Some(rustix::fs::Gid::from_raw(metadata.gid())),
        )?;
    }
    target.set_permissions(metadata.permissions())?;
    copy_attributes(source, target)
}

#[cfg(target_os = "linux")]
fn attributes(file: &File) -> io::Result<Vec<(std::ffi::CString, Vec<u8>)>> {
    let len = match rustix::fs::flistxattr_query(file) {
        Ok(len) => len,
        Err(rustix::io::Errno::OPNOTSUPP) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut names = vec![0u8; len];
    let len = rustix::fs::flistxattr(file, names.as_mut_slice())?;
    names.truncate(len);
    let mut attrs = Vec::new();
    for name in names.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
        let len = rustix::fs::fgetxattr_query(file, &name)?;
        let mut value = vec![0u8; len];
        let len = rustix::fs::fgetxattr(file, &name, value.as_mut_slice())?;
        value.truncate(len);
        attrs.push((name, value));
    }
    Ok(attrs)
}

#[cfg(target_os = "linux")]
fn copy_attributes(source: &File, target: &File) -> io::Result<()> {
    let source = attributes(source)?;
    let current = attributes(target)?;
    for (name, _) in &current {
        if !source.iter().any(|(other, _)| other == name) {
            rustix::fs::fremovexattr(target, name)?;
        }
    }
    for (name, value) in source {
        if current.iter().any(|(n, v)| n == &name && v == &value) {
            continue;
        }
        rustix::fs::fsetxattr(target, &name, &value, rustix::fs::XattrFlags::empty())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_attributes(source: &File, target: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: both owned files remain open; null requests copyfile's temporary state.
    if unsafe {
        libc::fcopyfile(
            source.as_raw_fd(),
            target.as_raw_fd(),
            std::ptr::null_mut(),
            libc::COPYFILE_ACL | libc::COPYFILE_XATTR,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn copy_attributes(_source: &File, _target: &File) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic replacement metadata is unsupported on this platform",
    ))
}

#[cfg(not(unix))]
pub(super) fn preserve(_source: &File, target: &File, metadata: &Metadata) -> io::Result<()> {
    #[cfg(windows)]
    if mandatory_labels(_source)? != mandatory_labels(target)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "atomic replacement could not preserve the mandatory integrity label",
        ));
    }
    target.set_permissions(metadata.permissions())
}

#[cfg(windows)]
fn mandatory_labels(file: &File) -> io::Result<Vec<Vec<u8>>> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Security::{
        ACE_HEADER, GetAce, GetKernelObjectSecurity, GetSecurityDescriptorSacl,
        LABEL_SECURITY_INFORMATION,
    };
    let mut length = 0;
    // SAFETY: the file handle is live; a null buffer requests the required capacity.
    unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            LABEL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut length,
        );
    }
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut descriptor = vec![0u8; length as usize];
    // SAFETY: descriptor has length writable bytes and the owned file remains open.
    if unsafe {
        GetKernelObjectSecurity(
            file.as_raw_handle(),
            LABEL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl = std::ptr::null_mut();
    // SAFETY: the OS returned a valid descriptor; acl points into its live allocation.
    if unsafe {
        GetSecurityDescriptorSacl(
            descriptor.as_mut_ptr().cast(),
            &mut present,
            &mut acl,
            &mut defaulted,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if present == 0 || acl.is_null() {
        return Ok(Vec::new());
    }
    let mut labels = Vec::new();
    // SAFETY: acl is a valid ACL inside the live OS-provided descriptor.
    for index in 0..unsafe { (*acl).AceCount } {
        let mut ace = std::ptr::null_mut();
        // SAFETY: index is within AceCount; the descriptor and ACL remain live.
        if unsafe { GetAce(acl, u32::from(index), &mut ace) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: GetAce returned a valid ACE whose header records its allocated size.
        let size = unsafe { (*ace.cast::<ACE_HEADER>()).AceSize };
        // SAFETY: the valid ACE occupies size bytes; copy before descriptor is dropped.
        labels.push(
            unsafe { std::slice::from_raw_parts(ace.cast::<u8>(), usize::from(size)) }.to_vec(),
        );
    }
    Ok(labels)
}

#[cfg(windows)]
pub(super) fn create(
    path: &std::path::Path,
    source: &std::path::Path,
    existing: bool,
) -> io::Result<File> {
    use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
    use windows_sys::Win32::{
        Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Security::{
            DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetFileSecurityW,
            LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL},
    };
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = OWNER_SECURITY_INFORMATION
        | GROUP_SECURITY_INFORMATION
        | DACL_SECURITY_INFORMATION
        | LABEL_SECURITY_INFORMATION;
    let mut length = 0;
    let mut descriptor = Vec::new();
    if existing {
        // SAFETY: source is NUL terminated; the null output asks for the required capacity.
        unsafe {
            GetFileSecurityW(source.as_ptr(), flags, std::ptr::null_mut(), 0, &mut length);
        }
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        descriptor.resize(length as usize, 0u8);
        // SAFETY: descriptor is writable for length bytes and lives through CreateFileW.
        if unsafe {
            GetFileSecurityW(
                source.as_ptr(),
                flags,
                descriptor.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: if existing {
            descriptor.as_mut_ptr().cast()
        } else {
            std::ptr::null_mut()
        },
        bInheritHandle: 0,
    };
    // SAFETY: all pointers refer to live allocations; CREATE_NEW cannot replace an existing file.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            0,
            &security,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreateFileW returned a uniquely owned valid handle.
    Ok(unsafe { File::from_raw_handle(handle) })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetFileSecurityW,
        LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
    };
    fn wide(path: &std::path::Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    fn descriptor(path: &std::path::Path) -> Vec<u8> {
        let path = wide(path);
        let flags = OWNER_SECURITY_INFORMATION
            | GROUP_SECURITY_INFORMATION
            | DACL_SECURITY_INFORMATION
            | LABEL_SECURITY_INFORMATION;
        let mut size = 0;
        // SAFETY: NUL terminated path and a writable size output; the first call only sizes the buffer.
        unsafe {
            GetFileSecurityW(path.as_ptr(), flags, std::ptr::null_mut(), 0, &mut size);
        }
        assert!(size > 0, "{}", io::Error::last_os_error());
        let mut bytes = vec![0u8; size as usize];
        assert_ne!(
            // SAFETY: bytes has the requested capacity and all pointers remain live.
            unsafe {
                GetFileSecurityW(
                    path.as_ptr(),
                    flags,
                    bytes.as_mut_ptr().cast(),
                    size,
                    &mut size,
                )
            },
            0
        );
        bytes.truncate(size as usize);
        bytes
    }
    #[test]
    fn protected_security_descriptor_is_present_before_payload_and_after_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("controlled");
        std::fs::write(&path, b"old").unwrap();
        let mut initial = descriptor(&path);
        let name = wide(&path);
        assert_ne!(
            // SAFETY: descriptor is self-relative, obtained from GetFileSecurityW; only this test file changes.
            unsafe {
                SetFileSecurityW(
                    name.as_ptr(),
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    initial.as_mut_ptr().cast(),
                )
            },
            0
        );
        let expected = descriptor(&path);
        super::super::replace_using(&path, super::super::Options::default(), |file| {
            let temp = std::fs::read_dir(dir.path())?
                .map(|e| e.unwrap().path())
                .find(|p| p.extension().is_some_and(|e| e == "tmp"))
                .unwrap();
            assert_eq!(descriptor(&temp), expected);
            assert_eq!(file.metadata()?.len(), 0);
            std::io::Write::write_all(file, b"new")
        })
        .unwrap();
        assert_eq!(descriptor(&path), expected);
    }

    /// Mandatory label ACEs read by path, independently of the handle-based product check.
    fn label_aces(path: &std::path::Path) -> Vec<Vec<u8>> {
        use windows_sys::Win32::Security::{ACE_HEADER, GetAce, GetSecurityDescriptorSacl};
        let name = wide(path);
        let mut size = 0;
        // SAFETY: NUL terminated path and a writable size output; the first call only sizes the buffer.
        unsafe {
            GetFileSecurityW(
                name.as_ptr(),
                LABEL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut size,
            );
        }
        assert!(size > 0, "{}", io::Error::last_os_error());
        let mut bytes = vec![0u8; size as usize];
        assert_ne!(
            // SAFETY: bytes has the requested capacity and all pointers remain live.
            unsafe {
                GetFileSecurityW(
                    name.as_ptr(),
                    LABEL_SECURITY_INFORMATION,
                    bytes.as_mut_ptr().cast(),
                    size,
                    &mut size,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl = std::ptr::null_mut();
        assert_ne!(
            // SAFETY: bytes holds the self-relative descriptor returned above; acl points into it.
            unsafe {
                GetSecurityDescriptorSacl(
                    bytes.as_mut_ptr().cast(),
                    &mut present,
                    &mut acl,
                    &mut defaulted,
                )
            },
            0,
            "{}",
            io::Error::last_os_error()
        );
        if present == 0 || acl.is_null() {
            return Vec::new();
        }
        // SAFETY: acl is a valid ACL inside bytes, which stays alive for the loop.
        (0..unsafe { (*acl).AceCount })
            .map(|index| {
                let mut ace = std::ptr::null_mut();
                // SAFETY: index is below AceCount; bytes and the ACL remain live.
                assert_ne!(unsafe { GetAce(acl, u32::from(index), &mut ace) }, 0);
                // SAFETY: GetAce returned a valid ACE whose header records its size.
                let len = usize::from(unsafe { (*ace.cast::<ACE_HEADER>()).AceSize });
                // SAFETY: the ACE occupies len bytes inside bytes; copied before bytes is dropped.
                unsafe { std::slice::from_raw_parts(ace.cast::<u8>(), len) }.to_vec()
            })
            .collect()
    }

    /// Asserts exactly one mandatory label ACE whose SID is S-1-16-<rid>.
    fn assert_single_label(aces: &[Vec<u8>], rid: u32) {
        // SYSTEM_MANDATORY_LABEL_ACE_TYPE; its SID starts after the 4 byte header and 4 byte mask.
        const LABEL_ACE: u8 = 0x11;
        assert_eq!(aces.len(), 1, "exactly one label ACE: {aces:?}");
        let ace = &aces[0];
        assert_eq!(ace[0], LABEL_ACE, "label ACE type: {ace:?}");
        let mut sid = vec![1u8, 1, 0, 0, 0, 0, 0, 16];
        sid.extend_from_slice(&rid.to_le_bytes());
        assert_eq!(ace.get(8..), Some(sid.as_slice()), "label SID S-1-16-{rid}");
    }

    fn level_rid(level: &str) -> u32 {
        match level {
            "L" => 0x1000,
            "H" => 0x3000,
            other => panic!("unexpected integrity level {other}"),
        }
    }

    fn integrity_label_survives_or_replacement_is_rejected(level: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("controlled-label");
        std::fs::write(&path, b"old").unwrap();
        let output = std::process::Command::new("icacls")
            .arg(&path)
            .args(["/setintegritylevel", level])
            .output()
            .unwrap();
        assert!(output.status.success(), "icacls: {output:?}");
        let expected = label_aces(&path);
        assert_single_label(&expected, level_rid(level));
        let mut payload_called = false;
        let result = super::super::replace_using(&path, super::super::Options::default(), |file| {
            payload_called = true;
            let temp = std::fs::read_dir(dir.path())?
                .map(|entry| entry.unwrap().path())
                .find(|path| path.extension().is_some_and(|extension| extension == "tmp"))
                .unwrap();
            assert_eq!(
                label_aces(&temp),
                expected,
                "label must exist before payload"
            );
            assert_eq!(file.metadata()?.len(), 0);
            std::io::Write::write_all(file, b"new")
        });
        assert_eq!(label_aces(&path), expected, "original label must survive");
        match result {
            Ok(()) => {
                assert!(payload_called);
                assert_eq!(std::fs::read(&path).unwrap(), b"new");
                tracing::info!(
                    level,
                    "integrity label preserved before payload and after replacement"
                );
            }
            Err(error) => {
                assert!(
                    !payload_called,
                    "security rejection must precede payload: {error}"
                );
                assert_eq!(std::fs::read(&path).unwrap(), b"old");
                tracing::info!(level, %error, "integrity label safely rejected before payload");
            }
        }
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no owned temp remains"
        );
    }

    #[test]
    fn changed_integrity_label_is_rejected_before_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source");
        let temp = dir.path().join("target");
        std::fs::write(&path, b"old").unwrap();
        let output = std::process::Command::new("icacls")
            .arg(&path)
            .args(["/setintegritylevel", "L"])
            .output()
            .unwrap();
        assert!(output.status.success(), "icacls: {output:?}");
        let source = File::open(&path).unwrap();
        let target = create(&temp, &path, false).unwrap();
        let expected = label_aces(&path);
        assert_single_label(&expected, level_rid("L"));
        let result = preserve(&source, &target, &source.metadata().unwrap());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(target.metadata().unwrap().len(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        assert_eq!(label_aces(&path), expected);
    }

    #[test]
    fn low_integrity_label_is_preserved_before_payload_or_safely_rejected() {
        integrity_label_survives_or_replacement_is_rejected("L");
    }

    #[test]
    fn high_integrity_label_is_preserved_before_payload_or_safely_rejected() {
        integrity_label_survives_or_replacement_is_rejected("H");
    }
}
