export function ago(seconds: number): string {
  if (seconds < 90) return `${seconds}s ago`;
  if (seconds < 5400) return `${Math.round(seconds / 60)}m ago`;
  return `${Math.round(seconds / 3600)}h ago`;
}

export const pct = (r: number | null): string =>
  r == null ? "–" : r === 0 ? "0%" : r < 0.001 ? "<0.1%" : `${(r * 100).toFixed(r < 0.1 ? 1 : 0)}%`;

export const rate = (r: number | null): string =>
  r == null ? "–" : r < 10 ? `${r.toFixed(2)}/s` : `${Math.round(r)}/s`;

export const ms = (v: number | null): string =>
  v == null ? "–" : v < 1000 ? `${Math.round(v)} ms` : `${(v / 1000).toFixed(1)} s`;
