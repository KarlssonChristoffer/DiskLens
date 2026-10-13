# DiskLens

A disk usage analyzer for Windows 11 in the spirit of
[WinDirStat](https://windirstat.net): a cushion treemap, a folder tree, file
types, largest files, duplicates and WinDirStat's cleanup tools. It has a Rust
engine and a Tauri + Svelte interface in the Windows 11 "Fluent" style.

> [!WARNING]
> **I didn't write this code, and I haven't read a single line of it.**
> All of it was written by an AI, Claude (through Claude Code), in one long
> session on 13 October 2026. I described what I wanted, looked at the
> running app and reported what was wrong. Nobody has reviewed the code.
> DiskLens can delete, move and compress files. Treat it as an experiment,
> not as tested software, and try it on files you can afford to lose first.

## How it was made

This started as a test of [REA (Reverse Engineer Anything)](https://github.com/morluto/rea),
a tool that lets an AI coding agent analyse compiled programs with Ghidra.

1. **Setup.** Claude cloned REA, built it, installed Ghidra and a Java
   runtime, and connected REA to Claude Code.
2. **Investigation.** Claude used REA on the WinDirStat 2.5.0 installed on my
   PC to find out how it scans drives, draws its treemap, colours file types
   and finds duplicates. REA decompiled and traced the shipped `WinDirStat.exe`
   and confirmed, for example:
   - the treemap's default settings stored in the program;
   - that it lists folders with the fast `NtQueryDirectoryFile` system call;
   - that it has a separate reader for the NTFS master file table;
   - that it hashes duplicates with SHA-512.

   Findings and REA evidence IDs are in [docs/research.md](docs/research.md).
3. **To be precise about REA's role:** WinDirStat is open source (GPL), so
   Claude also read its source code. Most algorithm details came from that
   source and were ported to Rust: the treemap layout, cushion shading and
   colour handling. REA's analysis mostly confirmed them in the real program
   and showed where to look. "Built with REA" is true, but this is not a
   clean-room reverse engineering of a closed program.
4. **Building.** Claude wrote the app: a Rust engine, Tauri commands and a
   Svelte UI. I chose the stack and the look and asked for features. I also
   reported bugs, such as "Move to Recycle Bin" failing on a 60 GB folder,
   and asked for WinDirStat's full feature set.

## What has been tested, and what hasn't

**Tested**

- **Automated tests:** the engine has 24 (`cargo test -p disklens-core`),
  covering:
  - scanning, exclusion filters, refreshing one folder, multi-drive scans;
  - treemap layout rules (rectangles don't overlap and fill the area);
  - colour maths, duplicate detection, saving and reloading results as CSV;
  - MD5/SHA hashes against known values;
  - NTFS compression of a temporary file.
- **By running the app:**
  - scanning a full C: drive (about 1.3 million files);
  - treemap rendering, and selection staying in sync between the tree and the treemap;
  - context menu layout.

**Not tested in the real app yet**

- **File actions:** deleting or recycling through the new Explorer-based
  route, Move to, Empty folder, compression and VHD optimisation.
- **Windows integration:** Explorer's own context menu, user-defined cleanup
  commands, and the system tools (shadow copies, user profiles, DISM,
  hibernation).
- **Dialogs and views:** Settings, search, the hash dialog, saving and loading
  results, multi-drive scans from the UI, and the light theme.

**Known gaps compared with WinDirStat**

- No fast admin-only scan that reads the NTFS master file table, so a cold
  first scan waits on the disk.
- Hard links aren't de-duplicated: a file with several hard links counts once
  per link, which makes `C:\Windows` (WinSxS) look larger than it is.
- No Pacman progress animation, and no translations.

## Features

- **Scanning:**
  - a parallel scanner using the same Windows call as WinDirStat, with its
    "size on disk" corrections;
  - pause, resume, and stop while keeping the results;
  - several drives at once, or all local drives; recent folders;
  - "Run as administrator".
- **Treemap:**
  - WinDirStat's cushion shading and layouts (KDirStat rows or squarified);
  - zoom with a breadcrumb; highlight every file of one type;
  - free and unknown space for drives;
  - Fluent and WinDirStat palettes, with adjustable lighting.
- **Lists:**
  - a folder tree with configurable columns (on disk, file size, items,
    files, folders, last change, attributes, owner);
  - file types, largest files, search results;
  - duplicates found by comparing content in stages, skipping online-only
    cloud files;
  - multi-select.
- **Clean up** (WinDirStat's menu and shortcuts):
  - open, show in Explorer, Command Prompt or PowerShell here, properties,
    copy path, compute hash;
  - delete to the Recycle Bin (Windows asks when something is too big for it)
    or permanently; empty folder; move to;
  - NTFS and WOF compression; VHD compaction;
  - Explorer's own context menu, and your own cleanup commands (Ctrl+0…9);
  - Windows maintenance: empty Recycle Bin, Disk Cleanup, Programs and
    Features, shadow copies, old user profiles, DISM, hibernation.
- **Other:**
  - save and load results as CSV, and save the duplicates list;
  - settings for scanning filters, columns, panes and treemap options.

## Measurements

All on one PC, with one NVMe C: drive (1.31 million files and 258,000 folders):

| What | Time |
|---|---|
| Full scan, disk cache warm (release build) | 4.4–6.7 s |
| Full scan, first run after boot | 84 s |
| Rendering a 1920×1080 treemap | ~40 ms |

These are single measurements, not benchmarks against WinDirStat.

## Building

You need Windows 10/11, Rust (MSVC toolchain) with the Visual Studio Build
Tools, and Node.js. The translucent Mica background needs Windows 11.

```bash
npm install
npm run tauri dev      # development build with hot reload
npm run tauri build    # target/release/disklens.exe and an NSIS installer
```

Engine only:

```bash
cargo test -p disklens-core
cargo run --release -p disklens-core --example scan -- C:\ treemap.png
```

| Path | Contents |
|---|---|
| `crates/core` | Engine without UI: scanner, tree, treemap, file types, search, duplicates, CSV, file operations |
| `src-tauri` | Tauri commands, settings, Windows shell integration |
| `src` | Svelte 5 UI |
| `docs/research.md` | The REA investigation of WinDirStat |

## Credits and license

- **WinDirStat** (© WinDirStat Team). DiskLens follows WinDirStat's design.
  `crates/core/src/treemap.rs` and `crates/core/src/palette.rs` are ports of
  WinDirStat 2.5.0 code, which is licensed under GPL-2.0-or-later.
- **Treemap algorithms:**
  - cushion treemaps: J. J. van Wijk and H. van de Wetering (1999);
  - squarified treemaps: M. Bruls, K. Huizing and J. J. van Wijk (2000);
  - row layout: KDirStat.
- **Analysis tools** (used, not included):
  [REA](https://github.com/morluto/rea) (MIT) and
  [Ghidra](https://github.com/NationalSecurityAgency/ghidra) (Apache 2.0).
- **Libraries:**
  - [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) (MIT);
  - Tauri, Svelte, rayon, BLAKE3, regex and windows-rs, under their own licenses.
- **Code** written by Claude (Anthropic) using Claude Code.

DiskLens is released under the **GNU General Public License v3.0 or later**
(see [LICENSE](LICENSE)). It comes with no warranty. It's a personal
experiment and has nothing to do with the WinDirStat project.
