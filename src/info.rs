// System information collection.
//
// Reads directly from /proc, /sys, and environment variables — no external
// commands or libraries. This keeps startup fast and the binary small.
// Every field is stored as a Display string so the renderer never has to
// format mid-render.

use color_eyre::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

static CPU_CORE_SUFFIX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\s+\d+-Core(?:\s+Processor|\s+APU)?$")
        .expect("CPU suffix regex is a compile-time constant")
});
static CPU_TYPE_SUFFIX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\s+(?:Processor|APU)$").expect("CPU type regex is a compile-time constant")
});
static GPU_TYPE_SUFFIX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\s+(?:Series|Graphics)$")
        .expect("GPU suffix regex is a compile-time constant")
});

#[derive(Debug, Clone, Default, Serialize)]
pub struct SysInfo {
    pub os: String,
    pub host: String,
    pub user: String,
    pub kernel: String,
    pub uptime: String,
    pub packages: String,
    pub shell: String,
    pub terminal: String,
    pub cpu: String,
    pub gpu: String,
    pub memory: String,
    pub disk: String,
    pub wm: String,
    pub load: String,
    pub processes: String,
    pub local_ip: String,
    pub resolution: String,
    pub de: String,
    pub font: String,
    pub vram: String,
    pub flatpak: String,
    pub snap: String,
    pub device: String,
    pub rom: String,
    pub soc: String,
    pub arch: String,
    pub battery_level: String,
    pub battery_temp: String,
    pub battery_health: String,
    pub battery_status: String,
    pub root_status: String,
    pub bootloader: String,
    pub selinux: String,
    pub storage: String,
    pub cpu_temp: String,
    pub brightness: String,
    pub refresh_rate: String,
    pub signal: String,
    pub wifi_ssid: String,
    pub security_patch: String,
    pub uptime_days: String,
}

impl SysInfo {
    pub fn get(&self, field: &str) -> Option<&str> {
        match field {
            "os" => Some(&self.os),
            "host" => Some(&self.host),
            "user" => Some(&self.user),
            "kernel" => Some(&self.kernel),
            "uptime" => Some(&self.uptime),
            "packages" => Some(&self.packages),
            "shell" => Some(&self.shell),
            "terminal" => Some(&self.terminal),
            "cpu" => Some(&self.cpu),
            "gpu" => Some(&self.gpu),
            "memory" => Some(&self.memory),
            "disk" => Some(&self.disk),
            "wm" => Some(&self.wm),
            "load" => Some(&self.load),
            "processes" => Some(&self.processes),
            "local_ip" => Some(&self.local_ip),
            "resolution" => Some(&self.resolution),
            "de" => Some(&self.de),
            "font" => Some(&self.font),
            "vram" => Some(&self.vram),
            "flatpak" => Some(&self.flatpak),
            "snap" => Some(&self.snap),
            "device" => Some(&self.device),
            "rom" => Some(&self.rom),
            "soc" => Some(&self.soc),
            "arch" => Some(&self.arch),
            "battery_level" => Some(&self.battery_level),
            "battery_temp" => Some(&self.battery_temp),
            "battery_health" => Some(&self.battery_health),
            "battery_status" => Some(&self.battery_status),
            "root_status" => Some(&self.root_status),
            "bootloader" => Some(&self.bootloader),
            "selinux" => Some(&self.selinux),
            "storage" => Some(&self.storage),
            "cpu_temp" => Some(&self.cpu_temp),
            "brightness" => Some(&self.brightness),
            "refresh_rate" => Some(&self.refresh_rate),
            "signal" => Some(&self.signal),
            "wifi_ssid" => Some(&self.wifi_ssid),
            "security_patch" => Some(&self.security_patch),
            "uptime_days" => Some(&self.uptime_days),
            _ => None,
        }
    }
}

pub fn collect() -> Result<SysInfo> {
    let uptime = format_uptime();
    let info = SysInfo {
        user: std::env::var("USER").unwrap_or_else(|_| whoami_fallback()),
        host: hostname(),
        os: detect_os(),
        kernel: read_kernel(),
        uptime,
        shell: detect_shell(),
        terminal: detect_terminal(),
        cpu: read_cpu(),
        gpu: read_gpu(),
        memory: format_memory(),
        disk: format_disk("/"),
        wm: detect_wm(),
        load: read_load(),
        processes: count_processes(),
        packages: count_packages(),
        local_ip: local_ip(),
        resolution: detect_resolution(),
        de: detect_de(),
        font: detect_font(),
        vram: read_vram(),
        flatpak: count_flatpak(),
        snap: count_snap(),
        arch: read_arch(),
        device: read_device_model(),
        battery_level: read_battery_level(),
        battery_temp: read_battery_temp(),
        battery_health: read_battery_health(),
        battery_status: read_battery_status(),
        cpu_temp: read_cpu_temp(),
        brightness: read_brightness(),
        refresh_rate: read_refresh_rate(),
        signal: read_signal(),
        wifi_ssid: read_wifi_ssid(),
        ..SysInfo::default()
    };
    Ok(info)
}

/// Refresh fields that can materially change during a live monitor session.
/// Expensive stable discovery (packages, GPU model, fonts, desktop) stays out
/// of the hot path.
pub fn refresh_live(info: &mut SysInfo) {
    info.uptime = format_uptime();
    info.memory = format_memory();
    info.disk = format_disk("/");
    info.load = read_load();
    info.processes = count_processes();
    info.battery_level = read_battery_level();
    info.battery_temp = read_battery_temp();
    info.battery_health = read_battery_health();
    info.battery_status = read_battery_status();
    info.cpu_temp = read_cpu_temp();
}

// ── OS detection ─────────────────────────────────────────────────────────

fn detect_os() -> String {
    for path in &["/etc/os-release", "/usr/lib/os-release"] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                    return val.trim_matches('"').to_string();
                }
            }
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("NAME=") {
                    let name = val.trim_matches('"').to_string();
                    if let Some(ver) = content.lines().find_map(|l| l.strip_prefix("VERSION_ID=")) {
                        return format!("{} {}", name, ver.trim_matches('"'));
                    }
                    return name;
                }
            }
        }
    }
    "Linux".into()
}

// ── Hostname ─────────────────────────────────────────────────────────────

fn hostname() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "localhost".into())
}

// ── Whoami fallback ──────────────────────────────────────────────────────

fn whoami_fallback() -> String {
    fs::read_to_string("/proc/self/uid_map")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(|s| s.to_string()))
        .unwrap_or_else(|| "user".into())
}

// ── Kernel ───────────────────────────────────────────────────────────────

fn read_kernel() -> String {
    fs::read_to_string("/proc/version")
        .ok()
        .map(|s| s.split_whitespace().nth(2).unwrap_or("unknown").to_string())
        .unwrap_or_else(|| "unknown".into())
}

// ── Uptime ───────────────────────────────────────────────────────────────

fn format_uptime() -> String {
    let secs = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;

    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{}d", d));
    }
    if h > 0 {
        parts.push(format!("{}h", h));
    }
    parts.push(format!("{}m", m));
    parts.join(" ")
}

// ── Shell ────────────────────────────────────────────────────────────────

fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.rsplit('/').next().map(|s| s.to_string()))
        .unwrap_or_else(|| "sh".into())
}

// ── Terminal ─────────────────────────────────────────────────────────────

fn detect_terminal() -> String {
    std::env::var("TERM").unwrap_or_else(|_| "unknown".into())
}

// ── CPU ──────────────────────────────────────────────────────────────────

fn read_cpu() -> String {
    let content = match fs::read_to_string("/proc/cpuinfo") {
        Ok(c) => c,
        Err(_) => return "unknown".into(),
    };

    let mut model = String::new();
    let mut cores = 0u32;

    // On ARM systems, "Hardware" line contains the SoC name
    // On x86, "model name" contains the CPU model
    // Also try "Processor" (ARM)
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("model name") {
            if let Some(name) = val.split(':').nth(1) {
                model = name.trim().to_string();
            }
        }
        if model.is_empty() {
            if let Some(val) = line.strip_prefix("Hardware") {
                if let Some(name) = val.split(':').nth(1) {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && trimmed != "UNKNOWN" {
                        model = trimmed;
                    }
                }
            }
        }
        if model.is_empty() {
            if let Some(val) = line.strip_prefix("Processor") {
                if let Some(name) = val.split(':').nth(1) {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && trimmed != "UNKNOWN" {
                        model = trimmed;
                    }
                }
            }
        }
        if line.starts_with("processor") {
            cores += 1;
        }
    }

    if model.is_empty() {
        return "unknown".into();
    }

    // Simplify CPU name: remove trademark symbols, "CPU" suffix, @ speed
    let simplified = model
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(r)", "")
        .replace("(tm)", "")
        .replace(" CPU", "");

    // Remove trailing @ speed
    let simplified = simplified
        .split(" @ ")
        .next()
        .unwrap_or(&simplified)
        .trim()
        .to_string();

    let shortened = shorten_cpu(&simplified);

    if cores > 1 {
        format!("{} ({} cores)", shortened, cores)
    } else {
        shortened
    }
}

/// Shorten a CPU name to its meaningful model identifier.
/// Removes verbose suffixes like "with ...", "N-Core Processor", etc.
fn shorten_cpu(name: &str) -> String {
    let name = name.trim();

    // Strip " with ..." (e.g., "AMD Ryzen 3 2200G with Radeon Vega Graphics")
    if let Some(pos) = name.find(" with ") {
        return name[..pos].trim().to_string();
    }

    // Strip trailing "N-Core Processor", "N-Core APU", or just "N-Core"
    // e.g., "AMD Ryzen 5 5600X 6-Core Processor" → "AMD Ryzen 5 5600X"
    let name = CPU_CORE_SUFFIX.replace(name, "");

    // Strip trailing " Processor" or " APU" (left after Core removal)
    let name = CPU_TYPE_SUFFIX.replace(&name, "");

    name.trim().to_string()
}

/// Shorten a GPU name to its meaningful model identifier.
fn shorten_gpu(name: &str) -> String {
    let name = name.trim();

    // NVIDIA: "NVIDIA GeForce RTX 3060" → "NVIDIA RTX 3060"
    let name = name.replace("GeForce ", "");
    // AMD: "AMD Radeon RX 570 Series" → "AMD RX 570"
    let name = name.replace("Radeon ", "");
    // Strip trailing " Series", " Graphics"
    let name = GPU_TYPE_SUFFIX.replace(&name, "");

    name.trim().to_string()
}

// ── GPU ──────────────────────────────────────────────────────────────────

fn read_gpu() -> String {
    // Try reading from DRM devices
    let drm_path = Path::new("/sys/class/drm");
    if let Ok(entries) = fs::read_dir(drm_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.contains("render") || !name_str.starts_with("card") {
                continue;
            }
            let dev_path = entry.path().join("device");

            // Try vendor/device from uevent
            let uevent_path = dev_path.join("uevent");
            if let Ok(uevent) = fs::read_to_string(&uevent_path) {
                let mut vendor = String::new();
                let mut device = String::new();
                for line in uevent.lines() {
                    if let Some(v) = line.strip_prefix("DRIVER=") {
                        vendor = v.to_string();
                    }
                    if vendor == "amdgpu" {
                        // Try product_name first (newer kernels)
                        let gpu_name = fs::read_to_string(dev_path.join("product_name"))
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "AMD Radeon".into());
                        return shorten_gpu(&gpu_name);
                    }
                    if vendor == "nvidia" {
                        // Try to get the model name
                        if let Ok(model) = fs::read_to_string(dev_path.join("model")) {
                            return shorten_gpu(model.trim());
                        }
                        return "NVIDIA".into();
                    }
                    if line.starts_with("MODALIAS") && device.is_empty() {
                        // Extract PCI ID from modalias
                        if let Some(pci_id) = line.split("pci:v").nth(1) {
                            let dev_info: Vec<&str> = pci_id.split('d').collect();
                            if dev_info.len() >= 2 {
                                let vendor_id = &dev_info[0][..4];
                                let _device_id = dev_info[1].chars().take(4).collect::<String>();
                                // Map known vendors
                                device = match vendor_id {
                                    "1002" | "1022" => "AMD".into(),
                                    "10de" => "NVIDIA".into(),
                                    "8086" => "Intel".into(),
                                    _ => format!("PCI:{}", vendor_id),
                                };
                            }
                        }
                    }
                }
                if !device.is_empty() {
                    return device;
                }
            }

            // Fallback: read class name
            if let Ok(class) = fs::read_to_string(dev_path.join("class")) {
                let trimmed = class.trim();
                if trimmed.contains("0300") || trimmed.contains("0302") {
                    // It's a VGA/3D controller
                    if let Ok(vendor) = fs::read_to_string(dev_path.join("vendor")) {
                        let v = vendor.trim();
                        return match v {
                            "0x1002" | "0x1022" => "AMD".into(),
                            "0x10de" => "NVIDIA".into(),
                            "0x8086" => "Intel".into(),
                            _ => format!("GPU (0x{})", &v[2..6]),
                        };
                    }
                }
            }
        }
    }

    if Path::new("/sys/class/kgsl/kgsl-3d0").exists() {
        // Qualcomm Adreno via KGSL
        let kgsl_path = Path::new("/sys/class/kgsl/kgsl-3d0");
        if kgsl_path.exists() {
            if let Ok(model) = fs::read_to_string(kgsl_path.join("gpu_model")) {
                let trimmed = model.trim().to_string();
                if !trimmed.is_empty() {
                    return format!("Adreno {}", trimmed);
                }
            }
            if let Ok(name) = fs::read_to_string(kgsl_path.join("device_name")) {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() {
                    return format!("Adreno {}", trimmed);
                }
            }
            // Some devices expose the GPU clock as a hint
            if let Ok(gpu_model) = fs::read_to_string(kgsl_path.join("gpu_clk")) {
                let _ = gpu_model;
            }
        }
        // ARM Mali — /sys/kernel/gpu
        let mali_sys = Path::new("/sys/kernel/gpu");
        if mali_sys.exists() {
            if let Ok(model) = fs::read_to_string(mali_sys.join("gpu_model")) {
                let trimmed = model.trim().to_string();
                if !trimmed.is_empty() {
                    return format!("Mali {}", trimmed);
                }
            }
        }
        // ARM Mali — platform devices
        let mali_path = Path::new("/sys/devices/platform");
        if let Ok(entries) = fs::read_dir(mali_path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.contains("mali") {
                    if let Ok(gpuinfo) = fs::read_to_string(entry.path().join("gpuinfo")) {
                        let trimmed = gpuinfo.trim().to_string();
                        if !trimmed.is_empty() {
                            return format!("Mali {}", trimmed);
                        }
                    }
                    return "ARM Mali".into();
                }
            }
        }
        // PowerVR
        if let Ok(content) = fs::read_to_string("/proc/gpucrypto") {
            if content.contains("GPU") || content.contains("gpu") {
                return "PowerVR".into();
            }
        }
        // Generic: try /dri symlinks
        let drm_path = Path::new("/sys/class/drm");
        if let Ok(entries) = fs::read_dir(drm_path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("card") && !name_str.contains("-") {
                    let dev_path = entry.path().join("device");
                    if let Ok(vendor) = fs::read_to_string(dev_path.join("vendor")) {
                        let v = vendor.trim().to_string();
                        return match v.as_str() {
                            "0x1002" | "0x1022" => "AMD GPU".into(),
                            "0x10de" => "NVIDIA GPU".into(),
                            "0x8086" => "Intel GPU".into(),
                            "0x13b5" => "ARM Mali".into(),
                            "0x5143" => "Qualcomm Adreno".into(),
                            _ => format!("GPU ({})", v),
                        };
                    }
                }
            }
        }
    }

    "unknown".into()
}

// ── Memory ───────────────────────────────────────────────────────────────

fn format_memory() -> String {
    let content = match fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return "unknown".into(),
    };

    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            total_kb = val
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
        }
        if let Some(val) = line.strip_prefix("MemAvailable:") {
            avail_kb = val
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
        }
    }

    if total_kb == 0 {
        return "unknown".into();
    }

    let used_kb = total_kb.saturating_sub(avail_kb);
    let used = used_kb as f64 / 1_048_576.0;
    let total = total_kb as f64 / 1_048_576.0;

    format!("{:.1}/{:.1}G", used, total)
}

// ── Disk ─────────────────────────────────────────────────────────────────

fn format_disk(mount: &str) -> String {
    #[cfg(target_os = "linux")]
    {
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let cpath = std::ffi::CString::new(mount).unwrap_or_default();
        if unsafe { libc::statvfs(cpath.as_ptr(), &mut stat) } == 0 {
            let total = stat.f_blocks as u64 * stat.f_frsize as u64;
            let free = stat.f_bfree as u64 * stat.f_frsize as u64;
            let used = total.saturating_sub(free);
            let total_g = total as f64 / 1_073_741_824.0;
            let used_g = used as f64 / 1_073_741_824.0;
            return format!("{:.0}/{:.0}G", used_g, total_g);
        }
    }

    // Fallback: read from /proc/mounts and statfs via /sys
    let stat_path = format!("/sys/fs/{}/", mount.trim_start_matches('/'));
    #[allow(unused_variables)]
    let _ = stat_path;

    "unknown".into()
}

// ── WM ───────────────────────────────────────────────────────────────────

fn detect_wm() -> String {
    // Check common environment variables set by WMs
    for (var, name) in &[
        ("XDG_CURRENT_DESKTOP", None),
        ("DESKTOP_SESSION", None),
        ("HYPRLAND_INSTANCE_SIGNATURE", Some("Hyprland")),
        ("SWAYSOCK", Some("Sway")),
        ("I3SOCK", Some("i3")),
        ("QTILE_SOCKET", Some("Qtile")),
        ("AWESOME_CLIENT_INSTANCE", Some("Awesome")),
    ] {
        if let Ok(val) = std::env::var(var) {
            if let Some(fixed) = name {
                return fixed.to_string();
            }
            if !val.is_empty() {
                return val;
            }
        }
    }

    // Try reading from /proc
    if let Ok(proc) = fs::read_dir("/proc") {
        for entry in proc.flatten() {
            let pid = entry.file_name();
            let pid_str = pid.to_string_lossy();
            if let Ok(comm) = fs::read_to_string(format!("/proc/{}/comm", pid_str)) {
                let comm = comm.trim();
                match comm {
                    "Hyprland" => return "Hyprland".into(),
                    "sway" => return "Sway".into(),
                    "i3" => return "i3".into(),
                    "qtile" => return "Qtile".into(),
                    "awesome" => return "Awesome".into(),
                    "bspwm" => return "bspwm".into(),
                    "dwm" => return "dwm".into(),
                    "openbox" => return "Openbox".into(),
                    "fluxbox" => return "Fluxbox".into(),
                    "xfwm4" => return "Xfwm4".into(),
                    "kwin_x11" | "kwin_wayland" => return "KWin".into(),
                    _ => {}
                }
            }
        }
    }

    "unknown".into()
}

// ── Desktop Environment ─────────────────────────────────────────────────

fn detect_de() -> String {
    // Check known DE-specific env vars in priority order
    if let Ok(val) = std::env::var("XDG_CURRENT_DESKTOP") {
        let de = val.trim().to_string();
        if !de.is_empty() {
            return de;
        }
    }
    if let Ok(val) = std::env::var("DESKTOP_SESSION") {
        let de = val.trim().to_string();
        if !de.is_empty() {
            return de;
        }
    }
    if std::env::var("GNOME_DESKTOP_SESSION_ID").is_ok() {
        return "GNOME".into();
    }
    if std::env::var("MATE_DESKTOP_SESSION_ID").is_ok() {
        return "MATE".into();
    }
    if std::env::var("KDE_FULL_SESSION").is_ok() {
        return "KDE".into();
    }
    // Detect via process name matching (same approach as detect_wm)
    let de_procs = &[
        "gnome-shell",
        "plasmashell",
        "xfce4-session",
        "mate-session",
        "lxqt-session",
        "lxpanel",
        "cinnamon-session",
        "budgie-wm",
        "deepin-wm",
        "enlightenment",
        "openbox",
        "fluxbox",
        "i3",
    ];
    if let Ok(proc) = fs::read_dir("/proc") {
        for entry in proc.flatten() {
            let pid = entry.file_name();
            let pid_str = pid.to_string_lossy();
            if let Ok(comm) = fs::read_to_string(format!("/proc/{}/comm", pid_str)) {
                let comm = comm.trim();
                if de_procs.contains(&comm) {
                    match comm {
                        "gnome-shell" => return "GNOME".into(),
                        "plasmashell" => return "KDE".into(),
                        "xfce4-session" => return "Xfce".into(),
                        "mate-session" => return "MATE".into(),
                        "lxqt-session" => return "LXQt".into(),
                        "lxpanel" => return "LXDE".into(),
                        "cinnamon-session" => return "Cinnamon".into(),
                        "budgie-wm" => return "Budgie".into(),
                        "deepin-wm" => return "Deepin".into(),
                        "enlightenment" => return "Enlightenment".into(),
                        _ => return comm.to_string(),
                    }
                }
            }
        }
    }
    String::new()
}

// ── Load ─────────────────────────────────────────────────────────────────

fn read_load() -> String {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(|v| v.to_string()))
        .unwrap_or_else(|| "?".into())
}

// ── Processes ────────────────────────────────────────────────────────────

fn count_processes() -> String {
    let count = fs::read_dir("/proc")
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .chars()
                        .all(|c| c.is_ascii_digit())
                })
                .count()
        })
        .unwrap_or(0);
    count.to_string()
}

// ── Packages ─────────────────────────────────────────────────────────────

fn count_packages() -> String {
    let pacman = Path::new("/var/lib/pacman/local");
    if let Ok(entries) = fs::read_dir(pacman) {
        let count = entries
            .flatten()
            .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
            .count();
        if count > 0 {
            return count.to_string();
        }
    }

    if let Ok(status) = fs::read_to_string("/var/lib/dpkg/status") {
        let count = status
            .lines()
            .filter(|line| *line == "Status: install ok installed")
            .count();
        if count > 0 {
            return count.to_string();
        }
    }

    for (program, arguments) in [
        ("rpm", &["-qa"][..]),
        ("xbps-query", &["-l"][..]),
        ("nix-store", &["-qR", "/run/current-system/sw"][..]),
    ] {
        if let Ok(output) = std::process::Command::new(program).args(arguments).output() {
            if output.status.success() {
                let count = String::from_utf8_lossy(&output.stdout).lines().count();
                if count > 0 {
                    return count.to_string();
                }
            }
        }
    }

    if let Ok(world) = fs::read_to_string("/var/lib/portage/world") {
        let count = world.lines().filter(|line| !line.trim().is_empty()).count();
        if count > 0 {
            return count.to_string();
        }
    }

    "—".into()
}

// ── VRAM ─────────────────────────────────────────────────────────────────

fn read_vram() -> String {
    if Path::new("/sys/class/kgsl/kgsl-3d0").exists() {
        // Qualcomm Adreno via KGSL
        let kgsl_path = Path::new("/sys/class/kgsl/kgsl-3d0");
        if kgsl_path.exists() {
            // Try dedicated VRAM size
            if let Ok(_content) = fs::read_to_string(kgsl_path.join("gpu_freq_table")) {
                // Not VRAM, but worth checking other files
            }
            // Some kernels expose gpubusy which contains busy/total time, not VRAM
            // Try to read meminfo for shared GPU memory
            if let Ok(content) = fs::read_to_string("/proc/meminfo") {
                for line in content.lines() {
                    if let Some(val) = line.strip_prefix("MemTotal:") {
                        let kb: u64 = val
                            .split_whitespace()
                            .next()
                            .unwrap_or("0")
                            .parse()
                            .unwrap_or(0);
                        if kb > 0 {
                            let gb = kb as f64 / 1_048_576.0;
                            return format!("Shared {:.1}G", gb);
                        }
                    }
                }
            }
        }
        return String::new();
    }

    let drm_path = Path::new("/sys/class/drm");
    if let Ok(entries) = fs::read_dir(drm_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("card") || name_str.contains("-") {
                continue;
            }
            let vram_path = entry.path().join("device").join("mem_info_vram_total");
            if let Ok(content) = fs::read_to_string(&vram_path) {
                let bytes: u64 = content.trim().parse().unwrap_or(0);
                if bytes > 0 {
                    let gib = bytes as f64 / 1_073_741_824.0;
                    return format!("{:.1}G", gib);
                }
            }
        }
    }
    String::new()
}

// ── Flatpak count ────────────────────────────────────────────────────────

fn count_flatpak() -> String {
    if let Ok(out) = std::process::Command::new("flatpak")
        .args(["list"])
        .output()
    {
        if out.status.success() {
            let count = String::from_utf8_lossy(&out.stdout).lines().count();
            if count > 0 {
                return count.to_string();
            }
        }
    }
    String::new()
}

// ── Snap count ───────────────────────────────────────────────────────────

fn count_snap() -> String {
    if let Ok(out) = std::process::Command::new("snap").args(["list"]).output() {
        if out.status.success() {
            let count = String::from_utf8_lossy(&out.stdout).lines().count();
            let count = count.saturating_sub(1);
            if count > 0 {
                return count.to_string();
            }
        }
    }
    String::new()
}

// ── Resolution ───────────────────────────────────────────────────────────

fn detect_resolution() -> String {
    // Read the first connected mode exposed by DRM
    let drm_path = Path::new("/sys/class/drm");
    if let Ok(entries) = fs::read_dir(drm_path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.contains("-") || !name_str.starts_with("card") {
                continue;
            }
            if let Ok(modes) = fs::read_to_string(entry.path().join("modes")) {
                for mode in modes.lines() {
                    let mode = mode.trim();
                    if !mode.is_empty() {
                        return mode.to_string();
                    }
                }
            }
        }
    }

    String::new()
}

// ── Local IP ─────────────────────────────────────────────────────────────

fn local_ip() -> String {
    // Connect to a public DNS to discover our local IP.
    // This sends a UDP packet but is the most portable approach.
    use std::net::UdpSocket;
    let sock = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    if sock.connect("8.8.8.8:80").is_err() {
        return String::new();
    }
    match sock.local_addr() {
        Ok(addr) => addr.ip().to_string(),
        Err(_) => String::new(),
    }
}

// ── Font ──────────────────────────────────────────────────────────────────

fn detect_font() -> String {
    // Try terminal-specific config files
    let home = std::env::var("HOME").unwrap_or_default();

    // Kitty
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/kitty/kitty.conf")) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(font) = line.strip_prefix("font_family ") {
                return font.trim().trim_matches('"').to_string();
            }
        }
    }

    // Alacritty (YAML)
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/alacritty/alacritty.yml")) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("family:") {
                return line
                    .strip_prefix("family:")
                    .unwrap()
                    .trim()
                    .trim_matches('"')
                    .to_string();
            }
        }
    }
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/alacritty/alacritty.toml")) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("family =") {
                return line
                    .strip_prefix("family =")
                    .unwrap()
                    .trim()
                    .trim_matches('"')
                    .to_string();
            }
        }
    }

    // WezTerm
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/wezterm/wezterm.lua")) {
        for line in content.lines() {
            let line = line.trim();
            if line.contains("font = wezterm.font") || line.contains("font=") {
                // Extract font name from wezterm.font("Font Name")
                if let Some(start) = line.find('"') {
                    if let Some(end) = line[start + 1..].find('"') {
                        return line[start + 1..start + 1 + end].to_string();
                    }
                }
                if let Some(start) = line.find('\'') {
                    if let Some(end) = line[start + 1..].find('\'') {
                        return line[start + 1..start + 1 + end].to_string();
                    }
                }
            }
        }
    }

    // Ghostty
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/ghostty/config")) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(font) = line.strip_prefix("font-family =") {
                return font.trim().to_string();
            }
        }
    }

    // Foot
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/foot/foot.ini")) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(font) = line.strip_prefix("font=") {
                return font.trim().to_string();
            }
        }
    }

    // Xresources / Xdefaults
    for path in &[format!("{home}/.Xresources"), format!("{home}/.Xdefaults")] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.ends_with(".font:") || line.contains("*font:") {
                    if let Some(font) = line.split(':').nth(1) {
                        return font.trim().to_string();
                    }
                }
            }
        }
    }

    // GNOME Terminal — resolve profile UUID then read font
    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.Terminal.ProfilesList", "default"])
        .output()
    {
        let uuid = String::from_utf8_lossy(&out.stdout)
            .trim()
            .trim_matches('\'')
            .to_string();
        if !uuid.is_empty() {
            let path = format!("/org/gnome/terminal/legacy/profiles:/:{}/", uuid);
            if let Ok(fout) = std::process::Command::new("gsettings")
                .args([
                    "get",
                    &format!("org.gnome.Terminal.Legacy.Profile:{}", path),
                    "font",
                ])
                .output()
            {
                let font = String::from_utf8_lossy(&fout.stdout)
                    .trim()
                    .trim_matches('\'')
                    .to_string();
                if !font.is_empty() && font != "@as" {
                    return font;
                }
            }
        }
    }

    // Konsole
    if let Ok(content) = fs::read_to_string(format!("{home}/.config/konsolerc")) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("Font=") {
                return line.strip_prefix("Font=").unwrap().trim().to_string();
            }
        }
    }

    // Environment variables (some terminals set these)
    for var in &["TERM_FONT", "FONT", "TERMINAL_FONT"] {
        if let Ok(font) = std::env::var(var) {
            if !font.is_empty() {
                return font;
            }
        }
    }

    String::new()
}

// ── Hardware and sensor information ─────────────────────────────────────

fn read_arch() -> String {
    if let Ok(out) = std::process::Command::new("uname").arg("-m").output() {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "unknown".into()
}

fn read_device_model() -> String {
    // PC: DMI product name
    for dmi in &[
        "/sys/class/dmi/id/product_name",
        "/sys/devices/virtual/dmi/id/product_name",
    ] {
        if let Ok(content) = fs::read_to_string(dmi) {
            let name = content.trim().to_string();
            if !name.is_empty() && name != "System Product Name" && name != "To Be Filled By O.E.M."
            {
                return name;
            }
        }
    }
    // ARM/Linux: getprop
    if let Ok(out) = std::process::Command::new("getprop")
        .arg("ro.product.model")
        .output()
    {
        if out.status.success() {
            let model = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !model.is_empty() {
                return model;
            }
        }
    }
    if let Ok(out) = std::process::Command::new("getprop")
        .arg("ro.product.marketname")
        .output()
    {
        if out.status.success() {
            let model = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !model.is_empty() {
                return model;
            }
        }
    }
    String::new()
}

fn read_battery_level() -> String {
    for psu in &[
        "/sys/class/power_supply/BAT0",
        "/sys/class/power_supply/battery",
    ] {
        let cap = Path::new(psu).join("capacity");
        if let Ok(content) = fs::read_to_string(&cap) {
            let level = content.trim().to_string();
            if !level.is_empty() {
                return format!("{}%", level);
            }
        }
    }
    String::new()
}

fn read_battery_temp() -> String {
    for psu in &[
        "/sys/class/power_supply/BAT0",
        "/sys/class/power_supply/battery",
    ] {
        let temp_path = Path::new(psu).join("temp");
        if let Ok(content) = fs::read_to_string(&temp_path) {
            if let Ok(raw) = content.trim().parse::<f64>() {
                return format!("{:.0}°C", raw / 10.0);
            }
        }
    }
    // Alternative: thermal zone
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains("battery") || name.contains("temp") {
                if let Ok(content) = fs::read_to_string(entry.path().join("temp")) {
                    if let Ok(raw) = content.trim().parse::<f64>() {
                        return format!("{:.0}°C", raw / 1000.0);
                    }
                }
            }
        }
    }
    String::new()
}

fn read_battery_health() -> String {
    for psu in &[
        "/sys/class/power_supply/BAT0",
        "/sys/class/power_supply/battery",
    ] {
        let health_path = Path::new(psu).join("health");
        if let Ok(content) = fs::read_to_string(&health_path) {
            let h = content.trim().to_string();
            if !h.is_empty() && h != "Unknown" {
                return h;
            }
        }
    }
    String::new()
}

fn read_battery_status() -> String {
    for psu in &[
        "/sys/class/power_supply/BAT0",
        "/sys/class/power_supply/battery",
    ] {
        let status_path = Path::new(psu).join("status");
        if let Ok(content) = fs::read_to_string(&status_path) {
            let s = content.trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    String::new()
}

fn read_cpu_temp() -> String {
    // Try thermal zones for CPU
    let thermal = Path::new("/sys/class/thermal");
    if let Ok(entries) = fs::read_dir(thermal) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("thermal_zone") {
                if let Ok(typ) = fs::read_to_string(entry.path().join("type")) {
                    if typ.trim().contains("cpu") || typ.trim().contains("CPUTEMP") {
                        if let Ok(content) = fs::read_to_string(entry.path().join("temp")) {
                            if let Ok(raw) = content.trim().parse::<f64>() {
                                let temp = if raw > 1000.0 { raw / 1000.0 } else { raw };
                                return format!("{:.0}°C", temp);
                            }
                        }
                    }
                }
            }
        }
    }
    String::new()
}

fn read_brightness() -> String {
    let backlight = Path::new("/sys/class/backlight");
    if let Ok(entries) = fs::read_dir(backlight) {
        for entry in entries.flatten() {
            let dir = entry.path();
            let max_path = dir.join("max_brightness");
            let cur_path = if dir.join("actual_brightness").exists() {
                dir.join("actual_brightness")
            } else {
                dir.join("brightness")
            };
            if let (Ok(max_str), Ok(cur_str)) =
                (fs::read_to_string(&max_path), fs::read_to_string(&cur_path))
            {
                if let (Ok(max), Ok(cur)) =
                    (max_str.trim().parse::<f64>(), cur_str.trim().parse::<f64>())
                {
                    if max > 0.0 {
                        return format!("{:.0}%", (cur / max) * 100.0);
                    }
                }
            }
        }
    }
    String::new()
}

fn read_refresh_rate() -> String {
    // Scan DRM connectors dynamically
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.contains('-') {
                continue;
            }
            let modes_path = entry.path().join("modes");
            if let Ok(content) = fs::read_to_string(&modes_path) {
                for mode in content.lines() {
                    let parts: Vec<&str> = mode.split_whitespace().collect();
                    if let Some(last) = parts.last() {
                        if let Ok(hz) = last.parse::<f64>() {
                            return format!("{:.0}Hz", hz);
                        }
                    }
                }
            }
        }
    }
    String::new()
}

fn read_signal() -> String {
    // Try to read from /proc/net/wireless
    if let Ok(content) = fs::read_to_string("/proc/net/wireless") {
        for line in content.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Ok(level) = parts[3].parse::<f64>() {
                    if level < 0.0 {
                        let bars = if level > -50.0 {
                            4
                        } else if level > -65.0 {
                            3
                        } else if level > -80.0 {
                            2
                        } else {
                            1
                        };
                        return format!("{}/4 ({}dBm)", bars, level as i64);
                    }
                }
            }
        }
    }
    String::new()
}

fn read_wifi_ssid() -> String {
    // Try wpa_supplicant or iwconfig
    if let Ok(out) = std::process::Command::new("iwgetid").arg("-r").output() {
        if out.status.success() {
            let ssid = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !ssid.is_empty() {
                return ssid;
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten_cpu() {
        let cases = vec![
            (
                "AMD Ryzen 3 2200G with Radeon Vega Graphics",
                "AMD Ryzen 3 2200G",
            ),
            ("AMD Ryzen 5 5600X 6-Core Processor", "AMD Ryzen 5 5600X"),
            ("AMD Ryzen 7 5800X3D", "AMD Ryzen 7 5800X3D"),
            ("AMD EPYC 7551P 32-Core Processor", "AMD EPYC 7551P"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten_cpu(input), expected, "CPU: {}", input);
        }
    }

    #[test]
    fn test_shorten_gpu() {
        let cases = vec![
            ("NVIDIA GeForce RTX 3060", "NVIDIA RTX 3060"),
            ("NVIDIA GeForce GTX 1060 6GB", "NVIDIA GTX 1060 6GB"),
            ("AMD Radeon RX 570 Series", "AMD RX 570"),
            ("AMD Radeon RX 7800 XT", "AMD RX 7800 XT"),
            ("Intel UHD Graphics 630", "Intel UHD Graphics 630"),
        ];
        for (input, expected) in cases {
            assert_eq!(shorten_gpu(input), expected, "GPU: {}", input);
        }
    }
}
