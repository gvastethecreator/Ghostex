//! Whether a hook was fired by an agent that another agent started from its tool.

/// CDXC:SessionIdentity 2026-10-05 WHY:
/// A `claude -p` (or `cswap run … claude -p`) an agent starts from its Bash tool inherits the terminal's GHOSTEX_GLOBAL_SESSION_REF, so its hooks posted into the session as if they were the session's own agent: the session took the nested run's conversation id (observed live 2026-10-05: throwaway thread G9c61 held its nested `claude -p`'s id for 1.2s, enough for a report, a title or the chat to follow the wrong transcript), and the nested run's prompt and Stop changed the session's activity mid-turn. Claude rewrites every CLAUDE_* variable for its own hooks, so the environment cannot tell the two apart; the process tree can. `CLAUDE_PID` names the Claude process that fired the hook, and a Claude process with another `claude` above it is a nested run, whose hooks belong to no Ghostex session. A parent link counts only when the parent is not newer than the child, so a reused Windows pid can never make the session's own agent look nested.
/// SEE-ALSO: server/src/agent_hooks/notify_runtime.rs (drops the event), server/src/zmx/process_identity.rs `resolve_process_tree_agent_identity` (the same rule for the live process scan).
pub(crate) fn claude_hook_is_nested() -> bool {
    let Some(pid) = std::env::var("CLAUDE_PID")
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 0)
    else {
        return false;
    };
    has_ancestor_named(pid, "claude")
}

/// Whether a process above `pid` (not `pid` itself) and below its session daemon runs an
/// executable whose name, without extension, is `name`. Unknown links end the walk, and so does
/// the zmx or wmx daemon: whatever started the daemon (an agent that launched Ghostex, say) is
/// not inside the session.
fn has_ancestor_named(pid: u32, name: &str) -> bool {
    let table = ProcessTable::read();
    let mut current = pid;
    for _ in 0..64 {
        let Some(parent) = table.parent(current) else {
            return false;
        };
        let Some(stem) = table.executable_stem(parent) else {
            return false;
        };
        if stem.eq_ignore_ascii_case("zmx") || stem.eq_ignore_ascii_case("wmx") {
            return false;
        }
        if stem.eq_ignore_ascii_case(name) {
            return true;
        }
        current = parent;
    }
    false
}

#[cfg(windows)]
struct ProcessTable {
    rows: std::collections::HashMap<u32, (u32, String)>,
}

#[cfg(windows)]
impl ProcessTable {
    fn read() -> Self {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        };
        let mut rows = std::collections::HashMap::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Self { rows };
            }
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut found = Process32FirstW(snapshot, &mut entry) != 0;
            while found {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..length]);
                rows.insert(entry.th32ProcessID, (entry.th32ParentProcessID, exe));
                found = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        Self { rows }
    }

    /// The parent, when it still exists and was created no later than `pid`: Windows keeps a dead
    /// parent's pid and hands it to new processes.
    fn parent(&self, pid: u32) -> Option<u32> {
        let (parent, _) = self.rows.get(&pid)?;
        let parent = *parent;
        if parent == 0 || parent == pid || !self.rows.contains_key(&parent) {
            return None;
        }
        (created(parent)? <= created(pid)?).then_some(parent)
    }

    fn executable_stem(&self, pid: u32) -> Option<String> {
        let (_, exe) = self.rows.get(&pid)?;
        Some(exe.strip_suffix(".exe").unwrap_or(exe).to_string())
    }
}

#[cfg(windows)]
fn created(pid: u32) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut times: [FILETIME; 4] = std::mem::zeroed();
        let [creation, exit, kernel, user] = &mut times;
        let ok = GetProcessTimes(process, creation, exit, kernel, user) != 0;
        CloseHandle(process);
        ok.then(|| ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
    }
}

/// POSIX reparents an orphan to init, so a parent pid is never stale; each link is read on demand.
#[cfg(not(windows))]
struct ProcessTable;

#[cfg(not(windows))]
impl ProcessTable {
    fn read() -> Self {
        Self
    }

    fn parent(&self, pid: u32) -> Option<u32> {
        posix::parent(pid).filter(|parent| *parent > 1 && *parent != pid)
    }

    fn executable_stem(&self, pid: u32) -> Option<String> {
        posix::name(pid)
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
mod posix {
    pub(super) fn parent(pid: u32) -> Option<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The command name sits in parentheses and may contain spaces; the fields after it are
        // state, then the parent pid.
        let after_name = &stat[stat.rfind(')')? + 1..];
        after_name.split_whitespace().nth(1)?.parse().ok()
    }

    pub(super) fn name(pid: u32) -> Option<String> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }
}

#[cfg(target_os = "macos")]
mod posix {
    fn info(pid: u32) -> Option<libc::proc_bsdinfo> {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let written = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        (written == size).then_some(info)
    }

    pub(super) fn parent(pid: u32) -> Option<u32> {
        Some(info(pid)?.pbi_ppid)
    }

    /// The short name, except that the native Claude install runs from
    /// `~/.local/share/claude/versions/<version>` through a `claude` symlink, so its short name
    /// can be the version; that executable path names it `claude`.
    pub(super) fn name(pid: u32) -> Option<String> {
        let info = info(pid)?;
        let bytes = info
            .pbi_comm
            .iter()
            .take_while(|byte| **byte != 0)
            .map(|byte| *byte as u8)
            .collect::<Vec<_>>();
        let comm = String::from_utf8_lossy(&bytes).into_owned();
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let length = unsafe {
            libc::proc_pidpath(
                pid as libc::c_int,
                path.as_mut_ptr().cast(),
                path.len() as u32,
            )
        };
        if length > 0
            && String::from_utf8_lossy(&path[..length as usize]).contains("/claude/versions/")
        {
            return Some("claude".to_string());
        }
        Some(comm)
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_os = "macos"
)))]
mod posix {
    pub(super) fn parent(_pid: u32) -> Option<u32> {
        None
    }

    pub(super) fn name(_pid: u32) -> Option<String> {
        None
    }
}
