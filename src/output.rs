//! Machine-readable output of the detected system information.

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

use crate::field::Field;
use crate::info::{Gauges, SysInfo, Usage};

const SCHEMA_VERSION: u32 = 2;

/// Pretty-printed JSON: `schema_version`, the display string of every present field
/// in [`Field::ALL`] order, and the numeric gauges that are set.
pub fn system_info_json(info: &SysInfo) -> serde_json::Result<String> {
    let document = Document {
        schema_version: SCHEMA_VERSION,
        system: System(info),
        gauges: GaugeDocument::from(&info.gauges),
    };
    serde_json::to_string_pretty(&document)
}

#[derive(Serialize)]
struct Document<'a> {
    schema_version: u32,
    system: System<'a>,
    gauges: GaugeDocument,
}

/// Serializes the present fields as a JSON object in declaration order. A map
/// built from a `BTreeMap` would sort the keys alphabetically instead.
struct System<'a>(&'a SysInfo);

impl Serialize for System<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        for field in Field::ALL {
            if let Some(value) = self.0.get(field) {
                map.serialize_entry(field.key(), value)?;
            }
        }
        map.end()
    }
}

#[derive(Serialize)]
struct GaugeDocument {
    #[serde(skip_serializing_if = "Option::is_none")]
    memory: Option<UsageDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    swap: Option<UsageDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disk: Option<UsageDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vram: Option<UsageDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu_temp_celsius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    battery_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    brightness_percent: Option<f64>,
}

#[derive(Serialize)]
struct UsageDocument {
    used: u64,
    total: u64,
    percent: f64,
}

impl From<Usage> for UsageDocument {
    fn from(usage: Usage) -> Self {
        UsageDocument {
            used: usage.used,
            total: usage.total,
            percent: round2(usage.ratio() * 100.0),
        }
    }
}

impl From<&Gauges> for GaugeDocument {
    fn from(gauges: &Gauges) -> Self {
        GaugeDocument {
            memory: gauges.memory.map(UsageDocument::from),
            swap: gauges.swap.map(UsageDocument::from),
            disk: gauges.disk.map(UsageDocument::from),
            vram: gauges.vram.map(UsageDocument::from),
            cpu_percent: gauges.cpu.map(|ratio| round2(ratio * 100.0)),
            gpu_percent: gauges.gpu.map(|ratio| round2(ratio * 100.0)),
            cpu_temp_celsius: gauges.cpu_temp.map(round2),
            battery_percent: gauges.battery.map(|ratio| round2(ratio * 100.0)),
            brightness_percent: gauges.brightness.map(|ratio| round2(ratio * 100.0)),
        }
    }
}

/// Two decimals, so that floating-point noise such as 37.00000000000001 is not printed.
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(info: &SysInfo) -> Value {
        let json = system_info_json(info).expect("system info serializes");
        serde_json::from_str(&json).expect("output is valid JSON")
    }

    #[test]
    fn output_declares_schema_version_two() {
        let value = parse(&SysInfo::default());
        assert_eq!(value["schema_version"], 2);
        assert!(value["system"]
            .as_object()
            .is_some_and(|map| map.is_empty()));
        assert!(value["gauges"]
            .as_object()
            .is_some_and(|map| map.is_empty()));
    }

    #[test]
    fn absent_fields_are_omitted() {
        let mut info = SysInfo::default();
        info.set(Field::Os, "Arch Linux");
        info.set(Field::Kernel, "6.12.1");
        let value = parse(&info);
        let system = value["system"].as_object().expect("system is an object");
        assert_eq!(system.len(), 2);
        assert_eq!(system["os"], "Arch Linux");
        assert!(!system.contains_key("swap"));
    }

    #[test]
    fn fields_appear_in_catalog_order() {
        let mut info = SysInfo::default();
        info.set(Field::Locale, "en_US.UTF-8");
        info.set(Field::Kernel, "6.12.1");
        info.set(Field::Os, "Arch Linux");
        let json = system_info_json(&info).expect("system info serializes");
        let os = json.find("\"os\"").expect("os present");
        let kernel = json.find("\"kernel\"").expect("kernel present");
        let locale = json.find("\"locale\"").expect("locale present");
        assert!(os < kernel && kernel < locale);
    }

    #[test]
    fn memory_gauge_carries_numbers_and_percent() {
        let mut info = SysInfo::default();
        info.gauges.memory = Some(Usage { used: 1, total: 2 });
        let value = parse(&info);
        let memory = &value["gauges"]["memory"];
        assert_eq!(memory["used"], 1);
        assert_eq!(memory["total"], 2);
        assert_eq!(memory["percent"], 50.0);
    }

    #[test]
    fn scalar_gauges_are_percentages_or_degrees() {
        let info = SysInfo::sample();
        let value = parse(&info);
        let gauges = &value["gauges"];
        assert_eq!(gauges["cpu_percent"], 37.0);
        assert_eq!(gauges["gpu_percent"], 3.0);
        assert_eq!(gauges["cpu_temp_celsius"], 54.0);
        assert_eq!(gauges["battery_percent"], 87.0);
        assert_eq!(gauges["brightness_percent"], 60.0);
        assert!(gauges.get("vram").is_some());
    }

    #[test]
    fn sample_serializes_every_field() {
        let value = parse(&SysInfo::sample());
        let system = value["system"].as_object().expect("system is an object");
        assert_eq!(system.len(), Field::ALL.len());
        assert_eq!(system["local_ip"], "192.168.1.5 (wlan0)");
    }
}
