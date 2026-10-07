# Handoff

State of the v3 rewrite and the Windows port, written so work can continue from a fresh session.

## Where things are

- Branch: `ccr-c9f6f9fa-40obji`, open as draft PR #2 against `main` (mafuzyk/atlasfetch).
- Version: 3.0.0 (`Cargo.toml`, `flake.nix`).
- CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests and a release build on `ubuntu-latest` and `windows-latest`. Both were green on `4e148d0`.
- `release.yml` builds Linux binaries and `x86_64-pc-windows-msvc` as a `.zip` with a `.sha256`.
- User-facing docs: `README.md` (English). Technical guide: `DOCS.md` (Brazilian Portuguese by project convention; keep it in pt-BR).

## Decisions already made

- Three scenes only: `classic`, `side`, `dashboard`. The old `cockpit` scene was merged into `dashboard` and the companion/STATUS block was removed. Old names still parse as aliases (`classicfetch`/`fastfetch` -> `side`, `cockpit` -> `dashboard`).
- The TUI config editor uses arrow keys only. No vim keys.
- Config is version 3. v1 and v2 files are migrated automatically on load; the original is kept as `.bak`, an unreadable file is moved aside as `.invalid`.
- Windows is the same binary with full parity, including the monitor through ConPTY.
- JSON output (`--format json`) is `schema_version` 2 with `system` and `gauges`.

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
| `src/config.rs` | Config v3, validation, normalization, migration, paths (`~/.config/atlasfetch`, `%APPDATA%\atlasfetch`). |
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

Fixed after that test (commits `9f6a698`, `9b56bbb`, not yet confirmed on the machine):

1. `monitor`: the shell pane stayed blank and accepted no input. ConPTY sends a cursor position request (`ESC[6n`) at startup and draws nothing until it is answered. The PTY reader in `src/live.rs` now answers it from the vt100 cursor. Confirm a prompt appears and typing works in PowerShell, pwsh and cmd.
2. GPU listed `Virtual Display Driver by MTT` next to the real GPU. Virtual adapters are now skipped (`is_virtual_adapter` in `src/info/windows/hardware.rs`).

Still open:

3. **Shell field missing.** `Terminal` showed `Windows Terminal` (from `WT_SESSION`), but `Shell` was absent, so the process-tree walk in `src/info/windows/process.rs` (`ProcTable::chain` from the parent of the current pid) returned no known shell. Get `atlasfetch --format json` and the parent chain from the machine (`Get-CimInstance Win32_Process` for the atlasfetch pid and its parents) before changing code. Possible fallback: when the chain has no known shell, use `PSModulePath`/`PROMPT` style environment hints (`pwsh` vs Windows PowerShell vs cmd).
4. **Icons render as replacement glyphs** without a Nerd Font (the default Windows Terminal font has none of them). Consider a config option to turn icons off or to use plain Unicode symbols, and detect nothing automatically.
5. **Long values truncated** in the classic scene at about 120 columns (`Desktop Window Manag…`, CPU and GPU names). Ideas: shorten the WM value to `DWM` on Windows (`DESKTOP_WINDOW_MANAGER` in `src/info/windows/mod.rs`), shorten GPU names (strip vendor suffixes), or let the composition move a long row to the side with more room.
6. **Cascade looks like misalignment.** The classic scene steps rows inward (`layout.cascade`, default 2, in `src/render/scene.rs`), so pills are not in a straight column. On Windows it read as a bug. Consider default 0 or a clearer shape.
7. **Dashboard is static.** `atlasfetch --scene dashboard` prints once and exits by design. Live updates are `atlasfetch monitor` (it accepts `--scene dashboard`). The user expected the dashboard to update; at least make this clear in the README, possibly add a hint line or a `--watch` flag on `fetch` that redraws in place without the shell.
8. VRAM is not reported on Windows (registry `HardwareInformation.qwMemorySize` under the display class key is a candidate).

## Conventions

- Conventional Commits, one commit per logical change, English messages. No AI attribution or co-author lines in commits or PRs.
- Minimal blast radius: do not reformat or touch unrelated code.
- No decorative comments or emojis in code.
- Code comments and README in English; `DOCS.md` in pt-BR.
- Talk to the user in pt-BR.
