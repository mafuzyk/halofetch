//! Identity, operating system, firmware model, uptime, load and locale.

use std::ffi::CStr;
use std::fs;

use super::read_text;
use crate::info::{device_name, env_value};

pub(super) fn user() -> Option<String> {
    env_value("USER").or_else(passwd_user)
}

/// Name of the effective user from the passwd database.
fn passwd_user() -> Option<String> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    // SAFETY: getpwuid returns null or a pointer to static storage. The name is copied
    // below before any other passwd lookup can overwrite it.
    let entry = unsafe { libc::getpwuid(uid) };
    if entry.is_null() {
        return None;
    }
    // SAFETY: a non-null entry has a NUL-terminated pw_name.
    let name = unsafe { CStr::from_ptr((*entry).pw_name) };
    name.to_str().ok().map(str::to_owned)
}

pub(super) fn host() -> Option<String> {
    read_text("/proc/sys/kernel/hostname")
}

pub(super) fn kernel() -> Option<String> {
    read_text("/proc/sys/kernel/osrelease")
}

/// Machine hardware name from `uname`, falling back to the compile-time target.
pub(super) fn arch() -> Option<String> {
    machine().or_else(|| Some(std::env::consts::ARCH.to_string()))
}

fn machine() -> Option<String> {
    // SAFETY: utsname is a plain C struct; the call below fills it before it is read.
    let mut info: libc::utsname = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a valid, writable utsname.
    if unsafe { libc::uname(&mut info) } != 0 {
        return None;
    }
    // SAFETY: uname NUL-terminates every field.
    let machine = unsafe { CStr::from_ptr(info.machine.as_ptr()) };
    machine
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// Operating system name and the os-release identifiers (`ID`, then each `ID_LIKE` word).
pub(super) fn os_release() -> (Option<String>, Vec<String>) {
    ["/etc/os-release", "/usr/lib/os-release"]
        .iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .map(|text| parse_os_release(&text))
        .unwrap_or_default()
}

/// Pretty name (or `NAME VERSION_ID`) and the lowercase identifiers from os-release text.
pub(super) fn parse_os_release(text: &str) -> (Option<String>, Vec<String>) {
    let mut pretty = None;
    let mut name = None;
    let mut version = None;
    let mut id = None;
    let mut id_like = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches(|c: char| c == '"' || c == '\'')
            .trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "PRETTY_NAME" => pretty = Some(value.to_string()),
            "NAME" => name = Some(value.to_string()),
            "VERSION_ID" => version = Some(value.to_string()),
            "ID" => id = Some(value.to_ascii_lowercase()),
            "ID_LIKE" => id_like = Some(value.to_string()),
            _ => {}
        }
    }

    let mut ids: Vec<String> = Vec::new();
    ids.extend(id);
    for word in id_like.unwrap_or_default().split_whitespace() {
        let word = word.to_ascii_lowercase();
        if !ids.contains(&word) {
            ids.push(word);
        }
    }

    let display = pretty.or_else(|| {
        name.map(|name| match version {
            Some(version) => format!("{name} {version}"),
            None => name,
        })
    });
    (display, ids)
}

/// Hardware model from DMI, or the device tree on ARM boards. See [`device_name`] for
/// how the vendor, product and version strings are combined.
pub(super) fn device() -> Option<String> {
    let dmi = |file: &str| read_text(format!("/sys/devices/virtual/dmi/id/{file}"));
    let vendor = dmi("sys_vendor");
    let product = dmi("product_name");
    let version = dmi("product_version");
    device_name(vendor.as_deref(), product.as_deref(), version.as_deref()).or_else(devicetree_model)
}

fn devicetree_model() -> Option<String> {
    let raw = fs::read_to_string("/sys/firmware/devicetree/base/model").ok()?;
    let model = raw.trim_end_matches('\0').trim();
    (!model.is_empty()).then(|| model.to_string())
}

/// Seconds since boot, from `/proc/uptime`.
pub(super) fn uptime_secs() -> Option<u64> {
    let text = read_text("/proc/uptime")?;
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(seconds.max(0.0) as u64)
}

/// One, five and fifteen minute load averages, from `/proc/loadavg`.
pub(super) fn load() -> Option<String> {
    let text = read_text("/proc/loadavg")?;
    let values = text
        .split_whitespace()
        .take(3)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<f64>, _>>()
        .ok()?;
    if values.len() != 3 {
        return None;
    }
    Some(
        values
            .iter()
            .map(|value| format!("{value:.2}"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

pub(super) fn locale() -> Option<String> {
    locale_from(env_value("LC_ALL").as_deref(), env_value("LANG").as_deref())
}

/// `LC_ALL` overrides `LANG`. The C and POSIX locales carry no information and are skipped.
fn locale_from(lc_all: Option<&str>, lang: Option<&str>) -> Option<String> {
    lc_all
        .or(lang)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "C" && *value != "POSIX")
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_prefers_pretty_name_and_collects_ids() {
        let arch = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\n";
        assert_eq!(
            parse_os_release(arch),
            (Some("Arch Linux".to_string()), vec!["arch".to_string()])
        );

        let ubuntu = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nID=ubuntu\nID_LIKE=debian\n";
        assert_eq!(
            parse_os_release(ubuntu),
            (
                Some("Ubuntu 24.04".to_string()),
                vec!["ubuntu".to_string(), "debian".to_string()]
            )
        );

        let manjaro = "PRETTY_NAME='Manjaro Linux'\nID=manjaro\nID_LIKE=arch\n";
        assert_eq!(
            parse_os_release(manjaro).1,
            ["manjaro".to_string(), "arch".to_string()]
        );
    }

    #[test]
    fn os_release_without_name_is_none() {
        assert_eq!(parse_os_release("GARBAGE\n"), (None, Vec::new()));
    }

    #[test]
    fn os_release_ids_are_deduplicated() {
        let text = "ID=fedora\nID_LIKE=\"fedora rhel\"\n";
        assert_eq!(
            parse_os_release(text).1,
            ["fedora".to_string(), "rhel".to_string()]
        );
    }

    #[test]
    fn locale_precedence_and_c_skipping() {
        assert_eq!(
            locale_from(None, Some("en_US.UTF-8")).as_deref(),
            Some("en_US.UTF-8")
        );
        assert_eq!(
            locale_from(Some("de_DE.UTF-8"), Some("en_US.UTF-8")).as_deref(),
            Some("de_DE.UTF-8")
        );
        assert_eq!(locale_from(Some("C"), Some("en_US.UTF-8")), None);
        assert_eq!(locale_from(None, Some("POSIX")), None);
        assert_eq!(locale_from(None, None), None);
    }

    #[test]
    fn arch_is_never_empty() {
        assert!(arch().is_some_and(|value| !value.is_empty()));
    }
}
