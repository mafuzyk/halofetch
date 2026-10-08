//! Battery charge and state from `GetSystemPowerStatus`.

use std::mem::zeroed;

use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use crate::info::Battery;

/// `BatteryFlag` value of a machine without a battery.
const NO_SYSTEM_BATTERY: u8 = 128;
/// Unknown value of `BatteryLifePercent`.
const UNKNOWN_PERCENT: u8 = 255;
/// `ACLineStatus` value when the machine runs on AC power.
const ON_AC_POWER: u8 = 1;

pub(super) fn battery() -> Option<Battery> {
    // SAFETY: SYSTEM_POWER_STATUS is plain data, so all-zero is a valid value to overwrite.
    let mut status: SYSTEM_POWER_STATUS = unsafe { zeroed() };
    // SAFETY: `status` is a valid, writable SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }
    battery_from(
        status.ACLineStatus,
        status.BatteryFlag,
        status.BatteryLifePercent,
    )
}

/// Battery state from the raw power status fields. `None` when the machine has no battery
/// or its charge is unknown.
fn battery_from(ac_line: u8, flag: u8, percent: u8) -> Option<Battery> {
    if flag == NO_SYSTEM_BATTERY || percent == UNKNOWN_PERCENT {
        return None;
    }
    let on_ac = ac_line == ON_AC_POWER;
    let status = if on_ac && percent >= 100 {
        "Full"
    } else if on_ac {
        "Charging"
    } else {
        "Discharging"
    };
    Some(Battery {
        level: f64::from(percent.min(100)) / 100.0,
        status: Some(status.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_state_follows_the_ac_line() {
        let text = |battery: Option<Battery>| battery.map(|battery| battery.text());
        assert_eq!(
            text(battery_from(1, 8, 87)).as_deref(),
            Some("87% (Charging)")
        );
        assert_eq!(
            text(battery_from(1, 1, 100)).as_deref(),
            Some("100% (Full)")
        );
        assert_eq!(
            text(battery_from(0, 2, 42)).as_deref(),
            Some("42% (Discharging)")
        );
    }

    #[test]
    fn missing_or_unknown_battery_is_absent() {
        assert_eq!(battery_from(1, NO_SYSTEM_BATTERY, 100), None);
        assert_eq!(battery_from(0, 1, UNKNOWN_PERCENT), None);
    }

    #[test]
    fn battery_query_does_not_panic() {
        let _ = battery();
    }
}
