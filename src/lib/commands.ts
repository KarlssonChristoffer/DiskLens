// Every WinDirStat command, defined once and shared by the menu bar, the
// toolbar, context menus and keyboard shortcuts (accelerators follow
// WinDirStat's windirstat.rc).
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { Compression } from "./api";
import type { IconName } from "./icons";
import { app } from "./state.svelte";

export interface Command {
  id: string;
  label: string;
  icon?: IconName;
  shortcut?: string;
  enabled?: () => boolean;
  checked?: () => boolean;
  danger?: boolean;
  run: (ctx?: { x: number; y: number }) => unknown;
}

export interface Submenu {
  label: string;
  icon?: IconName;
  items: () => MenuEntry[];
}

export type MenuEntry = string | "-" | Submenu;

const hasScan = () => app.phase === "results" && app.summary !== null;
const live = () => hasScan() && !app.summary!.fromFile;
const hasItems = () => live() && app.actionable.length > 0;
const one = () => live() && app.focus !== null && app.actionable.includes(app.focus);
const singleFile = () => live() && app.selectionInfo.length === 1 && app.selectionInfo[0].kind === "file";
const folders = () => live() && app.selectionInfo.some((n) => n.kind === "dir" && n.id !== 0);
const removable = () => hasItems() && app.actionable.some((id) => id !== 0);
const scanning = () => app.phase === "scanning";
const setting = (key: "showFreeSpace" | "showUnknown" | "showFileTypes" | "showTreemap" | "showToolbar" | "showStatusBar") => ({
  checked: () => app.settings?.[key] ?? false,
  run: () => app.settings && app.updateSettings({ [key]: !app.settings[key] }),
});

const compress = (id: Compression, label: string): Command => ({
  id: `compress.${id}`,
  label,
  enabled: hasItems,
  run: () => app.compress(id),
});

const list: Command[] = [
  // File
  { id: "file.select", label: "Select drives or folder…", icon: "hardDrive", shortcut: "Ctrl+O", run: () => app.goHome() },
  { id: "file.load", label: "Load results…", icon: "documentArrowUp", shortcut: "Ctrl+Alt+L", run: () => app.loadResults() },
  { id: "file.save", label: "Save results…", icon: "save", shortcut: "Ctrl+Alt+S", enabled: hasScan, run: () => app.saveResults() },
  { id: "file.saveDuplicates", label: "Save duplicates…", enabled: hasScan, run: () => app.saveDuplicates() },
  { id: "file.refreshAll", label: "Refresh all", icon: "arrowSync", shortcut: "Shift+F5", enabled: live, run: () => app.rescan() },
  { id: "file.refreshSelected", label: "Refresh selected", icon: "arrowClockwise", shortcut: "F5", enabled: live, run: () => app.refreshSelected() },
  {
    id: "file.elevate",
    label: "Run as administrator",
    icon: "shieldKeyhole",
    shortcut: "Ctrl+Alt+E",
    enabled: () => !app.info?.elevated,
    run: () => app.system("runElevated"),
  },
  { id: "scan.resume", label: "Resume scan", icon: "play", shortcut: "Ctrl+Alt+R", enabled: () => scanning() && !!app.progress?.paused, run: () => app.resumeScan() },
  { id: "scan.pause", label: "Pause scan", icon: "pause", shortcut: "Ctrl+Alt+Space", enabled: () => scanning() && !app.progress?.paused, run: () => app.pauseScan() },
  { id: "scan.stop", label: "Stop scan (keep results)", icon: "stop", shortcut: "Ctrl+Alt+T", enabled: scanning, run: () => app.stopScan() },

  {
    id: "tree.toggle",
    label: "Expand / collapse",
    enabled: () => hasScan() && app.selectionInfo.length === 1 && app.selectionInfo[0].isDir && app.selectionInfo[0].hasChildren,
    run: () => app.focus !== null && (app.toggleRequest = { id: app.focus }),
  },

  // Edit
  { id: "edit.copy", label: "Copy path", icon: "copy", shortcut: "Ctrl+C", enabled: () => hasScan() && app.selection.length > 0, run: () => app.copyPath() },
  { id: "edit.search", label: "Search…", icon: "search", shortcut: "Ctrl+F", enabled: hasScan, run: () => (app.dialog = { kind: "search" }) },
  { id: "edit.hash", label: "Compute hash", icon: "fingerprint", shortcut: "Ctrl+H", enabled: singleFile, run: () => app.computeHash() },

  // Clean Up
  { id: "cleanup.open", label: "Open", icon: "open", shortcut: "Enter", enabled: hasItems, run: () => app.openSelected() },
  { id: "cleanup.reveal", label: "Select in Explorer", icon: "folderOpen", shortcut: "Ctrl+E", enabled: one, run: () => app.revealSelected() },
  { id: "cleanup.console", label: "Open in Command Prompt", icon: "windowConsole", shortcut: "Ctrl+P", enabled: one, run: () => app.consoleSelected(false) },
  { id: "cleanup.pwsh", label: "Open in PowerShell", icon: "code", shortcut: "Ctrl+S", enabled: one, run: () => app.consoleSelected(true) },
  { id: "cleanup.properties", label: "Properties", icon: "info", shortcut: "Alt+Enter", enabled: one, run: () => app.propertiesSelected() },
  { id: "cleanup.deletePermanent", label: "Delete permanently", icon: "deleteDismiss", shortcut: "Shift+Delete", danger: true, enabled: removable, run: () => app.deleteSelected(true) },
  { id: "cleanup.delete", label: "Delete (to Recycle Bin)", icon: "delete", shortcut: "Delete", enabled: removable, run: () => app.deleteSelected(false) },
  { id: "cleanup.emptyBin", label: "Empty Recycle Bin", run: () => app.system("emptyRecycleBin") },
  { id: "cleanup.emptyFolder", label: "Empty folder", danger: true, enabled: folders, run: () => app.emptyFolder() },
  { id: "cleanup.move", label: "Move to…", icon: "folderArrowRight", shortcut: "Ctrl+M", enabled: removable, run: () => app.moveTo() },
  compress("none", "Uncompress (none)"),
  compress("lznt1", "LZNT1 (NTFS, compatible)"),
  compress("xpress4k", "XPRESS 4K (fast)"),
  compress("xpress8k", "XPRESS 8K"),
  compress("xpress16k", "XPRESS 16K"),
  compress("lzx", "LZX (smallest)"),
  {
    id: "cleanup.vhd",
    label: "Optimize VHD files",
    enabled: () => hasItems() && app.selectionInfo.some((n) => n.kind === "dir" || /\.vhdx?$/i.test(n.name)),
    run: () => app.optimizeVhd(),
  },
  { id: "cleanup.hibernate", label: "Disable hibernation", enabled: () => !!app.info?.hibernationFile, run: () => app.system("disableHibernation") },
  { id: "cleanup.diskCleanup", label: "Disk Cleanup", run: () => app.system("diskCleanup") },
  { id: "cleanup.programs", label: "Programs and Features", run: () => app.system("programs") },
  { id: "cleanup.roaming", label: "Remove roaming profiles…", run: () => app.system("removeRoamingProfiles") },
  { id: "cleanup.local", label: "Remove unused local profiles…", run: () => app.system("removeLocalProfiles") },
  { id: "cleanup.shadow", label: "Remove shadow copies…", run: () => app.system("removeShadowCopies") },
  { id: "dism.analyze", label: "/AnalyzeComponentStore", run: () => app.system("dismAnalyze") },
  { id: "dism.cleanup", label: "/StartComponentCleanup", run: () => app.system("dismCleanup") },
  { id: "dism.reset", label: "/StartComponentCleanup /ResetBase", run: () => app.system("dismReset") },
  {
    id: "cleanup.explorer",
    label: "Windows Explorer menu",
    icon: "folderOpen",
    enabled: hasItems,
    run: (ctx) => app.explorerMenu(ctx?.x ?? window.innerWidth / 2, ctx?.y ?? window.innerHeight / 2),
  },

  // Treemap
  { id: "view.treemap", label: "Show treemap", icon: "dataTreemap", shortcut: "F9", ...setting("showTreemap") },
  {
    id: "treemap.logical",
    label: "Show logical size",
    shortcut: "Ctrl+L",
    checked: () => app.settings?.view.sizeMode === "logical",
    run: () => app.settings && app.updateSettings({ view: { ...app.settings.view, sizeMode: app.settings.view.sizeMode === "logical" ? "physical" : "logical" } }),
  },
  { id: "treemap.zoomIn", label: "Zoom in", icon: "zoomIn", shortcut: "+", enabled: () => hasScan() && app.focus !== null, run: () => app.zoomIn() },
  { id: "treemap.zoomOut", label: "Zoom out", icon: "zoomOut", shortcut: "-", enabled: () => hasScan() && app.zoomRoot !== 0, run: () => app.zoomOut() },
  { id: "treemap.reselectChild", label: "Reselect child", shortcut: "*", enabled: () => hasScan() && app.canReselectChild, run: () => app.reselectChild() },
  { id: "treemap.selectParent", label: "Select parent", shortcut: "/", enabled: () => hasScan() && app.focus !== null && app.focus !== 0, run: () => app.selectParent() },

  // Options
  { id: "view.freeSpace", label: "Show free space", shortcut: "F6", ...setting("showFreeSpace") },
  { id: "view.unknown", label: "Show unknown", shortcut: "F7", ...setting("showUnknown") },
  { id: "view.fileTypes", label: "Show file types", shortcut: "F8", ...setting("showFileTypes") },
  { id: "view.toolbar", label: "Show toolbar", shortcut: "F10", ...setting("showToolbar") },
  { id: "view.statusBar", label: "Show status bar", shortcut: "F11", ...setting("showStatusBar") },
  { id: "view.settings", label: "Settings…", icon: "settings", shortcut: "Ctrl+I", run: () => (app.dialog = { kind: "settings" }) },
  { id: "view.allFiles", label: "All files", shortcut: "Ctrl+F1", enabled: hasScan, run: () => (app.tab = "folders") },
  { id: "view.largest", label: "Largest files", shortcut: "Ctrl+F2", enabled: hasScan, run: () => (app.tab = "largest") },
  { id: "view.duplicates", label: "Duplicate files", shortcut: "Ctrl+F3", enabled: hasScan, run: () => (app.tab = "duplicates") },
  { id: "view.searchResults", label: "Search results", shortcut: "Ctrl+F4", enabled: () => hasScan() && app.searchResults !== null, run: () => (app.tab = "search") },

  // Help
  { id: "help.about", label: "About DiskLens", icon: "questionCircle", shortcut: "F1", run: () => (app.dialog = { kind: "about" }) },

  // Start page helpers
  {
    id: "file.scanFolder",
    label: "Scan a folder…",
    icon: "folderOpen",
    run: async () => {
      const path = await openDialog({ directory: true, title: "Choose a folder to scan" });
      if (typeof path === "string") app.scan([path]);
    },
  },
];

export const commands = new Map(list.map((c) => [c.id, c]));

export function isEnabled(c: Command) {
  return c.enabled ? c.enabled() : true;
}

export function runCommand(id: string, ctx?: { x: number; y: number }) {
  const c = commands.get(id);
  if (c && isEnabled(c)) c.run(ctx);
}

const userCleanups = (): MenuEntry[] => {
  const list = app.settings?.userCleanups ?? [];
  if (list.length === 0) return ["cleanup.configure"];
  return list.map((_, i) => `user.${i}`);
};

// User-defined cleanups are generated from settings (Ctrl+0..9).
export function userCleanupCommand(i: number): Command {
  const c = app.settings?.userCleanups[i];
  return {
    id: `user.${i}`,
    label: c?.title || `Cleanup ${i + 1}`,
    shortcut: i < 10 ? `Ctrl+${i}` : undefined,
    enabled: () => hasItems() && !!c?.enabled,
    run: () => app.userCleanup(i),
  };
}

commands.set("cleanup.configure", {
  id: "cleanup.configure",
  label: "Configure cleanups…",
  icon: "wrench",
  run: () => (app.dialog = { kind: "settings", page: "cleanups" }),
});

export function resolve(id: string): Command | undefined {
  if (id.startsWith("user.")) return userCleanupCommand(Number(id.slice(5)));
  return commands.get(id);
}

const compressMenu: Submenu = {
  label: "Compress",
  icon: "archive",
  items: () => ["compress.none", "-", "compress.lznt1", "compress.xpress4k", "compress.xpress8k", "compress.xpress16k", "compress.lzx"],
};
const dismMenu: Submenu = { label: "DISM component store", items: () => ["dism.analyze", "dism.cleanup", "dism.reset"] };
const userMenu: Submenu = { label: "User-defined cleanup", icon: "wrench", items: userCleanups };

export const menuBar: { label: string; items: () => MenuEntry[] }[] = [
  {
    label: "File",
    items: () => [
      "file.select", "-", "file.load", "file.save", "file.saveDuplicates", "-",
      "file.refreshAll", "file.refreshSelected", "-", "scan.resume", "scan.pause", "scan.stop", "-", "file.elevate",
    ],
  },
  { label: "Edit", items: () => ["edit.copy", "-", "edit.search", "-", "edit.hash"] },
  {
    label: "Clean Up",
    items: () => [
      "cleanup.open", "-", "cleanup.reveal", "cleanup.console", "cleanup.pwsh", "cleanup.properties", "-",
      "cleanup.deletePermanent", "cleanup.delete", "cleanup.emptyBin", "cleanup.emptyFolder", "cleanup.move", "-",
      compressMenu, "cleanup.vhd", "-",
      "cleanup.hibernate", "cleanup.diskCleanup", "cleanup.programs", "cleanup.roaming", "cleanup.local", "cleanup.shadow", dismMenu, "-",
      userMenu,
    ],
  },
  {
    label: "Treemap",
    items: () => ["view.treemap", "treemap.logical", "-", "treemap.zoomIn", "treemap.zoomOut", "treemap.reselectChild", "treemap.selectParent"],
  },
  {
    label: "Options",
    items: () => [
      "view.freeSpace", "view.unknown", "-", "view.fileTypes", "view.treemap", "view.toolbar", "view.statusBar", "-",
      { label: "Show list", items: () => ["view.allFiles", "view.largest", "view.duplicates", "view.searchResults"] },
      "-", "view.settings",
    ],
  },
  { label: "Help", items: () => ["help.about"] },
];

/** Context menu for the folder tree and file lists (WinDirStat IDR_POPUP_TREE). */
export const treeMenu = (): MenuEntry[] => [
  "tree.toggle", "-", "cleanup.open", "-", "edit.copy", "cleanup.reveal", "cleanup.console", "cleanup.pwsh", "file.refreshSelected", "cleanup.properties", "edit.hash", "-",
  "cleanup.deletePermanent", "cleanup.delete", "cleanup.emptyFolder", "cleanup.move", "-",
  compressMenu, "cleanup.vhd", "-", userMenu, "-", "cleanup.explorer",
];

/** Context menu for the treemap (WinDirStat IDR_POPUP_MAP). */
export const treemapMenu = (): MenuEntry[] => [
  "treemap.zoomIn", "treemap.zoomOut", "treemap.reselectChild", "treemap.selectParent", "-",
  "edit.copy", "cleanup.reveal", "cleanup.console", "cleanup.pwsh", "-", "cleanup.delete", "cleanup.explorer",
];

export const toolbar: (string | "-")[] = [
  "file.select", "-", "scan.resume", "scan.pause", "scan.stop", "-", "file.refreshAll", "file.refreshSelected", "-",
  "edit.search", "view.settings", "-", "cleanup.open", "cleanup.reveal", "cleanup.console", "edit.copy", "cleanup.properties", "-",
  "cleanup.delete", "cleanup.deletePermanent", "-", "treemap.zoomIn", "treemap.zoomOut", "-", "help.about",
];

/** Normalized shortcut text for a key event, e.g. "Ctrl+Alt+S", "Shift+Delete". */
export function shortcutOf(e: KeyboardEvent): string {
  const keyNames: Record<string, string> = { " ": "Space", Del: "Delete", Esc: "Escape" };
  let key = keyNames[e.key] ?? e.key;
  if (e.code === "NumpadAdd" || (key === "+" && !e.ctrlKey) || (key === "=" && !e.ctrlKey && !e.shiftKey)) key = "+";
  else if (e.code === "NumpadSubtract" || key === "-") key = "-";
  else if (e.code === "NumpadMultiply") key = "*";
  else if (e.code === "NumpadDivide") key = "/";
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (key.length === 1) key = key.toUpperCase();
  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey && key.length > 1) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

/** Ctrl++ / Ctrl+- also reselect child / select parent, as in WinDirStat. */
const aliases: Record<string, string> = { "Ctrl++": "treemap.reselectChild", "Ctrl+=": "treemap.reselectChild", "Ctrl+-": "treemap.selectParent", "Ctrl+Insert": "edit.copy" };

export function commandForShortcut(shortcut: string): Command | undefined {
  if (aliases[shortcut]) return resolve(aliases[shortcut]);
  const m = /^Ctrl\+(\d)$/.exec(shortcut);
  if (m) return userCleanupCommand(Number(m[1]));
  for (const c of commands.values()) if (c.shortcut === shortcut) return c;
  return undefined;
}
