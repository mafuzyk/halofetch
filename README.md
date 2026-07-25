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

<img src="assets/fetch.png" alt="AtlasFetch centered scene" width="820">

</div>

AtlasFetch treats terminal output like a layout instead of a list. Its default scenes balance a colored ASCII logo with the information you actually care about; its optional live workspace keeps that composition above a PTY-backed shell you can genuinely use.

It began as a companion to [atlasWM](https://github.com/mafuzyk/atlaswm), but it has no desktop or window-manager allegiance. If it is Linux and it has a terminal, AtlasFetch should feel at home.

## Why AtlasFetch?

- **One command, two personalities.** Print once and exit, or stay as a live monitor with a shell underneath.
- **Designed, not hard-coded.** Four responsive scenes share one component system and Unicode-aware layout engine.
- **Personal without becoming homework.** The TUI edits themes, panels, fields, labels, icons, spacing, and ASCII with a real preview.
- **534 embedded logos.** Full and compact distro variants ship inside the binary; custom text art is welcome too.
- **Useful data, honest fallbacks.** CPU, GPU, memory, disks, sensors, packages, desktop state, network details, and more—without inventing values when a sensor is unavailable.
- **Safe configuration.** Versioned JSON, validation, migration from older formats, atomic writes, and preservation of invalid files.
- **Built for automation too.** Static output, isolated config profiles, versioned JSON, deterministic scene tests, and an integrated benchmark.
- **No mobile afterthought.** AtlasFetch intentionally targets Linux desktops and terminals, keeping its interface coherent.

## Choose a mode

### 1. Fetch and leave

```bash
atlasfetch fetch
```

One composition, zero ceremony. Ideal for terminal greetings, screenshots, dotfiles, and that completely legitimate need to show everyone your kernel version.

| Scene | Personality |
|---|---|
| `classic` | The AtlasFetch signature: centered logo, information on both sides |
| `dashboard` | Denser blocks for people who want more signal per row |
| `cockpit` | Logo, system data, and live-oriented components in one instrument panel |
| `classicfetch` | Familiar Fastfetch-style composition: logo left, information right |

Try a scene without changing your configuration:

```bash
atlasfetch --scene classicfetch
atlasfetch --scene cockpit
```

### 2. Stay in the cockpit

```bash
atlasfetch monitor
```

The upper canvas refreshes CPU, RAM, GPU, temperature, disk, load, processes, and uptime. The lower canvas is a real interactive shell connected through a PTY and rendered as VT100—not a fake command box.

```text
┌─ AtlasFetch · live 1000ms ───────────────────────────────┐
│                scene + live system metrics               │
├──────────────────────────────────────────────────────────┤
│ Shell · your prompt, commands, completion and programs   │
└──────────────────────────────────────────────────────────┘
```

- Type commands normally; completion, history, `sudo`, SSH, editors, and control keys are forwarded to the child shell.
- `Ctrl+Q` closes the entire workspace. `Ctrl+C`, `Ctrl+D`, `Esc`, and the usual navigation keys belong to the shell.
- Fish configuration is preserved, while redundant startup fetches and unsupported terminal probes are skipped inside the workspace.
- Metrics redraw only when data changes; AtlasFetch does not repaint at full speed while idle.
- In a pipe or non-interactive session, AtlasFetch automatically falls back to static output.

Enable **Monitor Mode** in Setup and plain `atlasfetch` will open this workspace by default. `atlasfetch fetch` always remains the escape hatch for one static render.

<div align="center">
<img src="assets/summary.png" alt="AtlasFetch summary view" width="48%">
<img src="assets/panels.png" alt="AtlasFetch configurable panels" width="48%">
</div>

## Quick start

```bash
atlasfetch
```

The first launch opens Setup and creates `~/.config/atlasfetch/config.json`. Return to the editor whenever you like:

```bash
atlasfetch --setup
# or
atlasfetch config
```

The editor is organized around five questions:

1. **Theme** — which colors feel like your desktop?
2. **Mode** — which scene, spacing, and startup behavior do you want?
3. **Panels** — which facts deserve space, and where?
4. **ASCII** — which embedded or custom logo represents the machine?
5. **Save** — does the preview look right?

It adapts between side-by-side and stacked layouts. Terminals below 52 × 16 receive a useful resize message instead of abstract ANSI wreckage.

<img src="assets/narrow.png" alt="AtlasFetch narrow responsive editor" width="620">

Essential controls:

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Move between editor sections |
| Arrow keys | Navigate the current section |
| `m` | Toggle persistent Monitor Mode from the Mode tab |
| `Ctrl+S` | Save and exit from anywhere |
| `?` | Open the complete keyboard guide |
| `q` / `Esc` | Exit, with a save/discard prompt when needed |

## Make it yours

### Fields and panels

AtlasFetch ships fields for OS, host, user, kernel, uptime, packages, shell, terminal, CPU, GPU, memory, disk, WM, DE, load, processes, local IP, resolution, font, VRAM, Flatpak, Snap, sensors, battery information, and more.

Every field can be enabled, hidden, renamed, given another icon, and reordered between the left and right panels. The preview and final output use the exact same renderer, so what you approve is what you get.

### Themes and ASCII

Choose a built-in palette, compose one from hex colors, change gradient direction, search all embedded logos, paste your own art, or run without a logo. Nerd Fonts are recommended for the default icons but are not required for the layout itself.

### Configuration without fear

The default configuration lives at:

```text
~/.config/atlasfetch/config.json
```

AtlasFetch validates before saving and replaces the file atomically. Malformed configurations are moved aside as `.invalid` instead of being destroyed. Use another profile without touching your daily setup:

```bash
atlasfetch --config ./screenshots.json --setup
atlasfetch --config ./screenshots.json fetch
```

Minimal shape of the v2 schema:

```json
{
  "version": 2,
  "scene": "classic",
  "live": {
    "enabled": false,
    "interval_ms": 1000
  },
  "logo": {
    "key": "arch",
    "colors": [
      { "r": 192, "g": 132, "b": 252 },
      { "r": 99, "g": 102, "b": 241 }
    ],
    "color_dir": "horizontal"
  },
  "display": {
    "left": [
      { "field": "os", "icon": "", "label": "OS", "enabled": true }
    ],
    "right": [
      { "field": "cpu", "icon": "", "label": "CPU", "enabled": true }
    ]
  }
}
```

The TUI is the recommended editor. See [DOCS.md](DOCS.md) for the complete schema, migration rules, architecture, and contributor notes.

## Command map

| Command | What it does |
|---|---|
| `atlasfetch` | Follow the saved startup preference |
| `atlasfetch fetch` | Render once and exit, ignoring Monitor Mode |
| `atlasfetch monitor` | Force the live workspace |
| `atlasfetch monitor -i 500` | Refresh volatile metrics every 500 ms |
| `atlasfetch monitor --scene cockpit` | Open live mode with a one-off scene |
| `atlasfetch config` | Open the TUI editor |
| `atlasfetch config path` | Print the active configuration path |
| `atlasfetch config reset` | Reset the active configuration and reopen Setup |
| `atlasfetch preset list` | List built-in color palettes |
| `atlasfetch preset apply dracula` | Apply a palette |
| `atlasfetch logos list` | List embedded logo keys |
| `atlasfetch --format json` | Emit versioned machine-readable system data |
| `atlasfetch benchmark` | Benchmark full information collection |
| `atlasfetch --just-ascii` | Render only the logo |
| `atlasfetch update` | Update a clean source checkout and reinstall |

`atlasfetch --help` is the authoritative reference.

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

The updater refuses a dirty checkout, pulls with Git, builds from the lockfile, and installs to `~/.local/bin/atlasfetch`. Point it at a custom checkout with `ATLASFETCH_SRC=/path/to/atlasfetch`.

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

Starting the live workspace recursively from the shell it launches would be spectacular but unhelpful, so use `fetch` in shell startup files.

## Machine-readable output

AtlasFetch is allowed to be pretty without being hostile to scripts:

```bash
atlasfetch --format json | jq '.system.cpu'
```

The JSON schema is versioned and omits unavailable optional values. Human renderers never need to be scraped.

## Development

AtlasFetch is organized as a component renderer rather than four unrelated print functions. System collection produces one model; scenes arrange components; ANSI output, Setup preview, and live mode consume the same visual rules.

```text
Linux interfaces + local commands
              │
              ▼
          SysInfo model
              │
      ┌───────┴────────┐
      ▼                ▼
 configurable       live metrics
   components        + PTY shell
      │                │
      └───────┬────────┘
              ▼
       responsive scenes
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
| `src/info.rs` | Linux information and sensor collection |
| `src/component/` | Components, responsive scenes, and live metrics |
| `src/tui/` | Editor state, input handling, and rendering |
| `src/live.rs` | Monitor workspace, PTY shell, VT100 rendering |
| `src/config.rs` | Schema, migration, validation, atomic persistence |
| `src/output.rs` | Versioned JSON output |
| `src/update.rs` | Safe source-checkout updater |

Contributions and bug reports are welcome. Please include the terminal emulator, shell, terminal dimensions, selected scene, and relevant config when reporting a layout problem—the terminal is part of the rendering environment.

## Roadmap

- [x] Four responsive visual scenes
- [x] Live TUI editor and exact preview
- [x] 534 embedded logo variants and custom ASCII
- [x] Configurable fields, panels, labels, icons, and palettes
- [x] Monitor workspace with a real PTY-backed shell
- [x] Versioned JSON and built-in collection benchmark
- [x] Atomic config persistence and migration
- [x] GNU/musl release artifacts with SHA-256 checksums
- [x] CI formatting, lint, tests, snapshots, and release builds
- [ ] Generate a complete configuration from CLI flags
- [ ] AUR package and Gentoo ebuild
- [ ] ARM64 and signed release artifacts

## License

AtlasFetch is licensed under [GPL-3.0-or-later](LICENSE).

<div align="center">

**Make the machine yours. Let the terminal show it.**

</div>
