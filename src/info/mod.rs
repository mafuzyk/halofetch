//! System information: one-shot detection, live refresh and display formatting.
//!
//! Everything reads `/proc`, `/sys` and the environment directly. `collect` runs the
//! full detection pass; `refresh_live` re-reads the values that change between frames
//! and never spawns a process.

mod desktop;
mod hardware;
mod network;
mod packages;
mod power;
mod procs;
mod system;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use hardware::CpuSampler;

use crate::field::Field;

const KIB: u64 = 1024;
const MIB: u64 = KIB * KIB;
const GIB: u64 = MIB * KIB;
const TIB: u64 = GIB * KIB;

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

/// Detects everything once. Never fails, never panics and never sleeps. The live
/// values (CPU and GPU usage) are left to [`refresh_live`].
pub fn collect() -> SysInfo {
    let mut info = SysInfo::default();
    let parent = std::os::unix::process::parent_id();

    std::thread::scope(|scope| {
        let procs = scope.spawn(procs::ProcTable::scan);
        let packages = scope.spawn(packages::collect);
        let gpu = scope.spawn(hardware::gpu_name);
        let wifi = scope.spawn(network::wifi);
        let font = scope.spawn(desktop::font);

        put(&mut info, Field::User, system::user());
        put(&mut info, Field::Host, system::host());
        put(&mut info, Field::Device, system::device());
        let (os, os_ids) = system::os_release();
        put(&mut info, Field::Os, os);
        info.os_ids = os_ids;
        put(&mut info, Field::Kernel, system::kernel());
        put(&mut info, Field::Arch, system::arch());
        put(
            &mut info,
            Field::Uptime,
            system::uptime_secs().map(system::format_uptime),
        );
        put(&mut info, Field::Locale, system::locale());
        put(&mut info, Field::Load, system::load());
        put(&mut info, Field::Cpu, hardware::cpu());
        let cpu_temp = hardware::cpu_temp();
        info.gauges.cpu_temp = cpu_temp;
        put(&mut info, Field::CpuTemp, cpu_temp.map(format_celsius));
        let (memory, swap) = hardware::memory();
        info.gauges.memory = memory;
        put(&mut info, Field::Memory, memory.map(format_bytes_pair));
        info.gauges.swap = swap;
        put(&mut info, Field::Swap, swap.map(format_bytes_pair));
        let disk = hardware::disk();
        info.gauges.disk = disk;
        put(&mut info, Field::Disk, disk.map(format_bytes_pair));
        let vram = hardware::vram();
        info.gauges.vram = vram;
        put(&mut info, Field::Vram, vram.map(format_bytes_pair));
        let battery = power::battery();
        info.gauges.battery = battery.as_ref().map(|battery| battery.level);
        put(
            &mut info,
            Field::Battery,
            battery.as_ref().map(power::Battery::text),
        );
        let brightness = power::brightness();
        info.gauges.brightness = brightness;
        put(&mut info, Field::Brightness, brightness.map(format_percent));
        put(&mut info, Field::Resolution, desktop::resolution());
        put(&mut info, Field::LocalIp, network::local_ip());
        put(
            &mut info,
            Field::Processes,
            procs::count().map(|n| n.to_string()),
        );

        let packages = packages.join().unwrap_or_default();
        put(&mut info, Field::Packages, packages.summary);
        put(
            &mut info,
            Field::Flatpak,
            packages.flatpak.map(|n| n.to_string()),
        );
        put(&mut info, Field::Snap, packages.snap.map(|n| n.to_string()));
        put(&mut info, Field::Gpu, gpu.join().unwrap_or_default());
        let wifi = wifi.join().unwrap_or_default();
        put(
            &mut info,
            Field::Wifi,
            wifi.as_ref().map(network::Wifi::text),
        );
        put(&mut info, Field::Font, font.join().unwrap_or_default());

        let procs = procs.join().unwrap_or_default();
        let chain = procs.chain(parent);
        let comms = procs.comms();
        put(&mut info, Field::Shell, desktop::shell(&chain));
        put(&mut info, Field::Terminal, desktop::terminal(&chain));
        put(&mut info, Field::De, desktop::de(&comms));
        put(&mut info, Field::Wm, desktop::wm(&comms));
    });

    info
}

/// Re-reads the values that change between frames: uptime, memory, swap, disk, load,
/// process count, battery, CPU temperature, backlight, and CPU and GPU usage.
/// Spawns no processes. CPU usage needs two calls on the same sampler, so the first
/// call leaves it unset.
pub fn refresh_live(info: &mut SysInfo, sampler: &mut CpuSampler) {
    put(
        info,
        Field::Uptime,
        system::uptime_secs().map(system::format_uptime),
    );
    put(info, Field::Load, system::load());
    put(
        info,
        Field::Processes,
        procs::count().map(|n| n.to_string()),
    );

    let (memory, swap) = hardware::memory();
    info.gauges.memory = memory;
    put(info, Field::Memory, memory.map(format_bytes_pair));
    info.gauges.swap = swap;
    put(info, Field::Swap, swap.map(format_bytes_pair));
    let disk = hardware::disk();
    info.gauges.disk = disk;
    put(info, Field::Disk, disk.map(format_bytes_pair));

    let battery = power::battery();
    info.gauges.battery = battery.as_ref().map(|battery| battery.level);
    put(
        info,
        Field::Battery,
        battery.as_ref().map(power::Battery::text),
    );

    let cpu_temp = hardware::cpu_temp();
    info.gauges.cpu_temp = cpu_temp;
    put(info, Field::CpuTemp, cpu_temp.map(format_celsius));

    let brightness = power::brightness();
    info.gauges.brightness = brightness;
    put(info, Field::Brightness, brightness.map(format_percent));

    let cpu = sampler.sample();
    info.gauges.cpu = cpu;
    put(info, Field::CpuUsage, cpu.map(format_percent));

    let gpu = hardware::gpu_busy();
    info.gauges.gpu = gpu;
    put(info, Field::GpuUsage, gpu.map(format_percent));
}

/// `"6.21 GiB / 30.60 GiB (20%)"`. Binary units with two decimals; the percentage is rounded.
pub fn format_bytes_pair(usage: Usage) -> String {
    format!(
        "{} / {} ({:.0}%)",
        format_size(usage.used),
        format_size(usage.total),
        usage.ratio() * 100.0
    )
}

fn format_size(bytes: u64) -> String {
    if bytes >= TIB {
        format!("{:.2} TiB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_percent(ratio: f64) -> String {
    format!("{:.0}%", ratio * 100.0)
}

fn format_celsius(celsius: f64) -> String {
    format!("{celsius:.0}°C")
}

/// Sets the field to the value, or removes it when there is none.
fn put(info: &mut SysInfo, field: Field, value: Option<impl Into<String>>) {
    match value {
        Some(value) => info.set(field, value),
        None => info.remove(field),
    }
}

/// Home directory from `$HOME`.
pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Trimmed contents of a small text file. `None` when it is missing or empty.
pub(super) fn read_text(path: impl AsRef<Path>) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Non-empty, trimmed environment variable.
pub(super) fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Entries of a directory whose names pass `keep`, sorted by path. Empty when unreadable.
pub(super) fn sorted_dir(path: impl AsRef<Path>, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| keep(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
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
    fn format_bytes_pair_uses_binary_units() {
        let usage = Usage {
            used: 512 * MIB,
            total: 2 * GIB,
        };
        assert_eq!(format_bytes_pair(usage), "512.00 MiB / 2.00 GiB (25%)");
        let usage = Usage {
            used: 6 * GIB + GIB / 100 * 21,
            total: 30 * GIB + GIB / 10 * 6,
        };
        assert_eq!(format_bytes_pair(usage), "6.21 GiB / 30.60 GiB (20%)");
        let disk = Usage {
            used: 0,
            total: TIB + TIB / 100 * 82,
        };
        assert_eq!(format_bytes_pair(disk), "0 B / 1.82 TiB (0%)");
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
    fn collect_and_refresh_do_not_panic() {
        let mut info = collect();
        let mut sampler = CpuSampler::default();
        refresh_live(&mut info, &mut sampler);
        refresh_live(&mut info, &mut sampler);
    }
}
