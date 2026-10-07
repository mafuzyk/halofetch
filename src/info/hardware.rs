//! CPU, GPU, memory and disk readings.

use std::ffi::CString;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use super::{read_text, sorted_dir, Usage, KIB};

static CPU_SPEED_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*@\s*[\d.]+\s*[GM]Hz$").expect("constant pattern is valid")
});
static CPU_CORE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+\d+-Core(?:\s+Processor|\s+APU)?$").expect("constant pattern is valid")
});
static CPU_TYPE_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+(?:Processor|APU)$").expect("constant pattern is valid"));

const PCI_IDS_PATHS: [&str; 3] = [
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
];

const HWMON_CPU_CHIPS: [&str; 5] = ["k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz"];

/// Turns successive `/proc/stat` snapshots into the busy fraction between them.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuSampler {
    /// Previous (total, idle) jiffies.
    previous: Option<(u64, u64)>,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Busy fraction since the previous call, 0..=1. `None` on the first call, when
    /// `/proc/stat` is unreadable, or when no time passed.
    pub fn sample(&mut self) -> Option<f64> {
        let (total, idle) = read_cpu_totals()?;
        self.update(total, idle)
    }

    fn update(&mut self, total: u64, idle: u64) -> Option<f64> {
        let (previous_total, previous_idle) = self.previous.replace((total, idle))?;
        let dtotal = total.saturating_sub(previous_total);
        let didle = idle.saturating_sub(previous_idle).min(dtotal);
        if dtotal == 0 {
            return None;
        }
        Some((dtotal - didle) as f64 / dtotal as f64)
    }
}

fn read_cpu_totals() -> Option<(u64, u64)> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    parse_cpu_line(text.lines().next()?)
}

/// Parses the aggregate `cpu` line of `/proc/stat` into (total, idle) jiffies.
/// Idle includes iowait, as `top` does.
fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let values: Vec<u64> = fields.map(|field| field.parse().unwrap_or(0)).collect();
    if values.len() < 4 {
        return None;
    }
    let total = values.iter().take(8).sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((total, idle))
}

/// CPU model, thread count and top clock, e.g. `"AMD Ryzen 7 7840U (16) @ 5.13 GHz"`.
pub(super) fn cpu() -> Option<String> {
    let text = fs::read_to_string("/proc/cpuinfo").ok()?;
    let info = parse_cpuinfo(&text);
    let model = clean_cpu_model(&info.model?);
    if model.is_empty() {
        return None;
    }
    Some(format_cpu(&model, info.threads, max_frequency_ghz()))
}

struct CpuInfo {
    model: Option<String>,
    threads: usize,
}

fn parse_cpuinfo(text: &str) -> CpuInfo {
    let mut model_name = None;
    let mut hardware = None;
    let mut processor = None;
    let mut threads = 0;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "processor" => threads += 1,
            "model name" => set_first(&mut model_name, value),
            "Hardware" if value != "UNKNOWN" => set_first(&mut hardware, value),
            "Processor" => set_first(&mut processor, value),
            _ => {}
        }
    }
    CpuInfo {
        model: model_name.or(hardware).or(processor),
        threads,
    }
}

fn set_first(slot: &mut Option<String>, value: &str) {
    if slot.is_none() {
        *slot = Some(value.to_string());
    }
}

/// Removes trademark marks, the "CPU" word, the nominal clock, and core-count or
/// "Processor" suffixes: `"Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz"` becomes
/// `"Intel Core i7-8700"`.
fn clean_cpu_model(model: &str) -> String {
    let mut name = model
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(r)", "")
        .replace("(tm)", "")
        .replace(" CPU", "");
    name = CPU_SPEED_SUFFIX.replace(&name, "").into_owned();
    if let Some(position) = name.find(" with ") {
        name.truncate(position);
    }
    name = CPU_CORE_SUFFIX.replace(&name, "").into_owned();
    name = CPU_TYPE_SUFFIX.replace(&name, "").into_owned();
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn max_frequency_ghz() -> Option<f64> {
    let khz: f64 = read_text("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")?
        .parse()
        .ok()?;
    (khz > 0.0).then(|| khz / 1_000_000.0)
}

fn format_cpu(model: &str, threads: usize, ghz: Option<f64>) -> String {
    let mut text = model.to_string();
    if threads > 0 {
        text = format!("{text} ({threads})");
    }
    if let Some(ghz) = ghz {
        text = format!("{text} @ {ghz:.2} GHz");
    }
    text
}

/// Hottest plausible CPU sensor in degrees Celsius, from hwmon chips and thermal zones.
pub(super) fn cpu_temp() -> Option<f64> {
    let mut best = None;

    for zone in sorted_dir("/sys/class/thermal", |name| {
        name.starts_with("thermal_zone")
    }) {
        let kind = read_text(zone.join("type"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !(kind.contains("x86_pkg_temp") || kind.contains("cpu") || kind.contains("soc")) {
            continue;
        }
        if let Some(raw) = read_f64(zone.join("temp")) {
            keep_plausible(&mut best, temp_from_raw(raw));
        }
    }

    for chip in sorted_dir("/sys/class/hwmon", |_| true) {
        let name = read_text(chip.join("name"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !HWMON_CPU_CHIPS.contains(&name.as_str()) {
            continue;
        }
        for input in sorted_dir(&chip, |name| {
            name.starts_with("temp") && name.ends_with("_input")
        }) {
            if let Some(raw) = read_f64(input) {
                keep_plausible(&mut best, temp_from_raw(raw));
            }
        }
    }

    best
}

/// Sensors report millidegrees; a value at or below 1000 is taken as whole degrees.
fn temp_from_raw(raw: f64) -> f64 {
    if raw > 1000.0 {
        raw / 1000.0
    } else {
        raw
    }
}

fn keep_plausible(best: &mut Option<f64>, celsius: f64) {
    if !(1.0..=150.0).contains(&celsius) {
        return;
    }
    *best = Some(best.map_or(celsius, |current| current.max(celsius)));
}

fn read_f64(path: impl AsRef<std::path::Path>) -> Option<f64> {
    read_text(path)?.parse().ok()
}

fn read_u64(path: impl AsRef<std::path::Path>) -> Option<u64> {
    read_text(path)?.parse().ok()
}

/// RAM and swap usage from `/proc/meminfo`. Swap is `None` when there is none.
pub(super) fn memory() -> (Option<Usage>, Option<Usage>) {
    fs::read_to_string("/proc/meminfo")
        .map(|text| parse_meminfo(&text))
        .unwrap_or_default()
}

fn parse_meminfo(text: &str) -> (Option<Usage>, Option<Usage>) {
    let mut mem_total = None;
    let mut mem_available = None;
    let mut swap_total = None;
    let mut swap_free = None;
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let kib = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok());
        match key.trim() {
            "MemTotal" => mem_total = kib,
            "MemAvailable" => mem_available = kib,
            "SwapTotal" => swap_total = kib,
            "SwapFree" => swap_free = kib,
            _ => {}
        }
    }
    let memory = match (mem_total, mem_available) {
        (Some(total), Some(available)) if total > 0 => Some(Usage {
            used: total.saturating_sub(available).saturating_mul(KIB),
            total: total.saturating_mul(KIB),
        }),
        _ => None,
    };
    let swap = match (swap_total, swap_free) {
        (Some(total), Some(free)) if total > 0 => Some(Usage {
            used: total.saturating_sub(free).saturating_mul(KIB),
            total: total.saturating_mul(KIB),
        }),
        _ => None,
    };
    (memory, swap)
}

/// Usage of the root filesystem, from `statvfs("/")`.
pub(super) fn disk() -> Option<Usage> {
    let path = CString::new("/").ok()?;
    // SAFETY: statvfs is a plain C struct; the call below fills it before any field is read.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is NUL-terminated and `stat` is a valid, writable struct.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = to_u64(stat.f_frsize);
    let total = to_u64(stat.f_blocks).saturating_mul(block);
    let free = to_u64(stat.f_bfree).saturating_mul(block);
    Some(Usage {
        used: total.saturating_sub(free),
        total,
    })
}

/// Widens a platform-dependent C integer to `u64` without a cast that is a no-op on
/// some targets.
fn to_u64<T: TryInto<u64>>(value: T) -> u64 {
    value.try_into().unwrap_or(u64::MAX)
}

/// Video memory of the first DRM card that reports it (amdgpu).
pub(super) fn vram() -> Option<Usage> {
    drm_cards().iter().find_map(|card| {
        let device = card.join("device");
        let total = read_u64(device.join("mem_info_vram_total"))?;
        if total == 0 {
            return None;
        }
        let used = read_u64(device.join("mem_info_vram_used"))?;
        Some(Usage { used, total })
    })
}

/// Busy fraction of the first GPU that exposes `gpu_busy_percent` (amdgpu).
pub(super) fn gpu_busy() -> Option<f64> {
    drm_cards().iter().find_map(|card| {
        let percent = read_f64(card.join("device/gpu_busy_percent"))?;
        Some((percent / 100.0).clamp(0.0, 1.0))
    })
}

/// Names of every GPU, joined with ", " and deduplicated.
pub(super) fn gpu_name() -> Option<String> {
    let cards = drm_cards();
    if cards.is_empty() {
        return None;
    }
    let database = PCI_IDS_PATHS
        .iter()
        .find_map(|path| fs::read_to_string(path).ok());

    let mut names: Vec<String> = Vec::new();
    for card in &cards {
        let device = card.join("device");
        let Some(vendor) = read_text(device.join("vendor")).and_then(|text| parse_hex_u16(&text))
        else {
            continue;
        };
        let device_id = read_text(device.join("device")).and_then(|text| parse_hex_u16(&text));
        let product = read_text(device.join("product_name"));
        let Some(name) = gpu_name_from(vendor, device_id, product.as_deref(), database.as_deref())
        else {
            continue;
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    (!names.is_empty()).then(|| names.join(", "))
}

/// Builds the display name of one GPU. The product name wins over the pci.ids
/// device name; the vendor is prefixed unless the name already starts with it.
fn gpu_name_from(
    vendor: u16,
    device: Option<u16>,
    product: Option<&str>,
    database: Option<&str>,
) -> Option<String> {
    let looked_up = match (database, device) {
        (Some(database), Some(device)) => pci_ids_lookup(database, vendor, device),
        _ => None,
    };
    let (db_vendor, db_device) = match looked_up {
        Some((vendor_name, device_name)) => (
            Some(vendor_name),
            Some(device_name).filter(|name| !name.is_empty()),
        ),
        None => (None, None),
    };
    let short = match vendor {
        0x10de => "NVIDIA".to_string(),
        0x1002 | 0x1022 => "AMD".to_string(),
        0x8086 => "Intel".to_string(),
        _ => db_vendor?,
    };
    let product = product.map(str::trim).filter(|name| !name.is_empty());
    let name = match (product, db_device) {
        (Some(product), _) => product.to_string(),
        (None, Some(device_name)) => prefer_bracket(&device_name).to_string(),
        (None, None) => return Some(format!("{short} GPU")),
    };
    Some(shorten_gpu(&with_vendor(&short, &name)))
}

fn with_vendor(short: &str, name: &str) -> String {
    if name
        .to_ascii_lowercase()
        .starts_with(&short.to_ascii_lowercase())
    {
        name.to_string()
    } else {
        format!("{short} {name}")
    }
}

/// pci.ids names often read `GA106 [GeForce RTX 3060]`; the bracket holds the marketing name.
fn prefer_bracket(name: &str) -> &str {
    let inner = match (name.find('['), name.rfind(']')) {
        (Some(open), Some(close)) if open < close => name[open + 1..close].trim(),
        _ => "",
    };
    if inner.is_empty() {
        name.trim()
    } else {
        inner
    }
}

/// Drops a trailing " Series" or " Graphics" marketing suffix.
fn shorten_gpu(name: &str) -> String {
    let trimmed = name.trim();
    let stripped = trimmed
        .strip_suffix(" Series")
        .or_else(|| trimmed.strip_suffix(" Graphics"))
        .unwrap_or(trimmed)
        .trim_end();
    if stripped.is_empty() {
        trimmed.to_string()
    } else {
        stripped.to_string()
    }
}

/// Looks up a PCI vendor and device in the text of a pci.ids database.
///
/// Returns the vendor name and the device name. The device name is empty when the
/// vendor is known but the device is not. `None` when the vendor is unknown.
fn pci_ids_lookup(database: &str, vendor: u16, device: u16) -> Option<(String, String)> {
    let mut vendor_name: Option<String> = None;
    let mut in_vendor = false;
    for line in database.lines() {
        if line.starts_with("C ") {
            break;
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some(entry) = line.strip_prefix('\t') {
            // Subsystem rows start with two tabs and are not device rows.
            if !in_vendor || entry.starts_with('\t') {
                continue;
            }
            let Some((id, name)) = parse_pci_entry(entry) else {
                continue;
            };
            if id == device {
                return Some((vendor_name.unwrap_or_default(), name.to_string()));
            }
        } else {
            in_vendor = false;
            let Some((id, name)) = parse_pci_entry(line) else {
                continue;
            };
            if id == vendor {
                in_vendor = true;
                vendor_name = Some(name.to_string());
            }
        }
    }
    vendor_name.map(|name| (name, String::new()))
}

/// Splits `"10de  NVIDIA Corporation"` into the hex id and the name.
fn parse_pci_entry(entry: &str) -> Option<(u16, &str)> {
    let id = entry.get(..4)?;
    let rest = entry.get(4..)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let id = u16::from_str_radix(id, 16).ok()?;
    Some((id, rest.trim()))
}

fn parse_hex_u16(text: &str) -> Option<u16> {
    u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

/// Paths of the DRM card devices (`card0`, `card1`, ...), excluding connectors.
fn drm_cards() -> Vec<PathBuf> {
    sorted_dir("/sys/class/drm", |name| {
        name.strip_prefix("card").is_some_and(|index| {
            !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCI_IDS: &str = "\
# Fixture: a few vendors and devices
10de  NVIDIA Corporation
\t2504  GA106 [GeForce RTX 3060 Lite Hash Rate]
\t\t1043 8741  Subsystem name
\t2206  GA102 [GeForce RTX 3080]
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t73df  Navi 22 [Radeon RX 6700/6700 XT/6800M]
8086  Intel Corporation
\t46a6  Alder Lake-P GT2 [Iris Xe Graphics]
C 03  Display controller
\t00  VGA compatible controller
";

    #[test]
    fn cpu_line_totals_and_idle() {
        let line = "cpu  100 20 30 400 10 5 5 0 0 0";
        assert_eq!(parse_cpu_line(line), Some((570, 410)));
        assert_eq!(parse_cpu_line("cpu0 1 2 3 4"), None);
        assert_eq!(parse_cpu_line("intr 1 2 3"), None);
    }

    #[test]
    fn cpu_sampler_reports_busy_fraction_between_calls() {
        let mut sampler = CpuSampler::new();
        assert_eq!(sampler.update(1000, 800), None);
        assert_eq!(sampler.update(1100, 825), Some(0.75));
        assert_eq!(sampler.update(1100, 825), None);
    }

    #[test]
    fn cpu_model_is_shortened() {
        let cases = [
            (
                "AMD Ryzen 3 2200G with Radeon Vega Graphics",
                "AMD Ryzen 3 2200G",
            ),
            ("AMD Ryzen 5 5600X 6-Core Processor", "AMD Ryzen 5 5600X"),
            ("AMD Ryzen 7 5800X3D", "AMD Ryzen 7 5800X3D"),
            ("AMD EPYC 7551P 32-Core Processor", "AMD EPYC 7551P"),
            (
                "Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz",
                "Intel Core i7-8700",
            ),
            (
                "Intel(R) Xeon(R) CPU E5-2680 v4 @ 2.40GHz",
                "Intel Xeon E5-2680 v4",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(clean_cpu_model(input), expected, "{input}");
        }
    }

    #[test]
    fn cpu_line_formats_threads_and_clock() {
        assert_eq!(
            format_cpu("AMD Ryzen 7 7840U", 16, Some(5.13)),
            "AMD Ryzen 7 7840U (16) @ 5.13 GHz"
        );
        assert_eq!(format_cpu("Cortex-A76", 0, None), "Cortex-A76");
        assert_eq!(format_cpu("Cortex-A76", 8, None), "Cortex-A76 (8)");
    }

    #[test]
    fn cpuinfo_uses_model_name_then_arm_fallbacks() {
        let x86 = "processor\t: 0\nmodel name\t: AMD Ryzen 7\nprocessor\t: 1\n";
        let info = parse_cpuinfo(x86);
        assert_eq!(info.model.as_deref(), Some("AMD Ryzen 7"));
        assert_eq!(info.threads, 2);

        let arm = "processor\t: 0\nHardware\t: BCM2835\nprocessor\t: 1\n";
        let info = parse_cpuinfo(arm);
        assert_eq!(info.model.as_deref(), Some("BCM2835"));

        let unknown = "Hardware\t: UNKNOWN\n";
        assert_eq!(parse_cpuinfo(unknown).model, None);
    }

    #[test]
    fn temperatures_are_normalized() {
        assert_eq!(temp_from_raw(54_000.0), 54.0);
        assert_eq!(temp_from_raw(54.0), 54.0);
        let mut best = None;
        keep_plausible(&mut best, 40.0);
        keep_plausible(&mut best, 70.0);
        keep_plausible(&mut best, 200.0);
        keep_plausible(&mut best, 0.5);
        assert_eq!(best, Some(70.0));
    }

    #[test]
    fn meminfo_gives_memory_and_swap() {
        let text = "\
MemTotal:       32000000 kB
MemFree:         1000000 kB
MemAvailable:   26000000 kB
SwapTotal:       4000000 kB
SwapFree:        3000000 kB
";
        let (memory, swap) = parse_meminfo(text);
        assert_eq!(
            memory,
            Some(Usage {
                used: 6_000_000 * 1024,
                total: 32_000_000 * 1024,
            })
        );
        assert_eq!(
            swap,
            Some(Usage {
                used: 1_000_000 * 1024,
                total: 4_000_000 * 1024,
            })
        );
    }

    #[test]
    fn meminfo_without_swap_reports_none() {
        let text = "MemTotal: 100 kB\nMemAvailable: 50 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n";
        let (memory, swap) = parse_meminfo(text);
        assert!(memory.is_some());
        assert_eq!(swap, None);
        assert_eq!(parse_meminfo("garbage").0, None);
    }

    #[test]
    fn pci_lookup_finds_vendor_and_device() {
        assert_eq!(
            pci_ids_lookup(PCI_IDS, 0x10de, 0x2504),
            Some((
                "NVIDIA Corporation".to_string(),
                "GA106 [GeForce RTX 3060 Lite Hash Rate]".to_string()
            ))
        );
        assert_eq!(
            pci_ids_lookup(PCI_IDS, 0x1002, 0x73df),
            Some((
                "Advanced Micro Devices, Inc. [AMD/ATI]".to_string(),
                "Navi 22 [Radeon RX 6700/6700 XT/6800M]".to_string()
            ))
        );
    }

    #[test]
    fn pci_lookup_ignores_subsystems_and_class_listing() {
        assert_eq!(
            pci_ids_lookup(PCI_IDS, 0x10de, 0x1043),
            Some(("NVIDIA Corporation".to_string(), String::new()))
        );
        assert_eq!(
            pci_ids_lookup(PCI_IDS, 0x8086, 0x0000),
            Some(("Intel Corporation".to_string(), String::new()))
        );
        assert_eq!(pci_ids_lookup(PCI_IDS, 0xffff, 0x0001), None);
    }

    #[test]
    fn gpu_names_use_vendor_and_device_database() {
        assert_eq!(
            gpu_name_from(0x10de, Some(0x2504), None, Some(PCI_IDS)).as_deref(),
            Some("NVIDIA GeForce RTX 3060 Lite Hash Rate")
        );
        assert_eq!(
            gpu_name_from(0x8086, Some(0x46a6), None, Some(PCI_IDS)).as_deref(),
            Some("Intel Iris Xe")
        );
        assert_eq!(
            gpu_name_from(0x1002, Some(0x73df), None, Some(PCI_IDS)).as_deref(),
            Some("AMD Radeon RX 6700/6700 XT/6800M")
        );
    }

    #[test]
    fn gpu_names_prefer_product_name_and_avoid_repeated_vendor() {
        assert_eq!(
            gpu_name_from(0x1002, Some(0x1234), Some("AMD Radeon 780M"), None).as_deref(),
            Some("AMD Radeon 780M")
        );
        assert_eq!(
            gpu_name_from(0x1002, None, Some("Radeon RX 570 Series"), None).as_deref(),
            Some("AMD Radeon RX 570")
        );
    }

    #[test]
    fn gpu_names_fall_back_to_vendor_only() {
        assert_eq!(
            gpu_name_from(0x10de, None, None, None).as_deref(),
            Some("NVIDIA GPU")
        );
        assert_eq!(
            gpu_name_from(0x8086, Some(0xffff), None, Some(PCI_IDS)).as_deref(),
            Some("Intel GPU")
        );
        assert_eq!(gpu_name_from(0x1234, None, None, None), None);
    }

    #[test]
    fn gpu_suffix_and_brackets() {
        assert_eq!(shorten_gpu("AMD Radeon RX 570 Series"), "AMD Radeon RX 570");
        assert_eq!(
            shorten_gpu("Intel UHD Graphics 630"),
            "Intel UHD Graphics 630"
        );
        assert_eq!(
            shorten_gpu("NVIDIA GeForce RTX 3060"),
            "NVIDIA GeForce RTX 3060"
        );
        assert_eq!(
            prefer_bracket("GA106 [GeForce RTX 3060]"),
            "GeForce RTX 3060"
        );
        assert_eq!(prefer_bracket("Raphael"), "Raphael");
        assert_eq!(prefer_bracket("Odd []"), "Odd []");
    }

    #[test]
    fn hex_ids_parse_with_or_without_prefix() {
        assert_eq!(parse_hex_u16("0x10de\n"), Some(0x10de));
        assert_eq!(parse_hex_u16("1002"), Some(0x1002));
        assert_eq!(parse_hex_u16("zz"), None);
    }
}
