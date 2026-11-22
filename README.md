<div align="center">

<h1>halofetch</h1>

<p><strong>A composed system fetch for the terminal.</strong></p>

<p>
  Three responsive scenes, 466 embedded ASCII logos, 32 fields, an image logo path,
  a live monitor, and a real shell workspace.<br>
  Written in Rust, shipped as a single binary.
</p>

<p>
  <a href="https://github.com/mafuzyk/halofetch/releases"><img src="https://img.shields.io/github/v/release/mafuzyk/halofetch?style=flat-square&color=8B5CF6&labelColor=1a1025" alt="Release" /></a>
  <a href="https://github.com/mafuzyk/halofetch/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/mafuzyk/halofetch/ci.yml?style=flat-square&color=8CCFA8&labelColor=1a1025&label=CI" alt="CI" /></a>
  <a href="https://github.com/mafuzyk/halofetch/actions/workflows/release.yml"><img src="https://img.shields.io/github/actions/workflow/status/mafuzyk/halofetch/release.yml?style=flat-square&color=8CCFA8&labelColor=1a1025&label=release" alt="Release workflow" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--or--later-8B5CF6?style=flat-square&labelColor=1a1025" alt="License" /></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/Rust-2021-E86B37?style=flat-square&logo=rust&logoColor=white&labelColor=1a1025" alt="Rust" /></a>
</p>

<img src="assets/fetch.png" width="880" alt="halofetch classic scene with a colored Linux logo and powerline panels" />

<sub><i>The classic scene: a colored logo, two information panels, and color blocks on one canvas.</i></sub>

<p>
  <a href="#installation">Installation</a> ·
  <a href="#benchmarks">Benchmarks</a> ·
  <a href="#modes">Modes</a> ·
  <a href="#make-it-yours">Customize</a> ·
  <a href="#fastfetch-migration">Migration</a> ·
  <a href="#documentation">Docs</a>
</p>

</div>

---

## Why halofetch?

- **Compose, do not dump.** `classic`, `side` and `dashboard` place a colored logo, two information panels and gauges on one deliberate canvas. The editor preview uses the same renderer as the output.
- **Only collect what you show.** The static fetch, the live monitor and JSON inspect the enabled fields. A hidden field does not pay its detection cost.
- **Real values, honest gaps.** CPU, GPU, memory, disk, sensors, battery, packages, desktop and network data come from `/proc`, `/sys`, `/etc` and local tools. A value that cannot be read is hidden.
- **Two modes, one shell.** Print one fetch and leave, or keep the scene above a PTY-backed shell that behaves like a normal terminal.
- **Safe configuration.** Version 3 schema, validation, normalization, migration from versions 1 and 2, numbered backups, private temporary files and atomic replacement. A symbolic link at the config path is refused.
- **Scriptable output.** Versioned JSON, separate profiles with `--config`, a benchmark subcommand and deterministic plain output for pipes.

<h2 align="center" id="benchmarks">Benchmarks</h2>

Median wall-clock time on the reference machine (`Linux 7.2.15-zen1 x86_64`, NixOS, `fastfetch 2.70.0`), same 13 modules, 30 measured runs after 5 warmups, no halofetch cache, stdout redirected to `/dev/null`:

| Benchmark | halofetch | fastfetch | Comparison |
| --- | ---: | ---: | --- |
| ANSI, median | **58.0 ms** | 147.0 ms | **39.5% of fastfetch** |
| ANSI, p95 | **60 ms** | 151 ms | 2.5× lower tail |
| `sharky -c exec` (shell startup included) | **59 ms** | 145 ms | 41% of fastfetch |

The same harness gives **58.0 ms median** and **64 ms p95** for `--format json`; JSON was not measured against fastfetch.

<details>
<summary>How the benchmark is measured, and why the package count differs</summary>

- Setup: the same 13 fields in both tools, no image and no logo image path, no halofetch cache, 5 warmups and 30 measured runs per command. The raw harness is a shell script kept outside the repository. JSON exists to show that serialization does not cost more than rendering.
- halofetch reports **265 real Nix packages** across the system and user profiles. It reads manifests and direct profile references, collapses split outputs and deduplicates store paths. It does not call a closure a package list.
- fastfetch reports the system and user profile closures (`nix-system` and `nix-user`), which is a different, larger quantity and a different workload. Correctness wins over the comparison: halofetch still runs in 39.5% of fastfetch's time while counting packages correctly.
- The one-shot fetch is field-oriented. A benchmark with more enabled fields does more work, as it should.

</details>

<h2 align="center" id="screenshots">Screenshots</h2>

| Classic | Side |
| --- | --- |
| ![classic scene](assets/fetch.png) | ![side scene](assets/narrow.png) |

| Panels | Summary |
| --- | --- |
| ![dashboard panels](assets/panels.png) | ![summary scene](assets/summary.png) |

<h2 align="center" id="installation">Installation</h2>

### Nix and NixOS

```bash
nix run github:mafuzyk/halofetch
nix profile install github:mafuzyk/halofetch
```

The flake exposes the package and can be added to a NixOS or Home Manager configuration. Nix installs are managed by Nix, so `halofetch update` refuses to replace the binary.

### Prebuilt binary

Each release publishes a static musl build (`x86_64-unknown-linux-musl`), a glibc build (`x86_64-unknown-linux-gnu`) and a Windows archive, each with a SHA-256 file:

```bash
version=v3.2.0
archive=halofetch-$version-x86_64-unknown-linux-musl.tar.gz
curl -fLO "https://github.com/mafuzyk/halofetch/releases/download/$version/$archive"
curl -fLO "https://github.com/mafuzyk/halofetch/releases/download/$version/$archive.sha256"
sha256sum -c "$archive.sha256"
tar -xzf "$archive"
install -Dm755 halofetch ~/.local/bin/halofetch
```

The archive also contains `LICENSE`, `README.md` and `THIRD_PARTY_NOTICES.md`. Make sure `~/.local/bin` is in `PATH`. For Windows, see [Windows](#windows).

### Build from source

```bash
git clone https://github.com/mafuzyk/halofetch.git
cd halofetch
cargo build --release --locked
install -Dm755 target/release/halofetch ~/.local/bin/halofetch
```

Rust and Cargo are needed only to build. The release profile is tuned in `Cargo.toml`.

### Start with your shell

Let halofetch write the guarded startup hook instead of copying shell snippets:

```bash
halofetch init fish     # fish
halofetch init bash     # bash
halofetch init zsh      # zsh
halofetch init sharky   # sharky
```

The hook runs only in interactive shells, checks that `halofetch` exists, and never starts the monitor inside its own embedded shell. Remove it with:

```bash
halofetch init fish --remove
```

<h2 align="center" id="modes">Modes</h2>

### Fetch

```bash
halofetch fetch
halofetch fetch --scene side
halofetch fetch --watch
```

`fetch` renders once and exits. `--watch` redraws the scene in place until `q`, `Esc` or `Ctrl+C`; `-i` sets the interval from 100 to 60000 ms.

| Scene | Also accepted as | Shape |
| --- | --- | --- |
| `classic` | | Logo centered between two information panels |
| `side` | `classicfetch`, `fastfetch` | Logo on the left, information on the right |
| `dashboard` | `cockpit` | Framed logo and System box, then Resources with gauges |

The scenes reflow instead of clipping. `classic` stacks the logo, title and rows when a panel would need fewer than 10 value columns. `side` puts the logo above the information when the logo and 30 information columns do not fit. `dashboard` stacks its boxes and uses two inner columns when the terminal is wide enough.

### Monitor

```bash
halofetch monitor
halofetch monitor -i 500
```

The upper region shows the scene with live values. The lower region is `$SHELL -i` in a pseudo-terminal, drawn through a VT100 parser, with completion, history, SSH, editors and control keys working normally. `Ctrl+Q` closes the workspace. Values refresh in place: uptime, load, processes, memory, swap, disk, battery, temperature, backlight, CPU usage and GPU usage. Packages, fonts and desktop detection run once.

In an interactive terminal without a configuration file, plain `halofetch` opens the editor. Saving creates `~/.config/halofetch/config.json`. `halofetch fetch` always prints one static fetch, and `halofetch monitor` always opens the workspace.

<h2 align="center" id="make-it-yours">Make it yours</h2>

### Fields and panels

HaloFetch has 32 fields in seven groups: System, Software, Desktop, Hardware, Resources, Network and Power. Each panel entry can be shown or hidden, renamed, given another icon, drawn as a bar when the field has a gauge, and moved within or between panels. Labels are limited to 24 characters and icons to 4.

CPU usage and GPU usage are live values and appear only in monitor mode. Narrow CPU values drop the clock before the core count. Narrow GPU values drop `Corporation`, `Inc.` and `Series` before truncation.

### Themes and logos

Choose one of the 27 built-in palettes, enter `#RRGGBB` colors separated by spaces, change the gradient, or save the current palette with `halofetch preset apply NAME`. The default is `amethyst`.

The logo can be detected from the distribution, chosen from 466 embedded logos, read from a file, pasted in the editor, or drawn as a PNG. `logo.compact` forces the small variant even when the full one fits. A file named after a logo in the configuration's `logos/` directory overrides the embedded art.

The embedded ASCII art is derived from fastfetch's MIT-licensed collection. See [`logos/README.md`](logos/README.md) and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) for exact provenance, the normalization applied and the license notice.

### Image logos

```jsonc
{
  "logo": {
    "source": { "kind": "auto" },
    "compact": false,
    "image": {
      "path": "~/Pictures/logo.png",
      "width": 32,
      "height": 16,
      "protocol": "auto"
    }
  }
}
```

```bash
halofetch --image ~/Pictures/logo.png fetch
```

Images are drawn with the Kitty graphics protocol on terminals that support it, including Koral, kitty, WezTerm and Ghostty. The reserved grid is laid out like an ASCII logo, so `classic`, `side` and `dashboard` position it normally. If the image is missing, invalid or cannot be drawn, the original character logo is rendered instead. `--image` changes the path only and keeps the configured protocol and size. Only the static fetch draws images; `fetch --watch` and `monitor` keep the character logo.

PNG files are validated before they reach the terminal: signature, chunk boundaries, CRC, IHDR dimensions and color type, IDAT presence and IEND. A file is limited to 16 MiB and 16384 pixels per side.

<h3 id="fastfetch-migration">Fastfetch migration</h3>

```bash
halofetch config import-fastfetch
halofetch config import-fastfetch ~/.config/fastfetch/config.jsonc --dry-run
```

The importer reads JSONC with comments and trailing commas, maps the logo, padding, colors, title and modules, splits modules into the two panels, and reports what it skipped instead of dropping it silently. Inline `logo.type: "data"` art is saved as `logos/fastfetch-logo.txt`; an existing file and configuration are backed up with numbered `.bak` names. `logo.type: "small"` sets `logo.compact`. Only Kitty images are imported because halofetch renders Kitty graphics; sixel, iTerm, chafa and raw logos produce a warning.

### Configuration without fear

The default path is `~/.config/halofetch/config.json`. `halofetch config path` prints the active path. The file is validated before saving and written through an exclusive temporary file with `fsync` and an atomic rename. Existing permissions are preserved, a new file is private (`0600`), and a symbolic link at the destination is refused. Values outside their range are clamped on load, and an unusable file is moved aside as `.invalid` instead of deleted.

Use another profile without touching the daily setup:

```bash
halofetch -c ./screenshots.json config
halofetch -c ./screenshots.json fetch
```

<details>
<summary>Editor sections and keys</summary>

1. **Appearance**: theme, palette, gradient, info style, title colors and palette saving.
2. **Logo**: source, compact variants, file path, image path, image size and image protocol.
3. **Fields**: both panels, visibility, order, labels, icons and bars.
4. **Layout**: scene, gap, padding, cascade, maximum value width, empty-field hiding, color blocks and title.
5. **Startup**: fetch or monitor mode and the refresh interval.

| Key | Action |
| --- | --- |
| `↑` `↓` | Move between sections or rows |
| `→` `Enter` `Tab` | Open a section |
| `Esc` `Tab` | Return to the section menu |
| `←` `→` | Change a choice or number |
| `Enter` | Edit text, open a list or activate a row |
| `Space` | Toggle a switch, or show/hide a field |
| `Shift` + `↑` `↓` | Move a field within its panel or across the edge |
| `Shift` + `←` `→` | Move a field to the left or right panel |
| `a` or `Insert` | Add a field after the selection |
| `Delete` or `Backspace` | Remove the selected field |
| `Ctrl+Z` | Restore the last removed field |
| `F1` or `?` | Key help |
| `F2` | Full-screen preview |
| `Ctrl+S` | Validate, save and exit |
| `q` | Quit, with a Save/Discard/Cancel prompt when needed |
| `Ctrl+C` | Quit without saving |

The editor needs at least 60 × 18. From 110 columns the menu, settings and preview share a row; narrower terminals put the preview below the settings.

</details>

<details>
<summary>Version 3 configuration shape</summary>

```json
{
  "version": 3,
  "scene": "classic",
  "startup": { "mode": "fetch", "interval_ms": 1000 },
  "logo": {
    "source": { "kind": "auto" },
    "gradient": "horizontal",
    "auto_small": true,
    "compact": false,
    "image": {
      "path": "",
      "width": 32,
      "height": 16,
      "protocol": "auto"
    }
  },
  "colors": {
    "palette": ["#C084FC", "#A78BFA", "#818CF8", "#6366F1", "#4F46E5"],
    "title": "#FF9A98",
    "separator": "#9D85FF",
    "value": "#F5DCE3"
  },
  "title": { "enabled": true, "format": "{user}@{host}", "separator": "─" },
  "layout": {
    "style": "powerline",
    "gap": 3,
    "padding": 2,
    "cascade": 0,
    "max_value_width": 0,
    "hide_empty": true,
    "icons": "nerd",
    "color_blocks": true
  },
  "fields": {
    "left": [
      { "field": "os", "label": "OS", "icon": "", "enabled": true, "bar": false },
      { "field": "kernel", "label": "Kernel", "icon": "", "enabled": true, "bar": false }
    ],
    "right": []
  },
  "custom_palettes": {}
}
```

`logo.image.protocol` accepts `auto`, `kitty` and `none`. `auto` detects the terminal through `TERM_PROGRAM`, `TERM`, `KITTY_WINDOW_ID` and `WEZTERM_PANE`; `kitty` forces the protocol when stdout is a terminal; `none` disables image drawing. Only Kitty graphics is implemented.

</details>

<h2 align="center" id="command-map">Command map</h2>

| Command | What it does |
| --- | --- |
| `halofetch` | Editor on first run, then the configured startup mode, otherwise one static fetch |
| `halofetch fetch` | Print one static fetch |
| `halofetch fetch --watch` | Redraw until `q`, `Esc` or `Ctrl+C`; `-i` sets the interval |
| `halofetch fetch --scene side` | Print one fetch with a chosen scene |
| `halofetch monitor` | Scene plus embedded shell |
| `halofetch --format json` | Detected fields as versioned JSON |
| `halofetch --image PATH fetch` | Use a PNG in place of the logo for one static fetch |
| `halofetch config` | Open the editor |
| `halofetch config path` | Print the active configuration path |
| `halofetch config reset` | Back up the configuration and open the editor with defaults |
| `halofetch config import-fastfetch [FILE] [--dry-run]` | Convert a fastfetch JSONC configuration |
| `halofetch init SHELL [--remove]` | Install or remove an interactive startup hook |
| `halofetch preset list` | List built-in and custom palettes |
| `halofetch preset apply dracula` | Apply a palette and save |
| `halofetch logos list` | List embedded and user logos |
| `halofetch logos show [KEY]` | Print one colored logo |
| `halofetch benchmark` | Measure collection and rendering (`-n` runs) |
| `halofetch update [--release]` | Update from a checkout or the latest release |

`--scene`, `--format`, `--image` and `-c/--config PATH` are global and work before or after the command. `--help` is the authoritative reference.

Colors go to a terminal only. A pipe or file gets plain text, as do `TERM=dumb` and a non-empty `NO_COLOR`. A non-empty `FORCE_COLOR` keeps colors in a pipe:

```bash
FORCE_COLOR=1 halofetch | less -R
```

<h2 align="center" id="machine-readable-output">Machine-readable output</h2>

```bash
halofetch --format json | jq '.system.cpu'
```

The document has `schema_version` 2, `system` and `gauges`. `system` contains the enabled fields in declaration order, plus `user` and `host` when the title is enabled; disabled or unreadable fields are absent. `gauges` holds numeric values: `memory`, `swap`, `disk` and `vram` as `used`/`total` bytes with `percent`, plus `cpu_percent`, `gpu_percent`, `cpu_temp_celsius`, `battery_percent` and `brightness_percent`. Scripts must check that a key exists.

<h2 align="center" id="windows">Windows</h2>

Windows 10 and 11 are supported through `%APPDATA%\halofetch\config.json`, PowerShell or cmd as the embedded shell, and the same editor, scenes and JSON output. Extract `halofetch.exe` from the release zip and put it on `PATH`. The archive also contains `LICENSE`, `README.md` and `THIRD_PARTY_NOTICES.md`.

Windows Terminal does not render Kitty images, so `logo.image` falls back to the character logo. GPU names, VRAM, memory, disk, battery, brightness, resolution, network and process values are read through Win32 where available. File replacement follows the platform rules; the Unix symlink refusal applies only where symlinks are supported.

<h2 align="center" id="documentation">Documentation</h2>

- [`DOCS.md`](DOCS.md): architecture, field catalog, configuration, migration, rendering and testing
- [`logos/README.md`](logos/README.md): logo provenance and normalization
- [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md): fastfetch MIT notice
- [`LICENSE`](LICENSE): GPL-3.0-or-later

<h2 align="center" id="development">Development</h2>

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo build --release --locked
```

The static fetch, editor preview and monitor share `src/render`. Detection lives in `src/info`, configuration and validation in `src/config`, image encoding in `src/image`, fastfetch migration in `src/fastfetch`, and startup hooks in `src/init`. CI runs formatting, clippy, tests and release builds on Linux and Windows.

<h2 align="center" id="license">License</h2>

HaloFetch is licensed under [GPL-3.0-or-later](LICENSE). The embedded ASCII logos are derived from fastfetch's MIT-licensed collection; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the required notice.

<div align="center">

<sub>Compose the machine. Do not dump it.</sub>

</div>
