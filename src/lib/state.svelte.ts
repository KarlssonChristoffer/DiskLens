import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  api,
  NodeFlags,
  type AppInfo,
  type Compression,
  type ExtStat,
  type FileEntry,
  type Hashes,
  type NodeDetail,
  type NodeInfo,
  type Settings,
  type Summary,
  type SystemAction,
} from "./api";
import { formatBytes, formatCount } from "./format";

export type Phase = "start" | "scanning" | "results";
export type Tab = "folders" | "largest" | "duplicates" | "search";

export interface ScanProgress {
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
  elapsedMs: number;
  paused: boolean;
}

export interface ConfirmRequest {
  title: string;
  message: string;
  items: string[];
  confirmLabel: string;
  danger: boolean;
  /** Offer "Don't ask again"; its state comes back in `dontAsk`. */
  offerDontAsk?: boolean;
  resolve: (ok: boolean, dontAsk: boolean) => void;
}

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  title: string;
  message?: string;
}

export interface MenuRequest {
  x: number;
  y: number;
  kind: "tree" | "treemap";
}

export type Dialog =
  | { kind: "settings"; page?: string }
  | { kind: "hash"; name: string; hashes: Hashes | null; error?: string }
  | { kind: "search" }
  | { kind: "about" };

class AppState {
  phase = $state<Phase>("start");
  scanPaths = $state<string[]>([]);
  progress = $state<ScanProgress | null>(null);
  summary = $state<Summary | null>(null);
  settings = $state<Settings | null>(null);
  info = $state<AppInfo | null>(null);
  extStats = $state<ExtStat[]>([]);

  /** Selected node ids; `focus` is the one acted on when only one counts. */
  selection = $state<number[]>([]);
  focus = $state<number | null>(null);
  /** Details of the selection, for enabling commands. */
  selectionInfo = $state<NodeInfo[]>([]);
  /** Node the treemap is zoomed into. */
  zoomRoot = $state(0);
  /** Node under the treemap pointer. */
  hovered = $state<number | null>(null);
  /** Extension highlighted in the treemap (from the file-type list). */
  highlightExt = $state<number | null>(null);
  /** Bumped whenever the scan data changes. */
  version = $state(0);
  tab = $state<Tab>("folders");
  settingsOpen = $state(false);

  searchResults = $state<FileEntry[] | null>(null);
  searchQuery = $state("");
  /** Asks the folder tree to expand or collapse a node (context menu). */
  toggleRequest = $state<{ id: number } | null>(null);

  confirm = $state<ConfirmRequest | null>(null);
  menu = $state<MenuRequest | null>(null);
  dialog = $state<Dialog | null>(null);
  busy = $state<string | null>(null);
  toasts = $state<Toast[]>([]);

  #details = new Map<number, Promise<NodeDetail>>();
  #toastId = 0;
  /** Children left behind by "Select parent", for "Reselect child". */
  #childStack: number[] = [];

  get selected() {
    return this.focus;
  }

  detail(id: number): Promise<NodeDetail> {
    let d = this.#details.get(id);
    if (!d) {
      d = api.nodeDetail(id);
      this.#details.set(id, d);
      d.catch(() => this.#details.delete(id));
    }
    return d;
  }

  async init() {
    const [info, settings] = await Promise.all([api.appInfo(), api.getSettings()]);
    this.info = info;
    this.settings = settings;
    if (info.accent) document.documentElement.style.setProperty("--accent-base", info.accent);
  }

  async updateSettings(patch: Partial<Settings>) {
    if (!this.settings) return;
    const next = { ...this.settings, ...patch };
    this.settings = next;
    await api.setSettings($state.snapshot(next) as Settings);
    if (this.summary) await this.refreshViews();
  }

  // ------------------------------------------------------------- scanning

  async scan(paths: string[]) {
    if (paths.length === 0) return;
    this.scanPaths = paths;
    this.progress = null;
    this.phase = "scanning";
    try {
      await api.startScan(paths, async (e) => {
        switch (e.type) {
          case "progress":
            this.progress = e;
            break;
          case "done":
            await this.loaded(e.summary);
            if (e.summary.incomplete) this.notify("info", "Scan stopped early", "Folders marked as incomplete weren't fully scanned.");
            if (e.summary.errors > 0) {
              this.notify("info", `${formatCount(e.summary.errors)} folders could not be read`,
                this.info?.elevated ? "Their contents aren't counted." : "Usually system folders that need administrator rights (File › Run as administrator).");
            }
            break;
          case "cancelled":
            this.phase = this.summary ? "results" : "start";
            break;
          case "failed":
            this.phase = this.summary ? "results" : "start";
            this.notify("error", "Scan failed", e.message);
            break;
        }
      });
      this.settings = await api.getSettings();
    } catch (err) {
      this.phase = this.summary ? "results" : "start";
      this.notify("error", "Could not start the scan", String(err));
    }
  }

  async loaded(summary: Summary) {
    this.#details.clear();
    this.summary = summary;
    this.setSelection([]);
    this.hovered = null;
    this.zoomRoot = 0;
    this.highlightExt = null;
    this.searchResults = null;
    this.#childStack = [];
    this.extStats = await api.extStats();
    this.version++;
    this.phase = "results";
  }

  pauseScan = () => api.pauseScan();
  resumeScan = () => api.resumeScan();
  stopScan = () => api.stopScan();
  cancelScan = () => api.cancelScan();

  rescan() {
    if (!this.summary || this.summary.fromFile) return;
    const paths = this.scanPaths.length ? this.scanPaths : [this.summary.rootPath];
    this.scan(paths);
  }

  goHome() {
    this.phase = "start";
  }

  /** Scan data changed underneath the views: refresh everything. */
  async refreshViews(summary?: Summary) {
    this.#details.clear();
    this.summary = summary ?? (await api.getSummary().catch(() => this.summary));
    this.extStats = await api.extStats().catch(() => this.extStats);
    // Drop selection and zoom that point at removed items.
    const alive = await Promise.all(this.selection.map((id) => this.detail(id).then((d) => d.alive).catch(() => false)));
    const keep = this.selection.filter((_, i) => alive[i]);
    if (keep.length !== this.selection.length) this.setSelection(keep);
    if (this.zoomRoot !== 0 && !(await this.detail(this.zoomRoot).then((d) => d.alive).catch(() => false))) this.zoomRoot = 0;
    if (this.searchResults && this.searchQuery && this.settings) {
      this.searchResults = await api.search(this.searchQuery, this.zoomRoot, $state.snapshot(this.settings.search)).catch(() => this.searchResults);
    }
    this.version++;
  }

  // ------------------------------------------------------------- selection

  setSelection(ids: number[], focus: number | null = ids[ids.length - 1] ?? null, infos?: NodeInfo[]) {
    this.selection = ids;
    this.focus = focus;
    if (infos) this.selectionInfo = infos;
    else {
      this.selectionInfo = [];
      Promise.all(ids.map((id) => this.detail(id).then((d) => d.info))).then((list) => {
        if (this.selection === ids) this.selectionInfo = list;
      }).catch(() => {});
    }
  }

  select(id: number | null, info?: NodeInfo) {
    this.setSelection(id === null ? [] : [id], id, info ? [info] : undefined);
  }

  isSelected(id: number) {
    return this.selection.includes(id);
  }

  /** Ctrl-click: add or remove one item. */
  toggleSelect(id: number) {
    if (this.isSelected(id)) {
      const ids = this.selection.filter((s) => s !== id);
      this.setSelection(ids, ids[ids.length - 1] ?? null);
    } else {
      this.setSelection([...this.selection, id], id);
    }
  }

  /** Files and folders (not pseudo items or the scan root) in the selection. */
  get actionable(): number[] {
    return this.selectionInfo
      .filter((n) => n.kind === "dir" || n.kind === "file")
      .filter((n) => !(n.flags & NodeFlags.myComputer))
      .map((n) => n.id);
  }

  // ------------------------------------------------------------- treemap navigation

  async zoomTo(id: number) {
    const d = await this.detail(id);
    this.zoomRoot = d.info.isDir ? id : (d.ancestry[d.ancestry.length - 2] ?? 0);
  }

  async zoomIn() {
    if (this.focus !== null) await this.zoomTo(this.focus);
  }

  async zoomOut() {
    if (this.zoomRoot === 0) return;
    const d = await this.detail(this.zoomRoot);
    this.zoomRoot = d.ancestry[d.ancestry.length - 2] ?? 0;
  }

  async selectParent() {
    if (this.focus === null || this.focus === 0) return;
    const d = await this.detail(this.focus);
    this.#childStack.push(this.focus);
    this.select(d.ancestry[d.ancestry.length - 2] ?? 0);
  }

  reselectChild() {
    const child = this.#childStack.pop();
    if (child !== undefined) this.select(child);
  }

  get canReselectChild() {
    return this.#childStack.length > 0;
  }

  // ------------------------------------------------------------- dialogs & toasts

  ask(req: Omit<ConfirmRequest, "resolve">): Promise<{ ok: boolean; dontAsk: boolean }> {
    return new Promise((resolve) => {
      this.confirm = { ...req, resolve: (ok, dontAsk) => resolve({ ok, dontAsk }) };
    });
  }

  notify(kind: Toast["kind"], title: string, message?: string) {
    const id = ++this.#toastId;
    this.toasts = [...this.toasts, { id, kind, title, message }];
    setTimeout(() => this.dismiss(id), kind === "error" ? 10000 : 5000);
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  async withBusy<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
    this.busy = label;
    try {
      return await fn();
    } catch (err) {
      const msg = String(err);
      if (msg !== "Cancelled") this.notify("error", "That didn't work", msg);
      return undefined;
    } finally {
      this.busy = null;
    }
  }

  // ------------------------------------------------------------- item actions

  async paths(ids: number[]) {
    return Promise.all(ids.map((id) => this.detail(id).then((d) => d.path)));
  }

  async copyPath() {
    if (this.selection.length === 0) return;
    const text = (await this.paths(this.selection)).join("\r\n");
    await writeText(text);
    this.notify("info", this.selection.length === 1 ? "Path copied" : `${this.selection.length} paths copied`, this.selection.length === 1 ? text : undefined);
  }

  async openSelected() {
    for (const id of this.actionable.slice(0, 10)) await api.openItem(id).catch((e) => this.notify("error", "Couldn't open", String(e)));
  }

  async revealSelected() {
    if (this.focus !== null) await api.revealItem(this.focus).catch((e) => this.notify("error", "Couldn't show in Explorer", String(e)));
  }

  async propertiesSelected() {
    if (this.focus !== null) await api.showProperties(this.focus).catch((e) => this.notify("error", "Couldn't open Properties", String(e)));
  }

  async consoleSelected(powershell: boolean) {
    if (this.focus !== null) await api.openConsole(this.focus, powershell).catch((e) => this.notify("error", "Couldn't open a console", String(e)));
  }

  async refreshSelected() {
    const ids = this.selection.length ? this.selection : [this.zoomRoot];
    const summary = await this.withBusy("Refreshing…", () => api.refreshItems(ids));
    if (summary) await this.refreshViews(summary);
  }

  async computeHash() {
    const id = this.actionable.find((i) => this.selectionInfo.find((n) => n.id === i)?.kind === "file");
    if (id === undefined) return;
    const d = await this.detail(id);
    this.dialog = { kind: "hash", name: d.info.name, hashes: null };
    try {
      const hashes = await api.computeHashes(id);
      if (this.dialog?.kind === "hash") this.dialog = { kind: "hash", name: d.info.name, hashes };
    } catch (err) {
      if (this.dialog?.kind === "hash") this.dialog = { kind: "hash", name: d.info.name, hashes: null, error: String(err) };
    }
  }

  async explorerMenu(x: number, y: number) {
    const ids = this.actionable;
    if (ids.length === 0) return;
    try {
      if (await api.explorerMenu(ids, x, y)) {
        // Explorer may have deleted or renamed the items; drop what's gone.
        setTimeout(async () => this.refreshViews(await api.pruneMissing(ids)), 1200);
      }
    } catch (err) {
      this.notify("error", "Couldn't show Explorer's menu", String(err));
    }
  }

  async describe(ids: number[]) {
    const details = await Promise.all(ids.map((id) => this.detail(id)));
    const total = details.reduce((sum, d) => sum + d.info.physical, 0);
    return { details, total, name: details.length === 1 ? `"${details[0].info.name}"` : `${details.length} items` };
  }

  /** Delete to the Recycle Bin (Del) or permanently (Shift+Del). */
  async deleteSelected(permanent: boolean) {
    const ids = this.actionable.filter((id) => id !== 0);
    if (ids.length === 0) return;
    const { details, total, name } = await this.describe(ids);
    if (permanent || this.settings?.deleteWarning !== false) {
      const answer = await this.ask({
        title: permanent ? `Permanently delete ${name}?` : `Move ${name} to the Recycle Bin?`,
        message: permanent
          ? `This deletes ${formatBytes(total)} right away. It can't be undone and won't go to the Recycle Bin.`
          : `This frees about ${formatBytes(total)} once the Recycle Bin is emptied. If something is too big for the Recycle Bin, Windows will ask before deleting it permanently.`,
        items: details.map((d) => d.path),
        confirmLabel: permanent ? "Delete permanently" : "Move to Recycle Bin",
        danger: true,
        offerDontAsk: !permanent,
      });
      if (!answer.ok) return;
      if (answer.dontAsk) await this.updateSettings({ deleteWarning: false });
    }
    const outcome = await this.withBusy(permanent ? "Deleting…" : "Moving to the Recycle Bin…", () => api.deleteItems(ids, permanent));
    if (!outcome) return;
    if (outcome.removed.length > 0) {
      this.notify("success",
        permanent ? `Deleted ${outcome.removed.length} ${outcome.removed.length === 1 ? "item" : "items"}` : `Moved ${outcome.removed.length} ${outcome.removed.length === 1 ? "item" : "items"} to the Recycle Bin`,
        permanent ? `${formatBytes(outcome.freed)} freed.` : `${formatBytes(outcome.freed)} will be freed when the Recycle Bin is emptied.`);
    }
    if (outcome.remaining.length > 0) {
      this.notify("info", `${outcome.remaining.length} ${outcome.remaining.length === 1 ? "item was" : "items were"} not removed`,
        "Skipped, in use or cancelled. What's left has been rescanned.");
    }
    await this.refreshViews(outcome.summary);
  }

  async emptyFolder() {
    const ids = this.selectionInfo.filter((n) => n.kind === "dir" && n.id !== 0).map((n) => n.id);
    if (ids.length === 0) return;
    const { details, total, name } = await this.describe(ids);
    const answer = await this.ask({
      title: `Empty ${name}?`,
      message: `Everything inside is deleted permanently (${formatBytes(total)}); the folder itself stays. This can't be undone.`,
      items: details.map((d) => d.path),
      confirmLabel: "Delete contents",
      danger: true,
    });
    if (!answer.ok) return;
    const outcome = await this.withBusy("Emptying…", () => api.emptyFolders(ids));
    if (outcome) {
      this.notify("success", "Folder emptied", `${formatBytes(outcome.freed)} freed.`);
      await this.refreshViews(outcome.summary);
    }
  }

  async moveTo() {
    const ids = this.actionable.filter((id) => id !== 0);
    if (ids.length === 0) return;
    const dest = await openDialog({ directory: true, title: "Move to folder" });
    if (typeof dest !== "string") return;
    const outcome = await this.withBusy("Moving…", () => api.moveItems(ids, dest));
    if (outcome) {
      this.notify("success", `Moved ${outcome.removed.length} ${outcome.removed.length === 1 ? "item" : "items"}`, dest);
      await this.refreshViews(outcome.summary);
    }
  }

  async compress(compression: Compression) {
    const ids = this.actionable;
    if (ids.length === 0) return;
    const label = compression === "none" ? "Uncompressing…" : "Compressing…";
    const outcome = await this.withBusy(label, () => api.compressItems(ids, compression));
    if (!outcome) return;
    this.notify(outcome.failed.length ? "info" : "success",
      `${compression === "none" ? "Uncompressed" : "Compressed"} ${formatCount(outcome.done)} files`,
      outcome.failed.length ? `${outcome.failed.length} skipped (in use or not supported): ${outcome.failed[0]}` : undefined);
    await this.refreshViews(outcome.summary);
  }

  async optimizeVhd() {
    const ids = this.actionable;
    if (ids.length === 0) return;
    const outcome = await this.withBusy("Optimizing virtual disks…", () => api.optimizeVhds(ids));
    if (!outcome) return;
    this.notify(outcome.failed.length ? "info" : "success", `Optimized ${outcome.done} virtual disks`,
      outcome.failed.length ? `Failed: ${outcome.failed[0]}${this.info?.elevated ? "" : " (try running as administrator; the disk must not be in use)"}` : undefined);
    await this.refreshViews(outcome.summary);
  }

  async userCleanup(index: number) {
    const cleanup = this.settings?.userCleanups[index];
    const ids = this.actionable;
    if (!cleanup || ids.length === 0) return;
    try {
      const preview = await api.previewUserCleanup(index, ids);
      if (cleanup.askFirst) {
        const answer = await this.ask({
          title: `Run "${cleanup.title}"?`,
          message: `This runs ${preview.commands.length === 1 ? "this command" : `these ${preview.commands.length} commands`} through cmd.exe.`,
          items: preview.commands,
          confirmLabel: "Run",
          danger: false,
        });
        if (!answer.ok) return;
      }
      const summary = await this.withBusy(`Running ${cleanup.title}…`, () => api.runUserCleanup(index, ids));
      if (summary) await this.refreshViews(summary);
    } catch (err) {
      this.notify("error", `Couldn't run "${cleanup.title}"`, String(err));
    }
  }

  async system(action: SystemAction) {
    const confirmations: Partial<Record<SystemAction, [string, string, string]>> = {
      emptyRecycleBin: ["Empty the Recycle Bin?", "Everything in the Recycle Bin on every drive is deleted permanently.", "Empty Recycle Bin"],
      disableHibernation: ["Turn off hibernation?", "Windows deletes hiberfil.sys (often several GB). Hibernate and Fast Startup stop working until you turn hibernation back on with 'powercfg /h on'.", "Turn off hibernation"],
      removeShadowCopies: ["Remove all shadow copies?", "Deletes every volume shadow copy (System Restore points and previous versions of files). An administrator console opens and asks once more.", "Continue"],
      removeRoamingProfiles: ["Remove roaming user profiles?", "Deletes the local copies of roaming profiles. An administrator PowerShell window asks for each profile.", "Continue"],
      removeLocalProfiles: ["Remove unused local user profiles?", "Deletes profiles of users who aren't signed in. Their files on this PC are lost. An administrator PowerShell window asks for each profile.", "Continue"],
      dismReset: ["Reset the component store base?", "DISM /ResetBase frees the most space, but installed Windows updates can no longer be uninstalled.", "Continue"],
    };
    const c = confirmations[action];
    if (c) {
      const answer = await this.ask({ title: c[0], message: c[1], items: [], confirmLabel: c[2], danger: true });
      if (!answer.ok) return;
    }
    const busyLabel = action === "emptyRecycleBin" ? "Emptying the Recycle Bin…" : action === "disableHibernation" ? "Turning off hibernation…" : "Starting…";
    const result = await this.withBusy(busyLabel, () => api.systemAction(action));
    if (result === undefined) return;
    if (action === "emptyRecycleBin") this.notify("success", "Recycle Bin emptied");
    if (action === "disableHibernation") this.notify("success", "Hibernation turned off");
    if (result) await this.refreshViews(result);
    if (action === "disableHibernation") this.info = await api.appInfo();
  }

  // ------------------------------------------------------------- results files

  async saveResults() {
    const path = await saveDialog({ title: "Save results", defaultPath: "DiskLens results.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    if ((await this.withBusy("Saving…", () => api.saveResults(path))) !== undefined) this.notify("success", "Results saved", path);
  }

  async saveDuplicates() {
    const path = await saveDialog({ title: "Save duplicates", defaultPath: "DiskLens duplicates.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    if ((await this.withBusy("Saving…", () => api.saveDuplicates(path))) !== undefined) this.notify("success", "Duplicates saved", path);
  }

  async loadResults() {
    const path = await openDialog({ title: "Load results", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (typeof path !== "string") return;
    const summary = await this.withBusy("Loading…", () => api.loadResults(path));
    if (summary) {
      this.scanPaths = [];
      await this.loaded(summary);
      this.notify("info", "Results loaded", "File actions are off for loaded results; rescan to work with the files.");
    }
  }

  // ------------------------------------------------------------- search

  async runSearch(query: string) {
    if (!this.settings || !query) return;
    this.searchQuery = query;
    const results = await this.withBusy("Searching…", () => api.search(query, this.zoomRoot, $state.snapshot(this.settings!.search)));
    if (results) {
      this.searchResults = results;
      this.tab = "search";
      this.dialog = null;
      if (results.length >= this.settings.search.maxResults) this.notify("info", `Showing the first ${formatCount(results.length)} matches`);
    }
  }
}

export const app = new AppState();
