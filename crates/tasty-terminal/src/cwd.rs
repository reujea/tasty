use std::path::PathBuf;

/// Get the current working directory of a process by PID.
/// Returns None if the PID is invalid or the cwd cannot be determined.
///
/// Windows에서는 항상 None을 반환한다. 다른 프로세스의 CWD를 얻는 표준 API가
/// 없고, WMI/PowerShell 호출은 콘솔창을 띄우는 무거운 동작이라 폴링에 부적합하다.
/// Windows에서는 OSC 7 시퀀스에만 의존한다 (Git Bash는 합성 rcfile이 보낸다).
pub fn get_cwd_of_pid(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{}/cwd", pid)).ok()
    }

    #[cfg(target_os = "macos")]
    {
        macos_proc_cwd(pid)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _pid = pid;
        None
    }
}

#[cfg(target_os = "macos")]
fn macos_proc_cwd(pid: u32) -> Option<PathBuf> {
    // Query libproc directly rather than spawning lsof. C layout sizes:
    // vinfo_stat=136, vnode_info=152, vnode_info_path=1176, proc_vnodepathinfo=2352.
    use std::mem::MaybeUninit;

    const PROC_PIDVNODEPATHINFO: libc::c_int = 9;

    #[repr(C)]
    struct VInfoStat {
        _pad: [u8; 136],
    }

    #[repr(C)]
    struct VnodeInfo {
        _stat: VInfoStat,
        _type: libc::c_int,
        _padding: [u8; 12], // alignment padding to reach 152 bytes total
    }

    #[repr(C)]
    struct VnodeInfoPath {
        vi: VnodeInfo,
        path: [u8; libc::PATH_MAX as usize],
    }

    #[repr(C)]
    struct ProcVnodePathInfo {
        cdir: VnodeInfoPath,
        rdir: VnodeInfoPath,
    }

    const _: () = assert!(std::mem::size_of::<ProcVnodePathInfo>() == 2352);

    let mut info = MaybeUninit::<ProcVnodePathInfo>::zeroed();
    // SAFETY: zeroed storage is valid for these integer/array fields. The pointer and size
    // describe this call's local ProcVnodePathInfo, whose layout is checked above.
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr() as *mut libc::c_void,
            std::mem::size_of::<ProcVnodePathInfo>() as libc::c_int,
        )
    };
    if ret <= 0 {
        return None;
    }
    // SAFETY: all fields were valid after zero-initialization, including any bytes not written by libproc.
    let info = unsafe { info.assume_init() };
    let path_bytes = &info.cdir.path;
    let len = path_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(path_bytes.len());
    if len == 0 {
        return None;
    }
    let path_str = std::str::from_utf8(&path_bytes[..len]).ok()?;
    Some(PathBuf::from(path_str))
}
