//! Machine-readable output formats.

use serde::Serialize;

use crate::info::SysInfo;

#[derive(Serialize)]
struct JsonOutput {
    schema_version: u32,
    system: serde_json::Map<String, serde_json::Value>,
}

pub fn system_info_json(info: &SysInfo) -> serde_json::Result<String> {
    let mut system = match serde_json::to_value(info)? {
        serde_json::Value::Object(fields) => fields,
        _ => serde_json::Map::new(),
    };
    system.retain(|_, value| !matches!(value, serde_json::Value::String(text) if text.is_empty()));
    serde_json::to_string_pretty(&JsonOutput {
        schema_version: 1,
        system,
    })
}

#[cfg(test)]
mod tests {
    use super::system_info_json;
    use crate::info::SysInfo;

    #[test]
    fn json_has_a_version_and_omits_empty_fields() {
        let info = SysInfo {
            os: "Runic Linux".into(),
            host: String::new(),
            ..SysInfo::default()
        };
        let value: serde_json::Value =
            serde_json::from_str(&system_info_json(&info).unwrap()).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["system"]["os"], "Runic Linux");
        assert!(value["system"].get("host").is_none());
    }
}
