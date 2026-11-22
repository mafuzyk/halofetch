//! One scan of `/proc` shared by the shell, terminal, desktop and window manager detectors.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;

/// Upper bound on the ancestor walk, guarding against cycles and odd process trees.
const MAX_DEPTH: usize = 64;

/// Interpreters whose command name hides the script that is actually running.
const INTERPRETERS: [&str; 8] = [
    "ruby", "python", "python3", "node", "perl", "deno", "bun", "lua",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Proc {
    pid: u32,
    ppid: u32,
    comm: String,
    title: Option<String>,
}

impl Proc {
    /// Name shown to the shell and terminal detectors. Interpreter-run shells
    /// expose their real name through the process title instead of `comm`.
    fn displayed_name(&self) -> &str {
        if INTERPRETERS.contains(&self.comm.as_str()) {
            self.title.as_deref().unwrap_or(&self.comm)
        } else {
            &self.comm
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct ProcTable {
    procs: Vec<Proc>,
    index: HashMap<u32, usize>,
}

impl ProcTable {
    /// Reads `/proc/<pid>/stat` for every process. Unreadable entries are skipped.
    pub(super) fn scan() -> Self {
        let Ok(entries) = fs::read_dir("/proc") else {
            return Self::default();
        };
        let mut procs = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !is_pid(&name) {
                continue;
            }
            let Some(pid) = name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            if let Some((comm, ppid)) = parse_stat(&stat) {
                let title = if INTERPRETERS.contains(&comm.as_str()) {
                    fs::read(entry.path().join("cmdline"))
                        .ok()
                        .and_then(|cmdline| process_title(&cmdline))
                } else {
                    None
                };
                procs.push(Proc {
                    pid,
                    ppid,
                    comm,
                    title,
                });
            }
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

    /// Command names of every process (truncated by the kernel to 15 bytes).
    pub(super) fn comms(&self) -> Vec<&str> {
        self.procs.iter().map(|proc| proc.comm.as_str()).collect()
    }

    /// Command names from `pid` upward through its parents, nearest first.
    pub(super) fn chain(&self, pid: u32) -> Vec<&str> {
        let mut names = Vec::new();
        let mut current = pid;
        for _ in 0..MAX_DEPTH {
            let Some(&position) = self.index.get(&current) else {
                break;
            };
            let proc = &self.procs[position];
            names.push(proc.displayed_name());
            if proc.ppid == 0 || proc.ppid == proc.pid {
                break;
            }
            current = proc.ppid;
        }
        names
    }
}

/// Number of processes, from the numeric entries under `/proc`.
pub(super) fn count() -> Option<usize> {
    let entries = fs::read_dir("/proc").ok()?;
    Some(
        entries
            .flatten()
            .filter(|entry| is_pid(&entry.file_name()))
            .count(),
    )
}

fn is_pid(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit()))
}

/// First argument of `/proc/<pid>/cmdline`, trimmed and without its directory.
///
/// `setproctitle` rewrites this field, which is how interpreter-based shells such as
/// sharky (a Ruby process titled `sharky`) identify themselves.
fn process_title(cmdline: &[u8]) -> Option<String> {
    let first = cmdline.split(|byte| *byte == 0).next()?;
    let text = String::from_utf8_lossy(first);
    let word = text.trim().rsplit('/').next()?.trim();
    (!word.is_empty()).then(|| word.to_string())
}

/// Extracts the command name and parent PID from a `/proc/<pid>/stat` line.
///
/// The command is wrapped in parentheses and may itself contain spaces or
/// parentheses, so the closing parenthesis is the last one in the line.
fn parse_stat(line: &str) -> Option<(String, u32)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    let comm = line.get(open + 1..close)?.to_string();
    let mut fields = line.get(close + 1..)?.split_whitespace();
    let _state = fields.next()?;
    let ppid = fields.next()?.parse::<u32>().ok()?;
    Some((comm, ppid))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, comm: &str) -> Proc {
        Proc {
            pid,
            ppid,
            comm: comm.to_string(),
            title: None,
        }
    }

    fn proc_with_title(pid: u32, ppid: u32, comm: &str, title: &str) -> Proc {
        Proc {
            pid,
            ppid,
            comm: comm.to_string(),
            title: Some(title.to_string()),
        }
    }

    #[test]
    fn parse_stat_reads_command_and_parent() {
        let line = "1234 (bash) S 1200 1234 1234 34816 1234 4194304 100 0 0 0";
        assert_eq!(parse_stat(line), Some(("bash".to_string(), 1200)));
    }

    #[test]
    fn parse_stat_handles_parentheses_in_the_command() {
        let line = "567 (Web Content (x)) S 100 567 567 0 -1 0";
        assert_eq!(parse_stat(line), Some(("Web Content (x)".to_string(), 100)));
    }

    #[test]
    fn parse_stat_rejects_truncated_lines() {
        assert_eq!(parse_stat("1234 (bash)"), None);
        assert_eq!(parse_stat("1234 bash S 1 2"), None);
        assert_eq!(parse_stat("1234 (bash) S"), None);
    }

    #[test]
    fn pid_names_are_numeric_only() {
        assert!(is_pid(OsStr::new("1")));
        assert!(is_pid(OsStr::new("4096")));
        assert!(!is_pid(OsStr::new("self")));
        assert!(!is_pid(OsStr::new("")));
        assert!(!is_pid(OsStr::new("1a")));
    }

    #[test]
    fn chain_walks_parents_and_stops_at_init() {
        let table = ProcTable::from_procs(vec![
            proc(1, 0, "systemd"),
            proc(100, 1, "kitty"),
            proc(200, 100, "fish"),
            proc(300, 200, "halofetch"),
        ]);
        assert_eq!(table.chain(200), ["fish", "kitty", "systemd"]);
        assert_eq!(table.chain(999), Vec::<&str>::new());
    }

    #[test]
    fn chain_survives_cycles() {
        let table = ProcTable::from_procs(vec![proc(5, 6, "a"), proc(6, 5, "b")]);
        let names = table.chain(5);
        assert_eq!(names.len(), MAX_DEPTH);
    }

    #[test]
    fn chain_uses_the_title_for_interpreter_shells() {
        let table = ProcTable::from_procs(vec![
            proc(1, 0, "systemd"),
            proc(100, 1, "kitty"),
            proc_with_title(200, 100, "ruby", "sharky"),
        ]);
        assert_eq!(table.chain(200), ["sharky", "kitty", "systemd"]);
    }

    #[test]
    fn chain_keeps_the_comm_for_native_processes() {
        let table = ProcTable::from_procs(vec![proc_with_title(
            100,
            1,
            "gnome-terminal-",
            "gnome-terminal",
        )]);
        assert_eq!(table.chain(100), ["gnome-terminal-"]);
    }

    #[test]
    fn comms_lists_every_process() {
        let table = ProcTable::from_procs(vec![proc(1, 0, "systemd"), proc(2, 1, "sway")]);
        assert_eq!(table.comms(), ["systemd", "sway"]);
    }

    #[test]
    fn process_title_reads_the_renamed_first_argument() {
        assert_eq!(process_title(b"sharky\0"), Some("sharky".to_string()));
        assert_eq!(
            process_title(b"sharky                 \0"),
            Some("sharky".to_string())
        );
        assert_eq!(
            process_title(b"/usr/bin/fish\0-i\0"),
            Some("fish".to_string())
        );
        assert_eq!(process_title(b"\0"), None);
    }
}
