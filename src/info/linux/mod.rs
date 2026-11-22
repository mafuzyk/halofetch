//! Linux detection from `/proc`, `/sys`, the environment and a few libc calls. A package
//! manager or Wi-Fi tool is spawned only when its database or interface exists.

mod command;
mod desktop;
mod hardware;
mod network;
mod nix;
mod packages;
mod power;
mod procs;
mod system;

use std::fs;
use std::path::{Path, PathBuf};

pub(super) use hardware::cpu_totals;

use super::{format_bytes_pair, format_percent, format_uptime, put, Battery, CpuSampler, SysInfo};
use crate::field::{Field, Needed};

/// Detects the system and stores the results in `info`. The slow and independent
/// sources run on scoped threads while the rest is read on the calling thread.
pub(super) fn collect(info: &mut SysInfo, needed: Needed) {
    let parent = std::os::unix::process::parent_id();
    let needs_procs = needed.any(&[
        Field::Shell,
        Field::Terminal,
        Field::Font,
        Field::De,
        Field::Wm,
    ]);
    let needs_terminal = needed.any(&[Field::Terminal, Field::Font]);
    let needs_packages = needed.any(&[Field::Packages, Field::Flatpak, Field::Snap]);

    std::thread::scope(|scope| {
        let procs = needs_procs.then(|| scope.spawn(procs::ProcTable::scan));
        let packages = needs_packages.then(|| scope.spawn(|| packages::collect(needed)));
        let gpu = needed
            .contains(Field::Gpu)
            .then(|| scope.spawn(hardware::gpu_name));
        let wifi = needed
            .contains(Field::Wifi)
            .then(|| scope.spawn(network::wifi));

        if needed.contains(Field::User) {
            put(info, Field::User, system::user());
        }
        if needed.contains(Field::Host) {
            put(info, Field::Host, system::host());
        }
        if needed.contains(Field::Device) {
            put(info, Field::Device, system::device());
        }
        let (os, os_ids) = system::os_release();
        if needed.contains(Field::Os) {
            put(info, Field::Os, os);
        }
        info.os_ids = os_ids;
        if needed.contains(Field::Kernel) {
            put(info, Field::Kernel, system::kernel());
        }
        if needed.contains(Field::Arch) {
            put(info, Field::Arch, system::arch());
        }
        if needed.contains(Field::Uptime) {
            put(
                info,
                Field::Uptime,
                system::uptime_secs().map(format_uptime),
            );
        }
        if needed.contains(Field::Locale) {
            put(info, Field::Locale, system::locale());
        }
        if needed.contains(Field::Load) {
            put(info, Field::Load, system::load());
        }
        if needed.contains(Field::Cpu) {
            put(info, Field::Cpu, hardware::cpu());
        }
        if needed.contains(Field::CpuTemp) {
            let cpu_temp = hardware::cpu_temp();
            info.gauges.cpu_temp = cpu_temp;
            put(info, Field::CpuTemp, cpu_temp.map(format_celsius));
        }
        if needed.any(&[Field::Memory, Field::Swap]) {
            let (memory, swap) = hardware::memory();
            if needed.contains(Field::Memory) {
                info.gauges.memory = memory;
                put(info, Field::Memory, memory.map(format_bytes_pair));
            }
            if needed.contains(Field::Swap) {
                info.gauges.swap = swap;
                put(info, Field::Swap, swap.map(format_bytes_pair));
            }
        }
        if needed.contains(Field::Disk) {
            let disk = hardware::disk();
            info.gauges.disk = disk;
            put(info, Field::Disk, disk.map(format_bytes_pair));
        }
        if needed.contains(Field::Vram) {
            let vram = hardware::vram();
            info.gauges.vram = vram;
            put(info, Field::Vram, vram.map(format_bytes_pair));
        }
        if needed.contains(Field::Battery) {
            let battery = power::battery();
            info.gauges.battery = battery.as_ref().map(|battery| battery.level);
            put(info, Field::Battery, battery.as_ref().map(Battery::text));
        }
        if needed.contains(Field::Brightness) {
            let brightness = power::brightness();
            info.gauges.brightness = brightness;
            put(info, Field::Brightness, brightness.map(format_percent));
        }
        if needed.contains(Field::Resolution) {
            put(info, Field::Resolution, desktop::resolution());
        }
        if needed.contains(Field::LocalIp) {
            put(info, Field::LocalIp, network::local_ip());
        }
        if needed.contains(Field::Processes) {
            put(
                info,
                Field::Processes,
                procs::count().map(|n| n.to_string()),
            );
        }

        if let Some(packages) = packages {
            let packages = packages.join().unwrap_or_default();
            if needed.contains(Field::Packages) {
                put(info, Field::Packages, packages.summary);
            }
            if needed.contains(Field::Flatpak) {
                put(
                    info,
                    Field::Flatpak,
                    packages.flatpak.map(|n| n.to_string()),
                );
            }
            if needed.contains(Field::Snap) {
                put(info, Field::Snap, packages.snap.map(|n| n.to_string()));
            }
        }
        if let Some(gpu) = gpu {
            put(info, Field::Gpu, gpu.join().unwrap_or_default());
        }
        if let Some(wifi) = wifi {
            let wifi = wifi.join().unwrap_or_default();
            put(info, Field::Wifi, wifi.as_ref().map(network::Wifi::text));
        }

        if let Some(procs) = procs {
            let procs = procs.join().unwrap_or_default();
            if needed.any(&[
                Field::Shell,
                Field::Terminal,
                Field::Font,
                Field::De,
                Field::Wm,
            ]) {
                let chain = procs.chain(parent);
                let comms = procs.comms();
                if needed.contains(Field::Shell) {
                    put(info, Field::Shell, desktop::shell(&chain));
                }
                let terminal = needs_terminal.then(|| desktop::terminal(&chain)).flatten();
                if needed.contains(Field::Terminal) {
                    put(info, Field::Terminal, terminal.clone());
                }
                if needed.contains(Field::Font) {
                    put(info, Field::Font, desktop::font(terminal.as_deref()));
                }
                if needed.contains(Field::De) {
                    put(info, Field::De, desktop::de(&comms));
                }
                if needed.contains(Field::Wm) {
                    put(info, Field::Wm, desktop::wm(&comms));
                }
            }
        }
    });
}

/// Re-reads the values that change between frames. Spawns no processes.
pub(super) fn refresh(info: &mut SysInfo, sampler: &mut CpuSampler, needed: Needed) {
    if needed.contains(Field::Uptime) {
        put(
            info,
            Field::Uptime,
            system::uptime_secs().map(format_uptime),
        );
    }
    if needed.contains(Field::Load) {
        put(info, Field::Load, system::load());
    }
    if needed.contains(Field::Processes) {
        put(
            info,
            Field::Processes,
            procs::count().map(|n| n.to_string()),
        );
    }

    if needed.any(&[Field::Memory, Field::Swap]) {
        let (memory, swap) = hardware::memory();
        if needed.contains(Field::Memory) {
            info.gauges.memory = memory;
            put(info, Field::Memory, memory.map(format_bytes_pair));
        }
        if needed.contains(Field::Swap) {
            info.gauges.swap = swap;
            put(info, Field::Swap, swap.map(format_bytes_pair));
        }
    }
    if needed.contains(Field::Disk) {
        let disk = hardware::disk();
        info.gauges.disk = disk;
        put(info, Field::Disk, disk.map(format_bytes_pair));
    }
    if needed.contains(Field::Battery) {
        let battery = power::battery();
        info.gauges.battery = battery.as_ref().map(|battery| battery.level);
        put(info, Field::Battery, battery.as_ref().map(Battery::text));
    }
    if needed.contains(Field::CpuTemp) {
        let cpu_temp = hardware::cpu_temp();
        info.gauges.cpu_temp = cpu_temp;
        put(info, Field::CpuTemp, cpu_temp.map(format_celsius));
    }
    if needed.contains(Field::Brightness) {
        let brightness = power::brightness();
        info.gauges.brightness = brightness;
        put(info, Field::Brightness, brightness.map(format_percent));
    }
    if needed.contains(Field::CpuUsage) {
        let cpu = sampler.sample();
        info.gauges.cpu = cpu;
        put(info, Field::CpuUsage, cpu.map(format_percent));
    }
    if needed.contains(Field::GpuUsage) {
        let gpu = hardware::gpu_busy();
        info.gauges.gpu = gpu;
        put(info, Field::GpuUsage, gpu.map(format_percent));
    }
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
