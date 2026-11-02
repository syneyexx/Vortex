// Bronweergave met coördinaten (W16.04): "p. 3 [x0, y0, x1, y1]" → pagina gerenderd met het
// geciteerde gebied gemarkeerd; afbeeldingen uit uploads/media direct.
import { useEffect, useState } from "react";
import { API_BASE, api } from "../api/client.ts";

const RE = /^p\.\s*(\d+)(?:\s*\[(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\])?/;

export function parseLocator(loc: string | null | undefined) {
  const m = loc ? RE.exec(loc) : null;
  if (!m) return null;
  const box = m[2] ? [m[2], m[3], m[4], m[5]].map(Number) : null;
  return { page: Number(m[1]), box: box as [number, number, number, number] | null };
}

export function RegionPreview({ path, locator }: { path: string | null; locator: string | null }) {
  const loc = parseLocator(locator);
  const [page, setPage] = useState<{ url: string; zoom: number; width: number; height: number } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const isPdf = !!path && path.toLowerCase().endsWith(".pdf");
  const pageNo = loc?.page;
  useEffect(() => {
    if (!path || !pageNo || !isPdf) return;
    api
      .renderPage(path, pageNo)
      .then((r) => setPage({ url: `${API_BASE}${r.url}`, zoom: r.zoom, width: r.width, height: r.height }))
      .catch((e: unknown) => setErr(String(e)));
  }, [path, pageNo, isPdf]);
  if (!path || !loc || !isPdf) return null;
  if (err) return <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>Bron niet te tonen: {err}</p>;
  if (!page) return <p style={{ fontSize: 12, color: "var(--vx-muted)" }}>Pagina laden…</p>;
  const z = page.zoom;
  const b = loc.box ? ([loc.box[0] * z, loc.box[1] * z, loc.box[2] * z, loc.box[3] * z] as const) : null;
  return (
    <div style={{ position: "relative", maxWidth: 520, marginTop: 10 }}>
      <img src={page.url} alt={`pagina ${loc.page}`} style={{ width: "100%", display: "block" }} />
      {b && (
        <svg
          viewBox={`0 0 ${page.width} ${page.height}`}
          preserveAspectRatio="none"
          style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }}
        >
          <title>{locator}</title>
          <rect
            x={b[0]}
            y={b[1]}
            width={b[2] - b[0]}
            height={b[3] - b[1]}
            fill="rgba(245,197,66,0.18)"
            stroke="#f5c542"
            strokeWidth={4}
          />
        </svg>
      )}
    </div>
  );
}
