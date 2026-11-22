//! Package counts from native package managers, Flatpak and Snap. A package manager
//! command runs only when its database directory exists.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::command;
use super::{home_dir, nix};
use crate::field::{Field, Needed};
use crate::info::join_counts;

/// A package manager that takes longer than this is treated as unavailable.
const COMMAND_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Default)]
pub(super) struct Summary {
    /// `"827 (dpkg)"`, or several managers joined with ", ".
    pub(super) summary: Option<String>,
    pub(super) flatpak: Option<usize>,
    pub(super) snap: Option<usize>,
}

pub(super) fn collect(needed: Needed) -> Summary {
    Summary {
        summary: needed
            .contains(Field::Packages)
            .then(native_summary)
            .flatten(),
        flatpak: needed
            .contains(Field::Flatpak)
            .then(flatpak_count)
            .flatten(),
        snap: needed.contains(Field::Snap).then(snap_count).flatten(),
    }
}

fn native_summary() -> Option<String> {
    let nix = std::thread::spawn(nix::count);

    let mut managers: Vec<(&str, Option<usize>)> = vec![
        ("pacman", count_dirs("/var/lib/pacman/local")),
        (
            "dpkg",
            fs::read_to_string("/var/lib/dpkg/status")
                .ok()
                .map(|text| count_dpkg(&text)),
        ),
        (
            "apk",
            fs::read_to_string("/lib/apk/db/installed")
                .ok()
                .map(|text| count_apk(&text)),
        ),
        (
            "xbps",
            exists("/var/db/xbps")
                .then(|| command_lines("xbps-query", &["-l"]))
                .flatten(),
        ),
        (
            "rpm",
            exists("/var/lib/rpm")
                .then(|| command_lines("rpm", &["-qa"]))
                .flatten(),
        ),
        ("portage", count_portage("/var/db/pkg")),
    ];
    if let Some(count) = nix.join().unwrap_or(None) {
        managers.push(("nix", Some(count)));
    }

    join_counts(
        &managers
            .into_iter()
            .filter_map(|(name, count)| count.map(|count| (name, count)))
            .collect::<Vec<_>>(),
    )
}

/// Count of `Status: install ok installed` stanzas in a dpkg status file.
fn count_dpkg(text: &str) -> usize {
    text.lines()
        .filter(|line| *line == "Status: install ok installed")
        .count()
}

/// Count of package records (lines starting with `P:`) in an apk installed database.
fn count_apk(text: &str) -> usize {
    text.lines().filter(|line| line.starts_with("P:")).count()
}

/// Non-empty lines of a command's standard output, when the command succeeds.
fn command_lines(program: &str, args: &[&str]) -> Option<usize> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    let output = command::output_with_timeout(cmd, COMMAND_TIMEOUT)?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().filter(|line| !line.trim().is_empty()).count())
}

fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Number of subdirectories, or `None` when the directory cannot be read.
fn count_dirs(path: impl AsRef<Path>) -> Option<usize> {
    let entries = fs::read_dir(path).ok()?;
    Some(
        entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .count(),
    )
}

/// Portage keeps packages at `<category>/<name>`.
fn count_portage(root: &str) -> Option<usize> {
    let categories = fs::read_dir(root).ok()?;
    Some(
        categories
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| count_dirs(entry.path()).unwrap_or(0))
            .sum(),
    )
}

/// Flatpak apps installed system-wide and for the current user. `None` when neither
/// installation directory exists.
fn flatpak_count() -> Option<usize> {
    let mut dirs: Vec<PathBuf> = vec![PathBuf::from("/var/lib/flatpak/app")];
    if let Some(home) = home_dir() {
        dirs.push(home.join(".local/share/flatpak/app"));
    }
    let counts: Vec<usize> = dirs.iter().filter_map(count_dirs).collect();
    (!counts.is_empty()).then(|| counts.iter().sum())
}

/// Entries of `/snap`, leaving out the `bin` link and the README.
fn snap_count() -> Option<usize> {
    let entries = fs::read_dir("/snap").ok()?;
    Some(
        entries
            .flatten()
            .filter(|entry| !matches!(entry.file_name().to_str(), Some("bin" | "README")))
            .count(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpkg_counts_installed_stanzas_only() {
        let text = "\
Package: bash
Status: install ok installed

Package: old
Status: deinstall ok config-files

Package: vim
Status: install ok installed
";
        assert_eq!(count_dpkg(text), 2);
    }

    #[test]
    fn apk_counts_package_records() {
        let text = "C:Q1abc=\nP:musl\nV:1.2\n\nP:busybox\nV:1.36\n";
        assert_eq!(count_apk(text), 2);
    }
}
