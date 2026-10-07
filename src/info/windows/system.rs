//! Identity, operating system, firmware model, uptime and locale.

use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::SystemInformation::{
    ComputerNameDnsHostname, GetComputerNameExW, GetNativeSystemInfo, GetTickCount64,
    PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64, PROCESSOR_ARCHITECTURE_INTEL,
    SYSTEM_INFO,
};
use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

use super::from_wide;
use super::registry::{read_string, read_u32};
use crate::info::{device_name, env_value};

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const BIOS: &str = r"HARDWARE\DESCRIPTION\System\BIOS";

/// First build number of Windows 11. Its registry product name still says "Windows 10".
const WINDOWS_11_BUILD: u32 = 22000;

/// Longest user name (UNLEN) plus the terminator.
const USER_NAME_CAPACITY: usize = 257;
/// Longest DNS host name plus the terminator.
const HOST_NAME_CAPACITY: usize = 256;
/// Longest locale name (LOCALE_NAME_MAX_LENGTH) including the terminator.
const LOCALE_NAME_CAPACITY: usize = 85;

pub(super) fn user() -> Option<String> {
    env_value("USERNAME").or_else(user_name_api)
}

fn user_name_api() -> Option<String> {
    let mut buffer = [0u16; USER_NAME_CAPACITY];
    let mut size = buffer.len() as u32;
    // SAFETY: `buffer` has room for `size` UTF-16 units, and `size` is passed by pointer.
    if unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut size) } == 0 {
        return None;
    }
    let name = from_wide(&buffer);
    (!name.is_empty()).then_some(name)
}

pub(super) fn host() -> Option<String> {
    let mut buffer = [0u16; HOST_NAME_CAPACITY];
    let mut size = buffer.len() as u32;
    // SAFETY: `buffer` has room for `size` UTF-16 units, and `size` is passed by pointer.
    let ok = unsafe { GetComputerNameExW(ComputerNameDnsHostname, buffer.as_mut_ptr(), &mut size) };
    let name = from_wide(&buffer);
    (ok != 0 && !name.is_empty()).then_some(name)
}

/// Hardware model from the firmware values in the BIOS key. The same junk filter and
/// vendor handling as on Linux apply.
pub(super) fn device() -> Option<String> {
    let bios = |value: &str| read_string(HKEY_LOCAL_MACHINE, BIOS, value);
    device_name(
        bios("SystemManufacturer").as_deref(),
        bios("SystemProductName").as_deref(),
        bios("SystemVersion").as_deref(),
    )
}

/// Display name of the operating system and the os-release style identifiers.
pub(super) fn os() -> (Option<String>, Vec<String>) {
    let product = current_version("ProductName");
    let display = current_version("DisplayVersion").or_else(|| current_version("ReleaseId"));
    let build = build_number();
    let name = os_name(
        product.as_deref().unwrap_or("Windows"),
        display.as_deref(),
        build,
    );
    (Some(name), os_ids(build))
}

/// `"Windows 11 Pro 24H2"`. Windows 11 keeps the "Windows 10" product name in the
/// registry, so the name is corrected from the build number.
fn os_name(product: &str, display: Option<&str>, build: Option<u32>) -> String {
    let product = match (product.strip_prefix("Windows 10"), build) {
        (Some(rest), Some(build)) if build >= WINDOWS_11_BUILD => format!("Windows 11{rest}"),
        _ => product.to_string(),
    };
    match display {
        Some(display) => format!("{product} {display}"),
        None => product,
    }
}

/// Logo keys for the operating system. Both are embedded logo names.
fn os_ids(build: Option<u32>) -> Vec<String> {
    let id = if build.is_some_and(|build| build >= WINDOWS_11_BUILD) {
        "windows_11"
    } else {
        "windows"
    };
    vec![id.to_string()]
}

/// `"10.0.26100.2033"`, or without the revision when it is unknown.
pub(super) fn kernel() -> Option<String> {
    let major = current_version_u32("CurrentMajorVersionNumber")?;
    let minor = current_version_u32("CurrentMinorVersionNumber")?;
    let build = build_number()?;
    Some(kernel_version(
        major,
        minor,
        build,
        current_version_u32("UBR"),
    ))
}

fn kernel_version(major: u32, minor: u32, build: u32, revision: Option<u32>) -> String {
    match revision {
        Some(revision) => format!("{major}.{minor}.{build}.{revision}"),
        None => format!("{major}.{minor}.{build}"),
    }
}

fn build_number() -> Option<u32> {
    current_version("CurrentBuildNumber")
        .or_else(|| current_version("CurrentBuild"))
        .and_then(|text| text.parse().ok())
}

fn current_version(value: &str) -> Option<String> {
    read_string(HKEY_LOCAL_MACHINE, CURRENT_VERSION, value)
}

fn current_version_u32(value: &str) -> Option<u32> {
    read_u32(HKEY_LOCAL_MACHINE, CURRENT_VERSION, value)
}

/// Machine architecture from `GetNativeSystemInfo`, so a 32-bit build on 64-bit Windows
/// still reports the machine.
pub(super) fn arch() -> Option<String> {
    let mut info = SYSTEM_INFO::default();
    // SAFETY: `info` is a valid, writable SYSTEM_INFO.
    unsafe { GetNativeSystemInfo(&mut info) };
    // SAFETY: GetNativeSystemInfo filled in the architecture member of the union.
    let code = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture };
    let name = arch_name(code).unwrap_or(std::env::consts::ARCH);
    Some(name.to_string())
}

fn arch_name(code: u16) -> Option<&'static str> {
    Some(match code {
        PROCESSOR_ARCHITECTURE_AMD64 => "x86_64",
        PROCESSOR_ARCHITECTURE_ARM64 => "aarch64",
        PROCESSOR_ARCHITECTURE_INTEL => "x86",
        _ => return None,
    })
}

/// Milliseconds since boot, as whole seconds.
pub(super) fn uptime_secs() -> Option<u64> {
    // SAFETY: GetTickCount64 has no preconditions.
    Some(unsafe { GetTickCount64() } / 1000)
}

/// Locale of the current user, such as `"en-US"`.
pub(super) fn locale() -> Option<String> {
    let mut buffer = [0u16; LOCALE_NAME_CAPACITY];
    let capacity = buffer.len() as i32;
    // SAFETY: `buffer` has room for `capacity` UTF-16 units, including the terminator.
    let length = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), capacity) };
    if length <= 0 {
        return None;
    }
    let name = from_wide(&buffer);
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_build_renames_the_product() {
        assert_eq!(
            os_name("Windows 10 Pro", Some("24H2"), Some(26100)),
            "Windows 11 Pro 24H2"
        );
        assert_eq!(
            os_name("Windows 10 Pro", Some("22H2"), Some(19045)),
            "Windows 10 Pro 22H2"
        );
        assert_eq!(os_name("Windows 10 Home", None, None), "Windows 10 Home");
        assert_eq!(
            os_name("Windows 11 Home", Some("23H2"), Some(22631)),
            "Windows 11 Home 23H2"
        );
    }

    #[test]
    fn os_ids_follow_the_build() {
        assert_eq!(os_ids(Some(26100)), ["windows_11"]);
        assert_eq!(os_ids(Some(19045)), ["windows"]);
        assert_eq!(os_ids(None), ["windows"]);
    }

    #[test]
    fn kernel_version_includes_revision_when_known() {
        assert_eq!(kernel_version(10, 0, 26100, Some(2033)), "10.0.26100.2033");
        assert_eq!(kernel_version(10, 0, 26100, None), "10.0.26100");
    }

    #[test]
    fn architecture_codes_map_to_names() {
        assert_eq!(arch_name(9), Some("x86_64"));
        assert_eq!(arch_name(12), Some("aarch64"));
        assert_eq!(arch_name(0), Some("x86"));
        assert_eq!(arch_name(5), None);
    }

    #[test]
    fn calls_do_not_panic() {
        assert!(arch().is_some_and(|value| !value.is_empty()));
        let _ = (
            user(),
            host(),
            device(),
            os(),
            kernel(),
            locale(),
            uptime_secs(),
        );
    }
}
