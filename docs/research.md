# How WinDirStat works: REA investigation notes

DiskLens re-implements WinDirStat's core ideas in new code. Before writing it,
the shipped `WinDirStat.exe` was analysed with
[REA](https://github.com/morluto/rea) 6.4.0 + Ghidra 12.1.4, and every finding
was cross-checked against WinDirStat's GPLv3 source at the matching tag.

- **Target:** `C:\Program Files\WinDirStat\WinDirStat.exe`, version 2.5.0, x86-64 PE,
  SHA-256 `cc2bcb73aa81c91f204343dc8698d22fa441e8dc0a4ae6b6492fd5b032d1ab45`
- **Source:** github.com/windirstat/windirstat, tag `release/v2.5.0`
- **Method:** static analysis only; WinDirStat was never run. Evidence IDs below
  are REA Evidence records from those runs.

Conventions: **Observed** means seen directly in the binary. **Source** means read in
the 2.5.0 source. **Inferred** means a conclusion that connects the two.

## Finding ledger

| # | Question | Finding | Evidence | Status |
|---|---|---|---|---|
| 1 | How are directories listed? | `NtQueryDirectoryFile` is resolved at startup with `GetProcAddress(GetModuleHandleW("ntdll.dll"), ...)` and stored in a global (function `0x140001090`). The source's basic finder calls it with `FileIdFullDirectoryInformation` (class 38), falling back to class 2, using a 4 MiB thread-local buffer. One call returns names, attributes, end-of-file **and** allocation size, with no per-file metadata call. | Observed: `ev_10e8dceb…704d` (trace), `ev_110d79ff…bcb` (decompile). Source: `FinderBasic.cpp` | Answered |
| 2 | Is there a faster scan mode? | Yes. A 6 KB function (`0x14005cd00`) references the strings `\\.\` and `\$MFT::$DATA`: an NTFS master-file-table reader (`FinderNtfs.cpp`), which needs admin rights. | Observed: `ev_ca28728744b1…dad33`, `ev_bc7505fc…99ac` | Answered (not yet ported) |
| 3 | How is "size on disk" corrected? | When an entry reports allocation 0 but is compressed, sparse or larger than a cluster, `GetCompressedFileSizeW` supplies the real on-disk size; WOF-compressed files get the compressed flag. | Observed: `GetCompressedFileSizeW` import, `ev_e98776dc…d7`. Source: `FinderBasic.cpp` | Answered |
| 4 | What are the treemap defaults? | Settings stored as percentages: brightness **88**, height **38**, scale factor **91**, ambient light **13**, light source X **−100** (range −200…200). | Observed: `ev_45b8f2a0…106e`, `ev_08a682e9…17b3`, `ev_43ee9c2b…fa57`, `ev_8d0c3c96…ab5c`, `ev_8c736bc5…1564` | Answered |
| 5 | How is a cushion shaded? | Each nested rectangle adds a parabolic ridge to a surface `z = s0·x² + s1·y² + s2·x + s3·y`. Per pixel, the normal `(-(2·s0·x + s2), -(2·s1·y + s3), 1)` is compared with a light vector `(lx, ly, 10)` normalised. Brightness is `Ia + (1−Ia)·max(cos, 0)`, scaled by `brightness / 0.6`. | Source: `TreeMap.cpp` (`DrawCushion`, `AddRidge`). Constants match finding 4 | Answered |
| 6 | How are rectangles laid out? | Default "KDirStat" rows: a row grows until the next child's aspect ratio would drop below 0.4. Optional SequoiaView squarification. Children are sorted by size, descending. | Source: `TreeMap.cpp` | Answered |
| 7 | How do file types get colours? | Extensions are sorted by bytes; the 18 palette colours go to the top 18 in order and every other extension gets the last (white) entry. All palette colours are first equalised to brightness 0.6 (`MakeBrightColor`), with channel overflow spread to the other channels. | Source: `DirStatDoc.cpp`, `TreeMap.h` | Answered |
| 8 | How are duplicates found? | Group by size, then hash in widening stages (4 KiB → 1 MiB → whole file) with CNG `SHA512` truncated to 16 bytes. Cloud placeholders are skipped. | Observed: BCrypt imports `ev_e98776dc…d7`; `SHA512` string referenced in `0x14001c1d0` and `0x140076a20`, `ev_099e703f…5d67`. Source: `Item.cpp` `GetFileHash`, `FileDupeControl.cpp` | Answered |
| 9 | Hard links? | A `ProcessHardlinks` setting exists (string observed); implementation not studied. | Observed: `ev_bc7505fc…99ac` | Open |

Full Evidence IDs:

- `ev_12b43132962fc69affa84c35e6337cc7f36a4f3ae21670a3f01ec86f8b146219` — "Treemap" string inventory
- `ev_91cec6db0188b8542749dfdff557873758312e1c4e85ef506a2f498b0f3bef9b` — `TreeMapBrightness` trace
- `ev_45b8f2a06b430b4213cdbd7f7ea825878c78b8a5d0b70d9595df917f4bfd106e` — brightness setting initialiser
- `ev_08a682e9198f789778bca5e40f358e7256e7c10d2289fd3afac5445561e817b3` — height setting initialiser
- `ev_43ee9c2bbbae56341ed7ac7e8e7495191f681b501c85247c45bbfc07efc3fa57` — scale-factor setting initialiser
- `ev_8d0c3c96fe94c6de99a64306fbd8cd65cbfc76bca18322c28eb6665de92aab5c` — ambient-light setting initialiser
- `ev_8c736bc58ddcd251c21086e3eb6b57986d78bba4b9e34170d613eeb89b671564` — light-source-X setting initialiser
- `ev_e98776dc929045c678f1edd798f19009cffac54f7b9ceebc33667012bee383d7` — import search (FindFirstFileExW, DeviceIoControl, GetCompressedFileSizeW, BCrypt*)
- `ev_bc7505fccfd762a21456e6651d69730156ac21f25dd555497c828bc857e799ac` — strings: `\$MFT::$DATA`, `SHA512`, `$RECYCLE.BIN`, `ProcessHardlinks`
- `ev_099e703fc0239d3d060a65a925ee32e4ca4f1e8f5967f00812a2497f82275d67` — `SHA512` trace
- `ev_ca28728744b15b8b7cf84b07beeb5cbed790fdabfefc53df0178a0e1cf4dad33` — `$MFT` trace
- `ev_10e8dceba82d59287dd7bc4298b3772ae45e32b495898e75c028fdf1155f704d` — `NtQueryDirectoryFile` trace
- `ev_110d79ff3d548703586170ee69d8418715cfe0b5cae68cd54de5b5d383861bcb` — dynamic-resolution decompile

## What DiskLens took, and where it differs

| WinDirStat | DiskLens |
|---|---|
| `NtQueryDirectoryFile` batches, physical-size correction | Same approach (`crates/core/src/scan`), 1 MiB buffer, rayon work-stealing across directories |
| KDirStat/SequoiaView layout and cushion shading | Faithful port (`crates/core/src/treemap.rs`); shading runs in parallel bands |
| Shrinks the treemap by 1 px for a border | Uses the full canvas (the UI draws the card border) |
| 18-colour palette | Same palette ("WinDirStat classic"), plus a Fluent palette pre-tuned to brightness 0.6 so equalisation keeps its hues |
| SHA-512 (truncated), during scan | BLAKE3, on demand after the scan, same three stages |
| MFT reader for admin scans | Not yet; candidate "turbo" mode |
| Hard-link handling | Not yet: hard-linked files (e.g. `C:\Windows\WinSxS`) count once per link |

## Measurements

On this machine (C: drive, 1.31 M files, 258 k folders, NTFS):

- **Warm cache:** 4.4–6.7 s for a full scan (release build). The first scan after boot is
  disk-bound: 84 s cold.
- **Treemap:** 1920×1080 rendered in about 40 ms (release, 12 threads).
- **File types and largest files:** under 10 ms each.
