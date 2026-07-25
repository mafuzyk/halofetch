use super::{Component, RenderCtx, StyledSpan};
use crate::theme::Color;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

struct CpuSnapshot {
    total: u64,
    idle: u64,
}

pub struct MonitorComponent {
    cpu_snapshot: Mutex<Option<CpuSnapshot>>,
}

impl MonitorComponent {
    pub fn new() -> Self {
        Self {
            cpu_snapshot: Mutex::new(None),
        }
    }
}

fn bar(pct: f64, width: usize) -> String {
    let pct = pct.clamp(0.0, 1.0);
    let filled = (pct * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

fn bar_color(pct: f64) -> Color {
    if pct < 0.50 {
        Color::new(0x98, 0xC3, 0x79)
    } else if pct < 0.80 {
        Color::new(0xE5, 0xC0, 0x7B)
    } else {
        Color::new(0xE0, 0x6C, 0x75)
    }
}

fn render_bar(label: &str, pct: f64, bar_w: usize, color: Color) -> Vec<StyledSpan> {
    vec![
        StyledSpan::new(format!(" {} ", label)).fg(color).bold(),
        StyledSpan::new(" "),
        StyledSpan::new(bar(pct, bar_w)).fg(bar_color(pct)),
        StyledSpan::new(format!(" {:>3.0}%", pct * 100.0)).fg(bar_color(pct)),
    ]
}

fn render_line(label: &str, value: &str, color: Color) -> Vec<StyledSpan> {
    vec![
        StyledSpan::new(format!(" {} ", label)).fg(color).bold(),
        StyledSpan::new(" "),
        StyledSpan::new(value).fg(Color::new(200, 200, 220)),
    ]
}

fn read_stat() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    let line = content.lines().next()?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    let user: u64 = parts.get(1)?.parse().ok()?;
    let nice: u64 = parts.get(2)?.parse().ok()?;
    let system: u64 = parts.get(3)?.parse().ok()?;
    let idle: u64 = parts.get(4)?.parse().ok()?;
    let iowait: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    let irq: u64 = parts.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
    let softirq: u64 = parts.get(7).and_then(|s| s.parse().ok()).unwrap_or(0);
    let steal: u64 = parts.get(8).and_then(|s| s.parse().ok()).unwrap_or(0);
    let total = user + nice + system + idle + iowait + irq + softirq + steal;
    Some((total, idle + iowait))
}

fn read_live_cpu_pct(snapshot: &Mutex<Option<CpuSnapshot>>) -> Option<f64> {
    let mut cur = read_stat()?;
    let mut guard = snapshot.lock().unwrap_or_else(|e| e.into_inner());
    let previous = if let Some(previous) = guard.as_ref() {
        CpuSnapshot {
            total: previous.total,
            idle: previous.idle,
        }
    } else {
        // A one-shot fetch has no previous frame. Take a short initial sample
        // so its CPU figure is useful without making every refresh block.
        let previous = CpuSnapshot {
            total: cur.0,
            idle: cur.1,
        };
        std::thread::sleep(Duration::from_millis(120));
        cur = read_stat()?;
        previous
    };
    let dtotal = cur.0.saturating_sub(previous.total);
    let didle = cur.1.saturating_sub(previous.idle);
    *guard = Some(CpuSnapshot {
        total: cur.0,
        idle: cur.1,
    });
    if dtotal == 0 {
        None
    } else {
        Some((dtotal.saturating_sub(didle)) as f64 / dtotal as f64)
    }
}

fn read_live_cpu_temp() -> f64 {
    let thermal = Path::new("/sys/class/thermal");
    let entries = match fs::read_dir(thermal) {
        Ok(e) => e,
        Err(_) => return 0.0,
    };
    let mut best = 0.0_f64;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("thermal_zone") {
            continue;
        }
        let typ = match fs::read_to_string(entry.path().join("type")) {
            Ok(t) => t.trim().to_lowercase(),
            Err(_) => continue,
        };
        if !typ.contains("cpu")
            && !typ.contains("x86_pkg")
            && !typ.contains("coretemp")
            && !typ.contains("soc_max")
            && !typ.contains("cpu_big")
            && !typ.contains("cpu_little")
        {
            continue;
        }
        let content = match fs::read_to_string(entry.path().join("temp")) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let raw: f64 = match content.trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let temp = if raw > 1000.0 { raw / 1000.0 } else { raw };
        if temp > best {
            best = temp;
        }
    }

    // Desktop CPUs commonly expose temperatures through hwmon rather than
    // thermal_zone (for example k10temp, zenpower, and coretemp).
    if let Ok(chips) = fs::read_dir("/sys/class/hwmon") {
        for chip in chips.flatten() {
            let chip_name = fs::read_to_string(chip.path().join("name"))
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if !matches!(
                chip_name.as_str(),
                "k10temp" | "zenpower" | "coretemp" | "cpu_thermal" | "acpitz"
            ) {
                continue;
            }
            let Ok(files) = fs::read_dir(chip.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();
                if !name.starts_with("temp") || !name.ends_with("_input") {
                    continue;
                }
                if let Ok(raw) = fs::read_to_string(file.path()) {
                    if let Ok(raw) = raw.trim().parse::<f64>() {
                        let temp = if raw > 1000.0 { raw / 1000.0 } else { raw };
                        if (1.0..=150.0).contains(&temp) {
                            best = best.max(temp);
                        }
                    }
                }
            }
        }
    }
    best
}

fn read_live_mem_pct() -> Option<f64> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = 0.0_f64;
    let mut available = 0.0_f64;
    for line in content.lines() {
        if let Some(t) = line.strip_prefix("MemTotal:") {
            total = t
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
        if let Some(t) = line.strip_prefix("MemAvailable:") {
            available = t
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
    }
    if total == 0.0 {
        return None;
    }
    Some(1.0 - (available / total))
}

fn read_live_gpu_pct() -> Option<f64> {
    // 1. AMD/Intel via sysfs gpu_busy_percent (amdgpu, i915, xe)
    let drm = Path::new("/sys/class/drm");
    if let Ok(entries) = fs::read_dir(drm) {
        for entry in entries.flatten() {
            let path = entry.path().join("device").join("gpu_busy_percent");
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(v) = content.trim().parse::<f64>() {
                    return Some((v / 100.0).clamp(0.0, 1.0));
                }
            }
        }
    }
    // 2. NVIDIA via nvidia-smi
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = s.trim().parse::<f64>() {
                return Some((v / 100.0).clamp(0.0, 1.0));
            }
        }
    }
    None
}

fn read_live_battery() -> (f64, String, String) {
    let psu = Path::new("/sys/class/power_supply");
    let entries = match fs::read_dir(psu) {
        Ok(e) => e,
        Err(_) => return (0.0, String::new(), String::new()),
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("BAT") && name != "battery" && !name.starts_with("axp") {
            continue;
        }
        if !dir.join("capacity").exists() {
            continue;
        }
        let capacity = fs::read_to_string(dir.join("capacity"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        let status = fs::read_to_string(dir.join("status"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let temp_raw = fs::read_to_string(dir.join("temp"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        let temp_str = if temp_raw > 0.0 {
            format!("{:.0}°C", temp_raw / 10.0)
        } else {
            String::new()
        };
        return (capacity / 100.0, status, temp_str);
    }
    // fallback: thermal zone named "battery"
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains("battery") || name.contains("temp") {
                if let Ok(content) = fs::read_to_string(entry.path().join("temp")) {
                    if let Ok(raw) = content.trim().parse::<f64>() {
                        let temp_str = format!("{:.0}°C", raw / 1000.0);
                        return (0.0, String::new(), temp_str);
                    }
                }
            }
        }
    }
    (0.0, String::new(), String::new())
}

impl Component for MonitorComponent {
    fn name(&self) -> &str {
        "monitor"
    }

    fn render_ansi(&self, ctx: &RenderCtx) -> String {
        let styled = self.render_styled(ctx);
        let mut out = String::new();
        for line in styled {
            for s in &line {
                if s.bold {
                    out.push_str("\x1b[1m");
                }
                if let Some(fg) = &s.fg {
                    out.push_str(&fg.fg_escape());
                }
                if let Some(bg) = &s.bg {
                    out.push_str(&bg.bg_escape());
                }
                out.push_str(&s.text);
                out.push_str("\x1b[0m");
            }
            out.push('\n');
        }
        out
    }

    fn render_styled(&self, ctx: &RenderCtx) -> Vec<Vec<StyledSpan>> {
        let bar_w = ctx.term_width.saturating_sub(12).clamp(6, 20);
        let ac = ctx
            .palette
            .first()
            .copied()
            .unwrap_or(Color::new(255, 102, 146));

        let mut lines: Vec<Vec<StyledSpan>> = Vec::new();
        lines.push(vec![StyledSpan::new(" MONITOR ").fg(ac).bold()]);

        // CPU — live from /proc/stat
        if let Some(cpu_pct) = read_live_cpu_pct(&self.cpu_snapshot) {
            lines.push(render_bar("CPU", cpu_pct, bar_w, Color::new(100, 200, 255)));
        } else {
            lines.push(render_line("CPU", "N/A", Color::new(100, 200, 255)));
        }

        // RAM — live from /proc/meminfo
        if let Some(mem_pct) = read_live_mem_pct() {
            lines.push(render_bar("RAM", mem_pct, bar_w, Color::new(157, 133, 255)));
        } else {
            lines.push(render_line("RAM", "N/A", Color::new(157, 133, 255)));
        }

        // GPU — live from sysfs/nvidia-smi
        if let Some(gpu_pct) = read_live_gpu_pct() {
            lines.push(render_bar("GPU", gpu_pct, bar_w, Color::new(255, 184, 131)));
        }

        // Temperature — live from thermal zones
        let cpu_temp = read_live_cpu_temp();
        if cpu_temp > 0.0 {
            let temp_pct = (cpu_temp / 100.0).min(1.0);
            lines.push(render_bar(
                "TEMP",
                temp_pct,
                bar_w,
                Color::new(255, 100, 100),
            ));
        }

        // Disk — from ctx.info (one-shot, but stable enough)
        if !ctx.info.disk.is_empty() {
            let disk_pct = parse_disk_pct(&ctx.info.disk);
            lines.push(render_bar("DSK", disk_pct, bar_w, Color::new(255, 200, 80)));
        }

        // Battery — live from power supply sysfs
        let (bat_pct, bat_status, bat_temp) = read_live_battery();
        if bat_pct > 0.0 {
            lines.push(render_bar("BAT", bat_pct, bar_w, Color::new(80, 200, 120)));
            let detail = if bat_temp.is_empty() && bat_status.is_empty() {
                String::new()
            } else {
                let mut parts = Vec::new();
                if !bat_temp.is_empty() {
                    parts.push(bat_temp);
                }
                if !bat_status.is_empty() {
                    parts.push(bat_status);
                }
                parts.join(" ")
            };
            if !detail.is_empty() {
                lines.push(render_line("", &detail, Color::new(140, 140, 160)));
            }
        } else if !ctx.info.battery_level.is_empty() {
            // fallback to one-shot info
            let bat_pct = ctx
                .info
                .battery_level
                .trim_end_matches('%')
                .parse::<f64>()
                .unwrap_or(0.0)
                / 100.0;
            let temp = if !ctx.info.battery_temp.is_empty() {
                format!(" {}", ctx.info.battery_temp)
            } else {
                String::new()
            };
            let status = if !ctx.info.battery_status.is_empty() {
                format!(" {}", ctx.info.battery_status)
            } else {
                String::new()
            };
            lines.push(render_bar("BAT", bat_pct, bar_w, Color::new(80, 200, 120)));
            lines.push(render_line(
                "",
                &format!("{}{}", temp, status),
                Color::new(140, 140, 160),
            ));
        }

        lines
    }

    fn min_width(&self) -> usize {
        25
    }
    fn min_height(&self) -> usize {
        6
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn parse_disk_pct(s: &str) -> f64 {
    let clean = s.trim_end_matches('G');
    if let Some((used, total)) = clean.split_once('/') {
        let u = used.trim().parse::<f64>().unwrap_or(0.0);
        let t = total.trim().parse::<f64>().unwrap_or(1.0);
        if t > 0.0 {
            return (u / t).min(1.0);
        }
    }
    0.0
}
