const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** Bytes in binary units with three significant digits, like Explorer. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

const counts = new Intl.NumberFormat();
export const formatCount = (n: number) => counts.format(n);

export function formatPercent(share: number): string {
  const p = share * 100;
  if (p > 0 && p < 0.1) return "<0.1%";
  return `${p.toFixed(p >= 10 ? 0 : 1)}%`;
}

const dates = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
export const formatDate = (ms: number) => (ms > 0 ? dates.format(new Date(ms)) : "");

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  const m = Math.floor(s / 60);
  return `${m} min ${Math.round(s - m * 60)} s`;
}

export const rgb = (c: [number, number, number] | null | undefined) =>
  c ? `rgb(${c[0]} ${c[1]} ${c[2]})` : "transparent";

/** A small cushion-like gradient for colour swatches. */
export const swatch = (c: [number, number, number] | null | undefined) =>
  c
    ? `radial-gradient(circle at 35% 30%, rgb(${c.map((v) => Math.min(255, v * 1.45)).join(" ")}), rgb(${c.map((v) => v * 0.75).join(" ")}))`
    : "transparent";

export function extLabel(ext: string): string {
  return ext === "" ? "No extension" : ext;
}
