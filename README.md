<div align="center">

# HaloFetch

### Your Linux, composed — not dumped.

A fast system fetch, a live hardware monitor, and a real interactive shell<br>
sharing one carefully arranged terminal canvas.

[![CI](https://github.com/mafuzyk/halofetch/actions/workflows/ci.yml/badge.svg)](https://github.com/mafuzyk/halofetch/actions/workflows/ci.yml)
[![Release](https://github.com/mafuzyk/halofetch/actions/workflows/release.yml/badge.svg)](https://github.com/mafuzyk/halofetch/actions/workflows/release.yml)
[![Rust](https://img.shields.io/badge/Rust-2021-e86b37?logo=rust&logoColor=white)](Cargo.toml)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-8b5cf6)](LICENSE)

[Quick start](#quick-start) · [Choose a mode](#choose-a-mode) · [Customize](#make-it-yours) · [Install](#installation) · [Contribute](#development)

</div>

HaloFetch treats terminal output like a layout instead of a list. Its scenes place a colored ASCII logo next to the information you care about; its live monitor keeps that composition above a PTY-backed shell you can use normally.

It began as a companion to [atlasWM](https://github.com/mafuzyk/atlaswm), but it has no desktop or window-manager allegiance. Linux is the primary target. Windows 10 and 11 are supported as well, see [Windows](#windows).

**Renamed from atlasfetch.** The command is now `halofetch`. On the first run the configuration directory moves automatically from `atlasfetch` to `halofetch` (`~/.config/atlasfetch` or `%APPDATA%\atlasfetch`), unless `--config` is given. `ATLASFETCH_SRC` still works as a fallback for `HALOFETCH_SRC`.

## Why HaloFetch?

- **Two modes.** Print one fetch and exit, or stay as a live monitor with a shell underneath.
- **Three scenes, one renderer.** `classic`, `side` and `dashboard` share the same rows, gauges and fallbacks. The editor preview uses the same code as the output.
- **Editable without JSON.** The setup editor changes palettes, logo, fields, labels, icons, spacing and startup mode, with a live preview.
- **456 embedded logos.** 78 of them have a compact variant for narrow terminals. Custom logos can be pasted in or read from a file.
- **32 fields, no invented values.** CPU, GPU, memory, disk, sensors, battery, packages, desktop, network and more. A value that cannot be read is hidden, or shown as `n/a` when you ask for that.
- **Safe configuration.** Versioned configuration (version 3), validation before saving, atomic writes, automatic migration from versions 1 and 2 with a backup, and invalid files moved aside instead of deleted.
- **Scriptable.** Static output, separate configuration profiles with `--config`, versioned JSON output and a built-in benchmark.

## Choose a mode

### 1. Fetch and leave

```bash
halofetch fetch
```

Renders one fetch and exits. Use it for terminal greetings, screenshots, dotfiles, or showing everyone your kernel version.

| Scene | Also accepted as | Shape |
|---|---|---|
| `classic` | | Logo centered between two information panels |
| `side` | `classicfetch`, `fastfetch` | Logo on the left, information on the right |
| `dashboard` | `cockpit` | Framed boxes: logo and System, then Resources with gauges |

Try a scene without changing your configuration:

```bash
halofetch fetch --scene side
halofetch --scene dashboard
```

`halofetch --scene dashboard` prints once and exits. To keep a scene on screen, use `halofetch fetch --watch` (`-w`): it redraws the scene in place every `startup.interval_ms` (1000 ms by default) until `q`, `Esc` or `Ctrl+C`. `-i` or `--interval` sets another interval, from 100 to 60000 ms. `halofetch monitor` adds the embedded shell below the scene.

The scenes adapt to the width of the terminal:

- `classic` shares the room beside the logo between the two panels. When a panel would have to cut its values below 10 columns, the logo, title and every row stack into one centered column.
- `side` places the logo above the information when the logo and at least 30 columns of information do not fit side by side.
- `dashboard` places the logo box above the System box when there is no room beside it. The System box uses two columns when it is wide enough, and the Resources box uses two columns from an inner width of 70.

Static output is 100 columns wide unless the terminal says otherwise. `COLUMNS` sets the width when output is not a terminal.

```text
                               ....             root@vm
                .',:clooo:  .:looooo:.          ───────
             .;looooooooc  .oooooooooo'           OS        Ubuntu 24.04.5 LTS
          .;looooool:,''.  :ooooooooooc           Kernel    6.18.44-fc-v77
         ;looool;.         'oooooooooo,           Packages  827 (dpkg)
        ;clool'             .cooooooc.  ,,        Shell     bash
           ...                ......  .:oo,       Terminal  Linux console
    .;clol:,.                        .loooo'      Uptime    59m
   :ooooooooo,                        'ooool      CPU       Intel Xeon (4)
  'ooooooooooo.                        loooo.     Memory    0.60 / 15.7 GiB (4%)
  'ooooooooool                         coooo.     Disk      10.7 / 252 GiB (4%)
   ,loooooooc.                        .loooo.
     .,;;;'.                          ;ooooc    ███████████████
         ...                         ,ooool.
      .cooooc.              ..',,'.  .cooo.
        ;ooooo:.           ;oooooooc.  :l.
         .coooooc,..      coooooooooo.
           .:ooooooolc:. .ooooooooooo'
             .':loooooo;  ,oooooooooc
                 ..';::c'  .;loooo:'
```

The sample is `halofetch fetch --scene side` with `NO_COLOR=1`. Nerd Font icons and powerline separators may show as blanks in a font without them.

### 2. Live monitor

```bash
halofetch monitor
```

The upper region shows the scene and refreshes its values at the configured interval. The lower region is a real shell in a pseudo-terminal, drawn through a VT100 parser.

```text
╭─ HaloFetch · live 1000ms ─────────────────────────────╮
│ scene with live values, at most 60% of the screen      │
╰────────────────────────────────────────────────────────╯
╭─ Shell · Ctrl+Q closes workspace ──────────────────────╮
│ $                                                      │
╰────────────────────────────────────────────────────────╯
```

- Keys go to the shell, including completion, history, `sudo`, SSH, editors and control keys. The only key the workspace keeps is `Ctrl+Q`, which closes it.
- The shell is `$SHELL -i`. Fish starts without its greeting and without its terminal query. On Windows the shell is PowerShell or cmd unless `$SHELL` names a file; see [Windows](#windows).
- Values refreshed in place: uptime, load, process count, memory, swap, disk, battery, CPU temperature, backlight, CPU usage and GPU usage. Package, font and desktop detection is not repeated. Which of these exist on Windows is listed in [Windows](#windows).
- The screen is redrawn after each refresh and whenever the shell prints something, not in a tight loop.
- `halofetch monitor -i 500` refreshes every 500 ms. The interval can be 100 to 60000 ms.
- Monitor needs an interactive terminal. In a pipe, use `fetch`.

Set **Startup → Mode** to *monitor* in the editor and a plain `halofetch` opens this workspace in an interactive terminal. `halofetch fetch` always prints one static fetch.

## Quick start

```bash
halofetch
```

In an interactive terminal without a configuration file, this opens the setup editor. Saving creates `~/.config/halofetch/config.json`. Quitting without saving keeps the defaults and writes nothing. Return to the editor whenever you like:

```bash
halofetch config
```

The editor has five sections:

1. **Appearance**: theme and palette, gradient, info style, title, separator and value colors, and saving the palette under a name.
2. **Logo**: source (automatic, a built-in logo, a file, or none), the compact variant for narrow terminals, and the file path. Type to filter the built-in logos.
3. **Fields**: the left and right panels. Show or hide entries, reorder them, move them between panels, edit labels, icons and bars, add or remove fields.
4. **Layout**: scene, gap, padding, cascade, maximum value width, hiding empty fields, color blocks, the title and its format and separator.
5. **Startup**: the startup mode (fetch or monitor) and the refresh interval.

The default `cascade` is `0`, which keeps the rows of the classic scene in a straight column. Values from 1 to 10 step the rows inward. A configuration saved before this default keeps its own value.

The editor needs a terminal of at least 60 × 18. Smaller windows show a resize message, and `q` still quits. From 110 columns, the menu, the settings and the preview sit side by side. Narrower terminals show the preview below the settings.

Keys:

| Key | Action |
|---|---|
| `↑` `↓` | Move between sections in the menu, or between rows in a section |
| `→` `Enter` `Tab` | Open the selected section |
| `Esc` `Tab` | Return to the section menu |
| `←` `→` | Change a choice or a number |
| `Enter` | Edit a text value, open a list, or activate a row |
| `Space` | Toggle a switch; in Fields, show or hide the selected entry |
| `Shift` + `↑` `↓` | Move the selected field within its panel, or across the edge into the other panel |
| `Shift` + `←` `→` | Move the selected field to the left or right panel |
| `a` or `Insert` (Fields) | Add a field after the selected one |
| `Delete` or `Backspace` (Fields) | Remove the selected field |
| `Ctrl+Z` | Restore the last removed field |
| paste (several lines, Logo section) | Use the pasted art as the logo |
| `F1` or `?` | Keys help |
| `F2` | Full-screen preview; any key returns |
| `Ctrl+S` | Save and exit; the configuration is checked first |
| `q` | Quit. With unsaved changes, a prompt offers Save, Discard or Cancel |
| `Ctrl+C` | Quit without saving, no prompt |

## Make it yours

### Fields and panels

HaloFetch has 32 fields in seven groups: System, Software, Desktop, Hardware, Resources, Network and Power. Each entry in a panel can be shown or hidden, renamed, given another icon, drawn as a bar when its field has a gauge, and moved within or between the left and right panels. Labels are limited to 24 characters and icons to 4.

Fields such as CPU usage and GPU usage are live values. They appear only in monitor mode.

When a panel is too narrow for a CPU or GPU value, the value is shortened before it is cut with an ellipsis. The CPU drops its clock frequency first, then its core count. The GPU drops the words Corporation, Inc. and Series.

### Themes and ASCII

Choose one of the 27 built-in palettes, enter your own colors as `#RRGGBB` values separated by spaces, change the gradient direction, or save the current palette under a name. Saved palettes appear next to the built-in ones and can be applied from the command line with `halofetch preset apply NAME`. The default palette is `amethyst`.

The logo can be detected from your distribution, chosen from the 456 embedded logos, read from a file, or pasted. A pasted logo is saved as `custom-logo.txt` next to the configuration when you save. A file named after a logo in `logos/` inside the configuration directory replaces the embedded logo with the same name.

Nerd Fonts are recommended for the default icons. The layout works without them. When icons show as replacement glyphs, which is the usual case with the default Windows Terminal font, set `"icons": "unicode"` in the `layout` section to draw one plain symbol per field group instead, or `"icons": "none"` to draw no icons. The **Icons** row in the editor's Layout section cycles through the same three choices. Older `true` and `false` values still load as `"nerd"` and `"none"`.

### Configuration without fear

The default configuration lives at:

```text
~/.config/halofetch/config.json
```

Run `halofetch config path` to print the path in use. The configuration is validated before it is saved and written atomically. Values that are out of range in a file are limited to their range when loaded.

Use another profile without touching your daily setup:

```bash
halofetch -c ./screenshots.json config
halofetch -c ./screenshots.json fetch
```

Minimal shape of the version 3 schema. Field entries are shown as two per panel; the default file lists all of them.

```json
{
  "version": 3,
  "scene": "classic",
  "startup": {
    "mode": "fetch",
    "interval_ms": 1000
  },
  "logo": {
    "source": {
      "kind": "auto"
    },
    "gradient": "horizontal",
    "auto_small": true
  },
  "colors": {
    "palette": [
      "#C084FC",
      "#A78BFA",
      "#818CF8",
      "#6366F1",
      "#4F46E5"
    ],
    "title": "#FF9A98",
    "separator": "#9D85FF",
    "value": "#F5DCE3"
  },
  "title": {
    "enabled": true,
    "format": "{user}@{host}",
    "separator": "─"
  },
  "layout": {
    "style": "powerline",
    "gap": 3,
    "padding": 2,
    "cascade": 0,
    "icons": "nerd",
    "max_value_width": 0,
    "hide_empty": true,
    "color_blocks": true
  },
  "fields": {
    "left": [
      {
        "field": "os",
        "label": "OS",
        "icon": "",
        "enabled": true,
        "bar": false
      },
      {
        "field": "kernel",
        "label": "Kernel",
        "icon": "",
        "enabled": true,
        "bar": false
      }
    ],
    "right": [
      {
        "field": "uptime",
        "label": "Uptime",
        "icon": "",
        "enabled": true,
        "bar": false
      },
      {
        "field": "cpu",
        "label": "CPU",
        "icon": "",
        "enabled": true,
        "bar": false
      }
    ]
  },
  "custom_palettes": {}
}
```

Older files are upgraded on first load:

- A version 1 or version 2 file is migrated to version 3 and written back. The original is kept next to it as `config.json.bak`, or `config.json.bak.1`, `.bak.2` and so on when a backup already exists.
- A file that cannot be used is moved aside as `config.json.invalid` (numbered the same way), and the defaults are used. This covers invalid JSON, an unknown version, a field key that is not a known field, and a value of the wrong type. Nothing is written back in that case.

The setup editor is the recommended way to edit the file. See [DOCS.md](DOCS.md) for the complete schema, the validation ranges, the migration rules and the architecture.

## Command map

| Command | What it does |
|---|---|
| `halofetch` | Interactive: opens the editor on the first run, then follows the startup mode. Otherwise prints the static fetch |
| `halofetch fetch` | Print one static fetch and exit |
| `halofetch fetch --scene side` | Print one fetch with the given scene |
| `halofetch --format json` | Print the detected system information as JSON |
| `halofetch monitor` | Open the live workspace |
| `halofetch monitor -i 500` | Open the live workspace, refreshing every 500 ms |
| `halofetch fetch --watch` | Redraw the scene in place until `q`, `Esc` or `Ctrl+C`; `-i` sets the interval |
| `halofetch config` | Open the setup editor (`config edit` is the same) |
| `halofetch config path` | Print the active configuration path |
| `halofetch config reset` | After confirmation, move the configuration aside as `.bak` and open the editor with defaults |
| `halofetch preset list` | List built-in and custom palettes |
| `halofetch preset apply dracula` | Use a palette for the logo colors and save |
| `halofetch logos list` | List logo keys: embedded logos and files in the user logo directory, sorted |
| `halofetch logos show` | Print the logo this machine's configuration selects, colored with the palette |
| `halofetch logos show arch` | Print one logo colored with the palette |
| `halofetch benchmark` | Measure information collection and the full render (`-n` sets the runs, 5 by default) |
| `halofetch update` | Update from the source checkout, or install the latest release binary (`--release` forces the download) |

`--scene`, `--format` and `-c, --config PATH` work before or after the command. `halofetch --help` is the authoritative reference.

## Installation

### Prebuilt binaries

Each release publishes a static Linux binary (`x86_64-unknown-linux-musl`, runs on any distribution), a glibc build (`x86_64-unknown-linux-gnu`) and a Windows build, each with a `.sha256` file, on the [release page](https://github.com/mafuzyk/halofetch/releases/latest).

```bash
version=v3.0.0
archive=halofetch-$version-x86_64-unknown-linux-musl.tar.gz
curl -fLO "https://github.com/mafuzyk/halofetch/releases/download/$version/$archive"
curl -fLO "https://github.com/mafuzyk/halofetch/releases/download/$version/$archive.sha256"
sha256sum -c "$archive.sha256"
tar -xzf "$archive"
install -Dm755 halofetch ~/.local/bin/halofetch
```

Make sure `~/.local/bin` is in `PATH`. For Windows, see [Windows](#windows).

### Build from source

```bash
git clone https://github.com/mafuzyk/halofetch.git
cd halofetch
cargo build --release --locked
install -Dm755 target/release/halofetch ~/.local/bin/halofetch
```

Make sure `~/.local/bin` is in `PATH`. Rust and Cargo are needed only to build from source.

### Nix

```bash
nix run github:mafuzyk/halofetch
nix profile install github:mafuzyk/halofetch
```

### Update

```bash
halofetch update
```

`halofetch update` checks for a source checkout first through `HALOFETCH_SRC`, the current directory, the directories around the executable, or a few common paths under the home directory (`ATLASFETCH_SRC` is used when `HALOFETCH_SRC` is not set). With a checkout it refuses local changes, runs `git pull --rebase --autostash`, builds with `cargo build --release --locked` and installs to `~/.local/bin/halofetch`. Without one, or with `--release`, it asks GitHub for the latest release, downloads the archive for this platform with `curl`, checks it against the published SHA-256, extracts it with `tar` and replaces the running executable in place. It does nothing when the installed version is already the latest, and refuses a copy managed by Nix. Prebuilt binaries exist for x86_64 Linux and Windows.

### Start with your shell

Static mode is the sensible choice for a greeting:

```fish
# ~/.config/fish/config.fish
if status is-interactive
    halofetch fetch
end
```

```bash
# ~/.bashrc
if [[ $- == *i* ]]; then
    halofetch fetch
fi
```

```zsh
# ~/.zshrc
if [[ -o interactive ]]; then
    halofetch fetch
fi
```

Starting the live workspace from the shell it launches would nest one shell inside another, so use `fetch` in shell startup files.

## Windows

HaloFetch runs on Windows 10 and 11 as a native `x86_64-pc-windows-msvc` program. The fetch, the editor, the JSON output and the monitor work as on Linux. Fields that have no Windows source are hidden, as listed below.

### Install

Download `halofetch-v<version>-x86_64-pc-windows-msvc.zip` and its `.sha256` file from the release page. Check the archive against the published hash:

```powershell
Get-FileHash -Algorithm SHA256 .\halofetch-v3.0.0-x86_64-pc-windows-msvc.zip
```

Extract `halofetch.exe` from the archive into a folder on your `PATH`. The archive also contains `LICENSE` and `README.md`.

To build from source, install Rust with the MSVC toolchain and the Visual Studio C++ build tools, then run:

```powershell
git clone https://github.com/mafuzyk/halofetch.git
cd halofetch
cargo install --path . --locked
```

`cargo install` places `halofetch.exe` in `%USERPROFILE%\.cargo\bin`. To greet every PowerShell session, add `halofetch fetch` to your PowerShell profile (`$PROFILE`).

### Configuration

The configuration is `%APPDATA%\halofetch\config.json`. `halofetch config path` prints the path in use, and `-c` selects another file as on Linux. Your own logos go in the `logos` folder next to it. A logo pasted in the editor is saved there as `custom-logo.txt`.

### Updating

`halofetch update` downloads the latest release and replaces `halofetch.exe` where it is installed (it uses `curl.exe` and `tar.exe`, included in Windows 10 and 11). With a source checkout (`git` and `cargo` on `PATH`) it builds instead and installs to `%LOCALAPPDATA%\Programs\halofetch\halofetch.exe`; `--release` forces the download. Windows does not let a running program be overwritten, so the installed copy is renamed to `halofetch.exe.old` first, and that file is removed by the next update. When the install folder is not on `PATH`, the update prints the line to add.

### What is shown

| Fields | Where the value comes from |
|---|---|
| OS, Kernel, Arch, Locale, Uptime | Registry product name, display version and build; native system information; the user's default locale; time since boot |
| User, Host, Device | The user name; the computer name; the BIOS manufacturer and product from the registry |
| CPU, CPU Usage | Processor name and clock from the registry, thread count; CPU usage is live, monitor only |
| GPU | Display adapter names from the registry |
| VRAM | Total dedicated video memory of the largest dedicated adapter, from the registry, with the usage from the `GPU Adapter Memory` performance counter. Only the total is shown when the counter is unavailable |
| Memory, Swap, Disk | Physical memory; the page file as swap; the system drive |
| Battery | The system power status. Absent on machines without a battery |
| Resolution | Attached displays and their current modes |
| Local IP | An up Ethernet or Wi-Fi adapter when there is one, otherwise another up adapter. Link-local `169.254.x.x` addresses are skipped |
| WM | Always `DWM`, the Desktop Window Manager, the compositor on Windows 8 and later |
| Packages | Scoop and Chocolatey counts, such as `34 (scoop), 12 (choco)`. Winget and Microsoft Store packages are not counted |
| Shell, Terminal, Processes | One process list: the nearest known shell in the parent processes (PowerShell, Windows PowerShell, cmd, bash, zsh, fish, nu, elvish, xonsh), and the terminal that hosts it |

These fields are Linux-only and stay hidden on Windows: Flatpak, Snap, Font, DE, CPU Temp, GPU Usage, Load, Wi-Fi and Brightness. Turn off **Hide empty fields** in the editor to show them as `n/a`.

Icons and powerline separators need a Nerd Font selected in your terminal. Windows Terminal is recommended because it draws the 24-bit colors of the themes. If icons show as replacement glyphs there, set `layout.icons` to `"unicode"` for plain symbols or to `"none"` to draw no icons. In the classic console, HaloFetch enables ANSI escape support at startup. If the console refuses it, static output is plain text.

### Monitor

`halofetch monitor` starts the shell from `$SHELL` when that names an existing file. Otherwise it uses `pwsh.exe`, then `powershell.exe`, then `%COMSPEC%` (normally `cmd.exe`). Bash, zsh and fish start with `-i`. PowerShell and cmd start without arguments. Git Bash sets `$SHELL` to a path Windows cannot open, so PowerShell is used unless you set `SHELL` to a Windows path. `Ctrl+Q` closes the workspace.

### Pasting a logo

Legacy consoles do not deliver a paste as a paste event, so the editor offers **Paste logo from clipboard** in the Logo section on Windows. Press `Enter` on it to use the clipboard text as the logo, which is saved with the configuration when you save.

## Machine-readable output

```bash
halofetch --format json | jq '.system.cpu'
```

The output is a JSON object with `schema_version` 2, `system` and `gauges`. A shortened example from a real run:

```json
{
  "schema_version": 2,
  "system": {
    "os": "Ubuntu 24.04.5 LTS",
    "kernel": "6.18.44-fc-v77",
    "cpu": "Intel Xeon (4)",
    "memory": "0.63 / 15.7 GiB (4%)"
  },
  "gauges": {
    "memory": {
      "used": 674316288,
      "total": 16876511232,
      "percent": 4.0
    }
  }
}
```

- `system` holds every detected field as its display text, in the order of the field list. Fields that cannot be read are absent. The panel configuration does not filter this output.
- `gauges` holds numbers for the fields that have them. `memory`, `swap`, `disk` and `vram` are objects with `used` and `total` in bytes and a `percent`. `cpu_percent` and `gpu_percent` come from the live monitor, so one-shot output leaves them out. `cpu_temp_celsius`, `battery_percent` and `brightness_percent` appear when the hardware reports them.
- Scripts should check for a key before reading it.

## Development

HaloFetch collects one model of the system, lays it out in a scene, and draws it through one styled canvas. The static output, the editor preview and the live monitor all use that path.

```text
/proc, /sys, /etc, environment, local commands
                    │
                    ▼
        info::collect ──► SysInfo ──► logo + Config
                    │                       │
                    │                       ▼
                    │               render::scene
                    │                       │
     info::refresh_live (monitor)           ▼
                    │           ANSI · plain text · ratatui (editor, monitor)
```

Run the same quality gate as CI:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

Important modules:

| Path | Responsibility |
|---|---|
| `src/main.rs` | Command dispatch, static fetch, first run |
| `src/cli.rs` | Commands and flags (clap); legacy flags are hidden |
| `src/config.rs` | Schema version 3, defaults, validation, migration, atomic saving |
| `src/field.rs` | The 32 fields: keys, labels, icons, groups, gauges |
| `src/info/linux/` | Detection and live readings from `/proc`, `/sys` and a few local commands |
| `src/info/windows/` | Detection through Win32 calls and read-only registry values |
| `src/render/scene.rs` | The three scenes and their fallbacks |
| `src/render/blocks.rs` | Information rows, bars, gauges, titles and color blocks |
| `src/render/mod.rs` | Styled canvas, text width and output conversion |
| `src/logo.rs` | Logo discovery, cleaning and coloring |
| `src/theme.rs` | Built-in palettes and gradients |
| `src/tui/` | Setup editor: state and keys (`app.rs`), drawing (`view.rs`), terminal lifecycle (`mod.rs`), Windows clipboard (`clipboard.rs`) |
| `src/live.rs` | Monitor workspace: PTY shell and VT100 rendering |
| `src/output.rs` | JSON output, schema version 2 |
| `src/benchmark.rs` | Timing of collection and rendering |
| `src/update.rs` | Updater: source checkouts, or the latest release binary |
| `build.rs` | Embeds the `logos/` directory in the binary |

CI runs clippy, the tests and the release build on Ubuntu and Windows. The formatting check runs on Ubuntu.

Contributions and bug reports are welcome. Please include the terminal emulator, shell, terminal dimensions, selected scene and relevant configuration when reporting a layout problem. The terminal is part of the rendering environment.

## Roadmap

- [x] Three responsive scenes: classic, side and dashboard
- [x] Setup editor with sections, popups and an exact preview
- [x] 456 embedded logos with 78 compact variants
- [x] Configurable fields, panels, labels, icons and palettes
- [x] Monitor workspace with a real PTY-backed shell
- [x] Versioned JSON output and a built-in benchmark
- [x] Atomic configuration writes and migration from versions 1 and 2
- [x] GNU, musl and Windows (MSVC) release artifacts with SHA-256 checksums
- [x] CI with formatting, lints, tests and release builds on Linux and Windows
- [ ] Generate a complete configuration from CLI flags
- [ ] AUR package and Gentoo ebuild
- [ ] ARM64 and signed release artifacts
- [ ] Fresh screenshots for v3

## License

HaloFetch is licensed under [GPL-3.0-or-later](LICENSE).

<div align="center">

**Make the machine yours. Let the terminal show it.**

</div>
