import type { NodeKind } from "./api";

export const KIND_LABEL: Record<NodeKind, string> = {
  service: "Service",
  gateway: "Gateway",
  database: "Database",
  cache: "Cache",
  queue: "Queue",
  external: "External",
  config: "Config / flags",
};

/** One shape per kind, drawn in a 56x56 box. Fill comes from the `--fill` CSS variable. */
export function Glyph({ kind, size = 56 }: { kind: NodeKind; size?: number }) {
  const common = { fill: "var(--fill)", stroke: "var(--stroke)", strokeWidth: 2, strokeLinejoin: "round" as const };
  let shape;
  switch (kind) {
    case "database":
      shape = (
        <>
          <path d="M8 13 v30 a20 7 0 0 0 40 0 v-30" {...common} />
          <ellipse cx="28" cy="13" rx="20" ry="7" {...common} />
          <path d="M8 26 a20 7 0 0 0 40 0" fill="none" stroke="var(--stroke)" strokeWidth="1.2" opacity="0.6" />
        </>
      );
      break;
    case "cache":
      shape = (
        <>
          <path d="M8 16 v24 a20 7 0 0 0 40 0 v-24" {...common} />
          <ellipse cx="28" cy="16" rx="20" ry="7" {...common} />
          <path d="M30 20 l-7 11 h5 l-2 9 8-12 h-5 z" fill="var(--stroke)" opacity="0.85" />
        </>
      );
      break;
    case "queue":
      shape = (
        <>
          <rect x="4" y="16" width="48" height="24" rx="12" {...common} />
          <path d="M18 20 v16 M28 20 v16 M38 20 v16" stroke="var(--stroke)" strokeWidth="1.5" opacity="0.7" />
        </>
      );
      break;
    case "gateway":
      shape = <path d="M16 6 h24 l12 22 -12 22 h-24 l-12 -22 z" {...common} />;
      break;
    case "external":
      shape = (
        <path
          d="M16 42 a9 9 0 0 1 -1 -18 a12 12 0 0 1 23 -3 a10 10 0 0 1 4 21 z"
          {...common}
          strokeDasharray="4 3"
        />
      );
      break;
    case "config":
      shape = (
        <>
          <path d="M28 4 l22 24 -22 24 -22 -24 z" {...common} />
          <path d="M22 20 v16 M22 21 h13 l-3 5 3 5 h-13" fill="none" stroke="var(--stroke)" strokeWidth="1.8" />
        </>
      );
      break;
    default:
      shape = (
        <>
          <circle cx="28" cy="28" r="22" {...common} />
          <circle cx="28" cy="28" r="12" fill="none" stroke="var(--stroke)" strokeWidth="1.2" opacity="0.5" />
        </>
      );
  }
  return (
    <svg width={size} height={size} viewBox="0 0 56 56" aria-hidden="true" className="glyph">
      {shape}
    </svg>
  );
}
