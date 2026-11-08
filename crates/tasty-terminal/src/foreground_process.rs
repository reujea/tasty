//! Unix foreground process-group lookup and Windows process-tree approximation.

#[cfg(any(windows, test))]
#[path = "foreground_process/tree.rs"]
mod tree;

/// Info about the foreground process.
#[derive(Debug, Clone)]
pub struct ForegroundProcessInfo {
    pub name: String,
    pub pid: u32,
}

/// Whether `name` matches a recognised interactive shell. Used to classify a
/// terminal as idle when its foreground process is the shell itself rather
/// than a user-launched program.
pub fn is_known_shell_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    matches!(
        stem,
        "bash"
            | "zsh"
            | "fish"
            | "sh"
            | "dash"
            | "ksh"
            | "tcsh"
            | "csh"
            | "nu"
            | "xonsh"
            | "elvish"
            | "pwsh"
            | "powershell"
            | "cmd"
    )
}

/// Get the foreground process info for a given shell PID.
pub fn get_foreground_process(shell_pid: u32) -> Option<ForegroundProcessInfo> {
    #[cfg(target_os = "linux")]
    {
        linux_foreground_process(shell_pid)
    }

    #[cfg(target_os = "macos")]
    {
        macos_foreground_process(shell_pid)
    }

    #[cfg(windows)]
    {
        windows_foreground_process(shell_pid)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _shell_pid = shell_pid;
        None
    }
}

/// Resolve multiple shell PIDs, returning results in the same order.
/// Windows shares one system process snapshot across all lookups. Linux/macOS query each PID.
pub fn resolve_foreground_many(shell_pids: &[u32]) -> Vec<Option<ForegroundProcessInfo>> {
    #[cfg(windows)]
    {
        let snapshot = capture_windows_snapshot();
        shell_pids
            .iter()
            .map(|&pid| snapshot.as_ref().and_then(|s| s.resolve(pid)))
            .collect()
    }

    #[cfg(not(windows))]
    {
        shell_pids
            .iter()
            .map(|&pid| get_foreground_process(pid))
            .collect()
    }
}

#[cfg(target_os = "linux")]
fn linux_foreground_process(shell_pid: u32) -> Option<ForegroundProcessInfo> {
    // Read /proc/<pid>/stat to get tpgid (foreground process group ID)
    // Format: pid (comm) state ppid pgrp session tty_nr tpgid ...
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", shell_pid)).ok()?;
    let after_comm = stat.rfind(')')? + 2;
    let fields: Vec<&str> = stat[after_comm..].split_whitespace().collect();
    // fields: [0]=state [1]=ppid [2]=pgrp [3]=session [4]=tty_nr [5]=tpgid
    let tpgid: u32 = fields.get(5)?.parse().ok()?;
    if tpgid == 0 {
        return None;
    }
    // Get process name from /proc/<tpgid>/comm
    let name = std::fs::read_to_string(format!("/proc/{}/comm", tpgid))
        .ok()?
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    Some(ForegroundProcessInfo { name, pid: tpgid })
}

#[cfg(target_os = "macos")]
fn macos_foreground_process(shell_pid: u32) -> Option<ForegroundProcessInfo> {
    // libproc supplies the foreground group ID and its leader's name without spawning ps.
    let bsd = macos_proc_bsdinfo(shell_pid)?;
    let tpgid = bsd.e_tpgid;
    if tpgid == 0 {
        return None;
    }
    // Name of the foreground process-group leader (PID == PGID for the leader).
    let leader = macos_proc_bsdinfo(tpgid)?;
    let name = cstr_field_to_string(&leader.pbi_name)
        .or_else(|| cstr_field_to_string(&leader.pbi_comm))?;
    if name.is_empty() {
        return None;
    }
    Some(ForegroundProcessInfo { name, pid: tpgid })
}

/// Query `proc_bsdinfo` for `pid` via the libproc `proc_pidinfo` syscall.
#[cfg(target_os = "macos")]
fn macos_proc_bsdinfo(pid: u32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: proc_bsdinfo is a plain-old-data C struct; an all-zero bit pattern is
    // a valid initial value (every field is an integer/array). proc_pidinfo overwrites it.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: proc_pidinfo is the darwin libproc syscall, thread-safe (Apple docs).
    // PROC_PIDTBSDINFO fills a proc_bsdinfo of exactly `size` bytes. Same pattern as
    // cwd.rs::macos_proc_cwd. A full write returns `size`; anything else is a miss.
    let ret = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    (ret == size).then_some(info)
}

/// Read a NUL-terminated C-char field (e.g. `pbi_name`) into an owned `String`.
#[cfg(target_os = "macos")]
fn cstr_field_to_string(buf: &[libc::c_char]) -> Option<String> {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    if bytes.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

#[cfg(windows)]
fn windows_foreground_process(shell_pid: u32) -> Option<ForegroundProcessInfo> {
    capture_windows_snapshot()?.resolve(shell_pid)
}

/// Take a single `TH32CS_SNAPPROCESS` snapshot of every process.
/// Returns `None` if the snapshot could not be created.
// 이유: ToolHelp 핸들 생성과 Process32FirstW/NextW 순회를 한 unsafe 블록에서 처리한다.
// 다른 플랫폼의 unsafe 검사를 유지하기 위해 이 함수에만 허용한다.
#[cfg(windows)]
#[allow(clippy::multiple_unsafe_ops_per_block)]
fn capture_windows_snapshot() -> Option<tree::ProcessSnapshot> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: CloseHandle on a HANDLE obtained from
            // CreateToolhelp32Snapshot is the documented release path.
            // Drop에서 핸들 해제를 시도하고 실패는 trace로 남긴다.
            unsafe {
                if let Err(e) = CloseHandle(self.0) {
                    tracing::trace!("ToolHelp snapshot CloseHandle: {e}");
                }
            }
        }
    }

    // SAFETY: 본 블록은 ToolHelp snapshot 위에서 Process32FirstW/NextW를
    // 순차 호출하는 표준 패턴. 모든 호출은 단일 흐름으로 실행되며 snapshot
    // HANDLE은 HandleGuard로 Drop-시 close된다. PROCESSENTRY32W는 dwSize를
    // 명시한 뒤 zeroed로 초기화 — Win32 API 요구사항. 반환된 entry.szExeFile은
    // 함수 호출 후 entry가 살아있는 동안만 valid.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let _guard = HandleGuard(snapshot);

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..std::mem::zeroed()
        };

        let mut entries: Vec<(u32, u32, String)> = Vec::new();
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name_len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
                entries.push((entry.th32ProcessID, entry.th32ParentProcessID, name));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        Some(tree::ProcessSnapshot::from_entries(entries).excluding_host(std::process::id()))
    }
}

#[cfg(all(test, windows))]
mod windows_smoke {
    use super::*;

    // Manual smoke test — environment dependent. Run with:
    //   cargo test -p tasty-terminal --lib windows_smoke -- --nocapture --ignored
    #[test]
    #[ignore]
    fn current_process_resolves() {
        let pid = std::process::id();
        let info = get_foreground_process(pid);
        eprintln!(
            "foreground for self pid {pid}: {:?}",
            info.map(|i| (i.name, i.pid))
        );
    }
}
