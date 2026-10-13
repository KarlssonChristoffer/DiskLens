// Typed wrappers around the Rust commands in src-tauri/src/{commands,actions}.rs.
import { Channel, invoke } from "@tauri-apps/api/core";

export type Rgb = [number, number, number];

export interface Volume {
  root: string;
  label: string;
  fileSystem: string;
  kind: "fixed" | "removable" | "network" | "optical" | "ram";
  totalBytes: number;
  freeBytes: number;
}

export interface Summary {
  rootPath: string;
  files: number;
  dirs: number;
  logical: number;
  physical: number;
  errors: number;
  elapsedMs: number;
  volumeTotal: number | null;
  volumeFree: number | null;
  incomplete: boolean;
  fromFile: boolean;
  myComputer: boolean;
  hasDrives: boolean;
}

export type NodeKind = "dir" | "file" | "free" | "unknown";

export interface NodeInfo {
  id: number;
  name: string;
  kind: NodeKind;
  isDir: boolean;
  size: number;
  logical: number;
  physical: number;
  files: number;
  dirs: number;
  modified: number;
  share: number;
  hasChildren: boolean;
  ext: string;
  color: Rgb | null;
  flags: number;
  attributes: number;
  attributeText: string;
}

export const NodeFlags = {
  deleted: 1,
  inaccessible: 2,
  reparseSkipped: 4,
  nameLossy: 8,
  incomplete: 16,
  hidden: 32,
  myComputer: 64,
  drive: 128,
} as const;

export interface NodeDetail {
  info: NodeInfo;
  path: string;
  ancestry: number[];
  alive: boolean;
}

export interface FileEntry {
  info: NodeInfo;
  path: string;
}

export interface ExtStat {
  id: number;
  ext: string;
  bytes: number;
  files: number;
  color: Rgb;
}

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export type SortKey = "size" | "name" | "files" | "folders" | "items" | "modified" | "logical" | "physical" | "attributes";

export interface TreemapOptions {
  style: "KDirStat" | "Squarified";
  grid: boolean;
  gridColor: Rgb;
  brightness: number;
  height: number;
  scaleFactor: number;
  ambientLight: number;
  lightX: number;
  lightY: number;
}

export interface ViewOptions {
  sizeMode: "physical" | "logical";
  palette: "fluent" | "classic";
  treemap: TreemapOptions;
}

export interface ScanOptions {
  excludeHiddenDirs: boolean;
  excludeHiddenFiles: boolean;
  excludeProtectedDirs: boolean;
  excludeProtectedFiles: boolean;
  followJunctions: boolean;
  followSymlinkDirs: boolean;
  countSymlinkFiles: boolean;
  excludeDirs: string[];
  excludeFiles: string[];
  useRegex: boolean;
  minFileSize: number;
  threads: number;
}

export interface SearchOptions {
  caseSensitive: boolean;
  wholeName: boolean;
  regex: boolean;
  maxResults: number;
}

export interface UserCleanup {
  title: string;
  command: string;
  enabled: boolean;
  worksForDrives: boolean;
  worksForFolders: boolean;
  worksForFiles: boolean;
  worksForUnc: boolean;
  recurse: boolean;
  askFirst: boolean;
  showConsole: boolean;
  wait: boolean;
  refresh: "none" | "item" | "parent";
}

export type Column = "subtree" | "physical" | "logical" | "items" | "files" | "folders" | "modified" | "attributes" | "owner";

export interface Settings {
  view: ViewOptions;
  scan: ScanOptions;
  search: SearchOptions;
  showFreeSpace: boolean;
  showUnknown: boolean;
  showFileTypes: boolean;
  showTreemap: boolean;
  showToolbar: boolean;
  showStatusBar: boolean;
  deleteWarning: boolean;
  listStripes: boolean;
  largeFileCount: number;
  folderHistory: string[];
  columns: Column[];
  userCleanups: UserCleanup[];
}

export interface AppInfo {
  elevated: boolean;
  version: string;
  accent: string | null;
  hibernationFile: boolean;
}

export type ScanEvent =
  | { type: "progress"; files: number; dirs: number; bytes: number; errors: number; elapsedMs: number; paused: boolean }
  | { type: "done"; summary: Summary }
  | { type: "cancelled" }
  | { type: "failed"; message: string };

export type DupeEvent =
  | { type: "progress"; candidates: number; filesHashed: number; bytesHashed: number; elapsedMs: number }
  | { type: "done"; groups: number; wasted: number; elapsedMs: number }
  | { type: "cancelled" };

export interface DupeGroup {
  size: number;
  wasted: number;
  hash: string;
  files: FileEntry[];
}

export interface Outcome {
  removed: number[];
  remaining: string[];
  freed: number;
  summary: Summary;
}

export interface BatchOutcome {
  done: number;
  failed: string[];
  summary: Summary;
}

export interface Hashes {
  md5: string;
  sha1: string;
  sha256: string;
  sha384: string;
  sha512: string;
}

export type Compression = "none" | "lznt1" | "xpress4k" | "xpress8k" | "xpress16k" | "lzx";

export type SystemAction =
  | "emptyRecycleBin"
  | "diskCleanup"
  | "programs"
  | "disableHibernation"
  | "removeShadowCopies"
  | "removeRoamingProfiles"
  | "removeLocalProfiles"
  | "dismAnalyze"
  | "dismCleanup"
  | "dismReset"
  | "runElevated";

export const api = {
  listVolumes: () => invoke<Volume[]>("list_volumes"),
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),

  startScan(paths: string[], onEvent: (e: ScanEvent) => void) {
    const events = new Channel<ScanEvent>();
    events.onmessage = onEvent;
    return invoke<void>("start_scan", { paths, events });
  },
  cancelScan: () => invoke<void>("cancel_scan"),
  stopScan: () => invoke<void>("stop_scan"),
  pauseScan: () => invoke<void>("pause_scan"),
  resumeScan: () => invoke<void>("resume_scan"),
  getSummary: () => invoke<Summary>("get_summary"),
  refreshItems: (ids: number[]) => invoke<Summary>("refresh_items", { ids }),

  listChildren: (id: number, sort: SortKey, descending: boolean, offset: number, limit: number) =>
    invoke<{ total: number; items: NodeInfo[] }>("list_children", { id, sort, descending, offset, limit }),
  nodeDetail: (id: number) => invoke<NodeDetail>("node_detail", { id }),
  owners: (ids: number[]) => invoke<(string | null)[]>("owners", { ids }),
  extStats: () => invoke<ExtStat[]>("ext_stats"),
  largestFiles: (from: number, count: number) => invoke<FileEntry[]>("largest_files", { from, count }),
  search: (query: string, from: number, options: SearchOptions) => invoke<FileEntry[]>("search_items", { query, from, options }),

  async renderTreemap(root: number, width: number, height: number, highlight: number | null): Promise<Uint8ClampedArray> {
    const buffer = await invoke<ArrayBuffer>("render_treemap", { root, width, height, highlight });
    return new Uint8ClampedArray(buffer);
  },
  treemapHit: (x: number, y: number) => invoke<number | null>("treemap_hit", { x, y }),
  treemapRect: (id: number) => invoke<Rect | null>("treemap_rect", { id }),

  revealItem: (id: number) => invoke<void>("reveal_item", { id }),
  openItem: (id: number) => invoke<void>("open_item", { id }),
  showProperties: (id: number) => invoke<void>("show_properties", { id }),
  openConsole: (id: number, powershell: boolean) => invoke<void>("open_console", { id, powershell }),
  explorerMenu: (ids: number[], x: number, y: number) => invoke<boolean>("explorer_menu", { ids, x, y }),
  pruneMissing: (ids: number[]) => invoke<Summary>("prune_missing", { ids }),
  deleteItems: (ids: number[], permanent: boolean) => invoke<Outcome>("delete_items", { ids, permanent }),
  emptyFolders: (ids: number[]) => invoke<Outcome>("empty_folders", { ids }),
  moveItems: (ids: number[], destination: string) => invoke<Outcome>("move_items", { ids, destination }),
  compressItems: (ids: number[], compression: Compression) => invoke<BatchOutcome>("compress_items", { ids, compression }),
  optimizeVhds: (ids: number[]) => invoke<BatchOutcome>("optimize_vhds", { ids }),
  computeHashes: (id: number) => invoke<Hashes>("compute_hashes", { id }),
  previewUserCleanup: (index: number, ids: number[]) => invoke<{ commands: string[] }>("preview_user_cleanup", { index, ids }),
  runUserCleanup: (index: number, ids: number[]) => invoke<Summary>("run_user_cleanup", { index, ids }),
  systemAction: (action: SystemAction) => invoke<Summary | null>("system_action", { action }),

  saveResults: (path: string) => invoke<void>("save_results", { path }),
  saveDuplicates: (path: string) => invoke<void>("save_duplicates", { path }),
  loadResults: (path: string) => invoke<Summary>("load_results", { path }),

  findDuplicates(minSize: number, onEvent: (e: DupeEvent) => void) {
    const events = new Channel<DupeEvent>();
    events.onmessage = onEvent;
    return invoke<void>("find_duplicates", { minSize, events });
  },
  cancelDuplicates: () => invoke<void>("cancel_duplicates"),
  getDuplicates: () => invoke<DupeGroup[] | null>("get_duplicates"),
};
