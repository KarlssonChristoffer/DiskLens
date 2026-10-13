# DiskLens

A fast, modern disk usage analyzer for Windows 11, inspired by WinDirStat.
It has a Rust scanning and treemap engine and a Fluent-styled UI (Tauri + Svelte).

- **Treemap** with WinDirStat's cushion shading: hover for details, click to select,
  double-click to zoom into a folder.
- **Folder tree** sorted by size, name, file count or date. It only draws visible
  rows, so folders with 100,000 entries stay smooth.
- **File types** with their share of the space and treemap colour.
- **Largest files** in the current treemap view.
- **Duplicates:** content comparison in widening stages (4 KiB, 1 MiB, then the whole
  file). Online-only cloud files are skipped so nothing gets downloaded.
- **Cleanup:** open, show in Explorer, copy path, properties, move to Recycle Bin
  (never permanent delete).
- **Size on disk or file size**, two palettes (Fluent, WinDirStat classic) and
  adjustable cushion lighting.

See [docs/research.md](docs/research.md) for how WinDirStat's algorithms were
studied with REA and how DiskLens differs.

## Run

Requires Rust (MSVC toolchain), Node.js and the VS Build Tools.

```bash
npm install
npm run tauri dev      # development, hot reload
npm run tauri build    # release: target/release/disklens.exe + NSIS installer
```

## Layout

| Path | What |
|---|---|
| `crates/core` | Engine, no UI: `scan` (NtQueryDirectoryFile, rayon), `tree` (arena), `treemap` (layout and cushion render), `ext`, `top`, `dupes`, `palette`, `volumes` |
| `src-tauri` | Tauri commands (`commands.rs`), scan session state, shell integrations |
| `src` | Svelte 5 UI: `lib/components`, Fluent tokens in `lib/theme.css`, typed API in `lib/api.ts` |

## Engine checks

```bash
cargo test -p disklens-core
cargo run --release -p disklens-core --example scan -- C:\ treemap.png
```

The example prints scan timing, top file types and largest files, and writes a
1920×1080 treemap PNG.

## Known limitations

- Hard-linked files are counted once per link (WinDirStat can deduplicate them).
- No NTFS master-file-table "turbo" scan yet; a cold first scan is disk-bound.
- The Mica backdrop needs Windows 11.

## License

The treemap layout, cushion shading and palette handling in
`crates/core/src/treemap.rs` and `crates/core/src/palette.rs` are ports of
WinDirStat's GPLv3 code (Controls/TreeMap.cpp, TreeMap.h). Personal use carries
no obligations. If DiskLens is ever distributed, it must be released under the
GPL v3 (or those parts re-implemented independently).
