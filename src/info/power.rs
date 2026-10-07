//! Battery and backlight readings from sysfs.

use super::{read_text, sorted_dir};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Battery {
    /// Charge, 0..=1.
    pub(super) level: f64,
    /// `Charging`, `Discharging`, `Full` and so on, when the kernel reports it.
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

/// The first power supply whose `type` is `Battery` and which reports a capacity.
pub(super) fn battery() -> Option<Battery> {
    sorted_dir("/sys/class/power_supply", |_| true)
        .into_iter()
        .find_map(|dir| {
            let kind = read_text(dir.join("type"))?;
            if !kind.eq_ignore_ascii_case("battery") {
                return None;
            }
            let capacity: f64 = read_text(dir.join("capacity"))?.parse().ok()?;
            Some(Battery {
                level: (capacity / 100.0).clamp(0.0, 1.0),
                status: read_text(dir.join("status")),
            })
        })
}

/// Backlight level of the first device with a positive maximum, 0..=1.
pub(super) fn brightness() -> Option<f64> {
    sorted_dir("/sys/class/backlight", |_| true)
        .into_iter()
        .find_map(|dir| {
            let max: f64 = read_text(dir.join("max_brightness"))?.parse().ok()?;
            if max <= 0.0 {
                return None;
            }
            let current_file = if dir.join("actual_brightness").exists() {
                "actual_brightness"
            } else {
                "brightness"
            };
            let current: f64 = read_text(dir.join(current_file))?.parse().ok()?;
            Some((current / max).clamp(0.0, 1.0))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
