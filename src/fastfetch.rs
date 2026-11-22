//! Migration from a fastfetch JSONC configuration.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{bail, eyre, Result};
use serde_json::Value;

use crate::config::{Config, FieldEntry, ImageProtocol, VERSION};
use crate::field::Field;
use crate::logo::LogoSource;
use crate::theme::Color;

/// Result of importing a fastfetch configuration: the target configuration plus
/// a report of what was translated, ignored and could not be translated.
pub struct Import {
    pub config: Config,
    pub mapped: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
    /// Inline `logo.type = "data"` art, to be written next to the configuration.
    pub logo_data: Option<String>,
}

/// Strip JSONC comments and trailing commas, then validate the result as JSON.
pub fn jsonc_to_json(text: &str) -> Result<String> {
    let without_comments = strip_comments(text)?;
    let json = strip_trailing_commas(&without_comments);
    serde_json::from_str::<Value>(&json)
        .map_err(|err| eyre!("the cleaned fastfetch configuration is not valid JSON: {err}"))?;
    Ok(json)
}

/// Convert a fastfetch configuration into the current halofetch schema.
pub fn parse(text: &str) -> Result<Import> {
    let json = jsonc_to_json(text)?;
    let root: Value = serde_json::from_str(&json)?;
    if !root.is_object() {
        bail!("the fastfetch configuration must be a JSON object");
    }

    let mut import = Import {
        config: Config::default(),
        mapped: Vec::new(),
        skipped: Vec::new(),
        warnings: Vec::new(),
        logo_data: None,
    };
    import.config.version = VERSION;

    if let Some(logo) = root.get("logo").filter(|value| value.is_object()) {
        apply_logo(&mut import, logo);
    }
    if let Some(display) = root.get("display").filter(|value| value.is_object()) {
        apply_display(&mut import, display);
    }
    if let Some(modules) = root.get("modules").and_then(Value::as_array) {
        apply_modules(&mut import, modules);
    }

    import.config.normalize();
    Ok(import)
}

/// First candidate path, whether or not the file exists.
pub fn preferred_path() -> Option<PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    let appdata = std::env::var("APPDATA").ok();
    path_candidates(xdg.as_deref(), home.as_deref(), appdata.as_deref())
        .into_iter()
        .next()
}

/// First candidate path that exists.
pub fn default_path() -> Option<PathBuf> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    let appdata = std::env::var("APPDATA").ok();
    path_candidates(xdg.as_deref(), home.as_deref(), appdata.as_deref())
        .into_iter()
        .find(|path| path.exists())
}

/// Pure path logic shared by the two public path helpers.
fn path_candidates(xdg: Option<&str>, home: Option<&str>, appdata: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push = |dir: PathBuf| {
        for name in ["config.jsonc", "config.json"] {
            let path = dir.join(name);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
    };

    if cfg!(windows) {
        if let Some(appdata) = appdata.filter(|value| !value.is_empty()) {
            push(PathBuf::from(appdata).join("fastfetch"));
        }
    } else if let Some(xdg) = xdg.filter(|value| Path::new(value).is_absolute()) {
        push(PathBuf::from(xdg).join("fastfetch"));
    }
    if let Some(home) = home.filter(|value| !value.is_empty()) {
        push(PathBuf::from(home).join(".config").join("fastfetch"));
    }
    candidates
}

// ── JSONC cleanup ─────────────────────────────────────────────────────────

fn strip_comments(text: &str) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }
        if ch != '/' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('/') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
                out.push(' ');
            }
            Some('*') => {
                chars.next();
                let mut closed = false;
                while let Some(next) = chars.next() {
                    if next == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        closed = true;
                        break;
                    }
                    if next == '\n' {
                        out.push('\n');
                    }
                }
                if !closed {
                    bail!("unterminated block comment in fastfetch configuration");
                }
                out.push(' ');
            }
            _ => out.push(ch),
        }
    }
    Ok(out)
}

fn strip_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }
        if ch == ',' {
            let mut ahead = chars.clone();
            while ahead.peek().is_some_and(|next| next.is_whitespace()) {
                ahead.next();
            }
            if matches!(ahead.peek(), Some('}') | Some(']')) {
                continue;
            }
        }
        out.push(ch);
    }
    out
}

// ── Configuration mapping ─────────────────────────────────────────────────

fn apply_logo(import: &mut Import, logo: &Value) {
    if logo.get("padding").is_some() {
        import
            .warnings
            .push("logo.padding was not imported".to_string());
    }
    let kind = logo
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let raw_source = logo
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let source = raw_source.trim();

    match kind.as_str() {
        "builtin" | "small" => {
            if source.is_empty() {
                import
                    .warnings
                    .push(format!("logo type '{kind}' has no source; not imported"));
                return;
            }
            import.config.logo.source = LogoSource::Builtin {
                key: source.to_string(),
            };
            if kind == "small" {
                import.config.logo.auto_small = true;
                import.config.logo.compact = true;
            }
            push_unique(&mut import.mapped, "logo");
        }
        "none" => {
            import.config.logo.source = LogoSource::None;
            push_unique(&mut import.mapped, "logo");
        }
        "file" => {
            if source.is_empty() {
                import
                    .warnings
                    .push("logo type 'file' has no source; not imported".to_string());
                return;
            }
            import.config.logo.source = LogoSource::File {
                path: source.to_string(),
            };
            push_unique(&mut import.mapped, "logo");
        }
        "kitty" => {
            if source.is_empty() {
                import.warnings.push(format!(
                    "logo type '{kind}' has no source; image logo was not imported"
                ));
                return;
            }
            import.config.logo.image.path = source.to_string();
            import.config.logo.image.protocol = ImageProtocol::Auto;
            push_unique(&mut import.mapped, "logo");
        }
        "sixel" | "iterm" | "chafa" | "raw" => {
            import.warnings.push(format!(
                "logo type '{kind}' is not supported by the Kitty renderer; logo was not imported"
            ));
        }
        "data" => {
            let data = strip_color_placeholders(raw_source);
            if data.trim().is_empty() {
                import
                    .warnings
                    .push("logo type 'data' has no visible art; not imported".to_string());
            } else {
                import.logo_data = Some(data);
                import
                    .warnings
                    .push("the inline logo will be saved as a logo file".to_string());
                if logo.get("color").is_some() {
                    import.warnings.push(
                        "logo.color placeholders are stripped; the saved logo uses the theme color"
                            .to_string(),
                    );
                }
                push_unique(&mut import.mapped, "logo");
            }
        }
        "" if source.is_empty() => {}
        "" | "auto" => {
            if !source.is_empty() {
                import.config.logo.source = LogoSource::Builtin {
                    key: source.to_string(),
                };
            }
            push_unique(&mut import.mapped, "logo");
        }
        other => import
            .warnings
            .push(format!("unknown logo type '{other}' was not imported")),
    }
}

fn apply_display(import: &mut Import, display: &Value) {
    let Some(fields) = display.as_object() else {
        return;
    };
    for (key, value) in fields {
        match key.as_str() {
            "padding" => match count(value) {
                Some(padding) => {
                    import.config.layout.padding = padding;
                    push_unique(&mut import.mapped, "display.padding");
                }
                None => import
                    .warnings
                    .push("display.padding is not a number; not imported".to_string()),
            },
            "color" => match value.as_object() {
                Some(colors) => {
                    for (color_key, color_value) in colors {
                        let Some(color) = color_value.as_str().and_then(parse_color) else {
                            import.warnings.push(format!(
                                "display.color.{color_key} is not a color; not imported"
                            ));
                            continue;
                        };
                        let name = match color_key.as_str() {
                            // fastfetch keys are labels; halofetch labels and values
                            // share the value color, so the key color wins.
                            "keys" => {
                                import.config.colors.value = color;
                                "display.color.keys"
                            }
                            "title" => {
                                import.config.colors.title = color;
                                "display.color.title"
                            }
                            "separator" => {
                                import.config.colors.separator = color;
                                "display.color.separator"
                            }
                            other => {
                                import
                                    .warnings
                                    .push(format!("display.color.{other} was not imported"));
                                continue;
                            }
                        };
                        push_unique(&mut import.mapped, name);
                    }
                }
                None => import
                    .warnings
                    .push("display.color was not imported".to_string()),
            },
            other => import
                .warnings
                .push(format!("display.{other} was not imported")),
        }
    }
}

fn apply_modules(import: &mut Import, modules: &[Value]) {
    let mut entries = Vec::new();
    for item in modules {
        let (name, object) = match item {
            Value::String(name) => (name.as_str(), None),
            Value::Object(object) => {
                let Some(name) = object.get("type").and_then(Value::as_str) else {
                    continue;
                };
                if object.get("enabled").and_then(Value::as_bool) == Some(false) {
                    continue;
                }
                (name, Some(object))
            }
            _ => continue,
        };
        let name = name.trim();

        if name.eq_ignore_ascii_case("title") {
            if let Some(format) = object
                .and_then(|object| object.get("format"))
                .and_then(Value::as_str)
            {
                match map_title_format(format) {
                    Some(mapped) => {
                        import.config.title.format = mapped;
                        push_unique(&mut import.mapped, "title.format");
                    }
                    None => import.warnings.push(
                        "title.format has an unknown placeholder; the default was kept".to_string(),
                    ),
                }
            } else {
                push_unique(&mut import.skipped, "title");
            }
            continue;
        }

        match field_for_type(name) {
            Some(field) => {
                let label = object
                    .and_then(|object| object.get("key"))
                    .and_then(Value::as_str)
                    .filter(|key| !key.is_empty() && !key.contains(['{', '#']))
                    .map(clean_label)
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| field.label().to_string());
                entries.push(FieldEntry {
                    field,
                    label,
                    icon: field.icon().to_string(),
                    enabled: true,
                    bar: false,
                });
                push_unique(&mut import.mapped, name);
            }
            None => push_unique(&mut import.skipped, name),
        }
    }

    let split = entries.len().div_ceil(2);
    import.config.fields.right = entries.split_off(split);
    import.config.fields.left = entries;
}

fn map_title_format(format: &str) -> Option<String> {
    let mapped = format
        .replace("{user-name}", "{user}")
        .replace("{host-name}", "{host}");
    let mut rest = mapped.as_str();
    while let Some(start) = rest.find('{') {
        let tail = &rest[start + 1..];
        let end = tail.find('}')?;
        let placeholder = &tail[..end];
        if placeholder != "user" && placeholder != "host" {
            return None;
        }
        rest = &tail[end + 1..];
    }
    if rest.contains('}') {
        return None;
    }
    Some(mapped)
}

fn field_for_type(name: &str) -> Option<Field> {
    let key = name.trim().to_ascii_lowercase();
    let key = match key.as_str() {
        "terminalfont" => "font",
        "cpuusage" => "cpu_usage",
        "cputemp" => "cpu_temp",
        "gpuusage" => "gpu_usage",
        "localip" => "local_ip",
        "loadavg" => "load",
        other => other,
    };
    Field::from_key(key)
}

/// Fastfetch color values are `#RRGGBB` or an SGR fragment such as `38;2;R;G;B`.
fn parse_color(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(color) = Color::from_hex(text) {
        return Some(color);
    }
    let parts: Vec<u32> = text
        .split(';')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    let [red, green, blue] = match parts.as_slice() {
        [38, 2, red, green, blue] | [48, 2, red, green, blue] | [red, green, blue] => {
            [*red, *green, *blue]
        }
        _ => return None,
    };
    Some(Color::new(
        u8::try_from(red).ok()?,
        u8::try_from(green).ok()?,
        u8::try_from(blue).ok()?,
    ))
}

/// Removes fastfetch color placeholders (`$1`, `$2`, ...) from inline logo art.
fn strip_color_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut look = chars.clone();
            if look.peek().is_some_and(|next| matches!(next, '1'..='9')) {
                look.next();
                if !look.peek().is_some_and(|next| next.is_ascii_digit()) {
                    chars = look;
                    continue;
                }
            }
        }
        out.push(ch);
    }
    out
}

/// A fastfetch key often carries its own Nerd Font icon; halofetch already draws
/// the field icon, so the private-use glyphs are removed from the label.
fn clean_label(key: &str) -> String {
    let label: String = key.chars().filter(|ch| !is_private_use(*ch)).collect();
    label.trim().chars().take(24).collect::<String>()
}

fn is_private_use(ch: char) -> bool {
    matches!(
        ch,
        '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}'
    )
}

fn count(value: &Value) -> Option<usize> {
    value
        .as_u64()
        .and_then(|number| usize::try_from(number).ok())
        .or_else(|| {
            value
                .as_f64()
                .filter(|number| number.is_finite() && *number >= 0.0)
                .map(|number| number as usize)
        })
}

fn push_unique(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|existing| existing == item) {
        list.push(item.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jsonc_keeps_comment_markers_inside_strings_and_removes_real_comments() {
        let input = r#"{
  // a real comment
  "url": "https://example.com//path/*not-a-comment*/",
  "quoted": "say \"hi\" and a backslash \\",
  "list": [1, 2, 3,], /* block
  comment */ "object": {"a": true,},
}"#;

        let json = jsonc_to_json(input).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["url"], "https://example.com//path/*not-a-comment*/");
        assert_eq!(value["quoted"], "say \"hi\" and a backslash \\");
        assert_eq!(value["list"], json!([1, 2, 3]));
        assert_eq!(value["object"], json!({"a": true}));
        // The comment markers are part of the URL value and must survive.
        assert!(json.contains("not-a-comment"));
        assert!(!json.contains("// a real comment"));
        assert!(!json.contains("block\n  comment"));
    }

    #[test]
    fn jsonc_rejects_unterminated_comments_and_broken_json() {
        assert!(jsonc_to_json(r#"{"a": 1 /* open"#).is_err());
        assert!(jsonc_to_json(r#"{"a": }"#).is_err());
    }

    #[test]
    fn imports_a_full_fastfetch_configuration() {
        let input = r##"{
  "logo": {"type": "builtin", "source": "arch", "padding": {"left": 1}},
  "display": {"padding": 5, "color": {"keys": "#ff9a98", "separator": "#9d85ff"}},
  "modules": [
    "os",
    {"type": "kernel", "key": "Kernel ver"},
    {"type": "memory", "enabled": true},
    {"type": "localip"},
    {"type": "custom"},
    {"type": "custom", "enabled": true},
    {"type": "title", "format": "{user-name}@{host-name}"},
    {"type": "cpu", "enabled": false}
  ]
}"##;

        let import = parse(input).unwrap();

        assert_eq!(import.config.version, VERSION);
        assert_eq!(
            import.config.logo.source,
            LogoSource::Builtin {
                key: "arch".to_string()
            }
        );
        assert_eq!(import.config.layout.padding, 5);
        assert_eq!(
            import.config.colors.value,
            Color::from_hex("#ff9a98").unwrap()
        );
        assert_eq!(
            import.config.colors.separator,
            Color::from_hex("#9d85ff").unwrap()
        );
        assert_eq!(import.config.title.format, "{user}@{host}");

        let left: Vec<(Field, &str)> = import
            .config
            .fields
            .left
            .iter()
            .map(|entry| (entry.field, entry.label.as_str()))
            .collect();
        assert_eq!(left, [(Field::Os, "OS"), (Field::Kernel, "Kernel ver")]);
        let right: Vec<(Field, &str)> = import
            .config
            .fields
            .right
            .iter()
            .map(|entry| (entry.field, entry.label.as_str()))
            .collect();
        assert_eq!(
            right,
            [(Field::Memory, "Memory"), (Field::LocalIp, "Local IP")]
        );

        assert_eq!(import.skipped, ["custom"]);
        assert!(import.mapped.iter().any(|item| item == "title.format"));
        assert!(import.mapped.iter().any(|item| item == "localip"));
        assert!(import
            .warnings
            .iter()
            .any(|warning| warning.contains("logo.padding")));
        assert!(import.config.validate().is_ok());
    }

    #[test]
    fn maps_file_logos_and_image_protocols() {
        let file = parse(r#"{"logo": {"type": "file", "source": "/tmp/logo.txt"}}"#).unwrap();
        assert_eq!(
            file.config.logo.source,
            LogoSource::File {
                path: "/tmp/logo.txt".to_string()
            }
        );

        let kitty = parse(r#"{"logo": {"type": "kitty", "source": "/tmp/logo.png"}}"#).unwrap();
        assert_eq!(kitty.config.logo.image.path, "/tmp/logo.png");
        assert_eq!(kitty.config.logo.image.protocol, ImageProtocol::Auto);

        for kind in ["sixel", "iterm", "chafa", "raw"] {
            let import = parse(&format!(
                r#"{{"logo": {{"type": "{kind}", "source": "/tmp/logo.png"}}}}"#
            ))
            .unwrap();
            assert_eq!(import.config.logo.image.path, "");
            assert!(import
                .warnings
                .iter()
                .any(|warning| warning.contains("not supported")));
        }

        let none = parse(r#"{"logo": {"type": "none"}}"#).unwrap();
        assert_eq!(none.config.logo.source, LogoSource::None);

        for config in [
            r#"{"logo": {"type": "auto", "source": "arch"}}"#,
            r#"{"logo": {"source": "arch"}}"#,
        ] {
            let import = parse(config).unwrap();
            assert_eq!(
                import.config.logo.source,
                LogoSource::Builtin {
                    key: "arch".to_string()
                }
            );
            assert!(import.mapped.iter().any(|item| item == "logo"));
            assert!(!import
                .warnings
                .iter()
                .any(|warning| warning.contains("logo type")));
        }
        let padding_only = parse(r#"{"logo": {"padding": {"top": 1}}}"#).unwrap();
        assert!(!padding_only.mapped.iter().any(|item| item == "logo"));

        let data = parse(
            r#"{"logo": {"type": "data", "source": "  $1A\n $2B$",
                "color": {"1": "38;2;1;2;3"}}}"#,
        )
        .unwrap();
        assert_eq!(data.logo_data.as_deref(), Some("  A\n B$"));
        assert!(data.mapped.iter().any(|item| item == "logo"));
        assert!(data
            .warnings
            .iter()
            .any(|warning| warning.contains("logo file")));
        assert!(data
            .warnings
            .iter()
            .any(|warning| warning.contains("logo.color")));
    }

    #[test]
    fn small_logo_keeps_auto_small_and_module_aliases_map_to_fields() {
        let import = parse(
            r#"{"logo": {"type": "small", "source": "arch"}, "modules":
                ["terminalfont", "cpuusage", "cputemp", "gpuusage", "local_ip", "loadavg"]}"#,
        )
        .unwrap();

        assert_eq!(
            import.config.logo.source,
            LogoSource::Builtin {
                key: "arch".to_string()
            }
        );
        assert!(import.config.logo.auto_small);
        assert!(import.config.logo.compact);
        let fields: Vec<Field> = import
            .config
            .fields
            .left
            .iter()
            .chain(import.config.fields.right.iter())
            .map(|entry| entry.field)
            .collect();
        assert_eq!(
            fields,
            [
                Field::Font,
                Field::CpuUsage,
                Field::CpuTemp,
                Field::GpuUsage,
                Field::LocalIp,
                Field::Load
            ]
        );
    }

    #[test]
    fn sgr_colors_and_icons_in_keys_are_cleaned() {
        let import = parse(
            r##"{
                "display": {"color": {
                    "keys": "38;2;255;0;10",
                    "title": "#010203",
                    "separator": "48;2;4;5;6"
                }},
                "modules": [
                    {"type": "os", "key": "\uf17c  OS"},
                    {"type": "kernel", "key": "  Kernel"}
                ]
            }"##,
        )
        .unwrap();

        assert_eq!(import.config.colors.value, Color::new(255, 0, 10));
        assert_eq!(import.config.colors.title, Color::new(1, 2, 3));
        assert_eq!(import.config.colors.separator, Color::new(4, 5, 6));
        assert_eq!(import.config.fields.left[0].label, "OS");
        assert_eq!(import.config.fields.right[0].label, "Kernel");
        assert_eq!(parse_color("#010203"), Some(Color::new(1, 2, 3)));
        assert_eq!(parse_color("48;2;1;2;3"), Some(Color::new(1, 2, 3)));
        assert_eq!(parse_color("1;2;3"), Some(Color::new(1, 2, 3)));
        assert_eq!(parse_color("not a color"), None);
        assert_eq!(strip_color_placeholders("a$1b$12c$"), "ab$12c$");
    }

    #[test]
    fn splits_enabled_modules_across_columns_in_order() {
        let import = parse(r#"{"modules": ["os", "host", "user", "kernel", "memory"]}"#).unwrap();
        let left: Vec<Field> = import
            .config
            .fields
            .left
            .iter()
            .map(|entry| entry.field)
            .collect();
        let right: Vec<Field> = import
            .config
            .fields
            .right
            .iter()
            .map(|entry| entry.field)
            .collect();
        assert_eq!(left, [Field::Os, Field::Host, Field::User]);
        assert_eq!(right, [Field::Kernel, Field::Memory]);
    }

    #[test]
    fn skipped_modules_are_recorded_once() {
        let import = parse(
            r#"{"modules": ["custom", "colors", {"type": "custom"},
                "separator", "colors", "title"]}"#,
        )
        .unwrap();
        assert_eq!(import.skipped, ["custom", "colors", "separator", "title"]);
    }

    #[test]
    fn unknown_title_placeholders_keep_the_default_format() {
        let import = parse(
            r#"{"modules": [{"type": "title",
                "format": "{user-name}@{host-name} ({os-name})"}]}"#,
        )
        .unwrap();
        assert_eq!(import.config.title.format, "{user}@{host}");
        assert!(!import.mapped.iter().any(|item| item == "title.format"));
        assert!(import
            .warnings
            .iter()
            .any(|warning| warning.contains("unknown placeholder")));
    }

    #[test]
    fn an_empty_document_keeps_every_default() {
        let import = parse("{}").unwrap();
        assert_eq!(import.config.version, VERSION);
        assert_eq!(import.config, Config::default());
        assert!(import.skipped.is_empty());
        assert!(import.warnings.is_empty());
    }

    #[test]
    fn path_candidates_prefer_jsonc_and_fall_back_to_json() {
        let xdg = "/xdg/config";
        let home = "/home/user";
        let appdata = r"C:\Users\user\AppData\Roaming";

        let candidates = path_candidates(Some(xdg), Some(home), Some(appdata));
        if cfg!(windows) {
            assert_eq!(
                candidates[0],
                Path::new(appdata).join("fastfetch").join("config.jsonc")
            );
            assert_eq!(
                candidates[1],
                Path::new(appdata).join("fastfetch").join("config.json")
            );
        } else {
            assert_eq!(
                candidates[0],
                Path::new(xdg).join("fastfetch").join("config.jsonc")
            );
            assert_eq!(
                candidates[1],
                Path::new(xdg).join("fastfetch").join("config.json")
            );
            assert_eq!(
                candidates[2],
                Path::new(home)
                    .join(".config")
                    .join("fastfetch")
                    .join("config.jsonc")
            );
            let relative = path_candidates(Some("relative"), Some(home), None);
            assert_eq!(
                relative[0],
                Path::new(home)
                    .join(".config")
                    .join("fastfetch")
                    .join("config.jsonc")
            );
        }
        assert!(path_candidates(None, None, None).is_empty());
    }
}
