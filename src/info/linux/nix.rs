//! Nix package counts from profiles. The closure of a profile is not a package
//! list: it contains every dependency and every split output. This module counts
//! the package entries visible in the profiles instead.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use super::command;
use super::home_dir;

/// Nix queries are local but the daemon can be busy. A late answer is dropped
/// instead of delaying the fetch indefinitely.
const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);

/// Number of distinct Nix packages in the system and user profiles. `None` when
/// no Nix profile is present or a profile cannot be read completely, so a failed
/// query is not reported as a smaller package count.
pub(super) fn count() -> Option<usize> {
    let home = home_dir()?;
    let mut roots = Vec::new();
    let mut manifest_keys = HashSet::new();

    if Path::new("/run/current-system/sw").is_dir() {
        roots.push(PathBuf::from("/run/current-system/sw"));
    }

    let state_home = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    let user_profiles = [
        home.join(".nix-profile"),
        state_home.join("nix/profiles/profile"),
    ];
    let mut seen_profiles = HashSet::new();
    for profile in user_profiles {
        let manifest = profile.join("manifest.json");
        if manifest.is_file() {
            let canonical = fs::canonicalize(&manifest).unwrap_or_else(|_| manifest.clone());
            if seen_profiles.insert(canonical) {
                manifest_keys.extend(manifest_package_keys(&manifest)?);
            }
        } else if profile.is_dir() {
            roots.push(profile);
        }
    }

    let per_user = Path::new("/etc/profiles/per-user").join(
        std::env::var_os("USER")
            .or_else(|| std::env::var_os("LOGNAME"))
            .unwrap_or_default(),
    );
    if per_user.is_dir() {
        let manifest = per_user.join("manifest.json");
        if manifest.is_file() {
            manifest_keys.extend(manifest_package_keys(&manifest)?);
        } else if let Some(root) = home_manager_root(&per_user) {
            roots.push(root);
        } else {
            roots.push(per_user);
        }
    }

    // `~/.nix-profile` and the state profile are often symlinks to the same
    // environment. Deduplicate the real paths, not the link names.
    roots = roots
        .into_iter()
        .map(|root| root.canonicalize().unwrap_or(root))
        .collect();
    roots.sort();
    roots.dedup();
    if roots.is_empty() && manifest_keys.is_empty() {
        return None;
    }

    let mut keys = manifest_keys;
    if !roots.is_empty() {
        let text = references(&roots)?;
        for line in text.lines() {
            if let Some(key) = package_key(line) {
                keys.insert(key);
            }
        }
    }
    Some(keys.len())
}

/// `nix-store -q --references` for every profile in one daemon round trip. The
/// outputs share a format, so their lines can be unioned directly. `None` means
/// the query failed.
fn references(roots: &[PathBuf]) -> Option<String> {
    let mut cmd = Command::new("nix-store");
    cmd.args(["-q", "--references"]).args(roots);
    let output = command::output_with_timeout(cmd, QUERY_TIMEOUT)?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Store path of the Home Manager environment behind a per-user profile. The
/// profile is a buildEnv whose `bin` symlinks point at that environment.
fn home_manager_root(profile: &Path) -> Option<PathBuf> {
    let bin = profile.join("bin");
    for entry in fs::read_dir(bin).ok()?.flatten() {
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        let text = target.to_string_lossy();
        if let Some(index) = text.find("-home-manager-path/bin/") {
            return Some(PathBuf::from(&text[..index + "-home-manager-path".len()]));
        }
    }
    None
}

/// Package keys from a `nix profile` manifest. Elements with `active: false` are
/// skipped. `storePaths` is preferred so the same package is deduplicated across
/// profiles by its store path.
fn manifest_package_keys(path: &Path) -> Option<HashSet<String>> {
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let elements = value.get("elements")?.as_object()?;
    let mut keys = HashSet::new();
    for (name, element) in elements {
        if element.get("active").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let mut found = false;
        if let Some(paths) = element.get("storePaths").and_then(Value::as_array) {
            for path in paths.iter().filter_map(Value::as_str) {
                if let Some(key) = package_key(path) {
                    keys.insert(key);
                    found = true;
                }
            }
        }
        if !found {
            keys.insert(name.clone());
        }
    }
    Some(keys)
}

/// Stable key for one store path: `<name>-<version>`, without split outputs.
fn package_key(path: &str) -> Option<String> {
    let rest = path.trim().strip_prefix("/nix/store/")?;
    if rest.len() < 34 || rest.as_bytes().get(32) != Some(&b'-') {
        return None;
    }
    if !rest[..32]
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
    {
        return None;
    }
    let mut name = &rest[33..];
    if name.ends_with(".drv") || name.starts_with("hm-session-vars") {
        return None;
    }
    for suffix in [
        "-man",
        "-doc",
        "-info",
        "-dev",
        "-terminfo",
        "-lib",
        "-bin",
        "-out",
        "-source",
        "-debug",
    ] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            name = stripped;
            break;
        }
    }
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_outputs_collapse_to_one_package() {
        assert_eq!(
            package_key("/nix/store/00000000000000000000000000000000-foo-1.2.3"),
            Some("foo-1.2.3".to_string())
        );
        for suffix in ["man", "doc", "info", "dev", "terminfo", "lib", "bin", "out"] {
            assert_eq!(
                package_key(&format!(
                    "/nix/store/00000000000000000000000000000000-foo-1.2.3-{suffix}"
                )),
                Some("foo-1.2.3".to_string())
            );
        }
    }

    #[test]
    fn non_packages_are_ignored() {
        assert_eq!(package_key("/nix/store/not-a-store-path"), None);
        assert_eq!(
            package_key("/nix/store/00000000000000000000000000000000-hm-session-vars.sh"),
            None
        );
        assert_eq!(
            package_key("/nix/store/00000000000000000000000000000000-foo-1.0.drv"),
            None
        );
        assert_eq!(package_key("/usr/bin/ls"), None);
    }

    #[test]
    fn distinct_packages_survive() {
        let paths = [
            "/nix/store/00000000000000000000000000000000-foo-1.0",
            "/nix/store/00000000000000000000000000000000-foo-1.0-man",
            "/nix/store/11111111111111111111111111111111-bar-2.0",
        ];
        let keys: HashSet<String> = paths.iter().filter_map(|path| package_key(path)).collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains("foo-1.0"));
        assert!(keys.contains("bar-2.0"));
    }

    #[test]
    fn manifest_reads_active_elements_and_store_paths() {
        let dir = std::env::temp_dir().join(format!("halofetch-nix-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let manifest = dir.join("manifest.json");
        fs::write(
            &manifest,
            r#"{
                "elements": {
                    "active": {
                        "active": true,
                        "storePaths": ["/nix/store/00000000000000000000000000000000-foo-1.0"]
                    },
                    "inactive": {
                        "active": false,
                        "storePaths": ["/nix/store/11111111111111111111111111111111-bar-2.0"]
                    },
                    "plain": {"active": true}
                }
            }"#,
        )
        .unwrap();

        let keys = manifest_package_keys(&manifest).unwrap();
        assert!(keys.contains("foo-1.0"));
        assert!(keys.contains("plain"));
        assert_eq!(keys.len(), 2);
        let _ = fs::remove_file(&manifest);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn home_manager_root_follows_a_split_bin() {
        use std::os::unix::fs::symlink;

        let dir = std::env::temp_dir().join(format!("halofetch-nix-{}-home", std::process::id()));
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let store = "/nix/store/00000000000000000000000000000000-home-manager-path";
        symlink(format!("{store}/bin/fish"), bin.join("fish")).unwrap();

        assert_eq!(home_manager_root(&dir), Some(PathBuf::from(store)));
        let _ = fs::remove_dir_all(&dir);
    }
}
