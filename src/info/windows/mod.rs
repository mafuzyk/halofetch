//! Windows detection through Win32 calls and read-only registry values. Nothing here
//! spawns a process, queries WMI or waits on the system.

mod desktop;
mod hardware;
mod network;
mod packages;
mod power;
mod process;
mod registry;
mod system;

use crate::field::Field;

use super::{format_bytes_pair, format_percent, format_uptime, put, Battery, CpuSampler, SysInfo};

pub(super) use hardware::cpu_totals;

/// Windows 8 and later always run this compositor; there is nothing to detect.
const DESKTOP_WINDOW_MANAGER: &str = "DWM";

pub(super) fn collect(info: &mut SysInfo) {
    std::thread::scope(|scope| {
        let procs = scope.spawn(process::ProcTable::scan);
        let packages = scope.spawn(packages::collect);

        put(info, Field::User, system::user());
        put(info, Field::Host, system::host());
        put(info, Field::Device, system::device());
        let (os, os_ids) = system::os();
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
        put(info, Field::Cpu, hardware::cpu());
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
        put(info, Field::Gpu, hardware::gpu_name());
        put(info, Field::Resolution, desktop::resolution());
        put(info, Field::LocalIp, network::local_ip());
        put(info, Field::Wm, Some(DESKTOP_WINDOW_MANAGER));
        put(info, Field::Packages, packages.join().unwrap_or_default());

        let procs = procs.join().unwrap_or_default();
        put(info, Field::Processes, procs.count().map(|n| n.to_string()));
        let chain = procs
            .parent_of(std::process::id())
            .map(|parent| procs.chain(parent))
            .unwrap_or_default();
        put(info, Field::Shell, process::shell(&chain));
        put(info, Field::Terminal, process::terminal(&chain));
    });
}

/// Re-reads the values that change between frames. Spawns no processes.
pub(super) fn refresh(info: &mut SysInfo, sampler: &mut CpuSampler) {
    put(
        info,
        Field::Uptime,
        system::uptime_secs().map(format_uptime),
    );
    let procs = process::ProcTable::scan();
    put(info, Field::Processes, procs.count().map(|n| n.to_string()));

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

    let cpu = sampler.sample();
    info.gauges.cpu = cpu;
    put(info, Field::CpuUsage, cpu.map(format_percent));
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
