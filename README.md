<div align="center">

# AtlasFetch

### Your Linux, composed — not dumped.

A fast system fetch, a live hardware monitor, and a real interactive shell<br>
sharing one carefully arranged terminal canvas.

[![CI](https://github.com/mafuzyk/atlasfetch/actions/workflows/ci.yml/badge.svg)](https://github.com/mafuzyk/atlasfetch/actions/workflows/ci.yml)
[![Release](https://github.com/mafuzyk/atlasfetch/actions/workflows/release.yml/badge.svg)](https://github.com/mafuzyk/atlasfetch/actions/workflows/release.yml)
[![Rust](https://img.shields.io/badge/Rust-2021-e86b37?logo=rust&logoColor=white)](Cargo.toml)
[![License](https://img.shields.io/badge/license-GPL--3.0--or--later-8b5cf6)](LICENSE)

[Quick start](#quick-start) · [Choose a mode](#choose-a-mode) · [Customize](#make-it-yours) · [Install](#installation) · [Contribute](#development)

</div>

AtlasFetch treats terminal output like a layout instead of a list. Its scenes place a colored ASCII logo next to the information you care about; its live monitor keeps that composition above a PTY-backed shell you can use normally.

It began as a companion to [atlasWM](https://github.com/mafuzyk/atlaswm), but it has no desktop or window-manager allegiance. If it is Linux and it has a terminal, AtlasFetch should run there.

## Why AtlasFetch?

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
atlasfetch fetch
```

Renders one fetch and exits. Use it for terminal greetings, screenshots, dotfiles, or showing everyone your kernel version.

| Scene | Also accepted as | Shape |
|---|---|---|
| `classic` | | Logo centered between two information panels |
| `side` | `classicfetch`, `fastfetch` | Logo on the left, information on the right |
| `dashboard` | `cockpit` | Framed boxes: logo and System, then Resources with gauges |

Try a scene without changing your configuration:

```bash
atlasfetch fetch --scene side
atlasfetch --scene dashboard
```

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

The sample is `atlasfetch fetch --scene side` with `NO_COLOR=1`. Nerd Font icons and powerline separators may show as blanks in a font without them.

### 2. Live monitor

```bash
atlasfetch monitor
```

The upper region shows the scene and refreshes its values at the configured interval. The lower region is a real shell in a pseudo-terminal, drawn through a VT100 parser.

```text
╭─ AtlasFetch · live 1000ms ─────────────────────────────╮
│ scene with live values, at most 60% of the screen      │
╰────────────────────────────────────────────────────────╯
╭─ Shell · Ctrl+Q closes workspace ──────────────────────╮
│ $                                                      │
╰────────────────────────────────────────────────────────╯
```

- Keys go to the shell, including completion, history, `sudo`, SSH, editors and control keys. The only key the workspace keeps is `Ctrl+Q`, which closes it.
- The shell is `$SHELL -i`. Fish starts without its greeting and without its terminal query.
- Values refreshed in place: uptime, load, process count, memory, swap, disk, battery, CPU temperature, backlight, CPU usage and GPU usage. Package, font and desktop detection is not repeated.
- The screen is redrawn after each refresh and whenever the shell prints something, not in a tight loop.
- `atlasfetch monitor -i 500` refreshes every 500 ms. The interval can be 100 to 60000 ms.
- Monitor needs an interactive terminal. In a pipe, use `fetch`.

Set **Startup → Mode** to *monitor* in the editor and a plain `atlasfetch` opens this workspace in an interactive terminal. `atlasfetch fetch` always prints one static fetch.

## Quick start

```bash
atlasfetch
```

In an interactive terminal without a configuration file, this opens the setup editor. Saving creates `~/.config/atlasfetch/config.json`. Quitting without saving keeps the defaults and writes nothing. Return to the editor whenever you like:

```bash
atlasfetch config
```

The editor has five sections:

1. **Appearance**: theme and palette, gradient, info style, title, separator and value colors, and saving the palette under a name.
2. **Logo**: source (automatic, a built-in logo, a file, or none), the compact variant for narrow terminals, and the file path. Type to filter the built-in logos.
3. **Fields**: the left and right panels. Show or hide entries, reorder them, move them between panels, edit labels, icons and bars, add or remove fields.
4. **Layout**: scene, gap, padding, cascade, maximum value width, hiding empty fields, color blocks, the title and its format and separator.
5. **Startup**: the startup mode (fetch or monitor) and the refresh interval.

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

AtlasFetch has 32 fields in seven groups: System, Software, Desktop, Hardware, Resources, Network and Power. Each entry in a panel can be shown or hidden, renamed, given another icon, drawn as a bar when its field has a gauge, and moved within or between the left and right panels. Labels are limited to 24 characters and icons to 4.

Fields such as CPU usage and GPU usage are live values. They appear only in monitor mode.

### Themes and ASCII

Choose one of the 27 built-in palettes, enter your own colors as `#RRGGBB` values separated by spaces, change the gradient direction, or save the current palette under a name. Saved palettes appear next to the built-in ones and can be applied from the command line with `atlasfetch preset apply NAME`.

The logo can be detected from your distribution, chosen from the 456 embedded logos, read from a file, or pasted. A pasted logo is saved as `custom-logo.txt` next to the configuration when you save. A file named after a logo in `logos/` inside the configuration directory replaces the embedded logo with the same name.

Nerd Fonts are recommended for the default icons. The layout works without them.

### Configuration without fear

The default configuration lives at:

```text
~/.config/atlasfetch/config.json
```

Run `atlasfetch config path` to print the path in use. The configuration is validated before it is saved and written atomically. Values that are out of range in a file are limited to their range when loaded.

Use another profile without touching your daily setup:

```bash
atlasfetch -c ./screenshots.json config
atlasfetch -c ./screenshots.json fetch
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
    "cascade": 2,
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
| `atlasfetch` | Interactive: opens the editor on the first run, then follows the startup mode. Otherwise prints the static fetch |
| `atlasfetch fetch` | Print one static fetch and exit |
| `atlasfetch fetch --scene side` | Print one fetch with the given scene |
| `atlasfetch --format json` | Print the detected system information as JSON |
| `atlasfetch monitor` | Open the live workspace |
| `atlasfetch monitor -i 500` | Open the live workspace, refreshing every 500 ms |
| `atlasfetch config` | Open the setup editor (`config edit` is the same) |
| `atlasfetch config path` | Print the active configuration path |
| `atlasfetch config reset` | After confirmation, move the configuration aside as `.bak` and open the editor with defaults |
| `atlasfetch preset list` | List built-in and custom palettes |
| `atlasfetch preset apply dracula` | Use a palette for the logo colors and save |
| `atlasfetch logos list` | List logo keys: embedded logos and files in the user logo directory, sorted |
| `atlasfetch logos show` | Print the logo this machine's configuration selects, colored with the palette |
| `atlasfetch logos show arch` | Print one logo colored with the palette |
| `atlasfetch benchmark` | Measure information collection and the full render (`-n` sets the runs, 5 by default) |
| `atlasfetch update` | Pull, rebuild and install from the source checkout |

`--scene`, `--format` and `-c, --config PATH` work before or after the command. `atlasfetch --help` is the authoritative reference.

## Installation

### Build from source

```bash
git clone https://github.com/mafuzyk/atlasfetch.git
cd atlasfetch
cargo build --release --locked
install -Dm755 target/release/atlasfetch ~/.local/bin/atlasfetch
```

Make sure `~/.local/bin` is in `PATH`. Rust and Cargo are needed only to build from source.

### Nix

```bash
nix run github:mafuzyk/atlasfetch
nix profile install github:mafuzyk/atlasfetch
```

### Update a source installation

```bash
atlasfetch update
```

The updater refuses a checkout with local changes, runs `git pull --rebase --autostash`, builds with `cargo build --release --locked`, and installs to `~/.local/bin/atlasfetch`. It finds the checkout through `ATLASFETCH_SRC`, the current directory, the directories around the executable, or a few common paths under the home directory. Set `ATLASFETCH_SRC=/path/to/atlasfetch` to choose one explicitly.

### Start with your shell

Static mode is the sensible choice for a greeting:

```fish
# ~/.config/fish/config.fish
if status is-interactive
    atlasfetch fetch
end
```

```bash
# ~/.bashrc
if [[ $- == *i* ]]; then
    atlasfetch fetch
fi
```

```zsh
# ~/.zshrc
if [[ -o interactive ]]; then
    atlasfetch fetch
fi
```

Starting the live workspace from the shell it launches would nest one shell inside another, so use `fetch` in shell startup files.

## Machine-readable output

```bash
atlasfetch --format json | jq '.system.cpu'
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

AtlasFetch collects one model of the system, lays it out in a scene, and draws it through one styled canvas. The static output, the editor preview and the live monitor all use that path.

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
| `src/info/` | Detection and live readings from `/proc`, `/sys` and a few local commands |
| `src/render/scene.rs` | The three scenes and their fallbacks |
| `src/render/blocks.rs` | Information rows, bars, gauges, titles and color blocks |
| `src/render/mod.rs` | Styled canvas, text width and output conversion |
| `src/logo.rs` | Logo discovery, cleaning and coloring |
| `src/theme.rs` | Built-in palettes and gradients |
| `src/tui/` | Setup editor: state and keys (`app.rs`), drawing (`view.rs`), terminal lifecycle (`mod.rs`) |
| `src/live.rs` | Monitor workspace: PTY shell and VT100 rendering |
| `src/output.rs` | JSON output, schema version 2 |
| `src/benchmark.rs` | Timing of collection and rendering |
| `src/update.rs` | Updater for source checkouts |
| `build.rs` | Embeds the `logos/` directory in the binary |

Contributions and bug reports are welcome. Please include the terminal emulator, shell, terminal dimensions, selected scene and relevant configuration when reporting a layout problem. The terminal is part of the rendering environment.

## Roadmap

- [x] Three responsive scenes: classic, side and dashboard
- [x] Setup editor with sections, popups and an exact preview
- [x] 456 embedded logos with 78 compact variants
- [x] Configurable fields, panels, labels, icons and palettes
- [x] Monitor workspace with a real PTY-backed shell
- [x] Versioned JSON output and a built-in benchmark
- [x] Atomic configuration writes and migration from versions 1 and 2
- [x] GNU and musl release artifacts with SHA-256 checksums
- [x] CI with formatting, lints, tests and release builds
- [ ] Generate a complete configuration from CLI flags
- [ ] AUR package and Gentoo ebuild
- [ ] ARM64 and signed release artifacts
- [ ] Fresh screenshots for v3

## License

AtlasFetch is licensed under [GPL-3.0-or-later](LICENSE).

<div align="center">

**Make the machine yours. Let the terminal show it.**

</div>
