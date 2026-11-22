//! Atomic file replacement used by the configuration and the updater.
//!
//! A temporary file is created with an unpredictable name in the destination
//! directory, written completely and renamed over the destination. Symbolic
//! links are refused so a write cannot be redirected to another file, and the
//! temporary file is created exclusively so an existing file is never clobbered.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Permission bits for a configuration file when there is no file to preserve.
const CONFIG_MODE: u32 = 0o600;
/// Permission bits for an executable when there is no file to preserve.
const EXECUTABLE_MODE: u32 = 0o755;
/// Enough attempts to get past a stale temporary file, few enough to fail fast.
const TEMP_ATTEMPTS: u32 = 100;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Replaces `path` with `contents`, preserving an existing regular file's mode.
/// A new file is created with mode `0600`.
pub(crate) fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_with_mode(path, contents, CONFIG_MODE)
}

/// Replaces `path` with an executable, preserving an existing regular file's mode.
/// A new file is created with mode `0755`.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn write_executable(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_with_mode(path, contents, EXECUTABLE_MODE)
}

fn write_with_mode(path: &Path, contents: &[u8], default_mode: u32) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "refusing to write through the symbolic link {}",
                        path.display()
                    ),
                ));
            }
            if !metadata.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} is not a regular file", path.display()),
                ));
            }
            Some(metadata)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no file name", path.display()),
        )
    })?;
    let mode = metadata
        .as_ref()
        .map(|metadata| mode_of(metadata.permissions()))
        .unwrap_or(default_mode);
    let (temporary, mut file) = create_temporary(parent, name)?;

    if let Err(error) = file.write_all(contents).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(file);

    let result = set_mode(&temporary, mode).and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_parent(parent);
    Ok(())
}

/// Creates a temporary file next to the destination. The name includes the
/// process id and a counter; `create_new` makes the operation safe even if a
/// name is predictable.
fn create_temporary(parent: &Path, name: &OsStr) -> io::Result<(PathBuf, File)> {
    let name = name.to_string_lossy();
    for _ in 0..TEMP_ATTEMPTS {
        let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.tmp-{}-{nonce}", std::process::id()));
        match open_new(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a temporary file next to the destination",
    ))
}

fn open_new(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(unix)]
fn mode_of(permissions: fs::Permissions) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    permissions.mode() & 0o777
}

#[cfg(not(unix))]
fn mode_of(_permissions: fs::Permissions) -> u32 {
    0
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// Best effort: makes the rename durable when the filesystem supports directory
/// sync. A failure here does not invalidate the completed replacement.
#[cfg(unix)]
fn sync_parent(parent: &Path) {
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("halofetch-atomic-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn replaces_content_and_leaves_no_temporary_file() {
        let dir = scratch("replace");
        let path = dir.join("config.json");
        fs::write(&path, "old").unwrap();

        write(&path, b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn preserves_an_existing_mode_and_creates_private_files() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("modes");
        let existing = dir.join("existing");
        fs::write(&existing, "old").unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o640)).unwrap();

        write(&existing, b"new").unwrap();
        let mode = fs::metadata(&existing).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);

        let fresh = dir.join("fresh");
        write(&fresh, b"new").unwrap();
        let mode = fs::metadata(&fresh).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_write_through_a_symbolic_link() {
        use std::os::unix::fs::symlink;

        let dir = scratch("symlink");
        let target = dir.join("target");
        let link = dir.join("link");
        fs::write(&target, "original").unwrap();
        symlink(&target, &link).unwrap();

        let error = write(&link, b"redirected").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_writers_leave_one_complete_value() {
        let dir = scratch("concurrent");
        let path = dir.join("config.json");
        std::thread::scope(|scope| {
            for index in 0..8 {
                let path = path.clone();
                scope.spawn(move || write(&path, format!("value-{index}").as_bytes()).unwrap());
            }
        });

        let value = fs::read_to_string(&path).unwrap();
        assert!(
            (0..8).any(|index| value == format!("value-{index}")),
            "{value}"
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_parent_is_an_error() {
        let dir = scratch("missing-parent");
        let path = dir.join("missing").join("config.json");
        assert!(write(&path, b"value").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_executable_gets_the_executable_mode() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = scratch("executable");
            let path = dir.join("binary");
            write_executable(&path, b"#!/bin/sh\n").unwrap();
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755);
            let _ = fs::remove_dir_all(&dir);
        }
        #[cfg(not(unix))]
        {
            let dir = scratch("executable");
            let path = dir.join("binary");
            write_executable(&path, b"binary").unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"binary");
            let _ = fs::remove_dir_all(&dir);
        }
    }
}
