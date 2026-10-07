//! Linux detection from `/proc`, `/sys`, the environment and a few libc calls. A package
//! manager or Wi-Fi tool is spawned only when its database or interface exists.

mod desktop;
mod hardware;
mod network;
mod packages;
mod power;
mod procs;
mod system;

use std::fs;
use std::path::{Path, PathBuf};

pub(super) use hardware::cpu_totals;

use super::{format_bytes_pair, format_percent, format_uptime, put, Battery, CpuSampler, SysInfo};
use crate::field::Field;

/// Detects the system and stores the results in `info`. The slow and independent
/// sources run on scoped threads while the rest is read on the calling thread.
pub(super) fn collect(info: &mut SysInfo) {
    let parent = std::os::unix::process::parent_id();

    std::thread::scope(|scope| {
        let procs = scope.spawn(procs::ProcTable::scan);
        let packages = scope.spawn(packages::collect);
        let gpu = scope.spawn(hardware::gpu_name);
        let wifi = scope.spawn(network::wifi);
        let font = scope.spawn(desktop::font);

        put(info, Field::User, system::user());
        put(info, Field::Host, system::host());
        put(info, Field::Device, system::device());
        let (os, os_ids) = system::os_release();
        put(info, Field::Os, os);
        info.os_ids = os_ids;
        put(info, Field::Kernel, system::kernel());
        put(info, Field::Arch, system::arch());
        put(
            info,
            Field::Uptime,
            system::uptime_secs().map(format_uptime),
        );
        put(info, Field::Locale, system::locale());
        put(info, Field::Load, system::load());
        put(info, Field::Cpu, hardware::cpu());
        let cpu_temp = hardware::cpu_temp();
        info.gauges.cpu_temp = cpu_temp;
        put(info, Field::CpuTemp, cpu_temp.map(format_celsius));
        let (memory, swap) = hardware::memory();
        info.gauges.memory = memory;
        put(info, Field::Memory, memory.map(format_bytes_pair));
        info.gauges.swap = swap;
        put(info, Field::Swap, swap.map(format_bytes_pair));
        let disk = hardware::disk();
        info.gauges.disk = disk;
        put(info, Field::Disk, disk.map(format_bytes_pair));
        let vram = hardware::vram();
        info.gauges.vram = vram;
        put(info, Field::Vram, vram.map(format_bytes_pair));
        let battery = power::battery();
        info.gauges.battery = battery.as_ref().map(|battery| battery.level);
        put(info, Field::Battery, battery.as_ref().map(Battery::text));
        let brightness = power::brightness();
        info.gauges.brightness = brightness;
        put(info, Field::Brightness, brightness.map(format_percent));
        put(info, Field::Resolution, desktop::resolution());
        put(info, Field::LocalIp, network::local_ip());
        put(
            info,
            Field::Processes,
            procs::count().map(|n| n.to_string()),
        );

        let packages = packages.join().unwrap_or_default();
        put(info, Field::Packages, packages.summary);
        put(
            info,
            Field::Flatpak,
            packages.flatpak.map(|n| n.to_string()),
        );
        put(info, Field::Snap, packages.snap.map(|n| n.to_string()));
        put(info, Field::Gpu, gpu.join().unwrap_or_default());
        let wifi = wifi.join().unwrap_or_default();
        put(info, Field::Wifi, wifi.as_ref().map(network::Wifi::text));
        put(info, Field::Font, font.join().unwrap_or_default());

        let procs = procs.join().unwrap_or_default();
        let chain = procs.chain(parent);
        let comms = procs.comms();
        put(info, Field::Shell, desktop::shell(&chain));
        put(info, Field::Terminal, desktop::terminal(&chain));
        put(info, Field::De, desktop::de(&comms));
        put(info, Field::Wm, desktop::wm(&comms));
    });
}

/// Re-reads the values that change between frames. Spawns no processes.
pub(super) fn refresh(info: &mut SysInfo, sampler: &mut CpuSampler) {
    put(
        info,
        Field::Uptime,
        system::uptime_secs().map(format_uptime),
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
    put(info, Field::Battery, battery.as_ref().map(Battery::text));

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

fn format_celsius(celsius: f64) -> String {
    format!("{celsius:.0}°C")
}

/// Home directory from `$HOME`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Trimmed contents of a small text file. `None` when it is missing or empty.
fn read_text(path: impl AsRef<Path>) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Entries of a directory whose names pass `keep`, sorted by path. Empty when unreadable.
fn sorted_dir(path: impl AsRef<Path>, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
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
