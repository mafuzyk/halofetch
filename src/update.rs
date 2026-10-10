//! Self-update: rebuild from a source checkout, or install the latest release binary.

use color_eyre::{
    eyre::{bail, eyre},
    Result,
};
use directories::BaseDirs;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// File name of the release binary cargo writes, and of the installed command.
const EXECUTABLE: &str = if cfg!(windows) {
    "halofetch.exe"
} else {
    "halofetch"
};

const REPOSITORY: &str = "mafuzyk/halofetch";

pub fn run(release: bool) -> Result<()> {
    if !release {
        if let Some(source) = find_source_dir()? {
            return update_from_source(&source);
        }
    }
    update_from_release()
}

fn update_from_source(source: &Path) -> Result<()> {
    println!("halofetch update — source: {}", source.display());
    ensure_clean_checkout(source)?;

    run_command(
        Command::new("git")
            .args(["pull", "--rebase", "--autostash"])
            .current_dir(source),
        "git pull",
    )?;
    run_command(
        Command::new("cargo")
            .args(["build", "--release", "--locked"])
            .current_dir(source),
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
        bail!("could not inspect the HaloFetch checkout before updating");
    }
    if !output.stdout.is_empty() {
        bail!(
            "the HaloFetch checkout has local changes; commit or stash them before running --update"
        );
    }
    Ok(())
}

/// The checkout to update from, or `None` when no candidate is one. An invalid
/// `HALOFETCH_SRC` is an error instead of a silent fallback.
fn find_source_dir() -> Result<Option<PathBuf>> {
    let source = std::env::var_os("HALOFETCH_SRC").or_else(|| std::env::var_os("ATLASFETCH_SRC"));
    if let Some(path) = source.map(PathBuf::from) {
        return validate_source(path).map(Some);
    }

    if let Ok(cwd) = std::env::current_dir() {
        if is_source_dir(&cwd) {
            return Ok(Some(cwd));
        }
    }

    if let Ok(executable) = std::env::current_exe() {
        for ancestor in executable.ancestors().skip(1).take(5) {
            if is_source_dir(ancestor) {
                return Ok(Some(ancestor.to_path_buf()));
            }
        }
    }

    if let Some(home) = BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()) {
        for relative in [
            "Projetos/halofetch",
            "src/halofetch",
            "halofetch",
            "code/halofetch",
            "dev/halofetch",
            "Projetos/atlasfetch",
            "src/atlasfetch",
            "atlasfetch",
            "code/atlasfetch",
            "dev/atlasfetch",
        ] {
            let candidate = home.join(relative);
            if is_source_dir(&candidate) {
                return Ok(Some(candidate));
            }
        }
    }

    Ok(None)
}

fn validate_source(path: PathBuf) -> Result<PathBuf> {
    if is_source_dir(&path) {
        Ok(path)
    } else {
        Err(eyre!(
            "HALOFETCH_SRC is not a HaloFetch source checkout: {}",
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
    Ok(home.join(".local/bin/halofetch"))
}

#[cfg(windows)]
fn install_path() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new()
        .ok_or_else(|| eyre!("cannot determine the local application data directory"))?;
    Ok(dirs
        .data_local_dir()
        .join("Programs")
        .join("halofetch")
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
        bail!("failed to install HaloFetch to {}", destination.display());
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
                "Add {} to your PATH to run halofetch from any terminal.",
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

/// Installs the latest GitHub release over the running executable.
fn update_from_release() -> Result<()> {
    let Some(target) = release_target() else {
        bail!(
            "no prebuilt binary is published for this platform; build from source (see the README)"
        );
    };

    println!("→ Checking the latest release...");
    let release = parse_release(&latest_release_json()?, target)?;
    if is_current(&release.tag) {
        println!(
            "halofetch {} is already the latest release.",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }

    let executable = std::env::current_exe()?;
    let destination = executable.canonicalize().unwrap_or(executable);
    if is_nix_store(&destination) {
        bail!("this halofetch is managed by Nix; update it with nix instead");
    }

    let archive = archive_name(&release.tag, target);
    let work = WorkDir::create(
        std::env::temp_dir().join(format!("halofetch-update-{}", std::process::id())),
    )?;
    let archive_path = work.path.join(&archive);
    let checksum_path = work.path.join(format!("{archive}.sha256"));

    println!("→ Downloading {}...", release.tag);
    run_command(
        Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(&archive_path)
            .arg(&release.archive_url),
        &format!("download {archive}"),
    )?;
    run_command(
        Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(&checksum_path)
            .arg(&release.checksum_url),
        "download checksum",
    )?;

    let expected = expected_checksum(&std::fs::read_to_string(&checksum_path).unwrap_or_default())
        .ok_or_else(|| eyre!("the checksum file for {archive} is not valid"))?;
    let actual = hex(&Sha256::digest(std::fs::read(&archive_path)?));
    if actual != expected {
        bail!("checksum mismatch for {archive}; nothing was installed");
    }

    let tar_flags = if cfg!(windows) { "-xf" } else { "-xzf" };
    run_command(
        Command::new("tar")
            .args([tar_flags])
            .arg(&archive_path)
            .arg("-C")
            .arg(&work.path),
        &format!("extract {archive}"),
    )?;
    let binary = work.path.join(EXECUTABLE);
    if !binary.is_file() {
        bail!("{archive} does not contain {EXECUTABLE}");
    }

    #[cfg(windows)]
    install_binary(&binary, &destination)?;
    #[cfg(not(windows))]
    replace_binary(&binary, &destination)?;

    println!("→ Installed {} to {}", release.tag, destination.display());
    Ok(())
}

/// The release target triple for this platform, when a binary is published for it.
fn release_target() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("x86_64-unknown-linux-musl")
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Some("x86_64-pc-windows-msvc")
    } else {
        None
    }
}

fn archive_name(tag: &str, target: &str) -> String {
    let extension = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("halofetch-{tag}-{target}.{extension}")
}

fn latest_release_json() -> Result<String> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: halofetch-updater",
        ])
        .arg(url)
        .output()
        .map_err(|_| {
            eyre!("`curl` is needed to download the release; install it or build from source")
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "could not read the latest release from GitHub ({}): {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, PartialEq)]
struct Release {
    tag: String,
    archive_url: String,
    checksum_url: String,
}

fn parse_release(json: &str, target: &str) -> Result<Release> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| eyre!("the latest release is not valid JSON: {error}"))?;
    let tag = value["tag_name"]
        .as_str()
        .ok_or_else(|| eyre!("the latest release has no tag"))?
        .to_owned();

    let archive = archive_name(&tag, target);
    let checksum = format!("{archive}.sha256");
    let assets = value["assets"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let download_prefix = format!("https://github.com/{REPOSITORY}/releases/download/");
    let download_url = |name: &str| -> Result<String> {
        let asset = assets
            .iter()
            .find(|asset| asset["name"].as_str() == Some(name))
            .ok_or_else(|| eyre!("the latest release has no asset named {name}"))?;
        asset["browser_download_url"]
            .as_str()
            .filter(|url| url.starts_with(&download_prefix))
            .map(str::to_owned)
            .ok_or_else(|| eyre!("asset {name} has no download URL from {REPOSITORY}"))
    };

    Ok(Release {
        archive_url: download_url(&archive)?,
        checksum_url: download_url(&checksum)?,
        tag,
    })
}

fn is_current(tag: &str) -> bool {
    tag.trim_start_matches('v') == env!("CARGO_PKG_VERSION")
}

/// A binary under the Nix store is replaced by a Nix rebuild, not by this command.
fn is_nix_store(path: &Path) -> bool {
    path.starts_with("/nix/store")
}

/// The download directory, removed when the update ends whether or not it succeeded.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn create(path: PathBuf) -> Result<Self> {
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The digest from the first token of a `sha256sum`-style line, lowercased. `None`
/// unless that token is 64 hexadecimal digits.
fn expected_checksum(text: &str) -> Option<String> {
    let digest = text.split_whitespace().next()?.to_ascii_lowercase();
    (digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(digest)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The new binary is copied beside the installed one and renamed over it, so the
/// installed file is never partly written.
#[cfg(not(windows))]
fn replace_binary(binary: &Path, destination: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let staged = destination.with_file_name(".halofetch.new");
    let replaced = std::fs::copy(binary, &staged)
        .and_then(|_| std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)))
        .and_then(|()| std::fs::rename(&staged, destination));
    if let Err(error) = replaced {
        let _ = std::fs::remove_file(&staged);
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            let dir = destination.parent().unwrap_or(destination);
            bail!(
                "cannot write {}; run the update with permission to change that folder, or reinstall there yourself",
                dir.display()
            );
        }
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        archive_name, expected_checksum, hex, is_current, is_nix_store, is_source_dir, on_path,
        parse_release, Release,
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::path::Path;

    const LINUX: &str = "x86_64-unknown-linux-musl";
    const WINDOWS: &str = "x86_64-pc-windows-msvc";
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn checkout_layout_is_detected_as_source() {
        // Built in a scratch directory: a release tarball or a Nix build has no `.git`.
        let root = std::env::temp_dir().join(format!("halofetch-source-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();
        std::fs::write(root.join("src/main.rs"), "").unwrap();
        assert!(!is_source_dir(&root));

        std::fs::create_dir_all(root.join(".git")).unwrap();
        assert!(is_source_dir(&root));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn path_lookup_ignores_case_and_trailing_separator() {
        let path =
            std::env::join_paths([r"\Windows", r"\Users\Me\AppData\Local\Programs\HaloFetch\"])
                .unwrap();
        assert!(on_path(
            Path::new(r"\users\me\appdata\local\programs\halofetch"),
            &path
        ));
        assert!(!on_path(Path::new(r"\Users\Me\bin"), &path));
    }

    #[test]
    fn archive_names_follow_the_published_assets() {
        assert_eq!(
            archive_name("v3.1.0", LINUX),
            "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            archive_name("v3.1.0", WINDOWS),
            "halofetch-v3.1.0-x86_64-pc-windows-msvc.zip"
        );
    }

    /// A release JSON document with one asset per name, as the GitHub API lists them.
    fn release_json(tag: Option<&str>, names: &[&str]) -> String {
        let assets: Vec<_> = names
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "browser_download_url": format!("https://github.com/mafuzyk/halofetch/releases/download/v3.1.0/{name}"),
                })
            })
            .collect();
        json!({ "tag_name": tag, "assets": assets }).to_string()
    }

    #[test]
    fn parse_release_finds_the_archive_for_each_target() {
        let json = release_json(
            Some("v3.1.0"),
            &[
                "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz",
                "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz.sha256",
                "halofetch-v3.1.0-x86_64-pc-windows-msvc.zip",
                "halofetch-v3.1.0-x86_64-pc-windows-msvc.zip.sha256",
                "notes.txt",
            ],
        );

        assert_eq!(
            parse_release(&json, LINUX).unwrap(),
            Release {
                tag: "v3.1.0".to_string(),
                archive_url:
                    "https://github.com/mafuzyk/halofetch/releases/download/v3.1.0/halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz"
                        .to_string(),
                checksum_url:
                    "https://github.com/mafuzyk/halofetch/releases/download/v3.1.0/halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz.sha256"
                        .to_string(),
            }
        );
        assert_eq!(
            parse_release(&json, WINDOWS).unwrap(),
            Release {
                tag: "v3.1.0".to_string(),
                archive_url: "https://github.com/mafuzyk/halofetch/releases/download/v3.1.0/halofetch-v3.1.0-x86_64-pc-windows-msvc.zip"
                    .to_string(),
                checksum_url:
                    "https://github.com/mafuzyk/halofetch/releases/download/v3.1.0/halofetch-v3.1.0-x86_64-pc-windows-msvc.zip.sha256"
                        .to_string(),
            }
        );
    }

    #[test]
    fn parse_release_names_a_missing_checksum() {
        let json = release_json(
            Some("v3.1.0"),
            &["halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz"],
        );
        let error = parse_release(&json, LINUX).unwrap_err().to_string();
        assert!(error.contains("halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz.sha256"));
    }

    #[test]
    fn parse_release_rejects_downloads_outside_the_repository() {
        let json = json!({
            "tag_name": "v3.1.0",
            "assets": [
                {
                    "name": "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz",
                    "browser_download_url": "https://example.test/halofetch.tar.gz",
                },
                {
                    "name": "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz.sha256",
                    "browser_download_url": "-o/etc/passwd",
                },
            ],
        })
        .to_string();
        assert!(parse_release(&json, LINUX).is_err());
    }

    #[test]
    fn parse_release_requires_a_tag() {
        let json = release_json(
            None,
            &[
                "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz",
                "halofetch-v3.1.0-x86_64-unknown-linux-musl.tar.gz.sha256",
            ],
        );
        assert!(parse_release(&json, LINUX).is_err());
    }

    #[test]
    fn expected_checksum_reads_the_first_token_in_lowercase() {
        assert_eq!(
            expected_checksum(&format!("{ABC_SHA256}  halofetch.tar.gz\n")),
            Some(ABC_SHA256.to_string())
        );
        assert_eq!(
            expected_checksum(&ABC_SHA256.to_uppercase()),
            Some(ABC_SHA256.to_string())
        );
        assert_eq!(expected_checksum(&ABC_SHA256[..63]), None);
        assert_eq!(expected_checksum(&format!("z{}", &ABC_SHA256[1..])), None);
        assert_eq!(expected_checksum(""), None);
    }

    #[test]
    fn hex_is_lowercase_and_zero_padded() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }

    #[test]
    fn sha256_of_abc_matches_the_known_digest() {
        assert_eq!(hex(&Sha256::digest(b"abc")), ABC_SHA256);
    }

    #[test]
    fn only_the_installed_version_counts_as_current() {
        assert!(is_current(&format!("v{}", env!("CARGO_PKG_VERSION"))));
        assert!(!is_current("v0.0.1"));
    }

    #[test]
    fn nix_store_paths_are_recognized() {
        assert!(is_nix_store(Path::new(
            "/nix/store/abc-halofetch/bin/halofetch"
        )));
        assert!(!is_nix_store(Path::new("/home/me/.local/bin/halofetch")));
    }
}
