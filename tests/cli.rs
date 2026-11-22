//! Runs the built binary the way a script does: stdin closed and stdout captured.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// A home directory with no configuration, unique to one test.
fn scratch_home(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("halofetch-cli-{name}-{}", std::process::id()))
}

fn halofetch(name: &str, args: &[&str], force_color: bool) -> Output {
    let home = scratch_home(name);
    let mut command = Command::new(env!("CARGO_BIN_EXE_halofetch"));
    command
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env("APPDATA", &home)
        .stdin(Stdio::null());
    if force_color {
        command.env("FORCE_COLOR", "1");
    }
    command.output().unwrap()
}

fn has_escapes(bytes: &[u8]) -> bool {
    bytes.contains(&0x1b)
}

#[test]
fn redirected_fetch_has_no_escape_codes() {
    let output = halofetch("fetch", &["fetch"], false);
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    assert!(!has_escapes(&output.stdout));
}

#[test]
fn redirected_preset_list_has_no_escape_codes() {
    let output = halofetch("presets", &["preset", "list"], false);
    assert!(output.status.success());
    assert!(!has_escapes(&output.stdout));
}

// The Windows runner has no console, so ANSI support is unknown there.
#[cfg(unix)]
#[test]
fn force_color_keeps_escape_codes_in_a_pipe() {
    let output = halofetch("force", &["fetch"], true);
    assert!(output.status.success());
    assert!(has_escapes(&output.stdout));
}

#[test]
fn a_missing_explicit_config_is_an_error() {
    let path = scratch_home("missing").join("nope.json");
    let output = halofetch(
        "missing",
        &["--config", path.to_str().unwrap(), "fetch"],
        false,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("nope.json"));
}

#[test]
fn an_explicit_config_can_be_created_by_a_preset() {
    let home = scratch_home("create");
    let path = home.join("new.json");
    let output = halofetch(
        "create",
        &[
            "--config",
            path.to_str().unwrap(),
            "preset",
            "apply",
            "dracula",
        ],
        false,
    );
    assert!(output.status.success(), "{output:?}");
    assert!(path.exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn error_reports_are_for_users() {
    let output = halofetch("errors", &["monitor"], false);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("monitor needs an interactive terminal"),
        "{stderr}"
    );
    assert!(!stderr.contains("Location:"), "{stderr}");
    assert!(!stderr.contains("RUST_BACKTRACE"), "{stderr}");
    assert!(!has_escapes(&output.stderr), "{stderr}");
}

#[test]
fn only_enabled_fields_are_collected_and_serialized() {
    let home = scratch_home("enabled-fields");
    std::fs::create_dir_all(&home).unwrap();
    let config = home.join("config.json");
    std::fs::write(
        &config,
        r#"{
            "version": 3,
            "title": {"enabled": false},
            "logo": {"source": {"kind": "none"}},
            "fields": {
                "left": [
                    {"field": "os", "label": "OS", "icon": "", "enabled": true, "bar": false},
                    {"field": "kernel", "label": "Kernel", "icon": "", "enabled": true, "bar": false}
                ],
                "right": []
            }
        }"#,
    )
    .unwrap();

    let output = halofetch(
        "enabled-fields",
        &[
            "--config",
            config.to_str().unwrap(),
            "--format",
            "json",
            "fetch",
        ],
        false,
    );
    assert!(output.status.success(), "{output:?}");
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut keys: Vec<&str> = document["system"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["kernel", "os"]);

    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn importing_a_second_inline_logo_keeps_a_backup() {
    let home = scratch_home("import-backup");
    std::fs::create_dir_all(&home).unwrap();
    let config = home.join("config.json");
    let source = home.join("fastfetch.jsonc");
    std::fs::write(
        &source,
        r#"{"logo": {"type": "data", "source": "AAA"}, "modules": ["os"]}"#,
    )
    .unwrap();

    for data in ["AAA", "BBB"] {
        std::fs::write(
            &source,
            format!(r#"{{"logo": {{"type": "data", "source": "{data}"}}, "modules": ["os"]}}"#),
        )
        .unwrap();
        let output = halofetch(
            "import-backup",
            &[
                "--config",
                config.to_str().unwrap(),
                "config",
                "import-fastfetch",
                source.to_str().unwrap(),
            ],
            false,
        );
        assert!(output.status.success(), "{output:?}");
    }

    let logo = home.join("logos/fastfetch-logo.txt");
    let backup = home.join("logos/fastfetch-logo.txt.bak");
    assert_eq!(std::fs::read_to_string(&logo).unwrap(), "BBB\n");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), "AAA\n");
    assert!(config.with_extension("json.bak").exists());

    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn init_installs_and_removes_an_interactive_hook() {
    let home = scratch_home("init-hook");
    std::fs::create_dir_all(&home).unwrap();

    let install = halofetch("init-hook", &["init", "bash"], false);
    assert!(install.status.success(), "{install:?}");
    let bashrc = home.join(".bashrc");
    let content = std::fs::read_to_string(&bashrc).unwrap();
    assert!(content.contains("case $- in"));
    assert!(content.contains("command -v halofetch"));

    let remove = halofetch("init-hook", &["init", "bash", "--remove"], false);
    assert!(remove.status.success(), "{remove:?}");
    assert!(!std::fs::read_to_string(&bashrc)
        .unwrap()
        .contains(">>> halofetch >>>"));

    let fish = halofetch("init-hook", &["init", "fish"], false);
    assert!(fish.status.success(), "{fish:?}");
    let fish_config = home.join("fish/config.fish");
    assert!(std::fs::read_to_string(&fish_config)
        .unwrap()
        .contains("status is-interactive"));

    std::fs::remove_dir_all(home).unwrap();
}
