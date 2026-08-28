# AtlasFetch v2 Reliability Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate AtlasFetch v2 into a reliable, non-systemd-first, responsive, testable terminal application by fixing known correctness defects, separating collection from rendering, simplifying scenes/layouts, hardening configuration and live terminal behavior, and aligning documentation, packaging, and public contracts with the actual implementation.

**Architecture:** Keep AtlasFetch as one Rust binary, but introduce clear boundaries between probing, typed data, volatile monitoring, layout planning, styled output, terminal capabilities, configuration/versioning, and live PTY lifecycle. The migration must be incremental: preserve working behavior and compatibility where reasonable, add regression tests before each refactor, and avoid a wholesale rewrite.

**Tech Stack:** Rust 2021, clap 4, serde/serde_json, ratatui 0.29, crossterm 0.28, color-eyre, libc, directories, unicode-width, regex, portable-pty 0.9, vt100 0.15, GitHub Actions, Nix flake.

**Spec:** `docs/superpowers/plans/2026-08-28-atlasfetch-v2-reliability-consolidation.md` — this file is intentionally both the consolidated audit/specification and the task-by-task implementation plan.

## Global Constraints

- Linux is the target platform, but systemd is never a mandatory dependency for generic collection, startup, rendering, or configuration.
- OpenRC is a service manager and is not assumed to be PID 1; init detection and service-manager detection are separate concepts.
- The refactor must be incremental. Do not replace AtlasFetch wholesale with a new implementation.
- Rendering must be pure: no sleeping, subprocess execution, `/proc`/`/sys` reads, network/interface probing, or mutable sampling inside render functions.
- Static and live modes must consume the same typed data contracts where the meaning is the same.
- Expensive or optional subprocess collectors must have bounded execution time and graceful absence behavior.
- No collector may require network access.
- Unknown or unavailable hardware information must remain unknown/omitted rather than being fabricated.
- The TUI preview must use the same composition rules as the output it claims to preview.
- Terminal width and height are both layout constraints.
- User/custom text rendered into the static ANSI stream must not be allowed to inject arbitrary terminal control sequences.
- Existing config data must be preserved whenever recovery is possible; a config created by a newer AtlasFetch must never be renamed or overwritten merely because the running binary is older.
- `NO_COLOR` and a user-selectable color policy must be respected in human-readable static output.
- Legacy CLI forms may remain as deprecated aliases during the v2.x consolidation, but they must normalize into one canonical action model.
- Tests are written before each bug fix/refactor that changes observable behavior.
- Do not add unrelated features until P0 and P1 items in this document are complete.

---

# 1. Review scope and evidence

This plan consolidates four review passes over the current `main` branch, snapshot commit `325380715e496ba7ba5f5a976593434d44e64935` at the time of review.

The review covered:

- Linux/system information probing and non-systemd compatibility;
- rendering architecture and component boundaries;
- static fetch and live monitor performance;
- TUI functional behavior and visual hierarchy;
- all current scenes/modes and their responsive behavior;
- configuration schema, validation, migration, themes, custom palettes, and logos;
- CLI behavior and machine-readable output;
- live PTY shell behavior and terminal emulation assumptions;
- updater behavior;
- CI, release packaging, Nix packaging, documentation, project metadata, and hardening.

The review is source/static analysis of the repository. A local clone/test run was attempted during the review environment but could not complete because that environment could not resolve `github.com`. Therefore this document must not be interpreted as evidence that the current branch passes local build/test execution. Implementation work must perform all verification commands listed at the end.

---

# 2. Executive diagnosis

AtlasFetch does not need a rewrite. It needs consolidation.

Most visible defects come from five boundaries that are currently blurred:

```text
collection ↔ formatting
sampling ↔ rendering
scene ↔ layout preset ↔ runtime mode
configuration identity ↔ derived presentation values
terminal capability ↔ assumed terminal behavior
```

The central architectural direction is:

```text
Linux probes / optional commands
            │
            ▼
      typed SystemData
            │
     ┌──────┴────────┐
     ▼               ▼
stable profile   MonitorSampler
                     │
                     ▼
               MonitorSnapshot
     └──────┬────────┘
            ▼
      ViewModel / fields
            │
            ▼
     LayoutPlanner
   width + height + density
            │
            ▼
        LayoutPlan
            │
            ▼
      StyledDocument
        │          │
        ▼          ▼
   ANSI backend   Ratatui backend
```

The project should stop growing new rendering modes or collectors until the P0/P1 correctness and boundary work below is complete.

---

# 3. Priority model

## P0 — correctness / user-visible broken behavior

A P0 item can produce demonstrably wrong output, contradictory UI/runtime behavior, broken migration, or a mode that is functionally invalid.

## P1 — architecture / portability / robustness

A P1 item is not always visible immediately but is a major source of recurring bugs, hangs, portability failures, broken live behavior, or unstable public contracts.

## P2 — product quality / packaging / maintainability

A P2 item improves usability, consistency, release discipline, downstream packaging, and long-term maintenance after correctness is secured.

## P3 — optional polish

A P3 item is valuable but should not block the core consolidation milestone.

---

# 4. Consolidated findings

## 4.1 System probing and non-systemd portability

### AF-001 — P0 — `*_bar` fields render `?`

The TUI creates synthetic field keys such as `cpu_bar`, `gpu_bar`, `memory_bar`, `disk_bar`, `vram_bar`, and `load_bar`, while `SysInfo::get()` does not define those keys. `FieldWidget::render_inherent()` consequently falls back to `?`.

**Required direction:** bar presentation is a field rendering style, not a fake system-data key.

Suggested model:

```rust
pub enum FieldPresentation {
    Text,
    Bar,
}

pub struct FieldDef {
    pub field: String,
    pub icon: String,
    pub label: String,
    pub enabled: bool,
    pub presentation: FieldPresentation,
}
```

Migration must map old `*_bar` field names to the base field plus `presentation = Bar`.

### AF-002 — P0 — WM detection can report the DE instead of the WM

`detect_wm()` checks `XDG_CURRENT_DESKTOP` / `DESKTOP_SESSION` before WM-specific evidence. KDE can therefore become the reported WM instead of KWin.

**Required direction:** collect `desktop_environment` and `window_manager` independently. Prefer explicit WM environment/process evidence for WM; use XDG desktop/session for DE.

### AF-003 — P0 — CPU “cores” are logical processors

The current `/proc/cpuinfo` count increments per `processor` and labels the result as cores.

**Required direction:** either label it as logical CPUs/threads or derive physical cores from sysfs topology and keep both values typed.

### AF-004 — P0 — ARM/Mali detection is nested under Qualcomm KGSL flow

Mali/PowerVR/platform fallbacks are currently located inside a Qualcomm path, so a Mali-only device without KGSL does not reach those probes.

**Required direction:** vendor-specific probes are independent fallback branches, not nested under unrelated hardware checks.

### AF-005 — P0 — refresh-rate detection parses the wrong source

The current DRM `modes` handling attempts to interpret mode text as if a final token contained refresh Hz. DRM `modes` normally provides mode names/resolutions rather than a reliable active refresh rate.

**Required direction:** use an appropriate active connector/mode source when available, or omit refresh rate rather than inventing it.

### AF-006 — P0 — fresh non-TTY invocation can launch the TUI

When the config does not exist, first-run setup occurs before static/non-TTY fallback. A fresh `atlasfetch | cat` can therefore try to open the TUI.

**Required direction:** interactive setup is only eligible when both stdin and stdout are terminals. Non-interactive first run creates/uses defaults and emits static/machine output without opening a full-screen editor.

### AF-007 — P1 — init and service manager are not modeled

AtlasFetch should not implement “systemd or unknown”.

**Required direction:** add separate typed values:

```rust
pub enum InitSystem {
    Systemd,
    Dinit,
    Runit,
    S6,
    SysVInit,
    OpenRcInit,
    Shepherd,
    Other(String),
}

pub enum ServiceManager {
    Systemd,
    OpenRc,
    Dinit,
    Runit,
    S6,
    Shepherd,
    Other(String),
}
```

Detect PID 1 primarily from `/proc/1/comm` and `/proc/1/cmdline`. Detect service-manager evidence separately. Do not infer that OpenRC is PID 1 simply because OpenRC commands/files exist.

### AF-008 — P1 — config path ignores `XDG_CONFIG_HOME`

The project uses `BaseDirs::home_dir().join(".config")` rather than the XDG config directory.

**Required direction:** use `BaseDirs::config_dir()` / `ProjectDirs` semantics, while preserving `--config` overrides exactly.

### AF-009 — P1 — terminal detection reports `$TERM`, not emulator

`$TERM` describes terminal capability type, not necessarily the terminal emulator.

**Required direction:** collect emulator identity separately using known environment markers/process ancestry, with `$TERM` retained as terminal type/capability evidence.

### AF-010 — P1 — shell detection reports login/default shell, not current shell

`$SHELL` is not guaranteed to identify the current interactive shell.

**Required direction:** inspect parent process identity first, then fall back to `$SHELL`.

### AF-011 — P1 — static and live collectors diverge

Battery and temperature logic differs between `info.rs` and `component/monitor.rs`, causing different behavior depending on presentation path.

**Required direction:** one probe layer, multiple consumers. Live polling may have different cadence, but not different hardware semantics.

### AF-012 — P1 — package counting semantics vary or are incomplete

Known problems include:

- Nix counting closure/store paths instead of installed package semantics;
- Gentoo using world atoms instead of installed package DB entries;
- Flatpak potentially counting runtimes when application count is intended;
- Alpine/APK missing;
- Guix missing.

**Required direction:** document the exact meaning of `packages` and implement backend-specific providers behind one interface. Missing manager support is not an error.

### AF-013 — P1 — battery/sensor discovery is too narrow

Static battery logic hardcodes common names; CPU temperature paths differ; health information may be missing even when design/full charge information can derive it.

**Required direction:** enumerate power supplies by `type == Battery`, enumerate hwmon/thermal sources, and derive health only from valid design/full ratios.

### AF-014 — P1 — username fallback is conceptually wrong

`/proc/self/uid_map` describes UID mapping, not a username.

**Required direction:** `LOGNAME`/`USER` fallback plus `getuid()`/`getpwuid_r()` when needed.

### AF-015 — P1 — unnecessary external `uname`

Architecture can be collected through libc, already a dependency.

**Required direction:** use `libc::uname` instead of spawning `uname -m`.

### AF-016 — P1 — Wi-Fi SSID fallback is too narrow

Only `iwgetid` is used despite it often not being installed.

**Required direction:** prefer a native/netlink-capable approach if practical; otherwise bounded fallback order such as `iw` then `iwgetid`, with optional NetworkManager-specific probing only when explicitly present.

### AF-017 — P1 — ARM device model coverage is incomplete

Android properties are not sufficient for generic ARM Linux.

**Required direction:** add device-tree model paths such as `/proc/device-tree/model` and `/sys/firmware/devicetree/base/model`.

### AF-018 — P1 — GPU model assumes one device and can misreport shared memory

Only the first card may be reported, hybrid systems can lose a GPU, and Qualcomm fallback can describe system RAM as VRAM/shared VRAM in a misleading way.

**Required direction:** model GPUs as a collection. If memory is shared system memory, label it explicitly rather than treating it as dedicated VRAM.

### AF-019 — P1 — local IP selection uses an IPv4 route trick

UDP `connect` to `8.8.8.8:80` is route-selection behavior, not interface enumeration, and is IPv4/default-route biased.

**Required direction:** enumerate interfaces (`getifaddrs` or equivalent), filter loopback/link-local according to policy, and choose/display addresses deterministically.

### AF-020 — P1 — memory fallback overstates used memory

If `MemAvailable` is absent, current behavior can make used memory equal total memory.

**Required direction:** use a documented fallback based on `MemFree + Buffers + Cached + SReclaimable - Shmem` where available.

### AF-021 — P2 — stale collector comments/fields

`info.rs` claims proc/sys/env-only behavior while invoking multiple commands, and `SysInfo` contains fields that are not consistently populated.

**Required direction:** update comments and either implement, type as optional, or remove dead fields before they become public API debt.

---

## 4.2 Rendering and component architecture

### AF-022 — P0/P1 — live sampling occurs inside rendering

`MonitorComponent::render_styled()` performs system reads, CPU sampling, and potentially `nvidia-smi`. First CPU sample sleeps. Any redraw can therefore become a hardware sampling event.

**Required direction:** `MonitorSampler` owns sampling and produces `MonitorSnapshot`; renderer consumes snapshots only.

```rust
pub struct MonitorSnapshot {
    pub sampled_at: std::time::Instant,
    pub cpu_usage_pct: Option<f32>,
    pub memory_usage_pct: Option<f32>,
    pub gpu_usage_pct: Option<f32>,
    pub cpu_temp_c: Option<f32>,
    pub battery: Option<BatterySnapshot>,
}

pub struct MonitorSampler {
    previous_cpu: Option<CpuTimes>,
}

impl MonitorSampler {
    pub fn sample(&mut self, ctx: &ProbeContext) -> MonitorSnapshot;
}
```

### AF-023 — P0/P1 — Classic eagerly renders Monitor even when not shown

Static Classic computes monitor lines before checking whether monitor content fits. This can trigger expensive collection and influence height even when the panel is ultimately omitted.

**Required direction:** Classic static scene does not sample/render Monitor unless the scene contract explicitly includes a supplied monitor snapshot. The default Classic fetch should not include live metrics implicitly.

### AF-024 — P1 — `Component` is overabstracted for a fixed component set

String names plus `Any` downcasts are used to recover concrete components.

**Required direction:** prefer typed scene input/context for the fixed built-in components. A dynamic registry should only exist if a real plugin API is introduced.

### AF-025 — P1 — duplicate styled-output models

`render::StyledSegment`, `component::StyledSpan`, widget output, and per-component ANSI serialization overlap.

**Required direction:** one model:

```rust
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

pub type StyledLine = Vec<StyledSpan>;

pub struct StyledDocument {
    pub lines: Vec<StyledLine>,
}
```

ANSI and Ratatui are backends over this model.

### AF-026 — P1/P2 — ANSI writer emits excessive reset/style sequences

Current serialization resets after every span; ASCII generation can produce one span per character.

**Required direction:** add a stateful ANSI writer that changes only differing attributes and groups adjacent compatible spans.

### AF-027 — P0/P1 — `--just-ascii` is a separate renderer that diverges

It has different dedent rules, color-direction handling, and Unicode padding behavior from `AsciiComponent`.

**Required direction:** `--just-ascii` must render through the same ASCII component/model/backend path as every other static output.

### AF-028 — P1 — Unicode truncation mixes byte/char/display-column units

`FieldWidget` measures with `unicode-width` but truncates via `chars().take()` and compares byte length.

**Required direction:** one utility:

```rust
pub fn truncate_display_width(text: &str, max_columns: usize) -> Cow<'_, str>;
```

Prefer grapheme-aware iteration if a dependency is accepted; otherwise guarantee valid Unicode and display-column bounds.

### AF-029 — P1 — static output accepts arbitrary terminal controls from custom text

Custom ASCII and editable labels can contain ESC/OSC/C0/C1 control sequences that are copied into the ANSI output stream.

**Required direction:** static user-controlled content must be sanitized. PTY output is exempt because escape interpretation is the purpose of that path.

Safe static policy:

- allow printable Unicode;
- allow newline/tab only where the model explicitly supports them;
- reject/strip ESC, BEL, C0/C1 controls, OSC introducers, and similar terminal-control bytes;
- apply the same utility to custom logo text and editable labels/icons where appropriate.

---

## 4.3 Scene/mode system

### AF-030 — P0/P1 — scene width metadata is not an actual layout gate

`Scene::min_width()` exists, but rendering does not perform a true measurement/planning phase. `saturating_sub()` prevents integer underflow but not visual overflow.

**Required direction:** scenes plan from measured content and `RenderConstraints { width, height, density, capabilities }`.

```rust
pub struct RenderConstraints {
    pub width: usize,
    pub height: Option<usize>,
    pub density: Density,
    pub capabilities: TerminalCapabilities,
}
```

### AF-031 — P0 — Classic has no correct logo-missing/oversized fallback

Classic mathematically positions panels around `logo_origin` and `ascii_w`. With no logo, left-side width collapses toward an artificial minimum. Oversized logos do not trigger a full composition fallback.

**Required direction:** Classic chooses one of explicit plans:

```text
Wide:      left | logo | right
Medium:    logo on top, left/right below
Narrow:    logo on top, one compact field list below
No logo:   left/right or one-column data without reserving logo space
```

### AF-032 — P1 — ClassicFetch is the best current responsive baseline

`compose_logo_and_info()` already falls back from side-by-side to stacked when remaining info width is too small.

**Required direction:** preserve this behavior, add height constraints, shared truncation, optional small-logo selection, and density support.

### AF-033 — P0/P1 — Dashboard assumes a fixed 50/50 grid

The ASCII width does not participate in the plan, no-logo mode wastes a half-screen, and hardcoded block headers introduce a separate visual language.

**Required direction:** rebuild Dashboard as semantic groups/cards. Two columns when measured groups fit; otherwise one column. Logo is optional and never reserves an empty quadrant.

### AF-034 — P0/P1 — Cockpit is redundant as a static scene

Current Cockpit is effectively `logo top + system/monitor below`, displays heavy rules even when logo is absent, and couples static fetch to live metrics.

**Required direction:** remove Cockpit from static compositions after a compatibility/deprecation window and reuse the name, if desired, for a live workspace presentation.

### AF-035 — P0 — Monitor Mode preview does not match real Live output

TUI preview uses `render_monitor_split()` while the actual live workspace renders `render_scene(scene, ...)`. The UI can therefore approve one composition and run another.

**Required direction:** delete the special preview-only composition path. Preview must call the same workspace planner used by Live.

### AF-036 — P0 — live scenes do not know available height

The live workspace allocates a percentage of terminal rows but scenes receive width only. Large logos/content are silently clipped.

**Required direction:** height is a first-class constraint; live workspace selects compact/full layout based on actual region dimensions.

### AF-037 — P0/P1 — `AppLayout` is not a meaningful second layout axis

Centered/Compact/Wide/Minimal/Balanced mostly mutate gap/padding/max value width and have little/no semantic effect on non-Classic scenes. Minimal claims “no ASCII” but does not implement that contract.

**Required direction:** replace with:

```rust
pub enum Composition {
    Classic,
    ClassicFetch,
    Dashboard,
    Minimal,
}

pub enum Density {
    Compact,
    Comfortable,
    Spacious,
}

pub enum RuntimeMode {
    Fetch,
    Live,
}
```

`Minimal` becomes a real composition; `Balanced` disappears; `Wide` maps to `Spacious`; `Compact` remains density; existing config is migrated deterministically.

---

## 4.4 TUI functional defects

### AF-038 — P0 — multiline ASCII paste is not robust

The paste editor handles only key events, uses Enter as “finish”, and does not enable bracketed paste. Multiline terminal paste can therefore terminate on a newline.

**Required direction:** enable bracketed paste and consume `Event::Paste(String)`. Use an explicit confirmation chord such as `Ctrl+S`/`Ctrl+Enter`, while ordinary newline can exist in pasted content.

### AF-039 — P1 — built-in logos disappear when a custom logo directory is non-empty

`available_logos()` selects filesystem keys instead of returning a union.

**Required direction:** `LogoRegistry` merges built-ins and user files; user source overrides duplicate keys but never hides unrelated built-ins.

### AF-040 — P1 — file browser hides dotfiles/directories

The browser skips names starting with `.`, making common Linux customization paths awkward/inaccessible.

**Required direction:** show hidden files or add a visible `h` toggle; handle symlinked directories deliberately.

### AF-041 — P0/P1 — TUI validates too late

`Ctrl+S` can close the editor before `Config::save()` rejects invalid input.

**Required direction:** validate inside the editor before setting `saved=true` or exiting. Keep the user in context and display the concrete validation error.

### AF-042 — P1 — custom palette parser silently drops invalid tokens

`filter_map(Color::from_hex_opt)` accepts partially invalid input.

**Required direction:** parse all tokens and report the first invalid token; do not silently modify the intended palette.

### AF-043 — P1 — custom palette selection state lies

If active colors do not match a built-in theme, the UI falls back to preset index 0.

**Required direction:** `ThemeSelection::{Preset(id), Custom}`.

### AF-044 — P0/P1 — filtered logo list and selection indices can diverge

`ascii_selected` indexes the original logo list while Ratatui `ListState` is used against a filtered list.

**Required direction:** translate original index to filtered-list position for rendering and scrolling; tests must cover search navigation and special rows.

### AF-045 — P1 — monitor preview refresh does not refresh all source data consistently

The preview periodically redraws, but not every field derived from `SysInfo` is refreshed; direct monitor sampling hides this inconsistency.

**Required direction:** fixed by `MonitorSampler`/snapshot separation and an explicit preview controller update cycle.

### AF-046 — P1 — custom logo load failure silently falls back

Selecting a file can ultimately show default ASCII rather than clearly reporting the file error.

**Required direction:** logo resolution returns a typed result/source and UI keeps the chooser open on failure.

---

## 4.5 TUI visual redesign

The TUI should remain a terminal-native Ratatui application, not a fake desktop GUI. The visual objective is **editor with canvas**, not **dashboard made of nested boxes**.

### AF-047 — P2 — excessive borders/chrome

Use borders only for spatially meaningful containers such as preview and modal overlays. Replace many nested rounded boxes with spacing, separators, accent markers, and typography.

### AF-048 — P2 — preview should dominate the workspace

Current wide layout uses approximately 45% controls / 55% preview.

**Target:** approximately 38–40% controls / 60–62% preview on wide terminals, with breakpoints rather than one binary threshold.

### AF-049 — P2 — permanent Welcome/Save tabs waste navigation

**Target tabs:** `Theme | Layout | Fields | Logo` (plus `General` only if the configuration surface genuinely requires it). Welcome is first-run-only; save is a global action.

### AF-050 — P2 — header is too visually heavy

Prefer a single-line identity/status header such as:

```text
atlasfetch config                                      ● modified
```

### AF-051 — P2 — TUI uses a second hardcoded identity palette

The editor currently uses many fixed purple/pink/blue values independent of the selected palette.

**Required direction:** mostly neutral UI colors plus one semantic accent derived from the active theme, with contrast fallback.

### AF-052 — P2 — Mode UI mixes unrelated concepts

Scene, AppLayout, and persistent Monitor Mode are presented together.

**Required direction:** after the mode-model migration, present Composition + Density, with Live startup as a separate toggle/property.

### AF-053 — P2 — field editing should communicate order and movement

Show row number/order, enabled state, icon, label, and side. Provide a discoverable key to move between panels, not just focus them.

### AF-054 — P2 — ASCII tab should behave like a source browser

Present source controls (`Built-in`, `File`, `Paste`, `None`), visible search, result count, and logo list. The global preview remains the visual result.

### AF-055 — P2 — modal size should be contextual

Rename input does not need a 70%-screen overlay; file browser/help may. Use content-specific dimensions.

### AF-056 — P2 — focus state is redundant

Do not simultaneously use colored border + `[focused]` + arrow marker for the same state. Prefer one strong selection marker plus accent.

### AF-057 — P2 — responsiveness needs multiple breakpoints

Recommended policy:

```text
>= 120 columns: controls 36–40%, preview 60–64%
80–119:        adaptive side-by-side or stacked based on height
52–79:         compact navigation + stacked controls/preview
< 52x16:       explicit resize fallback
```

---

## 4.6 Configuration and theme system

### AF-058 — P0 — config migration dispatch is wrong for explicit v1 and future versions

`Config::load()` only tries old Python migration when there is no `version` field. A genuine `{ "version": 1 }` document is treated as invalid. A future `{ "version": 3 }` document can be renamed `.invalid` and replaced by defaults.

**Required direction:** parse version first and dispatch explicitly.

```rust
pub enum LoadedConfig {
    Current(Config),
    Migrated(Config),
}

pub enum ConfigLoadError {
    Io(std::io::Error),
    InvalidJson(serde_json::Error),
    UnsupportedFutureVersion(u32),
    InvalidCurrentSchema(String),
}
```

Rules:

- no `version`: try legacy unversioned Python schema;
- `version == 1`: migrate v1;
- `version == 2`: parse/validate v2;
- `version > 2`: error without moving, rewriting, or replacing the file;
- corrupt current config may be preserved as `.invalid` only through an explicit recovery policy, not a hidden read side effect.

### AF-059 — P1 — normal config load mutates disk on invalid input

A normal read currently renames invalid config and writes defaults.

**Required direction:** separate load from recovery. A static fetch can report a useful error; `config recover`/interactive setup can offer preservation + reset explicitly.

### AF-060 — P1 — unknown JSON fields can be ignored silently

Serde defaults make typos such as `intervall_ms` fall back silently.

**Required direction:** version-aware unknown-field diagnostics. If strict `deny_unknown_fields` conflicts with forward compatibility, inspect raw JSON keys before typed deserialization and report unknown keys in the supported schema.

### AF-061 — P1/P2 — `custom_palettes` is schema without a complete product feature

The field exists but migration/management is incomplete.

**Required direction:** either implement named custom palette CRUD/select semantics or remove the field before further public-schema commitments. Preferred plan in this document: implement it as part of the theme model.

### AF-062 — P1 — “Theme” currently changes only the logo palette

Preset application changes `logo.colors`, while title/panel/separator/companion colors remain separate or hardcoded.

**Required direction:** create semantic `Theme`:

```rust
pub struct ThemeDefinition {
    pub id: String,
    pub logo_palette: Vec<Color>,
    pub accent: Color,
    pub accent_secondary: Color,
    pub text: Color,
    pub muted: Color,
    pub warning: Color,
    pub critical: Color,
    pub panel_value: Color,
    pub separator: Color,
}
```

If this scope is rejected, rename the feature everywhere from Theme to Palette. Do not keep ambiguous semantics.

### AF-063 — P1 — preset identity is derived by RGB equality

Current UI infers selected preset by comparing palette arrays.

**Required direction:** persist source identity:

```rust
pub enum ThemeSource {
    Preset { id: String },
    Custom { id: Option<String> },
}
```

### AF-064 — P1 — Intersex special rendering is inferred by matching colors

A custom palette with the same yellow/purple pair can accidentally trigger Intersex ring logic.

**Required direction:** semantic pattern metadata:

```rust
pub enum PalettePattern {
    Linear,
    IntersexRing,
}
```

### AF-065 — P1 — config types use strings where enums exist conceptually

`color_dir` is a string and similar settings are stringly typed.

**Required direction:** use serializable enums with explicit migration aliases.

---

## 4.7 Logo subsystem

### AF-066 — P1 — logo source precedence is inconsistent

`logo_dir()` chooses binary-adjacent logos or user config logos as one directory, while built-ins are separately embedded.

**Required direction:** explicit registry:

```rust
pub struct LogoRegistry {
    embedded: BTreeMap<String, &'static str>,
    user_dir: PathBuf,
}

pub enum LogoSource {
    Embedded,
    UserFile(PathBuf),
    ExplicitFile(PathBuf),
    Disabled,
}
```

Resolution precedence for named keys: user override > embedded built-in.

### AF-067 — P1/P2 — `ensure_logos()` is legacy complexity after embedding

Copying binary-adjacent logos into the user config directory creates unnecessary source states.

**Required direction:** built-ins remain embedded/read-only; user directory contains only user assets/overrides. Remove automatic copying after migration compatibility is handled.

### AF-068 — P2 — small-logo selection is width-only and duplicated

Small variant choice appears in loader/editor logic and ignores scene/height constraints.

**Required direction:** logo size selection belongs to the layout planning stage based on measured constraints, not global terminal width alone.

### AF-069 — P2 — logo provenance/licensing is undocumented

Hundreds of logo files are embedded in the distributed binary, but the repository currently has no dedicated third-party logo provenance/attribution manifest.

**Required direction:** audit logo sources and create `THIRD_PARTY.md` or `docs/LOGO_SOURCES.md` documenting origin/license/attribution expectations. Do not assert a license for a logo unless verified.

---

## 4.8 CLI and machine-readable contracts

### AF-070 — P0/P1 — two CLI APIs coexist with manual precedence

Legacy top-level flags and structured subcommands can conflict or silently ignore options. Example: `--scene` is handled before JSON format, so combinations can violate user intent.

**Required direction:** parse into one canonical action:

```rust
pub enum Action {
    Fetch(FetchOptions),
    Monitor(MonitorOptions),
    Config(ConfigAction),
    Preset(PresetAction),
    Logos(LogosAction),
    Benchmark(BenchmarkOptions),
    Update,
}

pub struct FetchOptions {
    pub scene: Option<Composition>,
    pub format: OutputFormat,
    pub color: ColorPolicy,
}
```

Legacy flags normalize into `Action` or fail on explicit conflict. They do not create separate dispatch paths.

### AF-071 — P1 — no color policy / `NO_COLOR`

Human ANSI output and preset swatches emit color unconditionally.

**Required direction:**

```rust
pub enum ColorPolicy {
    Auto,
    Always,
    Never,
}
```

`Auto` considers stdout TTY, `TERM=dumb`, `NO_COLOR`, and explicit override.

### AF-072 — P1/P2 — glyph capability is assumed

Default icons and Powerline separator use Nerd Font/PUA glyphs.

**Required direction:**

```rust
pub enum GlyphMode {
    Auto,
    Nerd,
    Unicode,
    Ascii,
}
```

Provide non-PUA fallback icon/separator sets so “Nerd Fonts recommended, not required” becomes true in practice.

### AF-073 — P1 — JSON schema is coupled directly to internal `SysInfo`

`serde_json::to_value(info)` makes internal field changes silently alter public JSON.

**Required direction:** explicit versioned DTO:

```rust
#[derive(Serialize)]
pub struct SystemInfoV1 {
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub cpu: Option<CpuInfoV1>,
    pub memory: Option<MemoryInfoV1>,
    // explicitly defined public fields only
}
```

Typed internal data converts to the stable DTO. A public JSON change requires an intentional schema-version decision.

### AF-074 — P2 — completions/manpage missing

Use Clap metadata to generate shell completions and a man page as release/package artifacts.

---

## 4.9 Live workspace / PTY terminal

### AF-075 — P1 — child shell is spawned before terminal session guard is established

`live::run()` creates/spawns the PTY child before entering alternate-screen/raw-mode state. If later initialization fails, cleanup behavior depends on object drops/library semantics and is not explicitly controlled.

**Required direction:** introduce an explicit `LiveChildGuard` that kills/waits when still running. Establish a clearly ordered session lifecycle and cover error paths.

```rust
pub struct LiveChildGuard {
    child: Box<dyn portable_pty::Child + Send>,
    exited: bool,
}

impl LiveChildGuard {
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>>;
    pub fn terminate(&mut self) -> Result<()>;
}

impl Drop for LiveChildGuard {
    fn drop(&mut self) {
        if !self.exited {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
```

Adapt the exact trait bounds to `portable-pty` APIs used by the project.

### AF-076 — P1 — PTY reader thread is detached

The reader thread is spawned and never joined.

**Required direction:** session state owns a stop flag/reader join handle and performs orderly shutdown after PTY closure/child exit.

### AF-077 — P1 — explicit signal lifecycle is absent

No dedicated SIGTERM/SIGHUP terminal restoration path was found. RAII handles normal Rust unwinding, but Unix signals do not guarantee destructors run.

**Required direction:** install signal handling appropriate for a TUI process, request graceful shutdown from the event loop, restore terminal state, and terminate/wait the child. Do not try to handle SIGKILL.

### AF-078 — P1 — `TERM=xterm-256color` overstates emulation capability

The code itself works around Fish terminal queries because the embedded renderer does not answer them.

**Required direction:** either implement the required bidirectional query responses, adopt a more complete terminal emulation layer, or document/set a capability contract that does not falsely promise xterm behavior. Avoid accumulating shell-specific exceptions.

### AF-079 — P1 — modified navigation/function keys are not encoded fully

Ctrl/Shift/Alt combinations for arrows/home/end/function keys are largely collapsed to unmodified sequences.

**Required direction:** implement a tested key encoder for common xterm-style modified key sequences. Ensure Ctrl+Shift combinations do not accidentally collapse into control characters such as Ctrl+C.

### AF-080 — P1 — bracketed paste is not enabled

Live handles `Event::Paste` but does not establish bracketed paste mode explicitly.

**Required direction:** enable on session entry and disable on exit. Mirror this in the configurator paste workflow.

### AF-081 — P1/P2 — 10,000-line scrollback is stored but inaccessible

The VT parser has scrollback capacity, but the UI only renders the current screen and PageUp/PageDown are forwarded to the child.

**Required direction:** add workspace scrollback navigation on a reserved modifier chord and a visible offset indicator; retain normal PageUp/PageDown forwarding when not using that chord.

### AF-082 — P1 — redraw cadence and sampling cadence are coupled indirectly

Shell output can trigger redraws, and because monitor sampling currently lives in rendering, shell activity can trigger extra sampling.

**Required direction:** fixed by AF-022. Redraw may happen frequently; sampling happens only on the configured sampling schedule.

---

## 4.10 Terminal capabilities and accessibility

### AF-083 — P1 — terminal capabilities are scattered assumptions

Color depth, glyph support, interactivity, and terminal type are not represented together.

**Required direction:**

```rust
pub struct TerminalCapabilities {
    pub interactive: bool,
    pub color_depth: ColorDepth,
    pub glyph_mode: GlyphMode,
    pub term: Option<String>,
}

pub enum ColorDepth {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}
```

Auto-detection must always be overrideable.

### AF-084 — P2 — selection/status meaning depends too heavily on color

TUI focus and warning states should have textual/shape cues in addition to hue.

**Required direction:** accent color enhances state but does not solely encode it.

---

## 4.11 Updater

### AF-085 — P1 — source checkout validation is weak

Any directory containing `.git`, `Cargo.toml`, and `src/main.rs` can be treated as AtlasFetch.

**Required direction:** validate Cargo package name and, when appropriate, repository remote identity. Do not rely on path shape alone.

### AF-086 — P1/P2 — generic `atlasfetch update` executes newly pulled source code

The updater runs `git pull`, then Cargo builds remote code including `build.rs`. This is a developer checkout workflow, not a neutral binary updater.

**Required direction:** either rename/source-scope it (`dev-update`) or implement official-release update with checksum/signature/attestation verification. Package-manager/Nix installations should be told to update through their package source rather than overwritten in `~/.local/bin`.

### AF-087 — P2 — install uses external `install`

Use Rust file copy + permissions + same-directory atomic replacement where possible to reduce external command assumptions.

---

## 4.12 CI, release, Nix, supply chain

### AF-088 — P1 — CI portability coverage is narrow

CI runs on Ubuntu and release tests only GNU x86_64 before building GNU/musl x86_64 artifacts.

**Required direction:** add fixture-based distro/init tests independent of host OS and at least build/test coverage for musl plus planned aarch64 targets.

Fixture profiles should include:

- Arch/pacman;
- Artix + OpenRC;
- Artix + dinit;
- Artix + runit;
- Void + xbps/runit;
- Alpine + apk/OpenRC;
- Gentoo + Portage/OpenRC;
- Debian + dpkg;
- Fedora + rpm;
- NixOS;
- ARM device-tree + Mali-like GPU + battery fixtures.

### AF-089 — P1 — `flake.nix` exists without a checked-in `flake.lock`

The repository root currently contains `flake.nix` but no `flake.lock`, so nixpkgs/flake-utils inputs are not pinned by the repository.

**Required direction:** generate and commit `flake.lock`; CI should evaluate/build the flake and update policy should be explicit.

### AF-090 — P1/P2 — GitHub Actions used by release are version tags, not immutable SHAs

Release has write permission and uses third-party actions by mutable major tags.

**Required direction:** pin release-critical actions to reviewed commit SHAs, with comments noting the upstream version.

### AF-091 — P2 — release version has multiple independent sources

Cargo and Nix hardcode `2.0.0`; Git tags trigger releases without validating against Cargo version.

**Required direction:** Cargo package version is the source of truth. Release workflow verifies tag `vX.Y.Z` equals `package.version`; Nix derives or is checked automatically against Cargo.

### AF-092 — P2 — no explicit MSRV

Add `rust-version` in Cargo metadata and an MSRV CI check.

### AF-093 — P2 — release archive is only the binary

**Required direction:** release archive should contain at minimum binary, LICENSE, README, generated manpage/completions, and checksum. Add provenance/attestation when the release process is hardened.

### AF-094 — P2 — no dependency policy automation

**Required direction:** add Dependabot/Renovate and a deliberate Rust dependency policy (for example `cargo-deny` for advisories/licenses/sources). Avoid blindly blocking every PR on untriaged transitive advisories; define review policy.

---

## 4.13 Documentation and public claims

### AF-095 — P1/P2 — README/DOCS describe desired architecture as current fact

Examples of claims that need implementation alignment or temporary correction:

- “four responsive scenes”;
- preview and final output use the exact same renderer;
- metrics redraw only when data changes;
- component renderer rather than unrelated scene functions;
- terminal/shell compatibility claims stronger than current emulation guarantees.

**Required direction:** docs follow verified behavior, not intended architecture.

### AF-096 — P2 — package metadata still centers atlasWM relationship

Update Cargo/Nix descriptions to describe AtlasFetch as a standalone composable Linux fetch/live terminal workspace.

### AF-097 — P2 — project hygiene files are missing

After core changes, add:

- `CHANGELOG.md`;
- `CONTRIBUTING.md`;
- `SECURITY.md`;
- issue templates for bug/layout/collector reports;
- compatibility/support policy.

---

## 4.14 Final hardening and test strategy

### AF-098 — P1/P2 — current scene snapshots are opaque hashes

Hash snapshots detect change but do not show readable diffs.

**Required direction:** use textual `.snap` fixtures or equivalent human-readable expected output. Keep hashing only if it adds a separate invariant.

### AF-099 — P1 — production scene tests omit important components and real logos

Current scene tests can omit Monitor/Companion and use tiny artificial ASCII.

**Required direction:** regression matrix includes real/large/small/no logo, long labels/values, disabled middle fields, full component set, and constrained width/height.

### AF-100 — P1/P2 — add property tests/fuzzing for layout/config/text

Properties:

```text
all static rendered lines <= planned width
renderer never panics for width >= 1 and height >= 1
valid config round-trip preserves semantics
future config version is never modified by load
sanitized static text contains no forbidden control sequences
truncate_display_width never exceeds requested display width
logo registry union preserves embedded keys when user directory is non-empty
```

Use `proptest` for structured input generation. Add `cargo-fuzz` targets later if the maintenance cost is acceptable, starting with config JSON and text sanitization.

### AF-101 — P2 — create a manual terminal compatibility matrix

Document release smoke tests for:

```text
kitty
foot
Ghostty
Alacritty
Konsole
GNOME Terminal
xterm
Linux virtual console
tmux
GNU screen
SSH session
```

Test representative dimensions, truecolor/256/no-color, Nerd/non-Nerd glyph mode, piping/redirection, and Live applications such as `less`, `nano`, `vim/neovim`, `fzf`, `htop/btop`, `ssh`, and `sudo`.

### AF-102 — P2 — define compatibility/SemVer policy

Document whether these are stable public contracts and what changes require migration/deprecation:

- config schema/version;
- JSON schema/version;
- command/subcommand names;
- legacy flags;
- composition IDs;
- preset IDs;
- logo keys;
- environment variables.

### AF-103 — P2 — add `atlasfetch doctor`

A diagnostic command can dramatically improve bug reports. Suggested machine/human-readable fields:

```text
AtlasFetch version
config path + schema version
terminal type + dimensions + color/glyph policy
selected composition + density + runtime preference
resolved logo source/key/size
shell identity
init + service manager
available optional collectors/commands
GPU/battery/sensor probe summary
```

The doctor command must not expose secrets or dump arbitrary environment variables.

---

# 5. Target file/module structure

This is an incremental target. Move code only when the corresponding task begins; do not create empty scaffolding for later phases.

```text
src/
├── main.rs                  # parse -> normalize Action -> dispatch
├── cli.rs                   # clap surface and legacy aliases
├── config.rs                # public config facade
├── config/
│   ├── schema.rs            # Config v2 structs/enums
│   ├── load.rs              # version dispatch + recovery policy
│   └── migrate.rs           # legacy/v1 -> current migration
├── info.rs                  # public probing facade
├── info/
│   ├── model.rs             # typed SystemData/SystemProfile types
│   ├── context.rs           # ProbeContext + CommandRunner
│   ├── os.rs
│   ├── cpu.rs
│   ├── gpu.rs
│   ├── memory.rs
│   ├── storage.rs
│   ├── desktop.rs
│   ├── terminal.rs
│   ├── network.rs
│   ├── power.rs
│   ├── sensors.rs
│   ├── init.rs
│   └── packages/
│       ├── mod.rs
│       ├── pacman.rs
│       ├── dpkg.rs
│       ├── rpm.rs
│       ├── xbps.rs
│       ├── apk.rs
│       ├── portage.rs
│       ├── nix.rs
│       └── guix.rs
├── ascii.rs                 # logo registry/load facade
├── theme.rs                 # semantic theme/palette definitions
├── render.rs                # render facade
├── render/
│   ├── model.rs             # StyledSpan/StyledDocument
│   ├── ansi.rs              # stateful ANSI backend
│   ├── capabilities.rs      # color/glyph/interactivity policy
│   ├── sanitize.rs          # static text control sanitization
│   ├── width.rs             # Unicode display width/truncation helpers
│   └── layout.rs            # RenderConstraints/LayoutPlan primitives
├── component/
│   ├── ascii.rs
│   ├── system.rs
│   └── scenes/
│       ├── mod.rs
│       ├── classic.rs
│       ├── classicfetch.rs
│       ├── dashboard.rs
│       └── minimal.rs
├── monitor.rs               # MonitorSampler + MonitorSnapshot
├── live.rs                  # live workspace coordinator
├── live/
│   ├── session.rs           # PTY child/thread/terminal guards
│   ├── input.rs             # key encoder + paste
│   └── view.rs              # live workspace layout + shell rendering
├── output.rs                # explicit JSON DTO conversion
├── tui/
│   ├── editor.rs
│   ├── events.rs
│   └── state.rs
├── update.rs
└── benchmark.rs
```

The exact split can be adjusted if a file would remain trivial, but responsibilities must stay separated.

---

# 6. Implementation phases

## Phase 0 — regression safety net and immediate P0 fixes

Deliverable: known broken behaviors have failing tests first and minimal fixes, without the large architecture refactor yet.

### Task 1: Add regression tests for known P0 behavior

**Files:**
- Modify: `src/component/mod.rs`
- Modify: `src/component/system.rs`
- Modify: `src/config.rs`
- Modify: `src/tui/editor.rs`
- Modify: `src/tui/events.rs`
- Modify: `src/main.rs` testable dispatch helpers as needed
- Create only if needed: `tests/regressions.rs`

**Interfaces:**
- Consumes: existing `Config`, `Scene`, TUI test backend, render functions.
- Produces: regression tests that lock current expected fixes before structural changes.

- [ ] **Step 1: write failing tests for disabled-field compaction, no-logo scene behavior, filtered-logo selection mapping, multiline paste event handling, and config v1/future-version behavior.**

Representative assertions:

```rust
#[test]
fn disabled_middle_field_does_not_leave_visual_hole() {
    let mut cfg = Config::default();
    cfg.display.left[1].enabled = false;
    let output = render_fixture(&cfg, Scene::Dashboard, 100, 30);
    assert!(!output.contains("\n                                      \n"));
}

#[test]
fn future_config_version_is_not_rewritten() {
    let raw = r#"{"version":999}"#;
    let result = load_from_fixture(raw);
    assert!(matches!(result, Err(ConfigLoadError::UnsupportedFutureVersion(999))));
}
```

- [ ] **Step 2: run the focused tests and verify they fail for the expected reasons.**

```bash
cargo test regressions -- --nocapture
```

- [ ] **Step 3: implement only the smallest correctness fixes necessary where architecture work is not required.**

Do not introduce the final module split in this task.

- [ ] **Step 4: run focused and full tests.**

```bash
cargo test --all-targets --all-features --locked
```

- [ ] **Step 5: commit.**

```bash
git add src tests
git commit -m "test: lock AtlasFetch v2 correctness regressions"
```

### Task 2: Fix first-run non-TTY dispatch and CLI conflict behavior

**Files:**
- Modify: `src/main.rs`
- Modify: `src/cli.rs`

**Interfaces:**
- Produces: one pre-dispatch decision about interactivity; explicit conflict tests.

- [ ] **Step 1: extract a pure decision helper for first-run setup eligibility.**

```rust
fn should_launch_first_run_setup(config_exists: bool, stdin_tty: bool, stdout_tty: bool) -> bool {
    !config_exists && stdin_tty && stdout_tty
}
```

- [ ] **Step 2: test that missing config + piped stdout returns false.**

```rust
#[test]
fn first_run_pipe_never_launches_tui() {
    assert!(!should_launch_first_run_setup(false, true, false));
}
```

- [ ] **Step 3: normalize or reject currently conflicting top-level flags enough to remove silent `--scene`/JSON precedence errors.**

- [ ] **Step 4: run CLI tests and full test suite.**

```bash
cargo test cli
cargo test --all-targets --all-features --locked
```

- [ ] **Step 5: commit.**

```bash
git commit -am "fix: make first-run and CLI dispatch deterministic"
```

### Task 3: Make config loading version-aware and non-destructive

**Files:**
- Modify: `src/config.rs`
- Create: `src/config/load.rs`
- Create: `src/config/migrate.rs`

**Interfaces:**
- Produces: explicit version dispatch and no hidden mutation for future versions.

- [ ] **Step 1: write tests for unversioned legacy, explicit v1, current v2, malformed JSON, and future v999.**

- [ ] **Step 2: implement a raw version probe before typed deserialization.**

```rust
fn document_version(value: &serde_json::Value) -> Option<u32> {
    value.get("version").and_then(|v| v.as_u64()).and_then(|v| u32::try_from(v).ok())
}
```

- [ ] **Step 3: route versions with no filesystem mutation in the read path.**

```rust
match document_version(&value) {
    None => migrate_unversioned(value),
    Some(1) => migrate_v1(value),
    Some(2) => parse_v2(value),
    Some(version) => Err(ConfigLoadError::UnsupportedFutureVersion(version)),
}
```

- [ ] **Step 4: move backup/reset behavior behind an explicit recovery function used by setup/reset flows.**

- [ ] **Step 5: run config tests and full suite.**

```bash
cargo test config
cargo test --all-targets --all-features --locked
```

- [ ] **Step 6: commit.**

```bash
git add src/config.rs src/config
git commit -m "fix: make config migration version-aware and non-destructive"
```

---

## Phase 1 — typed probing core and non-systemd reliability

Deliverable: system discovery is modular, fixture-testable, and not coupled to display strings.

### Task 4: Introduce `ProbeContext`, command timeout abstraction, and typed core model

**Files:**
- Modify: `src/info.rs`
- Create: `src/info/context.rs`
- Create: `src/info/model.rs`

**Interfaces:**
- Produces:

```rust
pub struct ProbeContext {
    pub proc_root: PathBuf,
    pub sys_root: PathBuf,
    pub etc_root: PathBuf,
    pub runner: CommandRunner,
}

pub struct CommandRunner {
    pub timeout: Duration,
}
```

Typed model should distinguish raw values from formatted values, for example:

```rust
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
}
```

- [ ] **Step 1: add fixture-root tests demonstrating `/proc` and `/sys` can be redirected away from the host.**
- [ ] **Step 2: implement context root helpers and bounded command execution.**
- [ ] **Step 3: migrate one low-risk collector (OS/kernel) to typed output as the pattern.**
- [ ] **Step 4: run tests and commit.**

```bash
cargo test info
cargo clippy --all-targets --all-features --locked -- -D warnings
git commit -am "refactor: introduce typed probe context"
```

### Task 5: Split CPU/memory/storage/network probes and fix known correctness defects

**Files:**
- Create: `src/info/cpu.rs`
- Create: `src/info/memory.rs`
- Create: `src/info/storage.rs`
- Create: `src/info/network.rs`
- Modify: `src/info.rs`

- [ ] **Step 1: add fixture tests for logical-vs-physical CPU count, missing `MemAvailable`, and multiple interfaces.**
- [ ] **Step 2: implement physical/logical CPU semantics without labeling threads as cores.**
- [ ] **Step 3: implement documented memory fallback.**
- [ ] **Step 4: replace UDP-to-8.8.8.8 IP selection with interface enumeration.**
- [ ] **Step 5: remove dead/useless disk fallback paths and keep statvfs semantics explicit.**
- [ ] **Step 6: run tests and commit.**

```bash
cargo test info::cpu info::memory info::network
cargo test --all-targets --all-features --locked
git commit -am "fix: make core Linux probes accurate and fixture-driven"
```

### Task 6: Split GPU/device/power/sensor probes and fix ARM/hybrid behavior

**Files:**
- Create: `src/info/gpu.rs`
- Create: `src/info/power.rs`
- Create: `src/info/sensors.rs`
- Modify: `src/info.rs`

- [ ] **Step 1: create ARM fixture tests where Mali exists without KGSL.**
- [ ] **Step 2: add device-tree model fixtures.**
- [ ] **Step 3: model multiple GPUs and distinguish dedicated/shared memory.**
- [ ] **Step 4: enumerate batteries and hwmon/thermal sources.**
- [ ] **Step 5: remove refresh-rate parsing that cannot produce trustworthy values; add a correct source only if available.**
- [ ] **Step 6: run tests and commit.**

```bash
cargo test info::gpu info::power info::sensors
cargo test --all-targets --all-features --locked
git commit -am "fix: harden GPU ARM battery and sensor probing"
```

### Task 7: Add init/service-manager and package-manager backends

**Files:**
- Create: `src/info/init.rs`
- Create: `src/info/packages/mod.rs`
- Create package backend files listed in section 5.
- Modify: `src/info.rs`

- [ ] **Step 1: fixture-test PID 1 for systemd, dinit, runit, s6, SysVInit, OpenRC-init, and Shepherd.**
- [ ] **Step 2: fixture-test OpenRC service-manager evidence separately from PID 1.**
- [ ] **Step 3: implement package backends with explicitly documented counting semantics.**
- [ ] **Step 4: add APK and Guix support; correct Portage/Nix/Flatpak semantics.**
- [ ] **Step 5: verify no generic collector invokes systemctl/depends on systemd.**
- [ ] **Step 6: run tests and commit.**

```bash
cargo test info::init info::packages
cargo test --all-targets --all-features --locked
git commit -am "feat: make init and package probing distro-neutral"
```

---

## Phase 2 — pure rendering, capabilities, and responsive composition

Deliverable: a redraw is deterministic and performs no collection; all scenes use width+height planning.

### Task 8: Unify styled output and add static sanitization/width utilities

**Files:**
- Modify: `src/render.rs`
- Create: `src/render/model.rs`
- Create: `src/render/ansi.rs`
- Create: `src/render/sanitize.rs`
- Create: `src/render/width.rs`
- Modify: `src/widget.rs`
- Modify: `src/component/ascii.rs`
- Modify: `src/component/system.rs`

- [ ] **Step 1: add tests for ESC/OSC/BEL stripping and Unicode truncation.**
- [ ] **Step 2: introduce `StyledDocument` as the only styled model.**
- [ ] **Step 3: migrate widgets/components to the model.**
- [ ] **Step 4: implement a stateful ANSI backend.**
- [ ] **Step 5: route `--just-ascii` through the same component/backend.**
- [ ] **Step 6: verify rendered line display width never exceeds the planner width for fixture inputs.**
- [ ] **Step 7: commit.**

```bash
cargo test render component::ascii component::system
cargo test --all-targets --all-features --locked
git commit -am "refactor: unify styled rendering and sanitize static output"
```

### Task 9: Add terminal color/glyph capabilities

**Files:**
- Create: `src/render/capabilities.rs`
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/theme.rs`
- Modify: `src/widget.rs`

- [ ] **Step 1: test `NO_COLOR`, non-TTY auto mode, `TERM=dumb`, and explicit always/never overrides.**
- [ ] **Step 2: add `ColorPolicy`, `ColorDepth`, and `GlyphMode`.**
- [ ] **Step 3: provide Nerd/Unicode/ASCII separator/icon fallbacks.**
- [ ] **Step 4: ensure `preset list` also obeys color policy.**
- [ ] **Step 5: commit.**

```bash
cargo test capabilities cli
cargo test --all-targets --all-features --locked
git commit -am "feat: add terminal color and glyph capability policy"
```

### Task 10: Introduce `RenderConstraints` and `LayoutPlan`

**Files:**
- Create: `src/render/layout.rs`
- Modify: `src/component/mod.rs`
- Modify: scene renderers.

**Interfaces:**

```rust
pub struct RenderConstraints {
    pub width: usize,
    pub height: Option<usize>,
    pub density: Density,
    pub capabilities: TerminalCapabilities,
}

pub enum LayoutPlan {
    ClassicWide,
    ClassicLogoTopTwoColumns,
    ClassicStacked,
    ClassicNoLogo,
    ClassicFetchSideBySide,
    ClassicFetchStacked,
    DashboardTwoColumn,
    DashboardSingleColumn,
    Minimal,
    LiveFull,
    LiveCompact,
}
```

- [ ] **Step 1: write planner tests across representative width/height/logo dimensions.**
- [ ] **Step 2: make planning pure and independent from rendering.**
- [ ] **Step 3: ensure every plan declares its required measurements before composition.**
- [ ] **Step 4: commit.**

```bash
cargo test render::layout
git commit -am "refactor: add measured layout planning"
```

### Task 11: Rebuild scene model as Composition + Density

**Files:**
- Modify: `src/component/mod.rs`
- Create: `src/component/scenes/` files
- Modify: `src/layout.rs` then remove it when no longer referenced
- Modify: `src/config.rs` migration
- Modify: `src/cli.rs`

- [ ] **Step 1: add migration tests from Classic/Dashboard/Cockpit/ClassicFetch + AppLayout-derived settings.**
- [ ] **Step 2: implement `Composition::{Classic, ClassicFetch, Dashboard, Minimal}` and `Density`.**
- [ ] **Step 3: rebuild Classic using planner fallbacks.**
- [ ] **Step 4: preserve/polish ClassicFetch responsive behavior.**
- [ ] **Step 5: rebuild Dashboard with semantic groups and no empty logo quadrant.**
- [ ] **Step 6: implement true Minimal composition.**
- [ ] **Step 7: deprecate static Cockpit alias with a deterministic compatibility mapping for v2.x.**
- [ ] **Step 8: replace opaque scene hashes with readable snapshots.**
- [ ] **Step 9: commit.**

```bash
cargo test component::scenes
cargo test --all-targets --all-features --locked
git commit -am "refactor: rebuild responsive compositions"
```

---

## Phase 3 — MonitorSampler and Live workspace hardening

Deliverable: sampling cadence, redraw cadence, terminal lifecycle, and PTY behavior are independently controlled.

### Task 12: Move all volatile metric sampling out of rendering

**Files:**
- Create: `src/monitor.rs`
- Modify: `src/component/monitor.rs` or replace it with a pure view
- Modify: `src/live.rs`
- Modify: TUI preview controller.

- [ ] **Step 1: write a fake sampler test proving multiple redraws do not produce multiple samples.**

```rust
#[test]
fn shell_redraw_does_not_resample_metrics() {
    let sampler = CountingSampler::default();
    // one scheduled sample, multiple view redraws
    assert_eq!(sampler.calls(), 1);
}
```

- [ ] **Step 2: implement `MonitorSampler` and `MonitorSnapshot`.**
- [ ] **Step 3: remove sleeps/subprocess/sysfs reads from render functions.**
- [ ] **Step 4: make benchmark modes distinguish collect/render/end-to-end.**
- [ ] **Step 5: commit.**

```bash
cargo test monitor live
cargo test --all-targets --all-features --locked
git commit -am "refactor: separate live sampling from rendering"
```

### Task 13: Harden PTY child/thread/session lifecycle

**Files:**
- Create: `src/live/session.rs`
- Modify: `src/live.rs`
- Update Cargo dependencies only if a signal crate is justified.

- [ ] **Step 1: create tests around a fake child/session abstraction for normal exit, Ctrl+Q, render error, and terminal-init error.**
- [ ] **Step 2: introduce `LiveChildGuard` and reader join ownership.**
- [ ] **Step 3: establish terminal session before entering the main interactive loop and guarantee cleanup ordering.**
- [ ] **Step 4: add graceful SIGTERM/SIGHUP request handling where supported.**
- [ ] **Step 5: commit.**

```bash
cargo test live::session
cargo test --all-targets --all-features --locked
git commit -am "fix: guarantee live PTY and terminal cleanup"
```

### Task 14: Implement robust Live input, bracketed paste, and scrollback

**Files:**
- Create: `src/live/input.rs`
- Modify: `src/live.rs`
- Create/modify: `src/live/view.rs`

- [ ] **Step 1: table-test modified arrows/home/end/function keys.**
- [ ] **Step 2: enable/disable bracketed paste in session guard.**
- [ ] **Step 3: implement tested modified-key sequences instead of collapsing modifiers.**
- [ ] **Step 4: add reserved scrollback chord and visible scroll offset.**
- [ ] **Step 5: document the supported terminal-emulation contract and remove claims that exceed it.**
- [ ] **Step 6: commit.**

```bash
cargo test live::input live::view
cargo test --all-targets --all-features --locked
git commit -am "feat: harden live terminal input and scrollback"
```

---

## Phase 4 — TUI functional and visual consolidation

Deliverable: editor behavior is reliable, composition-focused, and preview-exact.

### Task 15: Fix paste, validation, filtered logo selection, file browser, and logo errors

**Files:**
- Modify: `src/tui/events.rs`
- Modify: `src/tui/editor.rs`
- Modify: `src/tui/state.rs`
- Modify: `src/ascii.rs`

- [ ] **Step 1: enable bracketed paste in TUI terminal session and handle `Event::Paste`.**
- [ ] **Step 2: validate before setting `saved=true`; display errors without leaving.**
- [ ] **Step 3: reject invalid custom-palette tokens explicitly.**
- [ ] **Step 4: map filtered logo source index -> visible list index.**
- [ ] **Step 5: allow hidden-file navigation and explicit load errors.**
- [ ] **Step 6: add TestBackend regressions for every behavior above.**
- [ ] **Step 7: commit.**

```bash
cargo test tui
cargo test --all-targets --all-features --locked
git commit -am "fix: make configurator editing reliable"
```

### Task 16: Redesign TUI around controls + canvas

**Files:**
- Modify: `src/tui/editor.rs`
- Modify: `src/tui/state.rs`
- Modify: `src/tui/events.rs`

- [ ] **Step 1: replace permanent Welcome/Save tabs with first-run welcome + global save.**
- [ ] **Step 2: rename/restructure tabs to Theme/Layout/Fields/Logo after new config model exists.**
- [ ] **Step 3: implement wide/medium/narrow breakpoints and preview-dominant proportions.**
- [ ] **Step 4: reduce nested borders and use one focus marker language.**
- [ ] **Step 5: derive TUI accent from semantic theme with contrast-safe fallback.**
- [ ] **Step 6: resize overlays by content type.**
- [ ] **Step 7: ensure Live preview calls the same Live planner/view as actual runtime.**
- [ ] **Step 8: add golden/TestBackend checks at 120x40, 90x30, 70x24, 52x16, and below-minimum fallback.**
- [ ] **Step 9: commit.**

```bash
cargo test tui
cargo test --all-targets --all-features --locked
git commit -am "refactor: turn setup into a composition editor"
```

---

## Phase 5 — themes, logos, CLI, JSON, and updater contracts

### Task 17: Implement semantic theme source and named custom palettes

**Files:**
- Modify: `src/theme.rs`
- Modify: `src/config/schema.rs`
- Modify: migrations
- Modify: TUI theme state
- Modify: preset CLI actions.

- [ ] **Step 1: add migration tests that preserve existing `logo.colors`.**
- [ ] **Step 2: add `ThemeDefinition`, `ThemeSource`, and `PalettePattern`.**
- [ ] **Step 3: make built-in preset identity persistent.**
- [ ] **Step 4: implement named custom palette create/select/rename/delete semantics if `custom_palettes` remains public.**
- [ ] **Step 5: remove color-equality inference and color-pair pattern inference.**
- [ ] **Step 6: commit.**

```bash
cargo test theme config tui
cargo test --all-targets --all-features --locked
git commit -am "refactor: make themes semantic and persistent"
```

### Task 18: Replace logo directory switching with `LogoRegistry`

**Files:**
- Modify: `src/ascii.rs`
- Modify: `build.rs` only if generated registry API changes
- Modify: TUI logo browser.

- [ ] **Step 1: test embedded + user union and duplicate-key user override.**
- [ ] **Step 2: implement registry/source resolution.**
- [ ] **Step 3: remove normal dependence on copied binary-adjacent logos.**
- [ ] **Step 4: make small/full selection a planner decision rather than global-width side effect.**
- [ ] **Step 5: commit.**

```bash
cargo test ascii
cargo test --all-targets --all-features --locked
git commit -am "refactor: unify embedded and custom logo sources"
```

### Task 19: Normalize CLI into canonical `Action`

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: table-test canonical subcommands and each supported legacy alias.**
- [ ] **Step 2: implement `Args::into_action()` returning one action or one explicit conflict error.**
- [ ] **Step 3: move `--scene`, `--format`, color/glyph settings to the command where semantics apply, preserving deprecated compatibility aliases at the parser boundary.**
- [ ] **Step 4: remove duplicate static-fetch dispatch code.**
- [ ] **Step 5: commit.**

```bash
cargo test cli
cargo test --all-targets --all-features --locked
git commit -am "refactor: normalize CLI dispatch"
```

### Task 20: Decouple JSON schema from internal model

**Files:**
- Modify: `src/output.rs`
- Add typed DTO structs in `src/output.rs` or `src/output/v1.rs`.

- [ ] **Step 1: snapshot the intended JSON v1 public contract.**
- [ ] **Step 2: implement `From<&SystemData> for SystemInfoV1`.**
- [ ] **Step 3: ensure internal-only fields do not appear automatically.**
- [ ] **Step 4: decide typed numeric fields deliberately rather than preserving preformatted strings where a stable typed field is clearly superior; document any schema bump required.**
- [ ] **Step 5: commit.**

```bash
cargo test output
cargo test --all-targets --all-features --locked
git commit -am "refactor: stabilize machine-readable output contract"
```

### Task 21: Redesign update behavior by installation source

**Files:**
- Modify: `src/update.rs`
- Modify: `src/cli.rs`
- Modify: README/DOCS later in documentation task.

- [ ] **Step 1: strengthen source-checkout identity test using Cargo package metadata.**
- [ ] **Step 2: make source update explicitly developer/source scoped or implement verified release update.**
- [ ] **Step 3: replace external `install` with atomic Rust-side installation where feasible.**
- [ ] **Step 4: ensure packaged/Nix users are never silently overwritten into `~/.local/bin`.**
- [ ] **Step 5: commit.**

```bash
cargo test update
cargo test --all-targets --all-features --locked
git commit -am "fix: make self-update installation-aware"
```

---

## Phase 6 — release engineering, documentation, and hardening

### Task 22: Make Nix/release builds reproducible and version-consistent

**Files:**
- Create: `flake.lock`
- Modify: `flake.nix`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `Cargo.toml`

- [ ] **Step 1: add `rust-version` to Cargo metadata based on the oldest compiler actually verified by dependencies/code.**
- [ ] **Step 2: generate and commit `flake.lock`.**
- [ ] **Step 3: add CI `nix flake check`/build where Nix is available.**
- [ ] **Step 4: add release guard that compares `GITHUB_REF_NAME` to Cargo package version.**
- [ ] **Step 5: pin release-critical GitHub Actions to reviewed commit SHAs.**
- [ ] **Step 6: add musl and planned aarch64 validation where runner/toolchain constraints allow it.**
- [ ] **Step 7: package LICENSE/README/man/completions with the binary.**
- [ ] **Step 8: commit.**

```bash
cargo build --release --locked
nix flake check
nix build

git commit -am "ci: harden reproducible release builds"
```

### Task 23: Add readable snapshots, property tests, and distro/init fixtures

**Files:**
- Modify scene tests
- Create: `tests/fixtures/...`
- Add `proptest` as dev-dependency if accepted.

- [ ] **Step 1: replace opaque scene hashes with readable expected output.**
- [ ] **Step 2: add real-logo/no-logo/large-logo/small-logo layout fixtures.**
- [ ] **Step 3: add distro/init/proc/sys fixtures listed in AF-088.**
- [ ] **Step 4: add property tests from AF-100.**
- [ ] **Step 5: run repeated tests to check determinism.**

```bash
cargo test --all-targets --all-features --locked
for i in 1 2 3; do cargo test --all-targets --all-features --locked; done
```

- [ ] **Step 6: commit.**

```bash
git add Cargo.toml Cargo.lock tests src
git commit -m "test: expand portability and layout regression coverage"
```

### Task 24: Add diagnostics, compatibility docs, provenance, and project hygiene

**Files:**
- Modify: `README.md`
- Modify: `DOCS.md`
- Modify: `Cargo.toml`
- Modify: `flake.nix`
- Create: `CHANGELOG.md`
- Create: `CONTRIBUTING.md`
- Create: `SECURITY.md`
- Create: `docs/COMPATIBILITY.md`
- Create: `docs/LOGO_SOURCES.md` or `THIRD_PARTY.md`
- Create GitHub issue templates.
- Modify CLI if `doctor` is implemented in this milestone.

- [ ] **Step 1: correct every README/DOCS claim against implemented behavior.**
- [ ] **Step 2: document non-systemd compatibility and init/service-manager distinction.**
- [ ] **Step 3: document config/JSON/CLI/composition SemVer policy.**
- [ ] **Step 4: audit logo provenance and record only verified source/license facts.**
- [ ] **Step 5: add `atlasfetch doctor` without dumping sensitive environment data.**
- [ ] **Step 6: add contribution/security/reporting templates with requested terminal dimensions, emulator, shell, composition, config version, and doctor output.**
- [ ] **Step 7: update package descriptions so AtlasFetch is standalone rather than “for atlasWM”.**
- [ ] **Step 8: commit.**

```bash
git add README.md DOCS.md Cargo.toml flake.nix CHANGELOG.md CONTRIBUTING.md SECURITY.md docs .github

git commit -m "docs: align AtlasFetch contracts and contribution guidance"
```

---

# 7. Acceptance criteria by subsystem

## System probing

- No generic collector requires systemd.
- PID 1 and service manager are separate fields.
- Artix/OpenRC, Artix/dinit, Artix/runit, Void/runit, Alpine/OpenRC fixture profiles pass.
- CPU does not label logical processors as physical cores.
- Mali-only fixture is detected without KGSL.
- Multiple batteries and multiple GPUs do not panic or silently discard data without documented selection policy.
- Optional subprocesses are time-bounded.
- Missing information is omitted/unknown, not fabricated.

## Rendering

- No render function performs system I/O or sleeps.
- `--just-ascii` and normal ASCII share the same component/backend.
- Static custom text cannot emit forbidden terminal controls.
- Every planned static line respects the chosen display width.
- Unicode truncation is column-safe.
- `NO_COLOR` works.
- Non-Nerd fallback works.

## Compositions

- Classic has tested wide/medium/narrow/no-logo paths.
- ClassicFetch preserves side-by-side -> stacked fallback.
- Dashboard has no empty logo quadrant and collapses to one column.
- Minimal is a real composition.
- Static Cockpit is removed/deprecated with documented compatibility behavior.
- All Live plans receive both width and height.

## TUI

- Multiline paste is reliable.
- Invalid palette/label/config input cannot close the editor as “saved”.
- Filtered logo selection cannot point at the wrong row.
- Hidden files can be reached.
- Logo load errors are visible.
- Preview equals actual composition/workspace planner behavior.
- Wide and narrow layouts remain usable at documented breakpoints.

## Live workspace

- Redraws never trigger extra samples.
- Child shell is killed/waited on all error/exit paths.
- Reader thread is joined/closed cleanly.
- SIGTERM/SIGHUP perform graceful cleanup where supported.
- Bracketed paste is enabled/disabled explicitly.
- Common modified navigation keys are encoded correctly.
- Scrollback is accessible.
- Terminal emulation claims match tested capability.

## Config

- Explicit v1 migrates.
- Unversioned legacy migrates.
- v2 parses/validates.
- Future versions are never moved/replaced by older binaries.
- Unknown field diagnostics are intentional.
- Theme/palette source identity is not inferred from RGB equality.

## CLI/JSON

- One canonical Action dispatch exists.
- Conflicting legacy options fail clearly instead of silently winning by branch order.
- JSON public schema is explicit and independent from internal model additions.
- Completions/manpage can be generated from the canonical CLI.

## Release/packaging

- `flake.lock` is committed.
- Tag version equals Cargo version before release publication.
- Release-critical actions are SHA-pinned.
- Release archive includes license/docs/man/completions.
- MSRV is declared and tested.
- GNU/musl and planned aarch64 coverage are explicit.

---

# 8. Verification gate before declaring consolidation complete

Run all commands from a clean checkout/worktree:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --locked
```

If Nix support remains advertised:

```bash
nix flake check
nix build
```

Run static behavior checks:

```bash
atlasfetch fetch
atlasfetch fetch --format json
atlasfetch --just-ascii
NO_COLOR=1 atlasfetch fetch
atlasfetch preset list
atlasfetch config path
```

Run piping checks from a fresh isolated config path:

```bash
rm -f /tmp/atlasfetch-review-config.json
atlasfetch --config /tmp/atlasfetch-review-config.json fetch | cat >/tmp/atlasfetch-pipe.txt
atlasfetch --config /tmp/atlasfetch-review-config.json --format json | jq . >/dev/null
```

Neither command may attempt to open the TUI.

Run representative composition sizes in a test harness/snapshot suite:

```text
52x16
70x24
80x24
90x30
120x40
160x50
```

Run Live smoke tests in at least two terminal emulators before release:

```text
shell prompt/history/completion
Ctrl+C inside child shell
Ctrl+D child exit
Ctrl+Q workspace exit
multiline paste
Ctrl/Shift modified arrows
resize repeatedly
less
vim or neovim
fzf
ssh or an equivalent PTY-interactive command
scrollback navigation
```

Run non-systemd fixture suite and at least one real non-systemd environment before calling compatibility verified.

---

# 9. Explicit non-goals for this milestone

Do not use this consolidation as justification to add:

- a plugin ABI;
- a network daemon;
- remote telemetry;
- Windows/macOS support;
- a graphical desktop application;
- a scripting language;
- arbitrary shell-command widgets in config;
- dozens of new compositions;
- a dependency on systemd DBus APIs for generic functionality.

Those can be evaluated later only if the consolidated architecture makes them natural.

---

# 10. Recommended execution order

The safest order is:

```text
Phase 0  regression tests + P0 dispatch/config fixes
   ↓
Phase 1  typed probe core + non-systemd portability
   ↓
Phase 2  pure rendering + capabilities + new composition planner
   ↓
Phase 3  MonitorSampler + Live lifecycle/input
   ↓
Phase 4  TUI behavior + visual redesign
   ↓
Phase 5  themes/logos/CLI/JSON/updater contracts
   ↓
Phase 6  release hardening + docs + property tests
```

Do not start the visual TUI rebuild before Composition/Density and layout constraints are stable. Otherwise the editor will be redesigned around APIs that are immediately removed.

Do not rewrite Live before `MonitorSampler` exists. Otherwise sampling/render coupling will be copied into the new workspace.

Do not stabilize JSON by serializing the transitional typed internal model directly. Introduce an explicit DTO.

Do not add new system collectors before `ProbeContext` and fixture infrastructure exist unless it is required to fix a P0 regression.

---

# 11. Completion definition

This plan is complete when AtlasFetch can truthfully claim all of the following:

1. Static rendering is deterministic and pure.
2. Live sampling runs on its configured cadence regardless of redraw activity.
3. The editor preview is the same composition logic used by actual output.
4. Layout decisions consider width, height, logo dimensions, data dimensions, density, and terminal capabilities.
5. Non-systemd systems are first-class and no generic path assumes systemd.
6. Config migration is version-aware and never destroys/renames future-version data on read.
7. Human output respects color/glyph capabilities and sanitizes custom static terminal controls.
8. The PTY child/session has explicit lifecycle cleanup and documented terminal compatibility.
9. JSON and CLI behavior are explicit public contracts rather than accidental consequences of internal structure.
10. Nix/release artifacts are reproducible/version-consistent enough for downstream packaging.
11. Tests produce readable failure information for layouts and use fixtures/property checks for portability edge cases.
12. README/DOCS describe behavior that is actually implemented and verified.

At that point AtlasFetch is ready to return to feature development instead of accumulating more behavior on top of unstable boundaries.
