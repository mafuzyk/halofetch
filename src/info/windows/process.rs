//! One process snapshot shared by the shell and terminal detectors, plus the mapping from
//! process and environment names to display names.

use std::collections::HashMap;
use std::mem::size_of;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use super::from_wide;
use crate::info::env_value;

/// Upper bound on the ancestor walk, guarding against cycles and odd process trees.
const MAX_DEPTH: usize = 64;

/// Environment variables that a terminal sets for the programs it starts.
const TERMINAL_VARIABLES: [(&str, &str); 5] = [
    ("WT_SESSION", "Windows Terminal"),
    ("ConEmuPID", "ConEmu"),
    ("ALACRITTY_WINDOW_ID", "Alacritty"),
    ("ALACRITTY_SOCKET", "Alacritty"),
    ("WEZTERM_EXECUTABLE", "WezTerm"),
];

/// `TERM_PROGRAM` values set by terminals that run on Windows.
const TERM_PROGRAMS: [(&str, &str); 3] = [
    ("vscode", "VS Code"),
    ("WezTerm", "WezTerm"),
    ("mintty", "mintty"),
];

/// Terminal host processes by lowercase executable name, nearest ancestor wins.
const TERMINAL_PROCESSES: [(&str, &str); 9] = [
    ("windowsterminal", "Windows Terminal"),
    ("wezterm-gui", "WezTerm"),
    ("alacritty", "Alacritty"),
    ("mintty", "mintty"),
    ("conemu64", "ConEmu"),
    ("conemu", "ConEmu"),
    ("code", "VS Code"),
    ("hyper", "Hyper"),
    ("tabby", "Tabby"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Proc {
    pid: u32,
    ppid: u32,
    /// Lowercase executable name without `.exe`.
    name: String,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ProcTable {
    procs: Vec<Proc>,
    index: HashMap<u32, usize>,
}

impl ProcTable {
    /// Snapshot of every process. Empty when the snapshot cannot be taken.
    pub(super) fn scan() -> Self {
        let Some(snapshot) = Snapshot::take() else {
            return Self::default();
        };
        // SAFETY: a zeroed PROCESSENTRY32W is valid; dwSize is set before first use.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut procs = Vec::new();
        // SAFETY: the snapshot handle is valid and `entry` has dwSize set.
        let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        while more {
            procs.push(Proc {
                pid: entry.th32ProcessID,
                ppid: entry.th32ParentProcessID,
                name: exe_name(&entry.szExeFile),
            });
            // SAFETY: as above; each call fills `entry` with the next process.
            more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }
        Self::from_procs(procs)
    }

    fn from_procs(procs: Vec<Proc>) -> Self {
        let index = procs
            .iter()
            .enumerate()
            .map(|(position, proc)| (proc.pid, position))
            .collect();
        Self { procs, index }
    }

    /// Number of processes, or `None` when the snapshot was empty.
    pub(super) fn count(&self) -> Option<usize> {
        (!self.procs.is_empty()).then_some(self.procs.len())
    }

    /// Parent of `pid`, when it is in the snapshot.
    pub(super) fn parent_of(&self, pid: u32) -> Option<u32> {
        self.index
            .get(&pid)
            .and_then(|&position| self.procs.get(position))
            .map(|proc| proc.ppid)
    }

    /// Executable names from `pid` upward through its parents, nearest first.
    pub(super) fn chain(&self, pid: u32) -> Vec<&str> {
        let mut names = Vec::new();
        let mut current = pid;
        for _ in 0..MAX_DEPTH {
            let Some(proc) = self
                .index
                .get(&current)
                .and_then(|&position| self.procs.get(position))
            else {
                break;
            };
            names.push(proc.name.as_str());
            if proc.ppid == 0 || proc.ppid == proc.pid {
                break;
            }
            current = proc.ppid;
        }
        names
    }
}

/// A toolhelp snapshot handle, closed when dropped.
struct Snapshot(HANDLE);

impl Snapshot {
    fn take() -> Option<Snapshot> {
        // SAFETY: TH32CS_SNAPPROCESS ignores the process id argument.
        let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        (handle != INVALID_HANDLE_VALUE).then(|| Snapshot(handle))
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful CreateToolhelp32Snapshot and is closed
        // only here.
        unsafe { CloseHandle(self.0) };
    }
}

/// Lowercase executable name without the `.exe` suffix.
fn exe_name(units: &[u16]) -> String {
    let name = from_wide(units).to_ascii_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    stem.to_string()
}

/// Shell that started this process: the nearest ancestor that is a known shell.
pub(super) fn shell(chain: &[&str]) -> Option<&'static str> {
    chain.iter().find_map(|name| shell_name(name))
}

/// Display name of a shell from its lowercase executable name.
fn shell_name(exe: &str) -> Option<&'static str> {
    Some(match exe {
        "pwsh" => "PowerShell",
        "powershell" => "Windows PowerShell",
        "cmd" => "Command Prompt",
        "nu" => "nu",
        "bash" => "bash",
        "zsh" => "zsh",
        "fish" => "fish",
        "elvish" => "elvish",
        "xonsh" => "xonsh",
        _ => return None,
    })
}

/// Terminal that hosts this process. Environment hints win, then the nearest terminal
/// process in the ancestry. A console host (`conhost`, `OpenConsole`) is named only when
/// no terminal process is found, since Windows Terminal runs under OpenConsole.
pub(super) fn terminal(chain: &[&str]) -> Option<&'static str> {
    terminal_from(env_value, chain)
}

fn terminal_from(env: impl Fn(&str) -> Option<String>, chain: &[&str]) -> Option<&'static str> {
    if let Some((_, name)) = TERMINAL_VARIABLES
        .iter()
        .find(|(variable, _)| env(variable).is_some())
    {
        return Some(*name);
    }
    if let Some(program) = env("TERM_PROGRAM") {
        if let Some((_, name)) = TERM_PROGRAMS.iter().find(|(value, _)| *value == program) {
            return Some(*name);
        }
    }
    if let Some(name) = chain.iter().find_map(|exe| terminal_process(exe)) {
        return Some(name);
    }
    chain
        .iter()
        .any(|exe| matches!(*exe, "conhost" | "openconsole"))
        .then_some("Windows Console")
}

fn terminal_process(exe: &str) -> Option<&'static str> {
    TERMINAL_PROCESSES
        .iter()
        .find(|(process, _)| *process == exe)
        .map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn proc(pid: u32, ppid: u32, name: &str) -> Proc {
        Proc {
            pid,
            ppid,
            name: name.to_string(),
        }
    }

    #[test]
    fn executable_names_are_lowercase_without_exe() {
        let mut units: Vec<u16> = "PwSh.EXE".encode_utf16().collect();
        units.resize(260, 0);
        assert_eq!(exe_name(&units), "pwsh");
        let mut units: Vec<u16> = "conhost".encode_utf16().collect();
        units.resize(260, 0);
        assert_eq!(exe_name(&units), "conhost");
    }

    #[test]
    fn shell_is_the_nearest_known_ancestor() {
        assert_eq!(shell(&["pwsh", "windowsterminal"]), Some("PowerShell"));
        assert_eq!(shell(&["explorer", "cmd"]), Some("Command Prompt"));
        assert_eq!(shell(&["explorer"]), None);
    }

    #[test]
    fn shell_names_cover_windows_shells() {
        assert_eq!(shell_name("powershell"), Some("Windows PowerShell"));
        assert_eq!(shell_name("bash"), Some("bash"));
        assert_eq!(shell_name("sh"), None);
    }

    #[test]
    fn terminal_environment_hints_come_first() {
        let env = env_from(&[("WT_SESSION", "1a2b")]);
        assert_eq!(
            terminal_from(env, &["pwsh", "openconsole"]),
            Some("Windows Terminal")
        );
        let env = env_from(&[("TERM_PROGRAM", "vscode")]);
        assert_eq!(terminal_from(env, &["pwsh"]), Some("VS Code"));
        let env = env_from(&[("ConEmuPID", "4242")]);
        assert_eq!(terminal_from(env, &["cmd"]), Some("ConEmu"));
        let env = env_from(&[("ALACRITTY_WINDOW_ID", "7")]);
        assert_eq!(terminal_from(env, &["pwsh"]), Some("Alacritty"));
    }

    #[test]
    fn terminal_walks_ancestors_before_console_hosts() {
        let env = env_from(&[]);
        assert_eq!(
            terminal_from(env, &["cmd", "openconsole", "windowsterminal"]),
            Some("Windows Terminal")
        );
        let env = env_from(&[]);
        assert_eq!(
            terminal_from(env, &["bash", "mintty", "explorer"]),
            Some("mintty")
        );
    }

    #[test]
    fn console_host_is_the_last_resort() {
        let env = env_from(&[]);
        assert_eq!(
            terminal_from(env, &["cmd", "conhost", "explorer"]),
            Some("Windows Console")
        );
        let env = env_from(&[]);
        assert_eq!(terminal_from(env, &["cmd", "explorer"]), None);
    }

    #[test]
    fn chain_walks_parents_and_stops_at_the_root() {
        let table = ProcTable::from_procs(vec![
            proc(4, 0, "system"),
            proc(100, 4, "explorer"),
            proc(200, 100, "pwsh"),
            proc(300, 200, "halofetch"),
        ]);
        assert_eq!(table.chain(200), ["pwsh", "explorer", "system"]);
        assert_eq!(table.parent_of(300), Some(200));
        assert_eq!(table.parent_of(999), None);
        assert_eq!(table.chain(999), Vec::<&str>::new());
    }

    #[test]
    fn chain_survives_cycles() {
        let table = ProcTable::from_procs(vec![proc(5, 6, "a"), proc(6, 5, "b")]);
        assert_eq!(table.chain(5).len(), MAX_DEPTH);
    }

    #[test]
    fn count_is_absent_for_an_empty_table() {
        assert_eq!(ProcTable::default().count(), None);
        assert_eq!(
            ProcTable::from_procs(vec![proc(1, 0, "x")]).count(),
            Some(1)
        );
    }

    #[test]
    fn snapshot_does_not_panic() {
        let table = ProcTable::scan();
        let _ = table.chain(std::process::id());
    }
}
