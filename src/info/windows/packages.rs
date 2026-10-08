//! Package counts from Scoop and Chocolatey, the two managers that keep one directory per
//! package. Winget and the Microsoft Store are not counted.

use std::fs;
use std::path::{Path, PathBuf};

use crate::info::{env_value, join_counts};

/// `"34 (scoop), 12 (choco)"`. Managers with no packages are left out.
pub(super) fn collect() -> Option<String> {
    let managers = [("scoop", scoop_count()), ("choco", chocolatey_count())];
    let counts: Vec<(&str, usize)> = managers
        .into_iter()
        .filter_map(|(name, count)| Some((name, count?)))
        .collect();
    join_counts(&counts)
}

/// Scoop apps, excluding Scoop itself: `%SCOOP%\apps`, else `%USERPROFILE%\scoop\apps`.
fn scoop_count() -> Option<usize> {
    let apps = match env_value("SCOOP") {
        Some(root) => PathBuf::from(root).join("apps"),
        None => PathBuf::from(env_value("USERPROFILE")?)
            .join("scoop")
            .join("apps"),
    };
    count_dirs(&apps, |name| name != "scoop")
}

/// Chocolatey packages: `%ChocolateyInstall%\lib`, else `C:\ProgramData\chocolatey\lib`.
fn chocolatey_count() -> Option<usize> {
    let install = env_value("ChocolateyInstall")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData\chocolatey"));
    count_dirs(&install.join("lib"), |_| true)
}

/// Subdirectories whose names pass `keep`. `None` when the directory cannot be read.
fn count_dirs(path: &Path, keep: impl Fn(&str) -> bool) -> Option<usize> {
    let entries = fs::read_dir(path).ok()?;
    Some(
        entries
            .flatten()
            .filter(|entry| entry.path().is_dir() && keep(&entry.file_name().to_string_lossy()))
            .count(),
    )
}
