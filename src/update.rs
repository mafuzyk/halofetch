//! Source-based self-update support.

use color_eyre::{
    eyre::{bail, eyre},
    Result,
};
use directories::BaseDirs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// File name of the release binary cargo writes, and of the installed command.
const EXECUTABLE: &str = if cfg!(windows) {
    "atlasfetch.exe"
} else {
    "atlasfetch"
};

pub fn run() -> Result<()> {
    let source = detect_source_dir()?;
    println!("atlasfetch update — source: {}", source.display());
    ensure_clean_checkout(&source)?;

    run_command(
        Command::new("git")
            .args(["pull", "--rebase", "--autostash"])
            .current_dir(&source),
        "git pull",
    )?;
    run_command(
        Command::new("cargo")
            .args(["build", "--release", "--locked"])
            .current_dir(&source),
        "cargo build",
    )?;

    let binary = source.join("target").join("release").join(EXECUTABLE);
    let destination = install_path()?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("→ Installing to {}...", destination.display());
    install_binary(&binary, &destination)?;
    println!("Updated to latest version!");
    Ok(())
}

fn ensure_clean_checkout(source: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(source)
        .output()?;
    if !output.status.success() {
        bail!("could not inspect the AtlasFetch checkout before updating");
    }
    if !output.stdout.is_empty() {
        bail!(
            "the AtlasFetch checkout has local changes; commit or stash them before running --update"
        );
    }
    Ok(())
}

fn detect_source_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("ATLASFETCH_SRC").map(PathBuf::from) {
        return validate_source(path);
    }

    if let Ok(cwd) = std::env::current_dir() {
        if is_source_dir(&cwd) {
            return Ok(cwd);
        }
    }

    if let Ok(executable) = std::env::current_exe() {
        for ancestor in executable.ancestors().skip(1).take(5) {
            if is_source_dir(ancestor) {
                return Ok(ancestor.to_path_buf());
            }
        }
    }

    if let Some(home) = BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        for relative in [
            "Projetos/atlasfetch",
            "src/atlasfetch",
            "atlasfetch",
            "code/atlasfetch",
            "dev/atlasfetch",
        ] {
            let candidate = home.join(relative);
            if is_source_dir(&candidate) {
                return Ok(candidate);
            }
        }
    }

    Err(eyre!(
        "could not find the AtlasFetch source checkout; set ATLASFETCH_SRC to its path"
    ))
}

fn validate_source(path: PathBuf) -> Result<PathBuf> {
    if is_source_dir(&path) {
        Ok(path)
    } else {
        Err(eyre!(
            "ATLASFETCH_SRC is not an AtlasFetch source checkout: {}",
            path.display()
        ))
    }
}

fn is_source_dir(path: &Path) -> bool {
    path.join(".git").exists()
        && path.join("Cargo.toml").exists()
        && path.join("src/main.rs").exists()
}

#[cfg(not(windows))]
fn install_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Ok(home.join(".local/bin/atlasfetch"))
}

#[cfg(windows)]
fn install_path() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new()
        .ok_or_else(|| eyre!("cannot determine the local application data directory"))?;
    Ok(dirs
        .data_local_dir()
        .join("Programs")
        .join("atlasfetch")
        .join(EXECUTABLE))
}

fn run_command(command: &mut Command, label: &str) -> Result<()> {
    println!("→ Running {label}...");
    let status = command
        .status()
        .map_err(|error| color_eyre::eyre::eyre!("failed to run {label}: {error}"))?;
    if !status.success() {
        bail!("{label} failed with {status}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_binary(binary: &Path, destination: &Path) -> Result<()> {
    if !binary.is_file() {
        bail!("release build did not produce {}", binary.display());
    }

    let status = Command::new("install")
        .args(["-m", "755"])
        .arg(binary)
        .arg(destination)
        .status()?;
    if !status.success() {
        bail!("failed to install AtlasFetch to {}", destination.display());
    }
    Ok(())
}

/// A running executable cannot be overwritten on Windows but can be renamed, so the
/// installed copy is moved aside first. The previous copy is removed when it is no
/// longer in use.
#[cfg(windows)]
fn install_binary(binary: &Path, destination: &Path) -> Result<()> {
    if !binary.is_file() {
        bail!("release build did not produce {}", binary.display());
    }

    let previous = destination.with_extension("exe.old");
    let _ = std::fs::remove_file(&previous);
    if destination.exists() {
        std::fs::rename(destination, &previous)?;
    }
    std::fs::copy(binary, destination)?;

    if let Some(dir) = destination.parent() {
        if !on_path(dir, &std::env::var_os("PATH").unwrap_or_default()) {
            println!(
                "Add {} to your PATH to run atlasfetch from any terminal.",
                dir.display()
            );
        }
    }
    Ok(())
}

/// Whether `dir` is one of the entries of `path_var`. Windows paths compare without
/// regard to case and to a trailing separator.
#[cfg(any(windows, test))]
fn on_path(dir: &Path, path_var: &std::ffi::OsStr) -> bool {
    let wanted = normalized_dir(dir);
    std::env::split_paths(path_var).any(|entry| normalized_dir(&entry) == wanted)
}

#[cfg(any(windows, test))]
fn normalized_dir(dir: &Path) -> String {
    dir.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{is_source_dir, on_path};
    use std::path::Path;

    #[test]
    fn repository_root_is_detected_as_source() {
        assert!(is_source_dir(std::path::Path::new(env!(
            "CARGO_MANIFEST_DIR"
        ))));
    }

    #[test]
    fn path_lookup_ignores_case_and_trailing_separator() {
        let path =
            std::env::join_paths([r"\Windows", r"\Users\Me\AppData\Local\Programs\AtlasFetch\"])
                .unwrap();
        assert!(on_path(
            Path::new(r"\users\me\appdata\local\programs\atlasfetch"),
            &path
        ));
        assert!(!on_path(Path::new(r"\Users\Me\bin"), &path));
    }
}
