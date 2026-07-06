# Clam

A tiny Windows system-tray utility that toggles whether your laptop stays awake
with the lid shut — so it can run remote-desktop sessions, render jobs, or
downloads while closed and put away.

**Left-click** the tray icon to enable stay-awake mode.
**Left-click again** to restore your original settings.

## Screenshot

![Clam tray icon and menu](docs/screenshot.png)

## What it does

| State | Lid-close action | Idle-sleep timeout |
|-------|------------------|--------------------|
| **Stay-awake ON** | Do nothing (AC & DC) | Never (AC & DC) |
| **Normal / OFF** | Your exact previous settings, restored from saved state |

When you activate stay-awake mode, Clam captures your current power settings
to a file on disk, then writes the stay-awake values. When you toggle off (or
quit), it restores the exact originals.

## Tray icon

- **Solid red circle** — stay-awake is active.
- **Green ring (hollow)** — normal sleep behavior.

The icons differ in both color *and* shape (filled vs. outline) so the state is
unambiguous even for red–green colorblind users. Hover for a tooltip, and a
Windows notification pops on every toggle.

## Right-click menu

- **Status line** — shows current state (disabled/informational).
- **Reset to normal defaults** — writes lid-close = Sleep, idle timeout =
  30 min (AC) / 15 min (DC). Does *not* wipe other power-scheme customizations.
- **Start with Windows** — toggles an autostart entry in `HKCU\...\Run` (no
  admin required).
- **Quit** — restores original settings before exiting.

## Build

Requires the Rust toolchain and MSVC Build Tools (for `link.exe`).

```
cargo build --release
```

The output is a single `.exe` at `target\release\clam.exe`. No installer
needed — just copy it wherever you like.

No console window appears on launch (the binary uses
`#![windows_subsystem = "windows"]`).

## Saved-state file

Location: `%LOCALAPPDATA%\Clam\saved_state.json`

Written the moment stay-awake is enabled. On startup, if this file exists (i.e.
the app crashed or was killed while active), Clam auto-reverts to the
captured originals and starts in normal/off state.

## Single-instance

Only one copy runs at a time. A named mutex (`Global\Clam_SingleInstance`)
prevents duplicates — a second launch exits silently.

## Elevation

The power-policy writes target the active scheme at the current-user level,
which normally works without elevation (same as the Windows Settings UI). If
your system returns "Access denied," run the `.exe` as administrator.

## Caveats

- With lid-close set to "Do nothing," the internal display turns off when the
  lid shuts but the machine keeps running. This is the intended behavior for
  remote / render use.

- On laptops using **Modern Standby (S0)** instead of legacy S3, or with
  certain OEM firmware, lid-close behavior can still differ. If it sleeps
  anyway, that's a firmware / Modern-Standby quirk, not a bug in this tool.

- If you manually change power settings via Windows while stay-awake is active,
  toggling off will overwrite those manual changes with the originally captured
  values. This is a known, intended tradeoff.

## License

MIT
