# Handoff

State of the v3 rewrite and the Windows port, written so work can continue from a fresh session.

## Where things are

- The project was renamed from atlasfetch to halofetch. The local checkout is still `D:\atlasfetch`; the branch name is unchanged.
- Branch: `ccr-c9f6f9fa-40obji`, open as draft PR #2 against `main` (mafuzyk/halofetch).
- Version: 3.0.0 (`Cargo.toml`, `flake.nix`).
- The Rust toolchain (rustup, MSVC) is installed on the Windows machine, so the full gate runs there.
- CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests and a release build on `ubuntu-latest` and `windows-latest`. Both were green before the halofetch rename; the branch history was rewritten since, so older commit hashes no longer match.
- `release.yml` builds Linux binaries and `x86_64-pc-windows-msvc` as a `.zip` with a `.sha256`.
- User-facing docs: `README.md` (English). Technical guide: `DOCS.md` (Brazilian Portuguese by project convention; keep it in pt-BR).

## Decisions already made

- Three scenes only: `classic`, `side`, `dashboard`. The old `cockpit` scene was merged into `dashboard` and the companion/STATUS block was removed. Old names still parse as aliases (`classicfetch`/`fastfetch` -> `side`, `cockpit` -> `dashboard`).
- The TUI config editor uses arrow keys only. No vim keys.
- Config is version 3. v1 and v2 files are migrated automatically on load; the original is kept as `.bak`, an unreadable file is moved aside as `.invalid`.
- Windows is the same binary with full parity, including the monitor through ConPTY.
- JSON output (`--format json`) is `schema_version` 2 with `system` and `gauges`.
- The project and the binary are named `halofetch` (`halofetch.exe` on Windows). The configuration directory `atlasfetch` is moved to `halofetch` on first run, and `ATLASFETCH_SRC` is still read as a fallback for `HALOFETCH_SRC`.

## Architecture

| Path | Role |
|------|------|
| `src/field.rs` | `Field` enum (32 fields): keys, legacy aliases, labels, Nerd Font icons, groups. |
| `src/info/mod.rs` | Platform-neutral `SysInfo`, `Gauges`, `collect()`, `refresh_live()`, `CpuSampler`, shared formatting helpers. |
| `src/info/linux/` | Linux detection (process tree, DRM outputs, pci.ids, package managers, hwmon, battery). |
| `src/info/windows/` | Windows detection: registry wrapper, OS/kernel, CPU/GPU from registry, memory, disk, power, display mode, ToolHelp process tree, scoop/choco, adapters, `GetSystemTimes`. |
| `src/render/mod.rs` | Canvas: `Style`, `Span`, `Line`, ANSI / plain / ratatui output. |
| `src/render/blocks.rs` | Info rows (powerline and plain, mirrored for the left panel), bars, gauges, color blocks, title. |
| `src/render/scene.rs` | Scene composition. Classic centers the logo and fits both panels with a water-filling width split, falling back to a stacked layout. |
| `src/config.rs` | Config v3, validation, normalization, migration, paths (`~/.config/halofetch`, `%APPDATA%\halofetch`). |
| `src/logo.rs` | Embedded logos (built by `build.rs` from `logos/`), detection, file logos, coloring. |
| `src/theme.rs` | Palettes, gradients, contrast. |
| `src/tui/` | Config editor: `app.rs` state and events, `view.rs` drawing, `input.rs` text input, `clipboard.rs` (Windows only). |
| `src/live.rs` | `monitor`: scene refreshed in place above an embedded shell (portable-pty + vt100). |
| `src/update.rs` | Self-update from the source checkout. |
| `src/main.rs`, `src/cli.rs` | Subcommands `fetch`, `monitor`, `config`, `preset`, `logos`, `benchmark`, `update`. |

The TUI is tested by feeding synthetic events to `App` and checking state and the rendered buffer; follow that pattern for new editor behavior.

## Validation gate

Run all of these before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

From Linux, Windows code can be type-checked and cross-built (tests cannot run):

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt-get install gcc-mingw-w64-x86-64
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
cargo clippy --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings
cargo build --release --locked --target x86_64-pc-windows-gnu
```

On Windows itself the normal gate works as is.

## Feedback from the first test on real Windows

Tested on Windows 11 Pro 26H2, Windows Terminal, Windows PowerShell 5, Ryzen 5 5600GT.

Confirmed on the machine:

- The monitor prompt appears under ConPTY. Keyboard input is still to be confirmed by the user.
- Virtual display adapters are no longer listed.

Resolved:

3. **Shell field missing.** Not a detection bug: the user's config had `shell` disabled. Detection was verified for pwsh, Windows PowerShell 5 and cmd.
4. **Icons render as replacement glyphs.** Added `layout.icons` (default `true`); turn it off in the config or in the editor's Layout section.
5. **Long values truncated.** WM is shown as `DWM`. CPU and GPU values are shortened before they are cut (`src/render/blocks.rs`).
6. **Cascade looks like misalignment.** The default `layout.cascade` is now `0`.
7. **Dashboard is static.** `halofetch fetch --watch` redraws the scene in place. `halofetch --scene dashboard` still prints once.
8. **VRAM not reported.** VRAM shows the dedicated video memory total from the registry; there is no usage gauge on Windows.

New fix: the monitor's shell pane border was misdrawn because empty vt100 cells were written as empty symbols. Fixed in `src/live.rs`.

Still open:

- `fetch --watch` does not handle a scene taller than the terminal: the cursor cannot move above the top row, and old lines stay in the scrollback.
- `NO_COLOR` is honoured by the monitor (through crossterm) but not by the static fetch on a terminal.
- Existing config files keep their saved `cascade` value, so the new default only applies to new files.

## Conventions

- Conventional Commits, one commit per logical change, English messages. No AI attribution or co-author lines in commits or PRs.
- Minimal blast radius: do not reformat or touch unrelated code.
- No decorative comments or emojis in code.
- Code comments and README in English; `DOCS.md` in pt-BR.
- Talk to the user in pt-BR.
