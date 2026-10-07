//! Shell, terminal, desktop environment, window manager, display resolution and font.
//!
//! The detection rules are pure functions over an environment lookup and a list of
//! process names, so they can be tested without depending on the host.

use std::fs;
use std::path::PathBuf;

use super::{read_text, sorted_dir};
use crate::info::env_value;

const SHELLS: [&str; 16] = [
    "bash", "zsh", "fish", "nu", "elvish", "xonsh", "dash", "ksh", "mksh", "tcsh", "csh", "sh",
    "ion", "oil", "osh", "pwsh",
];

/// Environment variables that a window manager sets when it starts, with its name.
const WM_SOCKETS: [(&str, &str); 4] = [
    ("HYPRLAND_INSTANCE_SIGNATURE", "Hyprland"),
    ("SWAYSOCK", "Sway"),
    ("NIRI_SOCKET", "niri"),
    ("I3SOCK", "i3"),
];

/// Window manager processes in priority order. A trailing `*` matches a prefix.
const WM_PROCESSES: [(&str, &str); 28] = [
    ("Hyprland", "Hyprland"),
    ("sway", "Sway"),
    ("niri", "niri"),
    ("river", "river"),
    ("labwc", "labwc"),
    ("wayfire", "Wayfire"),
    ("dwl", "dwl"),
    ("atlaswm", "atlasWM"),
    ("atlasWM", "atlasWM"),
    ("gnome-shell", "Mutter"),
    ("mutter", "Mutter"),
    ("kwin_wayland", "KWin"),
    ("kwin_x11", "KWin"),
    ("xfwm4", "Xfwm4"),
    ("marco", "Marco"),
    ("muffin", "Muffin"),
    ("cinnamon", "Muffin"),
    ("openbox", "Openbox"),
    ("fluxbox", "Fluxbox"),
    ("i3", "i3"),
    ("bspwm", "bspwm"),
    ("dwm", "dwm"),
    ("awesome", "awesome"),
    ("qtile", "Qtile"),
    ("herbstluftwm", "herbstluftwm"),
    ("xmonad*", "XMonad"),
    ("icewm", "IceWM"),
    ("weston", "Weston"),
];

/// Desktop environment processes in priority order.
const DE_PROCESSES: [(&str, &str); 11] = [
    ("gnome-shell", "GNOME"),
    ("plasmashell", "KDE Plasma"),
    ("xfce4-session", "Xfce"),
    ("cinnamon-session", "Cinnamon"),
    ("mate-session", "MATE"),
    ("lxqt-session", "LXQt"),
    ("budgie-panel", "Budgie"),
    ("cosmic-comp", "COSMIC"),
    ("lxpanel", "LXDE"),
    ("enlightenment", "Enlightenment"),
    ("cinnamon", "Cinnamon"),
];

/// Environment variables set by terminal emulators, with the emulator name.
const TERMINAL_HINTS: [(&str, &str); 8] = [
    ("KITTY_WINDOW_ID", "kitty"),
    ("ALACRITTY_WINDOW_ID", "Alacritty"),
    ("ALACRITTY_SOCKET", "Alacritty"),
    ("WEZTERM_EXECUTABLE", "WezTerm"),
    ("KONSOLE_VERSION", "Konsole"),
    ("GNOME_TERMINAL_SCREEN", "GNOME Terminal"),
    ("TILIX_ID", "Tilix"),
    ("TERMINATOR_UUID", "Terminator"),
];

/// Login shell that started this process, from the process ancestry and then `$SHELL`.
pub(super) fn shell(chain: &[&str]) -> Option<String> {
    let from_chain = chain.iter().copied().find(|comm| SHELLS.contains(comm));
    from_chain
        .map(str::to_string)
        .or_else(|| env_value("SHELL").map(|path| basename(&path).to_string()))
}

pub(super) fn terminal(chain: &[&str]) -> Option<String> {
    terminal_from(env_value, chain)
}

pub(super) fn de(comms: &[&str]) -> Option<String> {
    de_from(env_value, comms).map(str::to_string)
}

pub(super) fn wm(comms: &[&str]) -> Option<String> {
    wm_from(env_value, comms)
}

/// Connected outputs joined with ", ", each with its first advertised mode.
pub(super) fn resolution() -> Option<String> {
    let modes: Vec<String> = sorted_dir("/sys/class/drm", |name| {
        name.starts_with("card") && name.contains('-')
    })
    .into_iter()
    .filter(|connector| read_text(connector.join("status")).as_deref() == Some("connected"))
    .filter_map(|connector| read_text(connector.join("modes")))
    .filter_map(|modes| modes.lines().next().map(|mode| mode.trim().to_string()))
    .filter(|mode| !mode.is_empty())
    .collect();
    (!modes.is_empty()).then(|| modes.join(", "))
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn terminal_from(env: impl Fn(&str) -> Option<String>, chain: &[&str]) -> Option<String> {
    if let Some(program) = env("TERM_PROGRAM") {
        match program.as_str() {
            "Apple_Terminal" => {}
            "WezTerm" => return Some("WezTerm".to_string()),
            "vscode" => return Some("VS Code".to_string()),
            "ghostty" => return Some("Ghostty".to_string()),
            other => return Some(other.to_string()),
        }
    }
    if let Some((_, name)) = TERMINAL_HINTS
        .iter()
        .find(|(variable, _)| env(variable).is_some())
    {
        return Some(name.to_string());
    }
    if let Some(name) = chain.iter().find_map(|comm| terminal_name(comm)) {
        return Some(name.to_string());
    }
    (env("TERM").as_deref() == Some("linux")).then(|| "Linux console".to_string())
}

/// Display name of a terminal emulator from its process command name.
///
/// The kernel truncates command names to 15 bytes, so the truncated spelling is
/// listed where it differs from the executable name.
fn terminal_name(comm: &str) -> Option<&'static str> {
    Some(match comm {
        "kitty" => "kitty",
        "alacritty" => "Alacritty",
        "wezterm-gui" | "wezterm" => "WezTerm",
        "foot" | "footclient" => "Foot",
        "gnome-terminal-" | "gnome-terminal-server" => "GNOME Terminal",
        "kgx" => "GNOME Console",
        "ptyxis" | "ptyxis-agent" => "Ptyxis",
        "konsole" => "Konsole",
        "xfce4-terminal" => "Xfce Terminal",
        "tilix" => "Tilix",
        "terminator" => "Terminator",
        "st" => "st",
        "urxvt" => "urxvt",
        "rxvt" => "rxvt",
        "xterm" => "xterm",
        "ghostty" => "Ghostty",
        "contour" => "Contour",
        "rio" => "Rio",
        "warp" | "warp-terminal" => "Warp",
        "blackbox" => "Black Box",
        "cool-retro-term" => "cool-retro-term",
        "sakura" => "Sakura",
        "lxterminal" => "LXTerminal",
        "qterminal" => "QTerminal",
        "mate-terminal" => "MATE Terminal",
        "deepin-terminal" => "Deepin Terminal",
        "yakuake" => "Yakuake",
        "tmux" | "tmux: server" => "tmux",
        "screen" => "screen",
        "zellij" => "Zellij",
        "sshd" => "SSH",
        "login" | "agetty" => "Linux console",
        _ => return None,
    })
}

fn de_from(env: impl Fn(&str) -> Option<String>, comms: &[&str]) -> Option<&'static str> {
    env("XDG_CURRENT_DESKTOP")
        .and_then(|list| list.split(':').find_map(normalize_desktop))
        .or_else(|| {
            env("DESKTOP_SESSION").and_then(|session| normalize_desktop(basename(&session)))
        })
        .or_else(|| {
            DE_PROCESSES
                .iter()
                .find(|(pattern, _)| process_matches(comms, pattern))
                .map(|(_, name)| *name)
        })
}

fn wm_from(env: impl Fn(&str) -> Option<String>, comms: &[&str]) -> Option<String> {
    let name = WM_SOCKETS
        .iter()
        .find(|(variable, _)| env(variable).is_some())
        .map(|(_, name)| *name)
        .or_else(|| {
            WM_PROCESSES
                .iter()
                .find(|(pattern, _)| process_matches(comms, pattern))
                .map(|(_, name)| *name)
        })?;

    let session = env("XDG_SESSION_TYPE").map(|value| value.to_ascii_lowercase());
    let protocol = match session.as_deref() {
        Some("wayland") => Some("Wayland"),
        Some("x11") => Some("X11"),
        _ => env("WAYLAND_DISPLAY").map(|_| "Wayland"),
    };
    Some(match protocol {
        Some(protocol) => format!("{name} ({protocol})"),
        None => name.to_string(),
    })
}

/// Whether any command name matches the pattern. A trailing `*` matches a prefix.
fn process_matches(comms: &[&str], pattern: &str) -> bool {
    comms.iter().any(|comm| match pattern.strip_suffix('*') {
        Some(prefix) => comm.starts_with(prefix),
        None => *comm == pattern,
    })
}

/// Canonical desktop name for one `XDG_CURRENT_DESKTOP` item or session name.
fn normalize_desktop(item: &str) -> Option<&'static str> {
    Some(match item.trim().to_ascii_lowercase().as_str() {
        "gnome" => "GNOME",
        "kde" | "plasma" => "KDE Plasma",
        "xfce" => "Xfce",
        "x-cinnamon" | "cinnamon" => "Cinnamon",
        "mate" => "MATE",
        "lxqt" => "LXQt",
        "lxde" => "LXDE",
        "budgie" | "budgie-desktop" => "Budgie",
        "deepin" => "Deepin",
        "pantheon" => "Pantheon",
        "cosmic" => "COSMIC",
        "unity" => "Unity",
        "enlightenment" => "Enlightenment",
        _ => return None,
    })
}

/// Font family from terminal configuration files. Each file is read at most once.
pub(super) fn font() -> Option<String> {
    let home = super::home_dir()?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));

    let sources: [(PathBuf, FontParser); 9] = [
        (config.join("kitty/kitty.conf"), parse_kitty),
        (config.join("alacritty/alacritty.toml"), parse_alacritty),
        (config.join("alacritty/alacritty.yml"), parse_alacritty),
        (config.join("wezterm/wezterm.lua"), parse_wezterm),
        (config.join("ghostty/config"), parse_ghostty),
        (config.join("foot/foot.ini"), parse_foot),
        (home.join(".Xresources"), parse_xresources),
        (home.join(".Xdefaults"), parse_xresources),
        (config.join("konsolerc"), parse_konsole),
    ];
    sources
        .iter()
        .find_map(|(path, parse)| parse(&fs::read_to_string(path).ok()?))
}

type FontParser = fn(&str) -> Option<String>;

fn parse_kitty(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("font_family")?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        clean_font(rest)
    })
}

/// Handles both the TOML (`family = "X"`) and YAML (`family: X`) forms.
fn parse_alacritty(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("family")?.trim_start();
        let value = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':'))?;
        clean_font(value)
    })
}

/// `wezterm.font("Name")` or `wezterm.font_with_fallback({"Name", ...})`.
fn parse_wezterm(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let at = line.find("wezterm.font")?;
        first_quoted(&line[at..])
    })
}

fn first_quoted(text: &str) -> Option<String> {
    let quote = text.chars().find(|c| *c == '"' || *c == '\'')?;
    let start = text.find(quote)? + quote.len_utf8();
    let inner = &text[start..];
    let end = inner.find(quote)?;
    clean_font(&inner[..end])
}

fn parse_ghostty(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("font-family")?.trim_start();
        clean_font(rest.strip_prefix('=')?)
    })
}

/// `font=Family:size=11`; the size and style after the colon are dropped.
fn parse_foot(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("font")?.trim_start();
        let value = rest.strip_prefix('=')?;
        clean_font(value.split(':').next()?)
    })
}

/// `*font: xft:Family:size=11` or `XTerm*font: Family`. X core font names (leading `-`) are skipped.
fn parse_xresources(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if !key.trim().ends_with("font") {
            return None;
        }
        let value = value.trim();
        let value = value.strip_prefix("xft:").unwrap_or(value);
        if value.starts_with('-') {
            return None;
        }
        clean_font(value.split(':').next()?)
    })
}

/// `Font=Family,10,-1,5,...` in konsolerc.
fn parse_konsole(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("Font=")?;
        clean_font(value.split(',').next()?)
    })
}

fn clean_font(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'')
        .trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_from(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn shell_comes_from_the_ancestor_chain() {
        assert_eq!(
            shell(&["fish", "kitty", "systemd"]).as_deref(),
            Some("fish")
        );
        assert_eq!(shell(&["sudo", "zsh", "kitty"]).as_deref(), Some("zsh"));
        assert!(SHELLS.contains(&"sh"));
    }

    #[test]
    fn terminal_env_hints_come_first() {
        let env = env_from(&[("TERM_PROGRAM", "WezTerm")]);
        assert_eq!(
            terminal_from(env, &["bash", "kitty"]).as_deref(),
            Some("WezTerm")
        );
        let env = env_from(&[("TERM_PROGRAM", "vscode")]);
        assert_eq!(terminal_from(env, &[]).as_deref(), Some("VS Code"));
        let env = env_from(&[("KITTY_WINDOW_ID", "3")]);
        assert_eq!(terminal_from(env, &["bash"]).as_deref(), Some("kitty"));
        let env = env_from(&[("TERM_PROGRAM", "Apple_Terminal")]);
        assert_eq!(
            terminal_from(env, &["bash", "foot"]).as_deref(),
            Some("Foot")
        );
    }

    #[test]
    fn terminal_walks_past_shells_and_wrappers() {
        let env = env_from(&[]);
        assert_eq!(
            terminal_from(env, &["bash", "sudo", "gnome-terminal-", "systemd"]).as_deref(),
            Some("GNOME Terminal")
        );
        let env = env_from(&[]);
        assert_eq!(
            terminal_from(env, &["zsh", "tmux: server", "alacritty"]).as_deref(),
            Some("tmux")
        );
        let env = env_from(&[]);
        assert_eq!(
            terminal_from(env, &["bash", "sshd"]).as_deref(),
            Some("SSH")
        );
    }

    #[test]
    fn terminal_falls_back_to_linux_console_only_for_tty_term() {
        let env = env_from(&[("TERM", "linux")]);
        assert_eq!(
            terminal_from(env, &["bash"]).as_deref(),
            Some("Linux console")
        );
        let env = env_from(&[("TERM", "xterm-256color")]);
        assert_eq!(terminal_from(env, &["bash"]), None);
    }

    #[test]
    fn terminal_names_include_truncated_comm() {
        assert_eq!(terminal_name("gnome-terminal-"), Some("GNOME Terminal"));
        assert_eq!(terminal_name("footclient"), Some("Foot"));
        assert_eq!(terminal_name("bash"), None);
    }

    #[test]
    fn desktop_names_are_normalized() {
        assert_eq!(normalize_desktop("X-Cinnamon"), Some("Cinnamon"));
        assert_eq!(normalize_desktop("KDE"), Some("KDE Plasma"));
        assert_eq!(normalize_desktop("GNOME"), Some("GNOME"));
        assert_eq!(normalize_desktop("ubuntu"), None);
        assert_eq!(normalize_desktop("sway"), None);
    }

    #[test]
    fn de_uses_env_list_then_session_then_processes() {
        let env = env_from(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")]);
        assert_eq!(de_from(env, &[]), Some("GNOME"));

        let env = env_from(&[
            ("XDG_CURRENT_DESKTOP", "sway"),
            ("DESKTOP_SESSION", "/x/plasma"),
        ]);
        assert_eq!(de_from(env, &["plasmashell"]), Some("KDE Plasma"));

        let env = env_from(&[]);
        assert_eq!(de_from(env, &["systemd", "xfce4-session"]), Some("Xfce"));

        let env = env_from(&[]);
        assert_eq!(de_from(env, &["bash"]), None);
    }

    #[test]
    fn wm_prefers_sockets_then_processes_with_protocol_suffix() {
        let env = env_from(&[
            ("SWAYSOCK", "/run/sway.sock"),
            ("XDG_SESSION_TYPE", "wayland"),
        ]);
        assert_eq!(wm_from(env, &[]).as_deref(), Some("Sway (Wayland)"));

        let env = env_from(&[("XDG_SESSION_TYPE", "x11")]);
        assert_eq!(wm_from(env, &["i3"]).as_deref(), Some("i3 (X11)"));

        let env = env_from(&[("WAYLAND_DISPLAY", "wayland-1")]);
        assert_eq!(
            wm_from(env, &["gnome-shell"]).as_deref(),
            Some("Mutter (Wayland)")
        );

        let env = env_from(&[]);
        assert_eq!(wm_from(env, &["bash"]), None);
    }

    #[test]
    fn wm_prefix_patterns_match() {
        let env = env_from(&[]);
        assert_eq!(
            wm_from(env, &["xmonad-x86_64-l"]).as_deref(),
            Some("XMonad")
        );
    }

    #[test]
    fn font_parsers_read_each_format() {
        assert_eq!(
            parse_kitty("font_size 11\nfont_family JetBrains Mono\n").as_deref(),
            Some("JetBrains Mono")
        );
        assert_eq!(parse_kitty("font_family_bold Foo\n"), None);
        assert_eq!(
            parse_alacritty("[font.normal]\nfamily = \"Iosevka\"\n").as_deref(),
            Some("Iosevka")
        );
        assert_eq!(
            parse_alacritty("font:\n  normal:\n    family: Hack\n").as_deref(),
            Some("Hack")
        );
        assert_eq!(
            parse_wezterm("config.font = wezterm.font(\"Fira Code\")\n").as_deref(),
            Some("Fira Code")
        );
        assert_eq!(
            parse_ghostty("font-family = Berkeley Mono\n").as_deref(),
            Some("Berkeley Mono")
        );
        assert_eq!(
            parse_foot("[main]\nfont=Monospace:size=11\n").as_deref(),
            Some("Monospace")
        );
        assert_eq!(
            parse_xresources("*font: xft:DejaVu Sans Mono:size=11\n").as_deref(),
            Some("DejaVu Sans Mono")
        );
        assert_eq!(parse_xresources("*font: -misc-fixed-medium-r\n"), None);
        assert_eq!(
            parse_konsole("[Desktop Entry]\nFont=Hack,10,-1,5,50,0,0,0,0,0\n").as_deref(),
            Some("Hack")
        );
    }

    #[test]
    fn empty_font_values_are_absent() {
        assert_eq!(parse_kitty("font_family   \n"), None);
        assert_eq!(parse_konsole("Font=,10\n"), None);
    }
}
