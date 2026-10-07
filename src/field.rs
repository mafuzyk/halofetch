//! The catalog of system fields a panel entry can show.

use serde::{Deserialize, Serialize};

/// One piece of system information. The serde name and [`Field::key`] are identical,
/// so configuration files and JSON output use the same spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Os,
    Host,
    Device,
    User,
    Kernel,
    Arch,
    Uptime,
    Packages,
    Flatpak,
    Snap,
    Shell,
    Terminal,
    Font,
    De,
    Wm,
    Resolution,
    Cpu,
    CpuUsage,
    CpuTemp,
    Gpu,
    GpuUsage,
    Vram,
    Memory,
    Swap,
    Disk,
    Load,
    Processes,
    LocalIp,
    Wifi,
    Battery,
    Brightness,
    Locale,
}

/// Section a field belongs to in the editor's add dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldGroup {
    System,
    Software,
    Desktop,
    Hardware,
    Resources,
    Network,
    Power,
}

impl FieldGroup {
    pub const ALL: [FieldGroup; 7] = [
        FieldGroup::System,
        FieldGroup::Software,
        FieldGroup::Desktop,
        FieldGroup::Hardware,
        FieldGroup::Resources,
        FieldGroup::Network,
        FieldGroup::Power,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            FieldGroup::System => "System",
            FieldGroup::Software => "Software",
            FieldGroup::Desktop => "Desktop",
            FieldGroup::Hardware => "Hardware",
            FieldGroup::Resources => "Resources",
            FieldGroup::Network => "Network",
            FieldGroup::Power => "Power",
        }
    }
}

impl Field {
    /// Every field in declaration order, which is also the display and JSON order.
    pub const ALL: [Field; 32] = [
        Field::Os,
        Field::Host,
        Field::Device,
        Field::User,
        Field::Kernel,
        Field::Arch,
        Field::Uptime,
        Field::Packages,
        Field::Flatpak,
        Field::Snap,
        Field::Shell,
        Field::Terminal,
        Field::Font,
        Field::De,
        Field::Wm,
        Field::Resolution,
        Field::Cpu,
        Field::CpuUsage,
        Field::CpuTemp,
        Field::Gpu,
        Field::GpuUsage,
        Field::Vram,
        Field::Memory,
        Field::Swap,
        Field::Disk,
        Field::Load,
        Field::Processes,
        Field::LocalIp,
        Field::Wifi,
        Field::Battery,
        Field::Brightness,
        Field::Locale,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Field::Os => "os",
            Field::Host => "host",
            Field::Device => "device",
            Field::User => "user",
            Field::Kernel => "kernel",
            Field::Arch => "arch",
            Field::Uptime => "uptime",
            Field::Packages => "packages",
            Field::Flatpak => "flatpak",
            Field::Snap => "snap",
            Field::Shell => "shell",
            Field::Terminal => "terminal",
            Field::Font => "font",
            Field::De => "de",
            Field::Wm => "wm",
            Field::Resolution => "resolution",
            Field::Cpu => "cpu",
            Field::CpuUsage => "cpu_usage",
            Field::CpuTemp => "cpu_temp",
            Field::Gpu => "gpu",
            Field::GpuUsage => "gpu_usage",
            Field::Vram => "vram",
            Field::Memory => "memory",
            Field::Swap => "swap",
            Field::Disk => "disk",
            Field::Load => "load",
            Field::Processes => "processes",
            Field::LocalIp => "local_ip",
            Field::Wifi => "wifi",
            Field::Battery => "battery",
            Field::Brightness => "brightness",
            Field::Locale => "locale",
        }
    }

    /// Resolves a key, including the names used by earlier configuration versions.
    pub fn from_key(key: &str) -> Option<Field> {
        match key {
            "battery_level" | "battery_status" => Some(Field::Battery),
            "signal" | "wifi_ssid" => Some(Field::Wifi),
            "refresh_rate" => Some(Field::Resolution),
            _ => Field::ALL.iter().copied().find(|field| field.key() == key),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Field::Os => "OS",
            Field::Host => "Hostname",
            Field::Device => "Model",
            Field::User => "User",
            Field::Kernel => "Kernel",
            Field::Arch => "Arch",
            Field::Uptime => "Uptime",
            Field::Packages => "Packages",
            Field::Flatpak => "Flatpak",
            Field::Snap => "Snap",
            Field::Shell => "Shell",
            Field::Terminal => "Terminal",
            Field::Font => "Font",
            Field::De => "DE",
            Field::Wm => "WM",
            Field::Resolution => "Display",
            Field::Cpu => "CPU",
            Field::CpuUsage => "CPU Usage",
            Field::CpuTemp => "CPU Temp",
            Field::Gpu => "GPU",
            Field::GpuUsage => "GPU Usage",
            Field::Vram => "VRAM",
            Field::Memory => "Memory",
            Field::Swap => "Swap",
            Field::Disk => "Disk",
            Field::Load => "Load",
            Field::Processes => "Processes",
            Field::LocalIp => "Local IP",
            Field::Wifi => "Wi-Fi",
            Field::Battery => "Battery",
            Field::Brightness => "Brightness",
            Field::Locale => "Locale",
        }
    }

    /// Nerd Font glyph shown before the label.
    pub const fn icon(self) -> &'static str {
        match self {
            Field::Os => "\u{f17c}",
            Field::Host => "\u{f109}",
            Field::Device => "\u{f108}",
            Field::User => "\u{f007}",
            Field::Kernel => "\u{e271}",
            Field::Arch => "\u{f2db}",
            Field::Uptime => "\u{f017}",
            Field::Packages => "\u{f1b3}",
            Field::Flatpak => "\u{f1b2}",
            Field::Snap => "\u{f1b2}",
            Field::Shell => "\u{f489}",
            Field::Terminal => "\u{f120}",
            Field::Font => "\u{f031}",
            Field::De => "\u{f2d0}",
            Field::Wm => "\u{f2d2}",
            Field::Resolution => "\u{f108}",
            Field::Cpu => "\u{f2db}",
            Field::CpuUsage => "\u{f0e4}",
            Field::CpuTemp => "\u{f2c9}",
            Field::Gpu => "\u{f26c}",
            Field::GpuUsage => "\u{f0e4}",
            Field::Vram => "\u{f26c}",
            Field::Memory => "\u{f1c0}",
            Field::Swap => "\u{f0ec}",
            Field::Disk => "\u{f0a0}",
            Field::Load => "\u{f0e7}",
            Field::Processes => "\u{f013}",
            Field::LocalIp => "\u{f0ac}",
            Field::Wifi => "\u{f1eb}",
            Field::Battery => "\u{f240}",
            Field::Brightness => "\u{f185}",
            Field::Locale => "\u{f1ab}",
        }
    }

    pub const fn group(self) -> FieldGroup {
        match self {
            Field::Os
            | Field::Host
            | Field::Device
            | Field::User
            | Field::Kernel
            | Field::Arch
            | Field::Uptime
            | Field::Locale => FieldGroup::System,
            Field::Packages
            | Field::Flatpak
            | Field::Snap
            | Field::Shell
            | Field::Terminal
            | Field::Font => FieldGroup::Software,
            Field::De | Field::Wm | Field::Resolution => FieldGroup::Desktop,
            Field::Cpu | Field::CpuTemp | Field::Gpu | Field::Vram => FieldGroup::Hardware,
            Field::CpuUsage
            | Field::GpuUsage
            | Field::Memory
            | Field::Swap
            | Field::Disk
            | Field::Load
            | Field::Processes => FieldGroup::Resources,
            Field::LocalIp | Field::Wifi => FieldGroup::Network,
            Field::Battery | Field::Brightness => FieldGroup::Power,
        }
    }

    /// One short sentence for the editor's add dialog.
    pub const fn description(self) -> &'static str {
        match self {
            Field::Os => "Distribution name and version",
            Field::Host => "Network name of this machine",
            Field::Device => "Hardware model reported by firmware",
            Field::User => "Name of the current user",
            Field::Kernel => "Running Linux kernel release",
            Field::Arch => "Processor instruction set architecture",
            Field::Uptime => "Time since the last boot",
            Field::Packages => "Packages from native package managers",
            Field::Flatpak => "Number of installed Flatpak apps",
            Field::Snap => "Number of installed Snap packages",
            Field::Shell => "Shell that started this session",
            Field::Terminal => "Terminal emulator hosting this session",
            Field::Font => "Terminal font from its configuration file",
            Field::De => "Desktop environment in use",
            Field::Wm => "Window or compositor manager in use",
            Field::Resolution => "Resolution of connected displays",
            Field::Cpu => "CPU model, thread count and top clock",
            Field::CpuUsage => "Live CPU load (monitor mode only)",
            Field::CpuTemp => "Hottest CPU temperature sensor",
            Field::Gpu => "Graphics adapter model",
            Field::GpuUsage => "Live GPU load (monitor mode only)",
            Field::Vram => "Video memory in use and total",
            Field::Memory => "RAM in use and total",
            Field::Swap => "Swap space in use and total",
            Field::Disk => "Root filesystem space in use and total",
            Field::Load => "One, five and fifteen minute load averages",
            Field::Processes => "Number of running processes",
            Field::LocalIp => "Primary IPv4 address on the local network",
            Field::Wifi => "Wireless network name and signal strength",
            Field::Battery => "Charge level and charging state",
            Field::Brightness => "Display backlight level",
            Field::Locale => "Language setting of the user session",
        }
    }

    /// Whether the value is drawn as a bar in addition to its text.
    pub const fn has_gauge(self) -> bool {
        matches!(
            self,
            Field::CpuUsage
                | Field::CpuTemp
                | Field::GpuUsage
                | Field::Vram
                | Field::Memory
                | Field::Swap
                | Field::Disk
                | Field::Battery
                | Field::Brightness
        )
    }

    /// Only meaningful in monitor mode, where the value is refreshed every interval.
    pub const fn live_only(self) -> bool {
        matches!(self, Field::CpuUsage | Field::GpuUsage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_key_round_trips_through_from_key() {
        for field in Field::ALL {
            assert_eq!(Field::from_key(field.key()), Some(field), "{}", field.key());
        }
    }

    #[test]
    fn keys_match_the_serde_names() {
        for field in Field::ALL {
            let json = serde_json::to_string(&field).expect("unit variants always serialize");
            assert_eq!(json, format!("\"{}\"", field.key()));
        }
    }

    #[test]
    fn local_ip_serializes_as_snake_case() {
        let json = serde_json::to_string(&Field::LocalIp).expect("unit variants always serialize");
        assert_eq!(json, "\"local_ip\"");
    }

    #[test]
    fn all_contains_no_duplicates() {
        let unique: HashSet<Field> = Field::ALL.into_iter().collect();
        assert_eq!(unique.len(), Field::ALL.len());
        assert_eq!(Field::ALL.len(), 32);
    }

    #[test]
    fn legacy_aliases_map_to_current_fields() {
        assert_eq!(Field::from_key("battery_level"), Some(Field::Battery));
        assert_eq!(Field::from_key("battery_status"), Some(Field::Battery));
        assert_eq!(Field::from_key("signal"), Some(Field::Wifi));
        assert_eq!(Field::from_key("wifi_ssid"), Some(Field::Wifi));
        assert_eq!(Field::from_key("refresh_rate"), Some(Field::Resolution));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert_eq!(Field::from_key("nonsense"), None);
        assert_eq!(Field::from_key(""), None);
        assert_eq!(Field::from_key("OS"), None);
    }

    #[test]
    fn gauge_and_live_flags() {
        assert!(Field::CpuUsage.live_only());
        assert!(Field::GpuUsage.live_only());
        assert!(!Field::Memory.live_only());
        assert!(Field::Memory.has_gauge());
        assert!(Field::Brightness.has_gauge());
        assert!(!Field::Os.has_gauge());
    }

    #[test]
    fn every_field_has_presentation_text_and_a_group() {
        for field in Field::ALL {
            assert!(!field.label().is_empty(), "{}", field.key());
            assert!(!field.icon().is_empty(), "{}", field.key());
            assert!(!field.description().is_empty(), "{}", field.key());
            assert!(FieldGroup::ALL.contains(&field.group()));
        }
    }

    #[test]
    fn specific_labels_and_groups() {
        assert_eq!(Field::Host.label(), "Hostname");
        assert_eq!(Field::Resolution.label(), "Display");
        assert_eq!(Field::De.group(), FieldGroup::Desktop);
        assert_eq!(Field::Battery.group(), FieldGroup::Power);
        assert_eq!(Field::LocalIp.group(), FieldGroup::Network);
    }
}
