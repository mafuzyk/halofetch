//! System information: one-shot detection, live refresh and display formatting.
//!
//! The platform detectors live in `linux` and `windows`. This module holds the types,
//! text formatting and helpers that both platforms share, and the public entry points
//! that delegate to the detectors of the current platform.

#[cfg(not(windows))]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use linux as platform;
#[cfg(windows)]
use windows as platform;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::field::Field;

const KIB: u64 = 1024;
const MIB: u64 = KIB * KIB;
const GIB: u64 = MIB * KIB;
const TIB: u64 = GIB * KIB;

static CPU_SPEED_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*@\s*[\d.]+\s*[GM]Hz$").expect("constant pattern is valid")
});
static CPU_CORE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+\d+-Core(?:\s+Processor|\s+APU)?$").expect("constant pattern is valid")
});
static CPU_TYPE_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+(?:Processor|APU)$").expect("constant pattern is valid"));

/// DMI strings that firmware uses as placeholders instead of a real model.
const JUNK_DMI: [&str; 7] = [
    "To Be Filled By O.E.M.",
    "System Product Name",
    "Default string",
    "System manufacturer",
    "Not Applicable",
    "None",
    "",
];

/// Bytes in use and the total of a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub used: u64,
    pub total: u64,
}

impl Usage {
    /// Fraction in use, 0.0..=1.0. Zero when the total is unknown.
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used as f64 / self.total as f64).clamp(0.0, 1.0)
    }
}

/// Numeric values behind the bars and percentages. Each is `None` when unknown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gauges {
    /// CPU busy fraction, 0..=1. Live only.
    pub cpu: Option<f64>,
    /// GPU busy fraction, 0..=1. Live only.
    pub gpu: Option<f64>,
    /// Hottest CPU sensor in degrees Celsius.
    pub cpu_temp: Option<f64>,
    pub memory: Option<Usage>,
    pub swap: Option<Usage>,
    pub disk: Option<Usage>,
    pub vram: Option<Usage>,
    /// Battery charge, 0..=1.
    pub battery: Option<f64>,
    /// Backlight level, 0..=1.
    pub brightness: Option<f64>,
}

/// Everything detected about the machine, stored as display strings per field.
#[derive(Debug, Clone, Default)]
pub struct SysInfo {
    values: BTreeMap<Field, String>,
    pub gauges: Gauges,
    /// The os-release `ID` followed by each `ID_LIKE` word, lowercase. Used to pick a logo.
    /// Windows reports `windows_11` or `windows` instead.
    pub os_ids: Vec<String>,
}

impl SysInfo {
    pub fn get(&self, field: Field) -> Option<&str> {
        self.values.get(&field).map(String::as_str)
    }

    /// Stores the trimmed value. An empty value removes the field.
    pub fn set(&mut self, field: Field, value: impl Into<String>) {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            self.values.remove(&field);
        } else {
            self.values.insert(field, trimmed.to_string());
        }
    }

    pub fn remove(&mut self, field: Field) {
        self.values.remove(&field);
    }

    /// Present fields in [`Field::ALL`] order.
    #[cfg(test)]
    pub fn fields(&self) -> impl Iterator<Item = (Field, &str)> + '_ {
        self.values
            .iter()
            .map(|(field, value)| (*field, value.as_str()))
    }

    /// Bar fill for a gauge field, 0..=1. `None` for fields without a gauge.
    pub fn gauge(&self, field: Field) -> Option<f64> {
        match field {
            Field::CpuUsage => self.gauges.cpu,
            Field::GpuUsage => self.gauges.gpu,
            Field::CpuTemp => self
                .gauges
                .cpu_temp
                .map(|celsius| (celsius / 100.0).clamp(0.0, 1.0)),
            Field::Memory => self.gauges.memory.map(|usage| usage.ratio()),
            Field::Swap => self.gauges.swap.map(|usage| usage.ratio()),
            Field::Disk => self.gauges.disk.map(|usage| usage.ratio()),
            Field::Vram => self.gauges.vram.map(|usage| usage.ratio()),
            Field::Battery => self.gauges.battery,
            Field::Brightness => self.gauges.brightness,
            _ => None,
        }
    }

    /// Realistic, deterministic data for tests, previews and documentation.
    /// Every field is filled and every gauge is set.
    #[cfg(test)]
    pub fn sample() -> SysInfo {
        let memory = Usage {
            used: 6 * GIB + GIB / 100 * 21,
            total: 30 * GIB + GIB / 10 * 6,
        };
        let swap = Usage {
            used: GIB + GIB / 5,
            total: 8 * GIB,
        };
        let disk = Usage {
            used: 412 * GIB,
            total: TIB + TIB / 100 * 82,
        };
        let vram = Usage {
            used: 512 * MIB,
            total: 2 * GIB,
        };

        let text: [(Field, &str); 22] = [
            (Field::Os, "Arch Linux"),
            (Field::Host, "workstation"),
            (Field::Device, "ThinkPad X1 Carbon Gen 11"),
            (Field::User, "atlas"),
            (Field::Kernel, "6.12.1-arch1-1"),
            (Field::Arch, "x86_64"),
            (Field::Uptime, "3h 4m"),
            (Field::Packages, "1203 (pacman), 45 (nix-user)"),
            (Field::Flatpak, "12"),
            (Field::Snap, "2"),
            (Field::Shell, "zsh"),
            (Field::Terminal, "kitty"),
            (Field::Font, "JetBrains Mono"),
            (Field::De, "GNOME"),
            (Field::Wm, "Mutter (Wayland)"),
            (Field::Resolution, "2560x1440, 1920x1080"),
            (Field::Cpu, "AMD Ryzen 7 7840U (16) @ 5.13 GHz"),
            (Field::Gpu, "AMD Radeon 780M"),
            (Field::Load, "0.52 0.48 0.40"),
            (Field::Processes, "312"),
            (Field::LocalIp, "192.168.1.5 (wlan0)"),
            (Field::Wifi, "HomeNet (-52 dBm)"),
        ];
        let mut info = SysInfo::default();
        for (field, value) in text {
            info.set(field, value);
        }
        info.set(Field::Locale, "en_US.UTF-8");
        info.set(Field::CpuUsage, "37%");
        info.set(Field::GpuUsage, "3%");
        info.set(Field::CpuTemp, "54°C");
        info.set(Field::Memory, format_bytes_pair(memory));
        info.set(Field::Swap, format_bytes_pair(swap));
        info.set(Field::Disk, format_bytes_pair(disk));
        info.set(Field::Vram, format_bytes_pair(vram));
        info.set(Field::Battery, "87% (Charging)");
        info.set(Field::Brightness, "60%");

        info.gauges = Gauges {
            cpu: Some(0.37),
            gpu: Some(0.03),
            cpu_temp: Some(54.0),
            memory: Some(memory),
            swap: Some(swap),
            disk: Some(disk),
            vram: Some(vram),
            battery: Some(0.87),
            brightness: Some(0.60),
        };
        info.os_ids = vec!["arch".to_string()];
        info
    }
}

/// Battery charge and state, as shown in the Battery field.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Battery {
    /// Charge, 0..=1.
    pub(super) level: f64,
    /// `Charging`, `Discharging`, `Full` and so on, when the platform reports it.
    pub(super) status: Option<String>,
}

impl Battery {
    /// `"87% (Charging)"`, or `"87%"` without a status.
    pub(super) fn text(&self) -> String {
        let percent = format!("{:.0}%", self.level * 100.0);
        match &self.status {
            Some(status) => format!("{percent} ({status})"),
            None => percent,
        }
    }
}

/// Turns successive CPU time counters into the busy fraction between them.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuSampler {
    /// Previous (total, idle) counters.
    previous: Option<(u64, u64)>,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Busy fraction since the previous call, 0..=1. `None` on the first call, when the
    /// counters are unreadable, or when no time passed.
    pub fn sample(&mut self) -> Option<f64> {
        let (total, idle) = platform::cpu_totals()?;
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

/// Detects everything once. Never fails, never panics and never sleeps. The live
/// values (CPU and GPU usage) are left to [`refresh_live`].
pub fn collect() -> SysInfo {
    let mut info = SysInfo::default();
    platform::collect(&mut info);
    info
}

/// Re-reads the values that change between frames: uptime, memory, swap, disk, battery
/// and process count, plus load, CPU temperature, backlight, CPU and GPU usage where the
/// platform reports them. Spawns no processes. CPU usage needs two calls on the same
/// sampler, so the first call leaves it unset.
pub fn refresh_live(info: &mut SysInfo, sampler: &mut CpuSampler) {
    platform::refresh(info, sampler);
}

/// `"6.21 / 30.6 GiB (20%)"`. The unit is picked from the total and printed once.
pub fn format_bytes_pair(usage: Usage) -> String {
    let (unit, label) = byte_unit(usage.total);
    format!(
        "{} / {} {label} ({:.0}%)",
        scaled(usage.used, unit),
        scaled(usage.total, unit),
        usage.ratio() * 100.0
    )
}

/// `"2.00 GiB"`. The unit is picked from the value itself.
#[cfg(any(windows, test))]
pub(super) fn format_bytes(bytes: u64) -> String {
    let (unit, label) = byte_unit(bytes);
    format!("{} {label}", scaled(bytes, unit))
}

/// The largest unit that does not exceed `bytes`, bytes when none does.
fn byte_unit(bytes: u64) -> (u64, &'static str) {
    const UNITS: [(u64, &str); 5] = [
        (TIB, "TiB"),
        (GIB, "GiB"),
        (MIB, "MiB"),
        (KIB, "KiB"),
        (1, "B"),
    ];
    UNITS
        .iter()
        .copied()
        .find(|(size, _)| bytes >= *size)
        .unwrap_or((1, "B"))
}

/// Expresses `bytes` in `unit` with 2 decimals below 10, 1 below 100 and none above.
fn scaled(bytes: u64, unit: u64) -> String {
    let value = bytes as f64 / unit as f64;
    let decimals = if value < 10.0 {
        2
    } else if value < 100.0 {
        1
    } else {
        0
    };
    format!("{value:.decimals$}")
}

pub(super) fn format_percent(ratio: f64) -> String {
    format!("{:.0}%", ratio * 100.0)
}

/// `"12m"`, `"3h 4m"` or `"2d 3h 4m"`. Zero days and hours are left out.
pub(super) fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    parts.push(format!("{minutes}m"));
    parts.join(" ")
}

/// CPU model, thread count and clock, e.g. `"AMD Ryzen 7 7840U (16) @ 5.13 GHz"`.
/// The thread count and the clock are left out when unknown (zero or `None`).
pub(super) fn format_cpu(model: &str, threads: usize, ghz: Option<f64>) -> String {
    let mut text = model.to_string();
    if threads > 0 {
        text = format!("{text} ({threads})");
    }
    if let Some(ghz) = ghz {
        text = format!("{text} @ {ghz:.2} GHz");
    }
    text
}

/// Removes trademark marks, the "CPU" word, the nominal clock, and core-count or
/// "Processor" suffixes: `"Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz"` becomes
/// `"Intel Core i7-8700"`.
pub(super) fn clean_cpu_model(model: &str) -> String {
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

/// A device name from firmware vendor and product strings. Placeholder values are
/// ignored. A Lenovo product version is the marketing name ("ThinkPad X1 Carbon Gen 11")
/// and is preferred. Otherwise the vendor and product name are joined unless the product
/// already starts with the vendor.
pub(super) fn device_name(
    vendor: Option<&str>,
    product: Option<&str>,
    version: Option<&str>,
) -> Option<String> {
    let vendor = vendor.filter(|value| !is_junk(value));
    let product = product.filter(|value| !is_junk(value));
    let version = version.filter(|value| !is_junk(value));

    if let Some(version) = version.filter(|_| vendor.is_some_and(is_lenovo)) {
        return Some(version.to_string());
    }
    match (vendor, product) {
        (Some(vendor), Some(product)) => {
            if product
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase())
            {
                Some(product.to_string())
            } else {
                Some(format!("{vendor} {product}"))
            }
        }
        (None, Some(product)) => Some(product.to_string()),
        _ => None,
    }
}

fn is_junk(value: &str) -> bool {
    let value = value.trim();
    JUNK_DMI.iter().any(|junk| junk.eq_ignore_ascii_case(value))
}

fn is_lenovo(vendor: &str) -> bool {
    vendor.to_ascii_uppercase().contains("LENOVO")
}

/// `"827 (dpkg)"`, or several managers joined with ", ". Managers with no packages are
/// left out.
pub(super) fn join_counts(counts: &[(&str, usize)]) -> Option<String> {
    let parts: Vec<String> = counts
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(name, count)| format!("{count} ({name})"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Sets the field to the value, or removes it when there is none.
pub(super) fn put(info: &mut SysInfo, field: Field, value: Option<impl Into<String>>) {
    match value {
        Some(value) => info.set(field, value),
        None => info.remove(field),
    }
}

/// Non-empty, trimmed environment variable.
pub(super) fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_ratio_handles_zero_and_overflow() {
        assert_eq!(Usage { used: 1, total: 0 }.ratio(), 0.0);
        assert_eq!(Usage { used: 1, total: 4 }.ratio(), 0.25);
        assert_eq!(Usage { used: 9, total: 4 }.ratio(), 1.0);
    }

    #[test]
    fn set_trims_and_empty_values_remove() {
        let mut info = SysInfo::default();
        info.set(Field::Host, "  workstation \n");
        assert_eq!(info.get(Field::Host), Some("workstation"));
        info.set(Field::Host, "   ");
        assert_eq!(info.get(Field::Host), None);
        info.set(Field::User, "atlas");
        info.remove(Field::User);
        assert_eq!(info.get(Field::User), None);
    }

    #[test]
    fn fields_follow_declaration_order() {
        let mut info = SysInfo::default();
        info.set(Field::Locale, "en_US.UTF-8");
        info.set(Field::Kernel, "6.12");
        info.set(Field::Os, "Arch Linux");
        let keys: Vec<&str> = info.fields().map(|(field, _)| field.key()).collect();
        assert_eq!(keys, ["os", "kernel", "locale"]);
    }

    #[test]
    fn gauge_maps_each_gauge_field() {
        let info = SysInfo::sample();
        assert_eq!(info.gauge(Field::CpuUsage), Some(0.37));
        assert_eq!(info.gauge(Field::Battery), Some(0.87));
        assert_eq!(info.gauge(Field::Brightness), Some(0.60));
        assert!(info
            .gauge(Field::CpuTemp)
            .is_some_and(|ratio| (ratio - 0.54).abs() < 1e-9));
        assert_eq!(info.gauge(Field::Os), None);
    }

    #[test]
    fn cpu_temp_gauge_is_clamped() {
        let mut info = SysInfo::default();
        info.gauges.cpu_temp = Some(150.0);
        assert_eq!(info.gauge(Field::CpuTemp), Some(1.0));
    }

    #[test]
    fn format_bytes_pair_picks_unit_from_total() {
        let usage = Usage {
            used: 512 * MIB,
            total: 2 * GIB,
        };
        assert_eq!(format_bytes_pair(usage), "0.50 / 2.00 GiB (25%)");
        let usage = Usage {
            used: 6 * GIB + GIB / 100 * 21,
            total: 30 * GIB + GIB / 10 * 6,
        };
        assert_eq!(format_bytes_pair(usage), "6.21 / 30.6 GiB (20%)");
        let disk = Usage {
            used: 0,
            total: TIB + TIB / 100 * 82,
        };
        assert_eq!(format_bytes_pair(disk), "0.00 / 1.82 TiB (0%)");
        let zero = Usage { used: 0, total: 0 };
        assert_eq!(format_bytes_pair(zero), "0.00 / 0.00 B (0%)");
    }

    #[test]
    fn format_bytes_picks_unit_from_value() {
        assert_eq!(format_bytes(2 * GIB), "2.00 GiB");
        assert_eq!(format_bytes(12 * GIB), "12.0 GiB");
        assert_eq!(format_bytes(0), "0.00 B");
    }

    #[test]
    fn format_bytes_pair_decimals_follow_magnitude() {
        let usage = Usage {
            used: 647 * MIB,
            total: GIB * 1572 / 100,
        };
        assert_eq!(format_bytes_pair(usage), "0.63 / 15.7 GiB (4%)");
        let usage = Usage {
            used: GIB * 1033 / 100,
            total: GIB * 25197 / 100,
        };
        assert_eq!(format_bytes_pair(usage), "10.3 / 252 GiB (4%)");
        let usage = Usage {
            used: 412 * GIB,
            total: TIB * 182 / 100,
        };
        assert_eq!(format_bytes_pair(usage), "0.40 / 1.82 TiB (22%)");
    }

    #[test]
    fn sample_fills_every_field_and_gauge() {
        let info = SysInfo::sample();
        for field in Field::ALL {
            assert!(info.get(field).is_some(), "missing {}", field.key());
        }
        assert!(info.gauges.memory.is_some());
        assert!(info.gauges.swap.is_some());
        assert!(info.gauges.disk.is_some());
        assert!(info.gauges.vram.is_some());
        assert_eq!(info.os_ids, ["arch"]);
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
    fn uptime_format_omits_zero_leading_units() {
        assert_eq!(format_uptime(0), "0m");
        assert_eq!(format_uptime(59), "0m");
        assert_eq!(format_uptime(12 * 60), "12m");
        assert_eq!(format_uptime(3 * 3600 + 4 * 60), "3h 4m");
        assert_eq!(format_uptime(2 * 86_400 + 3 * 3600 + 4 * 60), "2d 3h 4m");
        assert_eq!(format_uptime(2 * 86_400 + 4 * 60), "2d 4m");
    }

    #[test]
    fn device_name_joins_vendor_and_product() {
        assert_eq!(
            device_name(Some("Dell Inc."), Some("XPS 13"), None).as_deref(),
            Some("Dell Inc. XPS 13")
        );
        assert_eq!(
            device_name(Some("ASUSTeK"), Some("ASUSTeK ROG Zephyrus"), None).as_deref(),
            Some("ASUSTeK ROG Zephyrus")
        );
        assert_eq!(
            device_name(None, Some("Framework Laptop"), None).as_deref(),
            Some("Framework Laptop")
        );
    }

    #[test]
    fn lenovo_uses_the_product_version() {
        assert_eq!(
            device_name(
                Some("LENOVO"),
                Some("21HMCTO1WW"),
                Some("ThinkPad X1 Carbon Gen 11"),
            )
            .as_deref(),
            Some("ThinkPad X1 Carbon Gen 11")
        );
    }

    #[test]
    fn junk_dmi_values_are_ignored() {
        assert_eq!(
            device_name(
                Some("System manufacturer"),
                Some("System Product Name"),
                Some("Default string"),
            ),
            None
        );
        assert_eq!(
            device_name(Some("Acme"), Some("to be filled by o.e.m."), None),
            None
        );
    }

    #[test]
    fn battery_text_with_and_without_status() {
        let charging = Battery {
            level: 0.87,
            status: Some("Charging".to_string()),
        };
        assert_eq!(charging.text(), "87% (Charging)");
        let unknown = Battery {
            level: 0.5,
            status: None,
        };
        assert_eq!(unknown.text(), "50%");
    }

    #[test]
    fn joined_counts_skip_empty_managers() {
        assert_eq!(
            join_counts(&[("pacman", 1203), ("dpkg", 0), ("nix-user", 45)]).as_deref(),
            Some("1203 (pacman), 45 (nix-user)")
        );
        assert_eq!(join_counts(&[("dpkg", 0)]), None);
        assert_eq!(join_counts(&[]), None);
    }

    #[test]
    fn single_manager_has_no_separator() {
        assert_eq!(join_counts(&[("dpkg", 827)]).as_deref(), Some("827 (dpkg)"));
    }

    #[test]
    fn collect_and_refresh_do_not_panic() {
        let mut info = collect();
        let mut sampler = CpuSampler::default();
        refresh_live(&mut info, &mut sampler);
        refresh_live(&mut info, &mut sampler);
    }
}
