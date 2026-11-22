//! Windows detection through Win32 calls and read-only registry values. Nothing here
//! spawns a process, queries WMI or waits on the system.

mod desktop;
mod hardware;
mod network;
mod packages;
mod perf;
mod power;
mod process;
mod registry;
mod system;

use crate::field::{Field, Needed};

use super::{
    format_bytes, format_bytes_pair, format_percent, format_uptime, put, Battery, CpuSampler,
    SysInfo, Usage,
};

pub(super) use hardware::cpu_totals;

/// Windows 8 and later always run this compositor; there is nothing to detect.
const DESKTOP_WINDOW_MANAGER: &str = "DWM";

pub(super) fn collect(info: &mut SysInfo, needed: Needed) {
    let needs_procs = needed.any(&[Field::Processes, Field::Shell, Field::Terminal]);
    let needs_packages = needed.contains(Field::Packages);

    std::thread::scope(|scope| {
        let procs = needs_procs.then(|| scope.spawn(process::ProcTable::scan));
        let packages = needs_packages.then(|| scope.spawn(packages::collect));

        if needed.contains(Field::User) {
            put(info, Field::User, system::user());
        }
        if needed.contains(Field::Host) {
            put(info, Field::Host, system::host());
        }
        if needed.contains(Field::Device) {
            put(info, Field::Device, system::device());
        }
        let (os, os_ids) = system::os();
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
        if needed.contains(Field::Cpu) {
            put(info, Field::Cpu, hardware::cpu());
        }
        if needed.any(&[Field::Memory, Field::Swap]) {
            let (memory, swap) = hardware::memory();
            info.gauges.memory = memory;
            info.gauges.swap = swap;
            if needed.contains(Field::Memory) {
                put(info, Field::Memory, memory.map(format_bytes_pair));
            }
            if needed.contains(Field::Swap) {
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
        if needed.contains(Field::Gpu) {
            put(info, Field::Gpu, hardware::gpu_name());
        }
        if needed.contains(Field::Vram) {
            let vram = hardware::vram_total();
            let vram_usage = vram.and_then(|total| {
                perf::dedicated_vram_used().map(|used| Usage {
                    used: used.min(total),
                    total,
                })
            });
            info.gauges.vram = vram_usage;
            put(
                info,
                Field::Vram,
                vram_usage
                    .map(format_bytes_pair)
                    .or_else(|| vram.map(format_bytes)),
            );
        }
        if needed.contains(Field::Resolution) {
            put(info, Field::Resolution, desktop::resolution());
        }
        if needed.contains(Field::LocalIp) {
            put(info, Field::LocalIp, network::local_ip());
        }
        if needed.contains(Field::Wm) {
            put(info, Field::Wm, Some(DESKTOP_WINDOW_MANAGER));
        }
        if let Some(packages) = packages {
            put(info, Field::Packages, packages.join().unwrap_or_default());
        }

        if let Some(procs) = procs {
            let procs = procs.join().unwrap_or_default();
            if needed.contains(Field::Processes) {
                put(info, Field::Processes, procs.count().map(|n| n.to_string()));
            }
            if needed.any(&[Field::Shell, Field::Terminal]) {
                let chain = procs
                    .parent_of(std::process::id())
                    .map(|parent| procs.chain(parent))
                    .unwrap_or_default();
                put(info, Field::Shell, process::shell(&chain));
                put(info, Field::Terminal, process::terminal(&chain));
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
    if needed.contains(Field::Processes) {
        let procs = process::ProcTable::scan();
        put(info, Field::Processes, procs.count().map(|n| n.to_string()));
    }

    if needed.any(&[Field::Memory, Field::Swap]) {
        let (memory, swap) = hardware::memory();
        info.gauges.memory = memory;
        info.gauges.swap = swap;
        put(info, Field::Memory, memory.map(format_bytes_pair));
        put(info, Field::Swap, swap.map(format_bytes_pair));
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

    if needed.contains(Field::CpuUsage) {
        let cpu = sampler.sample();
        info.gauges.cpu = cpu;
        put(info, Field::CpuUsage, cpu.map(format_percent));
    }
}

/// UTF-16 copy of `text` with a terminating NUL, for the wide-string Win32 parameters.
fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Text of a UTF-16 buffer up to its first NUL.
fn from_wide(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}
