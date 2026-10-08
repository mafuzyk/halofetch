//! CPU, GPU, memory and disk readings from Win32 and the device class registry.

use std::mem::size_of;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetActiveProcessorCount, GetSystemTimes, ALL_PROCESSOR_GROUPS,
};

use super::registry::{read_string, read_u32, read_u64, subkeys};
use super::to_wide;
use crate::info::{clean_cpu_model, env_value, format_cpu, Usage};

const CPU0: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";

/// Device setup class of display adapters. Adapter keys are `0000`, `0001`, and so on.
const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// Adapters that only mirror the desktop or a remote session.
const GENERIC_ADAPTERS: [&str; 2] = [
    "Microsoft Basic Display Adapter",
    "Microsoft Remote Display Adapter",
];

/// Aggregate CPU time as (total, idle) in 100 ns ticks. Kernel time includes idle time,
/// so the total is kernel plus user.
pub(in crate::info) fn cpu_totals() -> Option<(u64, u64)> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: each pointer refers to a valid, writable FILETIME.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    let total = ticks(kernel).saturating_add(ticks(user));
    Some((total, ticks(idle)))
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

/// CPU model, logical processor count and nominal clock, e.g.
/// `"AMD Ryzen 7 7840U (16) @ 5.13 GHz"`.
pub(super) fn cpu() -> Option<String> {
    let model = clean_cpu_model(&read_string(
        HKEY_LOCAL_MACHINE,
        CPU0,
        "ProcessorNameString",
    )?);
    if model.is_empty() {
        return None;
    }
    // SAFETY: GetActiveProcessorCount has no preconditions.
    let processors = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) };
    let threads = processors as usize;
    let ghz = read_u32(HKEY_LOCAL_MACHINE, CPU0, "~MHz")
        .filter(|mhz| *mhz > 0)
        .map(|mhz| f64::from(mhz) / 1000.0);
    Some(format_cpu(&model, threads, ghz))
}

/// RAM and page file usage. Swap is the page file part of the commit limit, which
/// counts RAM as well.
pub(super) fn memory() -> (Option<Usage>, Option<Usage>) {
    let Some(status) = memory_status() else {
        return (None, None);
    };
    memory_from(
        status.ullTotalPhys,
        status.ullAvailPhys,
        status.ullTotalPageFile,
        status.ullAvailPageFile,
    )
}

fn memory_status() -> Option<MEMORYSTATUSEX> {
    // SAFETY: MEMORYSTATUSEX is plain data, so all-zero is a valid value before dwLength
    // is set below.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `status` is writable and its dwLength is set as the API requires.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status)
}

fn memory_from(
    total_phys: u64,
    avail_phys: u64,
    total_page: u64,
    avail_page: u64,
) -> (Option<Usage>, Option<Usage>) {
    let memory = (total_phys > 0).then(|| Usage {
        used: total_phys.saturating_sub(avail_phys),
        total: total_phys,
    });
    let swap_total = total_page.saturating_sub(total_phys);
    let swap = (swap_total > 0).then(|| {
        let avail_swap = avail_page.saturating_sub(avail_phys);
        Usage {
            used: swap_total.saturating_sub(avail_swap),
            total: swap_total,
        }
    });
    (memory, swap)
}

/// Usage of the system drive, `%SystemDrive%` or `C:` when that is not set.
pub(super) fn disk() -> Option<Usage> {
    let drive = env_value("SystemDrive").unwrap_or_else(|| "C:".to_string());
    let root = to_wide(&format!("{drive}\\"));
    let mut available: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    // SAFETY: `root` is NUL-terminated and each out-parameter is a valid u64.
    let ok =
        unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut available, &mut total, &mut total_free) };
    (ok != 0 && total > 0).then(|| Usage {
        used: total.saturating_sub(total_free),
        total,
    })
}

/// Names of the display adapters, joined with ", ". The generic mirror adapters are left
/// out, and so are repeats.
pub(super) fn gpu_name() -> Option<String> {
    let names = subkeys(HKEY_LOCAL_MACHINE, DISPLAY_CLASS)
        .into_iter()
        .filter(|name| is_adapter_key(name))
        .filter_map(|name| {
            read_string(
                HKEY_LOCAL_MACHINE,
                &format!("{DISPLAY_CLASS}\\{name}"),
                "DriverDesc",
            )
        });
    gpu_list(names)
}

/// Adapter subkeys are four decimal digits.
fn is_adapter_key(name: &str) -> bool {
    name.len() == 4 && name.bytes().all(|byte| byte.is_ascii_digit())
}

/// Display drivers for virtual monitors (streaming, VR, remote desktop) that sit next to
/// the real GPU.
fn is_virtual_adapter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "virtual display",
        "virtual monitor",
        "indirect display",
        "parsec",
    ]
    .iter()
    .any(|marker| name.contains(marker))
}

fn gpu_list(names: impl IntoIterator<Item = String>) -> Option<String> {
    let mut unique: Vec<String> = Vec::new();
    for name in names {
        let name = clean_adapter_name(&name);
        if name.is_empty()
            || GENERIC_ADAPTERS.contains(&name.as_str())
            || is_virtual_adapter(&name)
            || unique.contains(&name)
        {
            continue;
        }
        unique.push(name);
    }
    (!unique.is_empty()).then(|| unique.join(", "))
}

/// The adapter description without the trademark marks and with single spaces.
fn clean_adapter_name(description: &str) -> String {
    let name = description.replace("(R)", "").replace("(TM)", "");
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Video memory of the largest dedicated display adapter, in bytes.
pub(super) fn vram_total() -> Option<u64> {
    let adapters = subkeys(HKEY_LOCAL_MACHINE, DISPLAY_CLASS)
        .into_iter()
        .filter(|name| is_adapter_key(name))
        .map(|name| {
            let key = format!("{DISPLAY_CLASS}\\{name}");
            let description = read_string(HKEY_LOCAL_MACHINE, &key, "DriverDesc");
            let memory = read_u64(HKEY_LOCAL_MACHINE, &key, "HardwareInformation.qwMemorySize")
                .or_else(|| read_u64(HKEY_LOCAL_MACHINE, &key, "HardwareInformation.MemorySize"));
            (description, memory)
        });
    largest_vram(adapters)
}

/// Largest non-zero memory size among the adapters that have a description and are
/// neither generic nor virtual.
fn largest_vram(adapters: impl IntoIterator<Item = (Option<String>, Option<u64>)>) -> Option<u64> {
    adapters
        .into_iter()
        .filter_map(|(description, memory)| {
            let name = clean_adapter_name(&description?);
            if name.is_empty()
                || GENERIC_ADAPTERS.contains(&name.as_str())
                || is_virtual_adapter(&name)
            {
                return None;
            }
            memory
        })
        .filter(|bytes| *bytes > 0)
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_combine_high_and_low_words() {
        let time = FILETIME {
            dwLowDateTime: 5,
            dwHighDateTime: 1,
        };
        assert_eq!(ticks(time), (1u64 << 32) | 5);
    }

    #[test]
    fn memory_gives_ram_and_page_file_share() {
        let (memory, swap) = memory_from(
            32_000_000_000,
            26_000_000_000,
            40_000_000_000,
            30_000_000_000,
        );
        assert_eq!(
            memory,
            Some(Usage {
                used: 6_000_000_000,
                total: 32_000_000_000,
            })
        );
        assert_eq!(
            swap,
            Some(Usage {
                used: 4_000_000_000,
                total: 8_000_000_000,
            })
        );
    }

    #[test]
    fn memory_without_page_file_has_no_swap() {
        let (memory, swap) = memory_from(100, 50, 100, 50);
        assert!(memory.is_some());
        assert_eq!(swap, None);
        assert_eq!(memory_from(0, 0, 0, 0), (None, None));
    }

    #[test]
    fn swap_used_never_goes_negative() {
        let (_, swap) = memory_from(100, 90, 300, 400);
        assert_eq!(
            swap,
            Some(Usage {
                used: 0,
                total: 200
            })
        );
    }

    #[test]
    fn adapter_keys_are_four_digits() {
        assert!(is_adapter_key("0000"));
        assert!(is_adapter_key("0012"));
        assert!(!is_adapter_key("Properties"));
        assert!(!is_adapter_key("000"));
        assert!(!is_adapter_key("00001"));
    }

    #[test]
    fn gpu_names_are_deduplicated_and_generic_adapters_skipped() {
        let names = [
            "Intel(R) UHD Graphics 630",
            "Microsoft Basic Display Adapter",
            "Virtual Display Driver by MTT",
            "Parsec Virtual Display Adapter",
            "NVIDIA GeForce RTX 4070",
            "Intel(R) UHD Graphics 630",
            "",
        ]
        .map(str::to_string);
        assert_eq!(
            gpu_list(names).as_deref(),
            Some("Intel UHD Graphics 630, NVIDIA GeForce RTX 4070")
        );
        assert_eq!(
            gpu_list(["Microsoft Basic Display Adapter".to_string()]),
            None
        );
    }

    #[test]
    fn largest_vram_skips_missing_generic_virtual_and_empty_adapters() {
        const GIB: u64 = 1 << 30;
        let adapters = [
            (
                Some("Microsoft Basic Display Adapter".to_string()),
                Some(16 * GIB),
            ),
            (
                Some("Virtual Display Driver by MTT".to_string()),
                Some(32 * GIB),
            ),
            (None, Some(24 * GIB)),
            (Some("Intel(R) UHD Graphics 630".to_string()), Some(GIB)),
            (Some("NVIDIA GeForce RTX 4070".to_string()), Some(12 * GIB)),
            (Some("NVIDIA GeForce GTX 1050".to_string()), Some(0)),
            (Some("AMD Radeon Graphics".to_string()), None),
        ];
        assert_eq!(largest_vram(adapters), Some(12 * GIB));
        assert_eq!(
            largest_vram([(Some("NVIDIA GeForce GTX 1050".to_string()), Some(0))]),
            None
        );
    }

    #[test]
    fn calls_do_not_panic() {
        let _ = (
            cpu(),
            memory(),
            disk(),
            gpu_name(),
            vram_total(),
            cpu_totals(),
        );
    }
}
